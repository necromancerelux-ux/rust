//! Canvas image cards rendered with **tiny-skia** (raster) and **ab_glyph**
//! (OpenSans text). SVG assets are rasterized with **resvg**.
//!
//! Ported from the legacy Node renderers:
//! - `src/services/arenaRender.js`        → `render_arena_lineup`
//! - `src/services/lookupCardRender.js`   → chief gear / hero / cosmetics cards
//! - `src/lookup/services/cardRender.js`  → overview / standings / alliance / state cards

mod arena;
mod extra;
mod gear;
mod overview;
mod table;

pub use arena::render_arena_lineup;
#[allow(unused_imports)]
pub use extra::{
    render_appointments_card, render_crowns_card, render_map_card, render_matchup_history_card,
    render_prep_points_card, render_progression_card, AppointmentSeat, AppointmentsCard,
    CrownsCard, MapAlliance, MapCard, MapCity, MatchupHistoryCard, MatchupRow, Milestone, PrepDay,
    PrepPointsCard, ProgressionCard, ReignEntry, SeriesPoint, ThroneEntry,
};
pub use gear::{extract_cosmetics, render_cosmetics_card, render_loadout_cards};
#[allow(unused_imports)]
pub use overview::{
    render_alliance_overview_card, render_player_overview_card, render_standings_card,
    render_state_overview_card, render_state_vs_card, AllianceOverview, PlayerOverview,
    StandingRow, StateOverview, StateVsCard, StateVsRow, StateVsSide,
};
pub use table::{render_table_card, TableCard, TableColumn};

use std::sync::OnceLock;

use ab_glyph::{Font as _, FontVec, Glyph, OutlinedGlyph, PxScale, ScaleFont};
use resvg::usvg;
use tiny_skia::{
    Color, FillRule, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};

// --------------------------------------------------------------- font

fn font() -> &'static FontVec {
    static FONT: OnceLock<FontVec> = OnceLock::new();
    FONT.get_or_init(|| {
        FontVec::try_from_vec(include_bytes!("../../assets/fonts/OpenSans.ttf").to_vec())
            .expect("embedded OpenSans.ttf must parse")
    })
}

pub(crate) fn hex(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    Color::from_rgba8(byte(0), byte(2), byte(4), 0xff)
}

pub(crate) fn solid_paint(color: Color) -> Paint<'static> {
    Paint {
        shader: tiny_skia::Shader::SolidColor(color),
        anti_alias: true,
        ..Default::default()
    }
}

