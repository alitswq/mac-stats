use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::format::{format_bytes, format_percent};
use crate::model::Snapshot;

use super::chart::AreaChart;
use super::{
    APP, COMPRESSED, FREE, ProcessMetric, RED, WIRED, YELLOW, card, legend, process_table,
    render_lines, split_length, stacked_bar,
};

pub fn draw(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "RAM", |frame, inner| {
        let memory = &snapshot.memory;
        if !memory.available {
            frame.render_widget(Paragraph::new("Memory counters are unavailable."), inner);
            return;
        }

        let swap_style = if memory.swap_used_bytes > 1024 * 1024 * 1024 {
            Style::new().fg(RED).bold()
        } else if memory.swap_used_bytes > 0 {
            Style::new().fg(YELLOW).bold()
        } else {
            Style::new().bold()
        };

        let swap_text = if memory.swap_total_bytes > 0 {
            format!(
                "Swap {} of {}",
                format_bytes(memory.swap_used_bytes),
                format_bytes(memory.swap_total_bytes)
            )
        } else {
            format!("Swap {}", format_bytes(memory.swap_used_bytes))
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(format_bytes(memory.used_bytes), Style::new().bold()),
                Span::raw(" used of "),
                Span::styled(format_bytes(memory.total_bytes), Style::new().bold()),
                Span::raw("   "),
                Span::styled(
                    format_percent(memory.used_percent()),
                    Style::new().fg(APP).bold(),
                ),
            ]),
            stacked_bar(
                inner.width,
                &[
                    (memory.app_bytes, APP),
                    (memory.wired_bytes, WIRED),
                    (memory.compressed_bytes, COMPRESSED),
                    (memory.free_bytes, FREE),
                ],
            ),
        ];
        lines.extend(legend(
            &[
                ("App", memory.app_bytes, APP),
                ("Wired", memory.wired_bytes, WIRED),
                ("Compressed", memory.compressed_bytes, COMPRESSED),
                ("Free", memory.free_bytes, FREE),
            ],
            inner.width,
        ));
        lines.push(Line::from(vec![Span::styled(swap_text, swap_style)]));
        let (summary, rest) = split_length(inner, lines.len() as u16);
        render_lines(frame, summary, lines);
        let chart_height = if rest.height >= 16 {
            8
        } else {
            rest.height / 3
        };
        let (chart, table) = split_length(rest, chart_height);
        if chart.height > 0 {
            frame.render_widget(AreaChart::percent(&snapshot.memory_history, APP), chart);
        }
        if table.height > 0 {
            frame.render_widget(
                process_table(
                    &snapshot.top_memory,
                    ProcessMetric::Memory,
                    snapshot.rates_ready,
                ),
                table,
            );
        }
    });
}
