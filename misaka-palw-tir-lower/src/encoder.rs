//! **Encoders as version-2 programs** (RFC-0003 Part II.3, the Embedding profile; spec 04b §15).
//!
//! The lowering writes an encoder's blocks exactly as a decoder's, into a version-1 program whose
//! `logits` node is the embedding row (`post`: final norm, optional projection, optional L2
//! normalisation, then the class's fixed point). This module turns that program into a version-2
//! program with the matching output kind, and states the one-stage pipeline a class declares for
//! it.
//!
//! * **A causal encoder** (CLIP's text tower, a decoder-only embedder) is a scan over
//!   `prefix ‖ prompt ‖ suffix` (`TokenCount`). Last-token pooling is its `Final` output. Its
//!   per-token rows are its `Rows` output, which a diffusion class's denoiser binds.

use crate::error::{LowerError, Result};
use crate::lower::Lowered;
use misaka_palw_tir as tir;
use tir::pipeline::{StageDecl, TirPipelineV1, TokenPad, TokenRule, TokenSource, TripRule};
use tir::program_v2::{OutputDecl, TirProgramV2};

/// Which positions of an encoder's output the class takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncoderOutput {
    /// The last position's row: last-token (CLIP: `eos`) pooling.
    Final,
    /// Every position's row: the per-token hidden states.
    Rows,
}

/// The version-2 program of a causal encoder: the lowered program, reading only the token and the
/// position, with `post`'s output node as its `Final` or `Rows` output. Validated (NF-23 … NF-29
/// and version 1's normal form over the view).
pub fn causal_v2(lw: &Lowered, output: EncoderOutput) -> Result<TirProgramV2> {
    let node = lw.program.logits;
    let decl = match output {
        EncoderOutput::Final => OutputDecl::Final { node },
        EncoderOutput::Rows => OutputDecl::Rows { node },
    };
    let p = TirProgramV2::from_v1_lifting_params(&lw.program, &[], decl).map_err(|e| LowerError::eval(e.to_string()))?;
    tir::validate_v2::validate_v2(&p).map_err(|e| LowerError::eval(format!("version-2 normal form: {e}")))?;
    Ok(p)
}

/// The one-stage pipeline of a causal encoder class: `prefix ‖ prompt ‖ suffix` (padded with
/// `pad` when the class runs over pad ids, as a CLIP text encoder feeding a diffusion model does),
/// at most `max_trip` positions.
pub fn causal_pipeline(prefix: Vec<u32>, suffix: Vec<u32>, pad: Option<TokenPad>, max_trip: u32) -> TirPipelineV1 {
    TirPipelineV1 {
        version: 1,
        stages: vec![StageDecl {
            name: "encoder".into(),
            program: 0,
            trip: TripRule::TokenCount,
            max_trip: pad.map_or(max_trip, |p| p.to_len),
            tokens: Some(TokenRule { prefix, source: TokenSource::Prompt, suffix, pad }),
            bind: vec![],
        }],
        output_stage: 0,
    }
}

/// The version-2 program of a bidirectional encoder ([`crate::lower::bidir`]): its two input params
/// lifted into `External` inputs — `input.ids` `[L]` over `[0, vocab − 1]` and `input.count` `[]`
/// over `[0, L]` — and the pooled vector as its `Final` output. Validated.
pub fn bidir_v2(lw: &Lowered, vocab: u32, lmax: u32) -> Result<TirProgramV2> {
    use crate::lower::bidir::{COUNT_PARAM, IDS_PARAM};
    use tir::program_v2::InputSource;
    let idx = |name: &str| -> Result<u16> {
        lw.program
            .params
            .iter()
            .position(|p| p.name == name)
            .map(|i| i as u16)
            .ok_or_else(|| LowerError::eval(format!("internal: no input param `{name}`")))
    };
    let lifts = [
        (idx(IDS_PARAM)?, InputSource::External { lo: 0, hi: vocab as i64 - 1 }),
        (idx(COUNT_PARAM)?, InputSource::External { lo: 0, hi: lmax as i64 }),
    ];
    let p = TirProgramV2::from_v1_lifting_params(&lw.program, &lifts, OutputDecl::Final { node: lw.program.logits })
        .map_err(|e| LowerError::eval(e.to_string()))?;
    tir::validate_v2::validate_v2(&p).map_err(|e| LowerError::eval(format!("version-2 normal form: {e}")))?;
    Ok(p)
}

/// The version-1 params of a program some of whose params were lifted into inputs, re-indexed as
/// the version-2 program declares them (the lifted ones dropped, the others' order kept).
pub fn lifted_params(v1: &tir::TirProgramV1, lifted: &[&str], params: &crate::lower::IntParams) -> crate::lower::IntParams {
    let gone: Vec<u16> =
        v1.params.iter().enumerate().filter(|(_, p)| lifted.contains(&p.name.as_str())).map(|(i, _)| i as u16).collect();
    let mut out = crate::lower::IntParams::default();
    for ((j, layer), t) in &params.tensors {
        if gone.contains(j) {
            continue;
        }
        let shift = gone.iter().filter(|g| **g < *j).count() as u16;
        out.tensors.insert((j - shift, *layer), t.clone());
    }
    out
}

/// The one-stage pipeline of a bidirectional encoder class: `Fixed { n: 1 }`, the ids bound by
/// `JobTokens` and the count by `JobTokenCount`, both over `prefix ‖ prompt ‖ suffix` padded with
/// `pad` to `lmax`.
pub fn bidir_pipeline(prefix: Vec<u32>, suffix: Vec<u32>, pad: u32, lmax: u32) -> TirPipelineV1 {
    use tir::pipeline::Binding;
    let rule = TokenRule { prefix, source: TokenSource::Prompt, suffix, pad: Some(TokenPad { id: pad, to_len: lmax }) };
    TirPipelineV1 {
        version: 1,
        stages: vec![StageDecl {
            name: "encoder".into(),
            program: 0,
            trip: TripRule::Fixed { n: 1 },
            max_trip: 1,
            tokens: None,
            bind: vec![Binding::JobTokens { rule: rule.clone() }, Binding::JobTokenCount { rule }],
        }],
        output_stage: 0,
    }
}
