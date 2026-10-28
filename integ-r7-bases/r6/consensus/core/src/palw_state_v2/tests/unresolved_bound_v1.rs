//! **RFC-0023 / ADR-0186 stage 1 through the fold's own floor rows**: claims with different liability horizons, the retirement floor
//! (audit F05: the floor update never re-checked the oldest open claim — the invariant under test), a late conviction's rollback, and
//! the carriage round trip a pruned/IBD node restores from, as one set.
use super::*;
use crate::palw_rfc0018_v1::{PalwGateCRuleV1, PalwRfc0018MirrorV1};
use crate::palw_security_certification_v1::*;
use crate::palw_unresolved_bound_v1::palw_resolved_through_v1;

const FENCE: u64 = 50_000;

fn class() -> Hash64 {
    Hash64::from_u64_word(0xC1A5_0001)
}
fn id(n: u64) -> Hash64 {
    Hash64::from_u64_word(0x5EC0_0000 + n)
}
fn params(stage1: bool) -> PalwStateParamsV2 {
    let base = match crate::config::params::palw_t12_shipped_params().palw_consensus_mode {
        crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state,
        _ => unreachable!(),
    };
    base.with_rfc0018_v1(Some(PalwRfc0018MirrorV1 {
        sibling_attribution_from_daa: Some(FENCE),
        panel_anchor_permissionless_from_daa: Some(FENCE),
        rule_e_participation_v2_from_daa: Some(FENCE),
        gate_c_from_daa: Some(FENCE),
        gate_c_rule: PalwGateCRuleV1::SecurityCertifiedV1,
        certification: PalwSecurityCertificationPolicyV1::design_unapproved_v1(720),
        fp_receipt_tail_daa: 0,
    }))
    .with_unresolved_bound_from_daa(stage1.then_some(FENCE))
}
fn claim(daa: u64, blue: u64, l: u64, phase: PalwClaimPhaseV2) -> PalwClaimStateV2 {
    let mut c = palw_claim_template_v1(
        class(),
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(9), 0)),
        daa,
        0,
        1_000,
    );
    c.pwu = 900;
    c.phase = phase;
    c.accepted_blue_score = blue;
    c.accepted_block = Hash64::from_u64_word(0xB10C_0000 + blue);
    c.trace_retention_daa = daa + l;
    c
}
fn certify(state: &mut PalwChainStateV2, claim: Hash64) {
    let row = PalwClaimCertificationRowV1 {
        claim: claim.as_bytes(),
        class_id: class().as_bytes(),
        producer_bond: [1; 64],
        producer_principal: [2; 64],
        attempt: true,
        final_daa: FENCE + 10,
        committed_daa: FENCE + 10,
        committed_span: 0,
        anchor: None,
        draws: Vec::new(),
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: Vec::new(),
        held_reward: 0,
        payout_payload: [3; 64],
        outcome: Some((PalwCertificationOutcomeV1::Certified, FENCE + 99)),
    };
    state.set_kernel_aux_row_for_tests(
        PALW_SECURITY_CERTIFICATION_TABLE_V1,
        certification_claim_key_v1(&palw_certification_id_v1(&claim)),
        Some(borsh::to_vec(&row).unwrap()),
    );
}
fn world() -> PalwChainStateV2 {
    let mut s = PalwChainStateV2::genesis();
    // A: open, accepted FENCE + 1 at blue 10, a LONG horizon (bound FENCE + 20,001).
    s.set_false_valid_rows_for_tests(id(1), Some(claim(FENCE + 1, 10, 20_000, PalwClaimPhaseV2::Provisional)), None, None, 0);
    s.set_unresolved_for_tests(10, id(1), true);
    // B: certified Final at blue 20, a SHORT horizon (it retires while A is still open). C: certified Final at blue 30.
    s.set_false_valid_rows_for_tests(
        id(2),
        Some(claim(FENCE + 2, 20, 1_000, PalwClaimPhaseV2::Final { final_daa: FENCE + 100 })),
        None,
        None,
        0,
    );
    certify(&mut s, id(2));
    s.set_false_valid_rows_for_tests(
        id(3),
        Some(claim(FENCE + 3, 30, 1_000, PalwClaimPhaseV2::Final { final_daa: FENCE + 100 })),
        None,
        None,
        0,
    );
    certify(&mut s, id(3));
    let floor = PalwCertifiedFrontierFloorV1 { blue_score: 0, block: [0; 64] };
    s.set_kernel_aux_row_for_tests(
        PALW_SECURITY_CERTIFICATION_TABLE_V1,
        certification_frontier_floor_key_v1(),
        Some(borsh::to_vec(&floor).unwrap()),
    );
    s
}
fn at(s: &mut PalwChainStateV2, daa: u64) {
    s.set_last_point_for_tests(Some(PalwBlockContextV2 {
        block: BlockHash::from_u64_word(daa),
        daa_score: daa,
        blue_score: daa,
        subsidy: 0,
    }));
}
fn frontier(s: &PalwChainStateV2, p: &PalwStateParamsV2, now: u64) -> u64 {
    palw_gate_c_frontier_v1(s, p, palw_resolved_through_v1(s, p, now)).0
}
fn floor_of(s: &PalwChainStateV2) -> u64 {
    palw_certified_frontier_floor_v1(s).unwrap().blue_score
}

