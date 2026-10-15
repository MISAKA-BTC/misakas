//! Native seed-admission, collection and journal regressions over accepted public
//! class/job/custody fixtures. Outer signature, transaction fees and service E2E
//! remain separate tests; synthetic accepted rows do not prove those workflows.
use super::*;
use crate::palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1 as Action, PalwConstraintStreamMoveV1};
use crate::palw_prefix_public_owned_v1::tests::{bond, fixture};
use crate::palw_staged_court_state_v1::*;

fn seed_state(
    signed_seats: u64,
) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1)
{
    use crate::palw_constraint_stream_carriage_v1::{PalwAuxiliaryPublicCourtSeedV1 as Auxiliary, PalwConstraintStreamSeedV1};
    let (mut state, p, mut extras, claim, owner) = state();
    // The native public-accuser lane needs the network's split collateral ceiling;
    // the general pure-work fixture's 1000-permille ceiling leaves no accuser room.
    let p = p.with_fp_exposure_ceiling(500).unwrap();
    let (target, binding, job, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(true, false);
    assert_eq!(claim, target.claim_id);
    state.staged_courts.remove(&PalwStagedCourtKeyV1::Owner(claim, owner));
    let mut class = state.classes.values().next().unwrap().clone();
    class.artifact_root = target.artifact_root;
    state.classes.insert(target.class_id, class);
    let live = state.claims.get_mut(&claim).unwrap();
    live.class_id = target.class_id;
    live.accepted_daa = 100;
    live.source = PalwClaimSourceV2::FreePrompt { quanta: 0, spent: Default::default() };
    live.execution_root = target.execution_root;
    live.job_identity = target.job_identity;
    live.trace_root = target.trace_root;
    live.output_root = target.output_root;
    state.claim_semantics.insert(claim, target.registered_semantics.clone().unwrap());
    state.class_semantics.insert(target.class_id, target.registered_semantics.clone().unwrap());
    state.fp_work_profiles.insert(target.class_id, Box::new(binding.shape_profile.clone()));
    let commitment = crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 {
        job: job.clone(),
        trace_root: target.trace_root,
        output_root: target.output_root,
        schedule_root: ZERO_HASH64,
        execution_root: target.execution_root,
        decode_tokens_executed: 2,
        stop_reason: crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached,
        work_leaves: binding.step_leaf_count,
        trace_manifest_root: ZERO_HASH64,
        trace_chunk_count: 1,
        trace_retention_daa: live.trace_retention_daa,
    };
    state.fp_public_jobs.insert(claim, borsh::to_vec(&commitment).unwrap());
    let PalwStagedCourtRowV1::Claim(contract) = state.staged_courts.get_mut(&PalwStagedCourtKeyV1::Claim(claim)).unwrap() else {
        unreachable!()
    };
    let mut custody = crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1::new(
        target.execution_root,
        target.job_identity,
        binding.job_context.cu_ruleset_id,
        &contract.terms,
    );
    custody.expected_panel = contract.panel.clone();
    for n in 1..=signed_seats {
        assert!(custody.qualify(
            claim,
            &crate::palw_panel_v2::PalwSeatReceiptV2 {
                claim,
                seat_bond: bond(n + 7),
                signed_daa: 100,
                verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
                signature: vec![n as u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
            100
        ));
    }
    contract.prefix_custody = Some(custody);
    state.bonds.get_mut(&owner).unwrap().collateral = u64::MAX / 2;
    extras.held_context_ladder = Some(1 << 32);
    extras.shard_court_ladder = Some(1 << 32);
    palw_staged_court_fold_v1::rebuild_indices(&mut state);
    let opening = crate::palw_step_leg::PalwStepOpeningV1 { leaf_index: 0, leaf_hash: ZERO_HASH64, siblings: vec![] };
    let seed = PalwConstraintStreamSeedV1 {
        version: 1,
        claim,
        reserver: owner,
        reserved_daa: 100,
        binding,
        job: None,
        pin: crate::palw_step_refute::PalwTiledDecodePinV1 {
            position: 0,
            generated_token_ids: vec![],
            row_root: ZERO_HASH64,
            row_opening: opening.clone(),
            committed_tile_lanes: vec![],
            committed_opening: opening.clone(),
            beat_tile_lanes: vec![],
            beat_opening: opening.clone(),
            beat_lane: 0,
        },
        witness: None,
        eog_count: 0,
        eog_opening: opening,
        auxiliary: Some(Auxiliary::PublicPrefix {
            maximum_leaves: 1 << 32,
            prefix_tokens: 2,
            relation: target.registered_semantics.unwrap().prefix_state_relation.unwrap(),
            slots: signed_seats as u32 + 2,
        }),
        signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        retained_profile: None,
    };
    (state, p, extras, seed)
}

#[test]
fn public_prefix_seed_admission_is_independent_exact_funded_and_not_vetoed_by_unsigned_panel() {
    use crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1 as Auxiliary;
    for signed in [0, 1, 5] {
        let (state, p, extras, seed) = seed_state(signed);
        let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
        palw_staged_court_fold_v1::open(&mut builder, &ctx(2, 101, 101), &seed).unwrap();
        let key = PalwStagedCourtKeyV1::Owner(seed.claim, seed.reserver);
        let PalwStagedCourtRowV1::PublicPrefix(row) = &builder.state.staged_courts[&key] else { unreachable!() };
        row.check().unwrap();
        assert!(row.deposit_locked > 0);
        assert!(row.rent_paid > 0);
        let after = builder.state.state_root();
        assert!(palw_staged_court_fold_v1::open(&mut builder, &ctx(3, 102, 102), &seed).is_err());
        assert_eq!(builder.state.state_root(), after);
        let mut replayed = state.clone();
        for entry in &builder.entries {
            apply_delta_entry(&mut replayed, entry, false).unwrap();
        }
        assert_eq!(replayed.state_root(), after);
        assert_eq!(replayed.staged_due, builder.state.staged_due);
        for entry in builder.entries.iter().rev() {
            apply_delta_entry(&mut replayed, entry, true).unwrap();
        }
        assert_eq!(replayed.state_root(), state.state_root());
        assert_eq!(replayed.staged_due, state.staged_due);
        for bad in 0..7 {
            let mut altered = seed.clone();
            let mut parent = state.clone();
            match bad {
                0 => {
                    if let Some(Auxiliary::PublicPrefix { slots, .. }) = &mut altered.auxiliary {
                        *slots += 1;
                    }
                }
                1 => {
                    if let Some(Auxiliary::PublicPrefix { prefix_tokens, .. }) = &mut altered.auxiliary {
                        *prefix_tokens += 1;
                    }
                }
                2 => {
                    if let Some(Auxiliary::PublicPrefix { maximum_leaves, .. }) = &mut altered.auxiliary {
                        *maximum_leaves -= 1;
                    }
                }
                3 => {
                    parent.bonds.get_mut(&seed.reserver).unwrap().collateral = 0;
                }
                4 => {
                    parent.fp_public_jobs.remove(&seed.claim);
                }
                5 | 6 => {
                    altered.reserver = if bad == 5 { producer() } else { bond(8) };
                    let reservation = parent.legacy_disputes[&seed.claim].live[&seed.reserver].clone();
                    parent.legacy_disputes.get_mut(&seed.claim).unwrap().live.insert(altered.reserver, reservation);
                    parent.bonds.get_mut(&altered.reserver).unwrap().collateral = u64::MAX / 2;
                }
                _ => unreachable!(),
            }
            let mut rejected = TransitionBuilder::new(&parent, &p, false, false, false, true, &extras);
            assert!(palw_staged_court_fold_v1::open(&mut rejected, &ctx(2, 101, 101), &altered).is_err(), "invalid case {bad}");
            assert_eq!(rejected.state.state_root(), parent.state_root());
            assert!(rejected.entries.is_empty());
        }
    }
}

fn producer() -> PalwBondKeyV2 {
    crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(true, false).0.executor_bond
}

fn state() -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, Hash64, PalwBondKeyV2) {
    let (row, _) = fixture();
    let (target, _, _, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(true, false);
    let claim = row.claim;
    let owner = row.reserver;
    let p = params().with_rcore_plus_mirrors(Some(0), 12_900, vec![h64(0x2B)]);
    let (licensed, old_claim, _, _) = f2_licensed_false_execution(&p);
    let mut state = PalwChainStateV2::genesis();
    state.classes = licensed.classes.clone();
    let mut live = licensed.claim(&old_claim).unwrap().clone();
    live.bond = producer();
    live.execution_root = target.execution_root;
    live.job_identity = target.job_identity;
    live.trace_root = target.trace_root;
    live.output_root = target.output_root;
    live.class_id = target.class_id;
    let mut class = licensed.classes.values().next().unwrap().clone();
    class.artifact_root = target.artifact_root;
    state.classes.insert(target.class_id, class);
    state.claim_semantics.insert(claim, target.registered_semantics.clone().unwrap());
    live.reserved = 200;
    live.immature_contribution = 0;
    live.escrowed_reward = 20;
    live.trace_retention_daa = row.terms.material_until_daa;
    state.claims.insert(claim, live);
    let template = licensed.bond(&bond_key(1)).unwrap().clone();
    state.bonds.insert(producer(), template.clone());
    state.reserved_exposure.insert(producer(), 200);
    for n in 0..=12 {
        let mut record = template.clone();
        record.collateral = 1_000_000;
        state.bonds.insert(bond(n), record);
    }
    for n in 8..=12 {
        let template = licensed.slashable_locks.values().next().copied().unwrap();
        let mut lock = template;
        lock.amount = 10;
        lock.expiry_daa = row.terms.bond_exit_after_daa;
        state.slashable_locks.insert((bond(n), claim), lock);
    }
    state.last_point = Some(ctx(1, 100, 100));
    state.staged_courts.insert(
        PalwStagedCourtKeyV1::Claim(claim),
        PalwStagedCourtRowV1::Claim(PalwStagedClaimContractV1 {
            terms: row.terms,
            producer: producer(),
            panel: (8..=12).map(bond).collect(),
            prefix_custody: None,
        }),
    );
    state.staged_courts.insert(PalwStagedCourtKeyV1::Owner(claim, owner), PalwStagedCourtRowV1::PublicPrefix(Box::new(row).into()));
    let mut record =
        crate::palw_legacy_public_filer_v1::PalwDisputeClaimV1 { opened_daa: 100, hard_deadline_daa: u64::MAX, ..Default::default() };
    record.live.insert(
        owner,
        crate::palw_legacy_public_filer_v1::PalwDisputeReservationRowV1 {
            reserved_daa: 100,
            deposit: 1,
            sessions_opened: 0,
            lanes_joined: 0,
        },
    );
    state.legacy_disputes.insert(claim, record);
    palw_staged_court_fold_v1::rebuild_indices(&mut state);
    let extras = PalwTransitionExtrasV1 {
        staged_public_court: Some(crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1()),
        ..f2_extras()
    };
    (state, p, extras, claim, owner)
}

#[test]
fn public_prefix_demand_is_rooted_reversible_and_defaults_collect_from_all_literal_custodians_once() {
    let (state, p, extras, claim, owner) = state();
    let key = PalwStagedCourtKeyV1::Owner(claim, owner);
    let PalwStagedCourtRowV1::PublicPrefix(row) = &state.staged_courts[&key] else { unreachable!() };
    let demand = row.next_demand_v1(101).unwrap().unwrap();
    let deadline = demand.deadline_daa;
    let movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim,
        reserver: owner,
        reserved_daa: 100,
        seed_id: row.seed_id(),
        phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::PrefixPublicDemand(demand),
        signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(2, 101, 101), &movement).unwrap();
    assert!(builder.state.staged_due.contains(&(deadline, key)));
    assert_eq!(builder.state.staged_prefix_responders.len(), 7);
    let demand_entry = builder.entries.last().unwrap().clone();
    let mut replayed = state.clone();
    apply_delta_entry(&mut replayed, &demand_entry, false).unwrap();
    assert_eq!(replayed.state_root(), builder.state.state_root());
    assert_eq!(replayed.staged_due, builder.state.staged_due);
    assert_eq!(replayed.staged_prefix_responders, builder.state.staged_prefix_responders);
    apply_delta_entry(&mut replayed, &demand_entry, true).unwrap();
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_due, state.staged_due);
    assert_eq!(replayed.staged_prefix_responders, state.staged_prefix_responders);
    // Restoring a persisted mutable checkpoint must rebuild the exact pending clock.
    let bytes = borsh::to_vec(&builder.state.staged_courts[&key]).unwrap();
    let restored: PalwStagedCourtRowV1 = borsh::from_slice(&bytes).unwrap();
    restored.check(&key).unwrap();
    let before = builder.state.clone();
    palw_staged_court_fold_v1::sweep(&mut builder, &ctx(3, deadline - 1, deadline - 1)).unwrap();
    assert_eq!(builder.state.state_root(), before.state_root(), "the last response DAA is still open");
    palw_staged_court_fold_v1::sweep(&mut builder, &ctx(4, deadline, deadline)).unwrap();
    assert!(builder.state.bond(&producer()).unwrap().collateral < before.bond(&producer()).unwrap().collateral);
    for n in 8..=12 {
        assert!(builder.state.bond(&bond(n)).unwrap().collateral < before.bond(&bond(n)).unwrap().collateral);
    }
    assert_eq!(builder.state.bond(&owner).unwrap().collateral, before.bond(&owner).unwrap().collateral);
    let offence = crate::palw_da_rcore_v1::palw_da_offence_id_v1(&producer().0, &claim);
    assert!(builder.state.consumed_offence(&offence).unwrap().collected > 0);
    assert!(builder.state.staged_prefix_responders.is_empty());
    let after = builder.state.state_root();
    palw_staged_court_fold_v1::sweep(&mut builder, &ctx(5, deadline + 2, deadline + 2)).unwrap();
    assert_eq!(builder.state.state_root(), after, "a default cannot collect twice");
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), after);
    assert_eq!(replayed.staged_due, builder.state.staged_due);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_due, state.staged_due);
}

