//! Extra lookup image cards: progression, crowns, matchup history,
//! preparation rankings, appointments, and simplified state/alliance/player maps.
//! Visual language matches the navy-glass INTEL cards (OMNICORE branding only).

use tiny_skia::{Color, PathBuilder, Pixmap};

use super::overview::{base_card, fit_str, fmt};
use super::{
    draw_text, encode_png, fill_round_rect, hex, solid_paint, stroke_round_rect, text_width,
};

const W: u32 = 1060;
const TEXT: (u8, u8, u8) = (238, 244, 251);
const MUTED: (u8, u8, u8) = (143, 165, 191);
const SLATE: (u8, u8, u8) = (93, 115, 144);
const LABEL: (u8, u8, u8) = (183, 140, 255);
const NUM: (u8, u8, u8) = (108, 196, 255);
const GREEN: (u8, u8, u8) = (79, 211, 124);
const RED: (u8, u8, u8) = (255, 93, 93);
const ORANGE: (u8, u8, u8) = (245, 159, 58);
const GOLD: (u8, u8, u8) = (245, 197, 66);
const EDGE: &str = "#27507f";
const BOX: &str = "#0d1f38";
const ACCENT: &str = "#6cc4ff";

fn footer(canvas: &mut Pixmap, h: u32, right: &str) {
    draw_text(canvas, 13.0, 42.0, h as f32 - 34.0, "OMNICORE", SLATE, true);
    if !right.is_empty() {
        let tw = text_width(13.0, right);
        draw_text(
            canvas,
            13.0,
            W as f32 - 42.0 - tw,
            h as f32 - 34.0,
            right,
            SLATE,
            false,
        );
    }
}

