//! Toolkit panel registry. Keys and order mirror the JavaScript registry.
//! Each action delegates to an existing Rust feature handler (via its regular
//! custom id) rather than duplicating that feature's settings logic.

use poise::serenity_prelude as serenity;
use serenity::{ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, ReactionType};

use crate::db;
use crate::db::admin_toolkit::Level;

pub const KEYS: &[&str] = &[
    "welcome",
    "index",
    "player_ids",
    "gift_codes",
    "roles",
    "leveling",
    "tickets",
    "voting",
    "forum",
    "news",
    "forms",
    "notifications",
    "messages",
    "calculators",
    "giveaways",
    "lookup",
];

pub const REFRESH: Action = Action {
    id: "refresh",
    label: "Refresh",
    emoji: "🔄",
    style: ButtonStyle::Secondary,
    min_level: Level::R4,
    delegate: "",
    confirm: None,
};

#[derive(Debug, Clone, Copy)]
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub emoji: &'static str,
    pub style: ButtonStyle,
    pub min_level: Level,
    /// Regular custom id forwarded to the existing Rust handler.
    pub delegate: &'static str,
    pub confirm: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub struct Panel {
    pub key: &'static str,
    pub title: &'static str,
    pub post_name: &'static str,
    pub emoji: &'static str,
    pub blurb: &'static str,
    pub rows: &'static [&'static [Action]],
}

