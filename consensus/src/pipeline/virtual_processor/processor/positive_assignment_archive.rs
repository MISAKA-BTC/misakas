//! Private concrete archive loading from accepted native storage. The journal
//! supplies DATA identifiers; original acceptance/header/delta/UTXO stores must
//! independently resolve them. Missing/pruned originals never become empty
//! history or the current P state.
use super::*;
use crate::{
    model::stores::{
        acceptance_data::AcceptanceDataStoreReader, block_transactions::BlockTransactionsStoreReader, ghostdag::GhostdagStoreReader,
        headers::HeaderStoreReader, utxo_diffs::UtxoDiffsStoreReader,
    },
    processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags,
};
use kaspa_consensus_core::{
    palw_native_resource_v1::{NativeCoverageV1, reserve_native_requests_v1},
    palw_positive_assignment_case_material_v2::AssignmentCaseBoundaryV2,
    palw_positive_assignment_material_v2::{AssignmentAdmissionMaterialV2, AssignmentAdmissionMaterialViewV2},
    palw_positive_rows_v2::{PositiveRowErrorV2, PositiveRowsV2},
    palw_state_v2::{
        AssignmentControlBoundaryV2, NativeAcceptedCaseCarriageV2, NativeAssignedCaseBackingErrorV2, NativeAssignmentRulesV2,
        PalwChainStateV2,
    },
    tx::{PopulatedTransaction, UtxoEntry},
};
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentId,
        control_material::AssignmentControlViewV2,
        journal::{ASSIGNMENT_JOURNAL_HEADER_BYTES_V2, AssignmentJournalEventKindV2, AssignmentJournalRecordViewV2},
        open_material::AssignmentOpenLimitsV2,
    },
    journal::CaseJournalRecordView,
    native::NativeRowKey,
    rows::{AcceptedOrigin, MAX_NATIVE_KEY_BYTES, NativeRowBody, NativeRowValue},
};

/// Private concrete boundary selection. This is not a successful replay or
/// arbitrary provider interface. Assignment and Case subjects stay distinct.
enum NativeArchiveBoundaryV2 {
    Assignment(AssignmentControlBoundaryV2),
    Case(AssignmentCaseBoundaryV2),
}
impl NativeArchiveBoundaryV2 {
    fn fields(&self) -> (AcceptedOrigin, [u8; 64], [u8; 64], u64, u64) {
        match self {
            Self::Assignment(b) => (b.origin, b.selected_parent.0, b.pre_event_root.0, b.before_delta_end, b.fee),
            Self::Case(b) => (b.origin, b.selected_parent.0, b.pre_event_root.0, b.before_delta_end, b.fee),
        }
    }
    fn check_populated_data(&self, tx: &kaspa_consensus_core::tx::Transaction, entries: &[UtxoEntry], fee: u64) -> bool {
        match self {
            Self::Assignment(b) => b.check_populated_data(tx, entries, fee).is_ok(),
            Self::Case(b) => b.check_populated_data(tx, entries, fee).is_ok(),
        }
    }
}

