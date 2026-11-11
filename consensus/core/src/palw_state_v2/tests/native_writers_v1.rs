//! RFC-0006 source controls for the remaining shared writers (`palw_state_v2/native_writers.rs`): payout, exposure, court
//! group, deadline index, encoded vertex/improvement rows, kernel-route rows and the lenient-consumer capacity guard.
//! SOURCE ONLY — written, never compiled or executed in this lane. They assert ordering properties (refusal before any state or
//! journal change; flags-off byte parity), not capacity numbers.
#![allow(warnings)]
use super::*;
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};

/// Structural TFO3 recognition selects resource ownership even with the
/// unrelated BCT2 transport disabled. It does not authorize public intake.
pub(crate) fn exercise_public_pairing_construction_owner_v1(
    pairing: &crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
) {
    let p = params();
    let parent = PalwChainStateV2::genesis();
    let mut extras = PalwTransitionExtrasV1::default();
    assert!(extras.batch_plain_output_transport_v2.is_none());
    extras.tir_funded_originals.insert(pairing.claim(), pairing.clone());
    let builder = TransitionBuilder::try_new(&parent, &p, false, false, false, false, &extras).unwrap();
    assert!(builder.native_writers_tracking_v1());
    assert!(builder.native_reservation.requests_v1() > 0);
    let copy = builder.native_reservation.reserve_checkpoint_copy_v1(&builder.state).unwrap();
    assert!(copy.is_some());
    assert_eq!(builder.state, parent);
    assert!(builder.entries.is_empty());
}

