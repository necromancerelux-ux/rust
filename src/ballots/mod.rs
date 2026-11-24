//! WoS ballots — multi-question votes ("WoS alliance style").
//!
//! Layout:
//! - `/settings` → **Voting**: channel + results channel + create.
//! - The public vote message has **Vote** (ephemeral wizard), **Participants**
//!   and (if allowed) **Remove my vote**, plus a manage menu for admins.
//! - The `tasks` sweeper closes ballots whose timer ran out and posts the
//!   final results to the configured results channel.

pub mod translate_menu;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use poise::serenity_prelude as serenity;
use serenity::CreateEmbed;

use crate::db::ballots::{self, Ballot, BallotQuestion};
use crate::ui::components;
use crate::ui::theme;

// ------------------------------------------------------------- duration

const MIN_DURATION_MS: i64 = 60_000; // 1 minute
const MAX_DURATION_MS: i64 = 60 * 86_400_000; // 60 days

/// Parse `12h`, `2d`, `1w`, `90` (minutes), `1d 12h` into milliseconds.
pub fn parse_duration(raw: &str) -> Option<i64> {
    let text = raw.trim().to_ascii_lowercase();
    if text.is_empty() {
        return None;
    }
    if text.chars().all(|c| c.is_ascii_digit()) {
        let minutes: i64 = text.parse().ok()?;
        return (minutes > 0).then_some(minutes * 60_000);
    }

    let re = regex::Regex::new(
        r"(\d+(?:\.\d+)?)\s*(seconds?|secs?|minutes?|mins?|hours?|hrs?|days?|weeks?|[smhdw])",
    )
    .ok()?;
    let mut total = 0.0f64;
    let mut found = false;
    for cap in re.captures_iter(&text) {
        let value = cap.get(1)?.as_str().parse::<f64>().ok()?;
        let unit = cap.get(2)?.as_str();
        let multiplier = match unit {
            "s" | "sec" | "secs" | "second" | "seconds" => 1_000.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60_000.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000.0,
            "d" | "day" | "days" => 86_400_000.0,
            "w" | "week" | "weeks" => 604_800_000.0,
            _ => return None,
        };
        total += value * multiplier;
        found = true;
    }
    if !found {
        return None;
    }
    let total = total as i64;
    (MIN_DURATION_MS..=MAX_DURATION_MS)
        .contains(&total)
        .then_some(total)
}

// --------------------------------------------------------- vote sessions

const VOTE_SESSION_TTL_MS: u128 = 15 * 60 * 1000;

/// Pending picks per (ballot, user) while the voter clicks through the wizard.
#[derive(Debug, Clone, Default)]
struct VoteDraft {
    answers: Vec<Vec<usize>>,
    touched_ms: u128,
}

/// Draft ballot being composed by an admin (the create wizard).
#[derive(Debug, Clone, Default)]
pub struct BallotDraft {
    pub guild_id: u64,
    pub title: String,
    pub description: String,
    /// None = no end; Some(ms since epoch) = close time.
    pub ends_at: Option<i64>,
    pub questions: Vec<BallotQuestion>,
    pub role_id: Option<u64>,
    pub show_votes: bool,
    pub allow_change: bool,
    pub touched_ms: u128,
}

const DRAFT_TTL_MS: u128 = 15 * 60 * 1000;

fn drafts() -> &'static Mutex<HashMap<u64, BallotDraft>> {
    static DRAFTS: OnceLock<Mutex<HashMap<u64, BallotDraft>>> = OnceLock::new();
    DRAFTS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn draft_get(user_id: u64) -> Option<BallotDraft> {
    let now = crate::calculator::svs::now_ms();
    let mut guard = drafts().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, draft| now.saturating_sub(draft.touched_ms) < DRAFT_TTL_MS);
    guard.get(&user_id).cloned()
}

