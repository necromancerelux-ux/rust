//! `gift_codes` + `redemptions` tables.

use sqlx::FromRow;

use super::{now, Db};

#[derive(Debug, Clone, FromRow)]
pub struct GiftCode {
    pub code: String,
    pub status: String,
    pub added_by: String,
    pub source_guild_id: String,
    pub added_at: i64,
    pub date: Option<String>,
    pub source: Option<String>,
    pub api_pushed: i64,
    pub is_vip: i64,
    pub last_validated: Option<String>,
    pub announced: i64,
    pub announcement_complete: i64,
    pub redemption_handoff_by: Option<String>,
    pub redemption_handoff_at: Option<i64>,
}

impl GiftCode {
    pub fn is_active(&self) -> bool {
        self.status.eq_ignore_ascii_case("active")
    }

    pub fn is_vip(&self) -> bool {
        self.is_vip != 0
    }

    pub fn source_label(&self) -> &str {
        self.source.as_deref().unwrap_or("manual")
    }
}

#[derive(Debug, Clone, Default)]
pub struct GiftCodeStats {
    pub total: i64,
    pub active: i64,
    pub vip: i64,
    pub today: i64,
}

/// Insert a code. Returns `true` when it was new.
pub async fn insert(
    db: &Db,
    code: &str,
    added_by: u64,
    guild_id: u64,
    source: &str,
    date: Option<&str>,
    vip: bool,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "INSERT INTO gift_codes (code, status, added_by, source_guild_id, added_at, date, source, is_vip)
         VALUES (?, 'active', ?, ?, ?, ?, ?, ?)
         ON CONFLICT(code) DO NOTHING",
    )
    .bind(code)
    .bind(added_by.to_string())
    .bind(guild_id.to_string())
    .bind(now())
    .bind(date)
    .bind(source)
    .bind(vip as i64)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Update discovery metadata when a higher-priority feed also reports a code.
pub async fn update_source_metadata(
    db: &Db,
    code: &str,
    source: &str,
    date: Option<&str>,
    api_pushed: bool,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE gift_codes SET source = ?, date = COALESCE(?, date), api_pushed = MAX(api_pushed, ?) WHERE code = ?",
    )
    .bind(source)
    .bind(date)
    .bind(api_pushed as i64)
    .bind(code)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn get(db: &Db, code: &str) -> sqlx::Result<Option<GiftCode>> {
    sqlx::query_as::<_, GiftCode>("SELECT * FROM gift_codes WHERE code = ?")
        .bind(code)
        .fetch_optional(&db.pool)
        .await
}

