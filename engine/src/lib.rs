//! Voicio on-device speech engine: Silero VAD -> offline ASR -> speaker diarization (sherpa-onnx).
//!
//! RAM discipline: one worker thread owns every model call so inference never overlaps, audio is
//! held as i16 and freed at finalize, diarization runs once after stop, and glibc is asked to
//! return freed heap after loads and sessions. No I/O, no network, no audio persistence.

pub mod cleanup;
pub mod gguf;
pub mod grammar;
pub mod style;
pub mod denoise;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod ort_arena;

use sherpa_onnx::{
    FastClusteringConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineSpeakerDiarization,
    OfflineSpeakerDiarizationConfig, OfflineSpeakerSegmentationModelConfig,
    OfflineSpeakerSegmentationPyannoteModelConfig, OfflineTransducerModelConfig,
    SileroVadModelConfig, SpeakerEmbeddingExtractorConfig, VadModelConfig, VoiceActivityDetector,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const SAMPLE_RATE: usize = 16_000;
const VAD_WINDOW: usize = 512;
const PREROLL: usize = SAMPLE_RATE * 3 / 10; // 0.3 s kept before each VAD start so first phonemes survive
const POSTROLL: usize = SAMPLE_RATE * 15 / 100; // the VAD trims its silence window; keep the word's tail
const LOOKBACK: usize = SAMPLE_RATE / 2; // VAD reports speech ~0.25-0.5 s after it begins
const MIN_PARTIAL: usize = SAMPLE_RATE / 5;
const PARTIAL_EVERY: Duration = Duration::from_millis(250);
// ponytail: 30 min cap keeps session audio <= 58 MB as i16; spill to a temp file for longer sessions.
pub const MAX_SESSION_SAMPLES: usize = SAMPLE_RATE * 60 * 30;

pub const ENGLISH_ASR: &str = "sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8";
pub const ENGLISH_LABEL: &str = "Parakeet TDT 110M int8 (English)";

pub struct EngineConfig {
    pub models_dir: PathBuf,
    /// Directory (under `models_dir`) of a sherpa-onnx NeMo transducer, e.g. a Parakeet export.
    pub asr: String,
    /// Whisper only: ISO code such as "hi"; empty = auto-detect.
    pub language: String,
    pub threads: i32,
    pub provider: String,
    /// Speaker labels after stop (meeting mode). Off for dictation: saves ~110 MB and ~2 s.
    pub diarize: bool,
    /// Rewrite the finished transcript with the cleanup LLM (dictation).
    pub cleanup: bool,
    /// Suppress steady background noise before the VAD and recogniser (see `denoise`).
    pub denoise: bool,
    pub llm_threads: i32,
    /// A partial decode may start only after `partial_factor` x the previous decode's duration (CPU duty cap).
    pub partial_factor: u32,
    /// VAD force-splits speech longer than this. ONNX Runtime's arena grows to fit the longest
    /// decoded chunk and never shrinks, so this bounds ASR memory.
    pub max_utterance_secs: f32,
    /// The cleanup LLM is only loaded if the process stays under this RSS afterwards.
    pub memory_budget_mb: f64,
}

impl EngineConfig {
    pub fn new(models_dir: PathBuf) -> Self {
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        Self {
            models_dir,
            asr: ENGLISH_ASR.into(),
            language: String::new(),
            // Measured: ASR is fastest at 2-4 threads (8 was slower on a hybrid P/E CPU) ...
            threads: cores.clamp(1, 4) as i32,
            provider: "cpu".into(),
            diarize: true,
            cleanup: false,
            denoise: true,
            // ... while LLM token generation kept improving up to 8 (844 -> 449 ms per line).
            llm_threads: (cores / 2).clamp(2, 8) as i32,
            partial_factor: 1,
            max_utterance_secs: 12.0, // measured: same WER as 20 s on LibriSpeech, 30 MB less; 8 s loses accuracy
            memory_budget_mb: 1024.0,
        }
        .with_cpu_mode("balanced")
    }

    /// "fast" (the old defaults), "balanced" (default) or "low". Measured on a 16-thread CPU: ORT and llama.cpp
    /// worker threads spin-wait, so fewer threads and a duty cap on partial decodes cut CPU far more than latency.
    pub fn with_cpu_mode(mut self, mode: &str) -> Self {
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        let (asr, llm, factor) = match mode {
            "fast" => (cores.clamp(1, 4), (cores / 2).clamp(2, 8), 1),
            "low" => (1, (cores / 8).clamp(2, 3), 6),
            _ => (2, (cores / 4).clamp(2, 4), 3),
        };
        self.threads = asr as i32;
        self.llm_threads = llm as i32;
        self.partial_factor = factor;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Segment {
        id: String,
        text: String,
        start_ms: u64,
        end_ms: u64,
        is_final: bool,
    },
    Speakers(Vec<(String, String)>),
    /// Session finished: the full transcript, cleaned by the LLM when cleanup is on.
    Done(String),
    Error(String),
}

/// Live counters the UI polls; written by the worker.
#[derive(Default)]
pub struct Stats {
    pub decode_ms: AtomicU64,
    pub rtf_milli: AtomicU64,
}

struct Decode {
    id: String,
    start: usize,
    samples: Vec<f32>,
    is_final: bool,
    /// Clean this utterance right after decoding it (while the user is still talking).
    cleanup: bool,
    /// The session's base style; a spoken directive in the first utterance can change it.
    style: style::CleanupStyle,
    events: Sender<Event>,
    cancelled: Arc<AtomicBool>,
}

enum Job {
    Final(Decode),
    /// Wake-up for the coalescing partial slot; the slot may already be empty.
    Partial,
    Finish {
        audio: Option<Vec<f32>>,
        candidates: Vec<(String, u64, u64)>,
        cleanup: bool,
        style: style::CleanupStyle,
        events: Sender<Event>,
        cancelled: Arc<AtomicBool>,
    },
    /// Load the cleanup LLM if it is not resident (runs while the user is still speaking).
    LoadCleanup(Sender<Event>),
    /// Drop optional models (memory pressure).
    Trim,
    /// Clean a finished transcript from elsewhere (a cloud provider) exactly like a dictation's.
    Clean {
        text: String,
        cleanup: bool,
        style: style::CleanupStyle,
        reply: Sender<String>,
    },
}

/// Measured resident cost of S1-mini with the duplicate output table removed and memory-mapped,
/// no-repack weights: about 390 MB (it was 560 MB loaded the old way). See cleanup.rs.
const CLEANUP_RSS_MB: f64 = 420.0;

struct CleanupSource {
    models_dir: PathBuf,
    threads: i32,
    budget_mb: f64,
}

pub struct Engine {
    jobs: Sender<Job>,
    partial: Arc<Mutex<Option<Decode>>>,
    vad_config: VadModelConfig,
    diarize: bool,
    cleanup: bool,
    denoise: bool,
    partial_factor: u32,
    pub stats: Arc<Stats>,
    pub label: String,
    pub load_rss_mb: f64,
}

/// Which sherpa-onnx offline model family a model folder holds, detected from its files.
#[derive(Debug, PartialEq)]
pub enum AsrKind {
    Transducer,
    NemoCtc,
    /// File-name prefix, e.g. "small" for small-encoder.int8.onnx.
    Whisper(String),
}

impl AsrKind {
    pub fn detect(dir: &Path) -> Result<AsrKind, String> {
        let has = |f: &str| dir.join(f).is_file();
        if has("encoder.int8.onnx") && has("joiner.int8.onnx") {
            return Ok(AsrKind::Transducer);
        }
        if has("model.int8.onnx") {
            return Ok(AsrKind::NemoCtc);
        }
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                if let Some(p) = e.file_name().to_string_lossy().strip_suffix("-encoder.int8.onnx") {
                    return Ok(AsrKind::Whisper(p.to_string()));
                }
            }
        }
        Err(format!("{} holds no supported speech model (missing files; run the download again)", dir.display()))
    }

    /// Resident estimate: weights on disk plus ONNX Runtime working memory (errs high so a too-big
    /// pack is refused instead of swapping). Measured numbers: docs/research/engine-model-support.md.
    pub fn resident_estimate_mb(&self, dir: &Path) -> f64 {
        let bytes: u64 = std::fs::read_dir(dir)
            .map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
            .unwrap_or(0);
        let factor = if matches!(self, AsrKind::Whisper(_)) { 2.7 } else { 1.65 };
        bytes as f64 / 1_048_576.0 * factor + 60.0 // + VAD, denoiser, buffers
    }
}

