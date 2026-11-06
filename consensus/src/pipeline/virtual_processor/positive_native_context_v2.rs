//! PRIVATE child of the actual UTXO validator. A seal records Full script
//! validation at one real accepted prefix; it is NOT Court admission, a live
//! funding output, a coin lock, a role, a generation token or credit authority.
//! Ordinary UTXO acceptance remains the sole applier of transaction effects.
//! The second phase must independently resolve notice/origin, current roles,
//! policy, funding, resource leases and the canonical TransitionBuilder rows.
use super::BondSpendFilter;
use crate::model::{services::reachability::ReachabilityService, stores::headers::HeaderStoreReader};
use crate::{
    pipeline::virtual_processor::VirtualStateProcessor,
    processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags,
};
use kaspa_consensus_core::{
    BlockHash,
    acceptance_data::AcceptanceData,
    block::BlockTransactionsV3,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_positive_funding_relay_v2::pending_funding_relay_archive_workspace_v2,
    palw_state_v2::PalwBlockContextV2,
    tx::{Transaction, TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry, ValidatedTransaction},
    utxo::utxo_view::UtxoView,
};
use misaka_palw_positive_contract::assignment::funding_relay_rows::{FUNDING_RELAY_CHARGE_BYTES_V2, FundingRelayCarrierFeesV2};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Session(u64);

