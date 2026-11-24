//! Bear Trap / custom notification storage.
//!
//! Shares the legacy tables (`bear_notifications`, `notification_history`)
//! with the Node bot so both runtimes see the same rows. Timestamps stay in
//! ISO-8601 UTC format, matching luxon's `toISO()` output.

use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use sqlx::FromRow;

use super::Db;

/// Minutes before the event time at which each notification type fires.
pub fn slots(notification_type: i64) -> &'static [i64] {
    match notification_type {
        1 => &[30, 10, 5, 0],
        2 => &[10, 5, 0],
        3 => &[5, 0],
        4 => &[5],
        _ => &[0],
    }
}

/// Resolve lead-time offsets, including the legacy JS CUSTOM_TIMES prefix.
pub fn schedule_slots(notification_type: i64, description: &str) -> Vec<i64> {
    if notification_type == 6 {
        if let Some(raw) = description
            .strip_prefix("CUSTOM_TIMES:")
            .and_then(|value| value.split_once('|').map(|(times, _)| times))
        {
            // JS accepts both dash- and comma-separated custom lead times.
            let parsed = raw
                .split(['-', ','])
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::parse::<i64>)
                .collect::<Result<Vec<_>, _>>();
            if let Ok(values) = parsed {
                if !values.is_empty()
                    && values.iter().all(|value| *value >= 0)
                    && values.windows(2).all(|pair| pair[0] > pair[1])
                {
                    return values;
                }
            }
        }
    }
    slots(notification_type).to_vec()
}

pub fn parse_iso(raw: &str) -> Option<DateTime<Utc>> {
    // luxon emits milliseconds + Z; chrono's parser accepts both.
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                .map(|n| n.and_utc())
                .ok()
        })
}

