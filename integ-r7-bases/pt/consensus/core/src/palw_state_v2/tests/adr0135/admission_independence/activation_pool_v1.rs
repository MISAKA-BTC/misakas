//! **The Activation Pool through the fold** (ADR-0152-adjacent: Activation Pool, user decision
//! 2026-09-25; the adversarial review's §4, its findings cited by id).
//!
//! The fixture is ADR-0147's contested network — the floor and its producer (bond 1), the registrant
//! (bond 9, operator `op_id(29)`) that bought Kimi, its seven sybils (2..=8) and twenty honest
//! operators (11..=30), every one of them serving the floor at exactly the panel floor (1,000 sompi =
//! ten network floors of 100) — with the pool armed from genesis at the user's terms. Kimi's own
//! audit span is its staggered one (R2); the jury's randomness is a floor attempt in the span before
//! it, and the readiness proofs land two spans before it (review M6's `proved_span ≤ S − 2`).

use super::*;
use crate::palw_activation_pool_v1::*;

fn terms() -> PalwActivationPoolTermsV1 {
    PALW_ACTIVATION_POOL_TERMS_V1
}

/// ADR-0147 armed from genesis (`armed`) and the pool with it.
fn pooled(fold: Option<PalwModelRegistryFoldV1>) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { activation_pool: Some(terms()), ..armed(fold) }
}

const MSK: u64 = 100_000_000;

fn top_up(amount: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ActivationPoolFunded { class_id: kimi_id(), amount, sink_index: 1 }
}

/// The fold, checked: internal consistency (I1–I4), and the delta applying to the child and reverting
/// to the parent exactly (entries 76 and 77 included) — the reorg primitive.
fn step_checked(
    parent: &PalwChainStateV2,
    c: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    att: Option<&PalwAttemptEnvelopeV2>,
    extras: &PalwTransitionExtrasV1,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let p = params();
    let (next, delta) = fold_step(parent, &p, c, objects, att, extras)?;
    assert_eq!(apply_delta_v2(parent, &delta, &p).unwrap().state_root(), next.state_root(), "the delta reproduces the fold");
    let back = revert_delta_v2(&next, &delta, &p).unwrap();
    assert_eq!(back.state_root(), parent.state_root(), "and reverts to the parent");
    assert_eq!(back.activation_pool_counters(), parent.activation_pool_counters(), "counters included");
    Ok(next)
}

/// A chain walked to Kimi's own audit, and what it takes to reach it.
struct ToAudit {
    /// The state after the seed anchor of the span before the audit.
    before: PalwChainStateV2,
    /// The audit span.
    audit: u64,
    /// The next block number and blue score.
    next: u64,
}

impl ToAudit {
    fn ctx_at(&self, span: u64) -> PalwBlockContextV2 {
        ctx(self.next, span * SPAN, self.next)
    }
}

/// **Kimi to its staggered audit**: the contested network at DAA 100 (the pool opens with the
/// registration), the registry's first boundary at 110 (Kimi's row opens `Candidate`), `funded`
/// sompi topped up at 111, every one of `holders` proving possession two spans before the audit,
/// and a floor attempt in the span before it — the jury's anchor.
fn to_audit(funded: u64, holders: &[u64]) -> ToAudit {
    to_audit_among(funded, holders, &[])
}

/// [`to_audit`] on a network that also holds `crowd` (bonds registered beside the contested network).
fn to_audit_among(funded: u64, holders: &[u64], crowd: &[PalwConsensusObjectV2]) -> ToAudit {
    let (operands, root) = inventory();
    let f = fold(kimi_work());
    let period = crate::palw_model_registry_v1::palw_admission_audit_period_spans_v2(params().epoch_length, SPAN, None);
    let audit = (15u64..).find(|span| palw_admission_audit_due_staggered_v1(&kimi_id(), *span, period)).unwrap();
    let mut network = contested_network(root, false);
    network.extend_from_slice(crowd);
    let s1 = step_checked(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &network, None, &pooled(None)).unwrap();
    let opened = s1.activation_pool(&kimi_id()).cloned().expect("a bought class's pool opens with its registration");
    assert_eq!(opened, PalwActivationPoolV1::opened_at(100), "empty, its ramp from the registration");
    let s2 = step_checked(&s1, &ctx(2, 110, 2), &[], None, &pooled(Some(f.clone()))).unwrap();
    assert_eq!(s2.model_lifecycle(&kimi_id()).map(|row| row.state), Some(PalwModelLifecycleV1::Candidate));
    let s3 =
        if funded > 0 { step_checked(&s2, &ctx(3, 111, 3), &[top_up(funded)], None, &pooled(Some(f.clone()))).unwrap() } else { s2 };
    let proofs: Vec<PalwConsensusObjectV2> = holders.iter().map(|n| proof(&operands, bond_key(*n), audit - 2)).collect();
    let s4 = step_checked(&s3, &ctx(4, (audit - 2) * SPAN + 5, 4), &proofs, None, &pooled(Some(f.clone()))).unwrap();
    let before = seeding_attempt(&s4, &ctx(5, (audit - 1) * SPAN + 5, 5), &pooled(Some(f)));
    assert_eq!(before.round_seed_anchor().map(|anchor| anchor.span), Some(audit - 1), "the jury's anchor is the span before");
    ToAudit { before, audit, next: 6 }
}

/// The jury Kimi's audit draws, recomputed from outside the fold (ADR-0147's population rule, at the
/// PANEL floor past the pool's fence — P2), as bond numbers in draw order.
fn drawn(t: &ToAudit) -> Vec<u64> {
    // Past the pool's fence the jury's seed leaves the anchor's block hash out (the fix round's F2).
    use crate::palw_model_registry_v1::palw_admission_jury_seed_v2;
    let anchor = t.before.round_seed_anchor().unwrap();
    let seed = palw_admission_jury_seed_v2(&kimi_id(), t.audit, &anchor.execution_key);
    let cutoff = (t.audit - 1) * SPAN;
    let population: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = t
        .before
        .bonds_iter()
        .filter(|(key, bond)| {
            matches!(bond.status, PalwBondStatusV2::Active)
                && bond.collateral >= 1_000
                && bond.capable_classes.contains(&h64(1))
                && bond.registered_daa < cutoff
                && **key != bond_key(9)
                && bond.operator_id != op_id(29)
        })
        .collect();
    crate::palw_panel_v2::palw_admission_jury_v1(&seed, &population, 5)
        .into_iter()
        .map(|operator| (1..=40u64).find(|n| op_id(20 + n) == operator).expect("a fixture operator"))
        .collect()
}

fn payout_of(state: &PalwChainStateV2, n: u64) -> Option<u64> {
    state
        .pending_payouts_iter()
        .find(|(key, _)| **key == palw_activation_pool_payout_key_v1(&h64(0x9A00 + n)))
        .map(|(_, row)| row.amount)
}

