//! **ADR-0069 Decision 7 at the V2 state fold — the site the LIVE fork choice reads.**
//!
//! The V1 resolver (`palw_facts::resolve_block_weight_v1`) prices a block for the heap the
//! processor orders tips with, and on a `ConsensusV2` network that heap is a SEARCH ORDER. The
//! authority is this fold: `PalwChainStateV2::candidate_order` hands `safe_weight` and
//! `bounded_immature` to `PalwCandidateOrderV1::new`, and all three scalars are hashed into
//! `state_root`. Decision 7 has to hold here or it does not hold on the network that matters.
//!
//! Everything below drives the REAL transition — the same function block validation folds — with
//! two classes registered: the liveness floor at the whole cadence, and an entrant that holds no
//! cadence, which is ADR-0069 Decision 5's "admissible for liveness, weightless" and exactly the
//! state a permissionless uncertified registration lands in. The entrant appears in both shapes
//! the fold really has — `Active` at share 0, and `Registered` with its activation edge still
//! ahead — because only the second one can cross the rule while a claim of its is alive, and that
//! crossing is where the accounting either holds together or does not.
//!
//! Run: `MISAKA_PALW_POW_FIXTURE=1 cargo test -p kaspa-consensus-core --test palw_adr0069_d7_fold`

use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwPanelSeatV2,
    PalwPwuRuleV2, PalwStateParamsV2, apply_palw_transition_v2_with_policies, palw_operator_id_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

const BASE: u64 = 1;
const ENTRANT: u64 = 2;
/// β. The immature contribution of a `pwu`-40 claim is ⌊40·100/1000⌋ = 4.
const BETA: u16 = 100;
const PWU: u64 = 40;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}
fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}
fn ctx(word: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(word), daa_score: daa, blue_score: blue, subsidy: 0 }
}

/// Short windows, so one test walks a whole lattice: bind 10, receipt 10, challenge 20.
fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(BETA, 10, 10, 20, 500, 1_000, h(BASE), 4, 1_000, 100, 1_000, 0).expect("state params")
}

fn registration(class: u64, share: u16) -> Obj {
    registration_at(class, share, 0)
}

/// A registration whose activation edge is in the FUTURE. Until `activation_daa` the class is
/// `Registered`: in the registry, adjudicable, refusing nothing, and holding **no row at all** in
/// the share table — so it is weightless by ADR-0069 Decision 7's predicate, and it becomes
/// share-bearing later without anybody voting on it. That is the one transition the fold really
/// has that moves a class across the rule, and every test below that needs the move uses it.
fn registration_at(class: u64, share: u16, activation_daa: u64) -> Obj {
    Obj::ClassRegistered {
        class_id: h(class),
        artifact_root: h(11),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
        initial_target: u128::MAX / 2,
        share_permille: share,
        activation_daa,
        admission: None,
    }
}

fn bond() -> Obj {
    Obj::BondRegistered {
        bond: bond_key(1),
        pubkey: vec![7; 4],
        operator_pubkey: vec![21; 8],
        collateral: 1_000,
        payout_payload: Hash64::from_u64_word(0x9A11),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

fn attempt(class: u64, pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(999),
            challenge: h(nonce ^ 0xC0FF_EE),
            class_id: h(class),
            executor_bond: bond_key(1).0,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&[21u8; 8]),
            artifact_root: h(11),
            trace_root: h(nonce ^ 0x31),
            output_root: h(nonce ^ 0x32),
            pwu,
            trace_manifest_root: h(nonce ^ 0x33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: h(nonce ^ 0x41),
        },
        signature: vec![0; 8],
    }
}

fn seat_receipt(claim: Hash64) -> Vec<PalwSeatReceiptV2> {
    vec![PalwSeatReceiptV2 {
        claim,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(1),
        signed_daa: 0,
        signature: Vec::new(),
    }]
}

/// One fold step, with BOTH policies, and both consistency checkers run afterwards — so no test
/// here can quietly leave a state whose accumulators and claims disagree.
fn step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    c: &PalwBlockContextV2,
    objects: &[Obj],
    att: Option<&PalwAttemptEnvelopeV2>,
    weightless: bool,
) -> PalwChainStateV2 {
    let (state, _) = apply_palw_transition_v2_with_policies(parent, p, c, objects, att, false, weightless)
        .unwrap_or_else(|e| panic!("the transition at daa {} must apply: {e}", c.daa_score));
    state
        .assert_internal_consistency_v2(p, weightless)
        .unwrap_or_else(|e| panic!("internal consistency after the block at daa {}: {e}", c.daa_score));
    state.assert_deadline_consistency(p).expect("deadline consistency");
    state
}

