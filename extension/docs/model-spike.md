# In-browser model spike (E0)

Why: the design's "Parakeet 0.6B, 640 MB, WebGPU" is not reliable in a browser (the published browser ONNX export is
~1.2 GB fp16 for the encoder and its int8 export is wrong on WebGPU), and the S1-mini LLM cannot run in a browser.

Method: `extension/tools/spike/spike.mjs`, transformers.js on Node CPU, 20 LibriSpeech test-clean clips (135.6 s),
int8 (q8) weights. Browser WASM is expected to be about 2-3x slower than this; WebGPU faster. Re-measure in Chromium in E3.

| Model | Download (int8) | WER | RTF (Node CPU) |
|---|---|---|---|
| Moonshine Base (MIT) | ~63 MB | 4.64 % | 0.080 |
| Moonshine Tiny (MIT) | ~28 MB | 6.96 % | 0.054 |
| Whisper base.en | ~76 MB | 5.93 % | 0.173 |

Decision: **Moonshine Base int8** (+ Silero VAD 2.2 MB), about 66 MB total, pinned by revision and SHA-256 in
`apps/chrome/src/models.json`. UI sizes are read from that file. Moonshine Tiny is the low-end fallback.
Not yet measured: Indian-English clips (to be recorded by the owner, kept local), Chromium WASM/WebGPU timings.
