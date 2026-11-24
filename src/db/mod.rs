//! SQLite access.
//!
//! The schema is intentionally byte-compatible with the legacy Node bot
//! (`src/database/db.js`): same file, same table names, same primary keys.
//! Every migration is additive so the two runtimes can share one database
//! while features are moved over one at a time.

pub mod admin;
pub mod admin_toolkit;
pub mod ballots;
pub mod birthdays;
pub mod branding;
pub mod calculator;
pub mod giftcodes;
pub mod giveaways;
pub mod leveling;
pub mod logging;
pub mod lookup;
pub mod messages;
pub mod music;
pub mod notifications;
pub mod oracle;
pub mod player_limits;
pub mod players;
pub mod roles;
pub mod server_index;
pub mod settings;
pub mod sneak_peek;
pub mod subscriptions;
pub mod tempvoice;
pub mod tickets;
pub mod translate;
pub mod voting;
pub mod welcome;
pub mod wos_credentials;
pub mod wos_relay;

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context as _, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

/// Thin wrapper over the connection pool.
#[derive(Clone, Debug)]
pub struct Db {
    pub pool: SqlitePool,
}

impl Db {
    /// Open (creating if needed) the SQLite database and run migrations.
    pub async fn connect(path: &Path, pool_size: u32) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating database directory {}", parent.display()))?;
            }
        }

        // sqlite needs a real filesystem path, not a URL.
        let mut options = SqliteConnectOptions::from_str(&path.to_string_lossy())
            .with_context(|| format!("invalid database path {}", path.display()))?;

        options = options
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(10));

        let pool = SqlitePoolOptions::new()
            .max_connections(pool_size.max(1))
            .acquire_timeout(Duration::from_secs(15))
            .connect_with(options)
            .await
            .context("opening SQLite database")?;

        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    /// An in-memory database, used by tests.
    #[cfg(test)]
    pub async fn memory() -> Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    /// Create every table the Rust runtime needs.
    pub async fn migrate(&self) -> Result<()> {
        // Every entry is a compile-time literal; only plain `&'static str`
        // values are passed to `query()`.
        for &statement in SCHEMA {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .with_context(|| format!("running schema statement: {}", first_line(statement)))?;
        }
        self.migrate_notification_wizard_columns().await?;
        // Indexes on enrichment columns must wait until the columns exist on
        // databases created by an older runtime.
        self.migrate_players_oracle_columns().await?;
        self.migrate_ministry_v3().await?;
        self.migrate_tempvoice_columns().await?;
        self.migrate_giveaway_rules().await?;
        self.migrate_gift_code_announcement_state().await?;
        self.migrate_ticket_details().await?;
        for &statement in INDEXES {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .with_context(|| format!("running index statement: {}", first_line(statement)))?;
        }
        Ok(())
    }

    async fn migrate_notification_wizard_columns(&self) -> Result<()> {
        use sqlx::Row;
        let rows = sqlx::query("PRAGMA table_info(bear_notifications)")
            .fetch_all(&self.pool)
            .await?;
        let columns = rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("name").ok())
            .collect::<std::collections::HashSet<_>>();
        if !columns.contains("wizard_batch_id") {
            sqlx::query("ALTER TABLE bear_notifications ADD COLUMN wizard_batch_id TEXT")
                .execute(&self.pool)
                .await?;
        }
        if !columns.contains("instance_identifier") {
            sqlx::query("ALTER TABLE bear_notifications ADD COLUMN instance_identifier TEXT")
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn migrate_gift_code_announcement_state(&self) -> Result<()> {
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('gift_codes')")
                .fetch_all(&self.pool)
                .await?;
        if !columns
            .iter()
            .any(|column| column == "announcement_complete")
        {
            sqlx::query(
                "ALTER TABLE gift_codes ADD COLUMN announcement_complete INTEGER NOT NULL DEFAULT 0",
            )
            .execute(&self.pool)
            .await
            .context("adding gift_codes.announcement_complete")?;
        }
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('gift_codes')")
                .fetch_all(&self.pool)
                .await?;
        if !columns
            .iter()
            .any(|column| column == "redemption_handoff_by")
        {
            sqlx::query("ALTER TABLE gift_codes ADD COLUMN redemption_handoff_by TEXT")
                .execute(&self.pool)
                .await
                .context("adding gift_codes.redemption_handoff_by")?;
        }
        if !columns
            .iter()
            .any(|column| column == "redemption_handoff_at")
        {
            sqlx::query("ALTER TABLE gift_codes ADD COLUMN redemption_handoff_at INTEGER")
                .execute(&self.pool)
                .await
                .context("adding gift_codes.redemption_handoff_at")?;
        }
        // Rows marked announced by the legacy runtime are already complete.
        sqlx::query(
            "UPDATE gift_codes SET announcement_complete = 1 WHERE announced <> 0 AND announcement_complete = 0 AND NOT EXISTS (SELECT 1 FROM gift_code_announcement_claims c WHERE c.code = gift_codes.code)",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Rust-only additive column: the JS open-modal `details` field is only
    /// rendered in the welcome embed, but Rust rebuilds that embed on claim,
    /// so it needs to be persisted. Existing ticket rows get NULL.
    async fn migrate_ticket_details(&self) -> Result<()> {
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('tickets')")
                .fetch_all(&self.pool)
                .await?;
        if !columns.iter().any(|column| column == "details") {
            sqlx::query("ALTER TABLE tickets ADD COLUMN details TEXT")
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// Giveaway advanced rules (minLevel/accountDays/serverDays, role bonuses)
    /// were added in the legacy bot later; add the column on older databases.
    async fn migrate_giveaway_rules(&self) -> Result<()> {
        let existing: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('giveaways')")
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
        if !existing.iter().any(|c| c == "rules") {
            sqlx::query("ALTER TABLE giveaways ADD COLUMN rules TEXT NOT NULL DEFAULT '{}'")
                .execute(&self.pool)
                .await
                .context("adding giveaways.rules")?;
            tracing::info!("giveaways: added rules column");
        }
        Ok(())
    }

    /// Temp voice gained linked chat/waiting-room columns over time; add them
    /// on databases created by an older runtime.
    async fn migrate_tempvoice_columns(&self) -> Result<()> {
        let existing: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('tempvoice_channels')")
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
        for column in ["chat_channel_id", "waiting_channel_id"] {
            if !existing.iter().any(|c| c == column) {
                let sql = format!("ALTER TABLE tempvoice_channels ADD COLUMN {column} TEXT");
                sqlx::query(sqlx::AssertSqlSafe(sql))
                    .execute(&self.pool)
                    .await
                    .with_context(|| format!("adding tempvoice_channels.{column}"))?;
                tracing::info!(column, "tempvoice: added column");
            }
        }
        Ok(())
    }

    /// Ministry tables created by the legacy bot may still carry the old
    /// `category IN ('vp','moe')` CHECK constraint and/or miss the v3 columns.
    /// Soft-add missing columns; rebuild the two tables when the CHECK lock
    /// is present. Idempotent and safe to run on every startup.
    async fn migrate_ministry_v3(&self) -> Result<()> {
        const FORMS_COLS: &[(&str, &str)] = &[
            ("workspace_spreadsheet_id", "TEXT"),
            ("applicant_sheet_id", "TEXT"),
            ("td_sheet_id", "TEXT"),
            ("cd_sheet_id", "TEXT"),
            ("rd_sheet_id", "TEXT"),
            ("closes_at", "TEXT"),
        ];
        const SUB_COLS: &[(&str, &str)] = &[
            ("ts", "TEXT"),
            ("cs", "TEXT"),
            ("rs", "TEXT"),
            ("gs", "TEXT"),
            ("preferred_time", "TEXT"),
            ("want_td", "INTEGER DEFAULT 0"),
            ("want_cd", "INTEGER DEFAULT 0"),
            ("want_rd", "INTEGER DEFAULT 0"),
        ];

        async fn soft_add(
            pool: &sqlx::SqlitePool,
            table: &str,
            cols: &[(&str, &str)],
        ) -> Result<Vec<String>> {
            let existing: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT name FROM pragma_table_info('{table}')"
            )))
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            for (name, ty) in cols {
                if !existing.iter().any(|c| c == name) {
                    sqlx::query(sqlx::AssertSqlSafe(format!(
                        "ALTER TABLE {table} ADD COLUMN {name} {ty}"
                    )))
                    .execute(pool)
                    .await
                    .with_context(|| format!("adding {table}.{name}"))?;
                    tracing::info!(table, column = name, "ministry: added column");
                }
            }
            Ok(existing)
        }

        soft_add(&self.pool, "ministry_forms", FORMS_COLS).await?;
        soft_add(&self.pool, "ministry_form_submissions", SUB_COLS).await?;

        // Rebuild if the CHECK constraint still locks categories to vp/moe.
        for (table, rebuild_as, insert_cols, select_cols) in [
            (
                "ministry_forms",
                "ministry_forms_v3",
                "id, guild_id, category, form_id, sheet_id, sheet_gid, form_url, label, status, svs_number, workspace_spreadsheet_id, applicant_sheet_id, td_sheet_id, cd_sheet_id, rd_sheet_id, closes_at, created_at, archived_at",
                "id, guild_id, category, form_id, sheet_id, sheet_gid, form_url, label, CASE status WHEN 'active' THEN 'open' ELSE status END, svs_number, NULL, NULL, NULL, NULL, NULL, NULL, created_at, archived_at",
            ),
            (
                "ministry_form_submissions",
                "ministry_form_submissions_v3",
                "id, guild_id, form_id, category, svs_number, sheet_row_key, applicant_name, fid, speedups, rfc, fc, ts, cs, rs, gs, preferred_time, want_td, want_cd, want_rd, slot_label, slot_index, form_status_field, app_status, reviewed_by, reviewed_at, reject_reason, raw_json, created_at",
                "id, guild_id, form_id, category, svs_number, sheet_row_key, applicant_name, fid, speedups, rfc, fc, NULL, NULL, NULL, NULL, NULL, 0, 0, 0, slot_label, slot_index, form_status_field, app_status, reviewed_by, reviewed_at, reject_reason, raw_json, created_at",
            ),
        ] {
            let sql: Option<String> = sqlx::query_scalar(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name = ?",
            )
            .bind(table)
            .fetch_optional(&self.pool)
            .await
            .unwrap_or_default();

            let needs_rebuild = sql
                .as_deref()
                .map(|s| s.contains("'vp','moe'") || s.contains("'vp', 'moe'"))
                .unwrap_or(false);
            if !needs_rebuild {
                continue;
            }

            let create_ddl = if table == "ministry_forms" {
                "CREATE TABLE ministry_forms_v3 (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    guild_id TEXT NOT NULL,
                    category TEXT NOT NULL,
                    form_id TEXT,
                    sheet_id TEXT,
                    sheet_gid TEXT DEFAULT '0',
                    form_url TEXT,
                    label TEXT,
                    status TEXT NOT NULL DEFAULT 'open',
                    svs_number INTEGER,
                    workspace_spreadsheet_id TEXT,
                    applicant_sheet_id TEXT,
                    td_sheet_id TEXT,
                    cd_sheet_id TEXT,
                    rd_sheet_id TEXT,
                    closes_at TEXT,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    archived_at TEXT
                )"
            } else {
                "CREATE TABLE ministry_form_submissions_v3 (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    guild_id TEXT NOT NULL,
                    form_id INTEGER NOT NULL,
                    category TEXT NOT NULL,
                    svs_number INTEGER,
                    sheet_row_key TEXT NOT NULL,
                    applicant_name TEXT,
                    fid TEXT NOT NULL,
                    speedups TEXT,
                    rfc TEXT,
                    fc TEXT,
                    ts TEXT,
                    cs TEXT,
                    rs TEXT,
                    gs TEXT,
                    preferred_time TEXT,
                    want_td INTEGER DEFAULT 0,
                    want_cd INTEGER DEFAULT 0,
                    want_rd INTEGER DEFAULT 0,
                    slot_label TEXT,
                    slot_index INTEGER,
                    form_status_field TEXT,
                    app_status TEXT NOT NULL DEFAULT 'imported',
                    reviewed_by TEXT,
                    reviewed_at INTEGER,
                    reject_reason TEXT,
                    raw_json TEXT,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    UNIQUE(guild_id, form_id, sheet_row_key)
                )"
            };

            // PRAGMA foreign_keys is per-connection and ignored inside a
            // transaction, so pin one connection, switch FKs off on it, then run
            // the whole copy/drop/rename atomically. A crash can no longer leave
            // the table dropped but not renamed.
            let mut conn = self.pool.acquire().await?;
            sqlx::query("PRAGMA foreign_keys=OFF")
                .execute(&mut *conn)
                .await?;
            let outcome: Result<(), sqlx::Error> = async {
                use sqlx::Connection as _;
                let mut tx = (&mut *conn).begin().await?;
                sqlx::query(sqlx::AssertSqlSafe(create_ddl.to_string()))
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "INSERT INTO {rebuild_as} ({insert_cols}) SELECT {select_cols} FROM {table}"
                )))
                .execute(&mut *tx)
                .await?;
                sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE {table}")))
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "ALTER TABLE {rebuild_as} RENAME TO {table}"
                )))
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                Ok(())
            }
            .await;
            // Always restore FK enforcement on this connection before it returns to the pool.
            let _ = sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await;
            drop(conn);
            outcome?;
            tracing::warn!(table, "ministry: rebuilt table to drop legacy CHECK constraint");
        }

        Ok(())
    }

    /// The legacy `players` table gained enrichment columns over time; add any
    /// that are missing instead of recreating the table.
    async fn migrate_players_oracle_columns(&self) -> Result<()> {
        const COLUMNS: &[(&str, &str)] = &[
            ("game_name", "TEXT"),
            ("alliance_abbr", "TEXT"),
            ("alliance_id", "TEXT"),
            ("alliance_name", "TEXT"),
            ("alliance_rank", "INTEGER"),
            ("power", "INTEGER"),
            ("kills", "INTEGER"),
            ("town_hall_level", "INTEGER"),
            ("life_tree_level", "INTEGER"),
            ("avatar_url", "TEXT"),
            ("oracle_uid", "TEXT"),
            ("last_oracle_sync", "INTEGER"),
        ];

        let existing: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('players')")
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();

        for (name, ty) in COLUMNS {
            if !existing.iter().any(|c| c == name) {
                // `name` and `ty` come from the closed `COLUMNS` table above,
                // never from the network or the database.
                let sql = format!("ALTER TABLE players ADD COLUMN {name} {ty}");
                sqlx::query(sqlx::AssertSqlSafe(sql))
                    .execute(&self.pool)
                    .await
                    .with_context(|| format!("adding players.{name}"))?;
                tracing::info!(column = name, "players: added column");
            }
        }
        Ok(())
    }

    /// Quick health check used by the periodic watchdog.
    pub async fn healthy(&self) -> bool {
        sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }

    /// Run `VACUUM` occasionally to keep a small hosting disk tidy.
    pub async fn vacuum(&self) -> Result<()> {
        sqlx::query("VACUUM").execute(&self.pool).await?;
        Ok(())
    }
}

