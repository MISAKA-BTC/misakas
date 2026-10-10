//! **The sealed-source beacon v3** (`docs/design/palw/opv-beacon-bootstrap.md` §6.3) against the attacks that break v2 (SOUND SG-01,
//! SG-01a): the last contributor, first-`k` capture, withholding, abandonment after the reveal, censorship of a reveal, early
//! reveals, fork choice — and its accounting (`G = F`, `ε_src`).

use std::collections::BTreeSet;

use misaka_palw_challenge::hash::{Digest, object_id};
use misaka_palw_challenge::policy::{ImplementedV1, PolicyRefusalV1};
use misaka_palw_challenge::sealed::*;
use misaka_palw_challenge::*;

const SRC_A: Digest = [0xA1; 64];
const SRC_B: Digest = [0xA2; 64];
const CANDIDATE: Digest = [0xCA; 64];

/// k = 2, anchor delay 2, seal window W = 20, D = 2: S = 102, seals in [102, 122), reveals in [122, 142).
fn ctx() -> BeaconContextV1 {
    BeaconContextV1 {
        chain_genesis: [0x11; 64],
        ruleset_id: [0x22; 64],
        policy: sealed_source_policy_v3(2, 2, 20, 2, 1),
        subject_kind: SubjectKindV1::ModelConformance,
        commitment_root: [0xC0; 64],
        commitment_position: 100,
        challenge_epoch: 0,
        eligible_profiles: [SRC_A, SRC_B].into_iter().collect(),
        excluded_profiles: [CANDIDATE].into_iter().collect(),
        candidate_profile_id: RootV1::Present(CANDIDATE),
    }
}

const TIP: u64 = 10_000;

fn event(profile: Digest, job: u64, revealed: u64, settled: u64) -> WorkFinalEventV1 {
    WorkFinalEventV1 {
        kind: WorkSourceKindV1::RealUsefulWork,
        source_profile_id: profile,
        canonical_work_id: object_id(b"test/canonical-work", &(profile, job)),
        execution_commitment: [0xEE; 64],
        accepted_position: revealed,
        settlement_position: settled,
        occurrence_index: 0,
        claim_final: true,
        da_satisfied: true,
        validity_independent: true,
        depends_on_profiles: vec![],
        final_path: FinalPathV1::PanelIndependent,
    }
}

/// A seal by `producer` on `profile`, at `sealed`, revealed at `revealed` with `salt` and Final at `revealed + 50`.
fn sealed(profile: Digest, job: u64, producer: u8, sealed: u64, revealed: Option<u64>, salt: u64) -> SealedSourceV3 {
    SealedSourceV3 {
        source_profile_id: profile,
        attribution: SourceAttributionV1 { producer_id: [producer; 64], consumer_id: RootV1::Absent },
        seal: object_id(b"test/seal", &(profile, job, producer, salt)),
        seal_position: sealed,
        reveal: revealed.map(|r| SealRevealV3 {
            reveal_position: r,
            salt: object_id(b"test/salt", &salt),
            fate: SourceFateV3::Final(event(profile, job, r, r + 50)),
        }),
    }
}

fn locked(c: &BeaconContextV1, seals: &[SealedSourceV3]) -> Option<Digest> {
    match collect_sealed_work_beacon_v3(c, seals, TIP).unwrap() {
        SealedBeaconStateV3::Locked(b) => Some(b.output),
        _ => None,
    }
}

