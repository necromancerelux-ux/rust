//! `music_settings` table: per-guild control panel configuration.

use std::fmt;
use std::str::FromStr;

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoopMode {
    #[default]
    Off,
    Track,
    Queue,
}

impl LoopMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Track => "track",
            Self::Queue => "queue",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Track => "Track",
            Self::Queue => "Queue",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Track,
            Self::Track => Self::Queue,
            Self::Queue => Self::Off,
        }
    }
}

impl FromStr for LoopMode {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "track" | "song" | "one" => Self::Track,
            "queue" | "all" => Self::Queue,
            _ => Self::Off,
        })
    }
}

impl fmt::Display for LoopMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct MusicSettings {
    pub guild_id: String,
    pub channel_id: Option<String>,
    pub panel_message_id: Option<String>,
    pub volume: i64,
    pub loop_mode: String,
}

impl MusicSettings {
    pub fn loop_mode(&self) -> LoopMode {
        self.loop_mode.parse().unwrap_or_default()
    }

    pub fn channel_id(&self) -> Option<u64> {
        self.channel_id.as_deref().and_then(|v| v.parse().ok())
    }

    pub fn panel_message_id(&self) -> Option<u64> {
        self.panel_message_id
            .as_deref()
            .and_then(|v| v.parse().ok())
    }

    pub fn volume(&self) -> u16 {
        self.volume.clamp(0, 200) as u16
    }
}

pub async fn get(db: &Db, guild_id: u64) -> sqlx::Result<Option<MusicSettings>> {
    sqlx::query_as::<_, MusicSettings>("SELECT * FROM music_settings WHERE guild_id = ?")
        .bind(guild_id.to_string())
        .fetch_optional(&db.pool)
        .await
}

/// Fetch, creating a default row when absent.
pub async fn get_or_default(db: &Db, guild_id: u64) -> sqlx::Result<MusicSettings> {
    if let Some(row) = get(db, guild_id).await? {
        return Ok(row);
    }
    sqlx::query(
        "INSERT INTO music_settings (guild_id, volume, loop_mode) VALUES (?, 80, 'off')
         ON CONFLICT(guild_id) DO NOTHING",
    )
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(MusicSettings {
        guild_id: guild_id.to_string(),
        channel_id: None,
        panel_message_id: None,
        volume: 80,
        loop_mode: "off".to_string(),
    })
}

pub async fn set_panel(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: Option<u64>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO music_settings (guild_id, channel_id, panel_message_id)
         VALUES (?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET
             channel_id = excluded.channel_id,
             panel_message_id = excluded.panel_message_id",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.to_string())
    .bind(message_id.map(|id| id.to_string()))
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_volume(db: &Db, guild_id: u64, volume: u16) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO music_settings (guild_id, volume) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET volume = excluded.volume",
    )
    .bind(guild_id.to_string())
    .bind(volume.min(200) as i64)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_loop(db: &Db, guild_id: u64, mode: LoopMode) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO music_settings (guild_id, loop_mode) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET loop_mode = excluded.loop_mode",
    )
    .bind(guild_id.to_string())
    .bind(mode.as_str())
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Guilds that have a control panel pinned somewhere, for restart recovery.
pub async fn panels(db: &Db) -> sqlx::Result<Vec<MusicSettings>> {
    sqlx::query_as::<_, MusicSettings>(
        "SELECT * FROM music_settings WHERE channel_id IS NOT NULL AND panel_message_id IS NOT NULL",
    )
    .fetch_all(&db.pool)
    .await
}
