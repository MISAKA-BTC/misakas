//! Regression contracts for scoped, journaled table writes.

use super::*;
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};

#[test]
fn scoped_row_writers_preserve_noop_policy_and_ordered_delta_bytes() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let parent = PalwChainStateV2::genesis();
    let mut builder = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let bond = bond_key(1);
    let payout = PalwPayoutV2 { payload: h64(2), amount: 7 };
    let position = (h64(3), h64(4));

    builder.write_exposure(bond, Some(5)).unwrap();
    builder.write_exposure(bond, Some(5)).unwrap();
    builder.write_registration_exposure(bond, Some(11)).unwrap();
    builder.write_payout(h64(1), Some(payout)).unwrap();
    builder.write_payout(h64(1), Some(payout)).unwrap();
    builder.write_model_position(position, 9).unwrap();
    builder.write_model_position(position, 9).unwrap();
    builder.write_model_position(position, 0).unwrap();
    builder.write_model_position(position, 0).unwrap();
    builder.write_payout(h64(9), None).unwrap();

    let expected = vec![
        PalwDeltaEntryV2::Exposure { key: bond, old: None, new: Some(5) },
        PalwDeltaEntryV2::Exposure { key: bond, old: Some(5), new: Some(5) },
        PalwDeltaEntryV2::RegistrationExposure { key: bond, old: None, new: Some(11) },
        PalwDeltaEntryV2::Payout { key: h64(1), old: None, new: Some(payout) },
        PalwDeltaEntryV2::Payout { key: h64(1), old: Some(payout), new: Some(payout) },
        PalwDeltaEntryV2::ModelPosition { key: position, old: None, new: Some(9) },
        PalwDeltaEntryV2::ModelPosition { key: position, old: Some(9), new: None },
        PalwDeltaEntryV2::Payout { key: h64(9), old: None, new: None },
    ];
    assert_eq!(borsh::to_vec(&builder.entries).unwrap(), borsh::to_vec(&expected).unwrap());
    let delta = PalwStateDeltaV2 { point: ctx(1, 1, 1), entries: builder.entries.clone() };
    let replayed = apply_delta_v2(&parent, &delta, &p).unwrap();
    // Replay rebuilds derived census rows this writer never touches. Compare the
    // journaled tables, then the same tables after the delta is reversed.
    assert_eq!(replayed.reserved_exposure, builder.state.reserved_exposure);
    assert_eq!(replayed.registration_exposure, builder.state.registration_exposure);
    assert_eq!(replayed.pending_payouts, builder.state.pending_payouts);
    assert_eq!(replayed.model_positions, builder.state.model_positions);
    let reverted = revert_delta_v2(&replayed, &delta, &p).unwrap();
    assert_eq!(reverted.reserved_exposure, parent.reserved_exposure);
    assert_eq!(reverted.registration_exposure, parent.registration_exposure);
    assert_eq!(reverted.pending_payouts, parent.pending_payouts);
    assert_eq!(reverted.model_positions, parent.model_positions);
}

#[test]
fn scoped_heap_row_noop_and_removal_admit_before_mutation() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let demands = BTreeMap::from([(bond_key(1), 50)]);
    let mut parent = PalwChainStateV2::genesis();
    parent.held_leaf_demands.insert(h64(1), demands.clone());
    parent.pending_payouts.insert(h64(2), PalwPayoutV2 { payload: h64(3), amount: 7 });
    let mut builder = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 });
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);

    // Equality does not bypass the historical quote for the replacement clone.
    assert!(matches!(
        builder.write_held_leaf_demands(h64(1), Some(demands)),
        Err(PalwStateV2Error::NativeResource(_))
    ));
    // Removing an existing row still needs its delta slot before the removal.
    assert!(matches!(builder.write_payout(h64(2), None), Err(PalwStateV2Error::NativeResource(_))));
    assert_eq!(builder.state, parent);
    assert!(builder.entries.is_empty());
    assert_eq!(checker.outstanding(), (0, 0));
}

#[test]
fn scoped_heap_row_owner_survives_scope_and_checkpoint_rollback() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let parent = PalwChainStateV2::genesis();
    let mut builder = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
    builder.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
    let checkpoint = builder.try_checkpoint().unwrap();
    builder.write_held_leaf_demands(h64(1), Some(BTreeMap::from([(bond_key(1), 50)]))).unwrap();
    let retained = checker.outstanding();
    assert!(retained.0 > 0);
    assert!(builder.state.held_leaf_demands.contains_key(&h64(1)));

    builder.restore(checkpoint);
    assert_eq!(builder.state, parent);
    assert!(builder.entries.is_empty());
    assert_eq!(checker.outstanding(), retained, "rollback does not refund the construction owner's admitted work");
    drop(builder);
    assert_eq!(checker.outstanding(), (0, 0));
}
