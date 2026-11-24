//! Member-facing Tickets feature — port of the legacy JS `tickets/store.js`,
//! `tickets/menu.js`, `tickets/flow.js` and `adminToolkit/panels/tickets.js`.
//!
//! Storage stays in `db/tickets.rs` (the JS-owned `ticket_settings` / `tickets`
//! tables). This module owns the member panel, the private-channel open flow,
//! the in-channel staff/member controls, the close+transcript flow, and the
//! admin hub. All copy, ids, ordering, permissions, and error paths follow the
//! JS source; no translation or cleanup feature exists there, so none is added.

use anyhow::{anyhow, Result};
use poise::serenity_prelude as serenity;
use serenity::{
    ActionRowComponent, ChannelId, ChannelType, ComponentInteraction, ComponentInteractionDataKind,
    CreateActionRow, CreateAllowedMentions, CreateAttachment, CreateButton, CreateChannel,
    CreateEmbed, CreateEmbedFooter, CreateInteractionResponse, CreateInteractionResponseMessage,
    CreateMessage, CreateModal, CreateSelectMenu, CreateSelectMenuKind, EditInteractionResponse,
    GuildId, ModalInteraction, PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId,
    UserId,
};

use crate::db;
use crate::db::tickets::{Ticket, TicketSettings};
use crate::permissions;
use crate::state::Data;
use crate::ui::components;

const MAX_TRANSCRIPT_MESSAGES: usize = 5000;
const DELETE_DELAY_MS: u64 = 5;

fn pad4(number: impl std::fmt::Display) -> String {
    format!("{number:04}")
}

/// JS `utils/ui.truncate`.
fn truncate(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() > max {
        let mut out: String = chars[..max.saturating_sub(1)].iter().collect();
        out.push('…');
        out
    } else {
        text.to_string()
    }
}

fn member_allow() -> Permissions {
    Permissions::VIEW_CHANNEL
        | Permissions::SEND_MESSAGES
        | Permissions::READ_MESSAGE_HISTORY
        | Permissions::ATTACH_FILES
        | Permissions::EMBED_LINKS
        | Permissions::ADD_REACTIONS
}

fn ticket_row(claimed: bool) -> CreateActionRow {
    components::row(vec![
        components::success(
            "ticket:claim",
            if claimed {
                "🙋 Unclaim"
            } else {
                "🙋 Claim"
            }
            .to_string(),
        ),
        components::secondary("ticket:adduser", "➕ Add User"),
        components::secondary("ticket:removeuser", "➖ Remove User"),
        components::danger("ticket:close", "🔒 Close"),
    ])
}

fn back_row() -> CreateActionRow {
    components::row(vec![components::secondary("ticket:hub", "◀️ Back")])
}

/// JS `flow.buildTicketEmbed` / `renderWelcome`. Also used to rebuild the
/// welcome embed after a claim changes the `Claimed by` field.
fn ticket_embed(
    settings: &TicketSettings,
    opener: u64,
    user_name: &str,
    number: i64,
    subject: &str,
    details: Option<&str>,
    claimed_by: Option<u64>,
) -> CreateEmbed {
    let padded = pad4(number);
    let welcome = settings
        .welcome_message()
        .replace("{mention}", &format!("<@{opener}>"))
        .replace("{user}", user_name)
        .replace("{number}", &padded)
        .replace("{subject}", subject);
    let mut embed = CreateEmbed::new()
        .title(format!("🎫 Ticket #{padded}"))
        .description(truncate(&welcome, 4000))
        .field("Opened by", format!("<@{opener}>"), true)
        .field(
            "Claimed by",
            claimed_by
                .map(|id| format!("<@{id}>"))
                .unwrap_or_else(|| "Unclaimed".to_string()),
            true,
        )
        .field("Subject", truncate(subject, 1000), false)
        .color(0x5865F2)
        .footer(CreateEmbedFooter::new(crate::ui::theme::footer()));
    if let Some(details) = details.filter(|value| !value.is_empty()) {
        embed = embed.field("Details", truncate(details, 1000), false);
    }
    embed
}

// ---------------------------------------------------------------- panels

/// JS `flow.buildPanelEmbed` + `buildPanelRow`: the public Open Ticket panel.
pub fn panel_payload(settings: &TicketSettings) -> (CreateEmbed, Vec<CreateActionRow>) {
    let embed = CreateEmbed::new()
        .title(truncate(settings.panel_title(), 256))
        .description(truncate(settings.panel_description(), 4000))
        .color(settings.panel_color())
        .footer(CreateEmbedFooter::new(crate::ui::theme::footer()));
    let rows = vec![components::row(vec![components::primary(
        "ticket:open",
        "🎫 Open Ticket",
    )])];
    (embed, rows)
}

