//! Native Positive carrier provenance, owned by the actual UTXO validator.
//!
//! Register this as a PRIVATE CHILD of `utxo_validation`, not as a public
//! context constructor in consensus-core. `ValidatedTransaction` has public
//! fields and is not provenance by itself: this factory calls the actual
//! validator with script checks against the CURRENT composed accepted prefix.
//!
//! This module does not admit a Court, resolve a canonical inference job, lock
//! money or commit a DB batch. It supplies the linear R/W lease and exact native
//! transaction inputs that the A4/A5/A6 single atomic checkpoint must consume.
//! There is deliberately no successful fallback native store implementation.
//! Signed semantic Court failures belong to A4's accepted outcome/consumed
//! journal path, not to ordinary transaction rejection or rollback here.

use super::{BondSpendFilter, UtxoProcessingContext};
use crate::{
    pipeline::virtual_processor::VirtualStateProcessor,
    processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags,
};
use kaspa_consensus_core::{
    BlockHash,
    dns_finality::ActiveBondView,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2},
    tx::{Transaction, TransactionId, TransactionOutpoint, TransactionOutput, ValidatedTransaction},
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
    },
};
use std::collections::HashSet;

/// Local finite traversal/temporary-storage caps. The native admission owner
/// must additionally acquire the real A4/A6 resource reservation before use.
#[derive(Clone, Copy, Debug)]
pub(in crate::pipeline::virtual_processor) struct NativeCarrierLimitsV2 {
    pub max_inputs: u32,
    pub max_outputs: u32,
    pub max_payload_bytes: u64,
    /// Bounds the duplicate-ID scan over the actual accepted prefix.
    pub max_accepted_prefix_transactions: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::pipeline::virtual_processor) enum NativeContextErrorV2 {
    Inactive,
    Budget,
    Coinbase,
    StaleTransactionId,
    DuplicateTransaction,
    InputCount,
    DuplicateInput,
    MissingInput,
    ChangedInput,
    ExistingOutput,
    Amount,
    Fee,
    NativeValidation(String),
    UtxoDelta(String),
    PrefixChanged,
}

/// Diagnostic prefix identity only. It is never a persistent generation or an
/// authority token: A6 must supply its actual branch/root/ABA-safe generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::pipeline::virtual_processor) struct NativePrefixStampV2 {
    pub accepting: PalwBlockContextV2,
    pub selected_parent: BlockHash,
    pub accepted_transactions: u64,
    pub last_accepted_transaction: Option<TransactionId>,
}

/// A unique borrow of the ACTUAL candidate's mutable prefix and current PALW
/// state. A selected-parent snapshot cannot substitute for the current PALW
/// prefix: the caller must obtain `state` from A6's ordered prefix owner.
/// Its constructor is visible only to the UTXO-validation parent module.
pub(in crate::pipeline::virtual_processor) struct NativePrefixWriterV2<'p, 'block, V: UtxoView + Sync> {
    processor: &'p VirtualStateProcessor,
    context: &'p mut UtxoProcessingContext<'block>,
    state: &'p mut PalwChainStateV2,
    base: &'p V,
    bond_view: Option<&'p ActiveBondView>,
    point: PalwBlockContextV2,
    limits: NativeCarrierLimitsV2,
}

impl<'p, 'block, V: UtxoView + Sync> NativePrefixWriterV2<'p, 'block, V> {
    pub(super) fn from_native_prefix(
        processor: &'p VirtualStateProcessor,
        context: &'p mut UtxoProcessingContext<'block>,
        current_prefix_state: &'p mut PalwChainStateV2,
        selected_parent_utxos: &'p V,
        selected_parent_bond_view: Option<&'p ActiveBondView>,
        actual_accepting_point: PalwBlockContextV2,
        limits: NativeCarrierLimitsV2,
    ) -> Result<Self, NativeContextErrorV2> {
        // Read the actual processor's resolved fence before native transaction
        // hashing, validation, state traversal or temporary delta allocation.
        if !processor.palw_lifecycle_kind_fences.positive_court_reserved_active_at_v2(actual_accepting_point.daa_score) {
            return Err(NativeContextErrorV2::Inactive);
        }
        if limits.max_inputs == 0
            || limits.max_outputs == 0
            || limits.max_payload_bytes == 0
            || limits.max_accepted_prefix_transactions == 0
            || context.accepted_tx_ids.len() as u64 > limits.max_accepted_prefix_transactions
        {
            return Err(NativeContextErrorV2::Budget);
        }
        Ok(Self {
            processor,
            context,
            state: current_prefix_state,
            base: selected_parent_utxos,
            bond_view: selected_parent_bond_view,
            point: actual_accepting_point,
            limits,
        })
    }

