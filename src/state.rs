//! Shared runtime state handed to every command.
//!
//! Cloning is cheap: the database pool, HTTP clients and music manager all use
//! interior mutability or connection pooling.

use std::sync::Arc;
use std::time::Instant;

use poise::serenity_prelude as serenity;
use serenity::Http;

use crate::config::Config;
use crate::db::Db;
use crate::lookup::LookupClient;
use crate::music::MusicManager;
use crate::oracle::OracleClient;

#[derive(Clone)]
pub struct Data {
    pub cfg: Config,
    pub db: Db,
    /// Shared Discord HTTP client. `serenity::Http` is not `Clone`, so we
    /// share it behind an `Arc` (which implements `CacheHttp`).
    pub discord_http: Arc<Http>,
    pub music: Arc<MusicManager>,
    pub lookup: LookupClient,
    pub oracle: OracleClient,
    /// HTTP client used for the signed game gift-code API.
    pub gift_http: reqwest::Client,
    pub started: Instant,
}
impl Data {
    pub fn gift_http(&self) -> &reqwest::Client {
        &self.gift_http
    }

    /// `3d 4h 12m` style uptime.
    pub fn uptime(&self) -> String {
        format_uptime(self.started.elapsed())
    }

    pub async fn music_enabled(&self) -> bool {
        self.cfg.music_enabled() && self.music.client().await.is_some()
    }

    /// Guild id for commands that must run in a server.
    pub fn require_guild_id(
        &self,
        ctx: &crate::commands::Context<'_>,
    ) -> Result<u64, anyhow::Error> {
        ctx.guild_id().map(|id| id.get()).ok_or_else(|| {
            crate::error::OmniError::Other(anyhow::anyhow!("run this inside a server")).into()
        })
    }

    /// The active voice channel of a user, if any.
    pub fn voice_channel_of(&self, ctx: &crate::commands::Context<'_>, user: u64) -> Option<u64> {
        let guild = ctx.guild()?;
        guild
            .voice_states
            .get(&serenity::UserId::new(user))
            .and_then(|state| state.channel_id)
            .map(|id| id.get())
    }

    /// Bot user id, cached after the first gateway READY.
    pub fn bot_id(&self, ctx: &crate::commands::Context<'_>) -> u64 {
        ctx.framework().bot_id().get()
    }
}

pub fn format_uptime(elapsed: std::time::Duration) -> String {
    let total = elapsed.as_secs();
    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else {
        format!("{minutes}m {seconds}s")
    }
}
