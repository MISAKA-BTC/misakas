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
/// Job images a pipeline may bind (`Binding::JobImage { index }`, `index < 16`; RFC-0003 §II.4).
pub const MAX_JOB_IMAGES: usize = 16;
/// Finalized claims a pipeline may read (`TokenSource::FinalizedOutput { claim }`, `claim < 8`;
/// RFC-0004 §7.2 — a pairwise judge reads two).
pub const MAX_FINALIZED_CLAIMS: u8 = 8;

/// A stage's trip count `T`. Tags: `Fixed 0`, `JobSteps 1`, `TokenCount 2`, `TextStream 3`,
/// `Decode 4`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TripRule {
    /// Always `n` positions (a decoder: `n = 1`).
    Fixed { n: u32 },
    /// The job's step count (a denoiser).
    JobSteps,
    /// The length of the stage's token sequence (an encoder over the prompt).
    TokenCount,
    /// **The text stage** (RFC-0003 §II.2.1): the text job's stream — the prompt ids, then the
    /// generated ids — one position per id whose logits the decode consumed,
    /// `T = |prompt| + max(|generated|, 1) − 1`. Only a pipeline's output stage, a `Logits`
    /// program, has it (NF-P9′).
    TextStream,
    /// **A decode stage** (RFC-0004 §7.2, in RFC-0003's pipeline format): the text stage's stream
    /// and trip count, in a stage that is NOT the output — a `Logits` program whose run ends before
    /// the stages after it run. What it decoded (or, teacher-forced, was given) is the job's
    /// `generated` list, which a later stage reads through [`TokenSource::Generated`]; its rows, to a
    /// later `StageRows` / `StageRowCount`, are its consumed logits rows — position `|prompt| − 1`
    /// on, one per generated id, row `r` the logits `generated[r]` was selected from. At most one
    /// stream stage (this or `TextStream`) per pipeline.
    Decode,
}

/// Which of the job's token lists a template wraps. Tags: `Prompt 0`, `Negative 1`, `Source 2`,
/// `Generated 3`, `Key 4`, `FinalizedOutput 5`. Appended: a pipeline that names none of the later
/// ones encodes as before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TokenSource {
    Prompt,
    Negative,
    /// **The job's source ids** (RFC-0003 §II.2.2): the text an encoder–decoder's encoder reads, a
    /// second list beside the decoder's stream. Appended: every pipeline that names no source
    /// encodes as before.
    Source,
    /// **What the pipeline's decode stage produced** (RFC-0004 §7.2): the job's `generated` list — a
    /// claim's committed ids, or a teacher-forced job's given ones. Read only by a stage after the
    /// [`TripRule::Decode`] stage.
    Generated,
    /// **The item's key** (RFC-0004 §7.3): the ids an exact-match scoring stage compares an answer
    /// span with — the job's `key` list, a disclosed key's ids.
    Key,
    /// **A final claim's committed output** (RFC-0004 §7.2's `FinalizedOutput { claim, stage }`):
    /// the generated ids of the job's `claim`-th finalized claim, from that claim's stream stage
    /// `stage` — a pairwise judge reads the parent's and a candidate's generations without running
    /// them again. The job carries the ids; the chain holds their commitment and a court opens the
    /// ids against it.
    FinalizedOutput {
        claim: u8,
        stage: u8,
    },
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
/// `StageFinal 3`, `StageRowCount 4`, `JobTokenCount 5`, `JobImage 6`.
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
    /// The job's image `index` (RFC-0003 §II.4): `u8` HWC RGB at the input's size, into an `i16`
    /// input `[h, w, 3]` whose interval contains `[0, 255]` (NF-P10). The chain holds only the
    /// image's `input_root`; a court opens its tiles against it.
    JobImage { index: u8 },
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
    /// The job's images, pixels included — an executor's (and a reference run's) view. A court
    /// holds only each image's `input_root` and opens tiles against it (RFC-0003 §II.4).
    pub images: Vec<JobImageV1>,
    /// The text stage's generated ids, as committed (a replay's and a court's view; a run that
    /// generates them is [`run_text_pipeline`]). Empty for a pipeline with no text stage.
    pub generated: Vec<u32>,
    /// The job's source ids ([`TokenSource::Source`]): an encoder–decoder's source text. Empty for
    /// a pipeline that reads none.
    pub source: Vec<u32>,
    /// The item's key ids ([`TokenSource::Key`], RFC-0004 §7.3). Empty for a pipeline that reads none.
    pub key: Vec<u32>,
    /// The finalized claims' generated ids ([`TokenSource::FinalizedOutput`]), by `(claim, stage)`.
    pub finalized: BTreeMap<(u8, u8), Vec<u32>>,
}

