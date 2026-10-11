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
//!    descending to the first divergent leaf. Every other case (below that fence, or any codec) runs lane G14-V's authenticated
//!    `StepRange` subtree descent (`kaspa_consensus_core::palw_auth_descent_v1`): only subtree roots the range answers carried, each
//!    authenticated to the claim's step root, are compared with the verifier's own tree — never a lone midpoint leaf — so a lie at
//!    any position is located in at most `h − 9` reads (`h` the tree height; 33 reserved sessions at 2^40 leaves);
//! 4. **terminal** — the LG14-B replica builds tag 159 from its own registered model and public leaf hashes, or uses a public CKW
//!    to open a fused held court. The subtree descent demands `StepLeaf`. Silence on a required unit defaults (DA-7).
//!    Public binding/job/shape/output faults go through the reporter door as kind 4 before descent. **Past an agreeing step tree**
//!    (G14-V) the logits trace is localized from the binding read's event (flat: no further session; tiled: an authenticated
//!    rows/tiles descent of `Event` reads) and `LogitsNotStepOutput` / `ForgedOutput` filed through the same door; step-tree
//!    agreement never clears a mismatched execution. A checkpoint-leg-only difference is reported, not guessed (open).
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
#[path = "palw_fraud_filer_input.rs"]
pub(super) mod input;
#[path = "palw_fraud_filer_constraint.rs"]
mod constraint;
pub(super) use input::{palw_fraud_filer_fp_input_v1, palw_fraud_filer_fp_metadata_v1, palw_fraud_filer_fp_payload_v1};

use super::reporter_filer::{
    PalwConvictionDoorV1, PalwConvictionFilingV1, PalwFileOutcomeV1, PalwFilingOriginV1, PalwReporterFilerV1,
};
use crate::palw_legacy_held_v2::PalwLegacyReplicaV2;
use kaspa_consensus_core::palw_auth_descent_v1::{
    PalwAuthDescentV1, PalwAuthTreeV1, PalwOwnLeafTreeV1, PalwTraceDescentV1, PalwTraceLocalizeV1, palw_auth_descent_next_v1,
};
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1, palw_da_step_leaf_is_fused_v1};
use kaspa_consensus_core::palw_legacy_held_da_v2::{
    PalwCommittedKernelWitnessV2, PalwLegacyDescentStepV2, PalwLegacyFrontierV2, PalwLegacyHeldAnswerV2, PalwLegacyHeldUnitV2,
    palw_legacy_held_check_answer_v2, palw_legacy_leaf_is_fused_v2,
};
use kaspa_consensus_core::palw_legacy_public_filer_v1::{
    PALW_DISPUTE_RESERVATION_VERSION_V1, PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1,
    PalwFilerActionV1, PalwFilerPhaseV1, PalwFilerRoleV1, PalwLegacyBisectV1, PalwLegacyProbeV1, palw_dispute_reserved_object_v1,
    palw_fraud_filer_demand_object_v1, palw_fraud_filer_facts_v1, palw_fraud_filer_learn_v1,
    palw_fraud_filer_reservation_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwFraudFilerCandidateV1, PalwLegacyDisputeViewV1};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_core::{debug, error};

/// The court queue's round of this filer's reservation of a claim — a round no other lane keys by (P2-6 `u32::MAX`, the reporter
/// filer `u32::MAX − 3 ..= u32::MAX − 1`, P2-8d `− 6`, lane B `− 7`, the audit duty `− 8`, the rider outbox `− 9`).
pub(super) const PALW_FRAUD_FILER_RESERVE_ROUND_V1: u32 = u32::MAX - 10;
/// The court queue's round of this filer's demand on a claim (one open at a time: DA-1's one session per accuser).
pub(super) const PALW_FRAUD_FILER_DEMAND_ROUND_V1: u32 = u32::MAX - 11;
pub(super) const PALW_FRAUD_FILER_TERMINAL_ROUND_V2: u32 = u32::MAX - 12;
/// An input bootstrap that finds an honest claim relinquishes its held reservation.
pub(super) const PALW_FRAUD_FILER_RELEASE_ROUND_V1: u32 = u32::MAX - 13;
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

pub(super) use input::PalwFraudFilerFpBootstrapV1;
#[cfg(test)]
pub(super) use input::PalwFraudFilerFpDescriptorV1;

/// Whether a court-queue entry is this filer's.
pub(super) fn palw_fraud_filer_queued_v1(round: u32, responder: bool) -> bool {
    !responder
        && (round == PALW_FRAUD_FILER_RESERVE_ROUND_V1
            || round == PALW_FRAUD_FILER_DEMAND_ROUND_V1
            || round == PALW_FRAUD_FILER_TERMINAL_ROUND_V2
            || round == PALW_FRAUD_FILER_RELEASE_ROUND_V1)
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
    /// Lane G14-V: the own run's material for the authenticated subtree descent and the localizations past the step tree.
    pub(super) own: Option<Arc<PalwFraudFilerOwnRunV1>>,
    /// Authenticated public FP descriptor retained with a mismatched replay until prosecution.
    pub(super) fp_job: Option<Box<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>>,
    pub(super) _job_reservation: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
    pub(super) _reservation: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
}

/// **Lane G14-V: the verifier's own run, as the localizers past the binding read it** — its backend, its capture and the job's
/// prompt (its own material, never the producer's). Its own step tree is built once per run, retained at
/// [`kaspa_consensus_core::palw_auth_descent_v1::PalwOwnLeafTreeV1::retain_level_v1`] and folded below it on demand.
pub(super) struct PalwFraudFilerOwnRunV1 {
    pub(super) backend: Arc<dyn PalwExecutionBackendV1>,
    pub(super) material: Arc<Vec<u8>>,
    pub(super) prompt: Arc<Vec<u32>>,
    pub(super) tree: std::sync::OnceLock<Result<Arc<PalwOwnLeafTreeV1>, String>>,
}

impl PalwFraudFilerOwnRunV1 {
    pub(super) fn new(backend: Arc<dyn PalwExecutionBackendV1>, material: Arc<Vec<u8>>, prompt: Arc<Vec<u32>>) -> Self {
        Self { backend, material, prompt, tree: std::sync::OnceLock::new() }
    }

    /// The own run's committed event `(row, tile)` (its DA responder's disclosure of its own capture).
    pub(super) fn event(
        &self,
        row: u32,
        tile: u8,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1, String> {
        self.backend.disclose_trace_event(&self.material, row, tile)
    }

    /// The own run's binding (an out-of-range event discloses only the binding).
    pub(super) fn binding(&self) -> Result<PalwStepBindingV2, String> {
        self.event(u32::MAX, u8::MAX).map(|d| d.binding().clone())
    }

    /// The own run's leaf hashes of `[first, first + count)`.
    pub(super) fn range(&self, first: u64, count: u32) -> Result<Vec<Hash64>, String> {
        self.backend.held_step_range_answer_v1(&self.material, &self.prompt, first, count).map(|opening| opening.leaf_hashes)
    }

    /// The own run's opening of step leaf `leaf` (authenticates against the claim's step root only where the two trees agree).
    pub(super) fn opening(&self, leaf: u64) -> Result<kaspa_consensus_core::palw_step_leg::PalwStepOpeningV1, String> {
        let range = self.backend.held_step_range_answer_v1(&self.material, &self.prompt, leaf, 1)?;
        let leaf_hash = *range.leaf_hashes.first().ok_or("an empty own range")?;
        Ok(kaspa_consensus_core::palw_step_leg::PalwStepOpeningV1 { leaf_index: leaf, leaf_hash, siblings: range.siblings })
    }

    /// The own step tree over `leaves` leaves, built once.
    pub(super) fn tree(&self, leaves: u64) -> Result<Arc<PalwOwnLeafTreeV1>, String> {
        let tree = self
            .tree
            .get_or_init(|| {
                PalwOwnLeafTreeV1::build(leaves, PalwOwnLeafTreeV1::retain_level_v1(leaves), &|first, count| self.range(first, count))
                    .map(Arc::new)
            })
            .clone()?;
        if tree.leaf_count != leaves {
            return Err("the claim's step count is not this public job's: the job/count terminal is required".into());
        }
        Ok(tree)
    }
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
    Release,
    Reacquire { probe: PalwLegacyProbeV1, session_number: u8 },
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

/// Only the binding read and the ONE unit a case's localizer currently reads enter the cache (lane G14-V: the subtree descent's
/// range or the trace descent's event — any unit the chain answered for anyone, never every range a stranger demanded). Each is
/// learned into the case's authenticated tree and dropped, so the cache holds one binding and at most one unit per pursued claim.
/// LG14-B separately holds only the currently selected authenticated node/witness answer.
fn palw_fraud_filer_cache_unit_v1(unit: &PalwDaUnitV1, wanted: Option<&PalwDaUnitV1>) -> bool {
    matches!(*unit, PalwDaUnitV1::Event { row: 0, tile: 0 }) || wanted == Some(unit)
}

/// Read/resume one page. A reorg beyond the old anchor restarts at the current tip and the oldest pursued claim, including when
/// a recent incremental floor would otherwise hide the new branch's old answers. An unavailable old suffix retains authenticated
/// answers from the readable prefix, with an incomplete watermark and a restart at the next tip.
#[cfg(test)]
pub(super) fn palw_fraud_filer_read_page_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    oldest: u64,
    floor: u64,
    previous: Option<PalwFraudFilerWalkV1>,
    max_blocks: usize,
    wanted: &BTreeMap<Hash64, Hash64>,
    legacy_wanted: &BTreeMap<Hash64, PalwLegacyHeldUnitV2>,
) -> Result<(PalwFraudFilerWalkV1, bool, Vec<((Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1)>), String> {
    palw_fraud_filer_read_page_with_form_v1(
        consensus,
        oldest,
        floor,
        previous,
        max_blocks,
        wanted,
        legacy_wanted,
        &BTreeMap::new(),
        &BTreeMap::new(),
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn palw_fraud_filer_read_page_with_form_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    oldest: u64,
    floor: u64,
    previous: Option<PalwFraudFilerWalkV1>,
    max_blocks: usize,
    wanted: &BTreeMap<Hash64, Hash64>,
    legacy_wanted: &BTreeMap<Hash64, PalwLegacyHeldUnitV2>,
    legacy_rounds: &BTreeMap<Hash64, BTreeSet<PalwLegacyHeldUnitV2>>,
    rcore_wanted: &BTreeMap<Hash64, PalwDaUnitV1>,
    network: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<(PalwFraudFilerWalkV1, bool, Vec<((Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1)>), String> {
    palw_fraud_filer_read_page_with_budget_v2(
        consensus,
        oldest,
        floor,
        previous,
        max_blocks,
        wanted,
        legacy_wanted,
        legacy_rounds,
        rcore_wanted,
        network,
        kaspa_consensus_core::palw_legacy_answer_chunks_v2::PALW_LEGACY_ANSWER_MAX_BYTES_V2,
    )
}

#[allow(clippy::too_many_arguments)]
fn palw_fraud_filer_read_page_with_budget_v2(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    oldest: u64,
    floor: u64,
    previous: Option<PalwFraudFilerWalkV1>,
    max_blocks: usize,
    wanted: &BTreeMap<Hash64, Hash64>,
    legacy_wanted: &BTreeMap<Hash64, PalwLegacyHeldUnitV2>,
    legacy_rounds: &BTreeMap<Hash64, BTreeSet<PalwLegacyHeldUnitV2>>,
    rcore_wanted: &BTreeMap<Hash64, PalwDaUnitV1>,
    network: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    maximum: u64,
) -> Result<(PalwFraudFilerWalkV1, bool, Vec<((Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1)>), String> {
    let tip = consensus.get_sink();
    let same_branch = previous.is_none_or(|p| p.anchor == tip || consensus.is_chain_ancestor_of(p.anchor, tip).unwrap_or(false));
    let reset = !same_branch;
    let continuing = previous.filter(|p| same_branch && p.next.is_some() && p.next != Some(p.anchor));
    let floor = continuing.map_or(if reset { oldest } else { floor }, |p| p.floor);
    let anchor = continuing.map_or(tip, |p| p.anchor);
    let anchor_daa = continuing.map_or_else(
        || consensus.get_header(anchor).map(|h| h.daa_score).map_err(|e| format!("history anchor {anchor}: {e}")),
        |p| Ok(p.anchor_daa),
    )?;
    let start = continuing.and_then(|p| p.next).unwrap_or(tip);
    let mut answers = BTreeMap::new();
    let mut unavailable = false;
    let next =
        crate::palw_panel::walk_accepted_lifecycle_page_with_block_v2(consensus, start, floor, max_blocks, &mut |block, object| {
            match object {
                PalwConsensusObjectV2::MaterialDisclosedV2 { claim, unit, answer, .. }
                    if wanted.get(&claim).is_some_and(|root| {
                        kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
                            &unit, &answer, root,
                        )
                    }) && palw_fraud_filer_cache_unit_v1(&unit, rcore_wanted.get(&claim)) =>
                {
                    palw_fraud_filer_cache_answer_v1(&mut answers, (claim, unit), PalwDaBuiltAnswerV1::Rcore(answer));
                }
                PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
                    if (legacy_wanted.get(&answer.claim) == Some(&answer.unit)
                        || legacy_rounds.get(&answer.claim).is_some_and(|round| round.contains(&answer.unit)))
                        && wanted.get(&answer.claim).is_some_and(|root| {
                            kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v3(
                                root,
                                &answer.unit,
                                &answer.binding,
                                &answer.answer,
                                answer.binding.step_leaf_count,
                                network,
                            )
                            .is_ok()
                        }) =>
                {
                    answers
                        .entry((answer.claim, PalwDaUnitV1::LegacyHeldV2(answer.unit)))
                        .or_insert(PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((answer.binding, answer.answer))));
                }
                PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk }
                    if (legacy_wanted.get(&chunk.claim) == Some(&chunk.unit)
                        || legacy_rounds.get(&chunk.claim).is_some_and(|round| round.contains(&chunk.unit)))
                        && wanted.contains_key(&chunk.claim)
                        && !answers.contains_key(&(chunk.claim, PalwDaUnitV1::LegacyHeldV2(chunk.unit))) =>
                {
                    match consensus.palw_legacy_answer_history_v2(block, chunk, maximum) {
                        Ok(Some(answer))
                            if wanted.get(&answer.claim).is_some_and(|root| {
                                kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v3(
                                    root,
                                    &answer.unit,
                                    &answer.binding,
                                    &answer.answer,
                                    answer.binding.step_leaf_count,
                                    network,
                                )
                                .is_ok()
                            }) =>
                        {
                            answers
                                .entry((answer.claim, PalwDaUnitV1::LegacyHeldV2(answer.unit)))
                                .or_insert(PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((answer.binding, answer.answer))));
                        }
                        Err(why) => {
                            debug!("[{PALW_PANEL}] multipart public history at {block}: {why}; completion watermark unchanged");
                            unavailable = true;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        })
        .unwrap_or_else(|why| {
            debug!("[{PALW_PANEL}] partial public history: {why}; completion watermark unchanged");
            Some(anchor)
        });
    let next = if unavailable { Some(anchor) } else { next };
    // `next == anchor` is the unavailable-history sentinel: retain independently authenticated answers from the readable
    // prefix, never mark the floor completed, and restart at the NEW tip on the next tick so a reacquired copy is reachable.
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
    /// Lane G14-V: the claim's step tree as far as authenticated range answers reached it (the subtree descent's state).
    pub(super) step_tree: Option<PalwAuthTreeV1>,
    /// Lane G14-V: the authenticated tiled events a trace localization read past an agreeing step tree.
    pub(super) trace: PalwTraceDescentV1,
    /// Lane G14-V: the one range or event unit this case's localizer reads now (the history reader caches only it).
    pub(super) rcore_wanted: Option<PalwDaUnitV1>,
    pub(super) frontiers: Vec<PalwLegacyFrontierV2>,
    pub(super) witness: Option<Box<PalwCommittedKernelWitnessV2>>,
    pub(super) legacy_wanted: Option<PalwLegacyHeldUnitV2>,
    /// Lane G14-L: the current k-ary round's units (the lanes of one reserved session) — the history reader caches their answers.
    pub(super) legacy_round: Vec<PalwLegacyHeldUnitV2>,
    /// Lane G14-L: when each lane of the current round was last queued, and how many times (a lost lane is resent after
    /// [`PALW_FRAUD_FILER_RESEND_DAA_V1`], at most [`PALW_FRAUD_FILER_SENDS_V1`] times).
    pub(super) lanes_sent: BTreeMap<PalwLegacyHeldUnitV2, (u64, u8)>,
    pub(super) runs: u8,
    input_retry_at: u64,
    pub(super) fp_bootstrap: Option<PalwFraudFilerFpBootstrapV1>,
    /// Binding-only pursuit when the original FP job is unavailable; held through a later replay.
    fp_job_pending: bool,
    fp_expected_pin: Option<Hash64>,
    fp_job_disclosure_enabled: bool,
    fp_descriptor: Option<Arc<input::PalwFraudFilerFpDescriptorV1>>,
    _fp_job_reservation: Option<Arc<crate::palw_memory_ledger::PalwMemoryReservationV1>>,
    /// Keep a completed bootstrap in the book until its on-chain reservation is relinquished.
    release_pending: bool,
    awaiting_public: Option<(PalwLegacyProbeV1, u64)>,
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
            step_tree: None,
            trace: PalwTraceDescentV1::default(),
            rcore_wanted: None,
            frontiers: Vec::new(),
            witness: None,
            legacy_wanted: None,
            legacy_round: Vec::new(),
            lanes_sent: BTreeMap::new(),
            runs: 0,
            input_retry_at: 0,
            fp_bootstrap: None,
            fp_job_pending: false,
            fp_expected_pin: None,
            fp_job_disclosure_enabled: false,
            fp_descriptor: None,
            _fp_job_reservation: None,
            release_pending: false,
            awaiting_public: None,
            sent: None,
            direct_handed: 0,
            direct_asked_at: None,
        }
    }

    pub(super) fn pursued(&self) -> bool {
        self.release_pending
            || matches!(self.verdict, PalwFraudFilerVerdictV1::Mismatch(_))
            || (matches!(self.verdict, PalwFraudFilerVerdictV1::Pending) && (self.fp_bootstrap.is_some() || self.fp_job_pending))
    }

    /// Public metadata survives successful bootstrap cleanup through the held replay.
    fn public_fp_job_v1(&self) -> Option<&kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3> {
        self.fp_bootstrap.as_ref().map(|input| input.payload.job())
            .or_else(|| self.fp_descriptor.as_ref().map(|input| input.job()))
            .or_else(|| match &self.verdict {
                PalwFraudFilerVerdictV1::Mismatch(run) => run.fp_job.as_deref(),
                _ => None,
            })
    }

    pub(super) fn learn(&mut self, probe: PalwLegacyProbeV1, answer: &PalwDaBuiltAnswerV1) -> Result<(), String> {
        if self.fp_bootstrap.is_some() || self.fp_job_pending {
            match (probe, answer) {
                (PalwLegacyProbeV1::Binding { row: 0, tile: 0 }, PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event))) => {
                    if !kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
                        &probe.unit(),
                        &PalwDaAnswerV1::Event(event.clone()),
                        &self.candidate.job.execution_root,
                    ) {
                        return Err("bootstrap binding does not authenticate".into());
                    }
                    let binding = event.binding();
                    if binding.full_logits_trace_root != self.candidate.job.trace_root {
                        return Err("bootstrap binding has another trace root".into());
                    }
                    self.bisect = PalwLegacyBisectV1::new(binding.step_leaf_count);
                    self.step_tree = PalwAuthTreeV1::new(binding.step_leaf_count, binding.step_merkle_root);
                    self.binding = Some(binding.clone());
                    return Ok(());
                }
                (PalwLegacyProbeV1::HeldNode { unit }, PalwDaBuiltAnswerV1::LegacyHeldV2(answer)) => {
                    if self.binding.as_ref() != Some(&answer.0) {
                        return Err("bootstrap input carries another binding".into());
                    }
                    if unit == PalwLegacyHeldUnitV2::FpJob {
                        let PalwLegacyHeldAnswerV2::FpJob(job) = &answer.1 else {
                            return Err("job preimage has another answer kind".into());
                        };
                        self.fp_descriptor = Some(Arc::new(input::PalwFraudFilerFpDescriptorV1::from_disclosure(
                            &self.candidate,
                            self.fp_expected_pin.ok_or("no recorded FP job pin")?,
                            &answer.0,
                            job,
                        )?));
                    } else {
                        self.fp_bootstrap
                            .as_mut()
                            .ok_or("bootstrap still has no authenticated FP job/input")?
                            .learn(&answer.0, unit, &answer.1)?;
                    }
                    self.legacy_wanted = None;
                    self.input_retry_at = 0;
                    return Ok(());
                }
                _ => return Err("bootstrap waits for a binding or input unit".into()),
            }
        }
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
                            // Lane G14-L: a k-ary round holds up to 64 node lanes; a descent never runs more rounds than the
                            // one-node descent would (each round covers at least the nine levels one node does).
                            let cap = kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_descent_rounds_v2(binding.step_leaf_count)
                                as usize
                                * kaspa_consensus_core::palw_legacy_parallel_v1::PALW_LEGACY_LANES_PER_SESSION_V1 as usize;
                            if self.frontiers.len() >= cap {
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
                match probe {
                    // G14-V: a range is learned into the authenticated subtree descent, never into a linear scan.
                    PalwLegacyProbeV1::Range { .. } => {
                        let binding = self.binding.as_ref().ok_or("a range is learned after the binding")?;
                        let tree = self.step_tree.as_mut().ok_or("no step tree is held for this binding")?;
                        kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_learn_subtree_v1(
                            probe,
                            answer,
                            &self.candidate.job.execution_root,
                            binding,
                            tree,
                        )?;
                        self.rcore_wanted = None;
                        Ok(())
                    }
                    // G14-V: a tiled logits event past the binding, authenticated against the claim's own roots before it is kept.
                    PalwLegacyProbeV1::Binding { row, tile } if (row, tile) != (0, 0) => {
                        let binding = self.binding.as_ref().ok_or("an event is learned after the binding")?;
                        let PalwDaAnswerV1::Event(event) = answer else { return Err("an event is answered by an event".into()) };
                        if event.binding() != binding {
                            return Err("the event carries another binding".into());
                        }
                        self.trace.learn(
                            self.candidate.job.trace_root,
                            self.candidate.job.execution_root,
                            row,
                            tile,
                            event,
                            binding.step_leaf_count,
                        )?;
                        self.rcore_wanted = None;
                        Ok(())
                    }
                    _ => {
                        palw_fraud_filer_learn_v1(
                            probe,
                            answer,
                            &self.candidate.job.execution_root,
                            &mut self.binding,
                            &mut self.bisect,
                            |first, count| (run.own_range)(first, count),
                        )?;
                        if let (PalwLegacyProbeV1::Binding { .. }, Some(binding)) = (probe, self.binding.as_ref()) {
                            self.step_tree = PalwAuthTreeV1::new(binding.step_leaf_count, binding.step_merkle_root);
                        }
                        Ok(())
                    }
                }
            }
            _ => Err("the answer has another carriage than the probe".into()),
        }
    }

