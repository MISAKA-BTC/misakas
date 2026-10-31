//! Actual admitted court/disclosure → timeout → native rearm consumer.
#![allow(warnings)]
use super::*;
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
use std::sync::Arc;

fn ready() -> (PalwChainStateV2, PalwStateParamsV2, Hash64, Hash64) {
    let p = params_with_ladder();
    let (s, claim, sid) = licensed_with_court(&p);
    // A genuine folded disclosure owns a nonempty transcript. Empty initial
    // ladders may legitimately have zero heap and are not fake quota controls.
    let (s, _) = apply(&s, &p, &ctx(6, 105, 6), &[disclose(sid, 0, 8, 0xD0)], None);
    assert_eq!(s.court_session(&sid).unwrap().ladder.turn(), crate::palw_bisect::PalwBisectTurnV1::AwaitVerdict);
    (s, p, claim, sid)
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
fn actual_court_timeout_capacity_refuses_before_copies_or_verdict_and_retry_keeps_original_outcome() {
    let (s, p, claim, sid) = ready();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(7, 126, 7);
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_court_deadline_workspace_v2(point.daa_score).unwrap();
    let q = c.outstanding().0;
    assert!(q > 0, "real disclosure transcript, no invented charge");
    assert!(b.entries.is_empty());
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    for cap in [0, q - 1] {
        let c = checker(cap);
        let mut b = builder(&s, &p, &e, &c);
        assert!(matches!(sweep_court_deadlines(&mut b, &point), Err(PalwStateV2Error::NativeResource(_))));
        assert_eq!(b.state, s);
        assert_eq!(b.state.state_root(), s.state_root());
        assert!(b.entries.is_empty());
        assert_eq!(c.outstanding(), (0, 0));
        assert!(b.state.court_session(&sid).is_some());
    }
    let c = checker(q);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_court_deadline_workspace_v2(point.daa_score).unwrap();
    assert_eq!(c.outstanding(), (q, 0));
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    let c = checker(u64::MAX);
    let mut retry = builder(&s, &p, &e, &c);
    sweep_court_deadlines(&mut retry, &point).unwrap();
    assert!(retry.state.court_session(&sid).is_none());
    assert!(
        matches!(retry.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "challenger silence does not convict responder"
    );
    assert!(!retry.entries.is_empty());
    assert!(c.outstanding().0 >= q);
    drop(retry);
    assert_eq!(c.outstanding(), (0, 0));
}
#[test]
fn actual_court_timeout_flags_off_parity_and_not_due_boundary_preserve_state_and_journal() {
    let (s, p, _, sid) = ready();
    let e = PalwTransitionExtrasV1::default();
    let due = s.court_session(&sid).unwrap().ladder.last_deadline_daa();
    let c = checker(0);
    let mut b = builder(&s, &p, &e, &c);
    sweep_court_deadlines(&mut b, &ctx(7, due, 7)).unwrap();
    assert_eq!(b.state, s);
    assert!(b.entries.is_empty());
    assert_eq!(c.outstanding(), (0, 0));
    drop(b);
    let point = ctx(7, due + 1, 7);
    let mut raw = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
    sweep_court_deadlines(&mut raw, &point).unwrap();
    let c = checker(u64::MAX);
    let mut owned = builder(&s, &p, &e, &c);
    sweep_court_deadlines(&mut owned, &point).unwrap();
    assert_eq!(raw.state, owned.state);
    assert_eq!(raw.state.state_root(), owned.state.state_root());
    assert_eq!(borsh::to_vec(&raw.entries).unwrap(), borsh::to_vec(&owned.entries).unwrap());
    // The real journal is the state change the fold would publish; replay/reorg
    // use the original point, rather than creating a second timeout verdict.
    let delta = PalwStateDeltaV2 { point, entries: owned.entries.clone() };
    assert_eq!(apply_delta_v2(&s, &delta, &p).unwrap().state_root(), owned.state.state_root());
    assert_eq!(revert_delta_v2(&owned.state, &delta, &p).unwrap().state_root(), s.state_root());
    drop(owned);
    assert_eq!(c.outstanding(), (0, 0));
}
