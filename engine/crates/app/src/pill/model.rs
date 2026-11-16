//! The pill's lifecycle and motion: what the dictation is doing, and the eased values the painter
//! draws from. Pure and Slint-free; time is passed in, never read, so tests replay exactly.
//!
//! Honeycomb v2 (Honeycomb2.dc.html): the pill's mark is 37 soft hexagons in rings of 1, 6, 12
//! and 18 around an orange core. The voice lifts the rings from the centre out: each ring follows
//! the (auto-levelled) loudness 60 ms after the one inside it and eases there in about 200 ms, all
//! stepped at a fixed 240 Hz so it looks the same at any frame rate. While the text is cleaned up
//! an amber sweep circles the outer ring, on success seven cells form a tick, and every edge case
//! (nothing heard, mic silent, too loud, warming up, busy, copied, failed) has its own calm look
//! through `Notice`. Nothing flashes or shakes.
//!
//! The lifecycle idea (a pure model, minimum visible times so a fast result still registers,
//! nothing drawn while idle) follows Hex (https://github.com/anomalyco/hex, MIT).

use super::spring::{Spec, Spring};
use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Hexagons within axial radius 3 of the core: 1 + 6 + 12 + 18.
pub const CELLS: usize = 37;
pub const RINGS: usize = 4;

/// The capsule's width at rest and at most (points). Words and notices widen it between the two.
pub const BASE_WIDTH: f32 = 58.0;
pub const MAX_WIDTH: f32 = 380.0;
/// Left of the words: the lattice; right of them: padding.
pub const TEXT_LEFT: f32 = 52.0;
pub const TEXT_RIGHT: f32 = 16.0;

/// The shortest "working" look: a fast result still shows the sweep before the check.
const WORKING_MIN: Duration = Duration::from_millis(320);
/// The tick holds this long, then the capsule slips away (Capsule.dc.html: "+1.2 s").
const DONE_HOLD: Duration = Duration::from_millis(1200);
const WARN_HOLD: Duration = Duration::from_millis(2000);
const ERROR_HOLD: Duration = Duration::from_millis(2600);
const INFO_HOLD: Duration = Duration::from_millis(2400);

/// The simulation advances in fixed steps, so the picture is identical at any frame rate.
const SIM_STEP: f32 = 1.0 / 240.0;
/// The capsule eases in on key-down (settled in about 200 ms, a barely visible overshoot), and eases away.
const REVEAL_IN: Spec = Spec::new(0.34, 0.1);
const REVEAL_OUT: Spec = Spec::new(0.20, 0.0);
/// The capsule's width target moves in steps of this many points, so most partials change nothing.
const WIDTH_STEP: f32 = 16.0;

/// Each ring lifts this long after the one inside it, and eases to its target in about 200 ms.
const RING_DELAY: f32 = 0.060;
const RING_TAU: f32 = 0.067;
/// Voice history kept for the delayed rings (3 delays at 240 Hz, plus a little).
const HISTORY: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoticeKind {
    /// Something to adjust (amber): too-short press, silence, too loud, busy.
    Warn,
    /// Something went wrong (coral, shakes).
    Error,
    /// Good news that needs a word (teal check): "Copied".
    Info,
}

/// Microphone loudness (raw RMS, 0..1) overall and in three bands.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Bands {
    pub level: f32,
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

