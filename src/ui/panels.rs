//! The actual panels. Each returns an embed plus its component rows so the
//! caller can both send and edit messages from the same source of truth.
//!
//! Panels take plain data (no `Data`, no Discord client) which keeps them
//! cheap to render from commands, buttons, background refreshes and tests.

use poise::serenity_prelude as serenity;
use serenity::{ButtonStyle, CreateActionRow, CreateButton, CreateEmbed};

use super::components::{self, secondary};
use super::embeds;
use super::theme::{self, colors, icons};

/// Everything the music panel needs to draw itself.
#[derive(Debug, Clone, Default)]
pub struct NowPlayingView {
    pub title: String,
    pub author: String,
    pub uri: Option<String>,
    pub length_ms: u64,
    pub position_ms: u64,
    pub paused: bool,
    pub volume: u16,
    pub loop_label: String,
    pub requester: String,
    pub queue_len: usize,
    pub upcoming: Vec<(String, String)>,
    pub connected_channel: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LookupPlayerView {
    pub fid: String,
    pub game_name: Option<String>,
    pub alliance: Option<String>,
    pub power: Option<i64>,
    pub kills: Option<i64>,
    pub town_hall: Option<i64>,
    pub life_tree: Option<i64>,
    pub state: Option<i64>,
    pub avatar_url: Option<String>,
    pub synced: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct LookupAllianceView {
    pub abbr: String,
    pub name: Option<String>,
    pub members: usize,
    pub power: i64,
    pub kills: i64,
    pub top: Vec<(String, i64)>,
    pub synced: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct StateView {
    pub state: u64,
    pub alliances: usize,
    pub players: usize,
    pub power: i64,
    pub top_alliances: Vec<(String, i64, usize)>,
}

// ---------------------------------------------------------------- settings hub

pub struct SettingsContext {
    pub guild_name: String,
    pub player_ids: i64,
    pub player_limit: Option<i64>,
    pub music_configured: bool,
    pub lookup_configured: bool,
    pub gift_sync: Option<i64>,
    pub oracle_ready: bool,
    pub uptime: String,
    pub latency_ms: u64,
    pub is_owner: bool,
    /// True only when the current guild is the official OmniCore bot server —
    /// gates the Owner Panel button on the main settings menu.
    pub official_guild: bool,
}
// Extend with new config panels added this migration.
#[derive(Debug, Clone, Default)]
pub struct LevelingView {
    pub enabled: bool,
    pub xp_min: i64,
    pub xp_max: i64,
    pub cooldown_sec: i64,
    pub voice_xp: i64,
    pub announce_mode: String,
    pub stack_rewards: bool,
    pub reward_count: usize,
    pub level: i64,
    pub total_ranked: i64,
    pub cleanup_enabled: bool,
    pub cleanup_delay_minutes: i64,
    pub bot_can_manage_roles: bool,
    pub announce_channel: Option<u64>,
    pub ignored_channel_ids: Vec<u64>,
    /// Whether a public rank/leaderboard panel has been deployed.
    pub panel_deployed: bool,
    /// Optional one-line confirmation shown above the panel body (the legacy
    /// bot passed this as the interaction `content`).
    pub notice: Option<String>,
}

pub fn settings_hub(ctx: &SettingsContext) -> (CreateEmbed, Vec<CreateActionRow>) {
    let limit = ctx
        .player_limit
        .map(|v| v.to_string())
        .unwrap_or_else(|| "∞".to_string());
    let player_line = format!(
        "Player IDs: **{}**",
        format!("{}/{}", ctx.player_ids, limit)
    );

    let description = [
        "Welcome to the bot settings. Select a category to get started:",
        "",
        &player_line,
        theme::DIVIDER,
        "",
        "**Menu Categories**",
        theme::DIVIDER,
        "🪪 **Player ID** · 🔎 **Lookup** · 🎁 **Gift Code** · 📰 **WOS News**",
        "🧮 **Calculators** · 📋 **Forms** · 🔔 **Notifications** · ✉️ **Messages**",
        "🛠️ **Discord Tools** — Roles, Welcome, Birthday, Index, Music, Tickets, Giveaways, WoS Voting, Temp Voice, Levelling",
        "🛡️ **Admin** — administrators & branding",
        "📜 **Logs** — unified log channel (includes former Admin-Log)",
    ]
    .join("\n");

    let embed = embeds::menu(format!("{} Settings Menu", icons::SETTINGS), description);

    let main_row = |buttons: Vec<CreateButton>| CreateActionRow::Buttons(buttons);
    let row1 = main_row(vec![
        settings_menu_button(
            "settings:playerid",
            "Player ID",
            icons::ID,
            ButtonStyle::Primary,
        ),
        settings_menu_button(
            "settings:lookup",
            "Lookup",
            icons::LOOKUP,
            ButtonStyle::Primary,
        ),
        settings_menu_button(
            "settings:giftcode",
            "Gift Code",
            icons::GIFT,
            ButtonStyle::Primary,
        ),
        settings_menu_button(
            "settings:wos",
            "WOS News",
            icons::WOS_NEWS,
            ButtonStyle::Primary,
        ),
        settings_menu_button(
            "settings:calculator",
            "Calculators",
            icons::CALCULATOR,
            ButtonStyle::Primary,
        ),
    ]);
    let row2 = main_row(vec![
        settings_menu_button(
            "settings:ministry",
            "Forms",
            icons::LIST,
            ButtonStyle::Primary,
        ),
        settings_menu_button(
            "settings:notifications",
            "Notifications",
            icons::BELL,
            ButtonStyle::Primary,
        ),
        settings_menu_button(
            "settings:messages",
            "Messages",
            icons::MESSAGES,
            ButtonStyle::Primary,
        ),
        settings_menu_button(
            "settings:discordtools",
            "Discord Tools",
            icons::TOOLS,
            ButtonStyle::Primary,
        ),
        settings_menu_button(
            "settings:permissions:0",
            "Admin",
            icons::ALLIANCE,
            ButtonStyle::Secondary,
        ),
    ]);
    let row3 = main_row(vec![settings_menu_button(
        "settings:logs",
        "Logs",
        icons::LOG,
        ButtonStyle::Secondary,
    )]);
    let row4 = main_row(vec![settings_menu_button(
        "settings:owner",
        "Owner Panel",
        icons::CROWN,
        ButtonStyle::Danger,
    )]);

    let rows = if ctx.official_guild {
        vec![row1, row2, row3, row4]
    } else {
        vec![row1, row2, row3]
    };

    (embed, rows)
}

/// Button for the main settings menu: custom id, label, emoji, style.
fn settings_menu_button(id: &str, label: &str, emoji: &str, style: ButtonStyle) -> CreateButton {
    serenity::CreateButton::new(id)
        .label(label)
        .style(style)
        .emoji(serenity::ReactionType::Unicode(emoji.to_string()))
}

/// The 🛠️ Discord Tools submenu shown from the main settings menu.
///
/// Mirrors `buildDiscordToolsEmbed` / `buildDiscordToolsRows` in the JavaScript
/// settings menu: three button rows (two five-button feature rows + Back).
pub fn discord_tools() -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::menu(
        "🛠️ Discord Tools",
        [
            "Server Discord features:",
            "",
            "🎭 **Roles** · 👋 **Welcome** · 🎂 **Birthday** · 📑 **Index** · 🎵 **Music**",
            "🎫 **Tickets** · 🎉 **Giveaways** · 🗳️ **WoS Voting** · 🔊 **Temp Voice** · ⭐ **Levelling**",
        ]
        .join("\n"),
    );

    let row1 = CreateActionRow::Buttons(vec![
        serenity::CreateButton::new("settings:roles")
            .label("Roles")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("🎭".to_string())),
        serenity::CreateButton::new("settings:welcome")
            .label("Welcome")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("👋".to_string())),
        serenity::CreateButton::new("settings:birthday")
            .label("Birthday")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("🎂".to_string())),
        serenity::CreateButton::new("settings:index")
            .label("Index")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("📑".to_string())),
        serenity::CreateButton::new("settings:music")
            .label("Music")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("🎵".to_string())),
    ]);
    let row2 = CreateActionRow::Buttons(vec![
        serenity::CreateButton::new("settings:tickets")
            .label("Tickets")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("🎫".to_string())),
        serenity::CreateButton::new("settings:giveaways")
            .label("Giveaways")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("🎉".to_string())),
        serenity::CreateButton::new("settings:polls")
            .label("WoS Voting")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("🗳️".to_string())),
        serenity::CreateButton::new("settings:tempvoice")
            .label("Temp Voice")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("🔊".to_string())),
        serenity::CreateButton::new("settings:levels")
            .label("Levelling")
            .style(ButtonStyle::Primary)
            .emoji(serenity::ReactionType::Unicode("⭐".to_string())),
    ]);
    let row3 = CreateActionRow::Buttons(vec![serenity::CreateButton::new("settings:back")
        .label("Back to Settings")
        .style(ButtonStyle::Secondary)
        .emoji(serenity::ReactionType::Unicode(
            theme::icons::BACK.to_string(),
        ))]);

    (embed, vec![row1, row2, row3])
}

// ------------------------------------------------------------------- music

/// Welcome/autorole/join-DM configuration panel.
#[derive(Debug, Clone, Default)]
pub struct WelcomeView {
    pub channel: Option<u64>,
    pub avatar_mode: String,
    pub autorole_enabled: bool,
    pub member_roles: usize,
    pub join_dm: bool,
    pub reminder_interval: u64,
    pub member_count: Option<u64>,
    pub cleanup_enabled: bool,
    pub cleanup_delay_minutes: i64,
}

