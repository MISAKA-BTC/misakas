//! **The float reference for Qwen3.6's hybrid graph, and the ranges a converter calibrates from.**
//!
//! The integer class is a quantization of *this*. Two things need it and neither can be had
//! without it:
//!
//! * **Fidelity.** "The graph executes" and "the graph computes the model" are different claims,
//!   and only a comparison against the unquantized computation separates them. The converter's
//!   scales today are derived from each site's fan-in — the right shape, not a measurement — so
//!   the class runs and its output is not claimed to be faithful.
//! * **Calibration.** A quantization scale is a statement about a range, and a range has to be
//!   observed. Every A16 triple the converter writes should come from the magnitudes this pass
//!   measures at that site.
//!
//! # Streaming, because 35 B parameters in `f32` is 140 GiB
//!
//! The reference never holds the model. Weights arrive through [`TensorSource`] one tensor at a
//! time and are dropped when the layer that needed them is done; the activations are two thousand
//! floats. Peak memory is the largest single tensor, which is `ffn_down_exps` at 1.07 GiB.
//!
//! That also makes it composable with conversion: the converter already reads every tensor once,
//! in forward order, so a calibrating converter is this pass and that one sharing a read.
//!
//! # Offline and float on purpose
//!
//! ADR-0040 pins the class's scales at REGISTRATION. What measures a checkpoint may use float and
//! what executes one may not. Nothing here is on the block-validation path.

use crate::qwen36::{Qwen36LayerKind, Qwen36ShapeV1};
use std::collections::BTreeMap;

/// Where the reference gets a tensor. Returns row-major `f32` in the GGUF's own layout.
pub trait TensorSource {
    fn tensor(&mut self, name: &str) -> Result<Vec<f32>, String>;
}

/// A source backed by a map, for tests and for a caller that already has the weights.
pub struct MapSource(pub BTreeMap<String, Vec<f32>>);

impl TensorSource for MapSource {
    fn tensor(&mut self, name: &str) -> Result<Vec<f32>, String> {
        self.0.get(name).cloned().ok_or_else(|| format!("no tensor {name}"))
    }
}

/// The magnitudes one site reached, over a whole reference run.
///
/// `absmax` is what a symmetric scale is chosen from; `rms` is kept because a site whose absmax is
/// an outlier and whose rms is small is a site where a scale chosen from the absmax throws away
/// most of the range — the observation that drives whether a site needs its own treatment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteRangeV1 {
    pub absmax: f64,
    pub sum_squares: f64,
    pub count: u64,
}

impl SiteRangeV1 {
    fn observe(&mut self, values: &[f32]) {
        for v in values {
            let x = *v as f64;
            self.absmax = self.absmax.max(x.abs());
            self.sum_squares += x * x;
        }
        self.count += values.len() as u64;
    }
    pub fn rms(&self) -> f64 {
        if self.count == 0 { 0.0 } else { (self.sum_squares / self.count as f64).sqrt() }
    }
    /// How far the peak is above the typical magnitude. A ratio in the tens is an outlier channel,
    /// and a single symmetric scale gives the ordinary channels `log2(ratio)` fewer bits.
    pub fn crest(&self) -> f64 {
        let rms = self.rms();
        if rms > 0.0 { self.absmax / rms } else { 0.0 }
    }
}

/// Every site's ranges, keyed the way the artifact's parameter store is.
#[derive(Clone, Debug, Default)]
pub struct Qwen36CalibrationV1 {
    pub sites: BTreeMap<String, SiteRangeV1>,
    /// The logit rows the reference produced, for scoring the integer class against.
    pub logits: Vec<Vec<f32>>,
}

impl Qwen36CalibrationV1 {
    fn observe(&mut self, site: &str, values: &[f32]) {
        self.sites.entry(site.to_string()).or_default().observe(values);
    }
}

fn rms_norm(x: &[f32], gain: &[f32], eps: f32) -> Vec<f32> {
    let mean = x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len().max(1) as f64;
    let scale = 1.0 / (mean as f32 + eps).sqrt();
    x.iter().zip(gain).map(|(v, g)| v * scale * g).collect()
}

