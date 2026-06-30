//! Master volume — drives the DAC's hardware `Digital` control via `amixer`.
//!
//! The PCM5122's digital volume is the right master knob: it's 32-bit and
//! hardware-ramped (no zipper noise), and attenuating in the digital domain on a
//! 32-bit DAC is effectively lossless at listening levels. We expose it as a
//! WiiM-style 0–100 %, mapped linearly in dB over [-100, 0] dB (perceptually
//! even — a fader, not a raw register). The wide floor is deliberate: into a hot
//! input (e.g. the DAC's headphone jack) even -60 dB is loud, so the bottom of
//! the slider reaches near-silence. Persistence is server-owned (a file in
//! `$HOME/.config`), re-applied on boot, so no `sudo`/alsactl is needed.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

const CARD: &str = "BossDAC";
const CONTROL: &str = "Digital";
const MIN_DB: f64 = -100.0;
const MAX_DB: f64 = 0.0;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct VolumeState {
    pub pct: f64,
    pub db: f64,
    pub muted: bool,
}

pub struct Volume {
    inner: Mutex<Inner>,
    persist_path: Option<PathBuf>,
}
struct Inner {
    pct: f64,
    muted: bool,
}

fn pct_to_db(pct: f64) -> f64 {
    let p = (pct / 100.0).clamp(0.0, 1.0);
    MIN_DB + p * (MAX_DB - MIN_DB)
}
fn db_to_pct(db: f64) -> f64 {
    (((db - MIN_DB) / (MAX_DB - MIN_DB)) * 100.0).clamp(0.0, 100.0)
}
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn amixer_set_db(db: f64) -> Result<()> {
    let out = Command::new("amixer")
        .args(["-c", CARD, "--", "sset", CONTROL, &format!("{db:.2}dB")])
        .output()
        .context("run amixer (is alsa-utils installed?)")?;
    if !out.status.success() {
        bail!("amixer set failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}
/// Mute by setting the register to 0 (≈ -103 dB — effectively silent).
fn amixer_mute() -> Result<()> {
    let out = Command::new("amixer")
        .args(["-c", CARD, "sset", CONTROL, "0"])
        .output()
        .context("run amixer")?;
    if !out.status.success() {
        bail!("amixer mute failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}
/// Parse the current dB from `amixer sget` output (first `[-12.34dB]` token).
fn amixer_get_db() -> Option<f64> {
    let out = Command::new("amixer")
        .args(["-c", CARD, "sget", CONTROL])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    for tok in s.split(|c| c == '[' || c == ']') {
        if let Some(num) = tok.trim().strip_suffix("dB") {
            if let Ok(v) = num.trim().parse::<f64>() {
                return Some(v);
            }
        }
    }
    None
}

impl Volume {
    /// Read the persisted level (or the DAC's current level) and apply it so the
    /// hardware matches our state from the first frame.
    pub fn new() -> Self {
        let persist_path = std::env::var_os("HOME").map(|h| {
            let mut p = PathBuf::from(h);
            p.push(".config/yscale-server");
            p.push("volume");
            p
        });
        let persisted = persist_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| s.trim().parse::<f64>().ok());
        let pct = persisted
            .or_else(|| amixer_get_db().map(db_to_pct))
            .unwrap_or(45.0)
            .clamp(0.0, 100.0);

        let v = Self {
            inner: Mutex::new(Inner { pct, muted: false }),
            persist_path,
        };
        if let Err(e) = amixer_set_db(pct_to_db(pct)) {
            eprintln!("[yscale] volume: could not set initial level: {e}");
        }
        v
    }

    pub fn state(&self) -> VolumeState {
        let i = self.inner.lock().unwrap();
        VolumeState {
            pct: round1(i.pct),
            db: round1(pct_to_db(i.pct)),
            muted: i.muted,
        }
    }

    pub fn set_pct(&self, pct: f64) -> Result<VolumeState> {
        let pct = pct.clamp(0.0, 100.0);
        {
            let mut i = self.inner.lock().unwrap();
            i.pct = pct;
            i.muted = false;
        }
        amixer_set_db(pct_to_db(pct))?;
        self.persist(pct);
        Ok(self.state())
    }

    pub fn set_muted(&self, muted: bool) -> Result<VolumeState> {
        let pct = {
            let mut i = self.inner.lock().unwrap();
            i.muted = muted;
            i.pct
        };
        if muted {
            amixer_mute()?;
        } else {
            amixer_set_db(pct_to_db(pct))?;
        }
        Ok(self.state())
    }

    fn persist(&self, pct: f64) {
        if let Some(p) = &self.persist_path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, format!("{pct:.2}"));
        }
    }
}
