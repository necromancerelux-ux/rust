//! Birthdays: stores a member's birth month/day and announces them daily.
//!
//! Guards against announcing the same member twice on the same day with
//! `last_announced` (YYYY-MM-DD, compared in UTC as text).

use chrono::{Datelike, Utc};
use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, FromRow)]
pub struct Birthday {
    pub guild_id: String,
    pub user_id: String,
    pub day: i64,
    pub month: i64,
    pub set_by: String,
    pub set_at: String,
    pub last_announced: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct BirthdaySettings {
    pub guild_id: String,
    pub channel_id: Option<String>,
    pub panel_message_id: Option<String>,
    pub embed_title: Option<String>,
    pub embed_description: Option<String>,
    pub embed_color: Option<i64>,
    pub avatar_mode: String,
}

#[derive(Debug, Clone, Copy)]
pub enum BirthdayField {
    Title,
    Description,
    Color,
}

impl BirthdaySettings {
    pub fn channel(&self) -> Option<u64> {
        self.channel_id.as_deref().and_then(|v| v.parse().ok())
    }
}

const DEFAULT_TITLE: &str = "🎉 Happy Birthday!";
const DEFAULT_DESCRIPTION: &str =
    "🎉 {mention} is celebrating their birthday today! Wish them well!";
const DEFAULT_COLOR: i64 = 0xF47FFF;

#[derive(Debug, Clone)]
pub struct ParsedDayMonth {
    pub day: i64,
    pub month: i64,
}

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

pub fn parse_day_month(day_raw: &str, month_raw: &str) -> Result<ParsedDayMonth, anyhow::Error> {
    let day: i64 = day_raw
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("day must be a number between 1 and 31"))?;
    if !(1..=31).contains(&day) {
        return Err(anyhow::anyhow!("day must be between 1 and 31"));
    }

    let month_str = month_raw.trim().to_ascii_lowercase();
    let month: i64 = if month_str.chars().all(|c| c.is_ascii_digit()) {
        month_str
            .parse()
            .map_err(|_| anyhow::anyhow!("month must be 1-12 or a month name"))?
    } else {
        let index = MONTHS
            .iter()
            .position(|m| month_str == *m || m.starts_with(&month_str));
        index.map(|i| (i + 1) as i64).unwrap_or(-1)
    };
    if !(1..=12).contains(&month) {
        return Err(anyhow::anyhow!(
            "month must be 1-12 or a month name (e.g. July)"
        ));
    }

