//! **Grinding attacks on the PALW Work Beacon** (`docs/design/palw/opv-beacon-bootstrap.md` §6), each lever exercised as an attack
//! and its bound checked: output selection by a last contributor, the distinct source rules against one party filling every position,
//! the per-class cap, withholding after the lock (a counted veto), Final timing, fork choice and precomputation.

use std::collections::BTreeSet;

use misaka_palw_challenge::hash::{Digest, object_id};
use misaka_palw_challenge::lifecycle::{OnboardingRefusalV1, OnboardingStepV1};
use misaka_palw_challenge::policy::ImplementedV1;
use misaka_palw_challenge::*;

const SRC_A: Digest = [0xA1; 64];
const SRC_B: Digest = [0xA2; 64];
const CANDIDATE: Digest = [0xCA; 64];

fn policy(rule: Digest) -> PostCommitChallengePolicyV1 {
    PostCommitChallengePolicyV1 { source_eligibility_policy_id: rule, ..reference_policy_v1(2, 2, 40, 2, 1) }
}

fn ctx(rule: Digest) -> BeaconContextV1 {
    BeaconContextV1 {
        chain_genesis: [0x11; 64],
        ruleset_id: [0x22; 64],
        policy: policy(rule),
        subject_kind: SubjectKindV1::ModelConformance,
        commitment_root: [0xC0; 64],
        commitment_position: 100,
        challenge_epoch: 0,
        eligible_profiles: [SRC_A, SRC_B].into_iter().collect(),
        excluded_profiles: [CANDIDATE].into_iter().collect(),
        candidate_profile_id: RootV1::Present(CANDIDATE),
    }
}

/// An OPV Final: its work identity is `H(class, job)`, so a job nonce is free to grind; its execution commitment is a function of the
/// prompt (a deterministic program), the same for every nonce.
fn work(profile: Digest, nonce: u64, producer: u8, consumer: Option<u8>, accepted: u64, settled: u64) -> AttributedWorkV1 {
    AttributedWorkV1 {
        event: WorkFinalEventV1 {
            kind: WorkSourceKindV1::RealUsefulWork,
            source_profile_id: profile,
            canonical_work_id: object_id(b"test/canonical-work", &(profile, nonce)),
            execution_commitment: [0xEE; 64],
            accepted_position: accepted,
            settlement_position: settled,
            occurrence_index: 0,
            claim_final: true,
            da_satisfied: true,
            validity_independent: true,
            depends_on_profiles: vec![],
            final_path: FinalPathV1::PanelIndependent,
        },
        attribution: SourceAttributionV1 {
            producer_id: [producer; 64],
            consumer_id: consumer.map(|c| RootV1::Present([c; 64])).unwrap_or(RootV1::Absent),
        },
    }
}

fn output(c: &BeaconContextV1, works: &[AttributedWorkV1]) -> Option<Digest> {
    match collect_attributed_work_beacon_v1(c, works, 1_000).unwrap() {
        WorkBeaconStateV1::Locked(b) => Some(b.output),
        _ => None,
    }
}

/// **The last contributor.** One honest source has settled; the attacker posts ONE job whose nonce it grinds offline (a hash per
/// try), commits it and lets it settle second. Every nonce is a distinct beacon output, so the attacker's choices are its offline
/// work — the live cap, the distinct rules and every on-chain count leave it untouched. Here 2^12 tries hit an 8-bit target with
/// near certainty: one source buys ~log2(tries) bits of bias. This is why the interim policy is accounted with G = 2^128 and why a
/// sealed-source beacon is a design blocker for any sampled approval.
#[test]
fn one_last_contribution_steers_the_beacon_with_offline_work() {
    let c = ctx(ImplementedV1::source_eligibility_distinct());
    let honest = work(SRC_A, 1, 1, Some(1), 103, 110);
    let mut outputs = BTreeSet::new();
    let mut hit = None;
    for nonce in 0..(1u64 << 12) {
        let attacker = work(SRC_B, 1_000 + nonce, 9, Some(9), 105, 111);
        let out = output(&c, &[honest.clone(), attacker]).expect("two distinct sources lock");
        if hit.is_none() && out[0] == 0 {
            hit = Some(nonce);
        }
        outputs.insert(out);
    }
    assert_eq!(outputs.len(), 1 << 12, "every ground nonce is its own beacon: choices = offline tries");
    assert!(hit.is_some(), "an 8-bit target is hit within 2^12 tries");
    assert!(
        outputs.len() as u128 > beacon_grinding_choices_bound_v1(1, 32, 1),
        "the live-cap bound does not cover a free input chosen offline"
    );
}

