//! **G14 ③ / ⑦ (RFC-0006 layered Panel): the automatic GDN STEP filer** (2026-10-22; node policy, off by default behind
//! `--palw-gdn-step-filer`, and dormant below `palw_memory_routing_court_v2` — the fence that already owns tag 243
//! `GdnStepAccusedV2`; every preset leaves it off). Its sibling [`crate::palw_gdn_held_filer`] pursues a FORGED CHECKPOINT
//! (tag 244, through the held DA court); this module pursues a forged recurrence STEP standing on an honest checkpoint:
//!
//! ```text
//! detect    for each served step tile of the claim (a rotating cursor, PALW_GDN_STEP_PROBES_PER_TICK_V1 a tick), this node's OWN
//!           recompute builds the anchored step evidence (`base0_gdn_step_evidence_v2`: the predecessor checkpoint recomputed and
//!           authenticated against the producer's committed leaf) and the court's own verdict function
//!           (`palw_gdn_court_verdict_v2`) is run on it locally:
//!             ExecutorGuilty     → a mismatch: the step does not recompute
//!             FalseAccusation    → this tile is honest; next tile
//!             ForgedCheckpoint   → the predecessor checkpoint is itself forged: the held filer's case (tag 244), not this one
//! material  the DA/material session the held filer already runs (`gdn_accuser_view_v2` over the pool's served capture; the class
//!           inventory's openings) — public, authenticated material only
//! build     `GdnStepAccusedV2` signed over `palw_gdn_court_session_id_v2` under `PALW_GDN_COURT_MLDSA87_CONTEXT_V2`, ONLY after the
//!           fold's own verdict function convicted it locally (an honest producer is never accused; the accuser is never slashed
//!           for a false accusation by its own filer)
//! window    the fold refuses tag 243 on a terminal claim unless the claim is `Final` and the accuser holds a live dispute
//!           reservation (`legacy_dispute_reservation_v1`). So a filing is checked against the claim's Final (unpaid-reward)
//!           deadline: inside it → file; close to it and the reservation fence (`palw_legacy_public_filer_v1`) is in force →
//!           reserve first (tag 154) and file; past it with no reservation → refused here, logged with the exact DAAs
//! track     each tick the claim's dispute view is read: `Voided{CourtFraud}` → convicted, and the conviction is handed to the
//!           false-Valid filer as the chain's rooted `CourtFraud` contradiction (which files against EVERY liable `Valid` signer
//!           of the claim — `palw_false_valid_filings_v1` enumerates every seat); a reservation or accusation that was carried
//!           and is no longer on the selected chain (a reorg) is re-submitted (`palw_gdn_step_resend_v1`), a bounded number of
//!           times
//! ```
//!
//! Nothing here is a proof of computation (PRINCIPLES §8): Merkle openings authenticate data, the verdict is the court's
//! deterministic recomputation. The decision functions are pure over their inputs, so the node's tick and the tests drive the
//! same state machine. **What a restart loses**: the book is in memory, like the false-Valid filer's; a restarted node re-detects
//! from public material and reads every filing's fate back from the chain.
use kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1;
use kaspa_consensus_core::palw_gdn_court_v2::{
    PALW_GDN_COURT_MLDSA87_CONTEXT_V2, PALW_GDN_COURT_VERSION_V2, PalwGdnCourtAccusationV2, palw_gdn_court_session_id_v2,
    palw_gdn_court_verdict_v2,
};
use kaspa_consensus_core::palw_shard_court_v1::{PalwOneMoveClaimV2, PalwShardCourtVerdictV1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwVoidReasonV2};
use kaspa_consensus_core::palw_step_refute::memory_v2::PalwGdnStepRefutationV2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::fp_recompute::Base0FpRecomputeKernelsV1;
use misaka_palw_base0::gdn_evidence_v2::{Base0GdnEvidenceV2, Base0GdnMaterialV2, Base0GdnOwnedMaterialV2, base0_gdn_step_evidence_v2};
use std::collections::BTreeMap;

