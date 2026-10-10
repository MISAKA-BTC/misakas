//! **RFC-0010 production beacon adapter** — the one place the Panel engine's `ConsensusViewV1::verify_beacon` meets the shared
//! challenge contract (`misaka-palw-challenge`). Dormant with the fence: nothing here is reachable below an armed
//! `palw_permissionless_panel_v1`, which `validate_palw_permissionless_panel_v1` refuses at every real height.
//!
//! ```text
//! BeaconProofV1.proof  =  borsh(WorkBeaconV1)            (the PALW Work Beacon of subject kind PANEL_ASSIGNMENT)
//! verify              =  verify_work_beacon_v1(ctx(epoch), presented, chain-derived WorkFinalEventV1s, tip)
//! ```
//!
//! **What the adapter refuses by construction** (each is a test):
//!
//! * a scheme this release does not approve — [`approved_panel_beacon_policies_v1`] is EMPTY, so every proof is refused and
//!   every claim ends `BeaconUnavailable` (non-fraud: no slash, no strike, the reservations released);
//! * a source that is not REAL useful work (heartbeat, BASE-0 fallback, EXEC, receipt-only, provisional, Panel receipt) — by
//!   kind, in the challenge contract's eligibility;
//! * a source whose Final passed through a Panel licence — `FinalPathV1::PanelLicensed` is refused for
//!   `SubjectKindV1::PanelAssignment` (the circularity work → Panel → Final → beacon → Panel). Every Final the V2 lattice writes
//!   is Panel-licensed; the separate kernel route can supply Panel-independent OPV Finals;
//! * a source outside the epoch's FROZEN source set: the derived OPV-eligible profiles at the epoch's commitment position, frozen
//!   once by the engine in the first block whose DAA reaches `release_daa` (from that block's parent) and read back by
//!   [`ChainPanelBeaconHistoryV1`] — never re-derived from the ledger the verifying block holds;
//! * the claims being assigned, a work accepted before the epoch's start `S`, a duplicate, reordered or substituted contribution,
//!   a forged accumulator/anchor/output (the contract's recomputation).
//!
//! Forbidden as sources and absent here: a raw future block hash, a heartbeat or BASE-0 hash, a cheap unverified attempt, a
//! signature, a candidate-selected nonce, a DNS/BFT committee or validator, operator randomness, a test certificate.
//!
//! **Units.** Every position is a DAA score: `accepted_position = claim.accepted_daa`, `settlement_position = final_daa`.
//! The epoch's subject is committed at `release_daa` (every claim sealed for the epoch was sealed strictly before it), so the
//! contract's start is `S = release_daa + anchor_delay_slots`, and a freshly accepted source cannot be a claim already sealed
//! for the epoch.

use std::collections::BTreeSet;

use kaspa_hashes::Hash64;
use misaka_palw_challenge::beacon::BeaconEvidenceRefusalV1;
use misaka_palw_challenge::hash::{Digest, object_id};
use misaka_palw_challenge::policy::PolicyRefusalV1;
use misaka_palw_challenge::{
    AttributedWorkV1, BeaconContextV1, InteractiveModeV1, PostCommitChallengePolicyV1, SealedBeaconStateV3, SealedSourceV3,
    SourceRuleV1, SubjectKindV1, WorkBeaconStateV1, WorkBeaconV1, WorkFinalEventV1, collect_attributed_work_beacon_v1,
    collect_sealed_work_beacon_v3, collect_work_beacon_v1, verify_attributed_work_beacon_v1, verify_sealed_work_beacon_v3,
    verify_work_beacon_v1,
};
use misaka_palw_panel::{BeaconProofV1, BeaconRequestV1, MAX_BEACON_PROOF_BYTES_V1, PanelErrorV1};

/// The shared challenge contract, re-exported so a downstream test (the processor's) names its types without a new dependency edge.
pub use misaka_palw_challenge as challenge;

/// The domain of an epoch's committed subject. Not shared with any other subject kind's commitment.
pub const PANEL_ASSIGNMENT_COMMITMENT_DOMAIN_V1: &[u8] = b"MISAKA/PALW/PANEL-ASSIGNMENT/COMMITMENT/V1";

/// **The schemes this release approves for Panel binding entropy: none.** A scheme is approved only after the external review of
/// RFC-0007 §VI.8 item 2 (reorg / withholding / last-mover / candidate-grinding bias, and RFC-0010's P0-10 analysis); the shipped
/// registry is empty, so [`verify_panel_beacon_v1`] refuses every proof. EXTERNAL_GATE_PENDING.
pub fn approved_panel_beacon_policies_v1() -> Vec<PostCommitChallengePolicyV1> {
    Vec::new()
}

/// **The scheme id a policy is named by in `PanelPolicyV1::beacon_scheme`.**
pub fn panel_beacon_scheme_of_v1(policy: &PostCommitChallengePolicyV1) -> Hash64 {
    Hash64::from_bytes(policy.id())
}

