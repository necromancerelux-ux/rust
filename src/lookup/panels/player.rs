//! Player Lookup — Components V2 panel (matches JS `playerPanel.js` 1:1).
//!
//! Layout target (same as JS):
//!   ┃ [ tab dropdown — placeholder = current tab ]
//!   ┃ ───────────────
//!   ┃ ## Title
//!   ┃ body / rendered image card
//!   ┃ [ action buttons / selects ]
//!   ┃ ───────────────
//!   ┃ -# OmniCore · <label> as of <relative>
//!
//! Tabs: Overview · Standings · Heroes & Gear · Progression · Crowns
//! Extra views (via buttons): map · trail

use poise::serenity_prelude as serenity;
use serde_json::Value;
use serenity::{
    CreateAttachment, CreateInteractionResponseMessage, EditInteractionResponse, MessageFlags,
};

use crate::lookup::sessions::LookupSession;
use crate::ui::v2::{self, ButtonDef, SelectOpt, V2};

pub const PLAYER_TABS: &[(&str, &str)] = &[
    ("overview", "Overview"),
    ("standings", "Standings"),
    ("heroes", "Heroes & Gear"),
    ("progression", "Progression"),
    ("crowns", "Crowns"),
];

fn tab_label(key: &str) -> &str {
    PLAYER_TABS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, l)| *l)
        .unwrap_or(match key {
            "map" => "Location",
            "trail" => "Trails",
            _ => "Overview",
        })
}

fn valid_tab(tab: &str) -> &str {
    if PLAYER_TABS.iter().any(|(k, _)| *k == tab) || matches!(tab, "map" | "trail") {
        tab
    } else {
        "overview"
    }
}

// ---- raw field helpers (same aliases as sessions.rs / JS lookupData) ------

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

fn fmt_num(n: Option<i64>) -> String {
    v2::fmt(n)
}

fn field_block(raw: &Value, pairs: &[(&str, &[&str])]) -> String {
    let mut lines = Vec::new();
    for (label, keys) in pairs {
        if let Some(n) = pick_i64(raw, keys) {
            lines.push(format!("**{label}** `{}`", fmt_num(Some(n))));
        } else if let Some(s) = pick_str(raw, keys) {
            lines.push(format!("**{label}** `{s}`"));
        }
    }
    if lines.is_empty() {
        "_No data for this section._".into()
    } else {
        lines.join(" · ")
    }
}

