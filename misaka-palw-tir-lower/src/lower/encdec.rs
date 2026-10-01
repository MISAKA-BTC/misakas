//! **Encoder-decoder models** (T5, BART, mBART, Marian, Pegasus) as a two-stage text pipeline:
//! the encoder a stage over the job's source ids, the decoder RFC-0003 §II.2.1's text stage.
//!
//! * **Stage 0, the encoder**: ONE position over the padded source axis `L`, like §12's encoders.
//!   Its inputs are `input.ids` (`idx [L]`, `JobTokens`) and `input.count` (`idx []`,
//!   `JobTokenCount`). Its `Final` output is every decoder layer's cross-attention keys and values,
//!   `i16 [D, 2, L, inner]`: the encoder's last rows through all `2·D` projections as ONE `MatMul`
//!   against the stacked weights, narrowed per channel to each decoder layer's own code scale.
//! * **Stage 1, the decoder**: the text stage (`TextStream`), one position per stream id. Its inputs
//!   are `input.xkv` (`StageFinal { stage: 0 }`) and `input.enc_count` (`JobTokenCount`). Each layer
//!   occurrence picks its `[2, L, inner]` slice with a per-layer index param through `Gather`, and
//!   cross-attention runs over the source axis — a `Fixed` axis, not `H` — with the keys at or past
//!   the count masked. Self-attention is the decoders' own (K/V histories).
//!
//! **Scales across the edge.** The encoder narrows decoder layer `l`'s keys to the scale of the
//! statistics `post.xkv.L{l}.k`, and the decoder reads them at `L{l}.xattn.k`. The float
//! reference records both from the same values, so one calibration gives both programs the same
//! scale.
//!
//! **The families.**
//! * T5 (and mT5): RMSNorm without a mean, pre-norm, unscaled scores, no absolute positions but a
//!   learned bias per head over bucketed relative positions (layer 0's table, shared by every layer
//!   of a stack; bidirectional in the encoder, causal in the decoder), ReLU or gated FFNs, and the
//!   decoder output scaled by `d^−½` before a tied head (`scale_decoder_outputs`).
//! * BART: post-norm, learned positions at `pos + 2`, `layernorm_embedding`.
//! * mBART: BART's positions with pre-norm, `√d` embedding scale and final layer norms.
//! * Marian: post-norm, sinusoidal positions (computed: the checkpoint does not carry them), `√d`.
//! * Pegasus: pre-norm with final layer norms, sinusoidal positions (the checkpoint's table), `√d`.
//!
//! The BART family ties the head to the shared table and adds `final_logits_bias`.
//!
//! The params live in two synthetic HL programs (one anchor node per block), one per stage, as in
//! [`super::vision`]. The float references [`float_encoder`] and [`float_decoder`] use the
//! lowerings' site names, and they calibrate.

use super::bidir::{add_rows, codes_rows, hl_param, input_fill, linear_rows, norm_rows_kind, note_site, rows_val, site_key, softmax_committed, split_softmax};
use super::*;
use crate::float_ref::{ParamStore, SiteStat};
use crate::weights::{Binding, Src};
use serde_json::Value;

/// The encoder's input params (lifted into inputs in this order).
pub const IDS_PARAM: &str = "input.ids";
pub const COUNT_PARAM: &str = "input.count";
/// The decoder's input params (lifted into inputs in this order).
pub const XKV_PARAM: &str = "input.xkv";
pub const ENC_COUNT_PARAM: &str = "input.enc_count";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    T5,
    Bart,
    MBart,
    Marian,
    Pegasus,
}

/// How a stack knows positions.
#[derive(Clone, Debug, PartialEq)]
pub enum Positions {
    /// T5: a learned bias per head over bucketed relative positions.
    Relative { buckets: usize, max_distance: usize },
    /// A learned table read at `position + offset` (BART, mBART: offset 2).
    Learned { rows: usize, offset: usize },
    /// The sinusoid, `sin` in the first half and `cos` in the second (Marian, Pegasus).
    Sinusoidal { rows: usize },
}

impl Positions {
    /// The table rows and the offset of an absolute-position family.
    pub fn table(&self) -> Option<(usize, usize)> {
        match self {
            Positions::Learned { rows, offset } => Some((*rows, *offset)),
            Positions::Sinusoidal { rows } => Some((*rows, 0)),
            Positions::Relative { .. } => None,
        }
    }
}

/// One normalised encoder-decoder.
#[derive(Clone, Debug, PartialEq)]
pub struct EncDecSpec {
    pub architecture: String,
    pub family: Family,
    pub vocab: usize,
    pub d: usize,
    pub enc_layers: usize,
    pub dec_layers: usize,
    pub enc_heads: usize,
    pub dec_heads: usize,
    /// Per head, in each stack (T5: `d_kv` in both).
    pub enc_head_dim: usize,
    pub dec_head_dim: usize,
    pub enc_ffn: usize,
    pub dec_ffn: usize,
    pub act: Act,
    /// `act(gate·x) ⊙ (up·x)` (T5 v1.1's gated FFNs).
    pub gated: bool,
    /// RMSNorm without a mean and without a bias (T5); else LayerNorm with a bias.
    pub rms: bool,
    pub eps: f64,
    pub pre_norm: bool,
    /// Every projection has a bias (the BART family).
    pub bias: bool,
    pub positions: Positions,
    /// Multiplier on the token embedding (`√d` with `scale_embedding`).
    pub embed_scale: f64,
    /// `layernorm_embedding` (BART, mBART).
    pub embed_norm: bool,
    /// A layer norm after each stack (T5, mBART, Pegasus).
    pub final_norm: bool,
    /// The softmax scale on `q·k` (T5: 1).
    pub attn_scale: f64,
    /// Multiplier on the decoder's output before the head (T5 `scale_decoder_outputs`: `d^−½`).
    pub head_scale: f64,
    /// `final_logits_bias` (the BART family).
    pub logits_bias: bool,
    pub decoder_start: u32,
}

impl EncDecSpec {
    pub fn enc_inner(&self) -> usize {
        self.enc_heads * self.enc_head_dim
    }
    /// The cross-attention's width: `q`, and each decoder layer's keys and values.
    pub fn dec_inner(&self) -> usize {
        self.dec_heads * self.dec_head_dim
    }
}

fn get_usize(v: &Value, k: &str) -> Option<usize> {
    v.get(k).and_then(Value::as_u64).map(|x| x as usize)
}

/// Parse a Hugging Face `config.json` of a sequence-to-sequence model.
pub fn parse_encdec(config: &str) -> Result<EncDecSpec> {
    let root: Value = serde_json::from_str(config).map_err(|e| LowerError::bad(format!("config.json: {e}")))?;
    let arch = root
        .get("architectures")
        .and_then(|a| a.get(0))
        .and_then(Value::as_str)
        .ok_or_else(|| LowerError::bad("config.json: no architectures"))?
        .to_string();
    let need = |k: &str| -> Result<usize> { get_usize(&root, k).ok_or_else(|| LowerError::bad(format!("{arch}: no `{k}`"))) };
    let vocab = need("vocab_size")?;
    let d = need("d_model")?;
    let start = get_usize(&root, "decoder_start_token_id").or_else(|| get_usize(&root, "pad_token_id")).unwrap_or(0) as u32;
    match arch.as_str() {
        "T5ForConditionalGeneration" | "MT5ForConditionalGeneration" => {
            let heads = need("num_heads")?;
            let layers = need("num_layers")?;
            let ffp = root.get("feed_forward_proj").and_then(Value::as_str).unwrap_or("relu");
            let parts: Vec<&str> = ffp.split('-').collect();
            let (gated, act_name) = match parts.as_slice() {
                [a] => (false, *a),
                ["gated", a] => (true, if *a == "gelu" { "gelu_new" } else { *a }),
                _ => return Err(LowerError::not_lowerable(format!("{arch}: feed_forward_proj `{ffp}`"))),
            };
            let act = Act::from_hf(act_name).ok_or_else(|| LowerError::not_lowerable(format!("{arch}: activation `{act_name}`")))?;
            // transformers 5.17: `scale_decoder_outputs` when saved, else the old reading of
            // `tie_word_embeddings` (T5 v1.0 ties and scales; v1.1 and Flan do neither).
            let scale_out = root
                .get("scale_decoder_outputs")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| root.get("tie_word_embeddings").and_then(Value::as_bool) != Some(false));
            let dkv = need("d_kv")?;
            Ok(EncDecSpec {
                architecture: arch.clone(),
                family: Family::T5,
                vocab,
                d,
                enc_layers: layers,
                dec_layers: get_usize(&root, "num_decoder_layers").unwrap_or(layers),
                enc_heads: heads,
                dec_heads: heads,
                enc_head_dim: dkv,
                dec_head_dim: dkv,
                enc_ffn: need("d_ff")?,
                dec_ffn: need("d_ff")?,
                act,
                gated,
                rms: true,
                eps: root.get("layer_norm_epsilon").and_then(Value::as_f64).unwrap_or(1e-6),
                pre_norm: true,
                bias: false,
                positions: Positions::Relative {
                    buckets: get_usize(&root, "relative_attention_num_buckets").unwrap_or(32),
                    max_distance: get_usize(&root, "relative_attention_max_distance").unwrap_or(128),
                },
                embed_scale: 1.0,
                embed_norm: false,
                final_norm: true,
                attn_scale: 1.0,
                head_scale: if scale_out { (d as f64).powf(-0.5) } else { 1.0 },
                logits_bias: false,
                decoder_start: start,
            })
        }
        "BartForConditionalGeneration" | "MBartForConditionalGeneration" | "MarianMTModel" | "PegasusForConditionalGeneration" => {
            let family = match arch.as_str() {
                "BartForConditionalGeneration" => Family::Bart,
                "MBartForConditionalGeneration" => Family::MBart,
                "MarianMTModel" => Family::Marian,
                _ => Family::Pegasus,
            };
            if family == Family::Marian {
                if root.get("share_encoder_decoder_embeddings").and_then(Value::as_bool) == Some(false) {
                    return Err(LowerError::not_lowerable(format!("{arch}: separate encoder and decoder embeddings")));
                }
                if get_usize(&root, "decoder_vocab_size").is_some_and(|v| v != vocab) {
                    return Err(LowerError::not_lowerable(format!("{arch}: a decoder vocabulary other than the encoder's")));
                }
            }
            let act_name = root.get("activation_function").and_then(Value::as_str).unwrap_or("gelu");
            let act = Act::from_hf(act_name).ok_or_else(|| LowerError::not_lowerable(format!("{arch}: activation `{act_name}`")))?;
            let (enc_heads, dec_heads) = (need("encoder_attention_heads")?, need("decoder_attention_heads")?);
            if d % enc_heads != 0 || d % dec_heads != 0 {
                return Err(LowerError::bad(format!("{arch}: d_model {d} does not split into the heads")));
            }
            let max_pos = need("max_position_embeddings")?;
            let learned = matches!(family, Family::Bart | Family::MBart);
            let pre = matches!(family, Family::MBart | Family::Pegasus);
            let scale = root.get("scale_embedding").and_then(Value::as_bool).unwrap_or(false);
            let (enc_head_dim, dec_head_dim) = (d / enc_heads, d / dec_heads);
            Ok(EncDecSpec {
                architecture: arch.clone(),
                family,
                vocab,
                d,
                enc_layers: need("encoder_layers")?,
                dec_layers: need("decoder_layers")?,
                enc_heads,
                dec_heads,
                enc_head_dim,
                dec_head_dim,
                enc_ffn: need("encoder_ffn_dim")?,
                dec_ffn: need("decoder_ffn_dim")?,
                act,
                gated: false,
                rms: false,
                eps: 1e-5,
                pre_norm: pre,
                bias: true,
                positions: if learned { Positions::Learned { rows: max_pos + 2, offset: 2 } } else { Positions::Sinusoidal { rows: max_pos } },
                embed_scale: if scale { (d as f64).sqrt() } else { 1.0 },
                embed_norm: learned,
                final_norm: pre,
                attn_scale: (dec_head_dim as f64).powf(-0.5),
                head_scale: 1.0,
                logits_bias: true,
                decoder_start: start,
            })
        }
        other => Err(LowerError::not_lowerable(format!("`{other}` is not an encoder-decoder this lowerer models"))),
    }
}

