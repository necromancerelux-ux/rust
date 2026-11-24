//! Lookup subsystem: Whiteout Stats + WoS Oracle dual-source client,
//! private sessions, and Components V2 panels.
//!
//! Data flow:
//! 1. [`data::fetch_player`] races Whiteout Stats and Oracle in parallel.
//! 2. Sessions keep the normalised profile + raw payloads.
//! 3. Panels render with [`crate::ui::v2`] (same layout as the JS bot).

mod client;
pub mod data;
pub mod panels;
pub mod sessions;
pub mod troop_levels;
mod types;

pub use client::LookupClient;
pub use data::{fetch_player, PlayerBundle, Source as DataSource};
pub use types::PlayerProfile;