const WELCOME_ROW: &[Action] = &[
    Action {
        id: "settings",
        label: "Open Welcome Menu",
        emoji: "👋",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:welcome",
        confirm: None,
    },
    Action {
        id: "setchannel",
        label: "Set Channel",
        emoji: "📌",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:welcome:channel",
        confirm: None,
    },
    Action {
        id: "editmsg",
        label: "Edit Message",
        emoji: "✏️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:welcome:editor",
        confirm: None,
    },
    Action {
        id: "test",
        label: "Test",
        emoji: "👋",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:welcome:preview",
        confirm: None,
    },
    REFRESH,
];
const INDEX_ROW: &[Action] = &[
    Action {
        id: "settings",
        label: "Open Index Menu",
        emoji: "📑",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:index",
        confirm: None,
    },
    Action {
        id: "rebuild",
        label: "Refresh Index",
        emoji: "🛠️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:index",
        confirm: None,
    },
    Action {
        id: "setchannel",
        label: "Set Index Channel",
        emoji: "📌",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:index",
        confirm: None,
    },
    Action {
        id: "toggle",
        label: "Toggle On/Off",
        emoji: "🔘",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:index",
        confirm: None,
    },
    REFRESH,
];
const PLAYER_ROW1: &[Action] = &[
    Action {
        id: "list",
        label: "Open Player ID Menu",
        emoji: "🪪",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:playerid",
        confirm: None,
    },
    Action {
        id: "add",
        label: "Add Player",
        emoji: "➕",
        style: ButtonStyle::Success,
        min_level: Level::R4,
        delegate: "settings:playerid",
        confirm: None,
    },
    Action {
        id: "lookup",
        label: "Lookup Player",
        emoji: "🔎",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:lookup",
        confirm: None,
    },
];
const PLAYER_ROW2: &[Action] = &[
    Action {
        id: "remove",
        label: "Remove Player",
        emoji: "🗑️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:playerid",
        confirm: None,
    },
    Action {
        id: "export",
        label: "Export CSV",
        emoji: "📄",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:playerid",
        confirm: None,
    },
    REFRESH,
];
const GIFT_ROW1: &[Action] = &[
    Action {
        id: "menu",
        label: "Open Gift Code Menu",
        emoji: "🎁",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:giftcode",
        confirm: None,
    },
    Action {
        id: "add",
        label: "Add Code",
        emoji: "➕",
        style: ButtonStyle::Success,
        min_level: Level::R4,
        delegate: "settings:giftcode",
        confirm: None,
    },
    Action {
        id: "active",
        label: "Active Codes",
        emoji: "📋",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:giftcode",
        confirm: None,
    },
    Action {
        id: "claims",
        label: "Claim History",
        emoji: "📜",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:giftcode",
        confirm: None,
    },
];
const GIFT_ROW2: &[Action] = &[
    Action {
        id: "sync",
        label: "Sync Codes",
        emoji: "🔄",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "gift:sync",
        confirm: None,
    },
    Action {
        id: "announce",
        label: "Announce",
        emoji: "📢",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:giftcode",
        confirm: None,
    },
    REFRESH,
];
const ROLES_ROW1: &[Action] = &[
    Action {
        id: "menu",
        label: "Open Roles Menu",
        emoji: "🎭",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:roles",
        confirm: None,
    },
    Action {
        id: "catalog",
        label: "Role Catalog",
        emoji: "🏷️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:roles",
        confirm: None,
    },
    Action {
        id: "assigners",
        label: "Role Assigners",
        emoji: "📑",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:roles",
        confirm: None,
    },
];
const ROLES_ROW2: &[Action] = &[
    Action {
        id: "autoroles",
        label: "Auto Roles",
        emoji: "🎭",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:roles",
        confirm: None,
    },
    Action {
        id: "channel",
        label: "Panel Channel",
        emoji: "📌",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:roles",
        confirm: None,
    },
    REFRESH,
];
const LEVELING_ROW: &[Action] = &[
    Action {
        id: "menu",
        label: "Open Leveling Menu",
        emoji: "⭐",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:leveling",
        confirm: None,
    },
    Action {
        id: "rewards",
        label: "Role Rewards",
        emoji: "🎁",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:leveling:hint_rewards",
        confirm: None,
    },
    Action {
        id: "announce",
        label: "Announcements",
        emoji: "📣",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:leveling:hint_announce",
        confirm: None,
    },
    Action {
        id: "reset",
        label: "Reset XP",
        emoji: "🧹",
        style: ButtonStyle::Danger,
        min_level: Level::R5,
        delegate: "settings:leveling:reset_all_confirmed",
        confirm: Some("Reset **all** leveling XP for this server? This cannot be undone."),
    },
    REFRESH,
];
const TICKET_ROW1: &[Action] = &[
    Action {
        id: "panel",
        label: "Panel Channel",
        emoji: "📌",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "ticket:set:panel",
        confirm: None,
    },
    Action {
        id: "category",
        label: "Category",
        emoji: "📁",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "ticket:set:category",
        confirm: None,
    },
    Action {
        id: "log",
        label: "Log Channel",
        emoji: "🧾",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "ticket:set:log",
        confirm: None,
    },
    Action {
        id: "staff",
        label: "Staff Roles",
        emoji: "🛡️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "ticket:set:staff",
        confirm: None,
    },
];
const TICKET_ROW2: &[Action] = &[
    Action {
        id: "edit",
        label: "Panel Message",
        emoji: "✏️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "ticket:edit",
        confirm: None,
    },
    Action {
        id: "list",
        label: "Open Tickets",
        emoji: "📋",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "ticket:list",
        confirm: None,
    },
    Action {
        id: "update",
        label: "Update Panel",
        emoji: "📣",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "ticket:post",
        confirm: None,
    },
    Action {
        id: "hub",
        label: "All Settings",
        emoji: "⚙️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "ticket:hub",
        confirm: None,
    },
    REFRESH,
];
const VOTING_ROW: &[Action] = &[
    Action {
        id: "menu",
        label: "Open WoS Voting Menu",
        emoji: "🗳️",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:voting",
        confirm: None,
    },
    Action {
        id: "create",
        label: "Create Vote",
        emoji: "➕",
        style: ButtonStyle::Success,
        min_level: Level::R4,
        delegate: "settings:voting_create",
        confirm: None,
    },
    Action {
        id: "active",
        label: "Active Votes / Results",
        emoji: "📊",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "settings:voting",
        confirm: None,
    },
    REFRESH,
];
const FORUM_ROW: &[Action] = &[
    Action {
        id: "create",
        label: "Create Forum",
        emoji: "➕",
        style: ButtonStyle::Success,
        min_level: Level::R4,
        delegate: "atkmodal:forum:create",
        confirm: None,
    },
    Action {
        id: "list",
        label: "Forum List",
        emoji: "📋",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "atkforum:list",
        confirm: None,
    },
    REFRESH,
];
const NEWS_ROW: &[Action] = &[
    Action {
        id: "hub",
        label: "Open WoS News Menu",
        emoji: "📰",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:wos",
        confirm: None,
    },
    REFRESH,
];
const FORMS_ROW: &[Action] = &[
    Action {
        id: "menu",
        label: "Open Forms Menu",
        emoji: "📋",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:ministry",
        confirm: None,
    },
    REFRESH,
];
const NOTIFICATIONS_ROW: &[Action] = &[
    Action {
        id: "menu",
        label: "Open Notifications Menu",
        emoji: "🔔",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:notifications",
        confirm: None,
    },
    REFRESH,
];
const MESSAGES_ROW: &[Action] = &[
    Action {
        id: "menu",
        label: "Open Messages Menu",
        emoji: "✉️",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:messages",
        confirm: None,
    },
    REFRESH,
];
const CALC_ROW1: &[Action] = &[
    Action {
        id: "building",
        label: "Building",
        emoji: "🏗️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "buildingcalc:home",
        confirm: None,
    },
    Action {
        id: "chiefgear",
        label: "Chief Gear",
        emoji: "⚔️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "chiefgearcalc:home",
        confirm: None,
    },
    Action {
        id: "charm",
        label: "Charms",
        emoji: "💎",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "charmcalc:home",
        confirm: None,
    },
    Action {
        id: "pets",
        label: "Pets",
        emoji: "🐾",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "petcalc:home",
        confirm: None,
    },
    Action {
        id: "waracademy",
        label: "War Academy",
        emoji: "🎓",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "waracademycalc:home",
        confirm: None,
    },
];
const CALC_ROW2: &[Action] = &[
    Action {
        id: "svs",
        label: "SVS",
        emoji: "🗳️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "svscalc:home",
        confirm: None,
    },
    Action {
        id: "all",
        label: "All Calculators",
        emoji: "🧮",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:calculator",
        confirm: None,
    },
    REFRESH,
];
const GIVEAWAYS_ROW: &[Action] = &[
    Action {
        id: "menu",
        label: "Open Giveaways Menu",
        emoji: "🎉",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:giveaways",
        confirm: None,
    },
    REFRESH,
];
const LOOKUP_ROW: &[Action] = &[
    Action {
        id: "settings",
        label: "Open Lookup Settings",
        emoji: "⚙️",
        style: ButtonStyle::Primary,
        min_level: Level::R4,
        delegate: "settings:lookup",
        confirm: None,
    },
    Action {
        id: "player",
        label: "Search Player",
        emoji: "👤",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "lookup:player:search",
        confirm: None,
    },
    Action {
        id: "alliance",
        label: "Search Alliance",
        emoji: "🛡️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "lookup:alliance:search",
        confirm: None,
    },
    Action {
        id: "state",
        label: "Search State",
        emoji: "🗺️",
        style: ButtonStyle::Secondary,
        min_level: Level::R4,
        delegate: "lookup:state:search",
        confirm: None,
    },
    REFRESH,
];

