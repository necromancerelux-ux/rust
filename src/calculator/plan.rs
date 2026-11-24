//! Generic multi-entry "upgrade plan" calculator shared by Chief Gear and
//! Chief Charm. Mirrors the legacy plan-hub flow:
//!
//!   panel post → Calculate → plan hub → Add → pick item → Current → Target
//!   → Add to plan → Show totals.
//!
//! Sessions are private to each user and kept in memory for 15 minutes.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use poise::serenity_prelude as serenity;
use serenity::CreateEmbed;

use crate::calculator::game_numbers::format_game_amount;
use crate::ui::components;

#[derive(Debug, Clone, Copy)]
pub struct PieceSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub multiplier: i64,
}

#[derive(Debug, Clone)]
pub struct MaterialSpec {
    pub field: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone)]
pub struct StatSpec {
    pub field: &'static str,
    pub label: &'static str,
    pub percent: bool,
    pub prefix: &'static str,
}

impl StatSpec {
    pub const fn new(
        field: &'static str,
        label: &'static str,
        percent: bool,
        prefix: &'static str,
    ) -> Self {
        Self {
            field,
            label,
            percent,
            prefix,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StepData {
    pub materials: HashMap<&'static str, f64>,
    pub stats: HashMap<&'static str, f64>,
}

#[derive(Debug, Clone)]
pub struct PlanConfig {
    pub calc_key: &'static str,
    pub title: &'static str,
    pub panel_description: &'static str,
    pub emoji: &'static str,
    pub color: u32,
    /// Item options (pieces / slots), loaded from embedded JSON.
    pub pieces: fn() -> &'static [PieceSpec],
    /// Level order + labels, loaded from embedded JSON.
    pub levels: fn() -> &'static [(&'static str, &'static str)],
    pub steps: fn() -> &'static HashMap<&'static str, StepData>,
    /// Summed into "svsScore"; also used as the entry-multiplier base stat.
    pub svs_field: &'static str,
    pub materials: &'static [MaterialSpec],
    pub stat_gains: &'static [StatSpec],
}

impl PlanConfig {
    pub fn label_of(&self, key: &str) -> String {
        (self.levels)()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, label)| label.to_string())
            .unwrap_or_else(|| key.to_string())
    }

    pub fn piece_label(&self, key: &str) -> String {
        (self.pieces)()
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.label.to_string())
            .unwrap_or_else(|| key.to_string())
    }

    pub fn piece_multiplier(&self, key: &str) -> f64 {
        (self.pieces)()
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.multiplier as f64)
            .unwrap_or(1.0)
    }
}

// -------------------------------------------------------------- sessions

pub const SESSION_TTL_MS: u128 = 15 * 60 * 1000;

#[derive(Debug, Clone, Default)]
pub struct AddState {
    pub slots_key: String,
    pub current: Option<String>,
    pub target: Option<String>,
    pub current_page: i64,
    pub target_page: i64,
}

#[derive(Debug, Clone)]
pub struct PlanEntry {
    pub id: u64,
    pub slots_key: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default)]
pub struct PlanSession {
    pub add: Option<AddState>,
    pub entries: Vec<PlanEntry>,
    pub next_id: u64,
    pub touched_ms: u128,
}

pub fn now_ms() -> u128 {
    crate::calculator::svs::now_ms()
}

fn sessions() -> &'static Mutex<HashMap<(u8, u64), PlanSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<(u8, u64), PlanSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn session_for(kind: u8, user_id: u64) -> PlanSession {
    let now = now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    let session = guard.entry((kind, user_id)).or_default();
    session.touched_ms = now;
    session.clone()
}

pub fn update_session<F>(kind: u8, user_id: u64, update: F)
where
    F: FnOnce(&mut PlanSession),
{
    let now = now_ms();
    let mut guard = sessions().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, session| now.saturating_sub(session.touched_ms) < SESSION_TTL_MS);
    let session = guard.entry((kind, user_id)).or_default();
    session.touched_ms = now;
    update(session);
}

// ------------------------------------------------------------------ math

#[derive(Debug, Clone, Default)]
pub struct RangeTotals {
    pub materials: HashMap<&'static str, f64>,
    pub gains: HashMap<&'static str, f64>,
    pub svs: f64,
}

