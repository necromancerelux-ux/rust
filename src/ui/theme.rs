//! Shared visual language for every embed and panel.
//!
//! The legacy bot used a dark Discord-embed layout with a UTC clock in the
//! footer. We keep that identity but tighten it: one palette, one divider,
//! one footer helper, one set of icons.

use chrono::{DateTime, Utc};

/// Palette. Values are Discord embed colours (0xRRGGBB).
pub mod colors {
    /// Primary brand colour.
    pub const BRAND: u32 = 0x5865F2;
    /// Slightly deeper variant used for headers.
    pub const BRAND_DEEP: u32 = 0x4752C4;
    pub const SUCCESS: u32 = 0x3BA55D;
    pub const WARNING: u32 = 0xFAA61A;
    pub const DANGER: u32 = 0xED4245;
    pub const INFO: u32 = 0x00A8FC;
    pub const MUTED: u32 = 0x4F545C;
    pub const GOLD: u32 = 0xF1C40F;
    pub const VIOLET: u32 = 0x9B59B6;
    pub const TEAL: u32 = 0x1ABC9C;
}

/// Icons. Plain Unicode for UI chrome. Material icons in calculators use
/// `crate::emoji::material_tag` (home-guild custom emoji when installed).
pub mod icons {
    pub const BRAND: &str = "🌐";
    pub const SETTINGS: &str = "⚙️";
    pub const MUSIC: &str = "🎵";
    pub const PLAY: &str = "▶️";
    pub const PAUSE: &str = "⏸️";
    pub const STOP: &str = "⏹️";
    pub const SKIP: &str = "⏭️";
    pub const PREVIOUS: &str = "⏮️";
    pub const SHUFFLE: &str = "🔀";
    pub const REPEAT: &str = "🔁";
    pub const VOLUME: &str = "🔊";
    pub const QUEUE: &str = "📜";
    pub const LOOKUP: &str = "🔎";
    pub const PLAYER: &str = "👤";
    pub const ALLIANCE: &str = "🛡️";
    pub const STATE: &str = "🗺️";
    pub const GIFT: &str = "🎁";
    pub const ORACLE: &str = "🔮";
    pub const ADMIN: &str = "🛠️";
    pub const DATABASE: &str = "🗄️";
    pub const CLOCK: &str = "🕒";
    pub const CROWN: &str = "👑";
    pub const MEDAL: &str = "🏅";
    pub const BOLT: &str = "⚡";
    pub const SWORD: &str = "⚔️";
    pub const TOWN: &str = "🏰";
    pub const TREE: &str = "🌳";
    pub const CHECK: &str = "✅";
    pub const CROSS: &str = "❌";
    pub const WARN: &str = "⚠️";
    pub const INFO: &str = "ℹ️";
    pub const LOCK: &str = "🔒";
    pub const BACK: &str = "◀️";
    pub const FORWARD: &str = "▶️";
    pub const REFRESH: &str = "🔄";
    pub const ADD: &str = "➕";
    pub const REMOVE: &str = "➖";
    pub const TRASH: &str = "🗑️";
    pub const SAVE: &str = "💾";
    pub const LINK: &str = "🔗";
    pub const BELL: &str = "🔔";
    pub const TAG: &str = "🏷️";
    pub const LIST: &str = "📋";
    pub const MESSAGES: &str = "✉️";
    pub const SERVER: &str = "🖥️";
    pub const EDIT: &str = "✏️";
    pub const SNOWFLAKE: &str = "❄️";
    pub const MEGAPHONE: &str = "📣";
    pub const ROBOT: &str = "🤖";
    pub const ID: &str = "🪪";
    pub const WOS_NEWS: &str = "📰";
    pub const CALCULATOR: &str = "🧮";
    pub const TOOLS: &str = "🛠️";
    pub const LOG: &str = "📜";
}

/// Thin horizontal rule that reads well inside an embed.
pub const DIVIDER: &str = "─────────────────────────────";

/// Every embed footer ends with this credit.
pub const CREDIT: &str = "OmniCore • by Necromancer";

