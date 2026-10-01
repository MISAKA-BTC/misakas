//! **Vision towers** (RFC-0003 Part II.4): a canonical image, `u8` HWC RGB at the class's declared
//! size, through the integer preprocessing and a ViT to the class's output rows. The whole tower is
//! ONE position over a fixed patch axis: every value is a `[L, …]` tensor.
//!
//! **Preprocessing, in the program.** The input is `input.image`, an `i16 [H, W, 3]` holding the
//! pixel values exactly (lifted into an `External` input over `[0, 255]`). It passes through three
//! steps:
//! * an optional fixed-ratio downscale: a box filter of ratio `r`, two `MatMul`s with pinned 0/1
//!   matrices and one exact `Div` by `r²` (RFC-0003 II.4's "two `MatMul`s with pinned resampling
//!   matrices");
//! * patchify: `Reshape`/`Transpose` at rank ≤ 4, giving CLIP's row-major patch order or
//!   Qwen2-VL's block-major one (the processor's `patchify`). Within a patch the order is
//!   `(i, j, c)`, and the patch projection's columns are permuted to match;
//! * per-channel normalisation `(x/255 − mean)/std`, folded into the patch projection:
//!   `W' = W / (255·std_c)` and `b' = b − Σ W·mean_c/std_c`. The projection then reads the pixels
//!   exactly (codes at scale 1). Qwen2-VL's `Conv3d` over two identical temporal copies folds the
//!   same way: `W' = Σ_t W_t`.
//!
//! **The towers.** The towers covered:
//! * CLIP's vision tower: CLS, learned positions, `pre_layrnorm`, pre-LN layers, `post_layernorm`
//!   on the CLS row, optional `visual_projection`;
//! * SigLIP: no CLS, `post_layernorm`, and the attention-pooling head, whose query is the probe
//!   through `W_q` (data, so a param);
//! * Qwen2-VL: the folded `Conv3d`, 2D rotary positions, full attention, and the merger (4 rows →
//!   MLP);
//! * Qwen2.5-VL: RMSNorm, SwiGLU, and window attention. The window order is a pinned row
//!   permutation, the window mask is `Compare` of pinned window ids, and the merger's output is
//!   put back in order;
//! * LLaVA's tower: CLIP up to `vision_feature_layer`, CLS dropped, then the projector.
//!
//! The params live in a synthetic HL program (one anchor node per block, so `materialise` and the
//! binding see them per occurrence). The float reference [`float_forward`] uses the same site
//! names, and it calibrates.

use super::bidir::{add_rows, codes_rows, hl_param, input_fill, linear_rows, norm_rows_kind, note_site, resid_commit_needed, rows_val, site_key, softmax_rows};
use super::*;
use crate::float_ref::{ParamStore, SiteStat};
use crate::weights::{Binding, Pick, Src};
use serde_json::Value;

/// The image input's param name (lifted into an input by [`crate::encoder::vision_v2`]).
pub const IMAGE_PARAM: &str = "input.image";

/// What the tower's `post` produces.
#[derive(Clone, Debug, PartialEq)]
pub enum VisionOut {
    /// CLIP: the CLS row through `post_layernorm`, then `visual_projection` when `proj` is set.
    ClipPooled { proj: Option<usize> },
    /// SigLIP: `post_layernorm` over every row, then the attention-pooling head.
    SiglipHead,
    /// Qwen2-VL / Qwen2.5-VL: the merger to `out` columns.
    Merger { out: usize },
    /// LLaVA: the rows (CLS dropped) through the projector to `out` columns.
    Projector { out: usize, act: Act },
}

/// A vision tower as the lowering reads it.
#[derive(Clone, Debug)]
pub struct VisionSpec {
    pub architecture: String,
    /// The canonical input size (before any downscale) and the downscale ratio.
    pub h: u32,
    pub w: u32,
    pub downscale: u32,
    pub patch: u32,
    /// `Conv3d` temporal copies folded into the patch projection (Qwen2-VL: 2; else 1).
    pub temporal: u32,
    /// Spatial merge (Qwen2-VL: 2, block-major patch order; else 1).
    pub merge: u32,
    pub mean: [f64; 3],
    pub std: [f64; 3],
    pub d: usize,
    pub heads: usize,
    pub inter: usize,
    /// Layers in the checkpoint, and layers run (LLaVA stops at `vision_feature_layer`).
    pub layers: usize,
    pub run: usize,
    pub act: Act,
    pub swiglu: bool,
    pub rms: bool,
    pub eps: f64,
    pub pre_norm: bool,
    pub cls: bool,
    pub patch_bias: bool,
    pub learned_pos: bool,
    pub rope_theta: Option<f64>,
    /// Qwen2.5-VL: windows of `units × units` merge units; these layers attend over every row.
    pub window: Option<(u32, Vec<usize>)>,
    pub out: VisionOut,
    /// HL param name → checkpoint tensor template (`{L}` for the layer).
    pub names: BTreeMap<String, String>,
}

impl VisionSpec {
    pub fn grid(&self) -> (u32, u32) {
        (self.h / self.downscale / self.patch, self.w / self.downscale / self.patch)
    }
    pub fn patches(&self) -> usize {
        let (gh, gw) = self.grid();
        (gh * gw) as usize
    }
    /// Rows through the layers (patches, plus CLS).
    pub fn rows(&self) -> usize {
        self.patches() + usize::from(self.cls)
    }
    pub fn head_dim(&self) -> usize {
        self.d / self.heads
    }
    /// Pixels per patch (`p·p·3`).
    pub fn patch_len(&self) -> usize {
        (self.patch * self.patch * 3) as usize
    }
    /// Rows of the output.
    pub fn out_rows(&self) -> usize {
        match self.out {
            VisionOut::ClipPooled { .. } | VisionOut::SiglipHead => 1,
            VisionOut::Merger { .. } => self.patches() / (self.merge * self.merge) as usize,
            VisionOut::Projector { .. } => self.patches(),
        }
    }
    pub fn out_width(&self) -> usize {
        match self.out {
            VisionOut::ClipPooled { proj } => proj.unwrap_or(self.d),
            VisionOut::SiglipHead => self.d,
            VisionOut::Merger { out } | VisionOut::Projector { out, .. } => out,
        }
    }
}

const OPENAI_MEAN: [f64; 3] = [0.48145466, 0.4578275, 0.40821073];
const OPENAI_STD: [f64; 3] = [0.26862954, 0.26130258, 0.27577711];

fn cfg_usize(v: &Value, k: &str, default: usize) -> usize {
    v.get(k).and_then(Value::as_u64).map_or(default, |x| x as usize)
}
fn cfg_f64(v: &Value, k: &str, default: f64) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(default)
}
fn cfg_act(v: &Value, k: &str, default: &str) -> Result<Act> {
    let name = v.get(k).and_then(Value::as_str).unwrap_or(default);
    Act::from_hf(name).ok_or_else(|| LowerError::not_lowerable(format!("vision activation `{name}` is not modelled")))
}

