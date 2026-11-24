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
    let gear_privacy = r.get("gearPrivacy").or_else(|| r.get("gear_privacy")).and_then(Value::as_bool).unwrap_or(false);
    let gear_privacy_message = pick_str(r, &["gearPrivacyMessage","gear_privacy_message"]).map(str::to_string)
        .or_else(|| gear_privacy.then(|| "Chief Gear privacy is active — this chief is hiding their equipment.".to_string()));
    let pos = r.get("pos").or_else(|| r.get("position"));
    let position = pos.and_then(|p| {
        if let Some(s) = p.as_str() { let t=s.trim(); if !t.is_empty() { return Some(t.to_string()); } }
        let x = p.get("x").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
        let y = p.get("y").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
        match (x,y) { (Some(x),Some(y)) => Some(format!("{x}:{y}")), _ => None }
    }).or_else(|| match (pick_i64(r,&["x"]), pick_i64(r,&["y"])) { (Some(x),Some(y)) => Some(format!("{x}:{y}")), _ => None });
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
        likes: pick_i64(r, &["likes","like"]),
        total_like: pick_i64(r, &["total_like","total_likes"]),
        furnace: pick_i64(r, &["stove_lv","furnace_level","furnace","level"]),
        town_center: pick_i64(r, &["town_center_level","townCenter"]),
        vip: pick_i64(r, &["vip","vip_level"]),
        vip_active: r.get("vip_active").or_else(|| r.get("vipActive")).and_then(Value::as_bool),
        tg_level: pick_i64(r, &["tg_level","tgLevel"]),
        tg_label: pick_str(r, &["tg_label","level_label","tgLabel"]).map(str::to_string),
        gear_privacy,
        gear_privacy_message,
        x: pick_i64(r, &["x"]),
        y: pick_i64(r, &["y"]),
        position,
        last_power_change_ms: ts_ms(r, &["stats_updated_at","last_power_change_at"]),
        as_of_ms,
    }
}

#[derive(Debug, Clone, Default)]
pub struct AllianceView {
    pub tag: String,
    pub name: String,
    pub state_id: Option<i64>,
    pub power: Option<i64>,
    pub kills: Option<i64>,
    pub members: Option<i64>,
    pub leader: Option<String>,
    pub notice: Option<String>,
    pub as_of_ms: Option<i64>,
}

pub fn alliance_view(raw: &Value) -> AllianceView {
    let a = raw.get("alliance").or_else(|| raw.get("info")).unwrap_or(raw);
    AllianceView {
        tag: pick_str(a, &["abbr","tag","alliance_abbr"]).unwrap_or("").to_string(),
        name: pick_str(a, &["name","alliance_name"]).unwrap_or("").to_string(),
        state_id: pick_i64(a, &["kid","state_id","state"]),
        power: pick_i64(a, &["power","total_power"]),
        kills: pick_i64(a, &["kills","total_kills"]),
        members: pick_i64(a, &["member_count","members","count"]).or_else(|| raw.get("roster").and_then(Value::as_array).map(|a| a.len() as i64)),
        leader: pick_str(a, &["leader_name","leader"]).map(str::to_string),
        notice: pick_str(a, &["notice","announcement","message"]).map(str::to_string).or_else(|| raw.get("notice").and_then(Value::as_str).map(str::to_string)),
        as_of_ms: ts_ms(a, &["updated_at","detail_updated_at","fetched_at"]),
    }
}

#[derive(Debug, Clone, Default)]
pub struct StateView {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub players: Option<i64>,
    pub alliances: Option<i64>,
    pub power: Option<i64>,
    pub president: Option<String>,
    pub season: Option<i64>,
    pub as_of_ms: Option<i64>,
}

pub fn state_view(raw: &Value) -> StateView {
    let st = raw.get("state").or_else(|| raw.get("kingdom")).unwrap_or(raw);
    let pres = raw.get("president").or_else(|| st.get("president"));
    let president = pres.and_then(|p| pick_str(p, &["name","nick_name","username"]).map(str::to_string))
        .or_else(|| pick_str(st, &["president_name","president"]).map(str::to_string));
    StateView {
        id: pick_i64(st, &["kid","state_id","id"]),
        name: pick_str(st, &["name"]).map(str::to_string),
        players: pick_i64(st, &["player_count","players","tracked"]),
        alliances: pick_i64(st, &["alliance_count","alliances"]),
        power: pick_i64(st, &["power","total_power"]),
        president,
        season: pick_i64(st, &["season","season_id"]),
        as_of_ms: ts_ms(st, &["updated_at","detail_updated_at","fetched_at"]),
    }
}
