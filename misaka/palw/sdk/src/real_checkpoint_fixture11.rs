//! Off-chain, whole-graph commitments from an actual mapped checkpoint.
//! A fixture grants no chain eligibility, source equivalence or Final. Consumers
//! reconstruct candidate/job/network-domain evidence and authenticate its inputs.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::Hash64;
use misaka_palw_kernel::{hash::Digest, trace::ParamCommitmentsV1};

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct RealCheckpointExecutionFixtureV11 {
    pub version: u16,
    pub artifact_sha256: [u8; 32],
    pub protocol_sha256: [u8; 32],
    pub program_bytes: Vec<u8>,
    pub params: ParamCommitmentsV1,
    pub model_inventory_root: Hash64,
    pub prompt: Vec<u32>,
    pub generated: Vec<u32>,
    /// Every executed position, occurrence and node, without parameter payloads
    /// or private tensors. Generation has the native fixed-count greedy rule.
    pub commitments: Vec<Vec<Vec<Digest>>>,
}
