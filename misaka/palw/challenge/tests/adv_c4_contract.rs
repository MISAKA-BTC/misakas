//! **Agent C4 — independent adversarial tests of the shared challenge contract** (`misaka-palw-challenge`): the PALW Work Beacon,
//! the seed and its samplers, transcripts, the onboarding lifecycle and conformance evidence.
//!
//! `#[ignore = "FAIL F-C4-n …"]` marks a proven defect: the test asserts the SAFE property and fails today
//! (`cargo test --offline -p misaka-palw-challenge --test adv_c4_contract -- --ignored`). Every other test is a PASS verdict.
//! The expected result is stated in each test's doc comment BEFORE it was first run.

use std::collections::BTreeSet;
use std::panic::catch_unwind;

use misaka_palw_challenge::beacon::{IneligibleV1, eligibility_v1};
use misaka_palw_challenge::conformance::ConformanceRefusalV1;
use misaka_palw_challenge::hash::{Digest, named_id};
use misaka_palw_challenge::policy::{ImplementedV1, PolicyRefusalV1};
use misaka_palw_challenge::seed::{P127, SampleRefusalV1, TranscriptRefusalV1};
use misaka_palw_challenge::*;

const GENESIS: Digest = [0x11; 64];
const RULESET: Digest = [0x22; 64];
const CANDIDATE: Digest = [0xCA; 64];
const A: Digest = [0xA1; 64];
const B: Digest = [0xA2; 64];

fn policy() -> PostCommitChallengePolicyV1 {
    reference_policy_v1(3, 2, 40, 5, 4)
}

fn ctx(kind: SubjectKindV1) -> BeaconContextV1 {
    BeaconContextV1 {
        chain_genesis: GENESIS,
        ruleset_id: RULESET,
        policy: policy(),
        subject_kind: kind,
        commitment_root: [0xC0; 64],
        commitment_position: 100,
        challenge_epoch: 7,
        eligible_profiles: [A, B].into_iter().collect(),
        excluded_profiles: [CANDIDATE].into_iter().collect(),
        candidate_profile_id: RootV1::Present(CANDIDATE),
    }
}

fn work(n: u8, profile: Digest, accepted: u64, settled: u64) -> WorkFinalEventV1 {
    WorkFinalEventV1 {
        kind: WorkSourceKindV1::RealUsefulWork,
        source_profile_id: profile,
        canonical_work_id: [n; 64],
        execution_commitment: [n ^ 0xFF; 64],
        accepted_position: accepted,
        settlement_position: settled,
        occurrence_index: 0,
        claim_final: true,
        da_satisfied: true,
        validity_independent: true,
        depends_on_profiles: vec![],
        final_path: FinalPathV1::PanelIndependent,
    }
}

/// S = 102, window [102, 142), k = 3, D = 5.
fn good() -> Vec<WorkFinalEventV1> {
    vec![work(1, A, 103, 110), work(2, B, 104, 111), work(3, A, 105, 112)]
}

/// The beacon this context locks, as the crate derives it: a `VerifiedWorkBeaconV1` (F-C4-05's fix — a seed is never minted from
/// a presented beacon) that reads as its `WorkBeaconV1` through `Deref`.
fn locked(c: &BeaconContextV1, events: &[WorkFinalEventV1], tip: u64) -> VerifiedWorkBeaconV1 {
    match collect_work_beacon_v1(c, events, tip).unwrap() {
        WorkBeaconStateV1::Locked(b) => b,
        other => panic!("not locked: {other:?}"),
    }
}

fn subject(c: &BeaconContextV1) -> ChallengeSubjectV1 {
    ChallengeSubjectV1 {
        chain_genesis: GENESIS,
        ruleset_id: RULESET,
        challenge_policy_id: c.policy.id(),
        subject_kind: c.subject_kind,
        subject_id: CANDIDATE,
        kernel_id: RootV1::Present([1; 64]),
        verification_plan_root: RootV1::Present([2; 64]),
        program_root: RootV1::Present([3; 64]),
        artifact_root: RootV1::Present([4; 64]),
        tokenizer_or_schema_root: RootV1::Absent,
        layout_root: RootV1::Present([5; 64]),
        input_root: RootV1::Absent,
        state_root: RootV1::Absent,
        constraint_root: RootV1::Absent,
        commitment_root: c.commitment_root,
    }
}

// ── beacon sources ───────────────────────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): adding events of EVERY non-useful-work kind (heartbeat, BASE-0, EXEC_TX, EXEC slice, receipt-only,
/// provisional, bare Panel receipt), at every position and in every order, leaves the locked beacon byte-identical to the one built
/// from the real works alone: nothing but REAL useful work is entropy. VERDICT: PASS.
#[test]
fn heartbeat_base0_exec_and_receipts_are_never_entropy_whatever_their_number_position_or_order() {
    let c = ctx(SubjectKindV1::ClaimVerification);
    let base = locked(&c, &good(), 150);
    let kinds = [
        WorkSourceKindV1::Heartbeat,
        WorkSourceKindV1::Base0Fallback,
        WorkSourceKindV1::ExecTx,
        WorkSourceKindV1::ExecWorkSlice,
        WorkSourceKindV1::ReceiptOnly,
        WorkSourceKindV1::ProvisionalAttempt,
        WorkSourceKindV1::PanelReceipt,
    ];
    let mut noisy = good();
    for (i, k) in kinds.iter().enumerate() {
        for settled in [102u64, 105, 109, 110, 111, 120, 141] {
            let mut e = work(100 + i as u8 * 10 + (settled % 7) as u8, A, 103, settled);
            e.kind = *k;
            noisy.push(e);
        }
    }
    assert_eq!(locked(&c, &noisy, 150), base);
    let mut rev = noisy.clone();
    rev.reverse();
    assert_eq!(locked(&c, &rev, 150), base);
    // Even with ONLY non-real events the beacon is unavailable, never a fallback.
    let only: Vec<_> = noisy.iter().filter(|e| e.kind != WorkSourceKindV1::RealUsefulWork).cloned().collect();
    assert_eq!(collect_work_beacon_v1(&c, &only, 150).unwrap(), WorkBeaconStateV1::Unavailable { have: 0, need: 3 });
    for e in &only {
        assert!(matches!(eligibility_v1(&c, e), Err(IneligibleV1::NotUsefulWork(_))));
    }
}

