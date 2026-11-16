//! Voicio's models: what exists, which tier fits this machine, whether the files are installed
//! and intact, and the downloader. This crate holds all of the app's model-download network code;
//! dictation itself never needs the network once the files are on disk.
//!
//! Every file is pinned by SHA-256 in `catalog_data.rs`, so a changed, truncated or tampered file
//! is rejected whichever server it came from.

mod catalog_data;
mod install;

pub use install::{install, Cancel, InstallError, Progress};

use std::path::Path;

/// One file of a model, pinned by size and SHA-256.
pub struct FileSpec {
    /// Path relative to the models folder.
    pub path: &'static str,
    /// Where to download it (empty when it comes out of an archive).
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub enum Source {
    Files(&'static [FileSpec]),
    /// A `.tar.bz2` whose entries are exactly the listed paths (other entries are ignored).
    Archive { url: &'static str, size: u64, sha256: &'static str, files: &'static [FileSpec] },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    VoiceDetection,
    Speech,
    Cleanup,
}

pub struct Model {
    pub id: &'static str,
    /// The model's own name, first: "IndicConformer Hindi", "Parakeet v3".
    pub name: &'static str,
    /// Who made it, for "<languages> · by <maker>".
    pub maker: &'static str,
    pub kind: Kind,
    /// One line a non-expert can act on.
    pub summary: &'static str,
    pub detail: &'static str,
    /// Extra memory while loaded.
    pub ram_mb: u32,
    pub license: &'static str,
    /// Credit the licence requires us to show (About page and Models page).
    pub credit: &'static str,
    pub source: Source,
    /// Not part of any tier: a language pack the user adds from the Model library.
    pub optional: bool,
    /// Languages it understands, for people (empty for models that are not speech models).
    pub languages: &'static str,
    /// Caveat shown beside the model (empty for none).
    pub note: &'static str,
    /// The speech engine can run it today. False: downloadable, engine support coming.
    pub engine_ready: bool,
    /// Licence-clean and loadable, but not verified end to end by us. Shown with a caveat; never pre-selected.
    pub experimental: bool,
}

impl Model {
    pub fn files(&self) -> &'static [FileSpec] {
        match &self.source {
            Source::Files(files) | Source::Archive { files, .. } => files,
        }
    }

    /// Bytes to download.
    pub fn download_bytes(&self) -> u64 {
        match &self.source {
            Source::Files(files) => files.iter().map(|f| f.size).sum(),
            Source::Archive { size, .. } => *size,
        }
    }

    /// Bytes on disk once installed.
    pub fn installed_bytes(&self) -> u64 {
        self.files().iter().map(|f| f.size).sum()
    }
}

pub static CATALOG: &[Model] = &[
    Model {
        id: "silero-vad",
        name: "Silero VAD",
        maker: "Silero",
        kind: Kind::VoiceDetection,
        summary: "Finds where you are speaking, so silence is never transcribed.",
        detail: "A tiny model that runs all the time while you dictate.",
        ram_mb: 5,
        license: "MIT",
        credit: "Silero VAD, MIT licence",
        source: Source::Files(catalog_data::SILERO),
        optional: false,
        languages: "",
        note: "",
        engine_ready: true,
        experimental: false,
    },
    Model {
        id: "parakeet-110m",
        name: "Parakeet 110M",
        maker: "NVIDIA",
        kind: Kind::Speech,
        summary: "Fast, light and accurate English speech recognition. Good on any laptop.",
        detail: "NVIDIA Parakeet TDT, 110 million parameters, int8. About 25 seconds of speech per second of compute on a modern CPU.",
        ram_mb: 310,
        license: "CC-BY-4.0",
        credit: "NVIDIA Parakeet TDT 110M, CC-BY-4.0, via sherpa-onnx",
        source: Source::Archive {
            url: catalog_data::PARAKEET_110M_ARCHIVE_URL,
            size: catalog_data::PARAKEET_110M_ARCHIVE_SIZE,
            sha256: catalog_data::PARAKEET_110M_ARCHIVE_SHA256,
            files: catalog_data::PARAKEET_110M,
        },
        optional: false,
        languages: "English",
        note: "",
        engine_ready: true,
        experimental: false,
    },
    Model {
        id: "parakeet-0.6b-v2",
        name: "Parakeet 0.6B",
        maker: "NVIDIA",
        kind: Kind::Speech,
        summary: "The most accurate option. Needs a computer with 16 GB of memory.",
        detail: "NVIDIA Parakeet TDT, 600 million parameters, int8. Noticeably better on accents, names and noisy rooms; slower to load.",
        ram_mb: 1500,
        license: "CC-BY-4.0",
        credit: "NVIDIA Parakeet TDT 0.6B v2, CC-BY-4.0, via sherpa-onnx",
        source: Source::Files(catalog_data::PARAKEET_06B),
        optional: false,
        languages: "English",
        note: "",
        engine_ready: true,
        experimental: false,
    },
    Model {
        id: "s1-mini",
        name: "S1-mini",
        maker: "Superwhisper",
        kind: Kind::Cleanup,
        summary: "Adds punctuation, removes fillers and fixes numbers after you speak.",
        detail: "A small language model that tidies the text. Voicio rejects any rewrite that changes your meaning and falls back to the raw text.",
        ram_mb: 420,
        license: "Apache-2.0",
        credit: "\"S1-mini\" by \"Superwhisper\" (based on Qwen3-0.6B, Alibaba Cloud), Apache-2.0",
        source: Source::Files(catalog_data::S1_MINI),
        optional: false,
        languages: "English",
        note: "",
        engine_ready: true,
        experimental: false,
    },
    // ---- Model library: optional language packs. Pinned like every other file; the speech worker
    // runs all of them (transducer, NeMo CTC, Whisper: see docs/research/engine-model-support.md).
    Model {
        id: "indic-hi",
        name: "IndicConformer Hindi",
        maker: "AI4Bharat",
        kind: Kind::Speech,
        summary: "Hindi speech recognition from AI4Bharat's IndicConformer, int8.",
        detail: "A community export of AI4Bharat IndicConformer for sherpa-onnx (NeMo CTC). Not made or checked by AI4Bharat or Voicio: community export, quality unverified.",
        ram_mb: 450,
        license: "MIT",
        credit: "IndicConformer Hindi (AI4Bharat), community sherpa-onnx export by parismitaglobalsolutions, MIT",
        source: Source::Files(catalog_data::INDIC_HI),
        optional: true,
        languages: "Hindi",
        note: "Community export, quality unverified.",
        engine_ready: true,
        experimental: false,
    },
    Model {
        id: "parakeet-v3",
        name: "Parakeet v3",
        maker: "NVIDIA",
        kind: Kind::Speech,
        summary: "One model for English, German, French, Spanish, Italian, Dutch, Polish, Portuguese, Russian, Ukrainian and 15 more.",
        detail: "NVIDIA Parakeet TDT 0.6B v3, int8, with automatic language detection. Same size class as the English 0.6B model.",
        ram_mb: 1500,
        license: "CC-BY-4.0",
        credit: "NVIDIA Parakeet TDT 0.6B v3, CC-BY-4.0, via sherpa-onnx (csukuangfj)",
        source: Source::Files(catalog_data::PARAKEET_V3),
        optional: true,
        languages: "25 European languages: English, German, French, Spanish, Italian, Polish…",
        note: "",
        engine_ready: true,
        experimental: false,
    },
    Model {
        id: "whisper-small",
        name: "Whisper small",
        maker: "OpenAI",
        kind: Kind::Speech,
        summary: "The widest language coverage, including Hindi. Less accurate than the dedicated models.",
        detail: "OpenAI Whisper small, multilingual, int8, via sherpa-onnx.",
        ram_mb: 1000,
        license: "MIT",
        credit: "OpenAI Whisper small, MIT, via sherpa-onnx (csukuangfj)",
        source: Source::Files(catalog_data::WHISPER_SMALL),
        optional: true,
        languages: "99 languages incl. Hindi, Arabic, Chinese, Japanese",
        note: "Verified on English; Hindi output from the small model was garbled in testing, use the Hindi pack for Hindi.",
        engine_ready: true,
        experimental: false,
    },
    Model {
        id: "whisper-tiny-en",
        name: "Whisper tiny.en",
        maker: "OpenAI",
        kind: Kind::Speech,
        summary: "The smallest English model. Fast, but clearly less accurate than Parakeet.",
        detail: "OpenAI Whisper tiny.en, int8, via sherpa-onnx. LibriSpeech test-clean/other WER 8.44 / 14.86 (OpenAI model card, FP32).",
        ram_mb: 285,
        license: "Apache-2.0",
        credit: "OpenAI Whisper tiny.en, Apache-2.0, via sherpa-onnx (csukuangfj)",
        source: Source::Files(catalog_data::WHISPER_TINY_EN),
        optional: true,
        languages: "English",
        note: "Low accuracy; only for very small machines.",
        engine_ready: true,
        experimental: false,
    },
    Model {
        id: "whisper-base",
        name: "Whisper base",
        maker: "OpenAI",
        kind: Kind::Speech,
        summary: "A small multilingual model. Better than tiny, still well behind the dedicated models.",
        detail: "OpenAI Whisper base, multilingual, int8, via sherpa-onnx. LibriSpeech test-clean/other WER 5.01 / 12.85 (OpenAI model card, FP32).",
        ram_mb: 360,
        license: "Apache-2.0",
        credit: "OpenAI Whisper base, Apache-2.0, via sherpa-onnx (csukuangfj)",
        source: Source::Files(catalog_data::WHISPER_BASE),
        optional: true,
        languages: "99 languages incl. Hindi, Arabic, Chinese, Japanese",
        note: "May invent words on silence or noise.",
        engine_ready: true,
        experimental: false,
    },
    Model {
        id: "indic-ta",
        name: "IndicConformer Tamil",
        maker: "AI4Bharat",
        kind: Kind::Speech,
        summary: "Tamil speech recognition from AI4Bharat's IndicConformer, int8.",
        detail: "A community export of AI4Bharat IndicConformer for sherpa-onnx (NeMo CTC). Independent FLEURS test (first 100 sentences): WER 33.5, CER 16.7. Not made or checked by AI4Bharat or Voicio.",
        ram_mb: 330,
        license: "MIT",
        credit: "IndicConformer Tamil (AI4Bharat), community sherpa-onnx export by csndhanasekar, MIT",
        source: Source::Files(catalog_data::INDIC_TA),
        optional: true,
        languages: "Tamil",
        note: "Community export; accuracy measured by a third party only.",
        engine_ready: true,
        experimental: false,
    },
    Model {
        id: "nemo-de",
        name: "NeMo FastConformer German",
        maker: "NVIDIA",
        kind: Kind::Speech,
        summary: "German speech recognition with punctuation and capitals, in a small download.",
        detail: "NVIDIA NeMo FastConformer hybrid large (German, punctuation and case), transducer, int8, via sherpa-onnx.",
        ram_mb: 165,
        license: "CC-BY-4.0",
        credit: "NVIDIA NeMo FastConformer German, CC-BY-4.0, via sherpa-onnx",
        source: Source::Archive {
            url: catalog_data::NEMO_DE_ARCHIVE_URL,
            size: catalog_data::NEMO_DE_ARCHIVE_SIZE,
            sha256: catalog_data::NEMO_DE_ARCHIVE_SHA256,
            files: catalog_data::NEMO_DE,
        },
        optional: true,
        languages: "German, with punctuation",
        note: "Experimental: not tested by us end to end and no accuracy figure was verified. Parakeet v3 also covers German.",
        engine_ready: true,
        experimental: true,
    },
];

/// Optional language packs for the Model library.
pub fn library() -> impl Iterator<Item = &'static Model> {
    CATALOG.iter().filter(|m| m.optional)
}

pub fn model(id: &str) -> Option<&'static Model> {
    CATALOG.iter().find(|m| m.id == id)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    /// Speech model plus rule-based cleanup: smallest and fastest.
    Lite,
    /// Lite plus the cleanup model.
    Standard,
    /// The big speech model plus the cleanup model.
    Quality,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Lite, Tier::Standard, Tier::Quality];

    pub fn name(self) -> &'static str {
        match self {
            Tier::Lite => "Lite",
            Tier::Standard => "Standard",
            Tier::Quality => "Quality",
        }
    }

    /// The pack's name for people (the stored id stays `name().to_lowercase()`).
    pub fn label(self) -> &'static str {
        match self {
            Tier::Lite => "Lite",
            Tier::Standard => "Balanced",
            Tier::Quality => "Max",
        }
    }

    /// One sentence for the pack's card.
    pub fn line(self) -> &'static str {
        match self {
            Tier::Lite => "Fastest and lightest. A simple tidy-up: capitals and full stops.",
            Tier::Standard => "Smart clean-up of punctuation, fillers and numbers. Right for most people.",
            Tier::Quality => "The most accurate. Best with accents, names, jargon and noisy rooms.",
        }
    }

    /// Accuracy and speed out of five, for the segment bars.
    pub fn scores(self) -> (i32, i32) {
        match self {
            Tier::Lite => (3, 5),
            Tier::Standard => (4, 4),
            Tier::Quality => (5, 2),
        }
    }

    pub fn parse(text: &str) -> Option<Tier> {
        Tier::ALL.into_iter().find(|t| t.name().eq_ignore_ascii_case(text))
    }

    pub fn summary(self) -> &'static str {
        match self {
            Tier::Lite => "Fast and light. Tidies your text with simple rules (capital letters, full stops).",
            Tier::Standard => "Adds smart cleanup of punctuation, fillers, grammar and numbers. Stays under 1 GB of memory in total.",
            Tier::Quality => "The most accurate speech model, plus smart cleanup. Needs about 16 GB of memory.",
        }
    }

    /// The pack's one-word promise, shown on its card.
    pub fn tagline(self) -> &'static str {
        match self {
            Tier::Lite => "Efficient",
            Tier::Standard => "Balanced, recommended",
            Tier::Quality => "Highest accuracy",
        }
    }

    pub fn speed(self) -> &'static str {
        match self {
            Tier::Lite => "Fastest to start and to type",
            Tier::Standard => "Fast; smart cleanup adds a moment after you stop",
            Tier::Quality => "Slower to load and to type",
        }
    }

    pub fn audience(self) -> &'static str {
        match self {
            Tier::Lite => "Older or low-memory laptops (4 to 8 GB), or if you only want quick notes.",
            Tier::Standard => "Most people on an 8 GB or larger computer.",
            Tier::Quality => "16 GB or more of memory; accents, names, jargon and noisy rooms.",
        }
    }

    pub fn model_ids(self) -> &'static [&'static str] {
        match self {
            Tier::Lite => &["silero-vad", "parakeet-110m"],
            Tier::Standard => &["silero-vad", "parakeet-110m", "s1-mini"],
            Tier::Quality => &["silero-vad", "parakeet-0.6b-v2", "s1-mini"],
        }
    }

    pub fn models(self) -> impl Iterator<Item = &'static Model> {
        self.model_ids().iter().filter_map(|id| model(id))
    }

    /// Whether the cleanup model is part of this tier.
    pub fn cleanup(self) -> bool {
        self != Tier::Lite
    }

    /// Memory the tier needs while dictating.
    pub fn ram_mb(self) -> u32 {
        self.models().map(|m| m.ram_mb).sum()
    }

    pub fn download_bytes(self) -> u64 {
        self.models().map(Model::download_bytes).sum()
    }
}

