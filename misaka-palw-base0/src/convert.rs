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
    ShapeMismatch {
        tensor: String,
        want: Vec<usize>,
        got: Vec<usize>,
    },
    /// A dtype this converter does not read. Qwen2.5 ships BF16 throughout; anything else is a
    /// different file and guessing at its layout would produce a plausible, wrong artifact.
    UnsupportedDtype {
        tensor: String,
        dtype: String,
    },
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
        let dtype =
            spec.get("dtype").and_then(|v| v.as_str()).ok_or(ConvertError::BadContainer("a tensor entry has no dtype"))?.to_string();
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

/// **The amplifying gain a `fan_in`-long int8 dot needs to land in the Qk band (ADR-0040 H).**
///
/// Without it an attention logit arrives at `SoftMax` around 0.002 in Qk and the distribution is
/// uniform to four decimals — attention selecting nothing — and the SwiGLU gate degenerates to
/// the linear `x/2`. `derive_deterministic` uses fixture constants for this and says so: "a real
/// artifact's scales come from calibrating against its own activation statistics".
///
/// For a converted class both statistics are available without a forward pass:
///
/// * **σ_w is measured**, from the quantized weights themselves — an exact integer sum of squares
///   in `i64`, so it is order-independent and reproducible, which a float RMS would not have
///   been.
/// * **σ_x is a construction constant.** The dot's other operand is an RMS-normed activation, and
///   an RMS norm's output has RMS 1 by definition — so in codes its RMS is `1 / activation_scale`,
///   which is 128 at the established `shift = K − 7`.
///
/// An `n`-term dot of independent terms then has σ ≈ `√n · σ_w · σ_x`, and the target is `2^22`,
/// a quarter of Qk, leaving headroom before `rescale_q` saturates. `rescale` with
/// `multiplier ≈ 2^31` is a gain of `2^(31 − shift)`, so the shift follows.
pub fn amplification_for(fan_in: usize, weights: &[i8], activation_scale: f64) -> ScaleParams {
    // Exact, integer, order-independent: a float accumulation here would make the artifact
    // depend on summation order and two converters could disagree.
    let sum_sq: i64 = weights.iter().map(|w| (*w as i64) * (*w as i64)).sum();
    let sigma_w = ((sum_sq as f64) / (weights.len().max(1) as f64)).sqrt().max(1.0);
    let sigma_x = 1.0 / activation_scale;
    let dot_sigma = (fan_in as f64).sqrt() * sigma_w * sigma_x;
    // gain = 2^22 / dot_sigma, and gain = 2^(31 - shift).
    let gain_log2 = 22.0 - dot_sigma.log2();
    let shift = (31.0 - gain_log2).round().clamp(0.0, 62.0) as u8;
    ScaleParams { multiplier: i32::MAX, shift }
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

        let attn_logit_scale = amplification_for(s.d_head, &wq, activation_scale);
        let ffn_gate_scale = amplification_for(d, &w_gate, activation_scale);
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
            // Measured from the tensors that feed each dot: the attention logit reduces over
            // `d_head` against the ROTATED query codes, and the gate over `d_model` against the
            // normed ones.
            attn_logit_scale,
            ffn_gate_scale,
        });
    }

    // The SAME convention `derive_deterministic` uses: Qk → an activation code where 1.0 lands on
    // 127 rather than on 1. A shift of `K` would land it on 1 and collapse every normalized value
    // to a handful of levels — which is what this converter did until the bias arithmetic made it
    // visible.
    let norm_requant = QuantParams { multiplier: i32::MAX, shift: (kaspa_consensus_core::palw_base0::K as u8) - 7, zero: 0 };
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