impl Bands {
    /// One level for all bands (when only the overall loudness is known).
    #[cfg(test)]
    pub fn flat(rms: f32) -> Bands {
        Bands { level: rms, low: rms, mid: rms, high: rms }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Recording begins.
    Started,
    /// Microphone loudness, as it arrives.
    Meter(Bands),
    /// Recording ended; the text is being cleaned up.
    Submitted,
    /// The text was typed.
    Completed,
    /// Dictation was cancelled.
    Cancelled,
    /// Show a message with this look.
    Notice(NoticeKind),
    /// The speech models are loading (true) or ready (false).
    Warming(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Hidden,
    Listening,
    Working,
    Done,
    Notice,
}

/// Everything the painter needs for one frame; all 0..1 unless noted.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// The capsule's presence: grows in with a small settle, shrinks away.
    pub reveal: f32,
    /// The capsule's width in points: widens as live words arrive.
    pub width: f32,
    pub opacity: f32,
    /// Each cell's light (0 dark .. 1 full), for tests and simple painters.
    pub cells: [f32; CELLS],
    /// How far each ring is lifted by the voice, core first (0 rest .. 1 full voice).
    pub rings: [f32; RINGS],
    /// 0 listening .. 1 working.
    pub working: f32,
    /// The check mark drawing itself.
    pub done: f32,
    /// Coral tint.
    pub fail: f32,
    /// Amber tint (something to adjust).
    pub amber: f32,
    /// Horizontal offset in points (always 0: a problem is shown by colour alone).
    pub shake: f32,
    /// Smoothed voice level, for the glow.
    pub level: f32,
    /// How much room noise is being filtered out (0 quiet room .. 1 loud room). The painter shows
    /// it as a muted shimmer on the rim that never reaches the voice-lit core.
    pub noise: f32,
    /// Models loading: 0..1 of the lattice filled in.
    pub warming: f32,
    /// Seconds recording so far (for the timer).
    pub seconds: f32,
    /// Free-running seconds, for slow ambient motion.
    pub time: f32,
}

/// Axial (q, r) of every cell, row by row from the top.
fn axial_table() -> &'static [(i32, i32); CELLS] {
    static T: OnceLock<[(i32, i32); CELLS]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [(0, 0); CELLS];
        let mut n = 0;
        for r in -3..=3i32 {
            for q in -3..=3i32 {
                if q.abs().max(r.abs()).max((q + r).abs()) <= 3 {
                    t[n] = (q, r);
                    n += 1;
                }
            }
        }
        t
    })
}

/// Axial coordinates (q, r) of cell `i` (pointy-top hexagons).
pub fn axial(i: usize) -> (i32, i32) {
    axial_table()[i]
}

/// Which ring cell `i` is in: 0 the core, 3 the rim.
pub fn ring(i: usize) -> usize {
    let (q, r) = axial(i);
    q.abs().max(r.abs()).max((q + r).abs()) as usize
}

/// Centre of cell `i` in units of the design's spacing `s` (x = s·√3·(q + r/2), y = s·1.5·r).
pub fn cell_pos(i: usize) -> (f32, f32) {
    let (q, r) = axial(i);
    (3f32.sqrt() * (q as f32 + r as f32 / 2.0), 1.5 * r as f32)
}

/// The seven cells that form the "typed" tick.
pub fn tick(i: usize) -> bool {
    matches!(axial(i), (-1, -1) | (-1, 0) | (-1, 1) | (0, 0) | (1, -1) | (2, -2) | (3, -3))
}

fn approach(x: &mut f32, target: f32, rate: f32, dt: f32) {
    *x += (target - *x) * (1.0 - (-rate * dt).exp());
}

/// Maps microphone loudness to 0..1 so a quiet microphone and a loud one move the lattice alike: a
/// fast-rising, slow-falling peak and a slowly rising room floor, both in dBFS.
#[derive(Clone, Copy)]
struct Level {
    peak: f32,
    room: f32,
    started: bool,
}

impl Level {
    const FLOOR: f32 = -62.0;
    /// The loudest "speech" is never taken to be quieter than this above the room, nor quieter
    /// than this in absolute terms, so a quiet room is not stretched into a loud one.
    const MIN_SPAN: f32 = 20.0;
    const PEAK_MIN: f32 = -42.0;
    /// How far above the room's quiet dips a sound must be before it lights anything: ordinary
    /// noise wobbles by a few dB around its floor, speech by far more.
    const MARGIN: f32 = 6.0;

    fn new() -> Self {
        Self { peak: Self::PEAK_MIN, room: Self::FLOOR, started: false }
    }

    fn feed(&mut self, rms: f32, dt: f32) -> f32 {
        let db = 20.0 * (rms.max(0.0) + 1e-6).log10();
        if !self.started && rms <= 1e-5 {
            return 0.0; // no audio has arrived yet
        }
        if !self.started {
            // the first moment is taken to be the room (it only drops from here if speech began at once)
            self.started = true;
            self.room = db.max(Self::FLOOR);
        }
        // the peak jumps up at once and falls back over ~3 s
        self.peak = if db > self.peak { db } else { self.peak - (self.peak - db).max(0.0) * (1.0 - (-dt / 3.0).exp()) };
        // the room drops to any quieter moment quickly and creeps up over ~6 s, so speech never becomes the room
        self.room = if db < self.room { self.room + (db - self.room) * (1.0 - (-dt / 0.3).exp()) } else { self.room + (db - self.room) * (1.0 - (-dt / 6.0).exp()) * 0.2 };
        self.room = self.room.max(Self::FLOOR);
        self.peak = self.peak.max(self.room + Self::MIN_SPAN).max(Self::PEAK_MIN);
        (((db - self.room - Self::MARGIN) / (self.peak - self.room - Self::MARGIN)).clamp(0.0, 1.0)).powf(1.15)
    }
}

