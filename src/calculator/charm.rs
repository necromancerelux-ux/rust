//! Chief Charm Calculator — multi-entry plan (materials, power, leth & HP).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::plan::{MaterialSpec, PieceSpec, PlanConfig, StatSpec, StepData};

const DATA: &str = include_str!("data/charm.json");

#[derive(Debug, Deserialize)]
struct CharmData {
    #[serde(rename = "CHARM_LEVEL_ORDER")]
    order: Vec<String>,
    #[serde(rename = "CHARM_LEVEL_LABELS")]
    labels: HashMap<String, String>,
    #[serde(rename = "CHARM_STEPS")]
    steps: HashMap<String, RawStep>,
    #[serde(rename = "SLOT_OPTIONS")]
    slots: Vec<PieceSpecJson>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct RawStep {
    #[serde(default)]
    charmGuides: serde_json::Value,
    #[serde(default)]
    charmDesigns: serde_json::Value,
    #[serde(default)]
    jewelSecrets: serde_json::Value,
    #[serde(default)]
    svsScore: i64,
    #[serde(default)]
    power: i64,
    #[serde(default)]
    powerGain: i64,
    #[serde(default)]
    lethHp: f64,
}

#[derive(Debug, Deserialize)]
struct PieceSpecJson {
    key: String,
    label: String,
    multiplier: i64,
}

static CHARM_LEVELS: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
static STEP_TABLE: OnceLock<HashMap<&'static str, StepData>> = OnceLock::new();
static SLOTS: OnceLock<Vec<PieceSpec>> = OnceLock::new();
static PARSED: OnceLock<CharmData> = OnceLock::new();

fn parsed() -> &'static CharmData {
    PARSED.get_or_init(|| serde_json::from_str(DATA).expect("charm.json is valid"))
}

pub fn levels() -> &'static [(&'static str, &'static str)] {
    CHARM_LEVELS
        .get_or_init(|| {
            parsed()
                .order
                .iter()
                .map(|key| {
                    let label = parsed()
                        .labels
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| key.clone());
                    let key: &'static str = Box::leak(key.clone().into_boxed_str());
                    let label: &'static str = Box::leak(label.into_boxed_str());
                    (key, label)
                })
                .collect()
        })
        .as_slice()
}

fn steps() -> &'static HashMap<&'static str, StepData> {
    STEP_TABLE.get_or_init(|| {
        parsed()
            .steps
            .iter()
            .map(|(key, step)| {
                let mut materials = HashMap::new();
                materials.insert("charmGuides", amount(&step.charmGuides));
                materials.insert("charmDesigns", amount(&step.charmDesigns));
                materials.insert("jewelSecrets", amount(&step.jewelSecrets));
                materials.insert("svsScore", step.svsScore as f64);
                let mut stats = HashMap::new();
                stats.insert("power", step.power as f64);
                stats.insert("lethHp", step.lethHp);
                stats.insert("svsScore", step.svsScore as f64);
                let key: &'static str = Box::leak(key.clone().into_boxed_str());
                (key, StepData { materials, stats })
            })
            .collect()
    })
}

fn amount(value: &serde_json::Value) -> f64 {
    match value {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => {
            crate::calculator::game_numbers::parse_game_amount(s) as f64
        }
        _ => 0.0,
    }
}

fn pieces() -> &'static [PieceSpec] {
    SLOTS
        .get_or_init(|| {
            parsed()
                .slots
                .iter()
                .map(|piece| PieceSpec {
                    key: Box::leak(piece.key.clone().into_boxed_str()),
                    label: Box::leak(piece.label.clone().into_boxed_str()),
                    multiplier: piece.multiplier,
                })
                .collect()
        })
        .as_slice()
}

pub const CONFIG: PlanConfig = PlanConfig {
    calc_key: super::plan::CHARM,
    title: "Chief Charm Calculator",
    panel_description: "Plan Chief Charm upgrades with verified material costs, power, lethality & SvS.\n\n**How to use**\n🌐 **Translate** — private guide in your language (optional)\n📊 **Calculate** — build a multi-entry plan, then show totals",
    emoji: "🌀",
    color: 0x4fc3f7,
    pieces,
    levels,
    steps,
    svs_field: "svsScore",
    materials: &[
        MaterialSpec { field: "charmGuides", label: "🔮 Charm Guides" },
        MaterialSpec { field: "charmDesigns", label: "💎 Charm Designs" },
        MaterialSpec { field: "jewelSecrets", label: "💠 Jewel Secrets" },
    ],
    stat_gains: &[
        StatSpec::new("power", "💪 Power gain", false, "+"),
        StatSpec::new("lethHp", "📈 Leth & HP", true, "+"),
    ],
};