    /// Apply the authenticated worker result, including the input accumulator and its shared memory ticket.
    pub(super) fn accept_learned_v1(&mut self, learned: Self) {
        self.awaiting_public = None;
        self.binding = learned.binding;
        self.bisect = learned.bisect;
        self.step_tree = learned.step_tree;
        self.trace = learned.trace;
        self.rcore_wanted = learned.rcore_wanted;
        self.frontiers = learned.frontiers;
        self.witness = learned.witness;
        self.legacy_wanted = learned.legacy_wanted;
        self.legacy_round = learned.legacy_round;
        self.fp_bootstrap = learned.fp_bootstrap;
        self.fp_descriptor = learned.fp_descriptor;
        self.input_retry_at = learned.input_retry_at;
    }

    /// Give the bounded history walk a retry interval, then explicitly reacquire a missing answered unit. A new request
    /// consumes this reservation's next session counter; retries of the same signed request never consume another counter.
    pub(super) fn reacquire_after_v1(
        &mut self,
        view: &PalwLegacyDisputeViewV1,
        me: PalwBondKeyV2,
        probe: PalwLegacyProbeV1,
        now_daa: u64,
    ) -> Option<kaspa_consensus_core::palw_legacy_public_filer_v1::PalwDisputeReacquireV1> {
        if now_daa > view.hard_deadline_daa || view.sessions.iter().any(|(bond, _)| *bond == me) {
            return None;
        }
        let row = view.record.as_ref()?.live.get(&me)?;
        if !view.unit_was_answered_v1(&probe.unit()) {
            return None;
        }
        let since = match self.awaiting_public {
            Some((held, at)) if held == probe => at,
            _ => {
                self.awaiting_public = Some((probe, now_daa));
                return None;
            }
        };
        if now_daa.saturating_sub(since) < PALW_FRAUD_FILER_RESEND_DAA_V1 {
            return None;
        }
        let session_number = row.sessions_opened.checked_add(1)?;
        if session_number > PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1 {
            return None;
        }
        let binding = match probe.unit() {
            PalwDaUnitV1::Event { .. } => None,
            PalwDaUnitV1::Held(_) | PalwDaUnitV1::LegacyHeldV2(_) => Some(self.binding.clone()?),
            _ => return None,
        };
        Some(kaspa_consensus_core::palw_legacy_public_filer_v1::PalwDisputeReacquireV1 {
            version: PALW_DISPUTE_RESERVATION_VERSION_V1,
            claim: self.candidate.claim_id,
            reserver: me,
            reserved_daa: row.reserved_daa,
            session_number,
            valid_until_daa: now_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1).min(view.hard_deadline_daa),
            unit: probe.unit(),
            binding,
        })
    }

    /// May `item` be sent now? A different item always; the same item has a bounded retry interval and count.
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

/// A blocking replay is tied to the exact candidate snapshot that started it. Keep the handle until it terminates,
/// even after invalidation: aborting a started blocking worker cannot stop it or release its memory safely.
struct PalwFraudFilerReplayV1 {
    candidate: PalwFraudFilerCandidateV1,
    handle: tokio::task::JoinHandle<Result<PalwFraudFilerRunV1, String>>,
}

/// **The filer's book** (the module's header): the cases by claim, the replay in flight, and the answers read off the accepted blocks.
#[derive(Default)]
pub(super) struct PalwFraudFilerBookV1 {
    pub(super) cases: BTreeMap<Hash64, PalwFraudFilerCaseV1>,
    read_at: Option<u64>,
    running: Option<PalwFraudFilerReplayV1>,
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
    network_form: Option<kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1>,
}

impl PalwFraudFilerBookV1 {
    /// Forget cases below the fence, but drain any already-started blocking replay before another can start.
    pub(super) fn clear(&mut self) {
        let running = self.running.take();
        *self = Self { running, ..Self::default() };
    }

    /// **Take a fresh candidate read**: a new claim opens a case; a case whose claim left the candidates is dropped unless it is
    /// pursued (its outcome is read off the tip). A changed acceptance/job/role restarts that case and invalidates its queued items.
    /// Unrelated settled-case cleanup does not restart a pursued claim's partial backfill. Bounded by [`PALW_FRAUD_FILER_MAX_CASES_V1`].
    pub(super) fn refresh(&mut self, candidates: Vec<PalwFraudFilerCandidateV1>, now_daa: u64) -> BTreeSet<Hash64> {
        let fresh: BTreeSet<Hash64> = candidates.iter().map(|c| c.claim_id).collect();
        let mut invalidated = BTreeSet::new();
        let mut changed = false;
        self.cases.retain(|claim, case| {
            let keep = fresh.contains(claim) || case.pursued();
            if !keep {
                invalidated.insert(*claim);
            }
            keep
        });
        for candidate in candidates {
            let room = self.cases.len() < PALW_FRAUD_FILER_MAX_CASES_V1;
            match self.cases.get_mut(&candidate.claim_id) {
                Some(case) if case.candidate != candidate => {
                    changed = true;
                    invalidated.insert(candidate.claim_id);
                    *case = PalwFraudFilerCaseV1::new(candidate);
                }
                Some(_) => {}
                None if room => {
                    self.cases.insert(candidate.claim_id, PalwFraudFilerCaseV1::new(candidate));
                }
                None => {}
            }
        }
        if !invalidated.is_empty() {
            self.answers.retain(|(claim, _), _| !invalidated.contains(claim));
        }
        if changed {
            self.walked_from = None;
            self.walked_to = None;
            self.walk = None;
        }
        self.read_at = Some(now_daa);
        invalidated
    }

    /// Whether the chain should be read again at `now_daa`.
    pub(super) fn stale(&self, now_daa: u64) -> bool {
        self.read_at != Some(now_daa)
    }

    /// Mismatched runs held now.
    pub(super) fn held_runs(&self) -> usize {
        self.cases.values().filter(|case| matches!(case.verdict, PalwFraudFilerVerdictV1::Mismatch(_))).count()
    }

    /// The next replay or FP metadata probe. A busy replay lane may probe a pending FP body,
    /// so its binding/direct proof is not held behind captures; it never starts another execution.
    #[cfg(test)]
    pub(super) fn next_replay(&self) -> Option<Hash64> {
        self.next_replay_at(0)
    }

    #[cfg(test)]
    fn next_replay_at(&self, current_daa: u64) -> Option<Hash64> {
        self.next_probe_at(current_daa, true)
    }

