# OpenWhispr (Electron + React) - study notes for Whispr

Source: /home/manas/Documents/whispr-refs/openwhispr (v1.10.4, MIT). Read-only study; ideas and designs only, no code copied.
Primary sources: `README.md`, `CLAUDE.md` (137 KB, the authoritative internal reference), `package.json`, `src/helpers/*`, `src/components/*`, `resources/*` (native helpers), `src/locales/en/translation.json` (best map of every UI string and setting).
Anything not read directly is marked UNVERIFIED.

---------------------------------------------------------------------------------------------------

## 1. Architecture

### 1.1 Process and window split
- Electron 41. `main.js` boots managers; `preload.js` exposes a narrow `window.api` (context isolation on). `src/helpers/ipcHandlers.js` is the single central IPC registry (very large file). Renderer is one React 19 + Vite + Tailwind v4 + shadcn/Radix codebase, URL-routed into several windows (`src/AppRouter.jsx`, `src/App.jsx`).
- Windows (all the same bundle, different route): (a) the **dictation overlay** ("main window": a small always-on-top transparent frameless window holding the pill, the live-transcript panel, the assistant panel, toasts and the error card), (b) the **control panel** (normal window), (c) a **meeting-notification overlay** (content-protected so it never appears in screen shares), (d) onboarding permission-guide overlay on macOS (anchors to the System Settings window), (e) Settings is a modal inside the control panel, not a window.
- The overlay is ONE native window resized through a "size ladder" (panel > menu > toast > pill > base), owned by a single hook (`src/hooks/useMainWindowSizeOwner.js`) so animations are driven in CSS while native setBounds is kept minimal; `useWindowResizeCompensation.js` masks split setBounds frames with CSS counter-translation. Sizes in `src/helpers/windowConfig.js` (`WINDOW_SIZES`: base/recording box 208x120, error 112/168 high, menu 240x280, toast/expanded 400x500, control panel compact 480x624 / expanded 1000x740, notification 416x84 growing to 512). The pill footprints (idle 40x40, recording 98x36, hover cancel 28 + 8 gap) are a declared cross-process contract in `src/helpers/voicePillPresentation.js`.
- Pure "policy" modules everywhere (`dockPolicy.js`, `autoStartPolicy.js`, `localServerPolicy.js`, `meetingMicGate.js`, `meetingMicHoldback.js`, `dictationRouting.js`, `dictationLifecycle.js`, `hotkeyRepeatGate.js`, `smartSpacing.js`, `localSpeechGate.js`, `micWarmState.js`): Electron-free, injected clock, unit-tested with `node --test` under `test/helpers/`. This is the repo's best structural habit.
- Dictation lifecycle is a 4-state machine `idle / preparing / recording / processing` plus an input kind `dictation / assistant / translation` (`src/helpers/dictationLifecycle.js`). Hotkeys are ignored while `processing`.

### 1.2 Native helpers (small separate processes, stdout line protocol)
Source in `resources/`, built by `scripts/build-*.js` under `compile:native`; downloaded prebuilt where CI builds them (Windows MSVC):
- Windows: `windows-key-listener.c` (WH_KEYBOARD_LL, emits KEY_DOWN/KEY_UP, verifies compound modifiers; this is what gives true hold-to-talk), `windows-fast-paste.c`, `windows-mic-listener.c` (WASAPI session monitor), `windows-system-audio-helper.c` (process-loopback capture excluding own process tree, Win10 2004+, falls back to Chromium loopback), `windows-text-monitor.c`; plus bundled `nircmd` and PowerShell SendKeys as fallbacks.
- macOS (Swift): globe-key listener, fast-paste, text-monitor (AX: selected text, focused-field value, app context: app name, bundle id, window title, URL host), mic-listener (CoreAudio process objects), audio-tap (system audio), calendar-listener (EventKit), window-bounds (System Settings geometry for the permission guide), media-remote (pause music), `macos-disclaim-exec.c`.
- Linux: `linux-fast-paste.c` (X11 XTest + optional uinput + optional GIO RemoteDesktop portal + optional AT-SPI; flags: `--capabilities`, `--await-modifier-release <ms>`, `--atspi-target`, terminal detection list kept in sync with JS), `linux-key-listener.c` (reads /dev/input evdev for push-to-talk on Wayland; JS side `linuxKeyManager.js` filters to real keyboards by sysfs capability bits), `linux-system-audio-helper.c`, `linux-text-monitor.c/.py` (AT-SPI probe `--probe-editable <pid>`).
- `native/meeting-aec-helper/` is a C++/CMake WebRTC AEC3 helper (echo cancellation for meetings).
- Sidecar hygiene: `sidecarRegistry.js`, `sidecarPidFile.js`, `sidecarReaper.js`, `ownProcessPids.js` track child PIDs and reap orphans from a previous crash.

