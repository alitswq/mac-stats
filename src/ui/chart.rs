use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::format::format_rate;

/// Solid 2×2 blocks. The index is a bitmask: upper-left, upper-right, lower-left, lower-right.
const QUADRANTS: [&str; 16] = [
    " ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█",
];

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
    let columns = area.width as usize;
    let rows = area.height as usize;
    if columns == 0 || rows == 0 || samples.is_empty() || max == 0 {
        return;
    }
    let mut mask = vec![0u8; columns * rows];
    let pixels_x = columns * 2;
    let pixels_y = rows * 2;
    for pixel_x in 0..pixels_x {
        let value = sample_at(samples, pixel_x, pixels_x);
        let filled =
            ((value as u128) * (pixels_y as u128) / (max as u128)).min(pixels_y as u128) as usize;
        for step in 0..filled {
            let pixel_y = match direction {
                Direction::Up => pixels_y - 1 - step,
                Direction::Down => step,
            };
            let cell = (pixel_y / 2) * columns + (pixel_x / 2);
            mask[cell] |= quadrant_bit(pixel_x % 2, pixel_y % 2);
        }
    }
    let style = Style::new().fg(color);
    for (index, bits) in mask.into_iter().enumerate() {
        if bits == 0 {
            continue;
        }
        let x = area.x + (index % columns) as u16;
        let y = area.y + (index / columns) as u16;
        buf[(x, y)]
            .set_symbol(QUADRANTS[bits as usize])
            .set_style(style);
    }
}

fn quadrant_bit(pixel_x: usize, pixel_y: usize) -> u8 {
    match (pixel_x, pixel_y) {
        (0, 0) => 0b0001,
        (1, 0) => 0b0010,
        (0, 1) => 0b0100,
        _ => 0b1000,
    }
}

/// Linear sample across the plot. Pixel 0 is the oldest sample and the last pixel is the newest.
fn sample_at(samples: &[u64], pixel: usize, pixels: usize) -> u64 {
    let last = samples.len() - 1;
    if last == 0 || pixels <= 1 {
        return samples[last];
    }
    let span = (pixels - 1) as u128;
    let numer = pixel as u128 * last as u128;
    let index = (numer / span) as usize;
    let toward = numer % span;
    let left = u128::from(samples[index]);
    let right = u128::from(samples[(index + 1).min(last)]);
    let value = if right >= left {
        left + (right - left) * toward / span
    } else {
        left - (left - right) * toward / span
    };
    value as u64
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
    fn samples_are_interpolated_from_oldest_to_newest() {
        assert_eq!(sample_at(&[40], 3, 8), 40);
        assert_eq!(sample_at(&[0, 100], 0, 5), 0);
        assert_eq!(sample_at(&[0, 100], 2, 5), 50);
        assert_eq!(sample_at(&[0, 100], 4, 5), 100);
    }

    #[test]
    fn a_ramp_rises_across_the_plot_instead_of_forming_two_blocks() {
        let area = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(area);
        Widget::render(AreaChart::percent(&[0, 100], Color::White), area, &mut buf);
        let ink = |x| {
            (0..area.height)
                .filter(|y| buf[(x, *y)].symbol() != " ")
                .count()
        };
        assert!(ink(5) < ink(12));
        assert!(ink(12) < ink(19));
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
