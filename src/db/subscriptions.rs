//! Per-server feature flags, player-ID caps, gift-code priority and the
//! server-staff tier (Server Bot Admin / Sub-Admin).
//!
//! The paid subscription product was removed in the legacy bot as well;
//! everything is unlocked by default. The Bot Owner can still force a
//! feature off per server, order servers for gift-code announcements,
//! set a player cap and grant trusted users server-staff rights.

use sqlx::FromRow;

use super::Db;

// ------------------------------------------------------------ feature flags

pub async fn get_feature_flag(
    db: &Db,
    guild_id: u64,
    feature_key: &str,
) -> sqlx::Result<Option<bool>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT enabled FROM server_feature_flags WHERE guild_id = ? AND feature_key = ?",
    )
    .bind(guild_id.to_string())
    .bind(feature_key)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|(enabled,)| enabled != 0))
}

pub async fn is_feature_enabled(db: &Db, guild_id: u64, feature_key: &str) -> bool {
    get_feature_flag(db, guild_id, feature_key)
        .await
        .ok()
        .flatten()
        .unwrap_or(true)
}

pub async fn set_feature_flag(
    db: &Db,
    guild_id: u64,
    feature_key: &str,
    enabled: bool,
    set_by: u64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO server_feature_flags (guild_id, feature_key, enabled, set_by)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(guild_id, feature_key) DO UPDATE SET enabled = excluded.enabled, set_by = excluded.set_by, set_at = datetime('now')",
    )
    .bind(guild_id.to_string())
    .bind(feature_key)
    .bind(enabled as i64)
    .bind(set_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn clear_feature_flag(db: &Db, guild_id: u64, feature_key: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM server_feature_flags WHERE guild_id = ? AND feature_key = ?")
        .bind(guild_id.to_string())
        .bind(feature_key)
        .execute(&db.pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, FromRow)]
pub struct FlagRow {
    pub feature_key: String,
    pub enabled: i64,
}

pub async fn list_feature_flags(db: &Db, guild_id: u64) -> sqlx::Result<Vec<FlagRow>> {
    sqlx::query_as::<_, FlagRow>(
        "SELECT feature_key, enabled FROM server_feature_flags WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_all(&db.pool)
    .await
}

// ----------------------------------------------------------- player id cap

pub async fn player_id_limit(db: &Db, guild_id: u64) -> sqlx::Result<Option<i64>> {
    let row: Option<Option<i64>> =
        sqlx::query_scalar("SELECT player_id_limit FROM server_subscriptions WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.flatten())
}

// ------------------------------------------------------- official server

/// The configured official OmniCore bot server (DB override, then env default).
pub async fn official_server_guild_id(db: &Db) -> Option<u64> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM official_server_config WHERE key = 'OFFICIAL_SERVER_ID'")
            .fetch_optional(&db.pool)
            .await
            .ok()?;
    if let Some((value,)) = row {
        if let Ok(guild_id) = value.trim().parse::<u64>() {
            if guild_id > 0 {
                return Some(guild_id);
            }
        }
    }
    crate::config::Config::official_guild_id()
}

/// True when the given guild is the official OmniCore bot server.
pub async fn is_official_guild(db: &Db, guild_id: u64) -> bool {
    match official_server_guild_id(db).await {
        Some(official) => official == guild_id,
        None => false,
    }
}

pub async fn set_player_id_limit(db: &Db, guild_id: u64, limit: Option<i64>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO server_subscriptions (guild_id, tier, status, player_id_limit)
         VALUES (?, 'free', 'active', ?)
         ON CONFLICT(guild_id) DO UPDATE SET player_id_limit = excluded.player_id_limit, updated_at = datetime('now')",
    )
    .bind(guild_id.to_string())
    .bind(limit)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn count_tracked_players(db: &Db, guild_id: u64) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM players WHERE home_guild_id = ?")
        .bind(guild_id.to_string())
        .fetch_one(&db.pool)
        .await?;
    Ok(row.0)
}

pub async fn is_within_player_limit(db: &Db, guild_id: u64) -> bool {
    match player_id_limit(db, guild_id).await {
        Ok(Some(limit)) => count_tracked_players(db, guild_id)
            .await
            .map(|n| n < limit)
            .unwrap_or(true),
        _ => true,
    }
}

// -------------------------------------------------------- server gift priority

pub async fn set_gift_priority(db: &Db, guild_id: u64, priority: Option<i64>) -> sqlx::Result<()> {
    match priority {
        Some(priority) => {
            sqlx::query(
                "INSERT INTO server_gift_priority (guild_id, priority) VALUES (?, ?)
                 ON CONFLICT(guild_id) DO UPDATE SET priority = excluded.priority",
            )
            .bind(guild_id.to_string())
            .bind(priority)
            .execute(&db.pool)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM server_gift_priority WHERE guild_id = ?")
                .bind(guild_id.to_string())
                .execute(&db.pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn gift_priority(db: &Db, guild_id: u64) -> sqlx::Result<Option<i64>> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT priority FROM server_gift_priority WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.map(|(p,)| p))
}

/// Order guild ids for announcement delivery (lower priority number first).
pub async fn order_guilds_by_gift_priority(db: &Db, guild_ids: Vec<u64>) -> Vec<u64> {
    let rows =
        sqlx::query_as::<_, (String, i64)>("SELECT guild_id, priority FROM server_gift_priority")
            .fetch_all(&db.pool)
            .await
            .unwrap_or_default();
    let priority: std::collections::HashMap<u64, i64> = rows
        .into_iter()
        .filter_map(|(g, p)| g.parse::<u64>().ok().map(|g| (g, p)))
        .collect();
    let max_p = priority.values().copied().max().unwrap_or(100);
    let mut ordered = guild_ids;
    ordered.sort_by_key(|guild_id| priority.get(guild_id).copied().unwrap_or(max_p + 1));
    ordered
}

// ------------------------------------------------------------- server staff

#[derive(Debug, Clone, FromRow)]
pub struct StaffRow {
    pub user_id: String,
    pub role: String,
}

pub async fn staff_role(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT role FROM server_staff WHERE guild_id = ? AND user_id = ?")
            .bind(guild_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.map(|(role,)| role))
}

pub async fn is_server_staff(db: &Db, guild_id: u64, user_id: u64) -> bool {
    staff_role(db, guild_id, user_id)
        .await
        .ok()
        .flatten()
        .is_some()
}

pub async fn is_server_admin(db: &Db, guild_id: u64, user_id: u64) -> bool {
    staff_role(db, guild_id, user_id)
        .await
        .ok()
        .flatten()
        .as_deref()
        == Some("server_admin")
}

pub async fn set_staff(
    db: &Db,
    guild_id: u64,
    user_id: u64,
    role: &str,
    added_by: u64,
) -> sqlx::Result<()> {
    if role != "server_admin" && role != "sub_admin" {
        return Err(sqlx::Error::Protocol(format!(
            "invalid staff role `{role}`"
        )));
    }
    sqlx::query(
        "INSERT INTO server_staff (guild_id, user_id, role, added_by) VALUES (?, ?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET role = excluded.role, added_by = excluded.added_by, added_at = datetime('now')",
    )
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .bind(role)
    .bind(added_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn remove_staff(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM server_staff WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn list_staff(db: &Db, guild_id: u64) -> sqlx::Result<Vec<StaffRow>> {
    sqlx::query_as::<_, StaffRow>(
        "SELECT user_id, role FROM server_staff WHERE guild_id = ? ORDER BY role ASC, added_at ASC",
    )
    .bind(guild_id.to_string())
    .fetch_all(&db.pool)
    .await
}

/// First user to open /settings in a guild with no server admin becomes one.
/// Returns true when the claim happened.
pub async fn claim_server_admin(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<bool> {
    let admins: Vec<StaffRow> = list_staff(db, guild_id)
        .await?
        .into_iter()
        .filter(|row| row.role == "server_admin")
        .collect();
    if !admins.is_empty() {
        return Ok(false);
    }
    set_staff(db, guild_id, user_id, "server_admin", user_id).await?;
    Ok(true)
}