/// `out[c] = Σ_i w[c·n + i] · x[i]` — the GGUF's row-major layout.
fn matmul(w: &[f32], x: &[f32], out_dim: usize) -> Vec<f32> {
    let n = x.len();
    (0..out_dim).map(|c| w[c * n..(c + 1) * n].iter().zip(x).map(|(a, b)| a * b).sum()).collect()
}

fn silu(x: &[f32]) -> Vec<f32> {
    x.iter().map(|v| v / (1.0 + (-v).exp())).collect()
}

fn softmax(x: &[f32]) -> Vec<f32> {
    let max = x.iter().fold(f32::NEG_INFINITY, |a, v| a.max(*v));
    let e: Vec<f32> = x.iter().map(|v| (v - max).exp()).collect();
    let sum: f32 = e.iter().sum();
    e.iter().map(|v| v / sum).collect()
}

fn l2_norm(x: &[f32]) -> Vec<f32> {
    let n = x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>().sqrt() as f32;
    if n > 0.0 { x.iter().map(|v| v / n).collect() } else { x.to_vec() }
}

/// The state one sequence carries through the reference.
pub struct Qwen36RefState {
    /// Per linear layer, per value head: `d_v × d_k`.
    gdn: Vec<Vec<Vec<f32>>>,
    /// Per linear layer: the convolution's window, oldest first.
    conv: Vec<Vec<Vec<f32>>>,
    keys: Vec<Vec<Vec<f32>>>,
    values: Vec<Vec<Vec<f32>>>,
}

impl Qwen36RefState {
    pub fn new(s: &Qwen36ShapeV1) -> Self {
        let n = s.n_layers();
        let mut gdn = vec![Vec::new(); n];
        let mut conv = vec![Vec::new(); n];
        let width = 2 * s.linear_k_dim() + s.linear_v_dim();
        for (li, kind) in s.layer_types.iter().enumerate() {
            if *kind == Qwen36LayerKind::LinearAttention {
                gdn[li] = vec![vec![0.0; s.linear_head_dim * s.linear_head_dim]; s.linear_v_heads];
                conv[li] = vec![vec![0.0; width]; s.conv_kernel];
            }
        }
        Self { gdn, conv, keys: vec![Vec::new(); n], values: vec![Vec::new(); n] }
    }
}

/// Run one position through the hybrid graph in `f32`, recording every site's range.
///
/// `rope` is `(cos, sin)` for this position, `rotary_dim / 2` entries each — the same pinned
/// integer table the class uses, converted, so the reference rotates by the angles the class
/// rotates by rather than by the ones a float implementation would have chosen.
#[allow(clippy::too_many_arguments)]
pub fn qwen36_reference_token<S: TensorSource>(
    source: &mut S,
    shape: &Qwen36ShapeV1,
    state: &mut Qwen36RefState,
    calibration: &mut Qwen36CalibrationV1,
    token_id: usize,
    position: usize,
    rope: (&[f32], &[f32]),
    eps: f32,
) -> Result<Vec<f32>, String> {
    let d = shape.d_model;
    let embed = source.tensor("token_embd.weight")?;
    let mut h: Vec<f32> = embed[token_id * d..(token_id + 1) * d].to_vec();
    drop(embed);
    calibration.observe("embed", &h);

    for li in 0..shape.n_layers() {
        let g = |s: &str| format!("blk.{li}.{s}");
        let attn_gain = source.tensor(&g("attn_norm.weight"))?;
        let normed = rms_norm(&h, &attn_gain, eps);
        calibration.observe(&g("attn_norm"), &normed);

        let delta = match shape.layer_types[li] {
            Qwen36LayerKind::LinearAttention => linear_arm(source, shape, state, calibration, li, &normed, eps)?,
            Qwen36LayerKind::FullAttention => full_arm(source, shape, state, calibration, li, &normed, position, rope, eps)?,
        };
        for (a, b) in h.iter_mut().zip(&delta) {
            *a += b;
        }
        calibration.observe(&g("attn_residual"), &h);

        let ffn_gain = source.tensor(&g("post_attention_norm.weight"))?;
        let normed = rms_norm(&h, &ffn_gain, eps);
        calibration.observe(&g("ffn_norm"), &normed);
        let delta = moe(source, shape, calibration, li, &normed)?;
        for (a, b) in h.iter_mut().zip(&delta) {
            *a += b;
        }
        calibration.observe(&g("ffn_residual"), &h);
    }

    let final_gain = source.tensor("output_norm.weight")?;
    let fin = rms_norm(&h, &final_gain, eps);
    calibration.observe("final_norm", &fin);
    let output = source.tensor("output.weight")?;
    let logits = matmul(&output, &fin, shape.vocab);
    calibration.observe("logits", &logits);
    calibration.logits.push(logits.clone());
    Ok(logits)
}

