//! G14 r6 D4 (2026-10-22) — written, not compiled. Native post-Final duty clocks for text29 / funded v11: an accepted post-Final
//! accusation records RFC-0028-shaped clocks, the actor's answer is recorded, and the F02 default decides silence natively (no
//! injected silence fact): supplied + silent → refund + slash (test slash value), answered → no default, unsupplied → no-slash hold
//! and recovery by a re-opened turn, reorg restores everything.
use super::super::palw_native_liability_hold_fold_v1::*;
use super::super::palw_native_post_final_default_fold_v1::*;
use super::super::palw_native_post_final_duty_v1::*;
use super::*;
use crate::palw_court_service_binding_v1::{CourtServiceAttributionV1, CourtServiceProfileV1};
use crate::palw_court_service_v1::ServiceCost;
use crate::palw_native_liability_hold_v1::*;

const JOB: [u8; 64] = [0x6D; 64];
const HORIZON: u64 = 50_000;
const SLASH: u64 = 2_500;
const ACCUSED: u64 = 2_000;

fn producer() -> PalwBondKeyV2 {
    bond_key(5)
}
fn accuser() -> PalwBondKeyV2 {
    bond_key(6)
}
fn armed() -> PalwStateParamsV2 {
    params().with_settlement_once_from(Some(0)).with_native_liability_horizon_from(Some(100))
}
fn conviction_extras() -> PalwTransitionExtrasV1 {
    let mut e = PalwTransitionExtrasV1::default();
    e.tir_public_conviction_active = true;
    e
}
fn profile() -> CourtServiceProfileV1 {
    CourtServiceProfileV1 {
        version: 1,
        budget: ServiceCost { bytes: 100, work: 100, io: 10, controls: 10 },
        packet_cost: ServiceCost { bytes: 10, work: 10, io: 1, controls: 1 },
        packets_per_signer: 2,
        service_span_daa: 50,
    }
}
fn held(family: u8) -> PalwNativeHeldFeeRowV1 {
    PalwNativeHeldFeeRowV1 {
        version: PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1,
        family,
        job: JOB,
        claim: h64(0xC2A2).as_bytes(),
        producer: producer(),
        queue: h64(0xA1).as_bytes(),
        payload: h64(0xB1).as_bytes(),
        amount: 7_000,
        income_root: JOB,
        income_kind: 11,
        budget: h64(0xD1).as_bytes(),
        refund_payload: h64(0xB2).as_bytes(),
        final_daa: 1_500,
        horizon_daa: HORIZON,
    }
}
/// Producer and accuser bonds, the kernel route. No TIR service book: the native duty needs none.
fn parent() -> PalwChainStateV2 {
    let (mut state, _) = apply_palw_transition_v2(
        &PalwChainStateV2::genesis(),
        &params(),
        &ctx(1, 10, 1),
        &[bond_registration_with_collateral(5, 7, 21, 1_000_000), bond_registration_with_collateral(6, 8, 22, 1_000_000)],
        None,
    )
    .expect("the bonds register");
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    state
}
fn collateral(state: &PalwChainStateV2) -> u64 {
    state.bond(&producer()).unwrap().collateral
}
fn queued(state: &PalwChainStateV2, key: [u8; 64]) -> Option<(Hash64, u64)> {
    state.pending_payouts.get(&Hash64::from_bytes(key)).map(|p| (p.payload, p.amount))
}
/// Held + accused at ACCUSED (one builder; returns it at the accusation).
fn accused<'a>(state: &'a PalwChainStateV2, p: &'a PalwStateParamsV2, e: &'a PalwTransitionExtrasV1, family: u8) -> TransitionBuilder<'a> {
    let mut b = TransitionBuilder::new(state, p, false, false, false, false, e);
    hold_native_fee_v1(&mut b, &held(family)).unwrap();
    accuse_post_final_v1(&mut b, &ctx(2, ACCUSED, 2), &accuser(), family, &JOB, [0xAC; 64]).unwrap();
    b
}
fn deadline(p: &PalwStateParamsV2) -> u64 {
    ACCUSED + p.turn_deadline_daa()
}
fn production(b: &TransitionBuilder<'_>, r: &PalwNativeHeldFeeRowV1, c: &crate::palw_court_duty_clock_v1::CourtDutyClockV1) -> Result<bool, &'static str> {
    native_post_final_duty_answered_v1(b, r, c)
}

#[test]
fn r6_d4_post_final_accusation_creates_rooted_duty_clocks() {
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11] {
        let state = parent();
        let (p, e) = (armed(), conviction_extras());
        let b = accused(&state, &p, &e, family);
        let row = native_post_final_duty_row_v1(&b.state, family, &JOB).expect("accusation recorded");
        assert_eq!((row.accuser, row.accused_daa, row.accusation), (accuser(), ACCUSED, [0xAC; 64]));
        assert_eq!(row.duties.len(), 1, "one clock per responsible actor (the held producer)");
        let c = &row.duties[0].clock;
        assert_eq!(c.answerer, producer());
        assert_eq!((c.predecessor_daa, c.first_eligible_daa, c.response_deadline_daa), (ACCUSED, ACCUSED + 1, deadline(&p)));
        assert_eq!((c.claim, c.turn, c.original_hard_daa), (Hash64::from_bytes(held(family).claim), 0, HORIZON));
        assert!(row.duties[0].answered.is_none() && row.duties[0].supplied_daa.is_none());
        // Not a staged-court TIR clock: the TIR per-claim index is untouched.
        assert!(b.state.court_duty_clocks_for_claim_v1(c.claim).is_empty());
    }
    // Refusals: below the fences, the producer accusing itself, a second accusation, no held fee.
    let state = parent();
    let (p, e) = (params(), conviction_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    assert!(accuse_post_final_v1(&mut b, &ctx(2, ACCUSED, 2), &accuser(), 29, &JOB, [1; 64]).is_err());
    let (p, e) = (armed(), PalwTransitionExtrasV1::default());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    hold_native_fee_v1(&mut b, &held(29)).unwrap();
    assert!(accuse_post_final_v1(&mut b, &ctx(2, ACCUSED, 2), &accuser(), 29, &JOB, [1; 64]).is_err());
    let (p, e) = (armed(), conviction_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    assert!(accuse_post_final_v1(&mut b, &ctx(2, ACCUSED, 2), &accuser(), 29, &JOB, [1; 64]).is_err(), "no held fee");
    hold_native_fee_v1(&mut b, &held(29)).unwrap();
    assert!(accuse_post_final_v1(&mut b, &ctx(2, ACCUSED, 2), &producer(), 29, &JOB, [1; 64]).is_err(), "not the producer");
    assert!(accuse_post_final_v1(&mut b, &ctx(2, ACCUSED, 2), &accuser(), 39, &JOB, [1; 64]).is_err(), "mining39 not covered");
    accuse_post_final_v1(&mut b, &ctx(2, ACCUSED, 2), &accuser(), 29, &JOB, [1; 64]).unwrap();
    assert!(accuse_post_final_v1(&mut b, &ctx(3, ACCUSED + 1, 3), &accuser(), 29, &JOB, [2; 64]).is_err(), "one at a time");
}

#[test]
fn r6_d4_silent_supplied_actor_defaults_with_the_test_slash_value() {
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11] {
        let state = parent();
        let (p, e) = (armed(), conviction_extras());
        let mut b = accused(&state, &p, &e, family);
        let before = collateral(&b.state);
        // A block inside the window leaves room for one packet: the opportunity is recorded.
        observe_native_post_final_duties_with_v1(&mut b, &ctx(3, ACCUSED + 1, 3), &profile()).unwrap();
        assert_eq!(native_post_final_duty_row_v1(&b.state, family, &JOB).unwrap().duties[0].supplied_daa, Some(ACCUSED + 1));
        // Window still open → undecided.
        assert!(decide_native_post_final_default_v1(&b, &held(family), deadline(&p), &production).is_err());
        // Past the deadline, silent: the production silence fact decides natively.
        let after = ctx(4, deadline(&p) + 1, 4);
        assert_eq!(
            decide_native_post_final_default_v1(&b, &held(family), after.daa_score, &production),
            Ok(CourtServiceAttributionV1::PartyDefault)
        );
        recover_native_post_final_defaults_with_v1(&mut b, &after, SLASH, &production).unwrap();
        assert!(held_native_fee_v1(&b.state, family, &JOB).is_none(), "defaulted by the sweep");
        assert_eq!(queued(&b.state, native_refund_queue_v1(&held(family)).as_bytes()), Some((h64(0xB2), 7_000)), "requester refunded");
        assert_eq!(collateral(&b.state), before - SLASH, "actor slashed");
        assert!(native_post_final_duty_row_v1(&b.state, family, &JOB).is_none(), "the duty record left with the fee");
    }
    // Production: the slash amount is typed-missing, so the production sweep does nothing.
    assert!(PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1.is_none());
    let state = parent();
    let (p, e) = (armed(), conviction_extras());
    let mut b = accused(&state, &p, &e, 29);
    observe_native_post_final_duties_with_v1(&mut b, &ctx(3, ACCUSED + 1, 3), &profile()).unwrap();
    let n = b.entries.len();
    recover_native_post_final_defaults_v1(&mut b, &ctx(4, deadline(&p) + 1, 4)).unwrap();
    observe_native_post_final_duties_v1(&mut b, &ctx(4, deadline(&p) + 1, 4)).unwrap();
    assert_eq!(b.entries.len(), n);
}

#[test]
fn r6_d4_an_answering_actor_is_not_defaulted() {
    let family = PALW_NATIVE_FAMILY_FUNDED_V11;
    let state = parent();
    let (p, e) = (armed(), conviction_extras());
    let mut b = accused(&state, &p, &e, family);
    observe_native_post_final_duties_with_v1(&mut b, &ctx(3, ACCUSED + 1, 3), &profile()).unwrap();
    // Only the actor may answer, only inside its window.
    assert!(answer_post_final_v1(&mut b, &ctx(3, ACCUSED + 1, 3), &accuser(), family, &JOB, b"x").is_err());
    assert!(answer_post_final_v1(&mut b, &ctx(3, deadline(&p) + 1, 3), &producer(), family, &JOB, b"x").is_err(), "late");
    answer_post_final_v1(&mut b, &ctx(3, ACCUSED + 2, 3), &producer(), family, &JOB, b"the material").unwrap();
    assert!(answer_post_final_v1(&mut b, &ctx(3, ACCUSED + 3, 3), &producer(), family, &JOB, b"again").is_err(), "once");
    let before = collateral(&b.state);
    let after = ctx(4, deadline(&p) + 1, 4);
    assert!(decide_native_post_final_default_v1(&b, &held(family), after.daa_score, &production).is_err());
    assert!(apply_native_post_final_default_with_v1(&mut b, &after, family, &JOB, SLASH, &production).is_err());
    recover_native_post_final_defaults_with_v1(&mut b, &after, SLASH, &production).unwrap();
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_some(), "still held; paid at H");
    assert_eq!(collateral(&b.state), before);
    release_held_native_fees_v1(&mut b, HORIZON + 1, |_, _| false).unwrap();
    assert_eq!(queued(&b.state, held(family).queue), Some((h64(0xB1), 7_000)), "released to the producer at H");
    assert!(native_post_final_duty_row_v1(&b.state, family, &JOB).is_none());
}

