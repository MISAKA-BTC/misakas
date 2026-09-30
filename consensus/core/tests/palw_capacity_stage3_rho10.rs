//! **ADR-0160 stage 3 — the ρ = 10 capacity flag day as flag-day entries, the ready ρ 25 / ρ 100
//! variants, F1 and the floor-only credit, through testnet-12's own fold** (rcore/cap-s1; the user's
//! staged plan, stage 3, and the decision of 2026-09-27: the ρ = 10 architecture of stages 1, 2 and 4
//! rides the DAA-1,700 flag day, ρ 25 / 100 are later flag days).
//!
//! * **the entries** — [`PALW_T12_CAPACITY_RHO10_FENCES_V1`] is the capacity list with F-L at a FIXED
//!   ρ = 10, credited; each entry is a named `PalwPostLaunchFenceV1` a flag-day list takes in one line.
//!   Armed at DAA 1,700 over the DAA-1,300 release they validate together (and a list leaving out an
//!   entry another needs is refused), move the params and schedule ids but not the identity, gate the
//!   fork id from 1,700, and `set(None)` gives the release back to the id; testnet-12 as shipped carries
//!   none of them;
//! * **the ready variants** — ρ 25 then ρ 100 (or ρ 100 straight after ρ 10) are F-L's appended steps,
//!   each its own flag day's value and fork-id slot; a step armed before the flag days it builds on panics;
//! * **the crossing** — below DAA 1,700 a claim is priced as today; from it the floor's claim is credited
//!   (`⌈E/10⌉`, its `Final` behind its audit) and paid through it;
//! * **F1** — at every ρ, `G` reads the claim's full weight (`reserved × ρ`), so a conviction's `G` and an
//!   uncredited claim's seat lock are today's; **D-18** — only the floor is credited (8k's escrow is `E`);
//! * **the invariants at ρ 10, 25 and 100**: honest credited claims are paid through their audits and
//!   nobody honest loses; a convicted credited claim takes the whole bond and excludes its auditor; J-1
//!   holds under mass issuance and after a rewind; a void keeps its commitment and pays less than
//!   serving; a credited tape reverts, IBDs, restarts and reorgs root for root.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_capacity_stage3_rho10 -- --nocapture`

#[path = "capacity_stage1_common.rs"]
mod stage1;
use stage1::*;

