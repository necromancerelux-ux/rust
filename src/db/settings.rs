//! Generic per-guild key/value settings used by the settings hub.

use sqlx::FromRow;

use super::{now, Db};

#[derive(Debug, Clone, FromRow)]
pub struct GuildSetting {
    pub guild_id: String,
    pub key: String,
    pub value: Option<String>,
    pub updated_at: i64,
}

/// Well-known keys. Keeping them in one place avoids typo'd strings.
pub mod keys {
    pub const GIFT_ANNOUNCE_CHANNEL: &str = "giftcode.announce_channel";
    pub const GIFT_AUTO_SYNC: &str = "giftcode.auto_sync";
    pub const GIFT_PROBE_FID: &str = "giftcode.probe_fid";
    pub const GIFT_PROBE_STATE: &str = "giftcode.probe_state";
    pub const GIFT_ANNOUNCE_TEMPLATE: &str = "giftcode.announce_template";
    pub const PLAYER_PANEL_CHANNEL: &str = "playerid.panel_channel";
    pub const PLAYER_PANEL_MESSAGE: &str = "playerid.panel_message";
    pub const PLAYER_ROSTER_REFRESH_HOURS: &str = "playerid.roster_refresh_hours";
    pub const PLAYER_ROSTER_LAST_REFRESH: &str = "playerid.roster_last_refresh";
    pub const GIVEAWAY_CHANNEL: &str = "giveaway.channel";
    pub const LOOKUP_AUTO_REFRESH: &str = "lookup.auto_refresh";
    pub const WELCOME_CHANNEL: &str = "welcome.channel";
    pub const LOG_CHANNEL: &str = "log.channel";
    pub const NOTIF_DEFAULT_SLOTS: &str = "notifications.default_slots";
    pub const LEVELING_PANEL_CHANNEL: &str = "leveling.panel_channel";
    pub const LEVELING_PANEL_MESSAGE: &str = "leveling.panel_message";
}

pub async fn get(db: &Db, guild_id: u64, key: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT value FROM settings WHERE guild_id = ? AND key = ? AND value IS NOT NULL",
    )
    .bind(guild_id.to_string())
    .bind(key)
    .fetch_optional(&db.pool)
    .await
}

pub async fn get_u64(db: &Db, guild_id: u64, key: &str) -> sqlx::Result<Option<u64>> {
    Ok(get(db, guild_id, key)
        .await?
        .and_then(|v| v.trim().parse().ok()))
}

pub async fn get_bool(db: &Db, guild_id: u64, key: &str, default: bool) -> sqlx::Result<bool> {
    Ok(match get(db, guild_id, key).await?.as_deref() {
        Some("1") | Some("true") | Some("yes") | Some("on") => true,
        Some("0") | Some("false") | Some("no") | Some("off") => false,
        _ => default,
    })
}

pub async fn set(db: &Db, guild_id: u64, key: &str, value: Option<&str>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO settings (guild_id, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(guild_id, key) DO UPDATE SET
             value = excluded.value,
             updated_at = excluded.updated_at",
    )
    .bind(guild_id.to_string())
    .bind(key)
    .bind(value)
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn all(db: &Db, guild_id: u64) -> sqlx::Result<Vec<GuildSetting>> {
    sqlx::query_as::<_, GuildSetting>("SELECT * FROM settings WHERE guild_id = ? ORDER BY key")
        .bind(guild_id.to_string())
        .fetch_all(&db.pool)
        .await
}

/// Every guild that has a non-empty value for `key`. Used by background jobs
/// that announce into any server configured for a feature.
pub async fn guilds_with_key(db: &Db, key: &str) -> sqlx::Result<Vec<(u64, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT guild_id, value FROM settings WHERE key = ? AND value IS NOT NULL AND value <> ''",
    )
    .bind(key)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(guild, value)| guild.parse().ok().map(|id| (id, value)))
        .collect())
}