fn linear_arm<S: TensorSource>(
    source: &mut S,
    shape: &Qwen36ShapeV1,
    state: &mut Qwen36RefState,
    calibration: &mut Qwen36CalibrationV1,
    li: usize,
    normed: &[f32],
    eps: f32,
) -> Result<Vec<f32>, String> {
    let g = |s: &str| format!("blk.{li}.{s}");
    let (d, dk, dv, hd) = (shape.d_model, shape.linear_k_dim(), shape.linear_v_dim(), shape.linear_head_dim);
    let width = 2 * dk + dv;

    let qkv_w = source.tensor(&g("attn_qkv.weight"))?;
    let qkv = matmul(&qkv_w, normed, width);
    drop(qkv_w);
    calibration.observe(&g("linear_qkv"), &qkv);
    let z_w = source.tensor(&g("attn_gate.weight"))?;
    let z = matmul(&z_w, normed, dv);
    drop(z_w);
    calibration.observe(&g("linear_z"), &z);

    // The four-tap causal convolution, depthwise over the concatenated channels.
    let window = &mut state.conv[li];
    window.remove(0);
    window.push(qkv);
    let taps = source.tensor(&g("ssm_conv1d.weight"))?;
    let mut convolved = vec![0f32; width];
    for (c, slot) in convolved.iter_mut().enumerate() {
        *slot = (0..shape.conv_kernel).map(|t| window[t][c] * taps[t * width + c]).sum();
    }
    drop(taps);
    let activated = silu(&convolved);
    calibration.observe(&g("linear_conv"), &activated);

    let (qc, rest) = activated.split_at(dk);
    let (kc, vc) = rest.split_at(dk);

    let dt_w = source.tensor(&g("ssm_alpha.weight"))?;
    let dt = matmul(&dt_w, normed, shape.linear_v_heads);
    drop(dt_w);
    calibration.observe(&g("linear_dt"), &dt);
    let beta_w = source.tensor(&g("ssm_beta.weight"))?;
    let beta_raw = matmul(&beta_w, normed, shape.linear_v_heads);
    drop(beta_w);
    calibration.observe(&g("linear_beta"), &beta_raw);
    let a_log = source.tensor(&g("ssm_a"))?;

    let group = shape.linear_v_heads / shape.linear_k_heads;
    let mut out = Vec::with_capacity(dv);
    for vh in 0..shape.linear_v_heads {
        let kh = vh / group;
        let k = l2_norm(&kc[kh * hd..(kh + 1) * hd]);
        let q = l2_norm(&qc[kh * hd..(kh + 1) * hd]);
        let v = &vc[vh * hd..(vh + 1) * hd];
        // `decay = exp(-exp(A_log) * softplus(dt))`, the form the gate is written in.
        let softplus = |x: f32| if x > 20.0 { x } else { (1.0 + x.exp()).ln() };
        let decay = (-a_log[vh].exp() * softplus(dt[vh])).exp();
        let beta = 1.0 / (1.0 + (-beta_raw[vh]).exp());
        let s = &mut state.gdn[li][vh];
        for slot in s.iter_mut() {
            *slot *= decay;
        }
        let w: Vec<f32> = (0..hd).map(|i| (0..hd).map(|j| s[i * hd + j] * k[j]).sum()).collect();
        for i in 0..hd {
            let u = beta * (v[i] - w[i]);
            for j in 0..hd {
                s[i * hd + j] += u * k[j];
            }
        }
        out.extend((0..hd).map(|i| (0..hd).map(|j| s[i * hd + j] * q[j]).sum::<f32>()));
    }
    calibration.observe(&g("linear_state_out"), &out);

    // The output gate: RMS-normalized per head with a shared gain, times `silu(z)`.
    let norm_gain = source.tensor(&g("ssm_norm.weight"))?;
    let mut gated = Vec::with_capacity(dv);
    for vh in 0..shape.linear_v_heads {
        let head = &out[vh * hd..(vh + 1) * hd];
        gated.extend(rms_norm(head, &norm_gain, eps));
    }
    let z_act = silu(&z);
    for (a, b) in gated.iter_mut().zip(&z_act) {
        *a *= b;
    }
    calibration.observe(&g("linear_gated"), &gated);

    let out_w = source.tensor(&g("ssm_out.weight"))?;
    let delta = matmul(&out_w, &gated, d);
    calibration.observe(&g("linear_out"), &delta);
    Ok(delta)
}

