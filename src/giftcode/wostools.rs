//! Read-only gift-code discovery from the public WoSTools gift-code page.
//! Discovered codes are candidates only; the sync job validates them before
//! storing or announcing them.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

const DEFAULT_URL: &str = "https://wostools.net/gift-codes";
const MAX_CODES: usize = 60;

fn code_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[A-Za-z0-9]{4,32}$").expect("static regex"))
}

fn json_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)["\\]*code["\\]*\s*:\s*["\\]*([A-Za-z0-9]{4,32})["\\]*[^{}]{0,200}?["\\]*(?:status|state)["\\]*\s*:\s*["\\]*active"#)
            .expect("static regex")
    })
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn lines_from_html(html: &str) -> Vec<String> {
    static TAG: OnceLock<Regex> = OnceLock::new();
    static SCRIPT: OnceLock<Regex> = OnceLock::new();
    static STYLE: OnceLock<Regex> = OnceLock::new();
    let script = SCRIPT.get_or_init(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script>").unwrap());
    let style = STYLE.get_or_init(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style>").unwrap());
    let tag = TAG.get_or_init(|| Regex::new(r"(?s)<[^>]+>").unwrap());
    let without_scripts = script.replace_all(html, "\n");
    let without_styles = style.replace_all(&without_scripts, "\n");
    let text = tag.replace_all(&without_styles, "\n");
    decode_entities(&text)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn looks_like_code(token: &str) -> bool {
    if !code_pattern().is_match(token) {
        return false;
    }
    !matches!(
        token.to_ascii_lowercase().as_str(),
        "active"
            | "expired"
            | "copy"
            | "copied"
            | "redeem"
            | "premium"
            | "basic"
            | "supporter"
            | "commander"
            | "known"
            | "gift"
            | "codes"
            | "player"
            | "state"
            | "sign"
            | "login"
    )
}

/// Parse only active codes from WoSTools' known-code block, plus active entries
/// exposed as embedded JSON. Returns unique, bounded candidates.
pub fn parse_codes(html: &str) -> Vec<String> {
    let lines = lines_from_html(html);
    let start = lines
        .iter()
        .position(|line| line.eq_ignore_ascii_case("known gift codes"))
        .unwrap_or(0);
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| {
            line.to_ascii_lowercase().starts_with("expired codes")
                || line.eq_ignore_ascii_case("auto-redeem gift codes")
        })
        .map(|(index, _)| index)
        .unwrap_or(lines.len());
    let mut found = Vec::new();
    for pair in lines[start..end].windows(2) {
        if looks_like_code(&pair[0]) && pair[1].eq_ignore_ascii_case("active") {
            found.push(pair[0].clone());
        }
    }
    for capture in json_pattern().captures_iter(html) {
        if let Some(code) = capture.get(1).map(|value| value.as_str()) {
            if looks_like_code(code) {
                found.push(code.to_string());
            }
        }
    }
    let mut seen = HashSet::new();
    found.retain(|code| seen.insert(code.to_ascii_uppercase()));
    found.truncate(MAX_CODES);
    found
}

pub async fn fetch_codes() -> anyhow::Result<Vec<String>> {
    if std::env::var("GIFTCODE_WOSTOOLS_ENABLED").is_ok_and(|value| value == "0") {
        return Ok(Vec::new());
    }
    let url = std::env::var("GIFTCODE_WOSTOOLS_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let response = crate::net::builder()
        .timeout(std::time::Duration::from_secs(20))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?
        .get(&url)
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header(
            "User-Agent",
            "Mozilla/5.0 (compatible; Omnicore/2.0; +https://wostools.net)",
        )
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("wostools.net returned HTTP {status}");
    }
    let html = response.text().await?;
    Ok(parse_codes(&html))
}