pub(crate) fn exercise_protected_service_candidate_v1(
    frame: &crate::palw_positive_contract_v2::PublicTfp1FrameV1,
    booking: &crate::palw_positive_contract_v2::tfp1_original::preclaim::ProtectedBatchBookingV1,
    before: &crate::palw_court_service_v1::ServiceReservations,
    after: &crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_service_v1::ServiceReservations>,
) {
    use crate::palw_court_service_state_v2::{CourtServiceStateV2, court_service_profile_id_v2};
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    assert!(before.packets.is_empty(), "fixture seed has no unrelated claim authority");
    let mut prefix = PalwChainStateV2::genesis();
    let source_profile_id = h64(0x5100);
    prefix.staged_courts.insert(
        Key::CourtService,
        Row::CourtService(
            Box::new(CourtServiceStateV2 {
                version: 2,
                profile_id: court_service_profile_id_v2(source_profile_id),
                source_profile_id,
                book: before.clone(),
                claims: BTreeMap::new(),
            })
            .into(),
        ),
    );
    crate::palw_positive_contract_v2::tfp1_original::service_booking::tests::exercise_protected_service_booking_v1(
        &prefix, frame, booking, after,
    );
}

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
    let members = [
        PublicDrawMemberV1 { bond, principal: member, rescue_reservation: Digest([9; 64]) },
        PublicDrawMemberV1 { bond: other_bond, principal: member, rescue_reservation: Digest([10; 64]) },
    ];
    let draw = prepared.prepare_draw(Digest([5; 64]), draw_point, &members, Digest([6; 64])).unwrap();
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
        Claimant, MANIFEST_REQUEST_SIGN_CONTEXT, ManifestRequestContext, SignedManifestRequestV1,
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
    let request = prepared
        .drawn_manifest_request(&draw, &commit, bond, request_context.inspection_scope, request_context.supply_obligation)
        .unwrap();
    assert_eq!(request.signer, member);
    assert_eq!(request.reservation, request_context.reservation);
    assert_eq!(
        prepared.drawn_manifest_request(
            &draw,
            &commit,
            PanelBond { index: 1, ..bond },
            request_context.inspection_scope,
            request_context.supply_obligation,
        ),
        Err(misaka_palw_positive_contract::ContractError::Identity)
    );
    assert_eq!(
        prepared.drawn_manifest_request(&draw, &commit, bond, Digest([12; 64]), request_context.supply_obligation),
        Err(misaka_palw_positive_contract::ContractError::Scope)
    );
    let case = request.case_id(&commit, request_context.claimant).unwrap();
    let signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &request.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT, [2; 32])
            .unwrap();
    let wire = borsh::to_vec(&SignedManifestRequestV1 { request, key, signature: *signature.as_ref() }).unwrap();
    let pending = prepared.prepare_drawn_manifest_storage_record(Digest([2; 64]), &draw, &wire, &request_context).unwrap();
    assert_eq!(drawn.last_point(), Some(&draw_point));
    assert!(pending.storage_row(&drawn, &vote_point).is_err());
    let (pending_key, pending_row) = pending.storage_row_at_v1(&drawn, &vote_point).unwrap().unwrap();
    assert_eq!(pending_key, misaka_palw_positive_contract::public_one_honest::PendingManifestCaseRecordV1::key(original, case));
    assert_eq!(pending_row.origin(), origin);
    for wrong_point in [
        PalwBlockContextV2 { block: h64(5), ..vote_point },
        PalwBlockContextV2 { daa_score: vote_point.daa_score + 1, ..vote_point },
        PalwBlockContextV2 { blue_score: vote_point.blue_score + 1, ..vote_point },
    ] {
        assert_eq!(pending.storage_row_at_v1(&drawn, &wrong_point), Err(misaka_palw_positive_contract::ContractError::Scope),);
    }
    let case_extras = PalwTransitionExtrasV1 {
        kernel_route: extras.kernel_route.clone(),
        public_drawn_manifest_cases: vec![pending.clone(), pending],
        ..Default::default()
    };
    let (with_case, case_delta) =
        apply_palw_transition_v2_with_extras(&drawn, &params, &vote_point, &[], None, false, false, false, false, &case_extras)
            .unwrap();
    assert!(with_case.public_pending_manifest_record_v1(original, case).unwrap().is_some());
    // Ordered same-block intake can encounter its own already-installed row
    // before LastPoint advances. The explicit-point lookup must be idempotent
    // there too, without making an ordinary parent snapshot accept a future row.
    let mut same_prefix = with_case.clone();
    same_prefix.last_point = Some(draw_point);
    let prepared_case = &case_extras.public_drawn_manifest_cases[0];
    assert_eq!(prepared_case.storage_row_at_v1(&same_prefix, &vote_point), Ok(None));
    assert!(prepared_case.storage_row(&same_prefix, &vote_point).is_err());
    assert_eq!(apply_delta_v2(&drawn, &case_delta, &params).unwrap(), with_case);
    assert_eq!(revert_delta_v2(&with_case, &case_delta, &params).unwrap(), drawn);
    let restored_case = PalwStateCarriageV2::from_state(&with_case).into_state(&params, Some(with_case.state_root())).unwrap();
    // A first filer can bind only its own protected rank. The second bond
    // retains its rank even if the same principal owns both distinct bonds.
    let first_rank = prepared.rooted_drawn_batch_opportunity_v1(&drawn, &draw, bond).unwrap();
    let other_rank = prepared.rooted_drawn_batch_opportunity_v1(&drawn, &draw, other_bond).unwrap();
    assert_eq!((first_rank.index_v1(), other_rank.index_v1()), (1, 0));
    assert_eq!(first_rank.rescue_reservation_v1(), request_context.reservation);
    assert_eq!(first_rank.capacity_v1(), draw.rescue_capacity(bond).unwrap());
    assert!(prepared.rooted_drawn_batch_opportunity_v1(&child, &draw, bond).is_err());
    let foreign_draw = prepared.prepare_draw(Digest([12; 64]), draw_point, &members, Digest([6; 64])).unwrap();
    assert!(prepared.rooted_drawn_batch_opportunity_v1(&drawn, &foreign_draw, bond).is_err());
    assert!(prepared.rooted_drawn_batch_case_v1(&drawn, &draw, &wire, &request_context).is_err());
    let first_case = prepared.rooted_drawn_batch_case_v1(&restored_case, &draw, &wire, &request_context).unwrap();
    assert_eq!(first_case.case_id_v1(), case);
    assert_eq!(first_case.opportunity_v1(), first_rank);
    assert_eq!(prepared.rooted_drawn_batch_opportunity_v1(&restored_case, &draw, other_bond), Ok(other_rank));

    // A distinct accepting block may have the same DAA. Both requests
    // must fit the fixture's fixed first-request deadline at DAA 103.
    let other_point = ctx(5, vote_point.daa_score, 5);
    let other_context = ManifestRequestContext {
        claimant: Claimant::DrawnBond(other_bond),
        reservation: other_rank.rescue_reservation_v1(),
        origin: AcceptedOrigin {
            accepting_block: NativeHash64(other_point.block.as_bytes()),
            accepted_daa: other_point.daa_score,
            accepted_blue_score: other_point.blue_score,
            carrier_block: NativeHash64(h64(12).as_bytes()),
            carrier_transaction: NativeHash64(h64(13).as_bytes()),
            ..origin
        },
        ..request_context
    };
    let other_request = prepared
        .drawn_manifest_request(&draw, &commit, other_bond, other_context.inspection_scope, other_context.supply_obligation)
        .unwrap();
    let other_signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &other_request.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT, [3; 32])
            .unwrap();
    let other_wire =
        borsh::to_vec(&SignedManifestRequestV1 { request: other_request, key, signature: *other_signature.as_ref() }).unwrap();
    assert!(prepared.rooted_drawn_batch_case_v1(&restored_case, &draw, &other_wire, &other_context).is_err());
    assert!(prepared.rooted_drawn_batch_case_v1(&restored_case, &draw, &wire, &other_context).is_err());
    let other_pending = prepared.prepare_drawn_manifest_storage_record(Digest([2; 64]), &draw, &other_wire, &other_context).unwrap();
    let (both_cases, other_delta) = apply_palw_transition_v2_with_extras(
        &with_case,
        &params,
        &other_point,
        &[],
        None,
        false,
        false,
        false,
        false,
        &PalwTransitionExtrasV1 { public_drawn_manifest_cases: vec![other_pending], ..draw_extras_without_votes.clone() },
    )
    .unwrap();
    let other_case = prepared.rooted_drawn_batch_case_v1(&both_cases, &draw, &other_wire, &other_context).unwrap();
    assert_eq!(other_case.opportunity_v1(), other_rank);
    assert_ne!(first_case.case_id_v1(), other_case.case_id_v1());
    assert_ne!(first_case.case_scope_v1(h64(14)).unwrap(), other_case.case_scope_v1(h64(14)).unwrap());
    assert_eq!(prepared.rooted_drawn_batch_case_v1(&both_cases, &draw, &wire, &request_context), Ok(first_case));
    let rolled_back = revert_delta_v2(&both_cases, &other_delta, &params).unwrap();
    assert_eq!(rolled_back, with_case);
    assert!(prepared.rooted_drawn_batch_case_v1(&rolled_back, &draw, &other_wire, &other_context).is_err());
    assert_eq!(prepared.rooted_drawn_batch_opportunity_v1(&rolled_back, &draw, other_bond), Ok(other_rank));

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