/// **Phase 3's measurement: does a converted class stay numerically alive as it gets deeper?**
///
/// The failure mode integer inference has, and float inference does not, is silent collapse: each
/// residual add halves the stream (`residual_requant`'s shift of 1, the standard int8-residual
/// convention), so after enough layers every code can reach zero and every downstream projection
/// reads zeros. A model in that state still runs, still produces logits, and means nothing.
///
/// So the numbers here are the ones that distinguish "deep" from "dead", and each has a reason:
///
/// * `residual_peak` — the largest `|code|` in the stream after each block. A run whose peak
///   walks to zero has collapsed, and the LAYER it collapses at is what a depth sweep is for.
/// * `saturated_channels` — how many codes sit at ±127. The opposite failure: a stream pinned at
///   the rail carries no information either, and requantization that is too generous produces it.
/// * `gate_extremes` — SiLU's asymmetry. Fed below its Qk domain `IntSigmoid` returns ≈0.5 and
///   the gate becomes the linear `x/2`, whose output is still large and still weight-dependent —
///   so the peak cannot see the defect. A working gate floors at −0.278 and passes positives, so
///   `|min| ≪ max`; a degenerate one has `|min| ≈ max`.
/// * `attention_spread` — a spread of zero is a uniform distribution, i.e. attention selecting
///   nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthHealthV1 {
    pub layers: usize,
    /// Per layer, over the whole run: the largest residual `|code|` seen.
    pub residual_peak: Vec<i32>,
    /// Per layer: `(most negative, most positive)` SiLU gate code.
    pub gate_extremes: Vec<(i32, i32)>,
    /// The smallest attention spread seen at a position with MORE THAN ONE key. Zero means some
    /// head selected nothing among keys it had a choice between.
    ///
    /// Position 0 is excluded, and not as a convenience: it has exactly one key, so its
    /// distribution is `[1.0]` and its spread is necessarily zero. Including it reported every
    /// model as attention-dead, which is a statement about the metric rather than the model.
    pub min_attention_spread: i32,
    /// Layers whose residual peak sits at the int8 rail (`>= 127`), and how many layers there
    /// were.
    ///
    /// Measured on the RESIDUAL STREAM, not on the logits: logits are raw `i32` accumulators out
    /// of the unembedding matmul and are naturally in the thousands, so counting `|v| >= 127`
    /// there measures nothing. (It was the first thing this struct did, and the depth sweep
    /// reported every model as railed at depth 1.)
    pub saturated_residual: (usize, usize),
    /// The greedy argmax at each step — what a top-k agreement check compares.
    pub argmax: Vec<usize>,
    /// A cheap digest of the full logit row at each step.
    ///
    /// The argmax alone cannot answer "does the model read its input": under RANDOM weights it is
    /// pinned to whichever vocabulary row has the largest norm, whatever the prompt, so a
    /// constant argmax there is expected rather than a defect. The logits themselves do vary, and
    /// this is what says so.
    pub logits_digest: Vec<u64>,
}

impl DepthHealthV1 {
    /// The stream is alive at every layer: no collapse to zero, and no rail.
    pub fn is_alive(&self) -> bool {
        self.residual_peak.iter().all(|p| *p > 0) && self.saturated_residual.0 * 2 < self.saturated_residual.1.max(1)
    }

    /// The SiLU gate is doing its job at every layer: it FLOORS while positives pass through.
    ///
    /// The test is `|min| < max`, strictly — which is the property that actually separates SiLU
    /// from its degenerate form. Fed below its Qk domain `IntSigmoid` returns ≈0.5 and the gate
    /// becomes the linear `x/2`, whose output is symmetric: `|min| == max`. A tighter ratio
    /// (`|min| · 2 < max`) looks stricter and measures something else — as depth grows the
    /// positive peak decays while SiLU's floor stays put, so that test reports signal DECAY as
    /// gate degeneracy. The decay is real and is what [`Self::gate_peak_decay`] reports; it is
    /// not this predicate's question.
    pub fn gate_is_asymmetric(&self) -> bool {
        self.gate_extremes.iter().all(|(lo, hi)| lo.abs() < *hi || *hi == 0)
    }

    /// The gate's positive peak at each layer — the number a depth sweep is looking for. A
    /// sequence that walks toward zero is the signal dying with depth, whatever the ratios say.
    pub fn gate_peak_decay(&self) -> Vec<i32> {
        self.gate_extremes.iter().map(|(_, hi)| *hi).collect()
    }
}

