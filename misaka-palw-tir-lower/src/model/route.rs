//! **The data routes of the kinds the decoder pipeline does not run** (RFC-0002 Part II §II.2.9, RFC-0003 §II.2.1): a vision tower, a
//! convolutional network, an encoder–decoder (Whisper's feature-frame encoder and T5's encoder alone included) and the diffusers
//! components that have a route are read by an adapter of their own kind (`read_vision`, `read_cnn`, `read_encdec`, `read_diffusers`),
//! lowered to their RFC-0003 version-2 programs and ADMITTED. This is the read + lower + admit the corpus harness measures every such
//! entry by and the preflight judges them by; the weights stages of these routes (float against transformers, integer against it, the
//! three implementations, the court) are the lowering crate's own tests.

use crate::hf_schema::{AdapterChoice, ReadOptions, TensorIndex};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

fn short_msg(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

/// Whether `cfg` is a kind this module routes (not a causal language model).
pub fn is_data_route(cfg: &Value) -> bool {
    crate::hf_schema::is_diffusers(cfg)
        || crate::hf_schema::is_vision_tower(cfg)
        || crate::hf_schema::is_cnn(cfg)
        || crate::hf_schema::is_encoder_decoder(cfg)
}

/// Read, lower and admit `cfg` by its kind's route: `{"ok": true, "kind", "adapter", "programs": [...]}` or `{"ok": false, "error"}`.
pub fn probe_data_route(cfg: &Value, tensors: Option<&TensorIndex>, dir: Option<&Path>) -> Value {
    use crate::encoder;
    use crate::hf_schema::{AdapterSource, is_cnn, is_vision_tower, read_cnn, read_encdec, read_vision};
    use crate::lower::{cnn, encdec, vision};
    let inputs = crate::admission::default_inputs();
    let adm = |p: &misaka_palw_tir::program_v2::TirProgramV2| -> Result<Value, String> {
        let a = misaka_palw_tir::admit_v2::tir_admit_program_v2(p, &inputs).map_err(|e| format!("tir_admit_program_v2 refuses: {e}"))?;
        Ok(json!({
            "nodes": p.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(),
            "blocks": p.blocks.len(),
            "macs": a.view.position.cost.macs as f64,
            "step_leaves": a.view.position.step_leaves,
        }))
    };
    let id_of = |a: &AdapterSource| match a {
        AdapterSource::BuiltIn { id, .. } => id.clone(),
        other => format!("{other:?}"),
    };
    let opts = ReadOptions { adapter: AdapterChoice::Auto };
    let r = catch_unwind(AssertUnwindSafe(|| -> Result<Value, String> {
        if crate::hf_schema::is_diffusers(cfg) {
            // A diffusers component (`_class_name`): the route that exists for its class (the SD3 denoiser stage, the VAE decoder's chain of
            // stages), read, lowered from the fixture's weights on a seeded random calibration and ADMITTED stage by stage, every tensor accounted
            // for; every other class is refused with the capabilities its lowering lacks named.
            use crate::diffusion::probe::{probe_sd3_transformer, probe_vae_decoder};
            use crate::hf_schema::{DiffusersRoute, read_diffusers};
            let r = read_diffusers(cfg).map_err(|f| format!("{} [missing: {}]", f.error, f.missing.iter().map(|m| m.what.as_str()).collect::<Vec<_>>().join(", ")))?;
            let dir = dir.ok_or("no checkpoint to lower the component from")?;
            let ck = crate::weights::Checkpoint::open(&dir.join("diffusion_pytorch_model.safetensors")).map_err(|e| format!("checkpoint: {e}"))?;
            let probe = match &r.route {
                DiffusersRoute::Sd3Transformer(_) => probe_sd3_transformer(cfg, &ck)?,
                DiffusersRoute::VaeDecoder(c) => probe_vae_decoder(cfg, &ck, Some(cfg["sample_size"].as_u64().map(|s| s as usize / c.upscale()).unwrap_or(8)))?,
            };
            if !probe.unread.is_empty() {
                return Err(format!("checkpoint tensors nothing reads: {:?}", probe.unread));
            }
            let mut programs = Vec::new();
            for st in &probe.stages {
                let (macs, leaves) = st.admission.clone().map_err(|e| format!("stage `{}` refused: {e}", st.stage))?;
                programs.push(json!({"stage": st.stage, "nodes": st.nodes, "blocks": st.blocks, "macs": macs, "step_leaves": leaves}));
            }
            return Ok(json!({"kind": "diffusers", "adapter": r.route.id(), "programs": programs}));
        }
        if is_vision_tower(cfg) {
            let r = read_vision(cfg, &opts).map_err(|f| f.error.to_string())?;
            let (hl, _) = vision::hl_program(&r.spec).map_err(|e| format!("hl: {e}"))?;
            let lw = vision::lower_vision(&hl, &r.spec).map_err(|e| format!("lower: {e}"))?;
            let p2 = encoder::vision_v2(&lw).map_err(|e| format!("v2: {e}"))?;
            Ok(json!({"kind": "vision", "adapter": id_of(&r.adapter), "programs": [adm(&p2)?]}))
        } else if is_cnn(cfg) {
            let r = read_cnn(cfg, &opts).map_err(|f| f.error.to_string())?;
            let (hl, _) = cnn::hl_program(&r.spec).map_err(|e| format!("hl: {e}"))?;
            let lw = cnn::lower_cnn(&hl, &r.spec).map_err(|e| format!("lower: {e}"))?;
            let p2 = encoder::vision_v2(&lw).map_err(|e| format!("v2: {e}"))?;
            Ok(json!({"kind": "cnn", "adapter": id_of(&r.adapter), "programs": [adm(&p2)?]}))
        } else if crate::hf_schema::is_encoder_decoder(cfg) {
            let r = read_encdec(cfg, &opts).map_err(|f| f.error.to_string())?;
            let s = r.spec;
            let names: BTreeSet<String> = tensors.map(|t| t.names().map(str::to_string).collect()).unwrap_or_default();
            let has = |n: &str| names.contains(n);
            // A fixed-length source (feature frames) is lowered at its own length; ids are padded to 16.
            let l = if s.fixed_source() { s.enc_pos_rows.unwrap_or(16) as u32 } else { 16 };
            let mut programs = Vec::new();
            if s.encoder_only() {
                let (ehl, _) = encdec::hl_encoder(&s, l as usize, &has).map_err(|e| format!("hl: {e}"))?;
                let elw = encdec::lower_encoder(&ehl, &s, l).map_err(|e| format!("lower encoder: {e}"))?;
                programs.push(adm(&encoder::encdec_encoder_v2(&elw, s.vocab as u32, l).map_err(|e| format!("v2: {e}"))?)?);
            } else {
                let ((ehl, _), (dhl, _)) = encdec::hl_programs(&s, l as usize, &has).map_err(|e| format!("hl: {e}"))?;
                let elw = encdec::lower_encoder(&ehl, &s, l).map_err(|e| format!("lower encoder: {e}"))?;
                let dlw = encdec::lower_decoder(&dhl, &s, l, 64).map_err(|e| format!("lower decoder: {e}"))?;
                let (e2, d2) = if s.fixed_source() {
                    (encoder::encdec_frames_encoder_v2(&elw), encoder::encdec_fixed_decoder_v2(&dlw))
                } else {
                    (encoder::encdec_encoder_v2(&elw, s.vocab as u32, l), encoder::encdec_decoder_v2(&dlw, l))
                };
                programs.push(adm(&e2.map_err(|e| format!("v2: {e}"))?)?);
                programs.push(adm(&d2.map_err(|e| format!("v2: {e}"))?)?);
            }
            Ok(json!({"kind": "encdec", "adapter": id_of(&r.adapter), "programs": programs}))
        } else {
            Err("no adapter of kind vision, cnn or encdec claims this configuration".into())
        }
    }));
    match r {
        Ok(Ok(v)) => {
            let mut v = v;
            v["ok"] = json!(true);
            v
        }
        Ok(Err(m)) => json!({"ok": false, "error": short_msg(&m, 400)}),
        Err(p) => json!({"ok": false, "error": short_msg(&p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default(), 300)}),
    }
}

/// **An encoder–decoder's two stages, shape-only, at a declared source and target length** (RFC-0003 §II.2.2): the encoder over a
/// `source_len`-id padded source (`TokenSource::Source`) and the decoder as the text stage over a `target_len`-position stream, with
/// the decoder's start id as the class's forced prompt prefix. What a pipeline class registers is these two programs under a pipeline
/// ([`encdec_pipeline_v1`]) — the same programs [`probe_data_route`] admits one by one at a 16-id toy source, here at the length the
/// class declares.
#[derive(Clone, Debug)]
pub struct EncDecStagesV1 {
    /// The adapter that read the configuration (`t5`, `bart`, …).
    pub adapter: String,
    pub encoder: misaka_palw_tir::program_v2::TirProgramV2,
    pub decoder: misaka_palw_tir::program_v2::TirProgramV2,
    pub vocab: u32,
    /// The decoder's start id (the class's forced first prompt id).
    pub decoder_start: u32,
    /// The configuration's end-of-sequence id (the source's template suffix), when it declares one.
    pub eos: Option<u32>,
    /// The id the padded source is filled with (the configuration's pad id, else 0): the encoder masks by the source's count.
    pub pad: u32,
    pub source_len: u32,
    pub target_len: u32,
}

/// What an encoder–decoder configuration yields at the shape depth.
#[derive(Clone, Debug)]
pub enum EncDecShapeV1 {
    /// Both stages lowered: the class can be declared shape-only and judged by the pipeline admission.
    Stages(Box<EncDecStagesV1>),
    /// A class that cannot be declared from headers in this build, and why (never a pass): a fixed-length feature-frame source (the
    /// protocol has no audio-frame job binding) or an encoder alone (an embedding-profile class, whose lowering calibrates on weights).
    NotDeclarable(String),
}

/// The two-stage pipeline of an encoder–decoder class whose source is the job's `TokenSource::Source` ids, templated `ids ‖ suffix` and
/// padded with `pad` to `source_len`, then a text stream of at most `target_len` positions (`encoder::encdec_pipeline`).
pub fn encdec_pipeline_v1(source_len: u32, target_len: u32, pad: u32, suffix: Vec<u32>) -> misaka_palw_tir::pipeline::TirPipelineV1 {
    use misaka_palw_tir::pipeline::{TokenPad, TokenRule, TokenSource};
    let rule = TokenRule { prefix: vec![], source: TokenSource::Source, suffix, pad: Some(TokenPad { id: pad, to_len: source_len }) };
    crate::encoder::encdec_pipeline(rule, target_len)
}

/// A token id a configuration declares (a number, or the first of a list).
fn id_of(v: Option<&Value>) -> Option<u32> {
    match v? {
        Value::Array(a) => a.first().and_then(Value::as_u64),
        other => other.as_u64(),
    }
    .and_then(|x| u32::try_from(x).ok())
}

/// Read, lower and lift an encoder–decoder configuration to its two version-2 stage programs at `source_len` / `target_len`.
/// `Err` is a refusal of the lowering at that length (a source longer than the position table, a feature the adapter lacks).
pub fn lower_encdec_stages_v1(
    cfg: &Value,
    tensors: Option<&TensorIndex>,
    source_len: u32,
    target_len: u32,
) -> Result<EncDecShapeV1, String> {
    use crate::encoder;
    use crate::hf_schema::{AdapterSource, read_encdec};
    use crate::lower::encdec;
    let r = catch_unwind(AssertUnwindSafe(|| -> Result<EncDecShapeV1, String> {
        let read = read_encdec(cfg, &ReadOptions { adapter: AdapterChoice::Auto }).map_err(|f| f.error.to_string())?;
        let s = read.spec;
        let adapter = match &read.adapter {
            AdapterSource::BuiltIn { id, .. } => id.clone(),
            other => format!("{other:?}"),
        };
        if s.fixed_source() {
            return Ok(EncDecShapeV1::NotDeclarable(
                "its source is a fixed-length feature-frame stack (audio): the protocol has no job binding that supplies frames"
                    .into(),
            ));
        }
        if s.encoder_only() {
            return Ok(EncDecShapeV1::NotDeclarable(
                "an encoder alone is an embedding-profile class, whose lowering calibrates on the checkpoint's weights".into(),
            ));
        }
        let names: BTreeSet<String> = tensors.map(|t| t.names().map(str::to_string).collect()).unwrap_or_default();
        let has = |n: &str| names.contains(n);
        let ((ehl, _), (dhl, _)) = encdec::hl_programs(&s, source_len as usize, &has).map_err(|e| format!("hl: {e}"))?;
        let elw = encdec::lower_encoder(&ehl, &s, source_len).map_err(|e| format!("lower encoder: {e}"))?;
        let dlw = encdec::lower_decoder(&dhl, &s, source_len, target_len).map_err(|e| format!("lower decoder: {e}"))?;
        let encoder_v2 = encoder::encdec_encoder_v2(&elw, s.vocab as u32, source_len).map_err(|e| format!("v2: {e}"))?;
        let decoder_v2 = encoder::encdec_decoder_v2(&dlw, source_len).map_err(|e| format!("v2: {e}"))?;
        Ok(EncDecShapeV1::Stages(Box::new(EncDecStagesV1 {
            adapter,
            encoder: encoder_v2,
            decoder: decoder_v2,
            vocab: s.vocab as u32,
            decoder_start: s.decoder_start,
            eos: id_of(cfg.get("eos_token_id")),
            pad: id_of(cfg.get("pad_token_id")).unwrap_or(0),
            source_len,
            target_len,
        })))
    }));
    match r {
        Ok(v) => v,
        Err(p) => Err(short_msg(
            &p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default(),
            300,
        )),
    }
}

/// What a bidirectional encoder class computes (the head over the encoder's rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BidirHeadV1 {
    /// A pooled sentence embedding (`OUTPUT_EMBEDDING_V1`).
    Embedding,
    /// A sequence classifier's logits on the pooled row (`OUTPUT_CLASSIFY_V1`).
    Sequence,
    /// Per-token logits (`OUTPUT_TOKEN_LOGITS_V1`): a token classifier.
    Token,
    /// Per-token start and end logits (`OUTPUT_TOKEN_LOGITS_V1`, two): an extractive QA span head.
    SpanQa,
    /// Per-token vocabulary logits (`OutputSpec::MaskedLm`): a masked-language-model head (`fill-mask`). The row at the masked position is
    /// the answer; the program reads no position (HFX 2026-10-10).
    MaskedLm,
}

