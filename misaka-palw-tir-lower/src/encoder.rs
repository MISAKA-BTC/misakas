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
