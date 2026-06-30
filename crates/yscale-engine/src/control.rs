//! Live engine control: runs the real-time loop on a background thread and
//! accepts hot pipeline/source swaps while exposing per-output level meters.
//! This is what the web server drives.
//!
//! Unlike [`engine::run`](crate::engine::run) (which exits when the source ends),
//! the controlled loop fills silence and keeps the device open so the UI stays
//! live and ready for the next source.

use crate::alsa_out::AlsaOutput;
use crate::analyzer::Analyzer;
use crate::config::Config;
use crate::output::{Converter, SampleFormat};
use crate::source::Source;
use anyhow::{bail, Result};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use yscale_dsp::{BassManager, FirBank, Limiter, Pipeline};

enum Command {
    SwapPipeline(Box<Pipeline>),
    SwapSource(Box<dyn Source>),
    SwapBass(Box<BassManager>),
    SwapFir(Box<FirBank>),
}

/// Per-output-channel peak meters (linear 0..1), updated by the RT thread.
pub struct Meters {
    peaks: Vec<AtomicU32>,
}

impl Meters {
    fn new(n: usize) -> Self {
        Self {
            peaks: (0..n).map(|_| AtomicU32::new(0)).collect(),
        }
    }
    #[inline]
    fn store(&self, ch: usize, v: f32) {
        self.peaks[ch].store(v.to_bits(), Ordering::Relaxed);
    }
    /// Current peak (linear) per output channel.
    pub fn read(&self) -> Vec<f32> {
        self.peaks
            .iter()
            .map(|a| f32::from_bits(a.load(Ordering::Relaxed)))
            .collect()
    }
}

/// Handle to a running engine: swap the DSP graph or source live, read meters.
/// `Send + Sync` so it can live in shared web-server state.
pub struct EngineHandle {
    tx: Mutex<Sender<Command>>,
    stop: Arc<AtomicBool>,
    meters: Arc<Meters>,
    /// Safety-limiter gain reduction (dB, >= 0), stored as `f32` bits.
    gr: Arc<AtomicU32>,
    /// Live output spectrum analyzer (RTA).
    analyzer: Arc<Analyzer>,
    /// Engine block size (frames per period) — needed to build FIR convolvers.
    period: usize,
    join: Option<JoinHandle<()>>,
    pub n_in: usize,
    pub n_out: usize,
    pub sample_rate: u32,
}

