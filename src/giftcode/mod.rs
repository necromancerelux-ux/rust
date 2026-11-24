//! Gift-code synchronisation shared by commands, buttons and the background job.

pub mod community;
pub mod redeem;
pub mod wostools;

use poise::serenity_prelude as serenity;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use crate::db::{self, settings::keys};
use crate::state::Data;
use crate::ui::panels;

const SECONDARY_REJECTION_TTL: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);
const ANNOUNCEMENT_LEASE_SECONDS: i64 = 120;

fn announcement_nonce(guild_id: u64, channel_id: u64, codes: &[String]) -> u64 {
    use sha2::{Digest, Sha256};
    let mut ordered = codes.to_vec();
    ordered.sort();
    let mut hasher = Sha256::new();
    hasher.update(guild_id.to_be_bytes());
    hasher.update(channel_id.to_be_bytes());
    for code in ordered {
        hasher.update((code.len() as u64).to_be_bytes());
        hasher.update(code.as_bytes());
    }
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[..8].try_into().expect("sha256 has 8 bytes"))
}

fn secondary_rejections() -> &'static Mutex<HashMap<String, std::time::Instant>> {
    static REJECTED: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
    REJECTED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn was_secondary_candidate_rejected(code: &str) -> bool {
    let mut rejected = secondary_rejections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    rejected.retain(|_, rejected_at| rejected_at.elapsed() < SECONDARY_REJECTION_TTL);
    rejected.contains_key(&code.to_ascii_uppercase())
}

fn remember_rejected_secondary_candidate(code: &str) {
    let key = code.to_ascii_uppercase();
    let mut rejected = secondary_rejections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    rejected.retain(|_, rejected_at| rejected_at.elapsed() < SECONDARY_REJECTION_TTL);
    if rejected.len() >= 2048 && !rejected.contains_key(&key) {
        if let Some(oldest) = rejected
            .iter()
            .max_by_key(|(_, rejected_at)| rejected_at.elapsed())
            .map(|(code, _)| code.clone())
        {
            rejected.remove(&oldest);
        }
    }
    rejected.insert(key, std::time::Instant::now());
}

#[derive(Debug, Clone, Default)]
pub struct SyncOutcome {
    /// Number of unique active codes known after this sync.
    pub total: usize,
    /// Codes that were not in the database before this run.
    pub fresh: Vec<String>,
    /// How many channels the new codes were posted to.
    pub announcements: usize,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct SyncFailure {
    pub message: String,
    pub retry_after: Option<std::time::Duration>,
}

fn guild_redemption_queue(guild_id: u64) -> Arc<tokio::sync::Mutex<()>> {
    static QUEUES: OnceLock<Mutex<HashMap<u64, Weak<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let mut queues = QUEUES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    queues.retain(|_, queue| queue.strong_count() > 0);
    if let Some(queue) = queues.get(&guild_id).and_then(Weak::upgrade) {
        return queue;
    }
    let queue = Arc::new(tokio::sync::Mutex::new(()));
    queues.insert(guild_id, Arc::downgrade(&queue));
    queue
}

fn sync_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn announcement_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn auto_redeem_inflight() -> &'static Mutex<HashSet<String>> {
    static CODES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    CODES.get_or_init(|| Mutex::new(HashSet::new()))
}

struct AutoRedeemGuard(Vec<String>);

impl Drop for AutoRedeemGuard {
    fn drop(&mut self) {
        let mut inflight = auto_redeem_inflight()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for code in &self.0 {
            inflight.remove(code);
        }
        AUTO_REDEEM_WORKER.store(false, Ordering::Release);
    }
}

static AUTO_REDEEM_WORKER: AtomicBool = AtomicBool::new(false);

fn is_auto_redeem_inflight(code: &str) -> bool {
    auto_redeem_inflight()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(code)
}

