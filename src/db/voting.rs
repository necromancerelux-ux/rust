//! Voting (WoS ballot) settings: default vote channel and auto-post channel.

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, FromRow)]
struct VotingRow {
    channel_id: Option<String>,
    results_channel_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VotingSettings {
    pub channel_id: Option<u64>,
    pub results_channel_id: Option<u64>,
}

pub async fn settings(db: &Db, guild_id: u64) -> sqlx::Result<VotingSettings> {
    let row = sqlx::query_as::<_, VotingRow>(
        "SELECT channel_id, results_channel_id FROM voting_settings WHERE guild_id = ?",
    )
    .bind(guild_id.to_string())
    .fetch_optional(&db.pool)
    .await?;

    Ok(match row {
        Some(row) => VotingSettings {
            channel_id: row.channel_id.as_deref().and_then(|v| v.parse().ok()),
            results_channel_id: row
                .results_channel_id
                .as_deref()
                .and_then(|v| v.parse().ok()),
        },
        None => VotingSettings::default(),
    })
}

pub async fn set_channel(db: &Db, guild_id: u64, channel: Option<u64>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO voting_settings (guild_id, channel_id) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET channel_id = excluded.channel_id",
    )
    .bind(guild_id.to_string())
    .bind(channel.map(|id| id.to_string()))
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_results(db: &Db, guild_id: u64, channel: Option<u64>) -> sqlx::Result<()> {
    // The base row may not exist yet; seed it then fill the results column.
    sqlx::query(
        "INSERT INTO voting_settings (guild_id, channel_id) VALUES (?, NULL)
         ON CONFLICT(guild_id) DO NOTHING",
    )
    .bind(guild_id.to_string())
    .execute(&db.pool)
    .await?;

    sqlx::query("UPDATE voting_settings SET results_channel_id = ? WHERE guild_id = ?")
        .bind(channel.map(|id| id.to_string()))
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}
