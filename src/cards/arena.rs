//! Arena Lineup card — `src/services/arenaRender.js`.
//! Up to 5 heroes stacked vertically: bust, level badge, stars and gear icons.

use serde_json::Value;
use tiny_skia::Pixmap;

use super::{
    draw_image_stretched, draw_text, encode_png, fetch_image, fill_round_rect, fill_star, hex,
    text_width,
};

#[derive(Debug, Clone, Default)]
pub struct ArenaHero {
    pub id: i64,
    pub name: String,
    pub level: Option<i64>,
    pub star: Option<i64>,
    pub exclusive_id: Option<i64>,
    pub exclusive_level: Option<i64>,
    pub gear: Vec<GearPiece>,
    pub position: i64,
}

#[derive(Debug, Clone, Default)]
pub struct GearPiece {
    pub eid: Option<i64>,
    pub level: Option<i64>,
    pub refine: Option<i64>,
    pub slot: Option<i64>,
}

fn num_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|v| v as i64)),
        Value::String(s) => s.trim().parse::<f64>().ok().map(|v| v as i64),
        _ => None,
    }
}

fn pick<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key))
        .filter(|found| !found.is_null())
}

fn signed(value: Option<&Value>) -> Option<i64> {
    value.and_then(num_value)
}

/// Normalize one hero from either the Whiteout Stats or Oracle shape.
pub fn normalize_hero(raw: &Value) -> Option<ArenaHero> {
    let _ = raw.as_object()?;
    let id = pick(raw, &["id", "hero_id", "hid"]).and_then(num_value)?;

    let level = pick(raw, &["level", "lv", "hero_level"]).and_then(num_value);
    let star = pick(raw, &["star", "stars", "star_level"]).and_then(num_value);

    // Exclusive / widget
    let mut exclusive_id = None;
    let mut exclusive_level = None;
    if let Some(exclusive) = pick(raw, &["exclusive_gear"]) {
        exclusive_id = pick(exclusive, &["id", "eid"]).and_then(num_value);
        exclusive_level = pick(exclusive, &["level"])
            .and_then(num_value)
            .or_else(|| signed(pick(raw, &["exclusive_gear_level"])));
    } else if let Some(equip) = pick(raw, &["exclusive_equip"]).and_then(num_value) {
        exclusive_id = Some(equip);
        exclusive_level =
            pick(raw, &["exclusive_equip_lv", "exclusive_gear_level"]).and_then(num_value);
    }

    // Gear slots: Whiteout Stats `gear`, Oracle `equipment` (slv/rlv)
    let gear_source = pick(raw, &["gear", "equipment"])
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let gear = gear_source
        .iter()
        .map(|piece| GearPiece {
            eid: pick(piece, &["eid", "id", "equip_id"]).and_then(num_value),
            level: pick(piece, &["enhancement_level", "gear_level", "slv", "level"])
                .and_then(num_value),
            refine: pick(piece, &["refine_level", "rlv"]).and_then(num_value),
            slot: pick(piece, &["slot", "sid"]).and_then(num_value),
        })
        .collect();

    let name = pick(raw, &["name"])
        .and_then(Value::as_str)
        .map(|name| name.to_string())
        .unwrap_or_else(|| format!("Hero {id}"));

    Some(ArenaHero {
        id,
        name,
        level,
        star,
        exclusive_id,
        exclusive_level,
        gear,
        position: pick(raw, &["position", "pos", "slot"])
            .and_then(num_value)
            .unwrap_or(0),
    })
}

fn hero_bust_url(base: &str, id: i64) -> String {
    format!("{base}/assets/wos/heroes_bust/{id}.png")
}

fn hero_gear_url(base: &str, eid: i64) -> String {
    format!("{base}/assets/wos/hero_gear/{eid}.png")
}

