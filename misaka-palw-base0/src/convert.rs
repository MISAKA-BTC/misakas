//! **HF `safetensors` → PALW integer artifact (Phase 2, condition 5).**
//!
//! Reads a Qwen2.5 checkpoint and produces the artifact the integer engine runs and the court
//! opens against. Nothing here is on the block-validation path: an artifact is registered by its
//! root, and a verifier re-runs this to check that root rather than trusting the blob.
//!
//! # Why an offline float pipeline is not a contradiction
//!
//! The consensus constraint is that *execution* must not depend on float reduction order. This
//! runs once, offline, and its output is a hash — so the requirement here is different and
//! stricter in its own way: **this function must be bit-reproducible**, or two people converting
//! one checkpoint get two roots and the class has no identity.
//!
//! That is why there is no summation anywhere in the quantizer. A scale is `absmax / 127`, and
//! `absmax` is a MAX reduction — exact, order-independent, and identical on every platform. The
//! only rounding is one division per weight, done in `f64` with round-half-away-from-zero, which
//! has no FMA to contract and no accumulation to reorder. A quantizer that computed a mean or an
//! MSE would have had to pin an order; this one has nothing to pin.
//!
//! # The three folds, applied here
//!
//! Qwen2.5 has three steps with no BASE-0 op, and each is resolved by an exact transformation at
//! conversion time (`docs/palw-qwen25-class-phase0.md` records why each is exact):
//!
//! * **G1, the RMSNorm learned gain** — `W·diag(g)·x`, so `diag(g)` scales the columns of every
//!   consumer. **This un-ties the embedding**: `tie_word_embeddings` is true in the file, but
//!   `model.norm`'s gain folds into the lm_head and not into the embedding gather, so the two
//!   matrices differ by `diag(g)` afterwards and the artifact carries both. That is a real
//!   consequence of the fold, not a packing choice.
//! * **G2, the q/k/v bias** — quantized into the per-channel `zero` of that projection's
//!   requantization triple.
//! * **G3, RoPE's convention** — Qwen2 rotates `(i, i + d/2)` and BASE-0's pinned table rotates
//!   `(2i, 2i+1)`, so the head-dim axis is permuted once, in the q and k rows.
//!
//! # What this module does NOT decide
//!
//! Which checkpoint. `Qwen2.5-2B` does not exist; the caller passes the geometry it read from a
//! real `config.json`, and [`crate::convert::Qwen25ConvertPlan`] carries it.

use crate::artifact::{ArtifactError, Base0ArtifactV1, Base0LayerWeightsV1, Base0ShapeV1};
use kaspa_consensus_core::palw_base0_ops::{QuantParams, ScaleParams};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    /// The header did not parse, or the file is not `safetensors` at all.
    BadContainer(&'static str),
    /// A tensor the plan needs is not in the file.
    MissingTensor(String),
    /// A tensor is there but the wrong shape — the checkpoint is not the geometry claimed.
    ShapeMismatch { tensor: String, want: Vec<usize>, got: Vec<usize> },
    /// A dtype this converter does not read. Qwen2.5 ships BF16 throughout; anything else is a
    /// different file and guessing at its layout would produce a plausible, wrong artifact.
    UnsupportedDtype { tensor: String, dtype: String },
    /// A tensor whose every weight is zero has no scale — `absmax / 127` would be a division by
    /// zero, and a silently-1 scale would quantize it to a different tensor.
    DegenerateTensor(String),
    Artifact(ArtifactError),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::BadContainer(why) => write!(f, "not a readable safetensors container: {why}"),
            ConvertError::MissingTensor(n) => write!(f, "the checkpoint has no tensor `{n}`"),
            ConvertError::ShapeMismatch { tensor, want, got } => {
                write!(f, "`{tensor}` is {got:?}, and the declared geometry needs {want:?}")
            }
            ConvertError::UnsupportedDtype { tensor, dtype } => write!(f, "`{tensor}` is {dtype}, which this converter does not read"),
            ConvertError::DegenerateTensor(n) => write!(f, "`{n}` is entirely zero and has no quantization scale"),
            ConvertError::Artifact(e) => write!(f, "the artifact refused the converted weights: {e:?}"),
        }
    }
}

