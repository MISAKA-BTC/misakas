//! The shared challenge contract: policy refusals, beacon source rules, seed separation, samplers, transcripts, lifecycle and
//! conformance evidence — each rule exercised by the attack it exists for.

use std::collections::BTreeSet;

use misaka_palw_challenge::beacon::{BeaconEvidenceRefusalV1, IneligibleV1, eligibility_v1};
use misaka_palw_challenge::conformance::ConformanceRefusalV1;
use misaka_palw_challenge::hash::{Digest, hex, named_id};
use misaka_palw_challenge::policy::{ImplementedV1, PolicyRefusalV1};
use misaka_palw_challenge::seed::{P127, SeedRefusalV1, TranscriptRefusalV1};
use misaka_palw_challenge::*;

const GENESIS: Digest = [0x11; 64];
const RULESET: Digest = [0x22; 64];
const CANDIDATE: Digest = [0xCA; 64];
const ACTIVE_A: Digest = [0xA1; 64];
const ACTIVE_B: Digest = [0xA2; 64];
const DORMANT: Digest = [0xD0; 64];

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
        eligible_profiles: [ACTIVE_A, ACTIVE_B].into_iter().collect(),
        excluded_profiles: [CANDIDATE].into_iter().collect(),
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
        final_path: FinalPathV1::PanelLicensed { panel_seed_id: [0x5E; 64], panel_epoch: 1 },
    }
}

fn three_good() -> Vec<WorkFinalEventV1> {
    // S = 102, window [102, 142).
    vec![work(1, ACTIVE_A, 103, 110), work(2, ACTIVE_B, 104, 111), work(3, ACTIVE_A, 105, 112)]
}

fn locked(ctx: &BeaconContextV1, events: &[WorkFinalEventV1], tip: u64) -> WorkBeaconV1 {
    match collect_work_beacon_v1(ctx, events, tip).unwrap() {
        WorkBeaconStateV1::Locked(b) => b,
        other => panic!("not locked: {other:?}"),
    }
}

// ── policy ────────────────────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn the_policy_refuses_every_unknown_algorithm_missing_value_and_mismatched_transcript() {
    policy().validate().unwrap();
    let mut p = policy();
    p.version = 2;
    assert_eq!(p.validate(), Err(PolicyRefusalV1::Version(2)));
    let foreign = named_id("someone-elses-sampler");
    for (name, edit) in [
        ("randomness_source_policy_id", (|p: &mut PostCommitChallengePolicyV1, d| p.randomness_source_policy_id = d) as fn(&mut _, _)),
        ("source_eligibility_policy_id", |p, d| p.source_eligibility_policy_id = d),
        ("hash_suite_id", |p, d| p.hash_suite_id = d),
        ("sampling_algorithm_id", |p, d| p.sampling_algorithm_id = d),
        ("field_sampling_algorithm_id", |p, d| p.field_sampling_algorithm_id = d),
        ("reorg_policy_id", |p, d| p.reorg_policy_id = d),
    ] {
        let mut p = policy();
        edit(&mut p, foreign);
        assert_eq!(p.validate(), Err(PolicyRefusalV1::UnknownId(name)));
    }
    for (name, edit) in [
        ("work_count_k", (|p: &mut PostCommitChallengePolicyV1| p.work_count_k = 0) as fn(&mut _)),
        ("anchor_delay_slots", |p| p.anchor_delay_slots = 0),
        ("beacon_window_slots", |p| p.beacon_window_slots = 0),
        ("settlement_depth_d", |p| p.settlement_depth_d = 0),
        ("repetition_count", |p| p.repetition_count = 0),
        ("soundness_policy_id", |p| p.soundness_policy_id = [0; 64]),
    ] {
        let mut p = policy();
        edit(&mut p);
        assert_eq!(p.validate(), Err(PolicyRefusalV1::Missing(name)));
    }
    let mut p = policy();
    p.interactive_mode = InteractiveModeV1::StagedBeacon;
    assert_eq!(p.validate(), Err(PolicyRefusalV1::TranscriptMode), "a GKR mode needs its own transcript transform");
    p.transcript_transform_id = ImplementedV1::transcript_staged_beacon();
    p.validate().unwrap();
}

