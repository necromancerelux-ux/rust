//! Background jobs: gift-code sync, Oracle enrichment and music-panel ticks.
//!
//! Every loop is fail-soft: an upstream outage logs and retries on the next
//! interval, it never takes the gateway connection down.

use std::time::Duration;

use poise::serenity_prelude as serenity;
use tokio::time::{interval, MissedTickBehavior};

use crate::db;
use crate::state::Data;

/// Runs a background job forever: if it panics it is logged and started again after 5 s
/// (a panic would otherwise end that job silently while the bot looked healthy). A job
/// that returns normally is not restarted.
fn spawn_restarting<F, Fut>(name: &'static str, make: F)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match tokio::spawn(make()).await {
                Ok(()) => break,
                Err(err) if err.is_panic() => {
                    tracing::error!(
                        job = name,
                        "background job panicked: {err} — restarting in 5 s"
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(_) => break, // cancelled (runtime shutting down)
            }
        }
    });
}

/// Start every long-running job. Called once the bot id is known.
pub fn spawn(ctx: &serenity::Context, data: Data) {
    spawn_gift_sync(data.clone());
    spawn_gift_announcement_retries(data.clone());
    spawn_gift_revalidation(data.clone());
    spawn_oracle_enrichment(data.clone());
    spawn_player_roster_refresh(ctx.clone(), data.clone());
    spawn_music_ticks(ctx.clone(), data.clone());
    spawn_voice_xp(ctx.clone(), data.clone());
    spawn_join_reminders(ctx.clone(), data.clone());
    spawn_welcome_message_cleanup(ctx.clone(), data.clone());
    spawn_birthday_announcer(ctx.clone(), data.clone());
    spawn_giveaway_ender(ctx.clone(), data.clone());
    spawn_ballot_closer(ctx.clone(), data.clone());
    spawn_notification_sweep(ctx.clone(), data.clone());
    spawn_sneak_peek_poller(ctx.clone(), data.clone());
    crate::ministry::spawn_sync_loop(ctx.clone(), data.clone());
    crate::ministry::spawn_outbox_retry(data.clone());
}

fn spawn_welcome_message_cleanup(ctx: serenity::Context, data: Data) {
    spawn_restarting("welcome_message_cleanup", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            sweep_cleanup_messages(&ctx, &data).await;
            let mut ticker = interval(Duration::from_secs(30));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                sweep_cleanup_messages(&ctx, &data).await;
            }
        }
    });
}

async fn sweep_cleanup_messages(ctx: &serenity::Context, data: &Data) {
    let messages = match db::welcome::due_cleanup_messages(&data.db, 20).await {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(error = %err, "feature cleanup query failed");
            return;
        }
    };
    for message in messages {
        if let (Ok(channel_id), Ok(message_id)) = (
            message.channel_id.parse::<u64>(),
            message.message_id.parse::<u64>(),
        ) {
            if let Err(err) = serenity::ChannelId::new(channel_id)
                .delete_message(&ctx.http, serenity::MessageId::new(message_id))
                .await
            {
                tracing::debug!(error = %err, channel = channel_id, message = message_id, "feature cleanup delete failed; marking item handled");
            }
        }
        if let Err(err) = db::welcome::finish_cleanup_message(&data.db, message.id).await {
            tracing::warn!(error = %err, "feature cleanup history update failed");
        }
    }
}

