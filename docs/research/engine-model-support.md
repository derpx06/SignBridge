# Speech engine model support (measured 2026-11-10)

`voicio-engine worker --asr <dir>` now detects the model family from the files in `<dir>` (`AsrKind::detect` in
`engine/src/lib.rs`): `encoder+joiner.int8.onnx` = NeMo transducer, `model.int8.onnx` = NeMo CTC,
`*-encoder.int8.onnx` = Whisper. All three run as sherpa-onnx *offline* recognisers, which is what the engine
already used for Parakeet, so the existing VAD-then-decode path is unchanged. Partials are re-decodes of the
growing utterance for every family, so the CTC/Whisper packs show live words too (Whisper only slowly, see RTF).
`--language CODE` (Whisper only; empty = auto-detect). Unsupported folder or a pack whose estimated resident size
(file bytes x1.65, x2.7 for Whisper, + 60 MB) exceeds `--memory-budget` is refused with a clear error before loading.

Test box: 16 GB laptop CPU, 3 ASR threads (balanced), nice +5 not applied (eval mode). Audio: 14 s + 4 s English
(LibriSpeech-style clip + flite TTS), 25 s of real Hindi (Wikimedia Commons "Hindi Dengue Introduction", no
reference transcript). Files downloaded to `~/.cache/voicio-bakeoff/models`, SHA-256 verified against the catalog.

| Model | Load RSS | Peak RSS | RTF (decode time / audio) | CPU (user+sys) / wall | Result |
|---|---|---|---|---|---|
| parakeet-110m (baseline) | 181 MB | 305 MB | 0.04 | 1.4 cores | English correct |
| parakeet-v3 | 698 MB | ~1040 MB | 0.09-0.14 | ~1.8 cores | English correct (punctuated); Hindi audio -> Latin gibberish (not a v3 language, expected) |
| indic-hi (NeMo CTC) | 238 MB | 357 MB | 0.08-0.10 | ~2.1 cores | Hindi transcript fluent and plausible on the Commons clip (no reference, so no WER) |
| whisper-small | 425 MB | ~890-960 MB | 0.5-0.8 | ~1.3-1.9 cores | English correct and punctuated; Hindi (auto or `hi`) garbled |

Catalog: `parakeet-v3`, `indic-hi`, `whisper-small` set `engine_ready: true`; `whisper-small` `ram_mb` raised 800 -> 1000
to match the measured peak. With the default 1024 MB budget, v3 and Whisper refuse to load (needs 1115 / ~1070 MB);
the agent passes 3000 MB for Quality tier and about 1024 minus its own RSS for others.

Could not verify: any non-English European language on v3 (no non-English audio or TTS on this machine);
Indic accuracy as a WER number (no reference transcript); Windows/macOS; Parakeet 0.6B v2 was not installed so
it was not re-run after the refactor (same code path as v3); streaming latency inside the full agent (only the
engine `--eval` harness was run); `speech_pack`/`language` -> `--asr`/`--language` mapping is in agent.rs, not done here.
