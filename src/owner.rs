//! Bot-Owner panel: statistics, per-server inspect, feature toggles, gift
//! priority and DB backup — the Rust equivalent of the legacy Owner Panel.
//!
//! Bot-owner only. In a guild it's opened from `/settings` → **Owner Panel**;
//! in DMs, `/settings` shows it to the owner instead of the Permissions page.

use poise::serenity_prelude as serenity;
use serenity::{
    ComponentInteraction, CreateActionRow, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, CreateModal,
    ModalInteraction,
};

use crate::db;
use crate::state::Data;
use crate::ui::theme::{colors, icons};
use crate::ui::{components, embeds, theme};

/// Feature keys shown in the toggle grid (mirrors the legacy catalog).
pub const FEATURE_KEYS: &[(&str, &str)] = &[
    ("welcome", "Welcome"),
    ("birthday", "Birthdays"),
    ("building_calculator", "Building Calc"),
    ("wos_announcements", "WOS Relay"),
    ("logging", "Logging"),
    ("custom_notifications", "Notifications"),
    ("gift_redemption", "Gift Codes"),
    ("pet_calculator", "Pets Calc"),
    ("svs_calculator", "SVS Prep"),
    ("all_calculators", "All Calculators"),
    ("sneak_peek", "Sneak Peek"),
    ("full_notifications", "Full Notifications"),
    ("giveaways", "Giveaways"),
    ("tickets", "Tickets"),
    ("messages_guides", "Message Library"),
    ("polls", "Voting"),
    ("leveling", "Leveling"),
    ("tempvoice", "Temp Voice"),
    ("music", "Music"),
    ("autorole", "Role Assignment"),
    ("join_dm", "Join DM"),
    ("server_index", "Server Index"),
    ("ministry", "Ministry"),
    ("custom_branding", "Custom Branding"),
    ("installation_help", "Installation Help"),
    ("priority_support", "Priority Support"),
];

// --------------------------------------------------------------- rendering

pub async fn hub_embed(data: &Data) -> CreateEmbed {
    let total_players: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM players")
        .fetch_one(&data.db.pool)
        .await
        .unwrap_or(0);
    let total_servers: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT home_guild_id) FROM players WHERE home_guild_id IS NOT NULL",
    )
    .fetch_one(&data.db.pool)
    .await
    .unwrap_or(0);
    let db_size = std::fs::metadata(&data.cfg.database_path)
        .map(|m| m.len())
        .unwrap_or(0);

    embeds::brand(format!("{} Owner Panel", icons::CROWN))
        .color(colors::GOLD)
        .description(
            "Bot-owner tools: statistics, per-server feature toggles, gift priority and database backup.",
        )
        .field(
            "Instance",
            embeds::rows(&[
                ("Uptime".into(), format!("`{}`", data.uptime())),
                ("Players tracked".into(), format!("`{total_players}`")),
                ("Servers w/ players".into(), format!("`{total_servers}`")),
                (
                    "Database size".into(),
                    format!("`{} MB`", (db_size as f64 / 1_048_576.0 * 10.0).round() / 10.0),
                ),
            ]),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(theme::footer()))
}

pub fn hub_rows() -> Vec<CreateActionRow> {
    vec![
        CreateActionRow::Buttons(vec![
            components::primary("owner:servers", format!("{} Servers", icons::SERVER)),
            components::primary("owner:features", "🧩 Features"),
            components::success("owner:backup", format!("{} Backup DB", icons::SAVE)),
        ]),
        CreateActionRow::Buttons(vec![
            components::secondary("owner:giftprio", "🎁 Gift Priority"),
            components::danger("owner:restart", "🔄 Restart Bot"),
            components::secondary("settings:home", format!("{} Back", icons::BACK)),
        ]),
        CreateActionRow::Buttons(vec![
            components::primary("owner:wos", "📡 Whiteout Stats"),
            components::primary("owner:oracle", "🔮 WoS Oracle"),
        ]),
    ]
}

