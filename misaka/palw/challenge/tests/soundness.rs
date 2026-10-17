//! **Effective false-accept accounting, pinned** (golden vectors), its wiring into the approval check, and the complete-check
//! policy shape. Every number here is derived by hand in `docs/design/palw/verification/opv-beacon-bootstrap.md` §6–§7.

use misaka_palw_challenge::hash::named_id;
use misaka_palw_challenge::policy::{ImplementedV1, PolicyRefusalV1};
use misaka_palw_challenge::soundness::{ceil_log2_v1, sampled_family_millibits_v1};
use misaka_palw_challenge::*;

#[test]
fn cumulative_policies_sources_and_retries_do_not_reset_the_error_budget() {
    let term = |bits, opportunities| CumulativeFailureTermV1 { per_opportunity: EffectiveBitsV1::Bits(bits), opportunities };
    // Two policy versions with 2^20 subjects each, three attempts per subject.
    // Per-attempt algebra is 152 bits: count loss 22, policy union loss 1.
    let attempts = 3 * (1 << 20);
    assert_eq!(cumulative_failure_bits_v1(&[term(152, attempts); 2]), Ok(EffectiveBitsV1::Bits(129)));
    // A 128-bit source assumption per window becomes only 108 bits over 2^20
    // windows, then the three-term union costs another two bits. An algebraic
    // surplus cannot make an unreviewed source floor into a 128-bit guarantee.
    let terms = [term(152, attempts), term(152, attempts), term(128, 1 << 20)];
    assert_eq!(cumulative_failure_bits_v1(&terms), Ok(EffectiveBitsV1::Bits(106)));
    let mut reordered = terms;
    reordered.reverse();
    assert_eq!(cumulative_failure_bits_v1(&terms), cumulative_failure_bits_v1(&reordered));
    assert_eq!(cumulative_failure_bits_v1(&[term(152, attempts), term(0, 1)]), Ok(EffectiveBitsV1::Bits(0)));
}

#[test]
fn cumulative_exact_events_and_counter_edges_never_invent_security() {
    use EffectiveBitsV1::{Bits, Complete};
    let term = |per_opportunity, opportunities| CumulativeFailureTermV1 { per_opportunity, opportunities };
    assert_eq!(cumulative_failure_bits_v1(&[]), Err(SoundnessRefusalV1::NoFailureTerm));
    assert_eq!(cumulative_failure_bits_v1(&[term(Complete, 0)]), Err(SoundnessRefusalV1::ZeroOpportunities));
    assert_eq!(cumulative_failure_bits_v1(&[term(Complete, u128::MAX)]), Ok(Complete));
    assert_eq!(cumulative_failure_bits_v1(&[term(Bits(256), u128::MAX)]), Ok(Bits(128)));
    assert_eq!(cumulative_failure_bits_v1(&[term(Bits(256), u128::MAX); 2]), Ok(Bits(127)));
    assert_eq!(cumulative_failure_bits_v1(&[term(Bits(1), u128::MAX)]), Ok(Bits(0)));
    assert_eq!(cumulative_failure_bits_v1(&[term(Complete, u128::MAX), term(Bits(128), 1)]), Ok(Bits(128)));
}

