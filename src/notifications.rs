//! Notification engine: scheduled channel posts (Bear Trap / custom).
//!
//! Every notification is a row in `bear_notifications`: a wall-clock time in
//! an IANA timezone, a set of minute-before slots for delivery, and an
//! optional repeat. `tasks::spawn_notification_sweep` polls due notifications
//! every 500 ms and deletes expired messages every 10 s, matching the legacy
//! bot's cadence.

pub mod wizard;

use chrono::{DateTime, Duration, Utc};
use poise::serenity_prelude as serenity;

use crate::state::Data;

pub async fn check(ctx: &serenity::Context, data: &Data) -> anyhow::Result<usize> {
    let now = Utc::now();
    let due = crate::db::notifications::due(&data.db, now).await?;
    let mut sent = 0usize;

    for notification in due {
        let guild_id = notification.guild_id.parse::<u64>().unwrap_or(0);
        let Some(channel_id) = notification.channel() else {
            // Corrupt row — auto-disable so we don't spam the logs forever.
            let _ = crate::db::notifications::auto_disable(&data.db, notification.id).await;
            if let Some(timestamp) =
                crate::db::notifications::auto_disabled_timestamp(&data.db, notification.id)
                    .await
                    .ok()
                    .flatten()
            {
                send_quarantine_notice(
                    ctx,
                    data,
                    &notification.guild_id,
                    None,
                    &timestamp,
                    "the notification has an invalid channel",
                )
                .await;
            }
            continue;
        };
        // The slot that this send represents.
        let slot = slot_for(
            now,
            notification.hour,
            notification.minute,
            notification.tz(),
            notification.notification_type,
            &notification.description,
        );
        // JS schedules deletion only for the final lead-time alert. It uses
        // the event's own duration when one exists, otherwise the guild
        // default, with a per-notification override taking priority.
        let all_slots = crate::db::notifications::schedule_slots(
            notification.notification_type,
            &notification.description,
        );
        let is_last_slot = all_slots.iter().copied().min() == Some(slot);
        let default_delete_delay = if !is_last_slot {
            None
        } else if guild_id != 0 {
            match crate::db::notifications::guild_settings(&data.db, guild_id)
                .await
                .ok()
            {
                Some(settings) if settings.delete_enabled => {
                    Some(notification.custom_delete_delay_minutes.unwrap_or_else(|| {
                        let event_duration = notification
                            .event_type
                            .as_deref()
                            .map(notification_event_duration_minutes)
                            .unwrap_or(0);
                        if event_duration > 0 {
                            event_duration
                        } else {
                            settings.default_delete_delay_minutes
                        }
                    }))
                }
                _ => None,
            }
        } else {
            None
        };

        // Keep the legacy event-template placeholders available to custom fields.
        let minutes = slot.max(0);
        let time_phrase = match minutes {
            0 => "now".to_string(),
            1 => "in 1 minute".to_string(),
            n => format!("in {n} minutes"),
        };
        let event_label = notification
            .event_type
            .as_deref()
            .map(notification_event_label)
            .unwrap_or_default();
        let event_emoji = notification
            .event_type
            .as_deref()
            .map(notification_event_emoji)
            .unwrap_or("🔔");
        let mention_text = notification.mention_text();
        let template = if let Some(event) = notification.event_type.as_deref() {
            crate::db::notifications::event_template(&data.db, event)
                .await
                .ok()
                .flatten()
        } else {
            None
        };
        let custom_embed = crate::db::notifications::notification_embed(&data.db, notification.id)
            .await
            .ok()
            .flatten();
        let event_time = notification
            .next_notification
            .as_deref()
            .and_then(crate::db::notifications::parse_iso)
            .map(|dt| {
                dt.with_timezone(&notification.tz())
                    .format("%H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let event_date = notification
            .next_notification
            .as_deref()
            .and_then(crate::db::notifications::parse_iso)
            .map(|dt| {
                dt.with_timezone(&notification.tz())
                    .format("%b %d")
                    .to_string()
            })
            .unwrap_or_default();
        let expand = |text: &str| {
            text.replace("%t", &time_phrase)
                .replace("{time}", &time_phrase)
                .replace("%n", &event_label)
                .replace("%e", &event_time)
                .replace("%d", &event_date)
                .replace("%i", event_emoji)
                .replace("@tag", &mention_text)
                .replace("{tag}", &mention_text)
        };
        let stored_description = notification.description.as_str();
        let stored_description = stored_description
            .strip_prefix("CUSTOM_TIMES:")
            .and_then(|value| value.split_once('|').map(|(_, message)| message))
            .unwrap_or(stored_description);
        let plain_message = stored_description.strip_prefix("PLAIN_MESSAGE:");
        let body_template = custom_embed
            .as_ref()
            .and_then(|t| t.description.as_deref())
            .filter(|text| !text.is_empty())
            .or_else(|| {
                template
                    .as_ref()
                    .and_then(|t| t.embed_description.as_deref())
                    .filter(|text| !text.is_empty())
            })
            .unwrap_or_else(|| plain_message.unwrap_or(stored_description));
        let body_template = body_template
            .strip_prefix("CUSTOM_TIMES:")
            .and_then(|value| value.split_once('|').map(|(_, message)| message))
            .unwrap_or(body_template);
        let body = expand(body_template);

        let mention = mention_text.clone();
        let content = if mention.is_empty() {
            None
        } else {
            Some(
                custom_embed
                    .as_ref()
                    .and_then(|t| t.mention_message.as_deref())
                    .filter(|s| !s.trim().is_empty())
                    .map(expand)
                    .or_else(|| {
                        template
                            .as_ref()
                            .and_then(|t| t.mention_message.as_deref())
                            .filter(|s| !s.trim().is_empty())
                            .map(expand)
                    })
                    .unwrap_or_else(|| format!("{mention}{time_phrase}")),
            )
        };
        let embed_title = if let Some(title) = custom_embed
            .as_ref()
            .and_then(|t| t.title.clone())
            .filter(|s| !s.is_empty())
        {
            title
        } else if event_label.is_empty() {
            "🔔 Reminder".to_string()
        } else {
            template
                .as_ref()
                .and_then(|t| t.embed_title.clone())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| format!("{} {}", event_emoji, event_label))
        };
        let color = custom_embed
            .as_ref()
            .and_then(|t| t.color)
            .and_then(|v| u32::try_from(v).ok())
            .or_else(|| {
                template
                    .as_ref()
                    .and_then(|t| t.embed_color.as_deref())
                    .and_then(|value| {
                        value
                            .parse::<u32>()
                            .ok()
                            .or_else(|| u32::from_str_radix(value.trim_start_matches('#'), 16).ok())
                    })
            })
            .unwrap_or(crate::ui::theme::colors::BRAND);
        let mut embed = serenity::CreateEmbed::new()
            .title(expand(&embed_title))
            .description(body.clone())
            .color(color)
            .footer(serenity::CreateEmbedFooter::new(
                custom_embed
                    .as_ref()
                    .and_then(|t| t.footer.as_deref())
                    .map(expand)
                    .or_else(|| {
                        template
                            .as_ref()
                            .and_then(|t| t.footer.as_deref())
                            .map(expand)
                    })
                    .unwrap_or_else(|| crate::ui::theme::footer()),
            ));
        if let Some(author) = custom_embed
            .as_ref()
            .and_then(|t| t.author.as_deref())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                template
                    .as_ref()
                    .and_then(|t| t.author.as_deref())
                    .filter(|s| !s.trim().is_empty())
            })
        {
            embed = embed.author(serenity::CreateEmbedAuthor::new(expand(author)));
        }
        if let Some(url) = custom_embed
            .as_ref()
            .and_then(|t| t.image_url.as_deref())
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            .or_else(|| {
                template
                    .as_ref()
                    .and_then(|t| t.embed_image_url.as_deref())
                    .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            })
        {
            embed = embed.image(url);
        }
        if let Some(url) = custom_embed
            .as_ref()
            .and_then(|t| t.thumbnail_url.as_deref())
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            .or_else(|| {
                template
                    .as_ref()
                    .and_then(|t| t.embed_thumbnail_url.as_deref())
                    .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            })
        {
            embed = embed.thumbnail(url);
        }
        let extra_images = custom_embed
            .as_ref()
            .and_then(|t| t.images.as_deref())
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .or_else(|| {
                template
                    .as_ref()
                    .and_then(|t| t.embed_images.as_deref())
                    .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            })
            .unwrap_or_default();
        let mut embeds = vec![embed];
        embeds.extend(
            extra_images
                .into_iter()
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
                .take(8)
                .map(|url| serenity::CreateEmbed::new().color(color).image(url)),
        );

        let scheduled_trigger = notification
            .next_notification
            .as_deref()
            .and_then(crate::db::notifications::parse_iso)
            .unwrap_or(now);
        let (mut builder, mention_message_id, translation_source) = if plain_message.is_some() {
            let contains_placeholders = ["@tag", "{tag}", "%t", "{time}", "%n", "%e", "%d", "%i"]
                .iter()
                .any(|placeholder| body_template.contains(placeholder));
            let plain = if contains_placeholders {
                body.clone()
            } else if slot > 0 {
                format!(
                    "{}⏰ **{}** will start in **{}**!",
                    &mention_text, body, time_phrase
                )
            } else {
                format!("{}⏰ **{}**", mention_text, body)
            };
            let source = plain.clone();
            (
                serenity::CreateMessage::new().content(plain),
                None,
                (None, Some(source)),
            )
        } else {
            let source = crate::translate::stored_json(&embeds[0]);
            let mention_id = if let Some(content) = content {
                match serenity::ChannelId::new(channel_id)
                    .send_message(
                        &ctx.http,
                        serenity::CreateMessage::new()
                            .content(content)
                            .allowed_mentions(
                                serenity::CreateAllowedMentions::new()
                                    .everyone(matches!(
                                        notification.mention_type.as_str(),
                                        "here" | "everyone"
                                    ))
                                    .all_roles(notification.mention_type.starts_with("role_"))
                                    .all_users(notification.mention_type.starts_with("member_")),
                            ),
                    )
                    .await
                {
                    Ok(message) => {
                        let _ = crate::db::notifications::record_history(
                            &data.db,
                            notification.id,
                            scheduled_trigger,
                            Some(message.id.get()),
                            Some(channel_id),
                            default_delete_delay,
                        )
                        .await;
                        Some(message.id.get())
                    }
                    Err(error) => {
                        tracing::warn!(%error, channel = channel_id, "notification mention message send failed");
                        None
                    }
                }
            } else {
                None
            };
            (
                serenity::CreateMessage::new().embeds(embeds),
                mention_id,
                (source, None),
            )
        };
        builder = builder.allowed_mentions(
            serenity::CreateAllowedMentions::new()
                .everyone(matches!(
                    notification.mention_type.as_str(),
                    "here" | "everyone"
                ))
                .all_roles(notification.mention_type.starts_with("role_"))
                .all_users(notification.mention_type.starts_with("member_")),
        );

        // JS removes prior sibling alerts before sending the new one.
        delete_previous_alerts(ctx, data, &notification, channel_id).await;
        match serenity::ChannelId::new(channel_id)
            .send_message(&ctx.http, builder)
            .await
        {
            Ok(message) => {
                sent += 1;
                failure_strikes().lock().await.remove(&channel_id);
                let _ = mention_message_id;
                let _ = crate::db::notifications::record_history(
                    &data.db,
                    notification.id,
                    scheduled_trigger,
                    Some(message.id.get()),
                    Some(channel_id),
                    default_delete_delay,
                )
                .await;
                attach_notification_translate(ctx, data, guild_id, &message, translation_source)
                    .await;
                let _ = crate::db::notifications::advance(&data.db, &notification, slot, now).await;
            }
            Err(err) => {
                tracing::warn!(error = %err, channel = channel_id, "notification send failed");
                if is_channel_access_error(&err) && confirm_channel_failure(channel_id).await {
                    if let Ok(guild_id) = notification.guild_id.parse::<u64>() {
                        match crate::db::notifications::quarantine_enabled_channel(
                            &data.db, guild_id, channel_id,
                        )
                        .await
                        {
                            Ok(Some(timestamp)) => {
                                send_quarantine_notice(
                                    ctx,
                                    data,
                                    &notification.guild_id,
                                    Some(channel_id),
                                    &timestamp,
                                    "the bot lost channel access after repeated checks",
                                )
                                .await;
                            }
                            Ok(None) => {}
                            Err(error) => {
                                tracing::warn!(%error, channel = channel_id, "failed to quarantine inaccessible notification channel")
                            }
                        }
                    }
                } else if !is_channel_access_error(&err) {
                    tracing::debug!(channel = channel_id, "transient or non-permission notification error; leaving notifications enabled");
                }
            }
        }
    }
    Ok(sent)
}

