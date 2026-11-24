//! Shared giveaway rendering/ending logic (used by tasks and the panel).

use crate::db;
use crate::state::Data;
use crate::ui::theme::icons;
use crate::ui::{components, theme};
use poise::serenity_prelude as serenity;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// JS `engine.scheduleRefresh`: one debounced counter refresh per giveaway.
static ENTRY_REFRESH: LazyLock<Mutex<HashMap<i64, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn schedule_entry_refresh(ctx: serenity::Context, data: Data, giveaway_id: i64) {
    {
        let mut pending = ENTRY_REFRESH.lock().unwrap_or_else(|err| err.into_inner());
        if pending.contains_key(&giveaway_id) {
            return;
        }
        pending.insert(giveaway_id, Instant::now());
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        ENTRY_REFRESH
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .remove(&giveaway_id);

        let Ok(Some(giveaway)) = db::giveaways::get(&data.db, giveaway_id).await else {
            return;
        };
        if !giveaway.is_active() {
            return;
        }
        let (Some(channel_id), Some(message_id)) = (giveaway.channel_id(), giveaway.message_id())
        else {
            return;
        };
        let Ok((embed, _)) = render_giveaway(&data.db, &giveaway).await else {
            return;
        };
        let Ok(mut message) = serenity::ChannelId::new(channel_id)
            .message(&ctx.http, serenity::MessageId::new(message_id))
            .await
        else {
            return;
        };
        let _ = message
            .edit(&ctx.http, serenity::EditMessage::new().embed(embed))
            .await;
    });
}

/// JS `view.baseEmbed` — Giveaway footer shared by every giveaway embed.
fn base_embed(giveaway: &db::giveaways::Giveaway) -> serenity::CreateEmbed {
    let mut embed = serenity::CreateEmbed::new().footer(serenity::CreateEmbedFooter::new(format!(
        "Giveaway #{}  •  {}",
        giveaway.id,
        theme::footer()
    )));
    if let Some(timestamp) = giveaway
        .ends_at
        .checked_div(1000)
        .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
    {
        embed = embed.timestamp(timestamp);
    }
    embed
}

/// JS `view.buildActiveEmbed`.
fn active_embed(giveaway: &db::giveaways::Giveaway, entries: i64) -> serenity::CreateEmbed {
    let mut lines = Vec::new();
    if let Some(description) = giveaway
        .description
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        let description: String = description.chars().take(1500).collect();
        lines.push(description);
        lines.push(String::new());
    }
    let ends = giveaway.ends_at / 1000;
    lines.push(format!("**Ends:** <t:{ends}:R> (<t:{ends}:f>)",));
    lines.push(format!("**Hosted by:** <@{}>", giveaway.host_id));
    lines.push(format!("**Winners:** {}", giveaway.winner_count()));
    lines.push(format!("**Entries:** {entries}"));
    if let Some(role) = giveaway.required_role_id() {
        lines.push(format!("**Requirement:** must have <@&{role}>"));
    }
    let (requirements, bonuses) = giveaway.rules().describe();
    if !requirements.is_empty() {
        lines.push(format!("**Also required:** {}", requirements.join(" • ")));
    }
    if !bonuses.is_empty() {
        lines.push(format!("**Bonus entries:** {}", bonuses.join(" • ")));
    }
    lines.push(String::new());
    lines.push("Press **Enter** below to join!".to_string());

    let title: String = giveaway.prize.chars().take(240).collect();
    base_embed(giveaway)
        .title(format!("🎉 {title}"))
        .description(lines.join("\n"))
        .color(0xF1C4_0F)
}

