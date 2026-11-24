//! State Lookup — Components V2 panel (matches JS `statePanel.js`).
//!
//! Layout target (same as JS):
//!   ┃ [ tab dropdown — placeholder = current tab, defaults to Overview ]
//!   ┃ ───────────────
//!   ┃ ## Title
//!   ┃ body / rendered image card
//!   ┃ [ action buttons / selects / pager ]
//!   ┃ ───────────────
//!   ┃ -# OmniCore · <label> <t:…:R>
//!
//! Tabs: Overview · State vs State · Rankings · Alliances · Troop Level · Appointments
//! Sub-views: map · prep · history · battle · note (via buttons)

use crate::ui::v2::V2Edit;
use serde_json::Value;

use crate::lookup::sessions::LookupSession;
use crate::lookup::view::{self, SvsRow};
use crate::ui::v2::{self, ButtonDef, SelectOpt, V2};

pub const STATE_TABS: &[(&str, &str)] = &[
    ("overview", "Overview"),
    ("svs", "State vs State"),
    ("boards", "Rankings"),
    ("alliances", "Alliances"),
    ("troopLevel", "Troop Level"),
    ("appointments", "Appointments"),
];

/// JS `tabLabel`: sub-views have their own labels, unknown keys fall back.
fn tab_label(key: &str) -> &str {
    STATE_TABS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, l)| *l)
        .unwrap_or(match key {
            "battle" => "State vs State",
            "map" => "Map",
            "appointments" => "Appointments",
            "history" => "Matchup history",
            "prep" => "Prep points",
            _ => "Overview",
        })
}

fn valid_tab(tab: &str) -> &str {
    if STATE_TABS.iter().any(|(k, _)| *k == tab)
        || matches!(tab, "map" | "battle" | "history" | "prep" | "note")
    {
        tab
    } else {
        "overview"
    }
}

/// JS `tabSelect`: only main tabs are selectable — sub-views fall back to
/// Overview for the dropdown placeholder/default.
fn select_key(tab: &str) -> &str {
    if STATE_TABS.iter().any(|(k, _)| *k == tab) {
        tab
    } else {
        "overview"
    }
}

type Images = Vec<(String, Vec<u8>)>;

fn list_key<'a>(raw: &'a Value, keys: &[&str]) -> Vec<&'a Value> {
    for k in keys {
        if let Some(arr) = raw.get(*k).and_then(Value::as_array) {
            return arr.iter().collect();
        }
    }
    Vec::new()
}

/// Troop-level compact chips for board lines: `[I12/L11/M11]`.
fn tier_chip(row: &Value) -> String {
    let tl = row.get("troopLevels").or_else(|| row.get("troop_levels"));
    let Some(tl) = tl else {
        return String::new();
    };
    let mut bits = Vec::new();
    for (k, prefix) in [("infantry", "I"), ("lancer", "L"), ("marksman", "M")] {
        if let Some(v) = tl.get(k).and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        }) {
            bits.push(format!("{prefix}{v}"));
        }
    }
    if bits.is_empty() {
        String::new()
    } else {
        format!(" [{}]", bits.join("/"))
    }
}

