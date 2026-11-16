//! Streaming background-noise suppression for 16 kHz speech: a spectral Wiener filter.
//!
//! - 32 ms frames, 50 % overlap, square-root Hann windows (overlap-add reconstructs exactly).
//! - The noise floor of every frequency bin is tracked with minimum statistics: the smoothed power
//!   spectrum's minimum over the last ~1.5 s, so speech never becomes "the noise" and a changing
//!   room (a fan switching on) is followed within seconds.
//! - The a-priori speech-to-noise ratio is smoothed decision-directed, and the Wiener gain is
//!   smoothed over frequency, which keeps the residue free of "musical noise" (warbling tones).
//! - Attenuation is capped at 6 dB: speech is never gated away, and what is left is a quieter,
//!   steadier floor. Measured on 100 LibriSpeech files with fan-and-hum noise added (Parakeet TDT
//!   110M): 4.30 -> 3.90 % WER at 6 dB, 8.76 -> 8.31 % at 2 dB, 25.6 -> 18.6 % at -2 dB, clean
//!   speech unchanged. Reproduce with `voicio-engine --eval DIR --noise-snr 2 [--no-denoise]`.
//!
//! Pure Rust, no allocation per frame, ~2 % of one core. Stationary noise (fans, hum, hiss, road
//! and air-conditioning noise) is what it removes; other voices are not removed (that would take a
//! neural model).

use std::f32::consts::PI;

const FRAME: usize = 512;
const HOP: usize = FRAME / 2;
const BINS: usize = FRAME / 2 + 1;
/// Longest attenuation (linear): -6 dB. Measured: deeper cuts started to eat speech and raised
/// the word error rate at moderate noise; this setting lowered it at every level tried.
const GAIN_FLOOR: f32 = 0.5;
const SMOOTH_POWER: f32 = 0.6;
const DD_BETA: f32 = 0.85;
const SUB_WINDOWS: usize = 8;
const SUB_LEN: usize = 12; // hops per sub-window: 8 x 12 x 16 ms = 1.5 s
/// Minimum statistics underestimates the mean noise power; this puts it back.
const MIN_BIAS: f32 = 1.8;

struct Fft {
    cos: Vec<f32>,
    sin: Vec<f32>,
    rev: Vec<usize>,
}

impl Fft {
    fn new() -> Self {
        let bits = FRAME.trailing_zeros();
        Self {
            cos: (0..FRAME / 2).map(|k| (2.0 * PI * k as f32 / FRAME as f32).cos()).collect(),
            sin: (0..FRAME / 2).map(|k| -(2.0 * PI * k as f32 / FRAME as f32).sin()).collect(),
            rev: (0..FRAME).map(|i| i.reverse_bits() >> (usize::BITS - bits)).collect(),
        }
    }

    /// In-place radix-2 transform; `inverse` flips the sign and scales by 1/N.
    fn run(&self, re: &mut [f32; FRAME], im: &mut [f32; FRAME], inverse: bool) {
        for i in 0..FRAME {
            let j = self.rev[i];
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= FRAME {
            let (half, step) = (len / 2, FRAME / len);
            for start in (0..FRAME).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = (self.cos[k * step], if inverse { -self.sin[k * step] } else { self.sin[k * step] });
                    let (a, b) = (start + k, start + k + half);
                    let (tr, ti) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len *= 2;
        }
        if inverse {
            let s = 1.0 / FRAME as f32;
            for i in 0..FRAME {
                re[i] *= s;
                im[i] *= s;
            }
        }
    }
}

pub struct Denoiser {
    fft: Fft,
    window: Vec<f32>,
    /// Input not yet consumed by a frame (starts with `FRAME - HOP` zeros so frames line up).
    input: Vec<f32>,
    /// Overlap-add tail of the previous frame.
    tail: [f32; HOP],
    /// Output samples still to drop: the zeros that were prepended.
    skip: usize,
    total_in: usize,
    total_out: usize,
    // per-bin state
    smooth: [f32; BINS],
    noise: [f32; BINS],
    sub_min: [[f32; BINS]; SUB_WINDOWS],
    cur_min: [f32; BINS],
    hops: usize,
    prev_gain: [f32; BINS],
    prev_gamma: [f32; BINS],
    primed: bool,
}

impl Default for Denoiser {
    fn default() -> Self {
        Self::new()
    }
}

impl Denoiser {
    pub fn new() -> Self {
        Self {
            fft: Fft::new(),
            window: (0..FRAME).map(|n| (PI * (n as f32 + 0.5) / FRAME as f32).sin()).collect(), // sqrt-Hann
            input: vec![0.0; FRAME - HOP],
            tail: [0.0; HOP],
            skip: FRAME - HOP,
            total_in: 0,
            total_out: 0,
            smooth: [0.0; BINS],
            noise: [0.0; BINS],
            sub_min: [[f32::MAX; BINS]; SUB_WINDOWS],
            cur_min: [f32::MAX; BINS],
            hops: 0,
            prev_gain: [1.0; BINS],
            prev_gamma: [1.0; BINS],
            primed: false,
        }
    }