/// Merge Oracle, WoSTools and the optional community feed; validate secondary
/// candidates before storing them, then announce newly discovered active codes.
pub async fn sync(data: &Data) -> anyhow::Result<SyncOutcome> {
    let _sync_guard = sync_lock().lock().await;
    let oracle_enabled =
        !std::env::var("GIFTCODE_WOSORACLE_ENABLED").is_ok_and(|value| value == "0");
    let wosoracle_primary =
        std::env::var("GIFTCODE_WOSORACLE_PRIMARY").is_ok_and(|value| value == "1");
    let (mut oracle_result, mut wostools_result, community_result) = tokio::join!(
        async {
            if oracle_enabled {
                data.oracle
                    .fetch_codes()
                    .await
                    .map_err(|err| err.to_string())
            } else {
                Ok(Vec::new())
            }
        },
        wostools::fetch_codes(),
        community::fetch_codes()
    );
    if wosoracle_primary {
        if matches!(&oracle_result, Ok(rows) if !rows.is_empty()) {
            wostools_result = Ok(Vec::new());
        }
    } else if matches!(&wostools_result, Ok(codes) if !codes.is_empty()) {
        oracle_result = Ok(Vec::new());
    }
    let community_succeeded = community::configured()
        && !std::env::var("GIFTCODE_SYNC_ENABLED").is_ok_and(|value| value == "0")
        && community_result.is_ok();
    let secondary_has_codes = matches!(&wostools_result, Ok(codes) if !codes.is_empty())
        || matches!(&oracle_result, Ok(rows) if !rows.is_empty());
    if !secondary_has_codes && !community_succeeded {
        let message = format!(
            "gift-code sources unavailable: Oracle: {}; WoSTools: {}; community feed: {}",
            oracle_result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unknown error".into()),
            wostools_result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_else(|| "no active codes found".into()),
            community_result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_else(|| "not configured or no candidate codes".into())
        );
        let retry_after = community_result
            .as_ref()
            .err()
            .and_then(|err| err.downcast_ref::<community::HttpStatusError>())
            .and_then(|err| err.retry_after);
        return Err(SyncFailure {
            message,
            retry_after,
        }
        .into());
    }

    let mut active = Vec::new();
    let mut fresh = Vec::new();
    if let Ok(rows) = oracle_result {
        for row in rows.iter().filter(|row| !row.active) {
            if let Err(err) = db::giftcodes::set_status(&data.db, &row.code, "expired").await {
                tracing::debug!(error = %err, code = %row.code, "marking gift code expired failed");
            }
        }
        let active_rows: Vec<_> = rows.into_iter().filter(|row| row.active).collect();
        let oracle_active_count = active_rows.len();
        let probe = db::giftcodes::probe_player(&data.db, 0).await;
        for row in active_rows {
            if db::giftcodes::get(&data.db, &row.code).await?.is_some() {
                active.push(row.code);
                continue;
            }
            let Some(probe) = probe.clone() else {
                tracing::warn!("Oracle code candidates skipped: no probe player is configured");
                break;
            };
            let validation = redeem::validate_code_with_probe(data, Some(probe), &row.code).await;
            if validation.disposition != redeem::Disposition::Active {
                if validation.disposition == redeem::Disposition::Invalid {
                    remember_rejected_secondary_candidate(&row.code);
                }
                tracing::debug!(code = %row.code, status = %validation.status, "Oracle code candidate rejected or inconclusive");
                continue;
            }
            if db::giftcodes::insert(
                &data.db,
                &row.code,
                0,
                0,
                "wosoracle",
                row.date.as_deref(),
                validation.is_vip,
            )
            .await?
            {
                active.push(row.code.clone());
                fresh.push(row.code);
            }
        }
        db::oracle::touch_codes_sync(&data.db, oracle_active_count as i64).await?;
    }

    if let Ok(candidates) = wostools_result {
        if !candidates.is_empty() {
            let probe = db::giftcodes::probe_player(&data.db, 0).await;
            if probe.is_none() {
                tracing::warn!("WoSTools candidates skipped: no probe player is configured");
            } else {
                let today = chrono::Utc::now().date_naive().to_string();
                for code in candidates {
                    if was_secondary_candidate_rejected(&code) {
                        continue;
                    }
                    if let Some(existing) = db::giftcodes::get(&data.db, &code).await? {
                        if existing.source.as_deref() == Some("wosoracle") && !wosoracle_primary {
                            let today = chrono::Utc::now().date_naive().to_string();
                            db::giftcodes::update_source_metadata(
                                &data.db,
                                &code,
                                "wostools",
                                Some(&today),
                                false,
                            )
                            .await?;
                        }
                        continue;
                    }
                    let validation =
                        redeem::validate_code_with_probe(data, probe.clone(), &code).await;
                    if validation.disposition != redeem::Disposition::Active {
                        if validation.disposition == redeem::Disposition::Invalid {
                            remember_rejected_secondary_candidate(&code);
                        }
                        tracing::debug!(code = %code, status = %validation.status, "WoSTools candidate rejected or inconclusive");
                        continue;
                    }
                    if db::giftcodes::insert(
                        &data.db,
                        &code,
                        0,
                        0,
                        "wostools",
                        Some(&today),
                        validation.is_vip,
                    )
                    .await?
                    {
                        active.push(code.clone());
                        fresh.push(code);
                    }
                }
            }
        }
    }

    if let Ok(candidates) = community_result {
        if !candidates.is_empty() {
            let probe = db::giftcodes::probe_player(&data.db, 0).await;
            if probe.is_none() {
                tracing::warn!(
                    "community gift-code candidates skipped: no probe player is configured"
                );
            } else {
                for candidate in candidates {
                    if was_secondary_candidate_rejected(&candidate.code) {
                        continue;
                    }
                    if let Some(existing) = db::giftcodes::get(&data.db, &candidate.code).await? {
                        if matches!(
                            existing.source.as_deref(),
                            Some("wosoracle" | "wostools" | "whiteout-bot")
                        ) {
                            db::giftcodes::update_source_metadata(
                                &data.db,
                                &candidate.code,
                                "api",
                                candidate.date.as_deref(),
                                true,
                            )
                            .await?;
                        }
                        continue;
                    }
                    let validation =
                        redeem::validate_code_with_probe(data, probe.clone(), &candidate.code)
                            .await;
                    if validation.disposition != redeem::Disposition::Active {
                        if validation.disposition == redeem::Disposition::Invalid {
                            remember_rejected_secondary_candidate(&candidate.code);
                        }
                        tracing::debug!(code = %candidate.code, status = %validation.status, "community gift-code candidate rejected or inconclusive");
                        continue;
                    }
                    if db::giftcodes::insert(
                        &data.db,
                        &candidate.code,
                        0,
                        0,
                        "api",
                        candidate.date.as_deref(),
                        validation.is_vip,
                    )
                    .await?
                    {
                        active.push(candidate.code.clone());
                        fresh.push(candidate.code);
                    }
                }
            }
        }
    }

    let total = active.len();

    // Announce everything still unannounced (not just this run's new codes), so a code whose
    // first announcement reached no channel is retried on the next sync. The 7-day window
    // keeps an old backlog from flooding a channel that was only just configured.
    // Newly discovered codes are excluded here and announced by their background task only
    // after auto-redemption completes, matching the JS order without keeping sync interactions
    // open for the duration of every guild's roster.
    let cutoff = db::now() - 7 * 24 * 3600;
    let unannounced = db::giftcodes::unannounced(&data.db, 500).await?;
    let fresh_set: HashSet<&str> = fresh.iter().map(String::as_str).collect();
    let mut auto_redeem = Vec::new();
    let mut pending = Vec::new();
    for row in unannounced.into_iter().filter(|row| {
        row.is_active() && (row.added_at >= cutoff || row.redemption_handoff_at.is_some())
    }) {
        let source_is_automatic = matches!(
            row.source.as_deref(),
            Some("api" | "wostools" | "wosoracle" | "oracle" | "whiteout-bot")
        );
        let rust_owned = row.added_by == "0"
            || row.source_guild_id == "0"
            || row.redemption_handoff_at.is_some();
        if source_is_automatic && !rust_owned && !fresh_set.contains(row.code.as_str()) {
            // Let the JS runtime finish auto-redemption for a code it inserted.
            continue;
        }
        if (source_is_automatic && rust_owned && row.announced == 0)
            || fresh_set.contains(row.code.as_str())
        {
            auto_redeem.push(row.code);
        } else {
            pending.push(row.code);
        }
    }
    for code in &fresh {
        if !auto_redeem.contains(code) {
            auto_redeem.push(code.clone());
        }
    }
    spawn_auto_redemption(data, auto_redeem, fresh.clone());
    let announcements = if pending.is_empty() {
        0
    } else {
        match announce(data, &pending).await {
            Ok(delivered) => delivered,
            Err(err) => {
                tracing::warn!(error = %err, "gift-code announcement failed; will retry next sync");
                0
            }
        }
    };

    Ok(SyncOutcome {
        total,
        fresh,
        announcements,
    })
}

