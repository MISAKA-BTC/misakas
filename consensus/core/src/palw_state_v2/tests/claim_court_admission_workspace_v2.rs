//! Actual attempt/rider calls and funded-original admission phase. Stored escrow
//! rows exercise membership, not a new funded-original authorization campaign.
#![allow(warnings)]
use super::*;
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
use crate::palw_state_v2::palw_court_capacity_fold_v1 as court;
use std::{cell::Cell, rc::Rc, sync::Arc};

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
fn escrow(s: &mut PalwChainStateV2, contract: u64, claim: Hash64, open: bool) {
    let row = crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1::fresh_v1(
        h64(contract),
        claim,
        stored_outpoint(contract),
        1000,
        h64(3),
        h64(4),
        71,
        10100,
        open,
    )
    .unwrap();
    s.funded_escrows.insert(h64(contract), row);
}
fn stored_outpoint(n: u64) -> crate::tx::TransactionOutpoint {
    crate::tx::TransactionOutpoint::new(h64(n + 10000), 0)
}
fn source() -> (PalwChainStateV2, PalwStateParamsV2, Hash64) {
    let base = params().with_worker_carve_permille(620).unwrap();
    let (mut s, _) = apply(&PalwChainStateV2::genesis(), &base, &ctx(1, 70, 1), &register_class_and_bond(), None);
    // Same native class-record fact used by existing fused-court fixtures; no
    // court/session has yet been opened, so deadline indices are unchanged.
    s.classes.get_mut(&h64(1)).unwrap().fused_attention = true;
    let lead = attempt(40, 1);
    let id = attempt_id_v2(&lead.attempt);
    let (mut s, _) = apply(&s, &base, &PalwBlockContextV2 { subsidy: 1000, ..ctx(2, 71, 2) }, &[], Some(&lead));
    escrow(&mut s, 2001, id, true);
    escrow(&mut s, 2002, id, true); // F=2, D=1 (same claim)
    escrow(&mut s, 2003, id, false);
    let policy = crate::palw_court_capacity_v1::palw_court_capacity_placeholder_policy_v1();
    let p = base.with_court_capacity_v1(Some(crate::palw_court_capacity_v1::PalwCourtCapacityMirrorV1 { from_daa: 0, policy }));
    (s, p, id)
}
fn origin(env: &PalwAttemptEnvelopeV2) -> PalwAttemptOriginV1 {
    PalwAttemptOriginV1 {
        rider: false,
        carrying_block: ctx(3, 80, 3).block,
        escrows_reward: true,
        escrow_subsidy: 0,
        escrow_carve: None,
        execution_key: crate::palw_attempt_v2::execution_commitment_v3(&env.attempt, h64(5)),
        carrying_bits: 0,
        job_anchor: h64(5),
        floor_event: PalwFloorEventSourceV1::BlockBlue,
        budget_block_units: crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1,
    }
}
fn membership_q(s: &PalwChainStateV2, p: &PalwStateParamsV2, e: &PalwTransitionExtrasV1, claim: Hash64) -> u64 {
    let c = checker(u64::MAX);
    let mut b = builder(s, p, e, &c);
    b.check_funded_original_court_admission_native_v2(claim, 80).unwrap();
    let q = c.outstanding().0;
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    q
}