#[test]
fn nothing_is_approved_by_default_and_a_weaker_repetition_is_refused() {
    let p = policy();
    let suite = named_id("checker-suite/k2-freivalds/v1");
    assert_eq!(approved_v1(&[], &suite, &p), Err(PolicyRefusalV1::NotApproved));
    let tuple = ApprovedTupleV1 {
        checker_suite_id: suite,
        challenge_policy_id: p.id(),
        soundness_policy_id: p.soundness_policy_id,
        min_repetitions: 4,
    };
    approved_v1(&[tuple], &suite, &p).unwrap();
    let mut weaker = p.clone();
    weaker.repetition_count = 3;
    assert_eq!(approved_v1(&[tuple], &suite, &weaker), Err(PolicyRefusalV1::NotApproved), "another id, fewer repetitions");
    assert_ne!(p.id(), weaker.id(), "every field is in the policy id");
}

// ── beacon sources ────────────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn only_fresh_final_da_satisfied_useful_work_of_active_g14_profiles_is_a_source() {
    let c = ctx(SubjectKindV1::ModelConformance);
    let ok = work(9, ACTIVE_A, 103, 110);
    eligibility_v1(&c, &ok).unwrap();
    for kind in [
        WorkSourceKindV1::Heartbeat,
        WorkSourceKindV1::Base0Fallback,
        WorkSourceKindV1::ExecTx,
        WorkSourceKindV1::ExecWorkSlice,
        WorkSourceKindV1::ReceiptOnly,
        WorkSourceKindV1::ProvisionalAttempt,
        WorkSourceKindV1::PanelReceipt,
    ] {
        let mut e = ok.clone();
        e.kind = kind;
        assert_eq!(eligibility_v1(&c, &e), Err(IneligibleV1::NotUsefulWork(kind)));
    }
    let with = |f: fn(&mut WorkFinalEventV1)| {
        let mut e = ok.clone();
        f(&mut e);
        eligibility_v1(&c, &e)
    };
    assert_eq!(with(|e| e.source_profile_id = CANDIDATE), Err(IneligibleV1::SelfOrDependent), "no self-beacon");
    assert_eq!(with(|e| e.depends_on_profiles = vec![CANDIDATE]), Err(IneligibleV1::SelfOrDependent));
    assert_eq!(with(|e| e.source_profile_id = DORMANT), Err(IneligibleV1::ProfileNotEligible), "not Active+G14 at commitment");
    assert_eq!(with(|e| e.accepted_position = 101), Err(IneligibleV1::NotFresh), "committed before S, Final after it");
    assert_eq!(with(|e| e.settlement_position = 142), Err(IneligibleV1::OutsideWindow));
    assert_eq!(with(|e| e.claim_final = false), Err(IneligibleV1::NotFinal), "unfinalized work");
    assert_eq!(with(|e| e.da_satisfied = false), Err(IneligibleV1::DaUnsatisfied));
    assert_eq!(with(|e| e.validity_independent = false), Err(IneligibleV1::NotIndependent));
}

#[test]
fn a_panel_assignment_beacon_refuses_panel_licensed_finals_so_the_draw_never_seeds_itself() {
    let c = ctx(SubjectKindV1::PanelAssignment);
    let licensed = work(9, ACTIVE_A, 103, 110);
    assert_eq!(eligibility_v1(&c, &licensed), Err(IneligibleV1::PanelDependentFinal), "Panel → Final → beacon → Panel");
    let mut independent = licensed.clone();
    independent.final_path = FinalPathV1::PanelIndependent;
    eligibility_v1(&c, &independent).unwrap();
    // Today every Final is Panel-licensed: a Panel-assignment beacon is unavailable, never a fallback.
    assert_eq!(collect_work_beacon_v1(&c, &three_good(), 142).unwrap(), WorkBeaconStateV1::Unavailable { have: 0, need: 3 });
    // Other subjects may use Panel-licensed sources (their veto bias is a reviewed budget, not a proof).
    eligibility_v1(&ctx(SubjectKindV1::ModelConformance), &licensed).unwrap();
}

