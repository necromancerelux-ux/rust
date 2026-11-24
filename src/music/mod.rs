//! Music playback: Lavalink (Rustalink) client plus our own queue.
//!
//! `lavalink-rs` ships a queue, but it does not reliably auto-advance across
//! track-end reasons, and we need the panel to stay in sync. So the bot keeps
//! the queue itself and only uses `PlayerContext::play_now`/`skip`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use lavalink_rs::model::events as lava_events;
use lavalink_rs::model::track::{TrackData, TrackLoadData};
use lavalink_rs::prelude::*;
use poise::serenity_prelude as serenity;
use serenity::{ChannelId, Context as SerenityContext, GuildId as SGuildId, MessageId};
use tokio::sync::{mpsc, Mutex};

use crate::config::LavalinkConfig;
use crate::db::music::LoopMode;

/// Lavalink's guild id is a plain newtype over u64.
fn lava_gid(id: u64) -> GuildId {
    GuildId(id)
}

/// One queued track plus who asked for it.
#[derive(Debug, Clone)]
pub struct QueuedTrack {
    pub data: TrackData,
    pub requester: String,
    pub requester_id: u64,
}

impl QueuedTrack {
    fn new(data: TrackData, requester: String, requester_id: u64) -> Self {
        Self {
            data,
            requester,
            requester_id,
        }
    }

    pub fn title(&self) -> &str {
        &self.data.info.title
    }

    pub fn length(&self) -> u64 {
        self.data.info.length
    }
}

/// Per-guild playback state.
#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    pub queue: VecDeque<QueuedTrack>,
    pub current: Option<QueuedTrack>,
    pub position_ms: u64,
    pub volume: u16,
    pub paused: bool,
    pub loop_mode: LoopMode,
    pub text_channel: Option<u64>,
    pub panel_message: Option<u64>,
    pub voice_channel: Option<u64>,
}

impl PlayerState {
    pub fn current_length(&self) -> u64 {
        self.current.as_ref().map(|t| t.length()).unwrap_or(0)
    }

    pub fn is_idle(&self) -> bool {
        self.current.is_none() && self.queue.is_empty()
    }
}

/// Internal events from the Lavalink socket.
#[derive(Debug, Clone)]
pub enum MusicEvent {
    Ready,
    TrackStart {
        guild_id: u64,
        title: String,
    },
    TrackEnd {
        guild_id: u64,
        reason: &'static str,
        should_continue: bool,
    },
    TrackException {
        guild_id: u64,
        message: String,
    },
    WebSocketClosed {
        guild_id: u64,
        code: u16,
    },
}

impl MusicEvent {
    pub fn guild_id(&self) -> Option<u64> {
        match self {
            Self::Ready => None,
            Self::TrackStart { guild_id, .. }
            | Self::TrackEnd { guild_id, .. }
            | Self::TrackException { guild_id, .. }
            | Self::WebSocketClosed { guild_id, .. } => Some(*guild_id),
        }
    }
}

/// Global event sender. `lavalink-rs` requires plain `fn` pointers for handlers,
/// so the channel is the only way to reach runtime state.
static EVENT_TX: std::sync::OnceLock<mpsc::UnboundedSender<MusicEvent>> =
    std::sync::OnceLock::new();

fn publish(event: MusicEvent) {
    if let Some(tx) = EVENT_TX.get() {
        let _ = tx.send(event);
    }
}

/// The shared music service.
pub struct MusicManager {
    cfg: Option<LavalinkConfig>,
    client: Mutex<Option<Arc<LavalinkClient>>>,
    states: Mutex<std::collections::HashMap<u64, PlayerState>>,
    bot_id: Mutex<Option<u64>>,
    events: Mutex<Option<mpsc::UnboundedReceiver<MusicEvent>>>,
    tx: mpsc::UnboundedSender<MusicEvent>,
}

