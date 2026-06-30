//! TOML configuration: device/format settings plus a declarative DSP graph that
//! is compiled into a [`yscale_dsp::Pipeline`].

use crate::output::SampleFormat;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use yscale_dsp::crossover::{self, CrossoverKind};
use yscale_dsp::eq::{Band, BandKind, GraphicEq30};
use yscale_dsp::{BassManager, BiquadChain, ChannelMatrix, ChannelStrip, Limiter, Pipeline};

/// Top-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_device")]
    pub device: String,
    #[serde(default = "default_format")]
    pub format: FormatCfg,
    #[serde(default = "default_period")]
    pub period_frames: u32,
    #[serde(default = "default_buffer")]
    pub buffer_frames: u32,
    #[serde(default)]
    pub dither: bool,
    #[serde(default)]
    pub routing: Routing,
    /// One entry per output channel. Empty => stereo pass-through.
    #[serde(default)]
    pub channel: Vec<ChannelCfg>,
    /// Final-stage brickwall safety limiter (on by default).
    #[serde(default)]
    pub limiter: LimiterCfg,
    /// Bass management (mono-bass crossover; off by default).
    #[serde(default)]
    pub bass: BassCfg,
}

/// Bass management: split the low end at `freq`, sum it to mono, and either fold
/// it back into the mains (stereo) or route it to a dedicated `sub_channel`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BassCfg {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_bass_freq")]
    pub freq: f64,
    #[serde(default = "default_bass_order")]
    pub order: usize,
    /// Infrasonic (rumble) high-pass in Hz; 0 disables it.
    #[serde(default)]
    pub rumble_hz: f64,
    /// Dedicated sub output channel index (needs >2 output channels).
    #[serde(default)]
    pub sub_channel: Option<usize>,
    #[serde(default)]
    pub sub_gain_db: f64,
}

fn default_bass_freq() -> f64 {
    80.0
}
fn default_bass_order() -> usize {
    4
}

impl Default for BassCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            freq: default_bass_freq(),
            order: default_bass_order(),
            rumble_hz: 0.0,
            sub_channel: None,
            sub_gain_db: 0.0,
        }
    }
}

/// Final-stage safety limiter so the DSP can never clip the DAC, regardless of
/// the EQ/gain dialed into the channel strips.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimiterCfg {
    #[serde(default = "default_limiter_enabled")]
    pub enabled: bool,
    /// Output ceiling in dBFS (a touch below 0 leaves inter-sample headroom).
    #[serde(default = "default_ceiling_db")]
    pub ceiling_db: f64,
    /// Look-ahead window in ms (also sets the attack smoothing + latency).
    #[serde(default = "default_lookahead_ms")]
    pub lookahead_ms: f64,
    /// Release time in ms (how quickly gain recovers after a peak).
    #[serde(default = "default_release_ms")]
    pub release_ms: f64,
}

fn default_limiter_enabled() -> bool {
    true
}
fn default_ceiling_db() -> f64 {
    -1.0
}
fn default_lookahead_ms() -> f64 {
    2.0
}
fn default_release_ms() -> f64 {
    100.0
}

impl Default for LimiterCfg {
    fn default() -> Self {
        Self {
            enabled: default_limiter_enabled(),
            ceiling_db: default_ceiling_db(),
            lookahead_ms: default_lookahead_ms(),
            release_ms: default_release_ms(),
        }
    }
}

fn default_sample_rate() -> u32 {
    48_000
}
fn default_device() -> String {
    "hw:CARD=BossDAC,DEV=0".to_string()
}
fn default_format() -> FormatCfg {
    FormatCfg::S32Le
}
fn default_period() -> u32 {
    1024
}
fn default_buffer() -> u32 {
    4096
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sample_rate: default_sample_rate(),
            device: default_device(),
            format: default_format(),
            period_frames: default_period(),
            buffer_frames: default_buffer(),
            dither: false,
            routing: Routing::default(),
            channel: Vec::new(),
            limiter: LimiterCfg::default(),
            bass: BassCfg::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatCfg {
    S16Le,
    S32Le,
}

impl From<FormatCfg> for SampleFormat {
    fn from(f: FormatCfg) -> Self {
        match f {
            FormatCfg::S16Le => SampleFormat::S16Le,
            FormatCfg::S32Le => SampleFormat::S32Le,
        }
    }
}

/// Input-to-output routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Routing {
    pub preset: RoutingPreset,
    /// Custom `[out][in]` gain matrix (used when `preset = "custom"`).
    pub matrix: Option<Vec<Vec<f64>>>,
}

