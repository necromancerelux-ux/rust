//! Discord Components V2 helpers — Rust mirror of the JS `lookup/ui/v2.js`.
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
//!
//! Everything lives inside **one** [`CreateContainer`] so the left accent bar
//! spans the whole message. Messages must be sent with
//! [`MessageFlags::IS_COMPONENTS_V2`] (no top-level content/embeds).
//!
//! Requires Serenity 0.12.5+ with the `builder` feature (already enabled).

use poise::serenity_prelude as serenity;
use serenity::{
    ButtonStyle, Colour, CreateActionRow, CreateAttachment, CreateButton, CreateComponent,
    CreateContainer, CreateContainerComponent, CreateInteractionResponseMessage,
    CreateMediaGallery, CreateMediaGalleryItem, CreateSelectMenu, CreateSelectMenuKind,
    CreateSelectMenuOption, CreateSeparator, CreateTextDisplay, MessageFlags,
    SeparatorSpacingSize,
};

/// Light-blue accent bar used by the JS reference panels.
pub const ACCENT: u32 = 0x3B_A3_E0;
pub const BRAND: &str = "OmniCore";

// ---------------------------------------------------------------------------
// Container builder (mutable accumulator, same shape as the JS helpers)
// ---------------------------------------------------------------------------

/// Accumulates V2 components that will become a single accented container.
#[derive(Default)]
pub struct V2 {
    parts: Vec<CreateContainerComponent<'static>>,
}

impl V2 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Finish into a top-level container with the shared accent colour.
    pub fn into_container(self) -> CreateContainer<'static> {
        CreateContainer::new(self.parts).accent_color(Colour::new(ACCENT))
    }

    /// Finish as the single top-level component Discord expects.
    pub fn into_component(self) -> CreateComponent<'static> {
        CreateComponent::Container(self.into_container())
    }

    // ---- content ----------------------------------------------------------

    pub fn text(mut self, content: impl AsRef<str>) -> Self {
        let s: String = content.as_ref().chars().take(3900).collect();
        if !s.trim().is_empty() {
            self.parts
                .push(CreateContainerComponent::TextDisplay(CreateTextDisplay::new(s)));
        }
        self
    }

    pub fn title(self, t: impl AsRef<str>) -> Self {
        self.text(format!("## {}", t.as_ref()))
    }

    pub fn sep(mut self, divider: bool) -> Self {
        self.parts.push(CreateContainerComponent::Separator(
            CreateSeparator::new()
                .divider(divider)
                .spacing(SeparatorSpacingSize::Small),
        ));
        self
    }

    /// Image card from an attachment name (`attachment://name`).
    pub fn image(mut self, name: &str) -> Self {
        let url = format!("attachment://{name}");
        self.parts.push(CreateContainerComponent::MediaGallery(
            CreateMediaGallery::new(vec![CreateMediaGalleryItem::new(url)]),
        ));
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

    // ---- interactive rows -------------------------------------------------

    /// Up to 5 buttons in one action row.
    pub fn buttons(mut self, defs: &[ButtonDef]) -> Self {
        if defs.is_empty() {
            return self;
        }
        let mut row_btns = Vec::new();
        for d in defs.iter().take(5) {
            let mut b = if let Some(url) = d.url.as_ref() {
                CreateButton::new_link(url).label(d.label.chars().take(80).collect::<String>())
            } else {
                CreateButton::new(&d.id)
                    .label(d.label.chars().take(80).collect::<String>())
                    .style(d.style.unwrap_or(ButtonStyle::Secondary))
            };
            if d.disabled {
                b = b.disabled(true);
            }
            if let Some(emoji) = d.emoji.as_ref() {
                b = b.emoji(emoji.as_str());
            }
            row_btns.push(b);
        }
        self.parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::Buttons(row_btns),
        ));
        self
    }

    /// Tab strip. Active tab is Primary (blurple). Rows of `per_row` (default 4).
    pub fn tab_buttons(mut self, prefix: &str, tabs: &[Tab], active: &str, per_row: usize) -> Self {
        let per = per_row.max(1);
        for chunk in tabs.chunks(per) {
            let defs: Vec<ButtonDef> = chunk
                .iter()
                .map(|t| ButtonDef {
                    id: format!("{prefix}:{}", t.key),
                    label: t.label.clone(),
                    style: Some(if t.key == active {
                        ButtonStyle::Primary
                    } else {
                        ButtonStyle::Secondary
                    }),
                    ..Default::default()
                })
                .collect();
            self = self.buttons(&defs);
        }
        self
    }

    /// String select menu.
    pub fn select(mut self, id: &str, placeholder: &str, opts: &[SelectOpt]) -> Self {
        let mut seen = std::collections::HashSet::new();
        let options: Vec<CreateSelectMenuOption> = opts
            .iter()
            .filter_map(|o| {
                let value = o.value.chars().take(100).collect::<String>();
                let label = o.label.chars().take(100).collect::<String>();
                if value.is_empty() || label.trim().is_empty() || !seen.insert(value.clone()) {
                    return None;
                }
                let mut opt = CreateSelectMenuOption::new(label, value);
                if let Some(desc) = o.description.as_ref() {
                    opt = opt.description(desc.chars().take(100).collect::<String>());
                }
                if o.is_default {
                    opt = opt.default_selection(true);
                }
                Some(opt)
            })
            .take(25)
            .collect();
        if options.is_empty() {
            return self;
        }
        let menu = CreateSelectMenu::new(
            id,
            CreateSelectMenuKind::String { options },
        )
        .placeholder(placeholder.chars().take(150).collect::<String>());
        self.parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::SelectMenu(menu),
        ));
        self
    }

    /// « ‹ 1 / 4 › » pager. Centre button is a disabled page indicator.
    pub fn pager(self, prefix: &str, page: usize, total: usize) -> Self {
        let last = total.saturating_sub(1).max(0);
        let page = page.min(last);
        let mk = |tag: &str, target: usize, label: &str, disabled: bool| ButtonDef {
            id: format!("{prefix}:p:{tag}:{target}"),
            label: label.to_string(),
            style: Some(ButtonStyle::Secondary),
            disabled,
            ..Default::default()
        };
        self.buttons(&[
            mk("first", 0, "«", page == 0),
            mk("prev", page.saturating_sub(1), "‹", page == 0),
            ButtonDef {
                id: format!("{prefix}:noop"),
                label: format!("{} / {}", page + 1, total.max(1)),
                style: Some(ButtonStyle::Secondary),
                disabled: true,
                ..Default::default()
            },
            mk("next", (page + 1).min(last), "›", page >= last),
            mk("last", last, "»", page >= last),
        ])
    }
}

