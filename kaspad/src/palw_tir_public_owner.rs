//! **G14 R4: the outside verifier's acquisition driver and fault localization — the findings feed of the public filer** (node
//! policy; a child module of `palw_fraud_filer`, dormant below `palw_tir_public_conviction_v1` — there the book is emptied and
//! nothing is queued).
//!
//! A bond outside a frame's liable set and drawn Panel pages through the accepted ORIGINAL frames and, for a class whose model
//! this node holds and whose accepted prompt ids the chain kept, replays the original job independently
//! (`open_tir_own_step_source_v1`). A replay that agrees with the committed step root and output root opens nothing. One that
//! does not becomes a case:
//!
//! 1. **Open** an owner acquisition on the frame (its own rent and deposit, never the producer's pool; K_owner bounds owners).
//! 2. **Acquire and localize** — one `Demand` at a time against the frame's literal custodians; each answer is read back only
//!    from the ACCEPTED rooted chunks and re-authenticated against the original roots
//!    (`palw_tir_public_finished_material_v1`). A step-root disagreement descends the committed step tree with authenticated
//!    `StepNode` frontiers against the replay (`next_public_from_source_v1`) to one leaf, then probes the public sparse cone
//!    builder, demanding exactly the step ranges it names, until it returns a built primitive. An output-root disagreement (or a
//!    stop-rule question) demands the complete original IDs to their outer EOF.
//! 3. **Collect** a demand the custodians left unanswered past its own response clock (the DA default — itself a sanction).
//! 4. **Feed the filer** — a built cone/logits primitive, or a complete authenticated ID sequence that contradicts the committed
//!    output root or the job's stop rule, enters `PalwTirPublicFilerBookV1::queue_finding_v1`; the filer scheduler dry-runs the
//!    fold, files inside the frame's d0 door, and the fold convicts and slashes. No manual step.
//!
//! Budgets: the native ones the fold enforces (owner terms, turns, logical units, response spans), plus this node's own: two
//! cases at once, a bounded number of demands per case, the replay's quoted RAM held for the case's life, one probe per tick.
//! Restart: cases are process memory (the replay is rerun, accepted answers are re-read from the chain); the filer's journal holds
//! every finding already produced.
//!
//! **G14 close-out (2026-10-16):**
//! - **G-T2, selection faults are localized.** Whenever the replay's trace or step root differs, the case first acquires the
//!   complete committed IDs (EOF-authenticated) and compares them with the replay's own IDs. At the first position they differ it
//!   demands that row's trace tile(s) and builds the tiled decode-token pin under the ORIGINAL job's own rule (greedy, seeded,
//!   V4) with consensus-core's `palw_tir_public_selection_probe_v1`; a pin that refutes the committed token is the finding. A row
//!   that does not refute it means the committed logits differ there — the case falls back to the arithmetic descent. A replay
//!   whose roots all agree is still checked against the job's stop rule (Output door).
//! - **G-T4, an honest drawn seat files through the same path.** A drawn seat that holds no Valid custody grant is an independent
//!   owner (the fold's rule), so it is no longer skipped; and a claim this node's seat already found faulty (`seat_faults`) is
//!   pursued first, ahead of the page cursor. Its finding enters the same filer book.
//! - **G-T1:** a frame at K_owner is still pursued when a slot holder is idle (the Open reclaims that slot).
//!
//! **Not covered:** a class with no kept prompt ids is not replayed (the frame keeps them since G-T3); a trace-only disagreement
//! with equal IDs and equal steps is logged.
#![allow(dead_code)] // the panel loop calls the tick; the finding rule is consensus-core's (tested there).

use super::super::{COURT_MOVE_REPLAN_DAA, PalwPanelService};
use super::tir_public_filer::palw_tir_public_arithmetic_evidence_v1;
use super::tir_public_filer_scheduler::PalwTirPublicFilerBookV1;
use kaspa_consensus_core::{
    Hash64,
    palw_backend::{PalwExecutionBackendV1, PalwTirOwnStepSourceV1, PalwTirPublicConeNeedV1, PalwTirPublicConeProbeV1},
    palw_court_v2::PalwCourtVerdictProofV2,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2},
    palw_tir_public_carriage_v1::{
        PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1, PalwTirPublicControlActionV1, PalwTirPublicControlTargetV1, PalwTirPublicControlV1,
        PalwTirPublicConvictionEvidenceV1,
    },
    palw_tir_public_claim_v1::palw_tir_public_claim_v1,
    palw_tir_public_owned_v1::{PalwTirPublicFinishedV1, PalwTirPublicOwnedActionV1},
    palw_tir_public_source_v1::{
        PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1, PalwTirPublicAuthenticatedIdsV1, PalwTirPublicAuthenticatedUnitV1, PalwTirPublicRequestV1,
        PalwTirPublicResponseV1, PalwTirPublicSourceV1,
    },
};
use kaspa_core::{debug, info};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

const PALW_PANEL: &str = "palw-panel";

/// The court queue's lane of an outside owner's moves on one frame (Open, Demand, Collect), keyed by claim (free: `u32::MAX − 28`).
pub(crate) const PALW_TIR_PUBLIC_OWNER_ROUND_V1: u32 = u32::MAX - 28;
/// Frames pursued at once (each holds one independent replay).
const PALW_TIR_PUBLIC_OWNER_CASES_V1: usize = 2;
/// Demands one case may issue (on top of the native turn and logical-unit budgets).
const PALW_TIR_PUBLIC_OWNER_DEMANDS_V1: u32 = 256;
/// A frame judged not worth pursuing (agreeing replay, no model, no kept input) is looked at again after this many DAA.
const PALW_TIR_PUBLIC_OWNER_SKIP_DAA_V1: u64 = 3_600;
/// The sparse cone builder's per-probe allocation (operational, not a native certificate).
const PALW_TIR_PUBLIC_OWNER_COPY_BYTES_V1: u64 = 64 << 20;
const PALW_TIR_PUBLIC_OWNER_TARGETS_V1: u64 = 4_096;
const PALW_TIR_PUBLIC_OWNER_SOURCE_READS_V1: u64 = 1 << 20;

