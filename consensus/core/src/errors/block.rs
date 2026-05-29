use std::{collections::HashMap, fmt::Display};

use crate::{
    BlockHash, BlueWorkType, constants,
    errors::{coinbase::CoinbaseError, tx::TxRuleError},
    tx::{TransactionId, TransactionOutpoint},
};
use itertools::Itertools;
// PR-9.5e: `Hash` (32-byte) retained only for the two utxo-commitment
// positions of `BadUTXOCommitment`; block-identifier positions use `BlockHash`.
use kaspa_hashes::Hash;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct VecDisplay<T: Display>(pub Vec<T>);
impl<T: Display> Display for VecDisplay<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", self.0.iter().map(|item| item.to_string()).join(", "))
    }
}

#[derive(Clone, Debug)]
pub struct TwoDimVecDisplay<T: Display + Clone>(pub Vec<Vec<T>>);
impl<T: Display + Clone> Display for TwoDimVecDisplay<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[\n\t{}\n]", self.0.iter().cloned().map(|item| VecDisplay(item).to_string()).join(", \n\t"))
    }
}

#[derive(Error, Debug, Clone)]
pub enum RuleError {
    #[error("wrong block version: got {0} but expected {}", constants::BLOCK_VERSION)]
    WrongBlockVersion(u16),

    #[error("the block timestamp is too far into the future: block timestamp is {0} but maximum timestamp allowed is {1}")]
    TimeTooFarIntoTheFuture(u64, u64),

    #[error("block has no parents")]
    NoParents,

    #[error("block has too many parents: got {0} when the limit is {1}")]
    TooManyParents(usize, usize),

    #[error("block has ORIGIN as one of its parents")]
    OriginParent,

    #[error("parent {0} is an ancestor of parent {1}")]
    InvalidParentsRelation(BlockHash, BlockHash),

    #[error("parent {0} is invalid")]
    InvalidParent(BlockHash),

    #[error("block has missing parents: {0:?}")]
    MissingParents(Vec<BlockHash>),

    #[error("pruning point {0} is not in the past of this block")]
    PruningViolation(BlockHash),

    #[error("expected header daa score {0} but got {1}")]
    UnexpectedHeaderDaaScore(u64, u64),

    #[error("expected header blue score {0} but got {1}")]
    UnexpectedHeaderBlueScore(u64, u64),

    #[error("expected header blue work {0} but got {1}")]
    UnexpectedHeaderBlueWork(BlueWorkType, BlueWorkType),

    #[error("block {0} difficulty of {1} is not the expected value of {2}")]
    UnexpectedDifficulty(BlockHash, u32, u32),

    #[error("block timestamp of {0} is not after expected {1}")]
    TimeTooOld(u64, u64),

    #[error("block is known to be invalid")]
    KnownInvalid,

    #[error("block merges {0} blocks > {1} merge set size limit")]
    MergeSetTooBig(u64, u64),

    #[error("block is violating bounded merge depth")]
    ViolatingBoundedMergeDepth,

    #[error("invalid merkle root: header indicates {0} but calculated value is {1}")]
    // PR-9.5c: `MerkleRoot` widened to `Hash64`; both arguments
    // carry the wider value.
    BadMerkleRoot(crate::MerkleRoot, crate::MerkleRoot),

    #[error("block has no transactions")]
    NoTransactions,

    #[error("block first transaction is not coinbase")]
    FirstTxNotCoinbase,

    #[error("block has second coinbase transaction as index {0}")]
    MultipleCoinbases(usize),

    #[error("bad coinbase payload: {0}")]
    BadCoinbasePayload(CoinbaseError),

    #[error("coinbase blue score of {0} is not the expected value of {1}")]
    BadCoinbasePayloadBlueScore(u64, u64),

    #[error("transaction in isolation validation failed for tx {0}: {1}")]
    TxInIsolationValidationFailed(TransactionId, TxRuleError),

    #[error("block compute mass {0} exceeds limit of {1}")]
    ExceedsComputeMassLimit(u64, u64),

    #[error("block transient storage mass {0} exceeds limit of {1}")]
    ExceedsTransientMassLimit(u64, u64),

    #[error("block persistent storage mass {0} exceeds limit of {1}")]
    ExceedsStorageMassLimit(u64, u64),

    #[error("outpoint {0} is spent more than once on the same block")]
    DoubleSpendInSameBlock(TransactionOutpoint),

    #[error("outpoint {0} is created and spent on the same block")]
    ChainedTransaction(TransactionOutpoint),

    #[error("transaction in context validation failed for tx {0}: {1}")]
    TxInContextFailed(TransactionId, TxRuleError),

    #[error("wrong coinbase subsidy: expected {0} but got {1}")]
    WrongSubsidy(u64, u64),

    #[error("transaction {0} is found more than once in the block")]
    DuplicateTransactions(TransactionId),

    #[error("block has invalid proof-of-work")]
    InvalidPoW,

    #[error("expected header pruning point is {0} but got {1}")]
    WrongHeaderPruningPoint(BlockHash, BlockHash),

