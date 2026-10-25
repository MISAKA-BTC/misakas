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
    /// Required for v4/v5. Implementations must read an authenticated immutable
    /// epoch row, rather than reconstructing eligibility from the current tip.
    fn source_snapshot(
        &self,
        _request: &BeaconRequestV1,
    ) -> Result<crate::palw_panel_source_snapshot_v1::PalwPanelSourceSnapshotV1, String> {
        Err("the frozen Panel source subject is unavailable".into())
    }
    fn opt_in_material(&self, _context: &Digest) -> Result<(challenge::OptInContractV4, Vec<challenge::OptInSourceV4>), String> {
        Err("the authenticated Panel opt-in source contract is unavailable".into())
    }
    fn paid_material(&self, _context: &Digest) -> Result<crate::palw_beacon_source_fee_v1::PaidBeaconMaterialV5, String> {
        Err("the authenticated Panel prepaid source contract is unavailable".into())
    }
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
    #[error("the frozen source subject or complete monetary inventory is unavailable: {0}")]
    SourceContract(String),
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
    if policy.is_paid_opt_in_source() {
        Collector::Paid
    } else if policy.is_opt_in_source() {
        Collector::OptIn
    } else if policy.is_sealed_source() {
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
    OptIn,
    Paid,
}

/// These inventories are complete contracts. A candidate source is refused as
/// a whole contract: filtering it out would also erase a mixed or withheld
/// source and produce a different beacon.
fn opt_in_context_and_subjects(
    history: &impl PanelBeaconHistoryV1,
    request: &BeaconRequestV1,
    policy: &PostCommitChallengePolicyV1,
) -> Result<(BeaconContextV1, BTreeSet<Digest>), PanelBeaconRefusalV1> {
    let snapshot = history.source_snapshot(request).map_err(PanelBeaconRefusalV1::SourceContract)?;
    let ctx = snapshot.context(request, policy).map_err(PanelBeaconRefusalV1::SourceContract)?;
    Ok((ctx, snapshot.assigned_work_ids))
}

fn refuse_assigned_sources(sources: &[challenge::OptInSourceV4], assigned: &BTreeSet<Digest>) -> Result<(), PanelBeaconRefusalV1> {
    if sources.iter().any(|s| {
        matches!(s.source.reveal.as_ref().map(|r| &r.fate),
        Some(challenge::SourceFateV3::Final(event)) if assigned.contains(&event.canonical_work_id))
    }) {
        return Err(PanelBeaconRefusalV1::SourceContract("a sealed assignment is a source of its own Panel epoch".into()));
    }
    Ok(())
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
    let tip = history.tip_position();
    if matches!(collector_of(policy), Collector::OptIn | Collector::Paid) {
        let (ctx, assigned) = opt_in_context_and_subjects(history, request, policy)?;
        let verified = match collector_of(policy) {
            Collector::OptIn => {
                let (contract, sources) = history.opt_in_material(&ctx.id()).map_err(PanelBeaconRefusalV1::SourceContract)?;
                refuse_assigned_sources(&sources, &assigned)?;
                challenge::verify_opt_in_work_beacon_v4(&ctx, &contract, &sources, tip, &presented)
            }
            Collector::Paid => {
                let (contract, sources, promises) = history.paid_material(&ctx.id()).map_err(PanelBeaconRefusalV1::SourceContract)?;
                refuse_assigned_sources(&sources, &assigned)?;
                challenge::verify_paid_opt_in_work_beacon_v5(&ctx, &contract, &sources, &promises, tip, &presented)
            }
            _ => unreachable!(),
        };
        return verified.map(|_| ()).map_err(PanelBeaconRefusalV1::Policy);
    }
    let ctx = panel_beacon_context_v1(request, policy, history.eligible_profiles(request.release_daa));
    match collector_of(policy) {
        Collector::Plain => verify_work_beacon_v1(&ctx, &presented, &candidate_free_events(history, request.epoch), tip),
        Collector::Attributed => {
            verify_attributed_work_beacon_v1(&ctx, &presented, &candidate_free_works(history, request.epoch), tip)
        }
        Collector::Sealed => verify_sealed_work_beacon_v3(&ctx, &presented, &candidate_free_seals(history, request.epoch), tip),
        Collector::OptIn | Collector::Paid => unreachable!(),
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
    if matches!(collector_of(policy), Collector::OptIn | Collector::Paid) {
        let (ctx, assigned) = opt_in_context_and_subjects(history, request, policy)?;
        let state = match collector_of(policy) {
            Collector::OptIn => {
                let (contract, sources) = history.opt_in_material(&ctx.id()).map_err(PanelBeaconRefusalV1::SourceContract)?;
                refuse_assigned_sources(&sources, &assigned)?;
                challenge::collect_opt_in_work_beacon_v4(&ctx, &contract, &sources, tip_position)
            }
            Collector::Paid => {
                let (contract, sources, promises) = history.paid_material(&ctx.id()).map_err(PanelBeaconRefusalV1::SourceContract)?;
                refuse_assigned_sources(&sources, &assigned)?;
                challenge::collect_paid_opt_in_work_beacon_v5(&ctx, &contract, &sources, &promises, tip_position)
            }
            _ => unreachable!(),
        };
        return state.map(PanelBeaconStateV1::Sealed).map_err(PanelBeaconRefusalV1::Policy);
    }
    let ctx = panel_beacon_context_v1(request, policy, history.eligible_profiles(request.release_daa));
    let state = match collector_of(policy) {
        Collector::Plain => collect_work_beacon_v1(&ctx, &candidate_free_events(history, request.epoch), tip_position)
            .map(PanelBeaconStateV1::Accumulator),
        Collector::Attributed => collect_attributed_work_beacon_v1(&ctx, &candidate_free_works(history, request.epoch), tip_position)
            .map(PanelBeaconStateV1::Accumulator),
        Collector::Sealed => collect_sealed_work_beacon_v3(&ctx, &candidate_free_seals(history, request.epoch), tip_position)
            .map(PanelBeaconStateV1::Sealed),
        Collector::OptIn | Collector::Paid => unreachable!(),
    };
    state.map_err(PanelBeaconRefusalV1::Policy)
}

/// An epoch's beacon as [`panel_beacon_state_v1`] reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelBeaconStateV1 {
    Accumulator(WorkBeaconStateV1),
    Sealed(SealedBeaconStateV3),
}