pub fn draft_set<F>(user_id: u64, update: F) -> Option<BallotDraft>
where
    F: FnOnce(&mut BallotDraft),
{
    let now = crate::calculator::svs::now_ms();
    let mut guard = drafts().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, draft| now.saturating_sub(draft.touched_ms) < DRAFT_TTL_MS);
    let draft = guard.entry(user_id).or_default();
    draft.touched_ms = now;
    update(draft);
    Some(draft.clone())
}

pub fn draft_new(
    user_id: u64,
    guild_id: u64,
    title: String,
    description: String,
    ends_at: Option<i64>,
) {
    let now = crate::calculator::svs::now_ms();
    let mut guard = drafts().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, draft| now.saturating_sub(draft.touched_ms) < DRAFT_TTL_MS);
    guard.insert(
        user_id,
        BallotDraft {
            guild_id,
            title,
            description,
            ends_at,
            questions: Vec::new(),
            role_id: None,
            show_votes: true,
            allow_change: true,
            touched_ms: now,
        },
    );
}

pub fn draft_clear(user_id: u64) {
    drafts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&user_id);
}

fn vote_sessions() -> &'static Mutex<HashMap<(i64, u64), VoteDraft>> {
    static SESSIONS: OnceLock<Mutex<HashMap<(i64, u64), VoteDraft>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn vote_session(ballot_id: i64, user_id: u64, questions: &[BallotQuestion]) -> Vec<Vec<usize>> {
    let now = crate::calculator::svs::now_ms();
    let mut guard = vote_sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < VOTE_SESSION_TTL_MS);
    let session = guard
        .entry((ballot_id, user_id))
        .or_insert_with(|| VoteDraft {
            answers: vec![Vec::new(); questions.len()],
            touched_ms: now,
        });
    session.touched_ms = now;
    if session.answers.len() != questions.len() {
        session.answers.resize(questions.len(), Vec::new());
    }
    session.answers.clone()
}

pub fn vote_session_set(
    ballot_id: i64,
    user_id: u64,
    questions: &[BallotQuestion],
    question_index: usize,
    picks: Vec<usize>,
) -> Vec<Vec<usize>> {
    let now = crate::calculator::svs::now_ms();
    let mut guard = vote_sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < VOTE_SESSION_TTL_MS);
    let session = guard
        .entry((ballot_id, user_id))
        .or_insert_with(|| VoteDraft {
            answers: vec![Vec::new(); questions.len()],
            touched_ms: now,
        });
    session.touched_ms = now;
    if session.answers.len() != questions.len() {
        session.answers.resize(questions.len(), Vec::new());
    }
    if let Some(slot) = session.answers.get_mut(question_index) {
        *slot = picks;
    }
    session.answers.clone()
}

pub fn vote_session_clear(ballot_id: i64, user_id: u64) {
    vote_sessions()
        .lock()
        .unwrap()
        .remove(&(ballot_id, user_id));
}

// ---------------------------------------------------------------- render

