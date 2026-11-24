//! Welcome / auto-role / join-DM tables. All share the legacy schema so the
//! database works unchanged from either runtime.

use sqlx::FromRow;

use super::{now, Db};

const DEFAULT_WELCOME_TITLE: &str = "👋 Welcome!";
const DEFAULT_WELCOME_DESCRIPTION: &str =
    "Welcome to **{server}**, {mention}! We're glad to have you — you're member #{membercount}.";
const DEFAULT_COLOR: i64 = 0x5865F2;

#[derive(Debug, Clone, FromRow)]
pub struct WelcomeSettings {
    pub guild_id: String,
    pub channel_id: Option<String>,
    pub embed_title: Option<String>,
    pub embed_description: Option<String>,
    pub embed_color: Option<i64>,
    pub avatar_mode: String,
}

impl WelcomeSettings {
    /// Render the stored template with the legacy member token set:
    /// `{mention}`, `{user}`, `{tag}`, `{server}`, `{membercount}`.
    pub fn render(
        &self,
        member_mention: &str,
        display_name: &str,
        username: &str,
        server: &str,
        member_count: u64,
    ) -> WelcomeEmbed {
        let title = self
            .embed_title
            .clone()
            .unwrap_or_else(|| DEFAULT_WELCOME_TITLE.to_string());
        let description = self
            .embed_description
            .clone()
            .unwrap_or_else(|| DEFAULT_WELCOME_DESCRIPTION.to_string());

        let render = |text: &str| {
            text.replace("{mention}", member_mention)
                .replace("{user}", display_name)
                .replace("{tag}", username)
                .replace("{server}", server)
                .replace("{membercount}", &member_count.to_string())
        };

        WelcomeEmbed {
            title: render(&title),
            description: render(&description),
            color: self.embed_color.unwrap_or(DEFAULT_COLOR),
            avatar_mode: self.avatar_mode.clone(),
        }
    }

    pub fn channel(&self) -> Option<u64> {
        self.channel_id
            .as_deref()
            .and_then(|value| value.parse().ok())
    }
}

pub struct WelcomeEmbed {
    pub title: String,
    pub description: String,
    pub color: i64,
    pub avatar_mode: String,
}

pub async fn welcome(db: &Db, guild_id: u64) -> sqlx::Result<WelcomeSettings> {
    let row =
        sqlx::query_as::<_, WelcomeSettings>("SELECT * FROM welcome_settings WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_optional(&db.pool)
            .await?;

    Ok(row.unwrap_or(WelcomeSettings {
        guild_id: guild_id.to_string(),
        channel_id: None,
        embed_title: None,
        embed_description: None,
        embed_color: None,
        avatar_mode: "off".to_string(),
    }))
}

async fn upsert(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO welcome_settings (guild_id) VALUES (?) ON CONFLICT(guild_id) DO NOTHING",
    )
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_welcome_channel(
    db: &Db,
    guild_id: u64,
    channel_id: Option<u64>,
) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    sqlx::query("UPDATE welcome_settings SET channel_id = ? WHERE guild_id = ?")
        .bind(channel_id.map(|id| id.to_string()))
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_welcome_field(
    db: &Db,
    guild_id: u64,
    field: WelcomeField,
    value: &str,
) -> sqlx::Result<()> {
    upsert(db, guild_id).await?;
    // `field` comes from a closed enum, never user input.
    let column = match field {
        WelcomeField::Title => "embed_title",
        WelcomeField::Description => "embed_description",
        WelcomeField::Color => "embed_color",
        WelcomeField::AvatarMode => "avatar_mode",
    };
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE welcome_settings SET {column} = ? WHERE guild_id = ?"
    )))
    .bind(value)
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub enum WelcomeField {
    Title,
    Description,
    Color,
    AvatarMode,
}