pub fn compute_range(
    config: &PlanConfig,
    from_key: &str,
    to_key: &str,
    multiplier: f64,
) -> Option<RangeTotals> {
    let positions: HashMap<&str, usize> = (config.levels)()
        .iter()
        .enumerate()
        .map(|(i, (k, _))| (*k, i))
        .collect();
    let from_idx = *positions.get(from_key)?;
    let to_idx = *positions.get(to_key)?;
    if to_idx <= from_idx {
        return None;
    }

    let steps = (config.steps)();
    let order: Vec<&str> = (config.levels)().iter().map(|(k, _)| *k).collect();

    let mut totals = RangeTotals::default();
    for level in order.iter().take(to_idx + 1).skip(from_idx + 1) {
        let Some(step) = steps.get(*level) else {
            continue;
        };
        for (field, amount) in &step.materials {
            *totals.materials.entry(field).or_insert(0.0) += amount;
        }
        if let Some(svs) = step.stats.get(config.svs_field) {
            *totals.materials.entry(config.svs_field).or_insert(0.0) += svs;
        }
    }

    let from_step = steps.get(from_key).cloned().unwrap_or_default();
    let to_step = steps.get(to_key).cloned().unwrap_or_default();
    for spec in config.stat_gains {
        let from = from_step.stats.get(spec.field).copied().unwrap_or(0.0);
        let to = to_step.stats.get(spec.field).copied().unwrap_or(0.0);
        totals.gains.insert(spec.field, to - from);
    }

    for value in totals.materials.values_mut() {
        *value *= multiplier;
    }
    for value in totals.gains.values_mut() {
        *value *= multiplier;
    }
    totals.svs = totals
        .materials
        .get(config.svs_field)
        .copied()
        .unwrap_or(0.0);
    Some(totals)
}

pub fn material_amount(totals: &RangeTotals, field: &str) -> i64 {
    totals
        .materials
        .iter()
        .find(|(k, _)| **k == field)
        .map(|(_, v)| v.round() as i64)
        .unwrap_or(0)
}

/// Build a field title: custom material emoji (when installed) + plain name.
/// Falls back to the static `MaterialSpec.label` when the field has no icon key.
fn material_field_title(field: &str, fallback_label: &str) -> String {
    // Map calculator field keys → emoji module keys.
    let emoji_key = match field {
        "hardenedAlloy" => Some("hardenedAlloy"),
        "polishingSolution" => Some("polishingSolution"),
        "designPlans" => Some("designPlans"),
        "lunarAmber" => Some("lunarAmber"),
        "charmGuides" => Some("charmGuides"),
        "charmDesigns" => Some("charmDesigns"),
        "charmSecrets" | "jewelSecrets" => Some("charmSecrets"),
        "wood" => Some("wood"),
        "iron" => Some("iron"),
        "fireCrystal" => Some("fireCrystal"),
        "refinedFC" => Some("refinedFC"),
        _ => None,
    };
    match emoji_key {
        Some(key) => {
            let tag = crate::emoji::material_tag(key);
            // Strip any leading emoji/symbol from the static label so we don't
            // double-icon when the custom tag is still the Unicode fallback.
            let name = fallback_label
                .chars()
                .skip_while(|c| !c.is_ascii_alphanumeric())
                .collect::<String>()
                .trim()
                .to_string();
            if name.is_empty() {
                fallback_label.to_string()
            } else {
                format!("{tag} {name}")
            }
        }
        None => fallback_label.to_string(),
    }
}

// ------------------------------------------------------------------ view

pub fn level_options(
    config: &PlanConfig,
    after: Option<&str>,
) -> Vec<serenity::CreateSelectMenuOption> {
    let start = after
        .and_then(|current| (config.levels)().iter().position(|(k, _)| *k == current))
        .map(|pos| pos + 1)
        .unwrap_or(0);
    (config.levels)()[start..]
        .iter()
        .map(|(key, label)| serenity::CreateSelectMenuOption::new(*label, *key))
        .collect()
}

pub fn page_slice(total: usize, page: i64) -> (usize, usize, i64, i64) {
    let pages = (total.div_ceil(25).max(1)) as i64;
    let page = page.clamp(0, pages - 1);
    let start = (page * 25) as usize;
    let end = (start + 25).min(total);
    (start, end, page, pages)
}

