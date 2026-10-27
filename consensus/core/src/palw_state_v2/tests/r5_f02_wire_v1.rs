//! G14 lane r5-f02-wire (2026-10-22) — written, not compiled. RFC-0029 tag-178 dispatch, the RFC-0026 F02 default during the RFC08
//! post-Final hold (text29 / funded v11), its no-slash hold / recovery / release at H, the per-claim court-service index and the
//! shared bottom's captured end evidence.
use super::super::palw_court_service_lifecycle_fold_v1 as lifecycle;
use super::super::palw_native_liability_hold_fold_v1::*;
use super::super::palw_native_post_final_default_fold_v1::*;
use super::*;
use crate::palw_court_duty_clock_v1::CourtDutyClockV1;
use crate::palw_court_service_binding_v1::{CourtServiceAttributionV1, CourtServiceProfileV1, court_service_binding_candidate_v1};
use crate::palw_court_service_state_v2::CourtServiceStateV2;
use crate::palw_court_service_v1::ServiceCost;
use crate::palw_native_liability_hold_v1::*;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};

const JOB: [u8; 64] = [0x5E; 64];
const A: u64 = 1_000;
const HORIZON: u64 = 50_000;
const SLASH: u64 = 2_500;

fn producer() -> PalwBondKeyV2 {
    bond_key(5)
}
fn claim() -> Hash64 {
    h64(0xC1A1)
}
fn armed() -> PalwStateParamsV2 {
    params().with_settlement_once_from(Some(0)).with_native_liability_horizon_from(Some(100))
}
/// TEST-ONLY inclusion premise (lane F02): not a production value; production is typed-missing.
const TEST_ONLY_INCLUSION_PREMISE: crate::palw_court_service_binding_v1::CourtServiceInclusionPremiseV1 =
    crate::palw_court_service_binding_v1::CourtServiceInclusionPremiseV1 { version: 1, inclusion_delay_daa: 0 };