/// **The text stage's token run**: the prompt ids, then every generated id but the last (never fed
/// back) — `T = |prompt| + max(|generated|, 1) − 1` ids.
pub fn text_stream_run(job: &PipelineJob) -> Vec<u32> {
    let fed = job.generated.len().saturating_sub(1);
    job.prompt.iter().chain(&job.generated[..fed]).copied().collect()
}

/// Is `trip` a stream stage's — the text stage's (`TextStream`) or a decode stage's (`Decode`)?
pub fn is_stream(trip: TripRule) -> bool {
    matches!(trip, TripRule::TextStream | TripRule::Decode)
}

/// **The pipeline's stream stage**, if it has one: its `TextStream` or `Decode` stage (at most one,
/// NF-P9′).
pub fn stream_stage(p: &TirPipelineV1) -> Option<usize> {
    p.stages.iter().position(|st| is_stream(st.trip))
}

/// **The position a stage's rows start at, as an edge reads them**: `|prompt| − 1` for a stream stage
/// (its consumed logits rows), 0 for every other stage (every position is a row).
pub fn stage_rows_from(st: &StageDecl, job: &PipelineJob) -> u32 {
    if is_stream(st.trip) { (job.prompt.len() as u32).saturating_sub(1) } else { 0 }
}

/// A selector's answer after a text-stage position whose logits the decode consumes (RFC-0001 §A,
/// applied outside the program).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSelectV1 {
    /// The next generated id, fed back at the next position.
    Next(u32),
    /// The final generated id (a stop matched, or the budget is spent): never fed back.
    Last(u32),
    /// Generation ends with no id at this position (an empty allowed set).
    End,
}

/// One job image: `u8` HWC RGB, `rgb.len() = h · w · 3`, row-major.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobImageV1 {
    pub h: u32,
    pub w: u32,
    pub rgb: Vec<u8>,
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

/// Every token rule a stage reads: its token run's, then its bindings' (in order).
fn stage_rules(st: &StageDecl) -> impl Iterator<Item = &TokenRule> {
    st.tokens.iter().chain(st.bind.iter().filter_map(|b| match b {
        Binding::JobTokens { rule } | Binding::JobTokenCount { rule } => Some(rule),
        _ => None,
    }))
}

/// Does an edge read rows of this upstream: a `Rows` stage's, or a decode stage's logits rows?
fn rows_of(us: &StageDecl, up: &TirProgramV2) -> bool {
    matches!(up.output, OutputDecl::Rows { .. })
        || (matches!(us.trip, TripRule::Decode) && matches!(up.output, OutputDecl::Logits { .. }))
}