#[test]
fn public_prefix_answer_advances_only_after_original_leaf_eof_and_restores_the_pruning_clock() {
    use crate::palw_prefix_public_source_v1::{PalwPrefixPublicAnswerActionV1 as AnswerAction, PalwPrefixPublicAnswerV1};
    let (state, p, extras, claim, owner) = state();
    let (_, tiles) = fixture();
    let key = PalwStagedCourtKeyV1::Owner(claim, owner);
    let PalwStagedCourtRowV1::PublicPrefix(row) = &state.staged_courts[&key] else { unreachable!() };
    let demand = row.next_demand_v1(101).unwrap().unwrap();
    let mut movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim,
        reserver: owner,
        reserved_daa: 100,
        seed_id: row.seed_id(),
        phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::PrefixPublicDemand(demand.clone()),
        signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let id = Hash64::from_bytes(movement.signing_message().try_into().unwrap());
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(2, 101, 101), &movement).unwrap();
    let tile = &tiles[0];
    assert_eq!(tile.preimage.coord, demand.coordinate);
    for (n, action) in [
        AnswerAction::Begin { opening: tile.opening.clone() },
        AnswerAction::Part { offset: 0, bytes: tile.preimage.values_le.clone() },
        AnswerAction::Finish,
    ]
    .into_iter()
    .enumerate()
    {
        movement.action = Action::PrefixPublicAnswer(PalwPrefixPublicAnswerV1 {
            answerer: producer(),
            cursor_id: demand.cursor_id,
            demand_id: id,
            action,
        });
        assert_eq!(movement.authorization_bond_v1(), producer());
        palw_staged_court_fold_v1::movement(&mut builder, &ctx(3 + n as u64, 102 + n as u64, 102 + n as u64), &movement).unwrap();
        let PalwStagedCourtRowV1::PublicPrefix(row) = &builder.state.staged_courts[&key] else { unreachable!() };
        if n < 2 {
            assert_eq!(builder.state.staged_prefix_responders.len(), 7);
            assert!(row.pending_progress_v1().is_some());
            assert!(builder.state.staged_due.contains(&(demand.deadline_daa, key)));
        } else {
            assert!(builder.state.staged_prefix_responders.is_empty());
            assert!(row.pending_progress_v1().is_none());
            assert!(builder.state.staged_due.contains(&(row.terms.material_until_daa, key)));
            assert!(!builder.state.staged_due.contains(&(demand.deadline_daa, key)));
            assert_ne!(row.next_demand_v1(105).unwrap().unwrap().coordinate, demand.coordinate);
        }
    }
    let before = builder.state.state_root();
    assert!(
        palw_staged_court_fold_v1::movement(&mut builder, &ctx(6, 105, 105), &movement).is_err(),
        "an old signed finish cannot advance another source"
    );
    assert_eq!(builder.state.state_root(), before);
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), before);
    assert_eq!(replayed.staged_due, builder.state.staged_due);
    assert_eq!(replayed.staged_prefix_responders, builder.state.staged_prefix_responders);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_due, state.staged_due);
    assert_eq!(replayed.staged_prefix_responders, state.staged_prefix_responders);
}

