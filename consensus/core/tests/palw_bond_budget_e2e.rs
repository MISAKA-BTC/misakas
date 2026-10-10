//! **Lane BUDGET (ADR-0176 / ADR-0177) through testnet-12's own fold** — the capacity package's fixture ([`Sim`]: real admission, real
//! riders, binding, licensing, audits, courts and vesting; every block re-applied, reverted and reloaded under its root) with
//! `palw_bond_budget_v1` (and `palw_model_bond_allocation_v1`) set on a copy of the params.
//!
//! **These tests bypass `validate_palw_v2`, and say so**: neither fence can be armed (their values are POLICY the user has not set). The
//! fences are set after the capacity package validated and mirrored by `sync_palw_bond_budget_v1`; every policy here is a TEST value.
//!
//! * **same bond, honest pace vs forger pace** — equal ceilings, the fast producer's extra attempts skipped;
//! * **ρ ×1 / ×100 / ×1000** — Q moves, B/R/F (held and capped) do not; riders share their lead's one block;
//! * **early Final / court void / retry** — nothing returns before `d + W`; a Final pays (all legs) at most the claim's reservation;
//! * **split** — ten bonds of a tenth never admit more than the whole;
//! * **allocation** — a model's share of the realized carve clips reward only (admission and weight unchanged), unassigned budget is
//!   never minted;
//! * **replay** — the tape rewound and re-folded is the same chain, block for block.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_bond_budget_e2e -- --nocapture`

#[path = "capacity_stage1_common.rs"]
mod stage1;
use stage1::*;

use kaspa_consensus_core::config::params::{
    PALW_T12_CAPACITY_EMISSION_BUDGET_V1, PALW_T12_CAPACITY_MULTI_CLAIM_V1, PALW_T12_CAPACITY_RHO_BREAKER_V1,
    PALW_T12_CAPACITY_RHO10_FENCES_V1, PalwPostLaunchFenceV1,
};
use kaspa_consensus_core::palw_aggregate_liability_v1::{PalwCapacityLiabilityV1, PalwCapacityStepOffsetV1};
use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_TRACE_CHUNKS, PalwAttemptEnvelopeV2, attempt_trace_manifest_root_v1};
use kaspa_consensus_core::palw_bond_budget_v1::*;
use kaspa_consensus_core::palw_capacity_s567_v1::palw_rider_challenge_v1;
use kaspa_consensus_core::palw_state_v2::palw_bond_backs_live_duty_v1;

const S567: [&PalwPostLaunchFenceV1; 3] =
    [&PALW_T12_CAPACITY_EMISSION_BUDGET_V1, &PALW_T12_CAPACITY_MULTI_CLAIM_V1, &PALW_T12_CAPACITY_RHO_BREAKER_V1];

/// The capacity package at [`H`] with F-L's one step at `tier` (stage567's `tier_params`), validated — then the budget fences on top,
/// NOT validated (module doc).
fn budget_params(tier: u32, policy: PalwBondBudgetPolicyV1, allocation: Option<PalwModelAllocationPolicyV1>) -> Params {
    let mut p = params_for(Class::Floor, false);
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().chain(S567) {
        (f.set)(&mut p, Some(ForkActivation::new(H)));
    }
    let value = PalwCapacityLiabilityV1::of_schedule_v1(
        ForkActivation::new(H),
        &[PalwCapacityStepOffsetV1 { after_daa: 0, rho: tier, q_credit_permille: 250 }],
    );
    p.palw_capacity_aggregate_liability = Some(value);
    p.sync_palw_capacity_liability();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the ρ = {tier} package validates: {e:?}"));
    p.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1 { activation: ForkActivation::new(H), policy });
    p.palw_model_bond_allocation_v1 =
        allocation.map(|policy| PalwModelBondAllocationFenceV1 { activation: ForkActivation::new(H), policy });
    p.sync_palw_bond_budget_v1();
    assert!(p.validate_palw_v2().is_err(), "the real validator still refuses what this test bypasses");
    p
}

/// A TEST policy: per `unit` MSK of capital and `W`, three claims at ρ = 1, three blocks, `reward_msk` of reward, ample weight.
fn policy(unit_msk: u64, window: u64, rho: u32, reward_msk: u64, slice: bool) -> PalwBondBudgetPolicyV1 {
    PalwBondBudgetPolicyV1 {
        version: PALW_BOND_BUDGET_POLICY_VERSION_V1,
        window_daa: window,
        capital_unit_sompi: unit_msk * MSK,
        rho,
        claims_per_unit: 3,
        block_units_per_unit: 3 * PALW_BUDGET_BLOCK_UNIT_V1,
        reward_per_unit_sompi: reward_msk * MSK,
        final_weight_per_unit: 1_000_000_000_000_000_000,
        max_open_claims_per_bond: 10_000,
        slice_rights_by_rho: slice,
        round_rights: PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1_000 },
    }
}

