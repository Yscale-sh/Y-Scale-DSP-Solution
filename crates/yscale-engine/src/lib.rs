//! # yscale-engine
//!
//! The real-time audio engine for the Y-Scale-DSP-Solution: ALSA playback to the
//! DAC, built-in signal generators and WAV playback, and a TOML-configured DSP
//! [`Pipeline`](yscale_dsp::Pipeline).

pub mod alsa_out;
pub mod config;
pub mod control;
pub mod engine;
pub mod output;
pub mod source;

pub use config::Config;
pub use control::{spawn as spawn_engine, EngineHandle, Meters};
pub use engine::{run, stop_flag};
pub use output::SampleFormat;
pub use source::{
    Capture, Impulse, LogSweep, PinkNoise, Silence, Sine, Source, SourceSpec, WavFile, WhiteNoise,
};
