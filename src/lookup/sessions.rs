//! Private, per-user Lookup sessions with tabs and sub-views.
//!
//! Mirrors the JavaScript lookup panels: a search opens a private ephemeral
//! message whose tab selector re-renders in place. Sessions expire after 15
//! minutes and are scoped to both the guild and the clicking user.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use poise::serenity_prelude as serenity;
use serde_json::Value;
use serenity::{
    CreateActionRow, CreateButton, CreateEmbed, CreateSelectMenu, CreateSelectMenuKind,
};

use crate::ui::{embeds, theme};

const TTL: Duration = Duration::from_secs(15 * 60);
const MAX_SESSIONS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Player,
    Alliance,
    State,
}

#[derive(Debug, Clone)]
pub struct LookupSession {
    pub kind: Kind,
    pub raw: Value,
    pub name: String,
    pub state: Option<i64>,
    pub fid: Option<String>,
    pub alliance: Option<String>,
    pub tab: String,
    pub notice: Option<String>,
    /// Search results awaiting an explicit selection (kept per user/guild).
    pub choices: Vec<Value>,
    /// Current board and page for the Rankings view.
    pub board: Option<String>,
    pub page: usize,
    /// Map refresh request token returned by the public queue.
    pub map_token: Option<String>,
    pub created: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    user: u64,
    guild: u64,
}

fn store() -> &'static Mutex<HashMap<Key, LookupSession>> {
    static STORE: OnceLock<Mutex<HashMap<Key, LookupSession>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set(user: u64, guild: u64, session: LookupSession) {
    if let Ok(mut map) = store().lock() {
        map.retain(|_, value| value.created.elapsed() < TTL);
        map.insert(Key { user, guild }, session);
        while map.len() > MAX_SESSIONS {
            let oldest = map
                .iter()
                .min_by_key(|(_, value)| value.created)
                .map(|(key, _)| (key.user, key.guild));
            match oldest {
                Some((user, guild)) => {
                    map.remove(&Key { user, guild });
                }
                None => break,
            }
        }
    }
}

pub fn get(user: u64, guild: u64) -> Option<LookupSession> {
    let key = Key { user, guild };
    let mut map = store().lock().ok()?;
    let expired = map
        .get(&key)
        .is_some_and(|session| session.created.elapsed() >= TTL);
    if expired {
        map.remove(&key);
        return None;
    }
    map.get(&key).cloned()
}

pub fn player_tabs() -> &'static [(&'static str, &'static str)] {
    &[
        ("overview", "Overview"),
        ("standings", "Standings"),
        ("heroes", "Heroes & Gear"),
        ("progression", "Progression"),
        ("crowns", "Crowns"),
    ]
}

pub fn alliance_tabs() -> &'static [(&'static str, &'static str)] {
    &[
        ("overview", "Overview"),
        ("members", "Members"),
        ("furnace", "Furnace"),
        ("troopLevel", "Troop Level"),
        ("notice", "Notice"),
    ]
}

pub fn state_tabs() -> &'static [(&'static str, &'static str)] {
    &[
        ("overview", "Overview"),
        ("svs", "State vs State"),
        ("boards", "Rankings"),
        ("alliances", "Alliances"),
        ("troopLevel", "Troop Level"),
        ("appointments", "Appointments"),
    ]
}

pub fn tabs(kind: Kind) -> &'static [(&'static str, &'static str)] {
    match kind {
        Kind::Player => player_tabs(),
        Kind::Alliance => alliance_tabs(),
        Kind::State => state_tabs(),
    }
}

fn tab_row(kind: Kind, active: &str) -> CreateActionRow {
    let prefix = match kind {
        Kind::Player => "lookup:player:tab",
        Kind::Alliance => "lookup:alliance:tab",
        Kind::State => "lookup:state:tab",
    };
    let active = tabs(kind)
        .iter()
        .find(|(key, _)| *key == active)
        .map(|(key, _)| *key)
        .unwrap_or("overview");
    let label = tabs(kind)
        .iter()
        .find(|(key, _)| *key == active)
        .map(|(_, label)| *label)
        .unwrap_or("Overview");
    let options = tabs(kind)
        .iter()
        .map(|(key, label)| {
            serenity::CreateSelectMenuOption::new(*label, *key).default_selection(*key == active)
        })
        .collect();
    CreateActionRow::SelectMenu(
        CreateSelectMenu::new(prefix, CreateSelectMenuKind::String { options }).placeholder(label),
    )
}

fn pick_str<'a>(raw: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| raw.get(*key))
        .and_then(Value::as_str)
}

fn pick_id(raw: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| raw.get(*key)).and_then(|value| {
        value
            .as_str()
            .map(str::to_owned)
            .or_else(|| value.as_i64().map(|n| n.to_string()))
    })
}

fn pick_i64(raw: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| raw.get(*key)).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_f64().map(|value| value as i64))
            .or_else(|| {
                value
                    .as_str()
                    .and_then(|text| text.replace(',', "").parse::<f64>().ok())
                    .map(|value| value as i64)
            })
    })
}

fn timestamp_label(raw: &Value, keys: &[&str]) -> Option<String> {
    let mut timestamp = pick_i64(raw, keys)?;
    if timestamp.unsigned_abs() > 10_000_000_000 {
        timestamp /= 1000;
    }
    chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0)
        .map(|date| date.format("%d %b %Y").to_string())
}

