# Prompt for Claude Code — build Voicio (desktop app + Chrome extension)

Paste everything below into Claude Code, run from an empty folder that contains this `design_handoff_voicio/` folder.

---

You are building **Voicio**: private, offline voice dictation. The user presses a shortcut, speaks, and cleaned-up text is typed into whatever field has focus. Nothing leaves the computer.

There are two products. Build both, end to end, in one monorepo:
1. **Desktop app** (Linux first, then Windows and macOS) — Tauri 2 + Rust + React + TypeScript.
2. **Voicio for Chrome** — a **standalone** Manifest V3 extension that does NOT need the desktop app. Speech runs inside Chrome with onnxruntime-web.

## Source of truth
- Read `design_handoff_voicio/README.md` fully before writing code. It has every screen, state, token, copy string and flow.
- Open the HTML references in `design_handoff_voicio/designs/` in Chrome to see look and behaviour:
  - `Voicio Desktop.dc.html` — the desktop flow working end to end
  - `Voicio for Chrome.dc.html` — the extension flow working end to end
  - `Voicio.dc.html` — every screen in light and dark, plus tokens
  - `voicio-kit.js` — icon paths, waveform, level meter, logo mark (port these to React)
- These HTML files are design references, not code to ship. Rebuild them as real components. Match the colours, spacing, type, radii, copy and motion exactly.

## Monorepo layout
```
voicio/
  packages/
    ui/            Shared React design system: tokens.css (light + dark CSS variables), Button, IconButton,
                   Segmented, Switch, Keycap, Chip, Progress, Card, ListRow, Toast, Tooltip, Menu, Input, Select,
                   EmptyState, Waveform, LevelMeter, Logo, Icon (Lucide-style paths, 1.75 stroke)
    core/          Shared TS: text pipeline (dictionary corrections, voice commands, filler removal),
                   settings schema (zod), types for state machine
  apps/
    desktop/       Tauri 2 app (src-tauri in Rust, src in React)
    extension/     Chrome MV3 (Vite + @crxjs/vite-plugin + React)
```
Use pnpm workspaces, TypeScript strict, ESLint + Prettier, Vitest for TS, `cargo test` for Rust.

## Shared design system (packages/ui) — build this first
- Implement the tokens from the README's Design tokens section as CSS variables on `[data-theme="dark"]` and `[data-theme="light"]`. Also follow the system theme via `prefers-color-scheme`.
- Inter + JetBrains Mono. Base size 14px, 8px grid, radii 8/10/12/16/999.
- Every component needs real hover, `:focus-visible` (2px `--focus` ring, 2px offset), disabled (45% opacity), and loading states, as described in the README.
- `prefers-reduced-motion: reduce` turns off all animation.
- Build a `/storybook` (Ladle is fine) page that reproduces `Voicio Components.dc.html` in both themes.

## Shared state machine (packages/core)
```
ready → listening → processing → done → ready
           ↘ error (no speech | output blocked | mic blocked)
ready → nomodel (if the speech model is missing)
Esc / ×  → ready from listening, error or nomodel
```
- Text pipeline, in order: ASR final → dictionary corrections → voice commands → filler removal → clean-up model.
  - Voice commands: “new line”, “new paragraph”, “scratch that”, “press enter”, “stop listening”, plus the “literally …” escape.
- Unit-test the pipeline with the examples from the README.

## Desktop app (apps/desktop)
**Rust side (src-tauri):**
- Global shortcut: default Super+Alt+Space, rebindable. Support hold-to-talk and tap-to-toggle. Use `tauri-plugin-global-shortcut`.
- Audio: `cpal`, 16 kHz mono. Expose a level meter (RMS → dB) to the UI at 30 fps.
- VAD: Silero VAD (ONNX via the `ort` crate). It drives auto-stop.
- ASR: NVIDIA Parakeet TDT 0.6B in ONNX (Quality tier: 1.1B), via `ort`. Stream partial hypotheses to the UI every ~150 ms, marking the last unsettled words.
- Clean-up: “S1-mini” by “Superwhisper” (GGUF) via the `llama-cpp-2` crate. Run it after ASR. Time out after 1.5 s and fall back to the pipeline-only text.
- **Model manager**:
  - Download with resume, pause and cancel.
  - Report speed and time left.
  - Verify SHA-256; on a checksum mismatch, delete the file and show the error state.
  - Check free disk space before downloading and show the low-disk state if there isn't enough.
  - Unload models after N idle minutes (setting, 1–120).
- **Output**:
  - Type mode: Linux Wayland `wtype` or `ydotool`, X11 `xdotool` / XTest; Windows `SendInput`; macOS `CGEventKeyboardSetUnicodeString`.
  - Paste mode: save the clipboard, paste, then restore it.
  - Capture the focused window *when dictation starts*, and type into that window.
