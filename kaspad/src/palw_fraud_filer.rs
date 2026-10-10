//! **Lane LG14-A (RFC-0014 §6): the common fraud filer, the node's half** (node policy; a child module of `palw_panel`, whose loop
//! holds its one book and calls [`PalwPanelService::fraud_filer_tick_v1`] once a tick).
//!
//! **What it closes** (GAP-80, G14 on the legacy V2 Panel route). A claim every seat of its panel signed `Valid` reaches `Final` unless
//! someone OUTSIDE the panel can take it to an objective outcome. Past `palw_legacy_public_filer_v1` any bond can: this node replays
//! the claim's job on its own model (the attempt's job is derived from the claim's block, never served), and when its execution root
//! differs it runs the ONE engine every role runs (`kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_v1`, also
//! what the real-node suite `lg14a_legacy_filer_e2e` drives):
//!
//! 1. **reserve** — tag 154, asked of the fold first (`palw_legacy_dispute_reservation_check_v1`): the claim's ends are held until its
//!    own hard deadline, so no timeout outruns the pursuit;
//! 2. **read the binding** — a reserved `DefaultAccused` of event `(0, 0)`, whose answer carries the claim's step binding (the fold
//!    authenticates it against the claim's `execution_root`);
//! 3. **localize** — past LG14-B's fence a base0-codec replica compares authenticated tag-158 frontiers against its own tree,
//!    descending to the first divergent leaf. Other captures use the bounded, contiguous `StepRange` fallback;
//! 4. **terminal** — the LG14-B replica builds tag 159 from its own registered model and public leaf hashes, or uses a public CKW
//!    to open a fused held court. The fallback demands `StepLeaf`. Silence on a required unit defaults (DA-7).
//!    Public binding/job/shape/output faults go through the reporter door as kind 4 before descent. Checkpoint/trace localization
//!    beyond these direct proofs remains incomplete; step-tree agreement never clears a mismatched execution.
//!
//! **Shared progress** (§7.4): a unit the chain already answered — this node's before a restart, or anybody's — is read off the
//! accepted blocks, never demanded again; the unit must be marked answered for descent. A complete direct proof needs only
//! authenticated public bytes and the recorded target, even when the DA arm refused that disclosure.
//!
//! **Restart** (§6.3): the book is a cache, nothing is persisted. The candidates and every case's chain facts come from the tip, the
//! answers from the accepted blocks (walked back to the oldest pursued claim's acceptance after a start), and the honest run from a
//! fresh replay. A restart costs one replay a case and never a wrong filing.
//!
//! **Who** — identity decides, nothing opts in: a node that carries (`--palw-fee-outpoint`) past the fence. A claim on whose current
//! panel this bond sits is the seat's duties' (P2-6, P2-8b/8d), which this filer leaves alone so one bond never files a claim twice;
//! its own claims are never pursued.
//!
//! **Load.** One replay in flight, started only while the seat's own replay slots have room for a light replay; a mismatch's capture is
//! kept for its localization, at most [`PALW_FRAUD_FILER_HELD_RUNS_V1`] at once (the rest wait their turn); one item on the court queue a
//! case, each sent at most [`PALW_FRAUD_FILER_SENDS_V1`] times; the chain read once a DAA. Every item rides the priority lane dated
//! now: a reservation and a demand are what keep the claim's ends held and the producer's answer window running.
//!
//! **Consensus-inert, and dormant below the fence** — a clean no-op there: the book is emptied and nothing is queued, so a node below
//! `palw_legacy_public_filer_v1` (every shipped preset) behaves byte for byte as before.

use super::*;
use std::collections::BTreeSet;

use super::reporter_filer::{
    PalwConvictionDoorV1, PalwConvictionFilingV1, PalwFileOutcomeV1, PalwFilingOriginV1, PalwReporterFilerV1,
};
use crate::palw_legacy_held_v2::PalwLegacyReplicaV2;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1, palw_da_step_leaf_is_fused_v1};
use kaspa_consensus_core::palw_legacy_held_da_v2::{
    PalwCommittedKernelWitnessV2, PalwLegacyDescentStepV2, PalwLegacyFrontierV2, PalwLegacyHeldAnswerV2, PalwLegacyHeldUnitV2,
    palw_legacy_held_check_answer_v2, palw_legacy_leaf_is_fused_v2,
};
use kaspa_consensus_core::palw_legacy_public_filer_v1::{
    PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, PalwFilerActionV1, PalwFilerPhaseV1, PalwFilerRoleV1, PalwLegacyBisectV1,
    PalwLegacyProbeV1, palw_dispute_reserved_object_v1, palw_fraud_filer_demand_object_v1, palw_fraud_filer_facts_v1,
    palw_fraud_filer_learn_v1, palw_fraud_filer_next_v1, palw_fraud_filer_reservation_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwFraudFilerCandidateV1, PalwLegacyDisputeViewV1};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_core::{debug, error};

/// The court queue's round of this filer's reservation of a claim — a round no other lane keys by (P2-6 `u32::MAX`, the reporter
/// filer `u32::MAX − 3 ..= u32::MAX − 1`, P2-8d `− 6`, lane B `− 7`, the audit duty `− 8`, the rider outbox `− 9`).
pub(super) const PALW_FRAUD_FILER_RESERVE_ROUND_V1: u32 = u32::MAX - 10;
/// The court queue's round of this filer's demand on a claim (one open at a time: DA-1's one session per accuser).
pub(super) const PALW_FRAUD_FILER_DEMAND_ROUND_V1: u32 = u32::MAX - 11;
pub(super) const PALW_FRAUD_FILER_TERMINAL_ROUND_V2: u32 = u32::MAX - 12;
/// The host ledger's role for this filer's replays.
pub(super) const PALW_FRAUD_FILER_REPLAY_ROLE_V1: &str = "fraud-filer";
/// Mismatched runs held in memory at once (each keeps its capture for the localizer's own ranges).
pub(super) const PALW_FRAUD_FILER_HELD_RUNS_V1: usize = 2;
/// Replays one claim gets: the run, and one more after a failure that was this host's.
pub(super) const PALW_FRAUD_FILER_RUNS_PER_CLAIM_V1: u8 = 2;
/// Sends of one item (a reservation, one probe's demand): the first, and one more if the first was lost.
pub(super) const PALW_FRAUD_FILER_SENDS_V1: u8 = 2;
/// A queued item not on chain this long after it was sent is taken as lost.
pub(super) const PALW_FRAUD_FILER_RESEND_DAA_V1: u64 = 30;
/// Chain blocks one page reads at most. A partial page resumes from its cursor; DAA difference is not a bound on chain blocks.
pub(super) const PALW_FRAUD_FILER_WALK_BLOCKS_V1: usize = 40_000;
/// DAA a later walk re-reads below the last one's tip, for a reorg.
pub(super) const PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1: u64 = 64;
/// Cases the book keeps at most (settled ones are dropped first once their claim leaves the candidates).
pub(super) const PALW_FRAUD_FILER_MAX_CASES_V1: usize = 4_096;

/// Whether a court-queue entry is this filer's.
pub(super) fn palw_fraud_filer_queued_v1(round: u32, responder: bool) -> bool {
    !responder
        && (round == PALW_FRAUD_FILER_RESERVE_ROUND_V1
            || round == PALW_FRAUD_FILER_DEMAND_ROUND_V1
            || round == PALW_FRAUD_FILER_TERMINAL_ROUND_V2)
}

/// **One honest run of a claim's job on this host** — the filer's own material, never the producer's: the roots its replay
/// reproduced, and its own leaf hashes of any range (opened from its capture by the backend that ran it, which the closure holds with
/// the capture and the prompt). Holds the ledger's reservation for as long as the capture lives.
pub(super) struct PalwFraudFilerRunV1 {
    pub(super) execution_root: Hash64,
    pub(super) trace_root: Hash64,
    pub(super) output_root: Hash64,
    /// This node's own leaf hashes of `[first, first + count)`.
    pub(super) own_range: Box<dyn Fn(u64, u32) -> Result<Vec<Hash64>, String> + Send + Sync>,
    pub(super) legacy: Option<Arc<PalwLegacyReplicaV2>>,
    pub(super) _reservation: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
}

/// Where this node's judgement of one claim stands.
#[derive(Clone)]
pub(super) enum PalwFraudFilerVerdictV1 {
    /// Not yet replayed (or a replay that failed on this host, to be run once more).
    Pending,
    /// The replay reproduced the claim's execution root: nothing to file.
    Honest,
    /// The replay did not: the engine pursues it.
    Mismatch(Arc<PalwFraudFilerRunV1>),
    /// This host cannot judge it (the class does not resolve, the block is not held, the replay failed twice, a fused leaf).
    Unjudged(String),
    /// The pursuit is over.
    Settled(PalwFilerPhaseV1),
}

/// What this node sent last on a case, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwFraudFilerSentV1 {
    Reserve,
    Demand(PalwLegacyProbeV1),
    Terminal { leaf: u64 },
}

/// One history walk, anchored to the tip where it started. Its watermark advances only when `next` is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PalwFraudFilerWalkV1 {
    pub(super) floor: u64,
    pub(super) anchor: Hash64,
    pub(super) anchor_daa: u64,
    pub(super) next: Option<Hash64>,
}

