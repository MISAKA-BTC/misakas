//! RFC-0006 source controls for the remaining shared writers (`palw_state_v2/native_writers.rs`): payout, exposure, court
//! group, deadline index, encoded vertex/improvement rows, kernel-route rows and the lenient-consumer capacity guard.
//! SOURCE ONLY — written, never compiled or executed in this lane. They assert ordering properties (refusal before any state or
//! journal change; flags-off byte parity), not capacity numbers.
#![allow(warnings)]
use super::*;
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};

fn zero_capacity(builder: &mut TransitionBuilder<'_>) -> std::sync::Arc<NativeDemandCheckerV1> {
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 });
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    checker
}

#[test]
fn ordinary_registration_keeps_routing_profile_without_certification_or_price_index_changes() {
    let old = params();
    let (parent, _) = apply(&PalwChainStateV2::genesis(), &old, &ctx(1, 100, 1), &register_class_and_bond(), None);
    assert!(parent.kernel_route().is_none());
    let extras = PalwTransitionExtrasV1 {
        kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: h64(1),
            ruleset_digest: h64(2),
            ..Default::default()
        }),
        ..Default::default()
    };
    let profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).unwrap();
    let class = profile.shape_profile_id();
    let p = old.with_memory_routing_court_from_daa(Some(101));
    let mut object = registration(class, 0, Some(bond_key(1)));
    if let PalwConsensusObjectV2::ClassRegistered { slash_value_per_pwu, .. } = &mut object {
        *slash_value_per_pwu = parent.class(&h64(1)).unwrap().slash_value_per_pwu;
    }
    let (child, delta) =
        apply_palw_transition_v2_with_extras(&parent, &p, &ctx(2, 101, 2), &[object], None, false, false, false, false, &extras)
            .unwrap();
    assert_eq!(&*child.reader_router_profile_v2(class).unwrap().unwrap(), &profile);
    assert!(child.fp_work_profile_of(&class).is_none(), "registration must not populate the price/deadline index");
    assert!(!child.fp_certified_classes.contains_key(&class), "graph identity is not model approval");
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&child)).unwrap();
    let restored = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&p, Some(child.state_root())).unwrap();
    // Snapshot restore can initialize the dormant, uncommitted cohort census.
    assert_eq!(restored.state_root(), child.state_root());
    assert_eq!(&*restored.reader_router_profile_v2(class).unwrap().unwrap(), &profile);
    assert!(restored.fp_work_profile_of(&class).is_none());
    assert!(!restored.fp_certified_classes.contains_key(&class));
    assert_eq!(apply_delta_v2(&parent, &delta, &p).unwrap(), child);
    assert_eq!(revert_delta_v2(&child, &delta, &p).unwrap(), parent);
}

#[test]
fn reader_router_consumer_propagates_late_writer_capacity_refusal() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = PalwChainStateV2::genesis();
    parent.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    // Measure the concrete request of this write, not a policy or model-wide limit.
    // One byte below it passes the consumer's earlier reservations but refuses
    // the last reservation inside write_kernel_row, whose Result was discarded.
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
    let mut builder = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    builder.write_reader_router_cell_v2(&[7; 64], Some(&11u64)).unwrap();
    let required = checker.outstanding().0;
    assert!(required > 0);
    assert!(!builder.entries.is_empty());
    drop(builder);
    assert_eq!(checker.outstanding(), (0, 0));

    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: required - 1, database_bytes: 0 });
    let mut builder = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    assert!(matches!(builder.write_reader_router_cell_v2(&[7; 64], Some(&11u64)), Err(PalwStateV2Error::NativeResource(_))));
    assert!(checker.outstanding().0 > 0, "earlier reservations were admitted");
    assert_eq!(builder.state, parent);
    assert!(builder.entries.is_empty());
    drop(builder);
    assert_eq!(checker.outstanding(), (0, 0));
}

#[test]
fn remaining_writers_refuse_locally_before_state_or_journal_change() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let empty = PalwChainStateV2::genesis();
    let mut builder = TransitionBuilder::new(&empty, &p, false, false, false, false, &extras);
    let checker = zero_capacity(&mut builder);
    let local = |r: Result<(), PalwStateV2Error>| matches!(r, Err(PalwStateV2Error::NativeResource(_)));
    assert!(local(builder.write_payout(h64(1), Some(PalwPayoutV2 { payload: h64(2), amount: 7 }))));
    assert!(local(builder.write_exposure(bond_key(1), Some(5))));
    assert!(local(builder.write_registration_exposure(bond_key(1), Some(5))));
    assert!(local(builder.write_panel_reserve(9)));
    assert!(local(builder.write_round_span(3)));
    assert!(local(builder.arm_deadline(10, h64(3))));
    assert!(local(builder.write_claim_root(h64(4), Some(h64(5)))));
    assert!(local(builder.write_strikes(bond_key(2), Some(vec![1, 2, 3]))));
    assert!(local(builder.write_final_work((1, h64(6)), Some(11))));
    // Removal still owns a journal slot: refused too, and nothing is removed.
    assert!(local(builder.write_payout(h64(1), None)));
    // The refused writers changed nothing: no map row, no index node, no journal entry, no retained request.
    assert_eq!(builder.state, empty);
    assert!(builder.entries.is_empty());
    assert!(builder.state.deadlines.is_empty());
    assert_eq!(checker.outstanding(), (0, 0));
}

