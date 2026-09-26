//! **ADR-0160 lane liab — aggregate bond liability on testnet-12's own fold** (L-T1…L-T5, L-T7, L-T8
//! and the seat side's measurement L-T4).
//!
//! Every block is checked as `Chain::step` / `Tape::block` check one: the delta re-applies and reverts,
//! and the child's carriage reloads under its committed root (`into_state`: the ledger re-derived, the
//! freeze map's load check). F-L (`Params::palw_capacity_aggregate_liability`) is armed on testnet-12 at a
//! low height beside its twin with the fence `None`, so every test crosses the fence or compares against
//! the dormant chain:
//!
//! * **L-T1** — an intent-class conviction (a DA default, S1) past the fence forfeits the WHOLE bond:
//!   collateral 0, every live claim voided `AggregateForfeit`, every vesting row it is payee of burned
//!   (its seat legs in another producer's row leg by leg), a final freeze; below the fence (the same
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
//!
//! Run: cargo test -p kaspa-consensus-core --test palw_capacity_aggregate_liability -- --nocapture

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_admission_v2::{PalwAdmissionV2Error, PalwEpochBudgetFencesV1, check_palw_attempt_admission_v2};
use kaspa_consensus_core::palw_aggregate_liability_v1::{
    PalwBondFreezeV1, PalwCapacityLiabilityV1, PalwCapacityStepV1, palw_bond_is_frozen_v1,
};
use kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwVoidReasonV2, palw_bond_collateral_is_locked_v6, palw_claim_bond_reservation_v1,
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

