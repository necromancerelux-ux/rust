//! Companion Animals (Pets) calculator — level plan + inventory check.
//!
//! Mirrors the legacy `petCalculator` UX: panel → private session →
//! pick a pet → set Current / Target level → optional inventory check →
//! full report. All data lives in embedded `pet.json`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use poise::serenity_prelude as serenity;
use serde::Deserialize;
use serenity::CreateEmbed;

use crate::calculator::game_numbers::format_game_amount;
use crate::ui::components;

const DATA: &str = include_str!("data/pet.json");

// ------------------------------------------------------------------ data

#[derive(Debug, Clone, Default)]
pub struct Pet {
    pub key: String,
    pub label: String,
    pub rarity: String,
    pub max_level: i64,
    pub milestones: HashMap<i64, Milestone>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Milestone {
    #[serde(default)]
    pub food: f64,
    #[serde(default)]
    pub manual: f64,
    #[serde(default)]
    pub potion: f64,
    #[serde(default)]
    pub serum: f64,
    #[serde(rename = "advancementScore", default)]
    pub advancement_score: f64,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct RawPetData {
    pets: HashMap<String, RawPet>,
    order: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct RawPet {
    key: String,
    label: String,
    rarity: String,
    maxLevel: i64,
    milestones: HashMap<String, Milestone>,
}

static PETS: OnceLock<Vec<Pet>> = OnceLock::new();
static PET_INDEX: OnceLock<HashMap<String, usize>> = OnceLock::new();
static PARSED: OnceLock<RawPetData> = OnceLock::new();

fn parsed() -> &'static RawPetData {
    PARSED.get_or_init(|| serde_json::from_str(DATA).expect("pet.json is valid"))
}

/// Milestone keys: integer levels stored as `level * 10`, advancement rows
/// (`10.1`, `20.1`, …) stored as `level * 10 + 1`. One numeric space.
fn milestone_key(float_key: f64) -> i64 {
    (float_key * 10.0).round() as i64
}

pub fn pets() -> &'static [Pet] {
    PETS.get_or_init(|| {
        let raw = parsed();
        raw.order
            .iter()
            .filter_map(|key| raw.pets.get(key))
            .map(|pet| {
                let milestones = pet
                    .milestones
                    .iter()
                    .filter_map(|(k, milestone)| {
                        let float_key: f64 = k.parse().ok()?;
                        Some((milestone_key(float_key), milestone.clone()))
                    })
                    .collect();
                Pet {
                    key: pet.key.clone(),
                    label: pet.label.clone(),
                    rarity: pet.rarity.clone(),
                    max_level: pet.maxLevel,
                    milestones,
                }
            })
            .collect()
    })
}

fn index() -> &'static HashMap<String, usize> {
    PET_INDEX.get_or_init(|| {
        pets()
            .iter()
            .enumerate()
            .map(|(index, pet)| (pet.key.clone(), index))
            .collect()
    })
}

pub fn get_pet(key: &str) -> Option<&'static Pet> {
    index().get(key).map(|idx| &pets()[*idx])
}

fn food_milestones(pet: &Pet) -> Vec<i64> {
    let mut keys: Vec<i64> = pet
        .milestones
        .keys()
        .filter(|key| **key % 10 == 0)
        .map(|key| key / 10)
        .collect();
    keys.sort_unstable();
    keys
}

