//! Whiteout Stats API key storage — single global row (owner-managed).
//! Mirrors the legacy `database/wosCredentials.js`.

use super::Db;

#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub api_key: Option<String>,
    pub last_test_at: Option<i64>,
    pub last_test_ok: Option<bool>,
    pub last_test_message: Option<String>,
}

type CredentialRow = (String, Option<i64>, Option<i64>, Option<String>);

pub async fn get(db: &Db) -> sqlx::Result<Credentials> {
    let row: Option<CredentialRow> = sqlx::query_as(
        "SELECT api_key, last_test_at, last_test_ok, last_test_message FROM wos_stats_credentials WHERE id = 1",
    )
    .fetch_optional(&db.pool)
    .await?;
    Ok(
        row.map_or_else(Credentials::default, |(api_key, at, ok, message)| {
            Credentials {
                api_key: crate::secrets::open(&api_key),
                last_test_at: at,
                last_test_ok: ok.map(|v| v != 0),
                last_test_message: message,
            }
        }),
    )
}

pub async fn is_configured(db: &Db) -> bool {
    get(db).await.ok().and_then(|c| c.api_key).is_some()
}

pub async fn api_key(db: &Db) -> Option<String> {
    get(db).await.ok().and_then(|c| c.api_key)
}

pub async fn set(db: &Db, api_key: &str, updated_by: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO wos_stats_credentials (id, api_key, updated_at, updated_by)
         VALUES (1, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET api_key = excluded.api_key, updated_at = excluded.updated_at, updated_by = excluded.updated_by,
             last_test_at = NULL, last_test_ok = NULL, last_test_message = NULL",
    )
    .bind(crate::secrets::seal(api_key.trim()))
    .bind(chrono::Utc::now().timestamp_millis())
    .bind(updated_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn clear(db: &Db) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM wos_stats_credentials WHERE id = 1")
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn record_test(db: &Db, ok: bool, message: &str) -> sqlx::Result<()> {
    let message: String = message.chars().take(500).collect();
    sqlx::query(
        "UPDATE wos_stats_credentials SET last_test_at = ?, last_test_ok = ?, last_test_message = ? WHERE id = 1",
    )
    .bind(chrono::Utc::now().timestamp_millis())
    .bind(ok as i64)
    .bind(message)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Accepts a bare key, `Authorization: Bearer wos_…`, `x-api-key: wos_…`,
/// or a curl paste line; returns just the key.
pub fn parse_api_key_paste(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }

    // Any `Bearer <key>` token wins.
    let lower = text.to_ascii_lowercase();
    if let Some(index) = lower.find("bearer") {
        let candidate = text[index + 6..]
            .trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '=')
            .split_whitespace()
            .next()
            .map(|s| s.trim_matches(['"', '\'', ',', ';', ')']));
        if let Some(candidate) = candidate.filter(|s| plausible_key(s)) {
            return Some(candidate.to_string());
        }
    }

    // A token that looks like a key anywhere in the paste (e.g. -H "x-api-key: wos_XXXX")
    for token in text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '.')) {
        if plausible_key(token) {
            return Some(token.to_string());
        }
    }

    // Bare key with surrounding noise trimmed (whitespace only).
    let bare = text.split_whitespace().collect::<Vec<_>>().join("");
    if plausible_key(&bare) {
        return Some(bare);
    }
    None
}

fn plausible_key(token: &str) -> bool {
    token.len() >= 16
        && token.len() <= 300
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_key() {
        assert_eq!(
            parse_api_key_paste("wos_abc123def456ghi789"),
            Some("wos_abc123def456ghi789".to_string())
        );
    }

    #[test]
    fn parses_bearer_header() {
        assert_eq!(
            parse_api_key_paste("Authorization: Bearer wos_abc123def456ghi789"),
            Some("wos_abc123def456ghi789".to_string())
        );
    }

    #[test]
    fn parses_curl_paste() {
        let paste = "curl -H \"Authorization: Bearer wos_xyz987uvw654tsr321\" https://api.whiteoutstats.com";
        assert_eq!(
            parse_api_key_paste(paste),
            Some("wos_xyz987uvw654tsr321".to_string())
        );
    }

    #[test]
    fn rejects_noise() {
        assert_eq!(parse_api_key_paste(""), None);
        assert_eq!(parse_api_key_paste("Authorization: Bearer !!!"), None);
        assert_eq!(parse_api_key_paste("Connection: keep-alive"), None);
    }
}