/// Register the floor and the entrant, then bond. One block, so every scenario starts identical.
fn genesis_with_two_classes(p: &PalwStateParamsV2, weightless: bool) -> PalwChainStateV2 {
    step(
        &PalwChainStateV2::genesis(),
        p,
        &ctx(1, 100, 1),
        // The floor takes the whole cadence; the entrant registers WEIGHTLESS, which
        // `granted_share_table_v2` admits as a real state and dilutes nobody for.
        &[registration(BASE, 1_000), registration(ENTRANT, 0), bond()],
        None,
        weightless,
    )
}

/// The same start, except the entrant's cadence is PENDING: it holds no row until `activation_daa`
/// and then takes `share` permille by donation from the floor.
fn genesis_with_pending_entrant(p: &PalwStateParamsV2, share: u16, activation_daa: u64, weightless: bool) -> PalwChainStateV2 {
    let g = step(
        &PalwChainStateV2::genesis(),
        p,
        &ctx(1, 100, 1),
        &[registration(BASE, 1_000), registration_at(ENTRANT, share, activation_daa), bond()],
        None,
        weightless,
    );
    assert_eq!(g.class_share_permille(&h(ENTRANT)), None, "a pending registration holds no permille — that IS the case here");
    g
}

/// Accept one attempt of `class`, then walk it Provisional → PanelBound → ReceiptLicensed →
/// Final. Returns `(state at each stop's end, the claim id)`.
fn walk_to_final(p: &PalwStateParamsV2, from: &PalwChainStateV2, class: u64, weightless: bool) -> (PalwChainStateV2, Hash64) {
    let env = attempt(class, PWU, class);
    let claim_id = attempt_id_v2(&env.attempt);
    let s2 = step(from, p, &ctx(2, 101, 2), &[], Some(&env), weightless);
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h(90) }];
    let s3 = step(&s2, p, &ctx(3, 102, 3), &[Obj::PanelBound { claim: claim_id, anchor: h(77), seats }], None, weightless);
    let s4 =
        step(&s3, p, &ctx(4, 103, 4), &[Obj::ReceiptLicensed { claim: claim_id, receipts: seat_receipt(claim_id) }], None, weightless);
    // Past the challenge window (103 + 20): the sweep matures it.
    let s5 = step(&s4, p, &ctx(5, 130, 5), &[], None, weightless);
    assert!(matches!(s5.claim(&claim_id).expect("the claim survives to Final").phase, PalwClaimPhaseV2::Final { .. }));
    (s5, claim_id)
}

/// **E1 at the fold — a share-0 class's block adds nothing to EITHER chain weight.**
///
/// Both scalars, and both at their own moment: `bounded_immature` is priced when the claim is
/// created, `safe_weight` when it finalizes. A rule that closed only one of them would leave the
/// fabricated block competing — through `live` if the immature half leaked, through `safe` (which
/// is what the IBD and deep-reorg gates read) if the mature half did.
#[test]
fn e1_a_share_zero_class_adds_nothing_to_safe_or_bounded_immature() {
    let p = params();
    let immature = (PWU as u128) * (BETA as u128) / 1000;
    assert_eq!(immature, 4, "the fixture must have a non-zero immature term or half this test is vacuous");

    for weightless in [false, true] {
        let g = genesis_with_two_classes(&p, weightless);

        // The floor bears weight in both worlds — Decision 7 must not touch a certified class.
        let env = attempt(BASE, PWU, BASE);
        let mid = step(&g, &p, &ctx(2, 101, 2), &[], Some(&env), weightless);
        assert_eq!(mid.bounded_immature(), immature, "the floor's immature weight, fence {weightless}");
        let (base_final, _) = walk_to_final(&p, &g, BASE, weightless);
        assert_eq!(base_final.safe_weight(), PWU as u128, "the floor's matured weight, fence {weightless}");
        assert_eq!(base_final.bounded_immature(), 0, "and it is released on maturity");

        // The entrant is the case.
        let env = attempt(ENTRANT, PWU, ENTRANT);
        let mid = step(&g, &p, &ctx(2, 101, 2), &[], Some(&env), weightless);
        let (entrant_final, claim_id) = walk_to_final(&p, &g, ENTRANT, weightless);
        if weightless {
            assert_eq!(mid.bounded_immature(), 0, "a share-0 class's live weight is zero the moment its claim exists");
            assert_eq!(entrant_final.safe_weight(), 0, "and its Final adds nothing to safe(C) — the whole of Decision 7");
        } else {
            assert_eq!(mid.bounded_immature(), immature, "below the fence the entrant is priced exactly as it is today");
            assert_eq!(entrant_final.safe_weight(), PWU as u128, "…including at Final");
        }
        // Whatever the fence says, fork choice reads a coherent pair.
        let order = entrant_final.candidate_order(h(0xFEED));
        assert!(order.live_total >= order.safe_weight, "live is safe plus bounded immature, always");
        assert_eq!(entrant_final.claim(&claim_id).expect("still held").pwu, PWU, "the CLAIM's pwu is untouched — only its weight");
    }
}

