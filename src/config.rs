//! Environment-backed configuration.
//!
//! The names mirror the legacy `src/config.js` so an existing `.env` keeps
//! working verbatim.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result};

/// Which runtime banner / brand string to show.
#[derive(Debug, Clone)]
pub struct Branding {
    pub name: String,
    pub tagline: String,
    pub accent: u32,
}

impl Default for Branding {
    fn default() -> Self {
        Self {
            name: "OmniCore".to_string(),
            tagline: "One bot. Every tool.".to_string(),
            accent: 0x5865F2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LavalinkConfig {
    /// Host and port, e.g. `127.0.0.1:2333`. `lavalink-rs` builds `http://{host}/v4`.
    pub hostname: String,
    pub password: String,
    pub is_ssl: bool,
    pub connect_timeout: Duration,
}

impl LavalinkConfig {
    pub fn from_env() -> Option<Self> {
        let host_raw = env_first(&["LAVALINK_HOST", "LAVALINK_URL", "LAVALINK_SERVER"])?;
        let raw = host_raw.trim().to_string();
        if raw.is_empty() {
            return None;
        }

        let is_ssl = raw.starts_with("https://") || truthy(env("LAVALINK_SECURE").as_deref());

        // Lavalink requires `host:port`; the legacy layout used a separate
        // `LAVALINK_PORT` variable, so support both.
        let without_scheme = raw
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .trim_end_matches("/v4")
            .to_string();
        // Drop any path component.
        let authority = without_scheme.split('/').next().unwrap_or(&without_scheme);

        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) if !h.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
                (h.to_string(), p.parse::<u16>().unwrap_or(2333))
            }
            _ => (
                authority.to_string(),
                env_u64("LAVALINK_PORT").unwrap_or(2333) as u16,
            ),
        };

        let password = env("LAVALINK_PASSWORD").unwrap_or_else(|| "youshallnotpass".to_string());
        let local = matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "[::1]");
        if password == "youshallnotpass" && !local {
            tracing::error!(
                "LAVALINK_PASSWORD is unset/default for a non-local Lavalink host — music disabled. Set a strong password."
            );
            return None;
        }

        Some(Self {
            hostname: format!("{host}:{port}"),
            password,
            is_ssl,
            connect_timeout: Duration::from_secs(env_u64("LAVALINK_CONNECT_TIMEOUT").unwrap_or(10)),
        })
    }

    pub fn display_host(&self) -> String {
        format!(
            "{}://{}",
            if self.is_ssl { "https" } else { "http" },
            self.hostname
        )
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub token: String,
    pub client_id: Option<u64>,
    /// When set, commands are registered instantly in this guild (dev mode).
    pub guild_id: Option<u64>,
    pub owner_id: Option<u64>,

    pub database_path: PathBuf,
    pub db_pool_size: u32,

    pub lavalink: Option<LavalinkConfig>,

    pub wos_site: String,
    pub wos_api: String,
    pub wos_api_key: Option<String>,

    pub oracle_site: String,
    pub oracle_api: String,
    pub oracle_codes_url: String,


    pub giftcode_sync_minutes: u64,
    pub lookup_sync_minutes: u64,

    /// Google OAuth (Ministry forms + sheets).
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub google_redirect_uri: String,

    pub branding: Branding,
}

fn env(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| env(k))
}

fn env_u64(key: &str) -> Option<u64> {
    env(key).and_then(|v| v.parse::<u64>().ok())
}

fn truthy(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

fn base_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

impl Config {
    /// Load `.env` (optional) then read the process environment.
    pub fn load() -> Result<Self> {
        // `dotenvy` returns Err when the file is absent; that's fine.
        let _ = dotenvy::dotenv();

        let token = env_first(&["DISCORD_BOT_TOKEN", "TOKEN", "BOT_TOKEN", "DISCORD_TOKEN"])
            .context("no Discord token found (DISCORD_BOT_TOKEN / TOKEN / BOT_TOKEN)")?;

        let database_path =
            PathBuf::from(env("DATABASE_PATH").unwrap_or_else(|| "db/bot.sqlite".to_string()));

        let wos_site = base_url(
            &env_first(&["WHITEOUTSTATS_SITE_URL", "WOS_SITE_URL"])
                .unwrap_or_else(|| "https://www.whiteoutstats.com".to_string()),
        );
        let wos_api = base_url(
            &env_first(&["WHITEOUTSTATS_BASE_URL", "WOS_API_URL"])
                .unwrap_or_else(|| "https://api.whiteoutstats.com/v1".to_string()),
        );

        let oracle_site = base_url(
            &env("ORACLE_SITE_URL").unwrap_or_else(|| "https://wosoracle.com".to_string()),
        );

        let mut branding = Branding::default();
        if let Some(name) = env("BOT_NAME") {
            branding.name = name;
        }
        if let Some(tagline) = env("BOT_TAGLINE") {
            branding.tagline = tagline;
        }
        if let Some(accent) = env("BOT_ACCENT_COLOR") {
            let cleaned = accent.trim_start_matches('#').trim_start_matches("0x");
            if let Ok(value) = u32::from_str_radix(cleaned, 16) {
                branding.accent = value;
            }
        }

        Ok(Self {
            token,
            client_id: env_u64("DISCORD_CLIENT_ID").or_else(|| env_u64("CLIENT_ID")),
            guild_id: env_u64("DISCORD_GUILD_ID").or_else(|| env_u64("GUILD_ID")),
            owner_id: env_u64("OWNER_ID").or_else(|| env_u64("BOT_OWNER_ID")),

            database_path,
            db_pool_size: env_u64("DB_POOL_SIZE").unwrap_or(4) as u32,

            lavalink: LavalinkConfig::from_env(),

            wos_site,
            wos_api,
            wos_api_key: env_first(&["WHITEOUTSTATS_API_KEY", "WOS_API_KEY"]),

            oracle_site: oracle_site.clone(),
            oracle_api: base_url(
                &env("ORACLE_API_URL").unwrap_or_else(|| format!("{oracle_site}/api")),
            ),
            oracle_codes_url: env("ORACLE_CODES_URL")
                .unwrap_or_else(|| "https://wosoracle.com/api/codes".to_string()),

            giftcode_sync_minutes: env_u64("GIFTCODE_SYNC_MINUTES").unwrap_or(30),
            lookup_sync_minutes: env_u64("LOOKUP_SYNC_MINUTES").unwrap_or(60),

            google_client_id: env("GOOGLE_CLIENT_ID"),
            google_client_secret: env("GOOGLE_CLIENT_SECRET"),
            google_redirect_uri: env("GOOGLE_REDIRECT_URI")
                .unwrap_or_else(|| "http://localhost".to_string()),

            branding,
        })
    }

    pub fn music_enabled(&self) -> bool {
        self.lavalink.is_some()
    }

    /// Official OmniCore bot server (env default; per-server override lives in
    /// the DB and wins over this value).
    pub fn official_guild_id() -> Option<u64> {
        std::env::var("OFFICIAL_SERVER_ID")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|id| *id > 0)
    }

    pub fn google_configured(&self) -> bool {
        self.google_client_id.is_some() && self.google_client_secret.is_some()
    }

    pub fn db_dir(&self) -> &Path {
        self.database_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
    }
}
