//! Cross-server custom emoji install.
//!
//! A bot only needs to be a member of the ONE server that owns a custom emoji
//! to use that emoji's `<:name:id>` tag in messages/embeds it sends to ANY
//! other server it's in. Instead of re-uploading the icon set into every
//! server (which needs "Manage Expressions" everywhere and burns an emoji
//! slot per server), we upload once into a single "home" server and reuse
//! those tags everywhere else.
//!
//! Flow matches the legacy Node `utils/emojiSetup.js`:
//! - First server the bot is in (on boot) or the first it is invited to is
//!   auto-picked as home — or `EMOJI_GUILD_ID` in the environment overrides.
//! - Icons in `assets/emoji-icons/` are uploaded there once; tags are saved
//!   to `material_emojis`.
//! - Later `guildCreate` events only top up missing icons on the home server;
//!   they never upload into the newly joined server.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context as _, Result};
use base64::Engine as _;
use serenity::all::{Guild, GuildId, Http};
use serenity::model::guild::Emoji;
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::db::Db;

/// Maps Discord emoji file name (without `.png`) → calculator material key.
const FILE_NAME_TO_KEY: &[(&str, &str)] = &[
    ("mat_wood", "wood"),
    ("mat_iron", "iron"),
    ("mat_fire_crystal", "fireCrystal"),
    ("mat_refined_fc_shards", "refinedFC"),
    ("mat_hardened_alloy", "hardenedAlloy"),
    ("mat_polishing_solution", "polishingSolution"),
    ("mat_design_plans", "designPlans"),
    ("mat_lunar_amber", "lunarAmber"),
    ("mat_charm_guide", "charmGuides"),
    ("mat_charm_design", "charmDesigns"),
    ("mat_charm_secrets", "charmSecrets"),
];

/// Unicode fallbacks used until (or if) the custom emoji is uploaded.
const UNICODE_FALLBACKS: &[(&str, &str)] = &[
    ("wood", "🪵"),
    ("iron", "⛏️"),
    ("fireCrystal", "🔺"),
    ("refinedFC", "💠"),
    ("hardenedAlloy", "🔩"),
    ("polishingSolution", "🧴"),
    ("designPlans", "📐"),
    ("lunarAmber", "🟠"),
    ("charmGuides", "📙"),
    ("charmDesigns", "📜"),
    ("charmSecrets", "📕"),
];

static TAG_CACHE: OnceLock<std::sync::RwLock<HashMap<String, String>>> = OnceLock::new();

fn tag_cache() -> &'static std::sync::RwLock<HashMap<String, String>> {
    TAG_CACHE.get_or_init(|| std::sync::RwLock::new(HashMap::new()))
}

fn icon_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("assets/emoji-icons"),
        PathBuf::from("emoji-icons"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/emoji-icons"),
    ];
    for p in candidates {
        if p.is_dir() {
            return p;
        }
    }
    PathBuf::from("assets/emoji-icons")
}

/// Return the display tag for a material key (custom `<:name:id>` if installed,
/// otherwise the Unicode fallback). Safe to call from any thread after
/// [`reload_tags_from_db`] has run at least once.
pub fn material_tag(key: &str) -> String {
    if let Ok(guard) = tag_cache().read() {
        if let Some(tag) = guard.get(key) {
            return tag.clone();
        }
    }
    UNICODE_FALLBACKS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| (*v).to_string())
        .unwrap_or_else(|| "•".to_string())
}

/// Reload saved tags from the database into the process-wide cache.
pub async fn reload_tags_from_db(db: &Db) -> Result<()> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT material_key, tag FROM material_emojis")
            .fetch_all(&db.pool)
            .await
            .context("loading material_emojis")?;
    let mut map = HashMap::with_capacity(rows.len());
    for (k, v) in rows {
        map.insert(k, v);
    }
    if let Ok(mut guard) = tag_cache().write() {
        *guard = map;
    }
    Ok(())
}

async fn get_home_guild_id(db: &Db) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT guild_id FROM emoji_home_guild WHERE id = 1")
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.map(|r| r.0))
}

async fn set_home_guild_id(db: &Db, guild_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO emoji_home_guild (id, guild_id) VALUES (1, ?)
         ON CONFLICT(id) DO UPDATE SET guild_id = excluded.guild_id, set_at = datetime('now')",
    )
    .bind(guild_id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

async fn save_tag(db: &Db, key: &str, tag: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO material_emojis (material_key, tag) VALUES (?, ?)
         ON CONFLICT(material_key) DO UPDATE SET tag = excluded.tag, updated_at = datetime('now')",
    )
    .bind(key)
    .bind(tag)
    .execute(&db.pool)
    .await?;
    if let Ok(mut guard) = tag_cache().write() {
        guard.insert(key.to_string(), tag.to_string());
    }
    Ok(())
}

fn file_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

async fn get_saved_hash(db: &Db, file_name: &str) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT hash FROM emoji_file_hashes WHERE file_name = ?")
            .bind(file_name)
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.map(|r| r.0))
}