    fn next_probe_at(&self, current_daa: u64, replay_room: bool) -> Option<Hash64> {
        let busy = !replay_room || self.running.is_some() || self.held_runs() >= PALW_FRAUD_FILER_HELD_RUNS_V1;
        self.cases
            .values()
            .filter(|case| {
                matches!(case.verdict, PalwFraudFilerVerdictV1::Pending)
                    && case.fp_bootstrap.as_ref().is_none_or(|input| {
                        input.ready()
                            || input.payload.job().prompt_mode
                                == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL
                    })
                    && self.running.as_ref().is_none_or(|running| running.candidate != case.candidate)
                    && (!busy || (case.candidate.job.free_prompt && case.fp_bootstrap.is_none()))
                    && !case.candidate.seat
                    && current_daa >= case.input_retry_at
            })
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

    /// A result of an older acceptance/job/role snapshot must neither clear nor settle the current case.
    fn judge_replay(
        &mut self,
        replayed: &PalwFraudFilerCandidateV1,
        result: Result<PalwFraudFilerRunV1, String>,
    ) -> Option<&PalwFraudFilerVerdictV1> {
        if self.cases.get(&replayed.claim_id)?.candidate != *replayed {
            return None;
        }
        self.judge(replayed.claim_id, result)
    }

    /// Judge all three committed roots. An output-only or trace-only mismatch still needs a public proof or DA default.
    pub(super) fn judge(&mut self, claim: Hash64, result: Result<PalwFraudFilerRunV1, String>) -> Option<&PalwFraudFilerVerdictV1> {
        let case = self.cases.get_mut(&claim)?;
        case.runs = case.runs.saturating_add(1);
        let bootstrapped = case.fp_bootstrap.is_some() || case.fp_job_pending;
        if result.is_ok() {
            case.fp_bootstrap = None;
            case.fp_descriptor = None;
            case.fp_job_pending = false;
            case._fp_job_reservation = None;
        }
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
        case.release_pending =
            bootstrapped && matches!(case.verdict, PalwFraudFilerVerdictV1::Honest | PalwFraudFilerVerdictV1::Unjudged(_));
        if case.release_pending {
            case.fp_bootstrap = None;
            case.fp_descriptor = None;
            case.fp_job_pending = false;
            case._fp_job_reservation = None;
        }
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
                case.step_tree = None;
                case.trace = PalwTraceDescentV1::default();
                case.rcore_wanted = None;
                case.legacy_wanted = None;
                case.legacy_round.clear();
                case.lanes_sent.clear();
                case.sent = None;
                case.awaiting_public = None;
                if let Some(input) = case.fp_bootstrap.as_mut() {
                    input.ids.clear();
                    input.chunk = 0;
                }
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
                            (case.legacy_wanted == Some(unit) || case.legacy_round.contains(&unit))
                                && kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v3(
                                    &case.candidate.job.execution_root,
                                    &unit,
                                    &answer.0,
                                    &answer.1,
                                    answer.0.step_leaf_count,
                                    case.fp_bootstrap.as_ref().map_or(
                                        self.network_form
                                            .unwrap_or(kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
                                        |input| input.form,
                                    ),
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
            case.release_pending = false;
            case.fp_bootstrap = None;
            case.fp_descriptor = None;
            case.fp_job_pending = false;
            case._fp_job_reservation = None;
        }
        self.answers.retain(|(held, _), _| held != claim);
        if let Some(case) = self.cases.get_mut(claim) {
            case.frontiers.clear();
            case.witness = None;
            case.legacy_wanted = None;
            case.legacy_round.clear();
            case.lanes_sent.clear();
            case.rcore_wanted = None;
            case.trace = PalwTraceDescentV1::default();
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

    /// G14-V: the subtree/trace descent's next unit, read from history like a selected legacy unit (an answer that predates the
    /// selection is backfilled; only this unit and the binding are cached for the claim).
    pub(super) fn want_rcore(&mut self, claim: Hash64, unit: PalwDaUnitV1) {
        let Some(case) = self.cases.get_mut(&claim) else { return };
        if case.rcore_wanted == Some(unit) {
            return;
        }
        case.rcore_wanted = Some(unit);
        self.answers.retain(|(held, held_unit), _| {
            *held != claim || matches!(held_unit, PalwDaUnitV1::Event { row: 0, tile: 0 } | PalwDaUnitV1::LegacyHeldV2(_))
        });
        self.walked_from = None;
        self.walked_to = None;
        self.walk = None;
        self.read_at = None;
    }

    /// Reading a previously answered legacy unit needs its historical bytes, even if a prior walk predated this selection.
    pub(super) fn want_legacy(&mut self, claim: Hash64, unit: PalwLegacyHeldUnitV2) {
        let Some(case) = self.cases.get_mut(&claim) else { return };
        if case.legacy_wanted == Some(unit) {
            return;
        }
        case.legacy_wanted = Some(unit);
        let round = case.legacy_round.clone();
        self.answers.retain(|(held, held_unit), _| {
            *held != claim || !matches!(held_unit, PalwDaUnitV1::LegacyHeldV2(u) if !round.contains(u))
        });
        self.walked_from = None;
        self.walked_to = None;
        self.walk = None;
        self.read_at = None;
    }

    /// **Lane G14-L: the current k-ary round's units** — their historical answers are read (backfilled like a selected legacy unit);
    /// answers of the previous round's other units leave the cache (each learned one lives on in the case's frontiers).
    pub(super) fn want_legacy_round(&mut self, claim: Hash64, units: Vec<PalwLegacyHeldUnitV2>) {
        let Some(case) = self.cases.get_mut(&claim) else { return };
        if case.legacy_round == units {
            return;
        }
        case.lanes_sent.retain(|unit, _| units.contains(unit));
        case.legacy_round = units;
        let (round, single) = (case.legacy_round.clone(), case.legacy_wanted);
        self.answers.retain(|(held, held_unit), _| {
            *held != claim || !matches!(held_unit, PalwDaUnitV1::LegacyHeldV2(u) if !round.contains(u) && single != Some(*u))
        });
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
    /// G14-V: the claim's step tree is the verifier's own and its execution is not — localize past the step tree (the logits
    /// trace, the committed tokens) instead of settling a mismatched execution as honest.
    PastStepTree,
    /// Lane G14-L: demand these lanes of one k-ary round (each joins this bond's reserved session; the first to land opens it);
    /// `round` is the whole round, whose answers the history reader caches.
    DemandLanes { round: Vec<PalwLegacyHeldUnitV2>, demand: Vec<PalwLegacyProbeV1> },
    /// Lane G14-L: the chain answered lanes of this round that the walk has not read yet.
    AwaitRound(Vec<PalwLegacyHeldUnitV2>),
}

/// **Lane G14-L: the parallel-lane rule's inputs to a step** — `None` below either legacy fence (the one-unit descent, unchanged).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PalwFraudFilerLanesV1 {
    pub(super) now_daa: u64,
    pub(super) form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
}

/// **Lane G14-L: one round's next move**, from the chain and this node's reads alone (a restart re-derives it): a lane the chain
/// answered, this node read and may consume now is learned (one a step); the unanswered ones are demanded as lanes of this bond's
/// reserved session — opened by the first to land, joined by the others while the session's join window is open and it holds fewer
/// than 64 — never a lane already in the session; answered lanes the walk has not read yet are awaited; else wait.
pub(super) fn palw_fraud_filer_round_step_v1(
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    round: Vec<PalwLegacyHeldUnitV2>,
    learnable: &dyn Fn(&PalwLegacyHeldUnitV2) -> bool,
    answer_read: &dyn Fn(&PalwDaUnitV1) -> bool,
    now_daa: u64,
) -> PalwFraudFilerStepV1 {
    use kaspa_consensus_core::palw_legacy_parallel_v1::{PALW_LEGACY_LANE_JOIN_WINDOW_DAA_V1, PALW_LEGACY_LANES_PER_SESSION_V1};
    let unit = |u: &PalwLegacyHeldUnitV2| PalwDaUnitV1::LegacyHeldV2(*u);
    let answered = |u: &PalwLegacyHeldUnitV2| view.is_some_and(|v| v.unit_was_answered_v1(&unit(u)));
    if let Some(u) = round.iter().find(|u| answered(u) && answer_read(&unit(u)) && learnable(u)) {
        return PalwFraudFilerStepV1::Learn(PalwLegacyProbeV1::HeldNode { unit: *u });
    }
    let pending = round.iter().filter(|u| !answered(u)).copied();
    let mine = view.and_then(|v| v.sessions.iter().find(|(bond, _)| bond == me)).map(|(_, session)| session);
    let demand: Vec<PalwLegacyProbeV1> = match mine {
        None => pending.take(PALW_LEGACY_LANES_PER_SESSION_V1 as usize).map(|unit| PalwLegacyProbeV1::HeldNode { unit }).collect(),
        Some(session) => {
            let joinable = session.units.iter().all(|u| matches!(u, PalwDaUnitV1::LegacyHeldV2(_)))
                && now_daa <= session.opened_daa.saturating_add(PALW_LEGACY_LANE_JOIN_WINDOW_DAA_V1);
            let room = (PALW_LEGACY_LANES_PER_SESSION_V1 as usize).saturating_sub(session.units.len());
            if joinable {
                pending
                    .filter(|u| !session.units.contains(&unit(u)))
                    .take(room)
                    .map(|unit| PalwLegacyProbeV1::HeldNode { unit })
                    .collect()
            } else {
                Vec::new()
            }
        }
    };
    if !demand.is_empty() {
        return PalwFraudFilerStepV1::DemandLanes { round, demand };
    }
    if round.iter().any(|u| answered(u) && !answer_read(&unit(u)) && learnable(u)) {
        return PalwFraudFilerStepV1::AwaitRound(round);
    }
    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait)
}

/// **Lane G14-L: the bootstrap's parallel round** — after the binding, every input chunk AND the root round's node lanes ride ONE
/// reserved session (the plan's input round: the verifier replays after it, and the root round's frontiers are already on chain).
/// Chunks are learned in order; the root nodes are read after the replay. Falls back to the one-unit bootstrap below the fences, before
/// the binding, or where the claim needs no reservation step.
pub(super) fn palw_fraud_filer_bootstrap_step_v2(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    lanes: Option<PalwFraudFilerLanesV1>,
    read: impl Fn(&PalwDaUnitV1) -> bool,
) -> PalwFraudFilerStepV1 {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    let parallel = match (lanes, case.binding.as_ref(), case.fp_bootstrap.as_ref()) {
        (Some(lanes), Some(binding), Some(input))
            if facts.outcome.is_none()
                && !facts.ended
                && facts.reserved
                && facts.accusable
                && !input.ready()
                && input.payload.job().prompt_mode != kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL =>
        {
            Some((lanes, binding, input))
        }
        _ => None,
    };
    let Some((lanes, binding, input)) = parallel else {
        return palw_fraud_filer_bootstrap_step_v1(role, view, me, reservable, case, read);
    };
    use kaspa_consensus_core::palw_legacy_parallel_v1::{
        palw_legacy_input_lanes_v1, palw_legacy_kary_round_units_v1, palw_legacy_kary_width_bits_v1,
    };
    let chunks = palw_legacy_input_lanes_v1(binding, lanes.form);
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let Ok(s) = palw_legacy_kary_width_bits_v1(height, chunks) else {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Unjudged));
    };
    let mut round: Vec<PalwLegacyHeldUnitV2> = (0..chunks).map(|chunk| PalwLegacyHeldUnitV2::PromptIds { chunk }).collect();
    if height > 0 {
        round.extend(palw_legacy_kary_round_units_v1(
            kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyTreeV2::Step,
            binding.step_leaf_count,
            height,
            0,
            s,
        ));
    }
    let next = PalwLegacyHeldUnitV2::PromptIds { chunk: input.chunk };
    palw_fraud_filer_round_step_v1(view, me, round, &|u| *u == next, &read, lanes.now_daa)
}

/// **Lane G14-L: the node's LG14-B step with parallel lanes** — past both legacy fences a reserving public bond with a mismatched
/// base0 replica runs the k-ary descent ([`PalwLegacyReplicaV2::kary`]): one round's node lanes in one reserved session, the fused
/// leaf's witness and the terminal as before. Everything else (no binding yet, a seat, below the fences) is the one-unit step.
#[allow(clippy::too_many_arguments)]
pub(super) fn palw_fraud_filer_legacy_step_v3(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    own_court_open: bool,
    case: &PalwFraudFilerCaseV1,
    lanes: Option<PalwFraudFilerLanesV1>,
    answer_read: impl Fn(&PalwDaUnitV1) -> bool,
) -> Result<PalwFraudFilerStepV1, String> {
    use kaspa_consensus_core::palw_legacy_parallel_v1::{PalwLegacyKaryStepV1 as K, palw_legacy_input_lanes_v1};
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    let applies = lanes.is_some()
        && facts.outcome.is_none()
        && !facts.ended
        && !own_court_open
        && facts.reserved
        && facts.accusable
        && case.binding.is_some()
        && matches!(&case.verdict, PalwFraudFilerVerdictV1::Mismatch(run) if run.legacy.is_some());
    let (Some(lanes), true) = (lanes, applies) else {
        return palw_fraud_filer_legacy_step_v2(role, view, me, reservable, own_court_open, case, answer_read);
    };
    let binding = case.binding.as_ref().expect("checked above");
    let PalwFraudFilerVerdictV1::Mismatch(run) = &case.verdict else { unreachable!("checked above") };
    let replica = run.legacy.as_ref().expect("checked above");
    let step = replica.kary(binding, &case.frontiers, palw_legacy_input_lanes_v1(binding, lanes.form))?;
    let probe = match step {
        K::Round { units, .. } => {
            // A lane whose frontier the case already holds is consumed: never learned again.
            let fresh = |u: &PalwLegacyHeldUnitV2| {
                u.node().is_none_or(|(_, level, index)| !case.frontiers.iter().any(|f| f.level == level && f.index == index))
            };
            return Ok(palw_fraud_filer_round_step_v1(view, me, units, &fresh, &answer_read, lanes.now_daa));
        }
        _ if facts.session_open => return Ok(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait)),
        K::Agrees => return Ok(PalwFraudFilerStepV1::PastStepTree),
        K::FirstDivergentLeaf(leaf) => {
            if !palw_legacy_leaf_is_fused_v2(binding, leaf) || case.witness.is_some() {
                return Ok(PalwFraudFilerStepV1::LegacyTerminal { leaf });
            }
            PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::KernelWitness { leaf } }
        }
        K::OwnTreeInconsistent { .. } | K::OwnTreeUnavailable { .. } | K::Conflict { .. } | K::Refused(_) => {
            return Ok(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Unjudged)));
        }
    };
    let unit = probe.unit();
    Ok(match (view.is_some_and(|view| view.unit_was_answered_v1(&unit)), answer_read(&unit)) {
        (true, true) => PalwFraudFilerStepV1::Learn(probe),
        (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
        _ => PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe)),
    })
}

