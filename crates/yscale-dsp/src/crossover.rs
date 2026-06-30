//! Crossover filters for splitting a signal into frequency bands (e.g. feeding a
//! woofer, midrange and tweeter from separate outputs).
//!
//! Alignments:
//! - **Butterworth** — maximally-flat magnitude, 6 dB/oct per order (1..=8).
//! - **Linkwitz-Riley** — even orders (12/24/36/48 dB/oct); two cascaded
//!   Butterworth filters of half the order, giving the in-phase, flat-summing
//!   response speaker builders want.
//! - **Bessel** — maximally-flat *group delay* (best transient response),
//!   orders 1..=4. Built from `-3 dB`-normalized second-order sections.
//!
//! Roles: low-pass, high-pass, and **band-pass** (a high-pass + low-pass pair
//! for a midrange band).

use crate::biquad::{BiquadChain, Coeffs};
use std::f64::consts::PI;

/// Crossover alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossoverKind {
    /// Maximally-flat Butterworth. Orders 1..=8 (6..48 dB per octave).
    Butterworth,
    /// Linkwitz-Riley. Even orders; realized as two Butterworth filters of half
    /// the order (in-phase, flat power summing).
    LinkwitzRiley,
    /// Bessel — maximally-flat group delay. Orders 1..=4.
    Bessel,
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