fn conviction_extras() -> PalwTransitionExtrasV1 {
    let mut e = PalwTransitionExtrasV1::default();
    e.tir_public_conviction_active = true;
    e.court_service_inclusion_premise_v1 = Some(TEST_ONLY_INCLUSION_PREMISE);
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
fn service(skip: Option<u64>) -> CourtServiceStateV2 {
    let signers: BTreeSet<_> = [producer()].into_iter().collect();
    let mut s = court_service_binding_candidate_v1(None, claim(), h64(2), h64(3), A, &signers, &profile()).unwrap();
    observe(&mut s, A + 1, A + 101, skip);
    s
}
fn observe(s: &mut CourtServiceStateV2, from: u64, to: u64, skip: Option<u64>) {
    for daa in from..=to {
        if Some(daa) != skip {
            s.book.observe_block(daa, [daa as u8; 64], ServiceCost::default()).unwrap();
        }
    }
}
fn clock(duty: u64, deadline: u64) -> CourtDutyClockV1 {
    CourtDutyClockV1 {
        version: 1,
        profile_id: h64(9),
        duty_id: h64(duty),
        claim: claim(),
        allocation_id: h64(2),
        source_id: h64(3),
        acquisition: Key::TirAcquisition(claim(), producer(), h64(11)),
        answerer: producer(),
        demand_id: h64(12),
        turn: 0,
        predecessor_block: h64(13),
        predecessor_daa: A,
        first_eligible_daa: A + 1,
        response_deadline_daa: deadline,
        original_hard_daa: HORIZON,
        material_until_daa: HORIZON,
    }
}
fn held(family: u8) -> PalwNativeHeldFeeRowV1 {
    PalwNativeHeldFeeRowV1 {
        version: PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1,
        family,
        job: JOB,
        claim: claim().as_bytes(),
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
/// A parent with the producer's bond, the kernel route, a bound + observed service book and the actor's duty clock (index rebuilt).
fn parent(service: CourtServiceStateV2, deadline: u64) -> PalwChainStateV2 {
    let (mut state, _) = apply_palw_transition_v2(
        &PalwChainStateV2::genesis(),
        &params(),
        &ctx(1, 10, 1),
        &[bond_registration_with_collateral(5, 7, 21, 1_000_000)],
        None,
    )
    .expect("the producer's bond registers");
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    state.staged_courts.insert(Key::CourtService, Row::CourtService(Box::new(service).into()));
    state.staged_courts.insert(Key::CourtDutyClock(h64(77)), Row::CourtDutyClock(Box::new(clock(77, deadline))));
    super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
    state
}
fn queued(state: &PalwChainStateV2, key: [u8; 64]) -> Option<(Hash64, u64)> {
    state.pending_payouts.get(&Hash64::from_bytes(key)).map(|p| (p.payload, p.amount))
}
fn silent(_: &TransitionBuilder<'_>, _: &PalwNativeHeldFeeRowV1, _: &CourtDutyClockV1) -> Result<bool, &'static str> {
    Ok(false)
}
fn collateral(state: &PalwChainStateV2) -> u64 {
    state.bond(&producer()).unwrap().collateral
}

#[test]
fn r5_f02_wire_tag_178_is_dormant_below_its_fences() {
    let p = params();
    let state = PalwChainStateV2::genesis();
    let extras = PalwTransitionExtrasV1::default();
    let at = ctx(1, 100, 1);
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let ids = crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1::Ids { claim: h64(1), rows_root: h64(2), ids: vec![1] };
    assert!(matches!(lifecycle::apply_shared_court_wire_op_v1(&mut b, &at, &ids), Err(PalwStateV2Error::ObjectiveOffenceDormant)));
    let default = crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1::NativeHeldDefault { family: 29, job: JOB };
    assert!(matches!(lifecycle::apply_shared_court_wire_op_v1(&mut b, &at, &default), Err(PalwStateV2Error::ObjectiveOffenceDormant)));
    assert!(b.entries.is_empty());
}

#[test]
fn r5_f02_wire_post_final_default_is_dormant_and_production_is_typed_missing() {
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11] {
        let state = parent(service(None), 1_200);
        let at = ctx(2, 2_000, 2);
        // No hold fence.
        let (p, extras) = (params(), conviction_extras());
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        assert!(matches!(
            apply_native_post_final_default_with_v1(&mut b, &at, family, &JOB, SLASH, &silent),
            Err(PalwStateV2Error::ObjectiveOffenceDormant)
        ));
        // No conviction fence.
        let (p, extras) = (armed(), PalwTransitionExtrasV1::default());
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        assert!(matches!(
            apply_native_post_final_default_with_v1(&mut b, &at, family, &JOB, SLASH, &silent),
            Err(PalwStateV2Error::ObjectiveOffenceDormant)
        ));
        // Production past both fences: the service profile is pinned since 56d36e707 (user-approved G14 estimate V1), so the
        // path is no longer dormant here — it is refused on its own terms (this fixture's job book holds no readable reserve,
        // `p3_v6_the_production_slash_needs_the_claims_readable_reserve`), and nothing is written. Below either fence it is
        // still dormant (the two cases above).
        let (p, extras) = (armed(), conviction_extras());
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        hold_native_fee_v1(&mut b, &held(family)).unwrap();
        let before = b.entries.len();
        let production = apply_native_post_final_default_v1(&mut b, &at, family, &JOB);
        assert!(production.is_err() && !matches!(production, Err(PalwStateV2Error::ObjectiveOffenceDormant)), "{production:?}");
        recover_native_post_final_defaults_v1(&mut b, &at).unwrap();
        assert_eq!(b.entries.len(), before);
        // V6 (decided 2026-10-22, lane P3): the rule is the claim's reserve; the production stays dormant on the service profile.
        assert_eq!(PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1, Some(PalwNativePostFinalDefaultSlashV1::ClaimReserve));
    }
}

/// V6 (decided 2026-10-22, lane P3): the production slash is read from the family's anchored book — a held row whose job book is
/// absent has no readable reserve, so the production amount is refused (never a default at a guessed amount, never zero).
#[test]
fn p3_v6_the_production_slash_needs_the_claims_readable_reserve() {
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11] {
        let state = parent(service(None), 1_200);
        let (p, extras) = (armed(), conviction_extras());
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        hold_native_fee_v1(&mut b, &held(family)).unwrap();
        let rule = PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1.expect("decided");
        assert!(native_post_final_default_slash_v1(&b, rule, &held(family)).is_err(), "no anchored book: no amount");
        let mut other = held(family);
        other.family = crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_JOB_V39;
        assert!(native_post_final_default_slash_v1(&b, rule, &other).is_err(), "job v39 has no post-Final F02 default");
    }
}

/// V6 (decided 2026-10-22, lane P3): the amount is the collectible reserve — the reserve when the bond holds it, what the bond still
/// holds when it holds less (G-14), 0 for a vanished bond — the same function the families' conviction and DA default use.
#[test]
fn p3_v6_the_slash_is_the_collectible_reserve() {
    use crate::palw_g14_lane_closure_v1::palw_native_producer_collectible_v1 as collectible;
    assert_eq!(collectible(Some(1_000_000), 250_000), 250_000);
    assert_eq!(collectible(Some(100_000), 250_000), 100_000);
    assert_eq!(collectible(None, 250_000), 0);
    // Through the fixture path at that amount: refund + slash of exactly the collectible reserve.
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11] {
        let state = parent(service(None), 1_200);
        let (p, extras) = (armed(), conviction_extras());
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        hold_native_fee_v1(&mut b, &held(family)).unwrap();
        let before = collateral(&b.state);
        let reserve = before / 4;
        let slash = collectible(Some(before), reserve);
        let out = apply_native_post_final_default_with_v1(&mut b, &ctx(2, 2_000, 2), family, &JOB, slash, &silent).unwrap();
        assert_eq!(out, NativePostFinalDefaultV1::Defaulted { refunded: 7_000, slashed: reserve });
        assert_eq!(collateral(&b.state), before - reserve);
    }
}

#[test]
fn r5_f02_wire_post_final_supplied_and_silent_refunds_the_requester_and_slashes_the_actor() {
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11] {
        let state = parent(service(None), 1_200);
        let (p, extras) = (armed(), conviction_extras());
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        hold_native_fee_v1(&mut b, &held(family)).unwrap();
        let before = collateral(&b.state);
        let at = ctx(2, 2_000, 2);
        let out = apply_native_post_final_default_with_v1(&mut b, &at, family, &JOB, SLASH, &silent).unwrap();
        assert_eq!(out, NativePostFinalDefaultV1::Defaulted { refunded: 7_000, slashed: SLASH });
        assert!(held_native_fee_v1(&b.state, family, &JOB).is_none(), "the held fee left once");
        assert_eq!(queued(&b.state, native_refund_queue_v1(&held(family)).as_bytes()), Some((h64(0xB2), 7_000)), "requester refunded");
        assert_eq!(collateral(&b.state), before - SLASH, "actor slashed");
        // Never paid afterwards, never defaulted twice.
        release_held_native_fees_v1(&mut b, HORIZON + 1, |_, _| false).unwrap();
        assert!(queued(&b.state, held(family).queue).is_none());
        assert!(apply_native_post_final_default_with_v1(&mut b, &at, family, &JOB, SLASH, &silent).is_err());
    }
}