pub fn to_iso(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[derive(Debug, Clone, FromRow)]
pub struct Notification {
    pub id: i64,
    pub guild_id: String,
    pub channel_id: String,
    pub hour: i64,
    pub minute: i64,
    pub timezone: String,
    pub description: String,
    pub notification_type: i64,
    pub mention_type: String,
    pub repeat_enabled: i64,
    pub repeat_minutes: Option<i64>,
    pub is_enabled: i64,
    pub last_notification: Option<String>,
    pub next_notification: Option<String>,
    pub event_type: Option<String>,
    pub instance_identifier: Option<String>,
    pub custom_delete_delay_minutes: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
pub struct GuildSettings {
    pub delete_enabled: bool,
    pub default_delete_delay_minutes: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventTemplate {
    pub template_id: i64,
    pub event_type: String,
    pub embed_title: Option<String>,
    pub embed_description: Option<String>,
    pub embed_color: Option<String>,
    pub footer: Option<String>,
    pub author: Option<String>,
    pub mention_message: Option<String>,
    pub embed_image_url: Option<String>,
    pub embed_thumbnail_url: Option<String>,
    pub embed_images: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct NotificationEmbed {
    pub notification_id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<i64>,
    pub image_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub footer: Option<String>,
    pub author: Option<String>,
    pub mention_message: Option<String>,
    pub images: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum NotificationEmbedField {
    ImageUrl,
    ThumbnailUrl,
    Footer,
    Author,
    MentionMessage,
    Images,
}

pub async fn notification_embed(
    db: &Db,
    notification_id: i64,
) -> sqlx::Result<Option<NotificationEmbed>> {
    sqlx::query_as::<_, NotificationEmbed>(
        "SELECT notification_id, title, description, color, image_url, thumbnail_url, footer, author, mention_message, images FROM bear_notification_embeds WHERE notification_id = ?",
    )
    .bind(notification_id)
    .fetch_optional(&db.pool)
    .await
}

pub async fn save_notification_embed_basic(
    db: &Db,
    notification_id: i64,
    title: Option<&str>,
    description: Option<&str>,
    color: Option<i64>,
) -> sqlx::Result<()> {
    if notification_embed(db, notification_id).await?.is_some() {
        sqlx::query("UPDATE bear_notification_embeds SET title = ?, description = ?, color = ? WHERE notification_id = ?")
            .bind(title).bind(description).bind(color).bind(notification_id).execute(&db.pool).await?;
    } else {
        sqlx::query("INSERT INTO bear_notification_embeds (notification_id, title, description, color) VALUES (?, ?, ?, ?)")
            .bind(notification_id).bind(title).bind(description).bind(color).execute(&db.pool).await?;
    }
    Ok(())
}

pub async fn set_notification_embed_field(
    db: &Db,
    notification_id: i64,
    field: NotificationEmbedField,
    value: Option<&str>,
) -> sqlx::Result<()> {
    let (update, insert) = match field {
        NotificationEmbedField::ImageUrl => (
            "UPDATE bear_notification_embeds SET image_url = ? WHERE notification_id = ?",
            "INSERT INTO bear_notification_embeds (notification_id, image_url) VALUES (?, ?)",
        ),
        NotificationEmbedField::ThumbnailUrl => (
            "UPDATE bear_notification_embeds SET thumbnail_url = ? WHERE notification_id = ?",
            "INSERT INTO bear_notification_embeds (notification_id, thumbnail_url) VALUES (?, ?)",
        ),
        NotificationEmbedField::Footer => (
            "UPDATE bear_notification_embeds SET footer = ? WHERE notification_id = ?",
            "INSERT INTO bear_notification_embeds (notification_id, footer) VALUES (?, ?)",
        ),
        NotificationEmbedField::Author => (
            "UPDATE bear_notification_embeds SET author = ? WHERE notification_id = ?",
            "INSERT INTO bear_notification_embeds (notification_id, author) VALUES (?, ?)",
        ),
        NotificationEmbedField::MentionMessage => (
            "UPDATE bear_notification_embeds SET mention_message = ? WHERE notification_id = ?",
            "INSERT INTO bear_notification_embeds (notification_id, mention_message) VALUES (?, ?)",
        ),
        NotificationEmbedField::Images => (
            "UPDATE bear_notification_embeds SET images = ? WHERE notification_id = ?",
            "INSERT INTO bear_notification_embeds (notification_id, images) VALUES (?, ?)",
        ),
    };
    if notification_embed(db, notification_id).await?.is_some() {
        sqlx::query(update)
            .bind(value)
            .bind(notification_id)
            .execute(&db.pool)
            .await?;
    } else {
        sqlx::query(insert)
            .bind(notification_id)
            .bind(value)
            .execute(&db.pool)
            .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum EventTemplateField {
    Color,
    Footer,
    Author,
    MentionMessage,
    ImageUrl,
    ThumbnailUrl,
    Images,
}

pub async fn event_template(db: &Db, event: &str) -> sqlx::Result<Option<EventTemplate>> {
    sqlx::query_as::<_, EventTemplate>(
        "SELECT template_id, event_type, embed_title, embed_description, embed_color, footer, author, mention_message, embed_image_url, embed_thumbnail_url, embed_images
         FROM notification_templates WHERE event_type = ? AND is_global = 1 ORDER BY template_id DESC LIMIT 1",
    )
    .bind(event)
    .fetch_optional(&db.pool)
    .await
}

pub async fn set_event_template_field(
    db: &Db,
    event: &str,
    field: EventTemplateField,
    value: Option<&str>,
) -> sqlx::Result<()> {
    let template = event_template(db, event).await?;
    if let Some(template) = template {
        let (column, sql) = match field {
            EventTemplateField::Color => (
                "embed_color",
                "UPDATE notification_templates SET embed_color = ? WHERE template_id = ?",
            ),
            EventTemplateField::Footer => (
                "footer",
                "UPDATE notification_templates SET footer = ? WHERE template_id = ?",
            ),
            EventTemplateField::Author => (
                "author",
                "UPDATE notification_templates SET author = ? WHERE template_id = ?",
            ),
            EventTemplateField::MentionMessage => (
                "mention_message",
                "UPDATE notification_templates SET mention_message = ? WHERE template_id = ?",
            ),
            EventTemplateField::ImageUrl => (
                "embed_image_url",
                "UPDATE notification_templates SET embed_image_url = ? WHERE template_id = ?",
            ),
            EventTemplateField::ThumbnailUrl => (
                "embed_thumbnail_url",
                "UPDATE notification_templates SET embed_thumbnail_url = ? WHERE template_id = ?",
            ),
            EventTemplateField::Images => (
                "embed_images",
                "UPDATE notification_templates SET embed_images = ? WHERE template_id = ?",
            ),
        };
        let _ = column;
        sqlx::query(sql)
            .bind(value)
            .bind(template.template_id)
            .execute(&db.pool)
            .await?;
    } else {
        let (column, sql) = match field {
            EventTemplateField::Color => ("embed_color", "INSERT INTO notification_templates (template_name, event_type, is_global, embed_color) VALUES (?, ?, 1, ?)"),
            EventTemplateField::Footer => ("footer", "INSERT INTO notification_templates (template_name, event_type, is_global, footer) VALUES (?, ?, 1, ?)"),
            EventTemplateField::Author => ("author", "INSERT INTO notification_templates (template_name, event_type, is_global, author) VALUES (?, ?, 1, ?)"),
            EventTemplateField::MentionMessage => ("mention_message", "INSERT INTO notification_templates (template_name, event_type, is_global, mention_message) VALUES (?, ?, 1, ?)"),
            EventTemplateField::ImageUrl => ("embed_image_url", "INSERT INTO notification_templates (template_name, event_type, is_global, embed_image_url) VALUES (?, ?, 1, ?)"),
            EventTemplateField::ThumbnailUrl => ("embed_thumbnail_url", "INSERT INTO notification_templates (template_name, event_type, is_global, embed_thumbnail_url) VALUES (?, ?, 1, ?)"),
            EventTemplateField::Images => ("embed_images", "INSERT INTO notification_templates (template_name, event_type, is_global, embed_images) VALUES (?, ?, 1, ?)"),
        };
        let _ = column;
        sqlx::query(sql)
            .bind(event)
            .bind(event)
            .bind(value)
            .execute(&db.pool)
            .await?;
    }
    Ok(())
}

pub async fn save_event_template(
    db: &Db,
    event: &str,
    title: Option<&str>,
    description: Option<&str>,
) -> sqlx::Result<()> {
    if let Some(template) = event_template(db, event).await? {
        sqlx::query("UPDATE notification_templates SET embed_title = ?, embed_description = ? WHERE template_id = ?")
            .bind(title).bind(description).bind(template.template_id).execute(&db.pool).await?;
    } else {
        sqlx::query("INSERT INTO notification_templates (template_name, event_type, embed_title, embed_description, is_global) VALUES (?, ?, ?, ?, 1)")
            .bind(event).bind(event).bind(title).bind(description).execute(&db.pool).await?;
    }
    Ok(())
}

pub async fn reset_event_template(db: &Db, event: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM notification_templates WHERE event_type = ? AND is_global = 1")
        .bind(event)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn guild_settings(db: &Db, guild_id: u64) -> sqlx::Result<GuildSettings> {
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT delete_messages_enabled, default_delete_delay_minutes FROM bear_trap_settings WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    if let Some((enabled, delay)) = row {
        return Ok(GuildSettings {
            delete_enabled: enabled != 0,
            default_delete_delay_minutes: delay.max(1),
        });
    }
    sqlx::query("INSERT OR IGNORE INTO bear_trap_settings (guild_id) VALUES (?)")
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(GuildSettings {
        delete_enabled: true,
        default_delete_delay_minutes: 60,
    })
}

pub async fn set_auto_delete(db: &Db, guild_id: u64, enabled: bool) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO bear_trap_settings (guild_id, delete_messages_enabled) VALUES (?, ?) ON CONFLICT(guild_id) DO UPDATE SET delete_messages_enabled = excluded.delete_messages_enabled")
        .bind(guild_id.to_string())
        .bind(i64::from(enabled))
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_default_delete_delay(db: &Db, guild_id: u64, minutes: i64) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO bear_trap_settings (guild_id, default_delete_delay_minutes) VALUES (?, ?) ON CONFLICT(guild_id) DO UPDATE SET default_delete_delay_minutes = excluded.default_delete_delay_minutes")
        .bind(guild_id.to_string())
        .bind(minutes.clamp(1, 43_200))
        .execute(&db.pool)
        .await?;
    Ok(())
}

impl Notification {
    pub fn enabled(&self) -> bool {
        self.is_enabled != 0
    }

    pub fn channel(&self) -> Option<u64> {
        self.channel_id.parse().ok()
    }

    pub fn tz(&self) -> chrono_tz::Tz {
        self.timezone
            .parse::<chrono_tz::Tz>()
            .unwrap_or(chrono_tz::Tz::UTC)
    }

    pub fn mention_prefix(&self) -> &'static str {
        match self.mention_type.as_str() {
            "here" => "@here ",
            "everyone" => "@everyone ",
            _ => "",
        }
    }

    pub fn mention_text(&self) -> String {
        match self.mention_type.as_str() {
            "here" => "@here ".to_string(),
            "everyone" => "@everyone ".to_string(),
            raw if raw.starts_with("role_") => raw
                .strip_prefix("role_")
                .map(|id| format!("<@&{id}> "))
                .unwrap_or_default(),
            raw if raw.starts_with("member_") => raw
                .strip_prefix("member_")
                .map(|id| format!("<@{id}> "))
                .unwrap_or_default(),
            _ => String::new(),
        }
    }
}

pub async fn list_for_guild(db: &Db, guild_id: u64) -> sqlx::Result<Vec<Notification>> {
    sqlx::query_as::<_, Notification>(
        "SELECT id, guild_id, channel_id, hour, minute, timezone, description,
                notification_type, mention_type, repeat_enabled, repeat_minutes, is_enabled,
                last_notification, next_notification, event_type, instance_identifier,
                custom_delete_delay_minutes
         FROM bear_notifications WHERE guild_id = ? ORDER BY id",
    )
    .bind(guild_id.to_string())
    .fetch_all(&db.pool)
    .await
}

pub struct NewNotification<'a> {
    pub channel_id: u64,
    pub hour: i64,
    pub minute: i64,
    pub timezone: &'a str,
    pub description: &'a str,
    pub notification_type: i64,
    pub mention_type: &'a str,
    pub repeat_enabled: bool,
    pub repeat_minutes: i64,
    pub created_by: u64,
    pub custom_delete_delay_minutes: Option<i64>,
    pub event_type: Option<&'a str>,
    pub wizard_batch_id: Option<&'a str>,
    pub instance_identifier: Option<&'a str>,
    pub selected_weekdays: &'a [u32],
    pub start_at: Option<DateTime<Utc>>,
}

pub async fn create(db: &Db, guild_id: u64, input: NewNotification<'_>) -> sqlx::Result<i64> {
    let tz = input
        .timezone
        .parse::<chrono_tz::Tz>()
        .unwrap_or(chrono_tz::Tz::UTC);
    let now = Utc::now();
    let schedule_slots = schedule_slots(input.notification_type, input.description);
    let next = if let Some(start_at) = input.start_at {
        let local_date = start_at.with_timezone(&tz).date_naive();
        next_slot_on_date(
            &schedule_slots,
            input.hour,
            input.minute,
            tz,
            local_date,
            now,
        )
    } else if input.repeat_minutes == -1 && !input.selected_weekdays.is_empty() {
        next_weekday_slot(
            &schedule_slots,
            input.hour,
            input.minute,
            tz,
            now,
            input.selected_weekdays,
        )
    } else {
        next_slot(&schedule_slots, input.hour, input.minute, tz, now)
    };
    let row = sqlx::query(
        "INSERT INTO bear_notifications
         (guild_id, channel_id, hour, minute, timezone, description, notification_type,
          mention_type, repeat_enabled, repeat_minutes, is_enabled, created_by, next_notification,
          custom_delete_delay_minutes, event_type, wizard_batch_id, instance_identifier)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(guild_id.to_string())
    .bind(input.channel_id.to_string())
    .bind(input.hour)
    .bind(input.minute)
    .bind(tz.name())
    .bind(input.description)
    .bind(input.notification_type)
    .bind(input.mention_type)
    .bind(if input.repeat_enabled { 1 } else { 0 })
    .bind(input.repeat_minutes)
    .bind(input.created_by.to_string())
    .bind(next.map(to_iso))
    .bind(input.custom_delete_delay_minutes)
    .bind(input.event_type)
    .bind(input.wizard_batch_id)
    .bind(input.instance_identifier)
    .fetch_one(&db.pool)
    .await?;
    use sqlx::Row;
    let id = row.get(0);
    if input.repeat_minutes == -1 && !input.selected_weekdays.is_empty() {
        set_weekdays(db, id, input.selected_weekdays).await?;
    }
    if let (Some(event_type), Some(wizard_run_id)) = (input.event_type, input.wizard_batch_id) {
        sqlx::query(
            "INSERT OR REPLACE INTO wizard_notifications
             (notification_id, guild_id, event_type, created_by_wizard, wizard_run_id)
             VALUES (?, ?, ?, 1, ?)",
        )
        .bind(id)
        .bind(guild_id.to_string())
        .bind(event_type)
        .bind(wizard_run_id)
        .execute(&db.pool)
        .await?;
    }
    Ok(id)
}

pub async fn weekdays(db: &Db, notification_id: i64) -> sqlx::Result<Vec<u32>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT weekday FROM notification_days WHERE notification_id = ? ORDER BY rowid DESC LIMIT 1",
    )
    .bind(notification_id)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row
        .map(|(value,)| {
            value
                .split('|')
                .filter_map(|part| part.parse::<u32>().ok())
                .filter(|day| *day <= 6)
                .collect()
        })
        .unwrap_or_default())
}

async fn set_weekdays(db: &Db, notification_id: i64, days: &[u32]) -> sqlx::Result<()> {
    let mut days = days
        .iter()
        .copied()
        .filter(|day| *day <= 6)
        .collect::<Vec<_>>();
    days.sort_unstable();
    days.dedup();
    sqlx::query("DELETE FROM notification_days WHERE notification_id = ?")
        .bind(notification_id)
        .execute(&db.pool)
        .await?;
    if !days.is_empty() {
        let encoded = days
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join("|");
        sqlx::query("INSERT INTO notification_days (notification_id, weekday) VALUES (?, ?)")
            .bind(notification_id)
            .bind(encoded)
            .execute(&db.pool)
            .await?;
    }
    Ok(())
}

pub async fn update_repeat(
    db: &Db,
    notification: &Notification,
    repeat_minutes: i64,
    selected_weekdays: &[u32],
) -> sqlx::Result<()> {
    let now = Utc::now();
    let tz = notification.tz();
    let schedule_slots = schedule_slots(notification.notification_type, &notification.description);
    let next = if repeat_minutes == -1 && !selected_weekdays.is_empty() {
        next_weekday_slot(
            &schedule_slots,
            notification.hour,
            notification.minute,
            tz,
            now,
            selected_weekdays,
        )
    } else if repeat_minutes > 0 {
        Some(now + chrono::Duration::minutes(repeat_minutes))
    } else {
        next_slot(
            &schedule_slots,
            notification.hour,
            notification.minute,
            tz,
            now,
        )
    };
    let enabled = repeat_minutes != 0 && (repeat_minutes != -1 || !selected_weekdays.is_empty());
    sqlx::query("UPDATE bear_notifications SET repeat_enabled = ?, repeat_minutes = ?, next_notification = ? WHERE id = ?")
        .bind(i64::from(enabled))
        .bind(repeat_minutes)
        .bind(next.map(to_iso))
        .bind(notification.id)
        .execute(&db.pool)
        .await?;
    if repeat_minutes == -1 {
        set_weekdays(db, notification.id, selected_weekdays).await?;
    } else {
        sqlx::query("DELETE FROM notification_days WHERE notification_id = ?")
            .bind(notification.id)
            .execute(&db.pool)
            .await?;
    }
    Ok(())
}

/// Next send instant at or after `from`, given the event's wall-clock time
/// (`hour:minute` in `tz`) and the set of minute-before slots.
pub fn next_slot(
    slot_minutes: &[i64],
    hour: i64,
    minute: i64,
    tz: chrono_tz::Tz,
    from: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    use chrono::{Duration, TimeZone};

    // A custom lead time may be several days long. Search far enough forward
    // to find the first future send for the next event occurrence.
    let horizon_days = slot_minutes
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .div_euclid(1440)
        .saturating_add(2)
        .clamp(1, 10_000);
    for day_offset in 0..=horizon_days {
        let today = from.with_timezone(&tz).date_naive() + Duration::days(day_offset);
        let Some(local) = today.and_hms_opt(hour as u32, minute as u32, 0) else {
            continue;
        };
        let event_tz = match tz.from_local_datetime(&local) {
            chrono::LocalResult::Single(dt) => dt,
            chrono::LocalResult::Ambiguous(_, second) => second,
            chrono::LocalResult::None => continue, // nonexistent local time (DST gap) — skip today
        };
        let event_utc = event_tz.with_timezone(&Utc);

        for slot in slot_minutes {
            let send_at = event_utc - Duration::minutes(*slot);
            if send_at >= from {
                return Some(send_at);
            }
        }
    }
    None
}

pub fn next_slot_on_date(
    slot_minutes: &[i64],
    hour: i64,
    minute: i64,
    tz: chrono_tz::Tz,
    date: chrono::NaiveDate,
    from: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    use chrono::TimeZone;
    let local = date.and_hms_opt(hour as u32, minute as u32, 0)?;
    let event = match tz.from_local_datetime(&local) {
        chrono::LocalResult::Single(dt) => dt,
        chrono::LocalResult::Ambiguous(_, second) => second,
        chrono::LocalResult::None => return None,
    }
    .with_timezone(&Utc);
    slot_minutes
        .iter()
        .map(|slot| event - chrono::Duration::minutes(*slot))
        .find(|send_at| *send_at >= from)
}

pub fn next_weekday_slot(
    slot_minutes: &[i64],
    hour: i64,
    minute: i64,
    tz: chrono_tz::Tz,
    from: DateTime<Utc>,
    weekdays: &[u32],
) -> Option<DateTime<Utc>> {
    use chrono::{Duration, TimeZone};
    let horizon_days = slot_minutes
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .div_euclid(1440)
        .saturating_add(14)
        .clamp(14, 10_000);
    for day_offset in 0..=horizon_days {
        let date = from.with_timezone(&tz).date_naive() + Duration::days(day_offset);
        if !weekdays.contains(&date.weekday().num_days_from_monday()) {
            continue;
        }
        let local = date.and_hms_opt(hour as u32, minute as u32, 0)?;
        let event = match tz.from_local_datetime(&local) {
            chrono::LocalResult::Single(dt) => dt,
            chrono::LocalResult::Ambiguous(_, second) => second,
            chrono::LocalResult::None => continue,
        }
        .with_timezone(&Utc);
        for slot in slot_minutes {
            let send_at = event - Duration::minutes(*slot);
            if send_at >= from {
                return Some(send_at);
            }
        }
    }
    None
}

pub async fn count(db: &Db, guild_id: u64) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bear_notifications WHERE guild_id = ?")
        .bind(guild_id.to_string())
        .fetch_one(&db.pool)
        .await?;
    Ok(row.0)
}

pub async fn set_enabled(db: &Db, id: i64, enabled: bool) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE bear_notifications SET is_enabled = ?, auto_disabled_at = NULL WHERE id = ?",
    )
    .bind(if enabled { 1 } else { 0 })
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn auto_disable(db: &Db, id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE bear_notifications SET is_enabled = 0, auto_disabled_at = ? WHERE id = ?")
        .bind(to_iso(chrono::Utc::now()))
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct QuarantineRow {
    pub id: i64,
    pub guild_id: String,
    pub channel_id: String,
    pub description: String,
}

pub async fn auto_disabled_rows(
    db: &Db,
    guild_id: &str,
    timestamp: &str,
) -> sqlx::Result<Vec<QuarantineRow>> {
    let rows = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT id, CAST(guild_id AS TEXT), CAST(channel_id AS TEXT), description
         FROM bear_notifications WHERE guild_id = ? AND auto_disabled_at = ? AND is_enabled = 0
         ORDER BY id",
    )
    .bind(guild_id)
    .bind(timestamp)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, guild_id, channel_id, description)| QuarantineRow {
            id,
            guild_id,
            channel_id,
            description,
        })
        .collect())
}

