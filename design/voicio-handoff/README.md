# Handoff: Voicio — desktop app + Chrome extension

## Overview
Voicio is private, offline voice dictation. You press a shortcut, speak, and cleaned-up text is typed into whatever field has focus. Everything runs on the user's computer.

This bundle covers two products:
1. **Voicio desktop app** (Linux first; Windows/macOS later) — main window, floating pill, tray menu, onboarding.
2. **Voicio for Chrome** — a **standalone** Manifest V3 extension. It does **not** need the desktop app. Speech is recognised by an on-device model running inside Chrome (WebGPU/WASM). Audio and text never leave the computer.

## About the design files
The `.dc.html` files in `designs/` are **design references built in HTML**. They show the intended look and behaviour; they are not production code. Recreate them in the target stack using its normal patterns. If no codebase exists yet, use the stack recommended below.

Open any `.dc.html` file directly in Chrome (keep `support.js` and `voicio-kit.js` next to them).

## Fidelity
**High fidelity.** Colours, type, spacing, radii, copy and interactions are final. Match them closely.

## Recommended stack (if starting fresh)
| Part | Choice | Why |
|---|---|---|
| Desktop shell | **Tauri 2** (Rust) + React + TypeScript | Small binary, native tray, global shortcuts, no Chromium bundled |
| Audio capture | `cpal` (Rust) at 16 kHz mono | Cross-platform mic access |
| VAD | **Silero VAD** (ONNX) | Detects speech start/stop, drives auto-stop |
| Speech to text | **NVIDIA Parakeet TDT 0.6B** (ONNX via `ort`) — Quality tier: Parakeet TDT 1.1B | Fast, accurate, offline. CC-BY-4.0 — credit required |
| Clean-up | **“S1-mini” by “Superwhisper”** (local LLM, GGUF via llama.cpp) | Punctuation, filler-word removal, formatting |
| Typing output | Linux: `ydotool`/`wtype` (Wayland), `xdotool` (X11); Win: `SendInput`; mac: `CGEvent`. Paste mode: clipboard + Ctrl/Cmd+V, then restore clipboard | “Type it” vs “Paste it” setting |
| Floating pill | Separate transparent, always-on-top, **non-focusable** Tauri window (`focus: false`, `skipTaskbar`, `decorations: false`, `transparent`) | Must never steal focus from the target app |
| Chrome extension | MV3, React + Vite (`crxjs`), TypeScript | |
| Extension speech | Parakeet/Whisper-class ONNX model via **onnxruntime-web (WebGPU, WASM fallback)** in an **offscreen document**; model cached in OPFS/IndexedDB | Fully offline in the browser |
| Extension pill | Content script, **Shadow DOM** root, `all: initial` | Looks right on any site, isolated from page CSS |

## Files
```
designs/
  Voicio.dc.html               Design board: tokens, state notes, every extension screen in light + dark
  Voicio Desktop.dc.html       END-TO-END desktop prototype (desktop, editor, app window, pill, tray)
  Voicio for Chrome.dc.html    END-TO-END Chrome prototype (browser, page, popup, pill, context menu, options tab)
  Voicio App.dc.html           Desktop main window — Home, History, Models, Dictionary, Settings, About & Debug
  Voicio Onboarding.dc.html    Desktop onboarding, 6 steps (prop: step 1–6)
  Voicio Pill.dc.html          Desktop floating pill — all states, over blurred desktop / editor
  Voicio Components.dc.html    Component sheet
  Chrome Popup.dc.html         Extension toolbar popup 360×480 — all states
  Chrome Options.dc.html       Extension full settings page (all popup controls + advanced)
  Chrome In-page.dc.html       In-page pill, field mic + tooltip, right-click menu
  Chrome Permission.dc.html    Mic pre-prompt and “Permission blocked” help
  Voicio Extension.dc.html     (older v0 of the extension — superseded by the Chrome* files)
  voicio-kit.js                Icon paths, waveform, level meter, logo mark, spinner (reference implementation)
  support.js                   Runtime needed to open the .dc.html files
```
Each `.dc.html` has props (top of the file, `data-props`) that switch screens/states: e.g. `Voicio App` → `screen`, `variant`; `Chrome Popup` → `view`, `rec`.