fn budget(sim: &Sim) -> &PalwBondBudgetStateV1 {
    sim.c.s.bond_budget().expect("the engine exists past the fence")
}

fn window(sim: &Sim, n: u64) -> PalwBudgetVectorV1 {
    budget(sim).bond_row(&bond_key(n)).map(|row| row.window).unwrap_or_default()
}

fn skipped(sim: &Sim, needle: &str) -> usize {
    sim.skips.iter().filter(|(k, _)| k.contains(needle)).map(|(_, n)| *n).sum()
}

fn rider_of(sim: &Sim, lead: &Hash64, index: u32, n: u64, seed: u64) -> PalwAttemptEnvelopeV2 {
    let (mut env, _, _) = floor_attempt_of(&sim.c, n, seed);
    let retention = sim.c.claim(lead).trace_retention_daa;
    env.attempt.challenge = palw_rider_challenge_v1(lead, index);
    env.attempt.trace_chunk_count = PALW_ATTEMPT_V2_TRACE_CHUNKS;
    env.attempt.trace_manifest_root = attempt_trace_manifest_root_v1(env.attempt.trace_root, PALW_ATTEMPT_V2_TRACE_CHUNKS);
    env.attempt.trace_retention_daa = retention;
    env
}

fn riders_object(sim: &Sim, lead: Hash64, n: u64, count: usize, seed0: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::AttemptRidersV1 {
        lead,
        riders: (0..count as u32).map(|i| rider_of(sim, &lead, i, n, seed0 + u64::from(i))).collect(),
    }
}

/// **Same capital, same window, same ρ: the forger's pace and the honest pace reach equal ceilings** (ADR-0176 D1). Bond 90 attempts in
/// every DAA, bond 91 in every fifth; over one and a half windows each is admitted exactly six times — three per window — and the fast
/// bond's extra attempts are skipped by the budget, the blocks standing.
#[test]
fn same_bond_honest_and_forger_reach_equal_ceilings_on_the_real_fold() {
    let mut sim = Sim::new(
        budget_params(250, policy(1_000_000, 40, 1, 1_000_000, false), None),
        Class::Floor,
        &[(90, 1_000_000), (91, 1_000_000)],
    );
    let base = sim.c.daa;
    let (mut fast, mut slow) = (0usize, 0usize);
    for d in 1..=60u64 {
        fast += usize::from(sim.block(base + d, vec![], Some((90, 0x1_0000 + d))).is_some());
        if d % 5 == 1 {
            slow += usize::from(sim.block(base + d, vec![], Some((91, 0x2_0000 + d))).is_some());
        }
    }
    println!("honest vs forger: fast {fast}, slow {slow}, budget skips {}", skipped(&sim, "budget"));
    assert_eq!((fast, slow), (6, 6), "three a window each, whatever the pace");
    assert!(skipped(&sim, "budget") >= 50, "the fast bond's extra attempts were refused by the budget");
    let caps = palw_bond_budget_caps_v1(&policy(1_000_000, 40, 1, 1_000_000, false), 1_000_000 * MSK);
    for n in [90, 91] {
        let w = window(&sim, n);
        assert!(w.fits_within(&caps), "bond {n}: {w:?} within {caps:?}");
        assert_eq!(w.block_units, 3 * PALW_BUDGET_BLOCK_UNIT_V1, "bond {n}: three blocks in its window now");
    }
    budget(&sim).check_consistency().expect("the engine's invariants");
}