pub async fn auto_disabled_timestamp(db: &Db, id: i64) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT auto_disabled_at FROM bear_notifications WHERE id = ? AND is_enabled = 0",
    )
    .bind(id)
    .fetch_optional(&db.pool)
    .await
}

pub async fn quarantine_enabled_channel(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
) -> sqlx::Result<Option<String>> {
    let timestamp = to_iso(Utc::now());
    let result = sqlx::query(
        "UPDATE bear_notifications SET is_enabled = 0, auto_disabled_at = ?
         WHERE guild_id = ? AND channel_id = ? AND is_enabled = 1",
    )
    .bind(&timestamp)
    .bind(guild_id.to_string())
    .bind(channel_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok((result.rows_affected() > 0).then_some(timestamp))
}

pub async fn quarantine_enabled_guild(db: &Db, guild_id: u64) -> sqlx::Result<Option<String>> {
    let timestamp = to_iso(Utc::now());
    let result = sqlx::query(
        "UPDATE bear_notifications SET is_enabled = 0, auto_disabled_at = ?
         WHERE guild_id = ? AND is_enabled = 1",
    )
    .bind(&timestamp)
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok((result.rows_affected() > 0).then_some(timestamp))
}

pub async fn resolve_quarantine(
    db: &Db,
    guild_id: &str,
    timestamp: &str,
    delete: bool,
) -> sqlx::Result<u64> {
    let query = if delete {
        "DELETE FROM bear_notifications WHERE guild_id = ? AND auto_disabled_at = ? AND is_enabled = 0"
    } else {
        "UPDATE bear_notifications SET is_enabled = 1, auto_disabled_at = NULL
         WHERE guild_id = ? AND auto_disabled_at = ? AND is_enabled = 0"
    };
    Ok(sqlx::query(query)
        .bind(guild_id)
        .bind(timestamp)
        .execute(&db.pool)
        .await?
        .rows_affected())
}

pub async fn enabled_guild_ids(db: &Db) -> sqlx::Result<Vec<u64>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT CAST(guild_id AS TEXT) FROM bear_notifications WHERE is_enabled = 1",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id,)| id.parse::<u64>().ok())
        .collect())
}