---

## Design tokens

### Colour
| Token | Dark | Light | Use |
|---|---|---|---|
| `bg` | `#101417` | `#F6F7F7` | Window / page background |
| `sunk` | `#0C1013` | `#EDEFEF` | Sidebar, segmented-control track |
| `surf` | `#171C20` | `#FFFFFF` | Cards, inputs, transcript |
| `surf2` | `#1F252A` | `#EEF1F1` | Hover fill, selected tab/nav |
| `line` | `#242B30` | `#E2E6E7` | Soft 1px borders, dividers |
| `line2` | `#323A41` | `#CED4D6` | Control borders, switch-off track |
| `tx` | `#E9EDEF` | `#101417` | Primary text |
| `tx2` | `#A6B0B6` | `#475157` | Secondary text |
| `tx3` | `#85919A` | `#646E74` | Hints, captions, **unsettled live words** |
| `ac` | `#14B8A6` | `#0F766E` | Primary button, waveform, switch-on |
| `ac2` | `#22D3EE` | `#0E7490` | Mic-button gradient start, waveform edges |
| `on-ac` | `#03201C` | `#FFFFFF` | Text/icons on `ac` |
| `ac-tx` | `#2DD4BF` | `#0F766E` | Accent text and icons |
| `ac-soft` | `rgba(20,184,166,.14)` | `rgba(15,118,110,.09)` | Chips, glows, privacy card |
| `ac-line` | `rgba(45,212,191,.35)` | `rgba(15,118,110,.30)` | Idle mic ring, highlighted card border |
| `warn` / `warn-soft` | `#FBBF24` / `rgba(251,191,36,.12)` | `#A16207` / `rgba(234,179,8,.14)` | Low disk, needs attention |
| `err` / `err-soft` | `#F87171` / `rgba(248,113,113,.12)` | `#B91C1C` / `rgba(220,38,38,.08)` | Errors, destructive |
| `key` / `key-edge` | `#1F252A` / `#3A434A` | `#FFFFFF` / `#C3CBCE` | Keycap face / 2px bottom edge |
| `knob` | `#A6B0B6` | `#FFFFFF` | Switch knob when off |
| `pill` | `rgba(23,28,32,.88)` | `rgba(255,255,255,.90)` | Floating pill + live card (with `backdrop-filter: blur(20px) saturate(1.3)`) |
| `shadow` | `0 12px 32px rgba(0,0,0,.45)` | `0 12px 32px rgba(16,20,23,.12)` | Floating surfaces only |
| `focus` | `#5EEAD4` | `#0F766E` | 2px focus ring |

Mic button fill: `linear-gradient(160deg, ac2, ac)`. Waveform bars blend from `ac` (centre) to `ac2` (edges) with `color-mix(in oklch, …)`.

### Type
Inter (UI) + JetBrains Mono (shortcuts, numbers, timers, hostnames, logs).
| Role | Size/line | Weight | Tracking |
|---|---|---|---|
| Onboarding H1 | 32/40 | 600 | −0.025em |
| Page H1 | 22–26/30–34 | 600 | −0.02em |
| Window title | 15/20 | 600 | −0.01em |
| Status line | 17–18/24 | 600 | −0.01em |
| Body (base) | 14/20 | 400 | 0 |
| Buttons, row labels | 13–13.5/16–20 | 500 | 0 |
| Captions, chips, group headers | 12/16 | 500–600 | 0 |
| Mono | 11–13 | 400–500 | 0 |

### Spacing, radius, motion
- Spacing: 8px grid with 4px half-steps — 4, 8, 12, 16, 24, 32, 40, 48.
- Radius: 5 small keycap · 6 keycap · 8 buttons/inputs/nav items · 10 segmented track/rows · 12 cards/popup · 14–16 windows/panels · 999 pill, chips, dots, switch.
- Borders: 1px `line` (cards) or `line2` (controls). Almost no shadows — only pill, popup, menus, tooltips, toasts.
- Motion: 150 ms hover/press, 200 ms layout, `cubic-bezier(.2,.8,.2,1)` (ease-out). Waveform bar `scaleY(.22→1)` 0.42–0.92 s alternate with random negative delays; ring pulse 1.8 s `scale(1→1.3)` + fade; spinner 0.9 s linear; pop-in `scale(.6→1)` 180 ms.
- `prefers-reduced-motion: reduce` → turn off every animation; bars hold their static heights.
- Icons: Lucide-style line icons, 24px grid, **1.75 stroke**, round caps/joins, `currentColor`. Shown at 12/14/16/20/24/36 px. Paths are in `voicio-kit.js` (`P`).