#[test]
fn r5_f02_wire_post_final_open_window_answered_or_unknown_is_undecided() {
    let family = PALW_NATIVE_FAMILY_TEXT_V29;
    let (p, extras) = (armed(), conviction_extras());
    // The actor's window is still open at `now`.
    let state = parent(service(None), 3_000);
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    hold_native_fee_v1(&mut b, &held(family)).unwrap();
    assert!(apply_native_post_final_default_with_v1(&mut b, &ctx(2, 2_000, 2), family, &JOB, SLASH, &silent).is_err());
    // Answered.
    let state = parent(service(None), 1_200);
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    hold_native_fee_v1(&mut b, &held(family)).unwrap();
    let answered = |_: &TransitionBuilder<'_>, _: &PalwNativeHeldFeeRowV1, _: &CourtDutyClockV1| Ok(true);
    assert!(apply_native_post_final_default_with_v1(&mut b, &ctx(2, 2_000, 2), family, &JOB, SLASH, &answered).is_err());
    // The production silence fact: neither family records post-Final answers -> undecided, never inferred.
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11] {
        let row = held(family);
        assert!(native_post_final_duty_answered_v1(&b, &row, &clock(77, 1_200)).is_err());
        assert!(decide_native_post_final_default_v1(&b, &row, 2_000, &native_post_final_duty_answered_v1).is_err());
    }
    // An unsupported family (mining39) never defaults here.
    assert!(decide_native_post_final_default_v1(&b, &held(PALW_NATIVE_FAMILY_JOB_V39), 2_000, &silent).is_err());
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_some(), "nothing moved");
}

