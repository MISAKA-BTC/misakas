//! Verifier-owned constrained-selection continuation over authenticated public pins.
//! Transport does not advance the local cursor; only the accepted snapshot acknowledges it.
use super::constraint::TiledPlanV1;

pub(super) fn nominate_v1(
    target: &PalwOffenceTargetV1,
    job: &PalwFreePromptJobV3,
    initial: &PalwTraceEventDisclosureV1,
    own_ids: &[u32],
    table: Arc<dyn PalwPublicTokenTableSourceV1>,
    ledger: &Arc<PalwMemoryLedgerV1>,
) -> Result<Option<TiledPlanV1>, String> {
    let PalwTraceEventDisclosureV1::Tiled { binding, generated_token_ids, .. } = initial else { return Ok(None) };
    kaspa_consensus_core::palw_step_refute::check_trace_event_disclosure_v1(
        target.trace_root,
        target.execution_root,
        0,
        0,
        initial,
        binding.step_leaf_count,
    )
    .map_err(|e| e.to_string())?;
    nominate_authenticated_ids_v1(target, job, binding, generated_token_ids, own_ids, table, ledger)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn nominate_authenticated_ids_v1(
    target: &PalwOffenceTargetV1,
    job: &PalwFreePromptJobV3,
    binding: &PalwStepBindingV2,
    ids: &[u32],
    own: &[u32],
    table: Arc<dyn PalwPublicTokenTableSourceV1>,
    ledger: &Arc<PalwMemoryLedgerV1>,
) -> Result<Option<TiledPlanV1>, String> {
    use kaspa_consensus_core::palw_decode_constraint_v1::constraint_start_state_v1;
    palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
    let _scratch = reserve_v1(target, job, &[], (ids.len() as u64 + own.len() as u64) * 4, table.as_ref(), ledger)?;
    if palw_fp_constraint_table_root_v1(job) != Some(table.root_v1())
        || job.tokenizer_id != table.tokenizer_id_v1()
        || table.vocab_v1() != binding.shape_profile.vocab_size
    {
        return Err("streaming Constraint mask changes immutable root/tokenizer/vocabulary".into());
    }
    let grammar = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_constraint_of_bytes_v1(
        kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_tail_v1(job)
            .ok_or("streaming mask lacks original constraint")?,
    )
    .map_err(|e| e.to_string())?;
    let eog = table.eog_ids_v1()?;
    let admit = |state: &kaspa_consensus_core::palw_decode_constraint_v1::PalwConstraintStateV1, id: u32| {
        if eog.binary_search(&id).is_ok() { Ok(None) } else { table.admit_v1(&grammar, state, id) }
    };
    let mut state = constraint_start_state_v1(&grammar);
    let mut same = true;
    for (position, &committed) in ids.iter().enumerate() {
        if committed >= table.vocab_v1() {
            return Ok(Some(TiledPlanV1 { position: position as u32, committed, witness: None, beat_lane: None }));
        }
        if let Some(next) = admit(&state, committed)? {
            if same
                && let Some(&own) = own.get(position)
                && own != committed
                && own < table.vocab_v1()
                && admit(&state, own)?.is_some()
            {
                return Ok(Some(TiledPlanV1 { position: position as u32, committed, witness: None, beat_lane: Some(own) }));
            }
            same &= own.get(position) == Some(&committed);
            state = next;
        } else {
            let mut witness = None;
            for id in 0..table.vocab_v1() {
                if admit(&state, id)?.is_some() {
                    witness = Some(id);
                    break;
                }
            }
            if Some(&committed) == eog.first() && witness.is_none() && position + 1 == ids.len() {
                return Ok(None);
            }
            return Ok(Some(TiledPlanV1 { position: position as u32, committed, witness, beat_lane: None }));
        }
    }
    Ok(None)
}
use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryReservationKeyV1, PalwMemoryReservationV1};
use kaspa_consensus_core::palw_public_token_table_v1::PalwPublicTokenTableSourceV1;
use kaspa_consensus_core::{
    Hash64,
    palw_constraint_stream_carriage_v1::{
        PalwConstraintStreamActionV1, PalwConstraintStreamLeafV1, PalwConstraintStreamMoveV1, PalwConstraintStreamSeedV1,
    },
    palw_constraint_stream_court_v1::{PalwConstraintStreamCourtV1, PalwConstraintStreamPhaseV1},
    palw_eog_stream_v1::PALW_EOG_STREAM_PART_IDS_V1,
    palw_fp_constraint_job_v1::{PALW_TOKEN_TABLE_MAX_LEAVES_V1, palw_fp_constraint_table_root_v1},
    palw_freeprompt_v3::PalwFreePromptJobV3,
    palw_offence_attribution_v1::{PalwOffenceTargetV1, palw_constrained_job_target_v1},
    palw_rendering_resources_v1::PalwRenderingClaimTermsV1,
    palw_rendering_stream_v1::PALW_RENDERING_STREAM_PART_BYTES_V1,
    palw_staged_court_state_v1::{PalwStagedCertificateRefV1, PalwStagedOwnedCursorV1, PalwStagedOwnedProgressV1},
    palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1, step_opening_root_capped_v1},
    palw_step_refute::{
        PALW_LOGITS_TILE_LANES, PalwTiledDecodePinV1, PalwTraceEventDisclosureV1, tiled_row_authenticate_v1,
        tiled_tile_authenticate_v1,
    },
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ConstraintScheduleV1 {
    Seed(PalwConstraintStreamSeedV1),
    Leaf(PalwConstraintStreamLeafV1),
    Move(PalwConstraintStreamMoveV1),
    Finished(Option<PalwStagedCertificateRefV1>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_memory_ledger::PalwMemoryPoolV1;
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1;
    use kaspa_consensus_core::{
        palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1,
        palw_decode_constraint_v1::*,
        palw_fp_constraint_job_v1::{PALW_FP_CONSTRAINT_VERSION, PalwFpConstraintTailV1},
        palw_freeprompt_v3::PalwFpJobTailV1,
        palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1},
        palw_state_v2::PalwBondKeyV2,
        palw_step_refute::{logits_event_disclosure_v1, tiled_logits_scheme_id_v1, tiled_logits_trace_root_v1},
        tx::TransactionOutpoint,
    };

    type Fixture = (
        PalwOffenceTargetV1,
        PalwFreePromptJobV3,
        TiledPlanV1,
        Vec<PalwTraceEventDisclosureV1>,
        Arc<PalwTokenTableV1>,
        PalwStagedOwnedProgressV1,
    );

    fn fixture(rendering_bytes: usize, beaten: bool) -> Fixture {
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let (mut job, prompt) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
        let run = backend.execute_free_prompt(&job, &prompt.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        let mut binding = backend.disclose_trace_event(&run.outcome.material, 0, 0).unwrap().binding().clone();
        let table = Arc::new(PalwTokenTableV1 {
            entries: vec![Some(b"a".to_vec()), Some(vec![if beaten { b'a' } else { b'b' }; rendering_bytes]), None],
            eog_token_ids: vec![2],
            tokenizer_id: job.tokenizer_id,
        });
        let grammar = PalwDecodeConstraintV1 {
            version: 1,
            compiler_id: Hash64::from_u64_word(0xC014),
            start_frame: 0,
            frames: vec![PalwConstraintFrameV1 {
                start: 0,
                nodes: vec![PalwConstraintNodeV1 {
                    accepting: true,
                    edges: vec![PalwConstraintEdgeV1 { lo: b'a', hi: b'a', action: PalwConstraintActionV1::Goto(0) }],
                }],
            }],
        };
        binding.shape_profile.logits_scheme_id = tiled_logits_scheme_id_v1();
        binding.shape_profile.vocab_size = 3;
        job.version = PALW_FP_CONSTRAINT_VERSION;
        job.class_id = binding.shape_profile.shape_profile_id();
        job.decode = None;
        job.tail =
            Some(PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 { constraint: grammar.to_bytes(), table_root: table.root() }));
        let class = kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3 {
            model_profile_id: binding.job_context.model_profile_id,
            runtime_manifest_hash: binding.job_context.runtime_manifest_hash,
            runtime_class_id: binding.job_context.runtime_class_id,
            shape_profile_id: job.class_id,
            cu_ruleset_id: binding.job_context.cu_ruleset_id,
        };
        binding.job_context = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_context_v3(
            &job,
            &class,
            &kaspa_consensus_core::palw_fp_execution_v3::palw_fp_run_facts_for_executed_v1(&job, job.decode_token_limit),
            &binding.job_context.network_id,
        )
        .unwrap();
        binding.activation_leg_root =
            kaspa_consensus_core::palw_attempt_rules_v1::palw_int_activation_leg_root_v1(&binding.job_context);
        let mut ids = vec![0; job.decode_token_limit as usize];
        assert!(ids.len() > 2);
        ids[2] = 1;
        let rows = vec![vec![20, 10, 0]; ids.len()];
        binding.full_logits_trace_root = tiled_logits_trace_root_v1(&binding.job_context, &rows, &ids).unwrap();
        binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&binding);
        kaspa_consensus_core::palw_step_leg::verify_binding_v1(&binding).unwrap();
        let target = PalwOffenceTargetV1 {
            claim_id: Hash64::from_u64_word(0xC014),
            class_id: job.class_id,
            artifact_root: Hash64::default(),
            executor_bond: PalwBondKeyV2(job.executor_bond),
            execution_root: binding.committed_execution_root,
            lane: Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwClaimSourceKindV1::FreePrompt),
            segment_count: Some(4),
            phase: None,
            job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context),
            trace_root: binding.full_logits_trace_root,
            output_root: kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, &ids),
            registered_semantics: None,
        };
        let events = vec![logits_event_disclosure_v1(&binding, &rows, &ids, 2, 0).unwrap()];
        let plan = TiledPlanV1 { position: 2, committed: 1, witness: (!beaten).then_some(0), beat_lane: beaten.then_some(0) };
        let prices = PalwStagedPublicCourtConfigV1::network_v1().prices;
        let terms =
            PalwRenderingClaimTermsV1::derive(100, PalwRenderingResourcePlanV1::derive(4095, 3, prices).unwrap(), 100, 100, 10)
                .unwrap();
        let progress = PalwStagedOwnedProgressV1 {
            snapshot_block: Hash64::from_u64_word(100),
            now_daa: 100,
            claim: target.claim_id,
            reserver: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(99), 0)),
            reserved_daa: 100,
            terms,
            prefix_custodians: None,
            prefix_page_root: None,
            bundle: None,
            seed_id: None,
            cursor: None,
        };
        (target, job, plan, events, table, progress)
    }

    fn ledger(bytes: u64) -> Arc<PalwMemoryLedgerV1> {
        PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(bytes), || None)
    }

    fn accepted(progress: &mut PalwStagedOwnedProgressV1, row: &PalwOwnedConstraintStreamV1) {
        progress.seed_id = Some(row.seed_id());
        progress.cursor = Some(PalwStagedOwnedCursorV1::Constraint {
            phase: row.court().phase(),
            eog_offset: row.court().eog_offset_v1(),
            rendering: row.court().rendering_cursor_v1(),
            next_rendering_id: row.court().next_rendering_id().ok(),
            completed: row.certificate().ok().map(|f| f.is_some()),
        });
    }

    #[test]
    fn generic_constraint_streams_large_rendering_to_exact_rooted_fault_and_restarts() {
        let (target, job, plan, events, table, mut progress) = fixture(4 * 65536 + 17, false);
        let pool = ledger(64 << 20);
        let maximum = events[0].binding().step_leaf_count.max(1 << 22);
        let open = |p: &PalwStagedOwnedProgressV1| {
            ConstraintSchedulerV1::open(&target, &job, plan, &events, table.clone(), p, maximum, &pool).unwrap()
        };
        let mut scheduler = open(&progress);
        let ConstraintScheduleV1::Seed(mut seed) = scheduler.poll(&progress).unwrap() else { panic!("initial signed seed") };
        assert!(seed.job.is_none(), "the exact original job is read from accepted metadata, not duplicated in each carrier");
        seed.signature = vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
        let mut row = PalwOwnedConstraintStreamV1::admit_with_resolved_binding_v1(
            &seed,
            &job,
            target.execution_root,
            progress.terms,
            100,
            progress.terms.plan.rent_sompi + progress.terms.plan.deposit_sompi,
            90_000,
            maximum,
            progress.terms.material_until_daa,
            progress.terms.bond_exit_after_daa,
            &[progress.terms.bond_exit_after_daa],
            events[0].binding(),
        )
        .unwrap();
        accepted(&mut progress, &row);
        let mut restarted = false;
        let mut parts = 0;
        for _ in 0..100 {
            let decision = scheduler.poll(&progress).unwrap();
            assert_eq!(scheduler.poll(&progress).unwrap(), decision, "unaccepted transport does not advance an actor");
            match decision {
                ConstraintScheduleV1::Leaf(mut leaf) => {
                    leaf.signature = vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
                    row.leaf(progress.now_daa, &leaf).unwrap();
                }
                ConstraintScheduleV1::Move(mut movement) => {
                    if let PalwConstraintStreamActionV1::RenderingPart { bytes, .. } = &movement.action {
                        assert!(bytes.len() <= 65536);
                        parts += 1;
                    }
                    movement.signature = vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
                    row.movement(progress.now_daa, &movement).unwrap();
                }
                ConstraintScheduleV1::Finished(Some(reference)) => {
                    assert_eq!(
                        (reference.claim, reference.reserver, reference.reserved_daa, reference.seed_id),
                        (target.claim_id, progress.reserver, 100, row.seed_id())
                    );
                    assert!(row.certificate().unwrap().is_some());
                    assert!(parts >= 5 && restarted);
                    return;
                }
                _ => panic!("this public forbidden-token fixture must reach objective completion"),
            }
            accepted(&mut progress, &row);
            if parts == 2 && !restarted {
                drop(scheduler);
                scheduler =
                    ConstraintSchedulerV1::reopen(&target, &job, events[0].binding(), &seed, table.clone(), &progress, maximum, &pool)
                        .unwrap();
                restarted = true;
            }
        }
        panic!("bounded constraint stream did not complete");
    }

    #[test]
    fn generic_constraint_refuses_wrong_public_pin_table_owner_and_original_clock() {
        let (target, job, plan, events, table, progress) = fixture(8, true);
        let pool = ledger(64 << 20);
        let maximum = 1 << 22;
        let open = |target: &PalwOffenceTargetV1,
                    events: &[PalwTraceEventDisclosureV1],
                    table: Arc<PalwTokenTableV1>,
                    p: &PalwStagedOwnedProgressV1| {
            ConstraintSchedulerV1::open(target, &job, plan, events, table, p, maximum, &pool)
        };
        let mut wrong = target.clone();
        wrong.execution_root = Hash64::default();
        assert!(open(&wrong, &events, table.clone(), &progress).is_err());
        let mut wrong_events = events.clone();
        let PalwTraceEventDisclosureV1::Tiled { tile_lanes, .. } = &mut wrong_events[0] else { unreachable!() };
        tile_lanes[0] += 1;
        assert!(open(&target, &wrong_events, table.clone(), &progress).is_err());
        let mut wrong_table = table.as_ref().clone();
        wrong_table.entries[1] = Some(b"forged".to_vec());
        assert!(open(&target, &events, Arc::new(wrong_table), &progress).is_err());
        let mut ended = progress.clone();
        ended.now_daa = progress.terms.open_until_daa;
        assert!(open(&target, &events, table.clone(), &ended).is_err());
        let mut scheduler = open(&target, &events, table.clone(), &progress).unwrap();
        let mut substituted = progress.clone();
        substituted.reserver = target.executor_bond;
        assert!(scheduler.poll(&substituted).is_err());
        let mut substituted = progress.clone();
        substituted.terms.collect_until_daa += 1;
        assert!(scheduler.poll(&substituted).is_err());
        assert!(
            ConstraintSchedulerV1::open(&target, &job, plan, &events, table, &progress, maximum, &ledger(1)).is_err(),
            "unfunded public work remains Pending"
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cursor {
    phase: PalwConstraintStreamPhaseV1,
    eog: Option<u32>,
    rendering: Option<(u32, bool, u32, u32)>,
    next: Option<u32>,
    completed: Option<bool>,
}

struct Pending {
    before: Cursor,
    after: Cursor,
    decision: ConstraintScheduleV1,
}

pub(super) struct ConstraintSchedulerV1 {
    seed: PalwConstraintStreamSeedV1,
    seed_id: Hash64,
    position: u32,
    terms: PalwRenderingClaimTermsV1,
    table: Arc<dyn PalwPublicTokenTableSourceV1>,
    eog: Vec<u32>,
    court: PalwConstraintStreamCourtV1,
    moves: u64,
    pending: Option<Pending>,
    _reservation: Arc<PalwMemoryReservationV1>,
}

fn reserve_v1(
    target: &PalwOffenceTargetV1,
    job: &PalwFreePromptJobV3,
    events: &[PalwTraceEventDisclosureV1],
    extra_bytes: u64,
    table: &dyn PalwPublicTokenTableSourceV1,
    ledger: &Arc<PalwMemoryLedgerV1>,
) -> Result<Arc<PalwMemoryReservationV1>, String> {
    let encoded = (borsh::object_length(job).map_err(|e| e.to_string())? as u64)
        .checked_add(borsh::object_length(&events).map_err(|e| e.to_string())? as u64)
        .and_then(|n| n.checked_add(extra_bytes))
        .ok_or("constraint public copy bound overflow")?;
    let bytes = encoded
        .checked_mul(8)
        .and_then(|n| n.checked_add((table.vocab_v1() as usize as u64).checked_mul(256)?))
        .and_then(|n| n.checked_add((table.eog_count_v1() as usize as u64).checked_mul(32)?))
        .and_then(|n| n.checked_add(1_048_576))
        .ok_or("constraint public scratch bound overflow")?;
    Ok(Arc::new(
        ledger
            .reserve(
                PalwMemoryReservationKeyV1 { role: "public-constraint-staged", class_id: target.class_id, job: target.claim_id },
                bytes,
            )
            .map_err(|e| e.to_string())?,
    ))
}

/// The original u8 Event transport is not used to authenticate a wide selected tile.
/// Both its u32 tile number and row root come from the original Merkle opening.
fn pin_v1(
    target: &PalwOffenceTargetV1,
    job: &PalwFreePromptJobV3,
    plan: TiledPlanV1,
    events: &[PalwTraceEventDisclosureV1],
    maximum_leaves: u64,
) -> Result<(PalwStepBindingV2, PalwTiledDecodePinV1), String> {
    let first = events.first().ok_or("selected public row is missing")?;
    let binding = first.binding();
    palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
    let PalwTraceEventDisclosureV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening, .. } = first else {
        return Err("constraint stream requires the authenticated tiled selector format".into());
    };
    let (context, vocab, tiles) =
        tiled_row_authenticate_v1(binding, generated_token_ids, plan.position, row_root, row_opening, maximum_leaves)
            .map_err(|e| e.to_string())?;
    if generated_token_ids.get(plan.position as usize) != Some(&plan.committed) {
        return Err("selected row changes the nominated committed token".into());
    }
    let check_tile = |event: &PalwTraceEventDisclosureV1, tile: u64| -> Result<(), String> {
        let PalwTraceEventDisclosureV1::Tiled {
            binding: other,
            generated_token_ids: ids,
            row_root: root,
            row_opening: opening,
            tile_lanes,
            tile_opening,
            ..
        } = event
        else {
            return Err("selected tile has another public format".into());
        };
        if other.as_ref() != binding || ids != generated_token_ids || root != row_root || opening != row_opening {
            return Err("selected tiles change immutable binding/generated row".into());
        }
        tiled_tile_authenticate_v1(&context, plan.position, vocab, tiles, row_root, tile, tile_lanes, tile_opening, maximum_leaves)
            .map_err(|e| e.to_string())
    };
    let committed_tile = u64::from(plan.committed) / PALW_LOGITS_TILE_LANES as u64;
    let mut pin = PalwTiledDecodePinV1 {
        position: plan.position,
        generated_token_ids: generated_token_ids.clone(),
        row_root: *row_root,
        row_opening: row_opening.clone(),
        committed_tile_lanes: Vec::new(),
        committed_opening: row_opening.clone(),
        beat_tile_lanes: Vec::new(),
        beat_opening: row_opening.clone(),
        beat_lane: plan.beat_lane.unwrap_or(0),
    };
    if let Some(beat) = plan.beat_lane {
        check_tile(first, committed_tile)?;
        let beat_tile = u64::from(beat) / PALW_LOGITS_TILE_LANES as u64;
        let count = if beat_tile == committed_tile { 1 } else { 2 };
        if events.len() != count {
            return Err("selected beating tiles are incomplete or redundant".into());
        }
        let last = events.last().ok_or("beating tile missing")?;
        check_tile(last, beat_tile)?;
        let PalwTraceEventDisclosureV1::Tiled { tile_lanes: beats, tile_opening: opening, .. } = last else { unreachable!() };
        pin.committed_tile_lanes = tile_lanes.clone();
        pin.committed_opening = tile_opening.clone();
        pin.beat_tile_lanes = beats.clone();
        pin.beat_opening = opening.clone();
    } else {
        if events.len() != 1 {
            return Err("forbidden/stop proof needs exactly one original selected row".into());
        }
        check_tile(first, 0)?;
    }
    Ok((binding.clone(), pin))
}

fn opening_v1(table: &dyn PalwPublicTokenTableSourceV1, id: u32) -> Result<PalwStepOpeningV1, String> {
    table.checked_opening_v1(id)
}

impl ConstraintSchedulerV1 {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn open(
        target: &PalwOffenceTargetV1,
        job: &PalwFreePromptJobV3,
        plan: TiledPlanV1,
        events: &[PalwTraceEventDisclosureV1],
        table: Arc<dyn PalwPublicTokenTableSourceV1>,
        progress: &PalwStagedOwnedProgressV1,
        maximum_leaves: u64,
        ledger: &Arc<PalwMemoryLedgerV1>,
    ) -> Result<Self, String> {
        if progress.cursor.as_ref().is_some_and(|c| !matches!(c, PalwStagedOwnedCursorV1::Constraint { .. })) {
            return Err("another immutable family occupies this owner court".into());
        }
        if progress.claim != target.claim_id
            || progress.reserver == target.executor_bond
            || progress.terms.accepted_daa > progress.reserved_daa
            || progress.reserved_daa > progress.now_daa
            || progress.now_daa
                >= if progress.cursor.is_none() { progress.terms.open_until_daa } else { progress.terms.collect_until_daa }
        {
            return Err("constraint owner/acceptance/collection clock is not admissible".into());
        }
        let reservation = reserve_v1(target, job, events, 0, table.as_ref(), ledger)?;
        let (binding, pin) = pin_v1(target, job, plan, events, maximum_leaves)?;
        if table.tokenizer_id_v1() != job.tokenizer_id || table.vocab_v1() != binding.shape_profile.vocab_size {
            return Err("constraint holding changes registered tokenizer/vocabulary".into());
        }
        let root = palw_fp_constraint_table_root_v1(job).ok_or("job names no constraint table")?;
        let eog_opening = opening_v1(table.as_ref(), table.vocab_v1())?;
        if step_opening_root_capped_v1(table.vocab_v1() as usize as u64 + 1, &eog_opening, PALW_TOKEN_TABLE_MAX_LEAVES_V1)
            .map_err(|e| e.to_string())?
            != root
        {
            return Err("constraint holding is not authenticated by the immutable job table root".into());
        }
        if let Some(registered) = target.registered_semantics.as_ref() {
            if !registered.matches_context(&binding.job_context) || registered.token_table_root != Some(root) {
                return Err("constraint table differs from immutable registered semantics".into());
            }
        }
        let mut eog = table.eog_ids_v1()?;
        eog.sort_unstable();
        if eog.is_empty() || eog.windows(2).any(|w| w[0] >= w[1]) || eog.last().is_some_and(|id| *id >= table.vocab_v1()) {
            return Err("constraint EOG holding is not a canonical bounded set".into());
        }
        let court = PalwConstraintStreamCourtV1::new(
            target.claim_id,
            progress.reserver,
            progress.reserved_daa,
            target.execution_root,
            &binding,
            job,
            &pin,
            plan.witness,
            eog.len() as u32,
            &eog_opening,
            maximum_leaves,
        )?;
        let mut seed = PalwConstraintStreamSeedV1 {
            version: 1,
            claim: target.claim_id,
            reserver: progress.reserver,
            reserved_daa: progress.reserved_daa,
            binding: binding.clone(),
            job: None,
            pin,
            witness: plan.witness,
            eog_count: eog.len() as u32,
            eog_opening,
            auxiliary: None,
            signature: Vec::new(),
            retained_profile: None,
        };
        if borsh::object_length(&seed)
            .map_err(|e| e.to_string())?
            .checked_add(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN)
            .ok_or("constraint seed mass overflows")?
            > 90_000
        {
            seed.use_retained_profile_v1()?;
        }
        if borsh::object_length(&seed)
            .map_err(|e| e.to_string())?
            .checked_add(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN)
            .ok_or("constraint seed mass overflows")?
            > 90_000
        {
            return Err("authenticated constraint seed exceeds its fixed carrier envelope".into());
        }
        // The native row's seed id is its court's identity: the signed carrier digest only when the
        // profile is retained, otherwise the court's canonical context. Rebuild the local court the
        // way native admission does so both the id and every streamed context agree with it.
        let court = seed.court_with_resolved_binding_v1(target.execution_root, maximum_leaves, job, &binding)?;
        let seed_id = court.seed_id();
        if progress.seed_id.is_some_and(|id| id != seed_id) {
            return Err("accepted constraint seed differs; replacement is forbidden".into());
        }
        Ok(Self {
            seed,
            seed_id,
            position: plan.position,
            terms: progress.terms,
            table,
            eog,
            court,
            moves: 1,
            pending: None,
            _reservation: reservation,
        })
    }

    /// An accepted seed is read from the rooted public archive, not nominated again after
    /// restart. A reconstructed checkpoint never supplies admission authority.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn open_from_flat_v1(
        target: &PalwOffenceTargetV1,
        job: &PalwFreePromptJobV3,
        binding: &PalwStepBindingV2,
        pin: &kaspa_consensus_core::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1,
        source_id: Hash64,
        answerer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        witness: Option<u32>,
        original: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        table: Arc<dyn PalwPublicTokenTableSourceV1>,
        progress: &PalwStagedOwnedProgressV1,
        ledger: &Arc<PalwMemoryLedgerV1>,
    ) -> Result<Self, String> {
        if progress.cursor.is_some()
            || progress.seed_id.is_some()
            || progress.claim != target.claim_id
            || progress.reserver == target.executor_bond
            || progress.terms.accepted_daa > progress.reserved_daa
            || progress.reserved_daa > progress.now_daa
            || progress.now_daa >= progress.terms.open_until_daa
            || !pin.matches_binding_v1(binding)
        {
            return Err("flat constraint opening changes paid owner/source/original clock".into());
        }
        let extra = borsh::object_length(binding).map_err(|e| e.to_string())? as u64;
        let reservation = reserve_v1(target, job, &[], extra, table.as_ref(), ledger)?;
        palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
        if table.tokenizer_id_v1() != job.tokenizer_id || table.vocab_v1() != binding.shape_profile.vocab_size {
            return Err("flat constraint table differs from original registered tokenizer/vocabulary".into());
        }
        let root = palw_fp_constraint_table_root_v1(job).ok_or("flat constraint job lacks immutable table root")?;
        let eog_opening = opening_v1(table.as_ref(), table.vocab_v1())?;
        if step_opening_root_capped_v1(table.vocab_v1() as usize as u64 + 1, &eog_opening, PALW_TOKEN_TABLE_MAX_LEAVES_V1)
            .map_err(|e| e.to_string())?
            != root
        {
            return Err("flat constraint EOG holding differs from original table commitment".into());
        }
        let mut eog = table.eog_ids_v1()?;
        eog.sort_unstable();
        if eog.is_empty() || eog.windows(2).any(|w| w[0] >= w[1]) || eog.last().is_some_and(|id| *id >= table.vocab_v1()) {
            return Err("flat constraint EOG holding is not a canonical set".into());
        }
        let court = PalwConstraintStreamCourtV1::new_from_flat_pin_with_original_stop_context_v1(
            target.claim_id,
            progress.reserver,
            progress.reserved_daa,
            target.execution_root,
            binding,
            job,
            pin,
            witness,
            eog.len() as u32,
            &eog_opening,
            original,
            None,
        )?;
        let empty = || PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::default(), siblings: Vec::new() };
        let mut seed = PalwConstraintStreamSeedV1 {
            version: 1,
            claim: target.claim_id,
            reserver: progress.reserver,
            reserved_daa: progress.reserved_daa,
            binding: binding.clone(),
            job: None,
            pin: PalwTiledDecodePinV1 {
                position: 0,
                generated_token_ids: Vec::new(),
                row_root: Hash64::default(),
                row_opening: empty(),
                committed_tile_lanes: Vec::new(),
                committed_opening: empty(),
                beat_tile_lanes: Vec::new(),
                beat_opening: empty(),
                beat_lane: 0,
            },
            witness,
            eog_count: eog.len() as u32,
            eog_opening,
            auxiliary: Some(
                kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FlatConstraint {
                    source_id,
                    answerer,
                },
            ),
            signature: Vec::new(),
            retained_profile: None,
        };
        if borsh::object_length(&seed).map_err(|e| e.to_string())? + kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            > 90_000
        {
            seed.use_retained_profile_v1()?;
        }
        if borsh::object_length(&seed).map_err(|e| e.to_string())? + kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            > 90_000
        {
            return Err("flat semantic seed exceeds original fixed signed carrier".into());
        }
        let seed_id = Hash64::from_bytes(seed.signing_message().try_into().map_err(|_| "flat semantic seed digest width")?);
        Ok(Self {
            seed,
            seed_id,
            position: pin.position_v1(),
            terms: progress.terms,
            table,
            eog,
            court,
            moves: 1,
            pending: None,
            _reservation: reservation,
        })
    }

    /// Test-only legacy restart entry; the service reopens through `reopen_with_flat_v1`.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reopen(
        target: &PalwOffenceTargetV1,
        job: &PalwFreePromptJobV3,
        binding: &PalwStepBindingV2,
        seed: &PalwConstraintStreamSeedV1,
        table: Arc<dyn PalwPublicTokenTableSourceV1>,
        progress: &PalwStagedOwnedProgressV1,
        maximum_leaves: u64,
        ledger: &Arc<PalwMemoryLedgerV1>,
    ) -> Result<Self, String> {
        Self::reopen_with_flat_v1(target, job, binding, seed, None, None, table, progress, maximum_leaves, ledger)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn reopen_with_flat_v1(
        target: &PalwOffenceTargetV1,
        job: &PalwFreePromptJobV3,
        binding: &PalwStepBindingV2,
        seed: &PalwConstraintStreamSeedV1,
        flat: Option<&kaspa_consensus_core::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1>,
        original: Option<&kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3>,
        table: Arc<dyn PalwPublicTokenTableSourceV1>,
        progress: &PalwStagedOwnedProgressV1,
        maximum_leaves: u64,
        ledger: &Arc<PalwMemoryLedgerV1>,
    ) -> Result<Self, String> {
        let digest = Hash64::from_bytes(seed.signing_message().try_into().map_err(|_| "constraint seed identity width")?);
        if progress.seed_id.is_none()
            || !matches!(progress.cursor, Some(PalwStagedOwnedCursorV1::Constraint { .. }))
            || (seed.claim, seed.reserver, seed.reserved_daa) != (progress.claim, progress.reserver, progress.reserved_daa)
            || target.claim_id != progress.claim
            || progress.reserver == target.executor_bond
            || progress.now_daa < seed.reserved_daa
            || progress.now_daa >= progress.terms.collect_until_daa
            || match &seed.auxiliary {
                None => flat.is_some(),
                Some(kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FlatConstraint {
                    ..
                }) => flat.is_none(),
                Some(_) => true,
            }
            || seed.job.as_deref().is_some_and(|j| j != job)
        {
            return Err("public constraint archive changes admitted seed/owner/job/clock".into());
        }
        let extra = borsh::object_length(&(binding, seed)).map_err(|e| e.to_string())? as u64;
        let reservation = reserve_v1(target, job, &[], extra, table.as_ref(), ledger)?;
        seed.check(90_000)?;
        palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
        if seed.resolved_binding_v1(target.class_id, Some(&binding.shape_profile))? != *binding
            || table.tokenizer_id_v1() != job.tokenizer_id
            || table.vocab_v1() != binding.shape_profile.vocab_size
        {
            return Err("archived constraint binding differs from accepted immutable class/holding".into());
        }
        let root = palw_fp_constraint_table_root_v1(job).ok_or("original job has no table root")?;
        let actual_eog = opening_v1(table.as_ref(), table.vocab_v1())?;
        if actual_eog != seed.eog_opening
            || step_opening_root_capped_v1(table.vocab_v1() as usize as u64 + 1, &actual_eog, PALW_TOKEN_TABLE_MAX_LEAVES_V1)
                .map_err(|e| e.to_string())?
                != root
        {
            return Err("archive/table EOG opening changes original authenticated table".into());
        }
        let mut eog = table.eog_ids_v1()?;
        eog.sort_unstable();
        if eog.len() != seed.eog_count as usize
            || eog.windows(2).any(|w| w[0] >= w[1])
            || eog.is_empty()
            || eog.last().is_some_and(|id| *id >= table.vocab_v1())
        {
            return Err("archived EOG count differs from authenticated holding".into());
        }
        let court = if let Some(flat) = flat {
            PalwConstraintStreamCourtV1::new_from_flat_pin_with_original_stop_context_v1(
                target.claim_id,
                seed.reserver,
                seed.reserved_daa,
                target.execution_root,
                binding,
                job,
                flat,
                seed.witness,
                seed.eog_count,
                &seed.eog_opening,
                original.ok_or("Flat archive lost its native original commitment")?,
                Some(digest),
            )?
        } else {
            seed.court_with_resolved_binding_v1(target.execution_root, maximum_leaves, job, binding)?
        };
        // Same identity rule as native admission (see `open`).
        let seed_id = court.seed_id();
        if progress.seed_id != Some(seed_id) {
            return Err("public constraint archive changes admitted seed identity".into());
        }
        Ok(Self {
            seed: seed.clone(),
            seed_id,
            position: flat.map_or(seed.pin.position, |pin| pin.position_v1()),
            terms: progress.terms,
            table,
            eog,
            court,
            moves: 1,
            pending: None,
            _reservation: reservation,
        })
    }

    fn cursor(&self) -> Cursor {
        Cursor {
            phase: self.court.phase(),
            eog: self.court.eog_offset_v1(),
            rendering: self.court.rendering_cursor_v1(),
            next: self.court.next_rendering_id().ok(),
            completed: self.court.fault().ok().map(|f| f.is_some()),
        }
    }

    fn generate(&mut self) -> Result<Pending, String> {
        if self.moves >= self.terms.plan.maximum_moves {
            return Err("constraint move exceeds original cumulative paid work".into());
        }
        let before = self.cursor();
        let local = self.court.seed_id();
        let phase = before.phase;
        let action = if phase == PalwConstraintStreamPhaseV1::Eog {
            let offset = before.eog.ok_or("accepted EOG offset missing")?;
            let rest = self.eog.get(offset as usize..).ok_or("EOG cursor exceeds exact authenticated set")?;
            if rest.is_empty() {
                self.court.finish_eog()?;
                PalwConstraintStreamActionV1::EogFinish
            } else {
                let ids = rest[..rest.len().min(PALW_EOG_STREAM_PART_IDS_V1)].to_vec();
                self.court.push_eog(local, offset, &ids)?;
                PalwConstraintStreamActionV1::EogPart { offset, ids }
            }
        } else if phase == PalwConstraintStreamPhaseV1::Complete {
            return Err("completed constraint requires accepted certificate, not more semantic work".into());
        } else if let Some((id, present, length, offset)) = before.rendering {
            let actual = self.table.entry_length_v1(id)?;
            if present != actual.is_some() || length != actual.unwrap_or(0) || offset > length {
                return Err("rendering declaration changes immutable table leaf".into());
            }
            if offset == length {
                self.court.finish_rendering()?;
                PalwConstraintStreamActionV1::RenderingFinish
            } else {
                let bytes = self.table.read_part_v1(id, offset, PALW_RENDERING_STREAM_PART_BYTES_V1 as u32)?;
                if bytes.len() != (length - offset).min(PALW_RENDERING_STREAM_PART_BYTES_V1 as u32) as usize {
                    return Err("public rendering read changes exact native part length".into());
                }
                self.court.push_rendering(local, offset, &bytes)?;
                PalwConstraintStreamActionV1::RenderingPart { offset, bytes }
            }
        } else {
            let id = before.next.ok_or("accepted constraint phase has no forced next ID")?;
            if self.eog.binary_search(&id).is_ok() {
                self.court.finish_eog_token(local)?;
                PalwConstraintStreamActionV1::EogTokenFinish
            } else {
                let entry = self.table.entry_length_v1(id)?;
                let present = entry.is_some();
                let length = entry.unwrap_or(0);
                let opening = opening_v1(self.table.as_ref(), id)?;
                self.court.begin_rendering(local, id, present, length, &opening)?;
                self.moves += 1;
                return Ok(Pending {
                    before,
                    after: self.cursor(),
                    decision: ConstraintScheduleV1::Leaf(PalwConstraintStreamLeafV1 {
                        version: 1,
                        claim: self.seed.claim,
                        reserver: self.seed.reserver,
                        reserved_daa: self.seed.reserved_daa,
                        seed_id: self.seed_id,
                        id,
                        present,
                        length,
                        opening,
                        signature: Vec::new(),
                    }),
                });
            }
        };
        self.moves += 1;
        Ok(Pending {
            before,
            after: self.cursor(),
            decision: ConstraintScheduleV1::Move(PalwConstraintStreamMoveV1 {
                version: 1,
                claim: self.seed.claim,
                reserver: self.seed.reserver,
                reserved_daa: self.seed.reserved_daa,
                seed_id: self.seed_id,
                phase,
                action,
                signature: Vec::new(),
            }),
        })
    }

    pub(super) fn poll(&mut self, progress: &PalwStagedOwnedProgressV1) -> Result<ConstraintScheduleV1, String> {
        if (progress.claim, progress.reserver, progress.reserved_daa, progress.terms)
            != (self.seed.claim, self.seed.reserver, self.seed.reserved_daa, self.terms)
            || progress.now_daa < self.seed.reserved_daa
            || progress.now_daa >= self.terms.collect_until_daa
        {
            return Err("constraint accepted owner or original immutable clock changed".into());
        }
        let Some(cursor) = &progress.cursor else {
            if progress.seed_id.is_some() || progress.now_daa >= self.terms.open_until_daa {
                return Err("constraint initial opening door is closed".into());
            }
            return Ok(ConstraintScheduleV1::Seed(self.seed.clone()));
        };
        if progress.seed_id != Some(self.seed_id) {
            return Err("constraint accepted seed changed".into());
        }
        let PalwStagedOwnedCursorV1::Constraint { phase, eog_offset, rendering, next_rendering_id, completed } = cursor else {
            return Err("another immutable family owns the accepted cursor".into());
        };
        let accepted =
            Cursor { phase: *phase, eog: *eog_offset, rendering: *rendering, next: *next_rendering_id, completed: *completed };
        if let Some(pending) = &self.pending {
            if pending.before == accepted {
                if progress.now_daa >= self.terms.stream_until_daa {
                    return Err("constraint streaming door is closed".into());
                }
                return Ok(pending.decision.clone());
            }
            if pending.after != accepted {
                return Err("constraint branch/cursor changed; reconstruct from authenticated original seed".into());
            }
            self.pending = None;
        }
        while self.cursor() != accepted {
            let step = self.generate()?;
            if self.rank(&step.after) > self.rank(&accepted)
                || (step.after.phase == accepted.phase && step.after.eog > accepted.eog)
                || (step.after.phase == accepted.phase
                    && step.after.rendering.is_some_and(|r| accepted.rendering.is_none_or(|a| r != a && r.3 > a.3)))
            {
                return Err("constraint accepted cursor is not derived by exact original public operands".into());
            }
        }
        if let Some(fault) = accepted.completed {
            return Ok(ConstraintScheduleV1::Finished(fault.then_some(PalwStagedCertificateRefV1 {
                claim: self.seed.claim,
                reserver: self.seed.reserver,
                reserved_daa: self.seed.reserved_daa,
                seed_id: self.seed_id,
            })));
        }
        if progress.now_daa >= self.terms.stream_until_daa {
            return Err("constraint streaming door is closed".into());
        }
        self.pending = Some(self.generate()?);
        Ok(self.pending.as_ref().expect("generated").decision.clone())
    }

    fn rank(&self, cursor: &Cursor) -> u64 {
        match cursor.phase {
            PalwConstraintStreamPhaseV1::Eog => 0,
            PalwConstraintStreamPhaseV1::Prefix(p) => u64::from(p) + 1,
            PalwConstraintStreamPhaseV1::Committed => u64::from(self.position) + 1,
            PalwConstraintStreamPhaseV1::Auxiliary => u64::from(self.position) + 2,
            PalwConstraintStreamPhaseV1::Complete => u64::from(self.position) + 3,
            _ => u64::MAX,
        }
    }
}
