//! The real-time loop: pull from a [`Source`], process through the DSP
//! [`Pipeline`], quantize, and write to the DAC via ALSA.

use crate::alsa_out::AlsaOutput;
use crate::config::Config;
use crate::output::{Converter, SampleFormat};
use crate::source::Source;
use anyhow::{bail, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Run the engine until the source ends or `stop` is set. Blocks the caller.
pub fn run(config: &Config, mut source: Box<dyn Source>, stop: Arc<AtomicBool>) -> Result<()> {
    let mut pipeline = config.build_pipeline()?;
    let n_in = pipeline.n_in();
    let n_out = pipeline.n_out();

    if source.channels() != n_in {
        bail!(
            "source provides {} channels but the pipeline expects {} input channels",
            source.channels(),
            n_in
        );
    }
    if source.sample_rate() != config.sample_rate {
        bail!(
            "source is {} Hz but config sample_rate is {} Hz (resampling is not yet supported)",
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
    eprintln!(
        "[yscale] device='{}' {} Hz, {} ch, {:?}, period={} buffer={} frames ({:.1} ms latency)",
        config.device,
        rate,
        n_out,
        format,
        period,
        buffer,
        buffer as f64 / rate as f64 * 1000.0
    );

    let frames = period.max(1) as usize;
    let mut in_buf = vec![0.0f64; frames * n_in];
    let mut out_buf = vec![0.0f64; frames * n_out];
    let mut conv = Converter::new(format, config.dither);

    let mut i32_buf = vec![0i32; frames * n_out];
    let mut i16_buf = vec![0i16; frames * n_out];

    while !stop.load(Ordering::Relaxed) {
        let got = source.fill(&mut in_buf, frames);
        if got == 0 {
            break;
        }
        if got < frames {
            // Zero-pad a short final block.
            for s in in_buf[got * n_in..].iter_mut() {
                *s = 0.0;
            }
        }

        pipeline.process_interleaved(&in_buf, &mut out_buf, frames);

        match format {
            SampleFormat::S32Le => {
                conv.to_i32(&out_buf, &mut i32_buf);
                out.write_i32(&i32_buf)?;
            }
            SampleFormat::S16Le => {
                conv.to_i16(&out_buf, &mut i16_buf);
                out.write_i16(&i16_buf)?;
            }
        }

        if got < frames {
            break;
        }
    }

    out.drain()?;
    Ok(())
}

/// Convenience: a fresh shared stop flag.
pub fn stop_flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