impl BidirHeadV1 {
    pub fn name(self) -> &'static str {
        match self {
            BidirHeadV1::Embedding => "embedding",
            BidirHeadV1::Sequence => "sequence",
            BidirHeadV1::Token => "token",
            BidirHeadV1::SpanQa => "span_qa",
            BidirHeadV1::MaskedLm => "masked_lm",
        }
    }
}

/// **A bidirectional encoder's class, shape-only** (HFX 2026-10-08): the one-stage RFC-0003 pipeline a BERT-lineage repository
/// registers as — an embedding, a sequence classifier or a token head — lowered from the specification alone. No weight is read: a
/// program's structure (its nodes, shapes and declared params) does not depend on the weights' values or on the calibration, which
/// fill the params at materialisation. The template ids are the configuration's (`cls`/`bos`, `sep`/`eos`, `pad`), else 0.
#[derive(Clone, Debug)]
pub struct BidirClassShapeV1 {
    pub head: BidirHeadV1,
    pub program: misaka_palw_tir::program_v2::TirProgramV2,
    pub pipeline: misaka_palw_tir::pipeline::TirPipelineV1,
    /// The output `[rows, width]`: `[1, d]` pooled, `[lmax, labels]` per token.
    pub rows: u32,
    pub width: u32,
    /// `PALW_GEN_POOLING_*`: 1 cls, 2 mean.
    pub pooling: u8,
    pub normalised: bool,
    pub lmax: u32,
    pub vocab: u32,
    pub template: (u32, u32, u32),
    /// `ENC_PAIR_SEGMENTS_V1`: the separator that closes the first segment of a pair input, when the program computes segment ids.
    pub pair_sep: Option<u32>,
}

