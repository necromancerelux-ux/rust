//! Server Index: auto-updating public directory of channels and forum posts.
//!
//! Mirrors src/serverIndex/{builder,publisher,events}.js. The index lives in
//! a configured channel; it is rebuilt automatically whenever channels or
//! forum threads change (debounced), or manually via the settings panel.

use std::collections::HashMap;

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, ChannelType, CreateEmbed};

use crate::state::Data;
use crate::ui::theme::colors;

const FIELD_VALUE_MAX: usize = 900;

fn index_embed(server_name: &str, continuation: bool) -> CreateEmbed {
    let embed = CreateEmbed::new()
        .title(format!("📑 {} — Channel Index", server_name))
        .color(colors::BRAND)
        .description(if continuation {
            "Continued server directory."
        } else {
            "Auto-generated map of this server. Jump straight to any channel below."
        })
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));
    embed
}

fn push_index_section(
    embeds: &mut Vec<CreateEmbed>,
    current: &mut CreateEmbed,
    fields: &mut usize,
    server_name: &str,
    name: String,
    lines: Vec<String>,
) {
    let mut pages: Vec<Vec<String>> = Vec::new();
    let mut page = Vec::new();
    let mut page_len = 0usize;
    for line in lines {
        let line = if line.len() > FIELD_VALUE_MAX {
            line.chars().take(FIELD_VALUE_MAX - 1).collect::<String>()
        } else {
            line
        };
        if !page.is_empty() && page_len + line.len() + 1 > FIELD_VALUE_MAX {
            pages.push(std::mem::take(&mut page));
            page_len = 0;
        }
        page_len += line.len() + usize::from(!page.is_empty());
        page.push(line);
    }
    if !page.is_empty() {
        pages.push(page);
    }
    for (index, page) in pages.into_iter().enumerate() {
        if *fields >= 20 {
            embeds.push(std::mem::replace(current, index_embed(server_name, true)));
            *fields = 0;
        }
        let field_name = if index == 0 {
            name.clone()
        } else {
            format!("{name} · continued")
        };
        let value = page.join("\n");
        *current = std::mem::replace(current, CreateEmbed::new()).field(field_name, value, false);
        *fields += 1;
    }
}

fn section_emoji(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    let has = |pats: &[&str]| pats.iter().any(|p| n.contains(p));
    if has(&["get-start", "getstart", "welcome", "intro", "start"]) {
        "👋"
    } else if has(&["important", "announce", "news", "notice", "alert"]) {
        "📢"
    } else if has(&[
        "guide", "help", "info", "resource", "wiki", "calc", "hero", "event",
    ]) {
        "📖"
    } else if has(&["nap", "leader", "officer", "staff", "admin"]) {
        "🛡️"
    } else if has(&["voice", "vc", "talk", "speak", "stage"]) {
        "🎙️"
    } else if has(&["chat", "general", "social", "lounge"]) {
        "💬"
    } else if has(&["gift", "code", "giveaway", "redeem"]) {
        "🎁"
    } else if has(&["rule", "policy"]) {
        "📜"
    } else if has(&["vote", "poll"]) {
        "🗳️"
    } else {
        "📁"
    }
}

fn channel_icon(kind: ChannelType, name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    match kind {
        ChannelType::Forum | ChannelType::Unknown(16) => "🗂️",
        ChannelType::Stage => "🎙️",
        ChannelType::Voice => "🔊",
        ChannelType::News => "📢",
        ChannelType::Text => {
            if n.contains("rule") {
                "📜"
            } else if n.contains("verify") {
                "✅"
            } else if n.contains("intro") || n.contains("meet") {
                "👋"
            } else if n.contains("announce") || n.contains("news") {
                "📢"
            } else if n.contains("gift") || n.contains("code") || n.contains("giveaway") {
                "🎁"
            } else if n.contains("event") || n.contains("reminder") {
                "🕒"
            } else if n.contains("vote") || n.contains("poll") {
                "🗳️"
            } else if n.contains("help") {
                "❓"
            } else if n.contains("calc") {
                "🧮"
            } else if n.contains("nap") {
                "🐾"
            } else if n.contains("role") {
                "🏷️"
            } else {
                "💬"
            }
        }
        _ => "💬",
    }
}

