//! Signed WOS gift-code redemption engine.
//!
//! Mirrors the JavaScript `redeemEngine` / `giftCodeValidator` /
//! `giftCodeBroadcast` flow: one signed POST per attempt, the same status
//! classification, throttling, bounded retries, persistence of every attempt,
//! and retry of only eligible failures. No captcha or pre-lookup step exists
//! in the current game API.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use poise::serenity_prelude as serenity;
use serde_json::Value;

use crate::db;

pub mod config {
    pub const SECRET: &str = "tB87#kPtkxqOS2";
    pub const GIFT_CODE_URL: &str = "https://wos-giftcode-api.centurygame.com/api/gift_code";
    pub const ORIGIN: &str = "https://wos-giftcode.centurygame.com";
    pub const RETRY_DELAY_MS: u64 = 3_000;
    pub const RATE_LIMIT_DELAY_MS: u64 = 10_000;
    pub const IP_BLOCK_DELAY_MS: u64 = 60_000;
    pub const MAX_RETRIES: u32 = 3;
    pub const MIN_GLOBAL_INTERVAL_MS: u64 = 1_000;
    pub const MIN_SAME_FID_INTERVAL_MS: u64 = 10_000;
}

/// Statuses that mean the code itself is dead for everyone.
pub const ABORTABLE: &[&str] = &["USED", "TIME ERROR", "CDK NOT FOUND"];
/// Statuses that count as a genuine claim.
pub const SUCCESS_STATUSES: &[&str] = &["SUCCESS", "RECEIVED", "SAME TYPE EXCHANGE"];
const VIP_STATUSES: &[&str] = &["RECHARGE_MONEY ERROR", "RECHARGE_MONEY_VIP ERROR"];
const ACTIVE_STATUSES: &[&str] = &[
    "SUCCESS",
    "RECEIVED",
    "SAME TYPE EXCHANGE",
    "STOVE_LV ERROR",
    "RECHARGE_MONEY ERROR",
    "RECHARGE_MONEY_VIP ERROR",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedeemOutcome {
    pub success: bool,
    pub status: String,
    pub message: String,
    /// `Some(true)` = code still usable, `Some(false)` = dead code,
    /// `None` = inconclusive (rate limit / HTTP / unknown response).
    pub gift_code_active: Option<bool>,
    pub player_not_exist: bool,
    pub rate_limited: bool,
    pub retry_delay_ms: u64,
    pub attempts: u32,
}

impl RedeemOutcome {
    fn error(status: &str, message: impl Into<String>, active: Option<bool>) -> Self {
        Self {
            success: false,
            status: status.to_string(),
            message: message.into(),
            gift_code_active: active,
            player_not_exist: false,
            rate_limited: false,
            retry_delay_ms: 0,
            attempts: 0,
        }
    }
}

fn normalized(message: &str) -> String {
    message
        .to_uppercase()
        .trim_end_matches(|c: char| c.is_whitespace() || c == '.')
        .to_string()
}

/// Map an API response body to the JS status table. Unknown responses are
/// inconclusive (never "dead code") so a valid code is not discarded.
pub fn analyze_api_response(data: &Value) -> RedeemOutcome {
    let raw = data
        .get("msg")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let status = if raw.is_empty() {
        "UNKNOWN_API_RESPONSE".to_string()
    } else {
        normalized(&raw)
    };
    let mut outcome = RedeemOutcome::error(&status, raw.clone(), None);
    match status.as_str() {
        "ROLE NOT EXIST" | "USER INFO ERROR" => {
            outcome.player_not_exist = true;
            outcome.gift_code_active = Some(true);
        }
        "SUCCESS" | "RECEIVED" | "SAME TYPE EXCHANGE" => {
            outcome.success = true;
            outcome.gift_code_active = Some(true);
        }
        "USED" | "TIME ERROR" | "CDK NOT FOUND" => {
            outcome.success = true; // JS treats these as a definitive answer
            outcome.gift_code_active = Some(false);
        }
        "STOVE_LV ERROR" | "RECHARGE_MONEY ERROR" | "RECHARGE_MONEY_VIP ERROR" => {
            outcome.success = true;
            outcome.gift_code_active = Some(true);
        }
        "SIGN ERROR" => {
            outcome.rate_limited = true;
            outcome.retry_delay_ms = config::RETRY_DELAY_MS;
        }
        "TIMEOUT RETRY" => {
            outcome.rate_limited = true;
            outcome.retry_delay_ms = config::RATE_LIMIT_DELAY_MS;
        }
        "TOO FREQUENT" => {
            outcome.rate_limited = true;
            outcome.retry_delay_ms = config::RATE_LIMIT_DELAY_MS * 3;
        }
        _ => {
            // Unknown wording: not success, not "dead code" — inconclusive.
            outcome.gift_code_active = None;
        }
    }
    outcome
}

struct Throttle {
    queue: tokio::sync::Mutex<()>,
    last_any: Mutex<Option<Instant>>,
    last_fid: Mutex<HashMap<String, Instant>>,
}

fn throttle() -> &'static Throttle {
    static THROTTLE: OnceLock<Throttle> = OnceLock::new();
    THROTTLE.get_or_init(|| Throttle {
        queue: tokio::sync::Mutex::new(()),
        last_any: Mutex::new(None),
        last_fid: Mutex::new(HashMap::new()),
    })
}

