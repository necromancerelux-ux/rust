//! JS-parity view-model helpers (subset of `lookup/services/lookupData.js`).
//!
//! Goal: normalize raw lookup payloads into stable fields for V2 panels/cards.

use serde_json::Value;

fn obj(v: &Value) -> Option<&serde_json::Map<String, Value>> { v.as_object() }

pub fn pick_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let o = obj(v)?;
    for k in keys {
        if let Some(Value::String(s)) = o.get(*k) {
            let t = s.trim();
            if !t.is_empty() { return Some(t); }
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
                let cleaned: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.').collect();
                if let Ok(f) = cleaned.parse::<f64>() { return Some(f as i64); }
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
    if t.unsigned_abs() < 10_000_000_000 { t *= 1000; }
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
    pub furnace: Option<i64>,
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub as_of_ms: Option<i64>,
}

pub fn player_view(raw: &Value) -> PlayerView {
    let r = raw.get("player").unwrap_or(raw);
    let alliance = r.get("alliance");
    let as_of_ms = ts_ms(r, &["detail_updated_at","stats_updated_at","updated_at","last_seen","fetched_at"])
        .or_else(|| raw.get("updatedAt").and_then(|u| ts_ms(u, &["detail","stats","lastSeen"])));
    let alliance_tag = pick_str(r, &["alliance_abbr","alliance_tag","abbr"])
        .map(str::to_string)
        .or_else(|| alliance.and_then(|a| pick_str(a, &["tag","abbr"]).map(str::to_string)));
    let alliance_name = pick_str(r, &["alliance_name"])
        .map(str::to_string)
        .or_else(|| alliance.and_then(|a| pick_str(a, &["name"]).map(str::to_string)));
    let role_num = pick_i64(r, &["alliance_rank","rank"])
        .or_else(|| alliance.and_then(|a| pick_i64(a, &["rank"])));
    PlayerView {
        name: pick_str(r, &["nick_name","name","game_name","username"]).unwrap_or("Unknown").to_string(),
        fid: pick_str(r, &["fid","chief_id"]).map(str::to_string).or_else(|| pick_i64(r, &["fid","chief_id"]).map(|n| n.to_string())),
        uid: pick_str(r, &["uid"]).map(str::to_string).or_else(|| pick_i64(r, &["uid"]).map(|n| n.to_string())),
        avatar_url: pick_str(r, &["avatar_url","avatar"]).map(str::to_string),
        state_id: pick_i64(r, &["kid","state_id","state"]),
        alliance_tag,
        alliance_name,
        role: role_label(role_num),
        power: pick_i64(r, &["power"]),
        kills: pick_i64(r, &["kills"]),
        furnace: pick_i64(r, &["stove_lv","furnace_level","town_center_level","furnace","level"]),
        x: pick_i64(r, &["x"]),
        y: pick_i64(r, &["y"]),
        as_of_ms,
    }
}
