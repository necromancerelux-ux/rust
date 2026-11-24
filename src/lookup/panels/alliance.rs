//! Alliance Lookup — Components V2 panel (matches JS `alliancePanel.js`).

use serde_json::Value;
use crate::ui::v2::V2Edit;

use crate::lookup::sessions::LookupSession;
use crate::ui::v2::{self, ButtonDef, SelectOpt, Tab, V2};

pub const ALLIANCE_TABS: &[(&str, &str)] = &[
    ("overview", "Overview"),
    ("members", "Members"),
    ("furnace", "Furnace"),
    ("troopLevel", "Troop Level"),
    ("notice", "Notice"),
];

fn tabs_as_v2() -> Vec<Tab> {
    ALLIANCE_TABS
        .iter()
        .map(|(k, l)| Tab {
            key: k.to_string(),
            label: l.to_string(),
        })
        .collect()
}

fn valid_tab(tab: &str) -> &str {
    if ALLIANCE_TABS.iter().any(|(k, _)| *k == tab) || tab == "map" {
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

fn members(raw: &Value) -> Vec<&Value> {
    raw.get("members")
        .or_else(|| raw.get("players"))
        .or_else(|| raw.get("roster"))
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn build(
    session: &LookupSession,
    tab: &str,
    source_note: Option<&str>,
    card_png: Option<Vec<u8>>,
) -> (V2, Option<Vec<u8>>) {
    let tab = valid_tab(tab);
    let tabs = tabs_as_v2();
    let raw = &session.raw;
    let view = crate::lookup::view::alliance_view(raw);
    let mut v2 = V2::new()
        .tab_buttons("lookup:alliance:tab", &tabs, tab, 4)
        .sep(true);
    let mut keep_png = card_png;

    match tab {
        "overview" => {
            let title = if session.name.trim().is_empty() {
                session
                    .alliance
                    .clone()
                    .unwrap_or_else(|| "Alliance".into())
            } else {
                session.name.clone()
            };
            v2 = v2.title(&title);
            if keep_png.is_some() {
                v2 = v2.image("alliance.png");
            } else {
                let state = session
                    .state
                    .map(|s| format!("S{s}"))
                    .unwrap_or_else(|| "—".into());
                let tag = session.alliance.as_deref().unwrap_or("—");
                let power = pick_i64(raw, &["power", "total_power", "power_rank"]);
                let kills = pick_i64(raw, &["kills", "total_kills"]);
                let members_n =
                    pick_i64(raw, &["member_count", "members_count", "tracked_members"]).or_else(
                        || {
                            let m = members(raw);
                            if m.is_empty() {
                                None
                            } else {
                                Some(m.len() as i64)
                            }
                        },
                    );
                let mut lines = vec![format!("**Tag** `[{tag}]` · **State** `{state}`")];
                if let Some(p) = power {
                    lines.push(format!("**Power** `{}`", v2::fmt(Some(p))));
                }
                if let Some(k) = kills {
                    lines.push(format!("**Kills** `{}`", v2::fmt(Some(k))));
                }
                if let Some(n) = members_n {
                    lines.push(format!("**Members** `{n}`"));
                }
                v2 = v2.text(lines.join("\n"));
            }
            let mut buttons = Vec::new();
            if let Some(state) = session.state {
                buttons.push(ButtonDef {
                    id: "lookup:alliance:go:state".into(),
                    label: format!("S{state}"),
                    emoji: Some("🏰".into()),
                    ..Default::default()
                });
                buttons.push(ButtonDef {
                    id: "lookup:alliance:go:map".into(),
                    label: "Map".into(),
                    emoji: Some("🗺️".into()),
                    ..Default::default()
                });
            }
            if !buttons.is_empty() {
                v2 = v2.buttons(&buttons);
            }
        }
        "members" => {
            let title = if session.name.trim().is_empty() {
                "Members".into()
            } else {
                format!("{} — Members", session.name)
            };
            v2 = v2.title(&title);

            // Sort key stored in session.board when on members tab (power|furnace|role|name)
            let sort = session.board.as_deref().unwrap_or("power");
            let mut list = members(raw);
            match sort {
                "furnace" => list.sort_by_key(|row| {
                    std::cmp::Reverse(
                        pick_i64(row, &["furnace_level", "town_hall_level", "furnace", "level"])
                            .unwrap_or(0),
                    )
                }),
                "name" => list.sort_by(|a, b| {
                    let an = pick_str(a, &["name", "username", "nick_name", "game_name"])
                        .unwrap_or("")
                        .to_lowercase();
                    let bn = pick_str(b, &["name", "username", "nick_name", "game_name"])
                        .unwrap_or("")
                        .to_lowercase();
                    an.cmp(&bn)
                }),
                "role" => list.sort_by(|a, b| {
                    let ar = pick_str(a, &["role", "title"]).unwrap_or("").to_lowercase();
                    let br = pick_str(b, &["role", "title"]).unwrap_or("").to_lowercase();
                    ar.cmp(&br)
                }),
                _ => list.sort_by_key(|row| {
                    std::cmp::Reverse(pick_i64(row, &["power", "total_power"]).unwrap_or(0))
                }),
            }

            let per = 15usize;
            let total_pages = list.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages.saturating_sub(1));
            let slice: Vec<String> = list
                .iter()
                .skip(page * per)
                .take(per)
                .enumerate()
                .map(|(i, row)| {
                    let name = pick_str(row, &["name", "username", "nick_name", "game_name"])
                        .unwrap_or("—");
                    let power = pick_i64(row, &["power", "total_power"]);
                    let furnace =
                        pick_i64(row, &["furnace_level", "town_hall_level", "furnace", "level"]);
                    let role = pick_str(row, &["role", "title"]);
                    let rank = page * per + i + 1;
                    let mut bits = vec![format!("{} **{name}**", v2::rank_chip(rank as i64))];
                    if let Some(p) = power {
                        bits.push(v2::fmt(Some(p)));
                    }
                    if let Some(f) = furnace {
                        bits.push(format!("F{f}"));
                    }
                    if let Some(r) = role {
                        bits.push(r.to_string());
                    }
                    let mut tl = Vec::new();
                    if let Some(arr) = row.get("troop").and_then(Value::as_array) {
                        for t in arr {
                            let kind = pick_str(t, &["name", "kind"]).unwrap_or("?");
                            if let Some(tier) = pick_i64(t, &["tier"]) {
                                let fc = pick_i64(t, &["fc", "star"]);
                                tl.push(match fc {
                                    Some(fc) => format!("{kind} · T{tier} · FC{fc}"),
                                    None => format!("{kind} · T{tier}"),
                                });
                            }
                        }
                    }
                    if tl.is_empty() {
                        if let Some(obj) =
                            row.get("troopLevels").or_else(|| row.get("troop_levels"))
                        {
                            for (k, label) in [
                                ("infantry", "Infantry"),
                                ("lancer", "Lancer"),
                                ("marksman", "Marksman"),
                            ] {
                                if let Some(tier) = obj.get(k).and_then(|v| v.as_i64()) {
                                    tl.push(format!("{label} · T{tier}"));
                                }
                            }
                        }
                    }
                    if !tl.is_empty() {
                        bits.push(format!("[{}]", tl.join(" / ")));
                    }
                    bits.join(" · ")
                })
                .collect();
            if slice.is_empty() {
                v2 = v2.text("_No members in the stored payload._");
            } else {
                v2 = v2.text(slice.join("\n"));
                let sort_opts = [
                    ("power", "Power"),
                    ("furnace", "Furnace"),
                    ("role", "Role"),
                    ("name", "Name"),
                ]
                .map(|(v, l)| SelectOpt {
                    label: l.into(),
                    value: v.into(),
                    description: None,
                    is_default: v == sort,
                });
                v2 = v2.select("lookup:alliance:sort", "Sort members…", &sort_opts);
                if total_pages > 1 {
                    v2 = v2.pager("lookup:alliance:pg", page, total_pages);
                }
                let member_opts: Vec<SelectOpt> = list.iter().filter_map(|row| {
                    let fid = pick_str(row, &["fid", "chief_id", "id"]).or_else(|| pick_i64(row, &["fid", "id"]).map(|n| n.to_string()).as_deref().map(|_| "")).and_then(|_| None::<String>);
                    let fid = pick_str(row, &["fid", "chief_id", "id"]).map(|s| s.to_string()).or_else(|| pick_i64(row, &["fid", "chief_id", "id"]).map(|n| n.to_string()));
                    let fid = fid?;
                    let name = pick_str(row, &["name", "username", "nick_name", "game_name"]).unwrap_or("Unknown");
                    Some(SelectOpt { label: name.chars().take(100).collect(), value: fid.chars().take(100).collect(), description: None, is_default: false })
                }).take(25).collect();
                if !member_opts.is_empty() {
                    v2 = v2.select("lookup:alliance:player", "Open a player.", &member_opts);
                }
                // Custom footer with page counts (matches JS footerRaw)
                return (
                    v2.footer_raw(format!(
                        "Page {} / {} · {} members",
                        page + 1,
                        total_pages,
                        list.len()
                    )),
                    keep_png,
                );
            }
        }
        "furnace" => {
            v2 = v2.title(if session.name.trim().is_empty() {
                "Furnace".into()
            } else {
                format!("{} — Furnace", session.name)
            });
            let list = members(raw);
            let mut buckets: std::collections::BTreeMap<i64, usize> =
                std::collections::BTreeMap::new();
            for row in &list {
                if let Some(lvl) =
                    pick_i64(row, &["furnace_level", "town_hall_level", "furnace", "level"])
                {
                    *buckets.entry(lvl).or_default() += 1;
                }
            }
            if buckets.is_empty() {
                v2 = v2.text("_No furnace levels in the stored payload._");
            } else {
                let lines: Vec<String> = buckets
                    .iter()
                    .rev()
                    .take(20)
                    .map(|(lvl, n)| format!("• Furnace **{lvl}** — `{n}` members"))
                    .collect();
                v2 = v2.text(lines.join("\n"));
            }
        }
        "troopLevel" => {
            v2 = v2.title(if session.name.trim().is_empty() {
                "Troop Level".into()
            } else {
                format!("{} — Troop Level", session.name)
            });
            let mut list = members(raw);
            list.sort_by_key(|row| {
                let has = row
                    .get("troop")
                    .and_then(Value::as_array)
                    .map(|a| !a.is_empty())
                    .or_else(|| {
                        row.get("troopLevels")
                            .or_else(|| row.get("troop_levels"))
                            .map(|v| v.as_object().map(|o| !o.is_empty()).unwrap_or(false))
                    })
                    .unwrap_or(false);
                let power = pick_i64(row, &["power", "total_power"]).unwrap_or(0);
                (std::cmp::Reverse(has), std::cmp::Reverse(power))
            });
            let per = 12usize;
            let total_pages = list.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages.saturating_sub(1));
            let slice: Vec<String> = list
                .iter()
                .skip(page * per)
                .take(per)
                .enumerate()
                .map(|(i, row)| {
                    let name = pick_str(row, &["name", "username", "nick_name", "game_name"])
                        .unwrap_or("—");
                    let rank = page * per + i + 1;
                    let mut tl = Vec::new();
                    if let Some(arr) = row.get("troop").and_then(Value::as_array) {
                        for t in arr {
                            let kind = pick_str(t, &["name", "kind"]).unwrap_or("?");
                            if let Some(tier) = pick_i64(t, &["tier"]) {
                                let fc = pick_i64(t, &["fc", "star"]);
                                tl.push(match fc {
                                    Some(fc) => format!("{kind} · T{tier} · FC{fc}"),
                                    None => format!("{kind} · T{tier}"),
                                });
                            }
                        }
                    }
                    if tl.is_empty() {
                        if let Some(obj) =
                            row.get("troopLevels").or_else(|| row.get("troop_levels"))
                        {
                            for (k, label) in [
                                ("infantry", "Infantry"),
                                ("lancer", "Lancer"),
                                ("marksman", "Marksman"),
                            ] {
                                if let Some(tier) = obj.get(k).and_then(|v| v.as_i64()) {
                                    tl.push(format!("{label} · T{tier}"));
                                }
                            }
                        }
                    }
                    let extra = if tl.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", tl.join(" / "))
                    };
                    format!("{} **{name}**{extra}", v2::rank_chip(rank as i64))
                })
                .collect();
            if slice.is_empty() {
                v2 = v2.text(
                    "_Troop tier breakdown is shown when the alliance payload includes troop stats._",
                );
            } else {
                v2 = v2.text(slice.join("\n"));
                if total_pages > 1 {
                    v2 = v2.pager("lookup:alliance:tl:pg", page, total_pages);
                }
                let member_opts: Vec<SelectOpt> = list.iter().filter_map(|row| {
                    let fid = pick_str(row, &["fid", "chief_id", "id"]).map(|s| s.to_string()).or_else(|| pick_i64(row, &["fid", "chief_id", "id"]).map(|n| n.to_string()));
                    let fid = fid?;
                    let name = pick_str(row, &["name", "username", "nick_name", "game_name"]).unwrap_or("Unknown");
                    Some(SelectOpt { label: name.chars().take(100).collect(), value: fid.chars().take(100).collect(), description: None, is_default: false })
                }).take(25).collect();
                if !member_opts.is_empty() {
                    v2 = v2.select("lookup:alliance:player", "Open a player.", &member_opts);
                }
                v2 = v2.buttons(&[
                    ButtonDef { id: "lookup:alliance:tl:refresh".into(), label: "Refresh troop levels".into(), emoji: Some("🔄".into()), ..Default::default() },
                    ButtonDef { id: "lookup:alliance:tl:back".into(), label: "Back to overview".into(), emoji: Some("⬅️".into()), ..Default::default() },
                ]);
            }
        }
        "notice" => {
            v2 = v2.title("Notice");
            let notice = session
                .notice
                .as_deref()
                .or_else(|| pick_str(raw, &["notice", "announcement", "message"]))
                .unwrap_or("_No alliance notice on file._");
            v2 = v2.text(notice);
        }
        "map" => {
            v2 = v2.title(if session.name.trim().is_empty() { "Map".into() } else { format!("{} — Map", session.name) });
            if keep_png.is_some() {
                v2 = v2.image("alliance-map.png");
            } else {
                v2 = v2.text("_Map overlay will appear when location data is available._");
            }
            if let Some(notice) = &session.notice {
                v2 = v2.text(notice.clone());
            }
            v2 = v2.buttons(&[
                ButtonDef { id: "lookup:alliance:map:back".into(), label: "Back to overview".into(), emoji: Some("⬅️".into()), ..Default::default() },
                ButtonDef { id: "lookup:alliance:map:refresh".into(), label: "Refresh map".into(), emoji: Some("🔄".into()), ..Default::default() },
            ]);
        }
        _ => {
            v2 = v2.title(&session.name);
            v2 = v2.text("Unknown tab.");
        }
    }

    let footer = source_note.unwrap_or("Lookup");
    (v2.footer(footer, view.as_of_ms), keep_png)
}

pub fn render_alliance_v2(
    session: &LookupSession,
    tab: &str,
    source_note: Option<&str>,
    card_png: Option<Vec<u8>>,
) -> V2Edit {
    let (v2, png) = build(session, tab, source_note, card_png);
    let mut files = Vec::new();
    if let Some(png) = png {
        files.push(v2::attachment(png, "alliance.png"));
    }
    v2::payload(v2, files, true)
}

pub fn edit_alliance_v2(
    session: &LookupSession,
    tab: &str,
    source_note: Option<&str>,
    card_png: Option<Vec<u8>>,
) -> V2Edit {
    render_alliance_v2(session, tab, source_note, card_png)
}