#[test]
fn the_beacon_is_canonical_order_dedup_lock_at_depth_and_unavailable_never_falls_back() {
    let c = ctx(SubjectKindV1::ModelConformance);
    let events = three_good();
    // Before depth D: a candidate; at depth: locked.
    assert_eq!(collect_work_beacon_v1(&c, &events, 116).unwrap(), WorkBeaconStateV1::Candidate { have: 3, lock_position: 117 });
    let b = locked(&c, &events, 117);
    assert_eq!(b.sources.len(), 3);
    assert_eq!(b.accumulators.len(), 4);
    // Arrival order never matters.
    let mut shuffled = events.clone();
    shuffled.reverse();
    assert_eq!(locked(&c, &shuffled, 117), b, "node arrival order is not an input");
    // A duplicate / reattached work identity cannot enlarge the set; noise kinds never count.
    let mut noisy = events.clone();
    let mut dup = events[0].clone();
    dup.settlement_position = 113;
    dup.occurrence_index = 1;
    noisy.push(dup);
    let mut hb = work(4, ACTIVE_A, 103, 109);
    hb.kind = WorkSourceKindV1::Heartbeat;
    noisy.push(hb);
    assert_eq!(locked(&c, &noisy, 117), b);
    // A work committed before S and reattached after it is still not fresh (its earliest acceptance is what counts).
    let mut stale = work(5, ACTIVE_B, 99, 108);
    stale.occurrence_index = 0;
    let mut reattached = stale.clone();
    reattached.settlement_position = 109;
    assert_eq!(locked(&c, &[stale, reattached, events[0].clone(), events[1].clone(), events[2].clone()], 117), b);
    // Too few qualifying works: collecting inside the window, unavailable after it — never a fallback source.
    let two = &events[..2];
    assert_eq!(collect_work_beacon_v1(&c, two, 130).unwrap(), WorkBeaconStateV1::Collecting { have: 2, need: 3 });
    assert_eq!(collect_work_beacon_v1(&c, two, 142).unwrap(), WorkBeaconStateV1::Unavailable { have: 2, need: 3 });
    let mut flood = two.to_vec();
    for n in 20..60 {
        let mut e = work(n, ACTIVE_A, 103, 120);
        e.kind = if n % 2 == 0 { WorkSourceKindV1::Heartbeat } else { WorkSourceKindV1::Base0Fallback };
        flood.push(e);
    }
    assert_eq!(collect_work_beacon_v1(&c, &flood, 142).unwrap(), WorkBeaconStateV1::Unavailable { have: 2, need: 3 });
}

#[test]
fn a_fresh_node_refuses_reordered_substituted_or_forged_beacon_evidence_and_a_reorg_recomputes() {
    let c = ctx(SubjectKindV1::ModelConformance);
    let events = three_good();
    let b = locked(&c, &events, 117);
    verify_work_beacon_v1(&c, &b, &events, 200).unwrap();
    let mut reordered = b.clone();
    reordered.sources.swap(0, 1);
    assert!(matches!(verify_work_beacon_v1(&c, &reordered, &events, 200), Err(BeaconEvidenceRefusalV1::Mismatch { at: 0 })));
    let mut substituted = b.clone();
    substituted.sources[2].canonical_work_id = [0x77; 64];
    assert!(matches!(verify_work_beacon_v1(&c, &substituted, &events, 200), Err(BeaconEvidenceRefusalV1::Mismatch { at: 2 })));
    let mut forged = b.clone();
    forged.output = [0x42; 64];
    assert_eq!(verify_work_beacon_v1(&c, &forged, &events, 200), Err(BeaconEvidenceRefusalV1::Derivation));
    // A reorg that replaces a source before (or after) the lock: the new branch derives its own beacon, and the old one is refused.
    let mut branch = events.clone();
    branch[2] = work(8, ACTIVE_B, 106, 112);
    let b2 = locked(&c, &branch, 117);
    assert_ne!(b2.output, b.output);
    assert!(verify_work_beacon_v1(&c, &b, &branch, 200).is_err());
    // A reorg that removes a source after lock rolls the lock back on that branch.
    assert!(matches!(verify_work_beacon_v1(&c, &b, &events[..2], 130), Err(BeaconEvidenceRefusalV1::NotLocked(_))));
    // Restart / IBD: the same events give the same beacon.
    assert_eq!(locked(&c, &events.clone(), 117), b);
}