/// JS `view.buildEndedEmbed`.
fn ended_embed(
    giveaway: &db::giveaways::Giveaway,
    entries: i64,
    winners: &[u64],
) -> serenity::CreateEmbed {
    let winner_line = if winners.is_empty() {
        "**Winners:** nobody — there were no valid entries.".to_string()
    } else {
        let list = winners
            .iter()
            .map(|id| format!("<@{id}>"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "**{}:** {list}",
            if winners.len() == 1 {
                "Winner"
            } else {
                "Winners"
            }
        )
    };
    let title: String = giveaway.prize.chars().take(230).collect();
    base_embed(giveaway)
        .title(format!("🎉 {title} — ENDED"))
        .description(format!(
            "{winner_line}\n**Hosted by:** <@{}>\n**Total entries:** {entries}",
            giveaway.host_id
        ))
        .color(0x95A5_A6)
}

/// JS `view.buildCancelledEmbed`.
fn cancelled_embed(giveaway: &db::giveaways::Giveaway) -> serenity::CreateEmbed {
    let title: String = giveaway.prize.chars().take(230).collect();
    base_embed(giveaway)
        .title(format!("🚫 {title} — CANCELLED"))
        .description(format!(
            "This giveaway was cancelled by an admin.\n**Hosted by:** <@{}>",
            giveaway.host_id
        ))
        .color(0xED42_45)
}

/// Render a giveaway embed + enter button.
pub async fn render_giveaway(
    db: &crate::db::Db,
    giveaway: &db::giveaways::Giveaway,
) -> anyhow::Result<(serenity::CreateEmbed, Vec<serenity::CreateActionRow>)> {
    let entries = db::giveaways::count_entries(db, giveaway.id).await?;
    let embed = active_embed(giveaway, entries);

    let rows = vec![components::row(vec![components::primary(
        format!("giveaway:enter:{}", giveaway.id),
        format!("{} Enter", icons::ADD),
    )])];

    Ok((embed, rows))
}

/// Publish the public entry post for a newly created giveaway and persist its
/// message ID so the scheduled end flow can update it later.
pub async fn post_giveaway(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    giveaway_id: i64,
) -> anyhow::Result<u64> {
    let giveaway = db::giveaways::get(&data.db, giveaway_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("giveaway #{giveaway_id} was not found"))?;
    let channel_id = giveaway
        .channel_id()
        .ok_or_else(|| anyhow::anyhow!("giveaway channel ID is invalid"))?;
    let (embed, rows) = render_giveaway(&data.db, &giveaway).await?;
    let message = serenity::ChannelId::new(channel_id)
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new().embed(embed).components(rows),
        )
        .await?;
    if let Err(err) = db::giveaways::set_message_id(&data.db, giveaway_id, message.id.get()).await {
        let _ = message.delete(&ctx.http).await;
        return Err(err.into());
    }
    Ok(message.id.get())
}

/// Check the advanced giveaway rules for a member (blocked roles, account
/// age, server age, min level). Returns the legacy-style failure reason.
pub async fn check_eligibility(
    data: &crate::state::Data,
    giveaway: &db::giveaways::Giveaway,
    member: &serenity::Member,
) -> Result<(), String> {
    if member.user.bot {
        return Err("bots can't enter".to_string());
    }
    if let Some(required_role) = giveaway.required_role_id() {
        if !member.roles.iter().any(|role| role.get() == required_role) {
            return Err(format!(
                "you need <@&{required_role}> to enter this giveaway"
            ));
        }
    }
    let rules = giveaway.rules();

    if let Some(blocked) = rules
        .blocked_role_ids
        .iter()
        .find(|id| member.roles.iter().any(|r| r.get() == **id))
    {
        return Err(format!(
            "members with <@&{blocked}> can't enter this giveaway"
        ));
    }

    let now = chrono::Utc::now().timestamp();
    if rules.min_account_days > 0 {
        let age_days = (now - member.user.created_at().timestamp()) / 86_400;
        if age_days < rules.min_account_days {
            return Err(format!(
                "your Discord account must be at least **{}** day(s) old",
                rules.min_account_days
            ));
        }
    }
    if rules.min_server_days > 0 {
        let joined_days = member
            .joined_at
            .map(|joined| (now - joined.timestamp()) / 86_400)
            .unwrap_or(0);
        if joined_days < rules.min_server_days {
            return Err(format!(
                "you must have been in this server for at least **{}** day(s)",
                rules.min_server_days
            ));
        }
    }
    if rules.min_level > 0 {
        let xp =
            crate::db::leveling::get_user(&data.db, member.guild_id.get(), member.user.id.get())
                .await
                .ok()
                .map(|row| row.xp)
                .unwrap_or(0);
        let level = crate::leveling::level_of(xp) as i64;
        if level < rules.min_level {
            return Err(format!(
                "you need **level {}** (you're level {})",
                rules.min_level, level
            ));
        }
    }
    Ok(())
}