fn boards(raw: &Value) -> Vec<&Value> {
    raw.get("boards")
        .or_else(|| raw.get("leaderboards"))
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

/// Primary image name used when a card PNG is supplied.
fn image_name_for(tab: &str) -> &'static str {
    match tab {
        "overview" => "player.png",
        "standings" => "standings.png",
        "heroes" => "chief-gear.png",
        "progression" => "progression.png",
        "crowns" => "crowns.png",
        "map" => "player-map.png",
        _ => "player.png",
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
    let name = if session.name.trim().is_empty() {
        session
            .fid
            .clone()
            .unwrap_or_else(|| "Player".into())
    } else {
        session.name.clone()
    };

    // Main tab dropdown (JS uses select whose placeholder shows the current tab)
    let opts: Vec<SelectOpt> = PLAYER_TABS
        .iter()
        .map(|(k, l)| SelectOpt {
            label: l.to_string(),
            value: k.to_string(),
            description: None,
            is_default: *k == tab,
        })
        .collect();

    let mut v2 = V2::new()
        .select("lookup:player:tab", tab_label(tab), &opts)
        .sep(true);

    let mut keep_png = card_png;
    let img_name = image_name_for(tab);

    let footer_label = match tab {
        "overview" => "profile as of",
        "standings" => "boards as of",
        "heroes" => "loadout as of",
        "progression" => "last snapshot",
        "crowns" => "crown record as of",
        "map" => "position as of",
        "trail" => "trails as of",
        _ => "Lookup",
    };

    match tab {
        "overview" => {
            v2 = v2.title(&name);
            if keep_png.is_some() {
                v2 = v2.image(img_name);
            } else {
                // Text fallback when no card was rendered
                let fid = session.fid.as_deref().unwrap_or("—");
                let state = session
                    .state
                    .map(|s| format!("S{s}"))
                    .unwrap_or_else(|| "—".into());
                let alliance = session
                    .alliance
                    .as_deref()
                    .map(|t| format!("[{t}]"))
                    .unwrap_or_else(|| "—".into());
                v2 = v2.text(format!(
                    "**FID** `{fid}` · **State** `{state}` · **Alliance** `{alliance}`"
                ));
                v2 = v2.text(field_block(
                    raw,
                    &[
                        ("Power", &["power", "total_power", "totalPower"]),
                        ("Kills", &["kills", "total_kills", "totalKills"]),
                        (
                            "Furnace",
                            &["furnace_level", "town_hall_level", "townHallLevel", "level"],
                        ),
                    ],
                ));
            }

            // Action buttons matching JS order & labels
            let mut buttons = Vec::new();
            if let Some(state) = session.state {
                buttons.push(ButtonDef {
                    id: "lookup:player:go:state".into(),
                    label: format!("S{state}"),
                    emoji: None,
                    ..Default::default()
                });
            }
            if let Some(tag) = session.alliance.as_deref() {
                buttons.push(ButtonDef {
                    id: "lookup:player:go:alliance".into(),
                    label: format!("[{tag}] Alliance"),
                    emoji: None,
                    ..Default::default()
                });
            }
            buttons.push(ButtonDef {
                id: "lookup:player:go:map".into(),
                label: "Locate on map".into(),
                emoji: Some("🗺️".into()),
                ..Default::default()
            });
            buttons.push(ButtonDef {
                id: "lookup:player:go:trail".into(),
                label: "Trails".into(),
                ..Default::default()
            });
            v2 = v2.buttons(&buttons);
        }
        "standings" => {
            v2 = v2.title(format!("{name} — Leaderboard standings"));
            if keep_png.is_some() {
                v2 = v2.image(img_name);
            } else {
                let list = boards(raw);
                let per = 15usize;
                let total_pages = list.len().div_ceil(per).max(1);
                let page = session.page.min(total_pages - 1);
                let slice: Vec<String> = list
                    .iter()
                    .skip(page * per)
                    .take(per)
                    .map(|board| {
                        let bname = pick_str(board, &["name", "key"]).unwrap_or("Board");
                        let rank = pick_i64(board, &["rank"])
                            .map(|r| format!("#{r}"))
                            .unwrap_or_else(|| "—".into());
                        format!("• **{bname}** — `{rank}`")
                    })
                    .collect();
                if slice.is_empty() {
                    v2 = v2.text("_No leaderboard standings stored for this chief._");
                } else {
                    v2 = v2.text(slice.join("\n"));
                    if total_pages > 1 {
                        v2 = v2.pager("lookup:player:pg", page, total_pages);
                    }
                }
            }

            // Board open selects (JS has state + global selects)
            let st: Vec<SelectOpt> = boards(raw)
                .iter()
                .filter(|b| pick_i64(b, &["rank"]).is_some())
                .take(25)
                .map(|b| {
                    let bname = pick_str(b, &["name", "key"]).unwrap_or("Board");
                    let rank = pick_i64(b, &["rank"]).unwrap_or(0);
                    let score = pick_str(b, &["score", "value"]).unwrap_or("");
                    SelectOpt {
                        label: bname.to_string(),
                        value: pick_str(b, &["key", "name"]).unwrap_or(bname).to_string(),
                        description: Some(format!("#{rank} · {score}")),
                        is_default: false,
                    }
                })
                .collect();
            if !st.is_empty() {
                v2 = v2.select("lookup:player:board:state", "Open a state board…", &st);
            }
        }
        "heroes" => {
            v2 = v2.title(format!("{name} — Heroes & Gear"));
            // Quick bits line (TG / VIP / level) matching JS
            let mut bits = Vec::new();
            if let Some(tg) = pick_str(raw, &["tg_label", "level_label"]) {
                bits.push(tg.to_string());
            } else if let Some(lvl) = pick_i64(raw, &["tg_level"]) {
                bits.push(format!("TG{lvl}"));
            }
            if let Some(vip) = pick_i64(raw, &["vip", "vip_level", "vipLevel"]) {
                let active = raw
                    .get("vip_active")
                    .or_else(|| raw.get("vipActive"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                bits.push(if active {
                    format!("VIP {vip}")
                } else {
                    format!("VIP {vip} (inactive)")
                });
            }
            if !bits.is_empty() {
                v2 = v2.text(bits.join(" · "));
            }

            if keep_png.is_some() {
                v2 = v2.image(img_name);
            } else {
                let gear_private = raw
                    .get("gearPrivacy")
                    .or_else(|| raw.get("gear_privacy"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if gear_private {
                    v2 = v2.text(
                        "🔒 Chief Gear privacy is active — equipment is hidden on the public site.",
                    );
                } else {
                    v2 = v2.text(field_block(
                        raw,
                        &[
                            ("Hero power", &["hero_power", "heroes_power", "heroPower"]),
                            ("Pet power", &["pet_power", "pets_power", "petPower"]),
                        ],
                    ));
                    // List a few hero names if present
                    if let Some(heroes) = raw
                        .get("heroes")
                        .or_else(|| raw.get("hero_list"))
                        .and_then(Value::as_array)
                    {
                        let lines: Vec<String> = heroes
                            .iter()
                            .take(12)
                            .filter_map(|h| {
                                let hname = pick_str(h, &["name", "hero_name", "label"])?;
                                let lvl = pick_i64(h, &["level", "lvl", "star"]);
                                Some(match lvl {
                                    Some(l) => format!("• **{hname}** `{l}`"),
                                    None => format!("• **{hname}**"),
                                })
                            })
                            .collect();
                        if !lines.is_empty() {
                            v2 = v2.text(lines.join("\n"));
                        }
                    }
                }
            }
        }
        "progression" => {
            v2 = v2.title(format!("{name} — Progression"));
            if keep_png.is_some() {
                v2 = v2.image(img_name);
            } else {
                v2 = v2.text(field_block(
                    raw,
                    &[
                        (
                            "Furnace",
                            &["furnace_level", "town_hall_level", "townHallLevel", "level"],
                        ),
                        ("Life tree", &["life_tree_level", "lifeTreeLevel"]),
                        ("Research", &["research_power", "researchPower"]),
                        ("Troops", &["troop_power", "troops_power", "troopPower"]),
                    ],
                ));
            }
        }
        "crowns" => {
            v2 = v2.title(format!("{name} — Crowns"));
            if keep_png.is_some() {
                v2 = v2.image(img_name);
            } else {
                v2 = v2.text(field_block(
                    raw,
                    &[
                        ("Crowns", &["crowns", "crown_count", "conquest_crowns"]),
                        ("Conquests", &["conquests", "king_of_kings", "kok"]),
                    ],
                ));
            }
        }
        "map" => {
            v2 = v2.title(format!("{name} — Location"));
            if keep_png.is_some() {
                v2 = v2.image(img_name);
            }
            let state = session
                .state
                .map(|s| s.to_string())
                .unwrap_or_else(|| "—".into());
            let x = pick_i64(raw, &["x", "pos_x"]);
            let y = pick_i64(raw, &["y", "pos_y"]);
            match (x, y) {
                (Some(x), Some(y)) => {
                    v2 = v2.text(format!("State **{state}** · X **{x}** · Y **{y}**"));
                }
                _ => {
                    v2 = v2.text(
                        "Coordinates are not available for this chief right now.",
                    );
                }
            }
            if let Some(notice) = &session.notice {
                v2 = v2.text(notice.clone());
            }
            let mut buttons = vec![
                ButtonDef {
                    id: "lookup:player:tab:overview".into(),
                    label: "← Back to overview".into(),
                    ..Default::default()
                },
                ButtonDef {
                    id: "lookup:player:map:refresh".into(),
                    label: "Refresh map".into(),
                    emoji: Some("🔄".into()),
                    ..Default::default()
                },
            ];
            if session.state.is_some() {
                // Full map link is owner-only in JS; we expose a link button for everyone here
                // (Discord will still respect any server-level restrictions).
                buttons.push(ButtonDef {
                    id: String::new(),
                    label: "Open full map".into(),
                    emoji: Some("🗺️".into()),
                    url: Some(format!(
                        "https://whiteoutstats.com/map?kid={}",
                        session.state.unwrap_or(0)
                    )),
                    ..Default::default()
                });
            }
            v2 = v2.buttons(&buttons);
        }
        "trail" => {
            v2 = v2.title(format!("{name} — Exploration trails"));
            // Prefer dedicated trails payload; fall back to trail-flagged boards
            let mut towers: Vec<String> = Vec::new();
            if let Some(trails) = raw
                .get("trails")
                .or_else(|| raw.get("trails_list"))
                .or_else(|| raw.get("towers"))
                .and_then(Value::as_array)
            {
                for (i, t) in trails.iter().enumerate().take(20) {
                    let tname = pick_str(t, &["name", "label"]).unwrap_or("Tower");
                    let rank = pick_i64(t, &["rank"]).unwrap_or((i + 1) as i64);
                    let stage = pick_str(t, &["value_label"])
                        .map(str::to_string)
                        .or_else(|| {
                            pick_i64(t, &["stage", "score", "level_id"])
                                .map(|s| format!("stage {s}"))
                        });
                    let score = pick_i64(t, &["score", "rawScore"]);
                    let mut bits = vec![
                        format!("{} **{tname}**", v2::rank_chip(rank)),
                        score.map(|s| v2::int(Some(s))).unwrap_or_else(|| "—".into()),
                    ];
                    if let Some(st) = stage {
                        bits.push(st);
                    }
                    if pick_i64(t, &["rank"]).is_some() {
                        bits.push(format!("#{rank} in state"));
                    }
                    towers.push(bits.join(" · "));
                }
            }
            if towers.is_empty() {
                // fallback from boards marked trail
                for (i, b) in boards(raw).iter().enumerate().take(20) {
                    let is_trail = b
                        .get("trail")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if !is_trail {
                        continue;
                    }
                    let tname = pick_str(b, &["name", "key"]).unwrap_or("Trail");
                    let rank = pick_i64(b, &["rank"]).unwrap_or((i + 1) as i64);
                    towers.push(format!(
                        "{} **{tname}** · {}",
                        v2::rank_chip(rank),
                        fmt_num(pick_i64(b, &["score", "rawScore"]))
                    ));
                }
            }
            if towers.is_empty() {
                v2 = v2.text("No exploration trail scores recorded for this chief yet.");
            } else {
                v2 = v2.text(towers.join("\n"));
            }
            v2 = v2.buttons(&[ButtonDef {
                id: "lookup:player:tab:overview".into(),
                label: "← Back to overview".into(),
                ..Default::default()
            }]);
        }
        _ => {
            v2 = v2.title(&name);
            v2 = v2.text("Unknown tab.");
        }
    }

    let footer = source_note.unwrap_or(footer_label);
    (v2.footer(footer, None), keep_png, img_name)
}

pub fn render_player_v2(
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

pub fn edit_player_v2(
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
