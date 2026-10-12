use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::format::{format_load, format_uptime, percent_text};
use crate::model::{CoreCluster, CoreStats, Snapshot};

use super::chart::AreaChart;
use super::{
    APP, ECORE, FREE, PCORE, ProcessMetric, RED, WIRED, YELLOW, card, mini_bar, process_table,
    render_lines, split_length,
};

pub fn draw(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "CPU", |frame, inner| {
        if !snapshot.cpu.available {
            frame.render_widget(Paragraph::new("CPU counters are unavailable."), inner);
            return;
        }
        let mut lines = vec![Line::from(vec![
            Span::styled(
                percent_text(snapshot.cpu.usage_percent),
                Style::new().bold(),
            ),
            Span::raw("   "),
            Span::styled("user ", Style::new().dim()),
            Span::styled(
                percent_text(snapshot.cpu.user_percent),
                Style::new().fg(APP).bold(),
            ),
            Span::raw("   "),
            Span::styled("system ", Style::new().dim()),
            Span::styled(
                percent_text(snapshot.cpu.system_percent),
                Style::new().fg(WIRED).bold(),
            ),
            Span::raw("   "),
            Span::styled("idle ", Style::new().dim()),
            Span::styled(
                percent_text(snapshot.cpu.idle_percent),
                Style::new().fg(FREE),
            ),
        ])];
        if snapshot.cpu.efficiency_percent.is_some() || snapshot.cpu.performance_percent.is_some() {
            lines.push(Line::from(vec![
                Span::styled("e-cores ", Style::new().dim()),
                Span::styled(
                    percent_text(snapshot.cpu.efficiency_percent),
                    Style::new().fg(ECORE).bold(),
                ),
                Span::raw("   "),
                Span::styled("p-cores ", Style::new().dim()),
                Span::styled(
                    percent_text(snapshot.cpu.performance_percent),
                    Style::new().fg(PCORE).bold(),
                ),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("load ", Style::new().dim()),
            Span::styled(format_load(snapshot.cpu.load_one), Style::new().bold()),
            Span::raw("  "),
            Span::styled(format_load(snapshot.cpu.load_five), Style::new().bold()),
            Span::raw("  "),
            Span::styled(format_load(snapshot.cpu.load_fifteen), Style::new().bold()),
            Span::raw("    "),
            Span::styled("up ", Style::new().dim()),
            Span::styled(
                format_uptime(snapshot.cpu.uptime_seconds),
                Style::new().bold(),
            ),
        ]));
        let text_height = lines.len() as u16;
        let (summary, rest) = split_length(inner, text_height);
        render_lines(frame, summary, lines);

        let (graph_height, core_height, _) =
            section_heights(rest.height, core_rows(rest.width, &snapshot.cpu.cores));
        let (chart, rest) = split_length(rest, graph_height);
        if chart.height > 0 {
            frame.render_widget(AreaChart::percent(&snapshot.cpu_history, APP), chart);
        }

        let (cores, table) = if core_height == 0 {
            (Rect::new(rest.x, rest.y, rest.width, 0), rest)
        } else {
            split_length(rest, core_height)
        };
        if cores.height > 0 {
            draw_cores(frame, cores, &snapshot.cpu.cores);
        }
        if table.height > 0 {
            frame.render_widget(
                process_table(&snapshot.top_cpu, ProcessMetric::Cpu, snapshot.rates_ready),
                table,
            );
        }
    });
}

/// History band, per-core strip, and process table. Extra rows go to the process list.
fn section_heights(height: u16, core_rows: u16) -> (u16, u16, u16) {
    const GRAPH: u16 = 10;
    const TABLE_MIN: u16 = 6;
    if height == 0 {
        return (0, 0, 0);
    }
    let cores = core_rows.min(height);
    if height >= GRAPH + cores + TABLE_MIN {
        return (GRAPH, cores, height - GRAPH - cores);
    }
    if height >= 6 + cores + TABLE_MIN {
        return (height - cores - TABLE_MIN, cores, TABLE_MIN);
    }
    // Keep every core visible when the terminal is too short for a process list.
    (height - cores, cores, 0)
}

// Include the widest label, percentage, three bar cells, and two internal spaces.
fn core_entry_width(cores: &[CoreStats]) -> usize {
    let label_width = cores.len().to_string().len().max(2) + 1;
    let percent_width = cores
        .iter()
        .map(|core| format!("{:>3.0}%", core.usage_percent).len())
        .max()
        .unwrap_or(4);
    label_width + percent_width + 5
}

fn core_columns(width: u16, cores: &[CoreStats]) -> usize {
    ((width as usize + 2) / (core_entry_width(cores) + 2)).max(1)
}

fn core_rows(width: u16, cores: &[CoreStats]) -> u16 {
    if cores.is_empty() {
        return 0;
    }
    cores
        .len()
        .div_ceil(core_columns(width, cores))
        .max(4)
        .min(u16::MAX as usize) as u16
}

fn draw_cores(frame: &mut Frame, area: Rect, cores: &[CoreStats]) {
    if cores.is_empty() || area.height == 0 || area.width == 0 {
        return;
    }
    let max_rows = area.height as usize;
    let num_cores = cores.len();
    let num_cols = num_cores
        .div_ceil(max_rows)
        .min(core_columns(area.width, cores))
        .max(1);
    let col_width = (area.width as usize).saturating_sub(2 * (num_cols - 1)) / num_cols;
    let label_digits = num_cores.to_string().len().max(2);
    let bar_space = col_width.saturating_sub(core_entry_width(cores) - 3);

    let mut lines = Vec::new();
    for row_idx in 0..max_rows {
        let mut row_spans = Vec::new();
        for col_idx in 0..num_cols {
            let core_idx = col_idx * max_rows + row_idx;
            if core_idx >= num_cores {
                continue;
            }
            let core = &cores[core_idx];
            let (tag, tag_color) = match core.cluster {
                CoreCluster::Efficiency => ("E", ECORE),
                CoreCluster::Performance => ("P", PCORE),
                CoreCluster::Unknown => ("C", APP),
            };
            let bar_color = if core.usage_percent >= 80.0 {
                RED
            } else if core.usage_percent >= 50.0 {
                YELLOW
            } else {
                tag_color
            };
            let bar_len = if bar_space >= 8 {
                8
            } else if bar_space >= 5 {
                5
            } else {
                bar_space.min(3)
            };
            let ratio = (core.usage_percent / 100.0).clamp(0.0, 1.0);
            let bar = mini_bar(bar_len, ratio, bar_color);

            if !row_spans.is_empty() {
                row_spans.push(Span::raw("  "));
            }
            row_spans.push(Span::styled(
                format!("{tag}{:<label_digits$}", core_idx + 1),
                Style::new().fg(tag_color).bold(),
            ));
            row_spans.push(Span::raw(" "));
            row_spans.extend(bar.spans);
            row_spans.push(Span::raw(" "));
            row_spans.push(Span::styled(
                format!("{:>3.0}%", core.usage_percent),
                Style::new().bold(),
            ));
        }
        if !row_spans.is_empty() {
            lines.push(Line::from(row_spans));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::section_heights;

    #[test]
    fn the_history_graph_keeps_the_space_above_the_core_and_process_strips() {
        assert_eq!(section_heights(32, 4), (10, 4, 18));
        assert_eq!(section_heights(17, 4), (7, 4, 6));
        assert_eq!(section_heights(12, 4), (8, 4, 0));
        assert_eq!(section_heights(10, 4), (6, 4, 0));
        assert_eq!(section_heights(24, 0), (10, 0, 14));
    }
}
