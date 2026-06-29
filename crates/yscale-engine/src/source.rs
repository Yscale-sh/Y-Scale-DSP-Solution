//! Audio sources: built-in signal generators (the bread and butter of speaker
//! testing) plus WAV file playback. Every source produces interleaved `f64`
//! samples in `[-1, 1]`.

use alsa::pcm::{Access, Format, HwParams, PCM};
use alsa::{Direction, ValueOr};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;
use std::path::Path;

/// A pull-based source of interleaved audio.
pub trait Source: Send {
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> usize;
    /// Fill up to `frames` interleaved frames into `buf`
    /// (`buf.len() >= frames * channels`). Returns the number of frames
    /// produced; `0` signals end-of-stream.
    fn fill(&mut self, buf: &mut [f64], frames: usize) -> usize;
}

/// A tiny, fast, dependency-free PRNG (xorshift64*) for noise and dither.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in `[0, 1)`.
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Uniform in `[-1, 1)`.
    #[inline]
    pub fn next_bipolar(&mut self) -> f64 {
        2.0 * self.next_f64() - 1.0
    }
}

/// Continuous sine tone.
pub struct Sine {
    fs: u32,
    channels: usize,
    phase: f64,
    step: f64,
    amp: f64,
}

impl Sine {
    pub fn new(fs: u32, channels: usize, freq: f64, amplitude: f64) -> Self {
        Self {
            fs,
            channels,
            phase: 0.0,
            step: TAU * freq / fs as f64,
            amp: amplitude,
        }
    }
}

impl Source for Sine {
    fn sample_rate(&self) -> u32 {
        self.fs
    }
    fn channels(&self) -> usize {
        self.channels
    }
    fn fill(&mut self, buf: &mut [f64], frames: usize) -> usize {
        for f in 0..frames {
            let s = self.amp * self.phase.sin();
            self.phase += self.step;
            if self.phase >= TAU {
                self.phase -= TAU;
            }
            for c in 0..self.channels {
                buf[f * self.channels + c] = s;
            }
        }
        frames
    }
}

/// Exponential (logarithmic) sine sweep from `f1` to `f2` over `duration_s`,
/// optionally looping. Ideal for eyeballing/measuring frequency response.
pub struct LogSweep {
    fs: u32,
    channels: usize,
    f1: f64,
    f2: f64,
    amp: f64,
    duration: f64,
    looping: bool,
    t: f64,
    phase: f64,
    done: bool,
}

impl LogSweep {
    pub fn new(
        fs: u32,
        channels: usize,
        f1: f64,
        f2: f64,
        duration_s: f64,
        amplitude: f64,
        looping: bool,
    ) -> Self {
        Self {
            fs,
            channels,
            f1,
            f2,
            amp: amplitude,
            duration: duration_s,
            looping,
            t: 0.0,
            phase: 0.0,
            done: false,
        }
    }
}

impl Source for LogSweep {
    fn sample_rate(&self) -> u32 {
        self.fs
    }
    fn channels(&self) -> usize {
        self.channels
    }
    fn fill(&mut self, buf: &mut [f64], frames: usize) -> usize {
        if self.done {
            return 0;
        }
        let dt = 1.0 / self.fs as f64;
        let ratio = self.f2 / self.f1;
        for f in 0..frames {
            if self.t >= self.duration {
                if self.looping {
                    self.t = 0.0;
                    self.phase = 0.0;
                } else {
                    // Pad the rest of this block with silence and stop.
                    for c in 0..self.channels {
                        buf[f * self.channels + c] = 0.0;
                    }
                    continue;
                }
            }
            // Instantaneous frequency of an exponential sweep.
            let freq = self.f1 * ratio.powf(self.t / self.duration);
            let s = self.amp * self.phase.sin();
            self.phase += TAU * freq * dt;
            if self.phase >= TAU {
                self.phase -= TAU;
            }
            self.t += dt;
            for c in 0..self.channels {
                buf[f * self.channels + c] = s;
            }
        }
        if !self.looping && self.t >= self.duration {
            self.done = true;
        }
        frames
    }
}

/// White noise (uniform).
pub struct WhiteNoise {
    fs: u32,
    channels: usize,
    amp: f64,
    rng: Rng,
}

