//! `yscale-server` — axum web server for live DSP control over the LAN.
//!
//! Serves the embedded Vue UI and exposes:
//!   GET  /api/config   current DSP config (JSON)
//!   PUT  /api/config   replace the DSP graph live (rebuilds + hot-swaps)
//!   POST /api/source   set the audio source live (signal gen or WAV)
//!   GET  /api/status   sample rate / channels / live meters
//!   GET  /ws           WebSocket stream of output level meters (~25 Hz)

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
use rust_embed::RustEmbed;
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
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
    /// The current URL-stream player process (gst-play feeding the loopback).
    player: Arc<Mutex<Option<Child>>>,
}

/// Kill the running URL player, if any.
fn stop_player(s: &AppState) {
    if let Some(mut child) = s.player.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
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

    let state = AppState {
        engine,
        config: Arc::new(Mutex::new(config)),
        player: Arc::new(Mutex::new(None)),
    };

    let app = Router::new()
        .route("/api/config", get(get_config).put(put_config))
        .route("/api/source", post(post_source))
        .route("/api/play", post(post_play))
        .route("/api/status", get(get_status))
        .route("/ws", get(ws_handler))
        .fallback(static_handler)
        .with_state(state);

    let addr: SocketAddr = cli.bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!(
        "[yscale-server] listening on http://{addr}  — open it from any device on the LAN"
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_config(State(s): State<AppState>) -> Json<Config> {
    Json(s.config.lock().unwrap().clone())
}

async fn put_config(
    State(s): State<AppState>,
    Json(mut new): Json<Config>,
) -> Result<Json<serde_json::Value>, AppError> {
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
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn post_source(
    State(s): State<AppState>,
    Json(spec): Json<SourceSpec>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Any explicit source selection ends URL streaming.
    stop_player(&s);
    swap_source_releasing(&s, spec).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct PlayReq {
    url: String,
}

/// Play any HTTP(S)/HLS/DASH/file stream URL THROUGH the DSP: a `gst-play`
/// process decodes it into the ALSA loopback and the engine captures that as
/// its source. This is what lets the Pi natively play yscale-media streams (and
/// web radio) on the hardware DSP.
async fn post_play(
    State(s): State<AppState>,
    Json(req): Json<PlayReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let url = req.url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")) {
        return Err(AppError::bad(anyhow::anyhow!(
            "url must start with http://, https:// or file://"
        )));
    }
    stop_player(&s);
    // gst-launch with an explicit decode→convert→resample→alsasink pipeline
    // (uridecodebin handles http/https/hls/dash/file + most codecs; its dynamic
    // pads are auto-linked by the parser). Feeds the loopback the engine captures.
    let child = Command::new("gst-launch-1.0")
        .arg("uridecodebin")
        .arg(format!("uri={url}"))
        .arg("!")
        .arg("audioconvert")
        .arg("!")
        .arg("audioresample")
        .arg("!")
        .arg("alsasink")
        .arg(format!("device={LOOPBACK_PLAYBACK}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            AppError::bad(anyhow::anyhow!(
                "could not start player (is gstreamer1.0-tools installed?): {e}"
            ))
        })?;
    *s.player.lock().unwrap() = Some(child);

    // Route the loopback through the DSP (releasing any prior capture first).
    swap_source_releasing(
        &s,
        SourceSpec::Capture {
            device: LOOPBACK_CAPTURE.to_string(),
        },
    )
    .await?;

    Ok(Json(serde_json::json!({ "ok": true, "playing": url })))
}

async fn get_status(State(s): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
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
    let mut tick = tokio::time::interval(Duration::from_millis(40));
    loop {
        tick.tick().await;
        let payload = serde_json::json!({ "meters": s.engine.meters() });
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
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}