pub async fn delete(db: &Db, id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM bear_notifications WHERE id = ?")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Rows whose next send time has arrived. JS `engine.js` requires
/// `next_notification IS NOT NULL`; rows without one are inert.
pub async fn due(db: &Db, now: DateTime<Utc>) -> sqlx::Result<Vec<Notification>> {
    sqlx::query_as::<_, Notification>(
        "SELECT id, guild_id, channel_id, hour, minute, timezone, description,
                notification_type, mention_type, repeat_enabled, repeat_minutes, is_enabled,
                last_notification, next_notification, event_type, instance_identifier,
                custom_delete_delay_minutes
         FROM bear_notifications
         WHERE is_enabled = 1 AND next_notification IS NOT NULL AND next_notification <= ?",
    )
    .bind(to_iso(now))
    .fetch_all(&db.pool)
    .await
}

/// Advance `next_notification` after a successful send of `slot`
/// (minutes-before-event, 0 = final slot).
pub async fn advance(
    db: &Db,
    notification: &Notification,
    slot: i64,
    just_sent: DateTime<Utc>,
) -> sqlx::Result<()> {
    let tz = notification.tz();
    let now = Utc::now();
    let slots = schedule_slots(notification.notification_type, &notification.description);

    // If repeating on an interval, just schedule the next interval tick.
    let repeat_minutes = notification.repeat_minutes.unwrap_or(0);
    let new_next = if notification.repeat_enabled == 1 && repeat_minutes == -1 {
        let selected = weekdays(db, notification.id).await?;
        next_weekday_slot(
            &slots,
            notification.hour,
            notification.minute,
            tz,
            now,
            &selected,
        )
    } else if notification.repeat_enabled == 1 && repeat_minutes > 0 {
        Some(now + chrono::Duration::minutes(repeat_minutes))
    } else {
        next_slot(&slots, notification.hour, notification.minute, tz, now)
    };
    // One-shot: disable after the final configured slot (some presets and
    // custom JS schedules intentionally do not include the event-time slot).
    let final_slot = slots.last().copied().unwrap_or(0);
    let disable = (notification.repeat_enabled != 1 && slot == final_slot)
        || (repeat_minutes == -1 && new_next.is_none());

    if disable {
        sqlx::query(
            "UPDATE bear_notifications SET is_enabled = 0, last_notification = ? WHERE id = ?",
        )
        .bind(to_iso(just_sent))
        .bind(notification.id)
        .execute(&db.pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE bear_notifications SET last_notification = ?, next_notification = ? WHERE id = ?",
        )
        .bind(to_iso(just_sent))
        .bind(new_next.map(to_iso))
        .bind(notification.id)
        .execute(&db.pool)
        .await?;
    }
    Ok(())
}

pub async fn record_history(
    db: &Db,
    notification_id: i64,
    scheduled_at: DateTime<Utc>,
    message_id: Option<u64>,
    channel_id: Option<u64>,
    delete_delay_minutes: Option<i64>,
) -> sqlx::Result<()> {
    let scheduled =
        delete_delay_minutes.map(|mins| to_iso(scheduled_at + chrono::Duration::minutes(mins)));
    let sent_at = to_iso(chrono::Utc::now());
    sqlx::query(
        "INSERT INTO notification_history
         (notification_id, notification_time, sent_at, message_id, channel_id, scheduled_delete_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(notification_id)
    .bind(scheduled_at.timestamp())
    .bind(sent_at)
    .bind(message_id.map(|id| id.to_string()))
    .bind(channel_id.map(|id| id.to_string()))
    .bind(scheduled)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Messages due for deletion (scheduled_delete_at passed, not yet deleted).
pub async fn due_deletions(
    db: &Db,
    now: DateTime<Utc>,
) -> sqlx::Result<Vec<(i64, String, String)>> {
    // (history_id, channel_id, message_id)
    let rows: Vec<(i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, channel_id, message_id FROM notification_history
         WHERE scheduled_delete_at IS NOT NULL AND deleted_at IS NULL AND scheduled_delete_at <= ?",
    )
    .bind(to_iso(now))
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, channel_id, message_id)| Some((id, channel_id?, message_id?)))
        .collect())
}

/// Prior lead-time messages that have no scheduled deletion yet (JS
/// `deletePreviousNotifications`). With an event type and instance identifier,
/// JS also includes other enabled notifications in the same channel for that
/// event, so only the newest instance alert stays visible.
pub async fn pending_unscheduled_for_alert(
    db: &Db,
    notification_id: i64,
    channel_id: &str,
    event_type: Option<&str>,
    instance_identifier: Option<&str>,
) -> sqlx::Result<Vec<(i64, String)>> {
    let mut ids = vec![notification_id];
    if let (Some(event_type), Some(instance)) = (event_type, instance_identifier) {
        let siblings: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM bear_notifications
             WHERE channel_id = ? AND event_type = ? AND instance_identifier != ? AND is_enabled = 1",
        )
        .bind(channel_id)
        .bind(event_type)
        .bind(instance)
        .fetch_all(&db.pool)
        .await?;
        ids.extend(siblings.into_iter().map(|(id,)| id));
    }

    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, CAST(message_id AS TEXT) AS message_id FROM notification_history
         WHERE notification_id IN ({placeholders})
           AND scheduled_delete_at IS NULL AND deleted_at IS NULL AND message_id IS NOT NULL"
    );
    // Only the placeholder count is dynamic; all values are bound below.
    let mut query = sqlx::query_as::<_, (i64, Option<String>)>(sqlx::AssertSqlSafe(sql));
    for id in &ids {
        query = query.bind(*id);
    }
    let rows = query.fetch_all(&db.pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, message)| Some((id, message?)))
        .collect())
}

