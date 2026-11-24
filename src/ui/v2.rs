//! Discord Components V2 helpers — Rust mirror of the JS `lookup/ui/v2.js`.
//!
//! Built as **raw JSON** so we work with crates.io serenity 0.12.5 (which does
//! not yet ship CreateContainer / TextDisplay / IS_COMPONENTS_V2 typed builders).
//! The JSON is posted via the interaction edit/response HTTP endpoints.
//!
//! Layout target (matches the JS bot 1:1):
//!
//! ```text
//! ┃ [ tab buttons / tab dropdown ]
//! ┃ ───────────────
//! ┃ ## Title
//! ┃ body text / rendered image card
//! ┃ [ dropdown ]  [ pager « ‹ 1/4 › » ]  [ link buttons ]
//! ┃ ───────────────
//! ┃ -# OmniCore · data as of <relative time>
//! ```

use poise::serenity_prelude as serenity;
use serde_json::{json, Value};
use serenity::builder::CreateAttachment;
use serenity::http::Http;
use serenity::model::application::ComponentInteraction;

/// Light-blue accent bar used by the JS reference panels.
pub const ACCENT: u32 = 0x3B_A3_E0;
pub const BRAND: &str = "OmniCore";

/// Discord Components V2 type ids.
const T_ACTION_ROW: u8 = 1;
const T_BUTTON: u8 = 2;
const T_STRING_SELECT: u8 = 3;
const T_TEXT_DISPLAY: u8 = 10;
const T_MEDIA_GALLERY: u8 = 12;
const T_SEPARATOR: u8 = 14;
const T_CONTAINER: u8 = 17;

/// Message flag: IS_COMPONENTS_V2 (1 << 15).
const FLAG_IS_COMPONENTS_V2: u64 = 1 << 15;
const FLAG_EPHEMERAL: u64 = 1 << 6;

// ---------------------------------------------------------------------------
// Public option / button defs (same shape the panels already use)
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct ButtonDef {
    pub id: String,
    pub label: String,
    pub style: Option<serenity::ButtonStyle>,
    pub disabled: bool,
    pub emoji: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone)]
pub struct Tab {
    pub key: String,
    pub label: String,
}

#[derive(Clone)]
pub struct SelectOpt {
    pub label: String,
    pub value: String,
    pub description: Option<String>,
    pub is_default: bool,
}

// ---------------------------------------------------------------------------
// V2 accumulator
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct V2 {
    parts: Vec<Value>,
}

