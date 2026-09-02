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