/// T5's `_relative_position_bucket` for `rel = key − query`, in `f32` as torch computes it.
pub fn t5_bucket(rel: i64, bidirectional: bool, num_buckets: usize, max_distance: usize) -> usize {
    let mut nb = num_buckets as i64;
    let mut ret = 0i64;
    let n = if bidirectional {
        nb /= 2;
        if rel > 0 {
            ret += nb;
        }
        rel.abs()
    } else {
        (-rel).max(0)
    };
    let max_exact = nb / 2;
    if n < max_exact {
        return (ret + n) as usize;
    }
    let lg = ((n as f32) / (max_exact as f32)).ln() / ((max_distance as f64 / max_exact as f64).ln() as f32) * ((nb - max_exact) as f32);
    (ret + (max_exact + lg as i64).min(nb - 1)) as usize
}

/// The sinusoid of Marian and Pegasus (`create_weight`): `[rows, d]`, `sin` then `cos`.
pub fn sinusoid(rows: usize, d: usize) -> Vec<f32> {
    let half = d.div_ceil(2);
    let mut v = vec![0f32; rows * d];
    for p in 0..rows {
        for j in 0..d {
            let (k, f): (usize, fn(f64) -> f64) = if j < half { (2 * j, f64::sin) } else { (2 * (j - half) + 1, f64::cos) };
            let ang = p as f64 / 10000f64.powf((2 * (k / 2)) as f64 / d as f64);
            v[p * d + j] = f(ang) as f32;
        }
    }
    v
}

// ───────────────────────────── params ─────────────────────────────

type Table = Vec<(String, Vec<usize>, bool, Src)>;

/// Checkpoint names by role.
struct Names {
    shared: String,
    /// Per stack (`"encoder"` / `"decoder"`): the layer prefix with `{L}`.
    enc_layer: String,
    dec_layer: String,
}

fn names(s: &EncDecSpec) -> Names {
    match s.family {
        Family::T5 => Names { shared: "shared.weight".into(), enc_layer: "encoder.block.{L}.".into(), dec_layer: "decoder.block.{L}.".into() },
        _ => Names {
            shared: "model.shared.weight".into(),
            enc_layer: "model.encoder.layers.{L}.".into(),
            dec_layer: "model.decoder.layers.{L}.".into(),
        },
    }
}

fn push_lin(v: &mut Table, name: &str, ck: &str, out: usize, inp: usize, bias: bool, pl: bool) {
    v.push((format!("{name}.w"), vec![out, inp], pl, Src::t(format!("{ck}.weight"))));
    if bias {
        v.push((format!("{name}.b"), vec![out], pl, Src::t(format!("{ck}.bias"))));
    }
}

fn push_norm(v: &mut Table, name: &str, ck: &str, d: usize, bias: bool, pl: bool) {
    v.push((format!("{name}.gain"), vec![d], pl, Src::t(format!("{ck}.weight"))));
    if bias {
        v.push((format!("{name}.bias"), vec![d], pl, Src::t(format!("{ck}.bias"))));
    }
}

/// Roles of a layer's sublayers in the checkpoint: `(self-attention, its norm, cross-attention, its
/// norm, FFN, its norm)` under the layer prefix.
fn sublayers(s: &EncDecSpec, decoder: bool) -> [&'static str; 6] {
    match (s.family, decoder) {
        (Family::T5, false) => ["layer.0.SelfAttention", "layer.0.layer_norm", "", "", "layer.1.DenseReluDense", "layer.1.layer_norm"],
        (Family::T5, true) => [
            "layer.0.SelfAttention",
            "layer.0.layer_norm",
            "layer.1.EncDecAttention",
            "layer.1.layer_norm",
            "layer.2.DenseReluDense",
            "layer.2.layer_norm",
        ],
        (_, _) => ["self_attn", "self_attn_layer_norm", "encoder_attn", "encoder_attn_layer_norm", "", "final_layer_norm"],
    }
}

/// `(q, k, v, o)` projection names of an attention module.
fn qkvo(s: &EncDecSpec) -> [&'static str; 4] {
    match s.family {
        Family::T5 => ["q", "k", "v", "o"],
        _ => ["q_proj", "k_proj", "v_proj", "out_proj"],
    }
}

fn push_ffn(v: &mut Table, s: &EncDecSpec, lp: &str, ffn_role: &str, inter: usize) {
    let d = s.d;
    match s.family {
        Family::T5 => {
            if s.gated {
                push_lin(v, "mlp.gate", &format!("{lp}{ffn_role}.wi_0"), inter, d, false, true);
                push_lin(v, "mlp.up", &format!("{lp}{ffn_role}.wi_1"), inter, d, false, true);
            } else {
                push_lin(v, "mlp.up", &format!("{lp}{ffn_role}.wi"), inter, d, false, true);
            }
            push_lin(v, "mlp.down", &format!("{lp}{ffn_role}.wo"), d, inter, false, true);
        }
        _ => {
            push_lin(v, "mlp.up", &format!("{lp}fc1"), inter, d, true, true);
            push_lin(v, "mlp.down", &format!("{lp}fc2"), d, inter, true, true);
        }
    }
}

/// The stack's global prefix for embedding-side tensors (`model.encoder.`).
fn stack_prefix(s: &EncDecSpec, decoder: bool) -> String {
    match (s.family, decoder) {
        (Family::T5, false) => "encoder.".into(),
        (Family::T5, true) => "decoder.".into(),
        (_, false) => "model.encoder.".into(),
        (_, true) => "model.decoder.".into(),
    }
}

/// The embedding-side params of a stack: table, positions (when read from the checkpoint), T5's
/// relative bias, `layernorm_embedding`.
fn push_embedding(v: &mut Table, s: &EncDecSpec, decoder: bool, has: &dyn Fn(&str) -> bool) {
    let n = names(s);
    let sp = stack_prefix(s, decoder);
    v.push(("embed.table".into(), vec![s.vocab, s.d], false, Src::t(n.shared.clone())));
    match &s.positions {
        Positions::Learned { rows, .. } => {
            v.push(("embed.pos_table".into(), vec![*rows, s.d], false, Src::t(format!("{sp}embed_positions.weight"))));
        }
        Positions::Sinusoidal { rows } => {
            let t = format!("{sp}embed_positions.weight");
            if has(&t) {
                v.push(("embed.pos_table".into(), vec![*rows, s.d], false, Src::t(t)));
            }
        }
        Positions::Relative { buckets, .. } => {
            let heads = if decoder { s.dec_heads } else { s.enc_heads };
            let lp = if decoder { &n.dec_layer } else { &n.enc_layer };
            let t = format!("{}layer.0.SelfAttention.relative_attention_bias.weight", lp.replace("{L}", "0"));
            v.push(("attn.rel_bias".into(), vec![*buckets, heads], false, Src::t(t)));
        }
    }
    if s.embed_norm {
        push_norm(v, "embed.norm", &format!("{sp}layernorm_embedding"), s.d, true, false);
    }
}

fn push_final(v: &mut Table, s: &EncDecSpec, decoder: bool) {
    if !s.final_norm {
        return;
    }
    let sp = stack_prefix(s, decoder);
    let ck = if s.family == Family::T5 { format!("{sp}final_layer_norm") } else { format!("{sp}layer_norm") };
    push_norm(v, "final", &ck, s.d, s.bias, false);
}

/// The encoder program's params: its own, and the decoder layers' cross-attention `k`/`v`
/// projections stacked `[D·inner, d]`.
fn encoder_table(s: &EncDecSpec, has: &dyn Fn(&str) -> bool) -> Table {
    let n = names(s);
    let mut v = Table::new();
    push_embedding(&mut v, s, false, has);
    let [sa, sn, _, _, ffn, fnorm] = sublayers(s, false);
    let [q, k, vv, o] = qkvo(s);
    let lp = &n.enc_layer;
    let (d, inner) = (s.d, s.enc_inner());
    push_norm(&mut v, "norm.self", &format!("{lp}{sn}"), d, s.bias, true);
    push_lin(&mut v, "attn.q", &format!("{lp}{sa}.{q}"), inner, d, s.bias, true);
    push_lin(&mut v, "attn.k", &format!("{lp}{sa}.{k}"), inner, d, s.bias, true);
    push_lin(&mut v, "attn.v", &format!("{lp}{sa}.{vv}"), inner, d, s.bias, true);
    push_lin(&mut v, "attn.o", &format!("{lp}{sa}.{o}"), d, inner, s.bias, true);
    push_norm(&mut v, "norm.ffn", &format!("{lp}{fnorm}"), d, s.bias, true);
    push_ffn(&mut v, s, lp, ffn, s.enc_ffn);
    push_final(&mut v, s, false);
    let [_, _, ca, _, _, _] = sublayers(s, true);
    let dl = n.dec_layer.replace("{L}", "{E}");
    let (dn, di) = (s.dec_layers, s.dec_inner());
    for (role, proj) in [("xkv.k", k), ("xkv.v", vv)] {
        v.push((format!("{role}.w"), vec![dn * di, d], false, Src::t(format!("{dl}{ca}.{proj}.weight")).stack('E', dn).reshape(vec![dn * di, d])));
        if s.bias {
            v.push((format!("{role}.b"), vec![dn * di], false, Src::t(format!("{dl}{ca}.{proj}.bias")).stack('E', dn).reshape(vec![dn * di])));
        }
    }
    v
}