async fn notify_owner_new_code(data: &Data, code: &str, source: &str) {
    let owner_id = match data.cfg.owner_id {
        Some(owner_id) => Some(owner_id),
        None => db::admin::owner_id(&data.db).await.ok().flatten(),
    };
    let Some(owner_id) = owner_id else {
        return;
    };
    let source_label = match source {
        "wostools" => "Detected via wostools.net",
        "wosoracle" => "Detected via wosoracle.com",
        _ => "Detected via community sync",
    };
    let embed = serenity::CreateEmbed::new()
        .color(0x2ECC71)
        .title("🎁 New Gift Code Found")
        .description(format!(
            "A new code was detected: `{code}`\nAuto-redemption has completed for registered players."
        ))
        .footer(serenity::CreateEmbedFooter::new(source_label))
        .timestamp(serenity::Timestamp::now());
    let user = serenity::UserId::new(owner_id);
    match user.create_dm_channel(&data.discord_http).await {
        Ok(channel) => {
            if let Err(err) = channel
                .send_message(
                    &data.discord_http,
                    serenity::CreateMessage::new().embed(embed),
                )
                .await
            {
                tracing::debug!(error = %err, "could not DM owner about newly synced gift code");
            }
        }
        Err(err) => {
            tracing::debug!(error = %err, "could not open owner DM for newly synced gift code")
        }
    }
}

