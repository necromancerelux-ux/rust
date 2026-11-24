//! Player Lookup — Components V2 panel (matches JS `playerPanel.js` 1:1).
//!
//! Layout target (same as JS):
//!   ┃ [ tab dropdown — placeholder = current tab, defaults to Overview ]
//!   ┃ ───────────────
//!  ┃ ## Title
//!   ┃ body / rendered image card(s)
//!   ┃ [ action buttons / selects ]
//!   ┃ ───────────────
//!   ┃ -# OmniCore · <label> <t:…:R>
//!
//! Tabs: Overview · Standings · Heroes & Gear · Progression · Crowns
//! Extra views (via buttons): map · trail

use crate::ui::v2::V2Edit;
use serde_json::Value;

use crate::lookup::sessions::LookupSession;
use crate::lookup::view::{self, BoardRow};
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
        .unwrap_or("Overview")
}

/// JS `tabSelect`: only main tabs are selectable — sub-views fall back to
/// Overview for the dropdown placeholder/default.
fn select_key(tab: &str) -> &str {
    if PLAYER_TABS.iter().any(|(k, _)| *k == tab) {
        tab
    } else {
        "overview"
    }
}

fn valid_tab(tab: &str) -> &str {
    if PLAYER_TABS.iter().any(|(k, _)| *k == tab) || matches!(tab, "map" | "trail") {
        tab
    } else {
        "overview"
    }
}

fn fmt_num(n: Option<i64>) -> String {
    v2::fmt(n)
}

/// Images the caller rendered for this tab (JS `files` array).
type Images = Vec<(String, Vec<u8>)>;