    fn stamp(&self) -> NativePrefixStampV2 {
        NativePrefixStampV2 {
            accepting: self.point,
            selected_parent: self.context.selected_parent(),
            accepted_transactions: self.context.accepted_tx_ids.len() as u64,
            last_accepted_transaction: self.context.accepted_tx_ids.last().copied(),
        }
    }

    /// A Full native validation is intentionally performed here; no caller
    /// boolean or arbitrary `ValidatedTransaction` asserts script provenance.
    /// Mempool/read-only rehearsal must use a distinct adapter and acquires no
    /// writable checkpoint from that rehearsal.
    pub(super) fn prepare<'w, 'tx>(
        &'w mut self,
        transaction: &'tx Transaction,
        actual_carrier_block: BlockHash,
        actual_index_within_block: u32,
    ) -> Result<PreparedPositiveCarrierV2<'w, 'p, 'block, 'tx, V>, NativeContextErrorV2> {
        check_limits(transaction, self.limits)?;
        if self.context.accepted_tx_ids.len() as u64 >= self.limits.max_accepted_prefix_transactions {
            return Err(NativeContextErrorV2::Budget);
        }
        let stamp = self.stamp();
        if self.context.accepted_tx_ids.contains(&transaction.id()) {
            return Err(NativeContextErrorV2::DuplicateTransaction);
        }
        let composed = self.base.compose(&self.context.mergeset_diff);
        // Derive the existing spend filter from this SAME native processing
        // context. A requester cannot pass `None` to bypass current PALW or
        // provisional same-mergeset locks. P-specific funding qualification
        // remains A5's additional checkpoint obligation.
        let escrow_locked = !self.context.palw_escrow_locks.is_empty();
        let spend_filter = (self.bond_view.is_some()
            || !self.context.palw_v2_locked_bonds.is_empty()
            || !self.context.palw_v2_bond_burns.is_empty()
            || escrow_locked)
            .then_some(BondSpendFilter {
                bond_view: self.bond_view,
                daa_score: self.point.daa_score,
                palw_locked: &self.context.palw_v2_locked_bonds,
                palw_burns: &self.context.palw_v2_bond_burns,
                escrow: escrow_locked.then_some(&self.context.palw_escrow_locks),
            });
        let validated = self
            .processor
            .validate_transaction_in_utxo_context(transaction, &composed, self.point.daa_score, TxValidationFlags::Full, spend_filter)
            .map_err(|e| NativeContextErrorV2::NativeValidation(e.to_string()))?;
        verify_current_carrier(&validated, &composed, self.limits)?;
        // Stage only THIS transaction's delta. Do not clone the whole prefix
        // diff/state or publish UTXO changes without changing ordinary UTXO acceptance. A state-dependent PALW
        // refusal must leave the payer's normal fallback UTXO path intact.
        let mut transaction_delta = UtxoDiff::default();
        transaction_delta
            .add_transaction(&validated, self.point.daa_score)
            .map_err(|e| NativeContextErrorV2::UtxoDelta(e.to_string()))?;
        Ok(PreparedPositiveCarrierV2 {
            writer: self,
            validated,
            transaction_delta,
            stamp,
            carrier_block: actual_carrier_block,
            index_within_block: actual_index_within_block,
        })
    }
}

/// Single-use validated native-carrier preparation, NOT a complete Court admission, admitted case or
/// qualified spend. Cannot be cloned, decoded, constructed from DTOs or detached
/// from its exclusive prefix borrow. Dropping it publishes no PALW/state/UTXO effect. A caller may still
/// accept the ordinary validated transaction through its normal fallback path.
pub(in crate::pipeline::virtual_processor) struct PreparedPositiveCarrierV2<'w, 'p, 'block, 'tx, V: UtxoView + Sync> {
    writer: &'w mut NativePrefixWriterV2<'p, 'block, V>,
    validated: ValidatedTransaction<'tx>,
    transaction_delta: UtxoDiff,
    stamp: NativePrefixStampV2,
    carrier_block: BlockHash,
    index_within_block: u32,
}
impl<V: UtxoView + Sync> PreparedPositiveCarrierV2<'_, '_, '_, '_, V> {
    pub(in crate::pipeline::virtual_processor) fn validated(&self) -> &ValidatedTransaction<'_> {
        &self.validated
    }
    pub(in crate::pipeline::virtual_processor) fn current_state(&self) -> &PalwChainStateV2 {
        self.writer.state
    }
    pub(in crate::pipeline::virtual_processor) fn prefix(&self) -> NativePrefixStampV2 {
        self.stamp
    }
    /// The funding index is read from the signed FundingConsent by A5. Its
    /// transaction ID/value/script come exclusively from this actual carrier.
    /// This getter grants neither spendability nor a coin-lock receipt.
    pub(in crate::pipeline::virtual_processor) fn output(&self, index: u32) -> Option<(TransactionOutpoint, &TransactionOutput)> {
        Some((TransactionOutpoint::new(self.validated.tx.id(), index), self.validated.tx.outputs.get(index as usize)?))
    }
}

