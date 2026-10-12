//! **The float reference the integer class is measured against.**
//!
//! Every quality number reported for the Qwen class before this file was a *proxy*: residual peak,
//! gate asymmetry, attention spread, railed-layer count. Proxies are what you measure when you
//! have nothing to compare against — and each one I built had to be corrected at least once,
//! because a proxy can only ever say "this looks structurally alive", never "this computes the
//! same function as the model it was converted from".
//!
//! This is the comparison. It runs Qwen2.5's architecture in `f32` straight out of the BF16
//! checkpoint — the arithmetic the model was trained in — and hands back logits. The int8
//! artifact's logits are then scored against it: top-1 agreement, top-5 containment, and rank
//! correlation over the vocabulary. Those are quality numbers. The proxies are not.
//!
//! **Nothing here is on the block-validation path, and nothing here may ever be.** It is float
//! arithmetic: its reduction order is its own, `-ffast-math` would change it, and two builds of it
//! are not required to agree. That is precisely why the *class* is integer arithmetic. This file
//! exists to answer "is the integer artifact a faithful quantisation", which is a question about
//! the conversion, not about consensus.

use crate::convert::{ConvertError, SafetensorsIndex, parse_safetensors_header, read_bf16_tensor};

/// The architecture constants the reference needs, read from `config.json` by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefConfigV1 {
    pub n_layers: usize,
    pub n_heads: usize,
    pub n_kv_heads: usize,
    pub d_head: usize,
    pub d_ff: usize,
    pub vocab: usize,
    pub rms_eps: f32,
    pub rope_theta: f32,
}

impl RefConfigV1 {
    pub fn hidden(&self) -> usize {
        self.n_heads * self.d_head
    }
    pub fn kv_dim(&self) -> usize {
        self.n_kv_heads * self.d_head
    }
}

/// `y = W x + b`, with `W` stored `[out, in]` row-major — HF's `nn.Linear` layout.
fn matvec(w: &[f32], x: &[f32], out_dim: usize, bias: Option<&[f32]>) -> Vec<f32> {
    let in_dim = x.len();
    (0..out_dim)
        .map(|o| {
            let row = &w[o * in_dim..(o + 1) * in_dim];
            let acc: f32 = row.iter().zip(x).map(|(a, b)| a * b).sum();
            acc + bias.map_or(0.0, |b| b[o])
        })
        .collect()
}

/// RMSNorm exactly as Qwen2 computes it: normalise in f32, then scale by the learned gain.
fn rms_norm(x: &[f32], gain: &[f32], eps: f32) -> Vec<f32> {
    let mean_square: f32 = x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32;
    let inv = 1.0 / (mean_square + eps).sqrt();
    x.iter().zip(gain).map(|(v, g)| v * inv * g).collect()
}

/// NEOX-style `rotate_half` RoPE, applied in place to one head's row.
///
/// The halves-pairing (`i` with `i + d/2`) rather than the adjacent-pairing (`2i` with `2i+1`) is
/// the one that matches `transformers`' Qwen2 implementation. Getting this wrong produces a model
/// that is subtly and uniformly wrong, which is exactly the failure a reference exists to catch.
fn rope_head(row: &mut [f32], position: usize, theta: f32) {
    let d = row.len();
    let half = d / 2;
    for i in 0..half {
        let freq = 1.0f32 / theta.powf(2.0 * i as f32 / d as f32);
        let angle = position as f32 * freq;
        let (sin, cos) = angle.sin_cos();
        let (a, b) = (row[i], row[i + half]);
        row[i] = a * cos - b * sin;
        row[i + half] = b * cos + a * sin;
    }
}

/// One layer's weights, held only while that layer runs.
struct LayerRef {
    in_ln: Vec<f32>,
    wq: Vec<f32>,
    bq: Vec<f32>,
    wk: Vec<f32>,
    bk: Vec<f32>,
    wv: Vec<f32>,
    bv: Vec<f32>,
    wo: Vec<f32>,
    post_ln: Vec<f32>,
    w_gate: Vec<f32>,
    w_up: Vec<f32>,
    w_down: Vec<f32>,
}