fn cumulative_food(pet: &Pet, level: i64) -> f64 {
    if level <= 0 {
        return 0.0;
    }
    let mut keys: Vec<i64> = food_milestones(pet);
    keys.sort_unstable();

    let mut total = 0.0;
    let mut prev = 0i64;
    for m in keys {
        let seg = pet.milestones.get(&(m * 10)).map(|m| m.food).unwrap_or(0.0);
        if level >= m {
            total += seg;
            prev = m;
        } else {
            let span = (m - prev) as f64;
            if span > 0.0 && level > prev {
                total += (seg * (level - prev) as f64 / span).round();
            }
            return total;
        }
    }
    total
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevelRange {
    pub from: i64,
    pub to: i64,
    pub food: f64,
    pub manual: f64,
    pub potion: f64,
    pub serum: f64,
    pub advancement_score: f64,
}

pub fn compute_level_range(pet: &Pet, from_level: i64, to_level: i64) -> LevelRange {
    let from = from_level.max(0);
    let to = to_level.clamp(0, pet.max_level);
    let mut out = LevelRange {
        from,
        to,
        ..Default::default()
    };
    if to <= from {
        return out;
    }
    out.food = (cumulative_food(pet, to) - cumulative_food(pet, from)).max(0.0);
    for (key, milestone) in &pet.milestones {
        if key % 10 != 1 {
            continue;
        }
        let base = key / 10;
        if base > from && base <= to {
            out.manual += milestone.manual;
            out.potion += milestone.potion;
            out.serum += milestone.serum;
            out.advancement_score += milestone.advancement_score;
        }
    }
    out
}

/// Options for the level pickers: 0 plus every 5-level mark up to max.
pub fn level_options(max_level: i64) -> Vec<i64> {
    let mut levels = vec![0i64];
    let mut level = 5;
    while level <= max_level {
        levels.push(level);
        level += 5;
    }
    if *levels.last().unwrap_or(&0) != max_level {
        levels.push(max_level);
    }
    if levels.len() > 25 {
        levels.truncate(24);
        levels.push(max_level);
        levels.sort_unstable();
        levels.dedup();
    }
    levels
}

// --------------------------------------------------------------- session

pub const SESSION_TTL_MS: u128 = 15 * 60 * 1000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PetPlan {
    pub from: i64,
    pub to: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Inventory {
    pub food: i64,
    pub manual: i64,
    pub potion: i64,
    pub serum: i64,
}

#[derive(Debug, Clone)]
pub struct PetSession {
    pub selected_key: String,
    pub plans: HashMap<String, PetPlan>,
    pub inventory: Inventory,
    pub touched_ms: u128,
}

impl Default for PetSession {
    fn default() -> Self {
        Self {
            selected_key: pets()
                .first()
                .map(|pet| pet.key.clone())
                .unwrap_or_default(),
            plans: HashMap::new(),
            inventory: Inventory::default(),
            touched_ms: 0,
        }
    }
}

fn sessions() -> &'static Mutex<HashMap<u64, PetSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<u64, PetSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn now_ms() -> u128 {
    crate::calculator::svs::now_ms()
}

pub fn session_for(user_id: u64) -> PetSession {
    let now = now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    let session = guard.entry(user_id).or_default();
    session.touched_ms = now;
    session.clone()
}

pub fn update_session<F>(user_id: u64, update: F)
where
    F: FnOnce(&mut PetSession),
{
    let now = now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    let session = guard.entry(user_id).or_default();
    session.touched_ms = now;
    update(session);
}

impl PetSession {
    pub fn plan_for(&self, key: &str) -> PetPlan {
        self.plans.get(key).cloned().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Aggregate {
    pub food: f64,
    pub manual: f64,
    pub potion: f64,
    pub serum: f64,
    pub advancement_score: f64,
    /// (pet_key, plan, result)
    pub lines: Vec<(String, PetPlan, LevelRange)>,
}

pub fn aggregate(session: &PetSession) -> Aggregate {
    let mut out = Aggregate::default();
    for (key, plan) in &session.plans {
        let Some(pet) = get_pet(key) else { continue };
        let range = compute_level_range(pet, plan.from, plan.to);
        if range.to <= range.from {
            continue;
        }
        out.food += range.food;
        out.manual += range.manual;
        out.potion += range.potion;
        out.serum += range.serum;
        out.advancement_score += range.advancement_score;
        out.lines.push((key.clone(), plan.clone(), range));
    }
    // Stable display order: the PET_ORDER list, not hash order.
    out.lines
        .sort_by_key(|(key, _, _)| index().get(key).copied().unwrap_or(usize::MAX));
    out
}

// ------------------------------------------------------------------ view

pub fn panel_embed() -> CreateEmbed {
    let names = pets()
        .iter()
        .map(|pet| pet.label.as_str())
        .collect::<Vec<_>>()
        .join(" · ");
    let names: String = names.chars().take(280).collect();

    CreateEmbed::new()
        .title("🐾 Companion Animals Calculator")
        .color(0x57c964)
        .description(format!(
            "Plan **Pet Food**, **Taming Manuals**, **Energizing Potions**, **Strengthening Serums** and **Pet Advancement Score** for all companion animals.\n\n**How to use**\n1. Tap **Calculator** (private session)\n2. Pick a **pet**\n3. Set **current** and **target** level\n4. Optional: **Inventory** check\n5. **Full report** for the combined breakdown\n\n**All {count} pets**\n{names}",
            count = pets().len(),
            names = names,
        ))
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn panel_row(message_id: u64) -> serenity::CreateActionRow {
    serenity::CreateActionRow::Buttons(vec![
        crate::translate::translate_button(message_id),
        components::primary("petcalc:open", "📊 Calculator"),
    ])
}

/// Post (or repost) the persistent pets panel.
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
            kind: "pets",
            embed_json: stored_json,
            content: None,
            owner_id: None,
        },
    )
    .await?;

    crate::db::calculator::set_channel(
        &data.db,
        guild_id,
        crate::db::calculator::PETS,
        Some(channel_id),
        Some(message.id.get()),
    )
    .await?;
    Ok(())
}

pub fn plan_embed(session: &PetSession) -> CreateEmbed {
    let pet = get_pet(&session.selected_key)
        .or_else(|| pets().first())
        .expect("pets data is not empty");
    let plan = session.plan_for(&pet.key);
    let totals = aggregate(session);
    let inv = &session.inventory;

    let mut description = vec![
        format!(
            "**Selected pet**\n🐾 **{}** · {} · max Lv {}",
            pet.label, pet.rarity, pet.max_level
        ),
        format!("**Levels:** {} → {}", plan.from, plan.to),
        String::new(),
        "📊 **Resources required**".to_string(),
        format!(
            "🍽️ Pet Food: **{}**",
            format_game_amount(totals.food.round() as i64)
        ),
        format!(
            "📜 Taming Manuals: **{}**",
            format_game_amount(totals.manual.round() as i64)
        ),
        format!(
            "🧪 Energizing Potions: **{}**",
            format_game_amount(totals.potion.round() as i64)
        ),
        format!(
            "💉 Strengthening Serums: **{}**",
            format_game_amount(totals.serum.round() as i64)
        ),
        format!(
            "✨ Advancement Score: **{}**",
            format_game_amount(totals.advancement_score.round() as i64)
        ),
        String::new(),
        format!(
            "🎒 **Your stock**\n🍽️ {} · 📜 {} · 🧪 {} · 💉 {}",
            inv.food, inv.manual, inv.potion, inv.serum
        ),
    ];

    let mut shortages = Vec::new();
    let mut need = |have: i64, req: f64, label: &str| {
        let req = req.round() as i64;
        if req > have {
            shortages.push(format!(
                "{label}: need **{}**, have **{}**, short **{}**",
                format_game_amount(req),
                format_game_amount(have),
                format_game_amount(req - have)
            ));
        }
    };
    need(inv.food, totals.food, "Pet Food");
    need(inv.manual, totals.manual, "Taming Manuals");
    need(inv.potion, totals.potion, "Energizing Potions");
    need(inv.serum, totals.serum, "Strengthening Serums");

    if !shortages.is_empty() {
        description.push(String::new());
        description.push("⚠️ **Shortage**".to_string());
        description.extend(shortages);
    } else if totals.lines.is_empty() {
        description.push(String::new());
        description.push(
            "_Nothing planned yet — set a target level above current on at least one pet._"
                .to_string(),
        );
    }

    CreateEmbed::new()
        .title("🐾 Companion Animals")
        .color(0x57c964)
        .description(
            description
                .join("\n")
                .chars()
                .take(4000)
                .collect::<String>(),
        )
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn plan_rows(session: &PetSession) -> Vec<serenity::CreateActionRow> {
    use serenity::{CreateActionRow, CreateSelectMenu, CreateSelectMenuKind};

    let pet = get_pet(&session.selected_key)
        .or_else(|| pets().first())
        .expect("pets data is not empty");
    let plan = session.plan_for(&pet.key);

    let pet_options: Vec<serenity::CreateSelectMenuOption> = pets()
        .iter()
        .take(25)
        .map(|pet| {
            serenity::CreateSelectMenuOption::new(pet.label.clone(), pet.key.clone())
                .description(format!("{} · max Lv {}", pet.rarity, pet.max_level))
                .default_selection(pet.key == session.selected_key)
        })
        .collect();
    let level_options = level_options(pet.max_level);
    let level_menu = |selected: i64| -> Vec<serenity::CreateSelectMenuOption> {
        level_options
            .iter()
            .map(|level| {
                serenity::CreateSelectMenuOption::new(format!("Lv {level}"), level.to_string())
                    .default_selection(*level == selected)
            })
            .collect()
    };

    vec![
        CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                "petcalc:select_pet",
                CreateSelectMenuKind::String {
                    options: pet_options,
                },
            )
            .placeholder("Choose pet")
            .min_values(1)
            .max_values(1),
        ),
        CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                "petcalc:select_from",
                CreateSelectMenuKind::String {
                    options: level_menu(plan.from),
                },
            )
            .placeholder("Current level")
            .min_values(1)
            .max_values(1),
        ),
        CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                "petcalc:select_to",
                CreateSelectMenuKind::String {
                    options: level_menu(plan.to),
                },
            )
            .placeholder("Target level")
            .min_values(1)
            .max_values(1),
        ),
        CreateActionRow::Buttons(vec![
            components::secondary("petcalc:inventory", "🎒 Inventory"),
            components::success("petcalc:report", "📋 Full report"),
            components::danger("petcalc:reset", "🗑️ Reset all"),
        ]),
    ]
}

