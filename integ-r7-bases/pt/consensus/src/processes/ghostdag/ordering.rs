use kaspa_consensus_core::BlockHash;

use crate::model::{
    services::reachability::ReachabilityService,
    stores::{ghostdag::GhostdagStoreReader, headers::HeaderStoreReader, relations::RelationsStoreReader},
};

use super::protocol::GhostdagManager;

// The definition moved to `kaspa_consensus_core::sortable_block` so that chain selection outside
// the consensus crate — deciding whether a verified IBD candidate beats the chain already held —
// uses this exact order rather than a second implementation of it. Re-exported here because this is
// where GHOSTDAG readers expect to find it.
pub use kaspa_consensus_core::sortable_block::SortableBlock;

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService, V: HeaderStoreReader> GhostdagManager<T, S, U, V> {
    pub fn sort_blocks(&self, blocks: impl IntoIterator<Item = BlockHash>) -> Vec<BlockHash> {
        let mut sorted_blocks: Vec<BlockHash> = blocks.into_iter().collect();
        sorted_blocks
            .sort_by_cached_key(|block| SortableBlock { hash: *block, blue_work: self.ghostdag_store.get_blue_work(*block).unwrap() });
        sorted_blocks
    }
}
