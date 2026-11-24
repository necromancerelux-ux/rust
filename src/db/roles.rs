//! Role Assignment storage: catalog of assignable roles, per-actor grants
//! (by user or by Discord role) and the audit log.

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, FromRow)]
pub struct AssignableRole {
    pub guild_id: String,
    pub role_id: String,
    pub added_by: String,
}

pub async fn panel_settings(
    db: &Db,
    guild_id: u64,
) -> sqlx::Result<Option<(Option<u64>, Option<u64>)>> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT channel_id, panel_message_id FROM role_panel_settings WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|(channel, message)| {
        (
            channel.and_then(|v| v.parse().ok()).filter(|id| *id != 0),
            message.and_then(|v| v.parse().ok()).filter(|id| *id != 0),
        )
    }))
}

pub async fn set_panel(db: &Db, guild_id: u64, channel: u64, message_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO role_panel_settings (guild_id, channel_id, panel_message_id) VALUES (?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET channel_id = excluded.channel_id, panel_message_id = excluded.panel_message_id",
    )
    .bind(guild_id.to_string())
    .bind(channel.to_string())
    .bind(message_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

// ----------------------------------------------------------------- catalog

pub async fn list_catalog(db: &Db, guild_id: u64) -> sqlx::Result<Vec<u64>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT role_id FROM assignable_roles WHERE guild_id = ? ORDER BY added_at")
            .bind(guild_id.to_string())
            .fetch_all(&db.pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id,)| id.parse().ok())
        .collect())
}

pub async fn in_catalog(db: &Db, guild_id: u64, role_id: u64) -> sqlx::Result<bool> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM assignable_roles WHERE guild_id = ? AND role_id = ?")
            .bind(guild_id.to_string())
            .bind(role_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.is_some())
}

pub async fn add_to_catalog(
    db: &Db,
    guild_id: u64,
    role_id: u64,
    added_by: u64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO assignable_roles (guild_id, role_id, added_by) VALUES (?, ?, ?)",
    )
    .bind(guild_id.to_string())
    .bind(role_id.to_string())
    .bind(added_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Removing a role from the catalog also drops its grants.
pub async fn remove_from_catalog(db: &Db, guild_id: u64, role_id: u64) -> sqlx::Result<()> {
    let guild = guild_id.to_string();
    let role = role_id.to_string();
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM assignable_roles WHERE guild_id = ? AND role_id = ?")
        .bind(&guild)
        .bind(&role)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM role_assigners WHERE guild_id = ? AND role_id = ?")
        .bind(&guild)
        .bind(&role)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM role_assigner_roles WHERE guild_id = ? AND assignable_role_id = ?")
        .bind(&guild)
        .bind(&role)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

// ----------------------------------------------------------------- grants

pub async fn list_assigners(db: &Db, guild_id: u64, role_id: u64) -> sqlx::Result<Vec<u64>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT user_id FROM role_assigners WHERE guild_id = ? AND role_id = ? ORDER BY granted_at",
    )
    .bind(guild_id.to_string())
    .bind(role_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id,)| id.parse().ok())
        .collect())
}

pub async fn list_assigner_roles(db: &Db, guild_id: u64, role_id: u64) -> sqlx::Result<Vec<u64>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT granter_role_id FROM role_assigner_roles WHERE guild_id = ? AND assignable_role_id = ? ORDER BY granted_at",
    )
    .bind(guild_id.to_string())
    .bind(role_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id,)| id.parse().ok())
        .collect())
}

/// Per-guild summary lines for the Manage Catalog screen.
pub async fn catalog_summaries(db: &Db, guild_id: u64) -> sqlx::Result<Vec<(u64, usize)>> {
    let catalog = list_catalog(db, guild_id).await?;
    let mut out = Vec::with_capacity(catalog.len());
    for role_id in catalog {
        let users = list_assigners(db, guild_id, role_id).await?.len();
        let roles = list_assigner_roles(db, guild_id, role_id).await?.len();
        out.push((role_id, users + roles));
    }
    Ok(out)
}

