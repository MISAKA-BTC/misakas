//! Off-chain reproduction material for an actual checkpoint's isolated flat9
//! route. This is neither a consensus object nor an execution, fidelity or
//! source-attestation certificate. A consumer must authenticate every field
//! against independently obtained chain, job and artifact facts.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{Hash64, palw_onboarding_v1::ArtifactMismatchProofV1};
use misaka_palw_kernel::{descriptor::KernelDescriptorV1, hash::Digest, trace::ParamCommitmentsV1};

/// Exact public replies observed during a generic ordinary relation search.
/// `kind`: 0 node, 1 flat hash, 2 typed header, 3 typed value. `index` is the
/// requested leaf for hash, requested element for value, and zero otherwise.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct FlatFixtureReplyV9 {
    pub kind: u8,
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub index: u64,
    pub bytes: Vec<u8>,
}

/// Complete public commitment directories and bounded prosecution openings,
/// without private position values or whole model parameters. The honest
/// binding tile establishes only that particular R/PC tile agrees; the false
/// tile is an independently adjudicable negative control for a different PC.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct FlatCheckpointFixtureV9 {
    pub version: u16,
    pub descriptor: KernelDescriptorV1,
    pub program_bytes: Vec<u8>,
    pub params: ParamCommitmentsV1,
    pub model_inventory_root: Hash64,
    pub honest_binding: ArtifactMismatchProofV1,
    pub false_binding: ArtifactMismatchProofV1,
    pub tokens: Vec<u32>,
    pub honest_generated: Vec<u32>,
    pub honest_commitments: Vec<Vec<Vec<Digest>>>,
    pub wrong_commitments: Vec<Vec<Vec<Digest>>>,
    pub ordinary_fault_bytes: Vec<u8>,
    pub requested_public_replies: Vec<FlatFixtureReplyV9>,
}