/// **A bought registration opens an empty pool; a top-up is split by `α` while the class is a
/// Candidate and is wholly bonus once it is not; the counters are the rows' sums.**
#[test]
fn a_listing_opens_an_empty_pool_and_a_top_up_splits_by_alpha_while_it_is_a_candidate() {
    let t = to_audit(300 * MSK, &[]);
    let pool = t.before.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!((pool.prep_sompi, pool.bonus_sompi, pool.funded_sompi), (120 * MSK, 180 * MSK, 300 * MSK), "the design's 300 MSK");
    assert_eq!(pool.opened_daa, 100);
    let counters = t.before.activation_pool_counters();
    assert_eq!(
        (counters.funded_sompi, counters.prep_sompi, counters.bonus_sompi),
        (300 * MSK as u128, 120 * MSK as u128, 180 * MSK as u128)
    );
    // **The review's P5 / the fix round's F3: the floor takes no top-up** — its row is never a
    // Candidate's and never leaves Active, so no rule could pay a pool of it; refused (and paid back).
    let floor_top_up = PalwConsensusObjectV2::ActivationPoolFunded { class_id: h64(1), amount: 5 * MSK, sink_index: 1 };
    let refused = fold_step(&t.before, &params(), &t.ctx_at(t.audit), &[floor_top_up], None, &pooled(Some(fold(kimi_work()))));
    assert!(matches!(refused, Err(PalwStateV2Error::ActivationPoolOnFloor(id)) if id == h64(1)), "{:?}", refused.map(|_| ()));
}

/// **The fold refuses a top-up below the fence, of a class the state does not hold, of the floor, and
/// under the least top-up** — each refusal a carrier the P-B1 route pays back — **and folds one of a
/// Frozen class into its row's `withheld`** (the fix round's F6: accounted, never refunded), which the
/// mempool still refuses to relay.
#[test]
fn a_top_up_is_refused_dormant_missing_floor_or_below_the_least_and_a_frozen_one_is_withheld() {
    let (_, root) = inventory();
    let p = params();
    let s1 =
        step_checked(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &contested_network(root, false), None, &pooled(None)).unwrap();
    let refused = |state: &PalwChainStateV2, object: PalwConsensusObjectV2, extras: &PalwTransitionExtrasV1| {
        let next = state.last_point().map(|point| point.blue_score + 1).unwrap();
        fold_step(state, &p, &ctx(next, 100 + next, next), &[object], None, extras).map(|_| ()).unwrap_err()
    };
    assert!(matches!(refused(&s1, top_up(5 * MSK), &armed(None)), PalwStateV2Error::ActivationPoolDormant));
    let missing = PalwConsensusObjectV2::ActivationPoolFunded { class_id: h64(0xDEAD), amount: 5 * MSK, sink_index: 1 };
    assert!(matches!(refused(&s1, missing, &pooled(None)), PalwStateV2Error::MissingClass(_)));
    assert!(matches!(
        refused(&s1, top_up(MSK - 1), &pooled(None)),
        PalwStateV2Error::ActivationPoolTopUpBelowMinimum { amount, min, .. } if amount == MSK - 1 && min == MSK
    ));
    let floor = PalwConsensusObjectV2::ActivationPoolFunded { class_id: h64(1), amount: 5 * MSK, sink_index: 1 };
    assert!(matches!(refused(&s1, floor, &pooled(None)), PalwStateV2Error::ActivationPoolOnFloor(_)));
    let frozen = step_checked(&s1, &ctx(2, 101, 2), &[freeze(kimi_id())], None, &pooled(None)).unwrap();
    let withheld = step_checked(&frozen, &ctx(3, 102, 3), &[top_up(5 * MSK)], None, &pooled(None)).unwrap();
    let pool = withheld.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!((pool.funded_sompi, pool.withheld_sompi, pool.prep_sompi, pool.bonus_sompi), (5 * MSK, 5 * MSK, 0, 0));
    assert!(pool.is_balanced() && withheld.activation_pool_counters().is_balanced());
    // The mempool asks the fold's own question of a carrier (P-B3's gate), and takes one queue row
    // for it (P-B1's budget: the refund it may need).
    let carrier = crate::tx::Transaction::new(
        0,
        vec![],
        vec![crate::tx::TransactionOutput::new(5 * MSK, palw_activation_sink_spk_v1(&kimi_id()))],
        0,
        crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        borsh::to_vec(&crate::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 {
            version: crate::palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2,
            object: PalwConsensusObjectV2::ActivationPoolFunded { class_id: kimi_id(), amount: 5 * MSK, sink_index: 0 },
        })
        .unwrap(),
    );
    assert!(
        matches!(palw_model_market_carrier_refusal_v1(&frozen, &p, &carrier, || pooled(None)), Some(PalwStateV2Error::FrozenClass(_))),
        "the fold withholds it; a node does not relay or mine it"
    );
    assert_eq!(palw_model_market_carrier_refusal_v1(&s1, &p, &carrier, || pooled(None)), None, "a live listing takes it");
    assert_eq!(palw_model_carrier_payout_rows_v1(&carrier), Some(1));
    // A refused top-up's refund is read off its carrier's own outputs, as a buy's is (P-B1): this
    // carrier pays no P2PKH-ML-DSA-87 output, so it names none — and so it is not block-valid (the
    // review's P4, the fix round's F6: the sink rule refuses it); with change it names the change.
    assert_eq!(crate::palw_lifecycle_objects_v2::palw_model_carrier_refund_v1(&carrier, &top_up(5 * MSK)), None);
    assert!(
        crate::palw_activation_pool_v1::palw_activation_sink_binding_refusal_v1(&carrier)
            .is_some_and(|(_, why)| why.contains("P2PKH-ML-DSA-87")),
        "a carrier a refusal could not pay back is refused at block validity"
    );
    let mut with_change = carrier.clone();
    with_change.outputs.insert(0, crate::tx::TransactionOutput::new(1, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&[0x44; 64])));
    let refund = crate::palw_lifecycle_objects_v2::palw_model_carrier_refund_v1(&with_change, &top_up(5 * MSK)).expect("a payee");
    assert_eq!((refund.amount, refund.line_id, refund.payee), (5 * MSK, kimi_id(), Hash64::from_bytes([0x44; 64])));
    // The extraction binds the object to its own sink (the other direction of A8's block rule).
    assert!(crate::palw_lifecycle_objects_v2::palw_activation_pool_binds_its_carrier_v1(&carrier, &carrier_object(&carrier)).is_ok());
    assert!(
        crate::palw_lifecycle_objects_v2::palw_activation_pool_binds_its_carrier_v1(&with_change, &carrier_object(&carrier)).is_err(),
        "index 0 is the change now"
    );
}

fn carrier_object(tx: &crate::tx::Transaction) -> PalwConsensusObjectV2 {
    borsh::from_slice::<crate::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2>(&tx.payload).unwrap().object
}