fn decoder_table(s: &EncDecSpec, has: &dyn Fn(&str) -> bool) -> Table {
    let n = names(s);
    let mut v = Table::new();
    push_embedding(&mut v, s, true, has);
    let [sa, sn, ca, cn, ffn, fnorm] = sublayers(s, true);
    let [q, k, vv, o] = qkvo(s);
    let lp = &n.dec_layer;
    let (d, inner) = (s.d, s.dec_inner());
    push_norm(&mut v, "norm.self", &format!("{lp}{sn}"), d, s.bias, true);
    push_lin(&mut v, "attn.q", &format!("{lp}{sa}.{q}"), inner, d, s.bias, true);
    push_lin(&mut v, "attn.k", &format!("{lp}{sa}.{k}"), inner, d, s.bias, true);
    push_lin(&mut v, "attn.v", &format!("{lp}{sa}.{vv}"), inner, d, s.bias, true);
    push_lin(&mut v, "attn.o", &format!("{lp}{sa}.{o}"), d, inner, s.bias, true);
    push_norm(&mut v, "norm.cross", &format!("{lp}{cn}"), d, s.bias, true);
    push_lin(&mut v, "xattn.q", &format!("{lp}{ca}.{q}"), inner, d, s.bias, true);
    push_lin(&mut v, "xattn.o", &format!("{lp}{ca}.{o}"), d, inner, s.bias, true);
    push_norm(&mut v, "norm.ffn", &format!("{lp}{fnorm}"), d, s.bias, true);
    push_ffn(&mut v, s, lp, ffn, s.dec_ffn);
    push_final(&mut v, s, true);
    let head = if has("lm_head.weight") { "lm_head.weight".to_string() } else { n.shared.clone() };
    v.push(("head.w".into(), vec![s.vocab, d], false, Src::t(head)));
    if s.logits_bias {
        v.push(("head.b".into(), vec![s.vocab], false, Src::t("final_logits_bias").reshape(vec![s.vocab])));
    }
    v
}

/// A synthetic HL program over `table` (one anchor node per block, the params that are neither
/// per-layer nor in `post` in `pre`) and its binding.
fn synth(s: &EncDecSpec, name: &str, table: Table, layers: usize, rows: usize, post: &[&str]) -> Result<(HlProgram, Binding)> {
    use crate::hl::{Block, CarryDecl, HlType, Init, Node, ParamDecl, Ref};
    let params: Vec<ParamDecl> =
        table.iter().map(|(n, sh, pl, _)| ParamDecl { name: n.clone(), shape: sh.clone(), per_layer: *pl, init: Init::Normal(0.1) }).collect();
    let (mut pre, mut layer, mut postv) = (Vec::new(), Vec::new(), Vec::new());
    for (i, (n, _, pl, _)) in table.iter().enumerate() {
        let r = Ref::Param(i as u32);
        if *pl {
            layer.push(r);
        } else if post.iter().any(|p| n.starts_with(p)) {
            postv.push(r);
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
    let blocks = vec![
        Block { name: "pre".into(), role: BlockRole::Pre, nodes: vec![anchor(pre)], outputs: vec![Ref::Node(0, 0)] },
        Block { name: "layer".into(), role: BlockRole::Layer, nodes: vec![anchor(layer)], outputs: vec![Ref::Node(0, 0)] },
        Block { name: "post".into(), role: BlockRole::Post, nodes: vec![anchor(postv)], outputs: vec![Ref::Node(0, 0)] },
    ];
    let hl = HlProgram {
        architecture: format!("{}:{name}", s.architecture),
        output: crate::hl::HlOutput::Logits,
        vocab: s.vocab,
        hidden: s.d,
        carries: vec![CarryDecl { name: "rows".into(), shape: vec![rows, s.d] }],
        params,
        states: vec![],
        rope_tables: vec![],
        blocks,
        pre: 0,
        post: 2,
        schedule: vec![1; layers],
        layer_of: (0..layers).collect(),
    };
    hl.validate().map_err(|e| LowerError::eval(format!("internal: the {name}'s HL program: {e}")))?;
    let binding = Binding { srcs: table.into_iter().map(|(_, _, _, src)| src).collect(), aliases: vec![], ignored_prefixes: ignored(s) };
    Ok((hl, binding))
}

/// Checkpoint tensors neither stage reads by design: the per-stack copies of the shared table
/// (older checkpoints save them), T5's unused cross-attention bias, and a sinusoid table the
/// lowering computes.
fn ignored(s: &EncDecSpec) -> Vec<String> {
    let v: &[&str] = match s.family {
        Family::T5 => &["encoder.embed_tokens.", "decoder.embed_tokens.", "decoder.block.0.layer.1.EncDecAttention.relative_attention_bias."],
        Family::Marian => &["model.encoder.embed_tokens.", "model.decoder.embed_tokens.", "model.encoder.embed_positions.", "model.decoder.embed_positions."],
        _ => &["model.encoder.embed_tokens.", "model.decoder.embed_tokens."],
    };
    v.iter().map(|x| x.to_string()).collect()
}

/// The two stages' HL programs and bindings: `(encoder, decoder)`. `has` answers whether the
/// checkpoint carries a tensor (an untied head, a saved sinusoid).
#[allow(clippy::type_complexity)]
pub fn hl_programs(s: &EncDecSpec, lmax: usize, has: &dyn Fn(&str) -> bool) -> Result<((HlProgram, Binding), (HlProgram, Binding))> {
    let enc = synth(s, "encoder", encoder_table(s, has), s.enc_layers, lmax, &["final.", "xkv."])?;
    let dec = synth(s, "decoder", decoder_table(s, has), s.dec_layers, 1, &["final.", "head."])?;
    Ok((enc, dec))
}

// ───────────────────────────── the float reference ─────────────────────────────

pub type Stats = BTreeMap<String, SiteStat>;
/// Every site's rows of one run (the per-site diagnosis).
pub type Trace = BTreeMap<String, Vec<Vec<f64>>>;

/// Where a float run's observations go.
struct Obs<'a> {
    stats: Option<&'a mut Stats>,
    trace: Option<&'a mut Trace>,
}

fn observe(o: &mut Obs<'_>, key: String, rows: &[Vec<f64>]) {
    if let Some(t) = o.trace.as_deref_mut() {
        t.insert(key.clone(), rows.to_vec());
    }
    let Some(st) = o.stats.as_deref_mut() else { return };
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
        if i == 0 {
            e.pos0_absmax = e.pos0_absmax.max(row);
        } else {
            e.rest_absmax = e.rest_absmax.max(row);
        }
        e.count += r.len() as u64;
    }
}

/// A param as `f64`s, for `layer` when per-layer.
fn pf(hl: &HlProgram, params: &ParamStore, name: &str, layer: Option<usize>) -> Result<Vec<f64>> {
    let i = hl_param(hl, name)?;
    Ok(params.get(i, layer)?.data.iter().map(|x| *x as f64).collect())
}

fn norm_f(s: &EncDecSpec, x: &[f64], g: &[f64], b: Option<&[f64]>) -> Vec<f64> {
    let n = x.len() as f64;
    if s.rms {
        let ms = x.iter().map(|v| v * v).sum::<f64>() / n;
        let inv = 1.0 / (ms + s.eps).sqrt();
        x.iter().zip(g).map(|(v, g)| v * inv * g).collect()
    } else {
        let mu = x.iter().sum::<f64>() / n;
        let var = x.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / n;
        let inv = 1.0 / (var + s.eps).sqrt();
        x.iter().enumerate().map(|(i, v)| (v - mu) * inv * g[i] + b.map_or(0.0, |b| b[i])).collect()
    }
}

fn lin_f(x: &[f64], w: &[f64], b: Option<&[f64]>, out: usize) -> Vec<f64> {
    let inp = x.len();
    (0..out).map(|o| b.map_or(0.0, |b| b[o]) + (0..inp).map(|i| w[o * inp + i] * x[i]).sum::<f64>()).collect()
}

/// One stack's parameter reader.
struct P<'a> {
    hl: &'a HlProgram,
    params: &'a ParamStore,
    s: &'a EncDecSpec,
}

