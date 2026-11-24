//! DB-backed bot administrators (the legacy `admin` table).
//!
//! Two tiers: exactly one **Owner** (recovery anchor; can't be removed,
//! only transferred) and any number of **Admins** (full access).
//! Grants take effect within five seconds (short-TTL cache).

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, SystemTime};

use sqlx::Row;

use super::Db;

#[derive(Debug, Clone)]
pub struct AdminEntry {
    pub id: u64,
    pub is_owner: bool,
}

const CACHE_TTL: Duration = Duration::from_secs(5);

fn cache() -> &'static RwLock<(HashMap<u64, bool>, SystemTime)> {
    static CACHE: OnceLock<RwLock<(HashMap<u64, bool>, SystemTime)>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new((HashMap::new(), SystemTime::UNIX_EPOCH)))
}

pub fn invalidate_cache() {
    if let Ok(mut guard) = cache().write() {
        guard.0.clear();
        guard.1 = SystemTime::UNIX_EPOCH;
    }
}

pub async fn list(db: &Db) -> sqlx::Result<Vec<AdminEntry>> {
    let rows = sqlx::query(
        "SELECT CAST(id AS TEXT) AS id, is_owner FROM admin ORDER BY is_owner DESC, id",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.get::<String, _>("id");
            Some(AdminEntry {
                id: id.parse().ok()?,
                is_owner: row.get::<i64, _>("is_owner") != 0,
            })
        })
        .collect())
}

pub async fn count(db: &Db) -> sqlx::Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) FROM admin")
        .fetch_one(&db.pool)
        .await?;
    Ok(row.get::<i64, _>(0))
}

pub async fn owner_id(db: &Db) -> sqlx::Result<Option<u64>> {
    let row = sqlx::query("SELECT CAST(id AS TEXT) AS id FROM admin WHERE is_owner = 1 LIMIT 1")
        .fetch_optional(&db.pool)
        .await?;
    Ok(row.and_then(|row| row.get::<String, _>("id").parse().ok()))
}

async fn is_admin_cached(db: &Db, user_id: u64) -> sqlx::Result<bool> {
    let cached = {
        let guard = cache().read().unwrap();
        if guard
            .1
            .elapsed()
            .map(|elapsed| elapsed < CACHE_TTL)
            .unwrap_or(false)
        {
            return Ok(guard.0.contains_key(&user_id));
        }
        false
    };
    let _ = cached;
    let rows: Vec<(String,)> = sqlx::query_as("SELECT CAST(id AS TEXT) AS id FROM admin")
        .fetch_all(&db.pool)
        .await?;
    let mut map = HashMap::with_capacity(rows.len());
    for (id,) in rows {
        if let Ok(id) = id.parse() {
            map.insert(id, true);
        }
    }
    if let Ok(mut guard) = cache().write() {
        *guard = (map, SystemTime::now());
    }
    let found = cache()
        .read()
        .map(|guard| guard.0.contains_key(&user_id))
        .unwrap_or(false);
    Ok(found)
}

pub async fn is_admin(db: &Db, user_id: u64) -> bool {
    is_admin_cached(db, user_id).await.unwrap_or(false)
}

pub async fn is_owner(db: &Db, user_id: u64) -> bool {
    let row: Option<(i64,)> = sqlx::query_as("SELECT is_owner FROM admin WHERE id = ?")
        .bind(user_id.to_string())
        .fetch_optional(&db.pool)
        .await
        .unwrap_or(None);
    row.is_some_and(|(is_owner,)| is_owner != 0)
}