#[test]
fn targeted_public_collection_bypasses_maintenance_backlog_and_is_identity_bound_once() {
    let (state, p, extras, claim, owner) = state();
    let key = PalwStagedCourtKeyV1::Owner(claim, owner);
    let PalwStagedCourtRowV1::PublicPrefix(row) = &state.staged_courts[&key] else { unreachable!() };
    let demand = row.next_demand_v1(101).unwrap().unwrap();
    let deadline = demand.deadline_daa;
    let mut movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim,
        reserver: owner,
        reserved_daa: 100,
        seed_id: row.seed_id(),
        phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::PrefixPublicDemand(demand.clone()),
        signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let demand_id = Hash64::from_bytes(movement.signing_message().try_into().unwrap());
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(2, 101, 101), &movement).unwrap();
    for n in 1..=65 {
        let mut contract = builder.state.staged_claim_contract_v1(&claim).unwrap().clone();
        contract.terms.bond_exit_after_daa = deadline;
        builder.write_staged_row_v1(PalwStagedCourtKeyV1::Claim(h64(10_000 + n)), Some(PalwStagedCourtRowV1::Claim(contract)));
    }
    palw_staged_court_fold_v1::sweep(&mut builder, &ctx(3, deadline + 1, deadline + 1)).unwrap();
    assert!(builder.state.consumed_offences.is_empty(), "maintenance deliberately has not reached the target");
    movement.action = Action::PrefixPublicCollect { cursor_id: demand.cursor_id, demand_id };
    let before = builder.state.clone();
    assert!(palw_staged_court_fold_v1::movement(&mut builder, &ctx(4, deadline - 1, deadline - 1), &movement).is_err());
    assert_eq!(builder.state.state_root(), before.state_root());
    let mut wrong = movement.clone();
    wrong.action = Action::PrefixPublicCollect { cursor_id: demand.cursor_id, demand_id: h64(0xbad) };
    assert!(palw_staged_court_fold_v1::movement(&mut builder, &ctx(4, deadline + 1, deadline + 1), &wrong).is_err());
    assert_eq!(builder.state.state_root(), before.state_root());
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(4, deadline + 1, deadline + 1), &movement).unwrap();
    assert!(builder.state.bond(&producer()).unwrap().collateral < before.bond(&producer()).unwrap().collateral);
    for n in 8..=12 {
        assert!(builder.state.bond(&bond(n)).unwrap().collateral < before.bond(&bond(n)).unwrap().collateral);
    }
    let after = builder.state.state_root();
    assert!(palw_staged_court_fold_v1::movement(&mut builder, &ctx(5, deadline + 2, deadline + 2), &movement).is_err());
    assert_eq!(builder.state.state_root(), after);
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), after);
    assert_eq!(replayed.staged_due, builder.state.staged_due);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_due, state.staged_due);
}

#[test]
fn expired_public_collection_preserves_pending_material_but_never_revives_collection() {
    let (state, p, extras, claim, owner) = state();
    let key = PalwStagedCourtKeyV1::Owner(claim, owner);
    let PalwStagedCourtRowV1::PublicPrefix(row) = &state.staged_courts[&key] else { unreachable!() };
    let demand = row.next_demand_v1(101).unwrap().unwrap();
    let until = row.terms.collect_until_daa;
    let material = row.terms.material_until_daa;
    let mut movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim,
        reserver: owner,
        reserved_daa: 100,
        seed_id: row.seed_id(),
        phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::PrefixPublicDemand(demand.clone()),
        signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let demand_id = Hash64::from_bytes(movement.signing_message().try_into().unwrap());
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(2, 101, 101), &movement).unwrap();
    palw_staged_court_fold_v1::sweep(&mut builder, &ctx(3, until, until)).unwrap();
    let PalwStagedCourtRowV1::PublicPrefix(row) = &builder.state.staged_courts[&key] else { unreachable!() };
    assert!(row.pending_progress_v1().is_some());
    assert!(row.pending_deadline_v1().is_none());
    assert!(builder.state.staged_due.contains(&(material, key)));
    movement.action = Action::PrefixPublicCollect { cursor_id: demand.cursor_id, demand_id };
    let after = builder.state.state_root();
    assert!(palw_staged_court_fold_v1::movement(&mut builder, &ctx(4, until, until), &movement).is_err());
    assert_eq!(builder.state.state_root(), after);
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), after);
    assert_eq!(replayed.staged_due, builder.state.staged_due);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_due, state.staged_due);
    palw_staged_court_fold_v1::sweep(&mut builder, &ctx(5, material, material)).unwrap();
    assert!(!builder.state.staged_courts.contains_key(&key));
}

#[test]
fn public_collection_can_finish_after_stream_horizon_but_not_at_collection_expiry() {
    let (state, p, extras, claim, owner) = state();
    let key = PalwStagedCourtKeyV1::Owner(claim, owner);
    let PalwStagedCourtRowV1::PublicPrefix(row) = &state.staged_courts[&key] else { unreachable!() };
    let demand = row.next_demand_v1(101).unwrap().unwrap();
    let stream_until = row.terms.stream_until_daa;
    let collect_until = row.terms.collect_until_daa;
    assert!(demand.deadline_daa < stream_until && stream_until < collect_until);
    let mut movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim,
        reserver: owner,
        reserved_daa: 100,
        seed_id: row.seed_id(),
        phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::PrefixPublicDemand(demand.clone()),
        signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let demand_id = Hash64::from_bytes(movement.signing_message().try_into().unwrap());
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(2, 101, 101), &movement).unwrap();
    movement.action = Action::PrefixPublicCollect { cursor_id: demand.cursor_id, demand_id };
    let before = builder.state.clone();
    assert!(palw_staged_court_fold_v1::movement(&mut builder, &ctx(3, collect_until, collect_until), &movement).is_err());
    assert_eq!(builder.state.state_root(), before.state_root());
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(3, collect_until - 1, collect_until - 1), &movement).unwrap();
    assert!(builder.state.bond(&producer()).unwrap().collateral < before.bond(&producer()).unwrap().collateral);
    let after = builder.state.state_root();
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), after);
    assert_eq!(replayed.staged_due, builder.state.staged_due);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_due, state.staged_due);
}

#[test]
fn independently_voided_public_claim_keeps_paid_material_without_a_second_collection() {
    let (state, p, extras, claim, owner) = state();
    let key = PalwStagedCourtKeyV1::Owner(claim, owner);
    let PalwStagedCourtRowV1::PublicPrefix(row) = &state.staged_courts[&key] else { unreachable!() };
    let demand = row.next_demand_v1(101).unwrap().unwrap();
    let deadline = demand.deadline_daa;
    let material = row.terms.material_until_daa;
    let mut movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim,
        reserver: owner,
        reserved_daa: 100,
        seed_id: row.seed_id(),
        phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::PrefixPublicDemand(demand.clone()),
        signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let demand_id = Hash64::from_bytes(movement.signing_message().try_into().unwrap());
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(2, 101, 101), &movement).unwrap();
    let mut live = builder.state.claim(&claim).unwrap().clone();
    live.phase = PalwClaimPhaseV2::Voided { voided_daa: 102, reason: PalwVoidReasonV2::CourtFraud };
    builder.write_claim(claim, Some(live));
    let before = builder.state.clone();
    palw_staged_court_fold_v1::sweep(&mut builder, &ctx(3, deadline, deadline)).unwrap();
    assert_eq!(builder.state.bonds, before.bonds, "another conviction does not collect this owner's default");
    assert_eq!(builder.state.consumed_offences, before.consumed_offences);
    let PalwStagedCourtRowV1::PublicPrefix(row) = &builder.state.staged_courts[&key] else { unreachable!() };
    assert!(row.pending_progress_v1().is_some());
    assert!(row.pending_deadline_v1().is_none());
    assert!(row.joint_default_v1(deadline).unwrap().is_none());
    assert!(builder.state.staged_due.contains(&(material, key)));
    assert_eq!(builder.state.staged_retirement_floor_v1(&claim, 103), material);
    assert!(builder.state.staged_bond_exit_held_v1(&producer(), deadline));
    let encoded = borsh::to_vec(&builder.state.staged_courts[&key]).unwrap();
    let restored: PalwStagedCourtRowV1 = borsh::from_slice(&encoded).unwrap();
    restored.check(&key).unwrap();
    assert_eq!(restored, builder.state.staged_courts[&key]);
    movement.action = Action::PrefixPublicCollect { cursor_id: demand.cursor_id, demand_id };
    let after = builder.state.state_root();
    assert!(palw_staged_court_fold_v1::movement(&mut builder, &ctx(4, deadline + 1, deadline + 1), &movement).is_err());
    assert_eq!(builder.state.state_root(), after);
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), after);
    assert_eq!(replayed.staged_due, builder.state.staged_due);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_due, state.staged_due);
    palw_staged_court_fold_v1::sweep(&mut builder, &ctx(5, material, material)).unwrap();
    assert!(!builder.state.staged_courts.contains_key(&key));
}