### 1.3 Model runtime management
- **whisper.cpp**: `whisperServer.js` runs the bundled `whisper-server` binary as a local HTTP server (health check every 5 s, 30 s startup, 120 s for Vulkan), models in `~/.cache/openwhispr/whisper-models/` (tiny/base/small/medium/large/turbo GGML). GPU packs are optional downloads (CUDA `whisperCudaManager.js`, Vulkan `whisperVulkanManager.js`, `gpuBinaryManager.js`) with a CPU fallback toast ("GPU acceleration unavailable, Retry GPU") and a failure-reason classifier (`whisperGpuFailureReason.js`). Whisper VAD model via `whisperVadConfig.js` (Silero VAD toggles separately for dictation, note recording, meetings).
- **sherpa-onnx**: bundled `sherpa-onnx-ws` (offline websocket server) and `sherpa-onnx-online-ws` (online/streaming websocket server) executables, managed by `parakeetWsServer.js` / `parakeet.js` / `parakeetServer.js` (CLI fallback). Registry field `runtime: offline|online` picks the binary. Models: Parakeet TDT 0.6B v3 (25 langs), parakeet-unified-en-0.6b, Nemotron streaming (EN and multilingual, cache-aware online), Cohere Transcribe (served by the same manager, language is part of server identity so a language change restarts it), "orukeet" (their own, mostly cloud; UNVERIFIED local use). Partial/final JSON merged by `parakeetWsResult.js`; long audio segmented (`parakeetSegmentation.js`, 15 s offline, 30 s Cohere). Windows: ONNX Runtime DLL renamed `ow-onnxrt.dll` and PE import tables rewritten (`scripts/lib/pe-imports.js`) so the System32 `onnxruntime.dll` is never picked up. `-no-tts` sherpa archives only (avoids shipping GPL espeak-ng).
- **llama.cpp**: `llamaServer.js` runs `llama-server` (Vulkan variant via `llamaVulkanManager.js`), ONE shared server for all local LLM scopes; `localServerPolicy.js` decides what to prewarm and when it may stop (stop when no scope needs the loaded model); idle timeout **5 min** frees VRAM (`IDLE_TIMEOUT_MS`), startup timeout 120 s, `llamaContextPolicy.js` sizes context. LLM scopes: `dictationCleanup`, `dictationAgent`, `noteFormatting`, `chatIntelligence` (`src/config/inferenceScopes.ts`) resolved with fallback chains (`selectResolvedLLMConfig` in `src/stores/settingsStore.ts`). Provider registry in `src/services/ai/inferenceProviders/` (anthropic, enterprise, gemini, groq, lan, local, openai, openwhispr cloud).
- **ONNX utility process** (`src/helpers/onnxWorkerClient.js` -> `src/workers/onnxWorker.js`): hosts onnxruntime-node (text embeddings, speaker embeddings) in an Electron utility process; lazy-spawned, crash-isolated, killed on timeout, exits idle.
- **Qdrant sidecar + MiniLM** for semantic note search, fully lazy: nothing starts at launch, first semantic search activates, released after 5 min idle, FTS5 serves in the meantime, SQLite triggers journal pending vector changes (`semanticSearchLifecycle.js`, `qdrantManager.js`, `vectorIndex.js`). Hybrid search with reciprocal rank fusion (K=60).
- **Diarization**: sherpa pyannote segmentation 3.0 + 3D-Speaker CAM++ embeddings, voice fingerprints matched across meetings (`diarization.js`, `speakerEmbeddings.js`, `liveSpeakerIdentifier.js`).
- Memory policy summary: everything heavy is lazy and has an idle release (llama 5 min, Qdrant 5 min, ONNX worker idle exit, mic master-stream hold opt-in, mic warm TTL 5 s). No single process-per-dictation model like Whispr's; the Electron renderer + servers stay resident, so total RAM is far above Whispr's 1 GB budget (UNVERIFIED numbers, none documented).