/// JS `deletePreviousNotifications`: only the newest alert for a row stays
/// visible until the final one, which gets a scheduled deletion instead.
async fn delete_previous_alerts(
    ctx: &serenity::Context,
    data: &Data,
    notification: &crate::db::notifications::Notification,
    channel_id: u64,
) {
    // JS resolves the channel once and returns without touching history when
    // it is gone/inaccessible.
    if ctx
        .http
        .get_channel(serenity::ChannelId::new(channel_id))
        .await
        .is_err()
    {
        return;
    }
    let rows = match crate::db::notifications::pending_unscheduled_for_alert(
        &data.db,
        notification.id,
        &notification.channel_id,
        notification.event_type.as_deref(),
        notification.instance_identifier.as_deref(),
    )
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::debug!(%error, "could not load previous notification messages");
            return;
        }
    };
    for (history_id, message) in rows {
        if let Ok(message_id) = message.parse::<u64>() {
            // Delete through the resolved channel only, so messages in other
            // channels/guilds are never touched.
            let _ = serenity::ChannelId::new(channel_id)
                .delete_message(&ctx.http, serenity::MessageId::new(message_id))
                .await;
        }
        // Failures still mark the row handled, matching the JS sweeper.
        let _ = crate::db::notifications::mark_deleted(&data.db, history_id).await;
    }
}