/// Serialize all redemption requests and space calls to the same FID so the
/// game API does not answer "TOO FREQUENT" / 429.
async fn wait_for_slot(fid: &str) {
    let _guard = throttle().queue.lock().await;
    let last_any = *throttle()
        .last_any
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let last_fid = throttle()
        .last_fid
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(fid)
        .copied();
    let wait = [
        last_any
            .map(|t| config::MIN_GLOBAL_INTERVAL_MS.saturating_sub(t.elapsed().as_millis() as u64)),
        last_fid.map(|t| {
            config::MIN_SAME_FID_INTERVAL_MS.saturating_sub(t.elapsed().as_millis() as u64)
        }),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0);
    if wait > 0 {
        tokio::time::sleep(Duration::from_millis(wait)).await;
    }
    let at = Instant::now();
    *throttle()
        .last_any
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(at);
    throttle()
        .last_fid
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(fid.to_string(), at);
}

fn sign_body(fid: &str, code: &str, state: &str, time: &str) -> String {
    let fields = [("cdk", code), ("fid", fid), ("kid", state), ("time", time)];
    let encoded: Vec<(String, String)> = fields
        .iter()
        .map(|(key, value)| (key.to_string(), urlencoding::encode(value).to_string()))
        .collect();
    let mut canonical: Vec<(&String, &String)> =
        encoded.iter().map(|(key, value)| (key, value)).collect();
    canonical.sort_by(|a, b| a.0.cmp(b.0));
    let joined = canonical
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    let digest = {
        use md5::Digest;
        let mut hasher = md5::Md5::new();
        hasher.update(format!("{joined}{}", config::SECRET));
        hasher.finalize()
    };
    let sign = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let body = canonical
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    format!("sign={sign}&{body}")
}

/// Submit one redemption attempt, retrying rate-limit/network responses up to
/// `MAX_RETRIES`. Every call is spaced by [`wait_for_slot`].
pub async fn redeem_for_player(
    client: &reqwest::Client,
    fid: &str,
    state: Option<i64>,
    code: &str,
) -> RedeemOutcome {
    let Some(state) = state else {
        return RedeemOutcome::error(
            "MISSING_STATE",
            format!("No state on file for FID {fid} — re-register this player with their state."),
            Some(false),
        );
    };
    let mut last = None;
    for attempt in 1..=config::MAX_RETRIES {
        wait_for_slot(fid).await;
        let time = chrono::Utc::now().timestamp().to_string();
        let body = sign_body(fid, code, &state.to_string(), &time);
        let response = client
            .post(config::GIFT_CODE_URL)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("accept", "application/json, text/plain, */*")
            .header("origin", config::ORIGIN)
            .header("referer", format!("{}/", config::ORIGIN))
            .header(
                "user-agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/132.0.0.0 Safari/537.36",
            )
            .header("x-ratelimit-limit", "60")
            .body(body)
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(err) => {
                last = Some(RedeemOutcome::error("HTTP_ERROR", err.to_string(), None));
                tokio::time::sleep(Duration::from_millis(config::RETRY_DELAY_MS)).await;
                continue;
            }
        };
        let status_code = response.status().as_u16();
        if status_code == 429 {
            last = Some(RedeemOutcome::error(
                "HTTP_ERROR",
                "HTTP 429 Too Many Requests",
                None,
            ));
            if attempt < config::MAX_RETRIES {
                tokio::time::sleep(Duration::from_millis(config::IP_BLOCK_DELAY_MS)).await;
                continue;
            }
            let mut outcome = last.unwrap();
            outcome.rate_limited = true;
            outcome.retry_delay_ms = config::IP_BLOCK_DELAY_MS;
            outcome.attempts = attempt;
            return outcome;
        }
        if !(200..300).contains(&status_code) {
            last = Some(RedeemOutcome::error(
                "HTTP_ERROR",
                format!("HTTP {status_code}"),
                None,
            ));
            tokio::time::sleep(Duration::from_millis(config::RETRY_DELAY_MS)).await;
            continue;
        }
        let data: Value = match response.json().await {
            Ok(data) => data,
            Err(err) => {
                last = Some(RedeemOutcome::error("HTTP_ERROR", err.to_string(), None));
                tokio::time::sleep(Duration::from_millis(config::RETRY_DELAY_MS)).await;
                continue;
            }
        };
        let mut outcome = analyze_api_response(&data);
        outcome.attempts = attempt;
        if outcome.rate_limited && attempt < config::MAX_RETRIES {
            let delay = outcome.retry_delay_ms;
            last = Some(outcome);
            tokio::time::sleep(Duration::from_millis(delay)).await;
            continue;
        }
        return outcome;
    }
    let mut outcome = last.unwrap_or_else(|| {
        RedeemOutcome::error(
            "MAX_ATTEMPTS_EXCEEDED",
            "Maximum retry attempts exceeded",
            None,
        )
    });
    outcome.attempts = config::MAX_RETRIES;
    outcome
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    Active,
    Invalid,
    Network,
    NoProbePlayer,
}