/// Ticket weight for an entry (mirrors the legacy entryWeight()).
pub async fn weight_of(
    data: &crate::state::Data,
    giveaway: &db::giveaways::Giveaway,
    rules: &db::giveaways::GiveawayRules,
    member: Option<&serenity::Member>,
    user_id: u64,
) -> i64 {
    let mut weight = 1i64;
    if let Some(member) = member {
        if rules.role_bonus > 0 {
            let bonus = rules
                .bonus_role_ids
                .iter()
                .filter(|id| member.roles.iter().any(|r| r.get() == **id))
                .count() as i64;
            weight += bonus * rules.role_bonus;
        }
        if rules.booster_bonus > 0 && member.premium_since.is_some() {
            weight += rules.booster_bonus;
        }
    }
    if rules.level_step > 0 {
        let xp = crate::db::leveling::get_user(
            &data.db,
            giveaway.guild_id.parse::<u64>().unwrap_or(0),
            user_id,
        )
        .await
        .ok()
        .map(|row| row.xp)
        .unwrap_or(0);
        weight += crate::leveling::level_of(xp) as i64 / rules.level_step;
    }
    weight.clamp(1, rules.max_weight.max(1))
}

/// JS `drawWinners` bulk-fetches entrants in chunks of 100. Discord's REST
/// API only exposes paginated guild-member fetches, so Rust walks 1,000-member
/// pages once and keeps only the entrants, bounded by a runaway-safety cap.
async fn fetch_entrants(
    ctx: &serenity::Context,
    guild_id: u64,
    entrants: &[u64],
) -> std::collections::HashMap<u64, serenity::Member> {
    let mut members = std::collections::HashMap::new();
    if let Some(guild) = ctx.cache.guild(serenity::GuildId::new(guild_id)) {
        for id in entrants {
            if let Some(member) = guild.members.get(&serenity::UserId::new(*id)) {
                members.insert(*id, member.clone());
            }
        }
    }
    if members.len() == entrants.len() || entrants.is_empty() {
        return members;
    }
    let mut after = None;
    for _ in 0..200 {
        let page = match ctx
            .http
            .get_guild_members(serenity::GuildId::new(guild_id), Some(1000), after)
            .await
        {
            Ok(page) => page,
            Err(error) => {
                tracing::debug!(%error, guild_id, "giveaway entrant fetch stopped");
                break;
            }
        };
        if page.is_empty() {
            break;
        }
        let last = page.last().map(|member| member.user.id.get());
        let page_len = page.len();
        for member in page {
            let user_id = member.user.id.get();
            if entrants.contains(&user_id) {
                members.insert(user_id, member);
            }
        }
        after = last;
        if members.len() == entrants.len() || page_len < 1000 {
            break;
        }
    }
    members
}

/// End an active giveaway: flip status, draw winners, announce.
/// Safe to call twice — the DB claim decides who proceeds.
pub async fn end_giveaway(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    giveaway: &db::giveaways::Giveaway,
) -> anyhow::Result<Vec<u64>> {
    if !giveaway.is_active() {
        return Ok(giveaway.winners());
    }
    if !db::giveaways::claim_end(&data.db, giveaway.id).await? {
        return Ok(giveaway.winners());
    }

    match end_claimed_giveaway(ctx, data, giveaway).await {
        Ok(winners) => Ok(winners),
        Err(err) => {
            // Nothing was announced yet: hand the giveaway back so the sweep retries it.
            let _ = db::giveaways::reopen(&data.db, giveaway.id).await;
            Err(err)
        }
    }
}

