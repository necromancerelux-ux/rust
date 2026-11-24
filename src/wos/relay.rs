//! WOS announcement relay (hub + fan-out).
//!
//! Getting WOS's posts to leave their Discord server is done by Discord's own
//! "Follow Channel" feature (Announcement-type channels only) — but instead of
//! every server following WOS directly, it's followed ONCE into a single "hub"
//! channel, in a server the bot owner controls. When a post lands in the hub
//! channel (as a message from Discord's system crosspost webhook), this:
//!
//!  1. Attaches an on-demand Translate button to a companion message in the
//!     hub (the bot can't edit Discord's own webhook message).
//!  2. Reposts a copy into every server that configured a destination
//!     channel, each with its own Translate button.

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, CreateAllowedMentions};

use crate::state::Data;

const TRANSLATE_COMPANION: &str =
    "❄️ New WOS announcement above — tap to get a private translation.";

struct Payload {
    embed_title: Option<String>,
    embed_description: Option<String>,
    content: Option<String>,
}

impl Payload {
    fn extract(message: &serenity::Message) -> Option<Self> {
        if let Some(embed) = message.embeds.first() {
            if embed.title.is_some() || embed.description.is_some() {
                return Some(Self {
                    embed_title: embed.title.clone(),
                    embed_description: embed.description.clone(),
                    content: None,
                });
            }
        }
        if !message.content.trim().is_empty() {
            return Some(Self {
                embed_title: None,
                embed_description: None,
                content: Some(message.content.clone()),
            });
        }
        None // attachment/image-only post — nothing to translate
    }

    fn embed_json(&self) -> Option<String> {
        let stored = crate::translate::StoredEmbed {
            title: self.embed_title.clone(),
            description: self.embed_description.clone(),
            fields: Vec::new(),
            footer: None,
            color: Some(crate::ui::theme::colors::BRAND),
        };
        serde_json::to_string(&stored).ok()
    }
}

fn clone_embed(message: &serenity::Message) -> Option<serenity::CreateEmbed> {
    let embed = message.embeds.first()?;
    let mut create = serenity::CreateEmbed::new();
    if let Some(title) = &embed.title {
        create = create.title(title);
    }
    if let Some(description) = &embed.description {
        create = create.description(description);
    }
    if let Some(url) = &embed.url {
        create = create.url(url);
    }
    for field in &embed.fields {
        create = create.field(&field.name, &field.value, field.inline);
    }
    if let Some(footer) = &embed.footer {
        create = create.footer(serenity::CreateEmbedFooter::new(footer.text.clone()));
    }
    if let Some(color) = embed.colour {
        create = create.color(color.0);
    }
    if let Some(image) = &embed.image {
        create = create.image(image.url.clone());
    }
    Some(create)
}

/// Attach a Translate button to a bot-owned message and remember it.
async fn attach_translate(
    ctx: &serenity::Context,
    data: &Data,
    message_id: serenity::MessageId,
    payload: &Payload,
    guild_id: Option<u64>,
    channel_id: u64,
    kind: &str,
    full_embed_json: Option<String>,
) -> anyhow::Result<()> {
    ChannelId::new(channel_id)
        .edit_message(
            &ctx.http,
            message_id,
            serenity::EditMessage::new()
                .components(vec![crate::translate::translate_row(message_id.get())]),
        )
        .await?;

    crate::translate::register(
        &data.db,
        message_id.get(),
        crate::db::translate::NewTranslatable {
            guild_id,
            channel_id: Some(channel_id),
            kind,
            embed_json: full_embed_json.or_else(|| payload.embed_json()),
            content: payload.content.clone(),
            owner_id: None,
        },
    )
    .await?;
    Ok(())
}

/// Fan a hub-channel webhook post out to every configured server.
pub async fn on_message(
    ctx: &serenity::Context,
    data: &Data,
    message: &serenity::Message,
) -> anyhow::Result<()> {
    if message.webhook_id.is_none() {
        return Ok(()); // bot's own messages are never webhooks
    }
    let Some(guild_id) = message.guild_id else {
        return Ok(());
    };
    let guild_id = guild_id.get();

    let hub = crate::db::wos_relay::hub(&data.db).await?;
    if hub.guild() != Some(guild_id) || hub.channel() != Some(message.channel_id.get()) {
        return Ok(());
    }

    let Some(payload) = Payload::extract(message) else {
        return Ok(());
    };

    // 1) Hub: the post is already there — add a companion translate message.
    let companion = ChannelId::new(message.channel_id.get())
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .content(TRANSLATE_COMPANION)
                .allowed_mentions(CreateAllowedMentions::new()),
        )
        .await?;
    let _ = crate::db::welcome::record_wos_cleanup_message(
        &data.db,
        guild_id,
        message.channel_id.get(),
        companion.id.get(),
    )
    .await;
    if let Err(err) = attach_translate(
        ctx,
        data,
        companion.id,
        &payload,
        Some(guild_id),
        message.channel_id.get(),
        "wos_announcement",
        clone_embed(message).and_then(|embed| serde_json::to_string(&embed).ok()),
    )
    .await
    {
        tracing::warn!(error = %err, "translate companion attach failed");
    }

    // 2) Repost into every destination, with its own translate button.
    for (dest_guild, dest_channel) in crate::db::wos_relay::destinations(&data.db).await? {
        let target = ChannelId::new(dest_channel);
        let mut builder =
            serenity::CreateMessage::new().allowed_mentions(CreateAllowedMentions::new());
        if payload.embed_description.is_some() || payload.embed_title.is_some() {
            if let Some(embed) = clone_embed(message) {
                builder = builder.embed(embed);
            }
        } else if let Some(content) = &payload.content {
            builder = builder.content(content.clone());
        }
        match target.send_message(&ctx.http, builder).await {
            Ok(sent) => {
                let _ = crate::db::welcome::record_wos_cleanup_message(
                    &data.db,
                    dest_guild,
                    dest_channel,
                    sent.id.get(),
                )
                .await;
                if let Err(err) = attach_translate(
                    ctx,
                    data,
                    sent.id,
                    &payload,
                    Some(dest_guild),
                    dest_channel,
                    "wos_announcement",
                    clone_embed(message).and_then(|embed| serde_json::to_string(&embed).ok()),
                )
                .await
                {
                    tracing::warn!(error = %err, channel = dest_channel, "translate attach failed");
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, channel = dest_channel, "wos relay repost failed");
            }
        }
    }

    Ok(())
}