### 1.4 Data flow, hotkey to pasted text (local path)
1. Hotkey fires (platform-specific source, see 1.5) -> `hotkeyManager.js` slot (`dictation`, `voiceAgent`, `translation`, `meeting`) -> repeat gate (`hotkeyRepeatGate.js`: autorepeat fires within a 600 ms window renewing it count as one press) -> `windowManager.sendToggle...` IPC to overlay.
2. Main captures the **paste target** early (hotkey press time), not at paste time: `selectionManager.js` + native text monitors give app identity, pid, window title, selected text, caret/editable status (opaque "caret session"), cached 250 ms across the burst of call sites.
3. Renderer `useAudioRecording.js` + `audioManager.js` open the mic (warm-state/hold logic), MediaRecorder chunks are spooled to IndexedDB (`recordingSpool.js`) so audio survives a main-process crash; a parallel 16 kHz mono PCM tap with pre-roll (`pcmTap.js`, capped 4 min) is what local engines receive, avoiding an FFmpeg decode after stop. WebM kept for history/cloud.
4. Start cue sound and optional pause-media; waveform level from audio analyser; local speech gate (`localSpeechGate.js`, RMS/peak thresholds) skips transcription of silence.
5. Online models stream PCM over a persistent websocket during capture and commit the flushed text at stop (no second decode); partials feed the live preview. Stop flush is truncation-aware; anything unclean falls back to record-then-transcribe. Offline models use buffered 1.5 s chunks for preview only and decode once at stop.
6. Routing (`dictationRouting.js`, `reasoningRouting.js`, `transcriptionRoute.ts`): plain dictation -> optional cleanup LLM; agent-addressed ("Hey <name>") -> reasoning; voice-assistant hotkey -> always agent route; selection present -> in-place edit scope; translation hotkey -> translation chain (`translationChain.js`).
7. Cleanup output is validated (`src/utils/cleanupOutput.ts` rejects model output that doubles the transcript, strips "Cleaned transcript:" labels); failure pastes the raw dictation and shows a "Cleanup failed" toast.
8. Dictionary words feed the ASR prompt / cleanup prompt (`dictionaryPromptCap.js`, `dictionaryEchoFilter.js` removes echoed dictionary terms); snippets expand by trigger; smart spacing appends a trailing space except after CJK (`smartSpacing.js`).
9. Paste (`clipboard.js`): save clipboard (text + rich/image), write transcript, wait for held modifiers to release (Linux), inject paste chord, then restore the original clipboard after a platform delay. Paste queue serialises consecutive pastes until the previous restore completes (`pasteQueue` / `restoreGate`). Delays: paste 120 ms macOS, 10 ms Windows fast helper (30/40 ms nircmd/pwsh), 50 ms Linux X11, 0 on Wayland; restore 450 ms macOS, 500 ms Windows, 800 ms Linux, 1200 ms KDE Wayland; streaming paste uses 15/50 ms. Restore is skipped if the clipboard changed meanwhile.
10. Text-edit monitor (`textEditMonitor.js` + native text monitors) polls the focused field (500 ms) after paste and diffs original vs edited text to **auto-learn dictionary words** (`src/utils/correctionLearner.js`: edit distance, common-word stoplist, ignores trivial swaps); toast "Added X to your dictionary" with Undo.
11. History row written to SQLite (`database.js`, better-sqlite3): original and processed text, method, agent name, error, optional audio retained per retention setting.