fn spawn_auto_redemption(data: &Data, codes: Vec<String>, notify_owner: Vec<String>) {
    if codes.is_empty()
        || AUTO_REDEEM_WORKER
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return;
    }
    let claimed = {
        let mut inflight = auto_redeem_inflight()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut claimed = Vec::new();
        for code in codes {
            if inflight.insert(code.clone()) {
                claimed.push(code);
            }
        }
        claimed
    };
    if claimed.is_empty() {
        AUTO_REDEEM_WORKER.store(false, Ordering::Release);
        return;
    }
    let data = data.clone();
    tokio::spawn(async move {
        let _guard = AutoRedeemGuard(claimed.clone());
        if let Err(err) = redeem_fresh_codes(&data, &claimed).await {
            tracing::warn!(error = %err, "automatic redemption of new gift codes failed");
            return;
        }
        let mut deliverable = Vec::new();
        for code in &claimed {
            let stored = db::giftcodes::get(&data.db, code).await.ok().flatten();
            if let Some(stored) = stored.filter(|stored| stored.is_active()) {
                if notify_owner.iter().any(|new_code| new_code == code) {
                    notify_owner_new_code(&data, code, stored.source.as_deref().unwrap_or("api"))
                        .await;
                }
                deliverable.push(code.clone());
            }
        }
        if let Err(err) = announce_after_redemption(&data, &deliverable).await {
            tracing::warn!(error = %err, "post-redemption gift-code announcement failed");
        }
    });
}

