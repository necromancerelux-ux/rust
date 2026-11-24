//! `/settings` — the only slash command. Opens the button-panel hub.
//!
//! Inside a server it opens the server settings hub (admin-only editing;
//! members get a read-only view). In a DM with the bot it opens the bot
//! owner's control panel (the same Permissions manager).

use poise::serenity_prelude as serenity;
use poise::serenity_prelude::CreateActionRow;
use poise::serenity_prelude::CreateEmbedFooter;

use super::{Context, Error};
use crate::db;
use crate::permissions;
use crate::ui::panels;
use crate::ui::theme;

/// Open the settings menu.
#[poise::command(slash_command, category = "Settings")]
pub async fn settings(ctx: Context<'_>) -> Result<(), Error> {
    // Acknowledge immediately; permissions/cache lookups and settings reads can
    // otherwise leave Discord's initial interaction response spinning.
    ctx.defer_ephemeral().await?;
    let data = ctx.data();

    // ── Global first-run bootstrap (Bot Owner) ────────────────────────────
    // If no global admins exist yet, the first person to run /settings becomes
    // the Bot Owner (the DM path shares the same DB table).
    if db::admin::count(&data.db).await? == 0 {
        let _became_owner =
            db::admin::add(&data.db, ctx.author().id.get(), true, ctx.author().id.get()).await?;
        let first_time_embed = serenity::CreateEmbed::new()
            .title("🆕 First Time Setup")
            .description(format!(
                "This command has been used for the first time and no administrators were found.\n\n**{}** has been added as the **Bot Owner**.\n\nYou can now access all administrative functions.",
                ctx.author().name
            ))
            .color(0x5865F2);
        ctx.send(
            poise::CreateReply::default()
                .embed(first_time_embed)
                .ephemeral(true),
        )
        .await?;
    }

    let Some(guild_id) = ctx.guild_id().map(|id| id.get()) else {
        return dm_control_center(ctx).await;
    };

    // Ported check: the bot needs Administrator in the guild to function at all.
    let serenity_guild_id = serenity::GuildId::new(guild_id);
    let bot_id = ctx.framework().bot_id();
    let cached_bot_member = ctx
        .cache()
        .guild(serenity_guild_id)
        .and_then(|guild| guild.members.get(&bot_id).cloned());
    let bot_member = match cached_bot_member {
        Some(member) => Some(member),
        None => ctx.http().get_member(serenity_guild_id, bot_id).await.ok(),
    };
    let bot_has_admin = ctx
        .cache()
        .guild(serenity_guild_id)
        .zip(bot_member.as_ref())
        .is_some_and(|(guild, member)| {
            guild
                .member_permissions(member)
                .contains(serenity::Permissions::ADMINISTRATOR)
        });
    if !bot_has_admin {
        return ctx
            .send(
                poise::CreateReply::default()
                    .content(format!(
                        "Beep boop {} I need **Administrator** permissions to function. Go to Server Settings → Roles → find my role → scroll down and turn on Administrator.",
                        theme::icons::ROBOT
                    ))
                    .ephemeral(true),
            )
            .await
            .map(|_| ())
            .map_err(anyhow::Error::from);
    }

    // ── Per-server claim ──────────────────────────────────────────────────
    // If this guild has no Server Bot Admin yet, the first authorised person
    // (global staff / owner; otherwise the first user who got here) becomes
    // Server Bot Admin.
    let staff = db::subscriptions::list_staff(&data.db, guild_id)
        .await
        .unwrap_or_default();
    let has_server_admin = staff.iter().any(|entry| entry.role == "server_admin");
    let global_admin = permissions::is_admin(&ctx).await;
    let mut claimed = false;
    if !has_server_admin && (global_admin || staff.is_empty()) {
        claimed = db::subscriptions::claim_server_admin(&data.db, guild_id, ctx.author().id.get())
            .await?;
    }

    // ── Access check: Global Admin/Owner OR Server Staff of this guild. ──
    let allowed = global_admin
        || db::subscriptions::is_server_staff(&data.db, guild_id, ctx.author().id.get()).await;
    if !allowed {
        return ctx
            .send(
                poise::CreateReply::default()
                    .content("You do not have permission to access this menu on this server.")
                    .ephemeral(true),
            )
            .await
            .map(|_| ())
            .map_err(anyhow::Error::from);
    }

    // Optional notice when someone just became Server Bot Admin.
    if claimed {
        let notice = serenity::CreateEmbed::new()
            .title("👑 Server Bot Admin")
            .description(format!(
                "**{}** has been set as the **Server Bot Admin** for this server.\n\nYou can configure the bot, manage player IDs, and add Sub-Admins.\nSub-Admins cannot access the Permissions menu.",
                ctx.author().name
            ))
            .color(0x5865F2);
        ctx.send(poise::CreateReply::default().embed(notice).ephemeral(true))
            .await?;
    }

    let guild_name = ctx
        .guild()
        .map(|guild| guild.name.clone())
        .unwrap_or_else(|| "this server".to_string());

    let (player_stats, music, lookup, oracle_state, player_limit, official_guild) = tokio::join!(
        db::players::stats(&data.db, guild_id),
        db::music::get(&data.db, guild_id),
        db::lookup::get(&data.db, guild_id),
        db::oracle::state(&data.db),
        db::subscriptions::player_id_limit(&data.db, guild_id),
        db::subscriptions::is_official_guild(&data.db, guild_id),
    );
    let player_stats = player_stats?;
    let music = music?;
    let lookup = lookup?;
    let oracle_state = oracle_state.unwrap_or_default();
    let oracle_ready = data.oracle.is_configured().await;

    let view = panels::SettingsContext {
        guild_name,
        player_ids: player_stats.tracked,
        player_limit: player_limit.ok().flatten(),
        music_configured: music
            .as_ref()
            .is_some_and(|row| row.channel_id.is_some() && row.panel_message_id.is_some()),
        lookup_configured: lookup.channel_id.is_some(),
        gift_sync: oracle_state.last_codes_sync,
        oracle_ready,
        uptime: data.uptime(),
        latency_ms: ctx.ping().await.as_millis() as u64,
        is_owner: permissions::is_owner(&ctx),
        official_guild,
    };

    let (mut embed, mut rows) = panels::settings_hub(&view);
    if !global_admin {
        // Read-only view for regular members: strip the interactive rows.
        rows.clear();
        embed = embed.footer(CreateEmbedFooter::new(format!(
            "{} • view only (Manage Server required to edit)",
            theme::footer()
        )));
    }

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(rows)
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// The DM control center: the bot owner's permissions panel, button-driven.
async fn dm_control_center(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let user_id = ctx.author().id.get();

    let is_owner =
        data.cfg.owner_id == Some(user_id) || db::admin::is_admin(&data.db, user_id).await;

    if !is_owner {
        return ctx
            .send(
                poise::CreateReply::default()
                    .content("Run `/settings` inside your Discord server. (This DM menu is for the bot owner.)")
                    .ephemeral(true),
            )
            .await
            .map(|_| ())
            .map_err(anyhow::Error::from);
    }

    let is_bot_owner = data.cfg.owner_id == Some(ctx.author().id.get());
    if is_bot_owner {
        // Bot owner DM: owner hub (statistics, servers, features, gift priority,
        // backup). A button jumps into the global Permissions (DM-backed admins) panel.
        let embed = crate::owner::hub_embed(data).await;
        let mut rows = crate::owner::hub_rows();
        rows.push(CreateActionRow::Buttons(vec![
            crate::ui::components::secondary(
                "owner:perm",
                format!("{} Permissions", crate::ui::theme::icons::ADMIN),
            ),
        ]));
        return ctx
            .send(
                poise::CreateReply::default()
                    .embed(embed)
                    .components(rows)
                    .ephemeral(true),
            )
            .await
            .map(|_| ())
            .map_err(anyhow::Error::from);
    }

    // Same Permissions manager as the guild hub's Admin page; guild_id = 0
    // simply yields name-less entries (there is no guild cache outside a server).
    let (embed, rows) =
        crate::commands::interactions::admin_panel(ctx.serenity_context(), data, 0).await?;

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(rows)
            .ephemeral(true),
    )
    .await?;
    Ok(())
}
