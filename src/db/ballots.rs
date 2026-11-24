//! WoS ballot storage: `ballots`, `ballot_votes`, `ballot_voters`.
//!
//! A ballot is a multi-question vote (WoS style: ministry elections, yes/no
//! votes across points..). Each participant submits ONE complete ballot.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BallotQuestion {
    pub text: String,
    pub options: Vec<String>,
    #[serde(rename = "maxPicks", default = "one")]
    pub max_picks: i64,
}

fn one() -> i64 {
    1
}

#[derive(Debug, Clone, FromRow)]
struct BallotRow {
    id: i64,
    guild_id: String,
    channel_id: String,
    message_id: Option<String>,
    title: String,
    description: Option<String>,
    questions: String,
    role_id: Option<String>,
    show_votes: i64,
    allow_change: i64,
    creator_id: String,
    ends_at: Option<i64>,
    status: String,
    last_reminder_at: Option<i64>,
    created_at: String,
    closed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Ballot {
    pub id: i64,
    pub guild_id: String,
    pub channel_id: String,
    pub message_id: Option<String>,
    pub title: String,
    pub description: String,
    pub questions: Vec<BallotQuestion>,
    pub role_id: Option<String>,
    pub show_votes: bool,
    pub allow_change: bool,
    pub creator_id: String,
    pub ends_at: Option<i64>,
    pub status: String,
    pub last_reminder_at: i64,
}

impl Ballot {
    fn from_row(row: BallotRow) -> Self {
        let questions =
            serde_json::from_str::<Vec<BallotQuestion>>(&row.questions).unwrap_or_default();
        Self {
            id: row.id,
            guild_id: row.guild_id,
            channel_id: row.channel_id,
            message_id: row.message_id,
            title: row.title,
            description: row.description.unwrap_or_default(),
            questions,
            role_id: row.role_id,
            show_votes: row.show_votes != 0,
            allow_change: row.allow_change != 0,
            creator_id: row.creator_id,
            ends_at: row.ends_at,
            status: row.status,
            last_reminder_at: row.last_reminder_at.unwrap_or(0),
        }
    }

    pub fn is_active(&self) -> bool {
        self.status.eq_ignore_ascii_case("active")
    }

    pub fn channel_id(&self) -> Option<u64> {
        self.channel_id.parse().ok()
    }

    pub fn message_id(&self) -> Option<u64> {
        self.message_id.as_deref().and_then(|v| v.parse().ok())
    }