async fn redeem_fresh_codes(data: &Data, codes: &[String]) -> anyhow::Result<()> {
    let guilds = db::players::guilds_with_players(&data.db).await?;
    let guilds = db::subscriptions::order_guilds_by_gift_priority(&data.db, guilds).await;
    for code in codes {
        for guild_id in &guilds {
            let queue = guild_redemption_queue(*guild_id);
            let _guard = queue.lock().await;
            let players = db::players::list_for_guild(&data.db, *guild_id)
                .await?
                .into_iter()
                .map(|player| (player.fid, player.state))
                .collect::<Vec<_>>();
            if players.is_empty() {
                continue;
            }
            if db::giftcodes::get(&data.db, code)
                .await?
                .is_none_or(|stored| !stored.is_active())
            {
                break;
            }
            match redeem::redeem_code_for_players(data, code, &players, true).await {
                Ok(report) => {
                    let embed = serenity::CreateEmbed::new()
                        .color(0xF1C40F)
                        .title(format!("🎁 Automatic gift-code redemption — `{code}`"))
                        .description(format!(
                            "{}{}",
                            report.summary(),
                            report
                                .aborted
                                .as_ref()
                                .map(|status| format!(
                                    "\\n⛔ Stopped early: **{status}** — code treated as dead."
                                ))
                                .unwrap_or_default()
                        ))
                        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));
                    if db::logging::is_enabled(&data.db, *guild_id, "giftcode").await {
                        if let Ok(Some(channel_id)) =
                            db::logging::channel_id(&data.db, *guild_id).await
                        {
                            let _ = serenity::ChannelId::new(channel_id)
                                .send_message(
                                    &data.discord_http,
                                    serenity::CreateMessage::new().embed(embed),
                                )
                                .await;
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(guild_id, code, error = %err, "automatic gift-code redemption failed for guild")
                }
            }
        }
    }
    Ok(())
}

/// Post `codes` into every guild that configured an announcement channel.
pub async fn announce(data: &Data, codes: &[String]) -> anyhow::Result<usize> {
    announce_inner(data, codes, false).await
}

async fn announce_after_redemption(data: &Data, codes: &[String]) -> anyhow::Result<usize> {
    announce_inner(data, codes, true).await
}