/// JS `tickets/menu.buildHubEmbed` + three button rows.
pub fn hub_payload(
    settings: &TicketSettings,
    open_count: usize,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    fn status(ok: bool, label: &str, value: String) -> String {
        format!("{} **{label}** — {value}", if ok { "✅" } else { "⚠️" })
    }
    let staff = if settings.staff_roles().is_empty() {
        "not set (only admins can see tickets)".to_string()
    } else {
        settings
            .staff_roles()
            .iter()
            .map(|id| format!("<@&{id}>"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let lines = vec![
        "Private support channels opened by members from a panel button.\n".to_string(),
        status(
            settings.panel_channel().is_some(),
            "Panel channel",
            settings
                .panel_channel()
                .map(|id| format!("<#{id}>"))
                .unwrap_or_else(|| "not set".to_string()),
        ),
        status(
            settings.category().is_some(),
            "Ticket category",
            settings
                .category()
                .map(|id| format!("<#{id}>"))
                .unwrap_or_else(|| "not set (channels are created at the top level)".to_string()),
        ),
        status(
            settings.log_channel().is_some(),
            "Log channel",
            settings
                .log_channel()
                .map(|id| format!("<#{id}>"))
                .unwrap_or_else(|| "not set (no transcripts saved except by DM)".to_string()),
        ),
        status(!settings.staff_roles().is_empty(), "Staff roles", staff),
        format!(
            "ℹ️ **Max open per member** — {}",
            settings.max_open_per_user
        ),
        format!(
            "ℹ️ **DM transcript to opener** — {}",
            if settings.dm_transcript() {
                "on"
            } else {
                "off"
            }
        ),
        format!("ℹ️ **Open tickets now** — {open_count}"),
    ];
    let embed = CreateEmbed::new()
        .title("🎫 Tickets")
        .description(lines.join("\n"))
        .color(0x5865F2)
        .footer(CreateEmbedFooter::new(crate::ui::theme::footer()));
    let row1 = components::row(vec![
        styled(
            "ticket:set:panel",
            "📌 Panel Channel",
            settings.panel_channel().is_some(),
            true,
        ),
        styled(
            "ticket:set:category",
            "📁 Category",
            settings.category().is_some(),
            false,
        ),
        styled(
            "ticket:set:log",
            "🧾 Log Channel",
            settings.log_channel().is_some(),
            false,
        ),
        styled(
            "ticket:set:staff",
            "🛡️ Staff Roles",
            !settings.staff_roles().is_empty(),
            true,
        ),
    ]);
    let row2 = components::row(vec![
        components::secondary("ticket:edit", "✏️ Edit Text"),
        components::secondary("ticket:max", "🔢 Max Per Member"),
        components::secondary(
            "ticket:dm",
            format!(
                "📨 DM Transcript: {}",
                if settings.dm_transcript() {
                    "On"
                } else {
                    "Off"
                }
            ),
        ),
        CreateButton::new("ticket:post")
            .label("📣 Post / Update Panel")
            .style(serenity::ButtonStyle::Primary)
            .disabled(settings.panel_channel().is_none()),
    ]);
    let row3 = components::row(vec![
        components::secondary("ticket:list", "📋 Open Tickets"),
        components::secondary("settings:back", "◀️ Back to Settings"),
    ]);
    (embed, vec![row1, row2, row3])
}

fn styled(id: &str, label: &str, success: bool, danger_on_unset: bool) -> CreateButton {
    let style = if success {
        serenity::ButtonStyle::Success
    } else if danger_on_unset {
        serenity::ButtonStyle::Danger
    } else {
        serenity::ButtonStyle::Secondary
    };
    CreateButton::new(id).label(label).style(style)
}

fn select_screen(kind: &str, settings: &TicketSettings) -> (CreateEmbed, Vec<CreateActionRow>) {
    let (title, description, select) = match kind {
        "panel" => (
            "📌 Panel Channel",
            "Where the **Open Ticket** panel is posted.",
            CreateSelectMenu::new(
                "ticket:sel:panel",
                CreateSelectMenuKind::Channel {
                    channel_types: Some(vec![ChannelType::Text, ChannelType::News]),
                    default_channels: settings.panel_channel().map(|id| vec![ChannelId::new(id)]),
                },
            )
            .placeholder("Select a text channel…")
            .min_values(1)
            .max_values(1),
        ),
        "category" => (
            "📁 Ticket Category",
            "New ticket channels are created inside this category. Deselect to clear.",
            CreateSelectMenu::new(
                "ticket:sel:category",
                CreateSelectMenuKind::Channel {
                    channel_types: Some(vec![ChannelType::Category]),
                    default_channels: settings.category().map(|id| vec![ChannelId::new(id)]),
                },
            )
            .placeholder("Select a category…")
            .min_values(0)
            .max_values(1),
        ),
        "log" => (
            "🧾 Log Channel",
            "Closed-ticket transcripts are posted here. Deselect to clear.",
            CreateSelectMenu::new(
                "ticket:sel:log",
                CreateSelectMenuKind::Channel {
                    channel_types: Some(vec![ChannelType::Text, ChannelType::News]),
                    default_channels: settings.log_channel().map(|id| vec![ChannelId::new(id)]),
                },
            )
            .placeholder("Select a text channel…")
            .min_values(0)
            .max_values(1),
        ),
        _ => (
            "🛡️ Staff Roles",
            "Members with these roles can see every ticket, claim, close, and manage users. Deselect all to clear.",
            CreateSelectMenu::new(
                "ticket:sel:staff",
                CreateSelectMenuKind::Role {
                    default_roles: Some(
                        settings
                            .staff_roles()
                            .into_iter()
                            .map(RoleId::new)
                            .collect::<Vec<_>>(),
                    ),
                },
            )
            .placeholder("Select up to 10 roles…")
            .min_values(0)
            .max_values(10),
        ),
    };
    let embed = CreateEmbed::new()
        .title(title)
        .description(description)
        .color(0x5865F2)
        .footer(CreateEmbedFooter::new(crate::ui::theme::footer()));
    (embed, vec![CreateActionRow::SelectMenu(select), back_row()])
}

// ---------------------------------------------------------------- responses

async fn ephemeral(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    text: impl Into<String>,
) -> Result<()> {
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(text)
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

/// Match Rust's Toolkit convention: clicks on shared posts answer privately and
/// schedule a debounced refresh; ordinary panels are edited in place.
async fn respond(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    embed: CreateEmbed,
    rows: Vec<CreateActionRow>,
    content: Option<String>,
) -> Result<()> {
    let post_tool = db::admin_toolkit::find_post(&data.db, interaction.message.id.get())
        .await
        .ok()
        .flatten()
        .map(|(_, tool)| tool);
    if let Some(tool) = post_tool {
        let mut builder = CreateInteractionResponseMessage::new()
            .embed(embed)
            .components(rows)
            .ephemeral(true);
        if let Some(text) = content {
            builder = builder.content(text);
        }
        interaction
            .create_response(&ctx.http, CreateInteractionResponse::Message(builder))
            .await?;
        if let Ok(response) = interaction.get_response(&ctx.http).await {
            crate::admintoolkit::track_ephemeral(response.id.get());
        }
        crate::admintoolkit::schedule_post_refresh(
            ctx.clone(),
            data.clone(),
            interaction.guild_id.map(|g| g.get()).unwrap_or(0),
            tool,
        );
        return Ok(());
    }
    let mut builder = CreateInteractionResponseMessage::new()
        .embed(embed)
        .components(rows);
    if let Some(text) = content {
        builder = builder.content(text);
    }
    interaction
        .create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(builder))
        .await?;
    Ok(())
}

pub async fn show_hub(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    guild_id: u64,
) -> Result<()> {
    let settings = db::tickets::settings(&data.db, guild_id).await?;
    let open_count = db::tickets::open_for_guild(&data.db, guild_id).await?.len();
    let (embed, rows) = hub_payload(&settings, open_count);
    respond(ctx, data, interaction, embed, rows, None).await
}

async fn show_hub_notice(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    guild_id: u64,
    notice: String,
) -> Result<()> {
    let settings = db::tickets::settings(&data.db, guild_id).await?;
    let open_count = db::tickets::open_for_guild(&data.db, guild_id).await?.len();
    let (embed, rows) = hub_payload(&settings, open_count);
    respond(ctx, data, interaction, embed, rows, Some(notice)).await
}

/// JS `flow.postPanel`: edit the stored panel message when possible, otherwise
/// post a fresh one and remember its id. Returns `(message_id, edited)`.
pub async fn post_panel(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
) -> Result<(u64, bool)> {
    let settings = db::tickets::settings(&data.db, guild_id).await?;
    let channel_id = settings
        .panel_channel()
        .ok_or_else(|| anyhow!("No panel channel is set."))?;
    let channel = match ctx.http.get_channel(ChannelId::new(channel_id)).await {
        Ok(serenity::Channel::Guild(channel)) => channel,
        _ => {
            return Err(anyhow!(
                "The panel channel no longer exists or is not a text channel."
            ))
        }
    };
    let (embed, rows) = panel_payload(&settings);
    if let Some(message_id) = settings
        .panel_message_id
        .as_deref()
        .and_then(|id| id.parse::<u64>().ok())
    {
        if channel
            .id
            .edit_message(
                &ctx.http,
                message_id,
                serenity::EditMessage::new()
                    .embed(embed.clone())
                    .components(rows.clone()),
            )
            .await
            .is_ok()
        {
            return Ok((message_id, true));
        }
    }
    let message = channel
        .send_message(
            &ctx.http,
            CreateMessage::new().embed(embed).components(rows),
        )
        .await?;
    let id = message.id.get();
    db::tickets::set_panel_message(&data.db, guild_id, channel_id, id).await?;
    Ok((id, false))
}

// ---------------------------------------------------------------- permissions

async fn is_staff(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    settings: &TicketSettings,
) -> bool {
    if permissions::component_is_admin(ctx, data, interaction).await {
        return true;
    }
    let roles = interaction
        .member
        .as_ref()
        .map(|member| member.roles.clone())
        .unwrap_or_default();
    settings
        .staff_roles()
        .iter()
        .any(|id| roles.contains(&RoleId::new(*id)))
}

fn can_close(
    interaction: &ComponentInteraction,
    ticket: &Ticket,
    settings: &TicketSettings,
) -> bool {
    interaction.user.id.get() == ticket.opener_id().unwrap_or(0)
        || settings.staff_roles().iter().any(|id| {
            interaction
                .member
                .as_ref()
                .map(|m| m.roles.contains(&RoleId::new(*id)))
                .unwrap_or(false)
        })
}

/// JS `flow.ticketContext`.
async fn ticket_context(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
) -> Result<Option<(Ticket, TicketSettings)>> {
    let guild_id = match interaction.guild_id.map(|id| id.get()) {
        Some(id) => id,
        None => {
            ephemeral(ctx, interaction, "⚠️ This isn't an open ticket channel.").await?;
            return Ok(None);
        }
    };
    let ticket = db::tickets::get_by_channel(&data.db, interaction.channel_id.get()).await?;
    let ticket = match ticket {
        Some(ticket) if ticket.is_open() => ticket,
        _ => {
            ephemeral(ctx, interaction, "⚠️ This isn't an open ticket channel.").await?;
            return Ok(None);
        }
    };
    let settings = db::tickets::settings(&data.db, guild_id).await?;
    Ok(Some((ticket, settings)))
}

// ---------------------------------------------------------------- member flow

async fn live_open_tickets(
    data: &Data,
    ctx: &serenity::Context,
    guild_id: u64,
    user_id: u64,
) -> Result<Vec<Ticket>> {
    let rows = db::tickets::open_by_user(&data.db, guild_id, user_id).await?;
    let mut alive = Vec::new();
    for ticket in rows {
        let channel_id = match ticket.channel_id() {
            Some(id) => id,
            None => continue,
        };
        match ctx.http.get_channel(ChannelId::new(channel_id)).await {
            Ok(_) => alive.push(ticket),
            Err(_) => {
                let _ = db::tickets::mark_closed(&data.db, channel_id, None).await;
            }
        }
    }
    Ok(alive)
}

fn limit_message(open: &[Ticket], max: i64) -> String {
    let links = open
        .iter()
        .filter_map(|t| t.channel_id().map(|id| format!("<#{id}>")))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "⛔ You already have {} open ticket{} (limit {}): {}",
        open.len(),
        if open.len() == 1 { "" } else { "s" },
        max,
        links
    )
}

async fn handle_open_button(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
) -> Result<()> {
    let guild_id = match interaction.guild_id.map(|id| id.get()) {
        Some(id) => id,
        None => return Ok(()),
    };
    let settings = db::tickets::settings(&data.db, guild_id).await?;
    let open = live_open_tickets(data, ctx, guild_id, interaction.user.id.get()).await?;
    if open.len() as i64 >= settings.max_open_per_user {
        return ephemeral(
            ctx,
            interaction,
            limit_message(&open, settings.max_open_per_user),
        )
        .await;
    }
    let inputs = vec![
        CreateActionRow::InputText(
            components::short("subject", "Subject")
                .placeholder("Short summary of what you need")
                .max_length(100),
        ),
        CreateActionRow::InputText(
            components::paragraph("details", "Details (optional)")
                .placeholder("Anything that helps us help you faster")
                .required(false)
                .max_length(1000),
        ),
    ];
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Modal(
                CreateModal::new("ticketmodal:open", "Open a Ticket").components(inputs),
            ),
        )
        .await?;
    Ok(())
}

async fn modal_text(interaction: &ModalInteraction, id: &str) -> Option<String> {
    interaction
        .data
        .components
        .iter()
        .flat_map(|row| row.components.iter())
        .find_map(|component| match component {
            ActionRowComponent::InputText(input) if input.custom_id == id => {
                input.value.as_deref().map(str::to_string)
            }
            _ => None,
        })
        .map(|value| value.trim().to_string())
}

async fn handle_open_modal(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ModalInteraction,
) -> Result<()> {
    let guild_id = match interaction.guild_id.map(|id| id.get()) {
        Some(id) => id,
        None => return Ok(()),
    };
    // JS defers ephemerally before the re-check.
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Defer(
                CreateInteractionResponseMessage::new().ephemeral(true),
            ),
        )
        .await?;

    let settings = db::tickets::settings(&data.db, guild_id).await?;
    let open = live_open_tickets(data, ctx, guild_id, interaction.user.id.get()).await?;
    if open.len() as i64 >= settings.max_open_per_user {
        interaction
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new()
                    .content(limit_message(&open, settings.max_open_per_user)),
            )
            .await?;
        return Ok(());
    }

    let subject = modal_text(interaction, "subject")
        .await
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "No subject".to_string());
    let details = modal_text(interaction, "details").await.unwrap_or_default();

    // Category may have been deleted since configuration.
    let parent = match settings.category() {
        Some(id) => match ctx.http.get_channel(ChannelId::new(id)).await {
            Ok(serenity::Channel::Guild(channel)) if channel.kind == ChannelType::Category => {
                Some(channel.id)
            }
            _ => None,
        },
        None => None,
    };

    // Drop staff roles that no longer exist in the guild.
    let staff_roles: Vec<u64> = settings
        .staff_roles()
        .into_iter()
        .filter(|id| {
            ctx.cache
                .guild(GuildId::new(guild_id))
                .map(|guild| guild.roles.contains_key(&RoleId::new(*id)))
                .unwrap_or(false)
        })
        .collect();

    let number = db::tickets::reserve_number(&data.db, guild_id).await?;
    let padded = pad4(number);
    let opener = interaction.user.id;
    let everyone = RoleId::new(guild_id);

    let mut overwrites = vec![
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Role(everyone),
        },
        PermissionOverwrite {
            allow: member_allow(),
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(opener),
        },
        PermissionOverwrite {
            allow: member_allow() | Permissions::MANAGE_MESSAGES | Permissions::MANAGE_CHANNELS,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(ctx.cache.current_user().id),
        },
    ];
    for role in &staff_roles {
        overwrites.push(PermissionOverwrite {
            allow: member_allow() | Permissions::MANAGE_MESSAGES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Role(RoleId::new(*role)),
        });
    }

    let audit_reason = format!("Ticket #{number} opened by {}", interaction.user.tag());
    let mut builder = CreateChannel::new(format!("ticket-{padded}"))
        .kind(ChannelType::Text)
        .permissions(overwrites)
        .topic(truncate(
            &format!(
                "Ticket #{} • {} • {}",
                number, interaction.user.name, subject
            ),
            1000,
        ))
        .audit_log_reason(&audit_reason);
    if let Some(parent) = parent {
        builder = builder.category(parent);
    }
    let channel = match GuildId::new(guild_id)
        .create_channel(&ctx.http, builder)
        .await
    {
        Ok(channel) => channel,
        Err(err) => {
            tracing::warn!(error = %err, "tickets: channel creation failed");
            interaction
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new().content(
                        "⚠️ I couldn't create your ticket channel. Please let a server admin know — the ticket category may be full (50 channels) or I may be missing **Manage Channels**.",
                    ),
                )
                .await?;
            return Ok(());
        }
    };

    db::tickets::insert(
        &data.db,
        guild_id,
        number,
        channel.id.get(),
        opener.get(),
        Some(&subject),
        Some(&details),
    )
    .await?;

    // JS `renderWelcome` uses `member.displayName || member.user.username`.
    let display_name = interaction
        .member
        .as_ref()
        .and_then(|member| member.nick.clone())
        .filter(|nick| !nick.is_empty())
        .unwrap_or_else(|| interaction.user.name.clone());
    let embed = ticket_embed(
        &settings,
        opener.get(),
        &display_name,
        number as i64,
        &subject,
        Some(&details),
        None,
    );

    let pings = std::iter::once(format!("<@{opener}>"))
        .chain(staff_roles.iter().map(|id| format!("<@&{id}>")))
        .collect::<Vec<_>>()
        .join(" ");
    let allowed = CreateAllowedMentions::new()
        .users([opener])
        .roles(staff_roles.into_iter().map(RoleId::new).collect::<Vec<_>>());
    if let Err(err) = channel
        .id
        .send_message(
            &ctx.http,
            CreateMessage::new()
                .content(pings)
                .embed(embed)
                .components(vec![ticket_row(false)])
                .allowed_mentions(allowed),
        )
        .await
    {
        tracing::warn!(error = %err, "tickets: welcome message failed");
    }

    interaction
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().content(format!(
                "✅ Your ticket has been created: <#{}>",
                channel.id.get()
            )),
        )
        .await?;
    Ok(())
}

