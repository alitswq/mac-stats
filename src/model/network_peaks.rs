use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RatePeaks {
    pub minute: Option<u64>,
    pub hour: Option<u64>,
    pub day: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkPeaks {
    pub download: RatePeaks,
    pub upload: RatePeaks,
    pub observed_for: Duration,
}

/// Exact rolling maxima of observed rates. Monotonic queues discard samples that
/// can no longer become a peak, independently of the short graph histories.
pub struct NetworkPeakHistory {
    started_at: Option<Instant>,
    download: [RollingPeak; 3],
    upload: [RollingPeak; 3],
}

impl Default for NetworkPeakHistory {
    fn default() -> Self {
        let windows = [60, 3600, 86400];
        Self {
            started_at: None,
            download: windows.map(|seconds| RollingPeak::new(Duration::from_secs(seconds))),
            upload: windows.map(|seconds| RollingPeak::new(Duration::from_secs(seconds))),
        }
    }
}

impl NetworkPeakHistory {
    /// Calls must use monotonically increasing timestamps. Missing rates expire
    /// old readings without inventing a zero sample.
    pub fn observe(
        &mut self,
        now: Instant,
        download: Option<u64>,
        upload: Option<u64>,
    ) -> NetworkPeaks {
        if download.is_some() || upload.is_some() {
            self.started_at.get_or_insert(now);
        }
        let [minute, hour, day] = self
            .download
            .each_mut()
            .map(|peak| peak.observe(now, download));
        let download = RatePeaks { minute, hour, day };
        let [minute, hour, day] = self.upload.each_mut().map(|peak| peak.observe(now, upload));
        NetworkPeaks {
            download,
            upload: RatePeaks { minute, hour, day },
            observed_for: self
                .started_at
                .map(|start| now.saturating_duration_since(start))
                .unwrap_or_default(),
        }
    }
}

struct RollingPeak {
    window: Duration,
    candidates: VecDeque<(Instant, u64)>,
}

impl RollingPeak {
    fn new(window: Duration) -> Self {
        Self {
            window,
            candidates: VecDeque::new(),
        }
    }

    fn observe(&mut self, now: Instant, value: Option<u64>) -> Option<u64> {
        while self
            .candidates
            .front()
            .is_some_and(|(time, _)| now.saturating_duration_since(*time) >= self.window)
        {
            self.candidates.pop_front();
        }
        if let Some(value) = value {
            while self
                .candidates
                .back()
                .is_some_and(|(_, older)| *older <= value)
            {
                self.candidates.pop_back();
            }
            self.candidates.push_back((now, value));
        }
        self.candidates.front().map(|(_, peak)| *peak)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_expire_by_elapsed_time_independently_for_each_window_and_direction() {
        let start = Instant::now();
        let mut history = NetworkPeakHistory::default();
        history.observe(start, Some(900), Some(100));
        history.observe(start + Duration::from_secs(30), Some(300), Some(800));
        let peaks = history.observe(start + Duration::from_secs(61), Some(10), Some(20));
        assert_eq!(
            peaks.download,
            RatePeaks {
                minute: Some(300),
                hour: Some(900),
                day: Some(900)
            }
        );
        assert_eq!(
            peaks.upload,
            RatePeaks {
                minute: Some(800),
                hour: Some(800),
                day: Some(800)
            }
        );
        assert_eq!(peaks.observed_for, Duration::from_secs(61));
        for (seconds, expected) in [
            (
                3600,
                RatePeaks {
                    minute: None,
                    hour: Some(300),
                    day: Some(900),
                },
            ),
            (
                3630,
                RatePeaks {
                    minute: None,
                    hour: Some(10),
                    day: Some(900),
                },
            ),
            (
                86400,
                RatePeaks {
                    minute: None,
                    hour: None,
                    day: Some(300),
                },
            ),
            (
                86430,
                RatePeaks {
                    minute: None,
                    hour: None,
                    day: Some(10),
                },
            ),
            (86461, RatePeaks::default()),
        ] {
            let peaks = history.observe(start + Duration::from_secs(seconds), None, None);
            assert_eq!(peaks.download, expected, "at {seconds} seconds");
        }
    }

    #[test]
    fn missing_readings_stay_unavailable_and_zero_rates_are_valid() {
        let start = Instant::now();
        let mut history = NetworkPeakHistory::default();
        assert_eq!(history.observe(start, None, None), NetworkPeaks::default());
        let peaks = history.observe(start + Duration::from_secs(10), Some(0), None);
        assert_eq!(
            peaks.download,
            RatePeaks {
                minute: Some(0),
                hour: Some(0),
                day: Some(0)
            }
        );
        assert_eq!(peaks.upload, RatePeaks::default());
        let peaks = history.observe(start + Duration::from_secs(70), None, None);
        assert_eq!(
            peaks.download,
            RatePeaks {
                minute: None,
                hour: Some(0),
                day: Some(0)
            }
        );
        assert_eq!(peaks.observed_for, Duration::from_secs(60));
    }

    #[test]
    fn equal_peaks_keep_the_latest_timestamp() {
        let start = Instant::now();
        let mut history = NetworkPeakHistory::default();
        history.observe(start, Some(100), Some(200));
        history.observe(start + Duration::from_secs(30), Some(100), Some(200));
        let peaks = history.observe(start + Duration::from_secs(60), None, None);
        assert_eq!(peaks.download.minute, Some(100));
        assert_eq!(peaks.upload.minute, Some(200));
        let peaks = history.observe(start + Duration::from_secs(90), None, None);
        assert_eq!(peaks.download.minute, None);
        assert_eq!(peaks.upload.minute, None);
    }

    #[test]
    fn hour_and_day_peaks_outlive_the_graph_history() {
        let start = Instant::now();
        let mut history = NetworkPeakHistory::default();
        history.observe(start, Some(1000), Some(2000));
        for second in 1..=1200 {
            history.observe(start + Duration::from_secs(second), Some(1), Some(2));
        }
        let peaks = history.observe(start + Duration::from_secs(1200), None, None);
        assert_eq!(
            peaks.download,
            RatePeaks {
                minute: Some(1),
                hour: Some(1000),
                day: Some(1000)
            }
        );
        assert_eq!(
            peaks.upload,
            RatePeaks {
                minute: Some(2),
                hour: Some(2000),
                day: Some(2000)
            }
        );
        let mut reset = NetworkPeakHistory::default();
        let peaks = reset.observe(start + Duration::from_secs(1201), Some(3), Some(4));
        assert_eq!(peaks.download.day, Some(3));
        assert_eq!(peaks.upload.day, Some(4));
        assert_eq!(peaks.observed_for, Duration::ZERO);
    }
}