/// The tier that fits this machine: never one that would leave the system short of memory.
/// The floor is an 8 GB, 4-core laptop. Quality is never picked automatically: it is a choice
/// for people with 16 GB or more who want the most accurate model.
pub fn recommended_tier(total_mb: f64, cores: usize) -> Tier {
    if total_mb >= 7_000.0 && cores >= 4 {
        Tier::Standard
    } else {
        Tier::Lite
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Status {
    Installed,
    Missing,
    /// Some files are present (an interrupted install or a deleted file).
    Incomplete,
}

/// Fast check by presence and size: enough to decide whether dictation can start.
pub fn status(dir: &Path, model: &Model) -> Status {
    let present = |f: &FileSpec| std::fs::metadata(dir.join(f.path)).is_ok_and(|m| m.len() == f.size);
    let files = model.files();
    let found = files.iter().filter(|f| present(f)).count();
    if found == files.len() {
        Status::Installed
    } else if found == 0 {
        Status::Missing
    } else {
        Status::Incomplete
    }
}

pub fn tier_installed(dir: &Path, tier: Tier) -> bool {
    tier.models().all(|m| status(dir, m) == Status::Installed)
}

/// Folder (under the models folder) of the tier's speech model, as the engine expects it.
pub fn speech_dir(tier: Tier) -> &'static str {
    let speech = tier.models().find(|m| m.kind == Kind::Speech).expect("every tier has a speech model");
    let path = speech.files()[0].path;
    path.rsplit_once('/').map_or(path, |(dir, _)| dir)
}

/// Dictation can run: voice detection and the speech model are installed. The cleanup model is
/// optional, because a missing one only means rule-based cleanup.
pub fn can_dictate(dir: &Path, tier: Tier) -> bool {
    tier.models().filter(|m| m.kind != Kind::Cleanup).all(|m| status(dir, m) == Status::Installed)
}

pub fn cleanup_installed(dir: &Path) -> bool {
    model("s1-mini").is_some_and(|m| status(dir, m) == Status::Installed)
}

/// Models of a tier that still need downloading.
pub fn missing(dir: &Path, tier: Tier) -> Vec<&'static Model> {
    tier.models().filter(|m| status(dir, m) != Status::Installed).collect()
}