/// **L-T1 (intent class, DA default) with the fence crossed on one chain, beside its dormant twin.**
///
/// The genesis producer P (a 939,063 MSK card) defaults on claim A BELOW the fence: S1 only — the
/// commitment, a strike — its other claim B stays live, no freeze; and the armed chain's root is the
/// dormant twin's (nothing below the fence moved). Past the fence P defaults on A′ with B′ bound and C′
/// provisional: on the armed chain P's collateral goes to 0, B′ and C′ are voided `AggregateForfeit`
/// (their withheld reward never minted, their seats off duty), the freeze is final and names the
/// `DaDefault` record, whose `collected` is at least the collateral P posted (A-I1: never more than the
/// posted collateral plus the unmatured rows); P's next attempt is skipped `ProducerFrozen` and
/// admission refuses it; P's exit is shut. On the twin: S1 again, B′ and C′ live, P produces.
#[test]
fn l_t1_a_da_default_forfeits_the_whole_bond_past_the_fence_and_only_its_tier_below_it() {
    let w = da_window();
    let fence = 1_020 + w + 20;
    println!("DA window {w} DAA; F-L at {fence}");
    let mut chains = [Chain::new(t12_fl(fence)), Chain::new(t12())];
    let (producer, _, _) = floor_producer(&chains[0].p);
    let accuser = bond_key(1);
    let c0 = collateral(&chains[0], &producer);
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
    // Past the fence.
    for (armed, c) in [true, false].into_iter().zip(chains.iter_mut()) {
        run_to(c, fence + 1);
        let a = c.floor_claim(0xA2);
        let seats = c.floor_seats();
        c.bind(a, &seats);
        let commitment = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&a)).unwrap();
        let deadline = da_accuse_all(c, &[a], accuser);
        // The bond's other claims, made just before the default so no window of their own closes first.
        run_to(c, deadline - 6);
        let b = c.floor_claim(0xB2);
        c.bind(b, &seats);
        let cc = c.floor_claim(0xC2);
        let before = collateral(c, &producer);
        let at = da_run_out(c, &[a], deadline);
        let record = c.s.consumed_offence(&palw_da_offence_id_v1(&producer.0, &a)).expect("the DaDefault record").clone();
        if armed {
            assert_eq!(collateral(c, &producer), 0, "AG-2: the posted collateral, whole");
            assert!(voided_by(c, &b, PalwVoidReasonV2::AggregateForfeit), "AG-2: the bound claim, voided with the bond");
            assert!(voided_by(c, &cc, PalwVoidReasonV2::AggregateForfeit), "AG-2: the provisional claim too");
            assert!(c.s.panel_duty_row_of(&b).is_none(), "its seats left duty with it");
            let f = freeze(c, &producer).expect("AG-3: the freeze");
            assert!(f.final_ && f.since_daa == at, "an intent-class freeze is final, dated at the conviction: {f:?}");
            assert_eq!(f.offence_key, palw_da_offence_id_v1(&producer.0, &a), "it names the DaDefault record");
            assert_eq!(u128::from(f.forfeited_sompi), u128::from(before) - commitment, "the forfeiture took what S1 left (no vesting row here)");
            assert!(u128::from(record.collected) >= u128::from(before) - commitment, "A-I1: collected includes the whole posted collateral: {record:?}");
            assert!(u128::from(record.collected) <= u128::from(c0), "A-I1: never more than the bond ever posted");
            println!(
                "armed: P posted {:.2} MSK before the default; collected {:.2} MSK; claims A′ {:?}, B′/C′ AggregateForfeit",
                before as f64 / MSK as f64,
                record.collected as f64 / MSK as f64,
                phase(c, &a)
            );
            assert!(exit_shut(c, &producer), "AG-3: the exit is shut");
            // AG-3: P produces nothing — the fold skips its attempt and admission refuses it.
            let (floor, _, _, _) = genesis_classes(&c.p)[0];
            let (bond, pubkey, operator) = genesis_keys(&c.p, 0);
            let (env, key, id) = junk_attempt(floor, bond, pubkey, &operator, c.floor_pwu(c.daa + 1), 0xA3, 0x10C0 + 0xA3);
            let x = PalwBlockContextV2 { block: h(0x7E_0000 + c.daa + 1), daa_score: c.daa + 1, blue_score: c.daa + 1, subsidy: 0 };
            // (A forfeited bond posts nothing, so the producer floor — checked first — names the refusal; a
            // tier freeze, which leaves collateral, is refused `ProducerFrozen` by name: the Eq test.)
            let (next, _, skips) = c.try_fold(&c.s, &x, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
            assert!(next.claim(&id).is_none() && skips.len() == 1, "the attempt is skipped: {skips:?}");
            let mut env = env;
            env.attempt.artifact_root = c.s.class(&floor).expect("the floor").artifact_root;
            let refused = check_palw_attempt_admission_v2(&c.s, &c.sp, &bundle(&c.p).admission, &x, &env, fences(&c.p, x.daa_score));
            assert!(matches!(refused, Err(PalwAdmissionV2Error::ProducerBelowFloor { bond: b, collateral: 0, .. }) if b == producer), "{refused:?}");
            // AG-5: the voided claims stay convictable through h_obl — the record (and so a conviction's
            // target) stands until retirement, which is past h_obl on testnet-12.
            let h_obl = c.sp.window_receipt();
            assert!(c.sp.claim_retirement_daa() >= h_obl, "AG-5: claims retire no sooner than h_obl");
            run_to(c, at + h_obl);
            for id in [&a, &b, &cc] {
                assert!(
                    kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(&c.s, id).is_some(),
                    "AG-5: {id} is still a conviction's target at voided + h_obl"
                );
            }
            // The freeze outlives window_court: a final freeze never lifts.
            let wc = c.sp.window_court();
            run_to(c, at + wc + 10);
            assert!(freeze(c, &producer).is_some_and(|f| f.final_), "final: never lifted");
        } else {
            assert_eq!(u128::from(before - collateral(c, &producer)), commitment, "dormant: S1's forfeit alone");
            assert!(live(c, &b) && live(c, &cc), "dormant: the bond's other claims live on");
            assert_eq!(freeze(c, &producer), None);
            assert!(record.collected as u128 <= commitment + 1, "dormant: collected is the tier");
            let id = c.floor_claim(0xA3);
            assert!(c.s.claim(&id).is_some(), "dormant: the producer produces");
        }
    }
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
        c.step(&[bond_obj(7, 50_000 * MSK), bond_obj(1, 50_000 * MSK)]);
        let q1 = claim_by(&mut c, 7, 0x71);
        let seats = c.floor_seats();
        c.bind(q1, &seats);
        let (eq, evidence) = equivocation_of(q, floor, 0xE71);
        let before = collateral(&c, &q);
        c.step(&[eq]);
        let first = c.daa;
        let record = c.s.consumed_offence(&equivocation_key(q, evidence)).expect("the Eq record").clone();
        assert_eq!(u64::from(before - collateral(&c, &q)), record.collected, "armed={armed}: Eq's tier, and only it");
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
        // A later intent-class conviction: final, whole.
        c.bind(q2, &seats);
        let at = da_default(&mut c, &[q2], bond_key(1));
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
/// seat leg in X's row), and holds its own claims; its committed ledger is far above zero. S defaults
/// on its own claim (intent class): S's collateral goes to 0 whatever it had committed; its seat leg in
/// X's row is burned (the producer's and the other seats' legs stand); its other live claim is voided
/// `AggregateForfeit`.
#[test]
fn l_t5_a_pre_committed_pool_is_forfeited_whole_and_a_seat_s_legs_burn_leg_by_leg() {
    let mut c = Chain::new(t12_fl(1_001));
    c.step(&[bond_obj(1, 50_000 * MSK)]);
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
    da_default(&mut c, &[d], bond_key(1));
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

// ---------------------------------------------------------------------------------------------
// L-T7: reorg, restart and IBD across a conviction and a lift; below the fence, the dormant root
// ---------------------------------------------------------------------------------------------

/// The scripted run L-T7 records: bond 7 produces and equivocates (a tier freeze), the chain runs past
/// the lift, bond 7 produces again; then the genesis producer defaults (the intent class, final).
fn l_t7_script(t: &mut Tape) -> (usize, usize, usize) {
    let (floor, _, _, _) = genesis_classes(&t.c.p)[0];
    t.step(vec![bond_obj(7, 50_000 * MSK), bond_obj(1, 50_000 * MSK)]);
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
    t.step(vec![da_accuse(a, bond_key(1), 0)]);
    let deadline = t.c.s.da_session(&a, &bond_key(1)).expect("the session").deadline_daa;
    t.at(deadline + 1, vec![]);
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
/// pieces at the producer floor each make a claim; one piece defaults (intent class): that piece
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
    ]);
    let seats = c.floor_seats();
    let first = claim_by(&mut c, 11, 0xB0B);
    c.bind(first, &seats);
    let three_g = 3 * palw_claim_bond_reservation_v1(&c.sp, &c.claim(&first)).unwrap();
    let deadline = da_accuse_all(&mut c, &[first], bond_key(1));
    // The other pieces' claims, made just before the default so no window of their own closes first.
    run_to(&mut c, deadline - 10);
    let pieces: Vec<(u64, Hash64)> = (12..=14).map(|n| (n, claim_by(&mut c, n, 0xB00 + n))).collect();
    let before: Vec<u64> = (11..=14).map(|n| collateral(&c, &bond_key(n))).collect();
    da_run_out(&mut c, &[first], deadline);
    let record = c.s.consumed_offence(&palw_da_offence_id_v1(&bond_key(11).0, &first)).unwrap().clone();
    assert_eq!(collateral(&c, &bond_key(11)), 0, "the piece forfeits its whole bond");
    assert!(freeze(&c, &bond_key(11)).is_some_and(|f| f.final_), "and is frozen for good");
    assert!(u128::from(record.collected) >= three_g.min(u128::from(before[0])), "≥ 3G per conviction (or the whole piece)");
    println!("a 13k piece's conviction collects {:.2} MSK (3G = {:.2} MSK)", record.collected as f64 / 1e8, three_g as f64 / 1e8);
    for (i, (n, id)) in pieces.iter().enumerate() {
        assert_eq!(collateral(&c, &bond_key(*n)), before[i + 1], "piece {n} untouched");
        assert!(freeze(&c, &bond_key(*n)).is_none() && live(&c, id), "piece {n} neither frozen nor voided");
    }
    // The same 52,000 MSK as ONE bond: one conviction takes all of it, and voids its other claims.
    let whole = claim_by(&mut c, 20, 0xC00);
    c.bind(whole, &seats);
    let deadline = da_accuse_all(&mut c, &[whole], bond_key(1));
    run_to(&mut c, deadline - 10);
    let others: Vec<Hash64> = (1..4).map(|i| claim_by(&mut c, 20, 0xC00 + i)).collect();
    let whole_before = collateral(&c, &bond_key(20));
    assert!(whole_before > 3 * floor_msk, "the premise: the whole bond is worth more than three pieces");
    da_run_out(&mut c, &[whole], deadline);
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
    let duty = u128::from(c.s.panel_duty_row_of(&x).expect("the duty row").seat_exposure);
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