#[test]
fn prospective_prefix_acceptance_prices_complete_acquisition_and_refuses_overflow_before_writes() {
    let (mut state, p, mut extras, seed) = seed_state(5);
    extras.model_registry = Some(crate::palw_model_registry_v1::PalwModelRegistryFoldV1 {
        globals: crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1,
        span_daa: 100,
        genesis_works: Default::default(),
        grace_until_daa: 0,
        admission_audit_period_daa: None,
        readiness_v2_active: false,
        bond_maturity: None,
    });
    let commit: crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 = borsh::from_slice(&state.fp_public_jobs[&seed.claim]).unwrap();
    let live = state.claim(&seed.claim).unwrap().clone();
    let header = crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Legacy(
        crate::palw_public_binding_v1::PalwPublicLegacyBindingHeaderV1::of_binding_v1(&seed.binding),
    );
    extras.public_original_bindings.insert(seed.claim, header.clone());
    let maximum_custodians = state.staged_maximum_custodians_v1(live.class_id, live.accepted_daa, &p, &extras).unwrap();
    let quote = state
        .staged_preclaim_quote_v1(
            &commit,
            &header,
            live.accepted_daa,
            maximum_custodians,
            1 << 32,
            extras.staged_public_court.unwrap(),
        )
        .unwrap()
        .unwrap();
    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
    state.bonds.get_mut(&live.bond).unwrap().pubkey = keypair.verification_key.as_ref().to_vec();
    let mut pledge = crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1 {
        version: 1,
        claim: seed.claim,
        producer: live.bond,
        execution_root: live.execution_root,
        job_identity: live.job_identity,
        cu_ruleset_id: seed.binding.job_context.cu_ruleset_id,
        resource_plan_id: quote.resource_plan_id,
        maximum_records: quote.maximum_records,
        maximum_custodians: quote.maximum_custodians,
        accept_until_daa: live.accepted_daa + 10,
        material_until_daa: quote.plan.terms.material_until_daa,
        signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    pledge.signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &keypair.signing_key,
        pledge.pledge_id_v1().as_byte_slice(),
        crate::palw_history_custody_v1::PALW_HISTORY_PLEDGE_CONTEXT_V1,
        [0; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    extras.history_custody_pledges.insert(seed.claim, pledge.clone());
    let builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    let mut admitted = live.clone();
    let contract = builder.prepare_staged_claim_with_job_v1(seed.claim, &mut admitted, Some(&commit)).unwrap().unwrap();
    let contract = match &contract {
        PalwStagedCourtRowV1::Claim(contract) => contract,
        PalwStagedCourtRowV1::ClaimBundle(contract) => &contract.claim,
        _ => unreachable!(),
    };
    let prices = extras.staged_public_court.unwrap().prices;
    let base = crate::palw_prefix_resources_v1::PalwPrefixResourcePlanV1::derive(
        &seed.binding.shape_profile,
        crate::palw_class_semantics_v1::PalwPrefixStateRelationV1::Base0Int8KvV1,
        2,
        seed.binding.step_leaf_count,
        90_000,
        prices,
    )
    .unwrap();
    let complete = crate::palw_prefix_public_source_v1::PalwPrefixPublicAcquisitionPlanV1::derive(
        base,
        7,
        prices,
        borsh::object_length(&seed.binding.shape_profile).unwrap() as u64,
    )
    .unwrap();
    assert!(contract.terms.plan.maximum_moves >= complete.envelope.maximum_moves);
    assert!(contract.terms.plan.maximum_parser_steps >= complete.envelope.maximum_parser_steps);
    assert!(contract.terms.plan.maximum_stream_bytes >= complete.envelope.maximum_stream_bytes);
    assert!(contract.terms.plan.rent_sompi >= complete.envelope.rent_sompi);
    assert!(contract.terms.plan.deposit_sompi >= complete.envelope.deposit_sompi);
    assert!(admitted.trace_retention_daa >= contract.terms.material_until_daa);
    assert_eq!(builder.state.state_root(), state.state_root());
    assert!(builder.entries.is_empty());
    for bad in 0..4 {
        let mut altered = extras.clone();
        match bad {
            0 => {
                altered.public_original_bindings.remove(&seed.claim);
            }
            1 => {
                altered.history_custody_pledges.remove(&seed.claim);
            }
            2 => {
                altered.history_custody_pledges.get_mut(&seed.claim).unwrap().signature[0] ^= 1;
            }
            _ => {
                altered.history_custody_pledges.get_mut(&seed.claim).unwrap().resource_plan_id = h64(123456);
            }
        }
        let negative = TransitionBuilder::new(&state, &p, false, false, false, true, &altered);
        let mut unchanged = live.clone();
        assert!(negative.prepare_staged_claim_with_job_v1(seed.claim, &mut unchanged, Some(&commit)).is_err());
        assert_eq!(unchanged, live);
        assert!(negative.entries.is_empty());
    }
    extras.staged_public_court.as_mut().unwrap().prices.rent_per_parser_step = u64::MAX;
    let rejected = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    let mut unchanged = live.clone();
    assert!(rejected.prepare_staged_claim_with_job_v1(seed.claim, &mut unchanged, Some(&commit)).is_err());
    assert_eq!(unchanged, live);
    assert_eq!(rejected.state.state_root(), state.state_root());
    assert!(rejected.entries.is_empty());
    extras.staged_public_court = None;
    let dormant = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    assert!(dormant.prepare_staged_claim_with_job_v1(seed.claim, &mut unchanged, Some(&commit)).unwrap().is_none());
    assert_eq!(unchanged, live);
}

#[test]
fn public_response_index_is_bounded_signer_scoped_paged_and_rebuilt_from_literal_rows() {
    use crate::palw_prefix_public_owned_v1::tests::fixture_for_owner;
    let (state, p, extras, claim, _) = state();
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    for n in 100..=164 {
        let owner = bond(n);
        let (row, _) = fixture_for_owner(owner);
        let reservation = builder.state.legacy_disputes[&claim].live[&bond(7)].clone();
        builder.state.legacy_disputes.get_mut(&claim).unwrap().live.insert(owner, reservation);
        let demand = row.next_demand_v1(101).unwrap().unwrap();
        let movement = PalwConstraintStreamMoveV1 {
            version: 1,
            claim,
            reserver: owner,
            reserved_daa: 100,
            seed_id: row.seed_id(),
            phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
            action: Action::PrefixPublicDemand(demand),
            signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        builder.write_staged_row_v1(
            PalwStagedCourtKeyV1::Owner(claim, owner),
            Some(PalwStagedCourtRowV1::PublicPrefix(Box::new(row).into())),
        );
        palw_staged_court_fold_v1::movement(&mut builder, &ctx(2, 101, 101), &movement).unwrap();
    }
    let indexed = builder.state.staged_prefix_responders.clone();
    assert_eq!(indexed.len(), 65 * 7);
    let (page, next) = builder.state.staged_public_prefix_duties_after_v1(producer(), 100, None, usize::MAX).unwrap();
    assert_eq!(page.len(), 64);
    assert!(next.is_some());
    let (last, done) = builder.state.staged_public_prefix_duties_after_v1(producer(), 100, next, 64).unwrap();
    assert_eq!(last.len(), 1);
    assert!(done.is_none());
    assert_ne!(page.last().unwrap().reserver, last[0].reserver);
    let (outsider, done) = builder.state.staged_public_prefix_duties_after_v1(bond(9999), 100, None, 64).unwrap();
    assert!(outsider.is_empty() && done.is_none());
    let (own, done) = builder.state.staged_public_prefix_duties_after_v1(bond(100), 100, None, 64).unwrap();
    assert_eq!(own.len(), 1);
    assert!(done.is_none());
    assert!(builder.state.staged_public_prefix_duties_after_v1(producer(), 101, None, 64).is_err());
    assert!(builder.state.staged_public_prefix_duties_after_v1(producer(), 100, None, 0).is_err());
    let root = builder.state.state_root();
    builder.state.staged_prefix_responders.clear();
    assert_eq!(builder.state.state_root(), root, "derived index is not another rooted signer copy");
    palw_staged_court_fold_v1::rebuild_indices(&mut builder.state);
    assert_eq!(builder.state.staged_prefix_responders, indexed);
    let mut checkpoint = PalwChainStateV2::genesis();
    checkpoint.staged_courts = builder.state.staged_courts.clone();
    checkpoint.last_point = state.last_point;
    palw_staged_court_fold_v1::rebuild_indices(&mut checkpoint);
    let checkpoint_root = checkpoint.state_root();
    let restored = borsh::from_slice::<PalwStateCarriageV2>(&borsh::to_vec(&PalwStateCarriageV2::from_state(&checkpoint)).unwrap())
        .unwrap()
        .into_state(&p, Some(checkpoint_root))
        .unwrap();
    assert_eq!(restored.staged_prefix_responders, indexed);
    assert_eq!(restored.state_root(), checkpoint_root);
    let mut replayed = state.clone();
    // Reservation fixtures above are outside this index journal regression.
    replayed.legacy_disputes = builder.state.legacy_disputes.clone();
    let baseline = replayed.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), root);
    assert_eq!(replayed.staged_prefix_responders, indexed);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), baseline.state_root());
    assert_eq!(replayed.staged_prefix_responders, baseline.staged_prefix_responders);
    let first = *indexed.first().unwrap();
    builder.state.staged_prefix_responders.remove(&first);
    builder.state.staged_prefix_responders.insert((first.0, first.1 - 1, first.2, first.3));
    assert!(builder.state.staged_public_prefix_duties_after_v1(first.0, 100, None, 64).is_err(), "stale index is not authority");
}