fn model_file(dir: &Path, relative: &str) -> Result<String, String> {
    let path = dir.join(relative);
    if path.is_file() {
        Ok(path.to_string_lossy().into_owned())
    } else {
        Err(format!(
            "missing model file {} (run engine/scripts/fetch-models.sh)",
            path.display()
        ))
    }
}

impl Engine {
    /// Loads every model, largest first, trimming the heap after each so load buffers never stack.
    pub fn load(config: &EngineConfig) -> Result<Engine, String> {
        let dir = &config.models_dir;
        let provider = Some(config.provider.clone());
        let cleaner = if config.cleanup {
            let cleaner = cleanup::Cleaner::load(dir, config.llm_threads)?;
            release_memory();
            Some(cleaner)
        } else {
            None
        };

        let name = &config.asr;
        let mut asr = OfflineRecognizerConfig::default();
        let kind = AsrKind::detect(&dir.join(name))?;
        let need = kind.resident_estimate_mb(&dir.join(name));
        if need > config.memory_budget_mb {
            return Err(format!(
                "the speech pack {name} needs about {need:.0} MB but the memory limit is {:.0} MB; pick a smaller model or raise the limit",
                config.memory_budget_mb
            ));
        }
        match kind {
            AsrKind::Transducer => {
                asr.model_config.transducer = OfflineTransducerModelConfig {
                    encoder: Some(model_file(dir, &format!("{name}/encoder.int8.onnx"))?),
                    decoder: Some(model_file(dir, &format!("{name}/decoder.int8.onnx"))?),
                    joiner: Some(model_file(dir, &format!("{name}/joiner.int8.onnx"))?),
                };
                asr.model_config.tokens = Some(model_file(dir, &format!("{name}/tokens.txt"))?);
                asr.model_config.model_type = Some("nemo_transducer".into());
            }
            AsrKind::NemoCtc => {
                asr.model_config.nemo_ctc.model = Some(model_file(dir, &format!("{name}/model.int8.onnx"))?);
                asr.model_config.tokens = Some(model_file(dir, &format!("{name}/tokens.txt"))?);
                asr.model_config.model_type = Some("nemo_ctc".into());
            }
            AsrKind::Whisper(ref prefix) => {
                asr.model_config.whisper.encoder = Some(model_file(dir, &format!("{name}/{prefix}-encoder.int8.onnx"))?);
                asr.model_config.whisper.decoder = Some(model_file(dir, &format!("{name}/{prefix}-decoder.int8.onnx"))?);
                asr.model_config.whisper.language = Some(config.language.clone()); // "" = auto-detect
                asr.model_config.whisper.task = Some("transcribe".into());
                asr.model_config.tokens = Some(model_file(dir, &format!("{name}/{prefix}-tokens.txt"))?);
                asr.model_config.model_type = Some("whisper".into());
            }
        }
        asr.model_config.num_threads = config.threads;
        asr.model_config.provider = provider.clone();
        let recognizer = OfflineRecognizer::create(&asr).ok_or_else(|| format!("could not load the speech model {name}"))?;
        release_memory();

        let diarizer = if !config.diarize {
            None
        } else {
            Some(
                OfflineSpeakerDiarization::create(&OfflineSpeakerDiarizationConfig {
                    segmentation: OfflineSpeakerSegmentationModelConfig {
                        pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                            model: Some(model_file(
                                dir,
                                "sherpa-onnx-pyannote-segmentation-3-0/model.int8.onnx",
                            )?),
                            ..Default::default()
                        },
                        num_threads: config.threads,
                        provider: provider.clone(),
                        ..Default::default()
                    },
                    embedding: SpeakerEmbeddingExtractorConfig {
                        model: Some(model_file(
                            dir,
                            "3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx",
                        )?),
                        num_threads: config.threads,
                        provider: provider.clone(),
                        ..Default::default()
                    },
                    clustering: FastClusteringConfig {
                        num_clusters: -1,
                        threshold: 0.5,
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .ok_or("could not load the speaker diarization models")?,
            )
        };
        release_memory();

        let vad_config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(model_file(dir, "silero_vad.onnx")?),
                threshold: 0.5,
                min_silence_duration: 0.8, // measured: 0.5 s raised WER and CPU (more, shorter utterances); fewer mid-sentence cuts than 0.5 s
                min_speech_duration: 0.12, // one-word replies ("yes", "okay") must survive
                window_size: VAD_WINDOW as i32,
                max_speech_duration: config.max_utterance_secs,
            },
            sample_rate: SAMPLE_RATE as i32,
            num_threads: 1,
            provider,
            ..Default::default()
        };
        // Fail now, not on the first session, if the VAD model is unusable.
        VoiceActivityDetector::create(&vad_config, 1.0).ok_or("could not load the VAD model")?;

        let (jobs, rx) = mpsc::channel();
        let partial = Arc::new(Mutex::new(None));
        let stats = Arc::new(Stats::default());
        {
            let (partial, stats) = (partial.clone(), stats.clone());
            let source = CleanupSource {
                models_dir: dir.clone(),
                threads: config.llm_threads,
                budget_mb: config.memory_budget_mb,
            };
            std::thread::Builder::new()
                .name("voicio-inference".into())
                .spawn(move || worker(rx, partial, stats, recognizer, diarizer, cleaner, source))
                .map_err(|e| e.to_string())?;
        }
        release_memory();
        let load_rss_mb = memory_stats().map_or(0.0, |m| m.0);
        let mut label = if *name == ENGLISH_ASR {
            ENGLISH_LABEL.to_string()
        } else {
            name.clone()
        };
        if config.cleanup {
            label += &format!(" + {}", cleanup::LABEL);
        }
        Ok(Engine {
            jobs,
            partial,
            vad_config,
            diarize: config.diarize,
            cleanup: config.cleanup,
            denoise: config.denoise,
            partial_factor: config.partial_factor,
            stats,
            label,
            load_rss_mb,
        })
    }