/// **E2 at the fold — the block is otherwise unchanged.**
///
/// Weightless is not banned. The entrant's attempt is still accepted, its claim still walks the
/// whole lattice to `Final`, its production still lands in the epoch counter the retarget and the
/// budget read, and the chain point still advances. Decision 7 prices weight; it takes nothing
/// else away, which is the line ADR-0069 Decision 5 drew and this is the arithmetic behind it.
#[test]
fn e2_the_weightless_block_still_produces_matures_and_counts() {
    let p = params();
    let armed = genesis_with_two_classes(&p, true);
    let dormant = genesis_with_two_classes(&p, false);

    let (a, claim_a) = walk_to_final(&p, &armed, ENTRANT, true);
    let (d, claim_d) = walk_to_final(&p, &dormant, ENTRANT, false);
    assert_eq!(claim_a, claim_d, "the same block produces the same claim either way");

    let ca = a.claim(&claim_a).expect("held");
    let cd = d.claim(&claim_d).expect("held");
    assert!(matches!(ca.phase, PalwClaimPhaseV2::Final { .. }), "it still matures");
    assert_eq!(ca.phase, cd.phase, "and matures at the same point");
    assert_eq!(ca.pwu, cd.pwu, "its claimed work is the same work");
    assert_eq!(ca.reserved, cd.reserved, "its collateral is the same collateral — weightless is not free");
    assert_eq!(ca.escrowed_reward, cd.escrowed_reward, "and it is paid the same budgeted subsidy");
    assert_eq!(a.last_point().map(|point| point.daa_score), d.last_point().map(|point| point.daa_score), "DAA advanced alike");
    assert_eq!(a.last_point().map(|point| point.blue_score), d.last_point().map(|point| point.blue_score));

    // The class still holds the row that makes it admissible, and still holds no cadence.
    assert_eq!(a.class_share_permille(&h(ENTRANT)), Some(0), "registered, weightless, and still in the table");

    // The ONE difference, stated as a difference: the two states are otherwise the same state.
    assert_eq!(a.safe_weight(), 0);
    assert_eq!(d.safe_weight(), PWU as u128);
    assert_ne!(a.state_root(), d.state_root(), "a fork-choice rule that moved no root would be a silent fork");
}

/// **The fence is what switches it, and dormant is byte-identical.**
///
/// The state root is the whole ruleset's commitment, so "no shipped preset moves" has to be an
/// assertion about ROOTS and not about the two scalars. Two chains folded identically except for
/// the flag, with every class bearing share, must produce the same root at every step — that is
/// the statement "arming this rule changes nothing for a network whose classes are all certified".
#[test]
fn the_fence_off_is_the_state_this_fold_already_computed() {
    let p = params();
    // Only the floor: every class on this chain bears weight, so Decision 7 has nothing to price
    // at zero and the two worlds must agree root-for-root.
    let one_class = |weightless: bool| {
        let g = step(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &[registration(BASE, 1_000), bond()], None, weightless);
        walk_to_final(&p, &g, BASE, weightless).0
    };
    assert_eq!(
        one_class(false).state_root(),
        one_class(true).state_root(),
        "with every class share-bearing the rule is the identity — arming it must move nothing"
    );

    // With a weightless class present, dormant must still be exactly what it was.
    let p2 = params();
    let dormant_root = walk_to_final(&p2, &genesis_with_two_classes(&p2, false), ENTRANT, false).0.state_root();
    let armed_root = walk_to_final(&p2, &genesis_with_two_classes(&p2, true), ENTRANT, true).0.state_root();
    assert_ne!(dormant_root, armed_root, "and armed, over a weightless class, it must move the root — it is a consensus change");
}