fn bar(fraction: f64, width: usize) -> String {
    let filled = (fraction.clamp(0.0, 1.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn pct(n: i64, base: i64) -> i64 {
    if base > 0 {
        (n * 100 + base / 2) / base
    } else {
        0
    }
}

fn pick_text(question: &BallotQuestion) -> String {
    if question.max_picks <= 1 {
        "pick 1".to_string()
    } else if question.max_picks >= question.options.len() as i64 {
        "pick any".to_string()
    } else {
        format!("pick up to {}", question.max_picks)
    }
}

pub(crate) fn leaders(counts: &[i64]) -> Vec<usize> {
    let max = counts.iter().copied().max().unwrap_or(0);
    if max > 0 {
        counts
            .iter()
            .enumerate()
            .filter(|(_, n)| **n == max)
            .map(|(i, _)| i)
            .collect()
    } else {
        Vec::new()
    }
}

/// The rules footer line (public/private, change allowed, role restriction).
fn rules_line(ballot: &Ballot) -> String {
    let mut out = String::new();
    if ballot.show_votes {
        out.push_str("🗳️ Votes are public");
    } else {
        out.push_str("🕶️ Votes visible to admins only");
    }
    if !ballot.allow_change {
        out.push_str(" · 🔒 Votes are final");
    }
    match ballot.expected_role_id() {
        Some(id) => out.push_str(&format!(" · <@&{id}> only")),
        None => out.push_str(" · Everyone"),
    }
    out
}

pub struct BallotView<'a> {
    pub ballot: &'a Ballot,
    pub counts: Vec<Vec<i64>>,
    pub voters: i64,
    pub expected: Option<usize>,
}

pub fn ballot_embed(view: &BallotView<'_>) -> CreateEmbed {
    let closed = !view.ballot.is_active();
    let status = if closed {
        "🔒 **Voting closed — final results**".to_string()
    } else {
        match view.ballot.ends_at {
            Some(ends) => {
                let ts = ends / 1000;
                format!("⏳ Closes {}", crate::ui::theme::utc_with_relative(ts))
            }
            None => "⏳ Open until an admin closes it".to_string(),
        }
    };

    let expected_text = match view.expected {
        Some(total) if total > 0 => {
            let total = total.max(view.voters as usize);
            format!(
                "🗳️ **{}** of **{}** voted ({}%)  ·  ⏳ **{}** still to vote",
                view.voters,
                total,
                pct(view.voters, total as i64),
                total as i64 - view.voters
            )
        }
        _ => format!("🗳️ **{}** voted", view.voters),
    };

    let mut head = vec![status, rules_line(view.ballot)];
    if !view.ballot.description.is_empty() {
        head.push(String::new());
        head.push(view.ballot.description.chars().take(600).collect());
    }
    head.push(String::new());
    head.push(expected_text);
    if !closed {
        head.push("Press **Vote** to answer, **Participants** to see who has voted.".to_string());
    }

    let mut embed = CreateEmbed::new()
        .title(format!(
            "{} {}",
            if closed { "🔒" } else { "🗳️" },
            crate::error::truncate(&view.ballot.title, 240)
        ))
        .description(crate::error::truncate(&head.join("\n"), 4000))
        .color(if closed {
            0x95a5a6
        } else {
            crate::ui::theme::colors::BRAND
        })
        .footer(serenity::CreateEmbedFooter::new(format!(
            "Ballot #{} · {} question{} · {}",
            view.ballot.id,
            view.ballot.questions.len(),
            if view.ballot.questions.len() == 1 {
                ""
            } else {
                "s"
            },
            theme::footer()
        )));

    for (qi, question) in view.ballot.questions.iter().enumerate() {
        let counts = view.counts.get(qi).cloned().unwrap_or_default();
        let win = if closed { leaders(&counts) } else { Vec::new() };
        let lines: Vec<String> = question
            .options
            .iter()
            .enumerate()
            .map(|(oi, label)| {
                let n = counts.get(oi).copied().unwrap_or(0);
                let mark = if win.contains(&oi) { "🏆" } else { "▫️" };
                format!(
                    "{mark} `{}` **{}%** · {} — {}",
                    bar(
                        if view.voters > 0 {
                            n as f64 / view.voters as f64
                        } else {
                            0.0
                        },
                        8
                    ),
                    pct(n, view.voters),
                    n,
                    crate::error::truncate(label, 60)
                )
            })
            .collect();
        embed = embed.field(
            crate::error::truncate(
                &format!("Q{} — {}  ({})", qi + 1, question.text, pick_text(question)),
                256,
            ),
            crate::error::truncate(&lines.join("\n"), 1024),
            false,
        );
    }

    embed
}

/// Row layout for the ballot message. `message_id` is present once a real
/// message exists — that's when the Translate button becomes available.
pub fn ballot_rows(ballot: &Ballot, message_id: Option<u64>) -> Vec<serenity::CreateActionRow> {
    let mut buttons: Vec<serenity::CreateButton> = Vec::new();
    if ballot.is_active() {
        buttons.push(components::success(
            format!("ballot:vote:{}", ballot.id),
            "🗳️ Vote",
        ));
    }
    buttons.push(components::primary(
        format!("ballot:parts:{}", ballot.id),
        "👥 Participants",
    ));
    if ballot.is_active() && ballot.allow_change {
        buttons.push(components::secondary(
            format!("ballot:clear:{}", ballot.id),
            "↩️ Remove my vote",
        ));
    }

    let mut admin_buttons = vec![components::secondary(
        format!("ballot:manage:{}", ballot.id),
        format!("{} Manage", crate::ui::theme::icons::SETTINGS),
    )];
    if let Some(_message_id) = message_id {
        admin_buttons.push(components::secondary(
            format!("ballot:tr:{}", ballot.id),
            "🌐 Translate",
        ));
    }

    vec![
        serenity::CreateActionRow::Buttons(buttons),
        serenity::CreateActionRow::Buttons(admin_buttons),
    ]
}

/// Count expected voters from cache (non-bot members, or members with the
/// ballot's role). Best effort — cache may be partial.
pub fn expected_count(ctx: &serenity::Context, guild_id: u64, ballot: &Ballot) -> Option<usize> {
    let guild = ctx.cache.guild(serenity::GuildId::new(guild_id))?;
    let count = match ballot.expected_role_id() {
        Some(role_id) => guild
            .members
            .values()
            .filter(|member| !member.user.bot && member.roles.iter().any(|id| id.get() == role_id))
            .count(),
        None => guild
            .members
            .values()
            .filter(|member| !member.user.bot)
            .count(),
    };
    Some(count)
}

/// Fetch the full expected-voter roster via paginated gateway REST, without
/// the old 5,000-member cache cap. Bounded only by a runaway-safety page cap.
pub async fn fetch_expected_ids(
    ctx: &serenity::Context,
    guild_id: u64,
    ballot: &crate::db::ballots::Ballot,
) -> Vec<u64> {
    let mut ids = Vec::new();
    let mut after: Option<u64> = None;
    for _ in 0..200 {
        let members = match ctx
            .http
            .get_guild_members(serenity::GuildId::new(guild_id), Some(1000), after)
            .await
        {
            Ok(members) => members,
            Err(error) => {
                tracing::debug!(%error, guild_id, "ballot roster fetch stopped");
                break;
            }
        };
        if members.is_empty() {
            break;
        }
        for member in &members {
            if member.user.bot {
                continue;
            }
            if let Some(role_id) = ballot.expected_role_id() {
                if !member.roles.iter().any(|id| id.get() == role_id) {
                    continue;
                }
            }
            ids.push(member.user.id.get());
        }
        after = members.last().map(|member| member.user.id.get());
        if members.len() < 1000 {
            break;
        }
    }
    ids
}

/// Expected member ids (cache-based, capped) for the Participants page.
pub fn expected_ids(
    ctx: &serenity::Context,
    guild_id: u64,
    ballot: &Ballot,
    cap: usize,
) -> Vec<u64> {
    let Some(guild) = ctx.cache.guild(serenity::GuildId::new(guild_id)) else {
        return Vec::new();
    };
    guild
        .members
        .values()
        .filter(|member| {
            if member.user.bot {
                return false;
            }
            match ballot.expected_role_id() {
                Some(role_id) => member.roles.iter().any(|id| id.get() == role_id),
                None => true,
            }
        })
        .take(cap)
        .map(|member| member.user.id.get())
        .collect()
}

/// Display name for a user id (server nickname > username > id).
pub fn name_of(ctx: &serenity::Context, guild_id: u64, user_id: u64) -> String {
    ctx.cache
        .guild(serenity::GuildId::new(guild_id))
        .and_then(|guild| {
            guild
                .members
                .get(&serenity::UserId::new(user_id))
                .map(|m| m.display_name().to_string())
        })
        .unwrap_or_else(|| format!("User {user_id}"))
}

/// The stored-embed snapshot used for the Translate button.
pub fn stored_embed(view: &BallotView<'_>) -> crate::translate::StoredEmbed {
    let closed = !view.ballot.is_active();
    let mut fields = Vec::new();
    for (qi, question) in view.ballot.questions.iter().enumerate() {
        let counts = view.counts.get(qi).cloned().unwrap_or_default();
        let lines: Vec<String> = question
            .options
            .iter()
            .enumerate()
            .map(|(oi, label)| {
                let n = counts.get(oi).copied().unwrap_or(0);
                format!(
                    "**{pct}%** · {} — {label}",
                    pct(n, view.voters),
                    pct = pct(n, view.voters)
                )
            })
            .collect();
        fields.push(crate::translate::StoredField {
            name: format!("Q{} — {}", qi + 1, question.text),
            value: lines.join("\n"),
            inline: false,
        });
    }

    crate::translate::StoredEmbed {
        title: Some(format!("🗳️ {}", view.ballot.title)),
        description: Some(format!(
            "{}\n{}",
            rules_line(view.ballot),
            if view.ballot.description.is_empty() {
                String::new()
            } else {
                format!("\n{}", view.ballot.description)
            }
        )),
        fields,
        footer: None,
        color: Some(if closed {
            0x95a5a6
        } else {
            crate::ui::theme::colors::BRAND
        }),
    }
}

/// Post a ballot publicly and register it for Translate + message tracking.
pub async fn post_ballot(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    ballot_id: i64,
    channel_id: u64,
) -> anyhow::Result<u64> {
    let ballot = ballots::get(&data.db, ballot_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no ballot with id {ballot_id}"))?;
    let (counts, voters) = tally_view(&data.db, &ballot).await?;
    let expected = expected_count(ctx, ballot.guild_id.parse().unwrap_or(0), &ballot);
    let view = BallotView {
        ballot: &ballot,
        counts,
        voters,
        expected,
    };
    let embed = ballot_embed(&view);
    let rows = ballot_rows(&ballot, None);
    let target = serenity::ChannelId::new(channel_id);
    let message = target
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new().embed(embed).components(rows),
        )
        .await?;
    ballots::set_message_id(&data.db, ballot.id, message.id.get()).await?;
    rebuild_ballot_message(ctx, data, ballot_id).await?;
    Ok(message.id.get())
}

/// Re-render the public message (counts change as votes come in) and refresh
/// the Translate snapshot.
pub async fn rebuild_ballot_message(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    ballot_id: i64,
) -> anyhow::Result<()> {
    let Some(ballot) = ballots::get(&data.db, ballot_id).await? else {
        return Ok(());
    };
    let Some(channel_id) = ballot.channel_id() else {
        return Ok(());
    };
    let Some(message_id) = ballot.message_id() else {
        return Ok(());
    };
    let (counts, voters) = tally_view(&data.db, &ballot).await?;
    let expected = expected_count(ctx, ballot.guild_id.parse().unwrap_or(0), &ballot);
    let stored = stored_embed(&BallotView {
        ballot: &ballot,
        counts: counts.clone(),
        voters,
        expected,
    });
    let view = BallotView {
        ballot: &ballot,
        counts,
        voters,
        expected,
    };
    let embed = ballot_embed(&view);
    let rows = ballot_rows(&ballot, Some(message_id));

    serenity::ChannelId::new(channel_id)
        .edit_message(
            &ctx.http,
            serenity::MessageId::new(message_id),
            serenity::EditMessage::new().embed(embed).components(rows),
        )
        .await?;

    crate::translate::register(
        &data.db,
        message_id,
        crate::db::translate::NewTranslatable {
            guild_id: ballot.guild_id.parse().ok(),
            channel_id: Some(channel_id),
            kind: "ballot",
            embed_json: serde_json::to_string(&stored).ok(),
            content: None,
            owner_id: None,
        },
    )
    .await?;
    Ok(())
}

/// Build the full tally view for a ballot (counts + voter count).
pub async fn tally_view(
    db: &crate::db::Db,
    ballot: &Ballot,
) -> anyhow::Result<(Vec<Vec<i64>>, i64)> {
    Ok(ballots::tally(db, ballot).await?)
}