#[test]
fn claim_court_membership_real_attempt_and_funded_phase_refuse_before_write_and_retry() {
    let (s, p, claim) = source();
    let e = PalwTransitionExtrasV1::default();
    let q = membership_q(&s, &p, &e, claim);
    assert!(q > 0);
    let env = attempt(40, 2);
    for cap in [0, q - 1] {
        for funded in [false, true] {
            let c = checker(cap);
            let mut b = builder(&s, &p, &e, &c);
            let result = if funded {
                b.check_funded_original_court_admission_native_v2(claim, 80)
            } else {
                apply_attempt(&mut b, &ctx(3, 80, 3), &env, origin(&env))
            };
            assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))));
            assert_eq!(b.state, s);
            assert_eq!(b.state.state_root(), s.state_root());
            assert!(b.entries.is_empty());
            assert_eq!(c.outstanding(), (0, 0));
        }
    }
    let c = checker(q);
    let mut b = builder(&s, &p, &e, &c);
    b.check_funded_original_court_admission_native_v2(claim, 80).unwrap();
    assert_eq!(c.outstanding(), (q, 0)); // Set gone, construction owner alive
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    apply_attempt(&mut b, &ctx(3, 80, 3), &env, origin(&env)).unwrap();
    assert!(b.state.claims.contains_key(&attempt_id_v2(&env.attempt)));
    assert!(c.outstanding().0 >= q);
    let mut raw = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
    apply_attempt(&mut raw, &ctx(3, 80, 3), &env, origin(&env)).unwrap();
    assert_eq!(b.state, raw.state);
    assert_eq!(b.state.state_root(), raw.state.state_root());
    assert_eq!(borsh::to_vec(&b.entries).unwrap(), borsh::to_vec(&raw.entries).unwrap());
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
}

#[test]
fn claim_court_count_matches_collector_duplicates_exclusion_deadline_and_dormancy() {
    let (mut s, p, claim) = source();
    let e = PalwTransitionExtrasV1::default();
    let q = membership_q(&s, &p, &e, claim);
    assert_eq!(court::palw_funded_court_reserve_claims_v1(&s).len(), 1);
    s.funded_escrows.remove(&h64(2002));
    assert_eq!(membership_q(&s, &p, &e, claim) * 2, q, "F bounds streamed D memberships");
    let c = checker(0);
    let dormant = params();
    let mut b = builder(&s, &dormant, &e, &c);
    b.check_funded_original_court_admission_native_v2(claim, 80).unwrap();
    assert_eq!(c.outstanding(), (0, 0)); // dormant, no Set
    drop(b);
    s.funded_escrows.clear();
    assert_eq!(membership_q(&s, &p, &e, claim), 0, "F=0 allocates no Set");
    // Independently preserve the old collecting/sorting count predicate. The
    // public reserved collector still sorts; this claim-only check needs count.
    use crate::palw_court_paid_ingress_v1::{PalwCourtIngressScheduleV1, set_palw_court_ingress_test_schedule_v1};
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            set_palw_court_ingress_test_schedule_v1(None);
        }
    }
    let _reset = Reset;
    set_palw_court_ingress_test_schedule_v1(Some(PalwCourtIngressScheduleV1 { fee_sompi_per_session: 0 }));
    let p = p.with_court_outsider_lane_from_daa(Some(0));
    let cap = crate::palw_court_paid_ingress_v1::palw_court_ingress_reserved_claims_cap_v1(palw_max_concurrent_court_sessions_v1(
        p.turn_deadline_daa(),
    ));
    // Stored-row count projections, not fresh authenticated claim admissions:
    // hit the exact cap and its exclusion boundary, then phase/deadline edges.
    let template = s.claims[&claim].clone();
    for i in 1..cap {
        s.claims.insert(h64(3000 + i), template.clone());
    }
    let hard = crate::palw_court_clock_v2::palw_claim_court_reserve_hard_v2(&p, &template, palw_da_disclose_window_daa_v1(&p));
    for phase in [
        PalwClaimPhaseV2::Provisional,
        PalwClaimPhaseV2::PanelBound { bound_daa: 72 },
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 73 },
        PalwClaimPhaseV2::Final { final_daa: 80 },
        PalwClaimPhaseV2::Voided { voided_daa: 80, reason: PalwVoidReasonV2::BindTimeout },
    ] {
        s.claims.get_mut(&claim).unwrap().phase = phase;
        for now in [0, 71, 80, hard, hard.saturating_add(1)] {
            for exclude in [None, Some(claim), Some(h64(999))] {
                let mut old: Vec<_> = s
                    .claims
                    .iter()
                    .filter(|(_, row)| {
                        row.accepted_daa >= p.court_outsider_lane_from_daa().unwrap()
                            && crate::palw_legacy_public_filer_v1::palw_dispute_phase_reservable_v1(&row.phase, false)
                            && crate::palw_court_clock_v2::palw_claim_court_reserve_hard_v2(
                                &p,
                                row,
                                palw_da_disclose_window_daa_v1(&p),
                            ) >= now
                    })
                    .map(|(id, row)| (row.accepted_daa, *id))
                    .collect();
                old.sort_unstable();
                let old = old.into_iter().filter(|(_, id)| Some(*id) != exclude).count() as u64;
                let result = court::court_ingress_claim_admits_excluding_v1(&s, &p, &h64(1), now, exclude);
                assert_eq!(result.is_ok(), old < cap);
                if let Err(PalwStateV2Error::CourtClaimAdmissionFull { admitted, .. }) = result {
                    assert_eq!(admitted, old);
                }
            }
        }
    }
}

