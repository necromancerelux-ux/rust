//! Forum and panel-post lifecycle for the Admin Toolkit.
//!
//! Setup/repair creates or reconciles the forum and only the posts registered
//! in `admin_toolkit_posts`; rebuild deletes only those posts. No other server
//! content is ever touched, and saved configuration is preserved.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use poise::serenity_prelude as serenity;
use serenity::{
    AutoArchiveDuration, ChannelId, ChannelType, CreateAllowedMentions, CreateChannel,
    CreateForumPost, CreateMessage, EditMessage, GuildChannel, PermissionOverwrite,
    PermissionOverwriteType, Permissions,
};

use crate::db::admin_toolkit::ToolkitConfig;
use crate::state::Data;

use super::registry;

const FORUM_NAME: &str = "🛠️-admin-toolkit";
const FORUM_TOPIC: &str =
    "R4/R5 Admin Toolkit — control panels for the server. Managed automatically by Omnicore.";

#[derive(Debug, Default)]
pub struct RepairResult {
    pub forum_id: u64,
    pub created: Vec<String>,
    pub refreshed: Vec<String>,
    pub failed: Vec<(String, String)>,
}

fn overwrites(guild_id: u64, config: &ToolkitConfig, bot_id: u64) -> Vec<PermissionOverwrite> {
    let mut list = vec![
        PermissionOverwrite {
            deny: Permissions::VIEW_CHANNEL,
            allow: Permissions::empty(),
            kind: PermissionOverwriteType::Role(serenity::RoleId::new(guild_id)),
        },
        PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL
                | Permissions::SEND_MESSAGES
                | Permissions::SEND_MESSAGES_IN_THREADS
                | Permissions::CREATE_PUBLIC_THREADS
                | Permissions::MANAGE_THREADS
                | Permissions::EMBED_LINKS
                | Permissions::READ_MESSAGE_HISTORY,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(serenity::UserId::new(bot_id)),
        },
    ];
    let mut seen = std::collections::HashSet::new();
    for role_id in config.r4_role_ids.iter().chain(config.r5_role_ids.iter()) {
        if !seen.insert(*role_id) {
            continue;
        }
        list.push(PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY,
            deny: Permissions::SEND_MESSAGES
                | Permissions::SEND_MESSAGES_IN_THREADS
                | Permissions::CREATE_PUBLIC_THREADS,
            kind: PermissionOverwriteType::Role(serenity::RoleId::new(*role_id)),
        });
    }
    list
}

async fn fetch_forum(ctx: &serenity::Context, id: Option<u64>) -> Option<GuildChannel> {
    let channel = ctx.http.get_channel(ChannelId::new(id?)).await.ok()?;
    match channel {
        serenity::Channel::Guild(channel) if channel.kind == ChannelType::Forum => Some(channel),
        _ => None,
    }
}

/// Find the saved forum, or use `preferred`, or create a new one.
pub async fn ensure_forum(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    config: &ToolkitConfig,
    preferred: Option<u64>,
) -> anyhow::Result<GuildChannel> {
    if let Some(forum) = fetch_forum(ctx, preferred.or(config.forum_channel_id)).await {
        sync_permissions(ctx, guild_id, forum.id.get(), config).await;
        return Ok(forum);
    }

    let bot_id = {
        let guild = ctx
            .cache
            .guild(serenity::GuildId::new(guild_id))
            .ok_or_else(|| anyhow::anyhow!("guild not in cache"))?;
        let bot_user_id = ctx.cache.current_user().id;
        let can_manage = guild
            .members
            .get(&bot_user_id)
            .and_then(|member| member.permissions.as_ref())
            .is_some_and(|perms| perms.contains(Permissions::MANAGE_CHANNELS));
        if !can_manage {
            return Err(anyhow::anyhow!(
                "I need **Manage Channels** to create the Admin Toolkit forum."
            ));
        }
        bot_user_id.get()
    };
    let forum = serenity::GuildId::new(guild_id)
        .create_channel(
            &ctx.http,
            CreateChannel::new(FORUM_NAME)
                .kind(ChannelType::Forum)
                .topic(FORUM_TOPIC)
                .permissions(overwrites(guild_id, config, bot_id))
                .audit_log_reason("Omnicore R4/R5 Admin Toolkit setup"),
        )
        .await?;
    let _ = crate::db::admin_toolkit::set_forum(&data.db, guild_id, forum.id.get()).await;
    Ok(forum)
}

