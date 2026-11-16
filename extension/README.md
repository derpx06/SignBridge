# Voicio for Chrome

Standalone Manifest V3 extension: press Alt+Shift+V in a text field, speak, and the text is typed in. Speech recognition
(Moonshine Base, int8) runs inside Chrome with onnxruntime-web. No Voicio server and no desktop app; cloud speech is an optional bring-your-own-key extra.

```
packages/core    text pipeline, state machine, settings, message types (tested)
packages/ui      design tokens, components, icons (from design/voicio-handoff)
apps/chrome      the extension (background, offscreen engine, content script, popup, options, permission page)
tools/spike      model benchmark used to choose the model (docs/model-spike.md)
```

```bash
pnpm install
pnpm test && pnpm typecheck
pnpm --filter @voicio/chrome build        # dist/ -> chrome://extensions -> Load unpacked
pnpm --filter @voicio/chrome zip          # voicio-chrome-<version>.zip for the Web Store
VOICIO_E2E=1 pnpm --filter @voicio/chrome build && pnpm --filter @voicio/chrome e2e   # real Chromium, fake mic, needs network once
```

Notes: the e2e build adds `host_permissions` so no prompts appear; never publish it. `models.json` pins the model by revision and SHA-256.

## Troubleshooting the microphone
Voicio tells you what is wrong and gives a button to fix it (on the pill, in the popup, and in Settings -> Dictate -> Microphone):
- **Microphone blocked**: Chrome blocked it for Voicio (often after the prompt was closed a few times). "Fix it" opens a page that jumps to Voicio's own Chrome setting and continues by itself once you choose Allow.
- **No sound from your microphone**: the input is muted or is the wrong one. Pick another in Settings -> Dictate -> Microphone and press Test microphone.
- **Microphone is busy / disconnected / no microphone found**: close the other app, reconnect the device, or choose another input.
Automatic gain control is switched off on purpose (on Linux Chrome can drive the system input volume down with it); quiet microphones are lifted to a usable level before recognition.
