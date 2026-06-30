//! **Bass management** — split the low end off the main channels at a crossover
//! frequency, sum it to mono, and recombine. Mono bass below the crossover
//! tightens the low end and tames room modes (a staple of high-end processors),
//! and it's the basis of a 2.1 setup: with a dedicated sub channel the summed
//! bass is routed there instead of back into the mains.
//!
//! Built from Linkwitz-Riley low/high-pass pairs so the split sums flat and
//! in-phase. An optional infrasonic ("rumble") high-pass protects drivers from
//! sub-sonic energy.

use crate::biquad::BiquadChain;
use crate::crossover::{self, CrossoverKind};
use crate::MonoProcessor;

pub struct BassManager {
    enabled: bool,
    channels: usize,
    /// Index of a dedicated sub output channel, if any. `None` => mono bass is
    /// summed back into every main channel (works on a stereo DAC).
    sub_channel: Option<usize>,
    sub_gain: f64,
    lp: BiquadChain,        // shared mono low band
    hp: Vec<BiquadChain>,   // per-channel high-pass (mains)
    rumble: Vec<BiquadChain>, // per-channel infrasonic high-pass (0 => identity)
    rumble_on: bool,
}

impl BassManager {
    /// `freq` = crossover Hz, `order` = LR order (even). `rumble_hz` = 0 disables
    /// the infrasonic filter. `sub_channel` routes summed bass to that output.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fs: f64,
        channels: usize,
        enabled: bool,
        freq: f64,
        order: usize,
        rumble_hz: f64,
        sub_channel: Option<usize>,
        sub_gain_db: f64,
    ) -> Self {
        let channels = channels.max(1);
        let lp = crossover::lowpass(CrossoverKind::LinkwitzRiley, order, freq, fs);
        let hp = (0..channels)
            .map(|_| crossover::highpass(CrossoverKind::LinkwitzRiley, order, freq, fs))
            .collect();
        let rumble_on = rumble_hz > 1.0;
        let rumble = (0..channels)
            .map(|_| {
                if rumble_on {
                    // 4th-order LR rumble filter.
                    crossover::highpass(CrossoverKind::LinkwitzRiley, 4, rumble_hz, fs)
                } else {
                    BiquadChain::new()
                }
            })
            .collect();
        Self {
            enabled,
            channels,
            sub_channel: sub_channel.filter(|&c| c < channels),
            sub_gain: crate::db_to_linear(sub_gain_db),
            lp,
            hp,
            rumble,
            rumble_on,
        }
    }

    /// Process an interleaved buffer (`frames * channels`) in place.
    pub fn process(&mut self, buf: &mut [f64], frames: usize) {
        if !self.enabled {
            return;
        }
        let n = self.channels;
        // Number of "main" channels to high-pass and mono-sum (exclude the sub).
        for f in 0..frames {
            // Mono low content from the main channels.
            let mut sum = 0.0;
            let mut mains = 0;
            for c in 0..n {
                if Some(c) != self.sub_channel {
                    sum += buf[f * n + c];
                    mains += 1;
                }
            }
            let mono_in = if mains > 0 { sum / mains as f64 } else { 0.0 };
            let low = self.lp.process_sample(mono_in);

            for c in 0..n {
                let idx = f * n + c;
                if Some(c) == self.sub_channel {
                    // Dedicated sub: only the summed low band.
                    let mut y = low * self.sub_gain;
                    if self.rumble_on {
                        y = self.rumble[c].process_sample(y);
                    }
                    buf[idx] = y;
                } else {
                    let hi = self.hp[c].process_sample(buf[idx]);
                    // No sub channel => fold the mono bass back into the mains.
                    let mut y = if self.sub_channel.is_some() { hi } else { hi + low };
                    if self.rumble_on {
                        y = self.rumble[c].process_sample(y);
                    }
                    buf[idx] = y;
                }
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear_to_db;

    const FS: f64 = 48_000.0;

    fn rms(buf: &[f64], ch: usize, n: usize) -> f64 {
        let mut s = 0.0;
        let mut count = 0;
        let mut i = ch;
        while i < buf.len() {
            s += buf[i] * buf[i];
            count += 1;
            i += n;
        }
        (s / count as f64).sqrt()
    }

    fn run(bm: &mut BassManager, gen: impl Fn(usize) -> [f64; 2], frames: usize) -> Vec<f64> {
        let mut buf = vec![0.0; frames * 2];
        for f in 0..frames {
            let [l, r] = gen(f);
            buf[f * 2] = l;
            buf[f * 2 + 1] = r;
        }
        bm.process(&mut buf, frames);
        buf
    }

    #[test]
    fn low_tone_panned_left_becomes_mono() {
        // 30 Hz (well below the 80 Hz crossover) panned hard left should appear
        // roughly equally in both outputs (mono bass).
        let mut bm = BassManager::new(FS, 2, true, 80.0, 4, 0.0, None, 0.0);
        let step = std::f64::consts::TAU * 30.0 / FS;
        let out = run(&mut bm, |f| [(f as f64 * step).sin() * 0.5, 0.0], 8192);
        // Skip the filter warm-up.
        let tail = &out[3000 * 2..];
        let l = rms(tail, 0, 2);
        let r = rms(tail, 1, 2);
        // Mono bass = 0.25 peak (half of the 0.5 input) ⇒ ~0.177 RMS.
        assert!(r > 0.15, "bass not summed into the silent channel: R={r}");
        assert!(
            (linear_to_db(l) - linear_to_db(r)).abs() < 1.5,
            "L/R bass not balanced: L={l} R={r}"
        );
    }

    #[test]
    fn high_tone_stays_in_its_channel() {
        // 2 kHz (above crossover) panned left should stay left.
        let mut bm = BassManager::new(FS, 2, true, 80.0, 4, 0.0, None, 0.0);
        let step = std::f64::consts::TAU * 2000.0 / FS;
        let out = run(&mut bm, |f| [(f as f64 * step).sin(), 0.0], 4096);
        let tail = &out[1000 * 2..];
        let l = rms(tail, 0, 2);
        let r = rms(tail, 1, 2);
        assert!(l > 0.3, "high tone lost from left: {l}");
        assert!(r < 0.05, "high tone leaked to right: {r}");
    }

    #[test]
    fn disabled_is_passthrough() {
        let mut bm = BassManager::new(FS, 2, false, 80.0, 4, 0.0, None, 0.0);
        let mut buf = vec![0.3, -0.4, 0.1, 0.9];
        let orig = buf.clone();
        bm.process(&mut buf, 2);
        assert_eq!(buf, orig);
    }

    #[test]
    fn dedicated_sub_gets_bass_mains_get_highs() {
        // 3 channels: L, R mains + sub at index 2.
        let mut bm = BassManager::new(FS, 3, true, 80.0, 4, 0.0, Some(2), 0.0);
        let step = 2.0 * std::f64::consts::TAU * 40.0 / FS;
        let mut buf = vec![0.0; 4096 * 3];
        for f in 0..4096 {
            let s = (f as f64 * step / 2.0).sin();
            buf[f * 3] = s; // L
            buf[f * 3 + 1] = s; // R
            buf[f * 3 + 2] = 0.0; // sub (filled by the manager)
        }
        bm.process(&mut buf, 4096);
        let tail = &buf[1500 * 3..];
        let sub = rms(tail, 2, 3);
        let l = rms(tail, 0, 3);
        assert!(sub > 0.3, "sub got no bass: {sub}");
        assert!(l < 0.1, "mains kept the bass: {l}");
    }
}