/// JS `translate.attachTranslate`: register every delivered reminder as a
/// translatable message and add the private 🌐 Translate control.
async fn attach_notification_translate(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    message: &serenity::Message,
    source: (Option<String>, Option<String>),
) {
    let (embed_json, content) = source;
    if embed_json.is_none() && content.is_none() {
        return;
    }
    let record = crate::db::translate::NewTranslatable {
        guild_id: Some(guild_id),
        channel_id: Some(message.channel_id.get()),
        kind: "notification",
        embed_json,
        content,
        owner_id: None,
    };
    if let Err(error) = crate::translate::register(&data.db, message.id.get(), record).await {
        tracing::debug!(%error, "could not register notification translation");
        return;
    }
    let rows = vec![serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(format!("translate:{}", message.id.get()))
            .label("🌐 Translate")
            .style(serenity::ButtonStyle::Secondary),
    ])];
    let _ = message
        .channel_id
        .edit_message(
            &ctx.http,
            message.id,
            serenity::EditMessage::new().components(rows),
        )
        .await;
}

fn notification_event_label(event: &str) -> String {
    match event {
        "bear_trap" => "Bear Trap",
        "crazy_joe" => "Crazy Joe",
        "mercenary_prestige" => "Mercenary Prestige",
        "foundry_battle" => "Foundry Battle",
        "canyon_clash" => "Canyon Clash",
        "fortress_battle" => "Fortress Battle",
        "frostfire_mine" => "Frostfire Mine",
        "castle_battle" => "Castle Battle",
        "svs" => "SvS",
        "daily_reset" => "Daily Reset",
        "chief_order" => "Chief Order",
        other => return other.replace('_', " "),
    }
    .to_string()
}