/// Native identity rows are fixtures; the caller supplies a real signed TFO3
/// and its exact fresh escrow. This does not simulate a paid public admission.
pub(crate) fn prepare_tfp1_link_writer_fixture_v1(
    fx: &crate::palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1,
    point: PalwBlockContextV2,
    admission: crate::palw_positive_contract_v2::PreparedPublicOriginalRecordV1,
    escrow: crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1,
) -> (PalwChainStateV2, PalwStateParamsV2) {
    let (mut state, params, _) = super::super::palw_tir_public_fold_v1::conviction_e2e_tests::world(fx, &fx.frame);
    let id = fx.frame.plan_v1().original_claim;
    assert_eq!(point.daa_score, fx.frame.plan_v1().terms.accepted_daa);
    let claim = state.claims.get_mut(&id).unwrap();
    claim.accepted_block = point.block;
    claim.accepted_blue_score = point.blue_score;
    state.last_point = Some(point);
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: point.daa_score, burned: 0 },
    ));
    state.funded_escrows_by_claim.insert((escrow.claim, escrow.contract_id));
    state.funded_escrows.insert(escrow.contract_id, escrow);
    let (key, row) = admission.storage_row(&state, &point).unwrap().unwrap();
    let bytes = borsh::to_vec(row).unwrap();
    state
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((crate::palw_positive_contract_v2::PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, key.to_vec()), bytes);
    (state, params)
}

/// Storage-only exercise using the same actual paired companion as the
/// legacy link regression. It does not represent complete G14 admission.
pub(crate) fn exercise_public_tfp1_frame_writer_v1(
    mut parent: PalwChainStateV2,
    point: PalwBlockContextV2,
    params: &PalwStateParamsV2,
    claim: Hash64,
    original: misaka_palw_positive_contract::public_one_honest::OriginalId,
    material_until: u64,
    make: impl Fn(
        &PalwChainStateV2,
    ) -> (
        crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_positive_contract_v2::PublicTfp1FrameV1>,
        crate::palw_positive_contract_v2::tfp1_original::native_link::PreparedTfp1OriginalHistoryV1,
        super::super::objects::Tfp1OriginalPrefixV1,
    ),
) {
    use crate::palw_positive_contract_v2::PUBLIC_ORIGINAL_ADMISSION_TABLE_V1;
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    // Build an independent new-profile storage fixture, not an upgrade or
    // removal of any publicly accepted legacy claim.
    let legacy_key = Key::TirClaim(claim);
    let old = parent.staged_courts.remove(&legacy_key).unwrap();
    super::super::palw_staged_court_fold_v1::update_indices(&mut parent, legacy_key, Some(&old), None);
    parent.kernel_route.as_mut().unwrap().aux.remove(&(PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, original.0.0.to_vec())).unwrap();
    parent.claims.get_mut(&claim).unwrap().trace_retention_daa = material_until;
    let key = Key::PublicTfp1FrameV1(claim);
    assert!(!parent.public_tfp1_escrow_requires_native_settlement_v1(claim));
    let extras = PalwTransitionExtrasV1::default();
    let (frame, _, _) = make(&parent);
    let encoded = borsh::to_vec(&Row::PublicTfp1FrameV1(Box::new((*frame).clone()).into())).unwrap();
    assert_eq!(borsh::to_vec(&key).unwrap()[0], 39);
    assert_eq!(encoded[0], 46);
    crate::palw_native_resource_v1::preflight_native_value_v1::<Row>(&encoded, encoded.len() as u64).unwrap();
    assert_eq!(borsh::to_vec(&Key::TirClaim(claim)).unwrap()[0], 9, "legacy key tag unchanged");
    assert_eq!(borsh::to_vec(&old).unwrap()[0], 15, "legacy row tag unchanged");
    let mut generic = TransitionBuilder::new(&parent, params, false, false, false, false, &extras);
    assert!(generic.write_staged_row_v1(key, Some(Row::PublicTfp1FrameV1(Box::new((*frame).clone()).into()))).is_err());
    assert!(generic.entries.is_empty());
    assert_eq!(generic.state, parent);
    drop(generic);

    let (frame, history, _) = make(&parent);
    let mut untracked = TransitionBuilder::new(&parent, params, false, false, false, false, &extras);
    assert!(matches!(
        untracked.install_public_tfp1_original_frame_v1(&point, frame, history),
        Err(PalwStateV2Error::NativeResource(_))
    ));
    assert_eq!(untracked.state, parent);
    assert!(untracked.entries.is_empty());
    drop(untracked);
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });

    // Missing escrow fails after the frame has been staged. A bad prefix
    // fails in the three-row writer. Neither leaves a frame, indices or rows.
    for missing_escrow in [true, false] {
        let mut failed = TransitionBuilder::new(&parent, params, false, false, false, false, &extras);
        failed.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
        if missing_escrow {
            failed.state.funded_escrows.values_mut().find(|row| row.claim == claim).unwrap().liability_open = false;
        }
        let (frame, history, prefix) = make(&parent);
        failed.tfp1_history_journal_parent = Some(prefix.parent_v1());
        if missing_escrow {
            failed.tfp1_original_prefix = Some(prefix);
        }
        let before = failed.state.clone();
        let error = failed.install_public_tfp1_original_frame_v1(&point, frame, history).unwrap_err();
        if missing_escrow {
            assert!(
                matches!(error, PalwStateV2Error::CarriageInconsistent(reason) if reason == "public TFP1 frame/link/history mismatch")
            );
        } else {
            assert!(
                matches!(error, PalwStateV2Error::KernelRouteRefused(reason) if reason == "TFP1 original lacks the live accepting prefix")
            );
        }
        assert!(failed.entries.is_empty());
        assert_eq!(failed.state, before);
    }
    let (frame, history, prefix) = make(&parent);
    let mut builder = TransitionBuilder::new(&parent, params, false, false, false, false, &extras);
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    builder.tfp1_history_journal_parent = Some(prefix.parent_v1());
    builder.tfp1_original_prefix = Some(prefix);
    builder.install_public_tfp1_original_frame_v1(&point, frame, history).unwrap();
    assert_eq!(builder.entries.len(), 4);
    assert!(
        matches!(&builder.entries[0], PalwDeltaEntryV2::StagedCourtSeed { key: Key::PublicTfp1FrameV1(id), old: None, new: Some(Row::PublicTfp1FrameV1(_)) } if *id == claim)
    );
    assert!(builder.state.public_tfp1_original_link_v1(claim).unwrap().unwrap().uses_public_frame_v1());
    assert!(builder.state.court_service_holds_material_v1(&key));
    assert_eq!(builder.state.staged_tir_source_until_v1(claim), Some(material_until));
    assert!(builder.state.public_tfp1_escrow_requires_native_settlement_v1(claim));
    super::super::funded_escrow::tests::exercise_protected_funded_liability_v1(&builder.state, params, claim);

    // These four rows are the writer's intermediate result. The accepting
    // checkpoint must append the protected service book before publication.
    assert!(builder.state.validate_public_verification_rows_v1().is_err());
    let delta = PalwStateDeltaV2 { point, entries: builder.entries };
    assert!(apply_delta_v2(&parent, &delta, params).is_err());
    assert!(revert_delta_v2(&builder.state, &delta, params).is_err());
    assert!(PalwChainStateV2::genesis().validate_public_verification_delta_v1(&delta.entries, true).is_err());

    // Retirement observes the same retained horizon even without an old
    // TIR frame. Phase changes below are reader-only fixtures, not Final.
    let mut terminal = builder.state.clone();
    terminal.claims.get_mut(&claim).unwrap().phase = PalwClaimPhaseV2::Final { final_daa: point.daa_score + 1 };
    assert_eq!(terminal.staged_retirement_floor_v1(&claim, material_until - 1), material_until);
    assert_eq!(terminal.staged_retirement_floor_v1(&claim, material_until + 1), material_until + 1);
    let mut malformed_source = builder.state;
    malformed_source.staged_courts.insert(key, old.clone());
    assert_eq!(malformed_source.staged_tir_source_until_v1(claim), Some(u64::MAX), "wrong row kind cannot release source capital");
}

