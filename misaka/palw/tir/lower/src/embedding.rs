//! **A bidirectional embedding encoder as the pieces of a registered Embedding class** (RFC-0003
//! Part II.3, activation step 7).
//!
//! [`lower_bidir_embedding_v1`] takes a Hugging Face encoder directory (BERT, RoBERTa, XLM-R,
//! DistilBERT, MPNet — the families [`crate::lower::bidir`] lowers), runs the float reference over a
//! calibration set, materialises the integer parameters and returns what a class is made of: the
//! version-2 program (its pooled vector as a `Final` output of shape `[1, d]`, the shape an
//! `EmbeddingI32` header declares), the one-stage pipeline (`JobTokens` over `prefix ‖ prompt ‖ suffix`
//! padded to `lmax`, `JobTokenCount`), the integer parameters in the version-2 program's order and the
//! float value of one unit of the output. Declaring layouts, the output header and offers, the
//! artifact root, the registration and the container are the SDK's (`palw-class`); running the class is
//! the node's (`GenHeldClassV1`).
//!
//! Nothing here is specific to BERT: it composes [`crate::lower::bidir`] with the encoder module's
//! `bidir_v2`/`bidir_pipeline` and adds the one reshape an embedding row needs.

use crate::encoder;
use crate::error::{LowerError, Result};
use crate::fidelity;
use crate::float_ref::ParamStore;
use crate::float_ref::stream::Resident;
use crate::lower::bidir::{self, BidirCfg, COUNT_PARAM, IDS_PARAM, Padded, Pooling};
use crate::lower::{IntParams, materialise};
use crate::quant::QuantPolicy;
use crate::weights::Checkpoint;
use misaka_palw_tir as tir;
use std::path::Path;
use std::sync::Arc;
use tir::pipeline::TirPipelineV1;
use tir::program_v2::{OutputDecl, TirProgramV2};

/// What an operator chooses around the model (the checkpoint says everything else).
#[derive(Clone, Debug)]
pub struct EmbeddingLowerOptsV1 {
    pub pooling: Pooling,
    /// L2-normalise the pooled vector (sentence-transformers' `Normalize`).
    pub normalize: bool,
    /// The padded token axis `L`: at most `lmax − 2` prompt ids fit between the template's two ids.
    pub lmax: u32,
    /// The tokenizer's pad, class-start and separator ids (`[PAD]`, `[CLS]`, `[SEP]` for BERT).
    pub pad: u32,
    pub cls: u32,
    pub sep: u32,
    /// Random templated sequences the float reference is calibrated on, and the seed they are drawn from.
    pub calibration_sequences: usize,
    pub calibration_seed: u64,
}

/// The pieces of an Embedding class a lowered encoder yields.
pub struct LoweredEmbeddingV1 {
    pub program: TirProgramV2,
    pub pipeline: TirPipelineV1,
    /// The integer parameters, in the version-2 program's order (the two lifted input params dropped).
    pub params: IntParams,
    /// The float value of one unit of the output row.
    pub logits_scale: f64,
    /// `Some(q)` when one unit is exactly `2^-q` (a normalised embedding is `Q30`): the `q` an
    /// `EmbeddingI32` header declares. `None` when the unit is a calibrated value — the header then
    /// declares `q = 0` and the unit is provenance, which no verdict reads.
    pub q: Option<u8>,
    pub dims: u32,
    pub vocab: u32,
    /// The most prompt ids a job may carry: `lmax − |prefix| − |suffix|`.
    pub max_prompt_tokens: u32,
    pub lmax: u32,
    pub pooling: Pooling,
    pub normalised: bool,
    pub prefix: Vec<u32>,
    pub suffix: Vec<u32>,
    pub pad: u32,
}