/// **E3 at the fold — a class granted cadence AFTER its claim matured gains nothing backwards.**
///
/// The grant here is the protocol's own and the only one the fold has for a class that held no
/// permille: `activate_due_classes` flips a `Registered` class at its activation edge and funds it
/// by donation from the incumbents. (ADR-0054's growth walk is NOT that path — the shipped
/// `derive_class_share_growth_v1` skips `share == 0` explicitly, "a weightless class does not
/// grow", so a class that is already Active at zero stays there. A test that waited for the growth
/// walk to hand an entrant 1‰ would wait forever and assert nothing, which is what this one did.)
///
/// What must not happen is the claim it already produced acquiring weight when the grant lands. A
/// retroactive gain would let a certification REORDER HISTORY: a chain that had been behind since
/// it was built would overtake one that had been ahead, with no block produced and no work done.
#[test]
fn e3_a_class_granted_share_mid_chain_gains_no_weight_backwards() {
    let p = params();
    // Cadence pending until DAA 400 — long after this claim has matured at 130.
    let g = genesis_with_pending_entrant(&p, 100, 400, true);

    // The entrant produces, and its claim matures — all of it weightless.
    let (matured, claim_id) = walk_to_final(&p, &g, ENTRANT, true);
    assert_eq!(matured.safe_weight(), 0, "weightless from the start");
    assert_eq!(matured.claim(&claim_id).expect("held").immature_contribution, 0, "priced at zero when the claim was created");
    let before = matured.safe_weight();

    // Walk the chain over the activation edge. Nothing is produced on the way: the grant is due by
    // height, so it lands on its own.
    let mut state = matured;
    let mut blue = 5u64;
    for daa in [200u64, 300, 401, 402] {
        blue += 1;
        state = step(&state, &p, &ctx(blue + 100, daa, blue), &[], None, true);
    }
    assert_eq!(state.class_share_permille(&h(ENTRANT)), Some(100), "the grant really landed — otherwise this test asserts nothing");
    assert_eq!(state.class_share_permille(&h(BASE)), Some(900), "funded by donation from the floor, and the table still sums to 1000");

    // The heart of it: whatever the grant did, the weight already accounted did not move.
    assert_eq!(state.safe_weight(), before, "a share grant may not reach back and pay a block that was produced weightless");
    assert_eq!(state.safe_weight(), 0);
    assert_eq!(state.claim(&claim_id).expect("still held").immature_contribution, 0, "and the frozen immature price stays frozen");
}

/// **The residual, pinned rather than hidden: the safe half is priced at the FINALIZING block.**
///
/// `bounded_immature` is priced once, at claim creation, and stored — so no later share change can
/// reach it. `safe_weight` is priced at the `Final` transition, because that is where the running
/// total moves, and the claim carries no frozen weight-bearing bit for the finalize to consult.
/// So a claim ACCEPTED while its class was weightless and FINALIZED after the class was granted
/// share IS paid, in full, and this test says so in numbers rather than in a caveat.
///
/// It is bounded: the window is one claim's lattice (bind + receipt + challenge), the grant is a
/// height the registration named in public blocks earlier, and the work was really done — this is
/// a class certifying while its own block is in flight, not a fabrication. Closing it needs the
/// decision frozen on the claim record, which changes the claim encoding and so needs a
/// `PALW_STATE_V2_VERSION` bump and a re-mint — out of scope for a fenced remediation, and much
/// worse if it were discovered by an operator instead of read here.
#[test]
fn known_limitation_the_safe_half_is_priced_when_the_claim_finalizes() {
    let p = params();
    // Cadence pending until DAA 110: after the attempt at 101, before the maturing sweep at 130.
    let g = genesis_with_pending_entrant(&p, 100, 110, true);

    let env = attempt(ENTRANT, PWU, ENTRANT);
    let claim_id = attempt_id_v2(&env.attempt);
    let accepted = step(&g, &p, &ctx(2, 101, 2), &[], Some(&env), true);
    assert_eq!(accepted.bounded_immature(), 0, "the immature half is frozen at acceptance and cannot be revised");
    assert_eq!(
        accepted.claim(&claim_id).expect("held").immature_contribution,
        0,
        "and it is frozen IN THE RECORD, which is what makes it un-revisable"
    );

    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h(90) }];
    let bound = step(&accepted, &p, &ctx(3, 102, 3), &[Obj::PanelBound { claim: claim_id, anchor: h(77), seats }], None, true);
    let licensed =
        step(&bound, &p, &ctx(4, 103, 4), &[Obj::ReceiptLicensed { claim: claim_id, receipts: seat_receipt(claim_id) }], None, true);
    // The activation edge, in its own block so the grant is in the table before the sweep runs
    // (the transition sweeps deadlines BEFORE it activates due classes, and that order is fixed).
    let activated = step(&licensed, &p, &ctx(5, 115, 5), &[], None, true);
    assert_eq!(activated.class_share_permille(&h(ENTRANT)), Some(100), "the class took cadence while its claim was in flight");

    let matured = step(&activated, &p, &ctx(6, 130, 6), &[], None, true);
    assert!(matches!(matured.claim(&claim_id).expect("held").phase, PalwClaimPhaseV2::Final { .. }));
    assert_eq!(
        matured.safe_weight(),
        PWU as u128,
        "THE RESIDUAL: accepted weightless, finalized certified, and paid the full pwu — the safe half is priced at Final"
    );
    assert_eq!(matured.bounded_immature(), 0, "while the immature half stayed at the zero it was frozen at");
}