### Logo mark
Five rounded vertical bars on a teal tile (radius 28% of size). Inner area is 58% of the tile; each bar is 13.5% of the inner width. The bars sit lower towards the middle, so together they make a “V”. Bar [top, bottom] as a fraction of the inner area: `[.06,.40] [.25,.64] [.47,.94] [.25,.64] [.06,.40]`. Bars use `on-ac`, the tile uses `ac`. Sizes: 14, 16, 18, 22, 24, 32, 44, 48, 64.

---

## Shared components (see Voicio Components.dc.html)
- **Buttons** — height 32 (34–38 for prominent actions), radius 8, 13px/500, 6px icon gap.
  - Primary: `ac` background, `on-ac` text. Hover `filter: brightness(1.1)`.
  - Secondary: `surf` background, 1px `line2`. Hover `surf2`.
  - Ghost: transparent, `tx2` text. Hover `surf2` + `tx`.
  - Danger: `err-soft` background, `err` text. Hover adds a 1px `err` border.
  - Disabled: 45% opacity, `cursor: not-allowed`.
  - Focus (all buttons): `outline: 2px solid focus; outline-offset: 2px` via `:focus-visible`.
- **Icon button** 28–32 square, radius 8, `tx3` icon. Hover `surf2` + `tx` (destructive ones hover `err-soft` + `err`). Always has an `aria-label`.
- **Segmented control** — track 30–32 high, 2px padding, radius 10, `sunk` background, 1px `line`. Selected option: `surf` background, 1px `line2`, `tx`; other options `tx2`. Use `role=radiogroup`; arrow keys move the selection.
- **Switch** 34×20, radius 999. On: `ac` track, `on-ac` knob at x=17. Off: `line2` track, `knob` at x=3. Knob is 14px. 150 ms. `role=switch`, `aria-checked`.
- **Keycap** — mono 12/500, height 24, padding 0 7px, radius 6, `key` face, 1px `line2` border plus a 2px `key-edge` bottom. Large: 40 high, mono 15, 3px bottom edge. Combos are joined with a `tx3` “+”.
- **Chip/badge** height 22–24, radius 999, 12/500. Variants: Installed (`ac-soft`/`ac-tx` + check), Downloading (`surf2` + teal dot), Failed (`err-soft`), Low space (`warn-soft`), app chip (1px `line2` border).
- **Progress bar** 4px (6px in onboarding), radius 999, `line` track, `ac` fill (paused: `tx3`, error: `err`). Mono meta line underneath: “684 MB of 1.1 GB · 18.4 MB/s · 24 s left”.
- **Card** — `surf` background, 1px `line`, radius 12, padding 12–16.
- **List row** 40–48 high, radius 10, hover `surf2` across the full width. Actions use 28px icon buttons.
- **Toast** 36 high, radius 10, inverted (`tx` background, `bg` text) with a teal check. Lasts 1.8–2 s. Undo variant has an `ac` text button. Error variant: `surf2` background + `err` border.
- **Tooltip** 30 high, inverted, radius 8, 12/500 plus a mono shortcut hint. Shows after 400 ms hover or on focus; Esc hides it.
- **Tray / context menu** — 4px padding, radius 10–12, `bg` background, 1px `line2`, `shadow`. Items are 32–34 high, radius 6–8, hover `surf2`. Separators are 1px `line`.
- **Waveform** — 40 bars, 4px wide, 4px gap, 56px tall on Home. Pill uses 18–20 bars, 3px wide, 3px gap, 22px tall. Modes: `live` (animated), `processing` (bars at 45% height shimmering in a left-to-right sweep), `idle` (4px dots in `tx3`), `static`.
- **Level meter** — 20–32 segments, gap 3, radius 2. Teal segments, the last 16% amber, over a `line2` track; animated `clip-path`.

