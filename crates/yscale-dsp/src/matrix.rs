//! N-in / N-out channel routing matrix.
//!
//! `output[o] = Σ_i gain[o][i] · input[i]`. This covers mono summing, left/right
//! isolation, channel swaps, and arbitrary mixing — and is the place where one
//! source gets fanned out to several output channels for active crossovers.

/// A routing/mixing matrix of linear gains, stored row-major as `[out][in]`.
#[derive(Debug, Clone)]
pub struct ChannelMatrix {
    n_in: usize,
    n_out: usize,
    gains: Vec<f64>,
}

impl ChannelMatrix {
    /// All-zero matrix of the given size.
    pub fn new(n_in: usize, n_out: usize) -> Self {
        Self {
            n_in,
            n_out,
            gains: vec![0.0; n_in * n_out],
        }
    }

    /// Identity routing (`out[i] = in[i]`), sized `n × n`.
    pub fn identity(n: usize) -> Self {
        let mut m = Self::new(n, n);
        for i in 0..n {
            m.set(i, i, 1.0);
        }
        m
    }

    pub fn n_in(&self) -> usize {
        self.n_in
    }

    pub fn n_out(&self) -> usize {
        self.n_out
    }

    #[inline]
    pub fn set(&mut self, out: usize, inp: usize, gain: f64) {
        self.gains[out * self.n_in + inp] = gain;
    }

    #[inline]
    pub fn get(&self, out: usize, inp: usize) -> f64 {
        self.gains[out * self.n_in + inp]
    }

    /// Mix one input frame (`input.len() == n_in`) into one output frame
    /// (`output.len() == n_out`).
    #[inline]
    pub fn process_frame(&self, input: &[f64], output: &mut [f64]) {
        for o in 0..self.n_out {
            let row = &self.gains[o * self.n_in..(o + 1) * self.n_in];
            let mut acc = 0.0;
            for (g, &x) in row.iter().zip(input.iter()) {
                acc += g * x;
            }
            output[o] = acc;
        }
    }

    // ---- Common stereo (2-in/2-out) presets ----

    /// Straight stereo (L→L, R→R).
    pub fn stereo() -> Self {
        Self::identity(2)
    }

    /// Mono: both outputs get `(L + R) / 2`.
    pub fn mono() -> Self {
        let mut m = Self::new(2, 2);
        for o in 0..2 {
            m.set(o, 0, 0.5);
            m.set(o, 1, 0.5);
        }
        m
    }

    /// Left source fed to both outputs.
    pub fn left_to_both() -> Self {
        let mut m = Self::new(2, 2);
        m.set(0, 0, 1.0);
        m.set(1, 0, 1.0);
        m
    }

    /// Right source fed to both outputs.
    pub fn right_to_both() -> Self {
        let mut m = Self::new(2, 2);
        m.set(0, 1, 1.0);
        m.set(1, 1, 1.0);
        m
    }

    /// Swap left and right.
    pub fn swap() -> Self {
        let mut m = Self::new(2, 2);
        m.set(0, 1, 1.0);
        m.set(1, 0, 1.0);
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passes_through() {
        let m = ChannelMatrix::stereo();
        let mut out = [0.0; 2];
        m.process_frame(&[0.3, -0.7], &mut out);
        assert_eq!(out, [0.3, -0.7]);
    }

    #[test]
    fn mono_sums_halves() {
        let m = ChannelMatrix::mono();
        let mut out = [0.0; 2];
        m.process_frame(&[1.0, 0.0], &mut out);
        assert_eq!(out, [0.5, 0.5]);
    }

    #[test]
    fn swap_swaps() {
        let m = ChannelMatrix::swap();
        let mut out = [0.0; 2];
        m.process_frame(&[0.2, 0.9], &mut out);
        assert_eq!(out, [0.9, 0.2]);
    }

    #[test]
    fn left_to_both_isolates_left() {
        let m = ChannelMatrix::left_to_both();
        let mut out = [0.0; 2];
        m.process_frame(&[0.4, 0.8], &mut out);
        assert_eq!(out, [0.4, 0.4]);
    }
}
