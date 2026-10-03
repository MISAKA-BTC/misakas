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
        part: u8,
    ) -> Option<&PalwEvalJobStateV1> {
        self.improvement_eval_jobs.get(&(*line_id, epoch, item, *subject, kind, part))
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
        self.improvement_eval_jobs.range(palw_improve_eval_epoch_range_v1(line_id, epoch)).any(
            |((_, _, item, subject, kind, _), row)| {
                if self.improvement_eval_row_pending_v1(row) {
                    return true;
                }
                let final_generation = *kind == PalwScoringKindV1::ExactMatch && row.claim.is_some_and(|c| c.final_daa.is_some());
                final_generation
                    && self
                        .improvement_item(line_id, epoch, *item)
                        .is_some_and(|i| !i.dropped && self.improvement_item_awaits_key_v1(line_id, i))
                    && self
                        .improvement_result(line_id, epoch, *item, subject)
                        .is_none_or(|result| result.scores.iter().all(|score| score.kind != PalwScoringKindV1::ExactMatch))
            },
        )
    }

    /// **Does an item's generation wait for a key to be scored?** A hold-out case with an exact-match key and every
    /// setter item (whose keys open in `Closing`) do; a judged-only case (a known case with no reference) has none
    /// to come — a judge reads its generation, nothing scores it — and a suite entry's key is public and scores at
    /// `Final`.
    fn improvement_item_awaits_key_v1(&self, line_id: &Hash64, item: &PalwEvalItemV1) -> bool {
        match item.source {
            PalwItemSourceV1::HoldOut => self.improvement_case(line_id, &item.case_id).is_none_or(|case| {
                matches!(case.case.reference, crate::palw_improve_material_v1::PalwCaseReferenceV1::ExactKey { .. })
            }),
            PalwItemSourceV1::Setter { .. } => true,
            PalwItemSourceV1::Regression { .. } | PalwItemSourceV1::Safety { .. } => false,
        }
    }

    /// **Are the generations of `items` settled?** (spec 17 §17.8.2's key disclosure, E17: keys open
    /// only in `Closing`, which the material lane's reveal arms check) — no subject's generation (its
    /// ExactMatch job) on any of them holds a live claim that is not yet final.
    pub fn improvement_eval_generations_settled_v1(&self, line_id: &Hash64, epoch: u64, items: &[u32]) -> bool {
        items.iter().all(|item| {
            let lo = (*line_id, epoch, *item, PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch, 0);
            let hi = (
                *line_id,
                epoch,
                *item,
                PalwEvalSubjectV1::Previous(Hash64::from_bytes([0xFF; 64])),
                PalwScoringKindV1::Pairwise,
                u8::MAX,
            );
            self.improvement_eval_jobs
                .range(lo..=hi)
                .filter(|((_, _, _, _, kind, _), _)| *kind == PalwScoringKindV1::ExactMatch)
                .all(|(_, row)| !self.improvement_eval_row_pending_v1(row))
        })
    }
}

impl PalwChainStateV2 {
    /// **What the court holds of an evaluation claim** (spec 17 §17.8.6): the claim must be the evaluation claim the
    /// job table holds for the binding's job — the row's claim, by id and by the roots and the bond it committed —
    /// and its class an IR class the registry still holds (the program, the layout digest it was registered with and
    /// its artifact root). `Err` names what is missing; nothing is evaluated.
    pub fn improvement_eval_claim_facts_v1<'a>(
        &'a self,
        claim: &'a PalwClaimStateV2,
        binding: &PalwEvalBindingV1,
    ) -> Result<crate::palw_improve_eval_court_v1::PalwEvalClaimFactsV1<'a>, &'static str> {
        if !palw_improve_claim_is_evaluation_v1(claim) {
            return Err("the claim is not an evaluation claim");
        }
        let row = self.improvement_eval_jobs.get(&binding.job.key()).ok_or("the binding's job is not in the job table")?;
        let held = row.claim.as_ref().ok_or("the job holds no claim")?;
        let held_claim = self.claims.get(&held.claim_id).ok_or("the job's claim is gone")?;
        if held_claim.execution_root != claim.execution_root
            || held_claim.bond != claim.bond
            || held_claim.accepted_daa != claim.accepted_daa
        {
            return Err("the claim is not the one the job table holds for the binding's job");
        }
        let record = self.tir_classes.get(&claim.class_id).ok_or("the claim's class is not an IR class")?;
        let class = self.classes.get(&claim.class_id).ok_or("the claim's class is not registered")?;
        Ok(crate::palw_improve_eval_court_v1::PalwEvalClaimFactsV1 {
            class_id: &claim.class_id,
            execution_root: &claim.execution_root,
            trace_root: &claim.trace_root,
            output_root: &claim.output_root,
            work_leaves: claim.work_leaves,
            job: &row.job,
            program: record.program.as_slice(),
            layout_digest: record.layout_digest,
            artifact_root: class.artifact_root,
        })
    }
}

