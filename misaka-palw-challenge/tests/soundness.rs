//! **Effective false-accept accounting, pinned** (golden vectors), its wiring into the approval check, and the complete-check
//! policy shape. Every number here is derived by hand in `docs/design/palw/opv-beacon-bootstrap.md` §6–§7.

use misaka_palw_challenge::hash::named_id;
use misaka_palw_challenge::policy::{ImplementedV1, PolicyRefusalV1};
use misaka_palw_challenge::soundness::{ceil_log2_v1, sampled_family_millibits_v1};
use misaka_palw_challenge::*;

fn input(relations: Vec<RelationSoundnessV1>, r: u32, retry: u32, g: u128, beacons: u32, q: u128) -> EffectiveSoundnessInputV1 {
    EffectiveSoundnessInputV1 {
        relations,
        repetition_count: r,
        retry_limit: retry,
        grinding_choices_per_beacon: g,
        beacons_per_attempt: beacons,
        adaptive_queries: q,
    }
}

fn bits(i: EffectiveSoundnessInputV1) -> EffectiveBitsV1 {
    effective_false_accept_bits_v1(&i).unwrap()
}

use misaka_palw_challenge::soundness::RelationSoundnessV1::{Complete, MilliBits};

#[test]
fn ceil_log2_golden() {
    for (x, want) in [(0u128, 0u64), (1, 0), (2, 1), (3, 2), (4, 2), (5, 3), (992, 10), (1024, 10), (1025, 11), (u128::MAX, 128)] {
        assert_eq!(ceil_log2_v1(x), want, "ceil log2 {x}");
    }
}

#[test]
fn the_sampled_family_bits_are_the_scopes_own_model() {
    assert_eq!(sampled_family_millibits_v1(2, 1_000_000), 2_885, "the interim drill scope: 2 draws at the densest fault");
    assert_eq!(sampled_family_millibits_v1(4_096, 62_500), 369_305, "every check the chain bounds, leaves at scope v1's density");
    assert_eq!(sampled_family_millibits_v1(0, 1_000_000), 0);
}

#[test]
fn the_grinding_bound_golden() {
    assert_eq!(beacon_grinding_choices_bound_v1(2, 32, 1), 992, "P(32, 2): the interim live cap and k");
    assert_eq!(beacon_grinding_choices_bound_v1(3, 32, 1), 29_760);
    assert_eq!(beacon_grinding_choices_bound_v1(0, 32, 1), 1, "no beacon, no choice");
    assert_eq!(beacon_grinding_choices_bound_v1(5, 3, 1), 6, "more positions than works: every ordering of the three");
    assert_eq!(beacon_grinding_choices_bound_v1(2, 32, 4), 3_968, "four branches that can each lock");
    assert_eq!(beacon_grinding_choices_bound_v1(40, u32::MAX, 1), u128::MAX, "saturates");
    // C4 F-C4R4-08: slots refill at Final, so the interim window (120 DAA) against the OPV window (50) carries 3 turns of the
    // 32-slot live cap: 96 competing works, P(96, 2) = 9,120 lists, 14 bits — not 32 works and 10 bits.
    assert_eq!(competing_works_bound_v1(32, 120, 50), 96);
    assert_eq!(competing_works_bound_v1(32, 50, 50), 32, "one turn when the window is no longer than the refill time");
    assert_eq!(competing_works_bound_v1(32, 120, 0), u32::MAX, "a slot that refills instantly bounds nothing");
    let g = beacon_grinding_choices_bound_v1(2, competing_works_bound_v1(32, 120, 50), 1);
    assert_eq!((g, ceil_log2_v1(g)), (9_120, 14));
}