/// Step tiles re-executed per claim per tick (each is one prefix walk of the accuser's own kernels).
pub const PALW_GDN_STEP_PROBES_PER_TICK_V1: usize = 2;
/// Claims probed per tick.
pub const PALW_GDN_STEP_CLAIMS_PER_TICK_V1: usize = 1;
/// The most cases held at once (oldest dropped first).
pub const PALW_GDN_STEP_MAX_CASES_V1: usize = 64;
/// DAA after a carrier was handed before its absence from the selected chain reads as lost (a reorg or a dropped carrier) — the
/// false-Valid filer's re-file interval (six court re-plans).
pub const PALW_GDN_STEP_RESEND_DAA_V1: u64 = 60;
/// Times one object is handed (the first send plus re-sends) — `PALW_FILER_HAND_OFFS_PER_OFFENCE_V1`'s bound, plus one re-send for
/// a reorg.
pub const PALW_GDN_STEP_MAX_SENDS_V1: u8 = 3;
/// DAA before the claim's Final deadline at which a reservation is taken first (a carrier must land before the reward is paid).
pub const PALW_GDN_STEP_FINAL_MARGIN_DAA_V1: u64 = 60;

/// The claim the case pursues, as the chain records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwGdnStepClaimV1 {
    pub claim: Hash64,
    pub executor: PalwBondKeyV2,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
}

// ---- 1. detection --------------------------------------------------------------------------------------------------------------

/// What the evidence builder returned, mapped so the decision is generic over the refutation (tests use a stand-in).
#[derive(Clone, Debug)]
pub enum PalwGdnStepBuiltV1<R> {
    Step(R),
    ForgedCheckpoint { index: u32 },
}

impl From<Base0GdnEvidenceV2> for PalwGdnStepBuiltV1<Box<PalwGdnStepRefutationV2>> {
    fn from(e: Base0GdnEvidenceV2) -> Self {
        match e {
            Base0GdnEvidenceV2::Step(r) => Self::Step(r),
            Base0GdnEvidenceV2::ForgedCheckpoint { index } => Self::ForgedCheckpoint { index },
        }
    }
}

/// **What one probe of one served step tile found.**
#[derive(Clone, Debug)]
pub enum PalwGdnStepProbeV1<R> {
    /// The court's verdict on this node's own evidence is `FalseAccusation`: the tile recomputes.
    Honest,
    /// The predecessor checkpoint is not the job's state: the held filer's (tag 244) case.
    ForgedCheckpoint { index: u32 },
    /// The court's verdict on this node's own evidence is `ExecutorGuilty`: a mismatch at `leaf`.
    Mismatch { leaf: u64, refutation: R },
    /// Not a recurrence leaf, or the evidence does not build (logged; never a reason to accuse).
    Unbuildable(String),
}

/// **Classify one probe** — pure over the builder's result and a local verdict (`verdict(refutation)`: `Ok(true)` guilty,
/// `Ok(false)` honest, `Err` a refusal of shape/resources). A refused verdict is never a mismatch.
pub fn palw_gdn_step_classify_v1<R>(
    leaf: u64,
    built: Result<PalwGdnStepBuiltV1<R>, String>,
    verdict: impl FnOnce(&R) -> Result<bool, String>,
) -> PalwGdnStepProbeV1<R> {
    match built {
        Err(why) => PalwGdnStepProbeV1::Unbuildable(why),
        Ok(PalwGdnStepBuiltV1::ForgedCheckpoint { index }) => PalwGdnStepProbeV1::ForgedCheckpoint { index },
        Ok(PalwGdnStepBuiltV1::Step(refutation)) => match verdict(&refutation) {
            Ok(true) => PalwGdnStepProbeV1::Mismatch { leaf, refutation },
            Ok(false) => PalwGdnStepProbeV1::Honest,
            Err(why) => PalwGdnStepProbeV1::Unbuildable(format!("the local court refused the evidence: {why}")),
        },
    }
}

/// The served step-tile indices still to probe, from `cursor`, at most `budget` (the tiles in canonical index order).
pub fn palw_gdn_step_probe_window_v1(served: &[u64], cursor: usize, budget: usize) -> &[u64] {
    let start = cursor.min(served.len());
    &served[start..(start + budget).min(served.len())]
}

