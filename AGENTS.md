# Voicio Android app

Voicio for Android is a Wispr-Flow-style dictation bubble: a floating bubble appears over any focused text
field (accessibility service + overlay), you hold it, speak, and cleaned text is typed into the field. Speech
runs on the phone (sherpa-onnx, Parakeet 110M + Silero VAD); nothing leaves it.

## Verified commands

The project uses Java 17. `--offline` does not work on this machine (kotlin-reflect 2.0.21 is not cached),
so run Gradle online. Another project shares this machine's memory: take its lock so builds do not get
OOM-killed:

```bash
cd /home/manas/Documents/whispr
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk GRADLE_USER_HOME=/home/manas/.gradle
flock /tmp/claude-1000/desklink-gradle.lock ./gradlew --no-daemon --max-workers=2 :core:test :app:testDebugUnitTest :app:assembleDebug
scripts/fetch-sherpa-android.sh   # once: native sherpa-onnx libs into app/src/main/jniLibs (git-ignored)
```

APK: `app/build/outputs/apk/debug/app-debug.apk` (package `io.github.derpx06.voicio.debug`; Kotlin namespace stays `com.whispr.app`, minSdk 29, arm64 only).
Models are downloaded in the app (checksum-pinned, `flow/ModelDownloader.kt`) into the app's private files dir.

## Architecture boundaries

- `core` is pure Kotlin/JVM (no Android, Compose, Room, mic, native, network). It holds the session reducer and
  `dictation/` (ports of the desktop Rust logic: `Trigger` hold/toggle state machine, `CleanupRules`, `Dictionary`,
  `VoiceCommands`, `Pipeline`, `FieldInsert`). Keep ports behaviour-identical to the Rust originals; parity
  fixtures live in `core/src/test/resources/parity/`.
- `app` is the Android boundary:
  - `engine/` (`AudioCapture`, `SherpaEngine`): mic, VAD, ASR. The vendored sherpa Kotlin API is in
    `com/k2fsa/sherpa/onnx` (Apache-2.0, unchanged).
  - `flow/`: `FlowAccessibilityService` (bubble + insertion), `DictationController`, `TextInserter`,
    `OverlayHost`, `ModelDownloader`, `FlowSettings`.
  - `ui/`: Compose UI. `Theme.kt` (colours, type, motion), `Kit.kt` (icons, ripple art, components),
    `Onboarding.kt`, `Screens.kt`. `data/Stores.kt`: encrypted history and notes (text only, no audio).
- Network use: the user-started model download, and the optional BYO-key cloud speech providers (`providers/cloud-stt.json`, off by default, per-provider consent). With cloud off, dictation never opens a connection.
- Look: "Paper & Ink" (same as the desktop capsule): warm paper background, ink-black capsule and tiles, orange accent, soft pastel
  panels, Bricolage Grotesque headings. The one brand shape is the honeycomb v2 (`Lattice` in `Kit.kt`: 37 bevelled hexagons,
  cream round an orange core; moods rest/voice/noise/working/tick/notice/progress) on the Home hero, bubble, keyboard mic key and icon.
- Voicio Keyboard (`ime/`, an IME next to the accessibility bubble): docked, or floating (draggable card over the app, not resized)
  and landscape (shorter keys, wider card). `cloud/` + `ui/CloudScreen.kt`: optional BYO-key providers, off by default.
  Review UI without a device: `:app:testDebugUnitTest` writes Robolectric snapshots to `app/build/snap/*.png`; look at them after UI edits.

## Desktop app (Rust workspace, `engine/`)