    /// A session using the engine's configured cleanup setting.
    pub fn session(&self) -> Result<(Session<'_>, Receiver<Event>), String> {
        self.session_with(self.cleanup)
    }

    /// A session; with `cleanup` the LLM is loaded (if needed) while the user speaks.
    pub fn session_with(&self, cleanup: bool) -> Result<(Session<'_>, Receiver<Event>), String> {
        self.session_opts(cleanup, self.denoise)
    }
    pub fn session_opts(&self, cleanup: bool, denoise: bool) -> Result<(Session<'_>, Receiver<Event>), String> {
        self.session_styled(cleanup, denoise, style::CleanupStyle::default())
    }

    /// A session whose cleanup writes in `style` (a spoken directive can still override it).
    pub fn session_styled(&self, cleanup: bool, denoise: bool, style: style::CleanupStyle) -> Result<(Session<'_>, Receiver<Event>), String> {
        let vad = VoiceActivityDetector::create(&self.vad_config, 30.0)
            .ok_or("could not create the VAD")?;
        let (events, rx) = mpsc::channel();
        if cleanup {
            let _ = self.jobs.send(Job::LoadCleanup(events.clone()));
        }
        Ok((
            Session {
                engine: self,
                vad,
                events,
                cancelled: Arc::new(AtomicBool::new(false)),
                audio: Vec::with_capacity(SAMPLE_RATE * 60),
                pending: Vec::with_capacity(VAD_WINDOW * 4),
                utterance: next_id(),
                speech_start: None,
                last_end: 0,
                last_partial: Instant::now(),
                candidates: Vec::new(),
                cleanup,
                style,
                denoiser: denoise.then(denoise::Denoiser::new),
                stopped: false,
                partials: true,
            },
            rx,
        ))
    }

