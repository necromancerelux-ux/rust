//! `lookup_settings` table: the public lookup panel channel.

use sqlx::FromRow;

use super::now;
use super::Db;

#[derive(Debug, Clone, FromRow)]
pub struct LookupSettings {
    pub guild_id: String,
    pub channel_id: Option<String>,
    pub player_panel_msg_id: Option<String>,
    pub alliance_panel_msg_id: Option<String>,
    pub state_panel_msg_id: Option<String>,
    pub updated_at: Option<i64>,
}

impl LookupSettings {
    pub fn channel(&self) -> Option<u64> {
        self.channel_id.as_deref().and_then(|v| v.parse().ok())
    }

    pub fn player_panel(&self) -> Option<u64> {
        self.player_panel_msg_id
            .as_deref()
            .and_then(|v| v.parse().ok())
    }

    pub fn alliance_panel(&self) -> Option<u64> {
        self.alliance_panel_msg_id
            .as_deref()
            .and_then(|v| v.parse().ok())
    }

    pub fn state_panel(&self) -> Option<u64> {
        self.state_panel_msg_id
            .as_deref()
            .and_then(|v| v.parse().ok())
    }

    pub fn blank(guild_id: u64) -> Self {
        Self {
            guild_id: guild_id.to_string(),
            channel_id: None,
            player_panel_msg_id: None,
            alliance_panel_msg_id: None,
            state_panel_msg_id: None,
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Player,
    Alliance,
    State,
}

impl PanelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Alliance => "alliance",
            Self::State => "state",
        }
    }
}

pub async fn get(db: &Db, guild_id: u64) -> sqlx::Result<LookupSettings> {
    let row =
        sqlx::query_as::<_, LookupSettings>("SELECT * FROM lookup_settings WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.unwrap_or_else(|| LookupSettings::blank(guild_id)))
}

pub async fn set_channel(db: &Db, guild_id: u64, channel_id: Option<u64>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO lookup_settings (guild_id, channel_id, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET
             channel_id = excluded.channel_id,
             updated_at = excluded.updated_at",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.map(|id| id.to_string()))
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_panel(
    db: &Db,
    guild_id: u64,
    kind: PanelKind,
    message_id: Option<u64>,
) -> sqlx::Result<()> {
    // Column name comes from a closed enum, never user input.
    let column = match kind {
        PanelKind::Player => "player_panel_msg_id",
        PanelKind::Alliance => "alliance_panel_msg_id",
        PanelKind::State => "state_panel_msg_id",
    };
    // Column name comes from a closed enum, never user input, so the
    // interpolated string is audited safe to build at runtime.
    let sql = format!(
        "INSERT INTO lookup_settings (guild_id, {column}, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET
             {column} = excluded.{column},
             updated_at = excluded.updated_at"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild_id.to_string())
        .bind(message_id.map(|id| id.to_string()))
        .bind(now())
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Guilds that have lookup panels deployed.
pub async fn configured(db: &Db) -> sqlx::Result<Vec<LookupSettings>> {
    sqlx::query_as::<_, LookupSettings>(
        "SELECT * FROM lookup_settings
         WHERE channel_id IS NOT NULL
           AND (player_panel_msg_id IS NOT NULL
             OR alliance_panel_msg_id IS NOT NULL
             OR state_panel_msg_id IS NOT NULL)",
    )
    .fetch_all(&db.pool)
    .await
}
