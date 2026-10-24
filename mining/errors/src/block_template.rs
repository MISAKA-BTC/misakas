use kaspa_consensus_core::errors::{block::RuleError, coinbase::CoinbaseError};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum BuilderError {
    #[error("local template capacity unavailable: {0}")]
    LocalNativeCapacityUnavailable(#[from] kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1),
    /// A consensus rule error
    #[error(transparent)]
    ConsensusError(#[from] RuleError),

    /// A coinbase error
    #[error(transparent)]
    CoinbaseError(#[from] CoinbaseError),
}

pub type BuilderResult<T> = std::result::Result<T, BuilderError>;
