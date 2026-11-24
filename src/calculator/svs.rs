//! State vs State Prep Week planner — port of svsCalculator.js.
//!
//! A KoI-style planner: per day, pick activities and quantities, track day and
//! total points, and compare against an optional target score. Sessions are
//! private to each user and kept in memory for 15 minutes.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use poise::serenity_prelude as serenity;
use serenity::CreateEmbed;

use crate::calculator::game_numbers::format_game_amount;

pub const DAYS: &[(&str, &str, &str, &str, &[&str])] = &[
    (
        "day1",
        "Day 1: Construction",
        "D1 Construction",
        "🏗️",
        &["Construction", "Fire Crystals", "Building speedups"],
    ),
    (
        "day2",
        "Day 2: Research & Experts",
        "D2 Research",
        "🔎",
        &["Research", "Lucky Wheel", "Gather", "Experts"],
    ),
    (
        "day3",
        "Day 3: Heroes & Beasts",
        "D3 Heroes",
        "🦸",
        &["Lucky Wheel", "Hero Shards", "Beasts", "Polar Terror"],
    ),
    (
        "day4",
        "Day 4: Charms & Troops",
        "D4 Charms/Troops",
        "⚔️",
        &["Chief Charms", "Train/Promote Troops", "Troop speedups"],
    ),
    (
        "day5",
        "Day 5: Gear & Pets",
        "D5 Gear/Pets",
        "🐾",
        &["Chief Gear", "Pets", "Hero Gear"],
    ),
];

#[derive(Debug, Clone)]
pub struct Rate {
    pub label: &'static str,
    pub unit: &'static str,
    pub points: i64,
    pub manual: bool,
}

pub const RATES: &[(&str, Rate)] = &[
    (
        "rfc_building",
        Rate {
            label: "Use 1 Refined Fire Crystal to upgrade buildings",
            unit: "RFC",
            points: 30_000,
            manual: false,
        },
    ),
    (
        "fc_building",
        Rate {
            label: "Use 1 Fire Crystal to upgrade buildings",
            unit: "crystals",
            points: 2_000,
            manual: false,
        },
    ),
    (
        "fc_shards_research",
        Rate {
            label: "Use 1 Fire Crystal Shard for Research",
            unit: "shards",
            points: 1_000,
            manual: false,
        },
    ),
    (
        "charm_score",
        Rate {
            label: "Raise Chief Charm Score by 1",
            unit: "score",
            points: 70,
            manual: false,
        },
    ),
    (
        "gear_score",
        Rate {
            label: "Raise Chief Gear Score by 1",
            unit: "score",
            points: 36,
            manual: false,
        },
    ),
    (
        "speedup_construction",
        Rate {
            label: "Use 1m of Construction Speedup",
            unit: "minutes",
            points: 30,
            manual: false,
        },
    ),
    (
        "speedup_research",
        Rate {
            label: "Use 1m of Research Speedup",
            unit: "minutes",
            points: 30,
            manual: false,
        },
    ),
    (
        "speedup_troops",
        Rate {
            label: "Use 1m of Training/Promotion Speedup",
            unit: "minutes",
            points: 30,
            manual: false,
        },
    ),
    (
        "speedup_learning",
        Rate {
            label: "Use 1m of Speedup to Learn Expert Skills",
            unit: "minutes",
            points: 30,
            manual: false,
        },
    ),
    (
        "lucky_wheel",
        Rate {
            label: "Play Lucky Wheel 1 time",
            unit: "spins",
            points: 8_000,
            manual: false,
        },
    ),
    (
        "hero_shards_rare",
        Rate {
            label: "Use 1 Rare Hero Shard to ascend Heroes",
            unit: "shards",
            points: 350,
            manual: false,
        },
    ),
    (
        "hero_shards_epic",
        Rate {
            label: "Use 1 Epic Hero Shard to ascend Heroes",
            unit: "shards",
            points: 1_220,
            manual: false,
        },
    ),
    (
        "hero_shards_mythic",
        Rate {
            label: "Use 1 Mythic Hero Shard to ascend Heroes",
            unit: "shards",
            points: 3_040,
            manual: false,
        },
    ),
    (
        "gather_meat",
        Rate {
            label: "Gather 1,000 Meat in the Wilderness",
            unit: "per-1000",
            points: 2,
            manual: false,
        },
    ),
    (
        "gather_wood",
        Rate {
            label: "Gather 1,000 Wood in the Wilderness",
            unit: "per-1000",
            points: 2,
            manual: false,
        },
    ),
    (
        "gather_coal",
        Rate {
            label: "Gather 200 Coal in the Wilderness",
            unit: "per-200",
            points: 2,
            manual: false,
        },
    ),
    (
        "gather_iron",
        Rate {
            label: "Gather 50 Iron in the Wilderness",
            unit: "per-50",
            points: 2,
            manual: false,
        },
    ),
    (
        "pet_common_marks",
        Rate {
            label: "Use 1 Common Wild Mark to refine pets",
            unit: "marks",
            points: 1_150,
            manual: false,
        },
    ),
    (
        "pet_advanced_marks",
        Rate {
            label: "Use 1 Advanced Wild Mark to refine pets",
            unit: "marks",
            points: 15_000,
            manual: false,
        },
    ),
    (
        "pet_advance_score",
        Rate {
            label: "Increase Pet Advancement Score by 1",
            unit: "score",
            points: 50,
            manual: false,
        },
    ),
    (
        "polar_terror",
        Rate {
            label: "Call 1 Rally against Polar Terror",
            unit: "rallies",
            points: 30_000,
            manual: false,
        },
    ),
    (
        "beasts_1_25",
        Rate {
            label: "Slay 1 Beast Level 1–25",
            unit: "beasts",
            points: 11_250,
            manual: false,
        },
    ),
    (
        "beasts_26_30",
        Rate {
            label: "Slay 1 Beast Level 26–30",
            unit: "beasts",
            points: 12_000,
            manual: false,
        },
    ),
    (
        "essence_stones",
        Rate {
            label: "Use 1 Essence Stone to upgrade Hero Gear",
            unit: "stones",
            points: 4_000,
            manual: false,
        },
    ),
    (
        "widgets",
        Rate {
            label: "Use 1 Widget of any Hero Exclusive Gear",
            unit: "widgets",
            points: 8_000,
            manual: false,
        },
    ),
    (
        "mithril",
        Rate {
            label: "Use 1 Mithril to upgrade Hero Gear",
            unit: "mithril",
            points: 144,
            manual: false,
        },
    ),
    (
        "book_of_knowledge",
        Rate {
            label: "Use 1 Book of Knowledge",
            unit: "books",
            points: 60,
            manual: false,
        },
    ),
    (
        "expert_sigil",
        Rate {
            label: "Use 1 Expert Sigil (excluding Common)",
            unit: "sigils",
            points: 6_000,
            manual: false,
        },
    ),
    (
        "troops_train",
        Rate {
            label: "Train/Promote Troops (enter total points)",
            unit: "points",
            points: 1,
            manual: true,
        },
    ),
];

