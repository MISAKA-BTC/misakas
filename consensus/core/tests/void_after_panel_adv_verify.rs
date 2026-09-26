//! **The adversarial verifier's runs (2026-09-25), flipped into the regression of the panel seed.**
//! Beside `void_after_panel_probe`. Independent of the probe's `Run`: this file folds with
//! `rcore_common`'s `Chain` and draws with the processor's WHOLE draw policy, including the Valid-lock /
//! one-ledger seat filter the probe left `None` (`palw_panel_valid_lock_of_v1`, rebuilt from its public
//! core parts).
//!
//! Before the fix every run below captured: the anchor block's identity keyed the draw, the segment
//! assignment and the S3 sample, and its producer minted identities for free. Past lane F1's fence
//! (`Params::palw_panel_seed_execution`, armed here at [`F1_AT`] on testnet-12 as released: [`t12_f1`])
//! the chain draws from `PalwAnchorFactV2::panel_seed` — `H(anchor attempt's execution commitment ‖
//! claim)` ([`chain_seed`]) — and each test asserts the identity grind now fails. What it does not
//! close — a new lottery win is a new panel, and a win is cheap while P0-10 is open — a1 prints as the
//! per-win pair rate (docs/t12-panel-seed-2026-09-25.md):
//!
//! * `a1_…` — the ADR's own threat (§4.3 row 13, the undetectable coverage lie: the full seat AND the
//!   partial holder of the lied segment are the attacker's), 1–3 Sybils at the panel floor: 6,000
//!   identities of one anchor attempt are ONE panel, and it is not the pair.
//! * `a2_…` — end to end: the pair panel the old grind found is refused by the acceptance gate
//!   (`AnchorMismatch` naming the identity, `PanelMismatch` naming the seed); the chain binds the seed's
//!   panel, and the assignment the fold reads back from the stored seed names no Sybil pair.
//! * `a3_…` — identity freedom WITHOUT signature randomness (deterministic ML-DSA-87, rnd = 0; moving
//!   only the timestamp or the nonce inside its bucket): every header names one execution commitment
//!   and one seed.
//! * `a4_…` — the 8k class (readied seats only): one panel over 4,000 identities, no pair.
//! * `a5_…` — lane A (`Params::palw_operator_anchor`, armed with F1 at one common height over the eight
//!   genesis bonds: [`t12_f1_op`]) closes a1's residual for a non-operator: none of 1,000 fresh junk
//!   wins may anchor (the processor's predicate on each win's header), the pair panel a win finds under
//!   F1 alone is refused by the gate against the operator's anchor, and the claim gets ONE fair draw —
//!   whose pair rate over fresh operator executions is printed as the per-CLAIM residual (a claim, not
//!   a free re-roll, per try).
//!
//! Run: CARGO_BUILD_JOBS=2 cargo test -p kaspa-consensus-core --test void_after_panel_adv_verify -- --nocapture --test-threads=1
#![allow(dead_code, clippy::too_many_arguments)]

#[path = "rcore_common.rs"]
mod rc;
use rc::*;

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_TICKET_NONCE_BUCKET_LOG2, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
    attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3, execution_commitment_v3, palw_nonce_bucket_v1,
};
use kaspa_consensus_core::palw_model_registry_v1::PalwReadinessPolicyV1;
use kaspa_consensus_core::palw_panel_v2::{
    PalwAnchorFactV2, PalwPanelDrawPolicyV1, PalwPanelIndependenceV1, PalwPanelStakeDrawV1, PalwPanelV2Error, PalwPanelValidLockV1,
    PalwRcoreSeatFilterV1, derive_panel_v2_with_policy, palw_bond_maturity_window_v2, palw_panel_anchor_execution_v1,
    palw_seat_maturity_floor_v1, validate_panel_bound_v2_with_policy,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwPanelSeatV2, palw_panel_valid_lock_required_v1, palw_rcore_bind_prices_v1, palw_second_clock_depth_of_v1,
    palw_v2_pre_object_base_v1,
};
use std::collections::BTreeSet;
use std::time::Instant;

const ATT: u64 = 0xA1;
const MINER: u64 = 0xA2;

fn anchor_delay(p: &Params) -> u64 {
    bundle(p).panel.anchor_delay()
}

/// Lane F1's fence height in these runs (below every chain and header here; see
/// `void_after_panel_probe::F1_AT`).
const F1_AT: u64 = 900;

/// testnet-12 as released, with lane F1's fence armed at [`F1_AT`]; its predicate checked against
/// [`seed_rule_armed`], which the draws read.
fn t12_f1() -> Params {
    let mut p = t12();
    assert_eq!(p.palw_panel_seed_execution, None, "the release ships the fence dormant");
    p.palw_panel_seed_execution = Some(kaspa_consensus_core::config::params::ForkActivation::new(F1_AT));
    p.validate_palw_v2().expect("testnet-12 with lane F1 armed is a runnable ruleset");
    for daa in [0, F1_AT - 1, F1_AT, 1_000, 1_000_000] {
        assert_eq!(p.palw_panel_seed_execution_active_at(daa), seed_rule_armed(daa), "DAA {daa}");
    }
    p
}

