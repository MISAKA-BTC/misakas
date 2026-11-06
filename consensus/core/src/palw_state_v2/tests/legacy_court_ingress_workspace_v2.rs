//! Real Legacy ingress over native registry/claim rows and a stored running
//! session. Private fee injection is not production policy or court acceptance.
#![allow(warnings)]
use super::*;
use crate::palw_court_paid_ingress_v1::{PalwCourtIngressScheduleV1, set_palw_court_ingress_test_schedule_v1};
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
use crate::palw_state_v2::palw_court_capacity_fold_v1::{
    PalwCourtAdmissionV1, court_ingress_admission_v1, court_ingress_reserved_claims_v1, court_outsider_lane_is_followon_v1,
};
use std::sync::Arc;

struct Schedule;
impl Schedule {
    fn set(fee: Option<u64>) -> Self {
        set_palw_court_ingress_test_schedule_v1(fee.map(|fee_sompi_per_session| PalwCourtIngressScheduleV1 { fee_sompi_per_session }));
        Self
    }
}
impl Drop for Schedule {
    fn drop(&mut self) {
        set_palw_court_ingress_test_schedule_v1(None);
    }
}
fn source() -> (PalwChainStateV2, PalwStateParamsV2, Hash64) {
    let base = params().with_turn_deadline_daa(8).unwrap();
    let mut objects = register_class_and_bond();
    for n in 2..=4 {
        let mut object = bond_registration_with_collateral(n, n as u8, 20 + n, 1000);
        if let PalwConsensusObjectV2::BondRegistered { payout_payload, .. } = &mut object {
            *payout_payload = h64(700 + n);
        }
        objects.push(object);
    }
    let (s, _) = apply(&PalwChainStateV2::genesis(), &base, &ctx(1, 70, 1), &objects, None);
    let env = attempt(40, 1);
    let claim = attempt_id_v2(&env.attempt);
    let (mut s, _) = apply(&s, &base, &ctx(2, 71, 2), &[], Some(&env));
    let ladder = crate::palw_bisect::PalwBisectLadderV1::open(
        &claim,
        &h64(900),
        &h64(901),
        &h64(902),
        crate::palw_bisect::PalwBisectSpaceV1::StepLeaves,
        8,
        80,
        88,
    )
    .unwrap();
    s.court_sessions.insert(
        h64(800),
        PalwCourtSessionStateV2 { claim, challenger_bond: bond_key(3), opened_daa: 80, deadline_daa: 500, ladder, dissection: None },
    );
    let p = base.with_court_outsider_lane_from_daa(Some(0));
    assert!(p.court_capacity_at_v1(80).is_none());
    assert!(court_ingress_reserved_claims_v1(&s, &p, 80).contains(&claim), "the actual inflight/reserved buffers must be nonempty");
    assert!(court_outsider_lane_is_followon_v1(&s, claim, bond_key(2)), "the actual first-side set must be consulted");
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
fn legacy_court_ingress_workspace_refuses_before_fee_or_journal_and_retries_paid_and_unpaid() {
    for fee in [Some(0), Some(5)] {
        let _schedule = Schedule::set(fee);
        let (s, p, claim) = source();
        let e = PalwTransitionExtrasV1::default();
        let point = ctx(3, 80, 3);
        let c = checker(u64::MAX);
        let mut b = builder(&s, &p, &e, &c);
        b.reserve_legacy_court_ingress_workspace_v2(bond_key(2), false, 80).unwrap();
        let q = c.outstanding().0;
        assert!(q > 0);
        drop(b);
        assert_eq!(c.outstanding(), (0, 0));
        for cap in [0, q - 1] {
            let c = checker(cap);
            let mut b = builder(&s, &p, &e, &c);
            assert!(matches!(
                court_ingress_admission_v1(&mut b, &point, claim, bond_key(2), false, 20),
                Err(PalwStateV2Error::NativeResource(_))
            ));
            assert_eq!(b.state, s);
            assert_eq!(b.state.state_root(), s.state_root());
            assert!(b.entries.is_empty());
            assert_eq!(c.outstanding(), (0, 0));
        }
        let c = checker(q);
        let mut b = builder(&s, &p, &e, &c);
        b.reserve_legacy_court_ingress_workspace_v2(bond_key(2), false, 80).unwrap();
        assert_eq!(c.outstanding(), (q, 0), "Q is caller scratch, not the fee's writer/journal budget");
        drop(b);
        let c = checker(u64::MAX);
        let mut b = builder(&s, &p, &e, &c);
        assert_eq!(court_ingress_admission_v1(&mut b, &point, claim, bond_key(2), false, 20).unwrap(), PalwCourtAdmissionV1::Legacy);
        assert_eq!(b.state.bonds[&bond_key(2)].collateral, s.bonds[&bond_key(2)].collateral - fee.unwrap_or(0));
        assert_eq!(b.state.bonds[&bond_key(2)].slashed, s.bonds[&bond_key(2)].slashed + fee.unwrap_or(0));
        assert!(c.outstanding().0 >= q);
        drop(b);
        assert_eq!(c.outstanding(), (0, 0));
    }
}

#[test]
fn legacy_court_ingress_workspace_preserves_flags_off_fee_bytes_root_and_delta_replay() {
    let _schedule = Schedule::set(Some(5));
    let (s, p, claim) = source();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(3, 80, 3);
    let mut raw = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
    let expected = court_ingress_admission_v1(&mut raw, &point, claim, bond_key(2), false, 20).unwrap();
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    assert_eq!(court_ingress_admission_v1(&mut b, &point, claim, bond_key(2), false, 20).unwrap(), expected);
    assert_eq!(b.state, raw.state);
    assert_eq!(b.state.state_root(), raw.state.state_root());
    assert_eq!(borsh::to_vec(&b.entries).unwrap(), borsh::to_vec(&raw.entries).unwrap());
    assert!(
        b.entries.iter().all(|entry| matches!(entry, PalwDeltaEntryV2::Bond { .. })),
        "Legacy admission does not create a capacity row"
    );
    let delta = PalwStateDeltaV2 { point, entries: b.entries.clone() };
    assert_eq!(apply_delta_v2(&s, &delta, &p).unwrap().state_root(), b.state.state_root());
    assert_eq!(revert_delta_v2(&b.state, &delta, &p).unwrap().state_root(), s.state_root());
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
}

fn old_inside(s: &PalwChainStateV2, claim: Hash64, bond: PalwBondKeyV2) -> bool {
    let mut side = Vec::new();
    if let Some(c) = s.claims.get(&claim) {
        side.push(c.bond);
    }
    if let Some(panel) = s.panels.get(&claim) {
        side.extend(panel.seats.iter().map(|seat| seat.bond));
    }
    if side.contains(&bond) {
        return true;
    }
    let Some(record) = s.bonds.get(&bond) else { return false };
    side.iter()
        .filter_map(|key| s.bonds.get(key))
        .any(|row| row.operator_id == record.operator_id || row.payout_payload == record.payout_payload)
}

#[test]
fn legacy_court_ingress_borrowed_side_matches_old_membership_and_zero_quota_dormant_direct_cap_rules() {
    let (mut s, p, claim) = source();
    // Duplicate seat and linked-operator/payout identities retain the exact old
    // classifier semantics. These stored rows are not a panel admission claim.
    s.panels.insert(
        claim,
        PalwPanelStateV2 {
            anchor: h64(910),
            seats: vec![
                PalwPanelSeatV2 { bond: bond_key(4), operator_id: op_id(24) },
                PalwPanelSeatV2 { bond: bond_key(4), operator_id: op_id(24) },
            ],
            bound_daa: 80,
        },
    );
    for (id, by_operator) in [(5, true), (6, false)] {
        let mut row = s.bonds[&bond_key(2)].clone();
        if by_operator {
            row.operator_id = s.bonds[&bond_key(1)].operator_id;
        } else {
            row.payout_payload = s.bonds[&bond_key(1)].payout_payload;
        }
        s.bonds.insert(bond_key(id), row);
    }
    for n in 1..=7 {
        let old = s
            .court_sessions
            .values()
            .any(|row| row.claim == claim && old_inside(&s, claim, row.challenger_bond) == old_inside(&s, claim, bond_key(n)));
        assert_eq!(court_outsider_lane_is_followon_v1(&s, claim, bond_key(n)), old);
    }
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(3, 80, 3);
    let c = checker(0);
    {
        let _schedule = Schedule::set(Some(5));
        let off = p.clone().with_court_outsider_lane_from_daa(None);
        let mut b = builder(&s, &off, &e, &c);
        assert_eq!(court_ingress_admission_v1(&mut b, &point, claim, bond_key(2), false, 20).unwrap(), PalwCourtAdmissionV1::Legacy);
        assert_eq!(b.state, s);
        assert!(b.entries.is_empty());
        let cap = palw_max_concurrent_court_sessions_v1(p.turn_deadline_daa());
        let template = s.court_sessions.values().next().unwrap().clone();
        for n in 0..cap {
            s.court_sessions.insert(h64(1000 + n), template.clone());
        }
        let mut full = builder(&s, &p, &e, &c);
        assert!(matches!(
            court_ingress_admission_v1(&mut full, &point, claim, bond_key(2), true, 20),
            Err(PalwStateV2Error::CourtSessionsAtCapacity { .. })
        ));
        assert_eq!(full.state, s);
        assert!(full.entries.is_empty());
    }
    // Direct evidence is exempt from the follow-on split, but not overall cap.
    let _schedule = Schedule::set(Some(0));
    s.court_sessions.clear();
    let c = checker(u64::MAX);
    let mut direct = builder(&s, &p, &e, &c);
    assert_eq!(court_ingress_admission_v1(&mut direct, &point, claim, bond_key(2), true, 20).unwrap(), PalwCourtAdmissionV1::Legacy);
    assert_eq!(direct.state, s);
    assert!(direct.entries.is_empty());
    drop(direct);
    assert_eq!(c.outstanding(), (0, 0));
}

#[test]
fn paid_ingress_preserves_actual_claim_principal_and_refuses_before_burn() {
    let _schedule = Schedule::set(Some(500));
    let (state, params, claim) = source();
    let extras = PalwTransitionExtrasV1::default();
    let point = ctx(3, 80, 3);
    // The producer's already folded attempt supplies the real outstanding duty;
    // this is not a fabricated reserved_exposure entry.
    let opener = bond_key(1);
    let probe = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let held = u64::try_from(probe.committed_at(&opener, point.daa_score).unwrap()).unwrap();
    assert!(held > 0);
    drop(probe);
    for free in [499, 500] {
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let keep = held.checked_add(free).unwrap();
        let collateral = b.state.bond(&opener).unwrap().collateral;
        assert!(collateral >= keep && keep >= 500);
        b.slash_bond(opener, u128::from(collateral - keep)).unwrap();
        assert_eq!(b.committed_at(&opener, point.daa_score).unwrap(), u128::from(held));
        let root = b.state.state_root();
        let journal = borsh::to_vec(&b.entries).unwrap();
        let result = court_ingress_admission_v1(&mut b, &point, claim, opener, false, 20);
        if free < 500 {
            let err = result.unwrap_err();
            assert!(format!("{err:?}").contains("free principal"), "{err:?}");
            assert_eq!(b.state.state_root(), root);
            assert_eq!(borsh::to_vec(&b.entries).unwrap(), journal);
        } else {
            assert_eq!(result.unwrap(), PalwCourtAdmissionV1::Legacy);
            assert_eq!(b.state.bond(&opener).unwrap().collateral, held);
            assert_eq!(b.committed_at(&opener, point.daa_score).unwrap(), u128::from(held));
        }
    }
}

#[test]
fn paid_ingress_preserves_separate_accuser_principal_and_native_object_refusal() {
    let _schedule = Schedule::set(Some(500));
    let (mut state, params, claim) = source();
    let params = params.with_rcore_plus_mirrors(Some(0), 0, Vec::new());
    // source() has a stored running court. Rebuild the derived index exactly
    // as carriage restore does, so A-6 reads the real session/claim principal.
    rebuild_deadline_free_indices(&mut state);
    let extras = PalwTransitionExtrasV1::default();
    let point = ctx(3, 80, 3);
    let opener = bond_key(3);
    let probe = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let committed = probe.committed_at(&opener, point.daa_score).unwrap();
    let accuser = probe.read().accuser_ledger_v1(&opener, point.daa_score);
    assert!(accuser > 0, "the running court must hold its challenger's principal in A-6");
    let held = u64::try_from(committed.checked_add(accuser).unwrap()).unwrap();
    drop(probe);
    for free in [499, 500] {
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let keep = held.checked_add(free).unwrap();
        let collateral = b.state.bond(&opener).unwrap().collateral;
        b.slash_bond(opener, u128::from(collateral - keep)).unwrap();
        let root = b.state.state_root();
        let journal = borsh::to_vec(&b.entries).unwrap();
        let result = court_ingress_admission_v1(&mut b, &point, claim, opener, false, 20);
        if free < 500 {
            assert!(format!("{:?}", result.unwrap_err()).contains("free principal"));
            assert_eq!(b.state.state_root(), root);
            assert_eq!(borsh::to_vec(&b.entries).unwrap(), journal);
            // The production object boundary discards its local builder on
            // failure. Its caller keeps exactly the original state and root.
            let base = b.state.clone();
            let object = court_open(claim, base.claim(&claim).unwrap().trace_root, bond_key(1), opener);
            let error = palw_v2_apply_one_object_v1(&base, &params, &point, &object, false, false, false, false, &extras).unwrap_err();
            assert!(format!("{error:?}").contains("free principal"), "{error:?}");
            assert_eq!(base.state_root(), root);
            assert_eq!(b.state, base);
        } else {
            assert_eq!(result.unwrap(), PalwCourtAdmissionV1::Legacy);
            assert_eq!(b.state.bond(&opener).unwrap().collateral, held);
            assert_eq!(b.read().accuser_ledger_v1(&opener, point.daa_score), accuser);
        }
    }
}

#[test]
fn fee_principal_view_counts_pre_rcore_classic_court_once() {
    let _schedule = Schedule::set(Some(0));
    let (mut state, params, claim) = source();
    rebuild_deadline_free_indices(&mut state);
    let extras = PalwTransitionExtrasV1::default();
    let point = ctx(3, 80, 3);
    let who = bond_key(2);
    let object = court_open(claim, state.claim(&claim).unwrap().trace_root, bond_key(1), who);
    let opened = palw_v2_apply_one_object_v1(&state, &params, &point, &object, false, false, false, false, &extras).unwrap();
    let b = TransitionBuilder::new(&opened, &params, false, false, false, false, &extras);
    let a1 = b.committed_at(&who, point.daa_score).unwrap();
    let a6 = b.read().accuser_ledger_v1(&who, point.daa_score);
    assert!(a1 > 0 && a6 > 0, "the actual classic opening is represented in both old views");
    assert_eq!(a1, a6);
    assert_eq!(
        super::super::palw_court_capacity_fold_v1::fee_reserved_principal_v1(&b, &who, point.daa_score).unwrap(),
        a1,
        "the fee view must not subtract the same classic principal twice before R-core+"
    );
}
