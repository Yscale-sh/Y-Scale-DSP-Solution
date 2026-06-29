//! Crossover filters for splitting a signal into frequency bands (e.g. feeding a
//! woofer and a tweeter from the two RCA outputs).
//!
//! Supports Butterworth (6/12/18/24 dB/oct) and Linkwitz-Riley (12/24 dB/oct).
//! Linkwitz-Riley of order `2N` is built as two cascaded Butterworth filters of
//! order `N`, giving the in-phase, flat-summing response speaker builders want.

use crate::biquad::{BiquadChain, Coeffs};
use std::f64::consts::PI;

/// Crossover alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossoverKind {
    /// Maximally-flat Butterworth. Orders 1..=4 (6/12/18/24 dB per octave).
    Butterworth,
    /// Linkwitz-Riley. Orders 2 and 4 (12/24 dB per octave). The order must be
    /// even; it is realized as two Butterworth filters of half the order.
    LinkwitzRiley,
}

/// A high/low pass pair forming one crossover point.
#[derive(Debug, Clone)]
pub struct Crossover {
    pub lowpass: BiquadChain,
    pub highpass: BiquadChain,
}

impl Crossover {
    /// Build a complementary low/high pass pair at `fc` Hz.
    pub fn new(kind: CrossoverKind, order: usize, fc: f64, fs: f64) -> Crossover {
        Crossover {
            lowpass: lowpass(kind, order, fc, fs),
            highpass: highpass(kind, order, fc, fs),
        }
    }
}

/// Butterworth second-order-section Q values for a given filter `order`, plus a
/// flag indicating whether a trailing first-order section is required (odd order).
fn butterworth_section_qs(order: usize) -> (Vec<f64>, bool) {
    match order {
        0 => (vec![], false),
        1 => (vec![], true),
        n => {
            let pairs = n / 2;
            let odd = n % 2 == 1;
            let mut qs = Vec::with_capacity(pairs);
            if odd {
                // Odd order: complex-pair Qs are 1 / (2 cos(k·π / n)), k = 1..=pairs.
                for k in 1..=pairs {
                    qs.push(1.0 / (2.0 * ((k as f64) * PI / n as f64).cos()));
                }
            } else {
                // Even order: Qs are 1 / (2 cos((2k−1)·π / (2n))), k = 1..=pairs.
                for k in 1..=pairs {
                    let theta = (2.0 * k as f64 - 1.0) * PI / (2.0 * n as f64);
                    qs.push(1.0 / (2.0 * theta.cos()));
                }
            }
            (qs, odd)
        }
    }
}

fn butterworth_lowpass(order: usize, fc: f64, fs: f64) -> BiquadChain {
    let (qs, odd) = butterworth_section_qs(order);
    let mut chain = BiquadChain::new();
    for q in qs {
        chain.push(Coeffs::lowpass(fs, fc, q));
    }
    if odd {
        chain.push(Coeffs::lowpass_1st(fs, fc));
    }
    chain
}

fn butterworth_highpass(order: usize, fc: f64, fs: f64) -> BiquadChain {
    let (qs, odd) = butterworth_section_qs(order);
    let mut chain = BiquadChain::new();
    for q in qs {
        chain.push(Coeffs::highpass(fs, fc, q));
    }
    if odd {
        chain.push(Coeffs::highpass_1st(fs, fc));
    }
    chain
}

/// Low pass section of a crossover.
pub fn lowpass(kind: CrossoverKind, order: usize, fc: f64, fs: f64) -> BiquadChain {
    match kind {
        CrossoverKind::Butterworth => butterworth_lowpass(order, fc, fs),
        CrossoverKind::LinkwitzRiley => {
            assert!(order % 2 == 0, "Linkwitz-Riley order must be even");
            let half = order / 2;
            let mut chain = butterworth_lowpass(half, fc, fs);
            chain.extend(&butterworth_lowpass(half, fc, fs));
            chain
        }
    }
}

/// High pass section of a crossover.
pub fn highpass(kind: CrossoverKind, order: usize, fc: f64, fs: f64) -> BiquadChain {
    match kind {
        CrossoverKind::Butterworth => butterworth_highpass(order, fc, fs),
        CrossoverKind::LinkwitzRiley => {
            assert!(order % 2 == 0, "Linkwitz-Riley order must be even");
            let half = order / 2;
            let mut chain = butterworth_highpass(half, fc, fs);
            chain.extend(&butterworth_highpass(half, fc, fs));
            chain
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;
    const FC: f64 = 1000.0;

    #[test]
    fn butterworth_lp_is_minus_3db_at_fc() {
        for order in 1..=4 {
            let lp = butterworth_lowpass(order, FC, FS);
            let db = lp.magnitude_db(FC, FS);
            assert!((db + 3.0103).abs() < 0.05, "order {order}: {db} dB at fc");
        }
    }

    #[test]
    fn butterworth_slope_steepens_with_order() {
        // One octave above fc, attenuation should be roughly order * 6 dB.
        for order in 1..=4 {
            let lp = butterworth_lowpass(order, FC, FS);
            let db = lp.magnitude_db(2.0 * FC, FS);
            let expected = -6.0206 * order as f64;
            assert!((db - expected).abs() < 1.5, "order {order}: {db} vs {expected}");
        }
    }

    #[test]
    fn linkwitz_riley_lp_is_minus_6db_at_fc() {
        for order in [2, 4] {
            let lp = lowpass(CrossoverKind::LinkwitzRiley, order, FC, FS);
            let db = lp.magnitude_db(FC, FS);
            assert!((db + 6.0206).abs() < 0.1, "LR{order}: {db} dB at fc");
        }
    }

    #[test]
    fn linkwitz_riley_sums_flat() {
        // LR low + high pass voltages sum to (near) unity magnitude everywhere.
        let xo = Crossover::new(CrossoverKind::LinkwitzRiley, 4, FC, FS);
        for f in [50.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 15000.0] {
            // LR4 sums in phase, so magnitudes add coherently to ~0 dB.
            let lp = xo.lowpass.magnitude_db(f, FS);
            let hp = xo.highpass.magnitude_db(f, FS);
            let lp_lin = 10.0_f64.powf(lp / 20.0);
            let hp_lin = 10.0_f64.powf(hp / 20.0);
            let sum_db = 20.0 * (lp_lin + hp_lin).log10();
            assert!(sum_db.abs() < 0.35, "f={f}: sum {sum_db} dB");
        }
    }
}