/// The one UTC format used everywhere in the bot: `2026-11-19 08:31 UTC`
/// (ISO date, 24-hour time, no seconds, always explicitly UTC). Discord's own
/// `<t:…:f>` is NOT used for absolute times because it renders in each
/// viewer's local zone, which is ambiguous for UTC-coordinated game events.
const UTC_FORMAT: &str = "%Y-%m-%d %H:%M UTC";

/// Format a unix timestamp (seconds) as `2026-11-19 08:31 UTC`.
pub fn utc(ts: i64) -> String {
    match DateTime::<Utc>::from_timestamp(ts, 0) {
        Some(dt) => dt.format(UTC_FORMAT).to_string(),
        None => "unknown time".to_string(),
    }
}

/// The current time in the canonical UTC format.
pub fn utc_now() -> String {
    utc(Utc::now().timestamp())
}

/// An absolute UTC time plus Discord's live-relative form:
/// `<t:…:R> (2026-11-19 08:31 UTC)`.
pub fn utc_with_relative(ts: i64) -> String {
    format!("<t:{ts}:R> ({})", utc(ts))
}

/// The footer text for "now": `2026-11-19 08:31 UTC • OmniCore • by …`.
pub fn footer() -> String {
    footer_at(Utc::now())
}

pub fn footer_at(ts: DateTime<Utc>) -> String {
    format!("{} • {}", ts.format(UTC_FORMAT), CREDIT)
}

pub fn timestamp() -> String {
    utc_now()
}

/// `1_234_567` → `1.23M`.
pub fn compact_number(value: i64) -> String {
    let n = value as f64;
    let abs = n.abs();
    if abs >= 1_000_000_000.0 {
        format!("{:.2}B", n / 1_000_000_000.0)
    } else if abs >= 1_000_000.0 {
        format!("{:.2}M", n / 1_000_000.0)
    } else if abs >= 10_000.0 {
        format!("{:.1}K", n / 1_000.0)
    } else {
        let mut s = value.to_string();
        // Insert thin separators for readability.
        if abs >= 1_000.0 {
            let negative = s.starts_with('-');
            if negative {
                s.remove(0);
            }
            let bytes = s.as_bytes();
            let mut out = String::with_capacity(s.len() + s.len() / 3);
            for (i, ch) in bytes.iter().enumerate() {
                if i > 0 && (bytes.len() - i) % 3 == 0 {
                    out.push(',');
                }
                out.push(*ch as char);
            }
            if negative {
                format!("-{out}")
            } else {
                out
            }
        } else {
            s
        }
    }
}

/// Turn a millisecond duration into `m:ss` / `h:mm:ss`.
pub fn duration(ms: u64) -> String {
    let total = ms / 1000;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// A gradient-free progress bar. Uses block characters so it lines up.
pub fn progress_bar(position_ms: u64, length_ms: u64, width: usize) -> String {
    if length_ms == 0 {
        return "▬".repeat(width);
    }
    let ratio = (position_ms as f64 / length_ms as f64).clamp(0.0, 1.0);
    let filled = ((ratio * width as f64).round() as usize).min(width);
    let mut out = String::with_capacity(width + 2);
    for i in 0..width {
        if i == filled {
            out.push('🔘');
        } else if i < filled {
            out.push('━');
        } else {
            out.push('─');
        }
    }
    // Discord does not allow a marker to overlap; prepend/append the knob.
    out = format!(
        "▬{}🔘{}",
        "▬".repeat(filled.saturating_sub(1)),
        "▬".repeat(width.saturating_sub(filled))
    );
    out
}

/// Shorten a Discord snowflake for display.
pub fn short_id(id: u64) -> String {
    format!(
        "…{}",
        &id.to_string()[id.to_string().len().saturating_sub(4)..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact() {
        assert_eq!(compact_number(999), "999");
        assert_eq!(compact_number(12_345), "12.3K");
        assert_eq!(compact_number(2_500_000), "2.50M");
        assert_eq!(compact_number(1_234), "1,234");
    }

    #[test]
    fn durations() {
        assert_eq!(duration(0), "0:00");
        assert_eq!(duration(65_000), "1:05");
        assert_eq!(duration(3_725_000), "1:02:05");
    }
}
