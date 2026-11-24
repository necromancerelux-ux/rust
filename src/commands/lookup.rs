//! Shared lookup helpers used by the panel (no slash commands).

use crate::db::{self, players::PlayerRecord};
use crate::lookup::PlayerProfile;
use crate::ui::panels;
use crate::ui::{embeds, theme};
use poise::serenity_prelude as serenity;

fn record_view(row: &PlayerRecord) -> panels::LookupPlayerView {
    panels::LookupPlayerView {
        fid: row.fid.clone(),
        game_name: row.game_name.clone(),
        alliance: row
            .alliance_abbr
            .clone()
            .map(|abbr| match &row.alliance_name {
                Some(name) => format!("[{abbr}] {name}"),
                None => format!("[{abbr}]"),
            }),
        power: row.power,
        kills: row.kills,
        town_hall: row.town_hall_level,
        life_tree: row.life_tree_level,
        state: row.state,
        avatar_url: row.avatar_url.clone(),
        synced: row.last_oracle_sync,
    }
}

fn profile_view(profile: &PlayerProfile, fallback: &str) -> panels::LookupPlayerView {
    panels::LookupPlayerView {
        fid: profile.fid.clone().unwrap_or_else(|| fallback.to_string()),
        game_name: profile.name.clone(),
        alliance: profile
            .alliance_abbr
            .clone()
            .map(|abbr| match &profile.alliance_name {
                Some(name) => format!("[{abbr}] {name}"),
                None => format!("[{abbr}]"),
            }),
        power: profile.power,
        kills: profile.kills,
        town_hall: profile.town_hall,
        life_tree: profile.life_tree,
        state: profile.state,
        avatar_url: profile.avatar_url.clone(),
        synced: None,
    }
}

/// Public entry used by the lookup panel buttons.
///
/// Races Whiteout Stats + WoS Oracle (dual-source). Falls back to locally
/// tracked players when both upstream sources fail.
///
/// Returns the display view and the richest raw profile JSON available
/// (used by canvas card renderers and V2 panels).
pub async fn player_by_query(
    data: &crate::state::Data,
    guild: u64,
    query: &str,
) -> anyhow::Result<(panels::LookupPlayerView, Option<serde_json::Value>)> {
    match crate::lookup::fetch_player(&data.lookup, &data.oracle, query).await {
        Ok(bundle) => {
            let raw = bundle
                .oracle
                .clone()
                .or(bundle.whiteout.clone())
                .or_else(|| Some(bundle.profile.raw.clone()));
            return Ok((profile_view(&bundle.profile, query), raw));
        }
        Err(_) => { /* fall through to local DB */ }
    }
    let rows = db::players::search(&data.db, guild, query, 5).await?;
    if let Some(row) = rows.first() {
        return Ok((record_view(row), None));
    }
    Err(crate::error::OmniError::Lookup(format!("no player matched `{query}`")).into())
}

/// Dual-source fetch that also returns the full [`PlayerBundle`].
pub async fn player_bundle_by_query(
    data: &crate::state::Data,
    guild: u64,
    query: &str,
) -> anyhow::Result<crate::lookup::PlayerBundle> {
    match crate::lookup::fetch_player(&data.lookup, &data.oracle, query).await {
        Ok(bundle) => Ok(bundle),
        Err(upstream) => {
            let rows = db::players::search(&data.db, guild, query, 5).await?;
            if let Some(row) = rows.first() {
                let profile = crate::lookup::PlayerProfile {
                    fid: Some(row.fid.clone()),
                    name: row.game_name.clone(),
                    state: row.state,
                    alliance_abbr: row.alliance_abbr.clone(),
                    alliance_name: row.alliance_name.clone(),
                    power: row.power,
                    kills: row.kills,
                    town_hall: row.town_hall_level,
                    life_tree: row.life_tree_level,
                    avatar_url: row.avatar_url.clone(),
                    ..Default::default()
                };
                return Ok(crate::lookup::PlayerBundle {
                    profile,
                    primary_source: crate::lookup::DataSource::WhiteoutStats,
                    note: format!("local DB (upstream: {upstream})"),
                    ..Default::default()
                });
            }
            Err(upstream)
        }
    }
}

pub fn not_found(query: &str) -> serenity::CreateEmbed {
    embeds::error(
        "No player found",
        format!("Nothing matched `{query}`. Check the FID and try again."),
    )
    .color(theme::colors::DANGER)
}
