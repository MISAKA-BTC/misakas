//! **The media-pipeline family** (`docs/design/palw/versioned-kernels.md` §K.3 "media and pipelines", RFC-0003 §I.2.3, spec 04b §15) on the kernel route.
//!
//! A pipeline class is several TIR v2 programs run in declared order (a text encoder, a denoiser, a decoder; a vision encoder
//! and a language model; an evaluation's subject and scorer). Every stage is checked as a K2 claim over its **version-1 view**
//! ([`StageViewV1`]): the stage's input tensors are committed per position beside its node values, and the view's `post`
//! `Clamp`s (the v2 state writes) feed the next position's state reads. Stages meet only through **edges**, and an edge has no
//! arithmetic: a stage input is a job value (scalars, token templates and counts, a canonical `u8` RGB image), an earlier stage's
//! committed output rows or final value (an authenticated copy with zero pad), or RFC-0003's `R`. The [`CheckerIdV1::EdgeRecompute`]
//! relation recomputes each input exactly and checks the input's declared interval; its court ([`verify_edge_fault_v1`]) opens
//! one input at one position and the upstream values it copies, all against public commitments.
//!
//! * [`pipeline_plan_v1`] / [`check_pipeline_plan_v1`] — the reference plan (one stage plan per stage over its view, one edge per
//!   stage input) and its deterministic check: the descriptor implements the media-pipeline family (else
//!   `KERNEL_EXTENSION_REQUIRED [media-pipeline]`), the pipeline is in normal form and every program's v2 ranges are proven (else
//!   `FRONTEND_REQUIRED`), every stage plan passes the plan checker, every edge is present exactly once, and the whole pipeline's
//!   error (the union over every stage's probabilistic instances at its `max_trip`) reaches the target.
//! * [`trace_pipeline_v1`] — an honest producer's per-stage traces; [`build_pipeline_evidence_v1`] — the §15.3 evidence object of
//!   the whole pipeline (job, `R` binding, every stage's evidence object, the output root).
//! * [`verify_pipeline_v1`] — every edge, then every stage's relations, stage by stage; a stage's challenge is bound to the
//!   whole pipeline's evidence. A fault is a stage's [`KernelFaultProofV1`] or an [`EdgeFaultProofV1`]; missing or mismatched
//!   material is `Unavailable`.

use std::collections::BTreeMap;

use misaka_palw_tir::pipeline::{
    Binding, PipelineJob, PipelineParams, RandomSource, StageJobFacts, TirPipelineV1, stage_job_facts, validate_pipeline,
};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::program_v2::{InputSource, OutputDecl, TirProgramV2};
use misaka_palw_tir::{DType, Tensor};

use crate::challenge::ChallengeBindingV1;
use crate::check::{RangeRuleV1, check_plan_with_v1};
use crate::descriptor::{KernelDescriptorV1, KernelScheduleV1};
use crate::evidence::{EvidenceHeaderV1, VerificationEvidenceV1, build_evidence_v1};
use crate::family::{CheckerIdV1, ConstraintFamilyV1, CourtIdV1};
use crate::hash::{Digest, finish, keyed};
use crate::outcome::RegistrationOutcomeV1 as O;
use crate::plan::{VerificationPlanV1, derived_error_bits, plan_for_tir_program_v1};
use crate::public::program_root_v1;
use crate::trace::{EvidenceV1, ParamCommitmentsV1, StageBindingV1, TraceV1, WiringV1, tensor_commitment, trace_stage_v1};
use crate::verify::{ClaimContextV1, KernelFaultProofV1, MaterialV1, ScopeV1, ScopeVerdictV1, verify_scope_v1};

pub const PIPELINE_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline/v1";
pub const PIPELINE_PLAN_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline-plan/v1";
pub const PIPELINE_EVIDENCE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline-evidence/v1";
pub const PIPELINE_JOB_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline-job/v1";
pub const PIPELINE_OUTPUT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline-output/v1";
pub const STAGE_CLAIM_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline-stage-claim/v1";
pub const PIPELINE_PLAN_GRAMMAR_V1: u16 = 1;

/// A stage program's version-1 view and what makes it a stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageViewV1 {
    pub view: TirProgramV1,
    pub binding: StageBindingV1,
    /// The occurrence index of `post` (the last) and the output node in it.
    pub post_occurrence: u16,
    pub output_node: u16,
}

/// The view of one v2 program.
pub fn stage_view_v1(p: &TirProgramV2) -> StageViewV1 {
    let view = p.v1_view();
    let post_occurrence = (p.occurrences().len() - 1) as u16;
    let post_writers = p.post_writes().into_iter().map(|(node, state)| (state, post_occurrence, node)).collect();
    StageViewV1 {
        binding: StageBindingV1 { first_input: p.params.len() as u16, post_writers, inputs: p.inputs.len() as u16 },
        post_occurrence,
        output_node: p.output.node(),
        view,
    }
}

/// The root a pipeline class binds: the pipeline's bytes and every program's root, in order.
pub fn pipeline_root_v1(p: &TirPipelineV1, programs: &[TirProgramV2]) -> Digest {
    let mut s = keyed(PIPELINE_ROOT_DOMAIN_V1);
    let bytes = p.encode();
    s.update(&(bytes.len() as u64).to_le_bytes()).update(&bytes);
    s.update(&(programs.len() as u64).to_le_bytes());
    for prog in programs {
        s.update(&program_root_v1(&prog.encode()));
    }
    finish(s)
}

/// One edge: input `input` of stage `stage`.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct EdgeRelationV1 {
    pub stage: u8,
    pub input: u16,
    /// The binding's tag (`JobScalar 0` … `JobImage 6`), or 7 for a random input.
    pub kind: u8,
    /// A per-step random input: a different value at every position.
    pub per_position: bool,
    pub elements: u64,
    pub checker: CheckerIdV1,
    pub court: CourtIdV1,
}

