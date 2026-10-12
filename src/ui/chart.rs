use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::format::format_rate;

/// "100%" plus a one-column gap. Percent labels never change width.
const PERCENT_GUTTER: u16 = 5;
/// Widest ordinary rate label ("999.9 MB/s") plus a one-column gap.
const RATE_GUTTER: u16 = 11;

struct Band {
    samples: Vec<u64>,
    color: Color,
}

/// Filled history. One column per sample, newest on the right.
///
/// `scale` is the axis ceiling. The painter never reads it from the samples,
/// so a bar's height does not change as the bar moves left.
pub struct AreaChart {
    bands: Vec<Band>,
    scale: u64,
    top_label: String,
    bottom_label: String,
    gutter: u16,
}

impl AreaChart {
    pub fn percent(samples: &[u64], color: Color) -> Self {
        Self {
            bands: vec![Band {
                samples: samples.to_vec(),
                color,
            }],
            scale: 100,
            top_label: "100%".to_string(),
            bottom_label: "0%".to_string(),
            gutter: PERCENT_GUTTER,
        }
    }

    /// Read and write (or any series) share one axis and grow up from it.
    /// The shorter series is drawn in front, so both stay visible.
    pub fn overlay(bands: &[(&[u64], Color)], scale: u64) -> Self {
        let scale = scale.max(1);
        let has_samples = bands.iter().any(|(samples, _)| !samples.is_empty());
        Self {
            bands: bands
                .iter()
                .map(|(samples, color)| Band {
                    samples: samples.to_vec(),
                    color: *color,
                })
                .collect(),
            scale,
            top_label: if has_samples {
                format_rate(scale)
            } else {
                "…".to_string()
            },
            bottom_label: "0".to_string(),
            gutter: RATE_GUTTER,
        }
    }
}

impl Widget for AreaChart {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let plot = layout_plot(
            area,
            buf,
            &self.top_label,
            &self.bottom_label,
            Style::new().dim(),
            Style::new().dim(),
            self.gutter,
        );
        paint_floor(buf, plot);
        paint_bands(buf, plot, &self.bands, self.scale, Direction::Up);
    }
}

/// Upload grows up from the center line. Download grows down from it.
pub struct SplitChart {
    upper: Vec<u64>,
    lower: Vec<u64>,
    scale: u64,
    upper_color: Color,
    lower_color: Color,
}

impl SplitChart {
    pub fn throughput(
        upload: &[u64],
        download: &[u64],
        scale: u64,
        upload_color: Color,
        download_color: Color,
    ) -> Self {
        Self {
            upper: upload.to_vec(),
            lower: download.to_vec(),
            scale: scale.max(1),
            upper_color: upload_color,
            lower_color: download_color,
        }
    }
}

impl Widget for SplitChart {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let has_samples = !self.upper.is_empty() || !self.lower.is_empty();
        let label = if has_samples {
            format_rate(self.scale)
        } else {
            "…".to_string()
        };
        let (upper, baseline, lower) = split_rows(area.height);
        // An even-height area leaves one spare row so both bands use equal scales.
        let chart_area = Rect {
            height: upper + baseline + lower,
            ..area
        };
        let plot = layout_plot(
            chart_area,
            buf,
            &label,
            &label,
            Style::new().fg(self.upper_color),
            Style::new().fg(self.lower_color),
            RATE_GUTTER,
        );
        let upper_area = Rect::new(plot.x, plot.y, plot.width, upper);
        let lower_area = Rect::new(plot.x, plot.y + upper + baseline, plot.width, lower);
        paint_bands(
            buf,
            upper_area,
            &[Band {
                samples: self.upper,
                color: self.upper_color,
            }],
            self.scale,
            Direction::Up,
        );
        paint_bands(
            buf,
            lower_area,
            &[Band {
                samples: self.lower,
                color: self.lower_color,
            }],
            self.scale,
            Direction::Down,
        );
        if baseline == 1 && plot.width > 0 {
            let y = plot.y + upper;
            write_label(buf, area.x, y, plot.x - area.x, "0", Style::new().dim());
            for x in plot.x..plot.x + plot.width {
                buf[(x, y)].set_symbol("─").set_style(Style::new().dim());
            }
        }
    }
}

