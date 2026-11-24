//! Player-ID limits and ownership claims. Tables are byte-compatible with the
//! JavaScript bot (`giftcode/database/limits.js`, `claims.js`).

use sqlx::FromRow;

use super::Db;

pub const DEFAULT_BASE_CAP: i64 = 4;
pub const DEFAULT_BOOSTER_CAP: i64 = 12;
pub const MIN_CAP: i64 = 1;
pub const MAX_CAP: i64 = 500;

pub fn clamp_cap(value: i64) -> i64 {
    value.clamp(MIN_CAP, MAX_CAP)
}

#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub cap: i64,
    pub source: &'static str,
}

#[derive(Debug, Clone, FromRow)]
pub struct RoleCap {
    pub role_id: String,
    pub cap: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct PendingClaim {
    pub id: i64,
    pub fid: String,
    pub requesting_user_id: String,
    pub requesting_guild_id: String,
    pub current_owner_id: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub resolved_by: Option<String>,
}

async fn settings(db: &Db, guild_id: u64) -> sqlx::Result<(i64, i64)> {
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT base_cap, booster_cap FROM giftcode_limit_settings WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.unwrap_or((DEFAULT_BASE_CAP, DEFAULT_BOOSTER_CAP)))
}

pub async fn set_base_and_booster(
    db: &Db,
    guild_id: u64,
    base: i64,
    booster: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO giftcode_limit_settings (guild_id, base_cap, booster_cap) VALUES (?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET base_cap = excluded.base_cap, booster_cap = excluded.booster_cap",
    )
    .bind(guild_id.to_string())
    .bind(clamp_cap(base))
    .bind(clamp_cap(booster))
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn list_role_caps(db: &Db, guild_id: u64) -> sqlx::Result<Vec<RoleCap>> {
    sqlx::query_as(
        "SELECT role_id, cap FROM giftcode_role_caps WHERE guild_id = ? ORDER BY cap DESC, role_id",
    )
    .bind(guild_id.to_string())
    .fetch_all(&db.pool)
    .await
}

pub async fn set_role_cap(db: &Db, guild_id: u64, role_id: u64, cap: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO giftcode_role_caps (guild_id, role_id, cap) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, role_id) DO UPDATE SET cap = excluded.cap",
    )
    .bind(guild_id.to_string())
    .bind(role_id.to_string())
    .bind(clamp_cap(cap))
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn remove_role_cap(db: &Db, guild_id: u64, role_id: u64) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM giftcode_role_caps WHERE guild_id = ? AND role_id = ?")
        .bind(guild_id.to_string())
        .bind(role_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn user_cap(db: &Db, user_id: u64) -> sqlx::Result<Option<i64>> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT cap FROM giftcode_user_caps WHERE user_id = ?")
            .bind(user_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.map(|(cap,)| cap))
}

pub async fn set_user_cap(db: &Db, user_id: u64, cap: i64, set_by: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO giftcode_user_caps (user_id, cap, set_by, set_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET cap = excluded.cap, set_by = excluded.set_by, set_at = excluded.set_at",
    )
    .bind(user_id.to_string())
    .bind(clamp_cap(cap))
    .bind(set_by.to_string())
    .bind(super::now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn remove_user_cap(db: &Db, user_id: u64) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM giftcode_user_caps WHERE user_id = ?")
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Highest of base / booster / role caps; an admin override wins outright.
pub async fn resolve_limit(
    db: &Db,
    guild_id: u64,
    user_id: u64,
    is_booster: bool,
    role_ids: &[u64],
) -> sqlx::Result<Limit> {
    if let Some(cap) = user_cap(db, user_id).await? {
        return Ok(Limit {
            cap,
            source: "admin override",
        });
    }
    let (base, booster) = settings(db, guild_id).await?;
    let mut best = Limit {
        cap: base,
        source: "base limit",
    };
    if is_booster && booster > best.cap {
        best = Limit {
            cap: booster,
            source: "Server Booster",
        };
    }
    for role in list_role_caps(db, guild_id).await? {
        let id = role.role_id.parse::<u64>().unwrap_or(0);
        if id != 0 && role_ids.contains(&id) && role.cap > best.cap {
            // Keep the mention dynamic; callers can render source as role id.
            return Ok(Limit {
                cap: role.cap,
                source: "role cap",
            });
        }
    }
    Ok(best)
}

pub async fn has_pending_claim(db: &Db, fid: &str) -> sqlx::Result<bool> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM pending_claims WHERE fid = ? AND status = 'pending' LIMIT 1")
            .bind(fid)
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.is_some())
}

