//! Server-side PNG cards rendered from SVG.
//!
//! Cards are generated with `resvg` + `tiny-skia` and the bundled OpenSans font,
//! so no host font packages are required. Rendering is CPU-only and cheap
//! enough to run inline on an interaction (< 30 ms for a 900×420 card).

use anyhow::{Context as _, Result};
use resvg::usvg;

use crate::ui::panels::LookupPlayerView;
use crate::ui::theme::{self, colors};

/// Bundled so the binary needs no system fonts.
const OPEN_SANS: &[u8] = include_bytes!("../../assets/fonts/OpenSans.ttf");

fn options() -> usvg::Options<'static> {
    let mut options = usvg::Options {
        font_family: "Open Sans".to_string(),
        ..usvg::Options::default()
    };
    options.fontdb_mut().load_font_data(OPEN_SANS.to_vec());
    options
}

/// Rasterise an SVG document to PNG bytes.
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>> {
    let tree = usvg::Tree::from_str(svg, &options()).context("parsing card SVG")?;
    let size = tree.size().to_int_size();
    let (w, h) = (size.width(), size.height());
    let mut pixmap = tiny_skia::Pixmap::new(w, h).context("allocating card pixmap")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().context("encoding card PNG")
}

/// Escape text for embedding inside an SVG `<text>` node.
fn esc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Strip characters that some fonts render as tofu.
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

fn hex(color: u32) -> String {
    format!("#{:06X}", color)
}

fn initials(name: &str) -> String {
    let mut out = String::new();
    for word in name.split_whitespace().take(2) {
        if let Some(ch) = word.chars().next() {
            out.extend(ch.to_uppercase());
        }
    }
    if out.is_empty() {
        "?".to_string()
    } else {
        out
    }
}

/// A player profile card: avatar monogram, name, alliance and the headline
/// numbers. Used by `/lookup player card` and the lookup panel.
pub fn player_card(view: &LookupPlayerView) -> Result<Vec<u8>> {
    let w = 900.0f32;
    let h = 420.0f32;
    let name = view.game_name.clone().unwrap_or_else(|| view.fid.clone());
    let alliance = view
        .alliance
        .clone()
        .unwrap_or_else(|| "No alliance".into());
    let monogram = initials(&name);
    let brand = hex(colors::BRAND);
    let deep = hex(colors::BRAND_DEEP);
    let dial = hex(colors::INFO);

    let stat = |x: f32, label: &str, value: String| {
        format!(
            r##"<text x="{x}" y="286" fill="#8B93A7" font-size="17" font-family="Open Sans" letter-spacing="1.5">{label}</text>
               <text x="{x}" y="328" fill="#FFFFFF" font-size="32" font-family="Open Sans" font-weight="bold">{value}</text>"##,
            x = x,
            label = esc(label),
            value = esc(&value)
        )
    };

    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#12141C"/>
      <stop offset="1" stop-color="#1B2030"/>
    </linearGradient>
    <linearGradient id="accent" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="{brand}"/>
      <stop offset="1" stop-color="{deep}"/>
    </linearGradient>
  </defs>
  <rect width="{w}" height="{h}" rx="26" fill="url(#bg)"/>
  <rect width="{w}" height="8" rx="4" fill="url(#accent)"/>
  <circle cx="862" cy="70" r="150" fill="{dial}" opacity="0.06"/>
  <circle cx="120" cy="150" r="78" fill="url(#accent)" opacity="0.9"/>
  <text x="120" y="166" text-anchor="middle" fill="#FFFFFF" font-size="52" font-family="Open Sans" font-weight="bold">{monogram}</text>
  <text x="226" y="124" fill="#FFFFFF" font-size="42" font-family="Open Sans" font-weight="bold">{name}</text>
  <text x="226" y="164" fill="#9AA3B8" font-size="20" font-family="Open Sans">{alliance}</text>
  <text x="226" y="200" fill="{dial}" font-size="17" font-family="Open Sans" letter-spacing="1">FID {fid}</text>
  <line x1="48" y1="240" x2="852" y2="240" stroke="#2B3245" stroke-width="2"/>
  {power}
  {kills}
  {th}
  {tree}
  <text x="852" y="396" text-anchor="end" fill="#5C6478" font-size="15" font-family="Open Sans">{footer}</text>
</svg>"##,
        w = w,
        h = h,
        brand = brand,
        deep = deep,
        dial = dial,
        monogram = esc(&monogram),
        name = esc(&theme_trim(&name, 26)),
        alliance = esc(&theme_trim(&alliance, 34)),
        fid = esc(&view.fid),
        power = stat(
            60.0,
            "POWER",
            view.power
                .map(theme::compact_number)
                .unwrap_or_else(|| "—".into())
        ),
        kills = stat(
            280.0,
            "KILLS",
            view.kills
                .map(theme::compact_number)
                .unwrap_or_else(|| "—".into())
        ),
        th = stat(
            500.0,
            "TOWN HALL",
            view.town_hall
                .map(|v| v.to_string())
                .unwrap_or_else(|| "—".into())
        ),
        tree = stat(
            720.0,
            "LIFE TREE",
            view.life_tree
                .map(|v| v.to_string())
                .unwrap_or_else(|| "—".into())
        ),
        footer = esc(&format!(
            "State {}  •  OmniCore",
            view.state
                .map(|s| format!("#{s}"))
                .unwrap_or_else(|| "unknown".into())
        )),
    );

    svg_to_png(&svg, 2.0)
}

fn theme_trim(input: &str, limit: usize) -> String {
    crate::error::truncate(input, limit)
}

/// A compact card listing freshly discovered gift codes.
pub fn giftcode_card(codes: &[String]) -> Result<Vec<u8>> {
    let shown = codes.iter().take(8).collect::<Vec<_>>();
    let w = 760.0f32;
    let h = 120.0 + shown.len() as f32 * 46.0;
    let brand = hex(colors::BRAND);
    let gold = hex(colors::GOLD);

    let mut rows = String::new();
    for (i, code) in shown.iter().enumerate() {
        let y = 150.0 + i as f32 * 46.0;
        rows.push_str(&format!(
            r##"<circle cx="72" cy="{y}" r="9" fill="{gold}"/>
               <text x="104" y="{ty}" fill="#FFFFFF" font-size="24" font-family="Open Sans" font-weight="bold" letter-spacing="2">{code}</text>"##,
            y = y,
            ty = y + 8.0,
            gold = gold,
            code = esc(code)
        ));
    }

    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <rect width="{w}" height="{h}" rx="24" fill="#12141C"/>
  <rect width="{w}" height="7" rx="3.5" fill="{brand}"/>
  <text x="48" y="78" fill="#FFFFFF" font-size="30" font-family="Open Sans" font-weight="bold">New gift codes</text>
  <text x="48" y="108" fill="#8B93A7" font-size="16" font-family="Open Sans">Synced by OmniCore</text>
  {rows}
</svg>"##,
        w = w,
        h = h,
        brand = brand,
        rows = rows,
    );

    svg_to_png(&svg, 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_player_card() {
        let view = LookupPlayerView {
            fid: "123456789".into(),
            game_name: Some("Necromancer".into()),
            alliance: Some("[WOS] Warriors".into()),
            power: Some(12_345_678),
            kills: Some(98_765),
            town_hall: Some(30),
            life_tree: Some(12),
            state: Some(512),
            ..Default::default()
        };
        let png = player_card(&view).expect("render");
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.len() > 5_000);
    }

    #[test]
    fn escapes_svg_text() {
        assert_eq!(esc("a<b&c"), "a&lt;b&amp;c");
    }
}
