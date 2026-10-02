//! **Executor conformance by sampling, streamed** (RFC-0002 Part II §II.9 L2 and §II.7.5 Proposal B): the node's own typed
//! executor, over the artifact it holds, run against the reference evaluator on a FEW positions — and the two must agree at every
//! commit point and at the logits, byte for byte.
//!
//! The conformance of `palw-class pack verify` loads the artifact whole once per executor (the reference's tensors are `i128`).
//! That is the limitation L2 names. This is the other mode:
//!
//! * **streamed**: the reference evaluator reads each param through a lazy [`ParamSource`] over the artifact's container — one
//!   tensor decoded per ask and dropped after it — so its resident bytes are the largest single tensor, not the artifact; the
//!   typed executor is the node's own ([`TirArtifactV1`]: mapped in place, or held within a residency budget);
//! * **sampled**: a handful of positions of a prompt the class itself fixes (`positions`, default [`TIR_CONFORMANCE_POSITIONS_V1`]).
//!   Every block the schedule runs is executed at position 0, and the history, the carries and the checkpointed state at the
//!   later ones, so every primitive kind the program names is exercised.
//!
//! A seat runs it once per `(class, artifact root)` before it first proves readiness (Proposal B): a seat whose executor does not
//! reproduce the reference posts no proof and says why (`EXECUTOR_NOT_CONFORMANT`). A dishonest seat can skip the test; it harms
//! only itself, because the court convicts it on its first wrong tile. Nothing here is consensus, and no object or fingerprint
//! reads it.

use std::path::Path;

use misaka_palw_tir::{Interpreter, ParamSource, RunState, Tensor};
use misaka_palw_tir_artifact::PalwTirContainerV1;

use super::artifact::TirArtifactV1;
use crate::exec::{NodeValue, StepSink, TirExecutor};

/// The positions a seat's self-test runs unless told otherwise: the first, a second that reads one row of history, and a third.
pub const TIR_CONFORMANCE_POSITIONS_V1: usize = 3;

/// What a passing run measured (one line for the operator).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirConformanceV1 {
    pub positions: usize,
    /// Commit points compared, over all positions.
    pub commits: u64,
    /// Tensors the reference read (each decoded once per ask and dropped).
    pub reference_param_reads: u64,
    /// The largest tensor the reference held at once, in bytes of its `i128` form.
    pub reference_peak_tensor_bytes: u64,
}

impl TirConformanceV1 {
    pub fn summary(&self) -> String {
        format!(
            "{} positions, {} commit points identical (reference read {} tensors, at most {:.1} MiB at once)",
            self.positions,
            self.commits,
            self.reference_param_reads,
            self.reference_peak_tensor_bytes as f64 / (1 << 20) as f64
        )
    }
}

/// A [`ParamSource`] over an artifact's container that decodes each tensor when asked and keeps nothing: streamed.
pub struct LazyContainerParams<'c> {
    container: &'c PalwTirContainerV1,
    reads: std::cell::Cell<u64>,
    peak: std::cell::Cell<u64>,
}

impl<'c> LazyContainerParams<'c> {
    pub fn new(container: &'c PalwTirContainerV1) -> Self {
        Self { container, reads: std::cell::Cell::new(0), peak: std::cell::Cell::new(0) }
    }

    pub fn reads(&self) -> u64 {
        self.reads.get()
    }

    pub fn peak_tensor_bytes(&self) -> u64 {
        self.peak.get()
    }
}

impl ParamSource for LazyContainerParams<'_> {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        let d = self.container.program.params.get(index as usize)?;
        let bytes = self.container.read_tensor_bytes(index, layer).ok()?;
        let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
        let t = Tensor::from_le_bytes(d.dtype, &shape, &bytes).ok()?;
        self.reads.set(self.reads.get() + 1);
        self.peak.set(self.peak.get().max((t.data.len() * std::mem::size_of::<i128>()) as u64));
        Some(t)
    }
}

type Commit = (u32, u8, Option<u16>, u16, Vec<i128>);

struct Collect(Vec<Commit>);

impl StepSink for Collect {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.0.push((v.slot, v.block, v.layer, v.node, v.data.to_i128s()));
        }
    }
}