/// **Retirement may not re-decide, or the two totals drift apart with nothing left to reconcile
/// them** — the failure this whole file exists to avoid, as an executable case.
///
/// `retire_claim` moves a `Final` attempt claim's contribution from "held" to
/// `retired_safe_weight` and drops the record. If it asked the share table whether the class bears
/// weight — the same question the finalize asked, at a LATER chain point — the answer could differ,
/// and the difference would be permanent: the claim is gone, so no later block can reconcile it.
/// Here the class is weightless at `Final` (`safe_weight += 0`) and share-bearing by the time the
/// claim retires, so a Decision-7-priced retirement would book 40 against a running total that
/// received 0, and `assert_internal_consistency_v2` — which `step` runs after every block — refuses
/// the state the fold has just built. That is a chain that stops on every node at once.
///
/// So the retirement stays `claim.pwu` in both fence positions, and the consistency identity
/// becomes an upper bound when the fence is armed. Both halves of that are asserted here.
#[test]
fn retiring_a_weightless_claim_after_its_class_certifies_keeps_the_state_consistent() {
    for weightless in [false, true] {
        let p = params().with_claim_retirement_daa(50).expect("retirement span");
        // Weightless at the attempt (101) and at the maturing sweep (130); share-bearing from 150;
        // retired at Final + 50 = 180.
        let g = genesis_with_pending_entrant(&p, 100, 150, weightless);
        let (matured, claim_id) = walk_to_final(&p, &g, ENTRANT, weightless);
        let expected_safe = if weightless { 0 } else { PWU as u128 };
        assert_eq!(matured.safe_weight(), expected_safe, "fence {weightless}");

        let activated = step(&matured, &p, &ctx(6, 155, 6), &[], None, weightless);
        assert_eq!(activated.class_share_permille(&h(ENTRANT)), Some(100), "the class certified after its claim matured");

        // The retirement sweep. `step` runs `assert_internal_consistency_v2`, so this line IS the
        // assertion — it panics if the fold has built a state its own re-derivation refuses.
        let retired = step(&activated, &p, &ctx(7, 185, 7), &[], None, weightless);
        assert!(retired.claim(&claim_id).is_none(), "the claim really retired — otherwise nothing above was exercised");
        assert_eq!(retired.safe_weight(), expected_safe, "retirement carries weight across, it does not create or destroy any");
    }
}

// -------------------------------------------------------------------------------------------
// The RETIREMENT — the one consensus-visible choice in this remediation that had no test.
// -------------------------------------------------------------------------------------------

/// **What `retire_claim` books for an attempt claim is `claim.pwu`, and nothing else may decide
/// it.**
///
/// `retired_safe_weight` is hashed into `state_root` (`PalwChainStateV2::state_root`) and
/// `claim_retirement_daa` is `WINDOW_COURT` = 3000 on the shipped RC ruleset, so retirements
/// really happen on a live chain and the number booked here is a fork. The obvious symmetry —
/// pricing the retirement through `palw_claim_safe_contribution_v2`, the same helper the finalize
/// uses — is the mutation this test exists to fail on, and until it existed that mutation left
/// every test in this file green: the retirement case above certifies its class BEFORE the
/// retirement, so both spellings answer 40 there and the two builds agree.
///
/// Here the entrant is `Active` at share 0 and stays there, so the two spellings disagree: the
/// shipped one books the claim's full `pwu` (the upper-bound meaning `retired_safe_weight` has —
/// "the MOST the claims this state no longer holds could have carried"), a Decision-7-priced one
/// books 0. Both states pass the consistency check, in both fence positions, and their
/// `state_root`s differ — which is exactly why the assertion has to be on the accumulator and not
/// on `safe_weight`.
#[test]
fn the_retirement_books_the_claims_pwu_and_never_re_asks_the_share_table() {
    for weightless in [false, true] {
        let p = params().with_claim_retirement_daa(50).expect("retirement span");
        let g = genesis_with_two_classes(&p, weightless);
        // Weightless at every point this claim exists: registered `Active` at 0‰ and never granted.
        let (matured, claim_id) = walk_to_final(&p, &g, ENTRANT, weightless);
        assert_eq!(matured.class_share_permille(&h(ENTRANT)), Some(0), "the entrant holds a row and no cadence, start to finish");
        assert_eq!(matured.retired_safe_weight(), 0, "nothing has retired yet");

        // Final at 130, retirement armed for 180.
        let retired = step(&matured, &p, &ctx(7, 185, 7), &[], None, weightless);
        assert!(retired.claim(&claim_id).is_none(), "the claim really retired — otherwise this test asserts nothing");
        assert_eq!(
            retired.retired_safe_weight(),
            PWU as u128,
            "fence {weightless}: the retirement books the claim's OWN pwu. Pricing it through \
             `palw_claim_safe_contribution_v2` would book 0 here and fork silently — the whole \
             reason the retirement does not re-decide"
        );
        assert_eq!(
            retired.safe_weight(),
            if weightless { 0 } else { PWU as u128 },
            "and the running total is untouched by the retirement in either fence position"
        );
        assert_eq!(
            retired.class_share_permille(&h(ENTRANT)),
            Some(0),
            "the share table never moved — the disagreement is the pricing"
        );
    }
}