/// **`PipelinePlanV1`**: one stage plan per stage (over its view, at its `max_trip`), one edge per stage input.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PipelinePlanV1 {
    pub grammar: u16,
    pub descriptor_digest: Digest,
    pub pipeline_root: Digest,
    pub stages: Vec<VerificationPlanV1>,
    pub edges: Vec<EdgeRelationV1>,
    pub declared_error_bits: u16,
}

impl PipelinePlanV1 {
    pub fn root(&self) -> Digest {
        crate::hash::object_id(PIPELINE_PLAN_DOMAIN_V1, self)
    }
}

fn binding_tag(b: &Binding) -> u8 {
    match b {
        Binding::JobScalar { .. } => 0,
        Binding::JobTokens { .. } => 1,
        Binding::StageRows { .. } => 2,
        Binding::StageFinal { .. } => 3,
        Binding::StageRowCount { .. } => 4,
        Binding::JobTokenCount { .. } => 5,
        Binding::JobImage { .. } => 6,
    }
}

/// Input `k`'s binding in stage `st` (`None` for a random input).
fn binding_of<'a>(st: &'a misaka_palw_tir::pipeline::StageDecl, prog: &TirProgramV2, k: usize) -> Option<&'a Binding> {
    let ext = prog.inputs[..k].iter().filter(|d| d.is_external()).count();
    prog.inputs[k].is_external().then(|| st.bind.get(ext)).flatten()
}

fn expected_edges(
    descriptor: &KernelDescriptorV1,
    p: &TirPipelineV1,
    programs: &[TirProgramV2],
) -> Result<Vec<EdgeRelationV1>, String> {
    let support = descriptor.support(ConstraintFamilyV1::MediaPipeline).ok_or("no media-pipeline checker in this kernel")?;
    let mut out = Vec::new();
    for (si, st) in p.stages.iter().enumerate() {
        let prog = &programs[st.program as usize];
        for (k, d) in prog.inputs.iter().enumerate() {
            let (kind, per_position) = match d.source {
                InputSource::Random { per_step, .. } => (7, per_step),
                InputSource::External { .. } => {
                    (binding_tag(binding_of(st, prog, k).ok_or("an external input with no binding")?), false)
                }
            };
            out.push(EdgeRelationV1 {
                stage: si as u8,
                input: k as u16,
                kind,
                per_position,
                elements: d.shape.iter().map(|x| *x as u64).product(),
                checker: support.checker,
                court: support.court,
            });
        }
    }
    Ok(out)
}

fn stage_program_root(prog: &TirProgramV2) -> Digest {
    program_root_v1(&prog.encode())
}

fn total_instances(plans: &[VerificationPlanV1]) -> u128 {
    plans.iter().map(|pl| pl.budgets.probabilistic_instances_per_position as u128 * pl.max_positions as u128).sum()
}

/// **The reference frontend for a pipeline**: `Err` names the first missing family.
pub fn pipeline_plan_v1(
    descriptor: &KernelDescriptorV1,
    p: &TirPipelineV1,
    programs: &[TirProgramV2],
) -> Result<PipelinePlanV1, (ConstraintFamilyV1, String)> {
    let edges = expected_edges(descriptor, p, programs).map_err(|e| (ConstraintFamilyV1::MediaPipeline, e))?;
    let mut stages = Vec::with_capacity(p.stages.len());
    for st in &p.stages {
        let prog = &programs[st.program as usize];
        let v = stage_view_v1(prog);
        stages.push(plan_for_tir_program_v1(descriptor, &v.view, stage_program_root(prog), st.max_trip)?);
    }
    Ok(PipelinePlanV1 {
        grammar: PIPELINE_PLAN_GRAMMAR_V1,
        descriptor_digest: descriptor.digest(),
        pipeline_root: pipeline_root_v1(p, programs),
        declared_error_bits: derived_error_bits(descriptor, total_instances(&stages)),
        stages,
        edges,
    })
}

/// What a pipeline plan that passed carries forward.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineAcceptanceV1 {
    pub plan_root: Digest,
    pub error_bits: u16,
}

