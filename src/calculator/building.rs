//! Building Calculator — costs/upgrade planner.
//!
//! Rust port of `buildingCalculator.js` + `buildingData.js` +
//! `buffs.js`. Data tables (levels 1–30 + Fire Crystal tiers) ship embedded
//! in the binary.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use poise::serenity_prelude as serenity;
use regex::Regex;
use serde::Deserialize;
use serenity::CreateEmbed;

use crate::calculator::game_numbers::format_game_amount;

const BASIC: &str = include_str!("data/basic.json");
const FIRE_CRYSTAL: &str = include_str!("data/fire_crystal.json");

#[derive(Debug, Clone, Deserialize)]
pub struct TimeData {
    #[serde(default)]
    pub days: i64,
    #[serde(default)]
    pub hours: i64,
    #[serde(default)]
    pub minutes: i64,
    #[serde(default)]
    pub seconds: i64,
}

impl TimeData {
    pub fn seconds(&self) -> i64 {
        self.days * 86_400 + self.hours * 3_600 + self.minutes * 60 + self.seconds
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelData {
    #[serde(default)]
    pub cost: HashMap<String, f64>,
    #[serde(default)]
    pub requirements: Option<Requirements>,
    #[serde(default)]
    pub time: Option<TimeData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Requirements {
    #[serde(default)]
    pub buildings: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
pub struct Building {
    pub key: String,
    pub label: String,
    pub levels: HashMap<String, LevelData>,
    pub level_keys: Vec<i64>,
}

const BUILDING_META: &[(&str, &str, bool, bool)] = &[
    // key, label, has_fc, fc_only
    ("furnace", "Furnace", true, false),
    ("embassy", "Embassy", true, false),
    ("lancerCamp", "Lancer Camp", true, false),
    ("infantryCamp", "Infantry Camp", true, false),
    ("marksmanCamp", "Marksman Camp", true, false),
    ("commandCenter", "Command Center", true, false),
    ("researchCenter", "Research Center", false, false),
    ("warAcademy", "War Academy", true, true),
    ("infirmary", "Infirmary", true, false),
    ("storehouse", "Storehouse", false, false),
    ("barricade", "Barricade", false, false),
];

pub fn building_list() -> &'static [(&'static str, &'static str)] {
    const ORDER: &[(&str, &str)] = &[
        ("furnace", "Furnace"),
        ("embassy", "Embassy"),
        ("warAcademy", "War Academy"),
        ("commandCenter", "Command Center"),
        ("researchCenter", "Research Center"),
        ("infantryCamp", "Infantry Camp"),
        ("lancerCamp", "Lancer Camp"),
        ("marksmanCamp", "Marksman Camp"),
        ("infirmary", "Infirmary"),
        ("storehouse", "Storehouse"),
        ("barricade", "Barricade"),
    ];
    ORDER
}

static BUILDINGS: OnceLock<HashMap<String, Building>> = OnceLock::new();

fn load_buildings() -> &'static HashMap<String, Building> {
    BUILDINGS.get_or_init(|| {
        let basic: HashMap<String, HashMap<String, LevelData>> =
            serde_json::from_str(BASIC).unwrap_or_else(|e| panic!("basic.json: {e}"));
        let fire_crystal: HashMap<String, HashMap<String, LevelData>> =
            serde_json::from_str(FIRE_CRYSTAL).unwrap_or_else(|e| panic!("fire_crystal.json: {e}"));

        let mut map = HashMap::new();
        for (key, label, _, _) in BUILDING_META {
            let mut levels = HashMap::new();
            if let Some(table) = basic.get(*key).cloned() {
                levels.extend(table);
            }
            if let Some(table) = fire_crystal.get(*key).cloned() {
                levels.extend(table);
            }
            // Keep only numeric level keys.
            let mut numeric: Vec<(i64, String)> = levels
                .keys()
                .filter_map(|k| k.parse::<i64>().ok().map(|n| (n, k.clone())))
                .collect();
            numeric.sort_by_key(|(n, _)| *n);
            let level_keys: Vec<i64> = numeric.into_iter().map(|(n, _)| n).collect();
            if level_keys.is_empty() {
                continue;
            }
            map.insert(
                key.to_string(),
                Building {
                    key: key.to_string(),
                    label: label.to_string(),
                    levels,
                    level_keys,
                },
            );
        }
        map
    })
}

pub fn building(key: &str) -> Option<&'static Building> {
    load_buildings().get(key)
}

/// Furnace level → FC display label (levels 31–80).
pub fn furnace_level_mapping() -> &'static [(&'static str, &'static str)] {
    &[
        ("31", "30-1"),
        ("32", "30-2"),
        ("33", "30-3"),
        ("34", "30-4"),
        ("35", "FC 1"),
        ("36", "FC 1-1"),
        ("37", "FC 1-2"),
        ("38", "FC 1-3"),
        ("39", "FC 1-4"),
        ("40", "FC 2"),
        ("41", "FC 2-1"),
        ("42", "FC 2-2"),
        ("43", "FC 2-3"),
        ("44", "FC 2-4"),
        ("45", "FC 3"),
        ("46", "FC 3-1"),
        ("47", "FC 3-2"),
        ("48", "FC 3-3"),
        ("49", "FC 3-4"),
        ("50", "FC 4"),
        ("51", "FC 4-1"),
        ("52", "FC 4-2"),
        ("53", "FC 4-3"),
        ("54", "FC 4-4"),
        ("55", "FC 5"),
        ("56", "FC 5-1"),
        ("57", "FC 5-2"),
        ("58", "FC 5-3"),
        ("59", "FC 5-4"),
        ("60", "FC 6"),
        ("61", "FC 6-1"),
        ("62", "FC 6-2"),
        ("63", "FC 6-3"),
        ("64", "FC 6-4"),
        ("65", "FC 7"),
        ("66", "FC 7-1"),
        ("67", "FC 7-2"),
        ("68", "FC 7-3"),
        ("69", "FC 7-4"),
        ("70", "FC 8"),
        ("71", "FC 8-1"),
        ("72", "FC 8-2"),
        ("73", "FC 8-3"),
        ("74", "FC 8-4"),
        ("75", "FC 9"),
        ("76", "FC 9-1"),
        ("77", "FC 9-2"),
        ("78", "FC 9-3"),
        ("79", "FC 9-4"),
        ("80", "FC 10"),
    ]
}