// ── seed and streams ──────────────────────────────────────────────────────────────────────────────────────────────────────

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

#[test]
fn every_subject_kind_and_every_bound_root_has_its_own_seed_and_a_mismatched_subject_is_refused() {
    let mut seeds = BTreeSet::new();
    for kind in SubjectKindV1::ALL {
        let c = ctx(kind);
        let mut events = three_good();
        if kind == SubjectKindV1::PanelAssignment {
            events.iter_mut().for_each(|e| e.final_path = FinalPathV1::PanelIndependent);
        }
        let b = locked(&c, &events, 117);
        assert!(seeds.insert(challenge_seed_v1(&c, &subject(&c), &b).unwrap()), "{kind:?} shares a seed");
    }
    let c = ctx(SubjectKindV1::ModelConformance);
    let b = locked(&c, &three_good(), 117);
    let base = challenge_seed_v1(&c, &subject(&c), &b).unwrap();
    let mut s = subject(&c);
    s.layout_root = RootV1::Present([6; 64]);
    assert_ne!(challenge_seed_v1(&c, &s, &b).unwrap(), base, "a stale layout is another statement");
    let mut s = subject(&c);
    s.tokenizer_or_schema_root = RootV1::Present([0; 64]);
    assert_ne!(challenge_seed_v1(&c, &s, &b).unwrap(), base, "typed absence is not a zero root");
    let mut s = subject(&c);
    s.challenge_policy_id = named_id("weaker");
    assert_eq!(challenge_seed_v1(&c, &s, &b), Err(SeedRefusalV1::PolicyMismatch), "challenge policy substitution");
    let mut s = subject(&c);
    s.subject_kind = SubjectKindV1::PublicProsecution;
    assert_eq!(challenge_seed_v1(&c, &s, &b), Err(SeedRefusalV1::SubjectMismatch));
    let mut s = subject(&c);
    s.commitment_root = [0xEE; 64];
    assert_eq!(challenge_seed_v1(&c, &s, &b), Err(SeedRefusalV1::SubjectMismatch), "artifact changed after commitment");
}