pub fn welcome_panel(view: &WelcomeView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Welcome", icons::BELL))
        .color(colors::BRAND)
        .description("Greet new members, grant auto-roles and send a registration DM.")
        .field(
            "Welcome message",
            embeds::rows(&[
                (
                    "Channel".into(),
                    view.channel
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "`off`".into()),
                ),
                ("Avatar".into(), format!("`{}`", view.avatar_mode)),
                (
                    "Server members".into(),
                    view.member_count
                        .map(|count| format!("`{count}`"))
                        .unwrap_or_else(|| "`refresh to load`".into()),
                ),
            ]),
            true,
        )
        .field(
            "Auto-role",
            embeds::rows(&[
                (
                    "Enabled".into(),
                    if view.autorole_enabled {
                        "`yes`".into()
                    } else {
                        "`no`".into()
                    },
                ),
                ("Member roles".into(), format!("`{}`", view.member_roles)),
                (
                    "Auto-cleanup".into(),
                    if view.cleanup_enabled {
                        format!("`on · {} min`", view.cleanup_delay_minutes)
                    } else {
                        "`off`".into()
                    },
                ),
            ]),
            true,
        )
        .field(
            "Join DM",
            embeds::rows(&[
                (
                    "Enabled".into(),
                    if view.join_dm {
                        "`yes`".into()
                    } else {
                        "`no`".into()
                    },
                ),
                (
                    "Reminder interval".into(),
                    format!("`{}d`", view.reminder_interval),
                ),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary(
            "settings:welcome:channel",
            format!("{} Channel", icons::EDIT),
        ),
        components::secondary("settings:welcome:title", format!("{} Title", icons::EDIT)),
        components::secondary(
            "settings:welcome:description",
            format!("{} Description", icons::EDIT),
        ),
        components::secondary(
            "settings:welcome:avatar",
            format!("{} Avatar", icons::PLAYER),
        ),
        components::secondary(
            "settings:welcome:autorole",
            format!(
                "{} Auto-role {}",
                icons::BOLT,
                if view.autorole_enabled { "off" } else { "on" }
            ),
        ),
        components::secondary(
            "settings:welcome:joindm",
            format!("{} Join DM", icons::BELL),
        ),
        components::secondary("settings:welcome:cleanup", "🧹 Clean Up"),
        components::secondary(
            "settings:welcome:preview",
            format!("{} Preview", icons::LINK),
        ),
    ];
    (embed, components::rows(buttons))
}

/// Admin embed editor screen (the JS `welcome/editor.js` layout): field edit
/// buttons, avatar-mode cycle and Back. The live preview embed is appended by
/// the handler so it renders with the clicking member as the sample.
pub fn welcome_editor_panel(avatar_mode: &str) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand("✏️ Edit Welcome Embed")
        .color(colors::BRAND)
        .description(
            "Customize the embed posted when someone joins the server.\n\n\
             Placeholders: `{mention}` `{user}` `{tag}` `{server}` `{membercount}`\n\n\
             *The preview below uses your own account as a stand-in.*",
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    let rows = vec![
        components::row(vec![
            components::secondary("settings:welcome:title", "Edit Title"),
            components::secondary("settings:welcome:description", "Edit Description"),
            components::secondary("settings:welcome:color", "Edit Color"),
        ]),
        components::row(vec![components::primary(
            "settings:welcome:avatar",
            format!(
                "{} Avatar: {}",
                icons::PLAYER,
                match avatar_mode {
                    "member" => "Member",
                    "server" => "Server",
                    _ => "Off",
                }
            ),
        )]),
        components::row(vec![components::secondary(
            "settings:welcome",
            format!("{} Back", icons::BACK),
        )]),
    ];
    (embed, rows)
}

pub fn welcome_cleanup_panel(
    settings: &crate::db::welcome::CleanupSettings,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let state = if settings.enabled() {
        format!(
            "On — welcome messages are removed after **{} minutes**.",
            settings.delay_minutes()
        )
    } else {
        "Off — welcome messages are kept.".to_string()
    };
    let embed = embeds::brand("🧹 Welcome Clean Up")
        .color(colors::BRAND)
        .description(format!("{state}\n\nChoose a delay from one minute up to 30 days. The worker deletes at most 20 due messages per pass."));
    let options = [
        ("1 hour", 60),
        ("6 hours", 360),
        ("1 day", 1440),
        ("7 days", 10080),
        ("30 days", 43200),
    ]
    .into_iter()
    .map(|(label, minutes)| {
        let mut option = serenity::CreateSelectMenuOption::new(label, minutes.to_string());
        if settings.delay_minutes() == minutes {
            option = option.default_selection(true);
        }
        option
    })
    .collect::<Vec<_>>();
    let select = serenity::CreateSelectMenu::new(
        "settings:welcome:cleanup_delay",
        serenity::CreateSelectMenuKind::String { options },
    )
    .placeholder("Choose cleanup delay")
    .min_values(1)
    .max_values(1);
    let rows = vec![
        CreateActionRow::Buttons(vec![components::secondary(
            "settings:welcome:cleanup_toggle",
            if settings.enabled() {
                "🛑 Turn Off"
            } else {
                "✅ Turn On"
            },
        )]),
        CreateActionRow::SelectMenu(select),
        CreateActionRow::Buttons(vec![
            components::secondary("settings:welcome:cleanup_custom", "✏️ Custom Delay"),
            components::secondary("settings:welcome:cleanup_back", "◀️ Back"),
        ]),
    ];
    (embed, rows)
}

/// Logging channel + category toggles panel.
#[derive(Debug, Clone)]
pub struct LoggingView {
    pub channel: Option<u64>,
    pub disabled: Vec<String>,
}

pub fn logging_panel(view: &LoggingView) -> (CreateEmbed, Vec<CreateActionRow>) {
    const CATEGORIES: &[&str] = &[
        "giftcode", "players", "members", "messages", "voice", "channels", "roles", "server",
    ];
    let body = CATEGORIES
        .iter()
        .map(|category| {
            let state = if view.disabled.iter().any(|c| c == category) {
                "off"
            } else {
                "on"
            };
            format!("`{:<9}` — **{}**", category, state)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let embed = embeds::brand(format!("{} Logging", icons::DATABASE))
        .color(colors::BRAND_DEEP)
        .description("One channel for gift-code redemptions, player-IDs and server events.")
        .field(
            "Log channel",
            view.channel
                .map(|id| format!("<#{id}>"))
                .unwrap_or_else(|| "`not set`".into()),
            false,
        )
        .field("Categories", format!("```{}```", body), false)
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary(
            "settings:logging:channel",
            format!("{} Channel", icons::EDIT),
        ),
        components::secondary(
            "settings:logging:categories",
            format!("{} Categories", icons::LIST),
        ),
        components::secondary("settings:logging:test", format!("{} Test", icons::BOLT)),
    ];
    (embed, components::rows(buttons))
}

/// Temp voice lobby panel.
#[derive(Debug, Clone, Default)]
pub struct TempVoiceView {
    pub enabled: bool,
    pub lobby: Option<u64>,
    pub category: Option<u64>,
    pub template: String,
    pub user_limit: u32,
    pub active: i64,
}

pub fn tempvoice_panel(view: &TempVoiceView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Temp Voice", icons::BOLT))
        .color(colors::TEAL)
        .description("Members join the lobby and get their own voice channel.")
        .field(
            "Configuration",
            embeds::rows(&[
                (
                    "Enabled".into(),
                    if view.enabled {
                        "`yes`".into()
                    } else {
                        "`no`".into()
                    },
                ),
                (
                    "Lobby".into(),
                    view.lobby
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "`not set`".into()),
                ),
                (
                    "Category".into(),
                    view.category
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "`lobby default`".into()),
                ),
                ("Template".into(), format!("`{}`", view.template)),
                (
                    "Limit".into(),
                    format!(
                        "`{}`",
                        if view.user_limit == 0 {
                            "unlimited".to_string()
                        } else {
                            view.user_limit.to_string()
                        }
                    ),
                ),
                ("Active".into(), format!("`{}`", view.active)),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary(
            "settings:tempvoice:toggle",
            format!(
                "{} {}",
                if view.enabled { "Disable" } else { "Enable" },
                icons::BOLT
            ),
        ),
        components::secondary("settings:tempvoice:lobby", format!("{} Lobby", icons::EDIT)),
        components::secondary(
            "settings:tempvoice:category",
            format!("{} Category", icons::EDIT),
        ),
        components::secondary(
            "settings:tempvoice:template",
            format!("{} Template", icons::EDIT),
        ),
        components::secondary("settings:tempvoice:limit", format!("{} Limit", icons::EDIT)),
    ];
    (embed, components::rows(buttons))
}

pub fn voting_panel(view: &VotingPanel) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} WoS Voting", icons::SWORD))
        .color(colors::VIOLET)
        .description("Multi-question votes for ministry elections and similar.")
        .field(
            "Channels",
            embeds::rows(&[
                (
                    "Vote channel".into(),
                    view.voting_channel
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "`not set`".into()),
                ),
                (
                    "Results channel".into(),
                    view.results_channel
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "`same channel`".into()),
                ),
                ("Active votes".into(), format!("`{}`", view.active)),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary(
            "settings:voting_create",
            format!("{} Create vote", icons::ADD),
        ),
        components::secondary("ballot:active_list:0", "📊 Active votes / results"),
        components::primary(
            "settings:voting:channel",
            format!("{} Vote channel", icons::EDIT),
        ),
        components::secondary(
            "settings:voting:results",
            format!("{} Results channel", icons::EDIT),
        ),
    ];
    (embed, components::rows(buttons))
}

/// Ballot Clean Up screen (JS `ballots/cleanup.js` config).
pub fn ballot_cleanup_panel(
    settings: &crate::db::welcome::CleanupSettings,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let status = if settings.enabled() {
        format!(
            "🟢 On — closed ballots and announcements are removed after **{} minutes**.",
            settings.delay_minutes()
        )
    } else {
        "🔴 Off — finished ballot messages are kept.".to_string()
    };
    let embed = embeds::brand("🧹 Ballot Clean Up")
        .color(colors::GOLD)
        .description(format!(
            "Automatically delete closed ballots, their result announcements and reminder pings after a set delay (counted from when the ballot closes), so old votes don't pile up.\n\n{status}"
        ));
    let presets = [
        ("30m", 30),
        ("1h", 60),
        ("6h", 360),
        ("12h", 720),
        ("1d", 1440),
        ("3d", 4320),
        ("7d", 10080),
    ];
    let options = presets
        .into_iter()
        .map(|(label, minutes)| {
            let mut option = serenity::CreateSelectMenuOption::new(label, minutes.to_string());
            if settings.delay_minutes() == minutes {
                option = option.default_selection(true);
            }
            option
        })
        .collect::<Vec<_>>();
    let select = serenity::CreateSelectMenu::new(
        "ballot:cleanup_preset",
        serenity::CreateSelectMenuKind::String { options },
    )
    .placeholder(format!(
        "Delay — currently {} minutes",
        settings.delay_minutes()
    ))
    .min_values(1)
    .max_values(1);
    let rows = vec![
        CreateActionRow::Buttons(vec![components::secondary(
            "ballot:cleanup_toggle",
            if settings.enabled() {
                "🛑 Turn Off"
            } else {
                "✅ Turn On"
            },
        )]),
        CreateActionRow::SelectMenu(select),
        CreateActionRow::Buttons(vec![
            components::secondary("ballot:cleanup_custom", "✏️ Custom Delay"),
            components::secondary("ballot:cleanup_back", "◀️ Back"),
        ]),
    ];
    (embed, rows)
}

/// Voting Toolkit post: JS renders the unified `polls/menu.showHub` live hub
/// with its main controls, without the settings Back row.
pub fn voting_toolkit_panel(view: &VotingPanel) -> (CreateEmbed, Vec<CreateActionRow>) {
    let voting_channel = view
        .voting_channel
        .map(|id| format!("<#{id}>"))
        .unwrap_or_else(|| {
            "_not set — votes post in the channel where you create them_".to_string()
        });
    let results_channel = view
        .results_channel
        .map(|id| format!("<#{id}>"))
        .unwrap_or_else(|| {
            "_not set — results post in the channel where the vote was posted_".to_string()
        });
    let embed = embeds::brand("🗳️ WoS Voting")
        .color(colors::BRAND)
        .description(format!(
            "Run WoS-style votes (several questions at once) with who-voted / not-voted tracking.\n\n\
             1️⃣ **WoS Vote** — create a vote\n\
             2️⃣ **Channel** — the channel where votes are posted by default\n\
             3️⃣ **Post Results** — choose the channel where results are **posted automatically** when a vote closes (voter names + their votes + who didn't vote)\n\n\
             🗂️ **Manage** — results, extend the timer, close early, delete  •  🧹 **Clean Up** — auto-delete old votes\n\n\
             ℹ️ Active: **{}** vote{}\n\
             📍 Voting channel: {}\n\
             📢 Results channel: {}",
            view.active,
            if view.active == 1 { "" } else { "s" },
            voting_channel,
            results_channel,
        ))
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    let mut rows = vec![components::row(vec![
        components::success("settings:voting_create", "🗳️ WoS Vote"),
        components::primary("settings:voting:channel", "📍 Channel"),
        components::primary("settings:voting:results", "📢 Post Results"),
    ])];
    rows.push(components::row(vec![
        components::secondary("ballot:active_list:0", "🗂️ Manage"),
        components::secondary("ballot:cleanup", "🧹 Clean Up"),
    ]));
    (embed, rows)
}

#[derive(Debug, Clone, Default)]
pub struct VotingPanel {
    pub voting_channel: Option<u64>,
    pub results_channel: Option<u64>,
    pub active: i64,
}

/// Birthday announcements panel.
#[derive(Debug, Clone, Default)]
pub struct BirthdayView {
    pub channel: Option<u64>,
    pub count: i64,
}