// ---------------------------------------------------------------- in-channel actions

async fn handle_claim(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
) -> Result<()> {
    let Some((ticket, settings)) = ticket_context(ctx, data, interaction).await? else {
        return Ok(());
    };
    if !is_staff(ctx, data, interaction, &settings).await {
        return ephemeral(ctx, interaction, "⛔ Only staff can claim tickets.").await;
    }
    let current = ticket.claimed_by();
    let user = interaction.user.id.get();
    let next = match current {
        None => Some(user),
        Some(id) if id == user => None,
        Some(_) if permissions::component_is_admin(ctx, data, interaction).await => Some(user),
        Some(other) => {
            return ephemeral(
                ctx,
                interaction,
                format!("⚠️ This ticket is already claimed by <@{other}>."),
            )
            .await
        }
    };
    let channel_id = match ticket.channel_id() {
        Some(id) => id,
        None => return Ok(()),
    };
    db::tickets::set_claimed(&data.db, channel_id, next).await?;

    // JS edits the existing embed's `Claimed by` field in place, preserving the
    // description rendered at creation with the opener's display name. Rust
    // rebuilds the embed, so resolve the current guild display name; fall back
    // to the global username, then to an empty string if the member left.
    let opener = ticket.opener_id().unwrap_or(0);
    let guild_id = interaction.guild_id.map(|id| id.get()).unwrap_or(0);
    let user_name = match ctx
        .http
        .get_member(GuildId::new(guild_id), UserId::new(opener))
        .await
    {
        Ok(member) if !member.display_name().is_empty() => member.display_name().to_string(),
        _ => ctx
            .http
            .get_user(UserId::new(opener))
            .await
            .map(|user| user.name)
            .unwrap_or_default(),
    };
    let embed = ticket_embed(
        &settings,
        opener,
        &user_name,
        ticket.number,
        ticket.subject.as_deref().unwrap_or("—"),
        ticket.details.as_deref(),
        next,
    );
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(vec![ticket_row(next.is_some())]),
            ),
        )
        .await?;
    let content = match next {
        Some(id) => format!("🙋 <@{id}> claimed this ticket."),
        None => format!("↩️ <@{}> released this ticket.", user),
    };
    let _ = ChannelId::new(channel_id)
        .send_message(
            &ctx.http,
            CreateMessage::new()
                .content(content)
                .allowed_mentions(CreateAllowedMentions::new()),
        )
        .await;
    Ok(())
}