    const DAYS: [i64; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if day > DAYS[(month - 1) as usize] {
        return Err(anyhow::anyhow!("{month_str} doesn't have a day {day}"));
    }
    Ok(ParsedDayMonth { day, month })
}

pub fn format_day_month(day: i64, month: i64) -> String {
    let name = MONTHS
        .get((month - 1).max(0) as usize)
        .copied()
        .unwrap_or("?");
    let mut label = name.to_string();
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    format!("{label} {day}")
}

pub async fn settings(db: &Db, guild_id: u64) -> sqlx::Result<BirthdaySettings> {
    let row =
        sqlx::query_as::<_, BirthdaySettings>("SELECT * FROM birthday_settings WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_optional(&db.pool)
            .await?;

    Ok(row.unwrap_or(BirthdaySettings {
        guild_id: guild_id.to_string(),
        channel_id: None,
        panel_message_id: None,
        embed_title: None,
        embed_description: None,
        embed_color: None,
        avatar_mode: "off".to_string(),
    }))
}

async fn upsert(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO birthday_settings (guild_id) VALUES (?) ON CONFLICT(guild_id) DO NOTHING",
    )
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_channel(
    db: &Db,
    guild_id: u64,
    channel_id: Option<u64>,
    panel_message_id: Option<u64>,
) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    sqlx::query(
        "UPDATE birthday_settings SET channel_id = ?, panel_message_id = ? WHERE guild_id = ?",
    )
    .bind(channel_id.map(|id| id.to_string()))
    .bind(panel_message_id.map(|id| id.to_string()))
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_field(
    db: &Db,
    guild_id: u64,
    field: BirthdayField,
    value: &str,
) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    match field {
        BirthdayField::Title => {
            sqlx::query("UPDATE birthday_settings SET embed_title = ? WHERE guild_id = ?")
                .bind(value)
                .bind(guild_id.to_string())
                .execute(&db.pool)
                .await?;
        }
        BirthdayField::Description => {
            sqlx::query("UPDATE birthday_settings SET embed_description = ? WHERE guild_id = ?")
                .bind(value)
                .bind(guild_id.to_string())
                .execute(&db.pool)
                .await?;
        }
        BirthdayField::Color => {
            let color =
                i64::from_str_radix(value.trim_start_matches('#'), 16).unwrap_or(DEFAULT_COLOR);
            sqlx::query("UPDATE birthday_settings SET embed_color = ? WHERE guild_id = ?")
                .bind(color)
                .bind(guild_id.to_string())
                .execute(&db.pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn set_avatar_mode(db: &Db, guild_id: u64, mode: &str) -> sqlx::Result<()> {
    if !["off", "thumbnail", "banner", "both"].contains(&mode) {
        return Err(sqlx::Error::Protocol("invalid birthday avatar mode".into()));
    }
    upsert(db, guild_id).await?;
    sqlx::query("UPDATE birthday_settings SET avatar_mode = ? WHERE guild_id = ?")
        .bind(mode)
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn render(
    settings: &BirthdaySettings,
    mention: &str,
    user: &str,
    tag: &str,
    server: &str,
    member_count: u64,
) -> (String, String, u32) {
    let title = settings
        .embed_title
        .clone()
        .unwrap_or_else(|| DEFAULT_TITLE.to_string());
    let description = settings
        .embed_description
        .clone()
        .unwrap_or_else(|| DEFAULT_DESCRIPTION.to_string());

    let render = |text: &str| {
        text.replace("{mention}", mention)
            .replace("{user}", user)
            .replace("{tag}", tag)
            .replace("{server}", server)
            .replace("{membercount}", &member_count.to_string())
    };

    (
        render(&title),
        render(&description),
        settings.embed_color.unwrap_or(DEFAULT_COLOR) as u32,
    )
}

pub async fn set(
    db: &Db,
    guild_id: u64,
    user_id: u64,
    day: i64,
    month: i64,
    set_by: u64,
) -> sqlx::Result<()> {
    let existing = get(db, guild_id, user_id).await?;
    let last = existing.and_then(|b| b.last_announced);
    sqlx::query(
        "INSERT INTO birthdays (guild_id, user_id, day, month, set_by, set_at, last_announced)
         VALUES (?, ?, ?, ?, ?, datetime('now'), ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET
             day = excluded.day,
             month = excluded.month,
             set_by = excluded.set_by,
             set_at = excluded.set_at",
    )
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .bind(day)
    .bind(month)
    .bind(set_by.to_string())
    .bind(last)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn get(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<Option<Birthday>> {
    sqlx::query_as::<_, Birthday>("SELECT * FROM birthdays WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .fetch_optional(&db.pool)
        .await
}

pub async fn delete(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM birthdays WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list(db: &Db, guild_id: u64) -> sqlx::Result<Vec<Birthday>> {
    sqlx::query_as::<_, Birthday>("SELECT * FROM birthdays WHERE guild_id = ? ORDER BY month, day")
        .bind(guild_id.to_string())
        .fetch_all(&db.pool)
        .await
}

/// Today-in-UTC birthdays not yet announced today: `(guild_id, user_id)` pairs.
pub async fn due_today(db: &Db) -> sqlx::Result<Vec<(u64, u64)>> {
    let now = Utc::now();
    let (day, month) = (now.day() as i64, now.month() as i64);
    let today = now.format("%Y-%m-%d").to_string();

    let rows = sqlx::query_as::<_, Birthday>("SELECT * FROM birthdays WHERE day = ? AND month = ?")
        .bind(day)
        .bind(month)
        .fetch_all(&db.pool)
        .await?;

    let mut due = Vec::new();
    for row in rows {
        if row.last_announced.as_deref() == Some(today.as_str()) {
            continue;
        }
        if let (Ok(guild), Ok(user)) = (row.guild_id.parse::<u64>(), row.user_id.parse::<u64>()) {
            due.push((guild, user));
        }
    }
    Ok(due)
}

pub async fn mark_announced(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<()> {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    sqlx::query("UPDATE birthdays SET last_announced = ? WHERE guild_id = ? AND user_id = ?")
        .bind(today)
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}