/// **The deterministic check of a pipeline plan.**
pub fn check_pipeline_plan_v1(
    schedule: &KernelScheduleV1,
    descriptor: &KernelDescriptorV1,
    p: &TirPipelineV1,
    programs: &[TirProgramV2],
    plan: &PipelinePlanV1,
    daa: u64,
) -> Result<PipelineAcceptanceV1, O> {
    if plan.descriptor_digest != descriptor.digest() {
        return Err(O::PlanForged { why: "the pipeline plan names another kernel descriptor".into() });
    }
    if plan.grammar != PIPELINE_PLAN_GRAMMAR_V1 {
        return Err(O::KernelExtensionRequired {
            family: Some(ConstraintFamilyV1::MediaPipeline),
            relation: format!("pipeline plan grammar {}", plan.grammar),
            required: format!("grammar {}", plan.grammar),
            available: format!("grammar {PIPELINE_PLAN_GRAMMAR_V1}"),
        });
    }
    let Some(support) = descriptor.support(ConstraintFamilyV1::MediaPipeline) else {
        return Err(O::KernelExtensionRequired {
            family: Some(ConstraintFamilyV1::MediaPipeline),
            relation: "the pipeline's stage edges and media inputs".into(),
            required: "a media-pipeline relation".into(),
            available: "no media-pipeline checker in this kernel".into(),
        });
    };
    if support.checker != CheckerIdV1::EdgeRecompute {
        return Err(O::PlanForged { why: "the media-pipeline family must be checked by exact edge recompute".into() });
    }
    validate_pipeline(p, programs).map_err(|e| O::FrontendRequired { reason: format!("the pipeline is not in normal form: {e}") })?;
    for (i, prog) in programs.iter().enumerate() {
        misaka_palw_tir::interval_v2::analyze_ranges_v2(prog).map_err(|e| O::FrontendRequired {
            reason: format!("program {i}'s ranges are not proven (an exact primitive can overflow): {e}"),
        })?;
    }
    if plan.pipeline_root != pipeline_root_v1(p, programs) {
        return Err(O::PlanForged { why: "the plan is about another pipeline".into() });
    }
    if plan.stages.len() != p.stages.len() {
        return Err(O::IncompleteCoverage { what: format!("{} stage plans for {} stages", plan.stages.len(), p.stages.len()) });
    }
    let edges = expected_edges(descriptor, p, programs).map_err(|why| O::FrontendRequired { reason: why })?;
    if plan.edges != edges {
        return Err(O::IncompleteCoverage { what: "the edges are not one per stage input, as the pipeline binds them".into() });
    }
    for (si, (st, sp)) in p.stages.iter().zip(&plan.stages).enumerate() {
        let prog = &programs[st.program as usize];
        let v = stage_view_v1(prog);
        if sp.max_positions != st.max_trip {
            return Err(O::IncompleteCoverage {
                what: format!("stage {si}: checked at {} positions, max_trip {}", sp.max_positions, st.max_trip),
            });
        }
        check_plan_with_v1(schedule, descriptor, &v.view, stage_program_root(prog), sp, daa, RangeRuleV1::ProvenByV2).map_err(
            |o| match o {
                O::IncompleteCoverage { what } => O::IncompleteCoverage { what: format!("stage {si}: {what}") },
                O::PlanForged { why } => O::PlanForged { why: format!("stage {si}: {why}") },
                other => other,
            },
        )?;
    }
    let error_bits = derived_error_bits(descriptor, total_instances(&plan.stages));
    if error_bits < descriptor.soundness.target_bits {
        return Err(O::BoundsExceeded {
            what: "whole-pipeline error bits (below the target)",
            required: descriptor.soundness.target_bits as u128,
            limit: error_bits as u128,
        });
    }
    if plan.declared_error_bits > error_bits {
        return Err(O::PlanForged {
            why: format!("declares 2^-{} where the pipeline derives 2^-{error_bits}", plan.declared_error_bits),
        });
    }
    Ok(PipelineAcceptanceV1 { plan_root: plan.root(), error_bits })
}

/// **A pipeline class's static registration outcome** under `descriptor`.
pub fn pipeline_registration_outcome_v1(
    schedule: &KernelScheduleV1,
    descriptor: &KernelDescriptorV1,
    p: &TirPipelineV1,
    programs: &[TirProgramV2],
    daa: u64,
) -> O {
    if let Err(e) = validate_pipeline(p, programs) {
        return O::FrontendRequired { reason: format!("the pipeline is not in normal form: {e}") };
    }
    let plan = match pipeline_plan_v1(descriptor, p, programs) {
        Ok(plan) => plan,
        Err((family, why)) => {
            return O::KernelExtensionRequired {
                family: Some(family),
                relation: why.clone(),
                required: format!("a {} relation", family.name()),
                available: why,
            };
        }
    };
    match check_pipeline_plan_v1(schedule, descriptor, p, programs, &plan, daa) {
        Ok(a) => O::EligibleAt { daa, descriptor: descriptor.digest(), plan_root: a.plan_root, error_bits: a.error_bits },
        Err(o) => o,
    }
}

/// The run tokens of a stage (zeros for a stage that reads no token).
fn run_tokens(f: &StageJobFacts) -> Vec<u32> {
    if f.tokens.is_empty() { vec![0; f.trip as usize] } else { f.tokens.clone() }
}

/// **Input `k` of stage `si` at `position`**, from its binding: job facts, the upstream stage's output values (`upstream(stage)`
/// returns every position's output value of that stage, as committed), or `R`. `Err` when the semantics refuses (a value outside
/// the input's interval, an image of another size, rows past the pad): the stage then has no valid input, and any committed
/// value is a fault.
#[allow(clippy::too_many_arguments)]
pub fn stage_input_v1(
    p: &TirPipelineV1,
    programs: &[TirProgramV2],
    facts: &[StageJobFacts],
    job: &PipelineJob,
    random: &dyn RandomSource,
    si: usize,
    k: u16,
    position: u32,
    upstream: &dyn Fn(u8) -> Option<Vec<Tensor>>,
) -> Result<Tensor, String> {
    let st = &p.stages[si];
    let prog = &programs[st.program as usize];
    let d = prog.inputs.get(k as usize).ok_or("no such input")?;
    let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
    let value = match d.source {
        InputSource::Random { domain, dist, per_step } => {
            random.random(domain, dist, if per_step { position } else { 0 }, &d.shape).ok_or("R refused the domain")?
        }
        InputSource::External { .. } => match binding_of(st, prog, k as usize).ok_or("an external input with no binding")? {
            Binding::JobScalar { .. } | Binding::JobTokens { .. } | Binding::JobTokenCount { .. } | Binding::StageRowCount { .. } => {
                facts[si].inputs.get(&k).cloned().ok_or("the job does not fix this input")?
            }
            Binding::StageRows { stage, drop, pad_to } => {
                let rows = upstream(*stage).ok_or("the upstream rows are not available")?;
                let from = facts[*stage as usize].rows_from as usize;
                let kept = rows.get(from.saturating_add(*drop as usize)..).unwrap_or(&[]);
                if kept.len() > *pad_to as usize {
                    return Err(format!("{} rows exceed pad_to {pad_to}", kept.len()));
                }
                let per_row: usize = shape[1..].iter().product();
                let mut data = Vec::with_capacity(*pad_to as usize * per_row);
                for r in kept {
                    data.extend_from_slice(&r.data);
                }
                data.resize(*pad_to as usize * per_row, 0);
                Tensor::new(d.dtype, shape, data).map_err(|e| e.to_string())?
            }
            Binding::StageFinal { stage } => {
                upstream(*stage).and_then(|r| r.last().cloned()).ok_or("the upstream value is not available")?
            }
            Binding::JobImage { index } => {
                let img = job.images.get(*index as usize).ok_or("the job carries no such image")?;
                if [img.h, img.w, 3] != d.shape[..] || img.rgb.len() as u64 != img.h as u64 * img.w as u64 * 3 {
                    return Err(format!("job image {index} is {}×{}; the input is {:?}", img.h, img.w, d.shape));
                }
                Tensor::new(DType::I16, shape, img.rgb.iter().map(|b| *b as i128).collect()).map_err(|e| e.to_string())?
            }
        },
    };
    let (lo, hi) = d.interval();
    if value.dtype != d.dtype || value.shape != d.shape.iter().map(|x| *x as usize).collect::<Vec<_>>() {
        return Err("the bound value has another type than the input".into());
    }
    if value.data.iter().any(|v| *v < lo || *v > hi) {
        return Err(format!("a bound value lies outside the input's interval [{lo}, {hi}]"));
    }
    Ok(value)
}

