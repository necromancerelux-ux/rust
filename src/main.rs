//! OmniCore — Discord bot entry point.
//!
//! Startup order mirrors the legacy Node runtime: load config, open the shared
//! SQLite database, build the services, then hand control to poise. Everything
//! after the gateway READY (Lavalink, background jobs, command registration) is
//! fail-soft so a bad third-party service can never take the bot offline.
//!
//! NOTE: This crate is being migrated in stages, and some helpers are already
//! written for features that are not wired up yet (see `docs/MIGRATION.md`).
//! Dead code is allowed while the migration is in flight; remove this allow once
//! the remaining legacy features are ported.
#![allow(dead_code)]

mod admintoolkit;
mod backup;
mod ballots;
mod calculator;
mod cards;
mod commands;
mod config;
mod db;
mod emoji;
mod error;
mod giftcode;
mod leveling;
mod logging;
mod lookup;
mod ministry;
mod music;
mod net;
mod notifications;
mod oracle;
mod owner;
mod permissions;
mod playerid;
mod render;
mod roles;
mod secrets;
mod serverindex;
mod sneakpeek;
mod state;
mod tasks;
mod tempvoice;
mod tickets;
mod translate;
mod ui;
mod welcome;
mod wos;

use std::collections::HashSet;
use std::time::Instant;

use poise::serenity_prelude as serenity;
use serenity::{FullEvent, GatewayIntents, UserId};

use crate::config::Config;
use crate::db::Db;
use crate::lookup::LookupClient;
use crate::music::MusicManager;
use crate::oracle::{OracleClient, OracleConfig};
use crate::state::Data;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GUILD_MEMBERS and MESSAGE_CONTENT are privileged: enable both in the Discord
/// developer portal (Bot → Privileged Gateway Intents). MESSAGE_CONTENT is needed
/// by the WoS relay, which reads hub message text and embeds.
fn intents() -> GatewayIntents {
    GatewayIntents::GUILDS
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--snapshot-database") {
        let destination = args
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("usage: omnicore --snapshot-database <destination>"))?;
        let source = std::env::var_os("DATABASE_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("db/bot.sqlite"));
        backup::snapshot_database(&source, std::path::Path::new(destination)).await?;
        return Ok(());
    }

    let cfg = Config::load()?;
    let token = cfg.token.clone();

    let db = Db::connect(&cfg.database_path, cfg.db_pool_size).await?;

    let lookup = LookupClient::new(
        cfg.wos_site.clone(),
        cfg.wos_api.clone(),
        cfg.wos_api_key.clone(),
    )?;
    // A key stored in the DB (owner-managed) wins over the env fallback.
    if let Some(key) = db::wos_credentials::api_key(&db).await {
        tracing::info!("Whiteout Stats API key loaded from the database");
        lookup.set_api_key(Some(key));
    }

    let oracle = OracleClient::new(OracleConfig {
        site: cfg.oracle_site.clone(),
        codes_url: cfg.oracle_codes_url.clone(),
    })?;

    if let Ok(creds) = db::oracle::key_get(&db).await {
        if let Some(key) = creds.api_key {
            tracing::info!("WoS Oracle API key loaded from the database");
            oracle.set_api_key(Some(key));
        }
    }

    let music = MusicManager::new(cfg.lavalink.clone());

    // Discord REST clients built with the TLS/proxy options from `net.rs`
    // (fixes `UnknownIssuer` on hosts with a custom CA). reqwest clients are cheap
    // to clone and share one connection pool.
    let rest_client = crate::net::client();
    let make_http = || {
        serenity::http::HttpBuilder::new(token.clone())
            .client(rest_client.clone())
            .build()
    };
    let discord_http = std::sync::Arc::new(make_http());

    let data = Data {
        cfg: cfg.clone(),
        db,
        discord_http: discord_http.clone(),
        music,
        lookup,
        oracle,
        gift_http: crate::net::builder()
            .user_agent("Mozilla/5.0 (compatible; OmniCore/2.0; GiftCode)")
            .build()
            .expect("gift-code http client"),
        started: Instant::now(),
    };

    let mut owners = HashSet::new();
    if let Some(owner) = cfg.owner_id {
        owners.insert(UserId::new(owner));
    }

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: commands::all(),
            owners,
            event_handler: |framework, event| Box::pin(event_handler(framework, event)),
            on_error: |error| Box::pin(on_error(error)),
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            let data = data.clone();
            let guild_scope = data.cfg.guild_id;
            Box::pin(async move {
                tracing::info!(
                    user = %ready.user.name,
                    id = ready.user.id.get(),
                    guilds = ready.guilds.len(),
                    "gateway ready"
                );

                if data.cfg.music_enabled() {
                    match data.music.connect(ready.user.id.get()).await {
                        Ok(()) => {
                            if let Some(rx) = data.music.take_events().await {
                                tokio::spawn(crate::music::event_loop(
                                    ctx.clone(),
                                    data.music.clone(),
                                    rx,
                                ));
                            }
                            tracing::info!("Lavalink connected");
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "Lavalink unavailable — music disabled")
                        }
                    }
                }

                restore_music_panels(&data).await;
                tasks::spawn(ctx, data.clone());

                // Startup self-healing for configured Admin Toolkits. Never
                // blocks startup and never aborts the bot on failure.
                {
                    let ctx = ctx.clone();
                    let data = data.clone();
                    tokio::spawn(async move {
                        crate::admintoolkit::forum::startup_repair(&ctx, &data).await;
                    });
                }

                // Cross-server material emoji: upload once into the home guild.
                let guild_ids: Vec<_> = ready.guilds.iter().map(|g| g.id).collect();
                if let Err(err) = crate::emoji::ensure_emojis_ready(
                    &ctx.http,
                    &data.db,
                    None,
                    guild_ids,
                )
                .await
                {
                    tracing::warn!(error = %err, "[emoji] startup check failed");
                }

                // A failed registration (rate limit, transient API error) must not
                // keep the bot offline; already-registered commands keep working.
                let registered = match guild_scope {
                    Some(guild) => poise::builtins::register_in_guild(
                        ctx,
                        &framework.options().commands,
                        serenity::GuildId::new(guild),
                    )
                    .await
                    .map(|_| tracing::info!(guild, "registered commands in guild")),
                    None => poise::builtins::register_globally(ctx, &framework.options().commands)
                        .await
                        .map(|_| tracing::info!("registered commands globally")),
                };
                if let Err(err) = registered {
                    tracing::error!(error = %err, "command registration failed — continuing without it");
                }

                Ok(data)
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new_with_http(make_http(), intents())
        .framework(framework)
        .await?;

    if let Err(err) = client.start().await {
        tracing::error!(error = %err, "gateway terminated");
        return Err(err.into());
    }

    Ok(())
}