#[allow(clippy::too_many_arguments)]
fn full_arm<S: TensorSource>(
    source: &mut S,
    shape: &Qwen36ShapeV1,
    state: &mut Qwen36RefState,
    calibration: &mut Qwen36CalibrationV1,
    li: usize,
    normed: &[f32],
    position: usize,
    rope: (&[f32], &[f32]),
    eps: f32,
) -> Result<Vec<f32>, String> {
    let g = |s: &str| format!("blk.{li}.{s}");
    let (d, hd) = (shape.d_model, shape.head_dim);
    let q_dim = shape.n_heads * hd;
    let kv_dim = shape.kv_dim();

    // `attn_q` is double width: query then gate.
    let q_w = source.tensor(&g("attn_q.weight"))?;
    let both = matmul(&q_w, normed, 2 * q_dim);
    drop(q_w);
    let (q_raw, gate_raw) = both.split_at(q_dim);
    calibration.observe(&g("attn_q"), q_raw);
    calibration.observe(&g("attn_gate"), gate_raw);
    let k_w = source.tensor(&g("attn_k.weight"))?;
    let k_raw = matmul(&k_w, normed, kv_dim);
    drop(k_w);
    let v_w = source.tensor(&g("attn_v.weight"))?;
    let v = matmul(&v_w, normed, kv_dim);
    drop(v_w);
    calibration.observe(&g("attn_v"), &v);

    // QK-norm per head, before the rotation.
    let q_gain = source.tensor(&g("attn_q_norm.weight"))?;
    let k_gain = source.tensor(&g("attn_k_norm.weight"))?;
    let rotate = |row: &[f32], heads: usize, gain: &[f32]| -> Vec<f32> {
        let mut out = Vec::with_capacity(row.len());
        for head in 0..heads {
            let normed = rms_norm(&row[head * hd..(head + 1) * hd], gain, eps);
            let pairs = shape.rotary_dim / 2;
            for p in 0..pairs {
                let (a, b) = (normed[2 * p], normed[2 * p + 1]);
                out.push(a * rope.0[p] - b * rope.1[p]);
                out.push(a * rope.1[p] + b * rope.0[p]);
            }
            out.extend_from_slice(&normed[shape.rotary_dim..]);
        }
        out
    };
    let q = rotate(q_raw, shape.n_heads, &q_gain);
    let k = rotate(&k_raw, shape.n_kv_heads, &k_gain);
    calibration.observe(&g("attn_q_rot"), &q);

    state.keys[li].push(k);
    state.values[li].push(v);
    let history = state.keys[li].len();
    let _ = position;

    let group = shape.n_heads / shape.n_kv_heads;
    let scale = 1.0 / (hd as f32).sqrt();
    let mut attn = Vec::with_capacity(q_dim);
    for head in 0..shape.n_heads {
        let qh = &q[head * hd..(head + 1) * hd];
        let off = (head / group) * hd;
        let logits: Vec<f32> =
            (0..history).map(|j| qh.iter().zip(&state.keys[li][j][off..off + hd]).map(|(a, b)| a * b).sum::<f32>() * scale).collect();
        calibration.observe(&g("attn_logits"), &logits);
        let probs = softmax(&logits);
        attn.extend((0..hd).map(|i| (0..history).map(|j| probs[j] * state.values[li][j][off + i]).sum::<f32>()));
    }
    calibration.observe(&g("attn_values"), &attn);

    // The output gate.
    for (a, b) in attn.iter_mut().zip(gate_raw) {
        *a *= 1.0 / (1.0 + (-b).exp());
    }
    calibration.observe(&g("attn_gated"), &attn);

    let o_w = source.tensor(&g("attn_output.weight"))?;
    let delta = matmul(&o_w, &attn, d);
    calibration.observe(&g("attn_out"), &delta);
    Ok(delta)
}

