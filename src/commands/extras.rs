//! Legacy `/play`, `/volume`, `/rank`, `/leaderboard` implementations.
//!
//! These are intentionally no longer registered as slash commands — the
//! panel-first UI owns those features now. The implementations are kept intact
//! so they can be re-registered later without rewriting them.

use poise::serenity_prelude as serenity;

use super::{Context, Error};
use crate::db;
use crate::ui::embeds;

/// Voice channel the invoking member is currently in (read from the cache).
fn author_voice_channel(ctx: Context<'_>) -> Option<u64> {
    let user_id = ctx.author().id;
    ctx.guild()
        .and_then(|guild| {
            guild
                .voice_states
                .get(&user_id)
                .and_then(|state| state.channel_id)
        })
        .map(|channel| channel.get())
}

async fn say(ctx: Context<'_>, embed: serenity::CreateEmbed, ephemeral: bool) -> Result<(), Error> {
    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .ephemeral(ephemeral),
    )
    .await?;
    Ok(())
}

/// Play a song or playlist in your voice channel.
#[poise::command(slash_command, guild_only, category = "Music")]
pub async fn play(
    ctx: Context<'_>,
    #[description = "Song name or link"] query: String,
) -> Result<(), Error> {
    let data = ctx.data();
    if !data.music.enabled() {
        return say(
            ctx,
            embeds::info("Music is off", "This bot has no Lavalink node connected."),
            true,
        )
        .await;
    }
    let Some(guild_id) = ctx.guild_id().map(|id| id.get()) else {
        return Ok(());
    };
    let Some(voice_channel) = author_voice_channel(ctx) else {
        return say(
            ctx,
            embeds::warn(
                "Join a voice channel",
                "Join a voice channel first, then run `/play`.",
            ),
            true,
        )
        .await;
    };
    let query = query.trim().to_string();
    if query.is_empty() {
        return say(
            ctx,
            embeds::warn("Nothing to play", "Give me a song name or link."),
            true,
        )
        .await;
    }

    ctx.defer().await?;

    let search = if query.starts_with("http://") || query.starts_with("https://") {
        query
    } else {
        format!("ytsearch:{query}")
    };
    let tracks = match data.music.load(guild_id, &search).await {
        Ok(tracks) => tracks,
        Err(err) => {
            tracing::warn!(error = %crate::error::display_anyhow(&err), "play: load failed");
            return say(
                ctx,
                embeds::error(
                    "Couldn't find that",
                    "Search failed — try different words or a direct link.",
                ),
                true,
            )
            .await;
        }
    };
    if tracks.is_empty() {
        return say(
            ctx,
            embeds::warn("No results", "Nothing matched that search."),
            true,
        )
        .await;
    }

    let first_title = tracks[0].info.title.clone();
    let author = ctx.author();
    match data
        .music
        .enqueue(
            ctx.serenity_context(),
            guild_id,
            voice_channel,
            ctx.channel_id().get(),
            tracks,
            author.name.clone(),
            author.id.get(),
        )
        .await
    {
        Ok(count) => {
            crate::music::refresh_panel(ctx.serenity_context(), guild_id, &data.music).await;
            let body = if count == 1 {
                format!("Queued **{}**.", crate::error::truncate(&first_title, 120))
            } else {
                format!(
                    "Queued **{count}** tracks, starting with **{}**.",
                    crate::error::truncate(&first_title, 120)
                )
            };
            say(ctx, embeds::success("Added to the queue", body), false).await
        }
        Err(err) => {
            tracing::warn!(error = %crate::error::display_anyhow(&err), "play: enqueue failed");
            say(
                ctx,
                embeds::error("Couldn't play that", crate::error::public(&err)),
                true,
            )
            .await
        }
    }
}

/// Set the music volume (0–200).
#[poise::command(slash_command, guild_only, category = "Music")]
pub async fn volume(
    ctx: Context<'_>,
    #[description = "Volume percent, 0–200"]
    #[min = 0]
    #[max = 200]
    percent: u16,
) -> Result<(), Error> {
    let data = ctx.data();
    let Some(guild_id) = ctx.guild_id().map(|id| id.get()) else {
        return Ok(());
    };
    if author_voice_channel(ctx).is_none() {
        return say(
            ctx,
            embeds::warn(
                "Join a voice channel",
                "You need to be in voice to change the volume.",
            ),
            true,
        )
        .await;
    }
    let percent = percent.min(200);
    match data.music.set_volume(guild_id, percent).await {
        Ok(()) => {
            let _ = db::music::set_volume(&data.db, guild_id, percent).await;
            say(
                ctx,
                embeds::ok(format!("Volume set to **{percent}%**.")),
                false,
            )
            .await
        }
        Err(err) => {
            tracing::warn!(error = %crate::error::display_anyhow(&err), "volume failed");
            say(
                ctx,
                embeds::error("Volume failed", crate::error::public(&err)),
                true,
            )
            .await
        }
    }
}

/// Show your level, XP and server rank.
#[poise::command(slash_command, guild_only, category = "Levels")]
pub async fn rank(
    ctx: Context<'_>,
    #[description = "Member to look up (default: you)"] member: Option<serenity::User>,
) -> Result<(), Error> {
    let data = ctx.data();
    let Some(guild_id) = ctx.guild_id().map(|id| id.get()) else {
        return Ok(());
    };
    let settings = db::leveling::settings(&data.db, guild_id).await?;
    if settings.enabled == 0 {
        return say(
            ctx,
            embeds::info(
                "Leveling is off",
                "An admin can enable it from `/settings`.",
            ),
            true,
        )
        .await;
    }
    let user = member.unwrap_or_else(|| ctx.author().clone());
    let row = db::leveling::get_user(&data.db, guild_id, user.id.get()).await?;
    let rank = db::leveling::get_rank(&data.db, guild_id, row.xp).await?;
    let (level, into, needed) = crate::leveling::progress(row.xp);
    let body = format!(
        "<@{}>\n**Level {level}** · rank **#{rank}**\nXP: **{into} / {needed}** to next level ({} total)\nMessages: **{}**",
        user.id.get(),
        row.xp,
        row.messages
    );
    say(ctx, embeds::info("Rank", body), false).await
}

/// Show the server's top members by XP.
#[poise::command(slash_command, guild_only, category = "Levels")]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let Some(guild_id) = ctx.guild_id().map(|id| id.get()) else {
        return Ok(());
    };
    let settings = db::leveling::settings(&data.db, guild_id).await?;
    if settings.enabled == 0 {
        return say(
            ctx,
            embeds::info(
                "Leveling is off",
                "An admin can enable it from `/settings`.",
            ),
            true,
        )
        .await;
    }
    let top = db::leveling::top(&data.db, guild_id, 10, 0).await?;
    if top.is_empty() {
        return say(
            ctx,
            embeds::info("Leaderboard", "Nobody has earned XP yet."),
            false,
        )
        .await;
    }
    let lines: Vec<String> = top
        .iter()
        .enumerate()
        .map(|(index, (user_id, xp))| {
            format!(
                "`{:>2}.` <@{user_id}> — level **{}** · {xp} XP",
                index + 1,
                crate::leveling::level_of(*xp)
            )
        })
        .collect();
    say(ctx, embeds::info("Leaderboard", lines.join("\n")), false).await
}
