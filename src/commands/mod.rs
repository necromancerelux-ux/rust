//! Slash commands and the component/modal router.
//!
//! `/settings` opens the button-panel hub. `/clear` and `/clearall` are the
//! moderation commands; everything else lives in the panels.
pub mod clear;
pub mod extras;
pub mod giveaways;
pub mod interactions;
pub mod lookup;
pub mod settings;

use crate::state::Data;

pub type Error = anyhow::Error;
pub type Context<'a> = poise::Context<'a, Data, Error>;

/// Every registered slash command.
pub fn all() -> Vec<poise::Command<Data, Error>> {
    vec![settings::settings(), clear::clear(), clear::clearall()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_public_commands_are_registered() {
        let names: Vec<String> = all()
            .into_iter()
            .map(|command| command.name.to_string())
            .collect();
        assert_eq!(names, ["settings", "clear", "clearall"]);
    }
}
