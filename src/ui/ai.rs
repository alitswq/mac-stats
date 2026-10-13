use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::{GREEN, YELLOW, card};
use crate::ai::{AiSnapshot, usage_lines};

pub(super) fn draw(frame: &mut Frame, area: Rect, snapshot: &AiSnapshot, now: u64) {
    let areas = Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).split(area);
    for (index, (title, usage)) in [
        ("CODEX subscription", &snapshot.codex),
        ("CLAUDE subscription (Code feed)", &snapshot.claude),
    ]
    .into_iter()
    .enumerate()
    {
        card(frame, areas[index], title, |frame, inner| {
            let mut lines: Vec<Line<'static>> = usage_lines(usage, now)
                .into_iter()
                .map(Line::from)
                .collect();
            if let Some(line) = lines.first_mut() {
                *line = line.clone().style(Style::new().fg(
                    if usage.is_stale(now) || usage.observed_at.is_none() || usage.message.is_some()
                    {
                        YELLOW
                    } else {
                        GREEN
                    },
                ));
            }
            if lines.len() > inner.height as usize {
                let visible = inner.height.saturating_sub(1) as usize;
                let hidden = lines.len() - visible;
                lines.truncate(visible);
                lines.push(Line::from(format!("+{hidden} more lines: mac-stats --ai-once")).dim());
            }
            frame.render_widget(Paragraph::new(lines), inner);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ProviderUsage, QuotaWindow, TokenUsage};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn rendered(snapshot: &AiSnapshot, width: u16, height: u16, now: u64) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), snapshot, now))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..height {
            for x in 0..width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn allowance_reset_freshness_and_usage_fit_the_minimum_content_area() {
        let mut usage = ProviderUsage {
            observed_at: Some(1000),
            windows: vec![
                QuotaWindow {
                    label: "5h allowance".into(),
                    used_percent: 25.0,
                    resets_at: Some(5000),
                },
                QuotaWindow {
                    label: "7d allowance".into(),
                    used_percent: 100.0,
                    resets_at: Some(10000),
                },
            ],
            tokens: TokenUsage {
                context_input: Some(123),
                context_output: Some(45),
                lifetime: Some(1234),
                estimated_session_cost: Some(1.25),
            },
            ..Default::default()
        };
        usage.model = Some("Claude".into());
        let snapshot = AiSnapshot {
            codex: usage.clone(),
            claude: usage,
        };
        for (width, height) in [(70, 17), (80, 21), (100, 33)] {
            let text = rendered(&snapshot, width, height, 1000);
            for label in [
                "CODEX",
                "CLAUDE",
                "75.0% remaining",
                "0.0% remaining",
                "reset",
                "Updated 0s ago",
                "Context tokens: in 123  out 45",
                "$1.25",
            ] {
                assert!(
                    text.contains(label),
                    "missing {label} at {width}x{height}:\n{text}"
                );
            }
        }
    }

    #[test]
    fn stale_elapsed_and_missing_readings_have_explicit_states() {
        let snapshot = AiSnapshot {
            codex: ProviderUsage {
                observed_at: Some(1000),
                windows: vec![QuotaWindow {
                    label: "5h".into(),
                    used_percent: 25.0,
                    resets_at: Some(1200),
                }],
                ..Default::default()
            },
            claude: ProviderUsage {
                message: Some("Claude feed setup needed: run --install-claude-feed.".into()),
                ..Default::default()
            },
        };
        let text = rendered(&snapshot, 70, 17, 1400);
        assert!(text.contains("STALE"), "{text}");
        assert!(text.contains("awaiting update"), "{text}");
        assert!(text.contains("--install-claude-feed"), "{text}");
        assert!(!text.contains("75.0% remaining"), "{text}");
        assert!(!text.contains("100.0% remaining"), "{text}");
    }
}
