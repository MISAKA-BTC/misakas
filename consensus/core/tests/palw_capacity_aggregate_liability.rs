//! **ADR-0160 lane liab — aggregate bond liability on testnet-12's own fold** (L-T1…L-T5, L-T7, L-T8
//! and the seat side's measurement L-T4).
//!
//! Every block is checked as `Chain::step` / `Tape::block` check one: the delta re-applies and reverts,
//! and the child's carriage reloads under its committed root (`into_state`: the ledger re-derived, the
//! freeze map's load check). F-L (`Params::palw_capacity_aggregate_liability`) is armed on testnet-12 at a
//! low height beside its twin with the fence `None`, so every test crosses the fence or compares against
//! the dormant chain:
//!
//! * **L-T1** — an intent-class conviction (a proven verdict, `CourtFraud`; the user's decision 1 keeps
//!   the DA default, S1, in the TIER class — its own L-T1 test) past the fence forfeits the WHOLE bond:
//!   collateral 0, every live claim voided `AggregateForfeit`, the bond's own legs of every unmoved
//!   vesting row burned (its producer leg of each row it produced, its seat legs in another producer's
//!   row; every other payee keeps its leg), a final freeze; below the fence (the same
//!   chain, a default before the height) and on the dormant twin, only the S1 tier. A tier conviction
//!   (Eq) freezes and debits only its tier; the freeze lifts `window_court` after the last conviction;
//!   a later intent-class conviction makes it final.
//! * **L-T2** — the freeze's effects one by one: the attempt is skipped (`ProducerFrozen`) and admission
//!   refuses it; the bond is not drawn (the eligible list and the stake base); its `Valid` backs nothing
//!   (no lock, no credit); its exit gate is shut; its own unmatured vesting rows are re-keyed to the lift.
//! * **L-T3** — griefing: the claims of an honest bond fail unattributed (two panels that never answer,
//!   S0′): the bond loses exactly the stage commitments, is never frozen and never forfeited.
//! * **L-T5** — pool drain: a bond that pre-committed its pool as seat locks and claim commitments is
//!   forfeited whole all the same (AG-2 takes the POSTED collateral).
//! * **L-T7** — twin reorg, restart and IBD across a conviction and a freeze lift; below the fence the
//!   armed chain's roots are the dormant chain's, block for block (the empty map is not hashed).
//! * **L-T8** — 13k Sybil pieces: each piece's intent-class conviction forfeits its own whole 13,000
//!   MSK (≥ 3G, the per-conviction loss §4.5 credits), and no other piece is touched.
//! * **L-T4** — the seat side at ρ = 10 / 25 / 100: the duty the fold reserves at bind and the lock it
//!   posts at licence, the seat capital one floor claim holds, and the network's floor claims per DAA
//!   from the eight genesis seats — the lane's own measurement (printed), within 10% of §5.4's rows.
//! * **Review 2** — a held-forfeit refund that reaches a challenger already forfeited whole joins its
//!   forfeiture (the final freeze's load invariant holds, every node reloads the tip); the forger's
//!   own forfeiture makes the honest challenger whole; a bound seat is backed at the licence under
//!   every step (L-T4b); and L-T6 measured on the fold: one conviction's loss at 13k, by route.
//!
//! Run: cargo test -p kaspa-consensus-core --test palw_capacity_aggregate_liability -- --nocapture

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_admission_v2::{PalwAdmissionV2Error, PalwEpochBudgetFencesV1, check_palw_attempt_admission_v2};
use kaspa_consensus_core::palw_aggregate_liability_v1::{
    PalwBondFreezeV1, PalwCapacityLiabilityV1, PalwCapacityStepV1, PalwConvictedOffenceV1, palw_bond_is_frozen_v1,
    palw_producer_conviction_credits_q_v1,
};
use kaspa_consensus_core::palw_da_rcore_v1::{
    PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1, PalwDaClaimV1, PalwDaSessionV1, PalwDaStageV1, PalwDaUnitV1, palw_da_offence_id_v1,
};
use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwHeldForfeitV1, PalwVoidReasonV2, palw_bond_collateral_is_locked_v6, palw_claim_bond_reservation_v1,
    palw_da_disclose_window_daa_v1,
};

/// 1 MSK in sompi.
const MSK: u64 = 100_000_000;

// ---------------------------------------------------------------------------------------------
// The fixture
// ---------------------------------------------------------------------------------------------

/// testnet-12 with F-L armed at `at` over `steps` (`(after, ρ, q‰)` relative to it), mirrored and
/// validated — the value an operator's capacity release sets.
fn t12_with(at: u64, steps: &[(u64, u32, u16)]) -> Params {
    let mut p = t12();
    p.palw_capacity_aggregate_liability = Some(PalwCapacityLiabilityV1 {
        activation: ForkActivation::new(at),
        steps: steps
            .iter()
            .map(|(after, rho, q)| PalwCapacityStepV1 { from_daa: at + after, rho: *rho, q_credit_permille: *q })
            .collect(),
    });
    p.sync_palw_capacity_liability();
    // Stage 2 (rcore/cap-s1, D-23): a credited step needs the audit door at or below it.
    if steps.iter().any(|(_, _, q)| step_credits(*q)) {
        arm_capacity_audit_door(&mut p, at);
    }
    p.validate_palw_v2().unwrap_or_else(|e| panic!("F-L at {at}: {e:?}"));
    p
}

/// testnet-12's own schedule (ρ = 10, nothing credited) from `at`.
fn t12_fl(at: u64) -> Params {
    t12_with(at, &[(0, 10, 0)])
}

/// Genesis bond `i`'s key, bond pubkey and operator pubkey (registry order: 0 is the floor producer,
/// 1..6 the floor seats).
fn genesis_keys(p: &Params, i: usize) -> (PalwBondKeyV2, Vec<u8>, Vec<u8>) {
    bundle(p)
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => {
                Some((*bond, pubkey.clone(), operator_pubkey.clone()))
            }
            _ => None,
        })
        .nth(i)
        .expect("a genesis bond")
}

/// Genesis bonds `range` as panel seats.
fn genesis_seats(p: &Params, range: std::ops::Range<usize>) -> Vec<(PalwBondKeyV2, Hash64)> {
    genesis_bonds(p)[range].iter().map(|(k, o, _)| (*k, *o)).collect()
}

/// A floor attempt by genesis bond `i`, accepted in its own block.
fn claim_by_genesis(c: &mut Chain, i: usize, seed: u64) -> Hash64 {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let (bond, pubkey, operator) = genesis_keys(&c.p, i);
    let pwu = c.floor_pwu(c.daa + 1);
    let (env, key, id) = junk_attempt(floor, bond, pubkey, &operator, pwu, seed, 0x10C0 + seed);
    c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    assert!(c.s.claim(&id).is_some(), "genesis bond {i}'s attempt is accepted");
    id
}

/// A floor attempt by registered bond `n`, accepted in its own block.
fn claim_by(c: &mut Chain, n: u64, seed: u64) -> Hash64 {
    let (env, key, id) = floor_attempt_of(c, n, seed);
    c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    assert!(c.s.claim(&id).is_some(), "bond {n}'s attempt is accepted");
    id
}

/// Registered bond `n`'s next floor attempt folded on the tip without committing it: the skips.
fn probe_attempt_by(c: &Chain, n: u64, seed: u64) -> (bool, Vec<(Hash64, String)>) {
    let (env, key, id) = floor_attempt_of(c, n, seed);
    let at = c.daa + 1;
    let x = PalwBlockContextV2 { block: h(0x7E_0000 + at), daa_score: at, blue_score: at, subsidy: T12_BLOCK_SUBSIDY_SOMPI };
    let (next, _, skips) = c.try_fold(&c.s, &x, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
    (next.claim(&id).is_some(), skips)
}

/// Empty blocks up to `daa`: the one before it and the one at it.
fn run_to(c: &mut Chain, daa: u64) {
    if daa > c.daa + 1 {
        c.step_at(daa - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }
    c.step_at(daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
}

/// `ids` accused by `accuser` in one block; returns the sessions' deadline.
fn da_accuse_all(c: &mut Chain, ids: &[Hash64], accuser: PalwBondKeyV2) -> u64 {
    let objects: Vec<_> = ids.iter().map(|id| da_accuse(*id, accuser, 0)).collect();
    c.step(&objects);
    c.s.da_session(&ids[0], &accuser).expect("the session").deadline_daa
}

/// The accused `ids` run out: each is DA-defaulted (S1) in the sweep of the first block past
/// `deadline`. Returns that block's DAA.
fn da_run_out(c: &mut Chain, ids: &[Hash64], deadline: u64) -> u64 {
    run_to(c, deadline + 1);
    for id in ids {
        let phase = c.claim(id).phase;
        assert!(
            matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
            "{id}: the default voids it for withholding ({phase:?})"
        );
    }
    c.daa
}

/// [`da_accuse_all`] then [`da_run_out`].
fn da_default(c: &mut Chain, ids: &[Hash64], accuser: PalwBondKeyV2) -> u64 {
    let deadline = da_accuse_all(c, ids, accuser);
    da_run_out(c, ids, deadline)
}

fn collateral(c: &Chain, bond: &PalwBondKeyV2) -> u64 {
    c.s.bond(bond).expect("a bond").collateral
}

fn freeze(c: &Chain, bond: &PalwBondKeyV2) -> Option<PalwBondFreezeV1> {
    c.s.bond_freeze_of_v1(bond).copied()
}

fn phase(c: &Chain, id: &Hash64) -> PalwClaimPhaseV2 {
    c.s.claim(id).expect("the claim").phase.clone()
}

fn voided_by(c: &Chain, id: &Hash64, reason: PalwVoidReasonV2) -> bool {
    matches!(phase(c, id), PalwClaimPhaseV2::Voided { reason: r, .. } if r == reason)
}

fn live(c: &Chain, id: &Hash64) -> bool {
    matches!(phase(c, id), PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::ReceiptLicensed { .. })
}

/// B-3's gate on `bond` at the next DAA, the processor's inputs.
fn exit_shut(c: &Chain, bond: &PalwBondKeyV2) -> bool {
    let at = c.daa + 1;
    let raw = c.extras_at(at).settled_anchor_depth;
    palw_bond_collateral_is_locked_v6(&c.s, &c.sp, bond, c.s.bond(bond).unwrap(), at, c.sp.withdrawal_delay_daa(), raw, true)
}

/// The DA default window on testnet-12 (accusation to deadline), measured on a throwaway chain.
fn da_window() -> u64 {
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(1, 50_000 * MSK)]);
    let id = c.floor_claim(0xD0);
    let seats = c.floor_seats();
    c.bind(id, &seats);
    c.step(&[da_accuse(id, bond_key(1), 0)]);
    c.s.da_session(&id, &bond_key(1)).expect("the session").deadline_daa - c.daa
}

/// The admission fences the processor resolves at `daa`, as far as the stateful half reads them.
fn fences(p: &Params, daa: u64) -> PalwEpochBudgetFencesV1 {
    let fold = registry_fold(p, daa).expect("the registry");
    PalwEpochBudgetFencesV1 {
        audit_2026_09_23_active: p.palw_audit_2026_09_23_active_at(daa),
        canonical_work_daa: p.palw_canonical_work_daa(),
        base_known_draw: fold.genesis_works.get(&bundle(p).base_class_id).map(|w| w.economic_ccu_per_claim),
        settled_anchor_depth: extras(p, daa).settled_anchor_depth,
        ..Default::default()
    }
}

/// `claim` licensed by a `Valid` from each of `seats`, signed at `signed`.
fn license(c: &mut Chain, claim: Hash64, seats: &[(PalwBondKeyV2, Hash64)], signed: u64) {
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: seats.iter().map(|(k, _)| valid(claim, *k, signed)).collect() }]);
    assert!(matches!(phase(c, &claim), PalwClaimPhaseV2::ReceiptLicensed { .. }), "the licence folds");
}

// ---------------------------------------------------------------------------------------------
// L-T1 / A-I1 / A-I2: the intent class forfeits the whole bond; below the fence, the tier alone
// ---------------------------------------------------------------------------------------------

