use crate::BlockHash; // PR-9.5e: block-identifier positions widened to Hash64
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum SyncManagerError {
    #[error("low hash {0} is not in selected parent chain")]
    BlockNotInSelectedParentChain(BlockHash),

    #[error("low hash {0} is higher than high hash {1}")]
    LowHashHigherThanHighHash(BlockHash, BlockHash),

    #[error("pruning point {0} is not on selected parent chain of {1}")]
    PruningPointNotInChain(BlockHash, BlockHash),

    #[error("block locator low hash {0} is not on selected parent chain of high hash {1}")]
    LocatorLowHashNotInHighHashChain(BlockHash, BlockHash),
}

pub type SyncManagerResult<T> = std::result::Result<T, SyncManagerError>;
