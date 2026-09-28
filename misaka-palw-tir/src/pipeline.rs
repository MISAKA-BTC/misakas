//! **Pipelines of programs** (RFC-0003 §I.2.3, spec 04b §15.6) — the IR half.
//!
//! A generative class is several programs run in sequence, each over its own scan: a text encoder
//! over the prompt's tokens, a denoiser over its steps, a decoder at one position. A pipeline is
//! **not a VM**: its stages run in declared order, always, each over a trip count fixed when the job
//! is accepted, and nothing runs conditionally. Stages meet only through **structural edges** — an
//! input element of a later stage is one committed element of an earlier stage's output, a job
//! value, or a zero pad — so an edge has no arithmetic to adjudicate, and every leaf a court opens
//! precedes the leaf it adjudicates (Phase F's one-tree invariant, stage-major).
//!
//! This module is what the reference evaluator needs: the structure ([`TirPipelineV1`]), its normal
//! form ([`validate_pipeline`], with every edge's interval proved statically) and a run
//! ([`run_pipeline`]). The class object — identity, per-stage layouts, the artifact, registration,
//! the step tree and the court — is consensus's, and comes with `palw_gen_v1`.

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};

use crate::error::{TirError, TirErrorKind, TirResult, err};
use crate::interp::ParamSource;
use crate::interp_v2::{InputProvider, InterpreterV2, StepOutputV2};
use crate::interval_v2::output_interval_v2;
use crate::program::{MAX_NAME_BYTES, Ref};
use crate::program_v2::*;
use crate::tensor::Tensor;
use crate::types::{DType, Dim};
use crate::validate_v2::validate_v2;

pub const TIR_PIPELINE_VERSION_V1: u16 = 1;
pub const MAX_STAGES: usize = 16;
/// Job scalars a pipeline may bind (`Binding::JobScalar { index }`, `index < 16`).
pub const MAX_JOB_SCALARS: usize = 16;
/// A token template's prefix or suffix is at most this long.
pub const MAX_TEMPLATE_TOKENS: usize = 4096;

/// A stage's trip count `T`. Tags: `Fixed 0`, `JobSteps 1`, `TokenCount 2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TripRule {
    /// Always `n` positions (a decoder: `n = 1`).
    Fixed { n: u32 },
    /// The job's step count (a denoiser).
    JobSteps,
    /// The length of the stage's token sequence (an encoder over the prompt).
    TokenCount,
}

/// Which of the job's token lists a template wraps. Tags: `Prompt 0`, `Negative 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TokenSource {
    Prompt,
    Negative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenPad {
    pub id: u32,
    pub to_len: u32,
}

/// `prefix ‖ source ‖ suffix`, padded with `pad.id` to `pad.to_len` when there is a pad — a class's
/// fixed prompt template (the job carries only the user's ids).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenRule {
    pub prefix: Vec<u32>,
    pub source: TokenSource,
    pub suffix: Vec<u32>,
    pub pad: Option<TokenPad>,
}

