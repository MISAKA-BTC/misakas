//! **ADR-0165 A″ through the fold** — the floor state machine, in rooted state: the floor is the idle-only bonded
//! fallback. A REAL attempt the fold FULLY accepts steps the state (the block's own attempt BLUE, a merged one BLUE or RED
//! as `extras.merged_reds` says, in the fold's order); the floor attempt's gate (`FloorNotIdle`, a pre-write skip: no claim,
//! no reward, no weight) reads the state at its place in that order; time steps it at every block's start; every change is
//! one delta entry, so a reorg reverts it exactly; the carriage — a pruning snapshot — carries it. The machine itself
//! (`palw_floor_step_v1`) is tested beside its definition; this is the fold around it.

use super::*;
use crate::palw_real_share_v1::PalwFloorModeV1::{Idle, Normal, Probe};
use crate::palw_real_share_v1::{
    PALW_FLOOR_IDLE_SLOTS_V1 as IDLE, PALW_FLOOR_PROBE_COOLDOWN_SLOTS_V1 as COOL, PALW_FLOOR_PROBE_SLOTS_V1 as PROBE, PalwFloorStateV1,
};

const FENCE: u64 = 100;
/// The pwu every attempt here claims: 8 × 5 sompi = 40 of the bond's 1,000, so twenty-five claims fit.
const PWU: u64 = 8;

fn fp() -> PalwStateParamsV2 {
    params().with_floor_reserve_from_daa(Some(FENCE))
}

fn admission() -> crate::palw_admission_v2::PalwAdmissionParamsV2 {
    crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap()
}

/// A REAL class registered like the floor: the same price, the same pwu rule, a share of its own.
fn real_class(id: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ClassRegistered {
        class_id: h64(id),
        artifact_root: h64(0xA1),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
        initial_target: u128::MAX / 2,
        share_permille: 100,
        activation_daa: 0,
        admission: None,
    }
}

/// A world with the floor (class 1) and a REAL class (class 2), both registered at the fence.
fn world(p: &PalwStateParamsV2) -> PalwChainStateV2 {
    let mut objects = register_class_and_bond();
    objects.push(real_class(2));
    let (s, _) = apply(&PalwChainStateV2::genesis(), p, &ctx(1, FENCE, 1), &objects, None);
    s
}

fn env(class: Hash64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    attempt_for_class(PWU, nonce, class, bond_key(1), vec![7; 4], op_id(21), if class == h64(1) { h64(11) } else { h64(0xA1) })
}
fn real(nonce: u64) -> PalwAttemptEnvelopeV2 {
    env(h64(2), nonce)
}
fn floor(nonce: u64) -> PalwAttemptEnvelopeV2 {
    env(h64(1), nonce)
}
fn claim_of(e: &PalwAttemptEnvelopeV2) -> Hash64 {
    attempt_id_v2(&e.attempt)
}

/// Chain blocks are numbered by their DAA here: block word and blue score are the DAA's own, so every step is strictly
/// increasing in blue score and never decreasing in DAA.
fn at(daa: u64) -> PalwBlockContextV2 {
    ctx(daa, daa, daa)
}

fn floor_of(s: &PalwChainStateV2) -> PalwFloorStateV1 {
    s.floor_state_v1()
}

fn merged<'a>(block: u64, e: &'a PalwAttemptEnvelopeV2) -> PalwMergedWorkV1<'a> {
    PalwMergedWorkV1 {
        carrying_block: h64(block),
        work: PalwBlockWorkV3::Attempt(e),
        execution_key: h64(0x1000 + block),
        subsidy: 0,
        escrow_carve: None,
        bits: 0,
        job_anchor: Hash64::default(),
    }
}

/// A block at `daa` merging `works` (carrying block, envelope), with `reds` the carrying blocks of the mergeset's reds.
fn fold(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    daa: u64,
    own: Option<&PalwAttemptEnvelopeV2>,
    works: &[(u64, &PalwAttemptEnvelopeV2)],
    reds: &[u64],
) -> (PalwChainStateV2, PalwStateDeltaV2, Vec<(BlockHash, String)>) {
    fold_with(parent, p, daa, own, works, reds, PalwTransitionExtrasV1::default())
}

