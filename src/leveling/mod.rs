//! Leveling engine: XP curve, message handling and voice XP.
//!
//! The curve matches the legacy bot (and MEE6): XP needed to go from level `n`
//! to `n+1` is `5n² + 50n + 100`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, UserId};

use crate::db;
use crate::state::Data;

/// XP to advance from `level` to `level + 1`.
pub fn xp_to_next(level: i64) -> i64 {
    5 * level * level + 50 * level + 100
}

/// Total XP required to reach `level`.
pub fn total_for_level(level: i64) -> i64 {
    (0..level).map(xp_to_next).sum()
}

/// `(level, current_xp_in_level, xp_needed_for_next)`.
pub fn progress(total_xp: i64) -> (i64, i64, i64) {
    let total_xp = total_xp.max(0);
    let mut level = 0i64;
    let mut remaining = total_xp;
    loop {
        let needed = xp_to_next(level);
        if remaining < needed {
            return (level, remaining, needed);
        }
        remaining -= needed;
        level += 1;
    }
}

/// Level implied by a raw XP total.
pub fn level_of(xp: i64) -> i64 {
    progress(xp).0
}

static COOLDOWNS: std::sync::LazyLock<Mutex<HashMap<(u64, u64), Instant>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn cooldown_expired(guild_id: u64, user_id: u64, cooldown_sec: i64) -> bool {
    let now = Instant::now();
    let mut map = COOLDOWNS.lock().unwrap_or_else(|err| err.into_inner());
    if map.len() > 5_000 {
        // Entries older than an hour can never still be in cooldown.
        map.retain(|_, last| now.duration_since(*last).as_secs() < 3600);
    }
    match map.get(&(guild_id, user_id)) {
        Some(last) if now.duration_since(*last).as_secs() < cooldown_sec.max(0) as u64 => false,
        _ => {
            map.insert((guild_id, user_id), now);
            true
        }
    }
}

fn random_range(min: i64, max: i64) -> i64 {
    use rand::Rng;
    let lo = min.min(max);
    let hi = min.max(max);
    if lo == hi {
        return lo;
    }
    rand::thread_rng().gen_range(lo..=hi)
}

pub async fn handle_message(ctx: &serenity::Context, data: &Data, message: &serenity::Message) {
    let Some(guild_id) = message.guild_id else {
        return;
    };
    if message.author.bot || message.webhook_id.is_some() {
        return;
    }

    let settings = match db::leveling::settings(&data.db, guild_id.get()).await {
        Ok(settings) => settings,
        Err(err) => {
            tracing::debug!(error = %err, "leveling settings read failed");
            return;
        }
    };
    if !settings.enabled() {
        return;
    }

    if is_ignored(ctx, &settings, guild_id.get(), message.channel_id.get()) {
        return;
    }
    if !cooldown_expired(
        guild_id.get(),
        message.author.id.get(),
        settings.cooldown_sec,
    ) {
        return;
    }

    let gain = random_range(settings.xp_min, settings.xp_max);
    let Ok((before, after)) =
        db::leveling::add_xp(&data.db, guild_id.get(), message.author.id.get(), gain, 1).await
    else {
        return;
    };

    after_gain(
        ctx,
        data,
        settings,
        guild_id.get(),
        message.author.id.get(),
        before,
        after,
        Some(message.channel_id.get()),
    )
    .await;
}

/// Look up a channel or an active thread in the guild cache.
fn find_channel<'a>(
    guild: &'a serenity::Guild,
    channel_id: u64,
) -> Option<&'a serenity::GuildChannel> {
    guild
        .channels
        .get(&serenity::ChannelId::new(channel_id))
        .or_else(|| {
            guild
                .threads
                .iter()
                .find(|thread| thread.id.get() == channel_id)
        })
}

/// Mirrors the JavaScript engine: a channel is ignored when it, its parent
/// (forum/text channel), or its grandparent (category) is in the ignore list.
fn is_ignored_in(
    guild: &serenity::Guild,
    settings: &db::leveling::LevelSettings,
    channel_id: u64,
) -> bool {
    let ignored = settings.ignored_channels();
    if ignored.contains(&channel_id) {
        return true;
    }
    let Some(channel) = find_channel(guild, channel_id) else {
        return false;
    };
    let Some(parent_id) = channel.parent_id else {
        return false;
    };
    if ignored.contains(&parent_id.get()) {
        return true;
    }
    find_channel(guild, parent_id.get())
        .and_then(|parent| parent.parent_id)
        .is_some_and(|grandparent| ignored.contains(&grandparent.get()))
}

fn is_ignored(
    ctx: &serenity::Context,
    settings: &db::leveling::LevelSettings,
    guild_id: u64,
    channel_id: u64,
) -> bool {
    let Some(guild) = ctx.cache.guild(serenity::GuildId::new(guild_id)) else {
        return false;
    };
    is_ignored_in(&guild, settings, channel_id)
}

