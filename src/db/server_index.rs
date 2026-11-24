//! Server Index settings (auto-updating public channel directory).

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, FromRow)]
pub struct ServerIndexSettings {
    pub guild_id: String,
    pub channel_id: Option<String>,
    pub message_ids: Option<String>,
    pub enabled: i64,
    pub include_threads: i64,
    pub include_voice: i64,
}

impl ServerIndexSettings {
    pub fn channel(&self) -> Option<u64> {
        self.channel_id.as_deref().and_then(|v| v.parse().ok())
    }

    pub fn enabled(&self) -> bool {
        self.enabled != 0
    }

    pub fn include_threads(&self) -> bool {
        self.include_threads != 0
    }

    pub fn include_voice(&self) -> bool {
        self.include_voice != 0
    }

    pub fn message_ids(&self) -> Vec<u64> {
        self.message_ids
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .map(|ids| ids.into_iter().filter_map(|id| id.parse().ok()).collect())
            .unwrap_or_default()
    }
}

async fn ensure_row(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO server_index_settings (guild_id) VALUES (?)")
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn get(db: &Db, guild_id: u64) -> sqlx::Result<ServerIndexSettings> {
    ensure_row(db, guild_id).await?;
    sqlx::query_as::<_, ServerIndexSettings>(
        "SELECT guild_id, channel_id, message_ids, enabled, include_threads, include_voice
         FROM server_index_settings WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_one(&db.pool)
    .await
}

pub async fn set_channel(db: &Db, guild_id: u64, channel: Option<u64>) -> sqlx::Result<()> {
    ensure_row(db, guild_id).await?;
    sqlx::query("UPDATE server_index_settings SET channel_id = ?, updated_at = datetime('now') WHERE guild_id = ?")
        .bind(channel.map(|id| id.to_string()))
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_message_ids(db: &Db, guild_id: u64, ids: &[u64]) -> sqlx::Result<()> {
    ensure_row(db, guild_id).await?;
    let json =
        serde_json::to_string(&ids.iter().map(|id| id.to_string()).collect::<Vec<_>>()).unwrap();
    sqlx::query("UPDATE server_index_settings SET message_ids = ?, updated_at = datetime('now') WHERE guild_id = ?")
        .bind(json)
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_enabled(db: &Db, guild_id: u64, enabled: bool) -> sqlx::Result<()> {
    ensure_row(db, guild_id).await?;
    sqlx::query("UPDATE server_index_settings SET enabled = ?, updated_at = datetime('now') WHERE guild_id = ?")
        .bind(if enabled { 1 } else { 0 })
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_include_threads(db: &Db, guild_id: u64, on: bool) -> sqlx::Result<()> {
    ensure_row(db, guild_id).await?;
    sqlx::query("UPDATE server_index_settings SET include_threads = ?, updated_at = datetime('now') WHERE guild_id = ?")
        .bind(if on { 1 } else { 0 })
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_include_voice(db: &Db, guild_id: u64, on: bool) -> sqlx::Result<()> {
    ensure_row(db, guild_id).await?;
    sqlx::query("UPDATE server_index_settings SET include_voice = ?, updated_at = datetime('now') WHERE guild_id = ?")
        .bind(if on { 1 } else { 0 })
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Guilds with auto-index enabled and a post channel set.
pub async fn list_active_guilds(db: &Db) -> sqlx::Result<Vec<u64>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT guild_id FROM server_index_settings WHERE enabled = 1 AND channel_id IS NOT NULL",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id,)| id.parse().ok())
        .collect())
}