/// **The accusation, built** — unsigned fields from the claim and the evidence; the caller signs it.
pub fn palw_gdn_step_accusation_v1(
    claim: &PalwGdnStepClaimV1,
    accuser: PalwBondKeyV2,
    leaf: u64,
    refutation: PalwGdnStepRefutationV2,
    openings: &[PalwArtifactOpeningV1],
) -> PalwGdnCourtAccusationV2 {
    PalwGdnCourtAccusationV2 {
        version: PALW_GDN_COURT_VERSION_V2,
        claim: claim.claim,
        execution_root: claim.execution_root,
        trace_root: claim.trace_root,
        executor_bond: claim.executor,
        accuser_bond: accuser,
        leaf_index: leaf,
        refutation,
        artifact_openings: openings.to_vec(),
        signature: Vec::new(),
    }
}

/// **Mismatch → plan → object**: the court's own verdict function on the complete accusation (exactly what the fold runs), and
/// only on `ExecutorGuilty` the signature over the session id. Every other answer is a refusal the caller logs.
pub fn palw_gdn_step_object_v1(
    claim: &PalwGdnStepClaimV1,
    accuser: PalwBondKeyV2,
    network_domain: Hash64,
    leaf: u64,
    refutation: PalwGdnStepRefutationV2,
    openings: &[PalwArtifactOpeningV1],
    ladder: u64,
    sign: &dyn Fn(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    if accuser == claim.executor {
        return Err("the producer never accuses its own claim".into());
    }
    let mut accusation = palw_gdn_step_accusation_v1(claim, accuser, leaf, refutation, openings);
    let actual = PalwOneMoveClaimV2 { execution_root: claim.execution_root, class_id: claim.class_id, artifact_root: claim.artifact_root };
    match palw_gdn_court_verdict_v2(&accusation, &actual, ladder).map_err(|e| format!("the court refuses the accusation: {e}"))? {
        PalwShardCourtVerdictV1::ExecutorGuilty => {}
        other => return Err(format!("the court's own verdict on this accusation is {other:?}: nothing is filed")),
    }
    let session = palw_gdn_court_session_id_v2(network_domain, &accusation);
    accusation.signature =
        sign(session.as_byte_slice(), PALW_GDN_COURT_MLDSA87_CONTEXT_V2).ok_or("no signing key for a GDN step accusation")?;
    Ok(PalwConsensusObjectV2::GdnStepAccusedV2 { accusation: Box::new(accusation) })
}

// ---- 2. the accusation window vs. the unpaid reward ----------------------------------------------------------------------------

/// The chain's facts the window reads (the claim's dispute view at the tip).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwGdnStepWindowFactsV1 {
    pub phase: PalwClaimPhaseV2,
    /// DL-1's deadline: the DAA at which the claim goes `Final` and its reward is paid (`None` while something holds it).
    pub final_deadline_daa: Option<u64>,
    /// `trace_retention_daa − W_disclose`: the last DAA a reservation may open.
    pub hard_deadline_daa: u64,
    /// This bond holds a live dispute reservation on the claim.
    pub own_reservation_live: bool,
    /// This bond's reservation on the claim ended earlier (once per life: it cannot reserve again).
    pub own_reservation_closed: bool,
    /// `palw_legacy_public_filer_v1` is in force (tag 154 is admitted).
    pub reservation_fence: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwGdnStepWindowV1 {
    /// File now; `reserve`: carry a reservation first (the Final deadline is within the margin).
    Open { reserve: bool },
    /// Do not file: the reason, with the exact DAAs.
    Closed(String),
}

/// **May the accusation still be filed at `now`?** The fold's rule (tag 243 on a terminal claim only `Final` + a live reservation of
/// the accuser) read ahead by the carrier's landing margin, so nothing is filed that would land after the reward is paid.
pub fn palw_gdn_step_window_v1(f: &PalwGdnStepWindowFactsV1, now: u64, margin: u64) -> PalwGdnStepWindowV1 {
    let can_reserve = f.reservation_fence && !f.own_reservation_closed && now <= f.hard_deadline_daa;
    match &f.phase {
        PalwClaimPhaseV2::Voided { voided_daa, reason } => {
            PalwGdnStepWindowV1::Closed(format!("the claim was voided at DAA {voided_daa} ({reason:?}): nothing left to accuse"))
        }
        PalwClaimPhaseV2::Final { .. } if f.own_reservation_live => PalwGdnStepWindowV1::Open { reserve: false },
        PalwClaimPhaseV2::Final { final_daa } => PalwGdnStepWindowV1::Closed(format!(
            "the claim went Final at DAA {final_daa} (its reward is paid) and this bond holds no live reservation on it: the fold \
             refuses GdnStepAccusedV2 past that point (now {now})"
        )),
        _ if f.own_reservation_live => PalwGdnStepWindowV1::Open { reserve: false },
        _ => match f.final_deadline_daa {
            Some(deadline) if now.saturating_add(margin) >= deadline => {
                if can_reserve {
                    PalwGdnStepWindowV1::Open { reserve: true }
                } else if now < deadline {
                    // Inside the window but too close to rely on: file without a reservation and let the fold decide.
                    PalwGdnStepWindowV1::Open { reserve: false }
                } else {
                    PalwGdnStepWindowV1::Closed(format!(
                        "the claim's Final deadline is DAA {deadline} and it is DAA {now}: a carrier would land after the reward is \
                         paid, and no reservation can hold it (fence {}, closed before {}, hard deadline {})",
                        f.reservation_fence, f.own_reservation_closed, f.hard_deadline_daa
                    ))
                }
            }
            _ => PalwGdnStepWindowV1::Open { reserve: false },
        },
    }
}

// ---- 3. tracking: reorg re-submission and the verdict ---------------------------------------------------------------------------

/// What this node handed for a case: the DAA last handed and how many times.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwGdnStepSentV1 {
    pub reservation: Option<(u64, u8)>,
    pub accusation: Option<(u64, u8)>,
}