#[allow(clippy::too_many_arguments)]
async fn after_gain(
    ctx: &serenity::Context,
    data: &Data,
    settings: db::leveling::LevelSettings,
    guild_id: u64,
    user_id: u64,
    before: i64,
    after: i64,
    message_channel: Option<u64>,
) {
    let (old_level, _, _) = progress(before);
    let (new_level, _, _) = progress(after);
    if new_level <= old_level {
        return;
    }

    if let Err(err) = apply_rewards(ctx, data, &settings, guild_id, user_id, new_level).await {
        tracing::debug!(error = %err, guild = guild_id, level = new_level, "reward failed");
    }

    announce(
        ctx,
        data,
        &settings,
        guild_id,
        user_id,
        new_level,
        after,
        message_channel,
    )
    .await;
}

/// Assign any reward roles the bot is allowed to hand out.
///
/// Mirrors the JavaScript engine:
/// * Stack mode adds every earned reward the member is missing.
/// * Replace mode keeps only the highest earned reward and removes *every other
///   configured reward role* the member currently holds (not just roles at or
///   below their level), matching `allRewardIds` in the legacy bot.
async fn apply_rewards(
    ctx: &serenity::Context,
    data: &Data,
    settings: &db::leveling::LevelSettings,
    guild_id: u64,
    user_id: u64,
    level: i64,
) -> anyhow::Result<()> {
    let rewards = db::leveling::list_rewards(&data.db, guild_id).await?;
    let earned: Vec<&db::leveling::LevelReward> = rewards
        .iter()
        .filter(|reward| reward.level <= level)
        .collect();
    if earned.is_empty() {
        return Ok(());
    }

    // Read everything we need from the cache in one borrow, then drop the
    // guard before any `.await` (the guard is not `Send`).
    let (member, member_ids, roles) = {
        let Some(guild) = ctx.cache.guild(guild_id) else {
            return Ok(());
        };

        let bot_id = ctx.cache.current_user().id;

        // Highest position among the bot's own roles; nothing can be assigned
        // above it (the JS `me.roles.highest.comparePositionTo(role) > 0`).
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

        let roles: HashMap<u64, (u16, bool)> = guild
            .roles
            .iter()
            .map(|(id, role)| (id.get(), (role.position, role.managed)))
            .collect();

        let member = guild.members.get(&UserId::new(user_id)).cloned();
        let member_ids: Vec<u64> = member
            .as_ref()
            .map(|member| member.roles.iter().map(|id| id.get()).collect())
            .unwrap_or_default();

        (member, member_ids, (roles, bot_top))
    };

    let Some(member) = member else {
        return Ok(());
    };
    let (roles, bot_top) = roles;
    let usable = |role_id: u64| -> bool {
        roles
            .get(&role_id)
            .is_some_and(|(position, managed)| !*managed && *position < bot_top)
    };

    if settings.stack_rewards() {
        // Add every earned reward the member is missing.
        let missing: Vec<serenity::RoleId> = earned
            .iter()
            .filter_map(|reward| reward.role_id.parse::<u64>().ok())
            .filter(|role_id| usable(*role_id) && !member_ids.contains(role_id))
            .map(serenity::RoleId::new)
            .collect();
        if !missing.is_empty() {
            let _ = member.add_roles(&ctx.http, &missing).await;
        }
    } else {
        // Replace mode: keep only the highest reward role earned. The legacy
        // bot compares against the last entry of the level-sorted reward list.
        let highest_id = earned
            .last()
            .and_then(|reward| reward.role_id.parse::<u64>().ok());
        let remove: Vec<serenity::RoleId> = rewards
            .iter()
            .filter_map(|reward| reward.role_id.parse::<u64>().ok())
            .filter(|role_id| Some(*role_id) != highest_id)
            .filter(|role_id| member_ids.contains(role_id) && usable(*role_id))
            .map(serenity::RoleId::new)
            .collect();
        if !remove.is_empty() {
            let _ = member.remove_roles(&ctx.http, &remove).await;
        }
        if let Some(highest_id) = highest_id {
            if usable(highest_id) && !member_ids.contains(&highest_id) {
                let _ = member
                    .add_role(&ctx.http, serenity::RoleId::new(highest_id))
                    .await;
            }
        }
    }

    Ok(())
}