#[derive(Debug)]
pub(in crate::pipeline::virtual_processor) enum NativeAssignmentArchiveErrorV2 {
    Unavailable(&'static str),
    InvalidContinuation,
    Invalid(&'static str),
    Resource(NativeCoverageV1),
}
impl VirtualStateProcessor {
    /// Re-resolve one original accepted carriage and executed-prefix state.
    /// Limits are the independently authenticated ORIGINAL physical/replay
    /// limits, never counts or caps copied from this journal record.
    pub(in crate::pipeline::virtual_processor) fn load_positive_case_carriage_v2(
        &self,
        current_selected_parent: BlockHash,
        record: CaseJournalRecordView<'_>,
        boundary: AssignmentCaseBoundaryV2,
        maximum_heap: u64,
        maximum_work: u64,
    ) -> Result<NativeAcceptedCaseCarriageV2, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        if [maximum_heap, maximum_work].into_iter().any(|n| n == 0 || n == u64::MAX) {
            return Err(E::Invalid("original archive limits are not finite"));
        }
        let header = record.header();
        if boundary.case != header.case || boundary.origin != header.origin {
            return Err(E::Invalid("Case boundary scope/origin differs from literal record"));
        }
        // Exact fixed preimage and reference must be present ONCE, rather than
        // accepting a caller-provided decoded boundary detached from the raw.
        let mut boundary_count = 0u32;
        for aux in record.aux() {
            if aux.material() == boundary.material() {
                if AssignmentCaseBoundaryV2::decode(aux.preimage()).ok() != Some(boundary) {
                    return Err(E::Invalid("boundary preimage differs"));
                }
                boundary_count = boundary_count.checked_add(1).ok_or(E::Invalid("boundary count"))?;
            }
        }
        if boundary_count != 1 {
            return Err(E::Invalid("missing or duplicate actual boundary"));
        }
        self.load_positive_accepted_carriage_v2(
            current_selected_parent,
            NativeArchiveBoundaryV2::Case(boundary),
            maximum_heap,
            maximum_work,
        )
        .map(|(carriage, _)| carriage)
    }

    /// Read original decode rules for a continuation against the actual walk's
    /// selected-parent state. Absence of an Assignment row can be a later
    /// same-block intake. Missing history for an existing row is unavailable.
    pub(in crate::pipeline::virtual_processor) fn original_assignment_rules_v2(
        &self,
        parent: BlockHash,
        state: &PalwChainStateV2,
        assignment: AssignmentId,
        continuation_body: &[u8],
    ) -> Result<Option<(NativeAssignmentRulesV2, NativeAcceptedCaseCarriageV2)>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let workspace = (4 * std::mem::size_of::<NativeRowValue>()
            + 2 * std::mem::size_of::<AssignmentAdmissionMaterialV2>()
            + 2 * MAX_NATIVE_KEY_BYTES) as u64;
        let _workspace =
            reserve_native_requests_v1(workspace).map_err(E::Resource)?.ok_or(E::Unavailable("original rules workspace"))?;
        let rows = PositiveRowsV2::new(state);
        let row = match rows.read(NativeRowKey::Assignment(assignment)) {
            Ok(Some(value)) => match value.body {
                NativeRowBody::Assignment(row) => row,
                _ => return Err(E::Invalid("original Assignment key/value mismatch")),
            },
            Ok(None) | Err(PositiveRowErrorV2::MissingKernelRoute) => return Ok(None),
            Err(_) => return Err(E::Invalid("original Assignment row malformed")),
        };
        let frame = rows
            .read(NativeRowKey::AssignmentJournalFrame { assignment, ordinal: 0 })
            .map_err(|_| E::Invalid("original Assignment frame key/value"))?
            .ok_or(E::Unavailable("original Assignment frame missing"))?;
        let NativeRowBody::AssignmentJournalFrame(frame) = frame.body else {
            return Err(E::Invalid("original Assignment frame kind"));
        };
        if frame.header.kind != AssignmentJournalEventKindV2::Open || frame.header.origin != row.origin {
            return Err(E::Invalid("original Assignment frame differs from rooted origin"));
        }
        let max_payload = u64::from(frame.header.payload_bytes);
        let record_bytes = max_payload + ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64;
        let _raw_lease =
            reserve_native_requests_v1(record_bytes).map_err(E::Resource)?.ok_or(E::Unavailable("original Open bytes lease"))?;
        // Drop the store guard before loading the original state and delta.
        let raw = self
            .palw_state_v2_store
            .read()
            .with_positive_assignment_journal_record_v2(frame.header.id(), record_bytes, |raw| {
                let mut copy = Vec::new();
                copy.try_reserve_exact(raw.len()).map_err(|_| E::Unavailable("original Open allocation"))?;
                if copy.capacity() as u64 > record_bytes {
                    return Err(E::Unavailable("original Open allocation exceeded lease"));
                }
                copy.extend_from_slice(raw);
                Ok(copy)
            })
            .map_err(|_| E::Unavailable("rooted original Open bytes"))??;
        let record =
            AssignmentJournalRecordViewV2::decode(&raw, record_bytes).map_err(|_| E::Invalid("original Open journal codec"))?;
        if record.frame() != frame {
            return Err(E::Invalid("original Open frame does not match rooted frame"));
        }
        let envelope =
            AssignmentControlViewV2::decode(record.payload(), max_payload).map_err(|_| E::Invalid("original Open envelope"))?;
        let (_, body) = AssignmentControlBoundaryV2::native_body_from_record_data_v2(&record, max_payload)
            .map_err(|_| E::Invalid("original Open carrier framing"))?;
        let view = AssignmentAdmissionMaterialViewV2::decode(
            &row,
            envelope.inner(),
            body,
            AssignmentOpenLimitsV2::within_payload(max_payload),
        )
        .map_err(|_| E::Invalid("original Open material differs from rooted Assignment"))?;
        let rules = NativeAssignmentRulesV2::from_original_material_data_v2(&row, &view.material)
            .map_err(|_| E::Invalid("original Assignment rule scope"))?;
        let limits = rules.body_limits().map_err(|_| E::Invalid("original Assignment body limits"))?;
        kaspa_consensus_core::palw_positive_court_v2::coordinator::assignment_body::scan_assignment_body_v2(continuation_body, limits)
            .map_err(|_| E::InvalidContinuation)?;
        let (carriage, work) = self.load_positive_assignment_carriage_v2(
            parent,
            record,
            max_payload,
            view.material.custody.heap_bytes.min(view.material.native_preimages.physical.max_heap_bytes),
            view.material.custody.replay_work,
        )?;
        let prefix = carriage.prefix_state().map_err(|_| E::Unavailable("original Open pre-event state"))?;
        if !matches!(PositiveRowsV2::new(prefix).read(NativeRowKey::Assignment(assignment)), Ok(None)) {
            return Err(E::Invalid("original Open did not start at an absent Assignment"));
        }
        let anchor = BlockHash::from_bytes(row.bundle.terms.branch_anchor.0);
        if !self
            .reachability_service
            .try_is_chain_ancestor_of(anchor, carriage.selected_parent())
            .map_err(|_| E::Unavailable("original signed ancestor reachability"))?
        {
            return Err(E::Invalid("signed Assignment anchor is outside original selected chain"));
        }
        let anchor_daa = self.headers_store.get_daa_score(anchor).map_err(|_| E::Unavailable("original signed ancestor header"))?;
        let remaining_work = view.material.custody.replay_work.checked_sub(work).ok_or(E::Invalid("original replay work cap"))?;
        prefix
            .check_original_assignment_verifier_candidates_v2(&view, anchor_daa, row.bundle.terms.result_before_daa, remaining_work)
            .map_err(|_| E::Invalid("original verifier candidates differ or exceed replay budget"))?;
        Ok(Some((rules, carriage)))
    }

