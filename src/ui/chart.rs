use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::format::format_rate;

const LEVELS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// Filled history that uses the whole plot. New samples sit on the right.
pub struct AreaChart {
    samples: Vec<u64>,
    max: u64,
    color: Color,
    top_label: &'static str,
    bottom_label: &'static str,
}

impl AreaChart {
    pub fn percent(samples: &[u64], color: Color) -> Self {
        Self {
            samples: samples.to_vec(),
            max: 100,
            color,
            top_label: "100%",
            bottom_label: "0%",
        }
    }
}

impl Widget for AreaChart {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let plot = layout_plot(
            area,
            buf,
            self.top_label,
            self.bottom_label,
            Style::new().dim(),
            Style::new().dim(),
        );
        paint_series(
            buf,
            plot,
            &self.samples,
            self.max.max(1),
            self.color,
            Direction::Up,
        );
    }
}

/// Upload grows up from the center. Download grows down from it.
pub struct SplitChart {
    upper: Vec<u64>,
    lower: Vec<u64>,
    upper_color: Color,
    lower_color: Color,
}

impl SplitChart {
    pub fn throughput(
        upload: &[u64],
        download: &[u64],
        upload_color: Color,
        download_color: Color,
    ) -> Self {
        Self {
            upper: upload.to_vec(),
            lower: download.to_vec(),
            upper_color: upload_color,
            lower_color: download_color,
        }
    }
}

impl Widget for SplitChart {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let upper_label = peak_label(&self.upper);
        let lower_label = peak_label(&self.lower);
        let plot = layout_plot(
            area,
            buf,
            &upper_label,
            &lower_label,
            Style::new().fg(self.upper_color),
            Style::new().fg(self.lower_color),
        );
        let (upper, baseline, lower) = split_rows(plot.height);
        let upper_area = Rect::new(plot.x, plot.y, plot.width, upper);
        let lower_area = Rect::new(plot.x, plot.y + upper + baseline, plot.width, lower);
        let max = self
            .upper
            .iter()
            .chain(self.lower.iter())
            .copied()
            .max()
            .unwrap_or(1)
            .max(1);
        paint_series(
            buf,
            upper_area,
            &self.upper,
            max,
            self.upper_color,
            Direction::Up,
        );
        paint_series(
            buf,
            lower_area,
            &self.lower,
            max,
            self.lower_color,
            Direction::Down,
        );
        if baseline == 1 && plot.width > 0 {
            let y = plot.y + upper;
            for x in plot.x..plot.x + plot.width {
                buf[(x, y)].set_symbol("─").set_style(Style::new().dim());
            }
        }
    }
}

fn peak_label(samples: &[u64]) -> String {
    match samples.iter().copied().max() {
        Some(peak) => format_rate(peak),
        None => "…".to_string(),
    }
}

fn layout_plot(
    area: Rect,
    buf: &mut Buffer,
    top_label: &str,
    bottom_label: &str,
    top_style: Style,
    bottom_style: Style,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::new(area.x, area.y, 0, 0);
    }
    let label_width = top_label.chars().count().max(bottom_label.chars().count()) as u16;
    let gutter = if label_width > 0 && area.width > label_width + 8 {
        label_width + 1
    } else {
        0
    };
    if gutter > 0 {
        write_label(buf, area.x, area.y, area.width, top_label, top_style);
        if area.height > 1 {
            write_label(
                buf,
                area.x,
                area.y + area.height - 1,
                area.width,
                bottom_label,
                bottom_style,
            );
        }
    }
    Rect::new(area.x + gutter, area.y, area.width - gutter, area.height)
}

