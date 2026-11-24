//! R4/R5 Admin Toolkit: a Discord forum acting as the admin control surface.
//!
//! Every post in the forum is a panel over an existing Omnicore feature
//! (Welcome, Leveling, Tickets, WoS Voting, …). Opening a panel from a post
//! shows the relevant settings hub **ephemerally** to the clicker — the
//! shared forum post is never rewritten by a normal admin action.
//!
//! Elevation: members holding the configured R5 (or R4) Discord role may run
//! Toolkit actions, but only when the interaction originates from a Toolkit
//! post or from an ephemeral hub it opened. Everything else stays behind the
//! normal permission rules.

pub mod forum;
pub mod registry;
pub mod settings;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, CreateAllowedMentions, CreateEmbed, Permissions};

use crate::db;

/// In-memory TTL for ephemeral hub messages spawned from Toolkit posts.
const ELEVATION_TTL_MS: u64 = 30 * 60 * 1000;
const TRACKED_MAX: usize = 2000;

#[derive(Debug, Clone)]
struct TrackedMessage {
    user_id: u64,
    guild_id: u64,
    tool: Option<String>,
    expiry_ms: u64,
}

fn tracked() -> &'static Mutex<HashMap<u64, TrackedMessage>> {
    static TRACKED: OnceLock<Mutex<HashMap<u64, TrackedMessage>>> = OnceLock::new();
    TRACKED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn prune(map: &mut HashMap<u64, TrackedMessage>, now: u64) {
    map.retain(|_, entry| entry.expiry_ms > now);
    if map.len() > TRACKED_MAX {
        let mut entries: Vec<_> = map.iter().map(|(id, e)| (*id, e.expiry_ms)).collect();
        entries.sort_by_key(|(_, exp)| *exp);
        for (id, _) in entries.into_iter().take(map.len() - TRACKED_MAX) {
            map.remove(&id);
        }
    }
}

/// Track the ephemeral response produced by a Toolkit action, scoped to the
/// clicking user and guild (a different member or guild can never inherit it).
pub fn track_ephemeral_for(message_id: u64, user_id: u64, guild_id: u64, tool: Option<&str>) {
    if message_id == 0 {
        return;
    }
    let now = now_ms();
    if let Ok(mut guard) = tracked().lock() {
        prune(&mut guard, now);
        guard.insert(
            message_id,
            TrackedMessage {
                user_id,
                guild_id,
                tool: tool.map(str::to_string),
                expiry_ms: now + ELEVATION_TTL_MS,
            },
        );
    }
}

/// Legacy helper used by `update_panel` / `update_response`: track without
/// knowing user/guild (scoped to all contexts by message id only).
pub fn track_ephemeral(message_id: u64) {
    if let Ok(mut guard) = tracked().lock() {
        let now = now_ms();
        prune(&mut guard, now);
        guard.insert(
            message_id,
            TrackedMessage {
                user_id: 0,
                guild_id: 0,
                tool: None,
                expiry_ms: now + ELEVATION_TTL_MS,
            },
        );
    }
}

pub fn is_tracked(message_id: u64) -> bool {
    tracked()
        .lock()
        .map(|guard| {
            guard
                .get(&message_id)
                .is_some_and(|e| e.expiry_ms > now_ms())
        })
        .unwrap_or(false)
}

/// Which Toolkit panel opened this private message for this user/guild?
pub fn tool_of(message_id: u64, user_id: u64, guild_id: u64) -> Option<String> {
    let now = now_ms();
    tracked().lock().ok()?.get(&message_id).and_then(|entry| {
        if entry.expiry_ms > now
            && (entry.user_id == 0 || entry.user_id == user_id)
            && (entry.guild_id == 0 || entry.guild_id == guild_id)
        {
            entry.tool.clone()
        } else {
            None
        }
    })
}

/// Effective R4/R5 (or higher) level for a member in a guild.
pub async fn level_for(
    db: &db::Db,
    guild_id: u64,
    role_ids: &[u64],
    owner_id: u64,
) -> db::admin_toolkit::Level {
    use db::admin_toolkit::Level;
    if owner_id == data_owner_id(db).await {
        return Level::Owner;
    }
    let config = match db::admin_toolkit::ToolkitConfig::for_guild(db, guild_id).await {
        Ok(Some(config)) => config,
        _ => return Level::None,
    };
    if !config.enabled {
        return Level::None;
    }
    config.matches_role(role_ids)
}