async fn set_saved_hash(db: &Db, file_name: &str, hash: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO emoji_file_hashes (file_name, hash) VALUES (?, ?)
         ON CONFLICT(file_name) DO UPDATE SET hash = excluded.hash, updated_at = datetime('now')",
    )
    .bind(file_name)
    .bind(hash)
    .execute(&db.pool)
    .await?;
    Ok(())
}

fn emoji_to_tag(emoji: &Emoji) -> String {
    if emoji.animated {
        format!("<a:{}:{}>", emoji.name, emoji.id)
    } else {
        format!("<:{}:{}>", emoji.name, emoji.id)
    }
}

/// PNG bytes → Discord image data URI (`data:image/png;base64,…`).
fn png_data_uri(bytes: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:image/png;base64,{b64}")
}

/// Call on ClientReady and on every GuildCreate.
///
/// - Picks / persists the home guild (env override or first available).
/// - Uploads any missing icons into that home guild only.
/// - Never uploads into a newly joined non-home server.
pub async fn ensure_emojis_ready(
    http: &Http,
    db: &Db,
    newly_joined: Option<&Guild>,
    all_guild_ids: impl IntoIterator<Item = GuildId>,
) -> Result<()> {
    let _ = reload_tags_from_db(db).await;

    let env_override = std::env::var("EMOJI_GUILD_ID")
        .ok()
        .filter(|s| !s.is_empty());
    let mut home_id = env_override.clone().or(get_home_guild_id(db).await?);

    if home_id.is_none() {
        let candidate = newly_joined
            .map(|g| g.id)
            .or_else(|| all_guild_ids.into_iter().next());
        if let Some(id) = candidate {
            home_id = Some(id.to_string());
            set_home_guild_id(db, &id.to_string()).await?;
            let name = newly_joined.map(|g| g.name.as_str()).unwrap_or("?");
            info!(
                "[emoji] auto-picked \"{}\" ({}) as the home server for custom emoji",
                name, id
            );
        } else {
            return Ok(());
        }
    } else if let Some(ref env_id) = env_override {
        if get_home_guild_id(db).await?.as_deref() != Some(env_id.as_str()) {
            set_home_guild_id(db, env_id).await?;
        }
    }

    let home_id = home_id.expect("home_id set above");

    if let Some(g) = newly_joined {
        if g.id.to_string() != home_id {
            return Ok(());
        }
    }

    let guild_id: GuildId = home_id.parse().context("invalid emoji home guild id")?;

    let guild_name = match guild_id.to_partial_guild(http).await {
        Ok(g) => g.name,
        Err(err) => {
            warn!(
                "[emoji] home server {} is not reachable (bot may have been removed): {err}",
                home_id
            );
            return Ok(());
        }
    };

    let emojis = match http.get_emojis(guild_id).await {
        Ok(list) => list,
        Err(err) => {
            warn!("[emoji] cannot list emojis in home server: {err}");
            return Ok(());
        }
    };

    let dir = icon_dir();
    if !dir.is_dir() {
        warn!(
            "[emoji] icon directory {} not found — skipping upload",
            dir.display()
        );
        return Ok(());
    }

    let mut uploaded = 0u32;
    let mut skipped = 0u32;
    let mut failed = 0u32;

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let key = FILE_NAME_TO_KEY
            .iter()
            .find(|(name, _)| *name == stem)
            .map(|(_, k)| *k);

        if let Some(k) = key {
            if let Ok(guard) = tag_cache().read() {
                if guard.contains_key(k) {
                    skipped += 1;
                    continue;
                }
            }
        }

        if let Some(existing_emoji) = emojis.iter().find(|e| e.name == stem) {
            if let Some(k) = key {
                let tag = emoji_to_tag(existing_emoji);
                let _ = save_tag(db, k, &tag).await;
            }
            skipped += 1;
            continue;
        }

        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(err) => {
                warn!("[emoji] cannot read {}: {err}", path.display());
                failed += 1;
                continue;
            }
        };
        let hash = file_sha256(&bytes);

        // Optional: skip re-upload if hash matches and Discord still has it
        // (handled above via name match). If Discord lost it, re-upload.
        let _ = get_saved_hash(db, &file_name).await;

        let image = png_data_uri(&bytes);
        match guild_id.create_emoji(http, &stem, &image).await {
            Ok(emoji) => {
                if let Some(k) = key {
                    let tag = emoji_to_tag(&emoji);
                    let _ = save_tag(db, k, &tag).await;
                }
                let _ = set_saved_hash(db, &file_name, &hash).await;
                info!(
                    "[emoji] uploaded {} to home server \"{}\"",
                    stem, guild_name
                );
                uploaded += 1;
            }
            Err(err) => {
                warn!(
                    "[emoji] failed to upload {} (need Manage Expressions / Create Guild Expressions in home server?): {err}",
                    stem
                );
                failed += 1;
            }
        }
    }

    if uploaded > 0 || failed > 0 {
        info!(
            "[emoji] home-server install done: uploaded={uploaded} skipped={skipped} failed={failed}"
        );
    }
    Ok(())
}