fn spawn_player_roster_refresh(ctx: serenity::Context, data: Data) {
    spawn_restarting("player_roster_refresh", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            let mut ticker = interval(Duration::from_secs(15 * 60));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await; // leave startup to the gateway and database
            loop {
                ticker.tick().await;
                if !data.oracle.is_configured().await {
                    continue;
                }
                let Ok(guilds) = db::settings::guilds_with_key(
                    &data.db,
                    db::settings::keys::PLAYER_ROSTER_REFRESH_HOURS,
                )
                .await
                else {
                    continue;
                };
                for (guild_id, value) in guilds {
                    let hours = value.trim().parse::<i64>().unwrap_or(0).clamp(1, 24 * 30);
                    let last = db::settings::get(
                        &data.db,
                        guild_id,
                        db::settings::keys::PLAYER_ROSTER_LAST_REFRESH,
                    )
                    .await
                    .ok()
                    .flatten()
                    .and_then(|raw| raw.parse::<i64>().ok())
                    .unwrap_or(0);
                    let now = db::now();
                    if now.saturating_sub(last) < hours.saturating_mul(3600) {
                        continue;
                    }
                    let Ok(players) = db::players::list_for_guild(&data.db, guild_id).await else {
                        continue;
                    };
                    let mut updated = 0usize;
                    let mut failed = 0usize;
                    let mut retry_soon = false;
                    for player in &players {
                        match data.oracle.fetch_player(&player.fid).await {
                            Ok(value) => {
                                let enrichment = crate::oracle::enrichment_from_player(&value);
                                if db::players::apply_enrichment(&data.db, &player.fid, &enrichment)
                                    .await
                                    .is_ok()
                                {
                                    updated += 1;
                                } else {
                                    failed += 1;
                                }
                            }
                            Err(crate::oracle::OracleError::RateLimited) => {
                                failed += 1;
                                retry_soon = true;
                                tracing::warn!(
                                    guild_id,
                                    "scheduled roster refresh: Oracle rate limited"
                                );
                                break;
                            }
                            Err(err) => {
                                failed += 1;
                                tracing::debug!(guild_id, fid = %player.fid, error = %err, "scheduled roster profile refresh failed");
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(400)).await;
                    }
                    if !retry_soon {
                        let _ = db::settings::set(
                            &data.db,
                            guild_id,
                            db::settings::keys::PLAYER_ROSTER_LAST_REFRESH,
                            Some(&now.to_string()),
                        )
                        .await;
                    }
                    let embed = serenity::CreateEmbed::new()
                        .color(crate::ui::theme::colors::INFO)
                        .title("Scheduled Player ID roster refresh")
                        .description(format!(
                            "Checked **{}** registered player(s): **{updated}** updated · **{failed}** failed. {}",
                            players.len(),
                            if retry_soon {
                                "Oracle needs attention; this guild will retry on the next 15-minute poll."
                            } else {
                                "Next refresh follows the configured interval."
                            }
                        ));
                    crate::logging::log(&ctx, &data, guild_id, "players", embed).await;
                }
            }
        }
    });
}

fn spawn_notification_sweep(ctx: serenity::Context, data: Data) {
    spawn_restarting("notification_sweep", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            crate::notifications::sweep_orphaned(&ctx, &data).await;
            // JS polls for due reminders every 500ms and sweeps deletions
            // every 10s; keep them independent so deletion work does not slow
            // delivery precision.
            let mut check_ticker = interval(Duration::from_millis(500));
            check_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
            check_ticker.tick().await;
            let mut delete_ticker = interval(Duration::from_secs(10));
            delete_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
            delete_ticker.tick().await;
            loop {
                tokio::select! {
                    _ = check_ticker.tick() => {
                        if let Err(err) = crate::notifications::check(&ctx, &data).await {
                            tracing::warn!(error = %crate::error::display_anyhow(&err), "notification sweep failed");
                        }
                    }
                    _ = delete_ticker.tick() => {
                        if let Err(err) = crate::notifications::sweep_deletions(&ctx, &data).await {
                            tracing::warn!(error = %crate::error::display_anyhow(&err), "notification delete sweep failed");
                        }
                    }
                }
            }
        }
    });
}

fn spawn_gift_sync(data: Data) {
    let interval = Duration::from_secs(data.cfg.giftcode_sync_minutes.max(5).saturating_mul(60));
    spawn_restarting("gift_sync", move || {
        let data = data.clone();
        async move {
            tokio::time::sleep(interval).await;
            let mut failure_delay = Duration::from_secs(30);
            loop {
                match run_gift_sync(&data).await {
                    Ok(()) => {
                        failure_delay = Duration::from_secs(30);
                        tokio::time::sleep(interval).await;
                    }
                    Err(retry_after) => {
                        let delay = retry_after
                            .map(|hint| hint.max(failure_delay))
                            .unwrap_or(failure_delay)
                            .min(Duration::from_secs(5 * 60));
                        tokio::time::sleep(delay).await;
                        failure_delay = failure_delay
                            .saturating_mul(2)
                            .min(Duration::from_secs(5 * 60));
                    }
                }
            }
        }
    });
}

