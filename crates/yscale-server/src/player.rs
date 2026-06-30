//! `player` — the network "streamer brain".
//!
//! A single long-lived `mpv` runs in idle mode and is driven over its JSON IPC
//! socket. It decodes any http/https/hls/dash/file/web-radio URL into the ALSA
//! loopback the engine captures, so everything still flows THROUGH the DSP:
//!
//!   mpv → plughw:Loopback,0,0 → (snd-aloop) → engine Capture → DSP → DAC
//!
//! Unlike a one-shot `gst-launch`, mpv gives us real transport — play/pause,
//! seek, live position/duration — and now-playing metadata, which is what makes
//! this feel like a proper streamer (WiiM-style) instead of a fire-and-forget
//! URL player.

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const SOCKET: &str = "/tmp/yscale-mpv.sock";

/// A snapshot of what the player is doing, sent to the UI.
#[derive(Clone, Debug, Serialize, Default)]
pub struct PlaybackState {
    /// `stopped` | `loading` | `playing` | `paused`
    pub state: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    /// Playback position in seconds.
    pub position: f64,
    /// Track duration in seconds (`0` = unknown / live stream).
    pub duration: f64,
    /// `stream` | `dlna` | `generator` | `idle`
    pub source: String,
    pub url: String,
}

/// Metadata supplied by the caller (e.g. yscale-media) when starting playback.
#[derive(Clone, Debug, Default)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub source: String,
}

struct Inner {
    /// Metadata for the current URL load (overlaid on mpv's own tags).
    meta: Mutex<TrackMeta>,
    /// Latest mpv-derived state (refreshed by the poll thread).
    cache: Mutex<PlaybackState>,
    /// When set, a non-mpv source (generator/DLNA) is active and `snapshot`
    /// returns this verbatim instead of the mpv cache.
    external: Mutex<Option<PlaybackState>>,
    /// True between a `load()` and mpv actually starting decode.
    loading: AtomicBool,
}