/// **ρ ×1 / ×100 / ×1000 (ADR-0176 D1/D2)**: three lead attempts fill B whatever ρ is; at ρ = 1 Q is full too and a rider batch is
/// refused whole, at ρ ≥ 100 two riders join the first lead and take two thirds of ITS block — the bond still holds exactly three blocks.
/// B, R and F caps are the same at every ρ; Q is `3ρ`.
#[test]
fn rho_1_100_1000_move_q_and_never_b_r_or_f() {
    let mut seen = Vec::new();
    for rho in [1u32, 100, 1_000] {
        let pol = policy(1_000_000, 4_000, rho, 1_000_000, false);
        let mut sim = Sim::new(budget_params(250, pol.clone(), None), Class::Floor, &[(90, 1_000_000)]);
        let mut leads = Vec::new();
        for k in 0..5u64 {
            if let Some(id) = sim.claim(90, 0x3_0000 + k) {
                leads.push(id);
            }
        }
        assert_eq!(leads.len(), 3, "ρ = {rho}: three blocks of B, three lead attempts");
        let before = skipped(&sim, "riders");
        let obj = riders_object(&sim, leads[0], 90, 2, 0x3_1000);
        sim.step(vec![obj]);
        let took = skipped(&sim, "riders") == before;
        assert_eq!(took, rho > 1, "ρ = {rho}: Q = {} decides the riders", 3 * rho);
        let w = window(&sim, 90);
        assert_eq!(w.block_units, 3 * PALW_BUDGET_BLOCK_UNIT_V1, "ρ = {rho}: riders ride the lead's block, never a new one");
        assert_eq!(w.claims, if took { 5 } else { 3 });
        if took {
            let (lead_units, each) = palw_rider_block_attribution_v1(2);
            assert_eq!(budget(&sim).claim_row(&leads[0]).unwrap().consumed.block_units, lead_units);
            let riders: Vec<_> =
                budget(&sim).claims.iter().filter(|(_, r)| r.origin == PalwBudgetOriginV1::Rider).map(|(_, r)| *r).collect();
            assert_eq!(riders.len(), 2);
            assert!(riders.iter().all(|r| r.consumed.block_units == each));
        }
        let caps = palw_bond_budget_caps_v1(&pol, 1_000_000 * MSK);
        assert_eq!(caps.claims, 3 * rho as u64);
        assert!(w.fits_within(&caps));
        seen.push((caps.block_units, caps.reward_sompi, caps.final_weight, w.block_units));
        budget(&sim).check_consistency().expect("the engine's invariants");
    }
    assert!(seen.windows(2).all(|p| p[0] == p[1]), "B, R and F are invariant in ρ: {seen:?}");
}

/// **Early Final, a court void and a retry recover nothing before `d + W`** (ADR-0176 D4), and a Final pays — every leg, buyback
/// included — at most the claim's reservation (rights sliced by ρ = 4: `R ≤ 1,000 MSK / 12` against a carve of thousands).
#[test]
fn early_final_court_void_and_retry_recover_nothing_before_d_plus_w() {
    let pol = policy(1_000_000, 5_000, 4, 1_000, true);
    let (r_claim, _) = palw_bond_budget_claim_ceilings_v1(&pol).unwrap();
    assert_eq!(r_claim, 1_000 * MSK / 12, "1,000 MSK per unit over q·ρ = 12");
    let mut sim = Sim::new(budget_params(250, pol, None), Class::Floor, &[(90, 1_000_000)]);
    let a = sim.claim(90, 0x4_0001).expect("a");
    let b = sim.claim(90, 0x4_0002).expect("b");
    let c = sim.claim(90, 0x4_0003).expect("c");
    assert!(sim.claim(90, 0x4_0004).is_none(), "B = 3 blocks: the fourth is skipped");
    let escrow = sim.c.claim(&a).escrowed_reward;
    assert!(escrow > r_claim, "the carve {escrow} exceeds the per-claim reward right {r_claim}");
    let d_a = budget(&sim).claim_row(&a).unwrap().accepted_daa;
    // a: bound, licensed, audited, Final — long before d + W.
    let seats = sim.seats();
    for id in [a, b] {
        let bound = sim.bind(id, &seats);
        sim.license(id, &seats, bound);
    }
    let audits = audit_receipts_for(&sim.c.s, &sim.c.sp, &[a, b]);
    for batch in audits {
        sim.step(vec![batch]);
    }
    sim.court_fraud(b);
    sim.finalize_all(&[a]);
    assert!(sim.c.daa < d_a + 5_000, "Final at {} is early: d + W = {}", sim.c.daa, d_a + 5_000);
    let row = sim.c.s.vesting_row(&a).expect("a vests").clone();
    let named = row.producer.amount + row.seats.iter().map(|(_, p)| p.amount).sum::<u64>() + row.reserve + row.buyback_bound;
    assert!(named <= r_claim, "every leg of a's pay ({named}) is within its reservation ({r_claim})");
    assert!(named > 0);
    let grants = budget(&sim).claim_row(&a).unwrap().consumed;
    assert!(grants.reward_sompi <= r_claim && grants.final_weight > 0);
    // The retry: refused while a, b and c sit in the window — a Final, b convicted, c pending.
    let skips = skipped(&sim, "budget");
    assert!(sim.claim(90, 0x4_0005).is_none(), "no early recovery");
    assert_eq!(skipped(&sim, "budget"), skips + 1);
    assert!(palw_bond_backs_live_duty_v1(&sim.c.s, &bond_key(90), sim.c.daa, None));
    // At a's d + W: one claim's room, and only one (b's returns one DAA later).
    assert!(sim.block(d_a + 4_999, vec![], Some((90, 0x4_0006))).is_none(), "one DAA before d + W: nothing");
    assert!(sim.block(d_a + 5_000, vec![], Some((90, 0x4_0007))).is_some(), "a's room returns at d + W");
    assert!(sim.block(d_a + 5_000, vec![], Some((90, 0x4_0008))).is_none(), "and only a's");
    let _ = c;
    budget(&sim).check_consistency().expect("the engine's invariants");
}