/// EXPECTED (before running): the output is a function of the SET of canonical events: every permutation of six distinct events
/// (k = 4 of them used) gives the same beacon. VERDICT: PASS.
#[test]
fn every_permutation_of_the_same_events_gives_the_same_beacon() {
    let mut c = ctx(SubjectKindV1::WorkSlice);
    c.policy = reference_policy_v1(4, 2, 40, 5, 4);
    let events: Vec<_> = (0..6u8).map(|i| work(10 + i, if i % 2 == 0 { A } else { B }, 103 + i as u64, 110 + i as u64 / 2)).collect();
    let base = locked(&c, &events, 150);
    let mut idx: Vec<usize> = (0..6).collect();
    let mut count = 0;
    // Heap's algorithm.
    fn heap(k: usize, a: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if k == 1 {
            out.push(a.clone());
            return;
        }
        heap(k - 1, a, out);
        for i in 0..k - 1 {
            if k % 2 == 0 {
                a.swap(i, k - 1)
            } else {
                a.swap(0, k - 1)
            }
            heap(k - 1, a, out);
        }
    }
    let mut perms = Vec::new();
    heap(6, &mut idx, &mut perms);
    for p in perms {
        let shuffled: Vec<_> = p.iter().map(|i| events[*i].clone()).collect();
        assert_eq!(locked(&c, &shuffled, 150), base, "permutation {p:?}");
        count += 1;
    }
    assert_eq!(count, 720);
}

/// EXPECTED (before running): two events that tie on the whole sort key `(settlement_position, occurrence_index, canonical_work_id)`
/// but differ in `execution_commitment` (same canonical id, two different executions) must give the same beacon whatever the input
/// order, because the contract says arrival order is irrelevant. FAILS today: `sort_by` is stable and the key omits the execution
/// commitment and profile, so the input order picks which execution is mixed.
#[test]
#[ignore = "FAIL F-C4-06: events tying on (settlement, occurrence, canonical_work_id) are ordered by arrival, not canonically"]
fn events_tying_on_the_whole_sort_key_must_not_be_ordered_by_arrival() {
    let c = ctx(SubjectKindV1::ClaimVerification);
    let mut x = work(1, A, 103, 110);
    let mut y = work(1, A, 103, 110);
    x.execution_commitment = [0x01; 64];
    y.execution_commitment = [0x02; 64];
    let rest = vec![work(2, B, 104, 111), work(3, A, 105, 112)];
    let ab: Vec<_> = [vec![x.clone(), y.clone()], rest.clone()].concat();
    let ba: Vec<_> = [vec![y, x], rest].concat();
    let (x1, x2) = (locked(&c, &ab, 150), locked(&c, &ba, 150));
    assert!(x1.output == x2.output, "arrival order changed the beacon output");
}

/// EXPECTED (before running): unfinalized work, DA-unsatisfied work, validity-dependent work, a work committed before S and
/// reattached later (first occurrence counts), a work first seen OUTSIDE the window and re-included inside it, and the candidate's
/// own / dependent profile's work are never sources; an ineligible first occurrence is not rescued by a later eligible one.
/// VERDICT: PASS.
#[test]
fn unfinalized_stale_reattached_and_self_work_never_become_sources_and_are_never_rescued() {
    let c = ctx(SubjectKindV1::ModelConformance);
    let mut evs = good();
    // Each attack adds a candidate source that must stay out.
    let mut not_final = work(20, A, 103, 113);
    not_final.claim_final = false;
    let mut no_da = work(21, A, 103, 113);
    no_da.da_satisfied = false;
    let mut dependent = work(22, A, 103, 113);
    dependent.validity_independent = false;
    let stale = work(23, A, 99, 113); // committed before S = 102
    let self_work = work(24, CANDIDATE, 103, 113);
    let mut depends_on_candidate = work(25, A, 103, 113);
    depends_on_candidate.depends_on_profiles = vec![CANDIDATE];
    let early = work(26, A, 103, 101); // settled before the window
    let late = work(27, A, 103, 142); // settled at the window end (exclusive)
    // A work whose first occurrence is outside the window and whose re-inclusion is inside: not rescued.
    let first_out = work(28, A, 103, 101);
    let mut reincluded = work(28, A, 103, 113);
    reincluded.occurrence_index = 1;
    for e in [&not_final, &no_da, &dependent, &stale, &self_work, &depends_on_candidate, &early, &late, &first_out, &reincluded] {
        assert!(eligibility_v1(&c, e).is_err() || e.canonical_work_id == [28; 64], "{:?}", e.canonical_work_id[0]);
    }
    evs.extend([not_final, no_da, dependent, stale, self_work, depends_on_candidate, early, late, first_out, reincluded]);
    assert_eq!(locked(&c, &evs, 150), locked(&c, &good(), 150), "none of them entered the accumulator");
    let b = locked(&c, &evs, 150);
    assert_eq!(b.sources.len(), 3);
}

/// OBSERVATION O-C4-07 (descriptive, passes): the contract trusts the consumer to domain-separate `canonical_work_id` by work kind.
/// A heartbeat event that carries the same canonical id as a real work and is seen first is the "first occurrence", and the real
/// work is then never a source. Safe only while heartbeat ids cannot collide with real-work ids.
#[test]
fn observation_a_non_real_event_sharing_a_canonical_id_suppresses_the_real_work_it_collides_with() {
    let c = ctx(SubjectKindV1::ClaimVerification);
    let mut hb = work(1, A, 103, 109);
    hb.kind = WorkSourceKindV1::Heartbeat;
    let mut evs = good();
    evs.push(hb);
    let state = collect_work_beacon_v1(&c, &evs, 150).unwrap();
    let WorkBeaconStateV1::Unavailable { have, .. } = state else { panic!("{state:?}") };
    assert_eq!(have, 2, "work 1 was suppressed by an earlier heartbeat with its id");
}

/// EXPECTED (before running): a Panel-assignment beacon takes only Panel-independent Finals; with every Final Panel-licensed it
/// stays unavailable (RFC-0010 dormant); a PanelLicensed source cannot be swapped in by reordering; a mix counts only the
/// independent ones; and other subject kinds do accept Panel-licensed sources. VERDICT: PASS.
#[test]
fn a_panel_licensed_final_never_seeds_a_panel_assignment() {
    let c = ctx(SubjectKindV1::PanelAssignment);
    let licensed = |n: u8, s: u64| {
        let mut e = work(n, A, 103, s);
        e.final_path = FinalPathV1::PanelLicensed { panel_seed_id: [9; 64], panel_epoch: 3 };
        e
    };
    let all_licensed: Vec<_> = (50..=55u8).map(|n| licensed(n, 100 + n as u64 - 40)).collect();
    assert!(matches!(collect_work_beacon_v1(&c, &all_licensed, 200).unwrap(), WorkBeaconStateV1::Unavailable { have: 0, .. }));
    for e in &all_licensed {
        assert_eq!(eligibility_v1(&c, e), Err(IneligibleV1::PanelDependentFinal));
    }
    let mut mix = all_licensed.clone();
    mix.extend(good()); // three independent
    let b = locked(&c, &mix, 200);
    assert_eq!(b, locked(&c, &good(), 200), "licensed works are inert");
    assert!(
        collect_work_beacon_v1(&ctx(SubjectKindV1::ClaimVerification), &all_licensed, 200)
            .is_ok_and(|s| matches!(s, WorkBeaconStateV1::Locked(_)))
    );
}