pub struct Model {
    phase: Phase,
    kind: NoticeKind,
    since: Instant,
    started: Instant,
    completion_pending: bool,
    levels: [Level; 4],
    bands: Bands,
    /// Smoothed 0..1 loudness: overall, low, mid, high.
    voice: [f32; 4],
    /// Recent voice (newest first), for the rings that follow it later.
    history: VecDeque<f32>,
    rings: [f32; RINGS],
    reveal: Spring,
    /// The widest the capsule has needed during this dictation (it only grows until it ends).
    grown: f32,
    acc: f32,
    width: f32,
    opacity: f32,
    working: f32,
    done: f32,
    fail: f32,
    amber: f32,
    warming_flag: bool,
    warming: f32,
    glow: f32,
    noise: f32,
    clock: f32,
    active: bool,
}

impl Model {
    pub fn new(now: Instant) -> Self {
        Self {
            phase: Phase::Hidden,
            kind: NoticeKind::Warn,
            since: now,
            started: now,
            completion_pending: false,
            levels: [Level::new(); 4],
            bands: Bands::default(),
            voice: [0.0; 4],
            history: VecDeque::from(vec![0.0; HISTORY]),
            rings: [0.0; RINGS],
            reveal: Spring::at(0.0),
            grown: BASE_WIDTH,
            acc: 0.0,
            width: BASE_WIDTH,
            opacity: 0.0,
            working: 0.0,
            done: 0.0,
            fail: 0.0,
            amber: 0.0,
            warming_flag: false,
            warming: 0.0,
            glow: 0.0,
            noise: 0.0,
            clock: 0.0,
            active: false,
        }
    }

    /// The measured width of the text now shown (points), or `None` when there is none.
    pub fn set_text_width(&mut self, width: Option<f32>) {
        if let Some(text) = width {
            let need = ((TEXT_LEFT + text + TEXT_RIGHT) / WIDTH_STEP).ceil() * WIDTH_STEP;
            self.grown = self.grown.max(need.clamp(BASE_WIDTH, MAX_WIDTH));
        }
    }

    /// Whether frames are wanted: from an event until everything has settled out of sight.
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn handle(&mut self, event: Event, now: Instant) {
        self.active = true;
        match event {
            Event::Started => {
                self.phase = Phase::Listening;
                self.since = now;
                self.started = now;
                self.completion_pending = false;
                self.grown = BASE_WIDTH;
                self.levels = [Level::new(); 4];
                self.bands = Bands::default();
                self.done = 0.0;
                self.fail = 0.0;
                self.amber = 0.0;
            }
            Event::Meter(b) => self.bands = b,
            Event::Submitted => {
                if self.phase == Phase::Listening {
                    self.phase = Phase::Working;
                    self.since = now;
                    self.grown = BASE_WIDTH; // "Tidying…" sizes the capsule to itself
                }
            }
            Event::Completed => match self.phase {
                Phase::Listening | Phase::Working => {
                    if self.phase == Phase::Listening {
                        self.phase = Phase::Working;
                        self.since = now;
                    }
                    self.completion_pending = true; // the check waits out the minimum working look
                }
                _ => {}
            },
            Event::Cancelled => {
                if matches!(self.phase, Phase::Listening | Phase::Working) {
                    self.phase = Phase::Hidden;
                    self.since = now;
                    self.completion_pending = false;
                }
            }
            Event::Notice(kind) => {
                if self.phase != Phase::Notice {
                    self.grown = BASE_WIDTH; // each message sizes the capsule to its own text
                }
                self.phase = Phase::Notice;
                self.kind = kind;
                self.since = now;
                self.completion_pending = false;
            }
            Event::Warming(on) => self.warming_flag = on,
        }
    }