    /// A spoken directive ("make it formal") is taken off the front of `text`, then the text is cleaned
    /// in the resulting style: the same path a recognised dictation takes. Blocks until done.
    pub fn clean_text(&self, text: String, cleanup: bool, style: style::CleanupStyle) -> String {
        let (reply, rx) = mpsc::channel();
        let fallback = text.clone();
        if self.jobs.send(Job::Clean { text, cleanup, style, reply }).is_err() {
            return fallback;
        }
        rx.recv().unwrap_or(fallback)
    }

    /// Frees optional models (the cleanup LLM) now; they reload on the next session that needs them.
    pub fn trim(&self) {
        let _ = self.jobs.send(Job::Trim);
    }
}

fn next_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("u{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn worker(
    jobs: Receiver<Job>,
    partial: Arc<Mutex<Option<Decode>>>,
    stats: Arc<Stats>,
    recognizer: OfflineRecognizer,
    diarizer: Option<OfflineSpeakerDiarization>,
    cleaner: Option<cleanup::Cleaner>,
    source: CleanupSource,
) {
    // Shared with short-lived loader threads. The lock is held while loading, so `Finish` (the only
    // job that needs the model) waits for it, while ASR decodes never touch it.
    let cleaner = Arc::new(Mutex::new(cleaner));
    let source = Arc::new(source);
    // Final texts of the running session, keyed by utterance id (one session at a time).
    let mut finals: HashMap<String, (String, Option<String>)> = HashMap::new();
    // The style in force for the running session, keyed by the session's cancel flag (unique per session).
    let mut active: Option<(usize, style::CleanupStyle)> = None;
    let decode = |job: Decode, finals: &mut HashMap<String, (String, Option<String>)>, active: &mut Option<(usize, style::CleanupStyle)>| {
        if job.cancelled.load(Ordering::Relaxed) {
            return;
        }
        let began = Instant::now();
        let stream = recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, &job.samples);
        recognizer.decode(&stream);
        let text = stream
            .get_result()
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default();
        let elapsed = began.elapsed();
        let audio_secs = job.samples.len() as f64 / SAMPLE_RATE as f64;
        stats
            .decode_ms
            .store(elapsed.as_millis() as u64, Ordering::Relaxed);
        stats.rtf_milli.store(
            (elapsed.as_secs_f64() / audio_secs.max(1e-3) * 1000.0) as u64,
            Ordering::Relaxed,
        );
        let mut text = text;
        let session_key = Arc::as_ptr(&job.cancelled) as usize;
        if job.is_final {
            if text.is_empty() {
                return;
            }
            // A spoken directive ("email mode", "make it formal") opens the dictation: take it off the
            // front of the first utterance and apply it to everything that follows.
            if active.map(|(k, _)| k) != Some(session_key) {
                let (directive, rest) = if job.style.is_voice_on() { style::extract_directive(&text) } else { (style::Directive::default(), text.clone()) };
                *active = Some((session_key, job.style.apply(&directive)));
                text = rest;
                if text.is_empty() {
                    return; // the whole utterance was an instruction
                }
            }
            finals.insert(job.id.clone(), (text.clone(), None));
        }
        let start_ms = (job.start * 1000 / SAMPLE_RATE) as u64;
        let end_ms = start_ms + (audio_secs * 1000.0) as u64;
        let _ = job.events.send(Event::Segment {
            id: job.id.clone(),
            text: text.clone(),
            start_ms,
            end_ms,
            is_final: job.is_final,
        });
        // Clean while the user keeps talking, so stopping only has the last utterance left to do.
        // try_lock: never make speech recognition wait for the model to finish loading.
        let style_now = active.map_or(job.style, |(_, s)| s);
        if job.is_final && job.cleanup && !job.cancelled.load(Ordering::Relaxed) && !style_now.needs_whole_text() {
            if let Ok(slot) = cleaner.try_lock() {
                if let Some(c) = &*slot {
                    if let Some(entry) = finals.get_mut(&job.id) {
                        entry.1 = Some(c.clean_with(&text, &style_now).text);
                    }
                }
            }
        }
    };

    for job in jobs {
        match job {
            Job::Final(job) => decode(job, &mut finals, &mut active),
            Job::Partial => {
                let job = partial.lock().unwrap_or_else(|e| e.into_inner()).take();
                if let Some(job) = job {
                    decode(job, &mut finals, &mut active);
                }
            }
            Job::LoadCleanup(events) => {
                let (cleaner, source) = (cleaner.clone(), source.clone());
                let spawned = std::thread::Builder::new().name("voicio-cleanup-load".into()).spawn(move || {
                    let mut slot = cleaner.lock().unwrap_or_else(|e| e.into_inner());
                    if slot.is_some() {
                        return;
                    }
                    let rss = memory_stats().map_or(0.0, |m| m.0);
                    if rss + CLEANUP_RSS_MB > source.budget_mb {
                        let _ = events.send(Event::Error(format!(
                            "low memory: text cleanup skipped ({rss:.0} MB in use, budget {:.0} MB)",
                            source.budget_mb
                        )));
                        return;
                    }
                    match cleanup::Cleaner::load(&source.models_dir, source.threads) {
                        Ok(loaded) => *slot = Some(loaded),
                        Err(e) => {
                            let _ = events.send(Event::Error(format!("text cleanup unavailable: {e}")));
                        }
                    }
                    release_memory();
                });
                if let Err(e) = spawned {
                    eprintln!("voicio-engine: cannot start the cleanup loader: {e}");
                }
            }
            Job::Trim => {
                *cleaner.lock().unwrap_or_else(|e| e.into_inner()) = None;
                release_memory();
            }
            Job::Clean { text, cleanup, style, reply } => {
                let (directive, rest) = if style.is_voice_on() { style::extract_directive(&text) } else { (style::Directive::default(), text) };
                let style = style.apply(&directive).resolve(&rest);
                let out = if cleanup {
                    match &*cleaner.lock().unwrap_or_else(|e| e.into_inner()) {
                        Some(model) => model.clean_with(&rest, &style).text,
                        None => cleanup::rules(&rest, &style),
                    }
                } else if style.rules_only {
                    cleanup::rules(&rest, &style)
                } else {
                    rest
                };
                let _ = reply.send(out);
            }
            Job::Finish {
                audio,
                candidates,
                cleanup,
                style,
                events,
                cancelled,
            } => {
                let key = Arc::as_ptr(&cancelled) as usize;
                let style = match active.take() {
                    Some((k, s)) if k == key => s,
                    _ => style,
                };
                let entries: Vec<(String, Option<String>)> =
                    candidates.iter().filter_map(|(id, ..)| finals.remove(id)).collect();
                finals.clear();
                if cancelled.load(Ordering::Relaxed) {
                    release_memory();
                    continue;
                }
                if let (Some(diarizer), Some(audio), false) =
                    (&diarizer, &audio, candidates.is_empty())
                {
                    match diarizer.process(audio) {
                        Some(result) => {
                            let turns: Vec<(u64, u64, i32)> = result
                                .sort_by_start_time()
                                .iter()
                                .map(|t| {
                                    (
                                        (t.start * 1000.0) as u64,
                                        (t.end * 1000.0) as u64,
                                        t.speaker,
                                    )
                                })
                                .collect();
                            let _ =
                                events.send(Event::Speakers(assign_speakers(&candidates, &turns)));
                        }
                        None => {
                            let _ = events.send(Event::Error("speaker diarization failed".into()));
                        }
                    }
                }
                drop(audio); // the only copy of the session's audio; transcript-only from here on
                let cleaner = cleaner.lock().unwrap_or_else(|e| e.into_inner());
                let ruled = cleanup || style.rules_only; // rules still run when the model is skipped
                // "Auto" structure looks at the whole dictation: an enumeration or an email is laid out as one.
                let joined = entries.iter().map(|(raw, _)| raw.as_str()).collect::<Vec<_>>().join(" ");
                let style = style.resolve(&joined);
                let text = if ruled && style.needs_whole_text() {
                    // Bullet lists and emails are laid out over the whole dictation, not per utterance.
                    match &*cleaner {
                        Some(model) if cleanup => model.clean_with(&joined, &style).text,
                        _ => cleanup::rules(&joined, &style),
                    }
                } else {
                    entries
                        .into_iter()
                        .map(|(raw, cleaned)| match (cleaned, &*cleaner) {
                            (Some(done), _) => done,
                            (None, Some(model)) if cleanup => model.clean_with(&raw, &style).text,
                            (None, _) if ruled => cleanup::rules(&raw, &style),
                            (None, _) => raw,
                        })
                        .filter(|piece| !piece.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                drop(cleaner);
                release_memory();
                let _ = events.send(Event::Done(text));
            }
        }
    }
}

/// One recording. Feed 16 kHz mono f32; segments and speaker labels arrive on the event receiver.
pub struct Session<'e> {
    engine: &'e Engine,
    vad: VoiceActivityDetector,
    events: Sender<Event>,
    cancelled: Arc<AtomicBool>,
    audio: Vec<i16>,
    pending: Vec<f32>,
    utterance: String,
    speech_start: Option<usize>,
    /// End of the previous final; the next one never starts before it (no repeated words).
    last_end: usize,
    last_partial: Instant,
    candidates: Vec<(String, u64, u64)>,
    cleanup: bool,
    style: style::CleanupStyle,
    denoiser: Option<denoise::Denoiser>,
    stopped: bool,
    partials: bool,
}

