//! When to unload or trim the model worker. Pure functions so every rule is unit-tested.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Memory {
    pub available_mb: f64,
    pub total_mb: f64,
    /// Linux PSI `some avg10`: % of the last 10 s some task stalled waiting for memory.
    pub psi_some_avg10: Option<f64>,
}

pub fn under_pressure(memory: &Memory, low_memory_mb: f64) -> bool {
    memory.available_mb < low_memory_mb
        || memory.available_mb < memory.total_mb * 0.10
        || memory.psi_some_avg10.is_some_and(|stall| stall > 10.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    None,
    /// End the worker process: frees every byte of model memory.
    Unload,
    /// Keep dictating, but drop the optional cleanup model once this dictation is done.
    TrimAfterDictation,
}

pub fn decide(worker_running: bool, dictating: bool, idle_for: Duration, idle_limit: Duration, pressure: bool) -> Action {
    match (worker_running, dictating) {
        (false, _) => Action::None,
        (true, false) if pressure || idle_for >= idle_limit => Action::Unload,
        (true, true) if pressure => Action::TrimAfterDictation,
        _ => Action::None,
    }
}

/// Crash budget: at most 3 worker crashes per minute before we stop respawning for a while.
pub fn may_start(crashes: &[Instant], now: Instant) -> bool {
    crashes.iter().filter(|&&t| now.duration_since(t) < Duration::from_secs(60)).count() < 3
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);

    fn memory(available_mb: f64, psi: Option<f64>) -> Memory {
        Memory { available_mb, total_mb: 16_000.0, psi_some_avg10: psi }
    }

    #[test]
    fn pressure_from_low_ram_or_stalls() {
        assert!(!under_pressure(&memory(3_000.0, Some(0.5)), 1024.0));
        assert!(under_pressure(&memory(800.0, None), 1024.0));
        assert!(under_pressure(&memory(3_000.0, Some(25.0)), 1024.0));
        // 10% of total wins over a small configured floor on big machines.
        assert!(under_pressure(&Memory { available_mb: 5_000.0, total_mb: 64_000.0, psi_some_avg10: None }, 1024.0));
    }

    #[test]
    fn idle_and_pressure_unload_but_never_mid_dictation() {
        assert_eq!(decide(true, false, 5 * MIN, 5 * MIN, false), Action::Unload);
        assert_eq!(decide(true, false, MIN, 5 * MIN, false), Action::None);
        assert_eq!(decide(true, false, Duration::ZERO, 5 * MIN, true), Action::Unload);
        assert_eq!(decide(true, true, 9 * MIN, 5 * MIN, false), Action::None);
        assert_eq!(decide(true, true, Duration::ZERO, 5 * MIN, true), Action::TrimAfterDictation);
        assert_eq!(decide(false, false, 9 * MIN, 5 * MIN, true), Action::None);
    }

    #[test]
    fn crash_budget_resets_after_a_minute() {
        let now = Instant::now();
        let recent = [now - Duration::from_secs(5), now - Duration::from_secs(10), now - Duration::from_secs(20)];
        assert!(!may_start(&recent, now));
        assert!(may_start(&recent[..2], now));
        assert!(may_start(&recent, now + Duration::from_secs(61)));
    }
}
