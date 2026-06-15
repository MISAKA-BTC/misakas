use crate::{BlockHash, BlockLevel};

use super::{block::RuleError, tx::TxRuleError};
// kaspa-pq (ADR-0004 / design §12): `ImportedMultisetHashMismatch` carries two
// 64-byte `Hash64` muhash/multiset values; block-identifier positions use `BlockHash`.
use kaspa_hashes::Hash64;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum PruningImportError {
    #[error("pruning proof doesn't have {0} levels")]
    ProofNotEnoughLevels(usize),

    #[error("block {0} level is {1} when it's expected to be at least {2}")]
    PruningProofWrongBlockLevel(BlockHash, BlockLevel, BlockLevel),

    #[error("the proof header {0} is missing known parents at level {1}")]
    PruningProofHeaderWithNoKnownParents(BlockHash, BlockLevel),

    #[error("the proof header {0} at level {1} has blue work inconsistent with its parents")]
    PruningProofInconsistentBlueWork(BlockHash, BlockLevel),

    #[error("proof level {0} is missing the block at depth m in level {1}")]
    PruningProofMissingBlockAtDepthMFromNextLevel(BlockLevel, BlockLevel),

    #[error("the selected tip {0} at level {1} is not a parent of the pruning point")]
    PruningProofMissesBlocksBelowPruningPoint(BlockHash, BlockLevel),

    #[error("the pruning proof selected tip {0} at level {1} is not the pruning point")]
    PruningProofSelectedTipIsNotThePruningPoint(BlockHash, BlockLevel),

    #[error("the pruning proof selected tip {0} at level {1} is not a parent of the pruning point on the same level")]
    PruningProofSelectedTipNotParentOfPruningPoint(BlockHash, BlockLevel),

    #[error("the pruning proof selected tip {0} at level {1} blue score {2} < 2M and root is not genesis")]
    PruningProofSelectedTipNotEnoughBlueScore(BlockHash, BlockLevel, u64),

    #[error("provided pruning proof is weaker than local: {0}")]
    ProofWeaknessError(#[from] ProofWeakness),

    #[error("the pruning proof is missing headers")]
    PruningProofNotEnoughHeaders,

    #[error("block {0} already appeared in the proof headers for level {1}")]
    PruningProofDuplicateHeaderAtLevel(BlockHash, BlockLevel),

    #[error("trusted block {0} is in the anticone of the pruning point but does not have block body")]
    PruningPointAnticoneMissingBody(BlockHash),

    #[error("new pruning point has an invalid transaction {0}: {1}")]
    NewPruningPointTxError(BlockHash, TxRuleError),

    #[error("new pruning point has some invalid transactions")]
    NewPruningPointTxErrors,

    #[error("new pruning point transaction {0} is missing a UTXO entry")]
    NewPruningPointTxMissingUTXOEntry(BlockHash),

    #[error("the imported multiset hash was expected to be {0} and was actually {1}")]
    // kaspa-pq (ADR-0004 / design §12): both muhash/multiset values are 64-byte Hash64.
    ImportedMultisetHashMismatch(Hash64, Hash64),

    #[error("pruning import data lead to validation rule error")]
    PruningImportRuleError(#[from] RuleError),

    #[error("process exit was initiated while validating pruning point proof")]
    PruningValidationInterrupted,

    #[error("block {0} at level {1} has invalid proof of work for level")]
    ProofOfWorkFailed(BlockHash, BlockLevel),

    #[error("pruning proof header {0} at level {1} has unknown pow_algo_id {2}; Phase 1 admits only POW_ALGO_ID_KHEAVYHASH = 1")]
    PruningProofUnknownPowAlgoId(BlockHash, BlockLevel, u8),

    #[error("past pruning points at indices {0}, {1} have non monotonic blue score {2}, {3}")]
    InconsistentPastPruningPoints(usize, usize, u64, u64),

    #[error("past pruning points contains {0} duplications")]
    DuplicatedPastPruningPoints(usize),

    #[error("pruning point {0} of header {1} is not consistent with past pruning points")]
    WrongHeaderPruningPoint(BlockHash, BlockHash),

    #[error("a past pruning point is pointing at a missing point")]
    MissingPointedPruningPoint,

    #[error("a past pruning point is pointing at the wrong point")]
    WrongPointedPruningPoint,

    #[error("a past pruning point has not been pointed at")]
    UnpointedPruningPoint,

    #[error("got trusted block {0} in the future of the pruning point {1}")]
    TrustedBlockInPruningPointFuture(BlockHash, BlockHash),

    // kaspa-pq ADR-0022: pruned-IBD EVM/overlay snapshot import verification.
    #[error("imported EVM execution header for pruning point {0} has commitment {1} but the L1 header commits {2}")]
    ImportedEvmCommitmentMismatch(BlockHash, Hash64, Hash64),

    #[error("imported EVM state snapshot for pruning point {0} has state root {1:?} but the EVM header commits {2:?}")]
    ImportedEvmStateRootMismatch(BlockHash, kaspa_hashes::EvmH256, kaspa_hashes::EvmH256),

    #[error("imported EVM state snapshot for pruning point {0} is invalid: {1}")]
    ImportedEvmSnapshotInvalid(BlockHash, String),

    #[error("imported overlay snapshot for pruning point {0} has commitment {1} but the L1 header commits {2}")]
    ImportedOverlayCommitmentMismatch(BlockHash, Hash64, Hash64),

    #[error("imported overlay bond {0} references an outpoint absent from the imported UTXO set")]
    ImportedOverlayBondMissingUtxo(BlockHash),
}

#[derive(Error, Debug, Clone)]
pub enum ProofWeakness {
    #[error("no sufficient blue work in order to replace the current DAG")]
    InsufficientBlueWork,

    #[error("no shared blocks with the known level DAGs, and not enough headers from levels higher than the existing block levels.")]
    NotEnoughHeaders,
}

pub type PruningImportResult<T> = std::result::Result<T, PruningImportError>;