    pub fn step(&mut self, now: Instant, dt: f32) -> Frame {
        // Phase changes depend on wall-clock time; everything smooth advances in fixed steps.
        if self.completion_pending && now.duration_since(self.since) >= WORKING_MIN {
            self.phase = Phase::Done;
            self.since = now;
            self.completion_pending = false;
            self.grown = BASE_WIDTH; // "Typed" is shorter than the words were

        }
        let in_phase = now.duration_since(self.since);
        let hold = match self.phase {
            Phase::Done => Some(DONE_HOLD),
            Phase::Notice => Some(match self.kind {
                NoticeKind::Warn => WARN_HOLD,
                NoticeKind::Error => ERROR_HOLD,
                NoticeKind::Info => INFO_HOLD,
            }),
            _ => None,
        };
        if hold.is_some_and(|h| in_phase >= h) {
            self.phase = Phase::Hidden;
            self.since = now;
        }
        self.acc += dt.clamp(0.0, 0.25);
        while self.acc >= SIM_STEP {
            self.acc -= SIM_STEP;
            self.simulate(SIM_STEP);
        }
        let shown = self.phase != Phase::Hidden;
        self.active = shown || self.opacity > 0.004 || self.reveal.x > 0.004 || self.rings.iter().any(|r| *r > 0.01);

        // A problem is shown by its coral tint alone: nothing shakes or flashes.
        let shake = 0.0;
        Frame {
            reveal: self.reveal.x.max(0.0),
            width: self.width,
            opacity: self.opacity,
            cells: self.cells(in_phase.as_secs_f32()),
            rings: self.rings,
            working: self.working,
            done: self.done,
            fail: self.fail,
            amber: self.amber,
            shake,
            level: self.glow,
            noise: self.noise,
            warming: self.warming,
            seconds: if shown { now.duration_since(self.started).as_secs_f32() } else { 0.0 },
            time: self.clock,
        }
    }

    /// The light of every cell for the current look.
    fn cells(&self, in_phase: f32) -> [f32; CELLS] {
        let t = self.clock;
        let notice = self.phase == Phase::Notice;
        let mut out = [0.0; CELLS];
        for (i, cell) in out.iter_mut().enumerate() {
            let g = ring(i);
            let (x, y) = cell_pos(i);
            // listening: rest opacities, lifted ring by ring with the voice
            const REST: [f32; RINGS] = [0.8, 0.32, 0.32, 0.2];
            const VOICE: [f32; RINGS] = [1.0, 1.0, 0.75, 0.35];
            let listening = REST[g] + (VOICE[g] - REST[g]) * self.rings[g];
            // working: an amber sweep circles the rim, the inside stays low
            let w = ((y.atan2(x) + 0.9 - t * 4.0).cos() + 1.0) / 2.0;
            let working = if g == 3 { 0.15 + 0.85 * w } else if g == 0 { 0.9 } else { 0.3 };
            // done: seven cells form a tick, the rest fade back
            let check = if tick(i) { 1.0 } else { 0.14 };
            let mut v = listening + (working - listening) * self.working;
            v += (check - v) * self.done;
            // warming: the comb fills in from the middle
            if self.warming > 0.001 {
                let lit = ((self.warming * 1.3 - g as f32 / 3.0 * 0.45) * 3.0).clamp(0.0, 1.0);
                v = v.min(0.1 + 0.55 * lit);
            }
            // notices: amber and coral hold still; info is the tick
            if notice {
                let fade_in = (in_phase / 0.2).min(1.0);
                let target = match self.kind {
                    NoticeKind::Warn => 0.6,
                    NoticeKind::Error => if g >= 2 { 0.45 } else { 0.9 },
                    NoticeKind::Info => check,
                };
                v += (target - v) * fade_in;
            }
            *cell = v.clamp(0.0, 1.0);
        }
        out
    }