async fn announce_inner(
    data: &Data,
    codes: &[String],
    allow_inflight: bool,
) -> anyhow::Result<usize> {
    if codes.is_empty() {
        return Ok(0);
    }
    let _announcement_guard = announcement_lock().lock().await;
    let mut active_codes = Vec::new();
    for code in codes {
        if !allow_inflight && is_auto_redeem_inflight(code) {
            continue;
        }
        if db::giftcodes::get(&data.db, code)
            .await?
            .is_some_and(|row| row.is_active() && row.announcement_complete == 0)
            && db::giftcodes::claim_announcement(&data.db, code, ANNOUNCEMENT_LEASE_SECONDS).await?
        {
            active_codes.push(code.clone());
        }
    }
    if active_codes.is_empty() {
        return Ok(0);
    }

    let channels = db::settings::guilds_with_key(&data.db, keys::GIFT_ANNOUNCE_CHANNEL).await?;
    // Owner-set gift priority: lower number first; unconfigured guilds last.
    let mut channels = channels;
    let guild_ids: Vec<u64> = channels.iter().map(|(guild, _)| *guild).collect();
    let guild_ids = db::subscriptions::order_guilds_by_gift_priority(&data.db, guild_ids).await;
    let priority: std::collections::HashMap<u64, usize> = guild_ids
        .iter()
        .enumerate()
        .map(|(index, guild)| (*guild, index))
        .collect();
    channels.sort_by_key(|(guild, _)| priority.get(guild).copied().unwrap_or(usize::MAX));
    let components = vec![serenity::CreateActionRow::Buttons(vec![
        crate::ui::components::success("gift:use", "🎁 Redeem Code"),
        crate::ui::components::primary("player:register", "🪪 Register My ID"),
        crate::ui::components::secondary("player:remove", "🗑️ Remove My ID"),
        crate::ui::components::secondary("player:nickname", "🏷️ In-Game Name"),
        crate::ui::components::secondary("player:translate", "🌐 Translate"),
    ])];
    let mut delivered = 0usize;

    for (_guild, raw) in &channels {
        let Ok(channel_id) = raw.parse::<u64>() else {
            continue;
        };
        let template = db::settings::get(&data.db, *_guild, keys::GIFT_ANNOUNCE_TEMPLATE)
            .await
            .ok()
            .flatten();
        let mut guild_codes = Vec::new();
        for code in &active_codes {
            if !db::giftcodes::announced_to_guild(&data.db, code, *_guild).await? {
                guild_codes.push(code.clone());
            }
        }
        if guild_codes.is_empty() {
            continue;
        }
        for code in &guild_codes {
            db::giftcodes::renew_announcement_claim(&data.db, code, ANNOUNCEMENT_LEASE_SECONDS)
                .await?;
        }
        let embed = announcement_embed(&guild_codes, template.as_deref());
        let nonce = announcement_nonce(*_guild, channel_id, &guild_codes);
        let mut send_result = Err(anyhow::anyhow!("announcement send was not attempted"));
        for attempt in 0..2 {
            send_result = serenity::ChannelId::new(channel_id)
                .send_message(
                    &data.discord_http,
                    serenity::CreateMessage::new()
                        .embed(embed.clone())
                        .components(components.clone())
                        .nonce(serenity::model::channel::Nonce::Number(nonce))
                        .enforce_nonce(true),
                )
                .await
                .map_err(anyhow::Error::new);
            if send_result.is_ok() {
                break;
            }
            if attempt == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            }
        }
        match send_result {
            Ok(_) => {
                delivered += 1;
                db::giftcodes::mark_codes_announced_to_guild(&data.db, &guild_codes, *_guild)
                    .await?;
            }
            Err(err) => {
                tracing::warn!(error = %err, channel = channel_id, "gift-code announcement failed")
            }
        }
    }

    let configured_guilds: Vec<u64> = channels
        .iter()
        .filter_map(|(guild, channel)| channel.parse::<u64>().ok().map(|_| *guild))
        .collect();
    for code in &active_codes {
        if !configured_guilds.is_empty() {
            let mut complete = true;
            for guild_id in &configured_guilds {
                if !db::giftcodes::announced_to_guild(&data.db, code, *guild_id).await? {
                    complete = false;
                    break;
                }
            }
            if complete {
                db::giftcodes::finish_announcement(&data.db, code).await?;
            } else {
                let mut any_delivered = false;
                for guild_id in &configured_guilds {
                    if db::giftcodes::announced_to_guild(&data.db, code, *guild_id).await? {
                        any_delivered = true;
                        break;
                    }
                }
                if !any_delivered {
                    db::giftcodes::release_announcement_claim(&data.db, code).await?;
                }
            }
        } else {
            db::giftcodes::release_announcement_claim(&data.db, code).await?;
        }
    }

    Ok(delivered)
}

/// Build the default gift-code announcement or apply the guild's description
/// template. Supported tokens are `{codes}` and `{count}`.
pub fn announcement_embed(codes: &[String], template: Option<&str>) -> serenity::CreateEmbed {
    let Some(template) = template.map(str::trim).filter(|value| !value.is_empty()) else {
        return panels::giftcode_announce(codes);
    };
    let code_lines = codes
        .iter()
        .map(|code| format!("🎁 `{code}`"))
        .collect::<Vec<_>>()
        .join("\n");
    let description = template
        .replace("{codes}", &code_lines)
        .replace("{count}", &codes.len().to_string());
    serenity::CreateEmbed::new()
        .color(0xF1C40F)
        .title("🎁 New gift codes")
        .description(description)
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()))
}