/// Where an external input's value comes from. Tags: `JobScalar 0`, `JobTokens 1`, `StageRows 2`,
/// `StageFinal 3`, `StageRowCount 4`, `JobTokenCount 5`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Binding {
    /// `job.scalars[index]`, a rank-0 input.
    JobScalar { index: u8 },
    /// The template applied to the job's ids, padded to `pad.to_len`: an `idx` input `[to_len]`.
    JobTokens { rule: TokenRule },
    /// An earlier `Rows` stage's rows `drop..T`, zero-padded to `pad_to`: `[pad_to] ++ row shape`.
    StageRows { stage: u8, drop: u32, pad_to: u32 },
    /// An earlier `Final` stage's output.
    StageFinal { stage: u8 },
    /// `max(T − drop, 0)` of an earlier `Rows` stage, a rank-0 `idx` (the rows a mask admits).
    StageRowCount { stage: u8, drop: u32 },
    /// `|prefix ‖ ids ‖ suffix|` — the template's length BEFORE padding, a rank-0 `idx`: the tokens a
    /// bidirectional encoder's mask admits (`Compare(Iota < count)`), whatever ids the pad uses.
    JobTokenCount { rule: TokenRule },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageDecl {
    pub name: String,
    /// An index into the pipeline's program list.
    pub program: u16,
    pub trip: TripRule,
    /// The largest `T` the stage admits (`n` for `Fixed`, `pad.to_len` for a padded token run).
    pub max_trip: u32,
    /// `Input(0)` at each position; present exactly when the program reads the token.
    pub tokens: Option<TokenRule>,
    /// One binding per external input of the program, in declaration order.
    pub bind: Vec<Binding>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TirPipelineV1 {
    pub version: u16,
    pub stages: Vec<StageDecl>,
    /// The stage whose output is the class's output (a `Rows` or `Final` program).
    pub output_stage: u8,
}

impl TirPipelineV1 {
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("encoding into a Vec cannot fail")
    }

    /// Decode bytes that must be the unique encoding of a pipeline in normal form over `programs`:
    /// within [`MAX_PIPELINE_BYTES`], strict Borsh, re-encoding byte-identical, and
    /// [`validate_pipeline`] passing (spec 04b §15.6).
    pub fn decode_canonical(bytes: &[u8], programs: &[TirProgramV2]) -> TirResult<Self> {
        if bytes.len() > MAX_PIPELINE_BYTES {
            return err(TirErrorKind::Encoding, format!("{} bytes exceed the {MAX_PIPELINE_BYTES}-byte cap", bytes.len()));
        }
        let p: TirPipelineV1 = borsh::from_slice(bytes).map_err(|e| TirError::new(TirErrorKind::Encoding, e.to_string()))?;
        if p.encode() != bytes {
            return err(TirErrorKind::Encoding, "re-encoding differs: not the canonical encoding");
        }
        validate_pipeline(&p, programs)?;
        Ok(p)
    }
}

/// A pipeline's encoding is small (stages reference programs by index): 64 KiB is generous.
pub const MAX_PIPELINE_BYTES: usize = 64 * 1024;

/// The job facts a pipeline reads (the profile layer maps its job body onto these).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PipelineJob {
    pub prompt: Vec<u32>,
    pub negative: Vec<u32>,
    pub steps: u32,
    pub scalars: Vec<i64>,
}

/// Random inputs: `dist(R(seed, domain, step, position, lane))` for lanes `0..Π shape`, with the
/// job's seed and item index — computed by the caller (RFC-0003 §I.1; `misaka-palw-gen`).
pub trait RandomSource {
    fn random(&self, domain: u16, dist: RandomDist, step: u32, shape: &[u32]) -> Option<Tensor>;
}

/// Each program's params (the pipeline's artifact, by program).
pub trait PipelineParams {
    fn params(&self, program: u16) -> &dyn ParamSource;
}

fn nf<T>(msg: impl Into<String>) -> TirResult<T> {
    err(TirErrorKind::NormalForm, msg)
}

fn reads_token(p: &TirProgramV2) -> bool {
    p.blocks.iter().any(|b| b.nodes.iter().any(|n| n.inputs.contains(&Ref::Input(0))))
}

fn output_shape(p: &TirProgramV2) -> Vec<u32> {
    let node = &p.blocks[p.schedule.post as usize].nodes[p.output.node() as usize];
    node.out.shape.iter().map(|d| if let Dim::Fixed(n) = d { *n } else { 0 }).collect()
}

fn check_rule(what: &str, rule: &TokenRule, token_bound: u32) -> TirResult<()> {
    if rule.prefix.len() > MAX_TEMPLATE_TOKENS || rule.suffix.len() > MAX_TEMPLATE_TOKENS {
        return nf(format!("{what}: a template side is at most {MAX_TEMPLATE_TOKENS} tokens"));
    }
    let pad_id = rule.pad.map(|p| p.id);
    if let Some(t) = rule.prefix.iter().chain(&rule.suffix).chain(pad_id.iter()).find(|t| **t >= token_bound) {
        return nf(format!("{what}: template token {t} ≥ token_bound {token_bound}"));
    }
    if let Some(pad) = rule.pad
        && (pad.to_len as usize) < rule.prefix.len() + rule.suffix.len()
    {
        return nf(format!("{what}: pad length {} is shorter than the template", pad.to_len));
    }
    Ok(())
}