/// What the verifier reads of the branch the proof is judged against.
pub trait PanelBeaconHistoryV1 {
    /// Every settlement event of the branch (any order; the contract sorts).
    fn final_events(&self) -> Vec<WorkFinalEventV1>;
    /// The same settlements with who stands behind each (the producer bond, and the consumer where the route records one), for a
    /// policy whose source rule is distinct (RFC-0007 §VI). A source without an attribution is attributed to nobody (`[0; 64]`).
    fn attributed_works(&self) -> Vec<AttributedWorkV1>;
    /// The sealed-source beacon's (v3) facts: every claim seal of the branch with its reveal and its claim's fate.
    fn sealed_sources(&self) -> Vec<SealedSourceV3>;
    /// The last position the branch has settled (the carrying block's selected parent's DAA).
    fn tip_position(&self) -> u64;
    /// Source profiles that were Active AND G14-complete (derived OPV-eligible) at the epoch's commitment position — for the chain's
    /// history, the set the engine FROZE at that position (`PermissionlessPanelStateV1::frozen_sources_at`), never a re-derivation.
    fn eligible_profiles(&self, commitment_position: u64) -> BTreeSet<Digest>;
    /// The canonical work identities of the non-terminal claims sealed for `epoch`: the candidates under test, never sources.
    fn pending_work_of_epoch(&self, epoch: u64) -> BTreeSet<Digest>;
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PanelBeaconRefusalV1 {
    #[error("the proof is for another epoch than the request, or exceeds the proof bound")]
    Envelope,
    #[error("the request names a scheme this release does not approve")]
    UnapprovedScheme,
    #[error("the approved scheme is not a valid non-interactive challenge policy: {0}")]
    Policy(PolicyRefusalV1),
    #[error("the proof bytes are not exactly one WorkBeaconV1")]
    Malformed,
    #[error("the presented output is not the beacon's output")]
    OutputMismatch,
    #[error("the beacon is not the one this branch derives: {0}")]
    Evidence(BeaconEvidenceRefusalV1),
}

impl From<PanelBeaconRefusalV1> for PanelErrorV1 {
    fn from(_: PanelBeaconRefusalV1) -> Self {
        PanelErrorV1::InvalidBeacon
    }
}

fn digest(h: Hash64) -> Digest {
    h.as_bytes()
}

/// **An epoch's committed subject**: the chain, ruleset, scheme, epoch and the window that bounds when the proof may be carried.
pub fn panel_epoch_commitment_root_v1(request: &BeaconRequestV1) -> Digest {
    object_id(
        PANEL_ASSIGNMENT_COMMITMENT_DOMAIN_V1,
        &(
            digest(request.network),
            digest(request.ruleset),
            digest(request.scheme),
            request.epoch,
            request.release_daa,
            request.deadline_daa,
        ),
    )
}

/// The contract's context for an epoch (subject kind `PANEL_ASSIGNMENT`).
pub fn panel_beacon_context_v1(
    request: &BeaconRequestV1,
    policy: &PostCommitChallengePolicyV1,
    eligible_profiles: BTreeSet<Digest>,
) -> BeaconContextV1 {
    BeaconContextV1 {
        chain_genesis: digest(request.network),
        ruleset_id: digest(request.ruleset),
        policy: policy.clone(),
        subject_kind: SubjectKindV1::PanelAssignment,
        commitment_root: panel_epoch_commitment_root_v1(request),
        commitment_position: request.release_daa,
        challenge_epoch: request.epoch,
        eligible_profiles,
        excluded_profiles: BTreeSet::new(),
        // A Panel draw has no candidate profile: its circularity guard is the source rule (Panel-independent Finals only).
        candidate_profile_id: misaka_palw_challenge::RootV1::Absent,
    }
}

fn approved_policy<'a>(
    approved: &'a [PostCommitChallengePolicyV1],
    request: &BeaconRequestV1,
) -> Result<&'a PostCommitChallengePolicyV1, PanelBeaconRefusalV1> {
    let policy = approved.iter().find(|p| p.id() == digest(request.scheme)).ok_or(PanelBeaconRefusalV1::UnapprovedScheme)?;
    policy.validate().map_err(PanelBeaconRefusalV1::Policy)?;
    if policy.interactive_mode != InteractiveModeV1::NonInteractive {
        return Err(PanelBeaconRefusalV1::Policy(PolicyRefusalV1::TranscriptMode));
    }
    Ok(policy)
}

/// The branch's events with the claims under test removed: a work that is itself being assigned in this epoch cannot seed
/// its own Panel.
fn candidate_free_events(history: &impl PanelBeaconHistoryV1, epoch: u64) -> Vec<WorkFinalEventV1> {
    let under_test = history.pending_work_of_epoch(epoch);
    history.final_events().into_iter().filter(|event| !under_test.contains(&event.canonical_work_id)).collect()
}

/// [`candidate_free_events`] over attributed works.
fn candidate_free_works(history: &impl PanelBeaconHistoryV1, epoch: u64) -> Vec<AttributedWorkV1> {
    let under_test = history.pending_work_of_epoch(epoch);
    history.attributed_works().into_iter().filter(|work| !under_test.contains(&work.event.canonical_work_id)).collect()
}