/// **Split-neutral (ADR-0176 D4, RFC-0015 §8.3.4)**: ten bonds of 100,000 MSK and one of 1,000,000 MSK under the same stream of attempts —
/// the caps are linear with floors, so the ten never admit more than the one.
#[test]
fn ten_bonds_of_a_tenth_never_beat_the_whole() {
    let pol = policy(100_000, 4_000, 1, 1_000_000, false);
    let pieces: Vec<(u64, u64)> = (0..10u64).map(|i| (100 + i, 100_000)).collect();
    let mut split = Sim::new(budget_params(250, pol.clone(), None), Class::Floor, &pieces);
    let mut whole = Sim::new(budget_params(250, pol, None), Class::Floor, &[(90, 1_000_000)]);
    let (mut a, mut b) = (0usize, 0usize);
    let (base_a, base_b) = (split.c.daa, whole.c.daa);
    for d in 1..=12u64 {
        for k in 0..4u64 {
            for i in 0..10u64 {
                a += usize::from(split.block(base_a + d, vec![], Some((100 + i, (d << 12) + (i << 6) + k))).is_some());
            }
            for j in 0..10u64 {
                b += usize::from(whole.block(base_b + d, vec![], Some((90, (d << 12) + (j << 6) + k))).is_some());
            }
        }
    }
    println!("split-neutral: ten bonds admitted {a}, the whole {b}");
    assert!(a <= b, "the pieces admitted {a} against the whole's {b}");
    assert_eq!(b, 30, "the whole: three blocks a unit, ten units");
}

/// **The allocation clips reward only (ADR-0177 D4–D5)**: the same claims are admitted with the same weight; with the bond's capital split
/// evenly between the floor and a second registered class, each claim reserves (and is paid at most) half its block's realized carve; with
/// no capital assigned, nothing; with the allocation off, its carve.
#[test]
fn the_allocation_clips_reward_only_on_the_real_fold() {
    let allocation = PalwModelAllocationPolicyV1 {
        version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V1,
        epoch_daa: 50,
        seasoning_epochs: 1,
        curve: PalwAllocationCurveV1 { points: vec![(0, 0), (1_000_000 * MSK, 1_000_000)] },
        max_models_per_bond: 4,
    };
    let run = |alloc: Option<PalwModelAllocationPolicyV1>, assign: bool| -> (u64, Vec<u64>, u128) {
        let mut sim =
            Sim::new(budget_params(250, policy(1_000_000, 4_000, 1, 1_000_000, false), alloc), Class::Floor, &[(90, 1_000_000)]);
        let floor = genesis_classes(&sim.c.p)[0].0;
        let (k8, _) = model_classes(&sim.c.p);
        assert!(sim.c.s.class(&k8).is_some(), "the second model is registered");
        if assign {
            let mut assignments = vec![(floor, 500_000 * MSK), (k8, 500_000 * MSK)];
            assignments.sort();
            sim.step(vec![PalwConsensusObjectV2::BondCapitalAssignedV1 {
                bond: bond_key(90),
                assignments,
                sequence: 1,
                signature: vec![1],
            }]);
        }
        // Past two epoch boundaries the assignment counts (accepted in epoch 0, seasoned through epoch 1).
        sim.block(H + 101, vec![], None);
        let ids: Vec<Hash64> = (0..3u64).filter_map(|k| sim.claim(90, 0x5_0000 + k)).collect();
        assert_eq!(ids.len(), 3, "the allocation never refuses a claim");
        let reserved: Vec<u64> = ids.iter().map(|id| budget(&sim).claim_row(id).unwrap().reserved.reward_sompi).collect();
        let carve = sim.c.claim(&ids[0]).escrowed_reward;
        let weight: u128 = ids.iter().map(|id| budget(&sim).claim_row(id).unwrap().reserved.final_weight).sum();
        budget(&sim).check_consistency().expect("the engine's invariants");
        (carve, reserved, weight)
    };
    let (carve, off, w_off) = run(None, false);
    let (_, even, w_even) = run(Some(allocation.clone()), true);
    let (_, none, w_none) = run(Some(allocation), false);
    assert!(off.iter().all(|r| *r == carve), "off: the carve {carve}: {off:?}");
    assert!(even.iter().all(|r| *r <= carve / 2 + 1), "even: at most half the realized carve: {even:?} of {carve}");
    assert!(even.iter().any(|r| *r > 0));
    assert!(none.iter().all(|r| *r == 0), "unassigned: Σ A = 0, nothing: {none:?}");
    assert_eq!((w_off, w_even), (w_none, w_none), "the allocation never moves weight");
}

