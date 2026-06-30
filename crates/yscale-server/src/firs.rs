//! Stored **FIR impulse responses** for room correction / linear-phase filters.
//!
//! Uploaded as a mono (or multi-channel — channel 0 is taken) WAV, or a text
//! list of coefficients, and persisted as little-endian `f64` under
//! `$HOME/.config/yscale-server/firs/<name>.fir`. The engine loads the named
//! coefficients and convolves them per output channel.

use anyhow::{anyhow, bail, Result};
use std::io::Cursor;
use std::path::PathBuf;

/// Cap a FIR's length (16384 taps ≈ 341 ms @ 48 kHz — ample for room correction,
/// and bounded so a single block's FFT stays within the realtime budget).
const MAX_TAPS: usize = 16_384;

pub struct Firs {
    dir: PathBuf,
}

impl Firs {
    pub fn new() -> Self {
        let mut dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        dir.push(".config/yscale-server/firs");
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    fn sanitize(name: &str) -> Result<String> {
        let n = name.trim().trim_end_matches(".fir");
        if n.is_empty() || n.len() > 64 {
            bail!("FIR name must be 1–64 characters");
        }
        if !n
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        {
            bail!("FIR name: letters, numbers, space, '-', '_' and '.' only");
        }
        Ok(n.to_string())
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        Ok(self.dir.join(format!("{}.fir", Self::sanitize(name)?)))
    }

    /// `(name, taps)` for every stored FIR, alphabetical.
    pub fn list(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("fir") {
                    return None;
                }
                let name = p.file_stem().and_then(|s| s.to_str())?.to_string();
                let taps = e.metadata().ok().map(|m| m.len() as usize / 8).unwrap_or(0);
                Some((name, taps))
            })
            .collect();
        out.sort_by_key(|(n, _)| n.to_lowercase());
        out
    }

    /// Load a FIR's coefficients.
    pub fn load(&self, name: &str) -> Result<Vec<f64>> {
        let bytes = std::fs::read(self.path(name)?).map_err(|_| anyhow!("FIR '{name}' not found"))?;
        Ok(bytes
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
            .collect())
    }

    /// Parse an uploaded WAV / text body into coefficients and store it. Returns
    /// the number of taps kept.
    pub fn save(&self, name: &str, body: &[u8]) -> Result<usize> {
        let mut coeffs = parse(body)?;
        if coeffs.is_empty() {
            bail!("no coefficients found in upload");
        }
        if coeffs.len() > MAX_TAPS {
            coeffs.truncate(MAX_TAPS);
        }
        let mut out = Vec::with_capacity(coeffs.len() * 8);
        for c in &coeffs {
            out.extend_from_slice(&c.to_le_bytes());
        }
        std::fs::write(self.path(name)?, out)?;
        Ok(coeffs.len())
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        std::fs::remove_file(self.path(name)?).map_err(|_| anyhow!("FIR '{name}' not found"))?;
        Ok(())
    }
}

/// Parse a WAV (RIFF) or a whitespace/comma-separated text list into coeffs.
fn parse(body: &[u8]) -> Result<Vec<f64>> {
    if body.len() >= 4 && &body[..4] == b"RIFF" {
        let mut reader = hound::WavReader::new(Cursor::new(body))?;
        let spec = reader.spec();
        let ch = (spec.channels as usize).max(1);
        let samples: Vec<f64> = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .map(|s| s.unwrap_or(0.0) as f64)
                .collect(),
            hound::SampleFormat::Int => {
                let max = (1i64 << (spec.bits_per_sample.max(1) - 1)) as f64;
                reader
                    .samples::<i32>()
                    .map(|s| s.unwrap_or(0) as f64 / max)
                    .collect()
            }
        };
        // De-interleave channel 0.
        Ok(samples.into_iter().step_by(ch).collect())
    } else {
        let text = std::str::from_utf8(body).map_err(|_| anyhow!("upload is neither WAV nor text"))?;
        text.split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<f64>().map_err(|_| anyhow!("invalid coefficient '{s}'")))
            .collect()
    }
}
