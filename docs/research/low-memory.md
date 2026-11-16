# Low-memory protection (memguard)

Goal: Voicio must never make the laptop thrash. Code: `engine/crates/app/src/memguard.rs` (sample, classify with hysteresis, advice). It complements `policy.rs` (binary pressure -> unload) and `memory.rs` (sampler); nothing is wired yet, apply the snippets below.

## Findings (sources: kernel docs, MS/Apple/Android docs; from knowledge, not re-fetched this session)

- **Signal on Linux: `MemAvailable`** in /proc/meminfo (kernel Documentation/filesystems/proc.rst: estimate of memory available to start apps without swapping; counts reclaimable page cache). `MemFree` is wrong (cache looks "used"). It is a level, so cheap and instant.
- **PSI** (Documentation/accounting/psi.rst): `/proc/pressure/memory` `some avg10` = % of the last 10 s that at least one task stalled on memory. This is the real "thrashing has started" signal; MemAvailable can look fine while the working set is already being re-read from disk. Needs kernel 4.20+ and PSI enabled (Manjaro: yes); absent -> use None. Polling is fine; kernel triggers (poll() on the file with `some 150000 1000000`) exist if event-driven is wanted, not needed at 1-2 s polling.
- **cgroup v2** (Documentation/admin-guide/cgroup-v2.rst): `memory.high` is a throttle limit: above it the cgroup's tasks are slowed with direct reclaim, no OOM kill. `memory.max` is the hard limit (OOM kill inside the cgroup only). `memory.pressure` is PSI scoped to the cgroup.
- **systemd-oomd** (man oomd.conf, systemd.resource-control): kills whole cgroups when swap use or per-cgroup PSI exceeds `ManagedOOMMemoryPressureLimit` (default 60 % for 30 s) for units with `ManagedOOMMemoryPressure=kill`. It is a last resort; our levels must trip well before it.
- **mmap'd weights (llama.cpp GGUF with mmap, ONNX Runtime mmap'd/large initializers)**: file-backed clean pages. Under pressure the kernel drops them for free (page cache eviction) and re-reads on next touch, so they cost only major faults, not swap. Anonymous memory (KV cache, ORT arenas/activations, sherpa buffers, the Rust heap) has no backing file; it needs swap or is OOM-killed. Consequences: RSS of the worker overstates the pressure it causes; but on a thrashing machine the re-faulting of weights is exactly the freeze the user sees (the model is touched every token). Ending the worker process is the only way to free both cleanly, which is what Voicio already does.
- **oom_score_adj**: write 500 to `/proc/self/oom_score_adj` in the worker. Raising it needs no privilege (lowering does). The kernel OOM killer then picks the worker before browsers/IDE; the agent sees the pipe close and shows a notice. Implemented as `memguard::prefer_oom_victim()`. Lowering CPU priority (`nice`) does not help memory thrash; skip it.
- **systemd unit**: `MemoryHigh=900M` is a soft ceiling: when the worker passes it the kernel throttles and reclaims inside that cgroup only, so the rest of the desktop keeps running (slow dictation instead of a frozen laptop). `MemoryMax=1200M` is the hard cap: worker/agent OOM-killed alone. Measured: loaded 546 MB, peak 748 MB, so 900M is ~20 % above peak and does not touch normal use. Caveat: the unit must contain the worker (the agent spawns it as a child, so it inherits the cgroup). Add also `ManagedOOMPreference=omit` NOT; leave default so oomd may kill us before the desktop.
- **Windows**: `GlobalMemoryStatusEx` -> `ullAvailPhys` is the cheap level (what sysinfo uses); `QueryMemoryResourceNotification(LowMemoryResourceNotification)` is the event form (signals when the OS considers physical memory low), good later. No PSI equivalent. A Job Object with `JOB_OBJECT_LIMIT_PROCESS_MEMORY` is the hard cap analogue (agent already uses job objects).
- **macOS**: `host_statistics64(HOST_VM_INFO64)` free+inactive+purgeable pages for a level; the proper signal is `dispatch_source_create(DISPATCH_SOURCE_TYPE_MEMORYPRESSURE, ..., WARN|CRITICAL)` (kernel memorystatus; `sysctl kern.memorystatus_vm_pressure_level` = 1/2/4 normal/warn/critical). macOS compresses memory, so free bytes alone mislead. memguard uses sysinfo's available bytes and carries a TODO for the dispatch source.
- **Android** (for the separate app, not Rust): `ComponentCallbacks2.onTrimMemory` (TRIM_MEMORY_RUNNING_LOW/CRITICAL, UI_HIDDEN, BACKGROUND) is the primary event: on RUNNING_LOW unload the cleanup LLM, on RUNNING_CRITICAL / BACKGROUND release the whole `ModelHolder`. `ActivityManager.getMemoryInfo().lowMemory` / `availMem` / `threshold` is the polled check before a load: refuse with a notice if `availMem < threshold + modelSize`.

## Thresholds chosen (Config::default)

| | Low | Critical |
|---|---|---|
| available MB (after subtracting the pending load) | < 1500 | < 700 |
| available fraction of total | < 10 % | < 4 % |
| PSI some avg10 | > 10 | > 30 |
| exit | > 1.25x the entry MB, PSI < half | same |

Why: a cold Standard start needs ~750 MB peak, so Low (1500 after the load is subtracted, i.e. 2250 free before a cold start) leaves a working margin for the desktop; Critical 700 is below one more model load and near where swap/page cache churn begins. PSI 10 matches the existing `policy.rs`; 30 is visible stalling. 25 % exit margin stops the notice flapping around the line. `loaded_mb` = 0 when the models are resident, ~750 (Lite ~350) before a cold start.

Actions: Ok -> Proceed. Low -> SkipCleanupModel (Standard/Quality: rule-based cleanup, LLM not loaded) or UnloadAfterUse (Lite). Critical -> RefuseWithNotice("Low memory").

## Integration points (apply by hand)

1. `agent.rs`, where the agent samples memory each tick (next to the `policy::under_pressure` call): keep `guard: memguard::Pressure` in the agent state and
```rust
let snap = memguard::sample();
let loaded = if worker_running { 0.0 } else { cold_start_mb(tier) }; // 750 / 350
self.guard = memguard::classify(&snap, loaded, &memguard::Config::default(), self.guard);
```
2. `agent.rs`, dictation-start handler (before spawning/using the worker):
```rust
match memguard::advice(self.guard, tier) {
    memguard::Action::Proceed => {}
    memguard::Action::SkipCleanupModel => { /* send cleanup=false / rules-only in the start request */ self.show_notice("Low memory: quick cleanup"); }
    memguard::Action::UnloadAfterUse => self.trim_after_dictation = true, // reuse policy::Action::TrimAfterDictation path
    memguard::Action::RefuseWithNotice(text) => { self.show_notice(text); return; }
}
```
3. `ui.rs` / pill: reuse the amber `Notice` look (adjust-something) with text "Low memory"; clear it when `guard == Ok`.
4. `worker.rs` (or `engine/src/` worker main), first line of `worker` startup: `voicio::memguard::prefer_oom_victim()` equivalent; the engine crate cannot import the app module, so inline: `let _ = std::fs::write("/proc/self/oom_score_adj", "500");` under `#[cfg(target_os="linux")]`.
5. Unit file (`install.sh` / voicio.service `[Service]`): `MemoryHigh=900M` and `MemoryMax=1200M`.
6. Keep the existing `VOICIO_FAKE_AVAILABLE_MB` hook: build the Snapshot from `memory::Monitor::sample()` in tests instead of `memguard::sample()` if the hook is needed.
