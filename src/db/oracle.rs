//! `oracle_state` table (sync bookkeeping only; no credentials are stored).

use super::{now, Db};

#[derive(Debug, Clone, Default)]
pub struct OracleSyncState {
    pub last_codes_sync: Option<i64>,
    pub last_lookup_sync: Option<i64>,
    pub last_error: Option<String>,
    pub total_codes: i64,
    pub total_players: i64,
}

#[derive(sqlx::FromRow)]
struct OracleStateRow {
    last_codes_sync: Option<i64>,
    last_lookup_sync: Option<i64>,
    last_error: Option<String>,
    total_codes: i64,
    total_players: i64,
}

pub async fn state(db: &Db) -> sqlx::Result<OracleSyncState> {
    let row: Option<OracleStateRow> = sqlx::query_as(
        "SELECT last_codes_sync, last_lookup_sync, last_error, total_codes, total_players
         FROM oracle_state WHERE id = 1",
    )
    .fetch_optional(&db.pool)
    .await?;

    Ok(row
        .map(|row| OracleSyncState {
            last_codes_sync: row.last_codes_sync,
            last_lookup_sync: row.last_lookup_sync,
            last_error: row.last_error,
            total_codes: row.total_codes,
            total_players: row.total_players,
        })
        .unwrap_or_default())
}

pub async fn touch_codes_sync(db: &Db, total: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO oracle_state (id, last_codes_sync, total_codes) VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
             last_codes_sync = excluded.last_codes_sync,
             total_codes = excluded.total_codes,
             last_error = NULL",
    )
    .bind(now())
    .bind(total)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn touch_lookup_sync(db: &Db, total: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO oracle_state (id, last_lookup_sync, total_players) VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
             last_lookup_sync = excluded.last_lookup_sync,
             total_players = excluded.total_players,
             last_error = NULL",
    )
    .bind(now())
    .bind(total)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn record_error(db: &Db, message: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO oracle_state (id, last_error) VALUES (1, ?)
         ON CONFLICT(id) DO UPDATE SET last_error = excluded.last_error",
    )
    .bind(message)
    .execute(&db.pool)
    .await?;
    Ok(())
}

// ------------------------------------------------- optional wosoracle API key

type KeyRow = (String, Option<i64>, Option<i64>, Option<String>);

/// Optional WoS Oracle API key (owner-managed, one global row). The site is
/// open without it; the key only unlocks extra data (island image, map update…).
pub async fn key_get(db: &Db) -> sqlx::Result<super::wos_credentials::Credentials> {
    let row: Option<KeyRow> = sqlx::query_as(
        "SELECT api_key, last_test_at, last_test_ok, last_test_message
         FROM wos_oracle_credentials WHERE id = 1",
    )
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map_or_else(Default::default, |(key, at, ok, message)| {
        super::wos_credentials::Credentials {
            api_key: crate::secrets::open(&key),
            last_test_at: at,
            last_test_ok: ok.map(|v| v != 0),
            last_test_message: message,
        }
    }))
}

pub async fn key_set(db: &Db, api_key: &str, updated_by: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO wos_oracle_credentials (id, api_key, updated_at, updated_by)
         VALUES (1, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET api_key = excluded.api_key,
             updated_at = excluded.updated_at, updated_by = excluded.updated_by,
             last_test_at = NULL, last_test_ok = NULL, last_test_message = NULL",
    )
    .bind(crate::secrets::seal(api_key.trim()))
    .bind(chrono::Utc::now().timestamp_millis())
    .bind(updated_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn key_clear(db: &Db) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM wos_oracle_credentials WHERE id = 1")
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn key_record_test(db: &Db, ok: bool, message: &str) -> sqlx::Result<()> {
    let message: String = message.chars().take(500).collect();
    sqlx::query(
        "UPDATE wos_oracle_credentials SET last_test_at = ?, last_test_ok = ?, last_test_message = ?
         WHERE id = 1",
    )
    .bind(chrono::Utc::now().timestamp_millis())
    .bind(ok as i64)
    .bind(message)
    .execute(&db.pool)
    .await?;
    Ok(())
}
