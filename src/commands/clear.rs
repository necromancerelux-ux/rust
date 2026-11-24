//! `/clear` and `/clearall`.
//!
//! Ported from the legacy `clearCommand.js`:
//! - `/clear` bulk-deletes recent messages with optional user/role/bot filters.
//! - `/clearall` wipes a text channel by cloning it, repositioning the clone,
//!   and deleting the original.
//!
//! Both commands are always visible in the slash menu; permissions are checked
//! at runtime (Manage Messages / Manage Channels, or being a bot Admin).

use poise::serenity_prelude as serenity;

use super::{Context, Error};
use crate::permissions;
use crate::ui::embeds;
use crate::ui::theme::icons;

/// Send an ephemeral (or public) embed reply.
async fn say(ctx: Context<'_>, embed: serenity::CreateEmbed, ephemeral: bool) -> Result<(), Error> {
    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .ephemeral(ephemeral),
    )
    .await?;
    Ok(())
}

/// Edit the deferred interaction response (used after `defer_ephemeral`).
async fn edit_reply(ctx: Context<'_>, embed: serenity::CreateEmbed) -> Result<(), Error> {
    if let poise::Context::Application(app) = ctx {
        app.interaction
            .edit_response(
                ctx.http(),
                serenity::EditInteractionResponse::new().embed(embed),
            )
            .await?;
    }
    Ok(())
}

/// The bot's own permissions in a specific guild channel.
async fn bot_channel_permissions(
    ctx: Context<'_>,
    channel: &serenity::GuildChannel,
) -> Option<serenity::Permissions> {
    let guild_id = ctx.guild_id()?;
    let bot_id = ctx.cache().current_user().id;

    // Do not hold the cache read guard across an await.
    let cached = ctx
        .guild()
        .and_then(|guild| guild.members.get(&bot_id).cloned());
    let bot_member = match cached {
        Some(member) => member,
        None => ctx.http().get_member(guild_id, bot_id).await.ok()?,
    };

    let guild = ctx.guild()?;
    Some(guild.user_permissions_in(channel, &bot_member))
}