fn spawn_gift_announcement_retries(data: Data) {
    spawn_restarting("gift_announcement_retry", move || {
        let data = data.clone();
        async move {
            let mut ticker = interval(Duration::from_secs(30));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(err) = crate::giftcode::retry_expired_announcements(&data).await {
                    tracing::warn!(error = %crate::error::display_anyhow(&err), "gift-code announcement retry failed");
                }
            }
        }
    });
}

async fn run_gift_sync(data: &Data) -> Result<(), Option<Duration>> {
    match crate::giftcode::sync(data).await {
        Ok(outcome) if !outcome.fresh.is_empty() => {
            tracing::info!(
                total = outcome.total,
                fresh = outcome.fresh.len(),
                channels = outcome.announcements,
                "gift-code sync"
            );
            if outcome.announcements == 0 {
                tracing::debug!("no gift-code announcement channel configured");
            }
            Ok(())
        }
        Ok(_) => Ok(()),
        Err(err) => {
            let message = crate::error::display_anyhow(&err);
            tracing::warn!(error = %message, "gift-code sync failed");
            let _ = crate::db::oracle::record_error(&data.db, &message).await;
            let retry_after = err
                .downcast_ref::<crate::giftcode::SyncFailure>()
                .and_then(|failure| failure.retry_after);
            Err(retry_after)
        }
    }
}

fn spawn_gift_revalidation(data: Data) {
    spawn_restarting("gift_code_revalidation", move || {
        let data = data.clone();
        async move {
            let mut ticker = interval(Duration::from_secs(90 * 60));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await;
            tokio::time::sleep(Duration::from_secs(3 * 60)).await;
            loop {
                ticker.tick().await;
                if std::env::var("GIFTCODE_REVALIDATE_ENABLED").is_ok_and(|value| value == "0") {
                    continue;
                }
                let now = db::now();
                let codes = match db::giftcodes::needing_validation(&data.db, now, 25).await {
                    Ok(codes) => codes,
                    Err(err) => {
                        tracing::warn!(error = %err, "reading gift codes for revalidation failed");
                        continue;
                    }
                };
                if codes.is_empty() {
                    continue;
                }
                let probe = db::giftcodes::probe_player(&data.db, 0).await;
                let Some(probe) = probe else {
                    tracing::warn!("gift-code revalidation skipped: no probe player is configured");
                    continue;
                };
                let (mut checked, mut removed, mut inconclusive) = (0usize, 0usize, 0usize);
                for row in codes {
                    match crate::giftcode::redeem::validate_code_with_probe(
                        &data,
                        Some(probe.clone()),
                        &row.code,
                    )
                    .await
                    {
                        validation
                            if validation.disposition
                                == crate::giftcode::redeem::Disposition::Invalid =>
                        {
                            match db::giftcodes::delete(&data.db, &row.code).await {
                                Ok(true) => removed += 1,
                                Ok(false) => {}
                                Err(err) => {
                                    tracing::warn!(code = %row.code, error = %err, "deleting invalid gift code failed")
                                }
                            }
                            checked += 1;
                        }
                        validation
                            if validation.disposition
                                == crate::giftcode::redeem::Disposition::Active =>
                        {
                            if let Err(err) = db::giftcodes::update_validation(
                                &data.db,
                                &row.code,
                                validation.is_vip,
                            )
                            .await
                            {
                                tracing::warn!(code = %row.code, error = %err, "updating gift-code validation failed");
                            }
                            checked += 1;
                        }
                        validation => {
                            tracing::debug!(code = %row.code, status = %validation.status, "gift-code revalidation inconclusive; keeping code active");
                            inconclusive += 1;
                        }
                    }
                }
                tracing::info!(
                    checked,
                    removed,
                    inconclusive,
                    "gift-code revalidation finished"
                );
            }
        }
    });
}