/// Parse a vision tower's `config.json`. `size` is the class's declared input size when the model
/// has none (Qwen2-VL), and `mean_std` the processor's normalisation (`preprocessor_config.json`;
/// each family's default when `None`).
pub fn parse_vision(config: &str, size: Option<(u32, u32)>, mean_std: Option<([f64; 3], [f64; 3])>) -> Result<VisionSpec> {
    let root: Value = serde_json::from_str(config).map_err(|e| LowerError::bad(format!("config.json: {e}")))?;
    let arch = root["architectures"][0].as_str().unwrap_or("").to_string();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        names.insert(k.to_string(), v);
    };
    let spec = match arch.as_str() {
        "CLIPVisionModel" | "CLIPVisionModelWithProjection" | "LlavaForConditionalGeneration" => {
            let llava = arch == "LlavaForConditionalGeneration";
            let v = if llava { &root["vision_config"] } else { &root };
            let (p, pre) = if llava { ("vision_tower.", "vision_tower.") } else { ("vision_model.", "vision_model.") };
            let d = cfg_usize(v, "hidden_size", 768);
            let layers = cfg_usize(v, "num_hidden_layers", 12);
            let image = cfg_usize(v, "image_size", 224) as u32;
            let (run, out) = if llava {
                let fl = root.get("vision_feature_layer").and_then(Value::as_i64).unwrap_or(-2);
                if root.get("vision_feature_select_strategy").and_then(Value::as_str).unwrap_or("default") != "default" {
                    return Err(LowerError::not_lowerable("LLaVA vision_feature_select_strategy other than `default`"));
                }
                // `hidden_states[k]` is the output of layer k (index 0 is the embeddings).
                let run = if fl < 0 { (layers as i64 + 1 + fl) as usize } else { fl as usize };
                let tc = &root["text_config"];
                let act = cfg_act(&root, "projector_hidden_act", "gelu")?;
                (run, VisionOut::Projector { out: cfg_usize(tc, "hidden_size", 4096), act })
            } else if arch == "CLIPVisionModelWithProjection" {
                (layers, VisionOut::ClipPooled { proj: Some(cfg_usize(v, "projection_dim", 512)) })
            } else {
                (layers, VisionOut::ClipPooled { proj: None })
            };
            put("patch.w", format!("{p}embeddings.patch_embedding.weight"));
            put("cls", format!("{p}embeddings.class_embedding"));
            put("pos", format!("{p}embeddings.position_embedding.weight"));
            put("pre_norm", format!("{pre}pre_layrnorm"));
            put("post_norm", format!("{p}post_layernorm"));
            let l = format!("{p}encoder.layers.{{L}}.");
            put("norm1", format!("{l}layer_norm1"));
            put("norm2", format!("{l}layer_norm2"));
            for (k, h) in [("attn.q", "self_attn.q_proj"), ("attn.k", "self_attn.k_proj"), ("attn.v", "self_attn.v_proj"), ("attn.o", "self_attn.out_proj"), ("mlp.up", "mlp.fc1"), ("mlp.down", "mlp.fc2")] {
                put(k, format!("{l}{h}"));
            }
            if let VisionOut::ClipPooled { proj: Some(_) } = out {
                put("proj", "visual_projection".into());
            }
            if llava {
                put("proj.up", "multi_modal_projector.linear_1".into());
                put("proj.down", "multi_modal_projector.linear_2".into());
            }
            let (mean, std) = mean_std.unwrap_or((OPENAI_MEAN, OPENAI_STD));
            VisionSpec {
                architecture: arch.clone(),
                h: image,
                w: image,
                downscale: 1,
                patch: cfg_usize(v, "patch_size", 32) as u32,
                temporal: 1,
                merge: 1,
                mean,
                std,
                d,
                heads: cfg_usize(v, "num_attention_heads", 12),
                inter: cfg_usize(v, "intermediate_size", 3072),
                layers,
                run,
                act: cfg_act(v, "hidden_act", "quick_gelu")?,
                swiglu: false,
                rms: false,
                eps: cfg_f64(v, "layer_norm_eps", 1e-5),
                pre_norm: true,
                cls: true,
                patch_bias: false,
                learned_pos: true,
                rope_theta: None,
                window: None,
                out,
                names,
            }
        }
        "SiglipVisionModel" => {
            let v = &root;
            if !v.get("vision_use_head").and_then(Value::as_bool).unwrap_or(true) {
                return Err(LowerError::not_lowerable("SigLIP without its pooling head: take last_hidden_state (not modelled yet)"));
            }
            let d = cfg_usize(v, "hidden_size", 768);
            let image = cfg_usize(v, "image_size", 224) as u32;
            put("patch.w", "embeddings.patch_embedding.weight".into());
            put("patch.b", "embeddings.patch_embedding.bias".into());
            put("pos", "embeddings.position_embedding.weight".into());
            put("post_norm", "post_layernorm".into());
            let l = "encoder.layers.{L}.".to_string();
            put("norm1", format!("{l}layer_norm1"));
            put("norm2", format!("{l}layer_norm2"));
            for (k, h) in [("attn.q", "self_attn.q_proj"), ("attn.k", "self_attn.k_proj"), ("attn.v", "self_attn.v_proj"), ("attn.o", "self_attn.out_proj"), ("mlp.up", "mlp.fc1"), ("mlp.down", "mlp.fc2")] {
                put(k, format!("{l}{h}"));
            }
            put("head.probe", "head.probe".into());
            put("head.in_w", "head.attention.in_proj_weight".into());
            put("head.in_b", "head.attention.in_proj_bias".into());
            put("head.o", "head.attention.out_proj".into());
            put("head.norm", "head.layernorm".into());
            put("head.up", "head.mlp.fc1".into());
            put("head.down", "head.mlp.fc2".into());
            let (mean, std) = mean_std.unwrap_or(([0.5; 3], [0.5; 3]));
            VisionSpec {
                architecture: arch.clone(),
                h: image,
                w: image,
                downscale: 1,
                patch: cfg_usize(v, "patch_size", 16) as u32,
                temporal: 1,
                merge: 1,
                mean,
                std,
                d,
                heads: cfg_usize(v, "num_attention_heads", 12),
                inter: cfg_usize(v, "intermediate_size", 3072),
                layers: cfg_usize(v, "num_hidden_layers", 12),
                run: cfg_usize(v, "num_hidden_layers", 12),
                act: cfg_act(v, "hidden_act", "gelu_pytorch_tanh")?,
                swiglu: false,
                rms: false,
                eps: cfg_f64(v, "layer_norm_eps", 1e-6),
                pre_norm: false,
                cls: false,
                patch_bias: true,
                learned_pos: true,
                rope_theta: None,
                window: None,
                out: VisionOut::SiglipHead,
                names,
            }
        }
        "Qwen2VisionTransformerPretrainedModel"
        | "Qwen2_5_VisionTransformerPretrainedModel"
        | "Qwen2VLForConditionalGeneration"
        | "Qwen2_5_VLForConditionalGeneration" => {
            // A whole VLM keeps its tower under `visual.` (transformers 5 saves the original names;
            // `model.visual.` is aliased at binding).
            let whole = arch.ends_with("ForConditionalGeneration");
            let v = if whole { &root["vision_config"] } else { &root };
            let vp = if whole { "visual." } else { "" };
            let q25 = arch.starts_with("Qwen2_5");
            let (h, w) = size.ok_or_else(|| LowerError::bad("a Qwen2-VL tower needs the class's declared input size"))?;
            let d = if q25 { cfg_usize(v, "hidden_size", 1280) } else { cfg_usize(v, "embed_dim", 1280) };
            let heads = cfg_usize(v, "num_heads", 16);
            let depth = cfg_usize(v, "depth", 32);
            let inter = if q25 { cfg_usize(v, "intermediate_size", 3420) } else { d * cfg_usize(v, "mlp_ratio", 4) };
            let out = if q25 { cfg_usize(v, "out_hidden_size", 3584) } else { cfg_usize(v, "hidden_size", 3584) };
            let theta = v.get("rope_parameters").and_then(|r| r.get("rope_theta")).and_then(Value::as_f64).unwrap_or(10000.0);
            put("patch.w", format!("{vp}patch_embed.proj.weight"));
            let l = format!("{vp}blocks.{{L}}.");
            put("norm1", format!("{l}norm1"));
            put("norm2", format!("{l}norm2"));
            put("attn.qkv", format!("{l}attn.qkv"));
            put("attn.o", format!("{l}attn.proj"));
            if q25 {
                put("mlp.gate", format!("{l}mlp.gate_proj"));
                put("mlp.up", format!("{l}mlp.up_proj"));
                put("mlp.down", format!("{l}mlp.down_proj"));
            } else {
                put("mlp.up", format!("{l}mlp.fc1"));
                put("mlp.down", format!("{l}mlp.fc2"));
            }
            put("merger.norm", format!("{vp}merger.ln_q"));
            put("merger.up", format!("{vp}merger.mlp.0"));
            put("merger.down", format!("{vp}merger.mlp.2"));
            let merge = cfg_usize(v, "spatial_merge_size", 2) as u32;
            let patch = cfg_usize(v, "patch_size", 14) as u32;
            let window = if q25 {
                let ws = cfg_usize(v, "window_size", 112) as u32;
                let full: Vec<usize> = v
                    .get("fullatt_block_indexes")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as usize).collect())
                    .unwrap_or_default();
                Some((ws / merge / patch, full))
            } else {
                None
            };
            let (mean, std) = mean_std.unwrap_or((OPENAI_MEAN, OPENAI_STD));
            VisionSpec {
                architecture: arch.clone(),
                h,
                w,
                downscale: 1,
                patch,
                temporal: cfg_usize(v, "temporal_patch_size", 2) as u32,
                merge,
                mean,
                std,
                d,
                heads,
                inter,
                layers: depth,
                run: depth,
                act: cfg_act(v, "hidden_act", if q25 { "silu" } else { "quick_gelu" })?,
                swiglu: q25,
                rms: q25,
                eps: 1e-6,
                pre_norm: false,
                cls: false,
                patch_bias: false,
                learned_pos: false,
                rope_theta: Some(theta),
                window,
                out: VisionOut::Merger { out },
                names,
            }
        }
        other => return Err(LowerError::not_lowerable(format!("`{other}` is not a vision tower this lowering models"))),
    };
    let (gh, gw) = spec.grid();
    if spec.h % (spec.downscale * spec.patch) != 0 || spec.w % (spec.downscale * spec.patch) != 0 || gh % spec.merge != 0 || gw % spec.merge != 0 {
        return Err(LowerError::not_lowerable(format!("{}: a {}×{} input does not tile into {}-pixel patches (merge {})", spec.architecture, spec.h, spec.w, spec.patch, spec.merge)));
    }
    if spec.d % spec.heads != 0 {
        return Err(LowerError::bad(format!("{}: hidden {} not divisible by {} heads", spec.architecture, spec.d, spec.heads)));
    }
    Ok(spec)
}

// ───────────────────────────── params ─────────────────────────────