fn moe<S: TensorSource>(
    source: &mut S,
    shape: &Qwen36ShapeV1,
    calibration: &mut Qwen36CalibrationV1,
    li: usize,
    normed: &[f32],
) -> Result<Vec<f32>, String> {
    let g = |s: &str| format!("blk.{li}.{s}");
    let d = shape.d_model;

    let router_w = source.tensor(&g("ffn_gate_inp.weight"))?;
    let router = matmul(&router_w, normed, shape.n_experts);
    drop(router_w);
    calibration.observe(&g("ffn_router"), &router);
    let probs = softmax(&router);
    // Top-k by (probability descending, index ascending) — the same rule the class's router uses,
    // so a calibration run and an execution route to the same experts.
    let mut chosen: Vec<usize> = Vec::with_capacity(shape.experts_per_token);
    let mut taken = vec![false; shape.n_experts];
    for _ in 0..shape.experts_per_token {
        let mut best = usize::MAX;
        for i in 0..shape.n_experts {
            if !taken[i] && (best == usize::MAX || probs[i] > probs[best]) {
                best = i;
            }
        }
        taken[best] = true;
        chosen.push(best);
    }
    chosen.sort_unstable();
    let total: f32 = chosen.iter().map(|e| probs[*e]).sum();

    let gate_all = source.tensor(&g("ffn_gate_exps.weight"))?;
    let up_all = source.tensor(&g("ffn_up_exps.weight"))?;
    let down_all = source.tensor(&g("ffn_down_exps.weight"))?;
    let mid = shape.moe_dim;
    let per_up = mid * d;
    let per_down = d * mid;
    let mut out = vec![0f32; d];
    for e in &chosen {
        let w = probs[*e] / total.max(f32::MIN_POSITIVE);
        let gate = matmul(&gate_all[e * per_up..(e + 1) * per_up], normed, mid);
        let up = matmul(&up_all[e * per_up..(e + 1) * per_up], normed, mid);
        let act: Vec<f32> = silu(&gate).iter().zip(&up).map(|(a, b)| a * b).collect();
        calibration.observe(&g("ffn_expert_gated"), &act);
        let y = matmul(&down_all[e * per_down..(e + 1) * per_down], &act, d);
        for (slot, v) in out.iter_mut().zip(&y) {
            *slot += w * v;
        }
    }
    drop(gate_all);
    drop(up_all);
    drop(down_all);
    calibration.observe(&g("ffn_routed"), &out);

    // The shared expert, always on, behind its scalar gate.
    let sg_w = source.tensor(&g("ffn_gate_inp_shexp.weight"))?;
    let sg = 1.0 / (1.0 + (-matmul(&sg_w, normed, 1)[0]).exp());
    drop(sg_w);
    let s_mid = shape.shared_dim;
    let s_gate = source.tensor(&g("ffn_gate_shexp.weight"))?;
    let s_up = source.tensor(&g("ffn_up_shexp.weight"))?;
    let act: Vec<f32> = silu(&matmul(&s_gate, normed, s_mid)).iter().zip(&matmul(&s_up, normed, s_mid)).map(|(a, b)| a * b).collect();
    drop(s_gate);
    drop(s_up);
    let s_down = source.tensor(&g("ffn_down_shexp.weight"))?;
    let shared = matmul(&s_down, &act, d);
    for (slot, v) in out.iter_mut().zip(&shared) {
        *slot += sg * v;
    }
    calibration.observe(&g("ffn_moe_out"), &out);
    Ok(out)
}