fn topic_snippet(topic: Option<&str>) -> String {
    let Some(topic) = topic else {
        return String::new();
    };
    let first = topic
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let collapsed: String = first.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() > 90 {
        format!("{}…", collapsed.chars().take(89).collect::<String>())
    } else {
        collapsed
    }
}

struct IndexChannel {
    id: u64,
    name: String,
    kind: ChannelType,
    topic: Option<String>,
    position: u16,
    parent_id: Option<u64>,
}

async fn fetch_guild_channels(
    ctx: &serenity::Context,
    guild_id: u64,
) -> Vec<serenity::GuildChannel> {
    if let Ok(channels) = ctx
        .http
        .get_channels(serenity::GuildId::new(guild_id))
        .await
    {
        return channels;
    }
    ctx.cache
        .guild(guild_id)
        .map(|guild| guild.channels.values().cloned().collect())
        .unwrap_or_default()
}

fn collect_channels(channels: &[serenity::GuildChannel], include_voice: bool) -> Vec<IndexChannel> {
    channels
        .iter()
        .filter(|ch| match ch.kind {
            ChannelType::Text
            | ChannelType::News
            | ChannelType::Forum
            | ChannelType::Unknown(16) => true,
            ChannelType::Voice | ChannelType::Stage => include_voice,
            _ => false,
        })
        .map(|ch| IndexChannel {
            id: ch.id.get(),
            name: ch.name.clone(),
            kind: ch.kind,
            topic: ch.topic.clone(),
            position: ch.position,
            parent_id: ch.parent_id.map(|id| id.get()),
        })
        .collect()
}

fn collect_categories(channels: &[serenity::GuildChannel]) -> Vec<(u64, String, u16)> {
    let mut cats: Vec<(u64, String, u16)> = channels
        .iter()
        .filter(|ch| ch.kind == ChannelType::Category)
        .map(|ch| (ch.id.get(), ch.name.clone(), ch.position))
        .collect();
    cats.sort_by_key(|(_, _, position)| *position);
    cats
}