#[test]
fn rfc23s1_floor_horizons_late_conviction_and_restore_as_one_set() {
    let extras = PalwTransitionExtrasV1::default();
    // ---- below stage 1: B's retirement raises the floor PAST the open A (the F05 hole) ----
    let p0 = params(false);
    let mut s = world();
    at(&mut s, FENCE + 5_000);
    let mut b = TransitionBuilder::new(&s, &p0, false, false, false, false, &extras);
    palw_security_certification_fold_v1::note_retirement_frontier_floor_v1(&mut b, &id(2), &s.claim(&id(2)).unwrap().clone()).unwrap();
    assert_eq!(floor_of(&b.state), 20, "below the fence the retirement floor passes the open claim at blue 10");
    assert!(frontier(&b.state, &p0, FENCE + 5_000) > palw_resolved_through_v1(&b.state, &p0, FENCE + 5_000), "the invariant breaks");

    // ---- stage 1: the floor never passes an open claim inside its bound ----
    let p = params(true);
    let mut s = world();
    at(&mut s, FENCE + 5_000);
    let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
    let retiring = s.claim(&id(2)).unwrap().clone();
    palw_security_certification_fold_v1::note_retirement_frontier_floor_v1(&mut b, &id(2), &retiring).unwrap();
    b.state.set_false_valid_rows_for_tests(id(2), None, None, None, 0); // B retires
    let mut s = b.state.clone();
    assert_eq!(floor_of(&s), 0, "the floor is not raised past A");
    let deferred: PalwCertifiedFrontierFloorV1 = s
        .kernel_route_state_v1()
        .unwrap()
        .aux_row(PALW_SECURITY_CERTIFICATION_TABLE_V1, &palw_security_certification_fold_v1::certification_deferred_floor_key_v1())
        .expect("the retirement is deferred");
    assert_eq!(deferred.blue_score, 20);
    let deferred_bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s)).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(deferred_bytes.as_slice(), deferred_bytes.len() as u64)
        .expect("the actual deferred-floor carriage passes native preflight");
    for now in [FENCE + 5_000, FENCE + 20_001] {
        assert!(frontier(&s, &p, now) <= palw_resolved_through_v1(&s, &p, now), "invariant: frontier inside the prefix at {now}");
        assert_eq!(frontier(&s, &p, now), 0, "A holds the frontier while within its bound");
    }
    // ---- A passes its bound: the prefix opens, the deferred floor is applied, the frontier reaches C ----
    let now = FENCE + 20_002;
    at(&mut s, now);
    let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
    let through = palw_resolved_through_v1(&b.state, &p, now);
    assert_eq!(through, u64::MAX);
    palw_security_certification_fold_v1::apply_deferred_frontier_floor_v1(&mut b, through).unwrap();
    let mut s = b.state.clone();
    assert_eq!(floor_of(&s), 20);
    assert_eq!(frontier(&s, &p, now), 30);
    assert_eq!(s.unresolved_first_blue_score_v1(), Some(10), "A keeps its liability: still open, pruning stops below it");
    // ---- a late conviction of C rolls the frontier back to the floor, not below ----
    let mut c = s.claim(&id(3)).unwrap().clone();
    c.phase = PalwClaimPhaseV2::Voided { voided_daa: now, reason: PalwVoidReasonV2::BindTimeout };
    s.set_false_valid_rows_for_tests(id(3), Some(c), None, None, 0);
    assert_eq!(frontier(&s, &p, now), 20, "rollback to the floor");
    // ---- restore: the carriage a pruned / IBD node rebuilds from carries every input of the rule — the open claim (the index is
    // re-derived from the claims), the floor and the deferred floor rows — so the rebuilt prefix, floor and frontier are the same. (A
    // full `into_state_v3` of this synthetic state is refused by the carriage's own unrelated reservation consistency; the node
    // replay of stage 1 is not run yet.)
    let carriage = PalwStateCarriageV2::from_state(&s);
    let bytes = borsh::to_vec(&carriage).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(bytes.as_slice(), bytes.len() as u64)
        .expect("the actual applied-floor carriage passes native preflight");
    let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, carriage);
    assert!(back.claims.contains_key(&id(1)), "the open claim travels");
    let route = back.kernel_route.as_ref().expect("the route travels");
    for key in [certification_frontier_floor_key_v1()] {
        assert!(route.aux.contains_key(&(PALW_SECURITY_CERTIFICATION_TABLE_V1, key)), "the floor row travels");
    }
}
