//! Equalizers: free-form parametric EQ and a fixed 30-band ISO 1/3-octave
//! graphic EQ. Both compile down to a [`BiquadChain`].

use crate::biquad::{BiquadChain, Coeffs};

/// The kind of a parametric EQ band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandKind {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Notch,
    BandPass,
    AllPass,
}

/// One parametric EQ band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub kind: BandKind,
    /// Centre / corner frequency in Hz.
    pub freq: f64,
    /// Quality factor (bandwidth). Ignored magnitude-wise for shelves' DC/Nyquist
    /// asymptotes but controls their transition slope.
    pub q: f64,
    /// Gain in dB (used by peaking and shelving bands; ignored otherwise).
    pub gain_db: f64,
}

impl Band {
    pub fn peaking(freq: f64, q: f64, gain_db: f64) -> Self {
        Self { kind: BandKind::Peaking, freq, q, gain_db }
    }
    pub fn low_shelf(freq: f64, q: f64, gain_db: f64) -> Self {
        Self { kind: BandKind::LowShelf, freq, q, gain_db }
    }
    pub fn high_shelf(freq: f64, q: f64, gain_db: f64) -> Self {
        Self { kind: BandKind::HighShelf, freq, q, gain_db }
    }

    /// Design the biquad coefficients for this band at sample rate `fs`.
    pub fn to_coeffs(&self, fs: f64) -> Coeffs {
        match self.kind {
            BandKind::Peaking => Coeffs::peaking(fs, self.freq, self.q, self.gain_db),
            BandKind::LowShelf => Coeffs::low_shelf(fs, self.freq, self.q, self.gain_db),
            BandKind::HighShelf => Coeffs::high_shelf(fs, self.freq, self.q, self.gain_db),
            BandKind::LowPass => Coeffs::lowpass(fs, self.freq, self.q),
            BandKind::HighPass => Coeffs::highpass(fs, self.freq, self.q),
            BandKind::Notch => Coeffs::notch(fs, self.freq, self.q),
            BandKind::BandPass => Coeffs::bandpass(fs, self.freq, self.q),
            BandKind::AllPass => Coeffs::allpass(fs, self.freq, self.q),
        }
    }
}

/// A parametric EQ: an ordered set of [`Band`]s designed into a biquad cascade.
#[derive(Debug, Clone, Default)]
pub struct ParametricEq {
    bands: Vec<Band>,
}

impl ParametricEq {
    pub fn new() -> Self {
        Self { bands: Vec::new() }
    }

    pub fn from_bands<I: IntoIterator<Item = Band>>(bands: I) -> Self {
        Self { bands: bands.into_iter().collect() }
    }

    pub fn push(&mut self, band: Band) -> &mut Self {
        self.bands.push(band);
        self
    }

    pub fn bands(&self) -> &[Band] {
        &self.bands
    }

    /// Realize the EQ as a [`BiquadChain`] at sample rate `fs`.
    pub fn to_chain(&self, fs: f64) -> BiquadChain {
        BiquadChain::from_coeffs(self.bands.iter().map(|b| b.to_coeffs(fs)))
    }
}

/// ISO 266 / R10 preferred centre frequencies for a 30-band 1/3-octave graphic
/// EQ, spanning 20 Hz to 16 kHz.
pub const ISO_THIRD_OCTAVE_30: [f64; 30] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
    500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0,
    8000.0, 10000.0, 12500.0, 16000.0,
];

/// Constant-Q value for adjacent 1/3-octave bands:
/// `Q = sqrt(2^(1/3)) / (2^(1/3) − 1) ≈ 4.318`.
pub const THIRD_OCTAVE_Q: f64 = 4.318_473_8;

/// A 30-band graphic EQ on the ISO 1/3-octave grid. Each band is a peaking
/// filter; gains are in dB and can be updated live.
#[derive(Debug, Clone)]
pub struct GraphicEq30 {
    gains_db: [f64; 30],
}

impl Default for GraphicEq30 {
    fn default() -> Self {
        Self::flat()
    }
}

impl GraphicEq30 {
    /// All bands at 0 dB (flat).
    pub fn flat() -> Self {
        Self { gains_db: [0.0; 30] }
    }

    pub fn from_gains(gains_db: [f64; 30]) -> Self {
        Self { gains_db }
    }

    /// The ISO centre frequencies, in band order.
    pub fn frequencies() -> &'static [f64; 30] {
        &ISO_THIRD_OCTAVE_30
    }

    /// Set the gain (dB) of band `index` (0..30).
    pub fn set_band(&mut self, index: usize, gain_db: f64) {
        if index < self.gains_db.len() {
            self.gains_db[index] = gain_db;
        }
    }

    pub fn gains(&self) -> &[f64; 30] {
        &self.gains_db
    }

    /// Realize as a [`BiquadChain`] at sample rate `fs`. Bands at exactly 0 dB
    /// and bands whose centre is above Nyquist are skipped to save cycles.
    pub fn to_chain(&self, fs: f64) -> BiquadChain {
        let nyquist = fs / 2.0;
        let mut chain = BiquadChain::new();
        for (i, &f0) in ISO_THIRD_OCTAVE_30.iter().enumerate() {
            let g = self.gains_db[i];
            if g.abs() < 1e-6 || f0 >= nyquist * 0.95 {
                continue;
            }
            chain.push(Coeffs::peaking(fs, f0, THIRD_OCTAVE_Q, g));
        }
        chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;

    #[test]
    fn flat_graphic_eq_is_empty_chain() {
        let eq = GraphicEq30::flat();
        assert!(eq.to_chain(FS).is_empty());
    }

    #[test]
    fn graphic_band_boosts_its_frequency() {
        let mut eq = GraphicEq30::flat();
        // Boost the 1 kHz band (index 17).
        assert_eq!(ISO_THIRD_OCTAVE_30[17], 1000.0);
        eq.set_band(17, 6.0);
        let chain = eq.to_chain(FS);
        assert_eq!(chain.len(), 1);
        assert!((chain.magnitude_db(1000.0, FS) - 6.0).abs() < 0.1);
        // A distant band is barely affected.
        assert!(chain.magnitude_db(100.0, FS).abs() < 1.0);
    }

    #[test]
    fn parametric_chain_sums_bands() {
        let eq = ParametricEq::from_bands([
            Band::peaking(100.0, 1.0, 4.0),
            Band::peaking(5000.0, 1.0, -3.0),
        ]);
        let chain = eq.to_chain(FS);
        assert_eq!(chain.len(), 2);
        assert!((chain.magnitude_db(100.0, FS) - 4.0).abs() < 0.3);
        assert!((chain.magnitude_db(5000.0, FS) + 3.0).abs() < 0.3);
    }

    #[test]
    fn graphic_eq_skips_above_nyquist() {
        // At 8 kHz sample rate, 16 kHz band is above Nyquist and must be skipped.
        let mut eq = GraphicEq30::flat();
        eq.set_band(29, 6.0); // 16 kHz
        assert!(eq.to_chain(8_000.0).is_empty());
    }
}