#[derive(Debug, Clone)]
pub struct Validation {
    pub disposition: Disposition,
    pub status: String,
    pub is_vip: bool,
    pub message: String,
}

/// Result of checking whether a FID and state pair identifies a real player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeFidState {
    Valid,
    Invalid,
    Unavailable,
}

/// Verify a FID/state pair without redeeming a real code. The API checks the
/// player before validating the deliberately nonexistent code, so only
/// `CDK NOT FOUND` is a positive verification; uncertain responses fail closed.
pub async fn probe_fid_state(
    client: &reqwest::Client,
    fid: &str,
    state: i64,
    attempts: usize,
    delay: Duration,
) -> ProbeFidState {
    let attempts = attempts.clamp(1, 5);
    for attempt in 0..attempts {
        let fake_code = format!("zz{:016x}", rand::random::<u64>());
        let outcome = redeem_for_player(client, fid, Some(state), &fake_code).await;
        if outcome.player_not_exist {
            return ProbeFidState::Invalid;
        }
        if outcome.status == "CDK NOT FOUND" {
            return ProbeFidState::Valid;
        }
        if attempt + 1 < attempts {
            tokio::time::sleep(delay).await;
        }
    }
    ProbeFidState::Unavailable
}

/// Validate a code against the live API using a configured probe player (or
/// any registered player with a state). Unclear responses are `Network`, so a
/// possibly-good code is never discarded.
pub async fn validate_code(data: &crate::state::Data, guild_id: u64, code: &str) -> Validation {
    let probe = crate::db::giftcodes::probe_player(&data.db, guild_id).await;
    validate_code_with_probe(data, probe, code).await
}

pub async fn validate_code_with_probe(
    data: &crate::state::Data,
    probe: Option<(String, i64)>,
    code: &str,
) -> Validation {
    let Some((fid, state)) = probe else {
        return Validation {
            disposition: Disposition::NoProbePlayer,
            status: String::new(),
            is_vip: false,
            message: "No player with state registered for validation.".to_string(),
        };
    };
    let outcome = redeem_for_player(data.gift_http(), &fid, Some(state), code).await;
    let status = outcome.status.clone();
    let disposition = if outcome.player_not_exist {
        Disposition::Network
    } else if ABORTABLE.contains(&status.as_str()) || outcome.gift_code_active == Some(false) {
        Disposition::Invalid
    } else if outcome.success
        && (ACTIVE_STATUSES.contains(&status.as_str()) || outcome.gift_code_active == Some(true))
    {
        Disposition::Active
    } else {
        Disposition::Network
    };
    Validation {
        disposition,
        status,
        is_vip: VIP_STATUSES.contains(&outcome.status.as_str()),
        message: outcome.message,
    }
}

#[derive(Debug, Clone, Default)]
pub struct RedeemReport {
    pub code: String,
    pub succeeded: Vec<String>,
    pub already_claimed: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<(String, String)>,
    /// FIDs explicitly rejected by the game as nonexistent or paired with the wrong state.
    pub player_not_exist: Vec<String>,
    pub aborted: Option<String>,
    pub total: usize,
}