fn spawn_oracle_enrichment(data: Data) {
    let minutes = data.cfg.lookup_sync_minutes.max(10);
    spawn_restarting("oracle_enrichment", move || {
        let data = data.clone();
        async move {
            let mut ticker = interval(Duration::from_secs(minutes * 60));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if data.oracle.is_configured().await {
                    run_enrichment(&data).await;
                }
            }
        }
    });
}

async fn run_enrichment(data: &Data) {
    // Refresh anything older than six hours, oldest first.
    let cutoff = crate::db::now() - 6 * 3600;
    let fids = match crate::db::players::stale_fids(&data.db, cutoff, 25).await {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(error = %err, "reading stale players failed");
            return;
        }
    };

    let mut enriched = 0i64;
    for fid in &fids {
        match data.oracle.fetch_player(fid).await {
            Ok(value) => {
                let enrichment = crate::oracle::enrichment_from_player(&value);
                if let Err(err) =
                    crate::db::players::apply_enrichment(&data.db, fid, &enrichment).await
                {
                    tracing::debug!(error = %err, fid = %fid, "applying enrichment failed");
                } else {
                    enriched += 1;
                }
            }
            Err(crate::oracle::OracleError::RateLimited) => {
                tracing::warn!("oracle rate limited during enrichment");
                let _ = crate::db::oracle::record_error(&data.db, "rate limited (HTTP 429)").await;
                return;
            }
            Err(err) => {
                tracing::debug!(error = %err, fid = %fid, "player enrichment failed");
            }
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    if enriched > 0 {
        let _ = crate::db::oracle::touch_lookup_sync(&data.db, enriched).await;
        tracing::info!(players = enriched, "oracle enrichment");
    }
}

fn spawn_join_reminders(ctx: serenity::Context, data: Data) {
    spawn_restarting("join_reminders", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            // JS runs the first reminder pass ~2 min after boot so guild caches
            // can warm, then sweeps hourly.
            tokio::time::sleep(Duration::from_secs(120)).await;
            loop {
                let now_ms = crate::db::now() * 1000;
                match crate::db::welcome::due_reminders(&data.db, now_ms, 50).await {
                    Ok(due) => {
                        for (guild_id, user_id) in due {
                            crate::welcome::send_reminder(&ctx, &data, guild_id, user_id).await;
                            // JS spaces reminder DMs by 1500 ms to protect the
                            // shared REST queue.
                            tokio::time::sleep(Duration::from_millis(1500)).await;
                        }
                    }
                    Err(err) => {
                        tracing::debug!(error = %err, "reading due join reminders failed")
                    }
                }
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        }
    });
}