pub fn servers_page(
    guilds: &[(u64, String, usize)],
    page: usize,
    total_pages: usize,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let lines: Vec<String> = guilds
        .iter()
        .map(|(_, name, members)| format!("**{name}** — `{members}` members"))
        .collect();
    let embed = embeds::brand(format!("{} Servers", icons::SERVER))
        .color(colors::BRAND)
        .description(if lines.is_empty() {
            "_No servers cached._".to_string()
        } else {
            lines.join("\n")
        })
        .footer(serenity::CreateEmbedFooter::new(format!(
            "Page {}/{} · {}",
            page + 1,
            total_pages.max(1),
            theme::footer()
        )));

    let mut rows = Vec::new();
    if total_pages > 1 {
        rows.push(CreateActionRow::Buttons(vec![
            components::secondary(
                format!("owner:servers:{}", page.saturating_sub(1)),
                "◀ Prev",
            ),
            components::secondary(format!("owner:servers:{}", page + 1), "Next ▶"),
        ]));
    }
    rows.push(CreateActionRow::SelectMenu(
        serenity::CreateSelectMenu::new(
            "owner:inspect",
            serenity::CreateSelectMenuKind::String {
                options: guilds
                    .iter()
                    .map(|(id, name, _)| {
                        serenity::CreateSelectMenuOption::new(
                            name.chars().take(100).collect::<String>(),
                            id.to_string(),
                        )
                    })
                    .collect(),
            },
        )
        .placeholder("Inspect a server…")
        .min_values(1)
        .max_values(1),
    ));
    rows.push(CreateActionRow::Buttons(vec![components::secondary(
        "owner:hub",
        format!("{} Back", icons::BACK),
    )]));
    (embed, rows)
}

#[allow(clippy::too_many_arguments)]
pub fn inspect_page(
    guild_id: u64,
    guild_name: String,
    member_count: usize,
    players: i64,
    player_limit: Option<i64>,
    gift_priority: Option<i64>,
    flags: &std::collections::HashMap<String, bool>,
    staff_lines: &str,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let flags_line = if flags.is_empty() {
        "_all features default (enabled)_".to_string()
    } else {
        flags
            .iter()
            .map(|(key, enabled)| {
                let icon = if *enabled { "✅" } else { "🚫" };
                format!("{icon} `{key}`")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let embed = embeds::brand(format!("{} {guild_name}", icons::SERVER))
        .color(colors::BRAND_DEEP)
        .field(
            "Overview",
            embeds::rows(&[
                ("Guild id".into(), format!("`{guild_id}`")),
                ("Members".into(), format!("`{member_count}`")),
                ("Players tracked".into(), format!("`{players}`")),
                (
                    "Player cap".into(),
                    player_limit
                        .map(|v| format!("`{v}`"))
                        .unwrap_or_else(|| "`unlimited`".into()),
                ),
                (
                    "Gift priority".into(),
                    gift_priority
                        .map(|v| format!("`{v}`"))
                        .unwrap_or_else(|| "`unset`".into()),
                ),
            ]),
            false,
        )
        .field("Feature overrides", flags_line, false)
        .field("Server staff", staff_lines, false)
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let rows = vec![
        CreateActionRow::Buttons(vec![
            components::secondary(
                format!("owner:featureguild:{guild_id}"),
                "🧩 Feature Toggles",
            ),
            components::secondary(format!("owner:idcap:{guild_id}"), "👥 Player cap"),
        ]),
        CreateActionRow::Buttons(vec![
            components::secondary(
                format!("owner:giftprioguild:{guild_id}"),
                "🎁 Gift priority",
            ),
            components::secondary("owner:servers:0", format!("{} Back", icons::BACK)),
        ]),
    ];
    (embed, rows)
}

pub fn features_page(
    guild_id: u64,
    guild_name: String,
    overrides: &std::collections::HashMap<String, bool>,
    page: usize,
    total_pages: usize,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let page_size = 4;
    let start = (page * page_size).min(FEATURE_KEYS.len());
    let end = (start + page_size).min(FEATURE_KEYS.len());
    let slice = &FEATURE_KEYS[start..end];

    let lines: Vec<String> = slice
        .iter()
        .map(|(key, label)| {
            let state = match overrides.get(*key) {
                Some(true) => "🟢 forced ON",
                Some(false) => "🔴 forced OFF",
                None => "⚪ default (on)",
            };
            format!("`{key:<20}` {state} — {label}")
        })
        .collect();

    let embed = embeds::brand(format!("🧩 Features — {guild_name}"))
        .color(colors::BRAND)
        .description(format!(
            "Tap a feature to cycle: default → forced OFF → forced ON → default.\n\n{}",
            lines.join("\n")
        ))
        .footer(serenity::CreateEmbedFooter::new(format!(
            "Page {}/{} · {}",
            page + 1,
            total_pages.max(1),
            theme::footer()
        )));

    let mut rows: Vec<CreateActionRow> = slice
        .iter()
        .map(|(key, _label)| {
            CreateActionRow::Buttons(vec![components::secondary(
                format!("owner:featurecycle:{guild_id}:{key}"),
                (*key).to_string(),
            )])
        })
        .collect();
    rows.push(CreateActionRow::Buttons(vec![
        components::secondary(
            format!("owner:features:{}", page.saturating_sub(1)),
            "◀ Prev",
        ),
        components::secondary(format!("owner:features:{}", page + 1), "Next ▶"),
        components::secondary(format!("owner:inspect:{guild_id}"), "Back"),
    ]));
    (embed, rows)
}

// --------------------------------------------------------------- helpers

async fn reply(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    embed: CreateEmbed,
    components: Vec<CreateActionRow>,
    ephemeral: bool,
) -> anyhow::Result<()> {
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(components)
                    .ephemeral(ephemeral),
            ),
        )
        .await?;
    Ok(())
}

async fn update_message(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    embed: CreateEmbed,
    components: Vec<CreateActionRow>,
) -> anyhow::Result<()> {
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(components),
            ),
        )
        .await?;
    Ok(())
}