#[test]
fn encoded_row_writers_refuse_before_the_typed_clone_and_canonical_buffers() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let empty = PalwChainStateV2::genesis();
    let mut builder = TransitionBuilder::new(&empty, &p, false, false, false, false, &extras);
    let checker = zero_capacity(&mut builder);
    assert!(matches!(builder.write_mesh_audit(h64(1), None), Err(PalwStateV2Error::NativeResource(_))));
    assert!(matches!(builder.write_improvement_earning(bond_key(1), Some(3)), Err(PalwStateV2Error::NativeResource(_))));
    assert_eq!(builder.state, empty);
    assert!(builder.entries.is_empty());
    assert_eq!(checker.outstanding(), (0, 0));
}

#[test]
fn remaining_writers_flags_off_journal_is_byte_identical() {
    // No checker and no native admission: every admission returns before any work and the historical writer runs.
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let empty = PalwChainStateV2::genesis();
    let mut builder = TransitionBuilder::new(&empty, &p, false, false, false, false, &extras);
    assert!(!builder.native_reservation.is_tracking_v1());
    let row = PalwPayoutV2 { payload: h64(2), amount: 7 };
    builder.write_payout(h64(1), Some(row)).unwrap();
    builder.write_exposure(bond_key(1), Some(5)).unwrap();
    builder.arm_deadline(10, h64(3)).unwrap();
    assert_eq!(builder.entries.len(), 2);
    assert_eq!(
        borsh::to_vec(&builder.entries[0]).unwrap(),
        borsh::to_vec(&PalwDeltaEntryV2::Payout { key: h64(1), old: None, new: Some(row) }).unwrap()
    );
    assert_eq!(
        borsh::to_vec(&builder.entries[1]).unwrap(),
        borsh::to_vec(&PalwDeltaEntryV2::Exposure { key: bond_key(1), old: None, new: Some(5) }).unwrap()
    );
    assert!(builder.state.deadlines.contains(&(10, h64(3))));
    // A kernel-route row with no route header is the historical no-op, with or without tracking.
    builder.write_kernel_row(40, vec![1], Some(vec![2])).unwrap();
    assert_eq!(builder.entries.len(), 2);
    assert!(builder.state.kernel_route.is_none());
}

#[test]
fn admitted_writer_requests_live_until_the_construction_owner_drops() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let mut builder = TransitionBuilder::new(&PalwChainStateV2::genesis(), &p, false, false, false, false, &extras);
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    builder.write_payout(h64(1), Some(PalwPayoutV2 { payload: h64(2), amount: 7 })).unwrap();
    builder.write_strikes(bond_key(2), Some(vec![1, 2, 3])).unwrap();
    builder.arm_deadline(10, h64(3)).unwrap();
    assert_eq!(builder.entries.len(), 2);
    assert!(checker.outstanding().0 > 0);
    drop(builder);
    assert_eq!(checker.outstanding(), (0, 0));
}

#[test]
fn lenient_consumers_drop_semantic_refusals_but_never_local_capacity() {
    use super::super::native_writers::local_capacity_or_v1;
    let capacity = PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::zero());
    assert!(matches!(local_capacity_or_v1::<()>(Err(capacity)), Err(PalwStateV2Error::NativeResource(_))));
    assert!(matches!(local_capacity_or_v1::<()>(Err(PalwStateV2Error::Overflow("semantic"))), Ok(Err(PalwStateV2Error::Overflow(_)))));
    assert!(matches!(local_capacity_or_v1(Ok(5u64)), Ok(Ok(5))));
}

// ---- RFC06-1: the bond-budget engine's growth is admitted before the op runs ----

fn with_budget(mut state: PalwChainStateV2) -> PalwChainStateV2 {
    let policy = crate::palw_bond_budget_v1::PalwBondBudgetPolicyV1::unapproved_probe_v1();
    state.bond_budget = Some(crate::palw_bond_budget_v1::PalwBondBudgetStateV1::new(0, &policy));
    state
}