impl WhiteNoise {
    pub fn new(fs: u32, channels: usize, amplitude: f64, seed: u64) -> Self {
        Self {
            fs,
            channels,
            amp: amplitude,
            rng: Rng::new(seed),
        }
    }
}

impl Source for WhiteNoise {
    fn sample_rate(&self) -> u32 {
        self.fs
    }
    fn channels(&self) -> usize {
        self.channels
    }
    fn fill(&mut self, buf: &mut [f64], frames: usize) -> usize {
        for f in 0..frames {
            for c in 0..self.channels {
                buf[f * self.channels + c] = self.amp * self.rng.next_bipolar();
            }
        }
        frames
    }
}

/// Pink noise via Paul Kellet's economical 7-pole filter (independent per
/// channel). Roughly −3 dB/octave, the standard reference for room/speaker work.
pub struct PinkNoise {
    fs: u32,
    channels: usize,
    amp: f64,
    rng: Rng,
    state: Vec<[f64; 7]>,
}

impl PinkNoise {
    pub fn new(fs: u32, channels: usize, amplitude: f64, seed: u64) -> Self {
        Self {
            fs,
            channels,
            amp: amplitude,
            rng: Rng::new(seed),
            state: vec![[0.0; 7]; channels],
        }
    }
}

impl Source for PinkNoise {
    fn sample_rate(&self) -> u32 {
        self.fs
    }
    fn channels(&self) -> usize {
        self.channels
    }
    fn fill(&mut self, buf: &mut [f64], frames: usize) -> usize {
        for f in 0..frames {
            for c in 0..self.channels {
                let white = self.rng.next_bipolar();
                let s = &mut self.state[c];
                s[0] = 0.99886 * s[0] + white * 0.0555179;
                s[1] = 0.99332 * s[1] + white * 0.0750759;
                s[2] = 0.96900 * s[2] + white * 0.1538520;
                s[3] = 0.86650 * s[3] + white * 0.3104856;
                s[4] = 0.55000 * s[4] + white * 0.5329522;
                s[5] = -0.7616 * s[5] - white * 0.0168980;
                let pink = s[0] + s[1] + s[2] + s[3] + s[4] + s[5] + s[6] + white * 0.5362;
                s[6] = white * 0.115926;
                // Scale ~0.11 keeps the sum roughly in range.
                buf[f * self.channels + c] = self.amp * pink * 0.11;
            }
        }
        frames
    }
}

/// A single full-scale impulse followed by silence (looping at `period_frames`
/// if set). Useful for impulse-response / time-of-arrival checks.
pub struct Impulse {
    fs: u32,
    channels: usize,
    amp: f64,
    n: u64,
    period: Option<u64>,
}

impl Impulse {
    pub fn new(fs: u32, channels: usize, amplitude: f64, period_frames: Option<u64>) -> Self {
        Self {
            fs,
            channels,
            amp: amplitude,
            n: 0,
            period: period_frames,
        }
    }
}

impl Source for Impulse {
    fn sample_rate(&self) -> u32 {
        self.fs
    }
    fn channels(&self) -> usize {
        self.channels
    }
    fn fill(&mut self, buf: &mut [f64], frames: usize) -> usize {
        for f in 0..frames {
            let hit = match self.period {
                Some(p) => self.n % p == 0,
                None => self.n == 0,
            };
            let s = if hit { self.amp } else { 0.0 };
            for c in 0..self.channels {
                buf[f * self.channels + c] = s;
            }
            self.n += 1;
        }
        frames
    }
}

/// WAV file playback. Mono files are duplicated to all output channels; channel
/// counts are otherwise matched by truncation/zero-fill.
pub struct WavFile {
    fs: u32,
    channels: usize,
    src_channels: usize,
    samples: Vec<f64>,
    pos: usize,
    looping: bool,
}

impl WavFile {
    pub fn open(path: &Path, out_channels: usize, looping: bool) -> anyhow::Result<Self> {
        let mut reader = hound::WavReader::open(path)?;
        let spec = reader.spec();
        let max = (1i64 << (spec.bits_per_sample - 1)) as f64;
        let samples: Vec<f64> = match spec.sample_format {
            hound::SampleFormat::Float => {
                reader.samples::<f32>().map(|s| s.unwrap_or(0.0) as f64).collect()
            }
            hound::SampleFormat::Int => reader
                .samples::<i32>()
                .map(|s| s.unwrap_or(0) as f64 / max)
                .collect(),
        };
        Ok(Self {
            fs: spec.sample_rate,
            channels: out_channels,
            src_channels: spec.channels as usize,
            samples,
            pos: 0,
            looping,
        })
    }
}

