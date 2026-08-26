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
    /// Per lane, over every position. **A site's crest is not its lanes' crests.** An elementwise
    /// product of two well-conditioned rows can have a crest of forty-five — the large values do
    /// not co-occur — and one exponent chosen from the whole row's peak then leaves the ordinary
    /// lanes four or five bits. Measured, that cost the arm's output a cosine of 0.7 against the
    /// reference while every magnitude looked correct.
    pub lanes: Vec<f64>,
}

impl SiteRangeV1 {
    fn observe(&mut self, values: &[f32]) {
        if self.lanes.len() < values.len() {
            self.lanes.resize(values.len(), 0.0);
        }
        for (i, v) in values.iter().enumerate() {
            let x = (*v as f64).abs();
            self.absmax = self.absmax.max(x);
            self.sum_squares += x * x;
            self.lanes[i] = self.lanes[i].max(x);
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
    /// The LAST position's row at each named site. Magnitudes say whether a scale is right;
    /// only the rows say whether the computation is. A cosine that is fine at one site and halved
    /// at the next names the stage that is wrong, which no aggregate can.
    pub rows: BTreeMap<String, Vec<f32>>,
}

impl Qwen36CalibrationV1 {
    fn observe(&mut self, site: &str, values: &[f32]) {
        self.sites.entry(site.to_string()).or_default().observe(values);
        // Overwritten per position, so what survives is the last one — the same position the
        // integer probe reports.
        self.rows.insert(site.to_string(), values.to_vec());
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

/// One linear-attention layer's weights, borrowed. The converter already has these dequantized
/// when it writes the layer's codes, so the reference reads them rather than fetching again —
/// which is what lets ONE streaming pass over a 24 GiB checkpoint both quantize the weights and
/// measure the activations.
pub struct LinearWeights<'a> {
    pub attn_norm: &'a [f32],
    pub post_norm: &'a [f32],
    pub qkv: &'a [f32],
    pub gate: &'a [f32],
    pub conv: &'a [f32],
    pub alpha: &'a [f32],
    pub beta: &'a [f32],
    pub a_log: &'a [f32],
    pub ssm_norm: &'a [f32],
    pub out: &'a [f32],
}

/// One gated-attention layer's weights, borrowed.
pub struct FullWeights<'a> {
    pub attn_norm: &'a [f32],
    pub post_norm: &'a [f32],
    /// Double width: query then gate.
    pub q: &'a [f32],
    pub k: &'a [f32],
    pub v: &'a [f32],
    pub o: &'a [f32],
    pub q_norm: &'a [f32],
    pub k_norm: &'a [f32],
}

/// One mixture's weights, borrowed.
pub struct MoeWeights<'a> {
    pub post_norm: &'a [f32],
    pub router: &'a [f32],
    pub gate_exps: &'a [f32],
    pub up_exps: &'a [f32],
    pub down_exps: &'a [f32],
    pub shared_gate: &'a [f32],
    pub shared_up: &'a [f32],
    pub shared_down: &'a [f32],
    pub shared_router: &'a [f32],
}

/// **Advance every position through one linear-attention layer.**
///
/// Layer-major rather than token-major: all positions cross layer `i` while layer `i`'s weights
/// are in hand. The recurrence makes the order within the layer matter — position `t`'s state
/// update is what position `t+1` reads — so the positions are walked in order here, which is the
/// same order a token-major driver would have produced.
pub fn reference_linear_layer(
    shape: &Qwen36ShapeV1,
    state: &mut Qwen36RefState,
    calibration: &mut Qwen36CalibrationV1,
    li: usize,
    w: &LinearWeights<'_>,
    positions: &mut [Vec<f32>],
    eps: f32,
) {
    let g = |s: &str| format!("blk.{li}.{s}");
    let (d, dk, dv, hd) = (shape.d_model, shape.linear_k_dim(), shape.linear_v_dim(), shape.linear_head_dim);
    let width = 2 * dk + dv;
    let group = shape.linear_v_heads / shape.linear_k_heads;

    for h in positions.iter_mut() {
        let normed = rms_norm(h, w.attn_norm, eps);
        calibration.observe(&g("attn_norm"), &normed);

        let qkv = matmul(w.qkv, &normed, width);
        calibration.observe(&g("linear_qkv"), &qkv);
        let z = matmul(w.gate, &normed, dv);
        calibration.observe(&g("linear_z"), &z);

        let window = &mut state.conv[li];
        window.remove(0);
        window.push(qkv);
        let mut convolved = vec![0f32; width];
        for (c, slot) in convolved.iter_mut().enumerate() {
            // `[channel][tap]`, matching the checkpoint: `ssm_conv1d.weight` is `[4, 8192]`, which
            // in GGUF's fastest-varying-first order is 8,192 rows of four. Reading it the other way
            // computes a different convolution, and the reference has to compute the one the class
            // computes or the calibration measures the wrong ranges.
            *slot = (0..shape.conv_kernel).map(|t| window[t][c] * w.conv[c * shape.conv_kernel + t]).sum();
        }
        let activated = silu(&convolved);
        calibration.observe(&g("linear_conv"), &activated);
        let (qc, rest) = activated.split_at(dk);
        let (kc, vc) = rest.split_at(dk);

        let dt = matmul(w.alpha, &normed, shape.linear_v_heads);
        calibration.observe(&g("linear_dt"), &dt);
        let beta_raw = matmul(w.beta, &normed, shape.linear_v_heads);
        calibration.observe(&g("linear_beta"), &beta_raw);

        let mut out = Vec::with_capacity(dv);
        for vh in 0..shape.linear_v_heads {
            let kh = vh / group;
            let k = l2_norm(&kc[kh * hd..(kh + 1) * hd]);
            let q = l2_norm(&qc[kh * hd..(kh + 1) * hd]);
            let v = &vc[vh * hd..(vh + 1) * hd];
            let softplus = |x: f32| if x > 20.0 { x } else { (1.0 + x.exp()).ln() };
            let decay = (-w.a_log[vh].exp() * softplus(dt[vh])).exp();
            let beta = 1.0 / (1.0 + (-beta_raw[vh]).exp());
            calibration.observe(&g("linear_decay"), &[decay]);
            calibration.observe(&g("linear_beta_gate"), &[beta]);
            let s = &mut state.gdn[li][vh];
            for slot in s.iter_mut() {
                *slot *= decay;
            }
            let pred: Vec<f32> = (0..hd).map(|i| (0..hd).map(|j| s[i * hd + j] * k[j]).sum()).collect();
            for i in 0..hd {
                let u = beta * (v[i] - pred[i]);
                for j in 0..hd {
                    s[i * hd + j] += u * k[j];
                }
            }
            calibration.observe(&g("linear_state"), s);
            out.extend((0..hd).map(|i| (0..hd).map(|j| s[i * hd + j] * q[j]).sum::<f32>()));
        }
        calibration.observe(&g("linear_state_out"), &out);

        // **Three sites, not one.** The head-wise norm CHANGES the magnitude — that is what a norm
        // is — so the value that reaches the multiply is not the state output and cannot ride its
        // exponent. Placing it there put a value of 22 on a grid whose rail is 2, and the whole
        // arm's contribution was crushed by an order of magnitude. The gate is separate for the
        // same reason: `silu(z)` is unbounded above, so a probability's exponent saturates it.
        let mut normed_out = Vec::with_capacity(dv);
        for vh in 0..shape.linear_v_heads {
            normed_out.extend(rms_norm(&out[vh * hd..(vh + 1) * hd], w.ssm_norm, eps));
        }
        calibration.observe(&g("linear_normed"), &normed_out);
        let z_act = silu(&z);
        calibration.observe(&g("linear_gate_act"), &z_act);
        let gated: Vec<f32> = normed_out.iter().zip(&z_act).map(|(a, b)| a * b).collect();
        calibration.observe(&g("linear_gated"), &gated);

        let delta = matmul(w.out, &gated, d);
        calibration.observe(&g("linear_out"), &delta);
        for (a, b) in h.iter_mut().zip(&delta) {
            *a += b;
        }
        calibration.observe(&g("attn_residual"), h);
    }
}

/// **Advance every position through one gated-attention layer.**
pub fn reference_full_layer(
    shape: &Qwen36ShapeV1,
    state: &mut Qwen36RefState,
    calibration: &mut Qwen36CalibrationV1,
    li: usize,
    w: &FullWeights<'_>,
    positions: &mut [Vec<f32>],
    rope: &[(Vec<f32>, Vec<f32>)],
    eps: f32,
) {
    let g = |s: &str| format!("blk.{li}.{s}");
    let (d, hd) = (shape.d_model, shape.head_dim);
    let q_dim = shape.n_heads * hd;
    let kv_dim = shape.kv_dim();
    let group = shape.n_heads / shape.n_kv_heads;
    let scale = 1.0 / (hd as f32).sqrt();

    for (position, h) in positions.iter_mut().enumerate() {
        let normed = rms_norm(h, w.attn_norm, eps);
        calibration.observe(&g("attn_norm"), &normed);
        let both = matmul(w.q, &normed, 2 * q_dim);
        let (q_raw, gate_raw) = both.split_at(q_dim);
        calibration.observe(&g("attn_q"), q_raw);
        calibration.observe(&g("attn_gate"), gate_raw);
        let k_raw = matmul(w.k, &normed, kv_dim);
        let v = matmul(w.v, &normed, kv_dim);
        calibration.observe(&g("attn_v"), &v);

        let (cos, sin) = &rope[position.min(rope.len() - 1)];
        let rotate = |row: &[f32], heads: usize, gain: &[f32]| -> Vec<f32> {
            let mut out = Vec::with_capacity(row.len());
            for head in 0..heads {
                let n = rms_norm(&row[head * hd..(head + 1) * hd], gain, eps);
                for p in 0..shape.rotary_dim / 2 {
                    let (a, b) = (n[2 * p], n[2 * p + 1]);
                    out.push(a * cos[p] - b * sin[p]);
                    out.push(a * sin[p] + b * cos[p]);
                }
                out.extend_from_slice(&n[shape.rotary_dim..]);
            }
            out
        };
        let q = rotate(q_raw, shape.n_heads, w.q_norm);
        let k = rotate(&k_raw, shape.n_kv_heads, w.k_norm);
        calibration.observe(&g("attn_q_rot"), &q);
        calibration.observe(&g("attn_k_rot"), &k);

        state.keys[li].push(k);
        state.values[li].push(v);
        let history = state.keys[li].len();

        let mut attn = Vec::with_capacity(q_dim);
        for head in 0..shape.n_heads {
            let qh = &q[head * hd..(head + 1) * hd];
            let off = (head / group) * hd;
            let logits: Vec<f32> = (0..history)
                .map(|j| qh.iter().zip(&state.keys[li][j][off..off + hd]).map(|(a, b)| a * b).sum::<f32>() * scale)
                .collect();
            calibration.observe(&g("attn_logits"), &logits);
            let probs = softmax(&logits);
            attn.extend((0..hd).map(|i| (0..history).map(|j| probs[j] * state.values[li][j][off + i]).sum::<f32>()));
        }
        calibration.observe(&g("attn_values"), &attn);
        for (a, b) in attn.iter_mut().zip(gate_raw) {
            *a *= 1.0 / (1.0 + (-b).exp());
        }
        calibration.observe(&g("attn_gated"), &attn);
        let delta = matmul(w.o, &attn, d);
        calibration.observe(&g("attn_out"), &delta);
        for (a, b) in h.iter_mut().zip(&delta) {
            *a += b;
        }
        calibration.observe(&g("attn_residual"), h);
    }
}

/// **Advance every position through one mixture.**
pub fn reference_moe_layer(
    shape: &Qwen36ShapeV1,
    calibration: &mut Qwen36CalibrationV1,
    li: usize,
    w: &MoeWeights<'_>,
    positions: &mut [Vec<f32>],
    eps: f32,
) {
    let g = |s: &str| format!("blk.{li}.{s}");
    let d = shape.d_model;
    let mid = shape.moe_dim;
    let (per_up, per_down) = (mid * d, d * mid);

    for h in positions.iter_mut() {
        let normed = rms_norm(h, w.post_norm_of(), eps);
        calibration.observe(&g("ffn_norm"), &normed);
        let router = matmul(w.router, &normed, shape.n_experts);
        calibration.observe(&g("ffn_router"), &router);
        let probs = softmax(&router);
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

        let mut out = vec![0f32; d];
        for e in &chosen {
            let weight = probs[*e] / total.max(f32::MIN_POSITIVE);
            let gate = matmul(&w.gate_exps[e * per_up..(e + 1) * per_up], &normed, mid);
            let up = matmul(&w.up_exps[e * per_up..(e + 1) * per_up], &normed, mid);
            calibration.observe(&g("ffn_expert_gate"), &gate);
            calibration.observe(&g("ffn_expert_up"), &up);
            let act: Vec<f32> = silu(&gate).iter().zip(&up).map(|(a, b)| a * b).collect();
            calibration.observe(&g("ffn_expert_gated"), &act);
            let y = matmul(&w.down_exps[e * per_down..(e + 1) * per_down], &act, d);
            calibration.observe(&g("ffn_expert_out"), &y);
            for (slot, v) in out.iter_mut().zip(&y) {
                *slot += weight * v;
            }
        }
        calibration.observe(&g("ffn_routed"), &out);

        let sg = 1.0 / (1.0 + (-matmul(w.shared_router, &normed, 1)[0]).exp());
        let s_mid = shape.shared_dim;
        let s_gate = matmul(w.shared_gate, &normed, s_mid);
        let s_up = matmul(w.shared_up, &normed, s_mid);
        calibration.observe(&g("ffn_shared_up"), &s_up);
        let act: Vec<f32> = silu(&s_gate).iter().zip(&s_up).map(|(a, b)| a * b).collect();
        calibration.observe(&g("ffn_shared_gated"), &act);
        let shared = matmul(w.shared_down, &act, d);
        calibration.observe(&g("ffn_shared_out"), &shared);
        for (slot, v) in out.iter_mut().zip(&shared) {
            *slot += sg * v;
        }
        calibration.observe(&g("ffn_moe_out"), &out);
        for (a, b) in h.iter_mut().zip(&out) {
            *a += b;
        }
        calibration.observe(&g("ffn_residual"), h);
    }
}

impl MoeWeights<'_> {
    /// The mixture's own norm gain, carried alongside it so the layer functions stay one argument
    /// each. The linear and full arms carry it too; only one of the three uses it per layer.
    fn post_norm_of(&self) -> &[f32] {
        self.post_norm
    }
}