/// One tensor's location and shape inside a `safetensors` blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorSpan {
    pub dtype: String,
    pub shape: Vec<usize>,
    pub begin: usize,
    pub end: usize,
}

/// The parsed header: every tensor's dtype, shape and byte range, plus where the data starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetensorsIndex {
    pub tensors: BTreeMap<String, TensorSpan>,
    pub data_offset: usize,
}

/// Parse a `safetensors` header: 8 little-endian bytes of length, then that many bytes of JSON.
///
/// Total, never panicking: this reads a file someone else produced, and a panic on a malformed
/// one is a crash rather than a rejection.
pub fn parse_safetensors_header(blob: &[u8]) -> Result<SafetensorsIndex, ConvertError> {
    if blob.len() < 8 {
        return Err(ConvertError::BadContainer("shorter than the 8-byte header length"));
    }
    let n = u64::from_le_bytes(blob[..8].try_into().expect("8 bytes")) as usize;
    let end = 8usize.checked_add(n).ok_or(ConvertError::BadContainer("header length overflows"))?;
    if end > blob.len() {
        return Err(ConvertError::BadContainer("header length runs past the end of the file"));
    }
    let json: serde_json::Value =
        serde_json::from_slice(&blob[8..end]).map_err(|_| ConvertError::BadContainer("the header is not JSON"))?;
    let map = json.as_object().ok_or(ConvertError::BadContainer("the header is not a JSON object"))?;
    let mut tensors = BTreeMap::new();
    for (name, spec) in map {
        if name == "__metadata__" {
            continue;
        }
        let spec = spec.as_object().ok_or(ConvertError::BadContainer("a tensor entry is not an object"))?;
        let dtype = spec
            .get("dtype")
            .and_then(|v| v.as_str())
            .ok_or(ConvertError::BadContainer("a tensor entry has no dtype"))?
            .to_string();
        let shape: Vec<usize> = spec
            .get("shape")
            .and_then(|v| v.as_array())
            .ok_or(ConvertError::BadContainer("a tensor entry has no shape"))?
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as usize)
            .collect();
        let offsets = spec
            .get("data_offsets")
            .and_then(|v| v.as_array())
            .ok_or(ConvertError::BadContainer("a tensor entry has no data_offsets"))?;
        if offsets.len() != 2 {
            return Err(ConvertError::BadContainer("data_offsets is not a pair"));
        }
        let begin = offsets[0].as_u64().unwrap_or(0) as usize;
        let fin = offsets[1].as_u64().unwrap_or(0) as usize;
        if fin < begin {
            return Err(ConvertError::BadContainer("a tensor's data_offsets run backwards"));
        }
        tensors.insert(name.clone(), TensorSpan { dtype, shape, begin, end: fin });
    }
    Ok(SafetensorsIndex { tensors, data_offset: end })
}

/// Read one tensor as `f32`, checking its declared shape.
///
/// BF16 only, deliberately: Qwen2.5 ships BF16 throughout (measured), and a converter that
/// guessed at another layout would produce a plausible and wrong artifact rather than an error.
pub fn read_bf16_tensor(blob: &[u8], index: &SafetensorsIndex, name: &str, want: &[usize]) -> Result<Vec<f32>, ConvertError> {
    let span = index.tensors.get(name).ok_or_else(|| ConvertError::MissingTensor(name.to_string()))?;
    if span.dtype != "BF16" {
        return Err(ConvertError::UnsupportedDtype { tensor: name.to_string(), dtype: span.dtype.clone() });
    }
    if span.shape != want {
        return Err(ConvertError::ShapeMismatch { tensor: name.to_string(), want: want.to_vec(), got: span.shape.clone() });
    }
    let begin = index.data_offset + span.begin;
    let end = index.data_offset + span.end;
    if end > blob.len() || (end - begin) != want.iter().product::<usize>() * 2 {
        return Err(ConvertError::BadContainer("a tensor's byte range does not match its shape"));
    }
    // BF16 is the top 16 bits of an f32: widening is a shift, exact and platform-independent.
    Ok(blob[begin..end].chunks_exact(2).map(|c| f32::from_bits(u32::from(u16::from_le_bytes([c[0], c[1]])) << 16)).collect())
}