---

## DESKTOP APP

### Window chrome (Voicio App.dc.html)
- 960×640, radius 16, 1px `line`.
- Left sidebar, 208 wide, `sunk` background, 1px right border, padding 12:
  - Logo 24 + “Voicio” 15/600.
  - Nav items 34 high, radius 8: **Home 1 · History 2 · Models 3 · Dictionary 4 · Settings 5**, each with its number hint in mono `tx3`. Active item: `surf2` background, `tx` text, teal icon.
  - Bottom: a status card (“On this device” + “Standard · Parakeet + S1-mini”) and an **About & Debug** link.
- Main header 52 high: title 15/600 plus a `tx3` subtitle. On the right, window controls (minimise, maximise, close to tray); close hovers `err-soft`.

### 1. Home
- Centred column. Record button 128px inside a 168px area with a radial `ac-soft` glow.
  - **Ready**: `surf` background, 1px `line2` border, teal 36px mic, a 1px `ac-line` ring at inset 8.
  - **Listening**: gradient fill, 8px `ac-soft` halo, two pulsing rings (offset 0.9 s).
  - **Cleaning up**: a spinner ring around a `surf` button showing a wand icon.
- Waveform: 40 bars, 56 tall.
- Status line by state:
  - Ready: teal dot + “Ready”, then “Press [Super]+[Alt]+[Space] anywhere, or click the button.”
  - Listening: breathing dot + “Listening…” + mono timer, then “Speak naturally. Press the shortcut again to finish · [Esc] to cancel”.
  - Cleaning up: amber dot + “Cleaning up…”, then “Removing filler words and fixing punctuation”.
- **Last dictation card** (max-width 600): “Last dictation” + “App · 2 min ago” + a Copy button; the text (3 lines max); a mono line “48 words · 0:19 · cleaned up”.

### 2. History
- Search input 280×32 with a search icon and a “/” keycap. “Clear all” ghost button turns red on hover.
- Note bar: lock icon + “Kept in memory only. Turn on saving in Settings.” + an “Open Settings ›” link.
- Rows grouped under “Today” / “Yesterday”. Grid columns: 48 time (mono) · 96 app · 1fr text (one line, ellipsis) · copy + delete icon buttons.
- Deleting shows the toast “Dictation deleted”; copying shows “Copied to clipboard”.
- **Empty state**: 104px circle with a dashed outer ring, a static mini waveform and a teal dot. Then “No dictations yet”, “Dictations from this session show up here until you quit. Press Super + Alt + Space to start one.” and a “Try a dictation” button (goes to Home).
- **No search results**: “No matches” / “Nothing in this session mentions “x”.”

### 3. Models
- Segmented control **Automatic / Lite / Standard / Quality** plus a primary **“Get what I need”** button. The caption under it changes with the choice (e.g. “Automatic picks Standard for this computer — 16 GB memory, 41 GB free.”).
- Model cards: 36px icon tile · name 14/600 + kind 12 `tx3` · description 13 `tx2` · mono size/RAM on the right · a 112px action column.
  - Parakeet TDT 0.6B — Speech recognition — 640 MB / 1.2 GB RAM — **Installed**
  - S1-mini — Clean-up — 1.1 GB / 1.8 GB RAM — **Downloading**: progress bar + “684 MB of 1.1 GB · 18.4 MB/s · 24 s left” + Pause/Resume + Cancel (cancelled → Download button)
  - Silero VAD — Voice detection — 2 MB / 20 MB — Installed
  - Parakeet TDT 1.1B — Quality tier — 2.4 GB / 4.0 GB — Download
- **Low disk space** (variant `issues`): `warn-soft` banner “Low disk space. 1.1 GB free on /home — Quality needs 2.4 GB.” with a “Use Standard” button. The Quality Download button is disabled.
- **Checksum error**: the card gets an `err` border and a “Failed” chip, plus “Checksum didn't match, so the file was removed. Nothing was installed.”, “Copy details” and “Retry download”.
- Footer credits (exact): `Credits: “S1-mini” by “Superwhisper” · NVIDIA Parakeet (CC-BY-4.0) · Silero VAD`.