fn read_layer(blob: &[u8], index: &SafetensorsIndex, cfg: &RefConfigV1, li: usize) -> Result<LayerRef, ConvertError> {
    let d = cfg.hidden();
    let kv = cfg.kv_dim();
    let at = |t: &str| format!("model.layers.{li}.{t}");
    let r = |name: String, want: Vec<usize>| read_bf16_tensor(blob, index, &name, &want);
    let mut layer = LayerRef {
        in_ln: r(at("input_layernorm.weight"), vec![d])?,
        wq: r(at("self_attn.q_proj.weight"), vec![d, d])?,
        bq: r(at("self_attn.q_proj.bias"), vec![d])?,
        wk: r(at("self_attn.k_proj.weight"), vec![kv, d])?,
        bk: r(at("self_attn.k_proj.bias"), vec![kv])?,
        wv: r(at("self_attn.v_proj.weight"), vec![kv, d])?,
        bv: r(at("self_attn.v_proj.bias"), vec![kv])?,
        wo: r(at("self_attn.o_proj.weight"), vec![d, d])?,
        post_ln: r(at("post_attention_layernorm.weight"), vec![d])?,
        w_gate: r(at("mlp.gate_proj.weight"), vec![cfg.d_ff, d])?,
        w_up: r(at("mlp.up_proj.weight"), vec![cfg.d_ff, d])?,
        w_down: r(at("mlp.down_proj.weight"), vec![d, cfg.d_ff])?,
    };
    if let Some(levels) = fake_weight_levels() {
        fake_quant_rows(&mut layer.wq, d, levels);
        fake_quant_rows(&mut layer.wk, kv, levels);
        fake_quant_rows(&mut layer.wv, kv, levels);
        fake_quant_rows(&mut layer.wo, d, levels);
        fake_quant_rows(&mut layer.w_gate, cfg.d_ff, levels);
        fake_quant_rows(&mut layer.w_up, cfg.d_ff, levels);
        fake_quant_rows(&mut layer.w_down, d, levels);
    }
    Ok(layer)
}

/// The activation ranges one float pass produced — the measurements static quantisation is made
/// of. Every field is an absmax: exact under any summation order, because a max has no order.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerStatsV1 {
    /// **Every absmax here is over positions ≥ 1.** Position 0 is the attention-sink token:
    /// measured on Qwen2.5-1.5B its residual stream runs at |h| ≈ 3,000–7,000 while every later
    /// position sits at 5–213 — the massive-activation pattern. A scale sized to hold the sink
    /// wastes 5+ bits at every position that carries the actual computation; sized to the rest,
    /// the sink's row CLAMPS, which costs it magnitude it does not use (its role is to be found
    /// by attention, and its k/v come from the magnitude-free NORMED row).
    ///
    /// Absmax of the UNIT-RMS normed row (gain excluded — the integer norm's output), per site:
    /// the attention norm, then the FFN norm.
    pub norm_absmax: [f64; 2],
    /// Absmax of the biased q, over both the pre-rotation and post-rotation rows: the integer
    /// codes hold both, one before `RopeTable` and one after, at the same scale.
    pub q_absmax: f64,
    pub k_absmax: f64,
    /// Absmax of the biased v (no rotation applies to it).
    pub v_absmax: f64,
    /// Absmax of the probability-weighted value sum, before the output projection.
    pub attn_absmax: f64,
    /// Absmax of the residual stream after each site's add.
    pub h_absmax: [f64; 2],
    /// Absmax of the DELTA each site adds — the o_proj output, then the down-projection output.
    /// Sized separately from the stream because the add needs a scale BOTH operands fit: at layer
    /// 0 the incoming stream is the embedding (absmax ~0.1) and the attention delta is ~60× that,
    /// so a delta forced to the stream's scale clamps at a tenth of its real range — measured,
    /// that kills the stream by layer 2.
    pub delta_absmax: [f64; 2],
    /// Absmax of the gate PRE-activation — the value that must fit Qk's ±128 for `Silu`'s input.
    pub gate_absmax: f64,
    /// Absmax of the SiLU output — the value the fixed `QK_TO_CODE` narrowing saturated at ±1.
    pub silu_absmax: f64,
    pub up_absmax: f64,
    /// Absmax of the gate×up product.
    pub gated_absmax: f64,
    /// Position 0's own numbers — the SINK LANE's calibration column. Measured consequence of
    /// sizing everything on generic positions alone: the sink's stream clamps, its normed row's
    /// direction breaks, every later position's attention reads the broken k/v, and the whole
    /// pass collapses (44/57 → 3/57 in the float fake-quant simulation, from the clamp alone).
    pub sink_delta_absmax: [f64; 2],
    /// Position 0's stream absmax after each site's add.
    pub sink_stream_absmax: [f64; 2],
    /// Position 0's `[silu, up, gated]` absmaxes — the sink FORMS in an FFN (layer 1's gated
    /// product runs to thousands), so the lane must let it form.
    pub sink_ffn_absmax: [f64; 3],
    /// Per site, per channel: the MEAN of the residual stream over generic positions — the
    /// static bias `B` the integer stream carries separately. The heavy channels run at 5–6×
    /// their own standard deviation, so subtracting the mean shrinks the stream's range by two
    /// to three bits for every ordinary channel.
    pub h_channel_mean: [Vec<f64>; 2],
    /// Per site: absmax of `h − B` over generic positions — what actually sizes the stream.
    pub h_centered_absmax: [f64; 2],
    /// Per-CHANNEL absmax of the rotated q (HF layout; the converter permutes). Kept per channel
    /// because one k channel at 318 against ordinary channels at 5 is what turned five of twelve
    /// layer-0 heads exactly uniform under a per-tensor scale.
    pub q_channel_absmax: Vec<f64>,
    /// Per-channel absmax of the rotated k.
    pub k_channel_absmax: Vec<f64>,
    /// Per-channel SUM OF SQUARES (and the sample count) for q and k, pre+post rotation pooled.
    /// An absmax is one outlier's number; a scale sized at `min(absmax, 4·rms)` clips that one
    /// sample and returns the bits to every other one.
    pub q_channel_sumsq: Vec<f64>,
    pub k_channel_sumsq: Vec<f64>,
    pub qk_samples: usize,
    /// Per head: absmax of the scaled attention logits (`q·k/√d`), generic positions.
    pub logit_absmax: Vec<f64>,
    /// Per-channel absmax of the UNIT-RMS normed row, per site — the outlier map that decides how
    /// much of each channel's magnitude migrates into the weight columns (smoothing).
    pub norm_channel_absmax: [Vec<f64>; 2],
    /// Per-channel absmax of the SiLU output, the up row and the gate×up product. The FFN's
    /// internal rows carry the same outlier disease as the normed rows — measured at layer 1 the
    /// product's absmax is hundreds of times its median channel — and a uniform scale erases the
    /// ordinary channels of the down-projection's input.
    pub silu_channel_absmax: Vec<f64>,
    pub up_channel_absmax: Vec<f64>,
    pub gated_channel_absmax: Vec<f64>,
}