async fn render_hub(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
) -> anyhow::Result<()> {
    update_message(ctx, interaction, hub_embed(data).await, hub_rows()).await
}

fn guilds_with_counts(ctx: &serenity::Context) -> Vec<(u64, String, usize)> {
    ctx.cache
        .guilds()
        .into_iter()
        .map(|id| {
            let guild = ctx.cache.guild(id);
            let members = guild.as_ref().map(|g| g.members.len()).unwrap_or(0);
            let name = guild
                .map(|g| g.name.clone())
                .unwrap_or_else(|| id.to_string());
            (id.get(), name, members)
        })
        .collect()
}

async fn render_servers(
    ctx: &serenity::Context,
    _data: &Data,
    interaction: &ComponentInteraction,
    page: usize,
) -> anyhow::Result<()> {
    const PER: usize = 10;
    let all = guilds_with_counts(ctx);
    let pages = all.len().div_ceil(PER).max(1);
    let page = page.min(pages - 1);
    let slice: Vec<(u64, String, usize)> = all.into_iter().skip(page * PER).take(PER).collect();
    let (embed, rows) = servers_page(&slice, page, pages);
    update_message(ctx, interaction, embed, rows).await
}

async fn render_inspect(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    guild_id: u64,
) -> anyhow::Result<()> {
    let guild = ctx.cache.guild(serenity::GuildId::new(guild_id));
    let (name, members) = guild
        .map(|g| (g.name.clone(), g.members.len()))
        .unwrap_or_else(|| (format!("Guild {guild_id}"), 0));

    let players = db::subscriptions::count_tracked_players(&data.db, guild_id)
        .await
        .unwrap_or(0);
    let player_limit = db::subscriptions::player_id_limit(&data.db, guild_id)
        .await
        .ok()
        .flatten();
    let gift_priority = db::subscriptions::gift_priority(&data.db, guild_id)
        .await
        .ok()
        .flatten();
    let flags = db::subscriptions::list_feature_flags(&data.db, guild_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| (row.feature_key.clone(), row.enabled != 0))
        .collect();
    let staff = db::subscriptions::list_staff(&data.db, guild_id)
        .await
        .unwrap_or_default();
    let staff_lines = if staff.is_empty() {
        "_none yet — the first /settings user claims Server Bot Admin_".to_string()
    } else {
        staff
            .iter()
            .map(|row| {
                let role = if row.role == "server_admin" {
                    "👑 Server Bot Admin"
                } else {
                    "🛡️ Sub-Admin"
                };
                format!("{role} — <@{}>", row.user_id)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let (embed, rows) = inspect_page(
        guild_id,
        name,
        members,
        players,
        player_limit,
        gift_priority,
        &flags,
        &staff_lines,
    );
    update_message(ctx, interaction, embed, rows).await
}

async fn render_features(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    guild_id: Option<u64>,
    page: usize,
) -> anyhow::Result<()> {
    let Some(guild_id) = guild_id else {
        return render_servers(ctx, data, interaction, 0).await;
    };
    let guild = ctx.cache.guild(serenity::GuildId::new(guild_id));
    let name = guild
        .map(|g| g.name.clone())
        .unwrap_or_else(|| format!("Guild {guild_id}"));
    let overrides = db::subscriptions::list_feature_flags(&data.db, guild_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| (row.feature_key, row.enabled != 0))
        .collect();

    let pages = FEATURE_KEYS.len().div_ceil(4).max(1);
    let page = page.min(pages - 1);
    let (embed, rows) = features_page(guild_id, name, &overrides, page, pages);
    update_message(ctx, interaction, embed, rows).await
}


// ------------------------------------------------- data-source key panels

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    WhiteoutStats,
    Oracle,
}

impl Source {
    fn slug(self) -> &'static str {
        match self {
            Self::WhiteoutStats => "wos",
            Self::Oracle => "oracle",
        }
    }
}

fn mask_key(key: &str) -> String {
    if key.len() > 12 {
        format!("{}…{}", &key[..8], &key[key.len() - 4..])
    } else {
        "••••".to_string()
    }
}

async fn source_creds(data: &Data, source: Source) -> db::wos_credentials::Credentials {
    match source {
        Source::WhiteoutStats => db::wos_credentials::get(&data.db).await.unwrap_or_default(),
        Source::Oracle => db::oracle::key_get(&data.db).await.unwrap_or_default(),
    }
}

/// The "<source> — API Key" panel: status line, source order, Connect/Test/Disconnect.
async fn source_panel(
    data: &Data,
    source: Source,
    notice: Option<&str>,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let creds = source_creds(data, source).await;
    let has_key = creds.api_key.is_some();

    let key_line = match &creds.api_key {
        Some(key) => {
            let test = match creds.last_test_ok {
                None => "not tested".to_string(),
                Some(true) => format!(
                    "ok{}",
                    creds
                        .last_test_message
                        .as_deref()
                        .map(|m| format!(" · {m}"))
                        .unwrap_or_default()
                ),
                Some(false) => format!(
                    "failed{}",
                    creds
                        .last_test_message
                        .as_deref()
                        .map(|m| format!(" · {m}"))
                        .unwrap_or_default()
                ),
            };
            format!("🔑 **API key:** `{}` · {test}", mask_key(key))
        }
        None => match source {
            Source::WhiteoutStats => {
                "🔑 **API key:** none — the public site is used (no key required).".to_string()
            }
            Source::Oracle => {
                "🔑 **API key:** none — the open site is used (no key required).".to_string()
            }
        },
    };

    let (title, body) = match source {
        Source::WhiteoutStats => (
            "📡 Whiteout Stats — API Key",
            [
                "Whiteout Stats is the **first** data source for roster refresh, auto import and Lookup;",
                "WoS Oracle is the second option. The key is **optional** — it only adds higher limits / extra data.",
                "",
                "Paste your key after logging in with Discord on the stats site.",
            ],
        ),
        Source::Oracle => (
            "🔮 WoS Oracle — API Key",
            [
                "WoS Oracle is the **second** data source — Lookup asks it alongside Whiteout Stats and merges the richer gear, arena and progression data.",
                "No Discord login is required. The key is **optional** but unlocks player/kingdom data and higher limits.",
                "",
                "How to get a free token: open wosoracle.com → F12 → Network → any `/api/…` request → copy the `x-api-token` header value and paste it here.",
            ],
        ),
    };

    let mut lines: Vec<String> = Vec::new();
    if let Some(notice) = notice {
        lines.push(format!("{notice}\n"));
    }
    lines.push(key_line);
    lines.push(String::new());
    lines.extend(body.iter().map(|l| l.to_string()));

    let embed = embeds::brand(title)
        .color(colors::TEAL)
        .description(lines.join("\n"))
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));

    let slug = source.slug();
    let connect = if has_key {
        components::secondary(format!("owner:{slug}:connect"), "🔑 Update API Key")
    } else {
        components::success(format!("owner:{slug}:connect"), "🔑 Connect API Key")
    };
    let rows = vec![
        CreateActionRow::Buttons(vec![
            connect,
            components::secondary(format!("owner:{slug}:test"), "🧪 Test Data"),
            components::danger(format!("owner:{slug}:disconnect"), "🔌 Disconnect")
                .disabled(!has_key),
        ]),
        CreateActionRow::Buttons(vec![components::secondary(
            "owner:hub",
            format!("{} Back", icons::BACK),
        )]),
    ];
    (embed, rows)
}

