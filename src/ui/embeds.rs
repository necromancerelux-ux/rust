//! Reusable embed builders.

use poise::serenity_prelude as serenity;
use serenity::{CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter};

use super::theme::{self, colors, icons};

/// The one place embeds are constructed. Every constructor stamps the footer.
pub fn base(color: u32) -> CreateEmbed {
    CreateEmbed::new()
        .color(color)
        .footer(CreateEmbedFooter::new(theme::footer()))
}

/// Branded embed used for all top-level panels.
pub fn brand(title: impl Into<String>) -> CreateEmbed {
    base(colors::BRAND).author(CreateEmbedAuthor::new(format!(
        "OmniCore • {}",
        title.into()
    )))
}

/// Settings-menu embed: title + description + brand colour + footer.
pub fn menu(title: impl Into<String>, description: impl Into<String>) -> CreateEmbed {
    serenity::CreateEmbed::new()
        .title(title)
        .description(description)
        .color(colors::BRAND)
        .footer(serenity::CreateEmbedFooter::new(theme::footer()))
}

pub fn success(title: impl Into<String>, body: impl Into<String>) -> CreateEmbed {
    base(colors::SUCCESS)
        .title(format!("{} {}", icons::CHECK, title.into()))
        .description(body)
}

pub fn error(title: impl Into<String>, body: impl Into<String>) -> CreateEmbed {
    base(colors::DANGER)
        .title(format!("{} {}", icons::CROSS, title.into()))
        .description(body)
}

pub fn warn(title: impl Into<String>, body: impl Into<String>) -> CreateEmbed {
    base(colors::WARNING)
        .title(format!("{} {}", icons::WARN, title.into()))
        .description(body)
}

pub fn info(title: impl Into<String>, body: impl Into<String>) -> CreateEmbed {
    base(colors::INFO)
        .title(format!("{} {}", icons::INFO, title.into()))
        .description(body)
}

/// The "you cannot do that" card.
pub fn denied(what: &str) -> CreateEmbed {
    base(colors::DANGER)
        .title(format!("{} Access denied", icons::LOCK))
        .description(format!("You need **{what}** to do that."))
}

/// A neutral "feature not enabled" card.
pub fn disabled(feature: &str, hint: &str) -> CreateEmbed {
    base(colors::MUTED)
        .title(format!("{} {feature} is disabled", icons::WARN))
        .description(hint)
}

/// Small ephemeral acknowledgement.
pub fn ok(body: impl Into<String>) -> CreateEmbed {
    base(colors::SUCCESS).description(format!("{} {}", icons::CHECK, body.into()))
}

/// Wrap a body in the standard divider sandwich.
pub fn section(title: &str, body: &str) -> String {
    format!(
        "{}\n**{}**\n{}\n{}",
        theme::DIVIDER,
        title,
        theme::DIVIDER,
        body
    )
}

/// Render a list of `(label, value)` rows as aligned embed lines.
pub fn rows(entries: &[(String, String)]) -> String {
    entries
        .iter()
        .map(|(k, v)| format!("**{k}** · {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Field for "last synced at".
pub fn synced_field(ts: Option<i64>) -> (String, String, bool) {
    (
        format!("{} Last sync", icons::CLOCK),
        ts.map(db_relative).unwrap_or_else(|| "never".to_string()),
        true,
    )
}

pub fn db_relative(ts: i64) -> String {
    format!("<t:{ts}:R>")
}