fn notification_event_emoji(event: &str) -> &'static str {
    match event {
        "bear_trap" => "🐻",
        "crazy_joe" => "🤪",
        "mercenary_prestige" => "🗡️",
        "foundry_battle" => "🏭",
        "canyon_clash" => "⚔️",
        "fortress_battle" => "🏰",
        "frostfire_mine" => "⛏️",
        "castle_battle" => "☀️",
        "svs" => "⚡",
        _ => "🔔",
    }
}

/// Legacy event duration in minutes, used for auto-delete of the final
/// reminder (JS `engine.getDeletionTime`). 0 means "use the guild default".
fn notification_event_duration_minutes(event: &str) -> i64 {
    match event {
        "bear_trap" | "crazy_joe" | "frostfire_mine" => 30,
        "mercenary_prestige" | "foundry_battle" | "canyon_clash" => 60,
        "fortress_battle" => 120,
        "castle_battle" | "svs" => 360,
        _ => 0,
    }
}

/// Delete messages whose retention window expired.
pub async fn sweep_deletions(ctx: &serenity::Context, data: &Data) -> anyhow::Result<usize> {
    let now = Utc::now();
    let due = crate::db::notifications::due_deletions(&data.db, now).await?;
    let mut deleted = 0usize;

    for (history_id, channel_id, message_id) in due {
        let Ok(channel) = channel_id.parse::<u64>() else {
            let _ = crate::db::notifications::mark_deleted(&data.db, history_id).await;
            continue;
        };
        let Ok(message) = message_id.parse::<u64>() else {
            let _ = crate::db::notifications::mark_deleted(&data.db, history_id).await;
            continue;
        };
        match serenity::ChannelId::new(channel)
            .delete_message(&ctx.http, serenity::MessageId::new(message))
            .await
        {
            Ok(_) | Err(_) => {
                // Even a 404 counts as deleted — the row would fail forever otherwise.
                let _ = crate::db::notifications::mark_deleted(&data.db, history_id).await;
                deleted += 1;
            }
        }
    }
    Ok(deleted)
}