#[test]
fn r5_f02_wire_missing_supply_is_a_no_slash_hold_and_the_sweep_resumes_once_supplied() {
    let family = PALW_NATIVE_FAMILY_FUNDED_V11;
    let mut book = service(None);
    // Rebuild with one reserved unit unsupplied.
    let skipped = *book.book.assignments.values().next().unwrap();
    let unsupplied = |to: u64| {
        let signers: BTreeSet<_> = [producer()].into_iter().collect();
        let mut s = court_service_binding_candidate_v1(None, claim(), h64(2), h64(3), A, &signers, &profile()).unwrap();
        observe(&mut s, A + 1, to, Some(skipped));
        s
    };
    book = unsupplied(A + 101);
    let state = parent(book.clone(), 1_900);
    let (p, extras) = (armed(), conviction_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    hold_native_fee_v1(&mut b, &held(family)).unwrap();
    let before = collateral(&b.state);
    let at = ctx(2, 2_000, 2);
    let out = apply_native_post_final_default_with_v1(&mut b, &at, family, &JOB, SLASH, &silent).unwrap();
    assert!(matches!(out, NativePostFinalDefaultV1::NoSlashHold(_)));
    // Held: fee, evidence (the row) and the actor's collateral all stay; the sweep does nothing while supply is missing.
    recover_native_post_final_defaults_with_v1(&mut b, &at, SLASH, &silent).unwrap();
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_some());
    assert_eq!(collateral(&b.state), before);
    // Recovery: the canonical sweep moves the missing slot and later blocks supply it.
    // Since the D1 release rule (3fc206d09, fenced court service) a missed packet is moved only while its own `service_before`
    // is still ahead (`DeadlineExhausted` otherwise): the canonical sweep runs on the book as it stood just past the missed slot.
    let mut recovered = unsupplied(skipped + 1);
    let report = recovered.repair_missing_supply().unwrap();
    assert!(!report.is_empty() && report.iter().all(|r| matches!(r.result, Ok(n) if n > 0)), "the sweep moved the slot: {report:?}");
    observe(&mut recovered, skipped + 2, A + 800, None);
    assert_eq!(
        crate::palw_court_service_binding_v1::attribute_service_default_v1(
            &recovered,
            claim(),
            &[&clock(77, 1_900)],
            &[producer()].into_iter().collect(),
            Some(&TEST_ONLY_INCLUSION_PREMISE)
        ),
        Ok(CourtServiceAttributionV1::PartyDefault),
        "the moved slot is supplied inside the actor's window"
    );
    b.state.staged_courts.insert(Key::CourtService, Row::CourtService(Box::new(recovered).into()));
    recover_native_post_final_defaults_with_v1(&mut b, &at, SLASH, &silent).unwrap();
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_none(), "resumed: defaulted");
    assert_eq!(queued(&b.state, native_refund_queue_v1(&held(family)).as_bytes()), Some((h64(0xB2), 7_000)));
    assert_eq!(collateral(&b.state), before - SLASH);
}

#[test]
fn r5_f02_wire_an_unrecovered_hold_releases_at_the_horizon_without_slash() {
    let family = PALW_NATIVE_FAMILY_TEXT_V29;
    let signers: BTreeSet<_> = [producer()].into_iter().collect();
    let mut book = court_service_binding_candidate_v1(None, claim(), h64(2), h64(3), A, &signers, &profile()).unwrap();
    let skipped = *book.book.assignments.values().next().unwrap();
    observe(&mut book, A + 1, A + 101, Some(skipped));
    let state = parent(book, 1_900);
    let (p, extras) = (armed(), conviction_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    hold_native_fee_v1(&mut b, &held(family)).unwrap();
    let before = collateral(&b.state);
    let past = ctx(3, HORIZON + 1, 3);
    // Past H nothing is decidable; the sweep leaves it to the release.
    assert!(decide_native_post_final_default_v1(&b, &held(family), HORIZON + 1, &silent).is_err());
    recover_native_post_final_defaults_with_v1(&mut b, &past, SLASH, &silent).unwrap();
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_some());
    // The release (the family's open-dispute fact is the release sweep's own; here: none) pays the producer, no slash.
    release_held_native_fees_v1(&mut b, HORIZON + 1, |_, _| false).unwrap();
    assert!(held_native_fee_v1(&b.state, family, &JOB).is_none());
    assert_eq!(queued(&b.state, held(family).queue), Some((h64(0xB1), 7_000)));
    assert_eq!(collateral(&b.state), before, "released without slash");
}

