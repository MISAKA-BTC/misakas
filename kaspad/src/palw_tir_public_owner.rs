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
//! every finding already produced. **Not covered:** a decode-selection or trace-only disagreement (equal steps and output) is
//! logged, not localized; a class with no kept prompt ids is not replayed.
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
enum SuspicionV1 {
    /// The committed step tree differs from the replay's: localize to a leaf and build its cone/logits primitive.
    Arithmetic,
    /// Steps agree but the committed output does not (or the stop rule is in question): read the complete original IDs.
    Output,
}

/// One frame this node pursues as an outside owner (process memory).
struct OwnerCaseV1 {
    claim: Hash64,
    allocation_id: Hash64,
    source_id: Hash64,
    suspicion: SuspicionV1,
    own: Arc<dyn PalwTirOwnStepSourceV1>,
    prompt: Vec<u32>,
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
        let mut steps: Vec<OwnerStepV1> = Vec::new();
        let mut now = 0u64;
        let armed = {
            let mut read = |state: &PalwChainStateV2| -> Result<(), String> {
                steps.clear();
                now = state.last_point().ok_or("TIR owner snapshot has no accepted clock")?.daa_score;
                book.skipped.retain(|_, until| now < *until);
                // Discover: one page of accepted frames per tick; a replay is started for a frame this bond may own.
                let page = state.staged_tir_frames_after_v1(book.after, 16);
                book.after = page.last().copied();
                for claim in page {
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
                    if view.liable.contains(&signer)
                        || view.drawn_panel.contains(&signer)
                        || now < plan.preparation_until_daa
                        || now >= view.collect_until_daa
                        || view.owners_opened >= view.custodian_load.owners
                    {
                        continue;
                    }
                    let original = frame.original_v1();
                    let Some(prompt) = state.fp_claim_prompt_ids_v1(&original.job.class_id, &view.producer, &claim) else {
                        book.skipped.insert(claim, now.saturating_add(PALW_TIR_PUBLIC_OWNER_SKIP_DAA_V1));
                        continue;
                    };
                    if kaspa_consensus_core::palw_freeprompt_v3::palw_fp_prompt_ids_admit_v1(&original.job, &prompt, form).is_err() {
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
                            due: queue_due,
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
                    let want = match case.suspicion {
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
                    OwnerStepV1::Replay { claim, allocation_id, source_id, header, job, prompt, original_output_root } => {
                        let class = header.job_context.shape_profile_id;
                        let backend: Arc<dyn PalwExecutionBackendV1> = match self.resolve_backend(session, class, header.artifact_root) {
                            Ok(backend) => Arc::from(backend),
                            Err(_) => {
                                book.skipped.insert(claim, now.saturating_add(PALW_TIR_PUBLIC_OWNER_SKIP_DAA_V1));
                                return Ok(());
                            }
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
                        let suspicion = if info.header.step_merkle_root != header.step_merkle_root
                            || info.header.step_leaf_count != header.step_leaf_count
                        {
                            Some(SuspicionV1::Arithmetic)
                        } else if info.output_root != original_output_root {
                            Some(SuspicionV1::Output)
                        } else {
                            if info.header.full_logits_trace_root != header.full_logits_trace_root {
                                info!("[{PALW_PANEL}] TIR frame {claim}: trace-only disagreement (steps and output agree) is not localized here");
                            }
                            None
                        };
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
                                acquisition: None,
                                units: Vec::new(),
                                ids: None,
                                harvested: BTreeSet::new(),
                                leaf: None,
                                cone_want: None,
                                cone_needs_ids: false,
                                demands: 0,
                                done: false,
                                _replay: memory,
                            },
                        );
                    }
                    OwnerStepV1::Move { claim, mut control, due: due_daa } => {
                        let queue = (claim, PALW_TIR_PUBLIC_OWNER_ROUND_V1, false);
                        if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == queue)
                            || moved.get(&queue).is_some_and(|at| now < at.saturating_add(COURT_MOVE_REPLAN_DAA))
                        {
                            return Ok(());
                        }
                        let digest = control.signing_message_v1(network_domain)?;
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