#[test]
fn a_bond_budget_op_is_refused_locally_before_the_engine_runs() {
    use super::super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1;
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_budget(PalwChainStateV2::genesis());
    let mut builder = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let checker = zero_capacity(&mut builder);
    let ran = std::cell::Cell::new(false);
    let out = builder.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
        ran.set(true);
        budget.activate_reporter_income_v2(j)
    });
    assert!(matches!(out, Err(PalwStateV2Error::NativeResource(_))), "a local capacity refusal, never a verdict");
    assert!(!ran.get(), "the engine op never ran: nothing was allocated past the refusal");
    assert_eq!(builder.state, parent);
    assert!(builder.entries.is_empty());
    assert_eq!(checker.outstanding(), (0, 0));
    // A swept op is admitted for its rows too (a larger bound than a point op, never a smaller one).
    assert!(matches!(
        builder.bond_budget_op(PalwBudgetGrowthV1::rows(3), |budget, _, j| budget.release_due(0, j)),
        Err(PalwStateV2Error::NativeResource(_))
    ));
}

#[test]
fn a_bond_budget_op_with_tracking_off_is_the_historical_op_byte_for_byte() {
    use super::super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1;
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    // No engine: nothing is admitted, read or run.
    let bare = PalwChainStateV2::genesis();
    let mut builder = TransitionBuilder::new(&bare, &p, false, false, false, false, &extras);
    assert!(matches!(builder.bond_budget_op(PalwBudgetGrowthV1::point(), |_, _, _| ()), Ok(None)));
    // An engine, tracking off: the op runs and its journal becomes the same deltas as before RFC06-1.
    let parent = with_budget(PalwChainStateV2::genesis());
    let mut builder = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    assert!(!builder.native_reservation.is_tracking_v1());
    builder.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.activate_reporter_income_v2(j)).unwrap().unwrap();
    let mut expected = parent.bond_budget.clone().unwrap();
    let mut journal = Vec::new();
    expected.activate_reporter_income_v2(&mut journal);
    assert_eq!(builder.state.bond_budget.as_ref(), Some(&expected));
    assert_eq!(builder.entries.len(), journal.len());
}

#[test]
fn positive_aux_writer_reserves_retained_workspace_only_on_insert() {
    use crate::palw_native_resource_v1::positive_rows::positive_aux_row_memory_bytes_v1;
    use misaka_palw_positive_contract::native::KERNEL_AUX_TABLE;

    let params = params();
    let extras = PalwTransitionExtrasV1::default();
    let key = vec![1];
    let value = Some(vec![2]);
    let quote_bytes = |table, present| {
        let mut parent = PalwChainStateV2::genesis();
        let mut route = crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
            None,
            misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
        );
        if present {
            route.aux.insert((table, key.clone()), vec![2]);
        }
        parent.kernel_route = Some(route);
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
        let mut builder = TransitionBuilder::new(&parent, &params, false, false, false, false, &extras);
        builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
        builder.reserve_kernel_row_write_v1(table, &key, &value, false).unwrap();
        checker.outstanding().0
    };

    let ordinary_aux_table = 40;
    assert_eq!(quote_bytes(KERNEL_AUX_TABLE, false), quote_bytes(ordinary_aux_table, false) + positive_aux_row_memory_bytes_v1());
    assert_eq!(quote_bytes(KERNEL_AUX_TABLE, true), quote_bytes(ordinary_aux_table, true));
}

#[test]
fn malformed_public_admission_is_rejected_before_journaling() {
    use crate::palw_positive_contract_v2::PUBLIC_ORIGINAL_ADMISSION_TABLE_V1;
    use misaka_palw_positive_contract::{ContractError, identity::Digest, public_one_honest::OriginalId};

    let mut parent = PalwChainStateV2::genesis();
    parent.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    let params = params();
    let extras = PalwTransitionExtrasV1::default();
    let mut builder = TransitionBuilder::new(&parent, &params, false, false, false, false, &extras);
    let key = (PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, vec![1; 64]);
    assert!(matches!(builder.write_kernel_row(key.0, key.1.clone(), Some(vec![2])), Err(PalwStateV2Error::KernelRouteRefused(_))));
    assert!(!builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&key));
    assert!(builder.entries.is_empty());
    let original = OriginalId(Digest([1; 64]));
    assert_eq!(builder.state.public_original_admission_record_v1(original), Ok(None));
    let mut corrupt_snapshot = builder.state;
    corrupt_snapshot.set_kernel_aux_row_for_tests(key.0, key.1, Some(vec![2]));
    assert_eq!(corrupt_snapshot.public_original_admission_record_v1(original), Err(ContractError::NonCanonical));
}

