//! Self-service Player ID registration and management.
//!
//! Mirrors the JavaScript `giftcode/services/registration.js` and its modal
//! helpers. Gift-code redemption is reused from Task 5; no second engine is
//! created here.

use std::time::Duration;

use poise::serenity_prelude as serenity;

use crate::db;
use crate::state::Data;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    Valid,
    Invalid,
    Unavailable,
}

/// Verify a FID+state pair using the signed game endpoint with a deliberately
/// fake code: the server checks the player first, so "CDK NOT FOUND" means the
/// player is real, "USER INFO ERROR"/"ROLE NOT EXIST" means it is not.
pub async fn probe_fid_state(data: &Data, fid: &str, state: i64) -> Probe {
    match crate::giftcode::redeem::probe_fid_state(
        data.gift_http(),
        fid,
        state,
        3,
        Duration::from_millis(1500),
    )
    .await
    {
        crate::giftcode::redeem::ProbeFidState::Valid => Probe::Valid,
        crate::giftcode::redeem::ProbeFidState::Invalid => Probe::Invalid,
        crate::giftcode::redeem::ProbeFidState::Unavailable => Probe::Unavailable,
    }
}

pub fn parse_fids(raw: &str) -> Option<Vec<String>> {
    let tokens: Vec<&str> = raw
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty()
        || tokens
            .iter()
            .any(|token| !token.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let mut seen = std::collections::HashSet::new();
    Some(
        tokens
            .into_iter()
            .filter(|token| seen.insert((*token).to_string()))
            .map(str::to_string)
            .collect(),
    )
}

#[derive(Debug, Clone)]
pub enum RegisterResult {
    Registered,
    ClaimedStandalone,
    AlreadyElsewhere,
    ClaimCreated(i64),
    ClaimPending,
    CapReached { used: i64, max: i64 },
    ServerLimitReached,
    InvalidFid,
    VerificationUnavailable,
    Error(String),
}

impl RegisterResult {
    pub fn line(&self, fid: &str, state: i64) -> String {
        match self {
            Self::Registered => format!("✅ `{fid}` registered"),
            Self::ClaimedStandalone => format!("✅ `{fid}` claimed (was unclaimed roster)"),
            Self::AlreadyElsewhere => format!("✅ `{fid}` already linked to your account"),
            Self::ClaimCreated(_) => format!("⏳ `{fid}` — claim request sent to admins"),
            Self::ClaimPending => format!("⏳ `{fid}` — claim already pending"),
            Self::CapReached { used, max } => {
                format!("⚠️ `{fid}` — ID limit reached ({used}/{max})")
            }
            Self::ServerLimitReached => format!("⚠️ `{fid}` — server Player ID limit reached"),
            Self::InvalidFid => format!("❌ `{fid}` not recognized in state `{state}`"),
            Self::VerificationUnavailable => {
                format!("⚠️ `{fid}` — could not verify right now (not registered)")
            }
            Self::Error(message) => format!("❌ `{fid}` — {message}"),
        }
    }
}

/// Oracle enrichment is best-effort and remains detached from registration.
fn schedule_enrichment(data: &Data, fid: String) {
    let data = data.clone();
    tokio::spawn(async move {
        if let Ok(value) = data.oracle.fetch_player(&fid).await {
            let enrichment = crate::oracle::enrichment_from_player(&value);
            let _ = db::players::apply_enrichment(&data.db, &fid, &enrichment).await;
        }
    });
}

/// Match the legacy registration catch-up: attempt active codes before telling
/// the user registration succeeded, and reject a FID if the real endpoint
/// contradicts the earlier probe.
async fn catch_up_active_codes(data: &Data, fid: &str, state: i64) -> anyhow::Result<bool> {
    let codes = db::giftcodes::list_active(&data.db, i64::MAX).await?;
    let player = [(fid.to_string(), Some(state))];
    for code in codes {
        let report =
            crate::giftcode::redeem::redeem_code_for_players(data, &code.code, &player, true)
                .await?;
        if report.player_not_exist.iter().any(|bad_fid| bad_fid == fid) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub async fn register_for_user(
    data: &Data,
    fid: &str,
    state: i64,
    user_id: u64,
    guild_id: u64,
    max_ids: i64,
) -> anyhow::Result<RegisterResult> {
    let existing = db::players::get_by_fid(&data.db, fid).await?;
    if let Some(player) = existing {
        let owner = player
            .user_id
            .as_deref()
            .and_then(|id| id.parse::<u64>().ok());
        if owner == Some(user_id) {
            return Ok(RegisterResult::AlreadyElsewhere);
        }
        if owner.is_none() {
            // Standalone roster entry: the user may claim it outright. A new
            // state must be verified before overwriting the stored one.
            if player.state != Some(state) {
                match probe_fid_state(data, fid, state).await {
                    Probe::Invalid => return Ok(RegisterResult::InvalidFid),
                    Probe::Unavailable => return Ok(RegisterResult::VerificationUnavailable),
                    Probe::Valid => {}
                }
            }
            db::players::relink(&data.db, fid, user_id, guild_id, Some(state)).await?;
            if !catch_up_active_codes(data, fid, state).await? {
                db::players::remove(&data.db, fid).await?;
                return Ok(RegisterResult::InvalidFid);
            }
            schedule_enrichment(data, fid.to_string());
            return Ok(RegisterResult::ClaimedStandalone);
        }
        // Owned by someone else: a pending claim, never a silent transfer.
        if db::player_limits::has_pending_claim(&data.db, fid).await? {
            return Ok(RegisterResult::ClaimPending);
        }
        let claim =
            db::player_limits::create_claim(&data.db, fid, user_id, guild_id, owner).await?;
        return Ok(RegisterResult::ClaimCreated(claim.id));
    }

    let used = db::players::count_by_user(&data.db, user_id).await?;
    if used >= max_ids {
        return Ok(RegisterResult::CapReached { used, max: max_ids });
    }
    // Server-wide subscription cap (null = unlimited).
    let server_cap = db::subscriptions::player_id_limit(&data.db, guild_id).await?;
    if let Some(limit) = server_cap {
        let current = db::subscriptions::count_tracked_players(&data.db, guild_id).await?;
        if current >= limit {
            return Ok(RegisterResult::ServerLimitReached);
        }
    }
    match probe_fid_state(data, fid, state).await {
        Probe::Invalid => return Ok(RegisterResult::InvalidFid),
        Probe::Unavailable => return Ok(RegisterResult::VerificationUnavailable),
        Probe::Valid => {}
    }
    let fresh = db::players::insert_new(
        &data.db,
        fid,
        guild_id,
        Some(user_id),
        user_id,
        state,
        false,
    )
    .await?;
    if !fresh {
        // Concurrent registration: re-resolve instead of transferring.
        let now = db::players::get_by_fid(&data.db, fid).await?;
        return Ok(
            if now.and_then(|p| p.user_id).as_deref() == Some(&user_id.to_string()) {
                RegisterResult::AlreadyElsewhere
            } else {
                RegisterResult::ClaimPending
            },
        );
    }
    if !catch_up_active_codes(data, fid, state).await? {
        db::players::remove(&data.db, fid).await?;
        return Ok(RegisterResult::InvalidFid);
    }
    schedule_enrichment(data, fid.to_string());
    Ok(RegisterResult::Registered)
}

/// Register a multi-FID submission atomically. Existing IDs already owned by
/// this user are no-ops; an ID owned by someone else cancels the whole batch
/// so the claim flow remains an explicit single-ID action.
pub async fn register_many_for_user(
    data: &Data,
    fids: &[String],
    state: i64,
    user_id: u64,
    guild_id: u64,
    max_ids: i64,
) -> anyhow::Result<Vec<RegisterResult>> {
    if fids.len() <= 1 {
        return match fids.first() {
            Some(fid) => Ok(vec![
                register_for_user(data, fid, state, user_id, guild_id, max_ids).await?,
            ]),
            None => Ok(Vec::new()),
        };
    }

    let mut existing = Vec::with_capacity(fids.len());
    for fid in fids {
        let row = db::players::get_by_fid(&data.db, fid).await?;
        if let Some(player) = &row {
            if player.user_id.as_deref() == Some(&user_id.to_string()) {
                existing.push(row);
                continue;
            }
            if player.user_id.is_some() {
                let results = fids
                    .iter()
                    .map(|candidate| {
                        if candidate == fid {
                            RegisterResult::Error(
                                "Batch cancelled: this ID is linked to another player. Submit it separately to request a claim.".into(),
                            )
                        } else {
                            RegisterResult::Error(
                                "Batch cancelled because one ID needs a separate ownership claim; no IDs were registered.".into(),
                            )
                        }
                    })
                    .collect();
                return Ok(results);
            }
        }
        existing.push(row);
    }

    // Verify every new or standalone ID before opening the write transaction.
    let mut failed_at = None;
    for (index, (fid, row)) in fids.iter().zip(&existing).enumerate() {
        if row
            .as_ref()
            .is_some_and(|player| player.user_id.as_deref() == Some(&user_id.to_string()))
        {
            continue;
        }
        match probe_fid_state(data, fid, state).await {
            Probe::Invalid => {
                failed_at = Some((index, RegisterResult::InvalidFid));
                break;
            }
            Probe::Unavailable => {
                failed_at = Some((index, RegisterResult::VerificationUnavailable));
                break;
            }
            Probe::Valid => {}
        }
    }
    if let Some((index, failure)) = failed_at {
        return Ok(fids
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i == index {
                    failure.clone()
                } else {
                    RegisterResult::Error(
                        "Batch cancelled because another ID could not be verified; no IDs were registered.".into(),
                    )
                }
            })
            .collect());
    }

    let user_link_count = existing
        .iter()
        .filter(|row| {
            row.as_ref()
                .is_none_or(|player| player.user_id.as_deref() != Some(&user_id.to_string()))
        })
        .count() as i64;
    let used = db::players::count_by_user(&data.db, user_id).await?;
    if used + user_link_count > max_ids {
        return Ok(fids
            .iter()
            .enumerate()
            .map(|(index, _)| {
                if index == 0 {
                    RegisterResult::CapReached {
                        used,
                        max: max_ids,
                    }
                } else {
                    RegisterResult::Error(
                        "Batch cancelled because the Player ID limit would be exceeded; no IDs were registered.".into(),
                    )
                }
            })
            .collect());
    }

    let server_cap = db::subscriptions::player_id_limit(&data.db, guild_id).await?;
    if let Some(limit) = server_cap {
        let current = db::subscriptions::count_tracked_players(&data.db, guild_id).await?;
        let additions = existing
            .iter()
            .filter(|row| {
                row.as_ref()
                    .is_none_or(|player| player.home_guild_id != guild_id.to_string())
            })
            .count() as i64;
        if current + additions > limit {
            return Ok(fids
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    if index == 0 {
                        RegisterResult::ServerLimitReached
                    } else {
                        RegisterResult::Error(
                            "Batch cancelled because the server Player ID limit would be exceeded; no IDs were registered.".into(),
                        )
                    }
                })
                .collect());
        }
    }

    let mut tx = data.db.pool.begin().await?;
    let mut invalid_after_catchup = std::collections::HashSet::new();
    for (fid, row) in fids.iter().zip(&existing) {
        if row
            .as_ref()
            .is_some_and(|player| player.user_id.as_deref() == Some(&user_id.to_string()))
        {
            continue;
        }
        if row.is_some() {
            let result = sqlx::query(
                "UPDATE players SET user_id = ?, home_guild_id = ?, state = ?, is_standalone = 0, updated_at = ? WHERE fid = ? AND user_id IS NULL",
            )
            .bind(user_id.to_string())
            .bind(guild_id.to_string())
            .bind(state)
            .bind(db::now())
            .bind(fid)
            .execute(&mut *tx)
            .await?;
            if result.rows_affected() == 0 {
                anyhow::bail!(
                    "player ownership changed while the batch was being committed; retry the IDs"
                );
            }
        } else {
            let now = db::now();
            let result = sqlx::query(
                "INSERT INTO players (fid, user_id, home_guild_id, state, is_standalone, added_by, added_at, updated_at) VALUES (?, ?, ?, ?, 0, ?, ?, ?) ON CONFLICT(fid) DO NOTHING",
            )
            .bind(fid)
            .bind(user_id.to_string())
            .bind(guild_id.to_string())
            .bind(state)
            .bind(user_id.to_string())
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            if result.rows_affected() == 0 {
                anyhow::bail!(
                    "registration changed while the batch was being committed; retry the IDs"
                );
            }
        }
    }
    tx.commit().await?;

    for (fid, row) in fids.iter().zip(&existing) {
        if row
            .as_ref()
            .is_none_or(|player| player.user_id.as_deref() != Some(&user_id.to_string()))
        {
            if catch_up_active_codes(data, fid, state).await? {
                schedule_enrichment(data, fid.clone());
            } else {
                db::players::remove(&data.db, fid).await?;
                invalid_after_catchup.insert(fid.as_str());
            }
        }
    }
    Ok(existing
        .iter()
        .zip(fids)
        .map(|(row, fid)| {
            if invalid_after_catchup.contains(fid.as_str()) {
                RegisterResult::InvalidFid
            } else if row
                .as_ref()
                .is_some_and(|player| player.user_id.as_deref() == Some(&user_id.to_string()))
            {
                RegisterResult::AlreadyElsewhere
            } else if row.is_some() {
                RegisterResult::ClaimedStandalone
            } else {
                RegisterResult::Registered
            }
        })
        .collect())
}