    /// One fixed simulation step of `h` seconds.
    fn simulate(&mut self, h: f32) {
        self.clock += h;
        let shown = self.phase != Phase::Hidden;
        let listening = self.phase == Phase::Listening;
        let working = matches!(self.phase, Phase::Working | Phase::Done);
        let notice = self.phase == Phase::Notice;

        // The voice: normalised loudness per band, smoothed fast up and slower down.
        let raw = [self.bands.level, self.bands.low, self.bands.mid, self.bands.high];
        for k in 0..4 {
            let level = if listening { self.levels[k].feed(raw[k], h) } else { 0.0 };
            let tau = if level > self.voice[k] { 0.06 } else { 0.18 };
            self.voice[k] += (level - self.voice[k]) * (1.0 - (-h / tau).exp());
        }

        approach(&mut self.working, if working { 1.0 } else { 0.0 }, 9.0, h);
        approach(&mut self.done, if self.phase == Phase::Done || (notice && self.kind == NoticeKind::Info) { 1.0 } else { 0.0 }, 11.0, h);
        approach(&mut self.fail, if notice && self.kind == NoticeKind::Error { 1.0 } else { 0.0 }, 14.0, h);
        approach(&mut self.amber, if notice && self.kind == NoticeKind::Warn { 1.0 } else { 0.0 }, 14.0, h);
        approach(&mut self.warming, if self.warming_flag && listening { 1.0 } else { 0.0 }, 3.0, h);
        approach(&mut self.glow, if listening { self.voice[0] } else { 0.0 }, 10.0, h);
        // the room's loudness (-60 dBFS = silent .. -34 dBFS = loud), as the noise being filtered out
        let room = if listening && self.levels[0].started { ((self.levels[0].room + 60.0) / 26.0).clamp(0.0, 1.0) } else { 0.0 };
        approach(&mut self.noise, room, 3.0, h);
        approach(&mut self.opacity, if shown { 1.0 } else { 0.0 }, if shown { 22.0 } else { 14.0 }, h);
        // the capsule widens for words and messages (it only grows during one), then eases back
        let wanted = if shown { self.grown } else { BASE_WIDTH };
        approach(&mut self.width, wanted, 13.0, h);
        self.reveal.step_to(if shown { 1.0 } else { 0.0 }, if shown { REVEAL_IN } else { REVEAL_OUT }, h);

        // The rings: ring k follows the voice k·60 ms later, easing there in about 200 ms.
        self.history.pop_back();
        // ordinary speech (auto-levelled to about 0.2..0.6) should lift the rings well
        self.history.push_front(if listening { (self.voice[0] * 2.0).min(1.0) } else { 0.0 });
        for k in 0..RINGS {
            let at = ((k as f32 * RING_DELAY) / h).round() as usize;
            let target = self.history.get(at).copied().unwrap_or(0.0);
            self.rings[k] += (target - self.rings[k]) * (1.0 - (-h / RING_TAU).exp());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(model: &mut Model, now: &mut Instant, seconds: f32) -> Frame {
        let mut frame = model.step(*now, 1.0 / 60.0);
        for _ in 0..(seconds * 60.0) as usize {
            *now += Duration::from_micros(16_667);
            frame = model.step(*now, 1.0 / 60.0);
        }
        frame
    }

    fn mean(cells: &[f32; CELLS]) -> f32 {
        cells.iter().sum::<f32>() / CELLS as f32
    }

    #[test]
    fn the_comb_is_a_hexagon_of_37_cells_in_four_rings() {
        let counts = (0..RINGS).map(|g| (0..CELLS).filter(|i| ring(*i) == g).count()).collect::<Vec<_>>();
        assert_eq!(counts, [1, 6, 12, 18]);
        for i in 0..CELLS {
            for j in i + 1..CELLS {
                let ((a, b), (c, d)) = (cell_pos(i), cell_pos(j));
                assert!(((a - c).powi(2) + (b - d).powi(2)).sqrt() > 1.7, "cells {i} and {j} overlap");
            }
        }
        assert_eq!((0..CELLS).filter(|i| tick(*i)).count(), 7);
    }

    #[test]
    fn a_dictation_listens_works_finishes_and_settles_out_of_sight() {
        let mut now = Instant::now();
        let mut model = Model::new(now);
        assert!(!model.active());
        model.handle(Event::Started, now);
        let listening = run(&mut model, &mut now, 0.6);
        assert!(listening.opacity > 0.95 && listening.reveal > 0.9);
        model.handle(Event::Submitted, now);
        let working = run(&mut model, &mut now, 0.6);
        assert!(working.working > 0.9, "the lattice settles into the working sweep");
        model.handle(Event::Completed, now);
        let done = run(&mut model, &mut now, 0.35);
        assert!(done.done > 0.5, "the check draws itself");
        run(&mut model, &mut now, 2.0);
        assert!(!model.active(), "everything has settled; no more frames are wanted");
    }

    #[test]
    fn a_fast_result_still_shows_the_working_look_before_the_check() {
        let mut now = Instant::now();
        let mut model = Model::new(now);
        model.handle(Event::Started, now);
        run(&mut model, &mut now, 0.3);
        model.handle(Event::Submitted, now);
        model.handle(Event::Completed, now);
        let early = run(&mut model, &mut now, 0.1);
        assert!(early.done < 0.2, "done waits for the minimum working time");
        let later = run(&mut model, &mut now, 0.5);
        assert!(later.done > 0.5);
    }

    #[test]
    fn a_cancel_just_fades() {
        let mut now = Instant::now();
        let mut model = Model::new(now);
        model.handle(Event::Started, now);
        run(&mut model, &mut now, 0.4);
        model.handle(Event::Cancelled, now);
        run(&mut model, &mut now, 1.5);
        assert!(!model.active());
    }

    #[test]
    fn every_notice_has_its_own_look_and_leaves_by_itself() {
        for (kind, check) in [
            (NoticeKind::Warn, (|f: &Frame| f.amber > 0.5 && f.fail < 0.2) as fn(&Frame) -> bool),
            (NoticeKind::Error, |f: &Frame| f.fail > 0.5 && f.amber < 0.2),
            (NoticeKind::Info, |f: &Frame| f.done > 0.5 && f.fail < 0.2 && f.amber < 0.2),
        ] {
            let mut now = Instant::now();
            let mut model = Model::new(now);
            model.handle(Event::Notice(kind), now); // a notice can appear with nothing recording
            let frame = run(&mut model, &mut now, 0.6);
            assert!(check(&frame), "{kind:?}: amber {} fail {} done {}", frame.amber, frame.fail, frame.done);
            assert!(frame.opacity > 0.9, "the capsule is shown for a notice");
            run(&mut model, &mut now, 3.5);
            assert!(!model.active(), "{kind:?} leaves by itself");
        }
    }

    #[test]
    fn nothing_shakes_not_even_an_error() {
        let shakes = |kind| {
            let mut now = Instant::now();
            let mut model = Model::new(now);
            model.handle(Event::Notice(kind), now);
            let mut worst: f32 = 0.0;
            for _ in 0..30 {
                now += Duration::from_micros(16_667);
                worst = worst.max(model.step(now, 1.0 / 60.0).shake.abs());
            }
            worst
        };
        assert_eq!(shakes(NoticeKind::Error), 0.0);
        assert_eq!(shakes(NoticeKind::Warn), 0.0);
    }

    #[test]
    fn warming_fills_the_lattice_in_and_then_clears() {
        let mut now = Instant::now();
        let mut model = Model::new(now);
        model.handle(Event::Started, now);
        model.handle(Event::Warming(true), now);
        let early = run(&mut model, &mut now, 0.15);
        let late = run(&mut model, &mut now, 1.5);
        assert!(late.warming > early.warming && late.warming > 0.8);
        model.handle(Event::Warming(false), now);
        assert!(run(&mut model, &mut now, 2.0).warming < 0.1);
    }

    #[test]
    fn the_picture_does_not_depend_on_the_frame_rate() {
        // The same dictation at 30, 60 and 144 frames per second. The ripples oscillate, so compare
        // each cell's average light over the last 1.5 s (phase-free), not one instant.
        let at = |fps: u32| {
            let mut now = Instant::now();
            let mut model = Model::new(now);
            let dt = 1.0 / fps as f32;
            model.handle(Event::Started, now);
            model.handle(Event::Meter(Bands { level: 0.1, low: 0.12, mid: 0.08, high: 0.04 }), now);
            let (mut sum, mut n) = ([0.0f32; CELLS], 0);
            let mut width = 0.0;
            for k in 0..(fps * 3) {
                now += Duration::from_secs_f32(dt);
                let frame = model.step(now, dt);
                if k >= fps * 3 / 2 {
                    for i in 0..CELLS {
                        sum[i] += frame.cells[i];
                    }
                    n += 1;
                }
                width = frame.width;
            }
            (sum.map(|s| s / n as f32), width)
        };
        let ((a, wa), (b, wb), (c, wc)) = (at(30), at(60), at(144));
        for i in 0..CELLS {
            assert!((a[i] - c[i]).abs() < 0.08, "cell {i}: {} vs {}", a[i], c[i]);
            assert!((b[i] - c[i]).abs() < 0.08, "cell {i}: {} vs {}", b[i], c[i]);
        }
        assert!((wa - wc).abs() < 0.5 && (wb - wc).abs() < 0.5);
    }

    #[test]
    fn the_rings_lift_from_the_centre_out_and_settle_in_silence() {
        let mut now = Instant::now();
        let mut model = Model::new(now);
        model.handle(Event::Started, now);
        model.handle(Event::Meter(Bands::flat(0.002)), now); // the room
        run(&mut model, &mut now, 0.5);
        model.handle(Event::Meter(Bands::flat(0.3)), now); // then speech
        let early = run(&mut model, &mut now, 0.1);
        assert!(early.rings[0] > early.rings[3] + 0.1, "the core lifts before the rim: {:?}", early.rings);
        let later = run(&mut model, &mut now, 0.5);
        assert!(later.rings.iter().all(|r| (0.0..=1.0).contains(r) && *r > 0.3), "{:?}", later.rings);
        model.handle(Event::Meter(Bands::default()), now);
        let quiet = run(&mut model, &mut now, 3.0);
        assert!(quiet.rings.iter().all(|r| *r < 0.05), "the rings settle in silence: {:?}", quiet.rings);
    }

    #[test]
    fn a_louder_voice_lights_the_lattice_more_whatever_the_microphone_gain() {
        let lit = |rms: f32| {
            let mut now = Instant::now();
            let mut model = Model::new(now);
            model.handle(Event::Started, now);
            run(&mut model, &mut now, 0.2);
            let mut best: f32 = 0.0;
            for k in 0..180 {
                // speech-like: loudness comes and goes, so the auto-level has something to scale
                let wobble = 0.35 + 0.65 * (((k as f32) * 0.31).sin().abs());
                model.handle(Event::Meter(Bands::flat(rms * wobble)), now);
                now += Duration::from_micros(16_667);
                best = best.max(mean(&model.step(now, 1.0 / 60.0).cells));
            }
            best
        };
        assert!(lit(0.3) > lit(0.0) + 0.05, "a hot microphone moves the lattice");
        assert!(lit(0.01) > lit(0.0) + 0.05, "a quiet microphone moves it too");
    }

    #[test]
    fn steady_room_noise_barely_moves_the_lattice() {
        let lit = |noise_db: f32, wobble_db: f32| {
            let mut now = Instant::now();
            let mut model = Model::new(now);
            model.handle(Event::Started, now);
            run(&mut model, &mut now, 0.2);
            let mut worst: f32 = 0.0;
            for k in 0..420 {
                // noise: a steady floor that wobbles a few dB
                let db = noise_db + wobble_db * ((k as f32 * 1.7).sin() * 0.6 + (k as f32 * 0.37).sin() * 0.4);
                model.handle(Event::Meter(Bands::flat(10f32.powf(db / 20.0))), now);
                now += Duration::from_micros(16_667);
                let frame = model.step(now, 1.0 / 60.0);
                if k > 180 {
                    worst = worst.max(mean(&frame.cells));
                }
            }
            worst
        };
        let silence = lit(-90.0, 0.0);
        for noise in [-58.0, -50.0, -44.0] {
            assert!(lit(noise, 4.0) < silence + 0.06, "{noise} dBFS of noise lit the lattice: {} vs {silence}", lit(noise, 4.0));
        }
    }

    #[test]
    fn the_capsule_only_grows_while_it_listens_and_in_steps() {
        let mut now = Instant::now();
        let mut model = Model::new(now);
        model.handle(Event::Started, now);
        model.set_text_width(Some(130.0));
        let wide = run(&mut model, &mut now, 1.0).width;
        model.set_text_width(Some(60.0)); // a revised, shorter partial must not shrink it
        let still = run(&mut model, &mut now, 1.0).width;
        assert!(wide > BASE_WIDTH + 40.0, "it widened for the words: {wide}");
        assert!((still - wide).abs() < 1.0, "and did not shrink on a shorter partial");
        model.set_text_width(Some(130.5)); // under one step of difference changes nothing
        assert!((run(&mut model, &mut now, 0.5).width - wide).abs() < 1.0);
    }

    #[test]
    fn a_late_event_after_hiding_does_not_leave_the_pill_stuck() {
        let mut now = Instant::now();
        let mut model = Model::new(now);
        model.handle(Event::Completed, now); // nothing was recording
        model.handle(Event::Meter(Bands::flat(0.2)), now);
        run(&mut model, &mut now, 1.0);
        assert!(!model.active());
    }
}
