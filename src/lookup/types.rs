//! Normalised lookup types.
//!
//! The upstream API is loosely typed (`uid`, `fid`, `player_id` all appear), so
//! parsing is by alias search rather than a rigid struct.

use serde_json::Value;

fn pick<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(found) = object.get(*key) {
            if !found.is_null() {
                return Some(found);
            }
        }
    }
    None
}

fn pick_str(value: &Value, keys: &[&str]) -> Option<String> {
    let found = pick(value, keys)?;
    match found {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn pick_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    let found = pick(value, keys)?;
    match found {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            let cleaned: String = s.chars().filter(|c| *c != ',' && *c != '_').collect();
            cleaned.parse::<f64>().ok().map(|f| f as i64)
        }
        _ => None,
    }
}

fn pick_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    let found = pick(value, keys)?;
    match found {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlayerProfile {
    pub fid: Option<String>,
    pub uid: Option<String>,
    pub name: Option<String>,
    pub state: Option<i64>,
    pub alliance_abbr: Option<String>,
    pub alliance_name: Option<String>,
    pub alliance_id: Option<String>,
    pub alliance_rank: Option<i64>,
    pub power: Option<i64>,
    pub kills: Option<i64>,
    pub town_hall: Option<i64>,
    pub life_tree: Option<i64>,
    pub avatar_url: Option<String>,
    pub raw: Value,
}

impl PlayerProfile {
    pub fn from_value(value: &Value) -> Self {
        let player = pick(value, &["player", "profile", "data"]).unwrap_or(value);

        let alliance = pick(player, &["alliance", "team"]);
        let abbr = pick_str(
            player,
            &["alliance_abbr", "alliance_tag", "allianceAbbr", "tag"],
        )
        .or_else(|| {
            alliance.and_then(|a| pick_str(a, &["abbr", "tag", "short_name", "shortName"]))
        });
        let alliance_name = pick_str(player, &["alliance_name", "allianceName"])
            .or_else(|| alliance.and_then(|a| pick_str(a, &["name", "full_name", "fullName"])));
        let alliance_id = pick_str(player, &["alliance_id", "allianceId"])
            .or_else(|| alliance.and_then(|a| pick_str(a, &["id", "aid", "alliance_id"])));
        let alliance_rank = pick_i64(player, &["alliance_rank", "allianceRank", "rank"])
            .or_else(|| alliance.and_then(|a| pick_i64(a, &["rank", "member_rank"])));

        let avatar = pick_str(
            player,
            &["avatar_url", "avatar", "avatarUrl", "photo", "picture"],
        )
        .map(|url| absolute_avatar(&url));

        Self {
            fid: pick_str(player, &["fid", "player_id", "playerId", "id"]),
            uid: pick_str(player, &["uid", "uuid", "oracle_uid", "oracleUid"]),
            name: pick_str(
                player,
                &["game_name", "name", "nickname", "ign", "gameName"],
            ),
            state: pick_i64(player, &["state", "kid", "kingdom", "state_id", "stateId"]),
            alliance_abbr: abbr,
            alliance_name,
            alliance_id,
            alliance_rank,
            power: pick_i64(player, &["power", "total_power", "totalPower"])
                .or_else(|| pick_i64(value, &["power"])),
            kills: pick_i64(player, &["kills", "total_kills", "totalKills"]),
            town_hall: pick_i64(
                player,
                &[
                    "town_hall_level",
                    "townHallLevel",
                    "tc_level",
                    "furnace_level",
                    "furnaceLevel",
                ],
            ),
            life_tree: pick_i64(
                player,
                &[
                    "life_tree_level",
                    "lifeTreeLevel",
                    "tree_level",
                    "life_tree",
                ],
            ),
            avatar_url: avatar,
            raw: value.clone(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fid.is_none() && self.uid.is_none() && self.name.is_none()
    }
}

fn absolute_avatar(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if url.starts_with("//") {
        format!("https:{url}")
    } else if url.starts_with('/') {
        format!("https://www.whiteoutstats.com{url}")
    } else {
        url.to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct AllianceProfile {
    pub abbr: String,
    pub name: Option<String>,
    pub state: Option<i64>,
    pub power: Option<i64>,
    pub kills: Option<i64>,
    pub members: Vec<AllianceMember>,
    pub raw: Value,
}

#[derive(Debug, Clone, Default)]
pub struct AllianceMember {
    pub fid: Option<String>,
    pub name: String,
    pub power: i64,
    pub rank: Option<i64>,
}

impl AllianceProfile {
    pub fn from_value(abbr: &str, value: &Value) -> Self {
        let alliance = pick(value, &["alliance", "data", "info"]).unwrap_or(value);
        let roster = pick(alliance, &["roster", "members", "players"])
            .or_else(|| pick(value, &["roster", "members", "players"]))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let members = roster
            .iter()
            .map(|m| AllianceMember {
                fid: pick_str(m, &["fid", "player_id", "id"]),
                name: pick_str(m, &["game_name", "name", "nickname", "ign"])
                    .unwrap_or_else(|| "unknown".to_string()),
                power: pick_i64(m, &["power", "total_power"]).unwrap_or(0),
                rank: pick_i64(m, &["alliance_rank", "rank"]),
            })
            .collect::<Vec<_>>();

        Self {
            abbr: pick_str(alliance, &["abbr", "tag", "short_name"])
                .unwrap_or_else(|| abbr.to_string()),
            name: pick_str(alliance, &["name", "full_name"]),
            state: pick_i64(alliance, &["state", "kid", "kingdom"]),
            power: pick_i64(alliance, &["power", "total_power"]),
            kills: pick_i64(alliance, &["kills", "total_kills"]),
            members,
            raw: value.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StateProfile {
    pub state: i64,
    pub alliances: Vec<StateAlliance>,
    pub players: Vec<StatePlayer>,
    pub raw: Value,
}

#[derive(Debug, Clone, Default)]
pub struct StateAlliance {
    pub abbr: String,
    pub name: Option<String>,
    pub power: i64,
    pub members: usize,
    pub rank: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct StatePlayer {
    pub fid: Option<String>,
    pub name: String,
    pub power: i64,
    pub alliance: Option<String>,
}

impl StateProfile {
    pub fn from_value(state: i64, value: &Value) -> Self {
        let root = pick(value, &["state", "data", "kingdom"]).unwrap_or(value);

        let alliances = pick(root, &["alliances", "top_alliances"])
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(i, a)| StateAlliance {
                abbr: pick_str(a, &["abbr", "tag", "short_name", "name"])
                    .unwrap_or_else(|| format!("#{}", i + 1)),
                name: pick_str(a, &["name", "full_name"]),
                power: pick_i64(a, &["power", "total_power"]).unwrap_or(0),
                members: pick_i64(a, &["members", "member_count", "members_count"])
                    .unwrap_or(0)
                    .max(0) as usize,
                rank: pick_i64(a, &["rank"]),
            })
            .collect();

        let players = pick(root, &["players", "top_players"])
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|p| StatePlayer {
                fid: pick_str(p, &["fid", "player_id", "id"]),
                name: pick_str(p, &["game_name", "name", "nickname"])
                    .unwrap_or_else(|| "unknown".to_string()),
                power: pick_i64(p, &["power", "total_power"]).unwrap_or(0),
                alliance: pick_str(p, &["alliance_abbr", "alliance_tag", "alliance"]),
            })
            .collect();

        Self {
            state,
            alliances,
            players,
            raw: value.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_player() {
        let value = json!({
            "uid": "1234",
            "fid": "555666777",
            "name": "Necromancer",
            "state": 512,
            "alliance": { "abbr": "WOS", "name": "Warriors of Snow", "rank": 3 },
            "power": "12,345,678",
            "kills": 98765,
            "townHallLevel": 30,
            "avatar": "/avatars/a.png"
        });
        let p = PlayerProfile::from_value(&value);
        assert_eq!(p.fid.as_deref(), Some("555666777"));
        assert_eq!(p.alliance_abbr.as_deref(), Some("WOS"));
        assert_eq!(p.power, Some(12_345_678));
        assert_eq!(p.town_hall, Some(30));
        assert_eq!(
            p.avatar_url.as_deref(),
            Some("https://www.whiteoutstats.com/avatars/a.png")
        );
    }

    #[test]
    fn parses_state() {
        let value = json!({
            "kid": 42,
            "alliances": [
                { "abbr": "AAA", "power": 100, "members": 90 },
                { "abbr": "BBB", "power": 90, "members": 88 }
            ]
        });
        let s = StateProfile::from_value(42, &value);
        assert_eq!(s.alliances.len(), 2);
        assert_eq!(s.alliances[0].members, 90);
    }
}
