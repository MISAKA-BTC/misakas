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