/// Post every code that has never been announced (used by the Gift Codes panel's Announce button).
pub async fn announce_pending(data: &Data, limit: i64) -> anyhow::Result<usize> {
    // Serialize the manual action with source insertion so it cannot publish a
    // newly inserted automatic code in the gap before its redemption job claims it.
    let _sync_guard = sync_lock().lock().await;
    let pending = db::giftcodes::unannounced(&data.db, limit).await?;
    let codes = pending
        .into_iter()
        .filter(|row| !is_auto_redeem_inflight(&row.code))
        .map(|row| row.code)
        .collect::<Vec<_>>();
    announce(data, &codes).await
}

pub async fn retry_expired_announcements(data: &Data) -> anyhow::Result<usize> {
    let codes = db::giftcodes::expired_announcement_claims(&data.db, db::now(), 100).await?;
    announce_after_redemption(data, &codes).await
}

/// Called only after the bot owner confirms that the legacy JS broadcaster is stopped.
pub async fn handoff_js_pending(data: &Data, owner_id: u64) -> anyhow::Result<usize> {
    let codes = db::giftcodes::handoff_js_pending(&data.db, owner_id).await?;
    let count = codes.len();
    spawn_auto_redemption(data, codes, Vec::new());
    Ok(count)
}

/// Redeem a code for every player homed to a guild and write a summary to the
/// server's configured log channel. Runs in the background so a large guild
/// never blocks the interaction that triggered it.
pub async fn redeem_and_log(
    ctx: &serenity::Context,
    data: &Data,
    code: &str,
    guild_id: u64,
) -> anyhow::Result<redeem::RedeemReport> {
    let players: Vec<(String, Option<i64>)> = db::players::list_for_guild(&data.db, guild_id)
        .await?
        .into_iter()
        .map(|player| (player.fid, player.state))
        .collect();
    let progress_target = db::settings::get(&data.db, guild_id, keys::LOG_CHANNEL)
        .await
        .ok()
        .flatten()
        .and_then(|raw| raw.parse::<u64>().ok());
    let progress_target = if let Some(channel_id) = progress_target {
        serenity::ChannelId::new(channel_id)
            .send_message(
                &ctx.http,
                serenity::CreateMessage::new().embed(
                    serenity::CreateEmbed::new()
                        .color(0xF1C40F)
                        .title(format!("⏳ Gift-code redemption queued — `{code}`"))
                        .description(format!(
                            "Waiting for this server's current redemption to finish. **{}** player(s) queued.",
                            players.len()
                        )),
                ),
            )
            .await
            .ok()
            .map(|message| redeem::ProgressTarget {
                ctx: ctx.clone(),
                channel_id,
                message_id: message.id.get(),
            })
    } else {
        None
    };
    let queue = guild_redemption_queue(guild_id);
    let _queue_guard = queue.lock().await;
    if let Some(target) = &progress_target {
        let _ = serenity::ChannelId::new(target.channel_id)
            .edit_message(
                &ctx.http,
                serenity::MessageId::new(target.message_id),
                serenity::EditMessage::new().embed(
                    serenity::CreateEmbed::new()
                        .color(0xF1C40F)
                        .title(format!("⏳ Gift-code redemption running — `{code}`"))
                        .description(format!(
                            "Starting for **{}** registered player(s)…",
                            players.len()
                        )),
                ),
            )
            .await;
    }
    let report = redeem::redeem_code_for_players_with_progress(
        data,
        code,
        &players,
        true,
        progress_target.clone(),
    )
    .await?;
    let embed = serenity::CreateEmbed::new()
        .color(0xF1C40F)
        .title(format!("🎁 Gift code redemption — `{code}`"))
        .description(format!(
            "{}{}",
            report.summary(),
            report
                .aborted
                .as_ref()
                .map(|status| format!("\n⛔ Stopped early: **{status}** — code treated as dead."))
                .unwrap_or_default()
        ))
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));
    if let Some(target) = progress_target {
        let _ = serenity::ChannelId::new(target.channel_id)
            .edit_message(
                &ctx.http,
                serenity::MessageId::new(target.message_id),
                serenity::EditMessage::new().embed(embed.clone()),
            )
            .await;
    }
    crate::logging::log(ctx, data, guild_id, "giftcode", embed).await;
    Ok(report)
}
