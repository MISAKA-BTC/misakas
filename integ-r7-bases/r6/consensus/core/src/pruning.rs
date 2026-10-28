use crate::{
    BlockHash, BlueWorkType,
    header::Header,
    trusted::{TrustedGhostdagData, TrustedHeader},
};
use std::sync::Arc;

pub type PruningPointProof = Vec<Vec<Arc<Header>>>;

pub type PruningPointsList = Vec<Arc<Header>>;

/// Node-local pruning-import authority. The epoch is durable and never reused,
/// including when the next import has the same pruning point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PruningImportSessionV1 {
    pub point: BlockHash,
    pub epoch: u64,
}

/// Recovery state, committed together with the incomplete/stable marker.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PruningImportSessionStateV1 {
    pub epoch: u64,
    pub active: Option<PruningImportSessionV1>,
    pub utxos_started: bool,
    pub utxos_imported: bool,
}

pub struct PruningPointTrustedData {
    /// The pruning point anticone from virtual PoV
    pub anticone: Vec<BlockHash>,

    /// Union of DAA window data required to verify blocks in the future of the pruning point
    pub daa_window_blocks: Vec<TrustedHeader>,

    /// Union of GHOSTDAG data required to verify blocks in the future of the pruning point
    pub ghostdag_blocks: Vec<TrustedGhostdagData>,
}

#[derive(Clone, Copy)]
pub struct PruningProofMetadata {
    /// The claimed work of the initial relay block (from the prover)
    pub relay_block_blue_work: BlueWorkType,
}

impl PruningProofMetadata {
    pub fn new(relay_block_blue_work: BlueWorkType) -> Self {
        Self { relay_block_blue_work }
    }
}