impl Source for WavFile {
    fn sample_rate(&self) -> u32 {
        self.fs
    }
    fn channels(&self) -> usize {
        self.channels
    }
    fn fill(&mut self, buf: &mut [f64], frames: usize) -> usize {
        let sc = self.src_channels;
        for f in 0..frames {
            if self.pos + sc > self.samples.len() {
                if self.looping && !self.samples.is_empty() {
                    self.pos = 0;
                } else {
                    return f;
                }
            }
            let frame = &self.samples[self.pos..self.pos + sc];
            for c in 0..self.channels {
                // Mono source -> all outputs; otherwise map by index (zero-fill).
                buf[f * self.channels + c] = if sc == 1 { frame[0] } else { *frame.get(c).unwrap_or(&0.0) };
            }
            self.pos += sc;
        }
        frames
    }
}

/// A silent source (all zeros) — keeps the engine running with no signal.
pub struct Silence {
    fs: u32,
    channels: usize,
}

impl Silence {
    pub fn new(fs: u32, channels: usize) -> Self {
        Self { fs, channels }
    }
}

impl Source for Silence {
    fn sample_rate(&self) -> u32 {
        self.fs
    }
    fn channels(&self) -> usize {
        self.channels
    }
    fn fill(&mut self, buf: &mut [f64], frames: usize) -> usize {
        buf[..frames * self.channels].iter_mut().for_each(|s| *s = 0.0);
        frames
    }
}

fn def_freq() -> f64 {
    1000.0
}
fn def_amp() -> f64 {
    0.25
}
fn def_f1() -> f64 {
    20.0
}
fn def_f2() -> f64 {
    20000.0
}
fn def_dur() -> f64 {
    10.0
}

/// Captures audio from an ALSA device (e.g. an `snd-aloop` loopback) so a DLNA
/// renderer / network stream can be fed THROUGH the DSP. Non-blocking: when no
/// data is available it yields silence, so the realtime engine never stalls.
pub struct Capture {
    pcm: PCM,
    fs: u32,
    channels: usize,
    tmp: Vec<i32>,
}

impl Capture {
    pub fn open(device: &str, fs: u32, channels: usize) -> anyhow::Result<Self> {
        let pcm = PCM::new(device, Direction::Capture, true)?; // non-blocking
        {
            let hwp = HwParams::any(&pcm)?;
            hwp.set_channels(channels as u32)?;
            hwp.set_rate(fs, ValueOr::Nearest)?;
            hwp.set_access(Access::RWInterleaved)?;
            hwp.set_format(Format::s32())?;
            hwp.set_buffer_size_near(16384)?;
            hwp.set_period_size_near(1024, ValueOr::Nearest)?;
            pcm.hw_params(&hwp)?;
        }
        pcm.prepare()?;
        let _ = pcm.start();
        Ok(Self {
            pcm,
            fs,
            channels,
            tmp: Vec::new(),
        })
    }
}

impl Source for Capture {
    fn sample_rate(&self) -> u32 {
        self.fs
    }
    fn channels(&self) -> usize {
        self.channels
    }
    fn fill(&mut self, buf: &mut [f64], frames: usize) -> usize {
        const SCALE: f64 = 1.0 / (i32::MAX as f64);
        let ch = self.channels;
        let need = frames * ch;
        if self.tmp.len() < need {
            self.tmp.resize(need, 0);
        }
        // Default to silence — covers no-data / errors so the engine never stalls.
        buf[..need].iter_mut().for_each(|s| *s = 0.0);
        let io = match self.pcm.io_i32() {
            Ok(io) => io,
            Err(_) => return frames,
        };
        match io.readi(&mut self.tmp[..need]) {
            Ok(n) => {
                let got = n * ch;
                for i in 0..got {
                    buf[i] = self.tmp[i] as f64 * SCALE;
                }
            }
            Err(e) => {
                // EAGAIN (no data yet) -> silence; xrun/suspend -> recover + restart.
                if self.pcm.try_recover(e, true).is_ok() {
                    let _ = self.pcm.start();
                }
            }
        }
        frames
    }
}

