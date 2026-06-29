//! A per-output-channel processing strip: time-alignment delay → filter cascade
//! (EQ + crossover) → gain / polarity / mute.

use crate::biquad::BiquadChain;
use crate::delay::Delay;
use crate::{db_to_linear, MonoProcessor};

/// One output channel's signal chain.
#[derive(Debug, Clone)]
pub struct ChannelStrip {
    /// Time-alignment delay (applied first).
    pub delay: Delay,
    /// Combined EQ + crossover biquad cascade.
    pub filters: BiquadChain,
    gain_linear: f64,
    polarity: f64,
    muted: bool,
}

impl ChannelStrip {
    /// A unity, pass-through strip with `max_delay_samples` of delay headroom.
    pub fn new(max_delay_samples: f64) -> Self {
        Self {
            delay: Delay::new(max_delay_samples),
            filters: BiquadChain::new(),
            gain_linear: 1.0,
            polarity: 1.0,
            muted: false,
        }
    }

    pub fn set_filters(&mut self, filters: BiquadChain) {
        self.filters = filters;
    }

    pub fn set_gain_db(&mut self, gain_db: f64) {
        self.gain_linear = db_to_linear(gain_db);
    }

    pub fn set_gain_linear(&mut self, gain: f64) {
        self.gain_linear = gain;
    }

    /// `true` inverts polarity (180°), `false` leaves it normal.
    pub fn set_inverted(&mut self, inverted: bool) {
        self.polarity = if inverted { -1.0 } else { 1.0 };
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }
}

impl MonoProcessor for ChannelStrip {
    #[inline]
    fn process_sample(&mut self, x: f64) -> f64 {
        // Always run the delay/filter state so toggling mute stays click-light
        // and the filter memory remains coherent.
        let d = self.delay.process_sample(x);
        let y = self.filters.process_sample(d);
        if self.muted {
            0.0
        } else {
            y * self.gain_linear * self.polarity
        }
    }

    fn reset(&mut self) {
        self.delay.reset();
        self.filters.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_strip_passes_through_with_latency() {
        let mut s = ChannelStrip::new(16.0);
        // 1-sample delay latency, no filters, unity gain.
        let _ = s.process_sample(1.0);
        let y = s.process_sample(0.0);
        assert!((y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn mute_silences_output() {
        let mut s = ChannelStrip::new(16.0);
        s.set_muted(true);
        for _ in 0..8 {
            assert_eq!(s.process_sample(0.9), 0.0);
        }
    }

    #[test]
    fn polarity_inverts() {
        let mut s = ChannelStrip::new(16.0);
        s.set_inverted(true);
        let _ = s.process_sample(1.0);
        let y = s.process_sample(0.0);
        assert!((y + 1.0).abs() < 1e-9);
    }

    #[test]
    fn gain_scales() {
        let mut s = ChannelStrip::new(16.0);
        s.set_gain_db(6.0206);
        let _ = s.process_sample(1.0);
        let y = s.process_sample(0.0);
        assert!((y - 2.0).abs() < 1e-3);
    }
}
