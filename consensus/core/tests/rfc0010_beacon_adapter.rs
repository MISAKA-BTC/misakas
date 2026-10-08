//! **RFC-0010's production beacon adapter** (`palw_panel_beacon_v1`): a Panel binding's entropy is a PALW Work Beacon of subject
//! kind `PANEL_ASSIGNMENT`, verified by every node against its OWN settlements — and on today's chain none can exist.
//!
//! The history here is synthetic ([`History`]); the chain-derived history, and the fold that ends a claim `BeaconUnavailable`
//! when no proof arrives, are `rfc0010_production_fold.rs`'s. Every refusal below is a forbidden source or a manipulated
//! contribution the contract (`misaka-palw-challenge`) must not let seed a Panel:
//!
//! * a Panel-licensed Final (the circularity work → Panel → Final → beacon → Panel);
//! * a heartbeat, BASE-0 fallback, EXEC, receipt-only, provisional or bare Panel-receipt contribution;
//! * the claims being assigned (self-candidate), a duplicate, a reordered or a stale (pre-start) contribution;
//! * a scheme this release does not approve (the shipped registry is empty), a forged output or accumulator.

use std::collections::BTreeSet;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_panel_beacon_v1::{
    PanelBeaconHistoryV1, PanelBeaconRefusalV1, approved_panel_beacon_policies_v1, panel_beacon_context_v1, panel_beacon_scheme_of_v1,
    panel_beacon_state_v1, verify_panel_beacon_for_engine_v1, verify_panel_beacon_v1,
};
use kaspa_consensus_core::palw_permissionless_panel_v1::{BeaconProofV1, BeaconRequestV1, PanelErrorV1};
use misaka_palw_challenge::beacon::{BeaconEvidenceRefusalV1, BeaconSourceV1, challenge_anchor_v1, initial_accumulator_v1, mix_v1};
use misaka_palw_challenge::hash::Digest;
use misaka_palw_challenge::policy::reference_policy_v1;
use misaka_palw_challenge::{
    FinalPathV1, PostCommitChallengePolicyV1, WorkBeaconStateV1, WorkBeaconV1, WorkFinalEventV1, WorkSourceKindV1, collect_work_beacon_v1,
};

const RELEASE: u64 = 1_100;
const PROFILE: Digest = [0x77; 64];

fn policy(k: u32) -> PostCommitChallengePolicyV1 {
    reference_policy_v1(k, 1, 10, 1, 1)
}

fn request(policy: &PostCommitChallengePolicyV1) -> BeaconRequestV1 {
    BeaconRequestV1 {
        network: Hash64::from_u64_word(0xA1),
        ruleset: Hash64::from_u64_word(0xA2),
        scheme: panel_beacon_scheme_of_v1(policy),
        epoch: 11,
        release_daa: RELEASE,
        deadline_daa: RELEASE + 8,
    }
}

fn event(work: u8, accepted: u64, settled: u64) -> WorkFinalEventV1 {
    WorkFinalEventV1 {
        kind: WorkSourceKindV1::RealUsefulWork,
        source_profile_id: PROFILE,
        canonical_work_id: [work; 64],
        execution_commitment: [work; 64],
        accepted_position: accepted,
        settlement_position: settled,
        occurrence_index: 0,
        claim_final: true,
        da_satisfied: true,
        validity_independent: true,
        depends_on_profiles: Vec::new(),
        final_path: FinalPathV1::PanelIndependent,
    }
}

#[derive(Clone)]
struct History {
    events: Vec<WorkFinalEventV1>,
    eligible: BTreeSet<Digest>,
    pending: BTreeSet<Digest>,
    tip: u64,
}

impl History {
    fn new(events: Vec<WorkFinalEventV1>) -> Self {
        Self { events, eligible: BTreeSet::from([PROFILE]), pending: BTreeSet::new(), tip: RELEASE + 6 }
    }
}

impl PanelBeaconHistoryV1 for History {
    fn final_events(&self) -> Vec<WorkFinalEventV1> {
        self.events.clone()
    }
    fn tip_position(&self) -> u64 {
        self.tip
    }
    fn eligible_profiles(&self) -> BTreeSet<Digest> {
        self.eligible.clone()
    }
    fn pending_work_of_epoch(&self, _: u64) -> BTreeSet<Digest> {
        self.pending.clone()
    }
}

