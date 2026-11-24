//! Game-number parsing/formatting for the calculator features.
//!
//! Rust port of `src/calculator/gameNumbers.js` (the amount-related helpers).

pub fn parse_game_amount(raw: &str) -> i64 {
    let str = raw.trim();
    if str.is_empty() || str == "—" || str == "-" {
        return 0;
    }
    let cleaned = str.replace(',', "");
    let (digits, suffix) = match cleaned.find(|c: char| c.is_ascii_alphabetic()) {
        Some(index) => (
            &cleaned[..index],
            cleaned[index..].trim().to_ascii_uppercase(),
        ),
        None => (cleaned.as_str(), String::new()),
    };
    let Ok(value) = digits.trim().parse::<f64>() else {
        return 0;
    };
    match suffix.as_str() {
        "" => value.round() as i64,
        "K" => (value * 1_000.0).round() as i64,
        "M" => (value * 1_000_000.0).round() as i64,
        "B" => (value * 1_000_000_000.0).round() as i64,
        _ => value.round() as i64,
    }
}

fn trim_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

pub fn format_game_amount(n: i64) -> String {
    let abs = n.unsigned_abs();
    if abs < 1_000 {
        return format!("{}{}", if n < 0 { "-" } else { "" }, n.abs());
    }
    let (divisor, suffix) = if abs >= 1_000_000_000 {
        (1_000_000_000f64, "B")
    } else if abs >= 1_000_000 {
        (1_000_000f64, "M")
    } else {
        (1_000f64, "K")
    };
    let mut rounded = (abs as f64 / divisor * 100.0).round() / 100.0;
    let mut suffix = suffix;
    if suffix == "K" && rounded >= 1000.0 {
        rounded = (abs as f64 / 1_000_000.0 * 100.0).round() / 100.0;
        suffix = "M";
    } else if suffix == "M" && rounded >= 1000.0 {
        rounded = (abs as f64 / 1_000_000_000.0 * 100.0).round() / 100.0;
        suffix = "B";
    }
    format!(
        "{}{}{}",
        if n < 0 { "-" } else { "" },
        trim_trailing_zeros(&format!("{rounded:.2}")),
        suffix
    )
}

/// `93750` → `"1d 2h 2m 30s"` (legacy `formatGameDuration`).
pub fn format_game_duration(total_seconds: f64) -> String {
    if total_seconds <= 0.0 {
        return "0s".to_string();
    }
    let mut remaining = total_seconds.round() as i64;
    let days = remaining / 86400;
    remaining -= days * 86400;
    let hours = remaining / 3600;
    remaining -= hours * 3600;
    let minutes = remaining / 60;
    remaining -= minutes * 60;
    let seconds = remaining;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts() {
        assert_eq!(parse_game_amount("260,000"), 260_000);
        assert_eq!(parse_game_amount("3.8K"), 3_800);
        assert_eq!(parse_game_amount("1.25M"), 1_250_000);
        assert_eq!(parse_game_amount("—"), 0);
    }

    #[test]
    fn formats() {
        assert_eq!(format_game_amount(765), "765");
        assert_eq!(format_game_amount(260_000), "260K");
        assert_eq!(format_game_amount(1_250_000), "1.25M");
        assert_eq!(format_game_amount(999_999), "1M");
    }
}