/// Re-apply the access rules when R4/R5 roles change. Only touches the bot,
/// @everyone and configured role overwrites.
pub async fn sync_permissions(
    ctx: &serenity::Context,
    guild_id: u64,
    forum_id: u64,
    config: &ToolkitConfig,
) {
    let Some(forum) = fetch_forum(ctx, Some(forum_id)).await else {
        return;
    };
    let bot_id = ctx.cache.current_user().id.get();
    for overwrite in overwrites(guild_id, config, bot_id) {
        let _ = forum.create_permission(&ctx.http, overwrite).await;
    }
}

pub async fn fetch_post(ctx: &serenity::Context, post_id: u64) -> Option<GuildChannel> {
    let channel = ctx.http.get_channel(ChannelId::new(post_id)).await.ok()?;
    match channel {
        serenity::Channel::Guild(channel)
            if matches!(
                channel.kind,
                ChannelType::PublicThread | ChannelType::PrivateThread
            ) =>
        {
            Some(channel)
        }
        _ => None,
    }
}

pub(crate) async fn post_payload(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    key: &str,
) -> (serenity::CreateEmbed, Vec<serenity::CreateActionRow>) {
    let panel = registry::get_panel(key).expect("registered key");
    // The JavaScript Roles post is the live public role panel. Mirror that by
    // rendering the existing Rust role panel plus its settings shortcut.
    if key == "roles" {
        let embed = crate::roles::panel_embed();
        let mut rows = vec![crate::roles::panel_row(0)];
        rows.push(serenity::CreateActionRow::Buttons(vec![
            crate::ui::components::secondary("settings:roles", "⚙️ Role Settings"),
        ]));
        return (embed, rows);
    }
    // Forms is a live feature panel in the Toolkit forum. Render the existing
    // Ministry hub directly so its buttons use the same Rust handlers and data
    // as /settings → Forms.
    if key == "forms" {
        if let Ok(view) = crate::commands::interactions::ministry_view(data, guild_id).await {
            return crate::ui::panels::forms_toolkit_panel(&view);
        }
    }
    if key == "giveaways" {
        let active = crate::db::giveaways::count_active(&data.db, guild_id)
            .await
            .unwrap_or(0);
        let channel = crate::db::settings::get_u64(
            &data.db,
            guild_id,
            crate::db::settings::keys::GIVEAWAY_CHANNEL,
        )
        .await
        .unwrap_or(None);
        return crate::ui::panels::giveaways_panel(&crate::ui::panels::GiveawayView {
            active,
            channel,
        });
    }
    if key == "welcome" {
        let settings = crate::db::welcome::welcome(&data.db, guild_id).await;
        let autorole = crate::db::welcome::autorole(&data.db, guild_id).await;
        let joindm = crate::db::welcome::join_dm(&data.db, guild_id).await;
        let cleanup = crate::db::welcome::cleanup_settings(&data.db, guild_id).await;
        if let (Ok(settings), Ok(autorole), Ok(joindm), Ok(cleanup)) =
            (settings, autorole, joindm, cleanup)
        {
            let (embed, mut rows) =
                crate::ui::panels::welcome_panel(&crate::ui::panels::WelcomeView {
                    channel: settings.channel(),
                    avatar_mode: settings.avatar_mode,
                    autorole_enabled: autorole.enabled(),
                    member_roles: autorole.member_roles().len(),
                    join_dm: joindm.join_dm_enabled(),
                    reminder_interval: joindm.interval_days(),
                    // The post renderer has no cache context; the live panel still
                    // exposes all persisted settings and refreshes from the DB.
                    member_count: None,
                    cleanup_enabled: cleanup.enabled(),
                    cleanup_delay_minutes: cleanup.delay_minutes(),
                });
            // JS exposes these as the Welcome Toolkit controls. Keep them on
            // the live post so they stay reachable; their delegates open the
            // same private editors as the hub, while Refresh re-renders this
            // shared post from current DB state.
            rows.push(serenity::CreateActionRow::Buttons(vec![
                crate::ui::components::primary("settings:welcome", "👋 Open Welcome Menu"),
                crate::ui::components::secondary("settings:welcome:channel", "📌 Set Channel"),
                crate::ui::components::secondary("settings:welcome:editor", "✏️ Edit Message"),
                crate::ui::components::secondary("settings:welcome:preview", "👋 Test"),
                crate::ui::components::secondary("atk:welcome:refresh", "🔄 Refresh"),
            ]));
            return (embed, rows);
        }
    }
    if key == "index" {
        if let Ok(settings) = crate::db::server_index::get(&data.db, guild_id).await {
            let join_dm = crate::db::welcome::join_dm(&data.db, guild_id).await.ok();
            return crate::ui::panels::server_index_panel(&crate::ui::panels::ServerIndexView {
                channel: settings.channel(),
                enabled: settings.enabled(),
                include_threads: settings.include_threads(),
                include_voice: settings.include_voice(),
                message_count: settings.message_ids().len(),
                join_dm_enabled: join_dm
                    .as_ref()
                    .is_some_and(|settings| settings.join_dm_enabled()),
                reminder_enabled: join_dm
                    .as_ref()
                    .is_some_and(|settings| settings.reminders_enabled()),
                reminder_interval_days: join_dm
                    .as_ref()
                    .map_or(7, |settings| settings.interval_days()),
            });
        }
    }
    if key == "notifications" {
        // JS declares this panel as a live hub: the post shows the real
        // Notifications hub and only its five destinations. Per-user
        // management screens stay private; `atk:notifications:refresh`
        // re-renders this post from DB state.
        return crate::ui::panels::notifications_toolkit_panel();
    }
    if key == "messages" {
        let groups = crate::db::messages::visible_groups(&data.db, guild_id, 0, true)
            .await
            .unwrap_or_default();
        let mut message_count = 0;
        for group in &groups {
            message_count += crate::db::messages::group_message_count(&data.db, group.id)
                .await
                .unwrap_or(0);
        }
        return crate::ui::panels::messages_toolkit_panel(&crate::ui::panels::MessagesView {
            group_count: groups.len(),
            message_count,
        });
    }
    if key == "voting" {
        if let Ok(settings) = crate::db::voting::settings(&data.db, guild_id).await {
            let active = crate::db::ballots::count_active(&data.db, guild_id)
                .await
                .unwrap_or(0);
            return crate::ui::panels::voting_toolkit_panel(&crate::ui::panels::VotingPanel {
                voting_channel: settings.channel_id,
                results_channel: settings.results_channel_id,
                active,
            });
        }
    }
    if key == "news" {
        let destination = crate::db::wos_relay::destination(&data.db, guild_id)
            .await
            .ok()
            .flatten();
        let hub = crate::db::wos_relay::hub(&data.db)
            .await
            .unwrap_or_default();
        let sneakpeek = crate::db::sneak_peek::settings(&data.db, guild_id)
            .await
            .ok()
            .and_then(|settings| settings.channel());
        return crate::ui::panels::wos_relay_panel(&crate::ui::panels::WosRelayView {
            destination,
            sneakpeek,
            hub_channel: if hub.guild() == Some(guild_id) {
                hub.channel()
            } else {
                None
            },
            hub_configured: hub.channel().is_some(),
            is_owner: false,
        });
    }
    if key == "forum" {
        let list = forum_list(ctx, guild_id).await.unwrap_or_else(|_| {
            "Forum channels could not be loaded right now. Use Refresh to retry.".into()
        });
        let forum_count = ctx
            .cache
            .guild(serenity::GuildId::new(guild_id))
            .map(|guild| {
                guild
                    .channels
                    .values()
                    .filter(|channel| channel.kind == ChannelType::Forum)
                    .count()
            })
            .unwrap_or(0);
        let embed = serenity::CreateEmbed::new()
            .color(registry::COLOR)
            .title(format!(
                "📂 {}",
                registry::get_panel(key)
                    .map(|panel| panel.title)
                    .unwrap_or("FORUM MANAGEMENT")
            ))
            .description(format!(
                "{}\n{}\n\n**Status**\nForum channels: **{}**\n\n**Server forums**\n{}\n{}",
                registry::LINE,
                registry::get_panel(key)
                    .map(|panel| panel.blurb)
                    .unwrap_or(""),
                forum_count,
                list,
                registry::LINE
            ))
            .footer(serenity::CreateEmbedFooter::new(registry::FOOTER));
        let rows = vec![serenity::CreateActionRow::Buttons(vec![
            crate::ui::components::success("atk:forum:create", "➕ Create Forum"),
            crate::ui::components::primary("atk:forum:list", "📋 Forum List"),
            crate::ui::components::secondary("settings:discordtools", "⚙️ Forum Settings"),
            crate::ui::components::secondary("settings:index", "📑 Index Settings"),
            crate::ui::components::secondary("atk:forum:refresh", "🔄 Refresh"),
        ])];
        return (embed, rows);
    }
    // The JavaScript Gift Codes post is the live gift-code menu itself. Render
    // the existing Rust gift-code panel plus its full action set directly in
    // the shared Forum post (no private-only delegation for this panel).
    if key == "gift_codes" {
        let stats = crate::db::giftcodes::stats(&data.db)
            .await
            .unwrap_or_default();
        let recent: Vec<crate::ui::panels::GiftCodeView> =
            crate::db::giftcodes::list_recent(&data.db, 10)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|row| {
                    let source = row.source_label().to_string();
                    let vip = row.is_vip();
                    crate::ui::panels::GiftCodeView {
                        code: row.code,
                        status: row.status,
                        source,
                        added: row.added_at,
                        vip,
                    }
                })
                .collect();
        let sync = crate::db::oracle::state(&data.db)
            .await
            .ok()
            .and_then(|state| state.last_codes_sync);
        let auto_sync = crate::db::settings::get_bool(
            &data.db,
            guild_id,
            crate::db::settings::keys::GIFT_AUTO_SYNC,
            true,
        )
        .await
        .unwrap_or(true);
        let (mut embed, mut rows) =
            crate::ui::panels::giftcode_panel(&stats, &recent, sync, auto_sync);
        let probe_fid = crate::db::settings::get(
            &data.db,
            guild_id,
            crate::db::settings::keys::GIFT_PROBE_FID,
        )
        .await
        .ok()
        .flatten();
        let probe_state = crate::db::settings::get(
            &data.db,
            guild_id,
            crate::db::settings::keys::GIFT_PROBE_STATE,
        )
        .await
        .ok()
        .flatten();
        embed = embed.field(
            "Validation probe",
            match (probe_fid, probe_state) {
                (Some(fid), Some(state)) => format!("Configured player `{fid}` · state `{state}`"),
                _ => "Automatic registered-player fallback".to_string(),
            },
            false,
        );
        use crate::ui::components;
        rows.push(serenity::CreateActionRow::Buttons(vec![
            components::success("gift:add", "🎁 Add Code"),
            components::primary("gift:use", "🧾 Use Code"),
            components::secondary("gift:list", "📋 List Codes"),
            components::secondary("gift:validate", "✅ Validate"),
            components::secondary("gift:history", "📜 History"),
        ]));
        rows.push(serenity::CreateActionRow::Buttons(vec![
            components::danger("gift:cfremove", "🗑️ Remove Code"),
            components::secondary("gift:retry", "🔄 Retry Failed"),
            components::secondary("gift:channel", "📺 Channel"),
            components::secondary("gift:announce", "📢 Announce"),
            components::secondary("settings:back", "◀️ Back"),
        ]));
        rows.push(serenity::CreateActionRow::Buttons(vec![
            components::secondary("gift:probe", "🧪 Configure validation probe"),
            components::danger("gift:handoff", "⚠️ Recover JS Pending Codes"),
        ]));
        rows.push(serenity::CreateActionRow::Buttons(vec![
            components::secondary("gift:template", "✏️ Edit announcement template"),
            components::secondary("gift:template_preview", "👁️ Preview"),
            components::danger("gift:template_reset", "↩️ Reset template"),
        ]));
        return (embed, rows);
    }
    if key == "player_ids" {
        let tracked = crate::db::players::stats(&data.db, guild_id)
            .await
            .map(|stats| stats.tracked)
            .unwrap_or(0);
        let refresh_hours = crate::db::settings::get(
            &data.db,
            guild_id,
            crate::db::settings::keys::PLAYER_ROSTER_REFRESH_HOURS,
        )
        .await
        .ok()
        .flatten()
        .and_then(|hours| hours.parse::<u64>().ok())
        .filter(|hours| *hours > 0);
        let refresh_status = refresh_hours
            .map(|hours| format!("Every **{hours}** hour(s)"))
            .unwrap_or_else(|| "Disabled".into());
        let embed = serenity::CreateEmbed::new()
            .color(0x2ECC71)
            .title("🪪 Player IDs")
            .description(format!(
                "Manage WOS player IDs. The player ID is the stable identifier — names can change.\n{}\n**Status**\nRegistered players: **{}**\nScheduled roster refresh: {}\nLink: Discord user → WOS Player ID → Player name",
                registry::LINE,
                tracked,
                refresh_status
            ))
            .footer(serenity::CreateEmbedFooter::new(registry::FOOTER));
        use crate::ui::components;
        let rows = vec![
            serenity::CreateActionRow::Buttons(vec![
                components::primary("player:list", "📋 Open Player ID Menu"),
                components::success("player:register", "➕ Register ID"),
                components::secondary("player:add", "📥 Batch Add"),
            ]),
            serenity::CreateActionRow::Buttons(vec![
                components::secondary("player:claims", "⏳ Claims"),
                components::secondary("player:limits", "⚙️ ID Limits"),
                components::secondary("player:rolecap", "🎭 Role Caps"),
                components::secondary("player:usercap", "👤 User Caps"),
                crate::ui::components::secondary("settings:back", "◀️ Back"),
            ]),
            serenity::CreateActionRow::Buttons(vec![
                components::secondary("player:export", "📄 Export CSV"),
                components::secondary("player:overcap", "⚠️ Over Cap"),
                components::secondary("player:codes", "🎁 My Codes"),
                components::secondary("player:translate", "🌐 Translate"),
                components::primary("player:deploy", "📣 Deploy Public Panel"),
            ]),
            serenity::CreateActionRow::Buttons(vec![
                components::primary("player:oracle_import", "🔮 Oracle Lookup / Import"),
                components::secondary("player:refresh_roster", "🔄 Refresh Roster"),
                components::secondary("player:oracle_roster", "🗺️ Import State / Alliance"),
                components::secondary("player:roster_schedule", "⏱️ Schedule Refresh"),
            ]),
        ];
        return (embed, rows);
    }
    if key == "lookup" {
        let settings = crate::db::lookup::get(&data.db, guild_id).await.ok();
        let channel_line = settings
            .as_ref()
            .and_then(|settings| settings.channel())
            .map(|id| format!("<#{id}>"))
            .unwrap_or_else(|| "⚠️ not set".to_string());
        let embed = serenity::CreateEmbed::new()
            .color(registry::COLOR)
            .title("🔎 LOOKUP")
            .description(format!(
                "{}\nPrivate player / alliance / state search. Results only visible to the searcher.\n\n**Status**\nOpen Lookup Settings to set the panel channel and post search buttons.\nOr start a private search below.\nPanel channel: {}\n{}",
                registry::LINE,
                channel_line,
                registry::LINE
            ))
            .footer(serenity::CreateEmbedFooter::new(registry::FOOTER));
        use crate::ui::components;
        let rows = vec![serenity::CreateActionRow::Buttons(vec![
            components::primary("settings:lookup", "⚙️ Open Lookup Settings"),
            components::secondary("lookup:player_search", "👤 Search Player"),
            components::secondary("lookup:alliance_search", "🤝 Search Alliance"),
            components::secondary("lookup:state_search", "🏰 Search State"),
            components::secondary("atk:lookup:refresh", "🔄 Refresh"),
        ])];
        return (embed, rows);
    }
    if key == "leveling" {
        // The JavaScript Leveling Toolkit post is a live hub. Render the same
        // hub here; clicks use the ordinary `settings:leveling:*` handlers and
        // stay private via `update_panel`'s is_post path.
        let owner_id = ctx
            .cache
            .guild(serenity::GuildId::new(guild_id))
            .map(|guild| guild.owner_id.get())
            .unwrap_or(0);
        if let Ok(view) =
            crate::commands::interactions::leveling_view_for(ctx, data, guild_id, owner_id).await
        {
            return crate::ui::panels::leveling_toolkit_panel(&view);
        }
    }
    let status = registry::status_lines(&data.db, guild_id, key).await;
    registry::build_payload(panel, &status)
}