/// Display string for a level key (`0` = "Not built").
pub fn level_display(key: i64) -> String {
    if key == 0 {
        return "Not built".to_string();
    }
    if let Some((_, display)) = furnace_level_mapping()
        .iter()
        .find(|(num, _)| *num == key.to_string())
    {
        return display.to_string();
    }
    key.to_string()
}

static FC_RE: OnceLock<Regex> = OnceLock::new();
static SUB_RE: OnceLock<Regex> = OnceLock::new();

/// Parse user input like "5", "FC1", "FC 1-2", "30-1", "fc10" into a level.
pub fn parse_level_token(raw: &str, building_key: &str) -> Option<i64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("not built")
        || trimmed.eq_ignore_ascii_case("nb")
        || trimmed.eq_ignore_ascii_case("none")
    {
        return Some(0);
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        if n == 0 {
            return Some(0);
        }
        let building = building(building_key)?;
        if building.level_keys.contains(&n) {
            return Some(n);
        }
        // Furnace-style plain numbers up to 30 only; beyond that require FC syntax.
        return None;
    }

    let normalized = trimmed.replace(['\u{2014}', '–'], "-").to_uppercase();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");

    let fc_re = FC_RE.get_or_init(|| Regex::new(r"^FC\s*(\d{1,2})(?:\s*-\s*(\d))?$").unwrap());
    if let Some(captures) = fc_re.captures(&normalized) {
        let major: u64 = captures[1].parse().ok()?;
        let minor = captures
            .get(2)
            .map(|m| m.as_str().parse::<u64>().unwrap_or(0));
        let display = match minor {
            None => format!("FC {major}"),
            Some(0) => format!("FC {major}"),
            Some(minor) => format!("FC {major}-{minor}"),
        };
        for (num, disp) in furnace_level_mapping() {
            if disp.eq_ignore_ascii_case(&display) {
                return num.parse().ok();
            }
        }
        return None;
    }

    let sub_re = SUB_RE.get_or_init(|| Regex::new(r"^30\s*-\s*([1-4])$").unwrap());
    if let Some(captures) = sub_re.captures(&normalized) {
        return match &captures[1] {
            "1" => Some(31),
            "2" => Some(32),
            "3" => Some(33),
            "4" => Some(34),
            _ => None,
        };
    }

    None
}

