//! Chief/lord gear card, hero card and cosmetics strip.
//! Ported from `src/services/lookupCardRender.js`.

use serde_json::Value;

use super::{
    draw_image_contain, draw_image_stretched, draw_text, encode_png, fetch_image, fill_circle,
    fill_round_rect, fill_star, hex, stroke_circle, stroke_round_rect, text_width,
};

fn pick<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    keys.iter()
        .filter_map(|key| object.get(*key))
        .find(|found| !found.is_null())
}

fn num_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|v| v as i64)),
        Value::String(s) => s.trim().parse::<f64>().ok().map(|v| v as i64),
        _ => None,
    }
}

fn num_field(value: &Value, keys: &[&str]) -> Option<i64> {
    pick(value, keys).and_then(num_value)
}

pub fn hero_bust_url(base: &str, id: i64) -> String {
    format!("{base}/assets/wos/heroes_bust/{id}.png")
}

pub fn hero_gear_url(base: &str, eid: i64) -> String {
    format!("{base}/assets/wos/hero_gear/{eid}.png")
}

pub fn lord_gear_url(base: &str, e_pid: Option<i64>, equip_id: Option<i64>) -> Option<String> {
    let slot = e_pid?;
    if !(1..=6).contains(&slot) {
        return None;
    }
    let mut art = 6;
    if let Some(equip_id) = equip_id {
        let code: i64 = equip_id.rem_euclid(100);
        art = if code <= 10 {
            1
        } else if code <= 20 {
            2
        } else if code <= 30 {
            3
        } else if code <= 40 {
            4
        } else if code <= 50 {
            5
        } else {
            6
        };
    }
    Some(format!("{base}/assets/wos/gear/{}.png", slot * 1000 + art))
}

fn gem_color(k: Option<i64>) -> tiny_skia::Color {
    match k {
        Some(3) => hex("#58d68d"),
        Some(4) => hex("#f4d03f"),
        Some(5) => hex("#af7ac5"),
        _ => hex("#5dade2"),
    }
}

/// Does this look like one chief-gear piece? (mirrors the legacy check)
fn looks_like_gear_piece(row: &Value) -> bool {
    row.is_object()
        && (row.get("lords_equipment").is_some()
            || row.get("e_pid").is_some()
            || row.get("equipid").is_some())
}