/// The honest proof for `history`: what a producer builds from the same settlements.
fn honest(policy: &PostCommitChallengePolicyV1, history: &History) -> BeaconProofV1 {
    let request = request(policy);
    let context = panel_beacon_context_v1(&request, policy, history.eligible.clone());
    let WorkBeaconStateV1::Locked(beacon) = collect_work_beacon_v1(&context, &history.events, history.tip).unwrap() else {
        panic!("the history locks a beacon");
    };
    BeaconProofV1 { epoch: request.epoch, output: Hash64::from_bytes(beacon.output), proof: borsh::to_vec(&beacon).unwrap() }
}

/// A beacon built BY HAND from `sources`, as an adversary would: every accumulator, the anchor and the output computed correctly
/// for exactly these sources, over the epoch's context. The contract must refuse it unless these are the sources it derives.
fn forged(policy: &PostCommitChallengePolicyV1, sources: Vec<BeaconSourceV1>) -> BeaconProofV1 {
    let request = request(policy);
    let context = panel_beacon_context_v1(&request, policy, BTreeSet::from([PROFILE]));
    let mut accumulators = vec![initial_accumulator_v1(&context)];
    for (i, source) in sources.iter().enumerate() {
        accumulators.push(mix_v1(accumulators.last().unwrap(), (i + 1) as u32, source));
    }
    let lock_position = sources.iter().map(|s| s.settlement_position).max().unwrap() + policy.settlement_depth_d;
    let beacon = WorkBeaconV1 {
        output: *accumulators.last().unwrap(),
        challenge_anchor: challenge_anchor_v1(&context, &sources),
        accumulators,
        lock_position,
        sources,
    };
    BeaconProofV1 { epoch: request.epoch, output: Hash64::from_bytes(beacon.output), proof: borsh::to_vec(&beacon).unwrap() }
}

fn source_of(e: &WorkFinalEventV1) -> BeaconSourceV1 {
    BeaconSourceV1 {
        source_profile_id: e.source_profile_id,
        canonical_work_id: e.canonical_work_id,
        execution_commitment: e.execution_commitment,
        accepted_position: e.accepted_position,
        settlement_position: e.settlement_position,
        occurrence_index: e.occurrence_index,
    }
}

fn verify(policy: &PostCommitChallengePolicyV1, history: &History, proof: &BeaconProofV1) -> Result<(), PanelBeaconRefusalV1> {
    verify_panel_beacon_v1(std::slice::from_ref(policy), history, &request(policy), proof)
}

fn not_locked(why: Result<(), PanelBeaconRefusalV1>) {
    assert!(
        matches!(why, Err(PanelBeaconRefusalV1::Evidence(BeaconEvidenceRefusalV1::NotLocked(_)))),
        "the history derives no locked beacon, so the presented one is refused: {why:?}"
    );
}

#[test]
fn an_independent_source_inside_the_window_locks_and_verifies() {
    let p = policy(1);
    let history = History::new(vec![event(1, RELEASE + 1, RELEASE + 2)]);
    let proof = honest(&p, &history);
    verify(&p, &history, &proof).unwrap();
    assert_eq!(verify_panel_beacon_for_engine_v1(std::slice::from_ref(&p), &history, &request(&p), &proof), Ok(()));
    // The state machine an observer reads: collecting → candidate → locked, and unavailable once the window closed short.
    let state = |h: &History, tip: u64| panel_beacon_state_v1(std::slice::from_ref(&p), h, &request(&p), tip).unwrap();
    let empty = History::new(vec![]);
    assert!(matches!(state(&empty, RELEASE + 3), WorkBeaconStateV1::Collecting { have: 0, need: 1 }));
    assert!(matches!(state(&history, RELEASE + 2), WorkBeaconStateV1::Candidate { have: 1, .. }));
    assert!(matches!(state(&history, RELEASE + 3), WorkBeaconStateV1::Locked(_)));
    assert!(matches!(state(&empty, RELEASE + 1 + 10), WorkBeaconStateV1::Unavailable { have: 0, need: 1 }), "BEACON_UNAVAILABLE");
}

