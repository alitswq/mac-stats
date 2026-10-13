use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{ProviderUsage, now_unix, parse_codex};

/// Query account data through the user's existing Codex CLI sign-in.
pub fn collect_codex(cancel: &AtomicBool) -> ProviderUsage {
    let mut command = Command::new("codex");
    command.args(["app-server", "--listen", "stdio://"]);
    match query(command, cancel, Duration::from_secs(10)) {
        Ok(usage) => usage,
        Err(message) => ProviderUsage {
            message: Some(message.into()),
            ..Default::default()
        },
    }
}

fn query(
    command: Command,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<ProviderUsage, &'static str> {
    let mut session = Session::open(command, timeout)?;
    session.request(
        1,
        "initialize",
        json!({"clientInfo": {
            "name": "mac_stats", "title": "mac-stats", "version": crate::VERSION
        }}),
        cancel,
    )?;
    session.send(&json!({"method": "initialized", "params": {}}))?;
    let account = session.request(2, "account/read", json!({"refreshToken": false}), cancel)?;
    if account["account"]["type"].as_str() != Some("chatgpt") {
        return Err("A Codex ChatGPT sign-in is required for subscription allowance.");
    }
    let limits = session.request(3, "account/rateLimits/read", json!({}), cancel)?;
    let observed_at = now_unix();
    // Older CLI versions may omit this optional account summary.
    let usage = session
        .request(4, "account/usage/read", json!({}), cancel)
        .ok();
    parse_codex(&limits, usage.as_ref(), observed_at)
}

struct Session {
    child: Child,
    input: ChildStdin,
    messages: Option<Receiver<Result<Value, &'static str>>>,
    reader: Option<JoinHandle<()>>,
    deadline: Instant,
}

impl Session {
    fn open(mut command: Command, timeout: Duration) -> Result<Self, &'static str> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|_| "Codex CLI could not start. Install it and sign in with ChatGPT.")?;
        let input = child.stdin.take().ok_or("Codex input unavailable")?;
        let output = child.stdout.take().ok_or("Codex output unavailable")?;
        let (sender, receiver) = mpsc::sync_channel(8);
        let mut session = Self {
            child,
            input,
            messages: Some(receiver),
            reader: None,
            deadline: Instant::now() + timeout,
        };
        session.reader = Some(
            thread::Builder::new()
                .name("codex-account-reader".into())
                .spawn(move || {
                    let mut reader = BufReader::new(output);
                    let mut total = 0;
                    loop {
                        let mut line = Vec::new();
                        let count = Read::by_ref(&mut reader)
                            .take(1_048_577)
                            .read_until(b'\n', &mut line);
                        let Ok(count) = count else { break };
                        if count == 0 {
                            break;
                        }
                        total += count;
                        let value = if count > 1_048_576 || total > 8_388_608 {
                            Err("Codex account response is too large")
                        } else {
                            serde_json::from_slice(&line)
                                .map_err(|_| "Invalid Codex account response")
                        };
                        let failed = value.is_err();
                        if sender.send(value).is_err() || failed {
                            break;
                        }
                    }
                })
                .map_err(|_| "Codex account reader could not start")?,
        );
        Ok(session)
    }

    fn send(&mut self, value: &Value) -> Result<(), &'static str> {
        writeln!(self.input, "{value}")
            .and_then(|_| self.input.flush())
            .map_err(|_| "Codex account connection closed")
    }

    fn request(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
        cancel: &AtomicBool,
    ) -> Result<Value, &'static str> {
        self.send(&json!({"id": id, "method": method, "params": params}))?;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Codex account query canceled");
            }
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Codex account query timed out; retrying in one minute.");
            }
            match self
                .messages
                .as_ref()
                .unwrap()
                .recv_timeout(remaining.min(Duration::from_millis(50)))
            {
                Ok(value) => {
                    let reply = value?;
                    if reply["id"].as_u64() != Some(id) {
                        continue;
                    }
                    if reply.get("error").is_some() {
                        return Err("Codex account query failed; check the Codex CLI sign-in.");
                    }
                    return reply
                        .get("result")
                        .cloned()
                        .ok_or("Invalid Codex account response");
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("Codex account connection closed");
                }
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.messages.take();
        // This process group was created for this child only. Stop its helpers too,
        // so inherited stdout pipes cannot keep the reader blocked after a timeout.
        if let Ok(group) = i32::try_from(self.child.id()) {
            // SAFETY: a positive owned child ID becomes its isolated process-group ID.
            unsafe {
                libc::kill(-group, libc::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn account_protocol_ignores_notifications_and_keeps_quota_when_optional_usage_is_unsupported() {
        let command = shell(
            r#"
read -r request
printf '%s\n' '{"method":"account/updated","params":{}}' '{"id":1,"result":{}}'
read -r request
read -r request
printf '%s\n' '{"id":2,"result":{"account":{"type":"chatgpt"}}}'
read -r request
printf '%s\n' '{"id":3,"result":{"rateLimits":{"primary":{"usedPercent":25,"windowDurationMins":300,"resetsAt":4000000000}}}}'
read -r request
printf '%s\n' '{"id":4,"error":{"code":-32601,"message":"Not supported"}}'
"#,
        );
        let usage = query(command, &AtomicBool::new(false), Duration::from_secs(1)).unwrap();
        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.windows[0].remaining_percent(1000), Some(75.0));
        assert_eq!(usage.tokens.lifetime, None);
    }

    #[test]
    fn signed_out_accounts_are_not_treated_as_full_allowance() {
        let command = shell(
            r#"
read -r request
printf '%s\n' '{"id":1,"result":{}}'
read -r request
read -r request
printf '%s\n' '{"id":2,"result":{"account":null}}'
"#,
        );
        let error = query(command, &AtomicBool::new(false), Duration::from_secs(1)).unwrap_err();
        assert!(error.contains("sign-in"), "{error}");
    }

    #[test]
    fn timeout_and_cancellation_bound_the_owned_child_process() {
        let start = Instant::now();
        assert!(
            query(
                shell("exec sleep 3"),
                &AtomicBool::new(false),
                Duration::from_millis(30)
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        let cancel = AtomicBool::new(true);
        let error = query(shell("exec sleep 3"), &cancel, Duration::from_secs(1)).unwrap_err();
        assert!(error.contains("canceled"), "{error}");
        assert!(cancel.load(Ordering::Relaxed));
    }
}
