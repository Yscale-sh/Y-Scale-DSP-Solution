# Y-Scale-DSP-Solution

A slim, high-fidelity, open-source audio DSP stack built for loudspeaker
development and testing on small hardware — starting on a Raspberry Pi Zero 2 W
with an Innomaker DAC Mini (PCM512x, stereo RCA out, up to 384 kHz / 32-bit).

Inspired in function by tools like Helix / Audiotec Fischer: routing, time
alignment, parametric + graphic EQ, and active crossovers — but as a flexible
library you can build on.

> **Status:** headless engine, milestone 1. Real-time DSP runs from the CLI
> through the DAC. A Vue web UI and DLNA playback are on the roadmap below.

## Why

When you're bench-building speakers you want to, on the fly: force mono / left /
right, time-align drivers, sweep and measure, dial in a 30-band EQ, and run an
active crossover — all at high fidelity. This does that, and the core is a clean
library (`yscale-dsp`) meant to be reused and extended.

## Architecture

A Cargo workspace, all Rust (no GC pauses, memory-safe, tiny static-ish binary):

| Crate | Role |
|-------|------|
| **`yscale-dsp`** | The open-source DSP core. `f64`, allocation-free hot path, composable. Biquads (RBJ cookbook), parametric + 30-band ISO graphic EQ, Butterworth/Linkwitz-Riley crossovers, fractional-sample delay (time alignment), N×N channel routing, and a `Pipeline` graph. Plus a `verify` module of **convolution-based proofs** (see below). |
| **`yscale-engine`** | Real-time loop: ALSA playback, signal generators (sine, log sweep, pink/white noise, impulse) + WAV playback, `f64`→PCM conversion with TPDF dither, and a TOML-driven graph builder. |
| **`yscale-cli`** | `yscale` — the headless runner. |

Signal flow:

```
source ─▶ ChannelMatrix (routing/mix) ─▶ per-output ChannelStrip ─▶ DAC (ALSA)
                                          delay → EQ+crossover → gain/polarity/mute
```

## Hardware setup (Pi Zero 2 W + Innomaker DAC Mini)

In `/boot/firmware/config.txt`:

```ini
dtparam=i2c_arm=on
dtparam=i2s=on
dtoverlay=hifiberry-dacplus,slave   # PCM512x; the ",slave" is REQUIRED here
```

> **The `,slave` matters.** The DAC Mini does not master the I2S clock. Without
> `,slave` the card still enumerates, but every real playback throws
> `Input/output error` and the kernel logs `bcm2835-i2s: I2S SYNC error!`.
> `,slave` makes the **Pi** generate BCLK/LRCLK and the DAC follow — clean audio.

For glitch-free playback also pin the CPU clock (resets on reboot; make it
persistent with a small systemd oneshot if you want it permanent):

```bash
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

The DAC supports S16/S24/S32 at 8 kHz–384 kHz, stereo. We default to S32_LE.

## Build & deploy

The Pi has too little RAM to compile Rust (rustup OOMs unpacking). Instead we
build a **native aarch64 binary in Docker** (works great on an Apple-Silicon /
arm64 Docker host) and ship just the binary.

```bash
# one-time: build the builder image (Debian + libasound2-dev)
docker build -f build/Dockerfile -t yscale-builder .

# build + deploy to the Pi
./deploy/deploy.sh jake@treepi.local
```

Develop locally: the pure DSP core builds and tests natively on any platform.

```bash
cargo test -p yscale-dsp      # math + convolution proofs (no audio hardware)
```

## Usage

```bash
# Stereo pass-through, 1 kHz tone at a safe level (connect a load first!)
yscale --config /etc/yscale/passthrough.toml sine --freq 1000 --amp 0.1

# 10 s log sweep, looped — eyeball/measure frequency response
yscale --config /etc/yscale/two-way-speaker.toml sweep --f1 20 --f2 20000 --dur 10 --loop

# Pink noise for break-in / level matching
yscale pink --amp 0.2

# Single impulse for time-of-arrival, or play a WAV
yscale impulse
yscale file track.wav --loop
```

Sources: `sine`, `sweep`, `pink`, `white`, `impulse`, `file`. Global flags:
`--config <toml>`, `--device <alsa>`, `--rate <hz>`.

## Configuration

A config declares the device and the DSP graph. See `configs/` for full
examples — `passthrough.toml`, `two-way-speaker.toml` (active LR4 crossover),
and `graphic-eq.toml` (30-band). Routing presets: `stereo`, `mono`,
`left_to_both`, `right_to_both`, `swap`, `custom` (arbitrary `[out][in]` matrix).
Per output channel: `gain_db`, `delay_ms`/`delay_cm`, `invert`, `mute`,
parametric `eq` bands, a 30-band `graphic_eq`, and a `crossover`.

## Correctness: convolution proofs

Because high fidelity demands it, the DSP core is verified, not just hoped at.
Every linear time-invariant block is checked against convolution — its defining
operation:

1. capture the block's impulse response,
2. assert direct processing of an arbitrary signal **exactly equals**
   convolving that signal with the IR (proves it's truly LTI and self-consistent),
3. assert the **DTFT of the IR matches the analytic transfer function** derived
   from the coefficients (time-domain ↔ frequency-domain agreement).

The `yscale_dsp::verify` module exposes these primitives so you can convolution-
test your own configs. (Convolution only characterizes LTI systems; generators,
dither, and mute toggling are validated by other means.)

## Roadmap

- **Web UI** — Vue 3 + Tailwind, served by a Rust (`axum`) server with a
  WebSocket for live control, broadcasting on the LAN.
- **DLNA** — UPnP renderer so you can stream a source file from any device.
- **Multi-Pi** — slave several Pi Zero 2 W as N synchronized DACs/receivers in
  one enclosure, word-clocked together.
- DSP: more crossover alignments (Bessel), FIR/linear-phase option, RTA, and
  measurement export.

## License

MIT © 2026 Jake Nesler