/// Only units the contiguous fallback reads enter its cache. Authentication and deduplication bound it to one binding plus at most
/// 32 ranges per pursued claim. LG14-B separately holds only the currently selected authenticated node/witness answer.
fn palw_fraud_filer_cache_unit_v1(unit: &PalwDaUnitV1, leaf_count: Option<u64>) -> bool {
    use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
    use kaspa_consensus_core::palw_legacy_public_filer_v1::{
        PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1, PALW_LEGACY_BISECT_FINAL_RANGE_V1,
    };
    match *unit {
        PalwDaUnitV1::Event { row: 0, tile: 0 } => true,
        PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first, count }) => {
            let Some(leaves) = leaf_count else { return false };
            PalwLegacyBisectV1::max_demands(leaves) <= u64::from(PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1)
                && first < leaves
                && first % PALW_LEGACY_BISECT_FINAL_RANGE_V1 == 0
                && u64::from(count) == (leaves - first).min(PALW_LEGACY_BISECT_FINAL_RANGE_V1)
        }
        _ => false,
    }
}

/// Read/resume one page. A reorg beyond the old anchor restarts at the current tip and the oldest pursued claim, including when
/// a recent incremental floor would otherwise hide the new branch's old answers. No watermark is returned on a read failure.
pub(super) fn palw_fraud_filer_read_page_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    oldest: u64,
    floor: u64,
    previous: Option<PalwFraudFilerWalkV1>,
    max_blocks: usize,
    wanted: &BTreeMap<Hash64, Hash64>,
    legacy_wanted: &BTreeMap<Hash64, PalwLegacyHeldUnitV2>,
) -> Result<(PalwFraudFilerWalkV1, bool, Vec<((Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1)>), String> {
    let tip = consensus.get_sink();
    let same_branch = previous.is_none_or(|p| p.anchor == tip || consensus.is_chain_ancestor_of(p.anchor, tip).unwrap_or(false));
    let reset = !same_branch;
    let continuing = previous.filter(|p| same_branch && p.next.is_some());
    let floor = continuing.map_or(if reset { oldest } else { floor }, |p| p.floor);
    let anchor = continuing.map_or(tip, |p| p.anchor);
    let anchor_daa = continuing.map_or_else(
        || consensus.get_header(anchor).map(|h| h.daa_score).map_err(|e| format!("history anchor {anchor}: {e}")),
        |p| Ok(p.anchor_daa),
    )?;
    let start = continuing.and_then(|p| p.next).unwrap_or(tip);
    let mut answers = BTreeMap::new();
    let next = crate::palw_panel::walk_accepted_lifecycle_page_v1(consensus, start, floor, max_blocks, &mut |object| match object {
        PalwConsensusObjectV2::MaterialDisclosedV2 { claim, unit, answer, .. }
            if wanted.get(&claim).is_some_and(|root| {
                kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(&unit, &answer, root)
            }) && palw_fraud_filer_cache_unit_v1(
                &unit,
                match &answer {
                    PalwDaAnswerV1::Held(c) => Some(c.binding.step_leaf_count),
                    _ => None,
                },
            ) =>
        {
            palw_fraud_filer_cache_answer_v1(&mut answers, (claim, unit), PalwDaBuiltAnswerV1::Rcore(answer));
        }
        PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
            if legacy_wanted.get(&answer.claim) == Some(&answer.unit)
                && wanted.get(&answer.claim).is_some_and(|root| {
                    palw_legacy_held_check_answer_v2(
                        root,
                        &answer.unit,
                        &answer.binding,
                        &answer.answer,
                        answer.binding.step_leaf_count,
                    )
                    .is_ok()
                }) =>
        {
            answers
                .entry((answer.claim, PalwDaUnitV1::LegacyHeldV2(answer.unit)))
                .or_insert(PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((answer.binding, answer.answer))));
        }
        _ => {}
    })?;
    Ok((PalwFraudFilerWalkV1 { floor, anchor, anchor_daa, next }, reset, answers.into_iter().collect()))
}

/// Preserve an authenticated binding even when its event bytes are malformed, since the binding can prove a job/shape fault.
/// A valid public event replaces a binding-only carrier, so junk pin bytes cannot hide an older output proof across pages.
pub(super) fn palw_fraud_filer_cache_answer_v1(
    answers: &mut BTreeMap<(Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1>,
    key: (Hash64, PalwDaUnitV1),
    answer: PalwDaBuiltAnswerV1,
) {
    let event_valid = |a: &PalwDaBuiltAnswerV1| match (key.1, a) {
        (PalwDaUnitV1::Event { row, tile }, PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(e))) => {
            let b = e.binding();
            kaspa_consensus_core::palw_step_refute::check_trace_event_disclosure_v1(
                b.full_logits_trace_root,
                b.committed_execution_root,
                row,
                tile,
                e,
                b.step_leaf_count,
            )
            .is_ok()
        }
        _ => false,
    };
    match answers.entry(key) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert(answer);
        }
        std::collections::btree_map::Entry::Occupied(mut slot) if event_valid(&answer) && !event_valid(slot.get()) => {
            slot.insert(answer);
        }
        _ => {}
    }
}

/// **One case** — a candidate claim and this node's pursuit of it.
#[derive(Clone)]
pub(super) struct PalwFraudFilerCaseV1 {
    pub(super) candidate: PalwFraudFilerCandidateV1,
    pub(super) verdict: PalwFraudFilerVerdictV1,
    pub(super) binding: Option<PalwStepBindingV2>,
    pub(super) bisect: PalwLegacyBisectV1,
    pub(super) frontiers: Vec<PalwLegacyFrontierV2>,
    pub(super) witness: Option<Box<PalwCommittedKernelWitnessV2>>,
    pub(super) legacy_wanted: Option<PalwLegacyHeldUnitV2>,
    pub(super) runs: u8,
    /// The last item sent, when, and how many times.
    pub(super) sent: Option<(PalwFraudFilerSentV1, u64, u8)>,
    /// Reporter-door handoffs of the claim's direct proof, bounded like the replay lane's.
    direct_handed: u8,
    direct_asked_at: Option<u64>,
}

impl PalwFraudFilerCaseV1 {
    pub(super) fn new(candidate: PalwFraudFilerCandidateV1) -> Self {
        Self {
            candidate,
            verdict: PalwFraudFilerVerdictV1::Pending,
            binding: None,
            bisect: PalwLegacyBisectV1::new(0),
            frontiers: Vec::new(),
            witness: None,
            legacy_wanted: None,
            runs: 0,
            sent: None,
            direct_handed: 0,
            direct_asked_at: None,
        }
    }

    fn pursued(&self) -> bool {
        matches!(self.verdict, PalwFraudFilerVerdictV1::Mismatch(_))
    }

    pub(super) fn learn(&mut self, probe: PalwLegacyProbeV1, answer: &PalwDaBuiltAnswerV1) -> Result<(), String> {
        match (probe, answer) {
            (PalwLegacyProbeV1::HeldNode { unit }, PalwDaBuiltAnswerV1::LegacyHeldV2(answer)) => {
                let binding = self.binding.as_ref().ok_or("a legacy node is learned after the binding")?;
                if answer.0 != *binding {
                    return Err("the public legacy answer carries another binding".into());
                }
                palw_legacy_held_check_answer_v2(
                    &self.candidate.job.execution_root,
                    &unit,
                    binding,
                    &answer.1,
                    binding.step_leaf_count,
                )
                .map_err(|e| format!("the public legacy answer does not authenticate: {e}"))?;
                match (unit, &answer.1) {
                    (PalwLegacyHeldUnitV2::StepNode { level, index }, PalwLegacyHeldAnswerV2::Node { frontier, .. }) => {
                        let frontier = PalwLegacyFrontierV2::of_answer(binding.step_leaf_count, level, index, frontier)
                            .ok_or("the public frontier is not the requested node")?;
                        if let Some(prior) = self.frontiers.iter().find(|f| f.level == level && f.index == index) {
                            if *prior != frontier {
                                return Err("conflicting frontiers of one authenticated node".into());
                            }
                        } else {
                            let cap =
                                kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_descent_rounds_v2(binding.step_leaf_count);
                            if self.frontiers.len() >= cap as usize {
                                return Err("more frontiers than this descent can consume".into());
                            }
                            self.frontiers.push(frontier);
                        }
                    }
                    (PalwLegacyHeldUnitV2::KernelWitness { .. }, PalwLegacyHeldAnswerV2::KernelWitness(witness)) => {
                        self.witness = Some(witness.clone());
                    }
                    _ => return Err("this filer's step descent does not consume that legacy unit".into()),
                }
                self.legacy_wanted = None;
                Ok(())
            }
            (_, PalwDaBuiltAnswerV1::Rcore(answer)) => {
                let PalwFraudFilerVerdictV1::Mismatch(run) = &self.verdict else { return Err("no mismatched replay is held".into()) };
                palw_fraud_filer_learn_v1(
                    probe,
                    answer,
                    &self.candidate.job.execution_root,
                    &mut self.binding,
                    &mut self.bisect,
                    |first, count| (run.own_range)(first, count),
                )
            }
            _ => Err("the answer has another carriage than the probe".into()),
        }
    }

    /// May `item` be sent now? A different item always; the same one only once [`PALW_FRAUD_FILER_RESEND_DAA_V1`] passed since it was
    /// sent and fewer than [`PALW_FRAUD_FILER_SENDS_V1`] sends were made. `Err` once the sends are spent.
    fn may_send(&self, item: PalwFraudFilerSentV1, now_daa: u64) -> Result<bool, ()> {
        match self.sent {
            Some((last, at, sends)) if last == item => {
                if now_daa.saturating_sub(at) < PALW_FRAUD_FILER_RESEND_DAA_V1 {
                    Ok(false)
                } else if sends >= PALW_FRAUD_FILER_SENDS_V1 {
                    Err(())
                } else {
                    Ok(true)
                }
            }
            _ => Ok(true),
        }
    }

