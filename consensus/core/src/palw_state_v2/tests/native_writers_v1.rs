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
    assert_eq!(restored, child, "native snapshot restore retains the registered graph");
    assert_eq!(&*restored.reader_router_profile_v2(class).unwrap().unwrap(), &profile);
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
        public_one_honest::OriginalId,
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
    let original = OriginalId(Digest([3; 64]));
    let row = PublicOriginalAdmissionRecordV1::storage_fixture(
        original,
        AcceptedOrigin {
            accepting_block: NativeHash64(point.block.as_bytes()),
            carrier_block: NativeHash64(h64(3).as_bytes()),
            carrier_transaction: NativeHash64(h64(4).as_bytes()),
            accepted_tx_ordinal: 1,
            event_ordinal: 0,
            accepted_daa: point.daa_score,
            accepted_blue_score: point.blue_score,
        },
    );
    let extras = PalwTransitionExtrasV1 {
        kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: h64(1),
            ruleset_digest: h64(2),
            ..Default::default()
        }),
        exec_v2_test_kernel_rows: vec![(
            PUBLIC_ORIGINAL_ADMISSION_TABLE_V1,
            original.0.0.to_vec(),
            Some(borsh::to_vec(&row).unwrap()),
        )],
        ..Default::default()
    };
    let (child, delta) =
        apply_palw_transition_v2_with_extras(&parent, &params, &point, &[], None, false, false, false, false, &extras).unwrap();
    assert_eq!(child.public_original_admission_record_v1(original), Ok(Some(row)));
    assert_eq!(apply_delta_v2(&parent, &delta, &params).unwrap(), child);
    assert_eq!(revert_delta_v2(&child, &delta, &params).unwrap(), parent);
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&child)).unwrap();
    let restored = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&params, Some(child.state_root())).unwrap();
    assert_eq!(restored, child);
    assert_eq!(restored.public_original_admission_record_v1(original), Ok(Some(row)));
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