/// **An intent-class conviction of `id`'s producer** — rcore/cap-s1: the user's decision 1 keeps a DA
/// default (S1) in the TIER class, so this suite's whole-bond route is a proven court verdict
/// (`CourtFraud`: a court's `ExecutorGuilty` close on an arithmetic proof, L-T6's route): `id` licensed
/// by `seats` if it is still bound, a court opened on it by `challenger`, closed guilty. Returns the
/// conviction's DAA.
fn court_fraud(c: &mut Chain, id: Hash64, seats: &[(PalwBondKeyV2, Hash64)], challenger: PalwBondKeyV2) -> u64 {
    if let PalwClaimPhaseV2::PanelBound { .. } = phase(c, &id) {
        let bound = c.s.panel(&id).expect("bound").bound_daa;
        license(c, id, seats, bound);
    }
    c.step(&[court_opened(&c.s, id, challenger)]);
    let session = court_session_of(&c.s, id, challenger);
    c.step(&[guilty_close(session, false)]);
    assert!(voided_by(c, &id, PalwVoidReasonV2::CourtFraud), "the verdict voids {id} CourtFraud: {:?}", phase(c, &id));
    c.daa
}

/// The court challenger the suite's intent-class convictions use (L-T6's bond 61).
fn court_challenger() -> PalwConsensusObjectV2 {
    bond_obj(61, 400_000 * MSK)
}

/// **L-T1 (decision 1) — a DA default past the fence takes its S1 and a LIFTING freeze, never the
/// bond.** The genesis producer P (a 939,063 MSK card) defaults on claim A below the fence and on A′ past
/// it, on the armed chain beside its dormant twin. Below the fence both chains are one (roots equal).
/// Past it the default charges exactly what it charges on the twin — the claim's commitment (a first
/// strike adds no tier, X11) — P's other claims B′ (bound) and C′ (provisional) live on, and on the armed
/// chain the default writes a TIER freeze (not final) naming the `DaDefault` record: P's next attempt is
/// skipped `ProducerFrozen` and admission refuses it by name, its exit is shut, and the freeze lifts at
/// `since + window_court`, after which P produces again. The reporter reward is on the record's tier
/// debit on both chains. (Before decision 1 the lane forfeited P whole here — its intent class.)
#[test]
fn l_t1_a_da_default_takes_its_s1_and_a_lifting_freeze_past_the_fence_decision_1() {
    let w = da_window();
    let fence = 1_020 + w + 20;
    println!("DA window {w} DAA; F-L at {fence}");
    let mut chains = [Chain::new(t12_fl(fence)), Chain::new(t12())];
    let (producer, _, _) = floor_producer(&chains[0].p);
    let accuser = bond_key(1);
    // Below the fence, on both chains alike.
    let mut below = Vec::new();
    for c in chains.iter_mut() {
        c.step(&[bond_obj(1, 50_000 * MSK)]);
        let a = c.floor_claim(0xA1);
        let seats = c.floor_seats();
        c.bind(a, &seats);
        let b = c.floor_claim(0xB1);
        c.bind(b, &seats);
        let commitment = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&a)).unwrap();
        let before = collateral(c, &producer);
        let at = da_default(c, &[a], accuser);
        assert!(at < fence, "the first default lands below the fence ({at} < {fence})");
        assert_eq!(u128::from(before - collateral(c, &producer)), commitment, "below the fence: S1's forfeit, the commitment alone");
        assert!(live(c, &b), "below the fence: the bond's other claim lives on");
        assert_eq!(freeze(c, &producer), None, "below the fence: no freeze");
        below.push(c.s.state_root());
    }
    assert_eq!(below[0], below[1], "below the fence the armed chain IS the dormant one (the empty map is not hashed)");
    // Past the fence (a new aligned 1,000-DAA epoch, so this default is a strike of its own: X11).
    let mut charges = Vec::new();
    for (armed, c) in [true, false].into_iter().zip(chains.iter_mut()) {
        run_to(c, fence + 1);
        let a = c.floor_claim(0xA2);
        let seats = c.floor_seats();
        c.bind(a, &seats);
        let commitment = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&a)).unwrap();
        let deadline = da_accuse_all(c, &[a], accuser);
        run_to(c, deadline - 6);
        let b = c.floor_claim(0xB2);
        c.bind(b, &seats);
        let cc = c.floor_claim(0xC2);
        let before = collateral(c, &producer);
        let at = da_run_out(c, &[a], deadline);
        let record = c.s.consumed_offence(&palw_da_offence_id_v1(&producer.0, &a)).expect("the DaDefault record").clone();
        let lost = u128::from(before - collateral(c, &producer));
        charges.push((lost, record.collected));
        assert!(collateral(c, &producer) > 0, "armed={armed}: decision 1 — a DA default never takes the whole bond");
        assert!(live(c, &b) && live(c, &cc), "armed={armed}: the bond's other claims live on (a tier freeze voids nothing)");
        assert_eq!(lost, commitment, "armed={armed}: the first strike in its epoch: the commitment alone (X11)");
        let reward = c.s.reward_pending(&palw_da_offence_id_v1(&producer.0, &a)).expect("the accuser's reward is pending").amount;
        assert_eq!(
            reward,
            kaspa_consensus_core::palw_state_v2::palw_reporter_reward_amount_v1(record.collected, 0),
            "armed={armed}: the reporter reward is 49% of the tier debit (decision 4)"
        );
        if !armed {
            assert_eq!(freeze(c, &producer), None, "dormant: no freeze");
            let id = c.floor_claim(0xA3);
            assert!(c.s.claim(&id).is_some(), "dormant: the producer produces");
            continue;
        }
        let f = freeze(c, &producer).expect("AG-3: the first conviction freezes");
        assert!(!f.final_ && f.since_daa == at && f.forfeited_sompi == 0, "a TIER freeze, dated at the default: {f:?}");
        assert_eq!(f.offence_key, palw_da_offence_id_v1(&producer.0, &a), "it names the DaDefault record");
        assert!(exit_shut(c, &producer), "AG-3: the exit is shut while frozen");
        let (floor, _, _, _) = genesis_classes(&c.p)[0];
        let (bond, pubkey, operator) = genesis_keys(&c.p, 0);
        let (env, key, id) = junk_attempt(floor, bond, pubkey, &operator, c.floor_pwu(c.daa + 1), 0xA3, 0x10C0 + 0xA3);
        let x = PalwBlockContextV2 { block: h(0x7E_0000 + c.daa + 1), daa_score: c.daa + 1, blue_score: c.daa + 1, subsidy: 0 };
        let (next, _, skips) = c.try_fold(&c.s, &x, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
        assert!(next.claim(&id).is_none() && skips[0].1.contains("frozen"), "the attempt is skipped ProducerFrozen: {skips:?}");
        let mut env = env;
        env.attempt.artifact_root = c.s.class(&floor).expect("the floor").artifact_root;
        let refused = check_palw_attempt_admission_v2(&c.s, &c.sp, &bundle(&c.p).admission, &x, &env, fences(&c.p, x.daa_score));
        assert!(matches!(refused, Err(PalwAdmissionV2Error::ProducerFrozen { bond: b }) if b == producer), "{refused:?}");
        // The lift: since + window_court, then P produces again.
        let wc = c.sp.window_court();
        run_to(c, at + wc);
        assert_eq!(freeze(c, &producer), None, "a tier freeze lifts at since + window_court");
        let id = c.floor_claim(0xA4);
        assert!(c.s.claim(&id).is_some(), "lifted: the producer produces");
    }
    assert_eq!(charges[0], charges[1], "the default charges the same on both chains: decision 1's tier, not the bond");
}