/// **Work concentration under the distinct rule**: one producer bond cannot fill both positions (its second work is passed over),
/// and one consumer (job payer) cannot either where the consumer is known; without attribution (the plain rule) one bond fills both.
#[test]
fn the_distinct_rule_stops_one_party_filling_every_position() {
    let plain = ctx(ImplementedV1::source_eligibility());
    let distinct = ctx(ImplementedV1::source_eligibility_distinct());
    let same_bond = [work(SRC_A, 1, 7, None, 103, 110), work(SRC_B, 2, 7, None, 104, 111)];
    assert!(output(&plain, &same_bond).is_some(), "the plain rule: one bond owns the beacon");
    assert!(output(&distinct, &same_bond).is_none(), "the distinct rule: the second work of the bond is not a source");
    assert!(matches!(
        collect_attributed_work_beacon_v1(&distinct, &same_bond, 1_000).unwrap(),
        WorkBeaconStateV1::Unavailable { have: 1, need: 2 }
    ));
    // A third work by another bond takes the second place.
    let mut with_other = same_bond.to_vec();
    with_other.push(work(SRC_B, 3, 8, None, 105, 112));
    let locked = match collect_attributed_work_beacon_v1(&distinct, &with_other, 1_000).unwrap() {
        WorkBeaconStateV1::Locked(b) => b,
        other => panic!("{other:?}"),
    };
    assert_eq!(locked.sources[1].canonical_work_id, with_other[2].event.canonical_work_id);
    // Distinct bonds but one known consumer: the same.
    let same_payer = [work(SRC_A, 1, 1, Some(5), 103, 110), work(SRC_B, 2, 2, Some(5), 104, 111)];
    assert!(output(&distinct, &same_payer).is_none(), "one payer cannot fund every source");
    // An unknown consumer (Absent) constrains nothing ("where possible").
    let unknown = [work(SRC_A, 1, 1, None, 103, 110), work(SRC_B, 2, 2, None, 104, 111)];
    assert!(output(&distinct, &unknown).is_some());
    // The plain collector refuses a policy whose rule needs attribution.
    let bare: Vec<WorkFinalEventV1> = unknown.iter().map(|w| w.event.clone()).collect();
    assert!(collect_work_beacon_v1(&distinct, &bare, 1_000).is_err());
}

/// **The per-class cap**: at most ⌈k/2⌉ sources from one class, so one (tiny, cheap) class cannot own the beacon.
#[test]
fn the_class_cap_needs_a_second_class() {
    let capped = ctx(ImplementedV1::source_eligibility_distinct_class_capped());
    let one_class = [work(SRC_A, 1, 1, None, 103, 110), work(SRC_A, 2, 2, None, 104, 111)];
    assert!(output(&capped, &one_class).is_none(), "two works of one class: one counts");
    let two_classes = [work(SRC_A, 1, 1, None, 103, 110), work(SRC_A, 2, 2, None, 104, 111), work(SRC_B, 3, 3, None, 105, 112)];
    assert!(output(&capped, &two_classes).is_some());
    assert_eq!(SourceRuleV1::DistinctClassCapped.per_profile_cap(2), 1);
    assert_eq!(SourceRuleV1::DistinctClassCapped.per_profile_cap(5), 3);
    assert_eq!(SourceRuleV1::Distinct.per_profile_cap(5), 5);
}

/// **Final timing and selection among FIXED works**: with every input fixed, ordering and choosing which works settle first yields
/// at most `P(J, k)` beacons — every one of them reached here, none beyond.
#[test]
fn selection_and_timing_among_fixed_works_stay_inside_the_live_cap_bound() {
    let c = ctx(ImplementedV1::source_eligibility_distinct());
    let j = 5u64;
    let mut outputs = BTreeSet::new();
    for first in 0..j {
        for second in 0..j {
            if first == second {
                continue;
            }
            let a = work(if first % 2 == 0 { SRC_A } else { SRC_B }, first, first as u8 + 1, None, 103, 110);
            let b = work(if second % 2 == 0 { SRC_A } else { SRC_B }, second, second as u8 + 1, None, 104, 111);
            outputs.insert(output(&c, &[a, b]).unwrap());
        }
    }
    assert_eq!(outputs.len() as u128, beacon_grinding_choices_bound_v1(2, j as u32, 1), "P(5, 2) = 20: exactly the bound");
    // Swapping the settlement order of the same two works is two beacons (the order is canonical by settlement).
    let x = [work(SRC_A, 1, 1, None, 103, 110), work(SRC_B, 2, 2, None, 104, 111)];
    let y = [work(SRC_A, 1, 1, None, 104, 111), work(SRC_B, 2, 2, None, 103, 110)];
    assert_ne!(output(&c, &x), output(&c, &y));
}