### 1.5 Linux per-compositor handling
Hotkeys (`hotkeyManager.js`, detection in `linuxSession.js`):
- **GNOME Wayland**: preferred is the XDG GlobalShortcuts portal (`gnomeGlobalShortcutsPortal.js`, GNOME 48+, supports Hold); fallback registers a custom keybinding via `gsettings` under media-keys custom-keybindings and a D-Bus service `com.openwhispr.App` (pure-JS `@homebridge/dbus-native`); GNOME runs `dbus-send ... Toggle()`. Tap-only on the fallback. Keysym conversion handles F13-F24 by reading the active keymap (`xkbcomp`) because stock xkeyboard-config has no F16 etc.; refuses keys with no keysym or already reserved by gsd-media-keys. Error strings: key not in keymap, reserved by GNOME, hold needs portal.
- **KDE**: `kdeShortcut.js` talks to KGlobalAccel over D-Bus (`setShortcut`/`doRegister`, listens for `globalShortcutPressed`), Qt key code conversion. KDE per-monitor work areas via plasmashell `evaluateScript` (`plasmaWorkAreas.js`) because Chromium clips only the primary display on X11/XWayland.
- **Hyprland**: `hyprlandShortcut.js` runtime `hyprctl keyword bind` / `unbind` -> same D-Bus `Toggle()`; ephemeral, re-registered at start; no push-to-talk. Detect via `HYPRLAND_INSTANCE_SIGNATURE`.
- **X11 / others**: Electron `globalShortcut`; evdev key listener (`linux-key-listener.c`) for push-to-talk needing /dev/input access; modifier-only combos need that listener.
- **COSMIC**: `cosmicToplevel.js` is a minimal hand-rolled Wayland wire client for `zcosmic_toplevel_info_v1` to learn the focused app id (xdotool answers stale on XWayland).
Paste (`clipboard.js`, `linux-fast-paste.c`, `ensureYdotool.js`):
- X11: native XTest binary first, xdotool fallback. Hyprland: wtype -> `hyprctl sendshortcut` -> uinput/ydotool. Sway/wlroots: wtype -> uinput. COSMIC: wtype only for known terminals, otherwise uinput Shift+Insert. GNOME/KDE: RemoteDesktop **portal keysyms** (restore token reused so the permission dialog appears once; 2.5 s vs 15 s timeouts), then uinput/ydotool. KDE sends Shift+Insert everywhere except Warp.
- Wayland physical fallbacks use **Shift+Insert** (layout-independent, not intercepted by TUI agents as "paste image"); terminals use Ctrl+Shift+V; Konsole drops simulated Ctrl+Shift+V so it gets Shift+Insert.
- Clipboard on Wayland-from-XWayland: save falls back to `wl-paste --type text` when Electron reads nothing; write with `wl-copy` (+ `--primary` for terminals) on compositors that do not bridge (`requiresWlCopy`); failures surface `wl-copy-missing` / `wl-copy-failed` and a Wayland Paste Setup settings card.
- Window class vs terminal: keep JS list and C list in sync (comment in both files).
- Autostart via XDG `.desktop` (Electron's login item is a no-op on Linux), honours `X-GNOME-Autostart-enabled=false`/`Hidden=true`, AppImage-aware `Exec`.
- Mic detection for meetings via `pactl subscribe`; system audio via native helper. GPU compositing disabled whenever an NVIDIA kernel module is loaded (flicker), blur skipped on Linux dialogs (CPU compositing cost).
- Pill click-through on Linux: native X11 input shape / region so only the visible pill takes the pointer (`linuxWindowInputRegion.js`, `useLinuxPillInteractivity.ts`), override-redirect window needs explicit "focus" request.

### 1.6 Other platform glue
- Dock policy (macOS icon follows control panel), tray identity GUID (Windows keeps tray placement across updates), autostart policy, relaunch waiter for AppImage, deep links (`linuxUrlSchemeHandler.js`), updater (`src/updater.js`, electron-updater), secrets via `safeStorage` -> OS keychain.
- Local HTTP bridge for the CLI: `cliBridge.js`, 127.0.0.1 ports 8200-8219, bearer token in `~/.openwhispr/cli-bridge.json` (0600), endpoints incl. `POST /v1/transcribe` taking a file PATH (never audio bytes).
- Packaging: electron-builder (`electron-builder.json`): dmg (arm64+x64) / NSIS (forced signing, unsigned variant config) / AppImage, deb, rpm, tar.gz; Homebrew cask (community); Nix flake (`flake.nix`, `nix/`); `afterPack.js` fails the build if required native binaries are missing; `scripts/download-*.js` fetch pinned binaries; CI rebuilds Windows helpers when `.c` changes.

---------------------------------------------------------------------------------------------------

## 2. Complete feature list

### Dictation
- Global hotkey, **tap-to-toggle or hold-to-talk** (hold needs a key-release source; unavailable on GNOME fallback/Hyprland), multiple hotkeys per slot (comma list, `hotkeyList.js`), Globe/Fn on macOS, right-side modifiers, mouse-button hotkeys on macOS, modifier-only combos; per-slot conflict validation; auto fallback to F8/F9 with a toast.
- Language selector (large registry `config/languageRegistry.json`), auto-detect where the model allows; Chinese script conversion (`opencc-js`, `chineseScript.js`).
- Cleanup LLM (optional), tone and style, dictionary, snippets, auto-learn from corrections, smart spacing, custom agent name for addressed commands, dictation translation (own hotkey and target language), selection-aware editing, start/stop cue sounds, pause media while dictating, auto-paste toggle, keep-in-clipboard toggle, retry from the error pill, interrupted-recording recovery (spool).
- Dictation-error pill with actions: Retry, copy, open settings; guidance for missing paste tools, held modifiers, mic failures, unavailable model.

### Voice assistant ("agent")
- Dedicated hotkey, no wake word; named assistant ("Hey <name>") also works in plain dictation. Streams into a floating **assistant panel**; follow-up by hotkey or typing; Esc collapses; Copy; conversations persisted and browsable in the control panel chat.
- If a verified writable caret exists, answer is pasted there and pill returns to idle; otherwise panel + clipboard. Highlighted text = in-place edit (or "question about selection" goes to panel).
- Tools: notes search/create/update/get/list folders, calendar events and availability, web search, copy to clipboard, snippets get, update dictionary and snippets by voice; paid connectors (email draft/Gmail send, Slack, Linear, GitHub) behind an approval card; optional screen-context screenshot (JPEG quality ladder, never stored). Cloud-only/paid: connectors, web search provider, referral and leaderboards (ignore).

### Notes
- Folders and spaces, rich Tiptap Markdown editor (bubble/floating menus, tables, task lists, mentions), AI summary tab with **templates** (context + ordered sections, e.g. AI Summary, custom) and **actions** (chips, `/` menu, action manager, shorten/lengthen, TL;DR), note chat (docked or in-view), undo for assistant edits, per-note audio record control, dictation widget, batch queue, drag and drop, search (FTS5 + local semantic), share (cloud only), import (Granola), export/mirror to Markdown files (`markdownMirror.js`, `markdownTargets.js`), command search (Ctrl+K).

### Meetings
- Detection from three sources (process: Zoom/Teams/Webex/FaceTime; mic start events; calendar reminders 1 min before), one coalesced always-on-top prompt (Join, Record) with calendar-aware naming, suppressed during recordings, 2.5 s cooldown afterwards.
- Dual-stream recording (mic + system audio), live transcription (local or streaming providers), echo handling in three layers (audio echo-leak detector with 0-500 ms lag search, mic gate verdicts send/zero/skip, text-layer holdback/dedupe/retract), optional AEC3, live speaker diarization with voice fingerprints, speaker labels You/Others, segment reducer with partial/final/retract events (pure, tested).
- Calendars: Google, Microsoft Graph, Apple EventKit; upcoming meetings list on home; attendees merged into note participants.
- Audio import: drag files, batch upload, URL download (yt-dlp, `urlAudioDownloader.js`), optional diarization.

### Dictionary and snippets
- Dictionary tab: add (comma separated), import (paste), export, edit, search, clear all, prompt-length warning ("Groq and Whisper read only the last few hundred characters"), auto-learn toggle. Snippets tab: trigger -> replacement, edit/remove, voice management through the assistant.

### Tone and style
- Per-app-category tone (personal messages, work messages, email, other) with formality levels; setup dialog and home banner offered after 70 dictated words; classification from app name, window title, browser host (`toneAppCategories.js`); applies to cleanup and to assistant drafts; strips em dashes on assistant paste.

### History and insights
- History (home): chronological list, copy, delete, re-run, show/hide discarded recordings, clear all, retention (audio days, transcript hours), storage usage list. Insights: words spoken, streak, longest streak, words per minute, dictation count, 6-month heatmap; computed on device.

### Settings pages (modal sections, `src/components/SettingsModal.tsx`)
Account, Plans & Billing, Workspace (cloud; ignore); **General** (appearance light/dark/auto, floating icon: auto-hide when idle and start position bottom-left/center/right, sound effects + pause media, notifications, clipboard (auto paste, keep in clipboard), notes files, startup (launch at login, start minimized), microphone picker, dictionary auto-learn, Wayland paste setup card, updates); **Hotkeys** (dictation with tap/hold, voice assistant, translation, meeting); **Tone & Style**; **Speech to Text** (provider tabs, local models, Silero VAD toggles per surface, GPU device, diarization, transcription preview toggle, AEC, self-hosted URL); **LLMs** (per-scope provider/model, prompt studio with editable system prompts, voice assistant scope with screen context and separate vision model); **Privacy & Data** (audio retention, transcript retention, save discarded, permissions tester: mic/accessibility/system audio, reset accessibility); **System** (updates, developer: model cache open/clear, reset all data, logs, debug mode, API keys).

### Onboarding (`src/components/onboarding/flow.ts`, versioned route-based step machine, session persisted so it resumes)
Steps: auth (cloud, skippable as guest) -> setup choice (cloud / own keys / local) -> required model download (background download tray) -> permissions (mic, accessibility, system audio; macOS overlay guides through System Settings) -> language -> use case -> dictation hotkey (tap/hold confirm) -> dictation demo (try it live) -> assistant hotkey + assistant demo (mail-thread reply scenario) -> notes + calendar connect. Local-only branch: `local-dictation`, `local-assistant`.

### Hotkeys
dictation, voice assistant, translation, meeting (start/stop recording), plus tray/menu actions; cross-slot conflict detection; helpful per-desktop error messages (listed in `locales/en/translation.json` -> `hotkey.errors`).

### Overlay/pill and live preview behaviour
See section 3.

### Tray
Menu (`src/helpers/tray.js`): dictate toggle label (start/stop), Ask assistant, Start meeting recording, Show/Hide dictation panel, Open/Hide control panel, Quit; left click toggles the control panel. Tooltip "OpenWhispr - Voice Dictation".

### CLI / API
- npm CLI `@openwhispr/cli` (`openwhispr` binary, docs in `agent-skills/openwhispr-cli/SKILL.md`): notes, folders, transcriptions, audio, auth; backend local (loopback bridge) or remote (REST + API key), auto-detected. `agent-skills/openwhispr-api` for agents. MCP server and public API are cloud-side. `examples/custom-asr-shim` for self-hosted ASR.

### Packaging
See 1.6.

---------------------------------------------------------------------------------------------------

## 3. UI structure

### 3.1 Control panel
- Layout: left **sidebar** (`ControlPanelSidebar.tsx`, collapsible, workspace switcher, profile menu, Settings and Support entries) + **top bar** (`ControlPanelTopBar.tsx`, page title from the same nav list, `controlPanelNav.ts` is the single source of nav truth) + content. Window has compact (480x624) and expanded (1000x740) modes.
- Pages: Home (dictation history with date grouping, upcoming meetings, Tone & Style banner, GPU banner, required-models banner), Insights, Chat (agent conversations; policy-gated), Notes (personal notes + folders + spaces), Upload (audio import), Dictionary (+ Snippets tab), Integrations (calendars, connectors, API keys, MCP), Settings modal.
- Components worth emulating: opaque cards for settings (`SettingsPanel`), grouped sidebar in settings (group labels App / AI Models / System), `SectionHeader` + description rows with switches, model picker cards with download state and size, permission cards with Test/Open-settings buttons, toast system with action buttons and progress border, command palette (Ctrl+K), empty-state illustrations, i18n (9 UI languages, `scripts/check-i18n.js` enforces key parity), RTL provider.
- Design tokens (`src/index.css`): light primary #4577e9, agent brand #8787ff, accent oklch purple, Inter variable font, Caveat for handwriting accents; dark theme; "reduced motion" fallbacks; shared keyframes (waveform-bar, toast-progress, check-pop, ring-fill, etc.).

### 3.2 Dictation pill (overlay) - states (`components/dictation/VoicePill.tsx`, `PillWaveform.tsx`, `LiquidCancelButton.tsx`, `voiceIdentityMorph.ts`, `liquidFusion.ts`)
- **idle**: 40x40 circular brand mark (logo), optionally auto-hidden entirely when idle (setting). Draggable; start dock position bottom-left / center / bottom-right.
- **hover**: pill dims to ~0.72 opacity ring, tooltip with hotkey ("<hotkey> to talk"); hover reveals a 28 px **cancel** control that fuses with the pill outline (liquid-fusion SVG morph tweened with the same easing as the pill).
- **preparing/thinking**: ~260 ms hold ("acknowledged press") before the shape changes; pulsing icon, cursor not-allowed.
- **recording**: expands over 300 ms (cubic-bezier 0.2,0,0,1) to a 98x36 capsule: brand icon + 52 px **level waveform** (bars scroll right-to-left, 80 ms sample interval so adjacent bars straddle syllables, heights written straight to the DOM from rAF, no React renders), waveform reveals 100 ms after the width finishes.
- **processing**: waveform freezes/collapses back, icon pulses, clicks ignored.
- **unavailable**: pulsing "unavailable" icon (model/mic not ready) with tooltip.
- **error card** (`DictationErrorCard.tsx`): the overlay grows to 112/168 px high; title, description (can embed the hotkey), action buttons with pending/success feedback, auto-dismiss progress bar that pauses on hover.
- **command menu** (`PillCommandMenu.tsx`, 240x280 window): quick actions (ask assistant, start meeting, etc.).
- Windows/macOS: `setContentProtection` while screen-context is on so the pill never appears in screenshots.

### 3.3 Live preview ("transcription preview", `LiveTranscriptPanel.tsx`, `useLiveTranscriptPanel.js`)
- Optional toggle. Panel grows out of the pill: encapsulate (180 ms) -> horizontal expansion toward screen centre (320 ms, direction depends on dock side) -> controls reveal -> measure text then expand height (measure-then-reveal so there is no layout jump) -> content reveal -> stream. Phases `listening / live / cleanup / final`; label "Listening..." / "Polishing..." / "Ready"; shimmer on the unsettled tail words (`splitTranscriptForShimmer`), stick-to-bottom scroll that releases on user scroll, copy button, collapse button, hover holds the panel open. Online models replace text from partial results; offline models append 1.5 s chunks. Surface limits in `voiceSurfaceGeometry.json` shared by JS and CSS.

### 3.4 Assistant panel (`AssistantPanel.tsx`, `ExpandingPanelShell.tsx`, `VoiceModePanelCore.tsx`, `useAssistantPanel.js`)
- Same shell as the live preview; max size derived from work area with a gutter (ratio-based limits, height fits content). Header/response stream with markdown, tool-call status lines ("Searching notes..."), approval cards for connector actions, footer swaps between the pill (when recording a follow-up) and action buttons (Copy, etc.) with 180-220 ms cross-fade choreography (`getAssistantFooterTransitionTimeline`). Empty state with example commands.

### 3.5 Other surfaces
- Meeting notification card (top-right, 416 px wide, content protected, Join/Record, folder picker). Meeting recording pill in notes. Toasts for hotkey fallback, GPU fallback, cleanup failures, learned words, retired models.

---------------------------------------------------------------------------------------------------

## 4. Notable techniques and bug fixes

1. **Modifier-release wait before paste** (`linux-fast-paste --await-modifier-release`): XKB state on X11, `EVIOCGKEY` on Wayland; 1.5 s cap on CLOCK_MONOTONIC; ignores ydotoold's virtual keyboard; on timeout does NOT paste, leaves text on clipboard and shows an error pill whose Retry re-pastes instead of re-recording. Target lookup is redone if focus moved while waiting (#2113).
2. **Clipboard restore discipline**: snapshot before, sentinel/ownership check before restore ("skip restore because clipboard changed"), per-platform restore delays (450-1200 ms), a paste queue that waits for the previous restore, rich/image restore, Wayland plain-text fallbacks; KDE stale-X11-owner workaround (#2055).
3. **Layout-independent paste on Wayland**: Shift+Insert instead of Ctrl+V (KWin resolves keysyms in active layout only so Ctrl+V on non-Latin layouts breaks); terminals Ctrl+Shift+V; Konsole exception; Warp exception.
4. **Caret verification before delivering assistant answers**: opaque caret session, revalidated immediately before paste (`pasteAtCapturedTarget`), fails closed to panel + clipboard on any change; unknown accessibility state (dormant Chromium AX tree, AXError -25212) treated as unsafe; terminals and secure fields excluded.
5. **Early target capture** at hotkey press, 250 ms freshness reuse; reactivate the captured window before paste on Windows (#859); `ACTIVATE_CONFIRM` polling after activation.
6. **Windows hold-to-talk through a native WH_KEYBOARD_LL helper** rather than Electron's globalShortcut (no key-up there); fallback to F8 and refusal of modifier-only keys when the helper is missing; `afterPack.js` hard-fails packaging without it.
7. **Autorepeat gate** for globalShortcut (600 ms renewing window) so a held key = one press.
8. **Mic**: warm-state TTL 5 s (a timestamp not a latch), cold-open worst case 10-15 s documented, optional master-stream hold so re-acquire is ~0.1 ms via track clone, stale/default device recovery (`micSelectionRecovery.js`, `staleMicDevice.js`, `activeMicRecovery.js`, `micTrackHealth.js`), pre-roll PCM tap.
9. **Crash-safe audio**: chunks spooled to IndexedDB directly (not via IPC) and "interrupted recordings" recovered at next launch (#2073).
10. **Streaming commit**: no second decode; truncation-aware finish deadline; graceful fallback chain to record-then-transcribe.
11. **ONNX Runtime DLL rename** + PE import rewriting to defeat System32 DLL hijack of the wrong ORT version (#2054).
12. **Meeting echo pipeline** as pure, tested seams with PCM fixture generators; "characterization" tests pin known oddities.
13. **GNOME F13-F24**: read keymap with xkbcomp, refuse keysyms that are missing or reserved (#1716).
14. **Pill hit-testing on Linux** using native input regions rather than polling the pointer; override-redirect windows need explicit focus grab.
15. **Single owners** for contested resources (Dock icon, main window size ladder, local llama server stop rule, semantic-search lifecycle) with unit-tested policies and explicit "never derive state from show/hide events (macOS occlusion)" comments.
16. **Cleanup safety net** (`cleanupOutput.ts`): reject duplicated-output hallucinations, then fall back to raw text.
17. **Echoed dictionary terms** filtered from output (`dictionaryEchoFilter.js`); dictionary prompt capped.
18. **Window resize flicker masking** with CSS-variable counter-translation; CPU-compositing awareness on Linux (NVIDIA module check, no blurs).
19. **Sidecar reaper** removes orphan servers from previous crashed runs via pid files.
20. Linux AT-SPI calls bounded to 500 ms each because libatspi gives unseen apps 15 s to answer (#1944).

---------------------------------------------------------------------------------------------------

## 5. What Whispr should add (offline only; cloud-only features ignored)

Whispr already has: Rust agent + worker split, Parakeet 110M + Silero + llama.cpp cleanup, GNOME extension pill, Slint UI, tray, socket CLI, memory policy. Value / effort:

| # | Item | Value | Effort | Notes / source |
|---|------|-------|--------|----------------|
| 1 | **Modifier-release wait before injecting paste** (XKB state on X11, evdev on Wayland, 1.5 s cap, then clipboard + error pill with re-paste Retry) | High | S | `resources/linux-fast-paste.c`, `clipboard.js` |
| 2 | **Clipboard save / restore with ownership check and a paste queue** (per-platform delays, skip restore if changed, wl-paste fallback) | High | M | `clipboard.js` |
| 3 | **Paste target capture at hotkey time + terminal detection** (Ctrl+Shift+V / Shift+Insert, Konsole, Warp exceptions) | High | M | `selectionManager.js`, `linux-fast-paste.c` |
| 4 | **Error pill with actions** (Retry paste, copy, open settings, hotkey in text, auto-dismiss with hover pause) as a dedicated pill state | High | S | `DictationErrorCard.tsx` |
| 5 | **Pill state machine + animations**: idle 40 px mark / hover cancel / 260 ms acknowledge / capsule with scrolling 80 ms-sampled waveform / processing pulse / unavailable | High | M | `VoicePill.tsx`, `voicePillPresentation.js`, `PillWaveform.tsx` (draw waveform outside the UI toolkit's reactive path) |
| 6 | **Live preview panel growing out of the pill** (phases listening/live/cleanup/final, shimmer on unsettled tail, stick-to-bottom, copy/collapse) | High | M | `LiveTranscriptPanel.tsx`; needs streaming ASR (sherpa online Parakeet/Nemotron models) or chunked offline preview |
| 7 | **Dictionary + auto-learn from user edits** (diff after paste, stoplist, undo toast) and dictionary echo filter | High | M | `correctionLearner.js`, `textEditMonitor.js`; Linux needs AT-SPI text read |
| 8 | **Snippets** (spoken trigger -> expansion) | Med | S | `SnippetService.ts` |
| 9 | **Hold-to-talk on every platform**: evdev listener on Linux, low-level hook on Windows, GNOME GlobalShortcuts portal Hold on GNOME 48+, clear per-desktop capability messages | High | M | `linux-key-listener.c`, `windows-key-listener.c`, `gnomeGlobalShortcutsPortal.js`, `locales/en/translation.json` `hotkey.errors` |
| 10 | **Per-compositor hotkey + paste matrix** (KDE KGlobalAccel, Hyprland bind, Sway wtype, portal RemoteDesktop with restore token, ydotool/uinput fallback) with a "paste tools" diagnostics card in settings | High | L | `kdeShortcut.js`, `hyprlandShortcut.js`, `clipboard.js` |
| 11 | **Onboarding as a resumable, versioned step machine**: model download in background, permissions with Test buttons, hotkey confirm (tap/hold), live demo step | High | M | `onboarding/flow.ts`, `PermissionGuideOverlay.tsx` |
| 12 | **Tone and style by target app** (classify app/window/URL host, tone suffix on cleanup prompt; off by default) | Med | M | `toneAppCategories.js`, `config/prompts/toneStyle.ts` |
| 13 | **Cleanup output validator and raw-text fallback** (doubled output, labels, empty) + "Cleanup failed, pasted original" notice | High | S | `cleanupOutput.ts` |
| 14 | **Crash-safe audio spool + interrupted-recording recovery** (offline-friendly, no audio retained unless user opts in; note Whispr rule: engine never writes audio to disk, so make it opt-in or RAM-only with watchdog) | Med | M | `recordingSpool.js`; conflicts with AGENTS.md, needs a decision |
| 15 | **History + Insights** (date-grouped list, copy, retention controls, words/streak/WPM/heatmap, all local) | Med | M | `HistoryView.tsx`, `InsightsView.tsx` |
| 16 | **Local speech gate** (RMS/peak window gate to skip silent recordings before spending model time) | Med | S | `localSpeechGate.js` (Whispr's Silero may already cover; check) |
| 17 | **Smart spacing** (trailing space except CJK) and smart punctuation of consecutive dictations | Med | S | `smartSpacing.js` |
| 18 | **Voice assistant with selection edit** (rewrite highlighted text in place via local llama, caret-verified delivery, panel fallback) | Med | L | `selectionEditing.js`, `AssistantPanel.tsx` |
| 19 | **Translation hotkey** (dictate in A, paste in B using local LLM) | Med | M | `dictationTranslationInference.js` |
| 20 | **Meetings / system-audio notes** (pactl/pipewire monitor capture, dual stream, diarization) | Med | L | big feature; only if scope expands beyond dictation |
| 21 | **Notes with templates/actions** | Low | L | out of Whispr's current scope |
| 22 | **Semantic search stack (Qdrant + MiniLM)** | Low | L | heavy; FTS5 is enough for dictation history |
| 23 | **CLI breadth**: `transcribe FILE`, `history`, `dictionary add`, JSON output; loopback bridge not needed since Whispr has a control socket | Med | S | `cliBridge.js`, `agent-skills/openwhispr-cli` |
| 24 | **Settings information architecture**: General / Hotkeys / Tone & Style / Speech to Text / LLMs / Privacy & Data / System, with card-style rows and permission testers | High | M | `SettingsModal.tsx` |
| 25 | **Idle-release rule table** (one rule file deciding which resident servers may stop: single shared llama server, 5 min idle) - Whispr already does this; adopt the pure-policy + unit test style | Low | S | `localServerPolicy.js` |
| 26 | **Sidecar reaper / pid files** for orphaned workers after agent crash | Med | S | `sidecarReaper.js` |
| 27 | **Pause media while dictating**, start/stop cue sounds, auto-hide idle pill option | Med | S | General settings; MPRIS on Linux (UNVERIFIED how OpenWhispr does it on Linux) |
| 28 | **Launch-at-login with `--hidden`**, XDG autostart that honours user-disabled entries and repairs stale Exec | Med | S | `linuxAutostart.js`, `autoStartPolicy.js` |

### Top architectural lessons
- Make paste reliability the product: capture target early, wait for modifiers, queue pastes, restore clipboard safely, fail closed to clipboard + visible error.
- Keep UI choreography declarative (shared timing constants, pure presentation functions, one owner for window size).
- Heavy resources are lazy with explicit idle policies in pure testable modules.
- Whispr advantage to keep: process-per-model-lifetime releases memory exactly; OpenWhispr's resident Electron + servers cannot match that budget.