async fn source_test(data: &Data, source: Source) -> (bool, String) {
    let (ok, message) = match source {
        Source::WhiteoutStats => data.lookup.test_connection().await,
        Source::Oracle => data.oracle.test_connection().await,
    };
    let has_key = source_creds(data, source).await.api_key.is_some();
    if has_key {
        let _ = match source {
            Source::WhiteoutStats => db::wos_credentials::record_test(&data.db, ok, &message).await,
            Source::Oracle => db::oracle::key_record_test(&data.db, ok, &message).await,
        };
    }
    (ok, message)
}

async fn source_action(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    source: Source,
    rest: &str,
) -> anyhow::Result<()> {
    let slug = source.slug();
    match rest {
        "" => {
            let (embed, rows) = source_panel(data, source, None).await;
            update_message(ctx, interaction, embed, rows).await
        }
        "connect" => {
            let (label, placeholder) = match source {
                Source::WhiteoutStats => (
                    "Whiteout Stats API key",
                    "wos_…  or  Authorization: Bearer wos_…",
                ),
                Source::Oracle => (
                    "WoS Oracle API key",
                    "x-api-token value from browser DevTools  (or full header line)",
                ),
            };
            let inputs = vec![CreateActionRow::InputText(
                serenity::CreateInputText::new(serenity::InputTextStyle::Paragraph, label, "api_key")
                    .placeholder(placeholder)
                    .required(true)
                    .max_length(500),
            )];
            let title = match source {
                Source::WhiteoutStats => "Whiteout Stats API Key",
                Source::Oracle => "WoS Oracle API Key",
            };
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Modal(
                        CreateModal::new(format!("ownermodal:{slug}key:0"), title).components(inputs),
                    ),
                )
                .await?;
            Ok(())
        }
        "test" => {
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Defer(
                        CreateInteractionResponseMessage::new().ephemeral(true),
                    ),
                )
                .await?;
            let (ok, message) = source_test(data, source).await;
            let embed = if ok {
                embeds::success("Test passed", message)
            } else {
                embeds::warn("Test failed", message)
            };
            interaction
                .edit_response(&ctx.http, serenity::EditInteractionResponse::new().embed(embed))
                .await?;
            Ok(())
        }
        "disconnect" => {
            match source {
                Source::WhiteoutStats => {
                    db::wos_credentials::clear(&data.db).await?;
                    data.lookup.set_api_key(None);
                }
                Source::Oracle => {
                    db::oracle::key_clear(&data.db).await?;
                    data.oracle.set_api_key(None);
                }
            }
            let (embed, rows) = source_panel(data, source, Some("🔌 API key disconnected.")).await;
            update_message(ctx, interaction, embed, rows).await
        }
        other => Err(anyhow::anyhow!("unknown {slug} action `{other}`")),
    }
}

