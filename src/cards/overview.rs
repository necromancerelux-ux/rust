//! Overview-style cards: player, standings, alliance and state.
//! Ported from `src/lookup/services/cardRender.js`.

use tiny_skia::{Color, Pixmap};

use super::{
    draw_image_stretched, draw_text, encode_png, fetch_image, fill_round_rect, hex, text_width,
};

pub fn fmt(n: Option<i64>) -> String {
    match n {
        None => "—".to_string(),
        Some(n) => {
            let absolute = n.unsigned_abs();
            if absolute >= 1_000_000_000 {
                format!("{:.2}B", n as f64 / 1e9)
            } else if absolute >= 1_000_000 {
                format!("{:.2}M", n as f64 / 1e6)
            } else if absolute >= 1_000 {
                format!("{:.1}K", n as f64 / 1e3)
            } else {
                n.to_string()
            }
        }
    }
}

// OmniCore card palette — dark navy glass, purple labels, cyan numbers.
// Visual language ported from the legacy lookup image cards (no third-party branding).
const BG: &str = "#0a1626";
const PANEL: &str = "#10294a";
const BOX: &str = "#0d1f38";
const ACCENT: &str = "#6cc4ff";
const EDGE: &str = "#27507f";
const EDGE2: &str = "#14304f";
const TEXT: (u8, u8, u8) = (238, 244, 251);
const TITLE: (u8, u8, u8) = (238, 244, 251);
const MUTED: (u8, u8, u8) = (143, 165, 191);
const SLATE: (u8, u8, u8) = (93, 115, 144);
const LABEL: (u8, u8, u8) = (183, 140, 255);
const NUM: (u8, u8, u8) = (108, 196, 255);

