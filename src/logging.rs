//! Logging setup.
//!
//! Pretty, low-noise logs by default; `RUST_LOG` wins when present.

use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

pub mod events;
pub use events::{entry, log};

pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("omnicore=info,serenity=warn,poise=info,lavalink_rs=warn")
    });

    let fmt_layer = fmt::layer()
        .with_ansi(true)
        .with_thread_names(false)
        .with_target(false);

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .try_init();
}

/// Print the startup banner. Kept deliberately small so it fits a hosting panel.
pub fn banner(version: &str, runtime: &str, music: bool, browser: bool) {
    println!();
    println!("  ╔══════════════════════════════════════════╗");
    println!("  ║   OmniCore {version:<28}║");
    println!("  ║   One bot. Every tool.                   ║");
    println!("  ╚══════════════════════════════════════════╝");
    println!(
        "  runtime={runtime}  music={}  browser={}",
        if music { "on" } else { "off" },
        if browser { "on" } else { "off" }
    );
    println!();
}
