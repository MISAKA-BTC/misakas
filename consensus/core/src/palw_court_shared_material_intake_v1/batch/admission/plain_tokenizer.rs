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

/// Required native operands, independent of a profile's capacity and tariff.
/// Work counts byte/ID/hash visits; IO counts bytes; workspace is resident bytes.
/// These are deterministic floors, not wall-clock measurements. Control framing,
/// signatures, archive work and transaction overhead still require profile terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlainTokenizerResourceFloorV2 {
    pub work: u64,
    pub io: u64,
    pub workspace_bytes: u64,
}

impl PlainTokenizerResourceFloorV2 {
    pub(crate) fn derive_v2(bounds: PlainBatchTokenizerBoundsV2) -> Result<Self, BatchCourtErrorV1> {
        let work = bounds
            .opening_material_bytes
            .checked_add(bounds.input_ids_body_bytes)
            // Assembly and the typed decoder independently authenticate bytes.
            .and_then(|n| n.checked_mul(2))
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
        // Reserve both decoded operand sets and their authentication scratch
        // conservatively, including a combined Verify/Terminal consumer.
        // Borrowed canonical bytes and shared FullIds have separate owners.
        let input_workspace = u64::from(bounds.input_ids_count)
            .checked_mul(4)
            .and_then(|n| n.checked_add(std::mem::size_of::<Vec<u32>>() as u64))
            .and_then(|n| n.checked_add(bounds.input_ids_scratch_bytes))
            .ok_or(BatchCourtErrorV1::DeliveryCost)?;
        let per_rendering = bounds
            .maximum_rendering_siblings
            .checked_mul(64)
            .and_then(|n| n.checked_add(bounds.maximum_rendering_bytes))
            .and_then(|n| n.checked_add(std::mem::size_of::<crate::palw_fp_constraint_job_v1::PalwTokenTableOpeningV1>() as u64))
            .ok_or(BatchCourtErrorV1::DeliveryCost)?;
        let opening_workspace = per_rendering
            .checked_mul(u64::from(bounds.rendering_openings))
            .and_then(|n| n.checked_add(u64::from(bounds.eog_ids).checked_mul(4)?))
            .and_then(|n| n.checked_add(bounds.eog_siblings.checked_mul(64)?))
            .and_then(|n| n.checked_add(bounds.eog_scratch_bytes))
            .and_then(|n| n.checked_add(std::mem::size_of::<crate::palw_fp_constraint_job_v1::PalwTokenTableEogOpeningV1>() as u64))
            .and_then(|n| n.checked_add(std::mem::size_of::<Vec<crate::palw_fp_constraint_job_v1::PalwTokenTableOpeningV1>>() as u64))
            .ok_or(BatchCourtErrorV1::DeliveryCost)?;
        let scratch = input_workspace.checked_add(opening_workspace).ok_or(BatchCourtErrorV1::DeliveryCost)?;
        Ok(Self { work, io, workspace_bytes: scratch })
    }

    /// Compose the existing conservative control allowance without treating it
    /// as measured capacity. Keep wire allowance separate from work/IO in callers
    /// even while the draft reserves their maximum as an additional byte floor.
    pub(crate) fn budget_v2(
        self,
        control_floor: ServiceCost,
        workspace_floor: u64,
        verification_daa: u64,
    ) -> PlainTokenizerBatchBudgetV2 {
        PlainTokenizerBatchBudgetV2 {
            verification: ServiceCost {
                bytes: control_floor.bytes.max(self.work).max(self.io),
                work: control_floor.work.max(self.work),
                io: control_floor.io.max(self.io),
                controls: 1,
            },
            workspace_bytes: workspace_floor.max(self.workspace_bytes),
            verification_daa,
        }
    }
}

impl PlainTokenizerBatchBudgetV2 {
    pub(crate) fn check_v2(self, bounds: PlainBatchTokenizerBoundsV2) -> Result<(), BatchCourtErrorV1> {
        let required = PlainTokenizerResourceFloorV2::derive_v2(bounds)?;
        if self.verification_daa == 0
            || self.verification.controls != 1
            || self.verification.work < required.work
            || self.verification.io < required.io
            || self.workspace_bytes < required.workspace_bytes
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
    // The protected quote and the native admission checker share these floors.
    // Use the fixture's authenticated registration and real operand geometry.
    let required = PlainTokenizerResourceFloorV2::derive_v2(bounds).unwrap();
    let exact = required.budget_v2(ServiceCost { bytes: 1, work: 0, io: 0, controls: 1 }, 0, 1);
    exact.check_v2(bounds).unwrap();
    assert!(
        PlainTokenizerBatchBudgetV2 { verification: ServiceCost { work: required.work - 1, ..exact.verification }, ..exact }
            .check_v2(bounds)
            .is_err()
    );
    assert!(
        PlainTokenizerBatchBudgetV2 { verification: ServiceCost { io: required.io - 1, ..exact.verification }, ..exact }
            .check_v2(bounds)
            .is_err()
    );
    assert!(PlainTokenizerBatchBudgetV2 { workspace_bytes: required.workspace_bytes - 1, ..exact }.check_v2(bounds).is_err());
    // More FullIds grows IO, not the tokenizer opening work. A profile must not
    // silently convert either counter to the legacy G14 work-unit denominator.
    let more_ids = PlainBatchTokenizerBoundsV2 { full_ids_body_bytes: bounds.full_ids_body_bytes + 4, ..bounds };
    let more = PlainTokenizerResourceFloorV2::derive_v2(more_ids).unwrap();
    assert_eq!(more.work, required.work);
    assert_eq!(more.io, required.io + 4);
    assert!(exact.check_v2(more_ids).is_err());
    for overflowing in [
        PlainBatchTokenizerBoundsV2 { opening_material_bytes: u64::MAX, ..bounds },
        PlainBatchTokenizerBoundsV2 { full_ids_body_bytes: u64::MAX, ..bounds },
        PlainBatchTokenizerBoundsV2 { input_ids_scratch_bytes: u64::MAX, ..bounds },
    ] {
        assert!(PlainTokenizerResourceFloorV2::derive_v2(overflowing).is_err());
        assert!(budget.check_v2(overflowing).is_err());
    }
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
        assert_eq!(
            semantic.packet.cost.bytes,
            super::super::tokenizer_transport::tokenizer_response_wire_bytes_v2(Stage::Finish, 0).unwrap()
        );
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
            PlainTokenizerBatchBudgetV2 { workspace_bytes: bounds.input_ids_scratch_bytes.max(bounds.eog_scratch_bytes), ..budget },
            costs
        )
        .is_err(),
        "hash scratch alone does not cover decoded operands"
    );
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
