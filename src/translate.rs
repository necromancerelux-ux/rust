//! Shared "Translate" button for public bot messages.
//!
//! Any public message the bot posts can carry a **Translate** button; clicking
//! it replies with an ephemeral language picker; picking a language replies
//! with a private translated copy of the message. The original message is
//! never edited.
//!
//! Translation uses Google's free `translate_a/single?client=gtx` endpoint —
//! the same unofficial endpoint the legacy bot used via
//! `google-translate-api-x`. No API key is required.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Result};
use poise::serenity_prelude as serenity;
use serde::{Deserialize, Serialize};
use serenity::{
    ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, CreateSelectMenu,
    CreateSelectMenuKind, CreateSelectMenuOption, Embed, EmbedField, ReactionType,
};

/// Languages, labels, emojis, codes and order mirror `utils/translate.js`
/// exactly (including the Chinese label "简体中文").
pub const LANGUAGES: &[(&str, &str, &str)] = &[
    ("English", "🇬🇧", "en"),
    ("Deutsch", "🇩🇪", "de"),
    ("Français", "🇫🇷", "fr"),
    ("Español", "🇪🇸", "es"),
    ("Italiano", "🇮🇹", "it"),
    ("Русский", "🇷🇺", "ru"),
    ("Українська", "🇺🇦", "uk"),
    ("Português", "🇵🇹", "pt"),
    ("Türkçe", "🇹🇷", "tr"),
    ("العربية", "🇸🇦", "ar"),
    ("日本語", "🇯🇵", "ja"),
    ("한국어", "🇰🇷", "ko"),
    ("简体中文", "🇨🇳", "zh-CN"),
    ("हिन्दी", "🇮🇳", "hi"),
    ("नेपाली", "🇳🇵", "ne"),
];

/// True when `code` is one of the supported language codes. Interaction
/// handlers must validate selections with this before using them.
pub fn is_supported_lang(code: &str) -> bool {
    LANGUAGES.iter().any(|(_, _, lang)| *lang == code)
}

/// `Deutsch 🇩🇪` — the label shown on a language selector's placeholder.
pub fn lang_label(code: &str) -> String {
    LANGUAGES
        .iter()
        .find(|(_, _, lang)| *lang == code)
        .map(|(label, emoji, _)| format!("{label} {emoji}"))
        .unwrap_or_else(|| code.to_string())
}

/// Rare delimiter used to translate a batch of short strings in one API call.
/// Matches `BATCH_DELIMITER` from the JavaScript source.
const BATCH_DELIMITER: &str = "\n§§§\n";
const BATCH_SPLIT: &str = "§§§";
const PRIVATE_TRANSLATION_FOOTER: &str = "Private translation – only you can see this.";
const PRIVATE_ORIGINAL_PREFIX: &str = "**Private original – only you can see this.**\n";

/// Embed payload persisted for later translation. Kept intentionally minimal
/// for callers, but translation reads the raw stored JSON so full embed
/// metadata from either runtime is preserved.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredEmbed {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<StoredField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredField {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub inline: bool,
}

impl StoredEmbed {
    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }

    pub fn to_create_embed(&self) -> CreateEmbed {
        let mut embed = CreateEmbed::new();
        if let Some(title) = &self.title {
            embed = embed.title(clamp(title.clone(), 256));
        }
        if let Some(description) = &self.description {
            embed = embed.description(clamp(description.clone(), 4000));
        }
        for field in self.fields.iter().take(25) {
            embed = embed.field(
                clamp(field.name.clone(), 256),
                clamp(field.value.clone(), 1024),
                field.inline,
            );
        }
        if let Some(footer) = &self.footer {
            embed = embed.footer(serenity::CreateEmbedFooter::new(footer.clone()));
        }
        if let Some(color) = self.color {
            embed = embed.color(color);
        }
        embed
    }
}