/// Cancel an active giveaway without drawing winners, matching the legacy
/// manager action. The public post is frozen and scheduled for cleanup.
pub async fn cancel_giveaway(
    ctx: &serenity::Context,
    data: &Data,
    giveaway: &db::giveaways::Giveaway,
) -> anyhow::Result<bool> {
    if !giveaway.is_active() || !db::giveaways::claim_cancel(&data.db, giveaway.id).await? {
        return Ok(false);
    }
    let Some(channel_id) = giveaway.channel_id() else {
        return Ok(true);
    };
    if let Some(message_id) = giveaway.message_id() {
        if let Ok(mut message) = serenity::ChannelId::new(channel_id)
            .message(&ctx.http, serenity::MessageId::new(message_id))
            .await
        {
            let embed = cancelled_embed(giveaway);
            let _ = message
                .edit(
                    &ctx.http,
                    serenity::EditMessage::new()
                        .embed(embed)
                        .components(Vec::new()),
                )
                .await;
            let guild_id = giveaway.guild_id.parse::<u64>().unwrap_or_default();
            let _ = db::welcome::record_giveaway_cleanup_message(
                &data.db, guild_id, channel_id, message_id,
            )
            .await;
        }
    }
    Ok(true)
}

/// Draw replacement winners for an ended giveaway, excluding previous winners
/// and members who no longer satisfy its requirements.
pub async fn reroll_giveaway(
    ctx: &serenity::Context,
    data: &Data,
    giveaway: &db::giveaways::Giveaway,
) -> anyhow::Result<Vec<u64>> {
    if giveaway.status != "ended" {
        anyhow::bail!("only ended giveaways can be rerolled");
    }
    let guild_id = giveaway.guild_id.parse::<u64>().unwrap_or(0);
    if guild_id == 0 {
        anyhow::bail!("giveaway has an invalid server ID");
    }
    let entries = db::giveaways::list_entries(&data.db, giveaway.id).await?;
    let rules = giveaway.rules();
    let mut excluded = giveaway.winners();
    let candidates: Vec<u64> = entries
        .iter()
        .copied()
        .filter(|id| !excluded.contains(id))
        .collect();
    let members = fetch_entrants(ctx, guild_id, &candidates).await;
    let mut weights = std::collections::HashMap::with_capacity(entries.len());
    for user_id in candidates {
        let Some(member) = members.get(&user_id) else {
            excluded.push(user_id);
            continue;
        };
        if check_eligibility(data, giveaway, member).await.is_err() {
            excluded.push(user_id);
            continue;
        }
        weights.insert(
            user_id,
            weight_of(data, giveaway, &rules, Some(member), user_id).await,
        );
    }
    let winners = if rules.has_bonuses() {
        db::giveaways::draw_winners_weighted(&data.db, giveaway, &excluded, |id| {
            weights.get(&id).copied().unwrap_or(1)
        })
        .await?
    } else {
        db::giveaways::draw_winners(&data.db, giveaway, &excluded).await?
    };
    if winners.is_empty() {
        return Ok(winners);
    }
    db::giveaways::set_winners(&data.db, giveaway.id, &winners).await?;
    let Some(channel_id) = giveaway.channel_id() else {
        return Ok(winners);
    };
    let mentions = winners
        .iter()
        .map(|id| format!("<@{id}>"))
        .collect::<Vec<_>>()
        .join(", ");
    // JS reroll refreshes the frozen giveaway post with the new winners too.
    if let Some(message_id) = giveaway.message_id() {
        if let Ok(mut original) = serenity::ChannelId::new(channel_id)
            .message(&ctx.http, serenity::MessageId::new(message_id))
            .await
        {
            let entries = db::giveaways::count_entries(&data.db, giveaway.id)
                .await
                .unwrap_or(0);
            let ended = ended_embed(giveaway, entries, &winners);
            let _ = original
                .edit(&ctx.http, serenity::EditMessage::new().embed(ended))
                .await;
        }
    }
    let message = serenity::ChannelId::new(channel_id)
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .content(format!(
                    "🔁 New {} for **{}**: {mentions} — congratulations!",
                    if winners.len() == 1 {
                        "winner"
                    } else {
                        "winners"
                    },
                    giveaway.prize
                ))
                .allowed_mentions(
                    serenity::CreateAllowedMentions::new().users(
                        winners
                            .iter()
                            .map(|id| serenity::UserId::new(*id))
                            .collect::<Vec<_>>(),
                    ),
                ),
        )
        .await?;
    let _ = db::welcome::record_giveaway_cleanup_message(
        &data.db,
        guild_id,
        channel_id,
        message.id.get(),
    )
    .await;
    Ok(winners)
}

