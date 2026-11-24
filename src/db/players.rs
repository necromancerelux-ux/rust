//! `players` table: the tracked player IDs plus Oracle enrichment.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::{now, Db};

/// A row of `players`, with every enrichment column the Oracle sync fills in.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PlayerRecord {
    pub fid: String,
    pub user_id: Option<String>,
    pub home_guild_id: String,
    pub state: Option<i64>,
    pub is_standalone: i64,
    pub added_by: String,
    pub added_at: i64,
    pub updated_at: i64,

    pub game_name: Option<String>,
    pub alliance_abbr: Option<String>,
    pub alliance_id: Option<String>,
    pub alliance_name: Option<String>,
    pub alliance_rank: Option<i64>,
    pub power: Option<i64>,
    pub kills: Option<i64>,
    pub town_hall_level: Option<i64>,
    pub life_tree_level: Option<i64>,
    pub avatar_url: Option<String>,
    pub oracle_uid: Option<String>,
    pub last_oracle_sync: Option<i64>,
}

impl PlayerRecord {
    /// Best display label: game name, then fid.
    pub fn label(&self) -> &str {
        self.game_name.as_deref().unwrap_or(&self.fid)
    }

    pub fn in_alliance(&self, abbr: &str) -> bool {
        self.alliance_abbr
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(abbr))
    }
}

/// A lightweight search result set for the lookup panels.
#[derive(Debug, Clone, Default)]
pub struct PlayerSearch {
    pub total: usize,
    pub rows: Vec<PlayerRecord>,
}

const SELECT: &str = "SELECT * FROM players";

/// Look up a player by FID (global uniqueness).
pub async fn get_by_fid(db: &Db, fid: &str) -> sqlx::Result<Option<PlayerRecord>> {
    get(db, fid).await
}

/// Number of IDs linked to a Discord user, globally (JS counts across guilds).
pub async fn count_by_user(db: &Db, user_id: u64) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM players WHERE user_id = ?")
        .bind(user_id.to_string())
        .fetch_one(&db.pool)
        .await?;
    Ok(row.0)
}

/// Relink an unowned (standalone) or claimed FID to a Discord user.
pub async fn relink(
    db: &Db,
    fid: &str,
    user_id: u64,
    guild_id: u64,
    state: Option<i64>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE players SET user_id = ?, home_guild_id = ?, is_standalone = 0,
         state = COALESCE(?, state), updated_at = ? WHERE fid = ?",
    )
    .bind(user_id.to_string())
    .bind(guild_id.to_string())
    .bind(state)
    .bind(now())
    .bind(fid)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn update_state(db: &Db, fid: &str, state: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE players SET state = ?, updated_at = ? WHERE fid = ?")
        .bind(state)
        .bind(now())
        .bind(fid)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Players already linked to a Discord user in this guild. Used by user
/// redemption; this does not create or modify any player registration.
pub async fn for_user(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<Vec<PlayerRecord>> {
    sqlx::query_as::<_, PlayerRecord>(
        "SELECT * FROM players WHERE home_guild_id = ? AND user_id = ? ORDER BY added_at DESC",
    )
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .fetch_all(&db.pool)
    .await
}

/// Players registered to a guild, newest first.
pub async fn list_for_guild(db: &Db, guild_id: u64) -> sqlx::Result<Vec<PlayerRecord>> {
    sqlx::query_as::<_, PlayerRecord>(
        "SELECT * FROM players WHERE home_guild_id = ? ORDER BY added_at DESC",
    )
    .bind(guild_id.to_string())
    .fetch_all(&db.pool)
    .await
}

/// Guilds with at least one registered player, used for background gift-code fan-out.
pub async fn guilds_with_players(db: &Db) -> sqlx::Result<Vec<u64>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT home_guild_id FROM players ORDER BY home_guild_id")
            .fetch_all(&db.pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id,)| id.parse::<u64>().ok())
        .collect())
}

/// How many IDs a guild has registered.
pub async fn count_for_guild(db: &Db, guild_id: u64) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM players WHERE home_guild_id = ?")
        .bind(guild_id.to_string())
        .fetch_one(&db.pool)
        .await
}