// -------------------------------------------------------------------------------------------
// The FREE-PROMPT retirement — a node that refused its own durable tip.
// -------------------------------------------------------------------------------------------

/// `params()` with the free-prompt lane priced and a 50-DAA retirement span.
///
/// `MaxPerAttempt(160)` ÷ 8 quanta per canonical job = a 20-leaf quantum, so a 60-leaf job is
/// exactly 3 quanta of `pwu` 20 each — the same shape the in-crate fixture uses, chosen so the
/// per-quantum weight is a round number the assertions can name.
fn fp_params() -> PalwStateParamsV2 {
    params().with_fp_quanta(8, 64).expect("free-prompt price").with_claim_retirement_daa(50).expect("retirement span")
}

fn fp_commit(claim_word: u64, work_leaves: u64) -> Obj {
    Obj::FreePromptCommitted {
        claim: h(claim_word),
        class_id: h(BASE),
        bond: bond_key(1),
        executor_pubkey: vec![7; 4],
        work_leaves,
        prompt_token_ids_hash: h(0x7E00 ^ claim_word),
        decode_tokens_executed: 3,
        trace_root: h(41),
        output_root: h(42),
        execution_root: h(43),
        trace_chunk_count: 4,
        trace_retention_daa: 999_999,
    }
}

fn fp_spend(claim_word: u64, quantum_index: u32) -> kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3 {
    use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_V3_VERSION, PalwReceiptSpendUnsignedV3, spend_challenge_v3};
    let bond = bond_key(1).0;
    PalwReceiptSpendUnsignedV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: h(999),
        challenge: spend_challenge_v3(h(999), h(0xB0), 1_700, 7, h(claim_word), quantum_index, &bond),
        claim_id: h(claim_word),
        quantum_index,
        beacon_block: h(0xBEAC),
        producer_bond: bond,
        producer_pubkey: vec![7; 4],
    }
}

/// One fold step carrying a V3 work slot, WITHOUT running the consistency check — the check is
/// what these tests are about, so a helper that asserted it could not state the dormant case.
fn work_step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    c: &PalwBlockContextV2,
    objects: &[Obj],
    work: kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3<'_>,
    weightless: bool,
) -> PalwChainStateV2 {
    let (state, _, _) =
        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v6(parent, p, None, c, objects, work, &[], false, weightless)
            .unwrap_or_else(|e| panic!("the transition at daa {} must apply: {e}", c.daa_score));
    state.assert_deadline_consistency(p).expect("deadline consistency");
    state
}

/// Drive one free-prompt claim to `Final` and spend two of its three quanta.
///
/// Returns the state at DAA 131, holding `safe_weight` = 2 × 20 = 40 contributed entirely by
/// spends, with the claim's retirement armed for 174 (`Final` at 124 + 50).
fn fp_claim_with_two_spends(p: &PalwStateParamsV2, weightless: bool) -> (PalwChainStateV2, Hash64) {
    use kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3;
    let claim_id = h(0xFC);
    let g = step(&PalwChainStateV2::genesis(), p, &ctx(1, 100, 1), &[registration(BASE, 1_000), bond()], None, weightless);
    let s2 = step(&g, p, &ctx(2, 101, 2), &[fp_commit(0xFC, 60)], None, weightless);
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h(90) }];
    let s3 = step(&s2, p, &ctx(3, 102, 3), &[Obj::PanelBound { claim: claim_id, anchor: h(77), seats }], None, weightless);
    let s4 = step(&s3, p, &ctx(4, 103, 4), &[Obj::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None, weightless);
    let certified = step(&s4, p, &ctx(5, 124, 5), &[], None, weightless);
    assert!(matches!(certified.claim(&claim_id).expect("held").phase, PalwClaimPhaseV2::Final { .. }), "the fixture certifies");
    assert_eq!(certified.safe_weight(), 0, "certification licenses; it does not weigh");

    let spend0 = fp_spend(0xFC, 0);
    let s6 = work_step(&certified, p, &ctx(6, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&spend0), weightless);
    let spend2 = fp_spend(0xFC, 2);
    let s7 = work_step(&s6, p, &ctx(7, 131, 7), &[], PalwBlockWorkV3::ReceiptSpend(&spend2), weightless);
    assert_eq!(s7.safe_weight(), 40, "two quanta at pwu/quanta = 20 each — the weight the retirement must carry across");
    s7.assert_internal_consistency_v2(p, weightless).expect("consistent while the claim is still held");
    (s7, claim_id)
}

