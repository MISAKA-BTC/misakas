//! **ADR-0165 x ADR-0164: a capacity rider is no event of the floor state machine** (the coordinator's decision of 2026-10-03, on INT's
//! finding that the riders' attempt origin — built for ADR-0164 F-M1's `AttemptRidersV1`, tag 95 — carried the colour the machine reads).
//!
//! The machine exists to shield REAL attempt BLOCKS from the floor blocks that would colour against them. A rider rides an object the
//! accepting block carries, has no header and is in no colouring, so counting it (as a BLUE REAL attempt, which is what a `blue: true`
//! origin made it) would refuse the floor — the anchors, the bonded fallback — without shielding anything, and would open a keep-alive
//! that costs no block. Only an attempt-lane block's attempt is an event, own or merged; a rider steps the machine neither BLUE nor RED,
//! and its own accounting (the lead's carve split, the reservation, the weight) is unchanged.
//!
//! * `a_rider_only_flow_does_not_keep_normal_past_floor_idle_slots` — a BLUE REAL lead makes the chain Normal; its riders attach at the
//!   last DAA of their window (`PALW_RIDERS_WINDOW_DAA_V1`), eight slots on; Normal still ends `floor_idle_slots` after the LEAD (the
//!   floor is accepted from `lead + floor_idle_slots + 1`), not after the riders, and the rooted state is the lead's `Normal`
//!   untouched until it expires;
//! * `riders_in_idle_do_not_open_a_probe_or_make_the_chain_normal` — a floor lead accepted in Idle moves nothing; REAL riders of its bond
//!   attached to it open no Probe, make no Normal and write no floor state — the floor stays the fallback;
//! * `a_blue_real_lead_carrying_riders_steps_the_machine_exactly_once` — the lead's acceptance writes ONE floor-state entry, whether its
//!   riders ride the same block or a later one; the riders' block writes none.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_floor_riders_are_no_events -- --nocapture`

#[path = "capacity_stage1_common.rs"]
mod stage1;
use stage1::*;

use kaspa_consensus_core::config::params::{
    PALW_T12_CAPACITY_EMISSION_BUDGET_V1, PALW_T12_CAPACITY_MULTI_CLAIM_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1, PALW_T12_CAPACITY_RHO_BREAKER_V1,
    PalwPostLaunchFenceV1,
};
use kaspa_consensus_core::palw_admission_v2::{palw_attempt_derived_pwu_v1, palw_effective_class_target_v1};
use kaspa_consensus_core::palw_aggregate_liability_v1::{PalwCapacityLiabilityV1, PalwCapacityStepOffsetV1};
use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_TRACE_CHUNKS, PalwAttemptEnvelopeV2, attempt_id_v2, attempt_trace_manifest_root_v1};
use kaspa_consensus_core::palw_capacity_s567_v1::{PALW_RIDERS_WINDOW_DAA_V1, palw_rider_challenge_v1};
use kaspa_consensus_core::palw_real_share_v1::PalwFloorModeV1::{Idle, Normal};
use kaspa_consensus_core::palw_real_share_v1::{
    PALW_FLOOR_IDLE_SLOTS_V1 as IDLE, PALW_T12_FLOOR_RESERVE_ENTRY, PalwFloorStateV1,
};
use kaspa_consensus_core::palw_state_v2::{PalwDeltaEntryV2, PalwStateDeltaV2, PalwStateV2Error, palw_class_admits_claim_v1};

/// The three ADR-0164 entries (F-EM, F-M1, F-K): the riders' fence and what it needs.
const S567: [&PalwPostLaunchFenceV1; 3] =
    [&PALW_T12_CAPACITY_EMISSION_BUDGET_V1, &PALW_T12_CAPACITY_MULTI_CLAIM_V1, &PALW_T12_CAPACITY_RHO_BREAKER_V1];

/// testnet-12's release with the ×1000 capacity package armed at [`H`] (riders included, ρ = 250 as `tier_params` of the stage-5/6/7
/// suite does it) and ADR-0165's floor reserve armed at the same height.
fn armed_params() -> Params {
    let mut p = params_for(Class::K8, false);
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().chain(S567) {
        (f.set)(&mut p, Some(ForkActivation::new(H)));
    }
    let value = PalwCapacityLiabilityV1::of_schedule_v1(
        ForkActivation::new(H),
        &[PalwCapacityStepOffsetV1 { after_daa: 0, rho: 250, q_credit_permille: 250 }],
    );
    p.palw_capacity_aggregate_liability = Some(value);
    p.sync_palw_capacity_liability();
    (PALW_T12_FLOOR_RESERVE_ENTRY.set)(&mut p, Some(ForkActivation::new(H)));
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the capacity package and the floor reserve validate over the release: {e:?}"));
    p
}

