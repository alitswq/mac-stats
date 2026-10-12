//! Live CPU, memory, disk, and network stats for macOS.

#[cfg(not(target_os = "macos"))]
compile_error!("mac-stats reads macOS counters and only builds on macOS.");

mod collect;
mod format;
mod model;
mod ui;

pub use collect::Sampler;
pub use format::report;
pub use model::{Snapshot, View};
pub use ui::draw;