/// **ARMED: a spent free-prompt claim retires without stranding its weight.**
///
/// This is the network-kill. `apply_receipt_spend` adds `pwu / quanta` to `safe_weight` for each
/// spent quantum; `finalize_claim` arms the retirement for ANY source; the sweep then drops the
/// record. Booking only `Attempt` claims into `retired_safe_weight` left that spent weight in
/// `safe_weight` with nothing left to re-derive it, so `assert_internal_consistency_v2` refused
/// the state — and the fold does not run the check, so the block was accepted, the chain went on,
/// and the refusal arrived on the next RESTART: `DbPalwStateV2Store::load_tip` refusing the node's
/// own durable tip with `CarriageInconsistent`, and every peer importing that pruning-point
/// snapshot refused with it.
///
/// The free-prompt amount is exact, not a bound: that lane is not priced by Decision 7 at the
/// spend or in the re-derivation, so `per_quantum × |spent|` is the same expression on both sides
/// and no share table is asked at either point.
#[test]
fn a_retiring_free_prompt_claim_carries_its_spent_weight_across() {
    let p = fp_params();
    let (spent, claim_id) = fp_claim_with_two_spends(&p, true);

    // Retirement armed for Final (124) + 50 = 174.
    let retired = work_step(&spent, &p, &ctx(8, 180, 8), &[], kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::None, true);
    assert!(retired.claim(&claim_id).is_none(), "the claim really retired — otherwise this test asserts nothing");
    assert_eq!(retired.safe_weight(), 40, "the running total does not move at a retirement");
    assert_eq!(retired.retired_safe_weight(), 40, "…because the weight is carried across under a different name");
    retired
        .assert_internal_consistency_v2(&p, true)
        .expect("the state the fold just built must be one its own re-derivation accepts — this is the whole finding");
}

/// **DORMANT: the same retirement still strands the weight, and that is the exposure this fence
/// leaves open.**
///
/// Recorded as an executable fact rather than a caveat. `retired_safe_weight` is hashed into
/// `state_root`, so booking the spent weight below the fence would move the root of any block
/// that retires a spent free-prompt claim — a fork against the chain that is already running.
/// The repair therefore rides `Params::palw_uncertified_weightless`, which is `None` on every
/// shipped preset including `palw_rc_shipped_params()`.
///
/// So on a network running the shipped RC ruleset (`claim_retirement_daa = WINDOW_COURT = 3000`),
/// the first free-prompt claim that spends a quantum and then retires produces a durable tip its
/// own node refuses on the next restart. If this test ever starts passing with `Ok`, the fence has
/// been armed or the repair has been unfenced, and either way this test is the one to read first.
#[test]
fn known_limitation_dormant_a_spent_free_prompt_retirement_strands_its_weight() {
    let p = fp_params();
    let (spent, claim_id) = fp_claim_with_two_spends(&p, false);

    let retired = work_step(&spent, &p, &ctx(8, 180, 8), &[], kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::None, false);
    assert!(retired.claim(&claim_id).is_none(), "the claim retires below the fence exactly as it does above it");
    assert_eq!(retired.safe_weight(), 40, "the spends' weight is still in the running total…");
    assert_eq!(retired.retired_safe_weight(), 0, "…and nothing books it, because only `Attempt` claims are folded here");

    let refused = retired.assert_internal_consistency_v2(&p, false).expect_err(
        "dormant, the node's own tip is inconsistent — if this is Ok the fence moved and the exposure note above is stale",
    );
    assert!(
        matches!(refused, kaspa_consensus_core::palw_state_v2::PalwStateV2Error::CarriageInconsistent(ref m) if m.contains("safe_weight 40")),
        "the refusal names the stranded total: {refused}"
    );
}

