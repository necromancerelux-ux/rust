//! Welcome message, auto-role and join-DM plumbing.

use poise::serenity_prelude as serenity;
use serenity::UserId;

use crate::db;
use crate::state::Data;

/// Welcome a new member: send the configured embed and assign auto roles.
pub async fn on_member_join(ctx: &serenity::Context, data: &Data, member: &serenity::Member) {
    if member.user.bot {
        return;
    }

    let guild_id = member.guild_id.get();

    welcome_message(ctx, data, member).await;
    apply_auto_roles(ctx, data, member).await;
    join_dm(ctx, data, member).await;

    let _ = guild_id;
}

/// Forget the member (join-DM reminder tracking cleanup).
pub async fn on_member_remove(data: &Data, guild_id: u64, user_id: u64) {
    let _ = db::welcome::remove_join(&data.db, guild_id, user_id).await;
}

// ---------------------------------------------------------------- welcome

pub async fn welcome_message(ctx: &serenity::Context, data: &Data, member: &serenity::Member) {
    let guild_id = member.guild_id.get();
    let Ok(settings) = db::welcome::welcome(&data.db, guild_id).await else {
        return;
    };
    let Some(channel_id) = settings.channel() else {
        return;
    };

    let server = ctx
        .cache
        .guild(guild_id)
        .map(|guild| guild.name.clone())
        .unwrap_or_else(|| format!("guild {guild_id}"));
    let member_count = ctx
        .cache
        .guild(member.guild_id)
        .map(|guild| guild.members.len() as u64)
        .unwrap_or(0);
    let rendered = settings.render(
        &format!("<@{}>", member.user.id.get()),
        member.display_name(),
        &member.user.name,
        &server,
        member_count,
    );

    let mut embed = serenity::CreateEmbed::new()
        .title(&rendered.title)
        .description(&rendered.description)
        .color(rendered.color as u32)
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));

    match rendered.avatar_mode.as_str() {
        "member" => {
            embed = embed.thumbnail(member.user.face());
        }
        "server" => {}
        _ => {}
    }

    if let Ok(message) = serenity::ChannelId::new(channel_id)
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .content(format!("<@{}>", member.user.id.get()))
                .add_embed(embed)
                .allowed_mentions(serenity::CreateAllowedMentions::new().users([member.user.id])),
        )
        .await
    {
        if let Err(err) =
            db::welcome::record_cleanup_message(&data.db, guild_id, channel_id, message.id.get())
                .await
        {
            tracing::debug!(error = %err, guild = guild_id, "could not track welcome message for cleanup");
        }
    }
}

// ---------------------------------------------------------------- autorole