fn spawn_sneak_peek_poller(ctx: serenity::Context, data: Data) {
    spawn_restarting("sneak_peek_poller", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            use rand::Rng;
            let mut failures: u32 = 0;
            // JS staggers a fresh restart by 30-90s so instances don't poll together.
            let initial = rand::thread_rng().gen_range(30..=90u64);
            tokio::time::sleep(Duration::from_secs(initial)).await;
            loop {
                let mut ok = true;
                // Only poll if any server wants sneak peeks.
                let destinations = match crate::db::sneak_peek::destinations(&data.db).await {
                    Ok(destinations) => destinations,
                    Err(_) => {
                        ok = false;
                        Vec::new()
                    }
                };
                if !destinations.is_empty() {
                    let latest = match crate::sneakpeek::fetch_latest_url().await {
                        Ok(url) => url,
                        Err(_) => {
                            ok = false;
                            None
                        }
                    };
                    if let Some(latest) = latest {
                        let state = crate::db::sneak_peek::state(&data.db)
                            .await
                            .unwrap_or_default();
                        if state.last_url.as_deref() == Some(latest.as_str()) {
                            let _ = crate::db::sneak_peek::touch_checked(&data.db).await;
                        } else {
                            match crate::sneakpeek::fetch_post(&latest).await {
                                Ok(post) => match broadcast(&ctx, &data, &post).await {
                                    Ok(count) => {
                                        if count > 0 {
                                            tracing::info!(channels = count, url = %latest, "sneak peek broadcast");
                                        }
                                        let _ = crate::db::sneak_peek::set_state(
                                            &data.db, &latest, None,
                                        )
                                        .await;
                                    }
                                    Err(err) => {
                                        ok = false;
                                        tracing::warn!(error = %err, "sneak peek broadcast failed");
                                    }
                                },
                                Err(_) => ok = false,
                            }
                        }
                    } else {
                        let _ = crate::db::sneak_peek::touch_checked(&data.db).await;
                        ok = false;
                    }
                }

                // JS: 10-15 min in the 01:45-03:00 UTC hot window, 15-25 min
                // otherwise; on failure, capped exponential backoff from 30 min.
                let delay = if ok {
                    failures = 0;
                    use chrono::Timelike;
                    let now = chrono::Utc::now();
                    let minutes = now.hour() * 60 + now.minute();
                    if (105..180).contains(&minutes) {
                        rand::thread_rng().gen_range(600..=900u64)
                    } else {
                        rand::thread_rng().gen_range(900..=1500u64)
                    }
                } else {
                    failures = failures.saturating_add(1);
                    (900u64 * 2u64.pow(failures.min(10))).min(3600)
                };
                tracing::debug!(failures, delay_secs = delay, "sneak peek next poll");
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
        }
    });
}

async fn broadcast(
    ctx: &serenity::Context,
    data: &Data,
    post: &crate::sneakpeek::SneakPeekPost,
) -> anyhow::Result<usize> {
    let destinations = crate::db::sneak_peek::destinations(&data.db).await?;
    if destinations.is_empty() {
        return Ok(0);
    }
    let mut delivered = 0usize;
    for (guild_id, channel_id) in destinations {
        let embed = crate::sneakpeek::embed(post);
        match serenity::ChannelId::new(channel_id)
            .send_message(
                &ctx.http,
                serenity::CreateMessage::new()
                    .embed(embed)
                    .allowed_mentions(serenity::CreateAllowedMentions::new()),
            )
            .await
        {
            Ok(message) => {
                delivered += 1;
                let _ = serenity::ChannelId::new(channel_id)
                    .edit_message(
                        &ctx.http,
                        message.id,
                        serenity::EditMessage::new()
                            .components(vec![crate::translate::translate_row(message.id.get())]),
                    )
                    .await;
                let _ = crate::translate::register(
                    &data.db,
                    message.id.get(),
                    crate::db::translate::NewTranslatable {
                        guild_id: Some(guild_id),
                        channel_id: Some(channel_id),
                        kind: "sneakpeek",
                        embed_json: crate::sneakpeek::stored_embed(post),
                        content: None,
                        owner_id: None,
                    },
                )
                .await;
            }
            Err(err) => {
                tracing::warn!(error = %err, channel = channel_id, "sneak peek send failed")
            }
        }
    }
    Ok(delivered)
}

fn spawn_giveaway_ender(ctx: serenity::Context, data: Data) {
    spawn_restarting("giveaway_ender", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            let mut ticker = interval(Duration::from_secs(10));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await; // skip the immediate tick; run the pass now
            loop {
                let due = db::giveaways::list_due(&data.db, db::now() * 1000)
                    .await
                    .unwrap_or_default();
                for giveaway in due {
                    if let Err(err) =
                        crate::commands::giveaways::end_giveaway(&ctx, &data, &giveaway).await
                    {
                        tracing::warn!(giveaway = giveaway.id, error = %crate::error::display_anyhow(&err), "ending giveaway failed; will retry");
                    }
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                }
                ticker.tick().await;
            }
        }
    });
}

