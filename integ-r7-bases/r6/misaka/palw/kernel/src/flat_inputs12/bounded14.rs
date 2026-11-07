//! Versioned job binding for the frozen v2 external-input semantics.
//! Maximum geometry and actual geometry are independently checked. Synthetic
//! maximum input identifiers are used only to price capacity; they cannot be
//! acquired, executed or opened as an actual input. This component does not
//! install native media funding/Final or change legacy admission.
use super::*;
use crate::{
    flat_court9::{FlatContextV9, FlatConvictionV9, FlatFaultV9, verify_flat_fault_v9},
    flat_detect9::FlatPublicReadsV9,
    flat_replay9::{FlatReplayEnvelopeV9, FlatReplayResultV9, FlatReplaySessionV9, flat_replay_envelope_v9},
    flat_resources10::{FlatResourceVectorV10, derive_flat_resources_v10},
    verify::DismissalV1,
};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatStageJobV14 {
    pub version: u16,
    pub template: Digest,
    pub requester: Digest,
    pub producer: Digest,
    pub nonce: Digest,
    pub actual: FlatJobLengthsV7,
    pub positions: u32,
    pub input_roots: Vec<Digest>,
}
impl FlatStageJobV14 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/typed-stage-job/v14", self)
    }
    /// Bound the only variable field before Borsh allocates it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        const PREFIX: usize = 2 + 4 * 64 + 3 * 4 + 4;
        if bytes.len() < PREFIX + 4 || bytes.len() > PREFIX + 4 + INPUT_COUNT_MAX_V12 * 64 {
            return Err("stage job frame exceeds fixed input-root cap".into());
        }
        let count = u32::from_le_bytes(bytes[PREFIX..PREFIX + 4].try_into().expect("checked prefix")) as usize;
        if count == 0 || count > INPUT_COUNT_MAX_V12 || bytes.len() != PREFIX + 4 + count * 64 {
            return Err("stage job root count differs from bounded frame".into());
        }
        let job = Self::try_from_slice(bytes).map_err(|e| e.to_string())?;
        if job.version != 14 || borsh::to_vec(&job).map_err(|e| e.to_string())? != bytes {
            return Err("unknown or noncanonical typed stage job".into());
        }
        Ok(job)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatStageAdmissionV14 {
    pub version: u16,
    pub template: Digest,
    pub job: Digest,
    pub source_parameters: Digest,
    pub maxima: FlatJobLengthsV7,
    pub max_positions: u32,
    pub max_stage_program: Digest,
    pub max_scope: Digest,
    pub max_replay: Digest,
    pub max_resources: Digest,
    pub actual_input_admission: Digest,
    pub actual_stage_program: Digest,
    pub actual_scope: Digest,
    pub actual_replay: Digest,
    pub actual_resources: Digest,
    pub combined_parameters: Digest,
}
impl FlatStageAdmissionV14 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/typed-stage-admission/v14", self)
    }
}

pub struct CheckedStageJobV14 {
    admission: FlatStageAdmissionV14,
    job: FlatStageJobV14,
    actual: FlatInputSpecializationV12,
    commitments: FlatInputCommitmentsV12,
    combined_pc: ParamCommitmentsV1,
    replay: FlatReplayEnvelopeV9,
    resources: FlatResourceVectorV10,
    max_resources: FlatResourceVectorV10,
}
impl CheckedStageJobV14 {
    pub fn admission(&self) -> &FlatStageAdmissionV14 {
        &self.admission
    }
    pub fn job(&self) -> &FlatStageJobV14 {
        &self.job
    }
    pub fn combined_parameters(&self) -> &ParamCommitmentsV1 {
        &self.combined_pc
    }
    pub fn program(&self) -> &TirProgramV2 {
        &self.actual.program
    }
    pub fn scope(&self) -> &FlatResourceScopeV9 {
        &self.actual.scope
    }
    pub fn replay(&self) -> &FlatReplayEnvelopeV9 {
        &self.replay
    }
    pub fn resources(&self) -> &FlatResourceVectorV10 {
        &self.resources
    }
    pub fn maximum_resources(&self) -> &FlatResourceVectorV10 {
        &self.max_resources
    }
    pub fn commitments(&self) -> &FlatInputCommitmentsV12 {
        &self.commitments
    }

