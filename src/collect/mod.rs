mod disk_io;
mod mach;

use std::collections::HashMap;
use std::ffi::CString;
use std::net::{IpAddr, UdpSocket};
use std::time::{Duration, Instant};

use sysinfo::{
    Disks, InterfaceOperationalState, Networks, ProcessRefreshKind, ProcessesToUpdate, System,
};

use crate::model::{
    CoreCluster, CoreStats, CpuStats, CpuTicks, HISTORY_LEN, History, IfaceInfo, NetworkStats,
    ProcessRow, Snapshot, TOP_PROCESS_COUNT, VolumeStats, bytes_per_second, choose_interface,
    cluster_for, cpu_percents, is_user_volume, lookup_disk_counter, mean, scale_delta,
};
use mach::{CpuSample, MachHost, perf_core_counts};

pub struct Sampler {
    host: MachHost,
    system: System,
    networks: Networks,
    disks: Disks,
    performance_cores: Option<usize>,
    efficiency_cores: Option<usize>,
    previous_cpu: Option<CpuTicks>,
    previous_cores: Option<Vec<CpuTicks>>,
    previous_disk: HashMap<String, (u64, u64)>,
    last_sample: Option<Instant>,
    net_interface: Option<String>,
    net_down_total: u64,
    net_up_total: u64,
    cpu_history: History,
    memory_history: History,
    net_down_history: History,
    net_up_history: History,
    disk_read_history: HashMap<String, History>,
    disk_write_history: HashMap<String, History>,
}

impl Sampler {
    pub fn new() -> Self {
        let (performance_cores, efficiency_cores) = perf_core_counts();
        Self {
            host: MachHost::open(),
            system: System::new(),
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            performance_cores,
            efficiency_cores,
            previous_cpu: None,
            previous_cores: None,
            previous_disk: HashMap::new(),
            last_sample: None,
            net_interface: None,
            net_down_total: 0,
            net_up_total: 0,
            cpu_history: History::new(HISTORY_LEN),
            memory_history: History::new(HISTORY_LEN),
            net_down_history: History::new(HISTORY_LEN),
            net_up_history: History::new(HISTORY_LEN),
            disk_read_history: HashMap::new(),
            disk_write_history: HashMap::new(),
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
        let elapsed = self
            .last_sample
            .map(|then| now.saturating_duration_since(then))
            .unwrap_or(Duration::from_secs(1));
        let rates_ready = self.last_sample.is_some();

        let cpu_sample = self.host.read_cpu();
        let memory = self
            .host
            .read_memory()
            .unwrap_or(MemoryUnavailable::stats());
        if memory.available {
            let used_percent = memory.used_percent().round().clamp(0.0, 100.0) as u64;
            self.memory_history.push(used_percent);
        }

        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage(),
        );
        self.networks.refresh(true);
        self.disks.refresh(true);

        let disk_counters = disk_io::sample_disk_counters();
        let cpu = self.cpu_stats(cpu_sample.as_ref(), rates_ready);
        let processes = process_rows(&self.system, rates_ready, elapsed);
        let mut volumes = self.volumes(&disk_counters, rates_ready, elapsed);
        let network = self.network(rates_ready, elapsed);

        self.previous_disk = disk_counters;
        if let Some(sample) = cpu_sample {
            self.previous_cpu = Some(sample.aggregate);
            self.previous_cores = Some(sample.cores);
        }
        self.last_sample = Some(now);

        for volume in &mut volumes {
            volume.read_history = self
                .disk_read_history
                .get(&volume.mount_point)
                .map(History::samples)
                .unwrap_or_default();
            volume.write_history = self
                .disk_write_history
                .get(&volume.mount_point)
                .map(History::samples)
                .unwrap_or_default();
        }

        Snapshot {
            rates_ready,
            cpu,
            memory,
            volumes,
            network,
            top_cpu: top_by(processes.clone(), TOP_PROCESS_COUNT, |row| {
                (row.cpu_percent.max(0.0) * 10.0) as u64
            }),
            top_memory: top_by(processes.clone(), TOP_PROCESS_COUNT, |row| row.memory_bytes),
            top_disk: top_by(processes, TOP_PROCESS_COUNT, |row| {
                row.disk_read_bytes_per_sec
                    .saturating_add(row.disk_write_bytes_per_sec)
            }),
            cpu_history: self.cpu_history.samples(),
            memory_history: self.memory_history.samples(),
            net_down_history: self.net_down_history.samples(),
            net_up_history: self.net_up_history.samples(),
        }
    }