async fn show_user_select(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    kind: &str,
) -> Result<()> {
    let Some((_ticket, settings)) = ticket_context(ctx, data, interaction).await? else {
        return Ok(());
    };
    if !is_staff(ctx, data, interaction, &settings).await {
        return ephemeral(
            ctx,
            interaction,
            "⛔ You don't have permission to use this.",
        )
        .await;
    }
    let (id, placeholder, content) = if kind == "add" {
        (
            "ticket:usersel:add",
            "Choose who to add…",
            "➕ Who should be added to this ticket?",
        )
    } else {
        (
            "ticket:usersel:remove",
            "Choose who to remove…",
            "➖ Who should be removed from this ticket?",
        )
    };
    let select = CreateSelectMenu::new(
        id,
        CreateSelectMenuKind::User {
            default_users: None,
        },
    )
    .placeholder(placeholder)
    .min_values(1)
    .max_values(5);
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .components(vec![CreateActionRow::SelectMenu(select)])
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

async fn handle_user_select(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    kind: &str,
) -> Result<()> {
    let Some((ticket, settings)) = ticket_context(ctx, data, interaction).await? else {
        return Ok(());
    };
    if !is_staff(ctx, data, interaction, &settings).await {
        return ephemeral(
            ctx,
            interaction,
            "⛔ You don't have permission to use this.",
        )
        .await;
    }
    let users = match &interaction.data.kind {
        ComponentInteractionDataKind::UserSelect { values } => values.clone(),
        _ => Vec::new(),
    };
    let channel_id = match ticket.channel_id() {
        Some(id) => id,
        None => return Ok(()),
    };
    let channel = match ctx.http.get_channel(ChannelId::new(channel_id)).await {
        Ok(serenity::Channel::Guild(channel)) => channel,
        _ => return Ok(()),
    };
    let mut done = Vec::new();
    let mut skipped = Vec::new();
    for user in users {
        let id = user.get();
        if kind == "add" {
            match channel
                .create_permission(
                    &ctx.http,
                    PermissionOverwrite {
                        allow: member_allow(),
                        deny: Permissions::empty(),
                        kind: PermissionOverwriteType::Member(user),
                    },
                )
                .await
            {
                Ok(()) => done.push(id),
                Err(_) => skipped.push(id),
            }
        } else if Some(id) == ticket.opener_id() || id == ctx.cache.current_user().id.get() {
            skipped.push(id);
        } else {
            match channel
                .delete_permission(&ctx.http, PermissionOverwriteType::Member(user))
                .await
            {
                Ok(()) => done.push(id),
                Err(_) => skipped.push(id),
            }
        }
    }
    let mentions = |ids: &[u64]| {
        ids.iter()
            .map(|id| format!("<@{id}>"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut lines = Vec::new();
    if !done.is_empty() {
        lines.push(format!(
            "✅ {}: {}",
            if kind == "add" { "Added" } else { "Removed" },
            mentions(&done)
        ));
    }
    if !skipped.is_empty() {
        lines.push(format!(
            "⚠️ Skipped (opener, bot, or error): {}",
            mentions(&skipped)
        ));
    }
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(if lines.is_empty() {
                        "Nothing changed.".to_string()
                    } else {
                        lines.join("\n")
                    })
                    .components(Vec::new()),
            ),
        )
        .await?;
    if !done.is_empty() {
        let content = format!(
            "{} <@{}> {} {}.",
            if kind == "add" { "➕" } else { "➖" },
            interaction.user.id,
            if kind == "add" { "added" } else { "removed" },
            mentions(&done)
        );
        let allowed = if kind == "add" {
            CreateAllowedMentions::new()
                .users(done.into_iter().map(UserId::new).collect::<Vec<_>>())
        } else {
            CreateAllowedMentions::new()
        };
        let _ = channel
            .id
            .send_message(
                &ctx.http,
                CreateMessage::new()
                    .content(content)
                    .allowed_mentions(allowed),
            )
            .await;
    }
    Ok(())
}

async fn handle_close_button(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
) -> Result<()> {
    let Some((ticket, settings)) = ticket_context(ctx, data, interaction).await? else {
        return Ok(());
    };
    let is_opener = interaction.user.id.get() == ticket.opener_id().unwrap_or(0);
    if !is_staff(ctx, data, interaction, &settings).await && !is_opener {
        return ephemeral(
            ctx,
            interaction,
            "⛔ Only staff or the ticket's opener can close it.",
        )
        .await;
    }
    let rows = vec![components::row(vec![
        components::danger("ticket:closeconfirm", "🔒 Close Ticket"),
        components::secondary("ticket:closecancel", "Cancel"),
    ])];
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(
                        "Close this ticket? A transcript will be saved and the channel deleted.",
                    )
                    .components(rows)
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

async fn handle_close_confirm(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
) -> Result<()> {
    let Some((ticket, settings)) = ticket_context(ctx, data, interaction).await? else {
        return Ok(());
    };
    let is_opener = interaction.user.id.get() == ticket.opener_id().unwrap_or(0);
    if !is_staff(ctx, data, interaction, &settings).await && !is_opener {
        return ephemeral(
            ctx,
            interaction,
            "⛔ You don't have permission to use this.",
        )
        .await;
    }
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content("🔒 Closing…")
                    .components(Vec::new()),
            ),
        )
        .await?;
    let channel_id = match ticket.channel_id() {
        Some(id) => id,
        None => return Ok(()),
    };
    close_ticket(ctx, data, channel_id, &ticket, interaction.user.id.get()).await
}

fn format_message(message: &serenity::Message) -> String {
    let stamp = message.id.created_at().format("%Y-%m-%d %H:%M:%S");
    let who = format!(
        "{}{}",
        message.author.name,
        if message.author.bot { " [BOT]" } else { "" }
    );
    let body = message.content.replace('\n', "\n    ");
    let mut lines = vec![format!("[{stamp} UTC] {who}: {body}")
        .trim_end()
        .to_string()];
    for embed in &message.embeds {
        let text = [embed.title.as_deref(), embed.description.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" — ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            lines.push(format!("    [embed] {}", truncate(&text, 500)));
        }
    }
    for attachment in &message.attachments {
        lines.push(format!(
            "    [attachment] {}: {}",
            attachment.filename, attachment.url
        ));
    }
    lines.join("\n")
}

/// JS `flow.buildTranscript`: fetch up to 5000 messages in 100-message pages.
async fn build_transcript(
    ctx: &serenity::Context,
    channel_id: u64,
    ticket: &Ticket,
) -> Result<(String, usize)> {
    let channel = ChannelId::new(channel_id);
    let mut collected: Vec<serenity::Message> = Vec::new();
    let mut before: Option<serenity::MessageId> = None;
    while collected.len() < MAX_TRANSCRIPT_MESSAGES {
        let mut builder = serenity::GetMessages::new().limit(100u8);
        if let Some(id) = before {
            builder = builder.before(id);
        }
        let batch = channel.messages(&ctx.http, builder).await?;
        if batch.is_empty() {
            break;
        }
        before = batch.last().map(|message| message.id);
        let batch_len = batch.len();
        collected.extend(
            batch
                .into_iter()
                .take(MAX_TRANSCRIPT_MESSAGES - collected.len()),
        );
        if batch_len < 100 {
            break;
        }
    }
    collected.sort_by_key(|message| message.id.created_at());
    let header = vec![
        format!("Transcript — Ticket #{}", pad4(ticket.number)),
        format!("Subject: {}", ticket.subject.as_deref().unwrap_or("—")),
        format!("Opened by: {}", ticket.opener_id),
        format!(
            "Messages: {}{}",
            collected.len(),
            if collected.len() >= MAX_TRANSCRIPT_MESSAGES {
                " (truncated to the most recent)"
            } else {
                ""
            }
        ),
        format!(
            "Generated: {} UTC",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
        ),
        "=".repeat(60),
        String::new(),
    ];
    let text = header
        .into_iter()
        .chain(collected.iter().map(format_message))
        .collect::<Vec<_>>()
        .join("\n");
    Ok((text, collected.len()))
}

async fn close_ticket(
    ctx: &serenity::Context,
    data: &Data,
    channel_id: u64,
    ticket: &Ticket,
    closed_by: u64,
) -> Result<()> {
    // Flip first so a double-click cannot run twice.
    if !db::tickets::mark_closed(&data.db, channel_id, Some(closed_by)).await? {
        return Ok(());
    }
    let settings = db::tickets::settings(&data.db, ticket.guild_id.parse().unwrap_or(0)).await?;
    let padded = pad4(ticket.number);
    let _ = ChannelId::new(channel_id)
        .send_message(
            &ctx.http,
            CreateMessage::new().content(format!(
                "🔒 Ticket closed by <@{closed_by}>. Saving transcript — this channel will be deleted shortly."
            )),
        )
        .await;

    let (text, count) = match build_transcript(ctx, channel_id, ticket).await {
        Ok(transcript) => transcript,
        Err(err) => {
            tracing::warn!(error = %err, "tickets: transcript failed");
            let _ = db::tickets::reopen(&data.db, channel_id).await;
            let _ = ChannelId::new(channel_id)
                .send_message(
                    &ctx.http,
                    CreateMessage::new().content(
                        "⚠️ I couldn't build the transcript, so the ticket stays open. Please try closing it again.",
                    ),
                )
                .await;
            return Ok(());
        }
    };

    let file = || {
        CreateAttachment::bytes(
            text.as_bytes().to_vec(),
            format!("ticket-{padded}-transcript.txt"),
        )
    };

    if let Some(log_id) = settings.log_channel() {
        let embed = CreateEmbed::new()
            .title(format!("🎫 Ticket #{padded} closed"))
            .field("Opened by", format!("<@{}>", ticket.opener_id), true)
            .field("Closed by", format!("<@{closed_by}>"), true)
            .field(
                "Claimed by",
                ticket
                    .claimed_by()
                    .map(|id| format!("<@{id}>"))
                    .unwrap_or_else(|| "Unclaimed".to_string()),
                true,
            )
            .field(
                "Subject",
                truncate(ticket.subject.as_deref().unwrap_or("—"), 1000),
                false,
            )
            .field("Messages", count.to_string(), true)
            .color(0xED4245)
            .footer(CreateEmbedFooter::new(crate::ui::theme::footer()));
        let sent = ChannelId::new(log_id)
            .send_message(
                &ctx.http,
                CreateMessage::new()
                    .embed(embed)
                    .add_file(file())
                    .allowed_mentions(CreateAllowedMentions::new()),
            )
            .await;
        if sent.is_err() {
            let _ = db::tickets::reopen(&data.db, channel_id).await;
            let _ = ChannelId::new(channel_id)
                .send_message(
                    &ctx.http,
                    CreateMessage::new().content(
                        "⚠️ I couldn't post the transcript to the log channel, so the ticket stays open. Check my access to that channel and try again.",
                    ),
                )
                .await;
            return Ok(());
        }
    }

    if settings.dm_transcript() {
        if let Some(opener) = ticket.opener_id() {
            if let Ok(dm) = UserId::new(opener).create_dm_channel(&ctx.http).await {
                let _ = dm
                    .send_message(
                        &ctx.http,
                        CreateMessage::new()
                            .content(format!(
                                "Here is the transcript of your ticket **#{padded}**."
                            ))
                            .add_file(file()),
                    )
                    .await;
            }
        }
    }

    // JS waits 5s so members can see the closing notice before deletion.
    tokio::time::sleep(std::time::Duration::from_secs(DELETE_DELAY_MS)).await;
    if let Err(err) = ctx
        .http
        .delete_channel(
            ChannelId::new(channel_id),
            Some(&format!("Ticket #{padded} closed")),
        )
        .await
    {
        tracing::warn!(error = %err, "tickets: channel delete failed");
    }
    Ok(())
}

// ---------------------------------------------------------------- admin hub

async fn handle_set(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    kind: &str,
    guild_id: u64,
) -> Result<()> {
    if !permissions::component_is_admin(ctx, data, interaction).await {
        return ephemeral(
            ctx,
            interaction,
            "⛔ You don't have permission to use this.",
        )
        .await;
    }
    let settings = db::tickets::settings(&data.db, guild_id).await?;
    let (embed, rows) = select_screen(kind, &settings);
    respond(ctx, data, interaction, embed, rows, None).await
}

async fn handle_select(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    kind: &str,
    guild_id: u64,
) -> Result<()> {
    if !permissions::component_is_admin(ctx, data, interaction).await {
        return ephemeral(
            ctx,
            interaction,
            "⛔ You don't have permission to use this.",
        )
        .await;
    }
    let values = match &interaction.data.kind {
        ComponentInteractionDataKind::ChannelSelect { values } => {
            values.iter().map(|id| id.get()).collect::<Vec<_>>()
        }
        ComponentInteractionDataKind::RoleSelect { values } => {
            values.iter().map(|id| id.get()).collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };
    let next = values.first().copied();
    match kind {
        "staff" => {
            db::tickets::set_staff_roles(&data.db, guild_id, &values).await?;
            return show_hub(ctx, data, interaction, guild_id).await;
        }
        "panel" | "category" | "log" => {
            match kind {
                "panel" => {
                    db::tickets::set_panel_channel(&data.db, guild_id, next.unwrap_or(0)).await?
                }
                "category" => db::tickets::set_category(&data.db, guild_id, next).await?,
                _ => db::tickets::set_log_channel(&data.db, guild_id, next).await?,
            }
            if kind == "panel" {
                if let Some(channel) = next {
                    let previous = db::tickets::settings(&data.db, guild_id)
                        .await?
                        .panel_channel();
                    if previous != Some(channel) {
                        db::tickets::clear_panel_message(&data.db, guild_id).await?;
                    }
                    match post_panel(ctx, data, guild_id).await {
                        Ok((message_id, _)) => {
                            return show_hub_notice(
                                ctx,
                                data,
                                interaction,
                                guild_id,
                                format!("✅ Panel posted: <#{channel}> (message {message_id})"),
                            )
                            .await;
                        }
                        Err(err) => {
                            return show_hub_notice(
                                ctx,
                                data,
                                interaction,
                                guild_id,
                                format!(
                                    "⚠️ Channel saved, but I couldn't post the panel: {}\nCheck the bot has **View Channel, Send Messages, Embed Links** there.",
                                    crate::error::public(&err)
                                ),
                            )
                            .await;
                        }
                    }
                }
            }
            return show_hub(ctx, data, interaction, guild_id).await;
        }
        _ => {}
    }
    Ok(())
}

async fn show_open_list(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    guild_id: u64,
) -> Result<()> {
    if !permissions::component_is_admin(ctx, data, interaction).await {
        return ephemeral(
            ctx,
            interaction,
            "⛔ You don't have permission to use this.",
        )
        .await;
    }
    let open = db::tickets::open_for_guild(&data.db, guild_id).await?;
    let mut lines = open
        .iter()
        .take(20)
        .map(|ticket| {
            let claimed = ticket
                .claimed_by()
                .map(|id| format!(" • claimed by <@{id}>"))
                .unwrap_or_default();
            format!(
                "<#{}> — <@{}> — {}{}",
                ticket.channel_id,
                ticket.opener_id,
                truncate(ticket.subject.as_deref().unwrap_or("No subject"), 60),
                claimed
            )
        })
        .collect::<Vec<_>>();
    if open.len() > 20 {
        lines.push(format!("…and {} more", open.len() - 20));
    }
    let embed = CreateEmbed::new()
        .title("📋 Open Tickets")
        .description(if lines.is_empty() {
            "No open tickets right now.".to_string()
        } else {
            lines.join("\n")
        })
        .color(0x5865F2)
        .footer(CreateEmbedFooter::new(crate::ui::theme::footer()));
    respond(ctx, data, interaction, embed, vec![back_row()], None).await
}

/// Router for every `ticket:` component (public controls first, then admin hub).
pub async fn handle_component(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    custom_id: &str,
) -> Result<()> {
    let guild_id = match interaction.guild_id.map(|id| id.get()) {
        Some(id) => id,
        None => return Ok(()),
    };
    let action = custom_id.strip_prefix("ticket:").unwrap_or("");

    // Public / in-channel actions — permission checks live inside each.
    match action {
        "open" => return handle_open_button(ctx, data, interaction).await,
        "claim" => return handle_claim(ctx, data, interaction).await,
        "adduser" => return show_user_select(ctx, data, interaction, "add").await,
        "removeuser" => return show_user_select(ctx, data, interaction, "remove").await,
        "usersel:add" => return handle_user_select(ctx, data, interaction, "add").await,
        "usersel:remove" => return handle_user_select(ctx, data, interaction, "remove").await,
        "close" => return handle_close_button(ctx, data, interaction).await,
        "closeconfirm" => return handle_close_confirm(ctx, data, interaction).await,
        "closecancel" => {
            return interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .content("Cancelled — the ticket stays open.")
                            .components(Vec::new()),
                    ),
                )
                .await
                .map_err(Into::into)
        }
        _ => {}
    }

    // Everything below is the admin hub (bot-admin only, like JS).
    if !permissions::component_is_admin(ctx, data, interaction).await {
        return ephemeral(
            ctx,
            interaction,
            "⛔ You don't have permission to use this.",
        )
        .await;
    }
    match action {
        "hub" => show_hub(ctx, data, interaction, guild_id).await,
        "set:panel" | "set:category" | "set:log" | "set:staff" => {
            handle_set(
                ctx,
                data,
                interaction,
                action.strip_prefix("set:").unwrap_or(""),
                guild_id,
            )
            .await
        }
        "sel:panel" | "sel:category" | "sel:log" | "sel:staff" => {
            handle_select(
                ctx,
                data,
                interaction,
                action.strip_prefix("sel:").unwrap_or(""),
                guild_id,
            )
            .await
        }
        "edit" => {
            let settings = db::tickets::settings(&data.db, guild_id).await?;
            let inputs = vec![
                CreateActionRow::InputText(
                    components::short("title", "Panel title")
                        .value(settings.panel_title())
                        .max_length(200),
                ),
                CreateActionRow::InputText(
                    components::paragraph("description", "Panel description")
                        .value(settings.panel_description())
                        .max_length(2000),
                ),
                CreateActionRow::InputText(
                    components::short("color", "Panel color (hex, e.g. 5865f2)")
                        .value(format!("{:06x}", settings.panel_color()))
                        .max_length(6),
                ),
                CreateActionRow::InputText(
                    components::paragraph("welcome", "Message inside new tickets")
                        .value(settings.welcome_message())
                        .max_length(1000),
                ),
            ];
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Modal(
                        CreateModal::new("ticketmodal:edit", "Edit Ticket Text").components(inputs),
                    ),
                )
                .await?;
            Ok(())
        }
        "max" => {
            let settings = db::tickets::settings(&data.db, guild_id).await?;
            let inputs = vec![CreateActionRow::InputText(
                components::short("value", "Number between 1 and 10")
                    .value(settings.max_open_per_user.to_string())
                    .max_length(2),
            )];
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Modal(
                        CreateModal::new("ticketmodal:max", "Max Open Tickets Per Member")
                            .components(inputs),
                    ),
                )
                .await?;
            Ok(())
        }
        "dm" => {
            let settings = db::tickets::settings(&data.db, guild_id).await?;
            db::tickets::set_dm_transcript(&data.db, guild_id, !settings.dm_transcript()).await?;
            show_hub(ctx, data, interaction, guild_id).await
        }
        "post" => match post_panel(ctx, data, guild_id).await {
            Ok((message_id, edited)) => {
                show_hub_notice(
                    ctx,
                    data,
                    interaction,
                    guild_id,
                    format!(
                        "✅ Panel {}: message {message_id}",
                        if edited { "updated" } else { "posted" }
                    ),
                )
                .await
            }
            Err(err) => {
                show_hub_notice(
                    ctx,
                    data,
                    interaction,
                    guild_id,
                    format!("⚠️ Couldn't post the panel: {}", crate::error::public(&err)),
                )
                .await
            }
        },
        "list" => show_open_list(ctx, data, interaction, guild_id).await,
        _ => Ok(()),
    }
}

