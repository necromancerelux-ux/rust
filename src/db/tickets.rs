//! Support ticket storage (`ticket_settings` + `tickets`).

use sqlx::FromRow;

use super::{now, Db};

/// JS `tickets/store.js` defaults.
pub const DEFAULT_PANEL_TITLE: &str = "🎫 Support Tickets";
pub const DEFAULT_PANEL_DESCRIPTION: &str = "Need help or want to talk to the staff privately?\n\nPress **Open Ticket** below, tell us briefly what it is about, and a private channel will be created just for you and the team.";
pub const DEFAULT_WELCOME: &str = "Hi {mention}, thanks for opening a ticket! Please describe your issue in as much detail as you can — a staff member will be with you shortly.";
pub const DEFAULT_COLOR: i64 = 0x5865F2;

#[derive(Debug, Clone, FromRow)]
pub struct TicketSettings {
    pub guild_id: String,
    pub panel_channel_id: Option<String>,
    pub panel_message_id: Option<String>,
    pub category_id: Option<String>,
    pub log_channel_id: Option<String>,
    pub staff_role_ids: String,
    pub max_open_per_user: i64,
    pub panel_title: Option<String>,
    pub panel_description: Option<String>,
    pub panel_color: Option<i64>,
    pub welcome_message: Option<String>,
    pub dm_transcript: i64,
    pub next_ticket_number: i64,
    pub updated_at: i64,
}

impl TicketSettings {
    pub fn staff_roles(&self) -> Vec<u64> {
        serde_json::from_str::<Vec<String>>(&self.staff_role_ids)
            .map(|v| v.into_iter().filter_map(|id| id.parse().ok()).collect())
            .unwrap_or_default()
    }

    pub fn panel_channel(&self) -> Option<u64> {
        self.panel_channel_id
            .as_deref()
            .and_then(|v| v.parse().ok())
    }

    pub fn category(&self) -> Option<u64> {
        self.category_id.as_deref().and_then(|v| v.parse().ok())
    }

    pub fn log_channel(&self) -> Option<u64> {
        self.log_channel_id.as_deref().and_then(|v| v.parse().ok())
    }
    pub fn panel_title(&self) -> &str {
        self.panel_title
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_PANEL_TITLE)
    }
    pub fn panel_description(&self) -> &str {
        self.panel_description
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_PANEL_DESCRIPTION)
    }
    pub fn panel_color(&self) -> u32 {
        self.panel_color.unwrap_or(DEFAULT_COLOR).max(0) as u32
    }
    pub fn welcome_message(&self) -> &str {
        self.welcome_message
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_WELCOME)
    }
    pub fn dm_transcript(&self) -> bool {
        self.dm_transcript != 0
    }
}

/// In-memory defaults matching JS `store.getSettings` when no row exists.
pub fn empty_settings(guild_id: u64) -> TicketSettings {
    TicketSettings {
        guild_id: guild_id.to_string(),
        panel_channel_id: None,
        panel_message_id: None,
        category_id: None,
        log_channel_id: None,
        staff_role_ids: "[]".to_string(),
        max_open_per_user: 1,
        panel_title: None,
        panel_description: None,
        panel_color: None,
        welcome_message: None,
        dm_transcript: 1,
        next_ticket_number: 1,
        updated_at: 0,
    }
}

pub async fn settings(db: &Db, guild_id: u64) -> sqlx::Result<TicketSettings> {
    let row =
        sqlx::query_as::<_, TicketSettings>("SELECT * FROM ticket_settings WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_optional(&db.pool)
            .await?;

    Ok(row.unwrap_or_else(|| empty_settings(guild_id)))
}

async fn upsert(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO ticket_settings (guild_id, updated_at) VALUES (?, ?) ON CONFLICT(guild_id) DO NOTHING")
        .bind(guild_id.to_string())
        .bind(now())
        .execute(&db.pool)
        .await?;
    Ok(())
}

async fn set_field(db: &Db, guild_id: u64, col: &str, val: &str) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    // Column comes from a closed match — never caller input.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE ticket_settings SET {col} = ?, updated_at = ? WHERE guild_id = ?"
    )))
    .bind(val)
    .bind(now())
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_panel_channel(db: &Db, guild_id: u64, channel_id: u64) -> sqlx::Result<()> {
    set_field(db, guild_id, "panel_channel_id", &channel_id.to_string()).await
}