/// [`fold`] under `base` extras (the fences a test needs live); `merged_reds` is set from `reds`.
fn fold_with(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    daa: u64,
    own: Option<&PalwAttemptEnvelopeV2>,
    works: &[(u64, &PalwAttemptEnvelopeV2)],
    reds: &[u64],
    base: PalwTransitionExtrasV1,
) -> (PalwChainStateV2, PalwStateDeltaV2, Vec<(BlockHash, String)>) {
    let merged: Vec<_> = works.iter().map(|(block, e)| merged(*block, e)).collect();
    let extras = PalwTransitionExtrasV1 { merged_reds: reds.iter().map(|b| h64(*b)).collect(), ..base };
    let work = match own {
        Some(e) => PalwBlockWorkV3::Attempt(e),
        None => PalwBlockWorkV3::None,
    };
    let (next, delta, skips) = apply_palw_transition_v7(
        parent,
        p,
        Some(&admission()),
        &at(daa),
        &[],
        work,
        &merged,
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("the block stands");
    next.assert_internal_consistency(p).expect("internal consistency");
    (next, delta, skips)
}

/// A block at `daa` with nothing in it.
fn tick(parent: &PalwChainStateV2, p: &PalwStateParamsV2, daa: u64) -> (PalwChainStateV2, PalwStateDeltaV2) {
    let (s, d, _) = fold(parent, p, daa, None, &[], &[]);
    (s, d)
}

fn st(mode: crate::palw_real_share_v1::PalwFloorModeV1, last_probe_end: Option<u64>) -> PalwFloorStateV1 {
    PalwFloorStateV1 { mode, last_probe_end }
}

/// **The floor is accepted in Idle, refused in Probe and in Normal, and accepted again once the state has run out** — a
/// block's own attempts through the fold, claim by claim.
#[test]
fn the_floor_is_accepted_in_idle_refused_in_probe_and_normal_and_accepted_again() {
    let p = fp();
    let s = world(&p);
    assert_eq!(floor_of(&s), PalwFloorStateV1::default(), "nothing is written before a REAL attempt");
    // Idle: the bonded floor is the fallback — accepted, and it moves no state.
    let (s, _, skips) = fold(&s, &p, 105, Some(&floor(1)), &[], &[]);
    assert!(s.claim(&claim_of(&floor(1))).is_some() && skips.is_empty(), "idle: the floor attempt is accepted");
    assert_eq!(floor_of(&s), PalwFloorStateV1::default(), "a floor attempt is not a REAL event");
    // A RED REAL attempt merged in Idle opens a Probe until 106 + PROBE.
    let (s, _, _) = fold(&s, &p, 106, None, &[(0xB1, &real(2))], &[0xB1]);
    assert!(s.claim(&claim_of(&real(2))).is_some());
    assert_eq!(floor_of(&s), st(Probe { until: 106 + PROBE }, None));
    // In the Probe the floor is refused by name — no claim, nothing reserved, and it moves no state.
    let before = s.clone();
    let (s, _, skips) = fold(&s, &p, 108, Some(&floor(3)), &[], &[]);
    assert!(s.claim(&claim_of(&floor(3))).is_none(), "probe: the floor attempt writes no claim");
    // The fold returns the refusal in its skip list — the node logs it ("carried work this chain point refused") — naming the
    // block that carried the attempt and the state that refused it.
    assert_eq!(skips.len(), 1, "the refused floor attempt is reported, not folded");
    assert_eq!(skips[0].0, h64(108), "naming the block that carried it");
    assert!(skips[0].1.contains("idle-only fallback") && skips[0].1.contains("probe"), "and the state: {}", skips[0].1);
    assert_eq!((s.safe_weight(), s.bounded_immature()), (before.safe_weight(), before.bounded_immature()), "and no weight");
    assert_eq!(s.reserved_exposure(&bond_key(1)), before.reserved_exposure(&bond_key(1)), "and no reservation");
    let why = palw_class_admits_claim_v1(&s, &p, &PalwTransitionExtrasV1::default(), &h64(1), 108).unwrap_err();
    assert!(matches!(&why, PalwStateV2Error::FloorNotIdle { daa: 108, state, .. } if state.mode == Probe { until: 106 + PROBE }), "{why:?}");
    assert!(why.to_string().contains("idle-only fallback") && why.to_string().contains("probe"), "{why}");
    assert!(palw_class_admits_claim_v1(&s, &p, &PalwTransitionExtrasV1::default(), &h64(2), 108).is_ok(), "REAL work is untouched");
    // A BLUE REAL attempt inside the Probe (here the block's own) makes it Normal; the floor stays refused through
    // floor_idle_slots after it.
    let (s, _, _) = fold(&s, &p, 109, Some(&real(4)), &[], &[]);
    assert_eq!(floor_of(&s), st(Normal { last_blue: 109 }, None));
    for daa in [110, 109 + IDLE] {
        assert!(palw_class_admits_claim_v1(&s, &p, &PalwTransitionExtrasV1::default(), &h64(1), daa).is_err(), "normal at {daa}");
    }
    let (s, _) = tick(&s, &p, 109 + IDLE);
    assert_eq!(floor_of(&s), st(Normal { last_blue: 109 }, None), "still Normal at exactly floor_idle_slots");
    // One slot later Normal has run out: the block's start steps it to Idle and the floor is the fallback again.
    let (s, d, skips) = fold(&s, &p, 109 + IDLE + 1, Some(&floor(5)), &[], &[]);
    assert!(s.claim(&claim_of(&floor(5))).is_some() && skips.is_empty(), "the floor resumes one slot after Normal ran out");
    assert_eq!(floor_of(&s), PalwFloorStateV1::default(), "back to the default, rooted as nothing");
    assert!(d.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::RealWork { new: None, .. })), "and the step is one delta entry");
}