### 4. Settings (two columns)
- **Dictation** group:
  - Shortcut: keycap button with a dashed border; click to record new keys.
  - Shortcut behaviour: segmented Hold to talk / Tap to toggle, with a note that changes.
  - Output: segmented Type it / Paste it. Notes: “Types like a keyboard. Works almost everywhere.” / “Faster for long text. Restores your clipboard after.”
- **Microphone** group: device picker (34 high) and a live level meter with a mono dB value.
- **App** group:
  - Start at login (switch).
  - Unload models when idle: stepper “10 min”, 1–120 (“Frees about 3 GB of memory”).
  - Save history (switch; note “Off — kept in memory until you quit” / “Saved on this computer, encrypted”).
  - Check for updates (switch; “Once a day. Sends only the version number.”).
- **Privacy and network** card (`ac-soft` background, `ac-line` border, shield icon):
  - “Audio never leaves this device. Speech is recognised and cleaned up on this computer, then discarded.”
  - “The network is only used to download models and, if on, to check for updates. No account, no analytics.”

### 5. Dictionary & Voice commands
- **Left: Words and corrections** table with an “Add word” button. Columns: Type chip (Correction = `ac-soft`, Word = `surf2`) · When I say · Write (mono) · delete. Example rows: voice yo, voisho → Voicio; kube control → kubectl; post gress → PostgreSQL; jason → JSON; plus Words Anaïs, Tauri, Okafor.
- **Right: Voice commands**, each row with an icon tile, the phrase in mono quotes, a description and a switch:
  - “new line” — Inserts a line break
  - “new paragraph” — Inserts a blank line
  - “scratch that” — Deletes the last phrase
  - “press enter” — Sends the Enter key
  - “stop listening” — Ends dictation and types
- Footnote: “Say “literally” first to type the words instead — “literally new line”.”

### 6. About & Debug
- Header card:
  - Logo 44, “Voicio 1.4.2”, “Up to date” chip.
  - Mono line “build 2026.10.21 · Linux x86_64 · Wayland”.
  - Buttons: “Check for updates” and primary **“Copy diagnostics”** (toast: “Diagnostics copied — no text or audio included”).
- Licences list (240 wide): Voicio MIT, NVIDIA Parakeet CC-BY-4.0, S1-mini · Superwhisper, Silero VAD MIT, ONNX Runtime MIT, Tauri Apache-2.0, cpal Apache-2.0.
- **Live log**:
  - `scrim` background, mono 11.5/19. Each line: time · level (INFO = `ac-tx`, WARN = `warn`, DEBUG = `tx3`) · module · message.
  - Device names, window titles and dictated text are replaced with redaction chips (`surf2` background, `tx3` text, e.g. “device”, “window”, “214 chars”).
  - Header: breathing dot + “Text and device names redacted” + a Pause button.

### 7. Onboarding (Voicio Onboarding.dc.html, 960×640, no sidebar)
- Header: logo + “Voicio”, centred progress dots (6px dots; the current one is a 22px pill; done = `ac`), “n / 6” in mono, and a close button.
- Footer: Back (ghost), then Skip (ghost, steps 2 and 5) and a primary Continue / Get started / Finish button.
- Steps:
  1. **Welcome** — 64 logo in a ring, “Speak. It types.” and the subtitle. Chips: Works offline · Types into any app · About 2 minutes to set up.
  2. **Choose model** — “We picked a model for you”, a Standard card with a “Recommended” chip, a Lite/Standard/Quality segmented control, a 6px progress bar with mono “Parakeet TDT 0.6B · 263 of 640 MB · 21.3 MB/s · 18 s left”, and “Keeps downloading in the background while you finish setup.”
  3. **Microphone** — picker, a 32-segment meter, a “Hearing you” chip.
  4. **Shortcut** — 112px dashed teal recorder box with large keycaps and “Recording keys… press Esc to cancel”, “No conflicts with your desktop shortcuts.”, and the Hold/Tap segmented control.
  5. **Typing test** — focused text field with a blinking caret, a success line “Typing works. 68 chars · 0.4 s”, and the Type/Paste segmented control.
  6. **Try it** — “You're ready”, keycaps, a preview of the pill on a dotted stage, and tips about “new line” / “scratch that”.

