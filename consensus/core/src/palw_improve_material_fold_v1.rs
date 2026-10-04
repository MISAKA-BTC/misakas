//! **RFC-0004 A4/A5 in the fold: the material and candidates lane's objects, tables and settlement**
//! (spec 17 §17.6, §17.7, §17.8.3, §17.11.3). A child module of `palw_state_v2`, as the core's
//! `palw_improve_fold_v1` is, so it reads the builder and the state's tables directly and writes them
//! only through their one writers.
//!
//! * readers on [`PalwChainStateV2`] — what the core's epoch machine, the evaluation lane, the node
//!   and the RPC ask of the material;
//! * the object arms: hard cases (tag 71), opt-ins (72), setter sets (73–75), datasets (76), teaching
//!   artifacts (77–78), licences (79), candidates (80), and a hold-out case's key (86, whose variant
//!   lands after Phase F's 83–85);
//! * the gate of composite openings (main's decision 7a of 2026-09-29);
//! * the two hooks the core's scoring asks: whether a drawn item still owes a reveal, and the drops
//!   and forfeits right before scoring;
//! * the step-2 sweep after the core's: artifact reveal deadlines, opt-in and licence expiries, and the
//!   bounded retirement of the material of decided epochs and of lines leaving governance.

use super::*;
use crate::palw_improve_candidate_v1::*;
use crate::palw_improve_composite_v1::PalwTirCompositeRefV1;
use crate::palw_improve_epoch_v1::*;
use crate::palw_improve_material_v1::*;
use crate::palw_improve_state_v1::*;

/// Material rows the step-2 sweep settles and deletes per block, at most (the core's retirement bound).
pub const PALW_IMPROVE_MATERIAL_SWEEP_ROWS_PER_BLOCK_V1: usize = 512;

/// The retirement index's table tags (the third field of `improvement_material_epochs`).
pub(super) const MATERIAL_CASE: u8 = 1;
pub(super) const MATERIAL_SETTER_SET: u8 = 2;
pub(super) const MATERIAL_ARTIFACT: u8 = 3;

fn refused(why: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::ImprovementRefused(why)
}

fn zero() -> Hash64 {
    Hash64::from_bytes([0; 64])
}

// ---- readers ---------------------------------------------------------------------------------

impl PalwChainStateV2 {
    /// A hard case, by its line and id (spec 17 §17.6).
    pub fn improvement_case(&self, line_id: &Hash64, case_id: &Hash64) -> Option<&PalwHardCaseRecordV1> {
        self.improvement_cases.get(&(*line_id, *case_id))
    }

    /// A job's data-use opt-in, by its job pin (RFC-0004 §10).
    pub fn improvement_opt_in(&self, job_pin: &Hash64) -> Option<&PalwDataUseOptInRecordV1> {
        self.improvement_opt_ins.get(job_pin)
    }

    /// A setter set, by its line and id (spec 17 §17.6.2).
    pub fn improvement_setter_set(&self, line_id: &Hash64, set_id: &Hash64) -> Option<&PalwSetterSetRecordV1> {
        self.improvement_setter_sets.get(&(*line_id, *set_id))
    }

    /// A setter item's prompt, once the set revealed it.
    pub fn improvement_setter_prompt(&self, line_id: &Hash64, set_id: &Hash64, index: u32) -> Option<&[u32]> {
        self.improvement_setter_set(line_id, set_id)?.prompts.as_ref()?.get(index as usize).map(Vec::as_slice)
    }

    /// A setter item's key, once the set revealed it (empty for a judged item).
    pub fn improvement_setter_key(&self, line_id: &Hash64, set_id: &Hash64, index: u32) -> Option<&[u32]> {
        self.improvement_setter_set(line_id, set_id)?.keys.as_ref()?.get(index as usize).map(Vec::as_slice)
    }

    /// A registered dataset, by its line and id (spec 17 §17.11.3's S2 reads its registrant).
    pub fn improvement_dataset(&self, line_id: &Hash64, dataset_id: &Hash64) -> Option<&PalwDatasetRecordV1> {
        self.improvement_datasets.get(&(*line_id, *dataset_id))
    }

    /// A teaching artifact, by its line and commitment.
    pub fn improvement_artifact(&self, line_id: &Hash64, commit: &Hash64) -> Option<&PalwTeachingArtifactRecordV1> {
        self.improvement_artifacts.get(&(*line_id, *commit))
    }

    /// **The original of a revealed output** on a line: the earliest-committed revealed artifact with
    /// that `output_hash` (commit order: DAA, then the commitment's bytes).
    pub fn improvement_artifact_original(&self, line_id: &Hash64, output_hash: &Hash64) -> Option<Hash64> {
        let lo = (*line_id, *output_hash, 0u64, zero());
        let hi = (*line_id, *output_hash, u64::MAX, Hash64::from_bytes([0xFF; 64]));
        self.improvement_artifact_outputs.range(lo..=hi).next().map(|(_, _, _, commit)| *commit)
    }

    /// A teacher licence, by its id.
    pub fn improvement_licence(&self, licence_id: &Hash64) -> Option<&PalwTeacherLicenceRecordV1> {
        self.improvement_licences.get(licence_id)
    }

    /// **A class admitted as a composite candidate**, with the reference it was admitted with (main's
    /// decision 7a): the only classes a composite opening may be carried for.
    pub fn improvement_composite_class(&self, class_id: &Hash64) -> Option<PalwTirCompositeRefV1> {
        self.improvement_composite_classes.get(class_id).copied()
    }

    /// **Every composite candidate class with its reference**, in class-id order (the node lane's read: a
    /// seat proves a composite's possession over its adapter section, RFC-0004 §6.7, and only this record
    /// names the section's root — it outlives the epoch row that admitted the candidate).
    pub fn improvement_composite_classes_v1(&self) -> impl Iterator<Item = (&Hash64, &PalwTirCompositeRefV1)> + '_ {
        self.improvement_composite_classes.iter()
    }

    /// **Does any material table hold a row?** The `improvement-material/v1` root block's and the
    /// carriage's one guard: all empty on every network below the fence.
    pub fn has_improvement_material_rows_v1(&self) -> bool {
        !self.improvement_cases.is_empty()
            || !self.improvement_opt_ins.is_empty()
            || !self.improvement_setter_sets.is_empty()
            || !self.improvement_datasets.is_empty()
            || !self.improvement_artifacts.is_empty()
            || !self.improvement_licences.is_empty()
            || !self.improvement_composite_classes.is_empty()
    }

    /// The IR class record of a line's head, or of an epoch's parent.
    fn improvement_ir_record(
        &self,
        class_id: &Hash64,
    ) -> Result<&crate::palw_tir_admission_v1::PalwTirClassRecordV1, PalwStateV2Error> {
        self.tir_classes.get(class_id).ok_or_else(|| refused("the line's class is not an IR class"))
    }
}

impl PalwChainStateV2 {
    /// **The classes of a governed line** a composite may be built over (RFC-0004 §6.3): its class at
    /// opt-in, its head, and every class its kept head history names.
    pub fn improvement_line_classes(&self, line_id: &Hash64) -> Vec<Hash64> {
        self.improvement_line(line_id).map(|line| improvement_line_classes_v1(self, line)).unwrap_or_default()
    }

    /// **The acceptance layer's half of a `CandidateSubmitted`, over this state** (spec 17 §17.7): the
    /// line's head, classes and policy, the candidate's and its parent's IR records and registered
    /// roots, read here, and [`palw_candidate_acceptance_v1`] over them — the declarations' form, the
    /// parent, and the artifact's admission (for a composite, every terminal close sized in the
    /// composite form under `rules`). Heavy: one a block (the acceptance walk's sizing slot).
    pub fn improvement_candidate_acceptance_v1(
        &self,
        payload: &PalwCandidateSubmissionV1,
        rules: crate::palw_improve_composite_v1::PalwTirCompositeAdmissionV1,
    ) -> Result<(), String> {
        let line = self.improvement_line(&payload.line_id).ok_or("a candidate for a line that is not governed")?;
        let policy = self.improvement_policy(&payload.line_id).ok_or("the line has no policy")?;
        let classes = improvement_line_classes_v1(self, line);
        let parent = PalwCandidateAcceptanceV1::parent_of(&line.head, &classes, &payload.artifact)?;
        let record = self.tir_class_v1(&payload.class_id).ok_or("the candidate is not a registered IR class")?;
        let artifact_root = self.class(&payload.class_id).ok_or("the candidate's class has no row")?.artifact_root;
        let parent_record = self.tir_class_v1(&parent).ok_or("the parent is not a registered IR class")?;
        let parent_root = self.class(&parent).ok_or("the parent's class has no row")?.artifact_root;
        palw_candidate_acceptance_v1(
            payload,
            &PalwCandidateAcceptanceV1 {
                head: line.head,
                line_classes: &classes,
                full_weight_candidates: policy.provenance.full_weight_candidates,
                allowed_teacher_classes: policy.provenance.teacher_classes,
                record,
                artifact_root,
                parent_record,
                parent_root,
                rules,
            },
        )
    }
}

/// **The retirement index's entry of a material row**, when it has one: cases and setter sets by the
/// epoch they belong to, artifacts once revealed (by the epoch their material joined).
pub(super) fn material_epoch_of_case_v1(key: &(Hash64, Hash64), row: &PalwHardCaseRecordV1) -> (Hash64, u64, u8, Hash64) {
    (key.0, row.epoch, MATERIAL_CASE, key.1)
}

pub(super) fn material_epoch_of_set_v1(key: &(Hash64, Hash64), row: &PalwSetterSetRecordV1) -> (Hash64, u64, u8, Hash64) {
    (key.0, row.commitment.epoch, MATERIAL_SETTER_SET, key.1)
}

pub(super) fn material_epoch_of_artifact_v1(
    key: &(Hash64, Hash64),
    row: &PalwTeachingArtifactRecordV1,
) -> Option<(Hash64, u64, u8, Hash64)> {
    row.revealed.as_ref().map(|_| (key.0, row.epoch, MATERIAL_ARTIFACT, key.1))
}

/// **Rebuild the material lane's derived indices** from its tables (never serialized, never hashed).
pub(super) fn rebuild_improvement_material_indices_v1(state: &mut PalwChainStateV2) {
    let mut outputs = BTreeSet::new();
    let mut answers = BTreeSet::new();
    let mut deadlines = BTreeSet::new();
    let mut epochs = BTreeSet::new();
    for (key, row) in &state.improvement_artifacts {
        match &row.revealed {
            Some(a) => {
                outputs.insert((key.0, a.output_hash, row.committed_daa, key.1));
                if a.kind == PalwTeachingArtifactKindV1::Answer {
                    answers.insert((key.0, a.task_id, row.committed_daa, key.1));
                }
            }
            None => {
                deadlines.insert((row.reveal_by_daa, key.0, key.1));
            }
        }
        if let Some(entry) = material_epoch_of_artifact_v1(key, row) {
            epochs.insert(entry);
        }
    }
    for (key, row) in &state.improvement_cases {
        epochs.insert(material_epoch_of_case_v1(key, row));
    }
    for (key, row) in &state.improvement_setter_sets {
        epochs.insert(material_epoch_of_set_v1(key, row));
    }
    state.improvement_artifact_outputs = outputs;
    state.improvement_artifact_answers = answers;
    state.improvement_artifact_deadlines = deadlines;
    state.improvement_material_epochs = epochs;
    state.improvement_opt_in_expiries = state.improvement_opt_ins.iter().map(|(pin, row)| (row.expires_daa, *pin)).collect();
    state.improvement_licence_expiries = state.improvement_licences.iter().map(|(id, row)| (row.licence.expiry_daa, *id)).collect();
}

// ---- helpers ----------------------------------------------------------------------------------

impl TransitionBuilder<'_> {
    /// The line's policy, refused unless the line is governed at `daa`.
    fn material_line_policy_v1(
        &self,
        line_id: &Hash64,
        daa: u64,
    ) -> Result<(PalwImprovementLineV1, PalwImprovementPolicyV1), PalwStateV2Error> {
        if !self.params.improve_active_at(daa) {
            return Err(refused("palw_improvement_v1 is not in force"));
        }
        let line = self.state.improvement_lines.get(line_id).cloned().ok_or_else(|| refused("the line is not governed"))?;
        if !line.governed_at(daa) {
            return Err(refused("the line is not governed"));
        }
        let policy =
            self.state.improvement_policies.get(line_id).map(|r| r.policy.clone()).ok_or_else(|| refused("the line has no policy"))?;
        Ok((line, policy))
    }

    /// **Is `ids` a prompt of the class `class_id`**: every id under its program's token bound.
    fn material_ids_in_bound_v1(&self, class_id: &Hash64, ids: &[u32]) -> Result<(), PalwStateV2Error> {
        let bound = self.state.improvement_ir_record(class_id)?.facts.token_bound;
        if ids.iter().any(|id| *id >= bound) {
            return Err(refused("an id past the class's token bound"));
        }
        Ok(())
    }

    /// **Is `licence_id` a registered licence covering training at `daa`** — in `domain` when the use
    /// has one (a case's), in whatever domains it names otherwise (a dataset's, a candidate's).
    fn material_licence_covers_v1(&self, licence_id: &Hash64, domain: Option<u16>, daa: u64) -> bool {
        self.state.improvement_licence(licence_id).is_some_and(|r| match domain {
            Some(d) => palw_teacher_licence_covers_v1(&r.licence, d, daa),
            None => daa < r.licence.expiry_daa && r.licence.uses & PALW_IMPROVE_LICENCE_USE_TRAINING_V1 != 0,
        })
    }

    /// The prompt-ids form a claim's class commits its jobs' prompts in (ADR-0118 Decision 3): the
    /// Merkle root for a held class, the network's form otherwise.
    fn material_claim_prompt_form_v1(&self, claim: &PalwClaimStateV2) -> crate::palw_prompt_ids_v1::PalwPromptIdsFormV1 {
        let network = self.extras.prompt_ids_form_v1();
        if let Some(record) = self.state.tir_classes.get(&claim.class_id) {
            return record.facts.prompt_ids_form(network);
        }
        match self.state.fp_work_profiles.get(&claim.class_id) {
            Some(profile) => crate::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(network, profile),
            None => network,
        }
    }
}