async fn data_owner_id(db: &db::Db) -> u64 {
    db::admin::owner_id(db).await.ok().flatten().unwrap_or(0)
}

/// Can this member configure the Toolkit? Bot owner/admin, guild owner or
/// Administrator permission — never an R4/R5 role alone.
pub fn can_configure(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
) -> bool {
    let Some(guild_id) = interaction.guild_id else {
        return false;
    };
    let user_id = interaction.user.id.get();
    if ctx.cache.current_user().id == interaction.user.id {
        return false;
    }
    let guild = match ctx.cache.guild(guild_id) {
        Some(guild) => guild,
        None => return false,
    };
    if guild.owner_id.get() == user_id {
        return true;
    }
    let Some(member) = interaction.member.as_ref() else {
        return false;
    };
    guild
        .member_permissions(member)
        .contains(Permissions::ADMINISTRATOR)
        || crate::permissions::component_is_admin_blocking_member(&guild, member)
}

/// Modal variant of the configuration gate (used by the forum-create modal).
pub fn can_configure_modal(
    ctx: &serenity::Context,
    interaction: &serenity::ModalInteraction,
) -> bool {
    let Some(guild_id) = interaction.guild_id else {
        return false;
    };
    let Some(guild) = ctx.cache.guild(guild_id) else {
        return false;
    };
    if guild.owner_id == interaction.user.id {
        return true;
    }
    interaction.member.as_ref().is_some_and(|member| {
        guild
            .member_permissions(member)
            .contains(Permissions::ADMINISTRATOR)
    })
}

/// Role ids the acting member holds.
pub fn member_role_ids(interaction: &serenity::ComponentInteraction) -> Vec<u64> {
    interaction
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|id| id.get()).collect())
        .unwrap_or_default()
}

pub fn modal_role_ids(interaction: &serenity::ModalInteraction) -> Vec<u64> {
    interaction
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|id| id.get()).collect())
        .unwrap_or_default()
}

/// Elevated (R4/R5) access for a raw component interaction.
pub async fn elevates_component(db: &db::Db, interaction: &serenity::ComponentInteraction) -> bool {
    let Some(guild_id) = interaction.guild_id else {
        return false;
    };
    let roles = member_role_ids(interaction);
    if elevates(
        db,
        guild_id.get(),
        interaction.user.id.get(),
        interaction.message.id.get(),
        &roles,
    )
    .await
    {
        return true;
    }
    // Forum posts are represented by their thread channel ID in our storage;
    // component interactions carry the starter message ID separately.
    elevates(
        db,
        guild_id.get(),
        interaction.user.id.get(),
        interaction.channel_id.get(),
        &roles,
    )
    .await
}

/// Elevated (R4/R5) access for a raw modal interaction.
pub async fn elevates_modal(db: &db::Db, interaction: &serenity::ModalInteraction) -> bool {
    let Some(guild_id) = interaction.guild_id else {
        return false;
    };
    let Some(message) = interaction.message.as_ref() else {
        return false;
    };
    let roles = modal_role_ids(interaction);
    if elevates(
        db,
        guild_id.get(),
        interaction.user.id.get(),
        message.id.get(),
        &roles,
    )
    .await
    {
        return true;
    }
    // Modal submits opened from a Toolkit forum thread carry the starter
    // message ID; our per-tool registry stores the thread channel ID.
    elevates(
        db,
        guild_id.get(),
        interaction.user.id.get(),
        interaction.channel_id.get(),
        &roles,
    )
    .await
}