pub fn rate(key: &str) -> Option<&'static Rate> {
    RATES.iter().find(|(k, _)| *k == key).map(|(_, r)| r)
}

pub fn day(
    key: &str,
) -> &'static (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
) {
    DAYS.iter()
        .find(|(k, _, _, _, _)| *k == key)
        .unwrap_or(&DAYS[0])
}

/// `(activity key, priority)` per day, in display order.
pub const DAY_ACTIVITIES: &[(&str, &[(&str, &str)])] = &[
    (
        "day1",
        &[
            ("rfc_building", "high"),
            ("fc_building", "high"),
            ("fc_shards_research", "high"),
            ("charm_score", "high"),
            ("speedup_construction", "high"),
            ("speedup_research", "high"),
            ("speedup_troops", "low"),
            ("speedup_learning", "low"),
        ],
    ),
    (
        "day2",
        &[
            ("lucky_wheel", "high"),
            ("fc_shards_research", "high"),
            ("speedup_research", "high"),
            ("speedup_learning", "high"),
            ("book_of_knowledge", "high"),
            ("expert_sigil", "high"),
            ("gather_meat", "medium"),
            ("gather_wood", "medium"),
            ("gather_coal", "medium"),
            ("gather_iron", "medium"),
            ("speedup_construction", "low"),
            ("speedup_troops", "low"),
        ],
    ),
    (
        "day3",
        &[
            ("lucky_wheel", "high"),
            ("hero_shards_mythic", "high"),
            ("hero_shards_epic", "high"),
            ("hero_shards_rare", "medium"),
            ("polar_terror", "high"),
            ("beasts_26_30", "high"),
            ("beasts_1_25", "medium"),
        ],
    ),
    (
        "day4",
        &[
            ("charm_score", "high"),
            ("troops_train", "high"),
            ("speedup_troops", "high"),
            ("speedup_construction", "low"),
            ("speedup_research", "low"),
        ],
    ),
    (
        "day5",
        &[
            ("gear_score", "high"),
            ("pet_advanced_marks", "high"),
            ("pet_common_marks", "medium"),
            ("pet_advance_score", "high"),
            ("essence_stones", "high"),
            ("widgets", "high"),
            ("mithril", "medium"),
        ],
    ),
];