    fn note_sent(&mut self, item: PalwFraudFilerSentV1, now_daa: u64) {
        let sends = match self.sent {
            Some((last, _, sends)) if last == item => sends.saturating_add(1),
            _ => 1,
        };
        self.sent = Some((item, now_daa, sends));
    }
}

/// **The filer's book** (the module's header): the cases by claim, the replay in flight, and the answers read off the accepted blocks.
#[derive(Default)]
pub(super) struct PalwFraudFilerBookV1 {
    pub(super) cases: BTreeMap<Hash64, PalwFraudFilerCaseV1>,
    read_at: Option<u64>,
    running: Option<(Hash64, tokio::task::JoinHandle<Result<PalwFraudFilerRunV1, String>>)>,
    /// `(claim, unit) → authenticated answer`, only the binding and consecutive ranges this localizer reads.
    pub(super) answers: BTreeMap<(Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1>,
    /// The DAA the walks have read down to (`None`: never walked since the start).
    walked_from: Option<u64>,
    /// The original tip's DAA at the last COMPLETED walk.
    walked_to: Option<u64>,
    /// The current page's cursor and branch anchor, or the last completed walk's anchor.
    walk: Option<PalwFraudFilerWalkV1>,
    /// This bond's role (an operator's genesis bond, or any other) — read once per refresh.
    role: Option<PalwFilerRoleV1>,
}

impl PalwFraudFilerBookV1 {
    /// Forget everything (below the fence, or a node that does not carry).
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// **Take a fresh candidate read**: a new claim opens a case; a case whose claim left the candidates is dropped unless it is
    /// pursued (its outcome is read off the tip); a case's job facts follow the tip. Bounded by [`PALW_FRAUD_FILER_MAX_CASES_V1`].
    pub(super) fn refresh(&mut self, candidates: Vec<PalwFraudFilerCandidateV1>, now_daa: u64) {
        let fresh: BTreeSet<Hash64> = candidates.iter().map(|c| c.claim_id).collect();
        self.cases.retain(|claim, case| fresh.contains(claim) || case.pursued());
        for candidate in candidates {
            let room = self.cases.len() < PALW_FRAUD_FILER_MAX_CASES_V1;
            match self.cases.get_mut(&candidate.claim_id) {
                Some(case) => case.candidate = candidate,
                None if room => {
                    self.cases.insert(candidate.claim_id, PalwFraudFilerCaseV1::new(candidate));
                }
                None => {}
            }
        }
        self.read_at = Some(now_daa);
    }

    /// Whether the chain should be read again at `now_daa`.
    pub(super) fn stale(&self, now_daa: u64) -> bool {
        self.read_at != Some(now_daa)
    }

    /// Mismatched runs held now.
    pub(super) fn held_runs(&self) -> usize {
        self.cases.values().filter(|case| case.pursued()).count()
    }

    /// **The next claim to replay**: none while one runs or the held runs are at their cap; else the oldest pending case this bond
    /// files as a non-seat (not a seat of its panel), oldest acceptance first.
    pub(super) fn next_replay(&self) -> Option<Hash64> {
        if self.running.is_some() || self.held_runs() >= PALW_FRAUD_FILER_HELD_RUNS_V1 {
            return None;
        }
        self.cases
            .values()
            .filter(|case| matches!(case.verdict, PalwFraudFilerVerdictV1::Pending) && !case.candidate.seat)
            .min_by_key(|case| (case.candidate.accepted_daa, case.candidate.claim_id))
            .map(|case| case.candidate.claim_id)
    }

    /// The claims the engine steps this tick, oldest acceptance first.
    pub(super) fn pursued(&self) -> Vec<Hash64> {
        let mut out: Vec<(u64, Hash64)> = self
            .cases
            .values()
            .filter(|case| case.pursued())
            .map(|case| (case.candidate.accepted_daa, case.candidate.claim_id))
            .collect();
        out.sort();
        out.into_iter().map(|(_, claim)| claim).collect()
    }

    /// Judge all three committed roots. An output-only or trace-only mismatch still needs a public proof or DA default.
    pub(super) fn judge(&mut self, claim: Hash64, result: Result<PalwFraudFilerRunV1, String>) -> Option<&PalwFraudFilerVerdictV1> {
        let case = self.cases.get_mut(&claim)?;
        case.runs = case.runs.saturating_add(1);
        case.verdict = match result {
            Ok(run)
                if run.execution_root != case.candidate.job.execution_root
                    || run.trace_root != case.candidate.job.trace_root
                    || run.output_root != case.candidate.job.output_root =>
            {
                PalwFraudFilerVerdictV1::Mismatch(Arc::new(run))
            }
            Ok(_) => PalwFraudFilerVerdictV1::Honest,
            Err(_) if case.runs < PALW_FRAUD_FILER_RUNS_PER_CLAIM_V1 => PalwFraudFilerVerdictV1::Pending,
            Err(why) => PalwFraudFilerVerdictV1::Unjudged(format!("the replay failed twice: {why}")),
        };
        if case.pursued() {
            // Earlier walks did not collect answers for this case while it was pending, even if its acceptance is newer than
            // the old floor. Backfill every newly pursued case; do not skip it using another case's completed watermark.
            self.walked_from = None;
            self.walked_to = None;
            self.walk = None;
            self.read_at = None;
        }
        Some(&case.verdict)
    }

    /// The active backfill's floor, else the oldest pursued claim's acceptance before the first completed walk, else the last
    /// completed walk's original tip less a reorg margin. Every new pursuit invalidates the shared completion watermark.
    pub(super) fn walk_floor(&self) -> Option<u64> {
        let oldest = self.cases.values().filter(|case| case.pursued()).map(|case| case.candidate.accepted_daa).min()?;
        if let Some(walk) = self.walk.filter(|w| w.next.is_some()) {
            return Some(walk.floor);
        }
        Some(match (self.walked_from, self.walked_to) {
            (Some(from), Some(to)) if from <= oldest => to.saturating_sub(PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1),
            _ => oldest,
        })
    }

    /// Record only the range actually read. Partial pages retain their cursor and original tip; a branch restart discards the
    /// old branch's watermark/cache. Authenticated answers from a successful page can be used before the backfill completes.
    pub(super) fn walked(
        &mut self,
        walk: PalwFraudFilerWalkV1,
        reset: bool,
        answers: Vec<((Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1)>,
    ) {
        if reset {
            self.walked_from = None;
            self.walked_to = None;
            self.answers.clear();
            for case in self.cases.values_mut() {
                case.frontiers.clear();
                case.witness = None;
                case.binding = None;
                case.bisect = PalwLegacyBisectV1::new(0);
            }
        }
        if walk.next.is_none() {
            self.walked_from = Some(self.walked_from.map_or(walk.floor, |from| from.min(walk.floor)));
            self.walked_to = Some(walk.anchor_daa);
        }
        self.walk = Some(walk);
        for (key, answer) in answers {
            if self.cases.get(&key.0).is_some_and(|case| {
                case.pursued()
                    && match &answer {
                        PalwDaBuiltAnswerV1::Rcore(answer) => {
                            kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
                                &key.1,
                                answer,
                                &case.candidate.job.execution_root,
                            )
                        }
                        PalwDaBuiltAnswerV1::LegacyHeldV2(answer) => {
                            let PalwDaUnitV1::LegacyHeldV2(unit) = key.1 else { return false };
                            case.legacy_wanted == Some(unit)
                                && palw_legacy_held_check_answer_v2(
                                    &case.candidate.job.execution_root,
                                    &unit,
                                    &answer.0,
                                    &answer.1,
                                    answer.0.step_leaf_count,
                                )
                                .is_ok()
                        }
                    }
            }) {
                palw_fraud_filer_cache_answer_v1(&mut self.answers, key, answer);
            }
        }
        // An answer of a claim no longer pursued leaves with its case.
        let pursued: BTreeSet<Hash64> = self.pursued().into_iter().collect();
        self.answers.retain(|(claim, _), _| pursued.contains(claim));
    }

    /// Settle a case: its run (and the capture's reservation) and cached answers are let go.
    pub(super) fn settle(&mut self, claim: &Hash64, verdict: PalwFraudFilerVerdictV1) {
        if let Some(case) = self.cases.get_mut(claim) {
            case.verdict = verdict;
        }
        self.answers.retain(|(held, _), _| held != claim);
        if let Some(case) = self.cases.get_mut(claim) {
            case.frontiers.clear();
            case.witness = None;
            case.legacy_wanted = None;
        }
    }

    /// A newer binding-only carrier can be read before an older valid token pin. Do not discard the pursuit on descent's
    /// inability to explain matching step trees until that public backfill completes.
    fn descent_failed(&mut self, claim: &Hash64, why: String) -> bool {
        if self.walk.is_none_or(|walk| walk.next.is_some()) {
            return false;
        }
        self.settle(claim, PalwFraudFilerVerdictV1::Unjudged(why));
        true
    }