#[cfg(test)]
mod paid_panel_tests {
    use super::*;
    use crate::palw_panel_source_snapshot_v1::PalwPanelSourceSnapshotV1;
    use challenge::*;
    #[derive(Clone)]
    struct History {
        snapshot: PalwPanelSourceSnapshotV1,
        material: crate::palw_beacon_source_fee_v1::PaidBeaconMaterialV5,
        current_profiles: BTreeSet<Digest>,
        current_pending: BTreeSet<Digest>,
    }
    impl PanelBeaconHistoryV1 for History {
        fn final_events(&self) -> Vec<WorkFinalEventV1> {
            Vec::new()
        }
        fn attributed_works(&self) -> Vec<AttributedWorkV1> {
            Vec::new()
        }
        fn sealed_sources(&self) -> Vec<SealedSourceV3> {
            Vec::new()
        }
        fn tip_position(&self) -> u64 {
            211
        }
        fn eligible_profiles(&self, _: u64) -> BTreeSet<Digest> {
            self.current_profiles.clone()
        }
        fn pending_work_of_epoch(&self, _: u64) -> BTreeSet<Digest> {
            self.current_pending.clone()
        }
        fn source_snapshot(&self, _: &BeaconRequestV1) -> Result<PalwPanelSourceSnapshotV1, String> {
            Ok(self.snapshot.clone())
        }
        fn paid_material(&self, context: &Digest) -> Result<crate::palw_beacon_source_fee_v1::PaidBeaconMaterialV5, String> {
            if *context != self.material.0.source.context_id {
                return Err("substituted context".into());
            }
            Ok(self.material.clone())
        }
    }
    fn fixture() -> (PostCommitChallengePolicyV1, BeaconRequestV1, History, BeaconProofV1) {
        // Synthetic mechanics only: both sources are genuine contract types,
        // not an economic/soundness approval or a real-node provenance claim.
        let p = paid_opt_in_source_policy_v5(2, 2, 20, 2, 1);
        let req = BeaconRequestV1 {
            network: Hash64::from_u64_word(1),
            ruleset: Hash64::from_u64_word(2),
            scheme: Hash64::from_bytes(p.id()),
            epoch: 11,
            release_daa: 100,
            deadline_daa: 300,
        };
        let snapshot = PalwPanelSourceSnapshotV1 {
            version: 1,
            network: req.network,
            ruleset: req.ruleset,
            scheme: req.scheme,
            epoch: 11,
            release_daa: 100,
            deadline_daa: 300,
            committed_daa: 110,
            committed_block: Hash64::from_u64_word(3),
            checkpoint: Hash64::from_u64_word(4),
            checkpoint_daa: 90,
            eligible_profiles: BTreeSet::from([[4; 64]]),
            excluded_profiles: BTreeSet::from([[5; 64]]),
            assigned_work_ids: BTreeSet::from([[91; 64]]),
        };
        let ctx = snapshot.context(&req, &p).unwrap();
        let contract = PaidOptInContractV5 {
            source: OptInContractV4 { context_id: ctx.id(), terms_id: [6; 64], source_final_by: 210 },
            fee_terms_id: [7; 64],
            minimum_fee: 50,
            fee_per_source: 60,
            max_sources: 2,
            funded_position: 113,
        };
        let sources: Vec<_> = [1u8, 2]
            .into_iter()
            .map(|who| OptInSourceV4 {
                context_id: ctx.id(),
                terms_id: [6; 64],
                opted_in_position: 115,
                forfeited_position: None,
                source: SealedSourceV3 {
                    source_profile_id: [4; 64],
                    attribution: SourceAttributionV1 { producer_id: [who; 64], consumer_id: RootV1::Present([who + 10; 64]) },
                    seal: [who + 20; 64],
                    seal_position: 114,
                    reveal: Some(SealRevealV3 {
                        reveal_position: 135,
                        salt: [who + 30; 64],
                        fate: SourceFateV3::Final(WorkFinalEventV1 {
                            kind: WorkSourceKindV1::RealUsefulWork,
                            source_profile_id: [4; 64],
                            canonical_work_id: [who + 40; 64],
                            execution_commitment: [who + 50; 64],
                            accepted_position: 135,
                            settlement_position: 185,
                            occurrence_index: 0,
                            claim_final: true,
                            da_satisfied: true,
                            validity_independent: true,
                            depends_on_profiles: Vec::new(),
                            final_path: FinalPathV1::PanelIndependent,
                        }),
                    }),
                },
            })
            .collect();
        let promises: Vec<_> = sources
            .iter()
            .map(|s| PaidSourcePromiseV5 {
                context_id: ctx.id(),
                producer: s.source.attribution.producer_id,
                fee_terms_id: contract.fee_terms_id,
                accepted_position: 115,
                fee: 60,
            })
            .collect();
        let SealedBeaconStateV3::Locked(beacon) =
            collect_paid_opt_in_work_beacon_v5(&ctx, &contract, &sources, &promises, 211).unwrap()
        else {
            panic!("complete synthetic paid Panel sources lock")
        };
        let proof =
            BeaconProofV1 { epoch: 11, output: Hash64::from_bytes(beacon.output), proof: borsh::to_vec(beacon.beacon()).unwrap() };
        (
            p,
            req,
            History {
                snapshot,
                material: (contract, sources, promises),
                current_profiles: BTreeSet::new(),
                current_pending: BTreeSet::new(),
            },
            proof,
        )
    }
    #[test]
    fn paid_panel_adapter_uses_frozen_facts_and_complete_fee_inventory_without_approving_a_policy() {
        let (p, req, mut h, proof) = fixture();
        assert!(approved_panel_beacon_policies_v1().is_empty());
        assert_eq!(verify_panel_beacon_v1(&[], &h, &req, &proof), Err(PanelBeaconRefusalV1::UnapprovedScheme));
        let approved = [p];
        verify_panel_beacon_v1(&approved, &h, &req, &proof).unwrap();
        let before = panel_beacon_state_v1(&approved, &h, &req, 211).unwrap();
        h.current_profiles.insert([99; 64]);
        h.current_pending.insert([41; 64]);
        assert_eq!(
            panel_beacon_state_v1(&approved, &h, &req, 211).unwrap(),
            before,
            "later eligibility and pending changes cannot rewrite the frozen epoch"
        );
        verify_panel_beacon_v1(&approved, &h, &req, &proof).unwrap();
        h.material.2.pop();
        assert!(
            verify_panel_beacon_v1(&approved, &h, &req, &proof).is_err(),
            "an unpaid mixed source cannot use the legacy collector"
        );
        assert!(panel_beacon_state_v1(&approved, &h, &req, 211).is_err());
    }
    #[test]
    fn paid_panel_adapter_refuses_candidate_reuse_and_substituted_frozen_subject() {
        let (p, req, h, proof) = fixture();
        let approved = [p];
        let mut circular = h.clone();
        // Rewrite the final identity in the complete inventory, while its
        // current pending set stays empty; the immutable assignment still bars it.
        if let SourceFateV3::Final(event) = &mut circular.material.1[0].source.reveal.as_mut().unwrap().fate {
            event.canonical_work_id = [91; 64];
        }
        assert!(matches!(panel_beacon_state_v1(&approved, &circular, &req, 211), Err(PanelBeaconRefusalV1::SourceContract(_))));
        assert!(matches!(verify_panel_beacon_v1(&approved, &circular, &req, &proof), Err(PanelBeaconRefusalV1::SourceContract(_))));
        let mut other = h;
        other.snapshot.committed_daa += 1;
        assert!(verify_panel_beacon_v1(&approved, &other, &req, &proof).is_err());
    }
}
