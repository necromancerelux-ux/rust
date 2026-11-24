//! Generic dark "table" card renderer (ministry roster, future scoreboards).

use tiny_skia::{Color, Pixmap};

use super::{draw_text, encode_png, fill_round_rect, hex, stroke_round_rect, text_width};

pub struct TableColumn {
    pub label: String,
    pub width: f32,
}

pub struct TableCard {
    pub title: String,
    pub subtitle: Option<String>,
    pub columns: Vec<TableColumn>,
    pub rows: Vec<Vec<String>>,
    pub footer: Option<String>,
}

const BG: &str = "#0a1626";
const PANEL: &str = "#10294a";
const ACCENT: &str = "#6cc4ff";
const TEXT: (u8, u8, u8) = (238, 244, 251);
const TITLE: (u8, u8, u8) = (238, 244, 251);
const MUTED: (u8, u8, u8) = (143, 165, 191);
const ZEBRA: &str = "#0d1f38";

pub fn render_table_card(card: &TableCard) -> anyhow::Result<Vec<u8>> {
    const PAD: f32 = 18.0;
    const ROW_H: f32 = 26.0;
    const HEADER_H: f32 = 88.0;
    const FOOTER_H: f32 = 28.0;

    let table_width: f32 = card.columns.iter().map(|c| c.width).sum();
    let width = (PAD * 2.0 + table_width + 4.0).ceil() as u32;
    let rows = card.rows.len().max(1);
    let height = (HEADER_H + rows as f32 * ROW_H + PAD + FOOTER_H).ceil() as u32;

    let mut canvas =
        Pixmap::new(width, height).ok_or_else(|| anyhow::anyhow!("pixmap alloc failed"))?;
    canvas.fill(hex(BG));
    fill_round_rect(
        &mut canvas,
        0.0,
        0.0,
        width as f32,
        height as f32,
        16.0,
        hex(BG),
    );
    fill_round_rect(
        &mut canvas,
        12.0,
        12.0,
        width as f32 - 24.0,
        height as f32 - 24.0,
        14.0,
        hex(PANEL),
    );
    let bar =
        tiny_skia::Rect::from_xywh(12.0, 12.0, 4.0, height as f32 - 24.0).expect("non-empty bar");
    canvas.fill_rect(
        bar,
        &super::solid_paint(hex(ACCENT)),
        tiny_skia::Transform::identity(),
        None,
    );

    // Title + subtitle
    draw_text(&mut canvas, 22.0, PAD, 42.0, &card.title, TITLE, true);
    if let Some(subtitle) = &card.subtitle {
        draw_text(&mut canvas, 12.0, PAD, 62.0, subtitle, MUTED, false);
    }

    // Column headers
    let mut x = PAD;
    let header_y = 82.0f32;
    for column in &card.columns {
        draw_text(
            &mut canvas,
            10.0,
            x + 6.0,
            header_y,
            &column.label.to_uppercase(),
            MUTED,
            true,
        );
        x += column.width;
    }
    let line_y = header_y + 8.0;
    let span = table_width + 4.0;
    canvas.fill_rect(
        tiny_skia::Rect::from_xywh(PAD - 4.0, line_y, span, 2.0).expect("divider"),
        &super::solid_paint(hex(ACCENT)),
        tiny_skia::Transform::identity(),
        None,
    );

    // Rows
    for (index, row) in card.rows.iter().enumerate().take(rows) {
        let y = HEADER_H + index as f32 * ROW_H;
        if index % 2 == 1 {
            fill_round_rect(
                &mut canvas,
                PAD - 4.0,
                y - 4.0,
                span,
                ROW_H,
                4.0,
                hex(ZEBRA),
            );
        }
        let mut x = PAD;
        for (ci, column) in card.columns.iter().enumerate() {
            let cell = row.get(ci).cloned().unwrap_or_default();
            let cell = truncate_to_width(&cell, 12.0, column.width - 12.0);
            draw_text(&mut canvas, 12.0, x + 6.0, y + 14.0, &cell, TEXT, false);
            x += column.width;
        }
    }

    // Footer
    if let Some(footer) = &card.footer {
        draw_text(
            &mut canvas,
            11.0,
            PAD,
            (height as f32) - 14.0,
            footer,
            MUTED,
            false,
        );
    }

    stroke_round_rect(
        &mut canvas,
        12.0,
        12.0,
        width as f32 - 24.0,
        height as f32 - 24.0,
        14.0,
        Color::from_rgba8(0x2a, 0x35, 0x44, 255),
        1.0,
        None,
    );
    encode_png(&canvas)
}

/// Clip `text` so it fits into `max_px` at the given size (character granularity).
fn truncate_to_width(text: &str, size: f32, max_px: f32) -> String {
    if text_width(size, text) <= max_px {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let candidate = format!("{out}{ch}…");
        if text_width(size, &candidate) > max_px {
            break;
        }
        out.push(ch);
    }
    if out.is_empty() {
        "…".to_string()
    } else {
        format!("{out}…")
    }
}
