//! Optional community gift-code feed used by the legacy bot as its primary source.
//! Codes are treated as candidates and validated against the signed game endpoint
//! before they can be stored or announced.

use std::sync::OnceLock;

use regex::Regex;

const DEFAULT_URL: &str = "https://gift-code-api.whiteout-bot.com/giftcode_api.php";

#[derive(Debug, thiserror::Error)]
#[error("community gift-code API returned HTTP {status}")]
pub struct HttpStatusError {
    pub status: u16,
    pub retry_after: Option<std::time::Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub code: String,
    pub date: Option<String>,
}

fn code_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[A-Za-z0-9]{4,32}$").expect("static code regex"))
}

fn parse_code_line(line: &str) -> Option<Candidate> {
    let mut parts = line.split_whitespace();
    let code = parts.next()?.trim();
    let date = parts.next()?;
    if parts.next().is_some() || !code_pattern().is_match(code) {
        return None;
    }
    let parsed = chrono::NaiveDate::parse_from_str(date, "%d.%m.%Y").ok()?;
    Some(Candidate {
        code: code.to_string(),
        date: Some(parsed.format("%Y-%m-%d").to_string()),
    })
}

pub fn configured() -> bool {
    std::env::var("GIFTCODE_SYNC_API_KEY").is_ok_and(|key| !key.trim().is_empty())
}

pub async fn fetch_codes() -> anyhow::Result<Vec<Candidate>> {
    let Some(key) = std::env::var("GIFTCODE_SYNC_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
    else {
        return Ok(Vec::new());
    };
    if std::env::var("GIFTCODE_SYNC_ENABLED").is_ok_and(|value| value == "0") {
        return Ok(Vec::new());
    }
    let url = std::env::var("GIFTCODE_SYNC_API_URL").unwrap_or_else(|_| DEFAULT_URL.into());
    if !url.starts_with("https://") {
        anyhow::bail!("community gift-code API must use HTTPS");
    }
    let response = crate::net::builder()
        .timeout(std::time::Duration::from_secs(20))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?
        .get(&url)
        .header("X-API-Key", key)
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            "Mozilla/5.0 (compatible; OmniCore gift-code sync)",
        )
        .send()
        .await?;
    let status = response.status();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_secs);
    let body = response.text().await?;
    if !status.is_success() {
        return Err(HttpStatusError {
            status: status.as_u16(),
            retry_after,
        }
        .into());
    }
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|_| anyhow::anyhow!("community gift-code API returned invalid JSON"))?;
    if value.get("error").is_some() || value.get("detail").is_some() {
        anyhow::bail!("community gift-code API reported an error");
    }
    let rows = value
        .get("codes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("community gift-code API response has no codes list"))?;
    let mut seen = std::collections::HashSet::new();
    Ok(rows
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter_map(parse_code_line)
        .filter(|candidate| seen.insert(candidate.code.to_ascii_uppercase()))
        .take(100)
        .collect())
}
