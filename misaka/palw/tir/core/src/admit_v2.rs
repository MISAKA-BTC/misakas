//! **Admission of version-2 programs and pipelines** (spec 04b §15.9).
//!
//! A version-2 program is admitted by version 1's analyses, unchanged, over its view (§15.3), with
//! the two facts the view cannot carry supplied by version 2:
//!
//! * **its inputs' intervals** (§15.5) — `[lo, hi]`, the word range, or the Gaussian table's ends — in
//!   place of a param's full dtype range; and
//! * **how each input is opened at the court** ([`ParamLeafV1`]): an external input bound to an
//!   earlier stage is a committed value (4 bytes a lane, a committed operand), one bound to job data
//!   is 4 bytes a lane and no operand, one bound to a job image is 1 byte a lane opened against the
//!   image's `input_root` (an operand), and a random input or a row or token count is derived by the
//!   court itself and opens nothing. A program admitted on its own, without a pipeline, takes the
//!   conservative reading: every external input committed, every random input derived.
//!
//! Every `post` write is a commit point (NF-29), so the view's commit points are the program's and
//! its cones are the court's. A state `post` writes is a leaf at every position (the committed
//! write of the position before), so it has no replay and no checkpoint interval of its own.
//!
//! A **pipeline** is admitted stage by stage — each stage's program under the per-position ceilings,
//! its inputs opened as its bindings say — and then as a job: every stage's per-position cost and
//! step leaves times its `max_trip`, summed, against the job ceilings ([`TirJobCeilingsV1`]; the
//! network's per-profile caps). Its edges were proved by [`validate_pipeline`] (NF-P1 … NF-P9).

use crate::admit::{
    CostV1, ParamLeafV1, TirAdmissionV1, TirAdmitError, TirAdmitInputsV1, TirDemandRulesV1, admit_core, check_admit_inputs,
};
use crate::interval::Interval;
use crate::interval_v2::analyze_ranges_v2;
use crate::pipeline::{Binding, TirPipelineV1};
use crate::program_v2::{InputSource, TirProgramV2};
use crate::validate_v2::validate_v2;

/// One input as admission read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputAdmissionV2 {
    /// Every element's interval (§15.5).
    pub interval: Interval,
    /// How the court opens it.
    pub leaf: ParamLeafV1,
}

/// Everything admission derived for a version-2 program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirAdmissionV2 {
    pub program: TirProgramV2,
    /// Admission of the view: intervals, costs, cones and checkpoint intervals, by the program's
    /// own block and node indices (the view keeps them).
    pub view: TirAdmissionV1,
    /// The view's param index of input 0.
    pub first_input_param: u16,
    pub inputs: Vec<InputAdmissionV2>,
    /// The global `Fixed` states `post` writes, ascending: each is committed at every position.
    pub post_written: Vec<u16>,
}

/// The conservative opening of each input, without a pipeline.
fn standalone_leaves(p: &TirProgramV2) -> Vec<ParamLeafV1> {
    p.inputs
        .iter()
        .map(|d| match d.source {
            InputSource::External { .. } => ParamLeafV1::Committed,
            InputSource::Random { .. } => ParamLeafV1::Derived,
        })
        .collect()
}

fn admit_with(
    p: &TirProgramV2,
    inputs: &TirAdmitInputsV1,
    leaves: &[ParamLeafV1],
    rules: TirDemandRulesV1,
) -> Result<TirAdmissionV2, TirAdmitError> {
    check_admit_inputs(inputs)?;
    let info = validate_v2(p)?;
    let intervals = analyze_ranges_v2(p)?;
    let first = info.first_input_param;
    let param_leaf = |j: u16| if j < first { ParamLeafV1::Artifact } else { leaves[(j - first) as usize] };
    let view = admit_core(info.view.clone(), info.v1.clone(), intervals, inputs, &param_leaf, rules)?;
    let mut post_written: Vec<u16> = info.post_writes.iter().map(|(_, s)| *s).collect();
    post_written.sort_unstable();
    let inputs = p
        .inputs
        .iter()
        .zip(leaves)
        .map(|(d, leaf)| {
            let (lo, hi) = d.interval();
            InputAdmissionV2 { interval: Interval::new(lo, hi), leaf: *leaf }
        })
        .collect();
    Ok(TirAdmissionV2 { program: p.clone(), view, first_input_param: first, inputs, post_written })
}

/// **`tir_admit_v2`**: admit the canonical bytes of a version-2 program on its own, or refuse them
/// by rule and number.
pub fn tir_admit_v2(program_bytes: &[u8], inputs: &TirAdmitInputsV1) -> Result<TirAdmissionV2, TirAdmitError> {
    check_admit_inputs(inputs)?;
    let p = TirProgramV2::decode_canonical(program_bytes)?;
    admit_with(&p, inputs, &standalone_leaves(&p), TirDemandRulesV1::Release2000)
}