/// **The final norm and the unembedding**, which close a run.
pub fn reference_head(
    shape: &Qwen36ShapeV1,
    calibration: &mut Qwen36CalibrationV1,
    output_norm: &[f32],
    output: &[f32],
    positions: &[Vec<f32>],
    eps: f32,
) {
    for h in positions {
        let fin = rms_norm(h, output_norm, eps);
        calibration.observe("final_norm", &fin);
        let logits = matmul(output, &fin, shape.vocab);
        calibration.observe("logits", &logits);
        calibration.logits.push(logits);
    }
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
    /// Cosine of the whole logit vector. It separates "the head is right and the top is noisy"
    /// from "the model is computing something else": a rank correlation can be low for either
    /// reason and a cosine near one rules the second out.
    pub cosine: f64,
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

        let (mut dot, mut na, mut nb) = (0f64, 0f64, 0f64);
        for (x, y) in r.iter().zip(i) {
            let (x, y) = (*x as f64, *y as f64);
            dot += x * y;
            na += x * x;
            nb += y * y;
        }
        if na > 0.0 && nb > 0.0 {
            out.cosine += dot / (na.sqrt() * nb.sqrt());
        }
    }
    if out.positions > 0 {
        out.rank_correlation /= out.positions as f64;
        out.cosine /= out.positions as f64;
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