pub fn day_activities(day_key: &str) -> &'static [(&'static str, &'static str)] {
    DAY_ACTIVITIES
        .iter()
        .find(|(day, _)| *day == day_key)
        .map(|(_, acts)| *acts)
        .unwrap_or(&[])
}

pub const STRATEGY_TIPS: &[(&str, &[&str])] = &[
    (
        "day1",
        &[
            "Save building upgrades that need Fire Crystals / RFC for this day.",
            "Refined Fire Crystals = 30,000 pts each — huge if you have FC6+ furnace.",
            "Construction + Research speedups are High value (30 pts/min).",
            "Training / Expert skill speedups are Low value today — save them.",
            "Chief Charm score (+70 each) is High value on Construction day.",
            "Wait for presidential Construction buff if your state runs one.",
        ],
    ),
    (
        "day2",
        &[
            "Best day for Lucky Wheel spins (8,000 pts) with Research items.",
            "FC Shards + Research speedups + Books of Knowledge + Expert Sigils stack well.",
            "Learning speedups score High — pair with Expert Sigils.",
            "Gather resources for easy secondary points.",
        ],
    ),
    (
        "day3",
        &[
            "Dump Mythic / Epic hero shards today for max value.",
            "Lucky Wheel still scores — use remaining spins.",
            "Polar Terror rallies = 30,000 pts each.",
            "Beast hunts (especially Lv 26–30) are strong secondary points.",
        ],
    ),
    (
        "day4",
        &[
            "Focus Chief Charm score upgrades (+70 each).",
            "Train/Promote troops — enter total points from your troop plan.",
            "Troop speedups are High value today.",
            "Save gear / pet items for Day 5.",
        ],
    ),
    (
        "day5",
        &[
            "Chief Gear score (+36 each) + Hero Gear materials.",
            "Essence Stones (4,000), Widgets (8,000), Mithril score here.",
            "Pet Advancement Score (+50) and Wild Marks score today.",
            "Advanced Wild Marks = 15,000 pts each — high priority.",
            "Burn remaining stock you did not use earlier in the week.",
        ],
    ),
];

pub fn tips_for(day_key: &str) -> &'static [&'static str] {
    STRATEGY_TIPS
        .iter()
        .find(|(day, _)| *day == day_key)
        .map(|(_, tips)| *tips)
        .unwrap_or(&[])
}

pub fn priority_emoji(priority: &str) -> &'static str {
    match priority {
        "high" => "🟢",
        "medium" => "🟡",
        _ => "🔴",
    }
}

/// Per-activity resolved entry for rendering.
#[derive(Debug, Clone)]
pub struct ActivityLine {
    pub key: String,
    pub label: String,
    pub unit: String,
    pub priority: String,
    pub points: i64,
    pub manual: bool,
}

pub fn day_activities_resolved(day_key: &str) -> Vec<ActivityLine> {
    day_activities(day_key)
        .iter()
        .filter_map(|(key, priority)| {
            rate(key).map(|rate| ActivityLine {
                key: key.to_string(),
                label: rate.label.to_string(),
                unit: rate.unit.to_string(),
                priority: priority.to_string(),
                points: rate.points,
                manual: rate.manual,
            })
        })
        .collect()
}

/// Totals for one day's plan.
#[derive(Debug, Clone, Default)]
pub struct DayScore {
    pub total: i64,
    pub lines: Vec<(String, i64, i64)>, // (activity key, quantity, points)
}

pub fn compute_day_points(day_key: &str, day_plan: &HashMap<String, i64>) -> DayScore {
    let mut total = 0i64;
    let mut lines = Vec::new();
    for (key, _) in day_activities(day_key) {
        let quantity = day_plan.get(*key).copied().unwrap_or(0);
        if quantity <= 0 {
            continue;
        }
        let Some(rate) = rate(key) else { continue };
        let points = quantity.saturating_mul(rate.points);
        total = total.saturating_add(points);
        lines.push((key.to_string(), quantity, points));
    }
    lines.sort_by_key(|entry| std::cmp::Reverse(entry.2));
    DayScore { total, lines }
}

#[derive(Debug, Clone, Default)]
pub struct AllScore {
    pub by_day: HashMap<String, DayScore>,
    pub grand: i64,
}

