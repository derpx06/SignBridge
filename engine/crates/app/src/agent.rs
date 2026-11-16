//! The control loop: one thread owns all dictation state, so there are no locks to get wrong.
//!
//! Inputs arrive as `Msg`s (commands from D-Bus / the control socket / hotkeys, microphone audio,
//! worker events) plus a 250 ms tick that runs the memory policy. The model worker is spawned on
//! the first dictation and ended when idle or under memory pressure; audio captured while it
//! loads is buffered, so nothing said during a cold start is lost.

use crate::config::Config;
use crate::frontend::Frontend;
use crate::memguard;
use crate::memory::{own_rss_mb, Monitor};
use crate::policy::{self, Action};
use crate::pill::model::NoticeKind;
use crate::trigger::{Action as TriggerAction, Dictation, Mode, Trigger};
use crate::worker::{self, Worker};
use crate::audio::{self, Capture};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use voicio_ipc::{FromWorker, ToWorker};

pub enum Cmd {
    Start,
    Stop,
    Toggle,
    Cancel,
    OpenSettings,
    /// Re-run the health check (a permission was just answered).
    Recheck,
    ReloadConfig(Config),
    Status(Sender<Status>),
    Quit,
}

pub enum Msg {
    Command(Cmd),
    /// Global shortcut pressed (true) or released (false).
    Hotkey(bool),
    Audio(u64, Vec<f32>),
    Worker(u64, FromWorker),
    WorkerExited(u64),
    /// The optional cloud transcript of dictation `session` (or why it failed: the local text is kept).
    Cloud(u64, Result<String, String>),
    /// Live cloud transcription of dictation `session`: the words so far.
    CloudLive(u64, String),
    /// Live cloud transcription of dictation `session` ended: its text, or why it failed.
    CloudLiveDone(u64, Result<String, String>),
}

pub struct Status {
    pub state: &'static str,
    pub worker_loaded: bool,
    pub rss_mb: f64,
    pub peak_mb: f64,
    pub model: String,
    /// The key and mode bound right now (None while the desktop has not accepted one).
    pub shortcut: Option<(String, String)>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Idle,
    Listening,
    Finishing,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Listening => "listening",
            Phase::Finishing => "finishing",
        }
    }
}

/// What has been said so far: finished utterances plus the one still being spoken.
#[derive(Default)]
struct Preview {
    finals: Vec<(String, String)>,
    partial: Option<(String, String)>,
}

impl Preview {
    fn update(&mut self, id: String, text: String, is_final: bool) {
        if is_final {
            if self.partial.as_ref().is_some_and(|(p, _)| *p == id) {
                self.partial = None;
            }
            self.finals.push((id, text));
        } else if !self.finals.iter().any(|(f, _)| *f == id) {
            self.partial = Some((id, text));
        }
    }

    fn text(&self) -> String {
        let parts = self.finals.iter().map(|(_, t)| t.as_str()).chain(self.partial.as_ref().map(|(_, t)| t.as_str()));
        parts.filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")
    }
}

pub struct Options {
    pub input_wav: Option<PathBuf>,
}

pub struct Agent {
    config: Config,
    options: Options,
    tx: Sender<Msg>,
    frontend: Frontend,
    worker: Option<Worker>,
    generation: u64,
    expected_exit: Option<u64>,
    crashes: Vec<Instant>,
    phase: Phase,
    capture: Option<Capture>,
    capture_generation: u64,
    capture_rate: u32,
    /// Audio captured before the worker was ready (bounded to 60 s, oldest dropped first).
    pending: VecDeque<Vec<f32>>,
    pending_samples: usize,
    started_on_worker: bool,
    stop_requested: bool,
    trim_after_dictation: bool,
    preview: Preview,
    last_preview: String,
    last_used: Instant,
    monitor: Monitor,
    pressure: bool,
    /// Three-level low-memory guard (see memguard.rs), refreshed with `pressure` every 2 s.
    guard: memguard::Pressure,
    /// What the guard says about the dictation being started (see `start`).
    guard_skips_cleanup: bool,
    /// Cloud transcription is on for this dictation: keep the audio so it can be sent after release.
    record_for_cloud: bool,
    audio_log: Vec<f32>,
    /// The local transcript waiting for the cloud's answer: (session, local text, the cloud text is
    /// now being cleaned by the worker).
    cloud_job: Option<(u64, String, bool)>,
    /// No on-device speech model: this dictation is transcribed only by the cloud (`cloud_mode`).
    cloud_only: bool,
    /// Set when the dictation is cancelled or replaced: stops a cloud poll early.
    cloud_cancel: Arc<AtomicBool>,
    /// Live cloud dictation: 16 kHz audio goes to the streaming thread (dropping it = the user stopped).
    live_audio: Option<std::sync::mpsc::SyncSender<Vec<f32>>>,
    /// The streaming thread of this session has not reported its end yet.
    live_running: bool,
    /// Its result, when it arrives before the on-device text.
    live_result: Option<Result<String, String>>,
    /// Consecutive 2 s samples under pressure; one spike must not unload the models.
    pressure_samples: u32,
    session: u64,
    last_memory_check: Instant,
    model: String,
    trigger: Trigger,
    /// Per-dictation diagnostics and watchdog clocks.
    stats: CaptureStats,
    phase_since: Instant,
    last_audio: Instant,
    last_level: Instant,
    /// When a toggle last arrived (accepted or not): a held shortcut key repeats `voicio toggle`.
    last_toggle: Option<Instant>,
    /// Loudness overall and per band since the last level update (feeds the pill's lattice).
    splitter: crate::pill::bands::Splitter,
}