/// What the independent replay disagrees with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SuspicionV1 {
    /// The committed step tree differs from the replay's: localize to a leaf and build its cone/logits primitive.
    Arithmetic,
    /// Steps agree but the committed output does not (or the stop rule is in question): read the complete original IDs.
    Output,
    /// **G-T2:** the trace (or the step tree) differs: read the complete committed IDs first; a first differing position is a
    /// selection candidate (its row's tiles → the decode-token pin); equal IDs fall to `Arithmetic` (steps differ) or `Output`.
    Selection,
}

/// **G14 G-T2: what the replay says about a frame, before any acquisition** — `None` when nothing disagrees (the trace, the step
/// tree and the output root all equal the committed ones and the replay's answer obeys the job's own stop rule).
pub(crate) fn owner_suspicion_v1(
    committed: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    committed_output_root: Hash64,
    job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    vocab: u32,
    replay: &kaspa_consensus_core::palw_backend::PalwTirOwnStepSourceInfoV1,
) -> Option<SuspicionV1> {
    let steps = replay.header.step_merkle_root != committed.step_merkle_root || replay.header.step_leaf_count != committed.step_leaf_count;
    if steps || replay.header.full_logits_trace_root != committed.full_logits_trace_root {
        return Some(SuspicionV1::Selection);
    }
    if replay.output_root != committed_output_root
        || kaspa_consensus_core::palw_tir_objective_v1::original_stop_rule_fault_v1(job, vocab, &replay.generated_ids).is_some()
    {
        return Some(SuspicionV1::Output);
    }
    None
}

/// **G14 G-T4 / G-T1: may this bond start a case on a frame?** Not when it is liable on the claim (the producer or a credited
/// Valid custodian — a drawn seat that signed no Valid is independent), not before the original preparation door or once the
/// conviction door closed, and not when the frame has no slot for it (unless it already owns there).
pub(crate) fn owner_may_pursue_v1(
    view: &kaspa_consensus_core::palw_tir_public_claim_v1::PalwTirPublicFrameViewV1,
    preparation_until_daa: u64,
    signer: &PalwBondKeyV2,
    now: u64,
    slot_open: bool,
    owns_here: bool,
) -> bool {
    !view.liable.contains(signer) && now >= preparation_until_daa && now < view.collect_until_daa && (slot_open || owns_here)
}

/// **Lane g14-court-capacity-r2: may this bond Open now as far as the K_owner cap goes** — always past `palw_court_capacity_v1`
/// (`fair_start`: an Open at the cap waits in the owner queue instead of being refused), else only on a free or reclaimable slot.
pub(crate) fn owner_slot_open_v1(fair_start: bool, slot_free_or_reclaimable: bool) -> bool {
    fair_start || slot_free_or_reclaimable
}

/// **G14 G-T4: the order frames are considered in** — the claims this node's own seat found faulty first (its Panel replay
/// already disagrees), then the page of accepted frames; each once.
pub(crate) fn owner_discovery_order_v1(seat_faulted: &[Hash64], page: &[Hash64], is_frame: impl Fn(&Hash64) -> bool) -> Vec<Hash64> {
    let mut order: Vec<Hash64> = Vec::new();
    for claim in seat_faulted.iter().filter(|c| is_frame(c)).chain(page.iter()) {
        if !order.contains(claim) {
            order.push(*claim);
        }
    }
    order
}

/// One frame this node pursues as an outside owner (process memory).
struct OwnerCaseV1 {
    claim: Hash64,
    allocation_id: Hash64,
    source_id: Hash64,
    suspicion: SuspicionV1,
    own: Arc<dyn PalwTirOwnStepSourceV1>,
    prompt: Vec<u32>,
    /// The replay's own decode ids (G-T2: compared with the committed ones to find a selection candidate).
    replay_ids: Vec<u32>,
    /// Whether the replay's step tree differs (G-T2: equal IDs with differing steps is an arithmetic fault).
    steps_differ: bool,
    /// `(reserved_daa, acquisition allocation)` of this node's acquisition row, once its Open is accepted.
    acquisition: Option<(u64, Hash64)>,
    units: Vec<Arc<PalwTirPublicAuthenticatedUnitV1>>,
    ids: Option<PalwTirPublicAuthenticatedIdsV1>,
    harvested: BTreeSet<Hash64>,
    leaf: Option<u64>,
    /// The next unit the cone builder asked for (set by the last probe), or a need for the IDs.
    cone_want: Option<PalwTirPublicRequestV1>,
    cone_needs_ids: bool,
    demands: u32,
    // The unsigned signing digest distinguishes a retry from the next accepted-state move.
    last_move: Option<Hash64>,
    done: bool,
    _replay: crate::palw_memory_ledger::PalwMemoryReservationV1,
}

/// **The owner book**: the frame page cursor, the live cases and the frames set aside for a while.
#[derive(Default)]
pub(crate) struct PalwTirPublicOwnerBookV1 {
    after: Option<Hash64>,
    cases: HashMap<Hash64, OwnerCaseV1>,
    skipped: HashMap<Hash64, u64>,
}

// Only an identical move waits the retry interval; a new native turn/action can proceed.
fn owner_retry_waits_v1(last: Option<Hash64>, next: Hash64, moved_at: Option<u64>, now: u64) -> bool {
    last == Some(next) && moved_at.is_some_and(|at| now < at.saturating_add(COURT_MOVE_REPLAN_DAA))
}

/// The outcome of one sparse-cone probe over the accepted public material.
enum ConeOutcomeV1 {
    Acquire(PalwTirPublicRequestV1),
    AwaitIds,
    Ready(Box<PalwCourtVerdictProofV2>),
    Refused(String),
}