/// The reservation's presence at the tip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGdnStepReservationAtTipV1 {
    Live,
    /// Ended (released, lapsed or settled): once per life, never re-sent.
    Closed,
    Absent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwGdnStepResendV1 {
    Reservation,
    Accusation,
    /// The send budget is spent and the object is still not on the selected chain.
    GiveUp(&'static str),
}

/// **What to re-submit at `now`** — a reservation handed past `PALW_GDN_STEP_RESEND_DAA_V1` and absent at the tip (a reorg dropped
/// it, or it never landed); an accusation handed past the interval whose verdict is not on the chain (`verdict_seen`: the claim is
/// voided `CourtFraud`). A reservation is re-sent before the accusation that leans on it.
pub fn palw_gdn_step_resend_v1(
    sent: &PalwGdnStepSentV1,
    reservation: PalwGdnStepReservationAtTipV1,
    verdict_seen: bool,
    now: u64,
) -> Vec<PalwGdnStepResendV1> {
    let due = |at: u64| now >= at.saturating_add(PALW_GDN_STEP_RESEND_DAA_V1);
    let mut out = Vec::new();
    if let Some((at, sends)) = sent.reservation
        && reservation == PalwGdnStepReservationAtTipV1::Absent
        && due(at)
    {
        out.push(if sends < PALW_GDN_STEP_MAX_SENDS_V1 {
            PalwGdnStepResendV1::Reservation
        } else {
            PalwGdnStepResendV1::GiveUp("the reservation never stayed on the selected chain")
        });
    }
    if let Some((at, sends)) = sent.accusation
        && !verdict_seen
        && due(at)
    {
        out.push(if sends < PALW_GDN_STEP_MAX_SENDS_V1 {
            PalwGdnStepResendV1::Accusation
        } else {
            PalwGdnStepResendV1::GiveUp("the accusation's verdict never reached the selected chain")
        });
    }
    out
}

/// The conviction this node's accusation is waiting for: the claim voided `CourtFraud` at `voided_daa`.
pub fn palw_gdn_step_convicted_v1(phase: &PalwClaimPhaseV2) -> Option<u64> {
    match phase {
        PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } => Some(*voided_daa),
        _ => None,
    }
}

/// **The false-Valid proof a conviction leaves** — the chain's rooted `CourtFraud` contradiction, the small continuation the
/// false-Valid scan already accepts (`palw_false_valid_scan_v1`). Handed once per claim; the scan then files against every liable
/// `Valid` signer (`palw_false_valid_filings_v1` enumerates every seat, never only the first).
pub fn palw_gdn_step_false_valid_proof_v1(
    claim: Hash64,
    voided_daa: u64,
) -> kaspa_consensus_core::palw_false_valid_filing_v1::PalwFalseValidProofV1 {
    kaspa_consensus_core::palw_false_valid_filing_v1::PalwFalseValidProofV1 {
        claim_id: claim,
        contradiction: kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::CourtFraud { voided_daa },
        prompt_ids_opening: None,
    }
}