impl EngineHandle {
    /// Replace the DSP graph (must keep the same input/output channel counts).
    pub fn swap_pipeline(&self, p: Pipeline) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Command::SwapPipeline(Box::new(p)));
        }
    }
    /// Replace the audio source (must keep the same channel count).
    pub fn swap_source(&self, s: Box<dyn Source>) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Command::SwapSource(s));
        }
    }
    /// Replace the bass-management stage live.
    pub fn swap_bass(&self, b: BassManager) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Command::SwapBass(Box::new(b)));
        }
    }
    /// Replace the per-channel FIR convolution bank live. `per_channel[c]` =
    /// `Some(coeffs)` enables a FIR on output channel `c`. The (expensive) FFT
    /// precompute happens here, off the realtime thread.
    pub fn swap_fir(&self, per_channel: Vec<Option<Vec<f64>>>) {
        let bank = FirBank::new(self.n_out, self.period, per_channel);
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Command::SwapFir(Box::new(bank)));
        }
    }
    /// Current per-output-channel peak levels (linear).
    pub fn meters(&self) -> Vec<f32> {
        self.meters.read()
    }
    /// Current safety-limiter gain reduction in dB (>= 0; 0 = not limiting).
    pub fn gain_reduction(&self) -> f32 {
        f32::from_bits(self.gr.load(Ordering::Relaxed))
    }
    /// Latest output spectrum: per-band magnitude in dBFS (30 ISO bands).
    pub fn spectrum(&self) -> Vec<f32> {
        self.analyzer.spectrum()
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Open the DAC and start the RT thread. The ALSA device, rate and format are
/// fixed for the engine's lifetime; the DSP graph and source are swappable.
pub fn spawn(config: &Config, source: Box<dyn Source>) -> Result<EngineHandle> {
    let pipeline = config.build_pipeline()?;
    let n_in = pipeline.n_in();
    let n_out = pipeline.n_out();
    if source.channels() != n_in {
        bail!(
            "source has {} channels but pipeline expects {}",
            source.channels(),
            n_in
        );
    }
    if source.sample_rate() != config.sample_rate {
        bail!(
            "source is {} Hz but config is {} Hz",
            source.sample_rate(),
            config.sample_rate
        );
    }

    let format: SampleFormat = config.format.into();
    let out = AlsaOutput::open(
        &config.device,
        config.sample_rate,
        n_out,
        format,
        config.period_frames,
        config.buffer_frames,
    )?;
    let (rate, period, buffer) = out.actual_params().unwrap_or((
        config.sample_rate,
        config.period_frames,
        config.buffer_frames,
    ));
    let frames = period.max(1) as usize;

    let (tx, rx) = channel::<Command>();
    let stop = Arc::new(AtomicBool::new(false));
    let meters = Arc::new(Meters::new(n_out));
    let gr = Arc::new(AtomicU32::new(0));
    let limiter = config.build_limiter(n_out);
    let bass = config.build_bass(n_out);
    let fir = FirBank::empty(n_out, frames);
    let analyzer = Arc::new(Analyzer::start(config.sample_rate));

    let stop_t = stop.clone();
    let meters_t = meters.clone();
    let gr_t = gr.clone();
    let analyzer_t = analyzer.clone();
    let dither = config.dither;
    let join = std::thread::Builder::new()
        .name("yscale-rt".into())
        .spawn(move || {
            rt_loop(
                out, pipeline, source, rx, stop_t, meters_t, gr_t, analyzer_t, bass, fir, limiter,
                frames, n_in, n_out, format, dither,
            );
        })?;

    eprintln!(
        "[yscale] engine live: {} Hz, {} ch, {:?}, period={} buffer={} ({:.1} ms)",
        rate,
        n_out,
        format,
        period,
        buffer,
        buffer as f64 / rate as f64 * 1000.0
    );
    Ok(EngineHandle {
        tx: Mutex::new(tx),
        stop,
        meters,
        gr,
        analyzer,
        period: frames,
        join: Some(join),
        n_in,
        n_out,
        sample_rate: config.sample_rate,
    })
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn rt_loop(
    out: AlsaOutput,
    mut pipeline: Pipeline,
    mut source: Box<dyn Source>,
    rx: Receiver<Command>,
    stop: Arc<AtomicBool>,
    meters: Arc<Meters>,
    gr: Arc<AtomicU32>,
    analyzer: Arc<Analyzer>,
    mut bass: BassManager,
    mut fir: FirBank,
    mut limiter: Limiter,
    frames: usize,
    n_in: usize,
    n_out: usize,
    format: SampleFormat,
    dither: bool,
) {
    let mut in_buf = vec![0.0f64; frames * n_in];
    let mut out_buf = vec![0.0f64; frames * n_out];
    let mut conv = Converter::new(format, dither);
    let mut i32_buf = vec![0i32; frames * n_out];
    let mut i16_buf = vec![0i16; frames * n_out];
    let mut peak = vec![0.0f32; n_out];
    const DECAY: f32 = 0.82; // per-block meter decay

    while !stop.load(Ordering::Relaxed) {
        // Apply any pending live changes (graph/source swaps).
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Command::SwapPipeline(p) => {
                    if p.n_in() == n_in && p.n_out() == n_out {
                        pipeline = *p;
                    }
                }
                Command::SwapSource(s) => {
                    if s.channels() == n_in {
                        source = s;
                    }
                }
                Command::SwapBass(b) => {
                    bass = *b;
                }
                Command::SwapFir(f) => {
                    fir = *f;
                }
            }
        }

        let got = source.fill(&mut in_buf, frames);
        if got == 0 {
            in_buf.iter_mut().for_each(|x| *x = 0.0);
        } else if got < frames {
            in_buf[got * n_in..].iter_mut().for_each(|x| *x = 0.0);
        }

        pipeline.process_interleaved(&in_buf, &mut out_buf, frames);

        // Bass management (mono-bass crossover), then FIR room correction.
        bass.process(&mut out_buf, frames);
        fir.process(&mut out_buf, frames);

        // Final safety stage: brickwall-limit the output so it can never clip
        // the DAC, whatever EQ/gain is upstream. Meter the post-limiter signal
        // (what actually reaches the DAC) and publish the gain reduction.
        limiter.process(&mut out_buf, frames);
        gr.store((limiter.gain_reduction_db() as f32).to_bits(), Ordering::Relaxed);

        // Feed the RTA a mono sum of the (post-limiter) output.
        for f in 0..frames {
            let mut s = 0.0;
            for c in 0..n_out {
                s += out_buf[f * n_out + c];
            }
            analyzer.push((s / n_out as f64) as f32);
        }

        // Per-channel peak with decay, for the UI meters.
        for p in peak.iter_mut() {
            *p *= DECAY;
        }
        for f in 0..frames {
            for c in 0..n_out {
                let a = out_buf[f * n_out + c].abs() as f32;
                if a > peak[c] {
                    peak[c] = a;
                }
            }
        }
        for c in 0..n_out {
            meters.store(c, peak[c]);
        }

        let ok = match format {
            SampleFormat::S32Le => {
                conv.to_i32(&out_buf, &mut i32_buf);
                out.write_i32(&i32_buf).is_ok()
            }
            SampleFormat::S16Le => {
                conv.to_i16(&out_buf, &mut i16_buf);
                out.write_i16(&i16_buf).is_ok()
            }
        };
        if !ok {
            break;
        }
    }
    let _ = out.drain();
}