impl Session<'_> {
    /// Live partial text costs most of the ASR CPU; turn it off when nothing displays it.
    pub fn set_partials(&mut self, on: bool) {
        self.partials = on;
    }

    pub fn audio_seconds(&self) -> f64 {
        self.audio.len() as f64 / SAMPLE_RATE as f64
    }

    /// Returns `Err` once the session length cap is reached; the caller should then `stop()`.
    pub fn feed(&mut self, samples: &[f32]) -> Result<(), String> {
        if self.stopped {
            return Err("session already stopped".into());
        }
        let cleaned;
        let samples = match self.denoiser.as_mut() {
            Some(denoiser) => {
                cleaned = denoiser.process(samples);
                &cleaned[..]
            }
            None => samples,
        };
        let room = MAX_SESSION_SAMPLES - self.audio.len();
        let accepted = &samples[..samples.len().min(room)];
        for &s in accepted {
            let s = if s.is_finite() {
                s.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.audio.push((s * i16::MAX as f32) as i16);
            self.pending.push(s);
        }
        let windows = self.pending.len() / VAD_WINDOW * VAD_WINDOW;
        for window in self.pending[..windows].chunks_exact(VAD_WINDOW) {
            self.vad.accept_waveform(window);
        }
        self.pending.drain(..windows);
        self.pop_finals();

        if self.vad.detected() {
            let now = self.audio.len();
            let start = *self
                .speech_start
                .get_or_insert(now.saturating_sub(LOOKBACK));
            if self.partials && now - start >= MIN_PARTIAL && self.last_partial.elapsed() >= self.partial_gap() {
                self.last_partial = Instant::now();
                self.queue_partial(start, now);
            }
        } else {
            self.speech_start = None;
        }
        if accepted.len() < samples.len() {
            return Err("session length limit reached (30 min)".into());
        }
        Ok(())
    }

    fn pop_finals(&mut self) {
        while let Some(segment) = self.vad.front() {
            let raw_start = segment.start().max(0) as usize;
            let start = raw_start.saturating_sub(PREROLL).max(self.last_end);
            let end = (raw_start + segment.n().max(0) as usize + POSTROLL).min(self.audio.len());
            drop(segment);
            self.vad.pop();
            if start >= end {
                continue;
            }
            self.last_end = end;
            let id = std::mem::replace(&mut self.utterance, next_id());
            {
                // A final supersedes any not-yet-run partial of the same utterance.
                let mut slot = self
                    .engine
                    .partial
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if slot.as_ref().is_some_and(|p| p.id == id) {
                    *slot = None;
                }
            }
            let ms = |i: usize| (i * 1000 / SAMPLE_RATE) as u64;
            self.candidates.push((id.clone(), ms(start), ms(end)));
            let job = self.decode_job(id, start, end, true);
            let _ = self.engine.jobs.send(Job::Final(job));
            self.speech_start = None;
        }
    }

    /// At least PARTIAL_EVERY, and `partial_factor` x the last decode so the decoder never runs flat out.
    fn partial_gap(&self) -> Duration {
        let last = self.engine.stats.decode_ms.load(Ordering::Relaxed);
        PARTIAL_EVERY.max(Duration::from_millis(last * self.engine.partial_factor as u64))
    }

    fn queue_partial(&self, start: usize, end: usize) {
        let job = self.decode_job(self.utterance.clone(), start, end, false);
        let was_empty = self
            .engine
            .partial
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(job)
            .is_none();
        if was_empty {
            let _ = self.engine.jobs.send(Job::Partial);
        }
    }

    fn decode_job(&self, id: String, start: usize, end: usize, is_final: bool) -> Decode {
        Decode {
            id,
            start,
            samples: to_f32(&self.audio[start..end]),
            is_final,
            cleanup: self.cleanup && is_final,
            style: self.style,
            events: self.events.clone(),
            cancelled: self.cancelled.clone(),
        }
    }

    /// Flushes the last utterance, then diarizes once; `Event::Done` follows on the receiver.
    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        // the denoiser holds back up to one frame: hand it to the VAD before flushing
        if let Some(mut denoiser) = self.denoiser.take() {
            let tail = denoiser.finish();
            let _ = self.feed(&tail);
        }
        self.stopped = true;
        if !self.pending.is_empty() {
            self.vad.accept_waveform(&self.pending);
            self.pending = Vec::new();
        }
        self.vad.flush();
        self.pop_finals();
        *self.engine.partial.lock().unwrap_or_else(|e| e.into_inner()) = None;
        // The i16 copy is freed here; an f32 copy exists only if the diarizer needs it.
        let audio = std::mem::take(&mut self.audio);
        let audio = self.engine.diarize.then(|| to_f32(&audio));
        let _ = self.engine.jobs.send(Job::Finish {
            audio,
            candidates: std::mem::take(&mut self.candidates),
            cleanup: self.cleanup,
            style: self.style,
            events: self.events.clone(),
            cancelled: self.cancelled.clone(),
        });
    }
}

