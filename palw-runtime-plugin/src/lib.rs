//! # PALW Runtime Plugin contract (ADR-MA §16)
//!
//! The provider/auditor-side boundary of the model-agnostic architecture: one plugin implements
//! ONE registered Compute Set. Adding support for a new model is installing a plugin and
//! refreshing the on-chain capability record — never a node change (§16 「Full nodeはpluginを
//! 必要としない」— and structurally cannot be one: no consensus crate depends on this crate,
//! which is the compile-time form of that rule).
//!
//! ## Trust boundary (§16.1)
//!
//! NOTHING a plugin says about itself is consensus input. `implementation_id`, GPU vendor,
//! driver, kernel and performance are telemetry. Validity comes exclusively from
//! `compute_set_id` + receipt exact-match + PCPB + auditor replay + conformance + bond +
//! certificate + TraceVM — all verified OUTSIDE the plugin by parties that do not trust it.
//! A plugin can therefore lie about anything it self-reports and gain nothing but slashing.
//!
//! ## Contract shape
//!
//! * [`PalwRuntimePluginV1::run_conformance`] — replays the descriptor's conformance vectors
//!   (bit-exact expected outputs); a failing implementation must never be offered capability.
//! * [`PalwRuntimePluginV1::execute`] — runs one job to a full `ComputeReceiptV3` whose
//!   projection commits the set, challenge, outputs and execution roots.
//! * [`PalwRuntimePluginV1::replay_checkpoint`] — recomputes one committed trace checkpoint
//!   (the auditor sampling path).
//! * [`PalwRuntimePluginV1::expand_dispute_segment`] — bisection evidence between two
//!   checkpoints when replay disagrees (the dispute program's data source).

use kaspa_hashes::Hash64;
use misaka_palw::palw_determinism::{ConformanceReport, ConformanceVector};
use misaka_palw::receipt_v3::ComputeReceiptV3;
use thiserror::Error;

/// One conformance run request: the vectors come from the descriptor's
/// `conformance_vector_root` payload; the seed binds this run (anti-replay of cached results
/// when a verifier is watching).
#[derive(Clone, Debug)]
pub struct ConformanceChallengeV1 {
    pub compute_set_id: Hash64,
    pub challenge_seed: Hash64,
    pub vectors: Vec<ConformanceVector>,
}

/// The plugin's conformance outcome: the standard report plus the run binding.
#[derive(Clone, Debug)]
pub struct ConformanceResultV1 {
    pub compute_set_id: Hash64,
    pub challenge_seed: Hash64,
    pub report: ConformanceReport,
}

/// One job to execute against a registered Compute Set. Prompt bytes travel out-of-band
/// (content-addressed); the plugin re-derives and checks the commitment.
#[derive(Clone, Debug)]
pub struct PalwJobV1 {
    pub compute_set_id: Hash64,
    pub job_challenge: Hash64,
    pub prompt_commitment: Hash64,
    pub prompt: Vec<u8>,
    pub shape_id: u16,
    pub max_output_tokens: u32,
}

/// Auditor-side checkpoint replay request: recompute the trace state at `checkpoint_index`
/// for the execution committed by `execution_root`.
#[derive(Clone, Debug)]
pub struct CheckpointReplayRequestV1 {
    pub compute_set_id: Hash64,
    pub job_challenge: Hash64,
    pub execution_root: Hash64,
    pub checkpoint_index: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointReplayResultV1 {
    pub checkpoint_index: u64,
    pub checkpoint_root: Hash64,
}

/// Dispute bisection: expand the step commitments between two adjacent checkpoints so the
/// dispute program can locate the first divergent step.
#[derive(Clone, Debug)]
pub struct DisputeSegmentRequestV1 {
    pub compute_set_id: Hash64,
    pub job_challenge: Hash64,
    pub execution_root: Hash64,
    pub segment_start_checkpoint: u64,
    pub segment_end_checkpoint: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisputeSegmentEvidenceV1 {
    pub segment_start_checkpoint: u64,
    pub step_roots: Vec<Hash64>,
}

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("plugin does not implement compute set {0}")]
    UnsupportedComputeSet(Hash64),

    #[error("prompt bytes do not match the committed prompt_commitment")]
    PromptCommitmentMismatch,

    #[error("shape {0} is outside this set's allowed shape table")]
    UnsupportedShape(u16),

    #[error("execution failed: {0}")]
    Execution(String),

    #[error("trace checkpoint {0} is out of range for this execution")]
    CheckpointOutOfRange(u64),