/// The head a specification's output computes over a bidirectional encoder, or `None` for an output that is not one.
pub fn bidir_head_of(spec: &crate::spec::ArchSpec) -> Option<BidirHeadV1> {
    use crate::spec::OutputSpec;
    match &spec.output {
        OutputSpec::Embedding { proj: None, .. } => Some(BidirHeadV1::Embedding),
        OutputSpec::Classify { .. } => Some(BidirHeadV1::Sequence),
        OutputSpec::MaskedLm => Some(BidirHeadV1::MaskedLm),
        OutputSpec::TokenLogits { labels: 2, .. } if spec.architecture.ends_with("ForQuestionAnswering") => Some(BidirHeadV1::SpanQa),
        OutputSpec::TokenLogits { .. } => Some(BidirHeadV1::Token),
        _ => None,
    }
}

/// The feature a span head over a model with token types needs: BERT-type pair segments (token type 1 after the first separator).
/// Without it the program would add type row 0 to every position, so a `question ‖ sep ‖ context` input would be read as one segment —
/// not the function the checkpoint computes in its own pipeline. **Lowered since HFX 2026-10-10** (`lower::bidir::BidirExtras`):
/// the segment ids are computed in the program from the job's ids. `task-heads-profile-v1.md` §6.
pub const ENC_PAIR_SEGMENTS_V1: &str = "ENC_PAIR_SEGMENTS_V1";