fn build(session: &LookupSession, tab: &str, images: Images) -> (V2, Images) {
    let tab = valid_tab(tab);
    let raw = &session.raw;
    let sv = view::state_view(raw);
    let state_id = session
        .state
        .or(sv.id)
        .or_else(|| view::pick_i64(raw, &["kid"]));
    let state_label = state_id
        .map(|s| format!("State {s}"))
        .unwrap_or_else(|| "State".into());

    // JS: sub-views show the Overview option as default.
    let active = select_key(tab);
    let opts: Vec<SelectOpt> = STATE_TABS
        .iter()
        .map(|(k, l)| SelectOpt {
            label: l.to_string(),
            value: k.to_string(),
            description: None,
            is_default: *k == active,
        })
        .collect();

    let mut v2 = V2::new()
        .select("lookup:state:tab", tab_label(active), &opts)
        .sep(true);

    match tab {
        "overview" => {
            v2 = v2.title(&state_label);
            for (image, _) in images.iter() {
                v2 = v2.image(image);
            }
            if images.is_empty() {
                let mut lines = Vec::new();
                if let Some(p) = sv.power {
                    lines.push(format!("**Power** `{}`", v2::fmt(Some(p))));
                }
                if let Some(n) = sv.players {
                    lines.push(format!("**Players** `{n}`"));
                }
                if let Some(n) = sv.alliances {
                    lines.push(format!("**Alliances** `{n}`"));
                }
                if let Some(season) = sv.season {
                    lines.push(format!("**Season** `S{season}`"));
                }
                if lines.is_empty() {
                    lines.push("_State overview will fill when upstream data is present._".into());
                }
                v2 = v2.text(lines.join(" · "));
            }

            // JS bits line: President · Alliance · Rank · Health
            let mut bits = Vec::new();
            if let Some(president) = &sv.president {
                bits.push(format!("**President** · {president}"));
            }
            if let Some(tag) = &sv.ruling_alliance_tag {
                let mut line = format!("**Alliance** · [{tag}]");
                if let Some(name) = &sv.ruling_alliance_name {
                    line.push_str(&format!(" {name}"));
                }
                bits.push(line);
            }
            if let Some(rank) = sv.rank {
                bits.push(format!("**Rank** · #{rank}"));
            }
            if let Some(grade) = &sv.grade {
                if grade != "—" {
                    let mut line = format!("**Health** · {grade}");
                    if let Some(health) = sv.health {
                        line.push_str(&format!(" ({health})"));
                    }
                    bits.push(line);
                }
            }
            if !bits.is_empty() {
                v2 = v2.text(bits.join(" · "));
            }

            v2 = v2.buttons(&[ButtonDef {
                id: "lookup:state:go:map".into(),
                label: "Open map".into(),
                emoji: Some("🗺️".into()),
                ..Default::default()
            }]);
        }
        "svs" => {
            let matchup = raw
                .get("kvk_matchup")
                .or_else(|| raw.get("matchup"))
                .or_else(|| view::state_root(raw).get("kvk_matchup"))
                .or_else(|| view::state_root(raw).get("matchup"));
            let season = matchup
                .and_then(|m| view::pick_i64(m, &["season"]))
                .or(sv.season);
            let mut title = format!("{state_label} - State vs State");
            if let Some(season) = season {
                title.push_str(&format!(" · Season {season}"));
            }
            v2 = v2.title(title);
            if images.is_empty() {
                v2 = v2.text("_No SvS matchup data in the stored payload._");
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
            }

            let them = matchup.and_then(|m| m.get("opponent").or_else(|| m.get("them")));
            let them_kid = them.and_then(|t| view::pick_i64(t, &["kid", "state"]));
            if let Some(kid) = them_kid {
                let mut line = format!("**Opponent** · State {kid}");
                if let Some(stage) =
                    matchup.and_then(|m| view::pick_str(m, &["stage_name", "stage"]))
                {
                    line.push_str(&format!(" · {stage}"));
                }
                if let Some(name) = them.and_then(|t| view::pick_str(t, &["state_name", "name"])) {
                    line.push_str(&format!(" · {name}"));
                }
                v2 = v2.text(line);
            } else {
                v2 = v2.text("No active SvS matchup for this state right now.");
            }
            if let Some(note) = view::pick_str(raw, &["mirror_note", "mirrorNote"]) {
                v2 = v2.text(note);
            }
            if let Some(draw) = view::pick_str(raw, &["next_draw_label", "nextDraw"]) {
                let mut line = format!("Next draw {draw}");
                if let Some(group) = view::pick_str(raw, &["draw_group_label", "drawGroup"]) {
                    line.push_str(&format!(" · {group}"));
                }
                v2 = v2.text(line);
            }

            // Only offer sub-views that actually have public data behind them.
            let rows = view::svs_rows(raw);
            let prep_rows: Vec<&SvsRow> = rows
                .iter()
                .filter(|r| r.prep_score.is_some() || r.days > 0)
                .collect();
            let battles = view::pick_i64(raw, &["battles", "battle_count"]).is_some();
            let president = sv.president.clone();
            let mut svs_buttons = Vec::new();
            if battles || them_kid.is_some() {
                svs_buttons.push(ButtonDef {
                    id: "lookup:state:go:battle".into(),
                    label: "Sunfire Castle battle".into(),
                    ..Default::default()
                });
            }
            if !rows.is_empty() {
                svs_buttons.push(ButtonDef {
                    id: "lookup:state:go:history".into(),
                    label: "Matchup history".into(),
                    ..Default::default()
                });
            }
            if !prep_rows.is_empty() {
                svs_buttons.push(ButtonDef {
                    id: "lookup:state:go:prep".into(),
                    label: "Prep points".into(),
                    ..Default::default()
                });
            }
            if let Some(name) = &president {
                let label: String = name.chars().take(40).collect();
                svs_buttons.push(ButtonDef {
                    id: "lookup:state:go:president".into(),
                    label: format!("President - {label}"),
                    ..Default::default()
                });
            }
            if svs_buttons.is_empty() {
                v2 = v2.text("No State-vs-State details are published for this state right now.");
            } else {
                v2 = v2.buttons(&svs_buttons);
            }
            return (v2.footer("data as of", sv.as_of_ms), images);
        }
        "boards" => {
            let board_key = session.board.clone().unwrap_or_else(|| "3".into());
            let board_label = view::board_name(&board_key);
            let entries: Vec<Value> = raw
                .get("boardEntries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| {
                    list_key(raw, &["boards", "leaderboards", "rankings", "entries"])
                        .into_iter()
                        .cloned()
                        .collect()
                });
            v2 = v2.title(format!(
                "{state_label} — {board_label} · {} ranked",
                entries.len()
            ));
            let per = 15usize;
            let total_pages = entries.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages - 1);
            let items: Vec<&Value> = entries.iter().skip(page * per).take(per).collect();

            let lines: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let is_alliance = row.get("aid").is_some()
                        && row.get("nick_name").is_none()
                        && (row.get("abbr").is_some() || row.get("name").is_some());
                    let tl = tier_chip(row);
                    let name_part = if is_alliance {
                        let tag = view::pick_str(row, &["abbr", "tag"]).unwrap_or("???");
                        let name = view::pick_str(row, &["name"]).unwrap_or("");
                        let mut s = format!("**[{tag}] {name}**");
                        if let Some(members) = view::pick_i64(row, &["member_count", "members"]) {
                            s.push_str(&format!(" · {members} members"));
                        }
                        s
                    } else {
                        let tag = view::pick_str(row, &["alliance_abbr", "abbr", "tag"]);
                        let base = match tag {
                            Some(tag) => format!(
                                "**[{tag}] {}**",
                                view::pick_str(row, &["nick_name", "name", "label"])
                                    .unwrap_or("Unknown")
                            ),
                            None => format!(
                                "**{}**",
                                view::pick_str(row, &["nick_name", "name", "label"])
                                    .unwrap_or("Unknown")
                            ),
                        };
                        match view::pick_i64(row, &["furnace", "stove_lv"]) {
                            Some(f) => format!("{base} · F{f}"),
                            None => base,
                        }
                    };
                    let rank = view::pick_i64(row, &["rank", "position"])
                        .map(|r| r as usize)
                        .unwrap_or(page * per + i + 1);
                    let score = view::pick_i64(row, &["score", "value", "power", "game_power"])
                        .map(|n| v2::int(Some(n)))
                        .unwrap_or_else(|| "—".into());
                    format!("{} {name_part} · {score}{tl}", v2::rank_chip(rank as i64))
                })
                .collect();
            v2 = v2.text(if lines.is_empty() {
                "No rankings for this board yet.".to_string()
            } else {
                lines.join("\n")
            });

            // Board select (JS: placeholder = current board name, descriptions
            // carry the group + top-100 hint).
            let board_opts: Vec<SelectOpt> = view::BOARDS
                .iter()
                .map(|(key, label, group)| SelectOpt {
                    label: label.to_string(),
                    value: key.to_string(),
                    description: Some(format!("{group} · top 100")),
                    is_default: *key == board_key,
                })
                .collect();
            v2 = v2.select("lookup:state:board", board_label, &board_opts);
            v2 = v2.pager("lookup:state:pg", page, total_pages);

            let is_alliance_board = board_key == "1"
                || board_key == "2"
                || view::board_name(&board_key)
                    .to_lowercase()
                    .contains("alliance")
                || (!items.is_empty()
                    && items
                        .iter()
                        .all(|row| row.get("aid").is_some() && row.get("nick_name").is_none()));
            if is_alliance_board {
                let opts: Vec<SelectOpt> = items
                    .iter()
                    .filter_map(|row| {
                        let tag = view::pick_str(row, &["abbr", "tag", "alliance_abbr"])?;
                        let aid = view::pick_i64(row, &["aid"]).map(|n| n.to_string());
                        let value = aid.unwrap_or_else(|| tag.to_string());
                        let name = view::pick_str(row, &["name"]).unwrap_or("");
                        let label = if name.is_empty() || name == tag {
                            format!("[{tag}]")
                        } else {
                            format!("[{tag}] {name}")
                        };
                        let desc = view::pick_i64(row, &["score", "value", "power"])
                            .map(|n| v2::fmt(Some(n)))
                            .or_else(|| {
                                view::pick_i64(row, &["member_count", "members"])
                                    .map(|m| format!("{m} members"))
                            });
                        Some(SelectOpt {
                            label: label.chars().take(100).collect(),
                            value: value.chars().take(100).collect(),
                            description: desc,
                            is_default: false,
                        })
                    })
                    .take(25)
                    .collect();
                if !opts.is_empty() {
                    v2 = v2.select("lookup:state:alliance", "Open an alliance", &opts);
                }
            } else {
                let opts: Vec<SelectOpt> = items
                    .iter()
                    .filter_map(|row| {
                        let id = view::pick_str(row, &["uid"])
                            .map(str::to_string)
                            .or_else(|| {
                                view::pick_str(row, &["fid", "chief_id"]).map(str::to_string)
                            })
                            .or_else(|| {
                                view::pick_i64(row, &["fid", "chief_id", "uid"])
                                    .map(|n| n.to_string())
                            })?;
                        let tag = view::pick_str(row, &["alliance_abbr", "abbr", "tag"]);
                        let base = view::pick_str(row, &["nick_name", "name", "label"])
                            .unwrap_or("Unknown");
                        let label = match tag {
                            Some(tag) => format!("[{tag}] {base}"),
                            None => base.to_string(),
                        };
                        let desc = view::pick_i64(row, &["score", "value", "power", "game_power"])
                            .map(|n| v2::fmt(Some(n)));
                        Some(SelectOpt {
                            label: label.chars().take(100).collect(),
                            value: id.chars().take(100).collect(),
                            description: desc,
                            is_default: false,
                        })
                    })
                    .take(25)
                    .collect();
                if !opts.is_empty() {
                    v2 = v2.select("lookup:state:player", "Open a player", &opts);
                }
            }
            return (v2.footer("data as of", sv.as_of_ms), images);
        }
        "troopLevel" => {
            let all: Vec<Value> = raw
                .get("tlPlayers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| {
                    view::state_root(raw)
                        .get("players")
                        .or_else(|| view::state_root(raw).get("top_players"))
                        .and_then(Value::as_array)
                        .map(|a| a.iter().take(200).cloned().collect())
                        .unwrap_or_default()
                });
            let with_data = all
                .iter()
                .filter(|p| {
                    p.get("troopLevels")
                        .or_else(|| p.get("troop_levels"))
                        .is_some()
                })
                .count();
            let mut sorted: Vec<Value> = all;
            sorted.sort_by(|a, b| {
                view::max_tier(b)
                    .cmp(&view::max_tier(a))
                    .then(
                        b.get("troopLevels")
                            .or_else(|| b.get("troop_levels"))
                            .is_some()
                            .cmp(
                                &a.get("troopLevels")
                                    .or_else(|| a.get("troop_levels"))
                                    .is_some(),
                            ),
                    )
                    .then(
                        view::pick_i64(b, &["score", "power"])
                            .unwrap_or(0)
                            .cmp(&view::pick_i64(a, &["score", "power"]).unwrap_or(0)),
                    )
            });
            let per = 15usize;
            let total_pages = sorted.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages - 1);
            let items: Vec<&Value> = sorted.iter().skip(page * per).take(per).collect();

            v2 = v2.title(format!("{state_label} — Troop Level Rankings"));
            v2 = v2.text(format!(
                "Players ranked by highest troop tier (T12 → T1). {with_data}/{} players with troop data.",
                sorted.len()
            ));
            let lines: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let tl = view::troop_bits(row);
                    let tl_bits = if tl.is_empty() {
                        "No troop data".to_string()
                    } else {
                        tl.join("  ·  ")
                    };
                    let tag = view::pick_str(row, &["alliance_abbr", "abbr", "tag"]);
                    let base = view::pick_str(row, &["nick_name", "name"]).unwrap_or("Unknown");
                    let name = match tag {
                        Some(tag) => format!("**[{tag}] {base}**"),
                        None => format!("**{base}**"),
                    };
                    let score = view::pick_i64(row, &["score", "power"])
                        .map(|n| v2::fmt(Some(n)))
                        .unwrap_or_else(|| "—".into());
                    format!(
                        "{} {name} · {score}\n   {tl_bits}",
                        v2::rank_chip((page * per + i + 1) as i64)
                    )
                })
                .collect();
            v2 = v2.text(if lines.is_empty() {
                "No players found.".to_string()
            } else {
                lines.join("\n")
            });
            v2 = v2.pager("lookup:state:tl:pg", page, total_pages);
            v2 = v2.buttons(&[
                ButtonDef {
                    id: "lookup:state:tl:refresh".into(),
                    label: "Refresh troop levels".into(),
                    emoji: Some("🔄".into()),
                    ..Default::default()
                },
                ButtonDef {
                    id: "lookup:state:tl:back".into(),
                    label: "← Back to overview".into(),
                    ..Default::default()
                },
            ]);
            return (v2.footer("troop levels as of", sv.as_of_ms), images);
        }
        "alliances" => {
            let list = list_key(
                raw,
                &["alliances", "alliance_list", "tags", "alliances_list"],
            );
            v2 = v2.title(format!("{state_label} — Alliances"));
            let per = 15usize;
            let total_pages = list.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages - 1);
            let items: Vec<&&Value> = list.iter().skip(page * per).take(per).collect();
            let lines: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let tag =
                        view::pick_str(row, &["abbr", "tag", "alliance_abbr"]).unwrap_or("???");
                    let name = view::pick_str(row, &["name", "alliance_name"]).unwrap_or("");
                    let power = view::pick_i64(row, &["power", "total_power"])
                        .map(|n| v2::fmt(Some(n)))
                        .unwrap_or_else(|| "—".into());
                    let members = view::pick_i64(row, &["members", "member_count"])
                        .map(|m| format!("{m} members"))
                        .unwrap_or_else(|| "— members".into());
                    let kills = view::pick_i64(row, &["kills", "total_kills"])
                        .map(|n| v2::fmt(Some(n)))
                        .unwrap_or_else(|| "—".into());
                    format!(
                        "{} **[{tag}] {name}** · {power} · {members} · {kills} kills",
                        v2::rank_chip((page * per + i + 1) as i64)
                    )
                })
                .collect();
            v2 = v2.text(if lines.is_empty() {
                "No alliances tracked.".to_string()
            } else {
                lines.join("\n")
            });
            v2 = v2.pager("lookup:state:pg", page, total_pages);

            let opts: Vec<SelectOpt> = items
                .iter()
                .filter_map(|row| {
                    let tag = view::pick_str(row, &["abbr", "tag", "alliance_abbr"])?;
                    let aid = view::pick_i64(row, &["aid"]).map(|n| n.to_string());
                    let value = aid.unwrap_or_else(|| tag.to_string());
                    let name = view::pick_str(row, &["name", "alliance_name"]).unwrap_or("");
                    let label = if name.is_empty() || name == tag {
                        format!("[{tag}]")
                    } else {
                        format!("[{tag}] {name}")
                    };
                    let desc = view::pick_i64(row, &["power", "total_power"])
                        .map(|n| v2::fmt(Some(n)))
                        .or_else(|| {
                            view::pick_i64(row, &["members", "member_count"])
                                .map(|m| format!("{m} members"))
                        });
                    Some(SelectOpt {
                        label: label.chars().take(100).collect(),
                        value: value.chars().take(100).collect(),
                        description: desc,
                        is_default: false,
                    })
                })
                .take(25)
                .collect();
            if !opts.is_empty() {
                v2 = v2.select("lookup:state:alliance", "Open an alliance", &opts);
            }
            return (v2.footer("data as of", sv.as_of_ms), images);
        }
        "map" => {
            v2 = v2.title(format!("{state_label} — Map"));
            let summary = raw.get("mapSummary");
            if images.is_empty() {
                v2 = v2.text("Map data is not available for this state right now.");
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
                if let Some(summary) = summary {
                    let mut bits = Vec::new();
                    if let Some(cities) = summary.get("cities").and_then(Value::as_i64) {
                        bits.push(format!("**{cities}** cities"));
                    }
                    if let Some(alliances) = summary.get("alliances").and_then(Value::as_i64) {
                        bits.push(format!("**{alliances}** alliances"));
                    }
                    if let Some(ago) = summary.get("ago").and_then(Value::as_str) {
                        bits.push(format!("locations {ago}"));
                    }
                    if !bits.is_empty() {
                        v2 = v2.text(bits.join(" · "));
                    }
                }
            }
            if let Some(notice) = &session.notice {
                v2 = v2.text(notice.clone());
            }
            let mut buttons = vec![ButtonDef {
                id: "lookup:state:map:refresh".into(),
                label: "Refresh map".into(),
                emoji: Some("🔄".into()),
                ..Default::default()
            }];
            if let Some(kid) = state_id {
                buttons.push(ButtonDef {
                    id: String::new(),
                    label: "Open full map".into(),
                    emoji: Some("🗺️".into()),
                    url: Some(format!("https://whiteoutstats.com/map?kid={kid}")),
                    ..Default::default()
                });
            }
            buttons.push(ButtonDef {
                id: "lookup:state:map:back".into(),
                label: "← Back to overview".into(),
                ..Default::default()
            });
            v2 = v2.buttons(&buttons);
            let as_of = raw
                .get("mapSummary")
                .and_then(|s| s.get("at"))
                .and_then(Value::as_i64)
                .or(sv.as_of_ms);
            return (v2.footer("map as of", as_of), images);
        }
        "prep" => {
            v2 = v2.title(format!("{state_label} - Prep points"));
            let rows: Vec<SvsRow> = view::svs_rows(raw)
                .into_iter()
                .filter(|r| r.prep_score.is_some() || r.days > 0)
                .take(6)
                .collect();
            if rows.is_empty() {
                v2 = v2.text("Prep scores are not published for this state yet.");
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
                let latest = &rows[0];
                let opponent = latest
                    .opponent_kid
                    .map(|k| format!("State {k}"))
                    .unwrap_or_else(|| "Unknown opponent".into());
                let verdict = latest
                    .prep
                    .as_deref()
                    .or(latest.result.as_deref())
                    .unwrap_or("");
                let mut line = format!("**Latest:** {opponent}");
                if let Some(season) = &latest.season {
                    line.push_str(&format!(" · S{season}"));
                }
                if !verdict.is_empty() {
                    line.push_str(&format!(" · prep {verdict}"));
                }
                line.push_str(&format!(
                    " · {} vs {}",
                    latest
                        .prep_score
                        .map(|n| v2::fmt(Some(n)))
                        .unwrap_or_else(|| "—".into()),
                    latest
                        .prep_opp_score
                        .map(|n| v2::fmt(Some(n)))
                        .unwrap_or_else(|| "—".into())
                ));
                v2 = v2.text(line);
                if rows.len() > 1 {
                    let extra: Vec<String> = rows[1..]
                        .iter()
                        .map(|r| {
                            let opp = r
                                .opponent_kid
                                .map(|k| format!("State {k}"))
                                .unwrap_or_else(|| "Unknown opponent".into());
                            let v = r.prep.as_deref().or(r.result.as_deref()).unwrap_or("");
                            let mut s = format!("- **{opp}**");
                            if let Some(season) = &r.season {
                                s.push_str(&format!(" · S{season}"));
                            }
                            if !v.is_empty() {
                                s.push_str(&format!(" · {v}"));
                            }
                            s.push_str(&format!(
                                " · {} vs {}",
                                r.prep_score
                                    .map(|n| v2::fmt(Some(n)))
                                    .unwrap_or_else(|| "—".into()),
                                r.prep_opp_score
                                    .map(|n| v2::fmt(Some(n)))
                                    .unwrap_or_else(|| "—".into())
                            ));
                            s
                        })
                        .collect();
                    v2 = v2.text(extra.join("\n"));
                }
            }
            v2 = v2.buttons(&[ButtonDef {
                id: "lookup:state:svs:back".into(),
                label: "← Back to State vs State".into(),
                ..Default::default()
            }]);
            return (v2.footer("prep scores as of", sv.as_of_ms), images);
        }
        "appointments" => {
            v2 = v2.title(format!("{state_label} — Appointments"));
            if images.is_empty() {
                let ministers = list_key(raw, &["ministers", "appointments"]);
                let offenders = list_key(raw, &["offenders"]);
                if ministers.is_empty() && offenders.is_empty() {
                    v2 = v2.text("No appointment data available for this state yet.");
                } else {
                    let mut lines = Vec::new();
                    if !ministers.is_empty() {
                        lines.push("**Ministers**".to_string());
                        for m in ministers.iter().take(10) {
                            let role =
                                view::pick_str(m, &["role", "title", "position"]).unwrap_or("—");
                            let name = view::pick_str(m, &["name", "nick_name", "fid"])
                                .unwrap_or("vacant");
                            lines.push(format!("• {role}: **{name}**"));
                        }
                    }
                    if !offenders.is_empty() {
                        lines.push("**Offenders**".to_string());
                        for m in offenders.iter().take(8) {
                            let role = view::pick_str(m, &["role", "title"]).unwrap_or("—");
                            let name =
                                view::pick_str(m, &["name", "nick_name", "fid"]).unwrap_or("—");
                            lines.push(format!("• {role}: **{name}**"));
                        }
                    }
                    v2 = v2.text(lines.join("\n"));
                }
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
                let ministers = list_key(raw, &["ministers", "appointments"]);
                let offenders = list_key(raw, &["offenders"]);
                let fm = ministers
                    .iter()
                    .filter(|m| {
                        !m.get("vacant").and_then(Value::as_bool).unwrap_or(false)
                            && (m.get("nick_name").is_some()
                                || m.get("uid").is_some()
                                || m.get("name").is_some())
                    })
                    .count();
                let fo = offenders
                    .iter()
                    .filter(|m| {
                        !m.get("vacant").and_then(Value::as_bool).unwrap_or(false)
                            && (m.get("nick_name").is_some()
                                || m.get("uid").is_some()
                                || m.get("name").is_some())
                    })
                    .count();
                v2 = v2.text(format!(
                    "**Ministers** · {fm}/{}  ·  **Offenders** · {fo}",
                    ministers.len().max(6)
                ));
            }
            return (v2.footer("data as of", sv.as_of_ms), images);
        }
        "history" => {
            v2 = v2.title(format!("{state_label} - Matchup history"));
            let rows = view::svs_rows(raw);
            if !rows.is_empty() {
                if images.is_empty() {
                    v2 = v2
                        .text("Could not render matchup history right now. Try again in a moment.");
                } else {
                    for (image, _) in images.iter() {
                        v2 = v2.image(image);
                    }
                }
            } else {
                v2 = v2.text("No matchup history recorded yet.");
            }
            v2 = v2.buttons(&[ButtonDef {
                id: "lookup:state:svs:back".into(),
                label: "← Back to State vs State".into(),
                ..Default::default()
            }]);
            return (v2.footer("data as of", sv.as_of_ms), images);
        }
        "battle" => {
            v2 = v2.title(format!("{state_label} · Sunfire Castle battle"));
            let text = session
                .notice
                .clone()
                .or_else(|| view::pick_str(raw, &["battle_text", "battleText"]).map(str::to_string))
                .unwrap_or_else(|| {
                    "No Sunfire Castle battle on record for this state yet.".to_string()
                });
            v2 = v2.text(text);
            v2 = v2.buttons(&[ButtonDef {
                id: "lookup:state:svs:back".into(),
                label: "← Back to State vs State".into(),
                ..Default::default()
            }]);
            return (v2.footer_raw("retrieved just now"), images);
        }
        "note" => {
            v2 = v2.title(&state_label);
            let note = session
                .notice
                .clone()
                .unwrap_or_else(|| match &sv.president {
                    Some(name) => {
                        let sub = sv
                            .president_sub
                            .clone()
                            .unwrap_or_else(|| "President".to_string());
                        format!("**{name}** — {sub}")
                    }
                    None => "Nothing recorded yet.".to_string(),
                });
            let from_prep = note.to_lowercase().contains("prep");
            v2 = v2.text(note);
            v2 = v2.buttons(&[ButtonDef {
                id: if from_prep {
                    "lookup:state:svs:back".into()
                } else {
                    "lookup:state:overview:back".into()
                },
                label: if from_prep {
                    "← Back to State vs State".into()
                } else {
                    "← Back to overview".into()
                },
                ..Default::default()
            }]);
            return (v2.footer("data as of", sv.as_of_ms), images);
        }
        _ => {
            v2 = v2.title(&state_label);
            v2 = v2.text("Unknown tab.");
        }
    }

    (v2.footer("data as of", sv.as_of_ms), images)
}

pub fn edit_state_v2(session: &LookupSession, tab: &str, images: Images) -> V2Edit {
    let (v2, images) = build(session, tab, images);
    let files = images
        .into_iter()
        .map(|(name, bytes)| v2::attachment(bytes, &name))
        .collect();
    v2::payload(v2, files, true)
}
