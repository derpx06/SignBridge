# Voicio model catalogue (122 entries)
Generated 2026-11-11 from `model-library-euro-specialist.json`, `model-library-indic-asian.json` (earlier research, re-checked here) and 50 new entries. Machine-readable: `model-catalogue-100.json` (same field names on every entry).
Counts: 122 entries = 85 ASR + 2 VAD (not ASR) + 35 LLM/formatting (26 GGUF/ONNX LLMs + 9 punctuation/grammar models). Verified: 30, unverified: 92. Ship blockers or caveats flagged: 41.
**Verified** means all three held on 2026-11-11: the download URL resolved (HTTP), the size came from the source file listing, and the quoted metric number appears on the cited source page. Everything else is `verified:false`. Unknown numbers are `null`, never guessed. RAM figures are formula estimates (see `ram_method`), not measurements, except S1-mini (measured in `cleanup-model-bakeoff.md`). Metrics are as published for the FP32 upstream model unless stated; quantised WER is rarely published. Most sherpa-onnx repack pages carry no WER at all, so those are unverified by design.
## Top picks
### STT by budget tier
| Tier | Pick | Size / RAM (MB) | Why |
|---|---|---|---|
| Tiny / efficient (English) | `moonshine-tiny-en` (sherpa) or `moonshine-tiny-webgpu-int8` (Chrome) | 30 / ~45 | 27M params, MIT, variable-length input; WER unverified here |
| Tiny / efficient (English, wired) | `parakeet-tdt-ctc-110m-int8` | 108 / 162 | already in `catalog_data.rs`; punctuation+caps |
| Tiny / efficient (multilingual) | `fastconformer-multi-eu-20k-int8` (10 EU), `canary-180m-flash-int8` (en es de fr), `whisper-base-webgpu-q` (Chrome, 99 langs, LS clean 5.01) | 77-154 / 160-250 | smallest broad-coverage options |
| Balanced (English) | `parakeet-tdt-0.6b-v2-int8` | 482 / 724 | Open ASR avg 6.05, RTFx 3380 (GPU batch), CC-BY-4.0; wired |
| Balanced (multilingual) | `parakeet-tdt-0.6b-v3-int8` | 487 / 731 | 25 EU languages with auto language ID; wired |
| Highest accuracy | `parakeet-tdt-0.6b-v2-int8` (en), `nemotron-3.5-asr-streaming-0.6b-int8` (35 locales, streaming), `qwen3-asr-0.6b-int8` (30+ langs), `fireredasr2-aed-int8` (zh), `whisper-medium-sherpa-int8` (LS clean 2.9) | 470-946 / 700-1650 | check RAM on phones |
| Streaming (live words in pill) | `nemotron-speech-streaming-en-0.6b-int8`, `nemo-streaming-fastconformer-en-80ms-int8`, `streaming-zipformer-en-20m` | 106-469 | |

### STT by language group
| Group | Tiny | Balanced | Best accuracy |
|---|---|---|---|
| English | `moonshine-tiny-en` | `parakeet-tdt-ctc-110m-int8` | `parakeet-tdt-0.6b-v2-int8` |
| European | `fastconformer-multi-eu-20k-int8`, `vosk-small-ru-streaming-int8` (24 MB) | `parakeet-tdt-0.6b-v3-int8`, `gigaam-v3-ru-transducer-punct`, `stt-de-fastconformer-hybrid-int8` | `parakeet-tdt-0.6b-v3-int8`, `nemotron-3.5-asr-streaming-0.6b-int8` |
| Indic | `vosk-bn-streaming-zipformer` (Bengali, 87 MB), `dolphin-base-ctc-int8` | `indicconformer-{hi,ta,te,ml,pa}-int8` (138-198 MB, MIT repacks) | `indicconformer-600m-22lang-int8` (22 langs), `omnilingual-ctc-1b-v2-int8` |
| East Asian | `streaming-zipformer-small-ctc-zh-int8` (26 MB) | `sensevoice-small-int8-2025-09-09` (zh en ja ko yue), `paraformer-trilingual-zh-yue-en-int8` | `fireredasr2-aed-int8`, `parakeet-tdt-ctc-0.6b-ja-int8` |
| SE Asia / Middle East | `zipformer-vi-int8` (60 MB) | `shenava-koochik-fa-int8`, `whisper-turkish-small-int8`, `zipformer-multi-ar-en-id-ja-ru-th-vi-zh` | `whisper-large-v3-turbo-arabic-dialectal` (2 GB RAM) |
| Specialist | medical: `medasr-ctc-en-int8` (HAI-DEF terms unverified) | code-switching zh/en: `streaming-paraformer-zh-en-int8`, `sense-voice-int8`; Hinglish: `whisper-hindi2hinglish-apex` (needs conversion, licence check) | long tail (1600+ langs): `omnilingual-ctc-300m-v2-int8`; noisy/long-form: `distil-large-v3.5` (not sherpa-ready) |
| Chrome (transformers.js) | `moonshine-tiny-webgpu-int8`, `whisper-tiny-en-webgpu-q` | `moonshine-base-webgpu-int8`, `whisper-base-webgpu-q` | `whisper-large-v3-turbo-webgpu-q4` (WebGPU, 759 MB) |

