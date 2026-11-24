use std::{
    io::{Cursor, Write},
    path::Path,
    str::FromStr,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

/// Match the JS owner-panel download: a ZIP containing the database file.
pub fn zip_database_file(database_path: &Path) -> Result<(Vec<u8>, String)> {
    let bytes = std::fs::read(database_path)
        .with_context(|| format!("reading database file {}", database_path.display()))?;
    let entry_name = database_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("bot.sqlite");
    let timestamp = chrono::Utc::now()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        .replace(':', "-")
        .replace('.', "-");
    let filename = format!("devguide-backup-{timestamp}.zip");
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(9));
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    archive.start_file(entry_name, options)?;
    archive.write_all(&bytes)?;
    let archive = archive.finish()?.into_inner();
    Ok((archive, filename))
}

/// Build the JS-style ZIP from a consistent online SQLite snapshot.
pub async fn archive_database_snapshot(source: &Path) -> Result<(Vec<u8>, String)> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let scratch_dir =
        std::env::temp_dir().join(format!("omnicore-backup-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&scratch_dir).with_context(|| {
        format!(
            "creating temporary backup directory {}",
            scratch_dir.display()
        )
    })?;
    let entry_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("bot.sqlite");
    let snapshot_path = scratch_dir.join(entry_name);

    let result = async {
        snapshot_database(source, &snapshot_path).await?;
        let path_for_zip = snapshot_path.clone();
        tokio::task::spawn_blocking(move || zip_database_file(&path_for_zip))
            .await
            .context("joining database ZIP worker")?
    }
    .await;
    let _ = std::fs::remove_dir_all(&scratch_dir);
    result
}

/// Create a transactionally consistent SQLite snapshot without copying live WAL files.
pub async fn snapshot_database(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
        bail!("source database does not exist");
    }
    if destination.exists() {
        bail!("snapshot destination already exists");
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating snapshot directory {}", parent.display()))?;
    }

    let options = SqliteConnectOptions::from_str(&source.to_string_lossy())
        .context("parsing source database path")?
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(60));
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .context("opening source database for snapshot")?;

    // VACUUM INTO takes a consistent SQLite snapshot, including committed WAL data.
    // Escape single quotes because SQLite does not accept a bind parameter here.
    let destination_sql = destination.to_string_lossy().replace('\'', "''");
    let statement = format!("VACUUM INTO '{destination_sql}'");
    sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
        .execute(&mut connection)
        .await
        .context("creating consistent SQLite snapshot")?;
    connection
        .close()
        .await
        .context("closing snapshot connection")?;
    Ok(())
}