pub fn compute_all(plans: &HashMap<String, HashMap<String, i64>>) -> AllScore {
    let mut by_day = HashMap::new();
    let mut grand = 0i64;
    for (day_key, _, _, _, _) in DAYS {
        let score = commit_day_points(day_key, plans.get(*day_key));
        grand = grand.saturating_add(score.total);
        by_day.insert(day_key.to_string(), score);
    }
    AllScore { by_day, grand }
}

fn commit_day_points(day_key: &&str, day_plan: Option<&HashMap<String, i64>>) -> DayScore {
    match day_plan {
        Some(plan) => compute_day_points(day_key, plan),
        None => DayScore::default(),
    }
}

// ------------------------------------------------------------------ sessions

const SESSION_TTL_MS: u128 = 15 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct SvsSession {
    pub day_key: String,
    pub activity_key: Option<String>,
    pub plans: HashMap<String, HashMap<String, i64>>,
    pub target: i64,
    pub touched_ms: u128,
}

impl Default for SvsSession {
    fn default() -> Self {
        Self {
            day_key: "day1".to_string(),
            activity_key: None,
            plans: DAYS
                .iter()
                .map(|(key, _, _, _, _)| (key.to_string(), HashMap::new()))
                .collect(),
            target: 0,
            touched_ms: now_ms(),
        }
    }
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn sessions() -> &'static Mutex<HashMap<u64, SvsSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<u64, SvsSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn session_for(user_id: u64) -> SvsSession {
    let now = now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    let session = guard.entry(user_id).or_default();
    session.touched_ms = now;
    session.clone()
}

pub fn update_session<F>(user_id: u64, update: F)
where
    F: FnOnce(&mut SvsSession),
{
    let now = now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    let session = guard.entry(user_id).or_default();
    session.touched_ms = now;
    update(session);
}

/// Resolve the activity key for the current day, falling back to the first.
pub fn ensure_activity(session: &SvsSession) -> Option<String> {
    let acts = day_activities(&session.day_key);
    session
        .activity_key
        .as_ref()
        .filter(|key| acts.iter().any(|(k, _)| *k == key.as_str()))
        .cloned()
        .or_else(|| acts.first().map(|(key, _)| key.to_string()))
}

// ------------------------------------------------------------------ render

fn bar(points: i64, max_points: i64) -> String {
    if max_points <= 0 || points <= 0 {
        return "░".repeat(8);
    }
    let filled = ((points as f64 / max_points as f64) * 8.0)
        .round()
        .clamp(0.0, 8.0) as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(8 - filled))
}