#[test]
fn r6_d4_no_supply_is_a_no_slash_hold_then_a_recovery_turn() {
    let family = PALW_NATIVE_FAMILY_TEXT_V29;
    let state = parent();
    let (p, e) = (armed(), conviction_extras());
    let mut b = accused(&state, &p, &e, family);
    let before = collateral(&b.state);
    // Every block in the window spent the court capacity: no opportunity recorded.
    b.court_cost_spent = crate::palw_court_capacity_v1::PalwCourtCostV1 { work: 100, bytes: 100, memory: 0 };
    observe_native_post_final_duties_with_v1(&mut b, &ctx(3, ACCUSED + 1, 3), &profile()).unwrap();
    b.court_cost_spent = Default::default();
    assert!(native_post_final_duty_row_v1(&b.state, family, &JOB).unwrap().duties[0].supplied_daa.is_none());
    let after = ctx(4, deadline(&p) + 1, 4);
    assert!(matches!(
        apply_native_post_final_default_with_v1(&mut b, &after, family, &JOB, SLASH, &production).unwrap(),
        NativePostFinalDefaultV1::NoSlashHold(_)
    ));
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_some());
    assert_eq!(collateral(&b.state), before, "no slash");
    // Recovery: the sweep re-opens one more turn at this block.
    recover_native_post_final_defaults_with_v1(&mut b, &after, SLASH, &production).unwrap();
    let row = native_post_final_duty_row_v1(&b.state, family, &JOB).unwrap();
    assert_eq!(row.duties.len(), 2);
    let next = &row.duties[1].clock;
    assert_eq!((next.turn, next.first_eligible_daa, next.response_deadline_daa), (1, after.daa_score + 1, after.daa_score + p.turn_deadline_daa()));
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_some());
    // The new turn is supplied and the actor stays silent: default.
    observe_native_post_final_duties_with_v1(&mut b, &ctx(5, after.daa_score + 1, 5), &profile()).unwrap();
    let later = ctx(6, next.response_deadline_daa + 1, 6);
    recover_native_post_final_defaults_with_v1(&mut b, &later, SLASH, &production).unwrap();
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_none());
    assert_eq!(collateral(&b.state), before - SLASH);
}

