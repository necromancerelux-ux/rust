//! Sneak Peek storage: per-guild destination channel + shared poller state.

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, FromRow)]
pub struct SneakPeekSettings {
    pub guild_id: String,
    pub channel_id: Option<String>,
}

impl SneakPeekSettings {
    pub fn channel(&self) -> Option<u64> {
        self.channel_id.as_deref().and_then(|v| v.parse().ok())
    }
}

pub async fn settings(db: &Db, guild_id: u64) -> sqlx::Result<SneakPeekSettings> {
    let row = sqlx::query_as::<_, SneakPeekSettings>(
        "SELECT channel_id FROM sneak_peek_settings WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.unwrap_or(SneakPeekSettings {
        guild_id: guild_id.to_string(),
        channel_id: None,
    }))
}

pub async fn set_channel(db: &Db, guild_id: u64, channel: Option<u64>) -> sqlx::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO sneak_peek_settings (guild_id) VALUES (?)")
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    sqlx::query("UPDATE sneak_peek_settings SET channel_id = ? WHERE guild_id = ?")
        .bind(channel.map(|id| id.to_string()))
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Default, FromRow)]
pub struct SneakPeekState {
    pub last_url: Option<String>,
    pub last_modified: Option<String>,
    pub checked_at: Option<String>,
}

pub async fn state(db: &Db) -> sqlx::Result<SneakPeekState> {
    let row = sqlx::query_as::<_, SneakPeekState>(
        "SELECT last_url, last_modified, checked_at FROM sneak_peek_state WHERE id = 1",
    )
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.unwrap_or_default())
}

pub async fn set_state(db: &Db, url: &str, modified: Option<&str>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO sneak_peek_state (id, last_url, last_modified, checked_at) VALUES (1, ?, ?, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET last_url = excluded.last_url, last_modified = excluded.last_modified, checked_at = datetime('now')",
    )
    .bind(url)
    .bind(modified)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn touch_checked(db: &Db) -> sqlx::Result<()> {
    let state = state(db).await.unwrap_or_default();
    sqlx::query(
        "INSERT INTO sneak_peek_state (id, last_url, last_modified, checked_at) VALUES (1, ?, ?, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET checked_at = datetime('now')",
    )
    .bind(state.last_url.as_deref())
    .bind(state.last_modified.as_deref())
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Every configured destination channel: `(guild_id, channel_id)`.
pub async fn destinations(db: &Db) -> sqlx::Result<Vec<(u64, u64)>> {
    let rows: Vec<SneakPeekSettings> = sqlx::query_as(
        "SELECT guild_id, channel_id FROM sneak_peek_settings WHERE channel_id IS NOT NULL",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| Some((row.guild_id.parse::<u64>().ok()?, row.channel()?)))
        .collect())
}