/// **The golden vectors of the effective bound.**
#[test]
fn the_effective_bound_golden_vectors() {
    // The interim onboarding conformance: two families of 2.885 bits, one repetition, three attempts, a last contributor that grinds
    // offline (2^128) — and even with the live cap's 992 choices instead: 0 effective bits. A drill, never a security value.
    assert_eq!(bits(input(vec![MilliBits(2_885), MilliBits(2_885)], 1, 2, u128::MAX, 1, 1)), EffectiveBitsV1::Bits(0));
    assert_eq!(bits(input(vec![MilliBits(2_885), MilliBits(2_885)], 1, 2, 992, 1, 1)), EffectiveBitsV1::Bits(0));
    // A production-shaped tuple: four families of 64 bits × 3 repetitions = 192; − 2 (families) − 2 (retries) − 10 (P(32,2))
    // − 20 (2^20 adaptive statements) = 158.
    assert_eq!(bits(input(vec![MilliBits(64_000); 4], 3, 2, 992, 1, 1 << 20)), EffectiveBitsV1::Bits(158));
    // Exactly at the target: 150 − 1 (two attempts) − 20 (2^20 choices) − 1 (two statements) = 128 — and one more statement breaks it.
    let at = bits(input(vec![MilliBits(50_000)], 3, 1, 1 << 20, 1, 2));
    assert_eq!(at, EffectiveBitsV1::Bits(128));
    assert!(at.meets(128));
    let below = bits(input(vec![MilliBits(50_000)], 3, 1, 1 << 20, 1, 3));
    assert_eq!(below, EffectiveBitsV1::Bits(127));
    assert!(!below.meets(128));
    // Complete families are ε = 0: no number of tries makes an enumeration miss what it enumerates.
    assert_eq!(bits(input(vec![Complete, Complete], 1, u32::MAX, u128::MAX, 9, u128::MAX)), EffectiveBitsV1::Complete);
    assert!(EffectiveBitsV1::Complete.meets(u16::MAX));
    assert_eq!(
        bits(input(vec![Complete, MilliBits(130_000)], 1, 0, 1, 1, 1)),
        EffectiveBitsV1::Bits(130),
        "only sampled families count"
    );
    // A staged beacon: every round's beacon is ground separately (β · ⌈log2 G⌉).
    assert_eq!(bits(input(vec![MilliBits(100_000)], 2, 0, 1 << 10, 3, 1)), EffectiveBitsV1::Bits(170));
    // Against a last contributor with 2^128 offline work, the chain's whole check bound spent on leaves at scope v1's density
    // still reaches 369 − 2 − 128 = 239 bits (one family; three attempts).
    assert_eq!(bits(input(vec![MilliBits(369_305)], 1, 2, u128::MAX, 1, 1)), EffectiveBitsV1::Bits(239));
    // Saturation never wraps.
    assert_eq!(bits(input(vec![MilliBits(u64::MAX)], u32::MAX, u32::MAX, u128::MAX, 1, u128::MAX)), EffectiveBitsV1::Bits(u16::MAX));
}

#[test]
fn a_statement_without_its_terms_is_refused() {
    use SoundnessRefusalV1 as R;
    let ok = || input(vec![MilliBits(1_000)], 1, 0, 1, 1, 1);
    assert_eq!(effective_false_accept_bits_v1(&EffectiveSoundnessInputV1 { relations: vec![], ..ok() }), Err(R::NoRelation));
    assert_eq!(effective_false_accept_bits_v1(&EffectiveSoundnessInputV1 { repetition_count: 0, ..ok() }), Err(R::ZeroRepetitions));
    assert_eq!(
        effective_false_accept_bits_v1(&EffectiveSoundnessInputV1 { grinding_choices_per_beacon: 0, ..ok() }),
        Err(R::ZeroGrindingChoices)
    );
    assert_eq!(effective_false_accept_bits_v1(&EffectiveSoundnessInputV1 { beacons_per_attempt: 0, ..ok() }), Err(R::ZeroBeacons));
    assert_eq!(effective_false_accept_bits_v1(&EffectiveSoundnessInputV1 { adaptive_queries: 0, ..ok() }), Err(R::ZeroQueries));
}

fn tuple(p: &PostCommitChallengePolicyV1, suite: [u8; 64], relations: Vec<RelationSoundnessV1>, g: u128, q: u128) -> ApprovedTupleV1 {
    ApprovedTupleV1 {
        checker_suite_id: suite,
        challenge_policy_id: p.id(),
        soundness_policy_id: p.soundness_policy_id,
        min_repetitions: p.repetition_count,
        relations,
        grinding_choices_per_beacon: g,
        beacons_per_attempt: 1,
        adaptive_queries: q,
    }
}