fn seed_rule_armed(anchor_daa: u64) -> bool {
    anchor_daa >= F1_AT
}

/// [`t12_f1`] with lane A armed at the same height over testnet-12's eight genesis bonds (the
/// recommended rollout: one common post-launch height). Validated.
fn t12_f1_op() -> Params {
    let mut p = t12_f1();
    assert_eq!(p.palw_operator_anchor, None, "the release ships lane A dormant");
    p.palw_operator_anchor = p.palw_operator_anchor_of_genesis_bonds_v1(kaspa_consensus_core::config::params::ForkActivation::new(F1_AT));
    p.validate_palw_v2().expect("testnet-12 with lanes F1 and A armed is a runnable ruleset");
    p
}

/// An attempt-lane header at `daa` carrying `env` — what the anchor predicate reads.
fn header_of(p: &Params, env: &PalwAttemptEnvelopeV2, daa: u64) -> Header {
    let mut header = Header::from_precomputed_hash(identity(0x4EAD_0000 + daa), vec![]);
    header.pow_algo_id = p.palw_attempt_lane_at(daa).attempt_algo_id();
    header.daa_score = daa;
    header.palw_commitment = env.encode_wire();
    header
}

fn identity(i: u64) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).to_state();
    state.update(b"adv-verify/identity");
    state.update(&i.to_le_bytes());
    Hash64::from_slice(state.finalize().as_bytes())
}

/// The processor's whole draw policy at `daa` for `claim_id` on `base` (`palw_panel_draw_policy_at`
/// + `palw_panel_valid_lock_of_v1`).
fn policy_at(c: &Chain, base: &PalwChainStateV2, claim_id: &Hash64, daa: u64) -> PalwPanelDrawPolicyV1 {
    let (p, sp) = (&c.p, &c.sp);
    let readiness = registry_fold(p, daa).filter(|f| f.governs_at(daa)).map(|fold| {
        PalwReadinessPolicyV1::at(
            &fold,
            daa,
            sp.base_class_id(),
            p.palw_audit_2026_09_23_active_at(daa) && p.palw_readiness_v2_at(daa),
        )
    });
    let e = c.extras_at(daa);
    let claim = base.claim(claim_id).expect("the claim");
    let seat_count = bundle(p).panel.seat_count() as usize;
    let valid_lock = (e.audit_2026_09_23_active && e.objective_offence_at(daa)).then(|| PalwPanelValidLockV1 {
        required: palw_panel_valid_lock_required_v1(base, sp, &e, claim),
        now_daa: daa,
        settled_anchor_depth: palw_second_clock_depth_of_v1(base, sp, &e, daa),
        window_court: sp.window_court(),
        rcore: sp.rcore_plus_active_at(daa).then(|| PalwRcoreSeatFilterV1 {
            eligibility: palw_rcore_bind_prices_v1(base, sp, &e, claim_id, claim, seat_count, daa).eligibility,
            ceiling_permille: sp.fp_max_exposure_ratio_permille(),
            // Lane V02: as the processor resolves it at the binding block.
            resolved_locks_off_ceiling: sp.final_lock_full_collateral_active_at(daa),
            accuser_reserve: kaspa_consensus_core::palw_state_v2::palw_bond_accuser_reserve_v1(sp, daa),
            held_charge_floor: kaspa_consensus_core::palw_state_v2::palw_v02_held_charge_floor_v1(
                sp,
                daa,
                e.offence_attribution_active && e.held_context_ladder.is_some(),
            ),
        }),
    });
    PalwPanelDrawPolicyV1 {
        weighted: p.palw_audit_2026_09_11_deep_active_at(daa),
        economy: p.palw_seat_economy_at(daa),
        readiness,
        independence: p.palw_admission_independence_daa().map(|from_daa| PalwPanelIndependenceV1 {
            from_daa,
            base_class_id: sp.base_class_id(),
            anchor_daa: daa,
        }),
        valid_lock,
        stake: p.palw_rcore_plus.is_some_and(|f| f.is_active(daa)).then_some(PalwPanelStakeDrawV1::V1),
    }
}

fn pre_object_base(c: &Chain, parent: &PalwChainStateV2, block: Hash64, daa: u64) -> PalwChainStateV2 {
    let point = PalwBlockContextV2 { block, daa_score: daa, blue_score: daa, subsidy: T12_BLOCK_SUBSIDY_SOMPI };
    let f = flags(&c.p, daa);
    let mut e = c.extras_at(daa);
    e.sw8_anchor_delay = Some(anchor_delay(&c.p));
    palw_v2_pre_object_base_v1(
        parent,
        &c.sp,
        &point,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &e,
    )
    .expect("the pre-object base folds")
}