impl V2 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(mut self, content: impl AsRef<str>) -> Self {
        let s: String = content.as_ref().chars().take(3900).collect();
        if !s.trim().is_empty() {
            self.parts.push(json!({
                "type": T_TEXT_DISPLAY,
                "content": s,
            }));
        }
        self
    }

    pub fn title(self, t: impl AsRef<str>) -> Self {
        self.text(format!("## {}", t.as_ref()))
    }

    pub fn sep(mut self, divider: bool) -> Self {
        self.parts.push(json!({
            "type": T_SEPARATOR,
            "divider": divider,
            "spacing": 1, // Small
        }));
        self
    }

    pub fn image(mut self, name: &str) -> Self {
        let url = format!("attachment://{name}");
        self.parts.push(json!({
            "type": T_MEDIA_GALLERY,
            "items": [{ "media": { "url": url } }],
        }));
        self
    }

    pub fn footer(self, label: &str, when_ms: Option<i64>) -> Self {
        let when = when_ms.map(rel).unwrap_or_default();
        let line = if when.is_empty() {
            format!("-# {BRAND} · {label}")
        } else {
            format!("-# {BRAND} · {label} {when}")
        };
        self.sep(true).text(line)
    }

    pub fn footer_raw(self, str: impl AsRef<str>) -> Self {
        self.sep(true).text(format!("-# {BRAND} · {}", str.as_ref()))
    }

    pub fn buttons(mut self, defs: &[ButtonDef]) -> Self {
        if defs.is_empty() {
            return self;
        }
        let mut components = Vec::new();
        for d in defs.iter().take(5) {
            let style = d
                .style
                .map(|s| match s {
                    serenity::ButtonStyle::Primary => 1,
                    serenity::ButtonStyle::Secondary => 2,
                    serenity::ButtonStyle::Success => 3,
                    serenity::ButtonStyle::Danger => 4,
                    _ => 2,
                })
                .unwrap_or(2);
            let mut btn = json!({
                "type": T_BUTTON,
                "style": if d.url.is_some() { 5 } else { style },
                "label": d.label.chars().take(80).collect::<String>(),
                "disabled": d.disabled,
            });
            if let Some(url) = &d.url {
                btn["url"] = json!(url);
            } else {
                btn["custom_id"] = json!(&d.id);
            }
            if let Some(emoji) = &d.emoji {
                btn["emoji"] = json!({ "name": emoji });
            }
            components.push(btn);
        }
        self.parts.push(json!({
            "type": T_ACTION_ROW,
            "components": components,
        }));
        self
    }

    pub fn tab_buttons(mut self, prefix: &str, tabs: &[Tab], active: &str, per_row: usize) -> Self {
        let per = per_row.max(1);
        for chunk in tabs.chunks(per) {
            let defs: Vec<ButtonDef> = chunk
                .iter()
                .map(|t| ButtonDef {
                    id: format!("{prefix}:{}", t.key),
                    label: t.label.clone(),
                    style: Some(if t.key == active {
                        serenity::ButtonStyle::Primary
                    } else {
                        serenity::ButtonStyle::Secondary
                    }),
                    ..Default::default()
                })
                .collect();
            self = self.buttons(&defs);
        }
        self
    }

    pub fn select(mut self, id: &str, placeholder: &str, opts: &[SelectOpt]) -> Self {
        let mut seen = std::collections::HashSet::new();
        let options: Vec<Value> = opts
            .iter()
            .filter_map(|o| {
                let value: String = o.value.chars().take(100).collect();
                let label: String = o.label.chars().take(100).collect();
                if value.is_empty() || label.trim().is_empty() || !seen.insert(value.clone()) {
                    return None;
                }
                let mut opt = json!({
                    "label": label,
                    "value": value,
                    "default": o.is_default,
                });
                if let Some(desc) = &o.description {
                    opt["description"] = json!(desc.chars().take(100).collect::<String>());
                }
                Some(opt)
            })
            .take(25)
            .collect();
        if options.is_empty() {
            return self;
        }
        self.parts.push(json!({
            "type": T_ACTION_ROW,
            "components": [{
                "type": T_STRING_SELECT,
                "custom_id": id,
                "placeholder": placeholder.chars().take(150).collect::<String>(),
                "options": options,
            }],
        }));
        self
    }

    pub fn pager(self, prefix: &str, page: usize, total: usize) -> Self {
        let last = total.saturating_sub(1);
        let page = page.min(last);
        let mk = |tag: &str, target: usize, label: &str, disabled: bool| ButtonDef {
            id: format!("{prefix}:p:{tag}:{target}"),
            label: label.to_string(),
            style: Some(serenity::ButtonStyle::Secondary),
            disabled,
            ..Default::default()
        };
        self.buttons(&[
            mk("first", 0, "«", page == 0),
            mk("prev", page.saturating_sub(1), "‹", page == 0),
            ButtonDef {
                id: format!("{prefix}:noop"),
                label: format!("{} / {}", page + 1, total.max(1)),
                style: Some(serenity::ButtonStyle::Secondary),
                disabled: true,
                ..Default::default()
            },
            mk("next", (page + 1).min(last), "›", page >= last),
            mk("last", last, "»", page >= last),
        ])
    }

    /// Finish into a single accented container component (top-level).
    pub fn into_container_json(self) -> Value {
        json!({
            "type": T_CONTAINER,
            "accent_color": ACCENT,
            "components": self.parts,
        })
    }
}

// ---------------------------------------------------------------------------
// Payload + apply helpers
// ---------------------------------------------------------------------------

pub struct V2Edit {
    pub components: Vec<Value>,
    pub files: Vec<CreateAttachment>,
    pub ephemeral: bool,
}

