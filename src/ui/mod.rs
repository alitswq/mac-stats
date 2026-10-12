mod chart;
mod cpu;
mod disk;
mod essentials;
mod memory;
mod network;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Sparkline, Table};

use crate::format::{fit, format_bytes, format_cpu_process, format_rate};
use crate::model::{ProcessRow, Snapshot, View, bar_widths};

const MINIMUM_WIDTH: u16 = 70;
const MINIMUM_HEIGHT: u16 = 20;

pub(crate) const APP: Color = Color::Rgb(78, 161, 245);
pub(crate) const WIRED: Color = Color::Rgb(232, 164, 74);
pub(crate) const COMPRESSED: Color = Color::Rgb(232, 96, 109);
pub(crate) const FREE: Color = Color::Rgb(128, 134, 142);
pub(crate) const ECORE: Color = Color::Rgb(78, 196, 196);
pub(crate) const PCORE: Color = Color::Rgb(150, 130, 232);
pub(crate) const OK: Color = Color::Rgb(112, 196, 136);

pub fn draw(frame: &mut Frame, snapshot: &Snapshot, view: View) {
    let area = frame.area();
    if area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT {
        frame.render_widget(
            Paragraph::new(format!(
                "Make the terminal at least {MINIMUM_WIDTH} columns by {MINIMUM_HEIGHT} rows."
            )),
            area,
        );
        return;
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    draw_header(frame, header, view);
    draw_footer(frame, footer);
    match view {
        View::Essentials => essentials::draw(frame, body, snapshot),
        View::Ram => memory::draw(frame, body, snapshot),
        View::Cpu => cpu::draw(frame, body, snapshot),
        View::Disk => disk::draw(frame, body, snapshot),
        View::Network => network::draw(frame, body, snapshot),
    }
}

fn draw_header(frame: &mut Frame, area: Rect, view: View) {
    let [title_row, tab_row] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    frame.render_widget(Paragraph::new(" mac-stats").bold(), title_row);
    let tabs = View::cycle().iter().flat_map(|candidate| {
        let active = *candidate == view;
        let style = if active {
            Style::new().fg(Color::Black).bg(Color::White).bold()
        } else {
            Style::new().add_modifier(Modifier::DIM)
        };
        [
            Span::styled(format!(" {} ", candidate.label()), style),
            Span::raw(" "),
        ]
    });
    frame.render_widget(
        Paragraph::new(Line::from(tabs.collect::<Vec<_>>())),
        tab_row,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(" 5 1-4 views    tab or arrows switch    q quit").dim(),
        area,
    );
}

pub(crate) fn card<F>(frame: &mut Frame, area: Rect, title: &str, draw_inner: F)
where
    F: FnOnce(&mut Frame, Rect),
{
    let block = Block::bordered()
        .border_style(Style::new().dim())
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(title, Style::new().bold()),
            Span::raw(" "),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width > 0 && inner.height > 0 {
        draw_inner(frame, inner);
    }
}

pub(crate) fn render_lines(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    frame.render_widget(Paragraph::new(lines), area);
}

pub(crate) fn spark(data: &[u64], max: Option<u64>, color: Color) -> Sparkline<'static> {
    let sparkline = Sparkline::default()
        .data(tail(data, 80))
        .style(Style::new().fg(color));
    match max {
        Some(max) => sparkline.max(max.max(1)),
        None => sparkline,
    }
}

fn tail(data: &[u64], width: usize) -> Vec<u64> {
    if data.len() <= width {
        data.to_vec()
    } else {
        data[data.len() - width..].to_vec()
    }
}

pub(crate) fn stacked_bar(width: u16, parts: &[(u64, Color)]) -> Line<'static> {
    let values: Vec<u64> = parts.iter().map(|(value, _)| *value).collect();
    let widths = bar_widths(width as usize, &values);
    let mut spans = Vec::new();
    for (index, (_, color)) in parts.iter().enumerate() {
        let cells = widths.get(index).copied().unwrap_or(0);
        if cells > 0 {
            spans.push(Span::styled("█".repeat(cells), Style::new().fg(*color)));
        }
    }
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    Line::from(spans)
}

