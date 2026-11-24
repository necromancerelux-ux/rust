//! Component helpers: consistent button shapes and row grouping.

use poise::serenity_prelude as serenity;
use serenity::{ButtonStyle, CreateActionRow, CreateButton, CreateInputText, InputTextStyle};

use super::theme::icons;

pub fn button(id: impl Into<String>, label: impl Into<String>, style: ButtonStyle) -> CreateButton {
    CreateButton::new(id).label(label).style(style)
}

pub fn primary(id: impl Into<String>, label: impl Into<String>) -> CreateButton {
    button(id, label, ButtonStyle::Primary)
}

pub fn secondary(id: impl Into<String>, label: impl Into<String>) -> CreateButton {
    button(id, label, ButtonStyle::Secondary)
}

pub fn success(id: impl Into<String>, label: impl Into<String>) -> CreateButton {
    button(id, label, ButtonStyle::Success)
}

pub fn danger(id: impl Into<String>, label: impl Into<String>) -> CreateButton {
    button(id, label, ButtonStyle::Danger)
}

pub fn link(label: impl Into<String>, url: impl Into<String>) -> CreateButton {
    CreateButton::new_link(url).label(label)
}

/// A single row of buttons.
pub fn row(buttons: Vec<CreateButton>) -> CreateActionRow {
    CreateActionRow::Buttons(buttons)
}

/// Split buttons into rows of at most five (max 5 rows), dropping empty rows.
pub fn rows(buttons: Vec<CreateButton>) -> Vec<CreateActionRow> {
    // Discord rejects more than 5 rows (25 buttons) per message.
    buttons
        .chunks(5)
        .take(5)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| CreateActionRow::Buttons(chunk.to_vec()))
        .collect()
}

/// The navigation row used by every paged panel.
pub fn pager(prefix: &str, _page: usize, has_prev: bool, has_next: bool) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        secondary(format!("{prefix}:prev"), format!("{} Prev", icons::BACK)).disabled(!has_prev),
        secondary(
            format!("{prefix}:refresh"),
            format!("{} Refresh", icons::REFRESH),
        ),
        secondary(format!("{prefix}:next"), format!("Next {}", icons::FORWARD)).disabled(!has_next),
    ])
}

/// A modal text input with the common defaults applied.
pub fn text_input(
    id: impl Into<String>,
    label: impl Into<String>,
    style: InputTextStyle,
) -> CreateInputText {
    CreateInputText::new(style, label, id).required(true)
}

pub fn short(id: impl Into<String>, label: impl Into<String>) -> CreateInputText {
    text_input(id, label, InputTextStyle::Short)
}

pub fn paragraph(id: impl Into<String>, label: impl Into<String>) -> CreateInputText {
    text_input(id, label, InputTextStyle::Paragraph)
}