#[test]
fn flat_response_index_pages_literal_phase_deadlines_and_roundtrips_seed_journals() {
    use crate::palw_staged_bundle_v1::*;
    let (state, p, extras, claim, _) = state();
    let config = extras.staged_public_court.unwrap();
    let (binding, accusation) = crate::palw_decode_constraint_v1::tests::public_flat_constrained_case_for_tests_v1(false);
    let semantic =
        crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1::derive(4095, binding.shape_profile.vocab_size, config.prices)
            .unwrap();
    let source_quote = crate::palw_flat_constraint_stream_v1::flat_constraint_source_resource_plan_v1(
        binding.job_context.exact_decode_tokens,
        binding.shape_profile.vocab_size,
        borsh::object_length(&binding.shape_profile).unwrap() as u64,
        2,
        binding.step_leaf_count,
        config.prices,
    )
    .unwrap();
    let acquisition = staged_context_resource_plan_v1(borsh::object_length(&binding.shape_profile).unwrap() as u64, semantic, config)
        .unwrap()
        .checked_sum_v1(source_quote)
        .unwrap();
    let plan = PalwStagedBundlePlanV1::derive_with_acquisitions_v1(
        100,
        vec![(PalwStagedFamilyV1::Constraint, semantic, Some(acquisition))],
        None,
        config,
    )
    .unwrap();
    let respond = plan.preseed_source_until_v1(PalwStagedFamilyV1::Constraint, config).unwrap();
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    for n in 100..=164 {
        let owner = bond(n);
        let session = crate::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceSessionV1::new_deferred_v1(
            claim,
            owner,
            100,
            plan.id_v1(claim),
            producer(),
            [producer(), bond(8)].into_iter().collect(),
            &binding,
            respond,
            plan.stages[0].terms.collect_until_daa,
            config.prices,
        )
        .unwrap();
        let row = PalwOwnedStagedBundleV1 {
            claim,
            owner,
            reserved_daa: 100,
            execution_root: binding.committed_execution_root,
            plan: plan.clone(),
            inventory: None,
            selected: None,
            selected_context: None,
            flat: Some(Box::new(session).into()),
            flat_context: Some(
                Box::new(PalwSelectedSourceContextV1 { binding: Box::new(binding.clone()), job: Box::new(accusation.job.clone()) })
                    .into(),
            ),
            history_recovery: Default::default(),
            active: None,
            opened: vec![],
        };
        row.check_v1().unwrap();
        builder.write_staged_row_v1(
            PalwStagedCourtKeyV1::Owner(claim, owner),
            Some(PalwStagedCourtRowV1::OwnerBundle(Box::new(row).into())),
        );
    }
    let indexed = builder.state.staged_flat_responders.clone();
    assert_eq!(indexed.len(), 130);
    let (first, next) = builder.state.staged_flat_duties_after_v1(producer(), 100, None, usize::MAX).unwrap();
    assert_eq!(first.len(), 64);
    assert!(next.is_some());
    let (last, done) = builder.state.staged_flat_duties_after_v1(producer(), 100, next, 64).unwrap();
    assert_eq!(last.len(), 1);
    assert!(done.is_none());
    assert!(builder.state.staged_flat_duties_after_v1(bond(999), 100, None, 64).unwrap().0.is_empty());
    assert!(builder.state.staged_flat_duties_after_v1(producer(), 101, None, 64).is_err());
    assert!(builder.state.staged_flat_duties_after_v1(producer(), 100, None, 0).is_err());
    let root = builder.state.state_root();
    builder.state.staged_flat_responders.clear();
    assert_eq!(builder.state.state_root(), root);
    palw_staged_court_fold_v1::rebuild_indices(&mut builder.state);
    assert_eq!(builder.state.staged_flat_responders, indexed);
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), root);
    assert_eq!(replayed.staged_flat_responders, indexed);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_flat_responders, state.staged_flat_responders);
    let original = *indexed.first().unwrap();
    builder.state.staged_flat_responders.remove(&original);
    builder.state.staged_flat_responders.insert((original.0, original.1 - 1, original.2, original.3));
    assert!(builder.state.staged_flat_duties_after_v1(original.0, 100, None, 64).is_err());
}

#[test]
fn prospective_public_job_retention_precedes_the_legacy_fence_and_survives_native_reload() {
    let p = params().with_legacy_public_filer_from_daa(None);
    let (parent, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
    let body = public_fp_job_fixture();
    let id = crate::palw_freeprompt_v3::fp_claim_id_v3(&body);
    let mut object = fp_commit(0xFC, 60, 3);
    let PalwConsensusObjectV2::FreePromptCommitted { claim, job_pin, public_job, prompt_token_ids_hash, prompt_tokens, .. } =
        &mut object
    else {
        unreachable!()
    };
    *prompt_token_ids_hash = body.job.prompt_token_ids_hash;
    *prompt_tokens = body.job.prompt_tokens;
    *claim = id;
    *job_pin = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&body);
    *public_job = Some(Box::new(body.clone()));
    let extras = PalwTransitionExtrasV1 {
        offence_attribution_active: true,
        staged_public_court: Some(crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1()),
        ..Default::default()
    };
    let run = |object: &PalwConsensusObjectV2, extras: &PalwTransitionExtrasV1| {
        apply_palw_transition_v2_with_extras(
            &parent,
            &p,
            &ctx(2, 101, 2),
            std::slice::from_ref(object),
            None,
            false,
            false,
            false,
            true,
            extras,
        )
    };
    let (state, delta) = run(&object, &extras).unwrap();
    let wire = borsh::to_vec(&body).unwrap();
    assert_eq!(state.fp_public_jobs.get(&id), Some(&wire));
    assert!(state.staged_claim_contract_v1(&id).is_some());
    state.assert_public_fp_jobs_consistency_v1(&p).unwrap();
    let restored = borsh::from_slice::<PalwStateCarriageV2>(&borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap())
        .unwrap()
        .into_state(&p, Some(state.state_root()))
        .unwrap();
    assert_eq!(restored.state_root(), state.state_root());
    assert_eq!(restored.fp_public_jobs, state.fp_public_jobs);
    assert_eq!(apply_delta_v2(&parent, &delta, &p).unwrap(), state);
    assert_eq!(revert_delta_v2(&state, &delta, &p).unwrap(), parent);
    let mut missing = object.clone();
    let PalwConsensusObjectV2::FreePromptCommitted { public_job, .. } = &mut missing else { unreachable!() };
    *public_job = None;
    assert!(run(&missing, &extras).is_err());
    let dormant = PalwTransitionExtrasV1 { staged_public_court: None, ..extras };
    let before = run(&object, &dormant).unwrap().0;
    assert!(before.fp_public_jobs.is_empty());
    assert_eq!(before, run(&missing, &dormant).unwrap().0);
}

