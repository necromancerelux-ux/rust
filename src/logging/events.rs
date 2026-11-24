//! Server logging: shared admin log channel + per-category toggles.

use poise::serenity_prelude as serenity;

use crate::db;
use crate::state::Data;

/// Post an embed to the configured log channel (best effort, never panics).
pub async fn log(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    category: &str,
    embed: serenity::CreateEmbed,
) {
    if !db::logging::is_enabled(&data.db, guild_id, category).await {
        return;
    }
    let Ok(Some(channel_id)) = db::logging::channel_id(&data.db, guild_id).await else {
        return;
    };

    let _ = serenity::ChannelId::new(channel_id)
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .embed(embed)
                .allowed_mentions(
                    serenity::CreateAllowedMentions::new()
                        .everyone(false)
                        .all_users(false)
                        .all_roles(false),
                ),
        )
        .await;
}

pub fn entry(title: &str, description: &str, color: u32) -> serenity::CreateEmbed {
    serenity::CreateEmbed::new()
        .title(title)
        .description(description.chars().take(4096).collect::<String>())
        .color(color)
        .timestamp(serenity::Timestamp::now())
}

#[derive(Debug, Clone, Copy)]
pub enum Category {
    GiftCode,
    Players,
    Members,
    Messages,
    Voice,
    Channels,
    Roles,
    Server,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GiftCode => "giftcode",
            Self::Players => "players",
            Self::Members => "members",
            Self::Messages => "messages",
            Self::Voice => "voice",
            Self::Channels => "channels",
            Self::Roles => "roles",
            Self::Server => "server",
        }
    }
}