    /// Cleans `samples` (16 kHz mono); the result is delayed by at most one frame and flushes with
    /// [`Denoiser::finish`]. Chunk sizes do not matter.
    pub fn process(&mut self, samples: &[f32]) -> Vec<f32> {
        self.total_in += samples.len();
        self.input.extend_from_slice(samples);
        let mut out = Vec::with_capacity(samples.len() + HOP);
        while self.input.len() >= FRAME {
            let mut frame = [0.0f32; HOP];
            self.frame(&mut frame);
            self.input.drain(..HOP);
            let skip = self.skip.min(HOP);
            self.skip -= skip;
            out.extend_from_slice(&frame[skip..]);
        }
        self.total_out += out.len();
        out
    }

    /// The samples still held back, so that in total exactly as many come out as went in.
    pub fn finish(&mut self) -> Vec<f32> {
        let owed = self.total_in - self.total_out;
        let mut out = Vec::new();
        let zeros = vec![0.0; FRAME];
        while out.len() < owed {
            let before = self.total_in;
            out.extend(self.process(&zeros));
            self.total_in = before; // the padding is not input
        }
        out.truncate(owed);
        self.total_out = self.total_in;
        out
    }

    /// One frame: window, transform, gain, inverse, overlap-add; writes the finished `HOP` samples.
    fn frame(&mut self, done: &mut [f32; HOP]) {
        let mut re = [0.0f32; FRAME];
        let mut im = [0.0f32; FRAME];
        for n in 0..FRAME {
            re[n] = self.input[n] * self.window[n];
        }
        self.fft.run(&mut re, &mut im, false);

        let mut power = [0.0f32; BINS];
        for k in 0..BINS {
            power[k] = re[k] * re[k] + im[k] * im[k];
        }
        self.track_noise(&power);

        let mut gain = [1.0f32; BINS];
        for k in 0..BINS {
            let lambda = (self.noise[k] * MIN_BIAS).max(1e-10);
            let gamma = power[k] / lambda;
            // decision-directed a-priori SNR
            let previous = self.prev_gain[k] * self.prev_gain[k] * self.prev_gamma[k];
            let xi = DD_BETA * previous + (1.0 - DD_BETA) * (gamma - 1.0).max(0.0);
            gain[k] = (xi / (1.0 + xi)).max(GAIN_FLOOR);
        }
        // smooth across frequency (3 taps), then remember for the next frame
        let mut smoothed = gain;
        for k in 1..BINS - 1 {
            smoothed[k] = 0.25 * gain[k - 1] + 0.5 * gain[k] + 0.25 * gain[k + 1];
        }
        for k in 0..BINS {
            self.prev_gain[k] = smoothed[k];
            self.prev_gamma[k] = power[k] / (self.noise[k] * MIN_BIAS).max(1e-10);
            re[k] *= smoothed[k];
            im[k] *= smoothed[k];
            if k > 0 && k < FRAME / 2 {
                re[FRAME - k] = re[k];
                im[FRAME - k] = -im[k];
            }
        }
        self.fft.run(&mut re, &mut im, true);
        for n in 0..HOP {
            done[n] = self.tail[n] + re[n] * self.window[n];
            self.tail[n] = re[n + HOP] * self.window[n + HOP];
        }
    }