#[test]
fn streams_are_label_separated_rejection_sampled_bounded_and_pinned_by_golden_vectors() {
    let c = ctx(SubjectKindV1::ClaimVerification);
    let b = locked(&c, &three_good(), 117);
    let seed = challenge_seed_v1(&c, &subject(&c), &b).unwrap();
    let label = |kind, repetition| StreamLabelV1 { kind, scope_id: [9; 64], relation: 3, repetition };
    let mut a = ChallengeStreamV1::new(&seed, &label(StreamKindV1::Freivalds, 0));
    let mut a2 = ChallengeStreamV1::new(&seed, &label(StreamKindV1::Freivalds, 0));
    let mut other = ChallengeStreamV1::new(&seed, &label(StreamKindV1::Freivalds, 1));
    let mut query = ChallengeStreamV1::new(&seed, &label(StreamKindV1::Query, 0));
    let (x, x2, y, z) = (a.next_u64().unwrap(), a2.next_u64().unwrap(), other.next_u64().unwrap(), query.next_u64().unwrap());
    assert_eq!(x, x2, "deterministic");
    assert!(x != y && x != z && y != z, "repetitions and kinds are separate streams");
    for n in [1u64, 2, 3, 7, 1000, (1 << 63) + 1, u64::MAX] {
        for _ in 0..50 {
            assert!(a.index_below(n).unwrap() < n);
        }
    }
    let picks = a.distinct_indices(100, 30).unwrap();
    assert_eq!(picks.len(), 30);
    assert!(picks.windows(2).all(|w| w[0] < w[1]) && picks.iter().all(|i| *i < 100));
    assert_eq!(a.distinct_indices(5, 5).unwrap(), vec![0, 1, 2, 3, 4]);
    assert!(a.distinct_indices(5, 6).is_err());
    for _ in 0..100 {
        assert!(a.field_m127().unwrap() < P127);
    }
    let mut tiny = ChallengeStreamV1::with_bound(&seed, &label(StreamKindV1::Vector, 0), 3);
    tiny.next_u64().unwrap();
    tiny.next_u64().unwrap();
    tiny.next_u64().unwrap();
    assert!(tiny.next_u64().is_err(), "a named exhaustion, never a biased fallback");
    // Golden vectors: any change to a domain, encoding or derivation changes these.
    let g = ChallengeStreamV1::new(&seed, &label(StreamKindV1::Freivalds, 0)).next_u64().unwrap();
    assert_eq!(&hex(&c.policy.id())[..16], GOLDEN_POLICY_ID_PREFIX, "policy id");
    assert_eq!(&hex(&b.output)[..16], GOLDEN_BEACON_PREFIX, "beacon output");
    assert_eq!(&hex(&seed)[..16], GOLDEN_SEED_PREFIX, "challenge seed");
    assert_eq!(g, GOLDEN_FIRST_WORD, "first Freivalds word");
}

const GOLDEN_POLICY_ID_PREFIX: &str = "927ed75be91eb743";
const GOLDEN_BEACON_PREFIX: &str = "ac77cc132c9a804e";
const GOLDEN_SEED_PREFIX: &str = "7235de02dfc706e7";
const GOLDEN_FIRST_WORD: u64 = 1131241806272782869;

// ── interactive transcripts ───────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_staged_round_needs_its_message_committed_before_its_window_and_a_fiat_shamir_transcript_absorbs_every_prior_round() {
    let c = ctx(SubjectKindV1::ClaimVerification);
    let b = locked(&c, &three_good(), 117);
    let seed = challenge_seed_v1(&c, &subject(&c), &b).unwrap();
    let staged = InteractiveModeV1::StagedBeacon;
    let c1 = staged_round_challenge_v1(staged, &seed, 1, &[0xAB; 64], 150, &b, 151).unwrap();
    assert_eq!(
        staged_round_challenge_v1(staged, &seed, 1, &[0xAB; 64], 151, &b, 151),
        Err(TranscriptRefusalV1::MessageAfterWindow { round: 1, message_position: 151, window_start: 151 }),
        "a message chosen after its round's randomness started forming"
    );
    assert_ne!(staged_round_challenge_v1(staged, &seed, 1, &[0xAC; 64], 150, &b, 151).unwrap(), c1);
    assert!(staged_round_challenge_v1(InteractiveModeV1::NonInteractive, &seed, 1, &[0xAB; 64], 150, &b, 151).is_err());

    let fs = InteractiveModeV1::TranscriptBoundFiatShamir;
    let mut t = FiatShamirTranscriptV1::new([0x5A; 64], c.policy.id(), &b);
    let r0 = t.absorb(fs, [1; 64]).unwrap();
    let r1 = t.absorb(fs, [2; 64]).unwrap();
    t.verify().unwrap();
    let mut u = FiatShamirTranscriptV1::new([0x5A; 64], c.policy.id(), &b);
    u.absorb(fs, [9; 64]).unwrap();
    assert_ne!(u.absorb(fs, [2; 64]).unwrap(), r1, "a later challenge depends on every earlier message");
    let mut tampered = t.clone();
    tampered.rounds[0].0 = [3; 64];
    assert_eq!(tampered.verify(), Err(TranscriptRefusalV1::Mismatch(0)));
    assert_ne!(r0, r1);
}