/// Resolve a member's effective cap using base / booster / role / override.
pub async fn effective_limit(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: u64,
    user_id: u64,
) -> db::player_limits::Limit {
    let (is_booster, role_ids) = {
        let guild = ctx.cache.guild(serenity::GuildId::new(guild_id));
        let member = guild
            .as_ref()
            .and_then(|guild| guild.members.get(&serenity::UserId::new(user_id)));
        (
            member.and_then(|member| member.premium_since).is_some(),
            member
                .map(|member| member.roles.iter().map(|id| id.get()).collect::<Vec<u64>>())
                .unwrap_or_default(),
        )
    };
    db::player_limits::resolve_limit(&data.db, guild_id, user_id, is_booster, &role_ids)
        .await
        .unwrap_or(db::player_limits::Limit {
            cap: db::player_limits::DEFAULT_BASE_CAP,
            source: "base limit",
        })
}

/// `[ALLIANCE]Name-RANK`, trimmed from the player name to fit 32 characters.
pub fn format_nickname(alliance: &str, name: &str, rank: &str) -> Option<(String, bool)> {
    let alliance = alliance.trim().to_uppercase();
    let rank = rank.trim().to_uppercase();
    let name = name.trim();
    if alliance.len() != 3
        || !alliance.bytes().all(|byte| byte.is_ascii_alphabetic())
        || name.is_empty()
        || !["R1", "R2", "R3", "R4", "R5"].contains(&rank.as_str())
    {
        return None;
    }
    let mut nickname = format!("[{alliance}]{name}-{rank}");
    let truncated = if nickname.chars().count() > 32 {
        let overflow = nickname.chars().count() - 32;
        let keep = name.chars().count().saturating_sub(overflow).max(1);
        let trimmed: String = name.chars().take(keep).collect();
        nickname = format!("[{alliance}]{trimmed}-{rank}");
        true
    } else {
        false
    };
    Some((nickname, truncated))
}