/// In-memory channel-level confirmation state, matching JS's 3 checks at
/// five-minute intervals. Transient HTTP/network errors do not count.
struct ChannelFailureState {
    failures: u8,
    last_confirmed: std::time::Instant,
}

fn failure_strikes(
) -> &'static tokio::sync::Mutex<std::collections::HashMap<u64, ChannelFailureState>> {
    use std::sync::OnceLock;
    static STRIKES: OnceLock<
        tokio::sync::Mutex<std::collections::HashMap<u64, ChannelFailureState>>,
    > = OnceLock::new();
    STRIKES.get_or_init(|| tokio::sync::Mutex::new(std::collections::HashMap::new()))
}

async fn confirm_channel_failure(channel_id: u64) -> bool {
    let mut guard = failure_strikes().lock().await;
    let now = std::time::Instant::now();
    guard.retain(|_, state| {
        now.duration_since(state.last_confirmed) < std::time::Duration::from_secs(30 * 60)
    });
    if guard.len() >= 5_000 && !guard.contains_key(&channel_id) {
        if let Some(oldest) = guard
            .iter()
            .min_by_key(|(_, state)| state.last_confirmed)
            .map(|(id, _)| *id)
        {
            guard.remove(&oldest);
        }
    }
    if let Some(state) = guard.get_mut(&channel_id) {
        if now.duration_since(state.last_confirmed) < std::time::Duration::from_secs(300) {
            return false;
        }
        state.last_confirmed = now;
        state.failures += 1;
        if state.failures >= 3 {
            guard.remove(&channel_id);
            return true;
        }
    } else {
        guard.insert(
            channel_id,
            ChannelFailureState {
                failures: 1,
                last_confirmed: now,
            },
        );
    }
    false
}