/// **L-T1 (intent class) with the fence crossed on one chain, beside its dormant twin** — rcore/cap-s1:
/// the intent route is a proven court verdict (`CourtFraud`), decision 1 having left S1 in the tier.
///
/// The genesis producer P is convicted `CourtFraud` on claim A BELOW the fence: the verdict's charge
/// only — its other claim B stays live, no freeze; and the armed chain's root is the dormant twin's.
/// Past the fence P is convicted on A′ with B′ bound and C′ provisional: on the armed chain P's collateral
/// goes to 0, B′ and C′ are voided `AggregateForfeit` (their withheld reward never minted, their seats
/// off duty), the freeze is final, names the conviction's record and counts what the verdict left of the
/// collateral (A-I1: never more than the bond posted), while the record's `collected` — the reporter
/// reward's base — is the verdict's tier debit exactly as on the dormant twin (decision 4: never 10% of
/// a forfeited bond); P's next attempt is skipped (a forfeited bond posts nothing: the producer floor
/// names it) and admission refuses it; P's exit is shut; the voided claims stay convictable through
/// `h_obl` (AG-5); the final freeze never lifts. On the twin: the verdict's charge again, B′ and C′ live,
/// P produces.
#[test]
fn l_t1_an_intent_conviction_forfeits_the_whole_bond_past_the_fence_and_only_its_tier_below_it() {
    let fence = 1_100;
    let mut chains = [Chain::new(t12_fl(fence)), Chain::new(t12())];
    let (producer, _, _) = floor_producer(&chains[0].p);
    let challenger = bond_key(61);
    let c0 = collateral(&chains[0], &producer);
    let mut below = Vec::new();
    for c in chains.iter_mut() {
        c.step(&[bond_obj(1, 50_000 * MSK), court_challenger()]);
        let seats = c.floor_seats();
        let a = c.floor_claim(0xA1);
        c.bind(a, &seats);
        let b = c.floor_claim(0xB1);
        c.bind(b, &seats);
        let before = collateral(c, &producer);
        let at = court_fraud(c, a, &seats, challenger);
        assert!(at < fence, "the first conviction lands below the fence ({at} < {fence})");
        assert!(collateral(c, &producer) > 0 && collateral(c, &producer) < before, "below the fence: the verdict's charge alone");
        assert!(live(c, &b), "below the fence: the bond's other claim lives on");
        assert_eq!(freeze(c, &producer), None, "below the fence: no freeze");
        below.push(c.s.state_root());
    }
    assert_eq!(below[0], below[1], "below the fence the armed chain IS the dormant one (the empty map is not hashed)");
    let mut debits = Vec::new();
    for (armed, c) in [true, false].into_iter().zip(chains.iter_mut()) {
        run_to(c, fence + 1);
        let seats = c.floor_seats();
        let a = c.floor_claim(0xA2);
        c.bind(a, &seats);
        let b = c.floor_claim(0xB2);
        c.bind(b, &seats);
        let cc = c.floor_claim(0xC2);
        let before = collateral(c, &producer);
        let at = court_fraud(c, a, &seats, challenger);
        let key = kaspa_consensus_core::palw_state_v2::palw_court_conviction_offence_id_v1(&producer.0, &a);
        let record = c.s.consumed_offence(&key).expect("the verdict's CourtConviction record").clone();
        debits.push(record.collected);
        let reward = c.s.reward_pending(&key).map(|r| r.amount);
        if armed {
            assert_eq!(collateral(c, &producer), 0, "AG-2: the posted collateral, whole");
            assert!(voided_by(c, &b, PalwVoidReasonV2::AggregateForfeit), "AG-2: the bound claim, voided with the bond");
            assert!(voided_by(c, &cc, PalwVoidReasonV2::AggregateForfeit), "AG-2: the provisional claim too");
            assert!(c.s.panel_duty_row_of(&b).is_none(), "its seats left duty with it");
            let f = freeze(c, &producer).expect("AG-3: the freeze");
            assert!(f.final_ && f.since_daa == at, "an intent-class freeze is final, dated at the conviction: {f:?}");
            assert_eq!(f.offence_key, key, "it names the conviction's record");
            assert!(u128::from(f.forfeited_sompi) <= u128::from(before), "the forfeiture took at most what the verdict left");
            assert!(u128::from(f.forfeited_sompi) + u128::from(record.collected) <= u128::from(c0), "A-I1: never more than the bond ever posted");
            assert!(record.collected <= record.amount, "collected ≤ amount, past the fence too");
            if let Some(reward) = reward {
                assert_eq!(
                    reward,
                    kaspa_consensus_core::palw_state_v2::palw_reporter_reward_amount_v1(record.collected, 0),
                    "decision 4: the reward is on the tier debit, not on the forfeiture"
                );
            }
            println!(
                "armed: P posted {:.2} MSK before the verdict; forfeited {:.2} MSK; the record collected {:.2} MSK (tier); B′/C′ AggregateForfeit",
                before as f64 / MSK as f64,
                f.forfeited_sompi as f64 / MSK as f64,
                record.collected as f64 / MSK as f64,
            );
            assert!(exit_shut(c, &producer), "AG-3: the exit is shut");
            let (floor, _, _, _) = genesis_classes(&c.p)[0];
            let (bond, pubkey, operator) = genesis_keys(&c.p, 0);
            let (env, key, id) = junk_attempt(floor, bond, pubkey, &operator, c.floor_pwu(c.daa + 1), 0xA3, 0x10C0 + 0xA3);
            let x = PalwBlockContextV2 { block: h(0x7E_0000 + c.daa + 1), daa_score: c.daa + 1, blue_score: c.daa + 1, subsidy: 0 };
            let (next, _, skips) = c.try_fold(&c.s, &x, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
            assert!(next.claim(&id).is_none() && skips.len() == 1, "the attempt is skipped: {skips:?}");
            let mut env = env;
            env.attempt.artifact_root = c.s.class(&floor).expect("the floor").artifact_root;
            let refused = check_palw_attempt_admission_v2(&c.s, &c.sp, &bundle(&c.p).admission, &x, &env, fences(&c.p, x.daa_score));
            assert!(matches!(refused, Err(PalwAdmissionV2Error::ProducerBelowFloor { bond: b, collateral: 0, .. }) if b == producer), "{refused:?}");
            let h_obl = c.sp.window_receipt();
            assert!(c.sp.claim_retirement_daa() >= h_obl, "AG-5: claims retire no sooner than h_obl");
            run_to(c, at + h_obl);
            for id in [&a, &b, &cc] {
                assert!(
                    kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(&c.s, id).is_some(),
                    "AG-5: {id} is still a conviction's target at voided + h_obl"
                );
            }
            let wc = c.sp.window_court();
            run_to(c, at + wc + 10);
            assert!(freeze(c, &producer).is_some_and(|f| f.final_), "final: never lifted");
        } else {
            assert!(collateral(c, &producer) > 0, "dormant: the verdict's charge alone");
            assert!(live(c, &b) && live(c, &cc), "dormant: the bond's other claims live on");
            assert_eq!(freeze(c, &producer), None);
            let id = c.floor_claim(0xA3);
            assert!(c.s.claim(&id).is_some(), "dormant: the producer produces");
        }
    }
    assert_eq!(debits[0], debits[1], "the record's collected is the verdict's tier debit on both chains (decision 4's base)");
}

// ---------------------------------------------------------------------------------------------
// L-T1 (tier) / AG-3's lift, extension and upgrade
// ---------------------------------------------------------------------------------------------

/// **A tier conviction (Eq) freezes and takes only its tier; the freeze is re-dated by a later
/// conviction, lifts `window_court` after the last one, and a later intent-class conviction makes it
/// final.** Bond 7 (50,000 MSK) produces a claim, equivocates: Eq's `min(C₀, 3·G_eq)` is debited, its
/// claim stays live (a tier freeze voids nothing), its attempt is skipped and its exit shut. A second
/// Eq 100 DAA later re-dates the freeze. At the re-dated lift minus one the bond is still frozen; the
/// block at the lift lifts it (end of block), and the next attempt is recorded. The dormant twin: the
/// same debits, no freeze, attempts recorded throughout.
#[test]
fn l_t1_an_equivocation_freezes_by_its_tier_lifts_after_window_court_and_an_intent_conviction_makes_it_final() {
    for armed in [true, false] {
        let mut c = Chain::new(if armed { t12_fl(1_001) } else { t12() });
        let (floor, _, _, _) = genesis_classes(&c.p)[0];
        let q = bond_key(7);
        c.step(&[bond_obj(7, 50_000 * MSK), bond_obj(1, 50_000 * MSK), court_challenger()]);
        let q1 = claim_by(&mut c, 7, 0x71);
        let seats = c.floor_seats();
        c.bind(q1, &seats);
        let (eq, evidence) = equivocation_of(q, floor, 0xE71);
        let before = collateral(&c, &q);
        c.step(&[eq]);
        let first = c.daa;
        let record = c.s.consumed_offence(&equivocation_key(q, evidence)).expect("the Eq record").clone();
        assert_eq!(before - collateral(&c, &q), record.collected, "armed={armed}: Eq's tier, and only it");
        assert!(collateral(&c, &q) > 0, "armed={armed}: a tier conviction leaves the rest of the bond");
        assert!(live(&c, &q1), "armed={armed}: a tier freeze voids nothing");
        let wc = c.sp.window_court();
        if !armed {
            assert_eq!(freeze(&c, &q), None);
            assert!(probe_attempt_by(&c, 7, 0x72).0, "dormant: bond 7 produces");
            continue;
        }
        let f = freeze(&c, &q).expect("AG-3: the first conviction freezes");
        assert!(!f.final_ && f.since_daa == first && f.forfeited_sompi == 0, "a tier freeze: {f:?}");
        let (recorded, skips) = probe_attempt_by(&c, 7, 0x72);
        assert!(!recorded && skips[0].1.contains("frozen"), "frozen: the attempt is skipped ProducerFrozen: {skips:?}");
        let (env, _, _) = floor_attempt_of(&c, 7, 0x72);
        let at = c.daa + 1;
        let x = PalwBlockContextV2 { block: h(0x7E_0000 + at), daa_score: at, blue_score: at, subsidy: T12_BLOCK_SUBSIDY_SOMPI };
        let refused = check_palw_attempt_admission_v2(&c.s, &c.sp, &bundle(&c.p).admission, &x, &env, fences(&c.p, at));
        assert!(matches!(refused, Err(PalwAdmissionV2Error::ProducerFrozen { bond: b }) if b == q), "admission refuses it by name: {refused:?}");
        assert!(exit_shut(&c, &q), "frozen: the exit is shut");
        // A second conviction re-dates it.
        run_to(&mut c, first + 100);
        let (eq2, _) = equivocation_of(q, floor, 0xE72);
        c.step(&[eq2]);
        let second = c.daa;
        assert_eq!(freeze(&c, &q).map(|f| (f.since_daa, f.final_)), Some((second, false)), "re-dated by the later conviction");
        // The lift: through the block before it, frozen; the block at it lifts it at its end.
        run_to(&mut c, second + wc - 1);
        assert!(freeze(&c, &q).is_some(), "still frozen at the lift minus one");
        assert!(!probe_attempt_by(&c, 7, 0x73).0, "and the block at the lift still refuses its attempt");
        c.step(&[]);
        assert_eq!(c.daa, second + wc);
        assert_eq!(freeze(&c, &q), None, "lifted at the end of the block at since + window_court");
        let q2 = claim_by(&mut c, 7, 0x74);
        // A later intent-class conviction (a proven verdict; decision 1 left S1 in the tier): final, whole.
        c.bind(q2, &seats);
        let at = court_fraud(&mut c, q2, &seats, bond_key(61));
        let f = freeze(&c, &q).expect("frozen again");
        assert!(f.final_ && f.since_daa == at, "an intent-class conviction makes it final: {f:?}");
        assert_eq!(collateral(&c, &q), 0, "and takes the rest of the bond");
    }
}

// ---------------------------------------------------------------------------------------------
// L-T2: the freeze's effects, one by one
// ---------------------------------------------------------------------------------------------

/// **L-T2 — a frozen seat and a frozen producer, effect by effect.** A genesis seat S is tier-frozen
/// (Eq) while bound on the producer's claim X:
/// * **not drawn**: S leaves the eligible list of a pending claim and the stake draw's base;
/// * **its `Valid` backs nothing**: X is licensed by all five seats' `Valid`s; the four others lock and
///   are credited, S takes no lock (the licence stands on the other four);
/// * **exit held**: S's gate is shut by the freeze alone once its other obligations are gone.
///
/// The producer P reaches `Final` on X (a vesting row: P's leg and the seats'), then is tier-frozen:
/// its unlatched row is re-keyed to the freeze's lift (no maturity under the freeze) and it produces
/// nothing until the lift.
#[test]
fn l_t2_the_freeze_effects_one_by_one() {
    let mut c = Chain::new(t12_fl(1_001));
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let (producer, _, _) = floor_producer(&c.p);
    let seats = c.floor_seats();
    let s = seats[1].0;
    let x = c.floor_claim(0xC1);
    let bound = c.bind(x, &seats);
    let pending = c.floor_claim(0xC2);
    let eligible = |c: &Chain, claim: &Hash64| -> Vec<PalwBondKeyV2> {
        kaspa_consensus_core::palw_panel_v2::palw_panel_eligible_bonds_v2(&c.s, claim, c.sp.min_collateral_sompi(), None, false, None, None, 5)
            .expect("the population")
            .into_iter()
            .map(|(k, _)| *k)
            .collect()
    };
    let base = |c: &Chain, claim: &Hash64| -> Vec<PalwBondKeyV2> {
        kaspa_consensus_core::palw_panel_v2::palw_panel_stake_base_bonds_judging_v1(&c.s, claim, &floor, c.sp.min_collateral_sompi(), None, false, None, None, 5)
            .expect("the base")
            .into_iter()
            .map(|(k, _)| *k)
            .collect()
    };
    assert!(eligible(&c, &pending).contains(&s) && base(&c, &pending).contains(&s), "the premise: S is drawable");
    let (eq, _) = equivocation_of(s, floor, 0xE5);
    c.step(&[eq]);
    let since = c.daa;
    assert!(palw_bond_is_frozen_v1(&c.s, &s), "S is frozen");
    assert!(!eligible(&c, &pending).contains(&s), "not drawn");
    assert!(!base(&c, &pending).contains(&s), "and not in the stake draw's base (a frozen stake does not dilute the floor)");
    // Its `Valid` backs nothing: the licence stands on the other four.
    license(&mut c, x, &seats, bound);
    assert!(c.s.slashable_lock(s, x).is_none(), "the frozen seat takes no lock");
    for (k, _) in seats.iter().filter(|(k, _)| *k != s) {
        assert!(c.s.slashable_lock(*k, x).is_some(), "{k:?}: a backed Valid locks");
    }
    assert!(exit_shut(&c, &s), "the frozen seat's exit is shut");
    // The producer's row: Final, then a tier freeze re-keys it to the lift.
    c.finalize(x);
    let row = c.s.vesting_row(&x).expect("a Final past R-core+ writes the row").clone();
    assert!(row.matured_at.is_none() && row.seats.iter().all(|(k, _)| *k != s), "S was not credited: {row:?}");
    let later = c.daa + 50;
    run_to(&mut c, later);
    let (eq, _) = equivocation_of(producer, floor, 0xE6);
    c.step(&[eq]);
    let p_since = c.daa;
    let wc = c.sp.window_court();
    let rekeyed = c.s.vesting_row(&x).expect("the row stands").clone();
    assert!(row.expiry_daa < p_since + wc, "the premise: the row would have matured under the freeze");
    assert_eq!(rekeyed.expiry_daa, p_since + wc, "re-keyed to the freeze's lift");
    // S's own freeze lifts first (it is older than X's Final); then X's original expiry passes with P
    // frozen; then P's own lift.
    assert!(since + wc < row.expiry_daa && row.expiry_daa < p_since + wc, "the premise: the order of the three clocks");
    run_to(&mut c, since + wc);
    assert!(!palw_bond_is_frozen_v1(&c.s, &s), "S lifted at since + window_court");
    assert!(palw_bond_is_frozen_v1(&c.s, &producer), "P still frozen");
    run_to(&mut c, row.expiry_daa);
    assert!(c.s.vesting_row(&x).is_some_and(|r| r.matured_at.is_none()), "no maturity under the freeze");
    assert!(!probe_attempt_by_genesis(&c, 0, 0xC3), "the frozen producer produces nothing");
    run_to(&mut c, p_since + wc);
    assert!(!palw_bond_is_frozen_v1(&c.s, &producer), "P lifted");
    assert!(probe_attempt_by_genesis(&c, 0, 0xC4), "and produces again");
}

/// Genesis bond `i`'s next floor attempt folded on the tip without committing it: recorded?
fn probe_attempt_by_genesis(c: &Chain, i: usize, seed: u64) -> bool {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let (bond, pubkey, operator) = genesis_keys(&c.p, i);
    let (env, key, id) = junk_attempt(floor, bond, pubkey, &operator, c.floor_pwu(c.daa + 1), seed, 0x10C0 + seed);
    let at = c.daa + 1;
    let x = PalwBlockContextV2 { block: h(0x7E_0000 + at), daa_score: at, blue_score: at, subsidy: T12_BLOCK_SUBSIDY_SOMPI };
    let (next, _, _) = c.try_fold(&c.s, &x, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
    next.claim(&id).is_some()
}

// ---------------------------------------------------------------------------------------------
// L-T3 (AG-4) and L-T5 (pool drain)
// ---------------------------------------------------------------------------------------------

/// **L-T3 — griefing an honest bond costs at most its stage commitments and never freezes it.** An
/// honest 100,000 MSK bond makes ten floor claims; every one is bound to panels whose seats never
/// answer (the griefers' partial silence), twice — the second `ReceiptTimeout` voids it S0′ and
/// forfeits the stage commitment, and nothing else. The bond loses exactly Σ commitment, is never
/// frozen, never forfeited, and keeps producing — past the fence exactly as on the dormant twin.
#[test]
fn l_t3_griefing_an_honest_bond_costs_its_stage_commitments_and_never_freezes_it() {
    const N: u64 = 10;
    let mut roots = Vec::new();
    for armed in [true, false] {
        let mut c = Chain::new(if armed { t12_fl(1_001) } else { t12() });
        c.step(&[bond_obj(9, 100_000 * MSK)]);
        let honest = bond_key(9);
        let posted = collateral(&c, &honest);
        let ids: Vec<Hash64> = (0..N).map(|i| claim_by(&mut c, 9, 0x900 + i)).collect();
        let owed: u128 = ids.iter().map(|id| palw_claim_bond_reservation_v1(&c.sp, &c.claim(id)).unwrap()).sum();
        let seats = c.floor_seats();
        for _ in 0..2 {
            let bind: Vec<_> = ids
                .iter()
                .map(|id| PalwConsensusObjectV2::PanelBound { claim: *id, anchor: h(0xAC_0000 + c.daa), seats: seats_of(&seats) })
                .collect();
            c.step(&bind);
            let bound = c.daa;
            let claim = c.claim(&ids[0]);
            let rw = c.sp.receipt_window_for_claim_v1(
                &c.s,
                &claim.class_id,
                kaspa_consensus_core::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(&claim),
                bound,
            );
            run_to(&mut c, bound + rw + 1);
            assert!(freeze(&c, &honest).is_none(), "armed={armed}: an unattributed failure never freezes");
        }
        for id in &ids {
            assert!(voided_by(&c, id, PalwVoidReasonV2::ReceiptTimeout), "armed={armed}: S0′ on {id}");
        }
        let lost = u128::from(posted - collateral(&c, &honest));
        println!("armed={armed}: {N} griefed claims cost {:.2} MSK = Σ stage commitment {:.2} MSK", lost as f64 / 1e8, owed as f64 / 1e8);
        assert_eq!(lost, owed, "armed={armed}: honest loss is exactly Σ commitment (≤ Σ m_c), never the pool");
        assert!(probe_attempt_by(&c, 9, 0x9FF).0, "armed={armed}: the honest bond keeps producing");
        roots.push(c.s.state_root());
    }
    // Past the fence the seat duties differ (AS-1 ÷ρ), so the roots may; the honest bond's fate is one.
}

/// **L-T5 — a pool pre-committed as seat locks and claim commitments is forfeited whole (AG-2 takes
/// the POSTED collateral), and a seat's legs in another producer's row are burned leg by leg.** A
/// genesis seat S sits on the producer's claim X, which is licensed (S locks) and reaches `Final` (S's
/// seat leg in X's row), and holds its own claims; its committed ledger is far above zero. S is convicted
/// on its own claim (intent class: a proven verdict, rcore/cap-s1's decision-1 route): S's collateral
/// goes to 0 whatever it had committed; its seat leg in X's row is burned (the producer's and the other
/// seats' legs stand); its other live claim is voided `AggregateForfeit`.
#[test]
fn l_t5_a_pre_committed_pool_is_forfeited_whole_and_a_seat_s_legs_burn_leg_by_leg() {
    let mut c = Chain::new(t12_fl(1_001));
    c.step(&[bond_obj(1, 50_000 * MSK), court_challenger()]);
    let seats = c.floor_seats();
    let s = seats[0].0;
    let x = c.floor_claim(0xF1);
    let bound = c.bind(x, &seats);
    license(&mut c, x, &seats, bound);
    c.finalize(x);
    let row = c.s.vesting_row(&x).expect("X's row").clone();
    let s_leg: u64 = row.seats.iter().filter(|(k, _)| *k == s).map(|(_, p)| p.amount).sum();
    assert!(s_leg > 0, "the premise: S holds a seat leg in X's row: {row:?}");
    // S's own claims: one to default on, one live beside it (seated on genesis bonds 2..7, not S).
    let own_seats = genesis_seats(&c.p, 2..7);
    let s_index = genesis_bonds(&c.p).iter().position(|(k, _, _)| *k == s).expect("S is a genesis bond");
    let d = claim_by_genesis(&mut c, s_index, 0xF2);
    c.bind(d, &own_seats);
    let e = claim_by_genesis(&mut c, s_index, 0xF3);
    c.bind(e, &own_seats);
    let at = c.daa + 1;
    let committed = kaspa_consensus_core::palw_state_v2::palw_bond_committed_raw_v1(&c.s, &c.sp, &s, at, c.extras_at(at).settled_anchor_depth);
    assert!(committed > 0, "the premise: S's pool is committed (claims, duties, the lock on X)");
    let posted = collateral(&c, &s);
    println!("S posts {:.2} MSK with {:.2} MSK committed before the conviction", posted as f64 / 1e8, committed as f64 / 1e8);
    court_fraud(&mut c, d, &own_seats, bond_key(61));
    assert_eq!(collateral(&c, &s), 0, "AG-2 takes the posted collateral, whatever was committed");
    assert!(voided_by(&c, &e, PalwVoidReasonV2::AggregateForfeit), "S's other live claim goes with it");
    let burned = c.s.vesting_row(&x).expect("X's row stands").clone();
    assert!(burned.seats.iter().all(|(k, _)| *k != s), "S's seat leg left X's row");
    assert_eq!(burned.producer, row.producer, "the producer's leg stands");
    assert_eq!(
        burned.seats.iter().map(|(_, p)| p.amount).sum::<u64>() + s_leg,
        row.seats.iter().map(|(_, p)| p.amount).sum::<u64>(),
        "the other seats' legs stand"
    );
    let f = freeze(&c, &s).expect("final");
    assert!(f.final_ && f.forfeited_sompi >= s_leg, "the forfeiture counts the burned leg: {f:?}");
    assert!(c.s.vesting_counters().burned >= u128::from(s_leg), "burned, never minted");
}

/// **AG-2 takes the forfeited producer's REWARD, never its seats' (review finding 1; AG-4, ADR §4.5
/// "burns every unmatured reward of the bond").** X is a claim of the genesis producer P, licensed by
/// five seats and taken to `Final`: its row pays P's leg and each credited seat's. P is then convicted
/// on a DIFFERENT claim D past the fence (intent class: a proven verdict, rcore/cap-s1). On the armed chain X's row stands with P's leg
/// at 0 — exactly that leg burned (`vesting_burned`, and counted in the freeze's forfeiture) — and
/// every seat's leg, the reserve and the row's clocks as they were; on the dormant twin P keeps its
/// leg. Both chains then settle the second clock's depth of anchors (bond 2's licensed claims) and run
/// past X's expiry: X's row moves on both, and the armed chain moves exactly P's leg less — the
/// honest seats are paid.
#[test]
fn l_t5b_a_producer_s_forfeiture_burns_its_own_leg_and_the_seats_keep_theirs() {
    let mut rows = Vec::new();
    let mut moved = Vec::new();
    for armed in [true, false] {
        let mut c = Chain::new(if armed { t12_fl(1_001) } else { t12() });
        c.step(&[bond_obj(1, 50_000 * MSK), bond_obj(2, 500_000 * MSK), court_challenger()]);
        let (producer, _, _) = floor_producer(&c.p);
        let seats = c.floor_seats();
        let x = c.floor_claim(0xF1);
        let bound = c.bind(x, &seats);
        license(&mut c, x, &seats, bound);
        c.finalize(x);
        let row = c.s.vesting_row(&x).expect("X's row").clone();
        assert!(
            row.producer_bond == producer
                && row.producer.amount > 0
                && row.seats.len() == seats.len()
                && row.seats.iter().all(|(_, leg)| leg.amount > 0),
            "the premise: X's row pays P and its five seats: {row:?}"
        );
        let burned_before = c.s.vesting_counters().burned;
        let d = c.floor_claim(0xF2);
        c.bind(d, &seats);
        let at = court_fraud(&mut c, d, &seats, bond_key(61));
        assert!(at > 1_001, "the conviction lands past the fence ({at})");
        let after = c.s.vesting_row(&x).expect("X's row stands on both chains").clone();
        let burned = c.s.vesting_counters().burned - burned_before;
        assert_eq!(after.seats, row.seats, "armed={armed}: every credited seat keeps its leg");
        assert_eq!(
            (after.reserve, after.expiry_daa, after.settled_at_final, after.matured_at),
            (row.reserve, row.expiry_daa, row.settled_at_final, row.matured_at),
            "armed={armed}: the reserve and the row's clocks as they were"
        );
        if armed {
            assert_eq!(after.producer.amount, 0, "P's own leg is burned");
            assert_eq!(after.producer.payload, row.producer.payload, "the leg keeps its payee");
            assert_eq!(burned, u128::from(row.producer.amount), "exactly P's leg went to vesting_burned");
            let f = freeze(&c, &producer).expect("the final freeze");
            assert!(f.final_ && f.forfeited_sompi >= row.producer.amount, "the forfeiture counts P's leg: {f:?}");
        } else {
            assert_eq!(after.producer, row.producer, "dormant: P keeps its leg");
            assert_eq!(burned, 0, "dormant: X's row burns nothing");
        }
        println!(
            "armed={armed}: X's row after P's default at {at}: producer {:.2} MSK (was {:.2}), {} seat legs {:.2} MSK, reserve {:.2}; burned {:.2} MSK",
            after.producer.amount as f64 / 1e8,
            row.producer.amount as f64 / 1e8,
            after.seats.len(),
            after.seats.iter().map(|(_, leg)| leg.amount).sum::<u64>() as f64 / 1e8,
            after.reserve as f64 / 1e8,
            burned as f64 / 1e8
        );
        // The second clock: `depth` anchors settle after X's Final (bond 2's licensed claims), then
        // the DAA clock runs out and X's row latches and moves.
        let depth = c.extras_at(c.daa + 1).settled_anchor_depth.expect("testnet-12 has a second clock");
        for k in 0..=depth {
            let id = claim_by(&mut c, 2, 0x5E00 + k);
            let bound = c.bind(id, &seats);
            license(&mut c, id, &seats, bound);
        }
        let moved_before = c.s.vesting_counters().moved;
        run_to(&mut c, row.expiry_daa + 1);
        assert!(c.s.vesting_row(&x).is_none(), "armed={armed}: X's row matured and moved");
        moved.push(c.s.vesting_counters().moved - moved_before);
        rows.push(row);
    }
    assert_eq!(rows[0], rows[1], "the same row on both chains before the default");
    assert_eq!(
        moved[1] - moved[0],
        u128::from(rows[0].producer.amount),
        "the armed chain moved exactly P's leg less: every seat's leg and the reserve were paid"
    );
    println!("moved at X's maturity: armed {:.2} MSK, dormant {:.2} MSK", moved[0] as f64 / 1e8, moved[1] as f64 / 1e8);
}

// ---------------------------------------------------------------------------------------------
// L-T7: reorg, restart and IBD across a conviction and a lift; below the fence, the dormant root
// ---------------------------------------------------------------------------------------------

/// The scripted run L-T7 records: bond 7 produces and equivocates (a tier freeze), the chain runs past
/// the lift, bond 7 produces again; then the genesis producer is convicted by a proven verdict (the
/// intent class — rcore/cap-s1's decision-1 route — final).
fn l_t7_script(t: &mut Tape) -> (usize, usize, usize) {
    let (floor, _, _, _) = genesis_classes(&t.c.p)[0];
    t.step(vec![bond_obj(7, 50_000 * MSK), bond_obj(1, 50_000 * MSK), court_challenger()]);
    let q1 = t.attempt(Some(7), 0x71);
    t.bind(q1);
    let before_conviction = t.len();
    let (eq, _) = equivocation_of(bond_key(7), floor, 0xE71);
    t.step(vec![eq]);
    let since = t.c.daa;
    let wc = t.c.sp.window_court();
    t.at(since + wc - 1, vec![]);
    t.at(since + wc, vec![]);
    let after_lift = t.len();
    t.attempt(Some(7), 0x72);
    let a = t.attempt(None, 0x7A);
    t.bind(a);
    t.attempt(None, 0x7B);
    let seats = t.c.floor_seats();
    let bound = t.c.s.panel(&a).expect("bound").bound_daa;
    t.step(vec![PalwConsensusObjectV2::ReceiptLicensed { claim: a, receipts: seats.iter().map(|(k, _)| valid(a, *k, bound)).collect() }]);
    t.step(vec![court_opened(&t.c.s, a, bond_key(61))]);
    let session = court_session_of(&t.c.s, a, bond_key(61));
    t.step(vec![guilty_close(session, false)]);
    (before_conviction, after_lift, t.len())
}

/// **L-T7 / A-I6 — the freeze map is chain data: reorg, restart and IBD agree across a conviction, a
/// lift and a forfeiture; and below the fence the armed chain roots as the dormant one.**
#[test]
fn l_t7_reorg_restart_and_ibd_agree_across_a_conviction_a_lift_and_a_forfeiture() {
    let mut t = Tape::new(Chain::new(t12_fl(1_001)));
    let (before_conviction, after_lift, tip) = l_t7_script(&mut t);
    let (producer, _, _) = floor_producer(&t.c.p);
    assert!(t.c.s.bond_freeze_of_v1(&producer).is_some_and(|f| f.final_), "the script ends on a final freeze");
    assert!(t.state_at(before_conviction + 1).bond_freeze_of_v1(&bond_key(7)).is_some(), "bond 7 frozen by the Eq");
    assert!(t.state_at(after_lift).bond_freeze_of_v1(&bond_key(7)).is_none(), "and lifted");
    assert_ne!(
        t.state_at(before_conviction + 1).state_root(),
        t.state_at(before_conviction).state_root(),
        "a freeze moves the root"
    );
    t.revert_to_base_and_reapply();
    for j in [0, before_conviction, before_conviction + 1, after_lift - 1, after_lift, tip - 1] {
        t.restart_at(j);
    }
    t.ibd_from(t.base.clone());
    // A sibling forked before the conviction that never equivocates: the reorg to it and back.
    let mut sibling = t.fork(before_conviction);
    sibling.step(vec![]);
    sibling.attempt(Some(7), 0x7F);
    assert!(sibling.c.s.bond_freeze_of_v1(&bond_key(7)).is_none(), "no conviction, no freeze on the sibling");
    t.reorg_to(before_conviction, &sibling);
    // Below the fence: the same script on a chain whose fence has not come is the dormant chain, root
    // for root (the empty map is not hashed; no claim is priced by a step).
    let mut far = Tape::new(Chain::new(t12_fl(900_000)));
    let mut dormant = Tape::new(Chain::new(t12()));
    l_t7_script(&mut far);
    l_t7_script(&mut dormant);
    assert_eq!(far.len(), dormant.len());
    for j in 1..=far.len() {
        assert_eq!(far.state_at(j).state_root(), dormant.state_at(j).state_root(), "block {j}: below the fence, the dormant root");
    }
    assert!(dormant.c.s.bond_freeze_of_v1(&producer).is_none(), "the dormant chain freezes nobody");
}

// ---------------------------------------------------------------------------------------------
// L-T8: 13k Sybil pieces
// ---------------------------------------------------------------------------------------------

/// **L-T8 — splitting into 13,000 MSK pieces buys no escape from the per-conviction loss.** Four
/// pieces at the producer floor each make a claim; one piece is convicted by a proven verdict (the
/// intent class; rcore/cap-s1's decision-1 route): that piece
/// forfeits its whole 13,000 MSK — at least 3G of its claim, the L §4.5 credits per conviction — and
/// the other three pieces are untouched (not frozen, their collateral and claims as they were). The
/// same 52,000 MSK as one bond with four claims forfeits all 52,000 MSK and voids the other three.
#[test]
fn l_t8_each_13k_piece_forfeits_its_own_whole_bond_and_no_other() {
    let floor_msk = 13_000 * MSK;
    let mut c = Chain::new(t12_fl(1_001));
    c.step(&[
        bond_obj(1, 50_000 * MSK),
        bond_obj(11, floor_msk),
        bond_obj(12, floor_msk),
        bond_obj(13, floor_msk),
        bond_obj(14, floor_msk),
        bond_obj(20, 4 * floor_msk),
        court_challenger(),
    ]);
    let seats = c.floor_seats();
    let first = claim_by(&mut c, 11, 0xB0B);
    c.bind(first, &seats);
    let three_g = 3 * palw_claim_bond_reservation_v1(&c.sp, &c.claim(&first)).unwrap();
    let pieces: Vec<(u64, Hash64)> = (12..=14).map(|n| (n, claim_by(&mut c, n, 0xB00 + n))).collect();
    let before: Vec<u64> = (11..=14).map(|n| collateral(&c, &bond_key(n))).collect();
    court_fraud(&mut c, first, &seats, bond_key(61));
    let record = c
        .s
        .consumed_offence(&kaspa_consensus_core::palw_state_v2::palw_court_conviction_offence_id_v1(&bond_key(11).0, &first))
        .unwrap()
        .clone();
    assert_eq!(collateral(&c, &bond_key(11)), 0, "the piece forfeits its whole bond");
    assert!(freeze(&c, &bond_key(11)).is_some_and(|f| f.final_), "and is frozen for good");
    let lost = u128::from(before[0]);
    assert!(lost >= three_g.min(u128::from(floor_msk)), "≥ 3G per conviction (or the whole piece)");
    assert!(record.collected <= record.amount, "the record's collected is the tier debit (the reporter reward's base)");
    println!(
        "a 13k piece's conviction takes {:.2} MSK (3G = {:.2} MSK; the record's tier debit {:.2} MSK)",
        lost as f64 / 1e8,
        three_g as f64 / 1e8,
        record.collected as f64 / 1e8
    );
    for (i, (n, id)) in pieces.iter().enumerate() {
        assert_eq!(collateral(&c, &bond_key(*n)), before[i + 1], "piece {n} untouched");
        assert!(freeze(&c, &bond_key(*n)).is_none() && live(&c, id), "piece {n} neither frozen nor voided");
    }
    // The same 52,000 MSK as ONE bond: one conviction takes all of it, and voids its other claims.
    let whole = claim_by(&mut c, 20, 0xC00);
    c.bind(whole, &seats);
    let others: Vec<Hash64> = (1..4).map(|i| claim_by(&mut c, 20, 0xC00 + i)).collect();
    let whole_before = collateral(&c, &bond_key(20));
    assert!(whole_before > 3 * floor_msk, "the premise: the whole bond is worth more than three pieces");
    court_fraud(&mut c, whole, &seats, bond_key(61));
    assert_eq!(collateral(&c, &bond_key(20)), 0, "one bond: the whole of it");
    for id in &others {
        assert!(voided_by(&c, id, PalwVoidReasonV2::AggregateForfeit), "and its other claims go with it");
    }
}

// ---------------------------------------------------------------------------------------------
// L-T4: the seat side at ρ = 10 / 25 / 100, measured on the fold
// ---------------------------------------------------------------------------------------------

/// One floor claim priced on a chain with F-L at 1,001 over `(ρ, q)` (`None`: the fence dormant): what
/// the fold reserved at bind (duty per seat) and posted at licence (lock per `Valid`), beside the bind's
/// own inputs (`palw_rcore_bind_prices_v1` at the bind: the λ-term and `lock_2`) and the claim's `E`/`w`.
struct SeatPrices {
    duty: u128,
    lock: u128,
    commitment: u128,
    lambda: u128,
    lock_2: u128,
    escrow: u128,
    weight: u128,
}

fn seat_prices(step: Option<(u32, u16)>) -> SeatPrices {
    let p = match step {
        Some((rho, q)) => t12_with(1_001, &[(0, rho, q)]),
        None => t12(),
    };
    let mut c = Chain::new(p);
    let x = c.floor_claim(0x5EA7);
    let seats = c.floor_seats();
    let at = c.daa + 1;
    let claim = c.claim(&x);
    let inputs = kaspa_consensus_core::palw_state_v2::palw_rcore_bind_prices_v1(&c.s, &c.sp, &c.extras_at(at), &x, &claim, seats.len(), at);
    let bound = c.bind(x, &seats);
    let duty = c.s.panel_duty_row_of(&x).expect("the duty row").seat_exposure;
    assert_eq!(duty, inputs.duty_bind, "the fold reserves the bind's own price");
    let commitment = palw_claim_bond_reservation_v1(&c.sp, &claim).unwrap();
    assert!(duty * seats.len() as u128 <= commitment, "A-I5: seats × duty′ ≤ commitment′");
    // The coverage door (§5.4's door: every seat on its assigned segments, recount k = 2).
    let anchor = c.anchor(&x);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: x, receipts: covered(x, anchor, &seats, &[0, 1, 2, 3, 4], bound) }]);
    assert!(matches!(phase(&c, &x), PalwClaimPhaseV2::ReceiptLicensed { .. }), "the coverage licence folds");
    assert_eq!(c.claim(&x).rcore.basis_k, 2, "coverage recounts k = 2");
    let locks: Vec<u128> = seats.iter().map(|(k, _)| c.s.slashable_lock(*k, x).expect("a lock").amount).collect();
    assert!(locks.iter().all(|l| *l >= 1 && *l == locks[0]), "one price per set, lock′ ≥ 1 sompi: {locks:?}");
    SeatPrices {
        duty,
        lock: locks[0],
        commitment,
        lambda: inputs.lambda_term,
        lock_2: inputs.lock_2,
        escrow: u128::from(claim.escrowed_reward),
        weight: claim.reserved,
    }
}