#[derive(Debug, PartialEq, Eq)]
pub(in crate::pipeline::virtual_processor) enum NativeContextErrorV2 {
    Inactive,
    Coinbase,
    StaleTransactionId,
    InputCount,
    DuplicateInput,
    MissingInput,
    ChangedInput,
    ExistingOutput,
    Amount,
    Fee,
    NativeValidation,
    Origin,
    Prefix,
}
/// Diagnostic origin only. A6's canonical row/epoch remains the ABA authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::pipeline::virtual_processor) struct NativePrefixStampV2 {
    pub accepting: PalwBlockContextV2,
    pub selected_parent: BlockHash,
    pub accepted_transactions: u64,
    pub last_accepted_transaction: Option<TransactionId>,
}
/// No public fields, Clone, Deserialize or DTO conversion. Keeping the actual
/// admitted block body also retains its allocation owner; no deep tx copy.
/// Production capture requires a real native RAM lease before Full validation
/// or retained-entry allocation. `_lease: None` is only for private structural
/// tests, which make no validation, resource or admission claim.
pub(in crate::pipeline::virtual_processor) struct OwnedPositiveCarrierV2 {
    session: Session,
    body: BlockTransactionsV3,
    entries: Vec<UtxoEntry>,
    fee: u64,
    relay_carrier_fees: Option<FundingRelayCarrierFeesV2>,
    prefix: NativePrefixStampV2,
    carrier_block: BlockHash,
    index_within_block: u32,
    _lease: Option<NativeDemandLeaseV1>,
}
impl OwnedPositiveCarrierV2 {
    pub(in crate::pipeline::virtual_processor) fn transaction(&self) -> &Transaction {
        &self.body[self.index_within_block as usize]
    }
    pub(in crate::pipeline::virtual_processor) fn entries(&self) -> &[UtxoEntry] {
        &self.entries
    }
    pub(in crate::pipeline::virtual_processor) fn fee(&self) -> u64 {
        self.fee
    }
    pub(in crate::pipeline::virtual_processor) fn prefix(&self) -> NativePrefixStampV2 {
        self.prefix
    }
    pub(in crate::pipeline::virtual_processor) fn origin(&self) -> (BlockHash, u32) {
        (self.carrier_block, self.index_within_block)
    }
    /// An output in the original carrier, NOT evidence that it remains unspent.
    pub(in crate::pipeline::virtual_processor) fn output(&self, index: u32) -> Option<(TransactionOutpoint, &TransactionOutput)> {
        Some((TransactionOutpoint::new(self.transaction().id(), index), self.transaction().outputs.get(index as usize)?))
    }
}
#[derive(Default)]
pub(in crate::pipeline::virtual_processor) struct PositiveCarrierCompanionsV2 {
    session: Option<Session>,
    carriers: Vec<OwnedPositiveCarrierV2>,
    resource_fault: Option<NativeCoverageV1>,
}
/// An unarmed reservation is not capacity approval for this active native
/// consumer. This conversion never creates a ticket or arms a global checker.
fn require_positive_carrier_lease_v2(lease: Option<NativeDemandLeaseV1>) -> Result<NativeDemandLeaseV1, NativeCoverageV1> {
    lease.ok_or_else(|| {
        NativeCoverageV1::unsupported::<PositiveCarrierCompanionsV2>(
            "positive carrier provenance",
            "required native capacity lease is not armed",
        )
    })
}
impl PositiveCarrierCompanionsV2 {
    pub(super) fn capture(
        &mut self,
        processor: &VirtualStateProcessor,
        body: &BlockTransactionsV3,
        index: u32,
        actual_carrier_block: BlockHash,
        prefix: NativePrefixStampV2,
        ordinary: &ValidatedTransaction<'_>,
        view: &impl UtxoView,
        filter: Option<BondSpendFilter<'_>>,
    ) {
        if self.resource_fault.is_some() {
            return;
        }
        let Some(tx) = body.get(index as usize) else {
            return;
        };
        if tx.id() != ordinary.tx.id() {
            return;
        }
        if !processor.palw_lifecycle_kind_fences.positive_court_reserved_active_at_v2(prefix.accepting.daa_score) {
            return;
        }
        // The ordinary validator supplied sizing data, never a script-success
        // assertion. Reserve retained entries, their scripts and vector growth
        // BEFORE the second Full validator allocates its independent entries.
        let bytes =
            ordinary.entries.iter().fold((std::mem::size_of::<OwnedPositiveCarrierV2>() as u64).checked_mul(3), |total, entry| {
                total
                    .and_then(|n| n.checked_add(2 * std::mem::size_of::<UtxoEntry>() as u64))
                    .and_then(|n| n.checked_add(2 * entry.script_public_key.script().len() as u64))
            });
        let Some(bytes) = bytes else {
            self.resource_fault = Some(NativeCoverageV1::overflow::<Self>("positive carrier provenance"));
            return;
        };
        let lease = match reserve_native_requests_v1(bytes).and_then(require_positive_carrier_lease_v2) {
            Ok(lease) => lease,
            Err(e) => {
                self.resource_fault = Some(e);
                return;
            }
        };
        let Ok(validated) =
            processor.validate_transaction_in_utxo_context(tx, view, prefix.accepting.daa_score, TxValidationFlags::Full, filter)
        else {
            return;
        };
        if verify_current_carrier(&validated, view).is_err() || validated.calculated_fee != ordinary.calculated_fee {
            return;
        }
        let session = match self.session {
            Some(s) => s,
            None => match NEXT_SESSION.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1)) {
                Ok(n) => {
                    let s = Session(n);
                    self.session = Some(s);
                    s
                }
                Err(_) => {
                    self.resource_fault = Some(NativeCoverageV1::overflow::<Self>("positive processing session"));
                    return;
                }
            },
        };
        if self.carriers.try_reserve_exact(1).is_err() {
            self.resource_fault =
                Some(NativeCoverageV1::unsupported::<Self>("positive carrier provenance", "companion allocation refused"));
            return;
        }
        self.carriers.push(OwnedPositiveCarrierV2 {
            session,
            body: body.clone(),
            entries: validated.entries,
            fee: validated.calculated_fee,
            relay_carrier_fees: None,
            prefix,
            carrier_block: actual_carrier_block,
            index_within_block: index,
            _lease: Some(lease),
        });
    }
    pub(in crate::pipeline::virtual_processor) fn take_resource_fault(&mut self) -> Option<NativeCoverageV1> {
        self.resource_fault.take()
    }
    /// Capture the actual UTXO fee partition for the sealed carrier. The
    /// ordinary tip is the same amount the fee sink adds to the block.
    pub(super) fn record_relay_carrier_fees_v2(
        &mut self,
        tx: TransactionId,
        mandatory_burn: u64,
        before_relay: u64,
        credited_tip: u64,
    ) {
        let carrier = self.carriers.last_mut().filter(|carrier| carrier.transaction().id() == tx);
        if let Some(carrier) = carrier {
            if carrier.relay_carrier_fees.is_none()
                && let Ok(fees) = FundingRelayCarrierFeesV2::new(carrier.fee, mandatory_burn, before_relay, credited_tip)
            {
                carrier.relay_carrier_fees = Some(fees);
                return;
            }
        } else if credited_tip == before_relay {
            return;
        }
        self.resource_fault.get_or_insert_with(|| {
            NativeCoverageV1::unsupported::<Self>("relay carrier fees", "duplicate, invalid or unsealed native fee partition")
        });
    }

    pub(in crate::pipeline::virtual_processor) fn into_phase_two(
        self,
        acceptance: &AcceptanceData,
        accepted_ids: &[TransactionId],
        actual_point: PalwBlockContextV2,
        selected_parent: BlockHash,
    ) -> Result<PositiveCarrierPhaseTwoV2, NativeContextErrorV2> {
        let mut previous_ordinal = None;
        for c in &self.carriers {
            let tx = c.body.get(c.index_within_block as usize).ok_or(NativeContextErrorV2::Origin)?;
            let ordinal = usize::try_from(c.prefix.accepted_transactions).map_err(|_| NativeContextErrorV2::Prefix)?;
            if previous_ordinal.is_some_and(|previous| ordinal <= previous)
                || self.session != Some(c.session)
                || c.prefix.accepting != actual_point
                || c.prefix.selected_parent != selected_parent
                || accepted_ids.get(ordinal) != Some(&tx.id())
                || ordinal.checked_sub(1).and_then(|i| accepted_ids.get(i)).copied() != c.prefix.last_accepted_transaction
                || !acceptance.iter().any(|block| {
                    block.block_hash == c.carrier_block
                        && block
                            .accepted_transactions
                            .iter()
                            .any(|entry| entry.index_within_block == c.index_within_block && entry.transaction_id == tx.id())
                })
            {
                return Err(NativeContextErrorV2::Origin);
            }
            previous_ordinal = Some(ordinal);
        }
        Ok(PositiveCarrierPhaseTwoV2 { carriers: self.carriers })
    }
}
/// Held across the second-phase PALW walk. Its consumer is deliberately absent
/// until the native notice/role/funding/profile/checkpoint ports are assigned.
/// Dropping this transport neither admits a Court nor rolls back ordinary txs.
pub(in crate::pipeline::virtual_processor) struct PositiveCarrierPhaseTwoV2 {
    carriers: Vec<OwnedPositiveCarrierV2>,
}
impl PositiveCarrierPhaseTwoV2 {
    pub(in crate::pipeline::virtual_processor) fn carriers(&self) -> &[OwnedPositiveCarrierV2] {
        &self.carriers
    }
}
fn verify_current_carrier(validated: &ValidatedTransaction<'_>, view: &impl UtxoView) -> Result<(), NativeContextErrorV2> {
    let tx = validated.tx;
    if tx.is_coinbase() {
        return Err(NativeContextErrorV2::Coinbase);
    }
    if !tx.id_is_current() {
        return Err(NativeContextErrorV2::StaleTransactionId);
    }
    if tx.inputs.len() != validated.entries.len() {
        return Err(NativeContextErrorV2::InputCount);
    }
    let mut input = 0u64;
    for (index, (i, entry)) in tx.inputs.iter().zip(&validated.entries).enumerate() {
        if tx.inputs[..index].iter().any(|prior| prior.previous_outpoint == i.previous_outpoint) {
            return Err(NativeContextErrorV2::DuplicateInput);
        }
        let actual = view.get(&i.previous_outpoint).ok_or(NativeContextErrorV2::MissingInput)?;
        if actual != *entry {
            return Err(NativeContextErrorV2::ChangedInput);
        }
        input = input.checked_add(entry.amount).ok_or(NativeContextErrorV2::Amount)?;
    }
    let mut output = 0u64;
    for (i, value) in tx.outputs.iter().enumerate() {
        let index = u32::try_from(i).map_err(|_| NativeContextErrorV2::Amount)?;
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
        acceptance_data::{AcceptedTxEntry, MergesetBlockAcceptanceData},
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{ScriptPublicKey, ScriptVec, TransactionInput},
    };
    use std::collections::HashMap;
    struct View(HashMap<TransactionOutpoint, UtxoEntry>);
    impl UtxoView for View {
        fn get(&self, key: &TransactionOutpoint) -> Option<UtxoEntry> {
            self.0.get(key).cloned()
        }
    }
    fn fixture() -> (Transaction, View, UtxoEntry) {
        let outpoint = TransactionOutpoint::new(Hash64::from_u64_word(1), 0);
        let script = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51]));
        let entry = UtxoEntry::new(100, script.clone(), 7, false);
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(outpoint, vec![], u64::MAX, 0)],
            vec![TransactionOutput::new(90, script)],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        (tx, View(HashMap::from([(outpoint, entry.clone())])), entry)
    }
    #[test]
    fn unarmed_reservation_is_missing_capacity_not_permission_to_capture() {
        // reserve_native_requests_v1 returns Ok(None) when both checkers are
        // unarmed. Exercise that exact result without changing process-global
        // OnceLocks or fabricating a successful native ticket.
        let reservation: Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> = Ok(None);
        let refusal = reservation.and_then(require_positive_carrier_lease_v2);
        assert!(matches!(
            refusal,
            Err(NativeCoverageV1::Unsupported { component, reason, .. })
                if component == "positive carrier provenance" && reason == "required native capacity lease is not armed"
        ));
        let fault = NativeCoverageV1::overflow::<PositiveCarrierCompanionsV2>("native reservation");
        let refused: Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> = Err(fault.clone());
        assert_eq!(refused.and_then(require_positive_carrier_lease_v2).err(), Some(fault));
    }
    #[test]
    fn relay_debit_requires_one_sealed_fee_observation() {
        let (tx, _, entry) = fixture();
        let id = tx.id();
        let mut companions = PositiveCarrierCompanionsV2::default();
        companions.record_relay_carrier_fees_v2(id, 2, 7, 7);
        assert!(companions.take_resource_fault().is_none());
        companions.record_relay_carrier_fees_v2(id, 2, 7, 4);
        assert!(companions.take_resource_fault().is_some());

        companions.carriers.push(OwnedPositiveCarrierV2 {
            session: Session(1),
            body: vec![tx].into(),
            entries: vec![entry],
            fee: 10,
            relay_carrier_fees: None,
            prefix: NativePrefixStampV2 {
                accepting: PalwBlockContextV2 { block: Hash64::from_u64_word(3), daa_score: 8, blue_score: 9, subsidy: 0 },
                selected_parent: Hash64::from_u64_word(2),
                accepted_transactions: 1,
                last_accepted_transaction: None,
            },
            carrier_block: Hash64::from_u64_word(4),
            index_within_block: 0,
            _lease: None,
        });
        companions.record_relay_carrier_fees_v2(id, 2, 7, 4);
        assert!(companions.take_resource_fault().is_none());
        companions.record_relay_carrier_fees_v2(id, 2, 7, 3);
        assert!(companions.take_resource_fault().is_some());
        assert_eq!(companions.carriers[0].relay_carrier_fees.unwrap().relay_charge(), 3);
    }
    #[test]
    fn public_validated_dto_cannot_substitute_actual_current_entries_fee_or_outputs() {
        // This checks consistency only; even Ok here does NOT mint a seal.
        let (tx, mut view, entry) = fixture();
        let mut dto = ValidatedTransaction { tx: &tx, entries: vec![entry.clone()], calculated_fee: 10 };
        assert_eq!(verify_current_carrier(&dto, &view), Ok(()));
        dto.calculated_fee = 9;
        assert_eq!(verify_current_carrier(&dto, &view), Err(NativeContextErrorV2::Fee));
        dto.calculated_fee = 10;
        view.0.get_mut(&tx.inputs[0].previous_outpoint).unwrap().amount = 99;
        assert_eq!(verify_current_carrier(&dto, &view), Err(NativeContextErrorV2::ChangedInput));
        view.0.clear();
        assert_eq!(verify_current_carrier(&dto, &view), Err(NativeContextErrorV2::MissingInput));
        view.0.insert(tx.inputs[0].previous_outpoint, entry.clone());
        view.0.insert(TransactionOutpoint::new(tx.id(), 0), entry);
        assert_eq!(verify_current_carrier(&dto, &view), Err(NativeContextErrorV2::ExistingOutput));
    }
    #[test]
    fn duplicate_input_missing_entry_and_stale_transaction_are_refused() {
        let (mut tx, view, entry) = fixture();
        tx.inputs.push(tx.inputs[0].clone());
        tx.finalize();
        let dto = ValidatedTransaction { tx: &tx, entries: vec![entry.clone(), entry.clone()], calculated_fee: 110 };
        assert_eq!(verify_current_carrier(&dto, &view), Err(NativeContextErrorV2::DuplicateInput));
        let dto = ValidatedTransaction { tx: &tx, entries: vec![entry.clone()], calculated_fee: 10 };
        assert_eq!(verify_current_carrier(&dto, &view), Err(NativeContextErrorV2::InputCount));
        tx.payload.push(1);
        let dto = ValidatedTransaction { tx: &tx, entries: vec![entry.clone(), entry], calculated_fee: 110 };
        assert_eq!(verify_current_carrier(&dto, &view), Err(NativeContextErrorV2::StaleTransactionId));
    }
    #[test]
    fn transport_rechecks_session_exact_acceptance_origin_ordinal_and_accepting_point() {
        // Test-only synthetic transport. No call to capture and no assertion
        // of Full scripts or native Court standing is made by this fixture.
        for mode in 0..8 {
            let (tx, _, entry) = fixture();
            let id = tx.id();
            let point = PalwBlockContextV2 { block: Hash64::from_u64_word(3), daa_score: 8, blue_score: 9, subsidy: 0 };
            let parent = Hash64::from_u64_word(2);
            let carrier = Hash64::from_u64_word(4);
            let coinbase = Hash64::from_u64_word(5);
            let mut c = OwnedPositiveCarrierV2 {
                session: Session(1),
                body: vec![tx].into(),
                entries: vec![entry],
                fee: 10,
                relay_carrier_fees: None,
                prefix: NativePrefixStampV2 {
                    accepting: point,
                    selected_parent: parent,
                    accepted_transactions: 1,
                    last_accepted_transaction: Some(coinbase),
                },
                carrier_block: carrier,
                index_within_block: 0,
                _lease: None,
            };
            match mode {
                1 => c.session = Session(2),
                2 => c.prefix.accepting.daa_score += 1,
                3 => c.prefix.selected_parent = carrier,
                4 => c.prefix.accepted_transactions = 0,
                5 => c.prefix.last_accepted_transaction = None,
                6 => c.carrier_block = parent,
                7 => c.index_within_block = 1,
                _ => {}
            }
            let companions = PositiveCarrierCompanionsV2 { session: Some(Session(1)), carriers: vec![c], resource_fault: None };
            let accepted = vec![MergesetBlockAcceptanceData {
                block_hash: carrier,
                accepted_transactions: vec![AcceptedTxEntry { transaction_id: id, index_within_block: 0 }],
            }];
            let result = companions.into_phase_two(&accepted, &[coinbase, id], point, parent);
            if mode == 0 {
                let handoff = result.unwrap();
                assert_eq!(handoff.carriers().len(), 1);
            } else {
                assert!(matches!(result, Err(NativeContextErrorV2::Origin)), "mode {mode}");
            }
        }
    }
}