/// Create one registered panel post in a forum.
pub async fn create_post(
    ctx: &serenity::Context,
    data: &Data,
    forum_id: u64,
    guild_id: u64,
    key: &str,
) -> anyhow::Result<u64> {
    let panel = registry::get_panel(key).ok_or_else(|| anyhow::anyhow!("unknown panel {key}"))?;
    let (embed, mut rows) = post_payload(ctx, data, guild_id, key).await;
    let form_embed_json = (key == "forms")
        .then(|| crate::translate::stored_json(&embed))
        .flatten();
    let thread = ChannelId::new(forum_id)
        .create_forum_post(
            &ctx.http,
            CreateForumPost::new(
                panel.post_name,
                CreateMessage::new().embed(embed).components(rows.clone()),
            )
            .auto_archive_duration(AutoArchiveDuration::OneWeek)
            .audit_log_reason("Omnicore Admin Toolkit post"),
        )
        .await?;
    if key == "forms" {
        rows.push(crate::translate::translate_row(thread.id.get()));
        if let Err(error) = ChannelId::new(thread.id.get())
            .edit_message(
                &ctx.http,
                serenity::MessageId::new(thread.id.get()),
                EditMessage::new().components(rows),
            )
            .await
        {
            tracing::warn!(guild_id, error = %error, "could not add Forms Toolkit translate control");
        }
        let _ = crate::translate::register(
            &data.db,
            thread.id.get(),
            crate::db::translate::NewTranslatable {
                guild_id: Some(guild_id),
                channel_id: Some(thread.id.get()),
                kind: "forms_admin",
                embed_json: form_embed_json,
                content: None,
                owner_id: None,
            },
        )
        .await;
    }
    Ok(thread.id.get())
}

