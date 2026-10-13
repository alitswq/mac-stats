use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use super::{ProviderUsage, parse_claude};

pub(crate) const MAX_SNAPSHOT_BYTES: u64 = 1_048_576;
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

pub fn claude_usage_path() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("MAC_STATS_CLAUDE_USAGE_FILE") {
        return Ok(PathBuf::from(path));
    }
    let home =
        std::env::var_os("HOME").ok_or_else(|| io::Error::other("Home directory unavailable"))?;
    Ok(PathBuf::from(home).join(".local/share/mac-stats/claude-usage.json"))
}

pub fn read_claude(path: &Path, now: u64) -> ProviderUsage {
    let result =
        read_json(path).and_then(|value| parse_claude(&value, now).map_err(io::Error::other));
    match result {
        Ok(usage) => usage,
        Err(error) => ProviderUsage {
            message: Some(if error.kind() == io::ErrorKind::NotFound {
                "Claude feed setup needed: run --install-claude-feed.".into()
            } else {
                "Claude usage feed could not be read; waiting for a valid export.".into()
            }),
            ..Default::default()
        },
    }
}

/// Store only quota and usage fields; never persist the entire status-line input.
pub fn export_claude(input: &[u8], path: &Path, now: u64) -> io::Result<String> {
    if input.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(io::Error::other("Claude status-line input is too large"));
    }
    let input: Value = serde_json::from_slice(input)
        .map_err(|_| io::Error::other("Invalid Claude status-line JSON"))?;
    if !input.is_object() {
        return Err(io::Error::other("Invalid Claude status-line input"));
    }
    let mut limits = serde_json::Map::new();
    for key in ["five_hour", "seven_day"] {
        let window = &input["rate_limits"][key];
        if !window.is_null() {
            limits.insert(
                key.into(),
                json!({
                    "used_percentage": window["used_percentage"], "resets_at": window["resets_at"]
                }),
            );
        }
    }
    let snapshot = json!({
        "schema_version": 1, "observed_at": now,
        "data": {
            "rate_limits": limits,
            "model": {"display_name": input["model"]["display_name"]},
            "context_window": {
                "total_input_tokens": input["context_window"]["total_input_tokens"],
                "total_output_tokens": input["context_window"]["total_output_tokens"]
            },
            "cost": {"total_cost_usd": input["cost"]["total_cost_usd"]}
        }
    });
    let usage = parse_claude(&snapshot, now).map_err(io::Error::other)?;
    atomic_json(path, &snapshot)?;
    let mut text = usage.model.unwrap_or_else(|| "Claude".into());
    for window in usage.windows {
        let remaining = window
            .remaining_percent(now)
            .map(|value| format!("{value:.0}% left"))
            .unwrap_or_else(|| "refresh needed".into());
        text.push_str(&format!(" | {}: {remaining}", window.label));
    }
    Ok(text)
}

pub fn export_claude_reader(input: impl Read, path: &Path, now: u64) -> io::Result<String> {
    let mut bytes = Vec::new();
    input.take(MAX_SNAPSHOT_BYTES + 1).read_to_end(&mut bytes)?;
    export_claude(&bytes, path, now)
}

/// Install an explicit, idempotent status-line command while preserving other settings.
pub fn install_claude_feed(settings: &Path, executable: &Path) -> io::Result<()> {
    if !executable.is_absolute() {
        return Err(io::Error::other(
            "Claude feed executable path must be absolute",
        ));
    }
    if fs::symlink_metadata(settings).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(io::Error::other(
            "Claude settings is a symlink; configure its source file manually",
        ));
    }
    let mut value = match read_json(settings) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(error),
    };
    if !value.is_object() {
        return Err(io::Error::other("Claude settings must be a JSON object"));
    }
    let command = format!(
        "'{}' --claude-statusline",
        executable.to_string_lossy().replace('\'', "'\\''")
    );
    if !value["statusLine"].is_null() {
        if value["statusLine"]["type"] == "command" && value["statusLine"]["command"] == command {
            return Ok(());
        }
        return Err(io::Error::other(
            "Existing Claude status line preserved. Chain --claude-statusline into your current command.",
        ));
    }
    value["statusLine"] = json!({"type": "command", "command": command});
    atomic_json(settings, &value)
}

