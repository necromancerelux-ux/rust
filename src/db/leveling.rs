//! Leveling tables: per-guild settings and per-user XP.

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, FromRow)]
pub struct LevelSettings {
    pub guild_id: String,
    pub enabled: i64,
    pub xp_min: i64,
    pub xp_max: i64,
    pub cooldown_sec: i64,
    pub voice_xp: i64,
    pub announce_mode: String,
    pub announce_channel_id: Option<String>,
    pub announce_message: String,
    pub stack_rewards: i64,
    pub ignored_channel_ids: String,
}

pub const DEFAULT_MESSAGE: &str = "🎉 {user} reached **level {level}**!";

impl LevelSettings {
    pub fn enabled(&self) -> bool {
        self.enabled != 0
    }

    pub fn stack_rewards(&self) -> bool {
        self.stack_rewards != 0
    }

    pub fn announce_channel(&self) -> Option<u64> {
        self.announce_channel_id
            .as_deref()
            .and_then(|value| value.parse().ok())
    }

    pub fn ignored_channels(&self) -> Vec<u64> {
        parse_ignored(&self.ignored_channel_ids)
    }
}

fn parse_ignored(text: &str) -> Vec<u64> {
    serde_json::from_str::<Vec<String>>(text)
        .map(|items| {
            items
                .into_iter()
                .filter_map(|item| item.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default)]
pub struct LevelUser {
    pub xp: i64,
    pub messages: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct LevelReward {
    pub level: i64,
    pub role_id: String,
}

/// Fetch settings, or the documented defaults when the guild has no row yet.
///
/// The legacy JavaScript bot created `level_settings.updated_at` as a TEXT
/// column and left `announce_message` nullable. Rust's original schema used an
/// INTEGER timestamp and NOT NULL message, so a shared database could fail to
/// decode either direction. The query deliberately omits `updated_at` and
/// coalesces the nullable message/mode so both schemas remain readable.
pub async fn settings(db: &Db, guild_id: u64) -> sqlx::Result<LevelSettings> {
    let row = sqlx::query_as::<_, LevelSettings>(
        "SELECT guild_id, enabled, xp_min, xp_max, cooldown_sec, voice_xp,
                COALESCE(NULLIF(announce_mode, ''), 'same') AS announce_mode,
                announce_channel_id,
                COALESCE(announce_message, ?) AS announce_message,
                stack_rewards,
                COALESCE(ignored_channel_ids, '[]') AS ignored_channel_ids
         FROM level_settings WHERE guild_id = ?",
    )
    .bind(DEFAULT_MESSAGE)
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;

    Ok(row.unwrap_or(LevelSettings {
        guild_id: guild_id.to_string(),
        enabled: 1,
        xp_min: 15,
        xp_max: 25,
        cooldown_sec: 60,
        voice_xp: 0,
        announce_mode: "same".to_string(),
        announce_channel_id: None,
        announce_message: DEFAULT_MESSAGE.to_string(),
        stack_rewards: 1,
        ignored_channel_ids: "[]".to_string(),
    }))
}

async fn upsert(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    // `datetime('now')` matches the legacy JavaScript column type, so the row
    // stays readable when the JS and Rust bots share one SQLite file.
    sqlx::query(
        "INSERT INTO level_settings (guild_id, updated_at) VALUES (?, datetime('now'))
         ON CONFLICT(guild_id) DO NOTHING",
    )
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

async fn set_field(db: &Db, guild_id: u64, column: &str, value: &str) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    // `column` comes from a closed match in `set_*` helpers — never user input.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE level_settings SET {column} = ?, updated_at = datetime('now') WHERE guild_id = ?"
    )))
    .bind(value)
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_enabled(db: &Db, guild_id: u64, enabled: bool) -> sqlx::Result<()> {
    set_field(db, guild_id, "enabled", if enabled { "1" } else { "0" }).await
}

pub async fn set_xp_min(db: &Db, guild_id: u64, value: i64) -> sqlx::Result<()> {
    set_field(db, guild_id, "xp_min", &value.to_string()).await
}

pub async fn set_xp_max(db: &Db, guild_id: u64, value: i64) -> sqlx::Result<()> {
    set_field(db, guild_id, "xp_max", &value.to_string()).await
}

pub async fn set_cooldown(db: &Db, guild_id: u64, value: i64) -> sqlx::Result<()> {
    set_field(db, guild_id, "cooldown_sec", &value.to_string()).await
}

pub async fn set_voice_xp(db: &Db, guild_id: u64, value: i64) -> sqlx::Result<()> {
    set_field(db, guild_id, "voice_xp", &value.to_string()).await
}

pub async fn set_announce_mode(db: &Db, guild_id: u64, mode: &str) -> sqlx::Result<()> {
    set_field(db, guild_id, "announce_mode", mode).await
}

pub async fn set_announce_channel(
    db: &Db,
    guild_id: u64,
    channel_id: Option<u64>,
) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    // Store a real NULL (like the JavaScript bot) instead of an empty string so
    // either runtime reads the same "no channel" state.
    sqlx::query(
        "UPDATE level_settings SET announce_channel_id = ?, updated_at = datetime('now') WHERE guild_id = ?",
    )
    .bind(channel_id.map(|id| id.to_string()))
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_announce_message(db: &Db, guild_id: u64, message: &str) -> sqlx::Result<()> {
    set_field(db, guild_id, "announce_message", message).await
}

pub async fn set_stack_rewards(db: &Db, guild_id: u64, stack: bool) -> sqlx::Result<()> {
    set_field(db, guild_id, "stack_rewards", if stack { "1" } else { "0" }).await
}

pub async fn set_ignored(db: &Db, guild_id: u64, ids: &[u64]) -> sqlx::Result<()> {
    let encoded = serde_json::to_string(&ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
        .unwrap_or_else(|_| "[]".to_string());
    set_field(db, guild_id, "ignored_channel_ids", &encoded).await
}

/// Add (or subtract) XP and return the before/after totals.
pub async fn add_xp(
    db: &Db,
    guild_id: u64,
    user_id: u64,
    amount: i64,
    messages: i64,
) -> sqlx::Result<(i64, i64)> {
    let mut tx = db.pool.begin().await?;

    let row: Option<(i64,)> =
        sqlx::query_as("SELECT xp FROM level_users WHERE guild_id = ? AND user_id = ?")
            .bind(guild_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(&mut *tx)
            .await?;

    let before = row.map(|(xp,)| xp).unwrap_or(0);
    let after = (before + amount).max(0);

    sqlx::query(
        "INSERT INTO level_users (guild_id, user_id, xp, messages) VALUES (?, ?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET
             xp = excluded.xp,
             messages = level_users.messages + excluded.messages",
    )
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .bind(after)
    .bind(messages)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((before, after))
}

pub async fn get_user(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<LevelUser> {
    let row: Option<(i64, i64)> =
        sqlx::query_as("SELECT xp, messages FROM level_users WHERE guild_id = ? AND user_id = ?")
            .bind(guild_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(&db.pool)
            .await?;

    Ok(row
        .map(|(xp, messages)| LevelUser { xp, messages })
        .unwrap_or_default())
}

pub async fn get_rank(db: &Db, guild_id: u64, xp: i64) -> sqlx::Result<i64> {
    let (rank,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) + 1 FROM level_users WHERE guild_id = ? AND xp > ?")
            .bind(guild_id.to_string())
            .bind(xp)
            .fetch_one(&db.pool)
            .await?;
    Ok(rank)
}

pub async fn top(
    db: &Db,
    guild_id: u64,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<(String, i64)>> {
    sqlx::query_as::<_, (String, i64)>(
        "SELECT user_id, xp FROM level_users
         WHERE guild_id = ? AND xp > 0
         ORDER BY xp DESC, user_id
         LIMIT ? OFFSET ?",
    )
    .bind(guild_id.to_string())
    .bind(limit)
    .bind(offset)
    .fetch_all(&db.pool)
    .await
}

pub async fn count_ranked(db: &Db, guild_id: u64) -> sqlx::Result<i64> {
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM level_users WHERE guild_id = ? AND xp > 0")
            .bind(guild_id.to_string())
            .fetch_one(&db.pool)
            .await?;
    Ok(n)
}

pub async fn reset_user(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<u64> {
    let result = sqlx::query("DELETE FROM level_users WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn reset_guild(db: &Db, guild_id: u64) -> sqlx::Result<u64> {
    let result = sqlx::query("DELETE FROM level_users WHERE guild_id = ?")
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn list_rewards(db: &Db, guild_id: u64) -> sqlx::Result<Vec<LevelReward>> {
    sqlx::query_as::<_, LevelReward>(
        "SELECT level, role_id FROM level_rewards WHERE guild_id = ? ORDER BY level, role_id",
    )
    .bind(guild_id.to_string())
    .fetch_all(&db.pool)
    .await
}

pub async fn set_reward(db: &Db, guild_id: u64, level: i64, role_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO level_rewards (guild_id, level, role_id) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, role_id) DO UPDATE SET level = excluded.level",
    )
    .bind(guild_id.to_string())
    .bind(level.max(0))
    .bind(role_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn remove_reward(db: &Db, guild_id: u64, role_id: u64) -> sqlx::Result<u64> {
    let result = sqlx::query("DELETE FROM level_rewards WHERE guild_id = ? AND role_id = ?")
        .bind(guild_id.to_string())
        .bind(role_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected())
}