/// An honest producer's stage traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineTraceV1 {
    pub stages: Vec<TraceV1>,
    pub tokens: Vec<Vec<u32>>,
}

/// **Trace a pipeline**, stage by stage, every input bound as [`stage_input_v1`] binds it.
pub fn trace_pipeline_v1(
    p: &TirPipelineV1,
    programs: &[TirProgramV2],
    params: &dyn PipelineParams,
    random: &dyn RandomSource,
    job: &PipelineJob,
) -> Result<PipelineTraceV1, String> {
    let facts = stage_job_facts(p, programs, job).map_err(|e| e.to_string())?;
    let mut out = PipelineTraceV1 { stages: Vec::new(), tokens: Vec::new() };
    for (si, st) in p.stages.iter().enumerate() {
        let prog = &programs[st.program as usize];
        let v = stage_view_v1(prog);
        let tokens = run_tokens(&facts[si]);
        let views: Vec<StageViewV1> = p.stages[..si].iter().map(|u| stage_view_v1(&programs[u.program as usize])).collect();
        let done = &out.stages;
        let upstream = |s: u8| -> Option<Vec<Tensor>> {
            let (t, uv) = (done.get(s as usize)?, views.get(s as usize)?);
            t.values.iter().map(|pos| pos.get(uv.post_occurrence as usize)?.get(uv.output_node as usize).cloned()).collect()
        };
        let mut cache: BTreeMap<(u16, u32), Tensor> = BTreeMap::new();
        for k in 0..v.binding.inputs {
            for pos in 0..tokens.len() as u32 {
                cache.insert((k, pos), stage_input_v1(p, programs, &facts, job, random, si, k, pos, &upstream)?);
            }
        }
        let trace =
            trace_stage_v1(&v.view, Some(&v.binding), params.params(st.program), &tokens, &|k, pos| cache.get(&(k, pos)).cloned())
                .map_err(|e| format!("stage {si}: {e}"))?;
        out.stages.push(trace);
        out.tokens.push(tokens);
    }
    Ok(out)
}

/// The job's root: every list and value a pipeline reads, in a fixed order.
pub fn pipeline_job_root_v1(job: &PipelineJob) -> Digest {
    let mut s = keyed(PIPELINE_JOB_DOMAIN_V1);
    let ids = |s: &mut blake2b_simd::State, v: &[u32]| {
        s.update(&(v.len() as u64).to_le_bytes());
        for x in v {
            s.update(&x.to_le_bytes());
        }
    };
    ids(&mut s, &job.prompt);
    ids(&mut s, &job.negative);
    s.update(&job.steps.to_le_bytes());
    s.update(&(job.scalars.len() as u64).to_le_bytes());
    for x in &job.scalars {
        s.update(&x.to_le_bytes());
    }
    s.update(&(job.images.len() as u64).to_le_bytes());
    for img in &job.images {
        s.update(&img.h.to_le_bytes()).update(&img.w.to_le_bytes()).update(&(img.rgb.len() as u64).to_le_bytes()).update(&img.rgb);
    }
    ids(&mut s, &job.generated);
    ids(&mut s, &job.source);
    ids(&mut s, &job.key);
    s.update(&(job.finalized.len() as u64).to_le_bytes());
    for ((c, st), v) in &job.finalized {
        s.update(&[*c, *st]);
        ids(&mut s, v);
    }
    finish(s)
}

/// **The pipeline's evidence object**: everything bound before the challenge.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PipelineEvidenceV1 {
    pub network_domain: Digest,
    pub ruleset_digest: Digest,
    pub class_binding_id: Digest,
    pub pipeline_root: Digest,
    pub plan_root: Digest,
    pub job_root: Digest,
    /// The commitment of `R`'s job seed and item (what [`RandomSource`] draws from).
    pub random_binding: Digest,
    pub stages: Vec<VerificationEvidenceV1>,
    pub output_root: Digest,
}