/// One filter as second-order sections: `(Q, frequency-scaling-factor)` pairs,
/// plus an optional trailing first-order section's scaling factor (odd orders).
/// The FSF places each section's pole so the *composite* response is −3 dB at the
/// nominal cutoff.
struct Sections {
    biquads: Vec<(f64, f64)>,
    first_order: Option<f64>,
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

fn butterworth_sections(order: usize) -> Sections {
    let (qs, odd) = butterworth_section_qs(order);
    Sections {
        biquads: qs.into_iter().map(|q| (q, 1.0)).collect(),
        first_order: odd.then_some(1.0),
    }
}

/// `-3 dB`-normalized Bessel sections, orders 1..=4 (clamped). Each tuple is
/// `(Q, FSF)`; first-order entries carry their own FSF. Values from the standard
/// Bessel filter tables (TI SLOA049 / Analog Devices), validated by the unit
/// tests to land at −3 dB at the cutoff.
fn bessel_sections(order: usize) -> Sections {
    match order.clamp(1, 4) {
        1 => Sections {
            biquads: vec![],
            first_order: Some(1.0),
        },
        2 => Sections {
            biquads: vec![(0.5773, 1.2736)],
            first_order: None,
        },
        3 => Sections {
            biquads: vec![(0.6910, 1.4524)],
            first_order: Some(1.3270),
        },
        _ => Sections {
            biquads: vec![(0.5219, 1.4192), (0.8055, 1.5912)],
            first_order: None,
        },
    }
}

fn build_lowpass(s: &Sections, fc: f64, fs: f64) -> BiquadChain {
    let mut chain = BiquadChain::new();
    for &(q, fsf) in &s.biquads {
        chain.push(Coeffs::lowpass(fs, fc * fsf, q));
    }
    if let Some(fsf) = s.first_order {
        chain.push(Coeffs::lowpass_1st(fs, fc * fsf));
    }
    chain
}

fn build_highpass(s: &Sections, fc: f64, fs: f64) -> BiquadChain {
    // High-pass is the low-pass prototype with the frequency axis inverted, so
    // each section's scaling factor divides rather than multiplies the cutoff.
    let mut chain = BiquadChain::new();
    for &(q, fsf) in &s.biquads {
        chain.push(Coeffs::highpass(fs, fc / fsf, q));
    }
    if let Some(fsf) = s.first_order {
        chain.push(Coeffs::highpass_1st(fs, fc / fsf));
    }
    chain
}

/// Low pass section of a crossover.
pub fn lowpass(kind: CrossoverKind, order: usize, fc: f64, fs: f64) -> BiquadChain {
    match kind {
        CrossoverKind::Butterworth => build_lowpass(&butterworth_sections(order), fc, fs),
        CrossoverKind::Bessel => build_lowpass(&bessel_sections(order), fc, fs),
        CrossoverKind::LinkwitzRiley => {
            // Two cascaded Butterworth filters of half the order. Odd orders are
            // rounded down rather than panicking (a hostile API can't crash us).
            let half = (order / 2).max(1);
            let mut chain = build_lowpass(&butterworth_sections(half), fc, fs);
            chain.extend(&build_lowpass(&butterworth_sections(half), fc, fs));
            chain
        }
    }
}

/// High pass section of a crossover.
pub fn highpass(kind: CrossoverKind, order: usize, fc: f64, fs: f64) -> BiquadChain {
    match kind {
        CrossoverKind::Butterworth => build_highpass(&butterworth_sections(order), fc, fs),
        CrossoverKind::Bessel => build_highpass(&bessel_sections(order), fc, fs),
        CrossoverKind::LinkwitzRiley => {
            let half = (order / 2).max(1);
            let mut chain = build_highpass(&butterworth_sections(half), fc, fs);
            chain.extend(&build_highpass(&butterworth_sections(half), fc, fs));
            chain
        }
    }
}

/// Band pass section: a high-pass at `f_low` cascaded with a low-pass at
/// `f_high` — the band for a midrange driver. `f_low` should be below `f_high`.
pub fn bandpass(
    kind: CrossoverKind,
    order: usize,
    f_low: f64,
    f_high: f64,
    fs: f64,
) -> BiquadChain {
    let mut chain = highpass(kind, order, f_low, fs);
    chain.extend(&lowpass(kind, order, f_high, fs));
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;
    const FC: f64 = 1000.0;

    #[test]
    fn butterworth_lp_is_minus_3db_at_fc() {
        for order in 1..=8 {
            let lp = lowpass(CrossoverKind::Butterworth, order, FC, FS);
            let db = lp.magnitude_db(FC, FS);
            assert!((db + 3.0103).abs() < 0.05, "order {order}: {db} dB at fc");
        }
    }

    #[test]
    fn butterworth_slope_steepens_with_order() {
        for order in 1..=8 {
            let lp = lowpass(CrossoverKind::Butterworth, order, FC, FS);
            let db = lp.magnitude_db(2.0 * FC, FS);
            let expected = -6.0206 * order as f64;
            assert!((db - expected).abs() < 1.5, "order {order}: {db} vs {expected}");
        }
    }

    #[test]
    fn linkwitz_riley_lp_is_minus_6db_at_fc() {
        for order in [2, 4, 6, 8] {
            let lp = lowpass(CrossoverKind::LinkwitzRiley, order, FC, FS);
            let db = lp.magnitude_db(FC, FS);
            assert!((db + 6.0206).abs() < 0.1, "LR{order}: {db} dB at fc");
        }
    }

    #[test]
    fn linkwitz_riley_sums_flat() {
        let xo = Crossover::new(CrossoverKind::LinkwitzRiley, 4, FC, FS);
        for f in [50.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 15000.0] {
            let lp_lin = 10.0_f64.powf(xo.lowpass.magnitude_db(f, FS) / 20.0);
            let hp_lin = 10.0_f64.powf(xo.highpass.magnitude_db(f, FS) / 20.0);
            let sum_db = 20.0 * (lp_lin + hp_lin).log10();
            assert!(sum_db.abs() < 0.35, "f={f}: sum {sum_db} dB");
        }
    }

    #[test]
    fn bessel_is_minus_3db_at_fc() {
        // Both low- and high-pass land at −3 dB at the cutoff for every order.
        for order in 1..=4 {
            let lp = lowpass(CrossoverKind::Bessel, order, FC, FS);
            let hp = highpass(CrossoverKind::Bessel, order, FC, FS);
            assert!(
                (lp.magnitude_db(FC, FS) + 3.0103).abs() < 0.2,
                "Bessel LP order {order}: {} dB",
                lp.magnitude_db(FC, FS)
            );
            assert!(
                (hp.magnitude_db(FC, FS) + 3.0103).abs() < 0.2,
                "Bessel HP order {order}: {} dB",
                hp.magnitude_db(FC, FS)
            );
        }
    }

    #[test]
    fn bessel_lp_passband_flat_stopband_down() {
        let lp = lowpass(CrossoverKind::Bessel, 4, FC, FS);
        assert!(lp.magnitude_db(50.0, FS).abs() < 0.2, "passband not flat");
        assert!(lp.magnitude_db(8000.0, FS) < -24.0, "stopband not attenuated");
        // Monotonic roll-off past the knee.
        let mut prev = lp.magnitude_db(FC, FS);
        for f in [1500.0, 2000.0, 3000.0, 4000.0, 6000.0] {
            let db = lp.magnitude_db(f, FS);
            assert!(db < prev + 1e-6, "not monotonic at {f}");
            prev = db;
        }
    }

    #[test]
    fn bandpass_passes_mid_rejects_edges() {
        let bp = bandpass(CrossoverKind::LinkwitzRiley, 4, 300.0, 3000.0, FS);
        assert!(bp.magnitude_db(1000.0, FS).abs() < 1.0, "mid not passed");
        assert!(bp.magnitude_db(40.0, FS) < -24.0, "low not rejected");
        assert!(bp.magnitude_db(15000.0, FS) < -24.0, "high not rejected");
        // −6 dB (LR) at each edge.
        assert!((bp.magnitude_db(300.0, FS) + 6.0206).abs() < 0.6, "low edge");
        assert!((bp.magnitude_db(3000.0, FS) + 6.0206).abs() < 0.6, "high edge");
    }
}
