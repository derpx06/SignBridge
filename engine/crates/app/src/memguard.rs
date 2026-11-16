//! Low-memory guard: one cheap sample, a pure three-level classifier with hysteresis, and the
//! action to take. Complements `policy.rs` (binary pressure -> unload); see docs/research/low-memory.md.
#![allow(dead_code)] // some helpers (`languages`, `sample`, `prefer_oom_victim`) are for callers outside the agent

use voicio_models::Tier;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapshot {
    pub available_mb: f32,
    pub total_mb: f32,
    /// Linux PSI `some avg10` (% of the last 10 s a task stalled on memory). None elsewhere.
    pub psi_some_avg10: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pressure {
    Ok,
    Low,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Proceed,
    /// Use rule-based cleanup this time; do not load the cleanup LLM.
    SkipCleanupModel,
    /// Dictate, then end the worker so every model byte is returned.
    UnloadAfterUse,
    /// Do not start; show this text in the capsule.
    RefuseWithNotice(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub low_mb: f32,
    pub critical_mb: f32,
    /// Also trip when available is below this fraction of total.
    pub low_frac: f32,
    pub critical_frac: f32,
    pub psi_low: f32,
    pub psi_critical: f32,
    /// To leave a level, available must exceed the entry threshold times this, and PSI must be
    /// below half of its entry threshold. Prevents the notice flapping.
    pub exit_margin: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config { low_mb: 1500.0, critical_mb: 700.0, low_frac: 0.10, critical_frac: 0.04, psi_low: 10.0, psi_critical: 30.0, exit_margin: 1.25 }
    }
}

/// Read the machine's memory state (~50 us on Linux; poll every 1-2 s).
pub fn sample() -> Snapshot {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let (available_mb, total_mb) = parse_meminfo(&text);
        let psi = std::fs::read_to_string("/proc/pressure/memory").ok().and_then(|t| parse_psi(&t));
        Snapshot { available_mb, total_mb, psi_some_avg10: psi }
    }
    #[cfg(not(target_os = "linux"))]
    {
        // sysinfo wraps GlobalMemoryStatusEx (Windows) and host_statistics64 (macOS), already a dependency.
        // TODO: macOS DISPATCH_SOURCE_TYPE_MEMORYPRESSURE and Windows QueryMemoryResourceNotification
        // would be event-driven; the available-bytes number is the portable fallback.
        use sysinfo::{MemoryRefreshKind, System};
        let mut s = System::new();
        s.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let mb = |b: u64| (b / (1024 * 1024)) as f32;
        Snapshot { available_mb: mb(s.available_memory()), total_mb: mb(s.total_memory()), psi_some_avg10: None }
    }
}

/// (MemAvailable, MemTotal) in MB from /proc/meminfo text. Missing MemAvailable -> 0 (treated as critical).
fn parse_meminfo(text: &str) -> (f32, f32) {
    let kb = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|r| r.split_whitespace().next())
            .and_then(|v| v.parse::<f32>().ok())
            .map_or(0.0, |k| k / 1024.0)
    };
    (kb("MemAvailable:"), kb("MemTotal:"))
}

fn parse_psi(text: &str) -> Option<f32> {
    let some = text.lines().find(|l| l.starts_with("some "))?;
    some.split_whitespace().find_map(|f| f.strip_prefix("avg10=")).and_then(|v| v.parse().ok())
}

/// Classify the snapshot. `loaded_mb` is what the next step will add (about 750 for a cold standard
/// start, 0 when the models are already resident); it is subtracted from what is available.
/// `prev` supplies hysteresis: pass the last result (start with `Pressure::Ok`).
pub fn classify(s: &Snapshot, loaded_mb: f32, c: &Config, prev: Pressure) -> Pressure {
    let avail = s.available_mb - loaded_mb;
    let psi = s.psi_some_avg10.unwrap_or(0.0);
    let level = |mb: f32, frac: f32, psi_at: f32, prev_here: bool| {
        let m = if prev_here { c.exit_margin } else { 1.0 };
        let p = if prev_here { 0.5 } else { 1.0 };
        avail < mb * m || avail < s.total_mb * frac * m || psi > psi_at * p
    };
    if level(c.critical_mb, c.critical_frac, c.psi_critical, prev == Pressure::Critical) {
        Pressure::Critical
    } else if level(c.low_mb, c.low_frac, c.psi_low, prev >= Pressure::Low) {
        Pressure::Low
    } else {
        Pressure::Ok
    }
}

