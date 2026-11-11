//! Prospective tokenizer service costs and causal IDs. This does not add a
//! MaterialTargetV1 wire variant or authorize a V1 consumer to accept openings.
use super::*;
use crate::palw_court_shared_material_intake_v1::batch::plain_tokenizer::{PlainBatchTokenizerBoundsV2, PlainBatchTokenizerV2};

pub(crate) const TOKENIZER_DEMAND_ORDINAL_V2: u64 = u64::MAX - 2;

/// Native profile allowances. Raw byte/hash/ID work below is a lower bound
/// on the allowance, not a CPU calibration or a bound on signature/archive
/// work. A future literal control consumer must check its complete wire and
/// native execution against this same allowance before it can be activated.
/// This allowance is also added to each semantic Finish, where authentication
/// must precede immutable completion; case verification is a separate recheck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlainTokenizerBatchBudgetV2 {
    pub verification: ServiceCost,
    pub workspace_bytes: u64,
    pub verification_daa: u64,
}

impl PlainTokenizerBatchBudgetV2 {
    pub(crate) fn check_v2(self, bounds: PlainBatchTokenizerBoundsV2) -> Result<(), BatchCourtErrorV1> {
        let work = bounds
            .opening_material_bytes
            .checked_add(bounds.input_ids_body_bytes)
            .and_then(|n| n.checked_add(bounds.maximum_opening_hashes))
            .and_then(|n| n.checked_add(u64::from(bounds.input_ids_count)))
            .and_then(|n| n.checked_add(u64::from(bounds.rendering_openings)))
            .and_then(|n| n.checked_add(bounds.eog_replay_calls))
            .ok_or(BatchCourtErrorV1::DeliveryCost)?;
        let io = bounds
            .full_ids_body_bytes
            .checked_add(bounds.input_ids_body_bytes)
            .and_then(|n| n.checked_add(bounds.opening_material_bytes))
            .ok_or(BatchCourtErrorV1::DeliveryCost)?;
        let scratch = bounds.input_ids_scratch_bytes.max(bounds.eog_scratch_bytes);
        if self.verification_daa == 0
            || self.verification.controls != 1
            || self.verification.work < work
            || self.verification.io < io
            || self.workspace_bytes < scratch
            || self.workspace_bytes == 0
        {
            return Err(BatchCourtErrorV1::DeliveryCost);
        }
        Ok(())
    }

    pub(super) fn check_terminal_v2(self, numerical: ServiceCost, terminal: ServiceCost) -> Result<(), BatchCourtErrorV1> {
        let work = numerical.work.checked_add(self.verification.work).ok_or(BatchCourtErrorV1::DeliveryCost)?;
        let io = numerical.io.checked_add(self.verification.io).ok_or(BatchCourtErrorV1::DeliveryCost)?;
        if terminal.work < work || terminal.io < io || terminal.controls != 1 {
            return Err(BatchCourtErrorV1::DeliveryCost);
        }
        Ok(())
    }
}

/// Canonical standalone input-ID vector and ordered EOG/rendering body.
/// FullIds comes from the existing plain Output demand, exactly once.
pub(super) fn material_ranges_v2(branch: &PlainBatchTokenizerV2) -> Result<[u64; 2], BatchCourtErrorV1> {
    let bounds = branch.bounds_v2();
    let ranges = [bounds.input_ids_body_bytes, bounds.opening_material_bytes];
    if ranges.iter().any(|bytes| !(1..=super::super::MATERIAL_PIPELINED_MAX_BYTES).contains(bytes)) {
        return Err(BatchCourtErrorV1::CaseGraph);
    }
    Ok(ranges)
}