/// Discord length limits for translated text (kept generous but under the
/// hard API caps; translated text can be longer than the source).
fn clamp(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// The single Translate button, shared by rows that mix it with feature
/// controls. Matches `translateRow` in the JavaScript bot (Success + 🌐).
pub fn translate_button(message_id: u64) -> CreateButton {
    CreateButton::new(format!("translate:{message_id}"))
        .label("Translate")
        .style(ButtonStyle::Success)
        .emoji(ReactionType::Unicode("🌐".to_string()))
}

pub fn translate_row(message_id: u64) -> CreateActionRow {
    CreateActionRow::Buttons(vec![translate_button(message_id)])
}

/// Language selector for the generic private Translate flow.
pub fn language_select_row(message_id: u64) -> CreateActionRow {
    language_select_row_with_default(message_id, "en")
}

/// Language selector with the user's remembered language preselected.
pub fn language_select_row_with_default(message_id: u64, current: &str) -> CreateActionRow {
    let menu = CreateSelectMenu::new(
        format!("translate_lang:{message_id}"),
        CreateSelectMenuKind::String {
            options: LANGUAGES
                .iter()
                .map(|(label, emoji, code)| {
                    CreateSelectMenuOption::new(*label, *code)
                        .emoji(ReactionType::Unicode((*emoji).to_string()))
                        .default_selection(*code == current)
                })
                .collect(),
        },
    )
    .placeholder(if is_supported_lang(current) {
        lang_label(current)
    } else {
        "Choose your language".to_string()
    });
    CreateActionRow::SelectMenu(menu)
}

/// Controls on the private translated/original response. This is a Task 3
/// addition (the JavaScript shared flow has no "view original" button).
pub fn private_view_row(message_id: u64, lang: &str) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(format!("translate_view:translated:{message_id}:{lang}"))
            .label("Translated")
            .style(ButtonStyle::Primary)
            .emoji(ReactionType::Unicode("🌐".to_string())),
        serenity::CreateButton::new(format!("translate_view:original:{message_id}:{lang}"))
            .label("Original")
            .style(ButtonStyle::Secondary)
            .emoji(ReactionType::Unicode("📄".to_string())),
    ])
}

/// Persist a full embed snapshot, matching the JavaScript bot's
/// `EmbedBuilder#toJSON()` registration. Keeps fields, author, colour,
/// thumbnail and all other metadata available for private translation.
pub fn stored_json(embed: &CreateEmbed) -> Option<String> {
    serde_json::to_string(embed).ok()
}

/// Register a message as translatable (upsert).
pub async fn register(
    db: &crate::db::Db,
    message_id: u64,
    input: crate::db::translate::NewTranslatable<'_>,
) -> anyhow::Result<()> {
    crate::db::translate::register(db, message_id, input).await?;
    Ok(())
}

async fn translate_one(client: &reqwest::Client, text: &str, target: &str) -> Result<String> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t&q={}",
        target,
        urlencoding(text)
    );
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| anyhow!("translate request: {e}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("translate HTTP {}", resp.status().as_u16()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("translate json: {e}"))?;
    // Response shape: [[["translated","original",...],...],...]
    let segments = body[0]
        .as_array()
        .ok_or_else(|| anyhow!("unexpected translate response"))?;
    let mut out = String::new();
    for segment in segments {
        if let Some(text) = segment[0].as_str() {
            out.push_str(text);
        }
    }
    Ok(out)
}

fn urlencoding(text: &str) -> String {
    text.bytes()
        .flat_map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![b as char]
            }
            _ => format!("%{b:02X}").chars().collect(),
        })
        .collect()
}

/// Translate a single string. Failures are returned to the caller (which must
/// fall back to source text or show a clear private error) and are never
/// cached as successes.
pub async fn translate_text(client: &reqwest::Client, text: &str, target: &str) -> Result<String> {
    if !is_supported_lang(target) {
        return Err(anyhow!("unsupported language `{target}`"));
    }
    translate_one(client, text, target).await
}

