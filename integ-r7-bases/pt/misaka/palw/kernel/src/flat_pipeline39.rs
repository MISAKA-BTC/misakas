//! Owning pipeline39 over actual Program39 components. Frozen binding/trip
//! variants are structural recipes; no V1/V2 program proxy is validated here.
use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{
    DType, Prim, TirError, TirErrorKind, TirResult,
    pipeline::{
        Binding, MAX_FINALIZED_CLAIMS, MAX_JOB_IMAGES, MAX_JOB_SCALARS, MAX_TEMPLATE_TOKENS, PipelineInfo, StageDecl, TokenPad,
        TokenRule, TokenSource, TripRule, is_stream,
    },
    program::{INPUT_TOKEN, MAX_NAME_BYTES, Ref},
    program_v2::{InputSource, OutputDecl},
    program_v39::{DimV39, NodePrimV39, TirProgramV39},
};
use misaka_palw_tir::{
    Tensor,
    error::err,
    pipeline::{PipelineJob, StageJobFacts, stage_rows_from, text_stream_run},
};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_STAGES_V39: usize = 64;
pub const MAX_PIPELINE_BYTES_V39: usize = 1 << 20;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PipelineV39 {
    pub version: u16,
    pub stages: Vec<StageDecl>,
    pub output_stage: u8,
}
pub struct CheckedPipelineV39 {
    wire: PipelineV39,
    info: PipelineInfo,
    root: Digest,
}
impl CheckedPipelineV39 {
    pub fn root(&self) -> Digest {
        self.root
    }
    pub fn wire(&self) -> &PipelineV39 {
        &self.wire
    }
    pub fn info(&self) -> &PipelineInfo {
        &self.info
    }
}
fn nf<T>(s: impl Into<String>) -> TirResult<T> {
    Err(TirError::new(TirErrorKind::NormalForm, s))
}
fn reads_token(p: &TirProgramV39) -> bool {
    p.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.inputs.contains(&Ref::Input(INPUT_TOKEN)))
}
fn output_shape(p: &TirProgramV39) -> Vec<u32> {
    p.blocks[p.schedule.post as usize].nodes[p.output.node() as usize]
        .out
        .shape
        .iter()
        .map(|d| match d {
            DimV39::Fixed(n) => *n,
            _ => 0,
        })
        .collect()
}
fn rows_of(st: &StageDecl, p: &TirProgramV39) -> bool {
    matches!(p.output, OutputDecl::Rows { .. }) || matches!(st.trip, TripRule::Decode) && matches!(p.output, OutputDecl::Logits { .. })
}
fn stage_rules(st: &StageDecl) -> impl Iterator<Item = &TokenRule> {
    st.tokens.iter().chain(st.bind.iter().filter_map(|b| match b {
        Binding::JobTokens { rule } | Binding::JobTokenCount { rule } => Some(rule),
        _ => None,
    }))
}
fn check_rule(what: &str, r: &TokenRule, bound: u32) -> TirResult<()> {
    if r.prefix.len() > MAX_TEMPLATE_TOKENS
        || r.suffix.len() > MAX_TEMPLATE_TOKENS
        || r.prefix.iter().chain(&r.suffix).any(|v| *v >= bound)
        || r.pad.is_some_and(|p| p.to_len == 0 || p.id >= bound)
    {
        return nf(format!("{what}: token template bound"));
    }
    Ok(())
}
struct EdgeInterval39 {
    lo: i128,
    hi: i128,
}
fn output_interval_v39(p: &TirProgramV39) -> TirResult<EdgeInterval39> {
    let node = &p.blocks[p.schedule.post as usize].nodes[p.output.node() as usize];
    if node.out.shape.iter().any(|d| !matches!(d, DimV39::Fixed(_))) {
        return nf("pipeline39 dynamic output dimensions require an explicit owning binding");
    }
    let (mut lo, mut hi) = (node.out.dtype.min_value(), node.out.dtype.max_value());
    if let NodePrimV39::Frozen(Prim::Clamp { lo: a, hi: b }) = &node.prim {
        lo = lo.max(*a as i128);
        hi = hi.min(*b as i128);
    }
    Ok(EdgeInterval39 { lo, hi })
}
pub fn validate_pipeline_v39(p: &PipelineV39, programs: &[TirProgramV39]) -> TirResult<PipelineInfo> {
    if p.version != 39 {
        return nf(format!("pipeline version {} is not 39", p.version));
    }
    if p.stages.is_empty() || p.stages.len() > MAX_STAGES_V39 {
        return nf(format!("1..={MAX_STAGES_V39} stages"));
    }
    if p.output_stage as usize >= p.stages.len() {
        return nf("the output stage does not exist");
    }
    for prog in programs {
        misaka_palw_tir::validate_v39::validate_v39(prog).map_err(|e| TirError::new(TirErrorKind::NormalForm, e))?;
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
            let upstream = |u: u8| -> TirResult<(&StageDecl, &TirProgramV39)> {
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
                    let iv = output_interval_v39(up)?;
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
                    let iv = output_interval_v39(up)?;
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

impl PipelineV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/complete-pipeline/v39", self)
    }
    pub fn check(&self, programs: &[TirProgramV39]) -> TirResult<CheckedPipelineV39> {
        if programs.len() != self.stages.len() || self.stages.iter().enumerate().any(|(i, s)| s.program as usize != i) {
            return nf("pipeline39 exact ordered stage/program coverage");
        }
        let info = validate_pipeline_v39(self, programs)?;
        self.to_bytes().map_err(|e| TirError::new(TirErrorKind::NormalForm, e.to_string()))?;
        Ok(CheckedPipelineV39 { wire: self.clone(), info, root: self.root() })
    }
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        crate::flat_court4::size_with_limit(self, MAX_PIPELINE_BYTES_V39 as u64)?;
        borsh::to_vec(self)
    }
    /// Every allocation is bounded by the owning39 frame before Borsh parses it.
    pub fn from_bytes(raw: &[u8]) -> std::io::Result<Self> {
        use std::io::{Cursor, Error, ErrorKind, Read};
        fn bad() -> Error {
            Error::new(ErrorKind::InvalidData, "pipeline39 bounded canonical wire")
        }
        fn string(r: &mut Cursor<&[u8]>) -> std::io::Result<String> {
            let n = u32::deserialize_reader(r)? as usize;
            if n > MAX_NAME_BYTES || n > r.get_ref().len().saturating_sub(r.position() as usize) {
                return Err(bad());
            }
            let mut b = vec![0; n];
            r.read_exact(&mut b)?;
            String::from_utf8(b).map_err(|_| bad())
        }
        fn rule(r: &mut Cursor<&[u8]>) -> std::io::Result<TokenRule> {
            Ok(TokenRule {
                prefix: crate::flat_court4::vector(r, MAX_TEMPLATE_TOKENS, u32::deserialize_reader)?,
                source: TokenSource::deserialize_reader(r)?,
                suffix: crate::flat_court4::vector(r, MAX_TEMPLATE_TOKENS, u32::deserialize_reader)?,
                pad: Option::<TokenPad>::deserialize_reader(r)?,
            })
        }
        fn binding(r: &mut Cursor<&[u8]>) -> std::io::Result<Binding> {
            Ok(match u8::deserialize_reader(r)? {
                0 => Binding::JobScalar { index: u8::deserialize_reader(r)? },
                1 => Binding::JobTokens { rule: rule(r)? },
                2 => Binding::StageRows {
                    stage: u8::deserialize_reader(r)?,
                    drop: u32::deserialize_reader(r)?,
                    pad_to: u32::deserialize_reader(r)?,
                },
                3 => Binding::StageFinal { stage: u8::deserialize_reader(r)? },
                4 => Binding::StageRowCount { stage: u8::deserialize_reader(r)?, drop: u32::deserialize_reader(r)? },
                5 => Binding::JobTokenCount { rule: rule(r)? },
                6 => Binding::JobImage { index: u8::deserialize_reader(r)? },
                _ => return Err(bad()),
            })
        }
        if raw.len() > MAX_PIPELINE_BYTES_V39 {
            return Err(bad());
        }
        let mut r = Cursor::new(raw);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 39 {
            return Err(bad());
        }
        let stages = crate::flat_court4::vector(&mut r, MAX_STAGES_V39, |r| {
            let name = string(r)?;
            let program = u16::deserialize_reader(r)?;
            let trip = TripRule::deserialize_reader(r)?;
            let max_trip = u32::deserialize_reader(r)?;
            let tokens = match u8::deserialize_reader(r)? {
                0 => None,
                1 => Some(rule(r)?),
                _ => return Err(bad()),
            };
            let bind = crate::flat_court4::vector(r, 512, binding)?;
            Ok(StageDecl { name, program, trip, max_trip, tokens, bind })
        })?;
        let output_stage = u8::deserialize_reader(&mut r)?;
        let p = Self { version, stages, output_stage };
        if r.position() != raw.len() as u64 || p.to_bytes()? != raw {
            return Err(bad());
        }
        Ok(p)
    }
}
pub const PIPELINE39_MAX_JOB_TOKENS: usize = 1 << 20;
fn program_token_cap_v39() -> usize {
    PIPELINE39_MAX_JOB_TOKENS
}
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
    let n = rule
        .prefix
        .len()
        .checked_add(src.len())
        .and_then(|n| n.checked_add(rule.suffix.len()))
        .ok_or_else(|| TirError::new(TirErrorKind::Operand, "pipeline39 token extent overflow"))?;
    let max = program_token_cap_v39();
    if n > max || rule.pad.is_some_and(|p| p.to_len as usize > max) {
        return nf("pipeline39 token materialisation exceeds owning position cap");
    }
    let mut seq: Vec<u32> = rule.prefix.iter().chain(src.iter()).chain(rule.suffix.iter()).copied().collect();
    if let Some(pad) = rule.pad {
        if seq.len() > pad.to_len as usize {
            return err(TirErrorKind::Operand, format!("{} tokens exceed the template's {}", seq.len(), pad.to_len));
        }
        seq.resize(pad.to_len as usize, pad.id);
    }
    Ok(seq)
}