static PANELS: &[Panel] = &[
    Panel { key: "welcome", title: "WELCOME", post_name: "👋 Welcome", emoji: "👋", blurb: "Manage the server's welcome message system.", rows: &[WELCOME_ROW] },
    Panel { key: "index", title: "SERVER INDEX", post_name: "📑 Index", emoji: "📑", blurb: "The server index updates itself when channels, forums and forum posts are created, changed or deleted.", rows: &[INDEX_ROW] },
    Panel { key: "player_ids", title: "PLAYER IDs", post_name: "🪪 Player IDs", emoji: "🪪", blurb: "Manage WOS player IDs. The player ID is the stable identifier — names can change.", rows: &[PLAYER_ROW1, PLAYER_ROW2] },
    Panel { key: "gift_codes", title: "GIFT CODE MANAGEMENT", post_name: "🎁 Gift Codes", emoji: "🎁", blurb: "Manage WOS gift codes. This controls the existing Gift Code system — there is no second database.", rows: &[GIFT_ROW1, GIFT_ROW2] },
    Panel { key: "roles", title: "ROLE MANAGEMENT", post_name: "🎭 Roles", emoji: "🎭", blurb: "Manage the role catalog and assigners, and launch the public role-selection panel from here.", rows: &[ROLES_ROW1, ROLES_ROW2] },
    Panel { key: "leveling", title: "LEVELING", post_name: "⭐ Leveling", emoji: "⭐", blurb: "Configure the existing leveling engine: XP, role rewards, announcements and ignored channels.", rows: &[LEVELING_ROW] },
    Panel { key: "tickets", title: "TICKETS", post_name: "🎫 Tickets", emoji: "🎫", blurb: "Manage the existing ticket system — panel, category, logs and staff roles.", rows: &[TICKET_ROW1, TICKET_ROW2] },
    Panel { key: "voting", title: "WOS VOTING", post_name: "🗳️ WOS Voting", emoji: "🗳️", blurb: "Create and manage WOS votes. Votes keep persistent IDs; results stay accessible after closing.", rows: &[VOTING_ROW] },
    Panel { key: "forum", title: "FORUM MANAGEMENT", post_name: "🏠 Forum", emoji: "🏠", blurb: "Manage the server's forum channels. (The Admin Toolkit itself stays a separate, protected forum.)", rows: &[FORUM_ROW] },
    Panel { key: "news", title: "WOS NEWS", post_name: "📰 WOS News", emoji: "📰", blurb: "Controls for the existing WOS news / relay hub.", rows: &[NEWS_ROW] },
    Panel { key: "forms", title: "FORMS", post_name: "📋 Forms", emoji: "📋", blurb: "The Forms admin panel — SVS form / sheet, exports, history and settings.", rows: &[FORMS_ROW] },
    Panel { key: "notifications", title: "NOTIFICATIONS", post_name: "🔔 Notification", emoji: "🔔", blurb: "Create and manage notifications (WOS events, gift codes, news, server events, giveaways, voting, maintenance, custom).", rows: &[NOTIFICATIONS_ROW] },
    Panel { key: "messages", title: "MESSAGE TOOLS", post_name: "✉️ Message", emoji: "✉️", blurb: "Create, edit and send messages with embeds, buttons and components.", rows: &[MESSAGES_ROW] },
    Panel { key: "calculators", title: "WOS CALCULATORS", post_name: "🧮 Calculator", emoji: "🧮", blurb: "Pick a calculator to open or set up its public panel. (War Academy covers T11 and T12.)", rows: &[CALC_ROW1, CALC_ROW2] },
    Panel { key: "giveaways", title: "GIVEAWAYS", post_name: "🎉 Giveaway", emoji: "🎉", blurb: "Manage the existing giveaway engine: entries, requirements, roles, winners, rerolls, expiry and cleanup.", rows: &[GIVEAWAYS_ROW] },
    Panel { key: "lookup", title: "LOOKUP", post_name: "🔎 Lookup", emoji: "🔎", blurb: "Private player / alliance / state search. Results only visible to the searcher.", rows: &[LOOKUP_ROW] },
];