/// **A Probe that nobody answers expires at `until`, and the cooldown then holds RED off — never BLUE** — floors valid
/// again from `until`, the next probe only `probe_cooldown_slots` after it, and a BLUE attempt is Normal at once.
#[test]
fn an_unanswered_probe_expires_at_until_and_the_cooldown_holds_red_off_but_not_blue() {
    let p = fp();
    let s = world(&p);
    let (s, _, _) = fold(&s, &p, 105, None, &[(0xB1, &real(1))], &[0xB1]);
    assert_eq!(floor_of(&s), st(Probe { until: 105 + PROBE }, None));
    let (s, _) = tick(&s, &p, 105 + PROBE - 1);
    assert_eq!(floor_of(&s), st(Probe { until: 105 + PROBE }, None), "the last slot of the probe");
    // At `until` the block's start steps it to Idle and records the end; the floor is accepted in that very block.
    let (s, _, skips) = fold(&s, &p, 105 + PROBE, Some(&floor(2)), &[], &[]);
    assert!(s.claim(&claim_of(&floor(2))).is_some() && skips.is_empty());
    assert_eq!(floor_of(&s), st(Idle, Some(105 + PROBE)), "unanswered: Idle, and the probe's end is the cooldown's reference");
    // A RED REAL attempt during the cooldown changes nothing, and the floor stays valid…
    let (red_in_cooldown, _, _) = fold(&s, &p, 105 + PROBE + 3, None, &[(0xB2, &real(3))], &[0xB2]);
    assert_eq!(floor_of(&red_in_cooldown), st(Idle, Some(105 + PROBE)), "the cooldown is running: a RED one is held off");
    assert!(palw_class_admits_claim_v1(&red_in_cooldown, &p, &PalwTransitionExtrasV1::default(), &h64(1), 105 + PROBE + 4).is_ok());
    // …and once the cooldown has run the next RED one opens the next probe.
    let (next, _, _) = fold(&red_in_cooldown, &p, 105 + PROBE + COOL, None, &[(0xB3, &real(4))], &[0xB3]);
    assert_eq!(floor_of(&next), st(Probe { until: 105 + PROBE + COOL + PROBE }, Some(105 + PROBE)));
    // A BLUE one in the same cooldown is Normal at once, and leaves the cooldown's reference where it was.
    let (blue_in_cooldown, _, _) = fold(&s, &p, 105 + PROBE + 3, None, &[(0xB2, &real(3))], &[]);
    assert_eq!(floor_of(&blue_in_cooldown), st(Normal { last_blue: 105 + PROBE + 3 }, Some(105 + PROBE)));
    assert!(palw_class_admits_claim_v1(&blue_in_cooldown, &p, &PalwTransitionExtrasV1::default(), &h64(1), 105 + PROBE + 4).is_err());
}