/// Build the index embeds (title + one field per category).
pub async fn build_embeds(
    ctx: &serenity::Context,
    guild_id: u64,
    include_voice: bool,
    include_threads: bool,
    server_name: &str,
) -> Vec<CreateEmbed> {
    let guild_channels = fetch_guild_channels(ctx, guild_id).await;
    let channels = collect_channels(&guild_channels, include_voice);
    let categories = collect_categories(&guild_channels);
    let mut active_threads: HashMap<u64, Vec<(u64, String)>> = HashMap::new();
    if include_threads {
        let forum_ids = channels
            .iter()
            .filter(|channel| {
                channel.kind == ChannelType::Forum || channel.kind == ChannelType::Unknown(16)
            })
            .map(|channel| channel.id)
            .collect::<std::collections::HashSet<_>>();
        if !forum_ids.is_empty() {
            if let Ok(data) = ctx
                .http
                .get_guild_active_threads(serenity::GuildId::new(guild_id))
                .await
            {
                for thread in data.threads.into_iter().filter(|thread| {
                    forum_ids.contains(&thread.parent_id.map(|id| id.get()).unwrap_or_default())
                        && matches!(
                            thread.kind,
                            ChannelType::PublicThread
                                | ChannelType::NewsThread
                                | ChannelType::PrivateThread
                        )
                }) {
                    if let Some(parent) = thread.parent_id.map(|id| id.get()) {
                        active_threads
                            .entry(parent)
                            .or_default()
                            .push((thread.id.get(), thread.name));
                    }
                }
            }
        }
    }
    for list in active_threads.values_mut() {
        list.sort_by(|left, right| left.1.cmp(&right.1));
    }

    let mut by_cat: HashMap<u64, Vec<&IndexChannel>> = HashMap::new();
    let mut root: Vec<&IndexChannel> = Vec::new();
    for ch in &channels {
        match ch.parent_id {
            Some(parent) => by_cat.entry(parent).or_default().push(ch),
            None => root.push(ch),
        }
    }
    for list in by_cat.values_mut() {
        list.sort_by(|a, b| {
            a.position
                .cmp(&b.position)
                .then_with(|| a.name.cmp(&b.name))
        });
    }
    root.sort_by(|a, b| {
        a.position
            .cmp(&b.position)
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut embeds = Vec::new();
    let mut current = index_embed(server_name, false);
    let mut fields = 0usize;

    // Categories in order, then orphan channels.
    for (cat_id, cat_name, _) in &categories {
        let Some(list) = by_cat.remove(cat_id) else {
            continue;
        };
        if list.is_empty() {
            continue;
        }
        let lines: Vec<String> =
            list.iter()
                .flat_map(|ch| {
                    let mut lines = vec![channel_line(ch)];
                    if let Some(threads) = active_threads.get(&ch.id) {
                        lines.extend(threads.iter().map(|(id, name)| {
                            format!("　　🧵 {}: <#{id}>", escape_index_text(name))
                        }));
                    }
                    lines
                })
                .collect();
        let name = format!("{} {}", section_emoji(cat_name), cat_name);
        push_index_section(
            &mut embeds,
            &mut current,
            &mut fields,
            server_name,
            name,
            lines,
        );
    }
    if !root.is_empty() {
        let lines: Vec<String> =
            root.iter()
                .flat_map(|ch| {
                    let mut lines = vec![channel_line(ch)];
                    if let Some(threads) = active_threads.get(&ch.id) {
                        lines.extend(threads.iter().map(|(id, name)| {
                            format!("　　🧵 {}: <#{id}>", escape_index_text(name))
                        }));
                    }
                    lines
                })
                .collect();
        push_index_section(
            &mut embeds,
            &mut current,
            &mut fields,
            server_name,
            "📁 Other".to_string(),
            lines,
        );
    }

    embeds.push(current);
    embeds
}

fn escape_index_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if matches!(ch, '*' | '_' | '`' | '~' | '|') {
                format!("\\{ch}")
            } else {
                ch.to_string()
            }
        })
        .collect()
}

fn channel_line(ch: &IndexChannel) -> String {
    let icon = channel_icon(ch.kind, &ch.name);
    let snippet = topic_snippet(ch.topic.as_deref());
    if snippet.is_empty() {
        format!("{icon} <#{}>", ch.id)
    } else {
        format!("{icon} <#{}> — {}", ch.id, snippet)
    }
}