/// ♻️ Clears the message history by a specified amount of messages and optional filters.
#[poise::command(slash_command, guild_only, category = "Moderation")]
pub async fn clear(
    ctx: Context<'_>,
    #[description = "Number of messages to scan and clear (1–100, default 100)"]
    #[min = 1]
    #[max = 100]
    amount: Option<i64>,
    #[description = "Only clear messages from this user"] user: Option<serenity::User>,
    #[description = "Only clear messages from members with this role"] role: Option<serenity::Role>,
    #[description = "Only clear messages sent by bots"] bots: Option<bool>,
) -> Result<(), Error> {
    let has_manage_messages = permissions::member_permissions(&ctx)
        .await
        .is_some_and(|perms| perms.contains(serenity::Permissions::MANAGE_MESSAGES));

    if !has_manage_messages && !permissions::is_bot_admin(&ctx).await {
        return say(ctx, embeds::denied("Manage Messages"), true).await;
    }

    let Some(channel) = ctx.guild_channel().await else {
        return say(
            ctx,
            embeds::warn(
                "Not a server channel",
                "This command can only be used in a server text channel.",
            ),
            true,
        )
        .await;
    };

    let Some(bot_perms) = bot_channel_permissions(ctx, &channel).await else {
        return say(
            ctx,
            embeds::warn(
                "Can't check permissions",
                "I couldn't verify my permissions in this channel.",
            ),
            true,
        )
        .await;
    };

    if !bot_perms.contains(serenity::Permissions::MANAGE_MESSAGES) {
        return say(
            ctx,
            embeds::warn(
                "Missing permission",
                "I need the **Manage Messages** permission in this channel to clear messages.",
            ),
            true,
        )
        .await;
    }

    let amount = amount.unwrap_or(100).clamp(1, 100) as u8;
    let bots_only = bots == Some(true);

    ctx.defer_ephemeral().await?;

    // Fetch exactly `amount` recent messages, matching the legacy bot.
    let messages = match channel
        .id
        .messages(ctx.http(), serenity::GetMessages::new().limit(amount))
        .await
    {
        Ok(messages) => messages,
        Err(err) => {
            tracing::warn!(error = %err, "clear: fetching messages failed");
            return edit_reply(
                ctx,
                embeds::error(
                    "Clear failed",
                    "There was an error while clearing messages.\n\
                     Please check that the bot has **Manage Messages** and that the messages are less than 14 days old.",
                ),
            )
            .await;
        }
    };

    // Only non-pinned messages. The bot has Manage Messages, so every
    // remaining message is deletable.
    let matching: Vec<serenity::Message> = messages
        .into_iter()
        .filter(|message| !message.pinned)
        .filter(|message| {
            user.as_ref()
                .is_none_or(|filter| message.author.id == filter.id)
        })
        .filter(|message| {
            role.as_ref().is_none_or(|filter| {
                message
                    .member
                    .as_ref()
                    .is_some_and(|member| member.roles.contains(&filter.id))
            })
        })
        .filter(|message| !bots_only || message.author.bot)
        .collect();

    if matching.is_empty() {
        return edit_reply(
            ctx,
            embeds::info("Nothing to delete", "No matching messages found to delete."),
        )
        .await;
    }

    // Discord refuses bulk-deleting messages older than 14 days (1h safety
    // margin), so filter them out before calling the API.
    let cutoff = chrono::Utc::now().timestamp() - 14 * 24 * 3600 + 3600;
    let ids: Vec<serenity::MessageId> = matching
        .iter()
        .filter(|message| message.id.created_at().unix_timestamp() >= cutoff)
        .map(|message| message.id)
        .collect();

    if ids.is_empty() {
        return edit_reply(
            ctx,
            embeds::warn(
                "Nothing could be deleted",
                "All matching messages are older than 14 days and can't be bulk-deleted.",
            ),
        )
        .await;
    }

    let deleted = ids.len();
    let result = match deleted {
        1 => channel.id.delete_message(ctx.http(), ids[0]).await,
        _ => channel.id.delete_messages(ctx.http(), ids).await,
    };

    if let Err(err) = result {
        tracing::warn!(error = %err, "clear: deleting messages failed");
        return edit_reply(
            ctx,
            embeds::error(
                "Clear failed",
                "There was an error while clearing messages.\n\
                 Please check that the bot has **Manage Messages** and that the messages are less than 14 days old.",
            ),
        )
        .await;
    }

    let mut filter_parts = Vec::new();
    if let Some(filter) = &user {
        filter_parts.push(format!("from **{}**", filter.tag()));
    }
    if let Some(filter) = &role {
        filter_parts.push(format!("with role **{}**", filter.name));
    }
    if bots_only {
        filter_parts.push("from bots".to_string());
    }
    let filter_text = if filter_parts.is_empty() {
        String::new()
    } else {
        format!(" {}", filter_parts.join(", "))
    };

    let plural = if deleted == 1 { "" } else { "s" };
    edit_reply(
        ctx,
        embeds::success(
            "Messages cleared",
            format!("Successfully deleted **{deleted}** message{plural}{filter_text}."),
        ),
    )
    .await
}