impl MusicManager {
    pub fn new(cfg: Option<LavalinkConfig>) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = EVENT_TX.set(tx.clone());
        Arc::new(Self {
            cfg,
            client: Mutex::new(None),
            states: Mutex::new(std::collections::HashMap::new()),
            bot_id: Mutex::new(None),
            events: Mutex::new(Some(rx)),
            tx,
        })
    }

    pub fn enabled(&self) -> bool {
        self.cfg.is_some()
    }

    pub fn describe_host(&self) -> Option<String> {
        self.cfg.as_ref().map(|c| c.display_host())
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<MusicEvent> {
        self.tx.clone()
    }

    /// Take the event receiver exactly once; the background loop owns it.
    pub async fn take_events(&self) -> Option<mpsc::UnboundedReceiver<MusicEvent>> {
        self.events.lock().await.take()
    }

    /// Connect to Lavalink. Called from the framework setup once the bot id is
    /// known. Safe to call twice (the second call is a no-op).
    pub async fn connect(&self, bot_id: u64) -> Result<()> {
        let Some(cfg) = self.cfg.clone() else {
            return Err(anyhow!("Lavalink is not configured"));
        };

        if self.client.lock().await.is_some() {
            return Ok(());
        }

        // lavalink-rs retries a dead node forever (an error every 15 s). Probe the
        // port first so a missing Lavalink simply disables music.
        let probe = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::TcpStream::connect(cfg.hostname.as_str()),
        )
        .await;
        match probe {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => {
                return Err(anyhow!(
                    "no Lavalink server reachable at {} ({err}). Remove LAVALINK_HOST from .env to turn music off",
                    cfg.hostname
                ))
            }
            Err(_) => {
                return Err(anyhow!(
                    "Lavalink at {} timed out. Remove LAVALINK_HOST from .env to turn music off",
                    cfg.hostname
                ))
            }
        }

        *self.bot_id.lock().await = Some(bot_id);

        let node = NodeBuilder {
            hostname: cfg.hostname.clone(),
            is_ssl: cfg.is_ssl,
            password: cfg.password.clone(),
            user_id: UserId(bot_id),
            ..Default::default()
        };

        let client =
            LavalinkClient::new(events(), vec![node], NodeDistributionStrategy::sharded()).await;

        tracing::info!(host = %cfg.display_host(), "connected to Lavalink");
        *self.client.lock().await = Some(Arc::new(client));
        Ok(())
    }

    pub async fn client(&self) -> Option<Arc<LavalinkClient>> {
        self.client.lock().await.clone()
    }

    pub async fn bot_id(&self) -> Option<u64> {
        *self.bot_id.lock().await
    }

    // ------------------------------------------------------------- state

    pub async fn state(&self, guild_id: u64) -> Option<PlayerState> {
        self.states.lock().await.get(&guild_id).cloned()
    }

    pub async fn with_state<F, T>(&self, guild_id: u64, f: F) -> T
    where
        F: FnOnce(&mut PlayerState) -> T,
    {
        let mut guard = self.states.lock().await;
        let entry = guard.entry(guild_id).or_default();
        f(entry)
    }

    pub async fn set_volume_state(&self, guild_id: u64, volume: u16) {
        self.with_state(guild_id, |s| s.volume = volume).await;
    }

    pub async fn set_loop_state(&self, guild_id: u64, mode: LoopMode) {
        self.with_state(guild_id, |s| s.loop_mode = mode).await;
    }

    pub async fn set_text_channel(&self, guild_id: u64, channel_id: u64) {
        self.with_state(guild_id, |s| s.text_channel = Some(channel_id))
            .await;
    }

    pub async fn set_panel(&self, guild_id: u64, channel_id: u64, message_id: u64) {
        self.with_state(guild_id, |s| {
            s.text_channel = Some(channel_id);
            s.panel_message = Some(message_id);
        })
        .await;
    }

    pub async fn clear_guild(&self, guild_id: u64) {
        self.states.lock().await.remove(&guild_id);
    }

    // ------------------------------------------------------------- voice

    /// Send gateway opcode 4 to join a voice channel.
    pub fn send_voice_join(
        &self,
        ctx: &SerenityContext,
        guild_id: u64,
        channel_id: Option<u64>,
    ) -> Result<()> {
        let payload = serde_json::json!({
            "op": 4,
            "d": {
                "guild_id": guild_id.to_string(),
                "channel_id": channel_id.map(|c| c.to_string()),
                "self_mute": false,
                "self_deaf": true,
            }
        });
        ctx.shard
            .websocket_message(tokio_tungstenite::tungstenite::Message::Text(
                payload.to_string(),
            ));
        Ok(())
    }

    /// Ensure a Lavalink player exists for the guild, joining voice if needed.
    pub async fn ensure_player(
        &self,
        ctx: &SerenityContext,
        guild_id: u64,
        voice_channel: u64,
    ) -> Result<PlayerContext> {
        let client = self
            .client()
            .await
            .ok_or_else(|| anyhow!("music is not connected to Lavalink"))?;

        if let Some(existing) = client.get_player_context(lava_gid(guild_id)) {
            self.with_state(guild_id, |s| s.voice_channel = Some(voice_channel))
                .await;
            return Ok(existing);
        }

        self.send_voice_join(ctx, guild_id, Some(voice_channel))?;

        let info = client
            .get_connection_info(lava_gid(guild_id), Duration::from_secs(20))
            .await
            .map_err(|err| anyhow!("voice connection failed: {err}"))?;

        let player = client
            .create_player_context(lava_gid(guild_id), info)
            .await
            .map_err(|err| anyhow!("creating Lavalink player: {err}"))?;

        let volume = self
            .with_state(guild_id, |s| {
                s.voice_channel = Some(voice_channel);
                if s.volume == 0 {
                    s.volume = 80;
                }
                s.volume
            })
            .await;

        let _ = player.set_volume(volume).await;
        Ok(player)
    }

    // ------------------------------------------------------------- search

    pub async fn load(&self, guild_id: u64, query: &str) -> Result<Vec<TrackData>> {
        let client = self
            .client()
            .await
            .ok_or_else(|| anyhow!("music is not connected to Lavalink"))?;

        let result = client
            .load_tracks(lava_gid(guild_id), query)
            .await
            .map_err(|err| anyhow!("search failed: {err}"))?;

        match result.data {
            Some(TrackLoadData::Track(track)) => Ok(vec![track]),
            Some(TrackLoadData::Playlist(playlist)) => Ok(playlist.tracks),
            Some(TrackLoadData::Search(tracks)) => Ok(tracks.into_iter().take(1).collect()),
            Some(TrackLoadData::Error(err)) => Err(anyhow!(err.message)),
            None => Ok(Vec::new()),
        }
    }

    /// Queue tracks and start playback if idle. Returns how many were queued.
    // The argument list mirrors what a play request actually carries (who,
    // what, where); bundling it into a struct buys nothing at this call site.
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue(
        &self,
        ctx: &SerenityContext,
        guild_id: u64,
        voice_channel: u64,
        text_channel: u64,
        tracks: Vec<TrackData>,
        requester: String,
        requester_id: u64,
    ) -> Result<usize> {
        if tracks.is_empty() {
            return Err(anyhow!("no tracks matched that query"));
        }

        self.set_text_channel(guild_id, text_channel).await;
        let player = self.ensure_player(ctx, guild_id, voice_channel).await?;

        let count = tracks.len();
        let mut to_start: Option<QueuedTrack> = None;

        {
            let mut guard = self.states.lock().await;
            let state = guard.entry(guild_id).or_default();
            for track in tracks {
                let queued = QueuedTrack::new(track, requester.clone(), requester_id);
                if state.current.is_none() && to_start.is_none() {
                    to_start = Some(queued);
                } else {
                    state.queue.push_back(queued);
                }
            }
            if let Some(current) = to_start.clone() {
                state.current = Some(current);
                state.position_ms = 0;
            }
        }

        if let Some(current) = to_start {
            player
                .play_now(&current.data)
                .await
                .map_err(|err| anyhow!("starting playback: {err}"))?;
            let _ = player
                .set_volume(self.with_state(guild_id, |s| s.volume).await)
                .await;
        }

        Ok(count)
    }

    /// Play the next queued track (or stop when the queue is empty).
    pub async fn advance(&self, guild_id: u64) -> Option<QueuedTrack> {
        let client = self.client().await?;
        let player = client.get_player_context(lava_gid(guild_id))?;

        let next = {
            let mut guard = self.states.lock().await;
            let state = guard.entry(guild_id).or_default();

            match state.loop_mode {
                LoopMode::Track => state.current.clone(),
                LoopMode::Queue => {
                    if let Some(current) = state.current.clone() {
                        state.queue.push_back(current);
                    }
                    state.queue.pop_front()
                }
                LoopMode::Off => state.queue.pop_front(),
            }
        };

        match next {
            Some(track) => {
                self.with_state(guild_id, |s| {
                    s.current = Some(track.clone());
                    s.position_ms = 0;
                    s.paused = false;
                })
                .await;
                if let Err(err) = player.play_now(&track.data).await {
                    tracing::warn!(error = %err, guild = guild_id, "advancing queue failed");
                }
                Some(track)
            }
            None => {
                self.with_state(guild_id, |s| {
                    s.current = None;
                    s.position_ms = 0;
                })
                .await;
                let _ = player.stop_now().await;
                None
            }
        }
    }

    pub async fn set_paused(&self, guild_id: u64, paused: bool) -> Result<()> {
        let client = self
            .client()
            .await
            .ok_or_else(|| anyhow!("music is not connected to Lavalink"))?;
        let player = client
            .get_player_context(lava_gid(guild_id))
            .ok_or_else(|| anyhow!("nothing is playing"))?;
        player
            .set_pause(paused)
            .await
            .map_err(|err| anyhow!("toggling pause: {err}"))?;
        self.with_state(guild_id, |s| s.paused = paused).await;
        Ok(())
    }

    pub async fn set_volume(&self, guild_id: u64, volume: u16) -> Result<()> {
        let client = self
            .client()
            .await
            .ok_or_else(|| anyhow!("music is not connected to Lavalink"))?;
        let player = client
            .get_player_context(lava_gid(guild_id))
            .ok_or_else(|| anyhow!("nothing is playing"))?;
        player
            .set_volume(volume)
            .await
            .map_err(|err| anyhow!("setting volume: {err}"))?;
        self.with_state(guild_id, |s| s.volume = volume).await;
        Ok(())
    }

    /// Stop playback, clear the queue and leave the voice channel.
    pub async fn stop(&self, ctx: &SerenityContext, guild_id: u64) -> Result<()> {
        if let Some(client) = self.client().await {
            if let Some(player) = client.get_player_context(lava_gid(guild_id)) {
                let _ = player.stop_now().await;
                let _ = player.set_pause(true).await;
            }
        }
        self.send_voice_join(ctx, guild_id, None)?;
        self.clear_guild(guild_id).await;
        Ok(())
    }

    pub async fn shuffle(&self, guild_id: u64) -> usize {
        use rand::seq::SliceRandom;
        self.with_state(guild_id, |s| {
            let mut items: Vec<QueuedTrack> = s.queue.drain(..).collect();
            items.shuffle(&mut rand::thread_rng());
            s.queue.extend(items);
            s.queue.len()
        })
        .await
    }

    /// Replace `state.position_ms` from Lavalink's periodic update.
    pub async fn update_position(&self, guild_id: u64, position_ms: u64) {
        self.with_state(guild_id, |s| s.position_ms = position_ms)
            .await;
    }

    /// Render the panel view for a guild.
    pub async fn view(&self, guild_id: u64) -> crate::ui::panels::NowPlayingView {
        let state = self.state(guild_id).await.unwrap_or_default();
        let mut view = crate::ui::panels::NowPlayingView {
            volume: if state.volume == 0 { 80 } else { state.volume },
            paused: state.paused,
            loop_label: state.loop_mode.label().to_string(),
            queue_len: state.queue.len(),
            ..Default::default()
        };

        if let Some(current) = state.current.as_ref() {
            view.title = current.title().to_string();
            view.author = current.data.info.author.clone();
            view.uri = current.data.info.uri.clone();
            view.length_ms = current.length();
            view.position_ms = state.position_ms.min(current.length());
            view.requester = if current.requester_id == 0 {
                current.requester.clone()
            } else {
                format!("<@{}>", current.requester_id)
            };
        }

        view.connected_channel = state.voice_channel.map(|id| format!("<#{id}>"));
        view.upcoming = state
            .queue
            .iter()
            .take(5)
            .map(|t| {
                (
                    t.title().to_string(),
                    crate::ui::theme::duration(t.length()),
                )
            })
            .collect();

        view
    }

    /// Every guild currently holding playback state.
    pub async fn guild_ids(&self) -> Vec<u64> {
        self.states.lock().await.keys().copied().collect()
    }

    /// Restart the current track from the beginning (the "previous" button).
    pub async fn restart_current(&self, guild_id: u64) -> bool {
        let Some(current) = self.state(guild_id).await.and_then(|state| state.current) else {
            return false;
        };
        let Some(client) = self.client().await else {
            return false;
        };
        let Some(player) = client.get_player_context(lava_gid(guild_id)) else {
            return false;
        };
        self.with_state(guild_id, |state| {
            state.position_ms = 0;
            state.paused = false;
        })
        .await;
        player.play_now(&current.data).await.is_ok()
    }

    /// Extend `position_ms` for panels that tick between Lavalink updates.
    pub async fn tick_position(&self, guild_id: u64, step_ms: u64) -> bool {
        let mut playing = false;
        self.with_state(guild_id, |state| {
            if let Some(current) = state.current.as_ref() {
                if !state.paused {
                    let length = current.length();
                    state.position_ms = (state.position_ms + step_ms).min(length);
                    playing = true;
                }
            }
        })
        .await;
        playing
    }

    /// Directly jump to a queue position (1-based).
    pub async fn jump(&self, guild_id: u64, position: usize) -> Result<QueuedTrack> {
        let client = self
            .client()
            .await
            .ok_or_else(|| anyhow!("music is not connected to Lavalink"))?;
        let player = client
            .get_player_context(lava_gid(guild_id))
            .ok_or_else(|| anyhow!("nothing is playing"))?;

        let track = {
            let mut guard = self.states.lock().await;
            let state = guard.entry(guild_id).or_default();
            if position == 0 || position > state.queue.len() {
                return Err(anyhow!("no track at position {position}"));
            }
            if let Some(current) = state.current.take() {
                state.queue.push_front(current);
            }
            let mut idx = 0usize;
            let mut picked = None;
            while let Some(item) = state.queue.pop_front() {
                if idx == position - 1 {
                    picked = Some(item);
                    break;
                }
                state.queue.push_back(item);
                idx += 1;
            }
            let picked = picked.ok_or_else(|| anyhow!("no track at position {position}"))?;
            state.current = Some(picked.clone());
            state.position_ms = 0;
            picked
        };

        player
            .play_now(&track.data)
            .await
            .map_err(|err| anyhow!("jumping: {err}"))?;
        Ok(track)
    }
}