/// **L-T4 — the seat side at each step, measured on the fold, and the network's floor seat capital
/// DERIVED from it.** The duty and the lock are what the fold actually reserves at the bind and posts
/// at the licence (read back from the seat ledger). The network rate is §5.4's Little's-law steady
/// state over those prices, not a pipeline run: per floor claim the seats hold `5 · duty · 122 + 5 ·
/// lock · 3,000` MSK·DAA (ADR-0160 Appendix A: the duty bind → Final, the lock to F + 3,000); eight
/// genesis seats post 8 × 469,531.6 MSK of work room, so the network licenses `3,756,253 / capital`
/// floor claims per DAA — ≈ 0.94 today. (The processor capacity harness — `A_floor_small` /
/// `K_floor_big`, 160–200 DAA at ≈ 11 claims/DAA — cannot reach that steady state: the lock lives to
/// F + 3,000, so a 160-DAA run shows no saturation at ρ = 10 whatever the credit, and licence
/// carriage binds at ≈ 13/DAA before seat capital from ρ = 25 on.)
///
/// * **This lane alone** (the commitment still `E + w`): the fold's duty is AS-1's
///   `duty_bind(⌈λ/ρ⌉, lock′, E + w, 5)` with `lock′` AS-2's lock — `⌈lock/ρ⌉` once the credit
///   reaches `q_seat` (250‰), today's lock below it — both asserted against the formula, and the rate
///   printed.
/// * **With the escrow lane's commitment** (`m_c = ⌈E/ρ⌉` at that credit, so `commitment′ = ⌈E/ρ⌉ + w`),
///   the duty the same formula gives (a seat reserving at least the lock it will post, as that lane's
///   bind does) and the lock measured here reach §5.4's rows 9.4 / 23.5 / 94 — each within 10%.
/// * **With nothing credited (q = 0) the claim's seats are priced exactly as today, ×1.00** — stage 2
///   (rcore/cap-s1, ADR-0160 v3 AS-1′): the step prices a CREDITED claim's seats only, and a credit rides
///   the audit door (D-23; the fixture arms it with a credited step). The lane's AS-1 divided the λ-term
///   of every claim at every step (×1.07 at q = 0); the ×ρ rows stay conditional on a step crediting
///   q ≥ q_seat (250‰), review finding 3.
#[test]
fn l_t4_the_seat_side_at_rho_10_25_100_and_the_network_floor_seat_capital() {
    use kaspa_consensus_core::palw_aggregate_liability_v1::palw_seat_duty_v2;
    const SEAT_ROOM_MSK: f64 = 8.0 * 469_531.6;
    let capital = |duty: u128, lock: u128| -> f64 { (5.0 * duty as f64 * 122.0 + 5.0 * lock as f64 * 3_000.0) / 1e8 };
    let today = seat_prices(None);
    let today_rate = SEAT_ROOM_MSK / capital(today.duty, today.lock);
    println!(
        "today: λ-term {:.2} MSK, lock_2 {:.2} MSK, commitment {:.2} MSK (E {:.2} + w {:.4}) → duty {:.2} MSK, lock {:.2} MSK → {:.0} \
         MSK·DAA per claim → {:.2} floor claims/DAA",
        today.lambda as f64 / 1e8,
        today.lock_2 as f64 / 1e8,
        today.commitment as f64 / 1e8,
        today.escrow as f64 / 1e8,
        today.weight as f64 / 1e8,
        today.duty as f64 / 1e8,
        today.lock as f64 / 1e8,
        capital(today.duty, today.lock),
        today_rate
    );
    assert!((today_rate - 0.94).abs() / 0.94 < 0.10, "today's seat capital is §5.4's 0.94/DAA within 10% ({today_rate:.3})");
    for (rho, q, want) in [(10u32, 250u16, 9.4), (25, 250, 23.5), (100, 250, 94.0), (10, 0, 0.0), (100, 0, 0.0)] {
        let step = kaspa_consensus_core::palw_aggregate_liability_v1::PalwCapacityStepV1 { from_daa: 1_001, rho, q_credit_permille: q };
        let m = seat_prices(Some((rho, q)));
        // Stage 2 (v3 AS-1′): the step prices the credited claim's seats; an uncredited claim's are today's.
        let credited = q >= 250;
        assert_eq!(
            m.duty,
            palw_seat_duty_v2(today.lambda, today.lock_2, today.commitment, 5, credited.then_some(step)),
            "ρ = {rho}: AS-1's formula on the bind's own inputs"
        );
        let alone = SEAT_ROOM_MSK / capital(m.duty, m.lock);
        assert!(m.duty >= m.lock, "ρ = {rho}, q = {q}: L-4b — the duty a bound seat reserves backs the lock it posts");
        let composed_commitment = today.escrow.div_ceil(u128::from(rho)) + today.weight;
        // The escrow lane's bind reserves at least the lock (its eligibility) when the credit cuts the
        // commitment under seats × lock.
        let composed_duty = palw_seat_duty_v2(today.lambda, today.lock_2, composed_commitment, 5, credited.then_some(step)).max(m.lock);
        let composed = SEAT_ROOM_MSK / capital(composed_duty, m.lock);
        println!(
            "ρ = {rho:>3}, q = {q:>3}‰: lane alone duty {:.4} lock {:.4} MSK (fold) → derived {:.2}/DAA (×{:.2}); with escrow's m_c = ⌈E/ρ⌉ duty {:.4} MSK → derived {:.2}/DAA (×{:.2})",
            m.duty as f64 / 1e8,
            m.lock as f64 / 1e8,
            alone,
            alone / today_rate,
            composed_duty as f64 / 1e8,
            composed,
            composed / today_rate
        );
        if q >= 250 {
            assert_eq!(m.lock, today.lock.div_ceil(u128::from(rho)).max(1), "AS-2: the lock ÷ρ once credited");
            assert!((composed - want).abs() / want < 0.10, "ρ = {rho}: §5.4's {want}/DAA within 10% ({composed:.2})");
        } else {
            assert_eq!(m.lock, today.lock, "AS-2: an uncredited claim's lock is today's");
            assert_eq!(m.duty, today.duty, "v3 AS-1′ (stage 2): an uncredited claim's duty is today's");
            let gain = alone / today_rate;
            assert!((gain - 1.0).abs() < 1e-9, "ρ = {rho}, q = 0: ×{gain:.3} — an uncredited step moves no seat price");
        }
    }
}

