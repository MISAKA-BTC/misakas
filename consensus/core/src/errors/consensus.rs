use crate::BlockHash; // PR-9.5e: block-identifier positions widened to Hash64
use thiserror::Error;

use crate::{tx::TransactionIndexType, utxo::utxo_inquirer::UtxoInquirerError};

use super::{difficulty::DifficultyError, sync::SyncManagerError, traversal::TraversalError};

#[derive(Error, Debug, Clone)]
pub enum ConsensusError {
    #[error("cannot find full block {0}")]
    BlockNotFound(BlockHash),

    #[error("cannot find header {0}")]
    HeaderNotFound(BlockHash),

    #[error("trying to query {0} txs in block {1}, but the block only holds {2} txs")]
    TransactionQueryTooLarge(usize, BlockHash, usize),

    #[error("index {0} out of max {1} in block {2} is out of bounds")]
    TransactionIndexOutOfBounds(TransactionIndexType, usize, BlockHash),

    #[error("block {0} is invalid")]
    InvalidBlock(BlockHash),

    #[error("some data is missing for block {0}")]
    MissingData(BlockHash),

    #[error("got unexpected pruning point")]
    UnexpectedPruningPoint,

    #[error("pruning point is not at sufficient depth from virtual, cannot obtain its final anticone at this stage")]
    PruningPointInsufficientDepth,

    #[error("sync manager error: {0}")]
    SyncManagerError(#[from] SyncManagerError),

    #[error("traversal error: {0}")]
    TraversalError(#[from] TraversalError),

    #[error("difficulty error: {0}")]
    DifficultyError(#[from] DifficultyError),

    #[error("{0}")]
    General(&'static str),

    #[error("utxo inquirer error: {0}")]
    UtxoInquirerError(#[from] UtxoInquirerError),

    #[error("{0}")]
    GeneralOwned(String),
}

pub type ConsensusResult<T> = std::result::Result<T, ConsensusError>;
