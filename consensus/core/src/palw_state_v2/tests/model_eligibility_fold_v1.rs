//! **RFC-0017 through the actual fold** (`palw_model_eligibility_fold_v1`): the claim gate on every lane it serves, commit-then-draw on
//! the chain's own `round_seed_anchor`, the attempt fee burned at the commit, k-of-m promotion, attestation absent ⇒ no promotion and
//! no slash, lapse and renewal, re-registration of the same weights keeping the attempt history, and the dormant fence (A-2).
//!
//! Unverified source (2026-10-15): written, not built and not run. Every policy here is a TEST value
//! ([`PalwModelEligibilityPolicyV1::design_unapproved_v1`] at a 10-DAA day, fee 10 sompi), not a proposal.
use super::*;
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use crate::palw_model_eligibility_v1::*;
use misaka_palw_kernel::model_eligibility_v1::{
    ELIGIBILITY_ATTESTATION_VERSION_V1, ELIGIBILITY_EXAM_COMMIT_VERSION_V1, EligibilityAttestationV1, EligibilityExamCommitV1,
    EligibilityExamPurposeV1, EligibilityVerdictV1,
};
use misaka_palw_kernel::route::KernelRouteObjectV1;

/// A 10-DAA day: probation 70, maturity 70, attestation window 20, renewal 300, grace 20, cooldown 30, attempt epoch 300.
const DAY: u64 = 10;
const FENCE: u64 = 50;
const FEE: u64 = 10;
const ENTRANT: u64 = 2;
const RE_REGISTERED: u64 = 3;
const BONDS: u64 = 8;

fn test_policy() -> PalwModelEligibilityPolicyV1 {
    PalwModelEligibilityPolicyV1 { attempt_fee_sompi: FEE, ..PalwModelEligibilityPolicyV1::design_unapproved_v1(DAY) }
}

fn armed() -> PalwStateParamsV2 {
    params().with_model_eligibility_v1(Some(PalwModelEligibilityMirrorV1 { from_daa: FENCE, policy: test_policy() }))
}

fn extras() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: h64(10),
            ruleset_digest: h64(11),
            chain_genesis: h64(12),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    apply_palw_transition_v2_with_extras(parent, p, &ctx(daa, daa, daa), objects, None, false, false, false, false, &extras())
        .map(|(state, _)| state)
}

fn route(signer: u64, object: KernelRouteObjectV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::KernelRouteV1 { bytes: object.encode(), signer: bond_key(signer), signature: Vec::new() }
}

fn kid(bond: u64) -> [u8; 64] {
    palw_kernel_bond_id_v1(&bond_key(bond))
}

fn bond_of_kid(kernel_id: &[u8; 64]) -> u64 {
    (1..=BONDS).find(|b| kid(*b) == *kernel_id).expect("a drawn bond is one of the fixture's")
}

fn commit(class: u64, purpose: EligibilityExamPurposeV1, claims: u32, attempt: u32, committer: u64) -> EligibilityExamCommitV1 {
    EligibilityExamCommitV1 {
        version: ELIGIBILITY_EXAM_COMMIT_VERSION_V1,
        network: h64(10).as_bytes(),
        committer: kid(committer),
        class_id: h64(class).as_bytes(),
        purpose,
        claims_root: h64(0xC1).as_bytes(),
        claims_count: claims,
        attempt_index: attempt,
    }
}

fn attest(exam: [u8; 64], verifier: u64, verdict: EligibilityVerdictV1) -> PalwConsensusObjectV2 {
    route(
        verifier,
        KernelRouteObjectV1::AttestEligibilityExamV1 {
            attestation: EligibilityAttestationV1 {
                version: ELIGIBILITY_ATTESTATION_VERSION_V1,
                network: h64(10).as_bytes(),
                exam,
                verifier: kid(verifier),
                verdict,
                transcript: [0x7E; 64],
                claim_mismatches: Vec::new(),
            },
        },
    )
}

fn row<T: borsh::BorshDeserialize>(state: &PalwChainStateV2, key: &[u8]) -> Option<T> {
    state.kernel_route.as_ref()?.aux_row::<T>(PALW_MODEL_ELIGIBILITY_TABLE_V1, key)
}