/// Linear handoff to A6's genuine atomic checkpoint owner. All references name
/// one candidate prefix; no synthesized output DTO or historic snapshot can
/// replace them. Receiving these parts is still NOT permission to admit/pay:
/// A4/A5 must first resolve the canonical job, signatures, real quote and locks.
pub(in crate::pipeline::virtual_processor) struct PositiveCarrierWritePartsV2<'w, 'block, 'tx, V: UtxoView + Sync> {
    pub context: &'w mut UtxoProcessingContext<'block>,
    pub state: &'w mut PalwChainStateV2,
    pub selected_parent_utxos: &'w V,
    pub validated: ValidatedTransaction<'tx>,
    pub transaction_delta: UtxoDiff,
    pub prefix: NativePrefixStampV2,
    pub carrier_block: BlockHash,
    pub index_within_block: u32,
}
impl<'w, 'p, 'block, 'tx, V: UtxoView + Sync> PreparedPositiveCarrierV2<'w, 'p, 'block, 'tx, V> {
    /// Consume exactly once, rechecking the real prefix/UTXOs before handing
    /// mutable ownership to the one A6 checkpoint. A6 must stage rows, resource
    /// and money effects with this delta in the SAME DB batch/undo boundary.
    /// There is no `commit -> Ok` placeholder, independent DB writer or success
    /// receipt in this module. Normal prefix advances require a fresh prepare.
    pub(in crate::pipeline::virtual_processor) fn into_checkpoint_parts(
        self,
    ) -> Result<PositiveCarrierWritePartsV2<'w, 'block, 'tx, V>, NativeContextErrorV2> {
        if self.writer.stamp() != self.stamp {
            return Err(NativeContextErrorV2::PrefixChanged);
        }
        let composed = self.writer.base.compose(&self.writer.context.mergeset_diff);
        verify_current_carrier(&self.validated, &composed, self.writer.limits)?;
        Ok(PositiveCarrierWritePartsV2 {
            context: &mut *self.writer.context,
            state: &mut *self.writer.state,
            selected_parent_utxos: self.writer.base,
            validated: self.validated,
            transaction_delta: self.transaction_delta,
            prefix: self.stamp,
            carrier_block: self.carrier_block,
            index_within_block: self.index_within_block,
        })
    }
}

fn check_limits(transaction: &Transaction, limits: NativeCarrierLimitsV2) -> Result<(), NativeContextErrorV2> {
    if transaction.is_coinbase() {
        return Err(NativeContextErrorV2::Coinbase);
    }
    if transaction.inputs.len() as u64 > u64::from(limits.max_inputs)
        || transaction.outputs.len() as u64 > u64::from(limits.max_outputs)
        || transaction.payload.len() as u64 > limits.max_payload_bytes
    {
        return Err(NativeContextErrorV2::Budget);
    }
    if !transaction.id_is_current() {
        return Err(NativeContextErrorV2::StaleTransactionId);
    }
    Ok(())
}