// ---- the object arms -------------------------------------------------------------------------

/// **`HardCaseSubmitted` (tag 71)** (spec 17 §17.6.1; RFC-0004 §5.1; PALW-MIP-19): the case's form,
/// its prompt under the line head's token bound, one case per id, its source — a job's prompt only
/// under that job's opt-in and only if it is the job's committed prompt in the job's tokenizer, or a
/// revealed `SyntheticProblem`/`HardCaseVariant` of the line — and hardness evidence naming a claim the
/// chain holds. Then the core places it (and takes `hard_case_fee`).
pub(super) fn apply_hard_case_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    case: &PalwHardCaseV1,
    submitter: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    palw_hard_case_form_v1(case).map_err(refused)?;
    let (line, _) = builder.material_line_policy_v1(&case.line_id, daa)?;
    if builder.state.improvement_case(&case.line_id, &case.case_id).is_some() {
        return Err(refused("the line already holds this case"));
    }
    builder.material_ids_in_bound_v1(&line.head, &case.prompt_ids)?;
    match case.source {
        PalwCaseSourceV1::UsageOptIn { job_pin } => {
            let opt_in = builder.state.improvement_opt_in(&job_pin).ok_or_else(|| refused("a usage case of a job never opted in"))?;
            if opt_in.tokenizer_id != builder.state.improvement_ir_record(&line.head)?.tokenizer_id {
                return Err(refused("the opted-in job ran under another tokenizer than the line's"));
            }
            if !opt_in.prompt_is(&case.prompt_ids) {
                return Err(refused("the case's prompt is not the opted-in job's committed prompt"));
            }
        }
        PalwCaseSourceV1::Artifact { artifact_id } => {
            let artifact = builder
                .state
                .improvement_artifact(&case.line_id, &artifact_id)
                .and_then(|r| r.revealed.as_ref())
                .ok_or_else(|| refused("an artifact case of no revealed artifact of the line"))?;
            if !matches!(artifact.kind, PalwTeachingArtifactKindV1::SyntheticProblem | PalwTeachingArtifactKindV1::HardCaseVariant) {
                return Err(refused("an artifact case of an artifact that is no problem"));
            }
        }
        PalwCaseSourceV1::Setter => {}
    }
    if let Some(claim) = case.head_evidence
        && builder.state.claim(&claim).is_none()
    {
        return Err(refused("hardness evidence naming no claim"));
    }
    let placement =
        builder.note_improvement_material_v1(&case.line_id, PalwMaterialKindV1::HardCase, &case.case_id, submitter, daa)?;
    let (epoch, holdout) = match placement {
        PalwMaterialPlacementV1::EpochMaterial { epoch } | PalwMaterialPlacementV1::NextEpochMaterial { epoch } => (epoch, false),
        PalwMaterialPlacementV1::HoldOut { epoch } => (epoch, true),
    };
    builder.write_improvement_case(
        (case.line_id, case.case_id),
        Some(PalwHardCaseRecordV1 { case: case.clone(), submitter: *submitter, admitted_daa: daa, epoch, holdout, revealed: None }),
    );
    Ok(())
}

/// **`DataUseOptIn` (tag 72)** (RFC-0004 §10; PALW-MIP-19): the facts hash to the pin, the pin is the
/// free-prompt claim's recorded job, once. The opt-in keeps the job's prompt commitment, in the form
/// the job's class committed it, until the claim's retention ends.
pub(super) fn apply_data_use_opt_in_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    opt_in: &PalwDataUseOptInV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if !builder.params.improve_active_at(daa) {
        return Err(refused("palw_improvement_v1 is not in force"));
    }
    palw_data_use_opt_in_form_v1(opt_in).map_err(refused)?;
    let claim = builder.state.claim(&opt_in.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(opt_in.claim))?;
    if !matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }) || claim.job_identity != opt_in.job_pin {
        return Err(refused("the opt-in's pin is not the free-prompt claim's recorded job"));
    }
    if claim.trace_retention_daa <= daa {
        return Err(refused("the claim's retention has ended"));
    }
    if builder.state.improvement_opt_in(&opt_in.job_pin).is_some() {
        return Err(refused("the job is already opted in"));
    }
    let merkle = builder.material_claim_prompt_form_v1(&claim) == crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1;
    builder.write_improvement_opt_in(
        opt_in.job_pin,
        Some(PalwDataUseOptInRecordV1 {
            claim: opt_in.claim,
            committer: claim.bond,
            prompt_token_ids_hash: opt_in.job.prompt_token_ids_hash,
            prompt_ids_merkle: merkle,
            tokenizer_id: opt_in.job.tokenizer_id,
            prompt_tokens: opt_in.job.prompt_tokens,
            opted_in_daa: daa,
            expires_daa: claim.trace_retention_daa,
        }),
    );
    Ok(())
}

/// **`SetterSetCommitted` (tag 73)** (spec 17 §17.6.2): the form, one set per id, the epoch the core
/// enters it in (the line's open epoch, before `t_close`, holding `setter_bond`).
pub(super) fn apply_setter_set_committed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    set: &PalwSetterSetCommitmentV1,
    setter: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    palw_setter_set_form_v1(set).map_err(refused)?;
    let (_, policy) = builder.material_line_policy_v1(&set.line_id, daa)?;
    if builder.state.improvement_setter_set(&set.line_id, &set.set_id).is_some() {
        return Err(refused("the line already holds this setter set"));
    }
    let line = builder.state.improvement_lines.get(&set.line_id).ok_or_else(|| refused("the line is not governed"))?;
    if line.open_epoch != Some(set.epoch) {
        return Err(refused("a setter set for an epoch that is not the line's open one"));
    }
    let epoch = builder.add_improvement_setter_set_v1(&set.line_id, &set.set_id, setter, set.items, daa)?;
    debug_assert_eq!(epoch, set.epoch, "the core entered the set in the open epoch");
    builder.write_improvement_setter_set(
        (set.line_id, set.set_id),
        Some(PalwSetterSetRecordV1 {
            commitment: *set,
            setter: *setter,
            committed_daa: daa,
            bond: policy.fees.setter_bond,
            prompts: None,
            keys: None,
        }),
    );
    Ok(())
}

/// The epoch header a reveal is judged against.
fn material_epoch_v1(
    builder: &TransitionBuilder<'_>,
    line_id: &Hash64,
    epoch: u64,
) -> Result<PalwImprovementEpochV1, PalwStateV2Error> {
    builder.state.improvement_epoch(line_id, epoch).cloned().ok_or_else(|| refused("no such epoch"))
}

/// **`SetterSetRevealed` (tag 74)** (spec 17 §17.8.2): the prompts open the set's commitment, once,
/// while its epoch is `Evaluating` (from the draw), each under the epoch parent's token bound.
pub(super) fn apply_setter_set_revealed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    reveal: &PalwSetterSetRevealV1,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.improve_active_at(ctx.daa_score) {
        return Err(refused("palw_improvement_v1 is not in force"));
    }
    let key = (reveal.line_id, reveal.set_id);
    let mut row = builder.state.improvement_setter_sets.get(&key).cloned().ok_or_else(|| refused("a reveal of no setter set"))?;
    if row.prompts.is_some() {
        return Err(refused("the set's prompts are already revealed"));
    }
    palw_setter_prompts_open_v1(&row.commitment, reveal).map_err(refused)?;
    let header = material_epoch_v1(builder, &reveal.line_id, reveal.epoch)?;
    if header.state != PalwEpochStateV1::Evaluating {
        return Err(refused("a setter set's prompts are revealed while its epoch is Evaluating"));
    }
    for prompt in &reveal.prompts {
        builder.material_ids_in_bound_v1(&header.parent, prompt)?;
    }
    row.prompts = Some(reveal.prompts.clone());
    builder.write_improvement_setter_set(key, Some(row));
    Ok(())
}

/// The policy's ExactMatch key cap, if it scores ExactMatch.
fn material_key_cap_v1(policy: &PalwImprovementPolicyV1) -> Option<u32> {
    policy.eval.stages.iter().find_map(|stage| match stage.params {
        PalwScoringParamsV1::ExactMatch { key_cap, .. } => Some(key_cap),
        _ => None,
    })
}

/// **`SetterKeysRevealed` (tag 75)** (spec 17 §17.8.2–3): the keys open the set's commitment, once,
/// after its prompts, while its epoch is `Closing` — every key within the policy's ExactMatch
/// `key_cap` (none but empty keys without an ExactMatch stage). Each drawn item's key is then scored
/// for every subject and pays its S1 bounty.
pub(super) fn apply_setter_keys_revealed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    reveal: &PalwSetterKeysRevealV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if !builder.params.improve_active_at(daa) {
        return Err(refused("palw_improvement_v1 is not in force"));
    }
    let key = (reveal.line_id, reveal.set_id);
    let mut row = builder.state.improvement_setter_sets.get(&key).cloned().ok_or_else(|| refused("a reveal of no setter set"))?;
    if row.prompts.is_none() {
        return Err(refused("a set's keys before its prompts"));
    }
    if row.keys.is_some() {
        return Err(refused("the set's keys are already revealed"));
    }
    palw_setter_keys_open_v1(&row.commitment, reveal).map_err(refused)?;
    let drawn: Vec<(u32, u32)> = builder
        .state
        .improvement_items(&reveal.line_id, reveal.epoch)
        .into_iter()
        .filter_map(|item| match item.source {
            PalwItemSourceV1::Setter { set_id, index } if set_id == reveal.set_id && !item.dropped => Some((item.item, index)),
            _ => None,
        })
        .collect();
    let header = material_epoch_v1(builder, &reveal.line_id, reveal.epoch)?;
    let items: Vec<u32> = drawn.iter().map(|(item, _)| *item).collect();
    if !keys_may_open_v1(&builder.state, &header, &items) {
        return Err(refused("a set's keys before every subject's generations on its items are final"));
    }
    let policy = builder
        .state
        .improvement_policies
        .get(&reveal.line_id)
        .map(|r| r.policy.clone())
        .ok_or_else(|| refused("the line has no policy"))?;
    let cap = material_key_cap_v1(&policy).unwrap_or(0) as usize;
    if reveal.keys.iter().any(|k| k.len() > cap) {
        return Err(refused("a key past the policy's ExactMatch key_cap"));
    }
    row.keys = Some(reveal.keys.clone());
    builder.write_improvement_setter_set(key, Some(row));
    for (item, index) in drawn {
        let exact = &reveal.keys[index as usize];
        if exact.is_empty() {
            continue;
        }
        palw_improve_exact_match_on_reveal_hook_v1(builder, &reveal.line_id, reveal.epoch, item, exact)?;
        let task = palw_improve_setter_item_id_v1(&reveal.set_id, index);
        pay_s1_bounty_v1(builder, &reveal.line_id, &task, exact, daa)?;
    }
    Ok(())
}

/// **`DatasetRegistered` (tag 76)** (spec 17 §17.6.1, §17.11.1; RFC-0004 §5.3, §9): the form, one per
/// id, every teacher class the policy's, every licence class the policy's — or, under
/// `LICENSED_DISTILL`, a registered, unexpired licence that covers training (at least one) — then
/// `dataset_bond` held and the dataset placed in the material.
pub(super) fn apply_dataset_registered_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    dataset: &PalwDatasetV1,
    registrant: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if builder.params.audit_1004_active_at(daa) {
        // **Lane PA, RF-1 (`palw_audit_1004_v1`): the dataset's id names its registrant**, so a copy of a published dataset cannot be
        // registered first under another bond to take its S2 contributor share. The form is judged with the registrant-bound id.
        crate::palw_improve_material_v1::palw_dataset_form_bound_v1(dataset, registrant).map_err(refused)?;
    } else {
        palw_dataset_form_v1(dataset).map_err(refused)?;
    }
    let (_, policy) = builder.material_line_policy_v1(&dataset.line_id, daa)?;
    if builder.state.improvement_dataset(&dataset.line_id, &dataset.dataset_id).is_some() {
        return Err(refused("the line already holds this dataset"));
    }
    if dataset.teacher_classes & !policy.provenance.teacher_classes != 0 {
        return Err(refused("a dataset's teacher class the line's policy does not allow"));
    }
    let licensed = dataset.teacher_classes & PalwTeacherClassV1::LicensedDistill.bit() != 0;
    let mut licences = 0usize;
    for class in &dataset.license_classes {
        if policy.provenance.licence_classes.contains(class) {
            continue;
        }
        if licensed && builder.material_licence_covers_v1(class, None, daa) {
            licences += 1;
            continue;
        }
        return Err(refused("a dataset's licence class the policy does not allow, and no covering licence"));
    }
    if licensed && licences == 0 {
        return Err(refused("LICENSED_DISTILL declared with no registered licence covering it"));
    }
    builder.debit_improvement_hold_v1(&dataset.line_id, registrant, policy.fees.dataset_bond, daa)?;
    let placement =
        builder.note_improvement_material_v1(&dataset.line_id, PalwMaterialKindV1::Dataset, &dataset.dataset_id, registrant, daa)?;
    let epoch = match placement {
        PalwMaterialPlacementV1::EpochMaterial { epoch }
        | PalwMaterialPlacementV1::NextEpochMaterial { epoch }
        | PalwMaterialPlacementV1::HoldOut { epoch } => epoch,
    };
    builder.write_improvement_dataset(
        (dataset.line_id, dataset.dataset_id),
        Some(PalwDatasetRecordV1 {
            dataset: dataset.clone(),
            registrant: *registrant,
            registered_daa: daa,
            bond: policy.fees.dataset_bond,
            epoch,
        }),
    );
    Ok(())
}