fn spawn_ballot_closer(ctx: serenity::Context, data: Data) {
    spawn_restarting("ballot_closer", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            let mut ticker = interval(Duration::from_secs(10));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await; // consume the immediate tick
            loop {
                let due = db::ballots::list_due(&data.db, db::now() * 1000)
                    .await
                    .unwrap_or_default();
                for ballot in due {
                    let claimed = db::ballots::claim_close(&data.db, ballot.id)
                        .await
                        .unwrap_or(false);
                    if claimed {
                        let _ =
                            crate::ballots::rebuild_ballot_message(&ctx, &data, ballot.id).await;
                        if let (Some(channel), Some(message)) =
                            (ballot.channel_id(), ballot.message_id())
                        {
                            let _ = db::welcome::record_ballot_cleanup_message(
                                &data.db,
                                ballot.guild_id.parse::<u64>().unwrap_or(0),
                                channel,
                                message,
                            )
                            .await;
                        }
                        let _ = crate::commands::interactions::post_ballot_results(
                            &ctx, &data, ballot.id,
                        )
                        .await;
                    }
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                }
                ticker.tick().await;
            }
        }
    });
}

fn spawn_birthday_announcer(ctx: serenity::Context, data: Data) {
    spawn_restarting("birthday_announcer", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            let mut ticker = interval(Duration::from_secs(60));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await; // skip the first immediate tick
            loop {
                ticker.tick().await;
                let today = db::birthdays::due_today(&data.db).await.unwrap_or_default();
                for (guild_id, user_id) in today {
                    announce_birthday(&ctx, &data, guild_id, user_id).await;
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                }
            }
        }
    });
}

async fn announce_birthday(ctx: &serenity::Context, data: &Data, guild_id: u64, user_id: u64) {
    let Ok(settings) = db::birthdays::settings(&data.db, guild_id).await else {
        return;
    };
    let Some(channel_id) = settings.channel() else {
        return;
    };

    let server = ctx
        .cache
        .guild(guild_id)
        .map(|guild| guild.name.clone())
        .unwrap_or_else(|| "this server".to_string());
    let member_count = ctx
        .cache
        .guild(guild_id)
        .map(|guild| guild.members.len() as u64)
        .unwrap_or(0);

    let Ok(user) = ctx.http.get_user(serenity::UserId::new(user_id)).await else {
        return;
    };
    let (title, description, color) = db::birthdays::render(
        &settings,
        &format!("<@{user_id}>"),
        &user.name,
        &user.tag(),
        &server,
        member_count,
    )
    .await;

    let mut embed = serenity::CreateEmbed::new()
        .title(title)
        .description(description)
        .color(color)
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));

    if matches!(settings.avatar_mode.as_str(), "thumbnail" | "both") {
        embed = embed.thumbnail(user.face());
    }
    if matches!(settings.avatar_mode.as_str(), "banner" | "both") {
        embed = embed.image(user.face());
    }

    if let Ok(sent) = serenity::ChannelId::new(channel_id)
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .content(format!("<@{user_id}>"))
                .embed(embed),
        )
        .await
    {
        let _ = db::birthdays::mark_announced(&data.db, guild_id, user_id).await;
        let _ = db::welcome::record_birthday_cleanup_message(
            &data.db,
            guild_id,
            channel_id,
            sent.id.get(),
        )
        .await;
    }
}

fn spawn_voice_xp(ctx: serenity::Context, data: Data) {
    spawn_restarting("voice_xp", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            let mut ticker = interval(Duration::from_secs(60));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await; // skip the first immediate tick
            loop {
                ticker.tick().await;
                crate::leveling::voice_tick(&ctx, &data).await;
            }
        }
    });
}

fn spawn_music_ticks(ctx: serenity::Context, data: Data) {
    spawn_restarting("music_ticks", move || {
        let ctx = ctx.clone();
        let data = data.clone();
        async move {
            let mut ticker = interval(Duration::from_secs(10));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let guilds = data.music.guild_ids().await;
                for guild_id in guilds {
                    if data.music.tick_position(guild_id, 10_000).await {
                        crate::music::refresh_panel(&ctx, guild_id, &data.music).await;
                    }
                }
            }
        }
    });
}
