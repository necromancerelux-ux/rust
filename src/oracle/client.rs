//! wosoracle.com HTTP client.
//!
//! wosoracle.com no longer requires a Discord login. Access is open with an
//! optional `x-api-token` (paste from browser DevTools → Network → any /api
//! request). The token unlocks player / kingdom / loadout data and higher
//! limits; without it the public endpoints (codes, config) still work.
//!
//! Same shape as the Whiteout Stats client. No browser sessions, no Discord
//! OAuth. A short-lived `cf_clearance` cookie is optional and rarely needed.

use std::time::Duration;

use serde_json::Value;

use crate::db;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/139.0.0.0 Safari/537.36";

#[derive(Debug, thiserror::Error)]
pub enum OracleError {
    #[error("Oracle is rate limiting us — try again in a moment")]
    RateLimited,
    #[error("Oracle returned HTTP {0}")]
    Http(u16),
    #[error("Oracle returned an unexpected body")]
    Malformed,
    #[error("network error: {0}")]
    Network(String),
    #[error("{0}")]
    Other(String),
}

impl From<reqwest::Error> for OracleError {
    fn from(value: reqwest::Error) -> Self {
        if value.is_timeout() {
            Self::Network("request timed out".to_string())
        } else {
            Self::Network(value.to_string())
        }
    }
}

/// One row from `/api/codes`.
#[derive(Debug, Clone)]
pub struct CodeRow {
    pub code: String,
    pub date: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct OracleConfig {
    pub site: String,
    pub codes_url: String,
}

/// Cheap-to-clone Oracle access layer (optional API key / token).
#[derive(Clone)]
pub struct OracleClient {
    http: reqwest::Client,
    pub cfg: OracleConfig,
    /// Optional `x-api-token` (paste from browser). Unlocks player/kingdom data.
    api_key: std::sync::Arc<std::sync::RwLock<Option<String>>>,
    /// Optional short-lived Cloudflare clearance cookie (rarely needed).
    cf_cookie: std::sync::Arc<std::sync::RwLock<Option<String>>>,
}

impl OracleClient {
    pub fn new(cfg: OracleConfig) -> anyhow::Result<Self> {
        let http = crate::net::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(8)
            .gzip(true)
            .brotli(true)
            .build()?;
        Ok(Self {
            http,
            cfg,
            api_key: Default::default(),
            cf_cookie: Default::default(),
        })
    }

    pub fn set_api_key(&self, key: Option<String>) {
        if let Ok(mut guard) = self.api_key.write() {
            *guard = key.filter(|k| !k.trim().is_empty());
        }
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.read().map(|k| k.is_some()).unwrap_or(false)
    }

    /// Optional `cf_clearance=…` cookie string (or full Cookie header value).
    pub fn set_cf_cookie(&self, cookie: Option<String>) {
        if let Ok(mut guard) = self.cf_cookie.write() {
            *guard = cookie.filter(|c| !c.trim().is_empty());
        }
    }

    /// Probe endpoints and report exact status. Returns `(ok, message)`.
    pub async fn test_connection(&self) -> (bool, String) {
        // 1) Public config (works without token)
        let config_url = self.api_url("/api/config");
        let config = self.get_raw(&config_url).await;

        // 2) Codes (usually public)
        let codes = self.get_raw(self.codes_url()).await;

        // 3) A real data endpoint that needs the token
        let kingdom = self.get_raw(&self.api_url("/api/kingdoms/1")).await;

        let key_note = if self.has_api_key() {
            "key attached"
        } else {
            "no key"
        };

        // Prefer the richest successful response for the message.
        for (label, result) in [
            ("kingdoms", &kingdom),
            ("codes", &codes),
            ("config", &config),
        ] {
            if let Ok((status, body)) = result {
                if (200..300).contains(status) {
                    let hint = if body.contains("is_poisoned") && body.contains("true") {
                        " · poisoned notice (still usable)"
                    } else if body.trim_start().starts_with('{') || body.trim_start().starts_with('[') {
                        " · JSON ok"
                    } else {
                        ""
                    };
                    return (
                        true,
                        format!("HTTP {status} on /api/{label} ({key_note}){hint}"),
                    );
                }
                if *status == 401 || *status == 403 {
                    return (
                        false,
                        format!(
                            "HTTP {status} on /api/{label} ({key_note}) — token missing or expired. \
                             Paste a fresh x-api-token from browser DevTools."
                        ),
                    );
                }
                if *status == 429 {
                    return (false, format!("HTTP 429 rate-limited ({key_note})"));
                }
                if body.to_ascii_lowercase().contains("cloudflare")
                    || body.to_ascii_lowercase().contains("cf-ray")
                    || body.contains("Just a moment")
                {
                    return (
                        false,
                        format!("HTTP {status} Cloudflare challenge on /api/{label} ({key_note})"),
                    );
                }
            }
        }

        // All failed
        let err = kingdom
            .as_ref()
            .err()
            .or(codes.as_ref().err())
            .or(config.as_ref().err())
            .map(|e| e.to_string())
            .unwrap_or_else(|| "no response".into());
        (false, format!("wosoracle.com unreachable ({key_note}): {err}"))
    }