/// **(a) is paid to a drawn juror whose preparation is proven — when the quorum FAILS.** Of the five
/// jurors, two keep their readiness (the others' rows are gone): the jury does not seat Kimi, and
/// the two are paid `a = A_MAX(age)` each, once; nobody else is. The review's C5 is why it is the
/// preparation that is paid and never the vote: the same two would be paid had the jury seated.
#[test]
fn a_prepared_juror_is_paid_when_the_quorum_fails() {
    let all: Vec<u64> = SYBILS.chain(HONEST).collect();
    let mut t = to_audit(1_000 * MSK, &all);
    let jury = drawn(&t);
    assert_eq!(jury.len(), 5);
    for n in &jury[2..] {
        t.before.seat_readiness.remove(&(bond_key(*n), kimi_id()));
    }
    let audit = step_checked(&t.before, &t.ctx_at(t.audit), &[], None, &pooled(Some(fold(kimi_work())))).unwrap();
    assert_eq!(
        audit.model_lifecycle(&kimi_id()).map(|row| row.state),
        Some(PalwModelLifecycleV1::Candidate),
        "two of five: not seated"
    );
    let age = t.audit * SPAN - 100;
    let a = palw_activation_prep_cap_v1(&terms(), age);
    assert_eq!(a, palw_activation_prep_reward_v1(&terms(), 400 * MSK, age), "a large budget pays A_MAX");
    let pool = audit.activation_pool(&kimi_id()).cloned().unwrap();
    let mut paid: Vec<Hash64> = jury[..2].iter().map(|n| op_id(20 + n)).collect();
    paid.sort();
    assert_eq!(pool.prep_paid, paid, "the two prepared jurors, once each");
    for n in &jury[..2] {
        assert_eq!(payout_of(&audit, *n), Some(a), "juror {n} is paid a");
    }
    for n in &jury[2..] {
        assert_eq!(payout_of(&audit, *n), None, "juror {n} held nothing proven and is paid nothing");
    }
    assert_eq!((pool.prep_sompi, pool.paid_sompi), (400 * MSK - 2 * a, 2 * a));
    assert!(audit.vesting_iter_by_expiry().next().is_none(), "not vested: a pool payout is a queue row, not a claim's reward");
    // Once per operator: the same audit on a state that already paid the first juror pays only the second.
    let mut again = t.before.clone();
    let mut row = again.activation_pools.get(&kimi_id()).cloned().unwrap();
    row.prep_paid = vec![op_id(20 + jury[0])];
    again.activation_pools.insert(kimi_id(), row);
    let once = step_checked(&again, &t.ctx_at(t.audit), &[], None, &pooled(Some(fold(kimi_work())))).unwrap();
    assert_eq!(payout_of(&once, jury[0]), None, "a juror paid before is not paid again");
    assert_eq!(payout_of(&once, jury[1]), Some(a));
}

/// **(a) pays capacity that can serve, proven before the seed existed** (the review's C3/A2-i and M6,
/// and the review's P2 / the fix round's F4): a ready bond below the panel floor is not paid — past
/// the pool's P2 it is not even drawn, and the jury is the next operator's — and neither is a juror
/// whose proof NAMES `S − 2` — as the landing window lets it — but LANDED in `S − 1`, after the anchor
/// that seeds the audit (a block of the span before, here the seeding block). The late juror still
/// counts toward the jury's verdict, which is readiness and not pay.
#[test]
fn a_juror_below_the_panel_floor_or_whose_proof_landed_after_s_minus_2_is_not_paid() {
    let all: Vec<u64> = SYBILS.chain(HONEST).collect();
    let mut t = to_audit(1_000 * MSK, &all);
    let first = drawn(&t);
    // First juror: ready, but 600 sompi — above the readiness bar (3 × 100), below the panel floor
    // (1,000). Past P2 the audit's population leaves it out, and the draw moves to the next operator.
    t.before.bonds.get_mut(&bond_key(first[0])).unwrap().collateral = 600;
    let jury = drawn(&t);
    assert!(!jury.contains(&first[0]), "below the panel floor: not in the jury's population (P2)");
    assert_eq!(&jury[..4], &first[1..], "the rest of the draw keeps its order; the sixth operator joins");
    // Juror 1: a fresh proof naming S − 2, landed in a block of S − 1 AFTER the seeding block.
    let (operands, _) = inventory();
    let late = proof(&operands, bond_key(jury[1]), t.audit - 2);
    let landed_late =
        step_checked(&t.before, &ctx(t.next, (t.audit - 1) * SPAN + 7, t.next), &[late], None, &pooled(Some(fold(kimi_work()))))
            .unwrap();
    assert_eq!(landed_late.round_seed_anchor().map(|anchor| anchor.span), Some(t.audit - 1), "the anchor still stands");
    assert_eq!(
        landed_late.seat_readiness(&bond_key(jury[1]), &kimi_id()).map(|row| row.proved_span),
        Some(t.audit - 2),
        "it names S − 2"
    );
    assert_eq!(landed_late.activation_readiness_landed(&kimi_id(), &bond_key(jury[1])), Some(t.audit - 1), "and landed at S − 1");
    t.before = landed_late;
    t.next += 1;
    let audit = step_checked(&t.before, &t.ctx_at(t.audit), &[], None, &pooled(Some(fold(kimi_work())))).unwrap();
    assert_eq!(
        audit.model_lifecycle(&kimi_id()).map(|row| row.state),
        Some(PalwModelLifecycleV1::Prefetching),
        "all five ready: seated"
    );
    assert_eq!(payout_of(&audit, first[0]), None, "below the panel floor: no panel can draw it, and no jury does");
    assert_eq!(payout_of(&audit, jury[1]), None, "landed after S − 2: built with the seed in hand, whatever span it names");
    assert!(
        audit.activation_readiness_landed(&kimi_id(), &bond_key(jury[2])).is_none(),
        "seated: the class's landing records leave with its Candidate state"
    );
    let pool = audit.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!(pool.prep_paid.len(), 4, "the other four, whatever the verdict");
    assert!(pool.prep_paid.iter().all(|op| *op != op_id(29)), "never the registrant's operator (the population excludes it)");
    // Seated: the class is past Candidate, so (a) has no payee left and prep is bonus now.
    let a = palw_activation_prep_cap_v1(&terms(), t.audit * SPAN - 100);
    assert_eq!(pool.prep_sompi, 0, "prep moves to bonus at seating");
    assert_eq!(pool.bonus_sompi, 1_000 * MSK - 4 * a);
    assert!(pool.is_balanced());
}

/// A bond that serves the floor at `collateral` sompi: at or above the network floor (100) and, for
/// the crowd below, under the panel floor (1,000) — it can never be ready for Kimi.
fn floor_bond(n: u64, collateral: u64) -> PalwConsensusObjectV2 {
    match bond(n, collateral) {
        PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, collateral, payout_payload, signature, .. } => {
            PalwConsensusObjectV2::BondRegistered {
                bond,
                pubkey,
                operator_pubkey,
                collateral,
                payout_payload,
                capable_classes: [h64(1)].into_iter().collect(),
                signature,
            }
        }
        _ => unreachable!("`bond` builds a BondRegistered"),
    }
}

