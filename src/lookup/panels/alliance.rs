//! Alliance Lookup — Components V2 panel (matches JS `alliancePanel.js`).
//!
//! Layout target (same as JS):
//!   ┃ [ tab buttons — active tab highlighted ]  (rows of 4)
//!   ┃ ───────────────
//!   ┃ ## Title
//!   ┃ body / rendered image card
//!   ┃ [ selects / pager / buttons ]
//!   ┃ ───────────────
//!   ┃ -# OmniCore · <label> <t:…:R>
//!
//! Tabs: Overview · Members · Furnace · Troop Level · Notice (+ Map sub-view)

use crate::ui::v2::V2Edit;
use serde_json::Value;

use crate::lookup::sessions::LookupSession;
use crate::lookup::view;
use crate::ui::v2::{self, ButtonDef, SelectOpt, Tab, V2};

pub const ALLIANCE_TABS: &[(&str, &str)] = &[
    ("overview", "Overview"),
    ("members", "Members"),
    ("furnace", "Furnace"),
    ("troopLevel", "Troop Level"),
    ("notice", "Notice"),
];

const SORTS: &[(&str, &str)] = &[
    ("power", "Power"),
    ("labyrinth", "The Labyrinth score"),
    ("furnace", "Furnace"),
    ("role", "Role"),
    ("name", "Name"),
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

type Images = Vec<(String, Vec<u8>)>;

/// One normalised roster member (JS `loadAlliance().roster`).
#[derive(Debug, Clone)]
struct Member {
    name: String,
    fid: Option<String>,
    power: Option<i64>,
    furnace: Option<i64>,
    role: Option<String>,
    role_num: i64,
    labyrinth: i64,
    raw: Value,
}

fn role_label(v: Option<i64>) -> Option<String> {
    let v = v?;
    Some(match v {
        5 => "R5".into(),
        4 => "R4".into(),
        3 => "R3".into(),
        2 => "R2".into(),
        1 => "R1".into(),
        n => format!("R{n}"),
    })
}

fn roster_members(raw: &Value) -> Vec<Member> {
    let a = raw
        .get("alliance")
        .or_else(|| raw.get("info"))
        .unwrap_or(raw);
    let list = ["roster", "members", "players", "members_list"]
        .iter()
        .find_map(|k| raw.get(*k).or_else(|| a.get(*k)).and_then(Value::as_array));
    list.map(|v| v.as_slice())
        .unwrap_or(&[])
        .iter()
        .map(|m| {
            let rank = view::pick_i64(m, &["alliance_rank", "rank_num", "rank"]);
            Member {
                name: view::pick_str(m, &["nick_name", "name", "nick"])
                    .unwrap_or("Unknown")
                    .to_string(),
                fid: view::pick_str(m, &["fid", "chief_id", "id"])
                    .map(str::to_string)
                    .or_else(|| view::pick_i64(m, &["fid", "chief_id"]).map(|n| n.to_string())),
                power: view::pick_i64(m, &["power"]),
                furnace: view::pick_i64(m, &["stove_lv", "furnace", "furnace_level"]),
                role: view::pick_str(m, &["rank_label", "alliance_rank", "role", "rank"])
                    .map(str::to_string)
                    .or_else(|| role_label(rank)),
                role_num: rank.unwrap_or(0),
                labyrinth: view::pick_i64(m, &["labyrinth", "labyrinth_score"]).unwrap_or(0),
                raw: m.clone(),
            }
        })
        .collect()
}

/// JS `sorted(roster, mode)`.
fn sort_members(list: &mut [Member], mode: &str) {
    match mode {
        "labyrinth" => list.sort_by_key(|b| std::cmp::Reverse(b.labyrinth)),
        "furnace" => list.sort_by_key(|b| std::cmp::Reverse(b.furnace.unwrap_or(0))),
        "role" => list.sort_by_key(|b| {
            (
                std::cmp::Reverse(b.role_num),
                std::cmp::Reverse(b.power.unwrap_or(0)),
            )
        }),
        "name" => list.sort_by_key(|a| a.name.to_lowercase()),
        _ => list.sort_by_key(|b| std::cmp::Reverse(b.power.unwrap_or(0))),
    }
}

/// JS `sortTroopLevelRoster`.
fn sort_troop_level(list: &mut [Member]) {
    list.sort_by(|a, b| {
        let at = view::max_tier(&a.raw);
        let bt = view::max_tier(&b.raw);
        bt.cmp(&at)
            .then(
                b.raw
                    .get("troopLevels")
                    .or_else(|| b.raw.get("troop_levels"))
                    .is_some()
                    .cmp(
                        &a.raw
                            .get("troopLevels")
                            .or_else(|| a.raw.get("troop_levels"))
                            .is_some(),
                    ),
            )
            .then(b.power.unwrap_or(0).cmp(&a.power.unwrap_or(0)))
    })
}

/// `[TAG] Name` title (JS `nameOf`).
fn title_of(session: &LookupSession, view: &view::AllianceView) -> String {
    let tag = session
        .alliance
        .as_deref()
        .filter(|t| !t.is_empty())
        .unwrap_or(&view.tag);
    let tag = if tag.is_empty() { "???" } else { tag };
    let raw_name = if session.name.trim().is_empty() {
        view.name.as_str()
    } else {
        session.name.as_str()
    };
    // Strip a pre-existing "[TAG] " prefix so we never double it.
    let name = raw_name
        .strip_prefix(&format!("[{tag}]"))
        .map(|rest| rest.trim_start().to_string())
        .unwrap_or_else(|| {
            if raw_name.starts_with('[') && raw_name.contains(']') {
                raw_name
                    .split_once(']')
                    .map(|(_, r)| r.trim().to_string())
                    .unwrap_or_else(|| raw_name.to_string())
            } else if raw_name.trim().is_empty() || raw_name == tag {
                view.name.clone()
            } else {
                raw_name.to_string()
            }
        });
    if name.is_empty() {
        format!("[{tag}]")
    } else {
        format!("[{tag}] {name}")
    }
}

/// Member select (`lookup:alliance:player`) — JS `playerSelect`.
fn player_select(list: &[Member]) -> Vec<SelectOpt> {
    list.iter()
        .filter(|m| m.fid.is_some())
        .take(25)
        .map(|m| {
            let mut desc = Vec::new();
            if let Some(p) = m.power {
                desc.push(v2::fmt(Some(p)));
            }
            if let Some(f) = m.furnace {
                desc.push(format!("F{f}"));
            }
            if let Some(r) = &m.role {
                desc.push(r.clone());
            }
            SelectOpt {
                label: m.name.chars().take(100).collect(),
                value: m
                    .fid
                    .clone()
                    .unwrap_or_default()
                    .chars()
                    .take(100)
                    .collect(),
                description: if desc.is_empty() {
                    None
                } else {
                    Some(desc.join(" · "))
                },
                is_default: false,
            }
        })
        .collect()
}

fn member_line(rank: i64, m: &Member, with_troops: bool) -> String {
    let mut bits = vec![format!("{} **{}**", v2::rank_chip(rank), m.name)];
    if let Some(p) = m.power {
        bits.push(v2::fmt(Some(p)));
    }
    if let Some(f) = m.furnace {
        bits.push(format!("F{f}"));
    }
    if let Some(r) = &m.role {
        bits.push(r.clone());
    }
    if with_troops {
        let tl = view::troop_bits(&m.raw);
        if !tl.is_empty() {
            bits.push(format!("[{}]", tl.join(" / ")));
        }
    }
    bits.join(" · ")
}

fn build(session: &LookupSession, tab: &str, images: Images) -> (V2, Images) {
    let tab = valid_tab(tab);
    let tabs = tabs_as_v2();
    let raw = &session.raw;
    let av = view::alliance_view(raw);
    // JS highlights nothing when the sub-view (map) is active.
    let active = if ALLIANCE_TABS.iter().any(|(k, _)| *k == tab) {
        tab
    } else {
        "overview"
    };
    let mut v2 = V2::new()
        .tab_buttons("lookup:alliance:tab", &tabs, active, 4)
        .sep(true);
    let title = title_of(session, &av);
    let state_id = session.state.or(av.state_id);
    let roster = roster_members(raw);

    match tab {
        "overview" => {
            v2 = v2.title(&title);
            for (image, _) in images.iter() {
                v2 = v2.image(image);
            }
            if images.is_empty() {
                let state_txt = state_id
                    .map(|s| format!("S{s}"))
                    .unwrap_or_else(|| "—".into());
                let tag = if av.tag.is_empty() { "—" } else { &av.tag };
                let mut lines = vec![format!("**Tag** `[{tag}]` · **State** `{state_txt}`")];
                if let Some(p) = av.power {
                    lines.push(format!("**Power** `{}`", v2::fmt(Some(p))));
                }
                if let Some(k) = av.kills {
                    lines.push(format!("**Kills** `{}`", v2::fmt(Some(k))));
                }
                if let Some(n) = av
                    .members
                    .or_else(|| (!roster.is_empty()).then_some(roster.len() as i64))
                {
                    lines.push(format!("**Members** `{n}`"));
                }
                if let Some(leader) = &av.leader {
                    lines.push(format!("**Leader** `{leader}`"));
                }
                v2 = v2.text(lines.join("\n"));
            }

            // JS: player select over the top roster, then Open map + state buttons.
            let mut top = roster.clone();
            sort_members(&mut top, "power");
            top.truncate(6);
            let opts = player_select(&top);
            if !opts.is_empty() {
                v2 = v2.select("lookup:alliance:player", "Open a player…", &opts);
            }
            let mut buttons = vec![ButtonDef {
                id: "lookup:alliance:go:map".into(),
                label: "Open map".into(),
                emoji: Some("🗺️".into()),
                ..Default::default()
            }];
            if let Some(state) = state_id {
                buttons.push(ButtonDef {
                    id: "lookup:alliance:go:state".into(),
                    label: format!("S{state}"),
                    ..Default::default()
                });
            }
            v2 = v2.buttons(&buttons);
        }
        "map" => {
            v2 = v2.title(format!("{title} — Hive map"));
            let summary = raw.get("mapSummary");
            if images.is_empty() {
                v2 = v2.text("Map data is not available for this state right now.");
            } else {
                for (image, _) in images.iter() {
                    v2 = v2.image(image);
                }
                if let Some(summary) = summary {
                    let mine = summary.get("mine").and_then(Value::as_i64).unwrap_or(0);
                    let mut line = format!(
                        "**{mine}** cities on the map for this alliance · State {}",
                        state_id
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "—".into())
                    );
                    if let Some(ago) = summary.get("ago").and_then(Value::as_str) {
                        line.push_str(&format!(" · locations {ago}"));
                    }
                    v2 = v2.text(line);
                }
            }
            if let Some(notice) = &session.notice {
                v2 = v2.text(notice.clone());
            }
            let mut buttons = vec![
                ButtonDef {
                    id: "lookup:alliance:map:back".into(),
                    label: "Back to overview".into(),
                    emoji: Some("↩️".into()),
                    ..Default::default()
                },
                ButtonDef {
                    id: "lookup:alliance:map:refresh".into(),
                    label: "Refresh map".into(),
                    emoji: Some("🔄".into()),
                    ..Default::default()
                },
            ];
            if let Some(kid) = state_id {
                buttons.push(ButtonDef {
                    id: String::new(),
                    label: "Open full map".into(),
                    emoji: Some("🗺️".into()),
                    url: Some(format!("https://whiteoutstats.com/map?kid={kid}")),
                    ..Default::default()
                });
            }
            v2 = v2.buttons(&buttons);
            let as_of = raw
                .get("mapSummary")
                .and_then(|s| s.get("at"))
                .and_then(Value::as_i64)
                .or(av.as_of_ms);
            return (v2.footer("map as of", as_of), images);
        }
        "members" => {
            let sort = session.board.as_deref().unwrap_or("power");
            let mut list = roster.clone();
            sort_members(&mut list, sort);
            let per = 15usize;
            let total_pages = list.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages - 1);
            let items: Vec<&Member> = list.iter().skip(page * per).take(per).collect();

            v2 = v2.title(format!("{title} — Members"));
            let lines: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(i, m)| member_line((page * per + i + 1) as i64, m, true))
                .collect();
            v2 = v2.text(if lines.is_empty() {
                "No members listed.".to_string()
            } else {
                lines.join("\n")
            });

            let sort_opts: Vec<SelectOpt> = SORTS
                .iter()
                .map(|(v, l)| SelectOpt {
                    label: l.to_string(),
                    value: v.to_string(),
                    description: None,
                    is_default: *v == sort,
                })
                .collect();
            v2 = v2.select("lookup:alliance:sort", "Sort members…", &sort_opts);

            let opts = player_select(&items.iter().map(|m| (*m).clone()).collect::<Vec<_>>());
            if !opts.is_empty() {
                v2 = v2.select("lookup:alliance:player", "Open a player…", &opts);
            }
            v2 = v2.pager("lookup:alliance:pg", page, total_pages);
            return (
                v2.footer_raw(format!(
                    "Page {} / {} · {} members",
                    page + 1,
                    total_pages,
                    list.len()
                )),
                images,
            );
        }
        "troopLevel" => {
            let mut list = roster.clone();
            sort_troop_level(&mut list);
            let with_data = list
                .iter()
                .filter(|m| {
                    m.raw
                        .get("troopLevels")
                        .or_else(|| m.raw.get("troop_levels"))
                        .is_some()
                })
                .count();
            let total = list.len();
            let per = 15usize;
            let total_pages = list.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages - 1);
            let items: Vec<&Member> = list.iter().skip(page * per).take(per).collect();

            v2 = v2.title(format!("{title} — Troop Level"));
            v2 = v2.text(format!(
                "Members ranked by highest troop tier (T12 → T1). {with_data}/{total} members with troop data."
            ));
            let lines: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let rank = (page * per + i + 1) as i64;
                    let tl = view::troop_bits(&m.raw);
                    let tl_bits = if tl.is_empty() {
                        "No troop data".to_string()
                    } else {
                        tl.join("  ·  ")
                    };
                    let mut head = vec![format!("{} **{}**", v2::rank_chip(rank), m.name)];
                    if let Some(p) = m.power {
                        head.push(v2::fmt(Some(p)));
                    }
                    format!("{}\n   {tl_bits}", head.join(" · "))
                })
                .collect();
            v2 = v2.text(if lines.is_empty() {
                "No members found.".to_string()
            } else {
                lines.join("\n")
            });
            v2 = v2.pager("lookup:alliance:tl:pg", page, total_pages);
            v2 = v2.buttons(&[
                ButtonDef {
                    id: "lookup:alliance:tl:refresh".into(),
                    label: "Refresh troop levels".into(),
                    emoji: Some("🔄".into()),
                    ..Default::default()
                },
                ButtonDef {
                    id: "lookup:alliance:tl:back".into(),
                    label: "← Back to overview".into(),
                    emoji: Some("↩️".into()),
                    ..Default::default()
                },
            ]);
            return (
                v2.footer_raw(format!(
                    "Page {} / {} · {with_data}/{total} with troop data",
                    page + 1,
                    total_pages
                )),
                images,
            );
        }
        "furnace" => {
            let mut list: Vec<Member> =
                roster.into_iter().filter(|m| m.furnace.is_some()).collect();
            sort_members(&mut list, "furnace");
            let per = 15usize;
            let total_pages = list.len().div_ceil(per).max(1);
            let page = session.page.min(total_pages - 1);
            let items: Vec<&Member> = list.iter().skip(page * per).take(per).collect();

            v2 = v2.title(format!("{title} — Furnace levels"));
            let lines: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let mut bits = vec![
                        format!(
                            "{} **{}**",
                            v2::rank_chip((page * per + i + 1) as i64),
                            m.name
                        ),
                        format!("F{}", m.furnace.unwrap_or(0)),
                    ];
                    if let Some(p) = m.power {
                        bits.push(v2::fmt(Some(p)));
                    }
                    bits.join(" · ")
                })
                .collect();
            v2 = v2.text(if lines.is_empty() {
                "No furnace data.".to_string()
            } else {
                lines.join("\n")
            });
            v2 = v2.pager("lookup:alliance:pg", page, total_pages);
            return (
                v2.footer_raw(format!("Page {} / {}", page + 1, total_pages)),
                images,
            );
        }
        "notice" => {
            v2 = v2.title(format!("{title} — Notice"));
            let notice = av
                .notice
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("");
            let manifesto = av
                .manifesto
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("");
            if notice.is_empty() && manifesto.is_empty() {
                v2 = v2.text("This alliance has no notice or manifesto posted.");
            } else {
                if !notice.is_empty() {
                    let text: String = notice.chars().take(1800).collect();
                    v2 = v2.text(text);
                }
                if !manifesto.is_empty() {
                    let text: String = manifesto.chars().take(1200).collect();
                    let body = if notice.is_empty() {
                        text
                    } else {
                        format!("\n\n**Manifesto**\n{text}")
                    };
                    v2 = v2.text(body);
                }
            }
        }
        _ => {
            v2 = v2.title(&title);
            v2 = v2.text("Unknown tab.");
        }
    }

    let footer = match tab {
        "overview" => "roster as of",
        "notice" => "notice as of",
        _ => "Lookup",
    };
    (v2.footer(footer, av.as_of_ms), images)
}

pub fn edit_alliance_v2(session: &LookupSession, tab: &str, images: Images) -> V2Edit {
    let (v2, images) = build(session, tab, images);
    let files = images
        .into_iter()
        .map(|(name, bytes)| v2::attachment(bytes, &name))
        .collect();
    v2::payload(v2, files, true)
}
