use crate::BlockHash; // PR-9.5e: block-identifier positions widened to Hash64
use crate::{BlockHashMap, BlueWorkType, KType, block::Block, header::Header};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Represents semi-trusted externally provided Ghostdag data (by a network peer)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalGhostdagData {
    pub blue_score: u64,
    pub blue_work: BlueWorkType,
    pub selected_parent: BlockHash,
    pub mergeset_blues: Vec<BlockHash>,
    pub mergeset_reds: Vec<BlockHash>,
    pub blues_anticone_sizes: BlockHashMap<KType>,
}

/// Represents an externally provided block with associated Ghostdag data which
/// is only partially validated by the consensus layer. Note there is no actual trust
/// but rather these blocks are indirectly validated through the PoW mined over them
#[derive(Clone)]
pub struct TrustedBlock {
    pub block: Block,
    pub ghostdag: ExternalGhostdagData,
}

impl TrustedBlock {
    pub fn new(block: Block, ghostdag: ExternalGhostdagData) -> Self {
        Self { block, ghostdag }
    }
}

/// Represents an externally provided header with associated Ghostdag data which
/// is only partially validated by the consensus layer. Note there is no actual trust
/// but rather these headers are indirectly validated through the PoW mined over them
pub struct TrustedHeader {
    pub header: Arc<Header>,
    pub ghostdag: ExternalGhostdagData,
}

impl TrustedHeader {
    pub fn new(header: Arc<Header>, ghostdag: ExternalGhostdagData) -> Self {
        Self { header, ghostdag }
    }
}

/// Represents externally provided Ghostdag data associated with a block BlockHash
pub struct TrustedGhostdagData {
    pub hash: BlockHash,
    pub ghostdag: ExternalGhostdagData,
}

impl TrustedGhostdagData {
    pub fn new(hash: BlockHash, ghostdag: ExternalGhostdagData) -> Self {
        Self { hash, ghostdag }
    }
}