/// What validation learned: each stage's external inputs, in order, and the job's image slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineInfo {
    pub externals: Vec<Vec<u16>>,
    /// Image `i`'s `[h, w]`, for `i` in `0..n`: the images every job of the pipeline carries (NF-P10).
    pub images: Vec<[u32; 2]>,
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
    let mut images: BTreeMap<u8, [u32; 2]> = BTreeMap::new();
    // RFC-0004 §7.2: the decode stage, whose `generated` ids a later stage may read.
    let decode = p.stages.iter().position(|st| matches!(st.trip, TripRule::Decode));
    for (s, st) in p.stages.iter().enumerate() {
        let what = format!("stage {s} ({})", st.name);
        // The token sources a stage's rules read: `Generated` only after the decode stage (the ids
        // exist once it has run), a finalized claim below the cap.
        for rule in stage_rules(st) {
            match rule.source {
                TokenSource::Generated => match decode {
                    Some(d) if d < s => consumed[d] = true,
                    _ => return nf(format!("{what}: only a stage after the pipeline's Decode stage reads its generated ids")),
                },
                TokenSource::FinalizedOutput { claim, .. } if claim >= MAX_FINALIZED_CLAIMS => {
                    return nf(format!("{what}: a finalized claim is below {MAX_FINALIZED_CLAIMS}"));
                }
                _ => {}
            }
        }
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
            (TripRule::TextStream | TripRule::Decode, None, true) => {}
            _ => {
                return nf(format!(
                    "{what}: Fixed (with max_trip = n) and JobSteps stages read no token; a TokenCount stage reads the token and has a \
                     token rule; a TextStream or Decode stage reads the token and has none"
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
                    if !rows_of(us, up) {
                        return nf(format!("{edge}: StageRows reads a Rows stage or a Decode stage's logits rows"));
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
                    if !rows_of(us, up) {
                        return nf(format!("{edge}: StageRowCount counts a Rows stage or a Decode stage's logits rows"));
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
                // NF-P10: a job image is `i16 [h, w, 3]` over `[0, 255]`, one size per image.
                Binding::JobImage { index } => {
                    if *index as usize >= MAX_JOB_IMAGES {
                        return nf(format!("{edge}: a job image index is below {MAX_JOB_IMAGES}"));
                    }
                    if d.dtype != DType::I16 || d.shape.len() != 3 || d.shape[2] != 3 {
                        return nf(format!("{edge}: a job image is an i16 [h, w, 3] input"));
                    }
                    if lo > 0 || hi < 255 {
                        return nf(format!("{edge}: a pixel's [0, 255] exceeds [{lo}, {hi}]"));
                    }
                    let size = [d.shape[0], d.shape[1]];
                    if let Some(prev) = images.insert(*index, size)
                        && prev != size
                    {
                        return nf(format!("{edge}: job image {index} is bound at {prev:?} and at {size:?}"));
                    }
                }
            }
        }
        externals.push(ext);
    }
    // NF-P9 (with NF-P9′): the output stage ends in a tensor or is the text stage — a Logits program
    // over the text stream — and no other stage is either; a Logits program elsewhere is the decode
    // stage (RFC-0004 §7.2), which is never the output; at most one stream stage; no other stage is
    // dead.
    for (s, st) in p.stages.iter().enumerate() {
        let logits = matches!(programs[st.program as usize].output, OutputDecl::Logits { .. });
        let output = s == p.output_stage as usize;
        let ok = match st.trip {
            TripRule::TextStream => logits && output,
            TripRule::Decode => logits && !output,
            _ => !logits,
        };
        if !ok {
            return nf(format!(
                "stage {s} ({}): a Logits program is the text stage — the output stage, over TextStream — or the Decode stage, \
                 which is not the output, and only these are",
                st.name
            ));
        }
    }
    if p.stages.iter().filter(|st| is_stream(st.trip)).count() > 1 {
        return nf("a pipeline has at most one stream stage (TextStream or Decode): one generated list");
    }
    if let Some(dead) = (0..p.stages.len()).find(|s| *s != p.output_stage as usize && !consumed[*s]) {
        return nf(format!("stage {dead} feeds no later stage and is not the output"));
    }
    // NF-P10: the bound images are `0..n` — a job carries exactly these, and no index goes unread.
    if let Some((i, _)) = images.iter().enumerate().find(|(i, (index, _))| *i != **index as usize) {
        return nf(format!("job image {i} is bound by no stage, but a later one is"));
    }
    Ok(PipelineInfo { externals, images: images.into_values().collect() })
}

/// One stage of a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageRun {
    pub trip: u32,
    /// `Input(0)` at each position (empty for a stage that reads no token).
    pub tokens: Vec<u32>,
    pub steps: Vec<StepOutputV2>,
    /// The `Fixed` states after each position — what a step tree checkpoints (RFC-0003 §I.2.3's one
    /// tree): an absent instance is all zeros.
    pub fixed_after: Vec<BTreeMap<crate::interp::StateKey, Tensor>>,
}

/// Run one stage position by position, keeping each position's output and the `Fixed` states after it.
fn run_stage(
    interp: &InterpreterV2<'_>,
    params: &dyn ParamSource,
    inputs: &dyn InputProvider,
    tokens: &[u32],
) -> TirResult<(Vec<StepOutputV2>, Vec<BTreeMap<crate::interp::StateKey, Tensor>>)> {
    let mut state = crate::interp::RunState::default();
    let (mut steps, mut fixed_after) = (Vec::with_capacity(tokens.len()), Vec::with_capacity(tokens.len()));
    for t in tokens {
        steps.push(interp.step(params, inputs, &mut state, *t)?);
        fixed_after.push(state.fixed.clone());
    }
    Ok((steps, fixed_after))
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

/// The job's list a token source names; a finalized claim the job does not carry is `Missing`.
fn source_ids(source: TokenSource, job: &PipelineJob) -> TirResult<&[u32]> {
    Ok(match source {
        TokenSource::Prompt => &job.prompt,
        TokenSource::Negative => &job.negative,
        TokenSource::Source => &job.source,
        TokenSource::Generated => &job.generated,
        TokenSource::Key => &job.key,
        TokenSource::FinalizedOutput { claim, stage } => job.finalized.get(&(claim, stage)).ok_or_else(|| {
            TirError::new(TirErrorKind::Missing, format!("the job carries no finalized output of claim {claim}, stage {stage}"))
        })?,
    })
}

/// How many pad ids `apply_rule` appended.
fn pad_count(rule: &TokenRule, job: &PipelineJob) -> TirResult<usize> {
    let src = source_ids(rule.source, job)?.len();
    let unpadded = rule.prefix.len() + src + rule.suffix.len();
    Ok(rule.pad.map_or(0, |p| (p.to_len as usize).saturating_sub(unpadded)))
}

fn apply_rule(rule: &TokenRule, job: &PipelineJob) -> TirResult<Vec<u32>> {
    let src = source_ids(rule.source, job)?;
    let mut seq: Vec<u32> = rule.prefix.iter().chain(src.iter()).chain(rule.suffix.iter()).copied().collect();
    if let Some(pad) = rule.pad {
        if seq.len() > pad.to_len as usize {
            return err(TirErrorKind::Operand, format!("{} tokens exceed the template's {}", seq.len(), pad.to_len));
        }
        seq.resize(pad.to_len as usize, pad.id);
    }
    Ok(seq)
}

/// **What a job fixes of a stage before anything runs** — its trip count, its token run, and every
/// input the court derives from the job rather than opening (spec 04b §15.4): job scalars, token
/// tensors, token counts and row counts (an earlier stage's row count is its trip count, a job fact).
/// An input bound to an earlier stage's committed output, and a random input, are not here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageJobFacts {
    pub trip: u32,
    /// `Input(0)` at each position (empty for a stage that reads no token).
    pub tokens: Vec<u32>,
    /// Input index → its value.
    pub inputs: BTreeMap<u16, Tensor>,
    /// The position its rows start at as an edge reads them ([`stage_rows_from`]): `|prompt| − 1`
    /// for a stream stage, 0 otherwise.
    pub rows_from: u32,
}

/// **The job facts of every stage**, in order — what [`run_pipeline`] computes before it runs a
/// stage, without running anything. Refused as the run refuses: a template longer than its pad, a
/// trip count outside `[1, max_trip]`, a job scalar the job does not carry.
pub fn stage_job_facts(p: &TirPipelineV1, programs: &[TirProgramV2], job: &PipelineJob) -> TirResult<Vec<StageJobFacts>> {
    validate_pipeline(p, programs)?;
    let mut facts: Vec<StageJobFacts> = Vec::with_capacity(p.stages.len());
    for st in &p.stages {
        let prog = &programs[st.program as usize];
        let (tokens, trip) = stage_tokens(st, job)?;
        if trip == 0 || trip > st.max_trip {
            return err(TirErrorKind::Position, format!("stage {}: trip count {trip} outside [1, {}]", st.name, st.max_trip));
        }
        let mut inputs = BTreeMap::new();
        let externals = prog.inputs.iter().enumerate().filter(|(_, d)| d.is_external());
        for (b, (k, d)) in st.bind.iter().zip(externals) {
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            let value = match b {
                Binding::JobScalar { index } => {
                    let v = job.scalars.get(*index as usize).ok_or_else(|| {
                        TirError::new(TirErrorKind::Missing, format!("stage {}: the job carries no scalar {index}", st.name))
                    })?;
                    Tensor::scalar(d.dtype, *v as i128)?
                }
                Binding::JobTokens { rule } => {
                    let seq = apply_rule(rule, job)?;
                    Tensor::new(DType::Idx, shape, seq.iter().map(|t| *t as i128).collect())?
                }
                Binding::JobTokenCount { rule } => {
                    let n = apply_rule(rule, job)?.len() - pad_count(rule, job)?;
                    Tensor::scalar(DType::Idx, n as i128)?
                }
                Binding::StageRowCount { stage, drop } => {
                    let up = &facts[*stage as usize];
                    Tensor::scalar(DType::Idx, up.trip.saturating_sub(up.rows_from).saturating_sub(*drop) as i128)?
                }
                // An edge's values are an earlier stage's commitments, and an image's are opened
                // against its `input_root`: neither is a fact the job fixes for the court.
                Binding::StageRows { .. } | Binding::StageFinal { .. } | Binding::JobImage { .. } => continue,
            };
            inputs.insert(k as u16, value);
        }
        facts.push(StageJobFacts { trip, tokens, inputs, rows_from: stage_rows_from(st, job) });
    }
    Ok(facts)
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
        let (tokens, trip) = stage_tokens(st, job)?;
        let inputs = stage_inputs(p, st, prog, &runs, job, random)?;
        let interp = InterpreterV2::new(prog)?;
        let run_tokens = if tokens.is_empty() { vec![0; trip as usize] } else { tokens.clone() };
        let (steps, fixed_after) = run_stage(&interp, params.params(st.program), &inputs, &run_tokens)?;
        runs.push(StageRun { trip, tokens, steps, fixed_after });
    }
    let output = pipeline_output(p, programs, &runs)?;
    Ok(PipelineRun { stages: runs, output })
}