pub(crate) fn exercise_tfp1_link_writer_v1(
    mut state: PalwChainStateV2,
    point: PalwBlockContextV2,
    prepared: crate::palw_positive_contract_v2::tfp1_original::native_link::PreparedTfp1OriginalHistoryLinkV1,
    prefix: super::super::objects::Tfp1OriginalPrefixV1,
    foreign_prefix: super::super::objects::Tfp1OriginalPrefixV1,
    params: &PalwStateParamsV2,
) -> PalwChainStateV2 {
    use crate::palw_positive_contract_v2::{PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, PUBLIC_TFP1_LINK_TABLE_V1};
    use crate::palw_tfp1_original_history_v1::{PUBLIC_TFP1_HISTORY_TABLE_V1, rooted_tfp1_original_history_v1};
    let (key, row) = prepared.link_v1().storage_row(&state, &point).unwrap().unwrap();
    let row = *row;
    let bytes = borsh::to_vec(&row).unwrap();
    let extras = PalwTransitionExtrasV1::default();
    let pin_prefix = |builder: &mut TransitionBuilder<'_>| {
        builder.tfp1_history_journal_parent = Some(prefix.parent_v1());
        builder.tfp1_original_prefix = Some(prefix);
    };
    let mut partial = TransitionBuilder::new(&state, params, false, false, false, false, &extras);
    pin_prefix(&mut partial);
    assert!(partial.install_public_tfp1_original_v1(&point, &prepared).is_err(), "preexisting 198 is not repaired");
    assert!(partial.entries.is_empty());
    assert_eq!(partial.state, state);
    drop(partial);

    let admission_key = row.original_v1().0.0.to_vec();
    assert!(state.kernel_route.as_mut().unwrap().aux.remove(&(PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, admission_key)).is_some());

    let mut without_prefix = TransitionBuilder::new(&state, params, false, false, false, false, &extras);
    assert!(matches!(
        without_prefix.install_public_tfp1_original_v1(&point, &prepared),
        Err(PalwStateV2Error::KernelRouteRefused(reason)) if reason.contains("lacks the live accepting prefix")
    ));
    assert!(without_prefix.entries.is_empty());
    assert_eq!(without_prefix.state, state);
    pin_prefix(&mut without_prefix);
    without_prefix.tfp1_original_prefix = Some(foreign_prefix);
    assert_ne!(prefix.root_v1(), foreign_prefix.root_v1());
    assert!(matches!(
        without_prefix.install_public_tfp1_original_v1(&point, &prepared),
        Err(PalwStateV2Error::KernelRouteRefused(reason)) if reason.contains("substitutes the live accepting prefix")
    ));
    assert!(without_prefix.entries.is_empty());
    assert_eq!(without_prefix.state, state);
    drop(without_prefix);

    let mut missing_escrow = TransitionBuilder::new(&state, params, false, false, false, false, &extras);
    pin_prefix(&mut missing_escrow);
    missing_escrow.state.funded_escrows.values_mut().find(|escrow| escrow.claim == row.claim_v1()).unwrap().liability_open = false;
    let first_row_checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
    missing_escrow.native_reservation = NativeTransitionReservationV1::for_checker_v1(&first_row_checker);
    let failed_state = missing_escrow.state.clone();
    assert!(matches!(
        missing_escrow.install_public_tfp1_original_v1(&point, &prepared),
        Err(PalwStateV2Error::KernelRouteRefused(reason)) if reason.contains("invalid paired TFP1 link")
    ));
    assert!(missing_escrow.entries.is_empty(), "late link failure must rewind the earlier admission journal");
    assert_eq!(missing_escrow.state, failed_state);
    let first_row_requests = first_row_checker.outstanding().0;
    assert!(first_row_requests > 0);
    drop(missing_escrow);
    assert_eq!(first_row_checker.outstanding(), (0, 0));

    let mut previous_state = state.clone();
    let previous_point = PalwBlockContextV2 { block: h64(998), blue_score: point.blue_score - 1, ..point };
    previous_state.last_point = Some(previous_point);
    let mut before_last_point = TransitionBuilder::new(&previous_state, params, false, false, false, false, &extras);
    pin_prefix(&mut before_last_point);
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
    before_last_point.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    before_last_point.install_public_tfp1_original_v1(&point, &prepared).unwrap();
    let required = checker.outstanding().0;
    assert!(required > first_row_requests);
    before_last_point.tfp1_original_prefix = None;
    before_last_point.tfp1_history_journal_parent = None;
    before_last_point.install_public_tfp1_original_v1(&point, &prepared).unwrap();
    assert_eq!(before_last_point.entries.len(), 3);
    assert_eq!(before_last_point.state.last_point(), Some(&previous_point));
    drop(before_last_point);
    assert_eq!(checker.outstanding(), (0, 0));

    // One byte below the concrete writer's request lets it pass the first
    // insertion and fail a later reservation. Both state and journal roll back.
    let short_checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: required - 1, database_bytes: 0 });
    let mut short = TransitionBuilder::new(&previous_state, params, false, false, false, false, &extras);
    pin_prefix(&mut short);
    short.native_reservation = NativeTransitionReservationV1::for_checker_v1(&short_checker);
    assert!(matches!(short.install_public_tfp1_original_v1(&point, &prepared), Err(PalwStateV2Error::NativeResource(_))));
    assert!(short_checker.outstanding().0 > first_row_requests, "failure followed the admission insertion");
    assert!(short.entries.is_empty());
    assert_eq!(short.state, previous_state);
    assert_eq!(short.state.state_root(), previous_state.state_root());
    drop(short);
    assert_eq!(short_checker.outstanding(), (0, 0));

    // Only this writer's incremental requests are compared. The fixture
    // supplies the same sealed prefix and excludes its construction and hashing.
    let mut larger_state = previous_state.clone();
    for n in 1_000..1_032 {
        assert!(larger_state.pending_payouts.insert(h64(n), PalwPayoutV2 { payload: h64(n), amount: 1 }).is_none());
    }
    assert_ne!(larger_state.state_root(), previous_state.state_root());
    let larger_checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: required, database_bytes: 0 });
    let mut larger = TransitionBuilder::new(&larger_state, params, false, false, false, false, &extras);
    pin_prefix(&mut larger);
    larger.native_reservation = NativeTransitionReservationV1::for_checker_v1(&larger_checker);
    larger.install_public_tfp1_original_v1(&point, &prepared).unwrap();
    assert_eq!(larger_checker.outstanding().0, required);
    assert_eq!(larger.state.pending_payouts, larger_state.pending_payouts);
    drop(larger);
    assert_eq!(larger_checker.outstanding(), (0, 0));

    let mut builder = TransitionBuilder::new(&state, params, false, false, false, false, &extras);
    pin_prefix(&mut builder);
    assert!(builder.write_kernel_row(PUBLIC_TFP1_LINK_TABLE_V1, key.to_vec(), Some(bytes.clone())).is_err());
    assert!(builder.write_kernel_row_encoded_v1(PUBLIC_TFP1_LINK_TABLE_V1, &key, Some(&row)).is_err());
    assert!(builder.entries.is_empty());
    assert_eq!(builder.state, state);
    let history_bytes = borsh::to_vec(prepared.boundary_v1()).unwrap();
    assert_eq!(history_bytes.len(), crate::palw_tfp1_original_history_v1::Tfp1OriginalHistoryBoundaryV1::WIRE_BYTES);
    assert!(builder.write_kernel_row(PUBLIC_TFP1_HISTORY_TABLE_V1, key.to_vec(), Some(history_bytes)).is_err());
    builder.install_public_tfp1_original_v1(&point, &prepared).unwrap();
    let tables: Vec<_> = builder
        .entries
        .iter()
        .map(|entry| match entry {
            PalwDeltaEntryV2::KernelRouteRow { table, old: None, new: Some(_), .. } => *table,
            other => panic!("unexpected TFO3 writer entry: {other:?}"),
        })
        .collect();
    assert_eq!(tables, [PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, PUBLIC_TFP1_LINK_TABLE_V1, PUBLIC_TFP1_HISTORY_TABLE_V1]);
    assert_eq!(builder.state.public_tfp1_original_link_v1(row.claim_v1()).unwrap(), Some(row));
    assert_eq!(rooted_tfp1_original_history_v1(&builder.state, row.claim_v1()).unwrap().as_ref(), Some(prepared.boundary_v1()));
    builder.install_public_tfp1_original_v1(&point, &prepared).unwrap();
    assert_eq!(builder.entries.len(), 3, "exact replay does not append more journal writes");
    builder.state.validate_public_verification_rows_v1().unwrap();
    assert!(builder.state.court_service_holds_claim_v1(&row.claim_v1()));
    assert!(
        builder
            .state
            .court_service_holds_material_v1(&crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirClaim(row.claim_v1()))
    );
    let delta = PalwStateDeltaV2 { point, entries: builder.entries.clone() };
    let replayed = apply_delta_v2(&state, &delta, &params).unwrap();
    assert_eq!(replayed.state_root(), builder.state.state_root());
    for missing in [PUBLIC_TFP1_LINK_TABLE_V1, PUBLIC_TFP1_HISTORY_TABLE_V1] {
        let mut partial = delta.clone();
        partial.entries.retain(|entry| !matches!(entry, PalwDeltaEntryV2::KernelRouteRow { table, .. } if *table == missing));
        assert!(apply_delta_v2(&state, &partial, &params).is_err(), "forward journal lost one paired row");
        assert!(revert_delta_v2(&replayed, &partial, &params).is_err(), "inverse journal lost one paired row");
    }
    let mut no_admission = delta.clone();
    no_admission
        .entries
        .retain(|entry| !matches!(entry, PalwDeltaEntryV2::KernelRouteRow { table: PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, .. }));
    assert!(apply_delta_v2(&state, &no_admission, &params).is_err(), "forward journal lost the paired companion admission");
    assert!(revert_delta_v2(&replayed, &no_admission, &params).is_err(), "inverse journal lost the paired companion admission");

    // Exercise the journal structure validator directly on a route-less
    // post-state, as can occur after undo removes the kernel route header.
    let route_less = PalwChainStateV2::genesis();
    assert!(route_less.kernel_route_state_v1().is_none());
    route_less.validate_public_verification_delta_v1(&delta.entries, true).unwrap();
    let mut reordered = delta.entries.clone();
    reordered.swap(1, 2);
    assert!(matches!(route_less.validate_public_verification_delta_v1(&reordered, true), Err(PalwStateV2Error::DeltaMismatch(_))));

    let mut wrong_cut = delta.entries.clone();
    let PalwDeltaEntryV2::KernelRouteRow { table: PUBLIC_TFP1_HISTORY_TABLE_V1, new: Some(bytes), .. } = &mut wrong_cut[2] else {
        panic!("expected TFO3 history as the third entry");
    };
    let mut history = *prepared.boundary_v1();
    assert_eq!(history.before_delta_end, 0, "the admission is the first entry in this journal");
    history.before_delta_end = 1;
    *bytes = borsh::to_vec(&history).unwrap();
    assert!(matches!(route_less.validate_public_verification_delta_v1(&wrong_cut, true), Err(PalwStateV2Error::DeltaMismatch(_))));

    let reverted = revert_delta_v2(&replayed, &delta, &params).unwrap();
    assert_eq!(reverted.state_root(), state.state_root());
    assert_eq!(reverted.public_tfp1_original_link_v1(row.claim_v1()).unwrap(), None);
    assert_eq!(rooted_tfp1_original_history_v1(&reverted, row.claim_v1()).unwrap(), None);
    let encoded = borsh::to_vec(&PalwStateCarriageV2::from_state(&replayed)).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(&encoded[..], encoded.len() as u64).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(&params, Some(replayed.state_root())).unwrap();
    assert_eq!(restored.public_tfp1_original_link_v1(row.claim_v1()).unwrap(), Some(row));
    assert_eq!(rooted_tfp1_original_history_v1(&restored, row.claim_v1()).unwrap().as_ref(), Some(prepared.boundary_v1()));
    let mut missing_claim = replayed.clone();
    missing_claim.claims.remove(&row.claim_v1());
    assert!(missing_claim.public_tfp1_original_link_v1(row.claim_v1()).is_err());
    let mut wrong_origin = replayed.clone();
    wrong_origin.claims.get_mut(&row.claim_v1()).unwrap().accepted_block = h64(999);
    assert!(wrong_origin.public_tfp1_original_link_v1(row.claim_v1()).is_err());
    let mut erase = replayed.clone();
    assert!(apply_kernel_route_row_v1(&mut erase, PUBLIC_TFP1_LINK_TABLE_V1, &key, &Some(bytes), &None, false).is_err());
    assert_eq!(erase.state_root(), replayed.state_root());
    let mut no_capacity = TransitionBuilder::new(&state, params, false, false, false, false, &extras);
    pin_prefix(&mut no_capacity);
    let _checker = zero_capacity(&mut no_capacity);
    assert!(matches!(no_capacity.install_public_tfp1_original_v1(&point, &prepared), Err(PalwStateV2Error::NativeResource(_))));
    assert!(no_capacity.entries.is_empty());
    assert_eq!(no_capacity.state, state);
    restored
}