/// EXPECTED (before running): a forgetful consumer cannot make a candidate beacon itself for MODEL/KERNEL conformance:
/// `candidate_profile_id = Absent` is refused (`Missing`), and a `Present` candidate is excluded even if `excluded_profiles` is empty.
/// For the other subject kinds the candidate is the consumer's `excluded_profiles` to name: documented, not enforceable here.
/// VERDICT: PASS for conformance subjects; GAP-C4-A for the others (see the record).
#[test]
fn a_forgetful_consumer_cannot_let_a_conformance_candidate_seed_its_own_challenge() {
    for kind in [SubjectKindV1::ModelConformance, SubjectKindV1::KernelConformance] {
        let mut c = ctx(kind);
        c.candidate_profile_id = RootV1::Absent;
        assert!(matches!(collect_work_beacon_v1(&c, &good(), 150), Err(PolicyRefusalV1::Missing("candidate_profile_id"))), "{kind:?}");
        let mut c = ctx(kind);
        c.excluded_profiles.clear(); // the consumer forgot to list it
        let mut evs = good();
        evs.push(work(30, CANDIDATE, 103, 108)); // the candidate's own work, earliest
        c.eligible_profiles.insert(CANDIDATE);
        assert_eq!(locked(&c, &evs, 150), locked(&c, &good(), 150), "{kind:?}: the candidate's own work seeded its challenge");
    }
    // The non-conformance kinds: nothing in the context names the subject's own profile; the contract cannot refuse it.
    let mut c = ctx(SubjectKindV1::ClaimVerification);
    c.candidate_profile_id = RootV1::Absent;
    c.excluded_profiles.clear();
    c.eligible_profiles.insert(CANDIDATE);
    let mut evs = good();
    evs.push(work(30, CANDIDATE, 103, 108));
    assert_ne!(
        locked(&c, &evs, 150),
        locked(&c, &good(), 150),
        "GAP-C4-A: a forgetful consumer's self-beacon is accepted for CLAIM_VERIFICATION"
    );
}

// ── lock, reorg, verification ────────────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): the beacon is branch-relative: before the k-th source is D deep it is only a candidate; once locked it
/// is stable as the tip advances and under later events; a reorg that drops, replaces or inserts an EARLIER source gives a
/// different beacon and seed and the old presented beacon is refused against the new history; a reorg that only changes events
/// after the lock (k+1-th or later) changes nothing; undoing the reorg restores the old beacon exactly. VERDICT: PASS.
#[test]
fn a_reorg_before_the_lock_changes_the_beacon_and_one_after_the_k_th_source_does_not() {
    let c = ctx(SubjectKindV1::ClaimVerification);
    let evs = good(); // k-th settles at 112, lock at 117
    assert!(matches!(collect_work_beacon_v1(&c, &evs, 116).unwrap(), WorkBeaconStateV1::Candidate { lock_position: 117, .. }));
    let b = locked(&c, &evs, 117);
    assert_eq!(locked(&c, &evs, 10_000), b, "stable as the tip advances");
    let mut later = evs.clone();
    later.push(work(4, A, 106, 120));
    later.push(work(5, B, 106, 121));
    assert_eq!(locked(&c, &later, 10_000), b, "k+1-th and later works never change a locked beacon");
    // Reorg replaced the 2nd source by another work (a different branch).
    let mut other = evs.clone();
    other[1] = work(9, B, 104, 111);
    let b2 = locked(&c, &other, 117);
    assert_ne!(b2.output, b.output);
    assert!(verify_work_beacon_v1(&c, b.beacon(), &other, 117).is_err(), "the old beacon is refused on the new branch");
    assert!(verify_work_beacon_v1(&c, b2.beacon(), &other, 117).is_ok());
    let s = subject(&c);
    assert_ne!(challenge_seed_v1(&c, &s, &b).unwrap(), challenge_seed_v1(&c, &s, &b2).unwrap());
    // Reorg dropped the 3rd source: back to collecting (no lock).
    let dropped = vec![evs[0].clone(), evs[1].clone()];
    assert!(matches!(collect_work_beacon_v1(&c, &dropped, 130).unwrap(), WorkBeaconStateV1::Collecting { have: 2, .. }));
    // A new EARLIER source inserted by a reorg shifts the whole set.
    let mut inserted = evs.clone();
    inserted.push(work(8, A, 103, 109));
    assert_ne!(locked(&c, &inserted, 150), b);
    // Undone.
    assert_eq!(locked(&c, &evs, 150), b);
}

/// EXPECTED (before running): a presented beacon is a claim to recompute: every single-field edit of every part (a source field, an
/// accumulator, the output, the anchor, the lock position, source order, a duplicated or missing source) is refused by
/// `verify_work_beacon_v1`. VERDICT: PASS.
#[test]
fn every_single_field_edit_of_a_presented_beacon_is_refused() {
    let c = ctx(SubjectKindV1::ClaimVerification);
    // The presented form: the derived beacon's plain `WorkBeaconV1`, as a peer would send it.
    let b = locked(&c, &good(), 150).beacon().clone();
    assert!(verify_work_beacon_v1(&c, &b, &good(), 150).is_ok());
    let mut edits: Vec<(&str, WorkBeaconV1)> = Vec::new();
    for i in 0..b.sources.len() {
        let mut x = b.clone();
        x.sources[i].canonical_work_id[0] ^= 1;
        edits.push(("source work id", x));
        let mut x = b.clone();
        x.sources[i].execution_commitment[0] ^= 1;
        edits.push(("source execution", x));
        let mut x = b.clone();
        x.sources[i].source_profile_id[0] ^= 1;
        edits.push(("source profile", x));
        let mut x = b.clone();
        x.sources[i].settlement_position += 1;
        edits.push(("source settlement", x));
        let mut x = b.clone();
        x.sources[i].accepted_position += 1;
        edits.push(("source accepted", x));
        let mut x = b.clone();
        x.sources[i].occurrence_index += 1;
        edits.push(("source occurrence", x));
    }
    for i in 0..b.accumulators.len() {
        let mut x = b.clone();
        x.accumulators[i][5] ^= 1;
        edits.push(("accumulator", x));
    }
    let mut x = b.clone();
    x.output[0] ^= 1;
    edits.push(("output", x));
    let mut x = b.clone();
    x.challenge_anchor[0] ^= 1;
    edits.push(("anchor", x));
    let mut x = b.clone();
    x.lock_position -= 1;
    edits.push(("lock position", x));
    let mut x = b.clone();
    x.sources.swap(0, 1);
    edits.push(("order", x));
    let mut x = b.clone();
    x.sources[1] = x.sources[0].clone();
    edits.push(("duplicate", x));
    let mut x = b.clone();
    x.sources.pop();
    edits.push(("missing", x));
    let mut x = b.clone();
    x.accumulators.pop();
    edits.push(("short chain", x));
    for (what, e) in edits {
        assert!(verify_work_beacon_v1(&c, &e, &good(), 150).is_err(), "{what} was accepted");
    }
}