/// Explicit diagnosis at the actual accepted-carrier boundary. This is neither
/// an accepted Assignment nor a saved pass; no status can authorize a write.
#[derive(Debug)]
pub(in crate::pipeline::virtual_processor) enum AssignmentIngressPrerequisiteV2 {
    InactiveAssignmentPolicy,
    MissingNativePreimages,
    InvalidNativePreimages,
    WrongNativeRuleScope,
    SignedAnchorNotAncestor,
    MissingActualCertificationPolicy,
    PendingFraudReservationOwner,
    NativeAssignmentCheckpointOwner,
}
impl VirtualStateProcessor {
    /// Check the archived registration behind an exact repeat against the
    /// actual selected-parent state and its persisted acceptance index.
    /// The final fold still owns the original custody/replay decision.
    fn check_exact_relay_archive_v2(
        &self,
        parent: BlockHash,
        now: u64,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        body: &misaka_palw_positive_contract::assignment::funding_relay::FundingRelayRegistrationBodyV2,
        payload: &[u8],
    ) -> Result<Option<kaspa_consensus_core::palw_state_v2::NativePendingRelayReplayV2>, NativeCoverageV1> {
        use kaspa_consensus_core::palw_positive_rows_v2::{PositiveRowErrorV2, PositiveRowsV2};
        use misaka_palw_positive_contract::{native::NativeRowKey, rows::NativeRowBody};

        let fault = |why| NativeCoverageV1::unsupported::<Self>("rooted relay archive", why);
        let workspace = pending_funding_relay_archive_workspace_v2();
        let _lease = require_positive_carrier_lease_v2(reserve_native_requests_v1(workspace)?)?;
        let funding = body.terms.funding_outpoint;
        let purpose = body.terms.funding_purpose;
        let rows = PositiveRowsV2::new(state);
        let row = match rows.read(NativeRowKey::FundingRelayAuthorization { funding, purpose }) {
            Ok(Some(row)) => row,
            Ok(None) | Err(PositiveRowErrorV2::MissingKernelRoute) => return Ok(None),
            Err(_) => return Err(fault("malformed rooted relay authorization")),
        };
        let NativeRowBody::FundingRelayAuthorization(auth) = row.body else {
            return Err(fault("unexpected rooted relay authorization kind"));
        };
        if auth.terms != body.terms || auth.purpose_signature != body.purpose_signature {
            return Ok(None); // Core deterministically refuses the different package.
        }
        let Some(NativeRowBody::FundingRelayCustody(custody)) = rows
            .read(NativeRowKey::FundingRelayCustody { funding, purpose })
            .map_err(|_| fault("malformed rooted relay custody"))?
            .map(|row| row.body)
        else {
            return Err(fault("missing rooted relay custody"));
        };
        if custody.relay_signature != body.relay_signature {
            return Ok(None); // A different literal package cannot use the archive.
        }
        custody.check_pair(&auth).map_err(|_| fault("rooted relay rows disagree"))?;
        if custody.binding.is_some() || now >= custody.retention_until_daa {
            return Ok(None);
        }
        self.verify_pending_relay_archive_v2(parent, state, &auth, &custody, body, Some(payload), workspace).map(Some)
    }