### Text cleanup / formatting
| Role | Pick | Size / RAM (MB) | Note |
|---|---|---|---|
| **Recommended default LLM** | `s1-mini` (Qwen3-0.6B fine-tune, Q4_K_M) | 484 (356 repacked) / 428 measured | only model beating rules in our 109-case bake-off: final WER 4.9% vs rules 14.2%; already wired |
| Zero-LLM low-end formatter | `punct-cap-seg-47lang-onnx` (Apache-2.0, ONNX) | 233 / ~406 est | punctuation + casing + sentence split in 47 languages; run before or instead of the LLM on weak devices |
| Tiny LLM | `lfm2-350m-q4km`, `granite-4.0-350m-q4km` | 229-237 | in our bake-off sub-350M prompted models lost to rules; need fine-tuning. LFM2 has a revenue-threshold licence |
| Balanced LLM | `qwen2.5-1.5b-instruct-q4km`, `lfm2-700m-q4km`, `gemma-3-1b-it-q4km`, `llama-3.2-1b-instruct-q4km` | 469-1117 | candidates for `make it formal` / email / list; not yet benchmarked on `engine/eval/fluency.tsv` |
| Highest-quality LLM (over 1 GB RAM) | `smollm3-3b-q4km` (Apache-2.0, IFEval 76.7), `qwen3-1.7b-q8` (Apache-2.0) | 1.9 GB | exceeds the 2 GB RAM goal on small phones |
| Chrome | `qwen2.5-0.5b-instruct-onnx-q4f16`, `t5-efficient-mini-grammar-correction-onnx` (20 MB + 36 MB) | 483 / 20+ | |

Next step (not done here): run the balanced-tier GGUFs through `engine/eval/fluency.tsv`; published IFEval is not a cleanup metric.

### Ship blockers (licence)
Non-commercial or unusable: `qwen2.5-3b-instruct-q4km` (Qwen Research), `grammar-synthesis-small` (cc-by-nc-sa vs apache tags, ambiguous). Conditional: Gemma (Gemma ToU flow-down), Llama 3.2 (community licence, attribution), LFM2 (free under USD 10M revenue), Falcon3 (tag empty), Kroko (no licence on repo), TeleSpeech, CT-Transformer punct (FunASR licence), MedASR (HAI-DEF), SenseVoice (FunASR licence). Gated upstream (use a mirror but keep the licence text): Llama, Gemma, AI4Bharat IndicConformer. Too large for budget: `phi-3.5-mini`, `fullstop-punct-multilang-large` (2.2 GB fp32), `cohere-transcribe-14lang-int8`.

## Full table
Columns: id, kind (A = ASR, V = VAD, L = LLM/formatter), languages, size MB, RAM MB (est.), metric, licence, blocker flag (B), platforms (D desktop, A android, C chrome), verified, wired.