impl P<'_> {
    fn get(&self, name: &str, layer: Option<usize>) -> Result<Vec<f64>> {
        pf(self.hl, self.params, name, layer)
    }
    fn opt(&self, name: &str, layer: Option<usize>) -> Result<Option<Vec<f64>>> {
        if hl_param(self.hl, name).is_err() {
            return Ok(None);
        }
        self.get(name, layer).map(Some)
    }
    fn norm(&self, site: &str, layer: Option<usize>, x: &[Vec<f64>]) -> Result<Vec<Vec<f64>>> {
        let g = self.get(&format!("{site}.gain"), layer)?;
        let b = self.opt(&format!("{site}.bias"), layer)?;
        Ok(x.iter().map(|r| norm_f(self.s, r, &g, b.as_deref())).collect())
    }
    fn lin(&self, name: &str, layer: Option<usize>, x: &[Vec<f64>]) -> Result<Vec<Vec<f64>>> {
        let w = self.get(&format!("{name}.w"), layer)?;
        let b = self.opt(&format!("{name}.b"), layer)?;
        let i = hl_param(self.hl, &format!("{name}.w"))?;
        let out = self.hl.params[i as usize].shape[0];
        Ok(x.iter().map(|r| lin_f(r, &w, b.as_deref(), out)).collect())
    }
    /// The embedding rows of `ids` at positions `p0..`: scaled token rows plus positions, then
    /// `layernorm_embedding`. Observes `pre.embed.sum` and `pre.embed.norm`.
    fn embed(&self, ids: &[usize], p0: usize, stats: &mut Obs<'_>) -> Result<Vec<Vec<f64>>> {
        let (s, d) = (self.s, self.s.d);
        let table = self.get("embed.table", None)?;
        let pos: Option<(Vec<f64>, usize)> = match &s.positions {
            Positions::Learned { offset, .. } => Some((self.get("embed.pos_table", None)?, *offset)),
            Positions::Sinusoidal { rows } => match self.opt("embed.pos_table", None)? {
                Some(t) => Some((t, 0)),
                None => Some((sinusoid(*rows, d).iter().map(|x| *x as f64).collect(), 0)),
            },
            Positions::Relative { .. } => None,
        };
        let x: Vec<Vec<f64>> = ids
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (0..d)
                    .map(|j| {
                        // HF scales the table row in f32 (`embed_tokens(ids) * embed_scale`).
                        let w = (table[t * d + j] as f32 * s.embed_scale as f32) as f64;
                        w + pos.as_ref().map_or(0.0, |(p, off)| p[(p0 + i + off) * d + j])
                    })
                    .collect()
            })
            .collect();
        observe(stats, "pre.embed.sum".into(), &x);
        if s.embed_norm {
            let y = self.norm("embed.norm", None, &x)?;
            observe(stats, "pre.embed.norm".into(), &y);
            return Ok(y);
        }
        Ok(x)
    }
    /// The FFN of rows `x` (already normed), observing its sites under `pre`.
    fn ffn(&self, pre: &str, layer: Option<usize>, x: &[Vec<f64>], stats: &mut Obs<'_>) -> Result<Vec<Vec<f64>>> {
        let s = self.s;
        let act = |v: f64| crate::float_ref::act(s.act, v as f32) as f64;
        let hidden = if s.gated {
            let gate = self.lin("mlp.gate", layer, x)?;
            observe(stats, format!("{pre}mlp.gate"), &gate);
            let ga: Vec<Vec<f64>> = gate.iter().map(|r| r.iter().map(|v| act(*v)).collect()).collect();
            observe(stats, format!("{pre}mlp.gate_act"), &ga);
            let up = self.lin("mlp.up", layer, x)?;
            observe(stats, format!("{pre}mlp.up"), &up);
            let prod: Vec<Vec<f64>> = ga.iter().zip(&up).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a * b).collect()).collect();
            observe(stats, format!("{pre}mlp.prod"), &prod);
            prod
        } else {
            let up = self.lin("mlp.up", layer, x)?;
            observe(stats, format!("{pre}mlp.up"), &up);
            let a: Vec<Vec<f64>> = up.iter().map(|r| r.iter().map(|v| act(*v)).collect()).collect();
            observe(stats, format!("{pre}mlp.act"), &a);
            a
        };
        let down = self.lin("mlp.down", layer, &hidden)?;
        observe(stats, format!("{pre}mlp.down"), &down);
        Ok(down)
    }
}

/// Scaled-dot-product attention of `q [Lq][h·dh]` over `k`, `v` `[Lk][h·dh]`, with `bias[h][i][j]`
/// added to the scores and key `j` visible to query `i` when `visible(i, j)`.
#[allow(clippy::too_many_arguments)]
fn attend(
    q: &[Vec<f64>],
    k: &[Vec<f64>],
    v: &[Vec<f64>],
    h: usize,
    dh: usize,
    scale: f64,
    bias: &dyn Fn(usize, usize, usize) -> f64,
    visible: &dyn Fn(usize, usize) -> bool,
) -> Vec<Vec<f64>> {
    let mut ctx = vec![vec![0f64; h * dh]; q.len()];
    for hh in 0..h {
        for (i, qi) in q.iter().enumerate() {
            let js: Vec<usize> = (0..k.len()).filter(|j| visible(i, *j)).collect();
            let sc: Vec<f64> =
                js.iter().map(|&j| (0..dh).map(|t| qi[hh * dh + t] * k[j][hh * dh + t]).sum::<f64>() * scale + bias(hh, i, j)).collect();
            let mx = sc.iter().cloned().fold(f64::MIN, f64::max);
            let e: Vec<f64> = sc.iter().map(|x| (x - mx).exp()).collect();
            let z: f64 = e.iter().sum();
            for t in 0..dh {
                ctx[i][hh * dh + t] = js.iter().zip(&e).map(|(&j, w)| w / z * v[j][hh * dh + t]).sum();
            }
        }
    }
    ctx
}

fn add(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    a.iter().zip(b).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a + b).collect()).collect()
}

/// What the encoder stage hands the decoder: the source length, and each decoder layer's cross
/// keys and values over the real rows (`[D][count][inner]`).
#[derive(Clone, Debug)]
pub struct EncoderOut {
    pub count: usize,
    /// The encoder's output rows (after its final norm), `[count][d]`.
    pub hidden: Vec<Vec<f64>>,
    pub xk: Vec<Vec<Vec<f64>>>,
    pub xv: Vec<Vec<Vec<f64>>>,
}

/// The float encoder over the real source ids. With `stats`, every site's statistics under the
/// lowering's names (`pre.embed.sum`, `L1.attn.q`, `post.xkv.L0.k`, …).
pub fn float_encoder(hl: &HlProgram, s: &EncDecSpec, params: &ParamStore, ids: &[usize], stats: Option<&mut Stats>) -> Result<EncoderOut> {
    encoder_run(hl, s, params, ids, Obs { stats, trace: None })
}

/// [`float_encoder`], keeping every site's real rows for the per-site diagnosis.
pub fn float_encoder_traced(hl: &HlProgram, s: &EncDecSpec, params: &ParamStore, ids: &[usize], trace: &mut Trace) -> Result<EncoderOut> {
    encoder_run(hl, s, params, ids, Obs { stats: None, trace: Some(trace) })
}

fn encoder_run(hl: &HlProgram, s: &EncDecSpec, params: &ParamStore, ids: &[usize], mut stats: Obs<'_>) -> Result<EncoderOut> {
    let p = P { hl, params, s };
    let n = ids.len();
    if n == 0 {
        return Err(LowerError::eval("an empty source"));
    }
    let (h, dh) = (s.enc_heads, s.enc_head_dim);
    let mut x = p.embed(ids, 0, &mut stats)?;
    let rel = match &s.positions {
        Positions::Relative { buckets, max_distance } => {
            let t = p.get("attn.rel_bias", None)?;
            Some((t, *buckets, *max_distance))
        }
        _ => None,
    };
    let bias = |hh: usize, i: usize, j: usize| -> f64 {
        match &rel {
            Some((t, nb, md)) => t[t5_bucket(j as i64 - i as i64, true, *nb, *md) * h + hh],
            None => 0.0,
        }
    };
    for li in 0..s.enc_layers {
        let pre = format!("L{li}.");
        let ly = Some(li);
        observe(&mut stats, format!("{pre}carry0"), &x);
        let a_in = if s.pre_norm {
            let y = p.norm("norm.self", ly, &x)?;
            observe(&mut stats, format!("{pre}norm.self"), &y);
            y
        } else {
            x.clone()
        };
        let (q, k, v) = (p.lin("attn.q", ly, &a_in)?, p.lin("attn.k", ly, &a_in)?, p.lin("attn.v", ly, &a_in)?);
        observe(&mut stats, format!("{pre}attn.q"), &q);
        observe(&mut stats, format!("{pre}attn.k"), &k);
        observe(&mut stats, format!("{pre}attn.v"), &v);
        let ctx = attend(&q, &k, &v, h, dh, s.attn_scale, &bias, &|_, _| true);
        observe(&mut stats, format!("{pre}attn.ctx"), &ctx);
        let o = p.lin("attn.o", ly, &ctx)?;
        observe(&mut stats, format!("{pre}attn.o"), &o);
        let r1 = add(&x, &o);
        observe(&mut stats, format!("{pre}resid.self"), &r1);
        let (x1, f_in) = if s.pre_norm {
            let y = p.norm("norm.ffn", ly, &r1)?;
            observe(&mut stats, format!("{pre}norm.ffn"), &y);
            (r1, y)
        } else {
            let y = p.norm("norm.self", ly, &r1)?;
            observe(&mut stats, format!("{pre}norm.self"), &y);
            (y.clone(), y)
        };
        let down = p.ffn(&pre, ly, &f_in, &mut stats)?;
        let r2 = add(&x1, &down);
        observe(&mut stats, format!("{pre}resid.ffn"), &r2);
        x = if s.pre_norm {
            r2
        } else {
            let y = p.norm("norm.ffn", ly, &r2)?;
            observe(&mut stats, format!("{pre}norm.ffn"), &y);
            y
        };
    }
    observe(&mut stats, "post.carry0".into(), &x);
    let hidden = if s.final_norm {
        let y = p.norm("final", None, &x)?;
        observe(&mut stats, "post.final".into(), &y);
        y
    } else {
        x
    };
    let (dn, di) = (s.dec_layers, s.dec_inner());
    let (kw, vw) = (p.get("xkv.k.w", None)?, p.get("xkv.v.w", None)?);
    let (kb, vb) = (p.opt("xkv.k.b", None)?, p.opt("xkv.v.b", None)?);
    let (mut xk, mut xv) = (Vec::with_capacity(dn), Vec::with_capacity(dn));
    for l in 0..dn {
        let (a, b) = (l * di * s.d, (l + 1) * di * s.d);
        let kk: Vec<Vec<f64>> = hidden.iter().map(|r| lin_f(r, &kw[a..b], kb.as_ref().map(|v| &v[l * di..(l + 1) * di]), di)).collect();
        let vv: Vec<Vec<f64>> = hidden.iter().map(|r| lin_f(r, &vw[a..b], vb.as_ref().map(|v| &v[l * di..(l + 1) * di]), di)).collect();
        observe(&mut stats, format!("post.xkv.L{l}.k"), &kk);
        observe(&mut stats, format!("post.xkv.L{l}.v"), &vv);
        xk.push(kk);
        xv.push(vv);
    }
    Ok(EncoderOut { count: n, hidden, xk, xv })
}

/// The float decoder over a whole stream (causal), each position's logits. With `stats`, its sites'
/// statistics; the cross keys and values are observed at `L{l}.xattn.k`/`v` from `enc`, which the
/// encoder observed at `post.xkv.L{l}.k`/`v`.
pub fn float_decoder(
    hl: &HlProgram,
    s: &EncDecSpec,
    params: &ParamStore,
    enc: &EncoderOut,
    stream: &[usize],
    stats: Option<&mut Stats>,
) -> Result<Vec<Vec<f64>>> {
    decoder_run(hl, s, params, enc, stream, Obs { stats, trace: None })
}