/// **One probe of the public sparse cone builder** at `leaf`, over exactly the accepted, original-root-authenticated step units
/// (converted to range credentials whose root is re-checked) and, when present, the authenticated IDs as the decode pin. A Need
/// becomes the one canonical request that fills its first gap (`StepLeaf`, or a `StepRun` of at most the native run bound).
#[allow(clippy::too_many_arguments)]
fn owner_cone_probe_v1(
    source: &PalwTirPublicSourceV1,
    registered: &kaspa_consensus_core::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1,
    units: &[Arc<PalwTirPublicAuthenticatedUnitV1>],
    ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
    prompt: &[u32],
    backend: &dyn PalwExecutionBackendV1,
    rules: &kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1,
    leaf: u64,
) -> Result<ConeOutcomeV1, String> {
    let header = source.header_v1();
    let mut compact =
        header.resolve_registered_layout_v1(registered, layout, header.committed_execution_root, rules.max_step_leaf_count)?;
    kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut compact);
    let mut intervals = Vec::new();
    let mut runs = Vec::new();
    for unit in units {
        if unit.source_id_v1() != source.source_id_v1() {
            return Err("TIR cone material belongs to another original source".into());
        }
        let (preimages, range) = match unit.response_v1() {
            PalwTirPublicResponseV1::StepLeaf { preimage, opening } => (
                vec![preimage.clone()],
                kaspa_consensus_core::palw_step_leg::PalwStepRangeOpeningV1 {
                    first_leaf_index: opening.leaf_index,
                    leaf_hashes: vec![opening.leaf_hash],
                    siblings: opening.siblings.clone(),
                },
            ),
            PalwTirPublicResponseV1::StepRun { preimages, range } => (preimages.clone(), range.clone()),
            _ => continue,
        };
        if kaspa_consensus_core::palw_step_leg::step_range_opening_root_capped_v1(
            compact.step_leaf_count,
            &range,
            rules.max_step_leaf_count,
        )
        .map_err(|e| e.to_string())?
            != compact.step_merkle_root
        {
            return Err("TIR public range conversion changes the original step root".into());
        }
        let first = range.first_leaf_index;
        intervals.push((first, first.saturating_add(preimages.len() as u64)));
        runs.push(kaspa_consensus_core::palw_tir_court_v1::PalwTirStepRunDisclosureV1 { binding: compact.clone(), preimages, range });
    }
    intervals.sort_unstable();
    let gap = |first: u64, count: u64| -> Option<(u64, u64)> {
        let end = first.saturating_add(count);
        let mut cursor = first;
        for &(lo, hi) in &intervals {
            if hi <= cursor {
                continue;
            }
            if lo > cursor {
                return Some((cursor, lo.min(end) - cursor));
            }
            cursor = hi.min(end);
            if cursor >= end {
                return None;
            }
        }
        (cursor < end).then_some((cursor, end - cursor))
    };
    let request_for = |first: u64, length: u64| {
        let count = length.min(u64::from(PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1)) as u32;
        if count <= 1 { PalwTirPublicRequestV1::StepLeaf { index: first } } else { PalwTirPublicRequestV1::StepRun { first, count } }
    };
    if let Some((first, length)) = gap(leaf, 1) {
        return Ok(ConeOutcomeV1::Acquire(request_for(first, length)));
    }
    let trace = ids.map(|ids| kaspa_consensus_core::palw_step_refute::PalwTiledDecodeTokensV1 {
        rows_root: ids.rows_root_v1(),
        generated_token_ids: ids.ids_v1().to_vec(),
    });
    let limits = kaspa_consensus_core::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1 {
        evaluation: rules.limits,
        max_target_requests: PALW_TIR_PUBLIC_OWNER_TARGETS_V1,
        max_source_reads: PALW_TIR_PUBLIC_OWNER_SOURCE_READS_V1,
    };
    Ok(
        match backend.probe_tir_public_cone_v1(
            header,
            prompt,
            trace.as_ref(),
            &runs,
            leaf,
            rules,
            limits,
            PALW_TIR_PUBLIC_OWNER_COPY_BYTES_V1,
        )? {
            PalwTirPublicConeProbeV1::Ready { proof, .. } => ConeOutcomeV1::Ready(Box::new(proof)),
            PalwTirPublicConeProbeV1::Refused { reason } => ConeOutcomeV1::Refused(reason),
            PalwTirPublicConeProbeV1::Need(need) => {
                let (first, count) = match need {
                    PalwTirPublicConeNeedV1::StepPreimage { index } | PalwTirPublicConeNeedV1::StepOpening { index } => (index, 1),
                    PalwTirPublicConeNeedV1::StepRange { first, count } => (first, count),
                    PalwTirPublicConeNeedV1::DecodePin => return Ok(ConeOutcomeV1::AwaitIds),
                    PalwTirPublicConeNeedV1::Prompt { .. } | PalwTirPublicConeNeedV1::RegisteredArtifact => {
                        return Ok(ConeOutcomeV1::Refused(format!("the cone needs material no owner demand supplies: {need:?}")));
                    }
                };
                if first.checked_add(count).is_none_or(|end| end > source.leaf_count_v1()) {
                    return Err("the cone builder named a range outside the original execution".into());
                }
                match gap(first, count) {
                    Some((missing, length)) => ConeOutcomeV1::Acquire(request_for(missing, length)),
                    None => {
                        ConeOutcomeV1::Refused("the named credential is already authenticated; no demand repairs the builder".into())
                    }
                }
            }
        },
    )
}

/// What one accepted read decided for one frame.
enum OwnerStepV1 {
    Replay {
        claim: Hash64,
        allocation_id: Hash64,
        source_id: Hash64,
        header: Box<kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>,
        job: Box<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,
        prompt: Vec<u32>,
        original_output_root: Hash64,
        vocab: u32,
    },
    Move {
        claim: Hash64,
        control: Box<PalwTirPublicControlV1>,
        due: u64,
    },
    Cone {
        claim: Hash64,
        source: Box<PalwTirPublicSourceV1>,
        registered: Box<kaspa_consensus_core::palw_tir_admission_v1::PalwTirClassRecordV1>,
        layout: Box<kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1>,
        leaf: u64,
        maximum_leaves: u64,
    },
    Finding {
        claim: Hash64,
        evidence: Box<PalwTirPublicConvictionEvidenceV1>,
    },
}