impl PipelineEvidenceV1 {
    pub fn root(&self) -> Digest {
        let mut s = keyed(PIPELINE_EVIDENCE_DOMAIN_V1);
        for d in [
            &self.network_domain,
            &self.ruleset_digest,
            &self.class_binding_id,
            &self.pipeline_root,
            &self.plan_root,
            &self.job_root,
            &self.random_binding,
        ] {
            s.update(d);
        }
        s.update(&(self.stages.len() as u64).to_le_bytes());
        for e in &self.stages {
            s.update(&e.root());
        }
        s.update(&self.output_root);
        finish(s)
    }
}

/// The class header a pipeline claim is about (borsh: the wire form `getPalwKernelClaim` serves beside the record, GAP 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PipelineHeaderV1 {
    pub network_domain: Digest,
    pub ruleset_digest: Digest,
    pub class_binding_id: Digest,
}

fn stage_header(
    h: &PipelineHeaderV1,
    prog: &TirProgramV2,
    params: &ParamCommitmentsV1,
    plan: &VerificationPlanV1,
) -> EvidenceHeaderV1 {
    EvidenceHeaderV1 {
        network_domain: h.network_domain,
        ruleset_digest: h.ruleset_digest,
        class_binding_id: h.class_binding_id,
        program_root: stage_program_root(prog),
        artifact_root: params.root(),
        plan_root: plan.root(),
    }
}

/// The output root: the output stage's output node commitment at its last position (`Final`), or at every position (`Rows`,
/// `Logits`).
pub fn pipeline_output_root_v1(p: &TirPipelineV1, programs: &[TirProgramV2], traces: &[EvidenceV1]) -> Option<Digest> {
    let si = p.output_stage as usize;
    let prog = &programs[p.stages.get(si)?.program as usize];
    let v = stage_view_v1(prog);
    let t = traces.get(si)?;
    let at = |pos: usize| t.commitments.get(pos)?.get(v.post_occurrence as usize)?.get(v.output_node as usize).copied();
    let mut s = keyed(PIPELINE_OUTPUT_DOMAIN_V1);
    match prog.output {
        OutputDecl::Final { .. } => {
            s.update(&[0]).update(&at(t.commitments.len().checked_sub(1)?)?);
        }
        OutputDecl::Rows { .. } | OutputDecl::Logits { .. } => {
            s.update(&[1]).update(&(t.commitments.len() as u64).to_le_bytes());
            for pos in 0..t.commitments.len() {
                s.update(&at(pos)?);
            }
        }
    }
    Some(finish(s))
}

/// **Build the pipeline's evidence object** (segments of `segment_len` positions in every stage).
#[allow(clippy::too_many_arguments)]
pub fn build_pipeline_evidence_v1(
    header: PipelineHeaderV1,
    descriptor: &KernelDescriptorV1,
    p: &TirPipelineV1,
    programs: &[TirProgramV2],
    plan: &PipelinePlanV1,
    params: &[ParamCommitmentsV1],
    trace: &PipelineTraceV1,
    job: &PipelineJob,
    random_binding: Digest,
    segment_len: u32,
) -> Result<PipelineEvidenceV1, String> {
    let mut stages = Vec::with_capacity(p.stages.len());
    let commitments: Vec<EvidenceV1> = trace.stages.iter().map(TraceV1::evidence).collect();
    for (si, st) in p.stages.iter().enumerate() {
        let prog = &programs[st.program as usize];
        let v = stage_view_v1(prog);
        let w = WiringV1::for_stage(&v.view, Some(&v.binding)).map_err(|e| e.to_string())?;
        let h = stage_header(&header, prog, &params[st.program as usize], &plan.stages[si]);
        stages.push(build_evidence_v1(&w, &commitments[si], &trace.tokens[si], h, descriptor, segment_len)?);
    }
    Ok(PipelineEvidenceV1 {
        network_domain: header.network_domain,
        ruleset_digest: header.ruleset_digest,
        class_binding_id: header.class_binding_id,
        pipeline_root: pipeline_root_v1(p, programs),
        plan_root: plan.root(),
        job_root: pipeline_job_root_v1(job),
        random_binding,
        stages,
        output_root: pipeline_output_root_v1(p, programs, &commitments).ok_or("no output")?,
    })
}

/// Everything public a pipeline claim's verification and its courts read.
pub struct PipelineContextV1<'a> {
    pub descriptor: &'a KernelDescriptorV1,
    pub pipeline: &'a TirPipelineV1,
    pub programs: &'a [TirProgramV2],
    pub plan: &'a PipelinePlanV1,
    pub header: PipelineHeaderV1,
    /// Each PROGRAM's param commitments, by program index.
    pub params: &'a [ParamCommitmentsV1],
    /// Each stage's committed values' commitments.
    pub traces: &'a [EvidenceV1],
    pub evidence: &'a PipelineEvidenceV1,
    pub job: &'a PipelineJob,
    pub random: &'a dyn RandomSource,
    pub random_binding: Digest,
    pub claim_id: Digest,
    pub beacon: Digest,
}

/// An edge fault: stage `stage`'s committed input `input` at `position` is not its binding's value. The court re-authenticates
/// `claimed` and every upstream value against the public commitments and recomputes the binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeFaultProofV1 {
    pub stage: u8,
    pub input: u16,
    pub position: u32,
    pub claimed: Tensor,
    /// For a `StageRows`/`StageFinal` binding, every output value of the upstream stage, in position order; else empty.
    pub upstream: Vec<Tensor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelineVerdictV1 {
    Pass { error_bits: u16, probabilistic_checks: u128, edges_checked: u64 },
    StageFault { stage: u8, proof: Box<KernelFaultProofV1> },
    EdgeFault(Box<EdgeFaultProofV1>),
    Unavailable { what: String },
    EvidenceMalformed { why: String },
    Inconsistent { why: String },
}