/// **The reachability claim above, measured on the ruleset testnet-11 actually runs, so it is
/// re-runnable rather than asserted.**
///
/// The remediation report said the retirement path was unreachable "because `CLAIM_RETIREMENT`
/// is 0". It is 0 on `PalwStateParamsV2::new`, which is what the report read; it is
/// `WINDOW_COURT` = 3000 on `palw_rc_shipped_params()`, which is what the network runs. Every
/// gate between a live chain and
/// `known_limitation_dormant_a_spent_free_prompt_retirement_strands_its_weight` is checked here,
/// so if any of them closes later this test says which one.
#[test]
fn the_shipped_rc_ruleset_can_reach_the_dormant_free_prompt_retirement() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let p = kaspa_consensus_core::config::params::palw_rc_shipped_params();
    assert!(
        p.palw_uncertified_weightless.is_none(),
        "the repair is dormant on the shipped RC — if this ever fails, the exposure below is closed and the note can go"
    );
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else {
        panic!("the shipped RC is a ConsensusV2 network");
    };
    assert_eq!(bundle.state.claim_retirement_daa(), 3_000, "terminal claims really are swept — the report read `new`'s zero instead");
    assert_ne!(
        bundle.state.fp_quanta_per_canonical_job(),
        0,
        "the free-prompt lane is priced, so a commitment is not `FreePromptLaneUnpriced`"
    );
    let certified = bundle.state.fp_certified_classes().expect("a shipped preset always carries the drilled free-prompt set");
    assert!(
        !certified.is_empty(),
        "at least one class may take a free-prompt claim — with an empty set the commitment arm refuses every one and the \
         stranding is unreachable on this network"
    );
}

/// **Arming the fence does not REPAIR a chain that already stranded weight while it was dormant —
/// measured, in every fence position, on the state a dormant retirement actually leaves.**
///
/// This is the whole reason `Params::validate_palw_v2` now refuses a SCHEDULED arming of
/// `palw_uncertified_weightless` (genesis, or not at all). A rolling activation reads like a
/// remedy — "arm it, it closes a live network-kill" — and it is not one:
///
///  * the stranded weight sits in `safe_weight`, and the claim that would re-derive it is gone,
///    so there is nothing left for a repair to compute from;
///  * `retired_safe_weight` was never credited, so `safe_ceiling` (which adds it to the live
///    claims' sum) is *below* `safe_weight` — the armed `<=` bound refuses for the same reason
///    the dormant equality does, not for a weaker one;
///  * and crossing the activation height does not heal it either: the fold carries both scalars
///    forward unchanged, so every state after the activation is refused exactly as the one
///    before it was.
///
/// A node whose durable tip is such a state cannot start (`load_tip` → `CarriageInconsistent`)
/// and therefore never reaches the activation height at all, which is why "schedule it and the
/// rule fixes itself at height H" is not available. What repairs the chain is a state below the
/// fence being written differently, and that moves roots already committed — i.e. a re-mint. The
/// gate in `validate_palw_v2` is that sentence made unignorable.
#[test]
fn arming_the_fence_does_not_repair_a_chain_that_already_stranded_weight() {
    use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwStateV2Error};
    let p = fp_params();
    let (spent, claim_id) = fp_claim_with_two_spends(&p, false);

    // The retirement happens BELOW the fence — the chain that is already running.
    let stranded = work_step(&spent, &p, &ctx(8, 180, 8), &[], PalwBlockWorkV3::None, false);
    assert!(stranded.claim(&claim_id).is_none(), "the claim retired — otherwise this test asserts nothing");
    assert_eq!(stranded.safe_weight(), 40, "the spends' weight is stranded in the running total…");
    assert_eq!(stranded.retired_safe_weight(), 0, "…with nothing booked to re-derive it from");

    // Position 1 — dormant, the equality. Refused (this is the known limitation).
    stranded.assert_internal_consistency_v2(&p, false).expect_err("dormant refuses the stranded state");

    // Position 2 — the SAME state re-derived under the armed rule, which is what an activation
    // height would do to it. Still refused, and by the same 40.
    let armed = stranded
        .assert_internal_consistency_v2(&p, true)
        .expect_err("arming is not a repair: the armed bound refuses the stranded state too");
    assert!(
        matches!(armed, PalwStateV2Error::CarriageInconsistent(ref m)
            if m.contains("safe_weight 40") && m.contains("retired total 0")),
        "the armed refusal names the same stranded total the dormant one does: {armed}"
    );

    // Position 3 — carry the chain PAST the activation with the rule in force. The fold does not
    // re-derive either scalar, so the state stays refused for the rest of the chain's life.
    let past_activation = work_step(&stranded, &p, &ctx(9, 181, 9), &[], PalwBlockWorkV3::None, true);
    assert_eq!(past_activation.safe_weight(), 40, "the fold carries the stranded total forward");
    assert_eq!(past_activation.retired_safe_weight(), 0, "and books nothing retroactively — there is no claim left to book");
    past_activation
        .assert_internal_consistency_v2(&p, true)
        .expect_err("a block accepted past the activation height inherits the refusal — the fence heals nothing behind it");
}
