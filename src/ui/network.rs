use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::format::{format_bytes, rate_text};
use crate::model::Snapshot;

use super::{APP, COMPRESSED, card, render_lines, spark, split_length};

pub fn draw(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "NETWORK", |frame, inner| {
        let network = &snapshot.network;
        if network.interface_name.is_empty() {
            frame.render_widget(Paragraph::new("No active interface."), inner);
            return;
        }
        let status = if network.is_up { "UP" } else { "DOWN" };
        let ip = network
            .local_ip
            .clone()
            .unwrap_or_else(|| "no address".to_string());
        let mac = network.mac.clone().unwrap_or_else(|| "—".to_string());
        let lines = vec![
            Line::from(format!(
                "down {}",
                rate_text(network.down_bytes_per_sec, snapshot.rates_ready)
            ))
            .bold(),
            Line::from(format!(
                "up {}",
                rate_text(network.up_bytes_per_sec, snapshot.rates_ready)
            )),
            Line::from(format!("{}   {}", network.interface_name, status)),
            Line::from(ip),
            Line::from(mac),
            Line::from(format!(
                "since launch   down {}   up {}",
                format_bytes(network.down_total_bytes),
                format_bytes(network.up_total_bytes),
            )),
        ];
        let (text, charts) = split_length(inner, 6.min(inner.height.saturating_sub(2)));
        render_lines(frame, text, lines);
        if charts.height < 2 {
            return;
        }
        let [down, up] = ratatui::layout::Layout::vertical([
            ratatui::layout::Constraint::Fill(1),
            ratatui::layout::Constraint::Fill(1),
        ])
        .areas(charts);
        frame.render_widget(spark(&snapshot.net_down_history, None, APP), down);
        frame.render_widget(spark(&snapshot.net_up_history, None, COMPRESSED), up);
    });
}
