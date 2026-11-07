use crate::model::stores::relations::RelationsStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHashSet;
use kaspa_database::prelude::{ReadLock, StoreError, StoreResult};
use parking_lot::RwLock;
use std::sync::Arc;

/// Multi-threaded block-relations service imp
#[derive(Clone)]
pub struct MTRelationsService<T: RelationsStoreReader> {
    store: Arc<RwLock<T>>,
}

impl<T: RelationsStoreReader> MTRelationsService<T> {
    pub fn new(store: Arc<RwLock<T>>) -> Self {
        Self { store }
    }
}

impl<T: RelationsStoreReader> RelationsStoreReader for MTRelationsService<T> {
    fn get_parents(&self, hash: BlockHash) -> Result<kaspa_consensus_core::blockhash::BlockHashes, StoreError> {
        self.store.read().get_parents(hash)
    }

    fn get_children(&self, hash: BlockHash) -> StoreResult<ReadLock<BlockHashSet>> {
        self.store.read().get_children(hash)
    }

    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.store.read().has(hash)
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        self.store.read().counts()
    }
}