// ── lifecycle ─────────────────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn the_lifecycle_separates_every_stage_and_classifies_failures() {
    use OnboardingFailureV1 as F;
    use OnboardingStateV1 as S;
    use OnboardingStepV1 as P;
    let mut r = OnboardingRecordV1::new();
    // Converted is not registered, registered is not active.
    assert!(r.apply(P::Registered { class_id: [1; 64] }).is_err());
    r.apply(P::Frontend(Err(F::FrontendRequired))).unwrap();
    assert_eq!(r.status(), ("SOURCE_DISCOVERED", Some("FRONTEND_REQUIRED")));
    assert!(r.apply(P::Frontend(Err(F::KernelExtensionRequired))).is_err(), "a frontend never reports a kernel gap");
    r.apply(P::Frontend(Ok([2; 64]))).unwrap();
    r.apply(P::StaticAdmission(Err(F::KernelExtensionRequired))).unwrap();
    assert_eq!(r.status(), ("FRONTEND_READY", Some("KERNEL_EXTENSION_REQUIRED")));
    assert!(r.apply(P::StaticAdmission(Err(F::BeaconUnavailable))).is_err(), "a beacon never decides semantics");
    r.apply(P::StaticAdmission(Ok([3; 64]))).unwrap();
    r.apply(P::Registered { class_id: [4; 64] }).unwrap();
    assert!(!r.state.rewardable());
    assert!(r.apply(P::Activated { availability: true }).is_err(), "registered is not active");
    r.apply(P::ConformanceCommitted { commitment_root: [5; 64] }).unwrap();
    r.apply(P::BeaconUnavailable).unwrap();
    assert_eq!((r.state, r.beacon_retries, r.last_failure), (S::RegisteredDormant, 1, Some(F::BeaconUnavailable)));
    r.apply(P::ConformanceCommitted { commitment_root: [6; 64] }).unwrap();
    r.apply(P::ConformanceChecked(Err(F::ConformanceFailed))).unwrap();
    assert_eq!(r.state, S::RegisteredDormant, "a failed conformance needs a new commitment");
    r.apply(P::ConformanceCommitted { commitment_root: [7; 64] }).unwrap();
    r.apply(P::ConformanceChecked(Ok([8; 64]))).unwrap();
    r.apply(P::PublicProsecutionGate { complete: false }).unwrap();
    assert_eq!(r.status(), ("CONFORMANCE_PASSED", Some("PUBLIC_PROSECUTION_INCOMPLETE")));
    assert!(r.apply(P::Activated { availability: true }).is_err(), "no activation without G14");
    r.apply(P::PublicProsecutionGate { complete: true }).unwrap();
    r.apply(P::Activated { availability: false }).unwrap();
    assert_eq!(r.status(), ("G14_ELIGIBLE", Some("AVAILABILITY_REQUIRED")));
    r.apply(P::Activated { availability: true }).unwrap();
    assert!(r.state.rewardable());
}

// ── conformance evidence ──────────────────────────────────────────────────────────────────────────────────────────────────

fn commitment(c: &BeaconContextV1) -> ConformanceCommitmentV1 {
    ConformanceCommitmentV1 {
        version: 1,
        chain_genesis: GENESIS,
        ruleset_id: RULESET,
        subject_kind: SubjectKindV1::ModelConformance,
        candidate_id: CANDIDATE,
        kernel_descriptor_id: [1; 64],
        challenge_policy_id: c.policy.id(),
        artifact_root: [4; 64],
        program_root: [3; 64],
        tokenizer_or_input_schema_root: RootV1::Absent,
        layout_root: [5; 64],
        verification_plan_root: [2; 64],
        constraint_root: RootV1::Absent,
        implementation_set_root: [8; 64],
        test_scope_root: [9; 64],
        calibration_id: RootV1::Absent,
        input_and_state_binding_root: RootV1::Absent,
        resource_profile_id: [10; 64],
        commitment_object_id: None,
        canonical_commitment_position: None,
    }
}