/// Translate a batch of strings in one call, falling back to per-string calls
/// if the delimiter does not survive translation. Preserves positions (empty
/// strings included) so field names/values never shift.
pub async fn translate_batch(
    client: &reqwest::Client,
    texts: &[String],
    target: &str,
) -> Vec<String> {
    if target == "en" || texts.is_empty() {
        return texts.to_vec();
    }
    if !is_supported_lang(target) {
        return texts.to_vec();
    }
    let joined = texts.join(BATCH_DELIMITER);
    if let Ok(translated) = translate_one(client, &joined, target).await {
        let parts: Vec<String> = translated
            .split(BATCH_SPLIT)
            .map(|part| part.trim().to_string())
            .collect();
        if parts.len() == texts.len() {
            return parts;
        }
    }
    // Fallback: translate one by one; keep source text on failure.
    let mut out = Vec::with_capacity(texts.len());
    for text in texts {
        match translate_one(client, text, target).await {
            Ok(translated) => out.push(translated),
            Err(_) => out.push(text.clone()),
        }
    }
    out
}

/// Parse a stored embed payload into a serenity embed. Works with both the
/// compact JSON written by Rust call sites and full `EmbedBuilder#toJSON()`
/// payloads written by the JavaScript bot (DB compatibility).
fn parse_stored_embed(json: &str) -> Option<Embed> {
    serde_json::from_str::<Embed>(json).ok()
}

/// Build an embed for the private *translated* view: title, description and
/// every field name/value translated; inline flags and other metadata kept;
/// image removed and footer replaced, matching the JavaScript flow.
pub async fn translate_embed_payload(
    client: &reqwest::Client,
    json: &str,
    target: &str,
) -> Option<CreateEmbed> {
    let mut embed = parse_stored_embed(json)?;

    let mut strings = Vec::new();
    strings.push(embed.title.clone().unwrap_or_default());
    strings.push(embed.description.clone().unwrap_or_default());
    for field in &embed.fields {
        strings.push(field.name.clone());
        strings.push(field.value.clone());
    }
    let translated = translate_batch(client, &strings, target).await;
    let mut iter = translated.into_iter();

    if let Some(title) = iter.next() {
        embed.title = (!title.is_empty()).then(|| clamp(title, 256));
    }
    if let Some(description) = iter.next() {
        embed.description = (!description.is_empty()).then(|| clamp(description, 4000));
    }
    embed.fields = embed
        .fields
        .iter()
        .map(|field| {
            EmbedField::new(
                clamp(iter.next().unwrap_or_else(|| field.name.clone()), 256),
                clamp(iter.next().unwrap_or_else(|| field.value.clone()), 1024),
                field.inline,
            )
        })
        .take(25)
        .collect();
    // JS drops the image from the private translated embed (untranslatable,
    // external CDN fetch), and stamps a privacy footer.
    embed.image = None;
    let mut translated = CreateEmbed::from(embed);
    translated = translated.footer(serenity::CreateEmbedFooter::new(PRIVATE_TRANSLATION_FOOTER));
    Some(translated)
}

/// Compatibility helper: translate the embed stored on a translatable record.
pub async fn translate_stored(
    client: &reqwest::Client,
    record: &crate::db::translate::TranslatableMessage,
    target: &str,
) -> Option<CreateEmbed> {
    let json = record.embed_json.as_deref()?;
    translate_embed_payload(client, json, target).await
}

/// Build the private *original* view of a stored embed: the untranslated
/// payload (including its image and original footer) rendered verbatim.
pub fn original_embed_payload(json: &str) -> Option<CreateEmbed> {
    let embed = parse_stored_embed(json)?;
    Some(CreateEmbed::from(embed))
}

/// Plain original content with a private-view prefix.
pub fn original_content(content: &str) -> String {
    format!("{PRIVATE_ORIGINAL_PREFIX}{content}")
}

/// Plain translated content with the private-view prefix used by JavaScript.
pub fn translated_content(text: &str) -> String {
    format!("**Private translation – only you can see this.**\n{text}")
}