/// What validation learned: each stage's external inputs, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineInfo {
    pub externals: Vec<Vec<u16>>,
}

/// **Normal form of a pipeline** (NF-P1 … NF-P9, spec 04b §15.6), every edge's shape, dtype and
/// interval proved statically.
pub fn validate_pipeline(p: &TirPipelineV1, programs: &[TirProgramV2]) -> TirResult<PipelineInfo> {
    if p.version != TIR_PIPELINE_VERSION_V1 {
        return nf(format!("pipeline version {} is not {TIR_PIPELINE_VERSION_V1}", p.version));
    }
    if p.stages.is_empty() || p.stages.len() > MAX_STAGES {
        return nf(format!("1..={MAX_STAGES} stages"));
    }
    if p.output_stage as usize >= p.stages.len() {
        return nf("the output stage does not exist");
    }
    for prog in programs {
        validate_v2(prog)?;
    }
    let mut names = BTreeSet::new();
    let mut domains = BTreeSet::new();
    let mut consumed = vec![false; p.stages.len()];
    let mut externals = Vec::with_capacity(p.stages.len());
    for (s, st) in p.stages.iter().enumerate() {
        let what = format!("stage {s} ({})", st.name);
        if st.name.is_empty() || st.name.len() > MAX_NAME_BYTES || !names.insert(st.name.as_str()) {
            return nf(format!("{what}: stage names are 1..={MAX_NAME_BYTES} bytes and unique"));
        }
        let prog = programs
            .get(st.program as usize)
            .ok_or_else(|| TirError::new(TirErrorKind::NormalForm, format!("{what}: no program {}", st.program)))?;
        // NF-P3: the trip rule, its maximum, and the token rule.
        if st.max_trip == 0 || st.max_trip > prog.history_bound {
            return nf(format!("{what}: max_trip {} outside [1, history_bound]", st.max_trip));
        }
        match (st.trip, &st.tokens, reads_token(prog)) {
            (TripRule::Fixed { n }, None, false) if n == st.max_trip => {}
            (TripRule::JobSteps, None, false) => {}
            (TripRule::TokenCount, Some(rule), true) => {
                check_rule(&what, rule, prog.token_bound)?;
                if let Some(pad) = rule.pad
                    && pad.to_len != st.max_trip
                {
                    return nf(format!("{what}: a padded token run has max_trip = its pad length"));
                }
            }
            _ => {
                return nf(format!(
                    "{what}: Fixed (with max_trip = n) and JobSteps stages read no token; a TokenCount stage reads the token and has a token rule"
                ));
            }
        }
        // NF-P4: one random input per domain across the pipeline.
        for inp in &prog.inputs {
            if let InputSource::Random { domain, .. } = inp.source
                && !domains.insert(domain)
            {
                return nf(format!("{what}: domain {domain} is drawn by two random inputs of the pipeline"));
            }
        }
        // NF-P5 … P8: one binding per external input, each proved.
        let ext: Vec<u16> = prog.inputs.iter().enumerate().filter(|(_, i)| i.is_external()).map(|(k, _)| k as u16).collect();
        if st.bind.len() != ext.len() {
            return nf(format!("{what}: {} bindings for {} external inputs", st.bind.len(), ext.len()));
        }
        for (b, k) in st.bind.iter().zip(&ext) {
            let d = &prog.inputs[*k as usize];
            let (lo, hi) = d.interval();
            let edge = format!("{what} input {} ({})", k, d.name);
            let upstream = |u: u8| -> TirResult<(&StageDecl, &TirProgramV2)> {
                if u as usize >= s {
                    return nf(format!("{edge}: an edge reads only an earlier stage"));
                }
                let us = &p.stages[u as usize];
                Ok((us, &programs[us.program as usize]))
            };
            match b {
                Binding::JobScalar { index } => {
                    if *index as usize >= MAX_JOB_SCALARS || !d.shape.is_empty() {
                        return nf(format!("{edge}: a job scalar is a rank-0 input, index < {MAX_JOB_SCALARS}"));
                    }
                }
                Binding::JobTokens { rule } => {
                    check_rule(&edge, rule, u32::MAX)?;
                    let Some(pad) = rule.pad else { return nf(format!("{edge}: a token tensor is padded to its length")) };
                    if d.dtype != DType::Idx || d.shape != [pad.to_len] {
                        return nf(format!("{edge}: a token tensor is idx [pad length]"));
                    }
                    if rule.prefix.iter().chain(&rule.suffix).chain([pad.id].iter()).any(|t| (*t as i128) < lo || (*t as i128) > hi) {
                        return nf(format!("{edge}: a template token is outside [{lo}, {hi}]"));
                    }
                }
                Binding::StageRows { stage, drop, pad_to } => {
                    let (us, up) = upstream(*stage)?;
                    consumed[*stage as usize] = true;
                    if !matches!(up.output, OutputDecl::Rows { .. }) {
                        return nf(format!("{edge}: StageRows reads a Rows stage"));
                    }
                    let mut shape = vec![*pad_to];
                    shape.extend(output_shape(up));
                    let dtype = up.blocks[up.schedule.post as usize].nodes[up.output.node() as usize].out.dtype;
                    if d.shape != shape || d.dtype != dtype {
                        return nf(format!("{edge}: StageRows gives {} {shape:?}", dtype.name()));
                    }
                    if *pad_to == 0 || (*pad_to as u64) < us.max_trip.saturating_sub(*drop) as u64 {
                        return nf(format!("{edge}: pad_to {pad_to} cannot hold {} rows", us.max_trip.saturating_sub(*drop)));
                    }
                    let iv = output_interval_v2(up)?;
                    if iv.lo.min(0) < lo || iv.hi.max(0) > hi {
                        return nf(format!("{edge}: the rows' interval [{}, {}] and the zero pad exceed [{lo}, {hi}]", iv.lo, iv.hi));
                    }
                }
                Binding::StageFinal { stage } => {
                    let (_, up) = upstream(*stage)?;
                    consumed[*stage as usize] = true;
                    if !matches!(up.output, OutputDecl::Final { .. }) {
                        return nf(format!("{edge}: StageFinal reads a Final stage"));
                    }
                    let dtype = up.blocks[up.schedule.post as usize].nodes[up.output.node() as usize].out.dtype;
                    if d.shape != output_shape(up) || d.dtype != dtype {
                        return nf(format!("{edge}: StageFinal gives {} {:?}", dtype.name(), output_shape(up)));
                    }
                    let iv = output_interval_v2(up)?;
                    if iv.lo < lo || iv.hi > hi {
                        return nf(format!("{edge}: the output interval [{}, {}] exceeds [{lo}, {hi}]", iv.lo, iv.hi));
                    }
                }
                Binding::StageRowCount { stage, drop } => {
                    let (us, up) = upstream(*stage)?;
                    consumed[*stage as usize] = true;
                    if !matches!(up.output, OutputDecl::Rows { .. }) {
                        return nf(format!("{edge}: StageRowCount counts a Rows stage"));
                    }
                    if d.dtype != DType::Idx || !d.shape.is_empty() {
                        return nf(format!("{edge}: a row count is a rank-0 idx"));
                    }
                    if lo > 0 || hi < us.max_trip.saturating_sub(*drop) as i128 {
                        return nf(format!("{edge}: [0, {}] exceeds [{lo}, {hi}]", us.max_trip.saturating_sub(*drop)));
                    }
                }
                Binding::JobTokenCount { rule } => {
                    check_rule(&edge, rule, u32::MAX)?;
                    let Some(pad) = rule.pad else { return nf(format!("{edge}: a token count is of a padded template")) };
                    if d.dtype != DType::Idx || !d.shape.is_empty() {
                        return nf(format!("{edge}: a token count is a rank-0 idx"));
                    }
                    if lo > 0 || hi < pad.to_len as i128 {
                        return nf(format!("{edge}: [0, {}] exceeds [{lo}, {hi}]", pad.to_len));
                    }
                }
            }
        }
        externals.push(ext);
    }
    // NF-P9: the output stage ends in a tensor, and no other stage is dead.
    let out_prog = &programs[p.stages[p.output_stage as usize].program as usize];
    if matches!(out_prog.output, OutputDecl::Logits { .. }) {
        return nf("the output stage is a Rows or Final program");
    }
    if let Some(dead) = (0..p.stages.len()).find(|s| *s != p.output_stage as usize && !consumed[*s]) {
        return nf(format!("stage {dead} feeds no later stage and is not the output"));
    }
    Ok(PipelineInfo { externals })
}