    pub fn expected_role_id(&self) -> Option<u64> {
        self.role_id.as_deref().and_then(|v| v.parse().ok())
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    title: &str,
    description: Option<&str>,
    questions: &[BallotQuestion],
    role_id: Option<u64>,
    show_votes: bool,
    allow_change: bool,
    creator_id: u64,
    ends_at: Option<i64>,
) -> sqlx::Result<i64> {
    let questions_json = serde_json::to_string(questions).unwrap_or_else(|_| "[]".to_string());
    let result = sqlx::query(
        "INSERT INTO ballots
             (guild_id, channel_id, title, description, questions, role_id, show_votes, allow_change, creator_id, ends_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.to_string())
    .bind(title.trim())
    .bind(description.map(str::trim))
    .bind(questions_json)
    .bind(role_id.map(|id| id.to_string()))
    .bind(show_votes as i64)
    .bind(allow_change as i64)
    .bind(creator_id.to_string())
    .bind(ends_at)
    .execute(&db.pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get(db: &Db, id: i64) -> sqlx::Result<Option<Ballot>> {
    let row = sqlx::query_as::<_, BallotRow>("SELECT * FROM ballots WHERE id = ?")
        .bind(id)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row.map(Ballot::from_row))
}

pub async fn latest(db: &Db, guild_id: u64) -> sqlx::Result<Option<Ballot>> {
    let row = sqlx::query_as::<_, BallotRow>(
        "SELECT * FROM ballots WHERE guild_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(Ballot::from_row))
}

pub async fn set_message_id(db: &Db, id: i64, message_id: u64) -> sqlx::Result<()> {
    sqlx::query("UPDATE ballots SET message_id = ? WHERE id = ?")
        .bind(message_id.to_string())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn list_due(db: &Db, now_ms: i64) -> sqlx::Result<Vec<Ballot>> {
    let rows = sqlx::query_as::<_, BallotRow>(
        "SELECT * FROM ballots WHERE status = 'active' AND ends_at IS NOT NULL AND ends_at <= ? ORDER BY ends_at",
    )
    .bind(now_ms)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.into_iter().map(Ballot::from_row).collect())
}

pub async fn list_active_for_guild(
    db: &Db,
    guild_id: u64,
    limit: i64,
) -> sqlx::Result<Vec<Ballot>> {
    let rows = sqlx::query_as::<_, BallotRow>(
        "SELECT * FROM ballots WHERE guild_id = ? AND status = 'active' ORDER BY id DESC LIMIT ?",
    )
    .bind(guild_id.to_string())
    .bind(limit.clamp(1, 25))
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.into_iter().map(Ballot::from_row).collect())
}

pub async fn count_active(db: &Db, guild_id: u64) -> sqlx::Result<i64> {
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM ballots WHERE guild_id = ? AND status = 'active'")
            .bind(guild_id.to_string())
            .fetch_one(&db.pool)
            .await?;
    Ok(n)
}

/// active -> closed; true only when this call flipped it.
pub async fn claim_close(db: &Db, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE ballots SET status = 'closed', closed_at = datetime('now') WHERE id = ? AND status = 'active'",
    )
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Replace a voter's complete ballot atomically.
pub async fn submit(
    db: &Db,
    ballot_id: i64,
    user_id: u64,
    answers: &[Vec<usize>],
) -> sqlx::Result<()> {
    let mut tx = db.pool.begin().await?;

    sqlx::query("DELETE FROM ballot_votes WHERE ballot_id = ? AND user_id = ?")
        .bind(ballot_id)
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;

    for (question_index, picks) in answers.iter().enumerate() {
        for option_index in picks {
            sqlx::query(
                "INSERT OR IGNORE INTO ballot_votes (ballot_id, user_id, question_index, option_index)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(ballot_id)
            .bind(user_id.to_string())
            .bind(question_index as i64)
            .bind(*option_index as i64)
            .execute(&mut *tx)
            .await?;
        }
    }

    sqlx::query(
        "INSERT INTO ballot_voters (ballot_id, user_id, voted_at) VALUES (?, ?, ?)
         ON CONFLICT(ballot_id, user_id) DO UPDATE SET voted_at = excluded.voted_at",
    )
    .bind(ballot_id)
    .bind(user_id.to_string())
    .bind(super::now() * 1000)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn withdraw(db: &Db, ballot_id: i64, user_id: u64) -> sqlx::Result<()> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM ballot_votes WHERE ballot_id = ? AND user_id = ?")
        .bind(ballot_id)
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM ballot_voters WHERE ballot_id = ? AND user_id = ?")
        .bind(ballot_id)
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn has_voted(db: &Db, ballot_id: i64, user_id: u64) -> sqlx::Result<bool> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM ballot_voters WHERE ballot_id = ? AND user_id = ?")
            .bind(ballot_id)
            .bind(user_id.to_string())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.is_some())
}

/// A voter's answers as `answers[question_index] -> option indices`.
pub async fn answers(db: &Db, ballot: &Ballot, user_id: u64) -> sqlx::Result<Vec<Vec<usize>>> {
    let mut out: Vec<Vec<usize>> = ballot.questions.iter().map(|_| Vec::new()).collect();
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT question_index, option_index FROM ballot_votes
         WHERE ballot_id = ? AND user_id = ? ORDER BY option_index",
    )
    .bind(ballot.id)
    .bind(user_id.to_string())
    .fetch_all(&db.pool)
    .await?;

    for (q, o) in rows {
        if let Some(list) = out.get_mut(q as usize) {
            list.push(o as usize);
        }
    }
    Ok(out)
}

/// Everyone who voted with timestamps, for result/CSV delivery.
pub async fn list_voter_details(
    db: &Db,
    ballot: &Ballot,
) -> sqlx::Result<Vec<(u64, i64, Vec<Vec<usize>>)>> {
    let voters: Vec<(String, i64)> = sqlx::query_as(
        "SELECT user_id, voted_at FROM ballot_voters WHERE ballot_id = ? ORDER BY voted_at, rowid",
    )
    .bind(ballot.id)
    .fetch_all(&db.pool)
    .await?;
    let mut out = Vec::with_capacity(voters.len());
    for (user_id, voted_at) in voters {
        let Ok(user_id) = user_id.parse::<u64>() else {
            continue;
        };
        let answers = answers(db, ballot, user_id).await?;
        out.push((user_id, voted_at, answers));
    }
    Ok(out)
}

/// Everyone who voted, in vote order: `user_id -> answers`.
pub async fn list_voters(db: &Db, ballot: &Ballot) -> sqlx::Result<Vec<(u64, Vec<Vec<usize>>)>> {
    let voters: Vec<(String, i64)> = sqlx::query_as(
        "SELECT user_id, voted_at FROM ballot_voters WHERE ballot_id = ? ORDER BY voted_at, rowid",
    )
    .bind(ballot.id)
    .fetch_all(&db.pool)
    .await?;

    let mut out = Vec::with_capacity(voters.len());
    for (user_id, _) in voters {
        let Ok(user_id) = user_id.parse::<u64>() else {
            continue;
        };
        let answers = answers(db, ballot, user_id).await?;
        out.push((user_id, answers));
    }
    Ok(out)
}

/// `(counts per question per option, total voters)`.
pub async fn tally(db: &Db, ballot: &Ballot) -> sqlx::Result<(Vec<Vec<i64>>, i64)> {
    let mut counts: Vec<Vec<i64>> = ballot
        .questions
        .iter()
        .map(|q| vec![0; q.options.len()])
        .collect();

    let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT question_index, option_index, COUNT(*) FROM ballot_votes
         WHERE ballot_id = ? GROUP BY question_index, option_index",
    )
    .bind(ballot.id)
    .fetch_all(&db.pool)
    .await?;

    for (q, o, n) in rows {
        if let Some(options) = counts.get_mut(q as usize) {
            if let Some(slot) = options.get_mut(o as usize) {
                *slot = n;
            }
        }
    }

    let (voters,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM ballot_voters WHERE ballot_id = ?")
            .bind(ballot.id)
            .fetch_one(&db.pool)
            .await?;

    Ok((counts, voters))
}