/// **Does this head need [`ENC_PAIR_SEGMENTS_V1`]?** A span QA head (its input is a pair) over a token-type table of more than one row.
/// RoBERTa / XLM-R (one row) and DistilBERT (no table) do not.
pub fn bidir_needs_pair_segments_v1(spec: &crate::spec::ArchSpec) -> bool {
    bidir_head_of(spec) == Some(BidirHeadV1::SpanQa) && spec.embedding.type_rows.is_some_and(|rows| rows > 1)
}

/// Lower a bidirectional encoder's class shape-only at `lmax` padded positions (at most the position table's rows past its offset).
/// `mean` pools by the mean (an embedding's sentence-transformers default; the costlier of the two modes), else by `[CLS]`.
/// A span head over a token-type table lowers with **BERT-type pair segments** ([`ENC_PAIR_SEGMENTS_V1`], HFX 2026-10-10): the
/// separator is the configuration's (`sep_token_id`, else `eos_token_id`, else 0 — a shape-only class does not read a tokenizer;
/// [`lower_bidir_class_shape_with_v1`] takes the tokenizer's).
pub fn lower_bidir_class_shape_v1(
    spec: &crate::spec::ArchSpec,
    hl: &crate::hl::HlProgram,
    config: &Value,
    lmax: u32,
    mean: bool,
    normalize: bool,
) -> crate::error::Result<BidirClassShapeV1> {
    lower_bidir_class_shape_with_v1(spec, hl, config, lmax, mean, normalize, None)
}

