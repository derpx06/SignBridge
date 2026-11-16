# Offline dictation: landscape and what Whispr takes from it

Snapshot 2026-10-26. Compiled from five research agents. Stars, push dates, licences and languages were read
from the GitHub API where marked verified. Latency and RAM numbers are the projects' own claims, or absent:
**no project publishes measured latency or RAM we could verify.** Anything marked UNVERIFIED must be
measured before we act on it.

## 1. The projects

| # | Project | Stars | Last push | Licence | Stack | Local ASR | Specialty |
|---|---|---|---|---|---|---|---|
| 1 | [Handy](https://github.com/cjpais/Handy) | 34.7k | 2026-10-25 | MIT | Rust, Tauri 2, WebKitGTK | Whisper, Parakeet V3 (ggml/GGUF) | Widest platform reach; worker-process isolation; CLI/portal hotkeys |
| 2 | [OpenWhispr](https://github.com/OpenWhispr/openwhispr) | 10.0k | 2026-10-26 | MIT | Electron, React | whisper.cpp, sherpa-onnx Parakeet, llama.cpp cleanup | Broadest feature set; per-compositor paste adapters |
| 3 | [Buzz](https://github.com/chidiwilliams/buzz) | 22.2k | 2026-10-25 | MIT | Python, Qt | whisper.cpp, faster-whisper | File transcriber; packaging breadth (not dictation) |
| 4 | [FluidVoice](https://github.com/altic-dev/FluidVoice) | 12.5k | 2026-10-25 | GPL-3.0 | Swift, macOS | Parakeet (FluidAudio) | Claims near-zero-delay streaming; enhancement runtime is closed |
| 5 | [Vibe](https://github.com/thewh1teagle/vibe) | 7.9k | 2026-10-06 | MIT | Tauri | Whisper, Parakeet v3, Nemotron | File transcriber; GPU build matrix (not dictation) |
| 6 | [VoiceInk](https://github.com/Beingpax/VoiceInk) | 6.9k | 2026-10-26 | GPL-3.0 | Swift, macOS 15+ | whisper.cpp, Parakeet, SenseVoice | Per-app "Modes", dictionary, replacements |
| 7 | [Whispering / Epicenter](https://github.com/EpicenterHQ/epicenter) | 4.8k | 2026-10-22 | AGPL-3.0 (README) | TS, Svelte, Tauri | none (OpenAI-compatible endpoint) | Pivoted to voice notes; no longer dictates into other apps |
| 8 | [OpenSuperWhisper](https://github.com/Starmel/OpenSuperWhisper) | 3.1k | 2026-10-20 | MIT | Swift, Apple Silicon | whisper.cpp, Parakeet | Minimal native app; single-modifier and mouse-button hotkeys |
| 9 | [FreeFlow](https://github.com/zachlatta/freeflow) | 3.0k | 2026-10-23 | MIT | Swift, macOS | cloud (Groq), not local | Tap-to-latch from hold; voice edit of selected text |
| 10 | [ydotool](https://github.com/ReimuNotMoe/ydotool) | 2.4k | 2025-12-22 | AGPL-3.0 | C | n/a | Typing via uinput (root daemon) |
| 11 | [nerd-dictation](https://github.com/ideasman42/nerd-dictation) | 1.9k | 2025-10-10 | GPL-3.0 | Python | Vosk | Simple hackable CLI; weak accuracy |
| 12 | [Speech Note](https://github.com/mkiol/dsnote) | 1.7k | 2026-10-24 | MPL-2.0 | C++, Qt | whisper.cpp, faster-whisper, Vosk, others | Only one using the GlobalShortcuts portal; also TTS |
| 13 | [Voxtype](https://github.com/peteonrails/voxtype) | 1.7k | 2026-10-18 | MIT | Rust | 9 engines incl. Parakeet, Moonshine, SenseVoice | Best typing fallback chain; MPRIS media pause; CLI control |
| 14 | [hyprwhspr](https://github.com/goodroot/hyprwhspr) | 1.3k | 2026-10-25 | MIT | Python | Parakeet TDT v3 (onnx-asr), Whisper | Hyprland-first; layer-shell mic OSD |
| 15 | [WhisperWriter](https://github.com/savbell/whisper-writer) | 1.1k | 2024-08-24 (dormant) | GPL-3.0 | Python, PyQt5 | faster-whisper | Four recording modes (continuous, VAD-stop, toggle, hold) |
| 16 | [Vocalinux](https://github.com/VocaHQ/vocalinux) | 966 | 2026-10-26 | AGPL-3.0 | Python, GTK | whisper.cpp, Parakeet, Vosk | Many packaging formats; RemoteDesktop-portal typing |
| 17 | [hyprvoice](https://github.com/LeonardoTrapani/hyprvoice) | 273 | 2026-09-19 | MIT | Go | whisper.cpp + cloud | Hyprland only |
| 18 | [BlahST](https://github.com/QuantiusBenignus/BlahST) / [Blurt](https://github.com/QuantiusBenignus/blurt) | 181 / 111 | 2025-07 / 2026-05 | BSD-3 / GPL-3.0 | Shell / JS | whisper.cpp | Clipboard-based GNOME helpers |
| 19 | [whisrs](https://github.com/y0sif/whisrs) | 140 | 2026-10-23 | MIT | Rust | whisper.cpp or cloud | uinput + evdev, bundled GNOME extension |
| 20 | [speech2text-extension](https://github.com/kavehtehrani/speech2text-extension) | 77 | 2026-10-06 | GPL-3.0 | JS | Whisper (D-Bus service) | GNOME panel button; no auto-typing on Wayland |
| 21 | [TuxTalk](https://github.com/ronb1964/TuxTalk) | 31 | n/a | MIT | Python | Parakeet, Whisper | Own uinput keyboard; skips password fields |
| 22 | [WayVoice](https://github.com/pavlenkosa/wayvoice) | 34 | 2026-10-25 | AGPL-3.0 | Python, GTK4 | faster-whisper, Parakeet (experimental) | Clipboard + optional auto-paste |
| 23 | [ibus-sherpa-onnx](https://github.com/roojs/ibus-sherpa-onnx) | 0 | 2026-10-21 | none | Vala | sherpa-onnx, Nemotron | IBus engine; early stage |
| 24 | [Ghost Pepper](https://github.com/matthartman/ghost-pepper) | 3.2k | 2026-07-27 | MIT | Swift, macOS (Apple Silicon) | Whisper, Parakeet v3, Qwen3-ASR, Nemotron | Closest to our stack: Whisper/Parakeet plus local Qwen 0.8B cleanup (about 1-2 s), editable prompt |
| 25 | [TypeWhisper](https://github.com/TypeWhisper/typewhisper-mac) | 1.9k | 2026-10-25 | GPL-3.0 | Swift, macOS | WhisperKit, Parakeet, Qwen3-ASR | Most complete: dictionary that learns, snippets, per-app workflows, undo, restore raw, recovery |
| 26 | [Murmure](https://github.com/Kieirra/murmure) | 1.1k | 2026-10-22 | AGPL-3.0 | Tauri (Rust) | Parakeet TDT 0.6B v3 int8 | Only other Parakeet app on Win/Linux/macOS; no GNOME push-to-talk |
| 27 | [Amical](https://github.com/amicalhq/amical) | 1.6k | 2026-10-26 | MIT | Electron | Whisper | Per-app formatting; no Linux |
| 28 | [Pindrop](https://github.com/watzon/pindrop) | 622 | 2026-10-04 (archived) | MIT | Swift, macOS | WhisperKit, Parakeet, Nemotron | Always copies to clipboard; dictionary learns from corrections |
| 29 | [Tambourine Voice](https://github.com/kstonekuan/tambourine-voice) | 390 | 2026-06-29 | AGPL-3.0 | Rust, React, Python | cloud default, local Whisper | Editable three-part cleanup prompt; re-type last transcription |
| 30 | [BetterWispr](https://github.com/opennookorg/betterwispr) | 447 | 2026-10-24 | Apache-2.0 | macOS | local | Voice commands ("scratch that"), 10-minute auto-stop, raw vs corrected history |
| 31 | [notune/android_transcribe_app](https://github.com/notune/android_transcribe_app) | 328 | 2026-10-17 | MIT | Java + Rust | Parakeet TDT 0.6B v3 GGUF (485 MB) | Android IME and system recogniser; no INTERNET permission |
| 32 | [whisperIME](https://github.com/woheller69/whisperIME) | 651 | 2026-10-12 | MIT | Android | Whisper TFLite | IME + recogniser service |
| 33 | [Transcribro](https://github.com/soupslurpr/Transcribro) | 762 | 2026-09-29 | ISC | Kotlin | whisper.cpp + Silero VAD | Android keyboard and system STT, English only |
| 34 | [FUTO Keyboard](https://github.com/futo-org/android-keyboard) | 3.4k | 2026-09-23 | FUTO Source First (not OSI) | C++/Kotlin | Whisper (separate Voice Input app) | Offline Android keyboard |

Not studied in depth: voicetypr, tambourine-voice, SpeakoFlow, voltip (small Rust projects); Numen (site unreachable).
Wispr Flow itself is cloud-only, so it is a reference for UX, not a local competitor.

## 2. What every project gets wrong (our openings)

1. **Linux text insertion is the universal failure.** `wtype` does not work on GNOME or KDE. `ydotool` needs a root daemon and drops non-ASCII. Clipboard-and-timer paste pastes stale text (Handy issue #502, 90 comments). Handy loses the first character on GNOME Wayland (#429).
2. **Overlays steal focus.** Handy ships its Linux overlay **off by default** because the injected paste goes to the overlay. It is also empty on Hyprland and flickers under WebKitGTK.
3. **Heavy runtimes.** Electron (OpenWhispr, Wispr Flow about 800 MB) and Tauri/WebKitGTK (Handy). Our idle agent is about 22 MB.
4. **Cold start drops the first words** (VoiceInk, push-to-talk starts about 0.5 s late).
5. **Whisper hallucinates on silence** ("Thank you for watching"). VAD gating is mandatory.
6. **No cancel for hold-to-talk** (OpenWhispr top issue).
7. **Cleanup that rewrites meaning.** Only we have a guardrail with negation and number checks.
8. **No published measurements anywhere.** A reproducible benchmark is itself a differentiator.
9. **Nobody else types into GNOME apps through `Main.inputMethod`.** Our extension route has no counterpart in this survey.

Also from the fifth agent:
- **Cleanup-LLM validation:** Ghost Pepper runs a Qwen 0.8B cleanup at about 1-2 s, so our 0.6B step is the proven size class. Almost no project documents a guardrail. Voxtype falls back to raw text on timeout, and a macOS Parakeet + Ollama write-up (Dictee) checks language and drift before pasting. None publish thresholds, so our negation and number checks are ahead.
- **Trust features users praise:** clipboard safety net, dictation recovery, undo last dictation, restore raw text, raw vs cleaned history (TypeWhisper, BetterWispr, Pindrop).
- **Parakeet "invents words from silence"** (Dictee write-up): gate on real speech, as our VAD does.
- **"Scratch that" should be deterministic code,** not left to the LLM (Tambourine relies on the LLM; a 0.6B model will make mistakes).
- **Murmure** is the closest cross-platform rival (Parakeet, Tauri, AGPL) and has no GNOME push-to-talk. Our extension is the difference there.
- Mobile: Parakeet GGUF runs on Android at 485 MB (notune). An Android port would need an IME plus a system recogniser service. iOS: no open-source offline keyboard was found (UNVERIFIED).
- Pin model checksums (Vocalinux does).

## 3. Architecture lessons, what Whispr takes

| Lesson | Source | Whispr action |
|---|---|---|
| Model in a child process, unload by exiting | Handy `engine_supervisor` | Already done. Keep. |
| glibc retains buffers: pin `M_MMAP_THRESHOLD=128KB` (about 15 MB leaked per 2-min dictation) | Handy #1792 | Add to agent and worker; re-run the RSS selftest |
| Typing chain per compositor, ending in clipboard | Voxtype, OpenWhispr | GNOME: extension. Sway/Hyprland/river: `wtype`, then `dotool`. KDE: RemoteDesktop portal/libei, then clipboard. X11: xdotool. |
| Wait for held modifiers to release before pasting | OpenWhispr `linux-fast-paste` | Needed for clipboard fallback |
| Never restore the clipboard on a timer | Handy #502 | Keep dictated text on the clipboard; never restore |
| Overlay: layer-shell with no keyboard interactivity, or drawn by the extension | Handy, hyprwhspr | GNOME: extension pill (done). Others: layer-shell spike (Slint cannot do it natively; evaluate `layer-shika` or smithay-client-toolkit) |
| `whispr toggle/start/stop/cancel` CLI as the universal hotkey | Handy, Voxtype, OpenWhispr | Already there |
| Mic pre-roll buffer so the first words are not clipped | VoiceInk issue | Verify our pre-roll covers cold model start |
| Idle unload of the cleanup LLM | OpenWhispr | Have 5 min idle; add shorter LLM-only unload |
| Pause media while dictating (MPRIS) | Voxtype | Add |
| Per-app cleanup prompts, dictionary, replacements | VoiceInk, OpenWhispr | Planned |
| Tap-to-latch from hold | FreeFlow | Have hold or tap; add latch |
| Esc cancels; confirm only for takes over 30 s | Superwhisper | Add |
| Do not use SIGUSR1 for IPC | Handy #1660 | We use a socket; fine |
| Beeps switch Bluetooth to the HFP profile | Voxtype issue | Sounds off by default |
| Graceful CPU fallback when allocation fails | Voxtype issue | CPU-only, so n/a, but never lose a dictation |

## 4. Engines and performance

From the engines agent. Model-card WERs are self-reported.

| Model | Licence | Params | English WER (avg) | Streaming | Fit |
|---|---|---|---|---|---|
| Parakeet TDT 110M (current) | CC-BY-4.0 | 114M | 7.49 | No | Lite default. 25 s decodes in about 1.0 s, 310 MB peak (measured by us). |
| Moonshine v2 small / medium | MIT | 123M / 245M | 7.84 / 6.65 | Yes | Strongest new candidate. Medium is reported at or below Parakeet 0.6B WER at 40% of the size. In sherpa-onnx: UNVERIFIED. |
| Parakeet TDT 0.6B v2 | CC-BY-4.0 | 600M | 6.05 | No | Quality tier on 16 GB machines |
| Nemotron streaming 0.6B | NVIDIA Open Model | 600M | 6.93 to 8.43 by chunk size | Yes | Best true streaming, heavy for 8 GB |
| Whisper small / turbo | MIT | 244M / 809M | 8.59 / n/a | pseudo | Dominated by Moonshine at the same size; hallucinates on silence |

Gaps: no Indian-English WER exists in any primary source, so we must measure it ourselves. Sherpa hotwords
work only with `modified_beam_search` on transducers; whether the NeMo-TDT offline path accepts them is untested.

**Latency plan (stop to text under 600 ms):** about 150-250 ms end-of-speech detection (trial TEN VAD against
Silero) plus under 150 ms tail decode plus under 200 ms cleanup. Cleanup is the bottleneck at 0.8-1.0 s today.
Fixes in order: skip the LLM for short or clean text; cache the prompt-prefix KV state; prompt-lookup decoding
(output mostly copies input); clean stable partials while the user is still talking.

## 5. UI and UX spec

From the UI agent (partly verified; Superwhisper and Handy docs were read, other apps are UNVERIFIED).

- **Pill states** (logical px, bottom-centre, 24 px above the work area, on the focused monitor, never focusable, click-through):
  arming (model loading, amber), listening (about 148x36, five-bar live waveform, accent blue on a dark surface with a 1 px hairline), processing (blue shimmer), done (green check, 700 ms), error (up to 320x44 with Copy and Retry, 6 s), cancelled (400 ms).
- **Motion:** appear 160 ms ease-out with slight scale and slide; waveform with fast attack and slower release; respect reduced-motion; pair colour with an icon.
- **Waveform doubles as a mic diagnostic:** a flat line means the mic is dead. Show "no signal" after 1.5 s.
- **Settings pages:** General (hotkey, hold/toggle/hybrid, autostart, language), Audio (mic picker with meter, silence auto-stop, sounds), Overlay, Dictionary and Snippets, Modes (per app), Commands, History (retry, copy, undo last insertion), Models and Memory, Privacy ("no network"), Advanced (paste method, logs), About.
- **Onboarding:** welcome (offline statement), mic with live meter, hotkey with conflict detection, a typing test into a practice box, then dictate one sentence. Skippable and re-runnable.
- **GNOME has no tray by default,** so settings must be reachable from the extension.
- **Top ten details,** in priority order: never steal focus and never lose text; show Listening within about 100 ms and buffer audio from key-down; live mic meter with a no-signal error; Esc cancels; hold, toggle and hybrid hotkeys plus CLI; history with retry and undo; guided Linux setup that tests typing; honest model-loading state; per-app modes and dictionary; voice commands ("new line", "scratch that") that are never typed as text.

## 6. Where Whispr can win, and how we prove it

| Claim | Evidence required | Status |
|---|---|---|
| Lowest RAM | Idle agent plus peak per tier, published | Idle 22 MB, peak 815 MB with cleanup (measured) |
| Most reliable Linux typing | A typing matrix: GNOME, KDE, Sway, Hyprland, X11 | GNOME only built, and not yet verified with a real mic |
| Fastest stop to text | p50/p95 benchmark, 3/10/30 s takes | 2.0 s on 18.7 s with cleanup; target under 600 ms |
| Most accurate | WER on LibriSpeech, noisy, Indian English | 0% on the selftest clip only |
| Never changes your meaning | Zero negation or number flips on the cleanup test set | Guardrail added; needs a measured run |
| Truly offline | cargo-deny network ban plus empty-netns e2e | Planned |

Rule: no "better than X" claim ships until its row has a measured number from `whispr-engine --bench`.

## 7. Priorities that follow

1. Real-mic spoken test on GNOME (still the unknown behind the original bug).
2. Typing fallback chain and the focus-safe pill on KDE/Sway/Hyprland.
3. Add `M_MMAP_THRESHOLD`, MPRIS pause, Esc cancel, history with retry and undo.
4. Benchmark harness, then evaluate Moonshine v2 and TEN VAD against the current stack.
5. Packaging (Windows MSI first, since that is the machine the user can test).
