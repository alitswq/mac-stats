use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::format::{fit, format_bytes, format_load, format_percent, percent_text, rate_text};
use crate::model::{Snapshot, VolumeStats};

use super::chart::{AreaChart, SplitChart};
use super::{
    APP, COMPRESSED, ECORE, FREE, GREEN, PCORE, RED, WIRED, card, render_lines, stacked_bar,
};

pub fn draw(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    let wide = area.width >= 88 && area.height >= 16;
    if wide {
        let [top, bottom] =
            Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(area);
        let [cpu_area, ram_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(top);
        let [disk_area, net_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(bottom);
        draw_cpu(frame, cpu_area, snapshot);
        draw_ram(frame, ram_area, snapshot);
        draw_disk(frame, disk_area, snapshot);
        draw_net(frame, net_area, snapshot);
    } else {
        let [cpu_area, ram_area, disk_area, net_area] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .areas(area);
        draw_cpu(frame, cpu_area, snapshot);
        draw_ram(frame, ram_area, snapshot);
        draw_disk(frame, disk_area, snapshot);
        draw_net(frame, net_area, snapshot);
    }
}

fn draw_cpu(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "CPU", |frame, inner| {
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
        ]));
        let text_rows = lines.len() as u16;
        let (text, chart) = super::split_length(inner, text_rows.min(inner.height));
        render_lines(frame, text, lines);
        if chart.height > 0 {
            frame.render_widget(AreaChart::percent(&snapshot.cpu_history, APP), chart);
        }
    });
}

fn draw_ram(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "RAM", |frame, inner| {
        let memory = &snapshot.memory;
        if !memory.available {
            render_lines(frame, inner, vec![Line::from("unavailable")]);
            return;
        }
        let lines = vec![
            Line::from(vec![
                Span::styled(format_bytes(memory.used_bytes), Style::new().bold()),
                Span::raw(" used  "),
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
            Line::from(vec![
                Span::styled("app ", Style::new().dim()),
                Span::styled(format_bytes(memory.app_bytes), Style::new().fg(APP).bold()),
                Span::raw("  "),
                Span::styled("wired ", Style::new().dim()),
                Span::styled(
                    format_bytes(memory.wired_bytes),
                    Style::new().fg(WIRED).bold(),
                ),
            ]),
            Line::from(vec![
                Span::styled("compressed ", Style::new().dim()),
                Span::styled(
                    format_bytes(memory.compressed_bytes),
                    Style::new().fg(COMPRESSED).bold(),
                ),
                Span::raw("  "),
                Span::styled("free ", Style::new().dim()),
                Span::styled(format_bytes(memory.free_bytes), Style::new().fg(FREE)),
            ]),
            Line::from(vec![
                Span::styled("swap ", Style::new().dim()),
                Span::styled(format_bytes(memory.swap_used_bytes), Style::new().bold()),
            ]),
        ];
        let text_rows = lines.len() as u16;
        let (text, chart) = super::split_length(inner, text_rows.min(inner.height));
        render_lines(frame, text, lines);
        if chart.height > 0 {
            frame.render_widget(AreaChart::percent(&snapshot.memory_history, APP), chart);
        }
    });
}

fn draw_disk(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "DISK", |frame, inner| {
        if snapshot.volumes.is_empty() {
            render_lines(frame, inner, vec![Line::from("no user volumes")]);
            return;
        }
        // Give each volume at least one row; reserve a row for any hidden volumes.
        let overflow = snapshot.volumes.len() > inner.height as usize;
        let (volume_area, overflow_area) =
            super::split_length(inner, inner.height.saturating_sub(u16::from(overflow)));
        let visible = snapshot.volumes.len().min(volume_area.height as usize);
        if visible > 0 {
            let areas = Layout::vertical(vec![Constraint::Fill(1); visible]).split(volume_area);
            for (volume, area) in snapshot.volumes.iter().zip(areas.iter()).take(visible) {
                draw_disk_volume(frame, *area, volume, snapshot.rates_ready);
            }
        }
        if overflow {
            render_lines(
                frame,
                overflow_area,
                vec![Line::from(format!(
                    "+{} more volumes (3 Disk)",
                    snapshot.volumes.len() - visible,
                ))],
            );
        }
    });
}

fn draw_disk_volume(frame: &mut Frame, area: Rect, volume: &VolumeStats, rates_ready: bool) {
    if area.height == 1 {
        let capacity = format!("  {} free", format_bytes(volume.available_bytes));
        let name_width = (area.width as usize).saturating_sub(capacity.len());
        render_lines(
            frame,
            area,
            vec![Line::from(vec![
                Span::styled(fit(&volume.name, name_width), Style::new().bold()),
                Span::raw(capacity),
            ])],
        );
        return;
    }
    let lines = vec![
        Line::from(vec![
            Span::styled(volume.name.clone(), Style::new().bold()),
            Span::raw(" "),
            Span::styled(format!("({})", volume.mount_point), Style::new().dim()),
        ]),
        Line::from(vec![Span::styled(
            format!(
                "{} free of {}",
                format_bytes(volume.available_bytes),
                format_bytes(volume.total_bytes)
            ),
            Style::new().bold(),
        )]),
        Line::from(vec![
            Span::styled("▼ ", Style::new().fg(APP)),
            Span::styled(
                format!("read {}", rate_text(volume.read_bytes_per_sec, rates_ready)),
                Style::new().fg(APP).bold(),
            ),
            Span::raw("   "),
            Span::styled("▲ ", Style::new().fg(COMPRESSED)),
            Span::styled(
                format!(
                    "write {}",
                    rate_text(volume.write_bytes_per_sec, rates_ready)
                ),
                Style::new().fg(COMPRESSED).bold(),
            ),
        ]),
    ];
    let (text, chart) = super::split_length(area, 3);
    render_lines(frame, text, lines);
    if chart.height >= 2 {
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
}

fn draw_net(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "NETWORK", |frame, inner| {
        let network = &snapshot.network;
        if network.interface_name.is_empty() {
            render_lines(frame, inner, vec![Line::from("no active interface")]);
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
        let mut lines = vec![
            Line::from(vec![
                Span::styled("▼ ", Style::new().fg(APP)),
                Span::styled(
                    format!(
                        "down {}",
                        rate_text(network.down_bytes_per_sec, snapshot.rates_ready)
                    ),
                    Style::new().fg(APP).bold(),
                ),
            ]),
            Line::from(vec![
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
                Span::raw("  "),
                Span::styled(status_text, status_style),
                Span::raw("  "),
                Span::styled(ip, Style::new().dim()),
            ]),
        ];
        if inner.height < 10 {
            let mut combined = lines[0].clone();
            combined.spans.push(Span::raw("    "));
            combined.spans.extend(lines[1].spans.clone());
            if combined.width() <= inner.width as usize {
                lines[0] = combined;
                lines.remove(1);
            }
        }
        let peaks = super::network::peak_lines(&network.peaks, inner.width, snapshot.rates_ready);
        if inner.height as usize >= lines.len() + peaks.len() {
            lines.extend(peaks);
        } else if inner.height as usize > lines.len() {
            lines.push(Line::from("Peak rates: 4 Network"));
        }
        let (text, chart) = super::split_length(inner, lines.len() as u16);
        render_lines(frame, text, lines);
        if chart.height >= 3 {
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
