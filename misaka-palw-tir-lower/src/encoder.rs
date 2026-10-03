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
    let mut p = TirProgramV2::from_v1_lifting_params(&lw.program, &[], decl).map_err(|e| LowerError::eval(e.to_string()))?;
    // An `EmbeddingI32` is `[n, d]`: a `Final` row `[d]` is output as `[1, d]` (a committed
    // reshape of the committed row; a `Rows` output keeps `[d]` rows, stacked `[T, d]`).
    if output == EncoderOutput::Final {
        let post = p.schedule.post as usize;
        let row = &p.blocks[post].nodes[node as usize];
        let shape = row.out.shape.clone();
        if shape.len() == 1 {
            let dtype = row.out.dtype;
            let mut s = vec![tir::Dim::Fixed(1)];
            s.extend(shape);
            p.blocks[post].nodes.push(tir::Node {
                prim: tir::Prim::Reshape,
                inputs: vec![tir::Ref::Node(node)],
                out: tir::TensorType::new(dtype, s),
                commit: true,
            });
            p.output = OutputDecl::Final { node: (p.blocks[post].nodes.len() - 1) as u16 };
        }
    }
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

/// What a sentence-transformers repository says around the model: its pooling, whether a
/// `Normalize` module follows, and `max_seq_length` (`modules.json`, the Pooling module's
/// `config.json`, `sentence_bert_config.json`). `None` when the directory is not one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentenceTransformers {
    pub pooling: StPooling,
    pub normalize: bool,
    pub max_seq_length: Option<u32>,
}

/// The pooling modes the lowering models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StPooling {
    Cls,
    Mean,
    /// The last token's row (a causal embedder: the `Final` output).
    LastToken,
}

/// Read a sentence-transformers repository's module stack (`Transformer` → `Pooling` →
/// optional `Normalize`). A mode or module the lowering does not model (max, weighted-mean or
/// sqrt-length pooling, `Dense` layers) is refused by name rather than ignored.
pub fn sentence_transformers(dir: &std::path::Path) -> Result<Option<SentenceTransformers>> {
    let read = |p: std::path::PathBuf| -> Result<serde_json::Value> {
        let t = std::fs::read_to_string(&p).map_err(|e| LowerError::bad(format!("{}: {e}", p.display())))?;
        serde_json::from_str(&t).map_err(|e| LowerError::bad(format!("{}: {e}", p.display())))
    };
    let mpath = dir.join("modules.json");
    if !mpath.exists() {
        return Ok(None);
    }
    let modules = read(mpath)?;
    let list = modules.as_array().ok_or_else(|| LowerError::bad("modules.json is not a list"))?;
    let (mut pooling, mut normalize) = (None, false);
    for m in list {
        let ty = m["type"].as_str().unwrap_or("");
        let path = m["path"].as_str().unwrap_or("");
        match ty.rsplit('.').next().unwrap_or("") {
            "Transformer" => {}
            "Pooling" => {
                let c = read(dir.join(path).join("config.json"))?;
                let on = |k: &str| c[k].as_bool().unwrap_or(false);
                let modes: Vec<&str> = [
                    ("pooling_mode_cls_token", "cls"),
                    ("pooling_mode_mean_tokens", "mean"),
                    ("pooling_mode_lasttoken", "lasttoken"),
                    ("pooling_mode_max_tokens", "max"),
                    ("pooling_mode_mean_sqrt_len_tokens", "mean_sqrt_len"),
                    ("pooling_mode_weightedmean_tokens", "weightedmean"),
                ]
                .iter()
                .filter(|(k, _)| on(k))
                .map(|(_, n)| *n)
                .collect();
                pooling = Some(match modes.as_slice() {
                    ["cls"] => StPooling::Cls,
                    ["mean"] => StPooling::Mean,
                    ["lasttoken"] => StPooling::LastToken,
                    other => return Err(LowerError::not_lowerable(format!("sentence-transformers pooling {other:?} is not modelled"))),
                });
                if c["include_prompt"].as_bool() == Some(false) {
                    return Err(LowerError::not_lowerable("sentence-transformers `include_prompt: false` is not modelled"));
                }
            }
            "Normalize" => normalize = true,
            other => return Err(LowerError::not_lowerable(format!("sentence-transformers module `{other}` is not modelled"))),
        }
    }
    let pooling = pooling.ok_or_else(|| LowerError::not_lowerable("sentence-transformers stack without a Pooling module"))?;
    let max_seq_length = match dir.join("sentence_bert_config.json") {
        p if p.exists() => read(p)?["max_seq_length"].as_u64().map(|v| v as u32),
        _ => None,
    };
    Ok(Some(SentenceTransformers { pooling, normalize, max_seq_length }))
}