    /// Always true: no credentials are required for the client to exist.
    pub async fn is_configured(&self) -> bool {
        true
    }

    // -------------------------------------------------------------- transport

    fn codes_url(&self) -> &str {
        &self.cfg.codes_url
    }

    fn api_url(&self, path: &str) -> String {
        let base = self.cfg.site.trim_end_matches('/');
        if path.starts_with('/') {
            format!("{base}{path}")
        } else {
            format!("{base}/{path}")
        }
    }

    /// Low-level GET that returns (status, body text) for diagnostics.
    async fn get_raw(&self, url: &str) -> Result<(u16, String), OracleError> {
        let mut attempt = 0u8;
        loop {
            let key = self.api_key.read().ok().and_then(|k| k.clone());
            let cookie = self.cf_cookie.read().ok().and_then(|c| c.clone());

            let mut request = self
                .http
                .get(url)
                .header("accept", "application/json, */*")
                .header("accept-language", "en-US,en;q=0.9")
                .header(
                    "referer",
                    format!("{}/", self.cfg.site.trim_end_matches('/')),
                )
                .header("sec-fetch-dest", "empty")
                .header("sec-fetch-mode", "cors")
                .header("sec-fetch-site", "same-origin");

            // Primary auth header used by the live site.
            if let Some(ref key) = key {
                request = request.header("x-api-token", key.as_str());
                // Harmless fallbacks for future official API keys.
                request = request
                    .header("authorization", format!("Bearer {key}"))
                    .header("x-api-key", key.as_str());
            }
            if let Some(ref cookie) = cookie {
                request = request.header("cookie", cookie.as_str());
            }

            let response = request.send().await?;
            let status = response.status().as_u16();
            if (status == 429 || status >= 500) && attempt == 0 {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(600)).await;
                continue;
            }
            let text = response.text().await?;
            return Ok((status, text));
        }
    }

    /// One GET with a single retry on 429/5xx. Parses JSON.
    async fn get(&self, url: &str) -> Result<Value, OracleError> {
        let (status, text) = self.get_raw(url).await?;
        decode(status, text)
    }

    // ---------------------------------------------------------------- payloads