/// **A RED REAL attempt never extends a Probe or Normal, and a BLUE one does** — merged attempts, their colour read from
/// `extras.merged_reds`.
#[test]
fn a_red_real_attempt_never_extends_and_a_blue_one_does() {
    let p = fp();
    let s = world(&p);
    // A merged RED REAL attempt in Idle opens a probe (any colour does)…
    let (s, _, skips) = fold(&s, &p, 105, None, &[(0xB1, &real(1))], &[0xB1]);
    assert!(skips.is_empty() && s.claim(&claim_of(&real(1))).is_some(), "a RED REAL attempt is applied by the merging block's fold");
    assert_eq!(floor_of(&s), st(Probe { until: 105 + PROBE }, None));
    // …a second RED inside it extends nothing…
    let (s, _, _) = fold(&s, &p, 107, None, &[(0xB2, &real(2))], &[0xB2]);
    assert_eq!(floor_of(&s), st(Probe { until: 105 + PROBE }, None), "RED never extends a probe");
    // …a BLUE one makes it Normal…
    let (s, _, _) = fold(&s, &p, 108, None, &[(0xB3, &real(3))], &[]);
    assert_eq!(floor_of(&s), st(Normal { last_blue: 108 }, None));
    // …a RED one refreshes nothing, a BLUE one does.
    let (s, _, _) = fold(&s, &p, 120, None, &[(0xB4, &real(4))], &[0xB4]);
    assert_eq!(floor_of(&s), st(Normal { last_blue: 108 }, None), "RED never extends Normal");
    let (s, _, _) = fold(&s, &p, 125, None, &[(0xB5, &real(5))], &[]);
    assert_eq!(floor_of(&s), st(Normal { last_blue: 125 }, None));
    // The colour is the carrying block's membership in the mergeset's reds, nothing else: the same work with no reds is BLUE,
    // and BLUE in Idle is Normal at once.
    let fresh = world(&p);
    let (blue_run, _, _) = fold(&fresh, &p, 105, None, &[(0xB1, &real(1))], &[]);
    assert_eq!(floor_of(&blue_run), st(Normal { last_blue: 105 }, None));
}

/// **Several attempts in one mergeset apply in the fold's order**: the block's own first, then the merged works in
/// consensus order — and a floor's gate reads the state at its place in it.
#[test]
fn several_attempts_in_one_mergeset_apply_in_folds_order_and_a_floor_reads_its_place() {
    let p = fp();
    let s = world(&p);
    // Idle + [merged RED, merged BLUE]: Probe, then Normal.
    let (a, _, _) = fold(&s, &p, 105, None, &[(0xB1, &real(1)), (0xB2, &real(2))], &[0xB1]);
    assert_eq!(floor_of(&a), st(Normal { last_blue: 105 }, None));
    // Idle + [merged BLUE, merged RED]: Normal, and the RED changes nothing.
    let (b, _, _) = fold(&s, &p, 105, None, &[(0xB1, &real(1)), (0xB2, &real(2))], &[0xB2]);
    assert_eq!(floor_of(&b), st(Normal { last_blue: 105 }, None));
    // The block's own attempt comes first: own REAL (BLUE, Normal) then a merged RED changes nothing.
    let (c, _, _) = fold(&s, &p, 105, Some(&real(1)), &[(0xB2, &real(2))], &[0xB2]);
    assert_eq!(floor_of(&c), st(Normal { last_blue: 105 }, None));
    // Two REDs in Idle are one probe.
    let (d, _, _) = fold(&s, &p, 105, None, &[(0xB1, &real(1)), (0xB2, &real(2))], &[0xB1, 0xB2]);
    assert_eq!(floor_of(&d), st(Probe { until: 105 + PROBE }, None));
    // A floor merged BEFORE the REAL attempt meets Idle and is accepted; merged AFTER it, it meets the Probe and is refused.
    let (before, _, skips) = fold(&s, &p, 105, None, &[(0xB1, &floor(1)), (0xB2, &real(2))], &[0xB2]);
    assert!(before.claim(&claim_of(&floor(1))).is_some() && skips.is_empty(), "the floor ahead of the REAL attempt is accepted");
    assert_eq!(floor_of(&before), st(Probe { until: 105 + PROBE }, None));
    for reds in [&[0xB2u64][..], &[][..]] {
        let (after, _, skips) = fold(&s, &p, 105, None, &[(0xB2, &real(2)), (0xB1, &floor(1))], reds);
        assert!(after.claim(&claim_of(&floor(1))).is_none(), "the floor behind a REAL attempt (reds {reds:?}) is refused");
        assert_eq!(skips.len(), 1, "and the refusal is named in the skip list");
        assert!(skips[0].1.contains("idle-only fallback"), "{}", skips[0].1);
        assert_eq!(skips[0].0, h64(0xB1), "against the floor's carrying block");
    }
}

