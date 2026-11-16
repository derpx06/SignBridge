# Cactus Whistle (not shipped yet)
Apache-2.0, 16.9 MB, 7 languages. Needs Cactus's own WASM engine (`needle.js`, `needle.wasm`, from Cactus-Compute/needle3) and a worker (`whistle.worker.js`).
Not enabled because it has not been run or memory-tested here. To enable:
1. Copy these files to `apps/chrome/public/needle/`.
2. Add `models-entry.json` to `src/models.json` (set `wer`/`rtf` from the test).
3. `VOICIO_E2E=1 pnpm build && node e2e/models.mjs whistle`; keep it only if extension + model stay under 1 GB and the text is right.
The offscreen page already starts `needle/whistle.worker.js` for any model with `"engine": "needle"`.
