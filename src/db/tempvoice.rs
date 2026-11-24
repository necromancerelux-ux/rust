//! `tempvoice_settings` + `tempvoice_channels` tables.

use sqlx::FromRow;

use super::{now, Db};

#[derive(Debug, Clone, FromRow)]
pub struct TempVoiceSettings {
    pub guild_id: String,
    pub enabled: i64,
    pub lobby_channel_id: Option<String>,
    pub category_id: Option<String>,
    pub name_template: String,
    pub user_limit: i64,
    pub updated_at: i64,
}

impl TempVoiceSettings {
    pub fn enabled(&self) -> bool {
        self.enabled != 0
    }

    pub fn lobby_channel(&self) -> Option<u64> {
        self.lobby_channel_id
            .as_deref()
            .and_then(|v| v.parse().ok())
    }

    pub fn category(&self) -> Option<u64> {
        self.category_id.as_deref().and_then(|v| v.parse().ok())
    }

    pub fn name_template(&self) -> &str {
        if self.name_template.trim().is_empty() {
            "{user}'s channel"
        } else {
            self.name_template.as_str()
        }
    }

    pub fn user_limit(&self) -> u32 {
        self.user_limit.clamp(0, 99) as u32
    }
}

pub async fn settings(db: &Db, guild_id: u64) -> sqlx::Result<TempVoiceSettings> {
    let row = sqlx::query_as::<_, TempVoiceSettings>(
        "SELECT * FROM tempvoice_settings WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;

    Ok(row.unwrap_or(TempVoiceSettings {
        guild_id: guild_id.to_string(),
        enabled: 1,
        lobby_channel_id: None,
        category_id: None,
        name_template: "{user}'s channel".to_string(),
        user_limit: 0,
        updated_at: 0,
    }))
}

async fn upsert(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tempvoice_settings (guild_id, updated_at) VALUES (?, ?)
         ON CONFLICT(guild_id) DO NOTHING",
    )
    .bind(guild_id.to_string())
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_enabled(db: &Db, guild_id: u64, enabled: bool) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    sqlx::query("UPDATE tempvoice_settings SET enabled = ?, updated_at = ? WHERE guild_id = ?")
        .bind(enabled as i64)
        .bind(now())
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_lobby_opt(db: &Db, guild_id: u64, channel_id: Option<u64>) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    sqlx::query(
        "UPDATE tempvoice_settings SET lobby_channel_id = ?, updated_at = ? WHERE guild_id = ?",
    )
    .bind(channel_id.map(|id| id.to_string()))
    .bind(now())
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_lobby(db: &Db, guild_id: u64, channel_id: u64) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    sqlx::query(
        "UPDATE tempvoice_settings SET lobby_channel_id = ?, updated_at = ? WHERE guild_id = ?",
    )
    .bind(channel_id.to_string())
    .bind(now())
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_category(db: &Db, guild_id: u64, category: Option<u64>) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    sqlx::query("UPDATE tempvoice_settings SET category_id = ?, updated_at = ? WHERE guild_id = ?")
        .bind(category.map(|id| id.to_string()))
        .bind(now())
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_name_template(db: &Db, guild_id: u64, template: &str) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    sqlx::query(
        "UPDATE tempvoice_settings SET name_template = ?, updated_at = ? WHERE guild_id = ?",
    )
    .bind(template)
    .bind(now())
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_user_limit(db: &Db, guild_id: u64, limit: u32) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    sqlx::query("UPDATE tempvoice_settings SET user_limit = ?, updated_at = ? WHERE guild_id = ?")
        .bind(limit.min(99) as i64)
        .bind(now())
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn add_channel(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    owner_id: u64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO tempvoice_channels (channel_id, guild_id, owner_id, created_at)
         VALUES (?, ?, ?, datetime('now'))",
    )
    .bind(channel_id.to_string())
    .bind(guild_id.to_string())
    .bind(owner_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn get_channel(db: &Db, channel_id: u64) -> sqlx::Result<Option<TempVoiceChannel>> {
    sqlx::query_as::<_, TempVoiceChannel>(
        "SELECT channel_id, guild_id, owner_id, chat_channel_id, waiting_channel_id FROM tempvoice_channels WHERE channel_id = ?",
    )
    .bind(channel_id.to_string())
    .fetch_optional(&db.pool)
    .await
}

#[derive(Debug, Clone, FromRow)]
pub struct TempVoiceChannel {
    pub channel_id: String,
    pub guild_id: String,
    pub owner_id: String,
    pub chat_channel_id: Option<String>,
    pub waiting_channel_id: Option<String>,
}

impl TempVoiceChannel {
    pub fn channel_id(&self) -> Option<u64> {
        self.channel_id.parse().ok()
    }
    pub fn owner_id(&self) -> Option<u64> {
        self.owner_id.parse().ok()
    }
    pub fn chat_channel_id(&self) -> Option<u64> {
        self.chat_channel_id.as_deref().and_then(|s| s.parse().ok())
    }
    pub fn waiting_channel_id(&self) -> Option<u64> {
        self.waiting_channel_id
            .as_deref()
            .and_then(|s| s.parse().ok())
    }
}

pub async fn set_owner(db: &Db, channel_id: u64, owner_id: u64) -> sqlx::Result<()> {
    sqlx::query("UPDATE tempvoice_channels SET owner_id = ? WHERE channel_id = ?")
        .bind(owner_id.to_string())
        .bind(channel_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_chat_channel(
    db: &Db,
    channel_id: u64,
    chat_channel_id: Option<u64>,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE tempvoice_channels SET chat_channel_id = ? WHERE channel_id = ?")
        .bind(chat_channel_id.map(|id| id.to_string()))
        .bind(channel_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_waiting_channel(
    db: &Db,
    channel_id: u64,
    waiting_channel_id: Option<u64>,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE tempvoice_channels SET waiting_channel_id = ? WHERE channel_id = ?")
        .bind(waiting_channel_id.map(|id| id.to_string()))
        .bind(channel_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Find a temp channel row by its waiting-room channel id.
pub async fn get_by_waiting_channel(
    db: &Db,
    waiting_channel_id: u64,
) -> sqlx::Result<Option<TempVoiceChannel>> {
    sqlx::query_as::<_, TempVoiceChannel>(
        "SELECT channel_id, guild_id, owner_id, chat_channel_id, waiting_channel_id FROM tempvoice_channels WHERE waiting_channel_id = ?",
    )
    .bind(waiting_channel_id.to_string())
    .fetch_optional(&db.pool)
    .await
}

pub async fn remove_channel(db: &Db, channel_id: u64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM tempvoice_channels WHERE channel_id = ?")
        .bind(channel_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn count_for_guild(db: &Db, guild_id: u64) -> sqlx::Result<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tempvoice_channels WHERE guild_id = ?")
        .bind(guild_id.to_string())
        .fetch_one(&db.pool)
        .await?;
    Ok(n)
}