/// The tower's params: `(HL name, shape, per layer, source)`.
fn param_table(s: &VisionSpec) -> Result<Vec<(String, Vec<usize>, bool, Src)>> {
    let n = |k: &str| -> Result<String> { s.names.get(k).cloned().ok_or_else(|| LowerError::eval(format!("internal: no name for `{k}`"))) };
    let d = s.d;
    let raw = 3 * (s.temporal * s.patch * s.patch) as usize;
    let mut v: Vec<(String, Vec<usize>, bool, Src)> = Vec::new();
    let lin = |v: &mut Vec<(String, Vec<usize>, bool, Src)>, name: &str, role: &str, out: usize, inp: usize, bias: bool, pl: bool| -> Result<()> {
        let r = n(role)?;
        v.push((format!("{name}.w"), vec![out, inp], pl, Src::t(format!("{r}.weight"))));
        if bias {
            v.push((format!("{name}.b"), vec![out], pl, Src::t(format!("{r}.bias"))));
        }
        Ok(())
    };
    let norm = |v: &mut Vec<(String, Vec<usize>, bool, Src)>, name: &str, role: &str, width: usize, bias: bool, pl: bool| -> Result<()> {
        let r = n(role)?;
        v.push((format!("{name}.gain"), vec![width], pl, Src::t(format!("{r}.weight"))));
        if bias {
            v.push((format!("{name}.bias"), vec![width], pl, Src::t(format!("{r}.bias"))));
        }
        Ok(())
    };
    // pre
    v.push(("patch.w".into(), vec![d, raw], false, Src::t(n("patch.w")?).reshape(vec![d, raw])));
    if s.patch_bias {
        v.push(("patch.b".into(), vec![d], false, Src::t(n("patch.b")?)));
    }
    if s.cls {
        v.push(("cls".into(), vec![d], false, Src::t(n("cls")?)));
    }
    if s.learned_pos {
        v.push(("pos".into(), vec![s.rows(), d], false, Src::t(n("pos")?)));
    }
    if s.pre_norm {
        norm(&mut v, "pre_norm", "pre_norm", d, true, false)?;
    }
    // layers
    let bias_norm = !s.rms;
    norm(&mut v, "norm1", "norm1", d, bias_norm, true)?;
    if s.names.contains_key("attn.qkv") {
        let r = n("attn.qkv")?;
        for (i, k) in ["attn.q", "attn.k", "attn.v"].iter().enumerate() {
            let pick = Pick::Range { start: i * d, len: d };
            v.push((format!("{k}.w"), vec![d, d], true, Src::t(format!("{r}.weight")).rows(pick.clone())));
            v.push((format!("{k}.b"), vec![d], true, Src::t(format!("{r}.bias")).rows(pick)));
        }
    } else {
        for k in ["attn.q", "attn.k", "attn.v"] {
            lin(&mut v, k, k, d, d, true, true)?;
        }
    }
    lin(&mut v, "attn.o", "attn.o", d, d, true, true)?;
    norm(&mut v, "norm2", "norm2", d, bias_norm, true)?;
    if s.swiglu {
        lin(&mut v, "mlp.gate", "mlp.gate", s.inter, d, true, true)?;
    }
    lin(&mut v, "mlp.up", "mlp.up", s.inter, d, true, true)?;
    lin(&mut v, "mlp.down", "mlp.down", d, s.inter, true, true)?;
    // post
    match &s.out {
        VisionOut::ClipPooled { proj } => {
            norm(&mut v, "post_norm", "post_norm", d, true, false)?;
            if let Some(p) = proj {
                lin(&mut v, "proj", "proj", *p, d, false, false)?;
            }
        }
        VisionOut::SiglipHead => {
            norm(&mut v, "post_norm", "post_norm", d, true, false)?;
            v.push(("head.probe".into(), vec![d], false, Src::t(n("head.probe")?).reshape(vec![d])));
            let (w, b) = (n("head.in_w")?, n("head.in_b")?);
            for (i, k) in ["head.q", "head.k", "head.v"].iter().enumerate() {
                let pick = Pick::Range { start: i * d, len: d };
                v.push((format!("{k}.w"), vec![d, d], false, Src::t(w.clone()).rows(pick.clone())));
                v.push((format!("{k}.b"), vec![d], false, Src::t(b.clone()).rows(pick)));
            }
            lin(&mut v, "head.o", "head.o", d, d, true, false)?;
            norm(&mut v, "head.norm", "head.norm", d, true, false)?;
            lin(&mut v, "head.up", "head.up", s.inter, d, true, false)?;
            lin(&mut v, "head.down", "head.down", d, s.inter, true, false)?;
        }
        VisionOut::Merger { out } => {
            let m4 = d * (s.merge * s.merge) as usize;
            norm(&mut v, "merger.norm", "merger.norm", d, bias_norm, false)?;
            lin(&mut v, "merger.up", "merger.up", m4, m4, true, false)?;
            lin(&mut v, "merger.down", "merger.down", *out, m4, true, false)?;
        }
        VisionOut::Projector { out, .. } => {
            lin(&mut v, "proj.up", "proj.up", *out, d, true, false)?;
            lin(&mut v, "proj.down", "proj.down", *out, *out, true, false)?;
        }
    }
    Ok(v)
}

/// The synthetic HL program of a tower: its params, one anchor node per block referencing them
/// (so the materialisation and the binding see each param in its occurrences), and the binding.
pub fn hl_program(s: &VisionSpec) -> Result<(HlProgram, Binding)> {
    use crate::hl::{Block, BlockRole, CarryDecl, HlType, Init, Node, ParamDecl, Ref};
    let table = param_table(s)?;
    let params: Vec<ParamDecl> =
        table.iter().map(|(n, sh, pl, _)| ParamDecl { name: n.clone(), shape: sh.clone(), per_layer: *pl, init: Init::Normal(0.1) }).collect();
    let post_names: std::collections::BTreeSet<&str> = [
        "post_norm.gain", "post_norm.bias", "proj.w", "proj.up.w", "proj.up.b", "proj.down.w", "proj.down.b",
        "merger.norm.gain", "merger.norm.bias", "merger.up.w", "merger.up.b", "merger.down.w", "merger.down.b",
    ]
    .into_iter()
    .collect();
    let (mut pre, mut layer, mut post) = (Vec::new(), Vec::new(), Vec::new());
    for (i, (n, _, pl, _)) in table.iter().enumerate() {
        let r = Ref::Param(i as u32);
        if *pl {
            layer.push(r);
        } else if post_names.contains(n.as_str()) || n.starts_with("head.") {
            post.push(r);
        } else {
            pre.push(r);
        }
    }
    let anchor = |inputs: Vec<Ref>| Node {
        op: crate::hl::Op::Scale { c: 1.0 },
        inputs,
        outs: vec![vec![s.d]],
        out_types: vec![HlType::F32],
        site: None,
        writes: vec![],
    };
    let full_kinds: Vec<bool> = (0..s.run).map(|l| s.window.as_ref().is_none_or(|(_, full)| full.contains(&l))).collect();
    let mut blocks = vec![Block { name: "pre".into(), role: BlockRole::Pre, nodes: vec![anchor(pre)], outputs: vec![Ref::Node(0, 0)] }];
    // One block kind per attention kind (Qwen2.5-VL: windowed and full).
    let mut kind_of = BTreeMap::new();
    let mut schedule = Vec::new();
    for full in &full_kinds {
        let k = *kind_of.entry(*full).or_insert_with(|| {
            blocks.push(Block {
                name: if *full { "layer".into() } else { "layer.windowed".into() },
                role: BlockRole::Layer,
                nodes: vec![anchor(layer.clone())],
                outputs: vec![Ref::Node(0, 0)],
            });
            blocks.len() - 1
        });
        schedule.push(k as u16);
    }
    blocks.push(Block { name: "post".into(), role: BlockRole::Post, nodes: vec![anchor(post)], outputs: vec![Ref::Node(0, 0)] });
    let post_i = blocks.len() - 1;
    let hl = HlProgram {
        architecture: s.architecture.clone(),
        output: crate::hl::HlOutput::Embedding { normalized: false },
        vocab: 1,
        hidden: s.d,
        carries: vec![CarryDecl { name: "rows".into(), shape: vec![s.rows(), s.d] }],
        params,
        states: vec![],
        rope_tables: vec![],
        blocks,
        pre: 0,
        post: post_i,
        layer_of: (0..schedule.len()).collect(),
        schedule,
    };
    hl.validate().map_err(|e| LowerError::eval(format!("internal: the tower's HL program: {e}")))?;
    let mut ignored = Vec::new();
    // Layers past `run` (LLaVA's feature layer) and the unused post norm are unread by design.
    for l in s.run..s.layers {
        if let Some(t) = s.names.get("norm1") {
            ignored.push(t.split("{L}").next().unwrap_or("").to_string() + &format!("{l}."));
        }
    }
    if matches!(s.out, VisionOut::Projector { .. }) {
        if let Some(p) = s.names.get("post_norm") {
            ignored.push(format!("{p}."));
        }
        ignored.push("language_model.".into());
    }
    let mut aliases = vec![];
    if s.architecture.ends_with("VLForConditionalGeneration") {
        // A whole Qwen2-VL checkpoint: the language model is another stage's.
        ignored.extend(["model.layers.", "model.embed_tokens.", "model.norm.", "model.language_model.", "lm_head."].map(String::from));
        aliases.push(("visual.".to_string(), "model.visual.".to_string()));
    }
    let binding = Binding { srcs: table.into_iter().map(|(_, _, _, src)| src).collect(), aliases, ignored_prefixes: ignored };
    Ok((hl, binding))
}

// ───────────────────────────── pinned index tables ─────────────────────────────

/// `(h, w)` grid coordinates of each patch row, in the lowering's patch order.
pub fn patch_coords(s: &VisionSpec) -> Vec<(u32, u32)> {
    let (gh, gw) = s.grid();
    let m = s.merge;
    let mut v = Vec::with_capacity((gh * gw) as usize);
    if m == 1 {
        for i in 0..gh {
            for j in 0..gw {
                v.push((i, j));
            }
        }
    } else {
        for bh in 0..gh / m {
            for bw in 0..gw / m {
                for mh in 0..m {
                    for mw in 0..m {
                        v.push((bh * m + mh, bw * m + mw));
                    }
                }
            }
        }
    }
    v
}

