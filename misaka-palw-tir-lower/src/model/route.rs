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