/// [`tir_admit_v2`] of a program already in memory (its canonical encoding).
pub fn tir_admit_program_v2(p: &TirProgramV2, inputs: &TirAdmitInputsV1) -> Result<TirAdmissionV2, TirAdmitError> {
    tir_admit_v2(&p.encode(), inputs)
}

/// The network's per-job caps for a pipeline (per profile: the fence `palw_gen_v1` carries one set
/// per profile).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirJobCeilingsV1 {
    /// `Σ` over stages of `max_trip ×` the stage's per-position MACs.
    pub max_job_macs: u64,
    pub max_job_transcendentals: u64,
    /// `Σ` over stages of `max_trip ×` the stage's per-position step leaves.
    pub max_job_step_leaves: u64,
    /// Admission's own work over every stage.
    pub max_job_cone_work: u64,
}

impl TirJobCeilingsV1 {
    /// Caps no honest pipeline of the fixtures reaches.
    pub const fn open_v1() -> Self {
        Self { max_job_macs: 1 << 50, max_job_transcendentals: 1 << 40, max_job_step_leaves: 1 << 32, max_job_cone_work: 1 << 22 }
    }
}

/// One stage as admitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageAdmissionV1 {
    pub stage: u8,
    pub max_trip: u32,
    pub admission: TirAdmissionV2,
    /// `max_trip ×` the per-position cost.
    pub job_cost: CostV1,
    /// `max_trip ×` the per-position step leaves.
    pub job_step_leaves: u64,
}

/// Everything admission derived for a pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirPipelineAdmissionV1 {
    pub pipeline: TirPipelineV1,
    pub programs: Vec<TirProgramV2>,
    pub stages: Vec<StageAdmissionV1>,
    pub job_cost: CostV1,
    pub job_step_leaves: u64,
    pub cone_work: u64,
    /// The proven interval of the class's output — the domain its output kind's value domain must
    /// contain (RFC-0003 §I.3.2, PALW-OUT-2; checked by the class object).
    pub output_interval: Interval,
}

fn scaled(c: &CostV1, k: u64) -> CostV1 {
    CostV1 {
        macs: c.macs.saturating_mul(k),
        elementwise: c.elementwise.saturating_mul(k),
        transcendentals: c.transcendentals.saturating_mul(k),
        bytes_read: c.bytes_read.saturating_mul(k),
        bytes_written: c.bytes_written.saturating_mul(k),
    }
}

fn sum(a: &CostV1, b: &CostV1) -> CostV1 {
    CostV1 {
        macs: a.macs.saturating_add(b.macs),
        elementwise: a.elementwise.saturating_add(b.elementwise),
        transcendentals: a.transcendentals.saturating_add(b.transcendentals),
        bytes_read: a.bytes_read.saturating_add(b.bytes_read),
        bytes_written: a.bytes_written.saturating_add(b.bytes_written),
    }
}

fn exceeds(limit: &'static str, value: u64, cap: u64) -> Result<(), TirAdmitError> {
    if value > cap { Err(TirAdmitError::Exceeds { limit, at: "the job".into(), value, cap }) } else { Ok(()) }
}

/// **`tir_admit_pipeline_v1`**: admit a pipeline's canonical bytes over its programs' canonical
/// bytes: every program decoded (§15.1), the pipeline decoded and its edges proved (§15.6), every
/// stage admitted with its inputs opened as bound, and the job's totals against `job`. Every stage
/// is admitted under the same network inputs; [`tir_admit_pipeline_staged_v1`] takes one per stage.
pub fn tir_admit_pipeline_v1(
    pipeline_bytes: &[u8],
    program_bytes: &[Vec<u8>],
    inputs: &TirAdmitInputsV1,
    job: &TirJobCeilingsV1,
) -> Result<TirPipelineAdmissionV1, TirAdmitError> {
    check_admit_inputs(inputs)?;
    let (programs, pipeline) = decode_pipeline(pipeline_bytes, program_bytes)?;
    admit_pipeline(programs, pipeline, &|_| inputs, job, TirDemandRulesV1::Release2000)
}

/// **`tir_admit_pipeline_staged_v1`**: [`tir_admit_pipeline_v1`] with each stage's own network
/// inputs — `stage_inputs[s]` carries stage `s`'s tile length, history chunk and checkpoint interval,
/// since a class commits each stage under its own layout (RFC-0003 §I.2.3, Phase F D5 per stage).
/// Refused unless there is exactly one per stage.
pub fn tir_admit_pipeline_staged_v1(
    pipeline_bytes: &[u8],
    program_bytes: &[Vec<u8>],
    stage_inputs: &[TirAdmitInputsV1],
    job: &TirJobCeilingsV1,
) -> Result<TirPipelineAdmissionV1, TirAdmitError> {
    tir_admit_pipeline_staged_with_rules_v1(pipeline_bytes, program_bytes, stage_inputs, job, TirDemandRulesV1::Release2000)
}