/// Qwen2.5-VL's window order (`get_vision_window_index`): the merge units in window order, and
/// each unit's window. Returns `(patch permutation [N], window id per permuted row [N], unit
/// inverse permutation [N/m²])`.
pub fn window_tables(s: &VisionSpec) -> Option<(Vec<u32>, Vec<u32>, Vec<u32>)> {
    let (units, _) = s.window.as_ref()?;
    let (gh, gw) = s.grid();
    let m = s.merge;
    let (lh, lw) = (gh / m, gw / m);
    let ws = *units;
    let pad_h = ws - lh % ws;
    let pad_w = ws - lw % ws;
    let (nwh, nww) = ((lh + pad_h) / ws, (lw + pad_w) / ws);
    let mut order: Vec<u32> = Vec::new();
    let mut win_of_unit: Vec<u32> = Vec::new();
    let mut win = 0u32;
    for wh in 0..nwh {
        for ww in 0..nww {
            let mut any = false;
            for i in 0..ws {
                for j in 0..ws {
                    let (h, w) = (wh * ws + i, ww * ws + j);
                    if h < lh && w < lw {
                        order.push(h * lw + w);
                        win_of_unit.push(win);
                        any = true;
                    }
                }
            }
            if any {
                win += 1;
            }
        }
    }
    let mu = m * m;
    let perm: Vec<u32> = order.iter().flat_map(|u| (0..mu).map(move |k| u * mu + k)).collect();
    let wins: Vec<u32> = win_of_unit.iter().flat_map(|w| std::iter::repeat_n(*w, mu as usize)).collect();
    let mut inv = vec![0u32; order.len()];
    for (pos, u) in order.iter().enumerate() {
        inv[*u as usize] = pos as u32;
    }
    Some((perm, wins, inv))
}

/// The 2D rotary angles of each row (Qwen2-VL's axial rope): `[cos, sin]` per row and lane,
/// lanes `[h·f, w·f, h·f, w·f]` with `f` the `head_dim/4` frequencies.
pub fn rope_2d(s: &VisionSpec) -> Option<(Vec<f64>, Vec<f64>)> {
    let theta = s.rope_theta?;
    let dh = s.head_dim();
    let spatial = dh / 2;
    let inv: Vec<f64> = (0..spatial).step_by(2).map(|i| 1.0 / crate::detmath::powf(theta, i as f64 / spatial as f64)).collect();
    let mut coords = patch_coords(s);
    if let Some((perm, _, _)) = window_tables(s) {
        coords = perm.iter().map(|p| coords[*p as usize]).collect();
    }
    let (mut c, mut sn) = (Vec::with_capacity(coords.len() * dh), Vec::with_capacity(coords.len() * dh));
    for (h, w) in coords {
        let mut ang: Vec<f64> = inv.iter().map(|f| h as f64 * (*f as f32) as f64).collect();
        ang.extend(inv.iter().map(|f| w as f64 * (*f as f32) as f64));
        let half = ang.clone();
        ang.extend(half);
        c.extend(ang.iter().map(|a| crate::detmath::cos_f32(*a as f32) as f64));
        sn.extend(ang.iter().map(|a| crate::detmath::sin_f32(*a as f32) as f64));
    }
    Some((c, sn))
}

// ───────────────────────────── the float reference ─────────────────────────────

