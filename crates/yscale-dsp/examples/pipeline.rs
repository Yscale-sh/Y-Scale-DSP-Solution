//! Minimal example of using `yscale-dsp` as a library: build a 2-channel
//! pipeline with EQ + an active crossover and process an interleaved block.
//!
//! Run with:  `cargo run -p yscale-dsp --example pipeline`

use yscale_dsp::{
    crossover, BiquadChain, ChannelMatrix, ChannelStrip, Coeffs, CrossoverKind, Pipeline,
};

fn main() {
    let fs = 48_000.0;

    // LEFT (woofer): +3 dB peaking at 80 Hz, then a 24 dB/oct Linkwitz-Riley
    // low-pass at 2 kHz, and a 1 ms time-alignment delay.
    let mut left_filters = BiquadChain::new();
    left_filters.push(Coeffs::peaking(fs, 80.0, 1.0, 3.0));
    left_filters.extend(&crossover::lowpass(CrossoverKind::LinkwitzRiley, 4, 2000.0, fs));
    let mut left = ChannelStrip::new(0.02 * fs); // 20 ms delay headroom
    left.set_filters(left_filters);
    left.delay.set_delay_samples(0.001 * fs);
    left.set_gain_db(-1.0);

    // RIGHT (tweeter): complementary 24 dB/oct high-pass at 2 kHz.
    let mut right = ChannelStrip::new(0.02 * fs);
    right.set_filters(crossover::highpass(CrossoverKind::LinkwitzRiley, 4, 2000.0, fs));

    // Wire them into a stereo (2-in/2-out) pipeline.
    let mut pipeline = Pipeline::new(ChannelMatrix::stereo(), vec![left, right]);

    // Process one interleaved stereo block (L, R, L, R, ...).
    let frames = 8;
    let input = vec![0.5_f64; frames * 2];
    let mut output = vec![0.0_f64; frames * 2];
    pipeline.process_interleaved(&input, &mut output, frames);

    println!("{} in  -> {} out channels", pipeline.n_in(), pipeline.n_out());
    println!("first frames (L,R): {:?}", &output[..4]);
}