/// Whole-pass calibration statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibStatsV1 {
    pub per_layer: Vec<LayerStatsV1>,
    /// Absmax of the UNIT-RMS final-norm row.
    pub final_norm_absmax: f64,
    /// Its per-channel map, for the unembedding's smoothing.
    pub final_norm_channel_absmax: Vec<f64>,
    /// Absmax of the final logits over generic positions — what sizes the committed i16 logit
    /// codes (the A16 output row rides 4-byte lanes, so the class output is DEFINED over the
    /// narrowed codes; a clamp here would tie argmaxes, so the scale must hold the range).
    pub final_logit_absmax: f64,
}

fn absmax_of(row: &[f32]) -> f64 {
    row.iter().fold(0f64, |acc, v| acc.max((*v as f64).abs()))
}

/// **Fake quantization, for the one question the integer engine cannot answer about itself.**
///
/// Rounds a row to `levels` uniform steps of its own absmax — activations only, everything else
/// exact float — so a pass under `PALW_QWEN_FAKE_ACT_BITS=8` isolates "what does int8 activation
/// rounding alone cost this architecture", with weights, accumulation and nonlinearities exact.
/// If THIS reproduces the integer engine's depth collapse, the ceiling is the activation width
/// and no calibration of the int8 pipeline can move it; if 15 bits restores parity, the width
/// that fixes it is measured, not guessed.
fn fake_quant(row: &mut [f32], levels: f32) {
    let absmax = row.iter().fold(0f32, |a, v| a.max(v.abs()));
    if absmax == 0.0 {
        return;
    }
    let step = absmax / levels;
    for v in row.iter_mut() {
        *v = (*v / step).round() * step;
    }
}

fn fake_act_levels() -> Option<f32> {
    std::env::var("PALW_QWEN_FAKE_ACT_BITS").ok().and_then(|v| v.parse::<u32>().ok()).map(|bits| ((1u32 << (bits - 1)) - 1) as f32)
}

/// The weight-side twin: per-ROW fake quantization of a `[out][in]` matrix, matching how the
/// integer artifact quantizes weights. `PALW_QWEN_FAKE_WEIGHT_BITS=8` isolates what int8 weights
/// alone cost, and combined with the activation knob it simulates the full W8A8 regime in float —
/// everything the integer ENGINE adds beyond that (op approximations, scale plumbing) is then the
/// difference between this and the real engine's score.
fn fake_weight_levels() -> Option<f32> {
    std::env::var("PALW_QWEN_FAKE_WEIGHT_BITS").ok().and_then(|v| v.parse::<u32>().ok()).map(|bits| ((1u32 << (bits - 1)) - 1) as f32)
}

fn fake_quant_rows(w: &mut [f32], out_dim: usize, levels: f32) {
    let in_dim = w.len() / out_dim.max(1);
    for r in 0..out_dim {
        fake_quant(&mut w[r * in_dim..(r + 1) * in_dim], levels);
    }
}

/// Run the prompt through Qwen2.5 in `f32` and return the logits at every position.
///
/// A prefill, not a decode loop: every position is carried through each layer together, which is
/// what lets one pass over a layer's weights serve the whole prompt. Weights are read per layer
/// and dropped after it, so peak memory is one layer rather than the whole checkpoint.
pub fn reference_forward(blob: &[u8], cfg: &RefConfigV1, prompt: &[usize]) -> Result<Vec<Vec<f32>>, ConvertError> {
    reference_forward_full(blob, cfg, prompt).map(|(logits, _, _)| logits)
}

/// As [`reference_forward`], and also the residual stream after every layer at every position —
/// the material a layer-by-layer comparison against the integer engine needs.
#[allow(clippy::type_complexity)]
pub fn reference_forward_probed(
    blob: &[u8],
    cfg: &RefConfigV1,
    prompt: &[usize],
) -> Result<(Vec<Vec<f32>>, Vec<Vec<Vec<f32>>>), ConvertError> {
    reference_forward_full(blob, cfg, prompt).map(|(logits, probe, _)| (logits, probe.streams))
}