/// Add an admin; the very first admin becomes Owner automatically.
pub async fn add(db: &Db, user_id: u64, as_owner: bool, actor_id: u64) -> sqlx::Result<bool> {
    let total = count(db).await?;
    let owner_flag: i64 = if as_owner || total == 0 { 1 } else { 0 };
    sqlx::query("INSERT OR REPLACE INTO admin (id, is_owner) VALUES (?, ?)")
        .bind(user_id.to_string())
        .bind(owner_flag)
        .execute(&db.pool)
        .await?;
    audit(
        db,
        actor_id,
        if owner_flag == 1 {
            "add_owner"
        } else {
            "add_admin"
        },
        Some(user_id),
    )
    .await?;
    invalidate_cache();
    Ok(owner_flag == 1)
}

/// Remove an admin (owner is refused; transfer ownership first).
pub async fn remove(db: &Db, user_id: u64, actor_id: u64) -> sqlx::Result<Result<(), String>> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT is_owner FROM admin WHERE id = ?")
        .bind(user_id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    match row {
        None => Ok(Err("that user is not an admin".to_string())),
        Some((1,)) => Ok(Err(
            "cannot remove the bot owner — transfer ownership first".to_string(),
        )),
        Some((_,)) => {
            sqlx::query("DELETE FROM admin WHERE id = ?")
                .bind(user_id.to_string())
                .execute(&db.pool)
                .await?;
            audit(db, actor_id, "remove_admin", Some(user_id)).await?;
            invalidate_cache();
            Ok(Ok(()))
        }
    }
}

/// Transfer ownership atomically. Both users must already be admins.
pub async fn transfer_owner(
    db: &Db,
    new_owner_id: u64,
    actor_id: u64,
) -> sqlx::Result<Result<(), String>> {
    let new_row: Option<(i64,)> = sqlx::query_as("SELECT is_owner FROM admin WHERE id = ?")
        .bind(new_owner_id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    if new_row.is_none() {
        return Ok(Err("that user is not an admin — add them first".to_string()));
    }
    let current_owner = match owner_id(db).await? {
        Some(id) => id,
        None => {
            // No owner at all: promote the target directly.
            return Ok(add(db, new_owner_id, true, actor_id)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()));
        }
    };
    let mut tx = db.pool.begin().await?;
    sqlx::query("UPDATE admin SET is_owner = 0 WHERE id = ?")
        .bind(current_owner.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE admin SET is_owner = 1 WHERE id = ?")
        .bind(new_owner_id.to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit(db, actor_id, "transfer_owner", Some(new_owner_id)).await?;
    invalidate_cache();
    Ok(Ok(()))
}

/// Claim ownership when none exists (recovery anchor).
pub async fn claim_owner(db: &Db, user_id: u64, actor_id: u64) -> sqlx::Result<bool> {
    let existing = sqlx::query("SELECT 1 FROM admin WHERE is_owner = 1 LIMIT 1")
        .fetch_optional(&db.pool)
        .await?;
    if existing.is_some() {
        return Ok(false);
    }
    sqlx::query("UPDATE admin SET is_owner = 1 WHERE id = ?")
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    let _ = audit(db, actor_id, "claim_owner", Some(user_id)).await;
    invalidate_cache();
    Ok(true)
}

pub async fn audit(
    db: &Db,
    actor_id: u64,
    action: &str,
    target_id: Option<u64>,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO permission_audit_log (actor_id, action, target_id, detail) VALUES (?, ?, ?, NULL)")
        .bind(actor_id.to_string())
        .bind(action)
        .bind(target_id.map(|id| id.to_string()))
        .execute(&db.pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: i64,
    pub actor_id: String,
    pub action: String,
    pub target_id: Option<String>,
    pub created_at: String,
}

pub async fn audit_log(db: &Db, limit: i64) -> sqlx::Result<Vec<AuditEntry>> {
    let rows = sqlx::query(
        "SELECT id, actor_id, action, CAST(target_id AS TEXT) AS target_id, created_at
         FROM permission_audit_log ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| AuditEntry {
            id: row.get("id"),
            actor_id: row.get("actor_id"),
            action: row.get("action"),
            target_id: row.get("target_id"),
            created_at: row.get("created_at"),
        })
        .collect())
}
