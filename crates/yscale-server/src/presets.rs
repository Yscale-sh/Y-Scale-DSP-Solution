//! Named tuning **presets** ("scenes") — save and recall the full DSP graph
//! (channels, routing, EQ, crossovers, delays, gains, limiter). Stored as JSON
//! under `$HOME/.config/yscale-server/presets/`, one file per preset.

use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;
use yscale_engine::Config;

pub struct Presets {
    dir: PathBuf,
}

impl Presets {
    pub fn new() -> Self {
        let mut dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        dir.push(".config/yscale-server/presets");
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    /// Validate a preset name (also prevents path traversal).
    fn sanitize(name: &str) -> Result<String> {
        let n = name.trim();
        if n.is_empty() || n.len() > 64 {
            bail!("preset name must be 1–64 characters");
        }
        if !n
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        {
            bail!("preset name: letters, numbers, space, '-', '_' and '.' only");
        }
        Ok(n.to_string())
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        Ok(self.dir.join(format!("{}.json", Self::sanitize(name)?)))
    }

    /// Preset names, alphabetical.
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("json") {
                    p.file_stem().and_then(|s| s.to_str()).map(str::to_string)
                } else {
                    None
                }
            })
            .collect();
        names.sort_by_key(|s| s.to_lowercase());
        names
    }

    /// Save `cfg` under `name` (overwrites).
    pub fn save(&self, name: &str, cfg: &Config) -> Result<()> {
        std::fs::write(self.path(name)?, serde_json::to_string_pretty(cfg)?)?;
        Ok(())
    }

    /// Load a preset's config.
    pub fn load(&self, name: &str) -> Result<Config> {
        let path = self.path(name)?;
        let text =
            std::fs::read_to_string(&path).map_err(|_| anyhow!("preset '{name}' not found"))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        std::fs::remove_file(self.path(name)?)
            .map_err(|_| anyhow!("preset '{name}' not found"))?;
        Ok(())
    }
}
