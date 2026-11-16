# Third-party notices

Voicio copies and adapts code from the following projects. Each file taken from them carries a
header naming its origin; the licence text is reproduced here as that licence requires.

## Hex (https://github.com/anomalyco/hex)

Used in: `engine/crates/app/src/pill/spring.rs` (closed-form spring, from `src/rune_hud/spring.rs`),
`engine/crates/app/src/replacements.rs` (from `src/dictation/text_replacements.rs`). `durable.rs`, `history.rs`
and the typing ladder follow Hex's design but are written for Voicio.
Further Hex modules will be listed here as they are ported.

MIT License

Copyright (c) 2026 Christopher Langton

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## sherpa-onnx (Android Kotlin API and native libraries)
`app/src/main/kotlin/com/k2fsa/sherpa/onnx/*.kt` are copied unchanged from k2-fsa/sherpa-onnx v1.13.8
(Copyright Xiaomi Corporation and contributors), Apache License 2.0. The native libraries fetched by
`scripts/fetch-sherpa-android.sh` (sherpa-onnx, ONNX Runtime) are under Apache-2.0 and MIT respectively.

## Optional model packs (downloaded by the user from the Model library; not bundled)

- NVIDIA Parakeet TDT 0.6B v3 (and the 110M and 0.6B v2 models): (c) NVIDIA, licensed CC-BY-4.0
  (https://creativecommons.org/licenses/by/4.0/). The int8 ONNX exports are by k2-fsa / csukuangfj
  (sherpa-onnx). Voicio does not modify the weights; it downloads them unchanged and checks their SHA-256.
- OpenAI Whisper small, multilingual: MIT licence, (c) OpenAI. int8 export by csukuangfj (sherpa-onnx).
- AI4Bharat IndicConformer (Hindi): MIT licence. The sherpa-onnx export used is a community export by
  parismitaglobalsolutions (https://huggingface.co/parismitaglobalsolutions/indicconformer-sherpa-onnx);
  its quality has not been verified by Voicio.

## Fonts bundled in the desktop app, Android app and Chrome extension

- Inter, Copyright 2020 The Inter Project Authors (https://github.com/rsms/inter).
- Bricolage Grotesque, Copyright 2022 The Bricolage Grotesque Project Authors (https://github.com/ateliertriay/bricolage).

Both are licensed under the SIL Open Font License, Version 1.1 (full text in `OFL-Inter.txt` and
`OFL-Bricolage.txt` next to the fonts). The Android app bundles Bricolage Grotesque ExtraBold
(`app/src/main/res/font`); the Chrome extension bundles Inter and Bricolage Grotesque through the OFL-licensed
`@fontsource` npm packages (the licence files ship inside them), plus JetBrains Mono (OFL-1.1, The JetBrains Mono Project Authors). The bundled files are static instances (Inter 400/500/600/700,
Bricolage Grotesque 800) subset to Latin, made with fontTools from the Google Fonts variable fonts.