    /// Active gift codes from `wosoracle.com/api/codes`.
    pub async fn fetch_codes(&self) -> Result<Vec<CodeRow>, OracleError> {
        let value = self.get(self.codes_url()).await?;
        let rows = value
            .as_array()
            .cloned()
            .or_else(|| {
                ["codes", "data", "items", "results"]
                    .iter()
                    .find_map(|key| value.get(*key).and_then(Value::as_array).cloned())
            })
            .ok_or(OracleError::Malformed)?;

        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for row in rows {
            let raw = row
                .get("Code")
                .or_else(|| row.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if !valid_code(&raw) || !seen.insert(raw.clone()) {
                continue;
            }
            let active = row
                .get("IsActive")
                .or_else(|| row.get("isActive"))
                .or_else(|| row.get("is_active"))
                .or_else(|| row.get("active"))
                .and_then(Value::as_bool)
                .unwrap_or_else(|| {
                    let status = row
                        .get("Status")
                        .or_else(|| row.get("status"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    !matches!(status.as_str(), "expired" | "inactive" | "invalid")
                });
            let date = row
                .get("DiscoveredAt")
                .or_else(|| row.get("discoveredAt"))
                .or_else(|| row.get("discovered_at"))
                .and_then(Value::as_str)
                .filter(|d| d.len() >= 10)
                .map(|d| d[..10].to_string());
            out.push(CodeRow {
                code: raw,
                date,
                active,
            });
            if out.len() >= 100 {
                break;
            }
        }
        Ok(out)
    }

    /// Single player profile: `/api/players/{fid}`.
    pub async fn fetch_player(&self, fid: &str) -> Result<Value, OracleError> {
        let path = format!("/api/players/{}", urlencoding::encode(fid));
        self.get(&self.api_url(&path)).await
    }

    /// Player loadout (gear, heroes): `/api/players/{fid}/loadout`.
    pub async fn fetch_loadout(&self, fid: &str) -> Result<Value, OracleError> {
        let path = format!("/api/players/{}/loadout?cached=1", urlencoding::encode(fid));
        self.get(&self.api_url(&path)).await
    }

    /// State overview: `/api/kingdoms/{kid}`.
    pub async fn fetch_state(&self, kid: i64) -> Result<Value, OracleError> {
        self.get(&self.api_url(&format!("/api/kingdoms/{kid}")))
            .await
    }

    /// Page through the Oracle state player endpoint (up to 10,000 rows).
    pub async fn fetch_state_players(&self, kid: i64) -> Result<Vec<Value>, OracleError> {
        let mut players = Vec::new();
        let limit = 200usize;
        for page in 0..50usize {
            let offset = page * limit;
            let value = self
                .get(&self.api_url(&format!(
                    "/api/players?kingdom={kid}&limit={limit}&offset={offset}"
                )))
                .await?;
            let batch = player_rows(&value);
            if batch.is_empty() {
                break;
            }
            let batch_len = batch.len();
            players.extend(batch);
            if players.len() >= 10_000 || batch_len < limit {
                break;
            }
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
        let mut seen = std::collections::HashSet::new();
        players.retain(|player| player_fid(player).is_some_and(|fid| seen.insert(fid)));
        Ok(players)
    }

    /// Fetch a single Oracle alliance roster using the known route variants.
    pub async fn fetch_alliance_players(
        &self,
        kid: i64,
        aid: &str,
    ) -> Result<Vec<Value>, OracleError> {
        let kid = urlencoding::encode(&kid.to_string()).into_owned();
        let aid = urlencoding::encode(aid);
        let custom = std::env::var("ORACLE_ALLIANCE_PATH_TEMPLATE").ok();
        let custom_path = custom.map(|path| path.replace("{kid}", &kid).replace("{aid}", &aid));
        let paths = [
            custom_path,
            Some(format!("/api/alliances/{aid}?kid={kid}")),
            Some(format!("/api/alliances/{aid}")),
            Some(format!("/api/kingdoms/{kid}/alliances/{aid}")),
            Some(format!("/api/alliances/{kid}/{aid}")),
            Some(format!("/api/alliance/{aid}?kid={kid}")),
        ];
        let mut last_error = OracleError::Malformed;
        for path in paths.into_iter().flatten() {
            let value = match self.get(&self.api_url(&path)).await {
                Ok(value) => value,
                Err(err) => {
                    last_error = err;
                    continue;
                }
            };
            let rows = player_rows(&value);
            if !rows.is_empty() {
                return Ok(rows);
            }
            let base = path.split('?').next().unwrap_or(&path);
            for member_path in [
                format!("{base}/members?kid={kid}"),
                format!("{base}/members"),
                format!("/api/alliances/{aid}/players?kid={kid}"),
            ] {
                match self.get(&self.api_url(&member_path)).await {
                    Ok(value) => {
                        let rows = player_rows(&value);
                        if !rows.is_empty() {
                            return Ok(rows);
                        }
                    }
                    Err(err) => last_error = err,
                }
            }
        }
        Err(last_error)
    }

    /// Reachability probe (no auth involved).
    pub async fn test(&self) -> Result<String, OracleError> {
        self.get(&self.api_url("/api/config")).await?;
        Ok("reachable".to_string())
    }
}

fn player_rows(value: &Value) -> Vec<Value> {
    if let Some(rows) = value.as_array() {
        return rows.clone();
    }
    for key in ["players", "members", "roster", "results", "items", "data"] {
        if let Some(child) = value.get(key) {
            if let Some(rows) = child.as_array() {
                return rows.clone();
            }
            let rows = player_rows(child);
            if !rows.is_empty() {
                return rows;
            }
        }
    }
    Vec::new()
}

fn player_fid(value: &Value) -> Option<String> {
    let root = value
        .get("player")
        .or_else(|| value.get("profile"))
        .or_else(|| value.get("data"))
        .unwrap_or(value);
    ["fid", "player_id", "playerId", "id"]
        .iter()
        .find_map(|key| match root.get(*key) {
            Some(Value::String(fid)) if fid.chars().all(|ch| ch.is_ascii_digit()) => {
                Some(fid.clone())
            }
            Some(Value::Number(fid)) => Some(fid.to_string()),
            _ => None,
        })
}

fn decode(status: u16, text: String) -> Result<Value, OracleError> {
    if status == 429 {
        return Err(OracleError::RateLimited);
    }
    if !(200..300).contains(&status) {
        return Err(OracleError::Http(status));
    }
    let trimmed = text.trim_start();
    if trimmed.starts_with("<!DOCTYPE") || trimmed.starts_with("<html") {
        // The SPA serves 200 HTML for unknown routes.
        return Err(OracleError::Malformed);
    }
    serde_json::from_str(&text).map_err(|_| OracleError::Malformed)
}

/// Gift codes are alphanumeric, 4–32 chars.
pub fn valid_code(code: &str) -> bool {
    let len = code.len();
    (4..=32).contains(&len) && code.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Parse a JSON body into the enrichment struct used by `players`.
pub fn enrichment_from_player(value: &Value) -> db::players::Enrichment {
    use serde_json::Value as V;

    fn locate<'a>(value: &'a V, keys: &[&str]) -> Option<&'a V> {
        let obj = value.as_object()?;
        keys.iter()
            .find_map(|k| obj.get(*k))
            .filter(|v| !v.is_null())
    }

    fn text(value: &V, keys: &[&str]) -> Option<String> {
        let found = locate(value, keys)?;
        match found {
            V::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            V::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }

    fn number(value: &V, keys: &[&str]) -> Option<i64> {
        let found = locate(value, keys)?;
        match found {
            V::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
            V::String(s) => s
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '-')
                .collect::<String>()
                .parse()
                .ok(),
            _ => None,
        }
    }

    let root = locate(value, &["player", "data", "profile"]).unwrap_or(value);
    let alliance = locate(root, &["alliance", "alliance_info"]);

    let abbr = text(root, &["alliance_abbr", "allianceAbbr", "alliance_tag"])
        .or_else(|| alliance.and_then(|a| text(a, &["abbr", "tag", "short_name"])));
    let alliance_name = text(root, &["alliance_name", "allianceName"])
        .or_else(|| alliance.and_then(|a| text(a, &["name", "full_name"])));
    let alliance_id = text(root, &["alliance_id", "allianceId"])
        .or_else(|| alliance.and_then(|a| text(a, &["id", "aid"])));
    let alliance_rank = number(root, &["alliance_rank", "allianceRank", "rank"])
        .or_else(|| alliance.and_then(|a| number(a, &["rank"])));

    db::players::Enrichment {
        game_name: text(root, &["game_name", "name", "nickname", "ign"]),
        alliance_abbr: abbr,
        alliance_id,
        alliance_name,
        alliance_rank,
        power: number(root, &["power", "total_power", "totalPower"]),
        kills: number(root, &["kills", "total_kills", "totalKills"]),
        town_hall_level: number(
            root,
            &[
                "town_hall_level",
                "townHallLevel",
                "furnace_level",
                "furnaceLevel",
                "tc_level",
            ],
        ),
        life_tree_level: number(root, &["life_tree_level", "lifeTreeLevel"]),
        avatar_url: text(root, &["avatar_url", "avatar", "avatarUrl", "photo"]),
        oracle_uid: text(root, &["uid", "uuid", "oracle_uid"]),
        state: number(root, &["state", "kid", "kingdom"]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_enrichment() {
        let value = json!({
            "player": {
                "fid": "1",
                "name": "Bob",
                "power": "1,234",
                "alliance": { "abbr": "ABC", "name": "Alpha", "rank": 2 },
                "furnaceLevel": 30
            }
        });
        let e = enrichment_from_player(&value);
        assert_eq!(e.game_name.as_deref(), Some("Bob"));
        assert_eq!(e.alliance_abbr.as_deref(), Some("ABC"));
        assert_eq!(e.power, Some(1234));
        assert_eq!(e.town_hall_level, Some(30));
    }

    #[test]
    fn code_validation() {
        assert!(valid_code("ABCD1234"));
        assert!(!valid_code("AB"));
        assert!(!valid_code("AB CD"));
    }
}
