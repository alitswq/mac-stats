use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::format::{fit, format_bytes, rate_text};
use crate::model::{NetworkPeaks, RatePeaks, Snapshot};

use super::chart::SplitChart;
use super::{APP, COMPRESSED, GREEN, RED, card, render_lines, split_length};

pub fn draw(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "NETWORK", |frame, inner| {
        let network = &snapshot.network;
        if network.interface_name.is_empty() {
            frame.render_widget(Paragraph::new("No active interface."), inner);
            return;
        }
        let (status_text, status_style) = if network.is_up {
            ("● UP", Style::new().fg(GREEN).bold())
        } else {
            ("○ DOWN", Style::new().fg(RED).bold())
        };
        let ip = network
            .local_ip
            .clone()
            .unwrap_or_else(|| "no address".to_string());
        let mac = network.mac.clone().unwrap_or_else(|| "—".to_string());
        let mut lines = vec![
            Line::from(vec![
                Span::styled("▼ ", Style::new().fg(APP)),
                Span::styled(
                    format!(
                        "down {}",
                        rate_text(network.down_bytes_per_sec, snapshot.rates_ready)
                    ),
                    Style::new().fg(APP).bold(),
                ),
                Span::raw("    "),
                Span::styled("▲ ", Style::new().fg(COMPRESSED)),
                Span::styled(
                    format!(
                        "up {}",
                        rate_text(network.up_bytes_per_sec, snapshot.rates_ready)
                    ),
                    Style::new().fg(COMPRESSED).bold(),
                ),
            ]),
            Line::from(vec![
                Span::styled(network.interface_name.clone(), Style::new().bold()),
                Span::raw("   "),
                Span::styled(status_text, status_style),
                Span::raw("   "),
                Span::styled(ip, Style::new().bold()),
                Span::raw("   "),
                Span::styled(mac, Style::new().dim()),
            ]),
            Line::from(vec![
                Span::styled("since launch   ", Style::new().dim()),
                Span::styled(
                    format!("down {}", format_bytes(network.down_total_bytes)),
                    Style::new().fg(APP).bold(),
                ),
                Span::raw("   "),
                Span::styled(
                    format!("up {}", format_bytes(network.up_total_bytes)),
                    Style::new().fg(COMPRESSED).bold(),
                ),
            ]),
        ];
        lines.extend(peak_lines(
            &network.peaks,
            inner.width,
            snapshot.rates_ready,
        ));
        let header = lines.len() as u16;
        let (text, chart) = split_length(inner, header);
        render_lines(frame, text, lines);
        if chart.height >= 3 {
            frame.render_widget(
                SplitChart::throughput(
                    &snapshot.net_up_history,
                    &snapshot.net_down_history,
                    snapshot.network.rate_scale,
                    COMPRESSED,
                    APP,
                ),
                chart,
            );
        }
    });
}

/// Compact columns keep both directions and all three windows visible together.
pub(crate) fn peak_lines(
    peaks: &NetworkPeaks,
    width: u16,
    rates_ready: bool,
) -> Vec<Line<'static>> {
    const LABEL_WIDTH: usize = 8;
    let column_width = (width as usize).saturating_sub(LABEL_WIDTH) / 3;
    let mut header = vec![Span::styled(
        format!("{:<LABEL_WIDTH$}", "Peaks"),
        Style::new().bold(),
    )];
    header.extend(
        ["1m", "1h", "24h"]
            .map(|label| Span::styled(format!("{label:<column_width$}"), Style::new().dim())),
    );
    let row = |label: &str, rates: &RatePeaks, color| {
        let mut spans = vec![Span::styled(
            format!("{label:<LABEL_WIDTH$}"),
            Style::new().fg(color),
        )];
        spans.extend([rates.minute, rates.hour, rates.day].map(|rate| {
            let value = fit(&rate_text(rate, rates_ready), column_width);
            Span::styled(
                format!("{value:<column_width$}"),
                Style::new().fg(color).bold(),
            )
        }));
        Line::from(spans)
    };
    let mut lines = vec![
        Line::from(header),
        row("▼ down", &peaks.download, APP),
        row("▲ up", &peaks.upload, COMPRESSED),
    ];
    if peaks.observed_for.as_secs() < 86400 {
        let seconds = peaks.observed_for.as_secs();
        let observed = if seconds < 60 {
            format!("{seconds}s")
        } else if seconds < 3600 {
            format!("{}m", seconds / 60)
        } else {
            format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
        };
        lines.push(Line::from(format!("Observed {observed}; windows filling")));
    }
    lines
}