/// `absmax / 127`, as an exact max reduction.
///
/// No mean, no MSE, no percentile: each of those is a SUM, and a sum needs a pinned order to be
/// reproducible. A max has nothing to pin — it is order-independent and identical everywhere —
/// which is what makes two people converting one checkpoint reach one root.
fn scale_of(values: &[f32]) -> Option<f64> {
    let absmax = values.iter().fold(0f64, |acc, v| {
        let a = (*v as f64).abs();
        if a > acc { a } else { acc }
    });
    if absmax == 0.0 { None } else { Some(absmax / 127.0) }
}

/// One weight to int8, round-half-away-from-zero, saturating.
///
/// `f64` and no FMA: the only rounding in the whole pipeline, and it has neither an accumulation
/// to reorder nor a contraction to fuse.
fn quantize(value: f32, scale: f64) -> i8 {
    let scaled = value as f64 / scale;
    let rounded = if scaled >= 0.0 { (scaled + 0.5).floor() } else { (scaled - 0.5).ceil() };
    rounded.clamp(-127.0, 127.0) as i8
}

/// Quantize a whole tensor at one scale, refusing an all-zero one.
fn quantize_tensor(name: &str, values: &[f32]) -> Result<(Vec<i8>, f64), ConvertError> {
    let scale = scale_of(values).ok_or_else(|| ConvertError::DegenerateTensor(name.to_string()))?;
    Ok((values.iter().map(|v| quantize(*v, scale)).collect(), scale))
}

/// The geometry and the naming a conversion needs. Read from a real `config.json`; this type does
/// not invent one.
/// `Eq` is deliberately absent: the plan carries a float, and two plans that differ only by a
/// float are not a thing this type should claim to compare.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Qwen25ConvertPlan {
    pub shape: Base0ShapeV1,
    /// f32 bits of `rms_norm_eps`, carried so the artifact records which epsilon the float model
    /// used even though the integer class runs `shape.eps_q`.
    pub rms_norm_eps_bits: u32,
}

/// **The value one normalized activation code is worth — DERIVED, not calibrated.**
///
/// A bias enters as the `zero` of a requantization triple, and `zero` is in units of the output
/// activation code, so placing it needs the activation's own step. That looks like it needs
/// calibration — a forward pass measuring what range each projection actually produces — and it
/// does not, for one reason: **the matmul's input is the output of an RMS norm, whose RMS is 1 by
/// construction.** Its range is set by the requantization that narrows Qk to a code, not by the
/// model and not by the data.
///
/// `rms_norm` returns Qk (`K` fractional bits) and `norm_requant` shifts it down, so a code is
/// worth `2^(shift − K)`. At the convention `derive_deterministic` established — `shift = K − 7`,
/// "1.0 must land on 127, not on 1" — that is `1/128`.
///
/// *(This function replaces a `Qwen25ConvertPlan::activation_scale` field and the "calibration
/// blocker" that came with it. The blocker was mine: the converter had set `norm_requant` to a
/// shift of `K`, which is the collapse that comment warns about — every normalized value landing
/// on 1 or 0 — and a bias measured against it rounded away. The design was never data-dependent.)*
pub fn activation_scale_of(norm_requant: &QuantParams) -> f64 {
    let k = kaspa_consensus_core::palw_base0::K as i32;
    2f64.powi(norm_requant.shift as i32 - k)
}