/// **`TeachingArtifactCommitted` (tag 77)** (RFC-0004 §5.3, §8.1): one per commitment on a governed
/// line, `artifact_bond` held, to be revealed within the policy's `w_collect` (commit order decides
/// duplicates and bounties).
pub(super) fn apply_artifact_committed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    commit: &PalwTeachingArtifactCommitV1,
    teacher: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if commit.commit == zero() {
        return Err(refused("an artifact commitment to nothing"));
    }
    let (_, policy) = builder.material_line_policy_v1(&commit.line_id, daa)?;
    if builder.state.improvement_artifact(&commit.line_id, &commit.commit).is_some() {
        return Err(refused("the line already holds this commitment"));
    }
    builder.debit_improvement_hold_v1(&commit.line_id, teacher, policy.fees.artifact_bond, daa)?;
    builder.write_improvement_artifact(
        (commit.line_id, commit.commit),
        Some(PalwTeachingArtifactRecordV1 {
            commit: *commit,
            teacher: *teacher,
            committed_daa: daa,
            bond: policy.fees.artifact_bond,
            reveal_by_daa: daa.saturating_add(policy.windows.w_collect.max(1)),
            revealed: None,
            revealed_daa: 0,
            epoch: 0,
        }),
    );
    Ok(())
}

/// **Is a revealed artifact admissible by licence** (RFC-0004 §5.3's teacher-class table, §9): its
/// teacher class the policy's; under `LICENSED_DISTILL` its licence class a registered, unexpired
/// licence covering training in its task's domain (a case of the line; any domain otherwise needs a
/// licence naming none); under every other class, a licence class the policy allows.
fn artifact_admissible_v1(
    builder: &TransitionBuilder<'_>,
    policy: &PalwImprovementPolicyV1,
    a: &PalwTeachingArtifactV1,
    daa: u64,
) -> bool {
    if policy.provenance.teacher_classes & a.teacher_type.bit() == 0 {
        return false;
    }
    if a.teacher_type == PalwTeacherClassV1::LicensedDistill {
        let domain = builder.state.improvement_case(&a.line_id, &a.task_id).map(|r| r.case.domain);
        return builder.material_licence_covers_v1(&a.license_class, domain, daa);
    }
    policy.provenance.licence_classes.contains(&a.license_class)
}

/// **`TeachingArtifactRevealed` (tag 78)** (RFC-0004 §5.3): the payload opens a live commitment of its
/// line, before its reveal deadline. What it reveals is then judged, and the object stands either way:
/// spam — malformed, inadmissible by licence, or a duplicate of an earlier commitment's output —
/// forfeits the bond and leaves; an earlier commitment revealing an output a later one already
/// revealed makes the later one the duplicate; an honest artifact joins the material, its bond held
/// until the retirement of the epoch it joined. On a line no longer governed, the bond is refunded.
pub(super) fn apply_artifact_revealed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    artifact: &PalwTeachingArtifactV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if !builder.params.improve_active_at(daa) {
        return Err(refused("palw_improvement_v1 is not in force"));
    }
    let commit = palw_teaching_artifact_commit_v1(artifact);
    let key = (artifact.line_id, commit);
    let mut row = builder.state.improvement_artifacts.get(&key).cloned().ok_or_else(|| refused("a reveal of no live commitment"))?;
    if row.revealed.is_some() {
        return Err(refused("the artifact is already revealed"));
    }
    if daa >= row.reveal_by_daa {
        return Err(refused("an artifact revealed past its deadline"));
    }
    let governed = builder.state.improvement_governed_at(&artifact.line_id, daa);
    let policy = builder.state.improvement_policies.get(&artifact.line_id).map(|r| r.policy.clone());
    let (true, Some(policy)) = (governed, policy) else {
        builder.release_improvement_hold_v1(&artifact.line_id, &row.teacher, row.bond)?;
        builder.write_improvement_artifact(key, None);
        return Ok(());
    };
    let spam = palw_teaching_artifact_form_v1(artifact).is_err() || !artifact_admissible_v1(builder, &policy, artifact, daa);
    let original = builder.state.improvement_artifact_original(&artifact.line_id, &artifact.output_hash);
    let earlier = |other: &Hash64| {
        builder
            .state
            .improvement_artifact(&artifact.line_id, other)
            .is_some_and(|o| (o.committed_daa, *other) < (row.committed_daa, commit))
    };
    if spam || original.as_ref().is_some_and(earlier) {
        builder.forfeit_improvement_hold_v1(&artifact.line_id, row.bond)?;
        builder.write_improvement_artifact(key, None);
        return Ok(());
    }
    // Later commitments that revealed this output first are the duplicates now.
    let later: Vec<Hash64> = builder
        .state
        .improvement_artifact_outputs
        .range(
            (artifact.line_id, artifact.output_hash, 0u64, zero())
                ..=(artifact.line_id, artifact.output_hash, u64::MAX, Hash64::from_bytes([0xFF; 64])),
        )
        .map(|(_, _, _, c)| *c)
        .collect();
    for other in later {
        if let Some(dup) = builder.state.improvement_artifact(&artifact.line_id, &other).cloned() {
            builder.forfeit_improvement_hold_v1(&artifact.line_id, dup.bond)?;
            builder.write_improvement_artifact((artifact.line_id, other), None);
        }
    }
    let placement = builder.note_improvement_material_v1(
        &artifact.line_id,
        PalwMaterialKindV1::TeachingArtifact,
        &artifact.output_hash,
        &row.teacher,
        daa,
    )?;
    row.epoch = match placement {
        PalwMaterialPlacementV1::EpochMaterial { epoch }
        | PalwMaterialPlacementV1::NextEpochMaterial { epoch }
        | PalwMaterialPlacementV1::HoldOut { epoch } => epoch,
    };
    row.revealed = Some(artifact.clone());
    row.revealed_daa = daa;
    builder.write_improvement_artifact(key, Some(row));
    Ok(())
}

/// **`TeacherLicenceRegistered` (tag 79)** (RFC-0004 §9; PALW-MIP-18): the form, unexpired, once. Not
/// bound to a line: every line's `LICENSED_DISTILL` material may cite it.
pub(super) fn apply_teacher_licence_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    licence: &PalwTeacherLicenceV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if !builder.params.improve_active_at(daa) {
        return Err(refused("palw_improvement_v1 is not in force"));
    }
    palw_teacher_licence_form_v1(licence).map_err(refused)?;
    if licence.expiry_daa <= daa {
        return Err(refused("a licence already expired"));
    }
    // **Lane PA, RF-4 (`palw_audit_1004_v1`)**: a licence's life is bounded (the sweep removes it at its expiry, so the table is
    // bounded by the registration rate times this life; a table CAP would itself be a griefing channel and is not added).
    if builder.params.audit_1004_active_at(daa) {
        if licence.expiry_daa > daa.saturating_add(crate::palw_audit_1004_v1::PALW_AUDIT_1004_MAX_LICENCE_LIFE_DAA_V1) {
            return Err(refused("a licence's expiry is past the longest life a licence may be registered for"));
        }
    }
    if builder.state.improvement_licence(&licence.licence_id).is_some() {
        return Err(refused("the licence is already registered"));
    }
    builder.write_improvement_licence(
        licence.licence_id,
        Some(PalwTeacherLicenceRecordV1 { licence: licence.clone(), registered_daa: daa }),
    );
    Ok(())
}

/// **The classes of a governed line** a composite may be built over (RFC-0004 §6.3): its class at
/// opt-in, and every class its kept head history names.
pub(super) fn improvement_line_classes_v1(state: &PalwChainStateV2, line: &PalwImprovementLineV1) -> Vec<Hash64> {
    let mut classes = vec![line.class_id, line.head];
    for (_, entry) in state.improvement_head_history(&line.line_id) {
        classes.push(entry.class_id);
        classes.extend(entry.previous);
    }
    classes.sort();
    classes.dedup();
    classes
}

/// **`CandidateSubmitted` (tag 80)** (spec 17 §17.7; RFC-0004 §6; PALW-MIP-8, PALW-MIP-15): the fold's
/// half. The acceptance layer judged the signature and the class (the composite and family rules and
/// the sizing, `palw_candidate_acceptance_v1`); here, the second lock on the cheap parts — the
/// declarations' form, a composite's parent a class of the line, not the head — and the references:
/// every declared dataset registered on the line, every cited licence registered, unexpired and for
/// training. Then the core admits it (window, `k_max`, the bar, the payment) and a composite's class
/// is recorded with its reference, the only class a composite opening may name (decision 7a).
pub(super) fn apply_candidate_submitted_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    candidate: &PalwCandidateSubmissionV1,
    submitter: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    let (line, policy) = builder.material_line_policy_v1(&candidate.line_id, daa)?;
    // **Lane PA, RF-1 (`palw_audit_1004_v1`): the S2 trainer reward goes to the candidate's submitter, so the submitter must be the
    // candidate class's own registrant** — nobody else's class can be entered for the reward it earns.
    if builder.params.audit_1004_active_at(daa)
        && builder.state.classes.get(&candidate.class_id).and_then(|record| record.registrant_bond) != Some(*submitter)
    {
        return Err(refused("a candidate submitted by a bond that is not its class's registrant"));
    }
    palw_candidate_declarations_form_v1(&candidate.declarations, policy.provenance.teacher_classes).map_err(refused)?;
    let classes = improvement_line_classes_v1(&builder.state, &line);
    PalwCandidateAcceptanceV1::parent_of(&line.head, &classes, &candidate.artifact).map_err(refused)?;
    if matches!(candidate.artifact, crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { .. })
        && !policy.provenance.full_weight_candidates
    {
        return Err(refused("the line's policy admits no full-weight candidate"));
    }
    for (dataset_id, _) in &candidate.declarations.datasets {
        if builder.state.improvement_dataset(&candidate.line_id, dataset_id).is_none() {
            return Err(refused("a declared dataset the line has not registered"));
        }
    }
    for licence_id in &candidate.declarations.licences {
        if !builder.material_licence_covers_v1(licence_id, None, daa) {
            return Err(refused("a cited licence that is not registered, has expired, or licenses no training"));
        }
    }
    builder.admit_improvement_candidate_v1(
        &candidate.line_id,
        candidate.epoch,
        &candidate.class_id,
        submitter,
        candidate.artifact,
        palw_candidate_declarations_digest_v1(&candidate.declarations),
        candidate.declarations.datasets.clone(),
        daa,
    )?;
    if let Some(reference) = candidate.artifact.composite()
        && builder.state.improvement_composite_class(&candidate.class_id).is_none()
    {
        builder.write_improvement_composite_class(candidate.class_id, Some(reference));
    }
    Ok(())
}

/// **`HardCaseKeyRevealed` (tag 86)** (spec 17 §17.8.2): the key opens the case's committed reference,
/// once. A hold-out case's reference (a `Continuation`) is disclosed from the draw (`Evaluating`); its
/// key (an `ExactKey`) only in `Closing`, within the policy's ExactMatch `key_cap`. A training case's
/// may be disclosed at any time. A drawn case's key is then scored for every subject, pays the S1
/// bounty of the first matching `Answer`, and — for a problem an artifact set — the setter's reward
/// when the parent fails it.
pub(super) fn apply_case_key_revealed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    reveal: &PalwCaseKeyRevealV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if !builder.params.improve_active_at(daa) {
        return Err(refused("palw_improvement_v1 is not in force"));
    }
    palw_case_key_reveal_form_v1(reveal).map_err(refused)?;
    let key = (reveal.line_id, reveal.case_id);
    let mut row = builder.state.improvement_cases.get(&key).cloned().ok_or_else(|| refused("a key of no case"))?;
    if row.revealed.is_some() {
        return Err(refused("the case's key is already revealed"));
    }
    let (commitment, exact) = match row.case.reference {
        PalwCaseReferenceV1::ExactKey { commitment } => (commitment, true),
        PalwCaseReferenceV1::Continuation { commitment } => (commitment, false),
        PalwCaseReferenceV1::None => return Err(refused("a key of a judged-only case")),
    };
    if palw_case_key_commitment_v1(&reveal.line_id, &reveal.key, &reveal.salt) != commitment {
        return Err(refused("the key does not open the case's commitment"));
    }
    let policy = builder
        .state
        .improvement_policies
        .get(&reveal.line_id)
        .map(|r| r.policy.clone())
        .ok_or_else(|| refused("the line has no policy"))?;
    let drawn: Vec<u32> = match row.holdout {
        true => builder
            .state
            .improvement_items(&reveal.line_id, row.epoch)
            .into_iter()
            .filter(|item| item.source == PalwItemSourceV1::HoldOut && item.case_id == reveal.case_id && !item.dropped)
            .map(|item| item.item)
            .collect(),
        false => Vec::new(),
    };
    if row.holdout {
        let header = material_epoch_v1(builder, &reveal.line_id, row.epoch)?;
        let open = match exact {
            true => keys_may_open_v1(&builder.state, &header, &drawn),
            false => matches!(header.state, PalwEpochStateV1::Evaluating | PalwEpochStateV1::Closing),
        };
        if !open {
            return Err(refused(
                "a hold-out case's key before its disclosure (a reference from the draw, a key once its generations are final)",
            ));
        }
    }
    if exact && reveal.key.len() > material_key_cap_v1(&policy).unwrap_or(0) as usize {
        return Err(refused("a key past the policy's ExactMatch key_cap"));
    }
    row.revealed = Some(reveal.key.clone());
    let (holdout, epoch) = (row.holdout, row.epoch);
    builder.write_improvement_case(key, Some(row.clone()));
    if !exact {
        return Ok(());
    }
    if holdout {
        for item in drawn {
            palw_improve_exact_match_on_reveal_hook_v1(builder, &reveal.line_id, epoch, item, &reveal.key)?;
            pay_s1_setter_v1(builder, &reveal.line_id, epoch, item, &row.case)?;
        }
    }
    pay_s1_bounty_v1(builder, &reveal.line_id, &reveal.case_id, &reveal.key, daa)?;
    Ok(())
}