#[test]
fn r5_f02_wire_claim_index_matches_the_scan_and_follows_writes_and_undo() {
    use super::super::palw_staged_court_fold_v1::{rebuild_indices, update_indices};
    let mut state = parent(service(None), 1_200);
    assert_eq!(state.court_duty_clocks_for_claim_v1(claim()).len(), 1);
    assert_eq!(state.court_service_claim_scan_v1(claim()), (1, None));
    // A second clock and two notices, written through the index projection (as the writer and delta apply do).
    let second = Row::CourtDutyClock(Box::new(clock(78, 1_300)));
    state.staged_courts.insert(Key::CourtDutyClock(h64(78)), second.clone());
    update_indices(&mut state, Key::CourtDutyClock(h64(78)), None, Some(&second));
    let query = |daa: u64, id: u64| {
        Row::CourtSharedQuery(Box::new(crate::palw_court_shared_tir_state_v1::SharedTirQueryV1 {
            version: 1,
            claim: claim(),
            scope_id: h64(20),
            ordinal: id,
            request_id: h64(id),
            opened_block: h64(21),
            opened_daa: daa,
            opened_by: producer(),
            intent_digest: h64(22),
            material_until_daa: HORIZON,
            body: None,
            service: None,
        }))
    };
    for (daa, id) in [(A + 9, 0x31u64), (A + 3, 0x32)] {
        let row = query(daa, id);
        state.staged_courts.insert(Key::CourtSharedQuery(h64(id)), row.clone());
        update_indices(&mut state, Key::CourtSharedQuery(h64(id)), None, Some(&row));
    }
    assert_eq!(state.court_duty_clocks_for_claim_v1(claim()).len(), 2);
    assert_eq!(state.court_shared_initial_notice_daa_v1(claim()), Some(A + 3), "the earliest notice");
    assert_eq!(state.court_service_claim_scan_v1(claim()), (2, Some(A + 3)));
    assert_eq!(state.court_shared_initial_notice_daa_v1(h64(0xDEAD)), None);
    // Undo (reorg): the earliest notice leaves; the index follows.
    let row = state.staged_courts.remove(&Key::CourtSharedQuery(h64(0x32))).unwrap();
    update_indices(&mut state, Key::CourtSharedQuery(h64(0x32)), Some(&row), None);
    assert_eq!(state.court_shared_initial_notice_daa_v1(claim()), Some(A + 9));
    assert_eq!(state.court_service_claim_scan_v1(claim()), (2, Some(A + 9)));
    // A rebuild from the rooted rows alone yields the same projection.
    let incremental = state.staged_court_service_by_claim.clone();
    rebuild_indices(&mut state);
    assert_eq!(state.staged_court_service_by_claim, incremental);
}

#[test]
fn r5_f02_wire_shared_bottom_captures_its_actual_end_evidence() {
    let p = params();
    let state = PalwChainStateV2::genesis();
    let extras = PalwTransitionExtrasV1::default();
    let at = ctx(4, 4_000, 4);
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let manifest = crate::palw_court_numerical_transport_v1::NumericalManifestV1 {
        version: 1,
        network: h64(1),
        scope: h64(2),
        claim: claim(),
        leaf: 0,
        kind: crate::palw_court_numerical_transport_v1::NumericalKindV1::Bottom,
        total_bytes: 3,
        body_hash: h64(3),
    };
    let parts = vec![crate::palw_court_numerical_transport_v1::NumericalPartV1 {
        version: 1,
        stream_id: h64(4),
        offset: 0,
        bytes: vec![1, 2, 3],
    }];
    assert!(b.court_end_evidence.is_none());
    let previous = lifecycle::capture_shared_bottom_evidence_v1(&mut b, &at, &manifest, &parts).unwrap();
    assert!(previous.is_none());
    match b.court_end_evidence.as_deref() {
        Some(crate::palw_court_end_archive_v1::CourtEndEvidenceV1::Native { point, object }) => {
            assert_eq!(*point, at);
            assert!(matches!(object.as_ref(), PalwConsensusObjectV2::SharedCourtOpV1 { op }
                if matches!(op.as_ref(), crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1::Numerical { manifest: m, parts: q }
                    if *m == manifest && *q == parts)));
        }
        other => panic!("shared bottom evidence was not captured: {other:?}"),
    }
    // Idempotent when the object entry already captured it.
    let again = lifecycle::capture_shared_bottom_evidence_v1(&mut b, &at, &manifest, &parts).unwrap();
    assert!(again.is_some());
}
