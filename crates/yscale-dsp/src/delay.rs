//! Fractional-sample delay line for loudspeaker time alignment.
//!
//! Integer delays are exact; sub-sample delays use 4-point (cubic) Lagrange
//! interpolation for low interpolation error. The line carries a fixed 1-sample
//! of internal latency so the interpolation window is always causal; this
//! latency is identical on every channel, so *relative* alignment is exact.

use crate::{MonoProcessor, SPEED_OF_SOUND_M_S};

/// A delay line with fractional-sample resolution.
#[derive(Debug, Clone)]
pub struct Delay {
    buf: Vec<f64>,
    mask: usize,
    write: usize,
    /// Requested delay in samples (the applied delay adds 1 sample of latency).
    delay_samples: f64,
}

impl Delay {
    /// Create a delay line able to hold up to `max_delay_samples` of delay.
    pub fn new(max_delay_samples: f64) -> Self {
        let need = max_delay_samples.ceil().max(0.0) as usize + 8;
        let cap = need.next_power_of_two().max(8);
        Self {
            buf: vec![0.0; cap],
            mask: cap - 1,
            write: 0,
            delay_samples: 0.0,
        }
    }

    /// Largest delay (in samples) this line can produce.
    pub fn capacity_samples(&self) -> f64 {
        (self.buf.len() - 4) as f64
    }

    /// Set the delay in samples (clamped to `[0, capacity]`).
    pub fn set_delay_samples(&mut self, d: f64) {
        self.delay_samples = d.clamp(0.0, self.capacity_samples());
    }

    /// Set the delay from a time in milliseconds.
    pub fn set_delay_ms(&mut self, ms: f64, fs: f64) {
        self.set_delay_samples(ms * 1e-3 * fs);
    }

    /// Set the delay from an acoustic path-length difference in centimetres,
    /// using `SPEED_OF_SOUND_M_S`. Handy for aligning drivers physically offset
    /// from one another.
    pub fn set_delay_distance_cm(&mut self, cm: f64, fs: f64) {
        self.set_delay_samples(cm * 0.01 / SPEED_OF_SOUND_M_S * fs);
    }

    pub fn delay_samples(&self) -> f64 {
        self.delay_samples
    }

    #[inline]
    fn tap(&self, delay_back: usize) -> f64 {
        self.buf[self.write.wrapping_sub(delay_back) & self.mask]
    }
}

/// 4-point Lagrange interpolation through points at positions -1, 0, 1, 2,
/// evaluated at fractional position `c` in `[0, 1)`.
#[inline]
fn lagrange3(ym1: f64, y0: f64, y1: f64, y2: f64, c: f64) -> f64 {
    let cm1 = c - 1.0;
    let cm2 = c - 2.0;
    let cp1 = c + 1.0;
    let l_m1 = c * cm1 * cm2 / -6.0;
    let l_0 = cp1 * cm1 * cm2 / 2.0;
    let l_1 = cp1 * c * cm2 / -2.0;
    let l_2 = cp1 * c * cm1 / 6.0;
    l_m1 * ym1 + l_0 * y0 + l_1 * y1 + l_2 * y2
}

impl MonoProcessor for Delay {
    #[inline]
    fn process_sample(&mut self, x: f64) -> f64 {
        self.buf[self.write & self.mask] = x;
        // One sample of built-in latency keeps the Lagrange window causal.
        let d = self.delay_samples + 1.0;
        let di = d.floor();
        let frac = d - di;
        let di = di as usize; // >= 1
        // Window of taps around the read point (newer -> older).
        let ym1 = self.tap(di - 1);
        let y0 = self.tap(di);
        let y1 = self.tap(di + 1);
        let y2 = self.tap(di + 2);
        self.write = self.write.wrapping_add(1);
        lagrange3(ym1, y0, y1, y2, frac)
    }

    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.write = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_response(delay: &mut Delay, n: usize) -> Vec<f64> {
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let x = if i == 0 { 1.0 } else { 0.0 };
            out.push(delay.process_sample(x));
        }
        out
    }

    #[test]
    fn integer_delay_shifts_impulse() {
        let mut d = Delay::new(64.0);
        d.set_delay_samples(10.0);
        let ir = impulse_response(&mut d, 32);
        // +1 sample of internal latency: impulse lands at index 11.
        let peak = ir
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .unwrap()
            .0;
        assert_eq!(peak, 11);
        assert!((ir[11] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn fractional_delay_centroid() {
        // A half-sample delay should put the energy centroid near 1 + 10.5.
        let mut d = Delay::new(64.0);
        d.set_delay_samples(10.5);
        let ir = impulse_response(&mut d, 40);
        let total: f64 = ir.iter().map(|v| v.abs()).sum();
        let centroid: f64 =
            ir.iter().enumerate().map(|(i, v)| i as f64 * v.abs()).sum::<f64>() / total;
        assert!((centroid - 11.5).abs() < 0.15, "centroid {centroid}");
    }

    #[test]
    fn distance_to_samples() {
        // 34.3 cm at 343 m/s = 1 ms; at 48 kHz that's 48 samples.
        let mut d = Delay::new(256.0);
        d.set_delay_distance_cm(34.3, 48_000.0);
        assert!((d.delay_samples() - 48.0).abs() < 1e-6);
    }

    #[test]
    fn zero_delay_is_one_sample_latency() {
        let mut d = Delay::new(16.0);
        d.set_delay_samples(0.0);
        let ir = impulse_response(&mut d, 8);
        assert!((ir[1] - 1.0).abs() < 1e-9);
    }
}
