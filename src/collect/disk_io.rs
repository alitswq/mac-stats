use std::collections::HashMap;

const CAPACITY: usize = 128;

#[repr(C)]
#[derive(Clone, Copy)]
struct DiskIoCounter {
    bsd_name: [u8; 32],
    bytes_read: u64,
    bytes_written: u64,
}

unsafe extern "C" {
    fn mac_stats_sample_disk_io(out: *mut DiskIoCounter, capacity: i32) -> i32;
}

pub fn sample_disk_counters() -> HashMap<String, (u64, u64)> {
    let mut raw = [DiskIoCounter {
        bsd_name: [0; 32],
        bytes_read: 0,
        bytes_written: 0,
    }; CAPACITY];
    let count = unsafe { mac_stats_sample_disk_io(raw.as_mut_ptr(), CAPACITY as i32) };
    let count = count.max(0) as usize;
    let mut counters = HashMap::new();
    for counter in raw.into_iter().take(count.min(CAPACITY)) {
        let end = counter
            .bsd_name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(counter.bsd_name.len());
        let name = String::from_utf8_lossy(&counter.bsd_name[..end]).to_string();
        if !name.is_empty() {
            counters.insert(name, (counter.bytes_read, counter.bytes_written));
        }
    }
    counters
}
