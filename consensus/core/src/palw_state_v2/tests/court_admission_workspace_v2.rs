//! Stored capacity-book mechanism over a real folded registry and attempt.
//! The capacity/fee values below are explicit private fixtures, not policy approval.
use super::*;
use crate::palw_court_capacity_v1::*;
use crate::palw_court_paid_ingress_v1::{PalwCourtIngressScheduleV1, set_palw_court_ingress_test_schedule_v1};
use crate::palw_legacy_public_filer_v1::{PalwDisputeClaimV1, PalwDisputeReservationRowV1};
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
use crate::palw_state_v2::palw_court_capacity_fold_v1::{
    PalwCourtAdmissionV1, court_admission_v1, court_ingress_admission_v1, court_opened_v1,
};
use std::sync::Arc;

struct Schedule;
impl Schedule {
    fn install() -> Self {
        set_palw_court_ingress_test_schedule_v1(Some(PalwCourtIngressScheduleV1 { fee_sompi_per_session: 5 }));
        Self
    }
}
impl Drop for Schedule {
    fn drop(&mut self) {
        set_palw_court_ingress_test_schedule_v1(None);
    }
}
fn source() -> (PalwChainStateV2, PalwStateParamsV2, Hash64) {
    let base = params().with_turn_deadline_daa(2).unwrap();
    let mut objects = register_class_and_bond();
    for n in 2..=4 {
        objects.push(bond_registration_with_collateral(n, n as u8, 20 + n, 1000));
    }
    let (s, _) = apply(&PalwChainStateV2::genesis(), &base, &ctx(1, 70, 1), &objects, None);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let (mut s, _) = apply(&s, &base, &ctx(2, 71, 2), &[], Some(&env));
    let mut book = PalwDisputeClaimV1 { opened_daa: 80, hard_deadline_daa: 10_000, court_reserve_used: true, ..Default::default() };
    for n in 1..=4 {
        book.live.insert(bond_key(n), PalwDisputeReservationRowV1 { reserved_daa: 80, ..Default::default() });
    }
    let row = |stage, guaranteed| PalwReservedCourtV1 {
        stage,
        escrow: 100,
        rho_sompi: 1,
        weight: 1,
        service_daa: 10,
        reserve: false,
        guaranteed,
        admitted_daa: 80,
    };
    // Occupied spare pool, with both queue classes: admission must invoke the
    // actual guaranteed scheduler rather than taking its empty-pool fast path.
    book.courts.insert(bond_key(1), row(PalwCourtStageV1::Ticket { granted_daa: 90, expires_daa: 102 }, true));
    book.courts.insert(bond_key(3), row(PalwCourtStageV1::Queued { enqueued_daa: 82 }, true));
    book.courts.insert(bond_key(4), row(PalwCourtStageV1::Queued { enqueued_daa: 81 }, false));
    s.legacy_disputes.insert(id, book);
    let policy = palw_court_capacity_placeholder_policy_v1();
    assert_eq!(policy.spare_slots(base.turn_deadline_daa()), 1);
    let p = base
        .with_court_capacity_v1(Some(PalwCourtCapacityMirrorV1 { from_daa: 0, policy }))
        .with_court_outsider_lane_from_daa(Some(0));
    (s, p, id)
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
fn court_admission_workspace_refuses_before_queue_or_ingress_fee_and_retries() {
    let _schedule = Schedule::install();
    let (s, p, claim) = source();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(3, 100, 3);
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_court_admission_workspace_v2(claim, bond_key(2), 100).unwrap();
    let q = c.outstanding().0;
    assert!(q > 0);
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    for bytes in [0, q - 1] {
        let c = checker(bytes);
        let mut b = builder(&s, &p, &e, &c);
        assert!(matches!(
            court_ingress_admission_v1(&mut b, &point, claim, bond_key(2), false, 10),
            Err(PalwStateV2Error::NativeResource(_))
        ));
        assert_eq!(b.state, s);
        assert_eq!(b.state.state_root(), s.state_root());
        assert!(b.entries.is_empty());
        assert_eq!(c.outstanding(), (0, 0));
    }
    let c = checker(q);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_court_admission_workspace_v2(claim, bond_key(2), 100).unwrap();
    assert_eq!(c.outstanding(), (q, 0), "Q is caller scratch, not writer/journal cost");
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    assert_eq!(court_ingress_admission_v1(&mut b, &point, claim, bond_key(2), false, 10).unwrap(), PalwCourtAdmissionV1::Queued);
    let row = b.state.legacy_disputes[&claim].courts[&bond_key(2)];
    assert!(matches!(row.stage, PalwCourtStageV1::Queued { enqueued_daa: 100 }));
    assert!(row.guaranteed);
    assert_eq!(b.state.bond(&bond_key(2)).unwrap().collateral, s.bond(&bond_key(2)).unwrap().collateral - 5);
    let paid = b.state.clone();
    let journal = borsh::to_vec(&b.entries).unwrap();
    assert!(court_ingress_admission_v1(&mut b, &point, claim, bond_key(2), false, 10).is_err());
    assert_eq!(b.state, paid, "a queued retry cannot charge twice");
    assert_eq!(borsh::to_vec(&b.entries).unwrap(), journal);
    assert!(c.outstanding().0 >= q);
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
}

#[test]
fn court_admission_workspace_preserves_active_flags_off_queue_fee_and_delta_bytes() {
    let _schedule = Schedule::install();
    let (s, p, claim) = source();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(3, 100, 3);
    let mut raw = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
    let expected = court_ingress_admission_v1(&mut raw, &point, claim, bond_key(2), false, 10).unwrap();
    let c = checker(u64::MAX);
    let mut owned = builder(&s, &p, &e, &c);
    assert_eq!(court_ingress_admission_v1(&mut owned, &point, claim, bond_key(2), false, 10).unwrap(), expected);
    assert_eq!(owned.state, raw.state);
    assert_eq!(owned.state.state_root(), raw.state.state_root());
    assert_eq!(borsh::to_vec(&owned.entries).unwrap(), borsh::to_vec(&raw.entries).unwrap());
    let delta = PalwStateDeltaV2 { point, entries: owned.entries.clone() };
    assert_eq!(apply_delta_v2(&s, &delta, &p).unwrap().state_root(), owned.state.state_root());
    assert_eq!(revert_delta_v2(&owned.state, &delta, &p).unwrap().state_root(), s.state_root());
    drop(owned);
    assert_eq!(c.outstanding(), (0, 0));
}

#[test]
fn court_admission_opened_row_handoff_is_admitted_before_clone_and_dormant_is_free() {
    let _schedule = Schedule::install();
    let (mut s, p, claim) = source();
    s.legacy_disputes.get_mut(&claim).unwrap().courts.clear();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(3, 100, 3);
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    assert_eq!(court_ingress_admission_v1(&mut b, &point, claim, bond_key(2), false, 10).unwrap(), PalwCourtAdmissionV1::Open);
    let admitted = b.state.clone();
    let c0 = checker(0);
    let mut refused = builder(&admitted, &p, &e, &c0);
    assert!(matches!(court_opened_v1(&mut refused, &point, claim, bond_key(2), h64(900)), Err(PalwStateV2Error::NativeResource(_))));
    assert_eq!(refused.state, admitted);
    assert!(refused.entries.is_empty());
    court_opened_v1(&mut b, &point, claim, bond_key(2), h64(900)).unwrap();
    assert!(matches!(b.state.legacy_disputes[&claim].courts[&bond_key(2)].stage,
        PalwCourtStageV1::Running { session, started_daa: 100 } if session == h64(900)));
    assert_eq!(b.state.bond(&bond_key(2)), admitted.bond(&bond_key(2)), "handoff does not charge again");
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    let dormant = p.with_court_capacity_v1(None);
    let mut off = builder(&s, &dormant, &e, &c0);
    assert_eq!(court_admission_v1(&mut off, &point, claim, bond_key(2), 10).unwrap(), PalwCourtAdmissionV1::Legacy);
    court_opened_v1(&mut off, &point, claim, bond_key(2), h64(900)).unwrap();
    assert_eq!(off.state, s);
    assert!(off.entries.is_empty());
    assert_eq!(c0.outstanding(), (0, 0));
}
