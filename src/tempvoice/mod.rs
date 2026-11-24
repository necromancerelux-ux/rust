//! "Join to create" voice channels (tempvoice).
//!
//! Joining the lobby creates a voice channel for that member and moves them
//! in. Empty channels are deleted; if the owner leaves but others stay,
//! ownership transfers to another member in the channel.
//!
//! Owner controls live in the TempVoice Interface panel posted in the
//! channel's chat (the `tvc:*` button grid): name, limit, privacy, waiting
//! room, linked text chat, trust/kick/block, invite, region, claim,
//! transfer, delete.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, ChannelType, Permissions, UserId};

use crate::db;
use crate::state::Data;

const COOLDOWN: Duration = Duration::from_millis(5_000);
static LAST_CREATE: LazyLock<tokio::sync::Mutex<HashMap<u64, Instant>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(HashMap::new()));

pub const OWNER_ALLOW: Permissions = Permissions::CONNECT
    .union(Permissions::SPEAK)
    .union(Permissions::VIEW_CHANNEL)
    .union(Permissions::MANAGE_CHANNELS)
    .union(Permissions::MOVE_MEMBERS)
    .union(Permissions::MUTE_MEMBERS)
    .union(Permissions::DEAFEN_MEMBERS);

/// The "TempVoice Interface" control panel posted in the channel's chat.
/// Matches the legacy embed + three rows of icon-only buttons.
pub fn build_control_panel(
    owner_id: u64,
    locked: bool,
) -> (serenity::CreateEmbed, Vec<serenity::CreateActionRow>) {
    use serenity::{CreateActionRow, CreateButton};

    let embed = serenity::CreateEmbed::new()
        .color(0xe74c3c)
        .title("TempVoice Interface")
        .description(format!(
            concat!(
                "This **interface** can be used to manage temporary voice channels.\n",
                "More options are available with `/voice` commands.\n\n",
                "✅ **NAME**   🔧 **LIMIT**   🔊 **PRIVACY**   🚶 **WAITING ROOM**   💬 **CHAT**\n",
                "🟢 **TRUST**   ⛔ **UNTRUST**   🔗 **INVITE**   👢 **KICK**   🌍 **REGION**\n",
                "🚫 **BLOCK**   🆓 **UNBLOCK**   👑 **CLAIM**   🔁 **TRANSFER**   🗑️ **DELETE**\n\n",
                "**Owner:** <@{}>  ·  **Status:** {}\nPress the buttons below to use the interface"
            ),
            owner_id,
            if locked { "🔒 Locked" } else { "🔓 Open" },
        ));

    let secondary = serenity::ButtonStyle::Secondary;
    let btn = |id: &str, emoji: &str, style: serenity::ButtonStyle| {
        CreateButton::new(id)
            .style(style)
            .emoji(serenity::ReactionType::Unicode(emoji.to_string()))
    };

    let row1 = CreateActionRow::Buttons(vec![
        btn("tvc:name", "✏️", secondary),
        btn("tvc:limit", "🔧", secondary),
        btn(
            "tvc:privacy",
            "🔊",
            if locked {
                serenity::ButtonStyle::Success
            } else {
                secondary
            },
        ),
        btn("tvc:waiting", "🚶", secondary),
        btn("tvc:chat", "💬", secondary),
    ]);
    let row2 = CreateActionRow::Buttons(vec![
        btn("tvc:trust", "✅", secondary),
        btn("tvc:untrust", "⛔", secondary),
        btn("tvc:invite", "🔗", secondary),
        btn("tvc:kick", "👢", secondary),
        btn("tvc:region", "🌍", secondary),
    ]);
    let row3 = CreateActionRow::Buttons(vec![
        btn("tvc:block", "🚫", secondary),
        btn("tvc:unblock", "🆓", secondary),
        btn("tvc:claim", "👑", serenity::ButtonStyle::Primary),
        btn("tvc:transfer", "🔁", secondary),
        btn("tvc:delete", "🗑️", serenity::ButtonStyle::Danger),
    ]);

    (embed, vec![row1, row2, row3])
}

