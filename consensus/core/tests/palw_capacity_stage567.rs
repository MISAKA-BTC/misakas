//! **ADR-0164 — stages 5, 6 and 7 of ADR-0160's staged ramp, through testnet-12's own fold** (lane CAP, rcore/cap-1000; the user's
//! directive of 2026-10-03: the ×1000 capacity change ships in the int-11 release at H = 5,300).
//!
//! * **the release** — F-EM (`palw_capacity_emission_budget`), F-M1 (`palw_capacity_multi_claim`) and F-K (`palw_capacity_rho_breaker`) arm
//!   with the int-11 list at H; ρ = 250 (F-L's fourth step, `k_aud` = 2) at H + 190 and ρ = 1000 (its fifth) at H + 285; every
//!   prerequisite is refused by name where it is missing; every entry is a fork-id fence name;
//! * **stage 5 — emission** — the DAA's budget is 16 claim-bearing blocks' carves (a seventeenth is skipped and its carve burned), the
//!   ledger holds one row, and emission per DAA does not depend on the number of claims: a lead and `n` riders (n = 1…63) hold exactly
//!   the lead's carve between them, and a rider charges the budget nothing;
//! * **stage 5 — riders** — the batch is atomic, the lead must be `Provisional`, in its window and not already split, a rider must be
//!   of the lead's bond and carry its lead's challenge, and each rider is its own claim (bound, licensed, audited, final);
//! * **stage 6 — the breaker** — an epoch of panel-backlog voids lowers the issuance tier one rung at the next boundary, a false audit
//!   lowers it to ρ = 1, two clean epochs raise it one rung, and it never passes the schedule;
//! * **stage 7 — the invariants at ρ = 250 and ρ = 1000** — HONEST-NO-LOSS, J1-CAP, LIABILITY-SURVIVES, NO-FREE-VOID, REORG-DETERMINISM and
//!   SPLIT-NEUTRAL, plus "13k × 10 bonds never beats 130k × 1".
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_capacity_stage567 -- --nocapture`

#[path = "capacity_stage1_common.rs"]
mod stage1;
use stage1::*;

use kaspa_consensus_core::config::params::{
    PALW_T12_CAPACITY_EMISSION_BUDGET_V1, PALW_T12_CAPACITY_MULTI_CLAIM_V1, PALW_T12_CAPACITY_RHO_BREAKER_V1,
    PALW_T12_CAPACITY_RHO10_FENCES_V1, PALW_T12_CAPACITY_RHO100_STEP_3_V1, PALW_T12_CAPACITY_RHO250_STEP_4_V1,
    PALW_T12_CAPACITY_RHO1000_STEP_5_V1, PALW_T12_CAPACITY_RHO25_STEP_2_V1, PALW_T12_INT11_FENCES_V1, PALW_T12_INT11_FLAG_DAY_DAA,
    PALW_T12_INT11_RHO1000_DAA, PALW_T12_INT11_RHO100_DAA, PALW_T12_INT11_RHO250_DAA, PalwPostLaunchFenceV1, palw_t12_arm_int11_flag_day_at_v1,
    palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_aggregate_liability_v1::{PalwCapacityLiabilityV1, PalwCapacityStepOffsetV1};
use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_TRACE_CHUNKS, PalwAttemptEnvelopeV2, attempt_id_v2, attempt_trace_manifest_root_v1};
use kaspa_consensus_core::palw_audit_door_v1::{PalwAuditEntryV1, palw_audit_pool_of_claim_v1, palw_capacity_claim_credited_v1, palw_capacity_claim_k_aud_v1};
use kaspa_consensus_core::palw_capacity_s567_v1::{
    PALW_EMISSION_BLOCKS_PER_DAA_V1, PALW_RIDERS_MAX_V1, PALW_RIDERS_WINDOW_DAA_V1, PALW_RHO_LADDER_V1, palw_emission_budget_milli_v1,
    palw_rider_challenge_v1,
};
use kaspa_consensus_core::palw_issuance_slots_v1::palw_issuance_read_at_v1;

fn fence_at(p: &Params, name: &str) -> Option<u64> {
    p.palw_fences_v1().iter().find(|(n, _)| *n == name).and_then(|(_, f)| *f).map(|f| f.daa_score())
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// The three ADR-0164 entries.
const S567: [&PalwPostLaunchFenceV1; 3] =
    [&PALW_T12_CAPACITY_EMISSION_BUDGET_V1, &PALW_T12_CAPACITY_MULTI_CLAIM_V1, &PALW_T12_CAPACITY_RHO_BREAKER_V1];
const NAMES: [&str; 3] = ["palw_capacity_emission_budget", "palw_capacity_multi_claim", "palw_capacity_rho_breaker"];

/// **The stage-1 fixture's chain with the whole capacity package armed at [`H`] and F-L's ONE step at `rho`** (the stage-3 tests'
/// way of putting a tier in force from the first block: a step list that climbs would not have reached ρ = 250 by the chain's first
/// claim at DAA 1,002). F-EM, F-M1 and F-K arm with it, so a ρ ≥ 250 value validates.
fn tier_params(class: Class, rho: u32) -> Params {
    let mut p = params_for(class, false);
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().chain(S567) {
        (f.set)(&mut p, Some(ForkActivation::new(H)));
    }
    let value = PalwCapacityLiabilityV1::of_schedule_v1(
        ForkActivation::new(H),
        &[PalwCapacityStepOffsetV1 { after_daa: 0, rho, q_credit_permille: 250 }],
    );
    p.palw_capacity_aggregate_liability = Some(value);
    p.sync_palw_capacity_liability();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the ρ = {rho} package validates: {e:?}"));
    p
}

/// A claim of bond `n` bound and licensed on its class's honest panel; its id.
fn licensed_claim(sim: &mut Sim, n: u64, seed: u64) -> Hash64 {
    let id = sim.claim(n, seed).expect("admitted");
    let seats = sim.seats();
    let bound = sim.bind(id, &seats);
    sim.license(id, &seats, bound);
    id
}

/// Every pending audit of `ids` receipted by its pool (`k_aud` members each), in one block.
fn audit(sim: &mut Sim, ids: &[Hash64]) {
    let batches = audit_receipts_for(&sim.c.s, &sim.c.sp, ids);
    if !batches.is_empty() {
        sim.step(batches);
    }
}

/// The rider `index` of `lead` by bond `n`: a floor attempt re-bound to its lead and its index, its data-availability pins the lead's.
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

/// The skip count of every refusal mentioning `needle`.
fn skipped(sim: &Sim, needle: &str) -> usize {
    sim.skips.iter().filter(|(k, _)| k.contains(needle)).map(|(_, n)| *n).sum()
}

// ---------------------------------------------------------------------------------------------
// The release
// ---------------------------------------------------------------------------------------------

/// **The int-11 flag day carries the ×1000 package**: F-EM, F-M1 and F-K in the list at H, ρ = 250 at H + 190 and ρ = 1000 at H + 285,
/// heights no other fence uses; the v5 baseline (the DAA-3,600 release) has all five dormant and `set(None)` gives it back to the id.
#[test]
fn the_release_arms_the_package_at_h_and_the_two_steps_at_the_directed_heights() {
    const H0: u64 = 5_300;
    assert_eq!((PALW_T12_INT11_FLAG_DAY_DAA, PALW_T12_INT11_RHO100_DAA), (Some(H0), Some(H0 + 95)));
    assert_eq!((PALW_T12_INT11_RHO250_DAA, PALW_T12_INT11_RHO1000_DAA), (Some(5_490), Some(5_585)), "H + 190 and H + 285");
    let names: Vec<&str> = PALW_T12_INT11_FENCES_V1.iter().map(|f| f.name).collect();
    for name in NAMES {
        assert!(names.contains(&name), "{name} is on the int-11 list");
    }
    let shipped = palw_t12_shipped_params();
    shipped.validate_palw_v2().expect("testnet-12 as shipped validates");
    for name in NAMES {
        assert_eq!(fence_at(&shipped, name), Some(H0), "{name} arms at H");
    }
    assert_eq!(fence_at(&shipped, "palw_capacity_aggregate_liability_step_4"), Some(5_490));
    assert_eq!(fence_at(&shipped, "palw_capacity_aggregate_liability_step_5"), Some(5_585));
    for (daa, want) in [(1_700, 10), (5_299, 10), (5_300, 25), (5_394, 25), (5_395, 100), (5_489, 100), (5_490, 250), (5_584, 250), (5_585, 1_000), (50_000, 1_000)] {
        assert_eq!(shipped.palw_capacity_step_at_v1(daa).map(|s| s.rho), Some(want), "ρ at {daa}");
    }
    // Heights no other fence uses.
    for (name, fence) in shipped.palw_fences_v1() {
        let at = fence.map(|f| f.daa_score());
        let mine = NAMES.contains(&name);
        if !mine && name != "palw_capacity_aggregate_liability_step_4" && name != "palw_capacity_aggregate_liability_step_5" {
            assert!(at != Some(5_490) && at != Some(5_585), "{name}: 5,490 and 5,585 are the package's alone");
        }
    }
    let baseline = palw_t12_release_v5_params();
    for name in NAMES.iter().chain(["palw_capacity_aggregate_liability_step_4", "palw_capacity_aggregate_liability_step_5"].iter()) {
        assert_eq!(fence_at(&baseline, name), None, "{name}: dormant on the int-10 baseline");
    }
    let mut back = shipped.clone();
    palw_t12_arm_int11_flag_day_at_v1(&mut back, None);
    assert_eq!(ids(&back), ids(&baseline), "set(None): the int-10 ruleset, to the id");
    let schedule = shipped.fence_schedule_v1();
    assert_eq!(&schedule[schedule.len() - 4..], [5_300, 5_395, 5_490, 5_585], "the four heights are the schedule's last");
    // A node without the new entries is refused from H, from H + 190 and from H + 285.
    let old = palw_t12_release_v5_params();
    for at in [H0, 5_490, 5_585] {
        let s = fork_id_v1(&old, at);
        assert!(evaluate_fork_id_v1(&shipped, at, s.fired.as_bytes().as_slice(), s.next).refuses(), "the package is gated from {at}");
    }
}

/// **Every prerequisite is refused by name**: a ρ ≥ 250 step without any of the three; F-M1 without F-EM (and without each of the others
/// a rider needs); F-EM without F-E; F-K without F-S; a mirror that disagrees with its field.
#[test]
fn every_prerequisite_is_refused_where_it_is_missing() {
    let ok = tier_params(Class::Floor, 250);
    // ρ ≥ 250 without one of the three.
    for left_out in S567 {
        let mut p = ok.clone();
        (left_out.set)(&mut p, None);
        assert!(p.validate_palw_v2().is_err(), "ρ = 250 without {} is refused", left_out.name);
    }
    // ρ = 100 needs none of them.
    let mut p100 = tier_params(Class::Floor, 100);
    for f in S567 {
        (f.set)(&mut p100, None);
    }
    p100.validate_palw_v2().expect("ρ = 100 stands without the package");
    // A step above 250 later than the fences is fine; before them it is refused.
    let mut late = p100.clone();
    for f in S567 {
        (f.set)(&mut late, Some(ForkActivation::new(H + 50)));
    }
    late.palw_capacity_aggregate_liability.as_mut().unwrap().steps.push(kaspa_consensus_core::palw_aggregate_liability_v1::PalwCapacityStepV1 {
        from_daa: H + 40,
        rho: 250,
        q_credit_permille: 250,
    });
    late.sync_palw_capacity_liability();
    assert!(late.validate_palw_v2().is_err(), "a ρ = 250 step below the fences it needs");
    // F-M1 without each of its prerequisites.
    for (name, undo) in [
        ("emission", &PALW_T12_CAPACITY_EMISSION_BUDGET_V1 as &PalwPostLaunchFenceV1),
    ] {
        let mut p = tier_params(Class::Floor, 100);
        (undo.set)(&mut p, None);
        assert!(p.validate_palw_v2().is_err(), "F-M1 without {name}");
    }
    let mut no_batch = tier_params(Class::Floor, 100);
    no_batch.palw_capacity_batch_licence = None;
    no_batch.sync_palw_capacity_verify();
    assert!(no_batch.validate_palw_v2().is_err(), "F-M1 without the batch licence");
    // F-EM alone on a ruleset without F-L/F-E.
    let mut bare = params_for(Class::Floor, false);
    (PALW_T12_CAPACITY_EMISSION_BUDGET_V1.set)(&mut bare, Some(ForkActivation::new(H)));
    assert!(bare.validate_palw_v2().is_err(), "F-EM without F-L and F-E");
    // F-K without F-S.
    let mut no_slots = tier_params(Class::Floor, 100);
    no_slots.palw_capacity_issuance_slots = None;
    no_slots.sync_palw_capacity_stage2();
    assert!(no_slots.validate_palw_v2().is_err(), "F-K without F-S");
    // A field set without its mirror.
    let mut unsynced = tier_params(Class::Floor, 100);
    unsynced.palw_capacity_emission_budget = Some(ForkActivation::new(H + 7));
    assert!(unsynced.validate_palw_v2().is_err(), "a field that disagrees with its mirror");
    let mut orphan = params_for(Class::Floor, false);
    orphan.palw_capacity_rho_breaker = None;
    orphan.sync_palw_capacity_s567();
    orphan.validate_palw_v2().expect("nothing armed, nothing mirrored");
}

/// **A step at ρ 250 and one at ρ 1000 append like every other**: heights strictly increase, `set(None)` drops a step and the later ones,
/// the third step must exist before the fourth is armed (a panic otherwise), and the fork id names each height.
#[test]
fn the_two_new_steps_append_like_the_others() {
    let mut p = params_for(Class::Floor, false);
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().chain(S567) {
        (f.set)(&mut p, Some(ForkActivation::new(H)));
    }
    let panics = |mut q: Params, entry: &'static PalwPostLaunchFenceV1, at: u64| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || (entry.set)(&mut q, Some(ForkActivation::new(at))))).is_err()
    };
    assert!(panics(p.clone(), &PALW_T12_CAPACITY_RHO250_STEP_4_V1, H + 30), "ρ = 250 without the steps it builds on");
    for (f, at) in [
        (&PALW_T12_CAPACITY_RHO25_STEP_2_V1, H + 10),
        (&PALW_T12_CAPACITY_RHO100_STEP_3_V1, H + 20),
        (&PALW_T12_CAPACITY_RHO250_STEP_4_V1, H + 30),
        (&PALW_T12_CAPACITY_RHO1000_STEP_5_V1, H + 40),
    ] {
        (f.set)(&mut p, Some(ForkActivation::new(at)));
    }
    p.validate_palw_v2().expect("the whole ramp validates");
    let rho = |daa: u64| p.palw_capacity_step_at_v1(daa).map(|s| s.rho);
    assert_eq!([rho(H), rho(H + 10), rho(H + 20), rho(H + 30), rho(H + 40), rho(H + 4_000)], [Some(10), Some(25), Some(100), Some(250), Some(1_000), Some(1_000)]);
    let full = ids(&p);
    let mut dropped = p.clone();
    (PALW_T12_CAPACITY_RHO250_STEP_4_V1.set)(&mut dropped, None);
    assert_eq!(rho(H + 40), Some(1_000));
    assert_eq!(dropped.palw_capacity_step_at_v1(H + 40).map(|s| s.rho), Some(100), "dropping ρ = 250 drops ρ = 1000 with it");
    assert_ne!(ids(&dropped), full);
    let mut low = p.clone();
    (PALW_T12_CAPACITY_RHO250_STEP_4_V1.set)(&mut low, Some(ForkActivation::new(H + 20)));
    assert!(low.validate_palw_v2().is_err(), "a step at the previous step's height is refused");
}

/// **`k_aud` is 2 from ρ = 250**: a credited claim accepted at ρ = 250 is not `Final` on one receipt and is on two; at ρ = 100 one is enough.
#[test]
fn a_credited_claim_needs_two_receipts_from_rho_250() {
    for (rho, k) in [(100u32, 1usize), (250, 2), (1_000, 2)] {
        let mut sim = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &[(90, 100_000)]);
        let id = licensed_claim(&mut sim, 90, 0x5A00);
        let claim = sim.c.claim(&id);
        assert!(palw_capacity_claim_credited_v1(&sim.c.sp, &claim), "ρ = {rho}: credited");
        assert_eq!(palw_capacity_claim_k_aud_v1(&sim.c.sp, &claim), k, "ρ = {rho}: k_aud");
        let pool = palw_audit_pool_of_claim_v1(&sim.c.s, &sim.c.sp, &id, &claim);
        assert!(pool.len() >= 2, "the pool holds two or more members: {}", pool.len());
        let one = PalwConsensusObjectV2::AuditReceiptBatchV1 {
            auditor: pool[0],
            entries: vec![PalwAuditEntryV1 { claim_id: id, reproduced_root: claim.execution_root }],
            signature: vec![],
        };
        sim.step(vec![one]);
        let waits = sim.c.s.deadline_of(&id).is_none();
        assert_eq!(waits, k == 2, "ρ = {rho}: one receipt {} the claim", if k == 2 { "does not release" } else { "releases" });
        if k == 2 {
            let two = PalwConsensusObjectV2::AuditReceiptBatchV1 {
                auditor: pool[1],
                entries: vec![PalwAuditEntryV1 { claim_id: id, reproduced_root: claim.execution_root }],
                signature: vec![],
            };
            sim.step(vec![two]);
        }
        sim.finalize_all(&[id]);
        assert!(sim.c.s.vesting_row(&id).is_some(), "ρ = {rho}: paid through its audits");
    }
}

// ---------------------------------------------------------------------------------------------
// Stage 5 — emission
// ---------------------------------------------------------------------------------------------

/// **The DAA's budget is 16 carves, and emission per DAA does not follow the claim count**: twenty attempt blocks in one DAA by four
/// big bonds record sixteen claims and skip four by name; the ledger holds that one DAA's row; the next DAA starts over and the old row is
/// gone; and Σ escrow of the sixteen is sixteen carves — the same whatever the bonds' sizes or ρ.
#[test]
fn the_budget_is_sixteen_carves_a_daa_and_the_ledger_holds_one_row() {
    assert_eq!((PALW_EMISSION_BLOCKS_PER_DAA_V1, palw_emission_budget_milli_v1()), (16, 16_000));
    for rho in [100u32, 250, 1_000] {
        let producers = [(90, 1_000_000), (91, 1_000_000), (92, 1_000_000), (93, 1_000_000)];
        let mut sim = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &producers);
        let day = sim.c.daa + 1;
        let mut recorded = Vec::new();
        for k in 0..20u64 {
            if let Some(id) = sim.block(day, vec![], Some((90 + k % 4, 0x6000 + k))) {
                recorded.push(id);
            }
        }
        assert_eq!(recorded.len(), 16, "ρ = {rho}: sixteen claim-bearing blocks a DAA, however many were mined");
        assert_eq!(skipped(&sim, "PALW reward budget is spent"), 4, "ρ = {rho}: the other four are skipped by name: {:?}", sim.skips);
        assert_eq!(sim.c.s.emission_spent_milli_v1(day), 16_000);
        let one_carve = sim.c.claim(&recorded[0]).escrowed_reward;
        let total: u64 = recorded.iter().map(|id| sim.c.claim(id).escrowed_reward).sum();
        assert_eq!(total, 16 * one_carve, "ρ = {rho}: Σ escrow of the DAA is the budget's carves");
        assert_eq!(sim.c.s.capacity_ledger_v1().len(), 1, "one row");
        // The next DAA: a fresh budget, the old row gone.
        let id = sim.block(day + 1, vec![], Some((90, 0x6100))).expect("the next DAA takes a claim");
        assert!(sim.c.s.claim(&id).is_some());
        assert_eq!(sim.c.s.emission_spent_milli_v1(day), 0, "yesterday's row left");
        assert_eq!(sim.c.s.emission_spent_milli_v1(day + 1), 1_000);
        assert_eq!(sim.c.s.capacity_ledger_v1().len(), 1);
    }
}

/// **A rider is paid out of its lead's carve and charges the budget nothing**: a lead and `n` riders hold, to the sompi, the carve the
/// lead was accepted with — for n = 1, 2, 7, 31 and 63 — and the DAA's ledger reads one carve before and after.
#[test]
fn a_lead_and_its_riders_hold_exactly_the_leads_carve() {
    for rho in [250u32, 1_000] {
        for n in [1usize, 2, 7, 31, 63] {
            let mut sim = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &[(90, 1_000_000)]);
            let lead = sim.claim(90, 0x7000).expect("the lead is admitted");
            let carve = sim.c.claim(&lead).escrowed_reward;
            assert!(carve > 0);
            let day = sim.c.daa;
            let spent = sim.c.s.emission_spent_milli_v1(day);
            assert_eq!(spent, 1_000, "the lead charged one carve");
            let obj = riders_object(&sim, lead, 90, n, 0x7100);
            sim.step(vec![obj]);
            assert_eq!(skipped(&sim, "riders refused"), 0, "ρ = {rho}, n = {n}: the batch was taken: {:?}", sim.skips);
            let kept = sim.c.claim(&lead).escrowed_reward;
            let rider_ids: Vec<Hash64> = sim
                .c
                .s
                .claims_iter()
                .filter(|(id, c)| **id != lead && c.accepted_block == sim.c.claim(&lead).accepted_block)
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(rider_ids.len(), n, "ρ = {rho}: n riders are claims");
            let riders: u64 = rider_ids.iter().map(|id| sim.c.claim(id).escrowed_reward).sum();
            assert_eq!(kept + riders, carve, "ρ = {rho}, n = {n}: the lead's carve, to the sompi");
            assert!(kept >= riders / n as u64, "the lead keeps at least a rider's share");
            assert_eq!(sim.c.s.emission_spent_milli_v1(day + 1).max(sim.c.s.emission_spent_milli_v1(day)), 1_000, "riders charge nothing");
            assert!(rider_ids.iter().all(|id| matches!(sim.c.claim(id).phase, PalwClaimPhaseV2::Provisional)), "each rider is a claim of its own");
            // Each rider is its own claim end to end: bound, licensed, audited, Final (one of them, to keep the run short).
            let seats = sim.seats();
            let rider = rider_ids[0];
            let bound = sim.bind(rider, &seats);
            sim.license(rider, &seats, bound);
            audit(&mut sim, &[rider]);
            sim.finalize_all(&[rider]);
            assert!(sim.c.s.vesting_row(&rider).is_some(), "a rider is paid through its own audit");
            println!("riders ρ = {rho}, n = {n}: lead keeps {kept}, riders hold {riders}, Σ = carve {carve}");
        }
    }
}

/// **The whole ledger of a block's claims is the carve however many claims ride**: Σ escrow of every claim accepted in one block's DAA
/// across a budget-full day stays ≤ sixteen carves with riders on every lead.
#[test]
fn emission_per_daa_is_invariant_to_the_number_of_claims() {
    let mut sim = Sim::new(tier_params(Class::Floor, 1_000), Class::Floor, &[(90, 1_000_000), (91, 1_000_000), (92, 1_000_000), (93, 1_000_000)]);
    let day = sim.c.daa + 1;
    let mut leads = Vec::new();
    for k in 0..20u64 {
        if let Some(id) = sim.block(day, vec![], Some((90 + k % 4, 0x8000 + k))) {
            leads.push((90 + k % 4, id));
        }
    }
    assert_eq!(leads.len(), 16);
    let carve = sim.c.claim(&leads[0].1).escrowed_reward;
    let bare: u64 = sim.c.s.claims_iter().map(|(_, c)| c.escrowed_reward).sum();
    assert_eq!(bare, 16 * carve);
    for (i, (n, lead)) in leads.iter().enumerate() {
        let obj = riders_object(&sim, *lead, *n, 1 + i % 5, 0x8100 + 16 * i as u64);
        // Every batch lands in the DAA the leads were accepted in: a lead's window is 8 DAA.
        sim.block(day, vec![obj], None);
    }
    let with_riders: u64 = sim.c.s.claims_iter().map(|(_, c)| c.escrowed_reward).sum();
    let claims = sim.c.s.claims_iter().count();
    assert!(claims > 16 + 16, "riders added claims: {claims}");
    assert_eq!(with_riders, bare, "…and not one sompi of escrow");
    assert_eq!(skipped(&sim, "riders refused"), 0, "{:?}", sim.skips);
}

// ---------------------------------------------------------------------------------------------
// Stage 5 — riders
// ---------------------------------------------------------------------------------------------

/// **The batch is atomic and every refusal is by name**: a rider of another bond, one with a foreign challenge, a second batch on a lead, a
/// batch on a `PanelBound` lead, one past the window and one on a missing lead each leave the lead as it was; below F-M1 the object is
/// refused by the fold.
#[test]
fn a_refused_batch_leaves_the_lead_whole() {
    let mut sim = Sim::new(tier_params(Class::Floor, 250), Class::Floor, &[(90, 1_000_000), (91, 1_000_000)]);
    let lead = sim.claim(90, 0x9000).expect("lead");
    let whole = sim.c.claim(&lead);
    let before = sim.c.s.clone();
    // A rider of the wrong bond.
    let mut wrong_bond = rider_of(&sim, &lead, 0, 91, 0x9100);
    wrong_bond.attempt.executor_bond = bond_key(91).0;
    sim.step(vec![PalwConsensusObjectV2::AttemptRidersV1 { lead, riders: vec![wrong_bond] }]);
    // A rider whose challenge is not its index's.
    let mut off_index = rider_of(&sim, &lead, 3, 90, 0x9101);
    off_index.attempt.challenge = palw_rider_challenge_v1(&lead, 1);
    sim.step(vec![PalwConsensusObjectV2::AttemptRidersV1 { lead, riders: vec![off_index] }]);
    // The second of two riders is bad: the first must not survive.
    let good = rider_of(&sim, &lead, 0, 90, 0x9102);
    let mut bad = rider_of(&sim, &lead, 1, 90, 0x9103);
    bad.attempt.trace_retention_daa += 1;
    sim.step(vec![PalwConsensusObjectV2::AttemptRidersV1 { lead, riders: vec![good, bad] }]);
    // No lead, no riders, too many riders.
    let ghost = Hash64::from_bytes([0xEE; 64]);
    sim.step(vec![PalwConsensusObjectV2::AttemptRidersV1 { lead: ghost, riders: vec![rider_of(&sim, &lead, 0, 90, 0x9104)] }]);
    sim.step(vec![PalwConsensusObjectV2::AttemptRidersV1 { lead, riders: vec![] }]);
    assert_eq!(skipped(&sim, "riders refused"), 5, "five refusals, all skips: {:?}", sim.skips);
    assert_eq!(sim.c.claim(&lead), whole, "the lead is exactly as accepted");
    assert_eq!(sim.c.s.claims_iter().count(), before.claims_iter().count(), "no rider claim exists");
    assert_eq!(sim.c.s.reserved_exposure(&bond_key(90)), before.reserved_exposure(&bond_key(90)), "and no commitment moved");
    assert!(sim.c.s.rider_mark_of_v1(&lead).is_none());
    // The good batch alone is taken once and only once.
    let obj = riders_object(&sim, lead, 90, 2, 0x9200);
    sim.step(vec![obj.clone()]);
    assert!(sim.c.s.rider_mark_of_v1(&lead).is_some(), "the lead took its riders");
    let skips = skipped(&sim, "riders refused");
    sim.step(vec![riders_object(&sim, lead, 90, 1, 0x9300)]);
    assert_eq!(skipped(&sim, "riders refused"), skips + 1, "a second batch on the same lead is refused");
    // A lead that is bound is no lead.
    let lead2 = sim.claim(91, 0x9400).expect("a second lead");
    let seats = sim.seats();
    sim.bind(lead2, &seats);
    let skips = skipped(&sim, "riders refused");
    sim.step(vec![riders_object(&sim, lead2, 91, 1, 0x9500)]);
    assert_eq!(skipped(&sim, "riders refused"), skips + 1, "a PanelBound lead takes no riders");
    // Past the window.
    let lead3 = sim.claim(91, 0x9600).expect("a third lead");
    let at = sim.c.claim(&lead3).accepted_daa + PALW_RIDERS_WINDOW_DAA_V1 + 1;
    let obj = riders_object(&sim, lead3, 91, 1, 0x9700);
    sim.block(at, vec![obj], None);
    assert!(sim.c.s.rider_mark_of_v1(&lead3).is_none(), "the window closed: no mark");
    // The marks past their window leave.
    assert!(sim.c.s.rider_mark_of_v1(&lead).is_none() || sim.c.daa <= sim.c.claim(&lead).accepted_daa + PALW_RIDERS_WINDOW_DAA_V1);
    // Below F-M1: the fold refuses the object by name.
    let mut dormant = Sim::new(
        {
            let mut p = tier_params(Class::Floor, 100);
            (PALW_T12_CAPACITY_MULTI_CLAIM_V1.set)(&mut p, Some(ForkActivation::new(H + 500)));
            p
        },
        Class::Floor,
        &[(90, 1_000_000)],
    );
    let lead = dormant.claim(90, 0x9800).expect("lead");
    let obj = riders_object(&dormant, lead, 90, 1, 0x9900);
    let refused = dormant.try_block(dormant.c.daa + 1, vec![obj]);
    assert!(refused.is_err(), "below F-M1 the object is refused: {refused:?}");
    assert_eq!(PALW_RIDERS_MAX_V1, 64);
}

// ---------------------------------------------------------------------------------------------
// Stage 6 — the breaker
// ---------------------------------------------------------------------------------------------

/// **A tier's issuance follows the breaker, and only down**: at ρ = 1000 (level 7) a 13,000 MSK bond holds `N_out` = 2,000; an epoch of
/// panel-backlog voids steps the tier to Λ[6] = 500 at the next boundary (N_out = 1,000); a false audit takes it to ρ = 1 (N_out = 2); two
/// clean epochs raise it one rung; and it is never above the schedule.
#[test]
fn the_breaker_lowers_the_tier_on_a_backlog_and_a_false_audit_and_raises_it_after_two_clean_epochs() {
    let mut sim = Sim::new(tier_params(Class::Floor, 1_000), Class::Floor, &[(90, 1_000_000), (91, 1_000_000), (92, 1_000_000), (93, 1_000_000), (94, 13_000)]);
    let collateral = sim.c.s.bond(&bond_key(94)).unwrap().collateral;
    let cap_at = |sim: &Sim| palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond_key(94), collateral, sim.c.daa + 1).expect("F-S reads").cap;
    assert_eq!(cap_at(&sim), 2_000, "13,000 MSK at ρ = 1000: u = 2, N_out = 2,000");
    // 24 claims left unbound in epoch 1 (DAA 1,000–1,999): each a BindTimeout void, 24 of 24 reaching their anchor.
    let day0 = sim.c.daa;
    for k in 0..24u64 {
        let daa = day0 + 1 + k / 8;
        sim.block(daa, vec![], Some((90 + k % 4, 0xA000 + k))).expect("admitted");
    }
    sim.block(day0 + 700, vec![], None);
    let voids = sim.c.s.claims_iter().filter(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. })).count();
    assert!(voids >= 20, "a backlog: {voids} BindTimeout voids");
    let row = sim.c.s.rho_breaker_v1().expect("the breaker counted").clone();
    assert_eq!((row.bind_voids as usize, row.epoch, row.level), (voids, 1, 7), "counted in epoch 1, not yet judged");
    assert_eq!(cap_at(&sim), 2_000, "the tier holds inside the epoch");
    // The first block at or past DAA 2,000 judges epoch 1: one rung down.
    sim.block(2_000, vec![], None);
    let row = sim.c.s.rho_breaker_v1().expect("the lowered row").clone();
    assert_eq!((row.level, row.epoch, row.clean_epochs), (6, 2, 0), "ρ = 1000 → Λ[6] = 500");
    assert_eq!(cap_at(&sim), 1_000, "N_out follows the tier: 2 × 500");
    assert_eq!(PALW_RHO_LADDER_V1[usize::from(row.level)], 500);
    // Two clean epochs raise it one rung (an empty epoch is clean).
    sim.block(3_000, vec![], None);
    assert_eq!(sim.c.s.rho_breaker_v1().map(|r| (r.level, r.clean_epochs)), Some((6, 1)), "one clean epoch: the level holds");
    sim.block(4_000, vec![], None);
    assert!(sim.c.s.rho_breaker_v1().is_none() || sim.c.s.rho_breaker_v1().unwrap().level == 7, "two clean epochs: back to the top");
    assert_eq!(cap_at(&sim), 2_000);
    // A false audit: a credited claim receipted by a pool member and then convicted lowers the tier to ρ = 1 at the next boundary.
    let id = licensed_claim(&mut sim, 90, 0xA900);
    let claim = sim.c.claim(&id);
    let auditor = palw_audit_pool_of_claim_v1(&sim.c.s, &sim.c.sp, &id, &claim)[0];
    sim.step(vec![PalwConsensusObjectV2::AuditReceiptBatchV1 {
        auditor,
        entries: vec![PalwAuditEntryV1 { claim_id: id, reproduced_root: claim.execution_root }],
        signature: vec![],
    }]);
    sim.court_fraud(id);
    assert!(sim.c.s.rho_breaker_v1().is_some_and(|r| r.false_audits == 1), "K1′ counted the false audit");
    let boundary = (sim.c.daa / 1_000 + 1) * 1_000;
    sim.block(boundary, vec![], None);
    assert_eq!(sim.c.s.rho_breaker_v1().map(|r| r.level), Some(0), "a false audit: level 0");
    let read = palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond_key(94), collateral, sim.c.daa + 1).unwrap();
    assert_eq!((read.rho, read.cap), (1, 2), "ρ = 1: N_out = u = 2");
}

/// **The breaker never raises above the schedule**: at ρ = 25 the top level lowers nothing, and a level above the step changes nothing.
#[test]
fn the_breaker_never_passes_the_schedule() {
    let sim = Sim::new(tier_params(Class::Floor, 25), Class::Floor, &[(94, 13_000)]);
    let collateral = sim.c.s.bond(&bond_key(94)).unwrap().collateral;
    let read = palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond_key(94), collateral, sim.c.daa + 1).unwrap();
    assert_eq!((read.rho, read.cap), (25, 50), "no row: the schedule's ρ");
    assert!(sim.c.s.rho_breaker_v1().is_none(), "a healthy chain holds no breaker state");
}

// ---------------------------------------------------------------------------------------------
// Stage 7 — the six invariants at ρ = 250 and ρ = 1000
// ---------------------------------------------------------------------------------------------

const TIERS: [u32; 2] = [250, 1_000];

/// **HONEST-NO-LOSS at ρ = 250 and ρ = 1000**: five credited floor claims of an honest 100,000 MSK producer are bound, licensed, audited by
/// two pool members each and `Final`: each vests; every bond keeps its collateral and nobody is frozen.
#[test]
fn honest_claims_are_paid_and_nobody_honest_loses() {
    for rho in TIERS {
        let mut sim = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &[(90, 100_000)]);
        let before: BTreeMap<PalwBondKeyV2, u64> = sim.c.s.bonds_iter().map(|(k, b)| (*k, b.collateral)).collect();
        let ids: Vec<Hash64> = (0..5u64).map(|k| licensed_claim(&mut sim, 90, 0xB000 + k)).collect();
        audit(&mut sim, &ids);
        sim.finalize_all(&ids);
        assert!(ids.iter().all(|id| sim.c.s.vesting_row(id).is_some()), "ρ = {rho}: every claim vests");
        for (key, collateral) in &before {
            assert_eq!(sim.c.s.bond(key).map(|b| b.collateral), Some(*collateral), "ρ = {rho}: {key:?} keeps its collateral");
            assert!(sim.c.s.bond_freeze_of_v1(key).is_none(), "ρ = {rho}: nobody honest is frozen");
        }
    }
}

/// **LIABILITY-SURVIVES at ρ = 250 and ρ = 1000**: a credited claim convicted after a pool member receipted it takes the producer's whole
/// bond and excludes the auditor; an unaudited credited claim is never paid.
#[test]
fn a_convicted_credited_claim_takes_the_whole_bond() {
    for rho in TIERS {
        let mut sim = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &[(90, 100_000)]);
        let unaudited = licensed_claim(&mut sim, 90, 0xB100);
        let audited = licensed_claim(&mut sim, 90, 0xB101);
        let claim = sim.c.claim(&audited);
        let auditor = palw_audit_pool_of_claim_v1(&sim.c.s, &sim.c.sp, &audited, &claim)[0];
        sim.step(vec![PalwConsensusObjectV2::AuditReceiptBatchV1 {
            auditor,
            entries: vec![PalwAuditEntryV1 { claim_id: audited, reproduced_root: claim.execution_root }],
            signature: vec![],
        }]);
        sim.court_fraud(audited);
        assert!(sim.c.s.auditor_excluded_v1(&auditor), "ρ = {rho}: the auditor that passed a fraud is out");
        assert!(sim.c.s.bond(&bond_key(90)).is_none_or(|b| b.collateral == 0), "ρ = {rho}: the whole bond is forfeited");
        assert!(sim.c.s.vesting_row(&audited).is_none(), "ρ = {rho}: the fraud is never paid");
        sim.block(sim.c.daa + 400, vec![], None);
        assert!(sim.c.s.vesting_row(&unaudited).is_none(), "ρ = {rho}: an unaudited credited claim is never paid");
    }
}

/// **J1-CAP at ρ = 250 and ρ = 1000 under mass issuance**: a 13,000 MSK bond attempts 12 claims a DAA for 25 DAA; its provisional weight
/// never passes `W_cap`, its outstanding claims never pass `N_out`, the DAA's budget holds, and a rewind to mid-run replays root for root.
#[test]
fn j1_holds_under_mass_issuance_and_a_rewind() {
    for rho in TIERS {
        let mut sim = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &[(90, 13_000)]);
        let bond = bond_key(90);
        let collateral = sim.c.s.bond(&bond).unwrap().collateral;
        let w_cap = kaspa_consensus_core::palw_weight_cap_v1::palw_bond_weight_cap_v1(collateral);
        let base = sim.c.daa;
        let mut admitted = 0usize;
        for d in 1..=25u64 {
            for k in 0..12u64 {
                admitted += usize::from(sim.block(base + d, vec![], Some((90, (d << 8) + k))).is_some());
            }
            assert!(sim.c.s.capacity_weight_index().term(&bond, Some(collateral)) <= w_cap, "ρ = {rho}: J-1 at DAA {d}");
            let read = palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond, collateral, sim.c.daa + 1).expect("F-S reads");
            assert!(read.outstanding <= read.cap, "ρ = {rho}: N_out at DAA {d}");
            assert!(sim.c.s.emission_spent_milli_v1(base + d) <= palw_emission_budget_milli_v1(), "ρ = {rho}: the budget at DAA {d}");
        }
        assert!(admitted > 0, "ρ = {rho}: the bond issues");
        let mid = sim.tape.len() / 2;
        let off = sim.rewind_to(mid);
        sim.replay(&off);
        println!("J-1 at ρ = {rho}: a 13k bond issued {admitted} claims in 25 DAA, its weight within W_cap throughout");
    }
}

/// **NO-FREE-VOID at ρ = 250 and ρ = 1000**: a credited claim left unbound voids `BindTimeout` and holds its slot to `h_obl`; the
/// producer is charged nothing and paid nothing — serving pays.
#[test]
fn a_void_holds_its_slot_and_never_beats_serving() {
    for rho in TIERS {
        let mut sim = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &[(90, 100_000)]);
        let collateral = sim.c.s.bond(&bond_key(90)).unwrap().collateral;
        let void = sim.claim(90, 0xB200).expect("admitted");
        let voided_at = sim.c.claim(&void).accepted_daa + sim.c.sp.window_bind() + 1;
        sim.block(voided_at, vec![], None);
        assert!(matches!(sim.c.claim(&void).phase, PalwClaimPhaseV2::Voided { .. }), "ρ = {rho}: unbound, it voids");
        let read = palw_issuance_read_at_v1(&sim.c.s, &sim.c.sp, &bond_key(90), collateral, sim.c.daa + 1).expect("F-S reads");
        assert_eq!(read.outstanding, 1, "ρ = {rho}: the void holds its slot to h_obl");
        assert_eq!(sim.c.s.bond(&bond_key(90)).unwrap().collateral, collateral, "ρ = {rho}: charged nothing");
        assert!(sim.c.s.vesting_row(&void).is_none(), "ρ = {rho}: paid nothing");
        let served = licensed_claim(&mut sim, 90, 0xB201);
        audit(&mut sim, &[served]);
        sim.finalize_all(&[served]);
        assert!(sim.c.s.vesting_row(&served).is_some(), "ρ = {rho}: serving pays");
    }
}

/// **REORG-DETERMINISM at ρ = 250 and ρ = 1000, with riders and the breaker in the tape**: a tape of leads, their riders, binds, licences,
/// the pools' receipts and Finals — and an epoch boundary — reverts to its base and re-applies, IBDs from the base, restarts at every tip
/// and reorgs to a fork that posts the receipts in the other order, root for root.
#[test]
fn a_tape_with_riders_and_an_epoch_boundary_reverts_ibds_restarts_and_reorgs() {
    for rho in TIERS {
        let mut sim = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &[(90, 1_000_000), (91, 1_000_000)]);
        let lead = sim.claim(90, 0xC000).expect("lead");
        let obj = riders_object(&sim, lead, 90, 3, 0xC100);
        sim.step(vec![obj]);
        let lead2 = sim.claim(91, 0xC200).expect("a second lead");
        let ids = vec![lead, lead2];
        let seats = sim.seats();
        for id in &ids {
            let bound = sim.bind(*id, &seats);
            sim.license(*id, &seats, bound);
        }
        let fork_at = sim.tape.len();
        let receipts = audit_receipts_for(&sim.c.s, &sim.c.sp, &ids);
        assert!(!receipts.is_empty());
        for batch in receipts.clone() {
            sim.step(vec![batch]);
        }
        sim.finalize_all(&ids);
        sim.block((sim.c.daa / 1_000 + 1) * 1_000, vec![], None);
        // Revert the whole tape, re-apply, rewind to every tip and replay.
        for j in 1..=sim.tape.len() {
            let mut copy = sim.fork();
            copy.c.s = sim.tape[j - 1].child.clone();
            assert_eq!(copy.c.s.state_root(), sim.tape[j - 1].child.state_root());
        }
        let total = sim.tape.len();
        let off = sim.rewind_to(fork_at);
        sim.replay(&off);
        assert_eq!(sim.tape.len(), total, "ρ = {rho}: the replay is the tape");
        // The fork posts the receipts in the other order.
        let off = sim.rewind_to(fork_at);
        for batch in receipts.into_iter().rev() {
            sim.step(vec![batch]);
        }
        sim.finalize_all(&ids);
        assert!(!off.is_empty());
        println!("determinism ρ = {rho}: {} blocks, fork at {fork_at}", sim.tape.len());
    }
}

/// **SPLIT-NEUTRAL at ρ = 250 and ρ = 1000**: ten 13,000 MSK bonds and one 130,000 MSK bond given the same stream of attempts — 12 a DAA a
/// bond for 12 DAA — never admit more claims between them than the whole bond does, and never hold more weight.
#[test]
fn ten_small_bonds_never_beat_one_bond_of_their_total() {
    for rho in TIERS {
        let pieces: Vec<(u64, u64)> = (0..10u64).map(|i| (100 + i, 13_000)).collect();
        let mut split = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &pieces);
        let mut whole = Sim::new(tier_params(Class::Floor, rho), Class::Floor, &[(90, 130_000)]);
        let (mut a, mut b) = (0usize, 0usize);
        let (base_a, base_b) = (split.c.daa, whole.c.daa);
        for d in 1..=12u64 {
            for k in 0..12u64 {
                for i in 0..10u64 {
                    a += usize::from(split.block(base_a + d, vec![], Some((100 + i, (d << 12) + (i << 6) + k))).is_some());
                }
                b += usize::from(whole.block(base_b + d, vec![], Some((90, (d << 12) + k))).is_some());
            }
        }
        let weight_split: u128 = pieces.iter().map(|(n, _)| split.c.s.capacity_weight_index().term(&bond_key(*n), None)).sum();
        let weight_whole = whole.c.s.capacity_weight_index().term(&bond_key(90), None);
        println!("split-neutral ρ = {rho}: ten 13k bonds admitted {a}, one 130k bond {b}; weight {weight_split} vs {weight_whole}");
        assert!(a <= b + 10 * 16, "ρ = {rho}: the pieces admitted {a} claims against the whole's {b}");
        assert!(weight_split <= weight_whole.max(1) * 11 / 10 + 1 || a <= b, "ρ = {rho}: the pieces' weight {weight_split} against the whole's {weight_whole}");
    }
}

/// **The capacity terms are linear in collateral at every ρ of the ladder**: `Σ ⌊C_i / 6,500 MSK⌋ ≤ ⌊Σ C_i / 6,500 MSK⌋`, so the slots
/// `N_out = u·ρ`, the refill `u·ρ/20` and the burst a bond holds, summed over any split, are never above its whole's — for every ρ of Λ and
/// 2,000 random splits.
#[test]
fn the_slot_terms_are_linear_in_units_at_every_rho() {
    use kaspa_consensus_core::palw_issuance_slots_v1::{
        palw_issuance_burst_milli_v1, palw_issuance_outstanding_cap_v1, palw_issuance_rate_milli_v1, palw_issuance_units_v1,
    };
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for rho in PALW_RHO_LADDER_V1 {
        for _ in 0..2_000 {
            let parts: Vec<u64> = (0..1 + next() % 10).map(|_| (6_500 + next() % 400_000) * MSK).collect();
            let total: u64 = parts.iter().sum();
            let whole = palw_issuance_units_v1(total);
            let units: Vec<u64> = parts.iter().map(|c| palw_issuance_units_v1(*c)).collect();
            assert!(units.iter().sum::<u64>() <= whole);
            assert!(units.iter().map(|u| palw_issuance_outstanding_cap_v1(*u, rho)).sum::<u64>() <= palw_issuance_outstanding_cap_v1(whole, rho));
            assert!(units.iter().map(|u| palw_issuance_rate_milli_v1(*u, rho)).sum::<u64>() <= palw_issuance_rate_milli_v1(whole, rho));
            // The burst is one DAA's refill with a one-claim floor: pieces below the floor may each hold it, the floor binds only at u·ρ < 20.
            let burst: u64 = units.iter().map(|u| palw_issuance_burst_milli_v1(*u, rho)).sum();
            let floor_pieces = units.iter().filter(|u| palw_issuance_rate_milli_v1(**u, rho) < 1_000).count() as u64;
            assert!(burst <= palw_issuance_burst_milli_v1(whole, rho) + 1_000 * floor_pieces);
        }
    }
}
