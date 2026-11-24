//! Giveaway storage: `giveaways`, `giveaway_entries`.
//!
//! Entries are one-per-user; the winner draw picks uniformly at random, or
//! weighted when advanced rules grant bonus entries (`giveaways.rules`).

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GiveawayRules {
    /// 0 = off. Minimum leveling level to enter.
    pub min_level: i64,
    /// 0 = off. Minimum Discord account age in days.
    pub min_account_days: i64,
    /// 0 = off. Minimum membership age in this server in days.
    pub min_server_days: i64,
    /// Members holding any of these roles can't enter or win.
    pub blocked_role_ids: Vec<u64>,
    /// Members holding these roles get extra entries.
    pub bonus_role_ids: Vec<u64>,
    /// Extra entries per bonus role held.
    pub role_bonus: i64,
    /// Extra entries for server boosters.
    pub booster_bonus: i64,
    /// +1 entry per N leveling levels (0 = off).
    pub level_step: i64,
    /// Cap on total entries per person.
    pub max_weight: i64,
}

impl GiveawayRules {
    pub fn normalize(&mut self) {
        self.min_level = self.min_level.clamp(0, 1000);
        self.min_account_days = self.min_account_days.clamp(0, 3650);
        self.min_server_days = self.min_server_days.clamp(0, 3650);
        self.blocked_role_ids.sort_unstable();
        self.blocked_role_ids.dedup();
        self.blocked_role_ids.truncate(10);
        self.bonus_role_ids.sort_unstable();
        self.bonus_role_ids.dedup();
        self.bonus_role_ids.truncate(10);
        self.role_bonus = self.role_bonus.clamp(0, 50);
        self.booster_bonus = self.booster_bonus.clamp(0, 50);
        self.level_step = self.level_step.clamp(0, 1000);
        self.max_weight = self.max_weight.clamp(1, 100);
    }