pub async fn grant_user(
    db: &Db,
    guild_id: u64,
    user_id: u64,
    role_id: u64,
    granted_by: u64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO role_assigners (guild_id, user_id, role_id, granted_by) VALUES (?, ?, ?, ?)
         ON CONFLICT(guild_id, user_id, role_id) DO UPDATE SET granted_by = excluded.granted_by",
    )
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .bind(role_id.to_string())
    .bind(granted_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn revoke_user(db: &Db, guild_id: u64, user_id: u64, role_id: u64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM role_assigners WHERE guild_id = ? AND user_id = ? AND role_id = ?")
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .bind(role_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn grant_role(
    db: &Db,
    guild_id: u64,
    granter_role_id: u64,
    role_id: u64,
    granted_by: u64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO role_assigner_roles (guild_id, granter_role_id, assignable_role_id, granted_by) VALUES (?, ?, ?, ?)
         ON CONFLICT(guild_id, granter_role_id, assignable_role_id) DO UPDATE SET granted_by = excluded.granted_by",
    )
    .bind(guild_id.to_string())
    .bind(granter_role_id.to_string())
    .bind(role_id.to_string())
    .bind(granted_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn revoke_role(
    db: &Db,
    guild_id: u64,
    granter_role_id: u64,
    role_id: u64,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM role_assigner_roles WHERE guild_id = ? AND granter_role_id = ? AND assignable_role_id = ?")
        .bind(guild_id.to_string())
        .bind(granter_role_id.to_string())
        .bind(role_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Everything every grant in this guild points at, grouped per role.
pub async fn grant_summaries(
    db: &Db,
    guild_id: u64,
) -> sqlx::Result<Vec<(u64, Vec<u64>, Vec<u64>)>> {
    let catalog = list_catalog(db, guild_id).await?;
    let mut out = Vec::with_capacity(catalog.len());
    for role_id in catalog {
        out.push((
            role_id,
            list_assigners(db, guild_id, role_id).await?,
            list_assigner_roles(db, guild_id, role_id).await?,
        ));
    }
    Ok(out)
}

/// Every catalog role this actor may assign: full catalog for admins,
/// otherwise the union of user grants and Discord-role grants.
pub async fn assignable_for_actor(
    db: &Db,
    guild_id: u64,
    actor_user_id: u64,
    actor_role_ids: &[u64],
    actor_is_admin: bool,
) -> anyhow::Result<Vec<u64>> {
    let catalog = list_catalog(db, guild_id).await?;
    if actor_is_admin {
        return Ok(catalog);
    }
    let mut out = Vec::new();
    for role_id in catalog {
        let direct: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM role_assigners WHERE guild_id = ? AND role_id = ? AND user_id = ?",
        )
        .bind(guild_id.to_string())
        .bind(role_id.to_string())
        .bind(actor_user_id.to_string())
        .fetch_optional(&db.pool)
        .await?;
        if direct.is_some() {
            out.push(role_id);
            continue;
        }
        if actor_role_ids.is_empty() {
            continue;
        }
        let mut query =
            sqlx::QueryBuilder::new("SELECT 1 FROM role_assigner_roles WHERE guild_id = ");
        query.push_bind(guild_id.to_string());
        query.push(" AND assignable_role_id = ");
        query.push_bind(role_id.to_string());
        query.push(" AND granter_role_id IN (");
        {
            let mut separated = query.separated(", ");
            for granter in actor_role_ids {
                separated.push_bind(granter.to_string());
            }
        }
        query.push(")");
        let rows = query
            .build_query_as::<(i64,)>()
            .fetch_optional(&db.pool)
            .await?;
        if rows.is_some() {
            out.push(role_id);
        }
    }
    Ok(out)
}

pub async fn log_assignment(
    db: &Db,
    guild_id: u64,
    actor_id: u64,
    target_user_id: u64,
    role_id: u64,
    action: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO role_assignment_log (guild_id, actor_id, target_user_id, role_id, action, timestamp)
         VALUES (?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind(guild_id.to_string())
    .bind(actor_id.to_string())
    .bind(target_user_id.to_string())
    .bind(role_id.to_string())
    .bind(action)
    .execute(&db.pool)
    .await?;
    Ok(())
}