pub async fn create_claim(
    db: &Db,
    fid: &str,
    requesting_user_id: u64,
    requesting_guild_id: u64,
    current_owner_id: Option<u64>,
) -> sqlx::Result<PendingClaim> {
    sqlx::query(
        "INSERT INTO pending_claims (fid, requesting_user_id, requesting_guild_id, current_owner_id, status, created_at)
         VALUES (?, ?, ?, ?, 'pending', ?)",
    )
    .bind(fid)
    .bind(requesting_user_id.to_string())
    .bind(requesting_guild_id.to_string())
    .bind(current_owner_id.map(|id| id.to_string()))
    .bind(super::now())
    .execute(&db.pool)
    .await?;
    let claim = sqlx::query_as::<_, PendingClaim>(
        "SELECT * FROM pending_claims WHERE fid = ? AND status = 'pending' ORDER BY id DESC LIMIT 1",
    )
    .bind(fid)
    .fetch_one(&db.pool)
    .await?;
    Ok(claim)
}

pub async fn list_pending_for_guild(db: &Db, guild_id: u64) -> sqlx::Result<Vec<PendingClaim>> {
    sqlx::query_as::<_, PendingClaim>(
        "SELECT * FROM pending_claims WHERE requesting_guild_id = ? AND status = 'pending' ORDER BY created_at ASC",
    )
    .bind(guild_id.to_string())
    .fetch_all(&db.pool)
    .await
}

pub async fn get_claim(db: &Db, id: i64) -> sqlx::Result<Option<PendingClaim>> {
    sqlx::query_as::<_, PendingClaim>("SELECT * FROM pending_claims WHERE id = ?")
        .bind(id)
        .fetch_optional(&db.pool)
        .await
}

pub async fn resolve_claim(db: &Db, id: i64, status: &str, resolved_by: u64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE pending_claims SET status = ?, resolved_at = ?, resolved_by = ? WHERE id = ? AND status = 'pending'",
    )
    .bind(status)
    .bind(super::now())
    .bind(resolved_by.to_string())
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn limits_and_claims_roundtrip() {
        let db = Db::memory().await.unwrap();
        assert_eq!(
            resolve_limit(&db, 9, 7, false, &[]).await.unwrap().cap,
            DEFAULT_BASE_CAP
        );
        set_base_and_booster(&db, 9, 6, 15).await.unwrap();
        assert_eq!(resolve_limit(&db, 9, 7, true, &[]).await.unwrap().cap, 15);
        set_role_cap(&db, 9, 100, 20).await.unwrap();
        assert_eq!(
            resolve_limit(&db, 9, 7, false, &[100]).await.unwrap().cap,
            20
        );
        set_user_cap(&db, 7, 3, 1).await.unwrap();
        assert_eq!(resolve_limit(&db, 9, 7, true, &[100]).await.unwrap().cap, 3);
        remove_user_cap(&db, 7).await.unwrap();
        assert_eq!(resolve_limit(&db, 9, 7, false, &[]).await.unwrap().cap, 6);

        let claim = create_claim(&db, "123", 7, 9, Some(42)).await.unwrap();
        assert!(has_pending_claim(&db, "123").await.unwrap());
        assert_eq!(list_pending_for_guild(&db, 9).await.unwrap().len(), 1);
        resolve_claim(&db, claim.id, "approved", 1).await.unwrap();
        assert!(!has_pending_claim(&db, "123").await.unwrap());
    }
}
