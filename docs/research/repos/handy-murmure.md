# Study: Handy and Murmure (Tauri dictation apps)

Read-only study of local clones in `/home/manas/Documents/whispr-refs/{Handy,murmure}` (Handy HEAD 73c0958, Murmure HEAD 2218c1b).
No network used. No code is reproduced; this is ideas and designs only.

Licences (from `LICENSE`): **Handy = MIT**, **Murmure = GNU AGPL v3**. Treat Murmure as design reference only (do not copy code or distinctive text). Handy's MIT code could in principle be reused with attribution, but Whispr's licence is undecided, so this report still describes designs only.

Legend: V = verified in code/docs. UNVERIFIED = inferred or not confirmed from code.

---------------------------------------------------------------------------------------------------

# Part A. HANDY (MIT) - Tauri 2, Rust + React/TS, transcribe.cpp (GGUF)

## A1. Architecture

### Processes and threads
- Two process roles in ONE executable. The same binary is re-launched with a hidden first arg `--transcribe-worker` (`src-tauri/src/engine_supervisor/mod.rs`, checked in `main.rs` before CLI/Tauri/single-instance). All native ML code (transcribe.cpp / ggml, GPU drivers) runs only in the worker. A GPU driver fault, `GGML_ASSERT` abort or hang kills only the worker.
- `EngineSupervisor` (`engine_supervisor/supervisor.rs`, ~2000 lines) is the single owner. One owner thread works a command queue and holds the only worker slot, so "at most one model worker alive" holds by construction. Only `cancel` and device probes bypass the queue (a probe spawns its own short-lived model-less worker so a hung GPU driver cannot block dictation).
- Worker lifetime == model lifetime, so unload = process exit = all CPU/GPU memory returned to the OS (same philosophy as Whispr's two-process design).
- Worker is also spawnable CPU-only (`--cpu-only`) so no GPU driver code runs at all; used as automatic fallback after a GPU crash/hang.
- Protocol (`engine_supervisor/protocol.rs`): strict request/response framed on stdin/stdout, frame = `[u32 json_len][json][u32 pcm_len][pcm f32 LE]`. JSON chosen on purpose because NaN confidences round-trip as null.
- Hang detection: per-call deadlines scaled by audio length and by device (GPU: 30s call floor / 60s run floor / 10x audio; CPU: 120s / 300s / 20x). HELLO timeout 60s (Metal shader compile after install), load timeout 180s. Shutdown grace 2s then kill. Last 64 stderr lines kept for crash reports (catches GGML_ASSERT text). Worker exits the moment parent's stdin closes, even if hung (dedicated stdin reader thread).
- Tauri main thread: webview + tray + (macOS) NSPanel overlay. Many things deliberately hop to the main thread (`run_on_main_thread`): Enigo (macOS TIS APIs), overlay geometry (GDK/Xlib corrupt if touched off-thread, issue #227).
- Dedicated threads: shortcut coordinator (single thread, mpsc `Command` queue, `recv_timeout` driven by release-grace deadline), audio recorder, streaming worker (feeds + finalize share one FIFO channel so every fed frame is processed before finalize), history cleanup, signal handler, model idle watcher, tray apply loop.

### Module map (real paths, `src-tauri/src/`)
- `lib.rs` (setup, managers, specta command registry, ~59 commands), `main.rs`, `cli.rs` (clap), `signal_handle.rs` (SIGUSR2 toggle, shared with CLI), `portable.rs` ("portable" marker file next to exe puts all data in `Data/`), `autostart.rs` (SMAppService on macOS 13+ so Login Items shows the app name), `memory.rs` (glibc malloc tuning), `secure_input.rs` (macOS Secure Event Input), `tray.rs`, `tray_i18n.rs`, `overlay.rs`, `input.rs` (Enigo wrapper, layout-independent paste chords), `clipboard.rs` (paste + Linux tools), `paste_tx/{mod,macos,windows}.rs` (receipt-sequenced paste), `actions.rs` (the dictation pipeline), `transcription_coordinator.rs` (lifecycle state machine), `settings.rs` (2000 lines, schema version + migrations), `llm_client.rs` (post-processing HTTP), `apple_intelligence.rs`, `chinese_script.rs`, `system_languages.rs`, `audio_feedback.rs`.
- `managers/`: `audio.rs` (mic lifecycle, device resolve, mute-while-recording), `transcription.rs` + `transcription/{policy,live,chunked,job}.rs` (decode strategy), `transcription_jobs.rs` (background file/history jobs that yield to dictation), `model.rs` (3100 lines) + `model/download.rs` (+tests), `model_capabilities.rs`, `gguf_meta.rs` (probes GGUF headers), `history.rs` (SQLite + WAV files).
- `catalog/mod.rs`: bundled offline model catalog (pinned revisions, per-quant hashes, benchmark results) compiled into the binary; "catalog is just another descriptor producer" next to HF discovery and on-disk scan.
- `audio_toolkit/`: `audio/{device,sink,visualizer,decode,utils}.rs`, `vad/{silero,earshot,smoothed}.rs`, `chunking.rs` (cut long audio at pauses), `text.rs` (custom words, fillers), `lang_id.rs`.
- `shortcut/{mod,handler,handy_keys,tauri_impl}.rs`: two selectable keyboard backends (Tauri global-shortcut plugin vs. their own handy-keys lib); default = Tauri on Linux, handy-keys elsewhere.
- `commands/{audio,history,models,transcription,mod}.rs`; `helpers/clamshell.rs` (macOS lid-closed detection).

### Data flow: hotkey -> inserted text
1. Shortcut backend emits key edge (or SIGUSR2 / CLI flag / `--toggle-transcription` via single-instance plugin) -> `TranscriptionCoordinator` command queue.
2. `CoordinatorState` (pure, unit-tested; produces `Effect::Start/Stop`) decides. Stage = Idle | Recording(binding) | Processing.
3. Start (`actions.rs` TranscribeAction.start): bump session counter, begin loading ASR model + VAD in background (parallel with mic open), pick overlay form from model capability (streaming vs. compact), open mic, wait for FIRST REAL SAMPLE before showing "ready" + start chime + muting audio (a slow device must not produce a late "ready" for a session already ended; uses session generation tokens), dynamically register the cancel shortcut (in a separate task to avoid deadlock).
4. During recording: VAD (Silero, smoothed with pre-roll/onset/hangover) passes frames; if model streams, frames go via a lock-free `StreamRouter` (single relaxed atomic load when idle) to a streaming worker; overlay gets throttled mic levels + `StreamTextEvent{committed, tentative}`.
5. Stop: immediate visual change (compact "transcribing" pill, or Live card switches to spinner), unmute, stop chime, then concurrently: save WAV for history and transcribe (live result if any, else batch; long audio chunked at pauses ~30s chunks). Cancel is polled every 25ms; a cancel never starts a batch run.
6. Post: custom-word fuzzy correction -> filler-word removal (language-gated) -> Chinese script conversion -> optional LLM post-process (skipped when text is empty; strips `<think>` blocks and invisible Unicode; structured-output JSON schema when provider supports) -> history row -> paste.
7. Paste (`clipboard.rs`, `paste_tx/`): PASTE_LOCK serialises pastes; save clipboard (text, or image only if no text), write transcript, sleep `paste_delay_ms` (default 60), send chord (layout-independent keycodes), sleep `paste_delay_after_ms` (60), restore clipboard; optional auto-submit key (Enter / Ctrl+Enter / Cmd+Enter); optional append trailing space; or "Direct" typing; or external script; or None (history only).
8. `FinishGuard` (Drop): immediate-unload policy, tell coordinator processing is done (which may start a REMEMBERED press), call `malloc_trim`.

### State machine (the best idea in the repo)
`transcription_coordinator.rs`. All three activation modes run through ONE machine:
- Push-to-talk: every release stops (hold threshold 0).
- Toggle: releases ignored, next press stops (locked from the start).
- Hold-or-toggle ("Auto", the default): hold >= `hold_threshold_ms` (300) = PTT; shorter = tap, which LOCKS the session; next press stops.
- 30ms press debounce (not applied to external triggers, which are real edges; dropping one would desync toggle parity).
- 50ms RELEASE_GRACE: a key-up is deferred so a synthesized X11 auto-repeat key-down can cancel it (issue #1539).
- Busy pipeline (Stage::Processing): presses are REMEMBERED/FORGOTTEN to preserve parity instead of dropped; a remembered press starts as soon as the pipeline drains, and hold duration is measured from the real key-down time. A different binding cannot steal the pending slot.
- Mode changed mid-recording still stops correctly (toggle press always ends it).
- Emits `RecordingPhase{started, locked, stopped, cancelled}` for onboarding UI.

### IPC
Tauri commands + events via `tauri-specta` (generates `src/bindings.ts`, typed). Zustand stores mirror backend state. Overlay is a SECOND webview window (`src/overlay/index.html`) fed by `emit_to` (single target to avoid double eval_script).

### Model management
- Bundled catalog JSON (offline) + HF mirror fallbacks + on-disk rescan + custom GGUF models (`gguf_meta.rs` reads headers for capabilities). Download transport in `managers/model/download.rs` (direct HTTP, with tests), verifying hashes, resume UI (`DownloadProgressDisplay`). "Retired models" migration screen forces re-download of old ONNX models (`ModelMigration.tsx`, `lib/models/retired.ts`).
- Model table shows per-language accuracy grade (word errors on their test set), speed bars (30s of speech on test laptop, CPU or accelerated), tags, "fits your languages" sections, untested section. Files: `components/shared/modelTable/*`, `lib/models/grading.ts`, `GradeChip.tsx`, `SpeedBars.tsx`.
- Preferred languages (chosen in onboarding) drive model recommendations.

### Memory / unload policy
- `ModelUnloadTimeout` enum: Never, Immediately, 2/5/10/15 min, 1 h, 15 s (debug only). Default 5 min. Idle watcher resets timer on use. "Immediately" is implemented in the pipeline's drop guard.
- Unload never blocks the caller (event loop safe): state flips to "unloaded" immediately, the worker's exit is awaited elsewhere; a dictation started right after queues its load behind the unload.
- glibc tuning (`memory.rs`): pins `M_MMAP_THRESHOLD` at 128 KiB so multi-MB transient buffers (PCM, WAV copy, mel scratch) are mmapped and returned on free (glibc's dynamic threshold otherwise made RSS creep ~15 MB per 2-min dictation, issue #1792), plus `malloc_trim(0)` after each pipeline. Highly relevant to Whispr's RAM budget.
- "Keep mic open between transcriptions" (lazy stream close, 30 s) and "Always-on microphone" trade latency for privacy indicator.

### Error handling
- Typed outcome `TranscriptionOutcome{Typed, Empty, Silent, Failed}` (Silent = mic delivered only digital zeros, i.e. muted); drives onboarding messages and toasts.
- `recording-error` events with `error_type` -> platform-specific toast text (mic permission denied per OS, no input device).
- Mic fallback persists the successful fallback to settings, but re-reads first so it cannot clobber a concurrent user choice. Named-device resolution cached, cleared on open failure.
- Panics wrapped (`catch_unwind`), RAII guards for loading flags, token-guarded worker flags.
- Settings: container-level `serde(default)` so a partial/old store never fails the whole load; schema version + migrations; unknown enum variants fall back instead of failing; API keys held in `SecretMap` that redacts in Debug.

## A2. COMPLETE feature list (settings UI, README, i18n, commands)

Sidebar pages (`components/Sidebar.tsx`): General, History, Models, Advanced, Post Process (only visible when enabled), Debug (only when debug mode on, toggled Ctrl/Cmd+Shift+D), About. Footer: model selector/status + update checker.

**General** (`settings/general/GeneralSettings.tsx`, `ModelSettingsCard.tsx`)
- Shortcuts: Transcribe, Cancel, optional dedicated "Transcribe with post-processing" hotkey; shortcut capture widget (`ShortcutInput`, `useShortcutCapture`, `Keycaps`).
- Shortcut behaviour: Auto (hold-or-toggle) / Hold / Toggle; hold threshold (debug).
- Language (searchable, Auto), Preferred languages (used to recommend models), Translate to English toggle.
- Model settings card (language, translate, etc. per model capability).
- Sound: microphone select, input channel (average or specific channel for multi-input interfaces), audio feedback toggle + sound theme (Marimba, Pop, Custom files) + volume, output device select, Clamshell microphone (macOS lid closed), mute-while-recording, always-on mic.

**Models**: sections "On this computer", "For your languages", "Untested"; show more/fewer; download-and-use (auto switches when done); delete with space-freed prompt; cancel download; legend explaining grades; rescan folder / HF cache; retired-models notice; search.

**History** (`settings/history/*`): list with audio player (`ui/AudioPlayer.tsx`), copy, save/star, delete, re-transcribe (per entry, with progress %, cancel, queued state), show more/less, "Transcribe File" (audio and video decode via symphonia-style `audio/decode.rs`) and drag-and-drop file overlay (`FileDropOverlay.tsx`), open recordings folder, history limit, recording retention (never / preserve limit / 3 days / 2 weeks / 3 months), entries keep both raw and post-processed text and the prompt used.

**Advanced** (grouped App / Output / Transcription / History / Experimental)
- App: launch on startup, start hidden, show tray icon, app language (26 UI locales incl. RTL: ar, he), theme (system/light/dark), update checks toggle, show What's New, autostart.
- Output: paste method (Ctrl+V, Ctrl+Shift+V, Shift+Insert, Direct, None, External script with path), typing tool on Linux (Auto, wtype, kwtype, dotool, ydotool, xdotool), clipboard handling (don't modify / copy to clipboard), auto-submit (Off, Enter, Ctrl+Enter, Cmd/Super+Enter), append trailing space.
- Transcription: custom words (fuzzy correction threshold in debug), filler-word removal (+ custom filler list), Chinese script (as transcribed / Simplified / Traditional TW / Traditional HK), long-audio chunking, VAD on/off, VAD backend (Silero default / Earshot experimental), hardware acceleration (Auto/CPU/GPU + specific GPU device), model unload timeout, lazy stream close, overlay style (None / Minimal / Live), overlay position (top / bottom).
- Experimental toggle gates unfinished features.

**Post Process**: provider (OpenAI, Z.AI, plus others, custom, Apple Intelligence on Apple Silicon macOS 26+), API format (OpenAI Chat vs Anthropic Messages for custom), base URL, API key (secret), model combobox with fetch/refresh, prompt library (create/update/delete; `${output}` placeholder), selected prompt, dedicated hotkey.

**Debug**: log directory, log level, live log viewer (pause/resume/copy), keyboard implementation selector, keyboard diagnostic (event-kind counts only, no key identity), paste delay before/after, "reliable paste" beta, recording buffer (extra ms after stop), hold threshold, word-correction threshold, preview onboarding and What's New.

**About**: version, What's New toggle, app-data directory, source link, donate, acknowledgments.

**Tray** (`tray.rs`, `tray_i18n.rs`): version line (disabled), Settings, Check for updates, Copy last transcript (prefers post-processed text), Cancel (only while busy), Model submenu (check items), Unload model, Quit, and a "Shortcuts blocked by Secure Input" warning entry. Icon states idle/recording/transcribing x light/dark x warning badge. Tray is driven by one "desired state" snapshot applied on main thread (coalesces calls). Windows reads taskbar theme for icon colour.

**CLI** (`cli.rs`): `--toggle-transcription`, `--toggle-post-process`, `--cancel`, `--start-hidden`, `--no-tray`, `--debug`, `--help`. Runtime-only overrides (do not persist). Unix signal SIGUSR2 toggles (SIGUSR1 only on macOS because WebKitGTK uses it internally for GC and caused phantom recordings, #1660). Env: `HANDY_NO_GTK_LAYER_SHELL`, `HANDY_DISABLE_UPDATER`, `HANDY_KEEP_VULKAN_IMPLICIT_LAYERS`, `HANDY_TRANSCRIBE_WORKER_LOG`. Third-party Raycast extension exists.

**Onboarding** (`components/onboarding/*`): Welcome (app language + spoken languages incl. Chinese script choice) -> Permissions (mic + accessibility on macOS, relaunch step, "welcome back" variant, Windows variant) -> Model (recommended picks for chosen languages, download progress that survives quit/resume) -> Demo (animated mock desktop with "hold / tap" explanation, change-your-shortcut) -> Try It (live practice with recording-phase events; tells you hold vs tap done, nothing typed, silent mic, too slow, "high five" finish; measures speed via `TranscriptionCompleteEvent`). Linux guide screen (`LinuxGuide.tsx`). Screen transitions via View Transitions API (`viewTransitions.ts`). Separate standalone screens for returning users: ReturningPermissions, ModelMigration, ReplaceModel. Progress persisted in `onboardingStore.ts`.

**Packaging**: Windows NSIS (custom template) + winget (community), Azure Trusted Signing; macOS dmg/app, hardened runtime, entitlements, Homebrew cask (community); Linux deb (depends libgtk-layer-shell0, libopenblas0), rpm, AppImage (bundles media framework); Nix flake; portable mode; Tauri updater with minisign-style signatures; per-release What's New markdown (`content/release-notes/*.md`).

**Other**: Apple Intelligence local post-processing (Swift bridge in `src-tauri/swift`), clamshell mic, secure-input fallback, update checker with portable-install flow, error boundary, i18n lint rule (no hardcoded strings in JSX), RTL support, CJK font subset lazy-loaded.

## A3. UI structure

- Single main window, left `Sidebar` (icon + label, items conditionally enabled), scrollable content that resets scroll on section change, `Footer` (model status + update). Toasts via `sonner`. Settings composed of `SettingsGroup` + `SettingContainer` rows (title, description, tooltip, control) using primitives in `components/ui/` (Switch, ToggleSwitch, Select, Dropdown, Slider, Radio, Input, Textarea, Dialog, Alert, Badge, Tooltip, ResetButton, PathDisplay, AudioPlayer, Spinner).
- Overlay (`src/overlay/RecordingOverlay.tsx` + `.css`): states `recording | streaming | transcribing | processing`.
  - Compact pill, 3-zone grid: [dot | 9-bar smoothed waveform | cancel (x)].
    - Dot is in "arming" state until first real mic sample arrives, then "ready".
    - Transcribing/processing: [spinner | label ("Transcribing...", "Processing...", "Loading model...") | cancel].
  - Live card (streaming models): grows from the pill into a text card; committed text (stable) + tentative text (volatile) + caret; timer + cancel; top fade when overflowing; sticks to bottom unless user scrolls back; direction flips with overlay position (grows down from top overlay, up from bottom); on stop shows working spinner inside the card. Model-load notice only appears if the load takes >2s (avoid flashing).
  - Smoothing of FFT levels (0.7/0.3 EMA), RTL aware, language synced from settings on each show.
- Tray panel mock and desktop bar mock inside onboarding for explaining the UI (`TrayPanel.tsx`, `DesktopBar.tsx`).

## A4. Notable techniques and bug fixes (visible in code/comments)

- Auto-repeat vs release: 50 ms release grace so X11 synthesized key-repeat cancels a pending key-up (#1539).
- Busy-press parity: remember/forget presses instead of dropping when the pipeline is processing (toggle parity must hold).
- Hotkey self-trigger/modifier hold: paste chord keeps modifier down for a configurable hold (legacy 100 ms, added because real systems dropped quickly-released chords, #165); some apps poll global key state.
- Clipboard restore race (#502): fixed delay can restore the OLD clipboard before the target reads the new one. "Reliable paste" (`paste_tx/`): publish transcript as a lazy clipboard promise (Windows delayed rendering `WM_RENDERFORMAT`; macOS pasteboard owner `provideData`) and restore only after a read receipt arrives AFTER the chord was injected, only while still the clipboard owner (sequence number unchanged), with a quiet period after the last read (Chromium reads twice) and a hard timeout. Failure mode is "transcript lingers on clipboard", never "stale content pasted". Measured chord hold of 10 ms possible when receipts can detect failure.
- Clipboard fidelity: restores image if the original clipboard held an image; PASTE_LOCK prevents overlapping pastes restoring each other's transcript.
- Wayland: prefers wl-copy for clipboard; typing tool ladder (kwtype on KDE, wtype except on GNOME/KDE, dotool, ydotool, xdotool); GNOME Wayland cannot use wtype (no virtual-keyboard protocol).
- Wayland/layer-shell overlay: gtk-layer-shell loaded; overlay disabled by default on Linux because compositors treat it as active window, which steals focus and breaks paste-back. `HANDY_NO_GTK_LAYER_SHELL=1` escape hatch. Layer-surface sizing needs `set_size_request` + `resize(1,1)` trick.
- WebKit memory growth (#1279, wry#1489): do not emit overlay events when overlay disabled; throttle level events to ~30 fps; use `emit_to` (one eval_script instead of two).
- Delayed overlay hide uses a generation counter so a stale hide cannot hide a newer session's overlay (esp. remembered-press instant restart).
- Windows overlay: Win32 topmost forced (Tauri always-on-top can be overridden); per-monitor DPI placement with a single `SetWindowPos` in physical px; scales with Windows accessibility text-size; Vulkan implicit layers (overlays, capture hooks) disabled for the process via `VK_LOADER_LAYERS_DISABLE`.
- macOS: overlay is an NSPanel; bottom anchor uses visibleFrame to track Dock; Secure Event Input detection and Carbon-shortcut shadow-registration fallback so keyed shortcuts keep working (modifier-only keep working natively), dynamic Cancel shadowing; fn/Globe key limited to Apple keyboards; login item via SMAppService; Bluetooth headset mic quality caveat documented.
- Live stream "committed vs tentative" split for flicker-free preview.
- First-sample-driven readiness: UI/chime wait for first real audio callback; late events for ended sessions are invalidated.
- Mute-while-recording restores exact prior state (stays muted if it was muted; if unknown, unmute so audio never stranded).
- Digital-silence detection (all-zero samples) distinguished from "no speech".
- Language-gated filler removal: universal fillers always, risky ones ("um" is a real word in pt/de) only when output language is known, via confidence-gated text language ID (whatlang, >=0.9 and reliable, calibrated on Tatoeba); capital-letter debt handed to next word; repeated-word collapse (3+).
- Custom words: Levenshtein + Soundex + n-gram joining ("Charge B" -> "ChargeBee"), case-pattern preservation.
- Long-form chunking at pauses ported from another project; chunks are the unit of progress, of yielding to dictation, and of retry.
- Process-isolation fallbacks: GPU failure -> retry CPU-only worker; "forget GPU failures" when user changes accelerator.
- `<think>` stripping and zero-width-char stripping on LLM output; skip LLM on empty transcript.
- Window show on Wayland needs unminimize + show + focus (hidden-to-tray windows flagged minimised).
- WebKit DMA-BUF renderer disabled by default on Linux for startup crashes.
- Settings robustness: serde defaults at container level (#1619), enum alias for retired values.

## A5. What Handy lacks (relative to Whispr's goals)
No dictionary with ASR-level boosting (post-hoc fuzzy only), no regex/replace rules, no select-text voice commands, no local API, no wake word, no phone mic, no dictionary import/export, no stats. Feature freeze announced in AGENTS.md. UI is polished but each setting is a long list of toggles.

---------------------------------------------------------------------------------------------------

# Part B. MURMURE (AGPL v3) - Tauri 2, Rust + React/TS/Tailwind/shadcn, Parakeet TDT 0.6B v3 (ONNX int8, ort)

## B1. Architecture

### Processes and threads
- Single process (no worker split). `Model` (download/path) and `AudioState` (recorder, engine, chunk pipeline, session flags) are Tauri-managed state. The ASR engine (`ParakeetEngine`) is loaded lazily once (`preload_engine` in `audio/audio.rs`) and then CACHED IN MEMORY FOR THE APP LIFETIME. No idle unload or memory policy found (grep for unload/idle found only a 60 s idle timeout on the sound output stream). So the ~hundreds of MB model stays resident. UNVERIFIED that no other code path frees it, but searched `src-tauri/src` for unload/drop patterns.
- Threads: shortcut processor (platform-specific polling on Windows/macOS, X11 listener on Linux), start/stop each spawned off-thread so the processor stays reactive (~100 ms CPAL init; stop blocks on LLM and paste ~1 s), recorder writer thread, streaming-preview thread, wake-word listener loop with heartbeat watchdog, HTTP API server (axum), SmartMic HTTPS/WSS server, GTK main thread for overlay.
- Everything funnels through `AudioState.engine: Mutex<Option<ParakeetEngine>>`; the lock is held across inference (serialises dictation, chunk preview, wake-word transcription, API calls, smartmic).

### Module map (`src-tauri/src/`)
- `lib.rs` (setup, CLI early detection, plugin wiring), `main.rs`.
- `audio/`: `audio.rs` (start/stop/cancel orchestration, paste, enter key), `recorder.rs` (cpal capture, writes WAV), `microphone.rs` (device list; on Linux uses `pactl` names and temporarily routes default source), `pipeline.rs` (per-chunk transcribe, dictionary correction, cleanup, LLM, formatting rules, stats+history), `chunking.rs` (cut at silence: arm length, silence cut, forced cut with ~1 s overlap dedup), `streaming.rs` (overlay preview), `vad.rs`, `clean_recording.rs` (dedup repeats/hesitations), `sound.rs`, `output_volume/{linux,macos,windows}` (duck other audio), `helpers.rs`, `types.rs` (RecordingMode Standard/Llm/Command; RecordingTrigger Keyboard/WakeWord).
- `engine/`: `engine.rs` (decoder), `boost_tree.rs` (weighted Aho-Corasick phrase boosting fused into the greedy decoder, modelled on NeMo GPU-PB), `helpers.rs` (accent folding, case variants, BPE tokenise dictionary words, per-word confidence = weakest token), `transcription_engine.rs` (trait).
- `dictionary/` (store, helpers, correction), `formatting_rules/` (formatter, highlighter for regex UI, store, types), `history/` (last 5), `stats/` (WPM, words/month, audio MB, time saved), `llm/` (client, modes, command, transform-selection), `http_api/` (server.rs, prompt_mode.rs), `smartmic/` (server, websocket, pairing, cert, qr, audio/input bridges), `wake_word/`, `shortcuts/` (registry, types, capture, cli_dispatch, platform_{windows,macos,linux/x11}.rs, accessibility_macos), `overlay/` (overlay.rs, tray.rs, layer_shell.rs, input_region.rs), `clipboard/` (paste, snapshot), `settings/`, `onboarding/`, `cli/` (cli, import, helpers, types), `utils/` (platform.rs DE detection, wayland_inject.rs, wayland_xkb/*, enigo_session.rs, log_watchdog.rs).

### Data flow
Hotkey (or CLI/wake word) -> `shortcuts/shortcuts.rs` handle_recording_event: PushToTalk (press start, release stop) or ToggleToTalk (acts on release, with start/stop cooldowns) -> capture focus for onboarding -> spawn start (cpal init, lower other audio, start sound) -> recorder writes chunked audio; `ChunkPipeline` cuts at silences and transcribes each chunk in a fresh decoder state (padded with 0.25 s silence each side) while you speak (this also powers the streaming preview and bounds long recordings; hard cap 5 minutes) -> stop: flush last chunk -> merge chunks -> clean (dedup, optional hesitation removal) -> LLM post-process if mode is Llm/Command -> formatting rules -> stats + history -> paste -> cleanup temp WAV. Session ownership token: a newer session that starts while an older one finalizes takes over the flag.

### State machine
Much simpler than Handy: `RecordingSource` (None / Keyboard / WakeWord / ...) behind a mutex plus `ShortcutState` (toggled, suspended). Cooldowns between toggle start/stop and a 50 ms guard on voice-mode toggle. No concept of hold-vs-tap lock, no remembered presses. (See `shortcuts/shortcuts.rs`.) Shortcuts are suspended during synthetic paste so Murmure's own injected keys do not retrigger bindings.

### IPC
Tauri commands/events (stringly typed, no specta). Overlay is a second webview. Settings in `tauri-plugin-store`. Local HTTP API (axum) on 127.0.0.1:4800 with Origin allow-list middleware (rejects foreign origins), body size limit, disabled by default. SmartMic: self-signed TLS cert generated locally (10 yr, key file 0600), pairing tokens in system keyring/store with TTL, one connected device at a time, 100 msgs/s rate limit, optional relay URL (e.g., Cloudflare tunnel), bind-address selector.

### Model management
Single bundled model (Parakeet ultra int8 ONNX + tokenizer.json) located via resources (`model/model.rs`, `utils/resources.rs`). No download manager, no model choice. 25 European languages, auto language detect only (no forced language).

### Error handling
Fallible fns return `Result` (project rule: no `panic!`); `SecretString` redacts; Wayland injection failure emits `wayland-inject-unavailable` which drives an onboarding card; frontend listeners per error kind (`features/layout/listeners/*`: recording-error, llm-error, llm-mode-not-configured, accessibility, wayland, config-imported, tray-menu-i18n, toast); overlay shows short error text for a fixed time (mirrored constant between Rust and TS).

## B2. COMPLETE feature list

Sidebar (router.tsx, `features/layout/app-sidebar`): Home, Personalize (Dictionary, Formatting Rules), Extensions (Command Mode, Prompt Mode [LLM Connect], Voice Mode, Smart Speech Mic, Local API), Settings (Shortcuts, System, Import/Export), About. Update checker in layout.

**Home**: usage statistics cards (writing speed WPM, words this month, local audio MB, time saved), live audio visualiser (pixel-grid style), mic label + disconnected-mic banner (persisted friendly mic name), record label showing the shortcut, last-5 history list (click to copy; empty state), onboarding checklist card (3 tasks: try dictation, dictate in another app, add dictionary word; progress bar; congrats; dismiss), Wayland CLI setup card and Wayland clipboard-fallback card.

**Shortcuts**: Record (default Ctrl+Space), Paste last transcript (Ctrl+Shift+Space), Cancel (Ctrl+Backspace), Command/free prompt (Ctrl+Shift+X), Toggle Voice Mode (Ctrl+Shift+0), 4x "Dictate with Prompt N" (Ctrl+Shift+1..4), 4x "Transform with Prompt N" on selected text (Ctrl+Alt+Shift+1..4). Mouse side buttons usable. Linux Wayland: shortcuts replaced by a CLI commands panel (`shortcuts-cli/`) listing every command with copy rows.

**System** (sections General / Overlay / Advanced): UI language; microphone select + refresh; lower other audio while recording (+ volume %); record mode (push-to-talk / toggle-to-talk; toggle forced on Wayland CLI mode); text insertion mode (Standard Ctrl+V / Terminal Ctrl+Shift+V / Direct typing, with layout-fallback hook); automatic insert toggle (off = text stays in history); launch on startup; macOS dock visibility; history persistence on/off (default off = RAM only); sound on/off + volume; overlay visibility (hidden / while recording / always), position (top/bottom), size (small/medium/large), real-time preview toggle with text width, font size, max lines; "show result after dictation" (off / command / command+LLM / all) with duration; log level + open log folder; keep recordings (debug); copy-to-clipboard-after-dictation.

**Dictionary** (`personalize/custom-dictionary`): word list with toolbar (add, search?, import/export `.txt`, clear all, counter showing capacity health, empty state, removal toast with undo, "revamp seen" notice, medical presets). Rules: no digits, max one space, letters/accents/hyphens OK. Mechanism: words boosted INSIDE the decoder (phrase boosting automaton with spelling variants: as-is, accent-folded, lower, Title) and a post-hoc fuzzy correction that only fires when the model's per-word confidence is low (never overrides words the model was sure about); boost weakens as dictionary grows; fuzzy correction disabled above 100 entries; short words never fuzzy-corrected; 1 edit for short, 2 edits only for longer words; exact normalised matches restore canonical casing.

**Formatting Rules** (`personalize/formatting-rules`): built-ins: trailing space, space before ? and ! (French typography), text-to-number (multilingual), remove hesitations, short-text correction (strip final period and lowercase first letter for N words or fewer, threshold configurable, 0 = off; preserves ALL CAPS and mixedCase); custom find/replace with matching modes Contains(smart punctuation/spacing), Exact, Regex (capture groups $1..); drag-and-drop ordering; rule cards with rename, summary, regex validation hook, regex highlighter; used for voice commands ("new line", "open quote").

**Prompt Mode (LLM Connect)**: up to 4 saved prompts, each with name, system+user prompt (`<role>` block extracted as system prompt; `{{text}}` placeholder), provider (Ollama local or OpenAI-compatible remote), model, wake word, own shortcut; presets (translation, medical, development); drag-sortable tabs; highlighted prompt editor; model combobox with provider picker; 3-step onboarding (intro -> install/connect -> choose model, with hardware-based recommendations and in-app Ollama model pull with progress); advanced settings (local server URL, remote server URL + API key, connection test button, privacy acknowledgement shown only for non-private remote hosts); rewrite preview animation.

**Command Mode**: speak an instruction; with selected text it rewrites the selection in place, without selection it answers a question (short answers by default) either at the cursor or in the overlay result panel; own model setting; trigger rules; preview components (command preview, overlay preview, result-card preview); refuses to start recording with a clear overlay message if off or no model.

**Transform**: select text and press prompt N's Transform shortcut to run a saved prompt without speaking (copies selection via synthetic Ctrl+C after clearing the clipboard first so "unchanged" is distinguishable from "nothing selected").

**Voice Mode (beta, wake word)**: always-listening loop (VAD gate -> short transcription -> fuzzy match against wake phrases). Actions: record, record-with-prompt 1-4, command, cancel, validate (finish and submit), submit ("thank you X" strips and presses Enter); auto-Enter after wake word; silence timeout (default 1.5 s); longer phrases matched first; wake word is stripped from the output; heartbeat watchdog and restart backoff on mic death; toggle shortcut to mute listening; CPU warning.

**Smart Speech Mic**: phone as wireless mic via QR code + local HTTPS/WSS; phone PWA (service worker, manifest) with 3 views: Remote (REC button, trackpad with tap/long-press, Enter, Backspace), Transcription log (timestamps, copy, copy all), Translation (language pair, chat bubbles, LLM-detected direction); paired devices list with revoke; bind address, token TTL, relay (Cloudflare tunnel) config; Wayland notice; multilingual phone UI (`?lang=`).

**Local API**: opt-in HTTP on 127.0.0.1 (default 4800): `POST /api/transcribe` (WAV multipart), `POST /api/prompt-mode` (saved prompt by name), `POST /api/prompt-mode/custom` (inline instruction, provider, model; 4000-char cap), `GET /api/prompt-mode/prompts`. Calls have no side effects (no history, paste, overlay). In-app docs page with curl examples and a "Try it" button + WAV file picker.

**Import/Export** (`.murmure` JSON, versioned): categories settings, shortcuts, voice mode, smartmic, formatting rules, LLM connect, dictionary; category tree selection UI with sub-items, drop zone, preview, merge vs replace toggle, progress modal; CLI `murmure import file --strategy merge|replace` for IT deployment; hot-reload if app running.

**CLI**: `--transcription`, `--transcription-command`, `--paste-last`, `--cancel`, `--voice-mode`, `--quit`, `--llm-mode N`, `--llm-transform N`, `--hidden`, `--autostart`, `--help`, `--version`, `import`, plus headless `transcribe FILE` (stdout = text, logs on stderr; `--no-dictionary`, `--dictionary-file`, `-v`). If app not running the command starts it first. Early detection skips full init.

**Tray**: Open Murmure, Copy last transcript, Quit, with labels pushed from the webview for i18n; tray icon states via `set_tray_idle`/recording.

**Overlay behaviours**: hidden/recording/always modes; pixel-grid audio visualiser coloured by mode (standard/llm/command); cancel button; mode "flash" text when switching prompt (cold-start handoff via pending state); streaming text with frozen segments + provisional segment; result panel after dictation (copy, pause/resume countdown, close, auto-hide at 3x duration watchdog); inline error text; LLM processing animation; click-through input regions (`use-overlay-input-region`, `input_region.rs`) so only interactive pieces take clicks.

**Packaging**: Windows MSI + NSIS setup + winget, SignPath signing; macOS aarch64 + x86_64 dmg; Linux deb (installer script `install.sh`), AppImage, rpm, AUR PKGBUILD, udev rule for uinput (`packaging/linux/60-murmure-uinput.rules`), postinst/prerm/postrm scripts; docs site (mkdocs, EN/FR); French and English i18n (2 UI languages: `src/i18n/locales/fr.json` + source strings as keys).

## B3. UI structure
- Layout: left collapsible shadcn sidebar with groups (Home / Personalize / Extensions / Settings / About), `Page` header + sections (`SettingsUI.Section` with icon and title, separators between rows), pages titled with `Typography.MainTitle` plus muted paragraph. Components: button, checkbox, collapsible, command (combobox), dialog, dropdown-menu, input, number-input, popover, progress-bar, select, separator, sheet, skeleton, slider, switch, tabs, textarea, tooltip, kbd/render-keys, word-tag, rule-card, model-card, step-item, clear-button, reset-button, match-mode-toggle.
- Extension pages follow a template: `extension-page-layout` + `extension-header` + CTA (call-to-action enabling screen) + animated preview component + enable action + settings.
- Overlay (`src/overlay/overlay.tsx`): single black rounded container. Priority of content: error text > mode flash (prompt/mode name) > result panel > visualiser (+ cancel) with optional streaming text block above/below. Size variants small/medium/large via `VISUALIZER_CONFIG`.

## B4. Notable techniques and bug fixes
- Wayland strategy, three tiers: (1) GlobalShortcuts portal on KDE6/Hyprland/Sway, (2) CLI mode with OS-level custom shortcut on GNOME (push-to-talk impossible because key-release is not delivered; default record mode forced to toggle), (3) injection by a long-lived `/dev/uinput` virtual keyboard (no external daemon), with XKB layout detection (`wayland_xkb/*`: char map, dead keys, layout detect) so accented text types natively and unreachable chars are folded/skipped; udev rule shipped; failure emits event for onboarding card. enigo's XTEST only reaches XWayland, not native Wayland windows.
- Shared Enigo session kept alive (recreating re-prompts the RemoteDesktop portal every time).
- Wayland focus: hide overlay before paste, then wait extra (150 ms clipboard propagation + 400 ms after recent overlay hide so KWin restores focus before synthetic Ctrl+V). Tracks "ms since last overlay hide".
- Overlay on Linux: GTK layer shell loaded dynamically (libloading) with keyboard mode NONE so it never takes focus; on compositors without wlr-layer-shell (Mutter, Muffin) default overlay is HIDDEN because fallback misplaces it and steals focus. Destroy-vs-hide per platform: keep hidden webview alive on Windows/macOS to avoid 200-400 ms WebView cold start; destroy on X11 fallback to avoid stale transparent frames; recreate races handled by "present in place".
- GTK calls always dispatched to the main thread (SIGABRT otherwise).
- macOS: shortcuts by POLLING key state (CGEventSource) rather than rdev because event synthesis by enigo corrupted rdev events; separate CGEventTap only for fn/Globe key and to SUPPRESS matching key events (avoids the macOS "bonk" alert); tap auto re-arm when macOS disables a slow tap; first-click-on-cancel-button consumed by window activation fix; activation policy for dock hiding.
- Windows: OEM keys resolved via scan code so AZERTY etc. map correctly.
- Paste waits for physical modifiers to be released (synthetic keys merge with real key state), plus extra margin for X11 auto-repeat; shortcuts suspended while injecting; overlapping paste-last guarded by atomic flag; clipboard snapshot/restore.
- wl-copy: feed text on stdin (not argv: ARG_MAX, ps leaks) and null stdout/stderr because wl-copy forks a daemon that inherits pipes and would block `wait()` forever.
- Start sound timing: capture starts after the audible part of the start beep so the mic does not record the beep nor have it ducked.
- Mic selection on Linux temporarily routes PulseAudio/PipeWire default source; friendly device labels scored from metadata, GUID-like strings hidden, duplicate names suffixed.
- Per-chunk fresh decoder state + 0.25 s silence padding each side; forced-cut overlap and dedupe; ghost-text prevention on cancel (flag suppresses freeze-segment emission).
- Dictionary correction gated by model per-word confidence; calibrated thresholds with eval corpus.
- Shared reqwest client (rebuilding per request churns TLS/DNS); rustls provider installed explicitly (0.23 panics otherwise); never send API key over plain HTTP to a public host.
- Cold-start handoff pattern: events fired before overlay webview exists are staged in managed state and consumed on mount.
- Log watchdog caps log size; noisy crates demoted in log config.
- CLI cold path prints errors to stderr and exits non-zero (script-friendly); hot path (running instance) logs and shows window instead.

## B5. What Murmure lacks
No model choice or download manager, no process isolation, no idle unload (model resident forever), no hold-or-toggle, no remembered-press logic, no history beyond 5 entries (by privacy policy), no file transcription UI, no forced language, no GPU path.

---------------------------------------------------------------------------------------------------

# Part C. Prioritised recommendations for Whispr

Whispr context: Rust agent + worker (already matches Handy's process-isolation idea), Slint UI on Win/mac/X11, GNOME Shell extension pill on GNOME, Parakeet 110M via sherpa-onnx, Silero VAD, small llama.cpp cleanup, 1 GB budget, no network at runtime.

Value H/M/L, Effort S/M/L.

| # | Item | Value | Effort | Source / notes |
|---|------|-------|--------|----------------|
| 1 | Pure, unit-tested hotkey state machine with hold-or-toggle ("Auto"), 300 ms threshold, 50 ms release grace for X11 auto-repeat, 30 ms press debounce (external triggers exempt), remember/forget presses while busy | H | M | Handy `transcription_coordinator.rs`. Whispr's `whispr toggle|start|stop` socket maps onto "external" edges. |
| 2 | Hang/crash supervision of the worker: per-call deadlines scaled by audio length, stderr tail capture, kill after grace, auto-restart, worker exits when parent pipe closes | H | M | Handy `engine_supervisor/supervisor.rs`. Whispr already frames over stdio; add deadlines and recovery. |
| 3 | Memory hygiene on glibc: pin mmap threshold and call `malloc_trim` after each dictation, in both agent and worker | H | S | Handy `memory.rs` (#1792). Cheap win for the 1 GB budget. |
| 4 | Overlay state spec: arming dot until first real mic sample, 9-bar smoothed waveform, transcribing/processing spinner states, cancel button, timer, model-loading notice only after 2 s, error line; generation counter so stale delayed-hide cannot hide a newer session; throttle level events to ~30 fps | H | M | Handy `RecordingOverlay.tsx`, `overlay.rs`; Murmure overlay priority order (error > flash > result > visualiser). Reuse for both Slint overlay and GNOME pill. |
| 5 | Result/last-transcript surfaces: "paste last transcript" hotkey/command, tray "copy last transcript", optional result card with copy + countdown | H | S | Both repos. Works when focus was wrong. |
| 6 | Dictionary done properly: ASR-time phrase boosting if sherpa-onnx hotwords are usable for the transducer, plus confidence-gated fuzzy correction (never override confident words; disable beyond ~100 entries; min length; 1 vs 2 edits), import/export .txt, undo toast | H | M | Murmure `engine/boost_tree.rs` + `dictionary/`, Handy `audio_toolkit/text.rs`. Design only (AGPL). Check sherpa hotwords support first (UNVERIFIED for 110M). |
| 7 | Replace rules / voice commands: contains/exact/regex, ordered, drag reorder; plus built-ins (trailing space, short-text decapitalise, text-to-number); language-gated filler removal | M | M | Murmure `formatting_rules/`; Handy `text.rs`. Can run before the LLM cleanup, or let rules replace the LLM on weak hardware. |
| 8 | Paste robustness ladder: save/restore clipboard (text and image), serialise pastes, wait for modifiers released, suspend own hotkeys during injection, configurable pre/post delay, per-app "terminal" chord, Direct typing, auto-submit key; later receipt-sequenced restore on Windows/macOS | H | M | Handy `clipboard.rs`/`paste_tx/`; Murmure `clipboard/`, `shortcuts.rs`. Build the simple version first, receipts only if restore bugs appear. |
| 9 | Settings app structure: sidebar (General, History, Models/Model, Dictionary+Rules, Cleanup(AI), Advanced, About), grouped rows (title, description, control), search; Debug page hidden behind a shortcut with live log viewer | H | M | Handy `Sidebar.tsx`, Murmure `SettingsUI.Section`. Replaces the "few boxes" UI. |
| 10 | Onboarding flow: Welcome/language -> Permissions (mic; macOS accessibility; Wayland/uinput hints) -> Model (progress that survives quit) -> Demo -> "Try it" with live feedback ("held", "tapped", "nothing heard", "mic silent", "too slow") and speed measurement | H | M | Handy `components/onboarding/*`; Murmure checklist card on Home (3 tasks) as lighter variant. |
| 11 | History: SQLite, raw vs cleaned text, save/star, delete, retention (never / count / 3 d / 2 w / 3 m), re-transcribe from stored audio only if audio retention is opted in (Whispr never writes audio by default, so make this opt-in or skip), open file to transcribe via drag-drop; copy buttons | M | M | Handy `managers/history.rs`, Murmure last-5 (privacy-first alternative: RAM-only default). Respect "engine never writes audio to disk" rule. |
| 12 | Digital-silence vs no-speech distinction + typed dictation outcome (Typed/Empty/Silent/Failed) feeding toasts and onboarding | M | S | Handy `actions.rs`. Gives actionable "your mic is muted" message. |
| 13 | Mic robustness: device list with friendly names, persisted label for disconnected-mic banner, fallback to default with persisted result, mute-or-duck other audio while recording with exact prior-state restore, optional keep-mic-warm window (30 s), input channel selection | M | M | Handy `managers/audio.rs`; Murmure `microphone.rs`, `output_volume/*`. |
| 14 | Import/export of settings + dictionary + rules in one versioned JSON, merge vs replace, plus headless `whispr transcribe FILE` that prints text to stdout with logs on stderr | M | M | Murmure `cli/import.rs`, `cli/cli.rs`. Headless transcribe also useful for tests (`--input-wav` already exists). |
| 15 | Wayland/GNOME honesty: detect session + compositor, default overlay hidden where layer-shell is absent (your GNOME pill covers this), CLI custom-shortcut recipes (GNOME, KDE, Sway, Hyprland) generated in-app with copy buttons, force toggle mode when key-release is unavailable | M | S | Murmure `settings/types.rs` defaults, `configure-shortcuts-on-linux.md`; Handy README. Whispr's extension already beats both on GNOME; add the recipe UI for non-GNOME Wayland. |
| 16 | Settings robustness: container-level serde defaults, schema version + migrations, unknown-enum fallback, secret redaction in Debug | M | S | Handy `settings.rs` (#1619). Prevents a bad settings file bricking the agent. |
| 17 | Live preview text in overlay (committed vs tentative) when VAD chunking permits: Parakeet 110M is non-streaming so use chunk-at-pause previews like Murmure rather than true streaming | M | L | Murmure `audio/streaming.rs` + `chunking.rs`; Handy `live.rs`. Consider only after the basics; adds worker RAM/CPU. |
| 18 | Model catalogue UI: sections ("installed", "recommended for your languages"), speed/accuracy chips, download/cancel/delete with size freed, verify hashes, bundled offline catalogue JSON | M | L | Handy `catalog/`, `modelTable/*`. Only matters once Whispr offers more than one ASR/LLM model; start with a two-row version. |
| 19 | Pipeline instrumentation shown to user: Settings > Debug live logs, "last dictation: audio s, wait s" strip, `--selftest` result surfaced | L | S | Handy `LiveLogViewer`, `TranscriptionCompleteEvent`. |
| 20 | Local HTTP/unix API for other tools (transcribe WAV, apply cleanup prompt): Whispr already has D-Bus and a control socket; expose `transcribe` on the socket, keep loopback-only + Origin check if HTTP is ever added | L | M | Murmure `http_api/server.rs`. Low priority; engine "no network at runtime" rule argues for socket only. |
| 21 | Select-text voice commands ("Transform"/"Command" mode) using the existing llama.cpp model | M | L | Murmure `llm/transform.rs`. Strong differentiator but needs a bigger LLM than a "small cleanup" model; defer. |
| 22 | Wake word / hands-free mode | L | L | Murmure `wake_word/`. CPU-heavy, conflicts with idle-unload memory policy. Skip for now. |
| 23 | Phone-as-mic (QR + TLS WebSocket) | L | L | Murmure `smartmic/`. Network surface contradicts offline rule. Skip. |
| 24 | Packaging checklist: portable mode marker, SMAppService login item on macOS, deb/rpm/AppImage/AUR, udev rule for uinput if X11 typing falls back, winget/brew later, update channel with signature verification, What's New notes | L | M | Both. Do after features. |
| 25 | i18n from day one with a lint rule against hard-coded strings, RTL awareness | L | M | Handy AGENTS.md + 26 locales. Slint supports translations via gettext; decide early. |

### Top picks in order
1 hold-or-toggle state machine, 2 worker hang supervision, 3 malloc tuning, 4 overlay state spec, 5 paste-last and last-transcript surfaces, 6 dictionary (boost + gated fuzzy), 8 paste ladder, 9 settings IA, 10 onboarding with Try-it, 7 replace rules, 12 silent-mic outcome, 13 mic robustness, 11 history, 16 settings robustness, 15 Wayland recipes.

## UNVERIFIED / not confirmed
- Murmure model residency: no unload code found; did not trace every `engine` assignment.
- Murmure tray icon state images and exact tray tooltips; only menu items read.
- Handy live-streaming quality for non-whisper models and which catalog models stream (capability data lives in bundled catalog JSON, not read).
- Handy Windows/macOS reliable-paste field results (flagged as debug "beta" by authors).
- Whether sherpa-onnx hotwords/contextual biasing works with Parakeet TDT 110M (not in either repo; Murmure uses its own decoder).
- Exact text-to-number language coverage in Murmure (formatter calls a helper; not read).
- Murmure UI locale count (only `fr.json` present besides keys-as-English).
