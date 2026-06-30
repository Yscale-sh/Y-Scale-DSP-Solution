//! **FIR convolution** via overlap-save (FFT) — the engine for linear-phase
//! filtering and FIR room correction.
//!
//! Each [`FirConv`] convolves a fixed audio block size against an
//! impulse-response of arbitrary length using a single FFT per block (size = the
//! next power of two ≥ `block + taps − 1`). Overlap-save adds no algorithmic
//! latency beyond the FIR's own group delay, so a linear-phase correction FIR of
//! `N` taps delays by ~`N/2` samples (matched across channels of equal length).
//!
//! [`FirBank`] holds one optional `FirConv` per output channel and processes an
//! interleaved buffer in place.

use crate::fft::{fft, next_pow2};

/// Single-channel overlap-save FFT convolver for a fixed block size.
pub struct FirConv {
    block: usize,
    l: usize,        // FFT size
    ov: usize,       // overlap = l - block
    taps: usize,
    hre: Vec<f64>,   // FIR spectrum (length l)
    him: Vec<f64>,
    overlap: Vec<f64>, // retained input tail (length ov)
    re: Vec<f64>,    // scratch (length l)
    im: Vec<f64>,
}

impl FirConv {
    /// Build a convolver for `coeffs` (the impulse response) and the engine's
    /// `block` size. Empty `coeffs` yields a unity (pass-through) filter.
    pub fn new(coeffs: &[f64], block: usize) -> Self {
        let block = block.max(1);
        let taps = coeffs.len().max(1);
        let l = next_pow2(block + taps - 1);
        let mut hre = vec![0.0; l];
        let mut him = vec![0.0; l];
        if coeffs.is_empty() {
            hre[0] = 1.0; // identity
        } else {
            hre[..coeffs.len()].copy_from_slice(coeffs);
        }
        fft(&mut hre, &mut him, false);
        Self {
            block,
            l,
            ov: l - block,
            taps,
            hre,
            him,
            overlap: vec![0.0; l - block],
            re: vec![0.0; l],
            im: vec![0.0; l],
        }
    }

    pub fn taps(&self) -> usize {
        self.taps
    }

    /// Convolve one block in place. `io.len()` must equal the configured block.
    pub fn process_block(&mut self, io: &mut [f64]) {
        if io.len() != self.block {
            return;
        }
        let (l, ov, block) = (self.l, self.ov, self.block);

        // Segment = [retained overlap | new input], length L.
        self.re[..ov].copy_from_slice(&self.overlap);
        self.re[ov..l].copy_from_slice(io);
        for x in self.im.iter_mut() {
            *x = 0.0;
        }
        // Retain the last `ov` input samples for the next block (before the FFT
        // overwrites `re`).
        self.overlap.copy_from_slice(&self.re[block..l]);

        fft(&mut self.re, &mut self.im, false);
        for k in 0..l {
            let (ar, ai) = (self.re[k], self.im[k]);
            let (br, bi) = (self.hre[k], self.him[k]);
            self.re[k] = ar * br - ai * bi;
            self.im[k] = ar * bi + ai * br;
        }
        fft(&mut self.re, &mut self.im, true);

        // The last `block` samples carry full N-tap support (no wrap aliasing).
        io.copy_from_slice(&self.re[ov..l]);
    }
}

/// Per-output-channel bank of FIR convolvers.
pub struct FirBank {
    channels: usize,
    block: usize,
    convs: Vec<Option<FirConv>>,
    tmp: Vec<f64>,
}

impl FirBank {
    /// `per_channel[c]` = `Some(coeffs)` enables a FIR on channel `c`.
    pub fn new(channels: usize, block: usize, per_channel: Vec<Option<Vec<f64>>>) -> Self {
        let mut convs: Vec<Option<FirConv>> = per_channel
            .into_iter()
            .take(channels)
            .map(|c| c.map(|coeffs| FirConv::new(&coeffs, block)))
            .collect();
        convs.resize_with(channels, || None);
        Self {
            channels,
            block,
            convs,
            tmp: vec![0.0; block],
        }
    }

    /// An all-pass (no FIRs) bank.
    pub fn empty(channels: usize, block: usize) -> Self {
        Self::new(channels, block, Vec::new())
    }