#[derive(Debug, Clone, Default)]
pub struct BuildingTotals {
    pub meat: i64,
    pub wood: i64,
    pub coal: i64,
    pub iron: i64,
    pub fire_crystal: i64,
    pub refined_fire_crystal: i64,
    pub time_seconds: i64,
    pub num_levels: i64,
    pub requirements: HashMap<String, i64>,
}

/// Sum costs/time to go from `from` (owned) to `to` (inclusive).
pub fn compute_range(key: &str, from: i64, to: i64) -> Option<BuildingTotals> {
    let building = building(key)?;
    if to <= from {
        return None;
    }
    let mut totals = BuildingTotals::default();
    for level in &building.level_keys {
        if *level <= from || *level > to {
            continue;
        }
        let Some(step) = building.levels.get(&level.to_string()) else {
            continue;
        };
        totals.num_levels += 1;
        for (res, amt) in &step.cost {
            let n = *amt as i64;
            match res.as_str() {
                "meat" => totals.meat += n,
                "wood" => totals.wood += n,
                "coal" => totals.coal += n,
                "iron" => totals.iron += n,
                "fireCrystal" => totals.fire_crystal += n,
                "refinedFireCrystal" => totals.refined_fire_crystal += n,
                _ => {}
            }
        }
        if let Some(time) = &step.time {
            totals.time_seconds += time.seconds();
        }
        if let Some(reqs) = &step.requirements {
            for (bkey, lvl) in &reqs.buildings {
                let n = *lvl as i64;
                if n > 0 {
                    let entry = totals.requirements.entry(bkey.clone()).or_insert(0);
                    *entry = (*entry).max(n);
                }
            }
        }
    }
    Some(totals)
}

// ------------------------------------------------------------------ buffs

pub const PET_SPEED: [i64; 6] = [0, 5, 7, 9, 12, 15];
pub const ZINMAN_REDUCE: [i64; 6] = [0, 3, 6, 9, 12, 15];
pub const EXPERT_SECS: [i64; 6] = [0, 7_200, 10_800, 14_400, 21_600, 28_800];
pub const DISCOUNTABLE: &[&str] = &["meat", "wood", "coal", "iron"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildingBuffs {
    pub construction_speed: i64, // manual %
    pub flat_speed: i64,         // VP/DT: 0, 10, 15, 20, 30, 35
    pub pet_level: i64,          // 0–5
    pub zinman_level: i64,       // 0–5
    pub expert_level: i64,       // 0–5
}

impl BuildingBuffs {
    pub fn is_empty(&self) -> bool {
        self.construction_speed == 0
            && self.flat_speed == 0
            && self.pet_level == 0
            && self.zinman_level == 0
            && self.expert_level == 0
    }

    pub fn total_speed_pct(&self) -> i64 {
        self.construction_speed + self.flat_speed + PET_SPEED[clamp5(self.pet_level) as usize]
    }

    pub fn line(&self) -> String {
        let mut parts = Vec::new();
        if self.construction_speed > 0 {
            parts.push(format!("Speed {}%", self.construction_speed));
        }
        if self.flat_speed > 0 {
            let label = match self.flat_speed {
                10 => "VP +10%".to_string(),
                15 => "VP +15%".to_string(),
                20 => "Double Time +20%".to_string(),
                30 => "VP +10% + DT".to_string(),
                35 => "VP +15% + DT".to_string(),
                other => format!("+{other}%"),
            };
            parts.push(label);
        }
        if self.pet_level > 0 {
            parts.push(format!("Pet Lv{}", self.pet_level));
        }
        if self.zinman_level > 0 {
            parts.push(format!("Zinman Lv{}", self.zinman_level));
        }
        if self.expert_level > 0 {
            parts.push(format!("Expert Lv{}", self.expert_level));
        }
        parts.join(" · ")
    }
}

fn clamp5(v: i64) -> i64 {
    v.clamp(0, 5)
}

/// Zinman discount applies to basic RSS only; time speed divides the duration.
#[derive(Debug, Clone, Default)]
pub struct BuffedTotals {
    pub totals: BuildingTotals,
    pub base_seconds: i64,
    pub reduced_seconds: i64,
}

pub fn apply_buffs(raw: &BuildingTotals, buffs: &BuildingBuffs) -> BuffedTotals {
    let reduction = ZINMAN_REDUCE[clamp5(buffs.zinman_level) as usize] as f64 / 100.0;
    let mut totals = raw.clone();
    if reduction > 0.0 {
        totals.meat = ((totals.meat as f64) * (1.0 - reduction)).ceil() as i64;
        totals.wood = ((totals.wood as f64) * (1.0 - reduction)).ceil() as i64;
        totals.coal = ((totals.coal as f64) * (1.0 - reduction)).ceil() as i64;
        totals.iron = ((totals.iron as f64) * (1.0 - reduction)).ceil() as i64;
    }

    let total_speed = buffs.total_speed_pct();
    let speed_reduced = if total_speed > 0 {
        ((raw.time_seconds as f64) / (1.0 + total_speed as f64 / 100.0)).ceil() as i64
    } else {
        raw.time_seconds
    };
    let expert = EXPERT_SECS[clamp5(buffs.expert_level) as usize] * raw.num_levels;
    let reduced_seconds = (speed_reduced - expert).max(0);

    BuffedTotals {
        totals,
        base_seconds: raw.time_seconds,
        reduced_seconds,
    }
}

// ---------------------------------------------------------------- sessions

const SESSION_TTL_MS: u128 = 15 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: u64,
    pub building_key: String,
    pub from: i64,
    pub to: i64,
}

