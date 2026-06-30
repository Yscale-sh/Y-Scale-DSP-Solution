//! `yscale-server` — axum web server: a WiiM-style network streamer + live DSP.
//!
//! It owns the DAC engine and an mpv "streamer brain", and exposes:
//!   GET  /api/config   current DSP graph (JSON)
//!   PUT  /api/config   replace the DSP graph live (rebuilds + hot-swaps)
//!   POST /api/source   set a test source live (signal generator / WAV / DLNA)
//!   POST /api/play     play a URL through the DSP (+ optional now-playing meta)
//!   POST /api/pause    pause / resume / toggle transport
//!   POST /api/stop     stop playback
//!   POST /api/seek     seek to an absolute position (seconds)
//!   GET  /api/now      full player + volume + meter snapshot
//!   GET  /api/volume   master volume (DAC Digital)
//!   PUT  /api/volume   set master volume / mute
//!   GET  /api/status   sample rate / channels / live meters
//!   GET  /ws           WebSocket: live meters + now-playing + volume

mod player;
mod volume;

use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use player::{Player, TrackMeta};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use volume::Volume;
use yscale_engine::{spawn_engine, Config, EngineHandle, Silence, SourceSpec};

/// ALSA loopback the URL/DLNA players feed; the engine captures the other end.
const LOOPBACK_PLAYBACK: &str = "plughw:Loopback,0,0";
const LOOPBACK_CAPTURE: &str = "plughw:Loopback,1,0";

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

#[derive(Parser)]
#[command(name = "yscale-server", version, about = "Y-Scale-DSP web control server")]
struct Cli {
    /// DSP config file (TOML). Defaults to stereo pass-through.
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Override ALSA output device.
    #[arg(long)]
    device: Option<String>,
    /// Override sample rate (Hz).
    #[arg(long)]
    rate: Option<u32>,
    /// Address to bind (host:port).
    #[arg(long, default_value = "0.0.0.0:8080")]
    bind: String,
}

#[derive(Clone)]
struct AppState {
    engine: Arc<EngineHandle>,
    config: Arc<Mutex<Config>>,
    /// The mpv streamer brain (absent if mpv failed to start). Held behind an
    /// `Arc` so `Player::drop` (which kills mpv) only fires at shutdown, not when
    /// axum drops a per-request clone of the state.
    player: Option<Arc<Player>>,
    volume: Arc<Volume>,
}