/// **Blocks straddling the fence**: the machine runs only for blocks at or past the fence; a REAL attempt accepted below
/// it writes nothing, and one merged past it counts at the merging block's DAA whenever it was drawn.
#[test]
fn blocks_straddling_the_fence_count_at_the_accepting_blocks_daa() {
    let p = params().with_floor_reserve_from_daa(Some(110));
    let (s0, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &{
        let mut o = register_class_and_bond();
        o.push(real_class(2));
        o
    }, None);
    // Below the fence: a REAL attempt, accepted, writes nothing; the floor is accepted after it.
    let (s1, _, _) = fold(&s0, &p, 105, Some(&real(1)), &[], &[]);
    assert!(s1.claim(&claim_of(&real(1))).is_some());
    assert_eq!(floor_of(&s1), PalwFloorStateV1::default(), "below the fence nothing is written");
    let (s2, _, skips) = fold(&s1, &p, 108, Some(&floor(2)), &[], &[]);
    assert!(s2.claim(&claim_of(&floor(2))).is_some() && skips.is_empty(), "below the fence a floor follows a REAL attempt as it always did");
    // At the fence: the block at 112 merges a REAL attempt that was drawn below it — it counts, at 112.
    let (s3, _, _) = fold(&s2, &p, 112, None, &[(0xB1, &real(3))], &[0xB1]);
    assert_eq!(floor_of(&s3), st(Probe { until: 112 + PROBE }, None));
    let (s3_blue, _, _) = fold(&s2, &p, 112, None, &[(0xB1, &real(3))], &[]);
    assert_eq!(floor_of(&s3_blue), st(Normal { last_blue: 112 }, None), "a BLUE one is Normal");
    // A node that folds the same two blocks from the pre-fence state agrees: the state is a function of the chain.
    let (s3b, _, _) = fold(&s2.clone(), &p, 112, None, &[(0xB1, &real(3))], &[0xB1]);
    assert_eq!(s3.state_root(), s3b.state_root());
}

/// **Reorg determinism**: every state change is ONE delta entry; reverting the blocks newest-first and re-applying them
/// reproduces every root; two branches from one parent each compute their own state, and switching between them through
/// the deltas lands on each branch's own root — the state is a function of the branch, never a node-local memory.
#[test]
fn a_reorg_reverts_and_reapplies_the_floor_state_exactly_and_branches_compute_their_own() {
    let p = fp();
    let base = world(&p);
    // Branch A: a REAL attempt of its own (Normal), a BLUE merged one (Normal again), then quiet until Normal runs out (Idle).
    let (a1, da1, _) = fold(&base, &p, 105, Some(&real(1)), &[], &[]);
    let (a2, da2, _) = fold(&a1, &p, 107, None, &[(0xB1, &real(2))], &[]);
    let (a3, da3) = tick(&a2, &p, 107 + IDLE + 1);
    // Branch B from the same parent: a floor attempt and a RED REAL merge.
    let (b1, db1, _) = fold(&base, &p, 105, Some(&floor(9)), &[], &[]);
    let (b2, db2, _) = fold(&b1, &p, 107, None, &[(0xB1, &real(2))], &[0xB1]);
    assert_eq!(floor_of(&a1), st(Normal { last_blue: 105 }, None));
    assert_eq!(floor_of(&a2), st(Normal { last_blue: 107 }, None));
    assert_eq!(floor_of(&a3), PalwFloorStateV1::default());
    assert_eq!(floor_of(&b1), PalwFloorStateV1::default(), "branch B's floor moves no state");
    assert_eq!(floor_of(&b2), st(Probe { until: 107 + PROBE }, None), "branch B: a RED REAL merge only opens a probe");
    assert_ne!(a2.state_root(), b2.state_root(), "the branches' states differ in the root");
    // Revert A newest-first to the base, root by root…
    let back2 = revert_delta_v2(&a3, &da3, &p).expect("revert");
    assert_eq!(back2.state_root(), a2.state_root());
    let back1 = revert_delta_v2(&back2, &da2, &p).expect("revert");
    assert_eq!(back1.state_root(), a1.state_root());
    let back0 = revert_delta_v2(&back1, &da1, &p).expect("revert");
    assert_eq!(back0.state_root(), base.state_root(), "a reorg drags no floor state");
    // …and apply B on the reverted base: B's own roots.
    let fb1 = apply_delta_v2(&back0, &db1, &p).expect("apply");
    assert_eq!(fb1.state_root(), b1.state_root());
    let fb2 = apply_delta_v2(&fb1, &db2, &p).expect("apply");
    assert_eq!(fb2.state_root(), b2.state_root());
    // A delta applied to a state that is not its parent's is refused by name, never silently.
    assert!(apply_delta_v2(&b1, &da2, &p).is_err(), "a delta whose floor-state expectation is not the state's is a mismatch");
}