/// [`float_decoder`], keeping every site's rows (one per position) for the per-site diagnosis.
pub fn float_decoder_traced(hl: &HlProgram, s: &EncDecSpec, params: &ParamStore, enc: &EncoderOut, stream: &[usize], trace: &mut Trace) -> Result<Vec<Vec<f64>>> {
    decoder_run(hl, s, params, enc, stream, Obs { stats: None, trace: Some(trace) })
}

fn decoder_run(hl: &HlProgram, s: &EncDecSpec, params: &ParamStore, enc: &EncoderOut, stream: &[usize], mut stats: Obs<'_>) -> Result<Vec<Vec<f64>>> {
    let p = P { hl, params, s };
    let (h, dh) = (s.dec_heads, s.dec_head_dim);
    let mut x = p.embed(stream, 0, &mut stats)?;
    let rel = match &s.positions {
        Positions::Relative { buckets, max_distance } => Some((p.get("attn.rel_bias", None)?, *buckets, *max_distance)),
        _ => None,
    };
    let bias = |hh: usize, i: usize, j: usize| -> f64 {
        match &rel {
            Some((t, nb, md)) => t[t5_bucket(j as i64 - i as i64, false, *nb, *md) * h + hh],
            None => 0.0,
        }
    };
    let causal = |i: usize, j: usize| j <= i;
    for li in 0..s.dec_layers {
        let pre = format!("L{li}.");
        let ly = Some(li);
        observe(&mut stats, format!("{pre}carry0"), &x);
        // Self-attention.
        let a_in = if s.pre_norm {
            let y = p.norm("norm.self", ly, &x)?;
            observe(&mut stats, format!("{pre}norm.self"), &y);
            y
        } else {
            x.clone()
        };
        let (q, k, v) = (p.lin("attn.q", ly, &a_in)?, p.lin("attn.k", ly, &a_in)?, p.lin("attn.v", ly, &a_in)?);
        observe(&mut stats, format!("{pre}attn.q"), &q);
        observe(&mut stats, format!("{pre}attn.k"), &k);
        observe(&mut stats, format!("{pre}attn.v"), &v);
        let ctx = attend(&q, &k, &v, h, dh, s.attn_scale, &bias, &causal);
        observe(&mut stats, format!("{pre}attn.ctx"), &ctx);
        let o = p.lin("attn.o", ly, &ctx)?;
        observe(&mut stats, format!("{pre}attn.o"), &o);
        let r1 = add(&x, &o);
        observe(&mut stats, format!("{pre}resid.self"), &r1);
        let x1 = if s.pre_norm {
            r1
        } else {
            let y = p.norm("norm.self", ly, &r1)?;
            observe(&mut stats, format!("{pre}norm.self"), &y);
            y
        };
        // Cross-attention over the encoder's rows.
        let c_in = if s.pre_norm {
            let y = p.norm("norm.cross", ly, &x1)?;
            observe(&mut stats, format!("{pre}norm.cross"), &y);
            y
        } else {
            x1.clone()
        };
        let xq = p.lin("xattn.q", ly, &c_in)?;
        observe(&mut stats, format!("{pre}xattn.q"), &xq);
        observe(&mut stats, format!("{pre}xattn.k"), &enc.xk[li]);
        observe(&mut stats, format!("{pre}xattn.v"), &enc.xv[li]);
        let xctx = attend(&xq, &enc.xk[li], &enc.xv[li], h, dh, s.attn_scale, &|_, _, _| 0.0, &|_, _| true);
        observe(&mut stats, format!("{pre}xattn.ctx"), &xctx);
        let xo = p.lin("xattn.o", ly, &xctx)?;
        observe(&mut stats, format!("{pre}xattn.o"), &xo);
        let r2 = add(&x1, &xo);
        observe(&mut stats, format!("{pre}resid.cross"), &r2);
        let x2 = if s.pre_norm {
            r2
        } else {
            let y = p.norm("norm.cross", ly, &r2)?;
            observe(&mut stats, format!("{pre}norm.cross"), &y);
            y
        };
        // FFN.
        let f_in = if s.pre_norm {
            let y = p.norm("norm.ffn", ly, &x2)?;
            observe(&mut stats, format!("{pre}norm.ffn"), &y);
            y
        } else {
            x2.clone()
        };
        let down = p.ffn(&pre, ly, &f_in, &mut stats)?;
        let r3 = add(&x2, &down);
        observe(&mut stats, format!("{pre}resid.ffn"), &r3);
        x = if s.pre_norm {
            r3
        } else {
            let y = p.norm("norm.ffn", ly, &r3)?;
            observe(&mut stats, format!("{pre}norm.ffn"), &y);
            y
        };
    }
    observe(&mut stats, "post.carry0".into(), &x);
    let y = if s.final_norm {
        let y = p.norm("final", None, &x)?;
        observe(&mut stats, "post.final".into(), &y);
        y
    } else {
        x
    };
    let y: Vec<Vec<f64>> = y.iter().map(|r| r.iter().map(|v| v * s.head_scale).collect()).collect();
    let logits = p.lin("head", None, &y)?;
    observe(&mut stats, "post.logits".into(), &logits);
    Ok(logits)
}

// ───────────────────────────── the lowering ─────────────────────────────

fn new_cx(hl: &HlProgram, hb: u32, max_window: u32) -> Cx<'_> {
    Cx {
        hl,
        fills: Vec::new(),
        row_params: BTreeMap::new(),
        resid_sites: BTreeMap::new(),
        tstate: BTreeMap::new(),
        history_bound: hb,
        max_window,
        logits_key: None,
        site_nodes: BTreeMap::new(),
        shared: Default::default(),
        tables: Default::default(),
        image_rows: None,
        image_cursor: None,
        image_cursor_layer: None,
        split_max_readers: 0,
        quant: BTreeMap::new(),
        carry_keys: BTreeMap::new(),
    }
}

fn new_lb(hl: &HlProgram, hbk: usize) -> Lb {
    let blk = &hl.blocks[hbk];
    let n = blk.nodes.len();
    let prefixes: Vec<String> = match blk.role {
        BlockRole::Pre => vec!["pre.".into()],
        BlockRole::Post => vec!["post.".into()],
        BlockRole::Layer => hl.schedule.iter().enumerate().filter(|(_, k)| **k as usize == hbk).map(|(l, _)| format!("L{l}.")).collect(),
    };
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
        suffix: String::new(),
        appended: BTreeMap::new(),
        carry_in: Vec::new(),
    }
}

/// Record a committed value's site for the per-site diagnosis (the block being built).
fn note(b: &BlockBuilder<'_>, cx: &mut Cx<'_>, v: &Val) {
    note_site(cx, b.pb.blocks.len() as u8, v);
}

fn codes_want(site: &str) -> Want {
    Want { dt: DType::I16, key: site_key(site) }
}

fn resid_want() -> Want {
    Want { dt: DType::I32, key: ScaleKey::resid() }
}

fn norm_kind(s: &EncDecSpec) -> NormKind {
    if s.rms { NormKind::Rms } else { NormKind::Layer }
}

/// `{name}.b` when the family has biases.
fn bias_name(s: &EncDecSpec, name: &str) -> Option<String> {
    s.bias.then(|| format!("{name}.b"))
}

fn input_ref(b: &BlockBuilder<'_>, name: &str) -> Result<tir::Ref> {
    b.pb.params
        .iter()
        .position(|p| p.name == name)
        .map(|i| tir::Ref::Param(i as u16))
        .ok_or_else(|| LowerError::eval(format!("internal: no input `{name}`")))
}

/// A pinned `idx` table, clamped to `[0, bound)` for the analysis.
fn idx_table(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &Lb, name: &str, shape: &[usize], v: Vec<u32>, bound: usize) -> Result<tir::Ref> {
    let sh = shape.to_vec();
    let p = decl(b, cx, lb, name, DType::Idx, shape, false, Arc::new(move |_| Ok(IntTensor::idx(sh.clone(), v.clone()))))?;
    Ok(b.clamp(p, 0, bound.max(1) as i64 - 1, DType::Idx))
}

/// T5's bias table in Q`LOGIT_Q` logit units, `i32 [buckets, heads]` (the scores are unscaled).
fn rel_table_q(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &Lb, s: &EncDecSpec, heads: usize) -> Result<tir::Ref> {
    let Positions::Relative { buckets, .. } = s.positions else { return Err(LowerError::eval("internal: no relative positions")) };
    let tp = hl_param(cx.hl, "attn.rel_bias")?;
    decl(
        b,
        cx,
        lb,
        "attn.rel_bias.q",
        DType::I32,
        &[buckets, heads],
        false,
        Arc::new(move |c| {
            let t = c.f(tp)?;
            Ok(IntTensor::i32(
                vec![buckets, heads],
                t.data.iter().map(|v| (*v as f64 * (1u64 << LOGIT_Q) as f64).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32).collect(),
            ))
        }),
    )
}

/// The token rows of `ids` (`idx [n]`) at the residual scale, `[n, d]`: per-row `i16` table codes,
/// the embedding scale folded into each row's narrowing.
fn word_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &Lb, s: &EncDecSpec, ids: tir::Ref, n: u32) -> Result<tir::Ref> {
    let tp = hl_param(cx.hl, "embed.table")?;
    let (rows, cols) = (s.vocab, s.d);
    let table = decl_rows(b, cx, lb, "embed.table", &[rows, cols], false, RowKind::T16, tp)?;
    let es = s.embed_scale;
    let key = ScaleKey::resid();
    let (m, sh) = decl_ms(
        b,
        cx,
        lb,
        "embed",
        rows,
        Arc::new(move |c| {
            let scales = c.row_scales(tp, true)?;
            let to = c.scale(&key)?;
            Ok(scales.iter().map(|sw| sw * es / to).collect())
        }),
    )?;
    let row = b.gather(table, ids, 0, 0);
    let row = b.reshape_fixed(row, &[n, cols as u32]);
    let mt = b.gather(m, ids, 0, 0);
    let st = b.gather(sh, ids, 0, 0);
    let mt = b.reshape_fixed(mt, &[n, 1]);
    let st = b.reshape_fixed(st, &[n, 1]);
    Ok(narrow(b, row, mt, st, None, DType::I32))
}