/// The reference's intermediates, for locating WHERE the integer pass diverges: after each whole
/// layer, after each layer's attention half, and the (gained) normed row each layer's projections
/// read. All indexed `[layer][position]`.
#[derive(Debug, Clone, Default)]
pub struct RefProbeV1 {
    pub streams: Vec<Vec<Vec<f32>>>,
    pub mid_streams: Vec<Vec<Vec<f32>>>,
    pub normed_rows: Vec<Vec<Vec<f32>>>,
    /// Layer 0 only, `[position][head][key]` / `[position][d]`: the attention internals.
    pub l0_probs: Vec<Vec<Vec<f32>>>,
    /// `[position][head][key]`: the scaled logits (`q·k/√d`) the softmax saw.
    pub l0_logits: Vec<Vec<Vec<f32>>>,
    pub l0_attn: Vec<Vec<f32>>,
    pub l0_delta: Vec<Vec<f32>>,
}

/// The full pass: logits, per-layer residual streams, and the calibration statistics.
#[allow(clippy::type_complexity)]
pub fn reference_forward_full(
    blob: &[u8],
    cfg: &RefConfigV1,
    prompt: &[usize],
) -> Result<(Vec<Vec<f32>>, RefProbeV1, CalibStatsV1), ConvertError> {
    if prompt.is_empty() {
        return Err(ConvertError::BadContainer("the reference needs at least one token"));
    }
    let index = parse_safetensors_header(blob)?;
    let d = cfg.hidden();
    let t = prompt.len();

    // The embedding gather. Only the prompt's rows are needed, so the [vocab, hidden] tensor is
    // read once here and dropped — it is also the unembedding (weights are tied), which is read
    // again, streamed, at the end.
    let embed = read_bf16_tensor(blob, &index, "model.embed_tokens.weight", &[cfg.vocab, d])?;
    let mut x: Vec<Vec<f32>> = prompt
        .iter()
        .map(|tok| {
            if *tok >= cfg.vocab {
                return Err(ConvertError::BadContainer("a prompt token is outside the vocabulary"));
            }
            Ok(embed[tok * d..(tok + 1) * d].to_vec())
        })
        .collect::<Result<_, _>>()?;
    drop(embed);

    let group = cfg.n_heads / cfg.n_kv_heads.max(1);
    let scale = 1.0f32 / (cfg.d_head as f32).sqrt();

    let mut probe = RefProbeV1::default();
    // Which layer the detailed (l0_*) probes record — mirrors the engine's measurement knob.
    let probe_layer: usize = std::env::var("PALW_QWEN_PROBE_LAYER").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut stats: Vec<LayerStatsV1> = Vec::with_capacity(cfg.n_layers);
    let ones = vec![1f32; d];
    for li in 0..cfg.n_layers {
        let l = read_layer(blob, &index, cfg, li)?;
        let mut st = LayerStatsV1 {
            norm_absmax: [0.0; 2],
            q_absmax: 0.0,
            k_absmax: 0.0,
            v_absmax: 0.0,
            attn_absmax: 0.0,
            h_absmax: [0.0; 2],
            delta_absmax: [0.0; 2],
            sink_delta_absmax: [0.0; 2],
            sink_stream_absmax: [0.0; 2],
            sink_ffn_absmax: [0.0; 3],
            h_channel_mean: [vec![0.0; d], vec![0.0; d]],
            h_centered_absmax: [0.0; 2],
            gate_absmax: 0.0,
            silu_absmax: 0.0,
            up_absmax: 0.0,
            gated_absmax: 0.0,
            q_channel_absmax: vec![0.0; d],
            k_channel_absmax: vec![0.0; cfg.kv_dim()],
            q_channel_sumsq: vec![0.0; d],
            k_channel_sumsq: vec![0.0; cfg.kv_dim()],
            qk_samples: 0,
            logit_absmax: vec![0.0; cfg.n_heads],
            norm_channel_absmax: [vec![0.0; d], vec![0.0; d]],
            silu_channel_absmax: vec![0.0; cfg.d_ff],
            up_channel_absmax: vec![0.0; cfg.d_ff],
            gated_channel_absmax: vec![0.0; cfg.d_ff],
        };

        // ---- attention ----------------------------------------------------------------------
        let mut keys: Vec<Vec<f32>> = Vec::with_capacity(t);
        let mut values: Vec<Vec<f32>> = Vec::with_capacity(t);
        let mut queries: Vec<Vec<f32>> = Vec::with_capacity(t);
        let mut normed_rows: Vec<Vec<f32>> = Vec::with_capacity(t);
        for (pos, xi) in x.iter().enumerate() {
            // The unit-RMS row FIRST, gain after: the integer engine's norm outputs the unit row
            // (the gain is folded into the consuming projections), so the unit row's range is the
            // one its requantization must be sized to.
            let unit = rms_norm(xi, &ones, cfg.rms_eps);
            let generic = pos > 0 || t == 1 || std::env::var_os("PALW_QWEN_STATS_INCLUDE_SINK").is_some();
            let mut h: Vec<f32> = unit.iter().zip(&l.in_ln).map(|(v, g)| v * g).collect();
            if let Some(levels) = fake_act_levels() {
                fake_quant(&mut h, levels);
            }
            normed_rows.push(h.clone());
            // Measured on the GAINED row: the gain lives in the norm's own per-channel requant
            // (not in the weight columns — folded there, a small-γ channel's column quantizes to
            // zero and the channel vanishes from the projection), so the gained row is the one
            // whose range the requant must be sized to.
            if generic {
                st.norm_absmax[0] = st.norm_absmax[0].max(absmax_of(&h));
                for (c, v) in h.iter().enumerate() {
                    st.norm_channel_absmax[0][c] = st.norm_channel_absmax[0][c].max((*v as f64).abs());
                }
            }
            let mut q = matvec(&l.wq, &h, d, Some(&l.bq));
            let mut k = matvec(&l.wk, &h, cfg.kv_dim(), Some(&l.bk));
            let mut v = matvec(&l.wv, &h, cfg.kv_dim(), Some(&l.bv));
            if let Some(levels) = fake_act_levels() {
                fake_quant(&mut v, levels);
            }
            if generic {
                st.q_absmax = st.q_absmax.max(absmax_of(&q));
                st.k_absmax = st.k_absmax.max(absmax_of(&k));
                st.v_absmax = st.v_absmax.max(absmax_of(&v));
            }
            for head in 0..cfg.n_heads {
                rope_head(&mut q[head * cfg.d_head..(head + 1) * cfg.d_head], pos, cfg.rope_theta);
            }
            for head in 0..cfg.n_kv_heads {
                rope_head(&mut k[head * cfg.d_head..(head + 1) * cfg.d_head], pos, cfg.rope_theta);
            }
            if generic {
                st.q_absmax = st.q_absmax.max(absmax_of(&q));
                st.k_absmax = st.k_absmax.max(absmax_of(&k));
                st.qk_samples += 1;
                for (c, v) in q.iter().enumerate() {
                    st.q_channel_absmax[c] = st.q_channel_absmax[c].max((*v as f64).abs());
                    st.q_channel_sumsq[c] += (*v as f64) * (*v as f64);
                }
                for (c, v) in k.iter().enumerate() {
                    st.k_channel_absmax[c] = st.k_channel_absmax[c].max((*v as f64).abs());
                    st.k_channel_sumsq[c] += (*v as f64) * (*v as f64);
                }
            }
            if let Some(levels) = fake_act_levels() {
                fake_quant(&mut q, levels);
                fake_quant(&mut k, levels);
            }
            queries.push(q);
            keys.push(k);
            values.push(v);
        }

        for pos in 0..t {
            let mut attn = vec![0f32; d];
            let mut pos_probs: Vec<Vec<f32>> = Vec::new();
            for head in 0..cfg.n_heads {
                let off = head * cfg.d_head;
                let kv_off = (head / group) * cfg.d_head;
                let qh = &queries[pos][off..off + cfg.d_head];
                // Causal: position `pos` sees keys 0..=pos and nothing after.
                let logits: Vec<f32> = (0..=pos)
                    .map(|j| {
                        let kh = &keys[j][kv_off..kv_off + cfg.d_head];
                        qh.iter().zip(kh).map(|(a, b)| a * b).sum::<f32>() * scale
                    })
                    .collect();
                if li == probe_layer {
                    if probe.l0_logits.len() <= pos {
                        probe.l0_logits.push(Vec::new());
                    }
                    probe.l0_logits[pos].push(logits.clone());
                }
                if pos > 0 || t == 1 || std::env::var_os("PALW_QWEN_STATS_INCLUDE_SINK").is_some() {
                    for v in &logits {
                        st.logit_absmax[head] = st.logit_absmax[head].max((*v as f64).abs());
                    }
                }
                let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                let exps: Vec<f32> = logits.iter().map(|v| (v - max).exp()).collect();
                let sum: f32 = exps.iter().sum();
                let mut head_probs = Vec::with_capacity(exps.len());
                for (j, e) in exps.iter().enumerate() {
                    let p = e / sum;
                    head_probs.push(p);
                    let vh = &values[j][kv_off..kv_off + cfg.d_head];
                    for i in 0..cfg.d_head {
                        attn[off + i] += p * vh[i];
                    }
                }
                pos_probs.push(head_probs);
            }
            if let Some(levels) = fake_act_levels() {
                fake_quant(&mut attn, levels);
            }
            if li == probe_layer {
                probe.l0_probs.push(pos_probs);
                probe.l0_attn.push(attn.clone());
            }
            let projected = matvec(&l.wo, &attn, d, None);
            if li == probe_layer {
                probe.l0_delta.push(projected.clone());
            }
            if pos > 0 || t == 1 || std::env::var_os("PALW_QWEN_STATS_INCLUDE_SINK").is_some() {
                st.attn_absmax = st.attn_absmax.max(absmax_of(&attn));
                st.delta_absmax[0] = st.delta_absmax[0].max(absmax_of(&projected));
            } else {
                st.sink_delta_absmax[0] = st.sink_delta_absmax[0].max(absmax_of(&projected));
            }
            for i in 0..d {
                x[pos][i] += projected[i];
            }
            if let Some(levels) = fake_act_levels() {
                fake_quant(&mut x[pos], levels);
            }
        }
        // MEASUREMENT: what the integer engine's STATIC stream scale does to the sink — position
        // 0 clamps to the generic positions' range. If the fake-quant score collapses under this
        // alone, the sink clamp is the engine's missing forty tokens.
        if std::env::var_os("PALW_QWEN_FAKE_SINK_CLAMP").is_some() && t > 1 {
            let generic_absmax = x.iter().skip(1).fold(0f32, |a, row| a.max(row.iter().fold(0f32, |b, v| b.max(v.abs()))));
            for v in x[0].iter_mut() {
                *v = v.clamp(-generic_absmax, generic_absmax);
            }
        }
        let generic_n = if t == 1 { 1 } else { t - 1 } as f64;
        let include_sink = std::env::var_os("PALW_QWEN_STATS_INCLUDE_SINK").is_some();
        for (pos, xi) in x.iter().enumerate() {
            if pos == 0 && t > 1 {
                st.sink_stream_absmax[0] = st.sink_stream_absmax[0].max(absmax_of(xi));
            }
            if pos > 0 || t == 1 || include_sink {
                st.h_absmax[0] = st.h_absmax[0].max(absmax_of(xi));
            }
            if pos > 0 || t == 1 {
                for (c, v) in xi.iter().enumerate() {
                    st.h_channel_mean[0][c] += *v as f64 / generic_n;
                }
            }
        }
        for (pos, xi) in x.iter().enumerate() {
            if pos > 0 || t == 1 || include_sink {
                for (c, v) in xi.iter().enumerate() {
                    st.h_centered_absmax[0] = st.h_centered_absmax[0].max((*v as f64 - st.h_channel_mean[0][c]).abs());
                }
            }
        }
        probe.mid_streams.push(x.clone());
        probe.normed_rows.push(normed_rows);

        // ---- SwiGLU feed-forward ------------------------------------------------------------
        for (pos, xi) in x.iter_mut().enumerate() {
            let generic = pos > 0 || t == 1 || std::env::var_os("PALW_QWEN_STATS_INCLUDE_SINK").is_some();
            let unit = rms_norm(xi, &ones, cfg.rms_eps);
            let mut h: Vec<f32> = unit.iter().zip(&l.post_ln).map(|(v, g)| v * g).collect();
            if let Some(levels) = fake_act_levels() {
                fake_quant(&mut h, levels);
            }
            if generic {
                for (c, v) in h.iter().enumerate() {
                    st.norm_channel_absmax[1][c] = st.norm_channel_absmax[1][c].max((*v as f64).abs());
                }
            }
            let gate = matvec(&l.w_gate, &h, cfg.d_ff, None);
            let mut silu: Vec<f32> = gate.iter().map(|g| g / (1.0 + (-g).exp())).collect();
            let mut up = matvec(&l.w_up, &h, cfg.d_ff, None);
            if let Some(levels) = fake_act_levels() {
                fake_quant(&mut silu, levels);
                fake_quant(&mut up, levels);
            }
            let mut act: Vec<f32> = silu.iter().zip(&up).map(|(s, u)| s * u).collect();
            if let Some(levels) = fake_act_levels() {
                fake_quant(&mut act, levels);
            }
            let down = matvec(&l.w_down, &act, d, None);
            if generic {
                st.norm_absmax[1] = st.norm_absmax[1].max(absmax_of(&h));
                st.gate_absmax = st.gate_absmax.max(absmax_of(&gate));
                st.silu_absmax = st.silu_absmax.max(absmax_of(&silu));
                st.up_absmax = st.up_absmax.max(absmax_of(&up));
                st.gated_absmax = st.gated_absmax.max(absmax_of(&act));
                st.delta_absmax[1] = st.delta_absmax[1].max(absmax_of(&down));
                for c in 0..cfg.d_ff {
                    st.silu_channel_absmax[c] = st.silu_channel_absmax[c].max((silu[c] as f64).abs());
                    st.up_channel_absmax[c] = st.up_channel_absmax[c].max((up[c] as f64).abs());
                    st.gated_channel_absmax[c] = st.gated_channel_absmax[c].max((act[c] as f64).abs());
                }
            } else {
                st.sink_delta_absmax[1] = st.sink_delta_absmax[1].max(absmax_of(&down));
                st.sink_ffn_absmax[0] = st.sink_ffn_absmax[0].max(absmax_of(&silu));
                st.sink_ffn_absmax[1] = st.sink_ffn_absmax[1].max(absmax_of(&up));
                st.sink_ffn_absmax[2] = st.sink_ffn_absmax[2].max(absmax_of(&act));
            }
            for i in 0..d {
                xi[i] += down[i];
            }
            if let Some(levels) = fake_act_levels() {
                fake_quant(xi, levels);
            }
        }
        if std::env::var_os("PALW_QWEN_FAKE_SINK_CLAMP").is_some() && t > 1 {
            let generic_absmax = x.iter().skip(1).fold(0f32, |a, row| a.max(row.iter().fold(0f32, |b, v| b.max(v.abs()))));
            for v in x[0].iter_mut() {
                *v = v.clamp(-generic_absmax, generic_absmax);
            }
        }
        for (pos, xi) in x.iter().enumerate() {
            if pos == 0 && t > 1 {
                st.sink_stream_absmax[1] = st.sink_stream_absmax[1].max(absmax_of(xi));
            }
            if pos > 0 || t == 1 || include_sink {
                st.h_absmax[1] = st.h_absmax[1].max(absmax_of(xi));
            }
            if pos > 0 || t == 1 {
                for (c, v) in xi.iter().enumerate() {
                    st.h_channel_mean[1][c] += *v as f64 / generic_n;
                }
            }
        }
        for (pos, xi) in x.iter().enumerate() {
            if pos > 0 || t == 1 || include_sink {
                for (c, v) in xi.iter().enumerate() {
                    st.h_centered_absmax[1] = st.h_centered_absmax[1].max((*v as f64 - st.h_channel_mean[1][c]).abs());
                }
            }
        }
        probe.streams.push(x.clone());
        stats.push(st);
    }

    let final_gain = read_bf16_tensor(blob, &index, "model.norm.weight", &[d])?;
    let mut final_norm_absmax = 0f64;
    let mut final_norm_channel_absmax = vec![0f64; d];
    for (pos, xi) in x.iter().enumerate() {
        if pos > 0 || t == 1 || std::env::var_os("PALW_QWEN_STATS_INCLUDE_SINK").is_some() {
            let unit = rms_norm(xi, &ones, cfg.rms_eps);
            let gained: Vec<f32> = unit.iter().zip(&final_gain).map(|(v, g)| v * g).collect();
            final_norm_absmax = final_norm_absmax.max(absmax_of(&gained));
            for (c, v) in gained.iter().enumerate() {
                final_norm_channel_absmax[c] = final_norm_channel_absmax[c].max((*v as f64).abs());
            }
        }
    }
    let normed: Vec<Vec<f32>> = x.iter().map(|xi| rms_norm(xi, &final_gain, cfg.rms_eps)).collect();

    // The unembedding is the tied embedding matrix, streamed straight from the BF16 blob so a
    // 151,936 x 1,536 f32 copy never exists.
    let span = index
        .tensors
        .get("model.embed_tokens.weight")
        .ok_or_else(|| ConvertError::MissingTensor("model.embed_tokens.weight".into()))?;
    let begin = index.data_offset + span.begin;
    let bytes = &blob[begin..index.data_offset + span.end];
    let mut out = vec![vec![0f32; cfg.vocab]; t];
    for token in 0..cfg.vocab {
        let row = &bytes[token * d * 2..(token + 1) * d * 2];
        let w: Vec<f32> = row.chunks_exact(2).map(|c| f32::from_bits(u32::from(u16::from_le_bytes([c[0], c[1]])) << 16)).collect();
        for (pos, n) in normed.iter().enumerate() {
            out[pos][token] = w.iter().zip(n).map(|(a, b)| a * b).sum();
        }
    }
    let mut final_logit_absmax = 0f64;
    for (pos, row) in out.iter().enumerate() {
        if pos > 0 || t == 1 {
            final_logit_absmax = final_logit_absmax.max(absmax_of(row));
        }
    }
    Ok((out, probe, CalibStatsV1 { per_layer: stats, final_norm_absmax, final_norm_channel_absmax, final_logit_absmax }))
}

