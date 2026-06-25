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

    /// kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): a transaction spends a known
    /// non-releasable bond's locked output-0 ({0}). Above the
    /// `bond_spend_gate_mergeset_activation_daa_score` fence the per-tx UTXO validation rejects such a
    /// spend, so it is NOT accepted (skipped like any invalid mergeset tx — the carrying block stays
    /// valid, the bond UTXO stays locked). This closes the merge-blue mergeset hole the legacy
    /// own-body spend-gate (`bond_spend_gate`) cannot see. Inert below the fence (the per-tx check is
    /// only wired when the fence is reached), so it never fires on a current network.
    #[error("transaction input spends a non-releasable bond's locked output-0 (outpoint {0})")]
    SpendsNonReleasableBond(TransactionOutpoint),
}

pub type TxResult<T> = std::result::Result<T, TxRuleError>;