/// Swap the engine source. If the new source opens an exclusive ALSA device
/// (the loopback capture), first switch to silence so the RT thread *releases*
/// the currently-held capture before we open it again — otherwise the
/// double-open fails with EBUSY ("device or resource busy").
async fn swap_source_releasing(s: &AppState, spec: SourceSpec) -> Result<(), AppError> {
    if matches!(spec, SourceSpec::Capture { .. }) {
        s.engine
            .swap_source(Box::new(Silence::new(s.engine.sample_rate, s.engine.n_in)));
        // > a couple of engine periods so the RT thread drops the old source.
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    let src = spec
        .build(s.engine.sample_rate, s.engine.n_in)
        .map_err(AppError::bad)?;
    s.engine.swap_source(src);
    Ok(())
}

fn player_or_err(s: &AppState) -> Result<&Player, AppError> {
    s.player.as_deref().ok_or_else(|| {
        AppError(
            StatusCode::SERVICE_UNAVAILABLE,
            "player unavailable (mpv not running on the device)".into(),
        )
    })
}

#[tokio::main]
async fn main() -> Result<()> {
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

    // Start the engine with a silent source; the UI picks a source from there.
    let n_in = config.n_in();
    let source = SourceSpec::Silence.build(config.sample_rate, n_in)?;
    let engine = Arc::new(spawn_engine(&config, source)?);

    // Start the mpv streamer brain feeding the loopback at the engine's clock.
    let player = match Player::start(LOOPBACK_PLAYBACK, engine.sample_rate) {
        Ok(p) => {
            println!("[yscale-server] mpv streamer brain ready");
            Some(Arc::new(p))
        }
        Err(e) => {
            eprintln!("[yscale-server] WARNING: streamer brain disabled: {e}");
            None
        }
    };

    let volume = Arc::new(Volume::new());
    println!("[yscale-server] master volume: {:?}", volume.state());

    let state = AppState {
        engine,
        config: Arc::new(Mutex::new(config)),
        player,
        volume,
    };

    let app = Router::new()
        .route("/api/config", get(get_config).put(put_config))
        .route("/api/source", post(post_source))
        .route("/api/play", post(post_play))
        .route("/api/pause", post(post_pause))
        .route("/api/stop", post(post_stop))
        .route("/api/seek", post(post_seek))
        .route("/api/now", get(get_now))
        .route("/api/volume", get(get_volume).put(put_volume))
        .route("/api/status", get(get_status))
        .route("/ws", get(ws_handler))
        .fallback(static_handler)
        .with_state(state);

    let addr: SocketAddr = cli.bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("[yscale-server] listening on http://{addr}  — open it from any device on the LAN");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_config(State(s): State<AppState>) -> Json<Config> {
    Json(s.config.lock().unwrap().clone())
}

async fn put_config(
    State(s): State<AppState>,
    Json(mut new): Json<Config>,
) -> Result<Json<Value>, AppError> {
    // Device / rate / format are fixed at engine start; keep them from the
    // running engine so filter coefficients stay matched to the actual clock.
    new.sample_rate = s.engine.sample_rate;
    let pipeline = new.build_pipeline().map_err(AppError::bad)?;
    if pipeline.n_in() != s.engine.n_in || pipeline.n_out() != s.engine.n_out {
        return Err(AppError::bad(anyhow::anyhow!(
            "channel count can't change live (got {}in/{}out, engine is {}in/{}out)",
            pipeline.n_in(),
            pipeline.n_out(),
            s.engine.n_in,
            s.engine.n_out
        )));
    }
    s.engine.swap_pipeline(pipeline);
    *s.config.lock().unwrap() = new;
    Ok(Json(json!({ "ok": true })))
}

/// Human label for a test source, for the now-playing display.
fn source_label(spec: &SourceSpec) -> (&'static str, String) {
    match spec {
        SourceSpec::Capture { .. } => ("dlna", "DLNA / Stream In".into()),
        SourceSpec::Silence => ("idle", String::new()),
        SourceSpec::Sine { freq, .. } => ("generator", format!("Sine · {freq:.0} Hz")),
        SourceSpec::Sweep { f1, f2, .. } => ("generator", format!("Sweep · {f1:.0}–{f2:.0} Hz")),
        SourceSpec::Pink { .. } => ("generator", "Pink noise".into()),
        SourceSpec::White { .. } => ("generator", "White noise".into()),
        SourceSpec::Impulse { .. } => ("generator", "Impulse".into()),
        SourceSpec::File { path, .. } => (
            "generator",
            format!("File · {}", path.rsplit('/').next().unwrap_or(path)),
        ),
    }
}

async fn post_source(
    State(s): State<AppState>,
    Json(spec): Json<SourceSpec>,
) -> Result<Json<Value>, AppError> {
    // A test source takes over the loopback/engine input — stop the URL player
    // and reflect the source in now-playing.
    let (source, label) = source_label(&spec);
    if let Some(p) = &s.player {
        p.set_external(source, &label);
    }
    swap_source_releasing(&s, spec).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct PlayReq {
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    album: String,
    #[serde(default)]
    art: String,
    #[serde(default)]
    source: String,
}

/// Play any HTTP(S)/HLS/DASH/file stream URL THROUGH the DSP via mpv, with
/// optional now-playing metadata (yscale-media sends title/artist/art).
async fn post_play(
    State(s): State<AppState>,
    Json(req): Json<PlayReq>,
) -> Result<Json<Value>, AppError> {
    let url = req.url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")) {
        return Err(AppError::bad(anyhow::anyhow!(
            "url must start with http://, https:// or file://"
        )));
    }
    let player = player_or_err(&s)?;
    player
        .load(
            &url,
            TrackMeta {
                title: req.title,
                artist: req.artist,
                album: req.album,
                art_url: req.art,
                source: req.source,
            },
        )
        .map_err(AppError::bad)?;

    // Route the loopback through the DSP (releasing any prior capture first).
    swap_source_releasing(
        &s,
        SourceSpec::Capture {
            device: LOOPBACK_CAPTURE.to_string(),
        },
    )
    .await?;

    Ok(Json(json!({ "ok": true, "playing": url })))
}

#[derive(Deserialize)]
struct PauseReq {
    #[serde(default)]
    paused: Option<bool>,
}

async fn post_pause(
    State(s): State<AppState>,
    Json(req): Json<PauseReq>,
) -> Result<Json<Value>, AppError> {
    player_or_err(&s)?
        .set_pause(req.paused)
        .map_err(AppError::bad)?;
    Ok(Json(json!({ "ok": true })))
}

async fn post_stop(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    if let Some(p) = &s.player {
        let _ = p.stop();
    }
    s.engine
        .swap_source(Box::new(Silence::new(s.engine.sample_rate, s.engine.n_in)));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct SeekReq {
    position: f64,
}

async fn post_seek(
    State(s): State<AppState>,
    Json(req): Json<SeekReq>,
) -> Result<Json<Value>, AppError> {
    player_or_err(&s)?
        .seek(req.position)
        .map_err(AppError::bad)?;
    Ok(Json(json!({ "ok": true })))
}

fn now_snapshot(s: &AppState) -> Value {
    let now = s.player.as_ref().map(|p| p.snapshot()).unwrap_or_default();
    json!({
        "now": now,
        "volume": s.volume.state(),
        "meters": s.engine.meters(),
        "sample_rate": s.engine.sample_rate,
        "n_in": s.engine.n_in,
        "n_out": s.engine.n_out,
    })
}

async fn get_now(State(s): State<AppState>) -> Json<Value> {
    Json(now_snapshot(&s))
}

async fn get_volume(State(s): State<AppState>) -> Json<volume::VolumeState> {
    Json(s.volume.state())
}

#[derive(Deserialize)]
struct VolReq {
    #[serde(default)]
    pct: Option<f64>,
    #[serde(default)]
    muted: Option<bool>,
}

async fn put_volume(
    State(s): State<AppState>,
    Json(req): Json<VolReq>,
) -> Result<Json<volume::VolumeState>, AppError> {
    if let Some(m) = req.muted {
        s.volume.set_muted(m).map_err(AppError::bad)?;
    }
    if let Some(pct) = req.pct {
        s.volume.set_pct(pct).map_err(AppError::bad)?;
    }
    Ok(Json(s.volume.state()))
}

async fn get_status(State(s): State<AppState>) -> Json<Value> {
    Json(json!({
        "sample_rate": s.engine.sample_rate,
        "n_in": s.engine.n_in,
        "n_out": s.engine.n_out,
        "meters": s.engine.meters(),
    }))
}

async fn ws_handler(ws: WebSocketUpgrade, State(s): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| ws_loop(socket, s))
}

async fn ws_loop(mut socket: WebSocket, s: AppState) {
    // Meters stream fast (~25 Hz) for a smooth VU; the heavier now-playing +
    // volume snapshot piggybacks every 6th frame (~240 ms).
    let mut tick = tokio::time::interval(Duration::from_millis(40));
    let mut n: u32 = 0;
    loop {
        tick.tick().await;
        n = n.wrapping_add(1);
        let payload = if n % 6 == 0 {
            let now = s.player.as_ref().map(|p| p.snapshot()).unwrap_or_default();
            json!({ "meters": s.engine.meters(), "now": now, "volume": s.volume.state() })
        } else {
            json!({ "meters": s.engine.meters() })
        };
        if socket.send(Message::Text(payload.to_string())).await.is_err() {
            break;
        }
    }
}

/// Serve the embedded Vue SPA, falling back to index.html for client routes.
async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => match Assets::get("index.html") {
            Some(content) => (
                [(header::CONTENT_TYPE, "text/html")],
                content.data.into_owned(),
            )
                .into_response(),
            None => (StatusCode::NOT_FOUND, "UI not built").into_response(),
        },
    }
}

struct AppError(StatusCode, String);
impl AppError {
    fn bad(e: anyhow::Error) -> Self {
        AppError(StatusCode::BAD_REQUEST, e.to_string())
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}