/// **The prompt a class's self-test runs**: `positions` tokens, a function of the class's own program (its `token_bound`) and the
/// position alone — the same on every seat, so every seat is tested on the same bytes.
pub fn tir_conformance_prompt_v1(token_bound: u32, positions: usize) -> Vec<u32> {
    let bound = u64::from(token_bound.max(1));
    (0..positions as u64).map(|i| ((i * 7_919 + 13) % bound) as u32).collect()
}

/// **Run the executor of `artifact` against the reference evaluator** on [`TIR_CONFORMANCE_POSITIONS_V1`] positions (or `positions`):
/// every position's logits and every commit point must be the same bytes. `Err` names the position and what differs.
///
/// Refuses a composite candidate (its params `0..p` are its parent's: the parent's own self-test is the one that applies).
pub fn tir_executor_conformance_v1(artifact: &TirArtifactV1, positions: usize) -> Result<TirConformanceV1, String> {
    let lazy = LazyContainerParams::new(artifact.container());
    let (positions, commits) = tir_executor_conformance_against_v1(artifact, &lazy, positions)?;
    Ok(TirConformanceV1 {
        positions,
        commits,
        reference_param_reads: lazy.reads(),
        reference_peak_tensor_bytes: lazy.peak_tensor_bytes(),
    })
}

/// [`tir_executor_conformance_v1`] with the reference evaluator reading its params from `reference` (the artifact's container, lazily,
/// in the node; a deliberately different source in the tests that hold the gate to fire). Returns `(positions, commit points)`.
pub fn tir_executor_conformance_against_v1(
    artifact: &TirArtifactV1,
    reference: &dyn ParamSource,
    positions: usize,
) -> Result<(usize, u64), String> {
    if artifact.composite_ref().is_some() {
        return Err("a composite candidate is tested as its parent is (its params are the parent's)".into());
    }
    let program = &artifact.container().program;
    let prompt = tir_conformance_prompt_v1(program.token_bound, positions.clamp(1, program.history_bound.max(1) as usize));
    let interp = Interpreter::new(program).map_err(|e| e.to_string())?;
    let mut exec = TirExecutor::new(artifact.plan(), artifact.params()).map_err(|e| e.to_string())?;
    let mut state = RunState::default();
    let mut commits = 0u64;
    for (pos, token) in prompt.iter().copied().enumerate() {
        let out = interp.step(reference, &mut state, token).map_err(|e| format!("reference evaluator at position {pos}: {e}"))?;
        let expected: Vec<Commit> = out.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())).collect();
        let mut sink = Collect(Vec::new());
        exec.step(token, &mut sink).map_err(|e| format!("the executor at position {pos}: {e}"))?;
        let mut got = sink.0;
        got.sort_by_key(|c| c.0);
        if out.logits.data != exec.logits().1.to_i128s() {
            return Err(format!("position {pos}: the executor's logits are not the reference evaluator's"));
        }
        if expected != got {
            let first = expected.iter().zip(got.iter()).position(|(a, b)| a != b);
            return Err(match first {
                Some(i) => format!(
                    "position {pos}: the executor's commit point {i} (slot {}, block {}, node {}) is not the reference evaluator's",
                    expected[i].0, expected[i].1, expected[i].3
                ),
                None => format!("position {pos}: the executor commits {} points where the reference evaluator commits {}", got.len(), expected.len()),
            });
        }
        commits += expected.len() as u64;
    }
    Ok((prompt.len(), commits))
}

/// [`tir_executor_conformance_v1`] over the artifact file at `path`, opened as a node opens it.
pub fn tir_executor_conformance_of_file_v1(path: &Path, positions: usize) -> Result<TirConformanceV1, String> {
    let artifact = TirArtifactV1::open(path)?;
    tir_executor_conformance_v1(&artifact, positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_is_a_function_of_the_class_alone_and_stays_in_the_vocabulary() {
        let a = tir_conformance_prompt_v1(64, 3);
        assert_eq!(a, tir_conformance_prompt_v1(64, 3));
        assert!(a.iter().all(|t| *t < 64));
        assert_eq!(tir_conformance_prompt_v1(1, 3), vec![0, 0, 0], "a vocabulary of one token");
        assert_eq!(tir_conformance_prompt_v1(0, 2), vec![0, 0], "no vocabulary is read as one");
    }
}