/// A stage's token run and trip count, from the job: refused when the trip count leaves
/// `[1, max_trip]` (`Position`) or a template overflows its pad (`Operand`).
fn stage_tokens(st: &StageDecl, job: &PipelineJob) -> TirResult<(Vec<u32>, u32)> {
    let tokens = match (&st.tokens, st.trip) {
        (Some(rule), _) => apply_rule(rule, job)?,
        (None, TripRule::TextStream | TripRule::Decode) => text_stream_run(job),
        (None, _) => Vec::new(),
    };
    let trip = match st.trip {
        TripRule::Fixed { n } => n,
        TripRule::JobSteps => job.steps,
        TripRule::TokenCount | TripRule::TextStream | TripRule::Decode => tokens.len() as u32,
    };
    if trip == 0 || trip > st.max_trip {
        return err(TirErrorKind::Position, format!("stage {}: trip count {trip} outside [1, {}]", st.name, st.max_trip));
    }
    Ok((tokens, trip))
}

/// The pipeline's output tensor: a `Final` stage's last value, or a `Rows` (or text) stage's rows
/// stacked `[T] ++ row shape`.
fn pipeline_output(p: &TirPipelineV1, programs: &[TirProgramV2], runs: &[StageRun]) -> TirResult<Tensor> {
    let out_stage = &runs[p.output_stage as usize];
    let out_prog = &programs[p.stages[p.output_stage as usize].program as usize];
    // A stage runs at least one position (its trip count is checked), so `last` exists.
    let last = out_stage.last().cloned().ok_or_else(|| TirError::new(TirErrorKind::Position, "the output stage ran no position"))?;
    match out_prog.output {
        OutputDecl::Rows { .. } | OutputDecl::Logits { .. } => {
            let mut shape = vec![out_stage.steps.len()];
            shape.extend_from_slice(&last.shape);
            Tensor::new(last.dtype, shape, out_stage.steps.iter().flat_map(|s| s.output.data.iter().copied()).collect())
        }
        OutputDecl::Final { .. } => Ok(last),
    }
}