/// **The last contributor is gone.** Under v2 one attacker job ground offline gave 2^12 distinct beacons (`grinding.rs`). Under
/// v3 the attacker must SEAL before the window closes, while the honest salt is still hidden: whatever it seals, the output it gets
/// is a function of a salt it never saw. After the reveals its only move is to withhold — and every withholding vetoes: among all
/// 2^3 subsets of its three sealed sources, exactly one (reveal everything) locks. Choices per beacon: 1.
#[test]
fn after_the_reveals_the_adversary_can_only_veto_never_choose() {
    let c = ctx();
    let honest = sealed(SRC_A, 1, 1, 103, Some(125), 0x5EC2E7);
    let attacker = |mask: u8| -> Vec<SealedSourceV3> {
        (0..3u8).map(|i| sealed(SRC_B, 10 + i as u64, 9 + i, 110 + i as u64, (mask & (1 << i) != 0).then_some(130), 77)).collect()
    };
    let mut outputs = BTreeSet::new();
    let mut vetoed = 0;
    for mask in 0..8u8 {
        let mut seals = vec![honest.clone()];
        seals.extend(attacker(mask));
        match collect_sealed_work_beacon_v3(&c, &seals, TIP).unwrap() {
            SealedBeaconStateV3::Locked(b) => {
                outputs.insert(b.output);
            }
            SealedBeaconStateV3::Vetoed { withheld, failed: 0 } => {
                assert_eq!(withheld, 3 - mask.count_ones());
                vetoed += 1;
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!((outputs.len(), vetoed), (1, 7), "one lockable output; every withholding is a veto");
    assert_eq!(sealed_beacon_grinding_choices_v3(1), 1);
    // And the honest salt moves the output the attacker committed to blind.
    let mut seals = vec![sealed(SRC_A, 1, 1, 103, Some(125), 0x0DD)];
    seals.extend(attacker(7));
    let other = locked(&c, &seals).unwrap();
    assert!(!outputs.contains(&other), "the honest salt, unseen at sealing time, decides the output");
}

/// **No first-`k` capture.** The attacker seals the first ten positions of the window with ten bonds; one honest seal comes last.
/// Every qualifying seal is mixed, so the honest salt is in the output — and withholding any of the ten vetoes, never drops.
#[test]
fn sealing_first_or_sealing_many_never_pushes_an_honest_seal_out() {
    let c = ctx();
    let mut seals: Vec<SealedSourceV3> =
        (0..10u8).map(|i| sealed(SRC_B, 100 + i as u64, 20 + i, 102 + i as u64, Some(126), 7)).collect();
    let honest = |salt| sealed(SRC_A, 1, 1, 121, Some(127), salt);
    seals.push(honest(1));
    let a = locked(&c, &seals).unwrap();
    *seals.last_mut().unwrap() = honest(2);
    let b = locked(&c, &seals).unwrap();
    assert_ne!(a, b, "the last honest seal is mixed whatever precedes it");
    match collect_sealed_work_beacon_v3(&c, &seals, TIP).unwrap() {
        SealedBeaconStateV3::Locked(b) => assert_eq!(b.sources.len(), 11, "all eleven are mixed"),
        other => panic!("{other:?}"),
    }
}

/// **Censorship of an honest reveal is a veto, never an exclusion**: an honest seal in the window whose reveal is kept out of the
/// reveal window (late or never) stops the beacon from locking at all.
#[test]
fn a_censored_honest_reveal_vetoes_and_is_never_dropped() {
    let c = ctx();
    let attacker = sealed(SRC_B, 2, 9, 104, Some(123), 7);
    for reveal in [None, Some(142), Some(500)] {
        let seals = vec![sealed(SRC_A, 1, 1, 103, reveal, 1), attacker.clone()];
        assert_eq!(
            collect_sealed_work_beacon_v3(&c, &seals, TIP).unwrap(),
            SealedBeaconStateV3::Vetoed { withheld: 1, failed: 0 },
            "reveal {reveal:?}: the reveal window is [122, 142)"
        );
    }
}

/// **Abandonment after the reveal is a veto too**: a mixed source that is convicted, unavailable or timed out — or reaches a Final
/// that does not stand as a source — vetoes; one still undecided keeps the beacon settling.
#[test]
fn a_mixed_source_that_fails_after_its_reveal_vetoes() {
    let c = ctx();
    let mut seals = vec![sealed(SRC_A, 1, 1, 103, Some(125), 1), sealed(SRC_B, 2, 2, 104, Some(126), 2)];
    seals[1].reveal.as_mut().unwrap().fate = SourceFateV3::Failed;
    assert_eq!(collect_sealed_work_beacon_v3(&c, &seals, TIP).unwrap(), SealedBeaconStateV3::Vetoed { withheld: 0, failed: 1 });
    let mut not_standing = event(SRC_B, 2, 126, 176);
    not_standing.claim_final = false;
    seals[1].reveal.as_mut().unwrap().fate = SourceFateV3::Final(not_standing);
    assert_eq!(collect_sealed_work_beacon_v3(&c, &seals, TIP).unwrap(), SealedBeaconStateV3::Vetoed { withheld: 0, failed: 1 });
    seals[1].reveal.as_mut().unwrap().fate = SourceFateV3::Live;
    assert_eq!(collect_sealed_work_beacon_v3(&c, &seals, TIP).unwrap(), SealedBeaconStateV3::Settling { mixed: 2, finals: 1 });
}

/// **A reveal while sealing is still open is public too early**: not a source, and not a veto. A seal outside the window, on an
/// ineligible or excluded profile, or a producer's second seal, is not mixed — and its withholding vetoes nothing.
#[test]
fn early_reveals_late_seals_foreign_profiles_and_second_seals_are_not_mixed() {
    let c = ctx();
    let base = vec![sealed(SRC_A, 1, 1, 103, Some(125), 1), sealed(SRC_B, 2, 2, 104, Some(126), 2)];
    let reference = locked(&c, &base).unwrap();
    let noise = [
        sealed(SRC_A, 3, 3, 105, Some(110), 3), // revealed at 110 < 122: public while sealing was open
        sealed(SRC_A, 4, 4, 101, None, 4),      // sealed before S = 102
        sealed(SRC_A, 5, 5, 122, None, 5),      // sealed at S + W
        sealed([0x77; 64], 6, 6, 106, None, 6), // a profile not eligible at the commitment
        sealed(CANDIDATE, 7, 7, 106, None, 7),  // the candidate itself
        sealed(SRC_B, 8, 1, 107, None, 8),      // producer 1's second seal
    ];
    let mut all = base.clone();
    all.extend(noise);
    assert_eq!(locked(&c, &all), Some(reference), "none of them is mixed, none vetoes");
    // A known consumer funds one source only.
    let mut paid = base.clone();
    paid[0].attribution.consumer_id = RootV1::Present([5; 64]);
    paid[1].attribution.consumer_id = RootV1::Present([5; 64]);
    assert_eq!(
        collect_sealed_work_beacon_v3(&c, &paid, TIP).unwrap(),
        SealedBeaconStateV3::Unavailable { mixed: 1, need: 2 },
        "one payer, one source"
    );
}

/// **The phases**: sealing until `S + W` (and unavailability known right then), revealing until `S + 2W`, settling until every
/// mixed source is Final, a candidate until the last settlement is `D` deep, then locked — and a seed from it.
#[test]
fn the_phases_from_sealing_to_lock_and_a_seed() {
    let c = ctx();
    let seals = vec![sealed(SRC_A, 1, 1, 103, Some(125), 1), sealed(SRC_B, 2, 2, 104, Some(130), 2)];
    let at = |tip| collect_sealed_work_beacon_v3(&c, &seals, tip).unwrap();
    assert_eq!(at(103), SealedBeaconStateV3::Sealing { sealed: 1, need: 2 });
    assert_eq!(at(121), SealedBeaconStateV3::Sealing { sealed: 2, need: 2 });
    assert_eq!(at(126), SealedBeaconStateV3::Revealing { mixed: 2, revealed: 1 });
    assert_eq!(at(141), SealedBeaconStateV3::Revealing { mixed: 2, revealed: 2 });
    assert_eq!(at(142), SealedBeaconStateV3::Settling { mixed: 2, finals: 0 });
    assert_eq!(at(176), SealedBeaconStateV3::Settling { mixed: 2, finals: 1 });
    assert_eq!(at(180), SealedBeaconStateV3::Candidate { mixed: 2, lock_position: 182 });
    let SealedBeaconStateV3::Locked(b) = at(182) else { panic!("locked at 182") };
    assert_eq!(b.lock_position, 182);
    assert!(verify_sealed_work_beacon_v3(&c, b.beacon(), &seals, 182).is_ok());
    let subject = ChallengeSubjectV1 {
        chain_genesis: c.chain_genesis,
        ruleset_id: c.ruleset_id,
        challenge_policy_id: c.policy.id(),
        subject_kind: c.subject_kind,
        subject_id: CANDIDATE,
        kernel_id: RootV1::Absent,
        verification_plan_root: RootV1::Absent,
        program_root: RootV1::Absent,
        artifact_root: RootV1::Absent,
        tokenizer_or_schema_root: RootV1::Absent,
        layout_root: RootV1::Absent,
        input_root: RootV1::Absent,
        state_root: RootV1::Absent,
        constraint_root: RootV1::Absent,
        commitment_root: c.commitment_root,
    };
    challenge_seed_v1(&c, &subject, &b).expect("a v3 beacon seeds like any derived beacon");
    // Too few producers: unavailable as soon as the seal window closes.
    let one = vec![seals[0].clone()];
    assert_eq!(collect_sealed_work_beacon_v3(&c, &one, 122).unwrap(), SealedBeaconStateV3::Unavailable { mixed: 1, need: 2 });
}

/// **Fork choice** is the one remaining lever (`F`): two branches with different reveals lock different beacons, and a beacon of one
/// is refused on the other.
#[test]
fn a_v3_beacon_of_one_branch_is_refused_on_another() {
    let c = ctx();
    let a = vec![sealed(SRC_A, 1, 1, 103, Some(125), 1), sealed(SRC_B, 2, 2, 104, Some(126), 2)];
    let b = vec![sealed(SRC_A, 1, 1, 103, Some(125), 1), sealed(SRC_B, 2, 3, 104, Some(126), 9)];
    let SealedBeaconStateV3::Locked(on_a) = collect_sealed_work_beacon_v3(&c, &a, TIP).unwrap() else { panic!() };
    assert!(verify_sealed_work_beacon_v3(&c, on_a.beacon(), &a, TIP).is_ok());
    assert!(verify_sealed_work_beacon_v3(&c, on_a.beacon(), &b, TIP).is_err());
}

/// **The collectors do not cross**: v2's accumulator refuses a v3 policy and v3 refuses a v2 one; a v3 policy reads the distinct
/// rule only, and has the sampled policy's shape otherwise.
#[test]
fn v2_and_v3_policies_have_their_own_collectors() {
    let v3 = ctx();
    assert!(v3.policy.validate().is_ok() && v3.policy.is_sealed_source() && v3.policy.needs_beacon());
    assert!(matches!(collect_attributed_work_beacon_v1(&v3, &[], TIP), Err(PolicyRefusalV1::WrongCollector(_))));
    assert!(matches!(collect_work_beacon_v1(&v3, &[], TIP), Err(PolicyRefusalV1::WrongCollector(_))));
    let mut v2 = ctx();
    v2.policy = PostCommitChallengePolicyV1 {
        source_eligibility_policy_id: ImplementedV1::source_eligibility_distinct(),
        ..reference_policy_v1(2, 2, 20, 2, 1)
    };
    assert!(matches!(collect_sealed_work_beacon_v3(&v2, &[], TIP), Err(PolicyRefusalV1::WrongCollector(_))));
    let mut plain = v3.policy.clone();
    plain.source_eligibility_policy_id = ImplementedV1::source_eligibility();
    assert_eq!(plain.validate(), Err(PolicyRefusalV1::UnknownId("source_eligibility_policy_id")), "v3 mixes distinct producers");
    assert_ne!(v3.policy.id(), v2.policy.id());
}

/// **The accounting**: `G = F`; `ε_src` golden vectors — the interim window (W = 40, δ = 10, one block per DAA) against a ½ share is
/// 30 bits, and 128 bits needs `W − δ ≥ 128` blocks (so the ledger's seal TTL ≥ 2W); against ⅓, 81 blocks give 128. The combined
/// bound charges one bit for the union of the two failure events.
#[test]
fn the_v3_accounting_golden_vectors() {
    assert_eq!(sealed_beacon_grinding_choices_v3(0), 1);
    assert_eq!(sealed_beacon_grinding_choices_v3(4), 4);
    assert_eq!(sealed_source_censorship_bits_v3(40, 10, 1, 1_000), 30);
    assert_eq!(sealed_source_censorship_bits_v3(138, 10, 1, 1_000), 128);
    assert_eq!(sealed_source_censorship_bits_v3(91, 10, 1, 1_585), 128);
    assert_eq!(sealed_source_censorship_bits_v3(5, 10, 1, 1_000), 0, "a window shorter than the merge delay protects nothing");
    assert_eq!(sealed_source_censorship_bits_v3(u64::MAX, 0, u64::MAX, u64::MAX), u64::MAX, "saturates");
    use EffectiveBitsV1::{Bits, Complete};
    assert_eq!(combine_failure_bits_v1(Bits(158), Bits(130)), Bits(129));
    assert_eq!(combine_failure_bits_v1(Complete, Bits(30)), Bits(30));
    assert_eq!(combine_failure_bits_v1(Complete, Complete), Complete);
    assert_eq!(combine_failure_bits_v1(Bits(0), Bits(0)), Bits(0));
    // A v3 sampled conformance: 4 families × 64 bits × 3 repetitions, three attempts (vetoes included), G = F = 1, Q = 2^10: 178
    // algorithmic bits; against the interim window's 30 bits of ε_src the claim is 29 — and with W − δ = 130 at ½, 129.
    let alg = effective_false_accept_bits_v1(&EffectiveSoundnessInputV1 {
        relations: vec![RelationSoundnessV1::MilliBits(64_000); 4],
        repetition_count: 3,
        retry_limit: 2,
        grinding_choices_per_beacon: sealed_beacon_grinding_choices_v3(1),
        beacons_per_attempt: 1,
        adaptive_queries: 1 << 10,
    })
    .unwrap();
    assert_eq!(alg, Bits(178));
    assert_eq!(combine_failure_bits_v1(alg, Bits(sealed_source_censorship_bits_v3(40, 10, 1, 1_000) as u16)), Bits(29));
    assert_eq!(combine_failure_bits_v1(alg, Bits(sealed_source_censorship_bits_v3(140, 10, 1, 1_000) as u16)), Bits(129));
}