    pub fn is_active(&self) -> bool {
        self.convs.iter().any(Option::is_some)
    }

    /// Convolve an interleaved buffer (`frames * channels`) in place.
    pub fn process(&mut self, buf: &mut [f64], frames: usize) {
        if frames != self.block {
            return; // built for a fixed block size
        }
        let n = self.channels;
        for (c, conv) in self.convs.iter_mut().enumerate() {
            let Some(conv) = conv else { continue };
            for f in 0..frames {
                self.tmp[f] = buf[f * n + c];
            }
            conv.process_block(&mut self.tmp[..frames]);
            for f in 0..frames {
                buf[f * n + c] = self.tmp[f];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Naive direct linear convolution, for cross-checking.
    fn direct(x: &[f64], h: &[f64]) -> Vec<f64> {
        let mut y = vec![0.0; x.len()];
        for (n, yn) in y.iter_mut().enumerate() {
            let mut acc = 0.0;
            for (k, &hk) in h.iter().enumerate() {
                if n >= k {
                    acc += hk * x[n - k];
                }
            }
            *yn = acc;
        }
        y
    }

    #[test]
    fn unit_impulse_is_passthrough() {
        let mut c = FirConv::new(&[1.0], 8);
        let mut io = vec![0.1, -0.2, 0.3, 0.4, -0.5, 0.6, 0.7, -0.8];
        let orig = io.clone();
        c.process_block(&mut io);
        for i in 0..8 {
            assert!((io[i] - orig[i]).abs() < 1e-9, "i={i}");
        }
    }

    #[test]
    fn delayed_impulse_delays_signal() {
        // h = [0,0,1] delays by 2 samples; check across two blocks.
        let mut c = FirConv::new(&[0.0, 0.0, 1.0], 4);
        let mut b1 = vec![1.0, 2.0, 3.0, 4.0];
        c.process_block(&mut b1);
        assert!((b1[0]).abs() < 1e-9 && (b1[1]).abs() < 1e-9);
        assert!((b1[2] - 1.0).abs() < 1e-9 && (b1[3] - 2.0).abs() < 1e-9);
        let mut b2 = vec![5.0, 6.0, 7.0, 8.0];
        c.process_block(&mut b2);
        assert!((b2[0] - 3.0).abs() < 1e-9 && (b2[1] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn overlap_save_matches_direct_convolution() {
        // Random-ish signal + FIR, processed in blocks, must equal direct conv.
        let taps = 37;
        let h: Vec<f64> = (0..taps).map(|k| ((k * 7 % 11) as f64 - 5.0) * 0.1).collect();
        let total = 512;
        let x: Vec<f64> = (0..total).map(|n| (n as f64 * 0.21).sin() * 0.7).collect();
        let expected = direct(&x, &h);

        let block = 64;
        let mut c = FirConv::new(&h, block);
        let mut got = Vec::with_capacity(total);
        for chunk in x.chunks(block) {
            let mut b = chunk.to_vec();
            c.process_block(&mut b);
            got.extend_from_slice(&b);
        }
        for n in 0..total {
            assert!(
                (got[n] - expected[n]).abs() < 1e-7,
                "n={n}: {} vs {}",
                got[n],
                expected[n]
            );
        }
    }

    #[test]
    fn bank_applies_per_channel() {
        // Channel 0 gets a 2-sample delay; channel 1 passes through.
        let mut bank = FirBank::new(2, 4, vec![Some(vec![0.0, 0.0, 1.0]), None]);
        assert!(bank.is_active());
        let mut buf = vec![1.0, 10.0, 2.0, 20.0, 3.0, 30.0, 4.0, 40.0]; // [L,R]*4
        bank.process(&mut buf, 4);
        // R unchanged.
        assert!((buf[1] - 10.0).abs() < 1e-9 && (buf[7] - 40.0).abs() < 1e-9);
        // L delayed by 2: out L = [0,0,1,2].
        assert!(buf[0].abs() < 1e-9 && buf[2].abs() < 1e-9);
        assert!((buf[4] - 1.0).abs() < 1e-9 && (buf[6] - 2.0).abs() < 1e-9);
    }
}