/// **The carriage — what a pruning snapshot is — carries the state**, and a network that never left Idle carries nothing.
#[test]
fn the_carriage_carries_the_floor_state_and_an_idle_network_carries_nothing() {
    let p = fp();
    let (probe, _, _) = fold(&world(&p), &p, 105, None, &[(0xB1, &real(1))], &[0xB1]);
    assert!(matches!(floor_of(&probe).mode, Probe { .. }));
    let (normal, _, _) = fold(&probe, &p, 106, None, &[(0xB2, &real(2))], &[]);
    assert!(matches!(floor_of(&normal).mode, Normal { .. }));
    for state in [&probe, &normal] {
        let carriage = PalwStateCarriageV2::from_state(state);
        assert!(carriage.floor_state.is_some());
        let bytes = borsh::to_vec(&carriage).expect("encode");
        let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decode");
        assert_eq!(back.floor_state, state.floor_state);
        let rebuilt = back.into_state(&p, None).expect("a consistent state");
        assert_eq!(rebuilt.state_root(), state.state_root(), "the snapshot roots to the same value");
        assert_eq!(floor_of(&rebuilt), floor_of(state));
    }
    assert_ne!(probe.state_root(), normal.state_root(), "the root commits the state");
    let idle = world(&p);
    assert!(PalwStateCarriageV2::from_state(&idle).floor_state.is_none(), "nothing is carried before a REAL attempt");
    // Below the fence the root is the root of a build without the field: the same chain, the same bytes.
    let below = params();
    let (a, _) = apply(&PalwChainStateV2::genesis(), &below, &ctx(1, FENCE, 1), &register_class_and_bond(), None);
    let (b, _, _) = fold(&a, &below, 105, Some(&floor(1)), &[], &[]);
    assert!(b.floor_state.is_none() && PalwStateCarriageV2::from_state(&b).floor_state.is_none());
}

/// **Time writes once.** A Probe's expiry is one delta entry at the block that first sees it; the blocks after it write
/// nothing — the idle chain's roots do not move while it idles.
#[test]
fn time_writes_the_floor_state_once() {
    let p = fp();
    let s = world(&p);
    let (mut s, _, _) = fold(&s, &p, 105, None, &[(0xB1, &real(1))], &[0xB1]);
    assert_eq!(floor_of(&s), st(Probe { until: 105 + PROBE }, None));
    let mut writes = Vec::new();
    for daa in 106..=105 + PROBE + 10 {
        let (next, delta) = tick(&s, &p, daa);
        writes.push((daa, delta.entries.iter().filter(|e| matches!(e, PalwDeltaEntryV2::RealWork { .. })).count()));
        s = next;
    }
    let total: usize = writes.iter().map(|(_, n)| n).sum();
    assert_eq!(total, 1, "{writes:?}");
    assert_eq!(writes.iter().find(|(_, n)| *n == 1).map(|(d, _)| *d), Some(105 + PROBE), "at the first block at or past `until`");
    assert_eq!(floor_of(&s), st(Idle, Some(105 + PROBE)));
}

/// **An all-floor network crossing the fence is Idle and live** — the floor is accepted block after block, and one REAL
/// attempt arriving opens a probe and ends the stretch.
#[test]
fn an_all_floor_network_crossing_the_fence_is_idle_and_live() {
    let p = fp();
    let mut s = world(&p);
    for (i, daa) in (105..125).enumerate() {
        let (next, _, skips) = fold(&s, &p, daa, Some(&floor(100 + i as u64)), &[], &[]);
        assert!(next.claim(&claim_of(&floor(100 + i as u64))).is_some() && skips.is_empty(), "DAA {daa}: the floor is the fallback");
        s = next;
    }
    assert_eq!(floor_of(&s), PalwFloorStateV1::default());
}