/// Delete a temp channel and its linked chat / waiting room.
pub async fn delete_temp(ctx: &serenity::Context, data: &Data, channel_id: u64, reason: &str) {
    let row = db::tempvoice::get_channel(&data.db, channel_id)
        .await
        .ok()
        .flatten();
    if let Some(row) = &row {
        if let Some(chat) = row.chat_channel_id() {
            let _ = ctx
                .http
                .delete_channel(ChannelId::new(chat), Some("Temp voice chat cleanup"))
                .await;
        }
        if let Some(waiting) = row.waiting_channel_id() {
            let _ = ctx
                .http
                .delete_channel(ChannelId::new(waiting), Some("Temp voice waiting cleanup"))
                .await;
        }
    }
    let _ = db::tempvoice::remove_channel(&data.db, channel_id).await;
    let _ = ctx
        .http
        .delete_channel(ChannelId::new(channel_id), Some(reason))
        .await;
}

/// RTC regions for the Region picker.
pub const REGIONS: &[(&str, &str)] = &[
    ("Automatic", "null"),
    ("Brazil", "brazil"),
    ("Hong Kong", "hongkong"),
    ("India", "india"),
    ("Japan", "japan"),
    ("Rotterdam", "rotterdam"),
    ("Singapore", "singapore"),
    ("South Africa", "southafrica"),
    ("Sydney", "sydney"),
    ("US Central", "us-central"),
    ("US East", "us-east"),
    ("US South", "us-south"),
    ("US West", "us-west"),
];

pub async fn on_voice_state_update(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    user_id: u64,
    old_channel: Option<u64>,
    new_channel: Option<u64>,
) {
    // Left a temp channel → clean up / transfer ownership.
    if let Some(old_id) = old_channel {
        if old_channel != new_channel {
            if let Some(temp) = db::tempvoice::get_channel(&data.db, old_id)
                .await
                .ok()
                .flatten()
            {
                handle_leave(ctx, data, old_id, user_id, &temp).await;
                return;
            }
        }
    }

    // Joined a waiting room → hop into the main temp channel if permitted.
    if let Some(waiting_id) = new_channel {
        if let Some(row) = db::tempvoice::get_by_waiting_channel(&data.db, waiting_id)
            .await
            .ok()
            .flatten()
        {
            if let Some(main_id) = row.channel_id() {
                if let Some(member) = ctx
                    .cache
                    .guild(guild_id)
                    .and_then(|guild| guild.members.get(&UserId::new(user_id)).cloned())
                {
                    let _ = member
                        .move_to_voice_channel(&ctx.http, ChannelId::new(main_id))
                        .await;
                }
            }
            return;
        }
    }

    // Joined the lobby → create a channel.
    let Some(new_id) = new_channel else {
        return;
    };
    if old_channel == Some(new_id) {
        return;
    }

    let Ok(settings) = db::tempvoice::settings(&data.db, guild_id).await else {
        return;
    };
    if !settings.enabled() || settings.lobby_channel() != Some(new_id) {
        return;
    }
    // Bots don't get lobby-created channels.
    if ctx
        .cache
        .guild(guild_id)
        .and_then(|guild| {
            guild
                .members
                .get(&UserId::new(user_id))
                .map(|member| member.user.bot)
        })
        .unwrap_or(false)
    {
        return;
    }

    {
        let mut last = LAST_CREATE.lock().await;
        if let Some(previous) = last.get(&user_id) {
            if previous.elapsed() < COOLDOWN {
                return;
            }
        }
        last.insert(user_id, Instant::now());
    }

    if let Err(err) = create_for(ctx, data, guild_id, user_id, &settings).await {
        tracing::warn!(error = %err, guild = guild_id, user = user_id, "temp voice creation failed");
    }
}

