//! Provider-reported AI allowance and usage, independent of system sampling.

mod claude;
mod codex;
pub use claude::{claude_usage_path, export_claude, install_claude_feed, read_claude};
mod parse;
mod service;
pub use codex::collect_codex;
pub use parse::{claude as parse_claude, codex as parse_codex};
pub use service::{AiMonitor, collect_once};

use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const STALE_AFTER_SECONDS: u64 = 300;

#[derive(Clone, Debug, PartialEq)]
pub struct QuotaWindow {
    pub label: String,
    pub used_percent: f64,
    pub resets_at: Option<u64>,
}

impl QuotaWindow {
    /// An elapsed reset needs a new provider reading, not an assumed full quota.
    pub fn remaining_percent(&self, now: u64) -> Option<f64> {
        if !self.used_percent.is_finite()
            || !(0.0..=100.0).contains(&self.used_percent)
            || self.resets_at.is_some_and(|reset| reset <= now)
        {
            None
        } else {
            Some(100.0 - self.used_percent)
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenUsage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub lifetime: Option<u64>,
    pub estimated_session_cost: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProviderUsage {
    pub observed_at: Option<u64>,
    pub windows: Vec<QuotaWindow>,
    pub tokens: TokenUsage,
    pub model: Option<String>,
    pub message: Option<String>,
}

impl ProviderUsage {
    pub fn is_stale(&self, now: u64) -> bool {
        self.observed_at
            .is_some_and(|seen| now.saturating_sub(seen) > STALE_AFTER_SECONDS)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiSnapshot {
    pub codex: ProviderUsage,
    pub claude: ProviderUsage,
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_allowance_preserves_zero_and_waits_for_a_new_reading_after_reset() {
        let mut quota = QuotaWindow {
            label: "5h".into(),
            used_percent: 25.0,
            resets_at: Some(200),
        };
        assert_eq!(quota.remaining_percent(100), Some(75.0));
        quota.used_percent = 100.0;
        assert_eq!(quota.remaining_percent(100), Some(0.0));
        assert_eq!(quota.remaining_percent(200), None);
        quota.resets_at = None;
        quota.used_percent = 0.0;
        assert_eq!(quota.remaining_percent(100), Some(100.0));
    }

    #[test]
    fn freshness_uses_observation_time_instead_of_poll_time() {
        let usage = ProviderUsage {
            observed_at: Some(100),
            ..Default::default()
        };
        assert!(!usage.is_stale(400));
        assert!(usage.is_stale(401));
    }
}
