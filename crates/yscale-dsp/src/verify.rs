//! Convolution-based verification tooling for **linear time-invariant** blocks.
//!
//! Convolution is the defining operation of an LTI system: its output equals the
//! input convolved with its impulse response (IR). That makes convolution an
//! *independent oracle* for our filters. This module provides the primitives to:
//!
//! 1. capture a processor's [`impulse_response`];
//! 2. [`convolve`] an arbitrary signal with that IR;
//! 3. compare the IR's spectrum ([`dtft_magnitude`]) against an analytic transfer
//!    function — a time-domain ↔ frequency-domain cross-check.
//!
//! If direct processing equals convolution-with-IR *and* the IR's DTFT matches
//! the analytic response, the block is provably correct to numerical tolerance.
//!
//! **Scope.** This is only valid for LTI blocks: [`Biquad`](crate::Biquad),
//! [`BiquadChain`](crate::BiquadChain), EQs, crossovers, [`Delay`](crate::Delay)
//! and the [`ChannelMatrix`](crate::ChannelMatrix) (per output). It says nothing
//! about time-variant or nonlinear stages (signal generators, mute toggling,
//! clipping, dither) — those are not convolutions and must be checked otherwise.

use crate::MonoProcessor;
use std::f64::consts::PI;

/// Capture the first `len` samples of a processor's impulse response.
///
/// The processor is reset before and after so this is a pure measurement.
pub fn impulse_response<P: MonoProcessor>(p: &mut P, len: usize) -> Vec<f64> {
    p.reset();
    let mut ir = Vec::with_capacity(len);
    for n in 0..len {
        let x = if n == 0 { 1.0 } else { 0.0 };
        ir.push(p.process_sample(x));
    }
    p.reset();
    ir
}

/// Direct linear convolution `y = x * h`. Output length is
/// `x.len() + h.len() - 1` (empty if either input is empty).
pub fn convolve(x: &[f64], h: &[f64]) -> Vec<f64> {
    if x.is_empty() || h.is_empty() {
        return Vec::new();
    }
    let mut y = vec![0.0; x.len() + h.len() - 1];
    for (i, &xi) in x.iter().enumerate() {
        if xi == 0.0 {
            continue;
        }
        for (j, &hj) in h.iter().enumerate() {
            y[i + j] += xi * hj;
        }
    }
    y
}

/// Discrete-time Fourier transform magnitude of a finite sequence `h` evaluated
/// at frequency `f` Hz (sample rate `fs`). A single-bin DTFT — `O(N)`, no FFT
/// dependency, exact at the requested frequency.
pub fn dtft_magnitude(h: &[f64], f: f64, fs: f64) -> f64 {
    let w = 2.0 * PI * f / fs;
    let mut re = 0.0;
    let mut im = 0.0;
    for (n, &hn) in h.iter().enumerate() {
        let phase = w * n as f64;
        re += hn * phase.cos();
        im -= hn * phase.sin();
    }
    (re * re + im * im).sqrt()
}

/// DTFT magnitude in decibels.
pub fn dtft_magnitude_db(h: &[f64], f: f64, fs: f64) -> f64 {
    20.0 * dtft_magnitude(h, f, fs).log10()
}

/// Maximum absolute sample error between running `p` directly on `input` and
/// convolving `input` with `p`'s impulse response.
///
/// For a true LTI block this is ~0 (float rounding) for every output index
/// strictly less than the IR length, regardless of IIR tails — those indices
/// only ever touch IR taps that were actually captured.
pub fn lti_residual<P: MonoProcessor>(p: &mut P, input: &[f64], ir_len: usize) -> f64 {
    let ir = impulse_response(p, ir_len);
    p.reset();
    let direct: Vec<f64> = input.iter().map(|&x| p.process_sample(x)).collect();
    p.reset();
    let conv = convolve(input, &ir);
    let mut max = 0.0_f64;
    // Only compare indices the captured IR fully covers.
    let n = input.len().min(ir_len);
    for i in 0..n {
        let d = (direct[i] - conv[i]).abs();
        if d > max {
            max = d;
        }
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biquad::{BiquadChain, Coeffs};
    use crate::delay::Delay;
    use crate::eq::{Band, ParametricEq};

    const FS: f64 = 48_000.0;

    fn pseudo_random(n: usize) -> Vec<f64> {
        // Deterministic bipolar sequence (xorshift) — no rand dependency.
        let mut s: u64 = 0x1234_5678_9abc_def1;
        (0..n)
            .map(|_| {
                s ^= s >> 12;
                s ^= s << 25;
                s ^= s >> 27;
                let u = (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64;
                2.0 * u - 1.0
            })
            .collect()
    }

    #[test]
    fn biquad_chain_equals_its_own_convolution() {
        // The whole point: direct processing == convolution with the IR.
        let mut chain = ParametricEq::from_bands([
            Band::peaking(120.0, 1.2, 5.0),
            Band::low_shelf(80.0, 0.7, -4.0),
            Band::high_shelf(6000.0, 0.7, 3.0),
        ])
        .to_chain(FS);

        let input = pseudo_random(1024);
        // IR longer than the input -> compared indices are truncation-free.
        let residual = lti_residual(&mut chain, &input, 2048);
        assert!(residual < 1e-9, "LTI residual too large: {residual}");
    }

    #[test]
    fn ir_spectrum_matches_analytic_transfer_function() {
        // Time-domain (DTFT of IR) must equal frequency-domain (analytic) response.
        let coeffs = Coeffs::peaking(FS, 1000.0, 1.0, 6.0);
        let mut chain = BiquadChain::from_coeffs([coeffs]);
        let ir = impulse_response(&mut chain, 1 << 15); // long enough for IIR decay

        for f in [50.0, 200.0, 1000.0, 3000.0, 10_000.0] {
            let measured = dtft_magnitude_db(&ir, f, FS);
            let analytic = coeffs.magnitude_db(f, FS);
            assert!(
                (measured - analytic).abs() < 0.05,
                "f={f}: measured {measured} dB vs analytic {analytic} dB"
            );
        }
    }

    #[test]
    fn delay_ir_is_a_shifted_impulse_and_convolves() {
        let mut d = Delay::new(64.0);
        d.set_delay_samples(7.0);
        // Convolving any signal with the delay IR == delaying it directly.
        let input = pseudo_random(256);
        let residual = lti_residual(&mut d, &input, 512);
        assert!(residual < 1e-9, "delay LTI residual: {residual}");
    }

    #[test]
    fn convolve_identity() {
        let x = [1.0, 2.0, 3.0];
        let unit = [1.0];
        assert_eq!(convolve(&x, &unit), vec![1.0, 2.0, 3.0]);
        // Shifting kernel delays the signal.
        let shift = [0.0, 1.0];
        assert_eq!(convolve(&x, &shift), vec![0.0, 1.0, 2.0, 3.0]);
    }
}