/// A decoder read as a last-token embedder (Qwen3-Embedding's shape; sentence-transformers'
/// `lasttoken` pooling): the same layers and final norm, no head, the final-norm row as the output,
/// L2-normalised when the class says. Its program is a causal encoder's ([`causal_v2`], `Final`).
pub fn as_last_token_embedder(mut spec: crate::spec::ArchSpec, normalize: bool) -> crate::spec::ArchSpec {
    spec.output = crate::spec::OutputSpec::Embedding { proj: None, normalize };
    spec.notes.push("read as a last-token embedder: the head is not part of the class".into());
    spec
}

/// Lift named version-1 params into `External` inputs and declare `output`: the version-2 program
/// of a one-position encoder whose inputs the lowering declared as params. Validated.
pub fn lift_v2(lw: &Lowered, lifts: &[(&str, tir::program_v2::InputSource)], output: OutputDecl) -> Result<TirProgramV2> {
    let mut v = Vec::with_capacity(lifts.len());
    for (name, src) in lifts {
        let i = lw
            .program
            .params
            .iter()
            .position(|p| p.name == *name)
            .ok_or_else(|| LowerError::eval(format!("internal: no input param `{name}`")))?;
        v.push((i as u16, *src));
    }
    let p = TirProgramV2::from_v1_lifting_params(&lw.program, &v, output).map_err(|e| LowerError::eval(e.to_string()))?;
    tir::validate_v2::validate_v2(&p).map_err(|e| LowerError::eval(format!("version-2 normal form: {e}")))?;
    Ok(p)
}

/// The version-2 program of a vision tower ([`crate::lower::vision`]): the canonical image
/// `input.image` (`i16 [H, W, 3]`) lifted into an `External` input over `[0, 255]`, and the tower's
/// output rows `[R, width]` as its `Final` output. `JobImage` (RFC-0003 II.4, rfc3/impl) will bind
/// that input; until it lands the program runs standalone.
pub fn vision_v2(lw: &Lowered) -> Result<TirProgramV2> {
    lift_v2(
        lw,
        &[(crate::lower::vision::IMAGE_PARAM, tir::program_v2::InputSource::External { lo: 0, hi: 255 })],
        OutputDecl::Final { node: lw.program.logits },
    )
}

/// The one-stage pipeline of a vision class (RFC-0003 II.4, the Embedding profile): the tower's
/// program over one position, its canonical image bound by `JobImage { index: 0 }`. The class
/// declares the image slot `{h, w}` the tower was lowered for, and its output (`Final [n, d]`) is
/// an `EmbeddingI32`.
pub fn vision_pipeline() -> TirPipelineV1 {
    use tir::pipeline::Binding;
    TirPipelineV1 {
        version: 1,
        stages: vec![StageDecl {
            name: "vision".into(),
            program: 0,
            trip: TripRule::Fixed { n: 1 },
            max_trip: 1,
            tokens: None,
            bind: vec![Binding::JobImage { index: 0 }],
        }],
        output_stage: 0,
    }
}

/// **An encoder-decoder's encoder stage** ([`crate::lower::encdec`]): `input.ids` (`idx [L]`, ids
/// below `vocab`) and `input.count` (`idx []` in `[0, L]`) lifted into inputs, and the stacked
/// cross keys and values `i16 [D, 2, L, inner]` as its `Final` output.
pub fn encdec_encoder_v2(lw: &Lowered, vocab: u32, lmax: u32) -> Result<TirProgramV2> {
    use crate::lower::encdec::{COUNT_PARAM, IDS_PARAM};
    use tir::program_v2::InputSource;
    lift_v2(
        lw,
        &[
            (IDS_PARAM, InputSource::External { lo: 0, hi: vocab as i64 - 1 }),
            (COUNT_PARAM, InputSource::External { lo: 0, hi: lmax as i64 }),
        ],
        OutputDecl::Final { node: lw.program.logits },
    )
}

/// **A feature-frame encoder-decoder's encoder stage** (Whisper): `input.mel` (`i16 [frames, bins]`, a fixed point of
/// `2^-MEL_Q`, over `[−32768, 32767]`) lifted into an input, the stacked cross keys and values (or, for an encoder alone, the
/// rows) its `Final` output. A class needs a binding that supplies the frames (`JobAudio`, FR-23: not in the protocol yet); until
/// then the program runs standalone.
pub fn encdec_frames_encoder_v2(lw: &Lowered) -> Result<TirProgramV2> {
    use crate::lower::encdec::MEL_PARAM;
    use tir::program_v2::InputSource;
    lift_v2(lw, &[(MEL_PARAM, InputSource::External { lo: -32768, hi: 32767 })], OutputDecl::Final { node: lw.program.logits })
}

