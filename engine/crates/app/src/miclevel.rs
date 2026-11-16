//! Live microphone level for the setup's "check your microphone" step. It opens the default input only
//! while that step is on screen (so the OS microphone indicator is honest), keeps nothing, and turns the
//! loudness into plain guidance.

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SampleFormat;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub struct Meter {
    peak: Arc<AtomicU32>, // f32 bits of the loudest sample since the last read (non-negative, so ordered like u32)
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
}

impl Meter {
    /// Opens `device` (a name from `audio::input_names`, "" for the system default).
    pub fn start(device: &str) -> Meter {
        let meter = Meter { peak: Arc::default(), stop: Arc::default(), error: Arc::default() };
        let (peak, stop, error) = (meter.peak.clone(), meter.stop.clone(), meter.error.clone());
        let device = device.to_string();
        // The stream is not Send, so it lives and dies on its own thread.
        std::thread::Builder::new()
            .name("mic-meter".into())
            .spawn(move || {
                if let Err(e) = run(&device, &peak, &stop) {
                    *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                }
            })
            .ok();
        meter
    }

    /// Loudest sample (0..1) since the previous call.
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Drop for Meter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run(device: &str, peak: &Arc<AtomicU32>, stop: &Arc<AtomicBool>) -> Result<(), String> {
    let device = crate::audio::input_device(device).ok_or("no microphone found")?;
    let supported = device.default_input_config().map_err(|e| format!("microphone: {e}"))?;
    let config = supported.config();
    let on_error = |e| eprintln!("voicio: microphone meter: {e}");
    let note = |peak: &Arc<AtomicU32>, loudest: f32| {
        peak.fetch_max(loudest.to_bits(), Ordering::Relaxed);
    };
    let p = peak.clone();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => device.build_input_stream(config, move |d: &[f32], _| note(&p, d.iter().fold(0.0, |m, s| s.abs().max(m))), on_error, None),
        SampleFormat::I16 => device.build_input_stream(config, move |d: &[i16], _| note(&p, d.iter().fold(0.0, |m, s| (*s as f32 / 32768.0).abs().max(m))), on_error, None),
        SampleFormat::I32 => device.build_input_stream(config, move |d: &[i32], _| note(&p, d.iter().fold(0.0, |m, s| (*s as f32 / 2_147_483_648.0).abs().max(m))), on_error, None),
        SampleFormat::U16 => device.build_input_stream(config, move |d: &[u16], _| note(&p, d.iter().fold(0.0, |m, s| ((*s as f32 - 32768.0) / 32768.0).abs().max(m))), on_error, None),
        other => return Err(format!("unsupported microphone sample format {other:?}")),
    }
    .map_err(|e| format!("microphone: {e}"))?;
    stream.play().map_err(|e| format!("microphone: {e}"))?;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MicState {
    /// Just opened; say something.
    Waiting,
    /// Nothing reaches the app: muted, unplugged or the wrong input.
    NoAudio,
    /// Sound arrives but too faintly for good recognition.
    Quiet,
    Good,
}

impl MicState {
    pub fn key(self) -> &'static str {
        match self {
            MicState::Waiting => "waiting",
            MicState::NoAudio => "none",
            MicState::Quiet => "quiet",
            MicState::Good => "ok",
        }
    }
}

/// Judges the last few seconds of loudness. Speech peaks around 0.05 to 0.5; 0.02 is about -34 dB.
pub struct Guidance {
    recent: VecDeque<f32>,
    polls: usize,
}

const WINDOW: usize = 240; // ~8 s at 30 polls a second: "good" stays good between sentences
const SETTLE: usize = 45; // ~1.5 s before judging a silent input

impl Guidance {
    pub fn new() -> Guidance {
        Guidance { recent: VecDeque::new(), polls: 0 }
    }

    pub fn push(&mut self, peak: f32, failed: bool) -> MicState {
        self.polls += 1;
        self.recent.push_back(peak);
        if self.recent.len() > WINDOW {
            self.recent.pop_front();
        }
        let loudest = self.recent.iter().fold(0.0f32, |m, p| m.max(*p));
        if failed {
            MicState::NoAudio
        } else if loudest >= 0.02 {
            MicState::Good
        } else if self.polls < SETTLE {
            MicState::Waiting
        } else if loudest < 0.0015 {
            MicState::NoAudio
        } else {
            MicState::Quiet
        }
    }
}

/// Bar height 0..1 for a peak: a square root, so ordinary speech moves most of the bar.
pub fn display_level(peak: f32) -> f32 {
    (peak.max(0.0).sqrt() * 1.8).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_no_audio_faint_is_quiet_speech_is_good() {
        let mut g = Guidance::new();
        assert_eq!(g.push(0.0, false), MicState::Waiting);
        for _ in 0..SETTLE {
            g.push(0.0, false);
        }
        assert_eq!(g.push(0.0, false), MicState::NoAudio);
        assert_eq!(g.push(0.005, false), MicState::Quiet);
        assert_eq!(g.push(0.2, false), MicState::Good);
        assert_eq!(g.push(0.0, false), MicState::Good, "stays good between sentences");
        assert_eq!(Guidance::new().push(0.3, true), MicState::NoAudio, "no device");
    }

    #[test]
    fn speech_fills_most_of_the_bar() {
        assert!(display_level(0.0) == 0.0 && display_level(0.1) > 0.5 && display_level(1.0) == 1.0);
    }
}