/// The float tower over one canonical image (`u8` HWC): the output rows, and, when `stats` is
/// given, every site's statistics under the lowering's names.
pub fn float_forward(
    hl: &HlProgram,
    s: &VisionSpec,
    params: &ParamStore,
    image: &[u8],
    mut stats: Option<&mut BTreeMap<String, SiteStat>>,
) -> Result<Vec<Vec<f64>>> {
    let p = |name: &str, layer: Option<usize>| -> Result<Vec<f64>> {
        let i = hl_param(hl, name)?;
        Ok(params.get(i, layer)?.data.iter().map(|x| *x as f64).collect())
    };
    let mut observe = |key: String, rows: &[Vec<f64>]| {
        if let Some(st) = stats.as_deref_mut() {
            let e = st.entry(key).or_default();
            let width = rows.first().map_or(0, |r| r.len());
            if e.count == 0 {
                e.chan_absmax = vec![0.0; width];
            }
            for (i, r) in rows.iter().enumerate() {
                let mut row = 0f64;
                for (c, x) in r.iter().enumerate() {
                    let ax = x.abs();
                    row = row.max(ax);
                    e.sum_sq += ax * ax;
                    if let Some(ch) = e.chan_absmax.get_mut(c) {
                        *ch = ch.max(ax as f32);
                    }
                }
                e.absmax = e.absmax.max(row);
                if i == 0 { e.pos0_absmax = e.pos0_absmax.max(row) } else { e.rest_absmax = e.rest_absmax.max(row) }
                e.count += r.len() as u64;
            }
        }
    };
    let (h, w) = (s.h as usize, s.w as usize);
    if image.len() != h * w * 3 {
        return Err(LowerError::eval(format!("an image of {} bytes for {h}×{w}×3", image.len())));
    }
    // Downscale (box, exact integer rounding as the program does).
    let r = s.downscale as usize;
    let (hh, ww) = (h / r, w / r);
    let px: Vec<f64> = if r == 1 {
        image.iter().map(|v| *v as f64).collect()
    } else {
        let mut out = vec![0f64; hh * ww * 3];
        for i in 0..hh {
            for j in 0..ww {
                for c in 0..3 {
                    let mut sum = 0i64;
                    for a in 0..r {
                        for b in 0..r {
                            sum += image[((i * r + a) * w + (j * r + b)) * 3 + c] as i64;
                        }
                    }
                    let q = (r * r) as i64;
                    out[(i * ww + j) * 3 + c] = ((2 * sum + q) / (2 * q)) as f64;
                }
            }
        }
        out
    };
    // Patches: normalised pixels through the conv (sum over the temporal copies).
    let (d, ps, t) = (s.d, s.patch as usize, s.temporal as usize);
    let wconv = p("patch.w", None)?;
    let raw = 3 * t * ps * ps;
    let bconv = if s.patch_bias { Some(p("patch.b", None)?) } else { None };
    let coords = patch_coords(s);
    let mut x: Vec<Vec<f64>> = coords
        .iter()
        .map(|(gi, gj)| {
            (0..d)
                .map(|o| {
                    let mut acc = bconv.as_ref().map_or(0.0, |b| b[o]);
                    for c in 0..3 {
                        for tt in 0..t {
                            for i in 0..ps {
                                for j in 0..ps {
                                    let pix = px[((*gi as usize * ps + i) * ww + (*gj as usize * ps + j)) * 3 + c];
                                    let xn = (pix / 255.0 - s.mean[c]) / s.std[c];
                                    acc += wconv[o * raw + ((c * t + tt) * ps + i) * ps + j] * xn;
                                }
                            }
                        }
                    }
                    acc
                })
                .collect()
        })
        .collect();
    observe("pre.patch".into(), &x);
    let wt = window_tables(s);
    if let Some((perm, _, _)) = &wt {
        x = perm.iter().map(|q| x[*q as usize].clone()).collect();
    }
    if s.cls {
        x.insert(0, p("cls", None)?);
    }
    if s.learned_pos {
        let pos = p("pos", None)?;
        for (i, row) in x.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v += pos[i * d + j];
            }
        }
    }
    observe("pre.embed".into(), &x);
    let ln = |x: &[f64], g: &[f64], b: Option<&[f64]>, eps: f64, rms: bool| -> Vec<f64> {
        let n = x.len() as f64;
        let mu = if rms { 0.0 } else { x.iter().sum::<f64>() / n };
        let var = x.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / n;
        let inv = 1.0 / (var + eps).sqrt();
        x.iter().enumerate().map(|(i, v)| (v - mu) * inv * g[i] + b.map_or(0.0, |b| b[i])).collect()
    };
    let lin = |x: &[f64], w: &[f64], bias: Option<&[f64]>, out: usize| -> Vec<f64> {
        let inp = x.len();
        (0..out).map(|o| bias.map_or(0.0, |b| b[o]) + (0..inp).map(|i| w[o * inp + i] * x[i]).sum::<f64>()).collect()
    };
    if s.pre_norm {
        let (g, b) = (p("pre_norm.gain", None)?, p("pre_norm.bias", None)?);
        x = x.iter().map(|r| ln(r, &g, Some(&b), s.eps, false)).collect();
        observe("pre.pre_norm".into(), &x);
    }
    let (heads, dh, l_rows) = (s.heads, s.head_dim(), s.rows());
    let rope = rope_2d(s);
    let scale = 1.0 / (dh as f64).sqrt();
    for li in 0..s.run {
        let pre = format!("L{li}.");
        let ly = Some(li);
        observe(format!("{pre}carry0"), &x);
        let g1 = p("norm1.gain", ly)?;
        let b1 = if s.rms { None } else { Some(p("norm1.bias", ly)?) };
        let n1: Vec<Vec<f64>> = x.iter().map(|r| ln(r, &g1, b1.as_deref(), s.eps, s.rms)).collect();
        observe(format!("{pre}norm1"), &n1);
        let proj = |name: &str, x: &[Vec<f64>], out: usize, bias: bool| -> Result<Vec<Vec<f64>>> {
            let w = p(&format!("{name}.w"), ly)?;
            let b = if bias { Some(p(&format!("{name}.b"), ly)?) } else { None };
            Ok(x.iter().map(|r| lin(r, &w, b.as_deref(), out)).collect())
        };
        let (mut q, mut k, v) = (proj("attn.q", &n1, d, true)?, proj("attn.k", &n1, d, true)?, proj("attn.v", &n1, d, true)?);
        observe(format!("{pre}attn.q"), &q);
        observe(format!("{pre}attn.k"), &k);
        observe(format!("{pre}attn.v"), &v);
        if let Some((c, sn)) = &rope {
            let rot = |x: &mut Vec<Vec<f64>>| {
                for (i, row) in x.iter_mut().enumerate() {
                    for hh in 0..heads {
                        let seg = row[hh * dh..(hh + 1) * dh].to_vec();
                        for t in 0..dh {
                            let rh = if t < dh / 2 { -seg[t + dh / 2] } else { seg[t - dh / 2] };
                            row[hh * dh + t] = seg[t] * c[i * dh + t] + rh * sn[i * dh + t];
                        }
                    }
                }
            };
            rot(&mut q);
            rot(&mut k);
            observe(format!("{pre}attn.q"), &q);
            observe(format!("{pre}attn.k"), &k);
        }
        let windowed = s.window.as_ref().is_some_and(|(_, full)| !full.contains(&li));
        let wins = wt.as_ref().map(|(_, w, _)| w.clone());
        let mut ctx = vec![vec![0f64; d]; l_rows];
        for hh in 0..heads {
            for i in 0..l_rows {
                let keys: Vec<usize> = (0..l_rows).filter(|j| !windowed || wins.as_ref().is_none_or(|w| w[*j] == w[i])).collect();
                let sc: Vec<f64> = keys.iter().map(|j| (0..dh).map(|t| q[i][hh * dh + t] * k[*j][hh * dh + t]).sum::<f64>() * scale).collect();
                let mx = sc.iter().cloned().fold(f64::MIN, f64::max);
                let e: Vec<f64> = sc.iter().map(|z| (z - mx).exp()).collect();
                let z: f64 = e.iter().sum();
                for t in 0..dh {
                    ctx[i][hh * dh + t] = keys.iter().enumerate().map(|(a, j)| e[a] / z * v[*j][hh * dh + t]).sum();
                }
            }
        }
        observe(format!("{pre}attn.ctx"), &ctx);
        let o = proj("attn.o", &ctx, d, true)?;
        observe(format!("{pre}attn.o"), &o);
        let r1: Vec<Vec<f64>> = x.iter().zip(&o).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a + b).collect()).collect();
        observe(format!("{pre}resid.mix"), &r1);
        let g2 = p("norm2.gain", ly)?;
        let b2 = if s.rms { None } else { Some(p("norm2.bias", ly)?) };
        let n2: Vec<Vec<f64>> = r1.iter().map(|r| ln(r, &g2, b2.as_deref(), s.eps, s.rms)).collect();
        observe(format!("{pre}norm2"), &n2);
        let act = |z: f64| crate::float_ref::act(s.act, z as f32) as f64;
        let hidden: Vec<Vec<f64>> = if s.swiglu {
            let gate = proj("mlp.gate", &n2, s.inter, true)?;
            observe(format!("{pre}mlp.gate"), &gate);
            let up = proj("mlp.up", &n2, s.inter, true)?;
            observe(format!("{pre}mlp.up"), &up);
            let ga: Vec<Vec<f64>> = gate.iter().map(|r| r.iter().map(|z| act(*z)).collect()).collect();
            observe(format!("{pre}mlp.gate_act"), &ga);
            let prod: Vec<Vec<f64>> = ga.iter().zip(&up).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a * b).collect()).collect();
            observe(format!("{pre}mlp.prod"), &prod);
            prod
        } else {
            let up = proj("mlp.up", &n2, s.inter, true)?;
            observe(format!("{pre}mlp.up"), &up);
            let a: Vec<Vec<f64>> = up.iter().map(|r| r.iter().map(|z| act(*z)).collect()).collect();
            observe(format!("{pre}mlp.act"), &a);
            a
        };
        let down = proj("mlp.down", &hidden, d, true)?;
        observe(format!("{pre}mlp.down"), &down);
        x = r1.iter().zip(&down).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a + b).collect()).collect();
        observe(format!("{pre}resid.ffn"), &x);
    }
    observe("post.carry0".into(), &x);
    let gelu = |a: Act, z: f64| crate::float_ref::act(a, z as f32) as f64;
    let out: Vec<Vec<f64>> = match &s.out {
        VisionOut::ClipPooled { proj } => {
            let (g, b) = (p("post_norm.gain", None)?, p("post_norm.bias", None)?);
            let pooled = ln(&x[0], &g, Some(&b), s.eps, false);
            observe("post.post_norm".into(), std::slice::from_ref(&pooled));
            match proj {
                Some(pw) => vec![lin(&pooled, &p("proj.w", None)?, None, *pw)],
                None => vec![pooled],
            }
        }
        VisionOut::SiglipHead => {
            let (g, b) = (p("post_norm.gain", None)?, p("post_norm.bias", None)?);
            let xn: Vec<Vec<f64>> = x.iter().map(|r| ln(r, &g, Some(&b), s.eps, false)).collect();
            observe("post.post_norm".into(), &xn);
            let probe = p("head.probe", None)?;
            let q = lin(&probe, &p("head.q.w", None)?, Some(&p("head.q.b", None)?), d);
            observe("post.head.q".into(), std::slice::from_ref(&q));
            let (kw, kb, vw, vb) = (p("head.k.w", None)?, p("head.k.b", None)?, p("head.v.w", None)?, p("head.v.b", None)?);
            let k: Vec<Vec<f64>> = xn.iter().map(|r| lin(r, &kw, Some(&kb), d)).collect();
            let v: Vec<Vec<f64>> = xn.iter().map(|r| lin(r, &vw, Some(&vb), d)).collect();
            observe("post.head.k".into(), &k);
            observe("post.head.v".into(), &v);
            let mut ctx = vec![0f64; d];
            for hh in 0..heads {
                let sc: Vec<f64> = (0..l_rows).map(|j| (0..dh).map(|t| q[hh * dh + t] * k[j][hh * dh + t]).sum::<f64>() * scale).collect();
                let mx = sc.iter().cloned().fold(f64::MIN, f64::max);
                let e: Vec<f64> = sc.iter().map(|z| (z - mx).exp()).collect();
                let z: f64 = e.iter().sum();
                for t in 0..dh {
                    ctx[hh * dh + t] = (0..l_rows).map(|j| e[j] / z * v[j][hh * dh + t]).sum();
                }
            }
            observe("post.head.ctx".into(), std::slice::from_ref(&ctx));
            let o = lin(&ctx, &p("head.o.w", None)?, Some(&p("head.o.b", None)?), d);
            observe("post.head.o".into(), std::slice::from_ref(&o));
            let hn = ln(&o, &p("head.norm.gain", None)?, Some(&p("head.norm.bias", None)?), s.eps, false);
            observe("post.head.norm".into(), std::slice::from_ref(&hn));
            let up = lin(&hn, &p("head.up.w", None)?, Some(&p("head.up.b", None)?), s.inter);
            observe("post.head.up".into(), std::slice::from_ref(&up));
            let a: Vec<f64> = up.iter().map(|z| gelu(s.act, *z)).collect();
            observe("post.head.act".into(), std::slice::from_ref(&a));
            let down = lin(&a, &p("head.down.w", None)?, Some(&p("head.down.b", None)?), d);
            observe("post.head.down".into(), std::slice::from_ref(&down));
            vec![o.iter().zip(&down).map(|(a, b)| a + b).collect()]
        }
        VisionOut::Merger { out } => {
            let g = p("merger.norm.gain", None)?;
            let b = if s.rms { None } else { Some(p("merger.norm.bias", None)?) };
            let xn: Vec<Vec<f64>> = x.iter().map(|r| ln(r, &g, b.as_deref(), 1e-6, s.rms)).collect();
            observe("post.merger.norm".into(), &xn);
            let mu = (s.merge * s.merge) as usize;
            let grouped: Vec<Vec<f64>> = xn.chunks(mu).map(|c| c.concat()).collect();
            let m4 = d * mu;
            let up: Vec<Vec<f64>> = grouped.iter().map(|r| lin(r, &p("merger.up.w", None).unwrap(), Some(&p("merger.up.b", None).unwrap()), m4)).collect();
            observe("post.merger.up".into(), &up);
            let a: Vec<Vec<f64>> = up.iter().map(|r| r.iter().map(|z| gelu(Act::Gelu, *z)).collect()).collect();
            observe("post.merger.act".into(), &a);
            let mut down: Vec<Vec<f64>> =
                a.iter().map(|r| lin(r, &p("merger.down.w", None).unwrap(), Some(&p("merger.down.b", None).unwrap()), *out)).collect();
            if let Some((_, _, inv)) = &wt {
                down = inv.iter().map(|u| down[*u as usize].clone()).collect();
            }
            down
        }
        VisionOut::Projector { out, act } => {
            let rows = &x[usize::from(s.cls)..];
            observe("post.proj.in".into(), rows);
            let up: Vec<Vec<f64>> = rows.iter().map(|r| lin(r, &p("proj.up.w", None).unwrap(), Some(&p("proj.up.b", None).unwrap()), *out)).collect();
            observe("post.proj.up".into(), &up);
            let a: Vec<Vec<f64>> = up.iter().map(|r| r.iter().map(|z| gelu(*act, *z)).collect()).collect();
            observe("post.proj.act".into(), &a);
            a.iter().map(|r| lin(r, &p("proj.down.w", None).unwrap(), Some(&p("proj.down.b", None).unwrap()), *out)).collect()
        }
    };
    observe("post.out".into(), &out);
    Ok(out)
}

// ───────────────────────────── the lowering ─────────────────────────────