    #[error("model artifact unavailable or failed content verification: {0}")]
    Artifact(String),
}

/// §16 — the versioned plugin contract. Object-safe so hosts can hold `dyn` plugins per set.
pub trait PalwRuntimePluginV1: Send + Sync {
    /// The ONE Compute Set this plugin implements (a provider installs one plugin per set).
    fn compute_set_id(&self) -> Hash64;

    /// Telemetry ONLY (§16.1): never consensus input, never part of receipt validity.
    fn implementation_id(&self) -> Hash64;

    fn run_conformance(&self, challenge: &ConformanceChallengeV1) -> Result<ConformanceResultV1, RuntimeError>;

    fn execute(&self, job: &PalwJobV1) -> Result<ComputeReceiptV3, RuntimeError>;

    fn replay_checkpoint(&self, request: &CheckpointReplayRequestV1) -> Result<CheckpointReplayResultV1, RuntimeError>;

    fn expand_dispute_segment(&self, request: &DisputeSegmentRequestV1) -> Result<DisputeSegmentEvidenceV1, RuntimeError>;
}

/// A host-side plugin rack: providers/auditors register any number of plugins and route by
/// `compute_set_id` (§16 `supported_compute_sets = [A, B, C]`). Unknown sets fail closed.
#[derive(Default)]
pub struct PluginRack {
    plugins: Vec<Box<dyn PalwRuntimePluginV1>>,
}

impl PluginRack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a plugin. Duplicate sets are rejected — two implementations for one set on one
    /// host would make dispatch ambiguous (run two hosts instead).
    pub fn register(&mut self, plugin: Box<dyn PalwRuntimePluginV1>) -> Result<(), RuntimeError> {
        let set = plugin.compute_set_id();
        if self.plugins.iter().any(|existing| existing.compute_set_id() == set) {
            return Err(RuntimeError::Execution(format!("a plugin for compute set {set} is already registered")));
        }
        self.plugins.push(plugin);
        Ok(())
    }

    pub fn supported_compute_sets(&self) -> Vec<Hash64> {
        self.plugins.iter().map(|plugin| plugin.compute_set_id()).collect()
    }

    /// Route by set id — `None` is the fail-closed answer capability records must reflect.
    pub fn plugin_for(&self, compute_set_id: &Hash64) -> Option<&dyn PalwRuntimePluginV1> {
        self.plugins.iter().find(|plugin| &plugin.compute_set_id() == compute_set_id).map(|boxed| boxed.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NullPlugin(Hash64);

    impl PalwRuntimePluginV1 for NullPlugin {
        fn compute_set_id(&self) -> Hash64 {
            self.0
        }
        fn implementation_id(&self) -> Hash64 {
            Hash64::from_bytes([0xee; 64])
        }
        fn run_conformance(&self, _: &ConformanceChallengeV1) -> Result<ConformanceResultV1, RuntimeError> {
            Err(RuntimeError::Execution("null".into()))
        }
        fn execute(&self, job: &PalwJobV1) -> Result<ComputeReceiptV3, RuntimeError> {
            Err(RuntimeError::UnsupportedComputeSet(job.compute_set_id))
        }
        fn replay_checkpoint(&self, request: &CheckpointReplayRequestV1) -> Result<CheckpointReplayResultV1, RuntimeError> {
            Ok(CheckpointReplayResultV1 { checkpoint_index: request.checkpoint_index, checkpoint_root: Hash64::default() })
        }
        fn expand_dispute_segment(&self, request: &DisputeSegmentRequestV1) -> Result<DisputeSegmentEvidenceV1, RuntimeError> {
            Ok(DisputeSegmentEvidenceV1 { segment_start_checkpoint: request.segment_start_checkpoint, step_roots: vec![] })
        }
    }

    #[test]
    fn rack_routes_by_set_and_fails_closed() {
        let a = Hash64::from_bytes([1; 64]);
        let b = Hash64::from_bytes([2; 64]);
        let mut rack = PluginRack::new();
        rack.register(Box::new(NullPlugin(a))).unwrap();
        assert!(rack.register(Box::new(NullPlugin(a))).is_err(), "duplicate set must be rejected");
        rack.register(Box::new(NullPlugin(b))).unwrap();
        assert_eq!(rack.supported_compute_sets(), vec![a, b]);
        assert!(rack.plugin_for(&a).is_some());
        assert!(rack.plugin_for(&Hash64::from_bytes([9; 64])).is_none(), "unknown set fails closed");
    }
}