// ---- S1 (spec 17 §17.11.3) ---------------------------------------------------------------------

/// **The S1 bounty of a key** (RFC-0004 §8.1): the first revealed `Answer` to `task`, in commit order,
/// committed before this block, whose answer span is the key, earns `s1_bounty` while the period's
/// budget lasts. Once per key: a key is revealed once.
fn pay_s1_bounty_v1(
    builder: &mut TransitionBuilder<'_>,
    line_id: &Hash64,
    task: &Hash64,
    key: &[u32],
    daa: u64,
) -> Result<(), PalwStateV2Error> {
    let lo = (*line_id, *task, 0u64, zero());
    let hi = (*line_id, *task, daa.saturating_sub(1), Hash64::from_bytes([0xFF; 64]));
    if daa == 0 {
        return Ok(());
    }
    let winner = builder.state.improvement_artifact_answers.range(lo..=hi).find_map(|(_, _, _, commit)| {
        let row = builder.state.improvement_artifact(line_id, commit)?;
        (row.revealed.as_ref()?.answer_span.as_slice() == key).then_some(row.teacher)
    });
    if let Some(teacher) = winner {
        builder.grant_improvement_s1_v1(line_id, &teacher, PalwRewardStageV1::S1Bounty)?;
    }
    Ok(())
}

/// **The S1 setter reward** (RFC-0004 §8.1): a drawn hold-out case made from a `SyntheticProblem` or
/// `HardCaseVariant` artifact of the line, which the parent verifiably fails — its ExactMatch score,
/// recorded at this reveal, is 0 — earns `s1_setter_reward` for the artifact's teacher.
#[allow(dead_code)] // read by the tag-86 arm
fn pay_s1_setter_v1(
    builder: &mut TransitionBuilder<'_>,
    line_id: &Hash64,
    epoch: u64,
    item: u32,
    case: &PalwHardCaseV1,
) -> Result<(), PalwStateV2Error> {
    let PalwCaseSourceV1::Artifact { artifact_id } = case.source else { return Ok(()) };
    let failed = builder
        .state
        .improvement_result(line_id, epoch, item, &PalwEvalSubjectV1::Parent)
        .is_some_and(|r| r.scores.iter().any(|s| s.kind == PalwScoringKindV1::ExactMatch && s.value == 0));
    let teacher = builder.state.improvement_artifact(line_id, &artifact_id).and_then(|r| {
        let a = r.revealed.as_ref()?;
        matches!(a.kind, PalwTeachingArtifactKindV1::SyntheticProblem | PalwTeachingArtifactKindV1::HardCaseVariant)
            .then_some(r.teacher)
    });
    if let (true, Some(teacher)) = (failed, teacher) {
        builder.grant_improvement_s1_v1(line_id, &teacher, PalwRewardStageV1::S1Setter)?;
    }
    Ok(())
}

/// **May a generated item's key be disclosed** (spec 17 §17.8.2): in `Closing`, or in `Evaluating`
/// once every subject's generation claims on the items are final — the evaluation lane's answer.
fn keys_may_open_v1(state: &PalwChainStateV2, header: &PalwImprovementEpochV1, items: &[u32]) -> bool {
    matches!(header.state, PalwEpochStateV1::Evaluating | PalwEpochStateV1::Closing)
        && palw_improve_generations_settled_hook_v1(state, header, items)
}

/// **The evaluation lane's settlement predicate** (`improvement_eval_generations_settled_v1`, rfc4/eval):
/// in `Evaluating`, every subject's ExactMatch job on `items` holds a final claim; in `Closing`, none
/// holds a live one. The evaluation lane replaces this body with its job table's answer; until then a
/// key opens in `Closing` only.
fn palw_improve_generations_settled_hook_v1(state: &PalwChainStateV2, header: &PalwImprovementEpochV1, items: &[u32]) -> bool {
    header.state == PalwEpochStateV1::Closing && state.improvement_eval_generations_settled_v1(&header.line_id, header.epoch, items)
}

/// **The evaluation lane's ExactMatch scorer at a key's reveal** (spec 17 §17.8.3): for every subject
/// of the epoch, its generation claim's answer span against `key`, recorded through
/// `record_improvement_score_v1`. The evaluation lane (A6) replaces this body with its builder call
/// `score_improvement_exact_match_v1(line, epoch, item, key)`; until then a revealed key scores
/// nothing, and the item counts as the missing-evaluation rule says.
fn palw_improve_exact_match_on_reveal_hook_v1(
    builder: &mut TransitionBuilder<'_>,
    line_id: &Hash64,
    epoch: u64,
    item: u32,
    key: &[u32],
) -> Result<(), PalwStateV2Error> {
    builder.score_improvement_exact_match_v1(line_id, epoch, item, key)
}

// ---- the hooks the core's scoring asks (spec 17 §17.5.3 step 7, §17.9.1) ---------------------------

/// **Does a drawn item of the epoch still owe a reveal?** A setter set's prompts or keys, or a
/// hold-out case's committed key or reference. The core scores early (before `t_score`) only when
/// nothing is owed.
pub(super) fn palw_improve_material_pending_v1(state: &PalwChainStateV2, line_id: &Hash64, epoch: u64) -> bool {
    state.improvement_items(line_id, epoch).into_iter().filter(|item| !item.dropped).any(|item| owes_reveal_v1(state, line_id, item))
}

fn owes_reveal_v1(state: &PalwChainStateV2, line_id: &Hash64, item: &PalwEvalItemV1) -> bool {
    // A drawn item is owed a reveal only by a row that exists: every hold-out entry and setter set this
    // lane admits has one until its epoch retires (a set that forfeits leaves with its items dropped).
    match item.source {
        PalwItemSourceV1::Setter { set_id, .. } => {
            state.improvement_setter_set(line_id, &set_id).is_some_and(|set| set.prompts.is_none() || set.keys.is_none())
        }
        PalwItemSourceV1::HoldOut => state
            .improvement_case(line_id, &item.case_id)
            .is_some_and(|case| !matches!(case.case.reference, PalwCaseReferenceV1::None) && case.revealed.is_none()),
        PalwItemSourceV1::Regression { .. } | PalwItemSourceV1::Safety { .. } => false,
    }
}

/// **Right before scoring** (spec 17 §17.9.1): every drawn item that still owes a reveal is dropped for
/// every subject, and every setter set that owed one forfeits its bond and leaves.
pub(super) fn palw_improve_material_before_scoring_v1(
    builder: &mut TransitionBuilder<'_>,
    line_id: &Hash64,
    epoch: u64,
) -> Result<(), PalwStateV2Error> {
    let owing: Vec<PalwEvalItemV1> = builder
        .state
        .improvement_items(line_id, epoch)
        .into_iter()
        .filter(|item| !item.dropped && owes_reveal_v1(&builder.state, line_id, item))
        .copied()
        .collect();
    let mut forfeiting: Vec<Hash64> = Vec::new();
    for item in owing {
        builder.drop_improvement_item_v1(line_id, epoch, item.item)?;
        if let PalwItemSourceV1::Setter { set_id, .. } = item.source
            && !forfeiting.contains(&set_id)
        {
            forfeiting.push(set_id);
        }
    }
    for set_id in forfeiting {
        if let Some(set) = builder.state.improvement_setter_set(line_id, &set_id).cloned() {
            builder.forfeit_improvement_hold_v1(line_id, set.bond)?;
            builder.write_improvement_setter_set((*line_id, set_id), None);
        }
    }
    Ok(())
}

// ---- the step-2 sweep ---------------------------------------------------------------------------

/// **Is the line leaving governance** — dissolved, or opting out past its effective DAA with no epoch
/// open — so that everything it holds is settled?
fn line_closing_v1(line: &PalwImprovementLineV1, daa: u64) -> bool {
    match line.status {
        PalwImprovementLineStatusV1::Dissolved => true,
        PalwImprovementLineStatusV1::OptingOut { effective_daa: Some(at) } => daa >= at && line.open_epoch.is_none(),
        _ => false,
    }
}

/// **The material lane's step-2 sweep** (after the core's `advance_improvement_v1`, in both step-2
/// sites), at most [`PALW_IMPROVE_MATERIAL_SWEEP_ROWS_PER_BLOCK_V1`] rows a block, oldest first:
/// 1. every artifact commitment past its reveal deadline forfeits its bond and leaves;
/// 2. every opt-in past its claim's retention, and every licence past its expiry, leaves;
/// 3. the material of every decided epoch retires — its cases leave; its setter sets and honest
///    artifacts are refunded their bonds and leave — and so does everything a line leaving governance
///    holds, datasets included (whose bonds are held while they stand).
pub(super) fn settle_improvement_material_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if !builder.params.improve_active_at(daa) || !builder.state.has_improvement_material_rows_v1() {
        return Ok(());
    }
    let mut budget = PALW_IMPROVE_MATERIAL_SWEEP_ROWS_PER_BLOCK_V1;
    while budget > 0 {
        let Some(&(due, line_id, commit)) = builder.state.improvement_artifact_deadlines.first() else { break };
        if due > daa {
            break;
        }
        let row = builder.state.improvement_artifacts.get(&(line_id, commit)).cloned().expect("the deadline index follows the rows");
        if builder.state.improvement_governed_at(&line_id, daa) {
            builder.forfeit_improvement_hold_v1(&line_id, row.bond)?;
        } else {
            builder.release_improvement_hold_v1(&line_id, &row.teacher, row.bond)?;
        }
        builder.write_improvement_artifact((line_id, commit), None);
        budget -= 1;
    }
    while budget > 0 {
        let Some(&(due, pin)) = builder.state.improvement_opt_in_expiries.first() else { break };
        if due > daa {
            break;
        }
        builder.write_improvement_opt_in(pin, None);
        budget -= 1;
    }
    while budget > 0 {
        let Some(&(due, id)) = builder.state.improvement_licence_expiries.first() else { break };
        if due > daa {
            break;
        }
        builder.write_improvement_licence(id, None);
        budget -= 1;
    }
    let lines: Vec<PalwImprovementLineV1> = builder.state.improvement_lines.values().cloned().collect();
    for line in lines {
        if budget == 0 {
            break;
        }
        let closing = line_closing_v1(&line, daa);
        let lo = (line.line_id, 0u64, 0u8, zero());
        let hi = (line.line_id, u64::MAX, u8::MAX, Hash64::from_bytes([0xFF; 64]));
        let retiring: Vec<(Hash64, u64, u8, Hash64)> = builder
            .state
            .improvement_material_epochs
            .range(lo..=hi)
            .take_while(|(_, epoch, _, _)| closing || (*epoch < line.next_epoch && line.open_epoch != Some(*epoch)))
            .take(budget)
            .copied()
            .collect();
        for (line_id, _, table, id) in retiring {
            retire_material_row_v1(builder, &line_id, table, &id)?;
            budget -= 1;
        }
        if closing && budget > 0 {
            let unrevealed: Vec<(Hash64, PalwTeachingArtifactRecordV1)> = builder
                .state
                .improvement_artifacts
                .range((line.line_id, zero())..=(line.line_id, Hash64::from_bytes([0xFF; 64])))
                .filter(|(_, row)| row.revealed.is_none())
                .take(budget)
                .map(|((_, c), row)| (*c, row.clone()))
                .collect();
            for (commit, row) in unrevealed {
                builder.release_improvement_hold_v1(&line.line_id, &row.teacher, row.bond)?;
                builder.write_improvement_artifact((line.line_id, commit), None);
                budget -= 1;
            }
            let datasets: Vec<(Hash64, PalwDatasetRecordV1)> = builder
                .state
                .improvement_datasets
                .range((line.line_id, zero())..=(line.line_id, Hash64::from_bytes([0xFF; 64])))
                .take(budget)
                .map(|((_, id), row)| (*id, row.clone()))
                .collect();
            for (dataset_id, row) in datasets {
                builder.release_improvement_hold_v1(&line.line_id, &row.registrant, row.bond)?;
                builder.write_improvement_dataset((line.line_id, dataset_id), None);
                budget -= 1;
            }
        }
    }
    Ok(())
}

/// Retire one row of a decided epoch's material (or of a line leaving governance): a case leaves; a
/// setter set and an honest artifact are refunded their bonds and leave.
fn retire_material_row_v1(
    builder: &mut TransitionBuilder<'_>,
    line_id: &Hash64,
    table: u8,
    id: &Hash64,
) -> Result<(), PalwStateV2Error> {
    match table {
        MATERIAL_CASE => builder.write_improvement_case((*line_id, *id), None),
        MATERIAL_SETTER_SET => {
            if let Some(set) = builder.state.improvement_setter_set(line_id, id).cloned() {
                builder.release_improvement_hold_v1(line_id, &set.setter, set.bond)?;
            }
            builder.write_improvement_setter_set((*line_id, *id), None);
        }
        MATERIAL_ARTIFACT => {
            if let Some(row) = builder.state.improvement_artifact(line_id, id).cloned() {
                builder.release_improvement_hold_v1(line_id, &row.teacher, row.bond)?;
            }
            builder.write_improvement_artifact((*line_id, *id), None);
        }
        _ => unreachable!("the retirement index names only the three tables"),
    }
    Ok(())
}

