//! RFC-0006 source controls for the remaining shared writers (`palw_state_v2/native_writers.rs`): payout, exposure, court
//! group, deadline index, encoded vertex/improvement rows, kernel-route rows and the lenient-consumer capacity guard.
//! SOURCE ONLY — written, never compiled or executed in this lane. They assert ordering properties (refusal before any state or
//! journal change; flags-off byte parity), not capacity numbers.
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
    let (mut parent, _) = apply(&PalwChainStateV2::genesis(), &old, &ctx(1, 100, 1), &register_class_and_bond(), None);
    parent.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 100, burned: 0 },
    ));
    let profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).unwrap();
    let class = profile.shape_profile_id();
    let p = old.with_memory_routing_court_from_daa(Some(101));
    let mut object = registration(class, 0, Some(bond_key(1)));
    if let PalwConsensusObjectV2::ClassRegistered { slash_value_per_pwu, .. } = &mut object {
        *slash_value_per_pwu = parent.class(&h64(1)).unwrap().slash_value_per_pwu;
    }
    let (child, delta) = apply(&parent, &p, &ctx(2, 101, 2), &[object], None);
    assert_eq!(&*child.reader_router_profile_v2(class).unwrap().unwrap(), &profile);
    assert!(child.fp_work_profile_of(&class).is_none(), "registration must not populate the price/deadline index");
    assert!(!child.fp_certified_classes.contains_key(&class), "graph identity is not model approval");
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
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 {
        memory_bytes: u64::MAX, database_bytes: 0,
    });
    let mut builder = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    builder.write_reader_router_cell_v2(&[7; 64], Some(&11u64)).unwrap();
    let required = checker.outstanding().0;
    assert!(required > 0);
    assert!(!builder.entries.is_empty());
    drop(builder);
    assert_eq!(checker.outstanding(), (0, 0));

    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 {
        memory_bytes: required - 1, database_bytes: 0,
    });
    let mut builder = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    assert!(matches!(
        builder.write_reader_router_cell_v2(&[7; 64], Some(&11u64)),
        Err(PalwStateV2Error::NativeResource(_))
    ));
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