/// Lower a tower: `pre` (preprocessing, patch projection, CLS, positions, window order, pre-norm),
/// one block per layer kind, `post` (the output). The program's "logits" node is the output rows
/// `[R, width]` in a power-of-two fixed point.
pub fn lower_vision(hl: &HlProgram, s: &VisionSpec) -> Result<Lowered> {
    let hb = tir::program::HISTORY_BOUND_V1_SMALL;
    let mut pb = ProgramBuilder::new(1, hb);
    let mut cx = Cx {
        hl,
        fills: Vec::new(),
        row_params: BTreeMap::new(),
        resid_sites: BTreeMap::new(),
        tstate: BTreeMap::new(),
        history_bound: hb,
        max_window: hb,
        logits_key: None,
        site_nodes: BTreeMap::new(),
        shared: Default::default(),
        tables: Default::default(),
        image_rows: None,
        image_cursor: None,
        image_cursor_layer: None,
        split_max_readers: 0,
        quant: BTreeMap::new(),
        table_shift: 0,
        carry_keys: BTreeMap::new(),
    };
    let mut block_map = vec![u8::MAX; hl.blocks.len()];
    let mut order: Vec<usize> = vec![hl.pre];
    for k in &hl.schedule {
        if !order.contains(&(*k as usize)) {
            order.push(*k as usize);
        }
    }
    order.push(hl.post);
    let mut out_node = None;
    for &hbk in &order {
        let (tb, o) = vision_block(&mut pb, &mut cx, hbk, s)?;
        block_map[hbk] = tb;
        if o.is_some() {
            out_node = o;
        }
    }
    let out_node = out_node.ok_or_else(|| LowerError::eval("internal: no output node"))?;
    let layers: Vec<u8> = hl.schedule.iter().map(|k| block_map[*k as usize]).collect();
    let program = pb.finish(block_map[hl.pre], layers, block_map[hl.post], out_node);
    tir::validate::validate(&program).map_err(|e| LowerError::eval(format!("internal: the tower's program is not in normal form: {e}")))?;
    let resid_sites = cx.resid_sites.into_iter().map(|((k, _), f)| (k, f)).collect();
    let logits_key = cx.logits_key.ok_or_else(|| LowerError::eval("internal: no output scale"))?;
    Ok(Lowered { program, fills: cx.fills, row_params: cx.row_params, resid_sites, logits_key, block_map, site_nodes: cx.site_nodes, budget_fallbacks: vec![] })
}

fn new_lb(hl: &HlProgram, hbk: usize) -> Lb {
    let blk = &hl.blocks[hbk];
    let n = blk.nodes.len();
    let prefixes: Vec<String> = match blk.role {
        BlockRole::Pre => vec!["pre.".into()],
        BlockRole::Post => vec!["post.".into()],
        BlockRole::Layer => hl.schedule.iter().enumerate().filter(|(_, k)| **k as usize == hbk).map(|(l, _)| format!("L{l}.")).collect(),
    };
    let suffix = if blk.role == BlockRole::Layer && hl.schedule.first().map(|k| *k as usize) != Some(hbk) { format!("#{hbk}") } else { String::new() };
    Lb {
        hb: hbk,
        role: blk.role,
        prefixes,
        vals: Vec::new(),
        wants: vec![None; n],
        rope_sites: vec![Vec::new(); n],
        split: vec![0; n],
        windows: BTreeMap::new(),
        angles: BTreeMap::new(),
        requants: 0,
        absorbed: vec![false; n],
        gdn: BTreeMap::new(),
        ssm: BTreeMap::new(),
        wide: vec![false; n],
        w16: vec![false; n],
        w16_now: false,
        mrope_pos: None,
        suffix,
        appended: BTreeMap::new(),
        carry_in: Vec::new(),
    }
}

fn codes_want(site: &str) -> Want {
    Want { dt: DType::I16, key: site_key(site) }
}

fn resid_want() -> Want {
    Want { dt: DType::I32, key: ScaleKey::resid() }
}

fn out_key() -> ScaleKey {
    ScaleKey { base: Base::Pow2Site { names: vec!["out".into()] }, factor: 1.0 }
}

/// A pinned `idx` table as a global param, clamped to `[0, bound)` so the range analysis sees the
/// index range its reader needs (the clamp never fires: the table is data the lowering wrote).
fn idx_param(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &Lb, name: &str, v: Vec<u32>, bound: usize) -> Result<tir::Ref> {
    let n = v.len();
    let p = decl(b, cx, lb, name, DType::Idx, &[n], false, Arc::new(move |_| Ok(IntTensor::idx(vec![n], v.clone()))))?;
    Ok(b.clamp(p, 0, bound.max(1) as i64 - 1, DType::Idx))
}