/// **L-T4 — the seat side at each step, measured on the fold, and the network's floor seat capital.**
/// Per floor claim the seats hold `5 · duty · 122 + 5 · lock · 3,000` MSK·DAA (ADR-0160 Appendix A: the
/// duty bind → Final, the lock to F + 3,000); eight genesis seats post 8 × 469,531.6 MSK of work room,
/// so the network licenses `3,756,253 / capital` floor claims per DAA — ≈ 0.94 today.
///
/// * **This lane alone** (the commitment still `E + w`): the fold's duty is AS-1's
///   `duty_bind(⌈λ/ρ⌉, ⌈lock_2/ρ⌉, E + w, 5)` and its lock AS-2's `⌈lock/ρ⌉` once the credit reaches
///   `q_seat` (250‰) — both asserted against the formula, and the rate printed.
/// * **With the escrow lane's commitment** (`m_c = ⌈E/ρ⌉` at that credit, so `commitment′ = ⌈E/ρ⌉ + w`),
///   the duty the same formula gives and the lock measured here reach §5.4's rows 9.4 / 23.5 / 94 —
///   each within 10%.
/// * With nothing credited (testnet-12's first step, q = 0) only the duty divides; the table prints
///   what that buys.
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
        assert_eq!(
            m.duty,
            palw_seat_duty_v2(today.lambda, today.lock_2, today.commitment, 5, Some(step)),
            "ρ = {rho}: AS-1's formula on the bind's own inputs"
        );
        let alone = SEAT_ROOM_MSK / capital(m.duty, m.lock);
        let composed_commitment = today.escrow.div_ceil(u128::from(rho)) + today.weight;
        let composed_duty = palw_seat_duty_v2(today.lambda, today.lock_2, composed_commitment, 5, Some(step));
        let composed = SEAT_ROOM_MSK / capital(composed_duty, m.lock);
        println!(
            "ρ = {rho:>3}, q = {q:>3}‰: lane alone duty {:.4} lock {:.4} MSK → {:.2}/DAA (×{:.1}); with escrow's m_c = ⌈E/ρ⌉ duty {:.4} MSK → {:.2}/DAA (×{:.1})",
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
            assert_eq!(m.lock, today.lock, "AS-2: below q_seat the lock is today's");
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
    let (next, _, _) = c.try_fold(&c.s, &x_ctx, &[bind.clone()], PalwBlockWorkV3::None, Hash64::default()).expect("the block stands");
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