#[test]
fn shared_owned_rows_preserve_every_old_box_encoding_and_copy_only_the_written_owner() {
    use crate::palw_prefix_public_owned_v1::tests::{fixture_for_owner, inner_fixture};
    let constraint = crate::palw_constraint_stream_owned_v1::tests::completed_row(false).0;
    let tokenizer = crate::palw_tokenizer_owned_court_v1::tests::admitted_case(false, 4096).0;
    let prefix = inner_fixture();
    let public = fixture().0;
    let cases = [
        (1u8, borsh::to_vec(&constraint).unwrap(), PalwStagedCourtRowV1::Constraint(Box::new(constraint).into())),
        (2, borsh::to_vec(&tokenizer).unwrap(), PalwStagedCourtRowV1::Tokenizer(Box::new(tokenizer).into())),
        (3, borsh::to_vec(&prefix).unwrap(), PalwStagedCourtRowV1::Prefix(Box::new(prefix).into())),
        (4, borsh::to_vec(&public).unwrap(), PalwStagedCourtRowV1::PublicPrefix(Box::new(public).into())),
    ];
    fn pointer(row: &PalwStagedCourtRowV1) -> *const () {
        match row {
            PalwStagedCourtRowV1::Constraint(c) => c.as_ref() as *const _ as *const (),
            PalwStagedCourtRowV1::Tokenizer(c) => c.as_ref() as *const _ as *const (),
            PalwStagedCourtRowV1::Prefix(c) => c.as_ref() as *const _ as *const (),
            PalwStagedCourtRowV1::PublicPrefix(c) => c.as_ref() as *const _ as *const (),
            _ => unreachable!(),
        }
    }
    for (tag, body, row) in cases {
        let mut old_box_encoding = vec![tag];
        old_box_encoding.extend(body);
        assert_eq!(borsh::to_vec(&row).unwrap(), old_box_encoding);
        let cloned = row.clone();
        assert_eq!(pointer(&row), pointer(&cloned));
        let restored: PalwStagedCourtRowV1 = borsh::from_slice(&old_box_encoding).unwrap();
        assert_eq!(restored, row);
        assert_ne!(pointer(&row), pointer(&restored));
    }
    let (mut state, p, extras, claim, owner) = state();
    let other = bond(8);
    let other_key = PalwStagedCourtKeyV1::Owner(claim, other);
    let other_row = fixture_for_owner(other).0;
    state.staged_courts.insert(other_key, PalwStagedCourtRowV1::PublicPrefix(Box::new(other_row).into()));
    palw_staged_court_fold_v1::rebuild_indices(&mut state);
    let key = PalwStagedCourtKeyV1::Owner(claim, owner);
    let before = state.state_root();
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    assert_eq!(pointer(&state.staged_courts[&key]), pointer(&builder.state.staged_courts[&key]));
    assert_eq!(pointer(&state.staged_courts[&other_key]), pointer(&builder.state.staged_courts[&other_key]));
    let PalwStagedCourtRowV1::PublicPrefix(row) = &state.staged_courts[&key] else { unreachable!() };
    let movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim,
        reserver: owner,
        reserved_daa: 100,
        seed_id: row.seed_id(),
        phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::PrefixPublicDemand(row.next_demand_v1(101).unwrap().unwrap()),
        signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(2, 101, 101), &movement).unwrap();
    assert_eq!(state.state_root(), before, "COW does not alter the accepted parent snapshot");
    assert_ne!(pointer(&state.staged_courts[&key]), pointer(&builder.state.staged_courts[&key]));
    assert_eq!(pointer(&state.staged_courts[&other_key]), pointer(&builder.state.staged_courts[&other_key]));
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), builder.state.state_root());
    assert_eq!(replayed.staged_prefix_responders, builder.state.staged_prefix_responders);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), before);
    assert_eq!(replayed.staged_prefix_responders, state.staged_prefix_responders);
}

#[test]
fn public_prefix_history_is_literal_success_only_rooted_and_reversible() {
    use crate::palw_prefix_history_v1::{PalwPrefixHistoryRecordV1, PalwPrefixHistoryRequestV1};
    use crate::palw_prefix_public_source_v1::{PalwPrefixPublicAnswerActionV1 as AnswerAction, PalwPrefixPublicAnswerV1};
    let (state, p, extras, claim, owner) = state();
    let (_, tiles) = fixture();
    let key = PalwStagedCourtKeyV1::Owner(claim, owner);
    let PalwStagedCourtRowV1::PublicPrefix(row) = &state.staged_courts[&key] else { unreachable!() };
    assert_eq!(row.history_cursor_v1(), (0, ZERO_HASH64));
    let until = row.terms.material_until_daa;
    let demand = row.next_demand_v1(101).unwrap().unwrap();
    let mut movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim,
        reserver: owner,
        reserved_daa: 100,
        seed_id: row.seed_id(),
        phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::PrefixPublicDemand(demand.clone()),
        signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let demand_id = Hash64::from_bytes(movement.signing_message().try_into().unwrap());
    let point = ctx(2, 101, 101);
    let first = PalwPrefixHistoryRecordV1::new(ZERO_HASH64, 1, point.block, point.daa_score, until, movement.clone()).unwrap();
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    palw_staged_court_fold_v1::movement(&mut builder, &point, &movement).unwrap();
    let PalwStagedCourtRowV1::PublicPrefix(row) = &builder.state.staged_courts[&key] else { unreachable!() };
    assert_eq!(row.history_cursor_v1(), (1, first.head_v1().unwrap()));
    let before = builder.state.state_root();
    assert!(palw_staged_court_fold_v1::movement(&mut builder, &ctx(3, 102, 102), &movement).is_err());
    assert_eq!(builder.state.state_root(), before, "failed duplicate does not append another record");
    let mut previous = first.head_v1().unwrap();
    let tile = &tiles[0];
    for (n, action) in [
        AnswerAction::Begin { opening: tile.opening.clone() },
        AnswerAction::Part { offset: 0, bytes: tile.preimage.values_le.clone() },
        AnswerAction::Finish,
    ]
    .into_iter()
    .enumerate()
    {
        movement.action = Action::PrefixPublicAnswer(PalwPrefixPublicAnswerV1 {
            answerer: producer(),
            cursor_id: demand.cursor_id,
            demand_id,
            action,
        });
        let point = ctx(n as u64 + 3, n as u64 + 102, n as u64 + 102);
        let record =
            PalwPrefixHistoryRecordV1::new(previous, n as u64 + 2, point.block, point.daa_score, until, movement.clone()).unwrap();
        palw_staged_court_fold_v1::movement(&mut builder, &point, &movement).unwrap();
        previous = record.head_v1().unwrap();
        let PalwStagedCourtRowV1::PublicPrefix(row) = &builder.state.staged_courts[&key] else { unreachable!() };
        assert_eq!(row.history_cursor_v1(), (n as u64 + 2, previous));
        assert_eq!(row.checkpoint_v1().history_material_until_v1(), until);
    }
    let after = builder.state.state_root();
    let request = PalwPrefixHistoryRequestV1 {
        claim,
        owner,
        reserved_daa: movement.reserved_daa,
        seed_id: movement.seed_id,
        head: previous,
        sequence: 4,
        material_until_daa: until,
    };
    let mut held = builder.state.clone();
    let snapshot = ctx(6, until - 1, until - 1);
    held.last_point = Some(snapshot);
    held.legacy_disputes.get_mut(&claim).unwrap().live.remove(&owner);
    palw_prefix_history_request_check_v1(&held, Some(snapshot.block), &request).unwrap();
    assert!(palw_prefix_history_request_check_v1(&held, Some(point.block), &request).is_err());
    let mut stale = request.clone();
    stale.head = first.head_v1().unwrap();
    assert!(palw_prefix_history_request_check_v1(&held, Some(snapshot.block), &stale).is_err());
    let mut wrong_scope = request.clone();
    wrong_scope.seed_id = h64(9_999);
    assert!(palw_prefix_history_request_check_v1(&held, Some(snapshot.block), &wrong_scope).is_err());
    held.last_point = Some(ctx(7, until, until));
    assert!(palw_prefix_history_request_check_v1(&held, None, &request).is_err());
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), after);
    assert_eq!(replayed.staged_courts, builder.state.staged_courts);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_prefix_responders, state.staged_prefix_responders);
    assert!(palw_prefix_history_request_check_v1(&replayed, None, &request).is_err());
}

