use crate::mempool::tx::{Priority, RbfPolicy};
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint};
use kaspa_mining_errors::mempool::RuleError;
use std::{
    fmt::{Display, Formatter},
    sync::Arc,
};

pub(crate) struct MempoolTransaction {
    pub(crate) mtx: MutableTransaction,
    pub(crate) priority: Priority,
    pub(crate) added_at_daa_score: u64,
}

impl MempoolTransaction {
    pub(crate) fn new(mtx: MutableTransaction, priority: Priority, added_at_daa_score: u64) -> Self {
        assert_eq!(mtx.tx.inputs.len(), mtx.entries.len());
        Self { mtx, priority, added_at_daa_score }
    }

    pub(crate) fn id(&self) -> TransactionId {
        self.mtx.tx.id()
    }

    pub(crate) fn feerate(&self) -> f64 {
        self.mtx.calculated_feerate().unwrap()
    }
}

impl RbfPolicy {
    #[cfg(test)]
    /// Returns an alternate policy accepting a transaction insertion in case the policy requires a replacement
    pub(crate) fn for_insert(&self) -> RbfPolicy {
        match self {
            RbfPolicy::Forbidden | RbfPolicy::Allowed => *self,
            RbfPolicy::Mandatory => RbfPolicy::Allowed,
        }
    }
}

pub(crate) struct DoubleSpend {
    pub outpoint: TransactionOutpoint,
    pub owner_id: TransactionId,
}

impl DoubleSpend {
    pub fn new(outpoint: TransactionOutpoint, owner_id: TransactionId) -> Self {
        Self { outpoint, owner_id }
    }
}

impl From<DoubleSpend> for RuleError {
    fn from(value: DoubleSpend) -> Self {
        RuleError::RejectDoubleSpendInMempool(value.outpoint, value.owner_id)
    }
}

impl From<&DoubleSpend> for RuleError {
    fn from(value: &DoubleSpend) -> Self {
        RuleError::RejectDoubleSpendInMempool(value.outpoint, value.owner_id)
    }
}

pub(crate) struct TransactionPreValidation {
    pub transaction: MutableTransaction,
    pub feerate_threshold: Option<f64>,
}

#[derive(Default)]
pub(crate) struct TransactionPostValidation {
    pub removed: Option<Arc<Transaction>>,
    pub accepted: Option<Arc<Transaction>>,
}

#[derive(PartialEq, Eq)]
pub(crate) enum TxRemovalReason {
    Muted,
    Accepted,
    MakingRoom,
    Unorphaned,
    Expired,
    DoubleSpend,
    InvalidInBlockTemplate,
    RevalidationWithMissingOutpoints,
    ReplacedByFee,
    /// kaspa-pq DNS-finality: a `StakeAttestationShard` hard-expired by the attestation TTL
    /// (older than the hard-retention horizon) — removed even when high priority.
    AttestationExpired,
    /// kaspa-pq DNS-finality: a duplicate attestation shard removed/rejected during dedup.
    /// (Rejections surface as `RuleError::RejectDuplicateAttestation`; this removal reason is
    /// provided for completeness/diagnostics and may be used by future dedup-on-removal paths.)
    #[allow(dead_code)]
    AttestationDuplicate,
    /// kaspa-pq DNS-finality: an older attestation shard replaced by a higher-fee one for the same
    /// `(bond, validator, epoch)`.
    AttestationReplaced,
    /// kaspa-pq DNS-finality (audit v24 H-5): a `StakeAttestationShard` the consensus template
    /// classifier dropped with a TERMINAL reason (malformed / validator-id mismatch / bad
    /// signature) — evicted from the mempool so it is not re-selected into every template forever.
    AttestationTemplateDropped,
    /// **V01 (the 2026-09-25 sweep): an H-1 carrier the tip's fold now refuses**, evicted with its
    /// redeemers by the gate sweep every node runs at each new block
    /// (`MiningManager::evict_palw_refused_carriers`) — not only by a template build.
    PalwCarrierRefused,
}

impl TxRemovalReason {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            TxRemovalReason::Muted => "",
            TxRemovalReason::Accepted => "accepted",
            TxRemovalReason::MakingRoom => "making room",
            TxRemovalReason::Unorphaned => "unorphaned",
            TxRemovalReason::Expired => "expired",
            TxRemovalReason::DoubleSpend => "double spend",
            TxRemovalReason::InvalidInBlockTemplate => "invalid in block template",
            TxRemovalReason::RevalidationWithMissingOutpoints => "revalidation with missing outpoints",
            TxRemovalReason::ReplacedByFee => "replaced by fee",
            TxRemovalReason::AttestationExpired => "attestation expired",
            TxRemovalReason::AttestationDuplicate => "attestation duplicate",
            TxRemovalReason::AttestationReplaced => "attestation replaced",
            TxRemovalReason::AttestationTemplateDropped => "attestation template-dropped",
            TxRemovalReason::PalwCarrierRefused => "PALW carrier refused by the tip's fold",
        }
    }

    pub(crate) fn verbose(&self) -> bool {
        !matches!(self, TxRemovalReason::Muted)
    }
}

impl Display for TxRemovalReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