/// Re-arm the in-memory music state for every guild that had a panel before the
/// last restart, so the first edit finds the right message.
async fn restore_music_panels(data: &Data) {
    let rows = match db::music::panels(&data.db).await {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(error = %err, "reading saved music panels failed");
            return;
        }
    };

    let mut restored = 0usize;
    for row in rows {
        let Ok(guild_id) = row.guild_id.parse::<u64>() else {
            continue;
        };
        let (Some(channel), Some(message)) = (row.channel_id(), row.panel_message_id()) else {
            continue;
        };
        data.music.set_panel(guild_id, channel, message).await;
        data.music.set_volume_state(guild_id, row.volume()).await;
        data.music.set_loop_state(guild_id, row.loop_mode()).await;
        restored += 1;
    }

    if restored > 0 {
        tracing::info!(panels = restored, "music panels restored");
    }
}

async fn event_handler(
    framework: poise::FrameworkContext<'_, Data, commands::Error>,
    event: &FullEvent,
) -> Result<(), commands::Error> {
    let data = framework.user_data;

    match event {
        FullEvent::VoiceStateUpdate { old, new } => {
            let guild_id = new.guild_id.map(|id| id.get());

            // Music bookkeeping runs on our own voice state only.
            if let Some(bot_id) = data.music.bot_id().await {
                if new.user_id.get() == bot_id {
                    if let Some(guild_id) = guild_id {
                        if let Some(client) = data.music.client().await {
                            client.handle_voice_state_update(
                                lavalink_rs::model::GuildId(guild_id),
                                new.channel_id
                                    .map(|id| lavalink_rs::model::ChannelId(id.get())),
                                lavalink_rs::model::UserId(bot_id),
                                new.session_id.clone(),
                            );
                        }
                    }
                }
            }

            // Temp voice runs on every member's state.
            if let Some(guild_id) = guild_id {
                crate::tempvoice::on_voice_state_update(
                    framework.serenity_context,
                    data,
                    guild_id,
                    new.user_id.get(),
                    old.as_ref()
                        .and_then(|state| state.channel_id)
                        .map(|id| id.get()),
                    new.channel_id.map(|id| id.get()),
                )
                .await;

                // Voice log: channel transitions.
                let was = old.as_ref().and_then(|state| state.channel_id);
                let is = new.channel_id;
                if was != is {
                    let (title, color) = match (was, is) {
                        (Some(_), None) => ("Voice disconnected", crate::ui::theme::colors::DANGER),
                        (None, Some(_)) => ("Voice connected", crate::ui::theme::colors::SUCCESS),
                        _ => ("Voice moved", crate::ui::theme::colors::INFO),
                    };
                    let from = was
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "—".to_string());
                    let to = is
                        .map(|id| format!("<#{id}>"))
                        .unwrap_or_else(|| "—".to_string());
                    let embed = crate::logging::entry(
                        title,
                        &format!("<@{}>: {from} → {to}", new.user_id.get()),
                        color,
                    );
                    crate::logging::log(framework.serenity_context, data, guild_id, "voice", embed)
                        .await;
                }
            }
        }
        FullEvent::VoiceServerUpdate { event } => {
            let Some(guild_id) = event.guild_id else {
                return Ok(());
            };
            if let Some(client) = data.music.client().await {
                client.handle_voice_server_update(
                    lavalink_rs::model::GuildId(guild_id.get()),
                    event.token.clone(),
                    event.endpoint.clone(),
                );
            }
        }
        FullEvent::Message { new_message } => {
            if let Err(err) =
                crate::wos::relay::on_message(framework.serenity_context, data, new_message).await
            {
                tracing::warn!(error = %crate::error::display_anyhow(&err), "wos relay failed");
            }
            crate::leveling::handle_message(framework.serenity_context, data, new_message).await;
        }
        FullEvent::ChannelCreate { channel } => {
            crate::serverindex::schedule_refresh(
                data.clone(),
                framework.serenity_context.clone(),
                channel.guild_id.get(),
            )
            .await;
            let embed = crate::logging::entry(
                "Channel created",
                &format!("<#{}> (`{}`)", channel.id.get(), channel.name),
                crate::ui::theme::colors::SUCCESS,
            );
            crate::logging::log(
                framework.serenity_context,
                data,
                channel.guild_id.get(),
                "channels",
                embed,
            )
            .await;
        }
        FullEvent::ChannelDelete { channel, .. } => {
            // JS `tickets/events.js`: a deleted ticket channel closes its row.
            crate::tickets::on_channel_delete(&data.db, channel.id.get()).await;
            // Self-heal the Toolkit forum if it was deleted.
            if crate::db::admin_toolkit::ToolkitConfig::for_guild(&data.db, channel.guild_id.get())
                .await
                .ok()
                .flatten()
                .and_then(|config| config.forum_channel_id)
                == Some(channel.id.get())
            {
                crate::admintoolkit::forum::schedule_repair(
                    framework.serenity_context.clone(),
                    data.clone(),
                    channel.guild_id.get(),
                );
            }
            crate::serverindex::schedule_refresh(
                data.clone(),
                framework.serenity_context.clone(),
                channel.guild_id.get(),
            )
            .await;
            let embed = crate::logging::entry(
                "Channel deleted",
                &format!("`#{}`", channel.name),
                crate::ui::theme::colors::DANGER,
            );
            crate::logging::log(
                framework.serenity_context,
                data,
                channel.guild_id.get(),
                "channels",
                embed,
            )
            .await;
        }
        FullEvent::ChannelUpdate { old, new } => {
            let touch = match old {
                Some(old) => {
                    old.name != new.name
                        || old.parent_id != new.parent_id
                        || old.position != new.position
                }
                None => true,
            };
            if touch {
                crate::serverindex::schedule_refresh(
                    data.clone(),
                    framework.serenity_context.clone(),
                    new.guild_id.get(),
                )
                .await;
            }
        }
        FullEvent::CategoryCreate { category } => {
            crate::serverindex::schedule_refresh(
                data.clone(),
                framework.serenity_context.clone(),
                category.guild_id.get(),
            )
            .await;
        }
        FullEvent::CategoryDelete { category, .. } => {
            crate::serverindex::schedule_refresh(
                data.clone(),
                framework.serenity_context.clone(),
                category.guild_id.get(),
            )
            .await;
        }
        FullEvent::ThreadCreate { thread } => {
            if parent_is_forum(
                framework.serenity_context,
                thread.guild_id,
                thread.parent_id,
            ) {
                crate::serverindex::schedule_refresh(
                    data.clone(),
                    framework.serenity_context.clone(),
                    thread.guild_id.get(),
                )
                .await;
            }
        }
        FullEvent::ThreadUpdate { old, new } => {
            let changed = match old.as_ref() {
                Some(old) => {
                    old.name != new.name
                        || old.thread_metadata.as_ref().map(|m| m.archived)
                            != new.thread_metadata.as_ref().map(|m| m.archived)
                }
                None => true,
            };
            if changed && parent_is_forum(framework.serenity_context, new.guild_id, new.parent_id) {
                crate::serverindex::schedule_refresh(
                    data.clone(),
                    framework.serenity_context.clone(),
                    new.guild_id.get(),
                )
                .await;
            }
        }
        FullEvent::ThreadDelete { thread, .. } => {
            // Self-heal a Toolkit post that was deleted.
            if let Ok(Some((guild_id, _))) =
                crate::db::admin_toolkit::find_post(&data.db, thread.id.get()).await
            {
                crate::admintoolkit::forum::schedule_repair(
                    framework.serenity_context.clone(),
                    data.clone(),
                    guild_id,
                );
            }
            if parent_is_forum(
                framework.serenity_context,
                thread.guild_id,
                Some(thread.parent_id),
            ) {
                crate::serverindex::schedule_refresh(
                    data.clone(),
                    framework.serenity_context.clone(),
                    thread.guild_id.get(),
                )
                .await;
            }
        }
        FullEvent::GuildMemberAddition { new_member } => {
            crate::welcome::on_member_join(framework.serenity_context, data, new_member).await;
            let guild_id = new_member.guild_id.get();
            let embed = crate::logging::entry(
                "Member joined",
                &format!("<@{}> joined the server.", new_member.user.id.get()),
                crate::ui::theme::colors::SUCCESS,
            )
            .author(serenity::CreateEmbedAuthor::new(
                new_member.display_name().to_string(),
            ))
            .thumbnail(new_member.user.face());
            crate::logging::log(framework.serenity_context, data, guild_id, "members", embed).await;
        }
        FullEvent::GuildMemberUpdate {
            old_if_available,
            new,
            ..
        } => {
            if let (Some(old_member), Some(new_member)) = (old_if_available.as_ref(), new.as_ref())
            {
                if old_member.pending && !new_member.pending {
                    crate::welcome::apply_auto_roles(framework.serenity_context, data, new_member)
                        .await;
                }
            }
        }
        FullEvent::GuildMemberRemoval { guild_id, user, .. } => {
            let _ = db::giveaways::remove_entries_for_user(&data.db, guild_id.get(), user.id.get())
                .await;
            crate::welcome::on_member_remove(data, guild_id.get(), user.id.get()).await;
            let embed = crate::logging::entry(
                "Member left",
                &format!("<@{}> ({}) left the server.", user.id.get(), user.name),
                crate::ui::theme::colors::DANGER,
            )
            .author(serenity::CreateEmbedAuthor::new(user.name.clone()))
            .thumbnail(user.face());
            crate::logging::log(
                framework.serenity_context,
                data,
                guild_id.get(),
                "members",
                embed,
            )
            .await;
        }
        FullEvent::GuildRoleCreate { new } => {
            let embed = crate::logging::entry(
                "Role created",
                &format!("<@&{}> (`{}`)", new.id.get(), new.name),
                crate::ui::theme::colors::SUCCESS,
            );
            crate::logging::log(
                framework.serenity_context,
                data,
                new.guild_id.get(),
                "roles",
                embed,
            )
            .await;
        }
        FullEvent::GuildRoleDelete {
            guild_id,
            removed_role_id,
            removed_role_data_if_available,
        } => {
            let name = removed_role_data_if_available
                .as_ref()
                .map(|role| role.name.clone())
                .unwrap_or_else(|| format!("id {}", removed_role_id.get()));
            let embed = crate::logging::entry(
                "Role deleted",
                &format!("`{name}`"),
                crate::ui::theme::colors::DANGER,
            );
            crate::logging::log(
                framework.serenity_context,
                data,
                guild_id.get(),
                "roles",
                embed,
            )
            .await;
        }
        FullEvent::GuildRoleUpdate {
            old_data_if_available,
            new,
        } => {
            if let Some(old) = old_data_if_available {
                let mut changes = Vec::new();
                if old.name != new.name {
                    changes.push(format!("name: `{}` → `{}`", old.name, new.name));
                }
                if old.permissions != new.permissions {
                    changes.push("permissions changed".to_string());
                }
                if old.colour != new.colour {
                    changes.push("colour changed".to_string());
                }
                if !changes.is_empty() {
                    let embed = crate::logging::entry(
                        "Role updated",
                        &format!("<@&{}>\n{}", new.id.get(), changes.join("\n")),
                        crate::ui::theme::colors::INFO,
                    );
                    crate::logging::log(
                        framework.serenity_context,
                        data,
                        new.guild_id.get(),
                        "roles",
                        embed,
                    )
                    .await;
                }
            }
        }
        FullEvent::GuildUpdate {
            old_data_if_available,
            new_data,
        } => {
            if let Some(old) = old_data_if_available {
                if old.name != new_data.name {
                    let embed = crate::logging::entry(
                        "Server renamed",
                        &format!("`{}` → `{}`", old.name, new_data.name),
                        crate::ui::theme::colors::INFO,
                    );
                    crate::logging::log(
                        framework.serenity_context,
                        data,
                        new_data.id.get(),
                        "server",
                        embed,
                    )
                    .await;
                }
            }
        }
        FullEvent::MessageDelete {
            channel_id,
            deleted_message_id,
            guild_id,
        } => {
            if let Some(guild_id) = guild_id {
                // Don't log deletions inside the log channel itself.
                let log_channel = crate::db::logging::channel_id(&data.db, guild_id.get())
                    .await
                    .ok()
                    .flatten();
                if log_channel != Some(channel_id.get()) {
                    let embed = crate::logging::entry(
                        "Message deleted",
                        &format!(
                            "In <#{}> · message id `{}`",
                            channel_id.get(),
                            deleted_message_id.get()
                        ),
                        crate::ui::theme::colors::WARNING,
                    );
                    crate::logging::log(
                        framework.serenity_context,
                        data,
                        guild_id.get(),
                        "messages",
                        embed,
                    )
                    .await;
                }
            }
        }
        FullEvent::InteractionCreate { interaction } => match interaction {
            serenity::Interaction::Component(component) => {
                commands::interactions::handle_component(
                    framework.serenity_context,
                    data,
                    component,
                )
                .await;
            }
            serenity::Interaction::Modal(modal) => {
                commands::interactions::handle_modal(framework.serenity_context, data, modal).await;
            }
            _ => {}
        },
        FullEvent::GuildCreate { guild, .. } => {
            tracing::info!(guild = %guild.name, id = guild.id.get(), "joined a server");
            if let Err(err) = crate::emoji::ensure_emojis_ready(
                &framework.serenity_context.http,
                &data.db,
                Some(guild),
                std::iter::empty(),
            )
            .await
            {
                tracing::warn!(error = %err, "[emoji] guildCreate check failed");
            }
        }
        FullEvent::GuildDelete { incomplete, .. } => {
            tracing::info!(guild = incomplete.id.get(), "removed from a server");
            crate::notifications::quarantine_guild(
                framework.serenity_context,
                data,
                incomplete.id.get(),
                "the bot was removed from the server",
            )
            .await;
            if let Err(err) =
                crate::db::welcome::remove_join_guild(&data.db, incomplete.id.get()).await
            {
                tracing::warn!(error = %err, guild = incomplete.id.get(), "could not remove Join DM tracking for departed guild");
            }
        }
        _ => {}
    }

    Ok(())
}

