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
    palw_state_v2::{NativeAcceptedCaseCarriageV2, NativeAssignedCaseBackingErrorV2},
    tx::{PopulatedTransaction, UtxoEntry},
};
use misaka_palw_positive_contract::{journal::CaseJournalRecordView, rows::AcceptedOrigin};

#[derive(Debug)]
pub(in crate::pipeline::virtual_processor) enum NativeAssignmentArchiveErrorV2 {
    Unavailable(&'static str),
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
        let origin = header.origin;
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
            || selected_parent.as_bytes() != boundary.selected_parent.0
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
        boundary
            .check_populated_data(tx, &entries, boundary.fee)
            .map_err(|_| E::Invalid("actual original populated input/fee identity"))?;
        let populated = PopulatedTransaction::new(tx, entries);
        let actual_fee = self
            .transaction_validator
            .validate_populated_transaction_and_get_fee(&populated, origin.accepted_daa, TxValidationFlags::Full, None, None)
            .map_err(|_| E::Invalid("original ordinary transaction Full validation"))?;
        if actual_fee != boundary.fee {
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
        let end = usize::try_from(boundary.before_delta_end).map_err(|_| E::Invalid("prefix position address space"))?;
        let root = Hash64::from_bytes(boundary.pre_event_root.0);
        let prefix = parent.try_delta_prefix_v2(&delta, end, root, params).map_err(|e| match e {
            kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => E::Resource(r),
            _ => E::Invalid("original executed native prefix mismatch"),
        })?;
        let entries = populated.entries;
        let carriage =
            NativeAcceptedCaseCarriageV2::from_native_history_v2(body, index, entries, actual_fee, origin, selected_parent, lease)
                .map_err(|_| E::Invalid("actual native carriage shape"))?;
        carriage.with_native_prefix_state_v2(Arc::new(prefix), root).map_err(|error| match error {
            NativeAssignedCaseBackingErrorV2::NativeResource(r) => E::Resource(r),
            _ => E::Invalid("actual prefix owner mismatch"),
        })
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