#[test]
fn the_shipped_registry_approves_no_scheme_so_every_proof_is_refused() {
    assert!(approved_panel_beacon_policies_v1().is_empty(), "EXTERNAL_GATE_PENDING: no Panel beacon scheme is approved");
    let p = policy(1);
    let history = History::new(vec![event(1, RELEASE + 1, RELEASE + 2)]);
    let proof = honest(&p, &history);
    let refused = verify_panel_beacon_v1(&approved_panel_beacon_policies_v1(), &history, &request(&p), &proof);
    assert_eq!(refused, Err(PanelBeaconRefusalV1::UnapprovedScheme));
    assert_eq!(
        verify_panel_beacon_for_engine_v1(&approved_panel_beacon_policies_v1(), &history, &request(&p), &proof),
        Err(PanelErrorV1::InvalidBeacon)
    );
    // …and a scheme id the request does not name is not the approved one.
    let mut other = request(&p);
    other.scheme = Hash64::from_u64_word(5);
    assert_eq!(verify_panel_beacon_v1(std::slice::from_ref(&p), &history, &other, &proof), Err(PanelBeaconRefusalV1::UnapprovedScheme));
}

/// The circularity and every forbidden kind: a perfectly eligible-looking contribution of the wrong KIND or PATH seeds nothing.
#[test]
fn a_panel_licensed_final_and_every_non_useful_work_kind_are_refused() {
    let p = policy(1);
    // The contribution is otherwise perfect: fresh, final, DA-satisfied, independent, from an eligible profile.
    let mut licensed = event(1, RELEASE + 1, RELEASE + 2);
    licensed.final_path = FinalPathV1::PanelLicensed { panel_seed_id: [9; 64], panel_epoch: 3 };
    let history = History::new(vec![licensed.clone()]);
    not_locked(verify(&p, &history, &forged(&p, vec![source_of(&licensed)])));
    for kind in [
        WorkSourceKindV1::Heartbeat,
        WorkSourceKindV1::Base0Fallback,
        WorkSourceKindV1::ExecTx,
        WorkSourceKindV1::ExecWorkSlice,
        WorkSourceKindV1::ReceiptOnly,
        WorkSourceKindV1::ProvisionalAttempt,
        WorkSourceKindV1::PanelReceipt,
    ] {
        let mut wrong = event(1, RELEASE + 1, RELEASE + 2);
        wrong.kind = kind;
        let history = History::new(vec![wrong.clone()]);
        not_locked(verify(&p, &history, &forged(&p, vec![source_of(&wrong)])));
    }
    // The contract states the same by its own state machine: with only such contributions the window ends unavailable.
    let state = panel_beacon_state_v1(std::slice::from_ref(&p), &History::new(vec![licensed]), &request(&p), RELEASE + 20).unwrap();
    assert!(matches!(state, WorkBeaconStateV1::Unavailable { have: 0, need: 1 }));
}

#[test]
fn a_self_candidate_a_stale_a_dependent_an_undelivered_and_an_ineligible_profile_are_refused() {
    let p = policy(1);
    // The claim being assigned cannot seed its own Panel.
    let mine = event(1, RELEASE + 1, RELEASE + 2);
    let mut history = History::new(vec![mine.clone()]);
    history.pending.insert(mine.canonical_work_id);
    not_locked(verify(&p, &history, &forged(&p, vec![source_of(&mine)])));
    // A work accepted before the epoch's start S = release + delay is not fresh, even if it settled inside the window.
    let stale = event(2, RELEASE, RELEASE + 2);
    not_locked(verify(&p, &History::new(vec![stale.clone()]), &forged(&p, vec![source_of(&stale)])));
    // Settled outside the window; not final; DA unsatisfied; not independent.
    let late = event(3, RELEASE + 1, RELEASE + 1 + 10);
    not_locked(verify(&p, &History::new(vec![late.clone()]), &forged(&p, vec![source_of(&late)])));
    for mutate in [0, 1, 2] {
        let mut bad = event(4, RELEASE + 1, RELEASE + 2);
        match mutate {
            0 => bad.claim_final = false,
            1 => bad.da_satisfied = false,
            _ => bad.validity_independent = false,
        }
        not_locked(verify(&p, &History::new(vec![bad.clone()]), &forged(&p, vec![source_of(&bad)])));
    }
    // A profile that is not Active and G14-complete in the commitment state (the chain's set is empty today).
    let mut history = History::new(vec![event(5, RELEASE + 1, RELEASE + 2)]);
    history.eligible.clear();
    not_locked(verify(&p, &history, &forged(&p, vec![source_of(&history.events[0])])));
}