/// How close an integer run is to this reference: top-1 agreement, top-5 containment, and the rank
/// correlation over the head of the distribution.
///
/// The same three the dense tier's scorer uses, because the question is the same one and a second
/// scoring convention would make the two classes' fidelity numbers incomparable.
#[derive(Clone, Copy, Debug, Default)]
pub struct Qwen36FidelityV1 {
    pub positions: usize,
    pub top1_agree: usize,
    pub top5_contains: usize,
    pub rank_correlation: f64,
}

impl Qwen36FidelityV1 {
    /// Faithful enough to be worth registering: the class agrees on most tokens and ranks the head
    /// of the distribution the same way.
    pub fn is_faithful(&self) -> bool {
        self.positions > 0 && self.top1_agree * 2 >= self.positions && self.rank_correlation > 0.5
    }
}

/// Score integer logit rows against reference rows.
pub fn qwen36_score_fidelity(reference: &[Vec<f32>], integer: &[Vec<i32>]) -> Qwen36FidelityV1 {
    let mut out = Qwen36FidelityV1::default();
    for (r, i) in reference.iter().zip(integer) {
        if r.is_empty() || i.is_empty() {
            continue;
        }
        out.positions += 1;
        let arg = |v: &[f32]| -> usize {
            v.iter().enumerate().fold((0usize, f32::NEG_INFINITY), |a, (k, x)| if *x > a.1 { (k, *x) } else { a }).0
        };
        let arg_i = crate::engine::argmax_lowest(i);
        let top = arg(r);
        if top == arg_i {
            out.top1_agree += 1;
        }
        let mut order: Vec<usize> = (0..r.len()).collect();
        order.sort_by(|a, b| r[*b].partial_cmp(&r[*a]).unwrap_or(std::cmp::Ordering::Equal));
        if order[..5.min(order.len())].contains(&arg_i) {
            out.top5_contains += 1;
        }
        // Spearman over the reference's top 100, which is where a token is actually chosen from.
        let head = &order[..100.min(order.len())];
        let mut by_int: Vec<usize> = head.to_vec();
        by_int.sort_by_key(|k| std::cmp::Reverse(i[*k]));
        let position: BTreeMap<usize, usize> = by_int.iter().enumerate().map(|(rank, k)| (*k, rank)).collect();
        let n = head.len() as f64;
        let d2: f64 = head
            .iter()
            .enumerate()
            .map(|(rank, k)| {
                let d = rank as f64 - position[k] as f64;
                d * d
            })
            .sum();
        let rho = if n > 1.0 { 1.0 - 6.0 * d2 / (n * (n * n - 1.0)) } else { 1.0 };
        out.rank_correlation += rho;
    }
    if out.positions > 0 {
        out.rank_correlation /= out.positions as f64;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scorer separates agreement from disagreement, which is the only thing it has to do
    /// before it is trusted to say a conversion is faithful.
    #[test]
    fn the_scorer_separates_agreement_from_disagreement() {
        let reference: Vec<Vec<f32>> = vec![(0..200).map(|i| i as f32).collect()];
        let same: Vec<Vec<i32>> = vec![(0..200).collect()];
        let scored = qwen36_score_fidelity(&reference, &same);
        assert_eq!(scored.top1_agree, 1);
        assert!(scored.rank_correlation > 0.99, "identical rankings, got {scored:?}");
        assert!(scored.is_faithful());

        let reversed: Vec<Vec<i32>> = vec![(0..200).map(|i: i32| -i).collect()];
        let scored = qwen36_score_fidelity(&reference, &reversed);
        assert_eq!(scored.top1_agree, 0);
        assert!(scored.rank_correlation < -0.9, "reversed rankings, got {scored:?}");
        assert!(!scored.is_faithful());
    }

    /// A site's crest factor is what says whether one symmetric scale can serve a row. A flat row
    /// has a crest near one; a row with one outlier has a large one.
    #[test]
    fn the_crest_factor_finds_an_outlier_channel() {
        let mut flat = SiteRangeV1::default();
        flat.observe(&[1.0, -1.0, 1.0, -1.0]);
        assert!((flat.crest() - 1.0).abs() < 1e-9);

        let mut spiky = SiteRangeV1::default();
        spiky.observe(&[100.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0]);
        assert!(spiky.crest() > 2.5, "an outlier must show, got {}", spiky.crest());
        assert_eq!(spiky.absmax, 100.0);
    }
}