fn build(session: &LookupSession, tab: &str, images: Images) -> (V2, Images) {
    let tab = valid_tab(tab);
    let raw = &session.raw;
    let pv = view::player_view(raw);
    let name = if !session.name.trim().is_empty() {
        session.name.clone()
    } else if !pv.name.trim().is_empty() && pv.name != "Unknown" {
        pv.name.clone()
    } else {
        session.fid.clone().unwrap_or_else(|| "Player".into())
    };

    // Main tab dropdown (JS: placeholder = current tab, default = current tab)
    let active = select_key(tab);
    let opts: Vec<SelectOpt> = PLAYER_TABS
        .iter()
        .map(|(k, l)| SelectOpt {
            label: l.to_string(),
            value: k.to_string(),
            description: None,
            is_default: *k == active,
        })
        .collect();

    let mut v2 = V2::new()
        .select("lookup:player:tab", tab_label(active), &opts)
        .sep(true);

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
            if images.is_empty() {
                // Text fallback when no card was rendered
                let fid = session.fid.as_deref().or(pv.fid.as_deref()).unwrap_or("—");
                let state_txt = session
                    .state
                    .or(pv.state_id)
                    .map(|s| format!("S{s}"))
                    .unwrap_or_else(|| "—".into());
                let alliance_txt = session
                    .alliance
                    .as_deref()
                    .or(pv.alliance_tag.as_deref())
                    .map(|t| format!("[{t}]"))
                    .unwrap_or_else(|| "—".into());
                let role = if pv.role.trim().is_empty() || pv.role == "—" {
                    "—".to_string()
                } else {
                    pv.role.clone()
                };
                v2 = v2.text(format!(
                    "**FID** `{fid}` · **State** `{state_txt}` · **Alliance** `{alliance_txt}` · **Role** `{role}`"
                ));
                let mut bits = Vec::new();
                if let Some(p) = pv.power {
                    bits.push(format!("**Power** `{}`", v2::fmt(Some(p))));
                }
                if let Some(k) = pv.kills {
                    bits.push(format!("**Kills** `{}`", v2::fmt(Some(k))));
                }
                if let Some(f) = pv.furnace {
                    bits.push(format!("**Furnace** `FC {f}`"));
                }
                if !bits.is_empty() {
                    v2 = v2.text(bits.join(" · "));
                }
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
            }

            // Action buttons matching JS order & labels
            let mut buttons = Vec::new();
            let state_id = session.state.or(pv.state_id);
            let alliance_tag = session.alliance.clone().or_else(|| pv.alliance_tag.clone());
            if let Some(state) = state_id {
                buttons.push(ButtonDef {
                    id: "lookup:player:go:state".into(),
                    label: format!("S{state}"),
                    emoji: None,
                    ..Default::default()
                });
            }
            if let Some(tag) = alliance_tag.as_deref() {
                buttons.push(ButtonDef {
                    id: "lookup:player:go:alliance".into(),
                    label: format!("[{tag}] Alliance"),
                    emoji: None,
                    ..Default::default()
                });
            }
            // JS only offers the map jump when coordinates are known.
            if pv.x.is_some() && pv.y.is_some() {
                buttons.push(ButtonDef {
                    id: "lookup:player:go:map".into(),
                    label: "Locate on map".into(),
                    emoji: Some("🗺️".into()),
                    ..Default::default()
                });
            }
            buttons.push(ButtonDef {
                id: "lookup:player:go:trail".into(),
                label: "Trails".into(),
                ..Default::default()
            });
            v2 = v2.buttons(&buttons);
        }
        "standings" => {
            v2 = v2.title(format!("{name} — Leaderboard standings"));
            if images.is_empty() {
                let list = view::player_boards(raw);
                let per = 15usize;
                let total_pages = list.len().div_ceil(per).max(1);
                let page = session.page.min(total_pages - 1);
                let slice: Vec<String> = list
                    .iter()
                    .skip(page * per)
                    .take(per)
                    .map(|b| {
                        let rank = b
                            .rank
                            .map(|r| format!("#{r}"))
                            .unwrap_or_else(|| "—".into());
                        format!("• **{}** — `{rank}`", b.name)
                    })
                    .collect();
                if slice.is_empty() {
                    v2 = v2.text("_No leaderboard standings stored for this chief._");
                } else {
                    v2 = v2.text(slice.join("\n"));
                }
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
            }

            // Board open selects (JS: state boards + global boards)
            let boards = view::player_boards(raw);
            let state_id = session.state.or(pv.state_id);
            let st: Vec<SelectOpt> = boards
                .iter()
                .filter(|b| b.rank.is_some())
                .take(25)
                .map(|b| SelectOpt {
                    label: b.name.clone(),
                    value: if b.key.is_empty() {
                        b.name.clone()
                    } else {
                        b.key.clone()
                    },
                    description: Some(format!(
                        "#{} in S{} · {}",
                        b.rank.unwrap_or(0),
                        state_id.map(|s| s.to_string()).unwrap_or_default(),
                        b.score_label()
                    )),
                    is_default: false,
                })
                .collect();
            if !st.is_empty() {
                v2 = v2.select("lookup:player:board:state", "Open a state board…", &st);
            }
            let gl: Vec<SelectOpt> = boards
                .iter()
                .filter_map(|b| {
                    let global = b.global.clone()?;
                    Some(SelectOpt {
                        label: b.name.clone(),
                        value: if b.key.is_empty() {
                            b.name.clone()
                        } else {
                            b.key.clone()
                        },
                        description: Some(global),
                        is_default: false,
                    })
                })
                .take(25)
                .collect();
            if !gl.is_empty() {
                v2 = v2.select("lookup:player:board:global", "Open a global board…", &gl);
            }
        }
        "heroes" => {
            v2 = v2.title(format!("{name} — Heroes & Gear"));
            let has_gear = raw
                .get("chiefGear")
                .or_else(|| raw.get("chief_gear"))
                .map(|g| {
                    g.as_array().map(|a| !a.is_empty()).unwrap_or_else(|| {
                        g.get("pieces")
                            .and_then(Value::as_array)
                            .map(|a| !a.is_empty())
                            .unwrap_or_else(|| {
                                g.as_object().map(|o| !o.is_empty()).unwrap_or(false)
                            })
                    })
                })
                .unwrap_or(false)
                || images.iter().any(|(n, _)| n == "chief-gear.png");
            let heroes = raw
                .get("heroes")
                .or_else(|| raw.get("hero_list"))
                .and_then(Value::as_array)
                .map(|a| !a.is_empty())
                .unwrap_or(false)
                || images.iter().any(|(n, _)| n == "arena.png");

            if !has_gear && !heroes {
                v2 = v2.text("No chief gear or arena heroes stored for this chief yet.");
            } else {
                // Quick bits line: TG · VIP · level (JS order)
                let mut bits = Vec::new();
                if let Some(tg) = pv.tg_label.as_deref() {
                    bits.push(tg.to_string());
                } else if let Some(lvl) = pv.tg_level {
                    bits.push(format!("TG{lvl}"));
                }
                if let Some(vip) = pv.vip {
                    bits.push(match pv.vip_active {
                        Some(false) => format!("VIP {vip} (inactive)"),
                        _ => format!("VIP {vip}"),
                    });
                }
                if let Some(level) = view::pick_str(raw, &["level_label"]) {
                    bits.push(level.to_string());
                }
                if !bits.is_empty() {
                    v2 = v2.text(bits.join(" · "));
                }
            }

            // Chief gear + charms image (JS `chief-gear.png`)
            let rendered_chief = images.iter().any(|(n, _)| n == "chief-gear.png");
            if rendered_chief {
                for (image, _) in images.iter().filter(|(n, _)| n == "chief-gear.png") {
                    v2 = v2.image(image);
                }
            } else if has_gear || !heroes {
                if pv.gear_privacy {
                    v2 = v2.text(format!(
                        "🔒 {}",
                        pv.gear_privacy_message
                            .clone()
                            .unwrap_or_else(|| "Chief Gear privacy is active — equipment is hidden on the public site.".to_string())
                    ));
                } else {
                    v2 = v2.text(
                        "Chief gear & charms are not available for this chief right now. If the website recently added gear data, try again in a moment.",
                    );
                }
            }

            // Arena lineup image (JS `arena.png`)
            for (image, _) in images.iter().filter(|(n, _)| n == "arena.png") {
                v2 = v2.image(image);
            }
        }
        "progression" => {
            v2 = v2.title(format!("{name} — Progression"));
            if images.is_empty() {
                v2 = v2.text(view_field_block(
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
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
            }
        }
        "crowns" => {
            v2 = v2.title(format!("{name} — Crowns"));
            if images.is_empty() {
                v2 = v2.text(view_field_block(
                    raw,
                    &[
                        ("Crowns", &["crowns", "crown_count", "conquest_crowns"]),
                        ("Conquests", &["conquests", "king_of_kings", "kok"]),
                    ],
                ));
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
            }
        }
        "map" => {
            v2 = v2.title(format!("{name} — Location"));
            let state = session.state.or(pv.state_id);
            if state.is_none() {
                v2 = v2.text("No state is linked to this chief, so a map cannot be loaded.");
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
                let x = pv.x.or_else(|| view::pick_i64(raw, &["x", "pos_x"]));
                let y = pv.y.or_else(|| view::pick_i64(raw, &["y", "pos_y"]));
                match (x, y) {
                    (Some(x), Some(y)) => {
                        if images.is_empty() {
                            v2 = v2.text(format!(
                                "State **{}** · X **{x}** · Y **{y}**\n\nMap overlay data is not available right now.",
                                state.unwrap_or(0)
                            ));
                        } else {
                            v2 = v2.text(format!(
                                "State **{}** · X **{x}** · Y **{y}**",
                                state.unwrap_or(0)
                            ));
                        }
                    }
                    _ => {
                        v2 = v2.text("Coordinates are not available for this chief right now.");
                    }
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
                if let Some(kid) = state {
                    // Full map link is owner-only in JS; kept as a link button here.
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
            if let Some(notice) = &session.notice {
                v2 = v2.text(notice.clone());
            }
        }
        "trail" => {
            v2 = v2.title(format!("{name} — Exploration trails"));
            // Prefer dedicated trails payload; fall back to trail-flagged boards
            let mut towers: Vec<String> = Vec::new();
            let trails = raw
                .get("trails")
                .or_else(|| raw.get("trails_list"))
                .or_else(|| raw.get("trails_json"))
                .or_else(|| raw.get("towers"));
            let trails: Vec<Value> = match trails {
                Some(Value::Array(a)) => a.clone(),
                Some(Value::Object(o)) => o.values().filter(|v| v.is_object()).cloned().collect(),
                _ => {
                    // trails_json may be a JSON string
                    raw.get("trails_json")
                        .and_then(Value::as_str)
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .map(|v| match v {
                            Value::Array(a) => a,
                            Value::Object(o) => {
                                o.values().filter(|v| v.is_object()).cloned().collect()
                            }
                            _ => Vec::new(),
                        })
                        .unwrap_or_default()
                }
            };
            for (i, t) in trails.iter().enumerate().take(20) {
                let tname = view::pick_str(t, &["name", "label"])
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        format!(
                            "Tower {}",
                            view::pick_i64(t, &["tower_id", "id"])
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "?".into())
                        )
                    });
                let rank = view::pick_i64(t, &["rank"]);
                let stage = view::pick_str(t, &["value_label"])
                    .map(str::to_string)
                    .or_else(|| view::pick_i64(t, &["stage"]).map(|s| format!("stage {s}")));
                let score = view::pick_i64(t, &["score", "level_id"]);
                let mut bits = vec![
                    format!(
                        "{} **{tname}**",
                        v2::rank_chip(rank.unwrap_or((i + 1) as i64))
                    ),
                    score
                        .map(|s| v2::int(Some(s)))
                        .unwrap_or_else(|| "—".into()),
                ];
                if let Some(st) = stage {
                    bits.push(st);
                }
                if rank.is_some() {
                    bits.push(format!("#{} in state", rank.unwrap_or(0)));
                }
                towers.push(bits.join(" · "));
            }
            if towers.is_empty() {
                // fallback from boards marked trail
                for (i, b) in view::player_boards(raw).iter().enumerate().take(20) {
                    if !b.trail {
                        continue;
                    }
                    towers.push(format!(
                        "{} **{}** · {}",
                        v2::rank_chip(b.rank.unwrap_or((i + 1) as i64)),
                        b.name,
                        fmt_num(b.score)
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

    let as_of = if tab == "map" {
        raw.get("mapSummary")
            .and_then(|s| s.get("at"))
            .and_then(Value::as_i64)
            .or(pv.as_of_ms)
    } else {
        pv.as_of_ms
    };
    (v2.footer(footer_label, as_of), images)
}

fn view_field_block(raw: &Value, pairs: &[(&str, &[&str])]) -> String {
    let mut lines = Vec::new();
    for (label, keys) in pairs {
        if let Some(n) = view::pick_i64(raw, keys) {
            lines.push(format!("**{label}** `{}`", v2::fmt(Some(n))));
        } else if let Some(s) = view::pick_str(raw, keys) {
            lines.push(format!("**{label}** `{s}`"));
        }
    }
    if lines.is_empty() {
        "_No data for this section._".into()
    } else {
        lines.join(" · ")
    }
}

/// Board rows exposed for the standings card renderer.
pub fn standings_rows(raw: &Value) -> Vec<BoardRow> {
    view::player_boards(raw)
}

pub fn edit_player_v2(session: &LookupSession, tab: &str, images: Images) -> V2Edit {
    let (v2, images) = build(session, tab, images);
    let files = images
        .into_iter()
        .map(|(name, bytes)| v2::attachment(bytes, &name))
        .collect();
    v2::payload(v2, files, true)
}
