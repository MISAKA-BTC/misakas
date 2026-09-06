use crate::constants::MAX_SOMPI;
use crate::dns_finality::DnsTxError;
use crate::subnets::SubnetworkId;
use crate::tx::TransactionOutpoint;
use kaspa_txscript_errors::TxScriptError;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum TxRuleError {
    #[error("transaction has no inputs")]
    NoTxInputs,

    #[error("transaction has duplicate inputs")]
    TxDuplicateInputs,

    #[error("transaction has non zero gas value")]
    TxHasGas,

    #[error("transaction version {0} is unknown")]
    UnknownTxVersion(u16),

    #[error("transaction has {0} inputs where the max allowed is {1}")]
    TooManyInputs(usize, usize),

    #[error("transaction has {0} outputs where the max allowed is {1}")]
    TooManyOutputs(usize, usize),

    #[error("transaction input #{0} signature script is above {1} bytes")]
    TooBigSignatureScript(usize, usize),

    #[error("transaction input #{0} signature script is above {1} bytes")]
    TooBigScriptPublicKey(usize, usize),

    #[error("transaction input #{0} is not finalized")]
    NotFinalized(usize),

    #[error("coinbase transaction has {0} inputs while none are expected")]
    CoinbaseHasInputs(usize),

    #[error("coinbase transaction has {0} outputs while at most {1} are expected")]
    CoinbaseTooManyOutputs(usize, u64),

    #[error("script public key of coinbase output #{0} is too long")]
    CoinbaseScriptPublicKeyTooLong(usize),

    #[error("coinbase mass commitment field is not zero")]
    CoinbaseNonZeroMassCommitment,

    #[error(
        "transaction input #{0} tried to spend coinbase outpoint {1} with daa score of {2} 
    while the merging block daa score is {3} and the coinbase maturity period of {4} hasn't passed yet"
    )]
    ImmatureCoinbaseSpend(usize, TransactionOutpoint, u64, u64, u64),

    #[error("transaction total inputs spending amount overflowed u64")]
    InputAmountOverflow,

    #[error("transaction total inputs spending amount is higher than the max allowed of {}", MAX_SOMPI)]
    InputAmountTooHigh,

    #[error("transaction output {0} has zero value")]
    TxOutZero(usize),

    #[error("transaction output {0} value is higher than the max allowed of {}", MAX_SOMPI)]
    TxOutTooHigh(usize),

    #[error("transaction total outputs value overflowed u64")]
    OutputsValueOverflow,

    #[error("transaction total outputs value is higher than the max allowed of {}", MAX_SOMPI)]
    TotalTxOutTooHigh,

    #[error("transaction tries to spend {0} while its total inputs amount is {1}")]
    SpendTooHigh(u64, u64),

    #[error("one of the transaction sequence locks conditions was not met")]
    SequenceLockConditionsAreNotMet,

    #[error("outpoints corresponding to some transaction inputs are missing from current utxo context")]
    MissingTxOutpoints,

    #[error("failed to verify the signature script: {0}")]
    SignatureInvalid(TxScriptError),

    #[error("failed to verify empty signature script. Inner error: {0}")]
    SignatureEmpty(TxScriptError),

    #[error("input {0} sig op count is {1}, but the calculated value is {2}")]
    WrongSigOpCount(usize, u64, u64),

    #[error("contextual mass (including storage mass) is incomputable")]
    MassIncomputable,

    #[error("calculated contextual mass (including storage mass) {0} is not equal to the committed mass field {1}")]
    WrongMass(u64, u64),

    #[error("transaction subnetwork id {0} is neither native nor coinbase")]
    SubnetworksDisabled(SubnetworkId),

    /// kaspa-pq Phase 10 (ADR-0009): a transaction routed by a DNS finality
    /// overlay subnetwork carried a payload that failed stateless validation
    /// (see [`crate::dns_finality::dns_tx_kind`] + `validate_*_payload`).
    #[error("transaction has an invalid DNS finality overlay payload: {0}")]
    InvalidDnsOverlayPayload(DnsTxError),

    /// MISAKA Compute Token Program (design v0.1 §4.3): a transaction on the
    /// token-op band (0x30/0x31) carried a payload that failed stateless
    /// validation (see [`crate::token::validate_token_transfer_payload`] /
    /// `validate_token_burn_payload`). Nonce currency, balance sufficiency and
    /// the ML-DSA-87 signature are stateful and judged by the ledger fold —
    /// where a failing op is void (skip-class), not consensus-fatal.
    #[error("transaction has an invalid token-op payload: {0}")]
    InvalidTokenPayload(crate::token::TokenTxError),

    /// MISAKA PALW chain carriage (ADR-0029 Stage 1): a transaction on the PALW
    /// carriage band (0x40-0x45) carried a payload that failed stateless
    /// validation (see [`crate::palw_carriage::palw_carriage_tx_kind`] +
    /// [`crate::palw_carriage::validate_palw_carriage_stage1_tx`]). Bond
    /// existence, ML-DSA-87 signature validity and every cross-object question
    /// are stateful and belong to the Stage-2 walk — a stateless-valid carriage
    /// can still be a lie; it cannot be incoherent.
    #[error("transaction has an invalid PALW carriage payload: {0}")]
    InvalidPalwCarriagePayload(crate::palw_carriage::PalwCarriageError),

    /// ADR-0044: a transaction on the free-prompt commitment subnetwork (0x4a) carried a payload
    /// that failed the context-free half of its stateless rules. The network-dependent half — the
    /// domain binding and the derived CU price — is the extraction walk's, which holds the bundle
    /// and skips rather than rejects; see `palw_fp_objects_v3::validate_palw_fp_commitment_tx`.
    #[error("transaction has an invalid PALW free-prompt payload: {0}")]
    InvalidPalwFpPayload(crate::palw_freeprompt_v3::PalwFpV3Error),

    /// ADR-0042 Decisions 7/8: a transaction on the claim-lifecycle subnetwork (0x4b) carried a
    /// payload that does not decode, names another wire version, or carries an object kind that
    /// may not enter a chain through a transaction at all (a bond registration, a class
    /// registration, a derived panel binding, or a free-prompt commitment — each excluded for its
    /// own reason, see `palw_lifecycle_objects_v2`).
    #[error("transaction has an invalid PALW lifecycle payload: {0}")]
    InvalidPalwLifecyclePayload(crate::palw_lifecycle_objects_v2::PalwLifecycleTxError),

    /// [`TxRuleError::FeerateTooLow`] is not a consensus error but a mempool error triggered by the
    /// fee/mass RBF validation rule
    #[error("fee rate per contextual mass gram is not greater than the fee rate of the replaced transaction")]
    FeerateTooLow,

    /// kaspa-pq PQ-only (ADR-0019 §7 / docs/kaspa-pq-design-mldsa87.md): on a
    /// PQ-active network a transaction (native, coinbase, or DNS overlay) created
    /// an output whose script is not the sole standard ML-DSA-87 P2PKH class.
    /// Enforced with no exemptions so non-PQ, signature-free UTXOs (e.g. OP_TRUE)
    /// cannot enter the set via a coinbase miner output or an overlay output.
    #[error("transaction output #{0} uses a non-PQ script class (only ML-DSA P2PKH is standard in PQ-only mode)")]
    NonPqStandardOutputClass(usize),

    /// kaspa-pq PQ-only (ADR-0019 §6): on a PQ-active network a transaction spent
    /// an input whose referenced UTXO script is not the standard ML-DSA-87 P2PKH
    /// class. The spend-side complement to [`Self::NonPqStandardOutputClass`]: it
    /// makes any non-PQ UTXO (one created via a pre-fix exemption, or carrying an
    /// unknown script version) unspendable, so no value can move without an
    /// ML-DSA signature.
    #[error("transaction input #{0} spends a non-PQ script class UTXO (only ML-DSA P2PKH is spendable in PQ-only mode)")]
    NonPqStandardInputClass(usize),

    /// kaspa-pq EVM Lane v0.4 §9.2 (AC-2): an `EVM_DEPOSIT_LOCK` input was
    /// spent before its refund window opened — while `pov_daa < timeout` the
    /// lock is exclusively claimable by a `DepositClaim` system op.
    #[error("transaction input #{0} refunds an EVM deposit lock too early (pov daa {1} < timeout {2})")]
    EvmDepositLockNotRefundableYet(usize, u64, u64),

    /// kaspa-pq EVM Lane v0.4 §9.2 (audit F3): an `EVM_DEPOSIT_LOCK` output
    /// declared a `claim_tip` greater than its own value. The claim path rejects
    /// `claim_tip > amount`, so such a lock could never be claimed — it would
    /// only strand value until the refund window (permanent if `timeout =
    /// u64::MAX`). Rejected at creation so consensus never mints an unclaimable
    /// "bridge deposit".
    #[error("transaction output #{0} is an unclaimable EVM deposit lock (claim_tip {1} > value {2})")]
    EvmDepositLockTipExceedsValue(usize, u64, u64),

    /// **ADR-0087 Decision 6 (mainnet audit 2026-09-06, M-9): a model-market sink output before
    /// the market's own activation.**
    ///
    /// Decision 6 says "below the activation the objects are refused and no market exists". The PQ
    /// output-class rule lives in ISOLATION, which holds no DAA score, so isolation can only ask
    /// the height-free question "does this network declare the market at all". This is the
    /// height-indexed half, asked where a DAA score exists: a sink output is a legal output class
    /// only at or after `palw_model_market`'s activation. A build that merely SCHEDULES the fence
    /// therefore has the same transaction validity as one that does not carry it, right up to the
    /// activation score — which is what makes the scheduled and the dormant arms agree, and what
    /// stops a scheduled build from being an attacker-timed premature flag day for the other.
    #[error("transaction output #{0} is a model-market sink before the market's activation (daa {1})")]
    ModelSinkBeforeMarketActivation(usize, u64),

    /// kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): a transaction spends a known
    /// non-releasable bond's locked output-0 ({0}). Above the
    /// `bond_spend_gate_mergeset_activation_daa_score` fence the per-tx UTXO validation rejects such a
    /// spend, so it is NOT accepted (skipped like any invalid mergeset tx — the carrying block stays
    /// valid, the bond UTXO stays locked). This closes the merge-blue mergeset hole the legacy
    /// own-body spend-gate (`bond_spend_gate`) cannot see. Inert below the fence (the per-tx check is
    /// only wired when the fence is reached), so it never fires on a current network.
    #[error("transaction input spends a non-releasable bond's locked output-0 (outpoint {0})")]
    SpendsNonReleasableBond(TransactionOutpoint),

    /// MISAKA audit C-08: a transaction releasing a slashed PALW bond's collateral must leave at
    /// least what the bond lost unclaimed by any output. The block's fee pool loses the same
    /// amount, so the sompi are destroyed rather than redirected to the miner.
    #[error("a released bond's spend must burn {owed} sompi and leaves only {left}")]
    BondBurnNotPaid { owed: u64, left: u64 },
}

pub type TxResult<T> = std::result::Result<T, TxRuleError>;