const WELCOME_CLEANUP_FEATURE: &str = "welcome";
const GIVEAWAY_CLEANUP_FEATURE: &str = "giveaway";
const BALLOT_CLEANUP_FEATURE: &str = "ballot";
const WOS_CLEANUP_FEATURE: &str = "wos";
const BIRTHDAY_CLEANUP_FEATURE: &str = "birthday";
const LEVELING_CLEANUP_FEATURE: &str = "leveling";
const WELCOME_CLEANUP_MAX_MINUTES: i64 = 43_200;

#[derive(Debug, Clone, FromRow)]
pub struct CleanupSettings {
    pub enabled: i64,
    pub delay_minutes: i64,
}

impl CleanupSettings {
    pub fn enabled(&self) -> bool {
        self.enabled != 0
    }

    pub fn delay_minutes(&self) -> i64 {
        self.delay_minutes.clamp(1, WELCOME_CLEANUP_MAX_MINUTES)
    }
}

pub async fn cleanup_settings(db: &Db, guild_id: u64) -> sqlx::Result<CleanupSettings> {
    cleanup_settings_for(db, guild_id, WELCOME_CLEANUP_FEATURE).await
}

pub async fn giveaway_cleanup_settings(db: &Db, guild_id: u64) -> sqlx::Result<CleanupSettings> {
    cleanup_settings_for(db, guild_id, GIVEAWAY_CLEANUP_FEATURE).await
}

pub async fn ballot_cleanup_settings(db: &Db, guild_id: u64) -> sqlx::Result<CleanupSettings> {
    cleanup_settings_for(db, guild_id, BALLOT_CLEANUP_FEATURE).await
}

pub async fn wos_cleanup_settings(db: &Db, guild_id: u64) -> sqlx::Result<CleanupSettings> {
    cleanup_settings_for(db, guild_id, WOS_CLEANUP_FEATURE).await
}

pub async fn birthday_cleanup_settings(db: &Db, guild_id: u64) -> sqlx::Result<CleanupSettings> {
    cleanup_settings_for(db, guild_id, BIRTHDAY_CLEANUP_FEATURE).await
}

pub async fn leveling_cleanup_settings(db: &Db, guild_id: u64) -> sqlx::Result<CleanupSettings> {
    cleanup_settings_for(db, guild_id, LEVELING_CLEANUP_FEATURE).await
}

async fn cleanup_settings_for(
    db: &Db,
    guild_id: u64,
    feature: &str,
) -> sqlx::Result<CleanupSettings> {
    let row = sqlx::query_as::<_, CleanupSettings>(
        "SELECT enabled, delay_minutes FROM feature_cleanup_settings WHERE guild_id = ? AND feature = ?",
    )
    .bind(guild_id.to_string())
    .bind(feature)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.unwrap_or(CleanupSettings {
        enabled: 0,
        delay_minutes: 1440,
    }))
}

pub async fn set_cleanup_settings(
    db: &Db,
    guild_id: u64,
    enabled: bool,
    delay_minutes: i64,
) -> sqlx::Result<()> {
    set_cleanup_settings_for(
        db,
        guild_id,
        WELCOME_CLEANUP_FEATURE,
        enabled,
        delay_minutes,
    )
    .await
}

pub async fn set_giveaway_cleanup_settings(
    db: &Db,
    guild_id: u64,
    enabled: bool,
    delay_minutes: i64,
) -> sqlx::Result<()> {
    set_cleanup_settings_for(
        db,
        guild_id,
        GIVEAWAY_CLEANUP_FEATURE,
        enabled,
        delay_minutes,
    )
    .await
}

pub async fn set_ballot_cleanup_settings(
    db: &Db,
    guild_id: u64,
    enabled: bool,
    delay_minutes: i64,
) -> sqlx::Result<()> {
    set_cleanup_settings_for(db, guild_id, BALLOT_CLEANUP_FEATURE, enabled, delay_minutes).await
}

pub async fn set_wos_cleanup_settings(
    db: &Db,
    guild_id: u64,
    enabled: bool,
    delay_minutes: i64,
) -> sqlx::Result<()> {
    set_cleanup_settings_for(db, guild_id, WOS_CLEANUP_FEATURE, enabled, delay_minutes).await
}

