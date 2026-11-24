//! State Lookup — Components V2 panel (matches JS `statePanel.js`).
//!
//! Layout target (same as JS):
//!   ┃ [ tab dropdown — placeholder = current tab ]
//!   ┃ ───────────────
//!   ┃ ## Title
//!   ┃ body / rendered image card
//!   ┃ [ action buttons / selects / pager ]
//!   ┃ ───────────────
//!   ┃ -# OmniCore · <label> as of <relative>
//!
//! Tabs: Overview · State vs State · Rankings · Alliances · Troop Level · Appointments
//! Extra: map (via button)

use poise::serenity_prelude as serenity;
use serde_json::Value;
use serenity::{
    CreateAttachment, CreateInteractionResponseMessage, EditInteractionResponse, MessageFlags,
};

use crate::lookup::sessions::LookupSession;
use crate::ui::v2::{self, ButtonDef, SelectOpt, V2};

pub const STATE_TABS: &[(&str, &str)] = &[
    ("overview", "Overview"),
    ("svs", "State vs State"),
    ("boards", "Rankings"),
    ("alliances", "Alliances"),
    ("troopLevel", "Troop Level"),
    ("appointments", "Appointments"),
];

fn tab_label(key: &str) -> &str {
    STATE_TABS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, l)| *l)
        .unwrap_or(match key {
            "map" => "Map",
            _ => "Overview",
        })
}

fn valid_tab(tab: &str) -> &str {
    if STATE_TABS.iter().any(|(k, _)| *k == tab) || tab == "map" {
        tab
    } else {
        "overview"
    }
}

fn pick_str<'a>(raw: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let obj = raw.as_object()?;
    for key in keys {
        if let Some(Value::String(s)) = obj.get(*key) {
            if !s.trim().is_empty() {
                return Some(s.as_str());
            }
        }
    }
    None
}

fn pick_i64(raw: &Value, keys: &[&str]) -> Option<i64> {
    let obj = raw.as_object()?;
    for key in keys {
        match obj.get(*key) {
            Some(Value::Number(n)) => return n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
            Some(Value::String(s)) => {
                let cleaned: String = s.chars().filter(|c| *c != ',' && *c != '_').collect();
                if let Ok(f) = cleaned.parse::<f64>() {
                    return Some(f as i64);
                }
            }
            _ => {}
        }
    }
    None
}

fn list_key(raw: &Value, keys: &[&str]) -> Vec<&Value> {
    for k in keys {
        if let Some(arr) = raw.get(*k).and_then(Value::as_array) {
            return arr.iter().collect();
        }
    }
    Vec::new()
}

fn image_name_for(tab: &str) -> &'static str {
    match tab {
        "overview" => "state.png",
        "svs" => "svs.png",
        "map" => "state-map.png",
        "appointments" => "appointments.png",
        _ => "state.png",
    }
}