#[derive(Default)]
struct CaptureStats {
    samples: usize,
    peak: f32,
    sum_sq: f64,
}

impl CaptureStats {
    fn add(&mut self, chunk: &[f32]) {
        self.samples += chunk.len();
        for &s in chunk {
            self.peak = self.peak.max(s.abs());
            self.sum_sq += (s as f64) * (s as f64);
        }
    }

    fn rms_db(&self) -> f64 {
        20.0 * (self.sum_sq / self.samples.max(1) as f64).sqrt().max(1e-9).log10()
    }

    fn peak_db(&self) -> f64 {
        20.0 * (self.peak as f64).max(1e-9).log10()
    }
}

/// No audio callback for this long while listening = the microphone is not delivering.
const MIC_SILENT_AFTER: Duration = Duration::from_secs(3);
/// A held key that never reports its release (focus lost, screen locked) must not record forever.
const MAX_HOLD: Duration = Duration::from_secs(120);
/// The worker has this long to load models and say Ready.
const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// Peak below this (about -50 dBFS) means the mic is effectively muted or far too quiet.
const QUIET_PEAK_DB: f64 = -50.0;
/// Toggles closer together than this are one key press repeating, not two presses.
const TOGGLE_DEBOUNCE: Duration = Duration::from_millis(600);

/// Average level above which an empty result means the input is saturated with noise, not silent.
const LOUD_RMS_DB: f64 = -22.0;

/// Held longer than this = push-to-talk (release finishes); shorter = tap to toggle.

impl Agent {
    pub fn new(config: Config, options: Options, tx: Sender<Msg>, frontend: Frontend) -> Agent {
        let trigger = Trigger::new(Mode::parse(&config.shortcut_mode));
        Agent {
            config,
            options,
            tx,
            frontend,
            worker: None,
            generation: 0,
            expected_exit: None,
            crashes: Vec::new(),
            phase: Phase::Idle,
            capture: None,
            capture_generation: 0,
            capture_rate: 16_000,
            pending: VecDeque::new(),
            pending_samples: 0,
            started_on_worker: false,
            stop_requested: false,
            trim_after_dictation: false,
            preview: Preview::default(),
            last_preview: String::new(),
            last_used: Instant::now(),
            monitor: Monitor::new(),
            pressure: false,
            guard: memguard::Pressure::Ok,
            guard_skips_cleanup: false,
            record_for_cloud: false,
            audio_log: Vec::new(),
            cloud_job: None,
            cloud_only: false,
            cloud_cancel: Arc::new(AtomicBool::new(false)),
            live_audio: None,
            live_running: false,
            live_result: None,
            pressure_samples: 0,
            session: 0,
            last_memory_check: Instant::now().checked_sub(Duration::from_secs(60)).unwrap_or_else(Instant::now),
            model: "not loaded".into(),
            trigger,
            stats: CaptureStats::default(),
            phase_since: Instant::now(),
            last_audio: Instant::now(),
            last_level: Instant::now(),
            last_toggle: None,
            splitter: crate::pill::bands::Splitter::new(48_000),
        }
    }

    pub fn run(mut self, rx: Receiver<Msg>) {
        // First run (or a deleted model): open the app window on the Models page.
        self.frontend.health(&self.config);
        if self.options.input_wav.is_none() && !self.config.onboarded {
            self.frontend.open_onboarding(&self.config);
        } else if self.options.input_wav.is_none() && !voicio_models::can_dictate(&self.config.models_dir(), self.config.tier()) {
            self.frontend.open_models(&self.config);
        }
        self.frontend.state(self.phase.name());
        loop {
            let wait = self
                .trigger
                .deadline()
                .map_or(Duration::from_millis(250), |t| t.saturating_duration_since(Instant::now()).min(Duration::from_millis(250)));
            match rx.recv_timeout(wait) {
                Ok(Msg::Command(Cmd::Quit)) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Msg::Command(cmd)) => self.command(cmd),
                // With the GNOME extension loaded it owns the shortcut; the portal binding stays quiet.
                Ok(Msg::Hotkey(pressed)) if !self.frontend.extension_owns_shortcut() => self.hotkey(pressed),
                Ok(Msg::Hotkey(_)) => {}
                Ok(Msg::Audio(generation, chunk)) => self.audio(generation, chunk),
                Ok(Msg::Worker(generation, message)) => self.worker_message(generation, message),
                Ok(Msg::WorkerExited(generation)) => self.worker_exited(generation),
                Ok(Msg::Cloud(session, result)) => self.cloud_done(session, result),
                Ok(Msg::CloudLive(session, text)) => {
                    if session == self.session && self.phase != Phase::Idle && self.config.live_text && text != self.last_preview {
                        self.frontend.preview(&text);
                        self.last_preview = text;
                    }
                }
                Ok(Msg::CloudLiveDone(session, result)) => self.live_done(session, result),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.trigger_due();
            self.tick();
        }
        self.cancel();
        if let Some(worker) = self.worker.take() {
            worker.shutdown();
        }
    }