async fn elevates(
    db: &db::Db,
    guild_id: u64,
    user_id: u64,
    message_id: u64,
    role_ids: &[u64],
) -> bool {
    if role_ids.is_empty() {
        return false;
    }
    let owner = db::admin::owner_id(db).await.ok().flatten().unwrap_or(0);
    let level = level_for(db, guild_id, role_ids, owner).await;
    if level == db::admin_toolkit::Level::None {
        return false;
    }
    // Elevate only on toolkit posts, or on ephemeral hubs spawned by this
    // user in this guild.
    if let Ok(Some(entry)) = tracked()
        .lock()
        .map(|guard| guard.get(&message_id).cloned())
    {
        if (entry.user_id == 0 || entry.user_id == user_id)
            && (entry.guild_id == 0 || entry.guild_id == guild_id)
            && entry.expiry_ms > now_ms()
        {
            return true;
        }
    }
    matches!(db::admin_toolkit::find_post(db, message_id).await, Ok(Some((post_guild, _))) if post_guild == guild_id)
}

/// Debounced shared-post refresh after a Toolkit-originated mutation, matching
/// JS `permissions.js:scheduleRefresh`. Limited to Welcome and Leveling posts;
/// other panels keep their existing explicit-Refresh behavior.
pub fn schedule_post_refresh(
    ctx: serenity::Context,
    data: crate::state::Data,
    guild_id: u64,
    tool: String,
) {
    if tool != "welcome" && tool != "leveling" {
        return;
    }
    static PENDING: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashSet<(u64, String)>>,
    > = std::sync::OnceLock::new();
    let pending = PENDING.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    let key = (guild_id, tool.clone());
    let insert = pending
        .lock()
        .map(|mut guard| guard.insert(key.clone()))
        .unwrap_or(false);
    if !insert {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        if let Ok(mut guard) = pending.lock() {
            guard.remove(&key);
        }
        let _ = forum::refresh_post(&ctx, &data, guild_id, &tool).await;
    });
}

/// Modal-originated toolkit actions don't go through `update_panel`; resolve
/// the source post the same way elevation does and schedule its refresh.
pub async fn maybe_schedule_post_refresh(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    guild_id: u64,
    user_id: u64,
    message_id: Option<u64>,
) {
    let Some(message_id) = message_id else {
        return;
    };
    let tool = crate::db::admin_toolkit::find_post(&data.db, message_id)
        .await
        .ok()
        .flatten()
        .map(|(_, tool)| tool)
        .or_else(|| tool_of(message_id, user_id, guild_id));
    if let Some(tool) = tool {
        schedule_post_refresh(ctx.clone(), data.clone(), guild_id, tool);
    }
}

/// Store an audit entry and optionally mirror it into the configured private
/// audit channel. Mentions are suppressed in the channel copy.
pub async fn log_audit(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    guild_id: u64,
    user_id: u64,
    tool_key: Option<&str>,
    action: &str,
    detail: Option<&str>,
) {
    let _ =
        db::admin_toolkit::add_audit(&data.db, guild_id, user_id, tool_key, action, detail).await;
    let config = match db::admin_toolkit::ToolkitConfig::for_guild(&data.db, guild_id).await {
        Ok(Some(config)) => config,
        _ => return,
    };
    let Some(channel_id) = config.log_channel_id else {
        return;
    };
    let description = format!(
        "👤 <@{user_id}> · **{action}**{}{}",
        tool_key
            .map(|tool| format!(" · `{tool}`"))
            .unwrap_or_default(),
        detail
            .filter(|detail| !detail.is_empty())
            .map(|detail| format!("\n{}", detail.chars().take(400).collect::<String>()))
            .unwrap_or_default()
    );
    let embed = CreateEmbed::new()
        .color(0x5865F2)
        .description(description)
        .timestamp(serenity::Timestamp::now());
    let _ = ChannelId::new(channel_id)
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .embed(embed)
                .allowed_mentions(CreateAllowedMentions::new()),
        )
        .await;
}

/// Handle a toolkit action button by delegating to the existing Rust feature
/// handler. `delegate` is a regular custom id such as `settings:welcome`.
/// Boxed to break the async recursion cycle through the component router.
pub fn delegate_action<'a>(
    ctx: &'a serenity::Context,
    data: &'a crate::state::Data,
    interaction: &'a serenity::ComponentInteraction,
    delegate: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>> {
    Box::pin(crate::commands::interactions::route_component(
        ctx,
        data,
        interaction,
        delegate,
    ))
}