pub async fn apply_auto_roles(ctx: &serenity::Context, data: &Data, member: &serenity::Member) {
    // Members still in Membership Screening can't receive roles meaningfully.
    if member.pending {
        return;
    }

    let guild_id = member.guild_id.get();
    let Ok(settings) = db::welcome::autorole(&data.db, guild_id).await else {
        return;
    };
    if !settings.enabled() {
        return;
    }

    let wanted = if member.user.bot {
        settings.bot_roles()
    } else {
        settings.member_roles()
    };
    if wanted.is_empty() {
        return;
    }

    // Read everything from the cache in one borrow, then drop it before the await.
    let assignable: Vec<serenity::RoleId> = {
        let Some(guild) = ctx.cache.guild(member.guild_id) else {
            return;
        };

        let bot_id = ctx.cache.current_user().id;
        let bot_top = guild
            .members
            .get(&bot_id)
            .map(|bot| {
                bot.roles
                    .iter()
                    .filter_map(|role_id| guild.roles.get(role_id))
                    .map(|role| role.position)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        wanted
            .into_iter()
            .filter_map(|role_id| {
                guild
                    .roles
                    .get(&serenity::RoleId::new(role_id))
                    .filter(|role| !role.managed && role.position < bot_top)
                    .map(|role| role.id)
            })
            .filter(|role_id| !member.roles.contains(role_id))
            .collect()
    };

    if assignable.is_empty() {
        return;
    }

    let _ = member.add_roles(&ctx.http, &assignable).await;
}

// ---------------------------------------------------------------- join DM

pub async fn join_dm(ctx: &serenity::Context, data: &Data, member: &serenity::Member) {
    let guild_id = member.guild_id.get();
    if member.user.bot {
        return;
    }

    let _ = db::welcome::track_join(&data.db, guild_id, member.user.id.get()).await;

    let Ok(settings) = db::welcome::join_dm(&data.db, guild_id).await else {
        return;
    };
    if !settings.join_dm_enabled() {
        return;
    }

    let server = ctx
        .cache
        .guild(guild_id)
        .map(|guild| guild.name.clone())
        .unwrap_or_else(|| "this server".to_string());

    // Match the legacy join package: send a compact server directory first,
    // then a separate registration panel so the call to action remains clear.
    let index_settings = db::server_index::get(&data.db, guild_id).await.ok();
    let include_voice = index_settings
        .as_ref()
        .is_some_and(|settings| settings.include_voice());
    let include_threads = index_settings
        .as_ref()
        .is_some_and(|settings| settings.include_threads());
    let mut index_embeds =
        crate::serverindex::build_embeds(ctx, guild_id, include_voice, include_threads, &server)
            .await;
    index_embeds.truncate(2);
    if index_embeds.is_empty() {
        index_embeds.push(
            serenity::CreateEmbed::new()
                .color(crate::ui::theme::colors::BRAND)
                .title(format!("🚀 Welcome to {server}"))
                .description(
                    "The server directory is not set up yet. Ask an admin to post the Server Index.",
                )
                .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer())),
        );
    }
    match member
        .user
        .dm(
            &ctx.http,
            serenity::CreateMessage::new().embeds(index_embeds),
        )
        .await
    {
        Ok(message) => {
            attach_join_dm_translate(
                ctx,
                data,
                guild_id,
                member.user.id.get(),
                &message,
                "join_dm_index",
                Vec::new(),
            )
            .await
        }
        Err(error) => {
            tracing::debug!(error = %error, guild = guild_id, user = member.user.id.get(), "could not send join-DM server index")
        }
    }

    let registered = db::players::stats(&data.db, guild_id)
        .await
        .map(|stats| stats.tracked)
        .unwrap_or(0);
    let member_count = ctx
        .cache
        .guild(serenity::GuildId::new(guild_id))
        .map(|guild| guild.members.len() as i64)
        .unwrap_or(0);

    let embed = serenity::CreateEmbed::new()
        .color(crate::ui::theme::colors::BRAND)
        .title("🪪 Register your Player ID")
        .description(format!(
            "Welcome to **{server}**!\n\n\
             Gift codes are **only auto-redeemed** for members who register their in-game Player ID.\n\
             You can register up to **4** Player IDs on your Discord account.\n\n\
             Tap **Register my Play ID** below, or open the panel in the server."
        ))
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));

    // JS `buildIdPanel`: the second embed carries the panel instructions and
    // registration statistics from the same guild.
    let panel_embed = serenity::CreateEmbed::new()
        .color(0x2ECC_71)
        .title("🪪 Player ID Registration")
        .description(
            "**— What to do —**\n\
             🔵 **Register my Play ID** — enter your in-game ID (digits only)\n\
             🎁 **Use Code** — redeem a gift code for all your registered IDs\n\
             🟢 **My Gift Codes** — view your private redemption history\n\
             🔴 **Remove my ID** — unregister a player ID you added\n\
             🏷️ **In-Game Name** — set your server nickname to `[Alliance]Name-Rank`\n\
             🌐 **Translate** — private menu in your language (only you see it)",
        )
        .field(
            "Registration",
            format!("📊 {registered} / {member_count} members registered"),
            false,
        )
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));

    let mut buttons = vec![
        serenity::CreateButton::new(format!("wreg:{guild_id}"))
            .label("🔵 Register my Play ID")
            .style(serenity::ButtonStyle::Primary),
        serenity::CreateButton::new(format!("wreg:use:{guild_id}"))
            .label("🎁 Use Code")
            .style(serenity::ButtonStyle::Primary),
        serenity::CreateButton::new(format!("wreg:history:{guild_id}"))
            .label("🟢 My Gift Codes")
            .style(serenity::ButtonStyle::Success),
        serenity::CreateButton::new(format!("wreg:remove:{guild_id}"))
            .label("🔴 Remove my ID")
            .style(serenity::ButtonStyle::Danger),
        serenity::CreateButton::new(format!("wreg:nickname:{guild_id}"))
            .label("🏷️ In-Game Name")
            .style(serenity::ButtonStyle::Secondary),
        serenity::CreateButton::new(format!("wreg:manage:{guild_id}"))
            .label("Manage my IDs")
            .style(serenity::ButtonStyle::Secondary),
    ];
    let panel_channel =
        db::settings::get(&data.db, guild_id, db::settings::keys::PLAYER_PANEL_CHANNEL)
            .await
            .ok()
            .flatten()
            .and_then(|value| value.parse::<u64>().ok());
    let panel_message =
        db::settings::get(&data.db, guild_id, db::settings::keys::PLAYER_PANEL_MESSAGE)
            .await
            .ok()
            .flatten()
            .and_then(|value| value.parse::<u64>().ok());
    if let (Some(channel), Some(message)) = (panel_channel, panel_message) {
        buttons.push(
            serenity::CreateButton::new_link(format!(
                "https://discord.com/channels/{guild_id}/{channel}/{message}"
            ))
            .label("Open panel in server"),
        );
    }

    let button_rows = buttons
        .chunks(5)
        .map(|chunk| serenity::CreateActionRow::Buttons(chunk.to_vec()))
        .collect::<Vec<_>>();
    let sent = member
        .user
        .dm(
            &ctx.http,
            serenity::CreateMessage::new()
                .embed(embed)
                .embed(panel_embed)
                .components(button_rows.clone()),
        )
        .await;
    if let Ok(message) = sent {
        // JS `buildIdPanel` includes a private Translate control; attach it to
        // the delivered DM through the existing shared translation flow.
        attach_join_dm_translate(
            ctx,
            data,
            guild_id,
            member.user.id.get(),
            &message,
            "join_dm_registration",
            button_rows,
        )
        .await;
        let _ = db::welcome::mark_join_dm_sent(&data.db, guild_id, member.user.id.get()).await;
    }
}