/// The challenge claim id of stage `si`: bound to the whole pipeline's evidence, so no stage's vectors exist before every stage
/// is committed.
pub fn stage_claim_id_v1(claim_id: &Digest, pipeline_evidence_root: &Digest, si: usize) -> Digest {
    let mut s = keyed(STAGE_CLAIM_DOMAIN_V1);
    s.update(claim_id).update(pipeline_evidence_root).update(&(si as u32).to_le_bytes());
    finish(s)
}

impl PipelineContextV1<'_> {
    fn view(&self, si: usize) -> StageViewV1 {
        stage_view_v1(&self.programs[self.pipeline.stages[si].program as usize])
    }

    /// The structural checks of the evidence object: roots, job, stage count, every stage's header and the output root.
    fn check_evidence(&self) -> Result<Vec<StageJobFacts>, String> {
        let e = self.evidence;
        let h = &self.header;
        if (e.network_domain, e.ruleset_digest, e.class_binding_id) != (h.network_domain, h.ruleset_digest, h.class_binding_id) {
            return Err("the evidence names another network, ruleset or class".into());
        }
        if e.pipeline_root != pipeline_root_v1(self.pipeline, self.programs) || e.plan_root != self.plan.root() {
            return Err("the evidence names another pipeline or plan".into());
        }
        if self.plan.descriptor_digest != self.descriptor.digest() {
            return Err("the pipeline plan is bound to another kernel descriptor".into());
        }
        if e.job_root != pipeline_job_root_v1(self.job) || e.random_binding != self.random_binding {
            return Err("the job or its R binding is not the one committed".into());
        }
        let n = self.pipeline.stages.len();
        if e.stages.len() != n || self.traces.len() != n || self.plan.stages.len() != n {
            return Err("a stage count differs".into());
        }
        if Some(e.output_root) != pipeline_output_root_v1(self.pipeline, self.programs, self.traces) {
            return Err("the output root is not the output stage's committed output".into());
        }
        let facts = stage_job_facts(self.pipeline, self.programs, self.job).map_err(|e| format!("the job: {e}"))?;
        for (si, f) in facts.iter().enumerate() {
            if self.traces[si].commitments.len() != f.trip as usize {
                return Err(format!("stage {si}: {} positions, the job fixes {}", self.traces[si].commitments.len(), f.trip));
            }
        }
        Ok(facts)
    }

    fn stage_ctx<'b>(&'b self, si: usize, v: &'b StageViewV1, tokens: &'b [u32]) -> ClaimContextV1<'b> {
        let st = &self.pipeline.stages[si];
        let prog = &self.programs[st.program as usize];
        let ev = &self.evidence.stages[si];
        ClaimContextV1 {
            descriptor: self.descriptor,
            program: &v.view,
            plan: &self.plan.stages[si],
            trace: &self.traces[si],
            evidence: ev,
            header: stage_header(&self.header, prog, &self.params[st.program as usize], &self.plan.stages[si]),
            params: &self.params[st.program as usize],
            tokens,
            binding: ChallengeBindingV1 {
                network_domain: self.header.network_domain,
                claim_id: stage_claim_id_v1(&self.claim_id, &self.evidence.root(), si),
                class_binding_id: self.header.class_binding_id,
                plan_root: self.plan.stages[si].root(),
                evidence_root: ev.root(),
                beacon: self.beacon,
            },
            stage: Some(&v.binding),
        }
    }

    /// The upstream output values of stage `u`, opened and authenticated.
    fn open_upstream(&self, materials: &[&dyn MaterialV1], u: u8) -> Result<Vec<Tensor>, String> {
        let v = self.view(u as usize);
        let t = self.traces.get(u as usize).ok_or("no such upstream stage")?;
        let m = materials.get(u as usize).ok_or("no material for the upstream stage")?;
        (0..t.commitments.len() as u32)
            .map(|pos| {
                let val = m
                    .node_value(pos, v.post_occurrence, v.output_node)
                    .ok_or_else(|| format!("stage {u} output at {pos} was not served"))?;
                match t.at(pos, v.post_occurrence, v.output_node) {
                    Some(c) if *c == tensor_commitment(&val) => Ok(val),
                    _ => Err(format!("stage {u} output at {pos}: the served value is not the committed one")),
                }
            })
            .collect()
    }
}

/// **The structural checks every pipeline court runs first** (the evidence object's roots, job, `R` binding and output, then every
/// stage's bindings, commitment shapes and derivable fields): a chain refuses at inclusion exactly what fails here.
pub fn pipeline_structure_v1(c: &PipelineContextV1<'_>) -> Result<(), String> {
    let facts = c.check_evidence()?;
    for (si, f) in facts.iter().enumerate() {
        let v = c.view(si);
        let tokens = run_tokens(f);
        crate::verify::claim_structure_v1(&c.stage_ctx(si, &v, &tokens)).map_err(|e| format!("stage {si}: {e}"))?;
    }
    Ok(())
}