    pub fn acquire(
        &self,
        template: &FlatInputTemplateV12,
        source_pc: &ParamCommitmentsV1,
        provider: &dyn RawExternalInputsV12,
        budget: &mut FlatInputBudgetV12,
    ) -> Result<AuthenticatedInputsV12, String> {
        let inputs = acquire_authenticated_inputs_v12(
            template,
            &self.actual.admission,
            self.job.root(),
            &self.commitments,
            source_pc,
            provider,
            budget,
        )?;
        if inputs.combined_pc() != &self.combined_pc {
            return Err("acquired input roots differ from resource/checker binding".into());
        }
        Ok(inputs)
    }

    /// The same job-specific graph, PC, court set and exact position count
    /// checked on admission must enter the public terminal court. Neither a
    /// later slice nor a cheap independently supplied scope can enlarge it.
    pub fn verify_fault(&self, context: &FlatContextV9<'_>, fault: &FlatFaultV9) -> Result<FlatConvictionV9, DismissalV1> {
        self.check_context(context)?;
        if matches!(fault, FlatFaultV9::Legacy(crate::flat_court5::FlatFaultV5::Decode(_))) {
            return Err(DismissalV1::NotAuthentic("Rows/Final stage has no decoder court".into()));
        }
        verify_flat_fault_v9(context, fault)
    }

    /// Full graph replay and localization, charged by the same envelope that
    /// admission derived. The canonical position carrier is internal plumbing;
    /// its dummy delivered zero never constrains the tensor stage output.
    pub fn check_replay(
        &self,
        context: &FlatContextV9<'_>,
        session: &mut FlatReplaySessionV9,
        public: &dyn FlatPublicReadsV9,
        artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    ) -> FlatReplayResultV9 {
        if let Err(e) = self.check_context(context) {
            return FlatReplayResultV9::Inconsistent(format!("{e:?}"));
        }
        let first = self.actual.program.params.len();
        let checked_artifact = |index: u16, layer: Option<u16>| {
            let value = artifact(index, layer)?;
            if index as usize >= first {
                let d = self.actual.program.inputs.get(index as usize - first)?;
                let cells = d.shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d as usize))?;
                if layer.is_some()
                    || value.dtype != d.dtype
                    || value.data.len() != cells
                    || value.shape.iter().copied().ne(d.shape.iter().map(|d| *d as usize))
                {
                    return None;
                }
                let (lo, hi) = d.interval();
                if value.data.iter().any(|v| *v < lo || *v > hi) {
                    return None;
                }
            }
            Some(value)
        };
        session.check_stage_v14(
            context,
            &self.replay,
            public,
            &checked_artifact,
            context.base.claim.inline_prompt.expect("checked inline position carrier"),
        )
    }

    fn check_context(&self, context: &FlatContextV9<'_>) -> Result<(), DismissalV1> {
        let claim = &context.base.claim;
        if context.scope != &self.actual.scope
            || claim.program != &self.actual.program.v1_view()
            || claim.params != &self.combined_pc
            || claim.positions != self.job.positions
            || claim.encoder.is_some()
            || claim.prompt_len != self.job.positions
            || claim.generated != [0]
            || claim.inline_prompt.is_none_or(|ids| {
                ids.len() != self.job.positions as usize
                    || ids.iter().any(|t| *t != 0)
                    || crate::seg::prompt_root_of_ids_v1(ids) != claim.prompt_root
            })
        {
            return Err(DismissalV1::NotAuthentic("stage court differs from immutable job binding".into()));
        }
        Ok(())
    }
}