impl RedeemReport {
    pub fn summary(&self) -> String {
        format!(
            "✅ {} redeemed · ℹ️ {} already claimed · ❌ {} failed · ⏭️ {} skipped",
            self.succeeded.len(),
            self.already_claimed.len(),
            self.failed.len(),
            self.skipped.len()
        )
    }
}

/// Redeem a code for a set of `(fid, state)` players: skip recorded attempts,
/// persist every new outcome, abort the whole batch when the code is dead, and
/// auto-retry transient failures (bounded passes).
pub async fn redeem_code_for_players(
    data: &crate::state::Data,
    code: &str,
    players: &[(String, Option<i64>)],
    skip_recorded: bool,
) -> anyhow::Result<RedeemReport> {
    redeem_code_for_players_with_progress(data, code, players, skip_recorded, None).await
}

#[derive(Clone)]
pub struct ProgressTarget {
    pub ctx: serenity::Context,
    pub channel_id: u64,
    pub message_id: u64,
}

async fn update_progress(target: &ProgressTarget, code: &str, report: &RedeemReport, done: usize) {
    let embed = serenity::CreateEmbed::new()
        .color(0xF1C40F)
        .title(format!("⏳ Gift-code redemption — `{code}`"))
        .description(format!(
            "Players processed: **{done}/{}**\n{}",
            report.total,
            report.summary()
        ));
    let _ = serenity::ChannelId::new(target.channel_id)
        .edit_message(
            &target.ctx.http,
            serenity::MessageId::new(target.message_id),
            serenity::EditMessage::new().embed(embed),
        )
        .await;
}

pub async fn redeem_code_for_players_with_progress(
    data: &crate::state::Data,
    code: &str,
    players: &[(String, Option<i64>)],
    skip_recorded: bool,
    progress: Option<ProgressTarget>,
) -> anyhow::Result<RedeemReport> {
    let mut report = RedeemReport {
        code: code.to_string(),
        total: players.len(),
        ..Default::default()
    };
    let already_done = if skip_recorded {
        db::giftcodes::fids_redeemed_for_code(&data.db, code).await?
    } else {
        std::collections::HashSet::new()
    };
    let mut failed: Vec<(String, Option<i64>)> = Vec::new();
    for (fid, state) in players {
        if already_done.contains(fid) {
            report.skipped.push(fid.clone());
            if let Some(target) = &progress {
                update_progress(
                    target,
                    code,
                    &report,
                    report.succeeded.len()
                        + report.already_claimed.len()
                        + report.failed.len()
                        + report.skipped.len(),
                )
                .await;
            }
            continue;
        }
        let outcome = redeem_for_player(data.gift_http(), fid, *state, code).await;
        if outcome.player_not_exist && !report.player_not_exist.contains(fid) {
            report.player_not_exist.push(fid.clone());
        }
        let status = if outcome.status.is_empty() {
            "ERROR".to_string()
        } else {
            outcome.status.clone()
        };
        db::giftcodes::record_redemption(&data.db, fid, code, &status).await?;
        if status == "SUCCESS" {
            report.succeeded.push(fid.clone());
        } else if SUCCESS_STATUSES.contains(&status.as_str()) {
            report.already_claimed.push(fid.clone());
        } else {
            report.failed.push((fid.clone(), outcome.message.clone()));
            failed.push((fid.clone(), *state));
        }
        if ABORTABLE.contains(&status.as_str()) {
            report.aborted = Some(status.clone());
            if status == "CDK NOT FOUND" {
                let _ = db::giftcodes::set_status(&data.db, code, "invalid").await;
            } else {
                let _ = db::giftcodes::set_status(&data.db, code, "expired").await;
            }
            break;
        }
        if let Some(target) = &progress {
            let done = report.succeeded.len()
                + report.already_claimed.len()
                + report.failed.len()
                + report.skipped.len();
            update_progress(target, code, &report, done).await;
        }
    }
    // Bounded auto-retry of failed players (JS: two extra passes).
    for _ in 0..2 {
        if failed.is_empty() || report.aborted.is_some() {
            break;
        }
        let mut still_failing = Vec::new();
        for (fid, state) in failed.drain(..) {
            let outcome = redeem_for_player(data.gift_http(), &fid, state, code).await;
            if outcome.player_not_exist && !report.player_not_exist.contains(&fid) {
                report.player_not_exist.push(fid.clone());
            }
            let status = if outcome.status.is_empty() {
                "ERROR".to_string()
            } else {
                outcome.status.clone()
            };
            db::giftcodes::record_redemption(&data.db, &fid, code, &status).await?;
            report.failed.retain(|(failed_fid, _)| failed_fid != &fid);
            if status == "SUCCESS" {
                report.succeeded.push(fid);
            } else if SUCCESS_STATUSES.contains(&status.as_str()) {
                report.already_claimed.push(fid);
            } else {
                report.failed.push((fid.clone(), outcome.message));
                still_failing.push((fid, state));
            }
        }
        failed = still_failing;
    }
    if let Some(target) = &progress {
        let done = report.succeeded.len()
            + report.already_claimed.len()
            + report.failed.len()
            + report.skipped.len();
        update_progress(target, code, &report, done).await;
    }
    Ok(report)
}