fn model(state: &PalwChainStateV2, class: u64) -> PalwEligibilityModelRowV1 {
    row(state, &eligibility_model_key_v1(&h64(class).as_bytes())).expect("the model's eligibility row")
}

fn exam(state: &PalwChainStateV2, id: &[u8; 64]) -> PalwEligibilityExamRowV1 {
    row(state, &eligibility_exam_key_v1(id)).expect("the exam row")
}

/// The genesis floor class, eight bonds of distinct operators (collateral 1,000 each), the entrant class
/// registered PAST the fence by bond 1, and a second registration of the SAME weights (artifact root) by bond 2 — and the kernel route
/// header the fold creates on first use, with the entrant's probation history (a `Probation` row since `FENCE + 10`) as the probation
/// lane writes it.
fn world(p: &PalwStateParamsV2) -> PalwChainStateV2 {
    let mut objects = vec![registration(h64(1), 1000, None)];
    for b in 1..=BONDS {
        objects.push(bond_registration_with_collateral(b, b as u8, 100 + b, 1_000));
    }
    let s = step(&PalwChainStateV2::genesis(), p, 1, &objects).expect("the floor and the bonds register");
    let mut s = step(
        &s,
        p,
        FENCE + 10,
        &[registration(h64(ENTRANT), 1, Some(bond_key(1))), registration(h64(RE_REGISTERED), 1, Some(bond_key(2)))],
    )
    .expect("the entrants register past the fence (registration is free and immutable)");
    s.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(10), h64(11)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: FENCE + 10, burned: 0 },
    ));
    for class in [ENTRANT, RE_REGISTERED] {
        let view =
            crate::palw_state_v2::palw_model_eligibility_fold_v1::eligibility_class_view_v1(&s, &h64(class)).expect("registered");
        let probation =
            palw_eligibility_new_probation_row_v1(h64(class).as_bytes(), view.lineage, view.principal, FENCE + 10, FENCE + 10);
        s.kernel_route.as_mut().unwrap().aux.insert(
            (PALW_MODEL_ELIGIBILITY_TABLE_V1, eligibility_model_key_v1(&h64(class).as_bytes())),
            borsh::to_vec(&probation).unwrap(),
        );
    }
    s
}

fn anchored(mut s: PalwChainStateV2, span: u64) -> PalwChainStateV2 {
    s.round_seed_anchor =
        Some(crate::palw_execution_lane_v1::PalwExecSeedAnchorV1 { span, block: h64(0xB0 + span), execution_key: h64(0xE0 + span) });
    s
}

/// The promotion exam committed at `PROMOTE` by bond 1 against an anchor at span 7, then drawn on span 8: the state after the draw.
const PROMOTE: u64 = FENCE + 10 + 7 * DAY;

fn committed_and_drawn(p: &PalwStateParamsV2) -> (PalwChainStateV2, [u8; 64]) {
    let s = anchored(world(p), 7);
    let c = commit(ENTRANT, EligibilityExamPurposeV1::Promotion, 16, 0, 1);
    let s = step(&s, p, PROMOTE, &[route(1, KernelRouteObjectV1::CommitEligibilityExamV1 { commit: c })]).expect("the commit lands");
    let id = c.id();
    // The anchor that existed at the commit never draws it.
    let s = step(&s, p, PROMOTE + 1, &[]).expect("a block");
    assert!(exam(&s, &id).anchor.is_none(), "the anchor recorded at the commit is not strictly later");
    // A later anchor does.
    let s = step(&anchored(s, 8), p, PROMOTE + 2, &[]).expect("a block");
    (s, id)
}

// =================================================================================================================================