/// Fold a per-channel gain into the COLUMNS of a `[out][in]` row-major matrix.
///
/// `W diag(g) x` — the gain multiplies input channel `j`, so it scales column `j` of `W`. Folding
/// it into the rows instead would scale the OUTPUTS, which is a different (and wrong) model, and
/// the mistake would be invisible in a shape check.
fn fold_gain_into_columns(w: &mut [f32], out_dim: usize, in_dim: usize, gain: &[f32]) {
    debug_assert_eq!(w.len(), out_dim * in_dim);
    debug_assert_eq!(gain.len(), in_dim);
    for o in 0..out_dim {
        for i in 0..in_dim {
            w[o * in_dim + i] *= gain[i];
        }
    }
}

/// Permute the head-dimension axis of a `[out][in]` projection's ROWS from Qwen2's half-split
/// rotary layout to BASE-0's pairwise one (G3).
///
/// Qwen2 pairs `(i, i + d/2)`; the pinned table pairs `(2i, 2i+1)`. So output row `2i` must carry
/// what was row `i`, and row `2i+1` what was row `i + d/2`. Applied per head, because the axis is
/// per head.
fn permute_rope_rows(w: &[f32], out_dim: usize, in_dim: usize, d_head: usize) -> Vec<f32> {
    let mut out = vec![0f32; w.len()];
    let heads = out_dim / d_head;
    let half = d_head / 2;
    for h in 0..heads {
        for i in 0..half {
            let src_lo = (h * d_head + i) * in_dim;
            let src_hi = (h * d_head + half + i) * in_dim;
            let dst_lo = (h * d_head + 2 * i) * in_dim;
            let dst_hi = (h * d_head + 2 * i + 1) * in_dim;
            out[dst_lo..dst_lo + in_dim].copy_from_slice(&w[src_lo..src_lo + in_dim]);
            out[dst_hi..dst_hi + in_dim].copy_from_slice(&w[src_hi..src_hi + in_dim]);
        }
    }
    out
}

/// Permute a per-channel vector (a bias) the same way its matrix's rows were permuted.
fn permute_rope_vector(v: &[f32], d_head: usize) -> Vec<f32> {
    let mut out = vec![0f32; v.len()];
    let heads = v.len() / d_head;
    let half = d_head / 2;
    for h in 0..heads {
        for i in 0..half {
            out[h * d_head + 2 * i] = v[h * d_head + i];
            out[h * d_head + 2 * i + 1] = v[h * d_head + half + i];
        }
    }
    out
}