/// The absolute-position table at the residual scale, `i32 [rows, d]`: the checkpoint's (learned,
/// or Pegasus's saved sinusoid) or the computed sinusoid (Marian).
fn pos_table(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &Lb, s: &EncDecSpec, first: usize, count: usize) -> Result<tir::Ref> {
    let (rows, _) = s.positions.table().ok_or_else(|| LowerError::eval("internal: no absolute positions"))?;
    let d = s.d;
    let pp = hl_param(cx.hl, "embed.pos_table").ok();
    if first + count > rows {
        return Err(LowerError::not_lowerable(format!("{}: positions {first}..{} of a {rows}-row table", s.architecture, first + count)));
    }
    decl(
        b,
        cx,
        lb,
        "embed.pos_rows",
        DType::I32,
        &[count, d],
        false,
        Arc::new(move |c| {
            let sr = c.scale(&ScaleKey::resid())?;
            let t: Vec<f32> = match pp {
                Some(p) => c.f(p)?.data.clone(),
                None => sinusoid(rows, d),
            };
            let v = (first * d..(first + count) * d).map(|i| (t[i] as f64 / sr).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32).collect();
            Ok(IntTensor::i32(vec![count, d], v))
        }),
    )
}

/// `word + position`, then `layernorm_embedding`: the stack's first residual rows.
fn embed_finish(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, s: &EncDecSpec, word: tir::Ref, pos: Option<tir::Ref>) -> Result<Val> {
    let resid = ScaleKey::resid();
    let sum = match pos {
        Some(p) => {
            let a = b.add(word, p, DType::I64);
            b.clamp(a, i32::MIN as i64, i32::MAX as i64, DType::I32)
        }
        None => word,
    };
    let sum = rows_val(sum, DType::I32, resid.clone(), s.d, "embed.sum");
    note_resid(cx, lb, &sum);
    if !s.embed_norm {
        return Ok(sum);
    }
    let y = norm_rows_kind(b, cx, lb, &sum, NormKind::Layer, s.eps, "embed.norm", true, &resid_want())?;
    note_resid(cx, lb, &y);
    Ok(y)
}

/// [`linear_rows`] with per-row `i16` weights (`x·Wᵀ` exact in `i64`): the projections whose
/// error an unscaled score amplifies (T5's `q` and `k`).
#[allow(clippy::too_many_arguments)]
fn linear16_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, x: &Val, w_name: &str, site: &str, want: &Want) -> Result<Val> {
    let hl = cx.hl;
    let w = hl_param(hl, w_name)?;
    let (out, inp) = (hl.params[w as usize].shape[0], hl.params[w as usize].shape[1]);
    if x.dt != DType::I16 || x.len != inp {
        return Err(LowerError::eval(format!("internal: `{site}` reads {:?} rows of {}, W has {inp} columns", x.dt, x.len)));
    }
    let pl = lb.role == BlockRole::Layer;
    let wt = decl(
        b,
        cx,
        lb,
        &format!("{w_name}.t16"),
        DType::I16,
        &[inp, out],
        pl,
        Arc::new(move |c| {
            let rc = c.rows16(w)?;
            let mut t = vec![0i16; inp * out];
            for o in 0..out {
                for i in 0..inp {
                    t[i * out + o] = rc.codes[o * inp + i];
                }
            }
            Ok(IntTensor::i16(vec![inp, out], t))
        }),
    )?;
    let (kx, ky) = (x.key.clone(), want.key.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        out,
        Arc::new(move |c| {
            let scales = c.rows16(w)?.scales.clone();
            let (sx, sy) = (c.scale(&kx)?, c.scale_vec(&ky, out)?);
            Ok(scales.iter().zip(&sy).map(|(sw, sy)| sw * sx / sy).collect())
        }),
    )?;
    let acc = b.matmul(x.r, wt, DType::I64);
    let r = narrow(b, acc, m, s, None, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    Ok(rows_val(r, want.dt, want.key.clone(), out, site))
}

/// Unscaled scores (T5) multiply a query or key error by the score itself: those projections
/// (self `q`, `k`, cross `q`, and the cross keys) take per-row `i16` weights.
fn wide_qk(s: &EncDecSpec) -> bool {
    s.attn_scale == 1.0 && !s.bias
}

/// A projection: `i16` weights for a query or key of unscaled attention ([`wide_qk`]), `i8`
/// otherwise.
fn proj(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, s: &EncDecSpec, x: &Val, name: &str, want: &Want) -> Result<Val> {
    if wide_qk(s) && matches!(name, "attn.q" | "attn.k" | "xattn.q") {
        return linear16_rows(b, cx, lb, x, &format!("{name}.w"), name, want);
    }
    linear_rows(b, cx, lb, x, &format!("{name}.w"), bias_name(s, name).as_deref(), name, want)
}

/// The FFN of normed codes `x`, into the residual scale.
fn ffn_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, s: &EncDecSpec, x: &Val, inter: usize) -> Result<Val> {
    let hidden = if s.gated {
        let gate = linear_rows(b, cx, lb, x, "mlp.gate.w", bias_name(s, "mlp.gate").as_deref(), "mlp.gate", &codes_want("mlp.gate"))?;
        note(b, cx, &gate);
        let ga = lower_table_named(b, cx, lb, &gate, TableFn::Act(s.act), "mlp.gate_act")?;
        note(b, cx, &ga);
        let up = linear_rows(b, cx, lb, x, "mlp.up.w", bias_name(s, "mlp.up").as_deref(), "mlp.up", &codes_want("mlp.up"))?;
        note(b, cx, &up);
        let prod = b.mul(ga.r, up.r, DType::I32);
        let pk = site_key("mlp.prod");
        let (ka, ku, kp) = (ga.key.clone(), up.key.clone(), pk.clone());
        let (m, sh) = decl_ms(b, cx, lb, "mlp.prod", 1, Arc::new(move |c| Ok(vec![c.scale(&ka)? * c.scale(&ku)? / c.scale(&kp)?])))?;
        let r = narrow(b, prod, m, sh, None, DType::I16);
        b.commit(r);
        rows_val(r, DType::I16, pk, inter, "mlp.prod")
    } else {
        let up = linear_rows(b, cx, lb, x, "mlp.up.w", bias_name(s, "mlp.up").as_deref(), "mlp.up", &codes_want("mlp.up"))?;
        note(b, cx, &up);
        lower_table_named(b, cx, lb, &up, TableFn::Act(s.act), "mlp.act")?
    };
    note(b, cx, &hidden);
    linear_rows(b, cx, lb, &hidden, "mlp.down.w", bias_name(s, "mlp.down").as_deref(), "mlp.down", &resid_want())
}

/// The input of a sublayer: its norm (pre-norm, codes), or the residual's own codes (post-norm).
fn sub_in(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, s: &EncDecSpec, x: &Val, norm: &str) -> Result<Val> {
    let v = if s.pre_norm { norm_rows_kind(b, cx, lb, x, norm_kind(s), s.eps, norm, s.bias, &codes_want(norm))? } else { codes_rows(b, cx, lb, x)? };
    note(b, cx, &v);
    Ok(v)
}

/// The residual after a sublayer: `x + y`, then (post-norm) its norm at the residual scale. The
/// sum is a commit point: a norm reduces a whole row, and by box demand a tile of the norm would
/// otherwise reach every row of the projection `y` came from (`L·d` outputs of a `MatMul`).
fn sub_out(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, s: &EncDecSpec, x: &Val, y: &Val, resid_site: &str, norm: &str) -> Result<Val> {
    let r = add_rows(b, cx, lb, x, y, resid_site)?;
    b.commit(r.r);
    note(b, cx, &r);
    if s.pre_norm {
        return Ok(r);
    }
    let n = norm_rows_kind(b, cx, lb, &r, norm_kind(s), s.eps, norm, s.bias, &resid_want())?;
    b.commit(n.r);
    note_resid(cx, lb, &n);
    Ok(n)
}

/// A context `o:i64 [h, n, dh]` (Q24 probabilities × value codes) to codes at `site`, `[n, h·dh]`.
fn ctx_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, o: tir::Ref, vkey: &ScaleKey, site: &str, n: u32, h: u32, dh: u32) -> Result<Val> {
    let ck = site_key(site);
    let (kv, kc) = (vkey.clone(), ck.clone());
    let (m, sh) = decl_ms(b, cx, lb, site, 1, Arc::new(move |c| Ok(vec![c.scale(&kv)? / (1u64 << 24) as f64 / c.scale(&kc)?])))?;
    let ctx = narrow(b, o, m, sh, None, DType::I16);
    b.commit(ctx);
    if n == 1 {
        note(b, cx, &rows_val(ctx, DType::I16, ck.clone(), (h * dh) as usize, site));
    }
    let ctx = b.transpose(ctx, &[1, 0, 2]);
    let ctx = b.reshape_fixed(ctx, &[n, h * dh]);
    Ok(rows_val(ctx, DType::I16, ck, (h * dh) as usize, site))
}

/// Lower the encoder stage: one position over `L = lmax` source rows, out the stacked cross keys
/// and values `i16 [D, 2, L, inner]` (the program's `Final` output).
pub fn lower_encoder(hl: &HlProgram, s: &EncDecSpec, lmax: u32) -> Result<Lowered> {
    if let Some((rows, off)) = s.positions.table()
        && off + lmax as usize > rows
    {
        return Err(LowerError::not_lowerable(format!("{}: a source of {lmax} needs positions up to {} of {rows}", s.architecture, off + lmax as usize)));
    }
    let hb = tir::program::HISTORY_BOUND_V1_SMALL;
    let token_bound = u32::try_from(s.vocab).map_err(|_| LowerError::not_lowerable("vocabulary beyond u32"))?;
    let mut pb = ProgramBuilder::new(token_bound, hb);
    let mut cx = new_cx(hl, hb, hb);
    let mut block_map = vec![u8::MAX; hl.blocks.len()];
    let mut out_node = None;
    for hbk in [hl.pre, 1, hl.post] {
        let (tb, o) = encoder_block(&mut pb, &mut cx, hbk, s, lmax)?;
        block_map[hbk] = tb;
        if o.is_some() {
            out_node = o;
        }
    }
    finish(pb, cx, hl, block_map, out_node, "encoder")
}

fn finish(pb: ProgramBuilder, cx: Cx<'_>, hl: &HlProgram, block_map: Vec<u8>, out_node: Option<u16>, what: &str) -> Result<Lowered> {
    let out_node = out_node.ok_or_else(|| LowerError::eval("internal: no output node"))?;
    let layers: Vec<u8> = hl.schedule.iter().map(|k| block_map[*k as usize]).collect();
    let program = pb.finish(block_map[hl.pre], layers, block_map[hl.post], out_node);
    tir::validate::validate(&program).map_err(|e| LowerError::eval(format!("internal: the {what} program is not in normal form: {e}")))?;
    let resid_sites = cx.resid_sites.into_iter().map(|((k, _), f)| (k, f)).collect();
    let logits_key = cx.logits_key.ok_or_else(|| LowerError::eval("internal: no output scale"))?;
    Ok(Lowered { program, fills: cx.fills, row_params: cx.row_params, resid_sites, logits_key, block_map, site_nodes: cx.site_nodes, budget_fallbacks: vec![] })
}

