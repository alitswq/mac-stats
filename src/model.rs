use std::collections::{HashMap, VecDeque};
use std::time::Duration;

/// One-second samples kept for the history graphs. Wide terminals draw one
/// horizontal pixel per sample, so this is long enough to fill the window.
pub const HISTORY_LEN: usize = 600;
pub const TOP_PROCESS_COUNT: usize = 8;

/// Views selected from the number keys. `5` is the combined screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Essentials,
    Ram,
    Cpu,
    Disk,
    Network,
}

impl View {
    pub fn from_key(key: char) -> Option<Self> {
        match key {
            '5' => Some(Self::Essentials),
            '1' => Some(Self::Ram),
            '2' => Some(Self::Cpu),
            '3' => Some(Self::Disk),
            '4' => Some(Self::Network),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Essentials => "5 Essentials",
            Self::Ram => "1 RAM",
            Self::Cpu => "2 CPU",
            Self::Disk => "3 Disk",
            Self::Network => "4 Network",
        }
    }

    pub fn cycle() -> &'static [Self] {
        &[
            Self::Essentials,
            Self::Ram,
            Self::Cpu,
            Self::Disk,
            Self::Network,
        ]
    }

    pub fn next(self) -> Self {
        let order = Self::cycle();
        let index = order.iter().position(|view| *view == self).unwrap_or(0);
        order[(index + 1) % order.len()]
    }

    pub fn previous(self) -> Self {
        let order = Self::cycle();
        let index = order.iter().position(|view| *view == self).unwrap_or(0);
        order[(index + order.len() - 1) % order.len()]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub rates_ready: bool,
    pub cpu: CpuStats,
    pub memory: MemoryStats,
    pub volumes: Vec<VolumeStats>,
    pub network: NetworkStats,
    pub top_cpu: Vec<ProcessRow>,
    pub top_memory: Vec<ProcessRow>,
    pub top_disk: Vec<ProcessRow>,
    pub cpu_history: Vec<u64>,
    pub memory_history: Vec<u64>,
    pub net_down_history: Vec<u64>,
    pub net_up_history: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CpuStats {
    pub available: bool,
    pub usage_percent: Option<f64>,
    pub user_percent: Option<f64>,
    pub system_percent: Option<f64>,
    pub idle_percent: Option<f64>,
    pub efficiency_percent: Option<f64>,
    pub performance_percent: Option<f64>,
    pub cores: Vec<CoreStats>,
    pub load_one: f64,
    pub load_five: f64,
    pub load_fifteen: f64,
    pub uptime_seconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreCluster {
    Performance,
    Efficiency,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoreStats {
    pub usage_percent: f64,
    pub cluster: CoreCluster,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryStats {
    pub available: bool,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub app_bytes: u64,
    pub wired_bytes: u64,
    pub compressed_bytes: u64,
    pub free_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
}

impl MemoryStats {
    pub fn used_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.used_bytes as f64 * 100.0 / self.total_bytes as f64
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeStats {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub read_bytes_per_sec: Option<u64>,
    pub write_bytes_per_sec: Option<u64>,
    pub read_history: Vec<u64>,
    pub write_history: Vec<u64>,
}

impl VolumeStats {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkStats {
    pub interface_name: String,
    pub is_up: bool,
    pub local_ip: Option<String>,
    pub mac: Option<String>,
    pub down_bytes_per_sec: Option<u64>,
    pub up_bytes_per_sec: Option<u64>,
    pub down_total_bytes: u64,
    pub up_total_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRow {
    pub name: String,
    pub pid: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
}

/// Page counters from `host_statistics64`, in pages rather than bytes.
#[derive(Clone, Copy, Debug)]
pub struct VmPages {
    pub active: u64,
    pub inactive: u64,
    pub speculative: u64,
    pub wired: u64,
    pub compressed: u64,
    pub purgeable: u64,
    pub external: u64,
    pub page_size: u64,
    pub total_bytes: u64,
}

/// Memory split used by the menu-bar Stats app.
///
/// `used = active + inactive + speculative + wired + compressed - purgeable - external`.
/// App memory is used minus wired and compressed. Free is the rest of installed RAM,
/// so the four bars add up to the total.
pub fn memory_from_pages(pages: VmPages) -> MemoryStats {
    let bytes = |count: u64| count.saturating_mul(pages.page_size);
    let raw_used = bytes(pages.active)
        .saturating_add(bytes(pages.inactive))
        .saturating_add(bytes(pages.speculative))
        .saturating_add(bytes(pages.wired))
        .saturating_add(bytes(pages.compressed));
    let deducted = bytes(pages.purgeable).saturating_add(bytes(pages.external));
    let total = pages.total_bytes;
    let used = raw_used.saturating_sub(deducted).min(total);
    let wired = bytes(pages.wired).min(used);
    let compressed = bytes(pages.compressed).min(used.saturating_sub(wired));
    let app = used.saturating_sub(wired).saturating_sub(compressed);
    let free = total.saturating_sub(used);
    MemoryStats {
        available: total > 0,
        total_bytes: total,
        used_bytes: used,
        app_bytes: app,
        wired_bytes: wired,
        compressed_bytes: compressed,
        free_bytes: free,
        swap_used_bytes: 0,
        swap_total_bytes: 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuTicks {
    pub user: u64,
    pub system: u64,
    pub idle: u64,
    pub nice: u64,
}

/// User, system, and idle shares between two tick samples.
/// Nice time is counted as user, matching Activity Monitor.
/// Returns `(busy, user, system, idle)` percents.
pub fn cpu_percents(previous: CpuTicks, current: CpuTicks) -> Option<(f64, f64, f64, f64)> {
    let user = (current.user + current.nice).saturating_sub(previous.user + previous.nice);
    let system = current.system.saturating_sub(previous.system);
    let idle = current.idle.saturating_sub(previous.idle);
    let total = user + system + idle;
    if total == 0 {
        return None;
    }
    let percent = |ticks: u64| ticks as f64 * 100.0 / total as f64;
    let user_percent = percent(user);
    let system_percent = percent(system);
    let idle_percent = percent(idle);
    Some((
        user_percent + system_percent,
        user_percent,
        system_percent,
        idle_percent,
    ))
}

/// On Apple Silicon, Mach lists performance cores first and efficiency cores after them.
pub fn cluster_for(
    index: usize,
    performance_cores: Option<usize>,
    efficiency_cores: Option<usize>,
    total_cores: usize,
) -> CoreCluster {
    match (performance_cores, efficiency_cores) {
        (Some(performance), Some(efficiency))
            if performance > 0 && efficiency > 0 && performance + efficiency == total_cores =>
        {
            if index < performance {
                CoreCluster::Performance
            } else {
                CoreCluster::Efficiency
            }
        }
        _ => CoreCluster::Unknown,
    }
}

pub fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

pub fn bytes_per_second(previous: u64, current: u64, elapsed: Duration) -> Option<u64> {
    if current < previous {
        return None;
    }
    let seconds = elapsed.as_secs_f64();
    if seconds < 0.05 {
        return None;
    }
    Some(((current - previous) as f64 / seconds).round() as u64)
}

pub fn scale_delta(delta: u64, elapsed: Duration) -> u64 {
    let seconds = elapsed.as_secs_f64();
    if seconds < 0.05 {
        delta
    } else {
        (delta as f64 / seconds).round() as u64
    }
}

/// Finder-visible volumes: the startup disk and disks mounted under `/Volumes`.
pub fn is_user_volume(mount: &str) -> bool {
    if mount == "/" {
        return true;
    }
    match mount.strip_prefix("/Volumes/") {
        Some(name) => !name.is_empty() && !name.contains('/'),
        None => false,
    }
}

pub fn is_virtual_interface(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "lo", "awdl", "llw", "utun", "bridge", "gif", "stf", "ap", "anpi", "ipsec", "pktap", "XHC",
    ];
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfaceInfo {
    pub name: String,
    pub is_up: bool,
    pub ipv4: Option<String>,
    pub mac: Option<String>,
}

pub fn choose_interface(ifaces: &[IfaceInfo], primary_ip: Option<&str>) -> Option<usize> {
    if let Some(ip) = primary_ip
        && let Some(index) = ifaces
            .iter()
            .position(|iface| iface.ipv4.as_deref() == Some(ip))
    {
        return Some(index);
    }
    let mut indexes: Vec<usize> = ifaces
        .iter()
        .enumerate()
        .filter_map(|(index, iface)| {
            if is_virtual_interface(&iface.name) || iface.ipv4.is_none() {
                None
            } else {
                Some(index)
            }
        })
        .collect();
    indexes.sort_by(|&left, &right| {
        let left_iface = &ifaces[left];
        let right_iface = &ifaces[right];
        let left_rank = iface_rank(left_iface);
        let right_rank = iface_rank(right_iface);
        left_rank
            .cmp(&right_rank)
            .then(left_iface.name.cmp(&right_iface.name))
    });
    indexes.into_iter().next()
}

fn iface_rank(iface: &IfaceInfo) -> (u8, u8) {
    let up = if iface.is_up { 0 } else { 1 };
    let ethernet = if iface.name.starts_with("en") { 0 } else { 1 };
    (up, ethernet)
}

/// Map `disk3s1s1` to a counter recorded under that name or a parent BSD name.
pub fn lookup_disk_counter(
    counters: &HashMap<String, (u64, u64)>,
    bsd_name: &str,
) -> Option<(u64, u64)> {
    let mut current = bsd_name.to_string();
    loop {
        if let Some(counter) = counters.get(&current) {
            return Some(*counter);
        }
        let split_at = current.rfind('s')?;
        let tail = &current[split_at + 1..];
        if tail.is_empty() || !tail.bytes().all(|byte| byte.is_ascii_digit()) || split_at == 0 {
            return None;
        }
        current.truncate(split_at);
    }
}

#[derive(Clone, Debug)]
pub struct History {
    values: VecDeque<u64>,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, value: u64) {
        if self.capacity == 0 {
            return;
        }
        if self.values.len() == self.capacity {
            self.values.pop_front();
        }
        self.values.push_back(value);
    }

    pub fn samples(&self) -> Vec<u64> {
        self.values.iter().copied().collect()
    }
}

/// Largest-remainder widths that add up to `total_width`.
pub fn bar_widths(total_width: usize, parts: &[u64]) -> Vec<usize> {
    let sum: u64 = parts.iter().copied().sum();
    if total_width == 0 || sum == 0 {
        return vec![0; parts.len()];
    }
    let mut widths = Vec::with_capacity(parts.len());
    let mut remainders = Vec::with_capacity(parts.len());
    let mut used = 0usize;
    for (index, part) in parts.iter().copied().enumerate() {
        let exact = part as f64 * total_width as f64 / sum as f64;
        let base = exact.floor() as usize;
        used += base;
        widths.push(base);
        remainders.push((exact - base as f64, index));
    }
    remainders.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut left = total_width - used;
    for (_, index) in remainders {
        if left == 0 {
            break;
        }
        if parts[index] > 0 {
            widths[index] += 1;
            left -= 1;
        }
    }
    widths
}

#[cfg(test)]
const GIB: u64 = 1024 * 1024 * 1024;

#[cfg(test)]
pub fn example_snapshot() -> Snapshot {
    let app = 43 * GIB;
    let wired = 7 * GIB;
    let compressed = 44 * GIB;
    let free = 34 * GIB;
    Snapshot {
        rates_ready: true,
        cpu: CpuStats {
            available: true,
            usage_percent: Some(18.0),
            user_percent: Some(13.0),
            system_percent: Some(5.0),
            idle_percent: Some(82.0),
            efficiency_percent: Some(28.0),
            performance_percent: Some(15.0),
            cores: vec![
                CoreStats {
                    usage_percent: 20.0,
                    cluster: CoreCluster::Performance,
                },
                CoreStats {
                    usage_percent: 10.0,
                    cluster: CoreCluster::Performance,
                },
                CoreStats {
                    usage_percent: 40.0,
                    cluster: CoreCluster::Efficiency,
                },
                CoreStats {
                    usage_percent: 16.0,
                    cluster: CoreCluster::Efficiency,
                },
            ],
            load_one: 6.41,
            load_five: 5.46,
            load_fifteen: 5.93,
            uptime_seconds: 3 * 3600 + 2 * 60,
        },
        memory: MemoryStats {
            available: true,
            total_bytes: app + wired + compressed + free,
            used_bytes: app + wired + compressed,
            app_bytes: app,
            wired_bytes: wired,
            compressed_bytes: compressed,
            free_bytes: free,
            swap_used_bytes: 17 * 1024 * 1024,
            swap_total_bytes: 2 * GIB,
        },
        volumes: vec![VolumeStats {
            name: "Macintosh HD".to_string(),
            mount_point: "/".to_string(),
            total_bytes: 2 * 1024 * GIB,
            available_bytes: 209 * GIB,
            read_bytes_per_sec: Some(1_500_000),
            write_bytes_per_sec: Some(16 * 1024),
            read_history: vec![100, 400, 200],
            write_history: vec![10, 20, 16],
        }],
        network: NetworkStats {
            interface_name: "en13".to_string(),
            is_up: true,
            local_ip: Some("192.168.68.76".to_string()),
            mac: Some("4c:c5:d9:d5:11:d3".to_string()),
            down_bytes_per_sec: Some(20 * 1024),
            up_bytes_per_sec: Some(10 * 1024),
            down_total_bytes: 265 * 1024 * 1024,
            up_total_bytes: 54 * 1024 * 1024,
        },
        top_cpu: vec![ProcessRow {
            name: "Docker".to_string(),
            pid: 100,
            cpu_percent: 105.9,
            memory_bytes: 61 * GIB,
            disk_read_bytes_per_sec: 66 * 1024,
            disk_write_bytes_per_sec: 4_800_000,
        }],
        top_memory: vec![ProcessRow {
            name: "Docker".to_string(),
            pid: 100,
            cpu_percent: 105.9,
            memory_bytes: 61 * GIB,
            disk_read_bytes_per_sec: 66 * 1024,
            disk_write_bytes_per_sec: 4_800_000,
        }],
        top_disk: vec![ProcessRow {
            name: "Docker".to_string(),
            pid: 100,
            cpu_percent: 105.9,
            memory_bytes: 61 * GIB,
            disk_read_bytes_per_sec: 66 * 1024,
            disk_write_bytes_per_sec: 4_800_000,
        }],
        cpu_history: vec![10, 18, 16, 22],
        memory_history: vec![70, 72, 73, 73],
        net_down_history: vec![1000, 20_000, 18_000],
        net_up_history: vec![500, 10_000, 9_000],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_keys_select_views_and_arrows_cycle_in_tab_order() {
        assert_eq!(View::from_key('5'), Some(View::Essentials));
        assert_eq!(View::from_key('1'), Some(View::Ram));
        assert_eq!(View::from_key('4'), Some(View::Network));
        assert_eq!(View::from_key('q'), None);
        assert_eq!(View::Essentials.next(), View::Ram);
        assert_eq!(View::Network.next(), View::Essentials);
        assert_eq!(View::Ram.previous(), View::Essentials);
    }

    #[test]
    fn memory_parts_sum_to_installed_total() {
        let stats = memory_from_pages(VmPages {
            active: 100,
            inactive: 50,
            speculative: 10,
            wired: 20,
            compressed: 30,
            purgeable: 5,
            external: 15,
            page_size: 1000,
            total_bytes: 1_000_000,
        });
        assert_eq!(stats.used_bytes, 190_000);
        assert_eq!(stats.wired_bytes, 20_000);
        assert_eq!(stats.compressed_bytes, 30_000);
        assert_eq!(stats.app_bytes, 140_000);
        assert_eq!(stats.free_bytes, 810_000);
        assert_eq!(
            stats.app_bytes + stats.wired_bytes + stats.compressed_bytes + stats.free_bytes,
            stats.total_bytes
        );
    }

    #[test]
    fn memory_used_cannot_exceed_installed_total() {
        let stats = memory_from_pages(VmPages {
            active: 50,
            inactive: 50,
            speculative: 0,
            wired: 10,
            compressed: 10,
            purgeable: 0,
            external: 0,
            page_size: 100,
            total_bytes: 1_000,
        });
        assert_eq!(stats.used_bytes, 1_000);
        assert_eq!(stats.free_bytes, 0);
        assert_eq!(
            stats.app_bytes + stats.wired_bytes + stats.compressed_bytes + stats.free_bytes,
            1_000
        );
    }

    #[test]
    fn cpu_percents_count_nice_time_as_user() {
        let previous = CpuTicks {
            user: 0,
            system: 0,
            idle: 0,
            nice: 0,
        };
        let current = CpuTicks {
            user: 10,
            system: 5,
            idle: 80,
            nice: 5,
        };
        let (busy, user, system, idle) = cpu_percents(previous, current).unwrap();
        assert!((busy - 20.0).abs() < 0.001);
        assert!((user - 15.0).abs() < 0.001);
        assert!((system - 5.0).abs() < 0.001);
        assert!((idle - 80.0).abs() < 0.001);
    }

    #[test]
    fn apple_silicon_cores_are_performance_then_efficiency() {
        assert_eq!(
            cluster_for(0, Some(2), Some(2), 4),
            CoreCluster::Performance
        );
        assert_eq!(cluster_for(2, Some(2), Some(2), 4), CoreCluster::Efficiency);
        assert_eq!(cluster_for(0, None, None, 8), CoreCluster::Unknown);
        assert_eq!(cluster_for(0, Some(2), Some(2), 8), CoreCluster::Unknown);
    }

    #[test]
    fn rates_use_elapsed_time_and_reject_a_counter_reset() {
        let elapsed = Duration::from_millis(500);
        assert_eq!(bytes_per_second(1_000, 2_500, elapsed), Some(3_000));
        assert_eq!(bytes_per_second(2_500, 1_000, elapsed), None);
        assert_eq!(scale_delta(1_500, elapsed), 3_000);
    }

    #[test]
    fn user_volumes_are_the_startup_disk_and_external_mounts() {
        assert!(is_user_volume("/"));
        assert!(is_user_volume("/Volumes/Vault"));
        assert!(!is_user_volume("/Volumes"));
        assert!(!is_user_volume("/System/Volumes/Data"));
        assert!(!is_user_volume("/Volumes/Vault/nested"));
    }

    #[test]
    fn interface_choice_prefers_the_primary_address_then_an_up_en_interface() {
        let ifaces = vec![
            IfaceInfo {
                name: "lo0".into(),
                is_up: true,
                ipv4: Some("127.0.0.1".into()),
                mac: None,
            },
            IfaceInfo {
                name: "awdl0".into(),
                is_up: true,
                ipv4: Some("169.254.1.1".into()),
                mac: None,
            },
            IfaceInfo {
                name: "en0".into(),
                is_up: false,
                ipv4: Some("10.0.0.2".into()),
                mac: None,
            },
            IfaceInfo {
                name: "en13".into(),
                is_up: true,
                ipv4: Some("192.168.68.76".into()),
                mac: None,
            },
        ];
        assert_eq!(choose_interface(&ifaces, Some("10.0.0.2")), Some(2));
        assert_eq!(choose_interface(&ifaces, None), Some(3));
        assert!(is_virtual_interface("utun0"));
        assert!(!is_virtual_interface("en13"));
    }

    #[test]
    fn disk_counter_lookup_walks_to_the_parent_bsd_name() {
        let mut counters = HashMap::new();
        counters.insert("disk3".to_string(), (10, 20));
        assert_eq!(lookup_disk_counter(&counters, "disk3s1s1"), Some((10, 20)));
        assert_eq!(lookup_disk_counter(&counters, "disk9"), None);
    }

    #[test]
    fn history_keeps_the_newest_samples() {
        let mut history = History::new(3);
        for value in [1, 2, 3, 4] {
            history.push(value);
        }
        assert_eq!(history.samples(), vec![2, 3, 4]);
    }

    #[test]
    fn bar_widths_cover_the_row() {
        let widths = bar_widths(10, &[50, 25, 25]);
        assert_eq!(widths.iter().sum::<usize>(), 10);
        assert_eq!(widths, vec![5, 3, 2]);
    }
}