/// **The conversion.** A Qwen2.5 `safetensors` blob and its geometry in; a PALW integer artifact
/// out, with the three folds applied and every scale derived by max reduction.
pub fn convert_qwen25(blob: &[u8], plan: &Qwen25ConvertPlan) -> Result<Base0ArtifactV1, ConvertError> {
    let index = parse_safetensors_header(blob)?;
    let s = plan.shape;
    let norm_shift = (kaspa_consensus_core::palw_base0::K as u8) - 7;
    let activation_scale = activation_scale_of(&QuantParams { multiplier: i32::MAX, shift: norm_shift, zero: 0 });
    let d = s.d_model();
    let kv = s.kv_dim();
    let read = |name: &str, want: &[usize]| read_bf16_tensor(blob, &index, name, want);

    // The embedding, unfolded: the GATHER reads it as it is.
    let embed_f = read("model.embed_tokens.weight", &[s.vocab, d])?;
    let (embed, _) = quantize_tensor("model.embed_tokens.weight", &embed_f)?;

    // The lm_head is the SAME matrix with `model.norm`'s gain folded in — which un-ties it. The
    // file has no `lm_head.weight` (tie_word_embeddings is true), and after the fold the two
    // matrices differ by `diag(g)`, so the artifact carries both.
    let final_gain = read("model.norm.weight", &[d])?;
    let mut unembed_f = embed_f.clone();
    fold_gain_into_columns(&mut unembed_f, s.vocab, d, &final_gain);
    let (unembed, _) = quantize_tensor("lm_head (tied, norm-folded)", &unembed_f)?;

    let mut layers = Vec::with_capacity(s.n_layers);
    for li in 0..s.n_layers {
        let at = |t: &str| format!("model.layers.{li}.{t}");

        // G1: `input_layernorm`'s gain folds into all three of q, k and v — they consume the same
        // normed row, so they absorb the same `diag(g)`.
        let attn_gain = read(&at("input_layernorm.weight"), &[d])?;
        let mut wq_f = read(&at("self_attn.q_proj.weight"), &[d, d])?;
        let mut wk_f = read(&at("self_attn.k_proj.weight"), &[kv, d])?;
        let mut wv_f = read(&at("self_attn.v_proj.weight"), &[kv, d])?;
        fold_gain_into_columns(&mut wq_f, d, d, &attn_gain);
        fold_gain_into_columns(&mut wk_f, kv, d, &attn_gain);
        fold_gain_into_columns(&mut wv_f, kv, d, &attn_gain);

        // G3: q and k are rotated, so their rows are permuted into the pinned table's pairing. V
        // is NOT — no rotation applies to it, and permuting it would silently shuffle the value
        // vector's channels.
        let wq_f = permute_rope_rows(&wq_f, d, d, s.d_head);
        let wk_f = permute_rope_rows(&wk_f, kv, d, s.d_head);

        // G2: the biases. q and k's are permuted with their rows; v's is not.
        let bq = permute_rope_vector(&read(&at("self_attn.q_proj.bias"), &[d])?, s.d_head);
        let bk = permute_rope_vector(&read(&at("self_attn.k_proj.bias"), &[kv])?, s.d_head);
        let bv = read(&at("self_attn.v_proj.bias"), &[kv])?;

        let (wq, sq) = quantize_tensor(&at("q_proj"), &wq_f)?;
        let (wk, sk) = quantize_tensor(&at("k_proj"), &wk_f)?;
        let (wv, sv) = quantize_tensor(&at("v_proj"), &wv_f)?;
        let (wo, _) = quantize_tensor(&at("o_proj"), &read(&at("self_attn.o_proj.weight"), &[d, d])?)?;

        // G1 again on the FFN side: `post_attention_layernorm` feeds gate and up.
        let ffn_gain = read(&at("post_attention_layernorm.weight"), &[d])?;
        let mut gate_f = read(&at("mlp.gate_proj.weight"), &[s.d_ff, d])?;
        let mut up_f = read(&at("mlp.up_proj.weight"), &[s.d_ff, d])?;
        fold_gain_into_columns(&mut gate_f, s.d_ff, d, &ffn_gain);
        fold_gain_into_columns(&mut up_f, s.d_ff, d, &ffn_gain);
        let (w_gate, _) = quantize_tensor(&at("gate_proj"), &gate_f)?;
        let (w_up, _) = quantize_tensor(&at("up_proj"), &up_f)?;
        let (w_down, _) = quantize_tensor(&at("down_proj"), &read(&at("mlp.down_proj.weight"), &[d, s.d_ff])?)?;

        // The activation requantization. `shift` maps a `d`-long accumulator back to the
        // activation range; the bias enters as this channel's `zero`, at the accumulator's own
        // scale — which is what makes it the same quantity the matmul produced.
        let shift_for = |n: usize| -> u8 { 5 + ((usize::BITS - 1 - n.leading_zeros()) / 2) as u8 };
        let triples = |bias: &[f32], scale_w: f64, n: usize| -> Vec<QuantParams> {
            // `zero` is in units of the OUTPUT activation code, and one such code is worth
            // `scale_w × activation_scale × 2^shift` in the model's own units. Every factor is
            // derived: two from the tensors, the third from the norm's own requantization.
            let scale_out = scale_w * activation_scale * (1u64 << shift_for(n)) as f64;
            bias.iter()
                .map(|b| QuantParams {
                    multiplier: i32::MAX,
                    shift: shift_for(n),
                    zero: (*b as f64 / scale_out).round().clamp(-127.0, 127.0) as i32,
                })
                .collect()
        };

        layers.push(Base0LayerWeightsV1 {
            wq,
            wk,
            wv,
            wo,
            w_gate,
            w_up,
            w_down,
            qkv_channel_requant: Some([triples(&bq, sq, d), triples(&bk, sk, d), triples(&bv, sv, d)]),
            requant: [
                QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                QuantParams { multiplier: i32::MAX, shift: shift_for(s.d_ff), zero: 0 },
                QuantParams { multiplier: i32::MAX, shift: shift_for(s.d_ff), zero: 0 },
            ],
            attn_logit_scale: ScaleParams { multiplier: i32::MAX, shift: 0 },
            ffn_gate_scale: ScaleParams { multiplier: i32::MAX, shift: 0 },
        });
    }

    // The SAME convention `derive_deterministic` uses: Qk → an activation code where 1.0 lands on
    // 127 rather than on 1. A shift of `K` would land it on 1 and collapse every normalized value
    // to a handful of levels — which is what this converter did until the bias arithmetic made it
    // visible.
    let norm_requant =
        QuantParams { multiplier: i32::MAX, shift: (kaspa_consensus_core::palw_base0::K as u8) - 7, zero: 0 };
    let residual_requant = QuantParams { multiplier: i32::MAX, shift: 1, zero: 0 };
    Base0ArtifactV1::from_parts(s, embed, unembed, layers, norm_requant, residual_requant).map_err(ConvertError::Artifact)
}