/// Cosine similarity between a float row and an int8 row.
///
/// Scale-invariant by construction, which is the point: the integer stream lives at whatever code
/// scale calibration gave it, and the question is whether it points the same way, not whether it
/// is the same size.
pub fn cosine_i8(reference: &[f32], integer: &[i8]) -> f64 {
    if reference.len() != integer.len() || reference.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0f64, 0f64, 0f64);
    for (a, b) in reference.iter().zip(integer) {
        let (a, b) = (*a as f64, *b as f64);
        dot += a * b;
        na += a * a;
        nb += b * b;
    }
    if na <= 0.0 || nb <= 0.0 { 0.0 } else { dot / (na.sqrt() * nb.sqrt()) }
}

/// How faithfully an integer artifact reproduces the float model, on one prompt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FidelityV1 {
    /// Positions where the int8 artifact's argmax equals the reference's argmax.
    pub top1_agree: usize,
    /// Positions where the reference's argmax is anywhere in the artifact's top 5.
    pub top5_contains: usize,
    /// Positions scored.
    pub positions: usize,
    /// Spearman rank correlation over the reference's top 100 tokens, averaged over positions.
    /// Top-1 alone is a coin flip on a flat distribution; this says whether the whole head of the
    /// distribution survived quantisation or only its winner did.
    pub top100_rank_correlation: f64,
}