### 8. Floating pill (Voicio Pill.dc.html, live in Voicio Desktop.dc.html)
- **Capsule** 220×44, radius 999, `pill` background with blur, 1px `line2`, `shadow`.
  - Listening: teal dot with a 4px `ac-soft` halo · 20-bar waveform · mono timer. On hover the timer is replaced by a 30px cancel (×) button.
  - It is a separate always-on-top window that **never takes focus**: buttons cancel mousedown, and the window is created non-activating.
  - Draggable anywhere; it remembers its position.
- **Live-text card** above the capsule (8px gap): 400–420 wide, radius 16, same glass style. It shows the **last 3 lines** of recognised speech; the top fades out (mask). The **last ~3 unsettled words are `tx3`**. Updates `aria-live=polite`.
- **States**:
  - Listening (above).
  - **Processing**: spinner + “Cleaning up…” (then “Typing…”) + a shimmering mini waveform.
  - **Done**: check pops in + “Typed into <App>” + “48 words”. Fades after 1.2–1.5 s.
  - **Error**: capsule border turns `err`, alert icon + “Couldn't type”. Card above: “Couldn't type into <App>”, “The window blocked typed input. Your text is safe.”, a quoted text box, and the buttons **Copy** (primary) · **Retry** · **Open settings**.
  - **Speech model not installed**: download icon in `warn` + “Speech model not installed”. Card: “Download a speech model first”, “Standard is 1.7 GB and works offline once installed.”, buttons **Open Models** / Not now.

### 9. Tray menu
- Header line: status dot + “Voicio · ready, on this device” (or listening / model missing).
- **Start dictation** (Super Alt Space; label becomes “Stop and type” while listening) · Open Voicio · Settings · separator · **Quit** (Ctrl Q).
- While listening, the tray icon shows a teal dot and a mono timer.

### Desktop end-to-end flow (as in Voicio Desktop.dc.html)
1. The user focuses any text field in any app.
2. The global shortcut (Super+Alt+Space) starts listening; the pill appears bottom-centre.
3. Audio goes to the VAD, then the ASR streams partial results into the live card.
4. Stop happens on: the shortcut again, release (in hold mode), the “stop listening” command, or VAD silence (auto-stop).
5. **Processing**: ASR final → dictionary corrections → voice commands (“new line” → `\n`, “scratch that” deletes the last phrase) → S1-mini clean-up.
6. **Output** into the window that had focus when dictation started: type (simulated keystrokes) or paste (clipboard + paste key, then restore the clipboard).
7. **Done** flash, then the entry is added to History and Last dictation.
8. **Failures**:
   - Typing blocked → error card (Copy / Retry / Open settings).
   - No speech → “Didn't catch that”.
   - Model missing → “not installed” card.
9. If dictation is started from the app's own record button, the text goes to Last dictation only, with the toast “Not typed anywhere — copy it from Last dictation”.
10. Esc cancels at any time.

---

## CHROME EXTENSION (standalone)

### Architecture
- **Service worker**: commands (`Alt+Shift+V` via `chrome.commands`), context menu (“Dictate here”, editable contexts only), badge state, and routing.
- **Offscreen document**: `getUserMedia`, VAD, ASR (onnxruntime-web, WebGPU with WASM fallback) and clean-up. Streams partial and final text to the active tab. The model is downloaded once (~640 MB) to OPFS and verified by SHA-256.
- **Content script** (all frames, `document_idle`):
  - Tracks the focused editable element (input, textarea, contenteditable).
  - Injects the hover mic, tooltip and pill inside a **Shadow DOM** root.
  - Inserts text with `document.execCommand('insertText')` or `InputEvent`, and falls back to setting the value plus dispatching an `input` event.
  - **Never runs on password or credit-card fields** (`type=password`, `autocomplete=cc-*`). This is locked and cannot be changed.
- Permissions: `storage`, `contextMenus`, `offscreen`, `activeTab`, `scripting`; host permissions `<all_urls>` (optional, granted per site). No network calls except the one-time model download from a pinned URL.
- Storage: `chrome.storage.sync` for settings and site rules, `chrome.storage.session` for the current transcript.

### Toolbar icon
- Logo at 16/32/48.
- Badge dot: none = idle · teal = listening · amber = error / needs attention · grey = off or no model.

