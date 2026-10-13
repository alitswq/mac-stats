use serde_json::Value;

use super::{ProviderUsage, QuotaWindow, TokenUsage};

/// Decode the documented account/rateLimits/read response.
pub fn codex(
    limits: &Value,
    usage: Option<&Value>,
    now: u64,
) -> Result<ProviderUsage, &'static str> {
    if !limits.is_object() {
        return Err("Invalid Codex quota response");
    }
    let mut windows = Vec::new();
    let mut statuses = Vec::new();
    if limits["ordinaryUsageAllowed"].as_bool() == Some(false) {
        statuses.push("Ordinary Codex usage is unavailable.".to_string());
    }
    if let Some(status) = codex_status(&limits["rateLimits"]) {
        statuses.push(status);
    }
    if let Some(groups) = limits["rateLimitsByLimitId"]
        .as_object()
        .filter(|groups| !groups.is_empty())
    {
        for (id, group) in groups {
            codex_windows(group, &safe_text(id), &mut windows)?;
            if let Some(status) = codex_status(group) {
                statuses.push(status);
            }
        }
    } else if !limits["rateLimits"].is_null() {
        codex_windows(&limits["rateLimits"], "codex", &mut windows)?;
    }
    if windows.is_empty() {
        statuses.push("Subscription allowance not reported by Codex.".into());
    }
    statuses.sort();
    statuses.dedup();
    let message = (!statuses.is_empty()).then(|| statuses.join(" "));
    Ok(ProviderUsage {
        observed_at: Some(now),
        windows,
        tokens: TokenUsage {
            lifetime: usage.and_then(|data| data["summary"]["lifetimeTokens"].as_u64()),
            ..Default::default()
        },
        message,
        ..Default::default()
    })
}

fn codex_status(group: &Value) -> Option<String> {
    let reached = &group["rateLimitReachedType"];
    if reached.is_null() {
        return None;
    }
    Some(match reached.as_str() {
        Some(reason) => format!("Provider limit reached: {}.", safe_text(reason)),
        None => "Provider reports a reached usage limit.".into(),
    })
}

fn codex_windows(
    group: &Value,
    id: &str,
    windows: &mut Vec<QuotaWindow>,
) -> Result<(), &'static str> {
    if !group.is_object() {
        return Err("Invalid Codex quota group");
    }
    for key in ["primary", "secondary"] {
        let window = &group[key];
        if window.is_null() {
            continue;
        }
        let label = if window["windowDurationMins"].is_null() {
            format!("{id} {key}")
        } else {
            let minutes = window["windowDurationMins"]
                .as_u64()
                .filter(|minutes| *minutes > 0)
                .ok_or("Invalid Codex quota duration")?;
            let duration = if minutes % 1440 == 0 {
                format!("{}d", minutes / 1440)
            } else if minutes % 60 == 0 {
                format!("{}h", minutes / 60)
            } else {
                format!("{minutes}m")
            };
            format!("{id} {duration}")
        };
        windows.push(quota(window, label, "usedPercent", "resetsAt")?);
    }
    Ok(())
}

/// Decode the minimal snapshot exported by the Claude status-line command.
pub fn claude(snapshot: &Value, now: u64) -> Result<ProviderUsage, &'static str> {
    if snapshot["schema_version"].as_u64() != Some(1) {
        return Err("Unsupported Claude usage snapshot version");
    }
    let seen = snapshot["observed_at"]
        .as_u64()
        .filter(|seen| *seen <= now.saturating_add(5))
        .ok_or("Invalid Claude usage observation time")?;
    let data = &snapshot["data"];
    if !data.is_object() {
        return Err("Invalid Claude usage snapshot");
    }
    let mut windows = Vec::new();
    for (key, label) in [("five_hour", "5h allowance"), ("seven_day", "7d allowance")] {
        let window = &data["rate_limits"][key];
        if !window.is_null() {
            windows.push(quota(window, label.into(), "used_percentage", "resets_at")?);
        }
    }
    let message = windows
        .is_empty()
        .then(|| "Subscription allowance not reported by Claude Code.".into());
    Ok(ProviderUsage {
        observed_at: Some(seen),
        windows,
        model: data["model"]["display_name"].as_str().map(safe_text),
        tokens: TokenUsage {
            context_input: data["context_window"]["total_input_tokens"].as_u64(),
            context_output: data["context_window"]["total_output_tokens"].as_u64(),
            estimated_session_cost: data["cost"]["total_cost_usd"]
                .as_f64()
                .filter(|cost| cost.is_finite() && *cost >= 0.0),
            ..Default::default()
        },
        message,
    })
}

fn quota(
    value: &Value,
    label: String,
    percent: &str,
    reset: &str,
) -> Result<QuotaWindow, &'static str> {
    let used_percent = value[percent]
        .as_f64()
        .filter(|used| used.is_finite() && (0.0..=100.0).contains(used))
        .ok_or("Invalid provider quota percentage")?;
    let resets_at = if value[reset].is_null() {
        None
    } else {
        Some(
            value[reset]
                .as_u64()
                .ok_or("Invalid provider quota reset time")?,
        )
    };
    Ok(QuotaWindow {
        label,
        used_percent,
        resets_at,
    })
}