/// The extras this suite folds with: the chain's own (`Chain::extras_at`) with the work target's floor off. That floor is a function of
/// the block's subsidy and is read from a rider's share of it, and the harness carries no `work_target` fold to price it — so under it a
/// REAL rider's derived pwu saturates and no REAL rider could be admitted here. With it off the derived pwu is the class's declared
/// one (`class_pwu`), which is the rule the machine's tests need; nothing the floor state machine reads moves.
fn extras(c: &Chain, daa: u64) -> kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
    let mut e = c.extras_at(daa);
    e.work_target_active = false;
    e
}

/// One block on `parent` — the fold only.
fn peek(
    c: &Chain,
    parent: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
    x: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    key: Hash64,
) -> Result<(kaspa_consensus_core::palw_state_v2::PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error> {
    fold_with(&c.p, &c.sp, parent, x, objects, work, key, &extras(c, x.daa_score))
}

/// One block at `daa`, committed: the delta re-applies and reverts (a reorg restores it) and the carriage reloads under its root.
/// Returns the delta and the fold's skips.
fn block(
    c: &mut Chain,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    key: Hash64,
    subsidy: u64,
) -> (PalwStateDeltaV2, Vec<(Hash64, String)>) {
    assert!(daa > c.daa, "DAA moves forward");
    let parent = c.s.clone();
    let x = ctx(0xCA_0000 + daa, daa, daa, subsidy);
    let (child, delta, skips) = peek(c, &parent, &x, objects, work, key).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
    assert_eq!(apply_delta_v2(&parent, &delta, &c.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
    assert_eq!(revert_delta_v2(&child, &delta, &c.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(&child).into_state(&c.sp, Some(child.state_root())).expect("the carriage reloads");
    assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
    c.s = child;
    c.daa = daa;
    (delta, skips)
}

/// A model chain (the K8 class `Active`, seats ready) with rich producer bonds 1 and 2, on the armed ruleset.
fn chain() -> (Chain, Hash64) {
    let p = armed_params();
    let k8 = model_classes(&p).0;
    let mut c = Chain::new(p);
    c.room = true;
    c.s = readied(&c.sp, &activated(&c.sp, &c.s, k8), &honest(&c.p), k8, c.daa);
    let bonds: Vec<_> = (1..=2).map(|n| bond_obj(n, RICH)).collect();
    let daa = c.daa + 1;
    let (_, skips) = block(&mut c, daa, &bonds, PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(skips.is_empty(), "{skips:?}");
    (c, k8)
}

/// A REAL (K8) lead of bond `n` as the block's own attempt at the next DAA; its claim id and the data-availability retention it pins.
fn real_lead(c: &mut Chain, k8: Hash64, n: u64, seed: u64) -> (Hash64, u64) {
    reready(c, k8);
    let daa = c.daa + 1;
    let (env, key, id) = k8_attempt(c, k8, n, seed, daa);
    let (_, skips) = block(c, daa, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    assert!(skips.is_empty(), "the lead's block: {skips:?}");
    assert!(c.s.claim(&id).is_some(), "the REAL lead is accepted");
    (id, env.attempt.trace_retention_daa)
}

fn floor_state(c: &Chain) -> PalwFloorStateV1 {
    c.s.floor_state_v1()
}

fn st(mode: kaspa_consensus_core::palw_real_share_v1::PalwFloorModeV1) -> PalwFloorStateV1 {
    PalwFloorStateV1 { mode, last_probe_end: None }
}

/// The fold's own class gate for a floor attempt at `daa` — what a floor producer asks and the fold answers.
fn floor_admitted(c: &Chain, daa: u64) -> bool {
    let base = bundle(&c.p).base_class_id;
    palw_class_admits_claim_v1(&c.s, &c.sp, &extras(c, daa), &base, daa).is_ok()
}

/// The one pwu admission accepts for a `class` attempt at `daa` (ADR-0149), as `check_palw_attempt_admission_v2` derives it for a rider:
/// the class's state target (no work-target floor, [`extras`]) and the derived work of one draw.
fn derived_pwu(c: &Chain, class: Hash64, daa: u64) -> u64 {
    let e = extras(c, daa);
    let target = palw_effective_class_target_v1(&c.s, &c.sp, &class, None).expect("the class has a target");
    let per_draw = c.s.palw_attempt_per_draw_v1(&c.sp.base_class_id(), &class, daa, e.canonical_work_daa, None).expect("a model row");
    palw_attempt_derived_pwu_v1(target, per_draw)
}

/// The REAL (K8) attempt of bond `n` — what `model_claim` builds, at the pwu the chain derives — at `daa`, with its execution key and
/// claim id.
fn k8_attempt(c: &Chain, k8: Hash64, n: u64, seed: u64, daa: u64) -> (PalwAttemptEnvelopeV2, Hash64, Hash64) {
    junk_attempt(k8, bond_key(n), pubkey_of(n), &operator_pubkey_of(n), derived_pwu(c, k8, daa), seed, 0x5_0000 + seed)
}

/// `count` REAL riders of `lead` by bond `n`, to attach at `daa`: each a K8 attempt re-bound to its lead and its index with the lead's
/// data-availability pins (`retention` is the lead's `trace_retention_daa`).
fn k8_riders(c: &Chain, k8: Hash64, lead: Hash64, retention: u64, n: u64, count: usize, seed0: u64, daa: u64) -> (PalwConsensusObjectV2, Vec<Hash64>) {
    let mut ids = Vec::new();
    let riders = (0..count as u32)
        .map(|index| {
            let (mut env, _, _) = k8_attempt(c, k8, n, seed0 + u64::from(index), daa);
            // A rider is admitted like a merged attempt: it carries the class's registered artifact root.
            env.attempt.artifact_root = c.s.class(&k8).expect("the K8 class is registered").artifact_root;
            env.attempt.challenge = palw_rider_challenge_v1(&lead, index);
            env.attempt.trace_chunk_count = PALW_ATTEMPT_V2_TRACE_CHUNKS;
            env.attempt.trace_manifest_root = attempt_trace_manifest_root_v1(env.attempt.trace_root, PALW_ATTEMPT_V2_TRACE_CHUNKS);
            env.attempt.trace_retention_daa = retention;
            ids.push(attempt_id_v2(&env.attempt));
            env
        })
        .collect();
    (PalwConsensusObjectV2::AttemptRidersV1 { lead, riders }, ids)
}

/// The floor-state entries a block's delta wrote.
fn floor_entries(delta: &kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2) -> usize {
    delta.entries.iter().filter(|e| matches!(e, PalwDeltaEntryV2::RealWork { .. })).count()
}

/// Re-prove the genesis cards ready for the K8 class (the registry's readiness horizon) before a block that admits REAL claims.
fn reready(c: &mut Chain, k8: Hash64) {
    c.s = readied(&c.sp, &c.s, &honest(&c.p), k8, c.daa);
}

/// **A rider-only flow does not keep Normal past `floor_idle_slots`.** A BLUE REAL lead is accepted at `d0` (the chain is Normal); its
/// riders attach at `d0 + 8`, the last DAA of their window. The machine is the lead's alone: Normal stays `{ last_blue: d0 }`, it ends at
/// `d0 + floor_idle_slots` — the floor is accepted again from `d0 + floor_idle_slots + 1` — and not eight slots later, as it would if
/// the riders had refreshed it as BLUE REAL attempts.
#[test]
fn a_rider_only_flow_does_not_keep_normal_past_floor_idle_slots() {
    let (mut c, k8) = chain();
    assert!(floor_state(&c).is_default() && floor_admitted(&c, c.daa + 1), "an idle chain accepts the floor");
    let (lead, retention) = real_lead(&mut c, k8, 1, 0x100);
    let d0 = c.daa;
    assert_eq!(floor_state(&c), st(Normal { last_blue: d0 }), "a BLUE REAL lead: the chain is Normal");

    // The riders, at the last DAA of the window.
    let at = d0 + PALW_RIDERS_WINDOW_DAA_V1;
    reready(&mut c, k8);
    let (obj, rider_ids) = k8_riders(&c, k8, lead, retention, 1, 3, 0x200, at);
    let (delta, skips) = block(&mut c, at, &[obj], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(skips.is_empty(), "the batch was taken: {skips:?}");
    assert!(c.s.rider_mark_of_v1(&lead).is_some(), "the lead took its riders");
    for id in &rider_ids {
        assert!(c.s.claim(id).is_some(), "rider {id} is a claim of its own");
    }
    assert_eq!(floor_entries(&delta), 0, "the riders write no floor state");
    assert_eq!(floor_state(&c), st(Normal { last_blue: d0 }), "riders refresh nothing: Normal is still the lead's");

    // Normal ends `floor_idle_slots` after the LEAD.
    for daa in [d0 + 1, at, d0 + IDLE] {
        assert!(!floor_admitted(&c, daa), "DAA {daa}: still Normal, the floor is refused");
    }
    assert!(
        floor_admitted(&c, d0 + IDLE + 1),
        "the floor is the fallback again from `lead + floor_idle_slots + 1` — riders at `lead + {PALW_RIDERS_WINDOW_DAA_V1}` did not move it"
    );
    // And the rooted state follows: written Idle (the default, rooted as nothing) by the first block past it.
    let (delta, _) = block(&mut c, d0 + IDLE + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(floor_entries(&delta), 1, "Normal's expiry is one write");
    assert!(floor_state(&c).is_default(), "Normal ran out on the lead's clock: {}", floor_state(&c));
}

/// **Riders in Idle open no Probe and make no Normal.** A floor lead is accepted in Idle (the bonded fallback; no event); REAL riders of
/// its bond attach to it three slots on. The machine does not move: the state is the default, the riders' block writes no floor-state
/// entry, and the floor is still accepted — a rider is no event, neither BLUE (Normal) nor RED (a Probe).
#[test]
fn riders_in_idle_do_not_open_a_probe_or_make_the_chain_normal() {
    let (mut c, k8) = chain();
    let daa = c.daa + 1;
    let (env, key, lead) = floor_attempt_of(&c, 1, 0x300);
    let (_, skips) = block(&mut c, daa, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    assert!(skips.is_empty(), "the floor lead's block: {skips:?}");
    assert!(c.s.claim(&lead).is_some(), "the floor lead is accepted in Idle");
    assert!(floor_state(&c).is_default(), "a floor attempt is no event");
    let d0 = c.daa;
    let retention = env.attempt.trace_retention_daa;

    let at = d0 + 3;
    reready(&mut c, k8);
    let (obj, rider_ids) = k8_riders(&c, k8, lead, retention, 1, 2, 0x400, at);
    let (delta, skips) = block(&mut c, at, &[obj], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(skips.is_empty(), "the batch was taken: {skips:?}");
    assert_eq!(floor_entries(&delta), 0, "the riders write no floor state");
    for id in &rider_ids {
        let rider = c.s.claim(id).expect("a REAL rider is a claim of its own");
        assert_eq!(rider.class_id, k8, "…of the REAL class");
    }
    assert!(floor_state(&c).is_default(), "no Normal, no Probe: {}", floor_state(&c));
    assert_eq!(floor_state(&c).mode, Idle);
    assert!(floor_admitted(&c, at + 1), "the floor is still the fallback");
}

/// **A BLUE REAL lead carrying riders steps the machine exactly once** — the lead's acceptance writes one floor-state entry (Idle →
/// Normal), whether the riders ride the lead's own block (the batch is queued at step 3 and attached at step 4b′, after the lead's
/// own attempt) or a later one, and the riders' block writes none: a lead with n riders is one event, not 1 + n.
#[test]
fn a_blue_real_lead_carrying_riders_steps_the_machine_exactly_once() {
    for count in [1usize, 3] {
        // (a) the riders ride the lead's own block.
        let (mut c, k8) = chain();
        reready(&mut c, k8);
        let at = c.daa + 1;
        let (env, key, lead) = k8_attempt(&c, k8, 1, 0x500 + count as u64, at);
        let (obj, rider_ids) = k8_riders(&c, k8, lead, env.attempt.trace_retention_daa, 1, count, 0x600, at);
        let (delta, skips) = block(&mut c, at, &[obj], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
        assert!(skips.is_empty(), "n = {count}: the batch was taken: {skips:?}");
        assert_eq!(floor_entries(&delta), 1, "n = {count}, same block: the lead's acceptance is the one write");
        assert_eq!(floor_state(&c), st(Normal { last_blue: at }));
        for id in std::iter::once(&lead).chain(&rider_ids) {
            assert!(c.s.claim(id).is_some(), "n = {count}: the lead and every rider is a claim");
        }

        // (b) the riders ride a later block, five slots on.
        let (mut c, k8) = chain();
        let (lead, retention) = real_lead(&mut c, k8, 1, 0x700 + count as u64);
        let d0 = c.daa;
        assert_eq!(floor_state(&c), st(Normal { last_blue: d0 }), "the lead's block stepped the machine");
        reready(&mut c, k8);
        let at = d0 + 5;
        let (obj, rider_ids) = k8_riders(&c, k8, lead, retention, 1, count, 0x800, at);
        let (delta, skips) = block(&mut c, at, &[obj], PalwBlockWorkV3::None, Hash64::default(), 0);
        assert!(skips.is_empty(), "n = {count}: the batch was taken: {skips:?}");
        assert_eq!(floor_entries(&delta), 0, "n = {count}, later block: the riders write no floor state");
        assert_eq!(floor_state(&c), st(Normal { last_blue: d0 }), "…and refresh nothing");
        assert_eq!(rider_ids.iter().filter(|id| c.s.claim(id).is_some()).count(), count, "n = {count}: every rider is a claim");
    }
}