fn build(
    session: &LookupSession,
    tab: &str,
    source_note: Option<&str>,
    card_png: Option<Vec<u8>>,
) -> (V2, Option<Vec<u8>>, &'static str) {
    let tab = valid_tab(tab);
    let raw = &session.raw;
    let opts: Vec<SelectOpt> = STATE_TABS
        .iter()
        .map(|(k, l)| SelectOpt {
            label: l.to_string(),
            value: k.to_string(),
            description: None,
            is_default: *k == tab,
        })
        .collect();

    let mut v2 = V2::new()
        .select("lookup:state:tab", tab_label(tab), &opts)
        .sep(true);

    let mut keep_png = card_png;
    let img_name = image_name_for(tab);

    let state_label = session
        .state
        .map(|s| format!("State {s}"))
        .unwrap_or_else(|| {
            if session.name.trim().is_empty() {
                "State".into()
            } else {
                session.name.clone()
            }
        });

    let footer_label = match tab {
        "overview" => "state as of",
        "svs" => "matchup as of",
        "boards" => "rankings as of",
        "alliances" => "alliances as of",
        "troopLevel" => "troop levels as of",
        "appointments" => "appointments as of",
        "map" => "map as of",
        _ => "Lookup",
    };

    match tab {
        "overview" => {
            v2 = v2.title(&state_label);
            if keep_png.is_some() {
                v2 = v2.image(img_name);
            } else {
                let power = pick_i64(raw, &["power", "total_power"]);
                let players = pick_i64(raw, &["player_count", "players", "tracked_players"]);
                let alliances = pick_i64(raw, &["alliance_count", "alliances"]);
                let mut lines = Vec::new();
                if let Some(p) = power {
                    lines.push(format!("**Power** `{}`", v2::fmt(Some(p))));
                }
                if let Some(n) = players {
                    lines.push(format!("**Players** `{n}`"));
                }
                if let Some(n) = alliances {
                    lines.push(format!("**Alliances** `{n}`"));
                }
                if lines.is_empty() {
                    lines.push("_State overview will fill when upstream data is present._".into());
                }
                v2 = v2.text(lines.join(" · "));
            }
            if session.state.is_some() {
                v2 = v2.buttons(&[ButtonDef {
                    id: "lookup:state:go:map".into(),
                    label: "Map".into(),
                    emoji: Some("🗺️".into()),
                    ..Default::default()
                }]);
            }
        }
        "svs" => {
            v2 = v2.title(format!("{state_label} — State vs State"));
            if keep_png.is_some() {
                v2 = v2.image(img_name);
            } else {
                let opponent = pick_str(raw, &["opponent", "opponent_name", "vs_name"]);
                let result = pick_str(raw, &["result", "outcome", "last_result"]);
                let mut lines = Vec::new();
                if let Some(o) = opponent {
                    lines.push(format!("**Opponent** `{o}`"));
                }
                if let Some(r) = result {
                    lines.push(format!("**Last result** `{r}`"));
                }
                if lines.is_empty() {
                    lines.push("_No SvS matchup data in the stored payload._".into());
                }
                v2 = v2.text(lines.join("\n"));
            }
        }
        "boards" => {
            v2 = v2.title(format!("{state_label} — Rankings"));
            let list = list_key(raw, &["boards", "leaderboards", "rankings", "entries"]);
            let per = 15usize;
            let total_pages = list.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages.saturating_sub(1));
            let slice: Vec<String> = list
                .iter()
                .skip(page * per)
                .take(per)
                .enumerate()
                .map(|(i, row)| {
                    let name = pick_str(row, &["name", "username", "nick_name"]).unwrap_or("—");
                    let power = pick_i64(row, &["power", "value", "score"]);
                    let rank = page * per + i + 1;
                    match power {
                        Some(p) => format!(
                            "{} **{name}** — `{}`",
                            v2::rank_chip(rank as i64),
                            v2::fmt(Some(p))
                        ),
                        None => format!("{} **{name}**", v2::rank_chip(rank as i64)),
                    }
                })
                .collect();
            if slice.is_empty() {
                v2 = v2.text("_No ranking entries stored for this state._");
            } else {
                v2 = v2.text(slice.join("\n"));
                if total_pages > 1 {
                    v2 = v2.pager("lookup:state:pg", page, total_pages);
                }
            }
        }
        "alliances" => {
            v2 = v2.title(format!("{state_label} — Alliances"));
            let list = list_key(raw, &["alliances", "alliance_list", "tags"]);
            let per = 15usize;
            let total_pages = list.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages.saturating_sub(1));
            let slice: Vec<String> = list
                .iter()
                .skip(page * per)
                .take(per)
                .enumerate()
                .map(|(i, row)| {
                    let tag = pick_str(row, &["tag", "abbr", "alliance_abbr"]).unwrap_or("???");
                    let name = pick_str(row, &["name", "alliance_name"]).unwrap_or("");
                    let power = pick_i64(row, &["power", "total_power"]);
                    let rank = page * per + i + 1;
                    let label = if name.is_empty() {
                        format!("[{tag}]")
                    } else {
                        format!("[{tag}] {name}")
                    };
                    match power {
                        Some(p) => format!(
                            "{} **{label}** — `{}`",
                            v2::rank_chip(rank as i64),
                            v2::fmt(Some(p))
                        ),
                        None => format!("{} **{label}**", v2::rank_chip(rank as i64)),
                    }
                })
                .collect();
            if slice.is_empty() {
                v2 = v2.text("_No alliances listed for this state._");
            } else {
                v2 = v2.text(slice.join("\n"));
                if total_pages > 1 {
                    v2 = v2.pager("lookup:state:pg", page, total_pages);
                }
            }
        }
        "troopLevel" => {
            v2 = v2.title(format!("{state_label} — Troop Level"));
            let list = list_key(raw, &["troop_levels", "troopLevels", "players", "roster"]);
            let per = 12usize;
            let total_pages = list.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages.saturating_sub(1));
            let slice: Vec<String> = list
                .iter()
                .skip(page * per)
                .take(per)
                .enumerate()
                .map(|(i, row)| {
                    let name = pick_str(row, &["name", "nick_name", "username"]).unwrap_or("—");
                    let rank = page * per + i + 1;
                    let mut bits = Vec::new();
                    if let Some(arr) = row.get("troop").and_then(Value::as_array) {
                        for t in arr {
                            let kind = pick_str(t, &["name", "kind"]).unwrap_or("?");
                            let tier = pick_i64(t, &["tier"]);
                            let fc = pick_i64(t, &["fc", "star"]);
                            if let Some(tier) = tier {
                                bits.push(match fc {
                                    Some(fc) => format!("{kind} · T{tier} · FC{fc}"),
                                    None => format!("{kind} · T{tier}"),
                                });
                            }
                        }
                    }
                    if bits.is_empty() {
                        if let Some(tl) = row.get("troopLevels").or_else(|| row.get("troop_levels"))
                        {
                            for (k, label) in [
                                ("infantry", "Infantry"),
                                ("lancer", "Lancer"),
                                ("marksman", "Marksman"),
                            ] {
                                if let Some(tier) = tl.get(k).and_then(|v| v.as_i64()) {
                                    bits.push(format!("{label} · T{tier}"));
                                }
                            }
                        }
                    }
                    let extra = if bits.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", bits.join(" / "))
                    };
                    format!("{} **{name}**{extra}", v2::rank_chip(rank as i64))
                })
                .collect();
            if slice.is_empty() {
                v2 = v2.text("_No troop-level data for this state yet._");
            } else {
                v2 = v2.text(slice.join("\n"));
                if total_pages > 1 {
                    v2 = v2.pager("lookup:state:pg", page, total_pages);
                }
            }
        }
        "appointments" => {
            v2 = v2.title(format!("{state_label} — Appointments"));
            if keep_png.is_some() {
                v2 = v2.image(img_name);
            } else {
                let ministers = list_key(raw, &["ministers", "appointments"]);
                let offenders = list_key(raw, &["offenders"]);
                let mut lines = Vec::new();
                if !ministers.is_empty() {
                    lines.push("**Ministers**".to_string());
                    for m in ministers.iter().take(10) {
                        let role = pick_str(m, &["role", "title", "position"]).unwrap_or("—");
                        let name = pick_str(m, &["name", "fid", "chief_name"]).unwrap_or("vacant");
                        lines.push(format!("• {role}: **{name}**"));
                    }
                }
                if !offenders.is_empty() {
                    lines.push("**Offenders**".to_string());
                    for m in offenders.iter().take(8) {
                        let role = pick_str(m, &["role", "title"]).unwrap_or("—");
                        let name = pick_str(m, &["name", "fid"]).unwrap_or("—");
                        lines.push(format!("• {role}: **{name}**"));
                    }
                }
                if lines.is_empty() {
                    v2 = v2.text("_No appointment data stored for this state._");
                } else {
                    v2 = v2.text(lines.join("\n"));
                }
            }
        }
        "map" => {
            v2 = v2.title(format!("{state_label} — Map"));
            if keep_png.is_some() {
                v2 = v2.image(img_name);
            } else {
                v2 = v2.text("_Map overlay will appear when location data is available._");
            }
            if let Some(notice) = &session.notice {
                v2 = v2.text(notice.clone());
            }
            let mut buttons = vec![
                ButtonDef {
                    id: "lookup:state:tab:overview".into(),
                    label: "← Back to overview".into(),
                    ..Default::default()
                },
                ButtonDef {
                    id: "lookup:state:map:refresh".into(),
                    label: "Refresh map".into(),
                    emoji: Some("🔄".into()),
                    ..Default::default()
                },
            ];
            if let Some(kid) = session.state {
                buttons.push(ButtonDef {
                    id: String::new(),
                    label: "Open full map".into(),
                    emoji: Some("🗺️".into()),
                    url: Some(format!("https://whiteoutstats.com/map?kid={kid}")),
                    ..Default::default()
                });
            }
            v2 = v2.buttons(&buttons);
        }
        _ => {
            v2 = v2.title(&state_label);
            v2 = v2.text("Unknown tab.");
        }
    }

    let footer = source_note.unwrap_or(footer_label);
    (v2.footer(footer, None), keep_png, img_name)
}

pub fn render_state_v2(
    session: &LookupSession,
    tab: &str,
    source_note: Option<&str>,
    card_png: Option<Vec<u8>>,
) -> (CreateInteractionResponseMessage, Vec<CreateAttachment>) {
    let (v2, png, img_name) = build(session, tab, source_note, card_png);
    let mut files = Vec::new();
    if let Some(png) = png {
        files.push(v2::attachment(png, img_name));
    }
    (v2::payload(v2, files.clone(), true), files)
}

pub fn edit_state_v2(
    session: &LookupSession,
    tab: &str,
    source_note: Option<&str>,
    card_png: Option<Vec<u8>>,
) -> EditInteractionResponse {
    let (v2, png, img_name) = build(session, tab, source_note, card_png);
    let mut response = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![v2.into_component()]);
    if let Some(png) = png {
        response = response.new_attachment(v2::attachment(png, img_name));
    }
    response
}