/// Full check: re-hashes every file. Slow for the big models, so only on request.
pub fn verify(dir: &Path, model: &Model) -> Result<(), String> {
    for file in model.files() {
        let path = dir.join(file.path);
        let digest = install::sha256_file(&path).map_err(|e| format!("{}: {e}", file.path))?;
        if digest != file.sha256 {
            return Err(format!("{} is damaged (checksum differs)", file.path));
        }
    }
    Ok(())
}

pub fn format_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_fit_the_machine() {
        assert_eq!(recommended_tier(4_000.0, 2), Tier::Lite);
        assert_eq!(recommended_tier(8_000.0, 4), Tier::Standard);
        assert_eq!(recommended_tier(8_000.0, 2), Tier::Lite);
        assert_eq!(recommended_tier(32_000.0, 16), Tier::Standard); // Quality is opt-in
        assert_eq!(recommended_tier(16_000.0, 4), Tier::Standard);
    }

    #[test]
    fn every_tier_model_is_in_the_catalog_and_has_files() {
        for tier in Tier::ALL {
            assert_eq!(tier.models().count(), tier.model_ids().len(), "{tier:?}");
            assert!(tier.download_bytes() > 0);
        }
        assert!(CATALOG.iter().all(|m| m.files().iter().all(|f| f.sha256.len() == 64)));
    }

    #[test]
    fn status_notices_missing_and_truncated_files() {
        let dir = tempfile::tempdir().unwrap();
        let m = model("silero-vad").unwrap();
        assert_eq!(status(dir.path(), m), Status::Missing);
        std::fs::write(dir.path().join("silero_vad.onnx"), b"short").unwrap();
        assert_eq!(status(dir.path(), m), Status::Missing); // wrong size does not count as present
        assert!(!tier_installed(dir.path(), Tier::Lite));
    }

    #[test]
    fn speech_folder_matches_what_the_engine_loads() {
        assert_eq!(speech_dir(Tier::Lite), "sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8");
        assert_eq!(speech_dir(Tier::Standard), speech_dir(Tier::Lite));
        assert_eq!(speech_dir(Tier::Quality), "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8");
    }

    #[test]
    fn dictation_needs_speech_and_voice_detection_but_not_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!can_dictate(dir.path(), Tier::Standard));
        for id in ["silero-vad", "parakeet-110m"] {
            for f in model(id).unwrap().files() {
                let path = dir.path().join(f.path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::File::create(path).unwrap().set_len(f.size).unwrap();
            }
        }
        assert!(can_dictate(dir.path(), Tier::Standard));
        assert!(!cleanup_installed(dir.path()));
    }

    #[test]
    fn library_packs_are_pinned_and_honest_about_engine_support() {
        assert_eq!(library().count(), 7);
        for m in library() {
            assert!(!m.languages.is_empty());
            assert!(!Tier::ALL.iter().any(|t| t.model_ids().contains(&m.id)), "{} must not be in a tier", m.id);
        }
        assert!(model("indic-hi").unwrap().note.contains("unverified"));
    }

    #[test]
    fn names_put_the_model_first_and_say_who_made_it() {
        for m in CATALOG {
            assert!(!m.maker.is_empty(), "{} needs a maker", m.id);
            assert!(!m.name.contains('('), "{}: the model name comes first, languages go in the subtitle", m.id);
        }
        assert_eq!(model("indic-hi").unwrap().name, "IndicConformer Hindi");
        assert_eq!(model("parakeet-v3").unwrap().name, "Parakeet v3");
        assert_eq!(Tier::ALL.map(Tier::label), ["Lite", "Balanced", "Max"]);
        assert!(Tier::ALL.iter().all(|t| Tier::parse(t.name()) == Some(*t)), "stored ids are unchanged");
    }

    #[test]
    fn standard_is_lite_plus_cleanup_and_sizes_are_sane() {
        assert!(Tier::Standard.cleanup() && !Tier::Lite.cleanup());
        assert!(Tier::Quality.ram_mb() > Tier::Standard.ram_mb());
        assert!(Tier::Standard.download_bytes() > Tier::Lite.download_bytes());
    }

    #[test]
    fn standard_tier_fits_in_one_gigabyte_with_the_agent() {
        // the agent itself is about 30-40 MB; the worker (voice detection + speech + cleanup) must leave room for it
        assert!(Tier::Standard.ram_mb() + 60 < 1024, "Standard needs {} MB", Tier::Standard.ram_mb());
        assert!(Tier::Lite.ram_mb() < Tier::Standard.ram_mb());
    }

    #[test]
    fn library_entries_are_clean_and_well_formed() {
        let mut ids = std::collections::HashSet::new();
        for m in CATALOG {
            assert!(ids.insert(m.id), "duplicate id {}", m.id);
            let l = m.license.to_lowercase();
            assert!(!["nc", "non-commercial", "research", "llama", "gemma", "lfm", "openrail"].iter().any(|b| l.contains(b)), "{} licence {}", m.id, m.license);
            assert!(m.ram_mb > 0 && m.download_bytes() > 0 && m.installed_bytes() > 0, "{}", m.id);
            assert!(m.files().iter().all(|f| f.size > 0 && f.sha256.len() == 64), "{}", m.id);
            let urls: Vec<&str> = match &m.source {
                Source::Files(f) => f.iter().map(|f| f.url).collect(),
                Source::Archive { url, .. } => vec![*url],
            };
            assert!(urls.iter().all(|u| u.starts_with("https://")), "{} needs https urls", m.id);
        }
        for m in library().filter(|m| m.experimental) {
            assert!(m.note.starts_with("Experimental"), "{} must say so in its note", m.id);
            assert!(!Tier::ALL.iter().any(|t| t.model_ids().contains(&m.id)));
        }
        assert!(library().any(|m| m.experimental));
    }
}