#[test]
fn r6_d4_reorg_restores_clocks_answers_and_the_default() {
    let family = PALW_NATIVE_FAMILY_FUNDED_V11;
    let state = parent();
    let (p, e) = (armed(), conviction_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    hold_native_fee_v1(&mut b, &held(family)).unwrap();
    let held_root = b.state.state_root();
    let mark = b.entries.len();
    accuse_post_final_v1(&mut b, &ctx(2, ACCUSED, 2), &accuser(), family, &JOB, [0xAC; 64]).unwrap();
    observe_native_post_final_duties_with_v1(&mut b, &ctx(3, ACCUSED + 1, 3), &profile()).unwrap();
    recover_native_post_final_defaults_with_v1(&mut b, &ctx(4, deadline(&p) + 1, 4), SLASH, &production).unwrap();
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_none());
    let done = b.state.state_root();
    let mut back = b.state.clone();
    for entry in b.entries[mark..].iter().rev() {
        apply_delta_entry(&mut back, entry, true).unwrap();
    }
    assert_eq!(back.state_root(), held_root, "undo restores the held fee and removes the clocks");
    assert!(native_post_final_duty_row_v1(&back, family, &JOB).is_none());
    assert!(held_native_fee_v1(&back, family, &JOB).is_some());
    for entry in &b.entries[mark..] {
        apply_delta_entry(&mut back, entry, false).unwrap();
    }
    assert_eq!(back.state_root(), done);
}