/// ♻️ Clears ALL messages in a channel by cloning it (destructive).
#[poise::command(slash_command, guild_only, category = "Moderation")]
pub async fn clearall(ctx: Context<'_>) -> Result<(), Error> {
    let has_manage_channels = permissions::member_permissions(&ctx)
        .await
        .is_some_and(|perms| perms.contains(serenity::Permissions::MANAGE_CHANNELS));

    if !has_manage_channels && !permissions::is_bot_admin(&ctx).await {
        return say(ctx, embeds::denied("Manage Channels"), true).await;
    }

    let Some(channel) = ctx.guild_channel().await else {
        return say(
            ctx,
            embeds::warn(
                "Not a server channel",
                "This command can only be used in a server text channel.",
            ),
            true,
        )
        .await;
    };

    if channel.kind != serenity::ChannelType::Text {
        return say(
            ctx,
            embeds::warn(
                "Not a text channel",
                "This command can only be used in a server text channel.",
            ),
            true,
        )
        .await;
    }

    let Some(bot_perms) = bot_channel_permissions(ctx, &channel).await else {
        return say(
            ctx,
            embeds::warn(
                "Can't check permissions",
                "I couldn't verify my permissions in this channel.",
            ),
            true,
        )
        .await;
    };

    if !bot_perms.contains(serenity::Permissions::MANAGE_CHANNELS) {
        return say(
            ctx,
            embeds::warn(
                "Missing permission",
                "I need the **Manage Channels** permission to clone/delete this channel.",
            ),
            true,
        )
        .await;
    }

    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => {
            return say(
                ctx,
                embeds::warn(
                    "Not a server channel",
                    "This command can only be used in a server text channel.",
                ),
                true,
            )
            .await;
        }
    };

    ctx.defer_ephemeral().await?;

    let old_id = channel.id;
    let position = channel.position;
    let author = ctx.author();
    let reason = format!(
        "Clear all messages requested by {} ({})",
        author.tag(),
        author.id
    );

    // Clone keeps name, topic, permissions, NSFW, slowmode, category, etc.
    let mut builder = serenity::CreateChannel::new(channel.name.clone())
        .kind(channel.kind)
        .nsfw(channel.nsfw)
        .position(position)
        .permissions(channel.permission_overwrites.clone());

    if let Some(parent) = channel.parent_id {
        builder = builder.category(parent);
    }
    if let Some(topic) = &channel.topic {
        builder = builder.topic(topic.clone());
    }
    if let Some(rate_limit) = channel.rate_limit_per_user {
        builder = builder.rate_limit_per_user(rate_limit);
    }
    if let Some(bitrate) = channel.bitrate {
        builder = builder.bitrate(bitrate);
    }
    if let Some(user_limit) = channel.user_limit {
        builder = builder.user_limit(user_limit);
    }
    if let Some(region) = &channel.rtc_region {
        builder = builder.rtc_region(region.clone());
    }
    if let Some(quality) = channel.video_quality_mode {
        builder = builder.video_quality_mode(quality);
    }
    if let Some(duration) = channel.default_auto_archive_duration {
        builder = builder.default_auto_archive_duration(duration);
    }
    builder = builder.audit_log_reason(&reason);

    let new_channel = match guild_id.create_channel(ctx.http(), builder).await {
        Ok(channel) => channel,
        Err(err) => {
            tracing::warn!(error = %err, "clearall: cloning channel failed");
            return edit_reply(
                ctx,
                embeds::error(
                    "Clear failed",
                    "There was an error while clearing this channel.\n\
                     Please check that the bot has **Manage Channels** (and is not restricted by role hierarchy).",
                ),
            )
            .await;
        }
    };

    // Place the clone at the same position (Discord may not honour `position`
    // on creation). Also carry over the default thread slowmode.
    let mut edit = serenity::EditChannel::new().position(position);
    if let Some(rate_limit) = channel.default_thread_rate_limit_per_user {
        edit = edit.default_thread_rate_limit_per_user(rate_limit);
    }

    // The clone already exists with the right settings, so a failed
    // reposition is cosmetic: keep going rather than risk leaving a duplicate.
    if let Err(err) = new_channel.id.edit(ctx.http(), edit).await {
        tracing::warn!(error = %err, "clearall: repositioning clone failed");
    }

    // Deleting the original is what actually clears the history.
    if let Err(err) = ctx.http().delete_channel(old_id, Some(&reason)).await {
        tracing::warn!(error = %err, "clearall: deleting original channel failed");
        // The original is still there; remove the clone so the server does not
        // end up with two channels.
        if let Err(rollback) = ctx
            .http()
            .delete_channel(new_channel.id, Some("Rolling back /clearall clone"))
            .await
        {
            tracing::error!(error = %rollback, "clearall: rolling back clone failed");
        }
        return edit_reply(
            ctx,
            embeds::error(
                "Clear failed",
                "There was an error while clearing this channel.\n\
                 Please check that the bot has **Manage Channels** (and is not restricted by role hierarchy).",
            ),
        )
        .await;
    }

    // The interaction's original channel is gone, so the deferred ephemeral
    // response can no longer be edited. The public notice is the confirmation.
    let notice = format!(
        "{} <@{}> cleared all messages in this channel.",
        icons::TRASH,
        author.id
    );
    if let Err(err) = new_channel.id.say(ctx.http(), notice).await {
        tracing::warn!(error = %err, "clearall: sending public notice failed");
    }

    Ok(())
}