pub async fn set_panel_message(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    sqlx::query(
        "UPDATE ticket_settings SET panel_channel_id = ?, panel_message_id = ?, updated_at = ? WHERE guild_id = ?",
    )
    .bind(channel_id.to_string())
    .bind(message_id.to_string())
    .bind(now())
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Forget the last posted panel message (JS clears `panelMessageId` when the
/// panel channel changes so a fresh panel is posted in the new channel).
pub async fn clear_panel_message(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    set_field(db, guild_id, "panel_message_id", "").await
}

pub async fn set_category(db: &Db, guild_id: u64, category: Option<u64>) -> sqlx::Result<()> {
    match category {
        Some(id) => set_field(db, guild_id, "category_id", &id.to_string()).await,
        None => set_field(db, guild_id, "category_id", "").await,
    }
}

pub async fn set_log_channel(db: &Db, guild_id: u64, channel_id: Option<u64>) -> sqlx::Result<()> {
    match channel_id {
        Some(id) => set_field(db, guild_id, "log_channel_id", &id.to_string()).await,
        None => set_field(db, guild_id, "log_channel_id", "").await,
    }
}

pub async fn set_staff_roles(db: &Db, guild_id: u64, roles: &[u64]) -> sqlx::Result<()> {
    let encoded = serde_json::to_string(&roles.iter().map(|id| id.to_string()).collect::<Vec<_>>())
        .unwrap_or_else(|_| "[]".to_string());
    set_field(db, guild_id, "staff_role_ids", &encoded).await
}

pub async fn set_max_open(db: &Db, guild_id: u64, max: u64) -> sqlx::Result<()> {
    set_field(db, guild_id, "max_open_per_user", &max.to_string()).await
}

/// Textual panel/welcome settings (JS `store.setField` whitelist).
pub async fn set_panel_title(db: &Db, guild_id: u64, value: &str) -> sqlx::Result<()> {
    set_field(db, guild_id, "panel_title", value).await
}
pub async fn set_panel_description(db: &Db, guild_id: u64, value: &str) -> sqlx::Result<()> {
    set_field(db, guild_id, "panel_description", value).await
}
pub async fn set_panel_color(db: &Db, guild_id: u64, value: i64) -> sqlx::Result<()> {
    set_field(db, guild_id, "panel_color", &value.to_string()).await
}
pub async fn set_welcome_message(db: &Db, guild_id: u64, value: &str) -> sqlx::Result<()> {
    set_field(db, guild_id, "welcome_message", value).await
}
pub async fn set_dm_transcript(db: &Db, guild_id: u64, enabled: bool) -> sqlx::Result<()> {
    set_field(
        db,
        guild_id,
        "dm_transcript",
        if enabled { "1" } else { "0" },
    )
    .await
}

/// Atomically hand out the next per-guild ticket number. JS
/// `store.reserveNumber` uses a transaction; a single `UPDATE … RETURNING`
/// gives the same single-winner semantics without a read/modify/write race
/// between two simultaneous ticket opens.
pub async fn reserve_number(db: &Db, guild_id: u64) -> sqlx::Result<u64> {
    upsert(db, guild_id).await?;
    let n: i64 = sqlx::query_scalar(
        "UPDATE ticket_settings SET next_ticket_number = next_ticket_number + 1
         WHERE guild_id = ? RETURNING next_ticket_number - 1",
    )
    .bind(guild_id.to_string())
    .fetch_one(&db.pool)
    .await?;

    Ok(n.max(1) as u64)
}

#[derive(Debug, Clone, FromRow)]
pub struct Ticket {
    pub id: i64,
    pub guild_id: String,
    pub number: i64,
    pub channel_id: String,
    pub opener_id: String,
    pub subject: Option<String>,
    pub details: Option<String>,
    pub claimed_by: Option<String>,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
    pub closed_by: Option<String>,
}

impl Ticket {
    pub fn channel_id(&self) -> Option<u64> {
        self.channel_id.parse().ok()
    }
    pub fn opener_id(&self) -> Option<u64> {
        self.opener_id.parse().ok()
    }
    pub fn claimed_by(&self) -> Option<u64> {
        self.claimed_by.as_deref().and_then(|v| v.parse().ok())
    }
    pub fn is_open(&self) -> bool {
        self.status.eq_ignore_ascii_case("open")
    }
}

pub async fn insert(
    db: &Db,
    guild_id: u64,
    number: u64,
    channel_id: u64,
    opener_id: u64,
    subject: Option<&str>,
    details: Option<&str>,
) -> sqlx::Result<i64> {
    let result = sqlx::query(
        "INSERT INTO tickets (guild_id, number, channel_id, opener_id, subject, details) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(guild_id.to_string())
    .bind(number as i64)
    .bind(channel_id.to_string())
    .bind(opener_id.to_string())
    .bind(subject.map(str::trim))
    .bind(details.filter(|value| !value.is_empty()))
    .execute(&db.pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn open_for_guild(db: &Db, guild_id: u64) -> sqlx::Result<Vec<Ticket>> {
    sqlx::query_as::<_, Ticket>(
        "SELECT * FROM tickets WHERE guild_id = ? AND status = 'open' ORDER BY id",
    )
    .bind(guild_id.to_string())
    .fetch_all(&db.pool)
    .await
}

pub async fn open_by_user(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<Vec<Ticket>> {
    sqlx::query_as::<_, Ticket>(
        "SELECT * FROM tickets WHERE guild_id = ? AND opener_id = ? AND status = 'open' ORDER BY id",
    )
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .fetch_all(&db.pool)
    .await
}

pub async fn get_by_channel(db: &Db, channel_id: u64) -> sqlx::Result<Option<Ticket>> {
    sqlx::query_as::<_, Ticket>("SELECT * FROM tickets WHERE channel_id = ?")
        .bind(channel_id.to_string())
        .fetch_optional(&db.pool)
        .await
}

/// open -> closed. Returns true only if this call flipped it.
pub async fn mark_closed(db: &Db, channel_id: u64, closed_by: Option<u64>) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE tickets SET status = 'closed', closed_at = datetime('now'), closed_by = ? WHERE channel_id = ? AND status = 'open'",
    )
    .bind(closed_by.map(|id| id.to_string()))
    .bind(channel_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// JS `store.reopen`: restore a ticket after a failed transcript/log step.
/// `claimed_by` is intentionally left untouched, matching JS.
pub async fn reopen(db: &Db, channel_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE tickets SET status = 'open', closed_at = NULL, closed_by = NULL WHERE channel_id = ?",
    )
    .bind(channel_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_claimed(db: &Db, channel_id: u64, user: Option<u64>) -> sqlx::Result<()> {
    sqlx::query("UPDATE tickets SET claimed_by = ? WHERE channel_id = ?")
        .bind(user.map(|id| id.to_string()))
        .bind(channel_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}