#[derive(Debug, Clone)]
pub struct BuildingSession {
    pub buffs: BuildingBuffs,
    pub entries: Vec<Entry>,
    pub next_id: u64,
    pub touched_ms: u128,
}

impl Default for BuildingSession {
    fn default() -> Self {
        Self {
            buffs: BuildingBuffs::default(),
            entries: Vec::new(),
            next_id: 1,
            touched_ms: crate::calculator::svs::now_ms(),
        }
    }
}

fn sessions() -> &'static Mutex<HashMap<u64, BuildingSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<u64, BuildingSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn session_for(user_id: u64) -> BuildingSession {
    let now = crate::calculator::svs::now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    let session = guard.entry(user_id).or_default();
    session.touched_ms = now;
    session.clone()
}

pub fn update_session<F>(user_id: u64, update: F)
where
    F: FnOnce(&mut BuildingSession),
{
    let now = crate::calculator::svs::now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    let session = guard.entry(user_id).or_default();
    session.touched_ms = now;
    update(session);
}

// ------------------------------------------------------------------- UI

pub fn panel_embed() -> CreateEmbed {
    let names: Vec<&str> = building_list().iter().map(|(_, label)| *label).collect();
    CreateEmbed::new()
        .title("🏗️ Building Calculator — Costs & upgrade calculator")
        .description(format!(
            "Plan building upgrades with verified cost tables (full 1–30 + Fire Crystal tiers).\n\n**Available**\n{}\n\n**How to use**\n🌐 **Translate** — private guide in your language (optional)\n📊 **Calculate** — build a multi-entry plan, set buffs, see combined totals\n   Levels: plain numbers or FC tiers (`FC1`, `FC 2-3`, `30-1`)\n\n**Buffs**\nConstruction speed %, VP/Double Time, Pet, Zinman (RSS −%), Expert (time −)",
            names.join(" · ")
        ))
        .color(0xf4a83f)
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn panel_row(message_id: u64) -> serenity::CreateActionRow {
    serenity::CreateActionRow::Buttons(vec![
        crate::ui::components::primary("buildingcalc:open", "📊 Calculate"),
        crate::translate::translate_button(message_id),
    ])
}

/// Post (or repost) the persistent panel in the configured channel.
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
            kind: "building_panel",
            embed_json: stored_json,
            content: None,
            owner_id: None,
        },
    )
    .await?;

    crate::db::calculator::set_channel(
        &data.db,
        guild_id,
        "building",
        Some(channel_id),
        Some(message.id.get()),
    )
    .await?;
    Ok(message.id.get())
}