| id | k | languages | MB | RAM | metric (as published) | licence | B | plat | V | W |
|---|---|---|---|---|---|---|---|---|---|---|
| whisper-tiny-en-sherpa-int8 | A | en | 103.6 | 285 | LibriSpeech clean 8.44 / other 14.86 WER (openai card, FP32 PyTorch) | apache-2.0 |  | DA | yes |  |
| whisper-base-sherpa-int8 | A | 99 languages | 160.6 | 359 | LibriSpeech clean 5.01 / other 12.85 WER (openai card, FP32 PyTorch) | apache-2.0 |  | DA | yes |  |
| whisper-medium-sherpa-int8 | A | 99 languages | 946.1 | 1380 | LibriSpeech clean 2.9 / other 5.9 WER (openai card, FP32 PyTorch) | apache-2.0 |  | D | yes |  |
| streaming-zipformer-en-2023-06-26-int8 | A | en | 72.7 | 245 | null | apache-2.0 |  | DA | no |  |
| zipformer-en-libriheavy-small-punct-case-int8 | A | en | 69.8 | 241 | null | null | B | DA | no |  |
| zipformer-small-en-2023-06-26-int8 | A | en | 27.6 | 186 | null | apache-2.0 |  | DA | no |  |
| streaming-zipformer-fr-kroko | A | fr | 71.1 | 242 | null | null | B | DA | no |  |
| streaming-zipformer-es-kroko | A | es | 155.8 | 353 | null | null | B | DA | no |  |
| streaming-paraformer-zh-en-int8 | A | zh, en | 237.2 | 458 | null | apache-2.0 |  | DA | no |  |
| paraformer-trilingual-zh-yue-en-int8 | A | zh, yue, en | 244.8 | 468 | null | null | B | DA | no |  |
| paraformer-en-2024-03-09-int8 | A | en | 229.8 | 449 | null | null | B | D | no |  |
| telespeech-ctc-zh-dialects-int8 | A | zh (Mandarin and dialects) | 341.1 | 593 | null | null | B | DA | no |  |
| streaming-zipformer-small-ctc-zh-int8 | A | zh | 26.4 | 184 | null | null | B | DA | no |  |
| zipformer-ru-int8-2025-04-20 | A | ru | 73.2 | 245 | null | null | B | DA | no |  |
| nemo-streaming-fastconformer-en-80ms-int8 | A | en | 136.9 | 328 | null | null | B | D | no |  |
| whisper-tiny-en-webgpu-q | A | en | 40.9 | 203 | LibriSpeech clean 8.44 / other 14.86 WER (upstream openai/whisper-tin… | apache-2.0 |  | C | yes |  |
| whisper-base-webgpu-q | A | 99 languages | 76.9 | 250 | LibriSpeech clean 5.01 / other 12.85 WER (upstream openai/whisper-bas… | apache-2.0 |  | C | yes |  |
| whisper-small-webgpu-q | A | 99 languages | 249.1 | 474 | null | apache-2.0 |  | C | no |  |
| whisper-large-v3-turbo-webgpu-q4 | A | 99 languages | 759.1 | 1137 | null | mit |  | C | no |  |
| moonshine-tiny-webgpu-int8 | A | en | 28.1 | 187 | null | mit |  | C | no |  |
| moonshine-base-webgpu-int8 | A | en | 62.9 | 232 | null | mit |  | C | no |  |
| qwen2.5-1.5b-instruct-q4km | L | multilingual (29+) | 1117.3 | 1379 | null | apache-2.0 |  | DA | no |  |
| qwen3-1.7b-q8 | L | multilingual (100+) | 1834.4 | 2168 | IFEval 73.98 (LFM2 card comparison table) | other |  | DA | yes |  |
| qwen2.5-3b-instruct-q4km | L | multilingual | 1929.9 | 2273 | quality: strongest Qwen2.5 under 4B | other | B | DA | no |  |
| gemma-3-1b-it-q4km | L | multilingual (140) | 806.1 | 1037 | null | gemma | B | DA | no |  |
| llama-3.2-1b-instruct-q4km | L | en, de, fr, it, pt, hi, es,… | 807.7 | 1038 | IFEval 52.39 (LFM2 card comparison table) | other | B | DA | yes |  |
| llama-3.2-3b-instruct-q4km | L | en, de, fr, it, pt, hi, es,… | 2019.4 | 2371 | null | llama3.2 | B | DA | no |  |
| smollm2-135m-instruct-q4km | L | en | 105.5 | 266 | IFEval 29.9 (SmolLM2 card) | apache-2.0 |  | DA | yes |  |
| smollm2-1.7b-instruct-q4km | L | en | 1055.6 | 1311 | IFEval 56.7 (SmolLM2 card) | apache-2.0 |  | DA | yes |  |
| smollm3-3b-q4km | L | en, fr, es, de, it, pt (+ z… | 1915.3 | 2257 | IFEval 76.7 (SmolLM3 card, thinking-off table) | apache-2.0 |  | DA | yes |  |
| lfm2-350m-q4km | L | en, ar, zh, fr, de, ja, ko,… | 229.3 | 402 | IFEval 65.12 (LFM2 card) | other | B | DA | yes |  |
| lfm2-700m-q4km | L | en, ar, zh, fr, de, ja, ko,… | 468.6 | 665 | IFEval 72.23 (LFM2 card) | other | B | DA | yes |  |
| lfm2-1.2b-q4km | L | en, ar, zh, fr, de, ja, ko,… | 730.9 | 954 | IFEval 74.89 (LFM2 card) | other | B | DA | yes |  |
| granite-4.0-350m-q4km | L | en, de, es, fr, ja, pt, ar,… | 237.0 | 411 | null | apache-2.0 |  | DA | no |  |
| granite-4.0-h-1b-q4km | L | en, de, es, fr, ja, pt, ar,… | 901.2 | 1141 | null | apache-2.0 |  | DA | no |  |
| gemma-2-2b-it-q4km | L | en | 1708.6 | 2029 | null | gemma | B | DA | no |  |
| tinyllama-1.1b-chat-q4km | L | en | 668.8 | 886 | null | apache-2.0 |  | DA | no |  |
| h2o-danube3-500m-chat-q4km | L | en | 317.9 | 500 | Open LLM Leaderboard v1 avg 40.71; MT-Bench 3.28 (card) | apache-2.0 |  | DA | yes |  |
| falcon3-1b-instruct-q4km | L | en, fr, es, pt | 1057.0 | 1313 | null | other | B | DA | no |  |
| phi-3.5-mini-instruct-q4km | L | multilingual (mid-quality) | 2393.2 | 2783 | multilingual MMLU 55.4 (card) | mit | B | DA | yes |  |
| qwen2.5-0.5b-instruct-onnx-q4f16 | L | multilingual | 483.0 | 681 | null | apache-2.0 |  | C | no |  |
| sherpa-punct-ct-transformer-zh-en | L | zh, en | 294.4 | 474 | null | null | B | DA | no |  |
| punct-cap-seg-47lang-onnx | L | 47 languages incl. hi, ar, … | 233.0 | 406 | English test punct F1: '.' 88.28, ',' 67.93, '?' 78.27; micro avg 97.… | apache-2.0 |  | DAC | yes |  |
| punct-cap-seg-en-onnx | L | en | 209.5 | 380 | null | apache-2.0 |  | DAC | no |  |
| fullstop-punct-multilang-large-onnx | L | en, de, fr, it (+nl, es, et… | 2235.4 | 2609 | macro F1 EN 0.775 / DE 0.814 / FR 0.782 / IT 0.762 (card) | mit | B | D | yes |  |
| bert-restore-punctuation | L | en | 435.7 | 629 | accuracy 91%, overall F1 90% on 45,990 samples (card) | mit |  | D | yes |  |
| kredor-punctuate-all | L | 12 languages (en de fr es i… | 1109.9 | 1371 | weighted F1 0.98 overall; ',' 0.86, '.' 0.95, '?' 0.86 (card) | mit |  | D | yes |  |
| t5-efficient-mini-grammar-correction-onnx | L | en | 20.3 | 172 | null | mit |  | DAC | no |  |
| gec-t5-small | L | en | 242.1 | 416 | CoNLL-14 F0.5 60.70 (paper figure quoted on card) | apache-2.0 |  | D | yes |  |
| grammar-synthesis-small | L | en | 308.2 | 489 | null | cc-by-nc-sa-4.0/apache-2.0 | B | DA | no |  |
| parakeet-tdt-0.6b-v2-int8 | A | en | 482.4 | 724 | Open ASR avg 6.05 (8 sets: AMI 11.16, Earnings22 11.15, Gigaspeech 9.… | CC-BY-4.0 |  | DA | yes | yes |
| parakeet-tdt-0.6b-v3-int8 | A | 25 European: bg cs da de el… | 487.2 | 731 | English Open ASR avg 6.34 (LS clean 1.93 / other 3.59); multilingual … | CC-BY-4.0 |  | DA | yes | yes |
| parakeet-tdt-ctc-110m-int8 | A | en | 108.0 | 162 | Open ASR sets (card): AMI 15.88, Earnings22 12.42, Giga 10.52, LS cle… | CC-BY-4.0 |  | DA | yes | yes |
| canary-180m-flash-int8 | A | en es de fr (+ translation … | 153.7 | 231 | English Open ASR sets (card): AMI 14.86, Giga 10.51, LS clean 1.87, o… | CC-BY-4.0 |  | DA | yes |  |
| nemotron-speech-streaming-en-0.6b-int8 | A | en | 469.0 | 703 | Open ASR avg 6.93 at 1.12s chunk, 7.07 at 0.56s (card); smaller chunk… | NVIDIA Open Model License (… |  | DA | yes |  |
| nemotron-3.5-asr-streaming-0.6b-160ms-int8 | A | ~40 language-locales per ca… | 475.3 | 713 | FLEURS avg WER (card table, first block): 10.38 at 80ms to 8.84 at 11… | OpenMDW-1.1 |  | DA | yes |  |
| nemo-streaming-fastconformer-en-480ms-int8 | A | en | 105.9 | 159 | null | unverified | B | DA | no |  |
| kroko-streaming-zipformer | A | en de fr es (separate files… | 57.6 | 86 | null | Community models CC-BY-SA; … |  | DA | no |  |
| streaming-zipformer-en-20m | A | en | 127.9 | 192 | null | unverified | B | DA | no |  |
| moonshine-tiny-en | A | en | 29.9 | 45 | null | MIT (English v1); verify fo… |  | DA | no |  |
| moonshine-base-en | A | en | 111.3 | 167 | null | MIT |  | DA | no |  |
| moonshine-base-es | A | es | 50.8 | 76 | null | Moonshine AI Community Lice… |  | DA | no |  |
| whisper-tiny | A | 99 languages | 116.2 | 174 | null | Apache-2.0 |  | DA | no |  |
| whisper-small | A | 99 languages | 639.4 | 959 | null | Apache-2.0 |  | DA | no | yes |
| whisper-turbo | A | 99 languages | 563.8 | 846 | Short-form OOD WER 7.30, long-form OOD 10.25 (per distil-large-v3.5 c… | MIT |  | DA | no |  |
| distil-small-en | A | en | 453.7 | 681 | within ~3% WER of large-v2 (card text); table value not captured | MIT |  | DA | no |  |
| distil-large-v3.5 | A | en | 528.7 | 793 | Short-form OOD 7.08, long-form OOD 11.39 (card) | MIT |  | DA | yes |  |
| fastconformer-multi-eu-20k-int8 | A | be de en es fr hr it pl ru … | 106.5 | 160 | null | unverified (NeMo, typically… | B | DA | no |  |
| stt-de-fastconformer-hybrid-int8 | A | de (with punctuation+case) | 108.1 | 162 | null | CC-BY-4.0 |  | DA | no |  |
| stt-es-fastconformer-1424-int8 | A | es | 107.2 | 161 | null | unverified | B | DA | no |  |
| gigaam-v3-ru-transducer-punct | A | ru | 170.2 | 255 | null | unverified | B | DA | no |  |
| t-one-ru-streaming | A | ru | 128.5 | 193 | null | unverified | B | DA | no |  |
| vosk-small-ru-streaming-int8 | A | ru | 24.1 | 36 | null | unverified (Vosk models usu… | B | DA | no |  |
| qwen3-asr-0.6b-int8 | A | 30+ languages incl. en zh y… | 878.7 | 1318 | Card has multi-model tables (LS clean/other, Fleurs-en 2.40..., accen… | Apache-2.0 |  | DA | yes |  |
| cohere-transcribe-14lang-int8 | A | 14 languages | 1699.8 | 2550 | null | Apache-2.0 (HF gated: auto) |  | DA | no |  |
| medasr-ctc-en-int8 | A | en (medical dictation) | 131.8 | 198 | null | license:other (Health AI De… | B | DA | no |  |
| sense-voice-int8 | A | zh en ja ko yue | 165.8 | 249 | null | unverified | B | DA | no |  |
| voxtral-mini-4b-realtime | A | 13 languages (card) | null | null | null | Apache-2.0 |  | DA | no |  |
| kyutai-stt-1b-en_fr | A | en fr | null | null | null | CC-BY-4.0 |  | DA | no |  |
| silero-vad-int8 | V | language-agnostic | 0.2 | 10 | null | MIT (upstream; unverified i… |  | DA | no | yes |
| ten-vad-int8 | V | language-agnostic | 0.1 | 10 | null | unverified (TEN VAD has cus… | B | DA | no |  |
| s1-mini | L | en (multilingual not tested) | 484.2 | 428 | Project bake-off, 109 cases: final WER 4.9%, exact 65.1% | see superwhisper/s1-mini-GG… | B | DA | no | yes |
| qwen3-0.6b-q8 | L | 100+ languages (Qwen3 claim… | 639.4 | 767 | Bake-off (Q4_K_M, 4-shot): final WER 11.6%, exact 47.7% | Apache-2.0 |  | DA | no |  |
| qwen2.5-0.5b-q4km | L | 29+ languages (Qwen2.5 clai… | 491.4 | 590 | null | Apache-2.0 |  | DA | no |  |
| gemma-3-270m-q8 | L | 140+ (Google claim, unverif… | 291.5 | 350 | Bake-off: final WER 16.2%, raw 53.7% | Gemma Terms of Use (not OSI) |  | DA | no |  |
| smollm2-360m-q8 | L | en only | 386.4 | 464 | null | Apache-2.0 |  | DA | no |  |
| lfm2.5-350m-q4km | L | en ar zh fr de ja ko es (ca… | 229.3 | 278 | Bake-off: final WER 22.7%, raw 21.1% | LFM Open License v1.0 (HF l… |  | DA | no |  |
| indicconformer-ta-int8 | A | Tamil | 137.6 | 329 | FLEURS ta_in, first 100 sentences, third-party measurement: WER 33.5 … | MIT (upstream ai4bharat, ga… |  | DA | yes |  |
| indicconformer-te-int8 | A | Telugu | 137.6 | 329 | null | MIT |  | DA | no |  |
| indicconformer-ml-int8 | A | Malayalam | 137.6 | 329 | null | MIT |  | DA | no |  |
| indicconformer-pa-int8 | A | Punjabi | 197.7 | 407 | null | MIT |  | DA | no |  |
| indicconformer-600m-22lang-int8 | A | as bn brx doi gu hi kn ks k… | 658.6 | 1006 | Third-party, synthetic medical dialogues (Svara TTS, ~470 clips/lang)… | MIT |  | DA | no |  |
| indicconformer-8lang-meetsync | A | as bn brx gu hi kn ks mr (c… | 197.1 | 406 | null | MIT |  | DA | no |  |
| vosk-bn-streaming-zipformer | A | Bengali | 87.3 | 263 | alphacep card (WER %): Fleurs 20.6, Common Voice 17.9, Kathbath 19.3,… | Apache-2.0 |  | DA | no |  |
| whisper-hindi-small-vistaar | A | Hindi | null | null | FLEURS hi_in WER 9.02; Common Voice 11 hi WER 14.12 (model-index) | Apache-2.0 |  | DA | no |  |
| whisper-hindi-large-v2-vistaar | A | Hindi | null | null | FLEURS hi_in WER 6.80; Common Voice 11 hi WER 10.98 | Apache-2.0 |  | DA | no |  |
| whisper-tamil-telugu-kannada-vistaar | A | Tamil, Telugu, Kannada | null | null | FLEURS: ta large-v2 7.5 (CV11 6.61); te large-v2 9.65; kn medium 7.65 | Apache-2.0 |  | DA | no |  |
| whisper-large-v3-vaani-hindi | A | Hindi | null | null | Fleurs 11.20; Common Voice 13.84; Kathbath 8.85; Kathbath Noisy 11.80… | Apache-2.0 |  | DA | no |  |
| whisper-large-v3-urdu | A | Urdu | null | null | Common Voice 17 ur WER 26.02, CER 9.43 | Apache-2.0 |  | DA | no |  |
| whisper-hindi2hinglish-apex | A | Hindi (romanised output) | null | null | FLEURS hi 29.79; CV20 hi 35.96; IndicVoices hi 47.64 (Hinglish-normal… | Apache-2.0 |  | DA | no |  |
| dolphin-base-ctc-int8 | A | incl. hi ur bn ta te gu mr … | 80.7 | 255 | Average WER 33.3 over Dolphin's own test sets (card); per-language: n… | Apache-2.0 |  | DA | no |  |
| dolphin-small-ctc-int8 | A | same as base | 191.5 | 399 | Average WER 25.2 (card); FLEURS-ta (third-party, 100 sent) WER 66.3 /… | Apache-2.0 |  | DA | no |  |
| omnilingual-ctc-300m-v2-int8 | A | 1600+ incl. Swahili, Hausa,… | 292.3 | 530 | null | Apache-2.0 |  | DA | no |  |
| omnilingual-ctc-1b-v2-int8 | A | 1600+ | 787.3 | 1173 | null | Apache-2.0 |  | DA | no |  |
| sensevoice-small-int8-2025-09-09 | A | Chinese, English, Japanese,… | 165.8 | 366 | Card claims it beats Whisper on AISHELL-1/2, WenetSpeech, LibriSpeech… | FunASR Model Open Source Li… | B | DA | no |  |
| nemotron-3.5-asr-streaming-0.6b-int8 | A | en es de fr it ar ja ko pt … | 475.3 | 768 | FLEURS, 1.12 s chunk with LangID: Hindi 6.81, Korean 7.12 (others in … | OpenMDW-1.1 (permissive ope… |  | DA | no |  |
| fireredasr2-aed-int8 | A | Mandarin (20+ dialects), En… | 838.6 | 1658 | Mandarin 4 public sets avg CER 3.05 (AED), dialects 19 sets avg CER 1… | Apache-2.0 |  | DA | no |  |
| paraformer-zh-small-int8 | A | Chinese (+ some English) | 77.9 | 251 | null | FunASR Model Open Source Li… | B | DA | no |  |
| x-asr-zipformer-zh-en-int8 | A | Chinese, English | 136.9 | 328 | null | unverified | B | DA | no |  |
| zipformer-ja-reazonspeech | A | Japanese | 713.1 | 1077 | null | Apache-2.0 |  | DA | no |  |
| parakeet-tdt-ctc-0.6b-ja-int8 | A | Japanese | 489.4 | 786 | CER: JSUT basic5000 6.4; Common Voice 8 ja 7.1; Common Voice 16.1 ja … | CC-BY-4.0 |  | DA | no |  |
| zipformer-korean-ksponspeech | A | Korean | 329.7 | 579 | KsponSpeech CER eval_clean 10.35 / eval_other 11.35 (modified beam se… | Apache-2.0 (model card); Ks… | B | DA | no |  |
| zipformer-thai-gigaspeech2 | A | Thai | 695.6 | 1054 | null | Apache-2.0 (model card); Gi… | B | DA | no |  |
| zipformer-vi-int8 | A | Vietnamese | 60.2 | 228 | null | Apache-2.0 (source card) |  | DA | no |  |
| zipformer-multi-ar-en-id-ja-ru-th-vi-zh | A | Arabic, English, Indonesian… | 259.0 | 487 | null | unverified | B | DA | no |  |
| shenava-koochik-fa-int8 | A | Persian | 99.3 | 279 | FLEURS-fa WER 10.64 / CER 3.79 (FP32 NeMo parent; int8 not separately… | Apache-2.0 |  | DA | no |  |
| whisper-turkish-small-int8 | A | Turkish | 375.4 | 963 | FLEURS tr (743): small-tr 12.0% (fp16 GPU) vs openai small 14.8%; bas… | Apache-2.0 |  | DA | yes |  |
| whisper-large-v3-turbo-arabic-dialectal | A | Arabic (dialectal) | 1086.7 | 2030 | null | Apache-2.0 |  | DA | no |  |
| moonshine-base-ar-vi-ja-ko-zh | A | ar vi ja ko zh (base); ja k… | 119.1 | 579 | null | Moonshine Community License… | B | DA | no |  |
| whisper-large-v3-turbo-ivrit | A | Hebrew | 1614.3 | 2821 | null | Apache-2.0 (subtitleking re… |  | DA | no |  |
| mms-1b-all | A | 1100+ | null | null | null | CC-BY-NC-4.0 | B | DA | no |  |
| vakyansh-wav2vec2 | A | various Indic | null | null | null | MIT (ta/pa/hi cards) |  | DA | no |  |