/// **Run a text pipeline, generating** (RFC-0003 §II.2.1; RFC-0004 §7.2): every stage before the
/// stream stage (its `TextStream` output stage, or its `Decode` stage) as [`run_pipeline`] runs it,
/// then the stream stage position by position over the prompt, asking `select` after each position
/// whose logits the decode consumes (from `|prompt| − 1` on) for the next id — the RFC-0001 §A
/// decoder's answer, which the IR never computes itself — and then, for a `Decode` stage, every stage
/// after it with the generated ids as the job's `generated`. Returns the run and the generated ids;
/// [`run_pipeline`] over the same job with `generated` set replays it exactly.
pub fn run_text_pipeline(
    p: &TirPipelineV1,
    programs: &[TirProgramV2],
    params: &dyn PipelineParams,
    random: &dyn RandomSource,
    job: &PipelineJob,
    select: &mut dyn FnMut(u32, &Tensor) -> TextSelectV1,
) -> TirResult<(PipelineRun, Vec<u32>)> {
    validate_pipeline(p, programs)?;
    let Some(text) = stream_stage(p) else {
        return nf("run_text_pipeline runs a pipeline with a stream stage (TextStream or Decode)");
    };
    if job.prompt.is_empty() {
        return err(TirErrorKind::Position, "a text stream starts with at least one prompt id");
    }
    let mut runs: Vec<StageRun> = Vec::with_capacity(p.stages.len());
    for st in &p.stages[..text] {
        let prog = &programs[st.program as usize];
        let (tokens, trip) = stage_tokens(st, job)?;
        let inputs = stage_inputs(p, st, prog, &runs, job, random)?;
        let run_tokens = if tokens.is_empty() { vec![0; trip as usize] } else { tokens.clone() };
        let (steps, fixed_after) = run_stage(&InterpreterV2::new(prog)?, params.params(st.program), &inputs, &run_tokens)?;
        runs.push(StageRun { trip, tokens, steps, fixed_after });
    }
    let st = &p.stages[text];
    let prog = &programs[st.program as usize];
    let inputs = stage_inputs(p, st, prog, &runs, job, random)?;
    let interp = InterpreterV2::new(prog)?;
    let mut state = crate::interp::RunState::default();
    let (mut stream, mut generated, mut steps) = (job.prompt.clone(), Vec::new(), Vec::new());
    let mut fixed_after = Vec::new();
    let mut pos = 0usize;
    loop {
        if pos as u32 >= st.max_trip {
            return err(TirErrorKind::Position, format!("stage {}: the stream passes max_trip {}", st.name, st.max_trip));
        }
        let out = interp.step(params.params(st.program), &inputs, &mut state, stream[pos])?;
        fixed_after.push(state.fixed.clone());
        let consumed = pos + 1 >= job.prompt.len();
        let answer = if consumed { Some(select(pos as u32, &out.output)) } else { None };
        steps.push(out);
        match answer {
            None => {}
            Some(TextSelectV1::Next(id)) => {
                generated.push(id);
                stream.push(id);
            }
            Some(TextSelectV1::Last(id)) => {
                generated.push(id);
                break;
            }
            Some(TextSelectV1::End) => break,
        }
        pos += 1;
    }
    let tokens = stream[..steps.len()].to_vec();
    runs.push(StageRun { trip: steps.len() as u32, tokens, steps, fixed_after });
    // A decode stage's later stages read what it decoded (`TokenSource::Generated`).
    let after = PipelineJob { generated: generated.clone(), ..job.clone() };
    for st in &p.stages[text + 1..] {
        let prog = &programs[st.program as usize];
        let (tokens, trip) = stage_tokens(st, &after)?;
        let inputs = stage_inputs(p, st, prog, &runs, &after, random)?;
        let run_tokens = if tokens.is_empty() { vec![0; trip as usize] } else { tokens.clone() };
        let (steps, fixed_after) = run_stage(&InterpreterV2::new(prog)?, params.params(st.program), &inputs, &run_tokens)?;
        runs.push(StageRun { trip, tokens, steps, fixed_after });
    }
    let output = pipeline_output(p, programs, &runs)?;
    Ok((PipelineRun { stages: runs, output }, generated))
}