/// EXPECTED (before running): `challenge_seed_v1` is the one place a seed is minted, "after the subject is committed and the beacon
/// is locked". The safe property is that it refuses a beacon that was never derived from THIS context (another subject's
/// beacon, another epoch's, an invented output). FAILS today: it hashes `(subject, beacon.anchor, beacon.output)` for any
/// `WorkBeaconV1` shape; only a caller that remembered to run `verify_work_beacon_v1` is protected, and a typed `VerifiedBeacon`
/// would make the mistake unrepresentable.
///
/// F-C4-05 is fixed by exactly that: `challenge_seed_v1` takes a `VerifiedWorkBeaconV1`, which only this crate's collectors derive
/// (it has no decoder and no public constructor), so an invented beacon cannot reach the seed at all — it is refused where a
/// presented beacon enters, `verify_work_beacon_v1` — and another context's derived beacon is refused by its context id.
#[test]
fn a_seed_must_not_be_minted_from_a_beacon_that_is_not_this_contexts() {
    let c = ctx(SubjectKindV1::ClaimVerification);
    let s = subject(&c);
    let real = locked(&c, &good(), 150);
    // Another epoch's real beacon.
    let mut c2 = c.clone();
    c2.challenge_epoch = 8;
    let other_epoch = locked(&c2, &good(), 150);
    // An invented one: right shape, attacker-chosen output and anchor.
    let mut invented = real.beacon().clone();
    invented.output = [0x41; 64];
    invented.challenge_anchor = [0x42; 64];
    let ok = challenge_seed_v1(&c, &s, &real).is_ok();
    assert!(ok);
    assert!(challenge_seed_v1(&c, &s, &other_epoch).is_err(), "another epoch's beacon minted a seed for this subject");
    assert!(verify_work_beacon_v1(&c, &invented, &good(), 150).is_err(), "an invented beacon is never verified, so never seeds");
    assert!(verify_work_beacon_v1(&c, real.beacon(), &good(), 150).is_ok(), "the derived one is");
}

/// EXPECTED (before running): arbitrary contexts and events, including extreme positions and counts, never panic the beacon
/// derivation or the verifier (all window/lock arithmetic saturates). VERDICT: PASS.
#[test]
fn extreme_positions_windows_and_counts_never_panic_the_beacon() {
    let extremes = [0u64, 1, 2, u64::MAX / 2, u64::MAX - 1, u64::MAX];
    let mut n = 0u32;
    for &pos in &extremes {
        for &delay in &extremes {
            for &win in &extremes {
                for &depth in &[1u64, u64::MAX] {
                    for &k in &[1u32, 3, u32::MAX] {
                        let mut c = ctx(SubjectKindV1::WorkSlice);
                        c.policy = reference_policy_v1(k, delay.max(1), win.max(1), depth, 1);
                        c.commitment_position = pos;
                        let evs: Vec<_> = extremes.iter().enumerate().map(|(i, p)| work(i as u8 + 1, A, *p, *p)).collect();
                        let r = catch_unwind(|| {
                            for tip in [0u64, 100, u64::MAX] {
                                if let Ok(WorkBeaconStateV1::Locked(b)) = collect_work_beacon_v1(&c, &evs, tip) {
                                    let _ = verify_work_beacon_v1(&c, &b, &evs, tip);
                                    let _ = challenge_seed_v1(&c, &subject(&c), &b);
                                }
                            }
                        });
                        assert!(r.is_ok(), "panic at pos={pos} delay={delay} win={win} depth={depth} k={k}");
                        n += 1;
                    }
                }
            }
        }
    }
    assert!(n > 500);
}

// ── seeds across subject kinds ───────────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): the same events, commitment root and subject fields under each of the six subject kinds give six
/// distinct seeds and six distinct beacons-with-anchors; a beacon locked for one kind can never be replayed as another kind's seed
/// (`SubjectMismatch`); and no single-field edit of the subject keeps the seed. VERDICT: PASS.
#[test]
fn six_subject_kinds_six_seeds_and_no_single_subject_field_is_a_free_edit() {
    let mut seeds = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    for kind in SubjectKindV1::ALL {
        let c = ctx(kind);
        let b = locked(&c, &good(), 150);
        anchors.insert(b.output);
        let s = challenge_seed_v1(&c, &subject(&c), &b).unwrap();
        assert!(seeds.insert(s), "{kind:?} shares a seed");
        // The same beacon under another kind's subject is refused.
        for other in SubjectKindV1::ALL.iter().filter(|o| **o != kind) {
            let mut sub = subject(&c);
            sub.subject_kind = *other;
            assert_eq!(challenge_seed_v1(&c, &sub, &b), Err(misaka_palw_challenge::seed::SeedRefusalV1::SubjectMismatch));
        }
    }
    assert_eq!(seeds.len(), 6);
    assert_eq!(anchors.len(), 1, "the accumulator does not depend on the kind: the kind is in the SEED (documented)");
    let c = ctx(SubjectKindV1::ClaimVerification);
    let b = locked(&c, &good(), 150);
    let base = challenge_seed_v1(&c, &subject(&c), &b).unwrap();
    let edits: Vec<Box<dyn Fn(&mut ChallengeSubjectV1)>> = vec![
        Box::new(|s| s.subject_id[0] ^= 1),
        Box::new(|s| s.kernel_id = RootV1::Absent),
        Box::new(|s| s.verification_plan_root = RootV1::Present([0; 64])),
        Box::new(|s| s.program_root = RootV1::Present([9; 64])),
        Box::new(|s| s.artifact_root = RootV1::Present([9; 64])),
        Box::new(|s| s.tokenizer_or_schema_root = RootV1::Present([0; 64])),
        Box::new(|s| s.layout_root = RootV1::Absent),
        Box::new(|s| s.input_root = RootV1::Present([0; 64])),
        Box::new(|s| s.state_root = RootV1::Present([0; 64])),
        Box::new(|s| s.constraint_root = RootV1::Present([0; 64])),
    ];
    for (i, e) in edits.iter().enumerate() {
        let mut s = subject(&c);
        e(&mut s);
        assert_ne!(challenge_seed_v1(&c, &s, &b).unwrap(), base, "edit #{i} kept the seed");
    }
}

