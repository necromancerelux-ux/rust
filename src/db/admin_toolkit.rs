//! Admin Toolkit storage: forum config, per-tool posts and the audit log.

use sqlx::Row;

use super::Db;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolkitConfig {
    pub guild_id: String,
    pub forum_channel_id: Option<u64>,
    pub r4_role_ids: Vec<u64>,
    pub r5_role_ids: Vec<u64>,
    pub log_channel_id: Option<u64>,
    pub enabled: bool,
}

impl ToolkitConfig {
    pub fn r4_role_ids(&self) -> &[u64] {
        &self.r4_role_ids
    }

    pub fn r5_role_ids(&self) -> &[u64] {
        &self.r5_role_ids
    }

    pub fn matches_role(&self, role_ids: &[u64]) -> Level {
        match (
            role_ids.iter().any(|id| self.r5_role_ids.contains(id)),
            role_ids.iter().any(|id| self.r4_role_ids.contains(id)),
        ) {
            (true, _) => Level::R5,
            (false, true) => Level::R4,
            (false, false) => Level::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    None,
    R4,
    R5,
    Admin,
    Owner,
}

impl Level {
    pub fn rank(self) -> i32 {
        match self {
            Self::None => 0,
            Self::R4 => 1,
            Self::R5 | Self::Admin => 2,
            Self::Owner => 3,
        }
    }

    pub fn meets(self, min: Self) -> bool {
        self.rank() >= min.rank()
    }
}

impl ToolkitConfig {
    pub async fn for_guild(db: &Db, guild_id: u64) -> sqlx::Result<Option<Self>> {
        let row = sqlx::query(
            "SELECT forum_channel_id, r4_role_ids, r5_role_ids, log_channel_id, enabled
             FROM admin_toolkit_config WHERE guild_id = ?",
        )
        .bind(guild_id.to_string())
        .fetch_optional(&db.pool)
        .await?;
        Ok(row.map(|row| Self {
            guild_id: guild_id.to_string(),
            forum_channel_id: row
                .get::<Option<String>, _>("forum_channel_id")
                .and_then(|v| v.parse().ok()),
            r4_role_ids: parse_role_list(row.get::<String, _>("r4_role_ids")),
            r5_role_ids: parse_role_list(row.get::<String, _>("r5_role_ids")),
            log_channel_id: row
                .get::<Option<String>, _>("log_channel_id")
                .and_then(|v| v.parse().ok()),
            enabled: row.get::<i64, _>("enabled") != 0,
        }))
    }
}

fn parse_role_list(raw: String) -> Vec<u64> {
    serde_json::from_str::<Vec<String>>(&raw)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| id.parse().ok())
        .collect()
}

fn role_list_json(ids: &[u64]) -> String {
    serde_json::to_string(&ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
        .unwrap_or_else(|_| "[]".to_string())
}

/// Tristate patch: `None` = leave unchanged, `Some(None)` = clear the field,
/// `Some(Some(id))` = set it. Role lists are always replaced wholesale when
/// present, so an empty selection can intentionally clear all roles.
#[derive(Debug, Clone, Default)]
pub struct ConfigPatch {
    pub forum_channel_id: Option<Option<u64>>,
    pub r4_role_ids: Option<Vec<u64>>,
    pub r5_role_ids: Option<Vec<u64>>,
    pub log_channel_id: Option<Option<u64>>,
    pub enabled: Option<bool>,
}

async fn write_config(db: &Db, config: &ToolkitConfig) -> sqlx::Result<ToolkitConfig> {
    sqlx::query(
        "INSERT INTO admin_toolkit_config (guild_id, forum_channel_id, r4_role_ids, r5_role_ids, log_channel_id, enabled)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET
            forum_channel_id = excluded.forum_channel_id,
            r4_role_ids = excluded.r4_role_ids,
            r5_role_ids = excluded.r5_role_ids,
            log_channel_id = excluded.log_channel_id,
            enabled = excluded.enabled,
            updated_at = datetime('now')",
    )
    .bind(config.guild_id.clone())
    .bind(config.forum_channel_id.map(|id| id.to_string()))
    .bind(role_list_json(&config.r4_role_ids))
    .bind(role_list_json(&config.r5_role_ids))
    .bind(config.log_channel_id.map(|id| id.to_string()))
    .bind(if config.enabled { 1 } else { 0 })
    .execute(&db.pool)
    .await?;
    Ok(config.clone())
}

/// Apply a patch that distinguishes "leave unchanged" from "clear this value".
pub async fn apply_patch(
    db: &Db,
    guild_id: u64,
    patch: ConfigPatch,
) -> sqlx::Result<ToolkitConfig> {
    let mut config = ToolkitConfig::for_guild(db, guild_id)
        .await?
        .unwrap_or_else(|| ToolkitConfig {
            guild_id: guild_id.to_string(),
            ..Default::default()
        });
    if let Some(forum) = patch.forum_channel_id {
        config.forum_channel_id = forum;
    }
    if let Some(roles) = patch.r4_role_ids {
        config.r4_role_ids = roles;
    }
    if let Some(roles) = patch.r5_role_ids {
        config.r5_role_ids = roles;
    }
    if let Some(log) = patch.log_channel_id {
        config.log_channel_id = log;
    }
    if let Some(enabled) = patch.enabled {
        config.enabled = enabled;
    }
    write_config(db, &config).await
}

/// Replace the R4 role list (an empty list intentionally clears it).
pub async fn set_r4_roles(db: &Db, guild_id: u64, roles: Vec<u64>) -> sqlx::Result<ToolkitConfig> {
    apply_patch(
        db,
        guild_id,
        ConfigPatch {
            r4_role_ids: Some(roles),
            ..Default::default()
        },
    )
    .await
}

/// Replace the R5 role list (an empty list intentionally clears it).
pub async fn set_r5_roles(db: &Db, guild_id: u64, roles: Vec<u64>) -> sqlx::Result<ToolkitConfig> {
    apply_patch(
        db,
        guild_id,
        ConfigPatch {
            r5_role_ids: Some(roles),
            ..Default::default()
        },
    )
    .await
}

pub async fn set_forum(db: &Db, guild_id: u64, forum: u64) -> sqlx::Result<ToolkitConfig> {
    apply_patch(
        db,
        guild_id,
        ConfigPatch {
            forum_channel_id: Some(Some(forum)),
            ..Default::default()
        },
    )
    .await
}

pub async fn set_log_channel(db: &Db, guild_id: u64, channel: u64) -> sqlx::Result<ToolkitConfig> {
    apply_patch(
        db,
        guild_id,
        ConfigPatch {
            log_channel_id: Some(Some(channel)),
            ..Default::default()
        },
    )
    .await
}

pub async fn clear_log_channel(db: &Db, guild_id: u64) -> sqlx::Result<ToolkitConfig> {
    apply_patch(
        db,
        guild_id,
        ConfigPatch {
            log_channel_id: Some(None),
            ..Default::default()
        },
    )
    .await
}

pub async fn set_enabled(db: &Db, guild_id: u64, enabled: bool) -> sqlx::Result<ToolkitConfig> {
    apply_patch(
        db,
        guild_id,
        ConfigPatch {
            enabled: Some(enabled),
            ..Default::default()
        },
    )
    .await
}

pub async fn save_config(
    db: &Db,
    guild_id: u64,
    patch: &ToolkitConfig,
) -> sqlx::Result<ToolkitConfig> {
    let current = ToolkitConfig::for_guild(db, guild_id)
        .await?
        .unwrap_or_else(|| {
            ToolkitConfig {
                guild_id: guild_id.to_string(),
                ..Default::default()
            }
            .clone()
        });
    let merged = ToolkitConfig {
        guild_id: current.guild_id.clone(),
        forum_channel_id: patch.forum_channel_id.or(current.forum_channel_id),
        r4_role_ids: if patch.r4_role_ids.is_empty() {
            current.r4_role_ids
        } else {
            patch.r4_role_ids.clone()
        },
        r5_role_ids: if patch.r5_role_ids.is_empty() {
            current.r5_role_ids
        } else {
            patch.r5_role_ids.clone()
        },
        log_channel_id: patch.log_channel_id.or(current.log_channel_id),
        enabled: patch.enabled,
    };
    sqlx::query(
        "INSERT INTO admin_toolkit_config (guild_id, forum_channel_id, r4_role_ids, r5_role_ids, log_channel_id, enabled)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET
            forum_channel_id = excluded.forum_channel_id,
            r4_role_ids = excluded.r4_role_ids,
            r5_role_ids = excluded.r5_role_ids,
            log_channel_id = excluded.log_channel_id,
            enabled = excluded.enabled,
            updated_at = datetime('now')",
    )
    .bind(guild_id.to_string())
    .bind(merged.forum_channel_id.map(|id| id.to_string()))
    .bind(role_list_json(&merged.r4_role_ids))
    .bind(role_list_json(&merged.r5_role_ids))
    .bind(merged.log_channel_id.map(|id| id.to_string()))
    .bind(if merged.enabled { 1 } else { 0 })
    .execute(&db.pool)
    .await?;
    Ok(merged)
}

pub async fn clear_forum(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    sqlx::query("UPDATE admin_toolkit_config SET forum_channel_id = NULL WHERE guild_id = ?")
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn list_toolkit_guilds(db: &Db) -> sqlx::Result<Vec<u64>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT guild_id FROM admin_toolkit_config WHERE enabled = 1 AND forum_channel_id IS NOT NULL",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id,)| id.parse().ok())
        .collect())
}

pub async fn get_posts(
    db: &Db,
    guild_id: u64,
) -> sqlx::Result<std::collections::HashMap<String, u64>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT tool_key, post_id FROM admin_toolkit_posts WHERE guild_id = ?")
            .bind(guild_id.to_string())
            .fetch_all(&db.pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(tool, post)| Some((tool, post.parse().ok()?)))
        .collect())
}