/// Find the chief/lord gear piece array wherever Oracle put it.
pub fn find_lord_gear_pieces(loadout: &Value) -> Vec<Value> {
    if !loadout.is_object() && !loadout.is_array() {
        return Vec::new();
    }

    let known: Vec<Option<&Value>> = vec![
        pick(loadout, &["battle"])
            .and_then(|b| pick(b, &["lord_gear"]))
            .and_then(|v| pick(v, &["pieces"])),
        pick(loadout, &["battle"]).and_then(|b| pick(b, &["lord_gear"])),
        pick(loadout, &["lord_gear"]).and_then(|v| pick(v, &["pieces"])),
        pick(loadout, &["lord_gear"]),
        pick(loadout, &["battle"])
            .and_then(|b| pick(b, &["lords_gear"]))
            .and_then(|v| pick(v, &["pieces"])),
        pick(loadout, &["lords_gear"]).and_then(|v| pick(v, &["pieces"])),
        pick(loadout, &["lords_gear"]),
        pick(loadout, &["chief_gear"]).and_then(|v| pick(v, &["pieces"])),
        pick(loadout, &["chief_gear"]),
        pick(loadout, &["battle"])
            .and_then(|b| pick(b, &["chief_gear"]))
            .and_then(|v| pick(v, &["pieces"])),
        pick(loadout, &["battle"]).and_then(|b| pick(b, &["chief_gear"])),
        pick(loadout, &["gear"]).and_then(|v| pick(v, &["lord"])),
        pick(loadout, &["gear"]).and_then(|v| pick(v, &["pieces"])),
    ];

    for candidate in known {
        if let Some(list) = candidate.and_then(Value::as_array) {
            if !list.is_empty() && looks_like_gear_piece(&list[0]) {
                return list.clone();
            }
        }
    }

    // Bounded deep search (depth ≤ 5)
    fn walk(node: &Value, depth: u32) -> Option<Vec<Value>> {
        if depth > 5 {
            return None;
        }
        match node {
            Value::Array(list) => {
                if !list.is_empty() && looks_like_gear_piece(&list[0]) {
                    return Some(list.clone());
                }
                for item in list {
                    if let Some(found) = walk(item, depth + 1) {
                        return Some(found);
                    }
                }
                None
            }
            Value::Object(map) => {
                for value in map.values() {
                    if let Some(found) = walk(value, depth + 1) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }
    walk(loadout, 0).unwrap_or_default()
}

pub struct PieceParts {
    pub slot: Option<i64>,
    pub equip_id: Option<i64>,
    pub gems: Vec<i64>,
    pub tier: Option<i64>,
}

/// Normalize a piece so rendering works with either shape.
pub fn piece_parts(piece: &Value) -> PieceParts {
    let equipment = pick(piece, &["lords_equipment"]).unwrap_or(piece);
    PieceParts {
        slot: num_field(piece, &["e_pid"]).or_else(|| num_field(equipment, &["e_pid", "slot"])),
        equip_id: num_field(equipment, &["equipid", "equip_id", "id"]),
        gems: pick(equipment, &["gems"])
            .or_else(|| pick(piece, &["gems"]))
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|gem| {
                        num_field(gem, &["k", "id", "type"]).or_else(|| num_value(gem))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        tier: num_field(equipment, &["tier"]).or_else(|| num_field(piece, &["tier"])),
    }
}

/// Chief / lord gear as a 2×3 card grid (like the website).
pub async fn render_chief_gear_card(
    pieces: &[Value],
    asset_base: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    if pieces.is_empty() {
        return Ok(None);
    }

    let mut sorted: Vec<(Value, PieceParts)> = pieces
        .iter()
        .map(|piece| (piece.clone(), piece_parts(piece)))
        .collect();
    sorted.sort_by_key(|(_, parts)| parts.slot.unwrap_or(0));
    sorted.truncate(6);

    const CARD_W: f32 = 130.0;
    const CARD_H: f32 = 118.0;
    const GEM_ROW: f32 = 36.0;
    const GAP_X: f32 = 22.0;
    const GAP_Y: f32 = 12.0;
    const PAD: f32 = 16.0;
    const COLS: usize = 3;
    const ROWS: usize = 2;
    let width = (PAD * 2.0 + COLS as f32 * CARD_W + (COLS - 1) as f32 * GAP_X).ceil() as u32;
    let height =
        (PAD * 2.0 + ROWS as f32 * (CARD_H + GEM_ROW) + (ROWS - 1) as f32 * GAP_Y).ceil() as u32;

    let mut canvas = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("pixmap alloc failed"))?;
    canvas.fill(hex("#050b14"));

    // Fetch icons up front
    let mut icons = Vec::with_capacity(sorted.len());
    for (_, parts) in &sorted {
        let url = lord_gear_url(asset_base, parts.slot, parts.equip_id);
        icons.push(match url {
            Some(url) => fetch_image(&url, &format!("{asset_base}/")).await,
            None => None,
        });
    }

    for (index, (_piece, parts)) in sorted.iter().enumerate() {
        let col = (index % COLS) as f32;
        let row = (index / COLS) as f32;
        let x = PAD + col * (CARD_W + GAP_X);
        let y = PAD + row * (CARD_H + GEM_ROW + GAP_Y);

        // Card — salmon gradient with a light outline
        let gradient = tiny_skia::LinearGradient::new(
            tiny_skia::Point::from_xy(x, y),
            tiny_skia::Point::from_xy(x, y + CARD_H),
            vec![
                tiny_skia::GradientStop::new(0.0, hex("#e0705f")),
                tiny_skia::GradientStop::new(1.0, hex("#c4503f")),
            ],
            tiny_skia::SpreadMode::Pad,
            tiny_skia::Transform::identity(),
        );
        if let Some(shader) = gradient {
            let paint = tiny_skia::Paint {
                shader,
                anti_alias: true,
                ..Default::default()
            };
            if let Some(path) = super::rounded_rect_path(x, y, CARD_W, CARD_H, 14.0) {
                canvas.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        } else {
            fill_round_rect(&mut canvas, x, y, CARD_W, CARD_H, 14.0, hex("#e0705f"));
        }
        stroke_round_rect(
            &mut canvas,
            x,
            y,
            CARD_W,
            CARD_H,
            14.0,
            hex("#f0a090"),
            2.0,
            None,
        );

        // Tier label
        let tier_label = parts
            .tier
            .map(|tier| format!("T{tier}"))
            .unwrap_or_else(|| "T4".to_string());
        draw_text(
            &mut canvas,
            18.0,
            x + 9.0,
            y + 24.0,
            &tier_label,
            (247, 211, 107),
            true,
        );

        // 3 vertical stars
        for star in 0..3 {
            fill_star(
                &mut canvas,
                x + 16.0,
                y + 42.0 + star as f32 * 17.0,
                7.0,
                hex("#f5b83d"),
            );
        }

        // Gear art
        const ART: f32 = 86.0;
        match &icons[index] {
            Some(image) => {
                draw_image_stretched(
                    &mut canvas,
                    image,
                    x + CARD_W - ART - 6.0,
                    y + 24.0,
                    ART,
                    ART,
                );
            }
            None => {
                draw_text(
                    &mut canvas,
                    12.0,
                    x + 44.0,
                    y + 70.0,
                    &format!("Slot {}", parts.slot.unwrap_or(-1)),
                    (255, 255, 255),
                    true,
                );
            }
        }

        // Gems row
        let gems: Vec<Option<i64>> = if parts.gems.is_empty() {
            vec![Some(1), Some(1), Some(1)]
        } else {
            parts.gems.iter().take(3).copied().map(Some).collect()
        };
        let gem_radius = 11.0f32;
        let total = gems.len() as f32 * (gem_radius * 2.0 + 6.0) - 6.0;
        let mut gx = x + (CARD_W - total) / 2.0 + gem_radius;
        let gy = y + CARD_H + GEM_ROW / 2.0 + 2.0;
        for gem in gems {
            fill_circle(&mut canvas, gx, gy, gem_radius, gem_color(gem));
            stroke_circle(&mut canvas, gx, gy, gem_radius, hex("#c9a24a"), 3.0);
            fill_circle(
                &mut canvas,
                gx - 3.0,
                gy - 3.0,
                3.5,
                tiny_skia::Color::from_rgba8(255, 255, 255, 140),
            );
            gx += gem_radius * 2.0 + 6.0;
        }
    }

    Ok(Some(encode_png(&canvas)?))
}

/// Single hero card (bust + exclusive + up to 4 gear slots).
pub async fn render_hero_card(hero: &Value, asset_base: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(id) = num_field(hero, &["id", "hero_id", "hid"]) else {
        return Ok(None);
    };
    let level = num_field(hero, &["lv", "level", "hero_level"]);
    let star = num_field(hero, &["star", "stars", "star_level"]);

    const WIDTH: u32 = 480;
    const HEIGHT: u32 = 180;
    let mut canvas = tiny_skia::Pixmap::new(WIDTH, HEIGHT)
        .ok_or_else(|| anyhow::anyhow!("pixmap alloc failed"))?;
    canvas.fill(tiny_skia::Color::TRANSPARENT);

    // Card background
    fill_round_rect(
        &mut canvas,
        0.0,
        0.0,
        WIDTH as f32,
        HEIGHT as f32,
        14.0,
        hex("#0d1218"),
    );
    stroke_round_rect(
        &mut canvas,
        0.0,
        0.0,
        WIDTH as f32,
        HEIGHT as f32,
        14.0,
        hex("#2a3544"),
        2.0,
        None,
    );

    // Bust panel
    fill_round_rect(&mut canvas, 10.0, 10.0, 110.0, 160.0, 10.0, hex("#151c26"));
    if let Some(image) =
        fetch_image(&hero_bust_url(asset_base, id), &format!("{asset_base}/")).await
    {
        draw_image_stretched(&mut canvas, &image, 15.0, 15.0, 100.0, 150.0);
    }

    // Lv badge
    fill_round_rect(
        &mut canvas,
        14.0,
        14.0,
        48.0,
        20.0,
        6.0,
        tiny_skia::Color::from_rgba8(0, 0, 0, 170),
    );
    draw_text(
        &mut canvas,
        12.0,
        20.0,
        28.0,
        &format!(
            "Lv{}",
            level.map(|l| l.to_string()).unwrap_or_else(|| "?".into())
        ),
        (241, 196, 15),
        true,
    );

    // Stars
    let star_count = star.map(|star| (star / 6).clamp(1, 5)).unwrap_or(1);
    for i in 0..star_count {
        fill_star(
            &mut canvas,
            20.0 + i as f32 * 14.0,
            152.0,
            6.0,
            hex("#f5c542"),
        );
    }

    // Title line
    let title = format!(
        "Arena — ID {} — {} stars",
        id,
        star.map(|s| s.to_string()).unwrap_or_else(|| "?".into())
    );
    draw_text(
        &mut canvas,
        11.0,
        130.0,
        24.0,
        &title,
        (189, 195, 199),
        false,
    );

    // Icons: exclusive first, then up to 4 equipment pieces
    type GearIcon = (Option<i64>, Option<i64>, Option<i64>, bool);
    let mut items: Vec<GearIcon> = Vec::new();
    if let Some(exclusive) = num_field(hero, &["exclusive_equip"]) {
        items.push((
            Some(exclusive),
            num_field(hero, &["exclusive_equip_lv"]),
            None,
            true,
        ));
    }
    if let Some(equipment) = pick(hero, &["equipment"]).and_then(Value::as_array) {
        for piece in equipment.iter().take(4) {
            items.push((
                num_field(piece, &["eid"]),
                num_field(piece, &["slv"]),
                num_field(piece, &["rlv"]),
                false,
            ));
        }
    }

    let mut ix = 130.0f32;
    const SLOT_W: f32 = 62.0;
    const SLOT_H: f32 = 100.0;
    let iy = 40.0f32;

    for (eid, forge, enh, exclusive) in items {
        let have_enh = enh.unwrap_or(0) > 0;
        if have_enh {
            let gradient = tiny_skia::LinearGradient::new(
                tiny_skia::Point::from_xy(ix, iy),
                tiny_skia::Point::from_xy(ix + SLOT_W, iy + SLOT_H),
                vec![
                    tiny_skia::GradientStop::new(0.0, hex("#a51d1d")),
                    tiny_skia::GradientStop::new(1.0, hex("#4d0e0e")),
                ],
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            );
            if let Some(shader) = gradient {
                let paint = tiny_skia::Paint {
                    shader,
                    anti_alias: true,
                    ..Default::default()
                };
                if let Some(path) = super::rounded_rect_path(ix, iy, SLOT_W, SLOT_H, 8.0) {
                    canvas.fill_path(
                        &path,
                        &paint,
                        tiny_skia::FillRule::Winding,
                        tiny_skia::Transform::identity(),
                        None,
                    );
                }
            } else {
                fill_round_rect(&mut canvas, ix, iy, SLOT_W, SLOT_H, 8.0, hex("#a51d1d"));
            }
        } else {
            fill_round_rect(&mut canvas, ix, iy, SLOT_W, SLOT_H, 8.0, hex("#1a2330"));
        }
        let stroke = if exclusive {
            hex("#f1c40f")
        } else {
            hex("#3d4f63")
        };
        stroke_round_rect(&mut canvas, ix, iy, SLOT_W, SLOT_H, 8.0, stroke, 1.5, None);

        if exclusive {
            fill_star(
                &mut canvas,
                ix + SLOT_W / 2.0,
                iy + 12.0,
                7.0,
                hex("#f1c40f"),
            );
        } else if let Some(enh) = enh.filter(|value| *value > 0) {
            draw_text(
                &mut canvas,
                11.0,
                ix + 8.0,
                iy + 16.0,
                &format!("+{enh}"),
                (252, 165, 165),
                true,
            );
        }

        if let Some(eid) = eid {
            if let Some(image) =
                fetch_image(&hero_gear_url(asset_base, eid), &format!("{asset_base}/")).await
            {
                draw_image_stretched(&mut canvas, &image, ix + 9.0, iy + 24.0, 44.0, 44.0);
            }
        }

        if let Some(forge) = forge {
            draw_text(
                &mut canvas,
                11.0,
                ix + 12.0,
                iy + 88.0,
                &format!("Lv {forge}"),
                (236, 240, 241),
                true,
            );
        }

        ix += SLOT_W + 8.0;
    }

    Ok(Some(encode_png(&canvas)?))
}

// --------------------------------------------------------------- cosmetics

#[derive(Debug, Clone)]
pub struct CosmeticItem {
    pub slot: String,
    pub slot_label: String,
    pub name: String,
    pub id: Option<i64>,
    pub status: Option<String>,
    pub image_url: Option<String>,
}

impl CosmeticItem {
    pub fn label(&self) -> String {
        let base = if self.name != self.slot_label {
            format!("{}: {}", self.slot_label, self.name)
        } else {
            self.slot_label.clone()
        };
        match &self.status {
            Some(status) => format!("{base} · {status}"),
            None => base,
        }
    }
}

/// (key, label, aliases) — display order + the key names Oracle might use.
const COSMETIC_SLOTS: &[(&str, &str, &[&str])] = &[
    (
        "avatar_frame",
        "Avatar frame",
        &["avatar_frame", "avatarframe", "frame", "avatar_border"],
    ),
    (
        "city_skin",
        "City skin",
        &["city_skin", "cityskin", "city", "building_skin"],
    ),
    (
        "march_skin",
        "March skin",
        &["march_skin", "marchskin", "march"],
    ),
    ("nameplate", "Nameplate", &["nameplate", "name_plate"]),
    ("decoration", "Decoration", &["decoration", "decor"]),
    (
        "chat_bubble",
        "Chat bubble",
        &["chat_bubble", "chatbubble", "bubble"],
    ),
    (
        "profile_background",
        "Profile background",
        &[
            "profile_background",
            "profilebackground",
            "profile_bg",
            "background",
            "bg",
        ],
    ),
    (
        "avatar_badge",
        "Avatar badge",
        &["avatar_badge", "avatarbadge", "badge"],
    ),
    ("formation", "Formation", &["formation", "formation_skin"]),
];

fn norm_key(key: &str) -> String {
    key.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '-' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn slot_for_key(
    key: &str,
) -> Option<&'static (&'static str, &'static str, &'static [&'static str])> {
    let normalized = norm_key(key);
    COSMETIC_SLOTS.iter().find(|(_, _, aliases)| {
        aliases.iter().any(|alias| {
            *alias == normalized || alias.replace('_', "") == normalized.replace('_', "")
        })
    })
}

fn cosmetic_image_url(base: &str, slot_key: &str, entry: &Value) -> Option<String> {
    for key in ["image", "image_url", "icon", "icon_url", "img", "url"] {
        if let Some(direct) = pick(entry, &[key]).and_then(Value::as_str) {
            let direct = direct.trim();
            if direct.is_empty() {
                continue;
            }
            if direct.to_ascii_lowercase().starts_with("http://")
                || direct.to_ascii_lowercase().starts_with("https://")
            {
                return Some(direct.to_string());
            }
            return Some(if direct.starts_with('/') {
                format!("{base}{direct}")
            } else {
                format!("{base}/{direct}")
            });
        }
    }
    let id = num_field(entry, &["id", "item_id", "skin_id"])?;
    let template = std::env::var("ORACLE_COSMETIC_PATH_TEMPLATE")
        .unwrap_or_else(|_| "/assets/wos/cosmetics/{slot}/{id}.png".to_string());
    Some(format!(
        "{base}{}",
        template
            .replace("{slot}", slot_key)
            .replace("{id}", &id.to_string())
    ))
}

fn normalize_cosmetic_entry(
    base: &str,
    slot: &(&'static str, &'static str, &'static [&'static str]),
    raw: &Value,
) -> Option<CosmeticItem> {
    let (slot_key, slot_label, _) = *slot;

    if raw.is_null() {
        return None;
    }
    if let Some(text) = raw.as_str() {
        if text.trim().is_empty() {
            return None;
        }
    }
    if let Some(number) = raw.as_i64() {
        if number == 0 {
            return None;
        }
    }
    if raw.as_bool() == Some(false) {
        return None;
    }

    let entry: Value = match raw {
        Value::Object(_) => raw.clone(),
        Value::Number(_) => serde_json::json!({ "id": raw }),
        Value::Bool(true) => return None,
        _ => serde_json::json!({ "name": raw.as_str().unwrap_or_default() }),
    };

    let name = pick(&entry, &["name", "title", "label", "display_name"])
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string());
    let id = num_field(&entry, &["id", "item_id", "skin_id"]);

    // Permanent vs expiring
    let mut status = None;
    let permanent = pick(&entry, &["permanent", "is_permanent"]).and_then(Value::as_bool);
    let expires = pick(
        &entry,
        &[
            "expires_at",
            "expire_at",
            "expire_time",
            "expires",
            "expiry",
        ],
    )
    .cloned();
    match (permanent, expires.as_ref()) {
        (Some(true), _) => status = Some("permanent".to_string()),
        (Some(false), None) => status = None,
        (Some(false), Some(_)) => {}
        (None, Some(exp)) => {
            let expires_ms = |value: &Value| -> Option<i64> {
                match value {
                    Value::Number(n) => {
                        let n = n.as_i64().or_else(|| n.as_f64().map(|v| v as i64))?;
                        Some(if n < 1_000_000_000_000 { n * 1000 } else { n })
                    }
                    Value::String(s) => {
                        let trimmed = s.trim();
                        if matches!(trimmed, "0" | "-1") {
                            return Some(0);
                        }
                        trimmed
                            .parse::<i64>()
                            .ok()
                            .map(|n| if n < 1_000_000_000_000 { n * 1000 } else { n })
                            .or_else(|| {
                                chrono::DateTime::parse_from_rfc3339(trimmed)
                                    .ok()
                                    .map(|dt| dt.timestamp_millis())
                            })
                    }
                    _ => None,
                }
            };
            match expires_ms(exp) {
                Some(0) => status = Some("permanent".to_string()),
                Some(ms) if ms > 0 => {
                    if let Some(date) = chrono::DateTime::from_timestamp_millis(ms) {
                        status = Some(format!("until {}", date.format("%Y-%m-%d")));
                    }
                }
                _ => {}
            }
        }
        (None, None) => {}
    }

    let has_image = ["image", "icon", "image_url"].iter().any(|key| {
        pick(&entry, &[*key])
            .map(|value| !value.is_null())
            .unwrap_or(false)
    });
    let fallback_name = name.or_else(|| id.map(|id| format!("#{id}")));
    if fallback_name.is_none() && !has_image {
        return None;
    }

    let display_name = fallback_name.unwrap_or_else(|| slot_label.to_string());
    let image_url = cosmetic_image_url(base, slot_key, &entry);

    Some(CosmeticItem {
        slot: slot_key.to_string(),
        slot_label: slot_label.to_string(),
        name: display_name,
        id,
        status,
        image_url,
    })
}

/// Pull cosmetics out of the player profile and/or loadout JSON.
pub fn extract_cosmetics(
    base: &str,
    player_raw: Option<&Value>,
    loadout: Option<&Value>,
) -> Vec<CosmeticItem> {
    let mut sources: Vec<&Value> = Vec::new();
    if let Some(raw) = player_raw {
        sources.push(raw);
        for key in ["cosmetics", "cosmetic", "skins"] {
            if let Some(source) = raw.get(key) {
                sources.push(source);
            }
        }
        if let Some(object) = raw.as_object() {
            // whole raw object may directly hold slot keys
            let _ = object;
        }
    }
    if let Some(loadout) = loadout {
        sources.push(loadout);
        for key in ["cosmetics", "cosmetic"] {
            if let Some(source) = loadout.get(key) {
                sources.push(source);
            }
        }
        if let Some(extras) = loadout.get("extras").and_then(|v| v.get("cosmetics")) {
            sources.push(extras);
        }
        if let Some(battle) = loadout.get("battle").and_then(|v| v.get("cosmetics")) {
            sources.push(battle);
        }
    }

    let mut found: std::collections::HashMap<&'static str, CosmeticItem> =
        std::collections::HashMap::new();

    for source in sources {
        match source {
            Value::Array(rows) => {
                for row in rows {
                    let Some(object) = row.as_object() else {
                        continue;
                    };
                    let key = ["type", "slot", "category", "kind"]
                        .iter()
                        .find_map(|name| object.get(*name).and_then(Value::as_str));
                    let Some(key) = key else { continue };
                    let Some(slot) = slot_for_key(key) else {
                        continue;
                    };
                    if found.contains_key(slot.0) {
                        continue;
                    }
                    if let Some(item) = normalize_cosmetic_entry(base, slot, row) {
                        found.insert(slot.0, item);
                    }
                }
            }
            Value::Object(map) => {
                // direct slot keys
                for (key, value) in map {
                    let Some(slot) = slot_for_key(key) else {
                        continue;
                    };
                    if found.contains_key(slot.0) {
                        continue;
                    }
                    if let Some(item) = normalize_cosmetic_entry(base, slot, value) {
                        found.insert(slot.0, item);
                    }
                }
            }
            _ => {}
        }
    }

    COSMETIC_SLOTS
        .iter()
        .filter_map(|slot| found.get(slot.0).cloned())
        .collect()
}

/// "COSMETICS" strip — icon + label per item, flowing and wrapping.
pub async fn render_cosmetics_card(
    items: &[CosmeticItem],
    asset_base: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    if items.is_empty() {
        return Ok(None);
    }

    const WIDTH: u32 = 720;
    const PAD: f32 = 18.0;
    const ICON: f32 = 44.0;
    const ROW_H: f32 = 60.0;
    const GAP_X: f32 = 22.0;
    const TEXT_GAP: f32 = 8.0;
    const HEADER: f32 = 34.0;

    // Layout with wrapping
    struct Placed {
        index: usize,
        x: f32,
        row: usize,
        label: String,
    }
    let mut placed = Vec::with_capacity(items.len());
    let mut x = PAD;
    let mut row = 0usize;
    for (index, item) in items.iter().enumerate() {
        let label = item.label();
        let w = ICON + TEXT_GAP + text_width(13.0, &label);
        if x + w > WIDTH as f32 - PAD && x > PAD {
            x = PAD;
            row += 1;
        }
        placed.push(Placed {
            index,
            x,
            row,
            label,
        });
        x += w + GAP_X;
    }
    let rows = row + 1;
    let height = (HEADER + rows as f32 * ROW_H + PAD).ceil() as u32;

    let mut canvas = tiny_skia::Pixmap::new(WIDTH, height)
        .ok_or_else(|| anyhow::anyhow!("pixmap alloc failed"))?;
    canvas.fill(hex("#050b14"));
    draw_text(
        &mut canvas,
        11.0,
        PAD,
        22.0,
        "C O S M E T I C S",
        (138, 151, 168),
        true,
    );

    // Fetch icons
    let mut images = Vec::with_capacity(items.len());
    for item in items {
        images.push(match &item.image_url {
            Some(url) => fetch_image(url, &format!("{asset_base}/")).await,
            None => None,
        });
    }

    for entry in &placed {
        let y = HEADER + entry.row as f32 * ROW_H + (ROW_H - ICON) / 2.0 - 4.0;
        match &images[entry.index] {
            Some(image) => {
                draw_image_contain(&mut canvas, image, entry.x, y, ICON, ICON);
            }
            None => {
                stroke_round_rect(
                    &mut canvas,
                    entry.x + 2.0,
                    y + 2.0,
                    ICON - 4.0,
                    ICON - 4.0,
                    8.0,
                    hex("#2a3544"),
                    1.5,
                    tiny_skia::StrokeDash::new(vec![4.0, 3.0], 0.0),
                );
            }
        }
        draw_text(
            &mut canvas,
            13.0,
            entry.x + ICON + TEXT_GAP,
            y + ICON / 2.0 + 5.0,
            &entry.label,
            (232, 234, 237),
            false,
        );
    }

    Ok(Some(encode_png(&canvas)?))
}

/// Aggregation used by the player panel: chief gear + hero cards.
pub struct LoadoutCards {
    pub chief: Option<Vec<u8>>,
    pub heroes: Vec<Vec<u8>>,
}

pub async fn render_loadout_cards(loadout: &Value, asset_base: &str) -> LoadoutCards {
    let mut out = LoadoutCards {
        chief: None,
        heroes: Vec::new(),
    };

    let pieces = find_lord_gear_pieces(loadout);
    if !pieces.is_empty() {
        if let Ok(card) = render_chief_gear_card(&pieces, asset_base).await {
            out.chief = card;
        }
    }

    let heroes = loadout
        .get("heroes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut sorted = heroes;
    sorted.sort_by_key(|hero| num_field(hero, &["pos", "slot"]).unwrap_or(0));
    for hero in sorted.into_iter().take(5) {
        if let Ok(Some(card)) = render_hero_card(&hero, asset_base).await {
            out.heroes.push(card);
        }
    }

    out
}
