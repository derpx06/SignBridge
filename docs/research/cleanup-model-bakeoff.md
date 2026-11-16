# Cleanup model bake-off (2026-11-06)

Task: fix grammar, fluency, fillers, numbers and formatting of dictated text without changing meaning, in
well under 1 GB for the whole app. Test set: `engine/eval/fluency.tsv`, 109 hand-written cases with
reference answers (grammar 20, formatting 20, already-correct 20, disfluency 15, traps 15, long 8,
self-correction 6, short 5). Machine: 16 cores, CPU only, 4 threads. Q4_K_M unless noted. Lower WER is better.
"Final" = what the user gets (model, then guardrail, then rule fallback).

| System | File | Model RSS | Exact % | WER % final | WER % raw model | Latency p50 |
|---|---|---|---|---|---|---|
| Rules only | none | 0 | 18.3 | 14.2 | n/a | 0 |
| S1-mini (Qwen3-0.6B fine-tune), tied + mmap | 356 MB | 428 | 65.1 | 4.9 | 4.9 | about 1.0 s |
| S1-mini, original file, repacked | 484 MB | 550 | 56.9 (old guard) | 8.1 | 4.8 | 0.6-1.1 s |
| Qwen3-0.6B + 4 examples | 397 MB | 467 | 47.7 | 11.6 | 9.9 | 1.3 s |
| Qwen3-0.6B Q3_K_S + 4 examples | 323 MB | 413 | 26.6 | 11.4 | 9.5 | 1.9 s |
| LFM2.5-350M + 4 examples | 229 MB | 278 | 22.0 | 22.7 | 21.1 | 0.7 s |
| Gemma 3 270M + 4 examples | 253 MB | 322 | 23.9 | 16.2 | 53.7 | 0.7 s |

Findings
- Models under 350 MB prompted with instructions and examples are worse than the rules (LFM2.5, Gemma 3 270M).
  Reaching quality at that size needs fine-tuning on this task, which was not done.
- S1-mini is the best by a wide margin. Its weak spot is verb agreement and tense (grammar 25% exact);
  generic Qwen3-0.6B with examples is better at grammar (60% exact raw) but worse at everything else.
- The original guardrail wrongly refused 9 of 105 correct answers (it saw "dont" -> "don't" as a changed
  negation, and number or email formatting as dropped words). Fixing that took final WER from 8.1 to 4.9.
- Dropping the duplicate output table and memory-mapping the weights saved about 120 MB at identical quality.
  Cost: slower prompt processing (about 1.0 s p50 against 0.6 s).
- Not found: any published benchmark ranking sub-1B models on grammar correction. These numbers are ours.