impl Drop for Session<'_> {
    /// A dropped session (e.g. the client disconnected) cancels its queued work and frees its audio.
    fn drop(&mut self) {
        if !self.stopped {
            self.cancelled.store(true, Ordering::Relaxed);
            release_memory();
        }
    }
}

fn to_f32(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|&s| s as f32 / i16::MAX as f32)
        .collect()
}

/// Gives each transcript segment the diarized speaker it overlaps most (all times in ms).
pub fn assign_speakers(
    segments: &[(String, u64, u64)],
    turns: &[(u64, u64, i32)],
) -> Vec<(String, String)> {
    segments
        .iter()
        .filter_map(|(id, start, end)| {
            let mut overlap: HashMap<i32, u64> = HashMap::new();
            for &(turn_start, turn_end, speaker) in turns {
                let shared = (*end)
                    .min(turn_end)
                    .saturating_sub((*start).max(turn_start));
                if shared > 0 {
                    *overlap.entry(speaker).or_default() += shared;
                }
            }
            let speaker = overlap
                .into_iter()
                .max_by_key(|&(speaker, ms)| (ms, -speaker))?
                .0;
            Some((id.clone(), format!("Speaker {}", speaker + 1)))
        })
        .collect()
}

/// Word error rate inputs: (edit distance in words, reference word count), after normalization.
pub fn word_errors(reference: &str, hypothesis: &str) -> (usize, usize) {
    let words = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '\'' {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .map(str::to_string)
            .collect()
    };
    let (r, h) = (words(reference), words(hypothesis));
    let mut row: Vec<usize> = (0..=h.len()).collect();
    for i in 1..=r.len() {
        let mut diagonal = row[0];
        row[0] = i;
        for j in 1..=h.len() {
            let above = row[j];
            row[j] = (diagonal + usize::from(r[i - 1] != h[j - 1]))
                .min(above + 1)
                .min(row[j - 1] + 1);
            diagonal = above;
        }
    }
    (row[h.len()], r.len())
}

