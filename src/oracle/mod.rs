//! wosoracle.com integration: anonymous player/state/alliance data and the
//! gift-code feed. No login or session handling.

pub mod client;

pub use client::{enrichment_from_player, valid_code, OracleClient, OracleConfig, OracleError};