fn layout_plot(
    area: Rect,
    buf: &mut Buffer,
    top_label: &str,
    bottom_label: &str,
    top_style: Style,
    bottom_style: Style,
    gutter_columns: u16,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::new(area.x, area.y, 0, 0);
    }
    let gutter = if gutter_columns > 0 && area.width > gutter_columns.saturating_add(8) {
        gutter_columns
    } else {
        0
    };
    if gutter > 0 {
        write_label(buf, area.x, area.y, gutter, top_label, top_style);
        if area.height > 1 {
            write_label(
                buf,
                area.x,
                area.y + area.height - 1,
                gutter,
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

fn paint_floor(buf: &mut Buffer, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let y = area.y + area.height - 1;
    for x in area.x..area.x + area.width {
        buf[(x, y)].set_symbol("─").set_style(Style::new().dim());
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Up,
    Down,
}

/// Columns are one sample wide and right-aligned. Height comes only from
/// `scale`, so the same sample paints the same cells at every x.
fn paint_bands(buf: &mut Buffer, area: Rect, bands: &[Band], scale: u64, direction: Direction) {
    let columns = area.width as usize;
    let rows = area.height as usize;
    if columns == 0 || rows == 0 || scale == 0 || bands.is_empty() {
        return;
    }
    let history_len = bands
        .iter()
        .map(|band| band.samples.len())
        .max()
        .unwrap_or(0);
    if history_len == 0 {
        return;
    }
    let visible = history_len.min(columns);
    let origin = columns - visible;
    let pixels_y = rows * 2;
    let mut pixels = vec![None; columns * pixels_y];

    for index in 0..visible {
        let column = origin + index;
        let from_right = visible - 1 - index;
        let fills: Vec<(usize, Color)> = bands
            .iter()
            .map(|band| {
                let value = sample_from_right(&band.samples, from_right);
                let filled = ((u128::from(value) * pixels_y as u128) / u128::from(scale))
                    .min(pixels_y as u128) as usize;
                (filled, band.color)
            })
            .collect();
        for from_bottom in 0..pixels_y {
            let mut front: Option<(usize, Color)> = None;
            for (filled, color) in fills.iter().copied() {
                if filled > from_bottom {
                    match front {
                        Some((best, _)) if best <= filled => {}
                        _ => front = Some((filled, color)),
                    }
                }
            }
            let Some((_, color)) = front else {
                continue;
            };
            let pixel_y = match direction {
                Direction::Up => pixels_y - 1 - from_bottom,
                Direction::Down => from_bottom,
            };
            pixels[pixel_y * columns + column] = Some(color);
        }
    }

    for row in 0..rows {
        for column in 0..columns {
            let top = pixels[(row * 2) * columns + column];
            let bottom = pixels[(row * 2 + 1) * columns + column];
            let Some((symbol, style)) = (match (top, bottom) {
                (None, None) => None,
                (Some(color), None) => Some(("▀", Style::new().fg(color))),
                (None, Some(color)) => Some(("▄", Style::new().fg(color))),
                (Some(color), Some(other)) if color == other => Some(("█", Style::new().fg(color))),
                (Some(top_color), Some(bottom_color)) => {
                    Some(("▀", Style::new().fg(top_color).bg(bottom_color)))
                }
            }) else {
                continue;
            };
            let x = area.x + column as u16;
            let y = area.y + row as u16;
            buf[(x, y)].set_symbol(symbol).set_style(style);
        }
    }
}

fn sample_from_right(samples: &[u64], from_right: usize) -> u64 {
    samples
        .len()
        .checked_sub(from_right + 1)
        .map(|index| samples[index])
        .unwrap_or(0)
}

/// Upper band, baseline, and lower band. The bands match, so one rate is the
/// same height above the line and below it.
fn split_rows(height: u16) -> (u16, u16, u16) {
    match height {
        0 => (0, 0, 0),
        1 => (1, 0, 0),
        2 => (1, 0, 1),
        _ => {
            let band = (height - 1) / 2;
            (band, 1, band)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(widget: impl Widget, area: Rect) -> Buffer {
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        buf
    }

    fn signature(buf: &Buffer, x: u16) -> Vec<(String, Color, Color)> {
        (0..buf.area.height)
            .map(|y| {
                let cell = &buf[(x, y)];
                (cell.symbol().to_string(), cell.fg, cell.bg)
            })
            .collect()
    }

    #[test]
    fn a_spike_stays_one_column_wide_on_the_right() {
        let mut samples = vec![0; 30];
        samples.push(100);
        let area = Rect::new(0, 0, 40, 12);
        let buf = render(AreaChart::percent(&samples, Color::White), area);
        let above_floor = |x| {
            (0..area.height.saturating_sub(1))
                .filter(|y| buf[(x, *y)].symbol() != " ")
                .count()
        };
        assert!(above_floor(39) > 1);
        assert_eq!(above_floor(37), 0);
        assert_eq!(above_floor(20), 0);
    }

    #[test]
    fn percent_chart_fills_its_height_at_the_right_edge() {
        let area = Rect::new(0, 0, 20, 10);
        let buf = render(AreaChart::percent(&[0, 100], Color::White), area);
        assert_eq!(buf[(19, 9)].symbol(), "█");
        assert_eq!(buf[(18, 8)].symbol(), " ");
        assert_eq!(buf[(18, 9)].symbol(), "─");
        assert_eq!(buf[(0, 0)].symbol(), "1");
        assert_eq!(buf[(0, 9)].symbol(), "0");
    }

    #[test]
    fn a_bar_keeps_its_height_when_it_scrolls_left() {
        let area = Rect::new(0, 0, 40, 10);
        let alone = render(
            AreaChart::overlay(&[(&[40], Color::Blue), (&[15], Color::Red)], 200),
            area,
        );
        let scrolled = render(
            AreaChart::overlay(
                &[(&[40, 5, 9], Color::Blue), (&[15, 4, 8], Color::Red)],
                200,
            ),
            area,
        );
        assert_eq!(signature(&alone, 39), signature(&scrolled, 37));
    }

    #[test]
    fn a_later_spike_does_not_resize_older_bars_when_the_scale_is_held() {
        let area = Rect::new(0, 0, 40, 10);
        let held = render(AreaChart::overlay(&[(&[40, 180], Color::Blue)], 200), area);
        let alone = render(AreaChart::overlay(&[(&[40], Color::Blue)], 200), area);
        assert_eq!(signature(&held, 38), signature(&alone, 39));
    }

    #[test]
    fn the_plot_edge_stays_put_when_the_scale_label_changes_width() {
        let area = Rect::new(0, 0, 40, 8);
        let narrow = render(AreaChart::overlay(&[(&[10_000], Color::White)], 500), area);
        let wide = render(
            AreaChart::overlay(&[(&[10_000], Color::White)], 20_000),
            area,
        );
        let right_ink = |buf: &Buffer| {
            (0..area.width).rev().find(|x| {
                (0..area.height).any(|y| matches!(buf[(*x, y)].symbol(), "▄" | "█" | "▀"))
            })
        };
        assert_eq!(right_ink(&narrow), right_ink(&wide));
        assert_ne!(right_ink(&narrow), None);
    }

    #[test]
    fn split_chart_draws_upload_above_the_baseline_and_download_below() {
        let area = Rect::new(0, 0, 24, 11);
        let buf = render(
            SplitChart::throughput(&[50], &[100], 100, Color::Red, Color::Blue),
            area,
        );
        let text: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(text.contains("100"));
        assert_eq!(buf[(23, 5)].symbol(), "─");
        assert_eq!(buf[(23, 6)].symbol(), "█");
        assert_eq!(buf[(22, 6)].symbol(), " ");
    }

    #[test]
    fn split_scale_labels_match_the_band_endpoints_and_center_zero() {
        for height in [10, 11, 20, 21] {
            let area = Rect::new(0, 0, 30, height);
            let buf = render(
                SplitChart::throughput(&[100], &[100], 100, Color::Red, Color::Blue),
                area,
            );
            let baseline = (0..height).find(|y| buf[(29, *y)].symbol() == "─").unwrap();
            let labels: Vec<_> = (0..height)
                .filter(|y| {
                    let line: String = (0..11).map(|x| buf[(x, *y)].symbol()).collect();
                    line.trim() == "100 B/s"
                })
                .collect();
            assert_eq!(labels.len(), 2);
            assert_eq!(
                baseline - labels[0],
                labels[1] - baseline,
                "unequal scale endpoints at height {height}"
            );
            assert_eq!(
                buf[(0, baseline)].symbol(),
                "0",
                "missing zero at height {height}"
            );
            assert_eq!(buf[(29, labels[0])].symbol(), "█");
            assert_eq!(buf[(29, labels[1])].symbol(), "█");
        }
    }

    #[test]
    fn the_same_rate_is_the_same_height_above_and_below() {
        let area = Rect::new(0, 0, 30, 10);
        let buf = render(
            SplitChart::throughput(&[50], &[50], 100, Color::Red, Color::Blue),
            area,
        );
        let column = area.width - 1;
        let baseline = (0..area.height)
            .find(|y| buf[(column, *y)].symbol() == "─")
            .expect("baseline");
        let blocks =
            |rows: std::ops::Range<u16>| rows.filter(|y| buf[(column, *y)].symbol() == "█").count();
        assert_eq!(blocks(0..baseline), blocks(baseline + 1..area.height));
        assert!(blocks(0..baseline) > 0);
    }

    #[test]
    fn a_split_bar_keeps_its_shape_as_it_scrolls() {
        let area = Rect::new(0, 0, 40, 11);
        let first = render(
            SplitChart::throughput(&[40], &[80], 100, Color::Red, Color::Blue),
            area,
        );
        let later = render(
            SplitChart::throughput(&[40, 10, 12], &[80, 20, 18], 100, Color::Red, Color::Blue),
            area,
        );
        assert_eq!(signature(&first, 39), signature(&later, 37));
    }
}
