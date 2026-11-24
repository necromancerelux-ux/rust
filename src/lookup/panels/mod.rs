//! Components V2 Lookup panels (Player / Alliance / State).
//!
//! These replace the embed-based render path in `sessions::render` for a
//! pixel-matched UI against the JS bot (`src/lookup/components/*.js`).

pub mod alliance;
pub mod player;
pub mod state;

pub use alliance::{edit_alliance_v2, render_alliance_v2, ALLIANCE_TABS};
pub use player::{edit_player_v2, render_player_v2, PLAYER_TABS};
pub use state::{edit_state_v2, render_state_v2, STATE_TABS};