pub async fn get(db: &Db, fid: &str) -> sqlx::Result<Option<PlayerRecord>> {
    sqlx::query_as::<_, PlayerRecord>("SELECT * FROM players WHERE fid = ?")
        .bind(fid)
        .fetch_optional(&db.pool)
        .await
}

/// Register (or refresh ownership of) a player ID.
/// Insert a brand-new player row; returns false when the FID already exists
/// (never overwrites another user's ownership).
pub async fn insert_new(
    db: &Db,
    fid: &str,
    guild_id: u64,
    user_id: Option<u64>,
    added_by: u64,
    state: i64,
    standalone: bool,
) -> sqlx::Result<bool> {
    let ts = now();
    let result = sqlx::query(
        "INSERT INTO players (fid, user_id, home_guild_id, state, is_standalone, added_by, added_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(fid) DO NOTHING",
    )
    .bind(fid)
    .bind(user_id.map(|id| id.to_string()))
    .bind(guild_id.to_string())
    .bind(state)
    .bind(standalone as i64)
    .bind(added_by.to_string())
    .bind(ts)
    .bind(ts)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn upsert(
    db: &Db,
    guild_id: u64,
    fid: &str,
    user_id: Option<u64>,
    added_by: u64,
    state: Option<i64>,
    standalone: bool,
) -> sqlx::Result<()> {
    let ts = now();
    sqlx::query(
        "INSERT INTO players (fid, user_id, home_guild_id, state, is_standalone, added_by, added_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(fid) DO UPDATE SET
             user_id = COALESCE(excluded.user_id, players.user_id),
             state = COALESCE(excluded.state, players.state),
             updated_at = excluded.updated_at",
    )
    .bind(fid)
    .bind(user_id.map(|id| id.to_string()))
    .bind(guild_id.to_string())
    .bind(state)
    .bind(standalone as i64)
    .bind(added_by.to_string())
    .bind(ts)
    .bind(ts)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn remove(db: &Db, fid: &str) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM players WHERE fid = ?")
        .bind(fid)
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn remove_alliance(db: &Db, guild_id: u64, abbr: &str) -> sqlx::Result<u64> {
    let result = sqlx::query(
        "DELETE FROM players WHERE home_guild_id = ? AND UPPER(alliance_abbr) = UPPER(?)",
    )
    .bind(guild_id.to_string())
    .bind(abbr)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected())
}

/// First `limit` rows of a guild, used for the "all players" view.
pub async fn page_for_guild(
    db: &Db,
    guild_id: u64,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<PlayerRecord>> {
    sqlx::query_as::<_, PlayerRecord>(
        "SELECT * FROM players WHERE home_guild_id = ? ORDER BY COALESCE(power, 0) DESC, added_at DESC LIMIT ? OFFSET ?",
    )
    .bind(guild_id.to_string())
    .bind(limit)
    .bind(offset)
    .fetch_all(&db.pool)
    .await
}

/// Search by FID or in-game name, scoped to a guild.
pub async fn search(
    db: &Db,
    guild_id: u64,
    needle: &str,
    limit: i64,
) -> sqlx::Result<Vec<PlayerRecord>> {
    let pattern = format!("%{needle}%");
    sqlx::query_as::<_, PlayerRecord>(
        "SELECT * FROM players
         WHERE home_guild_id = ?
           AND (fid LIKE ? OR game_name LIKE ? OR alliance_abbr LIKE ? OR alliance_name LIKE ?)
         ORDER BY COALESCE(power, 0) DESC, added_at DESC
         LIMIT ?",
    )
    .bind(guild_id.to_string())
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(limit)
    .fetch_all(&db.pool)
    .await
}

pub async fn by_alliance(
    db: &Db,
    guild_id: u64,
    abbr: &str,
    limit: i64,
) -> sqlx::Result<Vec<PlayerRecord>> {
    sqlx::query_as::<_, PlayerRecord>(
        "SELECT * FROM players
         WHERE home_guild_id = ? AND UPPER(alliance_abbr) = UPPER(?)
         ORDER BY COALESCE(power, 0) DESC LIMIT ?",
    )
    .bind(guild_id.to_string())
    .bind(abbr)
    .bind(limit)
    .fetch_all(&db.pool)
    .await
}

/// Every distinct alliance abbreviation a guild tracks.
pub async fn alliances(db: &Db, guild_id: u64) -> sqlx::Result<Vec<(String, i64)>> {
    sqlx::query_as::<_, (String, i64)>(
        "SELECT alliance_abbr, COUNT(*) FROM players
         WHERE home_guild_id = ? AND alliance_abbr IS NOT NULL AND alliance_abbr <> ''
         GROUP BY alliance_abbr ORDER BY COUNT(*) DESC",
    )
    .bind(guild_id.to_string())
    .fetch_all(&db.pool)
    .await
}

/// The enrichment payload the Oracle sync writes back.
#[derive(Debug, Clone, Default)]
pub struct Enrichment {
    pub game_name: Option<String>,
    pub alliance_abbr: Option<String>,
    pub alliance_id: Option<String>,
    pub alliance_name: Option<String>,
    pub alliance_rank: Option<i64>,
    pub power: Option<i64>,
    pub kills: Option<i64>,
    pub town_hall_level: Option<i64>,
    pub life_tree_level: Option<i64>,
    pub avatar_url: Option<String>,
    pub oracle_uid: Option<String>,
    pub state: Option<i64>,
}

pub async fn apply_enrichment(db: &Db, fid: &str, data: &Enrichment) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE players SET
             game_name = COALESCE(?, game_name),
             alliance_abbr = COALESCE(?, alliance_abbr),
             alliance_id = COALESCE(?, alliance_id),
             alliance_name = COALESCE(?, alliance_name),
             alliance_rank = COALESCE(?, alliance_rank),
             power = COALESCE(?, power),
             kills = COALESCE(?, kills),
             town_hall_level = COALESCE(?, town_hall_level),
             life_tree_level = COALESCE(?, life_tree_level),
             avatar_url = COALESCE(?, avatar_url),
             oracle_uid = COALESCE(?, oracle_uid),
             state = COALESCE(?, state),
             last_oracle_sync = ?,
             updated_at = ?
         WHERE fid = ?",
    )
    .bind(&data.game_name)
    .bind(&data.alliance_abbr)
    .bind(&data.alliance_id)
    .bind(&data.alliance_name)
    .bind(data.alliance_rank)
    .bind(data.power)
    .bind(data.kills)
    .bind(data.town_hall_level)
    .bind(data.life_tree_level)
    .bind(&data.avatar_url)
    .bind(&data.oracle_uid)
    .bind(data.state)
    .bind(now())
    .bind(now())
    .bind(fid)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Aggregate numbers for the status panel.
#[derive(Debug, Clone, Default)]
pub struct PlayerStats {
    pub tracked: i64,
    pub enriched: i64,
    pub alliances: i64,
    pub total_power: i64,
}

pub async fn stats(db: &Db, guild_id: u64) -> sqlx::Result<PlayerStats> {
    let row: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*),
             COALESCE(SUM(CASE WHEN game_name IS NOT NULL THEN 1 ELSE 0 END), 0),
             COUNT(DISTINCT alliance_abbr),
             COALESCE(SUM(power), 0)
         FROM players WHERE home_guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_one(&db.pool)
    .await?;

    Ok(PlayerStats {
        tracked: row.0,
        enriched: row.1,
        alliances: row.2,
        total_power: row.3,
    })
}

/// Every tracked FID across all guilds, for background enrichment.
pub async fn all_fids(db: &Db, limit: i64) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT fid FROM players LIMIT ?")
        .bind(limit)
        .fetch_all(&db.pool)
        .await
}

/// Stale players that have not been enriched recently.
pub async fn stale_fids(db: &Db, older_than: i64, limit: i64) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT fid FROM players
         WHERE last_oracle_sync IS NULL OR last_oracle_sync < ?
         ORDER BY COALESCE(last_oracle_sync, 0) ASC LIMIT ?",
    )
    .bind(older_than)
    .bind(limit)
    .fetch_all(&db.pool)
    .await
}