/// **AG-3 at the fold's own bind**: a panel naming a seat frozen at the binding is refused by the
/// bind's own room test (inert past the 2026-09-23 fence: the claim stays `Provisional`), so a bound
/// panel never holds a seat frozen at the binding; the same claim binds to five unfrozen seats.
#[test]
fn a_frozen_seat_cannot_be_bound() {
    let mut c = Chain::new(t12_fl(1_001));
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let seats = c.floor_seats();
    let (eq, _) = equivocation_of(seats[2].0, floor, 0xE9);
    c.step(&[eq]);
    let x = c.floor_claim(0xE9);
    let at = c.daa + 1;
    let x_ctx = ctx(0xCA_0000 + at, at, at, 0);
    let bind = PalwConsensusObjectV2::PanelBound { claim: x, anchor: h(0xAC_0000 + c.daa), seats: seats_of(&seats) };
    let (next, _, _) = c.try_fold(&c.s, &x_ctx, std::slice::from_ref(&bind), PalwBlockWorkV3::None, Hash64::default()).expect("the block stands");
    assert!(
        matches!(next.claim(&x).unwrap().phase, PalwClaimPhaseV2::Provisional) && next.panel(&x).is_none(),
        "past the 2026-09-23 fence a panel naming a frozen seat is inert: the claim stays Provisional"
    );
    // Without the frozen seat the same claim binds.
    let other: Vec<_> = genesis_seats(&c.p, 1..7).into_iter().filter(|(k, _)| *k != seats[2].0).collect();
    let bind = PalwConsensusObjectV2::PanelBound { claim: x, anchor: h(0xAC_0000 + c.daa), seats: seats_of(&other) };
    let (next, _, _) = c.try_fold(&c.s, &x_ctx, &[bind], PalwBlockWorkV3::None, Hash64::default()).expect("the block stands");
    assert!(matches!(next.claim(&x).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "the other five bind it");
}

// ---------------------------------------------------------------------------------------------
// Review 2, findings 1 and 2: held forfeits (§4-ter.3 step 6) across an aggregate forfeiture
// ---------------------------------------------------------------------------------------------

/// The anchor checkpoint a held dissection's bottom opened, and its chunk (review2's fixture).
const ANCHOR_CHECKPOINT: u32 = 9;
const CHUNK: u32 = 3;
/// What the forger's acquittal kept from the challenger (review2's fixture).
const HELD_FORFEIT: u64 = 3_744 * MSK;

/// A floor claim of the genesis producer, licensed by the coverage door on the five floor seats.
fn covered_floor_claim(c: &mut Chain, seed: u64) -> Hash64 {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    assert!(matches!(phase(c, &id), PalwClaimPhaseV2::ReceiptLicensed { .. }), "the coverage licence folds");
    id
}

/// **review2's fixture: the forfeit a proven acquittal kept for `challenger` on `claim`** — written
/// through the carriage the way `record_held_forfeit_v1` leaves the state: the debit out of
/// `collateral` into `slashed`, and the record beside it.
fn with_forfeit(c: &mut Chain, claim: Hash64, challenger: PalwBondKeyV2) {
    c.s = edited(&c.sp, &c.s, |carriage| {
        let bond = carriage.bonds.get_mut(&challenger).expect("the challenger's bond");
        bond.collateral -= HELD_FORFEIT;
        bond.slashed += HELD_FORFEIT;
        carriage.held_forfeits.insert(
            (claim, Hash64::from_u64_word(0x5E55)),
            PalwHeldForfeitV1 {
                challenger,
                amount: HELD_FORFEIT,
                anchor_leaf_hash: Hash64::from_u64_word(0xA7),
                anchor_checkpoint: ANCHOR_CHECKPOINT,
                narrowed_leaf: 4_750,
                chunks: vec![CHUNK],
                closed_daa: c.daa,
                paused: false,
            },
        );
    });
    assert_eq!(c.s.held_forfeits_of_claim(&claim).count(), 1);
}

/// **review2's fixture: the challenger's step-6 demand of the record's anchor chunk** — a seat's
/// session open from this block (the claim paused, DA-5), written through the carriage. Returns its
/// deadline.
fn with_seat_demand(c: &mut Chain, claim: Hash64, seat: PalwBondKeyV2) -> u64 {
    let window = palw_da_disclose_window_daa_v1(&c.sp);
    let opened = c.daa;
    c.s = edited(&c.sp, &c.s, |carriage| {
        carriage.da_sessions.insert(
            (claim, seat),
            PalwDaSessionV1 {
                opened_daa: opened,
                deadline_daa: opened + window,
                accuser_is_seat: true,
                exposure: 1_000,
                units: vec![PalwDaUnitV1::Held(PalwHeldMissingV1::StateChunk { checkpoint: ANCHOR_CHECKPOINT, chunk: CHUNK })],
                stage: PalwDaStageV1::Licensed,
            },
        );
        carriage.da_claims.insert(
            claim,
            PalwDaClaimV1 {
                open_seat_sessions: 1,
                opened_by_seat: [(seat, 1)].into_iter().collect(),
                paused_since: Some(opened),
                ..Default::default()
            },
        );
    });
    opened + window
}

/// **One empty block at `daa` folded with the processor's extras** (DA-7's seat-answer rule landed,
/// `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1`), checked as `Chain::step_at` checks one — the delta
/// re-applies and reverts, and the child's carriage reloads under its committed root, which is what
/// every node's restart and pruning-point import run — and committed.
fn step_landed(c: &mut Chain, daa: u64) {
    let x = ctx(0xCA_0000 + daa, daa, daa, 0);
    let mut e = c.extras_at(daa);
    e.seat_da_answer_landed = PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1;
    let parent = c.s.clone();
    let (child, delta, _) =
        fold_with(&c.p, &c.sp, &parent, &x, &[], PalwBlockWorkV3::None, Hash64::default(), &e).expect("the block folds");
    assert_eq!(apply_delta_v2(&parent, &delta, &c.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
    assert_eq!(revert_delta_v2(&child, &delta, &c.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(&child)
        .into_state(&c.sp, Some(child.state_root()))
        .unwrap_or_else(|e| panic!("DAA {daa}: every node reloads the tip (restart, pruning-point import): {e}"));
    assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
    c.s = child;
    c.daa = daa;
}

/// **Review 2, finding 1 — a held-forfeit refund that reaches a challenger already forfeited whole
/// joins its forfeiture, and every node reloads the tip.** Genesis seat X holds a held forfeit
/// (3,744 MSK) on the genesis producer's licensed claim Y and the open step-6 demand of its anchor
/// chunk. Before that demand runs out, X's OWN claim Z is convicted past F-L by a proven verdict (the
/// intent class — rcore/cap-s1's route since decision 1 left the DA default in the tier): X is
/// forfeited whole (collateral 0) and final-frozen. Then a door refunds X's forfeit:
/// * **DA-7**: Y's producer withholds the demanded chunk, and the default refunds the record;
/// * **AG-2**: Y's producer is itself forfeited first — its other claim Z′ convicted by a verdict — and
///   the forfeiture voids Y and refunds the record (finding 2's door).
///
/// Either way, on the armed chain X's collateral stays 0 (a final freeze posts none: the load
/// invariant), its `slashed` keeps the refund, the freeze's `forfeited_sompi` grows by exactly it, and
/// each block's tip reloads under its root (before the fix the refund raised X's collateral and no
/// F-L node could reload the tip: the sink search panicked, on every restart). On the dormant twin X is
/// refunded into its collateral at Y's default, as review2 does.
#[test]
fn a_held_forfeit_refund_to_a_forfeited_challenger_joins_its_forfeiture_and_the_tip_reloads() {
    for through_forfeiture in [false, true] {
        let door = if through_forfeiture { "the producer's forfeiture" } else { "Y's DA-7 default" };
        for armed in [true, false] {
            let mut c = Chain::new(if armed { t12_fl(1_001) } else { t12() });
            c.attribution = true;
            c.step(&[bond_obj(1, 20_000 * MSK), court_challenger()]);
            let x = genesis_keys(&c.p, 2).0;
            assert_eq!(c.floor_seats()[1].0, x, "premise: floor seat 1 is genesis bond 2");
            // X's own claim Z, on a panel without X.
            let z = claim_by_genesis(&mut c, 2, 0x2A);
            let gb = genesis_bonds(&c.p);
            let z_seats: Vec<(PalwBondKeyV2, Hash64)> = [1usize, 3, 4, 5, 6].iter().map(|i| (gb[*i].0, gb[*i].1)).collect();
            c.bind(z, &z_seats);
            // The producer's other claim Z′ (the forfeiture door only).
            let z2 = through_forfeiture.then(|| {
                let z2 = c.floor_claim(0x2B);
                let seats = c.floor_seats();
                c.bind(z2, &seats);
                z2
            });
            // Y: X's held forfeit and its open step-6 demand.
            let y = covered_floor_claim(&mut c, 0x6E);
            with_forfeit(&mut c, y, x);
            let dy = with_seat_demand(&mut c, y, x);
            // Z convicted by a verdict: on the armed chain X is forfeited whole, before Y's demand runs out.
            let at = court_fraud(&mut c, z, &z_seats, bond_key(61));
            assert!(at < dy, "premise: X is convicted before its demand's deadline ({at} < {dy})");
            let before = c.s.bond(&x).unwrap().clone();
            let f0 = freeze(&c, &x);
            if armed {
                assert_eq!(before.collateral, 0, "{door}: X forfeited whole");
                assert!(f0.is_some_and(|f| f.final_), "{door}: a final freeze");
            }
            // The door.
            match z2 {
                Some(z2) => {
                    let seats = c.floor_seats();
                    court_fraud(&mut c, z2, &seats, bond_key(61));
                }
                None => {
                    run_to(&mut c, dy);
                    step_landed(&mut c, dy + 1);
                }
            }
            let after = c.s.bond(&x).unwrap().clone();
            let yp = phase(&c, &y);
            println!(
                "armed={armed}, {door}: Y {yp:?}; X collateral {:.2} → {:.2} MSK, slashed {:.2} → {:.2} MSK, forfeited {:?} → {:?}",
                before.collateral as f64 / 1e8,
                after.collateral as f64 / 1e8,
                before.slashed as f64 / 1e8,
                after.slashed as f64 / 1e8,
                f0.map(|f| f.forfeited_sompi as f64 / 1e8),
                freeze(&c, &x).map(|f| f.forfeited_sompi as f64 / 1e8)
            );
            if armed {
                assert_eq!(c.s.held_forfeits_of_claim(&y).count(), 0, "{door}: the record is gone with Y");
                assert!(
                    voided_by(&c, &y, if through_forfeiture { PalwVoidReasonV2::AggregateForfeit } else { PalwVoidReasonV2::ProducerWithholding }),
                    "{door}: Y voided ({yp:?})"
                );
                assert_eq!(after.collateral, 0, "{door}: a final freeze posts no collateral, refund or not");
                assert_eq!(after.slashed, before.slashed, "{door}: the refunded forfeit stays where the forfeiture keeps it");
                let f = freeze(&c, &x).expect("still frozen");
                assert!(f.final_, "{door}: still final");
                assert_eq!(f.forfeited_sompi, f0.unwrap().forfeited_sompi + HELD_FORFEIT, "{door}: the refund joined the forfeiture");
                // The tip every node restarts from, reloaded once more (step_at / step_landed did it per block).
                PalwStateCarriageV2::from_state(&c.s).into_state(&c.sp, Some(c.s.state_root())).expect("the tip reloads");
            } else {
                if through_forfeiture {
                    // The dormant twin forfeits nothing at Z′: Y's own DA-7 default refunds X.
                    assert!(matches!(yp, PalwClaimPhaseV2::ReceiptLicensed { .. }) || voided_by(&c, &y, PalwVoidReasonV2::ProducerWithholding));
                    run_to(&mut c, dy);
                    step_landed(&mut c, dy + 1);
                }
                assert!(voided_by(&c, &y, PalwVoidReasonV2::ProducerWithholding), "dormant: Y defaulted");
                assert_eq!(c.s.held_forfeits_of_claim(&y).count(), 0, "dormant: the record is gone with Y");
                let refunded = c.s.bond(&x).unwrap().clone();
                assert_eq!(refunded.collateral, before.collateral + HELD_FORFEIT, "dormant: X refunded into its collateral");
                assert_eq!(refunded.slashed, before.slashed - HELD_FORFEIT, "dormant: out of slashed");
                assert_eq!(freeze(&c, &x), None);
            }
        }
    }
}

/// **Review 2, finding 2 — the forger's own forfeiture makes the honest challenger whole.** The
/// genesis producer P (the forger) licensed Y; honest seat X lost a held dissection on Y (3,744 MSK
/// held) and demands the anchor chunk. P's OTHER claim Z′ is convicted by a proven verdict (the intent
/// class; rcore/cap-s1's route since decision 1) before that demand runs out — the forger choosing when
/// to lose its bond. Past F-L the forfeiture voids Y
/// `AggregateForfeit`, which closes both refund doors for good (the demand's DA-7 default and the
/// checkpoint accusation need a live claim), and refunds X in the same block: X's collateral is back
/// to what it posted, its `slashed` back to what it was, its demand closed with its exposure returned,
/// and X neither frozen nor charged. On the dormant twin Y stays licensed and its DA-7 default refunds
/// X at the demand's deadline. X's net loss is 0 on both (before the fix: 3,744 MSK armed).
#[test]
fn a_forgers_forfeiture_makes_the_honest_challenger_whole() {
    let mut nets = Vec::new();
    for armed in [true, false] {
        let mut c = Chain::new(if armed { t12_fl(1_001) } else { t12() });
        c.attribution = true;
        c.step(&[bond_obj(1, 20_000 * MSK), court_challenger()]);
        let (producer, _, _) = floor_producer(&c.p);
        let x = genesis_keys(&c.p, 2).0;
        let posted = collateral(&c, &x);
        let slashed = c.s.bond(&x).unwrap().slashed;
        // The forger's other claim Z′, bound first.
        let z = c.floor_claim(0x2B);
        let seats = c.floor_seats();
        c.bind(z, &seats);
        // Y: the forger's licensed claim; X lost a held dissection on it and demands the chunk.
        let y = covered_floor_claim(&mut c, 0x6E);
        with_forfeit(&mut c, y, x);
        let dy = with_seat_demand(&mut c, y, x);
        // Z′ convicted by a verdict before X's demand runs out.
        let at = court_fraud(&mut c, z, &seats, bond_key(61));
        assert!(at < dy, "premise: Z′ is convicted before X's demand runs out ({at} < {dy})");
        if armed {
            assert!(freeze(&c, &producer).is_some_and(|f| f.final_), "the forger is forfeited whole");
            assert!(voided_by(&c, &y, PalwVoidReasonV2::AggregateForfeit), "Y voided with the forger's bond: {:?}", phase(&c, &y));
            assert_eq!(c.s.held_forfeits_of_claim(&y).count(), 0, "the record went with the void");
            assert!(c.s.da_session(&y, &x).is_none(), "X's demand closed with the void");
            assert_eq!(collateral(&c, &x), posted, "X made whole in the forfeiture's block");
            assert_eq!(c.s.bond(&x).unwrap().slashed, slashed, "the forfeit restored where it went");
            assert_eq!(freeze(&c, &x), None, "X is neither convicted nor frozen");
        } else {
            assert!(matches!(phase(&c, &y), PalwClaimPhaseV2::ReceiptLicensed { .. }), "dormant: Y stands");
            assert_eq!(c.s.held_forfeits_of_claim(&y).count(), 1, "dormant: the record stands");
        }
        run_to(&mut c, dy);
        step_landed(&mut c, dy + 1);
        let after = collateral(&c, &x);
        println!(
            "armed={armed}: Y {:?}; X posted {:.2} → {:.2} MSK (net {:.2})",
            phase(&c, &y),
            posted as f64 / 1e8,
            after as f64 / 1e8,
            (after as f64 - posted as f64) / 1e8
        );
        nets.push(posted as i128 - after as i128);
    }
    assert_eq!(nets, vec![0, 0], "the honest challenger's net loss (armed, dormant)");
}

// ---------------------------------------------------------------------------------------------
// Review 2, finding 3: L-T4b — a bound seat is backed at the licence under every step
// ---------------------------------------------------------------------------------------------

/// Bond `bond`'s work room at `at`, the processor's inputs.
fn work_room(c: &Chain, bond: &PalwBondKeyV2, at: u64) -> u128 {
    let raw = c.extras_at(at).settled_anchor_depth;
    kaspa_consensus_core::palw_state_v2::palw_rcore_gate_room_v1(
        &c.s,
        &c.sp,
        bond,
        at,
        raw,
        kaspa_consensus_core::palw_state_v2::PalwRcoreGateV1::Work,
    )
}

/// **L-T4b (review 2, finding 3) — a bound seat is backed at the licence under every step (L-4b).**
/// Two seats S1, S2 at the seat floor have their room held down to ≈ 981 MSK by their own live claims
/// (bound on the genesis seats, kept live by a DA accusation). Fourteen test claims of the genesis
/// producer are offered, one a block, to the panel {S1, S2, g1, g2, g3}; each binds if the fold admits
/// it; then every bound claim is licensed in order by all five seats' coverage receipts. Under every
/// step — dormant, testnet-12's first (ρ = 10, nothing credited) and ρ = 10 with `q_seat` credited —
/// every bound claim licenses with both small seats' locks: the bind never admits a seat its room
/// cannot back at the licence. The steps differ only in how many binds that room admits (a duty of
/// 640.17, 640.17 and 64.02 MSK — stage 2, v3 AS-1′: the uncredited step prices seats as today). The old
/// AS-1 (`⌈lock_2/ρ⌉` as the duty's lock term) bound 12 of them at q = 0 on a 64.02 duty and could license
/// 1: the other 11 would run into `ReceiptTimeout`, and a redraw onto saturated seats into the honest
/// producer's S0′.
#[test]
fn l_t4b_a_bound_seat_is_backed_at_the_licence_under_every_step() {
    let mut bound_per_step = Vec::new();
    for step in [None, Some((10u32, 0u16)), Some((10, 250))] {
        let what = format!("{step:?}");
        let mut c = Chain::new(match step {
            Some((rho, q)) => t12_with(1_001, &[(0, rho, q)]),
            None => t12(),
        });
        let floor = kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_v1(c.sp.min_collateral_sompi());
        let small: [u64; 2] = [31, 32];
        c.step(&[bond_obj(31, floor), bond_obj(32, floor), bond_obj(1, 200_000 * MSK)]);
        let s_keys: Vec<(PalwBondKeyV2, Hash64)> = small
            .iter()
            .map(|n| (bond_key(*n), kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&operator_pubkey_of(*n))))
            .collect();
        let gseats = c.floor_seats();
        // Pre-commit S1/S2's room with their own claims, bound on the genesis seats and held live by
        // a DA accusation (a live claim's commitment stays reserved; a licence would release it).
        let mut own = Vec::new();
        for (i, n) in small.iter().enumerate() {
            let mut k = 0u64;
            while work_room(&c, &bond_key(*n), c.daa + 1) > 3_900 * u128::from(MSK) && k < 40 {
                let id = claim_by(&mut c, *n, 0x3000 + (i as u64) * 100 + k);
                c.bind(id, &gseats);
                c.step(&[da_accuse(id, bond_key(1), 0)]);
                own.push(id);
                k += 1;
            }
        }
        let room0 = work_room(&c, &s_keys[0].0, c.daa + 1);
        // The test claims, each offered to {S1, S2, g1, g2, g3}.
        let panel: Vec<(PalwBondKeyV2, Hash64)> = vec![s_keys[0], s_keys[1], gseats[0], gseats[1], gseats[2]];
        let mut bound_ids = Vec::new();
        let mut refused = 0;
        for t in 0..14u64 {
            let id = c.floor_claim(0x7000 + t);
            let at = c.daa + 1;
            let bind = PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xAC_0000 + c.daa), seats: seats_of(&panel) };
            let x_ctx = ctx(0xCA_0000 + at, at, at, 0);
            let (next, _, _) = c.try_fold(&c.s, &x_ctx, std::slice::from_ref(&bind), PalwBlockWorkV3::None, Hash64::default()).expect("the block stands");
            if matches!(next.claim(&id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }) {
                c.step(&[bind]);
                bound_ids.push((id, c.daa));
            } else {
                refused += 1;
                c.step(&[]);
            }
        }
        // License each bound test claim in order with all five seats' coverage receipts.
        let mut licensed = 0;
        let mut backed_by_both = 0;
        for (id, bound) in &bound_ids {
            let receipts = covered(*id, c.anchor(id), &panel, &[0, 1, 2, 3, 4], *bound);
            let at = c.daa + 1;
            let x_ctx = ctx(0xCA_0000 + at, at, at, 0);
            let obj = PalwConsensusObjectV2::ReceiptLicensedV2 { claim: *id, receipts };
            let folded = c.try_fold(&c.s, &x_ctx, std::slice::from_ref(&obj), PalwBlockWorkV3::None, Hash64::default());
            let licenses = matches!(&folded, Ok((next, _, skips)) if skips.is_empty()
                && matches!(next.claim(id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
            assert!(licenses, "{what}: bound claim {id} licenses (the bind never admits an unbacked seat): {:?}", folded.err());
            c.step(&[obj]);
            licensed += 1;
            if c.s.slashable_lock(s_keys[0].0, *id).is_some() && c.s.slashable_lock(s_keys[1].0, *id).is_some() {
                backed_by_both += 1;
            }
        }
        println!(
            "{what}: S1 room {:.2} MSK → {} bound ({} refused) → {} licensed, {} with both small seats' locks",
            room0 as f64 / 1e8,
            bound_ids.len(),
            refused,
            licensed,
            backed_by_both
        );
        assert!(!bound_ids.is_empty(), "{what}: the premise — the small seats take at least one bind");
        assert_eq!(backed_by_both, bound_ids.len(), "{what}: every bound claim is backed by both small seats' locks");
        assert!(own.iter().all(|id| matches!(phase(&c, id), PalwClaimPhaseV2::PanelBound { .. })), "{what}: own claims still held");
        bound_per_step.push(bound_ids.len());
    }
    assert!(
        bound_per_step[0] == bound_per_step[1] && bound_per_step[1] < bound_per_step[2],
        "the room admits more binds only where the credit cuts the duty (640.17 → 640.17 → 64.02): {bound_per_step:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Review 2, finding 4: L-T6 measured — one conviction's loss at 13k, by route
// ---------------------------------------------------------------------------------------------

/// A court's `ExecutorGuilty` close of `session_id` on an arithmetic proof (a `CourtFraud` verdict)
/// or on a held dissection's bottom (`CourtHeldVerdict` past `palw_offence_attribution`) — the fold
/// reads the verdict and the proof's form, never re-derives it (the acceptance layer adjudicated it):
/// `court_cleared`'s skeleton, and `t12_aheld_held_court`'s dissection bottom over it.
fn guilty_close(session_id: Hash64, dissection: bool) -> PalwConsensusObjectV2 {
    use kaspa_consensus_core::palw_attn_court_v1::{
        PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnDissectBottomV1, PalwAttnRowOpeningV1, PalwAttnTileEvidenceV1,
    };
    use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
    use kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2;
    let PalwConsensusObjectV2::CourtClosed { proof, .. } = court_cleared(session_id) else { unreachable!("a close") };
    let proof = if dissection {
        let PalwCourtVerdictProofV2::Arithmetic { refutation, .. } = proof else { unreachable!("court_cleared's arithmetic skeleton") };
        let tile = |index: u64| PalwAttnRowOpeningV1 {
            leaf: refutation.output_preimage.clone(),
            opening: kaspa_consensus_core::palw_step_leg::PalwStepOpeningV1 { leaf_index: index, leaf_hash: h(0xBAD5), siblings: vec![] },
        };
        PalwCourtVerdictProofV2::AttnDissection {
            binding: Box::new(refutation.binding.clone()),
            bottom: Box::new(PalwAttnDissectBottomV1 {
                version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                session_id,
                tile: 0,
                query: tile(0),
                anchor: None,
                k: PalwAttnTileEvidenceV1::CacheWrites { rows: vec![] },
                v: PalwAttnTileEvidenceV1::CacheWrites { rows: vec![] },
                out_tile: tile(0),
            }),
            operand_openings: vec![],
        }
    } else {
        proof
    };
    PalwConsensusObjectV2::CourtClosed { session_id, verdict: PalwCourtVerdictV2::ExecutorGuilty, proof }
}

/// `EV(1) = (1−q)·[P·E − (1−P)·m] − q·L` (ADR-0160 §4.5), in MSK.
fn ev1(q: f64, p: f64, e: f64, m: f64, l: f64) -> f64 {
    (1.0 - q) * (p * e - (1.0 - p) * m) - q * l
}

/// **L-T6, measured on the fold (review 2, finding 4): what ONE producer conviction takes from a 13k
/// bond, by route — and why only the whole-bond routes may count toward the credited q.** A 13,000
/// MSK piece makes one floor claim (bound; licensed by the five floor seats for the court routes) and
/// is convicted once past F-L, on its own chain per route:
/// * **S1** — a DA default: the TIER (the user's decision 1, rcore/cap-s1: a first default takes the
///   claim's commitment alone, X11);
/// * **`CourtFraud`** — a court's `ExecutorGuilty` close on an arithmetic proof: intent;
/// * **`CourtHeldVerdict`** — the same close on a held dissection's bottom: the TIER, and the same S2
///   arm (`reserved + E + min(10%·C₀, 3G)`, plus the court time) every tier-class producer conviction
///   of a live claim takes — kind 4's `StepArithmetic`, 6, 7, 8 and `LogitsNotStepOutput` void it
///   `CourtFraud` through `act_on_convicted_claim_v1` and the funnel adds nothing (the unit
///   `the_aggregate_funnel_by_offence_class`).
///
/// L is what the piece lost (collateral before less after; it has no vesting row yet). EV(1) at the
/// ramp's gate — q = 0.143, P* = ½, ρ = 10, `m = max(m*(q), ⌈E/ρ⌉)` with `m*` on the normative
/// `L = 3G` — with each route's MEASURED L, and for the tier also with the escrow lane's `m_c` in the
/// escrow slot (the review's ≈ 1,620 MSK): the intent routes take the whole piece (≥ 3G) and EV ≤ 0;
/// the tier route takes less than 3G and EV > 0. So `palw_producer_conviction_credits_q_v1` counts
/// exactly the intent routes.
#[test]
fn l_t6_measured_one_conviction_at_13k_by_route_and_only_whole_bond_routes_credit_q() {
    const PIECE: u64 = 13_000 * MSK;
    const Q: f64 = 0.143;
    const P_STAR: f64 = 0.5;
    const RHO: u64 = 10;
    for (route, offence) in [
        ("S1 (DA default)", PalwConvictedOffenceV1::DaDefault),
        ("CourtFraud (a verdict)", PalwConvictedOffenceV1::CourtFraud),
        ("CourtHeldVerdict (a dissection's verdict)", PalwConvictedOffenceV1::CourtHeldVerdict),
    ] {
        let mut c = Chain::new(t12_fl(1_001));
        c.attribution = true;
        c.step(&[bond_obj(1, 50_000 * MSK), bond_obj(11, PIECE), bond_obj(61, 400_000 * MSK)]);
        let piece = bond_key(11);
        let id = claim_by(&mut c, 11, 0x6A00);
        let claim = c.claim(&id);
        let e_sompi = u128::from(claim.escrowed_reward);
        let three_g = 3 * palw_claim_bond_reservation_v1(&c.sp, &claim).unwrap();
        let seats = c.floor_seats();
        let bound = c.bind(id, &seats);
        let before = collateral(&c, &piece);
        match offence {
            PalwConvictedOffenceV1::DaDefault => {
                da_default(&mut c, &[id], bond_key(1));
            }
            _ => {
                license(&mut c, id, &seats, bound);
                let before_open = collateral(&c, &piece);
                c.step(&[court_opened(&c.s, id, bond_key(61))]);
                let session = court_session_of(&c.s, id, bond_key(61));
                assert_eq!(collateral(&c, &piece), before_open, "{route}: opening a court takes nothing from the executor");
                c.step(&[guilty_close(session, offence == PalwConvictedOffenceV1::CourtHeldVerdict)]);
                let want = if offence == PalwConvictedOffenceV1::CourtFraud { PalwVoidReasonV2::CourtFraud } else { PalwVoidReasonV2::CourtHeldVerdict };
                assert!(voided_by(&c, &id, want), "{route}: the verdict voids the claim {want:?}: {:?}", phase(&c, &id));
            }
        }
        assert_eq!(before, PIECE, "{route}: the premise — a whole 13k piece before its one conviction");
        let lost = u128::from(before - collateral(&c, &piece));
        let f = freeze(&c, &piece).expect("every producer conviction freezes");
        let intent = palw_producer_conviction_credits_q_v1(offence);
        assert_eq!(f.final_, intent, "{route}: a final freeze exactly for the credited class");
        let msk = |sompi: u128| sompi as f64 / 1e8;
        let e = msk(e_sompi);
        let m = msk(e_sompi.div_ceil(u128::from(RHO))).max((e - 2.0 * Q * msk(three_g) / (1.0 - Q)).max(0.0));
        let ev = ev1(Q, P_STAR, e, m, msk(lost));
        let escrow_slot = msk(lost) - e + msk(e_sompi.div_ceil(u128::from(RHO)));
        println!(
            "{route}: the 13k piece lost {:.2} MSK (3G = {:.2}; E = {:.2}, m = {:.2}) → EV(1) at q = {Q}, ρ = {RHO}: {ev:+.1} MSK; \
             credited: {intent}{}",
            msk(lost),
            msk(three_g),
            e,
            m,
            if intent { String::new() } else { format!(" (with the escrow lane's m_c in the slot: L {escrow_slot:.2}, EV {:+.1})", ev1(Q, P_STAR, e, m, escrow_slot)) }
        );
        if intent {
            assert_eq!(lost, u128::from(PIECE), "{route}: the whole piece");
            assert!(lost >= three_g, "{route}: ≥ 3G");
            assert!(ev <= 0.0, "{route}: EV(1) ≤ 0 at the gate ({ev:+.1})");
        } else {
            assert!(collateral(&c, &piece) > 0, "{route}: the tier leaves the rest of the piece");
            assert!(lost < three_g, "{route}: the S2 tier alone, under 3G ({:.2} < {:.2})", msk(lost), msk(three_g));
            assert!(ev > 0.0, "{route}: credited, this route would make a 13k garbage claim pay at the gate ({ev:+.1})");
            assert!(ev1(Q, P_STAR, e, m, escrow_slot) > 0.0, "{route}: and with the escrow lane's m_c in the slot");
        }
    }
    // Every contradiction tag: credited exactly where the funnel forfeits the whole bond.
    for tag in 0..=u8::MAX {
        let offence = PalwConvictedOffenceV1::Contradiction { tag };
        assert_eq!(
            palw_producer_conviction_credits_q_v1(offence),
            kaspa_consensus_core::palw_aggregate_liability_v1::palw_offence_is_intent_class_v1(offence),
            "tag {tag}"
        );
    }
    for offence in [PalwConvictedOffenceV1::Equivocation, PalwConvictedOffenceV1::CoveringSigner, PalwConvictedOffenceV1::CourtHeldVerdict] {
        assert!(!palw_producer_conviction_credits_q_v1(offence), "{offence:?} is not a whole-bond producer conviction");
    }
}
