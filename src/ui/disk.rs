use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Gauge, Paragraph};

use crate::format::{format_bytes, rate_text};
use crate::model::Snapshot;

use super::chart::AreaChart;
use super::{APP, ProcessMetric, card, process_table, render_lines, split_length};

pub fn draw(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    let (volumes_area, table_area) = split_length_bottom(area, 11);
    if snapshot.volumes.is_empty() {
        card(frame, volumes_area, "DISK", |frame, inner| {
            frame.render_widget(Paragraph::new("No user volumes are mounted."), inner);
        });
    } else {
        let constraints = vec![Constraint::Fill(1); snapshot.volumes.len()];
        let areas = Layout::vertical(constraints).split(volumes_area);
        for (index, volume) in snapshot.volumes.iter().enumerate() {
            let Some(volume_area) = areas.get(index).copied() else {
                break;
            };
            card(frame, volume_area, "DISK", |frame, inner| {
                let used = volume.used_bytes();
                let ratio = if volume.total_bytes == 0 {
                    0.0
                } else {
                    used as f64 / volume.total_bytes as f64
                };
                let lines = vec![
                    Line::from(volume.name.clone()).bold(),
                    Line::from(format!(
                        "{} free of {}",
                        format_bytes(volume.available_bytes),
                        format_bytes(volume.total_bytes),
                    )),
                    Line::from(format!(
                        "read {}   write {}",
                        rate_text(volume.read_bytes_per_sec, snapshot.rates_ready),
                        rate_text(volume.write_bytes_per_sec, snapshot.rates_ready),
                    )),
                ];
                let (text, rest) = split_length(inner, 3);
                render_lines(frame, text, lines);
                if rest.height == 0 {
                    return;
                }
                let (gauge_area, chart) = split_length(rest, 1);
                frame.render_widget(
                    Gauge::default()
                        .ratio(ratio.clamp(0.0, 1.0))
                        .gauge_style(APP)
                        .use_unicode(true),
                    gauge_area,
                );
                if chart.height > 1 {
                    frame.render_widget(AreaChart::series(&volume.read_history, APP), chart);
                }
            });
        }
    }
    if table_area.height > 0 {
        frame.render_widget(
            process_table(
                &snapshot.top_disk,
                ProcessMetric::Disk,
                snapshot.rates_ready,
            ),
            table_area,
        );
    }
}

fn split_length_bottom(area: Rect, bottom: u16) -> (Rect, Rect) {
    if area.height <= bottom + 4 {
        (area, Rect::new(area.x, area.y + area.height, area.width, 0))
    } else {
        let [top, bottom_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(bottom)]).areas(area);
        (top, bottom_area)
    }
}