/// **Only a FULLY accepted REAL attempt moves the machine** (ADR-0165: "fully accepted" = every fold check passed and the claim
/// is written). A REAL attempt the fold refuses — here for the bond's exposure ceiling (the network's ceiling is lowered to 500‰
/// of the bond's 1,000, and the REAL class's longest job is worth 800), as the block's own attempt and as a merged one, BLUE or
/// RED — writes no claim and moves no state: the floor stays valid, nothing is written, and a fully accepted one right after it
/// moves the machine as ever. The kind a header claims never moves it; the claim does.
#[test]
fn only_a_fully_accepted_real_attempt_moves_the_floor_state() {
    let p = fp().with_fp_exposure_ceiling(500).unwrap();
    let s = world(&p);
    // 160 pwu × 5 sompi = 800 against a ceiling of 500: it never fits.
    let audit = PalwTransitionExtrasV1 { audit_2026_09_23_active: true, ..Default::default() };
    let heavy = |nonce: u64| attempt_for_class(160, nonce, h64(2), bond_key(1), vec![7; 4], op_id(21), h64(0xA1));
    let before = s.clone();
    // The block's own attempt, refused: skipped, nothing written.
    let own = heavy(11);
    let (a, d, skips) = fold_with(&s, &p, 105, Some(&own), &[], &[], audit.clone());
    assert!(a.claim(&claim_of(&own)).is_none(), "the refused attempt writes no claim");
    assert_eq!(skips.len(), 1, "and is reported: {skips:?}");
    assert!(skips[0].1.contains("exposure"), "for the ceiling: {}", skips[0].1);
    assert!(!d.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::RealWork { .. })), "and writes no floor state");
    assert_eq!(floor_of(&a), PalwFloorStateV1::default());
    // A merged one, BLUE and RED: the same.
    for (carrying, reds) in [(0xB1u64, vec![]), (0xB2, vec![0xB2])] {
        let merged_heavy = heavy(12 + carrying);
        let (b, d, skips) = fold_with(&s, &p, 106, None, &[(carrying, &merged_heavy)], &reds, audit.clone());
        assert!(b.claim(&claim_of(&merged_heavy)).is_none(), "carried by {carrying:#x}: no claim");
        assert_eq!(skips.len(), 1, "{skips:?}");
        assert!(!d.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::RealWork { .. })), "no floor state written");
        assert_eq!(floor_of(&b), PalwFloorStateV1::default());
        assert!(palw_class_admits_claim_v1(&b, &p, &PalwTransitionExtrasV1::default(), &h64(1), 106).is_ok(), "the floor is still valid");
    }
    // The refusal moved nothing anywhere else either.
    assert_eq!(s.state_root(), before.state_root());
    // A fully accepted attempt of the same class that fits, straight after, moves it.
    let (c, _, _) = fold_with(&s, &p, 107, Some(&real(21)), &[], &[], audit);
    assert!(c.claim(&claim_of(&real(21))).is_some());
    assert_eq!(floor_of(&c), st(Normal { last_blue: 107 }, None));
}

/// **The event source of an attempt's origin is explicit** (ADR-0165 × ADR-0164): a block's attempt is an event with the colour its
/// mergeset gives it; a capacity rider is none — neither BLUE nor RED (`palw_floor_riders_are_no_events` runs it through the fold).
#[test]
fn the_floor_event_source_is_a_colour_for_a_blocks_attempt_and_nothing_for_a_rider() {
    use crate::palw_state_v2::PalwFloorEventSourceV1 as Source;
    assert_eq!(Source::BlockBlue.blue(), Some(true));
    assert_eq!(Source::BlockRed.blue(), Some(false));
    assert_eq!(Source::Rider.blue(), None, "a rider is no event of the machine");
    assert_eq!(Source::of_merged_block(false), Source::BlockBlue, "a merged block outside the mergeset's reds is BLUE");
    assert_eq!(Source::of_merged_block(true), Source::BlockRed, "one in the reds is RED");
}