    fn cpu_stats(&mut self, sample: Option<&CpuSample>, rates_ready: bool) -> CpuStats {
        let load = System::load_average();
        let mut stats = CpuStats {
            available: sample.is_some(),
            usage_percent: None,
            user_percent: None,
            system_percent: None,
            idle_percent: None,
            efficiency_percent: None,
            performance_percent: None,
            cores: Vec::new(),
            load_one: load.one,
            load_five: load.five,
            load_fifteen: load.fifteen,
            uptime_seconds: System::uptime(),
        };
        let Some(sample) = sample else {
            return stats;
        };
        if let (true, Some(previous)) = (rates_ready, self.previous_cpu)
            && let Some((busy, user, system, idle)) = cpu_percents(previous, sample.aggregate)
        {
            stats.usage_percent = Some(busy);
            stats.user_percent = Some(user);
            stats.system_percent = Some(system);
            stats.idle_percent = Some(idle);
            self.cpu_history.push(busy.round().clamp(0.0, 100.0) as u64);
        }
        let previous_cores = self.previous_cores.as_deref();
        if rates_ready && let Some(previous_cores) = previous_cores {
            let total = sample.cores.len();
            for (index, current) in sample.cores.iter().enumerate() {
                let Some(previous) = previous_cores.get(index) else {
                    continue;
                };
                let Some((busy, _, _, _)) = cpu_percents(*previous, *current) else {
                    continue;
                };
                stats.cores.push(CoreStats {
                    usage_percent: busy,
                    cluster: cluster_for(
                        index,
                        self.performance_cores,
                        self.efficiency_cores,
                        total,
                    ),
                });
            }
            stats.performance_percent = mean_cluster(&stats.cores, CoreCluster::Performance);
            stats.efficiency_percent = mean_cluster(&stats.cores, CoreCluster::Efficiency);
        }
        stats
    }

    fn volumes(
        &mut self,
        counters: &HashMap<String, (u64, u64)>,
        rates_ready: bool,
        elapsed: Duration,
    ) -> Vec<VolumeStats> {
        let mounted: Vec<MountedVolume> = self
            .disks
            .list()
            .iter()
            .filter_map(|disk| {
                let mount = disk.mount_point().to_string_lossy().to_string();
                if !is_user_volume(&mount) || disk.total_space() == 0 {
                    return None;
                }
                let name = disk.name().to_string_lossy().trim().to_string();
                let name = if name.is_empty() {
                    disk.mount_point()
                        .file_name()
                        .map(|part| part.to_string_lossy().to_string())
                        .unwrap_or_else(|| mount.clone())
                } else {
                    name
                };
                Some(MountedVolume {
                    name,
                    mount: mount.clone(),
                    total_bytes: disk.total_space(),
                    available_bytes: disk.available_space(),
                    bsd_name: bsd_name_for_mount(disk.mount_point()),
                })
            })
            .collect();

        let mut volumes = Vec::new();
        let mut next_read = HashMap::new();
        let mut next_write = HashMap::new();
        for disk in mounted {
            let current = disk
                .bsd_name
                .as_deref()
                .and_then(|name| lookup_disk_counter(counters, name));
            let (read_rate, write_rate) = match (rates_ready, current, disk.bsd_name.as_deref()) {
                (true, Some((read_bytes, write_bytes)), Some(name)) => {
                    match self.previous_disk.get(name).copied().or_else(|| {
                        // Parent counters are stored under every walked BSD name.
                        lookup_disk_counter(&self.previous_disk, name)
                    }) {
                        Some((previous_read, previous_write)) => (
                            bytes_per_second(previous_read, read_bytes, elapsed),
                            bytes_per_second(previous_write, write_bytes, elapsed),
                        ),
                        None => (None, None),
                    }
                }
                _ => (None, None),
            };
            let mut read_history = self
                .disk_read_history
                .remove(&disk.mount)
                .unwrap_or_else(|| History::new(HISTORY_LEN));
            let mut write_history = self
                .disk_write_history
                .remove(&disk.mount)
                .unwrap_or_else(|| History::new(HISTORY_LEN));
            if let Some(rate) = read_rate {
                read_history.push(rate);
            }
            if let Some(rate) = write_rate {
                write_history.push(rate);
            }
            next_read.insert(disk.mount.clone(), read_history);
            next_write.insert(disk.mount.clone(), write_history);
            volumes.push(VolumeStats {
                name: disk.name,
                mount_point: disk.mount,
                total_bytes: disk.total_bytes,
                available_bytes: disk.available_bytes,
                read_bytes_per_sec: read_rate,
                write_bytes_per_sec: write_rate,
                read_history: Vec::new(),
                write_history: Vec::new(),
            });
        }
        self.disk_read_history = next_read;
        self.disk_write_history = next_write;
        volumes.sort_by(|left, right| {
            mount_rank(&left.mount_point)
                .cmp(&mount_rank(&right.mount_point))
                .then(left.name.cmp(&right.name))
        });
        volumes
    }