use kaspa_consensus_core::config::params::{
    PALW_T12_CAPACITY_AGGREGATE_LIABILITY_RHO10_V1, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1,
    PALW_T12_CAPACITY_RHO25_STEP_2_V1, PALW_T12_CAPACITY_RHO100_STEP_2_V1, PALW_T12_CAPACITY_RHO100_STEP_3_V1,
    PALW_T12_POST_LAUNCH_FENCE_V3_DAA, PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2,
    PALW_T12_POST_LAUNCH_FENCES_V3, PalwPostLaunchFenceV1, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_aggregate_liability_v1::PalwCapacityLiabilityV1;
use kaspa_consensus_core::palw_audit_door_v1::{PalwAuditEntryV1, palw_audit_pool_of_claim_v1, palw_capacity_claim_credited_v1};
use kaspa_consensus_core::palw_escrow_funding_v2::palw_escrow_term_v2;
use kaspa_consensus_core::palw_state_v2::palw_claim_g_v1;
use kaspa_consensus_core::palw_weight_cap_v1::PalwCapacityGainScaleV1;

/// The capacity flag day's height (the user's decision of 2026-09-27, with the FP Job V4 pipeline).
const FLAG_DAY: u64 = 1_700;
/// Two later flag days' heights for the ready variants (any heights above the ρ = 10 flag day's).
const X25_AT: u64 = 2_222;
const X100_AT: u64 = 2_888;

/// The ready step entries.
const STEPS: [&PalwPostLaunchFenceV1; 3] =
    [&PALW_T12_CAPACITY_RHO25_STEP_2_V1, &PALW_T12_CAPACITY_RHO100_STEP_3_V1, &PALW_T12_CAPACITY_RHO100_STEP_2_V1];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn steps(value: &PalwCapacityLiabilityV1) -> Vec<(u64, u32, u16)> {
    value.steps.iter().map(|s| (s.from_daa, s.rho, s.q_credit_permille)).collect()
}

fn fence_at(p: &Params, name: &str) -> Option<u64> {
    p.palw_fences_v1().iter().find(|(n, _)| *n == name).and_then(|(_, f)| *f).map(|f| f.daa_score())
}

/// **The release this build ships, with every capacity entry set back to dormant** — testnet-12 as
/// shipped here (the DAA-1,300 release: this build arms none of them), and the baseline a flag-day
/// build that lists them is judged against.
fn release() -> Params {
    let mut p = palw_t12_shipped_params();
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().chain(STEPS) {
        (f.set)(&mut p, None);
    }
    p.validate_palw_v2().expect("the release validates");
    p
}

/// `p` with every entry of the ρ = 10 list at `at`, validated.
fn arm_rho10(mut p: Params, at: u64) -> Params {
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1 {
        (f.set)(&mut p, Some(ForkActivation::new(at)));
    }
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the ρ = 10 list at {at}: {e:?}"));
    p
}

/// `p` with `entry` at `at`, validated.
fn with(mut p: Params, entry: &PalwPostLaunchFenceV1, at: u64) -> Params {
    (entry.set)(&mut p, Some(ForkActivation::new(at)));
    p.validate_palw_v2().unwrap_or_else(|e| panic!("{} at {at}: {e:?}", entry.name));
    p
}

/// **The tiers the invariants run at** on the stage-1 fixture (the chain's first claim is accepted at
/// DAA 1,002): the ρ = 10 list at [`H`], alone or with a ready variant's step at 1,002.
#[derive(Clone, Copy, Debug)]
enum Tier {
    X10,
    X25,
    X100,
}

impl Tier {
    const ALL: [Tier; 3] = [Tier::X10, Tier::X25, Tier::X100];

    fn rho(self) -> u32 {
        match self {
            Tier::X10 => 10,
            Tier::X25 => 25,
            Tier::X100 => 100,
        }
    }

    fn params(self, class: Class) -> Params {
        let p = arm_rho10(params_for(class, false), H);
        match self {
            Tier::X10 => p,
            Tier::X25 => with(p, &PALW_T12_CAPACITY_RHO25_STEP_2_V1, H + 1),
            Tier::X100 => with(p, &PALW_T12_CAPACITY_RHO100_STEP_2_V1, H + 1),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The entries
// ---------------------------------------------------------------------------------------------

/// **The ρ = 10 list is the capacity list with F-L at a fixed ρ = 10, credited**: the same eight fences
/// in the same order, every entry but F-L setting exactly what the proof list's does, F-L's value one
/// step `(at, 10, q_seat = 250‰)`; every entry and ready step is a fork-id fence name.
#[test]
fn the_rho10_list_is_the_capacity_list_with_f_l_at_a_fixed_rho10() {
    let names = |list: &[PalwPostLaunchFenceV1]| list.iter().map(|f| f.name).collect::<Vec<_>>();
    assert_eq!(names(PALW_T12_CAPACITY_RHO10_FENCES_V1), names(PALW_T12_CAPACITY_FENCES_V1), "the same eight fences, in order");
    let fences = release().palw_fences_v1();
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().chain(STEPS) {
        assert!(fences.iter().any(|(n, _)| *n == f.name), "{}: a fork-id fence name", f.name);
    }
    for (ten, one) in PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().zip(PALW_T12_CAPACITY_FENCES_V1) {
        let (mut x, mut y) = (release(), release());
        (ten.set)(&mut x, Some(ForkActivation::new(FLAG_DAY)));
        (one.set)(&mut y, Some(ForkActivation::new(FLAG_DAY)));
        if ten.name == "palw_capacity_aggregate_liability" {
            assert_eq!(steps(x.palw_capacity_aggregate_liability.as_ref().unwrap()), vec![(FLAG_DAY, 10, 250)], "ρ = 10, credited");
            assert_eq!(steps(y.palw_capacity_aggregate_liability.as_ref().unwrap()), vec![(FLAG_DAY, 1, 0)], "the proof list's ρ = 1");
        } else {
            assert_eq!(ids(&x), ids(&y), "{}: the same set as the proof list's", ten.name);
        }
    }
}

/// **This build ships the package dormant**: testnet-12 as shipped is the release (every capacity entry
/// dormant — the flag day's build flips exactly this assertion), and no entry is on a shipped list.
#[test]
fn testnet12_ships_the_package_armed_at_1700_over_the_daa1300_release() {
    let shipped = palw_t12_shipped_params();
    // The third post-launch flag day (2026-09-27): testnet-12 as shipped IS the package armed at 1,700 over
    // the DAA-1,300 release, entry by entry, and nothing else differs.
    assert_eq!(PALW_T12_POST_LAUNCH_FENCE_V3_DAA, Some(1_700), "the flag day's height");
    assert_eq!(ids(&shipped), ids(&arm_rho10(release(), FLAG_DAY)), "testnet-12 as shipped arms the package at 1,700");
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1 {
        assert_eq!(fence_at(&shipped, f.name), Some(1_700), "{}: armed at 1,700", f.name);
        assert!(PALW_T12_POST_LAUNCH_FENCES_V3.iter().any(|g| g.name == f.name), "{}: on the third list", f.name);
        assert!(
            !PALW_T12_POST_LAUNCH_FENCES_V1.iter().chain(PALW_T12_POST_LAUNCH_FENCES_V2).any(|g| g.name == f.name),
            "{}: on no earlier list",
            f.name
        );
    }
    for f in STEPS {
        assert_eq!(fence_at(&shipped, f.name), None, "{}: the later rho steps stay dormant", f.name);
    }
    shipped.validate_palw_v2().expect("testnet-12 as shipped validates");
}

/// **The flag day at DAA 1,700**: the eight entries over the DAA-1,300 release validate together; the
/// params and schedule ids move and the identity does not; every entry's fence is 1,700 on the fork-id
/// list, and a node without them is refused from 1,700; ρ is 10 from 1,700 and absent below; `set(None)`
/// and `set(Some(never()))` give the release back to the id. A list that leaves out an entry another one
/// needs is refused — every entry but F-N (which nothing else reads) is load-bearing for the validation.
#[test]
fn the_rho10_flag_day_arms_at_1700_moves_the_ids_and_gates_the_fork_id() {
    let before = release();
    let armed = arm_rho10(before.clone(), FLAG_DAY);
    let (bp, bi, bs) = ids(&before);
    let (ap, ai, as_) = ids(&armed);
    assert_ne!(ap, bp, "the ruleset names the package");
    assert_ne!(as_, bs, "the schedule names it");
    assert_eq!(ai, bi, "the identity does not move");
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1 {
        assert_eq!(fence_at(&armed, f.name), Some(FLAG_DAY), "{} at 1,700", f.name);
    }
    let s = fork_id_v1(&before, FLAG_DAY);
    assert!(evaluate_fork_id_v1(&armed, FLAG_DAY, s.fired.as_bytes().as_slice(), s.next).refuses(), "gated from 1,700");
    assert_eq!(armed.palw_capacity_step_at_v1(FLAG_DAY - 1), None, "below the height: no ρ");
    assert_eq!(armed.palw_capacity_step_at_v1(FLAG_DAY).map(|s| (s.rho, s.q_credit_permille)), Some((10, 250)));
    assert_eq!(armed.palw_capacity_step_at_v1(1_000_000).map(|s| s.rho), Some(10), "fixed: never dynamic");
    for undo in [None, Some(ForkActivation::never())] {
        let mut back = armed.clone();
        for f in PALW_T12_CAPACITY_RHO10_FENCES_V1 {
            (f.set)(&mut back, undo);
        }
        back.validate_palw_v2().expect("dormant again");
        assert_eq!(ids(&back).1, bi, "{undo:?}: the identity");
        if undo.is_none() {
            assert_eq!(ids(&back), ids(&before), "set(None): the release, to the id");
        }
    }
    for left_out in PALW_T12_CAPACITY_RHO10_FENCES_V1 {
        let mut partial = before.clone();
        for f in PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().filter(|f| f.name != left_out.name) {
            (f.set)(&mut partial, Some(ForkActivation::new(FLAG_DAY)));
        }
        let verdict = partial.validate_palw_v2();
        if left_out.name == "palw_capacity_network_room" {
            verdict.expect("F-N is the one entry nothing else needs");
        } else {
            assert!(verdict.is_err(), "a list without {} is refused", left_out.name);
        }
    }
}

/// **The ready variants are their own flag days**: ρ 25 then ρ 100 (or ρ 100 straight after ρ 10) are
/// F-L's appended steps — four rulesets, four params ids, one identity; each step its own
/// fork-id slot, refused from its height by a node without it; `set(None)` drops a step (and every later
/// one) back to the earlier flag day's ruleset, to the id; moving the ρ = 10 entry keeps the appended
/// steps; a step armed before the flag days it builds on panics, and one at or below the previous
/// step's height is refused.
#[test]
fn the_ready_variants_are_their_own_flag_days() {
    let rho10 = arm_rho10(release(), FLAG_DAY);
    let rho25 = with(rho10.clone(), &PALW_T12_CAPACITY_RHO25_STEP_2_V1, X25_AT);
    let rho100 = with(rho25.clone(), &PALW_T12_CAPACITY_RHO100_STEP_3_V1, X100_AT);
    let direct = with(rho10.clone(), &PALW_T12_CAPACITY_RHO100_STEP_2_V1, X25_AT);
    let value = |p: &Params| steps(p.palw_capacity_aggregate_liability.as_ref().unwrap());
    assert_eq!(value(&rho100), vec![(FLAG_DAY, 10, 250), (X25_AT, 25, 250), (X100_AT, 100, 250)]);
    assert_eq!(value(&direct), vec![(FLAG_DAY, 10, 250), (X25_AT, 100, 250)]);
    let all = [ids(&rho10), ids(&rho25), ids(&rho100), ids(&direct)];
    for (i, a) in all.iter().enumerate() {
        assert_eq!(a.1, all[0].1, "the identity never moves");
        for b in &all[i + 1..] {
            assert_ne!(a.0, b.0, "each its own params id");
        }
    }
    // The schedule id names heights: each appended step moves it; the two variants that append their
    // step at one height share it and are told apart by the params id (which carries F-L's value whole).
    assert_ne!(all[0].2, all[1].2);
    assert_ne!(all[1].2, all[2].2);
    assert_eq!(all[1].2, all[3].2, "ρ 25 and ρ 100 at one height: one schedule, two rulesets");
    assert_eq!(fence_at(&rho100, "palw_capacity_aggregate_liability_step_2"), Some(X25_AT));
    assert_eq!(fence_at(&rho100, "palw_capacity_aggregate_liability_step_3"), Some(X100_AT));
    for (without, with_step, at) in [(&rho10, &rho25, X25_AT), (&rho25, &rho100, X100_AT), (&rho10, &direct, X25_AT)] {
        let s = fork_id_v1(without, at);
        assert!(
            evaluate_fork_id_v1(with_step, at, s.fired.as_bytes().as_slice(), s.next).refuses(),
            "the step at {at} is gated from it"
        );
    }
    for (p, at, want) in
        [(&rho100, FLAG_DAY, 10u32), (&rho100, X25_AT - 1, 10), (&rho100, X25_AT, 25), (&rho100, X100_AT, 100), (&direct, X25_AT, 100)]
    {
        assert_eq!(p.palw_capacity_step_at_v1(at).map(|s| s.rho), Some(want), "ρ at {at}");
    }
    let mut back = rho100.clone();
    (PALW_T12_CAPACITY_RHO100_STEP_3_V1.set)(&mut back, None);
    assert_eq!(ids(&back), ids(&rho25), "the third step undone: the ρ 25 ruleset");
    (PALW_T12_CAPACITY_RHO25_STEP_2_V1.set)(&mut back, None);
    assert_eq!(ids(&back), ids(&rho10), "the second undone: the ρ 10 ruleset");
    let mut dropped = rho100.clone();
    (PALW_T12_CAPACITY_RHO25_STEP_2_V1.set)(&mut dropped, None);
    assert_eq!(ids(&dropped), ids(&rho10), "dropping a step drops every later one");
    let moved = arm_rho10(rho100.clone(), FLAG_DAY + 100);
    assert_eq!(value(&moved), vec![(FLAG_DAY + 100, 10, 250), (X25_AT, 25, 250), (X100_AT, 100, 250)], "a move keeps the later steps");
    let mut over = rho100.clone();
    (PALW_T12_CAPACITY_AGGREGATE_LIABILITY_RHO10_V1.set)(&mut over, Some(ForkActivation::new(X25_AT)));
    assert!(over.validate_palw_v2().is_err(), "a move onto a later step is refused");
    let panics = |mut p: Params, entry: &'static PalwPostLaunchFenceV1, at: u64| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || (entry.set)(&mut p, Some(ForkActivation::new(at))))).is_err()
    };
    assert!(panics(release(), &PALW_T12_CAPACITY_RHO25_STEP_2_V1, X25_AT), "a step on a ruleset without F-L");
    assert!(panics(rho10.clone(), &PALW_T12_CAPACITY_RHO100_STEP_3_V1, X100_AT), "the third step without the second");
    let mut low = rho10.clone();
    (PALW_T12_CAPACITY_RHO25_STEP_2_V1.set)(&mut low, Some(ForkActivation::new(FLAG_DAY)));
    assert!(low.validate_palw_v2().is_err(), "a step at the fence's own height is refused");
}

// ---------------------------------------------------------------------------------------------
// Through the fold
// ---------------------------------------------------------------------------------------------

/// A claim of bond 90 bound and licensed on its class's honest panel; its id.
fn licensed_claim(sim: &mut Sim, seed: u64) -> Hash64 {
    let id = sim.claim(90, seed).expect("admitted");
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

/// **The flag day crosses in the fold at DAA 1,700**, beside a twin on the release fed the same blocks: a
/// floor claim accepted below it is the twin's to the sompi and reaches `Final` without an audit; a
/// claim accepted AT 1,700 is credited — its reservation `⌈w/10⌉`, its escrow slot `⌈E/10⌉`, its `G`
/// the twin's — waits for its audit and is paid through it, and the honest producer loses nothing.
#[test]
fn the_flag_day_crosses_at_1700_in_the_fold() {
    let mut sim = Sim::new(arm_rho10(params_for(Class::Floor, false), FLAG_DAY), Class::Floor, &[(90, 100_000)]);
    let mut twin = Sim::new(params_for(Class::Floor, false), Class::Floor, &[(90, 100_000)]);
    let mut pair = Vec::new();
    for sim in [&mut sim, &mut twin] {
        let below = licensed_claim(sim, 0x3E00);
        sim.finalize_all(&[below]);
        sim.block(FLAG_DAY - 1, vec![], None);
        let at = licensed_claim(sim, 0x3E01);
        pair.push((below, at));
    }
    let ((below, at), (twin_below, twin_at)) = (pair[0], pair[1]);
    let (b, tb) = (sim.c.claim(&below), twin.c.claim(&twin_below));
    assert_eq!(
        (b.reserved, b.escrowed_reward, b.accepted_daa),
        (tb.reserved, tb.escrowed_reward, tb.accepted_daa),
        "below 1,700: the release's price"
    );
    assert!(!palw_capacity_claim_credited_v1(&sim.c.sp, &b), "below 1,700: nothing credited");
    assert!(sim.c.s.vesting_row(&below).is_some(), "Final without an audit, as today");
    let (a, ta) = (sim.c.claim(&at), twin.c.claim(&twin_at));
    assert_eq!(a.accepted_daa, FLAG_DAY, "accepted at the flag day");
    assert!(palw_capacity_claim_credited_v1(&sim.c.sp, &a), "the floor's claim is credited from 1,700");
    assert_eq!(a.reserved, ta.reserved.div_ceil(10), "its reservation ⌈w/10⌉");
    let e = u128::from(a.escrowed_reward);
    assert_eq!(a.escrowed_reward, ta.escrowed_reward, "the same reward");
    assert_eq!(
        palw_escrow_term_v2(&sim.c.sp, a.accepted_daa, a.escrowed_reward, &a.class_id),
        e.div_ceil(10).min(sim.c.sp.claim_escrow_reservation_v1(a.accepted_daa, a.escrowed_reward)),
        "its slot is ⌈E/10⌉"
    );
    let g = palw_claim_g_v1(&sim.c.s, &PalwCapacityGainScaleV1::of(&sim.c.sp), &at).unwrap().g();
    let g_twin = palw_claim_g_v1(&twin.c.s, &PalwCapacityGainScaleV1::of(&twin.c.sp), &twin_at).unwrap().g();
    assert!(g >= g_twin && g < g_twin + 10, "F1: G is the twin's, never below it ({g} vs {g_twin})");
    assert_eq!(sim.c.s.deadline_of(&at), None, "it waits for its audit");
    let collateral = sim.c.s.bond(&bond_key(90)).unwrap().collateral;
    audit(&mut sim, &[at]);
    sim.finalize_all(&[at]);
    assert!(sim.c.s.vesting_row(&at).is_some(), "paid through its audit");
    assert_eq!(sim.c.s.bond(&bond_key(90)).unwrap().collateral, collateral, "the honest producer keeps its collateral");
    println!(
        "crossing: below 1,700 the release's price (w {}), at 1,700 credited (w {} of {}), Final through its audit",
        tb.reserved, a.reserved, ta.reserved
    );
}

/// **F1 and D-18 at every tier.** The same attempt on the stage-1 ruleset (ρ = 1) and at each tier: the
/// floor claim (credited) and the 8k claim (uncredited: the credit is the floor's) reserve `⌈w/ρ⌉`, and
/// their `G` — the conviction's, the seat lock's — reads `reserved × ρ`, today's `w` up to the ceiling's
/// rounding (< ρ sompi); the floor's escrow slot is `⌈E/ρ⌉`, the 8k claim's is today's.
#[test]
fn f1_the_gain_is_todays_and_d18_credits_the_floor_only_at_every_tier() {
    for class in [Class::Floor, Class::K8] {
        let mut today = Sim::new(params_for(class, true), class, &[(90, 1_000_000)]);
        let id_today = today.claim(90, 0xF100).expect("admitted at ρ = 1");
        let claim_today = today.c.claim(&id_today);
        let g_today = palw_claim_g_v1(&today.c.s, &PalwCapacityGainScaleV1::of(&today.c.sp), &id_today).unwrap().g();
        let slot_today =
            palw_escrow_term_v2(&today.c.sp, claim_today.accepted_daa, claim_today.escrowed_reward, &claim_today.class_id);
        for tier in Tier::ALL {
            let rho = u128::from(tier.rho());
            let mut sim = Sim::new(tier.params(class), class, &[(90, 1_000_000)]);
            let id = sim.claim(90, 0xF100).expect("admitted at the tier");
            let claim = sim.c.claim(&id);
            let scale = PalwCapacityGainScaleV1::of(&sim.c.sp);
            assert_eq!(u128::from(scale.scale_of(&claim)), rho, "{} ×{rho}: F-W divided its reservation by ρ", class.label());
            assert_eq!(claim.reserved, claim_today.reserved.div_ceil(rho), "{} ×{rho}: the reservation is ⌈w/ρ⌉", class.label());
            assert_eq!(claim.escrowed_reward, claim_today.escrowed_reward, "{} ×{rho}: the same reward", class.label());
            let g = palw_claim_g_v1(&sim.c.s, &scale, &id).unwrap().g();
            assert!(g >= g_today && g < g_today + rho, "{} ×{rho}: G {g} is today's {g_today}", class.label());
            let slot = palw_escrow_term_v2(&sim.c.sp, claim.accepted_daa, claim.escrowed_reward, &claim.class_id);
            let credited = palw_capacity_claim_credited_v1(&sim.c.sp, &claim);
            assert_eq!(credited, class == Class::Floor, "{} ×{rho}: the credit is the floor's (D-18)", class.label());
            if class == Class::Floor {
                let option_a = sim.c.sp.claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward);
                assert_eq!(slot, u128::from(claim.escrowed_reward).div_ceil(rho).min(option_a), "floor ×{rho}: the slot is ⌈E/ρ⌉");
                assert!(slot < slot_today, "floor ×{rho}: below today's {slot_today}");
            } else {
                assert_eq!(slot, slot_today, "8k ×{rho}: the slot is today's");
            }
            println!(
                "{} ×{rho}: reserved {} (today {}), G {g} (today {g_today}), slot {slot} (today {slot_today}), credited {credited}",
                class.label(),
                claim.reserved,
                claim_today.reserved
            );
        }
    }
}

/// **HONEST-NO-LOSS at every tier**: five credited floor claims of an honest 100,000 MSK producer are
/// bound, licensed, audited by their pools and `Final`: each vests; every bond keeps its collateral and
/// nobody is frozen. At ρ = 10 an honest 8k claim (uncredited, D-18) is `Final` as today, no audit.
#[test]
fn honest_claims_are_paid_and_nobody_honest_loses_at_every_tier() {
    for tier in Tier::ALL {
        let mut sim = Sim::new(tier.params(Class::Floor), Class::Floor, &[(90, 100_000)]);
        let before: BTreeMap<PalwBondKeyV2, u64> = sim.c.s.bonds_iter().map(|(k, b)| (*k, b.collateral)).collect();
        let ids: Vec<Hash64> = (0..5u64).map(|k| licensed_claim(&mut sim, 0x3A00 + k)).collect();
        assert!(ids.iter().all(|id| palw_capacity_claim_credited_v1(&sim.c.sp, &sim.c.claim(id))), "×{}: credited", tier.rho());
        audit(&mut sim, &ids);
        sim.finalize_all(&ids);
        assert!(ids.iter().all(|id| sim.c.s.vesting_row(id).is_some()), "×{}: every claim vests", tier.rho());
        for (key, collateral) in &before {
            assert_eq!(sim.c.s.bond(key).map(|b| b.collateral), Some(*collateral), "×{}: {key:?} keeps its collateral", tier.rho());
            assert!(sim.c.s.bond_freeze_of_v1(key).is_none(), "×{}: nobody honest is frozen", tier.rho());
        }
    }
    let mut sim = Sim::new(Tier::X10.params(Class::K8), Class::K8, &[(90, 1_000_000)]);
    let id = licensed_claim(&mut sim, 0x3A80);
    assert!(!palw_capacity_claim_credited_v1(&sim.c.sp, &sim.c.claim(&id)), "8k: not credited");
    sim.finalize_all(&[id]);
    assert!(sim.c.s.vesting_row(&id).is_some(), "8k at ρ 10: Final as today, no audit");
}

/// **LIABILITY at every tier**: a credited claim convicted after a pool member receipted it takes the
/// producer's whole bond (AG-2) and excludes the auditor; a credited claim nobody audited is never paid.
#[test]
fn a_convicted_credited_claim_takes_the_whole_bond_and_its_auditor_at_every_tier() {
    for tier in Tier::ALL {
        let mut sim = Sim::new(tier.params(Class::Floor), Class::Floor, &[(90, 100_000)]);
        let unaudited = licensed_claim(&mut sim, 0x3B00);
        let audited = licensed_claim(&mut sim, 0x3B01);
        let claim = sim.c.claim(&audited);
        let auditor = palw_audit_pool_of_claim_v1(&sim.c.s, &sim.c.sp, &audited, &claim)[0];
        sim.step(vec![PalwConsensusObjectV2::AuditReceiptBatchV1 {
            auditor,
            entries: vec![PalwAuditEntryV1 { claim_id: audited, reproduced_root: claim.execution_root }],
            signature: vec![],
        }]);
        sim.court_fraud(audited);
        assert!(sim.c.s.auditor_excluded_v1(&auditor), "×{}: the auditor that passed a fraud is out", tier.rho());
        let bond = sim.c.s.bond(&bond_key(90));
        assert!(
            bond.is_none_or(|b| b.collateral == 0),
            "×{}: the whole bond is forfeited: {:?}",
            tier.rho(),
            bond.map(|b| b.collateral)
        );
        assert!(sim.c.s.vesting_row(&audited).is_none(), "×{}: the fraud is never paid", tier.rho());
        sim.block(sim.c.daa + 400, vec![], None);
        assert!(sim.c.s.vesting_row(&unaudited).is_none(), "×{}: an unaudited credited claim is never paid", tier.rho());
    }
}

/// **J1-CAP at every tier under mass issuance**: a 13,000 MSK bond attempts 12 claims a DAA for 25 DAA;
/// its provisional weight never passes `W_cap`, its outstanding claims never pass `N_out`, and a rewind to
/// mid-run replays root for root.
#[test]
fn j1_holds_under_mass_issuance_and_a_rewind_at_every_tier() {
    for tier in Tier::ALL {
        let mut sim = Sim::new(tier.params(Class::Floor), Class::Floor, &[(90, 13_000)]);
        let bond = bond_key(90);
        let collateral = sim.c.s.bond(&bond).unwrap().collateral;
        let w_cap = kaspa_consensus_core::palw_weight_cap_v1::palw_bond_weight_cap_v1(collateral);
        let base = sim.c.daa;
        let mut admitted = 0usize;
        for d in 1..=25u64 {
            for k in 0..12u64 {
                admitted += usize::from(sim.block(base + d, vec![], Some((90, (d << 8) + k))).is_some());
            }
            assert!(sim.c.s.capacity_weight_index().term(&bond, Some(collateral)) <= w_cap, "×{}: J-1 at DAA {d}", tier.rho());
            let read = kaspa_consensus_core::palw_issuance_slots_v1::palw_issuance_read_at_v1(
                &sim.c.s,
                &sim.c.sp,
                &bond,
                collateral,
                sim.c.daa + 1,
            )
            .expect("F-S reads");
            assert!(read.outstanding <= read.cap, "×{}: N_out at DAA {d}", tier.rho());
        }
        assert!(admitted > 0, "×{}: the bond issues", tier.rho());
        let mid = sim.tape.len() / 2;
        let off = sim.rewind_to(mid);
        sim.replay(&off);
        println!("J-1 at ×{}: a 13k bond issued {admitted} claims in 25 DAA, its weight within W_cap throughout", tier.rho());
    }
}

/// **NO-FREE-VOID at every tier**: a credited claim left unbound voids `BindTimeout` and holds its slot to
/// `h_obl`; the producer is charged nothing and paid nothing — serving pays.
#[test]
fn a_void_holds_its_slot_and_never_beats_serving_at_every_tier() {
    for tier in Tier::ALL {
        let mut sim = Sim::new(tier.params(Class::Floor), Class::Floor, &[(90, 100_000)]);
        let collateral = sim.c.s.bond(&bond_key(90)).unwrap().collateral;
        let void = sim.claim(90, 0x3C00).expect("admitted");
        let voided_at = sim.c.claim(&void).accepted_daa + sim.c.sp.window_bind() + 1;
        sim.block(voided_at, vec![], None);
        assert!(matches!(sim.c.claim(&void).phase, PalwClaimPhaseV2::Voided { .. }), "×{}: unbound, it voids", tier.rho());
        let read = kaspa_consensus_core::palw_issuance_slots_v1::palw_issuance_read_at_v1(
            &sim.c.s,
            &sim.c.sp,
            &bond_key(90),
            collateral,
            sim.c.daa + 1,
        )
        .expect("F-S reads");
        assert_eq!(read.outstanding, 1, "×{}: the void holds its slot to h_obl", tier.rho());
        assert_eq!(sim.c.s.bond(&bond_key(90)).unwrap().collateral, collateral, "×{}: and is charged nothing", tier.rho());
        assert!(sim.c.s.vesting_row(&void).is_none(), "×{}: and paid nothing", tier.rho());
        let served = licensed_claim(&mut sim, 0x3C01);
        audit(&mut sim, &[served]);
        sim.finalize_all(&[served]);
        assert!(sim.c.s.vesting_row(&served).is_some(), "×{}: serving pays", tier.rho());
    }
}

/// **REORG-DETERMINISM at every tier**: a tape of credited claims, their binds and licences, the pools'
/// receipts and their Finals reverts to its base and re-applies, IBDs from the base, restarts at every
/// tip, and reorgs to a fork that posts the receipts in the other order — root for root.
#[test]
fn a_credited_tape_reverts_ibds_restarts_and_reorgs_at_every_tier() {
    for tier in Tier::ALL {
        let mut c = Chain::new(tier.params(Class::Floor));
        c.attribution = true;
        c.step(&[bond_obj(90, 100_000 * MSK), bond_obj(CHALLENGER, 400_000 * MSK)]);
        let mut t = Tape::new(c);
        let seats = t.c.floor_seats();
        let mut ids = Vec::new();
        for seed in 0..4u64 {
            let (env, key, id) = floor_attempt_of(&t.c, 90, 0x3D00 + seed);
            let anchor = floor_job_anchor(&t.c.p, bond_key(90), 0x10C0 + 0x3D00 + seed);
            let daa = t.c.daa + 1;
            t.block(daa, vec![], Some((env, key, anchor)), T12_BLOCK_SUBSIDY_SOMPI).expect("the block folds");
            if t.c.s.claim(&id).is_some() {
                ids.push(id);
            }
        }
        assert!(ids.len() >= 2, "×{}: claims admitted: {}", tier.rho(), ids.len());
        for id in &ids {
            let bound = t.bind_to(*id, &seats);
            t.step(vec![PalwConsensusObjectV2::ReceiptLicensed {
                claim: *id,
                receipts: seats.iter().map(|(k, _)| valid(*id, *k, bound)).collect(),
            }]);
        }
        let fork_at = t.len();
        let receipts = audit_receipts_for(&t.c.s, &t.c.sp, &ids);
        assert!(!receipts.is_empty(), "×{}: the credited claims wait for receipts", tier.rho());
        for batch in receipts {
            t.step(vec![batch]);
        }
        let last = ids.iter().filter_map(|id| t.c.s.deadline_of(id)).max().expect("audited claims owe deadlines");
        t.at(last + 1, vec![]);
        assert!(
            ids.iter().all(|id| matches!(t.c.s.claim(id).map(|c| c.phase.clone()), Some(PalwClaimPhaseV2::Final { .. }))),
            "×{}: Final through the audits",
            tier.rho()
        );
        t.revert_to_base_and_reapply();
        t.ibd_from(t.base.clone());
        for j in 0..=t.len() {
            t.restart_at(j);
        }
        let mut fork = t.fork(fork_at);
        for batch in audit_receipts_for(&fork.c.s, &fork.c.sp, &ids).into_iter().rev() {
            fork.step(vec![batch]);
        }
        t.reorg_to(fork_at, &fork);
        println!("determinism ×{}: {} credited claims, {} blocks, fork at {fork_at}", tier.rho(), ids.len(), t.len());
    }
}

/// **D-13 at every tier: the 2M row stays `c_2M = 1` and uncredited.** Under the ρ = 10 package (and the
/// ready variants) one bond's 2M claim is admitted and another bond's is refused while it is live (the
/// C7 row's static `max_inflight_claims`, which no ρ scales); it is never credited, its escrow slot is
/// today's and its `G` today's (F1) — only its weight reservation is `⌈w/ρ⌉`, like every attempt's under
/// F-W, the whole bond standing behind it (AG-2).
#[test]
fn the_2m_row_stays_capped_at_one_and_uncredited_at_every_tier() {
    let producers = [(90, 1_000_000), (91, 1_000_000)];
    let mut today = Sim::new(params_for(Class::M2, true), Class::M2, &producers);
    let id_today = today.claim(90, 0x2A00).expect("a 2M claim at ρ = 1");
    assert!(today.claim(91, 0x2A01).is_none(), "ρ = 1: the cap of one");
    let t = today.c.claim(&id_today);
    let g_today = palw_claim_g_v1(&today.c.s, &PalwCapacityGainScaleV1::of(&today.c.sp), &id_today).unwrap().g();
    let slot_today = palw_escrow_term_v2(&today.c.sp, t.accepted_daa, t.escrowed_reward, &t.class_id);
    for tier in Tier::ALL {
        let rho = u128::from(tier.rho());
        let mut sim = Sim::new(tier.params(Class::M2), Class::M2, &producers);
        let id = sim.claim(90, 0x2A00).expect("one 2M claim");
        let skipped = sim.skips.values().sum::<usize>();
        assert!(sim.claim(91, 0x2A01).is_none(), "×{rho}: a second 2M claim is refused while the first is live");
        assert!(sim.skips.values().sum::<usize>() > skipped, "×{rho}: refused, not lost");
        let claim = sim.c.claim(&id);
        assert!(!palw_capacity_claim_credited_v1(&sim.c.sp, &claim), "×{rho}: 2M is never credited");
        assert_eq!(claim.reserved, t.reserved.div_ceil(rho), "×{rho}: the reservation ⌈w/ρ⌉ (F-W)");
        let slot = palw_escrow_term_v2(&sim.c.sp, claim.accepted_daa, claim.escrowed_reward, &claim.class_id);
        assert_eq!(slot, slot_today, "×{rho}: the escrow slot is today's");
        let g = palw_claim_g_v1(&sim.c.s, &PalwCapacityGainScaleV1::of(&sim.c.sp), &id).unwrap().g();
        assert!(g >= g_today && g < g_today + rho, "×{rho}: G {g} is today's {g_today}");
    }
}
