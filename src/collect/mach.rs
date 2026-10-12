use std::ffi::CString;
use std::mem::{self, size_of};

use libc::host_info64_t;

use crate::model::{CpuTicks, MemoryStats, VmPages, memory_from_pages};

pub struct MachHost {
    port: libc::mach_port_t,
}

impl MachHost {
    #[allow(deprecated)]
    pub fn open() -> Self {
        let port = unsafe { libc::mach_host_self() };
        Self { port }
    }

    pub fn read_cpu(&self) -> Option<CpuSample> {
        if self.port == 0 {
            return None;
        }
        let aggregate = read_aggregate(self.port)?;
        let cores = read_cores(self.port).unwrap_or_default();
        Some(CpuSample { aggregate, cores })
    }

    pub fn read_memory(&self) -> Option<MemoryStats> {
        if self.port == 0 {
            return None;
        }
        let pages = read_pages(self.port)?;
        let mut stats = memory_from_pages(pages);
        if let Some((used, total)) = read_swap() {
            stats.swap_used_bytes = used;
            stats.swap_total_bytes = total;
        }
        Some(stats)
    }
}

pub struct CpuSample {
    pub aggregate: CpuTicks,
    pub cores: Vec<CpuTicks>,
}

#[allow(deprecated)]
fn read_aggregate(host: libc::mach_port_t) -> Option<CpuTicks> {
    let mut info = unsafe { mem::zeroed::<libc::host_cpu_load_info>() };
    let mut count: libc::mach_msg_type_number_t = libc::HOST_CPU_LOAD_INFO_COUNT;
    let result = unsafe {
        libc::host_statistics(
            host,
            libc::HOST_CPU_LOAD_INFO,
            &mut info as *mut libc::host_cpu_load_info as libc::host_info_t,
            &mut count,
        )
    };
    if result != libc::KERN_SUCCESS {
        return None;
    }
    Some(ticks_from(&info.cpu_ticks))
}

#[allow(deprecated)]
fn read_cores(host: libc::mach_port_t) -> Option<Vec<CpuTicks>> {
    let mut cpu_count: libc::natural_t = 0;
    let mut info: libc::processor_info_array_t = std::ptr::null_mut();
    let mut info_count: libc::mach_msg_type_number_t = 0;
    let result = unsafe {
        libc::host_processor_info(
            host,
            libc::PROCESSOR_CPU_LOAD_INFO,
            &mut cpu_count,
            &mut info,
            &mut info_count,
        )
    };
    if result != libc::KERN_SUCCESS || info.is_null() || cpu_count == 0 {
        return None;
    }
    let guard = ProcessorInfo {
        ptr: info,
        count: info_count,
    };
    let records = unsafe {
        std::slice::from_raw_parts(
            guard.ptr as *const libc::processor_cpu_load_info,
            cpu_count as usize,
        )
    };
    Some(
        records
            .iter()
            .map(|record| CpuTicks {
                user: record.cpu_ticks[libc::CPU_STATE_USER as usize] as u64,
                system: record.cpu_ticks[libc::CPU_STATE_SYSTEM as usize] as u64,
                idle: record.cpu_ticks[libc::CPU_STATE_IDLE as usize] as u64,
                nice: record.cpu_ticks[libc::CPU_STATE_NICE as usize] as u64,
            })
            .collect(),
    )
}

struct ProcessorInfo {
    ptr: libc::processor_info_array_t,
    count: libc::mach_msg_type_number_t,
}

impl Drop for ProcessorInfo {
    #[allow(deprecated)]
    fn drop(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        let bytes = self.count as usize * size_of::<libc::integer_t>();
        unsafe {
            libc::vm_deallocate(
                libc::mach_task_self(),
                self.ptr as libc::vm_address_t,
                bytes as libc::vm_size_t,
            );
        }
    }
}

fn ticks_from(raw: &[libc::natural_t]) -> CpuTicks {
    CpuTicks {
        user: raw[libc::CPU_STATE_USER as usize] as u64,
        system: raw[libc::CPU_STATE_SYSTEM as usize] as u64,
        idle: raw[libc::CPU_STATE_IDLE as usize] as u64,
        nice: raw[libc::CPU_STATE_NICE as usize] as u64,
    }
}

#[allow(deprecated)]
fn read_pages(host: libc::mach_port_t) -> Option<VmPages> {
    let mut stats = unsafe { mem::zeroed::<libc::vm_statistics64>() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let result = unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            &mut stats as *mut libc::vm_statistics64 as host_info64_t,
            &mut count,
        )
    };
    if result != libc::KERN_SUCCESS {
        return None;
    }
    let page_size = page_size()?;
    let total_bytes = sysctl_u64("hw.memsize")?;
    Some(VmPages {
        active: stats.active_count as u64,
        inactive: stats.inactive_count as u64,
        speculative: stats.speculative_count as u64,
        wired: stats.wire_count as u64,
        compressed: stats.compressor_page_count as u64,
        purgeable: stats.purgeable_count as u64,
        external: stats.external_page_count as u64,
        page_size,
        total_bytes,
    })
}

fn read_swap() -> Option<(u64, u64)> {
    let key = CString::new("vm.swapusage").ok()?;
    let mut usage = unsafe { mem::zeroed::<libc::xsw_usage>() };
    let mut len = size_of::<libc::xsw_usage>();
    let result = unsafe {
        libc::sysctlbyname(
            key.as_ptr(),
            &mut usage as *mut libc::xsw_usage as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return None;
    }
    Some((usage.xsu_used, usage.xsu_total))
}

pub fn perf_core_counts() -> (Option<usize>, Option<usize>) {
    (
        sysctl_u32("hw.perflevel0.logicalcpu").map(|count| count as usize),
        sysctl_u32("hw.perflevel1.logicalcpu").map(|count| count as usize),
    )
}

fn page_size() -> Option<u64> {
    let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if value > 0 { Some(value as u64) } else { None }
}

fn sysctl_u64(name: &str) -> Option<u64> {
    let key = CString::new(name).ok()?;
    let mut value: u64 = 0;
    let mut len = size_of::<u64>();
    let result = unsafe {
        libc::sysctlbyname(
            key.as_ptr(),
            &mut value as *mut u64 as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if result == 0 { Some(value) } else { None }
}

fn sysctl_u32(name: &str) -> Option<u32> {
    let key = CString::new(name).ok()?;
    let mut value: u32 = 0;
    let mut len = size_of::<u32>();
    let result = unsafe {
        libc::sysctlbyname(
            key.as_ptr(),
            &mut value as *mut u32 as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if result == 0 { Some(value) } else { None }
}