### Popup (Chrome Popup.dc.html, 360×480)
- **Header** 48 high: logo 22 + “Voicio”, a status chip on the right (“On-device” / “Listening” / “Working” / “No model”), and a gear button that opens the full settings tab.
- **Tabs** (segmented, 32 high): **Dictate · This site · Settings**.
- **Dictate tab**:
  - 68px mic button with a gradient (stop icon while listening, wand icon while processing) inside a 92px area with a radial glow. Pulsing rings while listening, a spinner while processing.
  - 40-bar waveform 26 tall.
  - Status line: Ready / Listening… + timer / Cleaning up…
  - Hint with keycaps: Alt + Shift + V.
  - **Transcript** box: “Transcript” label + mono word count. Scrolls with a hidden scrollbar and a top fade; unsettled words are dimmed. Placeholder: “Your words appear here as you speak.”
  - Actions: **Insert into page** (primary, flex 1) · **Copy** (→ “Copied”) · **Clear** (icon). All three are disabled while empty or listening.
  - Switch: **Dictate into the focused field**.
- **This site tab**:
  - Hostname card with an on/off switch and a **“Never on this site”** danger button.
  - Locked row: “Password fields — Always off. Can't be changed.”
  - Recent sites list with On / Never chips.
- **Settings tab**:
  - Shortcut keycaps (link to `chrome://extensions/shortcuts`).
  - Segmented controls: Mode (Tap to toggle / Hold to talk), Insert (Type / Paste), Auto-stop after silence (Off / 3 s / 10 s), Appearance (System / Light / Dark).
  - **Advanced settings ↗** button.
- **Footer** 32 high, always visible: lock icon + “Offline. Nothing leaves your computer.”
- **Other states** (centred empty-state layout: 72px icon circle, title 16/600, body 13, primary button, link):
  - **nomodel**: “Download the speech model” · “Voicio recognises speech inside Chrome. The model is 640 MB and works offline once saved.” · button “Download · 640 MB” · link “What gets downloaded?”
  - **nomic**: “No microphone found” · button “Check again”
  - **blocked**: “Microphone blocked” · button “Show me how” → Permission help
  - **firstrun**: 3-step intro (progress 2/3).
    1. Download the speech model (done).
    2. **Allow the microphone** (active, `ac-soft` card, “Allow microphone” button).
    3. Try it.

### In-page (Chrome In-page.dc.html; live in Voicio for Chrome.dc.html)
- **Pill** 200×40, positioned next to the focused field's bottom-right (the position can be changed in settings and by dragging):
  - Listening: dot · 14-bar waveform · timer · a 28px × button (always shown).
  - **Live card** above it, 288 wide, showing the **last 2 lines**.
- **States**:
  - Listening.
  - Processing: “Cleaning up…” / “Typing…”.
  - Done: check + “Inserted” + word count.
  - Error: amber border + “Didn't catch that”. Card with **Retry** / Dismiss. Other error copy: “Microphone blocked”, “Couldn't insert the text”.
- **Field mic**: on hover over an editable field, a 28–30px mic button appears inside its bottom-right corner (pop-in 150 ms). Its tooltip reads “Dictate  Alt+Shift+V”.
- **Context menu**: “Dictate here” with the logo and the Alt+Shift+V hint.
- **Must look right on white and dark sites**: use the glass `pill` background + `line2` border + `shadow`; follow the extension theme, not the site theme.
- **Never steals focus**: every pill button calls `preventDefault` on mousedown, and focus stays in the field.

### Options page (Chrome Options.dc.html — full tab, sidebar 224 + content max-width 720)
The sidebar shows the logo, “Voicio for Chrome”, a status line, then these sections:
- **Dictate** — the same controls as the popup's Dictate tab, larger: 88px mic, transcript, Insert / Copy / Clear, the focused-field switch.
- **General**:
  - Shortcut recorder (dashed; “Press keys… Esc cancels”).
  - Mode (Tap to toggle / Hold to talk).
  - Insert as (Typing / Paste).
  - Auto-stop after silence (Off / 3 s / 10 s).
  - Switches: Right-click “Dictate here”; Mic button in text fields.