pub fn advice(p: Pressure, tier: Tier) -> Action {
    match (p, tier) {
        (Pressure::Ok, _) => Action::Proceed,
        (Pressure::Low, Tier::Lite) => Action::UnloadAfterUse,
        (Pressure::Low, _) => Action::SkipCleanupModel,
        (Pressure::Critical, _) => Action::RefuseWithNotice("Low memory"),
    }
}

/// Make the kernel's OOM killer pick the model worker before the user's apps (Linux, no privilege
/// needed to raise). Call in the worker right after start; harmless no-op elsewhere.
pub fn prefer_oom_victim() {
    #[cfg(target_os = "linux")]
    let _ = std::fs::write("/proc/self/oom_score_adj", "500");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(avail: f32, psi: Option<f32>) -> Snapshot {
        Snapshot { available_mb: avail, total_mb: 16_000.0, psi_some_avg10: psi }
    }
    fn go(avail: f32, loaded: f32, psi: Option<f32>, prev: Pressure) -> Pressure {
        classify(&snap(avail, psi), loaded, &Config::default(), prev)
    }

    #[test]
    fn levels() {
        assert_eq!(go(6000.0, 0.0, Some(1.0), Pressure::Ok), Pressure::Ok);
        assert_eq!(go(1200.0, 0.0, None, Pressure::Ok), Pressure::Low);
        assert_eq!(go(500.0, 0.0, None, Pressure::Ok), Pressure::Critical);
        assert_eq!(go(6000.0, 0.0, Some(15.0), Pressure::Ok), Pressure::Low);
        assert_eq!(go(6000.0, 0.0, Some(40.0), Pressure::Ok), Pressure::Critical);
        // 2000 free is fine now, but not enough once a 750 MB load lands on it.
        assert_eq!(go(2000.0, 0.0, None, Pressure::Ok), Pressure::Ok);
        assert_eq!(go(2000.0, 750.0, None, Pressure::Ok), Pressure::Low);
    }

    #[test]
    fn fraction_wins_on_big_machines() {
        let s = Snapshot { available_mb: 5000.0, total_mb: 64_000.0, psi_some_avg10: None };
        assert_eq!(classify(&s, 0.0, &Config::default(), Pressure::Ok), Pressure::Low);
    }

    #[test]
    fn hysteresis_does_not_flap() {
        // Entered Low at 1400; 1600 is above the entry threshold but under 1500*1.25: stay Low.
        assert_eq!(go(1600.0, 0.0, None, Pressure::Low), Pressure::Low);
        assert_eq!(go(2100.0, 0.0, None, Pressure::Low), Pressure::Ok);
        // From Ok the same 1600 is fine.
        assert_eq!(go(1600.0, 0.0, None, Pressure::Ok), Pressure::Ok);
        // Critical eases to Low (not Ok) once above 700*1.25, and stays Critical just above 700.
        assert_eq!(go(800.0, 0.0, None, Pressure::Critical), Pressure::Critical);
        assert_eq!(go(900.0, 0.0, None, Pressure::Critical), Pressure::Low);
        // PSI must halve before Low clears.
        assert_eq!(go(6000.0, 0.0, Some(7.0), Pressure::Low), Pressure::Low);
        assert_eq!(go(6000.0, 0.0, Some(4.0), Pressure::Low), Pressure::Ok);
    }

    #[test]
    fn advice_table() {
        assert_eq!(advice(Pressure::Ok, Tier::Quality), Action::Proceed);
        assert_eq!(advice(Pressure::Low, Tier::Standard), Action::SkipCleanupModel);
        assert_eq!(advice(Pressure::Low, Tier::Lite), Action::UnloadAfterUse);
        assert_eq!(advice(Pressure::Critical, Tier::Lite), Action::RefuseWithNotice("Low memory"));
    }

    #[test]
    fn parsing() {
        let m = "MemTotal:       16000000 kB\nMemFree: 1 kB\nMemAvailable:    2048000 kB\n";
        let (a, t) = parse_meminfo(m);
        assert!((a - 2000.0).abs() < 1.0 && (t - 15625.0).abs() < 1.0);
        assert_eq!(parse_psi("some avg10=12.50 avg60=3.00 avg300=1.00 total=99\nfull avg10=1.00 avg60=0 avg300=0 total=1\n"), Some(12.5));
        assert_eq!(parse_psi(""), None);
        let s = sample();
        assert!(s.total_mb > 0.0);
    }
}