fn first_line(sql: &str) -> &str {
    sql.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
}

/// Current unix time in seconds.
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Render a unix timestamp as Discord's relative format.
pub fn relative(ts: i64) -> String {
    format!("<t:{ts}:R>")
}

/// Render a unix timestamp as an explicit UTC date-time (`2026-11-19 08:31 UTC`).
pub fn absolute(ts: i64) -> String {
    crate::ui::theme::utc(ts)
}

const SCHEMA: &[&str] = &[
    // ---------------------------------------------------------------- players
    "CREATE TABLE IF NOT EXISTS players (
        fid TEXT PRIMARY KEY,
        user_id TEXT,
        home_guild_id TEXT NOT NULL,
        state INTEGER,
        is_standalone INTEGER NOT NULL DEFAULT 0,
        added_by TEXT NOT NULL,
        added_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    )",
    // Player-ID limits and ownership claims (byte-compatible with the JS bot).
    "CREATE TABLE IF NOT EXISTS giftcode_limit_settings (
        guild_id TEXT PRIMARY KEY,
        base_cap INTEGER NOT NULL DEFAULT 4,
        booster_cap INTEGER NOT NULL DEFAULT 12
    )",
    "CREATE TABLE IF NOT EXISTS giftcode_role_caps (
        guild_id TEXT NOT NULL,
        role_id TEXT NOT NULL,
        cap INTEGER NOT NULL,
        PRIMARY KEY (guild_id, role_id)
    )",
    "CREATE TABLE IF NOT EXISTS giftcode_user_caps (
        user_id TEXT PRIMARY KEY,
        cap INTEGER NOT NULL,
        set_by TEXT,
        set_at INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS pending_claims (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        fid TEXT NOT NULL,
        requesting_user_id TEXT NOT NULL,
        requesting_guild_id TEXT NOT NULL,
        current_owner_id TEXT,
        status TEXT NOT NULL DEFAULT 'pending',
        created_at INTEGER NOT NULL,
        resolved_at INTEGER,
        resolved_by TEXT
    )",
    // ------------------------------------------------------------- music panel
    "CREATE TABLE IF NOT EXISTS music_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT,
        panel_message_id TEXT,
        volume INTEGER NOT NULL DEFAULT 80,
        loop_mode TEXT NOT NULL DEFAULT 'off',
        created_at TEXT DEFAULT (datetime('now'))
    )",
    // ------------------------------------------------------------ lookup panel
    "CREATE TABLE IF NOT EXISTS lookup_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT,
        player_panel_msg_id TEXT,
        alliance_panel_msg_id TEXT,
        state_panel_msg_id TEXT,
        updated_at INTEGER DEFAULT (strftime('%s','now'))
    )",
    // --------------------------------------------------------------- gift codes
    "CREATE TABLE IF NOT EXISTS gift_codes (
        code TEXT PRIMARY KEY,
        status TEXT NOT NULL DEFAULT 'active',
        added_by TEXT NOT NULL,
        source_guild_id TEXT NOT NULL,
        added_at INTEGER NOT NULL,
        date TEXT,
        source TEXT DEFAULT 'manual',
        api_pushed INTEGER NOT NULL DEFAULT 0,
        is_vip INTEGER NOT NULL DEFAULT 0,
        last_validated TEXT,
        announced INTEGER NOT NULL DEFAULT 0,
        announcement_complete INTEGER NOT NULL DEFAULT 0,
        redemption_handoff_by TEXT,
        redemption_handoff_at INTEGER
    )",
    "CREATE INDEX IF NOT EXISTS idx_gift_codes_status ON gift_codes(status)",
    "CREATE TABLE IF NOT EXISTS gift_code_announcements (
        code TEXT NOT NULL,
        guild_id TEXT NOT NULL,
        announced_at INTEGER NOT NULL,
        PRIMARY KEY (code, guild_id)
    )",
    "CREATE TABLE IF NOT EXISTS gift_code_announcement_claims (
        code TEXT PRIMARY KEY,
        lease_until INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS redemptions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        fid TEXT NOT NULL,
        code TEXT NOT NULL,
        status TEXT NOT NULL,
        redeemed_at INTEGER NOT NULL,
        UNIQUE(fid, code)
    )",
    // ---------------------------------------------------------------- per guild
    "CREATE TABLE IF NOT EXISTS settings (
        guild_id TEXT NOT NULL,
        key TEXT NOT NULL,
        value TEXT,
        updated_at INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (guild_id, key)
    )",
    // ------------------------------------------------------------------ oracle
    "CREATE TABLE IF NOT EXISTS oracle_credentials (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        js_auth TEXT NOT NULL,
        api_token TEXT NOT NULL,
        extra_cookie TEXT,
        updated_at INTEGER NOT NULL,
        updated_by TEXT,
        last_test_at INTEGER,
        last_test_ok INTEGER,
        last_test_message TEXT
    )",
    "CREATE TABLE IF NOT EXISTS wos_oracle_credentials (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        api_key TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        updated_by TEXT,
        last_test_at INTEGER,
        last_test_ok INTEGER,
        last_test_message TEXT
    )",
    "CREATE TABLE IF NOT EXISTS oracle_state (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        last_codes_sync INTEGER,
        last_lookup_sync INTEGER,
        last_error TEXT,
        total_codes INTEGER NOT NULL DEFAULT 0,
        total_players INTEGER NOT NULL DEFAULT 0
    )",
    // --------------------------------------------------------------- giveaways
    "CREATE TABLE IF NOT EXISTS giveaways (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        channel_id TEXT NOT NULL,
        message_id TEXT,
        prize TEXT NOT NULL,
        description TEXT,
        winner_count INTEGER NOT NULL DEFAULT 1,
        host_id TEXT NOT NULL,
        required_role_id TEXT,
        ends_at INTEGER NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        winners TEXT NOT NULL DEFAULT '[]',
        rules TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        ended_at TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_giveaways_status_end ON giveaways (status, ends_at)",
    "CREATE INDEX IF NOT EXISTS idx_giveaways_guild ON giveaways (guild_id, status)",
    "CREATE TABLE IF NOT EXISTS giveaway_entries (
        giveaway_id INTEGER NOT NULL,
        user_id TEXT NOT NULL,
        entered_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (giveaway_id, user_id),
        FOREIGN KEY (giveaway_id) REFERENCES giveaways(id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS giveaway_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT
    )",
    // --------------------------------------------------------------- birthdays
    "CREATE TABLE IF NOT EXISTS birthdays (
        guild_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        day INTEGER NOT NULL,
        month INTEGER NOT NULL,
        set_by TEXT NOT NULL,
        set_at TEXT NOT NULL DEFAULT (datetime('now')),
        last_announced TEXT,
        PRIMARY KEY (guild_id, user_id)
    )",
    "CREATE TABLE IF NOT EXISTS birthday_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT,
        panel_message_id TEXT,
        embed_title TEXT,
        embed_description TEXT,
        embed_color INTEGER,
        avatar_mode TEXT NOT NULL DEFAULT 'off'
    )",
    // --------------------------------------------------------------- voting
    "CREATE TABLE IF NOT EXISTS voting_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT,
        results_channel_id TEXT
    )",
    // --------------------------------------------------------------- logging
    "CREATE TABLE IF NOT EXISTS log_settings (
        guild_id TEXT PRIMARY KEY,
        disabled_categories TEXT NOT NULL DEFAULT '[]',
        updated_at INTEGER NOT NULL DEFAULT 0
    )",
    // ------------------------------------------------------------- sneak peek
    "CREATE TABLE IF NOT EXISTS sneak_peek_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT,
        created_at TEXT DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS sneak_peek_state (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        last_url TEXT,
        last_modified TEXT,
        checked_at TEXT DEFAULT (datetime('now'))
    )",
    // ---------------------------------------------------- WOS announcements
    "CREATE TABLE IF NOT EXISTS wos_hub_channel (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        guild_id TEXT NOT NULL,
        channel_id TEXT NOT NULL,
        set_at TEXT DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS wos_relay_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT,
        created_at TEXT DEFAULT (datetime('now'))
    )",
    // ------------------------------------------------------ translatable
    "CREATE TABLE IF NOT EXISTS translatable_messages (
        message_id TEXT PRIMARY KEY,
        guild_id TEXT,
        channel_id TEXT,
        kind TEXT,
        embed_json TEXT,
        content TEXT,
        owner_id TEXT,
        created_at TEXT DEFAULT (datetime('now'))
    )",
    // Byte-compatible with the JavaScript bot's table; additive, so fresh
    // Rust DBs, existing Rust DBs and existing JS DBs all migrate safely.
    "CREATE TABLE IF NOT EXISTS user_language_prefs (
        guild_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        lang TEXT NOT NULL DEFAULT 'en',
        updated_at TEXT DEFAULT (datetime('now')),
        PRIMARY KEY (guild_id, user_id)
    )",
    // ------------------------------------------------------- calculator
    "CREATE TABLE IF NOT EXISTS calculator_settings (
        guild_id TEXT NOT NULL,
        calc_key TEXT NOT NULL,
        channel_id TEXT,
        panel_message_id TEXT,
        created_at TEXT DEFAULT (datetime('now')),
        PRIMARY KEY (guild_id, calc_key)
    )",
    // ---------------------------------------------------- Admin Toolkit
    "CREATE TABLE IF NOT EXISTS admin_toolkit_config (
        guild_id TEXT PRIMARY KEY,
        forum_channel_id TEXT,
        r4_role_ids TEXT NOT NULL DEFAULT '[]',
        r5_role_ids TEXT NOT NULL DEFAULT '[]',
        log_channel_id TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS admin_toolkit_posts (
        guild_id TEXT NOT NULL,
        tool_key TEXT NOT NULL,
        post_id TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (guild_id, tool_key)
    )",
    "CREATE TABLE IF NOT EXISTS admin_toolkit_audit (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        tool_key TEXT,
        action TEXT NOT NULL,
        detail TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_atk_audit_guild ON admin_toolkit_audit (guild_id, id DESC)",
    // ------------------------------------------------------------- admin
    "CREATE TABLE IF NOT EXISTS admin (
        id TEXT PRIMARY KEY,
        is_owner INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS permission_audit_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        actor_id TEXT NOT NULL,
        action TEXT NOT NULL,
        target_id TEXT,
        detail TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    // ------------------------------------------------------- role assignment
    "CREATE TABLE IF NOT EXISTS assignable_roles (
        guild_id TEXT NOT NULL,
        role_id TEXT NOT NULL,
        added_by TEXT NOT NULL,
        added_at TEXT DEFAULT (datetime('now')),
        PRIMARY KEY (guild_id, role_id)
    )",
    "CREATE TABLE IF NOT EXISTS role_panel_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT,
        panel_message_id TEXT,
        created_at TEXT DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS role_assigners (
        guild_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        role_id TEXT NOT NULL,
        granted_by TEXT NOT NULL,
        granted_at TEXT DEFAULT (datetime('now')),
        PRIMARY KEY (guild_id, user_id, role_id)
    )",
    "CREATE TABLE IF NOT EXISTS role_assigner_roles (
        guild_id TEXT NOT NULL,
        granter_role_id TEXT NOT NULL,
        assignable_role_id TEXT NOT NULL,
        granted_by TEXT NOT NULL,
        granted_at TEXT DEFAULT (datetime('now')),
        PRIMARY KEY (guild_id, granter_role_id, assignable_role_id)
    )",
    "CREATE TABLE IF NOT EXISTS role_assignment_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        actor_id TEXT NOT NULL,
        target_user_id TEXT NOT NULL,
        role_id TEXT NOT NULL,
        action TEXT NOT NULL,
        timestamp TEXT NOT NULL
    )",
    // ----------------------------------------------------------- server index
    "CREATE TABLE IF NOT EXISTS server_index_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT,
        message_ids TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        include_threads INTEGER NOT NULL DEFAULT 1,
        include_voice INTEGER NOT NULL DEFAULT 0,
        updated_at TEXT DEFAULT (datetime('now'))
    )",
    // ------------------------------------------------------ notifications
    "CREATE TABLE IF NOT EXISTS bear_notifications (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        channel_id TEXT NOT NULL,
        hour INTEGER NOT NULL,
        minute INTEGER NOT NULL,
        timezone TEXT NOT NULL,
        description TEXT NOT NULL,
        notification_type INTEGER NOT NULL,
        mention_type TEXT NOT NULL DEFAULT 'none',
        repeat_enabled INTEGER NOT NULL DEFAULT 0,
        repeat_minutes INTEGER DEFAULT 0,
        is_enabled INTEGER DEFAULT 1,
        created_at TEXT DEFAULT (datetime('now')),
        created_by TEXT NOT NULL,
        last_notification TEXT,
        next_notification TEXT,
        event_type TEXT,
        wizard_batch_id TEXT,
        instance_identifier TEXT,
        auto_disabled_at TEXT,
        custom_delete_delay_minutes INTEGER DEFAULT NULL
    )",
    "CREATE TABLE IF NOT EXISTS notification_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        notification_id INTEGER NOT NULL,
        notification_time INTEGER NOT NULL,
        sent_at TEXT DEFAULT (datetime('now')),
        message_id TEXT DEFAULT NULL,
        channel_id TEXT DEFAULT NULL,
        scheduled_delete_at TEXT DEFAULT NULL,
        deleted_at TEXT DEFAULT NULL,
        FOREIGN KEY (notification_id) REFERENCES bear_notifications(id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS bear_notification_embeds (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        notification_id INTEGER NOT NULL,
        title TEXT,
        description TEXT,
        color INTEGER,
        image_url TEXT,
        thumbnail_url TEXT,
        footer TEXT,
        author TEXT,
        mention_message TEXT,
        images TEXT,
        created_at TEXT DEFAULT (datetime('now')),
        FOREIGN KEY (notification_id) REFERENCES bear_notifications(id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS notification_days (
        notification_id INTEGER,
        weekday TEXT,
        FOREIGN KEY (notification_id) REFERENCES bear_notifications(id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS wizard_notifications (
        notification_id INTEGER PRIMARY KEY,
        guild_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        created_by_wizard INTEGER DEFAULT 1,
        wizard_run_id TEXT,
        FOREIGN KEY (notification_id) REFERENCES bear_notifications(id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS bear_trap_settings (
        guild_id TEXT PRIMARY KEY,
        delete_messages_enabled INTEGER DEFAULT 1,
        default_delete_delay_minutes INTEGER DEFAULT 60,
        show_daily_reset_on_schedule INTEGER DEFAULT 0
    )",
    "CREATE TABLE IF NOT EXISTS notification_templates (
        template_id INTEGER PRIMARY KEY AUTOINCREMENT,
        template_name TEXT NOT NULL,
        event_type TEXT,
        description TEXT,
        notification_type INTEGER,
        default_times TEXT,
        embed_title TEXT,
        embed_description TEXT,
        embed_color TEXT,
        embed_image_url TEXT,
        embed_thumbnail_url TEXT,
        mention_message TEXT,
        footer TEXT,
        author TEXT,
        repeat_config TEXT,
        is_global INTEGER DEFAULT 1,
        created_by TEXT,
        embed_images TEXT,
        created_at TEXT DEFAULT (datetime('now'))
    )",
    // --------------------------------------------------------- message tools
    "CREATE TABLE IF NOT EXISTS message_groups (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        guild_id TEXT,
        visibility TEXT NOT NULL DEFAULT 'owner',
        created_by TEXT NOT NULL,
        created_at TEXT DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS group_admin_access (
        group_id INTEGER NOT NULL,
        admin_id TEXT NOT NULL,
        granted_at TEXT DEFAULT (datetime('now')),
        PRIMARY KEY (group_id, admin_id),
        FOREIGN KEY (group_id) REFERENCES message_groups(id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS message_library (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        group_id INTEGER NOT NULL,
        name TEXT NOT NULL,
        is_embed INTEGER NOT NULL DEFAULT 1,
        plain_text TEXT,
        mention_type TEXT,
        created_by TEXT NOT NULL,
        created_at TEXT DEFAULT (datetime('now')),
        updated_at TEXT DEFAULT (datetime('now')),
        FOREIGN KEY (group_id) REFERENCES message_groups(id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS message_library_embeds (
        library_id INTEGER PRIMARY KEY,
        title TEXT,
        description TEXT,
        color INTEGER,
        image_url TEXT,
        thumbnail_url TEXT,
        footer TEXT,
        author TEXT,
        images TEXT,
        FOREIGN KEY (library_id) REFERENCES message_library(id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS tracked_messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        channel_id TEXT NOT NULL,
        message_id TEXT NOT NULL,
        is_embed INTEGER NOT NULL DEFAULT 1,
        plain_text TEXT,
        embed_title TEXT,
        embed_description TEXT,
        embed_color INTEGER,
        embed_image_url TEXT,
        embed_thumbnail_url TEXT,
        embed_footer TEXT,
        embed_author TEXT,
        embed_images TEXT,
        library_id INTEGER,
        sent_by TEXT NOT NULL,
        sent_at TEXT DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_tracked_messages_channel ON tracked_messages(channel_id, sent_at DESC)",
    // ---------------------------------------------------------------- ballots
    "CREATE TABLE IF NOT EXISTS ballots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        channel_id TEXT NOT NULL,
        message_id TEXT,
        title TEXT NOT NULL,
        description TEXT,
        questions TEXT NOT NULL,
        role_id TEXT,
        show_votes INTEGER NOT NULL DEFAULT 1,
        allow_change INTEGER NOT NULL DEFAULT 1,
        creator_id TEXT NOT NULL,
        ends_at INTEGER,
        status TEXT NOT NULL DEFAULT 'active',
        last_reminder_at INTEGER,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        closed_at TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_ballots_status_end ON ballots (status, ends_at)",
    "CREATE INDEX IF NOT EXISTS idx_ballots_guild ON ballots (guild_id, status)",
    "CREATE TABLE IF NOT EXISTS ballot_votes (
        ballot_id INTEGER NOT NULL,
        user_id TEXT NOT NULL,
        question_index INTEGER NOT NULL,
        option_index INTEGER NOT NULL,
        PRIMARY KEY (ballot_id, user_id, question_index, option_index),
        FOREIGN KEY (ballot_id) REFERENCES ballots(id) ON DELETE CASCADE
    )",
    "CREATE INDEX IF NOT EXISTS idx_ballot_votes_user ON ballot_votes (ballot_id, user_id)",
    "CREATE TABLE IF NOT EXISTS ballot_voters (
        ballot_id INTEGER NOT NULL,
        user_id TEXT NOT NULL,
        voted_at INTEGER NOT NULL,
        PRIMARY KEY (ballot_id, user_id),
        FOREIGN KEY (ballot_id) REFERENCES ballots(id) ON DELETE CASCADE
    )",
    // ---------------------------------------------------------------- tickets
    "CREATE TABLE IF NOT EXISTS ticket_settings (
        guild_id TEXT PRIMARY KEY,
        panel_channel_id TEXT,
        panel_message_id TEXT,
        category_id TEXT,
        log_channel_id TEXT,
        staff_role_ids TEXT NOT NULL DEFAULT '[]',
        max_open_per_user INTEGER NOT NULL DEFAULT 1,
        panel_title TEXT,
        panel_description TEXT,
        panel_color INTEGER,
        welcome_message TEXT,
        dm_transcript INTEGER NOT NULL DEFAULT 1,
        next_ticket_number INTEGER NOT NULL DEFAULT 1,
        updated_at INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE TABLE IF NOT EXISTS tickets (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        number INTEGER NOT NULL,
        channel_id TEXT NOT NULL UNIQUE,
        opener_id TEXT NOT NULL,
        subject TEXT,
        details TEXT,
        claimed_by TEXT,
        status TEXT NOT NULL DEFAULT 'open',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        closed_at TEXT,
        closed_by TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_tickets_guild_opener ON tickets (guild_id, opener_id, status)",
    // --------------------------------------------------------------- tempvoice
    "CREATE TABLE IF NOT EXISTS tempvoice_settings (
        guild_id TEXT PRIMARY KEY,
        enabled INTEGER NOT NULL DEFAULT 1,
        lobby_channel_id TEXT,
        category_id TEXT,
        name_template TEXT NOT NULL DEFAULT '{user}''s channel',
        user_limit INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE TABLE IF NOT EXISTS tempvoice_channels (
        channel_id TEXT PRIMARY KEY,
        guild_id TEXT NOT NULL,
        owner_id TEXT NOT NULL,
        created_at INTEGER NOT NULL DEFAULT 0,
        chat_channel_id TEXT,
        waiting_channel_id TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_tempvoice_guild ON tempvoice_channels (guild_id)",
    // ------------------------------------------------------------------ leveling
    "CREATE TABLE IF NOT EXISTS level_settings (
        guild_id TEXT PRIMARY KEY,
        enabled INTEGER NOT NULL DEFAULT 1,
        xp_min INTEGER NOT NULL DEFAULT 15,
        xp_max INTEGER NOT NULL DEFAULT 25,
        cooldown_sec INTEGER NOT NULL DEFAULT 60,
        voice_xp INTEGER NOT NULL DEFAULT 0,
        announce_mode TEXT NOT NULL DEFAULT 'same',
        announce_channel_id TEXT,
        announce_message TEXT,
        stack_rewards INTEGER NOT NULL DEFAULT 1,
        ignored_channel_ids TEXT NOT NULL DEFAULT '[]',
        updated_at TEXT DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS level_users (
        guild_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        xp INTEGER NOT NULL DEFAULT 0,
        messages INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (guild_id, user_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_level_users_rank ON level_users (guild_id, xp DESC)",
    "CREATE TABLE IF NOT EXISTS level_rewards (
        guild_id TEXT NOT NULL,
        level INTEGER NOT NULL,
        role_id TEXT NOT NULL,
        PRIMARY KEY (guild_id, role_id)
    )",
    // ------------------------------------------------------------------ welcome
    "CREATE TABLE IF NOT EXISTS welcome_settings (
        guild_id TEXT PRIMARY KEY,
        channel_id TEXT,
        embed_title TEXT,
        embed_description TEXT,
        embed_color INTEGER,
        avatar_mode TEXT NOT NULL DEFAULT 'off'
    )",
    "CREATE TABLE IF NOT EXISTS feature_cleanup_settings (
        guild_id TEXT NOT NULL,
        feature TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 0,
        delay_minutes INTEGER NOT NULL DEFAULT 1440,
        updated_at TEXT DEFAULT (datetime('now')),
        PRIMARY KEY (guild_id, feature)
    )",
    "CREATE TABLE IF NOT EXISTS feature_message_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        feature TEXT NOT NULL,
        channel_id TEXT NOT NULL,
        message_id TEXT NOT NULL,
        sent_at TEXT DEFAULT (datetime('now')),
        scheduled_delete_at TEXT,
        deleted_at TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_feature_message_history_due ON feature_message_history(feature, scheduled_delete_at) WHERE deleted_at IS NULL",
    "CREATE TABLE IF NOT EXISTS autorole_settings (
        guild_id TEXT PRIMARY KEY,
        enabled INTEGER NOT NULL DEFAULT 1,
        member_role_ids TEXT NOT NULL DEFAULT '[]',
        bot_role_ids TEXT NOT NULL DEFAULT '[]',
        updated_at INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE TABLE IF NOT EXISTS join_dm_settings (
        guild_id TEXT PRIMARY KEY,
        join_dm_enabled INTEGER NOT NULL DEFAULT 1,
        register_reminder_enabled INTEGER NOT NULL DEFAULT 1,
        reminder_interval_days INTEGER NOT NULL DEFAULT 7,
        updated_at INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE TABLE IF NOT EXISTS registration_reminders (
        guild_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        joined_at INTEGER NOT NULL DEFAULT 0,
        join_dm_sent_at INTEGER,
        last_reminder_at INTEGER,
        reminder_count INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (guild_id, user_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_reg_reminders_last ON registration_reminders(last_reminder_at)",
    // ------------------------------------------------------------------- stats
    "CREATE TABLE IF NOT EXISTS bot_stats (
        key TEXT PRIMARY KEY,
        value INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL DEFAULT 0
    )",

    // ------------------------------------------ owner panel / subscriptions
    "CREATE TABLE IF NOT EXISTS server_subscriptions (
        guild_id TEXT PRIMARY KEY,
        tier TEXT NOT NULL DEFAULT 'free',
        status TEXT NOT NULL DEFAULT 'active',
        starts_at TEXT,
        expires_at TEXT,
        payment_status TEXT NOT NULL DEFAULT 'none',
        approved_by TEXT,
        player_id_limit INTEGER,
        notes TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS server_feature_flags (
        guild_id TEXT NOT NULL,
        feature_key TEXT NOT NULL,
        enabled INTEGER NOT NULL,
        set_by TEXT,
        set_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (guild_id, feature_key)
    )",
    "CREATE TABLE IF NOT EXISTS server_gift_priority (
        guild_id TEXT PRIMARY KEY,
        priority INTEGER NOT NULL DEFAULT 100
    )",
    "CREATE INDEX IF NOT EXISTS idx_server_gift_priority ON server_gift_priority(priority)",
    "CREATE TABLE IF NOT EXISTS server_staff (
        guild_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        role TEXT NOT NULL,
        added_by TEXT,
        added_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (guild_id, user_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_server_staff_user ON server_staff(user_id)",
    "CREATE TABLE IF NOT EXISTS official_server_config (
        key TEXT PRIMARY KEY,
        value TEXT
    )",
    "CREATE TABLE IF NOT EXISTS wos_stats_credentials (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        api_key TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        updated_by TEXT,
        last_test_at INTEGER,
        last_test_ok INTEGER,
        last_test_message TEXT
    )",

    // -------------------------------------------------------------- branding
    "CREATE TABLE IF NOT EXISTS bot_branding (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        contact_enabled INTEGER NOT NULL DEFAULT 0,
        contact_label TEXT DEFAULT 'Contact Dev',
        contact_url TEXT,
        updated_at TEXT DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS bot_guild_profiles (
        guild_id TEXT PRIMARY KEY,
        nickname TEXT,
        avatar_url TEXT,
        updated_at TEXT DEFAULT (datetime('now'))
    )",

    // ------------------------------------------------------------ ministry
    "CREATE TABLE IF NOT EXISTS ministry_settings (
        guild_id TEXT PRIMARY KEY,
        target_state TEXT NOT NULL DEFAULT '3105',
        member_channel_id TEXT,
        member_message_id TEXT,
        admin_channel_id TEXT,
        admin_message_id TEXT,
        slot_minutes INTEGER NOT NULL DEFAULT 30,
        slots_per_day INTEGER NOT NULL DEFAULT 48,
        svs_number INTEGER DEFAULT 1,
        language TEXT DEFAULT 'en',
        applications_open INTEGER DEFAULT 1,
        last_sheet_sync_at TEXT,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    // Legacy-compatible reliable sheet-write queue (JS `ministry_sheet_outbox`).
    "CREATE TABLE IF NOT EXISTS ministry_sheet_outbox (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT,
        dedupe_key TEXT NOT NULL UNIQUE,
        kind TEXT NOT NULL DEFAULT 'append',
        spreadsheet_id TEXT NOT NULL,
        range_a1 TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        attempts INTEGER NOT NULL DEFAULT 0,
        last_error TEXT,
        status TEXT NOT NULL DEFAULT 'pending',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        completed_at TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_mso_status ON ministry_sheet_outbox(status, attempts)",
    "CREATE TABLE IF NOT EXISTS ministry_forms (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        category TEXT NOT NULL,
        form_id TEXT,
        sheet_id TEXT,
        sheet_gid TEXT DEFAULT '0',
        form_url TEXT,
        label TEXT,
        status TEXT NOT NULL DEFAULT 'open',
        svs_number INTEGER,
        workspace_spreadsheet_id TEXT,
        applicant_sheet_id TEXT,
        td_sheet_id TEXT,
        cd_sheet_id TEXT,
        rd_sheet_id TEXT,
        closes_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        archived_at TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_ministry_forms_guild ON ministry_forms(guild_id, category, status)",
    "CREATE TABLE IF NOT EXISTS ministry_form_submissions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        form_id INTEGER NOT NULL,
        category TEXT NOT NULL,
        svs_number INTEGER,
        sheet_row_key TEXT NOT NULL,
        applicant_name TEXT,
        fid TEXT NOT NULL,
        speedups TEXT,
        rfc TEXT,
        fc TEXT,
        ts TEXT,
        cs TEXT,
        rs TEXT,
        gs TEXT,
        preferred_time TEXT,
        want_td INTEGER DEFAULT 0,
        want_cd INTEGER DEFAULT 0,
        want_rd INTEGER DEFAULT 0,
        slot_label TEXT,
        slot_index INTEGER,
        form_status_field TEXT,
        app_status TEXT NOT NULL DEFAULT 'imported',
        reviewed_by TEXT,
        reviewed_at INTEGER,
        reject_reason TEXT,
        raw_json TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(guild_id, form_id, sheet_row_key)
    )",
    "CREATE INDEX IF NOT EXISTS idx_ministry_fsub_guild ON ministry_form_submissions(guild_id, category, app_status, svs_number)",
    "CREATE TABLE IF NOT EXISTS ministry_finalizations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT NOT NULL,
        form_id INTEGER,
        svs_number INTEGER,
        event_key TEXT NOT NULL,
        player_name TEXT,
        fid TEXT NOT NULL,
        slot_utc TEXT NOT NULL,
        origin TEXT NOT NULL DEFAULT 'form',
        added_by TEXT,
        submission_id INTEGER,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(guild_id, svs_number, event_key, fid, slot_utc)
    )",
    "CREATE INDEX IF NOT EXISTS idx_ministry_fin_guild ON ministry_finalizations(guild_id, svs_number, event_key)",
    "CREATE TABLE IF NOT EXISTS ministry_sync_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT,
        kind TEXT,
        message TEXT,
        ok INTEGER DEFAULT 1,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS ministry_google_tokens (
        scope_key TEXT PRIMARY KEY,
        access_token TEXT,
        refresh_token TEXT,
        expiry_ts INTEGER,
        email TEXT,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS ministry_player_registry (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id TEXT,
        fid TEXT NOT NULL,
        player_name TEXT,
        source TEXT,
        conflict_note TEXT,
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(guild_id, fid)
    )",
    "CREATE INDEX IF NOT EXISTS idx_mpr_guild ON ministry_player_registry(guild_id)",

    // Cross-server custom emoji: one home guild owns the uploads; every other
    // server the bot is in can use the <:name:id> tags without local copies.
    "CREATE TABLE IF NOT EXISTS emoji_home_guild (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        guild_id TEXT NOT NULL,
        set_at TEXT DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS material_emojis (
        material_key TEXT PRIMARY KEY,
        tag TEXT NOT NULL,
        updated_at TEXT DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS emoji_file_hashes (
        file_name TEXT PRIMARY KEY,
        hash TEXT NOT NULL,
        updated_at TEXT DEFAULT (datetime('now'))
    )",
];

/// Indexes that depend on columns added by `migrate_players_oracle_columns`.
const INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_players_user_id ON players(user_id)",
    "CREATE INDEX IF NOT EXISTS idx_players_home_guild ON players(home_guild_id)",
    "CREATE INDEX IF NOT EXISTS idx_players_alliance_abbr ON players(alliance_abbr)",
    "CREATE INDEX IF NOT EXISTS idx_players_state ON players(state)",
    "CREATE INDEX IF NOT EXISTS idx_players_game_name ON players(game_name)",
    "CREATE INDEX IF NOT EXISTS idx_gift_codes_announced ON gift_codes(announced)",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The JS bot's translatable_messages table and rows survive migration,
    /// and the new language-preference table is added non-destructively.
    #[tokio::test]
    async fn player_id_tables_migrate_with_existing_data() {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(
            "CREATE TABLE players (
                fid TEXT PRIMARY KEY, user_id TEXT, home_guild_id TEXT NOT NULL, state INTEGER,
                is_standalone INTEGER NOT NULL DEFAULT 0, added_by TEXT NOT NULL,
                added_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO players (fid, user_id, home_guild_id, state, added_by, added_at, updated_at)
             VALUES ('123', '42', '9', 3105, '42', 1000, 1000)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let db = Db { pool: pool.clone() };
        db.migrate().await.unwrap();
        let player = crate::db::players::get_by_fid(&db, "123")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(player.user_id.as_deref(), Some("42"));
        assert_eq!(player.state, Some(3105));
        assert_eq!(crate::db::players::count_by_user(&db, 42).await.unwrap(), 1);
        assert_eq!(
            crate::db::player_limits::resolve_limit(&db, 9, 42, false, &[])
                .await
                .unwrap()
                .cap,
            4
        );
    }

    #[tokio::test]
    async fn gift_code_tables_migrate_without_data_loss() {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        // Legacy/JS-shaped rows: the Rust migration must add nothing destructive.
        sqlx::query(
            "CREATE TABLE gift_codes (
                code TEXT PRIMARY KEY, status TEXT NOT NULL DEFAULT 'active', added_by TEXT NOT NULL,
                source_guild_id TEXT NOT NULL, added_at INTEGER NOT NULL, date TEXT,
                source TEXT DEFAULT 'manual', api_pushed INTEGER NOT NULL DEFAULT 0,
                is_vip INTEGER NOT NULL DEFAULT 0, last_validated TEXT,
                announced INTEGER NOT NULL DEFAULT 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE redemptions (
                id INTEGER PRIMARY KEY AUTOINCREMENT, fid TEXT NOT NULL, code TEXT NOT NULL,
                status TEXT NOT NULL, redeemed_at INTEGER NOT NULL, UNIQUE(fid, code))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO gift_codes (code, added_by, source_guild_id, added_at) VALUES ('COFFEE1001', '7', '9', 1000)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO redemptions (fid, code, status, redeemed_at) VALUES ('123', 'COFFEE1001', 'SUCCESS', 1001)")
            .execute(&pool)
            .await
            .unwrap();

        let db = Db { pool: pool.clone() };
        db.migrate().await.unwrap();

        let code = crate::db::giftcodes::get(&db, "COFFEE1001")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(code.status, "active");
        let done = crate::db::giftcodes::fids_redeemed_for_code(&db, "COFFEE1001")
            .await
            .unwrap();
        assert!(done.contains("123"));
        let statuses = crate::db::giftcodes::statuses_for_code(&db, "COFFEE1001")
            .await
            .unwrap();
        assert_eq!(statuses, vec![("123".to_string(), "SUCCESS".to_string())]);

        // Upsert semantics preserve one row per (fid, code).
        crate::db::giftcodes::record_redemption(&db, "123", "COFFEE1001", "RECEIVED")
            .await
            .unwrap();
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM redemptions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn notification_wizard_columns_add_without_changing_existing_rows() {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(
            "CREATE TABLE bear_notifications (
                id INTEGER PRIMARY KEY, guild_id TEXT NOT NULL, channel_id TEXT NOT NULL,
                hour INTEGER NOT NULL, minute INTEGER NOT NULL, timezone TEXT NOT NULL,
                description TEXT NOT NULL, notification_type INTEGER NOT NULL,
                mention_type TEXT NOT NULL DEFAULT 'none', repeat_enabled INTEGER NOT NULL DEFAULT 0,
                repeat_minutes INTEGER DEFAULT 0, is_enabled INTEGER DEFAULT 1,
                created_at TEXT DEFAULT (datetime('now')), created_by TEXT NOT NULL,
                last_notification TEXT, next_notification TEXT, event_type TEXT,
                custom_delete_delay_minutes INTEGER DEFAULT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO bear_notifications (id, guild_id, channel_id, hour, minute, timezone, description, notification_type, created_by, event_type) VALUES (41, '7', '8', 14, 30, 'UTC', 'legacy reminder', 2, '9', 'foundry_battle')")
            .execute(&pool)
            .await
            .unwrap();

        let db = Db { pool: pool.clone() };
        db.migrate().await.unwrap();
        let row: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT description, wizard_batch_id, instance_identifier FROM bear_notifications WHERE id = 41",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row, ("legacy reminder".into(), None, None));
    }

    #[tokio::test]
    async fn translate_tables_migrate_on_existing_database() {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(
            "CREATE TABLE translatable_messages (
                message_id TEXT PRIMARY KEY, guild_id TEXT, channel_id TEXT, kind TEXT,
                embed_json TEXT, content TEXT, owner_id TEXT,
                created_at TEXT DEFAULT (datetime('now')))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO translatable_messages (message_id, content) VALUES ('1', 'legacy content')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let db = Db { pool: pool.clone() };
        db.migrate().await.unwrap();

        let row: (String,) =
            sqlx::query_as("SELECT content FROM translatable_messages WHERE message_id = '1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, "legacy content");
        assert_eq!(
            crate::db::translate::get_user_lang(&db, Some(9), 5).await,
            "en"
        );
        crate::db::translate::set_user_lang(&db, Some(9), 5, "de")
            .await
            .unwrap();
        assert_eq!(
            crate::db::translate::get_user_lang(&db, Some(9), 5).await,
            "de"
        );
    }
}