fn encoder_block(pb: &mut ProgramBuilder, cx: &mut Cx<'_>, hbk: usize, s: &EncDecSpec, l: u32) -> Result<(u8, Option<u16>)> {
    let hl = cx.hl;
    let role = hl.blocks[hbk].role;
    let d = s.d;
    let carry_sig = vec![TensorType::fixed(DType::I32, &[l, d as u32])];
    let mut lb = new_lb(hl, hbk);
    let tb = pb.blocks.len() as u8;
    let mut b = pb.block(&hl.blocks[hbk].name, if role == BlockRole::Pre { vec![] } else { carry_sig });
    let resid = ScaleKey::resid();
    match role {
        BlockRole::Pre => {
            // The inputs first, so that lifting them leaves every other param's index alone.
            let lu = l as usize;
            let ids = decl(&mut b, cx, &lb, IDS_PARAM, DType::Idx, &[lu], false, input_fill(DType::Idx, vec![lu]))?;
            decl(&mut b, cx, &lb, COUNT_PARAM, DType::Idx, &[], false, input_fill(DType::Idx, vec![]))?;
            let ids = b.clamp(ids, 0, s.vocab as i64 - 1, DType::Idx);
            let word = word_rows(&mut b, cx, &lb, s, ids, l)?;
            let pos = match s.positions.table() {
                Some((_, off)) => Some(pos_table(&mut b, cx, &lb, s, off, lu)?),
                None => None,
            };
            let x = embed_finish(&mut b, cx, &mut lb, s, word, pos)?;
            let x = ensure_node(&mut b, &x);
            note_site(cx, tb, &x);
            Ok((b.finish(&[x.r]), None))
        }
        BlockRole::Layer => {
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let (h, dh) = (s.enc_heads as u32, s.enc_head_dim as u32);
            let a_in = sub_in(&mut b, cx, &mut lb, s, &x, "norm.self")?;
            let mut qkv = Vec::new();
            for name in ["attn.q", "attn.k", "attn.v"] {
                let v = proj(&mut b, cx, &mut lb, s, &a_in, name, &codes_want(name))?;
                let r = b.reshape_fixed(v.r, &[l, h, dh]);
                let r = b.transpose(r, &[1, 0, 2]);
                b.commit(r);
                qkv.push(Val { r, ..v });
            }
            let (q, k, v) = (qkv[0].clone(), qkv[1].clone(), qkv[2].clone());
            let kt = b.transpose(k.r, &[0, 2, 1]);
            let sc = b.matmul(q.r, kt, DType::I64);
            let (kq, kk, scale) = (q.key.clone(), k.key.clone(), s.attn_scale);
            let (m, sh) = decl_ms(&mut b, cx, &lb, "attn.score", 1, Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * scale * (1u64 << LOGIT_Q) as f64])))?;
            let mut logits = narrow(&mut b, sc, m, sh, None, DType::I32);
            if let Positions::Relative { buckets, max_distance } = s.positions {
                let table = rel_table_q(&mut b, cx, &lb, s, h as usize)?;
                let lu = l as usize;
                let ids: Vec<u32> =
                    (0..lu).flat_map(|i| (0..lu).map(move |j| t5_bucket(j as i64 - i as i64, true, buckets, max_distance) as u32)).collect();
                let bk = idx_table(&mut b, cx, &lb, "attn.buckets", &[lu, lu], ids, buckets)?;
                let bias = b.gather(table, bk, 0, 0); // [L, L, h]
                let bias = b.transpose(bias, &[2, 0, 1]);
                let sum = b.add(logits, bias, DType::I64);
                logits = b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
            }
            // Keys at or past the count score i32::MIN, which IntExp maps to exactly 0.
            let count = input_ref(&b, COUNT_PARAM)?;
            let iota = b.iota(DType::Idx, &[Dim::Fixed(l)], 0, 0, 1);
            let keep = b.compare(iota, count, tir::Cmp::Lt);
            let neg = b.c(DType::I32, i32::MIN as i128);
            let masked = b.select(keep, logits, neg, DType::I32);
            let pm = if split_softmax(h, l, dh) { softmax_committed(&mut b, masked, 24 - LOGIT_Q) } else { b.softmax_shifted(masked, 24 - LOGIT_Q) };
            let o = b.matmul(pm, v.r, DType::I64);
            let ctx = ctx_rows(&mut b, cx, &mut lb, o, &v.key, "attn.ctx", l, h, dh)?;
            note_site(cx, tb, &ctx);
            let att = linear_rows(&mut b, cx, &mut lb, &ctx, "attn.o.w", bias_name(s, "attn.o").as_deref(), "attn.o", &resid_want())?;
            let x1 = sub_out(&mut b, cx, &mut lb, s, &x, &att, "resid.self", "norm.self")?;
            let f_in = sub_in(&mut b, cx, &mut lb, s, &x1, "norm.ffn")?;
            let down = ffn_rows(&mut b, cx, &mut lb, s, &f_in, s.enc_ffn)?;
            let x2 = sub_out(&mut b, cx, &mut lb, s, &x1, &down, "resid.ffn", "norm.ffn")?;
            let x2 = ensure_node(&mut b, &x2);
            note_site(cx, tb, &x2);
            Ok((b.finish(&[x2.r]), None))
        }
        BlockRole::Post => {
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let xn = if s.final_norm {
                norm_rows_kind(&mut b, cx, &mut lb, &x, norm_kind(s), s.eps, "final", s.bias, &codes_want("final"))?
            } else {
                codes_rows(&mut b, cx, &mut lb, &x)?
            };
            // Every decoder layer's k and v: one MatMul against the stacked projections, channel
            // `(l·2 + kv)·inner + j`, each narrowed to its decoder layer's own code scale.
            let (dn, di) = (s.dec_layers, s.dec_inner());
            let n = dn * 2 * di;
            let (kp, vp) = (hl_param(hl, "xkv.k.w")?, hl_param(hl, "xkv.v.w")?);
            let (kbp, vbp) = (hl_param(hl, "xkv.k.b").ok(), hl_param(hl, "xkv.v.b").ok());
            // Per-row codes and scales of both stacks: `i16` for unscaled attention, else `i8`.
            let wide = wide_qk(s);
            let codes = move |c: &FillCtx<'_>, p: u32| -> Result<(Vec<i32>, Vec<f64>)> {
                Ok(if wide {
                    let r = c.rows16(p)?;
                    (r.codes.iter().map(|v| *v as i32).collect(), r.scales.clone())
                } else {
                    let r = c.rows(p)?;
                    (r.codes.iter().map(|v| *v as i32).collect(), r.scales.clone())
                })
            };
            let wt = decl(
                &mut b,
                cx,
                &lb,
                "xkv.w.t",
                if wide { DType::I16 } else { DType::I8 },
                &[d, n],
                false,
                Arc::new(move |c| {
                    let (rk, rv) = (codes(c, kp)?.0, codes(c, vp)?.0);
                    let mut t = vec![0i32; d * n];
                    for l in 0..dn {
                        for (kv, rc) in [&rk, &rv].into_iter().enumerate() {
                            for j in 0..di {
                                let (ch, row) = ((l * 2 + kv) * di + j, l * di + j);
                                for i in 0..d {
                                    t[i * n + ch] = rc[row * d + i];
                                }
                            }
                        }
                    }
                    Ok(if wide { IntTensor::i16(vec![d, n], t.iter().map(|v| *v as i16).collect()) } else { IntTensor::i8(vec![d, n], t.iter().map(|v| *v as i8).collect()) })
                }),
            )?;
            let key_of = |l: usize, kv: usize| ScaleKey::site(vec![format!("xkv.L{l}.{}", if kv == 0 { "k" } else { "v" })], false);
            let kx = xn.key.clone();
            let (m, sh) = decl_ms(
                &mut b,
                cx,
                &lb,
                "xkv",
                n,
                Arc::new(move |c| {
                    let (rk, rv) = (codes(c, kp)?.1, codes(c, vp)?.1);
                    let sx = c.scale(&kx)?;
                    let mut v = Vec::with_capacity(n);
                    for l in 0..dn {
                        for (kv, sc) in [&rk, &rv].into_iter().enumerate() {
                            let st = c.scale(&key_of(l, kv))?;
                            for j in 0..di {
                                v.push(sc[l * di + j] * sx / st);
                            }
                        }
                    }
                    Ok(v)
                }),
            )?;
            let z = match (kbp, vbp) {
                (Some(kb), Some(vb)) => Some(decl(
                    &mut b,
                    cx,
                    &lb,
                    "xkv.z",
                    DType::I64,
                    &[n],
                    false,
                    Arc::new(move |c| {
                        let (bk, bv) = (c.f(kb)?, c.f(vb)?);
                        let mut v = Vec::with_capacity(n);
                        for l in 0..dn {
                            for (kv, bt) in [&bk, &bv].into_iter().enumerate() {
                                let st = c.scale(&key_of(l, kv))?;
                                for j in 0..di {
                                    v.push((bt.data[l * di + j] as f64 / st).round() as i64);
                                }
                            }
                        }
                        Ok(IntTensor::i64(vec![n], v))
                    }),
                )?),
                _ => None,
            };
            let acc = b.matmul(xn.r, wt, DType::I64);
            let r = narrow(&mut b, acc, m, sh, z, DType::I16);
            let r = b.reshape_fixed(r, &[l, dn as u32, 2, di as u32]);
            let r = b.transpose(r, &[1, 2, 0, 3]);
            let out = rows_val(r, DType::I16, key_of(0, 0), di, "xkv");
            let out = ensure_node(&mut b, &out);
            let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
            b.commit(out.r);
            cx.logits_key = Some(out.key.clone());
            Ok((b.finish(&[]), Some(oi)))
        }
    }
}