/// One stage of a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageRun {
    pub trip: u32,
    /// `Input(0)` at each position (empty for a stage that reads no token).
    pub tokens: Vec<u32>,
    pub steps: Vec<StepOutputV2>,
}

impl StageRun {
    /// The output node's value at every position.
    pub fn rows(&self) -> Vec<Tensor> {
        self.steps.iter().map(|s| s.output.clone()).collect()
    }
    /// The output node's value at the last position.
    pub fn last(&self) -> Option<&Tensor> {
        self.steps.last().map(|s| &s.output)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineRun {
    pub stages: Vec<StageRun>,
    /// The output stage's result: a `Final` stage's last value, or a `Rows` stage's rows stacked
    /// `[T] ++ row shape`.
    pub output: Tensor,
}

/// How many pad ids `apply_rule` appended.
fn pad_count(rule: &TokenRule, job: &PipelineJob) -> usize {
    let src = match rule.source {
        TokenSource::Prompt => job.prompt.len(),
        TokenSource::Negative => job.negative.len(),
    };
    let unpadded = rule.prefix.len() + src + rule.suffix.len();
    rule.pad.map_or(0, |p| (p.to_len as usize).saturating_sub(unpadded))
}

fn apply_rule(rule: &TokenRule, job: &PipelineJob) -> TirResult<Vec<u32>> {
    let src = match rule.source {
        TokenSource::Prompt => &job.prompt,
        TokenSource::Negative => &job.negative,
    };
    let mut seq: Vec<u32> = rule.prefix.iter().chain(src.iter()).chain(rule.suffix.iter()).copied().collect();
    if let Some(pad) = rule.pad {
        if seq.len() > pad.to_len as usize {
            return err(TirErrorKind::Operand, format!("{} tokens exceed the template's {}", seq.len(), pad.to_len));
        }
        seq.resize(pad.to_len as usize, pad.id);
    }
    Ok(seq)
}

/// A stage's inputs: external ones computed once, random ones drawn per position when per-step.
struct StageInputs<'a> {
    constant: BTreeMap<u16, Tensor>,
    per_step: BTreeMap<u16, (u16, RandomDist, Vec<u32>)>,
    random: &'a dyn RandomSource,
}

impl InputProvider for StageInputs<'_> {
    fn input(&self, k: u16, pos: u32) -> Option<Tensor> {
        match self.per_step.get(&k) {
            Some((domain, dist, shape)) => self.random.random(*domain, *dist, pos, shape),
            None => self.constant.get(&k).cloned(),
        }
    }
}