/// **A-2**: with the fence absent the gate passes every lane and an exam object is refused; scheduled ahead, nothing moves either.
#[test]
fn below_the_fence_every_lane_keeps_the_old_rule_and_exam_objects_are_refused() {
    for p in
        [params(), params().with_model_eligibility_v1(Some(PalwModelEligibilityMirrorV1 { from_daa: 1 << 40, policy: test_policy() }))]
    {
        let s = world(&p);
        let x = extras();
        let read = PalwFoldReadV1::outside(&s, &p, &x);
        for lane in [PalwGatedClaimV1::Attempt, PalwGatedClaimV1::FreePrompt { quanta: 1 }, PalwGatedClaimV1::Evaluation] {
            assert!(read.check_class_admits_claim(&h64(ENTRANT), PROMOTE, lane).is_ok(), "old rule below the fence: {lane:?}");
        }
        let c = commit(ENTRANT, EligibilityExamPurposeV1::Promotion, 16, 0, 1);
        assert!(step(&s, &p, PROMOTE, &[route(1, KernelRouteObjectV1::CommitEligibilityExamV1 { commit: c })]).is_err());
        let status = palw_eligibility_status_of_v1(&s, &p, &h64(ENTRANT), PROMOTE);
        assert_eq!(status.stage, "not-governed");
        assert!(status.subsidy && status.block_right && status.final_weight);
    }
}

/// **REQ-ELIG-1 / GAP-E4, the per-path gates**: past the fence a class with no row and no registry admission is `Registered` and
/// takes no attempt, no free-prompt commitment and no evaluation claim; the genesis floor class is untouched.
#[test]
fn a_registered_class_is_refused_on_the_attempt_free_prompt_and_evaluation_lanes_and_genesis_is_not() {
    let p = armed();
    let mut s = world(&p);
    // Strip the entrant's probation row: it is a bare registration.
    s.kernel_route
        .as_mut()
        .unwrap()
        .aux
        .remove(&(PALW_MODEL_ELIGIBILITY_TABLE_V1, eligibility_model_key_v1(&h64(ENTRANT).as_bytes())));
    let x = extras();
    let read = PalwFoldReadV1::outside(&s, &p, &x);
    for lane in [PalwGatedClaimV1::Attempt, PalwGatedClaimV1::FreePrompt { quanta: 1 }, PalwGatedClaimV1::Evaluation] {
        match read.check_class_admits_claim(&h64(ENTRANT), PROMOTE, lane) {
            Err(PalwStateV2Error::ClassNotRewardable { code, .. }) => {
                assert_eq!(
                    code,
                    crate::palw_state_v2::palw_model_eligibility_fold_v1::PALW_ELIGIBILITY_NOT_ELIGIBLE_CODE_V1,
                    "{lane:?}"
                )
            }
            other => panic!("{lane:?}: a Registered class must be refused by RFC-0017, got {other:?}"),
        }
        assert!(read.check_class_admits_claim(&h64(1), PROMOTE, lane).is_ok(), "the genesis floor keeps the old rule: {lane:?}");
    }
    let status = palw_eligibility_status_of_v1(&s, &p, &h64(ENTRANT), PROMOTE);
    assert_eq!((status.stage.as_str(), status.subsidy, status.block_right, status.final_weight), ("registered", false, false, false));
    // A Probation class passes the gate (its bound is the probation grant at the budget plan, not a refusal here).
    let s = world(&p);
    let read = PalwFoldReadV1::outside(&s, &p, &x);
    assert!(read.check_class_admits_claim(&h64(ENTRANT), PROMOTE, PalwGatedClaimV1::Attempt).is_ok());
    assert!(!palw_eligibility_status_of_v1(&s, &p, &h64(ENTRANT), PROMOTE).final_weight, "probation: no Final weight");
}