pub async fn list_active(db: &Db, limit: i64) -> sqlx::Result<Vec<GiftCode>> {
    sqlx::query_as::<_, GiftCode>(
        "SELECT * FROM gift_codes WHERE status = 'active' ORDER BY added_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&db.pool)
    .await
}

pub async fn list_recent(db: &Db, limit: i64) -> sqlx::Result<Vec<GiftCode>> {
    sqlx::query_as::<_, GiftCode>("SELECT * FROM gift_codes ORDER BY added_at DESC LIMIT ?")
        .bind(limit)
        .fetch_all(&db.pool)
        .await
}

pub async fn search(db: &Db, needle: &str, limit: i64) -> sqlx::Result<Vec<GiftCode>> {
    let escaped = needle
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    sqlx::query_as::<_, GiftCode>(
        "SELECT * FROM gift_codes WHERE code LIKE ? ESCAPE '\\' ORDER BY added_at DESC LIMIT ?",
    )
    .bind(pattern)
    .bind(limit)
    .fetch_all(&db.pool)
    .await
}

pub async fn set_status(db: &Db, code: &str, status: &str) -> sqlx::Result<bool> {
    let result = sqlx::query("UPDATE gift_codes SET status = ?, last_validated = ? WHERE code = ?")
        .bind(status)
        .bind(now().to_string())
        .bind(code)
        .execute(&db.pool)
        .await?;
    if !status.eq_ignore_ascii_case("active") {
        clear_announcement_deliveries(db, code).await?;
        sqlx::query("DELETE FROM gift_code_announcement_claims WHERE code = ?")
            .bind(code)
            .execute(&db.pool)
            .await?;
    }
    Ok(result.rows_affected() > 0)
}

/// Active codes added at least an hour ago whose last validation is over a day
/// old (or has never been recorded), oldest validation first.
pub async fn needing_validation(
    db: &Db,
    now_seconds: i64,
    limit: i64,
) -> sqlx::Result<Vec<GiftCode>> {
    let minimum_age = now_seconds.saturating_sub(60 * 60);
    let cutoff = chrono::DateTime::<chrono::Utc>::from_timestamp(
        now_seconds.saturating_sub(24 * 60 * 60),
        0,
    )
    .map(|date| date.to_rfc3339())
    .unwrap_or_default();
    sqlx::query_as::<_, GiftCode>(
        "SELECT * FROM gift_codes
         WHERE status = 'active' AND added_at < ?
           AND (last_validated IS NULL OR last_validated < ?)
         ORDER BY (last_validated IS NULL) DESC, last_validated ASC, added_at ASC
         LIMIT ?",
    )
    .bind(minimum_age)
    .bind(cutoff)
    .bind(limit.clamp(1, 100))
    .fetch_all(&db.pool)
    .await
}

/// Mark a successful live validation and optionally refresh VIP classification.
pub async fn update_validation(db: &Db, code: &str, is_vip: bool) -> sqlx::Result<()> {
    let validated = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE gift_codes SET last_validated = ?, is_vip = ? WHERE code = ? AND status = 'active'",
    )
    .bind(validated)
    .bind(is_vip as i64)
    .bind(code)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn delete(db: &Db, code: &str) -> sqlx::Result<bool> {
    let mut tx = db.pool.begin().await?;
    let result = sqlx::query("DELETE FROM gift_codes WHERE code = ?")
        .bind(code)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM gift_code_announcements WHERE code = ?")
        .bind(code)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM gift_code_announcement_claims WHERE code = ?")
        .bind(code)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(result.rows_affected() > 0)
}

/// Atomically claim an announcement in the shared legacy `announced` column.
/// An existing lease means another Rust process owns it; an announced row with
/// no lease belongs to the JS runtime or was already completed.
pub async fn claim_announcement(db: &Db, code: &str, lease_seconds: i64) -> sqlx::Result<bool> {
    let now = now();
    let lease_until = now.saturating_add(lease_seconds.max(30));
    let mut tx = db.pool.begin().await?;
    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT lease_until FROM gift_code_announcement_claims WHERE code = ?")
            .bind(code)
            .fetch_optional(&mut *tx)
            .await?;
    match existing {
        Some((until,)) if until > now => {
            tx.rollback().await?;
            return Ok(false);
        }
        Some(_) => {
            sqlx::query("UPDATE gift_code_announcement_claims SET lease_until = ? WHERE code = ?")
                .bind(lease_until)
                .bind(code)
                .execute(&mut *tx)
                .await?;
        }
        None => {
            let claimed = sqlx::query(
                "UPDATE gift_codes SET announced = 1 WHERE code = ? AND status = 'active' AND announced = 0 AND announcement_complete = 0",
            )
            .bind(code)
            .execute(&mut *tx)
            .await?;
            if claimed.rows_affected() == 0 {
                tx.rollback().await?;
                return Ok(false);
            }
            sqlx::query(
                "INSERT INTO gift_code_announcement_claims (code, lease_until) VALUES (?, ?)",
            )
            .bind(code)
            .bind(lease_until)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(true)
}

pub async fn renew_announcement_claim(db: &Db, code: &str, lease_seconds: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE gift_code_announcement_claims SET lease_until = ? WHERE code = ?")
        .bind(now().saturating_add(lease_seconds.max(30)))
        .bind(code)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Release an unproductive claim so either runtime can retry it later.
pub async fn release_announcement_claim(db: &Db, code: &str) -> sqlx::Result<()> {
    let mut tx = db.pool.begin().await?;
    sqlx::query(
        "UPDATE gift_codes SET announced = 0 WHERE code = ? AND announcement_complete = 0 AND EXISTS (SELECT 1 FROM gift_code_announcement_claims c WHERE c.code = gift_codes.code)",
    )
    .bind(code)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM gift_code_announcement_claims WHERE code = ?")
        .bind(code)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn finish_announcement(db: &Db, code: &str) -> sqlx::Result<()> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("UPDATE gift_codes SET announced = 1, announcement_complete = 1 WHERE code = ?")
        .bind(code)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM gift_code_announcement_claims WHERE code = ?")
        .bind(code)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM gift_code_announcements WHERE code = ?")
        .bind(code)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn announced_to_guild(db: &Db, code: &str, guild_id: u64) -> sqlx::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM gift_code_announcements WHERE code = ? AND guild_id = ?",
    )
    .bind(code)
    .bind(guild_id.to_string())
    .fetch_one(&db.pool)
    .await?;
    Ok(count > 0)
}

pub async fn mark_announced_to_guild(db: &Db, code: &str, guild_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO gift_code_announcements (code, guild_id, announced_at) VALUES (?, ?, ?) ON CONFLICT(code, guild_id) DO NOTHING",
    )
    .bind(code)
    .bind(guild_id.to_string())
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn mark_codes_announced_to_guild(
    db: &Db,
    codes: &[String],
    guild_id: u64,
) -> sqlx::Result<()> {
    let mut tx = db.pool.begin().await?;
    for code in codes {
        sqlx::query(
            "INSERT INTO gift_code_announcements (code, guild_id, announced_at) VALUES (?, ?, ?) ON CONFLICT(code, guild_id) DO NOTHING",
        )
        .bind(code)
        .bind(guild_id.to_string())
        .bind(now())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn clear_announcement_deliveries(db: &Db, code: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM gift_code_announcements WHERE code = ?")
        .bind(code)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Codes added since the last announcement, oldest first.
pub async fn unannounced(db: &Db, limit: i64) -> sqlx::Result<Vec<GiftCode>> {
    sqlx::query_as::<_, GiftCode>(
        "SELECT * FROM gift_codes WHERE announced = 0 OR (announcement_complete = 0 AND EXISTS (SELECT 1 FROM gift_code_announcement_claims c WHERE c.code = gift_codes.code)) ORDER BY CASE WHEN redemption_handoff_at IS NOT NULL THEN 0 ELSE 1 END, added_at ASC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&db.pool)
    .await
}

pub async fn expired_announcement_claims(
    db: &Db,
    now_seconds: i64,
    limit: i64,
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT c.code FROM gift_code_announcement_claims c JOIN gift_codes g ON g.code = c.code WHERE c.lease_until <= ? AND g.status = 'active' AND g.announcement_complete = 0 ORDER BY c.lease_until ASC LIMIT ?",
    )
    .bind(now_seconds)
    .bind(limit.clamp(1, 500))
    .fetch_all(&db.pool)
    .await
}

/// Explicitly transfer unannounced legacy auto-sync rows to Rust recovery.
/// The caller must be the bot owner and confirm the JS worker is stopped.
pub async fn handoff_js_pending(db: &Db, owner_id: u64) -> sqlx::Result<Vec<String>> {
    let mut tx = db.pool.begin().await?;
    let codes: Vec<String> = sqlx::query_scalar(
        "SELECT code FROM gift_codes WHERE status = 'active' AND announced = 0 AND added_by = 'SYSTEM_AUTO_SYNC' AND source_guild_id = 'SYSTEM_AUTO_SYNC' AND redemption_handoff_at IS NULL ORDER BY added_at ASC",
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut handed_off = Vec::new();
    for code in &codes {
        let updated = sqlx::query(
            "UPDATE gift_codes SET redemption_handoff_by = ?, redemption_handoff_at = ? WHERE code = ? AND status = 'active' AND announced = 0 AND redemption_handoff_at IS NULL",
        )
        .bind(owner_id.to_string())
        .bind(now())
        .bind(code)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() > 0 {
            handed_off.push(code.clone());
        }
    }
    tx.commit().await?;
    Ok(handed_off)
}

pub async fn stats(db: &Db) -> sqlx::Result<GiftCodeStats> {
    let row: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            COUNT(*),
            COALESCE(SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(is_vip), 0),
            COALESCE(SUM(CASE WHEN added_at >= strftime('%s','now') - 86400 THEN 1 ELSE 0 END), 0)
         FROM gift_codes",
    )
    .fetch_one(&db.pool)
    .await?;

    Ok(GiftCodeStats {
        total: row.0,
        active: row.1,
        vip: row.2,
        today: row.3,
    })
}

/// Record a redemption attempt for a tracked player.
pub async fn record_redemption(db: &Db, fid: &str, code: &str, status: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO redemptions (fid, code, status, redeemed_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(fid, code) DO UPDATE SET status = excluded.status, redeemed_at = excluded.redeemed_at",
    )
    .bind(fid)
    .bind(code)
    .bind(status)
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// FIDs with any recorded attempt for this code (used to skip re-redeeming).
pub async fn fids_redeemed_for_code(
    db: &Db,
    code: &str,
) -> sqlx::Result<std::collections::HashSet<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT fid FROM redemptions WHERE code = ?")
        .bind(code)
        .fetch_all(&db.pool)
        .await?;
    Ok(rows.into_iter().map(|(fid,)| fid).collect())
}

/// Last recorded status per FID for a code, for retry selection.
pub async fn statuses_for_code(db: &Db, code: &str) -> sqlx::Result<Vec<(String, String)>> {
    sqlx::query_as("SELECT fid, status FROM redemptions WHERE code = ?")
        .bind(code)
        .fetch_all(&db.pool)
        .await
}

/// A probe player with a known state: explicit env override or any registered
/// player with a state on file.
pub async fn probe_player(db: &Db, guild_id: u64) -> Option<(String, i64)> {
    let configured_fid =
        crate::db::settings::get(db, guild_id, crate::db::settings::keys::GIFT_PROBE_FID)
            .await
            .ok()
            .flatten();
    let configured_state =
        crate::db::settings::get(db, guild_id, crate::db::settings::keys::GIFT_PROBE_STATE)
            .await
            .ok()
            .flatten();
    if let (Some(fid), Some(state)) = (configured_fid, configured_state) {
        if let (Ok(fid), Ok(state)) = (fid.parse::<u64>(), state.parse::<i64>()) {
            if fid != 0 && state >= 0 {
                return Some((fid.to_string(), state));
            }
        }
    }
    if let Ok(fid) = std::env::var("GIFTCODE_TEST_FID") {
        if let Ok(state) = std::env::var("GIFTCODE_TEST_STATE") {
            if let (Ok(fid), Ok(state)) = (fid.trim().parse::<u64>(), state.trim().parse::<i64>()) {
                if fid != 0 {
                    return Some((fid.to_string(), state));
                }
            }
        }
    }
    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT fid, state FROM players WHERE state IS NOT NULL LIMIT 1")
            .fetch_optional(&db.pool)
            .await
            .ok()?;
    row
}