/// Send a registration reminder DM (called by the background loop).
pub async fn send_reminder(ctx: &serenity::Context, data: &Data, guild_id: u64, user_id: u64) {
    if db::players::count_by_user(&data.db, user_id)
        .await
        .map(|count| count > 0)
        .unwrap_or(false)
    {
        let _ = db::welcome::remove_join(&data.db, guild_id, user_id).await;
        return;
    }
    if serenity::GuildId::new(guild_id)
        .member(&ctx.http, serenity::UserId::new(user_id))
        .await
        .is_err()
    {
        let _ = db::welcome::remove_join(&data.db, guild_id, user_id).await;
        return;
    }
    let server = ctx
        .cache
        .guild(guild_id)
        .map(|guild| guild.name.clone())
        .unwrap_or_else(|| "this server".to_string());

    let settings = db::welcome::join_dm(&data.db, guild_id).await.ok();
    let days = settings
        .as_ref()
        .map(|settings| settings.interval_days())
        .unwrap_or(7);
    let embed = serenity::CreateEmbed::new()
        .color(0xFAA6_1A)
        .title("⏰ Reminder: Register your Player ID")
        .description(format!(
            "Register your Player ID in **{server}** to get gift codes auto-redeemed.\n\
             This reminder is sent every **{days} days** until you register (or leave the server)."
        ))
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));

    let mut buttons = vec![serenity::CreateButton::new(format!("wreg:{guild_id}"))
        .label("Register my Play ID")
        .style(serenity::ButtonStyle::Primary)];

    let panel_channel =
        db::settings::get(&data.db, guild_id, db::settings::keys::PLAYER_PANEL_CHANNEL)
            .await
            .ok()
            .flatten()
            .and_then(|value| value.parse::<u64>().ok());
    let panel_message =
        db::settings::get(&data.db, guild_id, db::settings::keys::PLAYER_PANEL_MESSAGE)
            .await
            .ok()
            .flatten()
            .and_then(|value| value.parse::<u64>().ok());
    if let (Some(channel), Some(message)) = (panel_channel, panel_message) {
        buttons.push(
            serenity::CreateButton::new_link(format!(
                "https://discord.com/channels/{guild_id}/{channel}/{message}"
            ))
            .label("Open panel in server"),
        );
    }
    let rows = vec![serenity::CreateActionRow::Buttons(buttons)];

    let user = match ctx.http.get_user(UserId::new(user_id)).await {
        Ok(user) => user,
        Err(err) => {
            tracing::debug!(error = %err, guild = guild_id, user = user_id, "reminder target gone");
            return;
        }
    };

    match user
        .dm(
            &ctx.http,
            serenity::CreateMessage::new()
                .embed(embed)
                .components(rows.clone()),
        )
        .await
    {
        Ok(message) => {
            attach_join_dm_translate(
                ctx,
                data,
                guild_id,
                user_id,
                &message,
                "register_reminder",
                rows,
            )
            .await;
            let _ = db::welcome::mark_reminder_sent(&data.db, guild_id, user_id).await;
        }
        Err(error) => {
            tracing::debug!(error = %error, guild = guild_id, user = user_id, "registration reminder DM failed")
        }
    }
}

async fn attach_join_dm_translate(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    user_id: u64,
    message: &serenity::Message,
    kind: &str,
    mut rows: Vec<serenity::CreateActionRow>,
) {
    let source = message
        .embeds
        .iter()
        .map(|embed| {
            let mut parts = Vec::new();
            if let Some(title) = &embed.title {
                parts.push(title.clone());
            }
            if let Some(description) = &embed.description {
                parts.push(description.clone());
            }
            for field in &embed.fields {
                parts.push(format!("{}: {}", field.name, field.value));
            }
            parts.join("\n")
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if source.is_empty() {
        return;
    }
    let record = crate::db::translate::NewTranslatable {
        guild_id: Some(guild_id),
        channel_id: Some(message.channel_id.get()),
        kind,
        embed_json: None,
        content: Some(source),
        owner_id: Some(user_id),
    };
    if let Err(error) = crate::db::translate::register(&data.db, message.id.get(), record).await {
        tracing::debug!(error = %error, guild = guild_id, "could not register Join DM translation source");
        return;
    }
    rows.push(serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(format!("translate:{}", message.id.get()))
            .label("🌐 Translate")
            .style(serenity::ButtonStyle::Secondary),
    ]));
    let _ = message
        .channel_id
        .edit_message(
            &ctx.http,
            message.id,
            serenity::EditMessage::new().components(rows),
        )
        .await;
}
