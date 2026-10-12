use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Bar, BarChart, Paragraph};

use crate::format::{format_load, format_uptime, percent_text};
use crate::model::{CoreCluster, Snapshot};

use super::{
    APP, ECORE, PCORE, ProcessMetric, card, process_table, render_lines, spark, split_length,
};

pub fn draw(frame: &mut Frame, area: Rect, snapshot: &Snapshot) {
    card(frame, area, "CPU", |frame, inner| {
        if !snapshot.cpu.available {
            frame.render_widget(Paragraph::new("CPU counters are unavailable."), inner);
            return;
        }
        let mut lines = vec![
            Line::from(format!(
                "{}   user {}   system {}   idle {}",
                percent_text(snapshot.cpu.usage_percent),
                percent_text(snapshot.cpu.user_percent),
                percent_text(snapshot.cpu.system_percent),
                percent_text(snapshot.cpu.idle_percent),
            ))
            .bold(),
        ];
        if snapshot.cpu.efficiency_percent.is_some() || snapshot.cpu.performance_percent.is_some() {
            lines.push(Line::from(format!(
                "e-cores {}   p-cores {}",
                percent_text(snapshot.cpu.efficiency_percent),
                percent_text(snapshot.cpu.performance_percent),
            )));
        }
        lines.push(Line::from(format!(
            "load {}  {}  {}    up {}",
            format_load(snapshot.cpu.load_one),
            format_load(snapshot.cpu.load_five),
            format_load(snapshot.cpu.load_fifteen),
            format_uptime(snapshot.cpu.uptime_seconds),
        )));
        let text_height = lines.len() as u16;
        let (summary, rest) = split_length(inner, text_height);
        render_lines(frame, summary, lines);

        let show_chart = rest.height >= 12;
        let (chart, rest) = if show_chart {
            split_length(rest, 4)
        } else {
            (Rect::new(rest.x, rest.y, rest.width, 0), rest)
        };
        if chart.height > 0 {
            frame.render_widget(spark(&snapshot.cpu_history, Some(100), APP), chart);
        }

        let core_height = if rest.height > 14 { 8 } else { rest.height / 3 };
        let (cores, table) = if snapshot.cpu.cores.is_empty() || core_height < 3 {
            (Rect::new(rest.x, rest.y, rest.width, 0), rest)
        } else {
            split_length(rest, core_height)
        };
        if cores.height > 0 {
            frame.render_widget(core_chart(&snapshot.cpu.cores, cores.width), cores);
        }
        if table.height > 0 {
            frame.render_widget(
                process_table(&snapshot.top_cpu, ProcessMetric::Cpu, snapshot.rates_ready),
                table,
            );
        }
    });
}

fn core_chart(cores: &[crate::model::CoreStats], width: u16) -> BarChart<'static> {
    let bars = cores
        .iter()
        .map(|core| {
            let color = match core.cluster {
                CoreCluster::Performance => PCORE,
                CoreCluster::Efficiency => ECORE,
                CoreCluster::Unknown => APP,
            };
            Bar::new(core.usage_percent.round().clamp(0.0, 100.0) as u64)
                .style(Style::new().fg(color))
                .text_value(String::new())
        })
        .collect::<Vec<_>>();
    let count = bars.len().max(1) as u16;
    let (bar_width, bar_gap) = if width >= count.saturating_mul(3) {
        (2, 1)
    } else if width >= count.saturating_mul(2) {
        (1, 1)
    } else {
        (1, 0)
    };
    BarChart::new(bars)
        .bar_width(bar_width)
        .bar_gap(bar_gap)
        .max(100)
}
