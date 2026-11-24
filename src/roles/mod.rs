//! Role Assignment: a persistent staff-only panel in a configured channel.
//!
//! Flow: staff member with permission (bot admin, or granted for a specific
//! catalog role) clicks **Assign Role** → picks members via a user select →
//! picks roles from their permitted set → confirms Grant or Remove.
//! Picks are kept in a short-lived in-memory session keyed by a random sid.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, CreateActionRow, CreateEmbed};

use crate::state::Data;

const SESSION_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub struct AssignSession {
    pub guild_id: u64,
    pub actor_id: u64,
    pub created_ms: u128,
    pub target_user_ids: Vec<u64>,
    pub role_ids: Vec<u64>,
}

fn sessions() -> &'static Mutex<HashMap<String, AssignSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, AssignSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn new_session(guild_id: u64, actor_id: u64) -> String {
    use rand::Rng;
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    // Evict expired sessions while we're in here anyway.
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    guard.retain(|_, session| now.saturating_sub(session.created_ms) < SESSION_TTL.as_millis());

    let sid = hex_id(&rand::thread_rng().gen::<[u8; 16]>());
    guard.insert(
        sid.clone(),
        AssignSession {
            guild_id,
            actor_id,
            created_ms: now,
            target_user_ids: Vec::new(),
            role_ids: Vec::new(),
        },
    );
    sid
}

fn hex_id(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn get_session(sid: &str) -> Option<AssignSession> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(sid)
        .filter(|session| now.saturating_sub(session.created_ms) < SESSION_TTL.as_millis())
        .cloned()
}

pub fn update_session<F>(sid: &str, update: F)
where
    F: FnOnce(&mut AssignSession),
{
    if let Some(session) = sessions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(sid)
    {
        update(session);
    }
}

pub fn drop_session(sid: &str) {
    sessions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(sid);
}

/// The persistent panel: one button to start the flow, one translate button.
pub const PANEL_TITLE: &str = "🏷️ Role Assignment";
pub const PANEL_DESCRIPTION: &str = "This panel is for **staff use only** — members can't self-assign roles here.\n\n🌐 **Translate** — private guide in your language (only you see it)\n🏷️ **Assign Role** — staff with permission for a specific role can grant/remove it on one or more members";

pub fn panel_embed() -> CreateEmbed {
    CreateEmbed::new()
        .title(PANEL_TITLE)
        .description(PANEL_DESCRIPTION)
        .color(0x5865f2)
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn panel_row(message_id: u64) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        crate::ui::components::primary("role:assign", "Assign Role"),
        crate::translate::translate_button(message_id),
    ])
}

/// Post (or refresh) the persistent panel message.
pub async fn post_panel(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    channel_id: u64,
) -> anyhow::Result<u64> {
    let target = ChannelId::new(channel_id);
    // Send first so we know the message id for the translate row.
    let message = target
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new().embed(panel_embed()),
        )
        .await?;
    target
        .edit_message(
            &ctx.http,
            message.id,
            serenity::EditMessage::new().components(vec![panel_row(message.id.get())]),
        )
        .await?;

    let stored_json = crate::translate::stored_json(&panel_embed());
    crate::translate::register(
        &data.db,
        message.id.get(),
        crate::db::translate::NewTranslatable {
            guild_id: Some(guild_id),
            channel_id: Some(channel_id),
            kind: "role_panel",
            embed_json: stored_json,
            content: None,
            owner_id: None,
        },
    )
    .await?;

    crate::db::roles::set_panel(&data.db, guild_id, channel_id, message.id.get()).await?;
    Ok(message.id.get())
}

/// Actor's permission slice for the flow.
pub async fn access_for(
    data: &Data,
    guild_id: u64,
    actor_id: u64,
    actor_role_ids: &[u64],
    is_admin: bool,
) -> anyhow::Result<Vec<u64>> {
    crate::db::roles::assignable_for_actor(&data.db, guild_id, actor_id, actor_role_ids, is_admin)
        .await
}

pub fn actor_role_ids(interaction: &serenity::ComponentInteraction) -> Vec<u64> {
    interaction
        .member
        .as_ref()
        .map(|member| member.roles.iter().map(|id| id.get()).collect())
        .unwrap_or_default()
}