    #[error("expected indirect parents {0} but got {1}")]
    UnexpectedIndirectParents(TwoDimVecDisplay<BlockHash>, TwoDimVecDisplay<BlockHash>),

    #[error("block {0} UTXO commitment is invalid - block header indicates {1}, but calculated value is {2}")]
    BadUTXOCommitment(BlockHash, Hash, Hash),

    #[error("block {0} accepted ID merkle root is invalid - block header indicates {1}, but calculated value is {2}")]
    // PR-9.5c: positions 1 and 2 carry `AcceptedIdMerkleRoot`
    // (= `Hash64`). The block-identifier (position 0) is still
    // 32-byte `BlockHash` — that flips with the rest of `BlockHash`
    // in PR-9.5d.
    BadAcceptedIDMerkleRoot(BlockHash, crate::AcceptedIdMerkleRoot, crate::AcceptedIdMerkleRoot),

    #[error("coinbase transaction is not built as expected")]
    BadCoinbaseTransaction,

    // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): the Model-B
    // reward-eligibility block-validity rule. A block carrying a
    // `StakeAttestationShard` whose attestation does not resolve to an
    // `Active` bond (in the block's selected-parent bond view, at the
    // attestation's target DAA score) with a valid ML-DSA-65 signature is
    // rejected, so that every included attestation is rewardable and the
    // coinbase fan-out needs no skip set. Args: the referenced bond's
    // transaction id and the attestation epoch. Inert below
    // `dns_activation_daa_score`.
    #[error("block includes an ineligible stake attestation: bond {0} epoch {1} is not an active bond with a valid signature")]
    IneligibleAttestationInBlock(TransactionId, u64),

    // kaspa-pq Phase 10/11 (ADR-0009 §"SlashingEvidencePayload" / ADR-0013):
    // a block carrying a SlashingEvidence whose evidence is not genuine —
    // its referenced bond is unknown in the block's selected-parent bond view,
    // or one of the two equivocating attestations does not ML-DSA-verify
    // against that bond's validator key — is rejected, so a well-formed but
    // forged evidence cannot slash a bond. Arg: the referenced bond's
    // transaction id. Inert below dns_activation_daa_score.
    #[error("block includes unverifiable slashing evidence against bond {0}")]
    UnverifiableSlashingEvidenceInBlock(TransactionId),

    // kaspa-pq Phase 10/11 (ADR-0016 §D.2): the bond-UTXO spend-gate. A block
    // containing a transaction whose input spends a known bond outpoint (present
    // in the block's selected-parent active-bond view) whose bond is not
    // releasable — i.e. not `Unbonding` with the block's DAA score at or past
    // `unbond_request_daa_score + unbonding_period_blocks` — is rejected, so a
    // bond's staked output-0 is unspendable while the bond is `Pending`,
    // `Active`, mid-unbonding, or `Slashed`. This is what makes the declared
    // stake real locked capital. Args: the spending transaction id and the bond
    // outpoint it illegally spends. Inert below `dns_activation_daa_score`.
    #[error("block transaction {0} spends non-releasable bond outpoint {1}")]
    NonReleasableBondSpendInBlock(TransactionId, TransactionOutpoint),

    // kaspa-pq Phase 11 (ADR-0013 Addendum C.1.5): the strict slashing-tx
    // inclusion discipline. A block carrying a `SlashingEvidence` tx that is not
    // *effective* — its bond is unknown in the block's selected-parent bond
    // view, is not `Active`/`Unbonding` at the block's DAA score (no removable
    // locked output-0), or duplicates a bond already slashed earlier in the
    // block — is rejected, so every included slashing tx maps 1:1 to exactly one
    // stake removal + reporter mint. Arg: the offending transaction id. Inert
    // below `dns_activation_daa_score`.
    #[error("block includes an ineffective slashing transaction {0}")]
    IneffectiveSlashingInBlock(TransactionId),

    // kaspa-pq Phase 11 (ADR-0013 Addendum C.1.3): the reporter-output rule. A
    // block whose effective `SlashingEvidence` tx does not carry the exact
    // consensus-mandated reporter reward at `output[0]` (value + P2PKH spk), or
    // whose reward rounds to zero (an unslashable micro-bond), is rejected, so a
    // slashing tx mints exactly the reward and nothing else. Arg: the offending
    // transaction id. Inert below `dns_activation_daa_score`.
    #[error("block slashing transaction {0} has an incorrect reporter output")]
    WrongSlashingReporterOutput(TransactionId),

    #[error("{0} non-coinbase transactions (out of {1}) are invalid in UTXO context")]
    InvalidTransactionsInUtxoContext(usize, usize),

    #[error("invalid transactions in new block template")]
    InvalidTransactionsInNewBlock(HashMap<TransactionId, TxRuleError>),

    #[error("DAA window data has only {0} entries")]
    InsufficientDaaWindowSize(usize),

    /// Currently this error is never created because it is impossible to submit such a block
    #[error("cannot add block body to a pruned block")]
    PrunedBlock,
}

pub type BlockProcessResult<T> = std::result::Result<T, RuleError>;
