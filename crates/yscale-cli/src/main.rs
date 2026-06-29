//! `yscale` — headless runner for the Y-Scale-DSP engine.
//!
//! Pick a source (signal generator or WAV file); audio flows through the DSP
//! graph defined by `--config` (or a stereo pass-through by default) out to the
//! DAC. Press Ctrl-C to stop.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use yscale_engine::{Config, Impulse, LogSweep, PinkNoise, Sine, Source, WavFile, WhiteNoise};

#[derive(Parser)]
#[command(
    name = "yscale",
    version,
    about = "Y-Scale-DSP — headless high-fidelity DSP engine for speaker testing"
)]
struct Cli {
    /// DSP configuration file (TOML). Defaults to stereo pass-through.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Override the ALSA output device (e.g. hw:0,0).
    #[arg(long)]
    device: Option<String>,

    /// Override the sample rate in Hz.
    #[arg(long)]
    rate: Option<u32>,

    #[command(subcommand)]
    source: SourceCmd,
}

#[derive(Subcommand)]
enum SourceCmd {
    /// Continuous sine tone.
    Sine {
        #[arg(long, default_value_t = 1000.0)]
        freq: f64,
        #[arg(long, default_value_t = 0.25)]
        amp: f64,
    },
    /// Exponential log sweep (great for eyeballing frequency response).
    Sweep {
        #[arg(long, default_value_t = 20.0)]
        f1: f64,
        #[arg(long, default_value_t = 20000.0)]
        f2: f64,
        #[arg(long, default_value_t = 10.0)]
        dur: f64,
        #[arg(long, default_value_t = 0.25)]
        amp: f64,
        #[arg(long = "loop")]
        looping: bool,
    },
    /// Pink noise (-3 dB/oct), the room/speaker reference.
    Pink {
        #[arg(long, default_value_t = 0.25)]
        amp: f64,
    },
    /// White noise (flat).
    White {
        #[arg(long, default_value_t = 0.25)]
        amp: f64,
    },
    /// Impulse(s) for time-of-arrival / IR checks.
    Impulse {
        /// Repeat every N milliseconds (omit for a single impulse).
        #[arg(long)]
        period_ms: Option<f64>,
        #[arg(long, default_value_t = 0.5)]
        amp: f64,
    },
    /// Play a WAV file.
    File {
        path: PathBuf,
        #[arg(long = "loop")]
        looping: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut config = match &cli.config {
        Some(p) => Config::from_path(p)?,
        None => Config::default(),
    };
    if let Some(d) = cli.device {
        config.device = d;
    }
    if let Some(r) = cli.rate {
        config.sample_rate = r;
    }

    let fs = config.sample_rate;
    let n_in = config.n_in();

    let source: Box<dyn Source> = match cli.source {
        SourceCmd::Sine { freq, amp } => Box::new(Sine::new(fs, n_in, freq, amp)),
        SourceCmd::Sweep {
            f1,
            f2,
            dur,
            amp,
            looping,
        } => Box::new(LogSweep::new(fs, n_in, f1, f2, dur, amp, looping)),
        SourceCmd::Pink { amp } => Box::new(PinkNoise::new(fs, n_in, amp, 0xC0FFEE)),
        SourceCmd::White { amp } => Box::new(WhiteNoise::new(fs, n_in, amp, 0xBEEF)),
        SourceCmd::Impulse { period_ms, amp } => {
            let period = period_ms.map(|ms| (ms * 1e-3 * fs as f64) as u64);
            Box::new(Impulse::new(fs, n_in, amp, period))
        }
        SourceCmd::File { path, looping } => Box::new(WavFile::open(&path, n_in, looping)?),
    };

    // No resampler yet: align the engine sample rate to the source's.
    config.sample_rate = source.sample_rate();

    let stop = yscale_engine::stop_flag();
    {
        let s = stop.clone();
        ctrlc::set_handler(move || s.store(true, Ordering::SeqCst))?;
    }

    eprintln!("[yscale] starting; press Ctrl-C to stop");
    yscale_engine::run(&config, source, stop)?;
    eprintln!("[yscale] stopped cleanly");
    Ok(())
}
