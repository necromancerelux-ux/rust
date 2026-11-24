//! Branding: an owner-configurable contact button shown under public bot
//! messages, plus per-server bot nickname/avatar overrides.
//!
//! Mirrors the legacy `utils/branding.js` and `utils/botProfile.js`.

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, Default, FromRow)]
pub struct Branding {
    pub contact_enabled: i64,
    pub contact_label: Option<String>,
    pub contact_url: Option<String>,
}

impl Branding {
    pub fn is_enabled(&self) -> bool {
        self.contact_enabled != 0
    }

    pub fn label(&self) -> String {
        self.contact_label
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Message Necromancer".to_string())
    }
}

#[derive(Debug, Clone, Default, FromRow)]
pub struct GuildProfile {
    pub nickname: Option<String>,
    pub avatar_url: Option<String>,
}

pub async fn branding(db: &Db) -> sqlx::Result<Branding> {
    let row = sqlx::query_as::<_, Branding>(
        "SELECT contact_enabled, contact_label, contact_url FROM bot_branding WHERE id = 1",
    )
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.unwrap_or_default())
}

pub async fn set_branding_enabled(db: &Db, enabled: bool) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO bot_branding (id, contact_enabled, updated_at) VALUES (1, ?, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET contact_enabled = excluded.contact_enabled, updated_at = datetime('now')",
    )
    .bind(enabled as i64)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_contact(db: &Db, url: Option<&str>, label: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO bot_branding (id, contact_enabled, contact_url, contact_label, updated_at)
         VALUES (1, 1, ?, ?, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET contact_url = excluded.contact_url, contact_label = excluded.contact_label, contact_enabled = 1, updated_at = datetime('now')",
    )
    .bind(url)
    .bind(label)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn clear_branding(db: &Db) -> sqlx::Result<()> {
    sqlx::query("UPDATE bot_branding SET contact_enabled = 0, contact_url = NULL, updated_at = datetime('now') WHERE id = 1")
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn guild_profile(db: &Db, guild_id: u64) -> sqlx::Result<GuildProfile> {
    let row = sqlx::query_as::<_, GuildProfile>(
        "SELECT nickname, avatar_url FROM bot_guild_profiles WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.unwrap_or_default())
}

pub async fn set_nickname(db: &Db, guild_id: u64, nickname: Option<&str>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO bot_guild_profiles (guild_id, nickname, updated_at) VALUES (?, ?, datetime('now'))
         ON CONFLICT(guild_id) DO UPDATE SET nickname = excluded.nickname, updated_at = datetime('now')",
    )
    .bind(guild_id.to_string())
    .bind(nickname)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_avatar(db: &Db, guild_id: u64, avatar_url: Option<&str>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO bot_guild_profiles (guild_id, avatar_url, updated_at) VALUES (?, ?, datetime('now'))
         ON CONFLICT(guild_id) DO UPDATE SET avatar_url = excluded.avatar_url, updated_at = datetime('now')",
    )
    .bind(guild_id.to_string())
    .bind(avatar_url)
    .execute(&db.pool)
    .await?;
    Ok(())
}