/// Row layout shared by both calculators' "add" screens.
pub fn add_rows(
    config: &PlanConfig,
    state: &AddState,
    namespace: &str,
) -> Vec<serenity::CreateActionRow> {
    use serenity::{CreateActionRow, CreateSelectMenu, CreateSelectMenuKind};

    let mut rows = Vec::new();

    let current_options = level_options(config, None);
    let (c_start, c_end, c_page, _c_pages) = page_slice(current_options.len(), state.current_page);
    rows.push(CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            format!("{namespace}:current_select"),
            CreateSelectMenuKind::String {
                options: current_options[c_start..c_end].to_vec(),
            },
        )
        .placeholder("Current level")
        .min_values(1)
        .max_values(1),
    ));
    rows.push(CreateActionRow::Buttons(vec![
        components::secondary(format!("{namespace}:currentnav:{}", c_page - 1), "◀ Prev"),
        components::secondary(format!("{namespace}:currentnav:{}", c_page + 1), "Next ▶"),
    ]));

    let target_options = level_options(config, state.current.as_deref());
    let (t_start, t_end, t_page, t_pages) = page_slice(target_options.len(), state.target_page);
    if !target_options.is_empty() {
        rows.push(CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                format!("{namespace}:target_select"),
                CreateSelectMenuKind::String {
                    options: target_options[t_start..t_end].to_vec(),
                },
            )
            .placeholder("Target level")
            .min_values(1)
            .max_values(1),
        ));
    }
    let level_index = |key: &str| (config.levels)().iter().position(|(k, _)| *k == key);
    let can_add = state
        .current
        .as_deref()
        .and_then(level_index)
        .zip(state.target.as_deref().and_then(level_index))
        .is_some_and(|(from, to)| to > from);

    rows.push(CreateActionRow::Buttons(vec![
        components::secondary(format!("{namespace}:targetnav:{}", t_page - 1), "◀ Prev"),
        components::secondary(format!("{namespace}:targetnav:{}", t_page + 1), "Next ▶"),
        {
            let button = components::success(format!("{namespace}:confirm_add"), "✓ Add to plan");
            button.disabled(!can_add)
        },
    ]));
    rows.push(CreateActionRow::Buttons(vec![components::secondary(
        format!("{namespace}:backplan"),
        "↩ Back to plan",
    )]));

    let _ = t_pages;

    rows
}

/// The plan hub: entries list + action buttons (+ remove select).
pub fn plan_rows(
    config: &PlanConfig,
    session: &PlanSession,
    namespace: &str,
) -> Vec<serenity::CreateActionRow> {
    use serenity::{CreateActionRow, CreateSelectMenu, CreateSelectMenuKind};

    let mut rows = vec![
        CreateActionRow::Buttons(vec![
            components::primary(format!("{namespace}:add"), "➕ Add"),
            components::success(format!("{namespace}:totals"), "📊 Show totals"),
        ]),
        CreateActionRow::Buttons(vec![components::danger(
            format!("{namespace}:clear"),
            "🗑️ Clear plan",
        )]),
    ];

    if !session.entries.is_empty() {
        let options: Vec<serenity::CreateSelectMenuOption> = session
            .entries
            .iter()
            .take(25)
            .map(|entry| {
                serenity::CreateSelectMenuOption::new(
                    format!(
                        "{} {} → {}",
                        config.piece_label(&entry.slots_key),
                        config.label_of(&entry.from),
                        config.label_of(&entry.to)
                    )
                    .chars()
                    .take(100)
                    .collect::<String>(),
                    entry.id.to_string(),
                )
            })
            .collect();
        // Labels need the human-readable level names — patch them here with
        // the levels of the config is not available in this fn, so the
        // caller rewrites labels via `label_entry`.
        rows.push(CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                format!("{namespace}:remove"),
                CreateSelectMenuKind::String { options },
            )
            .placeholder("Remove an entry…")
            .min_values(1)
            .max_values(1),
        ));
    }

    rows
}

