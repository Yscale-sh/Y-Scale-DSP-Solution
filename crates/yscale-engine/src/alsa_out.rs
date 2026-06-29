//! Thin ALSA playback wrapper around the DAC, with underrun recovery.

use crate::output::SampleFormat;
use alsa::pcm::{Access, Format, HwParams, PCM};
use alsa::{Direction, ValueOr};
use anyhow::{Context, Result};

/// An open ALSA playback device configured for interleaved integer PCM.
pub struct AlsaOutput {
    pcm: PCM,
    channels: usize,
    format: SampleFormat,
}

impl AlsaOutput {
    /// Open `device` (e.g. `hw:CARD=sndrpihifiberry,DEV=0`) for playback.
    pub fn open(
        device: &str,
        sample_rate: u32,
        channels: usize,
        format: SampleFormat,
        period_frames: u32,
        buffer_frames: u32,
    ) -> Result<Self> {
        let pcm = PCM::new(device, Direction::Playback, false)
            .with_context(|| format!("opening ALSA device '{device}'"))?;
        {
            let hwp = HwParams::any(&pcm)?;
            hwp.set_channels(channels as u32)?;
            hwp.set_rate(sample_rate, ValueOr::Nearest)?;
            hwp.set_access(Access::RWInterleaved)?;
            hwp.set_format(match format {
                SampleFormat::S16Le => Format::s16(),
                SampleFormat::S32Le => Format::s32(),
            })?;
            hwp.set_buffer_size_near(buffer_frames as i64)?;
            hwp.set_period_size_near(period_frames as i64, ValueOr::Nearest)?;
            pcm.hw_params(&hwp)
                .context("applying ALSA hardware parameters")?;
        }
        // Software params: don't start playback until the buffer is nearly full,
        // so the first writes can't underrun on a slow Pi (the classic startup xrun).
        {
            let hwp = pcm.hw_params_current()?;
            let buffer = hwp.get_buffer_size()?;
            let period = hwp.get_period_size()?;
            let swp = pcm.sw_params_current()?;
            swp.set_start_threshold(buffer - period)?;
            swp.set_avail_min(period)?;
            pcm.sw_params(&swp)
                .context("applying ALSA software parameters")?;
        }
        pcm.prepare().context("preparing ALSA device")?;
        Ok(Self {
            pcm,
            channels,
            format,
        })
    }

    pub fn format(&self) -> SampleFormat {
        self.format
    }

    /// Report the negotiated rate / period / buffer for logging.
    pub fn actual_params(&self) -> Result<(u32, u32, u32)> {
        let hwp = self.pcm.hw_params_current()?;
        Ok((
            hwp.get_rate()?,
            hwp.get_period_size()? as u32,
            hwp.get_buffer_size()? as u32,
        ))
    }

    /// Write a full interleaved 32-bit buffer, recovering from underruns.
    pub fn write_i32(&self, buf: &[i32]) -> Result<()> {
        let total = buf.len() / self.channels;
        let io = self.pcm.io_i32()?;
        let mut written = 0usize;
        while written < total {
            let slice = &buf[written * self.channels..];
            match io.writei(slice) {
                Ok(n) => written += n,
                Err(e) => {
                    self.pcm.try_recover(e, true).context("ALSA underrun")?;
                }
            }
        }
        Ok(())
    }

    /// Write a full interleaved 16-bit buffer, recovering from underruns.
    pub fn write_i16(&self, buf: &[i16]) -> Result<()> {
        let total = buf.len() / self.channels;
        let io = self.pcm.io_i16()?;
        let mut written = 0usize;
        while written < total {
            let slice = &buf[written * self.channels..];
            match io.writei(slice) {
                Ok(n) => written += n,
                Err(e) => {
                    self.pcm.try_recover(e, true).context("ALSA underrun")?;
                }
            }
        }
        Ok(())
    }

    /// Drain remaining buffered audio.
    pub fn drain(&self) -> Result<()> {
        self.pcm.drain().ok();
        Ok(())
    }
}
