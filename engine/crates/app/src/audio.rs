//! Audio capture: the default microphone via cpal (ALSA/PipeWire/PulseAudio on Linux, WASAPI on
//! Windows, CoreAudio on macOS), or a WAV file at real-time pace for headless tests. The stream
//! exists only while dictating, so the OS microphone indicator is honest.

use crate::agent::Msg;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

pub enum Capture {
    Mic(#[allow(dead_code)] Stream),
    File(Arc<AtomicBool>),
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Capture::File(stop) = self {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

/// Starts capture; mono f32 chunks arrive as `Msg::Audio(generation, ..)`. Returns the sample rate.
pub fn open(input_wav: Option<&Path>, generation: u64, tx: Sender<Msg>) -> Result<(Capture, u32), String> {
    match input_wav {
        Some(path) => open_file(path, generation, tx),
        None => open_mic(generation, tx),
    }
}

/// ALSA lists every plugin PCM (rate converters, null sinks, ...): offer one entry per sound card.
fn real(d: &cpal::DeviceDescription) -> bool {
    !cfg!(target_os = "linux") || d.driver().is_some_and(|p| p.starts_with("sysdefault:"))
}

/// Names of the input devices, for the microphone choice in the app window.
pub fn input_names() -> Vec<String> {
    let Ok(devices) = cpal::default_host().input_devices() else { return Vec::new() };
    let mut names: Vec<String> = devices.filter_map(|d| d.description().ok().filter(real).map(|d| d.name().to_string())).collect();
    names.dedup();
    names
}

/// The chosen input device by name, or the system default when it is "" or no longer there.
pub fn input_device(name: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let chosen = (!name.is_empty())
        .then(|| host.input_devices().ok()?.find(|d| d.description().is_ok_and(|x| real(&x) && x.name() == name)))
        .flatten();
    chosen.or_else(|| host.default_input_device())
}

fn open_mic(generation: u64, tx: Sender<Msg>) -> Result<(Capture, u32), String> {
    let device = input_device(&crate::config::Config::load().input_device).ok_or("no microphone found")?;
    let supported = device.default_input_config().map_err(|e| format!("microphone: {e}"))?;
    let channels = supported.channels().max(1) as usize;
    let rate = supported.sample_rate();
    let config = supported.config();
    let on_error = |e| eprintln!("voicio: microphone stream: {e}");
    let sink = move |chunk: Vec<f32>| {
        let _ = tx.send(Msg::Audio(generation, chunk));
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => device.build_input_stream(config, mono(channels, sink, |s: f32| s), on_error, None),
        SampleFormat::I16 => {
            device.build_input_stream(config, mono(channels, sink, |s: i16| s as f32 / 32768.0), on_error, None)
        }
        SampleFormat::I32 => device.build_input_stream(
            config,
            mono(channels, sink, |s: i32| s as f32 / 2_147_483_648.0),
            on_error,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            config,
            mono(channels, sink, |s: u16| (s as f32 - 32768.0) / 32768.0),
            on_error,
            None,
        ),
        other => return Err(format!("unsupported microphone sample format {other:?}")),
    }
    .map_err(|e| format!("microphone: {e}"))?;
    stream.play().map_err(|e| format!("microphone: {e}"))?;
    Ok((Capture::Mic(stream), rate))
}

/// Downmixes interleaved frames to mono f32.
fn mono<T: Copy>(
    channels: usize,
    mut sink: impl FnMut(Vec<f32>) + Send + 'static,
    convert: impl Fn(T) -> f32 + Send + 'static,
) -> impl FnMut(&[T], &cpal::InputCallbackInfo) + Send + 'static {
    move |data: &[T], _| {
        let frames = data
            .chunks_exact(channels)
            .map(|frame| frame.iter().map(|&s| convert(s)).sum::<f32>() / channels as f32);
        sink(frames.collect());
    }
}

/// 16-bit PCM WAV played at real time (test hook for headless runs).
fn open_file(path: &Path, generation: u64, tx: Sender<Msg>) -> Result<(Capture, u32), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (rate, channels, pcm) = parse_wav(&bytes).ok_or(format!("{}: not a 16-bit PCM WAV", path.display()))?;
    let samples: Vec<f32> = pcm
        .chunks_exact(2 * channels)
        .map(|frame| {
            frame.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).sum::<f32>()
                / channels as f32
        })
        .collect();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        for chunk in samples.chunks(rate as usize / 10) {
            if flag.load(Ordering::Relaxed) || tx.send(Msg::Audio(generation, chunk.to_vec())).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    Ok((Capture::File(stop), rate))
}

/// Minimal RIFF reader: (sample rate, channels, PCM bytes) for 16-bit PCM.
fn parse_wav(bytes: &[u8]) -> Option<(u32, usize, &[u8])> {
    if bytes.get(0..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let (mut pos, mut format) = (12, None);
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = bytes.get(pos + 8..(pos + 8 + size).min(bytes.len()))?;
        match id {
            b"fmt " if body.len() >= 16 => {
                let tag = u16::from_le_bytes([body[0], body[1]]);
                let channels = u16::from_le_bytes([body[2], body[3]]) as usize;
                let rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
                let bits = u16::from_le_bytes([body[14], body[15]]);
                if tag != 1 || bits != 16 || channels == 0 {
                    return None;
                }
                format = Some((rate, channels));
            }
            b"data" => return format.map(|(rate, channels)| (rate, channels, body)),
            _ => {}
        }
        pos += 8 + size + (size & 1);
    }
    None
}
