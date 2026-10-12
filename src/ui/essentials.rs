use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;

use crate::format::{format_bytes, format_load, format_percent, percent_text, rate_text};
use crate::model::Snapshot;

use super::chart::{AreaChart, SplitChart};
use super::{APP, COMPRESSED, FREE, WIRED, card, render_lines, stacked_bar};

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
        let mut lines = vec![
            Line::from(percent_text(snapshot.cpu.usage_percent)).bold(),
            Line::from(format!(
                "user {}   system {}   idle {}",
                percent_text(snapshot.cpu.user_percent),
                percent_text(snapshot.cpu.system_percent),
                percent_text(snapshot.cpu.idle_percent),
            )),
        ];
        if snapshot.cpu.efficiency_percent.is_some() || snapshot.cpu.performance_percent.is_some() {
            lines.push(Line::from(format!(
                "e-cores {}   p-cores {}",
                percent_text(snapshot.cpu.efficiency_percent),
                percent_text(snapshot.cpu.performance_percent),
            )));
        }
        lines.push(Line::from(format!(
            "load {}  {}  {}",
            format_load(snapshot.cpu.load_one),
            format_load(snapshot.cpu.load_five),
            format_load(snapshot.cpu.load_fifteen),
        )));
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
            Line::from(format!(
                "{} used  {}",
                format_bytes(memory.used_bytes),
                format_percent(memory.used_percent())
            ))
            .bold(),
            stacked_bar(
                inner.width,
                &[
                    (memory.app_bytes, APP),
                    (memory.wired_bytes, WIRED),
                    (memory.compressed_bytes, COMPRESSED),
                    (memory.free_bytes, FREE),
                ],
            ),
            Line::from(format!(
                "app {}  wired {}",
                format_bytes(memory.app_bytes),
                format_bytes(memory.wired_bytes),
            )),
            Line::from(format!(
                "compressed {}  free {}",
                format_bytes(memory.compressed_bytes),
                format_bytes(memory.free_bytes),
            )),
            Line::from(format!("swap {}", format_bytes(memory.swap_used_bytes))),
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
        let Some(volume) = snapshot.volumes.first() else {
            render_lines(frame, inner, vec![Line::from("no user volumes")]);
            return;
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
        let (text, chart) = super::split_length(inner, 3);
        render_lines(frame, text, lines);
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

fn draw_net(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "NETWORK", |frame, inner| {
        let network = &snapshot.network;
        if network.interface_name.is_empty() {
            render_lines(frame, inner, vec![Line::from("no active interface")]);
            return;
        }
        let status = if network.is_up { "UP" } else { "DOWN" };
        let ip = network
            .local_ip
            .clone()
            .unwrap_or_else(|| "no address".to_string());
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
            Line::from(format!("{}  {}  {}", network.interface_name, status, ip)),
        ];
        let (text, chart) = super::split_length(inner, 3);
        render_lines(frame, text, lines);
        if chart.height > 0 {
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
