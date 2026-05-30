use crate::model::stores::statuses::StatusesStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_database::prelude::StoreError;
use parking_lot::RwLock;
use std::sync::Arc;

/// Multi-threaded block-statuses service imp
#[derive(Clone)]
pub struct MTStatusesService<T: StatusesStoreReader> {
    store: Arc<RwLock<T>>,
}

impl<T: StatusesStoreReader> MTStatusesService<T> {
    pub fn new(store: Arc<RwLock<T>>) -> Self {
        Self { store }
    }
}

impl<T: StatusesStoreReader> StatusesStoreReader for MTStatusesService<T> {
    fn get(&self, hash: BlockHash) -> Result<BlockStatus, StoreError> {
        self.store.read().get(hash)
    }

    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.store.read().has(hash)
    }
}