/// **P2 (user decision 2026-09-25): past the pool's fence the admission jury is drawn from bonds at
/// the panel floor.** A crowd of 150 floor bonds — serving the floor, at five network floors, under
/// the panel floor and so never ready — joins the contested network. Past the fence Kimi's jury is
/// five of the twenty-seven panel-floor operators, every one of them ready, and it seats. The
/// fence-off twin — ADR-0147 alone, the same network and the same readiness, walked to its own audit
/// at span 100 — still draws from the network floor, as it always did: the crowd fills its jury and
/// the class stays a Candidate.
#[test]
fn p2_past_the_pool_fence_the_jury_is_drawn_at_the_panel_floor_and_a_floor_crowd_cannot_block_it() {
    let crowd: Vec<PalwConsensusObjectV2> = (40..=189u64).map(|n| floor_bond(n, 500)).collect();
    let all: Vec<u64> = SYBILS.chain(HONEST).collect();
    let f = fold(kimi_work());

    // Past the fence: seated by a jury of panel-floor operators.
    let t = to_audit_among(0, &all, &crowd);
    assert_eq!(t.before.bonds_iter().filter(|(_, bond)| bond.collateral == 500).count(), 150, "the crowd is registered");
    let jury = drawn(&t);
    assert_eq!(jury.len(), 5);
    assert!(jury.iter().all(|n| all.contains(n)), "no floor bond is drawn: {jury:?}");
    let seated = step_checked(&t.before, &t.ctx_at(t.audit), &[], None, &pooled(Some(f.clone()))).unwrap();
    assert_eq!(
        seated.model_lifecycle(&kimi_id()).map(|row| row.state),
        Some(PalwModelLifecycleV1::Prefetching),
        "five ready jurors: seated, whatever the crowd"
    );

    // The fence-off twin: ADR-0147's own walk (`to_first_audit`'s blocks) on the same network.
    let p = params();
    let (operands, root) = inventory();
    let mut network = contested_network(root, false);
    network.extend(crowd.iter().cloned());
    let (s1, _) = fold_step(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &network, None, &armed(None)).unwrap();
    let proofs: Vec<PalwConsensusObjectV2> = all.iter().map(|n| proof(&operands, bond_key(*n), 98)).collect();
    let (s2, _) = fold_step(&s1, &p, &ctx(2, 985, 2), &proofs, None, &armed(Some(f.clone()))).unwrap();
    let s3 = seeding_attempt(&s2, &ctx(3, 995, 3), &armed(Some(f.clone())));
    let anchor = s3.round_seed_anchor().expect("span 99 recorded a seed anchor");
    let seed = crate::palw_model_registry_v1::palw_admission_jury_seed_v1(&kimi_id(), 100, &anchor.block, &anchor.execution_key);
    let population: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = s3
        .bonds_iter()
        .filter(|(key, bond)| {
            matches!(bond.status, PalwBondStatusV2::Active)
                && bond.collateral >= 100
                && bond.capable_classes.contains(&h64(1))
                && bond.registered_daa < 990
                && **key != bond_key(9)
        })
        .collect();
    let old_jury = crate::palw_panel_v2::palw_admission_jury_v1(&seed, &population, 5);
    let from_the_crowd = old_jury.iter().filter(|op| (40..=189u64).any(|n| op_id(20 + n) == **op)).count();
    assert!(from_the_crowd >= 3, "below the fence the network floor's crowd fills the jury: {from_the_crowd} of 5");
    let (s4, _) = fold_step(&s3, &p, &ctx(4, 1_000, 4), &[], None, &armed(Some(f))).unwrap();
    assert_eq!(
        s4.model_lifecycle(&kimi_id()).map(|row| row.state),
        Some(PalwModelLifecycleV1::Candidate),
        "the old rule, unchanged: a jury of never-ready floor bonds does not seat"
    );
}

/// **Per audit at most `seat_count × a`; the waiting bonus ramps `A_MAX` from `A0` to `3·A0`; and a
/// small budget pays a tenth of itself.**
#[test]
fn the_audit_pays_at_most_five_times_a_and_a_ramps_with_the_pools_age() {
    let all: Vec<u64> = SYBILS.chain(HONEST).collect();
    let t = to_audit(2_000 * MSK, &all);
    let f = fold(kimi_work());
    // At the fixture's age, and — with a 100-DAA ramp and the pool opened at 0 — past the whole ramp.
    let short_ramp = PalwActivationPoolTermsV1 { ramp_daa: 100, ..terms() };
    for (ramped, extras) in [
        (false, pooled(Some(f.clone()))),
        (true, PalwTransitionExtrasV1 { activation_pool: Some(short_ramp), ..armed(Some(f.clone())) }),
    ] {
        let mut before = t.before.clone();
        if ramped {
            before.activation_pools.get_mut(&kimi_id()).unwrap().opened_daa = 0;
        }
        let audit = step_checked(&before, &t.ctx_at(t.audit), &[], None, &extras).unwrap();
        let pool = audit.activation_pool(&kimi_id()).cloned().unwrap();
        let used = extras.activation_pool.unwrap();
        let a = palw_activation_prep_cap_v1(&used, t.audit * SPAN - pool.opened_daa);
        if ramped {
            assert_eq!(a, 3 * terms().prep_base_sompi, "past W the cap is 3·A0");
        } else {
            assert!(a > terms().prep_base_sompi && a < 3 * terms().prep_base_sompi, "part way up the ramp: {a}");
        }
        assert_eq!(pool.prep_paid.len(), 5, "five prepared jurors");
        assert_eq!(pool.paid_sompi, 5 * a, "seat_count × a, the audit's ceiling");
    }
    // A budget of 30 MSK pays a tenth of itself (3 MSK < A0 = 20 MSK) to each of the five.
    let small = to_audit(75 * MSK, &all);
    let audit = step_checked(&small.before, &small.ctx_at(small.audit), &[], None, &pooled(Some(f))).unwrap();
    let pool = audit.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!(pool.paid_sompi, 5 * 3 * MSK, "prep 30 MSK: 3 MSK each, 15 in all");
}