pub fn plan_embed(config: &PlanConfig, session: &PlanSession) -> CreateEmbed {
    let lines: Vec<String> = if session.entries.is_empty() {
        vec!["_Nothing in your plan yet — tap **Add** to start._".to_string()]
    } else {
        session
            .entries
            .iter()
            .map(|entry| {
                format!(
                    "• **{}** — {} → {}",
                    config.piece_label(&entry.slots_key),
                    config.label_of(&entry.from),
                    config.label_of(&entry.to)
                )
            })
            .collect()
    };

    CreateEmbed::new()
        .title(format!("{} {} — Plan", config.emoji, config.title))
        .color(config.color)
        .description(lines.join("\n"))
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn totals_embed(config: &PlanConfig, session: &PlanSession) -> Option<CreateEmbed> {
    if session.entries.is_empty() {
        return None;
    }

    let mut combined = RangeTotals::default();
    let mut entry_lines: Vec<String> = Vec::new();

    for entry in &session.entries {
        let mult = config.piece_multiplier(&entry.slots_key);
        let Some(range) = compute_range(config, &entry.from, &entry.to, mult) else {
            continue;
        };
        entry_lines.push(format!(
            "• {} {} → {}",
            config.piece_label(&entry.slots_key),
            config.label_of(&entry.from),
            config.label_of(&entry.to)
        ));
        for (field, amount) in range.materials {
            *combined.materials.entry(field).or_insert(0.0) += amount;
        }
        for (field, amount) in range.gains {
            *combined.gains.entry(field).or_insert(0.0) += amount;
        }
        combined.svs += range.svs;
    }

    let mut embed = CreateEmbed::new()
        .title(format!("{} {} — Plan totals", config.emoji, config.title))
        .color(config.color)
        .description(format!("**Plan**\n{}", entry_lines.join("\n")));

    for material in config.materials {
        let amount = material_amount(&combined, material.field);
        if amount > 0 {
            // Prefer home-guild custom emoji when installed.
            let title = material_field_title(material.field, material.label);
            embed = embed.field(title, format!("**{}**", format_game_amount(amount)), true);
        }
    }

    for spec in config.stat_gains {
        let value = combined.gains.get(spec.field).copied().unwrap_or(0.0);
        let rendered = if spec.percent {
            format!("{}{:.2}%", spec.prefix, value)
        } else {
            format!(
                "{}{}",
                spec.prefix,
                format_game_amount(value.round() as i64)
            )
        };
        embed = embed.field(spec.label, format!("**{rendered}**"), true);
    }

    embed = embed.field(
        "⚔️ SvS Score",
        format!("**+{}**", format_game_amount(combined.svs.round() as i64)),
        true,
    );

    embed = embed.footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));
    Some(embed)
}

pub fn panel_embed(config: &PlanConfig) -> CreateEmbed {
    let names: Vec<&str> = (config.pieces)()
        .iter()
        .map(|p| p.label.split('(').next().unwrap().trim())
        .collect();
    let summary = format!(
        "{}\n\n**How to use**\n🌐 **Translate** — private guide in your language (optional)\n📊 **Calculate** — build a multi-piece plan, then Show totals",
        config.panel_description
    );
    // names preview line, trimmed for embed limits
    let names_line = names.join(" · ");
    let names_line = if names_line.chars().count() > 300 {
        format!("{}…", names_line.chars().take(300).collect::<String>())
    } else {
        names_line
    };

    CreateEmbed::new()
        .title(format!("{} {}", config.emoji, config.title))
        .description(format!("{summary}\n\n**Pieces**\n{names_line}"))
        .color(config.color)
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

pub fn namespace(config: &PlanConfig) -> &'static str {
    if config.calc_key == CHARM {
        "charmcalc"
    } else {
        "chiefgearcalc"
    }
}

pub fn panel_row(config: &PlanConfig, message_id: u64) -> serenity::CreateActionRow {
    serenity::CreateActionRow::Buttons(vec![
        components::primary(format!("{}:open", namespace(config)), "📊 Calculate"),
        crate::translate::translate_button(message_id),
    ])
}

/// Post (or repost) the persistent calculator panel.
pub async fn post_panel(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    guild_id: u64,
    config: &'static PlanConfig,
) -> anyhow::Result<u64> {
    let channel = crate::db::calculator::get(&data.db, guild_id, config.calc_key)
        .await?
        .channel()
        .ok_or_else(|| {
            anyhow::anyhow!("no channel configured — set one in /settings → Calculators")
        })?;

    let target = serenity::ChannelId::new(channel);
    let message = target
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new().embed(panel_embed(config)),
        )
        .await?;
    target
        .edit_message(
            &ctx.http,
            message.id,
            serenity::EditMessage::new().components(vec![panel_row(config, message.id.get())]),
        )
        .await?;

    let stored_json = crate::translate::stored_json(&panel_embed(config));
    crate::translate::register(
        &data.db,
        message.id.get(),
        crate::db::translate::NewTranslatable {
            guild_id: Some(guild_id),
            channel_id: Some(channel),
            kind: config.calc_key,
            embed_json: stored_json,
            content: None,
            owner_id: None,
        },
    )
    .await?;

    crate::db::calculator::set_channel(
        &data.db,
        guild_id,
        config.calc_key,
        Some(channel),
        Some(message.id.get()),
    )
    .await?;
    Ok(message.id.get())
}

pub const CHIEF_GEAR: &str = "chiefgear";
pub const CHARM: &str = "charm";