async fn source_key_modal(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ModalInteraction,
    source: Source,
    raw: &str,
) -> anyhow::Result<()> {
    let Some(key) = db::wos_credentials::parse_api_key_paste(raw) else {
        return crate::commands::interactions::finish_modal(
            ctx,
            interaction,
            embeds::warn("API key", "Could not find a valid API key in that paste."),
        )
        .await;
    };
    let user = interaction.user.id.get();
    match source {
        Source::WhiteoutStats => {
            db::wos_credentials::set(&data.db, &key, user).await?;
            data.lookup.set_api_key(Some(key));
        }
        Source::Oracle => {
            db::oracle::key_set(&data.db, &key, user).await?;
            data.oracle.set_api_key(Some(key));
        }
    }
    let (ok, message) = source_test(data, source).await;
    let notice = if ok {
        format!("✅ API key saved & verified. {message}")
    } else {
        format!("⚠️ API key saved, but test failed: {message}")
    };
    let (embed, _) = source_panel(data, source, Some(&notice)).await;
    crate::commands::interactions::finish_modal(ctx, interaction, embed).await
}

// --------------------------------------------------------------- handlers

fn is_owner(data: &Data, interaction: &ComponentInteraction) -> bool {
    data.cfg.owner_id == Some(interaction.user.id.get())
}