fn def_loopback() -> String {
    "plughw:Loopback,1,0".to_string()
}

/// Declarative source selector for the control API (JSON-tagged by `kind`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceSpec {
    Silence,
    Sine {
        #[serde(default = "def_freq")]
        freq: f64,
        #[serde(default = "def_amp")]
        amp: f64,
    },
    Sweep {
        #[serde(default = "def_f1")]
        f1: f64,
        #[serde(default = "def_f2")]
        f2: f64,
        #[serde(default = "def_dur")]
        dur: f64,
        #[serde(default = "def_amp")]
        amp: f64,
        #[serde(default)]
        looping: bool,
    },
    Pink {
        #[serde(default = "def_amp")]
        amp: f64,
    },
    White {
        #[serde(default = "def_amp")]
        amp: f64,
    },
    Impulse {
        #[serde(default)]
        period_ms: Option<f64>,
        #[serde(default = "def_amp")]
        amp: f64,
    },
    File {
        path: String,
        #[serde(default)]
        looping: bool,
    },
    /// Capture from an ALSA device (DLNA/network stream via an snd-aloop loopback).
    Capture {
        #[serde(default = "def_loopback")]
        device: String,
    },
}

impl SourceSpec {
    /// Instantiate the source for the given sample rate and channel count.
    pub fn build(&self, fs: u32, channels: usize) -> anyhow::Result<Box<dyn Source>> {
        Ok(match self {
            SourceSpec::Silence => Box::new(Silence::new(fs, channels)),
            SourceSpec::Sine { freq, amp } => Box::new(Sine::new(fs, channels, *freq, *amp)),
            SourceSpec::Sweep {
                f1,
                f2,
                dur,
                amp,
                looping,
            } => Box::new(LogSweep::new(fs, channels, *f1, *f2, *dur, *amp, *looping)),
            SourceSpec::Pink { amp } => Box::new(PinkNoise::new(fs, channels, *amp, 0xC0FFEE)),
            SourceSpec::White { amp } => Box::new(WhiteNoise::new(fs, channels, *amp, 0xBEEF)),
            SourceSpec::Impulse { period_ms, amp } => {
                let period = period_ms.map(|ms| (ms * 1e-3 * fs as f64) as u64);
                Box::new(Impulse::new(fs, channels, *amp, period))
            }
            SourceSpec::File { path, looping } => {
                Box::new(WavFile::open(Path::new(path), channels, *looping)?)
            }
            SourceSpec::Capture { device } => Box::new(Capture::open(device, fs, channels)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_is_bounded_and_periodic() {
        let mut s = Sine::new(48_000, 2, 1000.0, 0.5);
        let mut buf = vec![0.0; 256 * 2];
        assert_eq!(s.fill(&mut buf, 256), 256);
        assert!(buf.iter().all(|&v| v.abs() <= 0.5 + 1e-12));
        // Both channels identical.
        for f in 0..256 {
            assert_eq!(buf[f * 2], buf[f * 2 + 1]);
        }
    }

    #[test]
    fn impulse_hits_once() {
        let mut s = Impulse::new(48_000, 1, 1.0, None);
        let mut buf = vec![0.0; 16];
        s.fill(&mut buf, 16);
        assert_eq!(buf[0], 1.0);
        assert!(buf[1..].iter().all(|&v| v == 0.0));
    }

    #[test]
    fn sweep_terminates_when_not_looping() {
        let mut s = LogSweep::new(48_000, 1, 20.0, 20_000.0, 0.01, 0.5, false);
        let mut buf = vec![0.0; 1024];
        let mut total = 0;
        loop {
            let n = s.fill(&mut buf, 1024);
            total += n;
            if n == 0 {
                break;
            }
            if total > 48_000 {
                panic!("sweep never ended");
            }
        }
        assert!(total > 0);
    }

    #[test]
    fn pink_noise_in_range() {
        let mut s = PinkNoise::new(48_000, 2, 0.8, 12345);
        let mut buf = vec![0.0; 4096 * 2];
        s.fill(&mut buf, 4096);
        // Statistically bounded; allow generous headroom.
        assert!(buf.iter().all(|&v| v.abs() < 2.0));
    }
}