fn write_label(buf: &mut Buffer, x: u16, y: u16, width: u16, text: &str, style: Style) {
    let end = x.saturating_add(width);
    for (column, ch) in (x..end).zip(text.chars()) {
        buf[(column, y)]
            .set_symbol(&ch.to_string())
            .set_style(style);
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Up,
    Down,
}

fn paint_series(
    buf: &mut Buffer,
    area: Rect,
    samples: &[u64],
    max: u64,
    color: Color,
    direction: Direction,
) {
    if area.width == 0 || area.height == 0 || samples.is_empty() {
        return;
    }
    let count = samples.len().min(area.width as usize);
    let start = samples.len() - count;
    let spans = column_spans(count, area.width as usize);
    let mut x = area.x;
    for (sample, span) in samples[start..].iter().zip(spans) {
        let ticks = eighths(*sample, max, area.height);
        for _ in 0..span {
            paint_column(buf, x, area.y, area.height, ticks, color, direction);
            x += 1;
        }
    }
}

fn paint_column(
    buf: &mut Buffer,
    x: u16,
    top: u16,
    cells: u16,
    ticks: u64,
    color: Color,
    direction: Direction,
) {
    let style = Style::new().fg(color);
    let mut remaining = ticks;
    let paint = |buf: &mut Buffer, row: u16, level: u64| {
        let symbol = match direction {
            Direction::Up => LEVELS[level as usize],
            Direction::Down => down_symbol(level),
        };
        if level > 0 {
            buf[(x, top + row)].set_symbol(symbol).set_style(style);
        }
    };
    match direction {
        Direction::Up => {
            for row in (0..cells).rev() {
                let level = remaining.min(8);
                paint(buf, row, level);
                remaining = remaining.saturating_sub(8);
            }
        }
        Direction::Down => {
            for row in 0..cells {
                let level = remaining.min(8);
                paint(buf, row, level);
                remaining = remaining.saturating_sub(8);
            }
        }
    }
}

fn down_symbol(level: u64) -> &'static str {
    match level {
        0 => " ",
        1 | 2 => "▔",
        3..=7 => "▀",
        _ => "█",
    }
}

fn eighths(value: u64, max: u64, cells: u16) -> u64 {
    if value == 0 || max == 0 || cells == 0 {
        return 0;
    }
    let ticks = u128::from(cells) * 8;
    let scaled = u128::from(value) * ticks / u128::from(max);
    scaled.min(ticks) as u64
}

/// Widths that add up to `width`. The newest sample, on the right, takes any remainder.
fn column_spans(samples: usize, width: usize) -> Vec<usize> {
    if samples == 0 || width == 0 {
        return Vec::new();
    }
    let count = samples.min(width);
    let base = width / count;
    let extra = width % count;
    (0..count)
        .map(|index| {
            let from_right = count - 1 - index;
            if from_right < extra { base + 1 } else { base }
        })
        .collect()
}

fn split_rows(height: u16) -> (u16, u16, u16) {
    if height <= 1 {
        (height, 0, 0)
    } else if height == 2 {
        (1, 0, 1)
    } else {
        let upper = (height - 1) / 2;
        let lower = height - 1 - upper;
        (upper, 1, lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_spans_fill_the_width_with_the_newest_sample_on_the_right() {
        assert!(column_spans(0, 10).is_empty());
        assert_eq!(column_spans(10, 10), vec![1; 10]);
        let spans = column_spans(3, 10);
        assert_eq!(spans.iter().sum::<usize>(), 10);
        assert!(spans[2] >= spans[0]);
    }

    #[test]
    fn percent_chart_fills_its_height_at_the_right_edge() {
        let area = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(area);
        Widget::render(AreaChart::percent(&[0, 100], Color::White), area, &mut buf);
        assert_eq!(buf[(19, 9)].symbol(), "█");
        assert_eq!(buf[(0, 0)].symbol(), "1");
        assert_eq!(buf[(0, 9)].symbol(), "0");
    }

    #[test]
    fn split_chart_draws_upload_above_the_baseline_and_download_below() {
        let area = Rect::new(0, 0, 24, 11);
        let mut buf = Buffer::empty(area);
        Widget::render(
            SplitChart::throughput(&[50], &[100], Color::Red, Color::Blue),
            area,
            &mut buf,
        );
        let text: String = (0..area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("50"));
        assert_eq!(buf[(23, 5)].symbol(), "─");
        assert_eq!(buf[(23, 6)].symbol(), "█");
    }
}
