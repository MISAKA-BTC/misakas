//! **RFC-0004 A6 in the fold: the evaluation job family's state** (spec 17 §17.8.2–§17.8.3). A child
//! module of `palw_state_v2`, as the core lane's `palw_improve_fold_v1` is, so it reads the builder and
//! the state's tables directly and writes them only through their one writers.
//!
//! * **The table** `improvement_eval_jobs` — `(line, epoch, item, subject, kind)` →
//!   [`PalwEvalJobStateV1`], one row per job, written at the job's first claim (a draw writes none),
//!   journaled as the improvement tables are (`ImprovementRow`, table
//!   [`PALW_IMPROVE_TABLE_EVAL_JOBS_V1`]), rooted in its own Some-only block (`improvement-eval/v1`)
//!   and carried in its own tail (`0xCC`). Its index `improvement_eval_claims` (claim → job) is
//!   rebuilt from the rows, never hashed.
//! * **Commitment** ([`apply_improvement_eval_commitment_v1`]): the free-prompt arm's one guarded
//!   branch for a version-9 job. The claim is an FP claim with no quanta, no pwu, no receipt rights
//!   and no weight — MIP-17's exclusion from the reward path — and it holds collateral as any claim
//!   of that work does ([`palw_improve_eval_reservation_v1`]). Everything else is derived and checked:
//!   the epoch is evaluating and before `t_eval`, the item is drawn and not dropped, the subject is
//!   the epoch's and its class the claim's, the kind is the policy's, the mode is the one the policy
//!   and the item fix, the prompt (and a teacher-forced job's reference) is the disclosed one, the
//!   roots are the tail's, and `work_leaves` is the chain's count of the derived context. The first
//!   such claim takes the job; a second is refused by name while the first lives.
//! * **`Final`** ([`TransitionBuilder::note_improvement_eval_final_v1`], from `finalize_claim`): the
//!   row records it; a kind that committed a score records it through the core's
//!   `record_improvement_score_v1`; the fee is paid from the subject's escrow
//!   (`pay_improvement_eval_fee_v1`). ExactMatch scores at the key's reveal instead
//!   ([`TransitionBuilder::score_improvement_exact_match_v1`], which the material lane's reveal arms call).
//! * **The core's hooks**: an evaluation claim is an FP claim with zero quanta
//!   ([`palw_improve_claim_is_evaluation_v1`]; the lane refuses sub-quantum work at acceptance, so
//!   nothing else has none); an epoch is pending while any of its jobs holds a live claim not yet final
//!   ([`PalwChainStateV2::improvement_eval_pending_v1`]).

use super::*;
use crate::palw_improve_eval_v1::*;
use crate::palw_improve_state_v1::*;

/// `improvement_eval_jobs`' table id in `ImprovementRow` (delta 92) — the next after the core's twelve
/// (spec 17 §17.0: a table added later takes a table id, not a delta discriminant).
pub const PALW_IMPROVE_TABLE_EVAL_JOBS_V1: u8 = 13;

fn refused(why: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::ImprovementRefused(why)
}

// ---- the material lane's answers (A4) --------------------------------------------------------------

/// **An item's disclosed prompt**, from the material lane's rows (A4): a hold-out case's prompt ids, or
/// a setter item's once `SetterSetRevealed` opened them. A suite item's prompt is not on chain, so it
/// is never disclosed here: its jobs take no claim, and the item counts for the incumbent.
fn palw_improve_eval_item_prompt_hook_v1(state: &PalwChainStateV2, line_id: &Hash64, epoch: u64, item: u32) -> Option<Vec<u32>> {
    #[cfg(test)]
    if let Some((prompt, _)) = test_disclosure::get(line_id, epoch, item) {
        return Some(prompt);
    }
    let row = state.improvement_item(line_id, epoch, item)?;
    match row.source {
        PalwItemSourceV1::HoldOut => state.improvement_case(line_id, &row.case_id).map(|case| case.case.prompt_ids.clone()),
        PalwItemSourceV1::Setter { set_id, index } => state.improvement_setter_prompt(line_id, &set_id, index).map(<[u32]>::to_vec),
        PalwItemSourceV1::Regression { .. } | PalwItemSourceV1::Safety { .. } => None,
    }
}