/// **REQ-ELIG-3/4 + GAP-E3**: the commit counts the attempt and burns the fee before any randomness; the draw uses the first anchor
/// strictly after the commit, over the population cut at the commit (the committer's operator excluded); k Reproduced positions
/// promote, and the model's next renewal is one epoch out.
#[test]
fn commit_then_draw_on_the_real_anchor_promotes_on_k_of_m() {
    let p = armed();
    let (s, id) = committed_and_drawn(&p);
    let e = exam(&s, &id);
    assert_eq!(e.anchor.map(|(span, _)| span), Some(8));
    assert_eq!(e.draws.len(), test_policy().exam_m as usize);
    assert!(e.draws.iter().all(|d| *d != kid(1)), "the committer's own operator is not in the population");
    assert_eq!(e.examined.len(), test_policy().exam_claims as usize);
    assert_eq!(e.deadline_daa, PROMOTE + 2 + test_policy().exam_attest_window_daa);
    // The fee: burned from bond 1 (collateral down, `slashed` up), recorded on the lineage, the principal and the network.
    assert_eq!(s.bond(&bond_key(1)).unwrap().collateral, 1_000 - FEE);
    let lineage: PalwEligibilityLineageRowV1 =
        row(&s, &eligibility_lineage_key_v1(&model(&s, ENTRANT).lineage)).expect("the lineage row");
    assert_eq!((lineage.attempts_total, lineage.fees_burned_sompi), (1, FEE as u128));
    assert_eq!(model(&s, ENTRANT).open_exam, Some(id));
    // Every distinct drawn bond attests Reproduced.
    let mut drawn: Vec<u64> = e.draws.iter().map(bond_of_kid).collect();
    drawn.sort_unstable();
    drawn.dedup();
    let objects: Vec<_> = drawn.iter().map(|b| attest(id, *b, EligibilityVerdictV1::Reproduced)).collect();
    let s = step(&s, &p, PROMOTE + 3, &objects).expect("the attestations land");
    let m = model(&s, ENTRANT);
    assert_eq!(m.stage, PalwEligibilityStageV1::Eligible);
    assert_eq!(m.next_renewal_daa, PROMOTE + 3 + test_policy().renewal_epoch_daa);
    assert_eq!(m.open_exam, None);
    assert!(matches!(exam(&s, &id).outcome, Some((PalwEligibilityExamOutcomeV1::Passed, _))));
    let x = extras();
    assert!(
        PalwFoldReadV1::outside(&s, &p, &x).check_class_admits_claim(&h64(ENTRANT), PROMOTE + 3, PalwGatedClaimV1::Attempt).is_ok()
    );
    // An undrawn bond's attestation is refused.
    let undrawn = (2..BONDS).find(|b| !drawn.contains(b));
    if let Some(b) = undrawn {
        assert!(step(&s, &p, PROMOTE + 4, &[attest(id, b, EligibilityVerdictV1::Reproduced)]).is_err());
    }
}

/// **§4C / §7-7: attestation absent ⇒ no promotion, and nobody is slashed** — the drawn verifiers stay silent (or report the model
/// unobtainable); at the deadline the exam is `NotPromoted`, the class stays `Probation`, and the only collateral that moved is the
/// committer's fee.
#[test]
fn without_k_attestations_the_exam_is_not_promoted_and_nothing_is_slashed() {
    let p = armed();
    let (s, id) = committed_and_drawn(&p);
    let e = exam(&s, &id);
    let first = bond_of_kid(&e.draws[0]);
    let s = step(&s, &p, PROMOTE + 3, &[attest(id, first, EligibilityVerdictV1::Unavailable)]).expect("an Unavailable attestation");
    let before: Vec<(u64, u64)> =
        (2..=BONDS).map(|b| (s.bond(&bond_key(b)).unwrap().collateral, s.bond(&bond_key(b)).unwrap().slashed)).collect();
    let s = step(&s, &p, e.deadline_daa + 1, &[]).expect("the deadline passes");
    assert!(matches!(exam(&s, &id).outcome, Some((PalwEligibilityExamOutcomeV1::NotPromoted, _))));
    assert_eq!(model(&s, ENTRANT).stage, PalwEligibilityStageV1::Probation);
    let after: Vec<(u64, u64)> =
        (2..=BONDS).map(|b| (s.bond(&bond_key(b)).unwrap().collateral, s.bond(&bond_key(b)).unwrap().slashed)).collect();
    assert_eq!(before, after, "no verifier and no producer is slashed for an exam that did not pass");
    assert_eq!(s.bond(&bond_key(1)).unwrap().slashed, FEE, "the committer's burned fee is the only bond write");
}