struct Observation;
impl Drop for Observation {
    fn drop(&mut self) {
        court::observe_claim_membership_admission_v2(None);
    }
}
#[test]
fn claim_court_actual_rider_preserves_typed_local_failure_after_prior_writes() {
    let (s, mut p, claim) = source();
    assert!(s.claims[&claim].escrowed_reward > 0, "the actual lead fold must create a shareable carve");
    p.capacity_riders_from_daa = Some(0);
    let e = PalwTransitionExtrasV1::default();
    let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(1000).unwrap();
    let mut rider = attempt(40, 8);
    rider.attempt.challenge = crate::palw_capacity_s567_v1::palw_rider_challenge_v1(&claim, 0);
    rider.attempt.trace_chunk_count = crate::palw_attempt_v2::PALW_ATTEMPT_V2_TRACE_CHUNKS;
    rider.attempt.trace_manifest_root =
        crate::palw_attempt_v2::attempt_trace_manifest_root_v1(rider.attempt.trace_root, rider.attempt.trace_chunk_count);
    rider.attempt.trace_retention_daa = s.claims[&claim].trace_retention_daa;
    let q = membership_q(&s, &p, &e, claim);
    let before = Rc::new(Cell::new(None));
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    let observed = before.clone();
    let observed_c = c.clone();
    let _observation = Observation;
    court::observe_claim_membership_admission_v2(Some(Box::new(move || observed.set(Some(observed_c.outstanding().0)))));
    b.pending_riders.push((claim, vec![rider.clone()]));
    let mut skips = Vec::new();
    apply_pending_riders_v1(&mut b, &ctx(3, 72, 3), &p, Some(&admission), &mut skips).unwrap();
    assert!(skips.is_empty(), "baseline must pass actual rider admission: {skips:?}");
    let prior = before.get().expect("real rider reached funded Set admission");
    assert!(prior > 0, "lead writes precede this phase");
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    before.set(None);
    let c = checker(prior + q - 1);
    let mut b = builder(&s, &p, &e, &c);
    let observed = before.clone();
    let observed_c = c.clone();
    court::observe_claim_membership_admission_v2(Some(Box::new(move || observed.set(Some(observed_c.outstanding().0)))));
    let rider_id = attempt_id_v2(&rider.attempt);
    b.pending_riders.push((claim, vec![rider]));
    let mut skips = Vec::new();
    let result = apply_pending_riders_v1(&mut b, &ctx(3, 72, 3), &p, Some(&admission), &mut skips);
    assert!(skips.is_empty(), "local refusal never becomes a semantic skipped rider");
    assert_eq!(before.get(), Some(prior), "not an earlier writer refusal");
    assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))), "no CapacityRiders string/semantic skip");
    assert!(!b.state.claims.contains_key(&rider_id));
    // The block fold propagates this typed error and publishes no output.
    // Dropping failed construction cannot publish its partially written lead.
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    let context = PalwStateV2Error::NativeContextUnavailable(
        crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
    );
    assert!(matches!(rider_attempt_refusal_v2(0, context), PalwStateV2Error::NativeContextUnavailable(_)));
    assert!(matches!(rider_attempt_refusal_v2(2, PalwStateV2Error::MissingClaim(claim)), PalwStateV2Error::CapacityRiders(_)));
}