pub(crate) fn legend(parts: &[(&str, u64, Color)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (label, bytes, color)) in parts.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled("■ ", Style::new().fg(*color)));
        spans.push(Span::raw(format!("{label} {}", format_bytes(*bytes))));
    }
    Line::from(spans)
}

pub(crate) enum ProcessMetric {
    Memory,
    Cpu,
    Disk,
}

pub(crate) fn process_table(
    processes: &[ProcessRow],
    metric: ProcessMetric,
    rates_ready: bool,
) -> Table<'static> {
    let (header, widths, rows) = match metric {
        ProcessMetric::Memory => (
            vec!["Process", "Memory"],
            vec![Constraint::Fill(1), Constraint::Length(12)],
            processes
                .iter()
                .map(|process| {
                    Row::new(vec![
                        Cell::from(fit(&process.name, 40)),
                        Cell::from(format_bytes(process.memory_bytes)),
                    ])
                })
                .collect::<Vec<_>>(),
        ),
        ProcessMetric::Cpu => (
            vec!["Process", "CPU"],
            vec![Constraint::Fill(1), Constraint::Length(10)],
            processes
                .iter()
                .map(|process| {
                    let value = if rates_ready {
                        format_cpu_process(process.cpu_percent)
                    } else {
                        "…".to_string()
                    };
                    Row::new(vec![Cell::from(fit(&process.name, 40)), Cell::from(value)])
                })
                .collect::<Vec<_>>(),
        ),
        ProcessMetric::Disk => (
            vec!["Process", "Read", "Write"],
            vec![
                Constraint::Fill(1),
                Constraint::Length(12),
                Constraint::Length(12),
            ],
            processes
                .iter()
                .map(|process| {
                    let (read, write) = if rates_ready {
                        (
                            format_rate(process.disk_read_bytes_per_sec),
                            format_rate(process.disk_write_bytes_per_sec),
                        )
                    } else {
                        ("…".to_string(), "…".to_string())
                    };
                    Row::new(vec![
                        Cell::from(fit(&process.name, 40)),
                        Cell::from(read).fg(APP),
                        Cell::from(write).fg(COMPRESSED),
                    ])
                })
                .collect::<Vec<_>>(),
        ),
    };
    Table::new(rows, widths)
        .header(Row::new(header).dim())
        .column_spacing(1)
}

pub(crate) fn split_length(area: Rect, length: u16) -> (Rect, Rect) {
    if area.height <= length {
        (area, Rect::new(area.x, area.y + area.height, area.width, 0))
    } else {
        let [top, bottom] =
            Layout::vertical([Constraint::Length(length), Constraint::Fill(1)]).areas(area);
        (top, bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::example_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn view_text(view: View, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let snapshot = example_snapshot();
        terminal.draw(|frame| draw(frame, &snapshot, view)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn each_view_renders_its_headline() {
        let essentials = view_text(View::Essentials, 100, 36);
        assert!(essentials.contains("5 Essentials"));
        assert!(essentials.contains("Macintosh HD"));
        assert!(essentials.contains("en13"));
        assert!(essentials.contains("18%"));

        let ram = view_text(View::Ram, 100, 36);
        assert!(ram.contains("Docker"));
        assert!(ram.contains("Swap"));

        let cpu = view_text(View::Cpu, 100, 36);
        assert!(cpu.contains("e-cores"));
        assert!(cpu.contains("Docker"));
        assert!(cpu.contains("3 hours, 2 minutes"));
        assert!(cpu.contains("100%"));
        assert!(cpu.contains('█'));

        let disk = view_text(View::Disk, 80, 24);
        assert!(disk.contains("Macintosh HD"));
        assert!(disk.contains("Docker"));

        let network = view_text(View::Network, 80, 24);
        assert!(network.contains("192.168.68.76"));
        assert!(network.contains("4c:c5:d9:d5:11:d3"));
        assert!(network.contains("UP"));
        assert!(network.contains('─'));
        assert!(network.contains('█'));
    }

    #[test]
    fn a_small_terminal_asks_for_more_room() {
        let text = view_text(View::Essentials, 40, 10);
        assert!(text.contains("at least 70 columns"));
    }
}