/// Publish (or refresh) the index in the configured channel.
/// `force` bypasses the "auto-index disabled" gate for manual Rebuild.
pub async fn publish(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    force: bool,
) -> Result<Vec<u64>, String> {
    let settings = crate::db::server_index::get(&data.db, guild_id)
        .await
        .map_err(|e| e.to_string())?;
    let Some(channel_id) = settings.channel() else {
        return Err("no channel configured".to_string());
    };
    if !force && !settings.enabled() {
        return Err("auto-index disabled".to_string());
    }

    let server_name = ctx
        .cache
        .guild(guild_id)
        .map(|guild| guild.name.clone())
        .unwrap_or_else(|| "Server".to_string());
    let embeds = build_embeds(
        ctx,
        guild_id,
        settings.include_voice(),
        settings.include_threads(),
        &server_name,
    )
    .await;
    let old_ids = settings.message_ids();
    let target = ChannelId::new(channel_id);

    let mut new_ids: Vec<u64> = Vec::with_capacity(embeds.len());
    for (index, embed) in embeds.into_iter().enumerate() {
        let message_id = old_ids.get(index).copied();
        // Snapshot the actual embed (including its channel-list fields) so the
        // private translated copy shows the same information as the public one.
        let stored_json = crate::translate::stored_json(&embed);

        match message_id {
            Some(id) => {
                let id = serenity::MessageId::new(id);
                match target
                    .edit_message(
                        &ctx.http,
                        id,
                        serenity::EditMessage::new()
                            .embed(embed.clone())
                            .components(vec![]),
                    )
                    .await
                {
                    Ok(_) => {
                        let _ = crate::translate::register(
                            &data.db,
                            id.get(),
                            crate::db::translate::NewTranslatable {
                                guild_id: Some(guild_id),
                                channel_id: Some(channel_id),
                                kind: "server_index",
                                embed_json: stored_json.clone(),
                                content: None,
                                owner_id: None,
                            },
                        )
                        .await;
                        new_ids.push(id.get());
                    }
                    Err(_) => {
                        let sent = target
                            .send_message(&ctx.http, serenity::CreateMessage::new().embed(embed))
                            .await
                            .map_err(|e| e.to_string())?;
                        new_ids.push(sent.id.get());
                    }
                }
            }
            None => {
                let sent = target
                    .send_message(&ctx.http, serenity::CreateMessage::new().embed(embed))
                    .await
                    .map_err(|e| e.to_string())?;
                if index == 0 {
                    let _ = crate::translate::register(
                        &data.db,
                        sent.id.get(),
                        crate::db::translate::NewTranslatable {
                            guild_id: Some(guild_id),
                            channel_id: Some(channel_id),
                            kind: "server_index",
                            embed_json: stored_json.clone(),
                            content: None,
                            owner_id: None,
                        },
                    )
                    .await;
                }
                new_ids.push(sent.id.get());
            }
        }

        // Only the first message carries a Translate button.
        if index == 0 {
            let _ = target
                .edit_message(
                    &ctx.http,
                    serenity::MessageId::new(*new_ids.last().unwrap()),
                    serenity::EditMessage::new().components(vec![crate::translate::translate_row(
                        *new_ids.last().unwrap(),
                    )]),
                )
                .await;
        }
    }

    // Delete leftover messages from previous versions.
    for old in old_ids.iter().skip(new_ids.len()) {
        let _ = target
            .delete_message(&ctx.http, serenity::MessageId::new(*old))
            .await;
    }

    crate::db::server_index::set_message_ids(&data.db, guild_id, &new_ids)
        .await
        .map_err(|e| e.to_string())?;
    Ok(new_ids)
}

/// Debounced rebuild — called from channel/thread events.
pub async fn schedule_refresh(data: Data, ctx: serenity::Context, guild_id: u64) {
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    };
    use tokio::task::JoinHandle;
    static PENDING: OnceLock<Mutex<HashMap<u64, (u64, JoinHandle<()>)>>> = OnceLock::new();
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    let pending = PENDING.get_or_init(|| Mutex::new(HashMap::new()));
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    let task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        let current = {
            let mut guard = pending.lock().unwrap_or_else(|e| e.into_inner());
            if guard
                .get(&guild_id)
                .is_some_and(|(current, _)| *current == generation)
            {
                guard.remove(&guild_id);
                true
            } else {
                false
            }
        };
        if !current {
            return;
        }
        let enabled = crate::db::server_index::get(&data.db, guild_id)
            .await
            .map(|s| s.enabled() && s.channel().is_some())
            .unwrap_or(false);
        if !enabled {
            return;
        }
        if let Err(err) = publish(&ctx, &data, guild_id, false).await {
            tracing::warn!(error = %err, guild = guild_id, "server index refresh failed");
        }
    });
    let mut guard = pending.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, previous)) = guard.remove(&guild_id) {
        previous.abort();
    }
    guard.insert(guild_id, (generation, task));
}
