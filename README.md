# Y-Scale-DSP-Solution

A slim, high-fidelity, open-source audio **DSP library** — and a complete
**network-streamer + loudspeaker processor** built on it. It runs on small
hardware (developed on a Raspberry Pi Zero 2 W with an Innomaker HiFi DAC,
PCM5122, stereo RCA) and turns it into a WiiM-class streamer with a full
Helix/Trinnov-style tuning suite.

Inspired in function by Helix / Audiotec Fischer and Trinnov: routing, time
alignment, parametric + graphic EQ, active crossovers, bass management, a
brickwall limiter, and FIR room correction — but as a **flexible Rust library
you can build on**, plus a polished web app to drive it.

> **Status:** working end to end. Real-time DSP through the DAC, an mpv-backed
> network streamer with transport + now-playing, a Vue web app, DLNA, presets,
> a live RTA, and FIR room correction. Also integrates as a first-class
> "play on this device" endpoint in [`yscale-media`](https://github.com/Yscale-sh/yscale-media).

## Features

**DSP (`yscale-dsp`)**
- RBJ Audio-EQ-Cookbook **biquads** (Direct-Form-II-Transposed) + cascades.
- **Parametric EQ** (peaking, shelves, pass, notch, all-pass) and a **30-band
  ISO 1/3-octave graphic EQ**.
- **Crossovers**: Butterworth (1–8, up to 48 dB/oct), Linkwitz-Riley (even
  2–8), and **Bessel** (1–4, flat group delay) — **low-, high- and band-pass**.
- **Time alignment**: fractional-sample delay (4-point Lagrange).
- **N×N routing matrix** (mono / L / R / swap / arbitrary custom mix).
- **Bass management**: mono-bass crossover (flat-summing LR) + rumble filter,
  fold-into-mains or route to a dedicated sub channel.
- **FIR convolution**: overlap-save FFT engine for linear-phase filtering /
  room correction (dependency-free radix-2 FFT).
- **Brickwall limiter**: look-ahead, channel-linked safety limiter.
- **`verify` module**: convolution + LTI correctness proofs (see below).

**Streamer / appliance (`yscale-engine` + `yscale-server` + web app)**
- Network player (mpv): play any HTTP(S)/HLS/DASH/`file://` stream **through the
  DSP**, with real transport (play/pause/seek), live position and now-playing
  metadata.
- **DLNA/UPnP** renderer (audio runs through the DSP before the DAC).
- **Master volume** on the DAC's hardware control, with mute.
- **Presets / scenes**: save & instantly recall full tunings.
- **Live RTA** (30-band spectrum of the output) + signal generators.
- A bold dark **Vue web app** (now-playing first; DSP in a "Sound" tab).
- First-class **remote control from `yscale-media`** ("Play on mediapi").

## Using the library

`yscale-dsp` is pure, `f64`, allocation-free on the hot path, and engine-agnostic
— bring your own audio I/O. Add it and compose a `Pipeline`:

```toml
[dependencies]
yscale-dsp = "0.1"
```

```rust
use yscale_dsp::{crossover, BiquadChain, ChannelMatrix, ChannelStrip,
                 Coeffs, CrossoverKind, Pipeline};

let fs = 48_000.0;

// A 2-way active crossover at 2 kHz with a little EQ + time-align on the woofer.
let mut woofer_filters = BiquadChain::new();
woofer_filters.push(Coeffs::peaking(fs, 80.0, 1.0, 3.0));
woofer_filters.extend(&crossover::lowpass(CrossoverKind::LinkwitzRiley, 4, 2000.0, fs));
let mut woofer = ChannelStrip::new(0.02 * fs);
woofer.set_filters(woofer_filters);
woofer.delay.set_delay_samples(0.001 * fs); // 1 ms

let mut tweeter = ChannelStrip::new(0.02 * fs);
tweeter.set_filters(crossover::highpass(CrossoverKind::LinkwitzRiley, 4, 2000.0, fs));

let mut pipeline = Pipeline::new(ChannelMatrix::stereo(), vec![woofer, tweeter]);
pipeline.process_interleaved(&input, &mut output, frames); // f64 interleaved
```

Runnable: `cargo run -p yscale-dsp --example pipeline`. Standalone blocks
(`Biquad`, `crossover`, `FirConv`, `BassManager`, `Limiter`, `Delay`,
`ChannelMatrix`, `ParametricEq`, `GraphicEq30`) can also be used on their own.

```bash
cargo test -p yscale-dsp     # math, crossovers, FIR vs direct conv, LTI proofs
```

## Architecture

A Cargo workspace, all Rust (no GC pauses, memory-safe, tiny binary):

| Crate | Role |
|-------|------|
| **`yscale-dsp`** | The open-source DSP core (above). No audio deps. |
| **`yscale-engine`** | Real-time loop: ALSA playback, signal generators + WAV/Capture sources, `f64`→PCM with TPDF dither, live-swappable pipeline / bass / FIR / limiter, RTA, and per-output meters + gain-reduction. |
| **`yscale-cli`** | `yscale` — the headless runner. |
| **`yscale-server`** | `axum` REST + WebSocket server: the mpv streamer brain, master volume, presets, FIR storage, and the embedded Vue web app. |

Signal flow:

```
source ─▶ ChannelMatrix ─▶ per-output ChannelStrip ─▶ bass mgmt ─▶ FIR ─▶ limiter ─▶ DAC
 (gen/file/        (routing/mix)   delay → EQ + crossover            (mono     (room       (safety
  URL/DLNA)                        → gain/polarity/mute               bass)    correction)  brickwall)
```

URL/DLNA sources are decoded by mpv into an ALSA `snd-aloop` loopback that the
engine captures — so streamed audio gets the full DSP, with the DAC as the
single clock master.

## Hardware setup (Pi Zero 2 W + Innomaker HiFi DAC / PCM5122)

The board is an I2S-**master** DAC (it owns the clock via its own oscillators),
exposed by the BossDAC overlay. In `/boot/firmware/config.txt`:

```ini
dtparam=i2c_arm=on
dtoverlay=allo-boss-dac-pcm512x-audio   # PCM512x master-clock board -> card "BossDAC"
```

> **Kernel note.** On current Raspberry Pi OS (kernel **6.12**) the master-clock
> path regresses — playback throws I/O errors / `I2S SYNC error`
> ([raspberrypi/linux#5843](https://github.com/raspberrypi/linux/issues/5843)).
> Pin a **6.6.x** kernel until it's fixed (e.g. install the 4 K `+rpt-rpi-v8`
> 6.6.62 kernel/initramfs/dtbs into `/boot/firmware/66b/` and add
> `os_prefix=66b/`). The Zero 2 W needs the **4 K**-page kernel to network
> headless (not the 16 K `-v8-16k`).

For glitch-free playback also pin the CPU governor (persist with a small systemd
oneshot):

```bash
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

The DAC does S16/S24/S32, stereo; we default to `S32_LE` @ 48 kHz.

## Build & deploy

The Pi has too little RAM to compile Rust, so build a **native aarch64 binary in
Docker** (great on an Apple-Silicon / arm64 Docker host) and ship the binary.

```bash
docker build -f build/Dockerfile -t yscale-builder .   # one-time builder image

./deploy/deploy.sh      jake@mediapi.local   # CLI engine + configs
./deploy/deploy-web.sh  jake@mediapi.local   # web app + server (systemd service)
./deploy/setup-dlna.sh  jake@mediapi.local   # snd-aloop + gmediarender + mpv
```

Then open **`http://mediapi.local:8080`** from any device on the LAN. The built
UI (`web/dist`) is embedded into the server binary via `rust-embed`, so the
server is a single file.

## CLI usage

```bash
yscale --config /etc/yscale/two-way-speaker.toml sweep --f1 20 --f2 20000 --dur 10 --loop
yscale --config /etc/yscale/passthrough.toml sine --freq 1000 --amp 0.1
yscale pink --amp 0.2     # break-in / level matching
yscale impulse            # time-of-arrival
yscale file track.wav --loop
```

Sources: `sine`, `sweep`, `pink`, `white`, `impulse`, `file`. Flags:
`--config <toml>`, `--device <alsa>`, `--rate <hz>`.

## Configuration

A TOML config declares the device + DSP graph; see `configs/`
(`passthrough.toml`, `two-way-speaker.toml`, `graphic-eq.toml`). Top level:
`routing` (presets `stereo`/`mono`/`left_to_both`/`right_to_both`/`swap`/`custom`
matrix), `[limiter]`, `[bass]`, and one `[[channel]]` per output with `gain_db`,
`delay_ms`/`delay_cm`, `invert`, `mute`, parametric `eq`, `graphic_eq`,
`crossover` (kind `butterworth`/`linkwitz_riley`/`bessel`, role
`low_pass`/`high_pass`/`band_pass` with `freq`/`freq_high`/`order`), and `fir`
(a stored FIR by name).

## HTTP API

```
GET/PUT /api/config            current DSP graph (PUT hot-swaps it live)
POST    /api/source            test source (sine/sweep/pink/white/impulse/file/capture)
POST    /api/play              play a URL through the DSP (+ now-playing metadata)
POST    /api/pause /stop /seek transport
GET     /api/now               now-playing + volume + meters + gain-reduction + spectrum
GET/PUT /api/volume            master volume / mute (DAC hardware control)
GET     /api/presets           list; POST /api/presets/save|load|delete
GET     /api/firs              list; POST /api/firs/upload?name= (WAV/text); /delete
GET     /api/status            rate / channels / meters / gain-reduction
GET     /ws                    live meters + now-playing + volume + spectrum
```

## Correctness: convolution + LTI verification

High fidelity demands it, so the DSP core is verified, not hoped at. The blocks
are **IIR** (DF-II-T biquads/cascades), so verification is against convolution —
an LTI system's defining operation — *within numerical tolerance*:

1. capture the block's impulse response (IR);
2. assert direct processing of an arbitrary signal **matches** convolving it with
   the IR within `f64` precision (compared only over indices the IR fully covers,
   so IIR-tail truncation can't skew it);
3. assert the **DTFT of the IR matches the analytic transfer function**;
4. assert the LTI properties directly — **homogeneity** (`process(k·x) ==
   k·process(x)`) and **time-invariance** — to catch hidden nonlinearities.

`yscale_dsp::verify` exposes these (`lti_residual`, `dtft_magnitude`,
`homogeneity_residual`, `time_invariance_residual`) for your own configs. The FIR
engine is additionally checked against a direct convolution reference, and the
crossovers against their −3/−6 dB points and slopes.

## Roadmap

- **Streamer UI** — make the web app itself a UPnP control point (browse + pick
  tracks in-app).
- **Multi-Pi** — slave several Pi Zero 2 W as N word-clocked DACs in one
  enclosure (enables a true dedicated subwoofer output).
- **Measurement** — in-app sweep capture + auto room-correction FIR generation.

## License

MIT © 2026 Jake Nesler