/// Send the level-up announcement if the settings allow it.
async fn announce(
    ctx: &serenity::Context,
    data: &Data,
    settings: &db::leveling::LevelSettings,
    guild_id: u64,
    user_id: u64,
    level: i64,
    xp: i64,
    message_channel: Option<u64>,
) {
    if settings.announce_mode == "off" {
        return;
    }

    // Resolve everything from the cache in one borrow, then drop the guard
    // before the HTTP call (the guard is not `Send`).
    let (channel, username) = {
        let guild = ctx.cache.guild(serenity::GuildId::new(guild_id));

        // JavaScript falls back to the level-up channel when the configured
        // announcement channel is missing from the cache.
        let configured = settings.announce_channel().filter(|id| {
            guild
                .as_ref()
                .is_some_and(|guild| find_channel(guild, *id).is_some())
        });
        let channel = match settings.announce_mode.as_str() {
            "channel" => configured.or(message_channel),
            _ => message_channel,
        };

        // JavaScript rejects any non-text target (`!target.isTextBased()`), so
        // a voice channel is never announced into even in "channel" mode.
        if let (Some(channel), Some(guild)) = (channel, guild.as_ref()) {
            if find_channel(guild, channel).is_some_and(|target| {
                !matches!(
                    target.kind,
                    serenity::ChannelType::Text
                        | serenity::ChannelType::News
                        | serenity::ChannelType::PublicThread
                        | serenity::ChannelType::PrivateThread
                        | serenity::ChannelType::NewsThread
                )
            }) {
                return;
            }
        }

        let username = guild
            .as_ref()
            .and_then(|guild| guild.members.get(&UserId::new(user_id)))
            .map(|member| member.display_name().to_string())
            .or_else(|| ctx.cache.user(user_id).map(|user| user.name.clone()))
            .unwrap_or_else(|| format!("<@{user_id}>"));

        (channel, username)
    };

    let Some(channel) = channel else {
        return;
    };

    let text = render_template(&settings.announce_message, user_id, &username, level, xp);
    let text: String = text.chars().take(2000).collect();

    if let Ok(message) = ChannelId::new(channel)
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .content(text)
                .allowed_mentions(serenity::CreateAllowedMentions::new().users([user_id])),
        )
        .await
    {
        if let Err(error) = db::welcome::record_leveling_cleanup_message(
            &data.db,
            guild_id,
            channel,
            message.id.get(),
        )
        .await
        {
            tracing::debug!(%error, guild_id, channel, "failed to record leveling announcement cleanup");
        }
    }
}

/// Voice XP loop: pays XP to humans in voice/stage channels with at least two
/// humans. Mirrors the JavaScript engine: bots, the AFK channel, ignored
/// channels, and channels that are not voice/stage are all skipped.
pub async fn voice_tick(ctx: &serenity::Context, data: &Data) {
    let guild_ids = ctx.cache.guilds();
    for guild_id in guild_ids {
        let Ok(settings) = db::leveling::settings(&data.db, guild_id.get()).await else {
            continue;
        };
        if !settings.enabled() || settings.voice_xp <= 0 {
            continue;
        }

        // channel id -> (user id, eligible for XP). Ineligible (deafened or
        // muted) humans still count toward the two-human threshold, exactly
        // like the JavaScript `humans.size < 2` check.
        let mut channels: HashMap<u64, Vec<(u64, bool)>> = HashMap::new();
        {
            let Some(guild) = ctx.cache.guild(guild_id) else {
                continue;
            };
            let afk_channel = guild
                .afk_metadata
                .as_ref()
                .map(|afk| afk.afk_channel_id.get());

            for (user_id, state) in guild.voice_states.iter() {
                let Some(channel_id) = state.channel_id else {
                    continue;
                };
                if Some(channel_id.get()) == afk_channel {
                    continue;
                }
                let Some(channel) = guild.channels.get(&channel_id) else {
                    continue;
                };
                if !matches!(
                    channel.kind,
                    serenity::ChannelType::Voice | serenity::ChannelType::Stage
                ) {
                    continue;
                }
                if is_ignored_in(&guild, &settings, channel_id.get()) {
                    continue;
                }
                if guild
                    .members
                    .get(&user_id)
                    .is_some_and(|member| member.user.bot)
                {
                    continue;
                }
                let eligible = !(state.self_deaf || state.deaf || state.self_mute || state.mute);
                channels
                    .entry(channel_id.get())
                    .or_default()
                    .push((user_id.get(), eligible));
            }
        }

        for (_channel, members) in channels {
            if members.len() < 2 {
                continue;
            }
            for (user_id, eligible) in members {
                if !eligible {
                    continue;
                }
                let Ok((before, after)) =
                    db::leveling::add_xp(&data.db, guild_id.get(), user_id, settings.voice_xp, 0)
                        .await
                else {
                    continue;
                };
                after_gain(
                    ctx,
                    data,
                    settings.clone(),
                    guild_id.get(),
                    user_id,
                    before,
                    after,
                    None,
                )
                .await;
            }
        }
    }
}

/// Expand `{user}`, `{username}`, `{level}`, `{xp}` placeholders.
pub fn render_template(
    template: &str,
    user_id: u64,
    username: &str,
    level: i64,
    xp: i64,
) -> String {
    template
        .replace("{user}", &format!("<@{user_id}>"))
        .replace("{username}", username)
        .replace("{level}", &level.to_string())
        .replace("{xp}", &xp.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_matches_mee6() {
        assert_eq!(xp_to_next(0), 100);
        assert_eq!(xp_to_next(1), 155);
        assert_eq!(xp_to_next(2), 220);
        assert_eq!(total_for_level(1), 100);
    }

    #[test]
    fn progress_at_zero() {
        let (level, current, needed) = progress(0);
        assert_eq!((level, current, needed), (0, 0, 100));
    }

    #[test]
    fn level_one() {
        assert_eq!(level_of(100), 1);
        assert_eq!(level_of(254), 1);
        assert_eq!(level_of(255), 2);
    }

    #[test]
    fn template() {
        assert_eq!(
            render_template("hi {user} you are {level}", 42, "Kai", 3, 999),
            "hi <@42> you are 3"
        );
    }
}