    /// Minimum statistics: the quietest the smoothed spectrum got over the last ~1.5 s.
    fn track_noise(&mut self, power: &[f32; BINS]) {
        if !self.primed {
            self.smooth = *power;
            self.noise = *power;
            self.primed = true;
        }
        for k in 0..BINS {
            self.smooth[k] = SMOOTH_POWER * self.smooth[k] + (1.0 - SMOOTH_POWER) * power[k];
            self.cur_min[k] = self.cur_min[k].min(self.smooth[k]);
        }
        self.hops += 1;
        let slot = (self.hops / SUB_LEN) % SUB_WINDOWS;
        if self.hops.is_multiple_of(SUB_LEN) {
            // the sub-window ended: file its minimum, start the next one
            let ended = (slot + SUB_WINDOWS - 1) % SUB_WINDOWS;
            self.sub_min[ended] = self.cur_min;
            self.cur_min = [f32::MAX; BINS];
        }
        for k in 0..BINS {
            let mut min = self.cur_min[k];
            for window in &self.sub_min {
                min = min.min(window[k]);
            }
            // until the first sub-windows exist, follow the smoothed spectrum downwards only
            self.noise[k] = if min == f32::MAX { self.noise[k].min(self.smooth[k]) } else { min };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic noise source (xorshift), uniform in -1..1.
    struct Noise(u32);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
        }
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// A voiced-speech stand-in: a 140 Hz fundamental with harmonics, gated on and off like words.
    fn voice(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / 16000.0;
                let gate = if (t * 2.0).fract() < 0.6 { 1.0 } else { 0.0 };
                let tone: f32 = (1..8).map(|h| (2.0 * PI * 140.0 * h as f32 * t).sin() / h as f32).sum();
                0.25 * gate * tone
            })
            .collect()
    }

    fn run(samples: &[f32], chunk: usize) -> Vec<f32> {
        let mut d = Denoiser::new();
        let mut out: Vec<f32> = samples.chunks(chunk).flat_map(|c| d.process(c)).collect();
        out.extend(d.finish());
        out
    }

    #[test]
    fn steady_noise_is_reduced_to_about_the_attenuation_cap() {
        let mut n = Noise(7);
        let noise: Vec<f32> = (0..16000 * 6).map(|_| 0.03 * n.next()).collect();
        let out = run(&noise, 320);
        let (before, after) = (rms(&noise[16000 * 3..]), rms(&out[16000 * 3..]));
        assert!(after < before * 0.6, "noise went from {before:.4} to {after:.4}");
    }

    #[test]
    fn speech_keeps_its_level_and_the_signal_to_noise_ratio_improves() {
        let mut n = Noise(99);
        let clean = voice(16000 * 8);
        let noise: Vec<f32> = (0..clean.len()).map(|_| 0.04 * n.next()).collect();
        let noisy: Vec<f32> = clean.iter().zip(&noise).map(|(c, n)| c + n).collect();
        let out = run(&noisy, 480);
        // judge the second half, after the noise floor has been learned
        let (from, to) = (16000 * 4, 16000 * 8);
        let speech_in: Vec<f32> = (from..to).filter(|i| clean[*i].abs() > 0.0).map(|i| noisy[i]).collect();
        let speech_out: Vec<f32> = (from..to).filter(|i| clean[*i].abs() > 0.0).map(|i| out[i]).collect();
        let quiet_in: Vec<f32> = (from..to).filter(|i| clean[*i] == 0.0).map(|i| noisy[i]).collect();
        let quiet_out: Vec<f32> = (from..to).filter(|i| clean[*i] == 0.0).map(|i| out[i]).collect();
        assert!(rms(&speech_out) > rms(&speech_in) * 0.8, "speech must keep its level");
        let snr = |s: &[f32], q: &[f32]| 20.0 * (rms(s) / rms(q)).log10();
        assert!(snr(&speech_out, &quiet_out) > snr(&speech_in, &quiet_in) + 3.0, "the gap between speech and the room widens");
    }

    #[test]
    fn chunk_size_does_not_change_the_result_and_length_is_kept() {
        let mut n = Noise(5);
        let noisy: Vec<f32> = voice(16000 * 3).iter().map(|v| v + 0.02 * n.next()).collect();
        let (a, b) = (run(&noisy, 160), run(&noisy, 1777));
        assert_eq!(a.len(), noisy.len());
        assert_eq!(b.len(), noisy.len());
        let worst = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max);
        assert!(worst < 1e-4, "chunking changed the output by {worst}");
    }

    #[test]
    fn silence_and_tiny_inputs_are_safe() {
        assert!(run(&[], 100).is_empty());
        assert_eq!(run(&[0.0; 10], 3).len(), 10);
        assert!(run(&vec![0.0; 16000], 400).iter().all(|v| v.abs() < 1e-6));
    }
}
