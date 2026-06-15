use std::{collections::HashMap, fmt::Display};

use crate::{
    BlockHash, BlueWorkType,
    errors::{coinbase::CoinbaseError, tx::TxRuleError},
    tx::{TransactionId, TransactionOutpoint},
};
use itertools::Itertools;
// kaspa-pq (ADR-0004 / design §12): the two utxo-commitment positions of
// `BadUTXOCommitment` are 64-byte `Hash64`; block-identifier positions use `BlockHash`.
use kaspa_hashes::Hash64;
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
    #[error("wrong block version: got {0} but expected {1}")]
    WrongBlockVersion(u16, u16),

    #[error("unknown pow_algo_id {0}: Phase 1 admits only kHeavyHash (POW_ALGO_ID_KHEAVYHASH = 1)")]
    UnknownPowAlgoId(u8),

    // kaspa-pq Selected-Parent EVM Lane (ADR-0020). The EVM state-root / receipts
    // / commitment mismatch variants are added in the executor phase (P2) when
    // they are actually produced.
    #[error("block carries a non-empty EVM payload but its header version is below EVM_HEADER_VERSION (EVM lane not active)")]
    NonEmptyEvmPayloadBeforeActivation,

    // audit #6: a pre-v2 header's EVM commitment fields are hash-invisible (the
    // preimage only includes them from EVM_HEADER_VERSION up), so non-zero values
    // there would be block-id malleability + a migration hazard. Force them zero.
    #[error("pre-activation (v1) header carries non-zero EVM commitment fields (must be zero while hash-invisible)")]
    NonZeroEvmHeaderFieldsBeforeActivation,

    // audit R2-#4: the producer-side EVM acceptance run failed while building a
    // template (e.g. a local EVM store-integrity error). A template build failure,
    // NOT a panic — the node skips producing rather than crashing.
    #[error("EVM template acceptance execution failed: {0}")]
    EvmTemplateExecutionFailed(String),

    // v0.4 §4.1 / §6.2: the header's payload DATA commitment must match the body.
    #[error("header evm_payload_hash does not match the block body's EVM payload")]
    EvmPayloadHashMismatch,

    // v0.4 §7 (D4 inclusion-side cap, checked at body validation).
    #[error("EVM payload too large: {0} bytes exceeds MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK = {1}")]
    EvmPayloadTooLarge(usize, usize),

    // v0.4 §9.2: bounded producer-selected system ops.
    #[error("too many EVM system ops: {0} exceeds MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK = {1}")]
    TooManyEvmSystemOps(usize, usize),

    // v0.4 §6.1 class 1: payload admission — a producer including a
    // syntactically inadmissible tx invalidates ITS OWN block (cheap syntactic
    // check; acceptance-time conditions are class-2 skips, never block faults).
    #[error("EVM payload tx {0} fails class-1 admission: {1}")]
    EvmPayloadTxInadmissible(usize, String),

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

    /// kaspa-pq PQ-only invariant: the coinbase payload's miner script must itself be ML-DSA
    /// P2PKH. Unlike the block's coinbase outputs (PQ-class-checked in isolation), the payload
    /// miner script flows into descendant blocks' reward fan-out, so a non-PQ script here would
    /// poison the reward path (every descendant's coinbase would carry a non-PQ output the PQ
    /// rule rejects) — a consensus-liveness hazard. Rejected at the source block.
    #[error("coinbase payload miner script is not a PQ-standard (ML-DSA P2PKH) script")]
    NonPqCoinbasePayloadScript,

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
    // kaspa-pq (ADR-0004 / design §12): the two commitment positions are 64-byte Hash64.
    BadUTXOCommitment(BlockHash, Hash64, Hash64),

    #[error("block {0} overlay commitment is invalid - block header indicates {1}, but calculated value is {2}")]
    // kaspa-pq ADR-0022: the DNS/PoS-v2 OverlaySnapshot commitment (as-of selected parent).
    // Positions 1/2 are 64-byte Hash64. Surfaces as StatusDisqualifiedFromChain like any c==v fault.
    BadOverlayCommitment(BlockHash, Hash64, Hash64),

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
    // attestation's target DAA score) with a valid ML-DSA-87 signature is
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

    // kaspa-pq H-05 (audit / ADR-0010 "Unbonding"): a block carrying a
    // `StakeUnbondRequest` that is not owner-authorized — its bond is unknown in
    // the block's selected-parent view, is not `Pending`/`Active` at the block's
    // DAA score, or its `owner_pubkey` does not hash to the bond's
    // `owner_pubkey_hash` / does not ML-DSA-verify over the canonical unbond
    // digest — is rejected, so an attacker cannot force an honest validator's
    // bond into `Unbonding` (a liveness/grief attack). Args: the unbond tx id
    // and the referenced bond outpoint.
    #[error("block includes an unauthorized stake-unbond request: tx {0} for bond {1}")]
    UnauthorizedUnbondRequestInBlock(TransactionId, TransactionOutpoint),

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