/// Private translated Player ID menu. Uses the shared translation store and
/// language preferences from Task 3; never creates a parallel store.
pub async fn translated_menu(
    _data: &Data,
    guild_id: u64,
    user_id: u64,
    lang: &str,
) -> (serenity::CreateEmbed, Vec<serenity::CreateActionRow>) {
    let _ = (guild_id, user_id);
    let lang = if crate::translate::is_supported_lang(lang) {
        lang.to_string()
    } else {
        "en".to_string()
    };
    let template: Vec<&str> = vec![
        "🪪 Player ID Registration",
        "🔒 Private menu — only you can see this message.",
        "↑ Dropdown = translate this menu · buttons stay below.",
        "Register my Play ID",
        "Use Code",
        "My Gift Codes",
        "Remove my ID",
        "In-Game Name",
    ];
    let translated = if lang == "en" {
        template
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    } else {
        crate::translate::translate_batch(
            crate::translate::translation_client(),
            &template
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>(),
            &lang,
        )
        .await
    };
    let embed = serenity::CreateEmbed::new()
        .title(&translated[0])
        .description(format!(
            "{}\nLanguage: {}\n{}\n\n🔵 **{}**\n🎁 **{}**\n🟢 **{}**\n🔴 **{}**\n🏷️ **{}**",
            translated[1],
            crate::translate::lang_label(&lang),
            translated[2],
            translated[3],
            translated[4],
            translated[5],
            translated[6],
            translated[7]
        ))
        .color(0x2ECC71)
        .footer(serenity::CreateEmbedFooter::new(crate::ui::theme::footer()));
    let options = crate::translate::LANGUAGES
        .iter()
        .map(|(label, emoji, code)| {
            serenity::CreateSelectMenuOption::new(*label, *code)
                .emoji(serenity::ReactionType::Unicode((*emoji).to_string()))
                .default_selection(*code == lang)
        })
        .collect();
    let lang_row = serenity::CreateActionRow::SelectMenu(
        serenity::CreateSelectMenu::new(
            "player:tr_lang",
            serenity::CreateSelectMenuKind::String { options },
        )
        .placeholder(crate::translate::lang_label(&lang)),
    );
    let button = |id: &str, label: &str, emoji: &str, style: serenity::ButtonStyle| {
        serenity::CreateButton::new(id)
            .label(label)
            .style(style)
            .emoji(serenity::ReactionType::Unicode(emoji.to_string()))
    };
    let actions = serenity::CreateActionRow::Buttons(vec![
        button(
            "player:register",
            &translated[3],
            "🔵",
            serenity::ButtonStyle::Primary,
        ),
        button(
            "gift:use",
            &translated[4],
            "🎁",
            serenity::ButtonStyle::Primary,
        ),
        button(
            "player:codes",
            &translated[5],
            "🟢",
            serenity::ButtonStyle::Success,
        ),
        button(
            "player:remove",
            &translated[6],
            "🔴",
            serenity::ButtonStyle::Danger,
        ),
        button(
            "player:nickname",
            &translated[7],
            "🏷️",
            serenity::ButtonStyle::Secondary,
        ),
    ]);
    (embed, vec![lang_row, actions])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_ids() {
        assert_eq!(
            parse_fids("123, 456\n789; 123"),
            Some(vec!["123".into(), "456".into(), "789".into()])
        );
        assert_eq!(parse_fids("123, abc"), None);
        assert_eq!(parse_fids("  "), None);
    }

    #[test]
    fn nickname_format_and_truncation() {
        assert_eq!(
            format_nickname("ing", "Dev", "r4"),
            Some(("[ING]Dev-R4".to_string(), false))
        );
        let (nickname, truncated) =
            format_nickname("ING", "AveryLongPlayerNameThatOverflows", "R5").unwrap();
        assert!(truncated);
        assert!(nickname.starts_with("[ING]"));
        assert!(nickname.ends_with("-R5"));
        assert!(nickname.chars().count() <= 32);
        assert!(format_nickname("IN", "Dev", "R4").is_none());
        assert!(format_nickname("ING", "Dev", "R9").is_none());
    }
}