impl Default for Routing {
    fn default() -> Self {
        Self {
            preset: RoutingPreset::Stereo,
            matrix: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingPreset {
    #[default]
    Stereo,
    Mono,
    LeftToBoth,
    RightToBoth,
    Swap,
    Custom,
}

/// One output channel's processing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelCfg {
    pub name: Option<String>,
    #[serde(default)]
    pub gain_db: f64,
    /// Time-alignment delay in milliseconds (added to `delay_cm`).
    #[serde(default)]
    pub delay_ms: f64,
    /// Time-alignment delay as an acoustic path length in centimetres.
    #[serde(default)]
    pub delay_cm: f64,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub mute: bool,
    /// Parametric EQ bands (applied in order).
    #[serde(default)]
    pub eq: Vec<EqBandCfg>,
    /// Optional 30-band graphic EQ gains (dB); up to 30 values.
    pub graphic_eq: Option<Vec<f64>>,
    /// Optional crossover section for this channel.
    pub crossover: Option<CrossoverCfg>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EqBandCfg {
    pub kind: BandKindCfg,
    pub freq: f64,
    #[serde(default = "default_q")]
    pub q: f64,
    #[serde(default)]
    pub gain_db: f64,
}

fn default_q() -> f64 {
    0.707
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandKindCfg {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Notch,
    BandPass,
    AllPass,
}

impl From<BandKindCfg> for BandKind {
    fn from(k: BandKindCfg) -> Self {
        match k {
            BandKindCfg::Peaking => BandKind::Peaking,
            BandKindCfg::LowShelf => BandKind::LowShelf,
            BandKindCfg::HighShelf => BandKind::HighShelf,
            BandKindCfg::LowPass => BandKind::LowPass,
            BandKindCfg::HighPass => BandKind::HighPass,
            BandKindCfg::Notch => BandKind::Notch,
            BandKindCfg::BandPass => BandKind::BandPass,
            BandKindCfg::AllPass => BandKind::AllPass,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossoverCfg {
    pub kind: XoverKindCfg,
    pub role: XoverRole,
    /// Cutoff in Hz. For `band_pass` this is the low edge (the high-pass cutoff).
    pub freq: f64,
    pub order: usize,
    /// High edge (low-pass cutoff) in Hz — only used when `role = band_pass`.
    #[serde(default)]
    pub freq_high: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XoverKindCfg {
    Butterworth,
    LinkwitzRiley,
    Bessel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XoverRole {
    LowPass,
    HighPass,
    BandPass,
}

impl Config {
    pub fn from_toml_str(s: &str) -> Result<Self> {
        Ok(toml::from_str(s)?)
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_toml_str(&text)
    }

    /// Number of input channels the engine expects from the source.
    pub fn n_in(&self) -> usize {
        match &self.routing.matrix {
            Some(m) if matches!(self.routing.preset, RoutingPreset::Custom) => {
                m.first().map(|row| row.len()).unwrap_or(2)
            }
            _ => 2,
        }
    }

    /// Number of output channels (= configured channel strips, default 2).
    pub fn n_out(&self) -> usize {
        if self.channel.is_empty() {
            2
        } else {
            self.channel.len()
        }
    }

    /// Build the bass-management stage for `n_out` output channels.
    pub fn build_bass(&self, n_out: usize) -> BassManager {
        BassManager::new(
            self.sample_rate as f64,
            n_out,
            self.bass.enabled,
            self.bass.freq,
            self.bass.order,
            self.bass.rumble_hz,
            self.bass.sub_channel,
            self.bass.sub_gain_db,
        )
    }

    /// Build the final-stage safety limiter for `n_out` output channels.
    pub fn build_limiter(&self, n_out: usize) -> Limiter {
        Limiter::new(
            self.sample_rate as f64,
            n_out,
            self.limiter.ceiling_db,
            self.limiter.lookahead_ms,
            self.limiter.release_ms,
            self.limiter.enabled,
        )
    }

    /// Compile the configuration into a runnable DSP pipeline.
    pub fn build_pipeline(&self) -> Result<Pipeline> {
        let fs = self.sample_rate as f64;
        let n_out = self.n_out();

        let matrix = self.build_matrix(n_out)?;
        let n_in = matrix.n_in();

        let strips = if self.channel.is_empty() {
            (0..n_out).map(|_| ChannelStrip::new(0.02 * fs)).collect()
        } else {
            self.channel
                .iter()
                .map(|c| build_strip(c, fs))
                .collect::<Result<Vec<_>>>()?
        };

        if matrix.n_out() != strips.len() {
            bail!(
                "routing matrix has {} outputs but {} channel strips are configured",
                matrix.n_out(),
                strips.len()
            );
        }
        let _ = n_in;
        Ok(Pipeline::new(matrix, strips))
    }

    fn build_matrix(&self, n_out: usize) -> Result<ChannelMatrix> {
        match self.routing.preset {
            RoutingPreset::Custom => {
                let rows = self
                    .routing
                    .matrix
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("routing.preset = custom requires routing.matrix"))?;
                if rows.len() != n_out {
                    bail!(
                        "custom matrix has {} rows but {} output channels are configured",
                        rows.len(),
                        n_out
                    );
                }
                let n_in = rows.first().map(|r| r.len()).unwrap_or(0);
                let mut m = ChannelMatrix::new(n_in, n_out);
                for (o, row) in rows.iter().enumerate() {
                    if row.len() != n_in {
                        bail!("custom matrix rows must all have the same width");
                    }
                    for (i, &g) in row.iter().enumerate() {
                        m.set(o, i, g);
                    }
                }
                Ok(m)
            }
            preset => {
                // Built-in presets are 2x2; require a stereo output config.
                if n_out != 2 {
                    bail!(
                        "routing preset {:?} only supports 2 output channels (got {}); use preset = \"custom\"",
                        preset,
                        n_out
                    );
                }
                Ok(match preset {
                    RoutingPreset::Stereo => ChannelMatrix::stereo(),
                    RoutingPreset::Mono => ChannelMatrix::mono(),
                    RoutingPreset::LeftToBoth => ChannelMatrix::left_to_both(),
                    RoutingPreset::RightToBoth => ChannelMatrix::right_to_both(),
                    RoutingPreset::Swap => ChannelMatrix::swap(),
                    RoutingPreset::Custom => unreachable!(),
                })
            }
        }
    }
}

fn build_strip(c: &ChannelCfg, fs: f64) -> Result<ChannelStrip> {
    // Delay headroom: configured delay plus a comfortable 20 ms margin.
    let configured_samples = c.delay_ms * 1e-3 * fs + c.delay_cm * 0.01 / 343.0 * fs;
    let headroom = (configured_samples + 0.02 * fs).max(0.02 * fs);
    let mut strip = ChannelStrip::new(headroom);

    strip.delay.set_delay_samples(configured_samples);
    strip.set_gain_db(c.gain_db);
    strip.set_inverted(c.invert);
    strip.set_muted(c.mute);

    let mut filters = BiquadChain::new();

    // Graphic EQ first (if any), then parametric, then crossover.
    if let Some(gains) = &c.graphic_eq {
        if gains.len() > 30 {
            bail!("graphic_eq accepts at most 30 bands, got {}", gains.len());
        }
        let mut arr = [0.0; 30];
        for (i, &g) in gains.iter().enumerate() {
            arr[i] = g;
        }
        filters.extend(&GraphicEq30::from_gains(arr).to_chain(fs));
    }

    for band in &c.eq {
        let b = Band {
            kind: band.kind.into(),
            freq: band.freq,
            q: band.q,
            gain_db: band.gain_db,
        };
        filters.push(b.to_coeffs(fs));
    }

    if let Some(x) = &c.crossover {
        let kind = match x.kind {
            XoverKindCfg::Butterworth => CrossoverKind::Butterworth,
            XoverKindCfg::LinkwitzRiley => CrossoverKind::LinkwitzRiley,
            XoverKindCfg::Bessel => CrossoverKind::Bessel,
        };
        let chain = match x.role {
            XoverRole::LowPass => crossover::lowpass(kind, x.order, x.freq, fs),
            XoverRole::HighPass => crossover::highpass(kind, x.order, x.freq, fs),
            XoverRole::BandPass => {
                // freq = low edge (high-pass), freq_high = high edge (low-pass).
                let f_high = x.freq_high.unwrap_or((fs / 2.0).min(20_000.0)).max(x.freq);
                crossover::bandpass(kind, x.order, x.freq, f_high, fs)
            }
        };
        filters.extend(&chain);
    }

    strip.set_filters(filters);
    Ok(strip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_build_stereo_passthrough() {
        let cfg = Config::default();
        let p = cfg.build_pipeline().unwrap();
        assert_eq!(p.n_in(), 2);
        assert_eq!(p.n_out(), 2);
    }

    #[test]
    fn parses_a_two_way_speaker_config() {
        let toml = r#"
sample_rate = 96000
device = "hw:0,0"

[routing]
preset = "mono"

[[channel]]
name = "Woofer"
gain_db = -2.0
delay_cm = 5.0
[[channel.eq]]
kind = "peaking"
freq = 120
q = 2.0
gain_db = 3.0
[channel.crossover]
kind = "linkwitz_riley"
role = "low_pass"
freq = 2200
order = 4

[[channel]]
name = "Tweeter"
invert = true
[channel.crossover]
kind = "linkwitz_riley"
role = "high_pass"
freq = 2200
order = 4
"#;
        let cfg = Config::from_toml_str(toml).unwrap();
        assert_eq!(cfg.sample_rate, 96000);
        assert_eq!(cfg.n_out(), 2);
        let p = cfg.build_pipeline().unwrap();
        assert_eq!(p.n_out(), 2);
    }

    #[test]
    fn custom_matrix_sets_input_width() {
        let toml = r#"
[routing]
preset = "custom"
matrix = [[1.0, 0.0], [0.0, 1.0], [0.5, 0.5]]

[[channel]]
name = "A"
[[channel]]
name = "B"
[[channel]]
name = "C"
"#;
        let cfg = Config::from_toml_str(toml).unwrap();
        let p = cfg.build_pipeline().unwrap();
        assert_eq!(p.n_in(), 2);
        assert_eq!(p.n_out(), 3);
    }
}