/// Structural/current-view consistency, called only AFTER actual native script
/// validation in the production factory. This helper by itself is not a seal.
fn verify_current_carrier(
    validated: &ValidatedTransaction<'_>,
    view: &impl UtxoView,
    limits: NativeCarrierLimitsV2,
) -> Result<(), NativeContextErrorV2> {
    let tx = validated.tx;
    check_limits(tx, limits)?;
    if tx.inputs.len() != validated.entries.len() {
        return Err(NativeContextErrorV2::InputCount);
    }
    let mut seen = HashSet::with_capacity(tx.inputs.len());
    let mut input = 0u64;
    for (i, entry) in tx.inputs.iter().zip(&validated.entries) {
        if !seen.insert(i.previous_outpoint) {
            return Err(NativeContextErrorV2::DuplicateInput);
        }
        let actual = view.get(&i.previous_outpoint).ok_or(NativeContextErrorV2::MissingInput)?;
        if actual != *entry {
            return Err(NativeContextErrorV2::ChangedInput);
        }
        input = input.checked_add(entry.amount).ok_or(NativeContextErrorV2::Amount)?;
    }
    let mut output = 0u64;
    for (index, value) in tx.outputs.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| NativeContextErrorV2::Budget)?;
        if view.get(&TransactionOutpoint::new(tx.id(), index)).is_some() {
            return Err(NativeContextErrorV2::ExistingOutput);
        }
        output = output.checked_add(value.value).ok_or(NativeContextErrorV2::Amount)?;
    }
    if input.checked_sub(output) != Some(validated.calculated_fee) {
        return Err(NativeContextErrorV2::Fee);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        Hash64,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{ScriptPublicKey, ScriptVec, TransactionInput, UtxoEntry},
    };
    use std::collections::BTreeMap;
    struct View(BTreeMap<TransactionOutpoint, UtxoEntry>);
    impl UtxoView for View {
        fn get(&self, key: &TransactionOutpoint) -> Option<UtxoEntry> {
            self.0.get(key).cloned()
        }
    }
    fn fixture() -> (Transaction, View, UtxoEntry) {
        let key = TransactionOutpoint::new(Hash64::from_u64_word(1), 0);
        let script = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51]));
        let entry = UtxoEntry::new(100, script.clone(), 7, false);
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(key, vec![], u64::MAX, 0)],
            vec![TransactionOutput::new(90, script)],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        (tx, View(BTreeMap::from([(key, entry.clone())])), entry)
    }
    fn limits() -> NativeCarrierLimitsV2 {
        NativeCarrierLimitsV2 { max_inputs: 4, max_outputs: 4, max_payload_bytes: 256, max_accepted_prefix_transactions: 16 }
    }
    #[test]
    fn public_validated_dto_does_not_substitute_current_composed_inputs_or_fee() {
        let (tx, mut view, entry) = fixture();
        let mut validated = ValidatedTransaction { tx: &tx, entries: vec![entry], calculated_fee: 10 };
        assert_eq!(verify_current_carrier(&validated, &view, limits()), Ok(()));
        validated.calculated_fee = 9;
        assert_eq!(verify_current_carrier(&validated, &view, limits()), Err(NativeContextErrorV2::Fee));
        validated.calculated_fee = 10;
        view.0.get_mut(&tx.inputs[0].previous_outpoint).unwrap().amount = 99;
        assert_eq!(verify_current_carrier(&validated, &view, limits()), Err(NativeContextErrorV2::ChangedInput));
        view.0.clear();
        assert_eq!(verify_current_carrier(&validated, &view, limits()), Err(NativeContextErrorV2::MissingInput));
    }
    #[test]
    fn current_prefix_output_collision_and_missing_entries_are_rejected() {
        let (tx, mut view, entry) = fixture();
        let mut validated = ValidatedTransaction { tx: &tx, entries: vec![], calculated_fee: 10 };
        assert_eq!(verify_current_carrier(&validated, &view, limits()), Err(NativeContextErrorV2::InputCount));
        validated.entries.push(entry.clone());
        view.0.insert(TransactionOutpoint::new(tx.id(), 0), entry);
        assert_eq!(verify_current_carrier(&validated, &view, limits()), Err(NativeContextErrorV2::ExistingOutput));
    }
    #[test]
    fn duplicate_inputs_stale_id_and_traversal_caps_fail_before_delta_staging() {
        let (mut tx, view, entry) = fixture();
        tx.inputs.push(tx.inputs[0].clone());
        tx.finalize();
        let validated = ValidatedTransaction { tx: &tx, entries: vec![entry.clone(), entry], calculated_fee: 110 };
        assert_eq!(verify_current_carrier(&validated, &view, limits()), Err(NativeContextErrorV2::DuplicateInput));
        assert_eq!(check_limits(&tx, NativeCarrierLimitsV2 { max_inputs: 1, ..limits() }), Err(NativeContextErrorV2::Budget));
        tx.payload.push(1);
        assert_eq!(check_limits(&tx, limits()), Err(NativeContextErrorV2::StaleTransactionId));
    }
}