pub async fn set_post(db: &Db, guild_id: u64, tool_key: &str, post_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO admin_toolkit_posts (guild_id, tool_key, post_id) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, tool_key) DO UPDATE SET post_id = excluded.post_id, updated_at = datetime('now')",
    )
    .bind(guild_id.to_string())
    .bind(tool_key)
    .bind(post_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn clear_posts(db: &Db, guild_id: u64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM admin_toolkit_posts WHERE guild_id = ?")
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Which guild/tool owns this forum post.
pub async fn find_post(db: &Db, post_id: u64) -> sqlx::Result<Option<(u64, String)>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT guild_id, tool_key FROM admin_toolkit_posts WHERE post_id = ?")
            .bind(post_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.map(|(guild, tool)| (guild.parse().unwrap_or(0), tool)))
}

pub async fn add_audit(
    db: &Db,
    guild_id: u64,
    user_id: u64,
    tool_key: Option<&str>,
    action: &str,
    detail: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO admin_toolkit_audit (guild_id, user_id, tool_key, action, detail)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(guild_id.to_string())
    .bind(user_id.to_string())
    .bind(tool_key)
    .bind(action)
    .bind(detail.map(|d| d.chars().take(500).collect::<String>()))
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: i64,
    pub user_id: String,
    pub tool_key: Option<String>,
    pub action: String,
    pub detail: Option<String>,
    pub created_at: String,
}

pub async fn list_audit(db: &Db, guild_id: u64, limit: i64) -> sqlx::Result<Vec<AuditEntry>> {
    let rows = sqlx::query(
        "SELECT id, user_id, tool_key, action, detail, created_at
         FROM admin_toolkit_audit WHERE guild_id = ? ORDER BY id DESC LIMIT ?",
    )
    .bind(guild_id.to_string())
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| AuditEntry {
            id: row.get("id"),
            user_id: row.get("user_id"),
            tool_key: row.get("tool_key"),
            action: row.get("action"),
            detail: row.get("detail"),
            created_at: row.get("created_at"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn patch_can_clear_and_replace_values() {
        let db = Db::memory().await.unwrap();
        let cfg = apply_patch(
            &db,
            1,
            ConfigPatch {
                forum_channel_id: Some(Some(22)),
                r4_role_ids: Some(vec![3, 4]),
                r5_role_ids: Some(vec![5]),
                log_channel_id: Some(Some(77)),
                enabled: Some(true),
            },
        )
        .await
        .unwrap();
        assert_eq!(cfg.forum_channel_id, Some(22));
        assert_eq!(cfg.r4_role_ids, vec![3, 4]);
        assert_eq!(cfg.log_channel_id, Some(77));

        // Empty role lists replace the old selections; None leaves values alone.
        let cfg = apply_patch(
            &db,
            1,
            ConfigPatch {
                r4_role_ids: Some(vec![]),
                log_channel_id: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(cfg.r4_role_ids.is_empty());
        assert_eq!(cfg.r5_role_ids, vec![5]);
        assert_eq!(cfg.forum_channel_id, Some(22));
        assert_eq!(cfg.log_channel_id, None);
        assert!(cfg.enabled);

        let stored = ToolkitConfig::for_guild(&db, 1).await.unwrap().unwrap();
        assert_eq!(stored, cfg);
    }
}