/// A cached private translation, retained so toggling back to the translated
/// view does not repeat the translation API call.
#[derive(Clone, Debug, Default)]
pub struct PrivateTranslation {
    pub embed: Option<CreateEmbed>,
    pub content: Option<String>,
}

struct TranslationCache {
    entries: HashMap<(u64, u64, String), PrivateTranslation>,
    order: VecDeque<(u64, u64, String)>,
}

const CACHE_MAX: usize = 256;

fn translation_cache() -> &'static Mutex<TranslationCache> {
    static CACHE: OnceLock<Mutex<TranslationCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(TranslationCache {
            entries: HashMap::new(),
            order: VecDeque::new(),
        })
    })
}

pub fn cache_translation(message_id: u64, user_id: u64, lang: &str, view: PrivateTranslation) {
    if let Ok(mut cache) = translation_cache().lock() {
        let key = (message_id, user_id, lang.to_string());
        if !cache.entries.contains_key(&key) {
            cache.order.push_back(key.clone());
            while cache.order.len() > CACHE_MAX {
                if let Some(oldest) = cache.order.pop_front() {
                    cache.entries.remove(&oldest);
                }
            }
        }
        cache.entries.insert(key, view);
    }
}

pub fn cached_translation(message_id: u64, user_id: u64, lang: &str) -> Option<PrivateTranslation> {
    let key = (message_id, user_id, lang.to_string());
    translation_cache().lock().ok()?.entries.get(&key).cloned()
}

pub fn translation_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        crate::net::builder()
            .user_agent("Mozilla/5.0 (compatible; OmniCore/2.0)")
            .build()
            .expect("translate client")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_table_matches_javascript() {
        assert_eq!(LANGUAGES.len(), 15);
        assert_eq!(LANGUAGES[0], ("English", "🇬🇧", "en"));
        assert_eq!(LANGUAGES[12], ("简体中文", "🇨🇳", "zh-CN"));
        assert_eq!(LANGUAGES[14], ("नेपाली", "🇳🇵", "ne"));
        assert!(is_supported_lang("zh-CN"));
        assert!(!is_supported_lang("zh"));
        assert_eq!(lang_label("de"), "Deutsch 🇩🇪");
    }

    #[test]
    fn delimiter_matches_javascript() {
        assert_eq!(BATCH_DELIMITER, "\n§§§\n");
        assert_eq!(BATCH_SPLIT, "§§§");
    }

    #[test]
    fn stored_embed_parses_full_and_compact_payloads() {
        let full = r#"{"title":"Hello","description":"World","color":5793246,
            "fields":[{"name":"A","value":"B","inline":true}],
            "author":{"name":"Bot"},"image":{"url":"https://example.com/i.png"}}"#;
        let embed = parse_stored_embed(full).unwrap();
        assert_eq!(embed.title.as_deref(), Some("Hello"));
        assert_eq!(embed.author.map(|a| a.name), Some("Bot".to_string()));
        assert!(embed.image.is_some());

        let compact = r#"{"title":"Hi","fields":[]}"#;
        assert_eq!(
            parse_stored_embed(compact).unwrap().title.as_deref(),
            Some("Hi")
        );
    }

    #[test]
    fn clamp_respects_limits() {
        assert_eq!(clamp("hello".to_string(), 5), "hello");
        assert_eq!(clamp("abcdef".to_string(), 5), "abcde…");
    }

    #[test]
    fn cache_is_bounded_and_scoped() {
        for i in 0..(CACHE_MAX + 10) as u64 {
            cache_translation(
                i,
                7,
                "de",
                PrivateTranslation {
                    content: Some(i.to_string()),
                    ..Default::default()
                },
            );
        }
        assert!(cached_translation(0, 7, "de").is_none());
        assert_eq!(
            cached_translation((CACHE_MAX + 9) as u64, 7, "de")
                .unwrap()
                .content
                .as_deref(),
            Some("265")
        );
        assert!(cached_translation((CACHE_MAX + 9) as u64, 8, "de").is_none());
    }
}
