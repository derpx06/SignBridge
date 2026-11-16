//! Splits microphone audio into three loudness bands with two one-pole filters, so the pill's
//! lattice can show bass in its middle, mids in the middle ring and treble at the edge. Cheap
//! (a few multiplications per sample) and pure, so it is unit-tested.

use super::model::Bands;

pub struct Splitter {
    low_k: f32,
    high_k: f32,
    low: f32,
    high: f32,
    sums: [f64; 4],
    count: usize,
}

impl Splitter {
    /// `rate` is the microphone's sample rate in Hz. Bass is below ~300 Hz, treble above ~2.5 kHz.
    pub fn new(rate: u32) -> Splitter {
        let k = |hz: f32| 1.0 - (-std::f32::consts::TAU * hz / rate.max(8000) as f32).exp();
        Splitter { low_k: k(300.0), high_k: k(2500.0), low: 0.0, high: 0.0, sums: [0.0; 4], count: 0 }
    }

    pub fn feed(&mut self, chunk: &[f32]) {
        for &x in chunk {
            self.low += self.low_k * (x - self.low);
            self.high += self.high_k * (x - self.high);
            let (bass, treble) = (self.low, x - self.high);
            let mid = self.high - self.low;
            for (sum, v) in self.sums.iter_mut().zip([x, bass, mid, treble]) {
                *sum += (v as f64) * (v as f64);
            }
        }
        self.count += chunk.len();
    }

    /// The RMS of each band since the last call, and a fresh start.
    pub fn take(&mut self) -> Bands {
        let n = self.count.max(1) as f64;
        let rms = |i: usize| (self.sums[i] / n).sqrt() as f32;
        let out = Bands { level: rms(0), low: rms(1), mid: rms(2), high: rms(3) };
        self.sums = [0.0; 4];
        self.count = 0;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, rate: u32, n: usize) -> Vec<f32> {
        (0..n).map(|i| 0.3 * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin()).collect()
    }

    fn bands_of(hz: f32) -> Bands {
        let mut s = Splitter::new(48_000);
        s.feed(&tone(hz, 48_000, 48_000));
        s.take()
    }

    #[test]
    fn a_deep_tone_lands_in_the_low_band_and_a_high_one_in_the_high_band() {
        let (deep, mid, high) = (bands_of(120.0), bands_of(1000.0), bands_of(6000.0));
        assert!(deep.low > deep.mid && deep.low > deep.high, "{deep:?}");
        assert!(mid.mid > mid.low && mid.mid > mid.high, "{mid:?}");
        assert!(high.high > high.low && high.high > high.mid, "{high:?}");
    }

    #[test]
    fn the_overall_level_is_the_plain_rms_and_silence_is_zero() {
        let b = bands_of(440.0);
        assert!((b.level - 0.3 / 2f32.sqrt()).abs() < 0.01, "{b:?}");
        let mut s = Splitter::new(16_000);
        s.feed(&[0.0; 1600]);
        assert_eq!(s.take(), Bands::default());
        assert_eq!(s.take(), Bands::default(), "an empty window is safe");
    }
}
