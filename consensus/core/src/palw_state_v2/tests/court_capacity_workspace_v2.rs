//! Stored scheduling-book fixture over a natively folded registry/claim. The
//! explicit placeholder mirror is not a production policy adoption/campaign.
use super::*;
use crate::palw_court_capacity_v1::*;
use crate::palw_legacy_public_filer_v1::{PalwDisputeClaimV1, PalwDisputeReservationRowV1};
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
use crate::palw_state_v2::palw_court_capacity_fold_v1::sweep_court_capacity_v1;
use std::sync::Arc;

fn source() -> (PalwChainStateV2, PalwStateParamsV2, Hash64) {
    let base = params().with_turn_deadline_daa(2).unwrap();
    let mut objects = register_class_and_bond();
    for n in 2..=4 {
        objects.push(bond_registration_with_collateral(n, n as u8, 20 + n, 1000));
    }
    let (s, _) = apply(&PalwChainStateV2::genesis(), &base, &ctx(1, 70, 1), &objects, None);
    let env = attempt(40, 1);
    let claim = attempt_id_v2(&env.attempt);
    let (mut s, _) = apply(&s, &base, &ctx(2, 71, 2), &[], Some(&env));
    let mut book = PalwDisputeClaimV1 { opened_daa: 80, hard_deadline_daa: 10_000, court_reserve_used: true, ..Default::default() };
    for n in 1..=4 {
        book.live.insert(bond_key(n), PalwDisputeReservationRowV1 { reserved_daa: 80, ..Default::default() });
    }
    let row = |stage, reserve, guaranteed| PalwReservedCourtV1 {
        stage,
        escrow: 100,
        rho_sompi: 1,
        weight: 1,
        service_daa: 10,
        reserve,
        guaranteed,
        admitted_daa: 80,
    };
    book.courts.insert(bond_key(1), row(PalwCourtStageV1::Ticket { granted_daa: 90, expires_daa: 95 }, false, true));
    book.courts.insert(bond_key(2), row(PalwCourtStageV1::Queued { enqueued_daa: 82 }, false, true));
    // An older best-effort entry still follows the guaranteed spare entry.
    book.courts.insert(bond_key(3), row(PalwCourtStageV1::Queued { enqueued_daa: 81 }, false, false));
    book.courts.insert(bond_key(4), row(PalwCourtStageV1::Queued { enqueued_daa: 83 }, true, true));
    s.legacy_disputes.insert(claim, book);
    let policy = palw_court_capacity_placeholder_policy_v1();
    assert_eq!(policy.spare_slots(base.turn_deadline_daa()), 1);
    assert_eq!(policy.reserve_slots(base.turn_deadline_daa()), 1);
    let p = base.with_court_capacity_v1(Some(PalwCourtCapacityMirrorV1 { from_daa: 0, policy }));
    (s, p, claim)
}
fn checker(bytes: u64) -> Arc<NativeDemandCheckerV1> {
    NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: bytes, database_bytes: 0 })
}
fn builder<'a>(
    s: &PalwChainStateV2,
    p: &'a PalwStateParamsV2,
    e: &'a PalwTransitionExtrasV1,
    c: &Arc<NativeDemandCheckerV1>,
) -> TransitionBuilder<'a> {
    let mut b = TransitionBuilder::new(s, p, false, false, false, false, e);
    b.native_reservation = NativeTransitionReservationV1::for_checker_v1(c);
    b
}
#[test]
fn court_capacity_sweep_workspace_refuses_before_occupancy_payment_and_retries_ticket_grant() {
    let (s, p, claim) = source();
    let e = PalwTransitionExtrasV1::default();
    let at = ctx(3, 100, 3);
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_court_capacity_sweep_workspace_v2().unwrap();
    let q = c.outstanding().0;
    assert!(q > 0);
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    for cap in [0, q - 1] {
        let c = checker(cap);
        let mut b = builder(&s, &p, &e, &c);
        assert!(matches!(sweep_court_capacity_v1(&mut b, &at), Err(PalwStateV2Error::NativeResource(_))));
        assert_eq!(b.state, s);
        assert_eq!(b.state.state_root(), s.state_root());
        assert!(b.entries.is_empty());
        assert_eq!(c.outstanding(), (0, 0));
    }
    let c = checker(q);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_court_capacity_sweep_workspace_v2().unwrap();
    assert_eq!(c.outstanding(), (q, 0));
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    sweep_court_capacity_v1(&mut b, &at).unwrap();
    let rows = &b.state.legacy_disputes[&claim].courts;
    assert!(!rows.contains_key(&bond_key(1)));
    assert!(matches!(rows[&bond_key(2)].stage, PalwCourtStageV1::Ticket { granted_daa: 100, .. }));
    assert_eq!(b.state.bond(&bond_key(1)).unwrap().collateral, s.bond(&bond_key(1)).unwrap().collateral - 5);
    assert!(c.outstanding().0 >= q);
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
}
#[test]
fn court_capacity_sweep_actual_pool_priority_and_flags_off_journal_replay_match() {
    let (s, p, claim) = source();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(3, 100, 3);
    let mut raw = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
    sweep_court_capacity_v1(&mut raw, &point).unwrap();
    let c = checker(u64::MAX);
    let mut owned = builder(&s, &p, &e, &c);
    sweep_court_capacity_v1(&mut owned, &point).unwrap();
    assert_eq!(raw.state, owned.state);
    assert_eq!(raw.state.state_root(), owned.state.state_root());
    assert_eq!(borsh::to_vec(&raw.entries).unwrap(), borsh::to_vec(&owned.entries).unwrap());
    let rows = &owned.state.legacy_disputes[&claim].courts;
    assert!(matches!(rows[&bond_key(4)].stage, PalwCourtStageV1::Ticket { .. }));
    assert!(matches!(rows[&bond_key(3)].stage, PalwCourtStageV1::Queued { .. }), "best effort cannot bypass guaranteed spare row");
    assert_eq!(rows[&bond_key(2)].stage, PalwCourtStageV1::Ticket { granted_daa: 100, expires_daa: 102 });
    let delta = PalwStateDeltaV2 { point, entries: owned.entries.clone() };
    assert_eq!(apply_delta_v2(&s, &delta, &p).unwrap().state_root(), owned.state.state_root());
    assert_eq!(revert_delta_v2(&owned.state, &delta, &p).unwrap().state_root(), s.state_root());
    drop(owned);
    assert_eq!(c.outstanding(), (0, 0));
}
#[test]
fn court_capacity_sweep_dormant_fence_has_no_new_allocation_or_payment() {
    let (s, p, _) = source();
    let p = p.with_court_capacity_v1(None);
    let e = PalwTransitionExtrasV1::default();
    let c = checker(0);
    let mut b = builder(&s, &p, &e, &c);
    sweep_court_capacity_v1(&mut b, &ctx(3, 100, 3)).unwrap();
    assert_eq!(b.state, s);
    assert!(b.entries.is_empty());
    assert_eq!(c.outstanding(), (0, 0));
}