/// **The decoder of a fixed-length source** (the encoder's rows are all real: no count, no mask): `input.xkv` alone lifted.
pub fn encdec_fixed_decoder_v2(lw: &Lowered) -> Result<TirProgramV2> {
    use crate::lower::encdec::XKV_PARAM;
    use tir::program_v2::InputSource;
    lift_v2(
        lw,
        &[(XKV_PARAM, InputSource::External { lo: -32767, hi: 32767 })],
        OutputDecl::Logits { node: lw.program.logits, scheme_id: lw.program.logits_scheme_id },
    )
}

/// **An encoder-decoder's decoder**, the text stage: `input.xkv` (the encoder's codes, `i16` over
/// `[−32767, 32767]`) and `input.enc_count` (`idx []` in `[0, L]`) lifted, its logits the
/// `Logits` output.
pub fn encdec_decoder_v2(lw: &Lowered, lmax: u32) -> Result<TirProgramV2> {
    use crate::lower::encdec::{ENC_COUNT_PARAM, XKV_PARAM};
    use tir::program_v2::InputSource;
    lift_v2(
        lw,
        &[
            (XKV_PARAM, InputSource::External { lo: -32767, hi: 32767 }),
            (ENC_COUNT_PARAM, InputSource::External { lo: 0, hi: lmax as i64 }),
        ],
        OutputDecl::Logits { node: lw.program.logits, scheme_id: lw.program.logits_scheme_id },
    )
}

/// The two-stage pipeline of an encoder-decoder class: the encoder over the source template
/// (`JobTokens` and `JobTokenCount` of `source`, padded to `L`), once (`Fixed { n: 1 }`); then the
/// decoder as the text stage (`TextStream`, the job's prompt its forced prefix: at least the
/// decoder start id), reading the encoder's stacked K/V (`StageFinal`) and the same count.
pub fn encdec_pipeline(source: TokenRule, max_trip: u32) -> TirPipelineV1 {
    use tir::pipeline::Binding;
    TirPipelineV1 {
        version: 1,
        stages: vec![
            StageDecl {
                name: "encoder".into(),
                program: 0,
                trip: TripRule::Fixed { n: 1 },
                max_trip: 1,
                tokens: None,
                bind: vec![Binding::JobTokens { rule: source.clone() }, Binding::JobTokenCount { rule: source.clone() }],
            },
            StageDecl {
                name: "decoder".into(),
                program: 1,
                trip: TripRule::TextStream,
                max_trip,
                tokens: None,
                bind: vec![Binding::StageFinal { stage: 0 }, Binding::JobTokenCount { rule: source }],
            },
        ],
        output_stage: 1,
    }
}

/// **The cross K/V stage of a vision cross-attention decoder** ([`crate::lower::cross`]): `input.cross_states` (`i32 [rows, hidden]`,
/// the vision stage's projected rows at the class's fixed unit, over `[lo, hi]`) lifted into an input, the stack of every cross layer's
/// keys and values `i16 [Dc, 2, rows, inner]` its `Final` output. `lo`/`hi` are the vision stage's output interval.
pub fn cross_kv_v2(lw: &Lowered, lo: i64, hi: i64) -> Result<TirProgramV2> {
    use crate::lower::cross::STATES_PARAM;
    use tir::program_v2::InputSource;
    lift_v2(lw, &[(STATES_PARAM, InputSource::External { lo, hi })], OutputDecl::Final { node: lw.program.logits })
}

/// **The text stage of a vision cross-attention decoder**: `input.xkv` (the stack's `i16` codes) lifted, its logits the output.
pub fn cross_text_v2(lw: &Lowered) -> Result<TirProgramV2> {
    use crate::lower::cross::XKV_PARAM;
    use tir::program_v2::InputSource;
    lift_v2(
        lw,
        &[(XKV_PARAM, InputSource::External { lo: -32767, hi: 32767 })],
        OutputDecl::Logits { node: lw.program.logits, scheme_id: lw.program.logits_scheme_id },
    )
}

/// The pipeline of a vision cross-attention class: the class's vision stage `tower` (program 0: the tower over `JobImage`, its
/// `Final` the projected rows), the cross K/V stage (program 1, `StageFinal { 0 }`, once), then the text stage (program 2,
/// `TextStream`, `StageFinal { 1 }`).
pub fn cross_pipeline(tower: StageDecl, max_trip: u32) -> TirPipelineV1 {
    use tir::pipeline::Binding;
    TirPipelineV1 {
        version: 1,
        stages: vec![
            tower,
            StageDecl { name: "cross_kv".into(), program: 1, trip: TripRule::Fixed { n: 1 }, max_trip: 1, tokens: None, bind: vec![Binding::StageFinal { stage: 0 }] },
            StageDecl { name: "text".into(), program: 2, trip: TripRule::TextStream, max_trip, tokens: None, bind: vec![Binding::StageFinal { stage: 1 }] },
        ],
        output_stage: 2,
    }
}