/// Lower the decoder stage (the text stage): one position per stream id, reading the encoder's
/// stacked cross keys and values over `L = lmax` source rows. Its self-attention histories keep
/// `max_window` positions (a class's `max_trip` stays within them).
pub fn lower_decoder(hl: &HlProgram, s: &EncDecSpec, lmax: u32, max_window: u32) -> Result<Lowered> {
    let hb = tir::program::HISTORY_BOUND_V1_SMALL;
    let token_bound = u32::try_from(s.vocab).map_err(|_| LowerError::not_lowerable("vocabulary beyond u32"))?;
    let mut pb = ProgramBuilder::new(token_bound, hb);
    let mut cx = new_cx(hl, hb, max_window.min(hb));
    let mut block_map = vec![u8::MAX; hl.blocks.len()];
    let mut out_node = None;
    for hbk in [hl.pre, 1, hl.post] {
        let (tb, o) = decoder_block(&mut pb, &mut cx, hbk, s, lmax)?;
        block_map[hbk] = tb;
        if o.is_some() {
            out_node = o;
        }
    }
    finish(pb, cx, hl, block_map, out_node, "decoder")
}

fn decoder_block(pb: &mut ProgramBuilder, cx: &mut Cx<'_>, hbk: usize, s: &EncDecSpec, l: u32) -> Result<(u8, Option<u16>)> {
    let hl = cx.hl;
    let role = hl.blocks[hbk].role;
    let d = s.d;
    let carry_sig = vec![TensorType::fixed(DType::I32, &[1, d as u32])];
    let mut lb = new_lb(hl, hbk);
    let tb = pb.blocks.len() as u8;
    let mut b = pb.block(&hl.blocks[hbk].name, if role == BlockRole::Pre { vec![] } else { carry_sig });
    let resid = ScaleKey::resid();
    let (dn, di) = (s.dec_layers, s.dec_inner());
    match role {
        BlockRole::Pre => {
            let lu = l as usize;
            decl(&mut b, cx, &lb, XKV_PARAM, DType::I16, &[dn, 2, lu, di], false, input_fill(DType::I16, vec![dn, 2, lu, di]))?;
            decl(&mut b, cx, &lb, ENC_COUNT_PARAM, DType::Idx, &[], false, input_fill(DType::Idx, vec![]))?;
            let tok = b.reshape_fixed(tir::Ref::Input(INPUT_TOKEN), &[1]);
            let word = word_rows(&mut b, cx, &lb, s, tok, 1)?;
            let pos = match s.positions.table() {
                Some((rows, off)) => {
                    let t = pos_table(&mut b, cx, &lb, s, 0, rows)?;
                    let p = b.cast(tir::Ref::Input(INPUT_POS), DType::I64);
                    let o = b.c(DType::I64, off as i128);
                    let at = b.add(p, o, DType::I64);
                    let at = b.clamp(at, 0, rows as i64 - 1, DType::Idx);
                    let row = b.gather(t, at, 0, 0);
                    Some(b.reshape_fixed(row, &[1, d as u32]))
                }
                None => None,
            };
            let x = embed_finish(&mut b, cx, &mut lb, s, word, pos)?;
            let x = ensure_node(&mut b, &x);
            note_site(cx, tb, &x);
            Ok((b.finish(&[x.r]), None))
        }
        BlockRole::Layer => {
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let (h, dh) = (s.dec_heads, s.dec_head_dim);
            let (h32, dh32, inner) = (h as u32, dh as u32, di as u32);
            // Self-attention over the stream so far (K/V histories).
            let a_in = sub_in(&mut b, cx, &mut lb, s, &x, "norm.self")?;
            let q = proj(&mut b, cx, &mut lb, s, &a_in, "attn.q", &codes_want("attn.q"))?;
            let k = proj(&mut b, cx, &mut lb, s, &a_in, "attn.k", &codes_want("attn.k"))?;
            let v = linear_rows(&mut b, cx, &mut lb, &a_in, "attn.v.w", bias_name(s, "attn.v").as_deref(), "attn.v", &codes_want("attn.v"))?;
            for x in [&q, &k, &v] {
                note(&b, cx, x);
            }
            let window = cx.max_window.min(hist_window_cap(di)).min(cx.history_bound);
            let ks = b.pb.hist_state("attn.k.hist", DType::I16, &[inner], window, true);
            let vs = b.pb.hist_state("attn.v.hist", DType::I16, &[inner], window, true);
            let kr = b.reshape_fixed(k.r, &[inner]);
            b.commit(kr);
            let vr = b.reshape_fixed(v.r, &[inner]);
            b.commit(vr);
            let kw = b.hist_append(ks, kr);
            let vw = b.hist_append(vs, vr);
            let qr = b.reshape_fixed(q.r, &[inner]);
            let qv = Val { r: qr, ..q };
            let rel_bias = match s.positions {
                Positions::Relative { buckets, max_distance } => {
                    let table = rel_table_q(&mut b, cx, &lb, s, h)?;
                    let v: Vec<u32> = (0..=max_distance).map(|dd| t5_bucket(-(dd as i64), false, buckets, max_distance) as u32).collect();
                    let bk = decl(&mut b, cx, &lb, "attn.buckets", DType::Idx, &[max_distance + 1], false, {
                        let n = max_distance + 1;
                        Arc::new(move |_| Ok(IntTensor::idx(vec![n], v.clone())))
                    })?;
                    Some(RelBias { table, buckets: bk, max_d: max_distance as u32, n_buckets: buckets as u32 })
                }
                _ => None,
            };
            let dims = AttnDims { heads: h, kv: h, d: dh, dv: dh, window };
            let ex = AttnExtras { scale: s.attn_scale, softcap: None, alibi: None, sinks: None, rel_bias, chunk: None };
            let ctx = lower_attention(&mut b, cx, &mut lb, &qv, (kw, k.key.clone()), (vw, v.key.clone()), dims, &ex, "attn.ctx", &codes_want("attn.ctx"))?;
            let cr = b.reshape_fixed(ctx.r, &[1, inner]);
            let ctx = Val { r: cr, ..ctx };
            note_site(cx, tb, &ctx);
            let att = linear_rows(&mut b, cx, &mut lb, &ctx, "attn.o.w", bias_name(s, "attn.o").as_deref(), "attn.o", &resid_want())?;
            let x1 = sub_out(&mut b, cx, &mut lb, s, &x, &att, "resid.self", "norm.self")?;
            // Cross-attention over the encoder's rows: this layer's [2, L, inner] slice of the input.
            let c_in = sub_in(&mut b, cx, &mut lb, s, &x1, "norm.cross")?;
            let xq = proj(&mut b, cx, &mut lb, s, &c_in, "xattn.q", &codes_want("xattn.q"))?;
            note(&b, cx, &xq);
            let li = decl(
                &mut b,
                cx,
                &lb,
                "xattn.layer",
                DType::Idx,
                &[],
                true,
                Arc::new(|c| Ok(IntTensor::idx(vec![], vec![c.layer.unwrap_or(0) as u32]))),
            )?;
            let li = b.clamp(li, 0, dn as i64 - 1, DType::Idx);
            let xkv = input_ref(&b, XKV_PARAM)?;
            let kv = b.gather(xkv, li, 0, 0); // [2, L, inner]
            let kk = b.slice(kv, 0, 0, 1);
            let kk = b.reshape_fixed(kk, &[l, h32, dh32]);
            let kk = b.transpose(kk, &[1, 2, 0]); // [h, dh, L]
            let vv = b.slice(kv, 0, 1, 1);
            let vv = b.reshape_fixed(vv, &[l, h32, dh32]);
            let vv = b.transpose(vv, &[1, 0, 2]); // [h, L, dh]
            let q3 = b.reshape_fixed(xq.r, &[h32, 1, dh32]);
            let sc = b.matmul(q3, kk, DType::I64); // [h, 1, L]
            let (kq, kk2, vk, scale) = (xq.key.clone(), site_key("xattn.k"), site_key("xattn.v"), s.attn_scale);
            let (m, sh) = decl_ms(&mut b, cx, &lb, "xattn.score", 1, Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk2)? * scale * (1u64 << LOGIT_Q) as f64])))?;
            let logits = narrow(&mut b, sc, m, sh, None, DType::I32);
            let count = input_ref(&b, ENC_COUNT_PARAM)?;
            let iota = b.iota(DType::Idx, &[Dim::Fixed(l)], 0, 0, 1);
            let keep = b.compare(iota, count, tir::Cmp::Lt);
            let neg = b.c(DType::I32, i32::MIN as i128);
            let masked = b.select(keep, logits, neg, DType::I32);
            let pm = b.softmax_shifted(masked, 24 - LOGIT_Q);
            let o = b.matmul(pm, vv, DType::I64); // [h, 1, dh]
            let xctx = ctx_rows(&mut b, cx, &mut lb, o, &vk, "xattn.ctx", 1, h32, dh32)?;
            note_site(cx, tb, &xctx);
            let xo = linear_rows(&mut b, cx, &mut lb, &xctx, "xattn.o.w", bias_name(s, "xattn.o").as_deref(), "xattn.o", &resid_want())?;
            let x2 = sub_out(&mut b, cx, &mut lb, s, &x1, &xo, "resid.cross", "norm.cross")?;
            // FFN.
            let f_in = sub_in(&mut b, cx, &mut lb, s, &x2, "norm.ffn")?;
            let down = ffn_rows(&mut b, cx, &mut lb, s, &f_in, s.dec_ffn)?;
            let x3 = sub_out(&mut b, cx, &mut lb, s, &x2, &down, "resid.ffn", "norm.ffn")?;
            let x3 = ensure_node(&mut b, &x3);
            note_site(cx, tb, &x3);
            Ok((b.finish(&[x3.r]), None))
        }
        BlockRole::Post => {
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let xn = if s.final_norm {
                norm_rows_kind(&mut b, cx, &mut lb, &x, norm_kind(s), s.eps, "final", s.bias, &codes_want("final"))?
            } else {
                codes_rows(&mut b, cx, &mut lb, &x)?
            };
            // T5's `d^−½` on the decoder output rides in the codes' scale into the head.
            let xn = Val { key: xn.key.times(s.head_scale), ..xn };
            let hb = if s.logits_bias { Some("head.b") } else { None };
            let lk = ScaleKey::site(vec!["logits".into()], true);
            let lg = linear_rows(&mut b, cx, &mut lb, &xn, "head.w", hb, "logits", &Want { dt: DType::I32, key: lk.clone() })?;
            let r = b.reshape_fixed(lg.r, &[s.vocab as u32]);
            let out = ensure_node(&mut b, &Val { r, ..lg });
            let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
            b.commit(out.r);
            note_site(cx, tb, &out);
            cx.logits_key = Some(lk);
            Ok((b.finish(&[]), Some(oi)))
        }
    }
}