// ---- 4. the book and the tick's pure half --------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum PalwGdnStepStageV1 {
    /// Probing served tiles from `cursor`.
    Probing { cursor: usize },
    /// A signed accusation, waiting for (or riding) the carrier.
    Filed { object: Box<PalwConsensusObjectV2>, leaf: u64, sent: PalwGdnStepSentV1 },
}

#[derive(Clone, Debug)]
pub struct PalwGdnStepCaseV1 {
    pub claim: PalwGdnStepClaimV1,
    pub stage: PalwGdnStepStageV1,
    pub opened_daa: u64,
}

/// The filer's memory across ticks.
#[derive(Default, Debug)]
pub struct PalwGdnStepBookV1 {
    pub cases: BTreeMap<Hash64, PalwGdnStepCaseV1>,
    /// Claims done with, and why (honest, forged-checkpoint, convicted, window closed, given up).
    pub settled: BTreeMap<Hash64, String>,
}

impl PalwGdnStepBookV1 {
    pub fn open(&mut self, claim: PalwGdnStepClaimV1, now: u64) -> bool {
        if self.settled.contains_key(&claim.claim) || self.cases.contains_key(&claim.claim) {
            return false;
        }
        while self.cases.len() >= PALW_GDN_STEP_MAX_CASES_V1 {
            let Some(oldest) = self.cases.iter().min_by_key(|(_, c)| c.opened_daa).map(|(k, _)| *k) else { break };
            self.cases.remove(&oldest);
        }
        self.cases.insert(claim.claim, PalwGdnStepCaseV1 { claim, stage: PalwGdnStepStageV1::Probing { cursor: 0 }, opened_daa: now });
        true
    }
    pub fn settle(&mut self, claim: &Hash64, why: impl Into<String>) {
        self.cases.remove(claim);
        self.settled.insert(*claim, why.into());
    }
}

/// **One probing step of one case**: re-execute up to `budget` served tiles; returns the signed object on a mismatch, `None` while
/// tiles remain, and settles the case (honest / forged checkpoint) otherwise. `kernels` builds the accuser's fresh recompute
/// kernels (one per probe). Logging is the caller's: the returned note says what happened.
#[allow(clippy::too_many_arguments)]
pub fn palw_gdn_step_probe_case_v1<K: Base0FpRecomputeKernelsV1>(
    book: &mut PalwGdnStepBookV1,
    claim_id: &Hash64,
    material: &Base0GdnMaterialV2<'_>,
    mut kernels: impl FnMut() -> K,
    accuser: PalwBondKeyV2,
    network_domain: Hash64,
    openings: &[PalwArtifactOpeningV1],
    sign: &dyn Fn(&[u8], &[u8]) -> Option<Vec<u8>>,
    budget: usize,
) -> Vec<String> {
    let mut notes = Vec::new();
    let Some(case) = book.cases.get(claim_id) else { return notes };
    let PalwGdnStepStageV1::Probing { cursor } = case.stage else { return notes };
    let claim = case.claim.clone();
    if material.binding.committed_execution_root != claim.execution_root {
        notes.push("the served material's binding is not the claim's execution: waiting for another capture".into());
        return notes;
    }
    let mut served: Vec<u64> = material.tiles.tiles.iter().map(|(i, _)| *i).collect();
    served.sort_unstable();
    served.dedup();
    let window: Vec<u64> = palw_gdn_step_probe_window_v1(&served, cursor, budget).to_vec();
    if window.is_empty() {
        book.settle(claim_id, format!("honest: every one of the {} served step tiles recomputes", served.len()));
        return notes;
    }
    let (profile, ctx) = (&material.binding.shape_profile, &material.binding.job_context);
    let actual = PalwOneMoveClaimV2 { execution_root: claim.execution_root, class_id: claim.class_id, artifact_root: claim.artifact_root };
    for leaf in &window {
        let Some(coord) = kaspa_consensus_core::palw_step::canonical_step_coordinates(profile, ctx, *leaf) else {
            continue;
        };
        let built = base0_gdn_step_evidence_v2(material, coord, &mut kernels).map(PalwGdnStepBuiltV1::from);
        let probe = palw_gdn_step_classify_v1(*leaf, built, |refutation| {
            let trial = palw_gdn_step_accusation_v1(&claim, accuser, *leaf, (**refutation).clone(), openings);
            match palw_gdn_court_verdict_v2(&trial, &actual, material.max_leaves) {
                Ok(PalwShardCourtVerdictV1::ExecutorGuilty) => Ok(true),
                Ok(PalwShardCourtVerdictV1::FalseAccusation) => Ok(false),
                Ok(other) => Err(format!("{other:?}")),
                Err(e) => Err(e.to_string()),
            }
        });
        match probe {
            PalwGdnStepProbeV1::Honest => {}
            PalwGdnStepProbeV1::Unbuildable(why) => notes.push(format!("leaf {leaf}: no step evidence ({why})")),
            PalwGdnStepProbeV1::ForgedCheckpoint { index } => {
                book.settle(
                    claim_id,
                    format!("leaf {leaf} stands on forged checkpoint {index}: the held-GDN filer's case (tag 244), not a step accusation"),
                );
                return notes;
            }
            PalwGdnStepProbeV1::Mismatch { leaf, refutation } => {
                match palw_gdn_step_object_v1(&claim, accuser, network_domain, leaf, *refutation, openings, material.max_leaves, sign) {
                    Ok(object) => {
                        let case = book.cases.get_mut(claim_id).expect("held above");
                        case.stage =
                            PalwGdnStepStageV1::Filed { object: Box::new(object), leaf, sent: PalwGdnStepSentV1::default() };
                        notes.push(format!("leaf {leaf}: the step does not recompute — GdnStepAccusedV2 built and signed"));
                    }
                    Err(why) => {
                        book.settle(claim_id, format!("leaf {leaf}: mismatch found but not fileable: {why}"));
                    }
                }
                return notes;
            }
        }
    }
    if let Some(case) = book.cases.get_mut(claim_id) {
        case.stage = PalwGdnStepStageV1::Probing { cursor: cursor + window.len() };
    }
    notes
}

