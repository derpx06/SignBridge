# Model catalogue (browser extension)

Rules from the owner: very small to medium models only, and the extension plus the model must stay under 1 GB of RAM.

## Measured in real Chromium (e2e/models.mjs, 8 s clip, fake mic)
Memory = resident memory of every Chromium process, minus the same browser idle with the extension pages open. Peak while loading and decoding.

| Model | Download | Word errors (20 LibriSpeech clips) | Extension + model RAM | Fits 1 GB |
|---|---|---|---|---|
| **Moonshine Tiny** (MIT) | 32 MB | 7.0 % | ~420 MB | yes |
| **Moonshine Base** (MIT, default) | 67 MB | 4.6 % | ~470 MB (up to ~700 MB on a cold load) | yes |
| Whisper Tiny (English) | 45 MB | 6.7 % | ~1.36 GB | no |
| Whisper Base (English) | 81 MB | 5.9 % | ~1.6 GB | no |
| Distil-Whisper Small (English) | 176 MB | 4.4 % | ~1.7 GB | no |
| Whisper Small (English) | 249 MB | 3.9 % | 1.5 GB in Node alone | no |
| Wav2Vec2 Base 960h (CTC) | 95 MB | 19.1 % (int8 hurts it) | not run | no (accuracy) |

Whisper always processes a padded 30 s window and keeps large decoder state, so its WebAssembly memory exceeds the budget even for the tiny model. Moonshine takes the real audio length, so it fits.

## Not offered (and why)
- Parakeet 0.6B (2.4 GB fp32, ~611 MB int8; int8 gives wrong output on WebGPU), Whisper large-v3-turbo (>600 MB): too big.
- Moonshine streaming v2 (WER tiny 4.49/12.09, small 2.49/6.78 on LibriSpeech clean/other): only safetensors officially; transformers.js has no `moonshine_streaming` type; community int8 ONNX exports exist (tiny ~80 MB, small ~226 MB) but need a custom onnxruntime-web pipeline and a memory check first.
- sherpa-onnx WebAssembly streaming Zipformer (English 20M, ~57 MB): true streaming, needs its own wasm runtime built with emscripten. No WER found in the sources.

## Streaming
Words appear while you speak for every model: the open speech segment is re-decoded every 600 ms and segments are finalised at pauses.

## Next candidates (if more choice is wanted)
1. Moonshine streaming tiny/small via custom onnxruntime-web code, gated by the same memory test.
2. WebGPU path for Whisper small/distil (GPU memory is not counted against the browser process), only where `navigator.gpu` exists.

Re-run the numbers: `VOICIO_E2E=1 pnpm build && node e2e/models.mjs` (writes `docs/model-measurements.json`).

## Cactus Whistle (parked, not shipped)
Apache-2.0, 16.9 MB single `.cact` file, English/German/French/Spanish/Italian/Dutch/Polish, own 930 KB WASM engine with a streaming API. Cactus reports it beats Whisper base and Moonshine tiny v2 on LibriSpeech; no numbers were readable. It has not been run or memory-tested here, so it is not in the catalogue. Files and enable steps: `experimental/whistle/README.md`.

## Pill and permissions
The dictation bar docks at the bottom centre (draggable). It no longer needs a focused field to appear. "Works on every website" (Welcome page and Settings → General) grants one optional permission and registers the content script, so the bar does not depend on the `activeTab` grant from the shortcut.