/// **A full payout queue defers (a)'s FLUSH, never the decision** (the fix round's F5): the audit
/// marks the five prepared jurors and moves `5a` from `prep` into `scheduled`, writes no queue row,
/// and a later block with room flushes it — conserved at every step.
#[test]
fn a_full_queue_defers_the_preparation_reward_flush_and_a_block_with_room_pays_it() {
    let all: Vec<u64> = SYBILS.chain(HONEST).collect();
    let t = to_audit(1_000 * MSK, &all);
    let mut full = t.before.clone();
    let mut filler = 0u64;
    // Full AFTER the block's own drain (step 1b takes the first eight rows before anything else).
    while full.pending_payouts.len() < PALW_V2_MAX_PENDING_PAYOUTS + PALW_V2_MAX_PAYOUTS_PER_BLOCK {
        let mut key = h64(0x7F00_0000 + filler).as_bytes();
        key[0] = 0x10;
        full.pending_payouts.insert(Hash64::from_bytes(key), PalwPayoutV2 { payload: h64(1), amount: 1 });
        filler += 1;
    }
    let p = params();
    let (audit, _) = apply_palw_transition_v2_with_extras(
        &full,
        &p,
        &t.ctx_at(t.audit),
        &[],
        None,
        false,
        false,
        false,
        false,
        &pooled(Some(fold(kimi_work()))),
    )
    .unwrap();
    let pool = audit.activation_pool(&kimi_id()).cloned().unwrap();
    let a = palw_activation_prep_cap_v1(&terms(), t.audit * SPAN - 100);
    assert_eq!(pool.prep_paid.len(), 5, "decided and marked: the money is committed to them");
    assert_eq!((pool.scheduled_sompi, pool.paid_sompi), (5 * a, 0), "owed, not yet in the queue");
    assert_eq!(audit.activation_pool_scheduled_iter().count(), 5);
    assert!(audit.pending_payouts_iter().all(|(key, _)| key.as_byte_slice()[..2] != PALW_ACTIVATION_POOL_PAYOUT_KEY_PREFIX_V1));
    assert!(pool.is_balanced() && audit.activation_pool_counters().is_balanced());
    audit.assert_internal_consistency(&p).expect("I5: the scheduled map sums to the row's scheduled");
    // Room again: every filler gone; the next block flushes the five (8 − 0 − 0 new keys of width).
    let mut room = audit.clone();
    room.pending_payouts.retain(|key, _| key.as_byte_slice()[0] != 0x10);
    let flushed =
        step_checked(&room, &ctx(t.next + 1, t.audit * SPAN + 1, t.next + 1), &[], None, &pooled(Some(fold(kimi_work())))).unwrap();
    let pool = flushed.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!((pool.scheduled_sompi, pool.paid_sompi), (0, 5 * a));
    assert_eq!(flushed.activation_pool_scheduled_iter().count(), 0);
    for n in drawn(&t) {
        assert_eq!(payout_of(&flushed, n), Some(a), "juror {n}'s row");
    }
}

/// Kimi walked on past its audit: seated, then `Probation` with seven ready seats — the state (b)
/// is read from.
fn in_probation(funded: u64) -> (PalwChainStateV2, u64, u64) {
    let all: Vec<u64> = SYBILS.chain(HONEST).collect();
    let t = to_audit(funded, &all);
    let f = fold(kimi_work());
    let seated = step_checked(&t.before, &t.ctx_at(t.audit), &[], None, &pooled(Some(f.clone()))).unwrap();
    assert_eq!(seated.model_lifecycle(&kimi_id()).map(|row| row.state), Some(PalwModelLifecycleV1::Prefetching));
    let (operands, _) = inventory();
    let proofs: Vec<PalwConsensusObjectV2> = all.iter().map(|n| proof(&operands, bond_key(*n), t.audit + 1)).collect();
    let probation =
        step_checked(&seated, &ctx(t.next + 1, (t.audit + 1) * SPAN, t.next + 1), &proofs, None, &pooled(Some(f))).unwrap();
    assert!(
        matches!(probation.model_lifecycle(&kimi_id()).map(|row| row.state), Some(PalwModelLifecycleV1::Probation { .. })),
        "{:?}",
        probation.model_lifecycle(&kimi_id())
    );
    (probation, t.audit + 1, t.next + 2)
}

/// Bond `n`'s credit on probe claim `claim`.
fn credit(n: u64, claim: Hash64) -> PalwActivationCreditV1 {
    PalwActivationCreditV1 { operator: op_id(20 + n), bond: bond_key(n), claim }
}

/// The next boundary with the row one probe short of `ActiveLimited` credited `credited`.
fn to_active_limited(state: &PalwChainStateV2, credited: &[u64], span: u64, next: u64) -> PalwChainStateV2 {
    let mut s = state.clone();
    let mut row = s.model_lifecycles.get(&kimi_id()).cloned().unwrap();
    row.state = PalwModelLifecycleV1::Probation { probes_passed: 10 };
    s.set_model_lifecycle_for_tests(kimi_id(), row);
    let mut pool = s.activation_pools.get(&kimi_id()).cloned().unwrap();
    for n in credited {
        pool.credit(credit(*n, h64(0xC1A1)));
    }
    s.activation_pools.insert(kimi_id(), pool);
    step_checked(&s, &ctx(next, span * SPAN, next), &[], None, &pooled(Some(fold(kimi_work())))).unwrap()
}

/// **(b) at `Probation → ActiveLimited`, to the operators credited on the probe Finals — the outsider
/// included, the registrant excluded — and a re-formation pays only new operators** (the review's A1).
#[test]
fn the_activation_bonus_pays_the_credited_operators_at_active_limited_and_only_new_ones_on_reformation() {
    // A pool small enough that every share here is under P1's b_cap: this test is about the split.
    let (probation, span, next) = in_probation(500 * MSK);
    let bonus_before = probation.activation_pool(&kimi_id()).unwrap().bonus_sompi;
    // The largest share below is the re-formation's lone payee: β of what the first run left, a quarter.
    assert!(bonus_before / 4 < terms().bonus_cap_sompi, "the premise: no share here reaches b_cap");
    // Credited: two sybils, an honest outsider (the ADR-0147 seat), and the registrant's operator.
    let active = to_active_limited(&probation, &[2, 3, 17, 9], span + 1, next);
    assert!(matches!(active.model_lifecycle(&kimi_id()).map(|row| row.state), Some(PalwModelLifecycleV1::ActiveLimited { .. })));
    let pool = active.activation_pool(&kimi_id()).cloned().unwrap();
    let b = palw_activation_bonus_reward_v1(&terms(), bonus_before, 3);
    assert_eq!(b, bonus_before / 2 / 3, "β = 500 ‰ over three payees");
    for n in [2u64, 3, 17] {
        assert_eq!(payout_of(&active, n), Some(b), "operator {n} is paid b");
    }
    assert_eq!(payout_of(&active, 9), None, "never the registrant's operator");
    let mut paid = vec![op_id(22), op_id(23), op_id(37)];
    paid.sort();
    assert_eq!(pool.bonus_paid, paid);
    assert!(pool.probe_credited.is_empty(), "the run's list is spent");
    assert_eq!(pool.bonus_sompi, bonus_before - 3 * b, "the other half stays for a re-formation");

    // Held, then Probation again: the new run's list restarts, and a paid operator is not paid twice.
    let mut held = active.clone();
    let mut row = held.model_lifecycles.get(&kimi_id()).cloned().unwrap();
    row.state = PalwModelLifecycleV1::Held;
    held.set_model_lifecycle_for_tests(kimi_id(), row);
    let mut dirty = held.activation_pools.get(&kimi_id()).cloned().unwrap();
    dirty.probe_credited = vec![credit(20, h64(0xC1A2))];
    held.activation_pools.insert(kimi_id(), dirty);
    let back = step_checked(&held, &ctx(next + 1, (span + 2) * SPAN, next + 1), &[], None, &pooled(Some(fold(kimi_work())))).unwrap();
    assert!(matches!(back.model_lifecycle(&kimi_id()).map(|row| row.state), Some(PalwModelLifecycleV1::Probation { .. })));
    assert!(back.activation_pool(&kimi_id()).unwrap().probe_credited.is_empty(), "entering Probation restarts the run");
    let again = to_active_limited(&back, &[2, 5], span + 3, next + 2);
    let pool_again = again.activation_pool(&kimi_id()).cloned().unwrap();
    let b2 = palw_activation_bonus_reward_v1(&terms(), pool.bonus_sompi, 1);
    assert_eq!(payout_of(&again, 5), Some(b2), "only the new operator, alone: the whole β share (to b_cap)");
    // Operator 2's first bonus left the queue with the drains since; nothing new was written for it.
    assert_eq!(payout_of(&again, 2), None, "a paid operator is not paid again");
    assert!(pool_again.bonus_paid.contains(&op_id(25)) && pool_again.bonus_paid.len() == 4);
    assert_eq!(pool_again.bonus_sompi, pool.bonus_sompi - b2);
    // Past Candidate a sponsor's top-up is all bonus: (a) has no payee left.
    let sponsored = step_checked(
        &again,
        &ctx(next + 3, (span + 3) * SPAN + 1, next + 3),
        &[top_up(10 * MSK)],
        None,
        &pooled(Some(fold(kimi_work()))),
    )
    .unwrap();
    let after = sponsored.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!((after.prep_sompi, after.bonus_sompi), (0, pool_again.bonus_sompi + 10 * MSK));
}