// ---- the composite gate (decision 7a) ------------------------------------------------------------

/// **The fold's lock of decision 7a**: an object carrying a composite opening names only classes the
/// candidate path admitted in composite form, each with its recorded reference.
pub(super) fn composite_openings_admitted_v1(
    state: &PalwChainStateV2,
    object: &PalwConsensusObjectV2,
) -> Result<(), PalwStateV2Error> {
    crate::palw_improve_composite_v1::palw_composite_openings_admitted_v1(object, |class| state.improvement_composite_class(class))
        .map_err(|why| PalwStateV2Error::ImprovementObjectRefused { object: "an IR close with composite openings", why })
}

// ---- the one writers of the material lane's tables (spec 17 §17.0: delta 93–99) ------------------

impl TransitionBuilder<'_> {
    /// **The one writer of `improvement_cases`**, journaled `ImprovementCase` (93).
    pub(crate) fn write_improvement_case(&mut self, key: (Hash64, Hash64), new: Option<PalwHardCaseRecordV1>) {
        let old = match new.clone() {
            Some(row) => self.state.improvement_cases.insert(key, row),
            None => self.state.improvement_cases.remove(&key),
        };
        if old == new {
            return;
        }
        if let Some(old) = &old {
            self.state.improvement_material_epochs.remove(&material_epoch_of_case_v1(&key, old));
        }
        if let Some(new) = &new {
            self.state.improvement_material_epochs.insert(material_epoch_of_case_v1(&key, new));
        }
        self.entries.push(PalwDeltaEntryV2::ImprovementCase { key, old: old.map(Box::new), new: new.map(Box::new) });
    }

    /// **The one writer of `improvement_opt_ins`**, journaled `ImprovementOptIn` (94).
    pub(crate) fn write_improvement_opt_in(&mut self, key: Hash64, new: Option<PalwDataUseOptInRecordV1>) {
        let old = match new.clone() {
            Some(row) => self.state.improvement_opt_ins.insert(key, row),
            None => self.state.improvement_opt_ins.remove(&key),
        };
        if old == new {
            return;
        }
        if let Some(old) = &old {
            self.state.improvement_opt_in_expiries.remove(&(old.expires_daa, key));
        }
        if let Some(new) = &new {
            self.state.improvement_opt_in_expiries.insert((new.expires_daa, key));
        }
        self.entries.push(PalwDeltaEntryV2::ImprovementOptIn { key, old: old.map(Box::new), new: new.map(Box::new) });
    }

    /// **The one writer of `improvement_setter_sets`**, journaled `ImprovementSetterSet` (95).
    pub(crate) fn write_improvement_setter_set(&mut self, key: (Hash64, Hash64), new: Option<PalwSetterSetRecordV1>) {
        let old = match new.clone() {
            Some(row) => self.state.improvement_setter_sets.insert(key, row),
            None => self.state.improvement_setter_sets.remove(&key),
        };
        if old == new {
            return;
        }
        if let Some(old) = &old {
            self.state.improvement_material_epochs.remove(&material_epoch_of_set_v1(&key, old));
        }
        if let Some(new) = &new {
            self.state.improvement_material_epochs.insert(material_epoch_of_set_v1(&key, new));
        }
        self.entries.push(PalwDeltaEntryV2::ImprovementSetterSet { key, old: old.map(Box::new), new: new.map(Box::new) });
    }

    /// **The one writer of `improvement_datasets`**, journaled `ImprovementDataset` (96).
    pub(crate) fn write_improvement_dataset(&mut self, key: (Hash64, Hash64), new: Option<PalwDatasetRecordV1>) {
        let old = match new.clone() {
            Some(row) => self.state.improvement_datasets.insert(key, row),
            None => self.state.improvement_datasets.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ImprovementDataset { key, old: old.map(Box::new), new: new.map(Box::new) });
        }
    }

    /// **The one writer of `improvement_artifacts`**, journaled `ImprovementArtifact` (97). Keeps the
    /// output, answer, deadline and retirement indices in step with the row.
    pub(crate) fn write_improvement_artifact(&mut self, key: (Hash64, Hash64), new: Option<PalwTeachingArtifactRecordV1>) {
        let old = match new.clone() {
            Some(row) => self.state.improvement_artifacts.insert(key, row),
            None => self.state.improvement_artifacts.remove(&key),
        };
        if old == new {
            return;
        }
        let s = &mut self.state;
        if let Some(old) = &old {
            match &old.revealed {
                Some(a) => {
                    s.improvement_artifact_outputs.remove(&(key.0, a.output_hash, old.committed_daa, key.1));
                    s.improvement_artifact_answers.remove(&(key.0, a.task_id, old.committed_daa, key.1));
                }
                None => {
                    s.improvement_artifact_deadlines.remove(&(old.reveal_by_daa, key.0, key.1));
                }
            }
            if let Some(entry) = material_epoch_of_artifact_v1(&key, old) {
                s.improvement_material_epochs.remove(&entry);
            }
        }
        if let Some(new) = &new {
            match &new.revealed {
                Some(a) => {
                    s.improvement_artifact_outputs.insert((key.0, a.output_hash, new.committed_daa, key.1));
                    if a.kind == PalwTeachingArtifactKindV1::Answer {
                        s.improvement_artifact_answers.insert((key.0, a.task_id, new.committed_daa, key.1));
                    }
                }
                None => {
                    s.improvement_artifact_deadlines.insert((new.reveal_by_daa, key.0, key.1));
                }
            }
            if let Some(entry) = material_epoch_of_artifact_v1(&key, new) {
                s.improvement_material_epochs.insert(entry);
            }
        }
        self.entries.push(PalwDeltaEntryV2::ImprovementArtifact { key, old: old.map(Box::new), new: new.map(Box::new) });
    }

    /// **The one writer of `improvement_licences`**, journaled `ImprovementLicence` (98).
    pub(crate) fn write_improvement_licence(&mut self, key: Hash64, new: Option<PalwTeacherLicenceRecordV1>) {
        let old = match new.clone() {
            Some(row) => self.state.improvement_licences.insert(key, row),
            None => self.state.improvement_licences.remove(&key),
        };
        if old == new {
            return;
        }
        if let Some(old) = &old {
            self.state.improvement_licence_expiries.remove(&(old.licence.expiry_daa, key));
        }
        if let Some(new) = &new {
            self.state.improvement_licence_expiries.insert((new.licence.expiry_daa, key));
        }
        self.entries.push(PalwDeltaEntryV2::ImprovementLicence { key, old: old.map(Box::new), new: new.map(Box::new) });
    }

    /// **The one writer of `improvement_composite_classes`**, journaled `ImprovementCompositeClass` (99).
    pub(crate) fn write_improvement_composite_class(&mut self, key: Hash64, new: Option<PalwTirCompositeRefV1>) {
        let old = match new {
            Some(row) => self.state.improvement_composite_classes.insert(key, row),
            None => self.state.improvement_composite_classes.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ImprovementCompositeClass { key, old, new });
        }
    }
}

// ---- the carriage's check of the material tables ----------------------------------------------