impl PalwPanelService {
    /// **One tick of the outside owner** (see the module doc). `signer` is this node's bond; `filer` is the public filer's book,
    /// which every finding enters. Never fails the loop.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn tir_public_owner_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        signer: PalwBondKeyV2,
        network_domain: Hash64,
        filer: &mut PalwTirPublicFilerBookV1,
        book: &mut PalwTirPublicOwnerBookV1,
        pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        due: &mut HashMap<(Hash64, u32, bool), u64>,
        moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        let ours = |round: u32, responder: bool| !responder && round == PALW_TIR_PUBLIC_OWNER_ROUND_V1;
        if self.consensus_config.params.palw_tir_public_conviction_v1_fence().is_none() {
            book.cases.clear();
            pending.retain(|(_, round, responder, _)| !ours(*round, *responder));
            return;
        }
        let form = self.config.prompt_ids_form;
        // Lane g14-court-capacity-r2: the owner fair start (`palw_court_capacity_v1`), read at the tick's clock below.
        let court_capacity_from = self.consensus_config.params.palw_court_capacity_v1_fence().map(|f| f.activation.daa_score());
        let mut fair_start = false;
        // G-T4: the claims this node's own seat found faulty are pursued first (process memory of the Panel replay).
        let seat_faulted: Vec<Hash64> = self.seat_faults.lock().unwrap().claims().collect();
        let court_limits = kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(&self.config.court);
        let mut steps: Vec<OwnerStepV1> = Vec::new();
        let mut now = 0u64;
        let armed = {
            let mut read = |state: &PalwChainStateV2| -> Result<(), String> {
                steps.clear();
                now = state.last_point().ok_or("TIR owner snapshot has no accepted clock")?.daa_score;
                fair_start = court_capacity_from.is_some_and(|from| now.saturating_add(1) >= from);
                book.skipped.retain(|_, until| now < *until);
                // Discover: the claims this node's seat found faulty first (G-T4), then one page of accepted frames per tick; a
                // replay is started for a frame this bond may own.
                let page = state.staged_tir_frames_after_v1(book.after, 16);
                book.after = page.last().copied();
                let order = owner_discovery_order_v1(&seat_faulted, &page, |claim| palw_tir_public_claim_v1(state, *claim).is_some());
                for claim in order {
                    if book.cases.len() >= PALW_TIR_PUBLIC_OWNER_CASES_V1
                        || book.cases.contains_key(&claim)
                        || book.skipped.contains_key(&claim)
                        || steps.iter().any(|s| matches!(s, OwnerStepV1::Replay { claim: c, .. } if *c == claim))
                    {
                        continue;
                    }
                    let Some(frame) = palw_tir_public_claim_v1(state, claim) else { continue };
                    let view = frame.view_v1(state.staged_tir_owner_count_v1(&claim));
                    let plan = frame.plan_v1();
                    let owns_here = state.staged_tir_acquisitions_of_v1(&claim).iter().any(|(owner, _)| *owner == signer);
                    // Lane g14-court-capacity-r2: past `palw_court_capacity_v1` an Open is never refused for the K_owner cap (it
                    // waits in the owner queue), so a free slot is not a precondition.
                    let slot_open =
                        owner_slot_open_v1(fair_start, state.staged_tir_owner_slot_open_v1(&claim, now.saturating_add(1)));
                    if !owner_may_pursue_v1(&view, plan.preparation_until_daa, &signer, now, slot_open, owns_here) {
                        continue;
                    }
                    let original = frame.original_v1();
                    // A canonical job's prompt is derived from the job itself at replay (`job_for_anchor`); a user prompt is the
                    // frame's kept public input (G-T3).
                    let canonical = original.job.prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL;
                    let prompt = match state.fp_claim_prompt_ids_v1(&original.job.class_id, &view.producer, &claim) {
                        Some(prompt) => prompt,
                        None if canonical => Vec::new(),
                        None => {
                            book.skipped.insert(claim, now.saturating_add(PALW_TIR_PUBLIC_OWNER_SKIP_DAA_V1));
                            continue;
                        }
                    };
                    if !(canonical && prompt.is_empty())
                        && kaspa_consensus_core::palw_freeprompt_v3::palw_fp_prompt_ids_admit_v1(&original.job, &prompt, form).is_err()
                    {
                        book.skipped.insert(claim, now.saturating_add(PALW_TIR_PUBLIC_OWNER_SKIP_DAA_V1));
                        continue;
                    }
                    steps.push(OwnerStepV1::Replay {
                        claim,
                        allocation_id: plan.allocation_id,
                        source_id: plan.source_id,
                        header: Box::new(frame.header_v1().clone()),
                        job: Box::new(original.job.clone()),
                        prompt,
                        original_output_root: original.output_root,
                        vocab: plan.acquisition.envelope.vocab,
                    });
                }
                // Advance every case by at most one decision.
                let mut ended = Vec::new();
                for (claim, case) in book.cases.iter_mut() {
                    let Some(frame) = palw_tir_public_claim_v1(state, *claim) else {
                        ended.push(*claim);
                        continue;
                    };
                    let terms = frame.plan_v1().terms;
                    if case.done || now >= terms.collect_until_daa || case.demands >= PALW_TIR_PUBLIC_OWNER_DEMANDS_V1 {
                        ended.push(*claim);
                        continue;
                    }
                    let maximum_leaves = frame.descriptor_v1().maximum_leaves;
                    let queue_due = terms.collect_until_daa.saturating_sub(1);
                    // 1. Our acquisition row, or an Open.
                    if case.acquisition.is_none() {
                        case.acquisition = state
                            .staged_tir_acquisitions_of_v1(claim)
                            .into_iter()
                            .find(|(owner, _)| *owner == signer)
                            .and_then(|(owner, allocation)| {
                                state.staged_tir_acquisition_v1(*claim, owner, allocation).map(|row| (row, allocation))
                            })
                            .map(|(row, allocation)| (row.owned.contract_v1().reserved_daa, allocation));
                    }
                    let Some((reserved_daa, acquisition)) = case.acquisition else {
                        // An Open is admitted only before the frame's owner-open door (`reserved_daa < open_until_daa`, the
                        // contract's own check): past it nothing can open, so the case ends instead of re-sending a refused Open.
                        if now.saturating_add(1) >= terms.open_until_daa {
                            ended.push(*claim);
                            continue;
                        }
                        let rent = frame.plan_v1().owner_terms_v1()?.plan.rent_sompi;
                        steps.push(OwnerStepV1::Move {
                            claim: *claim,
                            control: Box::new(PalwTirPublicControlV1 {
                                version: 1,
                                claim: *claim,
                                allocation_id: case.allocation_id,
                                source_id: case.source_id,
                                target: PalwTirPublicControlTargetV1::OwnerOpen { owner: signer },
                                expected_turn: 0,
                                declared_rent_sompi: rent,
                                signer,
                                action: PalwTirPublicControlActionV1::Open,
                                signature: Vec::new(),
                            }),
                            // Its own door, not the collection's: the court lane orders moves by due, and an Open scheduled at
                            // `collect_until − 1` waited behind every earlier-due move on the lane until its door had closed (node
                            // T-N1 harness, 2026-10-17: queued behind the fraud filer's demands, carried 70+ DAA after the door).
                            due: terms.open_until_daa.saturating_sub(1),
                        });
                        continue;
                    };
                    let Some(row) = state.staged_tir_acquisition_v1(*claim, signer, acquisition) else {
                        ended.push(*claim);
                        continue;
                    };
                    let source =
                        kaspa_consensus_core::palw_tir_public_state_v1::palw_tir_public_source_for_row_v1(state, row, maximum_leaves)?;
                    // 2. Harvest every accepted answer of the current turn, re-read from the rooted chunks and re-authenticated.
                    let active = row.owned.active_v1();
                    if let Some(p) = active {
                        for slot in p.slots.values() {
                            let Some(reference) = slot.finished.as_ref() else { continue };
                            let id = reference.hash_v1()?;
                            if !case.harvested.insert(id) {
                                continue;
                            }
                            match kaspa_consensus_core::palw_tir_public_state_v1::palw_tir_public_finished_material_v1(
                                state,
                                reference,
                                maximum_leaves,
                            )? {
                                Some(PalwTirPublicFinishedV1::Unit(unit)) => {
                                    if !case.units.iter().any(|u| u.request_v1() == unit.request_v1()) {
                                        case.units.push(Arc::new(unit));
                                    }
                                }
                                Some(PalwTirPublicFinishedV1::Ids(ids)) => case.ids = Some(ids),
                                None => {
                                    case.harvested.remove(&id);
                                }
                            }
                        }
                    }
                    // 3. A finding from the harvested material, or the next unit this case wants.
                    let original = frame.original_v1();
                    // G-T2: a selection candidate is decided first, from the complete committed IDs against the replay's own.
                    if case.suspicion == SuspicionV1::Selection
                        && let Some(ids) = case.ids.as_ref()
                    {
                        let rule = kaspa_consensus_core::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&original.job);
                        let differs = ids.ids_v1() != case.replay_ids.as_slice();
                        match (rule, differs) {
                            (Some(rule), true) => {
                                let registered =
                                    state.tir_class_v1(&original.job.class_id).ok_or("TIR owner: class record is gone")?;
                                let layout = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_layout_v1(
                                    state,
                                    original.job.class_id,
                                )
                                .ok_or("TIR owner: class layout is gone")?;
                                let header = frame.header_v1();
                                let binding = header.resolve_registered_layout_v1(
                                    registered,
                                    layout,
                                    header.committed_execution_root,
                                    maximum_leaves,
                                )?;
                                let rules = kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1 {
                                    max_step_leaf_count: maximum_leaves,
                                    prompt_form: form,
                                    limits: court_limits,
                                };
                                let tiles: Vec<_> = case
                                    .units
                                    .iter()
                                    .filter(|u| matches!(u.request_v1(), PalwTirPublicRequestV1::TraceTile { .. }))
                                    .map(|u| (u.request_v1(), u.response_v1().clone()))
                                    .collect();
                                use kaspa_consensus_core::palw_tir_objective_v1::{
                                    PalwTirSelectionProbeV1 as Probe, palw_tir_public_selection_probe_v1,
                                };
                                match palw_tir_public_selection_probe_v1(
                                    &binding,
                                    source.vocab_v1(),
                                    &rule,
                                    &rules,
                                    ids.ids_v1(),
                                    &case.replay_ids,
                                    &tiles,
                                )? {
                                    Probe::Ready { position, evidence } => {
                                        info!("[{PALW_PANEL}] TIR frame {claim}: decode-selection fault at position {position}");
                                        steps.push(OwnerStepV1::Finding { claim: *claim, evidence: Box::new(evidence) });
                                        continue;
                                    }
                                    Probe::Need(requests) => case.cone_want = requests.first().copied(),
                                    // The committed row does not refute its token: the logits differ upstream — descend the steps.
                                    Probe::Upstream { .. } | Probe::Agrees => {
                                        case.suspicion = if case.steps_differ { SuspicionV1::Arithmetic } else { SuspicionV1::Output };
                                        case.cone_want = None;
                                    }
                                }
                            }
                            // Equal IDs (or a constrained job, whose selection is its semantic court's): steps or output decide.
                            _ => {
                                case.suspicion = if case.steps_differ { SuspicionV1::Arithmetic } else { SuspicionV1::Output };
                                case.cone_want = None;
                            }
                        }
                    }
                    let want = match case.suspicion {
                        // Still before the complete IDs, or waiting on a selection row's tile (the probe named it).
                        SuspicionV1::Selection => match case.ids.as_ref() {
                            None => Some(PalwTirPublicRequestV1::Ids { offset: 0 }),
                            Some(_) => case.cone_want.filter(|w| !case.units.iter().any(|u| u.request_v1() == *w)),
                        },
                        SuspicionV1::Output => match case.ids.as_ref() {
                            Some(ids) => {
                                match kaspa_consensus_core::palw_tir_objective_v1::palw_tir_public_output_contradiction_v1(
                                    &original.job,
                                    &frame.header_v1().job_context,
                                    original.output_root,
                                    source.vocab_v1(),
                                    ids,
                                ) {
                                    Some(evidence) => {
                                        steps.push(OwnerStepV1::Finding { claim: *claim, evidence: Box::new(evidence) });
                                    }
                                    None => case.done = true,
                                }
                                None
                            }
                            None => Some(PalwTirPublicRequestV1::Ids { offset: 0 }),
                        },
                        SuspicionV1::Arithmetic => {
                            if case.cone_needs_ids && case.ids.is_none() {
                                Some(PalwTirPublicRequestV1::Ids { offset: 0 })
                            } else if let Some(want) = case.cone_want.filter(|w| !case.units.iter().any(|u| u.request_v1() == *w)) {
                                Some(want)
                            } else if let Some(leaf) = case.leaf {
                                // Everything the last probe named is here: probe again (off the read).
                                let registered =
                                    state.tir_class_v1(&original.job.class_id).ok_or("TIR owner: class record is gone")?;
                                let layout = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_layout_v1(
                                    state,
                                    original.job.class_id,
                                )
                                .ok_or("TIR owner: class layout is gone")?;
                                steps.push(OwnerStepV1::Cone {
                                    claim: *claim,
                                    source: Box::new(source.clone()),
                                    registered: Box::new(registered.clone()),
                                    layout: Box::new(layout.clone()),
                                    leaf,
                                    maximum_leaves,
                                });
                                None
                            } else {
                                let units = &case.units;
                                match super::tir_arithmetic_scheduler::next_public_from_source_v1(
                                    &source,
                                    case.own.as_ref(),
                                    |request| units.iter().find(|u| u.request_v1() == request).cloned(),
                                )? {
                                    super::tir_arithmetic_scheduler::PublicArithmeticStepV1::Acquire(request) => Some(request),
                                    super::tir_arithmetic_scheduler::PublicArithmeticStepV1::Leaf { index } => {
                                        case.leaf = Some(index);
                                        Some(PalwTirPublicRequestV1::StepLeaf { index })
                                    }
                                    super::tir_arithmetic_scheduler::PublicArithmeticStepV1::StepAgreement => {
                                        // The steps agree after all: the output (or stop rule) is what differs.
                                        case.suspicion = SuspicionV1::Output;
                                        Some(PalwTirPublicRequestV1::Ids { offset: 0 })
                                    }
                                }
                            }
                        }
                    };
                    // 4. The next move: a fresh Demand once the turn is answered, or Collect a default past its clock.
                    let control = |action: PalwTirPublicOwnedActionV1, turn: u64| {
                        Box::new(PalwTirPublicControlV1 {
                            version: 1,
                            claim: *claim,
                            allocation_id: case.allocation_id,
                            source_id: case.source_id,
                            target: PalwTirPublicControlTargetV1::Acquisition { owner: signer, reserved_daa },
                            expected_turn: turn,
                            declared_rent_sompi: 0,
                            signer,
                            action: PalwTirPublicControlActionV1::Acquisition(action),
                            signature: Vec::new(),
                        })
                    };
                    let open = active.filter(|p| {
                        !p.collected
                            && row
                                .owned
                                .response_custodians_v1()
                                .iter()
                                .any(|s| p.slots.get(s).is_none_or(|slot| slot.finished.is_none()))
                    });
                    match (open, want) {
                        (Some(p), _) if now >= p.respond_until_daa => {
                            steps.push(OwnerStepV1::Move {
                                claim: *claim,
                                control: control(PalwTirPublicOwnedActionV1::Collect { demand_id: p.demand_id }, p.turn),
                                due: queue_due,
                            });
                        }
                        (Some(_), _) => {}
                        // Lane g14-court-capacity-r2: a WAITING row's Demand is refused unless the rotation it triggers admits it —
                        // ask the fold's own predicate first, so no refused carrier is signed.
                        (None, Some(_))
                            if fair_start
                                && !state.staged_tir_owner_demand_admits_v1(
                                    claim,
                                    &kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirAcquisition(
                                        *claim,
                                        signer,
                                        acquisition,
                                    ),
                                    now.saturating_add(1),
                                    kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_owner_tenure_v1(
                                        &frame,
                                        row.owned.owner_idle_grace_daa_v1(),
                                    ),
                                ) => {}
                        (None, Some(request)) if active.is_none_or(|p| p.collected || p.request != request) => {
                            steps.push(OwnerStepV1::Move {
                                claim: *claim,
                                control: control(PalwTirPublicOwnedActionV1::Demand { request }, row.owned.spent_v1().turns),
                                due: queue_due,
                            });
                        }
                        _ => {}
                    }
                }
                for claim in ended {
                    book.cases.remove(&claim);
                }
                Ok(())
            };
            session.palw_tir_public_read_v1(&mut read)
        };
        match armed {
            Ok(true) => {}
            Ok(false) => {
                book.cases.clear();
                pending.retain(|(_, round, responder, _)| !ours(*round, *responder));
                return;
            }
            Err(why) => {
                debug!("[{PALW_PANEL}] TIR owner read failed: {why}");
                return;
            }
        }
        pending.retain(|(claim, round, responder, _)| !ours(*round, *responder) || book.cases.contains_key(claim));
        for step in steps {
            let result: Result<(), String> = async {
                match step {
                    OwnerStepV1::Replay { claim, allocation_id, source_id, header, job, prompt, original_output_root, vocab } => {
                        let class = header.job_context.shape_profile_id;
                        let backend: Arc<dyn PalwExecutionBackendV1> = match self.resolve_backend(session, class, header.artifact_root) {
                            Ok(backend) => Arc::from(backend),
                            Err(_) => {
                                book.skipped.insert(claim, now.saturating_add(PALW_TIR_PUBLIC_OWNER_SKIP_DAA_V1));
                                return Ok(());
                            }
                        };
                        let prompt = if prompt.is_empty()
                            && job.prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL
                        {
                            let anchor = kaspa_consensus_core::palw_freeprompt_v3::fp_canonical_anchor_v1(&job);
                            let (_, derived) = backend.job_for_anchor(anchor)?;
                            let ids: Vec<u32> = derived.iter().map(|t| *t as u32).collect();
                            kaspa_consensus_core::palw_freeprompt_v3::palw_fp_prompt_ids_admit_v1(&job, &ids, form)
                                .map_err(|e| format!("the canonical prompt this node derives is not the job's: {e:?}"))?;
                            ids
                        } else {
                            prompt
                        };
                        let need = backend.tir_own_step_source_need_v1(&header, &job, &prompt)?;
                        let memory = crate::palw_memory_ledger::host_ledger_v1()
                            .reserve(
                                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 { role: "public-tir-owner-replay", class_id: class, job: claim },
                                need.resident_ram_bytes,
                            )
                            .map_err(|e| e.to_string())?;
                        let cache = self.config.retention_dir.join("tir-public-owner").join(format!("{claim}"));
                        let (header_c, job_c, prompt_c) = (header.clone(), job.clone(), prompt.clone());
                        let own = tokio::task::spawn_blocking(move || {
                            std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
                            backend.open_tir_own_step_source_v1(&header_c, &job_c, &prompt_c, &cache)
                        })
                        .await
                        .map_err(|e| format!("TIR owner replay worker failed: {e}"))??;
                        let info = own.info_v1();
                        let steps_differ = info.header.step_merkle_root != header.step_merkle_root
                            || info.header.step_leaf_count != header.step_leaf_count;
                        // G-T2: any trace or step disagreement starts from the committed IDs (selection first, then the steps);
                        // agreeing roots are still checked against the job's own stop rule.
                        let suspicion = owner_suspicion_v1(&header, original_output_root, &job, vocab, info);
                        let replay_ids = info.generated_ids.clone();
                        let Some(suspicion) = suspicion else {
                            book.skipped.insert(claim, now.saturating_add(PALW_TIR_PUBLIC_OWNER_SKIP_DAA_V1));
                            return Ok(());
                        };
                        info!("[{PALW_PANEL}] TIR frame {claim}: independent replay disagrees ({suspicion:?}); opening an owner acquisition");
                        book.cases.insert(
                            claim,
                            OwnerCaseV1 {
                                claim,
                                allocation_id,
                                source_id,
                                suspicion,
                                own,
                                prompt,
                                replay_ids,
                                steps_differ,
                                acquisition: None,
                                units: Vec::new(),
                                ids: None,
                                harvested: BTreeSet::new(),
                                leaf: None,
                                cone_want: None,
                                cone_needs_ids: false,
                                demands: 0,
                                last_move: None,
                                done: false,
                                _replay: memory,
                            },
                        );
                    }
                    OwnerStepV1::Move { claim, mut control, due: due_daa } => {
                        let queue = (claim, PALW_TIR_PUBLIC_OWNER_ROUND_V1, false);
                        let digest = control.signing_message_v1(network_domain)?;
                        let last = book.cases.get(&claim).and_then(|case| case.last_move);
                        if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == queue)
                            || owner_retry_waits_v1(last, digest, moved.get(&queue).copied(), now)
                        {
                            return Ok(());
                        }
                        control.signature = self
                            .sign(digest.as_byte_slice(), PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1)
                            .ok_or("TIR owner signing key unavailable")?;
                        control.check_v1()?;
                        // A demand counts against the case's own budget only when it is actually queued.
                        if matches!(&control.action, PalwTirPublicControlActionV1::Acquisition(PalwTirPublicOwnedActionV1::Demand { .. }))
                            && let Some(case) = book.cases.get_mut(&claim)
                        {
                            case.demands += 1;
                        }
                        if let Some(case) = book.cases.get_mut(&claim) {
                            case.last_move = Some(digest);
                        }
                        pending.push((queue.0, queue.1, queue.2, PalwConsensusObjectV2::TirPublicControlV1 { control }));
                        due.insert(queue, due_daa);
                        moved.insert(queue, now);
                    }
                    OwnerStepV1::Cone { claim, source, registered, layout, leaf, maximum_leaves } => {
                        let Some(case) = book.cases.get(&claim) else { return Ok(()) };
                        let class = registered.facts.class_id;
                        let backend: Arc<dyn PalwExecutionBackendV1> =
                            Arc::from(self.resolve_backend(session, class, source.header_v1().artifact_root)?);
                        let units = case.units.clone();
                        let ids = case.ids.clone();
                        let prompt = case.prompt.clone();
                        let court = self.config.court;
                        let rules = kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1 {
                            max_step_leaf_count: maximum_leaves,
                            prompt_form: self.config.prompt_ids_form,
                            limits: kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(&court),
                        };
                        let outcome = tokio::task::spawn_blocking(move || {
                            owner_cone_probe_v1(&source, &registered, &layout, &units, ids.as_ref(), &prompt, backend.as_ref(), &rules, leaf)
                        })
                        .await
                        .map_err(|e| format!("TIR owner cone worker failed: {e}"))??;
                        let Some(case) = book.cases.get_mut(&claim) else { return Ok(()) };
                        match outcome {
                            ConeOutcomeV1::Acquire(request) => case.cone_want = Some(request),
                            ConeOutcomeV1::AwaitIds => case.cone_needs_ids = true,
                            ConeOutcomeV1::Ready(proof) => {
                                let evidence = palw_tir_public_arithmetic_evidence_v1(*proof)?;
                                if filer.queue_finding_v1(claim, evidence, now) {
                                    info!("[{PALW_PANEL}] TIR frame {claim}: arithmetic finding at leaf {leaf} handed to the public filer");
                                }
                                case.done = true;
                            }
                            ConeOutcomeV1::Refused(reason) => {
                                info!("[{PALW_PANEL}] TIR frame {claim}: cone construction refused at leaf {leaf}: {reason}");
                                case.done = true;
                            }
                        }
                    }
                    OwnerStepV1::Finding { claim, evidence } => {
                        if filer.queue_finding_v1(claim, *evidence, now) {
                            info!("[{PALW_PANEL}] TIR frame {claim}: output finding handed to the public filer");
                        }
                        if let Some(case) = book.cases.get_mut(&claim) {
                            case.done = true;
                        }
                    }
                }
                Ok(())
            }
            .await;
            if let Err(why) = result {
                debug!("[{PALW_PANEL}] TIR owner step remains pending: {why}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_tir_public_claim_v1::{PalwTirPublicCustodianLoadV1, PalwTirPublicFrameViewV1};

    #[test]
    fn owner_next_move_does_not_wait_for_the_previous_moves_retry_interval() {
        let previous = Hash64::from_u64_word(1);
        let next = Hash64::from_u64_word(2);
        assert!(owner_retry_waits_v1(Some(previous), previous, Some(100), 101));
        assert!(!owner_retry_waits_v1(Some(previous), next, Some(100), 101));
        assert!(!owner_retry_waits_v1(Some(previous), previous, Some(100), 100 + COURT_MOVE_REPLAN_DAA));
        assert!(!owner_retry_waits_v1(None, next, None, 0));
    }

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(n), 0))
    }
    fn header(step_root: u64, trace_root: u64) -> kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
        kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
            version: 1,
            job_context: kaspa_consensus_core::palw_v2::PalwJobContextV2 {
                version: 2,
                network_id: b"testnet-12".to_vec(),
                job_id: h(1),
                job_nullifier: h(2),
                assignment_id: h(3),
                execution_seed: [0u8; 32],
                model_profile_id: h(4),
                runtime_manifest_hash: h(5),
                runtime_class_id: h(6),
                shape_profile_id: h(7),
                trace_scheme_id: h(8),
                cu_ruleset_id: h(9),
                tokenizer_id: h(10),
                prompt_token_ids_hash: h(11),
                declared_prefill_tokens: 3,
                exact_decode_tokens: 2,
                max_context_tokens: 64,
            },
            artifact_root: h(12),
            full_logits_trace_root: h(trace_root),
            step_leaf_count: 19,
            step_merkle_root: h(step_root),
            committed_execution_root: h(15),
        }
    }
    /// The producer is bond 1, seat 2 holds a credited Valid custody grant (liable), seat 3 was drawn and signed nothing.
    fn view() -> PalwTirPublicFrameViewV1 {
        PalwTirPublicFrameViewV1 {
            claim: h(100),
            allocation_id: h(101),
            source_id: h(102),
            producer: bond(1),
            pledge_id: h(103),
            liable: BTreeSet::from([bond(1), bond(2)]),
            drawn_panel: BTreeSet::from([bond(2), bond(3)]),
            accepted_daa: 100,
            collect_until_daa: 200,
            material_until_daa: 300,
            conviction_rent_sompi: 777,
            maximum_witness_bytes: 1 << 20,
            selection: Some(kaspa_consensus_core::palw_tir_court_v1::PalwTirSelectionRuleV1::Keyed {
                sampling: kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
            }),
            owners_opened: 8,
            custodian_load: PalwTirPublicCustodianLoadV1 { owners: 8, demands_per_custodian: 64, answer_bytes_per_custodian: 1 << 20 },
            header: header(14, 13),
        }
    }
    fn job(prompt_mode: u8) -> kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3 {
        kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: h(0x4E7),
            class_id: h(7),
            executor_bond: bond(1).0,
            executor_pubkey: vec![1; 4],
            operator_id: h(0x10),
            anchor_block: h(0x11),
            anchor_daa: 90,
            job_nonce: [3; 32],
            tokenizer_id: h(10),
            prompt_token_ids_hash: h(11),
            prompt_tokens: 3,
            decode_token_limit: 2,
            max_context_tokens: 64,
            privacy_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode,
            sampling_seed: [0; 32],
            temperature_q: 0,
            decode: None,
            tail: None,
        }
    }

    /// **T-N6, node half: `g14_tir_seat_finding_reaches_the_filer_queue`** (G-T4). An honest drawn seat that signed no Valid is
    /// an independent owner: the owner worker pursues the frame for it (never for a credited seat or the producer), the claims its
    /// seat found faulty are taken first, a frame at K_owner is still pursued while a holder is idle (or where this bond already
    /// owns), and its finding enters the public filer's book exactly as an outsider's does.
    #[test]
    fn g14_tir_seat_finding_reaches_the_filer_queue() {
        let v = view();
        let prepared = 120;
        assert!(owner_may_pursue_v1(&v, prepared, &bond(3), 150, true, false), "the drawn seat that signed no Valid");
        assert!(owner_may_pursue_v1(&v, prepared, &bond(300), 150, true, false), "an outsider");
        assert!(!owner_may_pursue_v1(&v, prepared, &bond(2), 150, true, false), "a credited (liable) seat");
        assert!(!owner_may_pursue_v1(&v, prepared, &bond(1), 150, true, false), "the producer");
        assert!(!owner_may_pursue_v1(&v, prepared, &bond(3), 150, false, false), "no slot and none reclaimable");
        assert!(owner_may_pursue_v1(&v, prepared, &bond(3), 150, false, true), "it already owns there");
        assert!(!owner_may_pursue_v1(&v, prepared, &bond(3), 110, true, false), "before the original preparation door");
        assert!(!owner_may_pursue_v1(&v, prepared, &bond(3), 200, true, false), "the conviction door closed");
        // Lane g14-court-capacity-r2: past `palw_court_capacity_v1` a full frame does not stop an Open (it waits in the owner queue).
        assert!(owner_slot_open_v1(true, false), "past the fence: an Open at K_owner waits, never refused");
        assert!(!owner_slot_open_v1(false, false), "below the fence: no free or reclaimable slot, no Open");
        assert!(owner_may_pursue_v1(&v, prepared, &bond(3), 150, owner_slot_open_v1(true, false), false), "the newcomer pursues");
        assert!(
            !owner_may_pursue_v1(&v, prepared, &bond(2), 150, owner_slot_open_v1(true, false), false),
            "a liable seat still never owns, fence or not"
        );
        // The seat's own faulty claims first (when they are frames), then the page, each once.
        let order = owner_discovery_order_v1(&[h(9), h(5)], &[h(4), h(5), h(6)], |claim| *claim != h(9));
        assert_eq!(order, vec![h(5), h(4), h(6)]);
        // The seat's finding enters the same filer book (persisted), one per claim.
        let dir = tempfile::tempdir().unwrap();
        let mut book = PalwTirPublicFilerBookV1::load(dir.path());
        let finding = PalwTirPublicConvictionEvidenceV1::Output { rows_root: h(20), ids: vec![1, 2] };
        assert!(book.queue_finding_v1(v.claim, finding.clone(), 150));
        assert!(!book.queue_finding_v1(v.claim, finding, 151));
        assert_eq!(PalwTirPublicFilerBookV1::load(dir.path()).len(), 1, "it survives a restart");
    }

    /// **G-T2, the owner's first decision.** A replay whose trace or steps differ is a SELECTION candidate first (the committed IDs
    /// are read and compared before any step descent); one whose roots agree but whose output differs, or whose own answer breaks
    /// the job's stop rule, is an OUTPUT case; full agreement opens nothing.
    #[test]
    fn g14_tir_owner_reads_the_ids_first_on_any_trace_or_step_disagreement() {
        use kaspa_consensus_core::palw_backend::PalwTirOwnStepSourceInfoV1 as Info;
        let committed = header(14, 13);
        let j = job(kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER);
        let replay = |step: u64, trace: u64, output: u64, ids: Vec<u32>| Info {
            header: header(step, trace),
            generated_ids: ids,
            output_root: h(output),
            rows_root: None,
        };
        assert_eq!(owner_suspicion_v1(&committed, h(30), &j, 8, &replay(14, 13, 30, vec![1, 2])), None);
        assert_eq!(owner_suspicion_v1(&committed, h(30), &j, 8, &replay(14, 99, 30, vec![1, 2])), Some(SuspicionV1::Selection));
        assert_eq!(owner_suspicion_v1(&committed, h(30), &j, 8, &replay(99, 13, 30, vec![1, 2])), Some(SuspicionV1::Selection));
        assert_eq!(owner_suspicion_v1(&committed, h(30), &j, 8, &replay(14, 13, 31, vec![1, 2])), Some(SuspicionV1::Output));
        // The V3 rule never stops before its budget: a short own answer is an output (stop-rule) case even when the roots agree.
        assert_eq!(owner_suspicion_v1(&committed, h(30), &j, 8, &replay(14, 13, 30, vec![1])), Some(SuspicionV1::Output));
    }
}