/// **P1 (user decision 2026-09-25): a huge sponsored pool pays each credited operator at most
/// `b_cap`, and keeps the rest** — `b = min(⌊bonus × β / n⌋, b_cap)`, the per-`Final` seat pay at the
/// heaviest class (128.03 MSK). The budget loses exactly `n × b_cap`, every sompi stays accounted
/// (I1), and a later operator on a re-formation is paid out of what the cap kept.
#[test]
fn a_huge_sponsored_pool_pays_each_operator_at_most_b_cap_and_keeps_the_rest() {
    let huge = 10_000_000 * MSK;
    let (probation, span, next) = in_probation(huge);
    let before = probation.activation_pool(&kimi_id()).cloned().unwrap();
    let b_cap = terms().bonus_cap_sompi;
    assert_eq!(b_cap, 12_803_386_003, "E × 200 ‰ / 5");
    assert!(before.bonus_sompi / 2 / 3 > b_cap, "the premise: the uncapped share is far past the cap");
    let active = to_active_limited(&probation, &[2, 3, 17], span + 1, next);
    let pool = active.activation_pool(&kimi_id()).cloned().unwrap();
    for n in [2u64, 3, 17] {
        assert_eq!(payout_of(&active, n), Some(b_cap), "operator {n} is paid b_cap, not its share");
    }
    assert_eq!(pool.bonus_sompi, before.bonus_sompi - 3 * b_cap, "the rest stays for later operators");
    assert!(pool.is_balanced(), "I1: funded == prep + bonus + scheduled + paid + withheld");
    assert_eq!(pool.funded_sompi, before.funded_sompi);

    // A re-formation pays a new operator out of what the cap kept — and still at most b_cap.
    let mut held = active.clone();
    let mut row = held.model_lifecycles.get(&kimi_id()).cloned().unwrap();
    row.state = PalwModelLifecycleV1::Held;
    held.set_model_lifecycle_for_tests(kimi_id(), row);
    let back = step_checked(&held, &ctx(next + 1, (span + 2) * SPAN, next + 1), &[], None, &pooled(Some(fold(kimi_work())))).unwrap();
    let again = to_active_limited(&back, &[5], span + 3, next + 2);
    assert_eq!(payout_of(&again, 5), Some(b_cap));
    let kept = again.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!(kept.bonus_sompi, pool.bonus_sompi - b_cap);
    assert!(kept.is_balanced());
}

/// **(b) pays the CREDITED bond's payload** (the fix round's L3), not whichever bond of the operator
/// registered first: an operator credited by a bond paying elsewhere is paid there.
#[test]
fn the_activation_bonus_pays_the_credited_bonds_payload() {
    let (mut probation, span, next) = in_probation(1_000 * MSK);
    // Operator 22's credited seat is bond 2; point bond 2's payee somewhere its operator's other
    // bonds (none here, one bond per operator) could not be confused with.
    probation.bonds.get_mut(&bond_key(2)).unwrap().payout_payload = h64(0x2222);
    let active = to_active_limited(&probation, &[2], span + 1, next);
    let row = active
        .pending_payouts_iter()
        .find(|(key, _)| **key == palw_activation_pool_payout_key_v1(&h64(0x2222)))
        .map(|(_, row)| row.amount);
    assert!(row.is_some_and(|amount| amount > 0), "paid to the credited bond's payload");
    assert_eq!(payout_of(&active, 2), None, "not to a payload read off the operator");
}

/// **(b)'s flush waits for a full queue; the decision does not** (F5): scheduled and marked at
/// `Probation → ActiveLimited`, flushed at the next span with room.
#[test]
fn a_full_queue_defers_the_activation_bonus_flush_and_the_next_span_pays_it() {
    let (probation, span, next) = in_probation(1_000 * MSK);
    let mut full = probation.clone();
    let mut filler = 0u64;
    // Full AFTER the block's own drain (step 1b takes the first eight rows before anything else).
    while full.pending_payouts.len() < PALW_V2_MAX_PENDING_PAYOUTS + PALW_V2_MAX_PAYOUTS_PER_BLOCK {
        let mut key = h64(0x7F00_0000 + filler).as_bytes();
        key[0] = 0x10;
        full.pending_payouts.insert(Hash64::from_bytes(key), PalwPayoutV2 { payload: h64(1), amount: 1 });
        filler += 1;
    }
    let paid_before = probation.activation_pool(&kimi_id()).unwrap().paid_sompi;
    let deferred = to_active_limited(&full, &[2, 3], span + 1, next);
    let pool = deferred.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!(pool.bonus_paid.len(), 2, "decided and marked");
    assert!(pool.probe_credited.is_empty() && pool.scheduled_sompi > 0, "owed");
    assert_eq!(pool.paid_sompi, paid_before, "and not in the queue");
    assert_eq!(payout_of(&deferred, 2), None);
    let mut room = deferred.clone();
    room.pending_payouts.retain(|key, _| key.as_byte_slice()[0] != 0x10);
    let paid = step_checked(&room, &ctx(next + 1, (span + 2) * SPAN, next + 1), &[], None, &pooled(Some(fold(kimi_work())))).unwrap();
    let pool = paid.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!(pool.scheduled_sompi, 0, "flushed at the next span with room");
    assert!(payout_of(&paid, 2).is_some() && payout_of(&paid, 3).is_some());
}

