//! **The first image profile as a pipeline** (RFC-0003 §6, `TirPipelineV1`): the text encoder's two stages, the
//! denoise scan and the VAE decoder's chain, bound by structural edges only.
//!
//! ```text
//!   0  text_rows   TokenCount, bos ‖ prompt ‖ eos padded to L   Rows  -> [L, d] i32
//!   1  text_pool   TokenCount, bos ‖ prompt ‖ eos               Final -> [1, d] i32   (the last position is the eos)
//!   2  denoise     JobSteps   in.text  <- StageRows {0, pad_to L}
//!                             in.pooled <- StageFinal {1}
//!                             in.steps <- JobScalar {0}                (+ the Random noise input, domain 1)
//!   3.. vae.*      Fixed {1}  in.x <- StageFinal {previous}            ... vae.out -> [H, W, 3] i16 in [0, 255]
//! ```
//!
//! The text stages are the HF frontend's own lowerings (`encoder::causal_v2`), handed in with their params; the
//! denoiser and the decoder are this module tree's. Nothing runs conditionally: every stage always runs, over a trip
//! count fixed when the job is accepted.

use misaka_palw_tir::pipeline::{
    Binding, PipelineParams, StageDecl, TirPipelineV1, TokenPad, TokenRule, TokenSource, TripRule, validate_pipeline,
};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::{MapParams, ParamSource};

use super::dit::DitStage;
use super::vae::VaeChain;

/// The text encoder's two stages and its token template.
pub struct TextStagesV1 {
    /// `Rows`: the per-token hidden rows (`last_hidden_state`) over the padded template.
    pub rows: (TirProgramV2, MapParams),
    /// `Final`: the pooled vector at the `eos`.
    pub pool: (TirProgramV2, MapParams),
    pub bos: u32,
    pub eos: u32,
    pub pad: u32,
    /// The padded length `L` the denoiser reads.
    pub to_len: u32,
}

/// The assembled pipeline with every program's params.
pub struct Sd3Pipeline {
    pub pipeline: TirPipelineV1,
    pub programs: Vec<TirProgramV2>,
    pub params: Vec<MapParams>,
}

impl PipelineParams for Sd3Pipeline {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.params[program as usize]
    }
}

/// Assemble the pipeline and check its normal form (NF-P1 … P10: every edge's shape, dtype and interval proved).
pub fn assemble_sd3_pipeline(text: TextStagesV1, dit: &DitStage, vae: &VaeChain) -> Result<Sd3Pipeline, String> {
    let mut programs = vec![text.rows.0, text.pool.0, dit.program.clone()];
    let mut params = vec![text.rows.1, text.pool.1, bind(&dit.program, &dit.sink)];
    for st in &vae.stages {
        programs.push(st.program.clone());
        params.push(bind(&st.program, &st.sink));
    }
    let template =
        |pad: Option<TokenPad>| TokenRule { prefix: vec![text.bos], source: TokenSource::Prompt, suffix: vec![text.eos], pad };
    let max_steps = *dit.spec.counts.last().ok_or("no step counts")?;
    let mut stages = vec![
        StageDecl {
            name: "text_rows".into(),
            program: 0,
            trip: TripRule::TokenCount,
            max_trip: text.to_len,
            tokens: Some(template(Some(TokenPad { id: text.pad, to_len: text.to_len }))),
            bind: vec![],
        },
        StageDecl {
            name: "text_pool".into(),
            program: 1,
            trip: TripRule::TokenCount,
            max_trip: text.to_len,
            tokens: Some(template(None)),
            bind: vec![],
        },
        StageDecl {
            name: "denoise".into(),
            program: 2,
            trip: TripRule::JobSteps,
            max_trip: max_steps,
            tokens: None,
            bind: vec![
                Binding::StageRows { stage: 0, drop: 0, pad_to: text.to_len },
                Binding::StageFinal { stage: 1 },
                Binding::JobScalar { index: 0 },
            ],
        },
    ];
    for (k, st) in vae.stages.iter().enumerate() {
        stages.push(StageDecl {
            name: st.name.clone(),
            program: 3 + k as u16,
            trip: TripRule::Fixed { n: 1 },
            max_trip: 1,
            tokens: None,
            bind: vec![Binding::StageFinal { stage: (2 + k) as u8 }],
        });
    }
    let output_stage = (stages.len() - 1) as u8;
    let pipeline = TirPipelineV1 { version: 1, stages, output_stage };
    validate_pipeline(&pipeline, &programs).map_err(|e| format!("the pipeline's normal form: {e}"))?;
    Ok(Sd3Pipeline { pipeline, programs, params })
}

fn bind(program: &TirProgramV2, sink: &super::sink::ParamSink) -> MapParams {
    let names: Vec<String> = program.params.iter().map(|p| p.name.clone()).collect();
    sink.bind(names.iter().map(String::as_str))
}
