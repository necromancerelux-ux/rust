//! Small Whiteout Survival domain helpers.
//!
//! These mirror the validation and display rules the legacy bot used so the two
//! runtimes accept exactly the same input.

pub mod relay;

/// FIDs are numeric and between 4 and 32 digits.
pub fn valid_fid(fid: &str) -> bool {
    let len = fid.len();
    (4..=32).contains(&len) && fid.chars().all(|c| c.is_ascii_digit())
}

/// Trim separators people paste from the game (`1 234 567` / `1.234.567`).
pub fn normalize_fid(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if valid_fid(&digits) {
        Some(digits)
    } else {
        None
    }
}

/// Accepts `WOS`, `[WOS]`, ` wos ` and returns `WOS`.
pub fn normalize_alliance_tag(raw: &str) -> Option<String> {
    let cleaned = raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim()
        .to_uppercase();
    if cleaned.is_empty() || cleaned.len() > 8 {
        return None;
    }
    if cleaned
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Some(cleaned)
    } else {
        None
    }
}

/// WoS calls the main building a *furnace*; the UI says "Town Hall". Keep both
/// labels available so the legacy text stays familiar.
pub fn furnace_label(level: Option<i64>) -> String {
    match level {
        Some(level) if level > 0 => format!("FC {level}"),
        _ => "unknown".to_string(),
    }
}

pub fn town_hall_label(level: Option<i64>) -> String {
    match level {
        Some(level) if level > 0 => format!("TH {level}"),
        _ => "unknown".to_string(),
    }
}

/// `[WOS] Warriors of Snow`
pub fn alliance_tag(abbr: Option<&str>, name: Option<&str>) -> String {
    match (abbr, name) {
        (Some(abbr), Some(name)) => format!("[{abbr}] {name}"),
        (Some(abbr), None) => format!("[{abbr}]"),
        (None, Some(name)) => name.to_string(),
        (None, None) => "No alliance".to_string(),
    }
}

pub fn state_label(state: Option<i64>) -> String {
    state
        .map(|state| format!("#{state}"))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Parse the "SVIP"/"VIP" style flags the legacy gift-code panel used.
pub fn truthy_flag(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "vip" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fids() {
        assert!(valid_fid("123456789"));
        assert!(!valid_fid("12 34"));
        assert!(!valid_fid("abc12345"));
        assert_eq!(normalize_fid(" 12.345.678 ").as_deref(), Some("12345678"));
    }

    #[test]
    fn tags() {
        assert_eq!(normalize_alliance_tag("[wos]").as_deref(), Some("WOS"));
        assert_eq!(normalize_alliance_tag("  "), None);
        assert_eq!(
            alliance_tag(Some("WOS"), Some("Warriors")),
            "[WOS] Warriors"
        );
    }
}