/// Edit a post's panel message in place (never spams new messages).
pub async fn refresh_post(ctx: &serenity::Context, data: &Data, guild_id: u64, key: &str) -> bool {
    let Some(post_id) = crate::db::admin_toolkit::get_posts(&data.db, guild_id)
        .await
        .ok()
        .and_then(|posts| posts.get(key).copied())
    else {
        return false;
    };
    let Some(post) = fetch_post(ctx, post_id).await else {
        return false;
    };
    let config = match ToolkitConfig::for_guild(&data.db, guild_id).await {
        Ok(Some(config)) => config,
        _ => return false,
    };
    if post.parent_id.map(|id| id.get()) != config.forum_channel_id {
        return false; // stale post id from a different forum
    }
    let (embed, mut rows) = post_payload(ctx, data, guild_id, key).await;
    let form_embed_json = if key == "forms" {
        rows.push(crate::translate::translate_row(post_id));
        crate::translate::stored_json(&embed)
    } else {
        None
    };
    let updated = ChannelId::new(post_id)
        .edit_message(
            &ctx.http,
            serenity::MessageId::new(post_id),
            EditMessage::new().embed(embed).components(rows),
        )
        .await
        .is_ok();
    if updated && key == "forms" {
        let _ = crate::translate::register(
            &data.db,
            post_id,
            crate::db::translate::NewTranslatable {
                guild_id: Some(guild_id),
                channel_id: Some(post_id),
                kind: "forms_admin",
                embed_json: form_embed_json,
                content: None,
                owner_id: None,
            },
        )
        .await;
    }
    updated
}