pub(crate) fn tokenizer_verification_id_v2(
    claim: Hash64,
    scope: Hash64,
    branch: &PlainBatchTokenizerV2,
) -> Result<[u8; 64], BatchCourtErrorV1> {
    if scope == Hash64::default() || !branch.matches_claim_v2(claim) {
        return Err(BatchCourtErrorV1::CaseGraph);
    }
    crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/plain-batch-tokenizer-verification/v2",
        &(claim, scope, branch.branch_id_v2(), TOKENIZER_DEMAND_ORDINAL_V2),
    )
    .map(|id| id.as_bytes())
    .map_err(|_| BatchCourtErrorV1::CaseGraph)
}

/// Invoked by the accepted-tokenizer fixture, so the graph uses a branch
/// derived from native registration rather than public, forgeable bounds.
#[cfg(test)]
pub(crate) fn check_compiled_tokenizer_fixture_v2(
    branch: &PlainBatchTokenizerV2,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
) {
    let claim = crate::palw_freeprompt_v3::fp_claim_id_v3(original);
    let scope = Hash64::from_u64_word(981);
    let base = NumericalBatchReplayBudgetV1::test_only_v1();
    let numerical = base.service_cost_v1().unwrap();
    let budget = PlainTokenizerBatchBudgetV2 {
        verification: ServiceCost { bytes: 20_000, work: 1_000_000, io: 1_000_000, controls: 1 },
        workspace_bytes: 1 << 20,
        verification_daa: 1,
    };
    let bounds = branch.bounds_v2();
    budget.check_v2(bounds).unwrap();
    let output = PlainBatchOutputV2::derive_v2(original, source, budget.verification, 1).unwrap();
    let costs = NativeBatchDeliveryCostsV1 {
        numerical_verification: base,
        terminal: ServiceCost {
            bytes: 100_000,
            work: numerical.work + budget.verification.work,
            io: numerical.io + budget.verification.io,
            controls: 1,
        },
        recovery_daa: 1,
        maximum_packets: 64,
        timing: NativeBatchDeliveryTimingV1 {
            open_daa: 1,
            provider_wait_daa: 1,
            begin_daa: 1,
            parts_daa: 2,
            finish_daa: 1,
            verification_daa: 1,
            stop_daa: 1,
            terminal_daa: 1,
        },
    };
    let storage = MaterialIntakeScheduleV1 {
        sompi_per_byte: 2,
        draft_ttl_daa: 8,
        cover: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 {
            max_original_leaves: 1,
            max_total_edges: 1,
            max_total_coordinates: 1,
            max_root_memberships: 1,
        },
    };
    // The numeric demand is a fixture bound, not a whole-source quote.
    let shape = || NativeBatchDeliveryShapeV1 { claim, scope, accepted_daa: 12, material_until_daa: 11_000, maximums: &[64] };
    let price = |_, bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
    let capacity = ServiceCost { bytes: 20_000_000, work: 10_000_000, io: 10_000_000, controls: 16 };
    let mut baseline = ServiceReservations::new(capacity).unwrap();
    let old = compile_native_batch_plain_output_v2(shape(), &output, costs, storage, price).unwrap();
    let (old_quote, _) = CompiledBatchAdmissionV1::reserve_v1(old, &mut baseline).unwrap();
    let compile = |output: &PlainBatchOutputV2, budget, costs| {
        compile_native_batch_plain_tokenizer_v2(shape(), output, branch, budget, costs, storage, price)
    };
    let mut book = ServiceReservations::new(capacity).unwrap();
    let (quote, graph) = CompiledBatchAdmissionV1::reserve_v1(compile(&output, budget, costs).unwrap(), &mut book).unwrap();
    assert_eq!(
        quote.retained_payload_bytes - old_quote.retained_payload_bytes,
        bounds.input_ids_body_bytes + bounds.opening_material_bytes
    );
    assert_ne!(quote.relative_graph_id, old_quote.relative_graph_id);
    let control = tokenizer_verification_id_v2(claim, scope, branch).unwrap();
    let output_control = batch_plain_output_control_id_v2(claim, scope).unwrap();
    let full_ids_finish = BatchResponsePositionV1 { demand_ordinal: BATCH_PLAIN_OUTPUT_ORDINAL_V2, range_index: 0 }
        .packet_id(claim, scope, Stage::Finish)
        .unwrap();
    assert_eq!(graph.iter().filter(|node| node.packet.id == full_ids_finish).count(), 1);
    let node = graph.iter().find(|node| node.packet.id == control).unwrap();
    assert!(node.predecessors.contains(&full_ids_finish));
    assert!(!node.predecessors.contains(&output_control));
    assert_eq!(node.predecessors.len(), 3, "two semantic ranges plus shared FullIds");
    let full_ids_node = graph.iter().find(|node| node.packet.id == full_ids_finish).unwrap();
    for finish in node.predecessors.iter().filter(|id| **id != full_ids_finish) {
        let semantic = graph.iter().find(|node| node.packet.id == *finish).unwrap();
        assert!(semantic.predecessors.contains(&full_ids_finish), "authenticate before making semantic material immutable");
        assert!(semantic.packet.release_daa >= full_ids_node.packet.service_before);
        assert!(book.assigned_daa(&full_ids_finish).unwrap() < book.assigned_daa(finish).unwrap());
        assert_eq!(semantic.packet.cost.work, budget.verification.work + 1);
        assert_eq!(semantic.packet.cost.io, budget.verification.io + 1);
    }
    for predecessor in &node.predecessors {
        assert!(book.assigned_daa(predecessor).unwrap() < book.assigned_daa(&control).unwrap());
    }
    let (_, terminal) = batch_numerical_service_ids_v1(claim, scope).unwrap();
    assert!(graph.iter().find(|node| node.packet.id == terminal).unwrap().predecessors.contains(&control));
    assert!(book.assigned_daa(&control).unwrap() < book.assigned_daa(&terminal).unwrap());
    assert!(
        compile_native_batch_plain_tokenizer_v2(
            NativeBatchDeliveryShapeV1 { accepted_daa: 11, ..shape() },
            &output,
            branch,
            budget,
            costs,
            storage,
            price,
        )
        .is_err(),
        "an earlier registration lookup cannot be reused at another acceptance clock"
    );
    let mut wrong_output = PlainBatchOutputV2::derive_v2(original, source, budget.verification, 1).unwrap();
    wrong_output.full_ids_range[0] += 1;
    assert!(compile(&wrong_output, budget, costs).is_err());
    assert!(compile(&output, PlainTokenizerBatchBudgetV2 { workspace_bytes: 0, ..budget }, costs).is_err());
    assert!(
        compile(
            &output,
            PlainTokenizerBatchBudgetV2 { verification: ServiceCost { work: 1, ..budget.verification }, ..budget },
            costs
        )
        .is_err()
    );
    assert!(
        compile(
            &output,
            budget,
            NativeBatchDeliveryCostsV1 { terminal: ServiceCost { work: numerical.work, io: numerical.io, ..costs.terminal }, ..costs }
        )
        .is_err(),
        "terminal must fund both numerical and tokenizer rechecks"
    );
    let mut insufficient = ServiceReservations::new(ServiceCost { work: 1, ..capacity }).unwrap();
    let before = insufficient.clone();
    assert!(CompiledBatchAdmissionV1::reserve_v1(compile(&output, budget, costs).unwrap(), &mut insufficient).is_err());
    assert_eq!(insufficient, before, "capacity refusal cannot retain a partial booking");
    // The original Finish fits this TTL, but the authenticated Finish does
    // not. The new phase must not silently extend a draft's lifetime.
    let short_storage = MaterialIntakeScheduleV1 { draft_ttl_daa: 4, ..storage };
    assert!(compile_native_batch_plain_output_v2(shape(), &output, costs, short_storage, price).is_ok());
    assert!(compile_native_batch_plain_tokenizer_v2(shape(), &output, branch, budget, costs, short_storage, price).is_err());
}