/// Retry only players with a recorded, non-successful attempt for this code.
pub async fn retry_failed(
    data: &crate::state::Data,
    code: &str,
    guild_id: u64,
) -> anyhow::Result<RedeemReport> {
    let Some(stored) = db::giftcodes::get(&data.db, code).await? else {
        return Ok(RedeemReport {
            code: code.to_string(),
            aborted: Some("CODE NOT FOUND".to_string()),
            ..Default::default()
        });
    };
    if !stored.is_active() {
        return Ok(RedeemReport {
            code: code.to_string(),
            aborted: Some(format!("CODE {}", stored.status.to_ascii_uppercase())),
            ..Default::default()
        });
    }
    let statuses: HashMap<String, String> = db::giftcodes::statuses_for_code(&data.db, code)
        .await?
        .into_iter()
        .collect();
    let players: Vec<(String, Option<i64>)> = db::players::list_for_guild(&data.db, guild_id)
        .await?
        .into_iter()
        .filter(|player| {
            statuses
                .get(&player.fid)
                .is_some_and(|status| !SUCCESS_STATUSES.contains(&status.as_str()))
        })
        .map(|player| (player.fid, player.state))
        .collect();
    let mut report = redeem_code_for_players(data, code, &players, false).await?;
    // Retry targets were already attempted, so they must actually be retried.
    report.skipped.clear();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn status_mapping_matches_javascript() {
        let cases = [
            ("SUCCESS", true, Some(true), false),
            ("RECEIVED", true, Some(true), false),
            ("SAME TYPE EXCHANGE", true, Some(true), false),
            ("USED", true, Some(false), false),
            ("TIME ERROR.", true, Some(false), false),
            ("CDK NOT FOUND", true, Some(false), false),
            ("STOVE_LV ERROR", true, Some(true), false),
            ("RECHARGE_MONEY ERROR", true, Some(true), false),
            ("USER INFO ERROR.", false, Some(true), true),
            ("ROLE NOT EXIST", false, Some(true), true),
        ];
        for (message, success, active, missing) in cases {
            let outcome = analyze_api_response(&json!({ "msg": message }));
            assert_eq!(outcome.status, normalized(message), "{message}");
            assert_eq!(outcome.success, success, "{message}");
            assert_eq!(outcome.gift_code_active, active, "{message}");
            assert_eq!(outcome.player_not_exist, missing, "{message}");
        }
    }

    #[test]
    fn rate_limits_are_retryable_not_dead_codes() {
        for message in ["SIGN ERROR", "TIMEOUT RETRY", "TOO FREQUENT"] {
            let outcome = analyze_api_response(&json!({ "msg": message }));
            assert!(outcome.rate_limited, "{message}");
            assert!(!outcome.success, "{message}");
            assert!(outcome.retry_delay_ms > 0, "{message}");
        }
    }

    #[test]
    fn unknown_response_is_inconclusive() {
        let outcome = analyze_api_response(&json!({ "msg": "some new wording" }));
        assert!(!outcome.success);
        assert_eq!(outcome.gift_code_active, None);
    }

    #[test]
    fn signature_is_sorted_and_md5() {
        let body = sign_body("123", "CODE", "3105", "1000");
        assert!(body.starts_with("sign="));
        assert!(body.contains("cdk=CODE"));
        assert!(body.contains("fid=123"));
        assert!(body.contains("kid=3105"));
        assert!(body.contains("time=1000"));
        let sign = &body["sign=".len()..body.find('&').unwrap()];
        assert_eq!(sign.len(), 32);
    }
}