pub(super) fn safe_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .take(40)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_reads_all_reported_windows_without_duplicate_fallback_limits() {
        let limits = json!({
            "rateLimits": {"primary": {"usedPercent": 99}},
            "rateLimitsByLimitId": {"codex": {
                "primary": {"usedPercent": 25, "windowDurationMins": 300, "resetsAt": 2000},
                "secondary": {"usedPercent": 0, "windowDurationMins": 10080, "resetsAt": 9000}
            }}
        });
        let usage = codex(
            &limits,
            Some(&json!({"summary": {"lifetimeTokens": 1234}})),
            1000,
        )
        .unwrap();
        assert_eq!(usage.windows.len(), 2);
        assert_eq!(usage.windows[0].label, "codex 5h");
        assert_eq!(usage.windows[0].remaining_percent(1000), Some(75.0));
        assert_eq!(usage.windows[1].label, "codex 7d");
        assert_eq!(usage.tokens.lifetime, Some(1234));
        assert_eq!(usage.observed_at, Some(1000));
    }

    #[test]
    fn codex_handles_legacy_shape_and_absent_windows_without_inventing_quota() {
        let parsed = codex(
            &json!({"rateLimits": {"primary": {"usedPercent": 100}, "secondary": null}}),
            None,
            1000,
        )
        .unwrap();
        assert_eq!(parsed.windows[0].remaining_percent(1000), Some(0.0));
        let empty = codex(&json!({"rateLimits": null}), None, 1000).unwrap();
        assert!(empty.windows.is_empty());
        assert!(empty.message.is_some());
        assert_eq!(empty.tokens.lifetime, None);
    }

    #[test]
    fn codex_preserves_reached_limit_status_alongside_remaining_window_allowance() {
        let usage = codex(
            &json!({"rateLimitsByLimitId": {"codex": {
                "rateLimitReachedType": "workspaceCredits",
                "primary": {"usedPercent": 25, "windowDurationMins": 300, "resetsAt": 4000}
            }}}),
            None,
            1000,
        )
        .unwrap();
        assert_eq!(usage.windows[0].remaining_percent(1000), Some(75.0));
        assert!(
            usage
                .message
                .as_deref()
                .is_some_and(|message| message.contains("workspaceCredits"))
        );
        let restricted = codex(
            &json!({"ordinaryUsageAllowed": false,
                "rateLimits": {"primary": {"usedPercent": 25}}
            }),
            None,
            1000,
        )
        .unwrap();
        assert!(
            restricted
                .message
                .as_deref()
                .is_some_and(|message| message.contains("unavailable"))
        );
        let no_extra_credits = codex(
            &json!({"rateLimits": {
                "credits": {"hasCredits": false}, "primary": {"usedPercent": 25}
            }}),
            None,
            1000,
        )
        .unwrap();
        assert!(no_extra_credits.message.is_none());
    }

    #[test]
    fn claude_preserves_the_feed_timestamp_and_context_usage() {
        let snapshot = json!({"schema_version": 1, "observed_at": 1000, "data": {
            "rate_limits": {
                "five_hour": {"used_percentage": 40, "resets_at": 4000},
                "seven_day": {"used_percentage": 80, "resets_at": 8000}
            },
            "model": {"display_name": "Claude"},
            "context_window": {"total_input_tokens": 123, "total_output_tokens": 45},
            "cost": {"total_cost_usd": 1.25}
        }});
        let usage = claude(&snapshot, 1400).unwrap();
        assert!(usage.is_stale(1400));
        assert_eq!(usage.observed_at, Some(1000));
        assert_eq!(usage.windows[0].remaining_percent(1400), Some(60.0));
        assert_eq!(usage.tokens.context_input, Some(123));
        assert_eq!(usage.tokens.context_output, Some(45));
        assert_eq!(usage.tokens.estimated_session_cost, Some(1.25));
    }

    #[test]
    fn malformed_percentages_and_timestamps_are_not_accepted() {
        for percent in [json!(-1), json!(101), json!("25"), Value::Null] {
            assert!(
                codex(
                    &json!({"rateLimits": {"primary": {"usedPercent": percent}}}),
                    None,
                    1000
                )
                .is_err()
            );
        }
        assert!(
            claude(
                &json!({"schema_version": 1, "observed_at": 2000, "data": {}}),
                1000
            )
            .is_err()
        );
        assert!(
            claude(
                &json!({"schema_version": 2, "observed_at": 1000, "data": {}}),
                1000
            )
            .is_err()
        );
        assert!(
            codex(
                &json!({"rateLimits": {"primary": {"usedPercent": 25, "resetsAt": "later"}}}),
                None,
                1000
            )
            .is_err()
        );
    }

    #[test]
    fn missing_claude_subscription_fields_are_unknown_and_model_text_is_safe() {
        let usage = claude(
            &json!({"schema_version": 1, "observed_at": 1000, "data": {
                "model": {"display_name": "Claude\u{001b}[31m\nTest"}
            }}),
            1000,
        )
        .unwrap();
        assert!(usage.windows.is_empty());
        assert!(usage.message.is_some());
        assert!(
            usage
                .model
                .unwrap()
                .chars()
                .all(|character| !character.is_control())
        );
        assert_eq!(usage.tokens.context_input, None);
        assert_eq!(usage.tokens.estimated_session_cost, None);
    }
}