// ------------------------------------------------------------------ events

fn boxed() -> lavalink_rs::model::BoxFuture<'static, ()> {
    Box::pin(async {})
}

fn events() -> lava_events::Events {
    lava_events::Events {
        ready: Some(|_client, _session, _data| {
            publish(MusicEvent::Ready);
            boxed()
        }),
        track_start: Some(|_client, _session, data: &lava_events::TrackStart| {
            publish(MusicEvent::TrackStart {
                guild_id: data.guild_id.0,
                title: data.track.info.title.clone(),
            });
            boxed()
        }),
        track_end: Some(|_client, _session, data: &lava_events::TrackEnd| {
            let should_continue: bool = data.reason.clone().into();
            let reason = match data.reason {
                lava_events::TrackEndReason::Finished => "finished",
                lava_events::TrackEndReason::LoadFailed => "load failed",
                lava_events::TrackEndReason::Stopped => "stopped",
                lava_events::TrackEndReason::Replaced => "replaced",
                lava_events::TrackEndReason::Cleanup => "cleanup",
            };
            publish(MusicEvent::TrackEnd {
                guild_id: data.guild_id.0,
                reason,
                should_continue,
            });
            boxed()
        }),
        track_exception: Some(|_client, _session, data: &lava_events::TrackException| {
            publish(MusicEvent::TrackException {
                guild_id: data.guild_id.0,
                message: data.exception.message.clone(),
            });
            boxed()
        }),
        websocket_closed: Some(|_client, _session, data: &lava_events::WebSocketClosed| {
            publish(MusicEvent::WebSocketClosed {
                guild_id: data.guild_id.0,
                code: data.code,
            });
            boxed()
        }),
        ..Default::default()
    }
}

