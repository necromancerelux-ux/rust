//! Chief Gear Calculator — multi-entry plan (materials, power, stat, deploy cap).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::plan::{MaterialSpec, PieceSpec, PlanConfig, StatSpec, StepData};

const DATA: &str = include_str!("data/chief_gear.json");

#[derive(Debug, Deserialize)]
struct GearData {
    #[serde(rename = "GEAR_LEVEL_ORDER")]
    order: Vec<String>,
    #[serde(rename = "GEAR_LEVEL_LABELS")]
    labels: HashMap<String, String>,
    #[serde(rename = "GEAR_STEPS")]
    steps: HashMap<String, RawStep>,
    #[serde(rename = "GEAR_PIECE_OPTIONS")]
    pieces: Vec<PieceSpecJson>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct RawStep {
    #[serde(default)]
    hardenedAlloy: serde_json::Value,
    #[serde(default)]
    polishingSolution: serde_json::Value,
    #[serde(default)]
    designPlans: serde_json::Value,
    #[serde(default)]
    lunarAmber: serde_json::Value,
    #[serde(default)]
    svsScore: i64,
    #[serde(default)]
    power: i64,
    #[serde(default)]
    statBonus: f64,
    #[serde(default)]
    deployCap: i64,
}

#[derive(Debug, Deserialize)]
struct PieceSpecJson {
    key: String,
    label: String,
    multiplier: i64,
}

static GEAR_LEVELS: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
static STEP_TABLE: OnceLock<HashMap<&'static str, StepData>> = OnceLock::new();
static PIECES: OnceLock<Vec<PieceSpec>> = OnceLock::new();
static PARSED: OnceLock<GearData> = OnceLock::new();

fn parsed() -> &'static GearData {
    PARSED.get_or_init(|| serde_json::from_str(DATA).expect("chief_gear.json is valid"))
}

pub fn levels() -> &'static [(&'static str, &'static str)] {
    GEAR_LEVELS
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
                materials.insert("hardenedAlloy", amount(&step.hardenedAlloy));
                materials.insert("polishingSolution", amount(&step.polishingSolution));
                materials.insert("designPlans", amount(&step.designPlans));
                materials.insert("lunarAmber", amount(&step.lunarAmber));
                materials.insert("svsScore", step.svsScore as f64);
                let mut stats = HashMap::new();
                stats.insert("power", step.power as f64);
                stats.insert("statBonus", step.statBonus);
                stats.insert("deployCap", step.deployCap as f64);
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
    PIECES
        .get_or_init(|| {
            parsed()
                .pieces
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
    calc_key: super::plan::CHIEF_GEAR,
    title: "Chief Gear Calculator",
    panel_description: "Plan Chief Gear upgrades with verified material costs, power, stats & deployment.\n\n**How to use**\n🌐 **Translate** — private guide in your language (optional)\n📊 **Calculate** — build a multi-piece plan, then show totals",
    emoji: "🎖️",
    color: 0xffb74d,
    pieces,
    levels,
    steps,
    svs_field: "svsScore",
    materials: &[
        MaterialSpec { field: "hardenedAlloy", label: "🛡️ Hardened Alloy" },
        MaterialSpec { field: "polishingSolution", label: "🧪 Polishing Solution" },
        MaterialSpec { field: "designPlans", label: "📐 Design Plans" },
        MaterialSpec { field: "lunarAmber", label: "🌕 Lunar Amber" },
    ],
    stat_gains: &[
        StatSpec::new("power", "💪 Power gain", false, "+"),
        StatSpec::new("statBonus", "📈 Stat Bonus", true, "+"),
        StatSpec::new("deployCap", "🎖️ Deployment Cap", false, "+"),
    ],
};
