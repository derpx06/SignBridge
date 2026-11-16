//! System and process memory readings (cheap enough to poll every couple of seconds).

use crate::policy::Memory;
use sysinfo::{MemoryRefreshKind, System};

pub struct Monitor {
    system: System,
}

impl Monitor {
    pub fn new() -> Self {
        Monitor { system: System::new() }
    }

    pub fn sample(&mut self) -> Memory {
        self.system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let mb = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
        #[allow(unused_mut)] // only the debug-build test hook below changes it
        let mut memory = Memory {
            available_mb: mb(self.system.available_memory()),
            total_mb: mb(self.system.total_memory()),
            psi_some_avg10: psi_some_avg10(),
        };
        // Test hook (debug builds only): simulate a machine that is short on memory.
        #[cfg(debug_assertions)]
        if let Some(fake) = std::env::var("VOICIO_FAKE_AVAILABLE_MB").ok().and_then(|v| v.parse().ok()) {
            memory.available_mb = fake;
        }
        memory
    }
}

#[cfg(target_os = "linux")]
fn psi_some_avg10() -> Option<f64> {
    let text = std::fs::read_to_string("/proc/pressure/memory").ok()?;
    let some = text.lines().find(|l| l.starts_with("some "))?;
    some.split_whitespace().find_map(|field| field.strip_prefix("avg10=")).and_then(|v| v.parse().ok())
}

#[cfg(not(target_os = "linux"))]
fn psi_some_avg10() -> Option<f64> {
    None
}

/// This process's resident and peak memory in MB.
#[cfg(target_os = "linux")]
pub fn own_rss_mb() -> (f64, f64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| {
        status
            .lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|kb| kb.parse::<f64>().ok())
            .map_or(0.0, |kb| kb / 1024.0)
    };
    (field("VmRSS:"), field("VmHWM:"))
}

#[cfg(not(target_os = "linux"))]
pub fn own_rss_mb() -> (f64, f64) {
    let mut system = System::new();
    let pid = sysinfo::get_current_pid().ok();
    if let Some(pid) = pid {
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            false,
            sysinfo::ProcessRefreshKind::nothing().with_memory(),
        );
    }
    let rss = pid.and_then(|p| system.process(p)).map_or(0.0, |p| p.memory() as f64 / (1024.0 * 1024.0));
    (rss, rss)
}