/// Create anything that is missing and refresh every panel. Configuration is
/// preserved; posts are created in reverse order so the first tool is on top.
pub async fn repair(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    preferred_forum: Option<u64>,
) -> anyhow::Result<RepairResult> {
    let config = ToolkitConfig::for_guild(&data.db, guild_id)
        .await?
        .unwrap_or(ToolkitConfig {
            guild_id: guild_id.to_string(),
            ..Default::default()
        });
    let forum = ensure_forum(ctx, data, guild_id, &config, preferred_forum).await?;
    let mut result = RepairResult {
        forum_id: forum.id.get(),
        ..Default::default()
    };
    let posts = crate::db::admin_toolkit::get_posts(&data.db, guild_id)
        .await
        .unwrap_or_default();
    for key in registry::KEYS.iter().rev() {
        match fetch_post(ctx, posts.get(*key).copied().unwrap_or(0)).await {
            Some(thread) if thread.parent_id.map(|id| id.get()) == Some(result.forum_id) => {
                if refresh_post(ctx, data, guild_id, key).await {
                    result.refreshed.push((*key).to_string());
                }
            }
            _ => match create_post(ctx, data, result.forum_id, guild_id, key).await {
                Ok(post_id) => {
                    let _ =
                        crate::db::admin_toolkit::set_post(&data.db, guild_id, key, post_id).await;
                    result.created.push((*key).to_string());
                }
                Err(err) => result.failed.push(((*key).to_string(), err.to_string())),
            },
        }
    }
    Ok(result)
}

