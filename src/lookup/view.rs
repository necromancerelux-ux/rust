//! JS-parity view-model helpers (subset of `lookup/services/lookupData.js`).
//!
//! Goal: normalize raw lookup payloads into stable fields for V2 panels/cards.

use serde_json::Value;

fn obj(v: &Value) -> Option<&serde_json::Map<String, Value>> {
    v.as_object()
}

pub fn pick_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let o = obj(v)?;
    for k in keys {
        if let Some(Value::String(s)) = o.get(*k) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

pub fn pick_i64(v: &Value, keys: &[&str]) -> Option<i64> {
    let o = obj(v)?;
    for k in keys {
        match o.get(*k) {
            Some(Value::Number(n)) => return n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
            Some(Value::String(s)) => {
                let cleaned: String = s
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
                if let Ok(f) = cleaned.parse::<f64>() {
                    return Some(f as i64);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn arr(v: &Value, keys: &[&str]) -> Vec<Value> {
    for k in keys {
        if let Some(a) = v.get(*k).and_then(Value::as_array) {
            return a.clone();
        }
    }
    v.as_array().cloned().unwrap_or_default()
}

pub fn ts_ms(v: &Value, keys: &[&str]) -> Option<i64> {
    let mut t = pick_i64(v, keys)?;
    if t.unsigned_abs() < 10_000_000_000 {
        t *= 1000;
    }
    Some(t)
}

pub fn role_label(v: Option<i64>) -> String {
    match v {
        Some(5) => "R5".into(),
        Some(4) => "R4".into(),
        Some(3) => "R3".into(),
        Some(2) => "R2".into(),
        Some(1) => "R1".into(),
        Some(n) => format!("R{n}"),
        None => "—".into(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlayerView {
    pub name: String,
    pub fid: Option<String>,
    pub uid: Option<String>,
    pub avatar_url: Option<String>,
    pub state_id: Option<i64>,
    pub alliance_tag: Option<String>,
    pub alliance_name: Option<String>,
    pub role: String,
    pub power: Option<i64>,
    pub kills: Option<i64>,
    pub likes: Option<i64>,
    pub total_like: Option<i64>,
    pub furnace: Option<i64>,
    pub town_center: Option<i64>,
    pub vip: Option<i64>,
    pub vip_active: Option<bool>,
    pub tg_level: Option<i64>,
    pub tg_label: Option<String>,
    pub gear_privacy: bool,
    pub gear_privacy_message: Option<String>,
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub position: Option<String>,
    pub last_power_change_ms: Option<i64>,
    pub as_of_ms: Option<i64>,
}

pub fn player_view(raw: &Value) -> PlayerView {
    let r = raw.get("player").unwrap_or(raw);
    let alliance = r.get("alliance");
    let as_of_ms = ts_ms(
        r,
        &[
            "detail_updated_at",
            "stats_updated_at",
            "updated_at",
            "last_seen",
            "fetched_at",
        ],
    )
    .or_else(|| {
        raw.get("updatedAt")
            .and_then(|u| ts_ms(u, &["detail", "stats", "lastSeen"]))
    });
    let alliance_tag = pick_str(r, &["alliance_abbr", "alliance_tag", "abbr"])
        .map(str::to_string)
        .or_else(|| alliance.and_then(|a| pick_str(a, &["tag", "abbr"]).map(str::to_string)));
    let alliance_name = pick_str(r, &["alliance_name"])
        .map(str::to_string)
        .or_else(|| alliance.and_then(|a| pick_str(a, &["name"]).map(str::to_string)));
    let role_num = pick_i64(r, &["alliance_rank", "rank"])
        .or_else(|| alliance.and_then(|a| pick_i64(a, &["rank"])));
    let gear_privacy = r
        .get("gearPrivacy")
        .or_else(|| r.get("gear_privacy"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let gear_privacy_message = pick_str(r, &["gearPrivacyMessage", "gear_privacy_message"])
        .map(str::to_string)
        .or_else(|| {
            gear_privacy.then(|| {
                "Chief Gear privacy is active — this chief is hiding their equipment.".to_string()
            })
        });
    let pos = r.get("pos").or_else(|| r.get("position"));
    let position = pos
        .and_then(|p| {
            if let Some(s) = p.as_str() {
                let t = s.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
            let x = p.get("x").and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            });
            let y = p.get("y").and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            });
            match (x, y) {
                (Some(x), Some(y)) => Some(format!("{x}:{y}")),
                _ => None,
            }
        })
        .or_else(|| match (pick_i64(r, &["x"]), pick_i64(r, &["y"])) {
            (Some(x), Some(y)) => Some(format!("{x}:{y}")),
            _ => None,
        });
    PlayerView {
        name: pick_str(r, &["nick_name", "name", "game_name", "username"])
            .unwrap_or("Unknown")
            .to_string(),
        fid: pick_str(r, &["fid", "chief_id"])
            .map(str::to_string)
            .or_else(|| pick_i64(r, &["fid", "chief_id"]).map(|n| n.to_string())),
        uid: pick_str(r, &["uid"])
            .map(str::to_string)
            .or_else(|| pick_i64(r, &["uid"]).map(|n| n.to_string())),
        avatar_url: pick_str(r, &["avatar_url", "avatar"]).map(str::to_string),
        state_id: pick_i64(r, &["kid", "state_id", "state"]),
        alliance_tag,
        alliance_name,
        role: role_label(role_num),
        power: pick_i64(r, &["power"]),
        kills: pick_i64(r, &["kills"]),
        likes: pick_i64(r, &["likes", "like"]),
        total_like: pick_i64(r, &["total_like", "total_likes"]),
        furnace: pick_i64(r, &["stove_lv", "furnace_level", "furnace", "level"]),
        town_center: pick_i64(r, &["town_center_level", "townCenter"]),
        vip: pick_i64(r, &["vip", "vip_level"]),
        vip_active: r
            .get("vip_active")
            .or_else(|| r.get("vipActive"))
            .and_then(Value::as_bool),
        tg_level: pick_i64(r, &["tg_level", "tgLevel"]),
        tg_label: pick_str(r, &["tg_label", "level_label", "tgLabel"]).map(str::to_string),
        gear_privacy,
        gear_privacy_message,
        x: pick_i64(r, &["x"]),
        y: pick_i64(r, &["y"]),
        position,
        last_power_change_ms: ts_ms(r, &["stats_updated_at", "last_power_change_at"]),
        as_of_ms,
    }
}

#[derive(Debug, Clone, Default)]
pub struct AllianceView {
    pub tag: String,
    pub name: String,
    pub state_id: Option<i64>,
    pub aid: Option<String>,
    pub power: Option<i64>,
    pub kills: Option<i64>,
    pub members: Option<i64>,
    pub member_max: Option<i64>,
    pub level: Option<i64>,
    pub labyrinth: Option<i64>,
    pub power_rank: Option<i64>,
    pub kills_rank: Option<i64>,
    pub leader: Option<String>,
    pub notice: Option<String>,
    pub manifesto: Option<String>,
    pub as_of_ms: Option<i64>,
}

pub fn alliance_view(raw: &Value) -> AllianceView {
    let a = raw
        .get("alliance")
        .or_else(|| raw.get("info"))
        .unwrap_or(raw);
    let roster = raw
        .get("roster")
        .or_else(|| raw.get("members"))
        .or_else(|| a.get("roster"))
        .or_else(|| a.get("members"))
        .and_then(Value::as_array);
    AllianceView {
        tag: pick_str(a, &["abbr", "tag", "alliance_abbr"])
            .unwrap_or("")
            .to_string(),
        name: pick_str(a, &["name", "alliance_name"])
            .unwrap_or("")
            .to_string(),
        state_id: pick_i64(a, &["kid", "state_id", "state"]),
        aid: pick_str(a, &["aid", "alliance_id", "id"])
            .map(str::to_string)
            .or_else(|| {
                raw.get("aid")
                    .and_then(|v| v.as_i64().map(|n| n.to_string()))
            }),
        power: pick_i64(a, &["power", "total_power"]),
        kills: pick_i64(a, &["kills", "total_kills"]),
        members: pick_i64(a, &["member_count", "members", "count"])
            .or_else(|| roster.map(|r| r.len() as i64)),
        member_max: pick_i64(a, &["member_max", "max_members"]),
        level: pick_i64(a, &["lv", "level"]),
        labyrinth: pick_i64(a, &["labyrinth", "labyrinth_score"]),
        power_rank: pick_i64(a, &["power_rank", "rank"]),
        kills_rank: pick_i64(a, &["kills_rank"]),
        leader: pick_str(a, &["leader_name", "leader"])
            .map(str::to_string)
            .or_else(|| {
                roster.and_then(|r| {
                    r.iter()
                        .find(|m| pick_i64(m, &["alliance_rank", "rank"]) == Some(5))
                        .and_then(|m| pick_str(m, &["nick_name", "name"]).map(str::to_string))
                })
            }),
        notice: pick_str(a, &["notice", "announcement", "message"])
            .map(str::to_string)
            .or_else(|| {
                raw.get("notice")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
        manifesto: pick_str(a, &["manifesto"]).map(str::to_string).or_else(|| {
            raw.get("manifesto")
                .and_then(Value::as_str)
                .map(str::to_string)
        }),
        as_of_ms: ts_ms(a, &["updated_at", "detail_updated_at", "fetched_at"]),
    }
}

/* ───────────── boards (JS lookupData BOARDS) ───────────── */

/// key = in-game board type id, label, group (JS `BOARDS` table).
pub const BOARDS: &[(&str, &str, &str)] = &[
    ("1", "Alliance Power", "Core"),
    ("2", "Alliance Kills", "Core"),
    ("3", "Personal Power", "Core"),
    ("4", "Kill Count", "Core"),
    ("5", "Furnace Level", "Core"),
    ("6", "Exploration Stage", "Core"),
    ("7", "Hero Power", "Core"),
    ("8", "Hero's Total Power", "Core"),
    ("9", "Troop Power", "Core"),
    ("10", "Building Power", "Core"),
    ("11", "Research Power", "Core"),
    ("12", "Hero Power Without Equip", "Gear"),
    ("13", "Hero Equip Power", "Gear"),
    ("14", "Chief Gear Power", "Gear"),
    ("15", "Chief Charm Power", "Gear"),
    ("16", "Pet Power", "Core"),
    ("18", "Island Prosperity", "Other"),
    ("19", "State Transfer Score", "Other"),
    ("20", "The Labyrinth", "The Labyrinth"),
    ("21", "Land of Heroes", "The Labyrinth"),
    ("22", "Cave of Monsters", "The Labyrinth"),
    ("23", "Charm Mine", "The Labyrinth"),
    ("24", "Research Center", "The Labyrinth"),
    ("25", "Gear Forge", "The Labyrinth"),
    ("26", "Gaia Heart", "The Labyrinth"),
    ("29", "Total Expert Power", "Other"),
];

pub fn board_name(key: &str) -> &str {
    BOARDS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, l, _)| *l)
        .unwrap_or(key)
}

pub fn board_group(key: &str) -> &str {
    BOARDS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, g)| *g)
        .unwrap_or("Core")
}

/// One leaderboard row for the player Standings tab (JS `playerView.boards`).
#[derive(Debug, Clone, Default)]
pub struct BoardRow {
    pub key: String,
    pub name: String,
    pub group: String,
    pub score: Option<i64>,
    pub rank: Option<i64>,
    pub global: Option<String>,
    pub trail: bool,
    pub value_label: Option<String>,
}

impl BoardRow {
    /// Human score for select descriptions (`_util.num` in JS).
    pub fn score_label(&self) -> String {
        self.score
            .map(|n| crate::ui::v2::fmt(Some(n)))
            .unwrap_or_default()
    }
}

fn parse_json_value(v: &Value) -> Option<Value> {
    match v {
        Value::String(s) => serde_json::from_str(s).ok(),
        other => Some(other.clone()),
    }
}

/// Normalise the player's leaderboard payload into board rows.
/// Mirrors `playerView()` in JS: array shape, object shape, flat fallback.
pub fn player_boards(raw: &Value) -> Vec<BoardRow> {
    let r = raw.get("player").unwrap_or(raw);
    let ranks_obj = ["leaderboards", "ranks", "ranks_json"]
        .iter()
        .find_map(|k| r.get(*k).or_else(|| raw.get(*k)))
        .and_then(parse_json_value);

    let mut boards: Vec<BoardRow> = Vec::new();

    if let Some(ranks) = ranks_obj.as_ref() {
        let list: Vec<Value> = if let Some(arr) = ranks.as_array() {
            arr.clone()
        } else {
            arr(ranks, &["boards", "list", "entries", "leaderboards"])
        };
        for b in list.iter().filter(|b| b.is_object()) {
            let key = pick_str(b, &["rank_type", "board", "key", "type", "id"]).unwrap_or("");
            if key.is_empty() || key == "_checked" {
                continue;
            }
            boards.push(BoardRow {
                key: key.to_string(),
                group: board_group(key).to_string(),
                name: pick_str(b, &["label", "name", "title"])
                    .map(str::to_string)
                    .unwrap_or_else(|| board_name(key).to_string()),
                score: pick_i64(b, &["score", "value"]),
                rank: pick_i64(b, &["kingdom_rank", "state_rank", "rank", "position"]),
                global: pick_str(b, &["global_label"])
                    .map(str::to_string)
                    .or_else(|| {
                        pick_i64(b, &["global_rank"]).map(|rank| {
                            let mut label = format!("#{}", crate::ui::v2::int(Some(rank)));
                            if let Some(total) = pick_i64(b, &["global_total"]) {
                                label.push_str(&format!(" / {}", crate::ui::v2::fmt(Some(total))));
                            }
                            label
                        })
                    }),
                trail: b.get("trail").and_then(Value::as_bool).unwrap_or(false),
                value_label: pick_str(b, &["value_label"]).map(str::to_string),
            });
        }

        // ranks_json object shape: { "3": { type, rank, score }, "_checked": true }
        if boards.is_empty() {
            if let Some(map) = ranks.as_object() {
                for (k, b) in map.iter() {
                    if k == "_checked" || !b.is_object() {
                        continue;
                    }
                    let key = pick_str(b, &["type", "rank_type"]).unwrap_or(k);
                    boards.push(BoardRow {
                        key: key.to_string(),
                        group: board_group(key).to_string(),
                        name: board_name(key).to_string(),
                        score: pick_i64(b, &["score", "value"]),
                        rank: pick_i64(b, &["rank", "kingdom_rank", "state_rank"]),
                        global: None,
                        trail: matches!(key, "21" | "22" | "23" | "24" | "25" | "26"),
                        value_label: None,
                    });
                }
            }
        }
    }

    // Flat ranks from profile fields when the full board list is absent.
    if boards.is_empty() {
        let flat: [(&str, Option<i64>, Option<i64>); 5] = [
            ("3", pick_i64(r, &["power_rank"]), pick_i64(r, &["power"])),
            ("4", pick_i64(r, &["kills_rank"]), pick_i64(r, &["kills"])),
            (
                "5",
                pick_i64(r, &["stove_rank"]),
                pick_i64(r, &["stove_lv", "furnace"]),
            ),
            (
                "19",
                pick_i64(r, &["migrant_rank"]),
                pick_i64(r, &["migrant_score"]),
            ),
            (
                "20",
                pick_i64(r, &["mystic_rank"]),
                pick_i64(r, &["mystic_trial"]),
            ),
        ];
        for (key, rank, score) in flat {
            if rank.is_some() || score.is_some() {
                boards.push(BoardRow {
                    key: key.to_string(),
                    group: board_group(key).to_string(),
                    name: board_name(key).to_string(),
                    score,
                    rank,
                    global: None,
                    trail: false,
                    value_label: None,
                });
            }
        }
    }

    boards
}

/* ───────────── troop bits (JS `troopBits`) ───────────── */

const TROOP_KIND: &[(&str, &str)] = &[("inf", "Infantry"), ("lan", "Lancer"), ("mm", "Marksman")];

/// Per-member troop line bits, e.g. `["Infantry · T12 · FC10"]`.
pub fn troop_bits(m: &Value) -> Vec<String> {
    let mut bits = Vec::new();
    if let Some(list) = m
        .get("troop")
        .or_else(|| m.get("expedition_troops"))
        .and_then(Value::as_array)
    {
        for t in list {
            let kind = pick_str(t, &["kind"]).unwrap_or("");
            let name = pick_str(t, &["name"])
                .map(str::to_string)
                .or_else(|| {
                    TROOP_KIND
                        .iter()
                        .find(|(k, _)| *k == kind)
                        .map(|(_, l)| l.to_string())
                })
                .unwrap_or_else(|| kind.to_string());
            if name.is_empty() {
                continue;
            }
            let Some(tier) = pick_i64(t, &["tier"]) else {
                continue;
            };
            let fc = pick_i64(t, &["fc", "star"]);
            bits.push(match fc {
                Some(fc) => format!("{name} · T{tier} · FC{fc}"),
                None => format!("{name} · T{tier}"),
            });
        }
    }
    if !bits.is_empty() {
        return bits;
    }
    if let Some(tl) = m.get("troopLevels").or_else(|| m.get("troop_levels")) {
        for (k, label) in [
            ("infantry", "Infantry"),
            ("lancer", "Lancer"),
            ("marksman", "Marksman"),
        ] {
            if let Some(tier) = tl.get(k).and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            }) {
                bits.push(format!("{label} · T{tier}"));
            }
        }
    }
    bits
}

/// Highest troop tier across infantry/lancer/marksman (JS sort helper).
pub fn max_tier(m: &Value) -> i64 {
    m.get("troopLevels")
        .or_else(|| m.get("troop_levels"))
        .map(|tl| {
            ["infantry", "lancer", "marksman"]
                .iter()
                .filter_map(|k| tl.get(*k).and_then(|v| v.as_i64()))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// `Some(tier)` for JS `troopTier`: integer 1..=12 from a level/tier field.
fn troop_tier_of(v: &Value) -> Option<i64> {
    let level = match v {
        Value::Object(_) => v
            .get("level")
            .or_else(|| v.get("tier"))
            .or_else(|| v.get("troop_level"))
            .or_else(|| v.get("troopLevel"))
            .or_else(|| v.get("lv"))
            .and_then(Value::as_i64)?,
        other => other.as_i64()?,
    };
    (1..=12).contains(&level).then_some(level)
}

/// JS `normalizeTroopLevels`: per-type tier (`{infantry: 12, lancer: null, …}`).
/// Flat fields on the source and nested objects are searched first, then
/// `troop`/`expedition_troops`/`troops` arrays for the short kind key.
pub fn normalize_troop_levels(source: &Value) -> Option<Value> {
    const TYPES: [(&str, &[&str]); 3] = [
        (
            "infantry",
            &[
                "infantry_level",
                "infantryLevel",
                "infantry_lv",
                "infantryLv",
                "inf_lv",
                "infantry_tier",
                "infantryTier",
                "inf_tier",
                "infantry",
            ],
        ),
        (
            "lancer",
            &[
                "lancer_level",
                "lancerLevel",
                "lanc_lv",
                "lancerLv",
                "lancer_tier",
                "lancerTier",
                "lanc_tier",
                "lancer",
            ],
        ),
        (
            "marksman",
            &[
                "marksman_level",
                "marksmanLevel",
                "marksman_lv",
                "marksmanLv",
                "marks_lv",
                "marksman_tier",
                "marksmanTier",
                "marks_tier",
                "archer_level",
                "archer_tier",
                "marksman",
                "archer",
            ],
        ),
    ];
    const SHORT: [(&str, &str); 3] = [("infantry", "inf"), ("lancer", "lan"), ("marksman", "mm")];

    let flat = |v: &Value, keys: &[&str]| -> Option<i64> {
        keys.iter()
            .find_map(|k| pick_i64(v, &[k]).filter(|t| (1..=12).contains(t)))
    };

    let mut out = serde_json::Map::new();
    let mut any = false;
    for (name, keys) in TYPES {
        let mut tier = flat(source, keys);
        // Nested objects (troop_levels / troopLevels / troops).
        if tier.is_none() {
            for nested in ["troop_levels", "troopLevels", "troops"] {
                if let Some(v) = source.get(nested).filter(|v| !v.is_array()) {
                    tier = flat(v, keys);
                    if tier.is_some() {
                        break;
                    }
                }
            }
        }
        // Arrays (expedition_troops / troop / troops).
        if tier.is_none() {
            let short = SHORT.iter().find(|(n, _)| *n == name).map(|(_, s)| *s);
            for arr_key in ["expedition_troops", "troop", "troops"] {
                if let Some(arr) = source.get(arr_key).and_then(Value::as_array) {
                    if let Some(short) = short {
                        tier = arr
                            .iter()
                            .find(|t| {
                                pick_str(t, &["kind", "name", "type"])
                                    .is_some_and(|k| k.eq_ignore_ascii_case(short))
                            })
                            .and_then(troop_tier_of);
                    }
                    if tier.is_none() {
                        tier = arr.iter().find_map(troop_tier_of);
                    }
                    if tier.is_some() {
                        break;
                    }
                }
            }
        }
        any |= tier.is_some();
        out.insert(name.to_string(), tier.map_or(Value::Null, Value::from));
    }
    any.then_some(Value::Object(out))
}

/// Rank chip + name + score line helpers shared by list tabs.
pub fn role_num(v: Option<i64>) -> i64 {
    v.unwrap_or(0)
}

#[derive(Debug, Clone, Default)]
pub struct StateView {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub players: Option<i64>,
    pub alliances: Option<i64>,
    pub power: Option<i64>,
    pub president: Option<String>,
    pub president_sub: Option<String>,
    pub ruling_alliance_tag: Option<String>,
    pub ruling_alliance_name: Option<String>,
    pub rank: Option<i64>,
    pub grade: Option<String>,
    pub health: Option<i64>,
    pub season: Option<i64>,
    pub as_of_ms: Option<i64>,
}

/// Root object of a state payload (`state` / `kingdom` / raw).
pub fn state_root(raw: &Value) -> &Value {
    raw.get("state")
        .or_else(|| raw.get("kingdom"))
        .unwrap_or(raw)
}

/// One SvS history row (JS `loadState().svsHistory.rows`).
#[derive(Debug, Clone, Default)]
pub struct SvsRow {
    pub date_ts: Option<i64>,
    pub opponent_kid: Option<i64>,
    pub prep: Option<String>,
    pub result: Option<String>,
    pub season: Option<String>,
    pub prep_score: Option<i64>,
    pub prep_opp_score: Option<i64>,
    pub days: usize,
}

/// SvS history rows, newest first (throne.kvk / kvk cycles).
pub fn svs_rows(raw: &Value) -> Vec<SvsRow> {
    let st = state_root(raw);
    let kvk = st
        .get("throne")
        .and_then(|t| t.get("kvk"))
        .or_else(|| st.get("kvk"))
        .unwrap_or(st);
    let cycles = ["cycles", "matchups", "history"]
        .iter()
        .find_map(|k| kvk.get(*k).and_then(Value::as_array));
    let mut rows: Vec<SvsRow> = cycles
        .map(|v| v.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|c| {
            let opponent_kid = pick_i64(c, &["opponent_kid", "opponent"]);
            let date = pick_i64(c, &["appointed_at", "ts", "at"]);
            if opponent_kid.is_none() && date.is_none() {
                return None;
            }
            let days = c
                .get("days")
                .and_then(Value::as_array)
                .map(|d| {
                    d.iter()
                        .filter(|x| x.get("home_score").is_some() || x.get("away_score").is_some())
                        .count()
                })
                .unwrap_or(0);
            Some(SvsRow {
                date_ts: date,
                opponent_kid,
                prep: pick_str(c, &["prep"]).map(str::to_string),
                result: pick_str(c, &["result"]).map(str::to_string),
                season: pick_i64(c, &["season"])
                    .map(|n| n.to_string())
                    .or_else(|| pick_str(c, &["season"]).map(str::to_string)),
                prep_score: pick_i64(c, &["prep_score"]),
                prep_opp_score: pick_i64(c, &["prep_opp_score"]),
                days,
            })
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.date_ts.unwrap_or(0)));
    rows
}

pub fn state_view(raw: &Value) -> StateView {
    let st = state_root(raw);
    let health_obj = st.get("health").filter(|h| h.is_object()).unwrap_or(st);
    let pres = raw.get("president").or_else(|| st.get("president"));
    let president = pres
        .filter(|p| p.is_object())
        .and_then(|p| pick_str(p, &["name", "nick_name", "username"]).map(str::to_string))
        .or_else(|| pick_str(st, &["president_name"]).map(str::to_string))
        .or_else(|| {
            pres.filter(|p| p.is_string())
                .and_then(|p| p.as_str().map(str::to_string))
        });
    let president_sub = pres
        .filter(|p| p.is_object())
        .and_then(|p| pick_str(p, &["office_name", "role"]).map(str::to_string))
        .or_else(|| pick_str(st, &["president_label"]).map(str::to_string));
    let ruling = st
        .get("ruling_alliance")
        .or_else(|| st.get("rulingAlliance"));
    StateView {
        id: pick_i64(st, &["kid", "state_id", "id"]),
        name: pick_str(st, &["name"]).map(str::to_string),
        players: pick_i64(st, &["player_count", "players", "tracked"]).or_else(|| {
            st.get("players")
                .and_then(Value::as_array)
                .map(|a| a.len() as i64)
        }),
        alliances: pick_i64(st, &["alliance_count", "alliances"]).or_else(|| {
            st.get("alliances")
                .and_then(Value::as_array)
                .map(|a| a.len() as i64)
        }),
        power: pick_i64(st, &["power", "total_power"]),
        president,
        president_sub,
        ruling_alliance_tag: ruling
            .and_then(|r| pick_str(r, &["abbr", "tag"]).map(str::to_string))
            .or_else(|| pick_str(st, &["ruling_alliance_abbr"]).map(str::to_string)),
        ruling_alliance_name: ruling.and_then(|r| pick_str(r, &["name"]).map(str::to_string)),
        rank: pick_i64(st, &["rank", "power_rank"]),
        grade: health_obj
            .get("grade")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| pick_str(st, &["grade", "health_grade"]).map(str::to_string)),
        health: health_obj
            .get("score")
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .or_else(|| pick_i64(st, &["health_score", "health"])),
        season: pick_i64(st, &["season", "season_id"]),
        as_of_ms: ts_ms(
            st,
            &[
                "updated_at",
                "fetched_at",
                "rank_updated",
                "ingame_ranks_at",
            ],
        ),
    }
}