/// Explicit fixture: seal a prefix with the same private owner used by the
/// actual ordered fold. A singleton builder must refuse to mint it.
pub(crate) fn tfp1_history_prefix_fixture_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    point: &PalwBlockContextV2,
    pairing: &crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
) -> super::super::objects::Tfp1OriginalPrefixV1 {
    let mut extras = PalwTransitionExtrasV1::default();
    extras.tir_funded_originals.insert(pairing.claim(), pairing.clone());
    let mut builder = TransitionBuilder::new(state, params, false, false, false, false, &extras);
    assert!(super::super::objects::capture_tfp1_prefix_v1(&mut builder, point, pairing.claim()).is_err());
    builder.tfp1_history_journal_parent = Some(pairing.history_inputs_v1().unwrap().parent_v1());
    let prefix = super::super::objects::capture_tfp1_prefix_v1(&mut builder, point, pairing.claim()).unwrap();
    assert_eq!(prefix.root_v1(), state.state_root());
    assert_eq!(prefix.end_v1(), 0);
    let wrong_point = PalwBlockContextV2 { block: h64(999), ..*point };
    assert!(super::super::objects::capture_tfp1_prefix_v1(&mut builder, &wrong_point, pairing.claim()).is_err());
    builder.tfp1_history_journal_parent = Some(h64(998));
    assert!(super::super::objects::capture_tfp1_prefix_v1(&mut builder, point, pairing.claim()).is_err());

    builder.tfp1_history_journal_parent = Some(pairing.history_inputs_v1().unwrap().parent_v1());
    let _checker = zero_capacity(&mut builder);
    assert!(matches!(
        super::super::objects::capture_tfp1_prefix_v1(&mut builder, point, pairing.claim()),
        Err(PalwStateV2Error::NativeResource(_))
    ));
    assert!(builder.entries.is_empty());
    assert_eq!(&builder.state, state);
    prefix
}

