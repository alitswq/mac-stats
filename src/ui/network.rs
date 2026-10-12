use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::format::{format_bytes, rate_text};
use crate::model::Snapshot;

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
        let lines = vec![
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
        let header = lines.len() as u16;
        let header = if inner.height > header + 6 {
            header
        } else {
            inner.height.saturating_sub(6)
        };
        let (text, chart) = split_length(inner, header);
        render_lines(frame, text, lines);
        if chart.height > 0 {
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