/// **Frozen moves the budgets to the row's own `withheld`, never to `panel_reserve_sompi`** (the
/// review's C10), and nothing is refunded.
#[test]
fn a_frozen_class_withholds_its_pool() {
    let t = to_audit(300 * MSK, &[]);
    let reserve = t.before.panel_reserve_sompi();
    let frozen = step_checked(&t.before, &t.ctx_at(t.audit), &[freeze(kimi_id())], None, &pooled(Some(fold(kimi_work())))).unwrap();
    let pool = frozen.activation_pool(&kimi_id()).cloned().unwrap();
    assert_eq!((pool.prep_sompi, pool.bonus_sompi, pool.withheld_sompi, pool.funded_sompi), (0, 0, 300 * MSK, 300 * MSK));
    assert_eq!(frozen.panel_reserve_sompi(), reserve, "not the panel reserve");
    assert_eq!(frozen.activation_pool_counters().withheld_sompi, 300 * MSK as u128);
    assert!(
        frozen.pending_payouts_iter().all(|(key, _)| key.as_byte_slice()[..2] != PALW_ACTIVATION_POOL_PAYOUT_KEY_PREFIX_V1),
        "no refund"
    );
}

/// **An empty pool map leaves every root where it was**: the same blocks on a network nobody bought
/// from, folded with the pool armed and unarmed, root identically, and the carriage carries no `0xB5`
/// tail. What a network without the fence (testnet-11, mainnet) folds is the unarmed column.
#[test]
fn an_empty_pool_map_leaves_every_root_unchanged() {
    let f = fold(kimi_work());
    let mut roots = Vec::new();
    for extras in [pooled(Some(f.clone())), armed(Some(f.clone()))] {
        let mut s = step_checked(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &register_class_and_bond(), None, &extras).unwrap();
        for (i, daa) in [110u64, 120, 1_000, 2_000, 13_000].into_iter().enumerate() {
            s = step_checked(&s, &ctx(2 + i as u64, daa, 2 + i as u64), &[], None, &extras).unwrap();
        }
        assert!(!s.has_activation_pool_data());
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s)).unwrap();
        let no_pool = bytes.iter().rev().take(64).all(|byte| *byte != 0xB5) || s.activation_pools.is_empty();
        assert!(no_pool);
        roots.push(s.state_root());
    }
    assert_eq!(roots[0], roots[1], "an empty map roots as the build without it");
    // And one row does move it: the block is Some-only, not absent.
    let t = to_audit(0, &[2, 3]);
    assert_eq!(t.before.activation_readiness_landed(&kimi_id(), &bond_key(2)), Some(t.audit - 2), "a Candidate's landing is recorded");
    let mut cleared = t.before.clone();
    cleared.activation_pools.clear();
    cleared.activation_pool_counters = Default::default();
    cleared.activation_readiness_landed.clear();
    assert_ne!(cleared.state_root(), t.before.state_root(), "a pool row is in the root");
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&t.before)).unwrap();
    let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back.activation_pools, t.before.activation_pools, "the 0xB5 tail round-trips");
    assert_eq!(back.activation_readiness_landed, t.before.activation_readiness_landed, "the landing records with it");
    assert_eq!(back.into_state(&params(), Some(t.before.state_root())).unwrap().state_root(), t.before.state_root());
}

/// **I1–I4 are checked on import**: an unbalanced row, a counter off its rows, an unsorted list and a
/// row for no class are each refused.
#[test]
fn the_pool_invariants_are_checked_on_import() {
    let t = to_audit(300 * MSK, &[]);
    let p = params();
    let refused = |edit: &dyn Fn(&mut PalwChainStateV2)| {
        let mut s = t.before.clone();
        edit(&mut s);
        s.assert_internal_consistency(&p).expect_err("an inconsistent pool is refused")
    };
    refused(&|s| s.activation_pools.get_mut(&kimi_id()).unwrap().prep_sompi += 1);
    refused(&|s| s.activation_pool_counters.paid_sompi += 1);
    refused(&|s| s.activation_pools.get_mut(&kimi_id()).unwrap().prep_paid = vec![h64(2), h64(1)]);
    // I5: a scheduled payout the row does not account for.
    refused(&|s| {
        s.activation_pool_scheduled.insert((kimi_id(), h64(0x9A02)), 7);
    });
    refused(&|s| {
        let row = s.activation_pools.get(&kimi_id()).cloned().unwrap();
        s.activation_pools.insert(h64(0xDEAD), row.clone());
        s.activation_pool_counters = PalwActivationPoolCountersV1::of_rows(s.activation_pools.values());
    });
    t.before.assert_internal_consistency(&p).expect("the fixture itself is consistent");
}

/// **(b)'s tracking, through a real `Final`**: while the class's row is in `Probation`, the operators
/// of the credited seats of each attempt `Final` join the pool's `probe_credited`; an uncredited seat
/// does not. ADR-0124's economy fixture (the floor's claim, three seats, two of them Valid), with the
/// floor's row set to `Probation` and a pool opened for it.
#[test]
fn a_probe_final_credits_its_credited_seats_operators() {
    let p = super::super::super::params().with_worker_carve_permille(620).unwrap();
    let (s3, claim_id) = economy_bound(&p);
    let extras =
        PalwTransitionExtrasV1 { activation_pool: Some(terms()), model_registry: Some(fold(kimi_work())), ..economy_extras() };
    let mut s3 = s3;
    let mut row = crate::palw_model_registry_v1::PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Probation { probes_passed: 0 },
        work: kimi_work(),
        profile: Default::default(),
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    };
    row.profile = crate::palw_model_registry_v1::palw_lifecycle_profile_v1(&kimi_work(), 1, &fold(kimi_work()).globals, false);
    s3.set_model_lifecycle_for_tests(h64(1), row);
    let opened = PalwActivationPoolV1 { bonus_sompi: 10 * MSK, funded_sompi: 10 * MSK, ..PalwActivationPoolV1::opened_at(100) };
    s3.activation_pool_counters = PalwActivationPoolCountersV1::of_rows([&opened]);
    s3.activation_pools.insert(h64(1), opened);
    let receipts = vec![receipt_at(claim_id, bond_key(1), true, 103), receipt_at(claim_id, bond_key(2), true, 103)];
    let (s4, _) = apply_palw_transition_v2_with_extras(
        &s3,
        &p,
        &ctx(4, 103, 4),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }],
        None,
        false,
        false,
        false,
        false,
        &extras,
    )
    .unwrap();
    let (s5, d5) =
        apply_palw_transition_v2_with_extras(&s4, &p, &ctx(5, 124, 5), &[], None, false, false, false, false, &extras).unwrap();
    assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the premise: a Final");
    let mut credited = vec![
        PalwActivationCreditV1 { operator: op_id(21), bond: bond_key(1), claim: claim_id },
        PalwActivationCreditV1 { operator: op_id(22), bond: bond_key(2), claim: claim_id },
    ];
    credited.sort_by_key(|credit| credit.operator);
    assert_eq!(
        s5.activation_pool(&h64(1)).unwrap().probe_credited,
        credited,
        "the two Valid seats' operators, with their credited bonds and the claim, not the silent third"
    );
    assert!(d5.entries.iter().any(|entry| matches!(entry, PalwDeltaEntryV2::ActivationPool { .. })));
    assert_eq!(revert_delta_v2(&s5, &d5, &p).unwrap().state_root(), s4.state_root(), "and it reverts with its block");
    // **The fix round's L2: a conviction of that Final takes its credits back** — they were earned
    // serving a claim a court has since proved false.
    let mut b = TransitionBuilder::new(&s5, &p, false, false, false, false, &extras);
    b.reverse_convicted_final(&ctx(6, 125, 6), claim_id, PalwVoidReasonV2::CourtFraud).expect("the Final reverses");
    assert!(b.state.activation_pool(&h64(1)).unwrap().probe_credited.is_empty(), "the convicted Final's credits are gone");
}

