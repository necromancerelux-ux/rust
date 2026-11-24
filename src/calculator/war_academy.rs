//! War Academy calculator — T11 (Helios) and T12 (Exalted) research plans.
//!
//! Mirrors the legacy `warAcademyCalculator` flow:
//!   panel → tier → plan hub → Add research → troop → skill → levels
//!   → Buffs (research speed + VP) → Show totals.
//!
//! Both tiers live in embedded JSONs (`waracademy/{Infantry,Lancer,Marksman}.json`
//! and `waracademy/T12_{Infantry,Lancer,Marksman}.json`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use poise::serenity_prelude as serenity;
use serde::Deserialize;
use serenity::CreateEmbed;

use crate::calculator::game_numbers::{format_game_amount, format_game_duration};
use crate::ui::components;

// ------------------------------------------------------------------ data

#[derive(Debug, Clone, Default)]
pub struct SkillLevel {
    pub cost: HashMap<String, f64>,
    pub time_seconds: f64,
    pub war_academy: Option<i64>,
    pub other_skills: HashMap<String, i64>,
    pub buff: Option<BuffGain>,
}

#[derive(Debug, Clone, Default)]
pub struct BuffGain {
    pub name: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Skill {
    pub name: String,
    pub levels: Vec<(i64, SkillLevel)>,
    pub max_level: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Troop {
    pub key: String,
    pub label: String,
    pub skills: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct RawData {
    skills: HashMap<String, RawSkill>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct RawSkill {
    levels: HashMap<String, RawLevel>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct RawLevel {
    cost: Option<HashMap<String, serde_json::Value>>,
    time: Option<serde_json::Value>,
    requirements: Option<serde_json::Value>,
    buff: Option<RawBuff>,
}

#[derive(Debug, Deserialize)]
struct RawBuff {
    name: String,
    amount: serde_json::Value,
}

static T11: OnceLock<HashMap<&'static str, HashMap<String, Skill>>> = OnceLock::new();
static T12: OnceLock<HashMap<&'static str, HashMap<String, Skill>>> = OnceLock::new();

const T11_DATA: [(&str, &str); 3] = [
    ("infantry", include_str!("data/Infantry.json")),
    ("lancer", include_str!("data/Lancer.json")),
    ("marksman", include_str!("data/Marksman.json")),
];
const T12_DATA: [(&str, &str); 3] = [
    ("infantry", include_str!("data/T12_Infantry.json")),
    ("lancer", include_str!("data/T12_Lancer.json")),
    ("marksman", include_str!("data/T12_Marksman.json")),
];

fn parse_number(value: &serde_json::Value) -> f64 {
    match value {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn parse_time(value: &serde_json::Value) -> f64 {
    // {"raw-seconds": N} preferred; fall back to days/hours/minutes/seconds.
    if let Some(raw) = value.get("raw-seconds").and_then(|v| v.as_f64()) {
        return raw;
    }
    let get = |key: &str| value.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
    get("days") * 86400.0 + get("hours") * 3600.0 + get("minutes") * 60.0 + get("seconds")
}

fn parse_level(raw: RawLevel) -> SkillLevel {
    let mut level = SkillLevel::default();
    if let Some(cost) = raw.cost {
        for (key, value) in cost {
            level.cost.insert(key, parse_number(&value));
        }
    }
    if let Some(time) = raw.time {
        level.time_seconds = parse_time(&time);
    }
    if let Some(reqs) = raw.requirements {
        if let Some(wa) = reqs.get("war-academy") {
            level.war_academy = match wa {
                serde_json::Value::Number(n) => n.as_i64(),
                serde_json::Value::String(s) => s.parse().ok(),
                _ => None,
            };
        }
        if let Some(other) = reqs.get("other-skill").and_then(|v| v.as_object()) {
            for (skill, req) in other {
                let parsed = match req {
                    serde_json::Value::Number(n) => n.as_i64(),
                    serde_json::Value::String(s) => s.parse().ok(),
                    _ => None,
                };
                if let Some(lv) = parsed {
                    if lv > 0 {
                        level.other_skills.insert(skill.clone(), lv);
                    }
                }
            }
        }
    }
    if let Some(buff) = raw.buff {
        level.buff = Some(BuffGain {
            name: buff.name,
            amount: parse_number(&buff.amount),
        });
    }
    level
}

fn build(
    table: [(&'static str, &'static str); 3],
) -> HashMap<&'static str, HashMap<String, Skill>> {
    let mut out = HashMap::with_capacity(3);
    for (troop, json) in table {
        let parsed: RawData = serde_json::from_str(json).expect("waracademy json is valid");
        let mut skills = HashMap::with_capacity(parsed.skills.len());
        for (name, skill) in parsed.skills {
            let mut levels: Vec<(i64, SkillLevel)> = skill
                .levels
                .into_iter()
                .filter_map(|(key, raw)| {
                    let level_key: i64 = key.parse().ok()?;
                    Some((level_key, parse_level(raw)))
                })
                .collect();
            levels.sort_by_key(|(key, _)| *key);
            let max_level = levels.iter().map(|(key, _)| *key).max().unwrap_or(0);
            skills.insert(
                name.clone(),
                Skill {
                    name,
                    levels,
                    max_level,
                },
            );
        }
        out.insert(troop, skills);
    }
    out
}

fn tier_map(tier: &str) -> &'static HashMap<&'static str, HashMap<String, Skill>> {
    if tier == "t12" {
        T12.get_or_init(|| build(T12_DATA))
    } else {
        T11.get_or_init(|| build(T11_DATA))
    }
}

pub const TROOPS: [(&str, &str, &str); 3] = [
    ("infantry", "Infantry", "🛡️"),
    ("lancer", "Lancer", "🐎"),
    ("marksman", "Marksman", "🏹"),
];

pub fn troops(tier: &str) -> Vec<Troop> {
    tier_map(tier)
        .iter()
        .map(|(key, skills)| {
            let label = TROOPS
                .iter()
                .find(|(k, _, _)| k == key)
                .map(|(_, l, _)| l.to_string())
                .unwrap_or_else(|| key.to_string());
            let mut names: Vec<String> = skills.keys().cloned().collect();
            names.sort();
            Troop {
                key: key.to_string(),
                label,
                skills: names,
            }
        })
        .collect()
}

pub fn skill(tier: &str, troop_key: &str, skill_name: &str) -> Option<&'static Skill> {
    tier_map(tier).get(troop_key)?.get(skill_name)
}

#[derive(Debug, Clone, Default)]
pub struct RangeTotals {
    pub meat: f64,
    pub wood: f64,
    pub coal: f64,
    pub iron: f64,
    pub steel: f64,
    pub fc_shards: f64,
    pub refined_fire_crystal: f64,
    pub time_seconds: f64,
    pub max_war_academy: i64,
    pub other_skill_reqs: HashMap<String, i64>,
    pub buff_name: Option<String>,
    pub buff_amount: f64,
    pub num_levels: i64,
}

pub fn compute_range(
    tier: &str,
    troop_key: &str,
    skill_name: &str,
    from: i64,
    to: i64,
) -> Option<RangeTotals> {
    let skill = skill(tier, troop_key, skill_name)?;
    if to <= from {
        return None;
    }
    let mut out = RangeTotals::default();
    for (level_key, level) in &skill.levels {
        if *level_key <= from || *level_key > to {
            continue;
        }
        for (key, amount) in &level.cost {
            match key.as_str() {
                "meat" => out.meat += amount,
                "wood" => out.wood += amount,
                "coal" => out.coal += amount,
                "iron" => out.iron += amount,
                "steel" => out.steel += amount,
                "fc_shards" => out.fc_shards += amount,
                "refinedFireCrystal" => out.refined_fire_crystal += amount,
                _ => {}
            }
        }
        out.time_seconds += level.time_seconds;
        if let Some(wa) = level.war_academy {
            out.max_war_academy = out.max_war_academy.max(wa);
        }
        for (name, lv) in &level.other_skills {
            let entry = out.other_skill_reqs.entry(name.clone()).or_insert(0);
            *entry = (*entry).max(*lv);
        }
        if let Some(buff) = &level.buff {
            out.buff_name = Some(buff.name.clone());
            out.buff_amount += buff.amount;
        }
        out.num_levels += 1;
    }
    Some(out)
}

// --------------------------------------------------------------- session

pub const SESSION_TTL_MS: u128 = 15 * 60 * 1000;

#[derive(Debug, Clone, Default)]
pub struct WaBuffs {
    pub research_speed: f64,
    pub vp_bonus: i64,
}

impl WaBuffs {
    pub fn total_speed_pct(&self) -> f64 {
        self.research_speed + self.vp_bonus as f64
    }

    pub fn active(&self) -> bool {
        self.total_speed_pct() > 0.0
    }

    pub fn reduced_seconds(&self, seconds: f64) -> f64 {
        let pct = self.total_speed_pct();
        if pct > 0.0 {
            (seconds / (1.0 + pct / 100.0)).ceil()
        } else {
            seconds
        }
    }
}

#[derive(Debug, Clone)]
pub struct WaEntry {
    pub id: u64,
    pub troop_key: String,
    pub skill_name: String,
    pub from: i64,
    pub to: i64,
}

#[derive(Debug, Clone, Default)]
pub struct WaDraft {
    pub troop_key: Option<String>,
    pub skill_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WaSession {
    pub tier: String,
    pub buffs: WaBuffs,
    pub entries: Vec<WaEntry>,
    pub next_id: u64,
    pub draft: Option<WaDraft>,
    pub touched_ms: u128,
}

impl WaSession {
    fn new(tier: &str) -> Self {
        Self {
            tier: tier.to_string(),
            buffs: WaBuffs::default(),
            entries: Vec::new(),
            next_id: 1,
            draft: None,
            touched_ms: now_ms(),
        }
    }
}

fn sessions() -> &'static Mutex<HashMap<u64, WaSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<u64, WaSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn now_ms() -> u128 {
    crate::calculator::svs::now_ms()
}

pub fn session_for(user_id: u64) -> WaSession {
    let now = now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    guard.entry(user_id).or_default().clone_with_time(now)
}

pub fn new_session(user_id: u64, tier: &str) -> WaSession {
    let session = WaSession::new(tier);
    let now = now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    guard.insert(user_id, session.clone());
    session
}

pub fn update_session<F>(user_id: u64, update: F)
where
    F: FnOnce(&mut WaSession),
{
    let now = now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    let session = guard.entry(user_id).or_default();
    session.touched_ms = now;
    update(session);
}

impl WaSession {
    fn clone_with_time(&self, now: u128) -> Self {
        let mut clone = self.clone();
        clone.touched_ms = now;
        clone
    }
}

// ------------------------------------------------------------------ tiers

pub fn tier_label(tier: &str) -> &'static str {
    if tier == "t12" {
        "T12 Exalted"
    } else {
        "T11 Helios"
    }
}

pub fn troop_emoji(troop_key: &str) -> &'static str {
    TROOPS
        .iter()
        .find(|(key, _, _)| *key == troop_key)
        .map(|(_, _, emoji)| *emoji)
        .unwrap_or("📋")
}

pub fn troop_label(troop_key: &str) -> &'static str {
    TROOPS
        .iter()
        .find(|(key, _, _)| *key == troop_key)
        .map(|(_, label, _)| *label)
        .unwrap_or("Research")
}

pub fn level_label(level: i64) -> String {
    if level <= 0 {
        "Not researched".to_string()
    } else {
        format!("Level {level}")
    }
}

// ------------------------------------------------------------------ view

pub fn panel_embed() -> CreateEmbed {
    let mut t11_lines = Vec::new();
    for (key, label, emoji) in TROOPS {
        t11_lines.push(format!(
            "{emoji} **{label}** — {} lines",
            tier_map("t11")
                .get(key)
                .map(|skills| skills.len())
                .unwrap_or(0)
        ));
    }
    let mut t12_lines = Vec::new();
    for (key, label, emoji) in TROOPS {
        t12_lines.push(format!(
            "{emoji} **{label}** — {} lines",
            tier_map("t12")
                .get(key)
                .map(|skills| skills.len())
                .unwrap_or(0)
        ));
    }

    CreateEmbed::new()
        .title("🏛️ War Academy Calculator — T11 & T12 research")
        .color(0x57c964)
        .description(format!(
            "Plan War Academy research with verified cost tables.\n\n**T11 (Helios)**\n{}\n\n**T12 (Exalted)**\n{}\n\n**How to use**\n🌐 **Translate** — private guide in your language (optional)\n📗 **T11 Calculate** / 📘 **T12 Calculate** — multi-entry plan, buffs, combined totals\n\n**Buffs**\nResearch speed % and VP (+10% / +15%)",
            t11_lines.join("\n"),
            t12_lines.join("\n"),
        ))
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn panel_row(message_id: u64) -> serenity::CreateActionRow {
    serenity::CreateActionRow::Buttons(vec![
        crate::translate::translate_button(message_id),
        components::primary("waracademycalc:open:t11", "📗 T11 Calculate"),
        components::success("waracademycalc:open:t12", "📘 T12 Calculate"),
    ])
}

/// Post (or repost) the persistent waracademy panel.
pub async fn post_panel(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    guild_id: u64,
    channel_id: u64,
) -> anyhow::Result<()> {
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
            kind: "waracademy",
            embed_json: stored_json,
            content: None,
            owner_id: None,
        },
    )
    .await?;

    crate::db::calculator::set_channel(
        &data.db,
        guild_id,
        crate::db::calculator::WAR_ACADEMY,
        Some(channel_id),
        Some(message.id.get()),
    )
    .await?;
    Ok(())
}

pub fn plan_embed(session: &WaSession) -> CreateEmbed {
    let tier_label = tier_label(&session.tier);
    let mut lines: Vec<String> = Vec::new();
    if session.entries.is_empty() {
        lines.push("_Nothing planned yet — tap **Add research** to start._".to_string());
    } else {
        for entry in &session.entries {
            lines.push(format!(
                "• {} **{}** — {}: {} → {}",
                troop_emoji(&entry.troop_key),
                troop_label(&entry.troop_key),
                entry.skill_name,
                level_label(entry.from),
                level_label(entry.to)
            ));
        }
    }

    let mut buff_parts: Vec<String> = Vec::new();
    if session.buffs.research_speed > 0.0 {
        buff_parts.push(format!("Research {}%", session.buffs.research_speed));
    }
    if session.buffs.vp_bonus > 0 {
        buff_parts.push(format!("VP +{}%", session.buffs.vp_bonus));
    }
    let buff_line = if buff_parts.is_empty() {
        "🧙 Buffs: None".to_string()
    } else {
        format!("🧙 Buffs: {}", buff_parts.join(" · "))
    };

    CreateEmbed::new()
        .title(format!("🏛️ Research plan ({tier_label})"))
        .color(0x57c964)
        .description(format!("{}\n\n{buff_line}", lines.join("\n")))
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn plan_rows(session: &WaSession) -> Vec<serenity::CreateActionRow> {
    use serenity::{CreateActionRow, CreateSelectMenu, CreateSelectMenuKind};

    let mut rows = vec![
        CreateActionRow::Buttons(vec![
            components::primary("waracademycalc:add", "➕ Add research"),
            components::secondary("waracademycalc:buffs", "🧙 Buffs"),
            {
                let button = components::success("waracademycalc:totals", "📊 Show totals");
                button.disabled(session.entries.is_empty())
            },
        ]),
        CreateActionRow::Buttons(vec![{
            let button = components::danger("waracademycalc:clear", "🗑️ Clear plan");
            button.disabled(session.entries.is_empty())
        }]),
    ];

    if !session.entries.is_empty() {
        let options: Vec<serenity::CreateSelectMenuOption> = session
            .entries
            .iter()
            .take(25)
            .map(|entry| {
                serenity::CreateSelectMenuOption::new(
                    format!(
                        "{} {} {}→{}",
                        troop_label(&entry.troop_key),
                        entry.skill_name,
                        entry.from,
                        entry.to
                    )
                    .chars()
                    .take(100)
                    .collect::<String>(),
                    entry.id.to_string(),
                )
            })
            .collect();
        rows.push(CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                "waracademycalc:remove",
                CreateSelectMenuKind::String { options },
            )
            .placeholder("Remove an entry…")
            .min_values(1)
            .max_values(1),
        ));
    }