fn hline(canvas: &mut Pixmap, y: f32) {
    if let Some(rect) = tiny_skia::Rect::from_xywh(42.0, y, W as f32 - 84.0, 1.0) {
        canvas.fill_rect(
            rect,
            &solid_paint(Color::from_rgba8(130, 170, 220, 46)),
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

fn row_band(canvas: &mut Pixmap, y: f32, h: f32) {
    if let Some(rect) = tiny_skia::Rect::from_xywh(48.0, y, W as f32 - 96.0, h) {
        canvas.fill_rect(
            rect,
            &solid_paint(Color::from_rgba8(120, 170, 230, 18)),
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

// --------------------------------------------------------------- progression

#[derive(Debug, Clone, Default)]
pub struct SeriesPoint {
    pub v: f64,
    pub t: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct Milestone {
    pub date: String,
    pub kind: String,
    pub change: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProgressionCard {
    pub name: String,
    pub range_label: Option<String>,
    pub power_snaps: Option<i64>,
    pub kill_snaps: Option<i64>,
    pub power_series: Vec<SeriesPoint>,
    pub kill_series: Vec<SeriesPoint>,
    pub milestones: Vec<Milestone>,
    pub as_of: Option<String>,
}

fn spark(canvas: &mut Pixmap, pts: &[SeriesPoint], x: f32, y: f32, w: f32, h: f32, color: Color) {
    if pts.len() < 2 {
        draw_text(
            canvas,
            14.0,
            x,
            y + h / 2.0,
            "Not enough snapshots yet",
            SLATE,
            false,
        );
        return;
    }
    let min_v = pts.iter().map(|p| p.v).fold(f64::INFINITY, f64::min);
    let max_v = pts.iter().map(|p| p.v).fold(f64::NEG_INFINITY, f64::max);
    let span = (max_v - min_v).max(1.0);
    let mut pb = PathBuilder::new();
    for (i, p) in pts.iter().enumerate() {
        let px = x + (i as f32 / (pts.len() - 1) as f32) * w;
        let py = y + h - ((p.v - min_v) / span) as f32 * h;
        if i == 0 {
            pb.move_to(px, py);
        } else {
            pb.line_to(px, py);
        }
    }
    if let Some(path) = pb.finish() {
        let mut stroke = tiny_skia::Stroke::default();
        stroke.width = 2.5;
        stroke.line_cap = tiny_skia::LineCap::Round;
        stroke.line_join = tiny_skia::LineJoin::Round;
        canvas.stroke_path(
            &path,
            &solid_paint(color),
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

pub fn render_progression_card(data: &ProgressionCard) -> anyhow::Result<Vec<u8>> {
    let ms = data.milestones.len().min(6);
    let h = 560 + ms as u32 * 30;
    let mut canvas = base_card(W, h)?;

    draw_text(&mut canvas, 13.0, 46.0, 58.0, "PROGRESSION", LABEL, true);
    draw_text(
        &mut canvas,
        32.0,
        46.0,
        96.0,
        &fit_str(32.0, &data.name, 760.0),
        TEXT,
        true,
    );
    let sub = format!(
        "{}  ·  {} power  ·  {} kill count snapshots",
        data.range_label.as_deref().unwrap_or(""),
        data.power_snaps.unwrap_or(0),
        data.kill_snaps.unwrap_or(0)
    );
    draw_text(&mut canvas, 16.0, 46.0, 124.0, &sub, MUTED, false);

    draw_text(&mut canvas, 12.0, 46.0, 156.0, "POWER", MUTED, false);
    spark(
        &mut canvas,
        &data.power_series,
        46.0,
        164.0,
        W as f32 - 92.0,
        110.0,
        hex(ACCENT),
    );
    if let Some(first) = data.power_series.first() {
        draw_text(
            &mut canvas,
            14.0,
            46.0,
            296.0,
            &fmt(Some(first.v as i64)),
            MUTED,
            false,
        );
    }
    if let Some(last) = data.power_series.last() {
        let s = fmt(Some(last.v as i64));
        let tw = text_width(16.0, &s);
        draw_text(
            &mut canvas,
            16.0,
            W as f32 - 46.0 - tw,
            296.0,
            &s,
            NUM,
            true,
        );
    }

    draw_text(&mut canvas, 12.0, 46.0, 332.0, "KILL COUNT", MUTED, false);
    spark(
        &mut canvas,
        &data.kill_series,
        46.0,
        340.0,
        W as f32 - 92.0,
        110.0,
        Color::from_rgba8(245, 159, 58, 255),
    );
    if let Some(first) = data.kill_series.first() {
        draw_text(
            &mut canvas,
            14.0,
            46.0,
            472.0,
            &fmt(Some(first.v as i64)),
            MUTED,
            false,
        );
    }
    if let Some(last) = data.kill_series.last() {
        let s = fmt(Some(last.v as i64));
        let tw = text_width(16.0, &s);
        draw_text(
            &mut canvas,
            16.0,
            W as f32 - 46.0 - tw,
            472.0,
            &s,
            ORANGE,
            true,
        );
    }

    draw_text(&mut canvas, 12.0, 46.0, 512.0, "MILESTONES", MUTED, false);
    for (i, m) in data.milestones.iter().take(6).enumerate() {
        let y = 540.0 + i as f32 * 30.0;
        draw_text(&mut canvas, 15.0, 46.0, y, &m.date, MUTED, false);
        draw_text(&mut canvas, 15.0, 200.0, y, &m.kind, MUTED, false);
        draw_text(
            &mut canvas,
            15.0,
            360.0,
            y,
            &fit_str(15.0, &m.change, W as f32 - 406.0),
            TEXT,
            false,
        );
    }

    footer(
        &mut canvas,
        h,
        &data
            .as_of
            .as_ref()
            .map(|s| format!("Last snapshot {s}"))
            .unwrap_or_default(),
    );
    encode_png(&canvas)
}

// --------------------------------------------------------------- crowns

#[derive(Debug, Clone, Default)]
pub struct ThroneEntry {
    pub state: String,
    pub badge: Option<String>,
    pub terms: Option<String>,
    pub when: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ReignEntry {
    pub date: String,
    pub kind: String,
    pub state: String,
    pub delta: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CrownsCard {
    pub name: String,
    pub crowns: Option<i64>,
    pub terms: Option<i64>,
    pub conquests: Option<i64>,
    pub states_ruled: Option<i64>,
    pub thrones: Vec<ThroneEntry>,
    pub reigns: Vec<ReignEntry>,
    pub as_of: Option<String>,
}

pub fn render_crowns_card(data: &CrownsCard) -> anyhow::Result<Vec<u8>> {
    let reigns_n = data.reigns.len().min(6);
    let h = 360 + reigns_n as u32 * 34;
    let mut canvas = base_card(W, h)?;

    draw_text(&mut canvas, 13.0, 46.0, 58.0, "CROWNS", LABEL, true);
    draw_text(
        &mut canvas,
        32.0,
        46.0,
        98.0,
        &fit_str(32.0, &data.name, 760.0),
        TEXT,
        true,
    );
    if data.crowns.unwrap_or(0) > 0 {
        draw_text(&mut canvas, 48.0, W as f32 - 90.0, 96.0, "♛", GOLD, true);
    }
    hline(&mut canvas, 124.0);

    let cols = [
        ("Crowns", data.crowns.unwrap_or(0)),
        ("Terms", data.terms.unwrap_or(0)),
        ("Conquests", data.conquests.unwrap_or(0)),
        ("States ruled", data.states_ruled.unwrap_or(0)),
    ];
    let cw = (W as f32 - 84.0) / cols.len() as f32;
    for (i, (lab, val)) in cols.iter().enumerate() {
        let x = 46.0 + i as f32 * cw;
        draw_text(
            &mut canvas,
            12.0,
            x,
            150.0,
            &lab.to_uppercase(),
            MUTED,
            false,
        );
        draw_text(
            &mut canvas,
            28.0,
            x,
            190.0,
            &val.to_string(),
            if i == 0 { NUM } else { TEXT },
            true,
        );
    }
    hline(&mut canvas, 240.0);

    draw_text(&mut canvas, 12.0, 46.0, 266.0, "THRONES", MUTED, false);
    for (i, t) in data.thrones.iter().take(3).enumerate() {
        let y = 296.0 + i as f32 * 32.0;
        draw_text(
            &mut canvas,
            18.0,
            46.0,
            y,
            &format!("State {}", t.state),
            TEXT,
            true,
        );
        if let Some(b) = &t.badge {
            draw_text(&mut canvas, 14.0, 380.0, y, b, GREEN, true);
        }
        if let Some(terms) = &t.terms {
            draw_text(&mut canvas, 15.0, 640.0, y, terms, MUTED, false);
        }
        if let Some(when) = &t.when {
            let tw = text_width(15.0, when);
            draw_text(
                &mut canvas,
                15.0,
                W as f32 - 46.0 - tw,
                y,
                when,
                MUTED,
                false,
            );
        }
    }

    let ry = 296.0 + data.thrones.len().min(3).max(1) as f32 * 32.0 + 14.0;
    draw_text(&mut canvas, 12.0, 46.0, ry, "REIGN HISTORY", MUTED, false);
    for (i, r) in data.reigns.iter().take(6).enumerate() {
        let y = ry + 30.0 + i as f32 * 32.0;
        draw_text(&mut canvas, 15.0, 46.0, y, &r.date, MUTED, false);
        draw_text(&mut canvas, 15.0, 220.0, y, &r.kind, MUTED, false);
        draw_text(&mut canvas, 15.0, 380.0, y, &r.state, MUTED, false);
        if let Some(d) = &r.delta {
            let tw = text_width(15.0, d);
            draw_text(&mut canvas, 15.0, W as f32 - 46.0 - tw, y, d, TEXT, false);
        }
    }

    footer(
        &mut canvas,
        h,
        &data
            .as_of
            .as_ref()
            .map(|s| format!("Crown record as of {s}"))
            .unwrap_or_default(),
    );
    encode_png(&canvas)
}

// --------------------------------------------------------------- matchup history

#[derive(Debug, Clone, Default)]
pub struct MatchupRow {
    pub date_label: String,
    pub opponent_kid: Option<i64>,
    pub prep: Option<String>,
    pub castle: Option<String>,
    pub king_name: Option<String>,
    pub king_tag: Option<String>,
    pub king_role: Option<String>,
    pub high_king: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MatchupHistoryCard {
    pub title: String,
    pub castles_taken: Option<i64>,
    pub castles_lost: Option<i64>,
    pub rows: Vec<MatchupRow>,
    pub as_of: Option<String>,
}

fn outcome_color(v: &str) -> (u8, u8, u8) {
    let s = v.to_ascii_lowercase();
    if s == "won" || s == "held" {
        GREEN
    } else if s == "lost" {
        RED
    } else {
        MUTED
    }
}

fn outcome_label(v: &Option<String>) -> String {
    match v {
        None => "—".into(),
        Some(s) if s.is_empty() => "—".into(),
        Some(s) => {
            let mut c = s.chars();
            match c.next() {
                None => "—".into(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        }
    }
}

pub fn render_matchup_history_card(data: &MatchupHistoryCard) -> anyhow::Result<Vec<u8>> {
    let row_h = 36.0f32;
    let rows_n = data.rows.len().min(14);
    let h = (170.0 + rows_n as f32 * row_h + 40.0).ceil() as u32;
    let mut canvas = base_card(W, h)?;

    draw_text(
        &mut canvas,
        13.0,
        50.0,
        52.0,
        &data.title.to_uppercase(),
        LABEL,
        true,
    );

    // Castles taken / lost tiles
    let tile_y = 70.0f32;
    let tw = 200.0f32;
    let th = 64.0f32;
    let tile =
        |canvas: &mut Pixmap, x: f32, title: &str, value: Option<i64>, color: (u8, u8, u8)| {
            fill_round_rect(
                canvas,
                x,
                tile_y,
                tw,
                th,
                14.0,
                Color::from_rgba8(120, 170, 230, 15),
            );
            stroke_round_rect(canvas, x, tile_y, tw, th, 14.0, hex(EDGE), 1.5, None);
            let title_w = text_width(11.0, title);
            draw_text(
                canvas,
                11.0,
                x + tw / 2.0 - title_w / 2.0,
                tile_y + 22.0,
                title,
                MUTED,
                true,
            );
            let val = value.map(|n| n.to_string()).unwrap_or_else(|| "—".into());
            let vw = text_width(28.0, &val);
            draw_text(
                canvas,
                28.0,
                x + tw / 2.0 - vw / 2.0,
                tile_y + 50.0,
                &val,
                color,
                true,
            );
        };
    tile(
        &mut canvas,
        50.0,
        "CASTLES TAKEN",
        data.castles_taken,
        GREEN,
    );
    tile(&mut canvas, 270.0, "CASTLES LOST", data.castles_lost, RED);

    let header_y = tile_y + th + 28.0;
    draw_text(&mut canvas, 11.0, 50.0, header_y, "DATE", MUTED, true);
    draw_text(&mut canvas, 11.0, 180.0, header_y, "OPPONENT", MUTED, true);
    draw_text(&mut canvas, 11.0, 300.0, header_y, "PREP", MUTED, true);
    draw_text(&mut canvas, 11.0, 400.0, header_y, "CASTLE", MUTED, true);
    draw_text(&mut canvas, 11.0, 510.0, header_y, "KING", MUTED, true);
    hline(&mut canvas, header_y + 10.0);

    for (i, r) in data.rows.iter().take(14).enumerate() {
        let y = header_y + 36.0 + i as f32 * row_h;
        if i % 2 == 1 {
            row_band(&mut canvas, y - 22.0, row_h);
        }
        draw_text(&mut canvas, 14.0, 50.0, y, &r.date_label, TEXT, false);
        let opp = r
            .opponent_kid
            .map(|k| format!("#{k}"))
            .unwrap_or_else(|| "—".into());
        draw_text(&mut canvas, 14.0, 180.0, y, &opp, MUTED, false);
        let prep = outcome_label(&r.prep);
        draw_text(
            &mut canvas,
            14.0,
            300.0,
            y,
            &prep,
            outcome_color(r.prep.as_deref().unwrap_or("")),
            true,
        );
        let castle = outcome_label(&r.castle);
        draw_text(
            &mut canvas,
            14.0,
            400.0,
            y,
            &castle,
            outcome_color(r.castle.as_deref().unwrap_or("")),
            true,
        );
        let king = format!(
            "{}{}",
            r.king_tag
                .as_ref()
                .map(|t| format!("[{t}] "))
                .unwrap_or_default(),
            r.king_name.as_deref().unwrap_or("—")
        );
        draw_text(
            &mut canvas,
            14.0,
            510.0,
            y - 4.0,
            &fit_str(14.0, &king, 320.0),
            TEXT,
            true,
        );
        if let Some(role) = &r.king_role {
            draw_text(
                &mut canvas,
                11.0,
                510.0,
                y + 12.0,
                role,
                if r.high_king { ORANGE } else { MUTED },
                false,
            );
        }
    }

    footer(
        &mut canvas,
        h,
        &data
            .as_of
            .as_ref()
            .map(|s| format!("Matchup history · {s}"))
            .unwrap_or_default(),
    );
    encode_png(&canvas)
}

// --------------------------------------------------------------- prep points

#[derive(Debug, Clone, Default)]
pub struct PrepDay {
    pub day: i64,
    pub home: f64,
    pub away: f64,
}

#[derive(Debug, Clone, Default)]
pub struct PrepPointsCard {
    pub us_kid: i64,
    pub them_kid: Option<i64>,
    pub prep_us: Option<f64>,
    pub prep_them: Option<f64>,
    pub days: Vec<PrepDay>,
    pub as_of: Option<String>,
}

pub fn render_prep_points_card(data: &PrepPointsCard) -> anyhow::Result<Vec<u8>> {
    let rows_n = data.days.len().max(1);
    let h = (170.0 + rows_n as f32 * 62.0 + 110.0).max(520.0).ceil() as u32;
    let mut canvas = base_card(W, h)?;

    draw_text(
        &mut canvas,
        13.0,
        50.0,
        52.0,
        "PREPARATION RANKINGS",
        LABEL,
        true,
    );
    let them = data
        .them_kid
        .map(|k| k.to_string())
        .unwrap_or_else(|| "?".into());
    draw_text(
        &mut canvas,
        30.0,
        50.0,
        92.0,
        &format!("State {} vs {them}", data.us_kid),
        TEXT,
        true,
    );

    let total_us = data
        .prep_us
        .unwrap_or_else(|| data.days.iter().map(|d| d.home).sum());
    let total_them = data
        .prep_them
        .unwrap_or_else(|| data.days.iter().map(|d| d.away).sum());
    let us_win = total_us >= total_them;

    // Overall box
    let box_x = W as f32 - 420.0;
    let box_y = 120.0f32;
    let box_w = 370.0f32;
    let box_h = 170.0f32;
    fill_round_rect(
        &mut canvas,
        box_x,
        box_y,
        box_w,
        box_h,
        18.0,
        Color::from_rgba8(120, 170, 230, 20),
    );
    stroke_round_rect(
        &mut canvas,
        box_x,
        box_y,
        box_w,
        box_h,
        18.0,
        hex(EDGE),
        1.4,
        None,
    );
    draw_text(
        &mut canvas,
        13.0,
        box_x + 24.0,
        box_y + 30.0,
        "Overall Preparation Score",
        MUTED,
        true,
    );
    draw_text(
        &mut canvas,
        34.0,
        box_x + 24.0,
        box_y + 78.0,
        &fmt(Some(total_us as i64)),
        if us_win { NUM } else { TEXT },
        true,
    );
    draw_text(
        &mut canvas,
        18.0,
        box_x + 24.0,
        box_y + 104.0,
        if us_win { "(W)" } else { "(L)" },
        if us_win { GREEN } else { RED },
        true,
    );
    let mid = box_x + box_w / 2.0;
    let us_s = data.us_kid.to_string();
    let them_s = them.clone();
    draw_text(
        &mut canvas,
        18.0,
        mid - 60.0,
        box_y + 138.0,
        &us_s,
        TEXT,
        true,
    );
    draw_text(
        &mut canvas,
        18.0,
        mid - 8.0,
        box_y + 138.0,
        "vs",
        MUTED,
        false,
    );
    draw_text(
        &mut canvas,
        18.0,
        mid + 24.0,
        box_y + 138.0,
        &them_s,
        TEXT,
        true,
    );
    let them_str = fmt(Some(total_them as i64));
    let tw = text_width(34.0, &them_str);
    draw_text(
        &mut canvas,
        34.0,
        box_x + box_w - 24.0 - tw,
        box_y + 78.0,
        &them_str,
        if !us_win { RED } else { TEXT },
        true,
    );
    let wl = if !us_win { "(W)" } else { "(L)" };
    let wlw = text_width(18.0, wl);
    draw_text(
        &mut canvas,
        18.0,
        box_x + box_w - 24.0 - wlw,
        box_y + 104.0,
        wl,
        if !us_win { GREEN } else { RED },
        true,
    );

    // Daily bars
    let left_x = 50.0f32;
    let chart_w = box_x - 90.0;
    let mut y = 140.0f32;
    if data.days.is_empty() {
        draw_text(
            &mut canvas,
            16.0,
            left_x,
            y + 10.0,
            "No daily prep breakdown available.",
            MUTED,
            false,
        );
    } else {
        let max_v = data
            .days
            .iter()
            .map(|d| d.home.max(d.away))
            .fold(1.0f64, f64::max);
        for d in &data.days {
            draw_text(
                &mut canvas,
                14.0,
                left_x,
                y,
                &format!("Day {}", d.day),
                MUTED,
                true,
            );
            // home bar
            let hw = (chart_w * 0.45) * (d.home / max_v) as f32;
            fill_round_rect(
                &mut canvas,
                left_x,
                y + 12.0,
                chart_w * 0.45,
                10.0,
                4.0,
                Color::from_rgba8(255, 255, 255, 26),
            );
            if hw > 0.0 {
                fill_round_rect(
                    &mut canvas,
                    left_x,
                    y + 12.0,
                    hw.max(4.0),
                    10.0,
                    4.0,
                    hex(ACCENT),
                );
            }
            draw_text(
                &mut canvas,
                13.0,
                left_x + chart_w * 0.45 + 8.0,
                y + 22.0,
                &fmt(Some(d.home as i64)),
                NUM,
                false,
            );
            // away bar
            let aw = (chart_w * 0.45) * (d.away / max_v) as f32;
            fill_round_rect(
                &mut canvas,
                left_x,
                y + 30.0,
                chart_w * 0.45,
                10.0,
                4.0,
                Color::from_rgba8(255, 255, 255, 26),
            );
            if aw > 0.0 {
                fill_round_rect(
                    &mut canvas,
                    left_x,
                    y + 30.0,
                    aw.max(4.0),
                    10.0,
                    4.0,
                    Color::from_rgba8(255, 107, 122, 255),
                );
            }
            draw_text(
                &mut canvas,
                13.0,
                left_x + chart_w * 0.45 + 8.0,
                y + 40.0,
                &fmt(Some(d.away as i64)),
                RED,
                false,
            );
            y += 62.0;
        }
    }

    footer(
        &mut canvas,
        h,
        &data
            .as_of
            .as_ref()
            .map(|s| format!("Prep · {s}"))
            .unwrap_or_default(),
    );
    encode_png(&canvas)
}

// --------------------------------------------------------------- appointments

#[derive(Debug, Clone, Default)]
pub struct AppointmentSeat {
    pub role: String,
    pub vacant: bool,
    pub name: Option<String>,
    pub alliance_tag: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AppointmentsCard {
    pub state_id: i64,
    pub ministers: Vec<AppointmentSeat>,
    pub offenders: Vec<AppointmentSeat>,
    pub as_of: Option<String>,
}

pub fn render_appointments_card(data: &AppointmentsCard) -> anyhow::Result<Vec<u8>> {
    let h = 620u32;
    let mut canvas = base_card(W, h)?;

    draw_text(&mut canvas, 13.0, 50.0, 52.0, "APPOINTMENTS", LABEL, true);
    draw_text(
        &mut canvas,
        30.0,
        50.0,
        92.0,
        &format!("State {}", data.state_id),
        TEXT,
        true,
    );

    let cols = 3usize;
    let cell_w = 300.0f32;
    let cell_h = 140.0f32;
    let gap = 16.0f32;

    let draw_grid =
        |canvas: &mut Pixmap, title: &str, count: &str, seats: &[AppointmentSeat], start_y: f32| {
            draw_text(canvas, 12.0, 50.0, start_y, title, LABEL, true);
            let cw = text_width(12.0, count);
            draw_text(
                canvas,
                12.0,
                W as f32 - 50.0 - cw,
                start_y,
                count,
                MUTED,
                false,
            );
            let y0 = start_y + 18.0;
            let mut list: Vec<AppointmentSeat> = seats.to_vec();
            while list.len() < 6 {
                list.push(AppointmentSeat {
                    role: "—".into(),
                    vacant: true,
                    ..Default::default()
                });
            }
            for (i, seat) in list.iter().take(6).enumerate() {
                let col = i % cols;
                let row = i / cols;
                let x = 50.0 + col as f32 * (cell_w + gap);
                let y = y0 + row as f32 * (cell_h + gap);
                fill_round_rect(
                    canvas,
                    x,
                    y,
                    cell_w,
                    cell_h,
                    14.0,
                    Color::from_rgba8(120, 170, 230, 15),
                );
                stroke_round_rect(canvas, x, y, cell_w, cell_h, 14.0, hex(EDGE), 1.2, None);
                let role = seat.role.to_uppercase();
                let rw = text_width(10.0, &role);
                draw_text(
                    canvas,
                    10.0,
                    x + cell_w / 2.0 - rw / 2.0,
                    y + 18.0,
                    &fit_str(10.0, &role, cell_w - 12.0),
                    MUTED,
                    true,
                );
                if seat.vacant || seat.name.is_none() {
                    super::fill_circle(
                        canvas,
                        x + cell_w / 2.0,
                        y + 62.0,
                        22.0,
                        Color::from_rgba8(255, 255, 255, 20),
                    );
                    let vw = text_width(13.0, "Vacant");
                    draw_text(
                        canvas,
                        13.0,
                        x + cell_w / 2.0 - vw / 2.0,
                        y + 120.0,
                        "Vacant",
                        MUTED,
                        false,
                    );
                } else {
                    super::fill_circle(canvas, x + cell_w / 2.0, y + 64.0, 28.0, hex(BOX));
                    super::stroke_circle(canvas, x + cell_w / 2.0, y + 64.0, 28.0, hex(EDGE), 2.0);
                    let tag = seat
                        .alliance_tag
                        .as_ref()
                        .map(|t| format!("[{t}] "))
                        .unwrap_or_default();
                    let name = format!("{tag}{}", seat.name.as_deref().unwrap_or("Unknown"));
                    let nw = text_width(12.0, &fit_str(12.0, &name, cell_w - 12.0));
                    draw_text(
                        canvas,
                        12.0,
                        x + cell_w / 2.0 - nw / 2.0,
                        y + 118.0,
                        &fit_str(12.0, &name, cell_w - 12.0),
                        TEXT,
                        true,
                    );
                }
            }
            y0 + 2.0 * (cell_h + gap)
        };

    let filled_m = data.ministers.iter().filter(|s| !s.vacant).count();
    let filled_o = data.offenders.iter().filter(|s| !s.vacant).count();
    let y = draw_grid(
        &mut canvas,
        "MINISTERS",
        &format!("{filled_m}/{}", data.ministers.len().max(6)),
        &data.ministers,
        115.0,
    );
    let _ = draw_grid(
        &mut canvas,
        "OFFENDERS",
        &format!("{filled_o}"),
        &data.offenders,
        y + 28.0,
    );

    footer(
        &mut canvas,
        h,
        &data
            .as_of
            .as_ref()
            .map(|s| format!("Appointments · {s}"))
            .unwrap_or_default(),
    );
    encode_png(&canvas)
}

// --------------------------------------------------------------- maps (iso simplified)

#[derive(Debug, Clone, Default)]
pub struct MapCity {
    pub x: f64,
    pub y: f64,
    pub alliance_id: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MapAlliance {
    pub aid: String,
    pub tag: Option<String>,
    pub color_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MapCard {
    pub title: String,
    pub subtitle: String,
    pub cities: Vec<MapCity>,
    pub alliances: Vec<MapAlliance>,
    pub highlight: Option<(f64, f64)>, // player/alliance focus
    pub focus_alliance_id: Option<String>,
    pub as_of: Option<String>,
}

const MAP_CENTER: f64 = 600.0;
const FERTILE_R: f64 = 150.0;
const PLAINS_R: f64 = 300.0;
const VIEW_R: f64 = 220.0;

const PALETTE: &[&str] = &[
    "#2dd4bf", "#f59e0b", "#34d399", "#c084fc", "#f87171", "#a78bfa", "#4ade80", "#f472b6",
    "#38bdf8", "#fb923c", "#e0b84a", "#67e8f9", "#818cf8", "#fb7185", "#86efac", "#e07a4a",
];

fn world_to_screen(wx: f64, wy: f64, scale: f32, cx: f32, cy: f32) -> (f32, f32) {
    let dx = (wx - MAP_CENTER) as f32;
    let dy = (wy - MAP_CENTER) as f32;
    ((dx - dy) * scale + cx, -(dx + dy) * scale + cy)
}

fn fill_zone(
    canvas: &mut Pixmap,
    r: f64,
    scale: f32,
    cx: f32,
    cy: f32,
    fill: Color,
    stroke: Option<Color>,
) {
    let c0 = world_to_screen(MAP_CENTER - r, MAP_CENTER - r, scale, cx, cy);
    let c1 = world_to_screen(MAP_CENTER + r, MAP_CENTER - r, scale, cx, cy);
    let c2 = world_to_screen(MAP_CENTER + r, MAP_CENTER + r, scale, cx, cy);
    let c3 = world_to_screen(MAP_CENTER - r, MAP_CENTER + r, scale, cx, cy);
    let mut pb = PathBuilder::new();
    pb.move_to(c0.0, c0.1);
    pb.line_to(c1.0, c1.1);
    pb.line_to(c2.0, c2.1);
    pb.line_to(c3.0, c3.1);
    pb.close();
    if let Some(path) = pb.finish() {
        canvas.fill_path(
            &path,
            &solid_paint(fill),
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
        if let Some(sc) = stroke {
            let mut st = tiny_skia::Stroke::default();
            st.width = 1.2;
            canvas.stroke_path(
                &path,
                &solid_paint(sc),
                &st,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
}

pub fn render_map_card(data: &MapCard) -> anyhow::Result<Vec<u8>> {
    const H: u32 = 640;
    let mut canvas = base_card(W, H)?;

    draw_text(&mut canvas, 13.0, 50.0, 52.0, "STATE MAP", LABEL, true);
    draw_text(&mut canvas, 28.0, 50.0, 92.0, &data.title, TEXT, true);
    draw_text(&mut canvas, 14.0, 50.0, 118.0, &data.subtitle, MUTED, false);

    let mx = 40.0f32;
    let my = 135.0f32;
    let mw = W as f32 - 80.0;
    let mh = 420.0f32;

    // Plot background
    fill_round_rect(&mut canvas, mx, my, mw, mh, 16.0, hex("#2a323c"));

    let half = mw.min(mh) * 0.48;
    let scale = half / (VIEW_R as f32 * std::f32::consts::SQRT_2);
    let cx = mx + mw / 2.0;
    let cy = my + mh / 2.0 + 6.0;

    fill_zone(
        &mut canvas,
        PLAINS_R,
        scale,
        cx,
        cy,
        hex("#2f6a45"),
        Some(Color::from_rgba8(200, 220, 160, 46)),
    );
    fill_zone(
        &mut canvas,
        FERTILE_R,
        scale,
        cx,
        cy,
        hex("#1a4a32"),
        Some(Color::from_rgba8(120, 190, 140, 71)),
    );
    fill_zone(
        &mut canvas,
        48.0,
        scale,
        cx,
        cy,
        hex("#0f3324"),
        Some(Color::from_rgba8(80, 160, 100, 64)),
    );

    // Capital
    let (cap_x, cap_y) = world_to_screen(MAP_CENTER, MAP_CENTER, scale, cx, cy);
    super::fill_circle(&mut canvas, cap_x, cap_y, 4.0, hex("#e8c547"));

    // Color map by alliance
    let mut color_by: std::collections::HashMap<String, Color> = std::collections::HashMap::new();
    for (i, a) in data.alliances.iter().take(48).enumerate() {
        color_by.insert(a.aid.clone(), hex(PALETTE[i % PALETTE.len()]));
    }

    for c in &data.cities {
        let dx = (c.x - MAP_CENTER).abs();
        let dy = (c.y - MAP_CENTER).abs();
        if dx.max(dy) > VIEW_R + 40.0 {
            continue;
        }
        let (sx, sy) = world_to_screen(c.x, c.y, scale, cx, cy);
        if sx < mx || sy < my || sx > mx + mw || sy > my + mh {
            continue;
        }
        let color = c
            .alliance_id
            .as_ref()
            .and_then(|id| color_by.get(id).copied())
            .unwrap_or_else(|| Color::from_rgba8(180, 200, 220, 200));
        let focused = data
            .focus_alliance_id
            .as_ref()
            .is_some_and(|id| c.alliance_id.as_ref().is_some_and(|city_id| city_id == id));
        let color = if data.focus_alliance_id.is_some() && !focused {
            Color::from_rgba8(150, 160, 175, 65)
        } else if focused {
            hex("#f5c542")
        } else {
            color
        };
        super::fill_circle(&mut canvas, sx, sy, if focused { 4.0 } else { 3.0 }, color);
    }

    // Highlight focus (player / alliance HQ)
    if let Some((hx, hy)) = data.highlight {
        let (sx, sy) = world_to_screen(hx, hy, scale, cx, cy);
        super::stroke_circle(&mut canvas, sx, sy, 10.0, hex("#f5c542"), 2.0);
        super::fill_circle(&mut canvas, sx, sy, 4.0, hex("#f5c542"));
    }

    // Legend (top alliances)
    let mut ly = my + mh + 12.0;
    draw_text(&mut canvas, 11.0, mx, ly, "ALLIANCES", MUTED, true);
    ly += 4.0;
    for (i, a) in data.alliances.iter().take(8).enumerate() {
        let x = mx + (i as f32 % 4.0) * 240.0;
        let y = ly + 18.0 + (i as f32 / 4.0).floor() * 18.0;
        let color = hex(PALETTE[i % PALETTE.len()]);
        super::fill_circle(&mut canvas, x + 6.0, y - 4.0, 5.0, color);
        let tag = a.tag.clone().unwrap_or_else(|| a.aid.clone());
        draw_text(
            &mut canvas,
            12.0,
            x + 16.0,
            y,
            &fit_str(12.0, &tag, 200.0),
            TEXT,
            false,
        );
    }

    footer(
        &mut canvas,
        H,
        &data
            .as_of
            .as_ref()
            .map(|s| format!("Map · {s}"))
            .unwrap_or_default(),
    );
    encode_png(&canvas)
}
