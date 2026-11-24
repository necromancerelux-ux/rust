//! WOS Announcements relay storage: owner-set hub channel (singleton) and
//! per-guild destination channels.

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, Default, FromRow)]
pub struct WosHub {
    pub guild_id: Option<String>,
    pub channel_id: Option<String>,
}

impl WosHub {
    pub fn channel(&self) -> Option<u64> {
        self.channel_id.as_deref().and_then(|v| v.parse().ok())
    }

    pub fn guild(&self) -> Option<u64> {
        self.guild_id.as_deref().and_then(|v| v.parse().ok())
    }
}

pub async fn hub(db: &Db) -> sqlx::Result<WosHub> {
    let row = sqlx::query_as::<_, WosHub>(
        "SELECT guild_id, channel_id FROM wos_hub_channel WHERE id = 1",
    )
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.unwrap_or_default())
}

pub async fn set_hub(db: &Db, guild_id: u64, channel_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO wos_hub_channel (id, guild_id, channel_id) VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET guild_id = excluded.guild_id, channel_id = excluded.channel_id, set_at = datetime('now')",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn clear_hub(db: &Db) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM wos_hub_channel WHERE id = 1")
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn destination(db: &Db, guild_id: u64) -> sqlx::Result<Option<u64>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT channel_id FROM wos_relay_settings WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row
        .and_then(|(channel,)| channel)
        .and_then(|v| v.parse().ok()))
}

pub async fn set_destination(db: &Db, guild_id: u64, channel: Option<u64>) -> sqlx::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO wos_relay_settings (guild_id) VALUES (?)")
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    sqlx::query("UPDATE wos_relay_settings SET channel_id = ? WHERE guild_id = ?")
        .bind(channel.map(|id| id.to_string()))
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Every server subscribed to relays, excluding the hub itself.
pub async fn destinations(db: &Db) -> sqlx::Result<Vec<(u64, u64)>> {
    let hub = hub(db).await?;
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT guild_id, channel_id FROM wos_relay_settings WHERE channel_id IS NOT NULL",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(guild_id, channel_id)| {
            let guild_id: u64 = guild_id.parse().ok()?;
            let channel_id: u64 = channel_id?.parse().ok()?;
            if hub.guild() == Some(guild_id) && hub.channel() == Some(channel_id) {
                return None;
            }
            Some((guild_id, channel_id))
        })
        .collect())
}