/// A stage's inputs from its bindings, the earlier stages' runs, the job and `R`.
fn stage_inputs<'a>(
    p: &TirPipelineV1,
    st: &StageDecl,
    prog: &TirProgramV2,
    runs: &[StageRun],
    job: &PipelineJob,
    random: &'a dyn RandomSource,
) -> TirResult<StageInputs<'a>> {
    {
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
                            let from = stage_rows_from(&p.stages[*stage as usize], job) as usize;
                            let kept = rows.get(from.saturating_add(*drop as usize)..).unwrap_or(&[]);
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
                            let from = stage_rows_from(&p.stages[*stage as usize], job);
                            let n = runs[*stage as usize].trip.saturating_sub(from).saturating_sub(*drop);
                            Some(Tensor::scalar(DType::Idx, n as i128)?)
                        }
                        Binding::JobTokenCount { rule } => {
                            // The length before padding; `apply_rule` refuses a template longer than its pad.
                            let n = apply_rule(rule, job)?.len() - pad_count(rule, job)?;
                            Some(Tensor::scalar(DType::Idx, n as i128)?)
                        }
                        Binding::JobImage { index } => match job.images.get(*index as usize) {
                            Some(img) => {
                                let bytes = img.h as u64 * img.w as u64 * 3;
                                if [img.h, img.w, 3] != d.shape[..] || img.rgb.len() as u64 != bytes {
                                    return err(
                                        TirErrorKind::Operand,
                                        format!(
                                            "stage {}: job image {index} is {}×{} in {} bytes; the input is {:?}",
                                            st.name,
                                            img.h,
                                            img.w,
                                            img.rgb.len(),
                                            d.shape
                                        ),
                                    );
                                }
                                Some(Tensor::new(DType::I16, shape, img.rgb.iter().map(|b| *b as i128).collect())?)
                            }
                            None => None,
                        },
                    }
                }
            };
            if let Some(v) = value {
                inputs.constant.insert(k, v);
            }
        }
        Ok(inputs)
    }
}