    /// Reading a previously answered legacy unit needs its historical bytes, even if a prior walk predated this selection.
    pub(super) fn want_legacy(&mut self, claim: Hash64, unit: PalwLegacyHeldUnitV2) {
        let Some(case) = self.cases.get_mut(&claim) else { return };
        if case.legacy_wanted == Some(unit) {
            return;
        }
        case.legacy_wanted = Some(unit);
        self.answers.retain(|(held, unit), _| *held != claim || !matches!(unit, PalwDaUnitV1::LegacyHeldV2(_)));
        self.walked_from = None;
        self.walked_to = None;
        self.walk = None;
        self.read_at = None;
    }
}

/// **The engine's step for one case**, decided off what the chain says and what this node already read (pure, so the tests drive it):
/// the engine's own action, except that a demand the chain already answered is READ (shared progress, §7.4) — `Learn` — and one it
/// has not answered while this node holds no answer read for it yet is demanded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PalwFraudFilerStepV1 {
    Engine(PalwFilerActionV1),
    Learn(PalwLegacyProbeV1),
    /// The chain marks the unit answered but the walk has not read the answer yet.
    AwaitAnswer(PalwLegacyProbeV1),
    LegacyTerminal {
        leaf: u64,
    },
}

/// The node's LG14-B step: the common reservation/DA/outcome engine, an own replica, and authenticated public frontiers.
pub(super) fn palw_fraud_filer_legacy_step_v2(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    own_court_open: bool,
    case: &PalwFraudFilerCaseV1,
    answer_read: impl Fn(&PalwDaUnitV1) -> bool,
) -> Result<PalwFraudFilerStepV1, String> {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    if let Some(outcome) = facts.outcome {
        return Ok(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(outcome)));
    }
    if own_court_open && facts.outcome.is_none() {
        return Ok(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait));
    }
    let descent = match (&case.binding, &case.verdict) {
        (Some(binding), PalwFraudFilerVerdictV1::Mismatch(run))
            if !facts.session_open
                && facts.accusable
                && (facts.reserved || matches!(role, PalwFilerRoleV1::Seat | PalwFilerRoleV1::Watchdog)) =>
        {
            run.legacy.as_ref().ok_or("this replay has no legacy replica")?.descent(binding, &case.frontiers)?
        }
        _ => PalwLegacyDescentStepV2::Agrees,
    };
    let mut action = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_descent_v1(
        role,
        &facts,
        case.pursued(),
        descent,
        |_| false,
    );
    if let PalwFilerActionV1::HeldRoute { leaf } = action {
        let binding = case.binding.as_ref().ok_or("a located leaf needs the authenticated binding")?;
        if !palw_legacy_leaf_is_fused_v2(binding, leaf) || case.witness.is_some() {
            return Ok(PalwFraudFilerStepV1::LegacyTerminal { leaf });
        }
        action = PalwFilerActionV1::Demand(PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::KernelWitness { leaf } });
    }
    if matches!(action, PalwFilerActionV1::Done(PalwFilerPhaseV1::Honest)) && case.pursued() {
        return Err("the own step tree agrees but the execution root differs: checkpoint/trace/job localization is required".into());
    }
    Ok(match action {
        PalwFilerActionV1::Demand(probe) => {
            let unit = probe.unit();
            match (view.is_some_and(|view| view.answered.contains(&unit)), answer_read(&unit)) {
                (true, true) => PalwFraudFilerStepV1::Learn(probe),
                (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
                _ => PalwFraudFilerStepV1::Engine(action),
            }
        }
        _ => PalwFraudFilerStepV1::Engine(action),
    })
}

pub(super) fn palw_fraud_filer_step_v1(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    answer_read: impl Fn(&PalwDaUnitV1) -> bool,
) -> PalwFraudFilerStepV1 {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    let binding = case.binding.as_ref();
    let action = palw_fraud_filer_next_v1(role, &facts, case.pursued(), &case.bisect, |leaf| {
        binding.is_some_and(|binding| !palw_da_step_leaf_is_fused_v1(binding, leaf))
    });
    match action {
        PalwFilerActionV1::Demand(probe) if !matches!(probe, PalwLegacyProbeV1::Terminal { .. }) => {
            let unit = probe.unit();
            match (view.is_some_and(|view| view.answered.contains(&unit)), answer_read(&unit)) {
                (true, true) => PalwFraudFilerStepV1::Learn(probe),
                (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
                (false, _) => PalwFraudFilerStepV1::Engine(action),
            }
        }
        action => PalwFraudFilerStepV1::Engine(action),
    }
}

/// A direct proof from one public, authenticated event and the claim facts the tip recorded.
/// No producer capture, own replay, Panel receipt or answered-session flag enters this decision.
pub(super) fn palw_fraud_filer_public_filing_v1(
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    rules: kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1,
    answer: &PalwDaBuiltAnswerV1,
    file_by: Option<u64>,
) -> Result<Option<PalwConvictionFilingV1>, String> {
    use kaspa_consensus_core::palw_offence_attribution_v1::{
        palw_binding_identity_fault_v1, palw_output_fault_v1, palw_prompt_not_anchored_admit_v1, palw_prompt_not_anchored_fault_v1,
    };
    use kaspa_consensus_core::palw_offence_v1::{PalwPanelContradictionV1, PalwPromptProofV1};
    use kaspa_consensus_core::palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodeTokensV1, PalwTraceEventDisclosureV1};
    let PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event)) = answer else { return Ok(None) };
    let binding = event.binding();
    if !kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
        &PalwDaUnitV1::Event { row: 0, tile: 0 },
        &PalwDaAnswerV1::Event(event.clone()),
        &target.execution_root,
    ) {
        return Err("the public event does not authenticate to this claim".into());
    }
    let structural = kaspa_consensus_core::palw_step_leg::PalwStepRefutationV1 {
        binding: binding.clone(),
        evidence: kaspa_consensus_core::palw_step_leg::PalwStepEvidenceV1::Shape,
    };
    let contradiction = if palw_binding_identity_fault_v1(target, binding, rules, true).map_err(|e| e.to_string())?.is_some() {
        PalwPanelContradictionV1::IdentityMismatch { binding: binding.clone() }
    } else if kaspa_consensus_core::palw_step_leg::check_step_refutation_capped_v1(&structural, binding.step_leaf_count).is_ok() {
        PalwPanelContradictionV1::StepStructural(structural)
    } else if palw_prompt_not_anchored_admit_v1(target, binding, rules)
        .is_ok_and(|prefill| u64::from(prefill) <= kaspa_consensus_core::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1)
        && palw_prompt_not_anchored_fault_v1(target, binding, &PalwPromptProofV1::Whole, rules).map_err(|e| e.to_string())?
    {
        // Derive the anchor's prompt, not the producer's ids. The real gate still charges this Whole proof's heavy budget and rent.
        PalwPanelContradictionV1::PromptNotAnchored { binding: binding.clone(), proof: PalwPromptProofV1::Whole }
    } else {
        let pin = match event {
            PalwTraceEventDisclosureV1::Flat { pin, .. } => PalwDecodeTokenPinV1::Base0V1(pin.clone()),
            PalwTraceEventDisclosureV1::Tiled { generated_token_ids, row_opening, .. } => {
                let rows_root = kaspa_consensus_core::palw_step_leg::step_opening_root_capped_v1(
                    u64::from(binding.job_context.exact_decode_tokens),
                    row_opening,
                    binding.step_leaf_count,
                )
                .map_err(|e| e.to_string())?;
                PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: generated_token_ids.clone() })
            }
            PalwTraceEventDisclosureV1::OutOfRange { .. } => return Ok(None),
        };
        if !palw_output_fault_v1(target, binding, &pin).map_err(|e| e.to_string())? {
            return Ok(None);
        }
        PalwPanelContradictionV1::OutputMismatch { binding: binding.clone(), pin }
    };
    let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_v1(
        target.claim_id,
        target.executor_bond,
        contradiction,
        None,
    )
    .map_err(|e| e.to_string())?;
    let filing = PalwConvictionFilingV1::of_offence(built.object, target.claim_id, file_by, PalwFilingOriginV1::Replay)
        .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
        .ok_or("the reporter's key differs from the proof builder's")?;
    Ok(Some(filing))
}

pub(super) fn palw_fraud_filer_sign_terminal_v2(
    object: PalwConsensusObjectV2,
    domain: &Hash64,
    ceiling: u64,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    let object = match object {
        PalwConsensusObjectV2::LegacyLeafRecomputedV2 { accusation } => {
            if kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_leaf_recompute_bytes_v2(&accusation) > ceiling {
                return Err("the public leaf recompute exceeds the close ceiling".into());
            }
            crate::palw_legacy_held_v2::palw_legacy_leaf_recompute_object_v2(domain, *accusation, sign)?
        }
        PalwConsensusObjectV2::ShardCourtAccused { mut accusation } => {
            use kaspa_consensus_core::palw_shard_court_v1::{PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, palw_shard_court_session_id_v1};
            if kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_accusation_bytes_v1(&accusation) > ceiling {
                return Err("the public fused accusation exceeds the close ceiling".into());
            }
            let sid = palw_shard_court_session_id_v1(domain.as_byte_slice(), &accusation);
            accusation.signature = sign(sid.as_byte_slice(), PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT)
                .filter(|s| !s.is_empty())
                .ok_or("no signing key for the fused accusation")?;
            PalwConsensusObjectV2::ShardCourtAccused { accusation }
        }
        _ => return Err("not a public legacy terminal".into()),
    };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).map_err(|e| e.to_string())?;
    Ok(object)
}

