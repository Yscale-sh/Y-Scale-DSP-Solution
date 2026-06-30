//! # yscale-dsp
//!
//! The slim, high-fidelity, composable DSP core of the **Y-Scale-DSP-Solution**.
//!
//! Everything processes in `f64` for headroom and numerical stability, the hot
//! path performs no allocation, and the building blocks compose into an
//! arbitrary multi-channel [`Pipeline`]. It is engine-agnostic: feed it planar
//! or interleaved sample buffers; bring your own audio I/O.
//!
//! ## Building blocks
//! - [`biquad`] — RBJ Audio-EQ-Cookbook second-order sections (Direct Form II
//!   Transposed) and cascades.
//! - [`eq`] — parametric EQ and a 30-band ISO 1/3-octave graphic EQ.
//! - [`crossover`] — Butterworth and Linkwitz-Riley high/low pass.
//! - [`delay`] — fractional-sample delay for speaker time alignment.
//! - [`matrix`] — N-in/N-out channel routing (mono/left/right/swap/custom).
//! - [`strip`] / [`pipeline`] — per-channel processing chains wired into a graph.

pub mod biquad;
pub mod crossover;
pub mod delay;
pub mod eq;
pub mod limiter;
pub mod matrix;
pub mod pipeline;
pub mod strip;
pub mod verify;

pub use biquad::{Biquad, BiquadChain, Coeffs};
pub use crossover::{Crossover, CrossoverKind};
pub use delay::Delay;
pub use eq::{Band, BandKind, GraphicEq30, ParametricEq};
pub use limiter::Limiter;
pub use matrix::ChannelMatrix;
pub use pipeline::Pipeline;
pub use strip::ChannelStrip;

/// Speed of sound in dry air at ~20 °C, in metres per second.
pub const SPEED_OF_SOUND_M_S: f64 = 343.0;

/// A mono, sample-by-sample processing stage.
///
/// Implementors keep their own state and must be cheap and allocation-free in
/// [`process_sample`](MonoProcessor::process_sample); any allocation belongs in
/// construction or `set_*` methods.
pub trait MonoProcessor {
    /// Process a single input sample, returning a single output sample.
    fn process_sample(&mut self, x: f64) -> f64;

    /// Process a block of samples in place. The default iterates
    /// [`process_sample`](MonoProcessor::process_sample); implementors may
    /// override for block-level speedups.
    fn process_block(&mut self, buf: &mut [f64]) {
        for s in buf.iter_mut() {
            *s = self.process_sample(*s);
        }
    }

    /// Clear all internal state (delay lines, filter memory).
    fn reset(&mut self);
}

/// Convert a gain in decibels to a linear amplitude multiplier.
#[inline]
pub fn db_to_linear(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

/// Convert a linear amplitude multiplier to decibels.
#[inline]
pub fn linear_to_db(linear: f64) -> f64 {
    20.0 * linear.abs().max(f64::MIN_POSITIVE).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_round_trip() {
        for db in [-60.0, -12.0, -3.0, 0.0, 3.0, 6.0, 12.0] {
            let back = linear_to_db(db_to_linear(db));
            assert!((back - db).abs() < 1e-9, "db {db} -> {back}");
        }
    }

    #[test]
    fn unity_is_zero_db() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-12);
        assert!((db_to_linear(6.0206) - 2.0).abs() < 1e-3);
    }
}
