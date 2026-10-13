//! Live CPU, memory, disk, and network stats for macOS.

#[cfg(not(target_os = "macos"))]
compile_error!("mac-stats reads macOS counters and only builds on macOS.");

/// Release version from Cargo.toml, embedded at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Release build counter from BUILD_NUMBER, embedded at compile time.
pub const BUILD_NUMBER: &str = env!("MAC_STATS_BUILD_NUMBER");
/// Developer credit from the package authors in Cargo.toml.
pub const DEVELOPER: &str = env!("CARGO_PKG_AUTHORS");
/// Shared release identity for the dashboard and command output.
pub const RELEASE: &str = concat!(
    env!("CARGO_PKG_NAME"),
    " v",
    env!("CARGO_PKG_VERSION"),
    " (build ",
    env!("MAC_STATS_BUILD_NUMBER"),
    ")"
);

pub mod ai;
mod collect;
mod format;
mod model;
mod ui;

pub use collect::Sampler;
pub use format::report;
pub use model::{Snapshot, View};
pub use ui::draw;
