//! Log settings: one shared admin log channel with per-category toggles.
//!
//! The channel reuses `giftcode_guild_settings.admin_log_channel_id` when it is
//! set (legacy parity) — we never store a second copy.

use sqlx::FromRow;

use super::{now, Db};

#[derive(Debug, Clone, FromRow)]
struct LogRow {
    disabled_categories: String,
}

/// All categories recognised by the settings panel, in display order.
pub const CATEGORIES: &[&str] = &[
    "giftcode", "players", "members", "messages", "voice", "channels", "roles", "server",
];

fn parse(text: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(text).unwrap_or_default()
}

pub async fn disabled(db: &Db, guild_id: u64) -> sqlx::Result<Vec<String>> {
    let row = sqlx::query_as::<_, LogRow>(
        "SELECT disabled_categories FROM log_settings WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(row
        .map(|r| parse(&r.disabled_categories))
        .unwrap_or_default())
}

pub async fn is_enabled(db: &Db, guild_id: u64, category: &str) -> bool {
    disabled(db, guild_id)
        .await
        .map(|list| !list.iter().any(|c| c == category))
        .unwrap_or(true)
}

pub async fn set_disabled(db: &Db, guild_id: u64, list: &[String]) -> sqlx::Result<()> {
    let encoded = serde_json::to_string(list).unwrap_or_else(|_| "[]".to_string());
    sqlx::query(
        "INSERT INTO log_settings (guild_id, disabled_categories, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET
             disabled_categories = excluded.disabled_categories,
             updated_at = excluded.updated_at",
    )
    .bind(guild_id.to_string())
    .bind(encoded)
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// The admin log channel id, reusing the legacy column.
pub async fn channel_id(db: &Db, guild_id: u64) -> sqlx::Result<Option<u64>> {
    let row: Option<(Option<String>,)> = if sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM pragma_table_info('giftcode_guild_settings') WHERE name = 'admin_log_channel_id'",
    )
    .fetch_one(&db.pool)
    .await
    .is_ok()
    {
        sqlx::query_as(
            "SELECT admin_log_channel_id FROM giftcode_guild_settings WHERE guild_id = ?",
        )
        .bind(guild_id.to_string())
        .fetch_optional(&db.pool)
        .await?
    } else {
        None
    };

    Ok(row
        .and_then(|(value,)| value)
        .and_then(|value| value.parse().ok()))
}

pub async fn set_channel(db: &Db, guild_id: u64, channel_id: Option<u64>) -> sqlx::Result<()> {
    // The legacy column lives on giftcode_guild_settings.
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('giftcode_guild_settings') WHERE name = 'admin_log_channel_id'",
    )
    .fetch_one(&db.pool)
    .await?;

    if exists == 1 {
        sqlx::query(
            "UPDATE giftcode_guild_settings SET admin_log_channel_id = ? WHERE guild_id = ?",
        )
        .bind(channel_id.map(|id| id.to_string()))
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
        return Ok(());
    }

    // Fallback: keep our own row so behaviour is preserved on fresh databases.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS log_channel (
            guild_id TEXT PRIMARY KEY,
            channel_id TEXT
        )",
    )
    .execute(&db.pool)
    .await?;
    sqlx::query(
        "INSERT INTO log_channel (guild_id, channel_id) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET channel_id = excluded.channel_id",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.map(|id| id.to_string()))
    .execute(&db.pool)
    .await?;
    Ok(())
}