pub async fn mark_deleted(db: &Db, history_id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE notification_history SET deleted_at = ? WHERE id = ?")
        .bind(to_iso(chrono::Utc::now()))
        .bind(history_id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn slots_types() {
        assert_eq!(slots(1), &[30, 10, 5, 0]);
        assert_eq!(slots(2), &[10, 5, 0]);
        assert_eq!(slots(5), &[0]);
    }

    #[test]
    fn custom_times_decode_from_legacy_description_prefix() {
        assert_eq!(
            schedule_slots(6, "CUSTOM_TIMES:60-20-15-4-0|EMBED_MESSAGE:true"),
            vec![60, 20, 15, 4, 0]
        );
        assert_eq!(
            schedule_slots(6, "CUSTOM_TIMES:20-10|PLAIN_MESSAGE:hello"),
            vec![20, 10]
        );
        assert_eq!(schedule_slots(6, "CUSTOM_TIMES:10-20|bad"), vec![0]);
        assert_eq!(
            schedule_slots(2, "CUSTOM_TIMES:60-20|ignored"),
            vec![10, 5, 0]
        );
    }

    #[tokio::test]
    async fn wizard_instance_save_keeps_batch_and_instance_metadata() {
        let db = Db::memory().await.unwrap();
        let future = Utc::now() + chrono::Duration::days(1);
        let id = create(
            &db,
            123,
            NewNotification {
                channel_id: 456,
                hour: 14,
                minute: 0,
                timezone: "UTC",
                description: "Reminder",
                notification_type: 2,
                mention_type: "none",
                repeat_enabled: true,
                repeat_minutes: 20_160,
                created_by: 789,
                custom_delete_delay_minutes: None,
                event_type: Some("foundry_battle"),
                wizard_batch_id: Some("batch-test"),
                instance_identifier: Some("legion1"),
                selected_weekdays: &[],
                start_at: Some(future),
            },
        )
        .await
        .unwrap();
        let saved: (String, String) = sqlx::query_as(
            "SELECT wizard_batch_id, instance_identifier FROM bear_notifications WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(saved, ("batch-test".into(), "legion1".into()));
        let metadata: (String, String, String) = sqlx::query_as(
            "SELECT guild_id, event_type, wizard_run_id FROM wizard_notifications WHERE notification_id = ?",
        )
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            metadata,
            ("123".into(), "foundry_battle".into(), "batch-test".into())
        );
    }

    #[test]
    fn next_slot_picks_earliest_future_slot() {
        let tz = chrono_tz::Tz::UTC;
        let now = Utc.with_ymd_and_hms(2026, 11, 19, 11, 50, 0).unwrap();
        let next = next_slot(&[10, 5, 0], 12, 0, tz, now).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 11, 19, 11, 50, 0).unwrap());
    }

    #[test]
    fn next_slot_rolls_to_tomorrow_when_all_slots_passed() {
        let tz = chrono_tz::Tz::UTC;
        let now = Utc.with_ymd_and_hms(2026, 11, 19, 12, 30, 0).unwrap();
        let next = next_slot(&[10, 5, 0], 12, 0, tz, now).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 11, 20, 11, 50, 0).unwrap());
    }

    #[test]
    fn next_slot_searches_far_enough_for_multiday_custom_offsets() {
        let tz = chrono_tz::Tz::UTC;
        let now = Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).unwrap();
        let next = next_slot(&[3000], 13, 0, tz, now).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 11, 20, 11, 0, 0).unwrap());
    }

    #[test]
    fn iso_roundtrip() {
        let now = Utc::now();
        let s = to_iso(now);
        assert!(s.ends_with('Z'));
        assert!(parse_iso(&s).is_some());
    }
}