/// **§7-6: withdraw / re-register after seeing the draw** — a replay of the spent index is refused, and the same weights registered
/// under another class id share the lineage: its first commit must name the NEXT index; the cooldown runs across both.
#[test]
fn re_registering_the_same_weights_keeps_the_attempt_history() {
    let p = armed();
    let (s, _) = committed_and_drawn(&p);
    let same =
        crate::palw_state_v2::palw_model_eligibility_fold_v1::eligibility_class_view_v1(&s, &h64(RE_REGISTERED)).unwrap().lineage;
    assert_eq!(same, model(&s, ENTRANT).lineage, "one artifact root, one lineage");
    let later = PROMOTE + test_policy().attempt_cooldown_daa + 3;
    let replay = commit(RE_REGISTERED, EligibilityExamPurposeV1::Promotion, 16, 0, 2);
    assert!(step(&s, &p, later, &[route(2, KernelRouteObjectV1::CommitEligibilityExamV1 { commit: replay })]).is_err());
    let inside_cooldown = commit(RE_REGISTERED, EligibilityExamPurposeV1::Promotion, 16, 1, 2);
    assert!(step(&s, &p, PROMOTE + 3, &[route(2, KernelRouteObjectV1::CommitEligibilityExamV1 { commit: inside_cooldown })]).is_err());
    let next = commit(RE_REGISTERED, EligibilityExamPurposeV1::Promotion, 16, 1, 2);
    let s = step(&s, &p, later, &[route(2, KernelRouteObjectV1::CommitEligibilityExamV1 { commit: next })]).expect("the next index");
    let lineage: PalwEligibilityLineageRowV1 = row(&s, &eligibility_lineage_key_v1(&same)).unwrap();
    assert_eq!((lineage.attempts_total, lineage.fees_burned_sompi), (2, 2 * FEE as u128));
}

/// **GAP-E3**: a bond whose free collateral cannot pay the fee commits nothing; a genesis class takes no exam.
#[test]
fn an_unaffordable_fee_or_a_genesis_class_commits_nothing() {
    // A fee above every bond's whole collateral (1,000): no bond can pay it.
    let dear = params().with_model_eligibility_v1(Some(PalwModelEligibilityMirrorV1 {
        from_daa: FENCE,
        policy: PalwModelEligibilityPolicyV1 { attempt_fee_sompi: 5_000, ..test_policy() },
    }));
    let s = anchored(world(&dear), 7);
    let poor = commit(ENTRANT, EligibilityExamPurposeV1::Promotion, 16, 0, BONDS);
    assert!(step(&s, &dear, PROMOTE, &[route(BONDS, KernelRouteObjectV1::CommitEligibilityExamV1 { commit: poor })]).is_err());
    let p = armed();
    let s = anchored(world(&p), 7);
    let genesis = commit(1, EligibilityExamPurposeV1::Promotion, 16, 0, 1);
    assert!(step(&s, &p, PROMOTE, &[route(1, KernelRouteObjectV1::CommitEligibilityExamV1 { commit: genesis })]).is_err());
}

/// **REQ-ELIG-5 / §7-5: a pass is never permanent** — with no renewal the sweep lapses the class one renewal epoch plus the grace
/// after its promotion, the claim gate refuses it again, and a renewal exam may then be committed for the lapsed class.
#[test]
fn an_eligible_class_lapses_without_renewal_and_may_renew() {
    let p = armed();
    let (s, id) = committed_and_drawn(&p);
    let mut drawn: Vec<u64> = exam(&s, &id).draws.iter().map(bond_of_kid).collect();
    drawn.sort_unstable();
    drawn.dedup();
    let s = step(&s, &p, PROMOTE + 3, &drawn.iter().map(|b| attest(id, *b, EligibilityVerdictV1::Reproduced)).collect::<Vec<_>>())
        .expect("promoted");
    let lapse_at = model(&s, ENTRANT).next_renewal_daa + test_policy().lapse_grace_daa + 1;
    let s = step(&s, &p, lapse_at, &[]).expect("the sweep runs");
    assert_eq!(model(&s, ENTRANT).stage, PalwEligibilityStageV1::Lapsed);
    let x = extras();
    assert!(PalwFoldReadV1::outside(&s, &p, &x).check_class_admits_claim(&h64(ENTRANT), lapse_at, PalwGatedClaimV1::Attempt).is_err());
    let renew = commit(ENTRANT, EligibilityExamPurposeV1::Renewal, test_policy().exam_claims as u32, 1, 3);
    let s = anchored(s, 20);
    step(&s, &p, lapse_at + 1, &[route(3, KernelRouteObjectV1::CommitEligibilityExamV1 { commit: renew })]).expect("a renewal commit");
}