pub(super) fn palw_fraud_filer_bootstrap_step_v1(
    _role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    read: impl Fn(&PalwDaUnitV1) -> bool,
) -> PalwFraudFilerStepV1 {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    if let Some(outcome) = facts.outcome {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(outcome));
    }
    if facts.ended {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Expired));
    }
    if !facts.reserved {
        return PalwFraudFilerStepV1::Engine(if facts.reservable { PalwFilerActionV1::Reserve } else { PalwFilerActionV1::Wait });
    }
    if facts.session_open || !facts.accusable {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait);
    }
    let probe = if case.binding.is_none() && (case.fp_bootstrap.is_some() || case.fp_job_pending) {
        PalwLegacyProbeV1::Binding { row: 0, tile: 0 }
    } else if let Some(input) = case.fp_bootstrap.as_ref() {
        if input.ready() || input.payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL
        {
            return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait);
        }
        PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::PromptIds { chunk: input.chunk } }
    } else if case.fp_job_pending
        && case.fp_job_disclosure_enabled
        && case.fp_descriptor.is_none()
        && case.fp_expected_pin.is_some_and(|pin| pin != Hash64::default())
    {
        PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::FpJob }
    } else {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait);
    };
    let answered = view.is_some_and(|v| v.unit_was_answered_v1(&probe.unit()));
    match (answered, read(&probe.unit())) {
        (true, true) => PalwFraudFilerStepV1::Learn(probe),
        (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
        _ => PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe)),
    }
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
        return Ok(PalwFraudFilerStepV1::PastStepTree);
    }
    Ok(match action {
        PalwFilerActionV1::Demand(probe) => {
            let unit = probe.unit();
            match (view.is_some_and(|view| view.unit_was_answered_v1(&unit)), answer_read(&unit)) {
                (true, true) => PalwFraudFilerStepV1::Learn(probe),
                (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
                _ => PalwFraudFilerStepV1::Engine(action),
            }
        }
        _ => PalwFraudFilerStepV1::Engine(action),
    })
}

/// **G14-V: the node's codec-independent step** — the common engine over the authenticated `StepRange` subtree descent
/// (`palw_fraud_filer_next_subtree_v1`): the claim's committed subtree roots, as authenticated range answers carried them, against
/// the verifier's own tree (built once from its own run). Replaces the linear scan, which refused every claim past 32,768 leaves; an
/// agreeing step tree under a mismatched execution goes past the step tree, never to `Honest`. Blocking: the first call builds the
/// own tree.
pub(super) fn palw_fraud_filer_subtree_step_v1(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    answer_read: impl Fn(&PalwDaUnitV1) -> bool,
) -> Result<PalwFraudFilerStepV1, String> {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    let binding = case.binding.as_ref();
    let descent = match (binding, &case.verdict, &case.step_tree) {
        (Some(binding), PalwFraudFilerVerdictV1::Mismatch(run), Some(tree))
            if !facts.session_open
                && facts.accusable
                && facts.outcome.is_none()
                && (facts.reserved || matches!(role, PalwFilerRoleV1::Seat | PalwFilerRoleV1::Watchdog)) =>
        {
            let own = run.own.as_ref().ok_or("this replay keeps no own run")?;
            let own_tree = own.tree(binding.step_leaf_count)?;
            let range = |first: u64, count: u32| own.range(first, count);
            palw_auth_descent_next_v1(
                tree,
                &|level, index| own_tree.node(level, index, &range),
                kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_BISECT_FINAL_RANGE_V1,
            )
        }
        _ => PalwAuthDescentV1::Agrees,
    };
    let action = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_subtree_v1(
        role,
        &facts,
        case.pursued(),
        binding.map_or(0, |b| b.step_leaf_count),
        descent,
        |leaf| binding.is_some_and(|binding| !palw_da_step_leaf_is_fused_v1(binding, leaf)),
    );
    if matches!(action, PalwFilerActionV1::Done(PalwFilerPhaseV1::Honest)) && case.pursued() {
        return Ok(PalwFraudFilerStepV1::PastStepTree);
    }
    Ok(match action {
        PalwFilerActionV1::Demand(probe) if !matches!(probe, PalwLegacyProbeV1::Terminal { .. }) => {
            let unit = probe.unit();
            match (view.is_some_and(|view| view.unit_was_answered_v1(&unit)), answer_read(&unit)) {
                (true, true) => PalwFraudFilerStepV1::Learn(probe),
                (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
                (false, _) => PalwFraudFilerStepV1::Engine(action),
            }
        }
        action => PalwFraudFilerStepV1::Engine(action),
    })
}

/// **G14-V: what the public localization past an agreeing step tree does next** — a proof to file, or an event to read.
pub(super) enum PalwFraudFilerPastStepV1 {
    File(PalwConvictionFilingV1),
    Read(PalwLegacyProbeV1),
}

/// **G14-V: past the step tree** (blocking). The claim's step tree is the verifier's own and its execution, trace or output is not.
/// From the binding read (authenticated against the claim's execution root), the public tiled events this case read, and its OWN
/// run: a committed logits lane the agreeing head did not output (`LogitsNotStepOutput`), or a flat committed token that is not its
/// agreeing row's selection (`ForgedOutput`) — each held to the fold's own predicate before it is filed. Any other differing field
/// (job, shape, legs, prompt, output root) is a direct proof's, which the reporter door already ran from the same binding; a
/// differing checkpoint leg with everything else agreeing is reported, not guessed.
pub(super) fn palw_fraud_filer_past_step_v1(
    case: &PalwFraudFilerCaseV1,
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    event0: Option<&PalwDaBuiltAnswerV1>,
    file_by: Option<u64>,
    ladder: u64,
) -> Result<PalwFraudFilerPastStepV1, String> {
    let PalwFraudFilerVerdictV1::Mismatch(run) = &case.verdict else { return Err("no mismatched replay is held".into()) };
    let own = run.own.as_ref().ok_or("this replay keeps no own run")?;
    let binding = case.binding.as_ref().ok_or("past the step tree needs the binding read")?;
    let Some(PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event0))) = event0 else {
        return Err("the binding read's event is not held".into());
    };
    let own_binding = own.binding()?;
    if own_binding.step_merkle_root != binding.step_merkle_root || own_binding.step_leaf_count != binding.step_leaf_count {
        return Err("the step trees differ: the step descent's, not this localization's".into());
    }
    if own_binding.full_logits_trace_root == binding.full_logits_trace_root {
        let differs = [
            ("checkpoint leg", own_binding.checkpoint_merkle_root != binding.checkpoint_merkle_root),
            ("activation leg", own_binding.activation_leg_root != binding.activation_leg_root),
            ("job context", own_binding.job_context != binding.job_context),
            (
                "shape / checkpoint profile",
                own_binding.shape_profile != binding.shape_profile
                    || own_binding.checkpoint_profile != binding.checkpoint_profile
                    || own_binding.state_chunk_map_id != binding.state_chunk_map_id,
            ),
        ];
        let named: Vec<&str> = differs.iter().filter(|(_, d)| *d).map(|(n, _)| *n).collect();
        return Err(format!(
            "the step tree and logits trace are the verifier's own; the execution differs in {named:?} (a direct proof's, or the \
             checkpoint-state localization, which is not wired) — nothing guessed"
        ));
    }
    let next = kaspa_consensus_core::palw_auth_descent_v1::palw_trace_localize_next_v1(
        target,
        binding,
        event0,
        &case.trace,
        &|row, tile| own.event(row, tile),
        &|leaf| own.opening(leaf),
        ladder,
    )?;
    match next {
        PalwTraceLocalizeV1::Read { row, tile } => Ok(PalwFraudFilerPastStepV1::Read(PalwLegacyProbeV1::Binding { row, tile })),
        PalwTraceLocalizeV1::Proof(contradiction) => {
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
            Ok(PalwFraudFilerPastStepV1::File(filing))
        }
    }
}