/// **(a) under the configured readiness horizon** (user decision 2026-09-25, readiness capacity
/// option (a)): a drawn juror is paid on a V2 row that LANDED at `S − 2` or earlier (the fix round's
/// F4) AND is fresh at `S` by the registry's one rule — so the horizon moves only how old a row may
/// be, never when it may have landed. At twenty-four spans a row naming `S − 24` pays and one naming
/// `S − 25` does not; at the default eight, `S − 8` pays and `S − 9` does not; a fresh row that
/// landed in `S − 1`, after the seed existed, pays under neither. The landing window
/// (`palw_readiness_landing_spans_v1`, 40 spans on one-DAA spans) is not asked and need not change:
/// the pay reads the span a proof landed in, which no proof built with the seed in hand can put at
/// `S − 2`, and a row older than the horizon is stale whenever it landed.
#[test]
fn a_is_paid_on_a_row_landed_by_s_minus_2_and_fresh_at_s_under_the_configured_horizon() {
    let all: Vec<u64> = SYBILS.chain(HONEST).collect();
    let t = to_audit(1_000 * MSK, &all);
    let jury = drawn(&t);
    assert_eq!(jury.len(), 5);
    let s = t.audit;
    const DEFAULT_HORIZON: u32 = crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_V1;
    // Per drawn juror: (the span its row names, the span it landed in, paid at 24 spans, paid at 8).
    let cases = [
        (s - 24, s - 2, true, false),  // exactly the configured horizon old at S
        (s - 25, s - 2, false, false), // a span past it
        (s - 2, s - 1, false, false),  // fresh, but landed after S − 2
        (s - 8, s - 8, true, true),    // the default horizon's own edge, landed long before
        (s - 9, s - 3, true, false),   // past the default, inside twenty-four
    ];
    for spans in [crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_T12_V1, DEFAULT_HORIZON] {
        let mut f = fold(kimi_work());
        f.globals.readiness_v2_max_age_spans = spans;
        f.readiness_v2_active = true;
        let extras = PalwTransitionExtrasV1 { readiness_v2_active: true, ..pooled(Some(f.clone())) };
        let mut before = t.before.clone();
        for (n, (named, landed, ..)) in jury.iter().zip(cases) {
            before.seat_readiness.insert(
                (bond_key(*n), kimi_id()),
                crate::palw_model_registry_v1::PalwSeatReadinessRowV1 {
                    proved_daa: named * SPAN,
                    proved_span: named,
                    leaf_index: 0,
                    proof_version: 2,
                    chunks: 16,
                },
            );
            before.activation_readiness_landed.insert((kimi_id(), bond_key(*n)), landed);
        }
        let audit = step_checked(&before, &t.ctx_at(s), &[], None, &extras).unwrap();
        for (n, (named, landed, at_24, at_8)) in jury.iter().zip(cases) {
            let fresh = f.readiness_row_is_fresh(before.seat_readiness(&bond_key(*n), &kimi_id()).unwrap(), s * SPAN);
            let pays = if spans == DEFAULT_HORIZON { at_8 } else { at_24 };
            assert_eq!(fresh && landed + 2 <= s, pays, "{spans} spans, juror {n}: the pay is exactly `landed ≤ S − 2 ∧ fresh at S`");
            assert_eq!(
                payout_of(&audit, *n).is_some(),
                pays,
                "{spans} spans, juror {n}: a row naming S − {} that landed at S − {}",
                s - named,
                s - landed
            );
        }
    }
}

/// **A refused replay does not re-date the pool's landing record** (the readiness horizon's replay
/// fix; the fix round's F4 record, delta 79). A holder's proof landed at `S − 2`; past
/// `Params::palw_readiness_v2_max_age_spans` its replay in `S − 1` — the same proof, or an older one —
/// is refused, so the record stays at `S − 2` and the juror is still paid; a newer proof still moves
/// row and record forward. The fence-off twin shows the hole the fix closes: the same replay is taken
/// and re-dates the record to `S − 1`, after the seed, which costs the juror its pay.
#[test]
fn a_refused_replay_does_not_re_date_the_pools_landing_record() {
    let all: Vec<u64> = SYBILS.chain(HONEST).collect();
    let t = to_audit(1_000 * MSK, &all);
    let n = *drawn(&t).first().expect("a drawn juror");
    let (operands, _) = inventory();
    let armed = params().with_readiness_v2_max_age_spans(Some(24));
    let extras = pooled(Some(fold(kimi_work())));
    let at = ctx(t.next, (t.audit - 1) * SPAN + 7, t.next);
    let landed = |state: &PalwChainStateV2| state.activation_readiness_landed(&kimi_id(), &bond_key(n));
    assert_eq!(landed(&t.before), Some(t.audit - 2), "the holder's proof landed at S − 2");
    for span in [t.audit - 2, t.audit - 5] {
        let replay = proof(&operands, bond_key(n), span);
        let refused = fold_step(&t.before, &armed, &at, &[replay], None, &extras);
        assert!(
            matches!(refused, Err(PalwStateV2Error::ReadinessProofNotNewer { .. })),
            "span S − {}: {:?}",
            t.audit - span,
            refused.map(|_| ())
        );
    }
    // The block the replay was refused from: the record and the row are the parent's.
    let (kept, _) = fold_step(&t.before, &armed, &at, &[], None, &extras).unwrap();
    assert_eq!(landed(&kept), Some(t.audit - 2), "the record only moves forward");
    // …and a newer proof still moves it.
    let (moved, _) = fold_step(&t.before, &armed, &at, &[proof(&operands, bond_key(n), t.audit - 1)], None, &extras).unwrap();
    assert_eq!(landed(&moved), Some(t.audit - 1));
    assert_eq!(moved.seat_readiness(&bond_key(n), &kimi_id()).map(|row| row.proved_span), Some(t.audit - 1));
    // The fence-off twin: the replay is taken and re-dates the record past the seed.
    let (twin, _) = fold_step(&t.before, &params(), &at, &[proof(&operands, bond_key(n), t.audit - 2)], None, &extras).unwrap();
    assert_eq!(landed(&twin), Some(t.audit - 1), "below the fence the replay re-dates the record");
    // At the audit the armed chain pays the juror; the twin that took the replay does not.
    let audit_ctx = ctx(t.next + 1, t.audit * SPAN, t.next + 1);
    let (paid, _) = fold_step(&kept, &armed, &audit_ctx, &[], None, &extras).unwrap();
    assert!(payout_of(&paid, n).is_some(), "the record kept at S − 2: paid");
    let (unpaid, _) = fold_step(&twin, &params(), &audit_ctx, &[], None, &extras).unwrap();
    assert_eq!(payout_of(&unpaid, n), None, "re-dated to S − 1 by a stranger's replay: not paid");
}