    rows
}

pub fn totals_embed(session: &WaSession) -> Option<CreateEmbed> {
    if session.entries.is_empty() {
        return None;
    }

    let mut totals = RangeTotals::default();
    let mut entry_lines: Vec<String> = Vec::new();
    let mut buff_amounts: HashMap<String, f64> = HashMap::new();

    for entry in &session.entries {
        let Some(range) = compute_range(
            &session.tier,
            &entry.troop_key,
            &entry.skill_name,
            entry.from,
            entry.to,
        ) else {
            continue;
        };
        entry_lines.push(format!(
            "• {} **{}** — {}: {} → {}",
            troop_emoji(&entry.troop_key),
            troop_label(&entry.troop_key),
            entry.skill_name,
            level_label(entry.from),
            level_label(entry.to)
        ));
        totals.meat += range.meat;
        totals.wood += range.wood;
        totals.coal += range.coal;
        totals.iron += range.iron;
        totals.steel += range.steel;
        totals.fc_shards += range.fc_shards;
        totals.refined_fire_crystal += range.refined_fire_crystal;
        totals.time_seconds += range.time_seconds;
        totals.max_war_academy = totals.max_war_academy.max(range.max_war_academy);
        for (name, lv) in range.other_skill_reqs {
            let entry = totals.other_skill_reqs.entry(name).or_insert(0);
            *entry = (*entry).max(lv);
        }
        if let Some(name) = &range.buff_name {
            *buff_amounts.entry(name.clone()).or_insert(0.0) += range.buff_amount;
        }
    }

    let reduced = session.buffs.reduced_seconds(totals.time_seconds);

    let mut embed = CreateEmbed::new()
        .title(format!("🏛️ Plan totals ({})", tier_label(&session.tier)))
        .color(0x57c964)
        .description(format!("**Entries**\n{}", entry_lines.join("\n")));

    // Collect fields first (embed can't be borrowed by a closure while used).
    let mut fields: Vec<(String, String)> = Vec::new();
    let amount = |value: f64| format_game_amount(value.round() as i64);
    if totals.fc_shards > 0.0 {
        fields.push((
            "💎 FC Shards".into(),
            format!("**{}**", amount(totals.fc_shards)),
        ));
    }
    if totals.refined_fire_crystal > 0.0 {
        fields.push((
            "🔷 Refined FC".into(),
            format!("**{}**", amount(totals.refined_fire_crystal)),
        ));
    }
    if totals.steel > 0.0 {
        fields.push(("⚙️ Steel".into(), format!("**{}**", amount(totals.steel))));
    }
    if totals.meat > 0.0 {
        fields.push(("🍖 Meat".into(), format!("**{}**", amount(totals.meat))));
    }
    if totals.wood > 0.0 {
        fields.push((
            format!("{} Wood", crate::emoji::material_tag("wood")),
            format!("**{}**", amount(totals.wood)),
        ));
    }
    if totals.coal > 0.0 {
        fields.push(("🪨 Coal".into(), format!("**{}**", amount(totals.coal))));
    }
    if totals.iron > 0.0 {
        fields.push((
            format!("{} Iron", crate::emoji::material_tag("iron")),
            format!("**{}**", amount(totals.iron)),
        ));
    }
    fields.push((
        "⏱️ Total research time (base)".into(),
        format!("**{}**", format_game_duration(totals.time_seconds)),
    ));
    if session.buffs.active() && (reduced - totals.time_seconds).abs() >= 1.0 {
        fields.push((
            "⚡ With speed buffs".into(),
            format!("**{}**", format_game_duration(reduced)),
        ));
    }

    if session.buffs.active() {
        let mut parts: Vec<String> = Vec::new();
        if session.buffs.research_speed > 0.0 {
            parts.push(format!("Research speed {}%", session.buffs.research_speed));
        }
        if session.buffs.vp_bonus > 0 {
            parts.push(format!("VP +{}%", session.buffs.vp_bonus));
        }
        fields.push(("🧙 Active speed buffs".into(), parts.join("\n")));
    }

    let mut req_parts: Vec<String> = Vec::new();
    if totals.max_war_academy > 0 {
        req_parts.push(format!("🏛️ War Academy: {}", totals.max_war_academy));
    }
    let mut other: Vec<(&String, &i64)> = totals.other_skill_reqs.iter().collect();
    other.sort_by_key(|(name, _)| *name);
    for (name, lv) in other {
        req_parts.push(format!("• {name}: {lv}"));
    }
    if !req_parts.is_empty() {
        fields.push(("📌 Requirements".into(), req_parts.join("\n")));
    }

    if !buff_amounts.is_empty() {
        let lines: Vec<String> = buff_amounts
            .iter()
            .map(|(name, amount)| format!("+{} {name}", amount.round() as i64))
            .collect();
        fields.push(("📈 Buff gained".into(), lines.join("\n")));
    }

    for (name, value) in fields {
        embed = embed.field(name, value, true);
    }

    embed = embed.footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));
    Some(embed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t11_range_sums_costs_and_time() {
        // Flame Squad Lv 0 -> 1 (T11 Infantry).
        let range = compute_range("t11", "infantry", "Flame Squad", 0, 1).expect("skill exists");
        assert_eq!(range.meat, 300000.0);
        assert_eq!(range.wood, 300000.0);
        assert_eq!(range.coal, 60000.0);
        assert_eq!(range.iron, 15000.0);
        assert_eq!(range.steel, 5000.0);
        assert_eq!(range.fc_shards, 16.0);
        assert_eq!(range.time_seconds, 28800.0);
        assert_eq!(range.num_levels, 1);
        assert_eq!(range.max_war_academy, 35);
    }

    #[test]
    fn t12_has_twenty_levels() {
        let skill = skill("t12", "infantry", "T12 Infantry Research I").expect("T12 skill");
        assert_eq!(skill.max_level, 20);
        let range = compute_range("t12", "infantry", "T12 Infantry Research I", 0, 1)
            .expect("skill exists");
        assert!(range.time_seconds > 0.0);
    }

    #[test]
    fn range_rejects_inverted_or_equal() {
        assert!(compute_range("t11", "infantry", "Flame Squad", 1, 1).is_none());
        assert!(compute_range("t11", "infantry", "Flame Squad", 2, 1).is_none());
    }

    #[test]
    fn vp_buff_reduces_time() {
        let buffs = WaBuffs {
            research_speed: 0.0,
            vp_bonus: 10,
        };
        assert_eq!(buffs.reduced_seconds(110.0), 100.0);
        let buffs = WaBuffs {
            research_speed: 15.0,
            vp_bonus: 10,
        };
        assert_eq!(buffs.reduced_seconds(125.0), 100.0);
    }
}
