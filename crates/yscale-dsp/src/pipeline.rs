//! The full multi-channel processing graph:
//!
//! ```text
//!   inputs ──▶ ChannelMatrix (routing/mix) ──▶ per-output ChannelStrip ──▶ outputs
//!                                               (delay → EQ+xover → gain)
//! ```
//!
//! Built once from a configuration, then driven sample-block by sample-block.
//! The number of output channels can exceed inputs (e.g. one source fanned out
//! to a low-passed woofer channel and a high-passed tweeter channel).

use crate::matrix::ChannelMatrix;
use crate::strip::ChannelStrip;
use crate::MonoProcessor;

/// A complete, ready-to-run DSP graph.
#[derive(Debug, Clone)]
pub struct Pipeline {
    matrix: ChannelMatrix,
    strips: Vec<ChannelStrip>,
    n_in: usize,
    n_out: usize,
    in_frame: Vec<f64>,
    out_frame: Vec<f64>,
}

impl Pipeline {
    /// Assemble a pipeline from a routing matrix and one strip per output
    /// channel. Panics if `strips.len() != matrix.n_out()`.
    pub fn new(matrix: ChannelMatrix, strips: Vec<ChannelStrip>) -> Self {
        assert_eq!(
            strips.len(),
            matrix.n_out(),
            "need exactly one ChannelStrip per output channel"
        );
        let n_in = matrix.n_in();
        let n_out = matrix.n_out();
        Self {
            matrix,
            strips,
            n_in,
            n_out,
            in_frame: vec![0.0; n_in],
            out_frame: vec![0.0; n_out],
        }
    }

    pub fn n_in(&self) -> usize {
        self.n_in
    }

    pub fn n_out(&self) -> usize {
        self.n_out
    }

    /// Mutable access to an output channel's strip (for live control).
    pub fn strip_mut(&mut self, index: usize) -> Option<&mut ChannelStrip> {
        self.strips.get_mut(index)
    }

    /// Access the routing matrix.
    pub fn matrix(&self) -> &ChannelMatrix {
        &self.matrix
    }

    /// Replace the routing matrix (its size must match the existing graph).
    pub fn set_matrix(&mut self, matrix: ChannelMatrix) {
        assert_eq!(matrix.n_in(), self.n_in);
        assert_eq!(matrix.n_out(), self.n_out);
        self.matrix = matrix;
    }

    /// Process interleaved audio: `input` holds `frames × n_in` samples and
    /// `output` is filled with `frames × n_out` samples.
    pub fn process_interleaved(&mut self, input: &[f64], output: &mut [f64], frames: usize) {
        debug_assert_eq!(input.len(), frames * self.n_in);
        debug_assert_eq!(output.len(), frames * self.n_out);
        for f in 0..frames {
            let in_base = f * self.n_in;
            self.in_frame
                .copy_from_slice(&input[in_base..in_base + self.n_in]);
            self.matrix.process_frame(&self.in_frame, &mut self.out_frame);
            let out_base = f * self.n_out;
            for (o, strip) in self.strips.iter_mut().enumerate() {
                output[out_base + o] = strip.process_sample(self.out_frame[o]);
            }
        }
    }

    /// Clear all delay lines and filter memory.
    pub fn reset(&mut self) {
        for s in &mut self.strips {
            s.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crossover::{self, CrossoverKind};

    const FS: f64 = 48_000.0;

    #[test]
    fn stereo_passthrough() {
        let strips = vec![ChannelStrip::new(8.0), ChannelStrip::new(8.0)];
        let mut p = Pipeline::new(ChannelMatrix::stereo(), strips);
        // Two frames of interleaved stereo.
        let input = [1.0, -1.0, 0.0, 0.0];
        let mut output = [0.0; 4];
        p.process_interleaved(&input, &mut output, 2);
        // 1-sample strip latency pushes frame 0 into frame 1.
        assert!((output[2] - 1.0).abs() < 1e-9);
        assert!((output[3] + 1.0).abs() < 1e-9);
    }

    #[test]
    fn two_way_active_crossover_topology() {
        // Mono source -> out0 low-passed (woofer), out1 high-passed (tweeter).
        let mut matrix = ChannelMatrix::new(2, 2);
        matrix.set(0, 0, 0.5);
        matrix.set(0, 1, 0.5);
        matrix.set(1, 0, 0.5);
        matrix.set(1, 1, 0.5);

        let mut woofer = ChannelStrip::new(8.0);
        woofer.set_filters(crossover::lowpass(CrossoverKind::LinkwitzRiley, 4, 2000.0, FS));
        let mut tweeter = ChannelStrip::new(8.0);
        tweeter.set_filters(crossover::highpass(CrossoverKind::LinkwitzRiley, 4, 2000.0, FS));

        let mut p = Pipeline::new(matrix, vec![woofer, tweeter]);

        // Drive with a long low-frequency-ish impulse train and confirm the two
        // outputs differ (band-split actually happened).
        let mut out = [0.0; 2];
        let mut diff_energy = 0.0;
        for n in 0..2048 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            p.process_interleaved(&[x, x], &mut out, 1);
            diff_energy += (out[0] - out[1]).powi(2);
        }
        assert!(diff_energy > 1e-6, "woofer and tweeter outputs should differ");
    }
}
