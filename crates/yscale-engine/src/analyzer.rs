//! Real-time **spectrum analyzer (RTA)** of the engine output.
//!
//! The RT thread pushes a mono sum of the post-DSP output into a lock-free ring
//! (atomic stores only — never blocks the audio thread). A background thread
//! periodically snapshots the ring, applies a Hann window + a hand-rolled
//! radix-2 FFT, and aggregates the magnitude into 30 ISO 1/3-octave bands (the
//! same centres as the graphic EQ) as dBFS, which the UI draws live.

use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const RING: usize = 16_384;
const MASK: usize = RING - 1;
const FFT_SIZE: usize = 4096;

/// 30 ISO 1/3-octave band centres (Hz) — matches `yscale-dsp`'s graphic EQ.
const ISO_BANDS: [f64; 30] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
    500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0,
    8000.0, 10000.0, 12500.0, 16000.0,
];
pub const N_BANDS: usize = 30;
const FLOOR_DB: f32 = -90.0;

/// Lock-free single-producer ring of recent mono output samples.
struct Ring {
    buf: Vec<AtomicU32>,
    wpos: AtomicUsize,
}

impl Ring {
    fn new() -> Self {
        Self {
            buf: (0..RING).map(|_| AtomicU32::new(0)).collect(),
            wpos: AtomicUsize::new(0),
        }
    }
    /// Push one sample (called from the RT thread).
    #[inline]
    fn push(&self, s: f32) {
        let i = self.wpos.fetch_add(1, Ordering::Relaxed);
        self.buf[i & MASK].store(s.to_bits(), Ordering::Relaxed);
    }
    /// Copy the most recent `FFT_SIZE` samples (newest last).
    fn snapshot(&self, out: &mut [f64; FFT_SIZE]) {
        let end = self.wpos.load(Ordering::Relaxed);
        let start = end.wrapping_sub(FFT_SIZE);
        for (i, o) in out.iter_mut().enumerate() {
            *o = f32::from_bits(self.buf[start.wrapping_add(i) & MASK].load(Ordering::Relaxed))
                as f64;
        }
    }
}

/// Handle to the running analyzer. The RT thread holds `ring` to push samples;
/// `spectrum()` reads the latest banded result.
pub struct Analyzer {
    ring: Arc<Ring>,
    bands: Arc<Vec<AtomicU32>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Analyzer {
    pub fn start(sample_rate: u32) -> Self {
        let ring = Arc::new(Ring::new());
        let bands: Arc<Vec<AtomicU32>> =
            Arc::new((0..N_BANDS).map(|_| AtomicU32::new(FLOOR_DB.to_bits())).collect());
        let stop = Arc::new(AtomicBool::new(false));

        let r = ring.clone();
        let b = bands.clone();
        let st = stop.clone();
        let fs = sample_rate as f64;
        let join = std::thread::Builder::new()
            .name("yscale-rta".into())
            .spawn(move || analysis_loop(fs, r, b, st))
            .ok();

        Self {
            ring,
            bands,
            stop,
            join,
        }
    }

    /// Push one mono output sample (RT thread, allocation-free).
    #[inline]
    pub fn push(&self, sample: f32) {
        self.ring.push(sample);
    }

    /// Latest per-band magnitude in dBFS (length [`N_BANDS`]).
    pub fn spectrum(&self) -> Vec<f32> {
        self.bands
            .iter()
            .map(|a| f32::from_bits(a.load(Ordering::Relaxed)))
            .collect()
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn analysis_loop(fs: f64, ring: Arc<Ring>, bands: Arc<Vec<AtomicU32>>, stop: Arc<AtomicBool>) {
    // Hann window (and its coherent gain, for amplitude calibration).
    let window: Vec<f64> = (0..FFT_SIZE)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / FFT_SIZE as f64).cos())
        .collect();

    // Pre-compute the FFT bin range for each ISO band (±1/6 octave edges).
    let edge = 2.0_f64.powf(1.0 / 6.0);
    let bin_ranges: Vec<(usize, usize)> = ISO_BANDS
        .iter()
        .map(|&fc| {
            let lo = ((fc / edge) / fs * FFT_SIZE as f64).round() as usize;
            let hi = ((fc * edge) / fs * FFT_SIZE as f64).round() as usize;
            (lo.max(1), hi.min(FFT_SIZE / 2).max(lo.max(1)))
        })
        .collect();

    let mut re = [0.0f64; FFT_SIZE];
    let mut im = [0.0f64; FFT_SIZE];
    let mut samples = [0.0f64; FFT_SIZE];
    // Amplitude calibration: a full-scale sine peaks near 0 dBFS.
    let cal = 4.0 / FFT_SIZE as f64;

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(80));
        ring.snapshot(&mut samples);
        for i in 0..FFT_SIZE {
            re[i] = samples[i] * window[i];
            im[i] = 0.0;
        }
        fft_radix2(&mut re, &mut im);