- **Sites**:
  - Search/add input with **Allow** and **Never on this site** buttons.
  - List rows: globe · mono host · Allowed/Never chip (click to switch) · remove. Empty: “No sites match. Press Allow or Never to add it.”
  - Locked row: “Dictate into password fields — Always off.” (disabled switch + lock icon).
- **Appearance**:
  - Theme cards System / Light / Dark (selected card has a 2px `ac` border).
  - Pill position: a 2×2 picker (Above left / Above right / Below left / Below right).
- **Speech model** — status card + **Run speed test**. Facts: Model “Parakeet 0.6B”, “Stored in Chrome 640 MB · WebGPU”, speed test result.
- **Privacy** — `ac-soft` card “Audio never leaves your device”:
  - Body: “Your voice is turned into text by a model running inside Chrome on this computer. Text is typed into the page you're on. We can't see it, and neither can anyone else.”
  - Checks: No analytics or tracking · No account needed · Works with Wi-Fi off · Password fields always skipped.
  - Link: “Read the source code ↗”.
- **About** — version card “Version 1.2.0 · Manifest V3” + Up to date chip; licences; credit line “Clean-up model: “S1-mini” by “Superwhisper”. Speech: NVIDIA Parakeet (CC-BY-4.0). Voice detection: Silero VAD.”

### Permission screens (Chrome Permission.dc.html, opened as a tab)
- **Pre-prompt**:
  - Mic illustration and the heading “One more thing: your microphone”.
  - Three reassurances: processed on this computer / mic only on while dictating / turn off any time.
  - Buttons: Continue → calls `getUserMedia` (Chrome's prompt) · Not now.
- **Blocked**:
  - “Microphone blocked” chip and the heading “Let's turn the mic back on”.
  - Three numbered steps (lock icon → Microphone → Allow → come back).
  - Buttons: “Open Chrome settings” · “I've allowed it” (re-checks without reloading).
  - Right side: a screenshot placeholder for `chrome://settings/content/microphone`.

### Extension end-to-end flow (as in Voicio for Chrome.dc.html)
1. **Start**: focus a field and press Alt+Shift+V (or use the field mic, “Dictate here”, or the popup mic).
2. If the model is missing, the popup shows the nomodel state. If the mic is not granted, show the pre-prompt; if it is denied, show the blocked help.
3. **Listening**: pill and live card next to the field; the badge turns teal; the popup transcript mirrors the live text.
4. **Stop**: shortcut again / key release (hold mode) / auto-stop / ×. Esc cancels.
5. **Processing**: clean-up → insert at the caret of the original field (Typing = progressive insert; Paste = one insert), then the Done flash.
6. If “Dictate into the focused field” is off, the text stays in the popup for Insert or Copy.
7. **Site rules**: on “Never” sites, the shortcut, field mic and context menu are all inert.

---

## State model (both products)
```
rec: 'ready' | 'listening' | 'processing' | 'done' | 'error' | 'nomodel'
target: focused element / window id captured at start
partial: string[]        // live words; the last ~3 are "unsettled"
final: string            // cleaned text
secs: number             // timer
settings: { mode, insertAs, autostop, theme, pillPos, shortcut, historySaving, startAtLogin, unloadMins, updates }
history: { id, time, app, text }[]   // memory-only unless historySaving
models: { id, status: 'installed'|'downloading'|'paused'|'failed'|'missing', bytes, total, speed }[]
siteRules (ext): { host, rule: 'allow'|'never' }[]
```

## Accessibility
- WCAG AA contrast in both themes (the token values above are already checked).
- Every interactive element can be reached with the keyboard, and `:focus-visible` shows a 2px ring.
- Segmented controls use `role=radiogroup`; switches use `role=switch`; menus use `role=menu` / `menuitem`.
- The live transcript uses `aria-live=polite`; status lines use `role=status`.
- State is never shown by colour alone — there is always a text label.
- Reduced motion turns all animation off.

## Assets
- No raster images. Icons: Lucide-style paths in `voicio-kit.js`. Logo: generated from five bars (see “Logo mark”).
- Fonts: Inter and JetBrains Mono (Google Fonts / OFL).
- The permission help screen needs a real screenshot of Chrome's microphone settings.