/// The retired linear-scan step (kept for the engine's unit tests).
#[cfg(test)]
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
    let action = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_v1(role, &facts, case.pursued(), &case.bisect, |leaf| {
        binding.is_some_and(|binding| !palw_da_step_leaf_is_fused_v1(binding, leaf))
    });
    match action {
        PalwFilerActionV1::Demand(probe) if !matches!(probe, PalwLegacyProbeV1::Terminal { .. }) => {
            let unit = probe.unit();
            match (view.is_some_and(|view| view.unit_was_answered_v1(&unit)), answer_read(&unit)) {
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
    palw_fraud_filer_public_filing_with_fp_v2(target, rules, answer, file_by, None)
}

/// The public FP job is authenticated by the core proof's claim/binding checks. No input ids or
/// successful replay is required to prosecute a self-consistent bad canonical count/root.
pub(super) fn palw_fraud_filer_public_filing_with_fp_v2(
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    rules: kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1,
    answer: &PalwDaBuiltAnswerV1,
    file_by: Option<u64>,
    fp_job: Option<&kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,
) -> Result<Option<PalwConvictionFilingV1>, String> {
    use kaspa_consensus_core::palw_offence_attribution_v1::{
        palw_binding_identity_fault_v1, palw_fp_canonical_prompt_admit_v1, palw_output_fault_v1, palw_prompt_not_anchored_admit_v1,
        palw_prompt_not_anchored_fault_v1,
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
    let fp_proof = fp_job.and_then(|job| {
        let prefill = palw_fp_canonical_prompt_admit_v1(target, binding, job, rules).ok()?;
        if binding.job_context.declared_prefill_tokens == prefill
            && u64::from(prefill) > kaspa_consensus_core::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1
        {
            return None;
        }
        Some(PalwPromptProofV1::FpCanonicalWhole { job: Box::new(job.clone()) })
    });
    let contradiction = if palw_binding_identity_fault_v1(target, binding, rules, true).map_err(|e| e.to_string())?.is_some() {
        PalwPanelContradictionV1::IdentityMismatch { binding: binding.clone() }
    } else if kaspa_consensus_core::palw_step_leg::check_step_refutation_capped_v1(&structural, binding.step_leaf_count).is_ok() {
        PalwPanelContradictionV1::StepStructural(structural)
    } else if let Some(proof) = fp_proof
        && palw_prompt_not_anchored_fault_v1(target, binding, &proof, rules).map_err(|e| e.to_string())?
    {
        PalwPanelContradictionV1::PromptNotAnchored { binding: binding.clone(), proof }
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
        if palw_output_fault_v1(target, binding, &pin).map_err(|e| e.to_string())? {
            PalwPanelContradictionV1::OutputMismatch { binding: binding.clone(), pin }
        } else if let (Some(job), PalwTraceEventDisclosureV1::Flat { pin, .. }) = (fp_job.filter(|j| j.is_constraint()), event) {
            let table = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_token_table_for_job_v1(job)
                .ok_or("the public verifier does not hold this job's tokenizer table")?;
            let Some(proof) = constraint::flat_contradiction_v1(target, binding, job, pin, table)? else { return Ok(None) };
            proof
        } else {
            return Ok(None);
        }
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

/// Run only the verifier's own backend. FP execution uses the job's controls and determines its executed context itself.
#[allow(clippy::too_many_arguments)]
pub(super) fn palw_fraud_filer_execute_v1(
    backend: Box<dyn PalwExecutionBackendV1>,
    mut ctx: PalwJobContextV2,
    prompt32: Vec<u32>,
    prompt_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    attempt_draw: Option<bool>,
    fp_job: Option<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,
    reserved: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
) -> Result<PalwFraudFilerRunV1, String> {
    let prompt: Vec<usize> = prompt32.iter().map(|id| *id as usize).collect();
    let job_reservation = fp_job.as_ref().map(|job| {
        let bytes = borsh::object_length(job).map_err(|e| e.to_string())? as u64;
        crate::palw_memory_ledger::host_ledger_v1().reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 { role: "public-replay-job", class_id: job.class_id, job: ctx.job_id },
            bytes.saturating_add(1024).saturating_mul(8),
        ).map_err(|e| e.to_string())
    }).transpose()?;
    let outcome = if let Some(job) = fp_job.as_ref() {
        // The public replay owns its scope just as the producer and interval verifier do.
        // A constraint's authenticated job tail and this host's matching token table determine
        // the mask; a missing/mismatched table is a host refusal, never a fraud verdict.
        let mask = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_mask_for_host_v1(
            job,
            backend.token_table_for_job_v1(job),
        )?;
        let run = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_with_constraint_scope_v1(mask, || {
            backend.execute_free_prompt(job, &prompt)
        })?;
        ctx = backend
            .fp_job_context_for_executed_v1(job, run.facts.decode_tokens_executed)
            .ok_or("the backend cannot derive its own FP executed context")?;
        run.outcome
    } else {
        backend.execute(&ctx, &prompt)?
    };
    let backend: Arc<dyn PalwExecutionBackendV1> = Arc::from(backend);
    let material = Arc::new(outcome.material);
    let prompt32 = Arc::new(prompt32);
    let own_backend = backend.clone();
    let own_material = material.clone();
    let own_prompt = prompt32.clone();
    let own = Arc::new(PalwFraudFilerOwnRunV1::new(backend.clone(), material.clone(), prompt32.clone()));
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
                attempt_draw: if fp_job.is_some() { None } else { attempt_draw },
                output_root: fp_job.as_ref().map(|_| outcome.output_root),
                job_pin: fp_job.as_ref().map(|_| kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&ctx)),
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
        own: Some(own),
        fp_job: fp_job.map(Box::new),
        _job_reservation: job_reservation,
        _reservation: reserved,
    })
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
        false_valid: &mut crate::palw_filer_false_valid::PalwFalseValidFilerV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        seat_replay_room: bool,
        public_materials: &HashMap<Hash64, Vec<Vec<u8>>>,
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
            if book.running.as_ref().is_some_and(|replay| replay.handle.is_finished()) {
                let replay = book.running.take().expect("finished above");
                drop(replay.handle.await); // Discard the result, releasing its capture and reservation.
            }
            if !book.cases.is_empty() || book.running.is_some() {
                book.clear();
            }
            court_pending.retain(|(_, round, responder, _)| !palw_fraud_filer_queued_v1(*round, *responder));
            return;
        }
        book.network_form = Some(params.palw_prompt_ids_form_at(current_daa));
        if book.stale(current_daa) {
            let candidates = session.clone().spawn_blocking(move |c| c.palw_fraud_filer_candidates_v1(bond_key)).await;
            let invalidated = book.refresh(candidates, current_daa);
            court_pending.retain(|(claim, round, responder, _)| {
                !invalidated.contains(claim) || !palw_fraud_filer_queued_v1(*round, *responder)
            });
            court_due.retain(|(claim, round, responder), _| {
                !invalidated.contains(claim) || !palw_fraud_filer_queued_v1(*round, *responder)
            });
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
                let legacy_rounds: BTreeMap<Hash64, BTreeSet<PalwLegacyHeldUnitV2>> = book
                    .cases
                    .values()
                    .filter(|case| !case.legacy_round.is_empty())
                    .map(|case| (case.candidate.claim_id, case.legacy_round.iter().copied().collect()))
                    .collect();
                let rcore_wanted: BTreeMap<Hash64, PalwDaUnitV1> =
                    book.cases.values().filter_map(|case| case.rcore_wanted.map(|unit| (case.candidate.claim_id, unit))).collect();
                let history_form = params.palw_prompt_ids_form_at(current_daa);
                let history_maximum = self
                    .config
                    .court
                    .max_close_bytes()
                    .min(kaspa_consensus_core::palw_legacy_answer_chunks_v2::PALW_LEGACY_ANSWER_MAX_BYTES_V2);
                let scratch = crate::palw_memory_ledger::host_ledger_v1().reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "public-answer-history",
                        class_id: Hash64::default(),
                        job: previous.map_or(Hash64::default(), |w| w.anchor),
                    },
                    8 * history_maximum.saturating_add(16_384),
                );
                let result = match scratch {
                    Ok(scratch) => {
                        session
                            .clone()
                            .spawn_blocking(move |c| {
                                let _scratch = scratch;
                                palw_fraud_filer_read_page_with_budget_v2(
                                    c,
                                    oldest,
                                    floor,
                                    previous,
                                    PALW_FRAUD_FILER_WALK_BLOCKS_V1,
                                    &wanted,
                                    &legacy_wanted,
                                    &legacy_rounds,
                                    &rcore_wanted,
                                    history_form,
                                    history_maximum,
                                )
                            })
                            .await
                    }
                    Err(why) => Err(format!("public history waits for memory: {why}")),
                };
                match result {
                    Ok((walk, reset, answers)) => {
                        if reset {
                            let restarted: BTreeSet<Hash64> = book.pursued().into_iter().collect();
                            court_pending.retain(|(claim, round, responder, _)| {
                                !restarted.contains(claim) || !palw_fraud_filer_queued_v1(*round, *responder)
                            });
                            court_due.retain(|(claim, round, responder), _| {
                                !restarted.contains(claim) || !palw_fraud_filer_queued_v1(*round, *responder)
                            });
                        }
                        book.walked(walk, reset, answers);
                    }
                    Err(why) => {
                        warn!("[{PALW_PANEL}] the fraud filer's public history page is unavailable: {why}; watermark unchanged")
                    }
                }
            }
        }
        // The replay in flight, polled every tick until it returns.
        if book.running.as_ref().is_some_and(|replay| replay.handle.is_finished()) {
            let replay = book.running.take().expect("held above");
            let claim = replay.candidate.claim_id;
            let result = replay.handle.await.unwrap_or_else(|e| Err(format!("the replay task did not finish: {e}")));
            match book.judge_replay(&replay.candidate, result) {
                Some(PalwFraudFilerVerdictV1::Mismatch(run)) => error!(
                    "[{PALW_PANEL}] claim {claim}: this node's replay of the claim's job does NOT reproduce its execution root (replayed \
                     {}) — every seat of its panel let it pass or none looked; pursuing it as a public bond (LG14-A, RFC-0014 §6)",
                    run.execution_root
                ),
                Some(PalwFraudFilerVerdictV1::Honest) => {
                    #[cfg(test)]
                    {
                        let mut observed = self.fraud_filer_honest_replays.lock().unwrap();
                        if observed.len() >= PALW_FRAUD_FILER_MAX_CASES_V1 {
                            observed.clear();
                        }
                        observed.insert(claim);
                    }
                    debug!("[{PALW_PANEL}] claim {claim}: the fraud filer's replay reproduces the claim — honest, nothing filed")
                }
                Some(PalwFraudFilerVerdictV1::Unjudged(why)) => {
                    warn!("[{PALW_PANEL}] claim {claim}: the fraud filer cannot judge the claim — {why} (LG14-A)")
                }
                _ => {}
            }
        }
        if let Some(claim) = book.next_probe_at(current_daa, seat_replay_room) {
            self.fraud_filer_start_replay_v1(
                session,
                book,
                claim,
                network_domain,
                current_daa,
                public_materials.get(&claim).map(Vec::as_slice).unwrap_or(&[]),
                seat_replay_room,
            )
            .await;
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
            if let Some(case) = book.cases.get_mut(&claim) {
                case.fp_expected_pin = target.as_ref().map(|target| target.job_identity);
                case.fp_job_disclosure_enabled = params.palw_legacy_held_da_v2_active_at(current_daa);
            }
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
            if book.cases.get(&claim).is_some_and(|case| case.release_pending) {
                // Stop unsent bootstrap moves; a handed-off reservation may still land. Observe it
                // for one resend interval before forgetting a currently unreserved case.
                court_pending.retain(|(held, round, responder, _)| {
                    *held != claim || !palw_fraud_filer_queued_v1(*round, *responder) || *round == PALW_FRAUD_FILER_RELEASE_ROUND_V1
                });
                if !facts.reserved {
                    court_pending
                        .retain(|(held, round, responder, _)| *held != claim || !palw_fraud_filer_queued_v1(*round, *responder));
                    let case = book.cases.get_mut(&claim).expect("held above");
                    let reservation_in_flight = matches!(case.sent,
                        Some((PalwFraudFilerSentV1::Reserve, at, _))
                        if current_daa.saturating_sub(at) < PALW_FRAUD_FILER_RESEND_DAA_V1);
                    if !reservation_in_flight {
                        case.release_pending = false;
                        case.sent = None;
                    }
                } else if !facts.session_open
                    && !court_pending
                        .iter()
                        .any(|(held, round, responder, _)| *held == claim && palw_fraud_filer_queued_v1(*round, *responder))
                {
                    self.fraud_filer_queue_v1(
                        book,
                        claim,
                        PalwFraudFilerSentV1::Release,
                        current_daa,
                        court_pending,
                        court_due,
                        || {
                            let mut signed = true;
                            let object = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_released_object_v1(
                                network_domain,
                                claim,
                                bond_key,
                                |m| {
                                    self.sign(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).unwrap_or_else(|| {
                                        signed = false;
                                        Vec::new()
                                    })
                                },
                            );
                            if signed { Ok(object) } else { Err("no signing key for reservation release".into()) }
                        },
                    );
                }
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
                let fp_job = book.cases.get(&claim).and_then(|c| c.public_fp_job_v1().cloned());
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
                        .or_else(|| {
                            fp_job.as_ref().and_then(|job| {
                                kaspa_consensus_core::palw_offence_attribution_v1::palw_fp_canonical_prompt_admit_v1(
                                    target,
                                    event.binding(),
                                    job,
                                    rules,
                                )
                                .ok()
                                .filter(|n| *n == event.binding().job_context.declared_prefill_tokens)
                            })
                        })
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
                let bytes = bytes.saturating_add(if fp_job.as_ref().is_some_and(|job| job.is_constraint()) {
                    // Leaf hashes plus cached Merkle levels, built once for all opened IDs.
                    match answer {
                        PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event)) =>
                            u64::from(event.binding().shape_profile.vocab_size).saturating_add(1).saturating_mul(256),
                        _ => 0,
                    }
                } else { 0 });
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
                    palw_fraud_filer_public_filing_with_fp_v2(&target, rules, &answer, file_by, fp_job.as_ref())
                })
                .await
                .unwrap_or_else(|e| Err(format!("the public proof worker failed: {e}")));
                match proof {
                    Ok(Some(filing)) => {
                        // The public proof refutes the liable Valid signers as well as the
                        // executor. Keep it before settlement removes this case from pursuit;
                        // the existing false-Valid scanner independently checks every receipt.
                        false_valid.note_executor_refuted_v1(
                            crate::palw_filer_false_valid::palw_false_valid_armed_v1(
                                params,
                                self.config.fee_outpoint.is_some(),
                                current_daa,
                            ),
                            &filing.object,
                            view.as_ref().map(|v| v.accepted_daa),
                            current_daa,
                        );
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
            let lanes_on = params.palw_legacy_held_da_v2_active_at(current_daa);
            let lanes = lanes_on.then_some(PalwFraudFilerLanesV1 { now_daa: current_daa, form });
            court_pending.retain(|(held, round, responder, object)| {
                if *held != claim || !palw_fraud_filer_queued_v1(*round, *responder) { return true; }
                if *round == PALW_FRAUD_FILER_RESERVE_ROUND_V1 { return !facts.reserved; }
                if *round == PALW_FRAUD_FILER_TERMINAL_ROUND_V2 { return !own_court_open; }
                // Lane G14-L: a queued lane stays until it lands in this bond's session or its unit is answered.
                if lanes_on && let PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand } = object {
                    let unit = PalwDaUnitV1::LegacyHeldV2(demand.unit);
                    return !view.as_ref().is_some_and(|v| {
                        v.unit_was_answered_v1(&unit)
                            || v.sessions.iter().any(|(bond, session)| *bond == bond_key && session.units.contains(&unit))
                    });
                }
                if facts.session_open { return false; }
                if let Some((PalwFraudFilerSentV1::Reacquire { session_number, .. }, _, _)) = book.cases.get(&claim).and_then(|c| c.sent) {
                    return view.as_ref().and_then(|v| v.record.as_ref()).and_then(|r| r.live.get(&bond_key))
                        .is_some_and(|row| row.sessions_opened < session_number);
                }
                !book.cases.get(&claim).is_some_and(|case| matches!(case.sent,
                    Some((PalwFraudFilerSentV1::Demand(probe), _, _)) if view.as_ref().is_some_and(|v| v.unit_was_answered_v1(&probe.unit()))))
            });
            if court_pending.iter().any(|(held, round, responder, _)| *held == claim && palw_fraud_filer_queued_v1(*round, *responder))
            {
                continue;
            }
            // Lane G14-L: a k-ary round's lanes are read one a step; give a round of 64 its reads in one tick.
            let steps = if lanes_on { 4 + kaspa_consensus_core::palw_legacy_parallel_v1::PALW_LEGACY_LANES_PER_SESSION_V1 as usize } else { 4 };
            for _ in 0..steps {
                let Some(case) = book.cases.get(&claim) else { break };
                let legacy = params.palw_legacy_held_da_v2_active_at(current_daa)
                    && matches!(&case.verdict, PalwFraudFilerVerdictV1::Mismatch(run) if run.legacy.is_some());
                let step = if case.fp_bootstrap.is_some() || case.fp_job_pending {
                    palw_fraud_filer_bootstrap_step_v2(role, view.as_ref(), &bond_key, reservable, case, lanes, |unit| {
                        book.answers.contains_key(&(claim, *unit))
                    })
                } else if legacy {
                    let owned = case.clone();
                    let view = view.clone();
                    let read: BTreeSet<PalwDaUnitV1> =
                        book.answers.keys().filter(|(held, _)| *held == claim).map(|(_, unit)| *unit).collect();
                    match tokio::task::spawn_blocking(move || {
                        palw_fraud_filer_legacy_step_v3(role, view.as_ref(), &bond_key, reservable, own_court_open, &owned, lanes, |unit| {
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
                    // G14-V: the codec-independent authenticated subtree descent (its first call builds the own tree: blocking).
                    let owned = case.clone();
                    let view = view.clone();
                    let read: BTreeSet<PalwDaUnitV1> =
                        book.answers.keys().filter(|(held, _)| *held == claim).map(|(_, unit)| *unit).collect();
                    match tokio::task::spawn_blocking(move || {
                        palw_fraud_filer_subtree_step_v1(role, view.as_ref(), &bond_key, reservable, &owned, |unit| read.contains(unit))
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("the public subtree descent task did not finish: {e}")))
                    {
                        Ok(step) => step,
                        Err(why) => {
                            warn!("[{PALW_PANEL}] claim {claim}: the public subtree descent cannot proceed: {why}");
                            if !book.descent_failed(&claim, why) {
                                debug!("[{PALW_PANEL}] claim {claim}: waiting for the remaining public proof history before settling");
                            }
                            break;
                        }
                    }
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
                                case.accept_learned_v1(owned);
                                // A learned node, range or trace event lives on in the case's authenticated tree; the bytes go.
                                if matches!(probe, PalwLegacyProbeV1::HeldNode { .. } | PalwLegacyProbeV1::Range { .. })
                                    || matches!(probe, PalwLegacyProbeV1::Binding { row, tile } if (row, tile) != (0, 0))
                                {
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
                        match probe {
                            PalwLegacyProbeV1::HeldNode { unit } => book.want_legacy(claim, unit),
                            PalwLegacyProbeV1::Range { .. } => book.want_rcore(claim, probe.unit()),
                            PalwLegacyProbeV1::Binding { row, tile } if (row, tile) != (0, 0) => book.want_rcore(claim, probe.unit()),
                            _ => {}
                        }
                        if let Some(view) = view.as_ref()
                            && let Some(request) =
                                book.cases.get_mut(&claim).and_then(|c| c.reacquire_after_v1(view, bond_key, probe, current_daa))
                        {
                            let item = PalwFraudFilerSentV1::Reacquire { probe, session_number: request.session_number };
                            self.fraud_filer_queue_v1(book, claim, item, current_daa, court_pending, court_due, || {
                                let mut signed = true;
                                let object = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_reacquire_object_v1(
                                    network_domain,
                                    request,
                                    |m| {
                                        self.sign(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).unwrap_or_else(|| {
                                            signed = false;
                                            Vec::new()
                                        })
                                    },
                                );
                                if signed { Ok(object) } else { Err("no signing key for reacquisition".into()) }
                            });
                        }
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait) => break,
                    PalwFraudFilerStepV1::AwaitRound(round) => {
                        book.want_legacy_round(claim, round);
                        break;
                    }
                    PalwFraudFilerStepV1::DemandLanes { round, demand } => {
                        book.want_legacy_round(claim, round);
                        let Some(view) = view.as_ref() else { break };
                        self.fraud_filer_queue_lanes_v1(
                            book,
                            claim,
                            &demand,
                            current_daa,
                            court_pending,
                            court_due,
                            |probe, binding| {
                                palw_fraud_filer_demand_object_v1(
                                    &network_domain,
                                    claim,
                                    &view.execution_root,
                                    probe,
                                    binding,
                                    bond_key,
                                    form,
                                    |message, context| self.sign(message, context),
                                )
                            },
                        );
                        break;
                    }
                    PalwFraudFilerStepV1::PastStepTree => {
                        // G14-V: the step tree is the verifier's own and the execution is not — localize the logits trace or
                        // the committed tokens from the binding read, public events and the own run; never settle as honest.
                        let Some(target) = target.clone() else {
                            debug!("[{PALW_PANEL}] claim {claim}: past the step tree, the claim's offence target is not readable yet");
                            break;
                        };
                        let fallback = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
                            &self.config.court,
                            params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
                        );
                        let ladder = self.seat_refutation_ladder_v1(target.class_id, fallback, current_daa);
                        let file_by = view.as_ref().map(|v| v.hard_deadline_daa.saturating_sub(2));
                        let owned = case.clone();
                        let event0 = book.answers.get(&(claim, PalwDaUnitV1::Event { row: 0, tile: 0 })).cloned();
                        let past = tokio::task::spawn_blocking(move || {
                            palw_fraud_filer_past_step_v1(&owned, &target, event0.as_ref(), file_by, ladder)
                        })
                        .await
                        .unwrap_or_else(|e| Err(format!("the past-step localization task did not finish: {e}")));
                        match past {
                            Ok(PalwFraudFilerPastStepV1::File(filing)) => {
                                info!("[{PALW_PANEL}] claim {claim}: past an agreeing step tree, a public proof is filed (G14-V)");
                                let mut door = self.conviction_door_v1(session, reporter, bond_key, network_domain);
                                let case = book.cases.get_mut(&claim).expect("pursued above");
                                if !door.holds(&filing.offence_key)
                                    && case.direct_handed < super::reporter_filer::PALW_FILER_HAND_OFFS_PER_OFFENCE_V1
                                    && case
                                        .direct_asked_at
                                        .is_none_or(|at| current_daa.saturating_sub(at) >= PALW_FRAUD_FILER_RESEND_DAA_V1)
                                {
                                    case.direct_asked_at = Some(current_daa);
                                    match door.file(filing) {
                                        PalwFileOutcomeV1::Queued { .. } | PalwFileOutcomeV1::AlreadyFiled => case.direct_handed += 1,
                                        PalwFileOutcomeV1::AlreadyConvicted => {}
                                        why => warn!(
                                            "[{PALW_PANEL}] claim {claim}: the past-step proof waits at the reporter door: {why:?}"
                                        ),
                                    }
                                }
                                break;
                            }
                            Ok(PalwFraudFilerPastStepV1::Read(probe)) => {
                                let unit = probe.unit();
                                book.want_rcore(claim, unit);
                                let answered = view.as_ref().is_some_and(|v| v.unit_was_answered_v1(&unit));
                                if answered && let Some(answer) = book.answers.get(&(claim, unit)).cloned() {
                                    let mut owned = book.cases.get(&claim).expect("pursued above").clone();
                                    let learned = tokio::task::spawn_blocking(move || owned.learn(probe, &answer).map(|()| owned))
                                        .await
                                        .unwrap_or_else(|e| Err(format!("the read did not finish: {e}")));
                                    match learned {
                                        Ok(owned) => {
                                            book.cases.get_mut(&claim).expect("held above").accept_learned_v1(owned);
                                            book.answers.remove(&(claim, unit));
                                            continue;
                                        }
                                        Err(why) => {
                                            warn!("[{PALW_PANEL}] claim {claim}: the chain's event {probe:?} does not read: {why}");
                                            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
                                            break;
                                        }
                                    }
                                }
                                if answered {
                                    break; // The walk reads it next.
                                }
                                let Some(view) = view.as_ref() else { break };
                                let binding = book.cases.get(&claim).and_then(|c| c.binding.clone());
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
                            Err(why) => {
                                warn!("[{PALW_PANEL}] claim {claim}: past the step tree nothing is provable yet: {why}");
                                if !book.descent_failed(&claim, why) {
                                    debug!("[{PALW_PANEL}] claim {claim}: waiting for the remaining public proof history before settling");
                                }
                                break;
                            }
                        }
                    }
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
                        match probe {
                            PalwLegacyProbeV1::HeldNode { unit } => book.want_legacy(claim, unit),
                            PalwLegacyProbeV1::Range { .. } => book.want_rcore(claim, probe.unit()),
                            PalwLegacyProbeV1::Binding { row, tile } if (row, tile) != (0, 0) => book.want_rcore(claim, probe.unit()),
                            _ => {}
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
                    PalwFraudFilerSentV1::Release => PALW_FRAUD_FILER_RELEASE_ROUND_V1,
                    PalwFraudFilerSentV1::Demand(_) | PalwFraudFilerSentV1::Reacquire { .. } => PALW_FRAUD_FILER_DEMAND_ROUND_V1,
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

    /// **Lane G14-L: queue a round's lanes** — each a tag-157 demand under the filer's own key, dated now; a lane is resent only after
    /// [`PALW_FRAUD_FILER_RESEND_DAA_V1`] and at most [`PALW_FRAUD_FILER_SENDS_V1`] times (a lane whose sends are spent, or that cannot
    /// be built, ends this node's pursuit loudly, as one item does).
    #[allow(clippy::too_many_arguments)]
    fn fraud_filer_queue_lanes_v1(
        &self,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        demand: &[PalwLegacyProbeV1],
        current_daa: u64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        build: impl Fn(PalwLegacyProbeV1, Option<&PalwStepBindingV2>) -> Result<PalwConsensusObjectV2, String>,
    ) {
        let Some(case) = book.cases.get_mut(&claim) else { return };
        let binding = case.binding.clone();
        let mut queued = 0usize;
        let mut failed = None;
        for probe in demand {
            let PalwLegacyProbeV1::HeldNode { unit } = *probe else { continue };
            match case.lanes_sent.get(&unit) {
                Some((at, _)) if current_daa.saturating_sub(*at) < PALW_FRAUD_FILER_RESEND_DAA_V1 => continue,
                Some((_, sends)) if *sends >= PALW_FRAUD_FILER_SENDS_V1 => {
                    warn!(
                        "[{PALW_PANEL}] claim {claim}: the fraud filer's lane {unit:?} was sent {PALW_FRAUD_FILER_SENDS_V1} times and never \
                         landed (G14-L)"
                    );
                    failed = Some(format!("lane {unit:?} never landed"));
                    break;
                }
                _ => {}
            }
            match build(*probe, binding.as_ref()) {
                Ok(object) => {
                    court_due.insert((claim, PALW_FRAUD_FILER_DEMAND_ROUND_V1, false), current_daa);
                    court_pending.push((claim, PALW_FRAUD_FILER_DEMAND_ROUND_V1, false, object));
                    let sends = case.lanes_sent.get(&unit).map_or(1, |(_, sends)| sends.saturating_add(1));
                    case.lanes_sent.insert(unit, (current_daa, sends));
                    queued += 1;
                }
                Err(why) => {
                    warn!("[{PALW_PANEL}] claim {claim}: the fraud filer cannot build lane {unit:?}: {why} (G14-L)");
                    failed = Some(why);
                    break;
                }
            }
        }
        if let Some(why) = failed {
            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
            return;
        }
        if queued > 0 {
            info!("[{PALW_PANEL}] claim {claim}: the fraud filer queues {queued} lanes of one round (G14-L, k-ary descent)");
        }
    }

    fn fraud_filer_hold_missing_fp_job_v1(&self, book: &mut PalwFraudFilerBookV1, claim: Hash64) -> bool {
        let Some(case) = book.cases.get_mut(&claim) else { return false };
        if case.fp_job_pending {
            return true;
        }
        let Ok(ticket) = crate::palw_memory_ledger::host_ledger_v1().reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                role: "public-fp-job-binding",
                class_id: case.candidate.job.class_id,
                job: claim,
            },
            4 * self.config.court.max_close_bytes(),
        ) else {
            return false;
        };
        case.fp_job_pending = true;
        case._fp_job_reservation = Some(Arc::new(ticket));
        book.walked_from = None;
        book.walked_to = None;
        book.walk = None;
        book.read_at = None;
        true
    }

    /// Retain the authenticated public FP job under a small input ticket. In particular, canonical
    /// input proofs must be reachable before a full replay ticket or a backend ceiling context exists.
    fn fraud_filer_hold_fp_input_v1(
        &self,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        payload: input::PalwFraudFilerFpDescriptorV1,
        form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> bool {
        let Some(case) = book.cases.get_mut(&claim) else { return false };
        if case.fp_bootstrap.is_some() {
            return true;
        }
        // Canonical bootstrap retains only the public job. Its ids are derived under the full
        // replay ticket or the separate direct-proof ticket, never accumulated here. A producer's
        // inflated count must not require that many input bytes just to prove the count false.
        let n = if payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL {
            0
        } else {
            u64::from(payload.job().prompt_tokens)
        };
        let bytes = n
            .saturating_mul(16)
            .saturating_add(n.div_ceil(32).saturating_mul(256))
            .saturating_add(2 * self.config.court.max_close_bytes());
        let Ok(ticket) = crate::palw_memory_ledger::host_ledger_v1().reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                role: "public-fp-input",
                class_id: payload.job().class_id,
                job: claim,
            },
            bytes,
        ) else {
            return false;
        };
        case.fp_bootstrap = Some(input::PalwFraudFilerFpBootstrapV1 {
            payload: Arc::new(payload),
            form,
            ids: Vec::new(),
            chunk: 0,
            _reservation: Some(Arc::new(ticket)),
        });
        book.walked_from = None;
        book.walked_to = None;
        book.walk = None;
        book.read_at = None;
        true
    }

    /// Start an attempt derived from its header, or an FP job/input authenticated against its accepted commitment and recorded pin.
    /// Run the verifier's own model under the full-seat memory reservation, retaining only its own capture. Missing public history
    /// or input waits without spending a run; a backend that cannot execute the job remains explicitly unjudged.
    async fn fraud_filer_start_replay_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        network_domain: Hash64,
        current_daa: u64,
        public_material: &[Vec<u8>],
        replay_room: bool,
    ) {
        let Some(case) = book.cases.get(&claim) else { return };
        let candidate = case.candidate.clone();
        let job = candidate.job.clone();
        let producer = candidate.producer;
        let unjudged = |book: &mut PalwFraudFilerBookV1, why: String| {
            debug!("[{PALW_PANEL}] claim {claim}: the fraud filer does not judge it — {why}");
            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
        };
        let metadata_only = !replay_room || book.running.is_some() || book.held_runs() >= PALW_FRAUD_FILER_HELD_RUNS_V1;
        let fp_payload = if job.free_prompt {
            let snapshot = candidate.clone();
            let payload = if let Some(input) = book.cases.get(&claim).and_then(|c| c.fp_bootstrap.as_ref()) {
                Ok(input.payload.as_ref().clone())
            } else if let Some(job) = book.cases.get(&claim).and_then(|c| c.fp_descriptor.as_ref()) {
                Ok(job.as_ref().clone())
            } else {
                session
                    .clone()
                    .spawn_blocking(move |c| {
                        if metadata_only {
                            palw_fraud_filer_fp_metadata_v1(c, &snapshot, network_domain)
                        } else {
                            palw_fraud_filer_fp_payload_v1(c, &snapshot, network_domain)
                        }
                    })
                    .await
                    .map(input::PalwFraudFilerFpDescriptorV1::Accepted)
            };
            match payload {
                Ok(payload) => Some(payload),
                Err(why) => {
                    // Binding/job/shape proofs and the existing binding DA obligation do not
                    // require the accepted FP body, or an executable backend context.
                    self.fraud_filer_hold_missing_fp_job_v1(book, claim);
                    if let Some(case) = book.cases.get_mut(&claim) {
                        case.input_retry_at = current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    }
                    debug!("[{PALW_PANEL}] claim {claim}: public FP job unavailable; pursuing its binding: {why}");
                    return;
                }
            }
        } else {
            None
        };
        // A probe selected while a replay/capture occupies the lane must not instantiate a
        // backend or reserve/start an extra replay. Missing-body binding pursuits were held above.
        if metadata_only {
            book.cases.get_mut(&claim).expect("held above").input_retry_at =
                current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
            return;
        }
        let Some(artifact_root) = job.artifact_root else { return unjudged(book, "its class is gone from the registry".into()) };
        let mut backend = match self.resolve_backend(session, job.class_id, artifact_root) {
            Ok(backend) => backend,
            Err(why) => {
                if book.cases.get(&claim).is_some_and(|case| case.fp_job_pending) {
                    book.cases.get_mut(&claim).expect("held above").input_retry_at =
                        current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    debug!("[{PALW_PANEL}] claim {claim}: model resolution waits while public binding pursuit remains live: {why}");
                    return;
                }
                return unjudged(book, format!("its class does not resolve on this host ({why})"));
            }
        };
        let prompt_form = self.class_prompt_ids_form(job.class_id);
        let mut prepared_only = false;
        let (ctx, mut prompt, fp_payload) = if let Some(payload) = fp_payload {
            let fp_job = payload.job();
            if fp_job.network_domain != network_domain {
                return unjudged(book, "the disclosed FP job has another network domain".into());
            }
            let canonical = fp_job.prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL;
            let user = fp_job.prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER;
            if ((canonical
                && matches!(
                    fp_job.version,
                    kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION
                        | kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V4_VERSION
                ))
                || user)
                && self.consensus_config.params.palw_legacy_held_da_v2_active_at(current_daa)
            {
                let held = book.cases.get(&claim).is_some_and(|case| case.fp_bootstrap.is_some());
                if !self.fraud_filer_hold_fp_input_v1(book, claim, payload.clone(), prompt_form) {
                    book.cases.get_mut(&claim).expect("held above").input_retry_at =
                        current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    return;
                }
                // Missing user input must reach the public court BEFORE context derivation or
                // a full replay ticket can fail. Authentication holds only the input ticket;
                // the worker hashes one public prefix, never a producer capture, off the loop.
                if user {
                    let input = book.cases.get(&claim).and_then(|case| case.fp_bootstrap.as_ref()).expect("held above");
                    if !input.ready() {
                        match input::palw_fraud_filer_read_user_input_v1(input.payload.clone(), prompt_form, public_material).await {
                            Ok(ids) => {
                                book.cases.get_mut(&claim).expect("held above").fp_bootstrap.as_mut().expect("held above").ids = ids
                            }
                            Err(why) => {
                                book.cases.get_mut(&claim).expect("held above").input_retry_at =
                                    current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                                debug!(
                                    "[{PALW_PANEL}] claim {claim}: the fraud filer's authenticated public FP input waits before replay: {why}"
                                );
                                return;
                            }
                        }
                    }
                    // A fresh complete public input should not open an unnecessary reservation
                    // while the ordinary replay runs. An existing court bootstrap stays held
                    // until its replay verdict releases or prosecutes its live reservation.
                    prepared_only = !held && !book.cases.get(&claim).is_some_and(|case| case.fp_job_pending);
                }
            }
            // Budget the declared ceiling, even if the producer claims an early stop. Only the verifier's run decides its context.
            let Some(ctx) = backend.fp_job_context_v1(payload.job()) else {
                if book.cases.get(&claim).is_some_and(|c| c.fp_bootstrap.is_some()) {
                    book.cases.get_mut(&claim).expect("held above").input_retry_at =
                        current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    return;
                }
                return unjudged(book, "this backend cannot derive the FP job's ceiling context".into());
            };
            (ctx, Vec::new(), Some(payload))
        } else {
            let Some((ctx, prompt)) =
                self.attempt_job_for_claim(session, backend.as_ref(), network_domain, job.accepted_block, job.class_id, &producer)
            else {
                if let Some(case) = book.cases.get_mut(&claim) {
                    case.input_retry_at = current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                }
                return;
            };
            (ctx, prompt, None)
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
                    book.cases.get_mut(&claim).expect("held above").input_retry_at =
                        current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    crate::palw_backends::note_throttled_v1("panel-fraud-filer-ledger", || {
                        format!("[{PALW_PANEL}] claim {claim}: the fraud filer's replay waits: {why} (LG14-A)")
                    });
                    return;
                }
            };
        // Canonical derivation and replay input copies hold the full ticket. Acquired user ids
        // were already authenticated under the small ticket, off the loop, or by the DA learner.
        let fp_job = if let Some(payload) = fp_payload {
            let acquired = book.cases.get(&claim).and_then(|c| c.fp_bootstrap.as_ref()).filter(|input| input.ready());
            let input_result = if let Some(input) = acquired {
                Ok(input::PalwFraudFilerFpInputV1 { job: payload.job().clone(), prompt_ids: input.ids.clone() })
            } else {
                input::palw_fraud_filer_fp_descriptor_input_v1(backend.as_ref(), &payload, prompt_form, public_material)
            };
            let input = match input_result {
                Ok(input) => input,
                Err(why) => {
                    if let Some(case) = book.cases.get_mut(&claim) {
                        case.input_retry_at = current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    }
                    if ((payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL
                        && matches!(
                            payload.job().version,
                            kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION
                                | kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V4_VERSION
                        ))
                        || payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER)
                        && self.consensus_config.params.palw_legacy_held_da_v2_active_at(current_daa)
                    {
                        self.fraud_filer_hold_fp_input_v1(book, claim, payload, prompt_form);
                    }
                    debug!("[{PALW_PANEL}] claim {claim}: the fraud filer's authenticated public FP input waits: {why}");
                    return;
                }
            };
            prompt = input.prompt_ids.into_iter().map(|id| id as usize).collect();
            Some(input.job)
        } else {
            None
        };
        if prepared_only {
            book.cases.get_mut(&claim).expect("held above").fp_bootstrap = None;
        }
        let Ok(prompt32) = prompt.iter().map(|id| u32::try_from(*id)).collect::<Result<Vec<u32>, _>>() else {
            return unjudged(book, "a prompt id past u32".into());
        };
        info!("[{PALW_PANEL}] claim {claim}: the fraud filer replays the claim's job off the loop (LG14-A, RFC-0014 §6.1)");
        let attempt_draw = self.attempt_draw_for_claim(session, job.accepted_block);
        let handle = tokio::task::spawn_blocking(move || {
            palw_fraud_filer_execute_v1(backend, ctx, prompt32, prompt_form, attempt_draw, fp_job, Some(reserved))
        });
        book.running = Some(PalwFraudFilerReplayV1 { candidate, handle });
    }
}

#[cfg(test)]
pub(super) mod tests {
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

    pub(in crate::palw_panel) fn candidate_for_input_test() -> PalwFraudFilerCandidateV1 {
        candidate(1, 100, false)
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
            public_fp_job: None,
            work_leaves: 0,
            deadline_daa: None,
            hard_deadline_daa: 3_800,
            record: None,
            sessions: Vec::new(),
            answered: Vec::new(),
            flat_answered_rows: 0,
            open_courts: 0,
            executor_refuted: false,
            court_convicted: false,
            da_defaulted: false,
        }
    }

    #[test]
    fn waiting_public_input_does_not_spend_a_run_or_block_another_pending_claim() {
        let mut book = PalwFraudFilerBookV1::default();
        let first = candidate(1, 100, false);
        let second = candidate(2, 101, false);
        book.refresh(vec![first.clone(), second.clone()], 100);
        book.cases.get_mut(&first.claim_id).unwrap().input_retry_at = 130;
        assert_eq!(book.next_replay_at(100), Some(second.claim_id));
        assert_eq!(book.next_replay_at(129), Some(second.claim_id));
        assert_eq!(book.next_replay_at(130), Some(first.claim_id));
        let case = &book.cases[&first.claim_id];
        assert_eq!(case.runs, 0);
        assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Pending));
        let mut changed = first.clone();
        changed.job.accepted_block = Hash64::from_u64_word(999);
        book.refresh(vec![changed, second], 110);
        assert_eq!(book.next_replay_at(110), Some(first.claim_id), "a new job clears its predecessor's wait");
    }

    #[test]
    fn waiting_bootstrap_is_pursued_without_holding_a_replay_slot_and_reorg_forgets_input() {
        let mut book = PalwFraudFilerBookV1::default();
        let a = candidate(1, 100, false);
        let b = candidate(2, 101, false);
        book.refresh(vec![a.clone(), b.clone()], 100);
        let payload = Arc::new(input::tests::payload());
        book.cases.get_mut(&a.claim_id).unwrap().fp_bootstrap = Some(PalwFraudFilerFpBootstrapV1 {
            form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            payload: Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(payload.as_ref().clone())),
            ids: vec![],
            chunk: 0,
            _reservation: None,
        });
        assert_eq!(book.pursued(), vec![a.claim_id]);
        assert_eq!(book.held_runs(), 0);
        assert_eq!(book.next_replay_at(100), Some(b.claim_id));
        assert_eq!(book.walk_floor(), Some(a.accepted_daa));
        let input = book.cases.get_mut(&a.claim_id).unwrap().fp_bootstrap.as_mut().unwrap();
        input.ids = payload.prompt_token_ids.clone();
        input.chunk = 1;
        assert_eq!(book.next_replay_at(100), Some(a.claim_id));
        book.walked(PalwFraudFilerWalkV1 { floor: 100, anchor: Hash64::from_u64_word(9), anchor_daa: 150, next: None }, true, vec![]);
        let case = &book.cases[&a.claim_id];
        assert_eq!(case.runs, 0);
        assert!(case.fp_bootstrap.as_ref().unwrap().ids.is_empty());
        assert_eq!(case.fp_bootstrap.as_ref().unwrap().chunk, 0);
        assert_eq!(book.next_replay_at(150), Some(b.claim_id));
        let mut changed = a.clone();
        changed.job.accepted_block = Hash64::default();
        book.refresh(vec![changed, b], 151);
        assert!(book.cases[&a.claim_id].fp_bootstrap.is_none());
        assert_eq!(book.next_replay_at(151), Some(a.claim_id));
    }

    #[test]
    fn public_fp_replay_opens_its_own_constraint_mask_and_refuses_unavailable_tables() {
        use kaspa_consensus_core::palw_decode_constraint_v1::*;
        use kaspa_consensus_core::palw_fp_constraint_job_v1::*;
        use kaspa_consensus_core::palw_freeprompt_v3::PalwFpJobTailV1;
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let (mut job, ids) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
        let constraint = PalwDecodeConstraintV1 {
            version: 1,
            compiler_id: Hash64::from_u64_word(0xC014),
            start_frame: 0,
            frames: vec![PalwConstraintFrameV1 {
                start: 0,
                nodes: vec![PalwConstraintNodeV1 {
                    accepting: true,
                    edges: vec![PalwConstraintEdgeV1 { lo: b'a', hi: b'a', action: PalwConstraintActionV1::Goto(0) }],
                }],
            }],
        };
        let vocab = misaka_palw_base0::rc::palw_rc_base0_shape_v1().vocab;
        let mut table = PalwTokenTableV1 {
            entries: vec![Some(b"b".to_vec()); vocab],
            eog_token_ids: vec![(vocab - 1) as u32],
            tokenizer_id: job.tokenizer_id,
        };
        table.entries[0] = Some(b"a".to_vec());
        let root = palw_token_tables_v1().register(table);
        job.version = PALW_FP_CONSTRAINT_VERSION;
        job.decode = None;
        job.tail = Some(PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 { constraint: constraint.to_bytes(), table_root: root }));
        let prompt: Vec<_> = ids.iter().map(|id| *id as usize).collect();
        assert!(backend.execute_free_prompt(&job, &prompt).is_err(), "a constrained run requires its host scope");
        let expected = palw_fp_with_constraint_scope_v1(
            palw_fp_constraint_mask_for_host_v1(&job, backend.token_table_for_job_v1(&job)).unwrap(),
            || backend.execute_free_prompt(&job, &prompt),
        )
        .unwrap();
        assert!(expected.output_token_ids.iter().all(|id| *id == 0), "the constraint permits only lane zero");
        let ctx = backend.fp_job_context_for_executed_v1(&job, expected.facts.decode_tokens_executed).unwrap();
        let replay = |job| {
            palw_fraud_filer_execute_v1(
                Box::new(crate::palw_panel::seat_s_tests::floor_backend()),
                ctx.clone(),
                ids.clone(),
                backend.prompt_ids_form(),
                None,
                Some(job),
                None,
            )
        };
        let actual = replay(job.clone()).expect("the outside verifier holds the public table and its own registered model");
        assert_eq!(actual.execution_root, expected.outcome.execution_root);
        assert_eq!(actual.trace_root, expected.outcome.trace_root);
        assert_eq!(actual.output_root, expected.outcome.output_root);
        assert_eq!(actual.fp_job.as_deref(), Some(&job), "a mismatched run can later build its public job proof");
        assert!(actual._job_reservation.is_some(), "the retained descriptor holds its memory ticket");
        let c = candidate(0xC014, 300, false);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 300);
        assert!(matches!(book.judge(c.claim_id, Ok(actual)), Some(PalwFraudFilerVerdictV1::Mismatch(_))));
        assert!(book.cases[&c.claim_id].fp_bootstrap.is_none());
        assert!(book.cases[&c.claim_id].fp_descriptor.is_none());
        assert_eq!(book.cases[&c.claim_id].public_fp_job_v1(), Some(&job), "successful replay cleanup retains the prosecution job");
        assert!(palw_fp_constraint_scope_v1().is_none(), "the replay restores its thread scope");
        let mut wrong_tokenizer = job.clone();
        wrong_tokenizer.tokenizer_id = Hash64::from_u64_word(0xBAD_C015);
        assert!(replay(wrong_tokenizer).is_err(), "a held table from another tokenizer cannot drive a judgement");
        let Some(PalwFpJobTailV1::Constraint(tail)) = job.tail.as_mut() else { unreachable!() };
        tail.table_root = Hash64::from_u64_word(0xBAD_C014);
        assert!(replay(job).is_err(), "an unavailable table is a refusal, never a judgement");
        assert!(palw_fp_constraint_scope_v1().is_none());
    }

    #[test]
    fn input_bootstrap_keeps_binding_proofs_reachable_even_when_all_input_is_public() {
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let payload = Arc::new(input::tests::payload());
        let job = &payload.commitment.job;
        let executed =
            backend.execute_free_prompt(job, &payload.prompt_token_ids.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        let binding = misaka_palw_base0::produce::base0_material_decode_any_v1(&executed.outcome.material).unwrap().binding().clone();
        let me = bond(1);
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        case.fp_bootstrap = Some(PalwFraudFilerFpBootstrapV1 {
            payload: Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(payload.as_ref().clone())),
            form: backend.prompt_ids_form(),
            ids: payload.prompt_token_ids.clone(),
            chunk: 0,
            _reservation: None,
        });
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 300 });
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 300, deposit: 1, sessions_opened: 0, lanes_joined: 0 });
        v.record = Some(record);
        let step = |case: &PalwFraudFilerCaseV1| {
            palw_fraud_filer_bootstrap_step_v1(PalwFilerRoleV1::PublicBond, Some(&v), &me, true, case, |_| false)
        };
        assert_eq!(
            step(&case),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(PalwLegacyProbeV1::Binding { row: 0, tile: 0 })),
            "a failed ceiling/context must still be able to obtain a binding for direct job proofs"
        );
        case.binding = Some(binding);
        assert_eq!(
            step(&case),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait),
            "available input must not cause redundant PromptIds obligations"
        );
    }

    #[test]
    fn bootstrap_verdicts_release_only_after_honesty_or_final_host_failure_and_reset_on_job_change() {
        let c = candidate(3, 300, false);
        for fail in [false, true] {
            let mut book = PalwFraudFilerBookV1::default();
            book.refresh(vec![c.clone()], 300);
            book.cases.get_mut(&c.claim_id).unwrap().fp_bootstrap = Some(PalwFraudFilerFpBootstrapV1 {
                payload: Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(input::tests::payload())),
                form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                ids: vec![],
                chunk: 0,
                _reservation: None,
            });
            if fail {
                assert!(matches!(book.judge(c.claim_id, Err("host failure".into())), Some(PalwFraudFilerVerdictV1::Pending)));
                assert!(!book.cases[&c.claim_id].release_pending);
                assert!(book.cases[&c.claim_id].fp_bootstrap.is_some());
                book.judge(c.claim_id, Err("host failure again".into()));
                assert!(matches!(book.cases[&c.claim_id].verdict, PalwFraudFilerVerdictV1::Unjudged(_)));
            } else {
                book.judge(c.claim_id, Ok(run(c.job.execution_root, c.job.trace_root)));
                assert!(matches!(book.cases[&c.claim_id].verdict, PalwFraudFilerVerdictV1::Honest));
            }
            assert!(book.cases[&c.claim_id].release_pending);
            assert_eq!(book.pursued(), vec![c.claim_id]);
            assert!(
                book.cases[&c.claim_id].fp_bootstrap.is_none(),
                "the input ticket is relinquished before releasing the chain reservation"
            );
            let mut new = c.clone();
            new.job.accepted_block = Hash64::from_u64_word(999);
            book.refresh(vec![new], 301);
            assert!(!book.cases[&c.claim_id].release_pending, "old replay cannot release a different branch's reservation");
        }
    }

    #[tokio::test]
    async fn a_missing_fp_body_can_be_probed_while_the_local_capture_lane_is_full() {
        let mut book = PalwFraudFilerBookV1::default();
        let first = candidate(1, 100, false);
        let second = candidate(2, 101, false);
        let mut fp = candidate(3, 102, false);
        fp.job.free_prompt = true;
        let attempt = candidate(4, 103, false);
        book.refresh(vec![first.clone(), second.clone(), fp.clone(), attempt.clone()], 100);
        book.judge(first.claim_id, Ok(run(Hash64::default(), Hash64::default())));
        book.judge(second.claim_id, Ok(run(Hash64::default(), Hash64::default())));
        assert_eq!(book.held_runs(), PALW_FRAUD_FILER_HELD_RUNS_V1);
        assert_eq!(book.next_replay_at(100), Some(fp.claim_id), "a public binding proof does not need a capture slot");
        book.cases.get_mut(&fp.claim_id).unwrap().input_retry_at = 130;
        assert_eq!(book.next_replay_at(129), None, "busy probes neither bypass their retry nor execute the pending attempt");
        assert_eq!(book.next_replay_at(130), Some(fp.claim_id));
        assert_eq!(book.cases[&fp.claim_id].runs, 0);
        book.settle(&first.claim_id, PalwFraudFilerVerdictV1::Unjudged("done".into()));
        let mut other_fp = candidate(5, 104, false);
        other_fp.job.free_prompt = true;
        book.refresh(vec![first, second, fp.clone(), attempt, other_fp.clone()], 130);
        book.running =
            Some(PalwFraudFilerReplayV1 { candidate: fp.clone(), handle: tokio::spawn(async { Err("unused worker result".into()) }) });
        assert_eq!(
            book.next_probe_at(130, false),
            Some(other_fp.claim_id),
            "seat pressure cannot stop metadata probing, and the running acceptance is not probed twice"
        );
        assert!(book.running.take().unwrap().handle.await.unwrap().is_err());
    }

    #[test]
    fn binding_only_fp_bootstrap_authenticates_without_a_replay_and_keeps_its_ticket_until_settlement() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        let payload = input::tests::payload();
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let executed = backend
            .execute_free_prompt(&payload.commitment.job, &payload.prompt_token_ids.iter().map(|id| *id as usize).collect::<Vec<_>>())
            .unwrap();
        let event = backend.disclose_trace_event(&executed.outcome.material, 0, 0).unwrap();
        let mut c = candidate(3, 300, false);
        c.job.free_prompt = true;
        c.job.class_id = payload.commitment.job.class_id;
        c.producer = PalwBondKeyV2(payload.commitment.job.executor_bond);
        c.job.execution_root = payload.commitment.execution_root;
        c.job.trace_root = payload.commitment.trace_root;
        c.job.output_root = payload.commitment.output_root;
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(4096), || None);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 300);
        let case = book.cases.get_mut(&c.claim_id).unwrap();
        case.fp_job_pending = true;
        case._fp_job_reservation = Some(Arc::new(
            ledger
                .reserve(PalwMemoryReservationKeyV1 { role: "binding-test", class_id: c.job.class_id, job: c.claim_id }, 2048)
                .unwrap(),
        ));
        assert!(case.pursued());
        assert!(case.fp_bootstrap.is_none());
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 300 });
        v.execution_root = c.job.execution_root;
        v.trace_root = c.job.trace_root;
        let me = bond(1);
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 300, deposit: 1, sessions_opened: 0, lanes_joined: 0 });
        v.record = Some(record);
        let probe = PalwLegacyProbeV1::Binding { row: 0, tile: 0 };
        assert_eq!(
            palw_fraud_filer_bootstrap_step_v1(PalwFilerRoleV1::PublicBond, Some(&v), &me, true, case, |_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe))
        );
        let mut bad = event.clone();
        match &mut bad {
            kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Flat { binding, .. }
            | kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Tiled { binding, .. }
            | kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::OutOfRange { binding } => {
                binding.job_context.job_id = Hash64::default()
            }
        }
        assert!(case.learn(probe, &PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(bad))).is_err());
        assert!(case.binding.is_none());
        case.learn(probe, &PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event))).unwrap();
        assert_eq!(case.runs, 0);
        assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Pending));
        assert_eq!(
            palw_fraud_filer_bootstrap_step_v1(PalwFilerRoleV1::PublicBond, Some(&v), &me, true, case, |_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait),
            "a correct binding alone cannot invent the missing full job"
        );
        let binding = case.binding.clone().unwrap();
        case.fp_expected_pin = Some(kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&payload.commitment));
        case.fp_job_disclosure_enabled = true;
        let job_probe = PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::FpJob };
        assert_eq!(
            palw_fraud_filer_bootstrap_step_v1(PalwFilerRoleV1::PublicBond, Some(&v), &me, true, case, |_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(job_probe))
        );
        let mut wrong = payload.commitment.job.clone();
        wrong.job_nonce[0] ^= 1;
        assert!(
            case.learn(
                job_probe,
                &PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((binding.clone(), PalwLegacyHeldAnswerV2::FpJob(Box::new(wrong)))))
            )
            .is_err()
        );
        assert!(case.fp_descriptor.is_none());
        let mut owned = case.clone();
        owned
            .learn(
                job_probe,
                &PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((
                    binding,
                    PalwLegacyHeldAnswerV2::FpJob(Box::new(payload.commitment.job.clone())),
                ))),
            )
            .unwrap();
        assert!(case.fp_descriptor.is_none(), "the learner retains a separate state copy");
        case.accept_learned_v1(owned);
        assert!(matches!(case.fp_descriptor.as_deref(), Some(input::PalwFraudFilerFpDescriptorV1::Disclosed { .. })));
        assert!(case.fp_bootstrap.is_none(), "input memory is reserved later, before acquiring ids");
        assert_eq!(case.runs, 0);
        let mut own = run(c.job.execution_root, c.job.trace_root);
        own.output_root = c.job.output_root;
        assert!(matches!(book.judge(c.claim_id, Ok(own)), Some(PalwFraudFilerVerdictV1::Honest)));
        assert!(book.cases[&c.claim_id].release_pending, "a job later recovered must still release its earlier binding reservation");
        assert!(!book.cases[&c.claim_id].fp_job_pending);
        assert!(book.cases[&c.claim_id].fp_descriptor.is_none());
        assert_eq!(ledger.reserved_bytes(), 0);
    }

    /// A run whose roots are `execution_root` / `trace_root`, opening no range (the tests never read one).
    fn run(execution_root: Hash64, trace_root: Hash64) -> PalwFraudFilerRunV1 {
        PalwFraudFilerRunV1 {
            execution_root,
            trace_root,
            output_root: Hash64::default(),
            own_range: Box::new(|_, _| Err("no range".into())),
            legacy: None,
            own: None,
            fp_job: None,
            _job_reservation: None,
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

    #[test]
    fn another_acceptance_restarts_an_honest_or_mismatched_case_and_rejects_old_results() {
        let old = candidate(3, 300, false);
        let claim = old.claim_id;
        for mismatch in [false, true] {
            let mut book = PalwFraudFilerBookV1::default();
            book.refresh(vec![old.clone()], 10);
            let result =
                if mismatch { run(Hash64::default(), old.job.trace_root) } else { run(old.job.execution_root, old.job.trace_root) };
            book.judge_replay(&old, Ok(result)).expect("current job");
            let old_capture = match &book.cases[&claim].verdict {
                PalwFraudFilerVerdictV1::Mismatch(run) => Some(Arc::downgrade(run)),
                _ => None,
            };
            let case = book.cases.get_mut(&claim).unwrap();
            case.frontiers.push(PalwLegacyFrontierV2 {
                level: 1,
                index: 0,
                below: 0,
                first: 0,
                nodes: vec![Hash64::from_u64_word(1)],
                leaf_hashes: None,
            });
            case.bisect = PalwLegacyBisectV1::new(64);
            case.legacy_wanted = Some(PalwLegacyHeldUnitV2::StepNode { level: 1, index: 0 });
            case.note_sent(PalwFraudFilerSentV1::Reserve, 10);
            case.direct_handed = 2;
            case.direct_asked_at = Some(10);
            let page = PalwFraudFilerWalkV1 { floor: 300, anchor: Hash64::from_u64_word(1000), anchor_daa: 1000, next: None };
            book.walked(page, false, vec![]);
            assert!(book.refresh(vec![old.clone()], 11).is_empty(), "an unchanged candidate retains its replay");
            assert_eq!(book.cases[&claim].frontiers.len(), 1);
            let mut new = old.clone();
            new.job.accepted_block = Hash64::from_u64_word(2000); // Same claim and all three roots, a different header-derived job.
            assert_eq!(book.refresh(vec![new.clone()], 12), BTreeSet::from([claim]));
            let case = &book.cases[&claim];
            assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Pending));
            assert!(case.frontiers.is_empty());
            assert_eq!(
                (case.bisect, case.legacy_wanted, case.sent, case.runs, case.direct_handed, case.direct_asked_at),
                (PalwLegacyBisectV1::new(0), None, None, 0, 0, None)
            );
            assert_eq!((book.walk, book.walked_from, book.walked_to), (None, None, None));
            if let Some(capture) = old_capture {
                assert!(capture.upgrade().is_none(), "old own capture is released");
            }
            // Even matching roots in a late old result cannot make the new header's job Honest; an old failure cannot spend a retry.
            assert!(book.judge_replay(&old, Ok(run(old.job.execution_root, old.job.trace_root))).is_none());
            assert!(book.judge_replay(&old, Err("an old worker's failure".into())).is_none());
            assert_eq!(book.cases[&claim].runs, 0);
            assert_eq!(book.next_replay(), Some(claim));
            assert!(matches!(
                book.judge_replay(&new, Ok(run(Hash64::default(), new.job.trace_root))),
                Some(PalwFraudFilerVerdictV1::Mismatch(_))
            ));
            assert_eq!(book.walk_floor(), Some(300), "new job's mismatch backfills from acceptance");
        }
    }

    #[tokio::test]
    async fn an_invalidated_blocking_replay_drains_under_its_reservation_before_the_next_starts() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        for clear in [false, true] {
            let old = candidate(3, 300, false);
            let claim = old.claim_id;
            let mut new = old.clone();
            new.job.accepted_block = Hash64::from_u64_word(2000);
            let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(4096), || None);
            let reserved = ledger
                .reserve(
                    PalwMemoryReservationKeyV1 { role: PALW_FRAUD_FILER_REPLAY_ROLE_V1, class_id: old.job.class_id, job: claim },
                    2048,
                )
                .expect("reservation");
            let (started, ready) = tokio::sync::oneshot::channel();
            let (finish, wait) = std::sync::mpsc::channel();
            let job = old.job.clone();
            let handle = tokio::task::spawn_blocking(move || {
                started.send(()).expect("observer");
                wait.recv().expect("continue worker");
                let mut result = run(job.execution_root, job.trace_root);
                result._reservation = Some(reserved);
                Ok(result)
            });
            let mut book = PalwFraudFilerBookV1::default();
            book.refresh(vec![old.clone()], 10);
            book.running = Some(PalwFraudFilerReplayV1 { candidate: old, handle });
            tokio::time::timeout(std::time::Duration::from_secs(5), ready).await.expect("worker starts").expect("started");
            if clear {
                book.clear();
            }
            book.refresh(vec![new], 11);
            assert!(book.running.is_some(), "clear/invalidation must not detach a blocking worker");
            assert_eq!(book.next_replay(), None, "one worker remains in flight");
            assert_eq!(ledger.reserved_bytes(), 2048, "the live worker's memory stays reserved");
            finish.send(()).expect("finish old worker");
            let replay = book.running.take().expect("still observed");
            let result =
                tokio::time::timeout(std::time::Duration::from_secs(5), replay.handle).await.expect("worker drains").expect("joined");
            assert_eq!(ledger.reserved_bytes(), 2048, "the returned capture still holds its reservation");
            assert!(book.judge_replay(&replay.candidate, result).is_none());
            assert_eq!(ledger.reserved_bytes(), 0, "discarding the stale capture releases it");
            assert_eq!(book.next_replay(), Some(claim));
            assert!(matches!(book.cases[&claim].verdict, PalwFraudFilerVerdictV1::Pending));
        }
    }

    #[test]
    fn restarting_public_history_discards_old_localization_and_resend_state_but_keeps_the_same_jobs_replay() {
        let c = candidate(3, 300, false);
        let claim = c.claim_id;
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 10);
        book.judge_replay(&c, Ok(run(Hash64::default(), c.job.trace_root)));
        let case = book.cases.get_mut(&claim).unwrap();
        case.legacy_wanted = Some(PalwLegacyHeldUnitV2::StepNode { level: 1, index: 0 });
        case.note_sent(PalwFraudFilerSentV1::Reserve, 10);
        case.note_sent(PalwFraudFilerSentV1::Reserve, 40);
        assert_eq!(case.may_send(PalwFraudFilerSentV1::Reserve, 70), Err(()));
        let page = PalwFraudFilerWalkV1 { floor: 300, anchor: Hash64::from_u64_word(2000), anchor_daa: 1000, next: Some(claim) };
        book.walked(page, true, vec![]);
        let case = &book.cases[&claim];
        assert!(case.pursued(), "unchanged job's own replay is still valid");
        assert_eq!((case.legacy_wanted, case.sent), (None, None));
        assert_eq!(case.may_send(PalwFraudFilerSentV1::Reserve, 70), Ok(true), "new branch can reserve afresh");
        assert_eq!(book.walk_floor(), Some(300));
    }

    #[test]
    fn unrelated_settled_case_cleanup_cannot_restart_a_pursuits_history_and_a_new_seat_leaves_the_public_lane() {
        let c = candidate(3, 300, false);
        let claim = c.claim_id;
        let other = candidate(4, 400, false);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone(), other.clone()], 10);
        book.judge_replay(&c, Ok(run(Hash64::default(), c.job.trace_root)));
        book.judge_replay(&other, Ok(run(other.job.execution_root, other.job.trace_root)));
        let page = PalwFraudFilerWalkV1 {
            floor: 300,
            anchor: Hash64::from_u64_word(1000),
            anchor_daa: 1000,
            next: Some(Hash64::from_u64_word(600)),
        };
        book.walked(page, false, vec![]);
        assert_eq!(book.refresh(vec![c.clone()], 11), BTreeSet::from([other.claim_id]));
        assert_eq!(book.walk, Some(page), "unrelated expired/honest claims cannot continually rewind another pursuit");
        let mut seat = c.clone();
        seat.seat = true;
        assert_eq!(book.refresh(vec![seat], 12), BTreeSet::from([claim]));
        assert!(book.pursued().is_empty());
        assert_eq!(book.next_replay(), None, "the new Panel seat's replay belongs to its seat lane");
        assert!(book.judge_replay(&c, Ok(run(Hash64::default(), c.job.trace_root))).is_none());
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
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 151, deposit: 1, sessions_opened: 0, lanes_joined: 0 });
        v.record = Some(record);
        let binding = PalwLegacyProbeV1::Binding { row: 0, tile: 0 };
        assert_eq!(step(&v, false, false), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(binding)));
        v.answered.push(binding.unit());
        assert_eq!(step(&v, false, false), PalwFraudFilerStepV1::AwaitAnswer(binding));
        assert_eq!(step(&v, false, true), PalwFraudFilerStepV1::Learn(binding));
        v.answered.clear();
        v.flat_answered_rows = 1;
        assert_eq!(
            step(&v, false, false),
            PalwFraudFilerStepV1::AwaitAnswer(binding),
            "historical Flat coverage also waits for public bytes"
        );
        assert!(!v.unit_was_answered_v1(&PalwDaUnitV1::Event { row: 0, tile: 1 }), "Flat never covers another tile");
        assert!(!v.unit_was_answered_v1(&PalwDaUnitV1::Event { row: 1, tile: 0 }), "Flat never covers a row past its bound");
        let mut fresh = book.cases[&claim].clone();
        assert!(fresh.reacquire_after_v1(&v, me, binding, 200).is_none());
        assert!(fresh.reacquire_after_v1(&v, me, binding, 230).is_some(), "historical Flat coverage uses numbered reacquisition");
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
        // G14-V: only the ONE unit the case's descent reads now is cached — any position, never a stranger's other demands.
        let wanted = range(1 << 30, 1_024);
        for first in [0u64, 1_024, 1 << 30, (1 << 30) + 1_024] {
            for count in [1, 1_024] {
                assert_eq!(palw_fraud_filer_cache_unit_v1(&range(first, count), Some(&wanted)), range(first, count) == wanted);
            }
        }
        assert!(!palw_fraud_filer_cache_unit_v1(&range(0, 1_024), None));
        let event = PalwDaUnitV1::Event { row: 3, tile: 7 };
        assert!(palw_fraud_filer_cache_unit_v1(&event, Some(&event)));
        assert!(!palw_fraud_filer_cache_unit_v1(&PalwDaUnitV1::Event { row: 3, tile: 8 }, Some(&event)));
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
