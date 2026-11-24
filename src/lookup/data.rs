//! Dual-source Lookup data layer.
//!
//! Fetches Whiteout Stats and WoS Oracle **in parallel**.
//! - Basics (identity, power, alliance, state) come from whichever answers first.
//! - Rich fields (gear, arena, progression, crowns, loadout) are filled from
//!   Oracle when a token is present and the response is usable.
//!
//! Whiteout Stats remains the primary / faster source for the common case;
//! Oracle is the enrichment / second source.

use std::time::Duration;

use serde_json::Value;
use tokio::time::timeout;

use crate::lookup::types::PlayerProfile;
use crate::lookup::LookupClient;
use crate::oracle::OracleClient;

const RACE_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Debug, Clone, Default)]
pub struct PlayerBundle {
    /// Normalised profile used by panels.
    pub profile: PlayerProfile,
    /// Raw Whiteout Stats payload (if any).
    pub whiteout: Option<Value>,
    /// Raw Oracle player payload (if any).
    pub oracle: Option<Value>,
    /// Raw Oracle loadout (gear / heroes) if fetched.
    pub loadout: Option<Value>,
    /// Which source supplied the primary identity fields.
    pub primary_source: Source,
    /// Human-readable note for the footer / Test Data.
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    WhiteoutStats,
    Oracle,
    Merged,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::WhiteoutStats => write!(f, "Whiteout Stats"),
            Source::Oracle => write!(f, "WoS Oracle"),
            Source::Merged => write!(f, "Whiteout Stats + Oracle"),
        }
    }
}

/// Parallel player lookup.
///
/// `query` may be a player id / fid or a name (Whiteout Stats search).
pub async fn fetch_player(
    lookup: &LookupClient,
    oracle: &OracleClient,
    query: &str,
) -> anyhow::Result<PlayerBundle> {
    let q = query.trim().to_string();
    if q.is_empty() {
        anyhow::bail!("empty player query");
    }

    // Fire both sources at once.
    let ws_fut = async {
        timeout(RACE_TIMEOUT, lookup.player(&q))
            .await
            .map_err(|_| anyhow::anyhow!("Whiteout Stats timed out"))?
            .map_err(|e| anyhow::anyhow!("Whiteout Stats: {e}"))
    };
    let oracle_fut = async {
        // Oracle prefers numeric / fid ids. If the query looks like a name,
        // skip the direct player call (search is not implemented on Oracle yet).
        if q.chars().all(|c| c.is_ascii_digit()) {
            timeout(RACE_TIMEOUT, oracle.fetch_player(&q))
                .await
                .map_err(|_| anyhow::anyhow!("Oracle timed out"))?
                .map_err(|e| anyhow::anyhow!("Oracle: {e}"))
        } else {
            Err(anyhow::anyhow!("Oracle needs a numeric player id for direct lookup"))
        }
    };

    let (ws_res, oracle_res) = tokio::join!(ws_fut, oracle_fut);

    let mut bundle = PlayerBundle::default();

    match (ws_res, oracle_res) {
        (Ok(profile), Ok(oracle_val)) => {
            bundle.whiteout = Some(profile.raw.clone());
            bundle.oracle = Some(oracle_val.clone());
            // Prefer Whiteout Stats identity; enrich from Oracle.
            let mut merged = profile;
            enrich_profile_from_oracle(&mut merged, &oracle_val);
            bundle.profile = merged;
            bundle.primary_source = Source::Merged;
            bundle.note = "merged · Whiteout Stats + Oracle".into();

            // Loadout (gear / heroes) — best-effort, only when we have a fid.
            if let Some(fid) = bundle.profile.fid.clone() {
                if let Ok(Ok(loadout)) = timeout(RACE_TIMEOUT, oracle.fetch_loadout(&fid)).await {
                    bundle.loadout = Some(loadout);
                }
            }
        }
        (Ok(profile), Err(oracle_err)) => {
            bundle.whiteout = Some(profile.raw.clone());
            bundle.profile = profile;
            bundle.primary_source = Source::WhiteoutStats;
            bundle.note = format!("Whiteout Stats only ({oracle_err})");
        }
        (Err(ws_err), Ok(oracle_val)) => {
            bundle.oracle = Some(oracle_val.clone());
            let profile = PlayerProfile::from_value(&oracle_val);
            // Ensure fid is set from the query when Oracle omits it.
            let mut profile = profile;
            if profile.fid.is_none() {
                profile.fid = Some(q.clone());
            }
            bundle.profile = profile;
            bundle.primary_source = Source::Oracle;
            bundle.note = format!("Oracle only ({ws_err})");

            if let Some(fid) = bundle.profile.fid.clone() {
                if let Ok(Ok(loadout)) = timeout(RACE_TIMEOUT, oracle.fetch_loadout(&fid)).await {
                    bundle.loadout = Some(loadout);
                }
            }
        }
        (Err(ws_err), Err(oracle_err)) => {
            anyhow::bail!("both sources failed: {ws_err}; {oracle_err}");
        }
    }

    Ok(bundle)
}

/// Overlay Oracle fields onto a Whiteout Stats profile when WS left them empty.
fn enrich_profile_from_oracle(profile: &mut PlayerProfile, oracle: &Value) {
    let root = oracle
        .get("player")
        .or_else(|| oracle.get("data"))
        .or_else(|| oracle.get("profile"))
        .unwrap_or(oracle);

    let pick_str = |keys: &[&str]| -> Option<String> {
        for k in keys {
            if let Some(v) = root.get(*k) {
                if let Some(s) = v.as_str().filter(|s| !s.trim().is_empty()) {
                    return Some(s.trim().to_string());
                }
                if let Some(n) = v.as_i64() {
                    return Some(n.to_string());
                }
            }
        }
        None
    };
    let pick_i64 = |keys: &[&str]| -> Option<i64> {
        for k in keys {
            if let Some(v) = root.get(*k) {
                if let Some(n) = v.as_i64() {
                    return Some(n);
                }
                if let Some(s) = v.as_str() {
                    let cleaned: String = s.chars().filter(|c| *c != ',' && *c != '_').collect();
                    if let Ok(n) = cleaned.parse::<f64>() {
                        return Some(n as i64);
                    }
                }
            }
        }
        None
    };

    if profile.name.is_none() {
        profile.name = pick_str(&["username", "name", "game_name", "nickname", "ign"]);
    }
    if profile.fid.is_none() {
        profile.fid = pick_str(&["id", "fid", "player_id", "uid"]);
    }
    if profile.state.is_none() {
        profile.state = pick_i64(&["state", "kid", "kingdom", "sid"]);
    }
    if profile.alliance_abbr.is_none() {
        profile.alliance_abbr = pick_str(&["alliance_abbr", "alliance_tag", "allianceAbbr"]);
    }
    if profile.alliance_name.is_none() {
        profile.alliance_name = pick_str(&["alliance_name", "allianceName"]);
    }
    if profile.alliance_id.is_none() {
        profile.alliance_id = pick_str(&["alliance_id", "allianceId"]);
    }
    if profile.power.is_none() {
        profile.power = pick_i64(&["power", "total_power", "totalPower"]);
    }
    if profile.kills.is_none() {
        profile.kills = pick_i64(&["kills", "total_kills", "totalKills"]);
    }
    if profile.town_hall.is_none() {
        profile.town_hall = pick_i64(&[
            "town_hall_level",
            "townHallLevel",
            "furnace_level",
            "furnaceLevel",
            "level",
        ]);
    }
    if profile.avatar_url.is_none() {
        profile.avatar_url = pick_str(&["avatar_url", "avatar", "avatarUrl", "photo"]);
    }
}