/// Exercise the paid writer on the caller's rooted draw and booked packet.
/// The fixture's handoff factory supplies the same validated UTXO carrier.
pub(crate) fn exercise_paid_manifest_writer_v1(
    parent: PalwChainStateV2,
    at: PalwBlockContextV2,
    params: &PalwStateParamsV2,
    prepared: &crate::palw_positive_contract_v2::tfp1_original::manifest_request::PreparedPaidManifestRequestV1,
    make_handoff: impl Fn() -> crate::palw_positive_contract_v2::tfp1_original::manifest_carrier::ValidatedManifestRequestCarrierV1,
) -> PalwChainStateV2 {
    use crate::palw_positive_contract_v2::{PUBLIC_MANIFEST_PAID_TABLE_V1, PUBLIC_PENDING_MANIFEST_TABLE_V1};

    let (pending, paid) = prepared.storage_rows_at_v1(&parent, &at).unwrap().unwrap();
    let paid_key = paid.key_v1();
    let paid_bytes = borsh::to_vec(paid).unwrap();
    let pending_key = prepared.pending_key_v1();
    let charge = prepared.charge_sompi_v1();
    assert!(charge > 0);

    let extras = PalwTransitionExtrasV1::default();
    let mut failed = TransitionBuilder::new(&parent, params, false, false, false, false, &extras);
    assert!(failed.write_kernel_row(PUBLIC_MANIFEST_PAID_TABLE_V1, paid_key.to_vec(), Some(paid_bytes.clone())).is_err());
    assert!(failed.write_kernel_row_encoded_v1(PUBLIC_MANIFEST_PAID_TABLE_V1, &paid_key, Some(paid)).is_err());
    assert!(failed.entries.is_empty());
    assert_eq!(failed.state, parent);

    let mut precharged = make_handoff();
    let other_purpose = if prepared.charge_purpose_v1() == h64(1) { h64(2) } else { h64(1) };
    precharged.allocate_charge_v1(prepared.carrier_v1(), other_purpose, 1).unwrap();
    let remaining = precharged.remaining_fee_v1();
    let failure = failed.install_paid_manifest_request_v1(&at, prepared, &mut precharged).unwrap_err();
    assert!(matches!(failure, PalwStateV2Error::KernelRouteRefused(reason) if reason.contains("DuplicateAllocation")));
    assert_eq!(precharged.remaining_fee_v1(), remaining);
    assert!(failed.entries.is_empty(), "fee failure must rewind the entire writer journal");
    assert_eq!(failed.state, parent, "fee failure must rewind both rows");

    let mut legacy = TransitionBuilder::new(&parent, params, false, false, false, false, &extras);
    legacy.write_kernel_row_encoded_v1(PUBLIC_PENDING_MANIFEST_TABLE_V1, &pending_key, Some(pending)).unwrap();
    let legacy_prefix = legacy.state.clone();
    let legacy_journal_len = legacy.entries.len();
    assert_ne!(legacy_journal_len, 0);
    let mut legacy_handoff = make_handoff();
    let legacy_remaining = legacy_handoff.remaining_fee_v1();
    assert!(matches!(
        legacy.install_paid_manifest_request_v1(&at, prepared, &mut legacy_handoff),
        Err(PalwStateV2Error::KernelRouteRefused(_))
    ));
    assert_eq!(legacy_handoff.remaining_fee_v1(), legacy_remaining);
    assert_eq!(legacy.entries.len(), legacy_journal_len);
    assert_eq!(legacy.state, legacy_prefix);
    assert!(!legacy.state.kernel_route().unwrap().aux.contains_key(&(PUBLIC_MANIFEST_PAID_TABLE_V1, paid_key.to_vec())));

    let mut builder = TransitionBuilder::new(&parent, params, false, false, false, false, &extras);
    let mut handoff = make_handoff();
    let before = handoff.remaining_fee_v1();
    assert_eq!(builder.install_paid_manifest_request_v1(&at, prepared, &mut handoff).unwrap(), charge);
    assert_eq!(handoff.remaining_fee_v1(), before - charge);
    assert!(prepared.storage_rows_at_v1(&builder.state, &at).unwrap().is_none());

    let rows = &builder.state.kernel_route().unwrap().aux;
    assert_eq!(rows.get(&(PUBLIC_MANIFEST_PAID_TABLE_V1, paid_key.to_vec())), Some(&paid_bytes));
    assert!(rows.contains_key(&(PUBLIC_PENDING_MANIFEST_TABLE_V1, pending_key.to_vec())));
    let journal_len = builder.entries.len();
    assert_eq!(
        builder
            .entries
            .iter()
            .filter(|entry| matches!(
                entry,
                PalwDeltaEntryV2::KernelRouteRow {
                    table: PUBLIC_MANIFEST_PAID_TABLE_V1 | PUBLIC_PENDING_MANIFEST_TABLE_V1,
                    old: None,
                    new: Some(_),
                    ..
                }
            ))
            .count(),
        2
    );
    assert_eq!(builder.install_paid_manifest_request_v1(&at, prepared, &mut handoff).unwrap(), 0);
    assert_eq!(handoff.remaining_fee_v1(), before - charge);
    let mut duplicate = make_handoff();
    assert_eq!(duplicate.remaining_fee_v1(), before);
    assert_eq!(builder.install_paid_manifest_request_v1(&at, prepared, &mut duplicate).unwrap(), 0);
    assert_eq!(duplicate.remaining_fee_v1(), before);
    assert_eq!(builder.entries.len(), journal_len);

    // Both phases replay the SAME immutable UTXO seed from the SAME parent.
    // Discarding a successful preview cannot consume the actual phase's fee.
    use crate::palw_positive_contract_v2::tfp1_original::manifest_carrier::replay::reconcile_manifest_request_fees_v1;
    let parent_root = parent.state_root();
    let seed = make_handoff().into_replay_seed_v1(parent_root).unwrap();
    let mut preview = TransitionBuilder::new(&parent, params, false, false, false, false, &extras);
    let expected_fee = preview.install_shared_paid_manifest_request_v1(parent_root, &at, prepared, &seed).unwrap();
    assert_eq!(expected_fee.charge_v1(), charge);
    assert_eq!(preview.state, builder.state);
    drop(preview);
    assert_eq!(seed.candidate_at_v1(parent_root, &at).unwrap().remaining_fee_v1(), before);
    let mut actual = TransitionBuilder::new(&parent, params, false, false, false, false, &extras);
    assert!(actual.install_shared_paid_manifest_request_v1(h64(0), &at, prepared, &seed).is_err());
    assert_eq!(actual.state, parent);
    assert!(actual.entries.is_empty());
    use crate::palw_batch_court_v1::fold_order::{AcceptedPublicNativeCourtItemV2, in_accepted_public_native_order_v2};
    let manifests = [(1, seed.clone())];
    let empty: &[(usize, usize)] = &[];
    let ordered = in_accepted_public_native_order_v2(
        [0, 1].into_iter(),
        empty,
        empty,
        &manifests,
        |n| *n,
        |n| *n,
        |seed| seed.accepted_ordinal_v1(),
    )
    .unwrap();
    let mut ordinary_seen = 0;
    let mut actual_fee = None;
    for item in ordered {
        match item {
            AcceptedPublicNativeCourtItemV2::Ordinary(_) => ordinary_seen += 1,
            AcceptedPublicNativeCourtItemV2::Manifest(seed) => {
                assert_eq!(ordinary_seen, 1);
                actual_fee = Some(actual.install_shared_paid_manifest_request_v1(parent_root, &at, prepared, seed).unwrap());
            }
            _ => panic!("the fixture has no Batch or Tokenizer carrier"),
        }
    }
    let actual_fee = actual_fee.unwrap();
    let totals = reconcile_manifest_request_fees_v1(&[expected_fee], &[actual_fee], parent_root, &at, 1).unwrap();
    assert_eq!(totals.charge, charge);
    assert_eq!(totals.collectible_fee, before - charge);
    assert_eq!(actual.state, builder.state);
    let duplicate_fee = actual.install_shared_paid_manifest_request_v1(parent_root, &at, prepared, &seed).unwrap();
    assert_eq!(duplicate_fee.charge_v1(), 0);
    assert!(reconcile_manifest_request_fees_v1(&[expected_fee], &[duplicate_fee], parent_root, &at, 1).is_err());
    assert!(reconcile_manifest_request_fees_v1(&[actual_fee, actual_fee], &[actual_fee, actual_fee], parent_root, &at, 2).is_err());

    let previous = builder.state.last_point;
    assert_ne!(previous, Some(at), "the fixture must exercise an unadvanced prefix");
    builder.entries.push(PalwDeltaEntryV2::LastPoint { old: previous, new: Some(at) });
    builder.state.last_point = Some(at);
    builder.state.validate_public_verification_rows_v1().unwrap();
    let delta = PalwStateDeltaV2 { point: at, entries: builder.entries.clone() };
    let replayed = apply_delta_v2(&parent, &delta, params).unwrap();
    assert_eq!(replayed.state_root(), builder.state.state_root());
    let reverted = revert_delta_v2(&replayed, &delta, params).unwrap();
    assert_eq!(reverted.state_root(), parent.state_root());

    let encoded = borsh::to_vec(&PalwStateCarriageV2::from_state(&replayed)).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(&encoded[..], encoded.len() as u64).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(params, Some(replayed.state_root())).unwrap();
    assert_eq!(restored.state_root(), replayed.state_root());
    assert!(prepared.storage_rows_at_v1(&restored, &at).unwrap().is_none());
    restored
}