    /// Initial F must use a registration rooted in its selected parent's
    /// state. Rebuild the fixed literal package from that row pair and compare
    /// it with the archive before handing off any replay data.
    fn check_initial_f_relay_archive_v2(
        &self,
        parent: BlockHash,
        now: u64,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        purpose: misaka_palw_positive_contract::assignment::funding_relay::RelayFundingPurposeV2,
        funding: misaka_palw_positive_contract::rows::NativeOutpoint,
        matches_terms: impl FnOnce(&misaka_palw_positive_contract::assignment::funding_relay::FundingRelayTermsV2) -> bool,
    ) -> Result<Option<kaspa_consensus_core::palw_state_v2::NativePendingRelayReplayV2>, NativeCoverageV1> {
        use kaspa_consensus_core::palw_positive_rows_v2::{PositiveRowErrorV2, PositiveRowsV2};
        use misaka_palw_positive_contract::{
            assignment::funding_relay::{FUNDING_RELAY_OPERATION_V2, FundingRelayRegistrationBodyV2},
            native::NativeRowKey,
            rows::NativeRowBody,
        };
        let fault = |why| NativeCoverageV1::unsupported::<Self>("initial funding relay archive", why);
        let workspace = pending_funding_relay_archive_workspace_v2();
        let _lease = require_positive_carrier_lease_v2(reserve_native_requests_v1(workspace)?)?;
        let rows = PositiveRowsV2::new(state);
        let row = match rows.read(NativeRowKey::FundingRelayAuthorization { funding, purpose }) {
            Ok(Some(row)) => row,
            Ok(None) | Err(PositiveRowErrorV2::MissingKernelRoute) => return Ok(None),
            Err(_) => return Err(fault("malformed rooted relay authorization")),
        };
        let NativeRowBody::FundingRelayAuthorization(auth) = row.body else {
            return Err(fault("unexpected rooted relay authorization kind"));
        };
        if !matches_terms(&auth.terms) {
            return Ok(None);
        }
        let Some(NativeRowBody::FundingRelayCustody(custody)) = rows
            .read(NativeRowKey::FundingRelayCustody { funding, purpose })
            .map_err(|_| fault("malformed rooted relay custody"))?
            .map(|row| row.body)
        else {
            return Err(fault("missing rooted relay custody"));
        };
        custody.check_pair(&auth).map_err(|_| fault("rooted relay rows disagree"))?;
        if custody.binding.is_some() || now >= custody.retention_until_daa {
            return Ok(None);
        }
        let body = FundingRelayRegistrationBodyV2 {
            version: misaka_palw_positive_contract::CONTRACT_VERSION,
            revision: misaka_palw_positive_contract::CONTRACT_REVISION,
            operation: FUNDING_RELAY_OPERATION_V2,
            terms: auth.terms,
            purpose_signature: auth.purpose_signature,
            relay_signature: custody.relay_signature,
        };
        self.verify_pending_relay_archive_v2(parent, state, &auth, &custody, &body, None, workspace).map(Some)
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_pending_relay_archive_v2(
        &self,
        parent: BlockHash,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        auth: &misaka_palw_positive_contract::assignment::funding_relay_rows::FundingRelayAuthorizationRowV2,
        custody: &misaka_palw_positive_contract::assignment::funding_relay_rows::FundingRelayCustodyRowV2,
        body: &misaka_palw_positive_contract::assignment::funding_relay::FundingRelayRegistrationBodyV2,
        exact_retry_payload: Option<&[u8]>,
        workspace: u64,
    ) -> Result<kaspa_consensus_core::palw_state_v2::NativePendingRelayReplayV2, NativeCoverageV1> {
        use crate::model::stores::ghostdag::GhostdagStoreReader;
        use kaspa_consensus_core::{
            config::params::AssignmentFundingRelayProfileV2,
            palw_positive_funding_relay_v2::{
                CASE_RELAY_OPEN_LOOKUP_READS_V2, FundingRelayBodyErrorV2, RELAY_ARCHIVE_LOOKUP_READS_V2,
                funding_relay_carrier_matches_v2, verify_funding_relay_package_signatures_v2,
            },
        };
        let fault = |why| NativeCoverageV1::unsupported::<Self>("rooted relay archive", why);
        use misaka_palw_positive_contract::assignment::funding_relay::RelayTargetV2;
        let (case_contract_material, case_contract_work) = if auth.terms.funding_purpose
            == misaka_palw_positive_contract::assignment::funding_relay::RelayFundingPurposeV2::CaseC
        {
            let (material, work) = self.case_relay_original_contract_v2(parent, state, &auth.terms, &auth.physical, workspace)
                .map_err(|why| match why {
                    crate::pipeline::virtual_processor::processor::positive_assignment_archive::NativeAssignmentArchiveErrorV2::Resource(why) => why,
                    why => NativeCoverageV1::unsupported::<Self>("rooted relay archive", &format!("{why:?}")),
                })?
                .ok_or_else(|| fault("original Case relay profile unavailable"))?;
            (Some(material), work)
        } else {
            (None, 0)
        };
        // Two rooted relay rows, two reachability checks, header, selected
        // parent, acceptance index and material keys. C resolves its Open.
        let case_reads = if case_contract_material.is_some() { CASE_RELAY_OPEN_LOOKUP_READS_V2 } else { 0 };
        let fixed_reads = RELAY_ARCHIVE_LOOKUP_READS_V2 + case_reads;
        if auth.quote.usage.replay_reads < fixed_reads || auth.quote.usage.heap_bytes < workspace {
            return Err(fault("original relay replay limits do not cover archive lookup"));
        }
        let payload_bytes = auth.quote.usage.frame_bytes.checked_add(3).ok_or_else(|| fault("relay frame extent overflow"))?;
        let matches_payload = |raw: &[u8]| {
            raw.len() as u64 == payload_bytes
                && exact_retry_payload.map_or_else(|| funding_relay_carrier_matches_v2(body, raw), |exact| raw == exact)
        };
        // F compares two canonical encodings against the borrowed archive and
        // the historical transaction without retaining an extra payload Vec.
        let comparison_work = if exact_retry_payload.is_some() {
            0
        } else {
            payload_bytes.checked_mul(2).ok_or_else(|| fault("relay comparison work overflow"))?
        };
        let auth_bytes = borsh::object_length(auth).map_err(|_| fault("original relay authorization extent"))? as u64;
        let custody_bytes = borsh::object_length(custody).map_err(|_| fault("original relay custody extent"))? as u64;
        // Both Borsh row envelopes (version, revision, kind) and aux keys.
        let fixed_work = auth_bytes
            .checked_add(custody_bytes)
            .and_then(|n| n.checked_add(payload_bytes))
            .and_then(|n| n.checked_add((FUNDING_RELAY_CHARGE_BYTES_V2 + 2 * (74 + 5)) as u64))
            .and_then(|n| n.checked_add(comparison_work))
            .and_then(|n| n.checked_add(case_contract_work))
            .ok_or_else(|| fault("original relay fixed replay extent overflow"))?;
        let maximum_bytes = auth
            .quote
            .usage
            .replay_work
            .checked_sub(fixed_work)
            .ok_or_else(|| fault("original relay replay budget cannot cover literal records"))?;

        let origin = auth.origin;
        let accepting = BlockHash::from_bytes(origin.accepting_block.0);
        if origin.event_ordinal != 0
            || !self
                .reachability_service
                .try_is_chain_ancestor_of(accepting, parent)
                .map_err(|_| fault("original relay reachability unavailable"))?
        {
            return Err(fault("original relay origin is outside selected-parent lineage"));
        }
        let header = self.headers_store.get_header(accepting).map_err(|_| fault("original relay accepting header unavailable"))?;
        if header.daa_score != origin.accepted_daa || header.blue_score != origin.accepted_blue_score {
            return Err(fault("original relay accepting header clock differs"));
        }
        let original_parent =
            self.ghostdag_store.get_selected_parent(accepting).map_err(|_| fault("original relay selected parent unavailable"))?;
        let anchor = BlockHash::from_bytes(auth.terms.contract_anchor.0);
        if !self
            .reachability_service
            .try_is_chain_ancestor_of(anchor, original_parent)
            .map_err(|_| fault("original relay signed anchor reachability unavailable"))?
        {
            return Err(fault("original relay signed anchor is outside the original selected chain"));
        }

        let params = self.palw_positive_assignment_params_v2.as_ref().ok_or_else(|| fault("original relay parameters unavailable"))?;
        let assignment = params
            .palw_positive_assignment_rule_scope_v2_at(origin.accepted_daa)
            .ok_or_else(|| fault("original Assignment intake scope unavailable"))?;
        let contract_scope_matches = match auth.terms.target {
            misaka_palw_positive_contract::assignment::funding_relay::RelayTargetV2::InitialAssignment { .. } => {
                auth.terms.contract_ruleset.0 == assignment.ruleset().as_bytes()
            }
            // C's exact original contract is checked at R's actual accepted
            // prefix in the payer replay. Its registration uses R's own policy.
            RelayTargetV2::InitialCase { .. } => true,
            RelayTargetV2::Continuation { .. } => {
                auth.terms.funding_purpose == misaka_palw_positive_contract::assignment::funding_relay::RelayFundingPurposeV2::CaseC
            }
            _ => false,
        };
        let profile = AssignmentFundingRelayProfileV2::capture_at_intake(params, origin.accepted_daa)
            .map_err(|_| fault("original relay intake profile unavailable"))?;
        let AssignmentFundingRelayProfileV2::RequiresRelayV2 { ruleset, policy, fees, physical, .. } = profile else {
            return Err(fault("original relay intake fence was dormant"));
        };
        if auth.terms.contract_network.0 != self.palw_network_domain_v2().as_bytes()
            || !contract_scope_matches
            || auth.terms.relay_ruleset != ruleset
            || auth.policy != policy
            || auth.fees != fees
            || auth.physical != physical
            || fees.registration_quote(&policy, &physical, &auth.terms, origin.accepted_daa).ok() != Some(auth.quote)
        {
            return Err(fault("archived relay scope differs from original fired rules"));
        }

        let (carrier_block, accepted, acceptance_bytes) = self
            .acceptance_data_store
            .accepted_origin_at_bounded_v2(accepting, origin.accepted_tx_ordinal, maximum_bytes)
            .map_err(|_| fault("bounded original relay acceptance unavailable"))?
            .ok_or_else(|| fault("original relay acceptance ordinal missing"))?;
        if accepted.index_within_block == 0
            || carrier_block.as_bytes() != origin.carrier_block.0
            || accepted.transaction_id.as_bytes() != origin.carrier_transaction.0
        {
            return Err(fault("original relay accepted carrier differs"));
        }
        let observed_fees = self
            .palw_state_v2_store
            .read()
            .check_funding_relay_archive_v2(auth, matches_payload)
            .map_err(|_| fault("rooted relay payload or charge unavailable"))?;
        use crate::pipeline::virtual_processor::processor::positive_assignment_archive::{
            NativeAssignmentArchiveErrorV2, RelayCarrierReplayV2,
        };
        self.replay_original_relay_carrier_v2(RelayCarrierReplayV2 {
            origin,
            index: accepted.index_within_block,
            matches_payload: &matches_payload,
            gross_fee: observed_fees.gross_fee(),
            remaining_reads: auth.quote.usage.replay_reads - fixed_reads,
            remaining_work: maximum_bytes - acceptance_bytes,
            maximum_heap: auth.quote.usage.heap_bytes - workspace,
            acceptance_bytes,
            selected_parent: parent,
            state,
            authorization: auth,
            custody,
            case_contract_material,
        })
        .map_err(|error| match error {
            NativeAssignmentArchiveErrorV2::Resource(why) => why,
            why => NativeCoverageV1::unsupported::<Self>("rooted relay archive", &format!("{why:?}")),
        })?;
        verify_funding_relay_package_signatures_v2(body, &auth.payer_key, auth.quote.usage.heap_bytes - workspace).map_err(|why| {
            match why {
                FundingRelayBodyErrorV2::Contract(_) => fault("original relay purpose or settlement signature invalid"),
                FundingRelayBodyErrorV2::Native(why) => why,
            }
        })?;
        Ok(kaspa_consensus_core::palw_state_v2::NativePendingRelayReplayV2 {
            selected_parent: parent,
            reservation: custody.reservation,
            fees: observed_fees,
            physical: auth.physical,
        })
    }
}

impl PositiveCarrierPhaseTwoV2 {
    /// The old0--7 codec is not widened. New operations use their own native
    /// finite policy, scan before any inner allocation, acquire a real lease,
    /// and retain original transaction order. Semantic rejection leaves the
    /// ordinary native transaction effects alone; local capacity retries.
    pub(in crate::pipeline::virtual_processor) fn inspect_assignment_ingress_v2(
        &self,
        processor: &VirtualStateProcessor,
    ) -> Result<(), NativeCoverageV1> {
        use kaspa_consensus_core::palw_positive_court_v2::coordinator::assignment_body::{
            AssignmentBodyLimitsV2, PositiveAssignmentNativeOperationV2, decode_assignment_body_v2, scan_assignment_body_v2,
        };
        for carrier in &self.carriers {
            let tx = carrier.transaction();
            if !kaspa_consensus_core::palw_lifecycle_objects_v2::palw_positive_lifecycle_payload_v2(&tx.payload)
                || !matches!(tx.payload.get(7), Some(8 | 9 | 12 | 13))
            {
                continue;
            }
            let point = carrier.prefix.accepting;
            let Some(params) = processor.palw_positive_assignment_params_v2.as_ref() else {
                continue;
            };
            let Some(fence) = params.palw_positive_assignment_v2_at(point.daa_score) else {
                continue;
            };
            let Ok(limits) = AssignmentBodyLimitsV2::from_policy(&fence.policy) else {
                continue;
            };
            let body = &tx.payload[3..];
            let Ok(summary) = scan_assignment_body_v2(body, limits) else {
                continue;
            };
            let _decode_lease = require_positive_carrier_lease_v2(reserve_native_requests_v1(summary.heap)?)?;
            let Ok(decoded) = decode_assignment_body_v2(body, limits) else {
                continue;
            };
            let prerequisite: Result<(), AssignmentIngressPrerequisiteV2> = (|| {
                let preimages = fence.native_preimages.as_ref().ok_or(AssignmentIngressPrerequisiteV2::MissingNativePreimages)?;
                preimages.validate(&fence.policy).map_err(|_| AssignmentIngressPrerequisiteV2::InvalidNativePreimages)?;
                if let PositiveAssignmentNativeOperationV2::AcceptCanonicalAssignment { bundle, .. } = &decoded.operation {
                    let scope = params
                        .palw_positive_assignment_rule_scope_v2_at(point.daa_score)
                        .ok_or(AssignmentIngressPrerequisiteV2::InactiveAssignmentPolicy)?;
                    if bundle.terms.ruleset.0 != scope.ruleset().as_bytes() {
                        return Err(AssignmentIngressPrerequisiteV2::WrongNativeRuleScope);
                    }
                    let anchor = BlockHash::from_bytes(bundle.terms.branch_anchor.0);
                    if !processor
                        .reachability_service
                        .try_is_chain_ancestor_of(anchor, carrier.prefix.selected_parent)
                        .unwrap_or(false)
                    {
                        return Err(AssignmentIngressPrerequisiteV2::SignedAnchorNotAncestor);
                    }
                    let clock = kaspa_consensus_core::palw_positive_assignment_certification_v2::AssignmentGateCClockV2::resolve(
                        params,
                        point.daa_score,
                        // Conservatively quote every permitted later Result
                        // acceptance clock, never use Assignment intake as P's
                        // commit. This still grants no native reservation.
                        bundle.terms.result_before_daa,
                    )
                    .map_err(|_| AssignmentIngressPrerequisiteV2::MissingActualCertificationPolicy)?;
                    match clock.require_complete_native_bound() {
                        Err(_) => return Err(AssignmentIngressPrerequisiteV2::PendingFraudReservationOwner),
                        Ok(impossible) => match impossible {},
                    }
                }
                Err(AssignmentIngressPrerequisiteV2::NativeAssignmentCheckpointOwner)
            })();
            if let Err(why) = prerequisite {
                kaspa_core::info!(
                    "[palw-assignment] native carrier {} at accepted ordinal {} op{} not admitted: {:?}",
                    tx.id(),
                    carrier.prefix.accepted_transactions,
                    summary.kind,
                    why
                );
            }
        }
        Ok(())
    }
}

impl PositiveCarrierPhaseTwoV2 {
    /// One actual Full transport and ordinal stream for unchanged original0–7
    /// and new Assignment8–13. Their codec/signature limits stay independent.
    pub(in crate::pipeline::virtual_processor) fn into_assignment_raw_actions_v2(
        self,
        processor: &VirtualStateProcessor,
        current_view: &impl UtxoView,
        selected_parent_state: Option<&kaspa_consensus_core::palw_state_v2::PalwChainStateV2>,
    ) -> Result<Vec<kaspa_consensus_core::palw_state_v2::PositiveAssignmentRawActionV2>, NativeCoverageV1> {
        use crate::pipeline::virtual_processor::processor::positive_assignment_archive::NativeAssignmentArchiveErrorV2 as E;
        use kaspa_consensus_core::palw_positive_court_v2::coordinator::{
            assignment_body::{
                PositiveAssignmentNativeOperationV2 as A, decode_assignment_body_v2, scan_assignment_body_v2,
                scan_assignment_continuation_id_v2,
            },
            native_body::{
                MAX_BODY_BYTES_V2, MAX_EXACT_RELATIONS_V2, MAX_OPAQUE_BYTES_V2, MAX_SIGNATURES_V2, NativeBodyLimitsV2,
                PositiveNativeBodyV2, decode_native_body_v2, scan_native_body_v2,
            },
        };
        use kaspa_consensus_core::palw_state_v2::{
            NativeAssignmentRulesV2, PositiveAssignmentRawActionV2, PositiveAssignmentScheduledActionV2,
        };
        use kaspa_consensus_core::{
            config::params::AssignmentFundingRelayProfileV2, palw_positive_funding_relay_v2::quote_funding_relay_body_v2,
        };
        use misaka_palw_positive_contract::assignment::funding_relay::{
            FundingRelayRegistrationBodyV2, RelayFundingPurposeV2, RelayTargetV2,
        };
        use misaka_palw_positive_contract::rows::{AcceptedOrigin, NativeHash64, NativeOutpoint};
        let old_limits = NativeBodyLimitsV2 {
            max_body_bytes: MAX_BODY_BYTES_V2,
            max_opaque_bytes: MAX_OPAQUE_BYTES_V2,
            max_exact_relations: MAX_EXACT_RELATIONS_V2,
            max_signatures: MAX_SIGNATURES_V2,
        };
        let mut unforwarded_debits =
            self.carriers.iter().filter(|carrier| carrier.relay_carrier_fees.is_some_and(|fees| fees.relay_charge() != 0)).count();
        let mut actions = Vec::new();
        for carrier in self.carriers {
            let tx = carrier.transaction();
            if tx.payload.get(..3) != Some(&[1, 0, 176]) {
                continue;
            }
            let Some(kind @ 0..=14) = tx.payload.get(7).copied() else {
                continue;
            };
            let point = carrier.prefix.accepting;
            let Some(params) = processor.palw_positive_assignment_params_v2.as_ref() else { continue };
            // Keep the existing native interleave fence. Continuation rules
            // inside this transport use original material, including relay
            // profile changes while this enclosing lane stays active.
            if params.palw_positive_assignment_v2_at(point.daa_score).is_none() {
                continue;
            }
            let original = if (9..=13).contains(&kind) {
                let body = &tx.payload[3..];
                let Ok(id) = scan_assignment_continuation_id_v2(body) else { continue };
                if let Some(state) = selected_parent_state {
                    match processor.original_assignment_rules_v2(carrier.prefix.selected_parent, state, id, Some(body)) {
                        Ok(rules) => rules,
                        Err(E::InvalidContinuation) => continue,
                        Err(E::Resource(why)) => return Err(why),
                        Err(why) => {
                            return Err(NativeCoverageV1::unsupported::<Self>(
                                "original Assignment continuation rules",
                                &format!("{why:?}"),
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let (original, open_carriage) = match original {
                Some((rules, carriage, material)) => (Some(rules), (kind == 9).then_some((carriage, material))),
                None => (None, None),
            };
            let Some(rules) = original.or_else(|| {
                NativeAssignmentRulesV2::resolve(
                    params,
                    point.daa_score,
                    processor.palw_network_domain_v2(),
                    processor.palw_prompt_ids_form_at(point.daa_score),
                )
            }) else {
                continue;
            };
            let rules = if kind == 13 { rules.with_current_service_capacity_v2(params, point.daa_score) } else { rules };
            let relay_cap = if kind == 14 {
                let AssignmentFundingRelayProfileV2::RequiresRelayV2 { physical, .. } = rules.funding_relay_profile() else {
                    continue;
                };
                Some(*physical)
            } else {
                None
            };
            let Ok(limits) = rules.body_limits() else { continue };
            let heap = if let Some(physical) = relay_cap {
                let Ok(quote) = quote_funding_relay_body_v2(&tx.payload[3..], physical.max_frame_bytes) else { continue };
                Some(quote.logical_decode_requests)
            } else if kind <= 7 {
                let Ok(summary) = scan_native_body_v2(&tx.payload[3..], old_limits) else {
                    continue;
                };
                summary.opaque_bytes.checked_mul(2).and_then(|n| n.checked_add(std::mem::size_of::<PositiveNativeBodyV2>() as u64))
            } else {
                match scan_assignment_body_v2(&tx.payload[3..], limits) {
                    Ok(s) => Some(s.heap),
                    Err(_) => continue,
                }
            };
            let heap = heap.ok_or_else(|| NativeCoverageV1::overflow::<Self>("original/assigned body decode"))?;
            let output_script = tx.outputs.iter().map(|o| o.script_public_key.script().len() as u64).max().unwrap_or(0);
            let typed = std::mem::size_of::<PositiveAssignmentRawActionV2>() as u64
                + 2 * std::mem::size_of::<PositiveAssignmentScheduledActionV2>() as u64
                + std::mem::size_of::<NativeAssignmentRulesV2>() as u64
                + std::mem::size_of::<UtxoEntry>() as u64
                + PositiveAssignmentRawActionV2::retained_owner_bytes_v2();
            let bytes = heap
                .checked_add(typed)
                .and_then(|n| n.checked_add(output_script))
                .ok_or_else(|| NativeCoverageV1::overflow::<Self>("native Positive raw retained action"))?;
            let decode = require_positive_carrier_lease_v2(reserve_native_requests_v1(bytes)?)?;
            let mut signed_anchor = None;
            let mut registered_class = None;
            let mut pending_relay = None;
            let mut case_relay_contract = None;
            let mut case_backing = None;
            let mut initial_a = None;
            let mut initial_c = None;
            let output = if let Some(physical) = relay_cap {
                let Ok(body) = FundingRelayRegistrationBodyV2::decode(&tx.payload[3..], physical.max_frame_bytes) else { continue };
                if !matches!(body.terms.target, RelayTargetV2::InitialAssignment { .. } | RelayTargetV2::InitialCase { .. }) {
                    continue; // Other purposes need their own original account owner.
                }
                if let Some(state) = selected_parent_state {
                    pending_relay = processor.check_exact_relay_archive_v2(
                        carrier.prefix.selected_parent,
                        point.daa_score,
                        state,
                        &body,
                        &tx.payload,
                    )?;
                }
                let decode_profile = pending_relay.as_ref().map_or(&physical, |archive| &archive.physical);
                if size_of_val(&body) as u64 > decode_profile.max_decoded_bytes || heap > decode_profile.max_heap_bytes {
                    continue;
                }
                if pending_relay.is_none() {
                    let anchor = BlockHash::from_bytes(body.terms.contract_anchor.0);
                    if !processor
                        .reachability_service
                        .try_is_chain_ancestor_of(anchor, carrier.prefix.selected_parent)
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    let daa = processor.headers_store.get_daa_score(anchor).map_err(|_| {
                        NativeCoverageV1::unsupported::<Self>("funding relay signed ancestor", "verified ancestor header unavailable")
                    })?;
                    signed_anchor = Some((anchor, daa));
                    if body.terms.funding_purpose == RelayFundingPurposeV2::CaseC
                        && let Some(state) = selected_parent_state
                    {
                        let original = processor.case_relay_original_contract_v2(
                            carrier.prefix.selected_parent,
                            state,
                            &body.terms,
                            &physical,
                            bytes,
                        ).map_err(|why| match why {
                            E::Resource(why) => why,
                            why => NativeCoverageV1::unsupported::<Self>("Case relay original contract", &format!("{why:?}")),
                        })?;
                        case_relay_contract = original.map(|(material, _)| (body.terms.assignment, material));
                    }
                }
                None
            } else if kind <= 7 {
                let Ok(body) = decode_native_body_v2(&tx.payload[3..], old_limits) else {
                    continue;
                };
                let Ok(subject) = body.operation.case_target_v2() else { continue };
                if let (Some(subject), Some(state)) = (subject, selected_parent_state) {
                    case_backing = processor
                        .prepare_committed_case_history_v2(carrier.prefix.selected_parent, state, subject)
                        .map_err(|why| match why {
                            E::Resource(why) => why,
                            _ => NativeCoverageV1::unsupported::<Self>("committed Case predecessor", &format!("{why:?}")),
                        })?;
                }
                None
            } else {
                let Ok(body) = decode_assignment_body_v2(&tx.payload[3..], limits) else {
                    continue;
                };
                match &body.operation {
                    A::AcceptCanonicalAssignment { bundle, .. } => {
                        let policy = rules.assignment_policy();
                        if bundle.terms.network != rules.network()
                            || bundle.terms.ruleset.0 != rules.rule_scope().ruleset().as_bytes()
                            || policy.check_terms(&bundle.terms, point.daa_score).is_err()
                        {
                            continue;
                        }
                        let anchor = BlockHash::from_bytes(bundle.terms.branch_anchor.0);
                        if !processor
                            .reachability_service
                            .try_is_chain_ancestor_of(anchor, carrier.prefix.selected_parent)
                            .unwrap_or(false)
                        {
                            continue;
                        }
                        let anchor_daa = processor.headers_store.get_daa_score(anchor).map_err(|_| {
                            NativeCoverageV1::unsupported::<Self>("Assignment signed ancestor", "verified ancestor header unavailable")
                        })?;
                        signed_anchor = Some((anchor, anchor_daa));
                        if let Some((state, preimages)) = selected_parent_state.zip(rules.native_preimages()) {
                            let heap = policy.max_heap_bytes.min(preimages.physical.max_heap_bytes).saturating_sub(bytes);
                            let work = policy.max_replay_work.min(preimages.physical.max_replay_bytes);
                            registered_class = processor
                                .discover_tir_registration_archive_v2(
                                    carrier.prefix.selected_parent,
                                    state,
                                    bundle.terms.registered_class,
                                    heap,
                                    work,
                                )
                                .map_err(|why| match why {
                                    E::Resource(why) => why,
                                    why => NativeCoverageV1::unsupported::<Self>("original TIR registration", &format!("{why:?}")),
                                })?;
                        }
                        if matches!(rules.funding_relay_profile(), AssignmentFundingRelayProfileV2::RequiresRelayV2 { .. }) {
                            initial_a = Some(*bundle);
                        }
                        Some(bundle.funding.funding_output_index)
                    }
                    A::AdmitAssignedResult { assignment, payload } => {
                        if rules.funding_relay_profile().requires_relay() {
                            initial_c = Some((*assignment, payload.result.id(), payload.consent.clone()));
                        }
                        Some(payload.consent.funding_output_index)
                    }
                    A::SettleAssignment { assignment, case: Some(case), .. } => {
                        use kaspa_consensus_core::{
                            palw_positive_assignment_funds_v2::native_assignment_outpoint_v2, palw_positive_rows_v2::PositiveRowsV2,
                        };
                        use misaka_palw_positive_contract::{native::NativeRowKey, rows::NativeRowBody};
                        if let (Some(state), Some(input)) = (selected_parent_state, tx.inputs.first()) {
                            let rows = PositiveRowsV2::new(state);
                            let funding = |key| {
                                rows.read(key).map(|row| row.map(|row| row.body)).map_err(|why| {
                                    NativeCoverageV1::unsupported::<Self>("Case settlement funding row", &format!("{why:?}"))
                                })
                            };
                            let spends = |outpoint| input.previous_outpoint == native_assignment_outpoint_v2(outpoint);
                            let source = matches!(
                                funding(NativeRowKey::Funding(*case))?,
                                Some(NativeRowBody::Funding(row)) if row.current_outpoint.is_some_and(spends)
                            );
                            let worker = !source
                                && matches!(
                                    funding(NativeRowKey::AssignmentFunding(*assignment))?,
                                    Some(NativeRowBody::AssignmentFunding(row)) if row.current_outpoint.is_some_and(spends)
                                );
                            if source || worker {
                                case_backing = processor
                                    .prepare_committed_case_history_v2(carrier.prefix.selected_parent, state, (*case, None))
                                    .map_err(|why| match why {
                                        E::Resource(why) => why,
                                        why => {
                                            NativeCoverageV1::unsupported::<Self>("Case settlement predecessor", &format!("{why:?}"))
                                        }
                                    })?;
                            }
                        }
                        None
                    }
                    A::JoinAssignmentProver { consent, .. } => Some(consent.funding_output_index),
                    _ => None,
                }
            };
            let current_output = output.and_then(|index| current_view.get(&TransactionOutpoint::new(tx.id(), index)));
            if let (Some(index), Some(state), Some(entry)) = (output, selected_parent_state, current_output.as_ref()) {
                let funding = NativeOutpoint { transaction: NativeHash64(tx.id().as_bytes()), index };
                let now = point.daa_score;
                if let Some(bundle) = initial_a {
                    pending_relay = processor.check_initial_f_relay_archive_v2(
                        carrier.prefix.selected_parent,
                        now,
                        state,
                        RelayFundingPurposeV2::PosterA,
                        funding,
                        |terms| {
                            kaspa_consensus_core::palw_positive_assignment_funds_v2::check_initial_assignment_relay_target_v2(
                                terms,
                                &bundle,
                                funding,
                                entry.amount,
                                now,
                            )
                            .is_ok()
                        },
                    )?;
                }
                if let Some((assignment, result, consent)) = initial_c {
                    let target = RelayTargetV2::InitialCase { result, funding_scope: consent.scope, consent: consent.id() };
                    pending_relay = processor.check_initial_f_relay_archive_v2(
                        carrier.prefix.selected_parent,
                        now,
                        state,
                        RelayFundingPurposeV2::CaseC,
                        funding,
                        |terms| {
                            terms.target == target
                                && terms.assignment == assignment
                                && terms.contract_network == rules.network()
                                && terms.contract_ruleset.0 == rules.rule_scope().ruleset().as_bytes()
                                && terms.payer == consent.payer
                                && terms.refund_payload == consent.refund_payload
                                && terms.expected_amount == entry.amount
                                && now < terms.funding_admission_before_daa
                                && now < consent.admission_expiry
                        },
                    )?;
                }
            }

            let origin = AcceptedOrigin {
                accepting_block: NativeHash64(point.block.as_bytes()),
                carrier_block: NativeHash64(carrier.carrier_block.as_bytes()),
                carrier_transaction: NativeHash64(tx.id().as_bytes()),
                accepted_tx_ordinal: u32::try_from(carrier.prefix.accepted_transactions)
                    .map_err(|_| NativeCoverageV1::overflow::<Self>("Positive accepted ordinal"))?,
                event_ordinal: 0,
                accepted_daa: point.daa_score,
                accepted_blue_score: point.blue_score,
            };
            let transport = carrier._lease.ok_or_else(|| {
                NativeCoverageV1::unsupported::<Self>("actual Positive carrier", "missing original Full validation lease")
            })?;
            let action = PositiveAssignmentRawActionV2::from_native_transport_v2(
                carrier.body,
                carrier.index_within_block,
                carrier.entries,
                carrier.fee,
                origin,
                carrier.prefix.selected_parent,
                current_output,
                rules,
                transport,
                decode,
            )?;
            let action = if kind == 14 {
                let fees = carrier.relay_carrier_fees.ok_or_else(|| {
                    NativeCoverageV1::unsupported::<Self>("relay carrier fees", "accepted fee partition unavailable")
                })?;
                if fees.relay_charge() != 0 {
                    unforwarded_debits -= 1;
                }
                action.with_relay_carrier_fees_v2(fees)?
            } else {
                action
            };
            let action = match signed_anchor {
                Some((block, daa)) => action.with_signed_anchor_clock_v2(block, daa)?,
                None => action,
            };
            let action = match registered_class {
                Some(descriptor) => action.with_registered_class_v2(descriptor)?,
                None => action,
            };
            let action = match pending_relay {
                Some(archive) => action.with_pending_relay_replay_v2(archive)?,
                None => action,
            };
            let action = match case_relay_contract {
                Some((assignment, material)) => action.with_case_relay_contract_v2(assignment, material)?,
                None => action,
            };
            let action = match open_carriage {
                Some((carriage, material)) => action.with_original_open_carriage_v2(carriage, material)?,
                None => action,
            };
            let action = match case_backing {
                Some(backing) => action
                    .with_native_case_backing_v2(backing)
                    .map_err(|why| NativeCoverageV1::unsupported::<Self>("native Case predecessor owner", &format!("{why:?}")))?,
                None => action,
            };
            actions
                .try_reserve_exact(1)
                .map_err(|_| NativeCoverageV1::unsupported::<Self>("native Positive raw actions", "allocation refused"))?;
            actions.push(action);
        }
        if unforwarded_debits != 0 {
            return Err(NativeCoverageV1::unsupported::<Self>(
                "relay carrier fees",
                "native debit has no replayable registration action",
            ));
        }
        Ok(actions)
    }
}