/// The panel the processor would bind for `claim` in an anchor block whose identity is `anchor`.
fn derive_at(c: &Chain, base: &PalwChainStateV2, daa: u64, anchor: Hash64, claim: &Hash64) -> Result<Vec<PalwPanelSeatV2>, String> {
    let b = bundle(&c.p);
    let window = c.p.palw_bond_maturity.filter(|m| m.activation.is_active(daa)).map(|m| m.window_daa);
    let maturity = palw_seat_maturity_floor_v1(daa, window.map(|w| palw_bond_maturity_window_v2(daa, w, None)));
    derive_panel_v2_with_policy(
        base,
        &b.panel,
        claim,
        anchor,
        c.sp.min_collateral_sompi(),
        maturity,
        c.p.palw_capability_bound_at(daa),
        policy_at(c, base, claim, daa),
    )
    .map_err(|e| e.to_string())
}

/// **The seed the chain draws `claim`'s panel from in an anchor block with identity `block` whose
/// attempt's execution commitment is `execution`** — `PalwAnchorFactV2::panel_seed`, as the processor's
/// anchor walk builds the fact past `palw_rcore_plus`. The identity is in the fact and not in the seed.
fn anchor_fact(block: Hash64, daa: u64, execution: Hash64) -> PalwAnchorFactV2 {
    PalwAnchorFactV2 {
        anchor_block: block,
        anchor_daa: daa,
        predecessor_daa: daa - 1,
        anchor_execution: seed_rule_armed(daa).then_some(execution),
    }
}

fn chain_seed(block: Hash64, daa: u64, execution: Hash64, claim: &Hash64) -> Hash64 {
    anchor_fact(block, daa, execution).panel_seed(claim)
}

/// The acceptance gate the processor runs on a `PanelBound` carried by the anchor block `block`.
fn gate(
    c: &Chain,
    base: &PalwChainStateV2,
    daa: u64,
    fact: &PalwAnchorFactV2,
    claim: &Hash64,
    anchor: Hash64,
    seats: &[PalwPanelSeatV2],
) -> Result<(), PalwPanelV2Error> {
    let b = bundle(&c.p);
    let window =
        c.p.palw_bond_maturity.filter(|m| m.activation.is_active(daa)).map(|m| palw_bond_maturity_window_v2(daa, m.window_daa, None));
    let point = PalwBlockContextV2 { block: fact.anchor_block, daa_score: daa, blue_score: daa, subsidy: T12_BLOCK_SUBSIDY_SOMPI };
    validate_panel_bound_v2_with_policy(
        base,
        &b.panel,
        &c.sp,
        &point,
        claim,
        fact,
        anchor,
        seats,
        window,
        c.p.palw_capability_bound_at(daa),
        policy_at(c, base, claim, daa),
        None,
    )
}

/// Whether any segment's pair — the full seat and that segment's partial holder, the two attestations
/// the coverage door needs — is two coalition bonds, in the assignment drawn from `seed`.
fn any_pair(seed: Hash64, claim: Hash64, seats: &[PalwPanelSeatV2], coalition: &BTreeSet<PalwBondKeyV2>) -> bool {
    let a = palw_segment_assignment_v2(seed, claim, seats.len() as u16);
    (0..a.segments).any(|segment| {
        let (f, p, _, _) = pair_of(seed, claim, seats, segment);
        coalition.contains(&f) && coalition.contains(&p)
    })
}

/// `(the full seat's bond, the partial holder of `segment`)` in the assignment the fold derives from
/// `(anchor, claim)`.
fn pair_of(anchor: Hash64, claim: Hash64, seats: &[PalwPanelSeatV2], segment: u16) -> (PalwBondKeyV2, PalwBondKeyV2, usize, usize) {
    let a = palw_segment_assignment_v2(anchor, claim, seats.len() as u16);
    let full = a.full_seat as usize;
    let partial = (0..seats.len()).find(|i| *i != full && a.mask_of(*i as u16).covers(segment)).expect("a partial holder");
    (seats[full].bond, seats[partial].bond, full, partial)
}

fn sybil_objs(p: &Params, sybils: &[u64], class: Hash64) -> Vec<PalwConsensusObjectV2> {
    let seat_floor = p.palw_seat_economy_at(1_000).expect("t12 arms the panel economy").panel_floor_sompi;
    sybils
        .iter()
        .map(|n| match bond_obj(*n, seat_floor) {
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, collateral, payout_payload, signature, .. } => {
                PalwConsensusObjectV2::BondRegistered {
                    bond,
                    pubkey,
                    operator_pubkey,
                    collateral,
                    payout_payload,
                    capable_classes: [class].into_iter().collect(),
                    signature,
                }
            }
            _ => unreachable!(),
        })
        .collect()
}

/// A floor chain: the attacker's producing bond, `MINER`, and `sybils` at the panel floor declared
/// capable of the floor; stepped past bond maturity; the attacker's floor claim accepted; stepped to
/// its slot. Returns `(chain, claim, slot, parent-at-slot)`.
fn floor_setup(sybils: &[u64]) -> (Chain, Hash64, u64) {
    floor_setup_on(t12_f1(), sybils)
}