/// [`tir_admit_pipeline_staged_v1`] under the box-demand rules `rules` — the registering block's
/// (ref2's H7 `TopK` row past `palw_tir_fence2`).
pub fn tir_admit_pipeline_staged_with_rules_v1(
    pipeline_bytes: &[u8],
    program_bytes: &[Vec<u8>],
    stage_inputs: &[TirAdmitInputsV1],
    job: &TirJobCeilingsV1,
    rules: TirDemandRulesV1,
) -> Result<TirPipelineAdmissionV1, TirAdmitError> {
    for inputs in stage_inputs {
        check_admit_inputs(inputs)?;
    }
    let (programs, pipeline) = decode_pipeline(pipeline_bytes, program_bytes)?;
    if stage_inputs.len() != pipeline.stages.len() {
        return Err(TirAdmitError::Inputs("one set of network inputs per stage"));
    }
    admit_pipeline(programs, pipeline, &|s| &stage_inputs[s], job, rules)
}

/// Every program decoded strictly (§15.1), then the pipeline over them (§15.6).
fn decode_pipeline(pipeline_bytes: &[u8], program_bytes: &[Vec<u8>]) -> Result<(Vec<TirProgramV2>, TirPipelineV1), TirAdmitError> {
    let programs = program_bytes.iter().map(|b| TirProgramV2::decode_canonical(b)).collect::<Result<Vec<_>, _>>()?;
    let pipeline = TirPipelineV1::decode_canonical(pipeline_bytes, &programs)?;
    Ok((programs, pipeline))
}

fn admit_pipeline<'a>(
    programs: Vec<TirProgramV2>,
    pipeline: TirPipelineV1,
    inputs_of: &dyn Fn(usize) -> &'a TirAdmitInputsV1,
    job: &TirJobCeilingsV1,
    rules: TirDemandRulesV1,
) -> Result<TirPipelineAdmissionV1, TirAdmitError> {
    let mut stages = Vec::with_capacity(pipeline.stages.len());
    let (mut job_cost, mut job_step_leaves, mut cone_work) = (CostV1::default(), 0u64, 0u64);
    for (s, st) in pipeline.stages.iter().enumerate() {
        let prog = &programs[st.program as usize];
        let inputs = inputs_of(s);
        let mut bindings = st.bind.iter();
        let leaves: Vec<ParamLeafV1> = prog
            .inputs
            .iter()
            .map(|d| match d.source {
                InputSource::Random { .. } => ParamLeafV1::Derived,
                InputSource::External { .. } => match bindings.next() {
                    Some(Binding::StageRows { .. } | Binding::StageFinal { .. }) => ParamLeafV1::Committed,
                    Some(Binding::JobScalar { .. } | Binding::JobTokens { .. }) => ParamLeafV1::JobData,
                    Some(Binding::StageRowCount { .. } | Binding::JobTokenCount { .. }) => ParamLeafV1::Derived,
                    Some(Binding::JobImage { .. }) => ParamLeafV1::JobImage,
                    // Validated: one binding per external input.
                    None => ParamLeafV1::Committed,
                },
            })
            .collect();
        let admission = admit_with(prog, inputs, &leaves, rules).map_err(|e| match e {
            TirAdmitError::Exceeds { limit, at, value, cap } => {
                TirAdmitError::Exceeds { limit, at: format!("stage {s} ({}): {at}", st.name), value, cap }
            }
            other => other,
        })?;
        let trip = st.max_trip as u64;
        let stage_cost = scaled(&admission.view.position.cost, trip);
        let stage_leaves = admission.view.position.step_leaves.saturating_mul(trip);
        job_cost = sum(&job_cost, &stage_cost);
        job_step_leaves = job_step_leaves.saturating_add(stage_leaves);
        cone_work = cone_work.saturating_add(admission.view.cone_work);
        stages.push(StageAdmissionV1 {
            stage: s as u8,
            max_trip: st.max_trip,
            admission,
            job_cost: stage_cost,
            job_step_leaves: stage_leaves,
        });
    }
    exceeds("max_job_macs", job_cost.macs, job.max_job_macs)?;
    exceeds("max_job_transcendentals", job_cost.transcendentals, job.max_job_transcendentals)?;
    exceeds("max_job_step_leaves", job_step_leaves, job.max_job_step_leaves)?;
    exceeds("max_job_cone_work", cone_work, job.max_job_cone_work)?;
    let out = &stages[pipeline.output_stage as usize].admission;
    let output_interval = out.view.intervals[out.program.schedule.post as usize][out.program.output.node() as usize];
    Ok(TirPipelineAdmissionV1 { pipeline, programs, stages, job_cost, job_step_leaves, cone_work, output_interval })
}