/// **Lower a bidirectional encoder directory** (`config.json`, the weights) into the pieces of an
/// Embedding class.
pub fn lower_bidir_embedding_v1(dir: &Path, opts: &EmbeddingLowerOptsV1) -> Result<LoweredEmbeddingV1> {
    if opts.lmax < 3 {
        return Err(LowerError::eval("an embedding class needs room for a prompt between its template ids (lmax ≥ 3)"));
    }
    let cfg_text =
        std::fs::read_to_string(dir.join("config.json")).map_err(|e| LowerError::eval(format!("{}: {e}", dir.display())))?;
    let spec = crate::parse_config_str(&cfg_text)?;
    let hl = crate::hl::build_program(&spec)?;
    let binding = crate::hf_weights::bind(&spec, &hl)?;
    let ck = Checkpoint::open(dir)?;
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck)?;
    let cfg = BidirCfg { lmax: opts.lmax, pooling: opts.pooling, normalize: opts.normalize };
    // Calibrate on random templated sequences of several lengths.
    let mut stats = std::collections::BTreeMap::new();
    let body_max = opts.lmax as usize - 2;
    for (i, body) in fidelity::random_sequences(spec.vocab_size, opts.calibration_sequences.max(1), body_max, opts.calibration_seed)
        .into_iter()
        .enumerate()
    {
        let n = 1 + (i % body_max);
        let mut ids: Vec<usize> =
            std::iter::once(opts.cls as usize).chain(body.into_iter().take(n)).chain(std::iter::once(opts.sep as usize)).collect();
        let count = ids.len().min(opts.lmax as usize);
        ids.truncate(opts.lmax as usize);
        ids.resize(opts.lmax as usize, opts.pad as usize);
        bidir::float_forward(&hl, &spec, &cfg, &params_f, &Padded { ids, count }, Some(&mut stats))?;
    }
    let lw = bidir::lower_bidir(&hl, &spec, &cfg)?;
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet)?;
    // Version 2: the ids and the count lifted into inputs, the pooled vector the `Final` output.
    let vocab = spec.vocab_size as u32;
    let program = embedding_row_v2(encoder::bidir_v2(&lw, vocab, opts.lmax)?)?;
    let params = encoder::lifted_params(&lw.program, &[IDS_PARAM, COUNT_PARAM], &mat.params);
    let (prefix, suffix) = (vec![opts.cls], vec![opts.sep]);
    let pipeline = encoder::bidir_pipeline(prefix.clone(), suffix.clone(), opts.pad, opts.lmax);
    tir::pipeline::validate_pipeline(&pipeline, std::slice::from_ref(&program))
        .map_err(|e| LowerError::eval(format!("the pipeline's normal form: {e}")))?;
    let dims = match program.output {
        OutputDecl::Final { node } => {
            let post = &program.blocks[program.schedule.post as usize];
            post.nodes[node as usize].out.elements_at(1) as u32
        }
        _ => return Err(LowerError::eval("an embedding program's output is its `Final` row")),
    };
    let q = {
        let bits = (-mat.logits_scale.log2()).round();
        (bits >= 0.0 && bits <= 31.0 && (mat.logits_scale - (-bits).exp2()).abs() == 0.0).then_some(bits as u8)
    };
    Ok(LoweredEmbeddingV1 {
        program,
        pipeline,
        params,
        logits_scale: mat.logits_scale,
        q,
        dims,
        vocab,
        max_prompt_tokens: opts.lmax - 2,
        lmax: opts.lmax,
        pooling: opts.pooling,
        normalised: opts.normalize,
        prefix,
        suffix,
        pad: opts.pad,
    })
}

/// **A `Final` row as the `[1, d]` an `EmbeddingI32` header declares**: a one-dimensional output is
/// followed by a committed reshape (the committed row's own elements, in order); a `[1, d]` output is
/// returned as it is. The program is re-validated.
pub fn embedding_row_v2(mut p: TirProgramV2) -> Result<TirProgramV2> {
    let OutputDecl::Final { node } = p.output else {
        return Err(LowerError::eval("an embedding row is a `Final` output"));
    };
    let post = p.schedule.post as usize;
    let row = p.blocks[post].nodes[node as usize].out.clone();
    if row.shape.len() == 1 {
        let mut shape = vec![tir::Dim::Fixed(1)];
        shape.extend(row.shape.clone());
        p.blocks[post].nodes.push(tir::Node {
            prim: tir::Prim::Reshape,
            inputs: vec![tir::Ref::Node(node)],
            out: tir::TensorType::new(row.dtype, shape),
            commit: true,
        });
        p.output = OutputDecl::Final { node: (p.blocks[post].nodes.len() - 1) as u16 };
    }
    tir::validate_v2::validate_v2(&p).map_err(|e| LowerError::eval(format!("version-2 normal form: {e}")))?;
    Ok(p)
}
