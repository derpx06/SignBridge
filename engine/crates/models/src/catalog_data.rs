// Generated from the files in the models folder; checksums are SHA-256 of the exact upstream files.
// Do not edit by hand: re-run the generator when a model changes.

use crate::FileSpec;

pub static SILERO: &[FileSpec] = &[
    FileSpec { path: "silero_vad.onnx", url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx", size: 643854, sha256: "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6" },
];

/// Files of the 110M model after extracting its official archive.
pub static PARAKEET_110M: &[FileSpec] = &[
    FileSpec { path: "sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8/encoder.int8.onnx", url: "", size: 131113202, sha256: "0f35509ddeb9b39002fb077d979a9fe74f06eb0bc4dd5c34f512f82e5111d657" },
    FileSpec { path: "sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8/decoder.int8.onnx", url: "", size: 3955863, sha256: "f7c331c5504c2e593c76ed22b728e3f554af6c4a383dde862e719ced08b1da19" },
    FileSpec { path: "sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8/joiner.int8.onnx", url: "", size: 1411403, sha256: "bf7dff69e9f2cdbe9943d70da358f38b361c115ba0105bae7e908e0d6ec782f6" },
    FileSpec { path: "sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8/tokens.txt", url: "", size: 9953, sha256: "450e56bd2f036fe5b6aa821865838cc5aa9d8b0106134ce9a9ba0664abe6cd10" },
];

pub const PARAKEET_110M_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8.tar.bz2";
pub const PARAKEET_110M_ARCHIVE_SIZE: u64 = 108035095;
pub const PARAKEET_110M_ARCHIVE_SHA256: &str = "f628312e9fdf8686374cb01a69425c41732529d540860311f16f37cbc32cfe9b";

pub static PARAKEET_06B: &[FileSpec] = &[
    FileSpec { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/encoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/resolve/main/encoder.int8.onnx", size: 652184296, sha256: "a32b12d17bbbc309d0686fbbcc2987b5e9b8333a7da83fa6b089f0a2acd651ab" },
    FileSpec { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/decoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/resolve/main/decoder.int8.onnx", size: 7257753, sha256: "b6bb64963457237b900e496ee9994b59294526439fbcc1fecf705b31a15c6b4e" },
    FileSpec { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/joiner.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/resolve/main/joiner.int8.onnx", size: 1739080, sha256: "7946164367946e7f9f29a122407c3252b680dbae9a51343eb2488d057c3c43d2" },
    FileSpec { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/resolve/main/tokens.txt", size: 9384, sha256: "ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d" },
];

pub static S1_MINI: &[FileSpec] = &[
    FileSpec { path: "s1-mini/s1-mini-q4_k_m.gguf", url: "https://huggingface.co/superwhisper/s1-mini-GGUF/resolve/main/s1-mini-q4_k_m.gguf", size: 484219808, sha256: "3b41ebe2502cbd03e811d5d16b022f5ab551eda58d62597d152f89535003c634" },
    FileSpec { path: "s1-mini/LICENSE", url: "https://huggingface.co/superwhisper/s1-mini-GGUF/resolve/main/LICENSE", size: 12033, sha256: "f715982df6ce767ae64864d74b95644e5d658aab54717dbceeb252eb3cbcb421" },
    FileSpec { path: "s1-mini/NOTICE", url: "https://huggingface.co/superwhisper/s1-mini-GGUF/resolve/main/NOTICE", size: 507, sha256: "6eb1e98a0d0046dd1b71da19d3b64ebccc12622e7d62e5efcad10298b4d66292" },
];

// Optional language packs (Model library). Sizes and SHA-256 re-verified by downloading each file.
pub static INDIC_HI: &[FileSpec] = &[
    FileSpec { path: "indicconformer-hi/model.int8.onnx", url: "https://huggingface.co/parismitaglobalsolutions/indicconformer-sherpa-onnx/resolve/main/hi/model.int8.onnx", size: 197595593, sha256: "915c71e04dd7e5378a4057fdebb252b3a587188e4e99db6d7ce0909ad5ad05fa" },
    FileSpec { path: "indicconformer-hi/tokens.txt", url: "https://huggingface.co/parismitaglobalsolutions/indicconformer-sherpa-onnx/resolve/main/tokens.txt", size: 67605, sha256: "ee60967630213f31951817ac8b402b92ec18cce80718a24a49b388e56672dfb2" },
];

pub static PARAKEET_V3: &[FileSpec] = &[
    FileSpec { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/encoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main/encoder.int8.onnx", size: 652282294, sha256: "1afbe69c1ffdabee94755f42e4daf1d93549131a3da11356bda84fcef0f2c479" },
    FileSpec { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/decoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main/decoder.int8.onnx", size: 11845274, sha256: "0222a86147299d4dee90edf95b6d13827edb42ad8b7c9cf0b2582f030937c0b5" },
    FileSpec { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/joiner.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main/joiner.int8.onnx", size: 6355277, sha256: "cbd02f3696e9944970111fb439d9c3fd365a80374565f13f2b9c7007d253fc98" },
    FileSpec { path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main/tokens.txt", size: 93939, sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d" },
];

pub static WHISPER_SMALL: &[FileSpec] = &[
    FileSpec { path: "sherpa-onnx-whisper-small/small-encoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-small/resolve/main/small-encoder.int8.onnx", size: 112442483, sha256: "4cbe7b22fa9026b843b60a68640c747de05bafb1a11b57edc0e66c232d9f33a9" },
    FileSpec { path: "sherpa-onnx-whisper-small/small-decoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-small/resolve/main/small-decoder.int8.onnx", size: 262226114, sha256: "acad50b5c782696e91b55914cc5ab4f756f1532f76e22aa6fc615f39fb69a8ee" },
    FileSpec { path: "sherpa-onnx-whisper-small/small-tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-small/resolve/main/small-tokens.txt", size: 816730, sha256: "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126" },
];

// Added from docs/research/model-catalogue-100.json. Sizes and SHA-256 computed by downloading each file.
pub static WHISPER_TINY_EN: &[FileSpec] = &[
    FileSpec { path: "sherpa-onnx-whisper-tiny.en/tiny.en-encoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en/resolve/main/tiny.en-encoder.int8.onnx", size: 12937772, sha256: "0ce578b827c94a961aacb8fa14b02f096504b337e5c94be37c36238cbe3e8bc6" },
    FileSpec { path: "sherpa-onnx-whisper-tiny.en/tiny.en-decoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en/resolve/main/tiny.en-decoder.int8.onnx", size: 89853865, sha256: "06c0e6ff6348d427e51839219d1c886c18cfdf411e629e33f5e1679bff9c1527" },
    FileSpec { path: "sherpa-onnx-whisper-tiny.en/tiny.en-tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en/resolve/main/tiny.en-tokens.txt", size: 835554, sha256: "306cd27f03c1a714eca7108e03d66b7dc042abe8c258b44c199a7ed9838dd930" },
];

pub static WHISPER_BASE: &[FileSpec] = &[
    FileSpec { path: "sherpa-onnx-whisper-base/base-encoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base/resolve/main/base-encoder.int8.onnx", size: 29120534, sha256: "0b8fb1304b6109976038efff5ace81720e00386f3ff6b54ee8c75291ca0a1e11" },
    FileSpec { path: "sherpa-onnx-whisper-base/base-decoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base/resolve/main/base-decoder.int8.onnx", size: 130672026, sha256: "9759d217388a01b3a4c7c15533201067b48ae819c4daafc8624e64b9409dc02d" },
    FileSpec { path: "sherpa-onnx-whisper-base/base-tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base/resolve/main/base-tokens.txt", size: 816730, sha256: "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126" },
];

pub static INDIC_TA: &[FileSpec] = &[
    FileSpec { path: "indicconformer-ta/model.int8.onnx", url: "https://huggingface.co/csndhanasekar/sherpa-onnx-indicconformer-ta-int8/resolve/main/model.int8.onnx", size: 137563718, sha256: "dfedd42ca1a143fa0f08808b6b5d8f03d1a86e904d68375b335e39417128c0de" },
    FileSpec { path: "indicconformer-ta/tokens.txt", url: "https://huggingface.co/csndhanasekar/sherpa-onnx-indicconformer-ta-int8/resolve/main/tokens.txt", size: 3299, sha256: "61c6ad5e1d4b28ebbd38853209cf169bdc4dcd62cc32d9ec5185f3530371636d" },
];

pub static NEMO_DE: &[FileSpec] = &[
    FileSpec { path: "sherpa-onnx-nemo-transducer-stt_de_fastconformer_hybrid_large_pc-int8/encoder.int8.onnx", url: "", size: 131114014, sha256: "10fbb959c36c461afb02ea87c109119eb001f30e91a130663bd6d5bbcba74ba9" },
    FileSpec { path: "sherpa-onnx-nemo-transducer-stt_de_fastconformer_hybrid_large_pc-int8/decoder.int8.onnx", url: "", size: 3955863, sha256: "4633a4b0a3f21f7ace4df02e772f5b5e63d45f84a71ee1e2660998fc4beb46ad" },
    FileSpec { path: "sherpa-onnx-nemo-transducer-stt_de_fastconformer_hybrid_large_pc-int8/joiner.int8.onnx", url: "", size: 1408183, sha256: "bac3af36a9bd66bfcad2ae6a35f17c7853e034e497134a8b0cbe48a8e954bd9f" },
    FileSpec { path: "sherpa-onnx-nemo-transducer-stt_de_fastconformer_hybrid_large_pc-int8/tokens.txt", url: "", size: 10686, sha256: "abb1136142604d6d1766ad5060bd4f4b1048d7a096cd094b2d40eec3e666be9f" },
];

pub const NEMO_DE_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-transducer-stt_de_fastconformer_hybrid_large_pc-int8.tar.bz2";
pub const NEMO_DE_ARCHIVE_SIZE: u64 = 108124747;
pub const NEMO_DE_ARCHIVE_SHA256: &str = "73b3cce9f94a19fc04bc54b55abd576b622e3925a84ddd554a95e5984bc11677";