fn read_json(path: &Path) -> io::Result<Value> {
    let mut bytes = Vec::new();
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("AI usage input must be a regular file"));
    }
    file.take(MAX_SNAPSHOT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(io::Error::other("AI usage file is too large"));
    }
    serde_json::from_slice(&bytes).map_err(|_| io::Error::other("Invalid AI usage JSON"))
}

fn atomic_json(path: &Path, value: &Value) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let temporary = parent.join(format!(
        ".mac-stats-{}-{}.tmp",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "mac-stats-claude-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn statusline_export_stores_only_usage_fields_in_a_private_atomic_snapshot() {
        let directory = TestDirectory::new();
        let path = directory.0.join("claude.json");
        let input = json!({
            "api_key": "must-not-be-stored", "workspace": {"current_dir": "/private/project"},
            "model": {"display_name": "Claude"},
            "rate_limits": {"five_hour": {"used_percentage": 40, "resets_at": 4000}},
            "cost": {"total_cost_usd": 1.25}
        });
        let output = export_claude(input.to_string().as_bytes(), &path, 1000).unwrap();
        assert!(output.contains("60% left"), "{output}");
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("must-not-be-stored"));
        assert!(!text.contains("/private/project"));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            read_claude(&path, 1000).windows[0].remaining_percent(1000),
            Some(60.0)
        );
        assert!(read_claude(&path, 1400).is_stale(1400));
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn invalid_or_oversized_exports_preserve_the_previous_good_snapshot() {
        let directory = TestDirectory::new();
        let path = directory.0.join("claude.json");
        export_claude(b"{}", &path, 1000).unwrap();
        let original = fs::read(&path).unwrap();
        for input in [
            b"not JSON".to_vec(),
            vec![b' '; 1_048_577],
            br#"{"rate_limits":{"five_hour":{"used_percentage":101}}}"#.to_vec(),
        ] {
            assert!(export_claude(&input, &path, 1100).is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }

    #[test]
    fn setup_preserves_other_settings_and_refuses_to_replace_an_existing_statusline() {
        let directory = TestDirectory::new();
        let path = directory.0.join("settings.json");
        fs::write(&path, r#"{"model":"custom","permissions":{"allow":[]}}"#).unwrap();
        let executable = Path::new("/tmp/mac stats's binary");
        install_claude_feed(&path, executable).unwrap();
        let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["model"], "custom");
        assert_eq!(value["permissions"]["allow"], json!([]));
        assert!(
            value["statusLine"]["command"]
                .as_str()
                .unwrap()
                .contains("--claude-statusline")
        );
        install_claude_feed(&path, executable).unwrap();
        fs::write(
            &path,
            r#"{"statusLine":{"type":"command","command":"my-statusline"}}"#,
        )
        .unwrap();
        let original = fs::read(&path).unwrap();
        assert!(install_claude_feed(&path, executable).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn missing_or_corrupt_feeds_are_explicitly_unavailable() {
        let directory = TestDirectory::new();
        let path = directory.0.join("claude.json");
        let missing = read_claude(&path, 1000);
        assert!(missing.windows.is_empty());
        assert!(missing.message.unwrap().contains("setup"));
        fs::write(&path, "not JSON").unwrap();
        assert!(read_claude(&path, 1000).message.is_some());
    }

    #[test]
    fn named_pipes_are_rejected_without_waiting_for_a_writer() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::thread;
        use std::time::{Duration, Instant};
        let directory = TestDirectory::new();
        let path = directory.0.join("feed.pipe");
        let name = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: name is a valid, NUL-terminated path in this test's private directory.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let writer_path = path.clone();
        let writer = thread::spawn(move || {
            // Release a blocking old implementation so the regression fails instead of hanging.
            thread::sleep(Duration::from_millis(300));
            if let Ok(mut file) = OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(writer_path)
            {
                let _ = file.write_all(b"{}");
            }
        });
        let start = Instant::now();
        let usage = read_claude(&path, 1000);
        let elapsed = start.elapsed();
        writer.join().unwrap();
        assert!(
            elapsed < Duration::from_millis(150),
            "feed read blocked for {elapsed:?}"
        );
        assert!(usage.observed_at.is_none());
        assert!(usage.message.is_some());
    }
}