/// Recreate the complete Toolkit structure. Only Toolkit-owned posts are
/// deleted — nothing else in the forum or server is touched.
pub async fn rebuild(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
) -> anyhow::Result<RepairResult> {
    let posts = crate::db::admin_toolkit::get_posts(&data.db, guild_id)
        .await
        .unwrap_or_default();
    for post_id in posts.values() {
        if fetch_post(ctx, *post_id).await.is_some() {
            let _ = ChannelId::new(*post_id).delete(&ctx.http).await;
        }
    }
    let _ = crate::db::admin_toolkit::clear_posts(&data.db, guild_id).await;
    repair(ctx, data, guild_id, None).await
}

pub async fn refresh_all(ctx: &serenity::Context, data: &Data, guild_id: u64) -> usize {
    let mut count = 0;
    for key in registry::KEYS {
        if refresh_post(ctx, data, guild_id, key).await {
            count += 1;
        }
    }
    count
}

/// Debounced self-heal after a Toolkit post or forum channel is deleted.
/// Event-driven only — no periodic refresh loop is introduced.
pub fn schedule_repair(ctx: serenity::Context, data: crate::state::Data, guild_id: u64) {
    static JOBS: OnceLock<Mutex<HashMap<u64, tokio::task::JoinHandle<()>>>> = OnceLock::new();
    let jobs = JOBS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut jobs) = jobs.lock() {
        if let Some(previous) = jobs.remove(&guild_id) {
            previous.abort();
        }
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(4)).await;
            match repair(&ctx, &data, guild_id, None).await {
                Ok(result) => tracing::info!(
                    guild = guild_id,
                    created = result.created.len(),
                    "toolkit auto-repair after deletion"
                ),
                Err(err) => {
                    tracing::warn!(guild = guild_id, error = %err, "toolkit auto-repair failed")
                }
            }
        });
        jobs.insert(guild_id, handle);
    }
}