/// **The material tables' consistency** (spec 17 §17.0): every row under its own key, every line-bound
/// row under a line the chain governs (or governed), and every revealed artifact opening its own key.
pub(super) fn palw_improvement_material_carriage_consistent_v1(c: &PalwStateCarriageV2) -> Result<(), String> {
    let line = |id: &Hash64, what: &str| -> Result<(), String> {
        if c.improvement_lines.contains_key(id) { Ok(()) } else { Err(format!("{what} of {id}: no governed line holds it")) }
    };
    for ((line_id, case_id), row) in &c.improvement_cases {
        if (row.case.line_id, row.case.case_id) != (*line_id, *case_id) {
            return Err(format!("improvement case {line_id}/{case_id}: a row under another key"));
        }
        line(line_id, "a hard case")?;
    }
    for ((line_id, set_id), row) in &c.improvement_setter_sets {
        if (row.commitment.line_id, row.commitment.set_id) != (*line_id, *set_id) {
            return Err(format!("setter set {line_id}/{set_id}: a row under another key"));
        }
        line(line_id, "a setter set")?;
    }
    for ((line_id, dataset_id), row) in &c.improvement_datasets {
        if (row.dataset.line_id, row.dataset.dataset_id) != (*line_id, *dataset_id) {
            return Err(format!("dataset {line_id}/{dataset_id}: a row under another key"));
        }
        line(line_id, "a dataset")?;
    }
    for ((line_id, commit), row) in &c.improvement_artifacts {
        if (row.commit.line_id, row.commit.commit) != (*line_id, *commit) {
            return Err(format!("teaching artifact {line_id}/{commit}: a row under another key"));
        }
        if row.revealed.as_ref().is_some_and(|a| a.line_id != *line_id || palw_teaching_artifact_commit_v1(a) != *commit) {
            return Err(format!("teaching artifact {line_id}/{commit}: its reveal does not open its commitment"));
        }
        line(line_id, "a teaching artifact")?;
    }
    for (licence_id, row) in &c.improvement_licences {
        if row.licence.licence_id != *licence_id {
            return Err(format!("teacher licence {licence_id}: a row under another id"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::palw_improve_fold_v1::{advance_improvement_v1, apply_improvement_policy_set_v1};
    use super::*;
    use crate::palw_improve_artifact_v1::PalwTirArtifactRefV1;
    use crate::palw_improve_policy_v1::palw_improvement_policy_example_v1;
    use crate::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1;
    use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
    use crate::palw_tir_admission_v1::PalwTirClassRecordV1;
    use crate::tx::TransactionOutpoint;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn bond(byte: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: h(byte), index: 0 })
    }

    const OWNER: u8 = 1;
    const ALICE: u8 = 2;
    const BOB: u8 = 3;
    const CAROL: u8 = 4;
    const LINE: u8 = 0x10;
    const CAND_A: u8 = 0x11;
    const CAND_B: u8 = 0x12;
    const ACTIVE: u64 = 100;

    fn params() -> PalwStateParamsV2 {
        let p = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
        bundle
            .state
            .clone()
            .with_improve_from_daa(Some(ACTIVE))
            .with_improve_ceilings(Some(PALW_DRILL_IMPROVE_CEILINGS_V1))
            // The fold's mechanics, not Λ: the release arms testnet-12's fast-path check (rows 19-20), which its own test asks with an
            // explicit base (a_policys_windows_are_held_to_the_fast_honest_claim_path_at_the_daa_it_is_applied).
            .with_improve_lifecycle_base(None)
    }

    /// The core's small test policy (8 items, n_min 4, one ExactMatch stage with `key_cap` 64, a
    /// 1,000-DAA grid), with the licensed class allowed.
    fn policy() -> PalwImprovementPolicyV1 {
        let mut p = palw_improvement_policy_example_v1();
        p.eval.n = 8;
        p.eval.n_min = 4;
        p.eval.regression_items = 0;
        p.eval.regression_dataset = Hash64::default();
        p.eval.safety_items = 0;
        p.eval.safety_dataset = Hash64::default();
        p.eval.stages.truncate(1);
        p.eval.setter_cap_permille = 1_000;
        p.usage.value = 2;
        p.k_max = 2;
        p.provenance.teacher_classes |= PalwTeacherClassV1::LicensedDistill.bit();
        p
    }

    fn allowed_licence() -> Hash64 {
        policy().provenance.licence_classes[0]
    }

    fn genesis() -> PalwChainStateV2 {
        let mut s = PalwChainStateV2::genesis();
        for class in [LINE, CAND_A, CAND_B] {
            s.tir_classes.insert(h(class), PalwTirClassRecordV1::test_row_v1(h(class)));
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

    /// Run `f` on a builder over `state` at `daa`, after that block's step 2 (the core's sweep, then
    /// this lane's), and return the state.
    fn at(state: &PalwChainStateV2, p: &PalwStateParamsV2, daa: u64, f: impl FnOnce(&mut TransitionBuilder<'_>)) -> PalwChainStateV2 {
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(state, p, false, false, false, false, &extras);
        advance_improvement_v1(&mut builder, &ctx(daa)).expect("the core's sweep");
        settle_improvement_material_v1(&mut builder, &ctx(daa)).expect("the material sweep");
        f(&mut builder);
        builder.checkpoint().0
    }

    fn opted_in(daa: u64) -> PalwChainStateV2 {
        let p = params();
        let set = PalwImprovementPolicySetV1 { line_id: h(LINE), sequence: 1, policy: Some(policy()) };
        at(&genesis(), &p, daa, |b| apply_improvement_policy_set_v1(b, &ctx(daa), &set).expect("opt in"))
    }

    /// The pool's conservation (spec 17 §17.11.5).
    fn conserved(s: &PalwChainStateV2) {
        let pool = s.improvement_pool(&h(LINE)).unwrap();
        let inflow = pool.deposited + pool.fees_in + pool.phi_in + pool.held_in;
        let stock = pool.balance as u128 + pool.held as u128 + pool.unvested as u128 + pool.paid + pool.refunded;
        assert_eq!(inflow, stock, "the pool conserves: {pool:?}");
    }

    fn case(prompt_ids: Vec<u32>, reference: PalwCaseReferenceV1, source: PalwCaseSourceV1) -> PalwHardCaseV1 {
        let (line_id, domain) = (h(LINE), 3);
        PalwHardCaseV1 {
            line_id,
            case_id: palw_hard_case_id_v1(&line_id, domain, &prompt_ids, &reference),
            domain,
            prompt_ids,
            reference,
            source,
            head_evidence: None,
        }
    }

    fn exact(key: &[u32], salt: u8) -> PalwCaseReferenceV1 {
        PalwCaseReferenceV1::ExactKey { commitment: palw_case_key_commitment_v1(&h(LINE), key, &h(salt)) }
    }

    fn try_at(
        state: &PalwChainStateV2,
        daa: u64,
        f: impl FnOnce(&mut TransitionBuilder<'_>, &PalwBlockContextV2) -> Result<(), PalwStateV2Error>,
    ) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let p = params();
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(state, &p, false, false, false, false, &extras);
        advance_improvement_v1(&mut builder, &ctx(daa))?;
        settle_improvement_material_v1(&mut builder, &ctx(daa))?;
        f(&mut builder, &ctx(daa))?;
        Ok(builder.checkpoint().0)
    }

    #[test]
    fn a_hard_case_pays_its_fee_is_placed_and_is_one_case() {
        let s = opted_in(500);
        let c = case(vec![1, 2, 3], exact(&[5], 0x51), PalwCaseSourceV1::Setter);
        let s = try_at(&s, 600, |b, x| apply_hard_case_v1(b, x, &c, &bond(ALICE))).expect("admitted");
        let row = s.improvement_case(&h(LINE), &c.case_id).expect("kept");
        assert_eq!((row.epoch, row.holdout, row.submitter), (1, false, bond(ALICE)), "an idle line: the next epoch's material");
        assert_eq!(s.improvement_pool(&h(LINE)).unwrap().fees_in, policy().fees.hard_case_fee as u128);
        assert_eq!(s.improvement_material(&h(LINE), 1).unwrap().count, 1);
        conserved(&s);
        for (why, bad, who) in [
            ("the same case again", c.clone(), BOB),
            ("an id past the head's token bound", case(vec![1, 8], exact(&[5], 0x51), PalwCaseSourceV1::Setter), BOB),
            (
                "an artifact case of no artifact",
                case(vec![4], exact(&[5], 0x51), PalwCaseSourceV1::Artifact { artifact_id: h(0x77) }),
                BOB,
            ),
            ("a usage case of no opt-in", case(vec![4], exact(&[5], 0x51), PalwCaseSourceV1::UsageOptIn { job_pin: h(0x78) }), BOB),
            ("a malformed case", PalwHardCaseV1 { case_id: h(9), ..c.clone() }, BOB),
        ] {
            assert!(try_at(&s, 610, |b, x| apply_hard_case_v1(b, x, &bad, &bond(who))).is_err(), "{why}");
        }
        let evidence = PalwHardCaseV1 { head_evidence: Some(h(0x79)), ..case(vec![6], exact(&[5], 0x51), PalwCaseSourceV1::Setter) };
        assert!(try_at(&s, 610, |b, x| apply_hard_case_v1(b, x, &evidence, &bond(BOB))).is_err(), "evidence naming no claim");
        let other = PalwHardCaseV1 { line_id: h(0x20), ..c.clone() };
        assert!(try_at(&s, 610, |b, x| apply_hard_case_v1(b, x, &other, &bond(BOB))).is_err(), "a line that is not governed");
    }

    /// A free-prompt claim of BOB's whose recorded job is `pin`.
    fn with_fp_claim(mut s: PalwChainStateV2, claim_id: Hash64, pin: Hash64) -> PalwChainStateV2 {
        let mut claim = palw_claim_template_v1(h(LINE), bond(BOB), 550, 0, 0);
        claim.source = PalwClaimSourceV2::FreePrompt { quanta: 1, spent: BTreeSet::new() };
        claim.job_identity = pin;
        claim.trace_retention_daa = 5_000;
        s.claims.insert(claim_id, claim);
        s
    }

    fn facts(ids: &[u32]) -> PalwFpJobFactsV1 {
        PalwFpJobFactsV1 {
            job_id: h(0x61),
            execution_seed: [9; 32],
            tokenizer_id: h(LINE),
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, ids).unwrap(),
            prompt_tokens: ids.len() as u32,
            decode_tokens_executed: 4,
            max_context_tokens: 64,
        }
    }

    #[test]
    fn a_usage_case_is_the_opted_in_jobs_committed_prompt() {
        let ids = vec![7, 1, 7];
        let job = facts(&ids);
        let (claim, pin) = (h(0x62), job.pin());
        let s = with_fp_claim(opted_in(500), claim, pin);
        let opt_in = PalwDataUseOptInV1 { job_pin: pin, claim, job };
        let s = try_at(&s, 600, |b, x| apply_data_use_opt_in_v1(b, x, &opt_in)).expect("the committer's opt-in");
        let row = s.improvement_opt_in(&pin).expect("kept").clone();
        assert_eq!((row.committer, row.prompt_ids_merkle, row.expires_daa), (bond(BOB), false, 5_000));
        assert!(try_at(&s, 601, |b, x| apply_data_use_opt_in_v1(b, x, &opt_in)).is_err(), "once");
        let other_job = PalwFpJobFactsV1 { decode_tokens_executed: 5, ..job };
        let not_the_claims = PalwDataUseOptInV1 { job_pin: other_job.pin(), claim, job: other_job };
        let s2 = with_fp_claim(opted_in(500), claim, pin);
        assert!(try_at(&s2, 600, |b, x| apply_data_use_opt_in_v1(b, x, &not_the_claims)).is_err(), "another job than the claim's");
        // The case: exactly the committed prompt, under the line's tokenizer.
        let usage = |prompt: Vec<u32>| case(prompt, PalwCaseReferenceV1::None, PalwCaseSourceV1::UsageOptIn { job_pin: pin });
        assert!(try_at(&s, 610, |b, x| apply_hard_case_v1(b, x, &usage(vec![7, 1, 6]), &bond(ALICE))).is_err(), "another prompt");
        assert!(try_at(&s, 610, |b, x| apply_hard_case_v1(b, x, &usage(vec![7, 1]), &bond(ALICE))).is_err(), "a prefix");
        let s = try_at(&s, 610, |b, x| apply_hard_case_v1(b, x, &usage(ids.clone()), &bond(ALICE))).expect("the job's own prompt");
        assert!(s.improvement_case(&h(LINE), &usage(ids).case_id).is_some());
        // The opt-in leaves with its claim's retention.
        let s = at(&s, &params(), 5_000, |_| {});
        assert!(s.improvement_opt_in(&pin).is_none(), "expired with the claim's retention");
    }

    fn dataset(classes: Vec<Hash64>, teachers: u8) -> PalwDatasetV1 {
        let mut d = PalwDatasetV1 {
            line_id: h(LINE),
            dataset_id: Hash64::default(),
            content_root: h(0x30),
            items: 10,
            license_classes: classes,
            teacher_classes: teachers,
            provenance_commitment: h(0x31),
        };
        d.license_classes.sort();
        d.dataset_id = palw_dataset_id_v1(&d);
        d
    }

    fn licence(expiry_daa: u64) -> PalwTeacherLicenceV1 {
        let mut l = PalwTeacherLicenceV1 {
            licence_id: Hash64::default(),
            rights_holder_key: vec![3; PALW_IMPROVE_MLDSA87_PUBKEY_BYTES_V1],
            model_family: h(0x41),
            domains: vec![3],
            uses: PALW_IMPROVE_LICENCE_USE_TRAINING_V1,
            per_use_fee: 1,
            expiry_daa,
        };
        l.licence_id = palw_teacher_licence_id_v1(&l);
        l
    }

    #[test]
    fn datasets_hold_their_bond_under_the_policys_classes_and_licences() {
        let s = opted_in(500);
        let public = dataset(vec![allowed_licence()], PalwTeacherClassV1::PublicData.bit());
        let s = try_at(&s, 600, |b, x| apply_dataset_registered_v1(b, x, &public, &bond(CAROL))).expect("registered");
        let pool = s.improvement_pool(&h(LINE)).unwrap();
        assert_eq!(pool.held, policy().fees.dataset_bond, "the bond is held");
        assert_eq!(s.improvement_dataset(&h(LINE), &public.dataset_id).unwrap().registrant, bond(CAROL));
        conserved(&s);
        let l = licence(10_000);
        for (why, bad) in [
            ("again", public.clone()),
            ("a licence class the policy does not allow", dataset(vec![h(0x99)], PalwTeacherClassV1::PublicData.bit())),
            ("a teacher class the policy does not allow", dataset(vec![allowed_licence()], PalwTeacherClassV1::Human.bit())),
            ("LICENSED_DISTILL with no registered licence", dataset(vec![l.licence_id], PalwTeacherClassV1::LicensedDistill.bit())),
        ] {
            assert!(try_at(&s, 610, |b, x| apply_dataset_registered_v1(b, x, &bad, &bond(CAROL))).is_err(), "{why}");
        }
        let s = try_at(&s, 610, |b, x| apply_teacher_licence_v1(b, x, &l)).expect("a licence");
        let licensed = dataset(vec![l.licence_id], PalwTeacherClassV1::LicensedDistill.bit());
        let s = try_at(&s, 620, |b, x| apply_dataset_registered_v1(b, x, &licensed, &bond(CAROL))).expect("covered by the licence");
        assert!(s.improvement_dataset(&h(LINE), &licensed.dataset_id).is_some());
        assert!(try_at(&s, 630, |b, x| apply_teacher_licence_v1(b, x, &l)).is_err(), "a licence once");
        assert!(try_at(&s, 630, |b, x| apply_teacher_licence_v1(b, x, &licence(630))).is_err(), "already expired");
        let s = at(&s, &params(), 10_000, |_| {});
        assert!(s.improvement_licence(&l.licence_id).is_none(), "a licence leaves at its expiry");
    }

    fn artifact(kind: PalwTeachingArtifactKindV1, task: Hash64, output: u8, salt: u8) -> PalwTeachingArtifactV1 {
        PalwTeachingArtifactV1 {
            line_id: h(LINE),
            kind,
            task_id: task,
            teacher_type: PalwTeacherClassV1::OpenDistill,
            teacher_id: h(0x42),
            license_class: allowed_licence(),
            provenance_commitment: h(0x43),
            output_hash: h(output),
            verification_type: PalwVerificationTypeV1::Exact,
            answer_span: if kind == PalwTeachingArtifactKindV1::Answer { vec![5] } else { Vec::new() },
            salt: h(salt),
        }
    }

    fn commit_of(a: &PalwTeachingArtifactV1) -> PalwTeachingArtifactCommitV1 {
        PalwTeachingArtifactCommitV1 { line_id: a.line_id, commit: palw_teaching_artifact_commit_v1(a) }
    }

    #[test]
    fn an_artifact_is_committed_revealed_and_judged_in_commit_order() {
        use PalwTeachingArtifactKindV1 as K;
        let bond_amount = policy().fees.artifact_bond as u128;
        let s = opted_in(500);
        // Four commitments: two of one output (ALICE first, BOB second), CAROL's spam and OWNER's
        // never revealed.
        let first = artifact(K::Answer, h(0x90), 0xA0, 1);
        let copy = artifact(K::Answer, h(0x90), 0xA0, 2);
        let spam =
            PalwTeachingArtifactV1 { verification_type: PalwVerificationTypeV1::Judged, ..artifact(K::Answer, h(0x90), 0xA1, 3) };
        let silent = artifact(K::PreferencePair, h(0x90), 0xA2, 4);
        let s = try_at(&s, 600, |b, x| apply_artifact_committed_v1(b, x, &commit_of(&first), &bond(ALICE))).unwrap();
        let s = try_at(&s, 601, |b, x| {
            apply_artifact_committed_v1(b, x, &commit_of(&copy), &bond(BOB))?;
            apply_artifact_committed_v1(b, x, &commit_of(&spam), &bond(CAROL))?;
            apply_artifact_committed_v1(b, x, &commit_of(&silent), &bond(OWNER))
        })
        .unwrap();
        assert_eq!(s.improvement_pool(&h(LINE)).unwrap().held as u128, 4 * bond_amount);
        assert!(
            try_at(&s, 602, |b, x| apply_artifact_committed_v1(b, x, &commit_of(&first), &bond(BOB))).is_err(),
            "a commitment once"
        );
        // The copy reveals first, then the first commitment: the copy becomes the duplicate.
        let s = try_at(&s, 610, |b, x| apply_artifact_revealed_v1(b, x, &copy)).unwrap();
        assert_eq!(s.improvement_artifact_original(&h(LINE), &h(0xA0)), Some(commit_of(&copy).commit));
        let s = try_at(&s, 611, |b, x| apply_artifact_revealed_v1(b, x, &first)).unwrap();
        assert_eq!(s.improvement_artifact_original(&h(LINE), &h(0xA0)), Some(commit_of(&first).commit), "the earliest commit");
        assert!(s.improvement_artifact(&h(LINE), &commit_of(&copy).commit).is_none(), "the later duplicate forfeited and left");
        // Spam forfeits and leaves; the object stands.
        let s = try_at(&s, 612, |b, x| apply_artifact_revealed_v1(b, x, &spam)).unwrap();
        assert!(s.improvement_artifact(&h(LINE), &commit_of(&spam).commit).is_none());
        let pool = s.improvement_pool(&h(LINE)).unwrap();
        assert_eq!((pool.held as u128, pool.forfeited_in), (2 * bond_amount, 2 * bond_amount), "the copy's and the spam's bonds");
        assert!(try_at(&s, 613, |b, x| apply_artifact_revealed_v1(b, x, &first)).is_err(), "a reveal once");
        let unknown = artifact(K::Answer, h(0x90), 0xA3, 9);
        assert!(try_at(&s, 613, |b, x| apply_artifact_revealed_v1(b, x, &unknown)).is_err(), "a reveal of no commitment");
        // The silent one forfeits at its deadline (commit + w_collect); the honest one stays held.
        let deadline = 601 + policy().windows.w_collect;
        let s = at(&s, &params(), deadline, |_| {});
        assert!(s.improvement_artifact(&h(LINE), &commit_of(&silent).commit).is_none(), "unrevealed past its deadline");
        let pool = s.improvement_pool(&h(LINE)).unwrap();
        assert_eq!((pool.held as u128, pool.forfeited_in), (bond_amount, 3 * bond_amount));
        conserved(&s);
    }

    #[test]
    fn an_answer_to_a_training_case_earns_the_bounty_at_its_keys_reveal() {
        use PalwTeachingArtifactKindV1 as K;
        let key = vec![5u32];
        let c = case(vec![1, 2], exact(&key, 0x51), PalwCaseSourceV1::Setter);
        let answer = artifact(K::Answer, c.case_id, 0xB0, 1);
        let wrong = PalwTeachingArtifactV1 { answer_span: vec![6], ..artifact(K::Answer, c.case_id, 0xB1, 2) };
        let mut s = opted_in(500);
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        let mut s = try_at(&s, 600, |b, x| apply_hard_case_v1(b, x, &c, &bond(OWNER))).unwrap();
        // A sponsor's deposit before the opening gives the period an S1 budget of 10% of the balance.
        let pool = s.improvement_pools.get_mut(&h(LINE)).unwrap();
        pool.balance += 1_000_000_000;
        pool.deposited += 1_000_000_000;
        let s = at(&s, &params(), 1_000, |_| {});
        assert!(s.improvement_pool(&h(LINE)).unwrap().s1_budget > 0, "the period's S1 budget");
        let s = try_at(&s, 1_010, |b, x| {
            apply_artifact_committed_v1(b, x, &commit_of(&wrong), &bond(BOB))?;
            apply_artifact_committed_v1(b, x, &commit_of(&answer), &bond(ALICE))
        })
        .unwrap();
        let s = try_at(&s, 1_020, |b, x| {
            apply_artifact_revealed_v1(b, x, &answer)?;
            apply_artifact_revealed_v1(b, x, &wrong)
        })
        .unwrap();
        let reveal = PalwCaseKeyRevealV1 { line_id: h(LINE), case_id: c.case_id, key: key.clone(), salt: h(0x51) };
        let bad = PalwCaseKeyRevealV1 { salt: h(0x52), ..reveal.clone() };
        assert!(try_at(&s, 1_030, |b, x| apply_case_key_revealed_v1(b, x, &bad)).is_err(), "another salt");
        // …taken through the object's own arm (tag 86: past the fence, its admission landed).
        let object = PalwConsensusObjectV2::HardCaseKeyRevealed { payload: Box::new(reveal.clone()) };
        let s = try_at(&s, 1_030, |b, x| apply_object(b, x, &object)).unwrap();
        assert_eq!(s.improvement_case(&h(LINE), &c.case_id).unwrap().revealed, Some(key));
        assert_eq!(s.improvement_earnings(&bond(ALICE)), policy().fees.s1_bounty, "the matching Answer's teacher");
        assert_eq!(s.improvement_earnings(&bond(BOB)), 0, "a wrong answer earns nothing");
        assert!(try_at(&s, 1_031, |b, x| apply_case_key_revealed_v1(b, x, &reveal)).is_err(), "a key once");
        conserved(&s);
    }

    /// Drive an epoch to its draw with one setter set (two items) and two hold-out cases.
    fn drawn_epoch() -> (PalwChainStateV2, PalwSetterSetCommitmentV1, Vec<Vec<u32>>, Vec<Vec<u32>>, PalwHardCaseV1) {
        let (prompts, keys) = (vec![vec![1], vec![2, 3]], vec![vec![5], vec![]]);
        let mut set = PalwSetterSetCommitmentV1 {
            line_id: h(LINE),
            epoch: 1,
            set_id: Hash64::default(),
            items: 2,
            prompts_commitment: palw_setter_prompts_commitment_v1(&h(LINE), 1, &prompts, &h(0x5A)),
            keys_commitment: palw_setter_keys_commitment_v1(&h(LINE), 1, &keys, &h(0x5B)),
        };
        set.set_id = palw_setter_set_id_v1(&set);
        let holdout = case(vec![4], exact(&[6], 0x53), PalwCaseSourceV1::Setter);
        let mut s = opted_in(500);
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        let s = at(&s, &params(), 1_000, |_| {});
        let s = try_at(&s, 1_100, |b, x| apply_setter_set_committed_v1(b, x, &set, &bond(CAROL))).unwrap();
        assert!(try_at(&s, 1_101, |b, x| apply_setter_set_committed_v1(b, x, &set, &bond(CAROL))).is_err(), "a set once");
        let s = try_at(&s, 1_200, |b, x| {
            b.admit_improvement_candidate_v1(
                &h(LINE),
                1,
                &h(CAND_A),
                &bond(ALICE),
                PalwTirArtifactRefV1::Composite { parent_class: h(LINE), parent_root: h(0x21), adapter_root: h(0x22), p: 4 },
                h(0x50),
                Vec::new(),
                x.daa_score,
            )
            .map(|_| ())
        })
        .unwrap();
        let s = try_at(&s, 1_400, |b, x| {
            apply_hard_case_v1(b, x, &holdout, &bond(OWNER))?;
            apply_hard_case_v1(b, x, &case(vec![5], PalwCaseReferenceV1::None, PalwCaseSourceV1::Setter), &bond(BOB))
        })
        .unwrap();
        assert!(s.improvement_case(&h(LINE), &holdout.case_id).unwrap().holdout);
        let s = at(&s, &params(), 1_510, |_| {});
        let e = s.improvement_epoch(&h(LINE), 1).unwrap();
        assert_eq!((e.state, e.items), (PalwEpochStateV1::Evaluating, 4), "two setter items and two hold-out cases drawn");
        (s, set, prompts, keys, holdout)
    }

    #[test]
    fn a_setter_set_reveals_its_prompts_from_the_draw_and_its_keys_in_closing() {
        let (s, set, prompts, keys, _) = drawn_epoch();
        let reveal = PalwSetterSetRevealV1 { line_id: h(LINE), epoch: 1, set_id: set.set_id, prompts: prompts.clone(), salt: h(0x5A) };
        let keys_reveal = PalwSetterKeysRevealV1 { line_id: h(LINE), epoch: 1, set_id: set.set_id, keys, salt: h(0x5B) };
        assert!(try_at(&s, 1_520, |b, x| apply_setter_keys_revealed_v1(b, x, &keys_reveal)).is_err(), "keys before prompts");
        let past_bound = PalwSetterSetRevealV1 { prompts: vec![vec![9], vec![2, 3]], ..reveal.clone() };
        assert!(try_at(&s, 1_520, |b, x| apply_setter_set_revealed_v1(b, x, &past_bound)).is_err(), "not the committed prompts");
        let s = try_at(&s, 1_520, |b, x| apply_setter_set_revealed_v1(b, x, &reveal)).unwrap();
        assert_eq!(s.improvement_setter_prompt(&h(LINE), &set.set_id, 1), Some(&[2u32, 3][..]));
        assert!(try_at(&s, 1_521, |b, x| apply_setter_set_revealed_v1(b, x, &reveal)).is_err(), "prompts once");
        assert!(try_at(&s, 1_600, |b, x| apply_setter_keys_revealed_v1(b, x, &keys_reveal)).is_err(), "keys wait for Closing");
        let s = try_at(&s, 1_800, |b, x| apply_setter_keys_revealed_v1(b, x, &keys_reveal)).unwrap();
        assert_eq!(s.improvement_setter_key(&h(LINE), &set.set_id, 0), Some(&[5u32][..]));
        assert!(palw_improve_material_pending_v1(&s, &h(LINE), 1), "the hold-out case's key is still owed");
        // Scoring: the set revealed everything; the hold-out case never revealed its key and is dropped.
        let s = at(&s, &params(), 1_801, |b| palw_improve_material_before_scoring_v1(b, &h(LINE), 1).unwrap());
        let dropped: Vec<bool> = s.improvement_items(&h(LINE), 1).iter().map(|i| i.dropped).collect();
        assert_eq!(dropped.iter().filter(|d| **d).count(), 1, "only the hold-out case owing its key: {dropped:?}");
        assert!(!palw_improve_material_pending_v1(&s, &h(LINE), 1));
        assert!(s.improvement_setter_set(&h(LINE), &set.set_id).is_some(), "an honest set stays until its epoch retires");
        conserved(&s);
    }

    #[test]
    fn a_setter_that_never_reveals_is_dropped_and_forfeits_and_the_epochs_material_retires() {
        let (s, set, _, _, holdout) = drawn_epoch();
        let held_before = s.improvement_pool(&h(LINE)).unwrap().held;
        let s = at(&s, &params(), 1_800, |b| palw_improve_material_before_scoring_v1(b, &h(LINE), 1).unwrap());
        assert!(
            s.improvement_items(&h(LINE), 1).iter().filter(|i| matches!(i.source, PalwItemSourceV1::Setter { .. })).all(|i| i.dropped)
        );
        assert!(s.improvement_setter_set(&h(LINE), &set.set_id).is_none(), "forfeited and gone");
        assert_eq!(s.improvement_pool(&h(LINE)).unwrap().held, held_before - policy().fees.setter_bond);
        conserved(&s);
        // The epoch decides at t_score; its material retires on the sweeps after.
        let s = at(&s, &params(), 1_950, |_| {});
        assert!(s.improvement_epoch(&h(LINE), 1).unwrap().is_decided());
        let s = at(&s, &params(), 1_951, |_| {});
        assert!(s.improvement_case(&h(LINE), &holdout.case_id).is_none(), "the decided epoch's cases retired");
        conserved(&s);
    }

    #[test]
    fn a_composite_candidate_is_recorded_for_its_openings_and_its_references_are_checked() {
        let mut s = opted_in(500);
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        let s = at(&s, &params(), 1_000, |_| {});
        let public = dataset(vec![allowed_licence()], PalwTeacherClassV1::PublicData.bit());
        let s = try_at(&s, 1_010, |b, x| apply_dataset_registered_v1(b, x, &public, &bond(CAROL))).unwrap();
        let reference = PalwTirArtifactRefV1::Composite { parent_class: h(LINE), parent_root: h(0x21), adapter_root: h(0x22), p: 4 };
        let submission = |class: u8, artifact: PalwTirArtifactRefV1, datasets: Vec<(Hash64, u16)>, licences: Vec<Hash64>| {
            PalwCandidateSubmissionV1 {
                line_id: h(LINE),
                epoch: 1,
                class_id: h(class),
                artifact,
                layout: crate::palw_tir_class_v1::PalwTirLayoutV1 {
                    version: 1,
                    max_context: 8,
                    checkpoint_interval: 1,
                    h_tile: 1,
                    commit_tiles: vec![],
                    state_tiles: vec![],
                },
                declarations: PalwCandidateDeclarationsV1 {
                    datasets,
                    licences,
                    teacher_classes: PalwTeacherClassV1::PublicData.bit(),
                },
            }
        };
        for (why, bad) in [
            ("a dataset the line never registered", submission(CAND_A, reference, vec![(h(0x99), 500)], vec![])),
            ("a licence never registered", submission(CAND_A, reference, vec![], vec![h(0x98)])),
            (
                "full weights the policy does not admit",
                submission(CAND_A, PalwTirArtifactRefV1::Single { root: h(0x23) }, vec![], vec![]),
            ),
            (
                "a composite over a class not of the line",
                submission(
                    CAND_A,
                    PalwTirArtifactRefV1::Composite { parent_class: h(0x97), parent_root: h(0x21), adapter_root: h(0x22), p: 4 },
                    vec![],
                    vec![],
                ),
            ),
            ("the head itself", submission(LINE, reference, vec![], vec![])),
        ] {
            assert!(try_at(&s, 1_200, |b, x| apply_candidate_submitted_v1(b, x, &bad, &bond(ALICE))).is_err(), "{why}");
        }
        assert!(
            try_at(&s, 1_100, |b, x| apply_candidate_submitted_v1(b, x, &submission(CAND_A, reference, vec![], vec![]), &bond(ALICE)))
                .is_err(),
            "before t_fix"
        );
        let ok = submission(CAND_A, reference, vec![(public.dataset_id, 1_000)], vec![]);
        let s = try_at(&s, 1_200, |b, x| apply_candidate_submitted_v1(b, x, &ok, &bond(ALICE))).expect("admitted");
        assert_eq!(s.improvement_composite_class(&h(CAND_A)), reference.composite(), "recorded for its openings");
        // The node's read lists the record for as long as the class lives (a seat proves a composite's possession
        // over the adapter section the record names).
        let listed: Vec<_> = reference.composite().map(|r| (h(CAND_A), r)).into_iter().collect();
        assert_eq!(s.improvement_composite_classes_v1().map(|(c, r)| (*c, *r)).collect::<Vec<_>>(), listed);
        assert_eq!(s.improvement_status_v1().composite_classes, listed, "the status door carries it");
        assert_eq!(s.improvement_candidates(&h(LINE), 1).len(), 1);
        assert_eq!(s.improvement_line_classes(&h(LINE)), vec![h(LINE)]);
        conserved(&s);
    }

    #[test]
    fn a_line_leaving_governance_refunds_what_its_material_holds() {
        let s = opted_in(500);
        let public = dataset(vec![allowed_licence()], PalwTeacherClassV1::PublicData.bit());
        let a = artifact(PalwTeachingArtifactKindV1::PreferencePair, h(0x90), 0xC0, 1);
        let s = try_at(&s, 600, |b, x| {
            apply_dataset_registered_v1(b, x, &public, &bond(CAROL))?;
            apply_artifact_committed_v1(b, x, &commit_of(&a), &bond(ALICE))
        })
        .unwrap();
        assert!(s.improvement_pool(&h(LINE)).unwrap().held > 0);
        let out = PalwImprovementPolicySetV1 { line_id: h(LINE), sequence: 2, policy: None };
        let s = at(&s, &params(), 700, |b| apply_improvement_policy_set_v1(b, &ctx(700), &out).expect("opt out"));
        let effective = match s.improvement_line(&h(LINE)).unwrap().status {
            PalwImprovementLineStatusV1::OptingOut { effective_daa: Some(at) } => at,
            other => panic!("opting out: {other:?}"),
        };
        let s = at(&s, &params(), effective, |_| {});
        assert!(s.improvement_dataset(&h(LINE), &public.dataset_id).is_none(), "the dataset's bond refunded");
        assert!(s.improvement_artifact(&h(LINE), &commit_of(&a).commit).is_none(), "the unrevealed artifact refunded");
        assert_eq!(s.improvement_pool(&h(LINE)).map(|p| p.held).unwrap_or(0), 0, "nothing held: the line can dissolve");
        assert!(s.improvement_earnings(&bond(CAROL)) >= policy().fees.dataset_bond || s.improvement_pool(&h(LINE)).is_none());
    }

    #[test]
    fn the_material_writes_replay_and_revert_through_the_delta() {
        let p = params();
        let s = opted_in(500);
        let c = case(vec![1, 2, 3], exact(&[5], 0x51), PalwCaseSourceV1::Setter);
        let a = artifact(PalwTeachingArtifactKindV1::Answer, c.case_id, 0xD0, 1);
        let public = dataset(vec![allowed_licence()], PalwTeacherClassV1::PublicData.bit());
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        let x = ctx(600);
        apply_hard_case_v1(&mut builder, &x, &c, &bond(ALICE)).unwrap();
        apply_dataset_registered_v1(&mut builder, &x, &public, &bond(CAROL)).unwrap();
        apply_teacher_licence_v1(&mut builder, &x, &licence(9_000)).unwrap();
        apply_artifact_committed_v1(&mut builder, &x, &commit_of(&a), &bond(BOB)).unwrap();
        apply_artifact_revealed_v1(&mut builder, &x, &a).unwrap();
        builder.write_improvement_composite_class(
            h(CAND_B),
            Some(PalwTirCompositeRefV1 { parent_class: h(LINE), parent_root: h(1), adapter_root: h(2), p: 3 }),
        );
        let (child, _) = builder.checkpoint();
        let delta = PalwStateDeltaV2 { point: x, entries: builder.entries.clone() };
        let names: BTreeSet<&str> = delta
            .entries
            .iter()
            .filter_map(|e| match e {
                PalwDeltaEntryV2::ImprovementCase { .. } => Some("case"),
                PalwDeltaEntryV2::ImprovementDataset { .. } => Some("dataset"),
                PalwDeltaEntryV2::ImprovementLicence { .. } => Some("licence"),
                PalwDeltaEntryV2::ImprovementArtifact { .. } => Some("artifact"),
                PalwDeltaEntryV2::ImprovementCompositeClass { .. } => Some("composite"),
                _ => None,
            })
            .collect();
        assert_eq!(names.len(), 5, "{names:?}");
        let replayed = apply_delta_v2(&s, &delta, &p).expect("replays");
        assert_eq!(replayed, child, "the delta reproduces the child, indices included");
        assert_ne!(replayed.state_root(), s.state_root(), "the material moves the root");
        let reverted = revert_delta_v2(&child, &delta, &p).expect("reverts");
        assert_eq!(reverted, s, "the delta reverts to the parent");
        // The carriage round-trips the material and its indices come back (the fixture's IR rows carry no
        // program, so they stay out of this load).
        let mut bare = child.clone();
        bare.tir_classes.clear();
        let carried: PalwStateCarriageV2 =
            borsh::from_slice(&borsh::to_vec(&PalwStateCarriageV2::from_state(&bare)).unwrap()).unwrap();
        let back = carried.into_state(&p, Some(bare.state_root())).expect("consistent, and the root it was carried at");
        assert_eq!(back.improvement_artifact_original(&h(LINE), &h(0xD0)), Some(commit_of(&a).commit));
        assert_eq!(back, bare, "every table and index back");
        // Below the fence nothing is written and the block is byte-identical.
        let dormant = PalwChainStateV2::genesis();
        assert!(!dormant.has_improvement_material_rows_v1());
    }

    // -----------------------------------------------------------------------------------------
    // Lane PA (`palw_audit_1004_v1`): RF-1, RF-3, RF-4.
    // -----------------------------------------------------------------------------------------

    fn fenced_params(on: bool) -> PalwStateParamsV2 {
        if on { params().with_audit_1004_from_daa(Some(0)) } else { params() }
    }

    fn try_at_p(
        state: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
        daa: u64,
        f: impl FnOnce(&mut TransitionBuilder<'_>, &PalwBlockContextV2) -> Result<(), PalwStateV2Error>,
    ) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let mut builder = TransitionBuilder::new(state, p, false, false, false, false, extras);
        advance_improvement_v1(&mut builder, &ctx(daa))?;
        settle_improvement_material_v1(&mut builder, &ctx(daa))?;
        f(&mut builder, &ctx(daa))?;
        Ok(builder.checkpoint().0)
    }

    fn opted_in_p(p: &PalwStateParamsV2, daa: u64) -> PalwChainStateV2 {
        // (The fenced policy rules want a setter cap of at most 500 ‰.)
        let mut pol = policy();
        pol.eval.setter_cap_permille = 500;
        let set = PalwImprovementPolicySetV1 { line_id: h(LINE), sequence: 1, policy: Some(pol) };
        try_at_p(&genesis(), p, &PalwTransitionExtrasV1::default(), daa, |b, x| apply_improvement_policy_set_v1(b, x, &set)).expect("opt in")
    }

    fn class_row(registrant: u8) -> PalwClassStateV2 {
        PalwClassStateV2 {
            artifact_root: h(0x70),
            slash_value_per_pwu: 3,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(100),
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: Some(bond(registrant)),
            fused_attention: false,
        }
    }

    /// **RF-1**: the S2 trainer reward goes to a candidate's submitter, so past the fence the submitter must be its class's registrant.
    #[test]
    fn audit_1004_a_candidate_is_accepted_only_from_its_classs_registrant() {
        let submission = |class: u8| PalwCandidateSubmissionV1 {
            line_id: h(LINE),
            epoch: 1,
            class_id: h(class),
            artifact: PalwTirArtifactRefV1::Composite { parent_class: h(LINE), parent_root: h(0x21), adapter_root: h(0x22), p: 4 },
            layout: crate::palw_tir_class_v1::PalwTirLayoutV1 {
                version: 1,
                max_context: 8,
                checkpoint_interval: 1,
                h_tile: 1,
                commit_tiles: vec![],
                state_tiles: vec![],
            },
            declarations: PalwCandidateDeclarationsV1 { datasets: vec![], licences: vec![], teacher_classes: PalwTeacherClassV1::PublicData.bit() },
        };
        for on in [false, true] {
            let p = fenced_params(on);
            let mut s = opted_in_p(&p, 500);
            s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
            let s = try_at_p(&s, &p, &PalwTransitionExtrasV1::default(), 1_000, |_, _| Ok(())).unwrap();
            // BOB registered the class; ALICE submits it.
            let mut s = s;
            s.classes.insert(h(CAND_A), class_row(BOB));
            let extras = PalwTransitionExtrasV1::default();
            let by = |who: u8| try_at_p(&s, &p, &extras, 1_200, |b, x| apply_candidate_submitted_v1(b, x, &submission(CAND_A), &bond(who), ));
            let (alice, bob) = (by(ALICE), by(BOB));
            assert_eq!(alice.is_err(), on, "fence {on}: a copycat submitting another bond's class is refused past the fence only: {:?}", alice.as_ref().err());
            assert!(bob.is_ok(), "fence {on}: the registrant's own submission stands: {:?}", bob.err());
        }
    }

    /// **RF-1**: a dataset's id names its registrant past the fence, so a copy of a published dataset registered first under another bond
    /// does not stand: the plain id is refused and the id bound to the registrant is the one accepted.
    #[test]
    fn audit_1004_a_datasets_id_names_its_registrant() {
        let plain = dataset(vec![allowed_licence()], PalwTeacherClassV1::PublicData.bit());
        let mut mine = plain.clone();
        mine.dataset_id = palw_dataset_id_bound_v1(&mine, &bond(CAROL));
        for on in [false, true] {
            let p = fenced_params(on);
            let s = opted_in_p(&p, 500);
            let extras = PalwTransitionExtrasV1::default();
            let register = |d: &PalwDatasetV1, who: u8| try_at_p(&s, &p, &extras, 600, |b, x| apply_dataset_registered_v1(b, x, d, &bond(who)));
            assert_eq!(register(&plain, CAROL).is_ok(), !on, "fence {on}: the plain id stands below the fence only");
            assert_eq!(register(&mine, CAROL).is_ok(), on, "fence {on}: the registrant-bound id stands past the fence only");
            if on {
                assert!(register(&mine, BOB).is_err(), "another bond cannot register carol's bound id");
            }
        }
    }

    /// **RF-4**: a licence's expiry is bounded past the fence; below it any future expiry stands.
    #[test]
    fn audit_1004_a_teacher_licence_has_a_bounded_life() {
        let far = licence(600 + crate::palw_audit_1004_v1::PALW_AUDIT_1004_MAX_LICENCE_LIFE_DAA_V1 + 1);
        let near = licence(600 + crate::palw_audit_1004_v1::PALW_AUDIT_1004_MAX_LICENCE_LIFE_DAA_V1);
        for on in [false, true] {
            let p = fenced_params(on);
            let s = opted_in_p(&p, 500);
            let extras = PalwTransitionExtrasV1::default();
            let reg = |l: &PalwTeacherLicenceV1| try_at_p(&s, &p, &extras, 600, |b, x| apply_teacher_licence_v1(b, x, l));
            assert_eq!(reg(&far).is_err(), on, "fence {on}: an expiry past the longest life is refused past the fence only");
            assert!(reg(&near).is_ok(), "fence {on}: the longest life is allowed");
        }
    }

    /// **RF-4 / RF-3**: a fenced policy may not carry a free fee, a free bond, or a beacon delay the beacon would precede the pool's close under.
    #[test]
    fn audit_1004_a_policy_with_a_free_fee_or_a_short_beacon_delay_is_refused_past_the_fence() {
        let edits: [(&str, fn(&mut PalwImprovementPolicyV1)); 6] = [
            ("a free registration", |p| p.fees.registration_fee = 0),
            ("a free evaluation job", |p| p.fees.eval_fee_per_job = 0),
            ("a free hard case", |p| p.fees.hard_case_fee = 0),
            ("a free dataset bond", |p| p.fees.dataset_bond = 0),
            ("a beacon delay under twice the beacon's depth", |p| p.windows.beacon_delay = 7),
            ("a setter cap over 500 ‰", |p| p.eval.setter_cap_permille = 501),
        ];
        for on in [false, true] {
            let p = fenced_params(on);
            for (what, edit) in edits {
                let mut pol = policy();
                edit(&mut pol);
                let set = PalwImprovementPolicySetV1 { line_id: h(LINE), sequence: 1, policy: Some(pol) };
                let r = try_at_p(&genesis(), &p, &PalwTransitionExtrasV1::default(), 500, |b, x| apply_improvement_policy_set_v1(b, x, &set));
                assert_eq!(r.is_err(), on, "{what}, fence {on}: {:?}", r.as_ref().err());
            }
        }
        let mut sane = policy();
        sane.eval.setter_cap_permille = 500;
        assert_eq!(crate::palw_improve_policy_v1::palw_improvement_policy_audit_1004_check_v1(&sane), Ok(()), "the core's own test policy passes at 500 ‰");
    }

    /// **RF-3**: the epoch draw's seed is the beacon's, not the drawing block's own hash, past the fence.
    #[test]
    fn audit_1004_the_epoch_draw_reads_the_beacon() {
        // Drive an epoch to its draw (the same steps as `drawn_epoch`), at the draw block under a beacon.
        let seed_at = |on: bool, block_byte: u8, beacon: Option<u8>| {
            let p = fenced_params(on);
            let mut s = opted_in_p(&p, 500);
            s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
            let none = PalwTransitionExtrasV1::default();
            let s = try_at_p(&s, &p, &none, 1_000, |_, _| Ok(())).unwrap();
            let s = try_at_p(&s, &p, &none, 1_200, |b, x| {
                b.admit_improvement_candidate_v1(
                    &h(LINE),
                    1,
                    &h(CAND_A),
                    &bond(ALICE),
                    PalwTirArtifactRefV1::Composite { parent_class: h(LINE), parent_root: h(0x21), adapter_root: h(0x22), p: 4 },
                    h(0x50),
                    Vec::new(),
                    x.daa_score,
                )
                .map(|_| ())
            })
            .unwrap();
            let s = try_at_p(&s, &p, &none, 1_400, |_, _| Ok(())).unwrap();
            // The draw block: a block of a given hash and an extras beacon.
            let extras = PalwTransitionExtrasV1 { audit_1004_beacon: beacon.map(h), ..Default::default() };
            let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
            let c = PalwBlockContextV2 { block: h(block_byte), ..ctx(1_510) };
            advance_improvement_v1(&mut builder, &c).unwrap();
            let next = builder.checkpoint().0;
            next.improvement_epoch(&h(LINE), 1).and_then(|e| e.seed)
        };
        let a = seed_at(true, 1, Some(9));
        assert!(a.is_some(), "the draw ran and recorded its seed");
        assert_eq!(a, seed_at(true, 2, Some(9)), "past the fence the drawing block's own hash does not matter");
        assert_ne!(a, seed_at(true, 1, Some(10)), "the beacon does");
        assert_ne!(seed_at(false, 1, Some(9)), seed_at(false, 2, Some(9)), "below the fence the block's hash is the seed, and the beacon is not read");
        assert_eq!(seed_at(false, 1, Some(9)), seed_at(false, 1, None));
    }
}