pub fn get_panel(key: &str) -> Option<&'static Panel> {
    PANELS.iter().find(|panel| panel.key == key)
}

pub fn action(panel: &'static Panel, id: &str) -> Option<&'static Action> {
    panel
        .rows
        .iter()
        .flat_map(|row| row.iter())
        .find(|action| action.id == id)
}

pub const LINE: &str = "━━━━━━━━━━━━━━━━━━━━";
pub const COLOR: u32 = 0x2B6CB0;
pub const FOOTER: &str = "🔐 R4/R5 Admin Toolkit";

/// Build the shared forum-post payload for a panel.
pub fn build_payload(panel: &Panel, status: &[String]) -> (CreateEmbed, Vec<CreateActionRow>) {
    let status_text = if status.is_empty() {
        "No status available.".to_string()
    } else {
        status.join("\n")
    };
    let embed = CreateEmbed::new()
        .color(COLOR)
        .title(format!("{} {}", panel.emoji, panel.title))
        .description(format!(
            "{LINE}\n{}\n\n**Status**\n{status_text}\n{LINE}",
            panel.blurb
        ))
        .footer(serenity::CreateEmbedFooter::new(FOOTER));

    let rows = panel
        .rows
        .iter()
        .map(|row| {
            CreateActionRow::Buttons(
                row.iter()
                    .take(5)
                    .map(|action| {
                        CreateButton::new(format!("atk:{}:{}", panel.key, action.id))
                            .label(action.label)
                            .style(action.style)
                            .emoji(ReactionType::Unicode(action.emoji.to_string()))
                    })
                    .collect(),
            )
        })
        .take(5)
        .collect();
    (embed, rows)
}

/// Current status lines for a panel, read from the existing feature storage.
pub async fn status_lines(db: &crate::db::Db, guild_id: u64, key: &str) -> Vec<String> {
    match key {
        "tickets" => {
            let settings = db::tickets::settings(db, guild_id)
                .await
                .unwrap_or_else(|_| db::tickets::empty_settings(guild_id));
            let open = db::tickets::open_for_guild(db, guild_id)
                .await
                .map(|rows| rows.len().to_string())
                .unwrap_or_else(|_| "?".to_string());
            let channel = |id: Option<u64>| {
                id.map(|id| format!("<#{id}>"))
                    .unwrap_or_else(|| "⚠️ not set".to_string())
            };
            let staff = if settings.staff_roles().is_empty() {
                "⚠️ not set".to_string()
            } else {
                settings
                    .staff_roles()
                    .iter()
                    .map(|id| format!("<@&{id}>"))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            vec![
                format!("Panel: {}", channel(settings.panel_channel())),
                format!(
                    "Category: {}",
                    settings
                        .category()
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "⚠️ not set".to_string())
                ),
                format!("Log: {}", channel(settings.log_channel())),
                format!("Staff: {staff}"),
                format!("Open tickets: **{open}**"),
            ]
        }
        "player_ids" => vec![format!(
            "Registered players: **{}**",
            db::players::stats(db, guild_id)
                .await
                .map(|s| s.tracked)
                .unwrap_or(0)
        )],
        "gift_codes" => vec![format!(
            "Active codes: **{}**",
            db::giftcodes::stats(db)
                .await
                .map(|s| s.active)
                .unwrap_or(0)
        )],
        "index" => {
            let settings = db::server_index::get(db, guild_id).await.ok();
            vec![
                format!(
                    "Index channel: {}",
                    settings
                        .and_then(|s| s.channel())
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "_not set_".to_string())
                ),
                "The server index publishes automatically when configured.".to_string(),
            ]
        }
        "voting" => {
            let channel = crate::db::voting::settings(db, guild_id)
                .await
                .ok()
                .and_then(|settings| settings.channel_id);
            vec![
                format!(
                    "Active votes: **{}**",
                    crate::db::ballots::count_active(db, guild_id).await.unwrap_or(0)
                ),
                format!(
                    "Voting channel: {}",
                    channel
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "⚠️ not set".to_string())
                ),
            ]
        }
        "forms" => vec!["Use the panel in this post.".to_string()],
        "calculators" => vec![
            "Select a calculator below — each opens its existing setup screen.".to_string()
        ],
        "notifications" => vec![format!(
            "Configured notifications: **{}**",
            db::notifications::count(db, guild_id).await.unwrap_or(0)
        )],
        "news" => {
            let destination = db::wos_relay::destination(db, guild_id).await.ok().flatten();
            let hub = db::wos_relay::hub(db).await.unwrap_or_default();
            vec![
                format!(
                    "Destination channel: {}",
                    destination
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "⚠️ not set".to_string())
                ),
                format!(
                    "Hub server: {}",
                    if hub.guild().is_none() {
                        "⚠️ not set".to_string()
                    } else if hub.guild() == Some(guild_id) {
                        "this server".to_string()
                    } else {
                        "another server".to_string()
                    }
                ),
            ]
        }
        "messages" => vec![
            "Message library, groups and live edit are available in the hub.".to_string()
        ],
        "forum" => vec![
            "Use **Create Forum** or **Forum List** below. The Admin Toolkit's own forum stays separate and protected.".to_string(),
        ],
        "leveling" => {
            let Ok(settings) = db::leveling::settings(db, guild_id).await else {
                return vec!["⚠️ Status unavailable.".to_string()];
            };
            let voice = if settings.voice_xp > 0 {
                format!("🟢 {}", settings.voice_xp)
            } else {
                "🔴 OFF".to_string()
            };
            let announcements = match settings.announce_channel() {
                Some(id) => format!("{} (<#{id}>)", settings.announce_mode),
                None => settings.announce_mode.clone(),
            };
            vec![
                format!(
                    "Status: {}",
                    if settings.enabled() {
                        "🟢 ON"
                    } else {
                        "🔴 OFF"
                    }
                ),
                format!(
                    "XP: {}–{} · Cooldown: {}s",
                    settings.xp_min, settings.xp_max, settings.cooldown_sec
                ),
                format!("Voice XP: {voice}"),
                format!("Announcements: {announcements}"),
                format!(
                    "Ignored channels: {}",
                    settings.ignored_channels().len()
                ),
            ]
        }
        _ => vec!["Use the panel in this post to open the existing feature settings.".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_javascript_keys_and_limits() {
        assert_eq!(PANELS.len(), 16);
        for (panel, key) in PANELS.iter().zip(KEYS.iter()) {
            assert_eq!(panel.key, *key);
            assert!(panel.rows.len() <= 5);
            for row in panel.rows {
                assert!(row.len() <= 5);
                for action in row.iter() {
                    assert!(format!("atk:{}:{}", panel.key, action.id).len() <= 100);
                    if action.id != "refresh" {
                        assert!(!action.delegate.is_empty());
                    }
                }
            }
        }
        assert!(get_panel("welcome").is_some());
        assert!(get_panel("missing").is_none());
        assert!(action(get_panel("leveling").unwrap(), "reset")
            .is_some_and(|a| a.min_level == Level::R5));
    }
}