/// Render the combined Arena Lineup image (up to 5 heroes).
/// `None` when the payload has no usable heroes.
pub async fn render_arena_lineup(
    heroes: &[Value],
    asset_base: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    let mut list: Vec<ArenaHero> = heroes.iter().filter_map(normalize_hero).collect();
    list.sort_by_key(|hero| hero.position);
    list.truncate(5);
    if list.is_empty() {
        return Ok(None);
    }

    const WIDTH: u32 = 520;
    const ROW_H: u32 = 118;
    const PAD: u32 = 14;
    const GAP: u32 = 10;
    let rows = list.len() as u32;
    let height = PAD * 2 + rows * ROW_H + (rows - 1) * GAP;

    let mut canvas =
        Pixmap::new(WIDTH, height).ok_or_else(|| anyhow::anyhow!("pixmap alloc failed"))?;

    // Backgrounds
    let bg = hex("#0a0e14");
    canvas.fill(bg);
    fill_round_rect(
        &mut canvas,
        6.0,
        6.0,
        WIDTH as f32 - 12.0,
        height as f32 - 12.0,
        12.0,
        hex("#121820"),
    );

    for (index, hero) in list.iter().enumerate() {
        let y = (PAD + index as u32 * (ROW_H + GAP)) as f32;
        let row_bg = if index % 2 == 0 {
            hex("#111820")
        } else {
            hex("#0e141c")
        };
        fill_round_rect(
            &mut canvas,
            PAD as f32,
            y,
            WIDTH as f32 - 2.0 * PAD as f32,
            ROW_H as f32,
            10.0,
            row_bg,
        );

        // Bust panel + image
        const BUST: f32 = 90.0;
        let bx = (PAD + 10) as f32;
        let by = y + (ROW_H as f32 - BUST) / 2.0;
        fill_round_rect(&mut canvas, bx, by, BUST, BUST, 8.0, hex("#1a2330"));
        if let Some(image) = fetch_image(
            &hero_bust_url(asset_base, hero.id),
            &format!("{asset_base}/"),
        )
        .await
        {
            draw_image_stretched(
                &mut canvas,
                &image,
                bx + 4.0,
                by + 4.0,
                BUST - 8.0,
                BUST - 8.0,
            );
        }

        // Level badge
        if let Some(level) = hero.level {
            fill_round_rect(
                &mut canvas,
                bx + 4.0,
                by + 4.0,
                42.0,
                18.0,
                5.0,
                tiny_skia::Color::from_rgba8(0, 0, 0, 180),
            );
            draw_text(
                &mut canvas,
                11.0,
                bx + 8.0,
                by + 17.0,
                &format!("Lv.{level}"),
                (241, 196, 15),
                true,
            );
        }

        // Stars
        let star_count = hero.star.map(|star| (star / 6).clamp(1, 5)).unwrap_or(1);
        for star in 0..star_count {
            fill_star(
                &mut canvas,
                bx + 12.0 + star as f32 * 14.0,
                by + BUST - 10.0,
                5.0,
                hex("#f5c542"),
            );
        }

        // Gear icons row (exclusive first, then up to 5 pieces)
        let mut items: Vec<(Option<i64>, Option<i64>, bool)> = Vec::new();
        if let Some(eid) = hero.exclusive_id {
            items.push((Some(eid), hero.exclusive_level, true));
        }
        items.extend(
            hero.gear
                .iter()
                .take(5)
                .map(|piece| (piece.eid, piece.level, false)),
        );

        const SLOT: f32 = 52.0;
        let mut ix = bx + BUST + 18.0;
        let iy = y + (ROW_H as f32 - SLOT) / 2.0;

        for (eid, level, exclusive) in items.into_iter().take(6) {
            // Slot background
            let (bg, stroke) = if exclusive {
                (hex("#2a2208"), hex("#f1c40f"))
            } else {
                (hex("#1a2330"), hex("#3d4f63"))
            };
            fill_round_rect(&mut canvas, ix, iy, SLOT, SLOT, 7.0, bg);
            super::stroke_round_rect(&mut canvas, ix, iy, SLOT, SLOT, 7.0, stroke, 1.5, None);

            if let Some(eid) = eid {
                if let Some(image) =
                    fetch_image(&hero_gear_url(asset_base, eid), &format!("{asset_base}/")).await
                {
                    draw_image_stretched(&mut canvas, &image, ix + 6.0, iy + 4.0, 40.0, 40.0);
                }
            }

            if let Some(level) = level {
                draw_text(
                    &mut canvas,
                    11.0,
                    ix + SLOT / 2.0 - text_width(11.0, &format!("Lv.{level}")) / 2.0,
                    iy + SLOT + 13.0,
                    &format!("Lv.{level}"),
                    (232, 234, 237),
                    true,
                );
            }

            ix += SLOT + 10.0;
        }
    }

    Ok(Some(encode_png(&canvas)?))
}