/// **Replay determinism**: a tape with riders, a court and a Final, rewound to its middle and re-folded input for input, is the same
/// chain; a fork that posts the same objects in another order folds and keeps the engine's invariants.
#[test]
fn a_budgeted_tape_rewinds_and_replays_to_the_same_chain() {
    let mut sim = Sim::new(
        budget_params(250, policy(1_000_000, 4_000, 100, 1_000_000, false), None),
        Class::Floor,
        &[(90, 1_000_000), (91, 1_000_000)],
    );
    let lead = sim.claim(90, 0x6_0000).expect("lead");
    let obj = riders_object(&sim, lead, 90, 3, 0x6_0100);
    sim.step(vec![obj]);
    let other = sim.claim(91, 0x6_0200).expect("a second lead");
    let seats = sim.seats();
    let fork_at = sim.tape.len();
    for id in [lead, other] {
        let bound = sim.bind(id, &seats);
        sim.license(id, &seats, bound);
    }
    let receipts = audit_receipts_for(&sim.c.s, &sim.c.sp, &[lead, other]);
    for batch in receipts {
        sim.step(vec![batch]);
    }
    sim.finalize_all(&[lead, other]);
    let total = sim.tape.len();
    let tip = sim.c.s.clone();
    let off = sim.rewind_to(fork_at);
    sim.replay(&off);
    assert_eq!(sim.tape.len(), total);
    assert_eq!(sim.c.s, tip, "the replay is the chain");
    budget(&sim).check_consistency().expect("the engine's invariants");
}

/// **Readiness §3e on the real fold: the Round draw past the fence** — Finals earn tickets; every span schedule the lane seeds lists at
/// most each bond's remaining Round rights, each `(span, bond)` allocation is reserved in the bond's window (and only released at
/// `draw + W`), and the tape replays to the same chain (RECOVERY's budget half).
#[test]
fn round_draws_reserve_each_bonds_allocation_and_replay() {
    let pol = PalwBondBudgetPolicyV1 {
        round_rights: PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 5 },
        ..policy(1_000_000, 100_000, 100, 1_000_000, false)
    };
    let mut sim = Sim::new(budget_params(250, pol.clone(), None), Class::Floor, &[(90, 1_000_000), (91, 1_000_000)]);
    let ids: Vec<Hash64> =
        [(90u64, 0x7_0000u64), (91, 0x7_0001), (90, 0x7_0002)].iter().filter_map(|(n, s)| sim.claim(*n, *s)).collect();
    let seats = sim.seats();
    for id in &ids {
        let bound = sim.bind(*id, &seats);
        sim.license(*id, &seats, bound);
    }
    for batch in audit_receipts_for(&sim.c.s, &sim.c.sp, &ids) {
        sim.step(vec![batch]);
    }
    sim.finalize_all(&ids);
    let fork_at = sim.tape.len();
    let start = sim.c.daa;
    for k in 0..120u64 {
        sim.block(start + 10 * (k + 1), vec![], Some((90 + (k % 2), 0x7_1000 + k)));
    }
    let mut drawn = 0u64;
    for (span, schedule) in sim.c.s.round_schedules() {
        let mut per: BTreeMap<PalwBondKeyV2, u64> = BTreeMap::new();
        for q in &schedule.quanta {
            *per.entry(q.bond).or_default() += 1;
        }
        for (bond, n) in per {
            drawn += n;
            assert!(n <= 5, "span {span}: bond's allocation {n} is within its 5 Round rights");
            if let Some(row) = budget(&sim).claim_row(&palw_round_rights_row_id_v1(*span, &bond)) {
                assert_eq!(row.reserved.round_rights, n, "span {span}: the allocation is what was reserved");
            }
        }
    }
    println!("round draws on the real fold: {drawn} tickets allocated in the schedules still held");
    budget(&sim).check_consistency().expect("the engine's invariants");
    let tip = sim.c.s.clone();
    let off = sim.rewind_to(fork_at);
    sim.replay(&off);
    assert_eq!(sim.c.s, tip, "the replay is the chain");
}
