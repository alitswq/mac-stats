use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::format::{format_bytes, rate_text};
use crate::model::Snapshot;

use super::chart::AreaChart;
use super::{
    APP, COMPRESSED, ProcessMetric, RED, YELLOW, card, mini_bar, process_table, render_lines,
    split_length,
};

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
                let gauge_color = if ratio >= 0.90 {
                    RED
                } else if ratio >= 0.80 {
                    YELLOW
                } else {
                    APP
                };
                let lines = vec![
                    Line::from(vec![
                        Span::styled(volume.name.clone(), Style::new().bold()),
                        Span::raw(" "),
                        Span::styled(format!("({})", volume.mount_point), Style::new().dim()),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "{} free of {}",
                                format_bytes(volume.available_bytes),
                                format_bytes(volume.total_bytes)
                            ),
                            Style::new().bold(),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            format!("{:.0}% used", ratio * 100.0),
                            Style::new().fg(gauge_color).bold(),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("read ", Style::new().dim()),
                        Span::styled(
                            rate_text(volume.read_bytes_per_sec, snapshot.rates_ready),
                            Style::new().fg(APP).bold(),
                        ),
                        Span::raw("   "),
                        Span::styled("write ", Style::new().dim()),
                        Span::styled(
                            rate_text(volume.write_bytes_per_sec, snapshot.rates_ready),
                            Style::new().fg(COMPRESSED).bold(),
                        ),
                    ]),
                ];
                let (text, rest) = split_length(inner, 3);
                render_lines(frame, text, lines);
                if rest.height == 0 {
                    return;
                }
                let (gauge_area, chart) = split_length(rest, 1);
                let mut capacity_spans = vec![Span::raw("0% [")];
                capacity_spans.extend(
                    mini_bar(
                        gauge_area.width.saturating_sub(10) as usize,
                        ratio,
                        gauge_color,
                    )
                    .spans,
                );
                capacity_spans.push(Span::raw("] 100%"));
                frame.render_widget(Paragraph::new(Line::from(capacity_spans)), gauge_area);
                if chart.height > 0 {
                    frame.render_widget(
                        AreaChart::overlay(
                            &[
                                (&volume.read_history, APP),
                                (&volume.write_history, COMPRESSED),
                            ],
                            volume.rate_scale,
                        ),
                        chart,
                    );
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

#[cfg(test)]
mod tests {
    use crate::model::{View, example_snapshot};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn capacity_bars_show_the_full_scale_and_used_percentage() {
        for (width, filled_at_88_percent) in [(70, 51), (80, 60), (160, 130)] {
            for (total, free, percent) in
                [(1000, 1000, 0), (1000, 120, 88), (1000, 0, 100), (0, 0, 0)]
            {
                let mut snapshot = example_snapshot();
                snapshot.volumes[0].total_bytes = total;
                snapshot.volumes[0].available_bytes = free;
                snapshot.volumes.push(snapshot.volumes[0].clone());
                let mut terminal = Terminal::new(TestBackend::new(width, 36)).unwrap();
                terminal
                    .draw(|frame| crate::ui::draw(frame, &snapshot, View::Disk))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let rows: Vec<String> = (0..buffer.area.height)
                    .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
                    .collect();
                let text = rows.join("\n");
                assert_eq!(
                    text.matches(&format!("{percent}% used")).count(),
                    2,
                    "{text}"
                );
                let bars: Vec<_> = rows.iter().filter(|row| row.contains("0% [")).collect();
                assert_eq!(
                    bars.len(),
                    2,
                    "missing capacity scale at width {width}:\n{text}"
                );
                for row in bars {
                    assert!(
                        row.contains("] 100%"),
                        "missing full-capacity endpoint: {row}"
                    );
                    let track = row.split_once('[').unwrap().1.split_once(']').unwrap().0;
                    assert_eq!(track.chars().count(), (width - 12) as usize);
                    let expected_filled = match percent {
                        0 => 0,
                        88 => filled_at_88_percent,
                        100 => (width - 12) as usize,
                        _ => unreachable!(),
                    };
                    assert_eq!(track.matches('█').count(), expected_filled, "{row}");
                    assert_eq!(
                        track.matches('░').count(),
                        (width - 12) as usize - expected_filled,
                        "{row}"
                    );
                }
            }
        }
    }
}