/// Asks glibc to hand freed heap back to the OS. ONNX Runtime frees load-time buffers that glibc
/// otherwise keeps mapped: measured -295 MB on a 0.6B model and -52 MB on the 110M one.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn release_memory() {
    extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn release_memory() {}

/// (current RSS, peak RSS) in MB, where the OS exposes it cheaply.
pub fn memory_stats() -> Option<(f64, f64)> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| -> Option<f64> {
        let line = status.lines().find(|l| l.starts_with(name))?;
        Some(line.split_whitespace().nth(1)?.parse::<f64>().ok()? / 1024.0)
    };
    Some((field("VmRSS:")?, field("VmHWM:")?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(files: &[(&str, usize)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("voicio-pack-{}-{}", std::process::id(), next_id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, len) in files {
            std::fs::write(dir.join(name), vec![0u8; *len]).unwrap();
        }
        dir
    }

    #[test]
    fn model_family_is_detected_from_files() {
        assert_eq!(AsrKind::detect(&pack(&[("encoder.int8.onnx", 1), ("joiner.int8.onnx", 1)])), Ok(AsrKind::Transducer));
        assert_eq!(AsrKind::detect(&pack(&[("model.int8.onnx", 1)])), Ok(AsrKind::NemoCtc));
        assert_eq!(AsrKind::detect(&pack(&[("small-encoder.int8.onnx", 1)])), Ok(AsrKind::Whisper("small".into())));
        assert!(AsrKind::detect(&pack(&[("tokens.txt", 1)])).is_err());
    }

    #[test]
    fn oversized_pack_is_refused_before_loading() {
        let dir = pack(&[("model.int8.onnx", 3 << 20)]);
        let mut config = EngineConfig::new(dir.parent().unwrap().to_path_buf());
        config.asr = dir.file_name().unwrap().to_string_lossy().into_owned();
        config.memory_budget_mb = 10.0; // 3 MB of weights + 60 MB overhead
        config.diarize = false;
        let err = Engine::load(&config).err().expect("must refuse");
        assert!(err.contains("memory limit"), "{err}");
    }

    #[test]
    fn speakers_follow_the_largest_overlap() {
        let segments = vec![
            ("a".to_string(), 0, 1000),
            ("b".to_string(), 1000, 3000),
            ("c".to_string(), 5000, 6000),
        ];
        let turns = [(0, 1200, 0), (1200, 3000, 1)];
        assert_eq!(
            assign_speakers(&segments, &turns),
            vec![
                ("a".into(), "Speaker 1".into()),
                ("b".into(), "Speaker 2".into())
            ]
        );
    }

    #[test]
    fn word_errors_ignore_case_and_punctuation() {
        assert_eq!(word_errors("Hello, world!", "hello world"), (0, 2));
        assert_eq!(word_errors("the cat sat", "the bat sat down"), (2, 3));
        assert_eq!(word_errors("", "noise"), (1, 0));
    }
}