pub async fn set_birthday_cleanup_settings(
    db: &Db,
    guild_id: u64,
    enabled: bool,
    delay_minutes: i64,
) -> sqlx::Result<()> {
    set_cleanup_settings_for(
        db,
        guild_id,
        BIRTHDAY_CLEANUP_FEATURE,
        enabled,
        delay_minutes,
    )
    .await
}

pub async fn set_leveling_cleanup_settings(
    db: &Db,
    guild_id: u64,
    enabled: bool,
    delay_minutes: i64,
) -> sqlx::Result<()> {
    set_cleanup_settings_for(
        db,
        guild_id,
        LEVELING_CLEANUP_FEATURE,
        enabled,
        delay_minutes,
    )
    .await
}

async fn set_cleanup_settings_for(
    db: &Db,
    guild_id: u64,
    feature: &str,
    enabled: bool,
    delay_minutes: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO feature_cleanup_settings (guild_id, feature, enabled, delay_minutes, updated_at) VALUES (?, ?, ?, ?, datetime('now')) ON CONFLICT(guild_id, feature) DO UPDATE SET enabled = excluded.enabled, delay_minutes = excluded.delay_minutes, updated_at = excluded.updated_at",
    )
    .bind(guild_id.to_string())
    .bind(feature)
    .bind(enabled as i64)
    .bind(delay_minutes.clamp(1, WELCOME_CLEANUP_MAX_MINUTES))
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn record_cleanup_message(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
) -> sqlx::Result<()> {
    record_cleanup_message_for(
        db,
        guild_id,
        WELCOME_CLEANUP_FEATURE,
        channel_id,
        message_id,
    )
    .await
}

pub async fn record_giveaway_cleanup_message(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
) -> sqlx::Result<()> {
    record_cleanup_message_for(
        db,
        guild_id,
        GIVEAWAY_CLEANUP_FEATURE,
        channel_id,
        message_id,
    )
    .await
}

pub async fn record_ballot_cleanup_message(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
) -> sqlx::Result<()> {
    record_cleanup_message_for(db, guild_id, BALLOT_CLEANUP_FEATURE, channel_id, message_id).await
}

pub async fn record_wos_cleanup_message(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
) -> sqlx::Result<()> {
    record_cleanup_message_for(db, guild_id, WOS_CLEANUP_FEATURE, channel_id, message_id).await
}

pub async fn record_birthday_cleanup_message(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
) -> sqlx::Result<()> {
    record_cleanup_message_for(
        db,
        guild_id,
        BIRTHDAY_CLEANUP_FEATURE,
        channel_id,
        message_id,
    )
    .await
}

pub async fn record_leveling_cleanup_message(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
) -> sqlx::Result<()> {
    record_cleanup_message_for(
        db,
        guild_id,
        LEVELING_CLEANUP_FEATURE,
        channel_id,
        message_id,
    )
    .await
}

async fn record_cleanup_message_for(
    db: &Db,
    guild_id: u64,
    feature: &str,
    channel_id: u64,
    message_id: u64,
) -> sqlx::Result<()> {
    let settings = cleanup_settings_for(db, guild_id, feature).await?;
    if !settings.enabled() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO feature_message_history (guild_id, feature, channel_id, message_id, scheduled_delete_at) VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now', printf('+%d minutes', ?)))",
    )
    .bind(guild_id.to_string())
    .bind(feature)
    .bind(channel_id.to_string())
    .bind(message_id.to_string())
    .bind(settings.delay_minutes())
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, FromRow)]
pub struct CleanupMessage {
    pub id: i64,
    pub channel_id: String,
    pub message_id: String,
}

pub async fn due_cleanup_messages(db: &Db, limit: i64) -> sqlx::Result<Vec<CleanupMessage>> {
    sqlx::query_as::<_, CleanupMessage>(
        "SELECT id, channel_id, message_id FROM feature_message_history WHERE feature IN ('welcome', 'giveaway', 'ballot', 'wos', 'birthday', 'leveling') AND scheduled_delete_at IS NOT NULL AND deleted_at IS NULL AND julianday(scheduled_delete_at) <= julianday('now') ORDER BY julianday(scheduled_delete_at) ASC LIMIT ?",
    )
    .bind(limit.clamp(1, 20))
    .fetch_all(&db.pool)
    .await
}