    pub fn from_json(raw: &str) -> Self {
        let mut rules = if raw.trim().is_empty() {
            Self::default()
        } else {
            serde_json::from_str(raw).unwrap_or_default()
        };
        rules.normalize();
        if rules.max_weight == 0 {
            rules.max_weight = 10;
        }
        rules
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn has_requirements(&self) -> bool {
        self.min_level > 0
            || self.min_account_days > 0
            || self.min_server_days > 0
            || !self.blocked_role_ids.is_empty()
    }

    pub fn has_bonuses(&self) -> bool {
        (!self.bonus_role_ids.is_empty() && self.role_bonus > 0)
            || self.booster_bonus > 0
            || self.level_step > 0
    }

    /// One-line-per-section summary for embeds (mirrors the legacy describe()).
    pub fn describe(&self) -> (Vec<String>, Vec<String>) {
        let mut requirements = Vec::new();
        if self.min_level > 0 {
            requirements.push(format!("level **{}+**", self.min_level));
        }
        if self.min_account_days > 0 {
            requirements.push(format!("account **{}d+** old", self.min_account_days));
        }
        if self.min_server_days > 0 {
            requirements.push(format!("in server **{}d+**", self.min_server_days));
        }
        if !self.blocked_role_ids.is_empty() {
            requirements.push(format!(
                "not {}",
                self.blocked_role_ids
                    .iter()
                    .map(|id| format!("<@&{id}>"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let mut bonuses = Vec::new();
        if !self.bonus_role_ids.is_empty() && self.role_bonus > 0 {
            bonuses.push(format!(
                "{}: **+{}** each",
                self.bonus_role_ids
                    .iter()
                    .map(|id| format!("<@&{id}>"))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.role_bonus
            ));
        }
        if self.booster_bonus > 0 {
            bonuses.push(format!("Server Boosters: **+{}**", self.booster_bonus));
        }
        if self.level_step > 0 {
            bonuses.push(format!("**+1** per {} level(s)", self.level_step));
        }
        if !bonuses.is_empty() {
            bonuses.push(format!("max **{}** entries each", self.max_weight.max(1)));
        }
        (requirements, bonuses)
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct Giveaway {
    pub id: i64,
    pub guild_id: String,
    pub channel_id: String,
    pub message_id: Option<String>,
    pub prize: String,
    pub description: Option<String>,
    pub winner_count: i64,
    pub host_id: String,
    pub required_role_id: Option<String>,
    pub ends_at: i64,
    pub status: String,
    pub winners: String,
    pub rules: String,
    pub created_at: String,
    pub ended_at: Option<String>,
}

impl Giveaway {
    pub fn channel_id(&self) -> Option<u64> {
        self.channel_id.parse().ok()
    }

    pub fn message_id(&self) -> Option<u64> {
        self.message_id.as_deref().and_then(|v| v.parse().ok())
    }

    pub fn required_role_id(&self) -> Option<u64> {
        self.required_role_id
            .as_deref()
            .and_then(|v| v.parse().ok())
    }

    pub fn winner_count(&self) -> usize {
        self.winner_count.clamp(1, 20) as usize
    }

    pub fn winners(&self) -> Vec<u64> {
        serde_json::from_str::<Vec<String>>(&self.winners)
            .map(|v| v.into_iter().filter_map(|id| id.parse().ok()).collect())
            .unwrap_or_default()
    }

    pub fn is_active(&self) -> bool {
        self.status.eq_ignore_ascii_case("active")
    }

    pub fn rules(&self) -> GiveawayRules {
        GiveawayRules::from_json(&self.rules)
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    prize: &str,
    description: Option<&str>,
    winner_count: u64,
    host_id: u64,
    required_role_id: Option<u64>,
    ends_at: i64,
) -> sqlx::Result<i64> {
    let result = sqlx::query(
        "INSERT INTO giveaways
             (guild_id, channel_id, prize, description, winner_count, host_id, required_role_id, ends_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.to_string())
    .bind(prize.trim())
    .bind(description.map(str::trim))
    .bind(winner_count.clamp(1, 20) as i64)
    .bind(host_id.to_string())
    .bind(required_role_id.map(|id| id.to_string()))
    .bind(ends_at)
    .execute(&db.pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get(db: &Db, id: i64) -> sqlx::Result<Option<Giveaway>> {
    sqlx::query_as::<_, Giveaway>("SELECT * FROM giveaways WHERE id = ?")
        .bind(id)
        .fetch_optional(&db.pool)
        .await
}

pub async fn set_message_id(db: &Db, id: i64, message_id: u64) -> sqlx::Result<()> {
    sqlx::query("UPDATE giveaways SET message_id = ? WHERE id = ?")
        .bind(message_id.to_string())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Remove a just-created giveaway when its public post could not be sent.
pub async fn delete_unposted(db: &Db, id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM giveaways WHERE id = ? AND message_id IS NULL")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn list_due(db: &Db, now_ms: i64) -> sqlx::Result<Vec<Giveaway>> {
    sqlx::query_as::<_, Giveaway>(
        "SELECT * FROM giveaways WHERE status = 'active' AND ends_at <= ? ORDER BY ends_at",
    )
    .bind(now_ms)
    .fetch_all(&db.pool)
    .await
}

pub async fn count_active(db: &Db, guild_id: u64) -> sqlx::Result<i64> {
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM giveaways WHERE guild_id = ? AND status = 'active'")
            .bind(guild_id.to_string())
            .fetch_one(&db.pool)
            .await?;
    Ok(n)
}

pub async fn list_active(db: &Db, guild_id: u64, limit: i64) -> sqlx::Result<Vec<Giveaway>> {
    sqlx::query_as::<_, Giveaway>(
        "SELECT * FROM giveaways WHERE guild_id = ? AND status = 'active' ORDER BY ends_at ASC LIMIT ?",
    )
    .bind(guild_id.to_string())
    .bind(limit)
    .fetch_all(&db.pool)
    .await
}

pub async fn list_recent(db: &Db, guild_id: u64, limit: i64) -> sqlx::Result<Vec<Giveaway>> {
    // Legacy listForManage: active giveaways first (soonest end first), then
    // the most recent finished/cancelled ones by id.
    sqlx::query_as::<_, Giveaway>(
        "SELECT * FROM giveaways WHERE guild_id = ?
         ORDER BY CASE WHEN status = 'active' THEN 0 ELSE 1 END,
                  CASE WHEN status = 'active' THEN ends_at ELSE 0 END ASC,
                  id DESC
         LIMIT ?",
    )
    .bind(guild_id.to_string())
    .bind(limit.clamp(1, 25))
    .fetch_all(&db.pool)
    .await
}

/// active -> ended. true only for the caller that flipped it.
pub async fn claim_end(db: &Db, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE giveaways SET status = 'ended', ended_at = datetime('now') WHERE id = ? AND status = 'active'",
    )
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// active -> cancelled. Returns true only when this call claimed the active row.
pub async fn claim_cancel(db: &Db, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE giveaways SET status = 'cancelled', ended_at = datetime('now') WHERE id = ? AND status = 'active'",
    )
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Undo `claim_end` when finishing a giveaway failed, so the sweep retries it.
pub async fn reopen(db: &Db, id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE giveaways SET status = 'active', ended_at = NULL WHERE id = ? AND status = 'ended'",
    )
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Drop a departed member's entries from this server's still-active giveaways.
pub async fn remove_entries_for_user(db: &Db, guild_id: u64, user_id: u64) -> sqlx::Result<u64> {
    let result = sqlx::query(
        "DELETE FROM giveaway_entries WHERE user_id = ? AND giveaway_id IN \
         (SELECT id FROM giveaways WHERE guild_id = ? AND status = 'active')",
    )
    .bind(user_id.to_string())
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn set_winners(db: &Db, id: i64, winners: &[u64]) -> sqlx::Result<()> {
    let encoded =
        serde_json::to_string(&winners.iter().map(|id| id.to_string()).collect::<Vec<_>>())
            .unwrap_or_else(|_| "[]".to_string());
    sqlx::query("UPDATE giveaways SET winners = ? WHERE id = ?")
        .bind(encoded)
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn add_entry(db: &Db, giveaway_id: i64, user_id: u64) -> sqlx::Result<bool> {
    let result =
        sqlx::query("INSERT OR IGNORE INTO giveaway_entries (giveaway_id, user_id) VALUES (?, ?)")
            .bind(giveaway_id)
            .bind(user_id.to_string())
            .execute(&db.pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Remove a member's entry (the public "Leave Giveaway" action).
pub async fn remove_entry(db: &Db, giveaway_id: i64, user_id: u64) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM giveaway_entries WHERE giveaway_id = ? AND user_id = ?")
        .bind(giveaway_id)
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn has_entry(db: &Db, giveaway_id: i64, user_id: u64) -> sqlx::Result<bool> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM giveaway_entries WHERE giveaway_id = ? AND user_id = ?")
            .bind(giveaway_id)
            .bind(user_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.is_some())
}

pub async fn count_entries(db: &Db, giveaway_id: i64) -> sqlx::Result<i64> {
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM giveaway_entries WHERE giveaway_id = ?")
            .bind(giveaway_id)
            .fetch_one(&db.pool)
            .await?;
    Ok(n)
}

pub async fn list_entries(db: &Db, giveaway_id: i64) -> sqlx::Result<Vec<u64>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT user_id FROM giveaway_entries WHERE giveaway_id = ?")
            .bind(giveaway_id)
            .fetch_all(&db.pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id,)| id.parse().ok())
        .collect())
}

/// Uniformly random winner draw without repeats.
pub async fn draw_winners(db: &Db, giveaway: &Giveaway, exclude: &[u64]) -> sqlx::Result<Vec<u64>> {
    use rand::seq::SliceRandom;

    let mut entries = list_entries(db, giveaway.id).await?;
    if !exclude.is_empty() {
        entries.retain(|id| !exclude.contains(id));
    }
    entries.shuffle(&mut rand::thread_rng());
    Ok(entries.into_iter().take(giveaway.winner_count()).collect())
}

/// Weighted winner draw: each entry's tickets come from `weight(user_id)`
/// (1+). Winners are drawn without repeats.
pub async fn draw_winners_weighted<F>(
    db: &Db,
    giveaway: &Giveaway,
    exclude: &[u64],
    weight: F,
) -> sqlx::Result<Vec<u64>>
where
    F: Fn(u64) -> i64,
{
    use rand::seq::SliceRandom;

    let entries = list_entries(db, giveaway.id).await?;
    let entries: Vec<u64> = entries
        .into_iter()
        .filter(|id| !exclude.contains(id))
        .collect();

    let mut expanded: Vec<u64> = Vec::with_capacity(entries.len() * 4);
    for entry in &entries {
        let tickets = weight(*entry).clamp(1, 100);
        for _ in 0..tickets {
            expanded.push(*entry);
        }
    }
    expanded.shuffle(&mut rand::thread_rng());

    let mut winners = Vec::with_capacity(giveaway.winner_count());
    for id in expanded {
        if !winners.contains(&id) {
            winners.push(id);
            if winners.len() >= giveaway.winner_count() {
                break;
            }
        }
    }
    Ok(winners)
}

/// Set the advanced-rules JSON blob for a giveaway (normalized on write).
pub async fn set_rules(db: &Db, id: i64, rules: &GiveawayRules) -> sqlx::Result<()> {
    let mut rules = rules.clone();
    rules.normalize();
    sqlx::query("UPDATE giveaways SET rules = ? WHERE id = ?")
        .bind(rules.to_json())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}