pub(crate) fn base_card(width: u32, height: u32) -> anyhow::Result<Pixmap> {
    let mut canvas =
        Pixmap::new(width, height).ok_or_else(|| anyhow::anyhow!("pixmap alloc failed"))?;
    // Outer glass fill
    canvas.fill(hex(BG));
    fill_round_rect(
        &mut canvas,
        0.0,
        0.0,
        width as f32,
        height as f32,
        26.0,
        hex(BG),
    );
    // Inner panel
    fill_round_rect(
        &mut canvas,
        14.0,
        14.0,
        width as f32 - 28.0,
        height as f32 - 28.0,
        18.0,
        hex(PANEL),
    );
    // Soft edge ring (approximate the double stroke from the JS canvas cards)
    if let Some(path) =
        super::rounded_rect_path(14.0, 14.0, width as f32 - 28.0, height as f32 - 28.0, 18.0)
    {
        let mut stroke = tiny_skia::Stroke::default();
        stroke.width = 2.0;
        canvas.stroke_path(
            &path,
            &super::solid_paint(hex(EDGE)),
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }
    if let Some(path) =
        super::rounded_rect_path(20.0, 20.0, width as f32 - 40.0, height as f32 - 40.0, 14.0)
    {
        let mut stroke = tiny_skia::Stroke::default();
        stroke.width = 1.0;
        canvas.stroke_path(
            &path,
            &super::solid_paint(hex(EDGE2)),
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }
    Ok(canvas)
}

fn accent_bar(canvas: &mut Pixmap, height: u32) {
    let Some(rect) = tiny_skia::Rect::from_xywh(12.0, 12.0, 4.0, height as f32 - 24.0) else {
        return;
    };
    canvas.fill_rect(
        rect,
        &super::solid_paint(hex(ACCENT)),
        tiny_skia::Transform::identity(),
        None,
    );
}

fn stat_box(
    canvas: &mut Pixmap,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    label: &str,
    value: &str,
) {
    fill_round_rect(canvas, x, y, width, height, 10.0, hex(BOX));
    // Purple section label + cyan headline numbers (card UI style)
    draw_text(canvas, 11.0, x + 14.0, y + 22.0, label, LABEL, false);
    draw_text(canvas, 20.0, x + 14.0, y + height - 16.0, value, NUM, true);
}

// ------------------------------------------------------------ player

#[derive(Debug, Clone, Default)]
pub struct PlayerOverview {
    pub name: String,
    pub alliance_tag: Option<String>,
    pub alliance_name: Option<String>,
    pub alliance_rank_label: Option<String>,
    pub alliance_members: Option<i64>,
    pub alliance_flag_url: Option<String>,
    pub role: Option<String>,
    pub fid: Option<String>,
    pub kid: Option<i64>,
    pub furnace: Option<String>,
    pub level_label: Option<String>,
    pub avatar_url: Option<String>,
    pub power: Option<i64>,
    pub kills: Option<i64>,
    pub vip: Option<String>,
    // INTEL grid extras
    pub power_rank: Option<i64>,
    pub kills_rank: Option<i64>,
    pub migrant_score: Option<i64>,
    pub migrant_rank: Option<i64>,
    pub lab_progress: Option<i64>,
    pub lab_rank: Option<i64>,
    pub shield_label: Option<String>,
    pub shield_active: bool,
    pub burn_label: Option<String>,
    pub burn_active: bool,
    pub location: Option<String>,
    pub chief_id: Option<String>,
    pub last_power_change: Option<String>,
    pub last_updated: Option<String>,
    pub crowns: Option<i64>,
    pub troop_lines: Vec<String>,  // e.g. "T10 · FC3"
    pub troop_labels: Vec<String>, // e.g. "Infantry"
    pub redeem_note: Option<String>,
    pub as_of: Option<String>,
}

fn avatar_url(url: &str) -> String {
    if url.to_ascii_lowercase().starts_with("http://")
        || url.to_ascii_lowercase().starts_with("https://")
    {
        url.to_string()
    } else if url.starts_with('/') {
        format!("https://www.whiteoutstats.com{url}")
    } else {
        format!("https://www.whiteoutstats.com/{url}")
    }
}

pub(crate) fn fit_str(size: f32, text: &str, max_w: f32) -> String {
    if text_width(size, text) <= max_w {
        return text.to_string();
    }
    let mut s = text.to_string();
    while s.len() > 1 && text_width(size, &format!("{s}…")) > max_w {
        s.pop();
        while s.ends_with(|c: char| !c.is_ascii()) {
            s.pop();
        }
    }
    format!("{s}…")
}

fn rank_in_state(rank: Option<i64>, kid: Option<i64>) -> String {
    match rank {
        None => "—".into(),
        Some(r) => match kid {
            Some(k) => format!("#{r} in State {k}"),
            None => format!("#{r}"),
        },
    }
}

fn int_fmt(n: Option<i64>) -> String {
    match n {
        None => "—".into(),
        Some(n) => n.to_string(),
    }
}

/// Dense INTEL-style player profile card (navy glass, 4-col fact grid).
pub async fn render_player_overview_card(data: &PlayerOverview) -> anyhow::Result<Vec<u8>> {
    const W: u32 = 1060;

    let furnace_label = data
        .level_label
        .clone()
        .or_else(|| data.furnace.clone())
        .unwrap_or_else(|| "—".into());

    let mut facts: Vec<(&str, String, (u8, u8, u8))> = vec![
        ("Power", fmt(data.power), NUM),
        ("Furnace", furnace_label, TEXT),
        ("Kills", fmt(data.kills), TEXT),
        ("Power Rank", rank_in_state(data.power_rank, data.kid), NUM),
        ("Kills Rank", rank_in_state(data.kills_rank, data.kid), TEXT),
        ("State Transfer Score", int_fmt(data.migrant_score), TEXT),
        (
            "State Transfer Rank",
            rank_in_state(data.migrant_rank, data.kid),
            TEXT,
        ),
        ("Labyrinth Progress", int_fmt(data.lab_progress), TEXT),
        (
            "Labyrinth Rank",
            rank_in_state(data.lab_rank, data.kid),
            TEXT,
        ),
        (
            "Shield",
            data.shield_label
                .clone()
                .unwrap_or_else(|| "No shield".into()),
            if data.shield_active { NUM } else { SLATE },
        ),
        (
            "Burn",
            data.burn_label
                .clone()
                .unwrap_or_else(|| "Not burning".into()),
            if data.burn_active {
                (255, 93, 93)
            } else {
                SLATE
            },
        ),
        (
            "Location",
            data.location.clone().unwrap_or_else(|| "—".into()),
            TEXT,
        ),
        (
            "Chief ID",
            data.chief_id
                .clone()
                .or_else(|| data.fid.clone())
                .unwrap_or_else(|| "—".into()),
            TEXT,
        ),
        (
            "State",
            data.kid
                .map(|k| format!("State {k}"))
                .unwrap_or_else(|| "—".into()),
            NUM,
        ),
        (
            "Alliance rank",
            data.role
                .clone()
                .or_else(|| data.alliance_rank_label.clone())
                .unwrap_or_else(|| "—".into()),
            TEXT,
        ),
        (
            "Last power change",
            data.last_power_change.clone().unwrap_or_else(|| "—".into()),
            SLATE,
        ),
        (
            "Last updated",
            data.last_updated
                .clone()
                .or_else(|| data.as_of.clone())
                .unwrap_or_else(|| "—".into()),
            SLATE,
        ),
    ];

    for (i, line) in data.troop_lines.iter().enumerate() {
        let label = data
            .troop_labels
            .get(i)
            .map(|s| format!("Troop · {s}"))
            .unwrap_or_else(|| format!("Troop · {}", i + 1));
        // Leak-free: use owned in a separate vec later — store as static-ish via facts owned
        let _ = (label, line);
    }

    // Owned troop facts appended after
    let troop_owned: Vec<(String, String)> = data
        .troop_labels
        .iter()
        .zip(data.troop_lines.iter())
        .map(|(l, v)| (format!("Troop · {l}"), v.clone()))
        .collect();

    let cols = 4usize;
    let base_fact_count = facts.len() + troop_owned.len();
    let rows = (base_fact_count + cols - 1) / cols;
    let cell_h = 58.0f32;
    let grid_top = 250.0f32;
    let h = (grid_top + rows as f32 * cell_h + 70.0).ceil() as u32;
    let mut canvas = base_card(W, h)?;

    // Avatar
    let ax = 48.0f32;
    let ay = 42.0f32;
    let size = 100.0f32;
    let radius = size / 2.0;
    let avatar = match &data.avatar_url {
        Some(url) => fetch_image(&avatar_url(url), "https://www.whiteoutstats.com/").await,
        None => None,
    };
    match &avatar {
        Some(image) => {
            const S: u32 = 100;
            let mut tile =
                Pixmap::new(S, S).ok_or_else(|| anyhow::anyhow!("pixmap alloc failed"))?;
            draw_image_stretched(&mut tile, image, 0.0, 0.0, S as f32, S as f32);
            let mut mask = Pixmap::new(S, S).unwrap();
            super::fill_circle(
                &mut mask,
                S as f32 / 2.0,
                S as f32 / 2.0,
                S as f32 / 2.0,
                Color::from_rgba8(255, 255, 255, 255),
            );
            let mut masked = tile.clone();
            let tile_data = tile.data();
            let mask_data = mask.data();
            let out = masked.data_mut();
            for i in 0..(S * S) as usize {
                let a = mask_data[i * 4 + 3] as u16;
                out[i * 4 + 3] = ((tile_data[i * 4 + 3] as u16 * a) / 255) as u8;
                out[i * 4] = ((tile_data[i * 4] as u16 * a) / 255) as u8;
                out[i * 4 + 1] = ((tile_data[i * 4 + 1] as u16 * a) / 255) as u8;
                out[i * 4 + 2] = ((tile_data[i * 4 + 2] as u16 * a) / 255) as u8;
            }
            super::draw_image_stretched(&mut canvas, &masked, ax, ay, size, size);
        }
        None => {
            super::fill_circle(
                &mut canvas,
                ax + radius,
                ay + radius,
                radius,
                hex("#1c3556"),
            );
        }
    }
    super::stroke_circle(
        &mut canvas,
        ax + radius,
        ay + radius,
        radius + 1.0,
        hex(EDGE),
        2.0,
    );

    // Name + identity
    draw_text(
        &mut canvas,
        34.0,
        172.0,
        82.0,
        &fit_str(34.0, &data.name, 700.0),
        TITLE,
        true,
    );
    draw_text(
        &mut canvas,
        15.0,
        172.0,
        108.0,
        &format!("ID {}", data.fid.as_deref().unwrap_or("—")),
        MUTED,
        false,
    );
    if let Some(kid) = data.kid {
        draw_text(
            &mut canvas,
            15.0,
            172.0,
            130.0,
            &format!("State #{kid}"),
            NUM,
            true,
        );
    }
    if data.crowns.unwrap_or(0) > 0 {
        draw_text(
            &mut canvas,
            40.0,
            (W as f32) - 90.0,
            90.0,
            "♛",
            (245, 197, 66),
            true,
        );
    }

    // Alliance strip
    let strip_y = 156.0f32;
    fill_round_rect(
        &mut canvas,
        42.0,
        strip_y,
        W as f32 - 84.0,
        52.0,
        12.0,
        Color::from_rgba8(120, 170, 230, 20),
    );
    super::stroke_round_rect(
        &mut canvas,
        42.0,
        strip_y,
        W as f32 - 84.0,
        52.0,
        12.0,
        hex(EDGE),
        1.2,
        None,
    );

    if data.alliance_tag.is_some() || data.alliance_name.is_some() {
        let tag = data
            .alliance_tag
            .as_ref()
            .map(|t| format!("[{t}]"))
            .unwrap_or_default();
        let name = data.alliance_name.clone().unwrap_or_default();
        let full = format!("{tag} {name}").trim().to_string();
        draw_text(
            &mut canvas,
            18.0,
            100.0,
            strip_y + 33.0,
            &fit_str(18.0, &full, 520.0),
            TEXT,
            true,
        );
        let mut meta = Vec::new();
        if let Some(r) = &data.role {
            meta.push(r.clone());
        }
        if let Some(m) = data.alliance_members {
            meta.push(format!("{m} members"));
        }
        if let Some(k) = data.kid {
            meta.push(format!("State {k}"));
        }
        if !meta.is_empty() {
            let meta_x = 100.0 + text_width(18.0, &fit_str(18.0, &full, 520.0)) + 16.0;
            draw_text(
                &mut canvas,
                13.0,
                meta_x,
                strip_y + 33.0,
                &meta.join(" · "),
                MUTED,
                false,
            );
        }
    } else {
        draw_text(
            &mut canvas,
            15.0,
            60.0,
            strip_y + 32.0,
            "No alliance",
            MUTED,
            false,
        );
    }

    // INTEL section
    draw_text(&mut canvas, 13.0, 48.0, 236.0, "INTEL", LABEL, true);
    // horizontal rule
    if let Some(rect) = tiny_skia::Rect::from_xywh(42.0, 246.0, W as f32 - 84.0, 1.0) {
        canvas.fill_rect(
            rect,
            &super::solid_paint(Color::from_rgba8(130, 170, 220, 46)),
            tiny_skia::Transform::identity(),
            None,
        );
    }

    // Fact grid
    let grid_x = 48.0f32;
    let grid_w = W as f32 - 96.0;
    let cell_w = grid_w / cols as f32;

    let mut all_facts: Vec<(String, String, (u8, u8, u8))> = facts
        .drain(..)
        .map(|(l, v, c)| (l.to_string(), v, c))
        .collect();
    for (l, v) in troop_owned {
        all_facts.push((l, v, NUM));
    }

    for (i, (lab, val, color)) in all_facts.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = grid_x + col as f32 * cell_w;
        let y = grid_top + row as f32 * cell_h;
        if row % 2 == 0 {
            if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, cell_w - 8.0, cell_h - 6.0) {
                canvas.fill_rect(
                    rect,
                    &super::solid_paint(Color::from_rgba8(120, 170, 230, 18)),
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }
        draw_text(
            &mut canvas,
            12.0,
            x + 10.0,
            y + 20.0,
            &lab.to_uppercase(),
            MUTED,
            false,
        );
        draw_text(
            &mut canvas,
            18.0,
            x + 10.0,
            y + 44.0,
            &fit_str(18.0, val, cell_w - 24.0),
            *color,
            true,
        );
    }

    if let Some(note) = &data.redeem_note {
        draw_text(
            &mut canvas,
            13.0,
            48.0,
            h as f32 - 64.0,
            note,
            (240, 162, 74),
            false,
        );
    }

    // Footer
    draw_text(
        &mut canvas,
        13.0,
        42.0,
        h as f32 - 34.0,
        "OMNICORE",
        SLATE,
        true,
    );
    let right = data
        .as_of
        .as_ref()
        .map(|s| format!("Profile as of {s}"))
        .unwrap_or_default();
    if !right.is_empty() {
        let tw = text_width(13.0, &right);
        draw_text(
            &mut canvas,
            13.0,
            W as f32 - 42.0 - tw,
            h as f32 - 34.0,
            &right,
            SLATE,
            false,
        );
    }

    encode_png(&canvas)
}

// ------------------------------------------------------------ standings

#[derive(Debug, Clone)]
pub struct StandingRow {
    pub board: String,
    pub score: String,
    pub rank: String,
}

pub fn render_standings_card(
    name: &str,
    kid: Option<i64>,
    rows: &[StandingRow],
) -> anyhow::Result<Vec<u8>> {
    const W: u32 = 1060;
    const ROW_H: u32 = 28;
    let slice = &rows[..rows.len().min(12)];
    let height = 90 + slice.len().max(1) as u32 * ROW_H + 24;
    let mut canvas = base_card(W, height)?;

    let title = format!(
        "{}  ·  State {}",
        name,
        kid.map(|k| k.to_string()).unwrap_or_else(|| "—".into())
    );
    draw_text(&mut canvas, 18.0, 28.0, 42.0, &title, TITLE, true);
    draw_text(&mut canvas, 12.0, 28.0, 72.0, "BOARD", MUTED, false);
    draw_text(&mut canvas, 12.0, 320.0, 72.0, "SCORE", MUTED, false);
    draw_text(&mut canvas, 12.0, 480.0, 72.0, "RANK", MUTED, false);

    if slice.is_empty() {
        draw_text(
            &mut canvas,
            14.0,
            28.0,
            110.0,
            "No standings available.",
            SLATE,
            false,
        );
    } else {
        for (index, row) in slice.iter().enumerate() {
            let y = 96 + index as u32 * ROW_H;
            if index % 2 == 1 {
                fill_round_rect(
                    &mut canvas,
                    20.0,
                    (y - 18) as f32,
                    W as f32 - 40.0,
                    ROW_H as f32,
                    4.0,
                    hex(BOX),
                );
            }
            draw_text(
                &mut canvas,
                13.0,
                28.0,
                y as f32,
                &row.board.chars().take(28).collect::<String>(),
                TEXT,
                false,
            );
            draw_text(&mut canvas, 13.0, 320.0, y as f32, &row.score, TEXT, false);
            draw_text(
                &mut canvas,
                13.0,
                480.0,
                y as f32,
                &row.rank,
                (56, 189, 248),
                false,
            );
        }
    }

    encode_png(&canvas)
}

// ------------------------------------------------------------ alliance

#[derive(Debug, Clone, Default)]
pub struct AllianceOverview {
    pub tag: String,
    pub name: String,
    pub state_id: Option<i64>,
    pub leader: Option<String>,
    pub leader_role: Option<String>,
    pub power: Option<i64>,
    pub power_rank: Option<i64>,
    pub kills: Option<i64>,
    pub kills_rank: Option<i64>,
    pub members: Option<i64>,
    pub member_max: Option<i64>,
    pub rank: Option<i64>,
    pub level: Option<i64>,
    pub labyrinth: Option<i64>,
    pub lab_rank: Option<i64>,
    pub notice: Option<String>,
    pub roster: Vec<(String, Option<i64>, Option<String>)>, // name, power, role
}

pub fn render_alliance_overview_card(data: &AllianceOverview) -> anyhow::Result<Vec<u8>> {
    const W: u32 = 1060;
    let top_n = data.roster.len().min(6);
    let h = 400 + top_n as u32 * 50;
    let mut canvas = base_card(W, h)?;

    draw_text(&mut canvas, 13.0, 56.0, 62.0, "ALLIANCE", LABEL, true);
    let title = format!("[{}] {}", data.tag, data.name);
    draw_text(
        &mut canvas,
        40.0,
        56.0,
        108.0,
        &fit_str(40.0, &title, 700.0),
        TITLE,
        true,
    );

    let mut sub = Vec::new();
    if let Some(k) = data.state_id {
        sub.push(format!("State {k}"));
    }
    if let Some(lv) = data.level {
        sub.push(format!("Lv {lv}"));
    }
    if let Some(leader) = &data.leader {
        let role = data.leader_role.as_deref().unwrap_or("");
        sub.push(format!("{role} {leader}").trim().to_string());
    }
    draw_text(
        &mut canvas,
        18.0,
        56.0,
        140.0,
        &fit_str(18.0, &sub.join(" · "), 700.0),
        MUTED,
        false,
    );

    let member_label = match (data.members, data.member_max) {
        (Some(m), Some(max)) => format!("{m} / {max}"),
        (Some(m), None) => m.to_string(),
        _ => "—".into(),
    };
    let power_sub = data.power_rank.map(|r| format!("#{r}")).unwrap_or_default();
    let kills_sub = data.kills_rank.map(|r| format!("#{r}")).unwrap_or_default();
    let lab_val = data
        .labyrinth
        .map(|n| n.to_string())
        .unwrap_or_else(|| "—".into());
    let lab_sub = data.lab_rank.map(|r| format!("#{r} ranked"));

    let cols = [
        ("Power", fmt(data.power), Some(power_sub), NUM),
        ("Kills", fmt(data.kills), Some(kills_sub), TEXT),
        ("Members", member_label, Some("capacity".into()), TEXT),
        ("Labyrinth", lab_val, lab_sub, TEXT),
    ];
    let cw = (W as f32 - 112.0) / cols.len() as f32;
    for (i, (lab, val, sub, color)) in cols.iter().enumerate() {
        let x = 56.0 + i as f32 * cw;
        draw_text(
            &mut canvas,
            12.0,
            x,
            182.0,
            &lab.to_uppercase(),
            MUTED,
            false,
        );
        draw_text(&mut canvas, 28.0, x, 222.0, val, *color, true);
        if let Some(s) = sub {
            if !s.is_empty() {
                draw_text(&mut canvas, 13.0, x, 244.0, s, (79, 211, 124), false);
            }
        }
    }

    let mut y_cursor = 272.0f32;
    if let Some(blurb) = &data.notice {
        let t = blurb.chars().take(120).collect::<String>();
        draw_text(&mut canvas, 13.0, 56.0, y_cursor, &t, MUTED, false);
        y_cursor += 28.0;
    }

    // rule
    if let Some(rect) = tiny_skia::Rect::from_xywh(56.0, y_cursor, W as f32 - 112.0, 1.0) {
        canvas.fill_rect(
            rect,
            &super::solid_paint(Color::from_rgba8(130, 170, 220, 46)),
            tiny_skia::Transform::identity(),
            None,
        );
    }
    y_cursor += 24.0;
    draw_text(
        &mut canvas,
        12.0,
        56.0,
        y_cursor,
        "TOP MEMBERS",
        MUTED,
        false,
    );
    draw_text(
        &mut canvas,
        12.0,
        W as f32 - 180.0,
        y_cursor,
        "SHARE OF POWER",
        MUTED,
        false,
    );
    y_cursor += 16.0;

    let max_power = data
        .roster
        .iter()
        .filter_map(|(_, p, _)| *p)
        .max()
        .unwrap_or(1)
        .max(1);

    for (i, (name, power, role)) in data.roster.iter().take(6).enumerate() {
        let y = y_cursor + i as f32 * 50.0;
        if i % 2 == 0 {
            if let Some(rect) = tiny_skia::Rect::from_xywh(48.0, y - 2.0, W as f32 - 96.0, 46.0) {
                canvas.fill_rect(
                    rect,
                    &super::solid_paint(Color::from_rgba8(120, 170, 230, 18)),
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }
        // role badge
        fill_round_rect(&mut canvas, 60.0, y + 8.0, 40.0, 26.0, 7.0, hex(BOX));
        super::stroke_round_rect(
            &mut canvas,
            60.0,
            y + 8.0,
            40.0,
            26.0,
            7.0,
            hex(EDGE),
            1.5,
            None,
        );
        let role_s = role.as_deref().unwrap_or("R?");
        let rw = text_width(13.0, role_s);
        draw_text(
            &mut canvas,
            13.0,
            80.0 - rw / 2.0,
            y + 26.0,
            role_s,
            MUTED,
            true,
        );
        draw_text(
            &mut canvas,
            20.0,
            116.0,
            y + 29.0,
            &fit_str(20.0, name, 340.0),
            TEXT,
            true,
        );
        let pw = power.unwrap_or(0);
        draw_text(
            &mut canvas,
            16.0,
            W as f32 - 200.0,
            y + 29.0,
            &fmt(Some(pw)),
            NUM,
            true,
        );
        // bar
        let bar_w = 120.0f32 * (pw as f32 / max_power as f32);
        fill_round_rect(
            &mut canvas,
            W as f32 - 160.0,
            y + 14.0,
            120.0,
            8.0,
            4.0,
            Color::from_rgba8(255, 255, 255, 26),
        );
        if bar_w > 0.0 {
            fill_round_rect(
                &mut canvas,
                W as f32 - 160.0,
                y + 14.0,
                bar_w,
                8.0,
                4.0,
                hex(ACCENT),
            );
        }
    }

    draw_text(
        &mut canvas,
        13.0,
        42.0,
        h as f32 - 34.0,
        "OMNICORE",
        SLATE,
        true,
    );
    encode_png(&canvas)
}

// ------------------------------------------------------------ state

#[derive(Debug, Clone, Default)]
pub struct StateOverview {
    pub kid: i64,
    pub players: Option<i64>,
    pub alliances: Option<i64>,
    pub power: Option<i64>,
    pub avg_power: Option<i64>,
    pub top_power: Option<i64>,
    pub active_7d: Option<i64>,
    pub health_grade: Option<String>,
    pub health_blurb: Option<String>,
    pub health_components: Option<(String, String, String)>,
}

pub fn render_state_overview_card(data: &StateOverview) -> anyhow::Result<Vec<u8>> {
    const W: u32 = 1060;
    const H: u32 = 360;
    let mut canvas = base_card(W, H)?;

    draw_text(&mut canvas, 13.0, 56.0, 56.0, "STATE", LABEL, true);
    draw_text(
        &mut canvas,
        40.0,
        56.0,
        100.0,
        &format!("State {}", data.kid),
        TITLE,
        true,
    );
    let subtitle = format!(
        "{} tracked players  ·  {} alliances",
        data.players
            .map(|p| p.to_string())
            .unwrap_or_else(|| "—".into()),
        data.alliances
            .map(|a| a.to_string())
            .unwrap_or_else(|| "—".into())
    );
    draw_text(&mut canvas, 16.0, 56.0, 132.0, &subtitle, MUTED, false);

    if let Some(grade) = &data.health_grade {
        fill_round_rect(
            &mut canvas,
            W as f32 - 120.0,
            48.0,
            64.0,
            40.0,
            10.0,
            hex("#1d4ed8"),
        );
        let gw = text_width(20.0, grade);
        draw_text(
            &mut canvas,
            20.0,
            W as f32 - 120.0 + (64.0 - gw) / 2.0,
            76.0,
            grade,
            (255, 255, 255),
            true,
        );
    }

    let stats = [
        ("POWER", fmt(data.power)),
        ("AVG POWER", fmt(data.avg_power)),
        ("TOP POWER", fmt(data.top_power)),
        (
            "ACTIVE 7D",
            data.active_7d
                .map(|a| a.to_string())
                .unwrap_or_else(|| "—".into()),
        ),
    ];
    let cw = (W as f32 - 112.0) / 4.0;
    for (i, (lab, val)) in stats.iter().enumerate() {
        let x = 56.0 + i as f32 * cw;
        fill_round_rect(&mut canvas, x, 160.0, cw - 16.0, 80.0, 12.0, hex(BOX));
        draw_text(&mut canvas, 12.0, x + 14.0, 186.0, lab, LABEL, false);
        draw_text(&mut canvas, 26.0, x + 14.0, 222.0, val, NUM, true);
    }

    if let Some(blurb) = &data.health_blurb {
        draw_text(
            &mut canvas,
            14.0,
            56.0,
            280.0,
            &blurb.chars().take(100).collect::<String>(),
            MUTED,
            false,
        );
    }
    if let Some((power, activity, size)) = &data.health_components {
        let line = format!("Whales {power}   ·   Activity {activity}   ·   Size {size}");
        draw_text(&mut canvas, 13.0, 56.0, 308.0, &line, SLATE, false);
    }

    draw_text(
        &mut canvas,
        13.0,
        42.0,
        H as f32 - 34.0,
        "OMNICORE",
        SLATE,
        true,
    );
    encode_png(&canvas)
}

// ------------------------------------------------------------ state vs state

#[derive(Debug, Clone, Default)]
pub struct StateVsSide {
    pub kid: Option<i64>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct StateVsRow {
    pub label: String,
    pub us: Option<String>,
    pub them: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct StateVsCard {
    pub season: Option<i64>,
    pub us: StateVsSide,
    pub them: StateVsSide,
    pub rows: Vec<StateVsRow>,
}

pub fn render_state_vs_card(data: &StateVsCard) -> anyhow::Result<Vec<u8>> {
    const W: u32 = 1060;
    let row_h = 42.0f32;
    let h = (210.0 + data.rows.len() as f32 * row_h + 50.0).ceil() as u32;
    let mut canvas = base_card(W, h)?;

    draw_text(
        &mut canvas,
        13.0,
        50.0,
        52.0,
        "STATE VERSUS STATE (SVS)",
        LABEL,
        true,
    );
    if let Some(season) = data.season {
        let t = format!("Season {season}");
        let tw = text_width(13.0, &t);
        draw_text(
            &mut canvas,
            13.0,
            W as f32 - 50.0 - tw,
            52.0,
            &t,
            MUTED,
            true,
        );
    }

    let card_w = 280.0f32;
    let card_h = 110.0f32;
    let left_x = 80.0f32;
    let right_x = W as f32 - 80.0 - card_w;
    let card_y = 72.0f32;
    let mid_x = W as f32 / 2.0;

    let draw_side = |canvas: &mut Pixmap, x: f32, side: &StateVsSide| {
        fill_round_rect(
            canvas,
            x,
            card_y,
            card_w,
            card_h,
            16.0,
            Color::from_rgba8(120, 170, 230, 15),
        );
        super::stroke_round_rect(
            canvas,
            x,
            card_y,
            card_w,
            card_h,
            16.0,
            hex(EDGE),
            1.5,
            None,
        );
        let kid_s = side
            .kid
            .map(|k| format!("#{k}"))
            .unwrap_or_else(|| "—".into());
        let name = side
            .label
            .clone()
            .or_else(|| side.kid.map(|k| format!("State {k}")))
            .unwrap_or_else(|| "—".into());
        let kw = text_width(12.0, &kid_s);
        draw_text(
            canvas,
            12.0,
            x + card_w / 2.0 - kw / 2.0,
            card_y + 48.0,
            &kid_s,
            MUTED,
            true,
        );
        let nw = text_width(18.0, &name);
        draw_text(
            canvas,
            18.0,
            x + card_w / 2.0 - nw / 2.0,
            card_y + 80.0,
            &name,
            TEXT,
            true,
        );
    };
    draw_side(&mut canvas, left_x, &data.us);
    draw_side(&mut canvas, right_x, &data.them);

    // VS badge
    super::fill_circle(&mut canvas, mid_x, card_y + card_h / 2.0, 22.0, hex(EDGE2));
    super::stroke_circle(
        &mut canvas,
        mid_x,
        card_y + card_h / 2.0,
        22.0,
        hex(EDGE),
        2.0,
    );
    let vs_w = text_width(14.0, "VS");
    draw_text(
        &mut canvas,
        14.0,
        mid_x - vs_w / 2.0,
        card_y + card_h / 2.0 + 5.0,
        "VS",
        TEXT,
        true,
    );

    let snap_y = card_y + card_h + 28.0;
    for (i, row) in data.rows.iter().enumerate() {
        let y = snap_y + i as f32 * row_h;
        if i % 2 == 0 {
            if let Some(rect) = tiny_skia::Rect::from_xywh(48.0, y, W as f32 - 96.0, row_h - 4.0) {
                canvas.fill_rect(
                    rect,
                    &super::solid_paint(Color::from_rgba8(120, 170, 230, 15)),
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }
        draw_text(&mut canvas, 14.0, 64.0, y + 26.0, &row.label, MUTED, false);
        let us = row.us.as_deref().unwrap_or("—");
        let them = row.them.as_deref().unwrap_or("—");
        let us_w = text_width(16.0, us);
        draw_text(
            &mut canvas,
            16.0,
            mid_x - 40.0 - us_w,
            y + 26.0,
            us,
            NUM,
            true,
        );
        draw_text(&mut canvas, 16.0, mid_x + 40.0, y + 26.0, them, TEXT, true);
    }

    draw_text(
        &mut canvas,
        13.0,
        42.0,
        h as f32 - 34.0,
        "OMNICORE",
        SLATE,
        true,
    );
    encode_png(&canvas)
}
