//! Second-order IIR sections (biquads) and cascades.
//!
//! Coefficients follow Robert Bristow-Johnson's *Audio EQ Cookbook*. Filtering
//! uses Direct Form II Transposed, which has good numerical behaviour in `f64`.

use crate::MonoProcessor;
use std::f64::consts::PI;

/// Normalized biquad coefficients (the `a0` term is divided out, so `a0 == 1`).
///
/// Transfer function: `H(z) = (b0 + b1 z⁻¹ + b2 z⁻²) / (1 + a1 z⁻¹ + a2 z⁻²)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Default for Coeffs {
    fn default() -> Self {
        Coeffs::IDENTITY
    }
}

impl Coeffs {
    /// Pass-through (unity gain, no filtering).
    pub const IDENTITY: Coeffs = Coeffs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// Build from raw (un-normalized) coefficients, dividing through by `a0`.
    #[inline]
    pub fn from_raw(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Coeffs {
        Coeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// First-order low pass (6 dB/oct) via the bilinear transform.
    pub fn lowpass_1st(fs: f64, f0: f64) -> Coeffs {
        let k = (PI * f0 / fs).tan();
        let n = 1.0 + k;
        Coeffs {
            b0: k / n,
            b1: k / n,
            b2: 0.0,
            a1: (k - 1.0) / n,
            a2: 0.0,
        }
    }

    /// First-order high pass (6 dB/oct) via the bilinear transform.
    pub fn highpass_1st(fs: f64, f0: f64) -> Coeffs {
        let k = (PI * f0 / fs).tan();
        let n = 1.0 + k;
        Coeffs {
            b0: 1.0 / n,
            b1: -1.0 / n,
            b2: 0.0,
            a1: (k - 1.0) / n,
            a2: 0.0,
        }
    }

    /// Second-order low pass with quality factor `q`.
    pub fn lowpass(fs: f64, f0: f64, q: f64) -> Coeffs {
        let (w0, cw, _sw, alpha) = rbj_terms(fs, f0, q);
        let _ = w0;
        Coeffs::from_raw(
            (1.0 - cw) / 2.0,
            1.0 - cw,
            (1.0 - cw) / 2.0,
            1.0 + alpha,
            -2.0 * cw,
            1.0 - alpha,
        )
    }

    /// Second-order high pass with quality factor `q`.
    pub fn highpass(fs: f64, f0: f64, q: f64) -> Coeffs {
        let (_w0, cw, _sw, alpha) = rbj_terms(fs, f0, q);
        Coeffs::from_raw(
            (1.0 + cw) / 2.0,
            -(1.0 + cw),
            (1.0 + cw) / 2.0,
            1.0 + alpha,
            -2.0 * cw,
            1.0 - alpha,
        )
    }

    /// Band pass with 0 dB peak gain (constant peak gain) at `f0`.
    pub fn bandpass(fs: f64, f0: f64, q: f64) -> Coeffs {
        let (_w0, cw, _sw, alpha) = rbj_terms(fs, f0, q);
        Coeffs::from_raw(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
    }

    /// Notch (band-reject) at `f0`.
    pub fn notch(fs: f64, f0: f64, q: f64) -> Coeffs {
        let (_w0, cw, _sw, alpha) = rbj_terms(fs, f0, q);
        Coeffs::from_raw(1.0, -2.0 * cw, 1.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
    }

    /// All pass (flat magnitude, frequency-dependent phase) at `f0`.
    pub fn allpass(fs: f64, f0: f64, q: f64) -> Coeffs {
        let (_w0, cw, _sw, alpha) = rbj_terms(fs, f0, q);
        Coeffs::from_raw(
            1.0 - alpha,
            -2.0 * cw,
            1.0 + alpha,
            1.0 + alpha,
            -2.0 * cw,
            1.0 - alpha,
        )
    }

    /// Peaking (bell) EQ: `gain_db` boost/cut at `f0` with bandwidth set by `q`.
    pub fn peaking(fs: f64, f0: f64, q: f64, gain_db: f64) -> Coeffs {
        let (_w0, cw, _sw, alpha) = rbj_terms(fs, f0, q);
        let a = 10.0_f64.powf(gain_db / 40.0);
        Coeffs::from_raw(
            1.0 + alpha * a,
            -2.0 * cw,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cw,
            1.0 - alpha / a,
        )
    }

    /// Low shelf: `gain_db` applied below `f0` (slope set by `q`).
    pub fn low_shelf(fs: f64, f0: f64, q: f64, gain_db: f64) -> Coeffs {
        let (_w0, cw, _sw, alpha) = rbj_terms(fs, f0, q);
        let a = 10.0_f64.powf(gain_db / 40.0);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        Coeffs::from_raw(
            a * ((a + 1.0) - (a - 1.0) * cw + two_sqrt_a_alpha),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cw),
            a * ((a + 1.0) - (a - 1.0) * cw - two_sqrt_a_alpha),
            (a + 1.0) + (a - 1.0) * cw + two_sqrt_a_alpha,
            -2.0 * ((a - 1.0) + (a + 1.0) * cw),
            (a + 1.0) + (a - 1.0) * cw - two_sqrt_a_alpha,
        )
    }

    /// High shelf: `gain_db` applied above `f0` (slope set by `q`).
    pub fn high_shelf(fs: f64, f0: f64, q: f64, gain_db: f64) -> Coeffs {
        let (_w0, cw, _sw, alpha) = rbj_terms(fs, f0, q);
        let a = 10.0_f64.powf(gain_db / 40.0);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        Coeffs::from_raw(
            a * ((a + 1.0) + (a - 1.0) * cw + two_sqrt_a_alpha),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cw),
            a * ((a + 1.0) + (a - 1.0) * cw - two_sqrt_a_alpha),
            (a + 1.0) - (a - 1.0) * cw + two_sqrt_a_alpha,
            2.0 * ((a - 1.0) - (a + 1.0) * cw),
            (a + 1.0) - (a - 1.0) * cw - two_sqrt_a_alpha,
        )
    }

    /// Linear magnitude of the frequency response at `f` Hz (sample rate `fs`).
    pub fn magnitude(&self, f: f64, fs: f64) -> f64 {
        let w = 2.0 * PI * f / fs;
        let (sw, cw) = w.sin_cos();
        let (s2w, c2w) = (2.0 * w).sin_cos();
        // Numerator: b0 + b1 z⁻¹ + b2 z⁻², with z⁻¹ = e^{-jw}.
        let num_re = self.b0 + self.b1 * cw + self.b2 * c2w;
        let num_im = -(self.b1 * sw + self.b2 * s2w);
        let den_re = 1.0 + self.a1 * cw + self.a2 * c2w;
        let den_im = -(self.a1 * sw + self.a2 * s2w);
        let num = (num_re * num_re + num_im * num_im).sqrt();
        let den = (den_re * den_re + den_im * den_im).sqrt();
        num / den
    }

    /// Frequency response magnitude in decibels at `f` Hz.
    pub fn magnitude_db(&self, f: f64, fs: f64) -> f64 {
        20.0 * self.magnitude(f, fs).log10()
    }
}

/// Shared RBJ intermediate terms: `(w0, cos w0, sin w0, alpha)`.
#[inline]
fn rbj_terms(fs: f64, f0: f64, q: f64) -> (f64, f64, f64, f64) {
    let w0 = 2.0 * PI * f0 / fs;
    let (sw, cw) = w0.sin_cos();
    let alpha = sw / (2.0 * q);
    (w0, cw, sw, alpha)
}

/// A single second-order section with its own state.
#[derive(Debug, Clone)]
pub struct Biquad {
    c: Coeffs,
    z1: f64,
    z2: f64,
}

impl Biquad {
    pub fn new(c: Coeffs) -> Self {
        Self {
            c,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Replace coefficients while preserving the filter's state memory
    /// (suitable for click-free live retuning when changes are small).
    #[inline]
    pub fn set_coeffs(&mut self, c: Coeffs) {
        self.c = c;
    }

    pub fn coeffs(&self) -> Coeffs {
        self.c
    }
}

impl MonoProcessor for Biquad {
    #[inline]
    fn process_sample(&mut self, x: f64) -> f64 {
        let y = self.c.b0 * x + self.z1;
        self.z1 = self.c.b1 * x - self.c.a1 * y + self.z2;
        self.z2 = self.c.b2 * x - self.c.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// A cascade of biquads processed in series (e.g. a multi-band EQ or a
/// higher-order filter built from second-order sections).
#[derive(Debug, Clone, Default)]
pub struct BiquadChain {
    stages: Vec<Biquad>,
}

impl BiquadChain {
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    /// Build a chain from an iterator of coefficients.
    pub fn from_coeffs<I: IntoIterator<Item = Coeffs>>(coeffs: I) -> Self {
        Self {
            stages: coeffs.into_iter().map(Biquad::new).collect(),
        }
    }

    pub fn push(&mut self, c: Coeffs) {
        self.stages.push(Biquad::new(c));
    }

    /// Append all stages of another chain onto this one.
    pub fn extend(&mut self, other: &BiquadChain) {
        self.stages.extend(other.stages.iter().cloned());
    }

    pub fn len(&self) -> usize {
        self.stages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Combined frequency-response magnitude (dB) of the whole cascade.
    pub fn magnitude_db(&self, f: f64, fs: f64) -> f64 {
        self.stages.iter().map(|s| s.coeffs().magnitude_db(f, fs)).sum()
    }
}

impl MonoProcessor for BiquadChain {
    #[inline]
    fn process_sample(&mut self, x: f64) -> f64 {
        let mut s = x;
        for stage in &mut self.stages {
            s = stage.process_sample(s);
        }
        s
    }

    fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;

    #[test]
    fn identity_passes_through() {
        let mut b = Biquad::new(Coeffs::IDENTITY);
        for x in [0.1, -0.5, 0.9, 0.0, 0.3] {
            assert!((b.process_sample(x) - x).abs() < 1e-15);
        }
    }

    #[test]
    fn peaking_gain_at_center() {
        // +6 dB peaking at 1 kHz should give ~+6 dB at exactly 1 kHz.
        let c = Coeffs::peaking(FS, 1000.0, 1.0, 6.0);
        assert!((c.magnitude_db(1000.0, FS) - 6.0).abs() < 1e-6);
        // Far away it should be ~unity (0 dB).
        assert!(c.magnitude_db(50.0, FS).abs() < 0.5);
        assert!(c.magnitude_db(18_000.0, FS).abs() < 0.5);
    }

    #[test]
    fn lowpass_minus_3db_at_cutoff() {
        // Butterworth Q gives -3 dB at the cutoff frequency.
        let c = Coeffs::lowpass(FS, 1000.0, std::f64::consts::FRAC_1_SQRT_2);
        assert!((c.magnitude_db(1000.0, FS) + 3.0103).abs() < 1e-2);
        // Passband ~0 dB, stopband well attenuated.
        assert!(c.magnitude_db(100.0, FS).abs() < 0.1);
        assert!(c.magnitude_db(8000.0, FS) < -30.0);
    }

    #[test]
    fn highpass_complements_lowpass_dc() {
        let c = Coeffs::highpass(FS, 1000.0, std::f64::consts::FRAC_1_SQRT_2);
        assert!((c.magnitude_db(1000.0, FS) + 3.0103).abs() < 1e-2);
        assert!(c.magnitude_db(20.0, FS) < -30.0);
        assert!(c.magnitude_db(16_000.0, FS).abs() < 0.2);
    }

    #[test]
    fn shelves_reach_target_gain() {
        let ls = Coeffs::low_shelf(FS, 200.0, 0.707, -4.0);
        assert!((ls.magnitude_db(20.0, FS) + 4.0).abs() < 0.2);
        assert!(ls.magnitude_db(15_000.0, FS).abs() < 0.2);

        let hs = Coeffs::high_shelf(FS, 4000.0, 0.707, 5.0);
        assert!((hs.magnitude_db(20_000.0, FS) - 5.0).abs() < 0.3);
        assert!(hs.magnitude_db(100.0, FS).abs() < 0.2);
    }

    #[test]
    fn allpass_is_flat() {
        let c = Coeffs::allpass(FS, 1000.0, 0.7);
        for f in [50.0, 500.0, 1000.0, 5000.0, 18_000.0] {
            assert!(c.magnitude_db(f, FS).abs() < 1e-6, "f={f}");
        }
    }
}