/// [`floor_setup`] on the ruleset `p`.
fn floor_setup_on(p: Params, sybils: &[u64]) -> (Chain, Hash64, u64) {
    let floor = genesis_classes(&p)[0].0;
    let mut c = Chain::new(p.clone());
    c.attribution = true;
    let mut objects = vec![bond_obj(ATT, at_least_the_floor(&p, 13_000 * 100_000_000)), bond_obj(MINER, 1_000_000 * 100_000_000)];
    objects.extend(sybil_objs(&p, sybils, floor));
    c.step_at(1_001, &objects, PalwBlockWorkV3::None, Hash64::default(), 0);
    let window = p.palw_bond_maturity.map(|m| m.window_daa).unwrap_or(0);
    c.step_at(1_002 + window, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let (env, key, claim) = floor_attempt_of(&c, ATT, 0xADF1);
    c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    assert!(c.s.claim(&claim).is_some(), "the attacker's floor claim is accepted");
    let slot = c.s.claim(&claim).unwrap().bind_base_daa() + anchor_delay(&p);
    c.step_at(slot - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    (c, claim, slot)
}

// =================================================================================================

#[test]
fn a1_the_undetectable_coverage_lie_under_anchor_grinding_no_longer_captures() {
    println!(
        "=== A1 (fixed): capture events per anchor identity (floor, processor's whole policy incl. the one-ledger seat filter) ==="
    );
    const TRIES: u64 = 6_000;
    const WINS: u64 = 1_000;
    for sybils in [vec![0x5C1u64], vec![0x5C1, 0x5C2], vec![0x5C1, 0x5C2, 0x5C3]] {
        let (c, claim, slot) = floor_setup(&sybils);
        let coalition: BTreeSet<PalwBondKeyV2> = sybils.iter().map(|n| bond_key(*n)).collect();
        let base = pre_object_base(&c, &c.s, identity(0), slot);
        // The anchor block's producer holds ONE lottery win: every identity it can mint carries it.
        let (_, execution, _) = floor_attempt_of(&c, MINER, 0xADF0);
        let (mut pair0, mut any, mut q3, mut seated, mut errors) = (0u64, 0u64, 0u64, 0u64, 0u64);
        let mut panels: BTreeSet<Vec<PalwBondKeyV2>> = BTreeSet::new();
        let t = Instant::now();
        for i in 0..TRIES {
            let seed = chain_seed(identity(0x1000 + i), slot, execution, &claim);
            let seats = match derive_at(&c, &base, slot, seed, &claim) {
                Ok(s) => s,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            let n = seats.iter().filter(|s| coalition.contains(&s.bond)).count() as u64;
            seated += n;
            q3 += u64::from(n >= 3);
            let (f, p0, _, _) = pair_of(seed, claim, &seats, 0);
            pair0 += u64::from(coalition.contains(&f) && coalition.contains(&p0));
            any += u64::from(any_pair(seed, claim, &seats, &coalition));
            panels.insert(seats.iter().map(|s| s.bond).collect());
        }
        let ms = t.elapsed().as_secs_f64() * 1e3 / TRIES as f64;
        // What a pair costs now: fresh execution commitments — each a lottery win (a new win is a new
        // commitment, `void_after_panel_probe` T3a), stood in for here by fresh 64-byte values.
        let mut per_win = 0u64;
        for w in 0..WINS {
            let seed = chain_seed(identity(0), slot, identity(0xAE00_0000 + w), &claim);
            let seats = derive_at(&c, &base, slot, seed, &claim).expect("draws");
            let (f, p0, _, _) = pair_of(seed, claim, &seats, 0);
            per_win += u64::from(coalition.contains(&f) && coalition.contains(&p0));
        }
        let seat_floor = c.p.palw_seat_economy_at(1_000).unwrap().panel_floor_sompi;
        println!(
            "{} Sybil(s) × {:.0} MSK = {:.0} MSK: {TRIES} identities of one anchor attempt -> {} panel(s); Sybil seatings {:.3}/panel; \
             PAIR on segment 0 in {pair0}, on any segment in {any}; quorum≥3 in {q3}; draw errors {errors}; {ms:.2} ms/derive (debug). \
             Fresh anchor attempts: a segment-0 pair in {per_win} of {WINS} ({:.4} per lottery win)",
            sybils.len(),
            msk(seat_floor as u128),
            msk(seat_floor as u128 * sybils.len() as u128),
            panels.len(),
            seated as f64 / TRIES as f64,
            per_win as f64 / WINS as f64,
        );
        assert_eq!(errors, 0, "no identity makes the draw refuse (the refusal is not an anchor-producer lever)");
        assert_eq!(panels.len(), 1, "{} Sybil(s): the identities of one anchor attempt draw one panel", sybils.len());
        assert_eq!((pair0, any), (0, 0), "{} Sybil(s): the grind over identities captures no coverage pair", sybils.len());
        assert!(q3 == 0 || q3 == TRIES, "the identity cannot move the quorum outcome");
    }
}

#[test]
fn a2_the_ground_pair_is_refused_by_the_gate_and_the_bound_panel_names_no_pair() {
    let sybils = [0x5C1u64, 0x5C2];
    let (mut c, claim, slot) = floor_setup(&sybils);
    let coalition: BTreeSet<PalwBondKeyV2> = sybils.iter().map(|n| bond_key(*n)).collect();
    let ad = anchor_delay(&c.p);
    let parent = c.s.clone();
    let base = pre_object_base(&c, &parent, identity(0), slot);
    // The anchor block: MINER's floor attempt at the slot — one lottery win, whatever identity it takes.
    let (env, key, _) = floor_attempt_of(&c, MINER, 0xADF2);
    let job = floor_job_anchor(&c.p, bond_key(MINER), 0x10C0 + 0xADF2);
    // The OLD grind: identities as the seed, until one draws the pair (what bound before the fix).
    let mut tries = 0u64;
    let (id, ground) = loop {
        tries += 1;
        let id = identity(0x7_0000 + tries);
        let seats = derive_at(&c, &base, slot, id, &claim).expect("draws");
        let (f, p0, _, _) = pair_of(id, claim, &seats, 0);
        if coalition.contains(&f) && coalition.contains(&p0) {
            break (id, seats);
        }
        assert!(tries < 200_000);
    };
    let fact = anchor_fact(id, slot, key);
    let seed = fact.panel_seed(&claim);
    let seats = derive_at(&c, &base, slot, seed, &claim).expect("the seed draws");
    // The gate: the ground object is refused whichever anchor it names; the seed's panel is accepted.
    let as_ground = gate(&c, &base, slot, &fact, &claim, id, &ground);
    let ground_on_seed = gate(&c, &base, slot, &fact, &claim, seed, &ground);
    let honest = gate(&c, &base, slot, &fact, &claim, seed, &seats);
    // The identity grind under the chain's rule: every identity is the seed's one panel.
    let panels: BTreeSet<Vec<PalwBondKeyV2>> = (0..2_000u64)
        .map(|i| {
            let s = derive_at(&c, &base, slot, chain_seed(identity(0x9_0000 + i), slot, key, &claim), &claim).unwrap();
            s.iter().map(|x| x.bond).collect()
        })
        .collect();
    // Bind what the chain binds, in the anchor block, and read the assignment back from the fold.
    let point = PalwBlockContextV2 { block: id, daa_score: slot, blue_score: slot, subsidy: T12_BLOCK_SUBSIDY_SOMPI };
    let mut e = c.extras_at(slot);
    e.own_job_anchor = job;
    e.sw8_anchor_delay = Some(ad);
    let bound_obj = PalwConsensusObjectV2::PanelBound { claim, anchor: seed, seats: seats.clone() };
    let (child, _, skips) = fold_with(&c.p, &c.sp, &parent, &point, &[bound_obj], PalwBlockWorkV3::Attempt(&env), key, &e)
        .expect("the anchor block folds");
    assert!(skips.is_empty(), "{skips:?}");
    c.s = child;
    c.daa = slot;
    let panel = c.s.panel(&claim).expect("bound").clone();
    let a = palw_segment_assignment_v2(panel.anchor, claim, panel.seats.len() as u16);
    println!("=== A2 (fixed): grind → gate → bind ===");
    println!(
        "the old grind: {tries} identities to put 2 Sybils (260,000 MSK) on the full seat and segment 0's partial of the attacker's claim"
    );
    println!(
        "gate: that object naming its identity -> {as_ground:?}; naming the seed -> {ground_on_seed:?}; the seed's own panel -> {honest:?}"
    );
    println!(
        "2,000 identities of the anchor attempt -> {} panel(s); bound: {:?}, stored anchor = seed {}; assignment {a:?}; a Sybil pair on any segment: {}",
        panels.len(),
        c.s.claim(&claim).unwrap().phase,
        panel.anchor == seed,
        any_pair(panel.anchor, claim, &panel.seats, &coalition)
    );
    assert!(matches!(as_ground, Err(PalwPanelV2Error::AnchorMismatch(_))), "the block identity is not a panel anchor any more");
    if ground != seats {
        assert_eq!(ground_on_seed, Err(PalwPanelV2Error::PanelMismatch), "the ground panel is not the seed's");
    }
    assert_eq!(honest, Ok(()), "build = accept on the seed");
    assert_eq!(panels.len(), 1, "no identity of the anchor attempt draws another panel");
    assert_eq!((panel.anchor, &panel.seats), (seed, &seats), "the fold stored the seed and its panel");
    assert!(!any_pair(panel.anchor, claim, &panel.seats, &coalition), "the bound panel's assignment names no Sybil pair");
}

#[test]
fn a3_identity_freedom_without_signature_randomness_names_one_seed() {
    let p = t12_f1();
    let b = bundle(&p);
    let g = genesis_state(&p);
    let floor = genesis_classes(&p)[0].0;
    let net = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
    let target = kaspa_consensus_core::palw_admission_v2::palw_effective_class_target_v1(&g, &b.state, &floor, None).unwrap();
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x77u8; 32]);
    let bond = bond_key(ATT);
    let lane = p.palw_attempt_lane_at(1_000);
    let parents: kaspa_consensus_core::header::CompressedParents = vec![vec![h(0x9A7F)]].try_into().unwrap();
    let ts0 = p.genesis.timestamp + 1_000 * p.target_time_per_block();
    let nonce0 = (0xBEEFu64 << PALW_TICKET_NONCE_BUCKET_LOG2) + 9;
    let header0 = Header::new_finalized(
        kaspa_consensus_core::constants::BLOCK_VERSION,
        parents,
        Default::default(),
        Default::default(),
        Default::default(),
        ts0,
        0x207f_ffff,
        nonce0,
        lane.attempt_algo_id(),
        1_000,
        kaspa_consensus_core::BlueWorkType::from_u64(0),
        1_000,
        Default::default(),
    );
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&header0);
    let mut attempt = PalwAttemptUnsignedV2 {
        version: lane.attempt_version(),
        network_domain: net,
        challenge: challenge_v2(net, pre_pow, ts0, nonce0, floor, &bond.0),
        class_id: floor,
        executor_bond: bond.0,
        executor_pubkey: kp.verification_key.as_ref().to_vec(),
        operator_id: h(0x0C),
        artifact_root: g.class(&floor).unwrap().artifact_root,
        trace_root: Hash64::default(),
        output_root: h(0x0808),
        pwu: 1,
        trace_manifest_root: Hash64::default(),
        trace_chunk_count: kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_TRACE_CHUNKS,
        trace_retention_daa: 999_999,
        execution_root: h(0xE1E1),
    };
    let exec_anchor = execution_anchor_v3(net, pre_pow, floor, &bond.0, nonce0);
    let mut draws = 0u64;
    loop {
        draws += 1;
        attempt.trace_root = identity(0x3A00_0000 + draws);
        attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
        if class_ticket_v3(&attempt, exec_anchor) <= target {
            break;
        }
    }
    let ticket0 = class_ticket_v3(&attempt, exec_anchor);
    let exec0 = execution_commitment_v3(&attempt, exec_anchor);
    // Deterministic signing: rnd = 0 always (what a "deterministic signature" rule would mandate).
    let sign0 = |a: &PalwAttemptUnsignedV2| {
        let sig = libcrux_ml_dsa::ml_dsa_87::sign(
            &kp.signing_key,
            attempt_id_v2(a).as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0u8; 32],
        )
        .expect("sign");
        PalwAttemptEnvelopeV2 { attempt: a.clone(), signature: sig.as_ref().to_vec() }
    };
    let verify = |env: &PalwAttemptEnvelopeV2| {
        env.validate_signature_v2(|key, msg, sig, ctx| {
            let (Ok(key), Ok(sig)) = (<[u8; 2592]>::try_from(key), <[u8; 4627]>::try_from(sig)) else { return false };
            libcrux_ml_dsa::ml_dsa_87::portable::verify(
                &libcrux_ml_dsa::ml_dsa_87::MLDSA87VerificationKey::new(key),
                msg,
                ctx,
                &libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature::new(sig),
            )
            .is_ok()
        })
    };
    let mut ids: BTreeSet<Hash64> = BTreeSet::new();
    // The same attempt signed twice deterministically: one identity.
    let e1 = sign0(&attempt);
    let e2 = sign0(&attempt);
    assert_eq!(e1.signature, e2.signature, "rnd = 0 signs deterministically");
    let mut rows = Vec::new();
    let (mut executions, mut seeds): (BTreeSet<Hash64>, BTreeSet<Hash64>) = Default::default();
    for (dt, dn) in [(0u64, 0u64), (1, 0), (2, 0), (3, 0), (0, 1), (0, 2), (0, 3), (5, 7)] {
        let mut hdr = header0.clone();
        hdr.timestamp = ts0 + dt;
        hdr.nonce = nonce0 + dn;
        assert_eq!(palw_nonce_bucket_v1(hdr.nonce), palw_nonce_bucket_v1(nonce0));
        let pp = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&hdr);
        assert_eq!(pp, pre_pow, "timestamp and nonce are outside the pre-PoW hash");
        let mut a = attempt.clone();
        a.challenge = challenge_v2(net, pp, hdr.timestamp, hdr.nonce, floor, &bond.0);
        let ea = execution_anchor_v3(net, pp, floor, &bond.0, hdr.nonce);
        let env = sign0(&a);
        hdr.palw_commitment = env.encode_wire();
        hdr.finalize();
        env.validate_stateless_v2_at_version(lane.attempt_version(), net, pp, hdr.timestamp, hdr.nonce).expect("binds its header");
        verify(&env).expect("signed");
        assert_eq!(execution_commitment_v3(&a, ea), exec0, "the same execution");
        assert_eq!(class_ticket_v3(&a, ea), ticket0, "the same winning ticket");
        // The anchor walk's reading of this header, and the seed a claim anchored here draws from.
        let execution = palw_panel_anchor_execution_v1(net, &hdr).expect("an attempt header");
        seeds.insert(chain_seed(hdr.hash, 1_000, execution, &h(0xC1A1)));
        executions.insert(execution);
        rows.push((dt, dn, hdr.hash));
        ids.insert(hdr.hash);
    }
    println!("=== A3: identities from ONE win, deterministic signatures (rnd = 0) ===");
    println!("win after {draws} junk draws; ticket {ticket0:#x} <= target");
    for (dt, dn, id) in &rows {
        println!("  timestamp +{dt} ms, nonce +{dn} (same bucket): identity {}…", &id.to_string()[..16]);
    }
    println!(
        "{} distinct valid identities, {} execution commitment(s) read off their headers, {} panel seed(s), one ticket",
        ids.len(),
        executions.len(),
        seeds.len()
    );
    assert_eq!(
        ids.len(),
        rows.len(),
        "timestamp and nonce alone still mint identities (a deterministic-signature rule would close nothing)"
    );
    assert_eq!(executions, BTreeSet::from([exec0]), "…but every one names the one execution commitment");
    assert_eq!(seeds.len(), 1, "…and so one panel seed: the identities buy no draw");
}