    fn network(&mut self, rates_ready: bool, elapsed: Duration) -> NetworkStats {
        let ifaces = interface_list(&self.networks);
        let primary = primary_ipv4();
        let chosen = choose_interface(&ifaces, primary.as_deref()).map(|index| &ifaces[index]);
        let Some(iface) = chosen else {
            return empty_network();
        };
        if self.net_interface.as_deref() != Some(iface.name.as_str()) {
            self.net_interface = Some(iface.name.clone());
            self.net_down_total = 0;
            self.net_up_total = 0;
            self.net_down_history = History::new(HISTORY_LEN);
            self.net_up_history = History::new(HISTORY_LEN);
        }
        let data = self.networks.get(&iface.name);
        let (down_rate, up_rate) = if rates_ready {
            if let Some(data) = data {
                let down = data.received();
                let up = data.transmitted();
                self.net_down_total = self.net_down_total.saturating_add(down);
                self.net_up_total = self.net_up_total.saturating_add(up);
                let down_rate = scale_delta(down, elapsed);
                let up_rate = scale_delta(up, elapsed);
                self.net_down_history.push(down_rate);
                self.net_up_history.push(up_rate);
                (Some(down_rate), Some(up_rate))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        NetworkStats {
            interface_name: iface.name.clone(),
            is_up: iface.is_up,
            local_ip: iface.ipv4.clone(),
            mac: iface.mac.clone(),
            down_bytes_per_sec: down_rate,
            up_bytes_per_sec: up_rate,
            down_total_bytes: self.net_down_total,
            up_total_bytes: self.net_up_total,
        }
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

struct MountedVolume {
    name: String,
    mount: String,
    total_bytes: u64,
    available_bytes: u64,
    bsd_name: Option<String>,
}

struct MemoryUnavailable;

impl MemoryUnavailable {
    fn stats() -> crate::model::MemoryStats {
        crate::model::MemoryStats {
            available: false,
            total_bytes: 0,
            used_bytes: 0,
            app_bytes: 0,
            wired_bytes: 0,
            compressed_bytes: 0,
            free_bytes: 0,
            swap_used_bytes: 0,
            swap_total_bytes: 0,
        }
    }
}

fn mean_cluster(cores: &[CoreStats], cluster: CoreCluster) -> Option<f64> {
    let values: Vec<f64> = cores
        .iter()
        .filter(|core| core.cluster == cluster)
        .map(|core| core.usage_percent)
        .collect();
    mean(&values)
}

fn process_rows(system: &System, rates_ready: bool, elapsed: Duration) -> Vec<ProcessRow> {
    system
        .processes()
        .iter()
        .map(|(pid, process)| {
            let mut name = process.name().to_string_lossy().trim().to_string();
            if name.is_empty() {
                name = format!("pid {pid}");
            }
            let disk = process.disk_usage();
            ProcessRow {
                name,
                pid: pid.as_u32(),
                cpu_percent: if rates_ready {
                    process.cpu_usage().max(0.0)
                } else {
                    0.0
                },
                memory_bytes: process.memory(),
                disk_read_bytes_per_sec: if rates_ready {
                    scale_delta(disk.read_bytes, elapsed)
                } else {
                    0
                },
                disk_write_bytes_per_sec: if rates_ready {
                    scale_delta(disk.written_bytes, elapsed)
                } else {
                    0
                },
            }
        })
        .collect()
}

fn top_by(
    mut rows: Vec<ProcessRow>,
    limit: usize,
    score: impl Fn(&ProcessRow) -> u64,
) -> Vec<ProcessRow> {
    rows.sort_by(|left, right| {
        score(right)
            .cmp(&score(left))
            .then_with(|| left.name.cmp(&right.name))
    });
    rows.truncate(limit);
    rows
}

fn mount_rank(mount: &str) -> u8 {
    if mount == "/" { 0 } else { 1 }
}

fn bsd_name_for_mount(mount: &std::path::Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(mount.as_os_str().as_bytes()).ok()?;
    let mut info = unsafe { std::mem::zeroed::<libc::statfs>() };
    if unsafe { libc::statfs(path.as_ptr(), &mut info) } != 0 {
        return None;
    }
    let source = c_chars_to_string(&info.f_mntfromname);
    let name = source.strip_prefix("/dev/").unwrap_or(&source);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn c_chars_to_string(bytes: &[libc::c_char]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let owned: Vec<u8> = bytes[..end].iter().map(|byte| *byte as u8).collect();
    String::from_utf8_lossy(&owned).into_owned()
}

fn interface_list(networks: &Networks) -> Vec<IfaceInfo> {
    networks
        .iter()
        .map(|(name, data)| {
            let ipv4 = data
                .ip_networks()
                .iter()
                .find(|network| {
                    matches!(network.addr, IpAddr::V4(addr) if !addr.is_loopback() && !addr.is_unspecified())
                })
                .map(|network| network.addr.to_string());
            let mac = data.mac_address();
            IfaceInfo {
                name: name.clone(),
                is_up: matches!(
                    data.operational_state(),
                    InterfaceOperationalState::Up | InterfaceOperationalState::Dormant
                ),
                ipv4,
                mac: if mac.is_unspecified() {
                    None
                } else {
                    Some(mac.to_string())
                },
            }
        })
        .collect()
}

fn primary_ipv4() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(addr) if !addr.is_unspecified() && !addr.is_loopback() => Some(addr.to_string()),
        _ => None,
    }
}

fn empty_network() -> NetworkStats {
    NetworkStats {
        interface_name: String::new(),
        is_up: false,
        local_ip: None,
        mac: None,
        down_bytes_per_sec: None,
        up_bytes_per_sec: None,
        down_total_bytes: 0,
        up_total_bytes: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_reads_this_mac() {
        let mut sampler = Sampler::new();
        let first = sampler.sample();
        assert!(
            first.memory.total_bytes > 1024 * 1024 * 1024,
            "installed memory was {}",
            first.memory.total_bytes
        );
        assert_eq!(
            first.memory.app_bytes
                + first.memory.wired_bytes
                + first.memory.compressed_bytes
                + first.memory.free_bytes,
            first.memory.total_bytes
        );
        assert!(first.cpu.available);
        assert!(first.cpu.usage_percent.is_none());
        assert!(!first.volumes.is_empty(), "no user volumes");
        assert!(!first.network.interface_name.is_empty(), "no interface");
        assert!(!first.top_memory.is_empty(), "no processes");

        std::thread::sleep(Duration::from_millis(400));
        let second = sampler.sample();
        assert!(second.rates_ready);
        let usage = second.cpu.usage_percent.expect("cpu delta");
        assert!((0.0..=100.0).contains(&usage), "cpu {usage}");
        assert!(!second.cpu.cores.is_empty(), "no per-core samples");
        let user = second.cpu.user_percent.unwrap_or(0.0);
        let system = second.cpu.system_percent.unwrap_or(0.0);
        let idle = second.cpu.idle_percent.unwrap_or(0.0);
        assert!((user + system + idle - 100.0).abs() < 0.2);
    }
}