/// [`lower_bidir_class_shape_v1`] with the pair separator the class's tokenizer names (`pair_sep`; `None`: the configuration's).
pub fn lower_bidir_class_shape_with_v1(
    spec: &crate::spec::ArchSpec,
    hl: &crate::hl::HlProgram,
    config: &Value,
    lmax: u32,
    mean: bool,
    normalize: bool,
    pair_sep: Option<u32>,
) -> crate::error::Result<BidirClassShapeV1> {
    use crate::lower::bidir::{self, BidirCfg, BidirExtras, Pooling};
    let head = bidir_head_of(spec).ok_or_else(|| crate::error::LowerError::not_lowerable("not a bidirectional encoder's head"))?;
    let pooled = matches!(head, BidirHeadV1::Embedding | BidirHeadV1::Sequence);
    let pooling = if head == BidirHeadV1::Embedding && mean { Pooling::Mean } else { Pooling::Cls };
    let normalize = normalize && head == BidirHeadV1::Embedding;
    let rows_max = spec.embedding.positions.as_ref().map(|p| p.rows.saturating_sub(p.offset) as u32);
    let lmax = rows_max.map_or(lmax, |r| lmax.min(r)).max(3);
    let vocab = spec.vocab_size as u32;
    let id = |keys: &[&str]| {
        keys.iter().find_map(|k| config.get(*k).and_then(Value::as_u64)).unwrap_or(0).min(vocab.saturating_sub(1) as u64) as u32
    };
    let template = (id(&["cls_token_id", "bos_token_id"]), id(&["sep_token_id", "eos_token_id"]), id(&["pad_token_id"]));
    // A span head over a token-type table reads `question ‖ sep ‖ context` as two segments (type 1 after the first separator).
    let extras = BidirExtras { pair_sep: bidir_needs_pair_segments_v1(spec).then(|| pair_sep.unwrap_or(template.1)) };
    let lw = bidir::lower_bidir_with(hl, spec, &BidirCfg { lmax, pooling, normalize }, &extras)?;
    let mut program = crate::encoder::bidir_v2(&lw, vocab, lmax)?;
    if pooled {
        program = crate::embedding::embedding_row_v2(program)?;
    }
    let misaka_palw_tir::program_v2::OutputDecl::Final { node } = program.output else {
        return Err(crate::error::LowerError::eval("a bidirectional encoder's output is Final"));
    };
    let shape = &program.blocks[program.schedule.post as usize].nodes[node as usize].out.shape;
    let ext = |d: &misaka_palw_tir::Dim| match d {
        misaka_palw_tir::Dim::Fixed(n) => Some(*n),
        _ => None,
    };
    let (rows, width) = match shape.as_slice() {
        [r, w] => (ext(r), ext(w)),
        _ => (None, None),
    };
    let (Some(rows), Some(width)) = (rows, width) else {
        return Err(crate::error::LowerError::eval(format!("a bidirectional encoder's output is [rows, width], not {shape:?}")));
    };
    let pipeline = crate::encoder::bidir_pipeline(vec![template.0], vec![template.1], template.2, lmax);
    misaka_palw_tir::pipeline::validate_pipeline(&pipeline, std::slice::from_ref(&program))
        .map_err(|e| crate::error::LowerError::eval(format!("pipeline normal form: {e}")))?;
    Ok(BidirClassShapeV1 {
        head,
        program,
        pipeline,
        rows,
        width,
        pooling: if pooling == Pooling::Mean { 2 } else { 1 },
        normalised: normalize,
        lmax,
        vocab,
        template,
        pair_sep: extras.pair_sep,
    })
}