#[test]
fn a4_the_8k_class_one_panel_per_anchor_attempt() {
    let p = t12_f1();
    let (short, _) = model_classes(&p);
    let sybils = [0x5D1u64, 0x5D2, 0x5D3];
    let mut c = model_chain(p.clone(), short, 1);
    c.attribution = true;
    let window = p.palw_bond_maturity.map(|m| m.window_daa).unwrap_or(0);
    let objs = sybil_objs(&p, &sybils, short);
    c.step_at(c.daa + 1, &objs, PalwBlockWorkV3::None, Hash64::default(), 0);
    c.step_at(c.daa + 1 + window, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let claim = model_claim(&mut c, short, 1, 0x8C);
    let slot = c.s.claim(&claim).unwrap().bind_base_daa() + anchor_delay(&p);
    c.step_at(slot - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let coalition: BTreeSet<PalwBondKeyV2> = sybils.iter().map(|n| bond_key(*n)).collect();
    let mut ready: Vec<PalwBondKeyV2> = honest(&p);
    ready.extend(coalition.iter().copied());
    c.s = readied(&c.sp, &c.s, &ready, short, slot);
    println!("=== A4 (fixed): the 8k class ===");
    println!("shard plan on 8k: {:?}", c.s.class_shard_plan(&short).map(|p| p.shard_count));
    let base = pre_object_base(&c, &c.s, identity(0), slot);
    // The anchor block is a floor attempt at the slot: one lottery win.
    let (_, execution, _) = floor_attempt_of(&c, MINER, 0xADF4);
    const TRIES: u64 = 4_000;
    let (mut pair0, mut any, mut q3, mut seated, mut errors) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut first_err = None;
    let mut panels: BTreeSet<Vec<PalwBondKeyV2>> = BTreeSet::new();
    for i in 0..TRIES {
        let seed = chain_seed(identity(0x8_0000 + i), slot, execution, &claim);
        let seats = match derive_at(&c, &base, slot, seed, &claim) {
            Ok(s) => s,
            Err(e) => {
                errors += 1;
                first_err.get_or_insert(e);
                continue;
            }
        };
        let n = seats.iter().filter(|s| coalition.contains(&s.bond)).count() as u64;
        seated += n;
        q3 += u64::from(n >= 3);
        let (f, p0, _, _) = pair_of(seed, claim, &seats, 0);
        pair0 += u64::from(coalition.contains(&f) && coalition.contains(&p0));
        any += u64::from(any_pair(seed, claim, &seats, &coalition));
        panels.insert(seats.iter().map(|s| s.bond).collect());
    }
    println!(
        "8k, 3 Sybils readied beside the 8 genesis seats: {TRIES} identities of one anchor attempt -> {} panel(s); Sybil seatings {:.3}/panel; \
         pair on segment 0 in {pair0}, on any segment in {any}; quorum≥3 in {q3}; errors {errors} {first_err:?}",
        panels.len(),
        seated as f64 / TRIES as f64,
    );
    assert_eq!(errors, 0, "the 8k draw never refuses on the anchor");
    assert_eq!(panels.len(), 1, "one anchor attempt, one 8k panel");
    assert_eq!((pair0, any), (0, 0), "the identity grind captures no coverage pair on the 8k class");
    assert!(q3 == 0 || q3 == TRIES);
}

/// **Lane A: a non-operator's wins are not anchors, so the attacker's claim gets one fair draw.**
#[test]
fn a5_past_the_operator_fence_no_junk_win_anchors_and_the_claim_gets_one_fair_draw() {
    const WINS: u64 = 1_000;
    const FAIR: u64 = 1_000;
    println!("=== A5 (lane A): a non-operator's junk wins against the operator-anchor fence (floor, 2–3 Sybils) ===");
    for sybils in [vec![0x5C1u64, 0x5C2], vec![0x5C1, 0x5C2, 0x5C3]] {
        let p = t12_f1_op();
        let rule = p.palw_operator_anchor_rule_v1().expect("lane A reaches the processor");
        let (c, claim, slot) = floor_setup_on(p.clone(), &sybils);
        let coalition: BTreeSet<PalwBondKeyV2> = sybils.iter().map(|n| bond_key(*n)).collect();
        let base = pre_object_base(&c, &c.s, identity(0), slot);
        let pair0_of = |seed: Hash64, seats: &[PalwPanelSeatV2]| {
            let (f, p0, _, _) = pair_of(seed, claim, seats, 0);
            coalition.contains(&f) && coalition.contains(&p0)
        };

        // The attacker's fresh junk wins at the slot: F1 alone makes each an anchor and a panel; lane A
        // admits none.
        let (mut admitted, mut f1_pairs) = (0u64, 0u64);
        for w in 0..WINS {
            let (env, key, _) = floor_attempt_of(&c, ATT, 0xA500_0000 + w);
            admitted += u64::from(rule.admits_anchor_v1(&header_of(&p, &env, slot)));
            let seed = chain_seed(identity(0), slot, key, &claim);
            let seats = derive_at(&c, &base, slot, seed, &claim).expect("draws");
            f1_pairs += u64::from(pair0_of(seed, &seats));
        }
        // A win that WOULD capture under F1 alone (the attacker keeps drawing until it has one) …
        let mut tries = 0u64;
        let (ground_env, ground_seed, ground) = loop {
            tries += 1;
            let (env, key, _) = floor_attempt_of(&c, ATT, 0xA600_0000 + tries);
            let seed = chain_seed(identity(0), slot, key, &claim);
            let seats = derive_at(&c, &base, slot, seed, &claim).expect("draws");
            if pair0_of(seed, &seats) {
                break (env, seed, seats);
            }
            assert!(tries < 100_000, "a pair within 100,000 wins");
        };
        // … is not an anchor, and the gate — handed the walk's anchor under lane A, the operator's
        // attempt — refuses the panel it keyed.
        assert!(!rule.admits_anchor_v1(&header_of(&p, &ground_env, slot)), "the capturing win may not anchor");
        let (op_bond, op_pubkey, op_operator) = floor_producer(&c.p);
        let floor = genesis_classes(&c.p)[0].0;
        let (mut op_env, _, _) = junk_attempt(floor, op_bond, op_pubkey, &op_operator, c.floor_pwu(slot), 0x0B01, 0x10C0 + 0x0B01);
        op_env.attempt.artifact_root = c.s.class(&floor).expect("the floor").artifact_root;
        assert_eq!(rule.operator_of_v1(&header_of(&p, &op_env, slot)), Some(op_bond), "the genesis card's attempt is an operator's");
        let op_key = execution_commitment_v3(
            &op_env.attempt,
            execution_anchor_v3(h(NET), h(0x10C0 + 0x0B01), floor, &op_bond.0, 7),
        );
        let fact = anchor_fact(h(0x0B_0001), slot, op_key);
        let op_seed = fact.panel_seed(&claim);
        let op_seats = derive_at(&c, &base, slot, op_seed, &claim).expect("the operator's anchor draws");
        let refused = gate(&c, &base, slot, &fact, &claim, ground_seed, &ground);
        let accepted = gate(&c, &base, slot, &fact, &claim, op_seed, &op_seats);
        // The per-CLAIM residual: the pair rate of the one fair draw a claim gets (over fresh operator
        // executions the attacker does not choose).
        let mut fair_pairs = 0u64;
        for i in 0..FAIR {
            let seed = chain_seed(h(0x0B_0001), slot, identity(0xFA1A_0000 + i), &claim);
            let seats = derive_at(&c, &base, slot, seed, &claim).expect("draws");
            fair_pairs += u64::from(pair0_of(seed, &seats));
        }
        println!(
            "{} Sybil(s): {WINS} fresh junk wins -> {admitted} may anchor (F1 alone: a segment-0 pair in {f1_pairs}); the capturing win found after {tries} draws is not an anchor and its panel meets the gate on the operator's anchor as {refused:?}; the operator's panel {accepted:?}, pair: {}; the one fair draw's pair rate {fair_pairs}/{FAIR} = {:.4} per claim",
            sybils.len(),
            pair0_of(op_seed, &op_seats),
            fair_pairs as f64 / FAIR as f64
        );
        assert_eq!(admitted, 0, "{} Sybil(s): no junk win of a non-operator is an anchor past the fence", sybils.len());
        assert!(refused.is_err(), "the attacker's capturing panel is refused against the operator's anchor");
        assert_eq!(accepted, Ok(()), "build = accept on the operator's anchor");
    }
}