pub fn derive_stage_job_v14(
    template: &FlatInputTemplateV12,
    job: &FlatStageJobV14,
    source_pc: &ParamCommitmentsV1,
) -> Result<CheckedStageJobV14, String> {
    if job.version != 14
        || job.template != template.root()?
        || job.input_roots.len() != template.program.inputs.len()
        || job.input_roots.is_empty()
        || job.input_roots.len() > INPUT_COUNT_MAX_V12
    {
        return Err("stage job differs from exact bounded input template".into());
    }
    if template.program.blocks.iter().flat_map(|b| &b.nodes).flat_map(|n| &n.inputs).any(|r| *r == misaka_palw_tir::Ref::Input(0)) {
        return Err("external stage14 cannot read an uncommitted decoder token".into());
    }
    // Validate the real weight namespace before adding virtual job inputs.
    // The maximum/actual programs cannot resize weights or persistent state:
    // both pass V12's external-input-only specialization rule independently.
    let count = template
        .program
        .params
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(if p.per_layer { template.program.schedule.layers.len() } else { 1 }))
        .ok_or("stage source namespace overflow")?;
    if count > SOURCE_PC_INSTANCES_MAX_V12 || count != source_pc.by_instance.len() || source_pc.root() != template.source_parameters {
        return Err("stage source weights differ from template namespace/root".into());
    }
    for (j, p) in template.program.params.iter().enumerate() {
        if p.per_layer {
            for l in 0..template.program.schedule.layers.len() {
                if !source_pc.by_instance.contains_key(&(j as u16, Some(l as u16))) {
                    return Err("stage source per-layer instance missing".into());
                }
            }
        } else if !source_pc.by_instance.contains_key(&(j as u16, None)) {
            return Err("stage source global instance missing".into());
        }
    }
    let maximum = template.specialize(template.lengths.maxima, template.max_positions)?;
    let actual = template.specialize(job.actual, job.positions)?;
    let first = template.program.params.len() as u16;
    let mut max_pc = source_pc.clone();
    let mut combined_pc = source_pc.clone();
    for (i, d) in maximum.program.inputs.iter().enumerate() {
        // These declaration IDs quote maximum geometry without claiming that
        // a maximum-shaped input image exists. They are never exposed as PC.
        max_pc.by_instance.insert(
            (first + i as u16, None),
            object_id(b"misaka-palw/kernel/maximum-input-declaration/v14", &(job.template, i as u16, d)),
        );
        combined_pc.by_instance.insert((first + i as u16, None), job.input_roots[i]);
    }
    let max_plan = derive_flat_resources_v10(&maximum.program.v1_view(), &max_pc, template.max_positions, None)?;
    let actual_plan = derive_flat_resources_v10(&actual.program.v1_view(), &combined_pc, job.positions, None)?;
    if max_plan.court != maximum.scope || actual_plan.court != actual.scope {
        return Err("independent stage resource scope differs from input scope".into());
    }
    let max_replay = flat_replay_envelope_v9(&maximum.program.v1_view(), &maximum.scope, None).map_err(|e| format!("{e:?}"))?;
    let replay = flat_replay_envelope_v9(&actual.program.v1_view(), &actual.scope, None).map_err(|e| format!("{e:?}"))?;
    let admission = FlatStageAdmissionV14 {
        version: 14,
        template: job.template,
        job: job.root(),
        source_parameters: source_pc.root(),
        maxima: template.lengths.maxima,
        max_positions: template.max_positions,
        max_stage_program: maximum.admission.stage_program,
        max_scope: maximum.scope.root(),
        max_replay: max_replay.root(),
        max_resources: max_plan.vector.root(),
        actual_input_admission: actual.admission.root(),
        actual_stage_program: actual.admission.stage_program,
        actual_scope: actual.scope.root(),
        actual_replay: replay.root(),
        actual_resources: actual_plan.vector.root(),
        combined_parameters: combined_pc.root(),
    };
    let commitments = FlatInputCommitmentsV12 {
        version: 12,
        admission: actual.admission.root(),
        job: job.root(),
        input_roots: job.input_roots.clone(),
    };
    Ok(CheckedStageJobV14 {
        admission,
        job: job.clone(),
        actual,
        commitments,
        combined_pc,
        replay,
        resources: actual_plan.vector,
        max_resources: max_plan.vector,
    })
}

pub fn check_stage_job_v14(
    template: &FlatInputTemplateV12,
    job: &FlatStageJobV14,
    source_pc: &ParamCommitmentsV1,
    admission: &FlatStageAdmissionV14,
) -> Result<CheckedStageJobV14, String> {
    let checked = derive_stage_job_v14(template, job, source_pc)?;
    if checked.admission != *admission {
        return Err("stage admission differs from complete independent derivation".into());
    }
    Ok(checked)
}