async fn create_for(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    user_id: u64,
    settings: &db::tempvoice::TempVoiceSettings,
) -> anyhow::Result<()> {
    let display_name = ctx
        .cache
        .guild(guild_id)
        .and_then(|guild| {
            guild
                .members
                .get(&UserId::new(user_id))
                .map(|m| m.display_name().to_string())
        })
        .unwrap_or_else(|| format!("Voice-{user_id}"));

    let count = db::tempvoice::count_for_guild(&data.db, guild_id)
        .await
        .unwrap_or(0);
    let name = settings
        .name_template()
        .replace("{user}", &display_name)
        .replace("{count}", &(count + 1).to_string())
        .trim()
        .chars()
        .take(100)
        .collect::<String>();

    let category = settings.category().or_else(|| {
        ctx.cache.guild(guild_id).and_then(|guild| {
            guild
                .channels
                .get(&ChannelId::new(settings.lobby_channel().unwrap_or(0)))
                .and_then(|channel| channel.parent_id.map(|id| id.get()))
        })
    });

    let mut builder = serenity::CreateChannel::new(&name)
        .kind(ChannelType::Voice)
        .user_limit(settings.user_limit())
        .permissions(vec![serenity::PermissionOverwrite {
            allow: OWNER_ALLOW,
            deny: Permissions::empty(),
            kind: serenity::PermissionOverwriteType::Member(UserId::new(user_id)),
        }])
        .audit_log_reason("Temp voice for lobby join");

    if let Some(category_id) = category {
        builder = builder.category(ChannelId::new(category_id));
    }

    let created = serenity::GuildId::new(guild_id)
        .create_channel(&ctx.http, builder)
        .await?;

    db::tempvoice::add_channel(&data.db, guild_id, created.id.get(), user_id).await?;

    // Move the member into their fresh channel.
    if let Some(member) = ctx
        .cache
        .guild(guild_id)
        .and_then(|guild| guild.members.get(&UserId::new(user_id)).cloned())
    {
        if let Err(_err) = member.move_to_voice_channel(&ctx.http, created.id).await {
            // They left the lobby before we could move them — nothing to keep.
            delete_temp(ctx, data, created.id.get(), "owner left before move").await;
            return Ok(());
        }
    }

    // Post the TempVoice Interface panel in the channel's chat.
    let (embed, rows) = build_control_panel(user_id, false);
    let _ = created
        .id
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .content(format!("<@{user_id}>"))
                .embed(embed)
                .components(rows),
        )
        .await;

    Ok(())
}

async fn handle_leave(
    ctx: &serenity::Context,
    data: &Data,
    channel_id: u64,
    member_id: u64,
    temp: &db::tempvoice::TempVoiceChannel,
) {
    // Count humans remaining in the channel (from cache; best effort).
    let remaining = ctx
        .cache
        .guild(temp.guild_id.parse::<u64>().unwrap_or(0))
        .map(|guild| {
            guild
                .voice_states
                .values()
                .filter(|state| state.channel_id.map(|c| c.get()) == Some(channel_id))
                .count()
        })
        .unwrap_or(1); // cache miss → leave the channel alone

    if remaining == 0 {
        delete_temp(ctx, data, channel_id, "Temp voice empty").await;
        tracing::info!(channel = channel_id, "deleted empty temp voice");
        return;
    }

    if temp.owner_id() == Some(member_id) {
        transfer_owner(ctx, data, channel_id, &temp.guild_id).await;
    }
}

async fn transfer_owner(ctx: &serenity::Context, data: &Data, channel_id: u64, guild_id_str: &str) {
    let guild_id = guild_id_str.parse::<u64>().unwrap_or(0);

    // Pick another member currently in the channel (cache best effort).
    let next = ctx.cache.guild(guild_id).and_then(|guild| {
        guild.voice_states.iter().find_map(|(user, state)| {
            (state.channel_id.map(|c| c.get()) == Some(channel_id)).then_some(*user)
        })
    });

    let Some(next_user) = next else {
        return;
    };

    if let Err(err) = db::tempvoice::set_owner(&data.db, channel_id, next_user.get()).await {
        tracing::debug!(error = %err, channel = channel_id, "tempvoice owner update failed");
        return;
    }

    let _ = ChannelId::new(channel_id)
        .create_permission(
            &ctx.http,
            serenity::PermissionOverwrite {
                allow: OWNER_ALLOW,
                deny: Permissions::empty(),
                kind: serenity::PermissionOverwriteType::Member(next_user),
            },
        )
        .await;
}