/// **audit(b) B-02 reproducer** (round 1, not a fix): the post-Final duty is content-free. A decoy accusation by the producer's
/// ally (any Active bond, a free 64-byte digest) answered with ONE arbitrary byte (never judged) closes the job's post-Final path for
/// the whole liability horizon: an honest verifier's later accusation is refused ("one at a time") and the fee is released to the
/// producer at H without any judgement.
#[test]
fn audit_b02_a_decoy_accusation_answered_with_garbage_closes_the_post_final_path() {
    let family = PALW_NATIVE_FAMILY_TEXT_V29;
    let (mut state, _) = apply_palw_transition_v2(
        &PalwChainStateV2::genesis(),
        &params(),
        &ctx(1, 10, 1),
        &[
            bond_registration_with_collateral(5, 7, 21, 1_000_000),
            bond_registration_with_collateral(6, 8, 22, 1_000_000),
            bond_registration_with_collateral(7, 9, 23, 1_000_000),
        ],
        None,
    )
    .expect("the bonds register");
    state.kernel_route = parent().kernel_route.clone();
    let honest = bond_key(7);
    let (p, e) = (armed(), conviction_extras());
    let mut b = accused(&state, &p, &e, family); // the ally (bond 6) accuses with an arbitrary digest
    observe_native_post_final_duties_with_v1(&mut b, &ctx(3, ACCUSED + 1, 3), &profile()).unwrap();
    // The producer "answers" with one garbage byte.
    answer_post_final_v1(&mut b, &ctx(3, ACCUSED + 2, 3), &producer(), family, &JOB, &[0u8]).unwrap();
    let after = ctx(4, deadline(&p) + 1, 4);
    assert!(decide_native_post_final_default_v1(&b, &held(family), after.daa_score, &production).is_err(), "answered: no default");
    // An honest verifier can no longer accuse this job for the rest of H.
    assert!(b.state.bond(&honest).is_some_and(|v| matches!(v.status, PalwBondStatusV2::Active)));
    let refused = accuse_post_final_v1(&mut b, &ctx(5, deadline(&p) + 2, 5), &honest, family, &JOB, [0x77; 64]);
    eprintln!("[audit-b02] honest accusation after the decoy: {refused:?}");
    assert!(refused.is_err(), "B-02: the decoy blocks every later post-Final accusation of the job");
    let refused_late = accuse_post_final_v1(&mut b, &ctx(6, HORIZON - 100, 6), &honest, family, &JOB, [0x78; 64]);
    assert!(refused_late.is_err());
    release_held_native_fees_v1(&mut b, HORIZON + 1, |_, _| false).unwrap();
    assert_eq!(queued(&b.state, held(family).queue), Some((h64(0xB1), 7_000)), "released to the producer at H, never judged");
}
