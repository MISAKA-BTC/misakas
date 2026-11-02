//! PRIVATE child of the actual UTXO validator. A seal records Full script
//! validation at one real accepted prefix; it is NOT Court admission, a live
//! funding output, a coin lock, a role, a generation token or credit authority.
//! Ordinary UTXO acceptance remains the sole applier of transaction effects.
//! The second phase must independently resolve notice/origin, current roles,
//! policy, funding, resource leases and the canonical TransitionBuilder rows.
use super::BondSpendFilter;
use crate::model::services::reachability::ReachabilityService;
use crate::{
    pipeline::virtual_processor::VirtualStateProcessor,
    processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags,
};
use kaspa_consensus_core::{
    BlockHash,
    acceptance_data::AcceptanceData,
    block::BlockTransactionsV3,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_state_v2::PalwBlockContextV2,
    tx::{Transaction, TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry, ValidatedTransaction},
    utxo::utxo_view::UtxoView,
};
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
            prefix,
            carrier_block: actual_carrier_block,
            index_within_block: index,
            _lease: Some(lease),
        });
    }
    pub(in crate::pipeline::virtual_processor) fn take_resource_fault(&mut self) -> Option<NativeCoverageV1> {
        self.resource_fault.take()
    }
    /// Private transport handoff, not a Court-success result. Check exact real
    /// acceptance origin and ordinal before exposing companions to phase two.
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
                || !matches!(tx.payload.get(7), Some(8..=13))
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