async fn end_claimed_giveaway(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    giveaway: &db::giveaways::Giveaway,
) -> anyhow::Result<Vec<u64>> {
    let rules = giveaway.rules();
    let entries = db::giveaways::list_entries(&data.db, giveaway.id).await?;
    let guild_id = giveaway.guild_id.parse::<u64>().unwrap_or(0);

    // JS re-checks every entrant at draw time: bots, members who left, lost the
    // required role, or no longer meet the advanced rules are all excluded.
    // Do this for unweighted draws too — otherwise invalid entries can win.
    let mut weights = std::collections::HashMap::with_capacity(entries.len());
    let mut ineligible = Vec::new();
    let members = fetch_entrants(ctx, guild_id, &entries).await;
    for entry in &entries {
        let Some(member) = members.get(entry) else {
            ineligible.push(*entry);
            continue;
        };
        if check_eligibility(data, giveaway, member).await.is_err() {
            ineligible.push(*entry);
            continue;
        }
        weights.insert(
            *entry,
            weight_of(data, giveaway, &rules, Some(member), *entry).await,
        );
    }

    let winners = if rules.has_bonuses() {
        db::giveaways::draw_winners_weighted(&data.db, giveaway, &ineligible, |uid| {
            weights.get(&uid).copied().unwrap_or(1)
        })
        .await?
    } else {
        db::giveaways::draw_winners(&data.db, giveaway, &ineligible).await?
    };
    db::giveaways::set_winners(&data.db, giveaway.id, &winners).await?;
    let entries = db::giveaways::count_entries(&data.db, giveaway.id).await?;

    // Freeze the message and announce in-channel.
    if let (Some(channel_id), Some(message_id)) = (giveaway.channel_id(), giveaway.message_id()) {
        if let Ok(mut msg) = serenity::ChannelId::new(channel_id)
            .message(&ctx.http, serenity::MessageId::new(message_id))
            .await
        {
            let embed = ended_embed(giveaway, entries, &winners);
            let _ = msg
                .edit(
                    &ctx.http,
                    serenity::EditMessage::new()
                        .embed(embed)
                        .components(Vec::new()),
                )
                .await;
            let guild_id = giveaway.guild_id.parse::<u64>().unwrap_or_default();
            let _ = db::welcome::record_giveaway_cleanup_message(
                &data.db, guild_id, channel_id, message_id,
            )
            .await;
        }

        let announcement = if winners.is_empty() {
            format!(
                "😕 Nobody entered **{}** (or no valid entries were left), so there's no winner this time.",
                giveaway.prize
            )
        } else {
            let mentions = winners
                .iter()
                .map(|id| format!("<@{id}>"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "🎉 Congratulations {mentions}! You won **{}**!",
                giveaway.prize
            )
        };
        let announcement = serenity::ChannelId::new(channel_id)
            .send_message(
                &ctx.http,
                serenity::CreateMessage::new()
                    .content(announcement)
                    .allowed_mentions(
                        serenity::CreateAllowedMentions::new().users(
                            winners
                                .iter()
                                .map(|id| serenity::UserId::new(*id))
                                .collect::<Vec<_>>(),
                        ),
                    ),
            )
            .await;
        if let Ok(announcement) = announcement {
            let guild_id = giveaway.guild_id.parse::<u64>().unwrap_or_default();
            let _ = db::welcome::record_giveaway_cleanup_message(
                &data.db,
                guild_id,
                channel_id,
                announcement.id.get(),
            )
            .await;
        }
    }

    Ok(winners)
}