pub fn stage_job_facts_v39(p: &PipelineV39, programs: &[TirProgramV39], job: &PipelineJob) -> TirResult<Vec<StageJobFacts>> {
    let checked = p.check(programs)?;
    if job.scalars.len() > MAX_JOB_SCALARS
        || job.images.len() != checked.info().images.len()
        || job.finalized.len() > MAX_FINALIZED_CLAIMS as usize
    {
        return nf("pipeline39 original job fact count");
    }
    let token_cap = programs.iter().map(|p| p.history_bound as usize).max().unwrap_or(0);
    if [&job.prompt, &job.negative, &job.source, &job.generated, &job.key].iter().any(|ids| ids.len() > token_cap)
        || job.finalized.iter().any(|((c, _), ids)| *c >= MAX_FINALIZED_CLAIMS || ids.len() > token_cap)
    {
        return nf("pipeline39 token source allocation/count bound");
    }
    for (image, size) in job.images.iter().zip(&checked.info().images) {
        let n = (image.h as u64).checked_mul(image.w as u64).and_then(|n| n.checked_mul(3));
        if [image.h, image.w] != *size || n != Some(image.rgb.len() as u64) {
            return nf("pipeline39 original image extent differs");
        }
    }
    let mut facts: Vec<StageJobFacts> = Vec::with_capacity(p.stages.len());
    for st in &p.stages {
        let prog = &programs[st.program as usize];
        let (tokens, trip) = stage_tokens(st, job)?;
        if tokens.iter().any(|t| *t >= prog.token_bound) {
            return nf("pipeline39 original token bound exceeded");
        }
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
            let (lo, hi) = d.interval();
            if value.data.iter().any(|v| *v < lo || *v > hi) {
                return nf("pipeline39 job-fixed value exceeds original input interval");
            }
            inputs.insert(k as u16, value);
        }
        facts.push(StageJobFacts { trip, tokens, inputs, rows_from: stage_rows_from(st, job) });
    }
    Ok(facts)
}

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

