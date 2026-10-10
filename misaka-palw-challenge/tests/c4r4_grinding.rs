//! **C4 round 4 — PALW Work Beacon grinding (Target 1), against OPV-BOOT's source rules (`opv/bootstrap-beacon` @ `410d430e5`).**
//!
//! OPV-BOOT's own `tests/grinding.rs` already shows the last contributor grinding one free input; it is not repeated here. This file
//! attacks the defence OPV-BOOT added against work concentration — the DISTINCT source rules — and SOUND's SG-01a ("no first-k
//! capture").
//!
//! **F-C4R4-01 (P2, design; dormant — no consensus code reads the rules yet).** The distinct rules tell BONDS apart, not parties. A
//! party that settles first with `k` works from `k` Sybil producer bonds, on jobs posted by `k` Sybil poster bonds, across two eligible
//! classes, satisfies `Distinct` and `DistinctClassCapped` and owns EVERY source position: the honest Finals that settle after it are
//! never read, and since all `k` inputs are its own it grinds all of them offline before committing (`k` free nonces, not one). The
//! rule raises the price of capture to `k` producer bonds, `k` posted jobs and `k` OPV reservations; it does not remove capture. A
//! beacon that mixes ALL eligible Finals of its window (or a sealed-source v3 whose seal window closes before any reveal) is what SG-01a
//! needs; the distinct rules are a cost, to be priced in the effective accounting as such.

use std::collections::BTreeSet;

use misaka_palw_challenge::hash::{Digest, object_id};
use misaka_palw_challenge::policy::ImplementedV1;
use misaka_palw_challenge::*;

const CLASS_A: Digest = [0xA1; 64];
const CLASS_B: Digest = [0xA2; 64];
const CANDIDATE: Digest = [0xCA; 64];

fn ctx(rule: Digest) -> BeaconContextV1 {
    BeaconContextV1 {
        chain_genesis: [0x11; 64],
        ruleset_id: [0x22; 64],
        policy: PostCommitChallengePolicyV1 { source_eligibility_policy_id: rule, ..reference_policy_v1(2, 2, 40, 2, 1) },
        subject_kind: SubjectKindV1::ModelConformance,
        commitment_root: [0xC0; 64],
        commitment_position: 100,
        challenge_epoch: 0,
        eligible_profiles: [CLASS_A, CLASS_B].into_iter().collect(),
        excluded_profiles: [CANDIDATE].into_iter().collect(),
        candidate_profile_id: RootV1::Present(CANDIDATE),
    }
}

/// An OPV Final whose work identity is `H(class, job nonce)`, by `producer` on a job posted by `consumer`.
fn work(class: Digest, nonce: u64, producer: u8, consumer: u8, accepted: u64, settled: u64) -> AttributedWorkV1 {
    AttributedWorkV1 {
        event: WorkFinalEventV1 {
            kind: WorkSourceKindV1::RealUsefulWork,
            source_profile_id: class,
            canonical_work_id: object_id(b"test/canonical-work", &(class, nonce)),
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
        attribution: SourceAttributionV1 { producer_id: [producer; 64], consumer_id: RootV1::Present([consumer; 64]) },
    }
}

fn locked(c: &BeaconContextV1, works: &[AttributedWorkV1]) -> WorkBeaconV1 {
    match collect_attributed_work_beacon_v1(c, works, 1_000).unwrap() {
        WorkBeaconStateV1::Locked(b) => b.beacon().clone(),
        other => panic!("{other:?}"),
    }
}

/// **F-C4R4-01: one party with `k` Sybil bonds owns every position of a distinct-rule beacon and grinds all of them.** Two honest
/// Finals (distinct producers 1, 2; distinct consumers 1, 2; both classes) settle at 110 and 111. The attacker — producers 0xE1, 0xE2,
/// posters 0xF1, 0xF2, one work on each eligible class — settles at 103 and 104. Under both distinct rules the beacon is the
/// attacker's two works; neither honest Final is read. All the attacker's inputs are free nonces fixed before any honest source exists,
/// so 2^12 offline tries over the PAIR give 2^12 distinct outputs and an 8-bit target is hit.
#[test]
fn f_c4r4_01_the_distinct_rules_tell_bonds_apart_not_parties_and_k_sybil_bonds_own_every_source() {
    for rule in [ImplementedV1::source_eligibility_distinct(), ImplementedV1::source_eligibility_distinct_class_capped()] {
        let c = ctx(rule);
        let honest = [work(CLASS_A, 1, 1, 1, 105, 110), work(CLASS_B, 2, 2, 2, 106, 111)];
        let attacker =
            |n: u64| [work(CLASS_A, 1_000 + (n & 0x3F), 0xE1, 0xF1, 102, 103), work(CLASS_B, 5_000 + (n >> 6), 0xE2, 0xF2, 102, 104)];
        // Without the attacker the honest Finals are the beacon.
        let fair = locked(&c, &honest);
        let honest_ids: BTreeSet<Digest> = honest.iter().map(|w| w.event.canonical_work_id).collect();
        assert!(fair.sources.iter().all(|s| honest_ids.contains(&s.canonical_work_id)));

        let mut outputs = BTreeSet::new();
        let mut hit = None;
        for n in 0..(1u64 << 12) {
            let mut works = attacker(n).to_vec();
            works.extend(honest.iter().cloned());
            let b = locked(&c, &works);
            assert!(
                b.sources.iter().all(|s| !honest_ids.contains(&s.canonical_work_id)),
                "every source position is the attacker's under the distinct rule"
            );
            if hit.is_none() && b.output[0] == 0 {
                hit = Some(n);
            }
            outputs.insert(b.output);
        }
        assert_eq!(outputs.len(), 1 << 12, "every offline pair of nonces is its own beacon");
        assert!(hit.is_some(), "an 8-bit target is hit");
        eprintln!(
            "[F-C4R4-01] rule {:02x?}: 2 Sybil producers + 2 Sybil posters own both positions; 4096 offline nonce pairs -> 4096 \
             beacons; honest Finals never read",
            &rule[..4]
        );
    }
}