    /// Reconstruct a genuine Assignment control event from the original native
    /// stores. The caller still has to choose its record from CURRENT rooted
    /// history and replay duties/custody. This method issues no admission or
    /// accounting capability and never reselects a later intake profile.
    fn load_positive_assignment_carriage_v2(
        &self,
        current_selected_parent: BlockHash,
        record: AssignmentJournalRecordViewV2<'_>,
        maximum_payload: u64,
        maximum_heap: u64,
        maximum_work: u64,
    ) -> Result<(NativeAcceptedCaseCarriageV2, u64), NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        if record.header().kind == AssignmentJournalEventKindV2::MetadataReceipt {
            return Err(E::Unavailable("actual metadata receipt carriage owner is not connected"));
        }
        let (boundary, native_body) = AssignmentControlBoundaryV2::native_body_from_record_data_v2(&record, maximum_payload)
            .map_err(|_| E::Invalid("invalid original Assignment control carriage framing"))?;
        let (carriage, work) = self.load_positive_accepted_carriage_v2(
            current_selected_parent,
            NativeArchiveBoundaryV2::Assignment(boundary),
            maximum_heap,
            maximum_work,
        )?;
        let tx = carriage.transaction();
        if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
            || tx.payload.get(..3) != Some(&[1, 0, 176])
            || tx.payload.get(3..) != Some(native_body)
        {
            return Err(E::Invalid("original actual Assignment carrier differs from journal"));
        }
        Ok((carriage, work))
    }

    fn load_positive_accepted_carriage_v2(
        &self,
        current_selected_parent: BlockHash,
        boundary: NativeArchiveBoundaryV2,
        maximum_heap: u64,
        maximum_work: u64,
    ) -> Result<(NativeAcceptedCaseCarriageV2, u64), NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        if [maximum_heap, maximum_work].into_iter().any(|n| n == 0 || n == u64::MAX) {
            return Err(E::Invalid("original archive limits are not finite"));
        }
        let (origin, expected_parent, pre_event_root, before_delta_end, expected_fee) = boundary.fields();
        let accepting = BlockHash::from_bytes(origin.accepting_block.0);
        let carrier_block = BlockHash::from_bytes(origin.carrier_block.0);
        if !self
            .reachability_service
            .try_is_chain_ancestor_of(accepting, current_selected_parent)
            .map_err(|_| E::Unavailable("accepted lineage reachability"))?
        {
            return Err(E::Invalid("original accepting block is not on current selected chain"));
        }
        let actual_header = self.headers_store.get_header(accepting).map_err(|_| E::Unavailable("original accepting header"))?;
        let selected_parent =
            self.ghostdag_store.get_selected_parent(accepting).map_err(|_| E::Unavailable("original selected parent"))?;
        if actual_header.daa_score != origin.accepted_daa
            || actual_header.blue_score != origin.accepted_blue_score
            || selected_parent.as_bytes() != expected_parent
            || origin.event_ordinal != 0
        {
            return Err(E::Invalid("original header/parent/clock differs"));
        }
        let acceptance = self.acceptance_data_store.get(accepting).map_err(|_| E::Unavailable("original acceptance data"))?;
        let count = acceptance
            .iter()
            .try_fold(0u64, |n, b| n.checked_add(b.accepted_transactions.len() as u64))
            .ok_or(E::Invalid("acceptance length overflow"))?;
        if u64::from(origin.accepted_tx_ordinal) >= count {
            return Err(E::Invalid("accepted ordinal outside original acceptance"));
        }
        let mut ordinal = 0u64;
        let mut target = None;
        for block in acceptance.iter() {
            for tx in &block.accepted_transactions {
                if ordinal == u64::from(origin.accepted_tx_ordinal) {
                    if block.block_hash != carrier_block || tx.transaction_id.as_bytes() != origin.carrier_transaction.0 {
                        return Err(E::Invalid("actual accepted ordinal/carrier differs"));
                    }
                    target = Some(tx.index_within_block);
                }
                ordinal += 1;
            }
        }
        let index = target.ok_or(E::Invalid("no original accepted carrier"))?;
        let body = self.block_transactions_store.get(carrier_block).map_err(|_| E::Unavailable("original carrier block body"))?;
        let tx = body.get(index as usize).ok_or(E::Invalid("original carrier transaction index"))?;
        if tx.is_coinbase() || !tx.id_is_current() || tx.id().as_bytes() != origin.carrier_transaction.0 {
            return Err(E::Invalid("original actual transaction identity"));
        }
        let work = count
            .checked_mul(tx.inputs.len() as u64)
            .and_then(|n| n.checked_add(tx.payload.len() as u64))
            .ok_or(E::Invalid("archive work overflow"))?;
        if work > maximum_work {
            return Err(E::Invalid("original archive work cap"));
        }
        let diff = self.utxo_diffs_store.get(accepting).map_err(|_| E::Unavailable("original accepted UTXO delta"))?;
        // First pass borrows actual originals and prices every clone before
        // constructing the populated input vector or validation scratch.
        let mut heap = (std::mem::size_of::<NativeAcceptedCaseCarriageV2>() + std::mem::size_of::<[usize; 2]>()) as u64;
        heap = heap
            .checked_add(
                (tx.inputs.len() as u64)
                    .checked_mul(std::mem::size_of::<UtxoEntry>() as u64)
                    .ok_or(E::Invalid("input slots overflow"))?,
            )
            .ok_or(E::Invalid("input heap overflow"))?;
        for (position, input) in tx.inputs.iter().enumerate() {
            if tx.inputs[..position].iter().any(|prior| prior.previous_outpoint == input.previous_outpoint) {
                return Err(E::Invalid("duplicate original input"));
            }
            let bytes = if let Some(entry) = diff.remove.get(&input.previous_outpoint) {
                entry.script_public_key.script().len() as u64
            } else {
                self.with_prior_assignment_output_v2(&acceptance, origin, input.previous_outpoint, |output, _| {
                    output.script_public_key.script().len() as u64
                })?
                .ok_or(E::Unavailable("original populated input or same-prefix creator"))?
            };
            heap = heap.checked_add(bytes).ok_or(E::Invalid("input script heap overflow"))?;
        }
        // Full validator scratch is owned by its actual native implementation;
        // these retained inputs and owner remain separately reserved.
        if heap > maximum_heap {
            return Err(E::Invalid("original archive retained heap cap"));
        }
        let lease = reserve_native_requests_v1(heap).map_err(E::Resource)?.ok_or(E::Unavailable("actual archive allocation lease"))?;
        let mut entries = Vec::new();
        entries.try_reserve_exact(tx.inputs.len()).map_err(|_| E::Unavailable("pre-admitted input allocation"))?;
        for input in &tx.inputs {
            let entry = if let Some(entry) = diff.remove.get(&input.previous_outpoint) {
                entry.clone()
            } else {
                self.with_prior_assignment_output_v2(&acceptance, origin, input.previous_outpoint, |output, coinbase| {
                    UtxoEntry::new(output.value, output.script_public_key.clone(), origin.accepted_daa, coinbase)
                })?
                .ok_or(E::Unavailable("same-prefix original creator disappeared"))?
            };
            entries.push(entry);
        }
        if !boundary.check_populated_data(tx, &entries, expected_fee) {
            return Err(E::Invalid("actual original populated input/fee identity"));
        }
        let populated = PopulatedTransaction::new(tx, entries);
        let actual_fee = self
            .transaction_validator
            .validate_populated_transaction_and_get_fee(&populated, origin.accepted_daa, TxValidationFlags::Full, None, None)
            .map_err(|_| E::Invalid("original ordinary transaction Full validation"))?;
        if actual_fee != expected_fee {
            return Err(E::Invalid("original actual Full fee differs"));
        }
        let (_, parent) = self.palw_v2_state_at(selected_parent).ok_or(E::Unavailable("original native selected-parent state"))?;
        let params = self.palw_state_params_v2.as_ref().ok_or(E::Unavailable("native state parameters"))?;
        let store = self.palw_state_v2_store.read();
        let (delta_parent, delta) =
            store.delta_of_retained_v1(accepting).map_err(|_| E::Unavailable("original native accepting delta"))?;
        if delta_parent != selected_parent {
            return Err(E::Invalid("actual native delta parent differs"));
        }
        if delta.point.block != accepting
            || delta.point.daa_score != origin.accepted_daa
            || delta.point.blue_score != origin.accepted_blue_score
        {
            return Err(E::Invalid("actual native delta clock differs"));
        }
        let end = usize::try_from(before_delta_end).map_err(|_| E::Invalid("prefix position address space"))?;
        let root = Hash64::from_bytes(pre_event_root);
        let prefix = parent.try_delta_prefix_v2(&delta, end, root, params).map_err(|e| match e {
            kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => E::Resource(r),
            _ => E::Invalid("original executed native prefix mismatch"),
        })?;
        let entries = populated.entries;
        let carriage =
            NativeAcceptedCaseCarriageV2::from_native_history_v2(body, index, entries, actual_fee, origin, selected_parent, lease)
                .map_err(|_| E::Invalid("actual native carriage shape"))?;
        let carriage = carriage.with_native_prefix_state_v2(Arc::new(prefix), root).map_err(|error| match error {
            NativeAssignedCaseBackingErrorV2::NativeResource(r) => E::Resource(r),
            _ => E::Invalid("actual prefix owner mismatch"),
        })?;
        Ok((carriage, work))
    }

    /// Same accepting-prefix creator outputs disappear from net UTXO diffs;
    /// resolve them only from ACTUAL earlier accepted transaction bodies.
    fn with_prior_assignment_output_v2<T>(
        &self,
        acceptance: &kaspa_consensus_core::acceptance_data::AcceptanceData,
        origin: AcceptedOrigin,
        outpoint: kaspa_consensus_core::tx::TransactionOutpoint,
        mut visit: impl FnMut(&kaspa_consensus_core::tx::TransactionOutput, bool) -> T,
    ) -> Result<Option<T>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let mut ordinal = 0u64;
        for block in acceptance {
            for entry in &block.accepted_transactions {
                if ordinal >= u64::from(origin.accepted_tx_ordinal) {
                    return Ok(None);
                }
                if entry.transaction_id == outpoint.transaction_id {
                    let body = self
                        .block_transactions_store
                        .get(block.block_hash)
                        .map_err(|_| E::Unavailable("earlier accepted creator body"))?;
                    let tx = body.get(entry.index_within_block as usize).ok_or(E::Invalid("earlier creator index"))?;
                    if !tx.id_is_current() || tx.id() != outpoint.transaction_id {
                        return Err(E::Invalid("earlier accepted creator identity"));
                    }
                    return tx
                        .outputs
                        .get(outpoint.index as usize)
                        .map(|output| Some(visit(output, tx.is_coinbase())))
                        .ok_or(E::Invalid("earlier creator output index"));
                }
                ordinal += 1;
            }
        }
        Ok(None)
    }
}