// ---------------------------------------------------------------------------
// Payload helpers
// ---------------------------------------------------------------------------

/// Build a reply payload with `IS_COMPONENTS_V2` (+ optional Ephemeral).
pub fn payload(
    v2: V2,
    files: Vec<CreateAttachment>,
    ephemeral: bool,
) -> CreateInteractionResponseMessage {
    let mut flags = MessageFlags::IS_COMPONENTS_V2;
    if ephemeral {
        flags |= MessageFlags::EPHEMERAL;
    }
    let mut msg = CreateInteractionResponseMessage::new()
        .flags(flags)
        .components(vec![v2.into_component()]);
    for f in files {
        msg = msg.add_file(f);
    }
    msg
}

/// For edit/update — keeps IS_COMPONENTS_V2; Ephemeral cannot be toggled later.
pub fn edit_payload(v2: V2, files: Vec<CreateAttachment>) -> CreateInteractionResponseMessage {
    let mut msg = CreateInteractionResponseMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![v2.into_component()]);
    for f in files {
        msg = msg.add_file(f);
    }
    msg
}

pub fn attachment(bytes: Vec<u8>, name: &str) -> CreateAttachment {
    CreateAttachment::bytes(bytes, name)
}

// ---------------------------------------------------------------------------
// Small data types
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct ButtonDef {
    pub id: String,
    pub label: String,
    pub style: Option<ButtonStyle>,
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
// Formatting helpers (shared by all Lookup panels)
// ---------------------------------------------------------------------------

/// Discord relative timestamp, e.g. `<t:1710000000:R>`.
pub fn rel(ms: i64) -> String {
    let secs = if ms > 1_000_000_000_000 {
        ms / 1000
    } else {
        ms
    };
    if secs <= 0 {
        return "just now".into();
    }
    format!("<t:{secs}:R>")
}

pub fn fmt(n: Option<i64>) -> String {
    let Some(n) = n else { return "—".into() };
    let a = n.unsigned_abs();
    let s = if n < 0 { "−" } else { "" };
    if a >= 1_000_000_000 {
        format!("{s}{:.2}B", a as f64 / 1e9)
    } else if a >= 1_000_000 {
        format!("{s}{:.2}M", a as f64 / 1e6)
    } else if a >= 1_000 {
        format!("{s}{:.1}K", a as f64 / 1e3)
    } else {
        format!("{s}{a}")
    }
}

pub fn signed(n: Option<i64>) -> String {
    let Some(n) = n else { return "—".into() };
    let sign = if n >= 0 { "+" } else { "−" };
    format!("{sign}{}", fmt(Some(n.abs())))
}

pub fn int(n: Option<i64>) -> String {
    match n {
        Some(n) => n.to_string(), // locale formatting can be added later
        None => "—".into(),
    }
}

/// Compact age chip like `` `4h` `` `` `1d` ``.
pub fn age_chip(ts_ms: Option<i64>) -> String {
    let Some(t) = ts_ms else { return String::new() };
    let secs = if t > 1_000_000_000_000 { t / 1000 } else { t };
    let now = chrono::Utc::now().timestamp();
    let age = (now - secs).max(0);
    let v = if age < 3600 {
        format!("{}m", (age / 60).max(1))
    } else if age < 86_400 {
        format!("{}h", age / 3600)
    } else if age < 86_400 * 30 {
        format!("{}d", age / 86_400)
    } else {
        format!("{}mo", age / (86_400 * 30))
    };
    format!("`{v}`")
}

/// `` `#001` `` style rank chip.
pub fn rank_chip(n: i64) -> String {
    format!("`#{n:03}`")
}

pub fn paginate<T: Clone>(list: &[T], per: usize, page: usize) -> (Vec<T>, usize, usize) {
    let per = per.max(1);
    let total = list.len().div_ceil(per).max(1);
    let p = page.min(total - 1);
    let start = p * per;
    let end = (start + per).min(list.len());
    (list[start..end].to_vec(), p, total)
}