    fn command(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Start => self.start(),
            Cmd::Stop => self.stop(),
            Cmd::Toggle => {
                // A key held down repeats the command many times a second. Anything arriving within
                // 600 ms of the previous toggle is that repeat, not a new press: ignore it (and keep
                // ignoring until the key is let go).
                let repeat = self.last_toggle.is_some_and(|t| t.elapsed() < TOGGLE_DEBOUNCE);
                self.last_toggle = Some(Instant::now());
                if !repeat {
                    match self.phase {
                        Phase::Idle => self.start(),
                        Phase::Listening => self.stop(),
                        Phase::Finishing => {}
                    }
                }
            }
            Cmd::Cancel => self.cancel(),
            Cmd::Recheck => self.frontend.health(&self.config),
            Cmd::OpenSettings => {
                self.frontend.health(&self.config);
                self.frontend.open_settings(&self.config)
            }
            Cmd::ReloadConfig(config) => {
                self.trigger.set_mode(Mode::parse(&config.shortcut_mode));
                // A different pack or language model: the next dictation loads it (never mid-dictation).
                let model_changed = (config.tier(), &config.speech_pack, &config.language, &config.cpu_mode)
                    != (self.config.tier(), &self.config.speech_pack, &self.config.language, &self.config.cpu_mode);
                if model_changed && self.phase == Phase::Idle {
                    self.unload("the speech model was changed");
                }
                self.config = config;
            }
            Cmd::Status(reply) => {
                let (own_rss, own_peak) = own_rss_mb();
                let (worker_rss, worker_peak) = self.worker.as_ref().map_or((0.0, 0.0), |w| (w.rss_mb, w.peak_mb));
                let _ = reply.send(Status {
                    state: self.phase.name(),
                    worker_loaded: self.worker.as_ref().is_some_and(|w| w.ready),
                    rss_mb: own_rss + worker_rss,
                    peak_mb: own_peak + worker_peak,
                    model: self.model.clone(),
                    shortcut: self.frontend.bound_shortcut(&self.config),
                });
            }
            Cmd::Quit => {}
        }
    }

    fn hotkey(&mut self, pressed: bool) {
        let now = Instant::now();
        if pressed {
            let dictation = match self.phase {
                Phase::Idle => Dictation::Idle,
                Phase::Listening => Dictation::Listening,
                Phase::Finishing => Dictation::Busy,
            };
            let action = self.trigger.press(now, dictation);
            self.act(action);
        } else {
            self.trigger.release(now);
        }
    }

    /// Acts on a release whose grace period has ended (push-to-talk).
    fn trigger_due(&mut self) {
        let action = self.trigger.due(Instant::now());
        self.act(action);
    }

    fn act(&mut self, action: TriggerAction) {
        match action {
            TriggerAction::Start => self.start(),
            TriggerAction::Stop => self.stop(),
            TriggerAction::Discard => {
                self.cancel();
                self.frontend.notice(NoticeKind::Warn, "Hold the key while you speak");
            }
            TriggerAction::Busy => self.frontend.notice(NoticeKind::Warn, "One moment…"),
            TriggerAction::None => {}
        }
    }

    /// The user's dictionary applied to a finished dictation.
    fn polish(&self, text: &str) -> String {
        if self.config.replacements.is_empty() {
            return text.to_string();
        }
        crate::replacements::ReplacementSet::new(&self.config.replacements).replace(text)
    }

    fn set_phase(&mut self, phase: Phase) {
        if self.phase != phase {
            self.phase = phase;
            self.phase_since = Instant::now();
            self.frontend.state(phase.name());
        }
    }

    fn start(&mut self) {
        if self.phase != Phase::Idle {
            return;
        }
        if !policy::may_start(&self.crashes, Instant::now()) {
            self.frontend.problem(NoticeKind::Error, "Speech engine keeps crashing", "The speech engine keeps crashing; try again in a minute.");
            return;
        }
        if !voicio_models::can_dictate(&self.config.models_dir(), self.config.tier()) {
            if self.cloud_key().is_some() {
                return self.start_cloud_only(); // "fallback" (or "always") cloud mode covers a missing pack
            }
            self.frontend.problem(NoticeKind::Error, "Download a speech model", "Speech model not installed. Opening Voicio so you can download it.");
            self.frontend.open_models(&self.config);
            return;
        }
        // Low-memory guard (docs/research/low-memory.md): never make the computer thrash.
        match memguard::advice(self.guard, self.config.tier()) {
            memguard::Action::Proceed => self.guard_skips_cleanup = false,
            memguard::Action::SkipCleanupModel => {
                eprintln!("voicio: low memory, using rule-based clean-up for this dictation");
                self.frontend.notice(NoticeKind::Warn, "Low memory: basic clean-up this time");
                self.guard_skips_cleanup = true;
            }
            memguard::Action::UnloadAfterUse => {
                self.guard_skips_cleanup = false;
                self.trim_after_dictation = true; // free the models as soon as this dictation is typed
            }
            memguard::Action::RefuseWithNotice(text) => {
                self.frontend.problem(NoticeKind::Warn, text, "Low memory: Voicio did not start so the computer stays responsive. Close some applications and try again.");
                return;
            }
        }
        self.capture_generation += 1;
        self.session += 1;
        match audio::open(self.options.input_wav.as_deref(), self.capture_generation, self.tx.clone()) {
            Ok((capture, rate)) => {
                self.capture = Some(capture);
                self.capture_rate = rate;
                self.frontend.warming(true); // cleared as soon as the speech engine is ready
            }
            Err(e) => {
                self.frontend.problem(NoticeKind::Error, "Can't open the microphone", &e);
                return;
            }
        }
        if let Err(e) = self.ensure_worker() {
            self.capture = None;
            self.frontend.error(&e);
            return;
        }
        self.preview = Preview::default();
        self.audio_log.clear();
        self.cloud_job = None;
        self.cloud_only = false;
        self.record_for_cloud = self.config.cloud_mode == "always" && self.cloud_key().is_some();
        self.stats = CaptureStats::default();
        self.last_audio = Instant::now();
        self.last_preview.clear();
        self.pending.clear();
        self.pending_samples = 0;
        self.started_on_worker = false;
        self.stop_requested = false;
        self.set_phase(Phase::Listening);
        self.begin_on_worker();
        self.start_live();
    }

    /// The optional speech pack in use: the one chosen in the library, or Hindi when the language is
    /// Hindi. Only packs that are installed and that the engine can run count.
    fn speech_pack(&self) -> Option<&'static voicio_models::Model> {
        let id = match (self.config.speech_pack.as_str(), self.config.language.as_str()) {
            ("", "hi") => "indic-hi",
            ("", _) => return None,
            (id, _) => id,
        };
        voicio_models::model(id).filter(|m| m.optional && m.engine_ready && voicio_models::status(&self.config.models_dir(), m) == voicio_models::Status::Installed)
    }

    fn ensure_worker(&mut self) -> Result<(), String> {
        if self.worker.is_some() {
            return Ok(());
        }
        self.generation += 1;
        let binary = worker::locate(self.config.worker_path.as_deref());
        let tier = self.config.tier();
        // The big model plus the cleanup model need more room than the 1 GB the small tiers keep to.
        // The 1 GB promise covers the whole app, so the worker gets what is left after the agent itself.
        let budget = if tier == voicio_models::Tier::Quality { 3000 } else { 1024u32.saturating_sub(crate::memory::own_rss_mb().0.ceil() as u32 + 24) };
        // An installed optional pack (e.g. Hindi) replaces the tier's English speech model.
        let pack = self.speech_pack();
        let (asr_dir, budget, language) = match pack {
            Some(m) => (m.files()[0].path.rsplit_once('/').map_or(m.files()[0].path, |(d, _)| d), budget.max(m.ram_mb + 100), if self.config.language == "auto" { "" } else { self.config.language.as_str() }),
            None => (voicio_models::speech_dir(tier), budget, ""),
        };
        let worker = Worker::spawn(&binary, &self.config.models_dir(), asr_dir, budget, &self.config.cpu_mode, language, self.generation, self.tx.clone())
            .map_err(|e| e.to_string())?;
        self.worker = Some(worker);
        Ok(())
    }

    /// Once the worker is ready: start its session, replay buffered audio, and pass on a Stop
    /// that arrived while it was still loading.
    fn begin_on_worker(&mut self) {
        if self.phase == Phase::Idle || self.started_on_worker {
            return;
        }
        self.frontend.warming(false);
        let (cleanup, style) = (self.cleanup_on(), self.style_line());
        let rate = self.capture_rate;
        let Some(worker) = self.worker.as_mut().filter(|w| w.ready) else { return };
        let mut ok = worker.send(&ToWorker::Start { cleanup, sample_rate: rate, session: self.session, denoise: self.config.noise_reduction, partials: self.config.live_text, style });
        for chunk in self.pending.drain(..) {
            ok &= worker.audio(&chunk);
        }
        self.pending_samples = 0;
        if self.stop_requested {
            ok &= worker.send(&ToWorker::Stop);
        }
        self.started_on_worker = ok;
    }

    /// Whether this dictation is polished by the cleanup model (or only by rules).
    fn cleanup_on(&self) -> bool {
        self.config.cleanup
            && self.config.tier().cleanup()
            && voicio_models::cleanup_installed(&self.config.models_dir())
            && !self.pressure
            && !self.guard_skips_cleanup
            && self.speech_pack().is_none_or(|m| m.languages.split(',').all(|l| l.trim().eq_ignore_ascii_case("english") || l.trim() == "en"))
    }

    /// The cleanup style as the worker reads it: `styling,structure,context[,novoice]`.
    fn style_line(&self) -> String {
        // ",rules": the model is skipped (low memory, Lite tier) but the rule clean-up must still run, never raw text
        let rules = if self.config.cleanup && !self.cleanup_on() { ",rules" } else { "" };
        format!("{},{},{}{}{}", self.config.cleanup_styling, self.config.cleanup_structure, self.config.cleanup_context, if self.config.voice_commands { "" } else { ",novoice" }, rules)
    }

    fn audio(&mut self, generation: u64, chunk: Vec<f32>) {
        if generation != self.capture_generation || self.phase != Phase::Listening {
            return; // late audio from a stopped capture
        }
        self.last_audio = Instant::now();
        self.stats.add(&chunk);
        if let Some(live) = &self.live_audio {
            if live.try_send(crate::cloud::resample_16k(&chunk, self.capture_rate)).is_err() {
                // the network cannot keep up (or the stream ended): never use a stream that missed audio
                self.live_audio = None;
                self.cloud_cancel.store(true, Ordering::Relaxed);
            }
        }
        if self.record_for_cloud {
            // ponytail: raw capture-rate audio kept for the whole dictation (<= 2 min, ~23 MB at 48 kHz); a
            // longer one is not sent to the cloud, the local result is used.
            if self.audio_log.len() + chunk.len() > self.capture_rate as usize * 120 {
                self.record_for_cloud = false;
                self.audio_log = Vec::new();
            } else {
                self.audio_log.extend_from_slice(&chunk);
            }
        }
        // The level is the RMS of everything heard since the last update (about 50 ms), not of one
        // short callback chunk, so the pill's bars do not twitch with the buffer size.
        self.splitter.feed(&chunk);
        if self.last_level.elapsed() >= Duration::from_millis(50) {
            self.last_level = Instant::now();
            let bands = self.splitter.take();
            let rms = bands.level;
            // -50 dBFS (room noise and below) .. -16 dBFS (loud speech) mapped to 0..1
            self.frontend.level(((20.0 * (rms.max(1e-6) as f64).log10() + 50.0) / 34.0).clamp(0.0, 1.0));
            self.frontend.meter(bands);
        }
        if self.cloud_only {
            return; // no worker: the whole recording is in `audio_log`
        }
        if self.started_on_worker {
            if let Some(worker) = self.worker.as_mut() {
                worker.audio(&chunk);
            }
            return;
        }
        self.pending_samples += chunk.len();
        self.pending.push_back(chunk);
        let limit = self.capture_rate as usize * 60;
        while self.pending_samples > limit {
            if let Some(old) = self.pending.pop_front() {
                self.pending_samples -= old.len();
            }
        }
    }

    fn stop(&mut self) {
        if self.phase != Phase::Listening {
            return;
        }
        self.capture = None; // the mic closes before the slow part
        self.stop_requested = true;
        self.set_phase(Phase::Finishing);
        self.live_audio = None; // the stream sends its end message and delivers the last words
        if self.cloud_only && (self.live_running || self.live_result.is_some()) {
            if let Some(r) = self.live_result.take() {
                self.live_done(self.session, r);
            }
            return;
        }
        if self.cloud_only {
            if !self.start_cloud(self.session, "") {
                self.finish(String::new());
            }
            return;
        }
        if self.started_on_worker {
            if let Some(worker) = self.worker.as_mut() {
                worker.send(&ToWorker::Stop);
            }
        }
    }

    fn cancel(&mut self) {
        if self.phase == Phase::Idle {
            return;
        }
        self.capture = None;
        if self.started_on_worker {
            if let Some(worker) = self.worker.as_mut() {
                worker.send(&ToWorker::Cancel);
            }
        }
        self.reset_dictation();
    }

    fn reset_dictation(&mut self) {
        self.frontend.warming(false);
        self.audio_log = Vec::new();
        self.cloud_job = None;
        self.cloud_only = false;
        self.cloud_cancel.store(true, Ordering::Relaxed);
        self.live_audio = None;
        self.live_running = false;
        self.live_result = None;
        self.record_for_cloud = false;
        self.pending.clear();
        self.pending_samples = 0;
        self.started_on_worker = false;
        self.stop_requested = false;
        self.last_used = Instant::now();
        self.trigger.reset();
        voicio_ipc::heap::trim();
        self.set_phase(Phase::Idle);
    }

    fn worker_message(&mut self, generation: u64, message: FromWorker) {
        if self.worker.as_ref().is_none_or(|w| w.generation != generation) {
            return; // from a worker we already replaced
        }
        match message {
            FromWorker::Ready { model, rss_mb } => {
                if let Some(worker) = self.worker.as_mut() {
                    worker.ready = true;
                    worker.rss_mb = rss_mb;
                    worker.model = model.clone();
                }
                self.model = model;
                self.frontend.models(&format!("Models loaded · {rss_mb:.0} MB"));
                self.begin_on_worker();
            }
            FromWorker::Segment { id, text, is_final, .. } => {
                if self.phase == Phase::Idle {
                    return;
                }
                self.preview.update(id, text, is_final);
                let text = self.preview.text();
                if self.live_running {
                    return; // the live cloud words are shown instead
                }
                if text != self.last_preview {
                    if self.config.live_text {
                        self.frontend.preview(&text);
                    }
                    self.last_preview = text;
                }
            }
            FromWorker::Done { text, session } => {
                if self.phase == Phase::Idle || session != self.session {
                    return; // cancelled meanwhile, or the tail of an earlier dictation
                }
                let secs = self.stats.samples as f64 / self.capture_rate.max(1) as f64;
                eprintln!(
                    "voicio: dictation {secs:.1}s audio, peak {:.0} dBFS, rms {:.0} dBFS, {} chars",
                    self.stats.peak_db(),
                    self.stats.rms_db(),
                    text.chars().count()
                );
                if matches!(self.cloud_job, Some((s, _, true)) if s == session) {
                    // the worker cleaned the cloud transcript (if that came back empty, keep the local text)
                    let local = self.cloud_job.take().map(|(_, local, _)| local).unwrap_or_default();
                    return self.finish(if text.trim().is_empty() { local } else { text });
                }
                if self.live_running || self.live_result.is_some() {
                    // live cloud: its text replaces this one (any failure keeps this one)
                    self.cloud_job = Some((session, text, false));
                    if let Some(r) = self.live_result.take() {
                        self.cloud_done(session, r);
                    }
                    return;
                }
                if !text.trim().is_empty() && self.start_cloud(session, &text) {
                    return; // the cloud's answer (or its failure) finishes this dictation
                }
                self.finish(text);
            }
            FromWorker::Stats { rss_mb, peak_mb } => {
                if let Some(worker) = self.worker.as_mut() {
                    worker.rss_mb = rss_mb;
                    worker.peak_mb = peak_mb;
                }
            }
            FromWorker::Error { message } => {
                self.frontend.problem(NoticeKind::Error, "Speech engine error", &message);
                if self.phase != Phase::Idle {
                    self.recover_from_stall(); // the session is gone; never wait for a Done that won't come
                }
            }
        }
    }

    /// Types (or explains the lack of) the final text, then returns to idle.
    fn finish(&mut self, text: String) {
        if text.trim().is_empty() {
            let (short, long) = self.nothing_heard();
            self.frontend.problem(NoticeKind::Warn, short, &long);
        } else {
            let text = self.polish(&text);
            self.frontend.done(&text);
        }
        self.reset_dictation();
        if std::mem::take(&mut self.trim_after_dictation) {
            if let Some(worker) = self.worker.as_mut() {
                worker.send(&ToWorker::Trim);
            }
        }
    }

    /// The cloud provider to use now: active in the config and with a saved key.
    fn cloud_key(&self) -> Option<&str> {
        self.config.cloud_active().filter(|id| crate::cloud::load_key(id).is_some())
    }

    /// A dictation with no on-device speech model: record, then send it all to the cloud on release.
    fn start_cloud_only(&mut self) {
        self.capture_generation += 1;
        self.session += 1;
        match audio::open(self.options.input_wav.as_deref(), self.capture_generation, self.tx.clone()) {
            Ok((capture, rate)) => {
                self.capture = Some(capture);
                self.capture_rate = rate;
            }
            Err(e) => {
                self.frontend.problem(NoticeKind::Error, "Can't open the microphone", &e);
                return;
            }
        }
        self.preview = Preview::default();
        self.audio_log.clear();
        self.cloud_job = None;
        self.cloud_only = true;
        self.record_for_cloud = true;
        self.stats = CaptureStats::default();
        self.last_audio = Instant::now();
        self.last_preview.clear();
        self.pending.clear();
        self.pending_samples = 0;
        self.started_on_worker = false;
        self.stop_requested = false;
        self.set_phase(Phase::Listening);
        self.start_live();
    }

    /// Live cloud dictation (the provider's Live toggle): audio streams to the provider while the user
    /// speaks and its words replace the on-device preview. "Always" mode keeps no recording (the on-device
    /// text is the fallback); cloud-only keeps one, so a failed stream can still be sent whole.
    fn start_live(&mut self) {
        self.live_audio = None;
        self.live_running = false;
        self.live_result = None;
        if !self.config.cloud_live_on() {
            return;
        }
        let Some(id) = self.cloud_key().map(str::to_string) else { return };
        let Some(key) = crate::cloud::load_key(&id) else { return };
        let (tx_audio, rx_audio) = std::sync::mpsc::sync_channel(64); // ~0.6-1.3 s of capture chunks
        let lang = Some(self.config.language.clone()).filter(|l| l != "auto" && !l.is_empty());
        let (model, region, session) = (self.config.cloud_model.clone(), self.config.cloud_region.clone(), self.session);
        self.cloud_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cloud_cancel.clone();
        let tx = self.tx.clone();
        self.live_audio = Some(tx_audio);
        self.live_running = true;
        if !self.cloud_only {
            self.record_for_cloud = false;
        }
        std::thread::spawn(move || {
            let tx_text = tx.clone();
            let r = crate::cloud::live(&id, &key, &model, &region, lang.as_deref(), rx_audio, &cancel, |t| {
                let _ = tx_text.send(Msg::CloudLive(session, t.to_string()));
            });
            let _ = tx.send(Msg::CloudLiveDone(session, r.map_err(|e| e.to_string())));
        });
    }

    /// The stream ended. With the on-device text already in (or cloud-only), finish like a batch answer;
    /// otherwise keep it for when the on-device text arrives.
    fn live_done(&mut self, session: u64, result: Result<String, String>) {
        if session != self.session || self.phase == Phase::Idle {
            return;
        }
        self.live_running = false;
        self.live_audio = None;
        if let Err(e) = &result {
            eprintln!("voicio: live cloud transcription failed ({e})");
        }
        if matches!(self.cloud_job, Some((s, _, false)) if s == session) {
            return self.cloud_done(session, result);
        }
        if self.cloud_only && self.phase == Phase::Finishing {
            return match result {
                Ok(text) => {
                    self.cloud_job = Some((session, String::new(), false));
                    self.cloud_done(session, Ok(text))
                }
                // the stream failed: send the kept recording whole
                Err(e) => {
                    if !self.start_cloud(session, "") {
                        self.cloud_job = Some((session, String::new(), false));
                        self.cloud_done(session, Err(e));
                    }
                }
            };
        }
        self.live_result = Some(result);
    }

    /// Optional cloud transcription: the local engine has already produced `local` (empty for a
    /// cloud-only dictation); the whole recording goes to the provider on a background thread and its
    /// text replaces the local one if it succeeds. Any failure keeps `local`, so the user never loses a
    /// dictation. False = nothing was started.
    fn start_cloud(&mut self, session: u64, local: &str) -> bool {
        let Some(id) = self.config.cloud_active().map(str::to_string) else { return false };
        if !self.record_for_cloud || self.audio_log.is_empty() {
            return false;
        }
        let pcm = crate::cloud::resample_16k(&std::mem::take(&mut self.audio_log), self.capture_rate);
        let lang = Some(self.config.language.clone()).filter(|l| l != "auto" && !l.is_empty());
        let (model, region) = (self.config.cloud_model.clone(), self.config.cloud_region.clone());
        self.cloud_job = Some((session, local.to_string(), false));
        self.cloud_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cloud_cancel.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Cloud(session, crate::cloud::transcribe_saved(&id, &model, &region, &pcm, lang.as_deref(), &cancel)));
        });
        true
    }

    /// The cloud's answer goes through the same spoken-directive and cleanup path as local speech (the
    /// worker cleans it); without a worker (cloud-only) only the dictionary applies.
    fn cloud_done(&mut self, session: u64, result: Result<String, String>) {
        if self.phase == Phase::Idle || self.cloud_job.as_ref().is_none_or(|(s, _, cleaning)| *s != session || *cleaning) {
            return; // cancelled or replaced meanwhile
        }
        let Some((_, local, _)) = self.cloud_job.take() else { return };
        match result {
            Ok(text) => {
                let (cleanup, style) = (self.cleanup_on(), self.style_line());
                if let Some(worker) = self.worker.as_mut().filter(|w| w.ready && !self.cloud_only) {
                    if worker.send(&ToWorker::Clean { text: text.clone(), session, cleanup, style }) {
                        self.cloud_job = Some((session, local, true));
                        return;
                    }
                }
                self.finish(text);
            }
            Err(e) if self.cloud_only => {
                eprintln!("voicio: cloud transcription failed ({e}); no on-device model to fall back to");
                self.frontend.problem(NoticeKind::Error, "Cloud transcription failed", &format!("{e} No on-device speech model is installed to fall back to."));
                self.reset_dictation();
            }
            Err(e) => {
                eprintln!("voicio: cloud transcription failed ({e}); using the local result");
                self.finish(local);
            }
        }
    }

    fn worker_exited(&mut self, generation: u64) {
        if self.worker.as_ref().is_none_or(|w| w.generation != generation) {
            return;
        }
        drop(self.worker.take()); // reaps the process
        if self.expected_exit.take() == Some(generation) {
            return;
        }
        self.crashes.push(Instant::now());
        self.crashes.retain(|t| t.elapsed() < Duration::from_secs(60));
        self.model = "not loaded".into();
        self.frontend.models("Speech models not loaded");
        if self.phase != Phase::Idle {
            // Never lose what was said: hand over everything recognised so far.
            self.capture = None;
            let text = self.preview.text();
            if !text.is_empty() {
                self.frontend.done(&text);
            }
            self.frontend.problem(NoticeKind::Error, "Speech engine stopped", "The speech engine stopped unexpectedly; your text so far was kept.");
            self.reset_dictation();
        } else {
            self.frontend.problem(NoticeKind::Error, "Speech engine restarting", "The speech engine stopped unexpectedly; it will restart on the next dictation.");
        }
    }

    /// Why a dictation produced no text: (a few words for the pill, the full explanation).
    fn nothing_heard(&self) -> (&'static str, String) {
        if self.stats.samples == 0 {
            ("No audio from the microphone", "The microphone delivered no audio. Check the input device.".into())
        } else if self.stats.peak_db() < QUIET_PEAK_DB {
            ("Microphone is silent", format!("Heard silence (mic level {:.0} dBFS). Is the microphone muted or turned too low?", self.stats.peak_db()))
        } else if self.stats.rms_db() > LOUD_RMS_DB {
            (
                "Too loud and noisy",
                format!(
                    "The microphone is too loud and noisy (steady {:.0} dBFS), so speech is buried. Lower the input volume or mic boost in your sound settings.",
                    self.stats.rms_db()
                ),
            )
        } else {
            ("Didn't catch that", "No speech recognised. Try speaking closer to the microphone.".into())
        }
    }

    /// A stuck worker or mic: keep whatever was recognised, tell the user, and return to idle.
    fn recover_from_stall(&mut self) {
        if let Some(generation) = self.worker.as_ref().map(|w| w.generation) {
            self.worker_exited(generation); // kills + reaps, delivers the text so far, resets
        } else {
            self.capture = None;
            self.reset_dictation();
        }
    }

    fn watchdogs(&mut self) {
        match self.phase {
            Phase::Idle => {}
            Phase::Listening => {
                if self.options.input_wav.is_none() && self.last_audio.elapsed() > MIC_SILENT_AFTER {
                    self.frontend.problem(NoticeKind::Warn, "Microphone is silent", "The microphone is not delivering audio. Check the input device.");
                    self.cancel();
                } else if self.trigger.holding() && self.phase_since.elapsed() > MAX_HOLD {
                    self.frontend.problem(NoticeKind::Warn, "Time limit reached", "The shortcut was held for two minutes, so recording stopped. Your text so far is kept.");
                    self.trigger.reset();
                    self.stop();
                } else if !self.started_on_worker && !self.cloud_only && self.phase_since.elapsed() > READY_TIMEOUT {
                    self.frontend.problem(NoticeKind::Error, "Speech engine didn't start", "The speech engine did not start (are the models installed?).");
                    self.recover_from_stall();
                }
            }
            Phase::Finishing => {
                let secs = self.stats.samples as f64 / self.capture_rate.max(1) as f64;
                // a cloud request (upload + up to 25 s of polling) gets its own minute on top
                let cloud = if self.cloud_job.is_some() { 60.0 } else { 0.0 };
                let limit = Duration::from_secs_f64(READY_TIMEOUT.as_secs_f64() + 2.0 * secs + cloud);
                if self.phase_since.elapsed() > limit {
                    self.frontend.problem(NoticeKind::Error, "Speech engine took too long", "The speech engine took too long; your text so far was kept.");
                    self.recover_from_stall();
                }
            }
        }
    }

    fn tick(&mut self) {
        self.watchdogs();
        if self.last_memory_check.elapsed() >= Duration::from_secs(2) {
            self.last_memory_check = Instant::now();
            let memory = self.monitor.sample();
            let now = policy::under_pressure(&memory, self.config.low_memory_mb as f64);
            self.pressure_samples = if now { self.pressure_samples + 1 } else { 0 };
            self.pressure = self.pressure_samples >= 3; // about 6 s sustained
            // The guard's thresholds scale with the user's "low memory limit" (1024 MB = the documented defaults).
            let k = self.config.low_memory_mb.max(1) as f32 / 1024.0;
            let limits = memguard::Config { low_mb: 900.0 * k, critical_mb: 450.0 * k, low_frac: 0.05, critical_frac: 0.025, ..Default::default() };
            let snap = memguard::Snapshot { available_mb: memory.available_mb as f32, total_mb: memory.total_mb as f32, psi_some_avg10: memory.psi_some_avg10.map(|p| p as f32) };
            let cold_start_mb = match self.config.tier() {
                voicio_models::Tier::Lite => 350.0,
                voicio_models::Tier::Standard => 750.0,
                voicio_models::Tier::Quality => 1900.0,
            };
            let loaded = if self.worker.is_some() { 0.0 } else { cold_start_mb };
            self.guard = memguard::classify(&snap, loaded, &limits, self.guard);
        }
        let idle_limit = Duration::from_secs_f64(self.config.idle_unload_minutes.clamp(0.05, 1440.0) * 60.0);
        let action = policy::decide(
            self.worker.is_some(),
            self.phase != Phase::Idle,
            self.last_used.elapsed(),
            idle_limit,
            self.pressure,
        );
        match action {
            Action::Unload => self.unload(if self.pressure { "memory pressure" } else { "idle" }),
            Action::TrimAfterDictation => self.trim_after_dictation = true,
            Action::None => {}
        }
    }

    fn unload(&mut self, reason: &str) {
        if let Some(worker) = self.worker.take() {
            eprintln!("voicio: unloading speech models ({reason})");
            self.expected_exit = Some(worker.generation);
            worker.shutdown();
            self.model = "not loaded".into();
            self.frontend.models("Speech models not loaded");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Preview;

    #[test]
    fn preview_replaces_partials_and_keeps_finals_in_order() {
        let mut p = Preview::default();
        p.update("u0".into(), "hello".into(), false);
        p.update("u0".into(), "hello there".into(), false);
        assert_eq!(p.text(), "hello there");
        p.update("u0".into(), "Hello there.".into(), true);
        p.update("u0".into(), "stale partial".into(), false);
        p.update("u1".into(), "how are".into(), false);
        assert_eq!(p.text(), "Hello there. how are");
        p.update("u1".into(), "How are you?".into(), true);
        assert_eq!(p.text(), "Hello there. How are you?");
    }
}