pub async fn finish_cleanup_message(db: &Db, id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE feature_message_history SET deleted_at = datetime('now') WHERE id = ? AND deleted_at IS NULL")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

// ------------------------------------------------------------------ autorole

#[derive(Debug, Clone, FromRow)]
pub struct AutoRoleSettings {
    pub guild_id: String,
    pub enabled: i64,
    pub member_role_ids: String,
    pub bot_role_ids: String,
    pub updated_at: i64,
}

impl AutoRoleSettings {
    pub fn enabled(&self) -> bool {
        self.enabled != 0
    }

    pub fn member_roles(&self) -> Vec<u64> {
        parse_role_ids(&self.member_role_ids)
    }

    pub fn bot_roles(&self) -> Vec<u64> {
        parse_role_ids(&self.bot_role_ids)
    }
}

fn parse_role_ids(text: &str) -> Vec<u64> {
    serde_json::from_str::<Vec<String>>(text)
        .map(|items| {
            items
                .into_iter()
                .filter_map(|item| item.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

pub async fn autorole(db: &Db, guild_id: u64) -> sqlx::Result<AutoRoleSettings> {
    let row =
        sqlx::query_as::<_, AutoRoleSettings>("SELECT * FROM autorole_settings WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_optional(&db.pool)
            .await?;

    Ok(row.unwrap_or(AutoRoleSettings {
        guild_id: guild_id.to_string(),
        enabled: 1,
        member_role_ids: "[]".to_string(),
        bot_role_ids: "[]".to_string(),
        updated_at: 0,
    }))
}

async fn autorole_upsert(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO autorole_settings (guild_id, updated_at) VALUES (?, ?)
         ON CONFLICT(guild_id) DO NOTHING",
    )
    .bind(guild_id.to_string())
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_autorole_enabled(db: &Db, guild_id: u64, enabled: bool) -> sqlx::Result<()> {
    autorole_upsert(db, guild_id).await?;
    sqlx::query("UPDATE autorole_settings SET enabled = ?, updated_at = ? WHERE guild_id = ?")
        .bind(enabled as i64)
        .bind(now())
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_autorole_roles(
    db: &Db,
    guild_id: u64,
    kind: String,
    role_ids: &[u64],
) -> sqlx::Result<()> {
    autorole_upsert(db, guild_id).await?;
    // Column chosen from a closed match — never user input.
    let column = match kind.as_str() {
        "bot" => "bot_role_ids",
        _ => "member_role_ids",
    };
    let encoded =
        serde_json::to_string(&role_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
            .unwrap_or_else(|_| "[]".to_string());
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE autorole_settings SET {column} = ?, updated_at = ? WHERE guild_id = ?"
    )))
    .bind(encoded)
    .bind(now())
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

// -------------------------------------------------------------------- join DM

#[derive(Debug, Clone, FromRow)]
pub struct JoinDmSettings {
    pub guild_id: String,
    pub join_dm_enabled: i64,
    pub register_reminder_enabled: i64,
    pub reminder_interval_days: i64,
    pub updated_at: i64,
}

impl JoinDmSettings {
    pub fn join_dm_enabled(&self) -> bool {
        self.join_dm_enabled != 0
    }

    pub fn reminders_enabled(&self) -> bool {
        self.register_reminder_enabled != 0
    }

    pub fn interval_days(&self) -> u64 {
        self.reminder_interval_days.clamp(1, 90) as u64
    }
}

pub async fn join_dm(db: &Db, guild_id: u64) -> sqlx::Result<JoinDmSettings> {
    // The JS store materializes default settings on first read. Keep this row
    // so a newly joined member is eligible for the configured reminder sweep.
    sqlx::query("INSERT OR IGNORE INTO join_dm_settings (guild_id) VALUES (?)")
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    let row =
        sqlx::query_as::<_, JoinDmSettings>("SELECT * FROM join_dm_settings WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_optional(&db.pool)
            .await?;

    Ok(row.unwrap_or(JoinDmSettings {
        guild_id: guild_id.to_string(),
        join_dm_enabled: 1,
        register_reminder_enabled: 1,
        reminder_interval_days: 7,
        updated_at: 0,
    }))
}

pub async fn set_join_dm(db: &Db, guild_id: u64, enabled: bool) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO join_dm_settings (guild_id, join_dm_enabled, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET join_dm_enabled = excluded.join_dm_enabled, updated_at = excluded.updated_at",
    )
    .bind(guild_id.to_string())
    .bind(enabled as i64)
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_reminder_enabled(db: &Db, guild_id: u64, enabled: bool) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO join_dm_settings (guild_id, register_reminder_enabled, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET
             register_reminder_enabled = excluded.register_reminder_enabled,
             updated_at = excluded.updated_at",
    )
    .bind(guild_id.to_string())
    .bind(enabled as i64)
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, FromRow)]
pub struct TrackedJoin {
    pub guild_id: String,
    pub user_id: String,
    pub joined_at: i64,
    pub join_dm_sent_at: Option<i64>,
    pub last_reminder_at: Option<i64>,
    pub reminder_count: i64,
    pub reminder_interval_days: i64,
}

pub async fn track_join(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<()> {
    let now_ms = now() * 1000;
    sqlx::query(
        "INSERT INTO registration_reminders (guild_id, user_id, joined_at)
         VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET joined_at = excluded.joined_at",
    )
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .bind(now_ms)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn mark_join_dm_sent(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<()> {
    let now_ms = now() * 1000;
    sqlx::query(
        "INSERT INTO registration_reminders (guild_id, user_id, joined_at, join_dm_sent_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET join_dm_sent_at = excluded.join_dm_sent_at",
    )
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .bind(now_ms)
    .bind(now_ms)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn remove_join(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM registration_reminders WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Remove the legacy Join DM scheduler state when the bot leaves a guild.
pub async fn remove_join_guild(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM registration_reminders WHERE guild_id = ?")
        .bind(guild_id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM join_dm_settings WHERE guild_id = ?")
        .bind(guild_id.to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Track reminders that are due: joined at least `interval_days` ago and no
/// reminder sent recently. Returns `(guild_id, user_id)` pairs.
pub async fn due_reminders(db: &Db, now_ms: i64, limit: i64) -> sqlx::Result<Vec<(u64, u64)>> {
    let rows: Vec<TrackedJoin> = sqlx::query_as::<_, TrackedJoin>(
        "SELECT r.guild_id, r.user_id, r.joined_at, r.join_dm_sent_at, r.last_reminder_at, r.reminder_count, s.reminder_interval_days
         FROM registration_reminders r
         JOIN join_dm_settings s ON s.guild_id = r.guild_id
         WHERE s.register_reminder_enabled = 1
           AND NOT EXISTS (SELECT 1 FROM players p WHERE p.user_id = r.user_id)",
    )
    .fetch_all(&db.pool)
    .await?;

    let mut due = Vec::new();
    for row in rows {
        let interval_ms = row.reminder_interval_days.clamp(1, 90) * 86_400_000;
        let joined = row.joined_at;
        let last = row.last_reminder_at;
        let due_now =
            now_ms - joined >= interval_ms && last.is_none_or(|last| now_ms - last >= interval_ms);
        if due_now {
            if let (Ok(guild), Ok(user)) = (row.guild_id.parse::<u64>(), row.user_id.parse::<u64>())
            {
                due.push((guild, user));
                if due.len() as i64 >= limit {
                    break;
                }
            }
        }
    }
    Ok(due)
}

pub async fn mark_reminder_sent(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<()> {
    let now_ms = now() * 1000;
    sqlx::query(
        "UPDATE registration_reminders
         SET last_reminder_at = ?, reminder_count = reminder_count + 1
         WHERE guild_id = ? AND user_id = ?",
    )
    .bind(now_ms)
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}
