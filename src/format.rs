use crate::model::{MemoryStats, Snapshot, VolumeStats};

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let digits = if unit >= 3 {
        if value < 100.0 { 2 } else { 1 }
    } else if unit == 2 {
        if value < 10.0 { 2 } else { 1 }
    } else if value < 10.0 {
        1
    } else {
        0
    };
    format!("{:.*} {}", digits, value, UNITS[unit])
}

pub fn format_rate(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

pub fn format_percent(value: f64) -> String {
    if !value.is_finite() {
        return "0%".to_string();
    }
    let value = value.clamp(0.0, 999.0);
    if value > 0.0 && value < 10.0 {
        format!("{value:.1}%")
    } else {
        format!("{value:.0}%")
    }
}

pub fn format_cpu_process(value: f32) -> String {
    if !value.is_finite() || value < 0.0 {
        "0.0%".to_string()
    } else {
        format!("{value:.1}%")
    }
}

pub fn format_load(value: f64) -> String {
    if !value.is_finite() {
        "0.00".to_string()
    } else {
        format!("{value:.2}")
    }
}

pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!(
            "{}, {}",
            count(days, "day", "days"),
            count(hours, "hour", "hours")
        )
    } else if hours > 0 {
        format!(
            "{}, {}",
            count(hours, "hour", "hours"),
            count(minutes, "minute", "minutes")
        )
    } else {
        count(minutes, "minute", "minutes")
    }
}

fn count(value: u64, singular: &str, plural: &str) -> String {
    if value == 1 {
        format!("{value} {singular}")
    } else {
        format!("{value} {plural}")
    }
}

pub fn rate_text(value: Option<u64>, rates_ready: bool) -> String {
    match value {
        Some(rate) => format_rate(rate),
        None if rates_ready => "—".to_string(),
        None => "…".to_string(),
    }
}

pub fn percent_text(value: Option<f64>) -> String {
    match value {
        Some(percent) => format_percent(percent),
        None => "…".to_string(),
    }
}

pub fn report(snapshot: &Snapshot) -> String {
    let mut lines = Vec::new();
    lines.push("mac-stats".to_string());
    lines.push(String::new());
    lines.push(format!(
        "CPU   {}   user {}   system {}   idle {}",
        percent_text(snapshot.cpu.usage_percent),
        percent_text(snapshot.cpu.user_percent),
        percent_text(snapshot.cpu.system_percent),
        percent_text(snapshot.cpu.idle_percent),
    ));
    if snapshot.cpu.efficiency_percent.is_some() || snapshot.cpu.performance_percent.is_some() {
        lines.push(format!(
            "      e-cores {}   p-cores {}",
            percent_text(snapshot.cpu.efficiency_percent),
            percent_text(snapshot.cpu.performance_percent),
        ));
    }
    lines.push(format!(
        "      load {}  {}  {}   up {}",
        format_load(snapshot.cpu.load_one),
        format_load(snapshot.cpu.load_five),
        format_load(snapshot.cpu.load_fifteen),
        format_uptime(snapshot.cpu.uptime_seconds),
    ));
    lines.push(String::new());
    lines.push(memory_line(&snapshot.memory));
    lines.push(format!(
        "      app {}   wired {}   compressed {}   free {}",
        format_bytes(snapshot.memory.app_bytes),
        format_bytes(snapshot.memory.wired_bytes),
        format_bytes(snapshot.memory.compressed_bytes),
        format_bytes(snapshot.memory.free_bytes),
    ));
    lines.push(format!(
        "      swap {}",
        format_bytes(snapshot.memory.swap_used_bytes)
    ));
    lines.push(String::new());
    if snapshot.volumes.is_empty() {
        lines.push("DISK  no user volumes".to_string());
    }
    for volume in &snapshot.volumes {
        lines.push(volume_line(snapshot.rates_ready, volume));
    }
    lines.push(String::new());
    lines.push(network_line(snapshot));
    lines.push(String::new());
    lines.push("Top memory".to_string());
    for process in snapshot.top_memory.iter().take(5) {
        lines.push(format!(
            "  {:<24} {}",
            fit(&process.name, 24),
            format_bytes(process.memory_bytes)
        ));
    }
    lines.join("\n")
}

fn memory_line(memory: &MemoryStats) -> String {
    if !memory.available {
        return "RAM   unavailable".to_string();
    }
    format!(
        "RAM   {} used of {} ({})",
        format_bytes(memory.used_bytes),
        format_bytes(memory.total_bytes),
        format_percent(memory.used_percent()),
    )
}

fn volume_line(rates_ready: bool, volume: &VolumeStats) -> String {
    format!(
        "DISK  {}   {} free of {}   read {}   write {}",
        volume.name,
        format_bytes(volume.available_bytes),
        format_bytes(volume.total_bytes),
        rate_text(volume.read_bytes_per_sec, rates_ready),
        rate_text(volume.write_bytes_per_sec, rates_ready),
    )
}

fn network_line(snapshot: &Snapshot) -> String {
    let network = &snapshot.network;
    if network.interface_name.is_empty() {
        return "NET   no active interface".to_string();
    }
    let status = if network.is_up { "UP" } else { "DOWN" };
    let ip = network.local_ip.as_deref().unwrap_or("no address");
    format!(
        "NET   {}  {}  {}   down {}   up {}",
        network.interface_name,
        status,
        ip,
        rate_text(network.down_bytes_per_sec, snapshot.rates_ready),
        rate_text(network.up_bytes_per_sec, snapshot.rates_ready),
    )
}

pub fn fit(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut trimmed: String = text.chars().take(width - 1).collect();
    trimmed.push('…');
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::example_snapshot;

    #[test]
    fn bytes_use_binary_units_and_the_menu_bar_precision() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(16 * 1024), "16 KB");
        assert_eq!(format_bytes(17 * 1024 * 1024 + 300 * 1024), "17.3 MB");
        assert_eq!(format_bytes(94 * 1024 * 1024 * 1024), "94.00 GB");
        assert_eq!(format_rate(16 * 1024), "16 KB/s");
        for rate in [0, 1, 500, 1023, 20_000, 1024 * 1024, 100 * 1024 * 1024] {
            let label = format_rate(rate);
            assert!(label.chars().count() <= 10, "{label}");
        }
    }

    #[test]
    fn uptime_matches_the_long_form() {
        assert_eq!(format_uptime(3 * 3600 + 2 * 60), "3 hours, 2 minutes");
        assert_eq!(format_uptime(60), "1 minute");
        assert_eq!(format_uptime(86_400 + 3_600), "1 day, 1 hour");
    }

    #[test]
    fn report_includes_each_section() {
        let text = report(&example_snapshot());
        assert!(text.contains("CPU   18%"));
        assert!(text.contains("e-cores 28%"));
        assert!(text.contains("RAM   94.00 GB used of 128.0 GB"));
        assert!(text.contains("Macintosh HD"));
        assert!(text.contains("en13  UP  192.168.68.76"));
        assert!(text.contains("Docker"));
    }
}