#[test]
fn malformed_public_admission_delta_cannot_install_on_replay_or_revert() {
    use crate::palw_positive_contract_v2::PUBLIC_ORIGINAL_ADMISSION_TABLE_V1;

    let mut state = PalwChainStateV2::genesis();
    let table = PUBLIC_ORIGINAL_ADMISSION_TABLE_V1;
    let key = vec![1; 64];
    state.set_kernel_aux_row_for_tests(table, key.clone(), None);
    let root = state.state_root();
    let malformed = Some(vec![2]);
    assert!(matches!(
        apply_kernel_route_row_v1(&mut state, table, &key, &None, &malformed, false),
        Err(PalwStateV2Error::DeltaMismatch("invalid public verification row in kernel route delta"))
    ));
    assert!(matches!(
        apply_kernel_route_row_v1(&mut state, table, &key, &malformed, &None, true),
        Err(PalwStateV2Error::DeltaMismatch("invalid public verification row in kernel route delta"))
    ));
    assert_eq!(state.state_root(), root);
}

#[test]
fn public_admission_row_round_trips_through_delta_and_snapshot() {
    use crate::palw_positive_contract_v2::{PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, PublicOriginalAdmissionRecordV1};
    use misaka_palw_positive_contract::{
        identity::Digest,
        rows::{AcceptedOrigin, NativeHash64},
    };

    let params = params();
    let (mut parent, _) = apply(&PalwChainStateV2::genesis(), &params, &ctx(1, 100, 1), &register_class_and_bond(), None);
    parent.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 100, burned: 0 },
    ));
    let point = ctx(2, 101, 2);
    use misaka_palw_positive_contract::{
        identity::{ChainScope, PrincipalId},
        public_one_honest::{
            MANIFEST_RESPONSE_SIGN_CONTEXT, ManifestResponseCarrierV1, ManifestScopeRules, ManifestV1, MaterialScope, OriginalCommit,
            SignedManifestResponseV1, manifest_limits_id_v1, manifest_root,
        },
    };
    let digest = Digest([3; 64]);
    let commit = OriginalCommit {
        scope: ChainScope { network: digest, ruleset: digest, branch: digest },
        job: digest,
        execution: digest,
        miner: PrincipalId(digest),
        registered_class: digest,
        artifact: digest,
        program: digest,
        prompt: digest,
        input_tokens: digest,
        tokenizer: digest,
        execution_rules: digest,
        generation_seed: [0; 32],
        initial_state: digest,
        dependencies: digest,
        prefill: 1,
        decode: 1,
        max_context: 2,
        output: digest,
        trace_root: digest,
        manifest_root: digest,
        service_contract: digest,
    };
    let original = commit.id();
    let scope_limits = [ManifestScopeRules { codec: digest, max_bytes: 1, chunk_bytes: 1 }; MaterialScope::COUNT];
    let total_limit = MaterialScope::COUNT as u64;
    let manifest_limits = manifest_limits_id_v1(&scope_limits, total_limit, ManifestV1::WIRE_BYTES);
    let (row, prepared) = PublicOriginalAdmissionRecordV1::storage_fixture_with_manifest(
        &commit,
        AcceptedOrigin {
            accepting_block: NativeHash64(point.block.as_bytes()),
            carrier_block: NativeHash64(h64(3).as_bytes()),
            carrier_transaction: NativeHash64(h64(4).as_bytes()),
            accepted_tx_ordinal: 1,
            event_ordinal: 0,
            accepted_daa: point.daa_score,
            accepted_blue_score: point.blue_score,
        },
        manifest_limits,
    );
    let extras = PalwTransitionExtrasV1 {
        kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: h64(1),
            ruleset_digest: h64(2),
            ..Default::default()
        }),
        public_original_admissions: vec![row.storage_admission_fixture(), row.storage_admission_fixture()],
        ..Default::default()
    };
    let (child, delta) =
        apply_palw_transition_v2_with_extras(&parent, &params, &point, &[], None, false, false, false, false, &extras).unwrap();
    let wrong_accepting_block = PalwBlockContextV2 { block: h64(10), ..point };
    assert!(
        apply_palw_transition_v2_with_extras(
            &parent, &params, &wrong_accepting_block, &[], None, false, false, false, false, &extras,
        ).is_err()
    );
    assert_eq!(child.public_original_admission_record_v1(original), Ok(Some(row)));
    assert_eq!(child.has_public_pending_manifest_v1(original), Ok(false));
    assert_eq!(parent.has_public_pending_manifest_v1(original), Err(misaka_palw_positive_contract::ContractError::Scope));
    assert_eq!(apply_delta_v2(&parent, &delta, &params).unwrap(), child);
    assert_eq!(revert_delta_v2(&child, &delta, &params).unwrap(), parent);
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&child)).unwrap();
    let restored = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&params, Some(child.state_root())).unwrap();
    // This fixture does not activate execution-cohort tracking. Its derived
    // census can differ after restore even when the committed state matches.
    assert_eq!(restored.state_root(), child.state_root());
    assert_eq!(restored.last_point(), child.last_point());
    assert_eq!(restored.public_original_admission_record_v1(original), Ok(Some(row)));
    use misaka_palw_positive_contract::{
        identity::principal,
        public_one_honest::{FULL_REPLAY_SIGN_CONTEXT, FullReplayApproval, PanelBond, PublicDrawMemberV1, SignedFullReplayApprovalV1},
    };
    let draw_point = ctx(3, 102, 3);
    let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([17; 32]);
    let key = *pair.verification_key.as_ref();
    let bond = PanelBond { transaction: Digest([8; 64]), index: 0 };
    let member = principal(&key);
    let other_bond = PanelBond { transaction: Digest([7; 64]), index: 0 };
    let draw = row.storage_draw_fixture(
        draw_point,
        &[
            PublicDrawMemberV1 { bond, principal: member, rescue_reservation: Digest([9; 64]) },
            PublicDrawMemberV1 { bond: other_bond, principal: member, rescue_reservation: Digest([10; 64]) },
        ],
    );
    let draw_extras = PalwTransitionExtrasV1 {
        kernel_route: extras.kernel_route.clone(),
        public_draws: vec![draw.prepare_storage_record(), draw.prepare_storage_record()],
        ..Default::default()
    };
    let (drawn, draw_delta) =
        apply_palw_transition_v2_with_extras(&child, &params, &draw_point, &[], None, false, false, false, false, &draw_extras)
            .unwrap();
    assert_eq!(drawn.public_draw_record_v1(original), Ok(Some(draw.record())));
    assert_eq!(apply_delta_v2(&child, &draw_delta, &params).unwrap(), drawn);
    assert_eq!(revert_delta_v2(&drawn, &draw_delta, &params).unwrap(), child);
    let incorrect_draw_point = PalwBlockContextV2 { block: h64(9), ..draw_point };
    assert!(
        apply_palw_transition_v2_with_extras(
            &child,
            &params,
            &incorrect_draw_point,
            &[],
            None,
            false,
            false,
            false,
            false,
            &draw_extras,
        )
        .is_err()
    );
    let vote_point = ctx(4, 103, 4);
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64(vote_point.block.as_bytes()),
        carrier_block: NativeHash64(h64(5).as_bytes()),
        carrier_transaction: NativeHash64(h64(6).as_bytes()),
        accepted_daa: 103,
        accepted_blue_score: 4,
        ..row.carrier().origin
    };
    let context = draw.approval_context(bond).unwrap();
    let approval = FullReplayApproval { original, draw: context.draw, bond, member, full_scope: context.full_scope };
    let sig =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &approval.signing_digest().0, FULL_REPLAY_SIGN_CONTEXT, [0; 32]).unwrap();
    let bytes = borsh::to_vec(&SignedFullReplayApprovalV1 { approval, key, signature: *sig.as_ref() }).unwrap();
    let accepted = draw.prepare_replay_approval_bytes(104, origin, &bytes).unwrap();
    let vote_extras = PalwTransitionExtrasV1 {
        kernel_route: extras.kernel_route.clone(),
        public_replay_approvals: vec![accepted, accepted],
        ..Default::default()
    };
    let (voted, votes) =
        apply_palw_transition_v2_with_extras(&drawn, &params, &vote_point, &[], None, false, false, false, false, &vote_extras)
            .unwrap();
    let record = draw.verify_replay_approval_bytes(104, origin, &bytes).unwrap();
    assert_eq!(voted.public_replay_approval_record_v1(original, bond), Ok(Some(record)));
    assert_eq!(
        votes
            .entries
            .iter()
            .filter(|entry| matches!(
                entry,
                PalwDeltaEntryV2::KernelRouteRow { table: crate::palw_positive_contract_v2::PUBLIC_REPLAY_APPROVAL_TABLE_V1, .. }
            ))
            .count(),
        1
    );
    assert_eq!(apply_delta_v2(&drawn, &votes, &params).unwrap(), voted);
    assert_eq!(revert_delta_v2(&voted, &votes, &params).unwrap(), drawn);
    let restored_vote = PalwStateCarriageV2::from_state(&voted).into_state(&params, Some(voted.state_root())).unwrap();
    assert_eq!(restored_vote.public_draw_record_v1(original), Ok(Some(draw.record())));
    assert_eq!(restored_vote.public_replay_approval_record_v1(original, bond), Ok(Some(record)));
    use crate::palw_positive_contract_v2::PublicReplayApprovalLookupErrorV1;
    assert_eq!(drawn.has_reauthenticated_public_replay_approval_v1(&draw, 104, |_| panic!("no vote")), Ok(false));
    assert_eq!(
        restored_vote.has_reauthenticated_public_replay_approval_v1(&draw, 104, |_| None),
        Err(PublicReplayApprovalLookupErrorV1::CarrierUnavailable(origin))
    );
    let bytes: [u8; SignedFullReplayApprovalV1::WIRE_BYTES] = bytes.try_into().unwrap();
    assert_eq!(
        restored_vote.has_reauthenticated_public_replay_approval_v1(&draw, 104, |accepted_origin| {
            assert_eq!(accepted_origin, origin);
            Some(bytes)
        }),
        Ok(true)
    );
    let mut wrong = bytes;
    *wrong.last_mut().unwrap() ^= 1;
    assert_eq!(
        restored_vote.has_reauthenticated_public_replay_approval_v1(&draw, 104, |_| Some(wrong)),
        Err(PublicReplayApprovalLookupErrorV1::Contract(misaka_palw_positive_contract::ContractError::Signature))
    );

    use crate::palw_positive_contract_v2::PublicFinalReplayPrerequisiteV1;
    let clock = misaka_palw_positive_contract::public_one_honest::OriginalClock::new(point.daa_score).unwrap();
    let draw_extras_without_votes = PalwTransitionExtrasV1 { kernel_route: extras.kernel_route.clone(), ..Default::default() };
    let fold_at = |parent, block, daa| {
        let point = ctx(block, daa, block);
        apply_palw_transition_v2_with_extras(
            parent,
            &params,
            &point,
            &[],
            None,
            false,
            false,
            false,
            false,
            &draw_extras_without_votes,
        )
        .unwrap()
        .0
    };
    assert_eq!(
        restored_vote.public_final_replay_prerequisite_v1(&draw, 104, |_| panic!("notice is open")),
        Ok(PublicFinalReplayPrerequisiteV1::NoticeOpen)
    );
    let notice_end = fold_at(&restored_vote, 5, clock.notice_until());
    assert_eq!(
        notice_end.public_final_replay_prerequisite_v1(&draw, 104, |_| panic!("notice is inclusive")),
        Ok(PublicFinalReplayPrerequisiteV1::NoticeOpen)
    );
    let after_notice = fold_at(&notice_end, 6, clock.notice_until() + 1);
    assert_eq!(
        after_notice.public_final_replay_prerequisite_v1(&draw, 104, |_| Some(bytes)),
        Ok(PublicFinalReplayPrerequisiteV1::ReadyForRemainingChecks)
    );
    use misaka_palw_positive_contract::public_one_honest::{
        Claimant, MANIFEST_REQUEST_SIGN_CONTEXT, ManifestRequest, ManifestRequestContext, SignedManifestRequestV1,
    };
    let request_context = ManifestRequestContext {
        original: &commit,
        claimant: Claimant::DrawnBond(bond),
        signer: member,
        inspection_scope: Digest([6; 64]),
        supply_obligation: Digest([11; 64]),
        reservation: Digest([9; 64]),
        max_manifest_bytes: ManifestV1::WIRE_BYTES,
        manifest_limits,
        manifest_body_bytes_reserved: u64::from(ManifestV1::WIRE_BYTES),
        schedule: prepared.schedule,
        origin,
    };
    let request = ManifestRequest {
        original,
        manifest_root: commit.manifest_root,
        claimant: request_context.claimant,
        signer: member,
        inspection_scope: request_context.inspection_scope,
        supply_obligation: request_context.supply_obligation,
        collect_until: prepared.schedule.filed_by(),
        reservation: request_context.reservation,
    };
    let case = request.case_id(&commit, request_context.claimant).unwrap();
    let signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &request.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT, [2; 32])
            .unwrap();
    let wire = borsh::to_vec(&SignedManifestRequestV1 { request, key, signature: *signature.as_ref() }).unwrap();
    let pending = prepared.prepare_drawn_manifest_storage_record(Digest([2; 64]), &draw, &wire, &request_context).unwrap();
    let case_extras = PalwTransitionExtrasV1 {
        kernel_route: extras.kernel_route.clone(),
        public_drawn_manifest_cases: vec![pending.clone(), pending],
        ..Default::default()
    };
    let (with_case, case_delta) =
        apply_palw_transition_v2_with_extras(&drawn, &params, &vote_point, &[], None, false, false, false, false, &case_extras)
            .unwrap();
    assert!(with_case.public_pending_manifest_record_v1(original, case).unwrap().is_some());
    assert_eq!(apply_delta_v2(&drawn, &case_delta, &params).unwrap(), with_case);
    assert_eq!(revert_delta_v2(&with_case, &case_delta, &params).unwrap(), drawn);
    let restored_case = PalwStateCarriageV2::from_state(&with_case).into_state(&params, Some(with_case.state_root())).unwrap();
    assert_eq!(
        fold_at(&restored_case, 6, clock.notice_until() + 1).public_final_replay_prerequisite_v1(&draw, 104, |_| panic!("case open")),
        Ok(PublicFinalReplayPrerequisiteV1::ManifestCasePending)
    );

    let pending = prepared
        .verify_manifest_request_bytes(
            crate::palw_positive_contract_v2::PublicManifestClaimantAuthorityV1::Drawn(&draw),
            &wire,
            &request_context,
        )
        .unwrap();
    let delivery_point = ctx(5, 104, 5);
    let delivery_origin = AcceptedOrigin {
        accepting_block: NativeHash64(delivery_point.block.as_bytes()),
        accepted_blue_score: delivery_point.blue_score,
        accepted_daa: delivery_point.daa_score,
        ..origin
    };
    let body = b"authenticated but wrong material";
    let response = SignedManifestResponseV1 {
        case,
        supply_obligation: request_context.supply_obligation,
        provider: member,
        body_root: manifest_root(commit.scope.ruleset, body, ManifestV1::WIRE_BYTES).unwrap(),
    };
    let signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &response.signing_digest().0, MANIFEST_RESPONSE_SIGN_CONTEXT, [3; 32])
            .unwrap();
    let mut packet = borsh::to_vec(&ManifestResponseCarrierV1 { response, key, signature: *signature.as_ref() }).unwrap();
    packet.extend_from_slice(body);
    let delivery =
        pending.verify_provider_response_bytes(&packet, member, &commit, delivery_origin, &scope_limits, total_limit).unwrap();
    let delivery_row = delivery.record();
    let prepared_delivery = crate::palw_positive_contract_v2::PreparedPublicManifestResponseV1::new(&pending, &delivery).unwrap();
    let response_extras = PalwTransitionExtrasV1 {
        kernel_route: extras.kernel_route.clone(),
        public_manifest_responses: vec![prepared_delivery, prepared_delivery],
        ..Default::default()
    };
    let fold_response = |parent| {
        apply_palw_transition_v2_with_extras(parent, &params, &delivery_point, &[], None, false, false, false, false, &response_extras)
    };
    assert!(fold_response(&voted).is_err());
    let (delivered, delivery_delta) = fold_response(&with_case).unwrap();
    assert_eq!(delivered.public_manifest_response_record_v1(&delivery_row.key()), Ok(Some(delivery_row)));
    assert_eq!(delivered.has_public_pending_manifest_v1(original), Ok(true));
    assert_eq!(apply_delta_v2(&with_case, &delivery_delta, &params).unwrap(), delivered);
    assert_eq!(revert_delta_v2(&delivered, &delivery_delta, &params).unwrap(), with_case);
    let restored_delivery = PalwStateCarriageV2::from_state(&delivered).into_state(&params, Some(delivered.state_root())).unwrap();
    assert_eq!(restored_delivery.public_manifest_response_record_v1(&delivery_row.key()), Ok(Some(delivery_row)));
    use crate::palw_positive_contract_v2::PublicManifestResponseLookupErrorV1;
    let lookup = |packet: Option<Vec<u8>>| {
        restored_delivery.reauthenticate_public_manifest_response_v1(
            &delivery_row.key(),
            &pending,
            &commit,
            member,
            &scope_limits,
            total_limit,
            |accepted| {
                assert_eq!(accepted, delivery_origin);
                packet
            },
        )
    };
    assert_eq!(lookup(None), Err(PublicManifestResponseLookupErrorV1::CarrierUnavailable(delivery_origin)));
    let mut altered_packet = packet.clone();
    *altered_packet.last_mut().unwrap() ^= 1;
    assert_eq!(
        lookup(Some(altered_packet)),
        Err(PublicManifestResponseLookupErrorV1::Contract(misaka_palw_positive_contract::ContractError::Identity))
    );
    assert_eq!(lookup(Some(packet)), Ok(Some(delivery)));
    assert_eq!(
        fold_at(&restored_delivery, 6, clock.notice_until() + 1)
            .public_final_replay_prerequisite_v1(&draw, 104, |_| panic!("a response did not close the case")),
        Ok(PublicFinalReplayPrerequisiteV1::ManifestCasePending)
    );

    let absent_vote = fold_at(&drawn, 6, clock.notice_until() + 1);
    assert_eq!(
        absent_vote.public_final_replay_prerequisite_v1(&draw, 104, |_| panic!("no approval to look up")),
        Ok(PublicFinalReplayPrerequisiteV1::MissingReplayApproval)
    );
    let past_hard = fold_at(&after_notice, 7, clock.hard() + 1);
    assert_eq!(
        past_hard.public_final_replay_prerequisite_v1(&draw, 104, |_| panic!("hard deadline passed")),
        Ok(PublicFinalReplayPrerequisiteV1::PastHardDeadline)
    );

    let court_point = ctx(5, 104, 5);
    let court_origin = AcceptedOrigin {
        accepting_block: NativeHash64(court_point.block.as_bytes()),
        accepted_daa: 104,
        accepted_blue_score: 5,
        ..origin
    };
    let evidence_id = h64(41);
    let court = row.storage_court_fixture(evidence_id, court_origin);
    let court_extras = PalwTransitionExtrasV1 {
        kernel_route: extras.kernel_route.clone(),
        public_court_records: vec![court, court],
        ..Default::default()
    };
    let (refuted, court_delta) =
        apply_palw_transition_v2_with_extras(&voted, &params, &court_point, &[], None, false, false, false, false, &court_extras)
            .unwrap();
    assert!(refuted.public_original_court_record_v1(original, evidence_id).unwrap().is_some());
    assert_eq!(
        refuted.public_final_replay_prerequisite_v1(&draw, 104, |_| panic!("recorded refutation takes precedence")),
        Ok(PublicFinalReplayPrerequisiteV1::RecordedRefutation)
    );
    assert_eq!(apply_delta_v2(&voted, &court_delta, &params).unwrap(), refuted);
    assert_eq!(revert_delta_v2(&refuted, &court_delta, &params).unwrap(), voted);
    let restored_refuted = PalwStateCarriageV2::from_state(&refuted).into_state(&params, Some(refuted.state_root())).unwrap();
    assert_eq!(
        restored_refuted.public_final_replay_prerequisite_v1(&draw, 104, |_| panic!("restored refutation takes precedence")),
        Ok(PublicFinalReplayPrerequisiteV1::RecordedRefutation)
    );
    let fork_point = PalwBlockContextV2 { block: h64(42), ..court_point };
    assert!(
        apply_palw_transition_v2_with_extras(&voted, &params, &fork_point, &[], None, false, false, false, false, &court_extras)
            .is_err()
    );

    let earlier = FullReplayApproval { original, draw: context.draw, bond: other_bond, member, full_scope: context.full_scope };
    let signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &earlier.signing_digest().0, FULL_REPLAY_SIGN_CONTEXT, [1; 32]).unwrap();
    let earlier_wire = borsh::to_vec(&SignedFullReplayApprovalV1 { approval: earlier, key, signature: *signature.as_ref() }).unwrap();
    let other_origin = AcceptedOrigin { event_ordinal: origin.event_ordinal + 1, ..origin };
    let other_vote = draw.prepare_replay_approval_bytes(104, other_origin, &earlier_wire).unwrap();
    let both_extras = PalwTransitionExtrasV1 { public_replay_approvals: vec![accepted, other_vote], ..vote_extras.clone() };
    let (both, _) =
        apply_palw_transition_v2_with_extras(&drawn, &params, &vote_point, &[], None, false, false, false, false, &both_extras)
            .unwrap();
    let mut reads = Vec::new();
    assert_eq!(
        both.has_reauthenticated_public_replay_approval_v1(&draw, 104, |point| {
            reads.push(point);
            (point == origin).then_some(bytes)
        }),
        Ok(true)
    );
    assert_eq!(reads, [other_origin, origin]);

    let other_point = PalwBlockContextV2 { block: h64(8), ..vote_point };
    assert!(
        apply_palw_transition_v2_with_extras(&drawn, &params, &other_point, &[], None, false, false, false, false, &vote_extras,)
            .is_err()
    );

    let mut invalid_delta = delta.clone();
    let future_origin = AcceptedOrigin { accepted_blue_score: point.blue_score + 1, ..row.carrier().origin };
    let future_row = PublicOriginalAdmissionRecordV1::storage_fixture(original, future_origin);
    let new = invalid_delta
        .entries
        .iter_mut()
        .find_map(|entry| match entry {
            PalwDeltaEntryV2::KernelRouteRow { table: PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, new, .. } => Some(new),
            _ => None,
        })
        .unwrap();
    *new = Some(borsh::to_vec(&future_row).unwrap());
    assert!(matches!(apply_delta_v2(&parent, &invalid_delta, &params), Err(PalwStateV2Error::CarriageInconsistent(_))));
    let table = PUBLIC_ORIGINAL_ADMISSION_TABLE_V1;
    let key = original.0.0.to_vec();
    let old = Some(borsh::to_vec(&row).unwrap());
    let changed_origin = AcceptedOrigin { accepted_tx_ordinal: 2, ..row.carrier().origin };
    let replacement = Some(borsh::to_vec(&PublicOriginalAdmissionRecordV1::storage_fixture(original, changed_origin)).unwrap());
    let mut builder = TransitionBuilder::new(&child, &params, false, false, false, false, &extras);
    builder.write_kernel_row(table, key.clone(), old.clone()).unwrap();
    assert!(builder.entries.is_empty());
    assert!(matches!(builder.write_kernel_row(table, key.clone(), replacement.clone()), Err(PalwStateV2Error::KernelRouteRefused(_))));
    assert!(builder.entries.is_empty());
    assert_eq!(builder.state, child);
    let mut state = child.clone();
    assert!(matches!(
        apply_kernel_route_row_v1(&mut state, table, &key, &old, &replacement, false),
        Err(PalwStateV2Error::DeltaMismatch("public verification row is immutable"))
    ));
    assert_eq!(state, child);
}