impl Inner {
    /// Send a batch of mpv commands over one connection; return each `data`
    /// (or `None` on per-command error / missing reply), index-aligned.
    fn query(&self, cmds: &[Value]) -> Result<Vec<Option<Value>>> {
        let stream = UnixStream::connect(SOCKET).context("connect mpv ipc")?;
        stream.set_read_timeout(Some(Duration::from_millis(800)))?;
        let mut w = stream.try_clone()?;
        for (i, c) in cmds.iter().enumerate() {
            let req = json!({ "command": c, "request_id": i as i64 + 1 });
            let mut line = serde_json::to_string(&req)?;
            line.push('\n');
            w.write_all(line.as_bytes())?;
        }
        w.flush()?;

        let mut out = vec![None; cmds.len()];
        let mut filled = 0usize;
        for l in BufReader::new(stream).lines() {
            let l = match l {
                Ok(l) => l,
                Err(_) => break, // read timeout → stop waiting
            };
            let v: Value = match serde_json::from_str(&l) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // Async event lines have no request_id; skip them.
            let Some(id) = v.get("request_id").and_then(Value::as_i64) else {
                continue;
            };
            let idx = (id - 1) as usize;
            if idx < out.len() && out[idx].is_none() {
                let ok = v.get("error").and_then(Value::as_str) == Some("success");
                out[idx] = Some(if ok {
                    v.get("data").cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                });
                filled += 1;
                if filled == out.len() {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Fire a single command, returning its `data`.
    fn cmd(&self, args: Value) -> Result<Value> {
        self.query(&[args])?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| anyhow!("no reply from mpv"))
    }

    /// Refresh `cache` from mpv (called by the poll thread).
    fn refresh(&self) {
        let props = [
            "idle-active",
            "pause",
            "time-pos",
            "duration",
            "media-title",
            "metadata",
            "eof-reached",
        ];
        let cmds: Vec<Value> = props.iter().map(|p| json!(["get_property", p])).collect();
        let res = match self.query(&cmds) {
            Ok(r) => r,
            Err(_) => return, // mpv momentarily unreachable; keep last cache
        };
        let getb = |i: usize| res.get(i).and_then(|o| o.as_ref()).and_then(Value::as_bool);
        let getf = |i: usize| {
            res.get(i)
                .and_then(|o| o.as_ref())
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        };
        let gets = |i: usize| {
            res.get(i)
                .and_then(|o| o.as_ref())
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };

        let idle = getb(0).unwrap_or(true);
        let paused = getb(1).unwrap_or(false);
        let position = getf(2);
        let duration = getf(3);
        let media_title = gets(4);
        let md = res.get(5).and_then(|o| o.clone());

        if !idle {
            self.loading.store(false, Ordering::Relaxed);
        }
        let loading = self.loading.load(Ordering::Relaxed);
        let state = if loading {
            "loading"
        } else if idle {
            "stopped"
        } else if paused {
            "paused"
        } else {
            "playing"
        };

        // Prefer caller-supplied metadata; fall back to mpv's own tags.
        let meta = self.meta.lock().unwrap().clone();
        let tag = |key: &str| md_lookup(md.as_ref(), key);
        let title = first_nonempty(&[meta.title.clone(), tag("title"), media_title.clone()]);
        let artist = first_nonempty(&[meta.artist.clone(), tag("artist"), tag("album_artist")]);
        let album = first_nonempty(&[meta.album.clone(), tag("album")]);

        *self.cache.lock().unwrap() = PlaybackState {
            state: state.to_string(),
            title,
            artist,
            album,
            art_url: meta.art_url.clone(),
            position,
            duration,
            source: if !meta.source.is_empty() {
                meta.source.clone()
            } else if idle {
                "idle".into()
            } else {
                "stream".into()
            },
            url: String::new(),
        };
    }
}

/// Handle to the running mpv player. Wrap in an `Arc` to share across the axum
/// state (see `AppState`) so `Drop` — which kills mpv — fires only at shutdown,
/// not when a per-request state clone is dropped.
pub struct Player {
    inner: Arc<Inner>,
    child: Arc<Mutex<Child>>,
    stop: Arc<AtomicBool>,
}

impl Player {
    /// Spawn mpv idle, feeding `audio_device` (e.g. `plughw:Loopback,0,0`) at
    /// `samplerate` Hz so the loopback runs at the engine's clock.
    pub fn start(audio_device: &str, samplerate: u32) -> Result<Player> {
        // A previous run's mpv can outlive the server across a restart and keep
        // holding the IPC socket, which blocks our new instance. Clear any stray
        // mpv bound to our socket path before (re)starting.
        let _ = Command::new("pkill").args(["-f", SOCKET]).status();
        let _ = std::fs::remove_file(SOCKET);
        let child = Command::new("mpv")
            .args([
                "--idle=yes",
                "--no-video",
                "--no-terminal",
                "--no-config",
                "--gapless-audio=yes",
                "--force-seekable=yes",
                "--cache=yes",
                "--keep-open=no",
                "--volume=100",
                "--audio-format=s32",
            ])
            .arg(format!("--audio-device=alsa/{audio_device}"))
            .arg(format!("--audio-samplerate={samplerate}"))
            .arg(format!("--input-ipc-server={SOCKET}"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("could not start mpv (is it installed? `sudo apt install mpv`)")?;

        // Wait for the IPC socket to appear (mpv creates it shortly after start).
        let mut ready = false;
        for _ in 0..60 {
            if Path::new(SOCKET).exists() {
                ready = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !ready {
            bail!("mpv did not create its IPC socket at {SOCKET}");
        }

        let inner = Arc::new(Inner {
            meta: Mutex::new(TrackMeta::default()),
            cache: Mutex::new(PlaybackState {
                state: "stopped".into(),
                source: "idle".into(),
                ..Default::default()
            }),
            external: Mutex::new(None),
            loading: AtomicBool::new(false),
        });
        let stop = Arc::new(AtomicBool::new(false));

        {
            let inner = inner.clone();
            let stop = stop.clone();
            thread::Builder::new()
                .name("yscale-mpv-poll".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        inner.refresh();
                        thread::sleep(Duration::from_millis(300));
                    }
                })?;
        }

        Ok(Player {
            inner,
            child: Arc::new(Mutex::new(child)),
            stop,
        })
    }

    /// Start playing a URL with optional caller-supplied metadata. Clears any
    /// external-source override.
    pub fn load(&self, url: &str, meta: TrackMeta) -> Result<()> {
        *self.inner.external.lock().unwrap() = None;
        let mut m = meta;
        if m.source.is_empty() {
            m.source = "stream".into();
        }
        *self.inner.meta.lock().unwrap() = m.clone();
        self.inner.loading.store(true, Ordering::Relaxed);
        // Reflect "loading" immediately so the UI updates before the poll tick.
        *self.inner.cache.lock().unwrap() = PlaybackState {
            state: "loading".into(),
            title: m.title,
            artist: m.artist,
            album: m.album,
            art_url: m.art_url,
            source: m.source,
            ..Default::default()
        };
        self.inner.cmd(json!(["loadfile", url, "replace"]))?;
        self.inner.cmd(json!(["set_property", "pause", false]))?;
        Ok(())
    }

    /// Pause / resume. `None` toggles.
    pub fn set_pause(&self, paused: Option<bool>) -> Result<()> {
        let target = match paused {
            Some(p) => p,
            None => !self
                .inner
                .cmd(json!(["get_property", "pause"]))?
                .as_bool()
                .unwrap_or(false),
        };
        self.inner.cmd(json!(["set_property", "pause", target]))?;
        Ok(())
    }

    /// Stop playback and return mpv to idle.
    pub fn stop(&self) -> Result<()> {
        *self.inner.external.lock().unwrap() = None;
        *self.inner.meta.lock().unwrap() = TrackMeta::default();
        self.inner.loading.store(false, Ordering::Relaxed);
        *self.inner.cache.lock().unwrap() = PlaybackState {
            state: "stopped".into(),
            source: "idle".into(),
            ..Default::default()
        };
        self.inner.cmd(json!(["stop"]))?;
        Ok(())
    }

    /// Seek to an absolute position in seconds.
    pub fn seek(&self, position: f64) -> Result<()> {
        self.inner
            .cmd(json!(["seek", position.max(0.0), "absolute"]))?;
        Ok(())
    }

    /// Mark a non-mpv source active (generator / DLNA capture) so the UI shows
    /// it. Stops mpv so it isn't also feeding the loopback.
    pub fn set_external(&self, source: &str, title: &str) {
        let _ = self.inner.cmd(json!(["stop"]));
        self.inner.loading.store(false, Ordering::Relaxed);
        *self.inner.external.lock().unwrap() = Some(PlaybackState {
            state: if source == "idle" { "stopped" } else { "playing" }.into(),
            title: title.to_string(),
            source: source.to_string(),
            ..Default::default()
        });
    }

    /// Current playback state for the UI.
    pub fn snapshot(&self) -> PlaybackState {
        if let Some(e) = self.inner.external.lock().unwrap().clone() {
            return e;
        }
        self.inner.cache.lock().unwrap().clone()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Case-insensitive lookup in mpv's `metadata` map.
fn md_lookup(md: Option<&Value>, key: &str) -> String {
    let Some(Value::Object(map)) = md else {
        return String::new();
    };
    for (k, v) in map {
        if k.eq_ignore_ascii_case(key) {
            if let Some(s) = v.as_str() {
                return s.to_string();
            }
        }
    }
    String::new()
}

fn first_nonempty(cands: &[String]) -> String {
    cands
        .iter()
        .find(|s| !s.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}