pub async fn owner_component(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    action: &str,
) -> anyhow::Result<()> {
    if !is_owner(data, interaction) {
        return reply(
            ctx,
            interaction,
            embeds::denied("Bot Owner"),
            Vec::new(),
            true,
        )
        .await;
    }

    let mut parts = action.split(':');
    let kind = parts.next().unwrap_or("");
    let rest = parts.collect::<Vec<_>>().join(":");

    match kind {
        "hub" => render_hub(ctx, data, interaction).await,
        "wos" => source_action(ctx, data, interaction, Source::WhiteoutStats, &rest).await,
        "oracle" => source_action(ctx, data, interaction, Source::Oracle, &rest).await,
        "perm" => {
            let (embed, rows) = crate::commands::interactions::admin_panel(ctx, data, 0).await?;
            update_message(ctx, interaction, embed, rows).await
        }
        "servers" => {
            let page: usize = rest.parse().unwrap_or(0);
            render_servers(ctx, data, interaction, page).await
        }
        "inspect" => {
            let guild_id = rest.parse::<u64>().unwrap_or(0);
            render_inspect(ctx, data, interaction, guild_id).await
        }
        "features" => {
            let page: usize = rest.parse().unwrap_or(0);
            render_features(ctx, data, interaction, None, page).await
        }
        "featureguild" => {
            let guild_id = rest.parse::<u64>().unwrap_or(0);
            render_features(ctx, data, interaction, Some(guild_id), 0).await
        }
        "featurecycle" => {
            // owner:featurecycle:{guild}:{key}
            let mut inner = rest.splitn(2, ':');
            let guild_id: u64 = inner.next().unwrap_or("0").parse().unwrap_or(0);
            let key = inner.next().unwrap_or("").to_string();
            if guild_id == 0 || key.is_empty() {
                return reply(
                    ctx,
                    interaction,
                    embeds::warn("Toggle", "Malformed toggle id."),
                    Vec::new(),
                    true,
                )
                .await;
            }
            let overrides = db::subscriptions::list_feature_flags(&data.db, guild_id)
                .await
                .unwrap_or_default();
            let current = overrides
                .iter()
                .find(|row| row.feature_key == key)
                .map(|row| row.enabled != 0);
            match current {
                None => {
                    db::subscriptions::set_feature_flag(
                        &data.db,
                        guild_id,
                        &key,
                        false,
                        interaction.user.id.get(),
                    )
                    .await?
                }
                Some(false) => {
                    db::subscriptions::set_feature_flag(
                        &data.db,
                        guild_id,
                        &key,
                        true,
                        interaction.user.id.get(),
                    )
                    .await?
                }
                Some(true) => {
                    db::subscriptions::clear_feature_flag(&data.db, guild_id, &key).await?
                }
            }
            render_features(ctx, data, interaction, Some(guild_id), 0).await
        }
        "idcap" => {
            let locale_guild: u64 = rest.parse().unwrap_or(0);
            let inputs = vec![serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short,
                    "Player cap (blank = unlimited)",
                    "limit",
                )
                .required(false),
            )];
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Modal(
                        CreateModal::new(format!("ownermodal:idcap:{locale_guild}"), "Player cap")
                            .components(inputs),
                    ),
                )
                .await?;
            Ok(())
        }
        "giftprioguild" | "giftprio" => {
            let guild_id: u64 = rest.parse().unwrap_or(0);
            let current = db::subscriptions::gift_priority(&data.db, guild_id)
                .await
                .ok()
                .flatten();
            let inputs = vec![serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short,
                    "Priority (lower = first). Blank = clear",
                    "priority",
                )
                .required(false)
                .value(current.map(|v| v.to_string()).unwrap_or_default()),
            )];
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Modal(
                        CreateModal::new(
                            format!("ownermodal:giftprio:{guild_id}"),
                            "Gift priority",
                        )
                        .components(inputs),
                    ),
                )
                .await?;
            Ok(())
        }
        "backup" => {
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Defer(
                        CreateInteractionResponseMessage::new().ephemeral(true),
                    ),
                )
                .await?;

            let path = data.cfg.database_path.clone();
            let (bytes, filename) = crate::backup::archive_database_snapshot(&path).await?;
            const MAX_UPLOAD: usize = 10 * 1024 * 1024;
            if bytes.len() > MAX_UPLOAD {
                interaction
                    .edit_response(
                        &ctx.http,
                        serenity::EditInteractionResponse::new().content(format!(
                            "Backup is {:.1} MB — too large to upload.",
                            bytes.len() as f64 / 1_048_576.0
                        )),
                    )
                    .await?;
                return Ok(());
            }
            interaction
                .create_followup(
                    &ctx.http,
                    CreateInteractionResponseFollowup::new()
                        .content(format!(
                            "Here's your database backup ({:.1} MB).",
                            bytes.len() as f64 / 1_048_576.0
                        ))
                        .ephemeral(true)
                        .add_file(serenity::CreateAttachment::bytes(bytes, filename)),
                )
                .await?;
            Ok(())
        }
        "restart" => update_message(
            ctx,
            interaction,
            embeds::warn(
                "Restart?",
                "This exits the process. Your host (HidenCloud) should restart it automatically.",
            ),
            vec![CreateActionRow::Buttons(vec![
                components::danger("owner:restartconfirm", "Restart now"),
                components::secondary("owner:hub", "Cancel"),
            ])],
        )
        .await,
        "restartconfirm" => {
            update_message(
                ctx,
                interaction,
                embeds::ok("Restarting — goodbye!"),
                Vec::new(),
            )
            .await?;
            tokio::spawn(async {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                // 42 = "restart me" for the starter.js supervisor (0 looks like "stopped" to panels)
                std::process::exit(42);
            });
            Ok(())
        }
        other => Err(anyhow::anyhow!("unknown owner action `{other}`")),
    }
}