/// True when `parent_id` points at a forum channel (used to filter thread
/// events to forum posts only, matching the legacy behaviour).
fn parent_is_forum(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    parent_id: Option<serenity::ChannelId>,
) -> bool {
    let Some(parent_id) = parent_id else {
        return false;
    };
    match ctx.cache.guild(guild_id) {
        Some(guild) => guild.channels.get(&parent_id).is_some_and(|ch| {
            matches!(
                ch.kind,
                serenity::ChannelType::Forum | serenity::ChannelType::Unknown(16)
            )
        }),
        None => false,
    }
}

async fn on_error(error: poise::FrameworkError<'_, Data, commands::Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => {
            tracing::error!(error = %crate::error::display_anyhow(&error), "framework setup failed");
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            let message = crate::error::display_anyhow(&error);
            tracing::warn!(
                command = %ctx.command().qualified_name,
                user = ctx.author().id.get(),
                error = %message,
                "command failed"
            );
            let embed = crate::ui::embeds::error(
                "That did not work",
                format!("`{}`", crate::error::truncate(&message, 300)),
            );
            let _ = ctx
                .send(poise::CreateReply::default().embed(embed).ephemeral(true))
                .await;
        }
        other => {
            if let Err(err) = poise::builtins::on_error(other).await {
                tracing::warn!(error = %err, "poise error handler failed");
            }
        }
    }
}