/// Router for every `ticketmodal:` submit. `ticketmodal:open` is public; the
/// edit/max modals are bot-admin only, matching JS `tickets/menu.js`.
pub async fn handle_modal(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ModalInteraction,
    custom_id: &str,
) -> Result<()> {
    let id = custom_id.strip_prefix("ticketmodal:").unwrap_or("");
    if id == "open" {
        return handle_open_modal(ctx, data, interaction).await;
    }
    let guild_id = interaction
        .guild_id
        .map(|id| id.get())
        .ok_or_else(|| anyhow!("tickets only work inside a server"))?;
    if !permissions::modal_is_admin(ctx, data, interaction).await {
        interaction
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("⛔ You don't have permission to use this.")
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    }
    match id {
        "edit" => {
            let title = modal_text(interaction, "title").await.unwrap_or_default();
            let description = modal_text(interaction, "description")
                .await
                .unwrap_or_default();
            let color_raw = modal_text(interaction, "color")
                .await
                .unwrap_or_default()
                .trim_start_matches('#')
                .to_string();
            let welcome = modal_text(interaction, "welcome").await.unwrap_or_default();
            if color_raw.len() != 6 || !color_raw.chars().all(|c| c.is_ascii_hexdigit()) {
                interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("⚠️ Color must be a 6-digit hex value like `5865f2`. Nothing was saved.")
                                .ephemeral(true),
                        ),
                    )
                    .await?;
                return Ok(());
            }
            let color =
                i64::from_str_radix(&color_raw, 16).unwrap_or(crate::db::tickets::DEFAULT_COLOR);
            db::tickets::set_panel_title(&data.db, guild_id, &title).await?;
            db::tickets::set_panel_description(&data.db, guild_id, &description).await?;
            db::tickets::set_panel_color(&data.db, guild_id, color).await?;
            db::tickets::set_welcome_message(&data.db, guild_id, &welcome).await?;
            crate::admintoolkit::maybe_schedule_post_refresh(
                ctx,
                data,
                guild_id,
                interaction.user.id.get(),
                interaction.message.as_ref().map(|message| message.id.get()),
            )
            .await;
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(
                                "✅ Saved. Use **Post / Update Panel** to refresh the live panel.",
                            )
                            .ephemeral(true),
                    ),
                )
                .await?;
            Ok(())
        }
        "max" => {
            let raw = modal_text(interaction, "value").await.unwrap_or_default();
            let value = raw.parse::<i64>().ok();
            if !matches!(value, Some(1..=10)) {
                interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("⚠️ Enter a whole number from 1 to 10.")
                                .ephemeral(true),
                        ),
                    )
                    .await?;
                return Ok(());
            }
            db::tickets::set_max_open(&data.db, guild_id, value.unwrap_or(1) as u64).await?;
            crate::admintoolkit::maybe_schedule_post_refresh(
                ctx,
                data,
                guild_id,
                interaction.user.id.get(),
                interaction.message.as_ref().map(|message| message.id.get()),
            )
            .await;
            interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("✅ Max open tickets per member saved.")
                            .ephemeral(true),
                    ),
                )
                .await?;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// JS `tickets/events.js` — mark a ticket closed when its channel is deleted.
pub async fn on_channel_delete(db: &db::Db, channel_id: u64) {
    if let Err(err) = db::tickets::mark_closed(db, channel_id, None).await {
        tracing::warn!(error = %err, "tickets: channelDelete cleanup failed");
    }
}