pub fn planner_embed(session: &SvsSession) -> CreateEmbed {
    let (day_key, day_label, _, day_emoji, _) = day(&session.day_key);
    let _ = day_key;
    let activity = ensure_activity(session);
    let day_plan = session
        .plans
        .get(&session.day_key)
        .cloned()
        .unwrap_or_default();
    let score = compute_day_points(&session.day_key, &day_plan);
    let all = compute_all(&session.plans);
    let max_day = all
        .by_day
        .values()
        .map(|s| s.total)
        .max()
        .unwrap_or(1)
        .max(1);

    let top: Vec<String> = score
        .lines
        .iter()
        .take(5)
        .map(|(key, qty, pts)| {
            let (_, priority) = day_activities(&session.day_key)
                .iter()
                .find(|(k, _)| k == key)
                .copied()
                .unwrap_or(("", "low"));
            let label = rate(key).map(|r| r.label).unwrap_or("");
            format!(
                "{} **{}**: ×{} → **{}** pts",
                priority_emoji(priority),
                crate::error::truncate(label, 60),
                format_game_amount(*qty),
                format_game_amount(*pts)
            )
        })
        .collect();

    let breakdown: Vec<String> = DAYS
        .iter()
        .map(|(key, _, short, emoji, _)| {
            let pts = all.by_day.get(*key).map(|s| s.total).unwrap_or(0);
            let marker = if *key == session.day_key { "▸" } else { " " };
            format!(
                "{marker}{emoji} **{short}** `{}` {}",
                bar(pts, max_day),
                if pts > 0 {
                    format_game_amount(pts)
                } else {
                    "—".to_string()
                }
            )
        })
        .collect();

    let target_line = if session.target > 0 {
        let remaining = session.target - all.grand;
        if remaining <= 0 {
            format!(
                "🎯 **Target:** {} — done! ✅",
                format_game_amount(session.target)
            )
        } else {
            format!(
                "🎯 **Target:** {} — need {} more",
                format_game_amount(session.target),
                format_game_amount(remaining)
            )
        }
    } else {
        "🎯 **Target:** not set".to_string()
    };

    let qty = activity
        .as_ref()
        .and_then(|key| day_plan.get(key).copied())
        .unwrap_or(0);
    let activity_points = activity
        .as_ref()
        .and_then(|key| rate(key).map(|r| r.points * qty))
        .unwrap_or(0);

    CreateEmbed::new()
        .title("🗓️ SVS Prep — Points Calculator")
        .color(0x5865f2)
        .description(format!(
            "**{day_emoji} {day_label}**\n\n🟢 High value · 🟡 Medium · 🔴 Low — prioritise 🟢 first!\n\n🏆 **Day points**\n**{}** SVS points\n\n💎 **Total SVS points**\n**{}** total\n{target_line}\n\n📊 **5-day breakdown**\n{}\n\n🥇 **Today's top items**\n{}\n\n_Selected activity: {}×{} → **{}** pts_",
            format_game_amount(score.total),
            format_game_amount(all.grand),
            breakdown.join("\n"),
            if top.is_empty() { "_Nothing entered yet — tap **Quantity**._".to_string() } else { top.join("\n") },
            format_game_amount(qty),
            activity
                .as_ref()
                .and_then(|key| rate(key).map(|r| r.unit))
                .unwrap_or(""),
            format_game_amount(activity_points)
        ))
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

/// The persistent panel users see in the configured channel.
pub fn panel_embed() -> CreateEmbed {
    let day_lines: Vec<String> = DAYS
        .iter()
        .map(|(_, _, short, emoji, themes)| format!("{emoji} **{short}** — {}", themes.join(", ")))
        .collect();

    CreateEmbed::new()
        .title("🗓️ SVS Prep Week Planner")
        .color(0x5865f2)
        .description(format!(
            "Plan your **State vs State Prep** score across **5 event days**.\nEnter planned activities, track day totals, and aim for your target score.\n\n🟢 High value / 🟡 Medium / 🔴 Low priority scoring\n\n**Day themes**\n{}\n\n**How to use**\n1. Tap **📊 Planner** (private session)\n2. Pick **Day** → **Activity** → **Quantity**\n3. Optional: **Target** · **Overview** · **Tips**",
            day_lines.join("\n")
        ))
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn panel_row(message_id: u64) -> serenity::CreateActionRow {
    serenity::CreateActionRow::Buttons(vec![
        crate::ui::components::primary("svs:open", "📊 Planner"),
        crate::translate::translate_button(message_id),
    ])
}

/// Post (or repost) the persistent SVS panel in the given channel.
pub async fn post_panel(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    guild_id: u64,
    channel_id: u64,
) -> anyhow::Result<u64> {
    let target = serenity::ChannelId::new(channel_id);
    let message = target
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new().embed(panel_embed()),
        )
        .await?;
    target
        .edit_message(
            &ctx.http,
            message.id,
            serenity::EditMessage::new().components(vec![panel_row(message.id.get())]),
        )
        .await?;

    let stored_json = crate::translate::stored_json(&panel_embed());
    crate::translate::register(
        &data.db,
        message.id.get(),
        crate::db::translate::NewTranslatable {
            guild_id: Some(guild_id),
            channel_id: Some(channel_id),
            kind: "svs_panel",
            embed_json: stored_json,
            content: None,
            owner_id: None,
        },
    )
    .await?;

    crate::db::calculator::set_channel(
        &data.db,
        guild_id,
        crate::db::calculator::SVS,
        Some(channel_id),
        Some(message.id.get()),
    )
    .await?;
    Ok(message.id.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_points_sorted() {
        let mut plan = HashMap::new();
        plan.insert("charm_score".to_string(), 100);
        plan.insert("rfc_building".to_string(), 1);
        let score = compute_day_points("day1", &plan);
        assert_eq!(score.total, 30_000 + 7_000);
        assert_eq!(score.lines[0].0, "rfc_building");
    }

    #[test]
    fn compute_all_sums() {
        let mut plans = HashMap::new();
        let mut day4 = HashMap::new();
        day4.insert("charm_score".to_string(), 10);
        plans.insert("day4".to_string(), day4);
        let all = compute_all(&plans);
        assert_eq!(all.grand, 700);
        assert_eq!(all.by_day["day1"].total, 0);
    }

    #[test]
    fn ensure_activity_falls_back() {
        let session = SvsSession {
            day_key: "day4".to_string(),
            activity_key: Some("widgets".to_string()),
            ..SvsSession::default()
        };
        assert_eq!(ensure_activity(&session).as_deref(), Some("charm_score"));
    }
}