// ------------------------------------------------------------- panel updates

/// Refresh the control panel message for a guild. Errors are logged, never
/// propagated: a deleted panel must not break playback.
pub async fn refresh_panel(ctx: &SerenityContext, guild_id: u64, manager: &MusicManager) {
    let state = manager.state(guild_id).await.unwrap_or_default();
    let Some(channel_id) = state.text_channel else {
        return;
    };
    let Some(message_id) = state.panel_message else {
        return;
    };

    let view = manager.view(guild_id).await;
    let (embed, rows) = crate::ui::panels::music(&view);

    let edit = serenity::EditMessage::new().embed(embed).components(rows);
    if let Err(err) = ChannelId::new(channel_id)
        .edit_message(&ctx.http, MessageId::new(message_id), edit)
        .await
    {
        tracing::debug!(error = %err, guild = guild_id, "music panel refresh failed");
    }
}

/// The single background loop that turns Lavalink events into queue movement.
pub async fn event_loop(
    ctx: SerenityContext,
    manager: Arc<MusicManager>,
    mut rx: mpsc::UnboundedReceiver<MusicEvent>,
) {
    while let Some(event) = rx.recv().await {
        match event {
            MusicEvent::Ready => {
                tracing::info!("Lavalink session ready");
            }
            MusicEvent::TrackStart { guild_id, title } => {
                tracing::info!(guild = guild_id, title = %title, "track started");
                manager.with_state(guild_id, |s| s.position_ms = 0).await;
                refresh_panel(&ctx, guild_id, &manager).await;
            }
            MusicEvent::TrackEnd {
                guild_id,
                reason,
                should_continue,
            } => {
                tracing::debug!(guild = guild_id, reason, "track ended");
                if !should_continue {
                    continue;
                }
                // Small delay: Lavalink is still tearing down the old stream.
                tokio::time::sleep(Duration::from_millis(250)).await;
                let next = manager.advance(guild_id).await;
                refresh_panel(&ctx, guild_id, &manager).await;
                if next.is_none() {
                    tracing::debug!(guild = guild_id, "queue drained");
                }
            }
            MusicEvent::TrackException { guild_id, message } => {
                tracing::warn!(guild = guild_id, %message, "track exception");
            }
            MusicEvent::WebSocketClosed { guild_id, code } => {
                tracing::warn!(guild = guild_id, code, "lavalink voice socket closed");
                if code == 4014 || code == 4006 {
                    // Kicked / session invalid: stop cleanly.
                    manager.clear_guild(guild_id).await;
                }
            }
        }
    }
}

/// Convenience wrapper so command code never has to build `GuildId`s.
pub fn serenity_guild(id: u64) -> SGuildId {
    SGuildId::new(id)
}