Two processes, so model memory is released exactly by ending a process:
- `voicio` (`engine/crates/app`): the always-running agent (~20 MB). It handles:
  - single instance and the control socket (`voicio toggle|start|stop|cancel|status|settings|quit`)
  - the mic (cpal), supervision of the worker, and the memory policy (5 min idle / pressure → unload)
  - the Slint UI on the main thread (tray, settings, overlay), global shortcut, text insertion
  - the D-Bus API `io.github.voicio.Engine` for the GNOME extension.
  It has no native ML dependencies and must keep type-checking for Windows and macOS.
  The recording pill is `src/pill/`: `model.rs` (pure, time-injected lifecycle and motion, unit-tested),
  `bands.rs` (bass/mid/treble loudness) and `paint.rs` (CPU signed-distance painter, identical on every OS),
  shown by the Slint `Overlay` in `ui.rs`. The look is the approved "Paper & Ink" canvas with
  honeycomb v2 (`Capsule.dc.html`, `Honeycomb2.dc.html`): an ink capsule 40 pt tall, 37 soft pointy-top
  hexagons in rings 1/6/12/18 (bevelled cream cells, orange core), rings lifting from the centre out with
  the voice (60 ms apart, ~200 ms ease), a slate rim for room noise, an amber sweep while working, a lime
  tick when typed, amber for low memory, calm coral for a problem; nothing flashes or shakes. Actions
  are hints ("Ctrl+Z to undo", "Open Voicio to fix") because the capsule never takes focus. Shortcut behaviour is `shortcut_mode` in `config.toml` (`hold` default: listens only while held, a
  press under 250 ms is discarded with a hint; `toggle`; `hold-or-tap`), implemented by the pure `Trigger`.
  On GNOME the agent binds the shortcut through the GlobalShortcuts portal (press AND release; extensions
  only get presses), the extension only types (`Attach2`), and binds the key itself only as a fallback if
  `ShortcutBound` stays false.
  On Wayland with XWayland the app runs on X11 (`desktop::wants_xwayland`, chosen on the winit builder,
  not by env var) because native Wayland ignores always-on-top, no-focus and placement. On GNOME the
  extension draws the pill and the Slint one is the fallback. The GNOME capsule draws the same honeycomb with Cairo. Review
  it without touching a real screen: `voicio pill-snapshot out.ppm` (painter only) or
  `xvfb-run -a voicio overlay-snapshot out.ppm "words" [quiet|noise|working|typed|warn:TEXT|error:TEXT]`
  (the real overlay path); app pages with `xvfb-run -a voicio ui-snapshot PAGE out.ppm` (0 Home, 1 History,
  2 Models, 3 Settings, 4 About, 5 Dictionary, 10..15 setup steps; env `VOICIO_SNAP_SIZE`, `_MENU`, `_MIC`,
  `_DL`, `_EMPTY`, `_HEALTH`, `_CLOUD`, `_CAPTURE`); the GNOME capsule with
  `engine/scripts/extension-harness/run.sh OUTDIR`. The app UI bundles Inter and Bricolage Grotesque
  (OFL, `crates/app/ui/fonts`); Slint's software renderer cannot round gradient fills, so `GradCard`
  masks the corners. Never run a pill or window on the user's own screen to test.
  Text on Wayland without the GNOME extension is typed through the XDG RemoteDesktop portal
  (`src/typing.rs`, one-time permission prompt, restore token in `~/.config/voicio/portal-token`), with the
  clipboard as fallback. The shortcut on GNOME also binds through the GlobalShortcuts portal and is ignored
  while the extension is attached. Settings `live_text` (words in the pill while speaking, or only after
  release) and `noise_reduction` live in `config.toml`. The default shortcut is F9 (hold to talk, tap to
  toggle; the mic stays open ~150 ms after a hold ends); `Config::migrate` moves a saved old default once.
  The pill's motion runs in fixed 240 Hz steps with Hex's closed-form springs, so it looks the same at any
  frame rate, and it is painted at the window's exact scale (never resampled). Text is typed on its own
  thread (never the UI thread, except macOS) so the pill never freezes while typing. Measure pacing with
  `VOICIO_PILL_STATS=1` (prints interval median/p95/p99, gaps over 40 ms and paint cost).
  Also in the agent: a Dictionary (`replacements.rs`, Hex's longest-match engine; entries in `config.toml`),
  persistent history (`history.rs`: text only, owner-only JSONL, retention in days, 0 = memory only, search),
  atomic writes and damaged-file recovery (`durable.rs`), and a health check (`health.rs`) whose first
  problem shows as a banner on Home. Typing ladder (`typing.rs`): X11 keystrokes, `wtype` (non-GNOME
  Wayland, text on stdin), RemoteDesktop portal, clipboard.
- `voicio-engine` (`engine/`, the root package): the model worker (`voicio-engine worker`), spawned on
  demand and talking `voicio-ipc` frames over stdin/stdout. It also holds the dev tools: the web page,
  `--selftest`, `--eval`, `--cleanup-test`. Pipeline: Silero VAD → Parakeet TDT 110M (sherpa-onnx) →
  "S1-mini" by "Superwhisper" cleanup (llama.cpp), loaded lazily per dictation. Audio first passes a
  streaming spectral Wiener denoiser (`src/denoise.rs`, pure Rust, on by default, `--no-denoise` to compare).
  Measured on 100 LibriSpeech files with fan-and-hum noise: 4.30→3.90 % WER at 6 dB, 8.76→8.31 % at 2 dB,
  25.6→18.6 % at −2 dB, clean speech unchanged: `voicio-engine --eval DIR --noise-snr 2 [--no-denoise]`.
- `voicio-ipc` (`engine/crates/ipc`): the frame codec + messages shared by both.

Models live outside the repo in `~/.cache/whispr-voice-lab` (`engine/scripts/fetch-models.sh`).

Cleanup model and memory (measured on this machine; re-measure after changing models or load flags):
- S1-mini loads from a slimmed copy, `s1-mini/s1-mini-tied.gguf`, made once from the download by
  `gguf::drop_tensor` (drops the duplicate Q6_K `output.weight`, 127 MB), memory-mapped with weight repack
  off (`LlamaModelParams::with_use_extra_bufts(false)`, a one-line setter added in `engine/vendor/llama-cpp-2`
  and wired through `[patch.crates-io]`). RSS 550 -> 428 MB for the model, same output. `VOICIO_REPACK=1`
  restores the old load for comparison.
- Standard tier (VAD + Parakeet 110M + S1-mini) is budgeted at about 750 MB and the agent passes the worker
  `1024 - agent RSS` as its budget, so the whole app stays under 1 GB. Lite is about 350 MB.
- Writing style: `cleanup_styling` (casual, semi-casual, semi-formal, formal), `cleanup_structure` (prose,
  lists) and `cleanup_context` (general, email) in `config.toml` and Settings. They are exactly the control
  line S1-mini was trained on (`engine/src/style.rs`); the model does not follow free instructions. A spoken
  directive at the start of a dictation ("email mode", "make it formal", "as a list") overrides them for that
  dictation. Lists and email are cleaned over the whole dictation, not per utterance.
- Evaluation: `engine/eval/fluency.tsv` (109 cases: category, input, reference) scored by
  `voicio-engine --eval-tsv eval/fluency.tsv [--model GGUF --style s1|chat --control formal,lists,email --verbose]`:
  exact match, word error rate, over-editing of correct sentences, meaning changes, guardrail rejections,
  latency, RSS. Results and the model comparison are in `docs/research/cleanup-model-bakeoff.md`.
- The guardrail (`cleanup.rs::guardrail`) treats "dont" and "don't" alike and allows number and email
  formatting to shorten the text; keep the Kotlin copy in `core/.../CleanupRules.kt` in step (not yet done).
- Release package: `engine/scripts/package-release.sh [--with-models DIR]` builds
  `dist/voicio-<version>-linux-<arch>.tar.gz` (+ .sha256): agent, worker, GNOME extension, launcher entry and a
  compiler-free `install.sh` (`--no-models`, `--no-service`, `--uninstall`) that downloads the checksum-pinned
  models with `voicio models install`. Test it with a throwaway `HOME=... ./install.sh --no-service --no-models`
  so it never touches the real service. Measured with the real models: whole pipeline loaded 546 MB, peak 748 MB
  on a 33 s recording (`voicio-engine --selftest --cleanup --no-diarize`, needs `test_wavs/0..8.wav`).
- The agent picks X11/XWayland only where it needs it: on GNOME with the Voicio extension enabled it runs on
  native Wayland (real cursor size, sharp scaling, app id `io.github.voicio.Voicio`); see `desktop.rs`.


```bash
cd engine
cargo test --release --workspace
cargo check -p voicio --target x86_64-pc-windows-gnu      # agent must stay cross-platform
cargo check -p voicio --target aarch64-apple-darwin
# on Linux `ring` needs a C cross-compiler (no mingw/macOS SDK here): point CC_<target> at a stub that writes empty .o files
# (check only, nothing links); CI's native Windows/macOS runners do the real compile.
./target/release/voicio-engine --selftest --cleanup --no-diarize   # asserts peak RSS < 1 GB
./target/release/voicio-engine --eval DIR                # WER (engine/scripts/fetch-eval.py)
./target/release/voicio-engine --cleanup-test eval/dictation.txt
VOICIO_INSTANCE=test ./target/release/voicio run --input-wav FILE  # isolated agent hearing FILE
engine/scripts/install.sh                                # ~/.local/bin, systemd unit (the service has a safety net of MemoryHigh=2500M / MemoryMax=3G; the app itself keeps each quality level to its own budget, under 1 GB for Lite and Standard), extension
```

- Tests that run an agent must set `VOICIO_INSTANCE`, or they talk to the user's real one.
- RAM budget: agent + worker stay under 1 GB (the systemd unit's 3 GB cap is only a safety net). Re-check `--selftest` peak after changing models,
  threads, context sizes or utterance length.
- `Cargo.lock` must stay committed: it pins `zip` 2.4.2 (all 2.x releases are yanked), which
  `sherpa-onnx-sys` needs. The toolchain is Rust 1.93, so `sysinfo` is pinned to 0.38.
- `src/ort_arena.rs` patches the ONNX Runtime C API table (linker `--wrap`, see `build.rs`). It is
  tied to the ORT bundled with `sherpa-onnx-sys =1.13.8`; re-check the table size and indices on upgrade.
- The engine worker never writes audio to disk and makes no network calls. Cloud requests live only in the app layer (`engine/crates/app/src/cloud.rs`) and only when the user turned a provider on. Keep it that way.
- `gnome-extension/voicio@derpx06.github.io` (GJS) talks to the agent over D-Bus and types through
  `Main.inputMethod`; keep its D-Bus interface in sync with `crates/app/src/dbus.rs`.