/// How many q/k/v channels of a converted artifact carry a NON-ZERO bias.
///
/// A conversion where this is zero has not carried the biases: they rounded away, because
/// `activation_scale` was not calibrated for this checkpoint. The count exists so that failure is
/// a number somebody reads rather than a model that runs and is quietly wrong — and it is exactly
/// the quantity Phase 3's calibration has to move.
pub fn biased_channel_count(artifact: &Base0ArtifactV1) -> usize {
    artifact
        .layers
        .iter()
        .filter_map(|l| l.qkv_channel_requant.as_ref())
        .flat_map(|per| per.iter())
        .flat_map(|v| v.iter())
        .filter(|q| q.zero != 0)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::LN_THETA_10000_GEN_Q;

    /// A tiny Qwen2.5-shaped checkpoint, written as a real `safetensors` blob so the parser is
    /// exercised rather than bypassed. Values are a fixed integer sequence: reproducible, and
    /// distinct per tensor so a mixed-up name shows as a wrong number rather than a wrong shape.
    fn tiny_checkpoint(s: &Base0ShapeV1) -> Vec<u8> {
        let d = s.d_model();
        let kv = s.kv_dim();
        let mut specs: Vec<(String, Vec<usize>)> = vec![
            ("model.embed_tokens.weight".into(), vec![s.vocab, d]),
            ("model.norm.weight".into(), vec![d]),
        ];
        for li in 0..s.n_layers {
            for (t, shape) in [
                ("input_layernorm.weight", vec![d]),
                ("self_attn.q_proj.weight", vec![d, d]),
                ("self_attn.q_proj.bias", vec![d]),
                ("self_attn.k_proj.weight", vec![kv, d]),
                ("self_attn.k_proj.bias", vec![kv]),
                ("self_attn.v_proj.weight", vec![kv, d]),
                ("self_attn.v_proj.bias", vec![kv]),
                ("self_attn.o_proj.weight", vec![d, d]),
                ("post_attention_layernorm.weight", vec![d]),
                ("mlp.gate_proj.weight", vec![s.d_ff, d]),
                ("mlp.up_proj.weight", vec![s.d_ff, d]),
                ("mlp.down_proj.weight", vec![d, s.d_ff]),
            ] {
                specs.push((format!("model.layers.{li}.{t}"), shape));
            }
        }
        let mut header = serde_json::Map::new();
        let mut data: Vec<u8> = Vec::new();
        let mut seed = 1u64;
        for (name, shape) in &specs {
            let n: usize = shape.iter().product();
            let begin = data.len();
            for _ in 0..n {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                // A small signed value with an exact bf16 representation, so the fixture's
                // arithmetic is not itself a rounding experiment.
                let v = ((seed >> 33) % 9) as i32 - 4;
                let bits = (v as f32).to_bits();
                data.extend_from_slice(&((bits >> 16) as u16).to_le_bytes());
            }
            header.insert(
                name.clone(),
                serde_json::json!({ "dtype": "BF16", "shape": shape, "data_offsets": [begin, data.len()] }),
            );
        }
        let json = serde_json::to_vec(&serde_json::Value::Object(header)).unwrap();
        let mut blob = (json.len() as u64).to_le_bytes().to_vec();
        blob.extend_from_slice(&json);
        blob.extend_from_slice(&data);
        blob
    }

    fn tiny_qwen_shape() -> Base0ShapeV1 {
        // Qwen2.5's structure at a size a test can run: grouped-query attention with a real
        // group, and the head dim even so the rotary pairing is defined.
        Base0ShapeV1 {
            n_layers: 2,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 64,
            vocab: 32,
            max_position: 16,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1 << 8,
        }
    }

    /// **Condition 5: a checkpoint converts to an artifact, and the same checkpoint always
    /// converts to the SAME artifact.**
    ///
    /// Reproducibility is not a nicety here: a verifier re-runs this to check the registered
    /// root, so two conversions that disagreed would leave the class with no identity.
    #[test]
    fn a_checkpoint_converts_reproducibly() {
        let shape = tiny_qwen_shape();
        let blob = tiny_checkpoint(&shape);
        let plan = Qwen25ConvertPlan { shape, rms_norm_eps_bits: 1e-6f32.to_bits() };

        let a = convert_qwen25(&blob, &plan).expect("a well-formed checkpoint converts");
        let b = convert_qwen25(&blob, &plan).unwrap();
        assert_eq!(a.execution_class_id(), b.execution_class_id(), "one checkpoint, one class id");
        assert_eq!(a.embed, b.embed);
        assert_eq!(a.layers, b.layers);
        assert!(!a.is_derived(), "a converted artifact is not a derived one");

        // The shapes really came from the file.
        assert_eq!(a.embed.len(), shape.vocab * shape.d_model());
        assert_eq!(a.layers.len(), shape.n_layers);
        assert_eq!(a.layers[0].wk.len(), shape.kv_dim() * shape.d_model(), "K is kv-wide, not hidden-wide");

        // G2: every q/k/v channel carries its own triple, which is where the bias went.
        let per = a.layers[0].qkv_channel_requant.as_ref().expect("converted artifacts carry per-channel triples");
        assert_eq!(per[0].len(), shape.d_model());
        assert_eq!(per[1].len(), shape.kv_dim());
        assert!(per[0].iter().any(|q| q.zero != 0), "the biases reach their zero points");
        assert!(biased_channel_count(&a) > 0);

        // The activation step is DERIVED, not calibrated: an RMS norm's output has RMS 1 by
        // construction, so its range is set by the requantization that narrows Qk to a code.
        assert_eq!(activation_scale_of(&a.norm_requant), 1.0 / 128.0, "1.0 lands on 127, not on 1");

        // And the collapse the convention exists to prevent, measured: at a shift of `K` a
        // normalized value lands on 1 instead of 127, and a bias measured against that step
        // rounds away entirely. `biased_channel_count` is what makes that a number somebody reads
        // rather than a model that runs and is quietly wrong.
        let collapsed = QuantParams { multiplier: i32::MAX, shift: kaspa_consensus_core::palw_base0::K as u8, zero: 0 };
        assert_eq!(activation_scale_of(&collapsed), 1.0, "at a shift of K one code is one whole unit");
    }

    /// **And it really executes** — the converted artifact runs the integer engine, which is what
    /// "bit-exact execution at 1 layer" means at this phase.
    #[test]
    fn a_converted_artifact_runs_the_engine() {
        let mut shape = tiny_qwen_shape();
        shape.n_layers = 1;
        let blob = tiny_checkpoint(&shape);
        let plan = Qwen25ConvertPlan { shape, rms_norm_eps_bits: 1e-6f32.to_bits() };
        let a = convert_qwen25(&blob, &plan).unwrap();

        let engine = crate::engine::Base0Engine::new(&a);
        let run = || {
            let mut cache = crate::engine::KvCache::new(&a);
            (0..3).map(|p| engine.forward_token(&mut cache, p + 1, p).expect("the pass completes")).collect::<Vec<_>>()
        };
        let first = run();
        assert_eq!(first.len(), 3);
        assert!(first.iter().all(|l| l.len() == shape.vocab));
        assert_eq!(run(), first, "two runs of a converted artifact agree bit for bit");
    }

    /// The folds are applied where they belong, and the tests assert the direction — folding a
    /// gain into rows instead of columns is a different model and a shape check cannot see it.
    #[test]
    fn a_gain_folds_into_columns_and_the_rope_permutation_pairs_the_halves() {
        // `W diag(g)` scales COLUMN j by g[j], the same for every output row.
        let mut w = vec![1.0f32, 1.0, 1.0, 1.0, 1.0, 1.0];
        fold_gain_into_columns(&mut w, 2, 3, &[2.0, 3.0, 5.0]);
        assert_eq!(w, vec![2.0, 3.0, 5.0, 2.0, 3.0, 5.0], "each column carries its own gain, in every row");

        // The rotary permutation: row i and row i+d/2 become rows 2i and 2i+1.
        let rows: Vec<f32> = (0..4).flat_map(|r| vec![r as f32; 2]).collect(); // 4 rows of width 2
        let permuted = permute_rope_rows(&rows, 4, 2, 4);
        assert_eq!(permuted, vec![0.0, 0.0, 2.0, 2.0, 1.0, 1.0, 3.0, 3.0], "half-split pairs become adjacent pairs");
        // …and a bias moves the same way, or it would be added to the wrong lane.
        assert_eq!(permute_rope_vector(&[10.0, 11.0, 12.0, 13.0], 4), vec![10.0, 12.0, 11.0, 13.0]);
    }

    /// A checkpoint that is not the geometry claimed is refused by NAME and shape, not
    /// misinterpreted — a converter that read on regardless would produce a plausible, wrong
    /// artifact and a root nobody could trace back to a mistake.
    #[test]
    fn a_checkpoint_that_does_not_match_the_plan_is_refused() {
        let shape = tiny_qwen_shape();
        let blob = tiny_checkpoint(&shape);

        let mut wrong = shape;
        wrong.d_ff = 128;
        let plan = Qwen25ConvertPlan { shape: wrong, rms_norm_eps_bits: 0 };
        match convert_qwen25(&blob, &plan) {
            Err(ConvertError::ShapeMismatch { tensor, .. }) => assert!(tensor.contains("gate_proj")),
            other => panic!("a wrong ffn width must be refused by name, got {other:?}"),
        }

        // A missing tensor names itself.
        let mut truncated = tiny_checkpoint(&shape);
        let idx = parse_safetensors_header(&truncated).unwrap();
        assert!(idx.tensors.contains_key("model.norm.weight"));
        truncated.truncate(4);
        assert!(matches!(parse_safetensors_header(&truncated), Err(ConvertError::BadContainer(_))));

        // Garbage is a rejection, never a panic: this reads a file someone else produced.
        assert!(parse_safetensors_header(&[0xFF; 64]).is_err());
        assert!(parse_safetensors_header(&[]).is_err());
    }

    /// An all-zero tensor has no scale. `absmax / 127` would divide by zero and a silently-1
    /// scale would quantize it to a different tensor, so it is an error.
    #[test]
    fn a_degenerate_tensor_has_no_scale() {
        assert!(scale_of(&[0.0, 0.0, -0.0]).is_none());
        assert_eq!(scale_of(&[0.0, 127.0]), Some(1.0));
        // Rounding is half-away-from-zero and symmetric, and it saturates rather than wrapping.
        assert_eq!(quantize(0.5, 1.0), 1);
        assert_eq!(quantize(-0.5, 1.0), -1);
        assert_eq!(quantize(1.4, 1.0), 1);
        assert_eq!(quantize(1_000.0, 1.0), 127);
        assert_eq!(quantize(-1_000.0, 1.0), -127);
    }
}