// ── samplers ─────────────────────────────────────────────────────────────────────────────────────────────────────────────

fn stream(label: u32, rep: u32) -> ChallengeStreamV1 {
    ChallengeStreamV1::new(
        &[0x5E; 64],
        &StreamLabelV1 { kind: StreamKindV1::Query, scope_id: [7; 64], relation: label, repetition: rep },
    )
}

/// Pearson chi-square of `counts` against uniform.
fn chi2(counts: &[u64]) -> f64 {
    let total: u64 = counts.iter().sum();
    let e = total as f64 / counts.len() as f64;
    counts.iter().map(|c| (*c as f64 - e).powi(2) / e).sum()
}

/// EXPECTED (before running): rejection sampling has no modulo bias: for n = 3, 10, 1000 a million draws are uniform (chi-square
/// under 3x its critical value at p = 0.001); for n = 2^63 + 1 (the worst case for `w % n`) the draw is in range with the high half
/// hit about half the time; `n = 1`, `n = u64::MAX` and `n = 2^63` are in range; a zero range is refused. Fixed seed, generous
/// bounds. VERDICT: PASS.
#[test]
fn the_index_sampler_is_uniform_including_the_worst_case_modulus() {
    // Critical chi-square values at p = 0.001 for df = 2, 9, 999: 13.8, 27.9, 1143 (generous: x3).
    for (n, crit) in [(3u64, 13.8), (10, 27.9), (1000, 1143.0)] {
        let mut s = stream(n as u32, 0);
        let mut counts = vec![0u64; n as usize];
        for _ in 0..1_000_000 {
            counts[s.index_below(n).unwrap() as usize] += 1;
        }
        let x = chi2(&counts);
        assert!(x < crit * 3.0, "n={n}: chi-square {x}");
    }
    let n = (1u64 << 63) + 1;
    let mut s = stream(99, 0);
    let (mut high, total) = (0u64, 200_000u64);
    for _ in 0..total {
        let v = s.index_below(n).unwrap();
        assert!(v < n);
        if v >= (1u64 << 62) {
            high += 1;
        }
    }
    let frac = high as f64 / total as f64;
    assert!((0.48..0.52).contains(&frac), "upper-half fraction {frac}");
    assert_eq!(stream(1, 1).index_below(1).unwrap(), 0);
    for n in [u64::MAX, 1 << 63] {
        let mut s = stream(2, 2);
        for _ in 0..1000 {
            assert!(s.index_below(n).unwrap() < n);
        }
    }
    assert_eq!(stream(1, 1).index_below(0), Err(SampleRefusalV1::EmptyRange));
}

/// EXPECTED (before running): Floyd's distinct sampler returns exactly `count` sorted, distinct, in-range indices, uniformly: over
/// 60,000 draws of 3 of 10 every index is chosen with frequency 0.30 +/- 0.02 and every 3-subset appears; `count > n` is refused;
/// `count == n` returns everything. VERDICT: PASS.
#[test]
fn the_distinct_sampler_is_uniform_over_subsets_and_refuses_too_many() {
    let mut freq = [0u64; 10];
    let mut subsets = BTreeSet::new();
    for rep in 0..60_000u32 {
        let mut s = stream(7, rep);
        let v = s.distinct_indices(10, 3).unwrap();
        assert_eq!(v.len(), 3);
        assert!(v.windows(2).all(|w| w[0] < w[1]) && v.iter().all(|i| *i < 10));
        for i in &v {
            freq[*i as usize] += 1;
        }
        subsets.insert(v);
    }
    for (i, f) in freq.iter().enumerate() {
        let p = *f as f64 / 60_000.0;
        assert!((0.28..0.32).contains(&p), "index {i}: {p}");
    }
    assert_eq!(subsets.len(), 120, "all C(10,3) subsets occur");
    assert_eq!(stream(1, 1).distinct_indices(5, 6), Err(SampleRefusalV1::TooMany { count: 6, n: 5 }));
    assert_eq!(stream(1, 1).distinct_indices(5, 5).unwrap(), vec![0, 1, 2, 3, 4]);
    assert_eq!(stream(1, 1).distinct_indices(0, 0).unwrap(), Vec::<u64>::new());
}

/// EXPECTED (before running): the field sampler never returns p = 2^127 - 1 or anything past it, its top bit is set about half the
/// time, and a stream with a tiny word bound reports exhaustion instead of falling back to a biased value; streams of different
/// labels (kind, scope, relation, repetition) never share their first words. VERDICT: PASS.
#[test]
fn the_field_sampler_is_in_range_unbiased_bounded_and_label_separated() {
    let mut s = stream(3, 0);
    let mut top = 0u32;
    for _ in 0..100_000 {
        let x = s.field_m127().unwrap();
        assert!(x < P127);
        if x >> 126 == 1 {
            top += 1;
        }
    }
    assert!((48_000..52_000).contains(&top), "top bit {top}");
    let mut tiny = ChallengeStreamV1::with_bound(
        &[1; 64],
        &StreamLabelV1 { kind: StreamKindV1::Freivalds, scope_id: [0; 64], relation: 0, repetition: 0 },
        3,
    );
    assert!(tiny.field_m127().is_ok());
    assert_eq!(tiny.field_m127(), Err(SampleRefusalV1::Exhausted(3)), "a bound hit is an error, not a biased fallback");
    let mut firsts = BTreeSet::new();
    for kind in [
        StreamKindV1::Query,
        StreamKindV1::Segment,
        StreamKindV1::TensorRange,
        StreamKindV1::Vector,
        StreamKindV1::Freivalds,
        StreamKindV1::Aggregation,
        StreamKindV1::ProofRound,
    ] {
        for scope in [[0u8; 64], [1; 64]] {
            for relation in 0..3 {
                for repetition in 0..3 {
                    let mut st = ChallengeStreamV1::new(&[0x5E; 64], &StreamLabelV1 { kind, scope_id: scope, relation, repetition });
                    assert!(firsts.insert([st.next_u64().unwrap(), st.next_u64().unwrap()]), "two labels share their first words");
                }
            }
        }
    }
}