#[test]
fn named_prefix_job_keeps_its_immutable_registered_tokenizer_statement() {
    use crate::palw_tokenizer_table_stream_v1::palw_tokenizer_statement_v1;
    let (mut target, binding, job, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(true, false);
    let registered = target.registered_semantics.as_mut().unwrap();
    registered.token_table_root = Some(h64(991));
    registered.rendering_rule = Some(crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1);
    registered.raw_tokenizer_bytes = Some(43);
    registered.tokenizer_key_events = Some(4);
    assert!(!job.is_constraint() && job.is_prefix_state());
    assert_eq!(palw_tokenizer_statement_v1(&target, &binding, &job).unwrap().token_table_root, Some(h64(991)));
    let mut unqualified = target.clone();
    unqualified.registered_semantics.as_mut().unwrap().tokenizer_key_events = None;
    assert!(palw_tokenizer_statement_v1(&unqualified, &binding, &job).is_err());
    let mut wrong_job = job.clone();
    wrong_job.job_nonce[0] ^= 1;
    assert!(palw_tokenizer_statement_v1(&target, &binding, &wrong_job).is_err());
    let mut wrong_class = target.clone();
    wrong_class.registered_semantics.as_mut().unwrap().tokenizer_id = h64(992);
    assert!(palw_tokenizer_statement_v1(&wrong_class, &binding, &job).is_err());
}

/// One accepted signed step of a paid owner court: an ordinary move or a constraint leaf opening.
enum PaidStepV1 {
    Move(PalwConstraintStreamMoveV1),
    Leaf(crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamLeafV1),
}
fn paid_move_v1(step: &PaidStepV1) -> &PalwConstraintStreamMoveV1 {
    match step {
        PaidStepV1::Move(movement) => movement,
        PaidStepV1::Leaf(_) => panic!("this check needs an ordinary signed move"),
    }
}
fn continue_paid_case_after_sibling_default(row: PalwStagedCourtRowV1, moves: Vec<PalwConstraintStreamMoveV1>, root: Hash64) {
    continue_paid_steps_after_sibling_default(row, moves.into_iter().map(PaidStepV1::Move).collect(), root)
}
fn continue_paid_steps_after_sibling_default(row: PalwStagedCourtRowV1, moves: Vec<PaidStepV1>, root: Hash64) {
    use crate::palw_da_rcore_v1::{PalwDaClaimV1, PalwDaSessionV1, PalwDaStageV1, PalwDaUnitV1};
    let (mut state, p, mut extras, old_claim, _) = state();
    let p = p.with_legacy_public_filer_from_daa(Some(0));
    extras.objective_offence_daa = Some(0);
    extras.offence_attribution_active = true;
    let first = paid_move_v1(moves.first().unwrap());
    let (claim, owner, reserved, seed) = (first.claim, first.reserver, first.reserved_daa, first.seed_id);
    let terms = match &row {
        PalwStagedCourtRowV1::Constraint(c) => c.terms,
        PalwStagedCourtRowV1::Tokenizer(c) => c.terms,
        PalwStagedCourtRowV1::Prefix(c) => c.terms,
        PalwStagedCourtRowV1::PublicPrefix(c) => c.terms,
        _ => unreachable!(),
    };
    let template = state.bonds[&producer()].clone();
    state.bonds.insert(owner, template);
    let mut claim_row = state.claims.remove(&old_claim).unwrap();
    claim_row.execution_root = root;
    claim_row.phase = PalwClaimPhaseV2::Provisional;
    claim_row.accepted_daa = terms.accepted_daa;
    claim_row.trace_retention_daa = terms.material_until_daa;
    state.claims.insert(claim, claim_row);
    state.staged_courts.clear();
    state.staged_courts.insert(
        PalwStagedCourtKeyV1::Claim(claim),
        PalwStagedCourtRowV1::Claim(PalwStagedClaimContractV1 {
            terms,
            producer: producer(),
            panel: (8..=12).map(bond).collect(),
            prefix_custody: None,
        }),
    );
    state.staged_courts.insert(PalwStagedCourtKeyV1::Owner(claim, owner), row);
    let reservation = crate::palw_legacy_public_filer_v1::PalwDisputeReservationRowV1 {
        reserved_daa: reserved,
        deposit: 1,
        sessions_opened: 0,
        lanes_joined: 0,
    };
    state.legacy_disputes.clear();
    state.legacy_disputes.insert(
        claim,
        crate::palw_legacy_public_filer_v1::PalwDisputeClaimV1 {
            opened_daa: 100,
            hard_deadline_daa: terms.stream_until_daa,
            live: [(owner, reservation)].into_iter().collect(),
            ..Default::default()
        },
    );
    let sibling = bond(6);
    state.da_claims.insert(claim, PalwDaClaimV1 { open_other_sessions: 1, opened_non_seat_total: 1, ..Default::default() });
    state.da_sessions.insert(
        (claim, sibling),
        PalwDaSessionV1 {
            opened_daa: 100,
            deadline_daa: 101,
            accuser_is_seat: false,
            exposure: 1,
            units: vec![PalwDaUnitV1::Event { row: 0, tile: 0 }],
            stage: PalwDaStageV1::Live,
        },
    );
    state.da_by_accuser.insert((sibling, claim));
    palw_staged_court_fold_v1::rebuild_indices(&mut state);
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    palw_staged_court_fold_v1::movement(&mut builder, &ctx(2, 101, 101), first).unwrap();
    let reference = PalwStagedCertificateRefV1 { claim, reserver: owner, reserved_daa: reserved, seed_id: seed };
    assert!(reference.resolve_fault_v1(&builder.state, root).is_err());
    builder.da_default_v1(&ctx(3, 102, 102), claim, sibling).unwrap();
    assert!(builder.state.legacy_dispute_reservation_v1(&claim, &owner).is_none());
    let paid = palw_staged_paid_authority_v1(&builder.state, (claim, owner)).unwrap().unwrap();
    assert_eq!((paid.reserved_daa, paid.seed_id, paid.terms), (reserved, seed, terms));
    assert!(palw_staged_owned_progress_v1(&builder.state, (claim, owner)).unwrap().is_some());
    assert!(
        builder
            .state
            .palw_fraud_filer_candidates_for_claims_v1(&p, &owner, 103, &[])
            .iter()
            .any(|candidate| candidate.claim_id == claim),
        "paid pursuit survives a restart without explicit target"
    );
    let mut wrong = paid_move_v1(&moves[1]).clone();
    wrong.reserved_daa += 1;
    let before = builder.state.state_root();
    assert!(palw_staged_court_fold_v1::movement(&mut builder, &ctx(4, 103, 103), &wrong).is_err());
    assert_eq!(builder.state.state_root(), before);
    assert!(
        palw_staged_court_fold_v1::movement(
            &mut builder,
            &ctx(4, terms.stream_until_daa, terms.stream_until_daa),
            paid_move_v1(&moves[1])
        )
        .is_err()
    );
    assert_eq!(builder.state.state_root(), before);
    for step in moves.iter().skip(1) {
        match step {
            PaidStepV1::Move(movement) => palw_staged_court_fold_v1::movement(&mut builder, &ctx(4, 103, 103), movement).unwrap(),
            PaidStepV1::Leaf(leaf) => palw_staged_court_fold_v1::leaf(&mut builder, &ctx(4, 103, 103), leaf).unwrap(),
        }
    }
    reference.resolve_fault_v1(&builder.state, root).unwrap();
    let last = paid_move_v1(moves.last().unwrap());
    let before = builder.state.state_root();
    assert!(palw_staged_court_fold_v1::movement(&mut builder, &ctx(5, 104, 104), last).is_err());
    assert_eq!(builder.state.state_root(), before);
    assert!(builder.da_default_v1(&ctx(5, 104, 104), claim, sibling).is_err());
    assert_eq!(builder.state.state_root(), before);
    let (evidence_id, evidence) = crate::palw_offence_attribution_v1::palw_executor_refuted_evidence_v1(
        claim,
        crate::palw_offence_v1::PalwPanelContradictionV1::StagedCourt { reference },
        None,
    );
    builder
        .consume_objective_offence_with_budget_v1(
            &ctx(5, 104, 104),
            crate::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted,
            producer(),
            evidence_id,
            &evidence,
            crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1::DirectCarrier,
        )
        .unwrap();
    let offence = crate::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1(&producer().0, &claim);
    assert!(builder.state.consumed_offences.contains_key(&offence));
    let final_root = builder.state.state_root();
    builder
        .consume_objective_offence_with_budget_v1(
            &ctx(5, 104, 104),
            crate::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted,
            producer(),
            evidence_id,
            &evidence,
            crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1::DirectCarrier,
        )
        .unwrap();
    assert_eq!(builder.state.state_root(), final_root);
    let encoded = borsh::to_vec(&builder.state.staged_courts).unwrap();
    let restored: std::collections::BTreeMap<PalwStagedCourtKeyV1, PalwStagedCourtRowV1> = borsh::from_slice(&encoded).unwrap();
    assert_eq!(restored, builder.state.staged_courts);
    let mut replayed = state.clone();
    for entry in &builder.entries {
        apply_delta_entry(&mut replayed, entry, false).unwrap();
    }
    assert_eq!(replayed.state_root(), final_root);
    for entry in builder.entries.iter().rev() {
        apply_delta_entry(&mut replayed, entry, true).unwrap();
    }
    assert_eq!(replayed.state_root(), state.state_root());
    assert_eq!(replayed.staged_due, state.staged_due);
    assert_eq!(replayed.staged_prefix_responders, state.staged_prefix_responders);
}

#[test]
fn every_paid_family_continues_and_convicts_after_actual_sibling_da_default() {
    use crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1 as Phase;
    use crate::palw_prefix_family_court_v1::PalwPrefixCourtActionV1 as PrefixAction;
    let (mut public, tiles) = fixture();
    let root = public.execution_root_v1();
    let mut public_moves = vec![];
    let demand = public.next_demand_v1(101).unwrap().unwrap();
    let mut movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim: public.claim,
        reserver: public.reserver,
        reserved_daa: public.reserved_daa,
        seed_id: public.seed_id(),
        phase: Phase::PrefixCourt,
        action: Action::PrefixPublicDemand(demand.clone()),
        signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    public_moves.push(movement.clone());
    let demand_id = Hash64::from_bytes(movement.signing_message().try_into().unwrap());
    public
        .demand_v1(101, movement.claim, movement.reserver, movement.reserved_daa, movement.seed_id, demand_id, &demand, 6000)
        .unwrap();
    let tile = &tiles[0];
    movement.action = Action::PrefixPublicAnswer(crate::palw_prefix_public_source_v1::PalwPrefixPublicAnswerV1 {
        answerer: public.reserver,
        cursor_id: demand.cursor_id,
        demand_id,
        action: crate::palw_prefix_public_source_v1::PalwPrefixPublicAnswerActionV1::Batch(vec![
            crate::palw_prefix_public_source_v1::PalwPrefixPublicAnswerUnitV1::Begin { opening: tile.opening.clone() },
            crate::palw_prefix_public_source_v1::PalwPrefixPublicAnswerUnitV1::Part {
                offset: 0,
                bytes: tile.preimage.values_le.clone(),
            },
            crate::palw_prefix_public_source_v1::PalwPrefixPublicAnswerUnitV1::Finish,
        ]),
    });
    public_moves.push(movement.clone());
    for action in std::iter::once(PrefixAction::FinishTile).chain(tiles.iter().skip(1).flat_map(|tile| {
        [
            PrefixAction::BeginTile { opening: tile.opening.clone() },
            PrefixAction::TilePart { offset: 0, bytes: tile.preimage.values_le.clone() },
            PrefixAction::FinishTile,
        ]
    })) {
        movement.action = Action::Prefix(action);
        public_moves.push(movement.clone());
    }
    let (initial, _) = fixture();
    continue_paid_case_after_sibling_default(PalwStagedCourtRowV1::PublicPrefix(Box::new(initial).into()), public_moves, root);
    let prefix = crate::palw_prefix_public_owned_v1::tests::inner_fixture();
    let prefix_moves = tiles
        .iter()
        .flat_map(|tile| {
            [
                PrefixAction::BeginTile { opening: tile.opening.clone() },
                PrefixAction::TilePart { offset: 0, bytes: tile.preimage.values_le.clone() },
                PrefixAction::FinishTile,
            ]
        })
        .map(|action| PalwConstraintStreamMoveV1 {
            version: 1,
            claim: prefix.claim,
            reserver: prefix.reserver,
            reserved_daa: prefix.reserved_daa,
            seed_id: prefix.seed_id(),
            phase: Phase::PrefixCourt,
            action: Action::Prefix(action),
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        })
        .collect();
    continue_paid_case_after_sibling_default(PalwStagedCourtRowV1::Prefix(Box::new(prefix).into()), prefix_moves, root);
    let (tokenizer, raw, table) = crate::palw_tokenizer_owned_court_v1::tests::admitted_case(true, 64);
    let root = tokenizer.execution_root_v1();
    let mut completed = tokenizer.clone();
    let moves = crate::palw_tokenizer_owned_court_v1::tests::complete(&mut completed, &raw, &table);
    continue_paid_case_after_sibling_default(PalwStagedCourtRowV1::Tokenizer(Box::new(tokenizer).into()), moves, root);
    let (seed, accusation) = crate::palw_constraint_stream_owned_v1::tests::seed_for(true);
    let terms = crate::palw_constraint_stream_owned_v1::tests::terms(&seed);
    let root = seed.binding.committed_execution_root;
    let row = crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1::admit(
        &seed,
        root,
        terms,
        100,
        u64::MAX,
        2_880_000,
        1 << 25,
        terms.material_until_daa,
        terms.bond_exit_after_daa,
        &[terms.bond_exit_after_daa; 5],
    )
    .unwrap();
    let mut completed = row.clone();
    let mut moves = vec![];
    let mut add = |action| {
        let m = PalwConstraintStreamMoveV1 {
            version: 1,
            claim: row.claim,
            reserver: row.reserver,
            reserved_daa: row.reserved_daa,
            seed_id: row.seed_id(),
            phase: completed.court().phase(),
            action,
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        completed.movement(100, &m).unwrap();
        moves.push(PaidStepV1::Move(m));
    };
    add(Action::EogPart { offset: 0, ids: accusation.eog.ids.clone() });
    add(Action::EogFinish);
    drop(add);
    while completed.court().phase() != Phase::Complete {
        let id = completed.court().next_rendering_id().unwrap();
        let mut actions = if completed.court().is_eog(id) {
            vec![Action::EogTokenFinish]
        } else {
            let o = accusation.renderings.iter().find(|o| o.id == id).unwrap();
            // The rendering leaf opens through its own signed leaf object, not a move action.
            let leaf = crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamLeafV1 {
                version: 1,
                claim: row.claim,
                reserver: row.reserver,
                reserved_daa: row.reserved_daa,
                seed_id: row.seed_id(),
                id,
                present: o.rendering.is_some(),
                length: o.rendering.as_ref().map_or(0, Vec::len) as u32,
                opening: o.opening.clone(),
                signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            };
            completed.leaf(100, &leaf).unwrap();
            moves.push(PaidStepV1::Leaf(leaf));
            let mut actions = vec![];
            if let Some(bytes) = &o.rendering {
                actions.push(Action::RenderingPart { offset: 0, bytes: bytes.clone() });
            }
            actions.push(Action::RenderingFinish);
            actions
        };
        for action in actions.drain(..) {
            let m = PalwConstraintStreamMoveV1 {
                version: 1,
                claim: row.claim,
                reserver: row.reserver,
                reserved_daa: row.reserved_daa,
                seed_id: row.seed_id(),
                phase: completed.court().phase(),
                action,
                signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            };
            completed.movement(100, &m).unwrap();
            moves.push(PaidStepV1::Move(m));
        }
    }
    continue_paid_steps_after_sibling_default(PalwStagedCourtRowV1::Constraint(Box::new(row).into()), moves, root);
}

#[test]
fn staged_source_default_charges_the_producer_only_when_it_is_itself_in_the_unpaid_set() {
    let (state, p, extras, claim, owner) = state();
    let claim_state = state.claim(&claim).unwrap().clone();
    let now = claim_state.accepted_daa.max(100) + 50;
    let offence = crate::palw_da_rcore_v1::palw_da_offence_id_v1(&producer().0, &claim);
    let collateral = |s: &PalwChainStateV2, b: PalwBondKeyV2| s.bond(&b).unwrap().collateral;

    // Total withholding: the producer and one Valid signer never reached EOF.
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    let unpaid = [producer(), bond(8)].into_iter().collect();
    builder.staged_selected_default_charge_v1(&ctx(2, now, now), claim, &claim_state, producer(), &unpaid, owner).unwrap();
    assert!(collateral(&builder.state, producer()) < collateral(&state, producer()), "withholding producer is charged");
    assert!(collateral(&builder.state, bond(8)) < collateral(&state, bond(8)), "silent covering signer is charged");
    assert_eq!(collateral(&builder.state, bond(9)), collateral(&state, bond(9)), "an answering signer is not");
    assert!(builder.state.consumed_offence(&offence).is_some(), "the producer DA record is written once");
    assert_eq!(collateral(&builder.state, owner), collateral(&state, owner), "the outside owner is never debited");

    // The producer answered to EOF; only one signer stayed silent.
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    let unpaid = [bond(9)].into_iter().collect();
    builder.staged_selected_default_charge_v1(&ctx(2, now, now), claim, &claim_state, producer(), &unpaid, owner).unwrap();
    assert_eq!(collateral(&builder.state, producer()), collateral(&state, producer()), "an answering producer is not charged");
    assert!(builder.state.consumed_offence(&offence).is_none(), "no producer DA record without producer default");
    assert_eq!(builder.state.claim(&claim).unwrap().phase, claim_state.phase, "no void without producer default");
    assert!(collateral(&builder.state, bond(9)) < collateral(&state, bond(9)), "only the silent signer is charged");
    assert_eq!(collateral(&builder.state, bond(8)), collateral(&state, bond(8)));

    // Roles stay literal: the independent winner can never be in its own unpaid set.
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
    let unpaid = [producer(), owner].into_iter().collect();
    assert!(builder.staged_selected_default_charge_v1(&ctx(2, now, now), claim, &claim_state, producer(), &unpaid, owner).is_err());
}
