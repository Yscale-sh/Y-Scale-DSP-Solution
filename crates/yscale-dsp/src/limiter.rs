//! Look-ahead, channel-linked **brickwall limiter** — the final safety stage so
//! the DSP can never drive the DAC past its ceiling, no matter how much EQ or
//! gain is dialed in upstream.
//!
//! Design:
//! - **Linked** gain reduction: one envelope derived from the peak across *all*
//!   channels, so the stereo image never shifts when one channel limits.
//! - **Look-ahead**: the detector reads the signal a couple ms early (via a
//!   delay line on the audio), so the gain ducks *before* a transient arrives —
//!   transparent, not pumping.
//! - **Hard ceiling clamp** after the smoothed gain guarantees a true brickwall
//!   even for the rare transient the smoothing can't fully catch.
//!
//! It is sample-peak (not oversampled true-peak); the default ceiling leaves
//! headroom for inter-sample overs at the DAC's reconstruction filter.

use crate::{db_to_linear, linear_to_db};

/// A multi-channel look-ahead brickwall limiter operating on interleaved `f64`.
pub struct Limiter {
    enabled: bool,
    channels: usize,
    ceiling: f64, // linear amplitude ceiling
    la: usize,    // look-ahead, in frames (>= 1)
    delay: Vec<f64>, // ring buffer: channels * la
    wpos: usize,  // ring write position (frame index)
    env: f64,     // current gain (<= 1.0; 1.0 = no reduction)
    atk: f64,     // attack one-pole coefficient (gain moving down)
    rel: f64,     // release one-pole coefficient (gain recovering up)
    gr: f64,      // last block's max gain reduction in dB (>= 0), for metering
}

impl Limiter {
    /// Build a limiter. `lookahead_ms` sets both the latency and the attack
    /// smoothing (attack ≈ lookahead/3 so the gain reaches target right as the
    /// peak emerges).
    pub fn new(
        fs: f64,
        channels: usize,
        ceiling_db: f64,
        lookahead_ms: f64,
        release_ms: f64,
        enabled: bool,
    ) -> Self {
        let la = ((lookahead_ms * 1e-3 * fs).round() as usize).max(1);
        let attack_ms = (lookahead_ms / 3.0).max(0.05);
        Self {
            enabled,
            channels: channels.max(1),
            ceiling: db_to_linear(ceiling_db),
            la,
            delay: vec![0.0; channels.max(1) * la],
            wpos: 0,
            env: 1.0,
            atk: one_pole_coeff(attack_ms, fs),
            rel: one_pole_coeff(release_ms, fs),
            gr: 0.0,
        }
    }

    /// Limit an interleaved buffer (`frames * channels`) in place.
    pub fn process(&mut self, buf: &mut [f64], frames: usize) {
        if !self.enabled {
            self.gr = 0.0;
            return;
        }
        let n = self.channels;
        let mut min_env = 1.0f64; // most reduction this block (for metering)

        for f in 0..frames {
            // Linked detector: peak across all channels of the incoming frame.
            let mut peak = 0.0;
            for c in 0..n {
                let a = buf[f * n + c].abs();
                if a > peak {
                    peak = a;
                }
            }
            let desired = if peak > self.ceiling {
                self.ceiling / peak
            } else {
                1.0
            };
            // Fast attack down, slow release up. Because the audio is delayed by
            // `la` frames, a drop triggered now lands on the output ~la frames
            // before the peak it was caused by — that's the look-ahead.
            if desired < self.env {
                self.env += (desired - self.env) * self.atk;
            } else {
                self.env += (desired - self.env) * self.rel;
            }
            if self.env < min_env {
                min_env = self.env;
            }

            // Emit the delayed frame * gain, hard-clamped to the ceiling.
            let base = self.wpos * n;
            for c in 0..n {
                let idx = base + c;
                let delayed = self.delay[idx];
                self.delay[idx] = buf[f * n + c];
                buf[f * n + c] = (delayed * self.env).clamp(-self.ceiling, self.ceiling);
            }
            self.wpos += 1;
            if self.wpos >= self.la {
                self.wpos = 0;
            }
        }

        self.gr = if min_env < 1.0 {
            -linear_to_db(min_env)
        } else {
            0.0
        };
    }

    /// Max gain reduction from the last processed block, in dB (>= 0).
    pub fn gain_reduction_db(&self) -> f64 {
        self.gr
    }

    /// Output latency introduced by the look-ahead, in samples.
    pub fn latency_samples(&self) -> usize {
        if self.enabled {
            self.la
        } else {
            0
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// One-pole smoothing coefficient for a given time constant (ms).
fn one_pole_coeff(ms: f64, fs: f64) -> f64 {
    if ms <= 0.0 {
        return 1.0;
    }
    1.0 - (-1.0 / (ms * 1e-3 * fs)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The brickwall guarantee: no output sample exceeds the ceiling, even when
    /// fed a signal far above it.
    #[test]
    fn never_exceeds_ceiling() {
        let ceiling_db = -1.0;
        let ceiling = db_to_linear(ceiling_db);
        let mut lim = Limiter::new(48_000.0, 2, ceiling_db, 2.0, 100.0, true);
        // 4x full-scale square-ish bursts.
        let mut buf = vec![0.0; 4096 * 2];
        for (i, s) in buf.iter_mut().enumerate() {
            *s = if (i / 64) % 2 == 0 { 4.0 } else { -3.5 };
        }
        // A few blocks so the envelope settles past the look-ahead.
        for _ in 0..8 {
            lim.process(&mut buf, 4096);
            for &s in &buf {
                assert!(
                    s.abs() <= ceiling + 1e-9,
                    "sample {s} exceeds ceiling {ceiling}"
                );
            }
        }
        assert!(lim.gain_reduction_db() > 0.0);
    }

    /// Signal already under the ceiling passes essentially untouched.
    #[test]
    fn transparent_below_ceiling() {
        let mut lim = Limiter::new(48_000.0, 2, -1.0, 2.0, 100.0, true);
        let la = lim.latency_samples();
        let mut buf = vec![0.0; 1024 * 2];
        for f in 0..1024 {
            let v = 0.2 * (f as f64 * 0.05).sin();
            buf[f * 2] = v;
            buf[f * 2 + 1] = v;
        }
        let orig = buf.clone();
        lim.process(&mut buf, 1024);
        // No gain reduction, and output equals input delayed by the look-ahead.
        assert_eq!(lim.gain_reduction_db(), 0.0);
        for f in la..1024 {
            assert!((buf[f * 2] - orig[(f - la) * 2]).abs() < 1e-9);
        }
    }

    #[test]
    fn disabled_is_passthrough() {
        let mut lim = Limiter::new(48_000.0, 2, -1.0, 2.0, 100.0, false);
        let mut buf = vec![3.0; 256 * 2];
        lim.process(&mut buf, 256);
        assert!(buf.iter().all(|&s| s == 3.0));
        assert_eq!(lim.latency_samples(), 0);
    }
}