fn is_channel_access_error(error: &serenity::Error) -> bool {
    matches!(
        error,
        serenity::Error::Http(serenity::http::HttpError::UnsuccessfulRequest(response))
            if matches!(response.error.code, 10003 | 50001 | 50013)
    )
}

pub async fn quarantine_guild(ctx: &serenity::Context, data: &Data, guild_id: u64, reason: &str) {
    match crate::db::notifications::quarantine_enabled_guild(&data.db, guild_id).await {
        Ok(Some(timestamp)) => {
            send_quarantine_notice(ctx, data, &guild_id.to_string(), None, &timestamp, reason)
                .await;
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, guild_id, "failed to quarantine guild notifications"),
    }
}

pub async fn sweep_orphaned(ctx: &serenity::Context, data: &Data) {
    let guilds = match crate::db::notifications::enabled_guild_ids(&data.db).await {
        Ok(guilds) => guilds,
        Err(error) => {
            tracing::warn!(%error, "failed to list guilds for notification recovery sweep");
            return;
        }
    };
    for guild_id in guilds {
        if ctx.cache.guild(serenity::GuildId::new(guild_id)).is_none() {
            quarantine_guild(ctx, data, guild_id, "the bot is no longer in the server").await;
        }
    }
}

async fn send_quarantine_notice(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: &str,
    channel_id: Option<u64>,
    timestamp: &str,
    reason: &str,
) {
    let rows =
        match crate::db::notifications::auto_disabled_rows(&data.db, guild_id, timestamp).await {
            Ok(rows) if !rows.is_empty() => rows,
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(%error, "failed to read quarantined notification batch");
                return;
            }
        };
    let guild_label = ctx
        .cache
        .guild(serenity::GuildId::new(guild_id.parse::<u64>().unwrap_or(0)))
        .map(|guild| format!("**{}**", guild.name))
        .unwrap_or_else(|| format!("guild `{guild_id}`"));
    let scope = channel_id
        .map(|id| format!("channel `{id}` in {guild_label}"))
        .unwrap_or(guild_label);
    // JS `engine.js:sendQuarantineDm` reason phrases.
    let reason = match reason {
        "the bot lost channel access after repeated checks" => "the bot lost access to the channel",
        other => other,
    };
    let lines = rows
        .iter()
        .take(10)
        .map(|row| {
            let desc = if row.description.len() > 50 {
                format!(
                    "{}...",
                    row.description.chars().take(50).collect::<String>()
                )
            } else {
                row.description.clone()
            };
            format!(
                "- **ID {}:** {}",
                row.id,
                if desc.contains("EMBED_MESSAGE:") {
                    "(Embed notification)"
                } else if desc.is_empty() {
                    "(No description)"
                } else {
                    &desc
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let more = if rows.len() > 10 {
        format!("\n*…and {} more*", rows.len() - 10)
    } else {
        String::new()
    };
    let embed = crate::ui::embeds::brand("⚠️ Notifications Paused")
        .color(0xED42_45)
        .description(format!("**{}** notification(s) in {scope} were paused because {reason}.\n\n{lines}{more}\n\nPick **Re-enable** if access is restored, or **Delete Permanently** if you don't want these anymore. They stay paused until you choose.", rows.len()));
    let components = vec![serenity::CreateActionRow::Buttons(vec![
        crate::ui::components::success(
            format!("notif_q|re|{timestamp}|{guild_id}"),
            "✅ Re-enable",
        ),
        crate::ui::components::danger(
            format!("notif_q|del|{timestamp}|{guild_id}"),
            "🗑️ Delete Permanently",
        ),
    ])];
    let mut recipients = crate::db::admin::list(&data.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|admin| admin.id)
        .collect::<std::collections::HashSet<_>>();
    if let Some(owner) = data.cfg.owner_id {
        recipients.insert(owner);
    }
    for user_id in recipients {
        let dm = match serenity::UserId::new(user_id)
            .create_dm_channel(&ctx.http)
            .await
        {
            Ok(dm) => dm,
            Err(error) => {
                tracing::debug!(%error, user_id, "could not open notification recovery DM");
                continue;
            }
        };
        if let Err(error) = dm
            .send_message(
                &ctx.http,
                serenity::CreateMessage::new()
                    .embed(embed.clone())
                    .components(components.clone()),
            )
            .await
        {
            tracing::debug!(%error, user_id, "could not send notification recovery DM");
        }
    }
}

pub async fn handle_quarantine_component(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &serenity::ComponentInteraction,
) -> anyhow::Result<bool> {
    let Some(rest) = interaction.data.custom_id.strip_prefix("notif_q|") else {
        return Ok(false);
    };
    if data.cfg.owner_id != Some(interaction.user.id.get())
        && !crate::db::admin::is_admin(&data.db, interaction.user.id.get()).await
    {
        interaction
            .create_response(
                &ctx.http,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("⛔ Only bot admins can act on this notice.")
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(true);
    }
    let mut parts = rest.splitn(3, '|');
    let action = parts.next().unwrap_or_default();
    let timestamp = parts.next().unwrap_or_default();
    let guild_id = parts.next().unwrap_or_default();
    if !matches!(action, "re" | "del") || timestamp.is_empty() || guild_id.parse::<u64>().is_err() {
        anyhow::bail!("invalid notification recovery action");
    }
    let changed = crate::db::notifications::resolve_quarantine(
        &data.db,
        guild_id,
        timestamp,
        action == "del",
    )
    .await?;
    let content = if changed == 0 {
        "ℹ️ Already resolved — nothing matched.".to_string()
    } else if action == "del" {
        format!("✅ Permanently deleted **{changed}** notification(s).")
    } else {
        format!("✅ Re-enabled **{changed}** notification(s).")
    };
    interaction
        .create_response(
            &ctx.http,
            serenity::CreateInteractionResponse::UpdateMessage(
                serenity::CreateInteractionResponseMessage::new()
                    .content(content)
                    .embeds(Vec::new())
                    .components(Vec::new()),
            ),
        )
        .await?;
    Ok(true)
}

/// Which slot fired, given the trigger time (used for one-shot disabling).
fn slot_for(
    trigger: DateTime<Utc>,
    hour: i64,
    minute: i64,
    tz: chrono_tz::Tz,
    notification_type: i64,
    description: &str,
) -> i64 {
    use chrono::Timelike;
    for slot in crate::db::notifications::schedule_slots(notification_type, description) {
        let event = trigger + Duration::minutes(slot);
        let local = event.with_timezone(&tz);
        if local.hour() as i64 == hour && local.minute() as i64 == minute {
            return slot;
        }
    }
    0
}

/// Human-readable summary for the settings panel.
pub fn describe(notification: &crate::db::notifications::Notification) -> String {
    let tz = notification.timezone.replace("_", " ");
    let slots = crate::db::notifications::schedule_slots(
        notification.notification_type,
        &notification.description,
    );
    let slot_text: Vec<String> = slots
        .iter()
        .map(|s| {
            if *s == 0 {
                "at event".to_string()
            } else {
                format!("{s}m before")
            }
        })
        .collect();
    format!(
        "`{}` · **{:02}:{:02}** `{tz}` · {} · mention `{}` · {} · {}",
        notification.id,
        notification.hour,
        notification.minute,
        slot_text.join(", "),
        notification.mention_type,
        if notification.repeat_enabled == 1 {
            "repeats"
        } else {
            "one-shot"
        },
        if notification.is_enabled == 1 {
            "✅"
        } else {
            "⏸️"
        },
    )
}