impl PalwPanelService {
    /// **The filer's half of the tick** (the module's header): read the candidates and the accepted answers once a DAA, poll and judge
    /// the replay in flight, start the next one where the seat's slots have room, and step every pursued case through the engine —
    /// each item asked of the fold or built by the one builder, queued on the court lane dated now.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn fraud_filer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        reporter: &mut PalwReporterFilerV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        seat_replay_room: bool,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        // What the priority lane carried of this filer's items since the last tick, said and forgotten here: the filer keeps its own
        // spacing (`sent`), never `court_moved`'s debounce.
        court_moved.retain(|(claim, round, responder), at| {
            if !palw_fraud_filer_queued_v1(*round, *responder) {
                return true;
            }
            info!("[{PALW_PANEL}] claim {claim}: the fraud filer's item (round {round}) was carried at DAA {at} (LG14-A)");
            false
        });
        let params = &self.consensus_config.params;
        if !(params.palw_legacy_public_filer_active_at(current_daa) && self.config.fee_outpoint.is_some()) {
            if !book.cases.is_empty() || book.running.is_some() {
                book.clear();
            }
            court_pending.retain(|(_, round, responder, _)| !palw_fraud_filer_queued_v1(*round, *responder));
            return;
        }
        if book.stale(current_daa) {
            let candidates = session.clone().spawn_blocking(move |c| c.palw_fraud_filer_candidates_v1(bond_key)).await;
            book.refresh(candidates, current_daa);
            if book.role.is_none() {
                let operator = palw_operator_da::palw_operator_registrations_v1(params).iter().any(|(bond, _)| *bond == bond_key);
                book.role = Some(if operator { PalwFilerRoleV1::Operator } else { PalwFilerRoleV1::PublicBond });
            }
            if let Some(floor) = book.walk_floor() {
                let wanted: BTreeMap<Hash64, Hash64> = book
                    .cases
                    .values()
                    .filter(|case| case.pursued())
                    .map(|case| (case.candidate.claim_id, case.candidate.job.execution_root))
                    .collect();
                let oldest = book.cases.values().filter(|case| case.pursued()).map(|case| case.candidate.accepted_daa).min().unwrap();
                let previous = book.walk;
                let legacy_wanted =
                    book.cases.values().filter_map(|case| case.legacy_wanted.map(|unit| (case.candidate.claim_id, unit))).collect();
                let result = session
                    .clone()
                    .spawn_blocking(move |c| {
                        palw_fraud_filer_read_page_v1(
                            c,
                            oldest,
                            floor,
                            previous,
                            PALW_FRAUD_FILER_WALK_BLOCKS_V1,
                            &wanted,
                            &legacy_wanted,
                        )
                    })
                    .await;
                match result {
                    Ok((walk, reset, answers)) => book.walked(walk, reset, answers),
                    Err(why) => {
                        warn!("[{PALW_PANEL}] the fraud filer's public history page is unavailable: {why}; watermark unchanged")
                    }
                }
            }
        }
        // The replay in flight, polled every tick until it returns.
        if book.running.as_ref().is_some_and(|(_, handle)| handle.is_finished()) {
            let (claim, handle) = book.running.take().expect("held above");
            let result = handle.await.unwrap_or_else(|e| Err(format!("the replay task did not finish: {e}")));
            match book.judge(claim, result) {
                Some(PalwFraudFilerVerdictV1::Mismatch(run)) => error!(
                    "[{PALW_PANEL}] claim {claim}: this node's replay of the claim's job does NOT reproduce its execution root (replayed \
                     {}) — every seat of its panel let it pass or none looked; pursuing it as a public bond (LG14-A, RFC-0014 §6)",
                    run.execution_root
                ),
                Some(PalwFraudFilerVerdictV1::Honest) => {
                    debug!("[{PALW_PANEL}] claim {claim}: the fraud filer's replay reproduces the claim — honest, nothing filed")
                }
                Some(PalwFraudFilerVerdictV1::Unjudged(why)) => {
                    warn!("[{PALW_PANEL}] claim {claim}: the fraud filer cannot judge the claim — {why} (LG14-A)")
                }
                _ => {}
            }
        }
        if seat_replay_room && let Some(claim) = book.next_replay() {
            self.fraud_filer_start_replay_v1(session, book, claim, network_domain);
        }
        // Every pursued case, one engine step each (a step that reads an answer runs on to the next).
        let role = book.role.unwrap_or(PalwFilerRoleV1::PublicBond);
        let form = params.palw_prompt_ids_form_at(current_daa);
        for claim in book.pursued() {
            let (view, reservable, own_court_open, target) = session
                .clone()
                .spawn_blocking(move |c| {
                    let view = c.palw_legacy_dispute_v1(claim);
                    let reservable = view
                        .as_ref()
                        .and_then(|view| c.palw_legacy_dispute_reservation_check_v1(palw_fraud_filer_reservation_v1(view, bond_key)))
                        .is_some_and(|check: Result<u128, String>| check.is_ok());
                    let own_court_open =
                        c.palw_court_duties_v2(vec![bond_key]).iter().any(|duty| duty.claim_id == claim && !duty.i_am_responder);
                    (view, reservable, own_court_open, c.palw_fraud_filer_target_v1(claim))
                })
                .await;
            let facts = palw_fraud_filer_facts_v1(
                view.as_ref(),
                &bond_key,
                reservable,
                book.cases.get(&claim).is_some_and(|case| case.binding.is_some()),
            );
            if facts.ended || facts.outcome.is_some() {
                court_pending.retain(|(held, round, responder, _)| *held != claim || !palw_fraud_filer_queued_v1(*round, *responder));
                book.settle(&claim, PalwFraudFilerVerdictV1::Settled(facts.outcome.unwrap_or(PalwFilerPhaseV1::Expired)));
                continue;
            }
            // Even a disclosure the session arm refused can carry a complete cryptographic proof of a wrong job.
            // Judge the binding against the recorded target BEFORE a mismatched count can stop subtree descent.
            // The reporter door rehearses the actual gate; an open colluder court cannot veto a valid direct proof.
            if params.palw_offence_attribution_active_at(current_daa)
                && let (Some(target), Some(answer), kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle)) =
                    (target.as_ref(), book.answers.get(&(claim, PalwDaUnitV1::Event { row: 0, tile: 0 })), &params.palw_consensus_mode)
            {
                let rules = kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1 {
                    prompt_ids_form: params.palw_prompt_ids_form_at(current_daa),
                    base_class_id: bundle.base_class_id,
                    da_signer_liability: params.palw_rcore_plus_active_at(current_daa),
                };
                let file_by = view.as_ref().map(|v| v.hard_deadline_daa.saturating_sub(2));
                // The longest canonical prompt needs a whole-root recompute. Reserve its temporary ids/tree and proof buffers,
                // then do all direct-proof hashing off the service loop. Pressure delays this pursuit rather than settling it.
                let prefill = match answer {
                    PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event)) => {
                        kaspa_consensus_core::palw_offence_attribution_v1::palw_prompt_not_anchored_admit_v1(
                            target,
                            event.binding(),
                            rules,
                        )
                        .ok()
                        .map(u64::from)
                        .unwrap_or(0)
                        .min(kaspa_consensus_core::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1)
                    }
                    _ => 0,
                };
                let tiles = prefill.div_ceil(u64::from(kaspa_consensus_core::palw_prompt_ids_v1::PALW_PROMPT_IDS_TILE_LEN));
                let bytes = prefill
                    .saturating_mul(8)
                    .saturating_add(tiles.saturating_mul(256))
                    .saturating_add(2 * kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES);
                let reserved = match crate::palw_memory_ledger::host_ledger_v1().reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "public-binding-proof",
                        class_id: target.class_id,
                        job: claim,
                    },
                    bytes,
                ) {
                    Ok(reserved) => reserved,
                    Err(why) => {
                        debug!("[{PALW_PANEL}] claim {claim}: the public direct proof waits for memory: {why}");
                        continue;
                    }
                };
                let target = target.clone();
                let answer = answer.clone();
                let proof = tokio::task::spawn_blocking(move || {
                    let _reserved = reserved;
                    palw_fraud_filer_public_filing_v1(&target, rules, &answer, file_by)
                })
                .await
                .unwrap_or_else(|e| Err(format!("the public proof worker failed: {e}")));
                match proof {
                    Ok(Some(filing)) => {
                        // This proof supersedes any unsent localization carrier. Keep the chain's live obligations until conviction.
                        court_pending
                            .retain(|(held, round, responder, _)| *held != claim || !palw_fraud_filer_queued_v1(*round, *responder));
                        let mut door = self.conviction_door_v1(session, reporter, bond_key, network_domain);
                        let case = book.cases.get_mut(&claim).expect("pursued above");
                        if !door.holds(&filing.offence_key)
                            && case.direct_handed < super::reporter_filer::PALW_FILER_HAND_OFFS_PER_OFFENCE_V1
                            && case.direct_asked_at.is_none_or(|at| current_daa.saturating_sub(at) >= PALW_FRAUD_FILER_RESEND_DAA_V1)
                        {
                            case.direct_asked_at = Some(current_daa);
                            match door.file(filing) {
                                PalwFileOutcomeV1::Queued { .. } | PalwFileOutcomeV1::AlreadyFiled => case.direct_handed += 1,
                                PalwFileOutcomeV1::AlreadyConvicted => {}
                                why => {
                                    warn!("[{PALW_PANEL}] claim {claim}: the public direct proof waits at the reporter door: {why:?}")
                                }
                            }
                        }
                        continue;
                    }
                    Ok(None) => {}
                    Err(why) => debug!("[{PALW_PANEL}] claim {claim}: this public event yields no direct proof: {why}"),
                }
            }
            // A queued item can have landed on another branch/read, or another public party may have answered its unit.
            // Query the chain before a pending carrier suppresses progression; discard duplicates before paying to resend them.
            court_pending.retain(|(held, round, responder, _)| {
                if *held != claim || !palw_fraud_filer_queued_v1(*round, *responder) { return true; }
                if *round == PALW_FRAUD_FILER_RESERVE_ROUND_V1 { return !facts.reserved; }
                if *round == PALW_FRAUD_FILER_TERMINAL_ROUND_V2 { return !own_court_open; }
                if facts.session_open { return false; }
                !book.cases.get(&claim).is_some_and(|case| matches!(case.sent,
                    Some((PalwFraudFilerSentV1::Demand(probe), _, _)) if view.as_ref().is_some_and(|v| v.answered.contains(&probe.unit()))))
            });
            if court_pending.iter().any(|(held, round, responder, _)| *held == claim && palw_fraud_filer_queued_v1(*round, *responder))
            {
                continue;
            }
            for _ in 0..4 {
                let Some(case) = book.cases.get(&claim) else { break };
                let legacy = params.palw_legacy_held_da_v2_active_at(current_daa)
                    && matches!(&case.verdict, PalwFraudFilerVerdictV1::Mismatch(run) if run.legacy.is_some());
                let step = if legacy {
                    let owned = case.clone();
                    let view = view.clone();
                    let read: BTreeSet<PalwDaUnitV1> =
                        book.answers.keys().filter(|(held, _)| *held == claim).map(|(_, unit)| *unit).collect();
                    match tokio::task::spawn_blocking(move || {
                        palw_fraud_filer_legacy_step_v2(role, view.as_ref(), &bond_key, reservable, own_court_open, &owned, |unit| {
                            read.contains(unit)
                        })
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("the public descent task did not finish: {e}")))
                    {
                        Ok(step) => step,
                        Err(why) => {
                            warn!("[{PALW_PANEL}] claim {claim}: the public legacy descent cannot proceed: {why}");
                            if !book.descent_failed(&claim, why) {
                                debug!("[{PALW_PANEL}] claim {claim}: waiting for the remaining public proof history before settling");
                            }
                            break;
                        }
                    }
                } else {
                    palw_fraud_filer_step_v1(role, view.as_ref(), &bond_key, reservable, case, |unit| {
                        book.answers.contains_key(&(claim, *unit))
                    })
                };
                match step {
                    PalwFraudFilerStepV1::Learn(probe) => {
                        let answer = book.answers.get(&(claim, probe.unit())).cloned().expect("read above");
                        let mut owned = case.clone();
                        let learned = tokio::task::spawn_blocking(move || owned.learn(probe, &answer).map(|()| owned))
                            .await
                            .unwrap_or_else(|e| Err(format!("the read did not finish: {e}")));
                        match learned {
                            Ok(owned) => {
                                let case = book.cases.get_mut(&claim).expect("held above");
                                case.binding = owned.binding;
                                case.bisect = owned.bisect;
                                case.frontiers = owned.frontiers;
                                case.witness = owned.witness;
                                case.legacy_wanted = owned.legacy_wanted;
                                if matches!(probe, PalwLegacyProbeV1::HeldNode { .. }) {
                                    book.answers.remove(&(claim, probe.unit()));
                                }
                                debug!("[{PALW_PANEL}] claim {claim}: read the authenticated chain answer to {probe:?}");
                            }
                            Err(why) => {
                                warn!("[{PALW_PANEL}] claim {claim}: the chain's answer to {probe:?} does not read: {why} (LG14-A)");
                                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
                                break;
                            }
                        }
                    }
                    PalwFraudFilerStepV1::AwaitAnswer(probe) => {
                        if let PalwLegacyProbeV1::HeldNode { unit } = probe {
                            book.want_legacy(claim, unit);
                        }
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait) => break,
                    PalwFraudFilerStepV1::LegacyTerminal { leaf } => {
                        let item = PalwFraudFilerSentV1::Terminal { leaf };
                        if case.may_send(item, current_daa) != Ok(true) {
                            self.fraud_filer_queue_v1(book, claim, item, current_daa, court_pending, court_due, || {
                                Err("the terminal was not delivered after its retry".into())
                            });
                            break;
                        }
                        let PalwFraudFilerVerdictV1::Mismatch(run) = &case.verdict else { break };
                        let Some(replica) = run.legacy.clone() else { break };
                        let Some(binding) = case.binding.clone() else { break };
                        let Some(artifact_root) = case.candidate.job.artifact_root else { break };
                        let bound_to = kaspa_consensus_core::palw_shard_court_v1::PalwOneMoveClaimV2 {
                            execution_root: case.candidate.job.execution_root,
                            class_id: case.candidate.job.class_id,
                            artifact_root,
                        };
                        let (frontiers, witness, producer, trace_root) =
                            (case.frontiers.clone(), case.witness.clone(), case.candidate.producer, case.candidate.job.trace_root);
                        let fallback = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
                            &self.config.court,
                            params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
                        );
                        let ladder = self.seat_refutation_ladder_v1(bound_to.class_id, fallback, current_daa);
                        let terminal = tokio::task::spawn_blocking(move || {
                            replica.terminal(
                                &binding,
                                &frontiers,
                                witness.as_deref(),
                                leaf,
                                claim,
                                &bound_to,
                                trace_root,
                                producer,
                                bond_key,
                                ladder,
                            )
                        })
                        .await
                        .unwrap_or_else(|e| Err(format!("the public terminal task did not finish: {e}")));
                        self.fraud_filer_queue_v1(book, claim, item, current_daa, court_pending, court_due, || {
                            palw_fraud_filer_sign_terminal_v2(
                                terminal?,
                                &network_domain,
                                self.config.court.max_close_bytes(),
                                |m, context| self.sign(m, context),
                            )
                        });
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(phase)) => {
                        match phase {
                            PalwFilerPhaseV1::Convicted | PalwFilerPhaseV1::DaDefault => {
                                info!("[{PALW_PANEL}] claim {claim}: the pursuit ended — {phase:?} (LG14-A)")
                            }
                            _ => warn!(
                                "[{PALW_PANEL}] claim {claim}: the pursuit ended without an objective outcome — {phase:?} (LG14-A)"
                            ),
                        }
                        book.settle(&claim, PalwFraudFilerVerdictV1::Settled(phase));
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::HeldRoute { leaf }) => {
                        warn!(
                            "[{PALW_PANEL}] claim {claim}: the first divergent leaf {leaf} is a fused-attention site — its terminal is the \
                             held dissection's (LG14-B), not a unit; the reservation keeps the claim held until its hard deadline"
                        );
                        book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(format!("fused leaf {leaf}: the held dissection's")));
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Reserve) => {
                        let Some(view) = view.as_ref() else { break };
                        self.fraud_filer_queue_v1(
                            book,
                            claim,
                            PalwFraudFilerSentV1::Reserve,
                            current_daa,
                            court_pending,
                            court_due,
                            || {
                                let mut signed = true;
                                let object = palw_dispute_reserved_object_v1(
                                    network_domain,
                                    palw_fraud_filer_reservation_v1(view, bond_key),
                                    |m| {
                                        self.sign(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).unwrap_or_else(|| {
                                            signed = false;
                                            Vec::new()
                                        })
                                    },
                                );
                                if signed { Ok(object) } else { Err("no signing key for this bond".to_string()) }
                            },
                        );
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe)) => {
                        let Some(view) = view.as_ref() else { break };
                        let binding = case.binding.clone();
                        if let PalwLegacyProbeV1::HeldNode { unit } = probe {
                            book.want_legacy(claim, unit);
                        }
                        self.fraud_filer_queue_v1(
                            book,
                            claim,
                            PalwFraudFilerSentV1::Demand(probe),
                            current_daa,
                            court_pending,
                            court_due,
                            || {
                                palw_fraud_filer_demand_object_v1(
                                    &network_domain,
                                    claim,
                                    &view.execution_root,
                                    probe,
                                    binding.as_ref(),
                                    bond_key,
                                    form,
                                    |message, context| self.sign(message, context),
                                )
                            },
                        );
                        break;
                    }
                }
            }
        }
    }

    /// **Queue one item of a case** under the filer's own key, dated now — once per [`PALW_FRAUD_FILER_RESEND_DAA_V1`] and at most
    /// [`PALW_FRAUD_FILER_SENDS_V1`] times; an item whose sends are spent, or that cannot be built, ends this node's pursuit loudly.
    #[allow(clippy::too_many_arguments)]
    fn fraud_filer_queue_v1(
        &self,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        item: PalwFraudFilerSentV1,
        current_daa: u64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        build: impl FnOnce() -> Result<PalwConsensusObjectV2, String>,
    ) {
        let Some(case) = book.cases.get_mut(&claim) else { return };
        match case.may_send(item, current_daa) {
            Ok(false) => return,
            Ok(true) => {}
            Err(()) => {
                warn!(
                    "[{PALW_PANEL}] claim {claim}: the fraud filer's {item:?} was sent {PALW_FRAUD_FILER_SENDS_V1} times and never landed (LG14-A)"
                );
                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(format!("{item:?} never landed")));
                return;
            }
        }
        match build() {
            Ok(object) => {
                let round = match item {
                    PalwFraudFilerSentV1::Reserve => PALW_FRAUD_FILER_RESERVE_ROUND_V1,
                    PalwFraudFilerSentV1::Demand(_) => PALW_FRAUD_FILER_DEMAND_ROUND_V1,
                    PalwFraudFilerSentV1::Terminal { .. } => PALW_FRAUD_FILER_TERMINAL_ROUND_V2,
                };
                info!("[{PALW_PANEL}] claim {claim}: the fraud filer queues {item:?} (LG14-A, RFC-0014 §6–§7)");
                court_due.insert((claim, round, false), current_daa);
                court_pending.push((claim, round, false, object));
                case.note_sent(item, current_daa);
            }
            Err(why) => {
                warn!("[{PALW_PANEL}] claim {claim}: the fraud filer cannot build {item:?}: {why} (LG14-A)");
                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
            }
        }
    }

    /// **Start this node's replay of `claim`'s job** off the loop — the anchor's job derived from the claim's block
    /// (`attempt_job_for_claim`), on the class's own backend, under the host ledger's reservation at the full seat's need (lane B's
    /// replay, kept whole: the capture is what this node's own ranges are opened from). A claim this host cannot replay is unjudged.
    fn fraud_filer_start_replay_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        network_domain: Hash64,
    ) {
        let Some(case) = book.cases.get(&claim) else { return };
        let job = case.candidate.job.clone();
        let producer = case.candidate.producer;
        let unjudged = |book: &mut PalwFraudFilerBookV1, why: String| {
            debug!("[{PALW_PANEL}] claim {claim}: the fraud filer does not judge it — {why}");
            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
        };
        if job.free_prompt {
            return unjudged(book, "a free prompt's job is its caller's, served, never derived".into());
        }
        let Some(artifact_root) = job.artifact_root else { return unjudged(book, "its class is gone from the registry".into()) };
        let mut backend = match self.resolve_backend(session, job.class_id, artifact_root) {
            Ok(backend) => backend,
            Err(why) => return unjudged(book, format!("its class does not resolve on this host ({why})")),
        };
        let Some((ctx, prompt)) =
            self.attempt_job_for_claim(session, backend.as_ref(), network_domain, job.accepted_block, job.class_id, &producer)
        else {
            return unjudged(book, "its block's header is not held here".into());
        };
        let backends = self.backends();
        let reserved =
            match self.reserve_replay_at_widest_run_v1(PALW_FRAUD_FILER_REPLAY_ROLE_V1, backend.as_mut(), job.class_id, claim, |b| {
                backends.role_memory_need_for_backend_or_chain_v1(
                    b,
                    job.class_id,
                    artifact_root,
                    Some(&ctx),
                    super::FULL_SEAT_V1,
                    |id| self.chain_carriage_v1(session, id),
                )
            }) {
                Ok((reserved, _)) => reserved,
                Err(why) => {
                    crate::palw_backends::note_throttled_v1("panel-fraud-filer-ledger", || {
                        format!("[{PALW_PANEL}] claim {claim}: the fraud filer's replay waits: {why} (LG14-A)")
                    });
                    return;
                }
            };
        let Ok(prompt32) = prompt.iter().map(|id| u32::try_from(*id)).collect::<Result<Vec<u32>, _>>() else {
            return unjudged(book, "a prompt id past u32".into());
        };
        info!("[{PALW_PANEL}] claim {claim}: the fraud filer replays the claim's job off the loop (LG14-A, RFC-0014 §6.1)");
        let attempt_draw = self.attempt_draw_for_claim(session, job.accepted_block);
        let prompt_form = self.class_prompt_ids_form(job.class_id);
        let handle = tokio::task::spawn_blocking(move || {
            let outcome = backend.execute(&ctx, &prompt)?;
            let backend: Arc<dyn PalwExecutionBackendV1> = Arc::from(backend);
            let material = Arc::new(outcome.material);
            let prompt32 = Arc::new(prompt32);
            let own_backend = backend.clone();
            let own_material = material.clone();
            let own_prompt = prompt32.clone();
            let legacy = misaka_palw_base0::produce::base0_material_decode_any_v1(&material).is_ok().then(|| {
                Arc::new(PalwLegacyReplicaV2 {
                    backend,
                    capture: material,
                    prompt_ids: prompt32,
                    form: prompt_form,
                    roots: kaspa_consensus_core::palw_backend::PalwClaimRootsV1 {
                        execution_root: outcome.execution_root,
                        trace_root: outcome.trace_root,
                        anchor: ctx.job_id,
                        attempt_draw,
                        output_root: None,
                        job_pin: None,
                    },
                })
            });
            Ok(PalwFraudFilerRunV1 {
                execution_root: outcome.execution_root,
                trace_root: outcome.trace_root,
                output_root: outcome.output_root,
                own_range: Box::new(move |first, count| {
                    own_backend.held_step_range_answer_v1(&own_material, &own_prompt, first, count).map(|opening| opening.leaf_hashes)
                }),
                legacy,
                _reservation: Some(reserved),
            })
        });
        book.running = Some((claim, handle));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_legacy_public_filer_v1::{PalwDisputeClaimV1, PalwDisputeReservationRowV1};
    use kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1;
    use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xF1), n as u32))
    }

    fn candidate(claim: u64, accepted_daa: u64, seat: bool) -> PalwFraudFilerCandidateV1 {
        PalwFraudFilerCandidateV1 {
            claim_id: Hash64::from_u64_word(claim),
            producer: bond(99),
            accepted_daa,
            seat,
            job: PalwOperatorDaJobV1 {
                accepted_block: Hash64::from_u64_word(claim + 1_000),
                class_id: Hash64::from_u64_word(7),
                artifact_root: Some(Hash64::from_u64_word(8)),
                execution_root: Hash64::from_u64_word(0xE0 + claim),
                trace_root: Hash64::from_u64_word(0x70 + claim),
                output_root: Hash64::default(),
                work_leaves: 0,
                free_prompt: false,
                held_to_final: false,
            },
        }
    }

    fn view(claim: u64, phase: PalwClaimPhaseV2) -> PalwLegacyDisputeViewV1 {
        PalwLegacyDisputeViewV1 {
            claim_id: Hash64::from_u64_word(claim),
            class_id: Hash64::from_u64_word(7),
            producer: bond(99),
            phase,
            execution_root: Hash64::from_u64_word(0xE0 + claim),
            trace_root: Hash64::from_u64_word(0x70 + claim),
            accepted_daa: 100,
            trace_retention_daa: 5_000,
            job_identity: Hash64::default(),
            work_leaves: 0,
            deadline_daa: None,
            hard_deadline_daa: 3_800,
            record: None,
            sessions: Vec::new(),
            answered: Vec::new(),
            open_courts: 0,
            executor_refuted: false,
            court_convicted: false,
            da_defaulted: false,
        }
    }

    /// A run whose roots are `execution_root` / `trace_root`, opening no range (the tests never read one).
    fn run(execution_root: Hash64, trace_root: Hash64) -> PalwFraudFilerRunV1 {
        PalwFraudFilerRunV1 {
            execution_root,
            trace_root,
            output_root: Hash64::default(),
            own_range: Box::new(|_, _| Err("no range".into())),
            legacy: None,
            _reservation: None,
        }
    }

    #[test]
    fn trace_only_and_output_only_mismatches_are_pursued_and_only_three_matching_roots_are_honest() {
        let c = candidate(3, 300, false);
        let claim = c.claim_id;
        for (trace_diff, output_diff) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut book = PalwFraudFilerBookV1::default();
            book.refresh(vec![c.clone()], 10);
            let mut replay = run(c.job.execution_root, c.job.trace_root);
            if trace_diff {
                replay.trace_root = Hash64::from_u64_word(88);
            }
            if output_diff {
                replay.output_root = Hash64::from_u64_word(99);
            }
            let verdict = book.judge(claim, Ok(replay)).expect("held");
            if trace_diff || output_diff {
                assert!(matches!(verdict, PalwFraudFilerVerdictV1::Mismatch(_)));
                assert_eq!(book.walk_floor(), Some(300), "public binding backfill remains required");
            } else {
                assert!(matches!(verdict, PalwFraudFilerVerdictV1::Honest));
                assert_eq!(book.walk_floor(), None);
            }
        }
    }

    #[test]
    fn a_partial_history_page_cannot_end_a_pursuit_before_older_direct_evidence_is_read() {
        let c = candidate(3, 300, false);
        let claim = c.claim_id;
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c], 10);
        book.judge(claim, Ok(run(Hash64::default(), Hash64::default())));
        assert!(!book.descent_failed(&claim, "no successful history page yet".into()));
        let page = PalwFraudFilerWalkV1 {
            floor: 300,
            anchor: Hash64::from_u64_word(1000),
            anchor_daa: 1000,
            next: Some(Hash64::from_u64_word(500)),
        };
        book.walked(page, false, vec![]);
        assert!(!book.descent_failed(&claim, "a pin is not read yet".into()));
        assert_eq!(book.held_runs(), 1);
        assert_eq!(book.walk_floor(), Some(300));
        book.walked(PalwFraudFilerWalkV1 { next: None, ..page }, false, vec![]);
        assert!(book.descent_failed(&claim, "complete history cannot supply a proof".into()));
        assert_eq!(book.held_runs(), 0);
        assert!(matches!(book.cases[&claim].verdict, PalwFraudFilerVerdictV1::Unjudged(_)));
    }

    /// **The book replays oldest first, one at a time, never a seat's claim, and holds at most two mismatches.**
    #[test]
    fn the_book_replays_oldest_first_and_bounds_what_it_holds() {
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![candidate(3, 300, false), candidate(1, 100, true), candidate(2, 200, false), candidate(4, 400, false)], 10);
        assert_eq!(book.next_replay(), Some(Hash64::from_u64_word(2)), "the seat's claim is the seat's duties'");
        let honest = candidate(2, 200, false).job;
        book.judge(Hash64::from_u64_word(2), Ok(run(honest.execution_root, honest.trace_root)));
        assert!(matches!(book.cases[&Hash64::from_u64_word(2)].verdict, PalwFraudFilerVerdictV1::Honest));
        assert_eq!(book.next_replay(), Some(Hash64::from_u64_word(3)));
        book.judge(Hash64::from_u64_word(3), Ok(run(Hash64::from_u64_word(1), honest.trace_root)));
        book.judge(Hash64::from_u64_word(4), Ok(run(Hash64::from_u64_word(1), honest.trace_root)));
        assert_eq!(book.held_runs(), 2);
        book.refresh(vec![candidate(5, 50, false)], 11);
        assert_eq!(book.next_replay(), None, "two mismatches held: the next waits");
        assert_eq!(book.pursued(), vec![Hash64::from_u64_word(3), Hash64::from_u64_word(4)], "pursued cases survive the candidates");
        assert!(!book.cases.contains_key(&Hash64::from_u64_word(2)), "a settled case leaves with its candidate");
        // A failure on this host is run once more, then unjudged.
        book.settle(&Hash64::from_u64_word(3), PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::Convicted));
        let five = Hash64::from_u64_word(5);
        assert_eq!(book.next_replay(), Some(five));
        book.judge(five, Err("refused".into()));
        assert!(matches!(book.cases[&five].verdict, PalwFraudFilerVerdictV1::Pending));
        book.judge(five, Err("refused".into()));
        assert!(matches!(book.cases[&five].verdict, PalwFraudFilerVerdictV1::Unjudged(_)));
    }

    /// **Shared progress**: a probe the chain already answered is read (once the walk has the answer), never demanded; a reservation
    /// comes first; the engine's terminal is demanded even when answered (the fold adjudicates it).
    #[test]
    fn a_unit_the_chain_answered_is_read_never_demanded() {
        let me = bond(1);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![candidate(3, 300, false)], 10);
        let claim = Hash64::from_u64_word(3);
        book.judge(claim, Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        let case = &book.cases[&claim];
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 150 });
        let step = |v: &PalwLegacyDisputeViewV1, reservable, read: bool| {
            palw_fraud_filer_step_v1(PalwFilerRoleV1::PublicBond, Some(v), &me, reservable, case, |_| read)
        };
        assert_eq!(step(&v, true, false), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Reserve));
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 151, deposit: 1, sessions_opened: 0 });
        v.record = Some(record);
        let binding = PalwLegacyProbeV1::Binding { row: 0, tile: 0 };
        assert_eq!(step(&v, false, false), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(binding)));
        v.answered.push(binding.unit());
        assert_eq!(step(&v, false, false), PalwFraudFilerStepV1::AwaitAnswer(binding));
        assert_eq!(step(&v, false, true), PalwFraudFilerStepV1::Learn(binding));
        // The outcome ends it whatever else stands.
        v.phase =
            PalwClaimPhaseV2::Voided { voided_daa: 200, reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::CourtFraud };
        assert_eq!(step(&v, false, true), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Convicted)));
        assert_eq!(
            palw_fraud_filer_step_v1(PalwFilerRoleV1::PublicBond, None, &me, false, case, |_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Expired)),
            "a retired claim's pursuit expires"
        );
    }

    /// **An item is sent once, re-sent once after it was lost, and never a third time**; a different item is never held back.
    #[test]
    fn an_item_is_sent_at_most_twice() {
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        let reserve = PalwFraudFilerSentV1::Reserve;
        assert_eq!(case.may_send(reserve, 10), Ok(true));
        case.note_sent(reserve, 10);
        assert_eq!(case.may_send(reserve, 10 + PALW_FRAUD_FILER_RESEND_DAA_V1 - 1), Ok(false));
        assert_eq!(case.may_send(reserve, 10 + PALW_FRAUD_FILER_RESEND_DAA_V1), Ok(true));
        case.note_sent(reserve, 10 + PALW_FRAUD_FILER_RESEND_DAA_V1);
        assert_eq!(case.may_send(reserve, 10 + 3 * PALW_FRAUD_FILER_RESEND_DAA_V1), Err(()));
        assert_eq!(case.may_send(PalwFraudFilerSentV1::Demand(PalwLegacyProbeV1::Binding { row: 0, tile: 0 }), 11), Ok(true));
    }

    /// **The walk reads from the oldest pursued claim after a start, then only what the tip added (less a reorg margin).**
    #[test]
    fn the_walk_starts_at_the_oldest_pursuit_then_follows_the_tip() {
        let mut book = PalwFraudFilerBookV1::default();
        assert_eq!(book.walk_floor(), None, "nothing pursued: no walk");
        book.refresh(vec![candidate(3, 300, false), candidate(4, 400, false)], 10);
        book.judge(Hash64::from_u64_word(4), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        assert_eq!(book.walk_floor(), Some(400));
        book.walked(
            PalwFraudFilerWalkV1 { floor: 400, anchor: Hash64::from_u64_word(1_000), anchor_daa: 1_000, next: None },
            false,
            Vec::new(),
        );
        assert_eq!(book.walk_floor(), Some(1_000 - PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1));
        book.judge(Hash64::from_u64_word(3), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        assert_eq!(book.walk_floor(), Some(300), "an older pursuit walks back to its acceptance");
    }

    #[test]
    fn partial_pages_and_newer_pursuits_do_not_inherit_a_completed_watermark() {
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![candidate(3, 300, false), candidate(4, 400, false)], 10);
        book.judge(Hash64::from_u64_word(3), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        let anchor = Hash64::from_u64_word(1_000);
        let partial = PalwFraudFilerWalkV1 { floor: 300, anchor, anchor_daa: 1_000, next: Some(Hash64::from_u64_word(800)) };
        book.walked(partial, false, Vec::new());
        assert_eq!((book.walked_from, book.walked_to), (None, None));
        assert_eq!(book.walk_floor(), Some(300));
        book.walked(PalwFraudFilerWalkV1 { next: None, ..partial }, false, Vec::new());
        assert_eq!(book.walk_floor(), Some(1_000 - PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1));
        // The newer claim's responses were not collected while its replay was pending, although the walk crossed its acceptance.
        book.judge(Hash64::from_u64_word(4), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        assert_eq!(book.walk_floor(), Some(300), "a new pursuit forces backfill even when its acceptance is above the old floor");
        assert_eq!((book.walked_from, book.walked_to, book.walk), (None, None, None));
        assert!(book.stale(10), "start backfill on the next tick even if DAA has not advanced");
    }

    #[test]
    fn selecting_an_already_answered_legacy_unit_backfills_instead_of_inheriting_a_recent_watermark() {
        let mut book = PalwFraudFilerBookV1::default();
        let claim = Hash64::from_u64_word(3);
        book.refresh(vec![candidate(3, 300, false)], 10);
        book.judge(claim, Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        let complete = PalwFraudFilerWalkV1 { floor: 300, anchor: Hash64::from_u64_word(1_000), anchor_daa: 1_000, next: None };
        book.walked(complete, false, Vec::new());
        assert_eq!(book.walk_floor(), Some(936));
        let root = PalwLegacyHeldUnitV2::StepNode { level: 16, index: 0 };
        book.want_legacy(claim, root);
        assert_eq!(book.walk_floor(), Some(300));
        assert!(book.stale(10), "the selected unit may predate the last walk while it was unselected");
        book.walked(complete, false, Vec::new());
        book.want_legacy(claim, root);
        assert_eq!(book.walk_floor(), Some(936), "waiting for the same unit does not continually restart a partial read");
        book.want_legacy(claim, PalwLegacyHeldUnitV2::StepNode { level: 8, index: 1 });
        assert_eq!(book.walk_floor(), Some(300), "the next selected node also needs its older bytes");
        book.settle(&claim, PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::Convicted));
        assert_eq!(book.cases[&claim].legacy_wanted, None);
    }

    #[test]
    fn the_history_cache_cannot_accumulate_unused_range_units() {
        use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
        let range = |first, count| PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first, count });
        assert!(palw_fraud_filer_cache_unit_v1(&PalwDaUnitV1::Event { row: 0, tile: 0 }, None));
        assert!(!palw_fraud_filer_cache_unit_v1(&PalwDaUnitV1::Event { row: 1, tile: 0 }, None));
        let leaves = 2_049;
        for first in 0..leaves {
            for count in [0, 1, 1_024] {
                let expected = matches!((first, count), (0 | 1_024, 1_024) | (2_048, 1));
                assert_eq!(palw_fraud_filer_cache_unit_v1(&range(first, count), Some(leaves)), expected);
            }
        }
        assert!(palw_fraud_filer_cache_unit_v1(&range(31 * 1_024, 1_024), Some(32 * 1_024)));
        assert!(!palw_fraud_filer_cache_unit_v1(&range(0, 1_024), Some(32 * 1_024 + 1)), "an over-cap ladder is not this fallback's");
        assert!(!palw_fraud_filer_cache_unit_v1(&range(0, 1_024), None));
    }

    /// **The tick wires the filer behind no flag, after lane B, with the seat's replay room** (a source pin).
    #[test]
    fn the_tick_wires_the_filer_behind_no_flag() {
        let panel = include_str!("palw_panel.rs");
        let tick = panel.find("self.fraud_filer_tick_v1(").expect("the tick calls the filer");
        let operator = panel.find("self.operator_da_tick_v1(").expect("lane B");
        assert!(tick > operator, "after lane B");
        assert!(panel[tick..tick + 400].contains("seat_replays.has_room(false)"), "the replay yields to the seat's slots");
    }
}