/// Run `prompt` through `artifact` and report [`DepthHealthV1`].
pub fn measure_depth_health(artifact: &Base0ArtifactV1, prompt: &[usize]) -> Result<DepthHealthV1, crate::engine::EngineError> {
    let engine = crate::engine::Base0Engine::new(artifact);
    let mut cache = crate::engine::KvCache::new(artifact);
    let n = artifact.shape.n_layers;
    let mut residual_peak = vec![0i32; n];
    let mut gate_extremes = vec![(0i32, 0i32); n];
    let mut min_attention_spread = i32::MAX;
    let mut argmax = Vec::with_capacity(prompt.len());
    let mut logits_digest = Vec::with_capacity(prompt.len());

    for (position, token) in prompt.iter().enumerate() {
        let (logits, probe) = engine.forward_token_probed(&mut cache, *token, position)?;
        for (i, p) in probe.residual_peak.iter().enumerate() {
            residual_peak[i] = residual_peak[i].max(*p);
        }
        for (i, (lo, hi)) in probe.gate_extremes.iter().enumerate() {
            gate_extremes[i].0 = gate_extremes[i].0.min(*lo);
            gate_extremes[i].1 = gate_extremes[i].1.max(*hi);
        }
        // Only where there was a choice to make (see the field's doc).
        if position > 0 {
            for s in &probe.attention_spread {
                min_attention_spread = min_attention_spread.min(*s);
            }
        }
        argmax.push(crate::engine::argmax_lowest(&logits));
        // FNV-1a over the row: order-sensitive, cheap, and enough to tell two rows apart.
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for v in &logits {
            for b in v.to_le_bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        logits_digest.push(h);
    }
    // The rail, measured where int8 codes actually live. A layer whose peak reaches 127 has a
    // stream pinned against the boundary, which carries as little information as a collapsed one.
    let railed = residual_peak.iter().filter(|p| **p >= 127).count();
    Ok(DepthHealthV1 {
        layers: n,
        residual_peak,
        gate_extremes,
        min_attention_spread: if min_attention_spread == i32::MAX { 0 } else { min_attention_spread },
        saturated_residual: (railed, n),
        argmax,
        logits_digest,
    })
}

/// **Phase 3's contingency: derive a per-layer residual narrowing from a measured pass.**
///
/// One global `residual_requant` was not enough on the real Qwen2.5-1.5B — measured at 28 layers
/// the residual peak reaches 11 out of 127, so the stream occupies under a tenth of the int8
/// range, its effective precision is about 3.5 bits, and the argmax degenerates to a constant
/// token. A single shift cannot hold the stream up as the projections' gains vary from layer to
/// layer, because there is only one of it.
///
/// So: convert once with the global rule, run a pass, and re-derive each layer's shift from the
/// peak that layer actually produced. A layer whose stream sits at 11 of 127 is shifting one bit
/// too many; one at 127 is shifting one too few. The target is a peak near 64 — half the range,
/// which leaves a bit of headroom on each side rather than either rail.
///
/// Iterated, because one pass measures the stream the OLD shifts produced: changing layer 3's
/// shift changes what layer 4 sees. Two or three rounds is enough in practice and the loop stops
/// when nothing moves, so a shape that will not settle costs a bounded number of passes rather
/// than looping.
///
/// # What this can and cannot fix, measured
///
/// On the real Qwen2.5-1.5B it moves the argmax off a single constant token — `[11, 11, 11, 11]`
/// before, `[476, 854, 2878, 854]` after — and roughly triples the attention spread (2,855 →
/// 8,442). That is a real improvement and it is not the whole fix.
///
/// **A requantization can only ever REDUCE.** `QuantParams`' gain is `multiplier / 2^shift` with
/// the multiplier at most 1.0, so every setting attenuates and the best a decayed layer can be
/// given is `shift = 0`. Measured: the calibrated table is `[1, 0, 1, 1, …]` — layer 1 took the
/// one bit available and every other layer was already at the floor. A stream that has decayed
/// needs AMPLIFICATION, and that is `Rescale` (ADR-0040 Decision H), the op that exists precisely
/// because "requantize cannot: its gain is at most 1 at every parameter".
///
/// So the residual peak still reaches 5 of 127 at its worst. Closing that needs an amplifying
/// residual — a `Rescale` before the narrowing, per layer — which is a change to BASE-0's own
/// residual arithmetic. **ADR-0050 made that decision and it is implemented**, so this loop now
/// sets a GAIN as well as a shift: when a layer's stream is below the target the narrowing has
/// nothing left to give (`shift = 0` is its floor) and the gain lifts it instead.
///
/// **This is calibration, and its output is part of the class identity** — `artifact_root` covers
/// the per-layer table, so a class calibrated on one prompt set is a different class from one
/// calibrated on another. The prompt is an argument for exactly that reason: it is a registration
/// input, not a detail.
pub fn calibrate_layer_residuals(
    artifact: &Base0ArtifactV1,
    prompt: &[usize],
    rounds: usize,
) -> Result<Base0ArtifactV1, crate::engine::EngineError> {
    /// Half the int8 range: headroom on both sides rather than a rail on either.
    const TARGET_PEAK: i32 = 64;
    let n = artifact.shape.n_layers;
    let mut current = artifact.clone();
    let mut shifts: Vec<[u8; 2]> = (0..n).map(|_| [artifact.residual_requant.shift; 2]).collect();
    // ADR-0050 B: the amplification a decayed layer needs, in bits. `ScaleParams`' gain is
    // `multiplier · 2^-shift` with the multiplier read as a Q31 fraction, so `UNITY_SHIFT - g` is
    // a gain of `2^g` and 31 is unity.
    let mut gains: Vec<[u8; 2]> = (0..n).map(|_| [0u8; 2]).collect();

    for _ in 0..rounds.max(1) {
        let health = measure_depth_health(&current, prompt)?;
        let mut moved = false;
        for (layer, peak) in health.residual_peak.iter().enumerate() {
            // How many bits the stream is away from the target, as a shift correction. A peak of
            // 11 against 64 wants two bits back; a peak of 127 wants one bit given up.
            let delta = if *peak <= 0 {
                // A dead layer wants every bit it can get.
                -2i32
            } else {
                -((TARGET_PEAK as f64 / *peak as f64).log2().round() as i32)
            };
            if delta == 0 {
                continue;
            }
            for site in 0..2 {
                let wanted = shifts[layer][site] as i32 + delta;
                let next = wanted.clamp(0, 31) as u8;
                if next != shifts[layer][site] {
                    shifts[layer][site] = next;
                    moved = true;
                }
                // **What the narrowing could not give, the gain gives.** `wanted < 0` is the
                // measured case: the layer asked for more bits than `shift = 0` has, and before
                // ADR-0050 the request was simply clamped away. Capped so an amplifying residual
                // cannot itself rail the stream — a gain that overshoots is a different failure
                // with the same symptom.
                let deficit = (-wanted).clamp(0, 8) as u8;
                if deficit != gains[layer][site] {
                    gains[layer][site] = deficit;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
        let table: Vec<[QuantParams; 2]> = shifts
            .iter()
            .map(|pair| {
                [
                    QuantParams { multiplier: i32::MAX, shift: pair[0], zero: 0 },
                    QuantParams { multiplier: i32::MAX, shift: pair[1], zero: 0 },
                ]
            })
            .collect();
        current = current.with_layer_residual_requant(table).expect("one pair per layer, by construction");
        let scales: Vec<[ScaleParams; 2]> = gains
            .iter()
            .map(|pair| {
                [
                    ScaleParams { multiplier: i32::MAX, shift: ScaleParams::UNITY_SHIFT - pair[0] },
                    ScaleParams { multiplier: i32::MAX, shift: ScaleParams::UNITY_SHIFT - pair[1] },
                ]
            })
            .collect();
        current = current.with_layer_residual_scale(scales).expect("one pair per layer, by construction");
    }
    Ok(current)
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
        let mut specs: Vec<(String, Vec<usize>)> =
            vec![("model.embed_tokens.weight".into(), vec![s.vocab, d]), ("model.norm.weight".into(), vec![d])];
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
            header.insert(name.clone(), serde_json::json!({ "dtype": "BF16", "shape": shape, "data_offsets": [begin, data.len()] }));
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
        assert_eq!(a.artifact_digest(), b.artifact_digest(), "one checkpoint, one class id");
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

    /// **Phase 3's depth sweep, measured on converted artifacts.**
    ///
    /// The question is whether a converted class stays numerically alive as layers accumulate.
    /// Each residual add halves the stream, so the failure mode is silent collapse — a model that
    /// still runs, still produces logits, and means nothing.
    ///
    /// This prints nothing and asserts the properties: at every depth the residual peak is
    /// non-zero at every layer, the gate keeps SiLU's asymmetry, attention selects something, and
    /// the logits are not pinned at the rail.
    #[test]
    fn a_converted_class_stays_alive_as_it_deepens() {
        let mut collapsed_at = None;
        for layers in [1usize, 4, 8, 12] {
            let mut shape = tiny_qwen_shape();
            shape.n_layers = layers;
            let blob = tiny_checkpoint(&shape);
            let plan = Qwen25ConvertPlan { shape, rms_norm_eps_bits: 1e-6f32.to_bits() };
            let a = convert_qwen25(&blob, &plan).expect("converts at every depth");

            let health = measure_depth_health(&a, &[1, 5, 9, 2]).expect("the run completes");
            assert_eq!(health.layers, layers);
            assert_eq!(health.residual_peak.len(), layers, "one peak per layer");

            if health.residual_peak.contains(&0) {
                collapsed_at = Some((layers, health.residual_peak.clone()));
                continue;
            }
            assert!(health.is_alive(), "depth {layers}: the stream is alive and not railed — {health:?}");
            assert!(health.gate_is_asymmetric(), "depth {layers}: SiLU floors while positives pass — {:?}", health.gate_extremes);
            let decay = health.gate_peak_decay();
            assert!(decay.iter().all(|p| *p > 0), "depth {layers}: the gate signal reached zero — {decay:?}");
            // Measured 2026-08-21 on this fixture, and recorded because the shape of the curve is
            // the finding: the residual peak stabilises in the 56..96 band rather than walking to
            // zero, the gate peak drops sharply from layer 1 to layer 2 (91 → 46) and then holds
            // in the 39..58 band, and the attention spread falls with depth but stays far above
            // uniform. None of the three is the collapse an int8 residual stack is at risk of.
            //
            //   depth  residual peaks                          gate peaks                spread
            //   1      [96]                                    [91]                      1_788_600
            //   4      [96, 70, 68, 71]                        [91, 46, 43, 39]            274_624
            //   8      [96, 70, 68, 71, 73, 71, 71, 88]        [91, …, 48]                  28_936
            //   12     [96, …, 92, 75]                         [91, …, 43, 58]              28_936
            assert!(
                health.residual_peak.iter().all(|p| (16..127).contains(p)),
                "depth {layers}: the residual band is neither collapsing nor railing — {:?}",
                health.residual_peak
            );
            assert!(health.min_attention_spread > 0, "depth {layers}: every head selected something");
            assert_eq!(health.argmax.len(), 4);
        }
        // A collapse is a RESULT, not a test failure — it is what a depth sweep exists to find,
        // and the depth it happens at is the number Phase 3 owes. Fail loudly with it rather than
        // passing quietly.
        assert!(collapsed_at.is_none(), "the residual stream collapsed: {collapsed_at:?}");
    }

    /// **Determinism holds at depth**, which is the property the whole class rests on: the same
    /// artifact and the same prompt produce the same argmax sequence, run after run.
    #[test]
    fn a_deep_converted_class_is_reproducible() {
        let mut shape = tiny_qwen_shape();
        shape.n_layers = 8;
        let blob = tiny_checkpoint(&shape);
        let plan = Qwen25ConvertPlan { shape, rms_norm_eps_bits: 1e-6f32.to_bits() };
        let a = convert_qwen25(&blob, &plan).unwrap();
        let prompt = [3usize, 11, 7, 1, 9];
        let first = measure_depth_health(&a, &prompt).unwrap();
        assert_eq!(measure_depth_health(&a, &prompt).unwrap(), first, "two runs at depth 8 agree exactly");
        // …and the model reads its input: every position's logit row is distinct. A run that was
        // merely "deterministic" while ignoring the prompt would satisfy the check above and mean
        // nothing. The ARGMAX is not the right witness for that here — under the fixture's random
        // weights it is pinned to whichever vocabulary row has the largest norm, whatever the
        // prompt, so a constant argmax is expected rather than a defect.
        let distinct: std::collections::BTreeSet<u64> = first.logits_digest.iter().copied().collect();
        assert_eq!(distinct.len(), prompt.len(), "each position produced a different logit row");
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

    /// **Audit H-05 / ADR-0050: what the narrowing could not give, the gain gives.**
    ///
    /// The measured failure on the real checkpoint was not that the calibration was wrong — it was
    /// that it had nothing left to spend. A requantization's gain is `multiplier / 2^shift` with
    /// the multiplier at most 1.0, so every setting ATTENUATES; the calibrated table came out
    /// `[1, 0, 1, 1, …]` with every decayed layer already at `shift = 0` and the residual peak
    /// still at 5 of 127. `Rescale` is the op that exists because "requantize cannot", and
    /// ADR-0050 put one at each residual site.
    ///
    /// This asserts the loop now spends it: a layer that asks for more bits than the shift floor
    /// has gets the remainder as amplification instead of having the request clamped away.
    #[test]
    fn a_decayed_layer_gets_amplification_the_narrowing_cannot_give() {
        use kaspa_consensus_core::palw_base0_ops::ScaleParams;

        let shape = tiny_qwen_shape();
        let blob = tiny_checkpoint(&shape);
        let plan = Qwen25ConvertPlan { shape, rms_norm_eps_bits: 1e-6f32.to_bits() };
        let mut base = convert_qwen25(&blob, &plan).expect("a well-formed checkpoint converts");
        assert!(base.layer_residual_scale.is_none(), "an uncalibrated artifact declares no gain, which is unity");
        assert_eq!(base.residual_scale_at(0, 0), Base0ArtifactV1::UNITY_SCALE);
        // A deliberately over-attenuated residual, which is the shape the real checkpoint measured:
        // the stream collapses, and the narrowing has nothing to give it back because `shift = 0`
        // is its floor. The fixture's own weights do not decay, so the condition has to be created
        // for the remedy to be observable at all.
        base.residual_requant.shift = 8;

        let calibrated = calibrate_layer_residuals(&base, &[1, 2, 3], 3).expect("the loop runs");
        let scales = calibrated.layer_residual_scale.as_ref().expect("calibration sets a table");
        assert_eq!(scales.len(), shape.n_layers, "one pair per layer, always — an omission would be silent");

        // Every gain is amplifying-or-unity and inside the cap: `UNITY_SHIFT - g` with `g` in
        // `0..=8`, so a gain can lift by at most 256× and can never attenuate. An amplifying
        // residual that overshoots is a different failure with the same symptom, which is what the
        // cap is for.
        for pair in scales {
            for site in pair {
                assert!(
                    site.shift <= ScaleParams::UNITY_SHIFT && site.shift >= ScaleParams::UNITY_SHIFT - 8,
                    "a residual gain must amplify within the cap, got shift {}",
                    site.shift
                );
                assert_eq!(site.multiplier, i32::MAX, "the gain is a power of two — the multiplier is unity");
            }
        }

        // The identity moves with it, because `artifact_root` covers the table: a class calibrated
        // on one prompt set is a different class from one calibrated on another, which is exactly
        // what makes calibration a registration input rather than a detail.
        assert_ne!(calibrated.artifact_digest(), base.artifact_digest());
        // And it still runs: an amplifying residual is arithmetic the engine performs, not a
        // parameter it ignores.
        let engine = crate::engine::Base0Engine::new(&calibrated);
        assert!(engine.generate(&[1, 2, 3], 2).is_ok(), "the calibrated class executes");
    }
}