/// Seed only the test's already booked service opportunity. The PMR1
/// preparer and writer still perform the native binding and fee checks.
pub(crate) fn seed_manifest_service_book_v1(
    state: &mut PalwChainStateV2,
    service: crate::palw_court_service_release_v1::CourtServiceRowV3,
    params: &PalwStateParamsV2,
) {
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1, PalwStagedCourtRowV1};

    service.check().unwrap();
    service.state.check_context(state).unwrap();
    let key = PalwStagedCourtKeyV1::CourtService;
    assert!(!state.staged_courts.contains_key(&key));
    let row = PalwStagedCourtRowV1::CourtServiceV3(Box::new(service).into());
    let extras = PalwTransitionExtrasV1 { tir_public_conviction_active: true, ..Default::default() };
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
    let mut builder = TransitionBuilder::new(state, params, false, false, false, false, &extras);
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    builder.write_staged_row_v1(key, Some(row)).unwrap();
    let delta = PalwStateDeltaV2 { point: *state.last_point().unwrap(), entries: builder.entries.clone() };
    let replayed = apply_delta_v2(state, &delta, params).unwrap();
    assert_eq!(replayed.state_root(), builder.state.state_root());
    assert_eq!(revert_delta_v2(&replayed, &delta, params).unwrap().state_root(), state.state_root());
    drop(builder);
    *state = replayed;
}