impl FidelityV1 {
    /// The bar for a class that carries weight. Chosen, not measured: a quantisation that loses
    /// the argmax at a quarter of its positions is not the model it claims to be.
    pub fn is_faithful(&self) -> bool {
        self.positions > 0 && self.top1_agree * 4 >= self.positions * 3 && self.top100_rank_correlation >= 0.5
    }
}

fn top_k(row: &[f32], k: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..row.len()).collect();
    idx.sort_by(|a, b| row[*b].partial_cmp(&row[*a]).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(b)));
    idx.truncate(k);
    idx
}

fn top_k_i32(row: &[i32], k: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..row.len()).collect();
    idx.sort_by(|a, b| row[*b].cmp(&row[*a]).then(a.cmp(b)));
    idx.truncate(k);
    idx
}

/// Score integer logits against reference logits, position by position.
pub fn score_fidelity(reference: &[Vec<f32>], integer: &[Vec<i32>]) -> FidelityV1 {
    let positions = reference.len().min(integer.len());
    let (mut top1, mut top5, mut rho_sum) = (0usize, 0usize, 0f64);
    for pos in 0..positions {
        let (r, q) = (&reference[pos], &integer[pos]);
        let r_top = top_k(r, 100);
        let q_top5 = top_k_i32(q, 5);
        if !r_top.is_empty() && q_top5.first() == r_top.first() {
            top1 += 1;
        }
        if !r_top.is_empty() && q_top5.contains(&r_top[0]) {
            top5 += 1;
        }
        // Spearman over the reference's top 100: rank each of those tokens under the integer
        // model, then correlate with 0..100. `d^2` form, ties broken by index in `top_k_i32`.
        let n = r_top.len();
        if n > 1 {
            let q_rank_of: std::collections::BTreeMap<usize, usize> =
                top_k_i32(q, q.len()).into_iter().enumerate().map(|(rank, tok)| (tok, rank)).collect();
            let mut sub: Vec<(usize, usize)> =
                r_top.iter().enumerate().map(|(i, tok)| (i, *q_rank_of.get(tok).unwrap_or(&q.len()))).collect();
            sub.sort_by_key(|(_, qr)| *qr);
            let d2: f64 = sub
                .iter()
                .enumerate()
                .map(|(within, (ref_rank, _))| {
                    let d = within as f64 - *ref_rank as f64;
                    d * d
                })
                .sum();
            let nf = n as f64;
            rho_sum += 1.0 - 6.0 * d2 / (nf * (nf * nf - 1.0));
        }
    }
    FidelityV1 {
        top1_agree: top1,
        top5_contains: top5,
        positions,
        top100_rank_correlation: if positions > 0 { rho_sum / positions as f64 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scorer that cannot tell agreement from disagreement measures nothing. Identical rows must
    /// score perfectly; a reversed ranking must not.
    #[test]
    fn the_scorer_separates_agreement_from_disagreement() {
        let reference: Vec<Vec<f32>> = vec![(0..200).map(|i| i as f32).collect()];
        let same: Vec<Vec<i32>> = vec![(0..200).collect()];
        let scored = score_fidelity(&reference, &same);
        assert_eq!(scored.top1_agree, 1);
        assert!(scored.top100_rank_correlation > 0.99, "identical rankings, got {scored:?}");
        assert!(scored.is_faithful());

        let reversed: Vec<Vec<i32>> = vec![(0..200).map(|i: i32| -i).collect()];
        let scored = score_fidelity(&reference, &reversed);
        assert_eq!(scored.top1_agree, 0);
        assert!(scored.top100_rank_correlation < 0.0, "reversed rankings, got {scored:?}");
        assert!(!scored.is_faithful());
    }

    /// RoPE at position zero is the identity — every angle is zero — and at any other position it
    /// is not. If the halves-pairing were mis-wired this would still hold, so it is a floor, not a
    /// proof; the proof is the top-1 agreement against the real checkpoint.
    #[test]
    fn rope_is_the_identity_only_at_position_zero() {
        let base: Vec<f32> = (0..128).map(|i| (i as f32) * 0.01 - 0.5).collect();
        let mut at0 = base.clone();
        rope_head(&mut at0, 0, 1e6);
        assert_eq!(at0, base);
        let mut at3 = base.clone();
        rope_head(&mut at3, 3, 1e6);
        assert_ne!(at3, base);
    }

    /// RMSNorm's output has unit RMS before the gain is applied — the property the integer
    /// `norm_to_code` is built to mirror.
    #[test]
    fn rms_norm_normalises() {
        let x: Vec<f32> = (1..=64).map(|i| i as f32).collect();
        let ones = vec![1f32; 64];
        let y = rms_norm(&x, &ones, 0.0);
        let ms: f32 = y.iter().map(|v| v * v).sum::<f32>() / 64.0;
        assert!((ms - 1.0).abs() < 1e-4, "root-mean-square {ms}");
    }
}