/// **Withholding after the lock is a veto, and vetoes are counted.** A source convicted after the lock no longer counts: the branch
/// derives another beacon, the evidence about the old one is refused (BEACON_CHANGED), and the attempt ends as a counted retry —
/// `retry_limit + 1` attempts in all, then the record refuses another commitment.
#[test]
fn a_veto_after_the_lock_ends_the_attempt_and_vetoes_are_bounded_by_the_retry_limit() {
    let c = ctx(ImplementedV1::source_eligibility_distinct());
    let mut works = vec![work(SRC_A, 1, 1, None, 103, 110), work(SRC_B, 2, 2, None, 104, 111), work(SRC_A, 3, 3, None, 105, 115)];
    let locked = match collect_attributed_work_beacon_v1(&c, &works, 1_000).unwrap() {
        WorkBeaconStateV1::Locked(b) => b,
        other => panic!("{other:?}"),
    };
    works[1].event.claim_final = false; // convicted after Final: the fact is withdrawn
    let refused = verify_attributed_work_beacon_v1(&c, locked.beacon(), &works, 1_000);
    assert!(refused.is_err(), "the presented beacon is not this branch's any more");
    assert_ne!(output(&c, &works), Some(locked.output), "a veto buys one more beacon");
    let mut record = misaka_palw_challenge::lifecycle::OnboardingRecordV1::new(c.policy.retry_limit + 1);
    for step in [
        OnboardingStepV1::Frontend(Ok([1; 64])),
        OnboardingStepV1::StaticAdmission(Ok([2; 64])),
        OnboardingStepV1::Registered { class_id: [3; 64] },
    ] {
        record.apply(step).unwrap();
    }
    for _ in 0..=c.policy.retry_limit {
        record.apply(OnboardingStepV1::ConformanceCommitted { commitment_root: [4; 64] }).unwrap();
        record.apply(OnboardingStepV1::BeaconUnavailable).unwrap();
    }
    assert!(
        matches!(
            record.apply(OnboardingStepV1::ConformanceCommitted { commitment_root: [4; 64] }),
            Err(OnboardingRefusalV1::AttemptsExhausted(_))
        ),
        "R + 1 vetoes at most"
    );
}

/// **Fork choice**: the beacon is branch-relative — two branches with different Finals lock different beacons, and a beacon of one
/// branch is refused on the other (each alternative branch is one more choice, `F` in the accounting, bounded by `D`).
#[test]
fn a_beacon_of_one_branch_is_refused_on_another() {
    let c = ctx(ImplementedV1::source_eligibility_distinct());
    let a = [work(SRC_A, 1, 1, None, 103, 110), work(SRC_B, 2, 2, None, 104, 111)];
    let b = [work(SRC_A, 1, 1, None, 103, 110), work(SRC_B, 7, 3, None, 104, 111)];
    let on_a = match collect_attributed_work_beacon_v1(&c, &a, 1_000).unwrap() {
        WorkBeaconStateV1::Locked(x) => x,
        other => panic!("{other:?}"),
    };
    assert!(verify_attributed_work_beacon_v1(&c, on_a.beacon(), &a, 1_000).is_ok());
    assert!(verify_attributed_work_beacon_v1(&c, on_a.beacon(), &b, 1_000).is_err());
    // Before depth D the k-th source is not locked: a shallow reorg changes a candidate, never a lock.
    assert!(matches!(collect_attributed_work_beacon_v1(&c, &a, 112).unwrap(), WorkBeaconStateV1::Candidate { .. }));
}

/// **Precomputation**: a work committed before the subject's start `S` is not fresh, whatever it settles — the anchor delay keeps a
/// source from being fixed before the subject is.
#[test]
fn a_work_committed_before_the_start_is_never_a_source() {
    let c = ctx(ImplementedV1::source_eligibility_distinct());
    let early = [work(SRC_A, 1, 1, None, 101, 110), work(SRC_B, 2, 2, None, 104, 111)];
    assert!(output(&c, &early).is_none(), "S = 102: accepted at 101 is not fresh");
}