/// **Approval requires the effective bound**: a tuple in the registry whose effective bits fall below the policy's target is
/// refused; a target below 128 is refused whatever the tuple says; the interim 2-bit onboarding policy can never be approved; a
/// complete check is approved on its enumeration alone.
#[test]
fn an_approved_tuple_below_its_target_is_refused() {
    let suite = named_id("checker-suite/k2-freivalds/v1");
    let p = PostCommitChallengePolicyV1 { security_bits: 128, ..reference_policy_v1(2, 2, 120, 2, 3) };
    // 64 × 3 = 192 − 0 − 2 − 10 = 180: approved.
    approved_v1(&[tuple(&p, suite, vec![MilliBits(64_000)], 992, 1)], &suite, &p).unwrap();
    // 40 × 3 = 120 − 2 − 10 = 108 < 128: an entry in the registry, refused all the same.
    assert_eq!(
        approved_v1(&[tuple(&p, suite, vec![MilliBits(40_000)], 992, 1)], &suite, &p),
        Err(PolicyRefusalV1::BelowTarget { effective: EffectiveBitsV1::Bits(108), target: 128 })
    );
    // The last-contributor statement (G = 2^128) breaks a tuple the live cap alone would pass.
    assert!(matches!(
        approved_v1(&[tuple(&p, suite, vec![MilliBits(64_000)], u128::MAX, 1)], &suite, &p),
        Err(PolicyRefusalV1::BelowTarget { .. })
    ));
    // A statement that is not one.
    assert_eq!(
        approved_v1(&[tuple(&p, suite, vec![], 992, 1)], &suite, &p),
        Err(PolicyRefusalV1::Soundness(SoundnessRefusalV1::NoRelation))
    );
    // A target below the floor.
    let weak = PostCommitChallengePolicyV1 { security_bits: 64, ..p.clone() };
    assert_eq!(
        approved_v1(&[tuple(&weak, suite, vec![MilliBits(64_000)], 1, 1)], &suite, &weak),
        Err(PolicyRefusalV1::TargetBelowFloor { target: 64, floor: 128 })
    );
    let interim = PostCommitChallengePolicyV1 { security_bits: 2, ..reference_policy_v1(2, 2, 120, 2, 1) };
    assert!(matches!(
        approved_v1(&[tuple(&interim, suite, vec![Complete], 1, 1)], &suite, &interim),
        Err(PolicyRefusalV1::TargetBelowFloor { .. })
    ));
    // A complete check: approved by its enumeration, whatever the retries.
    let c = complete_check_policy_v1(128, 2);
    approved_v1(&[tuple(&c, suite, vec![Complete], 1, u128::MAX)], &suite, &c).unwrap();
}

/// **The complete-check policy**: valid as a complete check, no beacon, and every departure from the shape refused.
#[test]
fn the_complete_check_policy_has_one_shape_and_no_beacon() {
    let c = complete_check_policy_v1(128, 2);
    c.validate().unwrap();
    assert!(c.is_complete_check() && !c.needs_beacon());
    let sampled = reference_policy_v1(2, 2, 120, 2, 1);
    assert!(sampled.needs_beacon() && !sampled.is_complete_check());
    let shape = |f: fn(&mut PostCommitChallengePolicyV1)| {
        let mut p = complete_check_policy_v1(128, 2);
        f(&mut p);
        p.validate()
    };
    assert!(matches!(shape(|p| p.work_count_k = 2), Err(PolicyRefusalV1::CompleteCheckShape(_))));
    assert!(matches!(shape(|p| p.beacon_window_slots = 9), Err(PolicyRefusalV1::CompleteCheckShape(_))));
    assert!(matches!(shape(|p| p.repetition_count = 2), Err(PolicyRefusalV1::CompleteCheckShape(_))));
    assert!(matches!(shape(|p| p.interactive_mode = InteractiveModeV1::StagedBeacon), Err(PolicyRefusalV1::CompleteCheckShape(_))));
    assert!(matches!(shape(|p| p.soundness_policy_id = named_id("x")), Err(PolicyRefusalV1::CompleteCheckShape(_))));
    assert!(matches!(shape(|p| p.grinding_budget_policy_id = named_id("x")), Err(PolicyRefusalV1::CompleteCheckShape(_))));
    assert_eq!(
        shape(|p| p.sampling_algorithm_id = ImplementedV1::sampling()),
        Err(PolicyRefusalV1::UnknownId("sampling_algorithm_id"))
    );
    assert_eq!(shape(|p| p.security_bits = 0), Err(PolicyRefusalV1::Missing("security_bits")));
    assert_eq!(
        shape(|p| p.randomness_source_policy_id = named_id("someone-elses-randomness")),
        Err(PolicyRefusalV1::UnknownId("randomness_source_policy_id"))
    );
    // A collector asked for the beacon of a complete check refuses: there is none, by construction.
    let ctx = BeaconContextV1 {
        chain_genesis: [1; 64],
        ruleset_id: [2; 64],
        policy: c,
        subject_kind: SubjectKindV1::ModelConformance,
        commitment_root: [3; 64],
        commitment_position: 10,
        challenge_epoch: 0,
        eligible_profiles: Default::default(),
        excluded_profiles: Default::default(),
        candidate_profile_id: RootV1::Present([4; 64]),
    };
    assert_eq!(collect_work_beacon_v1(&ctx, &[], 1_000), Err(PolicyRefusalV1::NoBeacon));
    assert_eq!(collect_attributed_work_beacon_v1(&ctx, &[], 1_000), Err(PolicyRefusalV1::NoBeacon));
}
