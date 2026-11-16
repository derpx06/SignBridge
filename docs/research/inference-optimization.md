# Inference optimization: best accuracy at least compute

Date 2026-11-11. Sources are linked inline; web search was shallow (snippets, not full-paper reads). Items marked **[UNVERIFIED]** could not be confirmed from a primary source. Nothing here was benchmarked on this code.

## Current state (read from repo)
- Desktop `engine/src/lib.rs`: Silero VAD (threshold 0.5, min_silence 0.8 s, 1 VAD thread), offline Parakeet TDT 110M int8 by default, provider `cpu`, `num_threads = config.threads`, int8 encoder/decoder/joiner everywhere, 0.3 s preroll.
- Extension `extension/apps/chrome/src/offscreen/asr.worker.ts`: transformers.js, `device: "wasm"`, `numThreads = 1`, arena/mem-pattern off, `graphOptimizationLevel: "basic"`, dtype from catalog (q8). Idle unload 3 min in `index.ts`.
- Android `app/.../engine/SherpaEngine.kt` (sherpa-onnx).

## Model choice (evidence)
- Open ASR Leaderboard (paper: [arXiv 2510.06961](https://arxiv.org/html/2510.06961v3)): Parakeet TDT 0.6B v2 avg WER 6.05, RTFx ~3386 at batch 128 (GPU, [model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2)). The paper's finding: TDT/CTC decoders give far better RTFx than LLM-decoder models, which win only on WER. RTFx is GPU batch, so it does NOT predict laptop-CPU or phone speed.
- Moonshine v2 streaming ([arXiv 2602.12241](https://arxiv.org/html/2602.12241)): Tiny 34M / Small 123M / Medium 245M, sliding-window encoder so latency does not grow with utterance length. Model cards: Tiny AMI WER ~19, Open-ASR mean ~12 ([HF](https://huggingface.co/moonshine-ai/moonshine-streaming-tiny)); Small AMI 12.5, Medium 10.7 (third-party card). Medium was listed at 6.66 avg WER on one ranking (secondary source).
- Not found: Whisper large-v3-turbo / distil WER+RTFx in my results (**UNVERIFIED**; read the live HF leaderboard). Parakeet v3 and 110M numbers **UNVERIFIED** here; see `engine-model-support.md`.
- Rule of thumb for budgets (my inference, not sourced): <300 MB RAM -> Moonshine Tiny / Parakeet 110M; ~600-800 MB -> Parakeet 0.6B int8 or Moonshine Small/Medium; Whisper-turbo only for languages Parakeet lacks.

## Quantization
- int8 weight quant of transducers is the standard sherpa-onnx path; accuracy cost typically small but model-specific. Moonshine Tiny: FP32 12.72 vs INT8 12.81 vs W4-A16 13.61 avg WER, from a community repo citing an unverified "Edge-ASR" paper ([felixem](https://huggingface.co/felixem/moonshine-streaming-tiny-optimized)) **[UNVERIFIED]**. Action: keep int8; do not go to 4-bit for the small models.
- Whisper on WebGPU: a transformers.js issue reports q8 encoder/decoder produced gibberish on WebGPU but worked on WASM (v3.4.0, Chrome 135; may be fixed) ([issue](https://github.com/huggingface/transformers.js/issues/1317), [dtype docs](https://huggingface.co/docs/transformers.js/en/guides/dtypes)). Transformers.js defaults: fp32 on WebGPU, q8 on WASM; per-component dtype supported (e.g. encoder q4/fp16) ([v3 blog](https://huggingface.co/blog/transformersjs-v3)). Validate WER per dtype/device pair before shipping WebGPU.
- fp16 on desktop CPU: no speedup on most x86 CPUs without AVX512-FP16 **[UNVERIFIED]**; stay int8.

## ONNX Runtime options
- Threads/spinning: [ORT threading docs](https://onnxruntime.ai/docs/performance/tune-performance/threading.html): match intra-op threads to physical cores, inter-op 1; `session.intra_op.allow_spinning` trades latency for CPU/power. The docs say default enabled; ORT 1.22 headers say disabled for client workloads ([header](https://huggingface.co/csukuangfj/ios-onnxruntime/blob/main/1.22.2/onnxruntime.xcframework/Headers/onnxruntime_session_options_config_keys.h)). Version-dependent: check what the bundled sherpa-onnx ORT does. Whether sherpa-onnx exposes the key **[UNVERIFIED]**.
- Android EPs: NNAPI needs Android 8.1+, 9+ recommended; it may silently run on CPU ([ORT NNAPI](https://onnxruntime.ai/docs/execution-providers/NNAPI-ExecutionProvider)). NNAPI is deprecated in Android 15 (my knowledge, **UNVERIFIED** here). QNN build in sherpa-onnx exists ([docs](https://k2-fsa.github.io/sherpa/onnx/qnn/build.html)) but is Qualcomm-only and needs vendor libs. No trustworthy sherpa-onnx speedup numbers found; the "2-3x NNAPI" claim came from an emulator and should be ignored. Default: CPU, int8, 2-4 big cores.
- Browser: WASM threads need cross-origin isolation (COOP/COEP); an MV3 offscreen document can get it via manifest `cross_origin_*` keys **[UNVERIFIED, test]**. WebGPU wins mainly for large encoders (Whisper-class); for a 34M model, dispatch overhead and quant bugs likely outweigh gains **[my inference]**.

## Streaming, partials, endpointing
- Moonshine v2 streaming and sherpa-onnx online zipformer/transducers keep encoder state, so partials never re-decode old audio. Offline Parakeet re-run on a growing buffer is O(n^2) in total work. Mitigation: only show partials from a small streaming model or decode fixed ~2-4 s chunks and commit on VAD endpoint (cheap, loses little accuracy at sentence boundaries).
- Sherpa-onnx streaming Parakeet/NeMo support for 0.6B **[UNVERIFIED]**.
- Endpointing: `min_silence_duration 0.8` is conservative for accuracy but adds 0.8 s latency; consider 0.5 s for short dictation with preroll kept.

## VAD
- Silero is the baseline and already used. TEN VAD claims lower latency on speech-to-silence transitions and lower compute than Silero, but numbers are self-published on TEN's own test set ([README](https://github.com/TEN-framework/ten-vad)); no independent replication found. Silero ONNX runs ~100x realtime on CPU ([Cekura](https://www.cekura.ai/blogs/what-is-silero-vad)), so VAD cost is already negligible; do not switch for compute. An energy gate in front of Silero (skip the NN on obvious silence) is a cheap battery win on mobile **[inference]**.

## Decoding / accuracy
- Beam vs greedy: for TDT/transducers `modified_beam_search` with small beam (4) is usually a small WER gain at modest cost; required for hotwords in sherpa-onnx (transducer only) **[from sherpa-onnx docs knowledge, not re-fetched]**. Hotwords/context biasing is the cheapest accuracy lever for names and jargon.
- Post-processing: punctuation/ITN models cost ms-scale on CPU relative to ASR; rule-based ITN (sherpa has text-processing FSTs) is near free **[UNVERIFIED cost numbers]**. LLM cleanup is the expensive part; keep it optional (see `cleanup-model-bakeoff.md`).

## Keep-warm vs unload, battery
- Keep the model warm only while the user is actively dictating; unload on idle (desktop worker exit already frees mmap + arenas, see `low-memory.md`). Extension 3 min idle unload is reasonable; cold load from OPFS cost **[UNVERIFIED, measure]**.
- Mobile: disable ORT spinning, cap to 2-4 threads on big cores, run ASR only between VAD start/stop, release the mic and model on screen-off. Thermal throttling data for sherpa-onnx **[UNVERIFIED]**.

## Ranked recommendations
| # | Platform | Action | Gain | Risk |
|---|---|---|---|---|
| 1 | All | Build a fixed WER+RTF harness (`engine/eval`) on your own audio, per model/quant/threads | Makes all below measurable | None |
| 2 | Desktop | Set threads = physical cores minus 1, test `num_threads` 2/4/6 in `engine/src/lib.rs` | 1.3-2x on CPU if currently mis-set | Low |
| 3 | Desktop/Android | Streaming partials via small streaming model; commit offline decode at VAD endpoint | Removes re-decode O(n^2), lower latency | Medium |
| 4 | Desktop/Android | Hotwords with `modified_beam_search` for user dictionary | Accuracy on names | Low |
| 5 | Android | Stay CPU int8, 2-4 threads; benchmark XNNPACK, skip NNAPI/QNN unless measured | Battery, avoids silent CPU fallback | Low |
| 6 | Extension | Try Moonshine v2 streaming Tiny/Small via transformers.js | Latency constant vs length, better WER than v1 | Medium (ORT-web support **[UNVERIFIED]**) |
| 7 | Extension | Test wasm numThreads 2-4 with COOP/COEP, keep q8 | 1.5-2.5x **[inference]** | Memory, headers |
| 8 | Extension | Enable `graphOptimizationLevel: "all"` and re-enable arena if RAM allows | Small speedup | Memory |
| 9 | All | Energy gate before Silero; endpoint 0.5-0.6 s | Battery, latency | Clipped words (keep preroll) |
| 10 | Extension | WebGPU only for Whisper-class models, after WER check per dtype | Large for >200M models | q8 WebGPU bug history |

## Do these first (this codebase)
1. Add RTF/WER logging to `engine/src/lib.rs` + `engine/eval` and record baselines for 110M vs 0.6B int8.
2. Sweep `config.threads` and `--cpu-mode` defaults in `engine/src/lib.rs` (line ~276).
3. Check ORT spin default in the vendored sherpa-onnx under `engine/vendor`; disable spin on battery.
4. Switch decode to `modified_beam_search` (beam 4) with hotwords from the user dictionary in `engine/src/lib.rs`; measure.
5. Lower `min_silence_duration` from 0.8 to ~0.5 in `engine/src/lib.rs` (VAD config ~line 323) and compare WER; keep PREROLL/POSTROLL.
6. In `extension/apps/chrome/src/offscreen/asr.worker.ts`, test `numThreads` 2-4 and `graphOptimizationLevel: "all"`; record RAM and latency.
7. Prototype Moonshine v2 streaming Tiny in the worker (`shared/models` catalog entry) and compare with the current model.
8. Add an energy pre-gate before Silero in `app/.../engine/AudioCapture.kt` and `SherpaEngine.kt`; set threads 2-4, spinning off.
9. Android: benchmark XNNPACK vs CPU in `SherpaEngine.kt` on the real phone; do not ship NNAPI/QNN without numbers.
10. Keep unload-on-idle (desktop worker exit, extension `IDLE_UNLOAD_MS`); add Android release on screen-off.

## Measured results (desktop engine, 2026-11-11)
Harness: `python3 engine/scripts/bench.py` (worker over the real IPC, streamed at 1x real time; 192 s of flite-synthesized `engine/eval/dictation.txt` in two voices; 513 words; 16-thread CPU). Synthetic TTS speech: WER is a regression check only (absolute 15.8% is TTS plus unnormalized numbers). CPU seconds vary about +-15% run to run.

| config | WER % | CPU s (192 s audio) | peak RSS |
|---|---|---|---|
| before: balanced (3 thr, partials x3 duty) | 15.82 | 88.9 | 226 MB |
| before: low (2 thr) / fast (4 thr) | 15.82 / 15.82 | 50.9 / 106.2 | 228 / 229 MB |
| after: balanced (2 thr) | 15.82 | 61.0 | 228 MB |
| after: low (1 thr) | 15.82 | 35.3 | 229 MB |
| live_text off (`Start.partials=false`), 3 thr | 15.82 | 9.9 | 222 MB |

Findings
- Partial decodes (the pill's live words) are about 80-90% of ASR CPU; the final transcript does not depend on them. New `partials` flag on `Start`, sent as `config.live_text`.
- ORT spin-waiting (UNVERIFIED claim, now verified): enabling it costs 1105 CPU s vs 89, i.e. 12x. `ort_arena.rs` already disables it; keep.
- Threads: CPU scales about linearly (1/2/3/4 thr: 56/78/90/126 s, WER identical). Balanced now 2, low 1. A 11 s final decode is ~0.4 s at 1 thread on this machine.
- Idle: agent 0.05% CPU, loaded worker 0.05% (20 s samples, /proc). Silence/steady noise (room, fan) costs 0.5 CPU s per audio minute with zero decodes: the VAD already gates ASR, so an energy gate is not worth it.
- Pill: 0.54 ms (1x) / 2.0 ms (2x) per painted frame at 60 fps (about 12% of a core at 2x while listening); it already stops drawing when settled. Not changed: a 30 fps cap is a design call.

Rejected
- VAD min_silence 0.5 s: WER 15.82 -> 16.02 and CPU +16% (more, shorter utterances, more partial decodes).
- `modified_beam_search`: WER 15.04 (-4 errors) but +27% CPU (it also applies to partials; no per-call switch in the Rust API). Hotwords unmeasured.
- Partial cadence factor 6 at the same threads: -16% CPU, half the preview refresh; the `low` mode already covers it.
- Partial window cap / not re-decoding committed audio: offline model has no state to reuse; capping the window shows only the tail of the utterance and seams words. Not built.
- Keep-warm vs unload: load costs 0.7 CPU s and ~0.05% idle, so a 5 min keep-warm costs less than one reload; unchanged.