#[test]
fn conformance_evidence_is_recomputed_and_skipped_missing_forged_or_substituted_evidence_never_passes() {
    let mut c = ctx(SubjectKindV1::ModelConformance);
    let com = commitment(&c);
    com.well_formed().unwrap();
    // Provenance stays outside the statement; every bound root is inside it.
    let mut placed = com.clone();
    placed.commitment_object_id = Some([0xEE; 64]);
    placed.canonical_commitment_position = Some(100);
    assert_eq!(placed.statement_root(), com.statement_root());
    for edit in [
        (|m: &mut ConformanceCommitmentV1| m.artifact_root = [0x44; 64]) as fn(&mut _),
        |m| m.layout_root = [0x55; 64],
        |m| m.verification_plan_root = [0x22; 64],
        |m| m.implementation_set_root = [0x88; 64],
        |m| m.challenge_policy_id = [0x99; 64],
    ] {
        let mut m = com.clone();
        edit(&mut m);
        assert_ne!(m.statement_root(), com.statement_root(), "a change after commitment is a new commitment");
    }
    c.commitment_root = com.statement_root();
    let b = locked(&c, &three_good(), 117);
    let seed = challenge_seed_v1(&c, &com.subject(), &b).unwrap();
    let good = BeaconConformanceEvidenceV1 {
        version: 1,
        commitment_root: com.statement_root(),
        challenge_policy_id: com.challenge_policy_id,
        challenge_anchor: b.challenge_anchor,
        qualifying_source_evidence_root: [0; 64],
        lock_position: b.lock_position,
        beacon_output: b.output,
        challenge_seed: seed,
        selected_vectors_root: [1; 64],
        selected_tensor_ranges_root: [1; 64],
        reference_result_root: [1; 64],
        independent_result_root: [1; 64],
        backend_result_root: [1; 64],
        authenticated_openings_root: [1; 64],
        transcript_root: RootV1::Absent,
        checks_required: 12,
        checks_run: 12,
        checks_failed: 0,
        missing_checks: vec![],
        failures: vec![],
        scope_and_fault_model_id: [1; 64],
        derived_epsilon_bits: 40,
        status: ConformanceStatusV1::Passed,
        public_material_locator_root: [1; 64],
    };
    verify_conformance_evidence_v1(&com, &b, &seed, 40, &good).unwrap();
    let judge = |f: fn(&mut BeaconConformanceEvidenceV1)| {
        let mut e = good.clone();
        f(&mut e);
        verify_conformance_evidence_v1(&com, &b, &seed, 40, &e)
    };
    assert_eq!(judge(|e| e.status = ConformanceStatusV1::Skipped), Err(ConformanceRefusalV1::NotPassed(ConformanceStatusV1::Skipped)));
    assert!(matches!(judge(|e| e.checks_run = 11), Err(ConformanceRefusalV1::Incomplete { .. })));
    assert!(matches!(judge(|e| e.missing_checks = vec!["layer 3".into()]), Err(ConformanceRefusalV1::Incomplete { .. })));
    assert!(matches!(judge(|e| e.checks_required = 0), Err(ConformanceRefusalV1::Incomplete { .. })));
    assert_eq!(judge(|e| e.challenge_seed = [0x13; 64]), Err(ConformanceRefusalV1::Seed), "forged evidence");
    assert_eq!(judge(|e| e.beacon_output = [0x13; 64]), Err(ConformanceRefusalV1::Beacon));
    assert_eq!(judge(|e| e.challenge_policy_id = [0x13; 64]), Err(ConformanceRefusalV1::Policy));
    assert_eq!(judge(|e| e.commitment_root = [0x13; 64]), Err(ConformanceRefusalV1::Commitment));
    assert_eq!(judge(|e| e.derived_epsilon_bits = 39), Err(ConformanceRefusalV1::WeakEpsilon));
}