#[allow(clippy::too_many_lines)]
fn vision_block(pb: &mut ProgramBuilder, cx: &mut Cx<'_>, hbk: usize, s: &VisionSpec) -> Result<(u8, Option<u16>)> {
    let hl = cx.hl;
    let role = hl.blocks[hbk].role;
    let (d, l) = (s.d, s.rows());
    let carry_sig = vec![TensorType::fixed(DType::I32, &[l as u32, d as u32])];
    let mut lb = new_lb(hl, hbk);
    let tb = pb.blocks.len() as u8;
    let mut b = pb.block(&hl.blocks[hbk].name, if role == BlockRole::Pre { vec![] } else { carry_sig });
    let resid = ScaleKey::resid();
    match role {
        BlockRole::Pre => {
            let (h, w) = (s.h, s.w);
            let img = decl(&mut b, cx, &lb, IMAGE_PARAM, DType::I16, &[h as usize, w as usize, 3], false, input_fill(DType::I16, vec![h as usize, w as usize, 3]))?;
            // Fixed-ratio downscale: a box filter of ratio r (two MatMuls with pinned 0/1
            // matrices, then an exact rounded Div by r²).
            let r = s.downscale;
            let (hh, ww) = (h / r, w / r);
            let img = if r > 1 {
                let rows: Vec<i8> = (0..hh).flat_map(|i| (0..h).map(move |k| i8::from(k / r == i))).collect();
                let cols: Vec<i8> = (0..w).flat_map(|k| (0..ww).map(move |j| i8::from(k / r == j))).collect();
                let bh = decl(&mut b, cx, &lb, "pre.box_rows", DType::I8, &[hh as usize, h as usize], false, Arc::new(move |_| Ok(IntTensor::i8(vec![hh as usize, h as usize], rows.clone()))))?;
                let bw = decl(&mut b, cx, &lb, "pre.box_cols", DType::I8, &[w as usize, ww as usize], false, Arc::new(move |_| Ok(IntTensor::i8(vec![w as usize, ww as usize], cols.clone()))))?;
                // The 0/1 matrices are data: their range is stated for the analysis (never fires).
                let bh = b.clamp(bh, 0, 1, DType::I8);
                let bw = b.clamp(bw, 0, 1, DType::I8);
                let x = b.reshape_fixed(img, &[h, w * 3]);
                let x = b.matmul(bh, x, DType::I32); // [hh, w·3]
                let x = b.reshape_fixed(x, &[hh, w, 3]);
                let x = b.transpose(x, &[0, 2, 1]); // [hh, 3, w]
                let x = b.matmul(x, bw, DType::I32); // [hh, 3, ww]
                let x = b.transpose(x, &[0, 2, 1]); // [hh, ww, 3]
                let q = b.c(DType::I32, (r * r) as i128);
                let x = b.div(x, q, tir::Rounding::HalfAwayFromZero, DType::I32);
                b.clamp(x, 0, 255, DType::I16)
            } else {
                img
            };
            // Patchify: [H, W, 3] → [gh, p, gw, p·3] → [gh, gw, p, p·3] (within-patch (i, j, c)),
            // then row-major patches, or Qwen2-VL's block-major order.
            let (gh, gw) = s.grid();
            let p = s.patch;
            let pl = s.patch_len() as u32;
            let x = b.reshape_fixed(img, &[gh, p, gw, p * 3]);
            let x = b.transpose(x, &[0, 2, 1, 3]);
            let x = if s.merge > 1 {
                let m = s.merge;
                let x = b.reshape_fixed(x, &[gh / m, m, gw / m, m * pl]);
                b.transpose(x, &[0, 2, 1, 3])
            } else {
                x
            };
            let n = s.patches();
            let x = b.reshape_fixed(x, &[n as u32, pl]);
            // The patch projection with the normalisation folded in: i16 per-row weights over the
            // exact pixels (scale 1), stored [P, d] for `x · Wᵀ`, the folded bias as `z`.
            let wp = hl_param(hl, "patch.w")?;
            let bp = hl_param(hl, "patch.b").ok();
            let (mean, std, t, ps) = (s.mean, s.std, s.temporal as usize, s.patch as usize);
            let raw = 3 * t * ps * ps;
            let fold = move |c: &FillCtx<'_>| -> Result<(Vec<f32>, Vec<f64>)> {
                let wv = &c.f(wp)?.data;
                let mut wf = vec![0f32; d * ps * ps * 3];
                let mut bf = vec![0f64; d];
                for o in 0..d {
                    let mut bias = match bp {
                        Some(bp) => c.f(bp)?.data[o] as f64,
                        None => 0.0,
                    };
                    for ch in 0..3 {
                        for tt in 0..t {
                            for i in 0..ps {
                                for j in 0..ps {
                                    let wv = wv[o * raw + ((ch * t + tt) * ps + i) * ps + j] as f64;
                                    wf[o * ps * ps * 3 + (i * ps + j) * 3 + ch] += (wv / (255.0 * std[ch])) as f32;
                                    bias -= wv * mean[ch] / std[ch];
                                }
                            }
                        }
                    }
                    bf[o] = bias;
                }
                Ok((wf, bf))
            };
            let fold = Arc::new(fold);
            let (f1, f2, f3) = (fold.clone(), fold.clone(), fold);
            let pl_us = pl as usize;
            let wt = decl(
                &mut b,
                cx,
                &lb,
                "patch.wt",
                DType::I16,
                &[pl_us, d],
                false,
                Arc::new(move |c| {
                    let (wf, _) = f1(c)?;
                    let q = crate::quant::quantize_rows16(&wf, d, pl_us);
                    let mut t = vec![0i16; pl_us * d];
                    for o in 0..d {
                        for i in 0..pl_us {
                            t[i * d + o] = q.codes[o * pl_us + i];
                        }
                    }
                    Ok(IntTensor::i16(vec![pl_us, d], t))
                }),
            )?;
            let (m, sh) = decl_ms(
                &mut b,
                cx,
                &lb,
                "patch",
                d,
                Arc::new(move |c| {
                    let (wf, _) = f2(c)?;
                    let q = crate::quant::quantize_rows16(&wf, d, pl_us);
                    let sr = c.scale(&ScaleKey::resid())?;
                    Ok(q.scales.iter().map(|sw| sw / sr).collect())
                }),
            )?;
            let z = decl(
                &mut b,
                cx,
                &lb,
                "patch.z",
                DType::I64,
                &[d],
                false,
                Arc::new(move |c| {
                    let (_, bf) = f3(c)?;
                    let sr = c.scale(&ScaleKey::resid())?;
                    Ok(IntTensor::i64(vec![d], bf.iter().map(|v| (v / sr).round() as i64).collect()))
                }),
            )?;
            let acc = b.matmul(x, wt, DType::I64);
            let e = narrow(&mut b, acc, m, sh, Some(z), DType::I32);
            let mut e = rows_val(e, DType::I32, resid.clone(), d, "patch");
            note_resid(cx, &lb, &e);
            // Window order (Qwen2.5-VL): a pinned row permutation.
            if let Some((perm, _, _)) = window_tables(s) {
                let np = perm.len();
                let pi = idx_param(&mut b, cx, &lb, "pre.window_order", perm, np)?;
                let r = b.gather(e.r, pi, 0, 0);
                e = Val { r, ..e };
            }
            // CLS (a param row at the residual scale) and the learned positions.
            if s.cls {
                let cp = hl_param(hl, "cls")?;
                let cls = decl(
                    &mut b,
                    cx,
                    &lb,
                    "cls.row",
                    DType::I32,
                    &[1, d],
                    false,
                    Arc::new(move |c| {
                        let sr = c.scale(&ScaleKey::resid())?;
                        Ok(IntTensor::i32(vec![1, d], c.f(cp)?.data.iter().map(|v| (*v as f64 / sr).round() as i32).collect()))
                    }),
                )?;
                let r = b.concat(&[cls, e.r], 0);
                e = Val { r, ..e };
            }
            if s.learned_pos {
                let pp = hl_param(hl, "pos")?;
                let pos = decl(
                    &mut b,
                    cx,
                    &lb,
                    "pos.rows",
                    DType::I32,
                    &[l, d],
                    false,
                    Arc::new(move |c| {
                        let sr = c.scale(&ScaleKey::resid())?;
                        Ok(IntTensor::i32(vec![l, d], c.f(pp)?.data.iter().map(|v| (*v as f64 / sr).round() as i32).collect()))
                    }),
                )?;
                let sum = b.add(e.r, pos, DType::I64);
                let r = b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
                e = rows_val(r, DType::I32, resid.clone(), d, "embed");
                note_resid(cx, &lb, &e);
            }
            if s.pre_norm {
                // The norm reduces whole rows of the patch projection's output (see bidir's
                // `resid_commit_needed`).
                if resid_commit_needed(l as u32, d, pl_us) {
                    b.commit(e.r);
                }
                e = norm_rows_kind(&mut b, cx, &mut lb, &e, NormKind::Layer, s.eps, "pre_norm", true, &resid_want())?;
                note_resid(cx, &lb, &e);
            }
            let e = ensure_node(&mut b, &e);
            b.commit(e.r);
            note_site(cx, tb, &e);
            Ok((b.finish(&[e.r]), None))
        }
        BlockRole::Layer => {
            let li = lb.prefixes.first().and_then(|p| p.trim_start_matches('L').trim_end_matches('.').parse::<usize>().ok()).unwrap_or(0);
            let windowed = s.window.as_ref().is_some_and(|(_, full)| !full.contains(&li));
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let kind = if s.rms { NormKind::Rms } else { NormKind::Layer };
            let n1 = norm_rows_kind(&mut b, cx, &mut lb, &x, kind, s.eps, "norm1", !s.rms, &codes_want("norm1"))?;
            let (h, dh) = (s.heads as u32, s.head_dim() as u32);
            let rope = rope_2d(s);
            let mut qkv = Vec::new();
            for name in ["attn.q", "attn.k", "attn.v"] {
                let v = linear_rows(&mut b, cx, &mut lb, &n1, &format!("{name}.w"), Some(&format!("{name}.b")), name, &codes_want(name))?;
                let r = b.reshape_fixed(v.r, &[l as u32, h, dh]);
                let r = b.transpose(r, &[1, 0, 2]);
                qkv.push(Val { r, ..v });
            }
            // 2D rotary on q and k: x·cos + rotate_half(x)·sin, cos/sin Q24 params [1, L, dh].
            if let Some((cs, sn)) = &rope {
                let ln = l;
                let dhu = dh as usize;
                let q24 = |v: &Vec<f64>| -> Vec<i32> { v.iter().map(|x| (x * (1u64 << 24) as f64).round() as i32).collect() };
                let (cv, sv) = (q24(cs), q24(sn));
                let cos = decl(&mut b, cx, &lb, "rope.cos", DType::I32, &[1, ln, dhu], false, Arc::new(move |_| Ok(IntTensor::i32(vec![1, ln, dhu], cv.clone()))))?;
                let sin = decl(&mut b, cx, &lb, "rope.sin", DType::I32, &[1, ln, dhu], false, Arc::new(move |_| Ok(IntTensor::i32(vec![1, ln, dhu], sv.clone()))))?;
                for v in qkv.iter_mut().take(2) {
                    let x1 = b.slice(v.r, 2, 0, dh / 2);
                    let x2 = b.slice(v.r, 2, dh / 2, dh / 2);
                    let zero = b.c(DType::I16, 0);
                    let nx2 = b.sub(zero, x2, DType::I16);
                    let rh = b.concat(&[nx2, x1], 2);
                    let a = b.mul(v.r, cos, DType::I64);
                    let c2 = b.mul(rh, sin, DType::I64);
                    let sum = b.add(a, c2, DType::I64);
                    let rot = b.shr(sum, 24, tir::Rounding::HalfAwayFromZero, DType::I64);
                    let rot = b.clamp(rot, -32767, 32767, DType::I16);
                    b.commit(rot);
                    v.r = rot;
                }
            }
            let (q, k, v) = (qkv[0].clone(), qkv[1].clone(), qkv[2].clone());
            let kt = b.transpose(k.r, &[0, 2, 1]);
            let sc = b.matmul(q.r, kt, DType::I64);
            let (kq, kk, scale) = (q.key.clone(), k.key.clone(), 1.0 / (dh as f64).sqrt());
            let (m, sh) = decl_ms(&mut b, cx, &lb, "attn.score", 1, Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * scale * (1u64 << LOGIT_Q) as f64])))?;
            let mut logits = narrow(&mut b, sc, m, sh, None, DType::I32);
            if windowed {
                let (_, wins, _) = window_tables(s).ok_or_else(|| LowerError::eval("internal: windowed without windows"))?;
                let nw = wins.iter().max().map_or(1, |m| *m as usize + 1);
                let wi = idx_param(&mut b, cx, &lb, "attn.window_ids", wins, nw)?;
                let wr = b.reshape_fixed(wi, &[l as u32, 1]);
                let wc = b.reshape_fixed(wi, &[1, l as u32]);
                let same = b.compare(wr, wc, tir::Cmp::Eq);
                let neg = b.c(DType::I32, i32::MIN as i128);
                logits = b.select(same, logits, neg, DType::I32);
            }
            let pmat = softmax_rows(&mut b, logits, h, l as u32, dh);
            let o = b.matmul(pmat, v.r, DType::I64);
            let ck = site_key("attn.ctx");
            let (kv, kc) = (v.key.clone(), ck.clone());
            let (m, sh) = decl_ms(&mut b, cx, &lb, "attn.ctx", 1, Arc::new(move |c| Ok(vec![c.scale(&kv)? / (1u64 << 24) as f64 / c.scale(&kc)?])))?;
            let ctx = narrow(&mut b, o, m, sh, None, DType::I16);
            b.commit(ctx);
            let ctx = b.transpose(ctx, &[1, 0, 2]);
            let ctx = b.reshape_fixed(ctx, &[l as u32, h * dh]);
            let ctxv = rows_val(ctx, DType::I16, ck, d, "attn.ctx");
            let att = linear_rows(&mut b, cx, &mut lb, &ctxv, "attn.o.w", Some("attn.o.b"), "attn.o", &resid_want())?;
            let r1 = add_rows(&mut b, cx, &lb, &x, &att, "resid.mix")?;
            b.commit(r1.r);
            let n2 = norm_rows_kind(&mut b, cx, &mut lb, &r1, kind, s.eps, "norm2", !s.rms, &codes_want("norm2"))?;
            let hidden = if s.swiglu {
                let gate = linear_rows(&mut b, cx, &mut lb, &n2, "mlp.gate.w", Some("mlp.gate.b"), "mlp.gate", &codes_want("mlp.gate"))?;
                let ga = lower_table_named(&mut b, cx, &mut lb, &gate, TableFn::Act(s.act), "mlp.gate_act")?;
                let up = linear_rows(&mut b, cx, &mut lb, &n2, "mlp.up.w", Some("mlp.up.b"), "mlp.up", &codes_want("mlp.up"))?;
                let prod = b.mul(ga.r, up.r, DType::I32);
                let pk = site_key("mlp.prod");
                let (ka, ku, kp) = (ga.key.clone(), up.key.clone(), pk.clone());
                let (m, sh) = decl_ms(&mut b, cx, &lb, "mlp.prod", 1, Arc::new(move |c| Ok(vec![c.scale(&ka)? * c.scale(&ku)? / c.scale(&kp)?])))?;
                let r = narrow(&mut b, prod, m, sh, None, DType::I16);
                b.commit(r);
                rows_val(r, DType::I16, pk, s.inter, "mlp.prod")
            } else {
                let up = linear_rows(&mut b, cx, &mut lb, &n2, "mlp.up.w", Some("mlp.up.b"), "mlp.up", &codes_want("mlp.up"))?;
                lower_table_named(&mut b, cx, &mut lb, &up, TableFn::Act(s.act), "mlp.act")?
            };
            let down = linear_rows(&mut b, cx, &mut lb, &hidden, "mlp.down.w", Some("mlp.down.b"), "mlp.down", &resid_want())?;
            let r2 = add_rows(&mut b, cx, &lb, &r1, &down, "resid.ffn")?;
            note_site(cx, tb, &r2);
            Ok((b.finish(&[r2.r]), None))
        }
        BlockRole::Post => {
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let ok = out_key();
            let out = match &s.out {
                VisionOut::ClipPooled { proj } => {
                    let row = b.slice(x.r, 0, 0, 1);
                    let row = Val { r: row, ..x.clone() };
                    match proj {
                        Some(_) => {
                            let n = norm_rows_kind(&mut b, cx, &mut lb, &row, NormKind::Layer, s.eps, "post_norm", true, &codes_want("post_norm"))?;
                            linear_rows(&mut b, cx, &mut lb, &n, "proj.w", None, "out", &Want { dt: DType::I32, key: ok.clone() })?
                        }
                        None => norm_rows_kind(&mut b, cx, &mut lb, &row, NormKind::Layer, s.eps, "post_norm", true, &Want { dt: DType::I32, key: ok.clone() })?,
                    }
                }
                VisionOut::SiglipHead => {
                    let xn = norm_rows_kind(&mut b, cx, &mut lb, &x, NormKind::Layer, s.eps, "post_norm", true, &codes_want("post_norm"))?;
                    let (h, dh) = (s.heads as u32, s.head_dim() as u32);
                    // The query is the probe through W_q: data, so one param of codes.
                    let (qw, qb, pr) = (hl_param(hl, "head.q.w")?, hl_param(hl, "head.q.b")?, hl_param(hl, "head.probe")?);
                    let qk = site_key("head.q");
                    let qk2 = qk.clone();
                    let q = decl(
                        &mut b,
                        cx,
                        &lb,
                        "head.q.codes",
                        DType::I16,
                        &[1, d],
                        false,
                        Arc::new(move |c| {
                            let (w, bb, p) = (&c.f(qw)?.data, &c.f(qb)?.data, &c.f(pr)?.data);
                            let sq = c.scale(&qk2)?;
                            let v: Vec<i16> = (0..d)
                                .map(|o| {
                                    let z = bb[o] as f64 + (0..d).map(|i| w[o * d + i] as f64 * p[i] as f64).sum::<f64>();
                                    (z / sq).round().clamp(-32767.0, 32767.0) as i16
                                })
                                .collect();
                            Ok(IntTensor::i16(vec![1, d], v))
                        }),
                    )?;
                    let kv = linear_rows(&mut b, cx, &mut lb, &xn, "head.k.w", Some("head.k.b"), "head.k", &codes_want("head.k"))?;
                    let vv = linear_rows(&mut b, cx, &mut lb, &xn, "head.v.w", Some("head.v.b"), "head.v", &codes_want("head.v"))?;
                    let q3 = b.reshape_fixed(q, &[1, h, dh]);
                    let q3 = b.transpose(q3, &[1, 0, 2]);
                    let k3 = b.reshape_fixed(kv.r, &[l as u32, h, dh]);
                    let k3 = b.transpose(k3, &[1, 2, 0]);
                    let v3 = b.reshape_fixed(vv.r, &[l as u32, h, dh]);
                    let v3 = b.transpose(v3, &[1, 0, 2]);
                    let sc = b.matmul(q3, k3, DType::I64);
                    let (kq, kk, scale) = (qk, kv.key.clone(), 1.0 / (dh as f64).sqrt());
                    let (m, sh) = decl_ms(&mut b, cx, &lb, "head.score", 1, Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * scale * (1u64 << LOGIT_Q) as f64])))?;
                    let logits = narrow(&mut b, sc, m, sh, None, DType::I32);
                    let pm = b.softmax_shifted(logits, 24 - LOGIT_Q);
                    let o = b.matmul(pm, v3, DType::I64);
                    let ck = site_key("head.ctx");
                    let (kv2, kc) = (vv.key.clone(), ck.clone());
                    let (m, sh) = decl_ms(&mut b, cx, &lb, "head.ctx", 1, Arc::new(move |c| Ok(vec![c.scale(&kv2)? / (1u64 << 24) as f64 / c.scale(&kc)?])))?;
                    let ctx = narrow(&mut b, o, m, sh, None, DType::I16);
                    b.commit(ctx);
                    let ctx = b.transpose(ctx, &[1, 0, 2]);
                    let ctx = b.reshape_fixed(ctx, &[1, h * dh]);
                    let ctxv = rows_val(ctx, DType::I16, ck, d, "head.ctx");
                    let o = linear_rows(&mut b, cx, &mut lb, &ctxv, "head.o.w", Some("head.o.b"), "head.o", &resid_want())?;
                    let hn = norm_rows_kind(&mut b, cx, &mut lb, &o, NormKind::Layer, s.eps, "head.norm", true, &codes_want("head.norm"))?;
                    let up = linear_rows(&mut b, cx, &mut lb, &hn, "head.up.w", Some("head.up.b"), "head.up", &codes_want("head.up"))?;
                    let a = lower_table_named(&mut b, cx, &mut lb, &up, TableFn::Act(s.act), "head.act")?;
                    let down = linear_rows(&mut b, cx, &mut lb, &a, "head.down.w", Some("head.down.b"), "head.down", &resid_want())?;
                    let sum = add_rows(&mut b, cx, &lb, &o, &down, "head.out")?;
                    coerce(&mut b, cx, &mut lb, &Val { site: "out".into(), ..sum }, DType::I32, &ok)?
                }
                VisionOut::Merger { out } => {
                    let kind = if s.rms { NormKind::Rms } else { NormKind::Layer };
                    let xn = norm_rows_kind(&mut b, cx, &mut lb, &x, kind, 1e-6, "merger.norm", !s.rms, &codes_want("merger.norm"))?;
                    let mu = (s.merge * s.merge) as usize;
                    let groups = (l / mu) as u32;
                    let g = b.reshape_fixed(xn.r, &[groups, (d * mu) as u32]);
                    let g = Val { r: g, len: d * mu, ..xn };
                    let up = linear_rows(&mut b, cx, &mut lb, &g, "merger.up.w", Some("merger.up.b"), "merger.up", &codes_want("merger.up"))?;
                    let a = lower_table_named(&mut b, cx, &mut lb, &up, TableFn::Act(Act::Gelu), "merger.act")?;
                    let down = linear_rows(&mut b, cx, &mut lb, &a, "merger.down.w", Some("merger.down.b"), "out", &Want { dt: DType::I32, key: ok.clone() })?;
                    let _ = out;
                    match window_tables(s) {
                        Some((_, _, inv)) => {
                            let ni = inv.len();
                            let ii = idx_param(&mut b, cx, &lb, "post.window_restore", inv, ni)?;
                            let r = b.gather(down.r, ii, 0, 0);
                            Val { r, ..down }
                        }
                        None => down,
                    }
                }
                VisionOut::Projector { act, .. } => {
                    let rows = b.slice(x.r, 0, u32::from(s.cls), s.patches() as u32);
                    let rows = Val { r: rows, ..x.clone() };
                    let rc = codes_rows(&mut b, cx, &mut lb, &Val { site: "proj.in".into(), ..rows })?;
                    let up = linear_rows(&mut b, cx, &mut lb, &rc, "proj.up.w", Some("proj.up.b"), "proj.up", &codes_want("proj.up"))?;
                    let a = lower_table_named(&mut b, cx, &mut lb, &up, TableFn::Act(*act), "proj.act")?;
                    linear_rows(&mut b, cx, &mut lb, &a, "proj.down.w", Some("proj.down.b"), "out", &Want { dt: DType::I32, key: ok.clone() })?
                }
            };
            let out = ensure_node(&mut b, &out);
            let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
            b.commit(out.r);
            note_site(cx, tb, &out);
            cx.logits_key = Some(out.key.clone());
            Ok((b.finish(&[]), Some(oi)))
        }
    }
}