#[test]
fn cumulative_bound_dominates_an_exact_dyadic_union_without_independence() {
    // Independent oracle: compute Σ n_i / 2^b_i on a common 2^16 denominator.
    // Our integer bound must cover even the full sum (before capping it at 1).
    for a in 1..=16u16 {
        for b in 1..=16u16 {
            for na in 1..=9u128 {
                for nb in 1..=9u128 {
                    let terms = [
                        CumulativeFailureTermV1 { per_opportunity: EffectiveBitsV1::Bits(a), opportunities: na },
                        CumulativeFailureTermV1 { per_opportunity: EffectiveBitsV1::Bits(b), opportunities: nb },
                    ];
                    let EffectiveBitsV1::Bits(result) = cumulative_failure_bits_v1(&terms).unwrap() else { panic!("nonzero errors") };
                    let exact_numerator = ((na << (16 - a)) + (nb << (16 - b))).min(1 << 16);
                    assert!(1u128 << (16 - result) >= exact_numerator, "{a}/{na}, {b}/{nb}: {result}");
                }
            }
        }
    }
}

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
/// complete enumeration still needs an explicit binding/source/coverage review.
#[test]
fn an_approved_tuple_below_its_target_is_refused() {
    let suite = named_id("checker-suite/k2-freivalds/v1");
    let p = PostCommitChallengePolicyV1 { security_bits: 128, ..reference_policy_v1(2, 2, 120, 2, 3) };
    // 64 × 3 = 192 − 0 − 2 − 10 = 180: approved.
    assert_eq!(
        approved_v1(&[tuple(&p, suite, vec![MilliBits(64_000)], 992, 1)], &suite, &p),
        Err(PolicyRefusalV1::Missing("full binding/source/coverage review"))
    );
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
    assert_eq!(
        approved_v1(&[tuple(&c, suite, vec![Complete], 1, u128::MAX)], &suite, &c),
        Err(PolicyRefusalV1::Missing("full binding/source/coverage review"))
    );
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

fn reviewed(policy: &PostCommitChallengePolicyV1, algebraic: ApprovedTupleV1) -> ReviewedTupleV2 {
    ReviewedTupleV2 {
        policy: policy.clone(),
        algebraic,
        binding: ReviewedFailureBoundV2::Bits(256),
        source: ReviewedFailureBoundV2::Bits(256),
        coverage: ReviewedFailureBoundV2::Exact,
        review_root: named_id("review/synthetic-test-fixture-only/v2"),
    }
}

#[test]
fn full_approval_charges_binding_and_source_over_the_same_horizon() {
    let suite = named_id("checker-suite/synthetic-review/v2");
    let p = complete_check_policy_v1(128, 0);
    let mut r = reviewed(&p, tuple(&p, suite, vec![Complete], 1, 2));
    r.source = ReviewedFailureBoundV2::Exact;
    r.binding = ReviewedFailureBoundV2::Bits(128);
    assert_eq!(
        approved_v2(std::slice::from_ref(&r), &suite, &p),
        Err(PolicyRefusalV1::BelowTarget { effective: EffectiveBitsV1::Bits(127), target: 128 }),
        "complete enumeration does not make two 128-bit commitments exact"
    );
    r.binding = ReviewedFailureBoundV2::Bits(129);
    approved_v2(&[r], &suite, &p).unwrap();

    // 152-bit relation, two retries, four grinding outputs and four subjects:
    // loss = 2 + 2 + 2 = 6. Three non-exact terms union costs two more bits.
    let p = PostCommitChallengePolicyV1 { security_bits: 128, ..reference_policy_v1(2, 2, 120, 2, 1) };
    let mut r = reviewed(&p, tuple(&p, suite, vec![MilliBits(152_000)], 4, 4));
    r.binding = ReviewedFailureBoundV2::Bits(136);
    r.source = ReviewedFailureBoundV2::Bits(136);
    approved_v2(std::slice::from_ref(&r), &suite, &p).unwrap();
    r.source = ReviewedFailureBoundV2::Bits(135);
    assert_eq!(
        approved_v2(&[r], &suite, &p),
        Err(PolicyRefusalV1::BelowTarget { effective: EffectiveBitsV1::Bits(127), target: 128 })
    );
}

#[test]
fn full_approval_never_substitutes_algebra_for_unreviewed_failure_events() {
    let suite = named_id("checker-suite/synthetic-review/v2");
    let p = PostCommitChallengePolicyV1 { security_bits: 128, ..reference_policy_v1(2, 2, 120, 2, 3) };
    let r = reviewed(&p, tuple(&p, suite, vec![MilliBits(128_000)], 1, 1));
    approved_v2(std::slice::from_ref(&r), &suite, &p).unwrap();
    for event in 0..3 {
        let mut changed = r.clone();
        match event {
            0 => changed.binding = ReviewedFailureBoundV2::Unknown,
            1 => changed.source = ReviewedFailureBoundV2::Unknown,
            _ => changed.coverage = ReviewedFailureBoundV2::Unknown,
        }
        assert_eq!(
            approved_v2(&[changed], &suite, &p),
            Err(PolicyRefusalV1::BelowTarget { effective: EffectiveBitsV1::Bits(0), target: 128 })
        );
    }
    let mut missing = r.clone();
    missing.review_root = [0; 64];
    assert_eq!(approved_v2(&[missing], &suite, &p), Err(PolicyRefusalV1::Missing("full soundness review root")));
    assert!(shipped_registry_v2().is_empty());
    assert_eq!(approved_v2(&[], &suite, &p), Err(PolicyRefusalV1::NotApproved));
    assert_eq!(
        approved_v1(std::slice::from_ref(&r.algebraic), &suite, &p),
        Err(PolicyRefusalV1::Missing("full binding/source/coverage review"))
    );
    let old_bytes = borsh::to_vec(&r.algebraic).unwrap();
    let restored: ApprovedTupleV1 = borsh::from_slice(&old_bytes).unwrap();
    assert_eq!(restored, r.algebraic);
    assert!(borsh::from_slice::<ReviewedTupleV2>(&old_bytes).is_err(), "old bytes do not invent a full review");
}

#[test]
fn every_staged_beacon_and_scope_failure_pays_retries_and_adaptive_grinding() {
    let suite = named_id("checker-suite/staged-review/v2");
    let mut p = PostCommitChallengePolicyV1 { security_bits: 128, ..reference_policy_v1(2, 2, 120, 2, 3) };
    p.interactive_mode = InteractiveModeV1::StagedBeacon;
    p.transcript_transform_id = ImplementedV1::transcript_staged_beacon();
    let mut r = reviewed(&p, tuple(&p, suite, vec![MilliBits(128_000)], 2, 4));
    r.algebraic.beacons_per_attempt = 3;
    // Strategy loss 2 retries + 3 rounds grinding + 2 adaptive = 7.
    // Source-window union adds ceil(log2 3) = 2; four-event union adds 2.
    r.binding = ReviewedFailureBoundV2::Bits(137);
    r.source = ReviewedFailureBoundV2::Bits(139);
    r.coverage = ReviewedFailureBoundV2::Bits(137);
    approved_v2(std::slice::from_ref(&r), &suite, &p).unwrap();
    for event in 0..3 {
        let mut changed = r.clone();
        match event {
            0 => changed.binding = ReviewedFailureBoundV2::Bits(136),
            1 => changed.source = ReviewedFailureBoundV2::Bits(138),
            _ => changed.coverage = ReviewedFailureBoundV2::Bits(136),
        }
        assert_eq!(
            approved_v2(&[changed], &suite, &p),
            Err(PolicyRefusalV1::BelowTarget { effective: EffectiveBitsV1::Bits(127), target: 128 })
        );
    }
}

#[test]
fn switching_policy_or_suite_does_not_reset_the_registrys_cumulative_budget() {
    let suite = named_id("checker-suite/synthetic-review/v2");
    let first_policy = complete_check_policy_v1(128, 0);
    let second_policy =
        PostCommitChallengePolicyV1 { retention_policy_id: named_id("retention/synthetic-upgrade/v2"), ..first_policy.clone() };
    let mut first = reviewed(&first_policy, tuple(&first_policy, suite, vec![Complete], 1, 1));
    first.source = ReviewedFailureBoundV2::Exact;
    first.binding = ReviewedFailureBoundV2::Bits(128);
    let mut second = reviewed(&second_policy, tuple(&second_policy, named_id("checker-suite/second/v2"), vec![Complete], 1, 1));
    second.source = ReviewedFailureBoundV2::Exact;
    second.binding = ReviewedFailureBoundV2::Bits(128);
    approved_v2(std::slice::from_ref(&first), &suite, &first_policy).unwrap();
    assert_eq!(
        approved_v2(&[first.clone(), second.clone()], &suite, &first_policy),
        Err(PolicyRefusalV1::BelowTarget { effective: EffectiveBitsV1::Bits(127), target: 128 }),
        "both authorized suites/policies belong to the same cumulative failure union"
    );
    first.binding = ReviewedFailureBoundV2::Bits(129);
    second.binding = ReviewedFailureBoundV2::Bits(129);
    approved_v2(&[first.clone(), second.clone()], &suite, &first_policy).unwrap();
    approved_v2(&[second.clone(), first.clone()], &suite, &first_policy).unwrap();
    second.source = ReviewedFailureBoundV2::Unknown;
    assert_eq!(
        approved_v2(&[first.clone(), second.clone()], &suite, &first_policy),
        Err(PolicyRefusalV1::BelowTarget { effective: EffectiveBitsV1::Bits(0), target: 128 }),
        "an unreviewed authorized source is not hidden by choosing another policy"
    );
    second.policy.security_bits = 129;
    assert_eq!(
        approved_v2(&[first, second], &suite, &first_policy),
        Err(PolicyRefusalV1::NotApproved),
        "a reviewed descriptor substitution is refused"
    );
}
