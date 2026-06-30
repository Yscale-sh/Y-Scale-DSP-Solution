//! Minimal in-place radix-2 FFT (power-of-two lengths). Dependency-free, `f64`,
//! shared by the FIR convolution engine. Forward and inverse (the inverse scales
//! by `1/N`).

use std::f64::consts::PI;

/// Next power of two ≥ `n` (at least 1).
#[inline]
pub fn next_pow2(n: usize) -> usize {
    n.max(1).next_power_of_two()
}

/// In-place iterative radix-2 Cooley-Tukey FFT. `re.len()` must be a power of
/// two and equal to `im.len()`. `inverse = true` computes the IFFT (÷N).
pub fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    assert_eq!(n, im.len());
    assert!(n.is_power_of_two(), "FFT length must be a power of two");
    if n <= 1 {
        return;
    }

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let ang = sign * 2.0 * PI / len as f64;
        let wr_step = ang.cos();
        let wi_step = ang.sin();
        let half = len / 2;
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..half {
                let a = i + k;
                let b = i + k + half;
                let tr = cr * re[b] - ci * im[b];
                let ti = cr * im[b] + ci * re[b];
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr_step - ci * wi_step;
                ci = cr * wi_step + ci * wr_step;
                cr = ncr;
            }
            i += len;
        }
        len <<= 1;
    }

    if inverse {
        let s = 1.0 / n as f64;
        for k in 0..n {
            re[k] *= s;
            im[k] *= s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_then_inverse_is_identity() {
        let n = 64;
        let orig: Vec<f64> = (0..n).map(|i| (i as f64 * 0.3).sin() + 0.2 * i as f64).collect();
        let mut re = orig.clone();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im, false);
        fft(&mut re, &mut im, true);
        for i in 0..n {
            assert!((re[i] - orig[i]).abs() < 1e-9, "i={i}: {} vs {}", re[i], orig[i]);
            assert!(im[i].abs() < 1e-9);
        }
    }

    #[test]
    fn impulse_has_flat_magnitude() {
        let n = 16;
        let mut re = vec![0.0; n];
        let mut im = vec![0.0; n];
        re[0] = 1.0;
        fft(&mut re, &mut im, false);
        for k in 0..n {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            assert!((mag - 1.0).abs() < 1e-12, "bin {k} mag {mag}");
        }
    }

    #[test]
    fn next_pow2_works() {
        assert_eq!(next_pow2(1), 1);
        assert_eq!(next_pow2(1000), 1024);
        assert_eq!(next_pow2(1024), 1024);
        assert_eq!(next_pow2(1025), 2048);
    }
}