/// **A teacher-forced item's disclosed reference**, from the material lane's rows (A4): a hold-out
/// case's `Continuation`, opened from the draw on (RFC-0004 §7.1 as decided 2026-09-29). A setter
/// item's reference opens with its keys, in `Closing` — too late for a claim, so a setter set's
/// likelihood items take none (the material lane's to move to the draw).
fn palw_improve_eval_item_reference_hook_v1(state: &PalwChainStateV2, line_id: &Hash64, epoch: u64, item: u32) -> Option<Vec<u32>> {
    #[cfg(test)]
    if let Some((_, Some(reference))) = test_disclosure::get(line_id, epoch, item) {
        return Some(reference);
    }
    let row = state.improvement_item(line_id, epoch, item)?;
    match row.source {
        PalwItemSourceV1::HoldOut => {
            let case = state.improvement_case(line_id, &row.case_id)?;
            match case.case.reference {
                crate::palw_improve_material_v1::PalwCaseReferenceV1::Continuation { .. } => case.revealed.clone(),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod test_disclosure {
    //! A test's stand-in for the material lane's disclosed prompts and references.
    use super::Hash64;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    type Disclosed = BTreeMap<(Hash64, u64, u32), (Vec<u32>, Option<Vec<u32>>)>;
    thread_local! {
        static DISCLOSED: RefCell<Disclosed> = RefCell::new(BTreeMap::new());
    }

    pub fn set(line_id: Hash64, epoch: u64, item: u32, prompt: Vec<u32>, reference: Option<Vec<u32>>) {
        DISCLOSED.with(|d| d.borrow_mut().insert((line_id, epoch, item), (prompt, reference)));
    }

    pub fn get(line_id: &Hash64, epoch: u64, item: u32) -> Option<(Vec<u32>, Option<Vec<u32>>)> {
        DISCLOSED.with(|d| d.borrow().get(&(*line_id, epoch, item)).cloned())
    }
}

// ---- readers -----------------------------------------------------------------------------------

impl PalwChainStateV2 {
    /// An evaluation job's row, if its first claim has come.
    pub fn improvement_eval_job(
        &self,
        line_id: &Hash64,
        epoch: u64,
        item: u32,
        subject: &PalwEvalSubjectV1,
        kind: PalwScoringKindV1,
    ) -> Option<&PalwEvalJobStateV1> {
        self.improvement_eval_jobs.get(&(*line_id, epoch, item, *subject, kind))
    }

    /// The evaluation job a claim took, and its row.
    pub fn improvement_eval_job_of_claim(&self, claim_id: &Hash64) -> Option<(PalwEvalJobKeyV1, &PalwEvalJobStateV1)> {
        let key = *self.improvement_eval_claims.get(claim_id)?;
        Some((key, self.improvement_eval_jobs.get(&key)?))
    }

    /// Every job row of an epoch, in key order.
    pub fn improvement_eval_jobs_of_epoch(&self, line_id: &Hash64, epoch: u64) -> Vec<(PalwEvalJobKeyV1, &PalwEvalJobStateV1)> {
        self.improvement_eval_jobs.range(palw_improve_eval_epoch_range_v1(line_id, epoch)).map(|(k, row)| (*k, row)).collect()
    }

    /// Does the chain hold this claim live or final (not voided, not gone)?
    fn improvement_eval_claim_held_v1(&self, claim_id: &Hash64) -> bool {
        self.claims.get(claim_id).is_some_and(|claim| !matches!(claim.phase, PalwClaimPhaseV2::Voided { .. }))
    }

    /// Is this row's claim live and not yet final?
    fn improvement_eval_row_pending_v1(&self, row: &PalwEvalJobStateV1) -> bool {
        row.claim.as_ref().is_some_and(|c| c.final_daa.is_none() && self.improvement_eval_claim_held_v1(&c.claim_id))
    }

    /// **The core's completion hook** (spec 17 §17.5.3 step 7): is anything of the epoch's evaluation
    /// still to come? A job holding a live claim that is not yet final; or a final generation whose
    /// ExactMatch score is not recorded, on an item not dropped — its key is not revealed yet (the
    /// material lane's reveal scores it). Either keeps the epoch from scoring before `t_score`.
    pub fn improvement_eval_pending_v1(&self, line_id: &Hash64, epoch: u64) -> bool {
        self.improvement_eval_jobs.range(palw_improve_eval_epoch_range_v1(line_id, epoch)).any(|((_, _, item, subject, kind), row)| {
            if self.improvement_eval_row_pending_v1(row) {
                return true;
            }
            let final_generation = *kind == PalwScoringKindV1::ExactMatch && row.claim.is_some_and(|c| c.final_daa.is_some());
            final_generation
                && self.improvement_item(line_id, epoch, *item).is_some_and(|i| !i.dropped)
                && self
                    .improvement_result(line_id, epoch, *item, subject)
                    .is_none_or(|result| result.scores.iter().all(|score| score.kind != PalwScoringKindV1::ExactMatch))
        })
    }

    /// **Are the generations of `items` settled?** (spec 17 §17.8.2's key disclosure, E17: keys open
    /// only in `Closing`, which the material lane's reveal arms check) — no subject's generation (its
    /// ExactMatch job) on any of them holds a live claim that is not yet final.
    pub fn improvement_eval_generations_settled_v1(&self, line_id: &Hash64, epoch: u64, items: &[u32]) -> bool {
        items.iter().all(|item| {
            let lo = (*line_id, epoch, *item, PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch);
            let hi =
                (*line_id, epoch, *item, PalwEvalSubjectV1::Previous(Hash64::from_bytes([0xFF; 64])), PalwScoringKindV1::Pairwise);
            self.improvement_eval_jobs
                .range(lo..=hi)
                .filter(|((_, _, _, _, kind), _)| *kind == PalwScoringKindV1::ExactMatch)
                .all(|(_, row)| !self.improvement_eval_row_pending_v1(row))
        })
    }
}

/// The key range of one epoch's jobs.
fn palw_improve_eval_epoch_range_v1(line_id: &Hash64, epoch: u64) -> std::ops::RangeInclusive<PalwEvalJobKeyV1> {
    let lo = (*line_id, epoch, 0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch);
    let hi = (*line_id, epoch, u32::MAX, PalwEvalSubjectV1::Previous(Hash64::from_bytes([0xFF; 64])), PalwScoringKindV1::Pairwise);
    lo..=hi
}

/// **The core's claim predicate** (spec 17 §17.4.5): an evaluation claim is a free-prompt claim with no
/// quanta — the lane refuses sub-quantum work at acceptance (`ZeroQuanta`), so only the evaluation
/// branch writes one — and it never counts toward usage.
pub(super) fn palw_improve_claim_is_evaluation_v1(claim: &PalwClaimStateV2) -> bool {
    matches!(claim.source, PalwClaimSourceV2::FreePrompt { quanta: 0, .. })
}

/// The index's rows, from the table (every load and delta path).
pub(super) fn palw_improve_eval_claims_index_v1(
    jobs: &BTreeMap<PalwEvalJobKeyV1, PalwEvalJobStateV1>,
) -> BTreeMap<Hash64, PalwEvalJobKeyV1> {
    jobs.iter().filter_map(|(key, row)| row.claim.map(|c| (c.claim_id, *key))).collect()
}

/// **The table's consistency** (spec 17 §17.3): every row under its own job's key, under an existing
/// epoch.
pub(super) fn palw_improve_eval_carriage_consistent_v1(
    jobs: &BTreeMap<PalwEvalJobKeyV1, PalwEvalJobStateV1>,
    epochs: &BTreeMap<(Hash64, u64), PalwImprovementEpochV1>,
) -> Result<(), String> {
    for (key, row) in jobs {
        if row.job.key() != *key {
            return Err(format!("evaluation job {:?}: a row under another key", key.2));
        }
        if !epochs.contains_key(&(key.0, key.1)) {
            return Err(format!("evaluation job of {}/{}: no such epoch", key.0, key.1));
        }
    }
    Ok(())
}

// ---- the commitment ----------------------------------------------------------------------------

/// **What the free-prompt arm hands the evaluation branch** — the object's fields it reads.
pub(super) struct PalwEvalCommitV1<'a> {
    pub claim_id: &'a Hash64,
    pub class_id: &'a Hash64,
    pub bond: &'a PalwBondKeyV2,
    pub work_leaves: u64,
    pub prompt_token_ids: &'a [u32],
    pub decode_tokens_executed: u32,
    pub trace_root: &'a Hash64,
    pub output_root: &'a Hash64,
    pub execution_root: &'a Hash64,
    pub job_pin: &'a Hash64,
    pub eval: &'a PalwFpEvalCarriageV1,
}

/// The policy's stage parameters for a kind, as the context reads them.
fn palw_improve_eval_stage_params_of_v1(policy: &PalwImprovementPolicyV1, kind: PalwScoringKindV1) -> Option<PalwEvalStageParamsV1> {
    policy.eval.stages.iter().find(|stage| stage.kind == kind).map(|stage| match stage.params {
        PalwScoringParamsV1::ExactMatch { open, close, .. } => PalwEvalStageParamsV1::ExactMatch { open, close },
        PalwScoringParamsV1::RefLogLik { logit_scale_q24 } => PalwEvalStageParamsV1::RefLogLik { logit_scale_q24 },
        PalwScoringParamsV1::Judge { lo, hi } => PalwEvalStageParamsV1::Judge { lo, hi },
        PalwScoringParamsV1::Pairwise { margin } => PalwEvalStageParamsV1::Pairwise { margin },
    })
}

/// **The free-prompt arm's evaluation branch** (MIP-17, the commitment): see the module doc. The arm
/// has already checked the bond — it exists, the key is its own, it is not retiring, frozen or below
/// the producer floor — and this branch returns the arm's result.
pub(super) fn apply_improvement_eval_commitment_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    c: PalwEvalCommitV1<'_>,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if !builder.params.improve_active_at(daa) {
        return Err(refused("an evaluation claim below palw_improvement_v1"));
    }
    let job = &c.eval.job;
    let tail = &c.eval.tail;
    let (line_id, epoch, item, subject, kind) = job.key();
    // The epoch evaluates, before t_eval.
    let header = builder.state.improvement_epoch(&line_id, epoch).cloned().ok_or_else(|| refused("no such epoch"))?;
    if header.state != PalwEpochStateV1::Evaluating {
        return Err(refused("an evaluation claim outside the epoch's evaluation"));
    }
    if daa >= header.times.t_eval {
        return Err(refused("an evaluation claim at or after t_eval"));
    }
    // The item is drawn and not dropped; the subject is the epoch's; a pairwise job is not the parent's.
    let item_row = *builder.state.improvement_item(&line_id, epoch, item).ok_or_else(|| refused("an item the epoch did not draw"))?;
    if item_row.dropped {
        return Err(refused("an item dropped for every subject"));
    }
    if !builder.state.improvement_subjects(&line_id, epoch).contains(&subject) {
        return Err(refused("a subject the epoch does not evaluate"));
    }
    if kind == PalwScoringKindV1::Pairwise && subject == PalwEvalSubjectV1::Parent {
        return Err(refused("a pairwise job compares a subject with the parent, never the parent"));
    }
    // The subject's class is the claim's.
    let subject_class = match subject {
        PalwEvalSubjectV1::Parent => header.parent,
        PalwEvalSubjectV1::Candidate(class) | PalwEvalSubjectV1::Previous(class) => class,
    };
    if subject_class != *c.class_id {
        return Err(refused("the claim's class is not its subject's"));
    }
    // The kind is the policy's, and the mode the one the policy and the item fix.
    let policy = builder.state.improvement_policy(&line_id).cloned().ok_or_else(|| refused("the line has no policy"))?;
    let params = palw_improve_eval_stage_params_of_v1(&policy, kind).ok_or_else(|| refused("a kind the policy does not score"))?;
    match &job.mode {
        PalwEvalModeV1::Generate { seed, max_new, stop_ids } => {
            if *seed != item_row.seed || *max_new != policy.eval.max_new_tokens || *stop_ids != policy.eval.stop_ids {
                return Err(refused("a generating job's seed, budget and stops are the item's and the policy's"));
            }
            // One job per stage that applies to the item's kind (spec 17 §17.8.2): a likelihood item
            // is teacher-forced over its reference and takes no generation — a generation claim on it
            // could never be scored, and would hold the epoch's scoring until `t_score`.
            if item_row.source == PalwItemSourceV1::HoldOut
                && builder.state.improvement_case(&line_id, &item_row.case_id).is_some_and(|case| {
                    matches!(case.case.reference, crate::palw_improve_material_v1::PalwCaseReferenceV1::Continuation { .. })
                })
            {
                return Err(refused("a likelihood item takes no generation job"));
            }
        }
        PalwEvalModeV1::TeacherForced { .. } => {
            let reference = palw_improve_eval_item_reference_hook_v1(&builder.state, &line_id, epoch, item)
                .ok_or_else(|| refused("the item's reference is not disclosed"))?;
            if tail.generated != reference {
                return Err(refused("a teacher-forced job's ids are the item's reference"));
            }
        }
        PalwEvalModeV1::Judged { .. } => return Err(refused("judged kinds wait for the judge set's class kind")),
    }
    // The prompt is the item's.
    let prompt = palw_improve_eval_item_prompt_hook_v1(&builder.state, &line_id, epoch, item)
        .ok_or_else(|| refused("the item's prompt is not disclosed"))?;
    if c.prompt_token_ids != prompt.as_slice() {
        return Err(refused("the claim's prompt is not the item's"));
    }
    // The stage parameters are the policy's; the roots are the tail's (the extractor checked them; the
    // fold holds them again, over the policy's parameters and the item's prompt).
    if tail.params != params {
        return Err(refused("the claim's stage parameters are not the policy's"));
    }
    if tail.generated.len() as u64 != c.decode_tokens_executed as u64
        || palw_improve_eval_generated_root_v1(&tail.generated) != *c.output_root
        || tail.score.len() != palw_improve_eval_score_lanes_v1(kind)
        || palw_improve_eval_execution_root_v1(
            &job.id(),
            c.class_id,
            c.work_leaves,
            c.trace_root,
            &palw_improve_eval_prompt_root_v1(&prompt),
            prompt.len() as u32,
            &params,
            c.output_root,
            &palw_improve_eval_finalized_root_v1(&[]),
            &tail.score,
        ) != *c.execution_root
    {
        return Err(refused("the claim's roots are not its tail's"));
    }
    // **MIP-20: the epoch's evaluation budget** (spec 17 §17.8.2) — each job's positions (prompt and
    // stream ids) within its equal share of the policy's `max_eval_positions` and the network's ceiling.
    let ceilings = builder.params.improve_ceilings().ok_or_else(|| refused("an evaluation claim below palw_improvement_v1"))?;
    let epoch_jobs = palw_improve_eval_epoch_jobs_v1(&policy.eval, builder.state.improvement_subjects(&line_id, epoch).len());
    let job_cap =
        palw_improve_eval_job_position_cap_v1(palw_improve_eval_budget_positions_v1(policy.eval.max_eval_positions, &ceilings), epoch_jobs);
    if palw_improve_eval_positions_v1(prompt.len(), tail.generated.len()) > job_cap {
        return Err(refused("an evaluation claim past its job's share of the epoch's evaluation budget (MIP-20)"));
    }
    // The subject is an IR class, its layout the one its id binds, and the claim's leaves the chain's
    // count of the derived context.
    let record =
        builder.state.tir_classes.get(c.class_id).cloned().ok_or_else(|| refused("the subject's class is not an IR class"))?;
    if palw_improve_eval_layout_digest_v1(&tail.subject_layout) != record.layout_digest {
        return Err(refused("the carried layout is not the subject class's"));
    }
    let class = builder.state.classes.get(c.class_id).cloned().ok_or(PalwStateV2Error::MissingClass(*c.class_id))?;
    // **Panels: who can seat the claim** (RFC-0004 §13). An evaluation claim is seated as any claim of its
    // class is — drawn from the seats ready for that class — so the class must be one the chain serves: not
    // frozen, admitted by the registry where it governs, and verifiable inside the windows.
    if let PalwClassStatusV2::Frozen { .. } = class.status {
        return Err(PalwStateV2Error::FrozenClass(*c.class_id));
    }
    if let Some(state) = builder.read().class_lifecycle_refusal(c.class_id) {
        return Err(PalwStateV2Error::ClassNotAdmitting { class: *c.class_id, state });
    }
    builder.check_class_verify_admits_v1(
        c.class_id,
        daa,
        crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::FreePrompt { work_leaves: c.work_leaves },
        true,
    )?;
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(&record.program).map_err(|_| refused("the subject's program"))?;
    let subject_row = PalwEvalSubjectClassV1 {
        class_id: *c.class_id,
        artifact_root: class.artifact_root,
        program: &program,
        layout: &tail.subject_layout,
    };
    let context = palw_improve_eval_context_v1(job, &subject_row, params).map_err(|_| refused("the job's context does not derive"))?;
    let leaves = palw_improve_eval_step_leaves_v1(&context, &prompt, &tail.generated, &[])
        .map_err(|_| refused("the claim's step space does not derive"))?;
    if leaves != c.work_leaves as u128 {
        return Err(refused("the claim's work_leaves are not the chain's count of its context"));
    }
    // Open claiming: the first claim the chain holds takes the job.
    let key = job.key();
    let mut row =
        builder.state.improvement_eval_jobs.get(&key).cloned().unwrap_or(PalwEvalJobStateV1 { job: job.clone(), claim: None });
    let state = &builder.state;
    palw_improve_eval_row_release_dead_v1(&mut row, |claim| state.improvement_eval_claim_held_v1(claim));
    let answer = match params {
        PalwEvalStageParamsV1::ExactMatch { open, close } => palw_improve_answer_of_v1(&tail.generated, open, close),
        _ => None,
    };
    palw_improve_eval_take_v1(&mut row, *c.claim_id, *c.bond, daa, header.times.t_eval, answer).map_err(|e| match e {
        PalwEvalErrorV1::Taken { .. } => refused("the evaluation job is taken by a live claim"),
        _ => refused("an evaluation claim at or after t_eval"),
    })?;
    if kind != PalwScoringKindV1::ExactMatch {
        let value = palw_improve_eval_score_value_v1(kind, &tail.score).map_err(|_| refused("the committed score"))?;
        if !palw_improve_eval_score_in_range_v1(&params, value) {
            return Err(refused("a committed score outside its kind's range"));
        }
        if let Some(claim) = row.claim.as_mut() {
            claim.score = Some(value);
        }
    }
    // The reservation: what a claim of this work on the class reserves, under ADR-0160's stage 1.
    let raw = palw_improve_eval_reservation_v1(c.work_leaves, class.slash_value_per_pwu, 1);
    let reserved = crate::palw_weight_cap_v1::palw_claim_weight_reservation_of_v1(builder.params, 0, 0, raw, daa);
    let bond_record = builder.state.bonds.get(c.bond).cloned().ok_or(PalwStateV2Error::MissingBond(*c.bond))?;
    // The free-prompt arm's exposure ceiling, as it asks it.
    let declared = if builder.capability_bound { palw_bond_capability_exposure_v1(&bond_record) } else { 0 };
    let own = if builder.params.rcore_plus_active_at(daa) {
        builder.committed_at(c.bond, daa)
    } else {
        builder
            .state
            .reserved_exposure(c.bond)
            .checked_add(builder.state.registration_exposure(c.bond))
            .ok_or(PalwStateV2Error::Overflow("total exposure"))?
    };
    let backed = own.checked_add(declared).ok_or(PalwStateV2Error::Overflow("total exposure"))?;
    let ceiling = if builder.params.rcore_plus_active_at(daa) {
        own.saturating_add(builder.gate_room(c.bond, daa, PalwRcoreGateV1::Work))
    } else {
        (bond_record.collateral as u128)
            .checked_mul(builder.params.fp_max_exposure_ratio_permille as u128)
            .ok_or(PalwStateV2Error::Overflow("exposure ceiling"))?
            / 1000
    };
    let would_reserve = backed.checked_add(reserved).ok_or(PalwStateV2Error::Overflow("reserved exposure"))?;
    if would_reserve > ceiling {
        return Err(PalwStateV2Error::FreePromptExposureCeiling { bond: *c.bond, backed, claim: reserved, ceiling });
    }
    // **The evaluation's share of the executor's claim capacity** (RFC-0004 §13: evaluation cannot crowd
    // out attempts): one evaluation claim may reserve at most the fence's `max_eval_budget_permille` of
    // the room the executor's bond carries.
    if reserved.saturating_mul(1_000) > ceiling.saturating_mul(ceilings.max_eval_budget_permille as u128) {
        return Err(refused("an evaluation claim reserves more than the evaluation budget's share of its executor's claim capacity"));
    }
    // MIP-17: no quanta, no pwu, no receipt rights, no weight, no escrow; the DA trio the chain's.
    let claim = PalwClaimStateV2 {
        source: PalwClaimSourceV2::FreePrompt { quanta: 0, spent: BTreeSet::new() },
        class_id: *c.class_id,
        bond: *c.bond,
        pwu: 0,
        accepted_daa: daa,
        rebound_daa: None,
        accepted_blue_score: ctx.blue_score,
        accepted_block: ctx.block,
        trace_root: *c.trace_root,
        output_root: *c.output_root,
        execution_root: *c.execution_root,
        trace_chunk_count: 1,
        trace_retention_daa: daa.saturating_add(crate::palw_producer_v2::palw_min_trace_retention_daa_v1(builder.params)),
        reserved,
        immature_contribution: 0,
        escrowed_reward: 0,
        work_leaves: c.work_leaves,
        work_id: None,
        phase: PalwClaimPhaseV2::Provisional,
        rights_reserved: 0,
        job_identity: if builder.extras.offence_attribution_active { *c.job_pin } else { Hash64::default() },
        rcore: PalwClaimRcoreV1::default(),
    };
    builder.reserve_for_claim(&claim)?;
    builder.write_claim(*c.claim_id, Some(claim));
    builder.write_improvement_eval_job(key, Some(row));
    let deadline = daa.checked_add(builder.params.window_bind).ok_or(PalwStateV2Error::Overflow("bind deadline"))?;
    builder.arm_deadline(deadline, *c.claim_id);
    Ok(())
}

// ---- the builder's helpers -----------------------------------------------------------------------

impl TransitionBuilder<'_> {
    /// **RFC-0004 A6: the one writer of `improvement_eval_jobs`** (`ImprovementRow`, table 13), keeping
    /// the claim index in step.
    pub(crate) fn write_improvement_eval_job(&mut self, key: PalwEvalJobKeyV1, new: Option<PalwEvalJobStateV1>) {
        let old = self.write_improvement_row(PALW_IMPROVE_TABLE_EVAL_JOBS_V1, key, new.clone(), |s| &mut s.improvement_eval_jobs);
        if let Some(claim) = old.and_then(|row| row.claim) {
            self.state.improvement_eval_claims.remove(&claim.claim_id);
        }
        if let Some(claim) = new.and_then(|row| row.claim) {
            self.state.improvement_eval_claims.insert(claim.claim_id, key);
        }
    }

    /// Does the epoch take scores now (spec 17 §17.8.3), for this item and subject?
    fn improvement_eval_takes_score_v1(
        &self,
        line_id: &Hash64,
        epoch: u64,
        item: u32,
        subject: &PalwEvalSubjectV1,
        kind: PalwScoringKindV1,
    ) -> bool {
        let Some(header) = self.state.improvement_epoch(line_id, epoch) else { return false };
        matches!(header.state, PalwEpochStateV1::Evaluating | PalwEpochStateV1::Closing)
            && item < header.items
            && self.state.improvement_subjects(line_id, epoch).contains(subject)
            && self
                .state
                .improvement_result(line_id, epoch, item, subject)
                .is_none_or(|result| result.scores.iter().all(|score| score.kind != kind))
    }

    /// **MIP-17 at `Final`** (spec 17 §17.8.3, §17.11.2): an evaluation claim's row records its
    /// finality; a kind that committed a score records it; the subject's escrow pays the fee. Nothing
    /// for any other claim, nor for one whose job has retired or whose epoch no longer takes scores (a
    /// claim final after `t_score` earns neither: the escrow is the epoch's, and returns with it).
    pub(super) fn note_improvement_eval_final_v1(
        &mut self,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
        final_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !palw_improve_claim_is_evaluation_v1(claim) {
            return Ok(());
        }
        let Some((key, row)) = self.state.improvement_eval_job_of_claim(claim_id) else { return Ok(()) };
        let mut row = row.clone();
        let Some(mut taken) = row.claim.filter(|c| c.claim_id == *claim_id && c.final_daa.is_none()) else { return Ok(()) };
        taken.final_daa = Some(final_daa);
        row.claim = Some(taken);
        self.write_improvement_eval_job(key, Some(row));
        let (line_id, epoch, item, subject, kind) = key;
        let scoring = self.improvement_eval_takes_score_v1(&line_id, epoch, item, &subject, kind);
        if !scoring {
            return Ok(());
        }
        if let Some(value) = taken.score {
            self.record_improvement_score_v1(&line_id, epoch, item, subject, PalwEvalScoreV1 { kind, value })?;
        }
        self.pay_improvement_eval_fee_v1(&line_id, epoch, &subject, &claim.bond)?;
        Ok(())
    }

    /// **ExactMatch at a key's reveal** (spec 17 §17.8.3) — what the material lane's reveal arms call,
    /// once per revealed item: every subject whose generation is final is scored against `key`
    /// ([`palw_improve_exact_match_score_v1`]) and recorded through `record_improvement_score_v1`; a
    /// subject without one records nothing, and is missing (it counts for the incumbent). Refused while
    /// the item's generations are not settled ([`PalwChainStateV2::improvement_eval_generations_settled_v1`]).
    pub(crate) fn score_improvement_exact_match_v1(
        &mut self,
        line_id: &Hash64,
        epoch: u64,
        item: u32,
        key: &[u32],
    ) -> Result<(), PalwStateV2Error> {
        if !self.state.improvement_eval_generations_settled_v1(line_id, epoch, &[item]) {
            return Err(refused("a key revealed before every subject's generation of its item is settled"));
        }
        let policy = self.state.improvement_policy(line_id).ok_or_else(|| refused("the line has no policy"))?;
        if let Some(PalwScoringParamsV1::ExactMatch { key_cap, .. }) =
            policy.eval.stages.iter().find(|s| s.kind == PalwScoringKindV1::ExactMatch).map(|s| s.params)
            && key.len() > key_cap as usize
        {
            return Err(refused("a revealed key longer than the policy's key cap"));
        }
        for subject in self.state.improvement_subjects(line_id, epoch) {
            let kind = PalwScoringKindV1::ExactMatch;
            let Some(row) = self.state.improvement_eval_job(line_id, epoch, item, &subject, kind).cloned() else { continue };
            let Some(value) = palw_improve_exact_match_score_v1(&row, key) else { continue };
            if !self.improvement_eval_takes_score_v1(line_id, epoch, item, &subject, kind) {
                continue;
            }
            let score = PalwEvalScoreV1 { kind, value };
            self.record_improvement_score_v1(line_id, epoch, item, subject, score)?;
        }
        Ok(())
    }

    /// The bounded retirement of a decided epoch's job rows (spec 17 §17.5.4, beside the core's detail
    /// rows): at most `budget` rows; returns how many it deleted and whether any remain.
    pub(super) fn retire_improvement_eval_jobs_v1(&mut self, line_id: &Hash64, epoch: u64, budget: usize) -> (usize, bool) {
        let keys: Vec<PalwEvalJobKeyV1> = self
            .state
            .improvement_eval_jobs
            .range(palw_improve_eval_epoch_range_v1(line_id, epoch))
            .map(|(k, _)| *k)
            .take(budget)
            .collect();
        for key in &keys {
            self.write_improvement_eval_job(*key, None);
        }
        let remaining = self.state.improvement_eval_jobs.range(palw_improve_eval_epoch_range_v1(line_id, epoch)).next().is_some();
        (keys.len(), remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_improve_epoch_v1::PalwMaterialKindV1;
    use crate::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1;
    use crate::palw_tir_admission_v1::PalwTirClassRecordV1;
    use crate::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
    use crate::tx::TransactionOutpoint;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::interp::{MapParams, ParamSource};
    use misaka_palw_tir::pipeline::PipelineParams;
    use misaka_palw_tir::{DType, Ref, Tensor, TensorType, TirProgramV1};

    const ACTIVE: u64 = 100;
    const OWNER: u8 = 1;
    const ALICE: u8 = 2;
    const BOB: u8 = 3;
    const CAROL: u8 = 4;
    const LINE: u8 = 0x10;
    const CAND_A: u8 = 0x11;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn bond(byte: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: h(byte), index: 0 })
    }

    fn params() -> PalwStateParamsV2 {
        let p = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
        bundle.state.clone().with_improve_from_daa(Some(ACTIVE)).with_improve_ceilings(Some(PALW_DRILL_IMPROVE_CEILINGS_V1))
    }

    /// One ExactMatch stage, eight items, a four-id budget and no stops.
    fn policy() -> PalwImprovementPolicyV1 {
        let mut p = crate::palw_improve_policy_v1::palw_improvement_policy_example_v1();
        p.eval.n = 8;
        p.eval.n_min = 4;
        p.eval.regression_items = 0;
        p.eval.regression_suite_root = Hash64::default();
        p.eval.safety_items = 0;
        p.eval.safety_suite_root = Hash64::default();
        p.eval.stages.truncate(1);
        p.eval.setter_cap_permille = 1_000;
        p.eval.max_new_tokens = 4;
        p.eval.stop_ids = vec![];
        p.eval.stages[0].params = PalwScoringParamsV1::ExactMatch { open: -1, close: -1, key_cap: 4 };
        p.usage.value = 2;
        p.k_max = 2;
        p
    }

    /// A toy IR class: an `i8` embedding, one layer of an `i8 [4, 4]` matrix and `i64` multiplier, an
    /// `i16` head over 16 ids (tiled logits).
    fn program() -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(16, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
        let table = pb.param("embed.table", DType::I8, &[16, 4], false);
        let w = pb.param("blk.w", DType::I8, &[4, 4], true);
        let m = pb.param("blk.m", DType::I64, &[4], true);
        let head = pb.param("head.w", DType::I16, &[16, 4], false);
        let carry = vec![TensorType::fixed(DType::I32, &[4])];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let row = b.gather(table, Ref::Input(0), 0, 0);
            let row = b.cast(row, DType::I32);
            b.finish(&[row])
        };
        let layer = {
            let mut b = pb.block("layer", carry.clone());
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let acc = b.matmul(w, x, DType::I64);
            let acc = b.reshape_fixed(acc, &[4]);
            let y = b.mul(acc, m, DType::I128);
            let y = b.shr(y, 20, misaka_palw_tir::Rounding::HalfAwayFromZero, DType::I128);
            let y = b.clamp(y, -30_000, 30_000, DType::I32);
            b.finish(&[y])
        };
        let post = {
            let mut b = pb.block("post", carry);
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let l = b.matmul(head, x, DType::I64);
            let l = b.reshape_fixed(l, &[16]);
            let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
            b.commit(l);
            b.finish(&[])
        };
        let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
        let mut p = pb.finish(pre, vec![layer], post, logits);
        p.logits_scheme_id.copy_from_slice(crate::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
        p
    }

    fn weights(p: &TirProgramV1, salt: usize) -> MapParams {
        let mut out = MapParams::default();
        for (j, inst) in crate::palw_tir_artifact_v1::palw_tir_param_instances_v1(p).into_iter().enumerate() {
            let d = &p.params[j];
            for l in inst {
                let n: usize = d.shape.iter().map(|x| *x as usize).product();
                let data: Vec<i128> = (0..n)
                    .map(|i| {
                        let v = ((i * 37 + j * 11 + salt * 7 + l.map_or(0, |l| l as usize) * 5) % 200) as i128 - 100;
                        if d.dtype == DType::I64 { v.abs() * 9_000 + 1 } else { v }
                    })
                    .collect();
                out.tensors.insert((j as u16, l), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
            }
        }
        out
    }

    fn layout(p: &TirProgramV1) -> PalwTirLayoutV1 {
        let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
        PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 12,
            checkpoint_interval: 1,
            h_tile: 16,
            commit_tiles: vec![4; commits],
            state_tiles: vec![4; p.states.len()],
        }
    }

    struct Weights(Vec<MapParams>);
    impl PipelineParams for Weights {
        fn params(&self, program: u16) -> &dyn ParamSource {
            &self.0[program as usize]
        }
    }

    /// The line's class and one candidate's: the same program, other weights.
    fn genesis() -> PalwChainStateV2 {
        let mut s = PalwChainStateV2::genesis();
        let p = program();
        let bytes = p.encode();
        for class in [LINE, CAND_A] {
            let mut row = PalwTirClassRecordV1::test_row_v1(h(class));
            row.graph_ir_root = crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&bytes);
            row.program_bytes = bytes.len() as u32;
            row.program = std::sync::Arc::new(bytes.clone());
            row.layout_digest = palw_improve_eval_layout_digest_v1(&layout(&p));
            s.tir_classes.insert(h(class), row);
            s.classes.insert(
                h(class),
                PalwClassStateV2 {
                    artifact_root: h(class),
                    slash_value_per_pwu: 3,
                    pwu_rule: PalwPwuRuleV2::MaxPerAttempt(100),
                    status: PalwClassStatusV2::Active,
                    registered_daa: 0,
                    registrant_bond: Some(bond(OWNER)),
                    fused_attention: false,
                },
            );
            s.class_shares.insert(h(class), 500);
        }
        s.model_lines.insert(h(LINE), crate::palw_model_lines_v1::founding_line_v1(h(LINE), Some(bond(OWNER)), b"line".to_vec(), 0));
        for who in [OWNER, ALICE, BOB, CAROL] {
            s.bonds.insert(
                bond(who),
                palw_bond_state_from_registration_v2(&[who], &[who], 1_000_000_000_000_000, h(0x80 + who), 0, Default::default()),
            );
        }
        s
    }

    fn ctx(daa: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h((daa % 251) as u8), daa_score: daa, blue_score: daa, subsidy: 0 }
    }

    /// Run `f` on a builder over `state` at `daa`, after that block's sweep: the state and the delta.
    fn at(
        state: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        daa: u64,
        f: impl FnOnce(&mut TransitionBuilder<'_>),
    ) -> (PalwChainStateV2, PalwStateDeltaV2) {
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(state, p, false, false, false, false, &extras);
        crate::palw_state_v2::palw_improve_fold_v1::advance_improvement_v1(&mut builder, &ctx(daa)).expect("the sweep");
        f(&mut builder);
        let delta = PalwStateDeltaV2 { point: ctx(daa), entries: builder.entries.clone() };
        (builder.checkpoint().0, delta)
    }

    /// An epoch evaluating at 1,510: the line opted in at 500, candidate A in, eight hold-out items,
    /// their prompts disclosed.
    fn evaluating() -> PalwChainStateV2 {
        evaluating_with(&params(), policy())
    }

    /// [`evaluating`] under other params (the fence's ceilings) and another policy.
    fn evaluating_with(p: &PalwStateParamsV2, policy: PalwImprovementPolicyV1) -> PalwChainStateV2 {
        let p = p.clone();
        let set = PalwImprovementPolicySetV1 { line_id: h(LINE), sequence: 1, policy: Some(policy) };
        let (mut s, _) = at(&genesis(), &p, 500, |b| {
            crate::palw_state_v2::palw_improve_fold_v1::apply_improvement_policy_set_v1(b, &ctx(500), &set).expect("opt in")
        });
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        let (s, _) = at(&s, &p, 1_000, |b| {
            b.note_improvement_material_v1(&h(LINE), PalwMaterialKindV1::Dataset, &h(0x40), &bond(CAROL), 1_000).unwrap();
        });
        let (s, _) = at(&s, &p, 1_200, |b| {
            b.admit_improvement_candidate_v1(
                &h(LINE),
                1,
                &h(CAND_A),
                &bond(ALICE),
                crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(CAND_A) },
                h(0x50),
                Vec::new(),
                1_200,
            )
            .unwrap();
        });
        let (s, _) = at(&s, &p, 1_400, |b| {
            for i in 0..8u8 {
                b.note_improvement_material_v1(&h(LINE), PalwMaterialKindV1::HardCase, &h(0x60 + i), &bond(CAROL), 1_400).unwrap();
            }
        });
        let (s, _) = at(&s, &p, 1_510, |_| {});
        assert_eq!(s.improvement_epoch(&h(LINE), 1).unwrap().state, PalwEpochStateV1::Evaluating);
        for item in 0..8u32 {
            test_disclosure::set(h(LINE), 1, item, vec![3, 5, item % 16], None);
        }
        s
    }

    /// An honest ExactMatch claim of `subject` on `item`, by `executor`: the object the extractor
    /// builds from the payload.
    fn claim(s: &PalwChainStateV2, item: u32, subject: PalwEvalSubjectV1, executor: u8, claim_word: u8) -> PalwConsensusObjectV2 {
        let class = match subject {
            PalwEvalSubjectV1::Parent => h(LINE),
            PalwEvalSubjectV1::Candidate(c) | PalwEvalSubjectV1::Previous(c) => c,
        };
        let item_row = s.improvement_item(&h(LINE), 1, item).unwrap();
        let job = PalwEvalJobV1 {
            line_id: h(LINE),
            epoch: 1,
            item,
            subject,
            kind: PalwScoringKindV1::ExactMatch,
            mode: PalwEvalModeV1::Generate { seed: item_row.seed, max_new: 4, stop_ids: vec![] },
        };
        let p = program();
        let layout = layout(&p);
        let subject_row = PalwEvalSubjectClassV1 { class_id: class, artifact_root: class, program: &p, layout: &layout };
        let context =
            palw_improve_eval_context_v1(&job, &subject_row, PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 }).unwrap();
        let prompt = vec![3, 5, item % 16];
        let probe = misaka_palw_tir::pipeline::PipelineJob { prompt: prompt.clone(), ..Default::default() };
        let salt = if class == h(LINE) { 0 } else { 1 };
        let e = crate::palw_gen_worker_v1::palw_gen_execute_v1(
            &context.pipeline,
            &context.programs,
            &context.layouts,
            &Weights(vec![weights(&p, salt)]),
            &probe,
            context.decode.as_ref().unwrap(),
            context.seed,
        )
        .unwrap();
        let params = PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 };
        let binding =
            PalwEvalBindingV1::of(&job, class, &layout, &e.claim, e.space.leaf_count(), &probe.prompt, params, vec![], vec![]);
        let roots = binding.claim_roots();
        PalwConsensusObjectV2::FreePromptCommitted {
            claim: h(claim_word),
            class_id: class,
            bond: bond(executor),
            executor_pubkey: vec![executor],
            work_leaves: roots.work_leaves,
            prompt_token_ids_hash: Hash64::default(),
            prompt_tokens: prompt.len() as u32,
            prompt_token_ids: prompt,
            decode_tokens_executed: e.claim.generated.len() as u32,
            trace_root: roots.trace_root,
            output_root: roots.output_root,
            execution_root: roots.execution_root,
            trace_chunk_count: 1,
            trace_retention_daa: 0,
            consumed_prefix_state: crate::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(class),
            job_pin: Hash64::default(),
            eval: Some(Box::new(PalwFpEvalCarriageV1 {
                job,
                tail: PalwEvalClaimTailV1 { generated: e.claim.generated.clone(), score: vec![], subject_layout: layout, params },
            })),
        }
    }

    fn generated_of(object: &PalwConsensusObjectV2) -> Vec<u32> {
        let PalwConsensusObjectV2::FreePromptCommitted { eval: Some(eval), .. } = object else { panic!("an evaluation claim") };
        eval.tail.generated.clone()
    }

    #[test]
    fn an_evaluation_claim_takes_its_job_off_the_reward_path() {
        let p = params();
        let s = evaluating();
        let first = claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xA0);
        let (s1, delta) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &first).expect("an honest evaluation claim"));
        let row = s1.improvement_eval_job(&h(LINE), 1, 0, &PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch).unwrap();
        let taken = row.claim.unwrap();
        assert_eq!((taken.claim_id, taken.executor, taken.accepted_daa, taken.final_daa), (h(0xA0), bond(CAROL), 1_600, None));
        assert_eq!(taken.answer, palw_improve_answer_of_v1(&generated_of(&first), -1, -1), "the answer span is kept for the reveal");
        let claim_row = s1.claims.get(&h(0xA0)).unwrap();
        assert!(matches!(claim_row.source, PalwClaimSourceV2::FreePrompt { quanta: 0, .. }), "no quanta");
        assert_eq!(
            (claim_row.pwu, claim_row.immature_contribution, claim_row.escrowed_reward, claim_row.rights_reserved),
            (0, 0, 0, 0)
        );
        assert_eq!(claim_row.reserved, palw_improve_eval_reservation_v1(claim_row.work_leaves, 3, 1), "leaves × slash, ρ = 1");
        assert!(claim_row.reserved > 0, "it holds collateral");
        assert_eq!((claim_row.trace_chunk_count, claim_row.work_id), (1, None));
        assert!(palw_improve_claim_is_evaluation_v1(claim_row), "the core's predicate");
        assert_eq!(s1.improvement_eval_job_of_claim(&h(0xA0)).map(|(k, _)| k.2), Some(0));
        assert!(s1.improvement_eval_pending_v1(&h(LINE), 1), "a live claim, not yet final");
        // The delta reverts to the parent and replays to the child; the carriage round-trips with its index.
        let back = revert_delta_v2(&s1, &delta, &p).unwrap();
        assert_eq!(back.state_root(), s.state_root());
        assert!(back.improvement_eval_jobs.is_empty() && back.improvement_eval_claims.is_empty());
        let again = apply_delta_v2(&s, &delta, &p).unwrap();
        assert_eq!(again.state_root(), s1.state_root());
        assert_eq!(again.improvement_eval_claims, s1.improvement_eval_claims, "the index follows the rows");
        // The carriage carries the table in its own tail (0xCC), and a load rebuilds the index.
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s1)).unwrap();
        let tail = [&[0xCCu8][..], &borsh::to_vec(&s1.improvement_eval_jobs).unwrap()].concat();
        assert!(bytes.windows(tail.len()).any(|w| w == tail.as_slice()), "the table rides its own tail");
        let decoded = <PalwStateCarriageV2 as borsh::BorshDeserialize>::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.improvement_eval_jobs, s1.improvement_eval_jobs, "the tail round-trips");
        let mut rebuilt = s1.clone();
        rebuilt.improvement_eval_claims.clear();
        super::super::rebuild_improvement_indices_v1(&mut rebuilt);
        assert_eq!(rebuilt.improvement_eval_claims, s1.improvement_eval_claims, "the index is the rows'");
        assert_ne!(s1.state_root(), s.state_root(), "the job row is rooted");

        // A second claim on the job is refused while the first lives; any claim past t_eval is refused.
        let second = claim(&s1, 0, PalwEvalSubjectV1::Parent, BOB, 0xA1);
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(&s1, &p, false, false, false, false, &extras);
        assert_eq!(
            apply_object(&mut b, &ctx(1_601), &second),
            Err(PalwStateV2Error::ImprovementRefused("the evaluation job is taken by a live claim"))
        );
        let late = claim(&s1, 1, PalwEvalSubjectV1::Parent, BOB, 0xA2);
        let mut b = TransitionBuilder::new(&s1, &p, false, false, false, false, &extras);
        assert!(apply_object(&mut b, &ctx(1_800), &late).is_err(), "at t_eval");
        // A voided first claim frees the job.
        let mut voided = s1.clone();
        voided.claims.get_mut(&h(0xA0)).unwrap().phase =
            PalwClaimPhaseV2::Voided { voided_daa: 1_650, reason: PalwVoidReasonV2::ReceiptTimeout };
        let mut b = TransitionBuilder::new(&voided, &p, false, false, false, false, &extras);
        assert_eq!(apply_object(&mut b, &ctx(1_700), &second), Ok(()), "the job is re-taken");
    }

    #[test]
    fn a_claim_that_is_not_its_job_s_is_refused_by_name() {
        let p = params();
        let s = evaluating();
        let extras = PalwTransitionExtrasV1::default();
        let refused_as = |object: &PalwConsensusObjectV2, daa: u64| {
            let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
            match apply_object(&mut b, &ctx(daa), object) {
                Err(PalwStateV2Error::ImprovementRefused(why)) => why,
                other => panic!("refused by name, got {other:?}"),
            }
        };
        let honest = claim(&s, 2, PalwEvalSubjectV1::Candidate(h(CAND_A)), CAROL, 0xB0);
        let tamper = |f: &dyn Fn(&mut PalwConsensusObjectV2)| {
            let mut o = honest.clone();
            f(&mut o);
            o
        };
        fn eval_of(o: &mut PalwConsensusObjectV2) -> &mut PalwFpEvalCarriageV1 {
            let PalwConsensusObjectV2::FreePromptCommitted { eval: Some(eval), .. } = o else { unreachable!() };
            eval
        }
        assert_eq!(refused_as(&honest, 1_800), "an evaluation claim at or after t_eval");
        let wrong_prompt = tamper(&|o| {
            let PalwConsensusObjectV2::FreePromptCommitted { prompt_token_ids, .. } = o else { unreachable!() };
            prompt_token_ids[0] = 9;
        });
        assert_eq!(refused_as(&wrong_prompt, 1_600), "the claim's prompt is not the item's");
        let wrong_class = tamper(&|o| {
            let PalwConsensusObjectV2::FreePromptCommitted { class_id, .. } = o else { unreachable!() };
            *class_id = h(LINE);
        });
        assert_eq!(refused_as(&wrong_class, 1_600), "the claim's class is not its subject's");
        let wrong_seed = tamper(&|o| {
            if let PalwEvalModeV1::Generate { seed, .. } = &mut eval_of(o).job.mode {
                *seed = h(0x99);
            }
        });
        assert_eq!(refused_as(&wrong_seed, 1_600), "a generating job's seed, budget and stops are the item's and the policy's");
        let wrong_kind = tamper(&|o| eval_of(o).job.kind = PalwScoringKindV1::Judge);
        assert_eq!(refused_as(&wrong_kind, 1_600), "a kind the policy does not score");
        let wrong_leaves = tamper(&|o| {
            let PalwConsensusObjectV2::FreePromptCommitted { work_leaves, .. } = o else { unreachable!() };
            *work_leaves += 1;
        });
        assert_eq!(refused_as(&wrong_leaves, 1_600), "the claim's roots are not its tail's", "the leaves are in the root");
        let wrong_ids = tamper(&|o| eval_of(o).tail.generated[0] ^= 1);
        assert_eq!(refused_as(&wrong_ids, 1_600), "the claim's roots are not its tail's");
        let wrong_layout = tamper(&|o| eval_of(o).tail.subject_layout.h_tile = 8);
        assert_eq!(refused_as(&wrong_layout, 1_600), "the carried layout is not the subject class's");
        let stranger = tamper(&|o| eval_of(o).job.subject = PalwEvalSubjectV1::Candidate(h(0x77)));
        assert_eq!(refused_as(&stranger, 1_600), "a subject the epoch does not evaluate");
        let far_item = tamper(&|o| eval_of(o).job.item = 99);
        assert_eq!(refused_as(&far_item, 1_600), "an item the epoch did not draw");
        // Below the fence nothing is an evaluation claim.
        let below = {
            let extras = PalwTransitionExtrasV1::default();
            let p = params().with_improve_from_daa(None);
            let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
            apply_object(&mut b, &ctx(1_600), &honest)
        };
        assert!(below.is_err(), "dormant below palw_improvement_v1");
    }

    #[test]
    fn final_generations_score_at_the_key_s_reveal_and_the_epoch_completes() {
        let p = params();
        let s = evaluating();
        let parent = claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xC0);
        let cand = claim(&s, 0, PalwEvalSubjectV1::Candidate(h(CAND_A)), BOB, 0xC1);
        let (s, _) = at(&s, &p, 1_600, |b| {
            apply_object(b, &ctx(1_600), &parent).unwrap();
            apply_object(b, &ctx(1_600), &cand).unwrap();
        });
        assert!(!s.improvement_eval_generations_settled_v1(&h(LINE), 1, &[0]), "not final yet");
        let escrow_before = s.improvement_epoch(&h(LINE), 1).unwrap().escrow;
        // Both finalise: the rows record it and the subjects' escrows pay the fees.
        let (s, _) = at(&s, &p, 1_650, |b| {
            for id in [h(0xC0), h(0xC1)] {
                let claim_row = b.state.claims.get(&id).cloned().unwrap();
                b.note_improvement_eval_final_v1(&id, &claim_row, 1_650).unwrap();
            }
        });
        let fee = policy().fees.eval_fee_per_job;
        let e = s.improvement_epoch(&h(LINE), 1).unwrap();
        assert_eq!(e.escrow.parent_spent, escrow_before.parent_spent + fee, "the parent's escrow paid its job");
        let (_, a) = s.improvement_candidate(&h(LINE), 1, &h(CAND_A)).unwrap();
        assert_eq!(a.escrow_spent, fee, "the candidate's escrow paid its job");
        assert!(s.improvement_earnings(&bond(CAROL)) >= fee && s.improvement_earnings(&bond(BOB)) >= fee);
        assert!(s.improvement_eval_generations_settled_v1(&h(LINE), 1, &[0]));
        assert!(s.improvement_eval_pending_v1(&h(LINE), 1), "final, but its key is not revealed: still pending");
        // The key's reveal: the key is the parent's generation, not the candidate's.
        let s_before_reveal = s.clone();
        let key = generated_of(&parent);
        assert_ne!(key, generated_of(&cand), "the premise: other weights, another answer");
        let (s, _) = at(&s, &p, 1_700, |b| b.score_improvement_exact_match_v1(&h(LINE), 1, 0, &key).unwrap());
        let score = |subject: PalwEvalSubjectV1| {
            s.improvement_result(&h(LINE), 1, 0, &subject).and_then(|r| r.scores.first().copied()).map(|sc| (sc.kind, sc.value))
        };
        assert_eq!(score(PalwEvalSubjectV1::Parent), Some((PalwScoringKindV1::ExactMatch, 1)));
        assert_eq!(score(PalwEvalSubjectV1::Candidate(h(CAND_A))), Some((PalwScoringKindV1::ExactMatch, 0)));
        assert!(!s.improvement_eval_pending_v1(&h(LINE), 1), "nothing left to come");
        // A second reveal of the same item is refused: a score of a kind is taken once.
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        assert!(b.score_improvement_exact_match_v1(&h(LINE), 1, 0, &key).is_ok(), "an already-scored subject is skipped, not refused");
        assert_eq!(
            b.state.improvement_result(&h(LINE), 1, 0, &PalwEvalSubjectV1::Parent).unwrap().scores.len(),
            1,
            "and records nothing twice"
        );
        // An item no claim reached has nothing pending (E17: its key opens in Closing, the reveal arm's check).
        assert!(s.improvement_eval_generations_settled_v1(&h(LINE), 1, &[1]));
        // At t_eval nothing is pending: the epoch closes and scores in the same block (spec 17
        // §17.5.3), the other seven items missing for every subject — so they count for the parent.
        let (done, _) = at(&s, &p, 1_800, |_| {});
        let e = done.improvement_epoch(&h(LINE), 1).unwrap();
        assert_eq!(e.state, PalwEpochStateV1::Decided);
        assert!(matches!(e.outcome, Some(PalwPromotionOutcomeV1::NoChange { .. })), "{:?}", e.outcome);
        // Had the key not been revealed, the final but unscored generations would have kept the epoch
        // closing, not scored, until t_score.
        let (unrevealed, _) = at(&s_before_reveal, &p, 1_800, |_| {});
        assert_eq!(unrevealed.improvement_epoch(&h(LINE), 1).unwrap().state, PalwEpochStateV1::Closing);
        assert!(unrevealed.improvement_eval_generations_settled_v1(&h(LINE), 1, &[1]), "closing: no live claim on it");
        let (late, _) = at(&unrevealed, &p, 1_950, |_| {});
        assert_eq!(late.improvement_epoch(&h(LINE), 1).unwrap().state, PalwEpochStateV1::Decided, "at t_score at the latest");
    }

    /// An evaluation claim's carrier as its executor would build it: the FP payload's bytes at job
    /// version 9, then the claim's tail — the transaction the extraction walk reads.
    fn carrier_of(object: &PalwConsensusObjectV2) -> crate::tx::Transaction {
        let PalwConsensusObjectV2::FreePromptCommitted {
            class_id,
            bond,
            executor_pubkey,
            work_leaves,
            prompt_token_ids,
            decode_tokens_executed,
            trace_root,
            output_root,
            execution_root,
            eval: Some(eval),
            ..
        } = object
        else {
            panic!("an evaluation claim")
        };
        let PalwEvalModeV1::Generate { max_new, stop_ids, .. } = &eval.job.mode else { panic!("a generating job") };
        let job = crate::palw_freeprompt_v3::PalwFreePromptJobV3 {
            version: PALW_FP_EVAL_VERSION,
            network_domain: h(0x10),
            class_id: *class_id,
            executor_bond: bond.0,
            executor_pubkey: executor_pubkey.clone(),
            operator_id: h(0x12),
            anchor_block: h(0x13),
            anchor_daa: 99,
            job_nonce: [0; 32],
            tokenizer_id: h(0x14),
            prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids),
            prompt_tokens: prompt_token_ids.len() as u32,
            decode_token_limit: *max_new,
            max_context_tokens: 64,
            privacy_mode: crate::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: [0; 32],
            temperature_q: 0,
            decode: Some(palw_improve_eval_decode_config_v1(stop_ids)),
            tail: Some(crate::palw_freeprompt_v3::PalwFpJobTailV1::Eval(Box::new(eval.job.clone()))),
        };
        let commitment = crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 {
            trace_root: *trace_root,
            output_root: *output_root,
            schedule_root: Hash64::default(),
            execution_root: *execution_root,
            decode_tokens_executed: *decode_tokens_executed,
            stop_reason: if decode_tokens_executed == max_new {
                crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached
            } else {
                crate::palw_freeprompt_v3::PalwFpStopReasonV3::EndOfGeneration
            },
            work_leaves: *work_leaves,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 0,
            job,
        };
        let payload = crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            commitment,
            prompt_token_ids: prompt_token_ids.clone(),
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        crate::tx::Transaction::new(
            crate::constants::TX_VERSION,
            vec![],
            vec![],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
            0,
            palw_fp_eval_payload_encode_v1(&payload, &eval.tail),
        )
    }

    /// **The walk and the fold are one path**: the object the extraction builds from a carrier is the
    /// object the fold's evaluation branch accepts, a transaction the walk skips never reaches it, and a
    /// carrier the door refuses (below its heights, or not signed) takes no job.
    #[test]
    fn a_carried_evaluation_claim_is_extracted_and_folded_into_its_job() {
        let p = params();
        let s = evaluating();
        let honest = claim(&s, 3, PalwEvalSubjectV1::Candidate(h(CAND_A)), BOB, 0xD0);
        let tx = carrier_of(&honest);
        let freeprompt = crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_for_tests(
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(0xCA7),
            Hash64::from_u64_word(0xC0757),
        )
        .unwrap()
        .freeprompt;
        let extract = |tx: &crate::tx::Transaction, rules: crate::palw_freeprompt_v3::PalwFpDecodeRulesV1, signed: bool| {
            palw_fp_eval_objects_from_accepted_txs_v1(
                std::slice::from_ref(tx),
                h(0x10),
                &freeprompt,
                false,
                |_| crate::palw_fp_objects_v3::PalwFpClassCapsV1 {
                    step_ladder: 1 << 26,
                    held: false,
                    derived_work: crate::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Declared,
                    logits_q24: true,
                },
                false,
                false,
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                rules,
                move |_, _, _, _| signed,
            )
        };
        let active = crate::palw_freeprompt_v3::PalwFpDecodeRulesV1::Active;
        let extraction = extract(&tx, active, true);
        assert!(extraction.skipped.is_empty(), "{:?}", extraction.skipped);
        let [carried] = &extraction.objects[..] else { panic!("one object") };
        assert_eq!(carried.carrier, tx.id());
        // The extracted object differs from the hand-built one only in the fields the carrier derives
        // (the id, the DA retention, the job pin): the evaluation carriage — the job and its tail — is
        // the same, field for field.
        let (PalwConsensusObjectV2::FreePromptCommitted { eval: built, .. }, PalwConsensusObjectV2::FreePromptCommitted { eval: extracted, .. }) =
            (&honest, &carried.object)
        else {
            panic!("free-prompt commitments")
        };
        assert_eq!(built, extracted);
        // …and the fold takes it: the job's row is written and the claim is the chain's.
        let (s1, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &carried.object).expect("the extracted claim folds"));
        let row = s1.improvement_eval_job(&h(LINE), 1, 3, &PalwEvalSubjectV1::Candidate(h(CAND_A)), PalwScoringKindV1::ExactMatch).unwrap();
        let taken = row.claim.unwrap();
        let PalwConsensusObjectV2::FreePromptCommitted { claim: id, .. } = &carried.object else { unreachable!() };
        assert_eq!((taken.claim_id, taken.executor, taken.accepted_daa), (*id, bond(BOB), 1_600));
        assert!(palw_improve_claim_is_evaluation_v1(s1.claims.get(id).unwrap()), "an evaluation claim: no quanta");
        // The walk skips what its door refuses: below the decode rules, or unsigned — and nothing reaches the fold.
        assert!(extract(&tx, crate::palw_freeprompt_v3::PalwFpDecodeRulesV1::Dormant, true).objects.is_empty());
        assert!(extract(&tx, active, false).objects.is_empty());
    }

    /// Fold `object` on `s` at `daa` as a block's object step would, on the state and params given.
    fn fold_one(s: &PalwChainStateV2, p: &PalwStateParamsV2, daa: u64, object: &PalwConsensusObjectV2) -> Result<(), PalwStateV2Error> {
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(s, p, false, false, false, false, &extras);
        apply_object(&mut b, &ctx(daa), object)
    }

    /// **MIP-20 (spec 17 §17.8.2): the epoch's evaluation budget is shared among its jobs.** Each job's
    /// positions (the prompt's ids and the stream's) are held to `⌊budget / jobs⌋`, where the budget is the
    /// smaller of the policy's `max_eval_positions` and the network's per-epoch ceiling at its permille, and
    /// the jobs are the core's escrow bound over the epoch's subjects — so no order of claims can spend
    /// another job's share, and the epoch's total can never exceed its budget.
    #[test]
    fn an_evaluation_claim_holds_to_its_jobs_share_of_the_epoch_budget() {
        // The epoch: eight items, the parent and one candidate, one ExactMatch stage — one claimable job
        // an item and a subject: 8 + 8. The claim: a 3-id prompt and 4 generated ids.
        let jobs = palw_improve_eval_epoch_jobs_v1(&policy().eval, 2);
        assert_eq!(jobs, 16);
        let positions = 7u64;
        assert_eq!(palw_improve_eval_job_position_cap_v1(positions * jobs, jobs), positions);
        assert_eq!(palw_improve_eval_job_position_cap_v1(positions * jobs - 1, jobs), positions - 1);
        let refusal = Err(PalwStateV2Error::ImprovementRefused("an evaluation claim past its job's share of the epoch's evaluation budget (MIP-20)"));
        // By the policy's own budget.
        let p = params();
        for (budget, fits) in [(positions * jobs, true), (positions * jobs + jobs - 1, true), (positions * jobs - 1, false), (jobs, false)] {
            let mut tight = policy();
            tight.eval.max_eval_positions = budget;
            let s = evaluating_with(&p, tight);
            let honest = claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xE0);
            let folded = fold_one(&s, &p, 1_600, &honest);
            assert_eq!(folded, if fits { Ok(()) } else { refusal.clone() }, "a budget of {budget} positions");
        }
        // By the network's ceiling, at its permille of the span: the same epoch, the policy asking for the
        // ceiling whole, the share shrinking it.
        for (permille, fits) in [(1_000u16, true), (35, true), (34, false), (1, false)] {
            let ceilings = crate::palw_improve_v1::PalwImprovementCeilingsV1 {
                max_eval_positions_per_epoch: 3_200,
                max_eval_budget_permille: permille,
                ..PALW_DRILL_IMPROVE_CEILINGS_V1
            };
            let p = params().with_improve_ceilings(Some(ceilings));
            let mut tight = policy();
            tight.eval.max_eval_positions = 3_200;
            let s = evaluating_with(&p, tight);
            let honest = claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xE1);
            assert_eq!(fold_one(&s, &p, 1_600, &honest), if fits { Ok(()) } else { refusal.clone() }, "a permille of {permille}");
        }
        // The budget is the smaller of the two, and never past the ceiling itself.
        let ceilings = PALW_DRILL_IMPROVE_CEILINGS_V1;
        assert_eq!(palw_improve_eval_budget_positions_v1(1 << 20, &ceilings), 1 << 20, "the policy's, when smaller");
        assert_eq!(palw_improve_eval_budget_positions_v1(u64::MAX, &ceilings), (1 << 32) / 2, "the ceiling's share, when smaller");
        assert_eq!(
            palw_improve_eval_budget_positions_v1(u64::MAX, &crate::palw_improve_v1::PalwImprovementCeilingsV1::FORMAT_CAPS_V1),
            1 << 40,
            "the whole ceiling at 1,000 permille"
        );
        // More subjects only shrink a job's share; judged stages and a Pairwise subject each add jobs.
        let mut judged = policy().eval;
        let one = palw_improve_eval_epoch_jobs_v1(&judged, 1);
        assert_eq!(one, 8, "the parent alone");
        assert_eq!(palw_improve_eval_epoch_jobs_v1(&judged, 5), 8 * 5);
        judged.stages.push(PalwScoringStageV1 { kind: PalwScoringKindV1::Judge, params: PalwScoringParamsV1::Judge { lo: 0, hi: 10 } });
        judged.stages.push(PalwScoringStageV1 { kind: PalwScoringKindV1::Pairwise, params: PalwScoringParamsV1::Pairwise { margin: 0 } });
        assert_eq!(
            palw_improve_eval_epoch_jobs_v1(&judged, 2),
            (8 * 2) + (8 * 2 + 8),
            "a judge job for every subject's item, a pairwise job for every other subject's"
        );
        // Suites and a key's scoring add none: the regression and safety items are not claimable (§17.8.4).
        let mut suites = policy().eval;
        suites.regression_items = 32;
        suites.safety_items = 16;
        assert_eq!(palw_improve_eval_epoch_jobs_v1(&suites, 2), 16, "n drawn items bound the epoch's claimable jobs");
        assert_eq!(palw_improve_eval_job_position_cap_v1(10, 0), 10, "no division by zero");
    }

    /// **The panels' half of the capacity rules** (RFC-0004 §13): an evaluation claim is seated as any claim
    /// of its class is, so its class must be one the chain serves — a frozen class takes none — and the
    /// claim's reservation is a bounded share of its executor's room: one claim may take at most the
    /// fence's `max_eval_budget_permille` of it, so evaluation cannot crowd out the executor's attempts.
    #[test]
    fn an_evaluation_claim_needs_a_class_the_chain_serves_and_a_bounded_share_of_its_executors_room() {
        let p = params();
        let s = evaluating();
        let candidate = claim(&s, 1, PalwEvalSubjectV1::Candidate(h(CAND_A)), CAROL, 0xE2);
        assert_eq!(fold_one(&s, &p, 1_600, &candidate), Ok(()), "the premise: an honest claim folds");
        let mut frozen = s.clone();
        frozen.classes.get_mut(&h(CAND_A)).unwrap().status = PalwClassStatusV2::Frozen { since_daa: 1_550 };
        assert_eq!(fold_one(&frozen, &p, 1_600, &candidate), Err(PalwStateV2Error::FrozenClass(h(CAND_A))), "a frozen class takes no claim");
        // The parent's class too: the head is no exception.
        let parent = claim(&s, 1, PalwEvalSubjectV1::Parent, CAROL, 0xE3);
        let mut frozen_head = s.clone();
        frozen_head.classes.get_mut(&h(LINE)).unwrap().status = PalwClassStatusV2::Frozen { since_daa: 1_550 };
        assert_eq!(fold_one(&frozen_head, &p, 1_600, &parent), Err(PalwStateV2Error::FrozenClass(h(LINE))));

        // The share: raise the class's price per unit of work until the claim's reservation passes
        // the fence's share of its executor's room — the refusal sits between "folds" and the room's own
        // ceiling (`FreePromptExposureCeiling`), and a network that gives evaluation its whole room
        // (1,000 permille) never refuses it.
        let share = |permille: u16| params().with_improve_ceilings(Some(crate::palw_improve_v1::PalwImprovementCeilingsV1 {
            max_eval_budget_permille: permille,
            ..PALW_DRILL_IMPROVE_CEILINGS_V1
        }));
        let outcome = |price: u64, permille: u16| {
            let p = share(permille);
            let s = evaluating_with(&p, policy());
            let mut priced = s.clone();
            priced.classes.get_mut(&h(LINE)).unwrap().slash_value_per_pwu = price;
            let honest = claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xE4);
            fold_one(&priced, &p, 1_600, &honest)
        };
        let share_refusal = Err(PalwStateV2Error::ImprovementRefused(
            "an evaluation claim reserves more than the evaluation budget's share of its executor's claim capacity",
        ));
        let mut seen = (false, false, false);
        let mut price = 1u64;
        while price < u64::MAX / 10 {
            let result = outcome(price, 1);
            if result == Ok(()) {
                seen.0 = true;
            } else if result == share_refusal {
                seen.1 = true;
            } else if matches!(result, Err(PalwStateV2Error::FreePromptExposureCeiling { .. })) {
                seen.2 = true;
            } else {
                panic!("price {price}: {result:?}");
            }
            // With the room's whole share the same claim is never refused for its share.
            assert_ne!(outcome(price, 1_000), share_refusal, "price {price}");
            price = price.saturating_mul(4);
        }
        assert_eq!(seen, (true, true, true), "cheap work folds, dearer work meets the share, dearest the room's own ceiling");
    }

    /// **One job per stage that applies to the item's kind** (spec 17 §17.8.2): a likelihood item (its case's
    /// reference is a committed continuation) is teacher-forced and takes no generation job — one on it
    /// could never be scored, and would hold the epoch's scoring until `t_score`.
    #[test]
    fn a_likelihood_item_takes_no_generation_job() {
        use crate::palw_improve_material_v1::{
            PalwCaseReferenceV1, PalwCaseSourceV1, PalwHardCaseRecordV1, PalwHardCaseV1,
        };
        let p = params();
        let mut s = evaluating();
        let case_id = s.improvement_item(&h(LINE), 1, 2).unwrap().case_id;
        let record = |reference| PalwHardCaseRecordV1 {
            case: PalwHardCaseV1 {
                line_id: h(LINE),
                case_id,
                domain: 0,
                prompt_ids: vec![3, 5, 2],
                reference,
                source: PalwCaseSourceV1::Setter,
                head_evidence: None,
            },
            submitter: bond(CAROL),
            admitted_daa: 1_400,
            epoch: 1,
            holdout: true,
            revealed: None,
        };
        let generation = claim(&s, 2, PalwEvalSubjectV1::Parent, CAROL, 0xE5);
        s.improvement_cases.insert((h(LINE), case_id), record(PalwCaseReferenceV1::ExactKey { commitment: h(0x77) }));
        assert_eq!(fold_one(&s, &p, 1_600, &generation), Ok(()), "an exact-key item takes its generation");
        s.improvement_cases.insert((h(LINE), case_id), record(PalwCaseReferenceV1::None));
        assert_eq!(fold_one(&s, &p, 1_600, &generation), Ok(()), "a judged-only item's output feeds its judges");
        s.improvement_cases.insert((h(LINE), case_id), record(PalwCaseReferenceV1::Continuation { commitment: h(0x77) }));
        assert_eq!(
            fold_one(&s, &p, 1_600, &generation),
            Err(PalwStateV2Error::ImprovementRefused("a likelihood item takes no generation job"))
        );
        // The other items of the epoch are untouched.
        let other = claim(&s, 3, PalwEvalSubjectV1::Parent, CAROL, 0xE6);
        assert_eq!(fold_one(&s, &p, 1_600, &other), Ok(()));
    }

    /// **An evaluation claim runs the whole `Final` path** (MIP-17): the lane's own `finalize_claim` — the
    /// funnel every claim's Final passes — releases its reservation, writes no reward and no weight, records
    /// the row's finality and pays the subject's escrowed fee to the executor, once; and a voided holder
    /// frees the job for a re-claim.
    #[test]
    fn an_evaluation_claim_finalizes_through_the_lanes_own_funnel_and_pays_its_fee_once() {
        let p = params();
        let s = evaluating();
        let honest = claim(&s, 4, PalwEvalSubjectV1::Candidate(h(CAND_A)), CAROL, 0xE7);
        let id = h(0xE7);
        let (s1, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &honest).expect("an honest claim"));
        let live = s1.claims.get(&id).unwrap().clone();
        assert!(live.reserved > 0 && matches!(live.phase, PalwClaimPhaseV2::Provisional));
        let exposure = s1.reserved_exposure(&bond(CAROL));
        assert_eq!(exposure, live.reserved, "the executor's room carries the reservation");
        let fee = policy().fees.eval_fee_per_job;
        let earned = s1.improvement_earnings(&bond(CAROL));
        // Licensed, then Final: the funnel runs as the sweep runs it.
        let licensed = PalwClaimStateV2 { phase: PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1_620 }, ..live.clone() };
        let (s2, _) = at(&s1, &p, 1_700, |b| {
            b.write_claim(id, Some(licensed.clone()));
            b.finalize_claim(id, &licensed, 1_700).expect("the claim finalizes");
        });
        assert!(matches!(s2.claims.get(&id).unwrap().phase, PalwClaimPhaseV2::Final { final_daa: 1_700 }));
        assert_eq!(s2.reserved_exposure(&bond(CAROL)), 0, "the reservation is released");
        assert_eq!(s2.pending_payouts_iter().count(), 0, "no reward: an evaluation claim escrows none");
        assert_eq!(s2.improvement_earnings(&bond(CAROL)), earned + fee, "the subject's escrow paid the job's fee, once");
        let (_, row) = s2.improvement_eval_job_of_claim(&id).unwrap();
        assert_eq!(row.claim.unwrap().final_daa, Some(1_700), "the job's row records it");
        let (_, cand) = s2.improvement_candidate(&h(LINE), 1, &h(CAND_A)).unwrap();
        assert_eq!(cand.escrow_spent, fee, "from the candidate's escrow");
        // The next block's flush turns the earnings into a payout row; a second Final of the same claim pays
        // nothing more (the row is final), so the earnings stay at what the flush left.
        let (s3, _) = at(&s2, &p, 1_701, |b| {
            let done = b.state.claims.get(&id).cloned().unwrap();
            let before = b.state.improvement_earnings(&bond(CAROL));
            b.note_improvement_eval_final_v1(&id, &done, 1_701).unwrap();
            assert_eq!(b.state.improvement_earnings(&bond(CAROL)), before, "never twice");
        });
        assert_eq!(s3.improvement_earnings(&bond(CAROL)), 0, "the fee left as a payout, and nothing more came");
        // A voided holder frees the job: the next claim takes it, and the cap on its share is the job's own
        // (no budget was spent by the dead claim).
        let mut voided = s1.clone();
        voided.claims.get_mut(&id).unwrap().phase = PalwClaimPhaseV2::Voided { voided_daa: 1_650, reason: PalwVoidReasonV2::BindTimeout };
        let retake = claim(&s, 4, PalwEvalSubjectV1::Candidate(h(CAND_A)), BOB, 0xE8);
        assert_eq!(fold_one(&voided, &p, 1_700, &retake), Ok(()), "the job is re-taken");
    }
}