/// The v3 seals with the claims under test removed: a revealed seal whose claim is itself being assigned in this epoch is not a
/// source of its own Panel (an unrevealed one is unknown work and stays a seal, as the contract treats it).
fn candidate_free_seals(history: &impl PanelBeaconHistoryV1, epoch: u64) -> Vec<SealedSourceV3> {
    let under_test = history.pending_work_of_epoch(epoch);
    history
        .sealed_sources()
        .into_iter()
        .filter(|seal| match seal.reveal.as_ref().map(|r| &r.fate) {
            Some(misaka_palw_challenge::SourceFateV3::Final(event)) => !under_test.contains(&event.canonical_work_id),
            _ => true,
        })
        .collect()
}

/// Which collector a policy names: v3's sealed sources, the distinct rules' attributed works, or v1's plain events.
fn collector_of(policy: &PostCommitChallengePolicyV1) -> Collector {
    if policy.is_sealed_source() {
        Collector::Sealed
    } else if matches!(policy.source_rule(), Some(rule) if rule != SourceRuleV1::Plain) {
        Collector::Attributed
    } else {
        Collector::Plain
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Collector {
    Plain,
    Attributed,
    Sealed,
}

/// **A fresh node checks a presented Panel beacon against its own branch.** Refusals are typed; the engine adapter
/// ([`verify_panel_beacon_for_engine_v1`]) collapses them to `InvalidBeacon`.
pub fn verify_panel_beacon_v1(
    approved: &[PostCommitChallengePolicyV1],
    history: &impl PanelBeaconHistoryV1,
    request: &BeaconRequestV1,
    proof: &BeaconProofV1,
) -> Result<(), PanelBeaconRefusalV1> {
    if proof.epoch != request.epoch || proof.proof.len() > MAX_BEACON_PROOF_BYTES_V1 as usize {
        return Err(PanelBeaconRefusalV1::Envelope);
    }
    let policy = approved_policy(approved, request)?;
    let presented: WorkBeaconV1 = borsh::from_slice(&proof.proof).map_err(|_| PanelBeaconRefusalV1::Malformed)?;
    if presented.output != digest(proof.output) {
        return Err(PanelBeaconRefusalV1::OutputMismatch);
    }
    let ctx = panel_beacon_context_v1(request, policy, history.eligible_profiles(request.release_daa));
    let tip = history.tip_position();
    match collector_of(policy) {
        Collector::Plain => verify_work_beacon_v1(&ctx, &presented, &candidate_free_events(history, request.epoch), tip),
        Collector::Attributed => {
            verify_attributed_work_beacon_v1(&ctx, &presented, &candidate_free_works(history, request.epoch), tip)
        }
        Collector::Sealed => verify_sealed_work_beacon_v3(&ctx, &presented, &candidate_free_seals(history, request.epoch), tip),
    }
    .map(|_| ())
    .map_err(PanelBeaconRefusalV1::Evidence)
}

/// [`verify_panel_beacon_v1`] under the engine's `ConsensusViewV1::verify_beacon` signature.
pub fn verify_panel_beacon_for_engine_v1(
    approved: &[PostCommitChallengePolicyV1],
    history: &impl PanelBeaconHistoryV1,
    request: &BeaconRequestV1,
    proof: &BeaconProofV1,
) -> Result<(), PanelErrorV1> {
    verify_panel_beacon_v1(approved, history, request, proof).map_err(PanelErrorV1::from)
}

/// **The beacon an epoch has at `tip_position`**, for observation and for a producer that builds the proof it carries: v1/v2's
/// accumulator state (Collecting / Candidate / Locked / Unavailable) or v3's sealed state (Sealing / Revealing / Settling / Locked /
/// Unavailable / Vetoed). `Err` when no approved scheme names the request.
pub fn panel_beacon_state_v1(
    approved: &[PostCommitChallengePolicyV1],
    history: &impl PanelBeaconHistoryV1,
    request: &BeaconRequestV1,
    tip_position: u64,
) -> Result<PanelBeaconStateV1, PanelBeaconRefusalV1> {
    let policy = approved_policy(approved, request)?;
    let ctx = panel_beacon_context_v1(request, policy, history.eligible_profiles(request.release_daa));
    let state = match collector_of(policy) {
        Collector::Plain => collect_work_beacon_v1(&ctx, &candidate_free_events(history, request.epoch), tip_position)
            .map(PanelBeaconStateV1::Accumulator),
        Collector::Attributed => collect_attributed_work_beacon_v1(&ctx, &candidate_free_works(history, request.epoch), tip_position)
            .map(PanelBeaconStateV1::Accumulator),
        Collector::Sealed => collect_sealed_work_beacon_v3(&ctx, &candidate_free_seals(history, request.epoch), tip_position)
            .map(PanelBeaconStateV1::Sealed),
    };
    state.map_err(PanelBeaconRefusalV1::Policy)
}

/// An epoch's beacon as [`panel_beacon_state_v1`] reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelBeaconStateV1 {
    Accumulator(WorkBeaconStateV1),
    Sealed(SealedBeaconStateV3),
}