// ── transcripts ──────────────────────────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): staged rounds — the same beacon cannot serve two rounds with the same challenge (the round index and
/// message are in the preimage); a message committed at or after the round window's start is refused; the wrong mode is refused.
/// Fiat-Shamir — swapping, dropping, reordering or editing any (message, challenge) pair, or the statement/policy/source binding of
/// an otherwise self-consistent transcript, fails `verify` OR must be caught by the consumer comparing the header (the test asserts
/// which). VERDICT: PASS, with the consumer-comparison hazard recorded as GAP-C4-B.
#[test]
fn transcript_rounds_cannot_be_substituted_reordered_or_rebound() {
    let c = ctx(SubjectKindV1::WorkSlice);
    let b = locked(&c, &good(), 150);
    let base = [0x77; 64];
    let r0 = staged_round_challenge_v1(InteractiveModeV1::StagedBeacon, &base, 0, &[1; 64], 100, &b, 102).unwrap();
    let r1 = staged_round_challenge_v1(InteractiveModeV1::StagedBeacon, &base, 1, &[1; 64], 100, &b, 102).unwrap();
    let r0b = staged_round_challenge_v1(InteractiveModeV1::StagedBeacon, &base, 0, &[2; 64], 100, &b, 102).unwrap();
    assert!(r0 != r1 && r0 != r0b, "round index and message are bound");
    assert_eq!(
        staged_round_challenge_v1(InteractiveModeV1::StagedBeacon, &base, 1, &[1; 64], 102, &b, 102),
        Err(TranscriptRefusalV1::MessageAfterWindow { round: 1, message_position: 102, window_start: 102 })
    );
    assert!(staged_round_challenge_v1(InteractiveModeV1::NonInteractive, &base, 0, &[1; 64], 1, &b, 102).is_err());

    let mode = InteractiveModeV1::TranscriptBoundFiatShamir;
    let mut t = FiatShamirTranscriptV1::new([5; 64], c.policy.id(), &b);
    for m in 1..=4u8 {
        t.absorb(mode, [m; 64]).unwrap();
    }
    assert!(t.verify().is_ok());
    let mut swapped = t.clone();
    swapped.rounds.swap(1, 2);
    assert!(swapped.verify().is_err());
    let mut dropped = t.clone();
    dropped.rounds.remove(0);
    assert!(dropped.verify().is_err());
    let mut edited = t.clone();
    edited.rounds[2].0[0] ^= 1;
    assert!(edited.verify().is_err());
    let mut edited_c = t.clone();
    edited_c.rounds[3].1[0] ^= 1;
    assert!(edited_c.verify().is_err());
    // A self-consistent transcript built by someone else over another statement verifies: the CONSUMER must compare the header.
    let mut other = FiatShamirTranscriptV1::new([6; 64], c.policy.id(), &b);
    for m in 1..=4u8 {
        other.absorb(mode, [m; 64]).unwrap();
    }
    assert!(other.verify().is_ok(), "GAP-C4-B: verify() does not know which statement/policy/beacon it should be for");
    assert_ne!(other.statement_root, t.statement_root);
    assert_ne!(other.rounds, t.rounds, "but its challenges differ, so a consumer that compares statement_root is safe");
    // The beacon swap: same statement, another beacon.
    let mut c2 = c.clone();
    c2.challenge_epoch = 99;
    let b2 = locked(&c2, &good(), 150);
    assert_ne!(FiatShamirTranscriptV1::new([5; 64], c.policy.id(), &b2).source_binding, t.source_binding);
}

// ── policy ───────────────────────────────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): a policy of k = 1 (a one-source beacon: the last contributor chooses the seed) and a depth/window of
/// 1 are structurally VALID today and unapproved only because the registry is empty: the contract fixes no minimum k. Recorded as
/// observation O-C4-08, asserted descriptively. Every unknown id and every zero is refused; an approved tuple does not approve a
/// weaker repetition count or another policy id.
#[test]
fn observation_the_contract_fixes_no_minimum_k_and_approval_is_exact() {
    assert!(reference_policy_v1(1, 1, 1, 1, 1).validate().is_ok(), "O-C4-08: k = 1 / window 1 / depth 1 is structurally valid");
    for (k, d, w, dep, r) in [(0, 1, 1, 1, 1), (1, 0, 1, 1, 1), (1, 1, 0, 1, 1), (1, 1, 1, 0, 1), (1, 1, 1, 1, 0)] {
        assert!(matches!(reference_policy_v1(k, d, w, dep, r).validate(), Err(PolicyRefusalV1::Missing(_))));
    }
    let p = reference_policy_v1(3, 2, 40, 5, 4);
    let suite = named_id("suite");
    // Since round 1 a registry tuple also states its relations' soundness and its grinding / beacon / adaptive-query horizon, and
    // `approved_v1` never approves on the tuple alone: it then needs the effective bound and the full binding/source/coverage
    // review (`approved_v2`). What stays exact is the LOOKUP: only this suite and this policy id find the tuple.
    let reg = [ApprovedTupleV1 {
        checker_suite_id: suite,
        challenge_policy_id: p.id(),
        soundness_policy_id: p.soundness_policy_id,
        min_repetitions: 4,
        relations: Vec::new(),
        grinding_choices_per_beacon: 1,
        beacons_per_attempt: 1,
        adaptive_queries: 1,
    }];
    assert_ne!(approved_v1(&reg, &suite, &p), Err(PolicyRefusalV1::NotApproved), "the exact tuple is found");
    assert!(approved_v1(&reg, &suite, &p).is_err(), "and the tuple alone approves nothing");
    assert_eq!(approved_v1(&reg, &named_id("other suite"), &p), Err(PolicyRefusalV1::NotApproved));
    assert!(approved_v1(&reg, &suite, &reference_policy_v1(3, 2, 40, 5, 3)).is_err(), "fewer repetitions");
    assert!(approved_v1(&reg, &suite, &reference_policy_v1(2, 2, 40, 5, 4)).is_err(), "another k is another policy id");
    let mut q = p.clone();
    q.sampling_algorithm_id = named_id("sampler/other");
    assert_eq!(q.validate(), Err(PolicyRefusalV1::UnknownId("sampling_algorithm_id")));
    let mut q = p.clone();
    q.interactive_mode = InteractiveModeV1::StagedBeacon;
    assert_eq!(q.validate(), Err(PolicyRefusalV1::TranscriptMode));
    assert_ne!(ImplementedV1::transcript_none(), ImplementedV1::transcript_staged_beacon());
}