pub fn plan_embed(session: &BuildingSession) -> CreateEmbed {
    let mut lines = Vec::new();
    for entry in &session.entries {
        let label = building(&entry.building_key)
            .map(|b| b.label.clone())
            .unwrap_or_else(|| entry.building_key.clone());
        lines.push(format!(
            "• **{}** — {} → {}",
            label,
            level_display(entry.from),
            level_display(entry.to)
        ));
    }
    if lines.is_empty() {
        lines.push("_No buildings yet — add one to start your plan._".to_string());
    }
    let buff_line = if session.buffs.is_empty() {
        "🧙 **Buffs:** None".to_string()
    } else {
        format!("🧙 **Buffs:** {}", session.buffs.line())
    };

    CreateEmbed::new()
        .title("🏗️ Building plan")
        .description(format!("{}\n\n{}", lines.join("\n"), buff_line))
        .color(0xf4a83f)
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn totals_embed(session: &BuildingSession) -> Option<CreateEmbed> {
    if session.entries.is_empty() {
        return None;
    }

    let mut combined = BuildingTotals::default();
    for entry in &session.entries {
        if let Some(range) = compute_range(&entry.building_key, entry.from, entry.to) {
            combined.meat += range.meat;
            combined.wood += range.wood;
            combined.coal += range.coal;
            combined.iron += range.iron;
            combined.fire_crystal += range.fire_crystal;
            combined.refined_fire_crystal += range.refined_fire_crystal;
            combined.time_seconds += range.time_seconds;
            combined.num_levels += range.num_levels;
            for (bkey, lvl) in range.requirements {
                let acc = combined.requirements.entry(bkey).or_insert(0);
                *acc = (*acc).max(lvl);
            }
        }
    }
    let buffed = apply_buffs(&combined, &session.buffs);

    let mut materials = Vec::new();
    let mut push = |label: String, base: i64, discounted: i64, icon: &str| {
        if base > 0 || discounted > 0 {
            let suffix = if base != discounted {
                format!(" (was {})", format_game_amount(base))
            } else {
                String::new()
            };
            materials.push(format!(
                "{icon} **{label}:** {}{suffix}",
                format_game_amount(discounted)
            ));
        }
    };
    push("Meat".into(), combined.meat, buffed.totals.meat, "🍖");
    push(
        "Wood".into(),
        combined.wood,
        buffed.totals.wood,
        &crate::emoji::material_tag("wood"),
    );
    push("Coal".into(), combined.coal, buffed.totals.coal, "🪨");
    push(
        "Iron".into(),
        combined.iron,
        buffed.totals.iron,
        &crate::emoji::material_tag("iron"),
    );
    if combined.fire_crystal > 0 {
        materials.push(format!(
            "{} **Fire Crystal:** {}",
            crate::emoji::material_tag("fireCrystal"),
            format_game_amount(combined.fire_crystal)
        ));
    }
    if combined.refined_fire_crystal > 0 {
        materials.push(format!(
            "{} **Refined FC:** {}",
            crate::emoji::material_tag("refinedFC"),
            format_game_amount(combined.refined_fire_crystal)
        ));
    }
    if materials.is_empty() {
        materials.push("_Nothing required_".to_string());
    }

    let req_lines: Vec<String> = combined
        .requirements
        .iter()
        .map(|(key, lvl)| {
            let label = building(key)
                .map(|b| b.label.clone())
                .unwrap_or_else(|| key.clone());
            format!("• **{label}** — Lv {lvl}")
        })
        .collect();

    let time_base = format_game_duration(combined.time_seconds);
    let time_buffed = format_game_duration(buffed.reduced_seconds);
    let time_suffix = if buffed.reduced_seconds != buffed.base_seconds {
        format!(" (base {time_base})")
    } else {
        String::new()
    };

    let mut embed = CreateEmbed::new()
        .title("📊 Building plan — Totals")
        .color(0xf4a83f)
        .description(materials.join("\n"))
        .field("⏱️ Time", format!("**{time_buffed}**{time_suffix}"), true);
    if !session.buffs.is_empty() {
        embed = embed.field(
            "🧙 Buffs applied",
            format!("Speed {}% total", session.buffs.total_speed_pct()),
            true,
        );
    }
    if !req_lines.is_empty() {
        embed = embed.field("🏗️ Requires", req_lines.join("\n"), false);
    }
    embed = embed.footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));
    Some(embed)
}

pub fn format_game_duration(total_seconds: i64) -> String {
    if total_seconds <= 0 {
        return "0s".to_string();
    }
    let mut remaining = total_seconds;
    let days = remaining / 86_400;
    remaining %= 86_400;
    let hours = remaining / 3_600;
    remaining %= 3_600;
    let minutes = remaining / 60;
    let seconds = remaining % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}