impl V2Edit {
    pub fn from_v2(v2: V2, files: Vec<CreateAttachment>, ephemeral: bool) -> Self {
        Self {
            components: vec![v2.into_container_json()],
            files,
            ephemeral,
        }
    }

    /// Edit the original interaction response with Components V2 JSON.
    pub async fn apply_token(self, http: &Http, token: &str) -> anyhow::Result<()> {
        let mut flags = FLAG_IS_COMPONENTS_V2;
        if self.ephemeral {
            flags |= FLAG_EPHEMERAL;
        }
        // Discord matches multipart `files[n]` to this metadata by id. Panels
        // currently attach at most one card image, so index ids are sufficient.
        let attachments: Vec<Value> = self
            .files
            .iter()
            .enumerate()
            .map(|(id, file)| json!({ "id": id, "filename": file.filename }))
            .collect();
        // Message edits keep existing content/embeds unless they are explicitly
        // emptied. IS_COMPONENTS_V2 forbids both, so clear them here.
        let body = json!({
            "content": "",
            "embeds": [],
            "components": self.components,
            "flags": flags,
            "attachments": attachments,
        });
        http.edit_original_interaction_response(token, &body, self.files)
            .await
            .map_err(|e| anyhow::anyhow!("edit V2 response: {e}"))?;
        Ok(())
    }

    pub async fn apply(
        self,
        http: &Http,
        interaction: &ComponentInteraction,
    ) -> anyhow::Result<()> {
        self.apply_token(http, &interaction.token).await
    }
}

/// Build a V2 edit payload (ephemeral by default for private lookup).
pub fn payload(v2: V2, files: Vec<CreateAttachment>, ephemeral: bool) -> V2Edit {
    V2Edit::from_v2(v2, files, ephemeral)
}

/// Attachment helper matching previous API.
pub fn attachment(bytes: Vec<u8>, name: &str) -> CreateAttachment {
    CreateAttachment::bytes(bytes, name)
}

// ---------------------------------------------------------------------------
// Formatting helpers (shared with panels)
// ---------------------------------------------------------------------------

pub fn fmt(n: Option<i64>) -> String {
    match n {
        None => "—".into(),
        Some(n) => {
            let a = n.unsigned_abs();
            let s = if n < 0 { "−" } else { "" };
            if a >= 1_000_000_000 {
                format!("{s}{:.2}B", n as f64 / 1e9)
            } else if a >= 1_000_000 {
                format!("{s}{:.2}M", n as f64 / 1e6)
            } else if a >= 1_000 {
                format!("{s}{:.1}K", n as f64 / 1e3)
            } else {
                format!("{s}{n}")
            }
        }
    }
}

pub fn int(n: Option<i64>) -> String {
    match n {
        None => "—".into(),
        Some(n) => n
            .to_string()
            .as_bytes()
            .rchunks(3)
            .rev()
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(","),
    }
}

pub fn age_chip(ts_ms: Option<i64>) -> String {
    let Some(t) = ts_ms else { return String::new() };
    let now = chrono::Utc::now().timestamp_millis();
    let sec = ((now - t).max(0)) / 1000;
    let v = if sec < 3600 {
        format!("{}m", (sec / 60).max(1))
    } else if sec < 86400 {
        format!("{}h", sec / 3600)
    } else if sec < 86400 * 30 {
        format!("{}d", sec / 86400)
    } else {
        format!("{}mo", sec / (86400 * 30))
    };
    format!("`{v}`")
}

pub fn rank_chip(n: i64) -> String {
    format!("`#{:03}`", n.max(0))
}

pub fn rel(ms: i64) -> String {
    let secs = if ms > 1_000_000_000_000 {
        ms / 1000
    } else {
        ms
    };
    format!("<t:{secs}:R>")
}

pub fn paginate<T>(list: &[T], per: usize, page: usize) -> (Vec<&T>, usize, usize) {
    let total = list.len().div_ceil(per.max(1)).max(1);
    let p = page.min(total - 1);
    let start = p * per;
    let items = list.iter().skip(start).take(per).collect();
    (items, p, total)
}
