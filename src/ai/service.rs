use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{AiSnapshot, ProviderUsage, claude_usage_path, collect_codex, now_unix, read_claude};

/// Collectors run independently of the terminal's one-second system sampling.
pub struct AiMonitor {
    latest: AiSnapshot,
    updates: Receiver<AiSnapshot>,
    cancel: Arc<AtomicBool>,
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl AiMonitor {
    pub fn start() -> io::Result<Self> {
        Self::start_with(claude_usage_path()?, collect_codex)
    }

    fn start_with(
        path: PathBuf,
        mut codex: impl FnMut(&AtomicBool) -> ProviderUsage + Send + 'static,
    ) -> io::Result<Self> {
        let (sender, updates) = mpsc::sync_channel(1);
        let (stop, stopping) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let canceled = Arc::clone(&cancel);
        let latest = AiSnapshot {
            codex: ProviderUsage {
                message: Some("Reading Codex account allowance...".into()),
                ..Default::default()
            },
            claude: ProviderUsage {
                message: Some("Reading Claude Code usage feed...".into()),
                ..Default::default()
            },
        };
        let mut snapshot = latest.clone();
        let worker = thread::Builder::new()
            .name("ai-usage-collector".into())
            .spawn(move || {
                let mut last_codex = None;
                loop {
                    if canceled.load(Ordering::Relaxed) {
                        break;
                    }
                    merge(&mut snapshot.claude, read_claude(&path, now_unix()));
                    let _ = sender.try_send(snapshot.clone());
                    if last_codex
                        .is_none_or(|last: Instant| last.elapsed() >= Duration::from_secs(60))
                    {
                        merge(&mut snapshot.codex, codex(&canceled));
                        last_codex = Some(Instant::now());
                    }
                    let _ = sender.try_send(snapshot.clone());
                    if stopping.recv_timeout(Duration::from_secs(2)).is_ok() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            latest,
            updates,
            cancel,
            stop,
            worker: Some(worker),
        })
    }

    pub fn snapshot(&mut self) -> AiSnapshot {
        for update in self.updates.try_iter() {
            self.latest = update;
        }
        self.latest.clone()
    }
}

impl Drop for AiMonitor {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            // Filesystem stalls must not delay terminal restoration indefinitely.
            // Ordinary collectors observe cancellation and finish before this deadline.
            let deadline = Instant::now() + Duration::from_millis(200);
            while !worker.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(2));
            }
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

/// A bounded read-only check for use without opening the interactive monitor.
pub fn collect_once() -> AiSnapshot {
    let claude = claude_usage_path()
        .map(|path| read_claude(&path, now_unix()))
        .unwrap_or_else(|_| ProviderUsage {
            message: Some("Claude usage path unavailable; set MAC_STATS_CLAUDE_USAGE_FILE.".into()),
            ..Default::default()
        });
    AiSnapshot {
        codex: collect_codex(&AtomicBool::new(false)),
        claude,
    }
}

fn merge(current: &mut ProviderUsage, update: ProviderUsage) {
    // A failed refresh changes the status, never the last successful observation.
    if update.observed_at.is_some() {
        *current = update;
    } else {
        current.message = update.message;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::QuotaWindow;

    #[test]
    fn failed_refreshes_retain_the_last_successful_reading_and_age() {
        let mut usage = ProviderUsage {
            observed_at: Some(1000),
            windows: vec![QuotaWindow {
                label: "5h".into(),
                used_percent: 25.0,
                resets_at: Some(4000),
            }],
            ..Default::default()
        };
        merge(
            &mut usage,
            ProviderUsage {
                message: Some("Refresh failed".into()),
                ..Default::default()
            },
        );
        assert_eq!(usage.observed_at, Some(1000));
        assert_eq!(usage.windows[0].remaining_percent(1400), Some(75.0));
        assert!(usage.is_stale(1400));
        assert_eq!(usage.message.as_deref(), Some("Refresh failed"));
    }

    #[test]
    fn background_collection_keeps_snapshot_reads_nonblocking_and_cancels_on_drop() {
        let entered = Arc::new(AtomicBool::new(false));
        let started = Arc::clone(&entered);
        let start = Instant::now();
        let mut monitor = AiMonitor::start_with(
            PathBuf::from("/tmp/mac-stats-no-claude-test-feed"),
            move |cancel| {
                started.store(true, Ordering::Relaxed);
                while !cancel.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(2));
                }
                ProviderUsage::default()
            },
        )
        .unwrap();
        while !entered.load(Ordering::Relaxed) && start.elapsed() < Duration::from_secs(1) {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(entered.load(Ordering::Relaxed));
        assert!(monitor.snapshot().codex.windows.is_empty());
        drop(monitor);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn shutdown_does_not_wait_indefinitely_for_an_unresponsive_collector() {
        let entered = Arc::new(AtomicBool::new(false));
        let started = Arc::clone(&entered);
        let monitor = AiMonitor::start_with(
            PathBuf::from("/tmp/mac-stats-no-claude-test-feed"),
            move |_| {
                started.store(true, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(800));
                ProviderUsage::default()
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !entered.load(Ordering::Relaxed) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(entered.load(Ordering::Relaxed));
        let start = Instant::now();
        drop(monitor);
        assert!(
            start.elapsed() < Duration::from_millis(400),
            "shutdown blocked for {:?}",
            start.elapsed()
        );
    }
}
