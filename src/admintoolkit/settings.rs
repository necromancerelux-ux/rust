//! Admin Toolkit settings hub, opened from Settings → Admin → Admin Toolkit.
//! Everything is handled with `atkset:` custom ids; no slash command is added.

use poise::serenity_prelude as serenity;
use serenity::{
    ButtonStyle, ComponentInteraction, ComponentInteractionDataKind, CreateActionRow, CreateButton,
    CreateEmbed, CreateSelectMenu, CreateSelectMenuKind, ReactionType,
};

use crate::db;
use crate::state::Data;
use crate::ui::theme;

use super::{forum, registry};

const PREFIX: &str = "atkset:";

fn back(to: &str, label: &str) -> CreateButton {
    CreateButton::new(format!("{PREFIX}{to}"))
        .label(label)
        .style(ButtonStyle::Secondary)
        .emoji(ReactionType::Unicode(theme::icons::BACK.to_string()))
}

fn button(id: &str, label: &str, emoji: &str, style: ButtonStyle, disabled: bool) -> CreateButton {
    CreateButton::new(format!("{PREFIX}{id}"))
        .label(label)
        .style(style)
        .disabled(disabled)
        .emoji(ReactionType::Unicode(emoji.to_string()))
}

fn roles_line(ids: &[u64]) -> String {
    if ids.is_empty() {
        "_none_".to_string()
    } else {
        ids.iter()
            .map(|id| format!("<@&{id}>"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn embed(title: &str, description: String) -> CreateEmbed {
    CreateEmbed::new()
        .title(title)
        .description(description)
        .color(0x5865F2)
        .footer(serenity::CreateEmbedFooter::new(theme::footer()))
}

/// Home: status, forum, panel count, R4/R5 roles and audit channel.
pub async fn build_home(data: &Data, guild_id: u64) -> (CreateEmbed, Vec<CreateActionRow>) {
    let config = db::admin_toolkit::ToolkitConfig::for_guild(&data.db, guild_id)
        .await
        .ok()
        .flatten();
    let posts = db::admin_toolkit::get_posts(&data.db, guild_id)
        .await
        .unwrap_or_default();
    let ready = config.as_ref().and_then(|c| c.forum_channel_id).is_some();
    let enabled = config.as_ref().map(|c| c.enabled).unwrap_or(true);
    let status = if !ready {
        "⚪ Not set up yet"
    } else if enabled {
        "🟢 Enabled"
    } else {
        "🔴 Disabled"
    };

    let description = [
        "Manage the R4/R5 Admin Toolkit forum from one place.".to_string(),
        theme::DIVIDER.to_string(),
        format!("**Status:** {status}"),
        format!(
            "**Forum:** {}",
            config
                .as_ref()
                .and_then(|c| c.forum_channel_id)
                .map(|id| format!("<#{id}>"))
                .unwrap_or_else(|| "_not set_".to_string())
        ),
        format!("**Panels:** {}/{}", posts.len(), registry::KEYS.len()),
        format!(
            "**R4 roles:** {}",
            config
                .as_ref()
                .map(|c| roles_line(&c.r4_role_ids))
                .unwrap_or_else(|| "_none_".to_string())
        ),
        format!(
            "**R5 roles:** {}",
            config
                .as_ref()
                .map(|c| roles_line(&c.r5_role_ids))
                .unwrap_or_else(|| "_none_".to_string())
        ),
        format!(
            "**Audit log channel:** {}",
            config
                .as_ref()
                .and_then(|c| c.log_channel_id)
                .map(|id| format!("<#{id}>"))
                .unwrap_or_else(|| "_not set_".to_string())
        ),
        theme::DIVIDER.to_string(),
        if ready {
            String::new()
        } else {
            "👉 Press **Set up** to create the forum and all panels.".to_string()
        },
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .collect::<Vec<_>>()
    .join("\n");

    let rows = vec![
        CreateActionRow::Buttons(vec![
            button(
                "setup",
                if ready { "Repair" } else { "Set up" },
                if ready { "🛠️" } else { "✅" },
                ButtonStyle::Success,
                false,
            ),
            button(
                "refresh",
                "Refresh panels",
                "🔄",
                ButtonStyle::Primary,
                !ready,
            ),
            button("rebuild", "Rebuild", "♻️", ButtonStyle::Danger, !ready),
            button(
                "toggle",
                if enabled { "Disable" } else { "Enable" },
                if enabled { "⏸️" } else { "▶️" },
                ButtonStyle::Secondary,
                !ready,
            ),
        ]),
        CreateActionRow::Buttons(vec![
            button("roles", "R4 / R5 Roles", "🎭", ButtonStyle::Primary, false),
            button("channels", "Channels", "📺", ButtonStyle::Primary, false),
            button("audit", "Audit log", "📜", ButtonStyle::Secondary, false),
            CreateButton::new("settings:permissions:0")
                .label("Back to Admin")
                .style(ButtonStyle::Secondary)
                .emoji(ReactionType::Unicode(theme::icons::BACK.to_string())),
        ]),
    ];
    (embed("🧰 Admin Toolkit Settings", description), rows)
}

fn build_roles(
    config: Option<&db::admin_toolkit::ToolkitConfig>,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let description = [
        "Pick which Discord roles count as **R4** and **R5**. The selection **replaces** the current list.".to_string(),
        String::new(),
        format!("**R4:** {}", config.map(|c| roles_line(&c.r4_role_ids)).unwrap_or_else(|| "_none_".to_string())),
        format!("**R5:** {}", config.map(|c| roles_line(&c.r5_role_ids)).unwrap_or_else(|| "_none_".to_string())),
        String::new(),
        "_R5 can use every Toolkit action; R4 gets the standard ones._".to_string(),
    ]
    .join("\n");
    let r4 = CreateSelectMenu::new(
        format!("{PREFIX}r4"),
        CreateSelectMenuKind::Role {
            default_roles: None,
        },
    )
    .placeholder("Select R4 roles…")
    .min_values(0)
    .max_values(25);
    let r5 = CreateSelectMenu::new(
        format!("{PREFIX}r5"),
        CreateSelectMenuKind::Role {
            default_roles: None,
        },
    )
    .placeholder("Select R5 roles…")
    .min_values(0)
    .max_values(25);
    (
        embed("🎭 R4 / R5 Roles", description),
        vec![
            CreateActionRow::SelectMenu(r4),
            CreateActionRow::SelectMenu(r5),
            CreateActionRow::Buttons(vec![back("home", "Back")]),
        ],
    )
}

fn build_channels(
    config: Option<&db::admin_toolkit::ToolkitConfig>,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let description = [
        format!(
            "**Forum:** {}",
            config
                .and_then(|c| c.forum_channel_id)
                .map(|id| format!("<#{id}>"))
                .unwrap_or_else(|| "_not set_".to_string())
        ),
        format!(
            "**Audit log channel:** {}",
            config
                .and_then(|c| c.log_channel_id)
                .map(|id| format!("<#{id}>"))
                .unwrap_or_else(|| "_not set_".to_string())
        ),
        String::new(),
        "• Choosing a **forum** uses it as the Toolkit forum and creates any missing panels in it."
            .to_string(),
        "• The **audit log** channel should be private; every Toolkit action is posted there."
            .to_string(),
    ]
    .join("\n");
    let forum = CreateSelectMenu::new(
        format!("{PREFIX}forum"),
        CreateSelectMenuKind::Channel {
            channel_types: Some(vec![serenity::ChannelType::Forum]),
            default_channels: None,
        },
    )
    .placeholder("Use an existing forum channel…")
    .min_values(1)
    .max_values(1);
    let log = CreateSelectMenu::new(
        format!("{PREFIX}logch"),
        CreateSelectMenuKind::Channel {
            channel_types: Some(vec![serenity::ChannelType::Text]),
            default_channels: None,
        },
    )
    .placeholder("Set audit log channel…")
    .min_values(1)
    .max_values(1);
    (
        embed("📺 Toolkit Channels", description),
        vec![
            CreateActionRow::SelectMenu(forum),
            CreateActionRow::SelectMenu(log),
            CreateActionRow::Buttons(vec![
                button(
                    "clearlog",
                    "Clear audit log channel",
                    "🧹",
                    ButtonStyle::Secondary,
                    config.and_then(|c| c.log_channel_id).is_none(),
                ),
                back("home", "Back"),
            ]),
        ],
    )
}

async fn build_audit(data: &Data, guild_id: u64) -> (CreateEmbed, Vec<CreateActionRow>) {
    let entries = db::admin_toolkit::list_audit(&data.db, guild_id, 15)
        .await
        .unwrap_or_default();
    let text = if entries.is_empty() {
        "No actions logged yet.".to_string()
    } else {
        entries
            .iter()
            .map(|entry| {
                format!(
                    "`{}` <@{}> · {}{}{}",
                    entry.created_at,
                    entry.user_id,
                    entry.action,
                    entry
                        .tool_key
                        .as_ref()
                        .map(|tool| format!(" · {tool}"))
                        .unwrap_or_default(),
                    entry
                        .detail
                        .as_ref()
                        .filter(|detail| !detail.is_empty())
                        .map(|detail| format!(" — {detail}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    (
        CreateEmbed::new()
            .title("📜 Admin Toolkit Audit Log")
            .description(text.chars().take(4000).collect::<String>())
            .color(0x5865F2)
            .footer(serenity::CreateEmbedFooter::new(theme::footer())),
        vec![CreateActionRow::Buttons(vec![back("home", "Back")])],
    )
}

async fn respond(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    embed: CreateEmbed,
    rows: Vec<CreateActionRow>,
    content: Option<String>,
) -> anyhow::Result<()> {
    let mut builder = serenity::EditInteractionResponse::new()
        .embed(embed)
        .components(rows);
    if let Some(content) = content {
        builder = builder.content(content);
    }
    interaction.defer(&ctx.http).await?;
    interaction.edit_response(&ctx.http, builder).await?;
    Ok(())
}

async fn config(data: &Data, guild_id: u64) -> Option<db::admin_toolkit::ToolkitConfig> {
    db::admin_toolkit::ToolkitConfig::for_guild(&data.db, guild_id)
        .await
        .ok()
        .flatten()
}

/// Handle every `atkset:` interaction.
pub async fn handle(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    action: &str,
) -> anyhow::Result<()> {
    if !super::can_configure(ctx, interaction) {
        interaction
            .create_response(
                &ctx.http,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("🔒 Only the server owner, an Administrator, or a Bot Admin can configure the Admin Toolkit.")
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    }
    let Some(guild_id) = interaction.guild_id.map(|id| id.get()) else {
        anyhow::bail!("Admin Toolkit settings only work inside a server");
    };
    let user_id = interaction.user.id.get();

    match action {
        "home" => {
            let (embed, rows) = build_home(data, guild_id).await;
            respond(ctx, interaction, embed, rows, None).await
        }
        "roles" => {
            let (embed, rows) = build_roles(config(data, guild_id).await.as_ref());
            respond(ctx, interaction, embed, rows, None).await
        }
        "channels" => {
            let (embed, rows) = build_channels(config(data, guild_id).await.as_ref());
            respond(ctx, interaction, embed, rows, None).await
        }
        "audit" => {
            let (embed, rows) = build_audit(data, guild_id).await;
            respond(ctx, interaction, embed, rows, None).await
        }
        "r4" | "r5" => {
            let ids: Vec<u64> = match &interaction.data.kind {
                ComponentInteractionDataKind::RoleSelect { values } => {
                    values.iter().map(|id| id.get()).collect()
                }
                _ => Vec::new(),
            };
            let saved = if action == "r4" {
                db::admin_toolkit::set_r4_roles(&data.db, guild_id, ids.clone()).await?
            } else {
                db::admin_toolkit::set_r5_roles(&data.db, guild_id, ids.clone()).await?
            };
            if let Some(forum) = saved.forum_channel_id {
                super::forum::sync_permissions(ctx, guild_id, forum, &saved).await;
            }
            super::log_audit(
                ctx,
                data,
                guild_id,
                user_id,
                None,
                "roles",
                Some(&format!(
                    "Set {} roles ({})",
                    action.to_uppercase(),
                    ids.len()
                )),
            )
            .await;
            let (embed, rows) = build_roles(Some(&saved));
            respond(ctx, interaction, embed, rows, None).await
        }
        "setup" | "forum" => {
            let preferred = if action == "forum" {
                match &interaction.data.kind {
                    ComponentInteractionDataKind::ChannelSelect { values } => {
                        values.first().map(|id| id.get())
                    }
                    _ => None,
                }
            } else {
                None
            };
            interaction.defer(&ctx.http).await?;
            match forum::repair(ctx, data, guild_id, preferred).await {
                Ok(result) => {
                    let existed = action == "forum"
                        || config(data, guild_id)
                            .await
                            .and_then(|c| c.forum_channel_id)
                            .is_some();
                    super::log_audit(
                        ctx,
                        data,
                        guild_id,
                        user_id,
                        None,
                        if existed { "repair" } else { "setup" },
                        Some(&format!(
                            "{} created, {} failed",
                            result.created.len(),
                            result.failed.len()
                        )),
                    )
                    .await;
                    let (embed, rows) = build_home(data, guild_id).await;
                    let title = if action == "forum" {
                        "✅ Forum updated"
                    } else if existed {
                        "🛠️ Admin Toolkit repaired"
                    } else {
                        "✅ Admin Toolkit ready"
                    };
                    interaction
                        .edit_response(
                            &ctx.http,
                            serenity::EditInteractionResponse::new()
                                .embed(embed)
                                .components(rows)
                                .content(forum::format_result(&result, title)),
                        )
                        .await?;
                    Ok(())
                }
                Err(err) => {
                    interaction
                        .edit_response(
                            &ctx.http,
                            serenity::EditInteractionResponse::new().content(format!(
                                "❌ **Unable to complete that action.**\n\n**Reason:** {}",
                                err.to_string().chars().take(300).collect::<String>()
                            )),
                        )
                        .await?;
                    Ok(())
                }
            }
        }
        "logch" => {
            let channel = match &interaction.data.kind {
                ComponentInteractionDataKind::ChannelSelect { values } => {
                    values.first().map(|id| id.get())
                }
                _ => None,
            };
            if let Some(channel) = channel {
                db::admin_toolkit::set_log_channel(&data.db, guild_id, channel).await?;
                super::log_audit(
                    ctx,
                    data,
                    guild_id,
                    user_id,
                    None,
                    "log-channel",
                    Some(&format!("Audit log → <#{channel}>")),
                )
                .await;
            }
            let config = config(data, guild_id).await;
            let (embed, rows) = build_channels(config.as_ref());
            respond(ctx, interaction, embed, rows, None).await
        }
        "clearlog" => {
            db::admin_toolkit::clear_log_channel(&data.db, guild_id).await?;
            let config = config(data, guild_id).await;
            let (embed, rows) = build_channels(config.as_ref());
            respond(ctx, interaction, embed, rows, None).await
        }
        "refresh" => {
            interaction.defer(&ctx.http).await?;
            let count = forum::refresh_all(ctx, data, guild_id).await;
            super::log_audit(
                ctx,
                data,
                guild_id,
                user_id,
                None,
                "refresh",
                Some(&format!("{count} panels refreshed")),
            )
            .await;
            let (embed, rows) = build_home(data, guild_id).await;
            interaction
                .edit_response(
                    &ctx.http,
                    serenity::EditInteractionResponse::new()
                        .embed(embed)
                        .components(rows)
                        .content(format!("🔄 Refreshed {count} panel(s).")),
                )
                .await?;
            Ok(())
        }
        "toggle" => {
            let current = config(data, guild_id)
                .await
                .map(|c| c.enabled)
                .unwrap_or(true);
            db::admin_toolkit::set_enabled(&data.db, guild_id, !current).await?;
            super::log_audit(
                ctx,
                data,
                guild_id,
                user_id,
                None,
                "toggle",
                Some(if current { "Disabled" } else { "Enabled" }),
            )
            .await;
            let (embed, rows) = build_home(data, guild_id).await;
            respond(ctx, interaction, embed, rows, None).await
        }
        "rebuild" => {
            interaction
                .create_response(
                    &ctx.http,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("⚠️ This deletes the Toolkit's own forum posts and recreates them. Settings are kept; other server content is not touched.")
                            .components(vec![CreateActionRow::Buttons(vec![
                                button("rebuildgo", "Rebuild", "♻️", ButtonStyle::Danger, false),
                                back("home", "Cancel"),
                            ])])
                            .ephemeral(true),
                    ),
                )
                .await?;
            Ok(())
        }
        "rebuildgo" => {
            interaction.defer(&ctx.http).await?;
            let result = forum::rebuild(ctx, data, guild_id).await?;
            super::log_audit(
                ctx,
                data,
                guild_id,
                user_id,
                None,
                "rebuild",
                Some("Rebuilt the Admin Toolkit"),
            )
            .await;
            let (embed, rows) = build_home(data, guild_id).await;
            interaction
                .edit_response(
                    &ctx.http,
                    serenity::EditInteractionResponse::new()
                        .embed(embed)
                        .components(rows)
                        .content(forum::format_result(&result, "✅ Admin Toolkit rebuilt")),
                )
                .await?;
            Ok(())
        }
        other => anyhow::bail!("unknown Admin Toolkit action `{other}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_ids_fit_discord_limits() {
        for id in [
            "home",
            "roles",
            "channels",
            "audit",
            "r4",
            "r5",
            "forum",
            "logch",
            "clearlog",
            "refresh",
            "toggle",
            "rebuild",
            "rebuildgo",
        ] {
            assert!(format!("{PREFIX}{id}").len() <= 100);
        }
    }
}