fn map_card_from_payload(
    payload: &Value,
    title: String,
    highlight: Option<(f64, f64)>,
    focus_alliance_id: Option<String>,
) -> crate::cards::MapCard {
    let cities = payload
        .get("cities")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .take(50_000)
                .filter_map(|city| {
                    Some(crate::cards::MapCity {
                        x: city
                            .get("x")?
                            .as_f64()
                            .or_else(|| city.get("x")?.as_i64().map(|v| v as f64))?,
                        y: city
                            .get("y")?
                            .as_f64()
                            .or_else(|| city.get("y")?.as_i64().map(|v| v as f64))?,
                        alliance_id: pick_id(city, &["aid", "alliance_id"]),
                        label: pick_str(city, &["nick_name", "name"]).map(str::to_owned),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let alliances = payload
        .get("alliances")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .take(500)
                .enumerate()
                .filter_map(|(color_index, alliance)| {
                    Some(crate::cards::MapAlliance {
                        aid: pick_id(alliance, &["aid", "alliance_id", "id"])?,
                        tag: pick_str(alliance, &["abbr", "tag", "alliance_abbr"])
                            .map(str::to_owned),
                        color_index,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    crate::cards::MapCard {
        title,
        subtitle: format!("{} cities · refreshed lookup", cities.len()),
        cities,
        alliances,
        highlight,
        focus_alliance_id,
        as_of: None,
    }
}

fn field_lines(
    raw: &Value,
    keys: &[(&str, &[&str])],
    suffix: Option<&str>,
) -> Vec<(String, String, bool)> {
    keys.iter()
        .map(|(label, paths)| {
            let value = pick_i64(raw, paths)
                .map(|value| match suffix {
                    Some(suffix) => format!("`{value}{suffix}`"),
                    None => format!("`{}`", theme::compact_number(value)),
                })
                .or_else(|| pick_str(raw, paths).map(|text| format!("`{text}`")))
                .unwrap_or_else(|| "`—`".to_string());
            ((*label).to_string(), value, true)
        })
        .collect()
}

fn action_button(
    id: &str,
    label: impl Into<String>,
    emoji: &str,
    style: serenity::ButtonStyle,
) -> CreateButton {
    CreateButton::new(id)
        .label(label)
        .style(style)
        .emoji(serenity::ReactionType::Unicode(emoji.to_string()))
}

/// Build the private message for a session tab: embed, controls and optional
/// image attachments. All payloads are private to the searcher.
pub async fn render(
    ctx: &serenity::Context,
    data: &crate::state::Data,
    session: &LookupSession,
    tab: &str,
) -> (
    CreateEmbed,
    Vec<CreateActionRow>,
    Vec<serenity::CreateAttachment>,
) {
    let raw = &session.raw;
    let mut embed = embeds::brand(session.name.clone())
        .color(match session.kind {
            Kind::Player => theme::colors::INFO,
            Kind::Alliance => theme::colors::VIOLET,
            Kind::State => theme::colors::TEAL,
        })
        .footer(serenity::CreateEmbedFooter::new(theme::footer()));
    let mut files = Vec::new();
    let mut rows = vec![tab_row(session.kind, tab)];

    match (session.kind, tab) {
        (Kind::Player, "overview") => {
            embed = embed
                .field(
                    "FID",
                    format!("`{}`", session.fid.as_deref().unwrap_or("—")),
                    true,
                )
                .field(
                    "State",
                    session
                        .state
                        .map(|state| format!("`{state}`"))
                        .unwrap_or_else(|| "`—`".into()),
                    true,
                )
                .field(
                    "Alliance",
                    session
                        .alliance
                        .as_deref()
                        .map(|tag| format!("`[{tag}]`"))
                        .unwrap_or_else(|| "`—`".into()),
                    true,
                );
            for (name, value, inline) in field_lines(
                raw,
                &[
                    ("Power", &["power", "total_power"]),
                    ("Kills", &["kills", "total_kills"]),
                    ("Furnace", &["furnace_level", "town_hall_level"]),
                ],
                None,
            ) {
                embed = embed.field(name, value, inline);
            }
            let mut buttons = Vec::new();
            if let Some(state) = session.state {
                buttons.push(action_button(
                    "lookup:player:go:state",
                    format!("S{state}"),
                    "🏰",
                    serenity::ButtonStyle::Secondary,
                ));
                buttons.push(action_button(
                    "lookup:player:go:map",
                    "Locate on map",
                    "🗺️",
                    serenity::ButtonStyle::Secondary,
                ));
            }
            if let Some(tag) = session.alliance.as_deref() {
                buttons.push(action_button(
                    "lookup:player:go:alliance",
                    format!("[{tag}] Alliance"),
                    "🤝",
                    serenity::ButtonStyle::Secondary,
                ));
            }
            buttons.push(action_button(
                "lookup:player:go:trail",
                "Trails",
                "🧭",
                serenity::ButtonStyle::Secondary,
            ));
            rows.push(CreateActionRow::Buttons(buttons));
        }
        (Kind::Player, "standings") => {
            let boards = raw
                .get("boards")
                .or_else(|| raw.get("leaderboards"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let lines: Vec<String> = boards
                .iter()
                .skip(session.page.saturating_mul(15))
                .take(15)
                .map(|board| {
                    let name = pick_str(board, &["name", "key"]).unwrap_or("Board");
                    let rank = pick_i64(board, &["rank"])
                        .map(|rank| format!("#{rank}"))
                        .unwrap_or_else(|| "—".into());
                    format!("• **{name}** — {rank}")
                })
                .collect();
            embed = embed.description(if lines.is_empty() {
                "_No leaderboard standings stored for this chief._".to_string()
            } else {
                lines.join("\n")
            });
            let options = boards
                .iter()
                .filter_map(|board| {
                    let key = pick_str(board, &["key", "name"])?;
                    let name = pick_str(board, &["name"]).unwrap_or(key);
                    Some(serenity::CreateSelectMenuOption::new(name, key))
                })
                .take(25)
                .collect::<Vec<_>>();
            if !options.is_empty() {
                rows.push(CreateActionRow::SelectMenu(
                    CreateSelectMenu::new(
                        "lookup:player:board:state",
                        CreateSelectMenuKind::String { options },
                    )
                    .placeholder("Open a state board…"),
                ));
            }
            let pages = boards.len().div_ceil(15).max(1);
            if pages > 1 {
                let last = pages.saturating_sub(1);
                let prev = session.page.saturating_sub(1);
                let next = (session.page + 1).min(last);
                rows.push(CreateActionRow::Buttons(vec![
                    action_button(
                        &format!("lookup:player:pg:p:{prev}:prev"),
                        "Previous",
                        "◀️",
                        serenity::ButtonStyle::Secondary,
                    )
                    .disabled(session.page == 0),
                    action_button(
                        &format!("lookup:player:pg:p:{next}:next"),
                        format!("Page {} / {pages}", session.page + 1),
                        "▶️",
                        serenity::ButtonStyle::Secondary,
                    )
                    .disabled(session.page + 1 >= pages),
                ]));
            }
        }
        (Kind::Player, "heroes") => {
            embed = embed.description("Heroes & gear from the stored log/loadout payload.");
            for (name, value, inline) in field_lines(
                raw,
                &[
                    ("VIP", &["vip", "vip_level"]),
                    ("TG level", &["tg_level", "level_label"]),
                    ("Hero power", &["hero_power", "heroes_power"]),
                ],
                None,
            ) {
                embed = embed.field(name, value, inline);
            }
            if let Some(uid) = pick_str(raw, &["uid", "oracle_uid"]) {
                if let Ok(loadout) = data
                    .lookup
                    .site_json(&format!("/players/{uid}/loadout"))
                    .await
                {
                    let cards =
                        crate::cards::render_loadout_cards(&loadout, &data.cfg.oracle_site).await;
                    if let Some(bytes) = cards.chief {
                        files.push(serenity::CreateAttachment::bytes(bytes, "chief-gear.png"));
                    }
                }
            }
        }
        (Kind::Player, "progression") => {
            let uid = pick_str(raw, &["uid", "oracle_uid"]).unwrap_or("");
            let rows = data.lookup.player_history(uid).await.unwrap_or_default();
            if rows.is_empty() {
                embed = embed.description("_Not enough snapshots yet for a progression chart._");
            } else {
                let points: Vec<crate::cards::SeriesPoint> = rows
                    .iter()
                    .rev()
                    .take(40)
                    .filter_map(|row| {
                        Some(crate::cards::SeriesPoint {
                            v: pick_i64(row, &["power", "total_power"])? as f64,
                            t: pick_i64(row, &["t", "ts", "date", "at"]),
                        })
                    })
                    .collect();
                let kill_points: Vec<crate::cards::SeriesPoint> = rows
                    .iter()
                    .rev()
                    .take(40)
                    .filter_map(|row| {
                        Some(crate::cards::SeriesPoint {
                            v: pick_i64(row, &["kills", "kill", "total_kills"])? as f64,
                            t: pick_i64(row, &["t", "ts", "date", "at"]),
                        })
                    })
                    .collect();
                let card = crate::cards::ProgressionCard {
                    name: session.name.clone(),
                    power_series: points,
                    kill_series: kill_points,
                    power_snaps: Some(rows.len() as i64),
                    kill_snaps: Some(
                        rows.iter()
                            .filter(|row| {
                                pick_i64(row, &["kills", "kill", "total_kills"]).is_some()
                            })
                            .count() as i64,
                    ),
                    ..Default::default()
                };
                if let Ok(bytes) = crate::cards::render_progression_card(&card) {
                    files.push(serenity::CreateAttachment::bytes(bytes, "progression.png"));
                }
            }
        }
        (Kind::Player, "crowns") => {
            let crown = raw.get("crowns").unwrap_or(raw);
            for (name, value, inline) in field_lines(
                crown,
                &[
                    ("Crowns", &["count", "crowns", "crown_level"]),
                    ("Terms", &["terms"]),
                    ("Conquests", &["conquests"]),
                    ("States ruled", &["states_ruled", "statesRuled"]),
                ],
                None,
            ) {
                embed = embed.field(name, value, inline);
            }
        }
        (Kind::Player, "map") => {
            let state = session.state.unwrap_or(0);
            let map = data.lookup.state_map(state).await.ok();
            let city = map
                .as_ref()
                .and_then(|map| map.get("cities"))
                .and_then(Value::as_array)
                .and_then(|cities| {
                    let uid = pick_id(raw, &["uid", "oracle_uid"]);
                    let fid = pick_id(raw, &["fid", "player_id"]);
                    let name =
                        pick_str(raw, &["nick_name", "name", "game_name"]).map(str::to_lowercase);
                    cities
                        .iter()
                        .find(|city| {
                            uid.as_ref()
                                .is_some_and(|id| pick_id(city, &["uid"]).as_ref() == Some(id))
                        })
                        .or_else(|| {
                            cities.iter().find(|city| {
                                fid.as_ref().is_some_and(|id| {
                                    pick_id(city, &["fid", "player_id"]).as_ref() == Some(id)
                                })
                            })
                        })
                        .or_else(|| {
                            cities.iter().find(|city| {
                                name.as_ref().is_some_and(|n| {
                                    pick_str(city, &["nick_name", "name"])
                                        .is_some_and(|v| v.eq_ignore_ascii_case(n))
                                })
                            })
                        })
                });
            let x = city
                .and_then(|c| pick_i64(c, &["x", "pos_x"]))
                .or_else(|| pick_i64(raw, &["x", "pos_x"]));
            let y = city
                .and_then(|c| pick_i64(c, &["y", "pos_y"]))
                .or_else(|| pick_i64(raw, &["y", "pos_y"]));
            match (x, y) {
                (Some(x), Some(y)) => {
                    embed = embed.description(format!(
                        "State **{state}** · X **{x}** · Y **{y}**\n{}",
                        session.notice.as_deref().unwrap_or("")
                    ));
                    let mut buttons = vec![action_button(
                        "lookup:player:tab:overview",
                        "← Back to overview",
                        "◀️",
                        serenity::ButtonStyle::Secondary,
                    )];
                    buttons.push(action_button(
                        "lookup:player:map:refresh",
                        "Refresh map",
                        "🔄",
                        serenity::ButtonStyle::Secondary,
                    ));
                    rows.push(CreateActionRow::Buttons(buttons));
                    if let Some(map) = map.as_ref() {
                        let cities: Vec<crate::cards::MapCity> = map
                            .get("cities")
                            .and_then(Value::as_array)
                            .map(|rows| {
                                rows.iter()
                                    .take(10000)
                                    .filter_map(|city| {
                                        Some(crate::cards::MapCity {
                                            x: city.get("x")?.as_f64().or_else(|| {
                                                city.get("x")?.as_i64().map(|v| v as f64)
                                            })?,
                                            y: city.get("y")?.as_f64().or_else(|| {
                                                city.get("y")?.as_i64().map(|v| v as f64)
                                            })?,
                                            alliance_id: pick_id(city, &["aid", "alliance_id"]),
                                            label: pick_str(city, &["nick_name", "name"])
                                                .map(str::to_owned),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if let Ok(bytes) = crate::cards::render_map_card(&crate::cards::MapCard {
                            title: format!("State {state} map"),
                            subtitle: format!("{} cities · refreshed lookup", cities.len()),
                            cities,
                            alliances: Vec::new(),
                            highlight: Some((x as f64, y as f64)),
                            focus_alliance_id: None,
                            as_of: None,
                        }) {
                            embed = embed.image("attachment://map.png");
                            files.push(serenity::CreateAttachment::bytes(bytes, "map.png"));
                        }
                    }
                }
                _ => {
                    embed = embed
                        .description("Coordinates are not available for this chief right now.");
                }
            }
        }
        (Kind::Player, "trail") => {
            let trails = raw
                .get("trails")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let lines: Vec<String> = trails
                .iter()
                .take(20)
                .map(|tower| {
                    let name = pick_str(tower, &["name", "label"]).unwrap_or("Tower");
                    let score = pick_i64(tower, &["score", "stage", "level_id"])
                        .map(|score| score.to_string())
                        .unwrap_or_else(|| "—".into());
                    format!("• **{name}** — {score}")
                })
                .collect();
            embed = embed.description(if lines.is_empty() {
                "_No exploration trail scores recorded for this chief yet._".to_string()
            } else {
                lines.join("\n")
            });
        }
        (Kind::Alliance, "overview") | (Kind::Alliance, "members") => {
            let roster = raw
                .get("roster")
                .or_else(|| raw.get("members"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            embed = embed.description(format!("**{} member(s) on record**", roster.len()));
            let lines: Vec<String> = roster
                .iter()
                .take(20)
                .map(|member| {
                    let name =
                        pick_str(member, &["name", "game_name", "nickname"]).unwrap_or("Chief");
                    let power = pick_i64(member, &["power", "total_power"])
                        .map(theme::compact_number)
                        .unwrap_or_else(|| "—".into());
                    let role = pick_str(member, &["rank_label", "role"]).unwrap_or("");
                    format!("• **{name}** — `{power}`{role}")
                })
                .collect();
            if !lines.is_empty() {
                embed = embed.field("Roster", lines.join("\n"), false);
            }
            if tab == "members" {
                rows.push(CreateActionRow::Buttons(vec![action_button(
                    "lookup:alliance:pg:p:0",
                    "Next page →",
                    "➡️",
                    serenity::ButtonStyle::Secondary,
                )]));
            }
        }
        (Kind::Alliance, "furnace") => {
            let roster = raw
                .get("roster")
                .or_else(|| raw.get("members"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut levels: Vec<(String, i64)> = roster
                .iter()
                .filter_map(|member| {
                    Some((
                        pick_str(member, &["name", "game_name"])?.to_string(),
                        pick_i64(member, &["furnace_level", "town_hall_level", "furnace"])?,
                    ))
                })
                .collect();
            levels.sort_by_key(|(_, level)| std::cmp::Reverse(*level));
            embed = embed.description(if levels.is_empty() {
                "_No furnace levels on record._".to_string()
            } else {
                levels
                    .iter()
                    .take(20)
                    .map(|(name, level)| format!("• **{name}** — F{level}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        (Kind::Alliance, "troopLevel") => {
            use crate::lookup::troop_levels;
            let roster = raw
                .get("roster")
                .or_else(|| raw.get("members"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let lines: Vec<String> = roster
                .iter()
                .take(20)
                .map(|member| {
                    let name = pick_str(member, &["name", "game_name"]).unwrap_or("Chief");
                    format!("• **{name}** — {}", troop_levels::troop_summary(member))
                })
                .collect();
            embed = embed.description(if lines.is_empty() {
                "_No troop levels on record._".to_string()
            } else {
                lines.join("\n")
            });
        }
        (Kind::Alliance, "notice") => {
            embed = embed.description(
                pick_str(raw, &["notice", "announcement", "message"])
                    .map(|text| text.to_string())
                    .unwrap_or_else(|| "_No alliance notice on record._".to_string()),
            );
        }
        (Kind::State, "overview") => {
            for (name, value, inline) in field_lines(
                raw,
                &[
                    ("Alliances", &["alliances_count", "alliances"]),
                    ("Players", &["players_count", "players"]),
                    ("Power", &["power", "total_power"]),
                ],
                None,
            ) {
                embed = embed.field(name, value, inline);
            }
            let alliances = raw
                .get("alliances")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let lines: Vec<String> = alliances
                .iter()
                .take(12)
                .map(|alliance| {
                    let tag = pick_str(alliance, &["abbr", "tag"]).unwrap_or("—");
                    let power = pick_i64(alliance, &["power", "total_power"])
                        .map(theme::compact_number)
                        .unwrap_or_else(|| "—".into());
                    format!("• **[{tag}]** — `{power}`")
                })
                .collect();
            if !lines.is_empty() {
                embed = embed.field("Top alliances", lines.join("\n"), false);
            }
        }
        (Kind::State, "svs") => {
            let kvk = raw
                .get("throne")
                .and_then(|throne| throne.get("kvk"))
                .or_else(|| raw.get("kvk"));
            let battles = kvk
                .and_then(|value| value.get("svsHistory"))
                .and_then(|value| value.get("rows"))
                .or_else(|| kvk.and_then(|value| value.get("cycles")))
                .or_else(|| kvk.and_then(|value| value.get("matchups")))
                .or_else(|| kvk.and_then(|value| value.get("history")))
                .or_else(|| raw.get("svs"))
                .or_else(|| raw.get("matches"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            embed = embed.description(if battles.is_empty() {
                "_No State vs State results on record._".to_string()
            } else {
                battles
                    .iter()
                    .take(15)
                    .map(|battle| {
                        let opponent =
                            pick_str(battle, &["opponent_kid", "opponent", "enemy", "name"])
                                .unwrap_or("—");
                        let prep = pick_str(battle, &["prep"]).unwrap_or("—");
                        let castle = pick_str(battle, &["castle", "result"]).unwrap_or("—");
                        format!("• **{opponent}** — Prep: {prep} · Castle: {castle}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        (Kind::State, "boards") => {
            let boards = data
                .lookup
                .state_boards(session.state.unwrap_or(0))
                .await
                .ok();
            let board_map = boards
                .as_ref()
                .and_then(|value| value.get("boards"))
                .and_then(Value::as_object);
            let mut keys: Vec<String> = board_map
                .map(|map| map.keys().cloned().collect())
                .unwrap_or_default();
            keys.sort();
            let board_key = session
                .board
                .as_deref()
                .filter(|key| keys.iter().any(|k| k == key))
                .or_else(|| keys.first().map(String::as_str));
            let selected_rows = board_key
                .and_then(|key| board_map.and_then(|m| m.get(key)))
                .and_then(|block| block.get("rows"))
                .and_then(Value::as_array);
            let lines: Vec<String> = if let Some(rows) = selected_rows {
                rows.iter()
                    .skip(session.page.saturating_mul(15))
                    .take(15)
                    .enumerate()
                    .map(|(offset, row)| {
                        let name = pick_str(row, &["name", "nick_name", "nickname", "game_name"])
                            .unwrap_or("Chief");
                        let score = pick_str(row, &["value_label", "score", "value"])
                            .map(str::to_string)
                            .or_else(|| {
                                pick_i64(row, &["value", "score", "power"])
                                    .map(|n| theme::compact_number(n))
                            })
                            .unwrap_or_else(|| "—".into());
                        let rank = pick_i64(row, &["rank", "kingdom_rank"])
                            .map(|n| format!("#{n}"))
                            .unwrap_or_else(|| format!("#{}", session.page * 15 + offset + 1));
                        format!("**{rank}** · {name} — `{score}`")
                    })
                    .collect()
            } else {
                board_map
                    .map(|map| {
                        map.iter()
                            .take(20)
                            .map(|(key, block)| {
                                let count = block
                                    .get("rows")
                                    .and_then(Value::as_array)
                                    .map(|rows| rows.len())
                                    .unwrap_or(0);
                                format!("• `{key}` — {count} ranked entries")
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };
            if !keys.is_empty() {
                let options = keys
                    .iter()
                    .take(25)
                    .map(|key| {
                        serenity::CreateSelectMenuOption::new(key, key)
                            .default_selection(Some(key.as_str()) == board_key)
                    })
                    .collect();
                rows.push(CreateActionRow::SelectMenu(
                    CreateSelectMenu::new(
                        "lookup:state:board",
                        CreateSelectMenuKind::String { options },
                    )
                    .placeholder(board_key.unwrap_or("Choose a ranking")),
                ));
            }
            embed = embed.description(if lines.is_empty() {
                "_No ranking data available right now._".to_string()
            } else {
                lines.join("\n")
            });
            if let Some(total) = selected_rows.map(|rows| rows.len()) {
                let pages = total.div_ceil(15).max(1);
                if pages > 1 {
                    let last = pages.saturating_sub(1);
                        let prev = session.page.saturating_sub(1);
                        let next = (session.page + 1).min(last);
                        rows.push(CreateActionRow::Buttons(vec![
                            action_button(
                                &format!("lookup:state:pg:p:{prev}:prev"),
                                "Previous",
                                "◀️",
                                serenity::ButtonStyle::Secondary,
                            )
                            .disabled(session.page == 0),
                            action_button(
                                &format!("lookup:state:pg:p:{next}:next"),
                                format!("Page {} / {pages}", session.page + 1),
                                "▶️",
                                serenity::ButtonStyle::Secondary,
                            )
                            .disabled(session.page + 1 >= pages),
                        ]));
                }
            }
        }
        (Kind::State, "alliances") => {
            let alliances = raw
                .get("alliances")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let lines: Vec<String> = alliances
                .iter()
                .take(25)
                .map(|alliance| {
                    let tag = pick_str(alliance, &["abbr", "tag"]).unwrap_or("—");
                    let power = pick_i64(alliance, &["power", "total_power"])
                        .map(theme::compact_number)
                        .unwrap_or_else(|| "—".into());
                    format!("• **[{tag}]** — `{power}`")
                })
                .collect();
            embed = embed.description(if lines.is_empty() {
                "_No alliances on record._".to_string()
            } else {
                lines.join("\n")
            });
            let options = alliances
                .iter()
                .take(25)
                .filter_map(|alliance| {
                    let tag = pick_str(alliance, &["abbr", "tag", "alliance_abbr"])?;
                    let name = pick_str(alliance, &["name"]).unwrap_or(tag);
                    Some(serenity::CreateSelectMenuOption::new(
                        format!("[{tag}] {name}")
                            .chars()
                            .take(100)
                            .collect::<String>(),
                        tag,
                    ))
                })
                .collect::<Vec<_>>();
            if !options.is_empty() {
                rows.push(CreateActionRow::SelectMenu(
                    CreateSelectMenu::new(
                        "lookup:state:alliance",
                        CreateSelectMenuKind::String { options },
                    )
                    .placeholder("Open an alliance in this state"),
                ));
            }
            rows.push(CreateActionRow::Buttons(vec![action_button(
                "lookup:state:alliance:search",
                "Find alliance in this state",
                "🔎",
                serenity::ButtonStyle::Secondary,
            )]));
        }
        (Kind::State, "troopLevel") => {
            use crate::lookup::troop_levels;
            let players = raw
                .get("players")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let lines: Vec<String> = players
                .iter()
                .take(20)
                .map(|player| {
                    let name = pick_str(player, &["name", "game_name"]).unwrap_or("Chief");
                    format!("• **{name}** — {}", troop_levels::troop_summary(player))
                })
                .collect();
            embed = embed.description(if lines.is_empty() {
                "_No troop levels on record._".to_string()
            } else {
                lines.join("\n")
            });
        }
        (Kind::State, "appointments") => {
            let appointments = data
                .lookup
                .state_appointments(session.state.unwrap_or(0))
                .await
                .ok();
            let lines: Vec<String> = appointments
                .as_ref()
                .map(|value| {
                    ["ministers", "offenders"]
                        .iter()
                        .flat_map(|key| {
                            value
                                .get(*key)
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default()
                        })
                        .take(20)
                        .map(|seat| {
                            let name =
                                pick_str(&seat, &["name", "fid", "chief_name"]).unwrap_or("—");
                            let role =
                                pick_str(&seat, &["role", "title", "position"]).unwrap_or("—");
                            format!("• **{role}** — {name}")
                        })
                        .collect()
                })
                .unwrap_or_default();
            embed = embed.description(if lines.is_empty() {
                "_No appointments on record._".to_string()
            } else {
                lines.join("\n")
            });
        }
        _ => {
            embed = embed.description("This view is unavailable for the current lookup.");
        }
    }

    if tab == "choose" && !session.choices.is_empty() {
        // Oracle search can return duplicate alliance tags (and occasionally
        // duplicate player identifiers). Discord requires every select value
        // to be unique, so retain the first candidate for each selectable ID.
        let mut seen_values = HashSet::new();
        let options = session
            .choices
            .iter()
            .enumerate()
            .filter_map(|choice| {
                let (index, choice) = choice;
                let id = pick_id(choice, &["fid", "uid", "player_id", "id", "tag", "abbr"])?;
                if !seen_values.insert(id.clone()) {
                    return None;
                }
                let name = pick_id(
                    choice,
                    &["name", "nick_name", "nickname", "game_name", "tag", "abbr"],
                )
                .unwrap_or_else(|| id.clone());
                let desc = pick_i64(choice, &["kid", "state", "power", "total_power"])
                    .map(|v| v.to_string());
                Some(
                    serenity::CreateSelectMenuOption::new(
                        name.chars().take(100).collect::<String>(),
                        format!("candidate:{index}"),
                    )
                    .description(
                        desc.unwrap_or_else(|| "Select this match".into())
                            .chars()
                            .take(100)
                            .collect::<String>(),
                    ),
                )
            })
            .take(25)
            .collect();
        rows.clear();
        rows.push(CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                match session.kind {
                    Kind::Player => "lookup:player:candidate",
                    Kind::Alliance => "lookup:alliance:candidate",
                    Kind::State => "lookup:state:candidate",
                },
                CreateSelectMenuKind::String { options },
            )
            .placeholder("Choose the correct result"),
        ));
        embed =
            embed.description("Several matches were found. Choose the correct result to continue.");
    }
    {
        let base = &data.cfg.oracle_site;
        let card_result: Option<(String, Vec<u8>)> = match (session.kind, tab) {
            (Kind::Alliance, "overview") => {
                if let Some(state) = session.state.or_else(|| pick_i64(raw, &["kid", "state"])) {
                    if let Ok(map) = data.lookup.state_map(state).await {
                        let tag = session
                            .alliance
                            .as_deref()
                            .or_else(|| pick_str(raw, &["tag", "abbr", "alliance_abbr"]));
                        let aid = pick_id(raw, &["aid", "alliance_id", "id"]).or_else(|| {
                            tag.and_then(|tag| {
                                map.get("alliances")
                                    .and_then(Value::as_array)?
                                    .iter()
                                    .find(|alliance| {
                                        pick_str(alliance, &["abbr", "tag", "alliance_abbr"])
                                            .is_some_and(|value| value.eq_ignore_ascii_case(tag))
                                    })
                                    .and_then(|alliance| {
                                        pick_id(alliance, &["aid", "alliance_id", "id"])
                                    })
                            })
                        });
                        let card = map_card_from_payload(
                            &map,
                            format!(
                                "[{}] {} hive",
                                tag.unwrap_or("Alliance"),
                                pick_str(raw, &["name", "alliance_name"]).unwrap_or(&session.name)
                            ),
                            None,
                            aid,
                        );
                        if let Ok(bytes) = crate::cards::render_map_card(&card) {
                            files.push(serenity::CreateAttachment::bytes(bytes, "hive-map.png"));
                        }
                    }
                }
                let roster = raw
                    .get("roster")
                    .or_else(|| raw.get("members"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let card = crate::cards::AllianceOverview {
                    tag: session.alliance.clone().unwrap_or_else(|| {
                        pick_str(raw, &["tag", "abbr"]).unwrap_or("?").to_string()
                    }),
                    name: pick_str(raw, &["name", "alliance_name"])
                        .unwrap_or(&session.name)
                        .to_string(),
                    state_id: session.state.or_else(|| pick_i64(raw, &["kid", "state"])),
                    leader: pick_str(raw, &["leader", "leader_name"]).map(str::to_owned),
                    leader_role: pick_str(raw, &["leader_role"]).map(str::to_owned),
                    power: pick_i64(raw, &["power", "total_power"]),
                    power_rank: pick_i64(raw, &["power_rank"]),
                    kills: pick_i64(raw, &["kills", "total_kills"]),
                    kills_rank: pick_i64(raw, &["kills_rank"]),
                    members: Some(roster.len() as i64),
                    member_max: pick_i64(raw, &["member_max", "max_members"]),
                    rank: pick_i64(raw, &["rank", "alliance_rank"]),
                    level: pick_i64(raw, &["level", "alliance_level"]),
                    labyrinth: pick_i64(raw, &["labyrinth", "labyrinth_score"]),
                    lab_rank: pick_i64(raw, &["lab_rank"]),
                    notice: pick_str(raw, &["notice", "announcement"]).map(str::to_owned),
                    roster: roster
                        .iter()
                        .take(6)
                        .map(|m| {
                            (
                                pick_str(m, &["name", "game_name", "nickname"])
                                    .unwrap_or("Chief")
                                    .to_string(),
                                pick_i64(m, &["power", "total_power"]),
                                pick_str(m, &["role", "rank_label"]).map(str::to_owned),
                            )
                        })
                        .collect(),
                };
                crate::cards::render_alliance_overview_card(&card)
                    .ok()
                    .map(|bytes| ("alliance.png".to_string(), bytes))
            }
            (Kind::State, "overview") => {
                let root = raw
                    .get("kingdom")
                    .or_else(|| raw.get("data"))
                    .unwrap_or(raw);
                let alliances = root
                    .get("alliances")
                    .and_then(Value::as_array)
                    .map(|v| v.len() as i64);
                let players = root
                    .get("players")
                    .and_then(Value::as_array)
                    .map(|v| v.len() as i64);
                let power = pick_i64(root, &["power", "total_power"]).or_else(|| {
                    root.get("alliances").and_then(Value::as_array).map(|rows| {
                        rows.iter()
                            .filter_map(|a| pick_i64(a, &["power", "total_power"]))
                            .sum()
                    })
                });
                let card = crate::cards::StateOverview {
                    kid: session.state.unwrap_or(0),
                    players,
                    alliances,
                    power,
                    avg_power: match (power, players) {
                        (Some(p), Some(n)) if n > 0 => Some(p / n),
                        _ => None,
                    },
                    top_power: root
                        .get("alliances")
                        .and_then(Value::as_array)
                        .and_then(|rows| {
                            rows.iter()
                                .filter_map(|a| pick_i64(a, &["power", "total_power"]))
                                .max()
                        }),
                    ..Default::default()
                };
                if let Ok(map) = data.lookup.state_map(session.state.unwrap_or(0)).await {
                    let map_card = map_card_from_payload(
                        &map,
                        format!("State {} map", session.state.unwrap_or(0)),
                        None,
                        None,
                    );
                    if let Ok(bytes) = crate::cards::render_map_card(&map_card) {
                        files.push(serenity::CreateAttachment::bytes(bytes, "state-map.png"));
                    }
                }
                crate::cards::render_state_overview_card(&card)
                    .ok()
                    .map(|bytes| ("state.png".to_string(), bytes))
            }
            (Kind::Player, "overview") => {
                let overview = crate::commands::interactions::overview_from_raw(raw);
                match crate::cards::render_player_overview_card(&overview).await {
                    Ok(bytes) => Some(("player.png".to_string(), bytes)),
                    Err(_) => None,
                }
            }
            (Kind::Player, "standings") => {
                let rows = crate::commands::interactions::standings_from_raw(raw);
                if rows.is_empty() {
                    None
                } else {
                    let kid = pick_i64(raw, &["kid", "state", "server"]);
                    crate::cards::render_standings_card(&session.name, kid, &rows)
                        .ok()
                        .map(|bytes| ("standings.png".to_string(), bytes))
                }
            }
            (Kind::Player, "heroes") => {
                let mut hero_image = None;
                if let Some(heroes) = raw.get("heroes").and_then(Value::as_array) {
                    if let Ok(Some(bytes)) = crate::cards::render_arena_lineup(heroes, base).await {
                        hero_image = Some("arena.png");
                        files.push(serenity::CreateAttachment::bytes(bytes, "arena.png"));
                    }
                }
                let loadout = raw
                    .get("oracleLoadout")
                    .or_else(|| raw.get("loadout"))
                    .unwrap_or(raw);
                let gear_private = raw
                    .get("gearPrivacy")
                    .or_else(|| raw.get("gear_privacy"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !gear_private {
                    let cards = crate::cards::render_loadout_cards(loadout, base).await;
                    if let Some(bytes) = cards.chief {
                        if hero_image.is_none() {
                            hero_image = Some("chief-gear.png");
                        }
                        files.push(serenity::CreateAttachment::bytes(bytes, "chief-gear.png"));
                    }
                    for (i, bytes) in cards.heroes.into_iter().enumerate() {
                        files.push(serenity::CreateAttachment::bytes(
                            bytes,
                            format!("hero-{}.png", i + 1),
                        ));
                    }
                }
                let cosmetics = crate::cards::extract_cosmetics(base, Some(raw), Some(loadout));
                if let Ok(Some(bytes)) = crate::cards::render_cosmetics_card(&cosmetics, base).await
                {
                    if hero_image.is_none() {
                        hero_image = Some("cosmetics.png");
                    }
                    files.push(serenity::CreateAttachment::bytes(bytes, "cosmetics.png"));
                }
                if let Some(filename) = hero_image {
                    embed = embed.image(format!("attachment://{filename}"));
                }
                None
            }
            (Kind::Player, "crowns") => {
                let crown = raw.get("crowns").unwrap_or(raw);
                let card = crate::cards::CrownsCard {
                    name: session.name.clone(),
                    crowns: pick_i64(crown, &["count", "crowns", "crown_level"]),
                    terms: pick_i64(crown, &["terms"]),
                    conquests: pick_i64(crown, &["conquests"]),
                    states_ruled: pick_i64(crown, &["states_ruled", "statesRuled"]),
                    thrones: crown
                        .get("thrones")
                        .and_then(Value::as_array)
                        .map(|rows| {
                            rows.iter()
                                .take(8)
                                .map(|throne| crate::cards::ThroneEntry {
                                    state: pick_id(throne, &["kid", "state"])
                                        .unwrap_or_else(|| "—".into()),
                                    badge: throne
                                        .get("home")
                                        .and_then(Value::as_bool)
                                        .filter(|home| *home)
                                        .map(|_| "HOME".into()),
                                    terms: pick_id(throne, &["terms"])
                                        .map(|terms| format!("{terms} term(s)")),
                                    when: pick_str(throne, &["since", "when"]).map(str::to_owned),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    reigns: crown
                        .get("reigns")
                        .and_then(Value::as_array)
                        .map(|rows| {
                            rows.iter()
                                .take(8)
                                .map(|reign| crate::cards::ReignEntry {
                                    date: pick_str(reign, &["date", "day"]).unwrap_or("—").into(),
                                    kind: pick_str(reign, &["kind", "type"])
                                        .unwrap_or("Reign")
                                        .into(),
                                    state: format!(
                                        "State {}",
                                        pick_id(reign, &["kid", "state"])
                                            .unwrap_or_else(|| "—".into())
                                    ),
                                    delta: pick_id(reign, &["delta"]),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    ..Default::default()
                };
                crate::cards::render_crowns_card(&card)
                    .ok()
                    .map(|bytes| ("crowns.png".to_string(), bytes))
            }
            (Kind::State, "svs") => {
                let state_raw = raw
                    .get("state")
                    .or_else(|| raw.get("kingdom"))
                    .unwrap_or(raw);
                let matchup = state_raw
                    .get("kvk_matchup")
                    .or_else(|| state_raw.get("matchup"));
                if let Some(matchup) = matchup {
                    let us = matchup.get("us").unwrap_or(&Value::Null);
                    let them = matchup
                        .get("opponent")
                        .or_else(|| matchup.get("them"))
                        .unwrap_or(&Value::Null);
                    let compare = matchup
                        .get("compare")
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .take(12)
                                .map(|row| crate::cards::StateVsRow {
                                    label: pick_str(row, &["label", "name", "field"])
                                        .unwrap_or("—")
                                        .to_string(),
                                    us: pick_id(row, &["us", "left"]),
                                    them: pick_id(row, &["them", "right"]),
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let card = crate::cards::StateVsCard {
                        season: pick_i64(matchup, &["season"]),
                        us: crate::cards::StateVsSide {
                            kid: pick_i64(us, &["kid"]).or(session.state),
                            label: pick_str(us, &["name", "label"]).map(str::to_owned),
                        },
                        them: crate::cards::StateVsSide {
                            kid: pick_i64(them, &["kid", "state"]),
                            label: pick_str(them, &["name", "label"]).map(str::to_owned),
                        },
                        rows: compare,
                    };
                    if let Ok(bytes) = crate::cards::render_state_vs_card(&card) {
                        files.push(serenity::CreateAttachment::bytes(bytes, "state-vs.png"));
                    }
                }
                let kvk = raw
                    .get("throne")
                    .and_then(|throne| throne.get("kvk"))
                    .or_else(|| raw.get("kvk"));
                let history = kvk.and_then(|value| value.get("svsHistory"));
                let battles = history
                    .and_then(|value| value.get("rows"))
                    .or_else(|| kvk.and_then(|value| value.get("cycles")))
                    .or_else(|| kvk.and_then(|value| value.get("matchups")))
                    .or_else(|| kvk.and_then(|value| value.get("history")))
                    .or_else(|| raw.get("svs"))
                    .or_else(|| raw.get("matches"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let matchup_rows = battles
                    .iter()
                    .take(14)
                    .map(|battle| crate::cards::MatchupRow {
                        date_label: pick_str(battle, &["label", "date", "round"])
                            .map(str::to_owned)
                            .or_else(|| {
                                timestamp_label(battle, &["appointed_at", "dateTs", "ts", "at"])
                            })
                            .unwrap_or_else(|| "Match".to_string()),
                        opponent_kid: pick_i64(battle, &["opponent_kid", "opponent"]),
                        prep: pick_str(battle, &["prep"]).map(str::to_owned),
                        castle: pick_str(battle, &["castle"]).map(str::to_owned),
                        king_name: pick_str(battle, &["king_name", "king"]).map(str::to_owned),
                        king_tag: pick_str(battle, &["king_tag", "alliance_tag", "abbr"])
                            .map(str::to_owned),
                        king_role: pick_str(battle, &["king_role", "role"]).map(str::to_owned),
                        high_king: battle
                            .get("high_king")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                    .collect::<Vec<_>>();
                if !matchup_rows.is_empty() {
                    let history_card = crate::cards::MatchupHistoryCard {
                        title: format!("State {} · SVS", session.state.unwrap_or(0)),
                        castles_taken: pick_i64(history.unwrap_or(&Value::Null), &["won"])
                            .or_else(|| pick_i64(kvk.unwrap_or(&Value::Null), &["won", "wins"])),
                        castles_lost: pick_i64(history.unwrap_or(&Value::Null), &["lost"])
                            .or_else(|| pick_i64(kvk.unwrap_or(&Value::Null), &["lost", "losses"])),
                        rows: matchup_rows,
                        as_of: None,
                    };
                    if let Ok(bytes) = crate::cards::render_matchup_history_card(&history_card) {
                        files.push(serenity::CreateAttachment::bytes(bytes, "svs-history.png"));
                    }
                }
                let latest = battles.first();
                if let Some(latest) = latest {
                    let days = latest
                        .get("days")
                        .and_then(Value::as_array)
                        .map(|days| {
                            days.iter()
                                .filter_map(|day| {
                                    Some(crate::cards::PrepDay {
                                        day: pick_i64(day, &["day"]).unwrap_or(0),
                                        home: pick_i64(day, &["home", "home_score"])? as f64,
                                        away: pick_i64(day, &["away", "away_score"])? as f64,
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if !days.is_empty() {
                        let prep = crate::cards::PrepPointsCard {
                            us_kid: session.state.unwrap_or(0),
                            them_kid: pick_i64(latest, &["opponent_kid", "opponent"]),
                            prep_us: pick_i64(latest, &["prep_score"]).map(|v| v as f64),
                            prep_them: pick_i64(latest, &["prep_opp_score"]).map(|v| v as f64),
                            days,
                            as_of: None,
                        };
                        if let Ok(bytes) = crate::cards::render_prep_points_card(&prep) {
                            files.push(serenity::CreateAttachment::bytes(bytes, "prep-points.png"));
                        }
                    }
                }
                None
            }
            (Kind::State, "appointments") => {
                let value = data
                    .lookup
                    .state_appointments(session.state.unwrap_or(0))
                    .await
                    .ok();
                let seats = |key: &str| {
                    value
                        .as_ref()
                        .and_then(|value| value.get(key))
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .take(12)
                                .map(|seat| crate::cards::AppointmentSeat {
                                    role: pick_str(seat, &["role", "title", "position"])
                                        .unwrap_or("Appointment")
                                        .to_string(),
                                    vacant: seat
                                        .get("vacant")
                                        .and_then(Value::as_bool)
                                        .unwrap_or(false),
                                    name: pick_str(seat, &["name", "fid", "chief_name"])
                                        .map(str::to_owned),
                                    alliance_tag: pick_str(seat, &["alliance_tag", "abbr", "tag"])
                                        .map(str::to_owned),
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                };
                let ministers = seats("ministers");
                let offenders = seats("offenders");
                if ministers.is_empty() && offenders.is_empty() {
                    None
                } else {
                    let card = crate::cards::AppointmentsCard {
                        state_id: session.state.unwrap_or(0),
                        ministers,
                        offenders,
                        as_of: None,
                    };
                    crate::cards::render_appointments_card(&card)
                        .ok()
                        .map(|bytes| ("appointments.png".to_string(), bytes))
                }
            }
            _ => None,
        };
        if let Some((name, bytes)) = card_result {
            embed = embed.image(format!("attachment://{name}"));
            files.push(serenity::CreateAttachment::bytes(bytes, name));
        } else if session.kind == Kind::Player && tab == "progression" && !files.is_empty() {
            embed = embed.image("attachment://progression.png");
        }
    }
    if let Some(notice) = &session.notice {
        embed = embed.field("Notice", notice.clone(), false);
    }
    let _ = ctx;
    (embed, rows, files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(kind: Kind) -> LookupSession {
        LookupSession {
            kind,
            raw: json!({ "name": "Test" }),
            name: "Test".to_string(),
            state: Some(3105),
            fid: None,
            alliance: None,
            tab: "overview".to_string(),
            notice: None,
            choices: Vec::new(),
            board: None,
            page: 0,
            map_token: None,
            created: Instant::now(),
        }
    }

    #[test]
    fn tab_labels_match_javascript() {
        assert_eq!(
            player_tabs()
                .iter()
                .map(|(_, label)| *label)
                .collect::<Vec<_>>(),
            [
                "Overview",
                "Standings",
                "Heroes & Gear",
                "Progression",
                "Crowns"
            ]
        );
        assert_eq!(
            alliance_tabs()
                .iter()
                .map(|(_, label)| *label)
                .collect::<Vec<_>>(),
            ["Overview", "Members", "Furnace", "Troop Level", "Notice"]
        );
        assert_eq!(
            state_tabs()
                .iter()
                .map(|(_, label)| *label)
                .collect::<Vec<_>>(),
            [
                "Overview",
                "State vs State",
                "Rankings",
                "Alliances",
                "Troop Level",
                "Appointments"
            ]
        );
    }

    #[test]
    fn sessions_are_scoped_to_user_and_guild() {
        set(1, 9, session(Kind::Player));
        assert!(get(1, 9).is_some());
        assert!(get(2, 9).is_none());
        assert!(get(1, 8).is_none());
    }

    #[test]
    fn expired_sessions_are_dropped() {
        let mut expired = session(Kind::State);
        expired.created = Instant::now() - TTL - Duration::from_secs(1);
        set(3, 9, expired);
        assert!(get(3, 9).is_none());
    }

    #[test]
    fn raw_field_helpers_read_common_shapes() {
        let raw = json!({ "power": "1,234", "name": "Chief" });
        assert_eq!(pick_i64(&raw, &["power"]), Some(1234));
        assert_eq!(pick_str(&raw, &["name"]), Some("Chief"));
        assert_eq!(
            pick_id(&json!({ "fid": 501234 }), &["fid"]).as_deref(),
            Some("501234")
        );
    }
}