/// The owned material handed to [`palw_gdn_step_probe_case_v1`] by the host (re-exported for the service's seam).
pub type PalwGdnStepMaterialV1 = Base0GdnOwnedMaterialV2;

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xC0_0000 + n), index: 0 })
    }
    fn facts(phase: PalwClaimPhaseV2) -> PalwGdnStepWindowFactsV1 {
        PalwGdnStepWindowFactsV1 {
            phase,
            final_deadline_daa: Some(1_000),
            hard_deadline_daa: 5_000,
            own_reservation_live: false,
            own_reservation_closed: false,
            reservation_fence: true,
        }
    }

    #[test]
    fn a_refused_or_honest_verdict_is_never_a_mismatch_and_a_forged_predecessor_is_the_held_filers() {
        let step = || Ok(PalwGdnStepBuiltV1::Step(0xABu8));
        assert!(matches!(palw_gdn_step_classify_v1(7, step(), |_| Ok(true)), PalwGdnStepProbeV1::Mismatch { leaf: 7, refutation: 0xAB }));
        assert!(matches!(palw_gdn_step_classify_v1(7, step(), |_| Ok(false)), PalwGdnStepProbeV1::Honest));
        assert!(matches!(palw_gdn_step_classify_v1(7, step(), |_| Err("ram".into())), PalwGdnStepProbeV1::Unbuildable(_)));
        assert!(matches!(
            palw_gdn_step_classify_v1::<u8>(7, Ok(PalwGdnStepBuiltV1::ForgedCheckpoint { index: 3 }), |_| panic!("never judged")),
            PalwGdnStepProbeV1::ForgedCheckpoint { index: 3 }
        ));
        assert!(matches!(
            palw_gdn_step_classify_v1::<u8>(7, Err("not GDN".into()), |_| panic!("never judged")),
            PalwGdnStepProbeV1::Unbuildable(_)
        ));
    }

    #[test]
    fn the_probe_window_rotates_through_served_tiles_and_ends_empty() {
        let served = [2u64, 5, 9, 11, 40];
        assert_eq!(palw_gdn_step_probe_window_v1(&served, 0, 2), &[2, 5]);
        assert_eq!(palw_gdn_step_probe_window_v1(&served, 2, 2), &[9, 11]);
        assert_eq!(palw_gdn_step_probe_window_v1(&served, 4, 2), &[40]);
        assert!(palw_gdn_step_probe_window_v1(&served, 5, 2).is_empty());
        assert!(palw_gdn_step_probe_window_v1(&served, 99, 2).is_empty());
    }

    #[test]
    fn the_window_files_inside_reserves_near_the_reward_and_refuses_after_it() {
        // Far from the Final deadline: file, no reservation.
        assert_eq!(palw_gdn_step_window_v1(&facts(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1 }), 100, 60), PalwGdnStepWindowV1::Open {
            reserve: false
        });
        // Within the margin and the reservation fence is in force: reserve first.
        assert_eq!(palw_gdn_step_window_v1(&facts(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1 }), 950, 60), PalwGdnStepWindowV1::Open {
            reserve: true
        });
        // Within the margin, no fence: still before the deadline, file and let the fold decide.
        let mut f = facts(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1 });
        f.reservation_fence = false;
        assert_eq!(palw_gdn_step_window_v1(&f, 950, 60), PalwGdnStepWindowV1::Open { reserve: false });
        // At/after the deadline, no reservation possible: refused, the exact DAAs logged.
        let PalwGdnStepWindowV1::Closed(why) = palw_gdn_step_window_v1(&f, 1_000, 60) else { panic!("closed") };
        assert!(why.contains("1000"), "{why}");
        // A once-per-life reservation already closed cannot be taken again.
        let mut f = facts(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1 });
        f.own_reservation_closed = true;
        assert!(matches!(palw_gdn_step_window_v1(&f, 1_001, 60), PalwGdnStepWindowV1::Closed(_)));
        // Final: only with this bond's live reservation (the fold's rule).
        assert!(matches!(palw_gdn_step_window_v1(&facts(PalwClaimPhaseV2::Final { final_daa: 1_000 }), 1_100, 60), PalwGdnStepWindowV1::Closed(_)));
        let mut f = facts(PalwClaimPhaseV2::Final { final_daa: 1_000 });
        f.own_reservation_live = true;
        assert_eq!(palw_gdn_step_window_v1(&f, 1_100, 60), PalwGdnStepWindowV1::Open { reserve: false });
        // Voided: nothing to accuse.
        let voided = PalwClaimPhaseV2::Voided { voided_daa: 900, reason: PalwVoidReasonV2::CourtFraud };
        assert!(matches!(palw_gdn_step_window_v1(&facts(voided), 950, 60), PalwGdnStepWindowV1::Closed(_)));
    }

    #[test]
    fn a_reorg_that_drops_the_reservation_resends_it_before_the_accusation_and_the_budget_bounds_it() {
        let sent = PalwGdnStepSentV1 { reservation: Some((100, 1)), accusation: Some((100, 1)) };
        // Too soon: nothing.
        assert!(palw_gdn_step_resend_v1(&sent, PalwGdnStepReservationAtTipV1::Absent, false, 120).is_empty());
        // Past the interval, the reservation gone from the tip and no verdict: both, reservation first.
        assert_eq!(
            palw_gdn_step_resend_v1(&sent, PalwGdnStepReservationAtTipV1::Absent, false, 160),
            vec![PalwGdnStepResendV1::Reservation, PalwGdnStepResendV1::Accusation]
        );
        // The reservation is live: only the accusation.
        assert_eq!(palw_gdn_step_resend_v1(&sent, PalwGdnStepReservationAtTipV1::Live, false, 160), vec![PalwGdnStepResendV1::Accusation]);
        // A closed reservation is never re-sent (once per life); a verdict on chain ends the accusation's re-sends.
        assert!(palw_gdn_step_resend_v1(&sent, PalwGdnStepReservationAtTipV1::Closed, true, 160).is_empty());
        // The send budget.
        let spent = PalwGdnStepSentV1 { reservation: Some((100, PALW_GDN_STEP_MAX_SENDS_V1)), accusation: None };
        assert!(matches!(
            palw_gdn_step_resend_v1(&spent, PalwGdnStepReservationAtTipV1::Absent, false, 200).as_slice(),
            [PalwGdnStepResendV1::GiveUp(_)]
        ));
        // Nothing sent, nothing re-sent.
        assert!(palw_gdn_step_resend_v1(&PalwGdnStepSentV1::default(), PalwGdnStepReservationAtTipV1::Absent, false, 10_000).is_empty());
    }

    #[test]
    fn a_conviction_is_read_from_court_fraud_only_and_hands_the_rooted_contradiction_to_the_false_valid_filer() {
        let claim = Hash64::from_u64_word(0x77);
        assert_eq!(palw_gdn_step_convicted_v1(&PalwClaimPhaseV2::Voided { voided_daa: 42, reason: PalwVoidReasonV2::CourtFraud }), Some(42));
        assert_eq!(palw_gdn_step_convicted_v1(&PalwClaimPhaseV2::Final { final_daa: 42 }), None);
        let proof = palw_gdn_step_false_valid_proof_v1(claim, 42);
        assert_eq!(proof.claim_id, claim);
        assert!(matches!(
            proof.contradiction,
            kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::CourtFraud { voided_daa: 42 }
        ));
        // The false-Valid filer keeps it as the claim's proof; its scan asks EVERY Valid signer of the claim
        // (`palw_false_valid_filings_v1`, pinned by `one_filing_per_seat_never_the_own_bond_and_only_what_the_chain_convicts`).
        let mut filer = crate::palw_filer_false_valid::PalwFalseValidFilerV1::new(1_000, 500, 100);
        assert!(filer.note_proof_v1(proof.clone(), crate::palw_filer_false_valid::PalwFalseValidSourceV1::CourtClose { leaf: 9 }, None, 50));
        assert!(filer.holds(&claim));
        assert!(!filer.note_proof_v1(proof, crate::palw_filer_false_valid::PalwFalseValidSourceV1::CourtClose { leaf: 9 }, None, 51));
    }

    #[test]
    fn the_book_opens_once_settles_for_good_and_is_bounded() {
        let mut book = PalwGdnStepBookV1::default();
        let claim = |n: u64| PalwGdnStepClaimV1 {
            claim: Hash64::from_u64_word(n),
            executor: bond(1),
            execution_root: Hash64::default(),
            trace_root: Hash64::default(),
            class_id: Hash64::default(),
            artifact_root: Hash64::default(),
        };
        assert!(book.open(claim(1), 10));
        assert!(!book.open(claim(1), 11));
        book.settle(&Hash64::from_u64_word(1), "honest");
        assert!(!book.open(claim(1), 12), "a settled claim never reopens");
        for n in 0..(PALW_GDN_STEP_MAX_CASES_V1 as u64 + 5) {
            book.open(claim(100 + n), 100 + n);
        }
        assert_eq!(book.cases.len(), PALW_GDN_STEP_MAX_CASES_V1);
        assert!(!book.cases.contains_key(&Hash64::from_u64_word(100)), "the oldest leaves first");
    }

    /// The honest dev world (the held filer's fixture): every served step tile recomputes, nothing is filed, the case settles honest.
    #[test]
    fn an_honest_producer_is_probed_to_the_end_and_never_accused() {
        let w = crate::palw_gdn_held_filer::tests::world(0);
        let view = w.honest.view();
        let claim = PalwGdnStepClaimV1 {
            claim: Hash64::from_u64_word(0xA1),
            executor: bond(1),
            execution_root: w.honest.binding.committed_execution_root,
            trace_root: Hash64::from_u64_word(2),
            class_id: w.shape_profile_id,
            artifact_root: w.inventory_root,
        };
        let mut book = PalwGdnStepBookV1::default();
        book.open(claim.clone(), 1);
        let sign = |_: &[u8], _: &[u8]| Some(vec![1]);
        for _ in 0..=w.honest.tiles.tiles.len() {
            palw_gdn_step_probe_case_v1(
                &mut book,
                &claim.claim,
                &view,
                || w.backend.gdn_recompute_kernels_v2().unwrap(),
                bond(2),
                Hash64::default(),
                &w.openings,
                &sign,
                PALW_GDN_STEP_PROBES_PER_TICK_V1,
            );
            if book.settled.contains_key(&claim.claim) {
                break;
            }
        }
        let why = book.settled.get(&claim.claim).expect("settled");
        assert!(why.starts_with("honest"), "{why}");
    }
}