pub fn birthday_panel(view: &BirthdayView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Birthdays", icons::GIFT))
        .color(colors::VIOLET)
        .description("Members share their birthday and get a personalised embed.")
        .field(
            "Configuration",
            embeds::rows(&[
                (
                    "Channel".into(),
                    view.channel
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "`off`".into()),
                ),
                ("Stored birthdays".into(), format!("`{}`", view.count)),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary(
            "settings:birthday:channel",
            format!("{} Panel / Channel", icons::EDIT),
        ),
        components::secondary("settings:birthday:editmsg", "✏️ Edit announcement"),
        components::secondary("settings:birthday:cleanup", "🧹 Clean up"),
        components::secondary("settings:birthday:browse:0", "📋 View/edit members"),
        components::secondary("settings:birthday:refresh", "Refresh"),
    ];
    (embed, components::rows(buttons))
}

/// Giveaways panel.
#[derive(Debug, Clone, Default)]
pub struct GiveawayView {
    pub active: i64,
    pub channel: Option<u64>,
}

pub fn giveaways_panel(view: &GiveawayView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Giveaways", icons::GIFT))
        .color(colors::GOLD)
        .description("Members enter with one tap; winners are drawn automatically.")
        .field(
            "Active",
            format!("**{}** giveaway(s) running", view.active),
            false,
        )
        .field(
            "Giveaway channel",
            view.channel
                .map(|id| format!("<#{id}>"))
                .unwrap_or_else(|| {
                    "Not set — choose a channel before creating from the Toolkit post.".to_string()
                }),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary(
            "settings:giveaways:create",
            format!("{} New giveaway", icons::ADD),
        ),
        components::secondary("settings:giveaways:list", format!("{} List", icons::LIST)),
        components::secondary(
            "giveawayrules:pick",
            format!("{} Advanced rules", icons::EDIT),
        ),
        components::secondary("settings:giveaways:channel", "📌 Giveaway channel"),
        components::secondary(
            "settings:giveaways:refresh",
            format!("{} Refresh", icons::REFRESH),
        ),
        components::secondary("settings:giveaways:cleanup", "🧹 Clean Up"),
        components::primary("settings:giveaways:manage", "🗂️ Manage"),
    ];
    (embed, components::rows(buttons))
}

pub fn giveaway_cleanup_panel(
    settings: &crate::db::welcome::CleanupSettings,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let status = if settings.enabled() {
        format!(
            "🟢 On — finished giveaway posts and announcements are removed after **{} minutes**.",
            settings.delay_minutes()
        )
    } else {
        "🔴 Off — finished giveaway messages are kept.".to_string()
    };
    let embed = embeds::brand("🧹 Giveaway Clean Up")
        .color(colors::GOLD)
        .description(format!("Automatically delete finished giveaway posts and winner/reroll announcements after a delay counted from when the giveaway ends or is cancelled.\n\n{status}"));
    let presets = [
        ("30m", 30),
        ("1h", 60),
        ("6h", 360),
        ("12h", 720),
        ("1d", 1440),
        ("3d", 4320),
        ("7d", 10080),
    ];
    let options = presets
        .into_iter()
        .map(|(label, minutes)| {
            serenity::CreateSelectMenuOption::new(label, minutes.to_string())
                .default_selection(settings.delay_minutes() == minutes)
        })
        .collect();
    let rows = vec![
        CreateActionRow::Buttons(vec![components::success(
            "settings:giveaways:cleanup_toggle",
            if settings.enabled() {
                "🛑 Turn Off"
            } else {
                "✅ Turn On"
            },
        )]),
        CreateActionRow::SelectMenu(
            serenity::CreateSelectMenu::new(
                "settings:giveaways:cleanup_preset",
                serenity::CreateSelectMenuKind::String { options },
            )
            .placeholder("Choose a cleanup delay")
            .min_values(1)
            .max_values(1),
        ),
        CreateActionRow::Buttons(vec![
            components::secondary("settings:giveaways:cleanup_custom", "✏️ Custom Delay"),
            components::secondary("settings:giveaways:refresh", "◀️ Back"),
        ]),
    ];
    (embed, rows)
}

/// Message library panel.
#[derive(Debug, Clone, Default)]
pub struct MessagesView {
    pub group_count: usize,
    pub message_count: i64,
}

pub fn messages_panel(view: &MessagesView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Messages", icons::MESSAGES))
        .color(colors::TEAL)
        .description("Reusable announcement templates — saved, edited and posted from Discord.")
        .field(
            "Library",
            embeds::rows(&[
                ("Groups".into(), format!("`{}`", view.group_count)),
                ("Messages".into(), format!("`{}`", view.message_count)),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary("settings:messages:grouping", "📁 Grouping"),
        components::primary("settings:messages:manage", "🗂️ Manage"),
        components::secondary("settings:messages:create", format!("{} Save", icons::SAVE)),
        components::secondary("settings:messages:create_embed", "🧩 Save Embed"),
        components::secondary("settings:messages:send", format!("{} Send", icons::FORWARD)),
        components::secondary("settings:messages:edit", "✏️ Edit Sent"),
    ];
    (embed, components::rows(buttons))
}

/// Messages Toolkit post: JS renders the real Messages hub (Create / Manage /
/// Grouping / Edit) with stripped Back row. Rust keeps its two additional
/// working shortcuts after those four destinations.
pub fn messages_toolkit_panel(view: &MessagesView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand("✉️ Messages")
        .color(colors::BRAND)
        .description(
            "Build, save, and send standalone messages — separate from the notification system.\n\n\
             📝 **Create** — build a plain or embed message, save it to the library, and/or send it now\n\
             💾 **Manage** — browse the library by group, then view, edit, or delete a saved message\n\
             📁 **Grouping** — create, view, and delete the groups messages are organized into\n\
             ⚙️ **Edit** — find a message the bot already posted to a channel and edit or delete it live",
        )
        .field(
            "Library",
            embeds::rows(&[
                ("Groups".into(), format!("`{}`", view.group_count)),
                ("Messages".into(), format!("`{}`", view.message_count)),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    let rows = vec![
        components::row(vec![
            components::success("settings:messages:create", "📝 Create"),
            components::primary("settings:messages:manage", "💾 Manage"),
        ]),
        components::row(vec![
            components::secondary("settings:messages:grouping", "📁 Grouping"),
            components::secondary("settings:messages:edit", "⚙️ Edit"),
        ]),
    ];
    (embed, rows)
}

/// Server Index (auto-updating channel directory) panel.
#[derive(Debug, Clone, Default)]
pub struct ServerIndexView {
    pub channel: Option<u64>,
    pub enabled: bool,
    pub include_threads: bool,
    pub include_voice: bool,
    pub message_count: usize,
    pub join_dm_enabled: bool,
    pub reminder_enabled: bool,
    pub reminder_interval_days: u64,
}

pub fn server_index_panel(view: &ServerIndexView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Server Index", icons::SERVER))
        .color(colors::BRAND)
        .description("Auto-maintained public directory of channels and forum posts.")
        .field(
            "Configuration",
            embeds::rows(&[
                (
                    "Channel".into(),
                    view.channel
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "`not set`".into()),
                ),
                (
                    "Auto update".into(),
                    if view.enabled {
                        "`on`".into()
                    } else {
                        "`off`".into()
                    },
                ),
                (
                    "Forum threads".into(),
                    if view.include_threads {
                        "`included`".into()
                    } else {
                        "`excluded`".into()
                    },
                ),
                (
                    "Voice channels".into(),
                    if view.include_voice {
                        "`included`".into()
                    } else {
                        "`excluded`".into()
                    },
                ),
                ("Index messages".into(), format!("`{}`", view.message_count)),
                (
                    "Join DM".into(),
                    if view.join_dm_enabled {
                        "`on`".into()
                    } else {
                        "`off`".into()
                    },
                ),
                (
                    "Registration reminder".into(),
                    if view.reminder_enabled {
                        format!("`on · every {} days`", view.reminder_interval_days)
                    } else {
                        "`off`".into()
                    },
                ),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary("settings:index:channel", format!("{} Channel", icons::EDIT)),
        components::secondary(
            "settings:index:toggle",
            format!(
                "{} Auto {}",
                icons::BOLT,
                if view.enabled { "OFF" } else { "ON" }
            ),
        ),
        components::secondary(
            "settings:index:threads",
            format!(
                "{} Threads {}",
                icons::LIST,
                if view.include_threads { "ON" } else { "OFF" }
            ),
        ),
        components::secondary(
            "settings:index:voice",
            format!(
                "{} Voice {}",
                icons::VOLUME,
                if view.include_voice { "ON" } else { "OFF" }
            ),
        ),
        components::secondary("settings:index:joindm", "📬 Join DM"),
        components::secondary("settings:index:reminder", "⏰ Registration Reminder"),
        components::primary(
            "settings:index:rebuild",
            format!("{} Rebuild now", icons::REFRESH),
        ),
    ];
    (embed, components::rows(buttons))
}

/// Calculator hub panel.
#[derive(Debug, Clone, Default)]
pub struct CalculatorView {
    pub svs_channel: Option<u64>,
    pub svs_message_id: Option<u64>,
    pub building_channel: Option<u64>,
    pub building_message_id: Option<u64>,
    pub gear_channel: Option<u64>,
    pub gear_message_id: Option<u64>,
    pub charm_channel: Option<u64>,
    pub charm_message_id: Option<u64>,
    pub pets_channel: Option<u64>,
    pub pets_message_id: Option<u64>,
    pub waracademy_channel: Option<u64>,
    pub waracademy_message_id: Option<u64>,
}

pub fn calculator_panel(view: &CalculatorView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let channel_line = |channel: Option<u64>| {
        channel
            .map(|id| format!("<#{id}>"))
            .unwrap_or_else(|| "`not set`".to_string())
    };
    let msg_line = |id: Option<u64>| {
        id.map(|id| format!("`{id}`"))
            .unwrap_or_else(|| "`not posted`".into())
    };

    let embed = embeds::brand(format!("{} Calculators", icons::BOLT))
        .color(colors::BRAND)
        .description("Persistent calculator panels members use to plan upgrades.")
        .field(
            "🏗️ Building",
            embeds::rows(&[
                ("Panel channel".into(), channel_line(view.building_channel)),
                ("Message id".into(), msg_line(view.building_message_id)),
            ]),
            true,
        )
        .field(
            "🗓️ SVS Prep Planner",
            embeds::rows(&[
                ("Panel channel".into(), channel_line(view.svs_channel)),
                ("Message id".into(), msg_line(view.svs_message_id)),
            ]),
            true,
        )
        .field(
            "🎖️ Chief Gear",
            embeds::rows(&[
                ("Panel channel".into(), channel_line(view.gear_channel)),
                ("Message id".into(), msg_line(view.gear_message_id)),
            ]),
            true,
        )
        .field(
            "🌀 Chief Charm",
            embeds::rows(&[
                ("Panel channel".into(), channel_line(view.charm_channel)),
                ("Message id".into(), msg_line(view.charm_message_id)),
            ]),
            true,
        )
        .field(
            "🐾 Pets",
            embeds::rows(&[
                ("Panel channel".into(), channel_line(view.pets_channel)),
                ("Message id".into(), msg_line(view.pets_message_id)),
            ]),
            true,
        )
        .field(
            "🏛️ War Academy",
            embeds::rows(&[
                (
                    "Panel channel".into(),
                    channel_line(view.waracademy_channel),
                ),
                ("Message id".into(), msg_line(view.waracademy_message_id)),
            ]),
            true,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary(
            "settings:calculator:building_channel",
            format!("{} Building channel", icons::EDIT),
        ),
        components::secondary(
            "settings:calculator:building_repost",
            format!("{} Repost Building", icons::REFRESH),
        ),
        components::primary(
            "settings:calculator:svs_channel",
            format!("{} SVS channel", icons::EDIT),
        ),
        components::secondary(
            "settings:calculator:svs_repost",
            format!("{} Repost SVS", icons::REFRESH),
        ),
        components::primary(
            "settings:calculator:gear_channel",
            format!("{} Gear channel", icons::EDIT),
        ),
        components::secondary(
            "settings:calculator:gear_repost",
            format!("{} Repost Gear", icons::REFRESH),
        ),
        components::primary(
            "settings:calculator:charm_channel",
            format!("{} Charm channel", icons::EDIT),
        ),
        components::secondary(
            "settings:calculator:charm_repost",
            format!("{} Repost Charm", icons::REFRESH),
        ),
        components::primary(
            "settings:calculator:pets_channel",
            format!("{} Pets channel", icons::EDIT),
        ),
        components::secondary(
            "settings:calculator:pets_repost",
            format!("{} Repost Pets", icons::REFRESH),
        ),
        components::primary(
            "settings:calculator:waracademy_channel",
            format!("{} War Academy channel", icons::EDIT),
        ),
        components::secondary(
            "settings:calculator:waracademy_repost",
            format!("{} Repost War Academy", icons::REFRESH),
        ),
        components::secondary("settings:home", format!("{} Back", icons::BACK)),
    ];
    let mut rows = components::rows(buttons);
    rows.extend(components::rows(vec![
        components::secondary("svscalc:home", "🗳️ Open SVS"),
        components::secondary("buildingcalc:home", "🏗️ Building"),
        components::secondary("chiefgearcalc:home", "⚔️ Chief Gear"),
        components::secondary("charmcalc:home", "💎 Charms"),
        components::secondary("petcalc:home", "🐾 Pets"),
        components::secondary("waracademycalc:home", "🎓 War Academy"),
    ]));
    (embed, rows)
}

/// Role Assignment panel (staff-only catalog + grants).
#[derive(Debug, Clone, Default)]
pub struct RolesPanelView {
    pub channel: Option<u64>,
    pub panel_message_id: Option<u64>,
    /// `(role_id, grant_count)` lines, already truncated for the embed.
    pub catalog: Vec<(u64, usize)>,
    /// `(role_id, user_grants, role_grants)` per catalog role.
    pub grants: Vec<(u64, usize, usize)>,
}

pub fn roles_panel(view: &RolesPanelView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let catalog_body = if view.catalog.is_empty() {
        "_No roles in the catalog yet — add one below._".to_string()
    } else {
        view.catalog
            .iter()
            .map(|(role_id, grants)| {
                format!(
                    "<@&{role_id}> — `{grants}` grant{}",
                    if *grants == 1 { "" } else { "s" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let grants_body = view
        .grants
        .iter()
        .filter(|(_, users, roles)| *users > 0 || *roles > 0)
        .map(|(role_id, users, roles)| format!("<@&{role_id}> — {users} user · {roles} role(s)"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut embed = embeds::brand(format!("{} Role Assignment", icons::TAG))
        .color(colors::BRAND)
        .description("Staff-only tool for granting/removing Discord roles on members — not self-service. Anyone with perms/access can use the panel message in the channel below.")
        .field(
            "Panel",
            embeds::rows(&[
                ("Channel".into(), view.channel.map(|id| format!("<#{id}>")).unwrap_or_else(|| "`not set`".into())),
                ("Message".into(), view.panel_message_id.map(|id| format!("`{id}`")).unwrap_or_else(|| "`not posted`".into())),
            ]),
            false,
        )
        .field(format!("Catalog ({})", view.catalog.len()), catalog_body, false);
    if !grants_body.is_empty() {
        embed = embed.field("Grants", grants_body, false);
    }
    embed = embed.footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let mut rows = components::rows(vec![
        components::primary(
            "settings:roles:channel",
            format!("{} Panel Channel", icons::EDIT),
        ),
        components::secondary("settings:roles:add", format!("{} Add role", icons::ADD)),
        components::danger(
            "settings:roles:remove",
            format!("{} Remove role", icons::REMOVE),
        ),
        components::secondary(
            "settings:roles:refresh",
            format!("{} Refresh", icons::REFRESH),
        ),
    ]);
    rows.extend(components::rows(vec![
        components::secondary(
            "settings:roles:grant_user",
            format!("{} Grant user", icons::PLAYER),
        ),
        components::secondary(
            "settings:roles:grant_role",
            format!("{} Grant role", icons::LOCK),
        ),
        components::secondary(
            "settings:roles:revoke_user",
            format!("{} Revoke user", icons::REMOVE),
        ),
        components::secondary(
            "settings:roles:revoke_role",
            format!("{} Revoke role", icons::REMOVE),
        ),
    ]));
    (embed, rows)
}

/// Notification (Bear Trap style scheduled reminders) panel.
#[derive(Debug, Clone, Default)]
pub struct NotificationsView {
    /// Pre-rendered lines (description, enabled state), one per notification.
    pub lines: Vec<String>,
    pub default_slots: i64,
    pub total: i64,
}

pub fn notifications_panel(view: &NotificationsView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let body = if view.lines.is_empty() {
        "_No notifications yet — tap **Add** to schedule one._".to_string()
    } else {
        view.lines.join("\n")
    };
    let slot_label = match view.default_slots {
        1 => "30/10/5/0",
        2 => "10/5/0",
        3 => "5/0",
        4 => "5",
        _ => "0 (at event)",
    };

    let embed = embeds::brand(format!("{} Notifications", icons::BELL))
        .color(colors::BRAND)
        .description("Scheduled reminders for game events (Bear Trap & co).")
        .field(format!("Scheduled ({})", view.total), body, false)
        .field("Default slots", format!("`{slot_label} min before`"), true)
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary("settings:notifications:add", format!("{} Add", icons::ADD)),
        components::primary("settings:notifications:wizard", "🧭 Setup wizard"),
        components::primary("settings:notifications:manage", "🗂️ Manage"),
        components::secondary("settings:notifications:templates", "🎨 Templates"),
        components::secondary(
            "settings:notifications:slots",
            format!("{} Slots", icons::CLOCK),
        ),
        components::secondary(
            "settings:notifications:auto_delete",
            "🗑️ Auto-delete settings",
        ),
        components::secondary(
            "settings:notifications:toggle",
            format!("{} Toggle", icons::BOLT),
        ),
        components::danger(
            "settings:notifications:delete",
            format!("{} Delete", icons::TRASH),
        ),
    ];
    (embed, components::rows(buttons))
}

pub fn notifications_toolkit_panel() -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand("🔔 Notifications")
        .color(colors::BRAND)
        .description(
            "Set up and manage event notifications for this server.\n\n\
             🤖 **Setup Wizard** — guided flow to create notifications for one or more events\n\
             ⏰ **Custom Notification** — set up a new notification with custom time, message, mentions, and repeat options; perfect for anything not covered by the Wizard\n\
             💾 **Manage Notifications** — view, edit, toggle, or delete existing notifications\n\
             🎨 **Templates** — customize the default embed used per event\n\
             ⚙️ **Settings** — configure whether posted notifications are auto-deleted, and after how long",
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    // The JS live post strips its "Back to Settings" row; only the five hub
    // destinations remain. All of them open private management screens.
    let rows = vec![
        components::row(vec![
            components::success("settings:notifications:wizard", "🤖 Setup Wizard"),
            components::success("settings:notifications:add", "⏰ Custom Notification"),
            components::primary("settings:notifications:manage", "💾 Manage Notifications"),
        ]),
        components::row(vec![
            components::secondary("settings:notifications:templates", "🎨 Templates"),
            components::secondary("settings:notifications:auto_delete", "⚙️ Settings"),
        ]),
    ];
    (embed, rows)
}

/// WOS Announcements relay panel.
#[derive(Debug, Clone, Default)]
pub struct WosRelayView {
    pub destination: Option<u64>,
    pub sneakpeek: Option<u64>,
    pub hub_channel: Option<u64>,
    pub hub_configured: bool,
    pub is_owner: bool,
}

pub fn wos_relay_panel(view: &WosRelayView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let hub_line = if view.hub_configured {
        match view.hub_channel {
            Some(id) => format!("<#{id}>"),
            None => "configured (in another server)".to_string(),
        }
    } else {
        "not set up yet".to_string()
    };

    let mut lines = vec![
        "Relays the official Whiteout Survival server's announcements into this server, with an on-demand Translate button on each one.".to_string(),
        String::new(),
        format!("**This server's destination** — where relayed posts appear here (currently {})", view.destination.map(|id| format!("<#{id}>")).unwrap_or_else(|| "not set".to_string())),
        format!(
            "**Sneak Peek channel** — auto-posts a new Chief Concierge: Sneak Peek article as soon as it's spotted on the wiki (currently {})",
            view.sneakpeek.map(|id| format!("<#{id}>")).unwrap_or_else(|| "not set".to_string())
        ),
    ];
    if view.is_owner {
        lines.push(format!(
            "**Hub channel** (owner-only, shared across every server) — {hub_line}"
        ));
        lines.push(String::new());
        lines.push("**One-time hub setup (once, ever):**".to_string());
        lines.push("1. Pick a server only you manage, and add this bot to it.".to_string());
        lines.push("2. Open this menu there and hit **Set as Hub**.".to_string());
        lines.push("3. Join the official WOS Discord server as yourself.".to_string());
        lines.push(
            "4. Open a WOS 📣 channel → **Follow Channel** → pick the hub channel.".to_string(),
        );
        lines.push(
            "5. Repeat for any other WOS 📣 Announcement channel you want relayed.".to_string(),
        );
    } else if !view.hub_configured {
        lines.push(String::new());
        lines.push(
            "The bot owner hasn't set up the shared hub channel yet — ask them to.".to_string(),
        );
    }
    lines.push(String::new());
    lines.push(
        "Every server that sets a destination channel above gets a copy automatically once the hub is live.".to_string(),
    );
    lines.push(String::new());
    lines.push(
        "🧹 **Clean Up** — auto-delete old relayed announcements in this server after a set delay"
            .to_string(),
    );

    let embed = embeds::brand(format!("{} WOS Announcements", icons::MEGAPHONE))
        .color(colors::BRAND)
        .description(lines.join("\n"))
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let mut buttons = vec![components::primary(
        "settings:wos:destination",
        format!("{} Destination", icons::EDIT),
    )];
    if view.destination.is_some() {
        buttons.push(components::danger(
            "settings:wos:clear",
            format!("{} Clear", icons::TRASH),
        ));
    }
    buttons.push(components::secondary(
        "settings:sneakpeek",
        format!("{} Sneak Peek", icons::SNOWFLAKE),
    ));
    if view.sneakpeek.is_some() {
        buttons.push(components::secondary(
            "settings:sneakpeek:sendlink",
            "🔗 Send Link",
        ));
    }
    buttons.push(components::secondary("settings:wos:cleanup", "🧹 Clean Up"));
    if view.is_owner {
        buttons.push(components::primary(
            "settings:wos:hub",
            format!("{} Set as Hub", icons::SERVER),
        ));
        if view.hub_configured {
            buttons.push(components::danger(
                "settings:wos:clearhub",
                format!("{} Clear Hub", icons::TRASH),
            ));
        }
    }

    (embed, components::rows(buttons))
}

pub fn wos_cleanup_panel(
    settings: &crate::db::welcome::CleanupSettings,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let status = if settings.enabled() {
        format!(
            "🟢 On — relayed WOS announcements are removed after **{} minutes**.",
            settings.delay_minutes()
        )
    } else {
        "🔴 Off — relayed WOS announcements are kept.".to_string()
    };
    let embed = embeds::brand("🧹 WOS Announcements Clean Up").description(format!(
        "Automatically delete this server's relayed announcements after a delay.\n\n{status}"
    ));
    let presets = [
        ("30 minutes", 30),
        ("1 hour", 60),
        ("6 hours", 360),
        ("12 hours", 720),
        ("1 day", 1440),
        ("3 days", 4320),
        ("7 days", 10080),
    ];
    let options = presets
        .into_iter()
        .map(|(label, minutes)| {
            serenity::CreateSelectMenuOption::new(label, minutes.to_string())
                .default_selection(settings.delay_minutes() == minutes)
        })
        .collect();
    let rows = vec![
        CreateActionRow::Buttons(vec![components::primary(
            "settings:wos:cleanup_toggle",
            if settings.enabled() {
                "🛑 Turn Off"
            } else {
                "✅ Turn On"
            },
        )]),
        CreateActionRow::SelectMenu(
            serenity::CreateSelectMenu::new(
                "settings:wos:cleanup_preset",
                serenity::CreateSelectMenuKind::String { options },
            )
            .placeholder("Choose cleanup delay")
            .min_values(1)
            .max_values(1),
        ),
        CreateActionRow::Buttons(vec![
            components::secondary("settings:wos:cleanup_custom", "✏️ Custom Delay"),
            components::secondary("settings:wos:refresh", "◀️ Back"),
        ]),
    ];
    (embed, rows)
}

/// Sneak Peek auto-detect panel.
#[derive(Debug, Clone, Default)]
pub struct SneakPeekView {
    pub channel: Option<u64>,
    pub last_article: Option<String>,
    pub last_checked: Option<String>,
}

pub fn sneakpeek_panel(view: &SneakPeekView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Sneak Peek", icons::SNOWFLAKE))
        .color(colors::TEAL)
        .description(
            "Auto-post new Chief Concierge: Sneak Peek articles from whiteoutsurvival.wiki.",
        )
        .field(
            "Configuration",
            embeds::rows(&[
                (
                    "Channel".into(),
                    view.channel
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "`off`".into()),
                ),
                (
                    "Last article".into(),
                    view.last_article
                        .clone()
                        .unwrap_or_else(|| "`none yet`".into()),
                ),
                (
                    "Last check".into(),
                    view.last_checked
                        .clone()
                        .unwrap_or_else(|| "`never`".into()),
                ),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![
        components::primary(
            "settings:sneakpeek:channel",
            format!("{} Channel", icons::EDIT),
        ),
        components::secondary(
            "settings:sneakpeek:check",
            format!("{} Check now", icons::REFRESH),
        ),
        components::secondary(
            "settings:sneakpeek:sendlink",
            format!("{} Send Link", icons::LINK),
        ),
    ];
    (embed, components::rows(buttons))
}

/// Cleanup (bulk delete) panel.
#[derive(Debug, Clone, Default)]
pub struct CleanupView;

pub fn cleanup_panel() -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Cleanup", icons::TRASH))
        .color(colors::MUTED)
        .description("Bulk-delete recent messages in this channel, with optional filters.")
        .field(
            "How to",
            "Tap **Clear** and type how many messages to scan (1–100).",
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let buttons = vec![components::danger(
        "settings:cleanup:clear",
        format!("{} Clear", icons::TRASH),
    )];
    (embed, components::rows(buttons))
}

/// Human label for the stored announcement mode, matching the JavaScript hub.
fn announce_mode_label(mode: &str) -> &str {
    match mode {
        "channel" => "Set channel",
        "off" => "Off",
        _ => "Same channel",
    }
}

fn leveling_embed(view: &LevelingView) -> CreateEmbed {
    let role_warning = if !view.bot_can_manage_roles {
        "\n⚠️ The bot needs **Manage Roles** to assign reward roles."
    } else {
        ""
    };
    let mut description =
        format!("Reward activity with XP and levels. Configure the engine below.{role_warning}");
    if let Some(notice) = &view.notice {
        description = format!("{notice}\n\n{description}");
    }
    let embed = embeds::brand(format!("{} Leveling", icons::BOLT))
        .color(colors::VIOLET)
        .description(description)
        .field(
            "XP engine",
            embeds::rows(&[
                (
                    "Enabled".into(),
                    if view.enabled {
                        "`yes`".into()
                    } else {
                        "`no`".into()
                    },
                ),
                (
                    "XP per message".into(),
                    format!("`{}–{}`", view.xp_min, view.xp_max),
                ),
                ("Cooldown".into(), format!("`{}s`", view.cooldown_sec)),
                ("Voice XP / min".into(), format!("`{}`", view.voice_xp)),
                (
                    "Announcement cleanup".into(),
                    if view.cleanup_enabled {
                        format!("`on · {} min`", view.cleanup_delay_minutes)
                    } else {
                        "`off`".into()
                    },
                ),
                (
                    "Announcement channel".into(),
                    view.announce_channel
                        .map(|id| format!("<#{}>", id))
                        .unwrap_or_else(|| "`same channel / unset`".into()),
                ),
                (
                    "Ignored channels".into(),
                    if view.ignored_channel_ids.is_empty() {
                        "`none`".into()
                    } else {
                        view.ignored_channel_ids
                            .iter()
                            .map(|id| format!("<#{}>", id))
                            .collect::<Vec<_>>()
                            .join(" ")
                    },
                ),
                (
                    "Public panel".into(),
                    if view.panel_deployed {
                        "`deployed`".into()
                    } else {
                        "`not deployed`".into()
                    },
                ),
            ]),
            false,
        )
        .field(
            "Announcement",
            embeds::rows(&[
                (
                    "Mode".into(),
                    format!("`{}`", announce_mode_label(&view.announce_mode)),
                ),
                (
                    "Role rewards".into(),
                    format!(
                        "`{}` ({})",
                        view.reward_count,
                        if view.stack_rewards {
                            "stack"
                        } else {
                            "replace"
                        }
                    ),
                ),
            ]),
            true,
        )
        .field(
            "This server",
            embeds::rows(&[
                ("Your level".into(), format!("`{}`", view.level)),
                ("Members ranked".into(), format!("`{}`", view.total_ranked)),
            ]),
            true,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    embed
}

pub fn leveling_panel(view: &LevelingView) -> (CreateEmbed, Vec<CreateActionRow>) {
    (
        leveling_embed(view),
        components::rows(leveling_buttons(view, false)),
    )
}

/// Buttons for the Leveling hub. `toolkit = true` builds the live Toolkit-post
/// variant: no "Back to Settings" (pointless on a post home view), the confirmed
/// R5 reset action instead of the plain reset picker, and an explicit Refresh.
fn leveling_buttons(view: &LevelingView, toolkit: bool) -> Vec<serenity::CreateButton> {
    let mut buttons = vec![
        components::primary(
            "settings:leveling:toggle",
            format!(
                "{} {}",
                if view.enabled { "Disable" } else { "Enable" },
                icons::BOLT
            ),
        ),
        components::secondary(
            "settings:leveling:xp",
            format!("{} XP Settings", icons::EDIT),
        ),
        components::secondary(
            "settings:leveling:announce",
            format!(
                "{} Announce: {}",
                icons::BELL,
                announce_mode_label(&view.announce_mode)
            ),
        ),
        components::secondary("settings:leveling:announce_channel", "📌 Announce Channel")
            .disabled(view.announce_mode != "channel"),
        components::secondary("settings:leveling:ignored", "🚫 Ignored Channels"),
        components::secondary("settings:leveling:stack", format!("{} Stack", icons::MEDAL)),
        components::secondary(
            "settings:leveling:message",
            format!("{} Message", icons::EDIT),
        ),
        components::secondary(
            "settings:leveling:rewards",
            format!("{} Rewards", icons::CROWN),
        ),
        components::secondary(
            if toolkit {
                "atk:leveling:reset"
            } else {
                "settings:leveling:reset"
            },
            format!("{} Reset XP", icons::TRASH),
        ),
        components::secondary("settings:leveling:cleanup", "🧹 Clean Up"),
        components::secondary("settings:leveling:deploy_panel", "📣 Deploy Public Panel"),
    ];
    if toolkit {
        buttons.push(components::secondary("atk:leveling:refresh", "🔄 Refresh"));
    } else {
        buttons.push(components::secondary(
            "settings:back",
            format!("{} Back to Settings", icons::BACK),
        ));
    }
    buttons
}

/// Leveling hub rendered directly inside the R4/R5 Toolkit forum post.
pub fn leveling_toolkit_panel(view: &LevelingView) -> (CreateEmbed, Vec<CreateActionRow>) {
    (
        leveling_embed(view),
        components::rows(leveling_buttons(view, true)),
    )
}

/// Human-readable delay, matching the JavaScript `formatDelay` helper.
fn format_delay(minutes: i64) -> String {
    if minutes % 1440 == 0 {
        let days = minutes / 1440;
        format!("{days} day{}", if days == 1 { "" } else { "s" })
    } else if minutes % 60 == 0 {
        let hours = minutes / 60;
        format!("{hours} hour{}", if hours == 1 { "" } else { "s" })
    } else {
        format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" })
    }
}

pub fn leveling_cleanup_panel(
    settings: &crate::db::welcome::CleanupSettings,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let status = if settings.enabled() {
        format!(
            "On — level-up announcements are deleted **{}** after being sent.",
            format_delay(settings.delay_minutes())
        )
    } else {
        "Off — level-up announcements are kept.".to_string()
    };
    let embed = embeds::brand("🧹 Leveling Clean Up")
        .color(colors::BRAND)
        .description(format!(
        "Automatically remove level-up announcements so they do not flood channels.\n\n{status}"
    ));
    let options = [
        ("30 minutes", 30),
        ("1 hour", 60),
        ("6 hours", 360),
        ("12 hours", 720),
        ("1 day", 1440),
        ("3 days", 4320),
        ("7 days", 10080),
    ]
    .into_iter()
    .map(|(label, minutes)| {
        let mut option = serenity::CreateSelectMenuOption::new(label, minutes.to_string());
        if settings.delay_minutes() == minutes {
            option = option.default_selection(true);
        }
        option
    })
    .collect::<Vec<_>>();
    let select = serenity::CreateSelectMenu::new(
        "settings:leveling:cleanup_delay",
        serenity::CreateSelectMenuKind::String { options },
    )
    .placeholder(format!(
        "Delay — currently {}",
        format_delay(settings.delay_minutes())
    ))
    .min_values(1)
    .max_values(1);
    let toggle = if settings.enabled() {
        components::danger("settings:leveling:cleanup_toggle", "🛑 Turn Off")
    } else {
        components::success("settings:leveling:cleanup_toggle", "✅ Turn On")
    };
    let rows = vec![
        CreateActionRow::Buttons(vec![toggle]),
        CreateActionRow::SelectMenu(select),
        CreateActionRow::Buttons(vec![
            components::secondary("settings:leveling:cleanup_custom", "✏️ Custom Delay"),
            components::secondary("settings:leveling:cleanup_back", "◀️ Back"),
        ]),
    ];
    (embed, rows)
}

/// Public, member-facing Leveling panel deployed by an admin with
/// "Deploy Public Panel" on the Leveling hub. All controls are read-only and
/// reply privately, so clicking never edits this shared message.
pub fn leveling_public_panel() -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Levelling & Leaderboard", icons::CROWN))
        .color(colors::VIOLET)
        .description(
            "Check your level, XP and server rank, or browse the XP leaderboard.\n\
             Use the member picker to look up someone else's rank.",
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    let buttons = CreateActionRow::Buttons(vec![
        components::success("lvl:rank", format!("{} My Rank", icons::MEDAL)),
        components::primary("lvl:lb:0", "🏆 Leaderboard"),
    ]);
    let picker = CreateActionRow::SelectMenu(
        serenity::CreateSelectMenu::new(
            "lvl:rank_select",
            serenity::CreateSelectMenuKind::User {
                default_users: None,
            },
        )
        .placeholder("Check a member's rank…")
        .min_values(1)
        .max_values(1),
    );
    (embed, vec![buttons, picker])
}

/// Fancy player card — this is the panel the legacy bot posted in the music
/// channel, rebuilt with a progress bar and cleaner controls.
pub fn music(view: &NowPlayingView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let mut embed = embeds::brand(format!("{} Music", icons::MUSIC)).color(colors::BRAND_DEEP);

    if view.title.is_empty() {
        embed = embed.description(format!(
            "Nothing is playing.\n\n{}",
            "Use the Music panel's play button, or drop a link in this channel to queue something."
        ));
    } else {
        let mut desc = String::new();
        if let Some(uri) = view.uri.as_deref() {
            desc.push_str(&format!("**[{}]({})**\n", view.title, uri));
        } else {
            desc.push_str(&format!("**{}**\n", view.title));
        }
        if !view.author.is_empty() {
            desc.push_str(&format!("by `{}`\n", view.author));
        }
        desc.push_str(&format!(
            "\n{}\n`{}` {} `{}`\n{}",
            theme::DIVIDER,
            theme::duration(view.position_ms),
            theme::progress_bar(view.position_ms, view.length_ms, 18),
            theme::duration(view.length_ms),
            theme::DIVIDER,
        ));
        embed = embed.description(desc).field(
            "Status",
            embeds::rows(&[
                (
                    "Playback".to_string(),
                    if view.paused {
                        "`⏸ paused`".into()
                    } else {
                        "`▶ playing`".into()
                    },
                ),
                ("Volume".to_string(), format!("`{}%`", view.volume)),
                ("Loop".to_string(), format!("`{}`", view.loop_label)),
                (
                    "Queue".to_string(),
                    format!("`{} track(s)`", view.queue_len),
                ),
                (
                    "Voice".to_string(),
                    view.connected_channel
                        .clone()
                        .unwrap_or_else(|| "`not connected`".into()),
                ),
            ]),
            true,
        );
        if !view.requester.is_empty() {
            embed = embed.field("Requested by", view.requester.clone(), true);
        }
        if !view.upcoming.is_empty() {
            let list = view
                .upcoming
                .iter()
                .take(5)
                .enumerate()
                .map(|(i, (name, len))| {
                    format!("`{}.` {} — `{}`", i + 1, error_trunc(name, 55), len)
                })
                .collect::<Vec<_>>()
                .join("\n");
            embed = embed.field(format!("{} Up next", icons::QUEUE), list, false);
        }
    }

    if let Some(url) = view.uri.clone() {
        embed = embed.thumbnail(url);
    }

    let play_pause = if view.paused {
        components::success("music:resume", format!("{} Resume", icons::PLAY))
    } else {
        components::secondary("music:pause", format!("{} Pause", icons::PAUSE))
    };

    let row1 = components::row(vec![
        components::secondary("music:previous", icons::PREVIOUS),
        play_pause,
        components::secondary("music:skip", format!("{} Skip", icons::SKIP)),
        components::danger("music:stop", format!("{} Stop", icons::STOP)),
    ]);
    let row2 = components::row(vec![
        secondary("music:shuffle", format!("{} Shuffle", icons::SHUFFLE)),
        secondary(
            "music:loop",
            format!("{} Loop · {}", icons::REPEAT, view.loop_label),
        ),
        secondary("music:voldown", format!("{} Vol -", icons::VOLUME)),
        secondary("music:volup", format!("{} Vol +", icons::VOLUME)),
        secondary("music:queue", format!("{} Queue", icons::QUEUE)),
    ]);

    (embed, vec![row1, row2])
}

fn error_trunc(input: &str, limit: usize) -> String {
    crate::error::truncate(input, limit)
}

/// The compact "now playing" line used when a track starts.
pub fn now_playing(view: &NowPlayingView) -> CreateEmbed {
    let mut embed = embeds::base(colors::SUCCESS)
        .author(serenity::CreateEmbedAuthor::new(format!(
            "{} Now playing",
            icons::PLAY
        )))
        .title(error_trunc(&view.title, 200))
        .description(format!(
            "`{}` · requested by {}\n{}",
            view.author,
            if view.requester.is_empty() {
                "someone".to_string()
            } else {
                view.requester.clone()
            },
            theme::duration(view.length_ms)
        ))
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    if let Some(uri) = view.uri.clone() {
        embed = embed.url(uri);
    }
    embed
}

// ------------------------------------------------------------------- lookup

pub fn lookup_player(view: &LookupPlayerView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let title = view.game_name.clone().unwrap_or_else(|| view.fid.clone());
    let mut embed = embeds::brand(format!("{} Player lookup", icons::PLAYER))
        .color(colors::INFO)
        .title(format!("{} {}", icons::PLAYER, error_trunc(&title, 120)))
        .field("FID", format!("`{}`", view.fid), true)
        .field(
            "State",
            view.state
                .map(|s| format!("`#{s}`"))
                .unwrap_or_else(|| "`unknown`".into()),
            true,
        )
        .field(
            format!("{} Alliance", icons::ALLIANCE),
            view.alliance.clone().unwrap_or_else(|| "`none`".into()),
            true,
        )
        .field(
            format!("{} Power", icons::BOLT),
            view.power
                .map(|v| format!("`{}`", theme::compact_number(v)))
                .unwrap_or_else(|| "`—`".into()),
            true,
        )
        .field(
            format!("{} Kills", icons::SWORD),
            view.kills
                .map(|v| format!("`{}`", theme::compact_number(v)))
                .unwrap_or_else(|| "`—`".into()),
            true,
        )
        .field(
            format!("{} Town Hall", icons::TOWN),
            view.town_hall
                .map(|v| format!("`{v}`"))
                .unwrap_or_else(|| "`—`".into()),
            true,
        )
        .field(
            format!("{} Life Tree", icons::TREE),
            view.life_tree
                .map(|v| format!("`{v}`"))
                .unwrap_or_else(|| "`—`".into()),
            true,
        )
        .field(
            format!("{} Last sync", icons::CLOCK),
            view.synced
                .map(embeds::db_relative)
                .unwrap_or_else(|| "`never`".into()),
            true,
        );

    if let Some(url) = view.avatar_url.clone() {
        embed = embed.thumbnail(url);
    }

    let rows = vec![components::row(vec![
        components::primary(
            "lookup:player_search",
            format!("{} Search again", icons::LOOKUP),
        ),
        secondary("lookup:refresh", format!("{} Refresh", icons::REFRESH)),
    ])];
    (embed, rows)
}

pub fn lookup_alliance(view: &LookupAllianceView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let name = view.name.clone().unwrap_or_else(|| view.abbr.clone());
    let mut embed = embeds::brand(format!("{} Alliance lookup", icons::ALLIANCE))
        .color(colors::VIOLET)
        .title(format!(
            "{} {}",
            icons::ALLIANCE,
            error_trunc(&format!("[{}] {name}", view.abbr), 120)
        ))
        .field("Members tracked", format!("`{}`", view.members), true)
        .field(
            format!("{} Total power", icons::BOLT),
            format!("`{}`", theme::compact_number(view.power)),
            true,
        )
        .field(
            format!("{} Total kills", icons::SWORD),
            format!("`{}`", theme::compact_number(view.kills)),
            true,
        )
        .field(
            format!("{} Last sync", icons::CLOCK),
            view.synced
                .map(embeds::db_relative)
                .unwrap_or_else(|| "`never`".into()),
            true,
        );

    if !view.top.is_empty() {
        let leaderboard = view
            .top
            .iter()
            .take(10)
            .enumerate()
            .map(|(i, (name, power))| {
                let medal = match i {
                    0 => icons::CROWN,
                    1 => icons::MEDAL,
                    2 => icons::BOLT,
                    _ => "▫️",
                };
                format!(
                    "{medal} **{}** — `{}`",
                    error_trunc(name, 40),
                    theme::compact_number(*power)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        embed = embed.field("Top members", leaderboard, false);
    }

    let rows = vec![components::row(vec![
        components::primary(
            "lookup:alliance_search",
            format!("{} Search", icons::LOOKUP),
        ),
        secondary("lookup:refresh", format!("{} Refresh", icons::REFRESH)),
    ])];
    (embed, rows)
}

pub fn lookup_state(view: &StateView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let mut embed = embeds::brand(format!("{} State overview", icons::STATE))
        .color(colors::TEAL)
        .title(format!("{} State #{}", icons::STATE, view.state))
        .field("Alliances", format!("`{}`", view.alliances), true)
        .field("Players", format!("`{}`", view.players), true)
        .field(
            format!("{} State power", icons::BOLT),
            format!("`{}`", theme::compact_number(view.power)),
            true,
        );

    if !view.top_alliances.is_empty() {
        let lines = view
            .top_alliances
            .iter()
            .take(12)
            .enumerate()
            .map(|(i, (abbr, power, members))| {
                format!(
                    "`{:>2}.` **{}** — `{}` power · `{}` members",
                    i + 1,
                    error_trunc(abbr, 8),
                    theme::compact_number(*power),
                    members
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        embed = embed.field("Top alliances", lines, false);
    }

    let rows = vec![components::row(vec![
        components::primary(
            "lookup:state_search",
            format!("{} Change state", icons::LOOKUP),
        ),
        secondary("lookup:refresh", format!("{} Refresh", icons::REFRESH)),
    ])];
    (embed, rows)
}

/// Public search panel — deployed to a channel from /settings → Lookup.
pub fn lookup_public_panel() -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} WoS Lookup", icons::LOOKUP))
        .color(colors::INFO)
        .description(format!(
            "Search tracked players, alliances and states.\n\n{}",
            theme::DIVIDER
        ))
        .field(
            format!("{} Player", icons::PLAYER),
            "Look up a player by FID or in-game name.",
            true,
        )
        .field(
            format!("{} Alliance", icons::ALLIANCE),
            "See every tracked member of an alliance.",
            true,
        )
        .field(
            format!("{} State", icons::STATE),
            "Rank alliances inside a state.",
            true,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let rows = vec![
        components::row(vec![
            components::primary("lookup:player_search", format!("{} Player", icons::PLAYER)),
            components::primary(
                "lookup:alliance_search",
                format!("{} Alliance", icons::ALLIANCE),
            ),
            components::primary("lookup:state_search", format!("{} State", icons::STATE)),
        ]),
        components::row(vec![
            secondary(
                "lookup:leaderboard",
                format!("{} Power board", icons::CROWN),
            ),
            secondary("lookup:refresh", format!("{} Refresh", icons::REFRESH)),
        ]),
    ];
    (embed, rows)
}

// --------------------------------------------------------------- gift codes

pub struct GiftCodeView {
    pub code: String,
    pub status: String,
    pub source: String,
    pub added: i64,
    pub vip: bool,
}

pub fn giftcode_panel(
    stats: &crate::db::giftcodes::GiftCodeStats,
    recent: &[GiftCodeView],
    sync: Option<i64>,
    auto_sync: bool,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let mut embed = embeds::brand(format!("{} Gift codes", icons::GIFT))
        .color(colors::GOLD)
        .description(format!(
            "Community gift codes, synced from Oracle and added by hand.\n{}",
            theme::DIVIDER
        ))
        .field(
            format!("{} Numbers", icons::DATABASE),
            embeds::rows(&[
                ("Total".to_string(), format!("`{}`", stats.total)),
                ("Active".to_string(), format!("`{}`", stats.active)),
                ("VIP".to_string(), format!("`{}`", stats.vip)),
                ("Last 24h".to_string(), format!("`{}`", stats.today)),
            ]),
            true,
        )
        .field(
            format!("{} Sync", icons::REFRESH),
            embeds::rows(&[
                (
                    "Last sync".to_string(),
                    sync.map(embeds::db_relative)
                        .unwrap_or_else(|| "`never`".into()),
                ),
                (
                    "Auto sync".to_string(),
                    if auto_sync {
                        "`on`".to_string()
                    } else {
                        "`off`".to_string()
                    },
                ),
            ]),
            true,
        );

    if recent.is_empty() {
        embed = embed.field("Recent codes", "_No codes stored yet._", false);
    } else {
        let list = recent
            .iter()
            .take(10)
            .map(|c| {
                format!(
                    "`{}` {}{} — {}",
                    c.code,
                    if c.vip {
                        format!("{} ", icons::CROWN)
                    } else {
                        String::new()
                    },
                    c.status,
                    embeds::db_relative(c.added)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        embed = embed.field("Recent codes", list, false);
    }

    let buttons = vec![
        components::primary("gift:add", format!("{} Add code", icons::ADD)),
        secondary("gift:list", format!("{} Browse", icons::LIST)),
        secondary("gift:sync", format!("{} Sync Oracle", icons::REFRESH)),
        secondary("gift:announce", format!("{} Announce", icons::BELL)),
        components::danger("gift:delete", format!("{} Delete", icons::TRASH)),
    ];
    (embed, components::rows(buttons))
}

/// The public "Open Ticket" panel (posted from /settings → Tickets).
/// JS `tickets/flow.buildPanelEmbed` + `buildPanelRow`.
pub fn ticket_panel(
    settings: &crate::db::tickets::TicketSettings,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    crate::tickets::panel_payload(settings)
}

/// Announcement card posted when new codes appear.
pub fn giftcode_announce(codes: &[String]) -> CreateEmbed {
    let list = codes
        .iter()
        .map(|c| format!("{} `{}`", icons::GIFT, c))
        .collect::<Vec<_>>()
        .join("\n");
    embeds::base(colors::GOLD)
        .author(serenity::CreateEmbedAuthor::new(format!(
            "{} New gift code{}",
            icons::GIFT,
            if codes.len() == 1 { "" } else { "s" }
        )))
        .description(list)
        .footer(serenity::CreateEmbedFooter::new(theme::footer()))
}

// --------------------------------------------------------------------- help

pub fn help_panel(_prefix: &str) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Help", icons::BRAND))
        .color(colors::BRAND)
        .description(format!(
            "A single bot for Discord communities and Whiteout Survival alliances.\n{}",
            theme::DIVIDER
        ))
        .field(
            format!("{} Players", icons::PLAYER),
            "**Players** — track WoS IDs, power, alliances & state\n➡️ open /settings → **Players**".to_string(),
            false,
        )
        .field(
            format!("{} Lookup", icons::LOOKUP),
            "**Lookup panel** — search players, alliances, states & leaderboards on a public panel\n➡️ /settings → **Lookup**, or tap **Search** on the deployed panel".to_string(),
            false,
        )
        .field(
            format!("{} Music", icons::MUSIC),
            "**Music panel** — play, pause, queue, volume from buttons in the panel channel\n➡️ /settings → **Music** → **Deploy panel here**".to_string(),
            false,
        )
        .field(
            format!("{} Gift codes", icons::GIFT),
            "**Gift Codes** — sync, announce and manage WoS codes\n➡️ /settings → **Gift Codes**".to_string(),
            false,
        )
        .field(
            format!("{} Admin", icons::ADMIN),
            "**Everything else** — tickets, votes, welcome, leveling, music, temp voice, messages, giveaways, calculators… all from /settings".to_string(),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let rows = vec![components::row(vec![
        components::primary("help:players", format!("{} Players", icons::PLAYER)),
        components::primary("help:lookup", format!("{} Lookup", icons::LOOKUP)),
        components::primary("help:music", format!("{} Music", icons::MUSIC)),
        components::primary("help:gift", format!("{} Gift", icons::GIFT)),
    ])];
    (embed, rows)
}

/// Standard "list too long" pager embed.
pub fn paged_list(title: &str, page: usize, pages: usize, body: String) -> CreateEmbed {
    embeds::brand(title)
        .color(colors::BRAND_DEEP)
        .description(body)
        .footer(serenity::CreateEmbedFooter::new(format!(
            "Page {}/{} • {}",
            page + 1,
            pages.max(1),
            theme::footer()
        )))
}

#[allow(dead_code)]
fn unused_style(_: ButtonStyle) {}

// -------------------------------------------------------------- branding

#[derive(Debug, Clone, Default)]
pub struct BrandingView {
    pub contact_enabled: bool,
    pub contact_label: String,
    pub contact_url: Option<String>,
    pub can_manage: bool,
    pub is_owner: bool,
    pub nickname: Option<String>,
    pub avatar_url: Option<String>,
}

pub fn branding_panel(view: &BrandingView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let contact_status = if view.contact_enabled {
        format!(
            "🟢 **On** — [{}]({})",
            view.contact_label,
            view.contact_url.clone().unwrap_or_default()
        )
    } else {
        "🔴 **Off**".to_string()
    };
    let embed = embeds::brand(format!("{} Branding", icons::TAG))
        .color(colors::VIOLET)
        .description("Control how the bot presents itself in this server.")
        .field(
            "Contact button (all servers, owner only)",
            format!("{contact_status}\n_Shown under Messages-feature embeds._"),
            false,
        )
        .field(
            "This server",
            embeds::rows(&[
                (
                    "Nickname".into(),
                    view.nickname.clone().unwrap_or_else(|| "`default`".into()),
                ),
                (
                    "Avatar".into(),
                    view.avatar_url
                        .as_deref()
                        .map(|_| "`custom`".to_string())
                        .unwrap_or_else(|| "`default`".into()),
                ),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let mut buttons = Vec::new();
    if view.is_owner {
        buttons.push(components::primary("branding:toggle", "✉️ Contact on/off"));
        buttons.push(components::secondary(
            "branding:contact",
            "✏️ Edit contact link",
        ));
        buttons.push(components::secondary(
            "branding:clearcontact",
            "🗑️ Clear contact",
        ));
    }
    if view.can_manage {
        buttons.push(components::secondary(
            "branding:nickname",
            "🏷️ Set bot nickname",
        ));
        buttons.push(components::secondary(
            "branding:avatar",
            "🖼️ Set bot avatar",
        ));
        buttons.push(components::secondary(
            "branding:clearnick",
            "🏷️ Clear nickname",
        ));
        buttons.push(components::secondary(
            "branding:clearavatar",
            "🖼️ Clear avatar",
        ));
    }

    let mut rows = Vec::new();
    if !buttons.is_empty() {
        rows.push(CreateActionRow::Buttons(buttons));
    }
    rows.push(CreateActionRow::Buttons(vec![components::secondary(
        "settings:home",
        format!("{} Back", icons::BACK),
    )]));
    (embed, rows)
}

// ---------------------------------------------------------------- ministry

/// Ministry settings page (ephemeral) and the persistent admin panel.
#[derive(Debug, Clone, Default)]
pub struct MinistryView {
    pub svs_number: i64,
    pub applications_open: bool,
    pub google: String,
    pub channel: Option<u64>,
    /// `None` when no SVS form exists; otherwise `(label, url, closed)`.
    pub form: Option<(String, String, bool)>,
    pub spreadsheet_url: Option<String>,
    pub last_sheet_sync_at: Option<String>,
    pub total: i64,
    pub approved: i64,
    pub pending: i64,
}

fn ministry_embed(view: &MinistryView, title: &str) -> CreateEmbed {
    let app_line = if view.applications_open {
        "🟢 Applications: **Open**"
    } else {
        "🔴 Applications: **Closed**"
    };
    let form_line = match &view.form {
        Some((label, url, closed)) => {
            if *closed {
                format!("📄 **{label}** — closed")
            } else {
                format!("📄 **{label}** — [Open form]({url})")
            }
        }
        None => "📄 **No SVS form** — create one below".to_string(),
    };
    let sheet_line = view
        .spreadsheet_url
        .as_deref()
        .map(|url| format!("📑 [Reservation workspace]({url})"))
        .unwrap_or_else(|| "📑 Workspace will be created with the form".to_string());

    embeds::brand(title)
        .color(colors::BRAND)
        .description(format!(
            "{app_line}\n**SVS #{}**\n{form_line}\n{sheet_line}\n\n**Google:** {}\n**Last sync:** {}\n**Submissions:** `{}` total · `{}` approved · `{}` pending",
            view.svs_number,
            view.google,
            view.last_sheet_sync_at.as_deref().map(|at| format!("{at} UTC")).unwrap_or_else(|| "_never_".to_string()),
            view.total,
            view.approved,
            view.pending,
        ))
        .footer(serenity::CreateEmbedFooter::new(theme::footer()))
}

pub fn ministry_panel(view: &MinistryView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = ministry_embed(view, &format!("{} Ministry — Forms", icons::LIST));
    let rows = vec![
        CreateActionRow::Buttons(vec![
            components::primary("ministry:svs", "📝 SVS Form / Sheet"),
            components::primary("ministry:custom", "📝 Custom Excel / Form"),
            components::success("ministry:export_menu", "📊 Export SVS"),
        ]),
        CreateActionRow::Buttons(vec![
            components::secondary("ministry:history", format!("{} History", icons::LIST)),
            components::secondary("ministry:settings", "📌 Panel"),
            components::secondary(
                "ministry:refresh",
                format!("{} Refresh All", icons::REFRESH),
            ),
        ]),
        // The JS Toolkit live form panel appends this shortcut beneath its
        // six-button hub; preserve it when the Rust panel is rendered in-post.
        CreateActionRow::Buttons(vec![components::secondary(
            "settings:ministry",
            "⚙️ Forms Settings",
        )]),
    ];
    (embed, rows)
}

/// Forms Toolkit post: the JS panel renders `ministry/panelV2` admin embed
/// plus its six hub buttons and the Forms Settings shortcut directly in the
/// shared post; clicks still open private menus.
pub fn forms_toolkit_panel(view: &MinistryView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let applications = if view.applications_open {
        "🟢 Open"
    } else {
        "🔴 Closed"
    };
    let google = if view.google.contains("🟢") {
        "🟢 connected"
    } else {
        "🔴 connect via Owner Panel"
    };
    let form_line = match &view.form {
        Some((label, url, closed)) if !url.is_empty() && !*closed => {
            format!("**Active SVS form:** [Open form]({url}) · status `{label}`")
        }
        Some((label, _, closed)) if *closed => {
            format!("**Active SVS form:** {label} · status `closed`")
        }
        _ => "**Active SVS form:** _none — use SVS Form / Sheet to create_".to_string(),
    };
    let sheet_line = view
        .spreadsheet_url
        .as_deref()
        .map(|url| format!("**Workspace sheet:** [Open spreadsheet]({url})"))
        .unwrap_or_default();
    let sync_line = view
        .last_sheet_sync_at
        .as_deref()
        .map(|at| format!("**Last sync:** {at} UTC"))
        .unwrap_or_else(|| "**Last sync:** _never_".to_string());
    let mut lines = vec![
        "**Admin control panel**".to_string(),
        format!(
            "**SVS #{}** · Applications: {applications}",
            view.svs_number
        ),
        format!("**Google:** {google}"),
        form_line,
    ];
    if !sheet_line.is_empty() {
        lines.push(sheet_line);
    }
    lines.push(sync_line);
    lines.push(String::new());
    lines.push(
        "🌐 **Translate** — private copy in your language\n\
         Add players by editing the **Google Sheet** (Player Name + Player ID), then **Refresh All**."
            .to_string(),
    );

    let embed = embeds::brand("📋 Forms — Ministry Update")
        .color(colors::BRAND)
        .description(lines.join("\n"))
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    let (_, rows) = ministry_panel(view);
    (embed, rows)
}

pub fn ministry_export_panel(view: &MinistryView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = ministry_embed(view, "📊 Export SVS")
        .description("Download snapshots of the current SVS data. CSV and Excel export submissions; PNG exports schedule tables as shareable images.");
    let rows = vec![
        CreateActionRow::Buttons(vec![
            components::success("ministry:export_xlsx", "📥 Excel (.xlsx)"),
            components::primary("ministry:export_png", "🖼️ Images (.png)"),
            components::secondary("ministry:export", "CSV"),
        ]),
        CreateActionRow::Buttons(vec![components::secondary("ministry:home", "◀️ Back")]),
    ];
    (embed, rows)
}

pub fn ministry_custom_panel(
    forms: &[crate::ministry::MinistryForm],
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let body = if forms.is_empty() {
        "_No custom forms yet._".to_string()
    } else {
        forms
            .iter()
            .take(10)
            .map(|form| {
                let title = form.label.as_deref().unwrap_or("Untitled form");
                let edit = form
                    .form_id
                    .as_deref()
                    .map(|id| format!(" · [Edit](https://docs.google.com/forms/d/{id}/edit)"))
                    .unwrap_or_default();
                let public = form
                    .form_url
                    .as_deref()
                    .map(|url| format!(" · [Public form]({url})"))
                    .unwrap_or_default();
                let sheet = form
                    .spreadsheet_id()
                    .map(|id| {
                        format!(" · [Responses](https://docs.google.com/spreadsheets/d/{id}/edit)")
                    })
                    .unwrap_or_default();
                format!(
                    "**#{} {}** · `{}`{}{}{}",
                    form.id, title, form.status, edit, public, sheet
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let embed = embeds::brand("📝 Custom Forms")
        .description(format!("Create blank Google Forms for any event, add questions in Google Forms, then sync responses here.\n\n**Recent forms**\n{body}"))
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    let rows = vec![
        CreateActionRow::Buttons(vec![
            components::success("ministry:custom_create", "➕ Create Form"),
            components::primary("ministry:custom_sync", "🔄 Sync Responses"),
        ]),
        CreateActionRow::Buttons(vec![
            components::secondary("ministry:custom_edit", "✏️ Edit Form"),
            components::secondary("ministry:custom_view", "👁️ View Form"),
            components::danger("ministry:custom_close", "🔒 Close Form"),
        ]),
        CreateActionRow::Buttons(vec![components::secondary(
            "ministry:home",
            "◀️ Forms Home",
        )]),
    ];
    (embed, rows)
}

/// JS `minv2:custom:{edit|view|close}` picker with validation and options.
pub fn ministry_custom_picker_panel(
    mode: &str,
    forms: &[crate::ministry::MinistryForm],
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let (title, description) = match mode {
        "edit" => (
            "✏️ Edit custom form",
            "Select a form to open its **Google Forms editor** (add/edit questions there).",
        ),
        "view" => (
            "👁️ View custom form",
            "Select a form to see public submit link and response sheet.",
        ),
        _ => (
            "🔒 Close custom form",
            "Select a form to **close** (stops new submissions; data is kept).",
        ),
    };
    let options = forms
        .iter()
        .take(25)
        .map(|form| {
            let label = form
                .label
                .as_deref()
                .or(form.form_id.as_deref())
                .unwrap_or("Untitled form");
            let label: String = label.chars().take(100).collect();
            let description: String = format!("{} · #{}", form.status, form.id)
                .chars()
                .take(100)
                .collect();
            serenity::CreateSelectMenuOption::new(label, form.id.to_string())
                .description(description)
        })
        .collect::<Vec<_>>();
    let embed = embeds::brand(title)
        .description(description)
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    let rows = vec![
        CreateActionRow::SelectMenu(
            serenity::CreateSelectMenu::new(
                format!("ministry:custom_pick:{mode}"),
                serenity::CreateSelectMenuKind::String { options },
            )
            .placeholder("Select a form…")
            .min_values(1)
            .max_values(1),
        ),
        components::row(vec![components::secondary(
            "ministry:custom",
            format!("{} Back", icons::BACK),
        )]),
    ];
    (embed, rows)
}

pub fn ministry_settings_panel(view: &MinistryView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = ministry_embed(view, &format!("{} Ministry — Settings", icons::SETTINGS)).field(
        "Panel channel",
        view.channel
            .map(|id| format!("<#{id}>"))
            .unwrap_or_else(|| "`not set` — use **Admin Panel Channel**".to_string()),
        false,
    );
    let rows = vec![
        CreateActionRow::Buttons(vec![
            components::primary(
                "ministry:channel",
                format!("{} Admin Panel Channel", icons::EDIT),
            ),
            components::primary(
                "ministry:post_here",
                format!("{} Open Panel Here", icons::ADD),
            ),
            components::secondary(
                "ministry:repost",
                format!("{} Repost Panel", icons::REFRESH),
            ),
        ]),
        CreateActionRow::Buttons(vec![
            components::secondary("ministry:svsset", "#️⃣ Set SVS #"),
            components::secondary(
                "ministry:toggleapps",
                if view.applications_open {
                    "🔒 Close applications"
                } else {
                    "🔓 Open applications"
                },
            ),
            components::secondary("ministry:google", "🔗 Google"),
        ]),
        CreateActionRow::Buttons(vec![components::secondary(
            "ministry:home",
            format!("{} Back", icons::BACK),
        )]),
    ];
    (embed, rows)
}

pub fn ministry_svs_panel(view: &MinistryView) -> (CreateEmbed, Vec<CreateActionRow>) {
    let has_open_form = matches!(&view.form, Some((_, _, false)));
    let has_any_form = view.form.is_some();
    let embed = ministry_embed(view, &format!("{} Ministry — SVS", icons::SWORD))
        .field(
            "Unified SVS reservation form",
            "One open form per server. Creating a form also creates the reservation workspace spreadsheet (Applicant + TD/CD/RD tabs) and shares it anyone-with-link.",
            false,
        );
    let create_btn = components::success("ministry:create", "➕ Create Unified SVS");
    let close_btn = components::danger("ministry:close", "🔒 Close Form");
    let share_btn = components::primary("ministry:share", "🔗 Share Public");
    let copy_btn = components::secondary("ministry:copy", "📋 Copy links");
    let rows = vec![
        CreateActionRow::Buttons(vec![
            create_btn.disabled(has_open_form),
            close_btn.disabled(!has_open_form),
        ]),
        CreateActionRow::Buttons(vec![
            share_btn.disabled(!has_any_form),
            copy_btn.disabled(!has_any_form),
        ]),
        CreateActionRow::Buttons(vec![components::secondary(
            "ministry:settings",
            format!("{} Settings", icons::BACK),
        )]),
    ];
    (embed, rows)
}

pub fn ministry_google_panel(
    view: &MinistryView,
    auth_url: Option<String>,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = embeds::brand(format!("{} Ministry — Google", icons::LINK))
        .color(colors::BRAND)
        .description(format!(
            "**Google:** {}\n\nMinistry forms, the reservation workspace and the applicant-sheet live write need a Google account with Sheets + Forms + Drive access.\n\n1. Click **Continue with Google** (opens Google sign-in)\n2. Sign in and grant access\n3. Copy the code (or the whole redirect URL)\n4. Click **Paste code** and paste it here",
            view.google,
        ))
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let mut rows = Vec::new();
    if let Some(url) = auth_url {
        rows.push(CreateActionRow::Buttons(vec![components::link(
            "Continue with Google",
            &url,
        )]));
    }
    rows.push(CreateActionRow::Buttons(vec![
        components::primary("ministry:pastecode", "📋 Paste code"),
        components::danger("ministry:disconnect", "🗑️ Disconnect"),
    ]));
    rows.push(CreateActionRow::Buttons(vec![components::secondary(
        "ministry:settings",
        format!("{} Back", icons::BACK),
    )]));
    (embed, rows)
}

/// History page — latest submissions + pending picker for review.
/// Picking an entry opens the review page via `ministry:review:{id}`.
pub fn ministry_history_panel(
    view: &MinistryView,
    forms: &[String],
    recent: &[String],
    pending_options: &[serenity::CreateSelectMenuOption],
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let forms_lines = if forms.is_empty() {
        "_No forms yet._".to_string()
    } else {
        forms.join("\n")
    };
    let mut lines = recent.join("\n");
    if lines.is_empty() {
        lines = "_No submissions yet — they appear here after the form syncs._".to_string();
    }
    let embed = ministry_embed(view, &format!("{} Ministry — History", icons::LIST))
        .field("Recent forms", forms_lines, false)
        .field(format!("Latest submissions ({})", view.total), lines, false);

    let mut rows = Vec::new();
    if !pending_options.is_empty() {
        rows.push(CreateActionRow::SelectMenu(
            serenity::CreateSelectMenu::new(
                "ministry:review",
                serenity::CreateSelectMenuKind::String {
                    options: pending_options.to_vec(),
                },
            )
            .placeholder(format!("Review pending ({})", pending_options.len()))
            .min_values(1)
            .max_values(1),
        ));
    }
    rows.push(CreateActionRow::Buttons(vec![components::secondary(
        "ministry:home",
        format!("{} Back", icons::BACK),
    )]));
    (embed, rows)
}

/// Review page for one submission (approve / reject).
pub fn ministry_review_panel(
    view: &MinistryView,
    submission: &crate::ministry::Submission,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let id = submission.id;
    let embed = ministry_embed(view, &format!("{} Ministry — Review #{id}", icons::LIST)).field(
        format!("Submission #{id}"),
        format!(
            "**Applicant:** {}\n**FID:** `{}`\n**Events:** {}\n**FC:** {} · **rFC:** {}\n**TS:** {} · **CS:** {} · **RS:** {} · **GS:** {}\n**Preferred time:** {}\n**Status:** `{}`",
            submission.applicant_name.as_deref().unwrap_or("?"),
            submission.fid,
            submission.events(),
            submission.fc.as_deref().unwrap_or("—"),
            submission.rfc.as_deref().unwrap_or("—"),
            submission.ts.as_deref().unwrap_or("—"),
            submission.cs.as_deref().unwrap_or("—"),
            submission.rs.as_deref().unwrap_or("—"),
            submission.gs.as_deref().unwrap_or("—"),
            submission.preferred_time.as_deref().unwrap_or("—"),
            submission.app_status,
        ),
        false,
    );
    let rows = vec![CreateActionRow::Buttons(vec![
        components::success(format!("ministry:approve:{id}"), "✅ Approve"),
        components::danger(format!("ministry:reject:{id}"), "❌ Reject"),
        components::secondary("ministry:history", format!("{} Back", icons::BACK)),
    ])];
    (embed, rows)
}

#[cfg(test)]
mod settings_menu_tests {
    use super::*;
    use serde_json::Value;

    fn ctx(official_guild: bool, player_limit: Option<i64>) -> SettingsContext {
        SettingsContext {
            guild_name: "Test Guild".to_string(),
            player_ids: 7,
            player_limit,
            music_configured: false,
            lookup_configured: false,
            gift_sync: None,
            oracle_ready: false,
            uptime: "1m".to_string(),
            latency_ms: 0,
            is_owner: false,
            official_guild,
        }
    }

    fn ids(rows: &[CreateActionRow], row: usize) -> Vec<String> {
        let row = serde_json::to_value(&rows[row]).unwrap();
        row["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|button| button["custom_id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn main_menu_matches_legacy_javascript_layout() {
        let (embed, rows) = settings_hub(&ctx(false, None));
        let embed: Value = serde_json::to_value(embed).unwrap();
        assert_eq!(embed["title"], "⚙️ Settings Menu");
        let description = embed["description"].as_str().unwrap();
        assert!(description
            .starts_with("Welcome to the bot settings. Select a category to get started:"));
        assert!(description.contains("Player IDs: **7/∞**"));
        assert!(description.contains("🛡️ **Admin** — administrators & branding"));
        assert!(
            description.contains("📜 **Logs** — unified log channel (includes former Admin-Log)")
        );
        assert_eq!(embed["color"], 0x5865F2);

        assert_eq!(rows.len(), 3);
        assert_eq!(
            ids(&rows, 0),
            [
                "settings:playerid",
                "settings:lookup",
                "settings:giftcode",
                "settings:wos",
                "settings:calculator"
            ]
        );
        assert_eq!(
            ids(&rows, 1),
            [
                "settings:ministry",
                "settings:notifications",
                "settings:messages",
                "settings:discordtools",
                "settings:permissions:0"
            ]
        );
        assert_eq!(ids(&rows, 2), ["settings:logs"]);
    }

    #[test]
    fn owner_panel_button_only_in_official_guild() {
        let (_embed, rows) = settings_hub(&ctx(false, None));
        assert_eq!(rows.len(), 3);
        let (_embed, rows) = settings_hub(&ctx(true, Some(100)));
        assert_eq!(rows.len(), 4);
        assert_eq!(ids(&rows, 3), ["settings:owner"]);
        let row: Value = serde_json::to_value(&rows[3]).unwrap();
        assert_eq!(row["components"][0]["style"], 4); // Danger
    }

    #[test]
    fn discord_tools_submenu_matches_legacy_javascript_layout() {
        let (embed, rows) = discord_tools();
        let embed: Value = serde_json::to_value(embed).unwrap();
        assert_eq!(embed["title"], "🛠️ Discord Tools");
        let description = embed["description"].as_str().unwrap();
        assert!(description.contains("🎭 **Roles** · 👋 **Welcome**"));
        assert_eq!(rows.len(), 3);
        assert_eq!(
            ids(&rows, 0),
            [
                "settings:roles",
                "settings:welcome",
                "settings:birthday",
                "settings:index",
                "settings:music"
            ]
        );
        assert_eq!(
            ids(&rows, 1),
            [
                "settings:tickets",
                "settings:giveaways",
                "settings:polls",
                "settings:tempvoice",
                "settings:levels"
            ]
        );
        assert_eq!(ids(&rows, 2), ["settings:back"]);
        let row: Value = serde_json::to_value(&rows[2]).unwrap();
        assert_eq!(row["components"][0]["label"], "Back to Settings");
        assert_eq!(row["components"][0]["emoji"]["name"], "◀️");
    }
}