#[test]
fn duplicate_reordered_substituted_and_forged_contributions_are_refused() {
    let p = policy(2);
    let (a, b) = (event(1, RELEASE + 1, RELEASE + 2), event(2, RELEASE + 1, RELEASE + 3));
    let history = History::new(vec![a.clone(), b.clone()]);
    let good = honest(&p, &history);
    verify(&p, &history, &good).unwrap();
    // Reordered: the same two sources in the other order, accumulators recomputed for that order.
    let reordered = forged(&p, vec![source_of(&b), source_of(&a)]);
    assert!(matches!(verify(&p, &history, &reordered), Err(PanelBeaconRefusalV1::Evidence(BeaconEvidenceRefusalV1::Mismatch { at: 0 }))));
    // Duplicate: one work listed twice cannot make `k`.
    let duplicate = forged(&p, vec![source_of(&a), source_of(&a)]);
    assert!(matches!(verify(&p, &history, &duplicate), Err(PanelBeaconRefusalV1::Evidence(BeaconEvidenceRefusalV1::Mismatch { .. }))));
    // Re-attachment never enlarges the set: the same work identity settled again later is not a second contribution.
    let mut again = a.clone();
    again.settlement_position += 1;
    let one_work = History::new(vec![a.clone(), again]);
    not_locked(verify(&p, &one_work, &forged(&p, vec![source_of(&a), source_of(&a)])));
    // Substituted: a source the branch has not settled.
    let alien = event(9, RELEASE + 1, RELEASE + 2);
    let substituted = forged(&p, vec![source_of(&a), source_of(&alien)]);
    assert!(matches!(verify(&p, &history, &substituted), Err(PanelBeaconRefusalV1::Evidence(BeaconEvidenceRefusalV1::Mismatch { at: 1 }))));
    // Forged output or anchor on the right sources.
    let mut tampered: WorkBeaconV1 = borsh::from_slice(&good.proof).unwrap();
    tampered.output[0] ^= 1;
    let output = BeaconProofV1 { output: Hash64::from_bytes(tampered.output), proof: borsh::to_vec(&tampered).unwrap(), ..good.clone() };
    assert!(matches!(verify(&p, &history, &output), Err(PanelBeaconRefusalV1::Evidence(BeaconEvidenceRefusalV1::Derivation))));
    let mut anchor: WorkBeaconV1 = borsh::from_slice(&good.proof).unwrap();
    anchor.challenge_anchor[0] ^= 1;
    let anchored = BeaconProofV1 { proof: borsh::to_vec(&anchor).unwrap(), ..good.clone() };
    assert!(matches!(verify(&p, &history, &anchored), Err(PanelBeaconRefusalV1::Evidence(BeaconEvidenceRefusalV1::Derivation))));
}

#[test]
fn the_envelope_binds_the_epoch_the_output_the_size_and_exact_decoding() {
    let p = policy(1);
    let history = History::new(vec![event(1, RELEASE + 1, RELEASE + 2)]);
    let good = honest(&p, &history);
    // Another epoch's proof.
    assert_eq!(verify(&p, &history, &BeaconProofV1 { epoch: 12, ..good.clone() }), Err(PanelBeaconRefusalV1::Envelope));
    // An output that is not the beacon's.
    assert_eq!(
        verify(&p, &history, &BeaconProofV1 { output: Hash64::from_u64_word(1), ..good.clone() }),
        Err(PanelBeaconRefusalV1::OutputMismatch)
    );
    // Bytes that are not exactly one beacon: truncated, with trailing bytes, empty, oversize.
    let mut truncated = good.clone();
    truncated.proof.truncate(truncated.proof.len() - 1);
    assert_eq!(verify(&p, &history, &truncated), Err(PanelBeaconRefusalV1::Malformed));
    let mut trailing = good.clone();
    trailing.proof.push(0);
    assert_eq!(verify(&p, &history, &trailing), Err(PanelBeaconRefusalV1::Malformed));
    assert_eq!(verify(&p, &history, &BeaconProofV1 { proof: Vec::new(), ..good.clone() }), Err(PanelBeaconRefusalV1::Malformed));
    assert_eq!(verify(&p, &history, &BeaconProofV1 { proof: vec![0; 65_537], ..good.clone() }), Err(PanelBeaconRefusalV1::Envelope));
}

#[test]
fn a_scheme_that_is_not_a_valid_non_interactive_policy_is_not_approved() {
    let mut p = policy(1);
    p.work_count_k = 0; // structurally invalid
    let history = History::new(vec![event(1, RELEASE + 1, RELEASE + 2)]);
    let proof = honest(&policy(1), &history);
    let mut req = request(&p);
    req.scheme = panel_beacon_scheme_of_v1(&p);
    assert!(matches!(verify_panel_beacon_v1(std::slice::from_ref(&p), &history, &req, &proof), Err(PanelBeaconRefusalV1::Policy(_))));
}