/// Rounded-rect path (quadratic corners).
pub(crate) fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.clamp(0.0, w.min(h) / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

pub(crate) fn fill_round_rect(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    color: Color,
) {
    if let Some(path) = rounded_rect_path(x, y, w, h, r) {
        let paint = solid_paint(color);
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn stroke_round_rect(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    color: Color,
    width: f32,
    dash: Option<tiny_skia::StrokeDash>,
) {
    let Some(path) = rounded_rect_path(x, y, w, h, r) else {
        return;
    };
    let paint = solid_paint(color);
    let stroke = Stroke {
        width,
        line_join: tiny_skia::LineJoin::Round,
        dash,
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

pub(crate) fn fill_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color) {
    let mut builder = PathBuilder::new();
    builder.push_circle(cx, cy, r);
    let Some(path) = builder.finish() else { return };
    let paint = solid_paint(color);
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

pub(crate) fn stroke_circle(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    r: f32,
    color: Color,
    width: f32,
) {
    let mut builder = PathBuilder::new();
    builder.push_circle(cx, cy, r);
    let Some(path) = builder.finish() else { return };
    let paint = solid_paint(color);
    let stroke = Stroke {
        width,
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

/// 5-point star (outer radius `r`, inner radius 0.45·r).
pub(crate) fn fill_star(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color) {
    let mut pb = PathBuilder::new();
    for i in 0..10 {
        let angle = -std::f32::consts::FRAC_PI_2 + (i as f32 * std::f32::consts::PI) / 5.0;
        let radius = if i % 2 == 0 { r } else { r * 0.45 };
        let px = cx + angle.cos() * radius;
        let py = cy + angle.sin() * radius;
        if i == 0 {
            pb.move_to(px, py);
        } else {
            pb.line_to(px, py);
        }
    }
    pb.close();
    let Some(path) = pb.finish() else { return };
    let paint = solid_paint(color);
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

// --------------------------------------------------------------- text

fn blend_pixel(
    data: &mut [u8],
    width: u32,
    height: u32,
    px: i32,
    py: i32,
    rgb: (u8, u8, u8),
    coverage: f32,
) {
    if px < 0 || py < 0 || px as u32 >= width || py as u32 >= height {
        return;
    }
    let src_a = (coverage.clamp(0.0, 1.0) * 255.0).round() as u16;
    if src_a == 0 {
        return;
    }
    let inv = 255 - src_a;
    let offset = (py as u32 * width + px as u32) as usize * 4;
    let dst = &mut data[offset..offset + 4];
    let src_r = (rgb.0 as u16 * src_a) / 255;
    let src_g = (rgb.1 as u16 * src_a) / 255;
    let src_b = (rgb.2 as u16 * src_a) / 255;
    dst[0] = (src_r + (dst[0] as u16 * inv) / 255) as u8;
    dst[1] = (src_g + (dst[1] as u16 * inv) / 255) as u8;
    dst[2] = (src_b + (dst[2] as u16 * inv) / 255) as u8;
    dst[3] = (src_a + (dst[3] as u16 * inv) / 255) as u8;
}

/// Draw `text` with baseline at `y_baseline`. `bold` applies a cheap 1px
/// double-draw (OpenSans ships a single weight for this bundle).
pub(crate) fn draw_text(
    pixmap: &mut Pixmap,
    size: f32,
    x: f32,
    y_baseline: f32,
    text: &str,
    rgb: (u8, u8, u8),
    bold: bool,
) {
    let font = font();
    let scaled = font.as_scaled(PxScale::from(size));
    draw_run(pixmap, &scaled, x, y_baseline, text, rgb);
    if bold {
        draw_run(pixmap, &scaled, x + 0.7, y_baseline, text, rgb);
    }
}

fn draw_run(
    pixmap: &mut Pixmap,
    scaled: &ab_glyph::PxScaleFont<&FontVec>,
    x: f32,
    y_baseline: f32,
    text: &str,
    rgb: (u8, u8, u8),
) {
    let width = pixmap.width();
    let height = pixmap.height();
    let data = pixmap.data_mut();
    let mut cursor = x;
    for ch in text.chars() {
        let mut positioned: Glyph = scaled.scaled_glyph(ch);
        let advance = scaled.h_advance(positioned.id);
        positioned.position = ab_glyph::point(cursor, y_baseline);
        if let Some(outline) = scaled.outline_glyph(positioned) {
            draw_outline(data, width, height, &outline, rgb);
        }
        cursor += advance;
    }
}

fn draw_outline(
    data: &mut [u8],
    width: u32,
    height: u32,
    outline: &OutlinedGlyph,
    rgb: (u8, u8, u8),
) {
    let bounds = outline.px_bounds();
    let base_x = bounds.min.x.floor() as i32;
    let base_y = bounds.min.y.floor() as i32;
    outline.draw(|gx, gy, coverage| {
        blend_pixel(
            data,
            width,
            height,
            base_x + gx as i32,
            base_y + gy as i32,
            rgb,
            coverage,
        );
    });
}

/// Pixel width of `text` for layout.
pub(crate) fn text_width(size: f32, text: &str) -> f32 {
    let font = font();
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0f32;
    for ch in text.chars() {
        let glyph = scaled.scaled_glyph(ch);
        width += scaled.h_advance(glyph.id);
    }
    width
}
// --------------------------------------------------------------- images

/// Fetch+decode a remote image. PNG/JPEG/WebP via `image`; `image/svg+xml`
/// via resvg. Returns `None` on any failure (icons are decorative).
pub(crate) async fn fetch_image(url: &str, referer: &str) -> Option<Pixmap> {
    let client = crate::net::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) omnicore")
        .build()
        .ok()?;
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "image/png,image/svg+xml,image/*")
        .header(reqwest::header::REFERER, referer)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = response.bytes().await.ok()?;
    if bytes.len() < 80 {
        return None;
    }
    if content_type.contains("svg") || bytes.windows(4).any(|w| w == b"<svg") {
        return decode_svg(&bytes);
    }
    decode_raster(&bytes)
}

fn decode_svg(bytes: &[u8]) -> Option<Pixmap> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &options).ok()?;
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return None;
    }
    let mut pixmap = Pixmap::new(size.width().ceil() as u32, size.height().ceil() as u32)?;
    resvg::render(&tree, Transform::from_scale(1.0, 1.0), &mut pixmap.as_mut());
    Some(pixmap)
}

fn decode_raster(bytes: &[u8]) -> Option<Pixmap> {
    let image = image::load_from_memory(bytes).ok()?;
    let (width, height) = (image.width().max(1), image.height().max(1));
    let rgba = image.to_rgba8().into_raw();
    Some(premultiplied_pixmap(rgba, width, height))
}

/// Build a tiny-skia pixmap from straight RGBA8 bytes (premultiplies).
pub(crate) fn premultiplied_pixmap(rgba: Vec<u8>, width: u32, height: u32) -> Pixmap {
    let mut premul = vec![0u8; rgba.len()];
    for (src, dst) in rgba.chunks_exact(4).zip(premul.chunks_exact_mut(4)) {
        let a = src[3] as u16;
        dst[0] = (src[0] as u16 * a / 255) as u8;
        dst[1] = (src[1] as u16 * a / 255) as u8;
        dst[2] = (src[2] as u16 * a / 255) as u8;
        dst[3] = src[3];
    }
    Pixmap::from_vec(
        premul,
        tiny_skia::IntSize::from_wh(width, height).expect("non-zero dimensions"),
    )
    .expect("premultiplied buffer length matches")
}

/// Stretch-draw an image into an exact rectangle (legacy `drawImage` behavior).
pub(crate) fn draw_image_stretched(
    canvas: &mut Pixmap,
    image: &Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let transform = Transform::from_row(
        w / image.width() as f32,
        0.0,
        0.0,
        h / image.height() as f32,
        x,
        y,
    );
    let paint = PixmapPaint::default();
    canvas.draw_pixmap(0, 0, image.as_ref(), &paint, transform, None);
}

/// Contain-fit (keep aspect ratio, centered) inside a box.
pub(crate) fn draw_image_contain(
    canvas: &mut Pixmap,
    image: &Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let iw = image.width() as f32;
    let ih = image.height() as f32;
    if iw <= 0.0 || ih <= 0.0 {
        return;
    }
    let scale = (w / iw).min(h / ih);
    let sw = iw * scale;
    let sh = ih * scale;
    draw_image_stretched(
        canvas,
        image,
        x + (w - sw) / 2.0,
        y + (h - sh) / 2.0,
        sw,
        sh,
    );
}

/// Encode a pixmap to PNG (demultiplied straight alpha).
pub(crate) fn encode_png(pixmap: &Pixmap) -> anyhow::Result<Vec<u8>> {
    let width = pixmap.width();
    let height = pixmap.height();
    let demultiplied = pixmap.clone().take_demultiplied();
    let image = image::RgbaImage::from_raw(width, height, demultiplied)
        .ok_or_else(|| anyhow::anyhow!("pixmap buffer length mismatch"))?;
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_rect_is_finite() {
        let path = rounded_rect_path(0.0, 0.0, 100.0, 50.0, 10.0);
        assert!(path.is_some(), "rounded rect path must build");
    }

    #[test]
    fn draw_text_marks_pixels() {
        let mut pixmap = Pixmap::new(120, 40).expect("pixmap");
        draw_text(&mut pixmap, 16.0, 4.0, 24.0, "Hi", (255, 255, 255), false);
        let transparent = pixmap
            .data()
            .chunks_exact(4)
            .filter(|px| px[3] == 0)
            .count();
        assert!(transparent < 120 * 40, "at least one pixel must be painted");
        let total: usize = 120 * 40;
        let opaque = total - transparent;
        assert!(
            opaque > 10,
            "text should cover more than a handful of pixels, got {opaque}"
        );
    }

    #[test]
    fn encode_png_has_signature() {
        let pixmap = Pixmap::new(4, 4).expect("pixmap");
        let bytes = encode_png(&pixmap).expect("png encode");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn premultiplied_roundtrip_blends_image() {
        // 4x4 opaque red square
        let red = [255, 0, 0, 255].repeat(16);
        let image = premultiplied_pixmap(red, 4, 4);
        let mut canvas = Pixmap::new(16, 16).expect("canvas");
        canvas.fill(Color::from_rgba8(10, 20, 30, 255));
        draw_image_stretched(&mut canvas, &image, 8.0, 8.0, 4.0, 4.0);
        let data = canvas.data();
        let px = |x: u32, y: u32| {
            let offset = (y * 16 + x) as usize * 4;
            (
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            )
        };
        assert_eq!(px(8, 8), (255, 0, 0, 255));
        assert_eq!(px(9, 9), (255, 0, 0, 255));
        assert_eq!(px(0, 0), (10, 20, 30, 255));
    }

    #[test]
    fn draw_contain_keeps_aspect() {
        // 8x4 image contain-fit into a 8x8 box → centered vertically at y=2..6
        let blue = [0, 0, 255, 255].repeat(32);
        let image = premultiplied_pixmap(blue, 8, 4);
        let mut canvas = Pixmap::new(8, 8).expect("canvas");
        draw_image_contain(&mut canvas, &image, 0.0, 0.0, 8.0, 8.0);
        let data = canvas.data();
        let px = |x: u32, y: u32| {
            let offset = (y * 8 + x) as usize * 4;
            (
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            )
        };
        assert_eq!(px(4, 0).3, 0, "top band must stay transparent");
        assert_eq!(px(4, 3), (0, 0, 255, 255));
        assert_eq!(px(4, 7).3, 0, "bottom band must stay transparent");
    }

    #[test]
    fn normalize_whiteout_stats_hero() {
        let hero = serde_json::json!({
            "id": 12,
            "level": 35,
            "star": 60,
            "name": "Aisha",
            "gear": [
                {"eid": 101, "enhancement_level": 12, "slot": 1},
                {"eid": 102, "enhancement_level": 8, "slot": 2}
            ],
            "position": 2
        });
        let normalized = arena::normalize_hero(&hero).expect("ws hero parses");
        assert_eq!(normalized.id, 12);
        assert_eq!(normalized.level, Some(35));
        assert_eq!(normalized.star, Some(60));
        assert_eq!(normalized.gear.len(), 2);
        assert_eq!(normalized.gear[0].level, Some(12));
        assert_eq!(normalized.position, 2);
    }

    #[test]
    fn normalize_oracle_hero_with_exclusive() {
        let hero = serde_json::json!({
            "id": 77,
            "lv": 42,
            "star_level": 72,
            "equipment": [
                {"eid": 2011, "slv": 15, "rlv": 3}
            ],
            "exclusive_equip": 6015,
            "exclusive_equip_lv": 14
        });
        let normalized = arena::normalize_hero(&hero).expect("oracle hero parses");
        assert_eq!(normalized.level, Some(42));
        assert_eq!(normalized.star, Some(72));
        assert_eq!(normalized.exclusive_id, Some(6015));
        assert_eq!(normalized.exclusive_level, Some(14));
        assert_eq!(normalized.gear[0].level, Some(15));
        assert_eq!(normalized.gear[0].refine, Some(3));
    }

    #[test]
    fn normalize_hero_rejects_missing_id() {
        assert!(arena::normalize_hero(&serde_json::json!({"name": "x"})).is_none());
    }

    #[test]
    fn finds_lord_gear_pieces_with_default_path() {
        let loadout = serde_json::json!({
            "battle": {
                "lord_gear": {
                    "pieces": [
                        {"lords_equipment": {"equipid": 105, "e_pid": 1, "tier": 4}}
                    ]
                }
            }
        });
        let pieces = gear::find_lord_gear_pieces(&loadout);
        assert_eq!(pieces.len(), 1);
    }

    #[test]
    fn finds_lord_gear_pieces_by_deep_search() {
        let loadout = serde_json::json!({
            "payload": { "odd": { "nested": [
                {"lords_equipment": {"equipid": 205, "e_pid": 2}}
            ] }}
        });
        assert_eq!(gear::find_lord_gear_pieces(&loadout).len(), 1);
    }

    #[test]
    fn piece_parts_reads_equipment_wrapper() {
        let piece = serde_json::json!({
            "lords_equipment": {"equipid": 305, "e_pid": 3, "tier": 4, "gems": [{"k": 3}, {"k": 5}]}
        });
        let parts = gear::piece_parts(&piece);
        assert_eq!(parts.slot, Some(3));
        assert_eq!(parts.equip_id, Some(305));
        assert_eq!(parts.tier, Some(4));
        assert_eq!(parts.gems, vec![3, 5]);
    }

    #[test]
    fn lord_gear_url_maps_tiers() {
        let base = "https://wosoracle.com";
        assert_eq!(
            gear::lord_gear_url(base, Some(3), Some(105)).as_deref(),
            Some("https://wosoracle.com/assets/wos/gear/3001.png")
        );
        assert_eq!(gear::lord_gear_url(base, None, None), None);
        assert_eq!(gear::lord_gear_url(base, Some(9), None), None);
    }

    #[test]
    fn cosmetics_from_object_shape() {
        let raw = serde_json::json!({
            "cosmetics": {
                "city_skin": {"id": 42, "name": "Kyoto", "permanent": true},
                "march_skin": {"name": "Aurora"}
            }
        });
        let items = gear::extract_cosmetics("https://wosoracle.com", Some(&raw), None);
        assert_eq!(items.len(), 2);
        let city = items.iter().find(|i| i.slot == "city_skin").expect("city");
        assert_eq!(city.status.as_deref(), Some("permanent"));
        assert!(city
            .image_url
            .as_deref()
            .unwrap_or("")
            .contains("cosmetics/city_skin/42.png"));
        assert!(items.iter().any(|i| i.slot == "march_skin"));
    }

    #[test]
    fn cosmetics_from_array_shape() {
        let raw = serde_json::json!({
            "cosmetics": [
                {"type": "chat_bubble", "id": 7, "name": "Bubbles"}
            ]
        });
        let items = gear::extract_cosmetics("https://wosoracle.com", Some(&raw), None);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].slot, "chat_bubble");
    }

    #[test]
    fn fmt_thresholds() {
        assert_eq!(overview::fmt(None), "—");
        assert_eq!(overview::fmt(Some(999)), "999");
        assert_eq!(overview::fmt(Some(1_500)), "1.5K");
        assert_eq!(overview::fmt(Some(2_500_000)), "2.50M");
        assert_eq!(overview::fmt(Some(3_000_000_000)), "3.00B");
    }
}
