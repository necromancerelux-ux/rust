//! Calculator panels: persistent per-channel panels for the WoS calculators.
//!
//! Panels are per-guild, per-`calc_key` (embedded in the settings hub like
//! every other panel). A calculator counts as enabled simply by having a
//! channel set — same convention as the legacy bot.

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, Default, FromRow)]
pub struct CalculatorSettings {
    pub guild_id: String,
    pub calc_key: String,
    pub channel_id: Option<String>,
    pub panel_message_id: Option<String>,
}

impl CalculatorSettings {
    pub fn channel(&self) -> Option<u64> {
        self.channel_id.as_deref().and_then(|v| v.parse().ok())
    }

    pub fn message_id(&self) -> Option<u64> {
        self.panel_message_id
            .as_deref()
            .and_then(|v| v.parse().ok())
    }
}

pub const SVS: &str = "svs";
pub const BUILDING: &str = "building";
pub const CHIEF_GEAR: &str = "chiefgear";
pub const CHARM: &str = "charm";
pub const PETS: &str = "pets";
pub const WAR_ACADEMY: &str = "waracademy";

pub async fn get(db: &Db, guild_id: u64, calc_key: &str) -> sqlx::Result<CalculatorSettings> {
    sqlx::query_as::<_, CalculatorSettings>(
        "SELECT guild_id, calc_key, channel_id, panel_message_id FROM calculator_settings
         WHERE guild_id = ? AND calc_key = ?",
    )
    .bind(guild_id.to_string())
    .bind(calc_key)
    .fetch_optional(&db.pool)
    .await
    .map(|row| {
        row.unwrap_or(CalculatorSettings {
            guild_id: guild_id.to_string(),
            calc_key: calc_key.to_string(),
            channel_id: None,
            panel_message_id: None,
        })
    })
}

pub async fn set_channel(
    db: &Db,
    guild_id: u64,
    calc_key: &str,
    channel: Option<u64>,
    message_id: Option<u64>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO calculator_settings (guild_id, calc_key, channel_id, panel_message_id) VALUES (?, ?, ?, ?)
         ON CONFLICT(guild_id, calc_key) DO UPDATE SET channel_id = excluded.channel_id, panel_message_id = excluded.panel_message_id",
    )
    .bind(guild_id.to_string())
    .bind(calc_key)
    .bind(channel.map(|id| id.to_string()))
    .bind(message_id.map(|id| id.to_string()))
    .execute(&db.pool)
    .await?;
    Ok(())
}