// ── lifecycle and conformance evidence ──────────────────────────────────────────────────────────────────────────────────

/// A model of the lifecycle written from the doc table, independent of the implementation: the only legal successful steps.
fn legal(state: OnboardingStateV1, step: &OnboardingStepV1) -> bool {
    use OnboardingStateV1 as S;
    use OnboardingStepV1 as P;
    matches!(
        (state, step),
        (S::SourceDiscovered, P::Frontend(_))
            | (S::FrontendReady, P::StaticAdmission(_))
            | (S::StaticAdmitted, P::Registered { .. })
            | (S::RegisteredDormant, P::ConformanceCommitted { .. })
            | (S::ChallengePending, P::BeaconUnavailable)
            | (S::ChallengePending, P::ConformanceChecked(_))
            | (S::ConformancePassed, P::PublicProsecutionGate { .. })
            | (S::G14Eligible, P::Activated { .. })
    )
}

/// EXPECTED (before running): for every state reachable by legal steps and every step (with Ok and every failure payload), the
/// record advances only through the table's legal transitions; every other pair is `OutOfOrder` and changes nothing; `Active` is
/// reachable only through a Passed conformance, a complete G14 gate and availability, in that order. VERDICT: PASS.
#[test]
fn the_onboarding_lifecycle_has_no_shortcut_to_active() {
    use OnboardingFailureV1 as F;
    use OnboardingStepV1 as P;
    let all_failures = [
        F::FrontendRequired,
        F::KernelExtensionRequired,
        F::KernelNotActive,
        F::LayoutRequired,
        F::ResourceRefused,
        F::ConformanceFailed,
        F::BeaconUnavailable,
        F::PublicProsecutionIncomplete,
        F::AvailabilityRequired,
    ];
    let mut steps: Vec<P> = vec![
        P::Frontend(Ok([1; 64])),
        P::StaticAdmission(Ok([2; 64])),
        P::Registered { class_id: [3; 64] },
        P::ConformanceCommitted { commitment_root: [4; 64] },
        P::BeaconUnavailable,
        P::ConformanceChecked(Ok([5; 64])),
        P::PublicProsecutionGate { complete: true },
        P::PublicProsecutionGate { complete: false },
        P::Activated { availability: true },
        P::Activated { availability: false },
    ];
    for f in all_failures {
        steps.push(P::Frontend(Err(f)));
        steps.push(P::StaticAdmission(Err(f)));
        steps.push(P::ConformanceChecked(Err(f)));
    }
    // The happy prefix, recording the record after each step.
    let prefix = [
        P::Frontend(Ok([1; 64])),
        P::StaticAdmission(Ok([2; 64])),
        P::Registered { class_id: [3; 64] },
        P::ConformanceCommitted { commitment_root: [4; 64] },
        P::ConformanceChecked(Ok([5; 64])),
        P::PublicProsecutionGate { complete: true },
    ];
    let mut r = OnboardingRecordV1::new(u32::MAX);
    let mut records = vec![r.clone()];
    for p in &prefix {
        r.apply(p.clone()).unwrap();
        records.push(r.clone());
    }
    for rec in &records {
        for step in &steps {
            let mut x = rec.clone();
            let res = x.apply(step.clone());
            if legal(rec.state, step) {
                // A legal step either advances or records a permitted failure; a forbidden failure payload is WrongFailure.
                if let Err(e) = &res {
                    assert!(
                        matches!(e, misaka_palw_challenge::lifecycle::OnboardingRefusalV1::WrongFailure(_)),
                        "{:?} {step:?}: {e}",
                        rec.state
                    );
                    assert_eq!(&x, rec, "a refused step changed the record");
                }
            } else {
                assert!(res.is_err(), "{:?} accepted {step:?}", rec.state);
                assert_eq!(&x, rec, "a refused step changed the record");
            }
            assert!(
                !x.state.rewardable()
                    || (rec.state == OnboardingStateV1::G14Eligible && matches!(step, P::Activated { availability: true }))
            );
        }
    }
}

/// GAP-C4-C, CLOSED since round 1: the record used to count only `BeaconUnavailable` against the policy's retry limit, so a
/// candidate whose conformance FAILED could commit again without limit and pass on a lucky draw (round 1: 60 failures then a pass).
/// `OnboardingRecordV1::new(attempt_limit)` now bounds every attempt that ended without a pass, failed ones included: once the
/// limit is spent the next commitment is refused (`AttemptsExhausted`) and the record is unchanged, so no draw is left to pass on.
#[test]
fn gap_a_failed_conformance_can_be_recommitted_without_limit_until_a_draw_passes() {
    use OnboardingFailureV1 as F;
    use OnboardingStepV1 as P;
    const LIMIT: u32 = 5;
    let mut r = OnboardingRecordV1::new(LIMIT);
    r.apply(P::Frontend(Ok([1; 64]))).unwrap();
    r.apply(P::StaticAdmission(Ok([2; 64]))).unwrap();
    r.apply(P::Registered { class_id: [3; 64] }).unwrap();
    for i in 0..LIMIT as u8 {
        r.apply(P::ConformanceCommitted { commitment_root: [i; 64] }).unwrap();
        r.apply(P::ConformanceChecked(Err(F::ConformanceFailed))).unwrap();
    }
    assert_eq!(r.attempts(), LIMIT, "failed attempts are counted");
    let before = r.clone();
    assert_eq!(
        r.apply(P::ConformanceCommitted { commitment_root: [99; 64] }),
        Err(misaka_palw_challenge::lifecycle::OnboardingRefusalV1::AttemptsExhausted(LIMIT)),
        "no commitment past the limit, so no lucky draw"
    );
    assert_eq!(r, before, "the refused commitment changed nothing");
    assert_ne!(r.state, OnboardingStateV1::ConformancePassed);
}

