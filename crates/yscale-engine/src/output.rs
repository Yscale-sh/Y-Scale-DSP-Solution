//! Conversion from internal `f64` samples to integer PCM for the DAC.
//!
//! 32-bit output needs no dither (its quantization floor sits below any DAC's
//! analog noise). For 16-bit output we apply TPDF dither, the correct,
//! artifact-free way to quantize.

use crate::source::Rng;

/// Output sample format written to ALSA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    S16Le,
    S32Le,
}

impl SampleFormat {
    pub fn bits(self) -> u32 {
        match self {
            SampleFormat::S16Le => 16,
            SampleFormat::S32Le => 32,
        }
    }
}

/// Quantizes `f64` (nominally in `[-1, 1]`) to integer PCM, with optional TPDF
/// dither for sub-32-bit depths.
pub struct Converter {
    dither: bool,
    rng: Rng,
}

impl Converter {
    pub fn new(format: SampleFormat, dither: bool) -> Self {
        Self {
            // 32-bit never needs dither.
            dither: dither && format.bits() < 32,
            rng: Rng::new(0x9E37_79B9_7F4A_7C15),
        }
    }

    /// Convert into a 32-bit interleaved buffer (used for `S32_LE`).
    pub fn to_i32(&mut self, input: &[f64], out: &mut [i32]) {
        const SCALE: f64 = i32::MAX as f64;
        for (o, &x) in out.iter_mut().zip(input.iter()) {
            *o = (x.clamp(-1.0, 1.0) * SCALE).round() as i32;
        }
    }

    /// Convert into a 16-bit interleaved buffer (used for `S16_LE`), applying
    /// TPDF dither at the LSB when enabled.
    pub fn to_i16(&mut self, input: &[f64], out: &mut [i16]) {
        const SCALE: f64 = i16::MAX as f64;
        for (o, &x) in out.iter_mut().zip(input.iter()) {
            let mut v = x.clamp(-1.0, 1.0) * SCALE;
            if self.dither {
                // Triangular PDF dither: sum of two independent uniforms, ±1 LSB.
                v += (self.rng.next_f64() - self.rng.next_f64()) * 0.5;
            }
            *o = v.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_scale_maps_to_extremes() {
        let mut c = Converter::new(SampleFormat::S32Le, false);
        let mut out = [0i32; 3];
        c.to_i32(&[1.0, -1.0, 0.0], &mut out);
        assert_eq!(out[0], i32::MAX);
        assert_eq!(out[1], -i32::MAX);
        assert_eq!(out[2], 0);
    }

    #[test]
    fn clamps_out_of_range() {
        let mut c = Converter::new(SampleFormat::S16Le, false);
        let mut out = [0i16; 2];
        c.to_i16(&[2.0, -2.0], &mut out);
        // Symmetric scaling by i16::MAX: full scale clamps to ±32767.
        assert_eq!(out[0], i16::MAX);
        assert_eq!(out[1], -i16::MAX);
    }
}