pub fn report_embed(session: &PetSession) -> CreateEmbed {
    let aggregate = aggregate(session);
    if aggregate.lines.is_empty() {
        return CreateEmbed::new()
            .title("🐾 Companion Animals — Full report")
            .description("_Nothing planned yet._")
            .color(0xf4c542)
            .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));
    }

    let mut lines: Vec<String> = Vec::new();
    for (key, _plan, range) in &aggregate.lines {
        let pet = get_pet(key).expect("aggregated pet exists");
        let name = pet.label.clone();
        lines.push(format!(
            "**{name}** ({}) — Lv {} → {}\n🍽️ {} · 📜 {} · 🧪 {} · 💉 {} · ✨ {}",
            pet.rarity,
            range.from,
            range.to,
            format_game_amount(range.food.round() as i64),
            format_game_amount(range.manual.round() as i64),
            format_game_amount(range.potion.round() as i64),
            format_game_amount(range.serum.round() as i64),
            format_game_amount(range.advancement_score.round() as i64),
        ));
    }
    lines.push(String::new());
    lines.push("**Grand totals**".to_string());
    lines.push(format!(
        "🍽️ {} · 📜 {} · 🧪 {} · 💉 {} · ✨ {}",
        format_game_amount(aggregate.food.round() as i64),
        format_game_amount(aggregate.manual.round() as i64),
        format_game_amount(aggregate.potion.round() as i64),
        format_game_amount(aggregate.serum.round() as i64),
        format_game_amount(aggregate.advancement_score.round() as i64),
    ));

    CreateEmbed::new()
        .title("🐾 Companion Animals — Full report")
        .color(0xf4c542)
        .description(lines.join("\n").chars().take(4000).collect::<String>())
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn report_text(session: &PetSession) -> String {
    let aggregate = aggregate(session);
    let mut lines: Vec<String> = Vec::new();
    for (key, _plan, range) in &aggregate.lines {
        let pet = get_pet(key).expect("aggregated pet exists");
        lines.push(format!(
            "{} ({}) — Lv {} -> {} | Food {} | Manuals {} | Potions {} | Serums {} | Score {}",
            pet.label,
            pet.rarity,
            range.from,
            range.to,
            format_game_amount(range.food.round() as i64),
            format_game_amount(range.manual.round() as i64),
            format_game_amount(range.potion.round() as i64),
            format_game_amount(range.serum.round() as i64),
            format_game_amount(range.advancement_score.round() as i64),
        ));
    }
    lines.push(format!(
        "TOTAL | Food {} | Manuals {} | Potions {} | Serums {} | Score {}",
        format_game_amount(aggregate.food.round() as i64),
        format_game_amount(aggregate.manual.round() as i64),
        format_game_amount(aggregate.potion.round() as i64),
        format_game_amount(aggregate.serum.round() as i64),
        format_game_amount(aggregate.advancement_score.round() as i64),
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cave_hyena() -> &'static Pet {
        get_pet("cave_hyena").expect("cave hyena data")
    }

    #[test]
    fn food_milestone_lookup() {
        let pet = cave_hyena();
        let ms = food_milestones(pet);
        assert_eq!(ms, vec![10, 20, 30, 40, 50]);
        assert_eq!(cumulative_food(pet, 0), 0.0);
        assert_eq!(cumulative_food(pet, 10), 1715.0);
        assert_eq!(cumulative_food(pet, 20), 1715.0 + 3180.0);
        assert_eq!(
            cumulative_food(pet, 50),
            1715.0 + 3180.0 + 5010.0 + 7660.0 + 11310.0
        );
    }

    #[test]
    fn range_costs_match_legacy() {
        let pet = cave_hyena();
        // 0 -> 10: exactly one segment of food, plus the 10.1 advancement row.
        let range = compute_level_range(pet, 0, 10);
        assert_eq!(range.food, 1715.0);
        assert_eq!(range.manual, 15.0);
        assert_eq!(range.advancement_score, 500.0);

        // 15 -> 25: half of the 10->20 segment (1590) + half of the 20->30
        // segment (2505) = 4095, plus the 20.1 advancement row.
        let range = compute_level_range(pet, 15, 25);
        assert_eq!(range.food, 1590.0 + 2505.0); // 3180/2 + round(5010/2)
        assert_eq!(range.manual, 30.0);
        assert_eq!(range.advancement_score, 1000.0);
    }

    #[test]
    fn level_options_stay_under_25() {
        for pet in pets() {
            let options = level_options(pet.max_level);
            assert!(options.len() <= 25, "pet {} exceeds 25 options", pet.key);
            assert_eq!(options[0], 0);
            assert_eq!(*options.last().unwrap(), pet.max_level);
        }
    }
}