/// **Verify a pipeline claim**: every stage's edges, then its relations, in stage order. `materials[s]` serves stage `s`.
pub fn verify_pipeline_v1(c: &PipelineContextV1<'_>, materials: &[&dyn MaterialV1]) -> PipelineVerdictV1 {
    let facts = match c.check_evidence() {
        Ok(f) => f,
        Err(why) => return PipelineVerdictV1::EvidenceMalformed { why },
    };
    if materials.len() != c.pipeline.stages.len() {
        return PipelineVerdictV1::EvidenceMalformed { why: "one material source per stage".into() };
    }
    let (mut checks, mut edges) = (0u128, 0u64);
    for si in 0..c.pipeline.stages.len() {
        let v = c.view(si);
        let tokens = run_tokens(&facts[si]);
        // Edges: every input at every position, authenticated and recomputed. The upstream values an edge copies are opened and
        // authenticated once per stage.
        let st = &c.pipeline.stages[si];
        let prog = &c.programs[st.program as usize];
        let mut upstream_rows: BTreeMap<u8, Vec<Tensor>> = BTreeMap::new();
        for k in 0..prog.inputs.len() {
            if let Some(Binding::StageRows { stage, .. } | Binding::StageFinal { stage }) = binding_of(st, prog, k)
                && !upstream_rows.contains_key(stage)
            {
                match c.open_upstream(materials, *stage) {
                    Ok(rows) => {
                        upstream_rows.insert(*stage, rows);
                    }
                    Err(what) => return PipelineVerdictV1::Unavailable { what },
                }
            }
        }
        let upstream = |u: u8| upstream_rows.get(&u).cloned();
        for k in 0..v.binding.inputs {
            for pos in 0..tokens.len() as u32 {
                edges += 1;
                let Some(served) = materials[si].stage_input(k, pos) else {
                    return PipelineVerdictV1::Unavailable { what: format!("stage {si} input {k} at {pos} was not served") };
                };
                if c.traces[si].input_at(pos, k) != Some(&tensor_commitment(&served)) {
                    return PipelineVerdictV1::Unavailable {
                        what: format!("stage {si} input {k} at {pos}: the served value is not the committed one"),
                    };
                }
                let expected = stage_input_v1(c.pipeline, c.programs, &facts, c.job, c.random, si, k, pos, &upstream);
                if expected.as_ref() != Ok(&served) {
                    let up = match binding_of(st, prog, k as usize) {
                        Some(Binding::StageRows { stage, .. } | Binding::StageFinal { stage }) => {
                            upstream_rows.get(stage).cloned().unwrap_or_default()
                        }
                        _ => Vec::new(),
                    };
                    return PipelineVerdictV1::EdgeFault(Box::new(EdgeFaultProofV1 {
                        stage: si as u8,
                        input: k,
                        position: pos,
                        claimed: served,
                        upstream: up,
                    }));
                }
            }
        }
        // The stage's relations.
        let ctx = c.stage_ctx(si, &v, &tokens);
        match verify_scope_v1(&ctx, materials[si], &ScopeV1::WholeClaim) {
            ScopeVerdictV1::Pass { probabilistic_checks, .. } => checks += probabilistic_checks,
            ScopeVerdictV1::Fault(proof) => return PipelineVerdictV1::StageFault { stage: si as u8, proof },
            ScopeVerdictV1::Unavailable { what } => return PipelineVerdictV1::Unavailable { what: format!("stage {si}: {what}") },
            ScopeVerdictV1::EvidenceMalformed { why } => {
                return PipelineVerdictV1::EvidenceMalformed { why: format!("stage {si}: {why}") };
            }
            ScopeVerdictV1::Inconsistent { why } => return PipelineVerdictV1::Inconsistent { why: format!("stage {si}: {why}") },
        }
    }
    PipelineVerdictV1::Pass {
        error_bits: derived_error_bits(c.descriptor, checks),
        probabilistic_checks: checks,
        edges_checked: edges,
    }
}

/// **The court of a stage's fault**, from public material.
pub fn verify_stage_fault_v1(
    c: &PipelineContextV1<'_>,
    stage: u8,
    proof: &KernelFaultProofV1,
) -> Result<crate::verify::ConvictionV1, crate::verify::DismissalV1> {
    use crate::verify::DismissalV1 as D;
    let facts = c.check_evidence().map_err(D::NotAuthentic)?;
    let si = stage as usize;
    if si >= c.pipeline.stages.len() {
        return Err(D::NotAuthentic("no such stage".into()));
    }
    let v = c.view(si);
    let tokens = run_tokens(&facts[si]);
    crate::verify::verify_fault_proof_v1(&c.stage_ctx(si, &v, &tokens), proof)
}

/// Why an edge accusation was dismissed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EdgeDismissalV1 {
    NotAuthentic(String),
    NoFault,
}

/// **The edge court**: authenticate the claimed input and the upstream values against public commitments, recompute the binding.
pub fn verify_edge_fault_v1(c: &PipelineContextV1<'_>, proof: &EdgeFaultProofV1) -> Result<(u8, u16, u32), EdgeDismissalV1> {
    verify_edge_value_v1(c, proof.stage, proof.input, proof.position, Some(&proof.claimed), &proof.upstream)
}

/// An input binding can be convicted by comparing its expected commitment, without opening the producer's input value.
pub fn verify_edge_commitment_fault_v1(
    c: &PipelineContextV1<'_>,
    stage: u8,
    input: u16,
    position: u32,
    upstream: &[Tensor],
) -> Result<(u8, u16, u32), EdgeDismissalV1> {
    verify_edge_value_v1(c, stage, input, position, None, upstream)
}