- **Floating pill**: a second Tauri window, 420×200, transparent, no decorations, always on top, skip taskbar, **`focusable: false`** so it never takes focus. Bottom-centre by default, draggable, position saved.
- **Tray**: status line + Start dictation / Open Voicio / Settings / Quit. The tray icon changes with the state (teal dot while listening, plus a timer where the OS supports it).
- Settings and history in `tauri-plugin-store`. History is in memory unless “Save history” is on; then it is encrypted at rest.
- Start at login via `tauri-plugin-autostart`.
- Update check (on/off) sends only the version number. No other network calls.
- **Live log** for About & Debug: a ring buffer of tracing events, with device names, window titles and dictated text redacted. “Copy diagnostics” copies only versions, the OS, models and settings.

**React side (src):** build every screen in the README exactly:
- The main window (960×640, sidebar with Home / History / Models / Dictionary / Settings, plus About & Debug). Number keys 1–5 switch sections.
- Onboarding (6 steps).
- The pill window with every state: listening + 3-line live card, processing, done, error card with Copy / Retry / Open settings, and “Speech model not installed”.
- The empty, low-disk and checksum-error states.

## Chrome extension (apps/extension) — standalone
- **manifest.json** (MV3):
  - Permissions: `storage`, `contextMenus`, `offscreen`, `activeTab`, `scripting`.
  - Optional host permission `<all_urls>`.
  - Command `_execute_action`-style toggle `toggle-dictation` with suggested key Alt+Shift+V.
- **Service worker**: commands, the “Dictate here” context menu (editable contexts only), badge dot colours (teal listening / amber error / grey off or no model), and message routing.
- **Offscreen document** (reason `USER_MEDIA`):
  - Runs `getUserMedia` and an AudioWorklet that resamples to 16 kHz.
  - Silero VAD + Parakeet (or another small streaming ASR model) via **onnxruntime-web** (WebGPU, falling back to WASM SIMD).
  - Optional clean-up model in WebGPU.
  - The model is downloaded once to OPFS with a SHA-256 check, and the download can be resumed.
- **Content script**:
  - Tracks the focused editable (input, textarea, contenteditable, including iframes).
  - Mounts a Shadow DOM root for the hover mic + tooltip, the pill (200×40) + 2-line live card, and the Done / Error states. All pill buttons call `preventDefault` on mousedown so **focus never leaves the field**.
  - Inserts text at the caret with `execCommand('insertText')`, falling back to `InputEvent`, then to setting the value plus dispatching `input`/`change`.
  - **Never** activates on `type=password` or `autocomplete^=cc-` fields.
  - Respects site rules: on “Never” sites the shortcut, mic and menu do nothing.
- **Popup (360×480)**:
  - Tabs **Dictate / This site / Settings** and a footer “Offline. Nothing leaves your computer.”
  - States: main (ready, listening, processing), nomodel, nomic, blocked, firstrun.
  - The gear opens the options page.
- **Options page** (full tab, sidebar + 720px content): Dictate, General, Sites, Appearance, Speech model, Privacy, About. It must contain **every control the popup has**, plus the advanced ones.
- **Permission pages**: the pre-prompt before calling `getUserMedia`, and the “Permission blocked” help page with steps and a re-check that doesn't need a reload.
- Settings are stored in `chrome.storage.sync`; the transcript in `chrome.storage.session`.

## Accessibility (both)
- WCAG AA contrast.
- Every control reachable by keyboard, with proper roles (`radiogroup`, `switch`, `menu`).
- `aria-live=polite` on live transcripts.
- Status is never shown by colour alone.
- Esc cancels everywhere.

## Licences and credits
Show these exactly in Models (desktop) and About (both):
`Credits: “S1-mini” by “Superwhisper” · NVIDIA Parakeet (CC-BY-4.0) · Silero VAD`
Add a THIRD_PARTY_NOTICES file.

## Order of work
1. Set up the monorepo, then `packages/ui` with tokens and all components plus Storybook matching the component sheet, in light and dark.
2. `packages/core` pipeline + state machine + tests.
3. Desktop: model manager → audio + VAD → ASR streaming → pill window → output typing → main window screens → tray → onboarding → settings and history persistence → About & Debug.
4. Extension: manifest + service worker → offscreen ASR → content-script pill and insertion → popup → options → permission pages.
5. End-to-end tests:
   - Desktop: a scripted WAV fed in place of the mic, checking that the text lands in a test window.
   - Extension: Playwright with `--use-fake-device-for-media-stream` and a WAV, dictating into a test page's textarea, contenteditable and an iframe, and checking that password fields are skipped.
6. Packaging:
   - Desktop: AppImage + .deb (Linux), MSI (Windows), DMG (macOS).
   - Extension: a zip ready for the Chrome Web Store.

## Definition of done
- Both apps work fully offline once the models are downloaded. Check with the network disabled.
- Every screen and state in `Voicio.dc.html` exists in the real apps, in light and dark, and matches the design.
- The two end-to-end flows in the README pass their automated tests.
- No analytics and no network calls besides model downloads and the optional update check.

Work step by step. After each step, run the tests and briefly report what is done and what is next.