fn commitment(c: &BeaconContextV1) -> ConformanceCommitmentV1 {
    ConformanceCommitmentV1 {
        version: 1,
        chain_genesis: GENESIS,
        ruleset_id: RULESET,
        subject_kind: c.subject_kind,
        candidate_id: CANDIDATE,
        kernel_descriptor_id: [1; 64],
        challenge_policy_id: c.policy.id(),
        artifact_root: [4; 64],
        program_root: [3; 64],
        source_root: RootV1::Present([8; 64]),
        tokenizer_or_input_schema_root: RootV1::Absent,
        layout_root: [5; 64],
        verification_plan_root: [2; 64],
        constraint_root: RootV1::Absent,
        implementation_set_root: [6; 64],
        test_scope_root: [7; 64],
        calibration_id: RootV1::Absent,
        input_and_state_binding_root: RootV1::Absent,
        resource_profile_id: [9; 64],
        commitment_object_id: None,
        canonical_commitment_position: None,
    }
}

fn evidence(cm: &ConformanceCommitmentV1, b: &WorkBeaconV1, seed: Digest) -> BeaconConformanceEvidenceV1 {
    BeaconConformanceEvidenceV1 {
        version: 1,
        commitment_root: cm.statement_root(),
        challenge_policy_id: cm.challenge_policy_id,
        challenge_anchor: b.challenge_anchor,
        qualifying_source_evidence_root: [1; 64],
        lock_evidence_root: lock_evidence_root_v1(b),
        lock_position: b.lock_position,
        beacon_output: b.output,
        challenge_seed: seed,
        selected_vectors_root: [2; 64],
        selected_tensor_ranges_root: [3; 64],
        reference_result_root: [4; 64],
        independent_result_root: [5; 64],
        backend_result_root: [6; 64],
        authenticated_openings_root: [7; 64],
        transcript_root: RootV1::Absent,
        checks_required: 10,
        checks_run: 10,
        checks_failed: 0,
        missing_checks: vec![],
        failures: vec![],
        scope_and_fault_model_id: [8; 64],
        soundness_assumptions_root: [9; 64],
        derived_epsilon_bits: 40,
        status: ConformanceStatusV1::Passed,
        public_material_locator_root: [10; 64],
    }
}

/// EXPECTED (before running): conformance evidence passes only when its commitment, policy, beacon fields, seed, status, counters
/// and epsilon all match; each of 20 single-field forgeries (and a stale seed from another history) is refused; SKIPPED, INCOMPLETE,
/// FAILED and BEACON_UNAVAILABLE are never a pass, however the counters are set. VERDICT: PASS.
#[test]
fn conformance_evidence_is_recomputed_and_every_forged_field_or_non_pass_status_is_refused() {
    let c = ctx(SubjectKindV1::ModelConformance);
    let cm = commitment(&c);
    let mut cc = c.clone();
    cc.commitment_root = cm.statement_root();
    let b = locked(&cc, &good(), 150);
    let seed = challenge_seed_v1(&cc, &cm.subject(), &b).unwrap();
    let ok = evidence(&cm, &b, seed);
    assert!(verify_conformance_evidence_v1(&cm, &b, &seed, 40, &ok).is_ok());
    let forged: Vec<(&str, Box<dyn Fn(&mut BeaconConformanceEvidenceV1)>)> = vec![
        ("version", Box::new(|e| e.version = 2)),
        ("commitment", Box::new(|e| e.commitment_root[0] ^= 1)),
        ("policy", Box::new(|e| e.challenge_policy_id[0] ^= 1)),
        ("anchor", Box::new(|e| e.challenge_anchor[0] ^= 1)),
        ("lock evidence", Box::new(|e| e.lock_evidence_root[0] ^= 1)),
        ("lock position", Box::new(|e| e.lock_position += 1)),
        ("output", Box::new(|e| e.beacon_output[0] ^= 1)),
        ("seed", Box::new(|e| e.challenge_seed[0] ^= 1)),
        ("assumptions", Box::new(|e| e.soundness_assumptions_root = [0; 64])),
        ("status skipped", Box::new(|e| e.status = ConformanceStatusV1::Skipped)),
        ("status incomplete", Box::new(|e| e.status = ConformanceStatusV1::Incomplete)),
        ("status failed", Box::new(|e| e.status = ConformanceStatusV1::Failed)),
        ("status unavailable", Box::new(|e| e.status = ConformanceStatusV1::BeaconUnavailable)),
        (
            "zero required",
            Box::new(|e| {
                e.checks_required = 0;
                e.checks_run = 0;
            }),
        ),
        ("fewer run", Box::new(|e| e.checks_run = 9)),
        ("more run", Box::new(|e| e.checks_run = 11)),
        ("one failed", Box::new(|e| e.checks_failed = 1)),
        ("one missing", Box::new(|e| e.missing_checks = vec!["x".into()])),
        ("weak epsilon", Box::new(|e| e.derived_epsilon_bits = 39)),
        (
            "skipped with passing counters",
            Box::new(|e| {
                e.status = ConformanceStatusV1::Skipped;
                e.checks_run = e.checks_required;
            }),
        ),
    ];
    for (what, f) in &forged {
        let mut e = ok.clone();
        f(&mut e);
        assert!(verify_conformance_evidence_v1(&cm, &b, &seed, 40, &e).is_err(), "{what} was accepted");
    }
    // A stale seed from another history.
    let mut events2 = good();
    events2[1] = work(9, B, 104, 111);
    let b2 = locked(&cc, &events2, 150);
    let seed2 = challenge_seed_v1(&cc, &cm.subject(), &b2).unwrap();
    assert_eq!(verify_conformance_evidence_v1(&cm, &b2, &seed2, 40, &ok), Err(ConformanceRefusalV1::Beacon));
    // Another commitment (artifact changed after commit) is another statement.
    let mut cm2 = cm.clone();
    cm2.artifact_root[0] ^= 1;
    assert_eq!(verify_conformance_evidence_v1(&cm2, &b, &seed, 40, &ok), Err(ConformanceRefusalV1::Commitment));
}

/// EXPECTED (before running): evidence that says `Passed` with all counters clean but LISTS failures is internally inconsistent
/// and must not pass. FAILS today: `failures` is never read by `verify_conformance_evidence_v1`.
#[test]
#[ignore = "FAIL F-C4-08: Passed evidence that lists failures is accepted (the failures vector is never read)"]
fn passed_evidence_that_lists_failures_must_not_pass() {
    let c = ctx(SubjectKindV1::ModelConformance);
    let cm = commitment(&c);
    let mut cc = c.clone();
    cc.commitment_root = cm.statement_root();
    let b = locked(&cc, &good(), 150);
    let seed = challenge_seed_v1(&cc, &cm.subject(), &b).unwrap();
    let mut e = evidence(&cm, &b, seed);
    e.failures = vec!["vector 3 diverged at position 2".into()];
    assert!(verify_conformance_evidence_v1(&cm, &b, &seed, 40, &e).is_err());
}