/// **Run a pipeline**: every stage in order over its trip count, each input resolved from its
/// binding or drawn, and the output stage's result.
pub fn run_pipeline(
    p: &TirPipelineV1,
    programs: &[TirProgramV2],
    params: &dyn PipelineParams,
    random: &dyn RandomSource,
    job: &PipelineJob,
) -> TirResult<PipelineRun> {
    validate_pipeline(p, programs)?;
    let mut runs: Vec<StageRun> = Vec::with_capacity(p.stages.len());
    for st in &p.stages {
        let prog = &programs[st.program as usize];
        let tokens = match &st.tokens {
            Some(rule) => apply_rule(rule, job)?,
            None => Vec::new(),
        };
        let trip = match st.trip {
            TripRule::Fixed { n } => n,
            TripRule::JobSteps => job.steps,
            TripRule::TokenCount => tokens.len() as u32,
        };
        if trip == 0 || trip > st.max_trip {
            return err(TirErrorKind::Position, format!("stage {}: trip count {trip} outside [1, {}]", st.name, st.max_trip));
        }
        let mut inputs = StageInputs { constant: BTreeMap::new(), per_step: BTreeMap::new(), random };
        let mut bindings = st.bind.iter();
        for (k, d) in prog.inputs.iter().enumerate() {
            let k = k as u16;
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            let value = match d.source {
                InputSource::Random { domain, dist, per_step: true } => {
                    inputs.per_step.insert(k, (domain, dist, d.shape.clone()));
                    continue;
                }
                InputSource::Random { domain, dist, per_step: false } => random.random(domain, dist, 0, &d.shape),
                InputSource::External { .. } => {
                    match bindings.next().ok_or_else(|| TirError::new(TirErrorKind::NormalForm, "a binding per external input"))? {
                        Binding::JobScalar { index } => match job.scalars.get(*index as usize) {
                            Some(v) => Some(Tensor::scalar(d.dtype, *v as i128)?),
                            None => None,
                        },
                        Binding::JobTokens { rule } => {
                            let seq = apply_rule(rule, job)?;
                            Some(Tensor::new(DType::Idx, shape, seq.iter().map(|t| *t as i128).collect())?)
                        }
                        Binding::StageRows { stage, drop, pad_to } => {
                            let rows = runs[*stage as usize].rows();
                            let kept = rows.get(*drop as usize..).unwrap_or(&[]);
                            if kept.len() > *pad_to as usize {
                                return err(TirErrorKind::Operand, format!("{} rows exceed pad_to {pad_to}", kept.len()));
                            }
                            let per_row: usize = shape[1..].iter().product();
                            let mut data = Vec::with_capacity(*pad_to as usize * per_row);
                            for r in kept {
                                data.extend_from_slice(&r.data);
                            }
                            data.resize(*pad_to as usize * per_row, 0);
                            Some(Tensor::new(d.dtype, shape, data)?)
                        }
                        Binding::StageFinal { stage } => runs[*stage as usize].last().cloned(),
                        Binding::StageRowCount { stage, drop } => {
                            let n = runs[*stage as usize].trip.saturating_sub(*drop);
                            Some(Tensor::scalar(DType::Idx, n as i128)?)
                        }
                        Binding::JobTokenCount { rule } => {
                            // The length before padding; `apply_rule` refuses a template longer than its pad.
                            let n = apply_rule(rule, job)?.len() - pad_count(rule, job);
                            Some(Tensor::scalar(DType::Idx, n as i128)?)
                        }
                    }
                }
            };
            if let Some(v) = value {
                inputs.constant.insert(k, v);
            }
        }
        let interp = InterpreterV2::new(prog)?;
        let run_tokens = if tokens.is_empty() { vec![0; trip as usize] } else { tokens.clone() };
        let steps = interp.run(params.params(st.program), &inputs, &run_tokens)?;
        runs.push(StageRun { trip, tokens, steps });
    }
    let out_stage = &runs[p.output_stage as usize];
    let out_prog = &programs[p.stages[p.output_stage as usize].program as usize];
    // A stage runs at least one position (its trip count is checked above), so `last` exists.
    let last = out_stage.last().cloned().ok_or_else(|| TirError::new(TirErrorKind::Position, "the output stage ran no position"))?;
    let output = match out_prog.output {
        OutputDecl::Rows { .. } => {
            let mut shape = vec![out_stage.steps.len()];
            shape.extend_from_slice(&last.shape);
            Tensor::new(last.dtype, shape, out_stage.steps.iter().flat_map(|s| s.output.data.iter().copied()).collect())?
        }
        _ => last,
    };
    Ok(PipelineRun { stages: runs, output })
}
