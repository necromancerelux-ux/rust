//! User-scoped drafts for batch notification setup.
//!
//! The legacy wizard holds its preview in memory until the administrator
//! confirms Create All. Keep the same draft boundary in Rust and persist only
//! after confirmation.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

const TTL: Duration = Duration::from_secs(2 * 60 * 60);
const MAX_DRAFTS: usize = 500;

#[derive(Debug, Clone)]
pub struct WizardSetup {
    pub id: String,
    pub guild_id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    pub events: Vec<String>,
    pub event_times: HashMap<String, String>,
    pub event_repeats: HashMap<String, String>,
    pub configured_events: HashSet<String>,
    /// Per-run JS wizard reminder selection; `None` uses the default preset.
    pub notification_type: Option<i64>,
    /// Descending minute offsets when notification_type is 6.
    pub custom_times: Option<String>,
    pub created: Instant,
}

#[derive(Debug, Clone)]
pub struct WizardInstance {
    pub event_type: String,
    pub instance_identifier: String,
    pub hour: i64,
    pub minute: i64,
    pub timezone: String,
    pub description: String,
    pub notification_type: i64,
    pub mention_type: String,
    pub embed_title: Option<String>,
    pub repeat_minutes: i64,
    pub weekdays: Vec<u32>,
    pub start_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct WizardDraft {
    pub id: String,
    pub guild_id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    pub instances: Vec<WizardInstance>,
    pub created: Instant,
}

fn drafts() -> &'static Mutex<HashMap<String, WizardDraft>> {
    static DRAFTS: OnceLock<Mutex<HashMap<String, WizardDraft>>> = OnceLock::new();
    DRAFTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn setups() -> &'static Mutex<HashMap<String, WizardSetup>> {
    static SETUPS: OnceLock<Mutex<HashMap<String, WizardSetup>>> = OnceLock::new();
    SETUPS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn insert_setup(setup: WizardSetup) {
    if let Ok(mut map) = setups().lock() {
        map.retain(|_, setup| setup.created.elapsed() < TTL);
        map.insert(setup.id.clone(), setup);
        while map.len() > MAX_DRAFTS {
            let oldest = map
                .iter()
                .min_by_key(|(_, setup)| setup.created)
                .map(|(id, _)| id.clone());
            if let Some(id) = oldest {
                map.remove(&id);
            } else {
                break;
            }
        }
    }
}

pub fn get_setup(id: &str, user_id: u64, guild_id: u64) -> Option<WizardSetup> {
    let mut map = setups().lock().ok()?;
    if map
        .get(id)
        .is_some_and(|setup| setup.created.elapsed() >= TTL)
    {
        map.remove(id);
        return None;
    }
    map.get(id)
        .filter(|setup| setup.user_id == user_id && setup.guild_id == guild_id)
        .cloned()
}

pub fn remove_setup(id: &str, user_id: u64, guild_id: u64) -> bool {
    let Ok(mut map) = setups().lock() else {
        return false;
    };
    if map
        .get(id)
        .is_some_and(|setup| setup.user_id == user_id && setup.guild_id == guild_id)
    {
        map.remove(id);
        true
    } else {
        false
    }
}

pub fn insert(draft: WizardDraft) {
    if let Ok(mut map) = drafts().lock() {
        map.retain(|_, draft| draft.created.elapsed() < TTL);
        map.insert(draft.id.clone(), draft);
        while map.len() > MAX_DRAFTS {
            let oldest = map
                .iter()
                .min_by_key(|(_, draft)| draft.created)
                .map(|(id, _)| id.clone());
            if let Some(id) = oldest {
                map.remove(&id);
            } else {
                break;
            }
        }
    }
}

pub fn get(id: &str, user_id: u64, guild_id: u64) -> Option<WizardDraft> {
    let mut map = drafts().lock().ok()?;
    let expired = map
        .get(id)
        .is_some_and(|draft| draft.created.elapsed() >= TTL);
    if expired {
        map.remove(id);
        return None;
    }
    map.get(id)
        .filter(|draft| draft.user_id == user_id && draft.guild_id == guild_id)
        .cloned()
}

pub fn remove(id: &str, user_id: u64, guild_id: u64) -> bool {
    let Ok(mut map) = drafts().lock() else {
        return false;
    };
    if map
        .get(id)
        .is_some_and(|draft| draft.user_id == user_id && draft.guild_id == guild_id)
    {
        map.remove(id);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_access_is_scoped_to_owner_and_server() {
        let id = format!(
            "wizard-test-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        let draft = WizardDraft {
            id: id.clone(),
            guild_id: 10,
            user_id: 20,
            channel_id: 30,
            instances: Vec::new(),
            created: Instant::now(),
        };
        insert(draft);
        assert!(get(&id, 20, 10).is_some());
        assert!(get(&id, 21, 10).is_none());
        assert!(get(&id, 20, 11).is_none());
        assert!(remove(&id, 20, 10));
        assert!(get(&id, 20, 10).is_none());
    }
}