use misaka_palw_tir::pipeline::RandomSource;
fn binding_of<'a>(st: &'a StageDecl, p: &TirProgramV39, k: usize) -> Option<&'a Binding> {
    let slot = p.inputs.iter().take(k + 1).filter(|d| d.is_external()).count().checked_sub(1)?;
    p.inputs.get(k)?.is_external().then(|| st.bind.get(slot)).flatten()
}
fn stage_input_v39(
    p: &PipelineV39,
    programs: &[TirProgramV39],
    facts: &[StageJobFacts],
    job: &PipelineJob,
    random: &dyn RandomSource,
    si: usize,
    k: u16,
    position: u32,
    upstream: &dyn Fn(u8) -> Option<Vec<Tensor>>,
) -> Result<Tensor, String> {
    let st = p.stages.get(si).ok_or("pipeline39 stage absent")?;
    let prog = programs.get(st.program as usize).ok_or("pipeline39 program absent")?;
    let d = prog.inputs.get(k as usize).ok_or("no such input")?;
    let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
    let value = match d.source {
        InputSource::Random { domain, dist, per_step } => {
            random.random(domain, dist, if per_step { position } else { 0 }, &d.shape).ok_or("R refused the domain")?
        }
        InputSource::External { .. } => match binding_of(st, prog, k as usize).ok_or("an external input with no binding")? {
            Binding::JobScalar { .. } | Binding::JobTokens { .. } | Binding::JobTokenCount { .. } | Binding::StageRowCount { .. } => {
                facts
                    .get(si)
                    .ok_or("pipeline39 stage facts absent")?
                    .inputs
                    .get(&k)
                    .cloned()
                    .ok_or("the job does not fix this input")?
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

/// Invocation32 borrows authenticated upstream output tensors. Only the
/// actually selected rows/value are copied into the declared input. A large
/// unused upstream prefix is never cloned once for each execution position.
/// This keeps the original all-position input/copy reservation meaningful;
/// the frozen v1 helper retains its original API and semantics.
#[allow(clippy::too_many_arguments)]
pub fn stage_input_borrowed_v39<'a>(
    p: &PipelineV39,
    programs: &[TirProgramV39],
    facts: &[StageJobFacts],
    job: &PipelineJob,
    random: &dyn RandomSource,
    si: usize,
    k: u16,
    position: u32,
    upstream: &dyn Fn(u8) -> Option<&'a [Tensor]>,
) -> Result<Tensor, String> {
    let st = p.stages.get(si).ok_or("pipeline39 input stage absent")?;
    let prog = programs.get(st.program as usize).ok_or("pipeline39 input program absent")?;
    let d = prog.inputs.get(k as usize).ok_or("pipeline39 input absent")?;
    let len = d.shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("pipeline39 input extent overflow")?;
    if len.checked_mul(16).is_none_or(|n| n > prog.profile.max_input_expanded_bytes)
        || len.checked_mul(d.dtype.width() as u64).is_none_or(|n| n > prog.profile.max_input_raw_bytes)
    {
        return Err("pipeline39 original paid input capacity".into());
    }
    if position >= facts.get(si).ok_or("pipeline39 actual stage facts absent")?.trip {
        return Err("pipeline39 position exceeds original actual stage".into());
    }
    let value = match (&d.source, binding_of(st, prog, k as usize)) {
        (InputSource::External { .. }, Some(Binding::StageRows { stage, drop, pad_to })) => {
            let rows = upstream(*stage).ok_or("pipeline39 upstream rows absent")?;
            let original = facts.get(*stage as usize).ok_or("pipeline39 upstream facts absent")?;
            if rows.len() != original.trip as usize {
                return Err("pipeline39 upstream certificate omits original output rows".into());
            }
            let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
            let kept = rows.get((original.rows_from as usize).saturating_add(*drop as usize)..).unwrap_or(&[]);
            if shape.is_empty() || shape[0] != *pad_to as usize || kept.len() > *pad_to as usize {
                return Err("pipeline39 rows exceed original input padding".into());
            }
            let width = shape[1..].iter().try_fold(1usize, |n, d| n.checked_mul(*d).ok_or("pipeline39 row extent overflow"))?;
            let len = width.checked_mul(*pad_to as usize).ok_or("pipeline39 input extent overflow")?;
            let mut data = Vec::with_capacity(len);
            for row in kept {
                if row.dtype != d.dtype || row.shape != shape[1..] || row.data.len() != width {
                    return Err("pipeline39 upstream row type differs".into());
                }
                data.extend_from_slice(&row.data);
            }
            data.resize(len, 0);
            Tensor::new(d.dtype, shape, data).map_err(|e| e.to_string())?
        }
        (InputSource::External { .. }, Some(Binding::StageFinal { stage })) => {
            let values = upstream(*stage).ok_or("pipeline39 upstream Final absent")?;
            if values.len() != 1 {
                return Err("pipeline39 upstream Final certificate count differs".into());
            }
            values[0].clone()
        }
        _ => return stage_input_v39(p, programs, facts, job, random, si, k, position, &|_| None),
    };
    let (lo, hi) = d.interval();
    if value.dtype != d.dtype
        || value.shape != d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>()
        || value.data.iter().any(|v| *v < lo || *v > hi)
    {
        return Err("pipeline39 bound upstream value violates original input declaration".into());
    }
    Ok(value)
}