/// Create an ordinary forum channel from the Forum Management panel modal.
pub async fn create_forum_modal(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    interaction: &serenity::ModalInteraction,
) -> anyhow::Result<()> {
    let Some(guild_id) = interaction.guild_id.map(|id| id.get()) else {
        anyhow::bail!("Use the Admin Toolkit inside a server");
    };
    if !crate::permissions::modal_is_admin(ctx, data, interaction).await
        && !crate::admintoolkit::can_configure_modal(ctx, interaction)
    {
        anyhow::bail!("R4/R5 permission required to create a forum here");
    }
    let name = interaction
        .data
        .components
        .iter()
        .flat_map(|row| row.components.iter())
        .find_map(|component| match component {
            serenity::ActionRowComponent::InputText(input) if input.custom_id == "name" => {
                input.value.as_deref()
            }
            _ => None,
        })
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        anyhow::bail!("Forum name cannot be empty");
    }
    let reason = format!("Admin Toolkit: created by {}", interaction.user.id.get());
    let channel = serenity::GuildId::new(guild_id)
        .create_channel(
            &ctx.http,
            CreateChannel::new(name)
                .kind(ChannelType::Forum)
                .audit_log_reason(&reason),
        )
        .await?;
    super::log_audit(
        ctx,
        data,
        guild_id,
        interaction.user.id.get(),
        Some("forum"),
        "create",
        Some(&format!("Created forum #{}", channel.name)),
    )
    .await;
    interaction
        .create_response(
            &ctx.http,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .content(format!("✅ Forum created: <#{}>", channel.id.get()))
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

/// List the guild's forum channels for the Forum Management panel.
pub async fn forum_list(ctx: &serenity::Context, guild_id: u64) -> anyhow::Result<String> {
    let forums: Vec<_> = {
        let guild = ctx
            .cache
            .guild(serenity::GuildId::new(guild_id))
            .ok_or_else(|| anyhow::anyhow!("guild not in cache"))?;
        guild
            .channels
            .values()
            .filter(|channel| channel.kind == ChannelType::Forum)
            .map(|channel| channel.id)
            .collect()
    };
    if forums.is_empty() {
        return Ok("No forum channels in this server.".to_string());
    }
    let active_threads = serenity::GuildId::new(guild_id)
        .get_active_threads(&ctx.http)
        .await
        .ok()
        .map(|data| data.threads);
    let mut lines = Vec::with_capacity(forums.len());
    for channel in forums {
        let count = active_threads.as_ref().map(|threads| {
            threads
                .iter()
                .filter(|thread| thread.parent_id == Some(channel))
                .count()
        });
        let count = count.map_or_else(|| "?".to_string(), |count| count.to_string());
        lines.push(format!(
            "📂 <#{}> — **{}** active post(s)",
            channel.get(),
            count
        ));
    }
    Ok(format!("**Forums**\n{}", lines.join("\n")))
}

/// Startup validation and recovery. Never throws — a broken Toolkit must not
/// stop the bot from starting.
pub async fn startup_repair(ctx: &serenity::Context, data: &Data) {
    let guilds = crate::db::admin_toolkit::list_toolkit_guilds(&data.db)
        .await
        .unwrap_or_default();
    for guild_id in guilds {
        match repair(ctx, data, guild_id, None).await {
            Ok(result) => tracing::info!(
                guild = guild_id,
                created = result.created.len(),
                refreshed = result.refreshed.len(),
                failed = result.failed.len(),
                "toolkit startup repair"
            ),
            Err(err) => {
                tracing::warn!(guild = guild_id, error = %err, "toolkit startup repair skipped")
            }
        }
    }
}

/// Format a repair result for ephemeral display (mirrors `formatResultText`).
pub fn format_result(result: &RepairResult, title: &str) -> String {
    let mut text = format!(
        "{title}\n**Forum:** <#{}> · created {}, refreshed {}",
        result.forum_id,
        result.created.len(),
        result.refreshed.len()
    );
    if !result.failed.is_empty() {
        let keys: Vec<&str> = result.failed.iter().map(|(key, _)| key.as_str()).collect();
        text.push_str(&format!(
            "\n⚠️ {} panel(s) failed: {}",
            result.failed.len(),
            keys.join(", ")
        ));
    }
    text.chars().take(1900).collect()
}

/// Audit channel messages suppress mentions; helper kept here so callers do
/// not have to import serenity mention builders.
pub fn audit_mentions() -> CreateAllowedMentions {
    CreateAllowedMentions::new()
}
