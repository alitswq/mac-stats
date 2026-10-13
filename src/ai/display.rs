use time::{OffsetDateTime, UtcOffset};

use super::{AiSnapshot, ProviderUsage, QuotaWindow};
use crate::format::fit;

pub(crate) fn duration(seconds: u64) -> String {
    if seconds >= 86400 {
        format!("{}d {}h", seconds / 86400, seconds % 86400 / 3600)
    } else if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

fn reset_clock(timestamp: u64) -> String {
    let Some(time) = i64::try_from(timestamp)
        .ok()
        .and_then(|timestamp| OffsetDateTime::from_unix_timestamp(timestamp).ok())
    else {
        return "time unavailable".into();
    };
    let (offset, label) = UtcOffset::local_offset_at(time)
        .map(|offset| (offset, "local"))
        .unwrap_or((UtcOffset::UTC, "UTC"));
    let time = time.to_offset(offset);
    format!(
        "{:02}-{:02} {:02}:{:02} {label}",
        time.month() as u8,
        time.day(),
        time.hour(),
        time.minute()
    )
}

fn quota_line(window: &QuotaWindow, now: u64) -> String {
    let label = fit(&window.label, 12);
    match window.remaining_percent(now) {
        Some(remaining) => {
            let reset = window
                .resets_at
                .map(|reset| {
                    format!(
                        "{} ({})",
                        duration(reset.saturating_sub(now)),
                        reset_clock(reset)
                    )
                })
                .unwrap_or_else(|| "time not reported".into());
            format!("{label:<12} {remaining:5.1}% remaining  reset {reset}")
        }
        None => format!("{label:<12} — remaining  reset elapsed; awaiting update"),
    }
}

pub(crate) fn usage_lines(usage: &ProviderUsage, now: u64) -> Vec<String> {
    let mut lines = vec![match usage.observed_at {
        Some(seen) => format!(
            "{}Updated {} ago",
            if usage.is_stale(now) {
                "STALE • "
            } else {
                ""
            },
            duration(now.saturating_sub(seen))
        ),
        None => "Allowance unavailable".into(),
    }];
    if let Some(message) = &usage.message {
        lines.push(message.clone());
    }
    lines.extend(usage.windows.iter().map(|window| quota_line(window, now)));
    let tokens = &usage.tokens;
    if tokens.context_input.is_some() || tokens.context_output.is_some() {
        let input = tokens
            .context_input
            .map(|value| value.to_string())
            .unwrap_or_else(|| "—".into());
        let output = tokens
            .context_output
            .map(|value| value.to_string())
            .unwrap_or_else(|| "—".into());
        lines.push(format!("Context tokens: in {input}  out {output}"));
    }
    if let Some(cost) = tokens.estimated_session_cost {
        lines.push(format!("Est. session API cost: ${cost:.2}"));
    }
    if let Some(lifetime) = tokens.lifetime {
        lines.push(format!("Account lifetime tokens: {lifetime}"));
    }
    if tokens.context_input.is_none()
        && tokens.context_output.is_none()
        && tokens.lifetime.is_none()
    {
        lines.push("Token usage: not reported.".into());
    }
    if let Some(model) = &usage.model {
        lines.push(format!("Latest session model: {model}"));
    }
    lines
}

pub fn report(snapshot: &AiSnapshot, now: u64) -> String {
    let mut lines = vec![
        crate::RELEASE.into(),
        format!("Developed by {}", crate::DEVELOPER),
        String::new(),
    ];
    for (label, usage) in [
        ("CODEX subscription", &snapshot.codex),
        ("CLAUDE subscription (Code feed)", &snapshot.claude),
    ] {
        lines.push(label.into());
        lines.extend(usage_lines(usage, now));
        lines.push(String::new());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_and_cost_labels_preserve_the_scope_of_claude_reported_data() {
        let mut snapshot = AiSnapshot::default();
        snapshot.claude.tokens.context_input = Some(123);
        snapshot.claude.tokens.context_output = Some(45);
        snapshot.claude.tokens.estimated_session_cost = Some(1.25);
        let text = report(&snapshot, 1000);
        assert!(text.contains("Context tokens: in 123  out 45"), "{text}");
        assert!(!text.contains("Session tokens:"), "{text}");
        assert!(text.contains("Est. session API cost: $1.25"), "{text}");
        assert!(!text.contains("list price"), "{text}");
    }

    #[test]
    fn reset_countdowns_handle_seconds_days_and_expired_windows() {
        assert_eq!(duration(59), "59s");
        assert_eq!(duration(3601), "1h 0m");
        assert_eq!(duration(86400), "1d 0h");
        let window = QuotaWindow {
            label: "5h".into(),
            used_percent: 25.0,
            resets_at: Some(2000),
        };
        assert!(quota_line(&window, 1000).contains("75.0% remaining"));
        assert!(quota_line(&window, 2000).contains("awaiting update"));
        assert_eq!(reset_clock(u64::MAX), "time unavailable");
    }
}