        for (bi, &(lo, hi)) in bin_ranges.iter().enumerate() {
            let mut power = 0.0;
            for k in lo..=hi {
                let mag = (re[k] * re[k] + im[k] * im[k]).sqrt() * cal;
                power += mag * mag;
            }
            let amp = power.sqrt();
            let db = (20.0 * amp.max(1e-6).log10()).clamp(FLOOR_DB as f64, 6.0) as f32;
            bands[bi].store(db.to_bits(), Ordering::Relaxed);
        }
    }
}

/// In-place iterative radix-2 Cooley-Tukey FFT (`len` must be a power of two).
fn fft_radix2(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
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
    // Butterflies.
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f64; // forward transform
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_of_dc_is_in_bin_zero() {
        let mut re = [0.0; 8];
        let mut im = [0.0; 8];
        re.iter_mut().for_each(|x| *x = 1.0);
        fft_radix2(&mut re, &mut im);
        assert!((re[0] - 8.0).abs() < 1e-9 && im[0].abs() < 1e-9);
        for k in 1..8 {
            assert!(re[k].abs() < 1e-9 && im[k].abs() < 1e-9, "bin {k} not zero");
        }
    }

    #[test]
    fn fft_of_sine_peaks_at_its_bin() {
        // A pure cosine at bin 3 should put all energy in bin 3 (and N-3).
        let n = 64;
        let mut re: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 3.0 * i as f64 / n as f64).cos())
            .collect();
        let mut im = vec![0.0; n];
        fft_radix2(&mut re, &mut im);
        let mag = |k: usize| (re[k] * re[k] + im[k] * im[k]).sqrt();
        assert!(mag(3) > 0.9 * n as f64 / 2.0, "bin 3 weak: {}", mag(3));
        for k in [1, 2, 4, 5, 10, 20] {
            assert!(mag(k) < 1e-6, "bin {k} should be ~0");
        }
    }

    #[test]
    fn analyzer_reports_a_tone_in_the_right_band() {
        let fs = 48_000.0;
        let a = Analyzer::start(48_000);
        // Feed a 1 kHz full-scale sine for ~a few FFT windows.
        let step = 2.0 * PI * 1000.0 / fs;
        for n in 0..(FFT_SIZE * 6) {
            a.push((n as f64 * step).sin() as f32);
        }
        std::thread::sleep(Duration::from_millis(200));
        let spec = a.spectrum();
        assert_eq!(spec.len(), N_BANDS);
        // 1 kHz is band index 17. It should be well above the floor and the
        // loudest band.
        let loudest = spec
            .iter()
            .cloned()
            .enumerate()
            .max_by(|x, y| x.1.partial_cmp(&y.1).unwrap())
            .unwrap()
            .0;
        assert!(spec[17] > -20.0, "1 kHz band low: {}", spec[17]);
        assert!((loudest as i32 - 17).abs() <= 1, "loudest band {loudest}");
    }
}