/// The key range of one epoch's jobs.
fn palw_improve_eval_epoch_range_v1(line_id: &Hash64, epoch: u64) -> std::ops::RangeInclusive<PalwEvalJobKeyV1> {
    let lo = (*line_id, epoch, 0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch, 0);
    let hi =
        (*line_id, epoch, u32::MAX, PalwEvalSubjectV1::Previous(Hash64::from_bytes([0xFF; 64])), PalwScoringKindV1::Pairwise, u8::MAX);
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

/// The policy's stage parameters for a kind, as the context reads them. A judged stage's include its
/// specification's logit scale (spec 17 §17.8.5): the pass it is runs at the judge's scale.
fn palw_improve_eval_stage_params_of_v1(policy: &PalwImprovementPolicyV1, kind: PalwScoringKindV1) -> Option<PalwEvalStageParamsV1> {
    policy.eval.stages.iter().find(|stage| stage.kind == kind).and_then(|stage| {
        Some(match stage.params {
            PalwScoringParamsV1::ExactMatch { open, close, .. } => PalwEvalStageParamsV1::ExactMatch { open, close },
            PalwScoringParamsV1::RefLogLik { logit_scale_q24 } => PalwEvalStageParamsV1::RefLogLik { logit_scale_q24 },
            PalwScoringParamsV1::Judge { lo, hi } => {
                PalwEvalStageParamsV1::Judge { lo, hi, logit_scale_q24: policy.eval.judge.as_ref()?.logit_scale_q24 }
            }
            PalwScoringParamsV1::Pairwise { margin } => {
                PalwEvalStageParamsV1::Pairwise { margin, logit_scale_q24: policy.eval.pairwise.as_ref()?.logit_scale_q24 }
            }
        })
    })
}

/// **What the chain derives a claim's inputs from**: the prompt, the reference a teacher-forced job's ids must
/// equal, an ExactMatch suite entry's key, and the generations a judged part reads.
struct PalwEvalFactsV1 {
    prompt: Vec<u32>,
    /// The ids a teacher-forced job carries (a hold-out case's continuation, a suite entry's, a judge's verdict).
    reference: Option<Vec<u32>>,
    /// An ExactMatch suite entry's key: the job is scored from it at acceptance (the entry is public).
    key: Option<Vec<u32>>,
    /// The output roots a judged part reads, in the order its prompt shows them.
    finalized_roots: Vec<Hash64>,
}

/// The `output_root` of a subject's final generation of an item (its ExactMatch job's final claim) — what a
/// judged part reads. `None` while there is none.
fn palw_improve_eval_final_generation_root_v1(
    state: &PalwChainStateV2,
    line_id: &Hash64,
    epoch: u64,
    item: u32,
    subject: &PalwEvalSubjectV1,
) -> Option<Hash64> {
    let row = state.improvement_eval_job(line_id, epoch, item, subject, PalwScoringKindV1::ExactMatch, 0)?;
    let claim = row.claim.as_ref()?;
    claim.final_daa.map(|_| claim.output_root)
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
    let (line_id, epoch, item, subject, kind, part) = job.key();
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
    // The class the claim runs: its subject's — or, for a judged part, the judge the draw named for the item.
    let judged = matches!(job.mode, PalwEvalModeV1::Judged { .. });
    let run_class = match (&job.mode, subject) {
        (PalwEvalModeV1::Judged { judge }, _) => *judge,
        (_, PalwEvalSubjectV1::Parent) => header.parent,
        (_, PalwEvalSubjectV1::Candidate(class) | PalwEvalSubjectV1::Previous(class)) => class,
    };
    if run_class != *c.class_id {
        return Err(refused(if judged {
            "the claim's class is not its item's judge"
        } else {
            "the claim's class is not its subject's"
        }));
    }
    // The kind is the policy's, and the mode the one the policy and the item fix.
    let policy = builder.state.improvement_policy(&line_id).cloned().ok_or_else(|| refused("the line has no policy"))?;
    let params = palw_improve_eval_stage_params_of_v1(&policy, kind).ok_or_else(|| refused("a kind the policy does not score"))?;
    if part >= palw_improve_scoring_parts_v1(kind) {
        return Err(refused("a part past the kind's claims"));
    }
    // What the item is: a drawn case or setter item (its prompt on chain), or an entry of a registered suite
    // dataset (disclosed by the claim's opening, spec 17 §17.8.1).
    let suite = match item_row.source {
        PalwItemSourceV1::Regression { index } => Some((policy.eval.regression_dataset, index)),
        PalwItemSourceV1::Safety { index } => Some((policy.eval.safety_dataset, index)),
        PalwItemSourceV1::HoldOut | PalwItemSourceV1::Setter { .. } => None,
    };
    let facts = match (&job.mode, suite) {
        // ---- a suite item: the claim opens its entry under the dataset's content root ----
        (PalwEvalModeV1::Generate { .. } | PalwEvalModeV1::TeacherForced { .. }, Some((dataset_id, index))) => {
            let opening = tail.opening.as_ref().ok_or_else(|| refused("a suite item's claim opens its entry"))?;
            let dataset =
                builder.state.improvement_dataset(&line_id, &dataset_id).ok_or_else(|| refused("the suite's dataset is gone"))?;
            let entry = PalwSuiteEntryV1 { prompt: c.prompt_token_ids.to_vec(), reference: opening.reference.clone() };
            if !palw_improve_suite_verify_v1(
                &palw_improve_suite_leaf_v1(&entry),
                index as u64,
                dataset.dataset.items,
                &opening.proof,
                &dataset.dataset.content_root,
            ) {
                return Err(refused("a suite entry's opening does not verify under its dataset's content_root"));
            }
            match (&job.mode, &opening.reference) {
                (PalwEvalModeV1::Generate { .. }, PalwSuiteReferenceV1::ExactKey(key)) => {
                    let key_cap = policy
                        .eval
                        .stages
                        .iter()
                        .find_map(
                            |s| if let PalwScoringParamsV1::ExactMatch { key_cap, .. } = s.params { Some(key_cap) } else { None },
                        )
                        .unwrap_or(0);
                    if key.len() > key_cap as usize {
                        return Err(refused("a suite key longer than the policy's key cap"));
                    }
                    PalwEvalFactsV1 { prompt: entry.prompt, reference: None, key: Some(key.clone()), finalized_roots: Vec::new() }
                }
                (PalwEvalModeV1::TeacherForced { .. }, PalwSuiteReferenceV1::Continuation(reference)) => PalwEvalFactsV1 {
                    prompt: entry.prompt,
                    reference: Some(reference.clone()),
                    key: None,
                    finalized_roots: Vec::new(),
                },
                _ => return Err(refused("a suite entry's reference is not the job's kind")),
            }
        }
        // ---- a case or setter item: its prompt, and a teacher-forced job's reference, are on chain ----
        (PalwEvalModeV1::Generate { .. } | PalwEvalModeV1::TeacherForced { .. }, None) => {
            if tail.opening.is_some() {
                return Err(refused("only a suite item's claim carries an opening"));
            }
            let prompt = palw_improve_eval_item_prompt_hook_v1(&builder.state, &line_id, epoch, item)
                .ok_or_else(|| refused("the item's prompt is not disclosed"))?;
            let reference = match &job.mode {
                PalwEvalModeV1::TeacherForced { .. } => Some(
                    palw_improve_eval_item_reference_hook_v1(&builder.state, &line_id, epoch, item)
                        .ok_or_else(|| refused("the item's reference is not disclosed"))?,
                ),
                _ => None,
            };
            PalwEvalFactsV1 { prompt, reference, key: None, finalized_roots: Vec::new() }
        }
        // ---- a judged part: the judge's pass over the template filled with the item and the generations ----
        (PalwEvalModeV1::Judged { judge }, None) => {
            if tail.opening.is_some() {
                return Err(refused("a judged part opens no suite entry"));
            }
            if item_row.judge != Some(*judge) {
                return Err(refused("a judged part runs the judge the draw named for its item"));
            }
            let spec = match kind {
                PalwScoringKindV1::Judge => policy.eval.judge.as_ref(),
                _ => policy.eval.pairwise.as_ref(),
            }
            .ok_or_else(|| refused("a judged kind the policy does not score"))?;
            let (order, verdict) = palw_improve_judged_part_v1(kind, part).ok_or_else(|| refused("a part past the kind's claims"))?;
            let verdict_ids = if verdict == 0 { &spec.verdict_a } else { &spec.verdict_b };
            if tail.generated != *verdict_ids {
                return Err(refused("a judged part's ids are its verdict sequence"));
            }
            // The template: the registered dataset's one entry.
            let read = tail.read.as_ref().ok_or_else(|| refused("a judged part carries its reading"))?;
            let template_row = builder
                .state
                .improvement_dataset(&line_id, &spec.template_dataset)
                .ok_or_else(|| refused("the judge's template dataset is not registered"))?;
            if template_row.dataset.items != 1
                || palw_improve_judge_template_root_v1(&read.template) != template_row.dataset.content_root
            {
                return Err(refused("a judged part's template is not the registered one"));
            }
            // The outputs: the subject's final generation (and, for a pairwise part, the parent's), in the order
            // this part's order shows them.
            let subject_root = palw_improve_eval_final_generation_root_v1(&builder.state, &line_id, epoch, item, &subject)
                .ok_or_else(|| refused("the generation a judged part reads is not final"))?;
            let finalized_roots = if kind == PalwScoringKindV1::Pairwise {
                let parent_root =
                    palw_improve_eval_final_generation_root_v1(&builder.state, &line_id, epoch, item, &PalwEvalSubjectV1::Parent)
                        .ok_or_else(|| refused("the parent's generation a pairwise part reads is not final"))?;
                if order == 0 { vec![subject_root, parent_root] } else { vec![parent_root, subject_root] }
            } else {
                vec![subject_root]
            };
            let (item_ids, outputs) =
                palw_improve_judge_unfill_v1(&read.template, c.prompt_token_ids, read.item_len, &read.output_lens)
                    .ok_or_else(|| refused("a judged part's prompt is not its template's fill"))?;
            let item_prompt = palw_improve_eval_item_prompt_hook_v1(&builder.state, &line_id, epoch, item)
                .ok_or_else(|| refused("the item's prompt is not disclosed"))?;
            if item_ids != item_prompt.as_slice() {
                return Err(refused("a judged part's item prompt is not the item's"));
            }
            let read_roots: Vec<Hash64> = outputs.iter().map(|ids| palw_improve_eval_generated_root_v1(ids)).collect();
            if read_roots != finalized_roots {
                return Err(refused("a judged part did not read the final generations its order shows"));
            }
            PalwEvalFactsV1 { prompt: c.prompt_token_ids.to_vec(), reference: Some(verdict_ids.clone()), key: None, finalized_roots }
        }
        (PalwEvalModeV1::Judged { .. }, Some(_)) => return Err(refused("a suite item is not judged")),
    };
    // The mode's own fields against the policy and the item.
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
        PalwEvalModeV1::TeacherForced { .. } | PalwEvalModeV1::Judged { .. } => {
            if Some(&tail.generated) != facts.reference.as_ref() {
                return Err(refused("a teacher-forced job's ids are the item's reference"));
            }
        }
    }
    // The prompt is the item's.
    if c.prompt_token_ids != facts.prompt.as_slice() {
        return Err(refused("the claim's prompt is not the item's"));
    }
    let prompt = facts.prompt;
    // The stage parameters are the policy's; the roots are the tail's (the extractor checked them; the
    // fold holds them again, over the policy's parameters, the item's prompt and the chain's own reads).
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
            &palw_improve_eval_finalized_root_v1(&facts.finalized_roots),
            &tail.score,
        ) != *c.execution_root
    {
        return Err(refused("the claim's roots are not its tail's"));
    }
    // **MIP-20: the epoch's evaluation budget** (spec 17 §17.8.2) — each job's positions (prompt and
    // stream ids) within its equal share of the policy's `max_eval_positions` and the network's ceiling.
    let ceilings = builder.params.improve_ceilings().ok_or_else(|| refused("an evaluation claim below palw_improvement_v1"))?;
    let epoch_jobs = palw_improve_eval_epoch_jobs_v1(&policy.eval, builder.state.improvement_subjects(&line_id, epoch).len());
    let job_cap = palw_improve_eval_job_position_cap_v1(
        palw_improve_eval_budget_positions_v1(policy.eval.max_eval_positions, &ceilings),
        epoch_jobs,
    );
    if palw_improve_eval_positions_v1(prompt.len(), tail.generated.len()) > job_cap {
        return Err(refused("an evaluation claim past its job's share of the epoch's evaluation budget (MIP-20)"));
    }
    // The executed class is an IR class, its layout the one its id binds, and the claim's leaves the chain's
    // count of the derived context.
    let record = builder.state.tir_classes.get(c.class_id).cloned().ok_or_else(|| refused("the executed class is not an IR class"))?;
    if palw_improve_eval_layout_digest_v1(&tail.subject_layout) != record.layout_digest {
        return Err(refused("the carried layout is not the executed class's"));
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
    // **The class's replay room counts the claim** (RFC-0004 §13 as decided 2026-09-30): an evaluation claim is a
    // whole claim of its class on the panel's budget, as any free-prompt claim is, and the fence's
    // `max_eval_budget_permille` bounds the share of the room evaluation may hold — so it cannot crowd out the
    // class's attempts. And the executor's share of the class's unlicensed claims, as on the lane's own arm.
    if builder.extras.audit_2026_09_23_active {
        builder.check_class_admits_claim(c.class_id, daa, PalwGatedClaimV1::Evaluation)?;
        // RFC-0002 Part II Proposal A: an evaluation claim of a class is a claim of it — the one seating function.
        builder.read().check_class_seated_v1(c.class_id, c.bond, daa)?;
    }
    if let Some(why) = builder.read().eval_room_share_refusal_v1(c.class_id, daa, ceilings.max_eval_budget_permille) {
        return Err(refused(why));
    }
    builder.check_bond_class_share(c.bond, c.class_id, daa)?;
    let program =
        misaka_palw_tir::TirProgramV1::decode_canonical(&record.program).map_err(|_| refused("the executed class's program"))?;
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
    palw_improve_eval_take_v1(&mut row, *c.claim_id, *c.bond, daa, header.times.t_eval, answer, *c.output_root).map_err(
        |e| match e {
            PalwEvalErrorV1::Taken { .. } => refused("the evaluation job is taken by a live claim"),
            _ => refused("an evaluation claim at or after t_eval"),
        },
    )?;
    if kind != PalwScoringKindV1::ExactMatch {
        let value = palw_improve_eval_score_value_v1(kind, &tail.score).map_err(|_| refused("the committed score"))?;
        if !palw_improve_eval_score_in_range_v1(&params, value) {
            return Err(refused("a committed score outside its kind's range"));
        }
        if let Some(claim) = row.claim.as_mut() {
            claim.score = Some(value);
        }
    } else if let Some(key) = &facts.key {
        // A suite entry's key is public: the generation is scored from it, recorded at Final.
        if let Some(claim) = row.claim.as_mut() {
            claim.score = Some((claim.answer == Some(palw_improve_answer_span_hash_v1(key))) as i64);
        }
    }
    // The reservation: what a claim of this work on the class reserves, under ADR-0160's stage 1 ...
    let raw = palw_improve_eval_reservation_v1(c.work_leaves, class.slash_value_per_pwu, 1);
    let work_reserved = crate::palw_weight_cap_v1::palw_claim_weight_reservation_of_v1(builder.params, 0, 0, raw, daa);
    // ... and at least what the claim could swing (spec 17 §17.8.4, decided 2026-09-30): a forged evaluation
    // gains a promotion's payout, so its executor locks the epoch's largest payout over the fewest forged pairs
    // that flip the sign test — never less than the work's own lock.
    let balance = builder.state.improvement_pools.get(&line_id).map_or(0, |pool| pool.balance);
    let swing = palw_improve_eval_swing_lock_v1(palw_improve_max_promotion_payout_v1(balance, policy.promotion_share_permille));
    let reserved = work_reserved.max(swing);
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
    // the room the executor's bond carries — the work's own lock; the swing lock is a collateral bar, not capacity.
    if work_reserved.saturating_mul(1_000) > ceiling.saturating_mul(ceilings.max_eval_budget_permille as u128) {
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

    /// **A judged score, once all its parts are final** (spec 17 §17.8.5): the policy's stage parameters and the
    /// parts' committed log-likelihoods combined ([`palw_improve_judged_score_v1`]). `None` while a part is missing
    /// or not yet final — and when a generation the parts read is no longer what they read (spec 17 §17.8.6: a
    /// generation convicted after `Final` and perhaps claimed anew): each generation must still be a held FINAL claim,
    /// and every part must have been accepted at or after the newest of them was final, so a part that read a
    /// reversed generation records nothing and the item is missing for the incumbent.
    fn improvement_eval_judged_score_v1(
        &self,
        line_id: &Hash64,
        epoch: u64,
        item: u32,
        subject: &PalwEvalSubjectV1,
        kind: PalwScoringKindV1,
    ) -> Option<i64> {
        let policy = self.state.improvement_policy(line_id)?;
        let params = palw_improve_eval_stage_params_of_v1(policy, kind)?;
        let generation_final_daa = |of: &PalwEvalSubjectV1| -> Option<u64> {
            let claim = self.state.improvement_eval_job(line_id, epoch, item, of, PalwScoringKindV1::ExactMatch, 0)?.claim?;
            self.state.improvement_eval_claim_held_v1(&claim.claim_id).then_some(claim.final_daa).flatten()
        };
        let mut newest = generation_final_daa(subject)?;
        if kind == PalwScoringKindV1::Pairwise {
            newest = newest.max(generation_final_daa(&PalwEvalSubjectV1::Parent)?);
        }
        for part in 0..palw_improve_scoring_parts_v1(kind) {
            let accepted = self.state.improvement_eval_job(line_id, epoch, item, subject, kind, part)?.claim?.accepted_daa;
            if accepted < newest {
                return None;
            }
        }
        let parts: Option<Vec<i64>> = (0..palw_improve_scoring_parts_v1(kind))
            .map(|part| {
                self.state.improvement_eval_job(line_id, epoch, item, subject, kind, part).and_then(palw_improve_eval_score_v1)
            })
            .collect();
        palw_improve_judged_score_v1(&params, &parts?)
    }

    /// **MIP-17 at `Final`** (spec 17 §17.8.3, §17.8.5, §17.11.2): an evaluation claim's row records its
    /// finality; a kind that committed a score records it — a judged kind once all its parts are final; the
    /// subject's escrow pays the job's fee, the credited seats' share of it to them (§17.11.2). Nothing
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
        let (line_id, epoch, item, subject, kind, _part) = key;
        let scoring = self.improvement_eval_takes_score_v1(&line_id, epoch, item, &subject, kind);
        if !scoring {
            return Ok(());
        }
        let value = match kind {
            PalwScoringKindV1::ExactMatch | PalwScoringKindV1::RefLogLik => taken.score,
            PalwScoringKindV1::Judge | PalwScoringKindV1::Pairwise => {
                self.improvement_eval_judged_score_v1(&line_id, epoch, item, &subject, kind)
            }
        };
        if let Some(value) = value {
            self.record_improvement_score_v1(&line_id, epoch, item, subject, PalwEvalScoreV1 { kind, value })?;
        }
        // The panel that verified the claim: its drawn seats and those credited (those that answered), as the
        // ordinary claim's `Final` reads them — the row is still here, `release_seat_duties` comes after.
        let panel = self.state.panel_duties.get(claim_id).map(|row| {
            let credited: Vec<PalwBondKeyV2> = row.seats.iter().filter(|(_, at)| **at != 0).map(|(seat, _)| *seat).collect();
            (row.seats.len(), credited)
        });
        self.pay_improvement_eval_fee_v1(&line_id, epoch, &subject, &claim.bond, panel.as_ref().map(|(n, c)| (*n, c.as_slice())))?;
        Ok(())
    }

    /// **A convicted `Final` evaluation claim leaves the epoch** (spec 17 §17.8.6): the court reversed it, so its job is
    /// free again (open claiming: another executor may take it while the epoch still takes claims) and the score it
    /// gave — a hold-out generation's at the key's reveal, a suite entry's, a likelihood's, a judged score a part of
    /// which it was — is taken back out of the epoch's results while the epoch still takes scores, the item then
    /// missing for that subject and counting for the incumbent (§17.9.1). A claim voided before `Final` records no
    /// score and its row is cleared by open claiming already; an epoch already decided is not reopened.
    pub(super) fn note_improvement_eval_reversed_v1(
        &mut self,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
    ) -> Result<(), PalwStateV2Error> {
        if !palw_improve_claim_is_evaluation_v1(claim) {
            return Ok(());
        }
        let Some((key, row)) = self.state.improvement_eval_job_of_claim(claim_id) else { return Ok(()) };
        let mut row = row.clone();
        let (line_id, epoch, item, subject, kind, _part) = key;
        row.claim = None;
        self.write_improvement_eval_job(key, Some(row));
        self.retract_improvement_score_v1(&line_id, epoch, item, subject, kind)?;
        // A judge reads FINAL generations (spec 17 §17.8.5): a convicted generation takes the judged scores that read it
        // out too — the subject's Judge and Pairwise, and for the parent's generation every Pairwise score of the item.
        // A judge's honest pass over a dishonest output is no evidence, and a missing score favours the incumbent.
        if kind == PalwScoringKindV1::ExactMatch {
            let dependents: Vec<(PalwEvalSubjectV1, PalwScoringKindV1)> = match subject {
                PalwEvalSubjectV1::Parent => {
                    let lo = (line_id, epoch, item, PalwEvalSubjectV1::Parent);
                    let hi = (line_id, epoch, item, PalwEvalSubjectV1::Previous(Hash64::from_bytes([0xFF; 64])));
                    let mut keys = vec![(PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge)];
                    keys.extend(
                        self.state.improvement_results.range(lo..=hi).map(|((_, _, _, s), _)| (*s, PalwScoringKindV1::Pairwise)),
                    );
                    keys
                }
                other => vec![(other, PalwScoringKindV1::Judge), (other, PalwScoringKindV1::Pairwise)],
            };
            for (dependent, dependent_kind) in dependents {
                self.retract_improvement_score_v1(&line_id, epoch, item, dependent, dependent_kind)?;
            }
        }
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
            let Some(row) = self.state.improvement_eval_job(line_id, epoch, item, &subject, kind, 0).cloned() else { continue };
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
    use crate::palw_improve_epoch_v1::{PalwMaterialKindV1, palw_improve_suite_draw_v1};
    use crate::palw_improve_material_v1::PalwDatasetRecordV1;
    use crate::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1;
    use crate::palw_tir_admission_v1::PalwTirClassRecordV1;
    use crate::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
    use crate::tx::TransactionOutpoint;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::interp::{MapParams, ParamSource};
    use misaka_palw_tir::pipeline::PipelineParams;
    use misaka_palw_tir::{DType, Ref, Tensor, TensorType, TirProgramV1};
    use std::collections::BTreeMap;

    const ACTIVE: u64 = 100;
    const OWNER: u8 = 1;
    const ALICE: u8 = 2;
    const BOB: u8 = 3;
    const CAROL: u8 = 4;
    const DAVE: u8 = 5;
    const LINE: u8 = 0x10;
    const CAND_A: u8 = 0x11;
    const JUDGE: u8 = 0x12;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn bond(byte: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: h(byte), index: 0 })
    }

    fn params() -> PalwStateParamsV2 {
        let p = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
        // The evaluation court's moves ride the IR court's tag 62: `palw_tir_v1` is in force from ACTIVE as well (the shipped schedule arms it at the
        // RFC-0002 flag day, above these tests' DAA).
        bundle
            .state
            .clone()
            .with_tir_from_daa(Some(ACTIVE))
            .with_improve_from_daa(Some(ACTIVE))
            .with_improve_ceilings(Some(PALW_DRILL_IMPROVE_CEILINGS_V1))
            // The fold's mechanics, not Λ: the release arms testnet-12's fast-path check (rows 19-20), which its own test asks with an
            // explicit base (a_policys_windows_are_held_to_the_fast_honest_claim_path_at_the_daa_it_is_applied).
            .with_improve_lifecycle_base(None)
    }

    /// One ExactMatch stage, eight items, a four-id budget and no stops.
    fn policy() -> PalwImprovementPolicyV1 {
        let mut p = crate::palw_improve_policy_v1::palw_improvement_policy_example_v1();
        p.eval.n = 8;
        p.eval.n_min = 4;
        p.eval.regression_items = 0;
        p.eval.regression_dataset = Hash64::default();
        p.eval.safety_items = 0;
        p.eval.safety_dataset = Hash64::default();
        p.eval.stages.truncate(1);
        p.eval.setter_cap_permille = 1_000;
        p.eval.max_new_tokens = 4;
        p.eval.stop_ids = vec![];
        p.eval.stages[0].params = PalwScoringParamsV1::ExactMatch { open: -1, close: -1, key_cap: 4 };
        p.usage.value = 2;
        p.k_max = 2;
        p
    }

    /// A test's subject class in place of the toy — a real decoder with attention, whose leaves are dissected
    /// ([`use_golden_subject`]). One per test thread; the guard clears it.
    #[derive(Clone)]
    struct Subject {
        program: TirProgramV1,
        weights: MapParams,
        layout: PalwTirLayoutV1,
    }

    thread_local! {
        static SUBJECT: std::cell::RefCell<Option<Subject>> = const { std::cell::RefCell::new(None) };
    }

    fn subject() -> Option<Subject> {
        SUBJECT.with(|s| s.borrow().clone())
    }

    struct SubjectGuard;
    impl Drop for SubjectGuard {
        fn drop(&mut self) {
            SUBJECT.with(|s| *s.borrow_mut() = None);
        }
    }

    /// The subject of every class the test registers: the golden decoder's program, weights and layout, or the toy's.
    fn program() -> TirProgramV1 {
        subject().map_or_else(toy_program, |s| s.program)
    }

    fn weights(p: &TirProgramV1, salt: usize) -> MapParams {
        subject().map_or_else(|| toy_weights(p, salt), |s| s.weights)
    }

    fn layout(p: &TirProgramV1) -> PalwTirLayoutV1 {
        subject().map_or_else(|| toy_layout(p), |s| s.layout)
    }

    /// The layout the IR fixtures use for the golden programs: ragged multi-tile commit points and two-row history tiles.
    fn golden_layout(p: &TirProgramV1, positions: u32) -> PalwTirLayoutV1 {
        let mut tiles = Vec::new();
        let mut k = 0u32;
        for (bi, b) in p.blocks.iter().enumerate() {
            for (ni, n) in b.nodes.iter().enumerate() {
                if !n.commit {
                    continue;
                }
                let is_logits = bi == p.schedule.post as usize && ni == p.logits as usize;
                tiles.push(if is_logits { 4096 } else { 4 + (k * 7) % 6 });
                k += 1;
            }
        }
        PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: positions,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: tiles,
            state_tiles: (0..p.states.len() as u32).map(|j| 4 + j % 3).collect(),
        }
    }

    /// **The golden dense decoder with grouped-query attention as the subject** (`consensus-vectors/tir-v1/programs/
    /// dense-gqa-2layer.json`, RFC-0002 F7): its attention leaves are dissected. Returns the guard that restores the
    /// toy and the golden job's tokens.
    fn use_golden_subject() -> (SubjectGuard, Vec<u32>) {
        let path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v1/programs/dense-gqa-2layer.json");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
        let unhex =
            |s: &str| -> Vec<u8> { (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect() };
        let mut program = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
        let mut w = MapParams::default();
        for e in v["params"].as_array().unwrap() {
            let j = e["param"].as_u64().unwrap() as u16;
            let layer = e["layer"].as_u64().map(|l| l as u16);
            let d = &program.params[j as usize];
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            w.tensors.insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &unhex(e["le_hex"].as_str().unwrap())).unwrap());
        }
        let tokens: Vec<u32> = v["steps"].as_array().unwrap().iter().map(|s| s["token"].as_u64().unwrap() as u32).collect();
        program.logits_scheme_id.copy_from_slice(crate::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
        let program = TirProgramV1::decode_canonical(&program.encode()).expect("still canonical under the tiled scheme");
        let layout = golden_layout(&program, 12);
        SUBJECT.with(|s| *s.borrow_mut() = Some(Subject { program, weights: w, layout }));
        (SubjectGuard, tokens)
    }

    /// A toy IR class: an `i8` embedding, one layer of an `i8 [4, 4]` matrix and `i64` multiplier, an
    /// `i16` head over 16 ids (tiled logits).
    fn toy_program() -> TirProgramV1 {
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

    fn toy_weights(p: &TirProgramV1, salt: usize) -> MapParams {
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

    fn toy_layout(p: &TirProgramV1) -> PalwTirLayoutV1 {
        let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
        PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 24,
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
        for class in [LINE, CAND_A, JUDGE] {
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
        for who in [OWNER, ALICE, BOB, CAROL, DAVE] {
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
        at_with(state, p, daa, &PalwTransitionExtrasV1::default(), f)
    }

    /// [`at`] under the block's `extras` (the held regime's ladder, say).
    fn at_with(
        state: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        daa: u64,
        extras: &PalwTransitionExtrasV1,
        f: impl FnOnce(&mut TransitionBuilder<'_>),
    ) -> (PalwChainStateV2, PalwStateDeltaV2) {
        let mut builder = TransitionBuilder::new(state, p, false, false, false, false, extras);
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
        evaluating_custom(p, policy, &|_| {})
    }

    /// [`evaluating_with`] over a genesis state `setup` has prepared (registered datasets, say).
    fn evaluating_custom(
        p: &PalwStateParamsV2,
        policy: PalwImprovementPolicyV1,
        setup: &dyn Fn(&mut PalwChainStateV2),
    ) -> PalwChainStateV2 {
        let p = p.clone();
        let set = PalwImprovementPolicySetV1 { line_id: h(LINE), sequence: 1, policy: Some(policy) };
        let mut genesis = genesis();
        setup(&mut genesis);
        let (mut s, _) = at(&genesis, &p, 500, |b| {
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
        claim_with(s, item, subject, executor, claim_word, vec![3, 5, item % 16], None)
    }

    /// [`claim`] over a given prompt, carrying a suite entry's opening where there is one.
    fn claim_with(
        s: &PalwChainStateV2,
        item: u32,
        subject: PalwEvalSubjectV1,
        executor: u8,
        claim_word: u8,
        prompt: Vec<u32>,
        opening: Option<PalwSuiteOpeningV1>,
    ) -> PalwConsensusObjectV2 {
        claim_run(s, item, subject, executor, claim_word, prompt, opening, Lie::None).object
    }

    /// An executor's whole run of an evaluation claim: the object it files, and what it holds to defend it in court.
    struct EvalRun {
        object: PalwConsensusObjectV2,
        execution: crate::palw_gen_worker_v1::PalwGenExecutionV1,
        binding: PalwEvalBindingV1,
        prompt: Vec<u32>,
        weights: Weights,
    }

    /// What a lying executor does to its own run before it commits it (spec 17 §17.8.6).
    #[derive(Clone, Copy, Debug)]
    enum Lie {
        /// Nothing: an honest claim.
        None,
        /// A wrong value at one step leaf of the tree, committed consistently into its stage's root.
        Leaf { stage: usize, index: usize, delta: i128 },
        /// A wrong committed score (the lane off by `delta`) over an honest tree.
        Score { lane: usize, delta: i32 },
        /// A wrong generated id (position `t`) over an honest tree.
        Id { t: usize },
    }

    /// The tree's lie, committed: the leaf's hash, its stage's root and the step root recomputed over it.
    fn plant(e: &mut crate::palw_gen_worker_v1::PalwGenExecutionV1, lie: Lie) {
        match lie {
            Lie::Leaf { stage, index, delta } => {
                e.leaf_values[stage][index][0] += delta;
                let leaf = e.space.stages[stage].leaves()[index];
                e.leaf_hashes[stage][index] =
                    crate::palw_gen_step_v1::palw_gen_step_leaf_hash_v1(&leaf, &e.leaf_values[stage][index]).unwrap();
                e.claim.stage_roots[stage] = crate::palw_gen_step_v1::palw_gen_stage_root_v1(stage as u8, &e.leaf_hashes[stage]);
                e.claim.step_root = crate::palw_gen_step_v1::palw_gen_step_root_v1(&e.claim.stage_roots);
            }
            Lie::Id { t } => e.claim.generated[t] ^= 1,
            Lie::None | Lie::Score { .. } => {}
        }
    }

    /// [`claim_with`], its run kept, and a `lie` planted in it before the commit.
    #[allow(clippy::too_many_arguments)]
    fn claim_run(
        s: &PalwChainStateV2,
        item: u32,
        subject: PalwEvalSubjectV1,
        executor: u8,
        claim_word: u8,
        prompt: Vec<u32>,
        opening: Option<PalwSuiteOpeningV1>,
        lie: Lie,
    ) -> EvalRun {
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
            part: 0,
            mode: PalwEvalModeV1::Generate { seed: item_row.seed, max_new: 4, stop_ids: vec![] },
        };
        let p = program();
        let layout = layout(&p);
        let subject_row = PalwEvalSubjectClassV1 { class_id: class, artifact_root: class, program: &p, layout: &layout };
        let context =
            palw_improve_eval_context_v1(&job, &subject_row, PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 }).unwrap();
        let probe = misaka_palw_tir::pipeline::PipelineJob { prompt: prompt.clone(), ..Default::default() };
        let salt = if class == h(LINE) { 0 } else { 1 };
        let weights = Weights(vec![weights(&p, salt)]);
        let mut e = crate::palw_gen_worker_v1::palw_gen_execute_v1(
            &context.pipeline,
            &context.programs,
            &context.layouts,
            &weights,
            &probe,
            context.decode.as_ref().unwrap(),
            context.seed,
        )
        .unwrap();
        plant(&mut e, lie);
        let params = PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 };
        let binding =
            PalwEvalBindingV1::of(&job, class, &layout, &e.claim, e.space.leaf_count(), &probe.prompt, params, vec![], vec![]);
        let roots = binding.claim_roots();
        let object = PalwConsensusObjectV2::FreePromptCommitted {
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
                tail: PalwEvalClaimTailV1 {
                    generated: e.claim.generated.clone(),
                    score: vec![],
                    subject_layout: layout,
                    params,
                    read: None,
                    opening,
                },
            })),
        };
        EvalRun { object, execution: e, binding, prompt: probe.prompt, weights }
    }

    /// The ids `class`'s model decodes for `prompt` under the test policy's budget: what an honest executor claims.
    fn generation_of(class: u8, prompt: &[u32]) -> Vec<u32> {
        let p = program();
        let layout = layout(&p);
        let job = PalwEvalJobV1 {
            line_id: h(LINE),
            epoch: 1,
            item: 0,
            subject: PalwEvalSubjectV1::Parent,
            kind: PalwScoringKindV1::ExactMatch,
            part: 0,
            mode: PalwEvalModeV1::Generate { seed: h(0), max_new: 4, stop_ids: vec![] },
        };
        let row = PalwEvalSubjectClassV1 { class_id: h(class), artifact_root: h(class), program: &p, layout: &layout };
        let ctx = palw_improve_eval_context_v1(&job, &row, PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 }).unwrap();
        let probe = misaka_palw_tir::pipeline::PipelineJob { prompt: prompt.to_vec(), ..Default::default() };
        let salt = if class == LINE { 0 } else { 1 };
        crate::palw_gen_worker_v1::palw_gen_execute_v1(
            &ctx.pipeline,
            &ctx.programs,
            &ctx.layouts,
            &Weights(vec![weights(&p, salt)]),
            &probe,
            ctx.decode.as_ref().unwrap(),
            ctx.seed,
        )
        .unwrap()
        .claim
        .generated
    }

    /// An honest teacher-forced claim — a likelihood item's, a suite continuation's, or a judged part's — of
    /// `class` over `prompt`, its ids the `reference`: the object the extractor builds from the payload.
    #[allow(clippy::too_many_arguments)]
    fn forced_object(
        job: PalwEvalJobV1,
        class: u8,
        executor: u8,
        claim_word: u8,
        prompt: Vec<u32>,
        reference: Vec<u32>,
        params: PalwEvalStageParamsV1,
        finalized: Vec<Vec<u32>>,
        read: Option<PalwEvalReadV1>,
        opening: Option<PalwSuiteOpeningV1>,
    ) -> PalwConsensusObjectV2 {
        forced_run(job, class, executor, claim_word, prompt, reference, params, finalized, read, opening, Lie::None).object
    }

    /// [`forced_object`], its run kept, and a `lie` planted in it before the commit.
    #[allow(clippy::too_many_arguments)]
    fn forced_run(
        job: PalwEvalJobV1,
        class: u8,
        executor: u8,
        claim_word: u8,
        prompt: Vec<u32>,
        reference: Vec<u32>,
        params: PalwEvalStageParamsV1,
        finalized: Vec<Vec<u32>>,
        read: Option<PalwEvalReadV1>,
        opening: Option<PalwSuiteOpeningV1>,
        lie: Lie,
    ) -> EvalRun {
        let p = program();
        let layout = layout(&p);
        let row = PalwEvalSubjectClassV1 { class_id: h(class), artifact_root: h(class), program: &p, layout: &layout };
        let context = palw_improve_eval_context_v1(&job, &row, params).unwrap();
        let pj = misaka_palw_tir::pipeline::PipelineJob {
            prompt: prompt.clone(),
            generated: reference.clone(),
            scalars: context.scalars.clone(),
            ..Default::default()
        };
        let salt = match class {
            LINE => 0,
            JUDGE => 2,
            _ => 1,
        };
        let weights = Weights(vec![weights(&p, salt), MapParams::default(), MapParams::default()]);
        let mut e = crate::palw_gen_worker_v1::palw_gen_replay_committed_v1(
            &context.pipeline,
            &context.programs,
            &context.layouts,
            &weights,
            &pj,
            context.seed,
        )
        .unwrap();
        plant(&mut e, lie);
        let mut score: Vec<i32> = e.run.output.data.iter().map(|v| *v as i32).collect();
        if let Lie::Score { lane, delta } = lie {
            score[lane] += delta;
        }
        let binding =
            PalwEvalBindingV1::of(&job, h(class), &layout, &e.claim, e.space.leaf_count(), &prompt, params, finalized, score.clone());
        let roots = binding.claim_roots();
        let run_prompt = prompt.clone();
        let object = PalwConsensusObjectV2::FreePromptCommitted {
            claim: h(claim_word),
            class_id: h(class),
            bond: bond(executor),
            executor_pubkey: vec![executor],
            work_leaves: roots.work_leaves,
            prompt_token_ids_hash: Hash64::default(),
            prompt_tokens: prompt.len() as u32,
            prompt_token_ids: prompt,
            decode_tokens_executed: reference.len() as u32,
            trace_root: roots.trace_root,
            output_root: roots.output_root,
            execution_root: roots.execution_root,
            trace_chunk_count: 1,
            trace_retention_daa: 0,
            consumed_prefix_state: crate::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(h(class)),
            job_pin: Hash64::default(),
            eval: Some(Box::new(PalwFpEvalCarriageV1 {
                job,
                tail: PalwEvalClaimTailV1 { generated: reference, score, subject_layout: layout, params, read, opening },
            })),
        };
        EvalRun { object, execution: e, binding, prompt: run_prompt, weights }
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
        let row = s1.improvement_eval_job(&h(LINE), 1, 0, &PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch, 0).unwrap();
        let taken = row.claim.unwrap();
        assert_eq!((taken.claim_id, taken.executor, taken.accepted_daa, taken.final_daa), (h(0xA0), bond(CAROL), 1_600, None));
        assert_eq!(taken.answer, palw_improve_answer_of_v1(&generated_of(&first), -1, -1), "the answer span is kept for the reveal");
        let claim_row = s1.claims.get(&h(0xA0)).unwrap();
        assert!(matches!(claim_row.source, PalwClaimSourceV2::FreePrompt { quanta: 0, .. }), "no quanta");
        assert_eq!(
            (claim_row.pwu, claim_row.immature_contribution, claim_row.escrowed_reward, claim_row.rights_reserved),
            (0, 0, 0, 0)
        );
        // The lock: the work's own (leaves × slash, ρ = 1) or the claim's swing, whichever is more.
        let swing = palw_improve_eval_swing_lock_v1(palw_improve_max_promotion_payout_v1(
            s.improvement_pools.get(&h(LINE)).map_or(0, |pool| pool.balance),
            policy().promotion_share_permille,
        ));
        let work = palw_improve_eval_reservation_v1(claim_row.work_leaves, 3, 1);
        assert_eq!(claim_row.reserved, work.max(swing), "the work's lock, or what the claim could swing");
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
        assert_eq!(refused_as(&wrong_layout, 1_600), "the carried layout is not the executed class's");
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
            prefix_state_armed: false,
            prefix_inherit_armed: false,
            constraint_armed: false,
            constraint_v2_armed: false,
            tokenizer: crate::palw_fp_tokenizer_v1::PalwFpTokenizerRuleV1::Dormant,
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
        let (
            PalwConsensusObjectV2::FreePromptCommitted { eval: built, .. },
            PalwConsensusObjectV2::FreePromptCommitted { eval: extracted, .. },
        ) = (&honest, &carried.object)
        else {
            panic!("free-prompt commitments")
        };
        assert_eq!(built, extracted);
        // …and the fold takes it: the job's row is written and the claim is the chain's.
        let (s1, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &carried.object).expect("the extracted claim folds"));
        let row = s1
            .improvement_eval_job(&h(LINE), 1, 3, &PalwEvalSubjectV1::Candidate(h(CAND_A)), PalwScoringKindV1::ExactMatch, 0)
            .unwrap();
        let taken = row.claim.unwrap();
        let PalwConsensusObjectV2::FreePromptCommitted { claim: id, .. } = &carried.object else { unreachable!() };
        assert_eq!((taken.claim_id, taken.executor, taken.accepted_daa), (*id, bond(BOB), 1_600));
        assert!(palw_improve_claim_is_evaluation_v1(s1.claims.get(id).unwrap()), "an evaluation claim: no quanta");
        // The walk skips what its door refuses: below the decode rules, or unsigned — and nothing reaches the fold.
        assert!(extract(&tx, crate::palw_freeprompt_v3::PalwFpDecodeRulesV1::Dormant, true).objects.is_empty());
        assert!(extract(&tx, active, false).objects.is_empty());
    }

    /// Fold `object` on `s` at `daa` as a block's object step would, on the state and params given.
    fn fold_one(
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        daa: u64,
        object: &PalwConsensusObjectV2,
    ) -> Result<(), PalwStateV2Error> {
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
        let refusal = Err(PalwStateV2Error::ImprovementRefused(
            "an evaluation claim past its job's share of the epoch's evaluation budget (MIP-20)",
        ));
        // By the policy's own budget.
        let p = params();
        for (budget, fits) in
            [(positions * jobs, true), (positions * jobs + jobs - 1, true), (positions * jobs - 1, false), (jobs, false)]
        {
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
        judged
            .stages
            .push(PalwScoringStageV1 { kind: PalwScoringKindV1::Judge, params: PalwScoringParamsV1::Judge { lo: 0, hi: 10 } });
        judged
            .stages
            .push(PalwScoringStageV1 { kind: PalwScoringKindV1::Pairwise, params: PalwScoringParamsV1::Pairwise { margin: 0 } });
        assert_eq!(
            palw_improve_eval_epoch_jobs_v1(&judged, 2),
            (8 * 3) * 2 + 8 * 4,
            "a judge's two claims for every subject's item, a pairwise score's four for every other subject's"
        );
        // A suite's items are claimable — each discloses its entry in its claim (§17.8.1) — one per entry and
        // subject; a key's scoring adds none.
        let mut suites = policy().eval;
        suites.regression_items = 32;
        suites.safety_items = 16;
        assert_eq!(palw_improve_eval_epoch_jobs_v1(&suites, 2), (8 + 32 + 16) * 2, "the drawn items and the suites'");
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
        assert_eq!(
            fold_one(&frozen, &p, 1_600, &candidate),
            Err(PalwStateV2Error::FrozenClass(h(CAND_A))),
            "a frozen class takes no claim"
        );
        // The parent's class too: the head is no exception.
        let parent = claim(&s, 1, PalwEvalSubjectV1::Parent, CAROL, 0xE3);
        let mut frozen_head = s.clone();
        frozen_head.classes.get_mut(&h(LINE)).unwrap().status = PalwClassStatusV2::Frozen { since_daa: 1_550 };
        assert_eq!(fold_one(&frozen_head, &p, 1_600, &parent), Err(PalwStateV2Error::FrozenClass(h(LINE))));

        // The share: raise the class's price per unit of work until the claim's reservation passes
        // the fence's share of its executor's room — the refusal sits between "folds" and the room's own
        // ceiling (`FreePromptExposureCeiling`), and a network that gives evaluation its whole room
        // (1,000 permille) never refuses it.
        let share = |permille: u16| {
            params().with_improve_ceilings(Some(crate::palw_improve_v1::PalwImprovementCeilingsV1 {
                max_eval_budget_permille: permille,
                ..PALW_DRILL_IMPROVE_CEILINGS_V1
            }))
        };
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
        use crate::palw_improve_material_v1::{PalwCaseReferenceV1, PalwCaseSourceV1, PalwHardCaseRecordV1, PalwHardCaseV1};
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
        voided.claims.get_mut(&id).unwrap().phase =
            PalwClaimPhaseV2::Voided { voided_daa: 1_650, reason: PalwVoidReasonV2::BindTimeout };
        let retake = claim(&s, 4, PalwEvalSubjectV1::Candidate(h(CAND_A)), BOB, 0xE8);
        assert_eq!(fold_one(&voided, &p, 1_700, &retake), Ok(()), "the job is re-taken");
    }

    // ---- suites: a registered public dataset, drawn by R, disclosed by the claim's opening ----------------

    const SUITE_DS: u8 = 0x72;

    /// Six public suite entries: the even ones exact-match entries whose key is the parent's own answer (so the
    /// parent passes and a model with other weights fails), the odd ones likelihood entries.
    fn suite_entries() -> Vec<PalwSuiteEntryV1> {
        (0..6u32)
            .map(|i| {
                let prompt = vec![7, i, 1];
                let reference = if i % 2 == 0 {
                    PalwSuiteReferenceV1::ExactKey(generation_of(LINE, &prompt))
                } else {
                    PalwSuiteReferenceV1::Continuation(vec![4, 9, 2])
                };
                PalwSuiteEntryV1 { prompt, reference }
            })
            .collect()
    }

    fn registered(dataset_id: u8, content_root: Hash64, items: u64) -> PalwDatasetRecordV1 {
        PalwDatasetRecordV1 {
            dataset: crate::palw_improve_material_v1::PalwDatasetV1 {
                line_id: h(LINE),
                dataset_id: h(dataset_id),
                content_root,
                items,
                license_classes: vec![h(0x91)],
                teacher_classes: 1,
                provenance_commitment: h(0x92),
            },
            registrant: bond(CAROL),
            registered_daa: 10,
            bond: 6,
            epoch: 0,
        }
    }

    /// An epoch evaluating with a regression suite of four entries drawn from [`suite_entries`]; both primary stages.
    fn evaluating_with_suite() -> (PalwChainStateV2, Vec<PalwSuiteEntryV1>, Vec<Hash64>) {
        let entries = suite_entries();
        let leaves: Vec<Hash64> = entries.iter().map(palw_improve_suite_leaf_v1).collect();
        let root = palw_improve_suite_root_v1(&leaves).unwrap();
        let mut pol = policy();
        pol.eval.stages.push(PalwScoringStageV1 {
            kind: PalwScoringKindV1::RefLogLik,
            params: PalwScoringParamsV1::RefLogLik { logit_scale_q24: 1 << 12 },
        });
        pol.eval.regression_dataset = h(SUITE_DS);
        pol.eval.regression_items = 4;
        let s = evaluating_custom(&params(), pol, &|g| {
            g.improvement_datasets.insert((h(LINE), h(SUITE_DS)), registered(SUITE_DS, root, entries.len() as u64));
        });
        (s, entries, leaves)
    }

    fn opening_of(entries: &[PalwSuiteEntryV1], leaves: &[Hash64], index: u32) -> PalwSuiteOpeningV1 {
        PalwSuiteOpeningV1 {
            reference: entries[index as usize].reference.clone(),
            proof: palw_improve_suite_proof_v1(leaves, index as usize).unwrap(),
        }
    }

    /// **A suite is public content, drawn by R and disclosed by the claim** (spec 17 §17.8.1; RFC-0004 §7 as decided
    /// 2026-09-30): the epoch draws `regression_items` distinct entries of the registered dataset the policy names;
    /// a claim on such an item carries its entry's reference and an inclusion proof under the dataset's
    /// `content_root`, its prompt being the claim's own; an exact-match entry is scored from its public key at
    /// `Final` (no reveal), a likelihood entry is a teacher-forced pass over its continuation; and nothing else
    /// takes an opening.
    #[test]
    fn a_suite_item_discloses_its_entry_in_its_claim_and_scores_from_it() {
        let p = params();
        let (s, entries, leaves) = evaluating_with_suite();
        let header = s.improvement_epoch(&h(LINE), 1).unwrap();
        assert_eq!(header.items, 12, "eight drawn items and four suite items");
        let drawn: Vec<(u32, u32)> = (8..12u32)
            .map(|item| match s.improvement_item(&h(LINE), 1, item).unwrap().source {
                PalwItemSourceV1::Regression { index } => (item, index),
                other => panic!("a regression item, got {other:?}"),
            })
            .collect();
        let mut distinct: Vec<u32> = drawn.iter().map(|(_, index)| *index).collect();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), 4, "distinct entries");
        assert!(distinct.iter().all(|index| *index < 6));
        let seed = header.seed.unwrap();
        assert_eq!(
            drawn.iter().map(|(_, index)| *index).collect::<Vec<_>>(),
            palw_improve_suite_draw_v1(&seed, &h(SUITE_DS), 1, 6, 4),
            "the draw is R's, by the epoch seed"
        );
        let (exact_item, exact_index) =
            *drawn.iter().find(|(_, index)| index % 2 == 0).expect("an exact-match entry among four of six");
        let (likely_item, likely_index) =
            *drawn.iter().find(|(_, index)| index % 2 == 1).expect("a likelihood entry among four of six");
        let exact = &entries[exact_index as usize];
        let open_exact = Some(opening_of(&entries, &leaves, exact_index));
        let claim_of = |subject: PalwEvalSubjectV1, executor: u8, word: u8, opening: Option<PalwSuiteOpeningV1>| {
            claim_with(&s, exact_item, subject, executor, word, exact.prompt.clone(), opening)
        };

        // The parent's claim opens an exact-match entry and folds; its score comes from the entry's public key.
        let parent = claim_of(PalwEvalSubjectV1::Parent, CAROL, 0xF0, open_exact.clone());
        let candidate = claim_of(PalwEvalSubjectV1::Candidate(h(CAND_A)), BOB, 0xF1, open_exact.clone());
        let PalwSuiteReferenceV1::ExactKey(key) = &exact.reference else { panic!("an exact-match entry") };
        assert_ne!(generation_of(CAND_A, &exact.prompt), *key, "the premise: other weights, another answer");
        let (s1, _) = at(&s, &p, 1_600, |b| {
            apply_object(b, &ctx(1_600), &parent).expect("an honest suite claim");
            apply_object(b, &ctx(1_600), &candidate).expect("an honest suite claim");
        });
        let key_of = |s: &PalwChainStateV2, subject: PalwEvalSubjectV1| {
            s.improvement_eval_job(&h(LINE), 1, exact_item, &subject, PalwScoringKindV1::ExactMatch, 0).unwrap().claim.unwrap().score
        };
        assert_eq!(key_of(&s1, PalwEvalSubjectV1::Parent), Some(1), "the parent's answer is the key");
        assert_eq!(key_of(&s1, PalwEvalSubjectV1::Candidate(h(CAND_A))), Some(0), "the candidate's is not");
        // At Final the score is recorded and nobody reveals anything: the key was public.
        let (s2, _) = at(&s1, &p, 1_650, |b| {
            for id in [h(0xF0), h(0xF1)] {
                let row = b.state.claims.get(&id).cloned().unwrap();
                b.note_improvement_eval_final_v1(&id, &row, 1_650).unwrap();
            }
        });
        let score = |subject: PalwEvalSubjectV1| {
            s2.improvement_result(&h(LINE), 1, exact_item, &subject)
                .and_then(|r| r.scores.first().copied())
                .map(|sc| (sc.kind, sc.value))
        };
        assert_eq!(score(PalwEvalSubjectV1::Parent), Some((PalwScoringKindV1::ExactMatch, 1)));
        assert_eq!(score(PalwEvalSubjectV1::Candidate(h(CAND_A))), Some((PalwScoringKindV1::ExactMatch, 0)));
        assert!(!s2.improvement_eval_pending_v1(&h(LINE), 1), "scored at Final: no key to wait for");

        // The refusals, by name.
        let refused_as = |object: &PalwConsensusObjectV2| match fold_one(&s, &p, 1_600, object) {
            Err(PalwStateV2Error::ImprovementRefused(why)) => why,
            other => panic!("refused by name, got {other:?}"),
        };
        let mut bad_proof = open_exact.clone().unwrap();
        bad_proof.proof[0] = h(0xEE);
        assert_eq!(
            refused_as(&claim_of(PalwEvalSubjectV1::Parent, CAROL, 0xF2, Some(bad_proof))),
            "a suite entry's opening does not verify under its dataset's content_root"
        );
        let mut forged_key = open_exact.clone().unwrap();
        forged_key.reference = PalwSuiteReferenceV1::ExactKey(vec![1, 1]);
        assert_eq!(
            refused_as(&claim_of(PalwEvalSubjectV1::Parent, CAROL, 0xF3, Some(forged_key))),
            "a suite entry's opening does not verify under its dataset's content_root",
            "a key the dataset does not hold"
        );
        assert_eq!(refused_as(&claim_of(PalwEvalSubjectV1::Parent, CAROL, 0xF4, None)), "a suite item's claim opens its entry");
        let other_entry = opening_of(&entries, &leaves, (exact_index + 2) % 6);
        assert_eq!(
            refused_as(&claim_of(PalwEvalSubjectV1::Parent, CAROL, 0xF5, Some(other_entry))),
            "a suite entry's opening does not verify under its dataset's content_root",
            "another entry's proof is another leaf"
        );
        let holdout = claim_with(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xF6, vec![3, 5, 0], open_exact.clone());
        assert_eq!(refused_as(&holdout), "only a suite item's claim carries an opening");

        // A likelihood entry: a teacher-forced pass over its continuation, the entry opened the same way.
        let likely = &entries[likely_index as usize];
        let PalwSuiteReferenceV1::Continuation(continuation) = &likely.reference else { panic!("a likelihood entry") };
        let stage = PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 };
        let forced = |subject: PalwEvalSubjectV1, class: u8, word: u8, opening: Option<PalwSuiteOpeningV1>| {
            forced_object(
                PalwEvalJobV1 {
                    line_id: h(LINE),
                    epoch: 1,
                    item: likely_item,
                    subject,
                    kind: PalwScoringKindV1::RefLogLik,
                    part: 0,
                    mode: PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::default() },
                },
                class,
                CAROL,
                word,
                likely.prompt.clone(),
                continuation.clone(),
                stage,
                vec![],
                None,
                opening,
            )
        };
        let open_likely = Some(opening_of(&entries, &leaves, likely_index));
        let honest = forced(PalwEvalSubjectV1::Parent, LINE, 0xF7, open_likely.clone());
        let (s3, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &honest).expect("a likelihood suite claim"));
        let row =
            s3.improvement_eval_job(&h(LINE), 1, likely_item, &PalwEvalSubjectV1::Parent, PalwScoringKindV1::RefLogLik, 0).unwrap();
        assert!(row.claim.unwrap().score.is_some(), "its committed log-likelihood");
        let wrong_ids = {
            let mut o = honest.clone();
            let PalwConsensusObjectV2::FreePromptCommitted { eval: Some(eval), .. } = &mut o else { unreachable!() };
            eval.tail.generated[0] ^= 1;
            o
        };
        assert_eq!(refused_as(&wrong_ids), "a teacher-forced job's ids are the item's reference");
        // A generation claim on a likelihood entry: its opening's reference is not the job's kind.
        let generation = claim_with(&s, likely_item, PalwEvalSubjectV1::Parent, CAROL, 0xF8, likely.prompt.clone(), open_likely);
        assert_eq!(refused_as(&generation), "a suite entry's reference is not the job's kind");
        // And a teacher-forced claim that opens an exact-match entry is refused the same way.
        let mismatched = forced(PalwEvalSubjectV1::Parent, LINE, 0xF9, Some(opening_of(&entries, &leaves, exact_index)));
        assert!(matches!(fold_one(&s, &p, 1_600, &mismatched), Err(PalwStateV2Error::ImprovementRefused(_))));
    }

    // ---- judges: the deterministic teacher-forced pass over the registered template's fill ---------------------

    const TEMPLATE_J: u8 = 0x73;
    const TEMPLATE_P: u8 = 0x74;
    const VERDICT_A: [u32; 1] = [7];
    const VERDICT_B: [u32; 2] = [2, 9];
    const SCALE: i32 = 1 << 12;
    const LO_HI: i32 = 2_000_000_000;

    fn judge_template() -> PalwJudgeTemplateV1 {
        PalwJudgeTemplateV1 { segments: vec![vec![1], vec![2], vec![3]] }
    }

    fn pairwise_template() -> PalwJudgeTemplateV1 {
        PalwJudgeTemplateV1 { segments: vec![vec![1], vec![2], vec![3], vec![4]] }
    }

    /// An epoch evaluating with a Judge stage and a Pairwise stage (the judge class `JUDGE`, a template dataset for
    /// each) over eight hold-out items.
    fn evaluating_judged() -> PalwChainStateV2 {
        let mut pol = policy();
        pol.eval
            .stages
            .push(PalwScoringStageV1 { kind: PalwScoringKindV1::Judge, params: PalwScoringParamsV1::Judge { lo: -LO_HI, hi: LO_HI } });
        pol.eval
            .stages
            .push(PalwScoringStageV1 { kind: PalwScoringKindV1::Pairwise, params: PalwScoringParamsV1::Pairwise { margin: 0 } });
        pol.eval.judge_set = vec![h(JUDGE)];
        pol.eval.judge = Some(PalwJudgeSpecV1 {
            template_dataset: h(TEMPLATE_J),
            verdict_a: VERDICT_A.to_vec(),
            verdict_b: VERDICT_B.to_vec(),
            logit_scale_q24: SCALE,
        });
        pol.eval.pairwise = Some(PalwJudgeSpecV1 {
            template_dataset: h(TEMPLATE_P),
            verdict_a: VERDICT_A.to_vec(),
            verdict_b: VERDICT_B.to_vec(),
            logit_scale_q24: SCALE,
        });
        evaluating_custom(&params(), pol, &|g| {
            g.improvement_datasets
                .insert((h(LINE), h(TEMPLATE_J)), registered(TEMPLATE_J, palw_improve_judge_template_root_v1(&judge_template()), 1));
            g.improvement_datasets.insert(
                (h(LINE), h(TEMPLATE_P)),
                registered(TEMPLATE_P, palw_improve_judge_template_root_v1(&pairwise_template()), 1),
            );
        })
    }

    /// One part of a judged score, as its executor would claim it: the judge's pass over the template's fill of the
    /// item's prompt and `outputs` (in the order shown), its reference the part's verdict sequence.
    #[allow(clippy::too_many_arguments)]
    fn judged_part(
        item: u32,
        subject: PalwEvalSubjectV1,
        kind: PalwScoringKindV1,
        part: u8,
        template: &PalwJudgeTemplateV1,
        outputs: &[&[u32]],
        executor: u8,
        word: u8,
    ) -> PalwConsensusObjectV2 {
        let (_, verdict) = palw_improve_judged_part_v1(kind, part).unwrap();
        let verdict_ids: Vec<u32> = if verdict == 0 { VERDICT_A.to_vec() } else { VERDICT_B.to_vec() };
        let item_prompt = vec![3u32, 5, item % 16];
        let prompt = palw_improve_judge_fill_v1(template, &item_prompt, outputs).unwrap();
        let params = if kind == PalwScoringKindV1::Judge {
            PalwEvalStageParamsV1::Judge { lo: -LO_HI, hi: LO_HI, logit_scale_q24: SCALE }
        } else {
            PalwEvalStageParamsV1::Pairwise { margin: 0, logit_scale_q24: SCALE }
        };
        forced_object(
            PalwEvalJobV1 { line_id: h(LINE), epoch: 1, item, subject, kind, part, mode: PalwEvalModeV1::Judged { judge: h(JUDGE) } },
            JUDGE,
            executor,
            word,
            prompt,
            verdict_ids,
            params,
            outputs.iter().map(|o| o.to_vec()).collect(),
            Some(PalwEvalReadV1 {
                template: template.clone(),
                item_len: item_prompt.len() as u32,
                output_lens: outputs.iter().map(|o| o.len() as u32).collect(),
            }),
            None,
        )
    }

    /// A part's committed log-likelihood.
    fn part_value(object: &PalwConsensusObjectV2) -> i64 {
        let PalwConsensusObjectV2::FreePromptCommitted { eval: Some(eval), .. } = object else { panic!("an evaluation claim") };
        misaka_palw_tir::scoring::ref_loglik_join_v1(eval.tail.score[0], eval.tail.score[1])
    }

    fn finalize_all(s: &PalwChainStateV2, p: &PalwStateParamsV2, daa: u64, ids: &[Hash64]) -> PalwChainStateV2 {
        at(s, p, daa, |b| {
            for id in ids {
                let row = b.state.claims.get(id).cloned().unwrap();
                b.note_improvement_eval_final_v1(id, &row, daa).unwrap();
            }
        })
        .0
    }

    /// **A judged score is the judge's pass, part by part** (spec 17 §17.8.5; RFC-0004 §7.3 as decided 2026-09-30).
    /// A Judge score is two teacher-forced passes of the judge class over the registered template filled with the
    /// item's prompt and the subject's FINAL generation — one per verdict sequence — recorded as the margin of
    /// the two once both are final. A Pairwise score is four: both orders × both verdicts, the judge seeing the
    /// parent's output and the candidate's in each order; the outcome is +1 only when the two margins' sum
    /// (from the candidate's side) beats the margin, and a tie goes to the incumbent.
    #[test]
    fn a_judged_score_is_the_judges_teacher_forced_pass_part_by_part() {
        let p = params();
        let s = evaluating_judged();
        assert_eq!(s.improvement_item(&h(LINE), 1, 0).unwrap().judge, Some(h(JUDGE)), "the draw named the judge");
        let (parent_gen, cand_gen) = {
            let parent = claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xA0);
            let cand = claim(&s, 0, PalwEvalSubjectV1::Candidate(h(CAND_A)), BOB, 0xA1);
            (generated_of(&parent), generated_of(&cand))
        };
        let (jt, pt) = (judge_template(), pairwise_template());
        let cand_subject = PalwEvalSubjectV1::Candidate(h(CAND_A));
        let refused_as = |s: &PalwChainStateV2, daa: u64, object: &PalwConsensusObjectV2| match fold_one(s, &p, daa, object) {
            Err(PalwStateV2Error::ImprovementRefused(why)) => why,
            other => panic!("refused by name, got {other:?}"),
        };

        // No generation is final: a judge has nothing to read.
        let early = judged_part(0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge, 0, &jt, &[&parent_gen], DAVE, 0xB0);
        assert_eq!(refused_as(&s, 1_600, &early), "the generation a judged part reads is not final");

        // Both generations run and finalize (the key, were there to be one, would still be hidden).
        let (s1, _) = at(&s, &p, 1_600, |b| {
            apply_object(b, &ctx(1_600), &claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xA0)).unwrap();
            apply_object(b, &ctx(1_600), &claim(&s, 0, cand_subject, BOB, 0xA1)).unwrap();
        });
        let s2 = finalize_all(&s1, &p, 1_650, &[h(0xA0), h(0xA1)]);

        // ---- Judge: the parent's output, two parts ----
        let a = judged_part(0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge, 0, &jt, &[&parent_gen], ALICE, 0xB1);
        let b = judged_part(0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge, 1, &jt, &[&parent_gen], ALICE, 0xB2);
        let (s3, _) = at(&s2, &p, 1_660, |bld| {
            apply_object(bld, &ctx(1_660), &a).expect("part 0");
            apply_object(bld, &ctx(1_660), &b).expect("part 1");
        });
        let ids = |s: &PalwChainStateV2, subject: PalwEvalSubjectV1| {
            s.improvement_result(&h(LINE), 1, 0, &subject).map(|r| r.scores.iter().map(|sc| (sc.kind, sc.value)).collect::<Vec<_>>())
        };
        // One part final: nothing recorded yet, and the fee is paid at each part's Final.
        let earned = s3.improvement_earnings(&bond(ALICE));
        let s4 = finalize_all(&s3, &p, 1_700, &[h(0xB1)]);
        assert!(
            ids(&s4, PalwEvalSubjectV1::Parent).unwrap_or_default().iter().all(|(kind, _)| *kind != PalwScoringKindV1::Judge),
            "a judged score waits for all its parts"
        );
        assert_eq!(
            s4.improvement_earnings(&bond(ALICE)),
            earned + policy().fees.eval_fee_per_job,
            "each part earns the fee at its Final"
        );
        let s5 = finalize_all(&s4, &p, 1_701, &[h(0xB2)]);
        let margin = (part_value(&a) as i128 - part_value(&b) as i128).clamp(-(LO_HI as i128), LO_HI as i128) as i64;
        assert_eq!(
            ids(&s5, PalwEvalSubjectV1::Parent).unwrap().iter().find(|(k, _)| *k == PalwScoringKindV1::Judge).map(|(_, v)| *v),
            Some(margin),
            "the judge's preference for the first verdict over the second on the parent's output"
        );
        // A part, once final, is never claimed again; and a third part does not exist.
        let again = judged_part(0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge, 0, &jt, &[&parent_gen], ALICE, 0xB3);
        assert_eq!(refused_as(&s5, 1_710, &again), "the evaluation job is taken by a live claim");
        let third = {
            let mut o = a.clone();
            let PalwConsensusObjectV2::FreePromptCommitted { eval: Some(eval), claim, .. } = &mut o else { unreachable!() };
            eval.job.part = 2;
            *claim = h(0xB4);
            o
        };
        assert_eq!(refused_as(&s2, 1_660, &third), "a part past the kind's claims");

        // ---- the refusals: every input is the chain's ----
        let mine =
            |part: u8, out: &[&[u32]]| judged_part(0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge, part, &jt, out, DAVE, 0xC0);
        assert_eq!(
            refused_as(&s2, 1_660, &mine(0, &[&cand_gen])),
            "a judged part did not read the final generations its order shows",
            "the candidate's output is not the parent's"
        );
        let other_template = PalwJudgeTemplateV1 { segments: vec![vec![1], vec![2], vec![5]] };
        let wrong_template =
            judged_part(0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge, 0, &other_template, &[&parent_gen], DAVE, 0xC1);
        assert_eq!(refused_as(&s2, 1_660, &wrong_template), "a judged part's template is not the registered one");
        let lying = {
            let mut o = mine(0, &[&parent_gen]);
            let PalwConsensusObjectV2::FreePromptCommitted { eval: Some(eval), claim, .. } = &mut o else { unreachable!() };
            eval.tail.read.as_mut().unwrap().item_len -= 1;
            *claim = h(0xC2);
            o
        };
        assert_eq!(refused_as(&s2, 1_660, &lying), "a judged part's prompt is not its template's fill");
        let wrong_verdict = {
            let mut o = mine(0, &[&parent_gen]);
            let PalwConsensusObjectV2::FreePromptCommitted { eval: Some(eval), claim, .. } = &mut o else { unreachable!() };
            eval.tail.generated = VERDICT_B.to_vec();
            *claim = h(0xC3);
            o
        };
        assert_eq!(refused_as(&s2, 1_660, &wrong_verdict), "a judged part's ids are its verdict sequence");
        let wrong_class = {
            let mut o = mine(0, &[&parent_gen]);
            let PalwConsensusObjectV2::FreePromptCommitted { class_id, claim, .. } = &mut o else { unreachable!() };
            *class_id = h(LINE);
            *claim = h(0xC4);
            o
        };
        assert_eq!(refused_as(&s2, 1_660, &wrong_class), "the claim's class is not its item's judge");
        let no_reading = {
            let mut o = mine(0, &[&parent_gen]);
            let PalwConsensusObjectV2::FreePromptCommitted { eval: Some(eval), claim, .. } = &mut o else { unreachable!() };
            eval.tail.read = None;
            *claim = h(0xC5);
            o
        };
        assert_eq!(refused_as(&s2, 1_660, &no_reading), "a judged part carries its reading");

        // ---- Pairwise: the candidate against the parent, both orders ----
        let pair = |part: u8, order_outputs: [&[u32]; 2], word: u8| {
            judged_part(0, cand_subject, PalwScoringKindV1::Pairwise, part, &pt, &order_outputs, ALICE, word)
        };
        // Order 0 shows the candidate's output first; order 1 the parent's.
        let parts = [
            pair(0, [&cand_gen, &parent_gen], 0xD0),
            pair(1, [&cand_gen, &parent_gen], 0xD1),
            pair(2, [&parent_gen, &cand_gen], 0xD2),
            pair(3, [&parent_gen, &cand_gen], 0xD3),
        ];
        let swapped = pair(0, [&parent_gen, &cand_gen], 0xD4);
        assert_eq!(
            refused_as(&s2, 1_660, &swapped),
            "a judged part did not read the final generations its order shows",
            "part 0 shows the candidate's output first"
        );
        let parent_pair =
            judged_part(0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::Pairwise, 0, &pt, &[&parent_gen, &parent_gen], ALICE, 0xD5);
        assert_eq!(refused_as(&s2, 1_660, &parent_pair), "a pairwise job compares a subject with the parent, never the parent");
        let (s6, _) = at(&s2, &p, 1_660, |bld| {
            for part in &parts {
                apply_object(bld, &ctx(1_660), part).expect("a pairwise part");
            }
        });
        let s7 = finalize_all(&s6, &p, 1_700, &[h(0xD0), h(0xD1), h(0xD2)]);
        let pairwise_of = |s: &PalwChainStateV2| {
            s.improvement_result(&h(LINE), 1, 0, &cand_subject)
                .and_then(|r| r.scores.iter().find(|sc| sc.kind == PalwScoringKindV1::Pairwise).map(|sc| sc.value))
        };
        assert_eq!(pairwise_of(&s7), None, "three of four parts: no outcome yet");
        let s8 = finalize_all(&s7, &p, 1_701, &[h(0xD3)]);
        let v: Vec<i64> = parts.iter().map(part_value).collect();
        let sum = (v[0] as i128 - v[1] as i128) - (v[2] as i128 - v[3] as i128);
        assert_eq!(pairwise_of(&s8), Some(if sum > 0 { 1 } else { -1 }), "the sum of the two margins, a tie to the incumbent");
        assert_ne!(pairwise_of(&s8), Some(0), "a pairwise outcome is +1 or −1: there is no tie");
    }

    /// **A judged part that read a generation later reversed records no score** (spec 17 §17.8.6, E-C6): a judge reads
    /// FINAL generations, and a generation convicted after `Final` is no longer what its parts read — whether it is
    /// claimed anew or not, the judged score is missing and the item counts for the incumbent.
    #[test]
    fn a_judged_part_that_read_a_generation_later_reversed_records_no_score() {
        let p = params();
        let s = evaluating_judged();
        let parent_claim = claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xA0);
        let parent_gen = generated_of(&parent_claim);
        let jt = judge_template();
        let (s1, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &parent_claim).unwrap());
        // The generation is licensed and finalized by the lane's own funnel, so it is a real `Final` claim.
        let finalize = |s: &PalwChainStateV2, id: Hash64, daa: u64| {
            let licensed =
                PalwClaimStateV2 { phase: PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: daa - 1 }, ..s.claims[&id].clone() };
            at(s, &p, daa, |b| {
                b.write_claim(id, Some(licensed.clone()));
                b.finalize_claim(id, &licensed, daa).expect("the claim finalizes");
            })
            .0
        };
        let s2 = finalize(&s1, h(0xA0), 1_650);
        let a = judged_part(0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge, 0, &jt, &[&parent_gen], ALICE, 0xB1);
        let b = judged_part(0, PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge, 1, &jt, &[&parent_gen], ALICE, 0xB2);
        let (s3, _) = at(&s2, &p, 1_660, |bld| {
            apply_object(bld, &ctx(1_660), &a).expect("part 0");
            apply_object(bld, &ctx(1_660), &b).expect("part 1");
        });
        let judged = |s: &PalwChainStateV2| {
            s.improvement_result(&h(LINE), 1, 0, &PalwEvalSubjectV1::Parent)
                .is_some_and(|r| r.scores.iter().any(|score| score.kind == PalwScoringKindV1::Judge))
        };
        // Control: parts that read a generation that stands record their score.
        assert!(judged(&finalize_all(&finalize_all(&s3, &p, 1_700, &[h(0xB1)]), &p, 1_701, &[h(0xB2)])), "the generation stands");

        // The generation is convicted after Final: the parts are accepted, and finalize, but record nothing.
        let (s4, _) = at(&s3, &p, 1_670, |bld| {
            bld.reverse_convicted_final(&ctx(1_670), h(0xA0), PalwVoidReasonV2::CourtFraud).expect("reversed")
        });
        assert!(matches!(s4.claims[&h(0xA0)].phase, PalwClaimPhaseV2::Voided { .. }));
        let s5 = finalize_all(&finalize_all(&s4, &p, 1_700, &[h(0xB1)]), &p, 1_701, &[h(0xB2)]);
        assert!(!judged(&s5), "a part that read a reversed generation records no judged score");
        // …and when the generation is claimed anew and finalized, the old parts are still stale: they read the old one.
        let retake = claim(&s4, 0, PalwEvalSubjectV1::Parent, BOB, 0xA9);
        let (s6, _) = at(&s4, &p, 1_680, |bld| apply_object(bld, &ctx(1_680), &retake).expect("the freed job is re-taken"));
        let s7 = finalize(&s6, h(0xA9), 1_690);
        let s8 = finalize_all(&finalize_all(&s7, &p, 1_700, &[h(0xB1)]), &p, 1_701, &[h(0xB2)]);
        assert!(!judged(&s8), "the parts were accepted before the new generation was final: stale");
    }

    // ---- panels: seat pay, the class's replay room, and the swing lock ------------------------------------------

    /// **The seats are paid from the escrow, at the ordinary per-claim rule** (spec 17 §17.11.2, decided
    /// 2026-09-30): the policy's `seat_pool_permille` of the job's fee is the seats' pool, divided over the seats
    /// DRAWN, each credited seat taking one share; the executor gets the rest. What no seat was credited for, and
    /// the division's dust, is not spent — it stays in the escrow and returns with it — and the executor never takes
    /// it.
    #[test]
    fn the_credited_seats_are_paid_from_the_escrow_and_what_no_seat_earned_stays_in_it() {
        let p = params();
        let mut pol = policy();
        pol.eval.seat_pool_permille = 300;
        let fee = pol.fees.eval_fee_per_job;
        let s = evaluating_with(&p, pol);
        let honest = claim(&s, 0, PalwEvalSubjectV1::Candidate(h(CAND_A)), CAROL, 0xA7);
        let (s1, _) = at(&s, &p, 1_600, |b| {
            apply_object(b, &ctx(1_600), &honest).unwrap();
            // The claim's panel: three seats drawn; two of them answered.
            let seats = [(bond(ALICE), 1_610u64), (bond(BOB), 1_611), (bond(DAVE), 0)].into_iter().collect();
            b.write_panel_duties(h(0xA7), Some(PalwPanelDutyRowV1 { seats, seat_exposure: 0 }));
        });
        let before = |who: u8| s1.improvement_earnings(&bond(who));
        let escrow_before = s1.improvement_candidate(&h(LINE), 1, &h(CAND_A)).unwrap().1.escrow_spent;
        let held_before = s1.improvement_pools.get(&h(LINE)).unwrap().held;
        let s2 = finalize_all(&s1, &p, 1_700, &[h(0xA7)]);
        let pool = fee * 300 / 1000;
        let per_seat = pool / 3;
        assert!(per_seat > 0, "the premise: a fee a pool can divide");
        let gained = |who: u8| s2.improvement_earnings(&bond(who)) - before(who);
        assert_eq!(gained(CAROL), fee - pool, "the executor: the fee less the seats' pool");
        assert_eq!(gained(ALICE), per_seat, "a credited seat: one share of the pool, over the DRAWN seats");
        assert_eq!(gained(BOB), per_seat);
        assert_eq!(gained(DAVE), 0, "a seat that never answered earns nothing");
        let spent = fee - pool + 2 * per_seat;
        assert!(spent < fee, "the uncredited seat's share and the dust are not spent");
        assert_eq!(
            s2.improvement_candidate(&h(LINE), 1, &h(CAND_A)).unwrap().1.escrow_spent,
            escrow_before + spent,
            "the escrow paid exactly what was earned"
        );
        assert_eq!(s2.improvement_pools.get(&h(LINE)).unwrap().held, held_before - spent, "the pool's held escrow follows");
        // A claim with no panel row (a claim bound below the panel economy) pays its executor the whole fee.
        let solo = claim(&s, 1, PalwEvalSubjectV1::Candidate(h(CAND_A)), CAROL, 0xA8);
        let (t1, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &solo).unwrap());
        let earned = t1.improvement_earnings(&bond(CAROL));
        let t2 = finalize_all(&t1, &p, 1_700, &[h(0xA8)]);
        assert_eq!(t2.improvement_earnings(&bond(CAROL)) - earned, fee, "no panel, no pool: the whole fee");
    }

    /// **An evaluation claim is counted in its class's replay room** (spec 17 §17.8.4, decided 2026-09-30): a lane
    /// of the in-flight tally of its own, one whole claim while its replay is owed — as an attempt is — and not once
    /// it is licensed; the claim a gate is asked about counts as one more; and the claims evaluation may hold are
    /// bounded to the fence's share of the class's capacity, at least one always.
    #[test]
    fn evaluation_claims_are_counted_in_their_classs_replay_room() {
        let p = params();
        let s = evaluating();
        let a = claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xA0);
        let b2 = claim(&s, 1, PalwEvalSubjectV1::Parent, BOB, 0xA1);
        let (s1, _) = at(&s, &p, 1_600, |b| {
            apply_object(b, &ctx(1_600), &a).unwrap();
            apply_object(b, &ctx(1_600), &b2).unwrap();
        });
        let index = super::super::palw_inflight_index_build_v1(&s1);
        let tally = index.get(&h(LINE)).expect("the class has claims in flight");
        assert_eq!(
            (tally.eval_claims, tally.free_prompts, tally.free_prompt_quanta, tally.attempts),
            (2, 0, 0, 0),
            "a lane of their own"
        );
        assert_eq!(tally.counted(true, 8), 2, "each is a whole claim in the registry's count, whatever the quanta per job");
        assert_eq!(tally.counted(false, 0), 2);
        let owed = |index: &BTreeMap<Hash64, super::super::PalwInflightTallyV1>, extra| {
            super::super::palw_panel_owed_v1(&s1, &p, index, None, extra).get(&h(LINE)).copied().unwrap_or(0)
        };
        assert_eq!(owed(&index, None), 2, "both owe the panel a replay");
        assert_eq!(owed(&index, Some((h(LINE), super::super::PalwGatedClaimV1::Evaluation))), 3, "the claim asked about is one more");
        assert_eq!(super::super::PalwGatedClaimV1::Evaluation.whole_claims(8), 1, "alone, one whole claim");
        // One licensed: its replay is done, and it stops owing — the lane moves with the claim, as the others do.
        let mut licensed = s1.clone();
        licensed.claims.get_mut(&h(0xA0)).unwrap().phase = PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1_620 };
        let index = super::super::palw_inflight_index_build_v1(&licensed);
        assert_eq!((index[&h(LINE)].eval_claims, index[&h(LINE)].licensed_eval_claims), (2, 1));
        assert_eq!(super::super::palw_panel_owed_v1(&licensed, &p, &index, None, None)[&h(LINE)], 1);
        // Final: it leaves the tally. The incremental index is the walk's (the fold asserts it; here by hand).
        let mut done = s1.clone();
        done.claims.get_mut(&h(0xA0)).unwrap().phase = PalwClaimPhaseV2::Final { final_daa: 1_700 };
        assert_eq!(super::super::palw_inflight_index_build_v1(&done)[&h(LINE)].eval_claims, 1);

        // The share: at least one, then the fence's permille of the class's capacity.
        let exceeded = super::super::palw_eval_room_share_exceeded_v1;
        assert!(!exceeded(0, 0, 0), "a class with no room still takes one evaluation claim");
        assert!(exceeded(1, 0, 0), "and not a second");
        assert!(!exceeded(0, 100, 100), "a share of ten");
        assert!(!exceeded(9, 100, 100), "the tenth fits");
        assert!(exceeded(10, 100, 100), "the eleventh does not");
        assert!(!exceeded(999, 1_000, 1_000), "the whole room at 1,000 permille");
        assert!(exceeded(1_000, 1_000, 1_000));
    }

    /// **The swing lock** (spec 17 §17.8.4, decided 2026-09-30): an evaluation claim can swing a promotion's
    /// payout, so its executor locks the epoch's largest payout over the fewest forged pairs that flip the sign test
    /// — never less than the work's own lock — and a claim whose lock the executor's room cannot carry is refused.
    #[test]
    fn an_evaluation_claim_locks_what_it_could_swing() {
        let p = params();
        let pol = policy();
        let s = evaluating();
        let balance = s.improvement_pools.get(&h(LINE)).unwrap().balance;
        let payout = palw_improve_max_promotion_payout_v1(balance, pol.promotion_share_permille);
        assert_eq!(payout, (balance as u128 * 250 / 1000) as u64, "S2's R: the promotion share of the pool");
        assert!(payout > 0, "the premise: a funded pool");
        let honest = claim(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xA0);
        let (s1, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &honest).unwrap());
        let work = palw_improve_eval_reservation_v1(s1.claims[&h(0xA0)].work_leaves, 3, 1);
        assert_eq!(s1.claims[&h(0xA0)].reserved, work.max(palw_improve_eval_swing_lock_v1(payout)), "the dearer of the two");
        assert_eq!(palw_improve_eval_swing_lock_v1(payout), payout as u128, "at one forged pair, the whole payout");
        assert_eq!(s1.reserved_exposure(&bond(CAROL)), s1.claims[&h(0xA0)].reserved, "and it is the executor's exposure");
        // A pool with nothing to win has nothing to swing: the work's own lock.
        let mut empty = s.clone();
        empty.improvement_pools.get_mut(&h(LINE)).unwrap().balance = 0;
        let (s2, _) = at(&empty, &p, 1_600, |b| apply_object(b, &ctx(1_600), &honest).unwrap());
        assert_eq!(s2.claims[&h(0xA0)].reserved, work, "no payout, no swing");
        // An executor whose room cannot carry the swing is refused by the exposure ceiling: a pool so rich that the
        // payout outruns the bond (the test bonds hold 10^15 sompi).
        let mut rich = s.clone();
        rich.improvement_pools.get_mut(&h(LINE)).unwrap().balance = 8_000_000_000_000_000;
        let outcome = fold_one(&rich, &p, 1_600, &honest);
        assert!(
            matches!(outcome, Err(PalwStateV2Error::FreePromptExposureCeiling { .. })),
            "a lock the bond's room cannot carry: {outcome:?}"
        );
        // And a poor pool, whatever the bond, locks only the work.
        // The share-of-capacity bound reads the work's own lock, not the swing: a rich pool does not make an executor
        // that fits its share unable to claim for that reason alone.
        assert_eq!(palw_improve_eval_swing_lock_v1(0), 0);
        assert_eq!(palw_improve_max_promotion_payout_v1(u64::MAX, 1_000), u64::MAX);
        assert_eq!(palw_improve_max_promotion_payout_v1(1_000, 1), 1, "the promotion share is in permille");
    }

    // ---- the evaluation court (spec 17 §17.8.6): planted lies convicted, honest claims untouched ----------------

    const LIMITS: misaka_palw_tir::demand::DemandLimits =
        misaka_palw_tir::demand::DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };

    /// The params the court tests fold accusations under: the IR fence in force (a one-move accusation is an IR object).
    fn court_params() -> PalwStateParamsV2 {
        params().with_tir_from_daa(Some(0))
    }

    /// The ruleset's court the acceptance layer judges at (its default ceilings).
    fn court() -> crate::palw_mode_v2::PalwCourtParamsV2 {
        crate::palw_mode_v2::PalwCourtParamsV2::new(1 << 26, 20, 2).expect("a court")
    }

    /// A class's weights as the court proves them: the inventory root over [`weights`] of its `salt`.
    fn real_artifact_root(salt: usize) -> Hash64 {
        struct Src<'a>(&'a MapParams);
        impl crate::palw_tir_artifact_v1::PalwTirTensorSourceV1 for Src<'_> {
            fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<std::borrow::Cow<'_, [u8]>> {
                self.0.tensors.get(&(param, layer)).map(|t| std::borrow::Cow::Owned(t.to_le_bytes()))
            }
        }
        let p = program();
        let w = weights(&p, salt);
        crate::palw_tir_artifact_v1::palw_tir_inventory_root_v1(&p, &Src(&w)).expect("an inventory root").0
    }

    /// The state with every class registered under its weights' real artifact root, which a court proves openings to.
    fn with_real_roots(mut s: PalwChainStateV2) -> PalwChainStateV2 {
        for (class, salt) in [(LINE, 0), (CAND_A, 1), (JUDGE, 2)] {
            s.classes.get_mut(&h(class)).expect("a class").artifact_root = real_artifact_root(salt);
        }
        s
    }

    /// The executor's evidence for `run`, as the state holds its claim.
    fn evidence_of<'a>(
        s: &'a PalwChainStateV2,
        run: &'a EvalRun,
        claim: &'a PalwClaimStateV2,
    ) -> crate::palw_improve_eval_court_v1::PalwEvalEvidenceV1<'a> {
        crate::palw_improve_eval_court_v1::PalwEvalEvidenceV1 {
            facts: s.improvement_eval_claim_facts_v1(claim, &run.binding).expect("the claim the job table holds"),
            params: &run.weights,
            execution: &run.execution,
            binding: &run.binding,
            prompt: &run.prompt,
            composite: None,
        }
    }

    /// The accusation the node files for `proof`: the verdict as the acceptance layer re-derives it
    /// (`palw_tir_one_move_outcome_v1`), signed by position, its own shape checked.
    fn accusation_of(
        s: &PalwChainStateV2,
        claim_id: Hash64,
        accuser: u8,
        proof: crate::palw_court_v2::PalwCourtVerdictProofV2,
    ) -> (PalwConsensusObjectV2, PalwCourtVerdictV2) {
        use crate::palw_tir_one_move_v1::*;
        let claim = s.claims.get(&claim_id).expect("the accused claim").clone();
        let mut a = palw_tir_one_move_accusation_v1(claim_id, &claim, bond(accuser), PalwCourtVerdictV2::ExecutorGuilty, proof);
        a.signature = vec![9; 8];
        palw_tir_one_move_shape_v1(&a).expect("the accusation's own shape");
        let outcome = palw_tir_one_move_outcome_v1(
            s,
            &claim,
            &a,
            &court(),
            1 << 26,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            false,
        )
        .expect("the close adjudicates");
        let PalwTirOneMoveOutcomeV1::Verdict(verdict) = outcome else { panic!("the toy program has no dissected leaf") };
        a.verdict = verdict;
        (PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(a) }, verdict)
    }

    fn cone_proof(
        evidence: &crate::palw_improve_eval_court_v1::PalwEvalEvidenceV1<'_>,
        leaf: u64,
    ) -> crate::palw_court_v2::PalwCourtVerdictProofV2 {
        crate::palw_court_v2::PalwCourtVerdictProofV2::EvalCone {
            close: Box::new(evidence.cone_close(leaf, &LIMITS).expect("a cone close")),
        }
    }

    /// The claim's phase, as a test reads it.
    fn voided_by_the_court(s: &PalwChainStateV2, id: &Hash64) -> bool {
        matches!(s.claims.get(id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }))
    }

    /// **A wrong value in the tree is convicted at its leaf, and the claim is voided and slashed** (spec 17 §17.8.6):
    /// the fold cannot see inside an execution, so it accepts a claim whose tree holds a lie; one accusation —
    /// carrying the evaluation cone close of the lie's leaf — convicts it by the claim rules (`CourtFraud`, the
    /// swing lock slashed, the accuser the challenger), its job is free again and no score of it was recorded; an
    /// honest claim of another job is untouched, and a false accusation of an honest claim is charged to its accuser.
    #[test]
    fn a_planted_lie_in_the_tree_is_convicted_voided_and_slashed_and_an_honest_claim_survives() {
        let p = court_params();
        let s = with_real_roots(evaluating());
        let honest_probe = claim_run(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xB0, vec![3, 5, 0], None, Lie::None);
        let last = honest_probe.execution.space.stages[0].leaves().len() - 1;
        let lying = claim_run(
            &s,
            0,
            PalwEvalSubjectV1::Parent,
            CAROL,
            0xB0,
            vec![3, 5, 0],
            None,
            Lie::Leaf { stage: 0, index: last, delta: 1 },
        );
        assert_ne!(lying.binding.committed_execution_root, honest_probe.binding.committed_execution_root, "the lie is committed");
        let bystander = claim_run(&s, 1, PalwEvalSubjectV1::Parent, ALICE, 0xB2, vec![3, 5, 1], None, Lie::None);
        let (s1, _) = at(&s, &p, 1_600, |b| {
            apply_object(b, &ctx(1_600), &lying.object).expect("a lie the fold cannot see is accepted");
            apply_object(b, &ctx(1_600), &bystander.object).expect("an honest claim");
        });
        let (id, other) = (h(0xB0), h(0xB2));
        let before = s1.claims[&id].clone();
        assert!(before.reserved > 0 && matches!(before.phase, PalwClaimPhaseV2::Provisional));

        // The close of the lie's leaf convicts; the close of an earlier leaf of the same claim acquits.
        let accused_claim = s1.claims[&id].clone();
        let evidence = evidence_of(&s1, &lying, &accused_claim);
        let (guilty, verdict) = accusation_of(&s1, id, BOB, cone_proof(&evidence, last as u64));
        assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "the lie's own leaf");
        let (defeated, verdict) = accusation_of(&s1, id, BOB, cone_proof(&evidence, 0));
        assert_eq!(verdict, PalwCourtVerdictV2::ChallengerDefeated, "an honest leaf of a lying claim acquits: the lie is the leaf's");

        // The honest claim: a false accusation is charged to its accuser and leaves the claim standing.
        let honest_evidence = evidence_of(&s1, &bystander, &s1.claims[&other]);
        let (false_accusation, verdict) = accusation_of(&s1, other, BOB, cone_proof(&honest_evidence, last as u64));
        assert_eq!(verdict, PalwCourtVerdictV2::ChallengerDefeated, "an honest claim's leaf acquits");
        let bob_before = s1.bond(&bond(BOB)).unwrap().collateral;
        let (s2, _) = at(&s1, &p, 1_650, |b| apply_object(b, &ctx(1_650), &false_accusation).expect("a false accusation folds"));
        assert!(s2.bond(&bond(BOB)).unwrap().collateral < bob_before, "and is charged");
        assert_eq!(s2.claims[&other], s1.claims[&other], "the honest claim is untouched");
        assert!(!s2.claims[&other].phase.is_terminal());
        let (s2b, _) = at(&s1, &p, 1_650, |b| apply_object(b, &ctx(1_650), &defeated).expect("a false accusation folds"));
        assert!(!s2b.claims[&id].phase.is_terminal(), "a defeated accusation convicts nothing");

        // The conviction.
        let carol_before = s1.bond(&bond(CAROL)).unwrap().collateral;
        let (s3, _) = at(&s1, &p, 1_650, |b| apply_object(b, &ctx(1_650), &guilty).expect("the accusation folds"));
        assert!(voided_by_the_court(&s3, &id), "{:?}", s3.claims[&id].phase);
        let slashed = u128::from(carol_before - s3.bond(&bond(CAROL)).unwrap().collateral);
        assert!(slashed >= before.reserved, "the lock is slashed: {slashed} of {}", before.reserved);
        assert_eq!(s3.claims[&other], s1.claims[&other], "an honest claim of another job is untouched");
        let job = s3.improvement_eval_job(&h(LINE), 1, 0, &PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch, 0).unwrap();
        assert!(!s3.improvement_eval_claim_held_v1(&job.claim.unwrap().claim_id), "the job's holder is dead");
        assert!(!s3.improvement_eval_row_pending_v1(job), "a voided claim is no live claim: it keeps nothing pending");
        let bystander_job =
            s3.improvement_eval_job(&h(LINE), 1, 1, &PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch, 0).unwrap();
        assert!(s3.improvement_eval_row_pending_v1(bystander_job), "an honest claim of another job is live");
        assert!(s3.improvement_result(&h(LINE), 1, 0, &PalwEvalSubjectV1::Parent).is_none(), "no score was recorded");
        // The job is free: another executor takes it.
        let retake = claim_with(&s3, 0, PalwEvalSubjectV1::Parent, BOB, 0xB3, vec![3, 5, 0], None);
        assert_eq!(fold_one(&s3, &p, 1_700, &retake), Ok(()), "the convicted claim's job is re-taken");
        // A second conviction of the same claim is no second charge.
        assert!(matches!(fold_one(&s3, &p, 1_700, &guilty), Err(PalwStateV2Error::WrongPhase { .. })), "a claim already voided");
    }

    /// **D-M3: a lying evaluation claim loses the whole bond, and every job it held is free again** (ADR-0160 AG-2
    /// over spec 17 §17.8.6). With the aggregate-liability fence armed, one conviction — an `EvalCone` close of a
    /// planted leaf, or an `EvalDecodeToken` close of a wrong id (the two proofs the drill's liar can be caught by) —
    /// is an intent-class conviction (`CourtFraud`): the producer's whole posted collateral is forfeited, the bond is
    /// frozen for good, the convicted claim is voided `CourtFraud`, and the liar's OTHER live evaluation claim is
    /// voided `AggregateForfeit`. Both jobs are claimable again and no score of either was recorded; an honest
    /// bystander's claim and its job are untouched; and an honest executor re-takes both jobs.
    #[test]
    fn d_m3_a_lying_claim_forfeits_the_whole_bond_voids_its_other_claims_and_frees_every_job() {
        use crate::palw_aggregate_liability_v1::{PalwCapacityLiabilityV1, PalwCapacityStepV1};
        let p = court_params().with_capacity_liability(Some(PalwCapacityLiabilityV1 {
            activation: crate::config::params::ForkActivation::new(0),
            steps: vec![PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 0 }],
        }));
        let s = with_real_roots(evaluating());
        let honest_probe = claim_run(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xC0, vec![3, 5, 0], None, Lie::None);
        let last = honest_probe.execution.space.stages[0].leaves().len() - 1;
        for by_decode in [false, true] {
            // The liar's two claims: item 0 carries the lie, item 1 is an honest tree of the same bond.
            let lie = if by_decode { Lie::Id { t: 1 } } else { Lie::Leaf { stage: 0, index: last, delta: 1 } };
            let lying = claim_run(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xC1, vec![3, 5, 0], None, lie);
            let other = claim_run(&s, 1, PalwEvalSubjectV1::Parent, CAROL, 0xC2, vec![3, 5, 1], None, Lie::None);
            let bystander = claim_run(&s, 2, PalwEvalSubjectV1::Parent, ALICE, 0xC3, vec![3, 5, 2], None, Lie::None);
            let (s1, _) = at(&s, &p, 1_600, |b| {
                for run in [&lying, &other, &bystander] {
                    apply_object(b, &ctx(1_600), &run.object).expect("a claim the fold cannot see into is accepted");
                }
            });
            let (liar, other_id, bystander_id) = (h(0xC1), h(0xC2), h(0xC3));
            assert!(s1.bond(&bond(CAROL)).unwrap().collateral > 0 && s1.bond_freeze_of_v1(&bond(CAROL)).is_none());

            // The accusation: the proof the drill's accuser files, adjudicated as the acceptance layer re-derives it.
            let accused = s1.claims[&liar].clone();
            let evidence = evidence_of(&s1, &lying, &accused);
            let proof = if by_decode {
                crate::palw_court_v2::PalwCourtVerdictProofV2::EvalDecodeToken { close: Box::new(evidence.decode_close(1).unwrap()) }
            } else {
                cone_proof(&evidence, last as u64)
            };
            let (guilty, verdict) = accusation_of(&s1, liar, BOB, proof);
            assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty);
            let (s2, _) = at(&s1, &p, 1_650, |b| apply_object(b, &ctx(1_650), &guilty).expect("the accusation folds"));

            // AG-2: the convicted claim is the court's, the sibling the bond's, and the whole bond is gone.
            assert!(voided_by_the_court(&s2, &liar), "{:?}", s2.claims[&liar].phase);
            assert!(
                matches!(
                    s2.claims[&other_id].phase,
                    PalwClaimPhaseV2::Voided { reason: crate::palw_state_v2::PalwVoidReasonV2::AggregateForfeit, .. }
                ),
                "the liar's other evaluation claim: {:?}",
                s2.claims[&other_id].phase
            );
            assert_eq!(s2.bond(&bond(CAROL)).unwrap().collateral, 0, "AG-2 takes the whole posted collateral");
            assert!(s2.bond_freeze_of_v1(&bond(CAROL)).is_some_and(|f| f.final_), "and freezes the bond for good");
            // The honest bystander is untouched.
            assert_eq!(s2.claims[&bystander_id], s1.claims[&bystander_id]);
            // Both jobs are free: no holder, nothing pending, no score; the bystander's job is still held.
            for item in [0u32, 1] {
                let job =
                    s2.improvement_eval_job(&h(LINE), 1, item, &PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch, 0).unwrap();
                assert!(!s2.improvement_eval_claim_held_v1(&job.claim.unwrap().claim_id), "item {item}: the holder is dead");
                assert!(!s2.improvement_eval_row_pending_v1(job), "item {item}: nothing pending");
                assert!(s2.improvement_result(&h(LINE), 1, item, &PalwEvalSubjectV1::Parent).is_none(), "item {item}: no score");
            }
            let held = s2.improvement_eval_job(&h(LINE), 1, 2, &PalwEvalSubjectV1::Parent, PalwScoringKindV1::ExactMatch, 0).unwrap();
            assert!(s2.improvement_eval_row_pending_v1(held), "the bystander's job is live");
            // The freed jobs are re-taken by an honest executor of another bond.
            for (item, word, prompt) in [(0u32, 0xC4u8, vec![3, 5, 0]), (1, 0xC5, vec![3, 5, 1])] {
                let retake = claim_with(&s2, item, PalwEvalSubjectV1::Parent, BOB, word, prompt, None);
                assert_eq!(fold_one(&s2, &p, 1_700, &retake), Ok(()), "item {item} is re-taken");
            }
            // The liar cannot take them back: its bond is frozen.
            let again = claim_with(&s2, 0, PalwEvalSubjectV1::Parent, CAROL, 0xC6, vec![3, 5, 0], None);
            assert!(fold_one(&s2, &p, 1_700, &again).is_err(), "a frozen bond claims nothing");
        }
    }

    /// **A wrong generated id over an honest tree is a wrong decode** (`EvalDecodeToken`, `Token`): the committed id is
    /// not the lane FP Job V4's rules select from the committed logits row it was read from.
    #[test]
    fn a_wrong_generated_id_is_a_wrong_decode_and_is_convicted() {
        let p = court_params();
        let s = with_real_roots(evaluating());
        let honest = claim_run(&s, 2, PalwEvalSubjectV1::Parent, CAROL, 0xB4, vec![3, 5, 2], None, Lie::None);
        let lying = claim_run(&s, 2, PalwEvalSubjectV1::Parent, CAROL, 0xB5, vec![3, 5, 2], None, Lie::Id { t: 1 });
        let (s1, _) = at(&s, &p, 1_600, |b| {
            apply_object(b, &ctx(1_600), &lying.object).expect("a wrong id the fold cannot see is accepted");
        });
        let claim = s1.claims[&h(0xB5)].clone();
        let evidence = evidence_of(&s1, &lying, &claim);
        let decode = |t: u32| crate::palw_court_v2::PalwCourtVerdictProofV2::EvalDecodeToken {
            close: Box::new(evidence.decode_close(t).unwrap()),
        };
        let (guilty, verdict) = accusation_of(&s1, h(0xB5), BOB, decode(1));
        assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "the id at position 1 is not the selection from its row");
        let (_, verdict) = accusation_of(&s1, h(0xB5), BOB, decode(0));
        assert_eq!(verdict, PalwCourtVerdictV2::ChallengerDefeated, "an honest id acquits");
        let (s2, _) = at(&s1, &p, 1_650, |b| apply_object(b, &ctx(1_650), &guilty).expect("the accusation folds"));
        assert!(voided_by_the_court(&s2, &h(0xB5)));
        // The honest twin, were it filed, would have been acquitted at every id.
        assert_eq!(honest.binding.generated.len(), lying.binding.generated.len());
        let (s3, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &honest.object).unwrap());
        let honest_claim = s3.claims[&h(0xB4)].clone();
        let honest_evidence = evidence_of(&s3, &honest, &honest_claim);
        for t in 0..honest.binding.generated.len() as u32 {
            let proof = crate::palw_court_v2::PalwCourtVerdictProofV2::EvalDecodeToken {
                close: Box::new(honest_evidence.decode_close(t).unwrap()),
            };
            assert_eq!(accusation_of(&s3, h(0xB4), BOB, proof).1, PalwCourtVerdictV2::ChallengerDefeated, "id {t}");
        }
    }

    /// **A wrong score at an item is convicted, and never reaches the sign test** (`EvalDecodeToken`, `Score`): a
    /// likelihood claim commits a score its own tree does not produce; its leaves are all honest, so the cone close
    /// acquits every one of them and only the score close convicts. The claim is voided before `Final`, so no score of
    /// it is recorded, and the honest re-claim records the right one.
    #[test]
    fn a_wrong_score_at_an_item_is_convicted_and_its_score_never_reaches_the_sign_test() {
        let p = court_params();
        let (s, entries, leaves) = evaluating_with_suite();
        let s = with_real_roots(s);
        let drawn: Vec<(u32, u32)> = (8..12u32)
            .map(|item| match s.improvement_item(&h(LINE), 1, item).unwrap().source {
                PalwItemSourceV1::Regression { index } => (item, index),
                other => panic!("a regression item, got {other:?}"),
            })
            .collect();
        let (likely_item, likely_index) =
            *drawn.iter().find(|(_, index)| index % 2 == 1).expect("a likelihood entry among four of six");
        let likely = &entries[likely_index as usize];
        let PalwSuiteReferenceV1::Continuation(continuation) = &likely.reference else { panic!("a likelihood entry") };
        let run_of = |executor: u8, word: u8, lie: Lie| {
            forced_run(
                PalwEvalJobV1 {
                    line_id: h(LINE),
                    epoch: 1,
                    item: likely_item,
                    subject: PalwEvalSubjectV1::Parent,
                    kind: PalwScoringKindV1::RefLogLik,
                    part: 0,
                    mode: PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::default() },
                },
                LINE,
                executor,
                word,
                likely.prompt.clone(),
                continuation.clone(),
                PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 },
                vec![],
                None,
                Some(opening_of(&entries, &leaves, likely_index)),
                lie,
            )
        };
        let honest = run_of(CAROL, 0xB6, Lie::None);
        let lying = run_of(CAROL, 0xB7, Lie::Score { lane: 1, delta: 1 });
        assert_ne!(honest.binding.score, lying.binding.score);
        let (s1, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &lying.object).expect("a wrong score the fold cannot see"));
        let id = h(0xB7);
        let claim = s1.claims[&id].clone();
        let evidence = evidence_of(&s1, &lying, &claim);
        // The tree is honest: every leaf acquits.
        let n: usize = lying.execution.space.stages.iter().map(|st| st.leaves().len()).sum();
        for leaf in 0..n as u64 {
            assert_eq!(
                accusation_of(&s1, id, BOB, cone_proof(&evidence, leaf)).1,
                PalwCourtVerdictV2::ChallengerDefeated,
                "leaf {leaf}"
            );
        }
        // The committed score is not the score stage's output: convicted at the lane.
        let proof =
            crate::palw_court_v2::PalwCourtVerdictProofV2::EvalDecodeToken { close: Box::new(evidence.score_close().unwrap()) };
        let (guilty, verdict) = accusation_of(&s1, id, BOB, proof);
        assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty);
        let (s2, _) = at(&s1, &p, 1_650, |b| apply_object(b, &ctx(1_650), &guilty).expect("the accusation folds"));
        assert!(voided_by_the_court(&s2, &id));
        assert!(
            s2.improvement_result(&h(LINE), 1, likely_item, &PalwEvalSubjectV1::Parent).is_none_or(|r| r.scores.is_empty()),
            "a convicted claim records no score"
        );
        // The honest re-claim takes the freed job and records the right score at Final.
        let retake = run_of(BOB, 0xB8, Lie::None);
        let (s3, _) = at(&s2, &p, 1_700, |b| apply_object(b, &ctx(1_700), &retake.object).expect("the freed job is re-taken"));
        let retaken = s3.claims[&h(0xB8)].clone();
        let licensed = PalwClaimStateV2 { phase: PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1_710 }, ..retaken };
        let (s4, _) = at(&s3, &p, 1_720, |b| {
            b.write_claim(h(0xB8), Some(licensed.clone()));
            b.finalize_claim(h(0xB8), &licensed, 1_720).expect("the honest claim finalizes");
        });
        let recorded = s4
            .improvement_result(&h(LINE), 1, likely_item, &PalwEvalSubjectV1::Parent)
            .and_then(|r| r.scores.iter().find(|score| score.kind == PalwScoringKindV1::RefLogLik).copied())
            .expect("the honest score");
        assert_eq!(
            palw_improve_eval_score_value_v1(PalwScoringKindV1::RefLogLik, &honest.binding.score).unwrap(),
            recorded.value,
            "the recorded score is the honest claim's, never the convicted one's"
        );
    }

    /// **Gating** (spec 17 §17.8.6): below `palw_improvement_v1` an evaluation proof is refused by name; an
    /// evaluation proof accuses an evaluation claim of the job table and no other; a close of another claim is not
    /// this claim's; a claim already `Final` is beyond the court.
    #[test]
    fn an_evaluation_accusation_is_refused_below_the_fence_on_another_kind_of_claim_and_on_another_claim() {
        let p = court_params();
        let s = with_real_roots(evaluating());
        let a =
            claim_run(&s, 0, PalwEvalSubjectV1::Parent, CAROL, 0xC0, vec![3, 5, 0], None, Lie::Leaf { stage: 0, index: 3, delta: 1 });
        let b = claim_run(&s, 1, PalwEvalSubjectV1::Parent, ALICE, 0xC1, vec![3, 5, 1], None, Lie::None);
        let (s1, _) = at(&s, &p, 1_600, |bld| {
            apply_object(bld, &ctx(1_600), &a.object).unwrap();
            apply_object(bld, &ctx(1_600), &b.object).unwrap();
        });
        let (id_a, id_b) = (h(0xC0), h(0xC1));
        let claim_a = s1.claims[&id_a].clone();
        let (guilty, verdict) = accusation_of(&s1, id_a, BOB, cone_proof(&evidence_of(&s1, &a, &claim_a), 3));
        assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty);

        // Below the fence: refused by name, whatever it proves.
        let below = court_params().with_improve_from_daa(None);
        assert!(matches!(
            fold_one(&s1, &below, 1_650, &guilty),
            Err(PalwStateV2Error::ImprovementObjectRefused { object: "an evaluation court move", .. })
        ));
        assert!(palw_object_is_eval_v1(&guilty), "the walk drops it by name");

        // Another claim's close (the acceptance layer's checks): an accusation that names claim B with its roots and
        // carries A's close is refused by the proof's own roots; and A's close against B, the roots left as A's, is
        // not the claim the job table holds for its binding's job.
        let PalwConsensusObjectV2::TirShardCourtAccused { accusation } = &guilty else { unreachable!() };
        let mut on_b = accusation.as_ref().clone();
        on_b.claim = id_b;
        on_b.executor_bond = s1.claims[&id_b].bond;
        assert!(
            matches!(
                crate::palw_tir_one_move_v1::palw_tir_one_move_outcome_v1(
                    &s1,
                    &s1.claims[&id_b],
                    &on_b,
                    &court(),
                    1 << 26,
                    crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                    false
                ),
                Err(crate::palw_court_v2::PalwCourtV2Error::DoesNotAdjudicate(_))
            ),
            "A's close is not claim B's"
        );
        on_b.execution_root = s1.claims[&id_b].execution_root;
        on_b.trace_root = s1.claims[&id_b].trace_root;
        assert!(
            crate::palw_tir_one_move_v1::palw_tir_one_move_shape_v1(&on_b).is_err(),
            "a close's binding speaks about its own claim's roots, never the accusation's other ones"
        );

        // A claim that is not an evaluation claim (an attempt claim of the same class): refused by name.
        let mut plain = s1.clone();
        plain.claims.get_mut(&id_a).unwrap().source = PalwClaimSourceV2::Attempt;
        assert_eq!(
            fold_one(&plain, &p, 1_650, &guilty),
            Err(PalwStateV2Error::ImprovementObjectRefused {
                object: "an evaluation court move",
                why: "it accuses a claim that is not an evaluation claim"
            })
        );
        assert!(matches!(
            crate::palw_tir_one_move_v1::palw_tir_one_move_outcome_v1(
                &plain,
                &plain.claims[&id_a],
                accusation,
                &court(),
                1 << 26,
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                false
            ),
            Err(crate::palw_court_v2::PalwCourtV2Error::DoesNotAdjudicate(_))
        ));

        // A claim already Final is beyond the court.
        let mut done = s1.clone();
        done.claims.get_mut(&id_a).unwrap().phase = PalwClaimPhaseV2::Final { final_daa: 1_640 };
        assert!(matches!(fold_one(&done, &p, 1_650, &guilty), Err(PalwStateV2Error::WrongPhase { .. })));
    }

    /// **A convicted `Final` evaluation claim leaves the epoch** (spec 17 §17.8.6): the claim's score is taken back out
    /// while the epoch still takes scores, the job is free again, and an epoch already decided is not reopened.
    #[test]
    fn a_convicted_final_claim_takes_its_score_back_while_the_epoch_still_takes_scores() {
        let p = params();
        let (s, entries, leaves) = evaluating_with_suite();
        let likely_item = (8..12u32)
            .find(|item| match s.improvement_item(&h(LINE), 1, *item).unwrap().source {
                PalwItemSourceV1::Regression { index } => index % 2 == 1,
                _ => false,
            })
            .expect("a likelihood suite item");
        let PalwItemSourceV1::Regression { index } = s.improvement_item(&h(LINE), 1, likely_item).unwrap().source else {
            unreachable!()
        };
        let likely = &entries[index as usize];
        let PalwSuiteReferenceV1::Continuation(continuation) = &likely.reference else { panic!("a likelihood entry") };
        let run = forced_run(
            PalwEvalJobV1 {
                line_id: h(LINE),
                epoch: 1,
                item: likely_item,
                subject: PalwEvalSubjectV1::Parent,
                kind: PalwScoringKindV1::RefLogLik,
                part: 0,
                mode: PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::default() },
            },
            LINE,
            CAROL,
            0xD1,
            likely.prompt.clone(),
            continuation.clone(),
            PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 },
            vec![],
            None,
            Some(opening_of(&entries, &leaves, index)),
            Lie::None,
        );
        let id = h(0xD1);
        let (s1, _) = at(&s, &p, 1_600, |b| apply_object(b, &ctx(1_600), &run.object).expect("an honest claim"));
        let licensed = PalwClaimStateV2 { phase: PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1_620 }, ..s1.claims[&id].clone() };
        let (s2, _) = at(&s1, &p, 1_700, |b| {
            b.write_claim(id, Some(licensed.clone()));
            b.finalize_claim(id, &licensed, 1_700).expect("the claim finalizes");
        });
        let scored = |s: &PalwChainStateV2| {
            s.improvement_result(&h(LINE), 1, likely_item, &PalwEvalSubjectV1::Parent).is_some_and(|r| !r.scores.is_empty())
        };
        assert!(scored(&s2), "recorded at Final");
        // Convicted after Final, in the epoch still taking scores: the score and the holder go.
        let (s3, _) =
            at(&s2, &p, 1_750, |b| b.reverse_convicted_final(&ctx(1_750), id, PalwVoidReasonV2::CourtFraud).expect("reversed"));
        assert!(matches!(s3.claims[&id].phase, PalwClaimPhaseV2::Voided { .. }));
        assert!(!scored(&s3), "the convicted claim's score is taken back out of the epoch");
        let row =
            s3.improvement_eval_job(&h(LINE), 1, likely_item, &PalwEvalSubjectV1::Parent, PalwScoringKindV1::RefLogLik, 0).unwrap();
        assert!(row.claim.is_none(), "the job is free again");
        // An epoch already decided is not reopened: the reversal leaves the results alone.
        let (decided, _) = at(&s2, &p, 1_800, |_| {});
        assert_eq!(decided.improvement_epoch(&h(LINE), 1).unwrap().state, PalwEpochStateV1::Decided);
        // (The decided epoch's rows retire by a bounded sweep, so the baseline is the same block without the reversal.)
        let (baseline, _) = at(&decided, &p, 1_900, |_| {});
        let (after, _) = at(&decided, &p, 1_900, |b| {
            let _ = b.reverse_convicted_final(&ctx(1_900), id, PalwVoidReasonV2::CourtFraud);
        });
        assert_eq!(
            after.improvement_result(&h(LINE), 1, likely_item, &PalwEvalSubjectV1::Parent),
            baseline.improvement_result(&h(LINE), 1, likely_item, &PalwEvalSubjectV1::Parent),
            "a decided epoch's results stand"
        );
    }

    /// [`fold_one`] under the block's `extras`.
    fn fold_with(
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        daa: u64,
        extras: &PalwTransitionExtrasV1,
        object: &PalwConsensusObjectV2,
    ) -> Result<(), PalwStateV2Error> {
        let mut b = TransitionBuilder::new(s, p, false, false, false, false, extras);
        apply_object(&mut b, &ctx(daa), object)
    }

    /// A signature the test's verifier accepts: the key, the message and the context, concatenated.
    fn fake_sign(key: &[u8], message: &[u8], context: &[u8]) -> Vec<u8> {
        let mut out = key.to_vec();
        out.extend_from_slice(message);
        out.extend_from_slice(context);
        out
    }

    fn fake_verify(key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
        signature == fake_sign(key, message, context).as_slice()
    }

    /// **A dissected leaf of an evaluation claim is argued by F7 through the fold** (spec 17 §17.8.6.3): the subject is
    /// a real decoder with attention, its claim honest; a cone accusation that NAMES the attention leaf opens a session
    /// at `Terminal` on it under the held regime (the leaf is never tried whole there), the responder's root claim
    /// (`CourtEvalRootClaimed`) opens F7's phase over the claim's own evaluation context, the rounds and the choices are
    /// F7's own objects, and the bottom (`EvalDissection`) is graded against the phase: an honest responder is acquitted
    /// and the challenger pays; a responder whose totals lie — hidden in the last tile so every fold checks — is
    /// convicted where the dissection narrows it, its claim voided and slashed.
    #[test]
    fn a_dissected_leaf_of_an_evaluation_claim_is_argued_by_f7_and_the_fold_convicts_where_the_totals_lie() {
        use crate::palw_court_v2::{PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT, PalwCourtVerdictProofV2 as Proof};
        use crate::palw_improve_eval_court_v1 as court_v1;
        use crate::palw_tir_dissect_v1::{PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirFoldV1};
        use crate::palw_tir_one_move_v1::{PalwTirOneMoveOutcomeV1, palw_tir_one_move_accusation_v1, palw_tir_one_move_outcome_v1};
        const FORM: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1 = crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat;
        let (_subject, tokens) = use_golden_subject();
        let prompt: Vec<u32> = tokens.iter().take(3).copied().collect();
        let p = court_params();
        let base = with_real_roots(evaluating());
        test_disclosure::set(h(LINE), 1, 0, prompt.clone(), None);
        let run = claim_run(&base, 0, PalwEvalSubjectV1::Parent, CAROL, 0xE0, prompt.clone(), None, Lie::None);
        let id = h(0xE0);
        let (s1, _) = at(&base, &p, 1_600, |b| apply_object(b, &ctx(1_600), &run.object).expect("an honest claim"));
        let claim = s1.claims[&id].clone();
        let ev = evidence_of(&s1, &run, &claim);

        // The attention's commit point at the stream's last position: a leaf whose cone reduces over the history.
        let sp = &run.execution.space.stages[0];
        let dissected = sp
            .leaves()
            .iter()
            .rfind(|l| crate::palw_gen_court_v1::palw_gen_dissect_site_v1(sp, &l.coord).is_some())
            .expect("the attention's commit points are dissected");
        let index = run.execution.space.global_index(&dissected.coord).expect("a leaf of the space");

        // The accusation names the leaf and carries nothing else: under the held regime it opens a dissection;
        // outside it a leaf is argued whole, and this carries none of its cone.
        let named = Proof::EvalCone { close: Box::new(ev.named_leaf_close(index).expect("a named leaf")) };
        let mut accusation = palw_tir_one_move_accusation_v1(id, &claim, bond(BOB), PalwCourtVerdictV2::ExecutorGuilty, named);
        accusation.signature = vec![9; 8];
        crate::palw_tir_one_move_v1::palw_tir_one_move_shape_v1(&accusation).expect("the accusation's own shape");
        let outcome = |held: bool| palw_tir_one_move_outcome_v1(&s1, &claim, &accusation, &court(), 1 << 26, FORM, held);
        assert_eq!(outcome(true), Ok(PalwTirOneMoveOutcomeV1::NeedsDissection { leaf: index }), "the held regime dissects it");
        assert!(outcome(false).is_err(), "outside the held regime a leaf is argued whole, and this carries none of its cone");

        // The fold, under the held regime: a session at Terminal on the leaf; the claim's path to Final is frozen.
        let extras = PalwTransitionExtrasV1 { held_context_ladder: Some(1 << 26), ..PalwTransitionExtrasV1::default() };
        let object = PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation.clone()) };
        let (s2, _) =
            at_with(&s1, &p, 1_650, &extras, |b| apply_object(b, &ctx(1_650), &object).expect("the accusation opens a dissection"));
        let sid = *s2.court_sessions.iter().find(|(_, session)| session.claim == id).map(|(sid, _)| sid).expect("a session opened");
        assert_eq!(s2.court_session(&sid).unwrap().ladder.terminal_index(), Some(index), "at the named leaf");
        assert!(!s2.claims[&id].phase.is_terminal(), "the claim stands while it is argued");
        // An accusation that opens a dissection prosecutes: one declaring its own defeat is refused (the IR court's rule).
        let mut defeat = accusation.clone();
        defeat.verdict = PalwCourtVerdictV2::ChallengerDefeated;
        let defeat_object = PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(defeat) };
        assert!(
            matches!(fold_with(&s1, &p, 1_650, &extras, &defeat_object), Err(PalwStateV2Error::ShardCourt(_))),
            "an accusation that opens a dissection declares ExecutorGuilty"
        );

        // The responder's root claim: admitted by the acceptance layer's checks, then folded.
        let root = ev.root_claim(index, &LIMITS).expect("a root claim");
        let signature =
            fake_sign(&[CAROL], &court_v1::palw_eval_root_claim_message_v1(&sid, &root), PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT);
        crate::palw_court_v2::check_court_eval_root_claim_acceptance_v1(&s2, &sid, &root, &signature, fake_verify)
            .expect("signed by the claim's bond");
        let by_the_challenger =
            fake_sign(&[BOB], &court_v1::palw_eval_root_claim_message_v1(&sid, &root), PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT);
        assert_eq!(
            crate::palw_court_v2::check_court_eval_root_claim_acceptance_v1(&s2, &sid, &root, &by_the_challenger, fake_verify),
            Err(crate::palw_court_v2::PalwCourtV2Error::RungSignatureInvalid),
            "the responder's key only"
        );
        let site = crate::palw_court_v2::check_court_eval_root_claim_admits_v1(&s2, &sid, &root, 2, 2, &court())
            .expect("the root claim finalizes to the committed leaf");
        assert!(
            matches!(
                crate::palw_court_v2::check_court_eval_root_claim_admits_v1(&s2, &sid, &root, 4, 2, &court()),
                Err(crate::palw_court_v2::PalwCourtV2Error::ArityIsNotTheDerivedOne { declared: 4, derived: 2 })
            ),
            "the ruleset's arity only"
        );

        // The session's close door: a cone close must open the leaf the ladder narrowed to, a decode close one of the tiles it
        // concerns, and an honest close of the narrowed leaf is an acquittal (a leaf is judged in the claim's one stage-major order).
        let close_verdict = |proof: &Proof| {
            crate::palw_court_v2::adjudicate_court_close_v3(&s2, &sid, proof, &court(), 1 << 26, FORM, true, false, None)
        };
        let elsewhere = Proof::EvalCone { close: Box::new(ev.cone_close(0, &LIMITS).expect("a cone close of the first leaf")) };
        assert!(
            matches!(close_verdict(&elsewhere), Err(crate::palw_court_v2::PalwCourtV2Error::CloseIsNotTheNarrowedStep { .. })),
            "a cone close of another leaf is not the ladder's"
        );
        let token = Proof::EvalDecodeToken { close: Box::new(ev.decode_close(0).expect("a decode close")) };
        assert!(
            matches!(close_verdict(&token), Err(crate::palw_court_v2::PalwCourtV2Error::CloseIsNotTheNarrowedStep { .. })),
            "a decode close whose tiles do not include the narrowed leaf is not the ladder's"
        );
        let whole =
            Proof::EvalCone { close: Box::new(ev.cone_close(index, &LIMITS).expect("a whole cone close of the narrowed leaf")) };
        assert_eq!(
            close_verdict(&whole),
            Ok(PalwCourtVerdictV2::ChallengerDefeated),
            "an honest cone close of the narrowed leaf acquits"
        );

        // Play the phase to its bottom, honest or with a lie in reduction `lying`'s first element, hidden in the last tile.
        let tiles = (site.history_positions as u64).div_ceil(2);
        let open_phase = |lying: Option<usize>| -> PalwChainStateV2 {
            let mut claimed = root.clone();
            if let Some(r) = lying {
                claimed.totals.partials[r][0] += 1;
            }
            let signature = fake_sign(
                &[CAROL],
                &court_v1::palw_eval_root_claim_message_v1(&sid, &claimed),
                PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT,
            );
            let object = PalwConsensusObjectV2::CourtEvalRootClaimed { session_id: sid, root: Box::new(claimed), arity: 2, signature };
            let (mut state, _) =
                at_with(&s2, &p, 1_651, &extras, |b| apply_object(b, &ctx(1_651), &object).expect("the root claim opens F7's phase"));
            let mut daa = 1_652;
            while state.tir_dissection_v1(&sid).expect("an open phase").turn() == crate::palw_bisect::PalwBisectTurnV1::AwaitDisclosure
            {
                let phase = state.tir_dissection_v1(&sid).unwrap().clone();
                let mut round = ev.round(&phase, &LIMITS).expect("honest children");
                if let Some(r) = lying {
                    let ranges = phase.child_ranges();
                    let at = ranges.iter().position(|(first, count)| (*first..first + count).contains(&(tiles - 1))).unwrap_or(0);
                    round.children[at].partials[r][0] += 1;
                }
                let claimed_children = round.children.clone();
                let round_object = PalwConsensusObjectV2::CourtTirDissected { session_id: sid, round, signature: vec![1; 8] };
                (state, _) = at_with(&state, &p, daa, &extras, |b| apply_object(b, &ctx(daa), &round_object).expect("a round"));
                daa += 1;
                // The challenger names the child its own partials disagree with.
                let phase = state.tir_dissection_v1(&sid).unwrap().clone();
                let honest = ev.round(&phase, &LIMITS).expect("honest children").children;
                let child = claimed_children.iter().zip(&honest).position(|(c, h)| c != h).unwrap_or(0) as u8;
                let choice = PalwTirDissectChoiceV1 {
                    version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                    session_id: sid,
                    round: phase.round(),
                    child,
                };
                let choice_object = PalwConsensusObjectV2::CourtTirChildChosen { session_id: sid, choice, signature: vec![1; 8] };
                (state, _) = at_with(&state, &p, daa, &extras, |b| apply_object(b, &ctx(daa), &choice_object).expect("a choice"));
                daa += 1;
            }
            state
        };
        // The bottom close and the verdict the acceptance layer derives from it.
        let close_of = |state: &PalwChainStateV2| -> (PalwConsensusObjectV2, PalwCourtVerdictV2) {
            let phase = state.tir_dissection_v1(&sid).expect("an open phase").clone();
            let bottom = ev.bottom(&phase, &LIMITS).expect("the bottom close");
            let proof = Proof::EvalDissection { bottom: Box::new(bottom) };
            let verdict =
                crate::palw_court_v2::adjudicate_court_close_v3(state, &sid, &proof, &court(), 1 << 26, FORM, true, false, None)
                    .expect("the bottom adjudicates");
            (PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict, proof }, verdict)
        };

        // An honest responder is acquitted at the bottom, and the challenger pays.
        let honest = open_phase(None);
        let (close, verdict) = close_of(&honest);
        assert_eq!(verdict, PalwCourtVerdictV2::ChallengerDefeated, "an honest responder is acquitted");
        let bob_before = honest.bond(&bond(BOB)).unwrap().collateral;
        let (done, _) = at_with(&honest, &p, 1_700, &extras, |b| apply_object(b, &ctx(1_700), &close).expect("the close folds"));
        assert!(
            done.court_session(&sid).is_none() && done.tir_dissection_v1(&sid).is_none(),
            "the session and its phase end together"
        );
        assert!(!done.claims[&id].phase.is_terminal(), "the honest claim stands");
        assert!(done.bond(&bond(BOB)).unwrap().collateral < bob_before, "the challenger pays");

        // A responder whose totals lie is convicted where the dissection narrows it.
        let r = (0..site.folds.len())
            .rev()
            .find(|i| site.folds[*i] == PalwTirFoldV1::Sum && !root.elements[*i].is_empty())
            .expect("a sum reduction");
        let lying = open_phase(Some(r));
        let (close, verdict) = close_of(&lying);
        assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "the lie is convicted at its tile");
        let carol_before = lying.bond(&bond(CAROL)).unwrap().collateral;
        let (done, _) = at_with(&lying, &p, 1_700, &extras, |b| apply_object(b, &ctx(1_700), &close).expect("the close folds"));
        assert!(
            matches!(done.claims[&id].phase, PalwClaimPhaseV2::Voided { .. }),
            "the claim is voided: {:?}",
            done.claims[&id].phase
        );
        assert!(done.bond(&bond(CAROL)).unwrap().collateral < carol_before, "and its executor slashed");
        assert!(done.court_session(&sid).is_none() && done.tir_dissection_v1(&sid).is_none());
        assert!(
            done.improvement_result(&h(LINE), 1, 0, &PalwEvalSubjectV1::Parent).is_none_or(|r| r.scores.is_empty()),
            "a convicted claim records no score"
        );
    }
}