pub async fn owner_modal(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ModalInteraction,
    prefix: &str,
    guild_id: u64,
) -> anyhow::Result<()> {
    if data.cfg.owner_id != Some(interaction.user.id.get()) {
        return crate::commands::interactions::finish_modal(
            ctx,
            interaction,
            embeds::denied("Bot Owner"),
        )
        .await;
    }

    let get = |id: &str| -> Option<String> {
        interaction
            .data
            .components
            .iter()
            .flat_map(|row| row.components.iter())
            .find_map(|component| match component {
                serenity::ActionRowComponent::InputText(input) if input.custom_id == id => {
                    input.value.clone()
                }
                _ => None,
            })
    };

    match prefix {
        "idcap" => {
            let raw = get("limit").unwrap_or_default();
            let limit: Option<i64> = raw.trim().parse::<i64>().ok().filter(|v| *v > 0);
            db::subscriptions::set_player_id_limit(&data.db, guild_id, limit).await?;
            crate::commands::interactions::finish_modal(
                ctx,
                interaction,
                embeds::ok(match limit {
                    Some(limit) => format!("Player cap set to **{limit}** for this server."),
                    None => "Player cap cleared — unlimited.".to_string(),
                }),
            )
            .await
        }
        "giftprio" => {
            let raw = get("priority").unwrap_or_default();
            let priority: Option<i64> = raw.trim().parse::<i64>().ok();
            db::subscriptions::set_gift_priority(&data.db, guild_id, priority).await?;
            crate::commands::interactions::finish_modal(
                ctx,
                interaction,
                embeds::ok(match priority {
                    Some(priority) => format!("Gift priority set to **{priority}**."),
                    None => "Gift priority cleared — default announcement order.".to_string(),
                }),
            )
            .await
        }
        "woskey" => {
            let raw = get("api_key").unwrap_or_default();
            source_key_modal(ctx, data, interaction, Source::WhiteoutStats, &raw).await
        }
        "oraclekey" => {
            let raw = get("api_key").unwrap_or_default();
            source_key_modal(ctx, data, interaction, Source::Oracle, &raw).await
        }
        other => Err(anyhow::anyhow!("unknown owner modal `{other}`")),
    }
}