fn verify_edge_value_v1(
    c: &PipelineContextV1<'_>,
    stage: u8,
    input: u16,
    position: u32,
    claimed: Option<&Tensor>,
    upstream_values: &[Tensor],
) -> Result<(u8, u16, u32), EdgeDismissalV1> {
    use EdgeDismissalV1 as D;
    let facts = c.check_evidence().map_err(D::NotAuthentic)?;
    let si = stage as usize;
    let st = c.pipeline.stages.get(si).ok_or_else(|| D::NotAuthentic("no such stage".into()))?;
    let prog = &c.programs[st.program as usize];
    if input as usize >= prog.inputs.len() || position >= facts[si].trip {
        return Err(D::NotAuthentic("no such input or position".into()));
    }
    let committed = c.traces[si].input_at(position, input).ok_or_else(|| D::NotAuthentic("no committed input".into()))?;
    if claimed.is_some_and(|t| !crate::verify::canonical_tensor_v1(t)) {
        return Err(D::NotAuthentic("the input is not a canonical tensor".into()));
    }
    if claimed.is_some_and(|t| committed != &tensor_commitment(t)) {
        return Err(D::NotAuthentic("the claimed input is not the committed one".into()));
    }
    let up_stage = match binding_of(st, prog, input as usize) {
        Some(Binding::StageRows { stage, .. } | Binding::StageFinal { stage }) => Some(*stage),
        _ => None,
    };
    if let Some(u) = up_stage {
        let v = c.view(u as usize);
        let t = &c.traces[u as usize];
        if upstream_values.len() != t.commitments.len() {
            return Err(D::NotAuthentic("the proof opens another number of upstream values".into()));
        }
        for (pos, val) in upstream_values.iter().enumerate() {
            if !crate::verify::canonical_tensor_v1(val) {
                return Err(D::NotAuthentic(format!("upstream value {pos} is not a canonical tensor")));
            }
            if t.at(pos as u32, v.post_occurrence, v.output_node) != Some(&tensor_commitment(val)) {
                return Err(D::NotAuthentic(format!("upstream value {pos} is not the committed one")));
            }
        }
    } else if !upstream_values.is_empty() {
        return Err(D::NotAuthentic("a public job/random input has no upstream openings".into()));
    }
    let upstream = |u: u8| (Some(u) == up_stage).then(|| upstream_values.to_vec());
    match stage_input_v1(c.pipeline, c.programs, &facts, c.job, c.random, si, input, position, &upstream) {
        Ok(expected) if claimed.map_or_else(|| tensor_commitment(&expected) == *committed, |v| expected == *v) => Err(D::NoFault),
        _ => Ok((stage, input, position)),
    }
}

/// Exact local replay of every stage and edge, reading only the acquired parameters and public job/random inputs.
pub enum PipelineReexecutionV1 {
    Match { outputs: Vec<Vec<Tensor>> },
    StageFault { stage: u8, proof: KernelFaultProofV1 },
    EdgeFault { stage: u8, input: u16, position: u32, upstream: Vec<Tensor> },
}

pub fn reexecute_pipeline_v1(c: &PipelineContextV1<'_>, materials: &[&dyn MaterialV1]) -> Result<PipelineReexecutionV1, String> {
    pipeline_structure_v1(c)?;
    if materials.len() != c.pipeline.stages.len() {
        return Err("one acquired artifact source per stage is required".into());
    }
    let facts = c.check_evidence()?;
    let mut outputs: Vec<Vec<Tensor>> = Vec::new();
    for (si, st) in c.pipeline.stages.iter().enumerate() {
        let prog = &c.programs[st.program as usize];
        let upstream = |u: u8| outputs.get(u as usize).cloned();
        let mut inputs = BTreeMap::new();
        for position in 0..facts[si].trip {
            for input in 0..prog.inputs.len() as u16 {
                let value = stage_input_v1(c.pipeline, c.programs, &facts, c.job, c.random, si, input, position, &upstream);
                if !value.as_ref().is_ok_and(|v| c.traces[si].input_at(position, input) == Some(&tensor_commitment(v))) {
                    let upstream = match binding_of(st, prog, input as usize) {
                        Some(Binding::StageRows { stage, .. } | Binding::StageFinal { stage }) => {
                            outputs.get(*stage as usize).cloned().ok_or("no replay upstream output")?
                        }
                        _ => Vec::new(),
                    };
                    verify_edge_commitment_fault_v1(c, si as u8, input, position, &upstream)
                        .map_err(|e| format!("replay edge proof did not authenticate: {e:?}"))?;
                    return Ok(PipelineReexecutionV1::EdgeFault { stage: si as u8, input, position, upstream });
                }
                inputs.insert((input, position), value?);
            }
        }
        struct ReplayInputs<'a> {
            source: &'a dyn MaterialV1,
            inputs: BTreeMap<(u16, u32), Tensor>,
        }
        impl MaterialV1 for ReplayInputs<'_> {
            fn node_value(&self, _: u32, _: u16, _: u16) -> Option<Tensor> {
                unreachable!("local stage replay")
            }
            fn param(&self, i: u16, l: Option<u16>) -> Option<Tensor> {
                self.source.param(i, l)
            }
            fn stage_input(&self, k: u16, p: u32) -> Option<Tensor> {
                self.inputs.get(&(k, p)).cloned()
            }
        }
        let material = ReplayInputs { source: materials[si], inputs };
        let view = c.view(si);
        let tokens = run_tokens(&facts[si]);
        match crate::verify::reexecute_claim_v1(&c.stage_ctx(si, &view, &tokens), &material)? {
            crate::verify::ReexecutionV1::Fault(proof) => return Ok(PipelineReexecutionV1::StageFault { stage: si as u8, proof }),
            crate::verify::ReexecutionV1::Match { logits, .. } => outputs.push(logits),
        }
    }
    Ok(PipelineReexecutionV1::Match { outputs })
}

/// The material of honest stage traces (tests, drills, a producer serving itself).
pub struct PipelineMaterialV1<'a> {
    pub trace: &'a TraceV1,
    pub params: &'a misaka_palw_tir::MapParams,
}

impl MaterialV1 for PipelineMaterialV1<'_> {
    fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        self.trace.value(p, s, n).cloned()
    }
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.params.tensors.get(&(index, layer)).cloned()
    }
    fn stage_input(&self, k: u16, position: u32) -> Option<Tensor> {
        self.trace.input(position, k).cloned()
    }
}
