//! **`ATTN_JOINT_STREAMS_V1`** — MMDiT's joint attention: two streams (the image tokens and the text tokens), each
//! with its own q/k/v projections, ONE attention over the concatenated stream, split back, each stream with its own
//! output projection (RFC-0003 §6; diffusers `JointAttnProcessor2_0`, image tokens first).
//!
//! Full attention over a FIXED axis of `T = N + L` tokens (not a history window), as the bidirectional encoders'
//! (`lower::bidir`): q, k, v are `i16` codes committed head-major `[h, T, dh]` (one head's K and V are whole
//! leaves); the scores `Q·Kᵀ` are exact `i64` dots narrowed to `i32` logits in Q`LOGIT_Q` (the `1/√dh` scale and
//! both code scales are the multiplier); the softmax is the library's two-pass `softmax_shifted` written with its
//! logits, row maximum and row reciprocal as COMMIT POINTS (so a tile of anything downstream opens rows, not the
//! whole `T²` score tensor: "committed row maxima and denominators"); `P·V` (Q24 against value codes, exact
//! `i64`) is narrowed back to codes, committed, and projected. The lossy sites: the six projections' and the two
//! output projections' narrowings, the scores' narrowing, the softmax's `IntExp`/`IntRecip`, the context's
//! narrowing — about four beyond the projections, as the RFC counts.

use misaka_palw_tir::arith::K;
use misaka_palw_tir::builder::BlockBuilder;
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::{DType, Ref, Rounding};

use super::linear::{QLinearRefs, lower_linear, lower_linear_codes};
use crate::quant::mul_shift;

/// Attention logits are `i32` in Q`LOGIT_Q` (the bidirectional encoders' `lower::LOGIT_Q`).
pub const LOGIT_Q: u32 = 14;

/// The geometry of one joint attention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JointAttnSpec {
    pub heads: usize,
    pub head_dim: usize,
    pub n_img: usize,
    pub n_txt: usize,
}

impl JointAttnSpec {
    pub fn width(&self) -> usize {
        self.heads * self.head_dim
    }
    pub fn tokens(&self) -> usize {
        self.n_img + self.n_txt
    }
}

/// The declared projections and the two narrowings' constants.
#[derive(Clone, Copy, Debug)]
pub struct JointAttnRefs {
    pub q: QLinearRefs,
    pub k: QLinearRefs,
    pub v: QLinearRefs,
    pub add_q: QLinearRefs,
    pub add_k: QLinearRefs,
    pub add_v: QLinearRefs,
    pub o: QLinearRefs,
    /// The text stream's output projection; `None` in the last block (`context_pre_only`: nothing reads it).
    pub add_o: Option<QLinearRefs>,
}

/// The scores' and the context's narrowing pairs `(m, s)`, from the codes' scales:
/// `m_score/2^s_score = q_scale · k_scale · (1/√dh) · 2^LOGIT_Q`, `m_ctx/2^s_ctx = v_scale / 2^24 / ctx_scale`.
pub fn attn_narrowings(spec: &JointAttnSpec, q_scale: f64, k_scale: f64, v_scale: f64, ctx_scale: f64) -> ((i64, i8), (i64, i8)) {
    let score = mul_shift(q_scale * k_scale / (spec.head_dim as f64).sqrt() * (1u64 << LOGIT_Q) as f64);
    let ctx = mul_shift(v_scale / (1u64 << K) as f64 / ctx_scale);
    (score, ctx)
}

/// The library's `softmax_shifted` over the last axis with the logits, the row maximum and the row reciprocal
/// committed (bit for bit the same values; the reciprocal's clamp never fires: `[2^24 / T, 2^24]`).
pub fn softmax_committed(b: &mut BlockBuilder<'_>, x: Ref, up_bits: u32) -> Ref {
    b.commit(x);
    let axis = b.shape(x).len() - 1;
    let up = up_bits.min(62);
    let max = b.reduce_max(x, axis);
    b.commit(max);
    let diff = b.sub(x, max, DType::I64);
    let d = b.clamp(diff, (i32::MIN as i64) >> up, 0, DType::I64);
    let scale = b.c(DType::I64, 1i128 << up);
    let w = b.mul(d, scale, DType::I64);
    let arg = b.clamp(w, i32::MIN as i64, 0, DType::I32);
    let e = b.int_exp(arg);
    let sum = b.reduce_sum(e, axis, DType::I64);
    let recip = b.int_recip(sum);
    let recip = b.clamp(recip, 0, i32::MAX as i64, DType::I32);
    b.commit(recip);
    let p = b.mul(e, recip, DType::I128);
    let q = b.shr(p, K, Rounding::Floor, DType::I64);
    b.clamp(q, 0, 1 << 25, DType::I32)
}

/// A `[T, d]` tensor as head-major `[h, T, dh]`.
fn head_major(b: &mut BlockBuilder<'_>, x: Ref, t: u32, h: u32, dh: u32) -> Ref {
    let r = b.reshape_fixed(x, &[t, h, dh]);
    b.transpose(r, &[1, 0, 2])
}

/// One of q, k, v: each stream's own projection to codes, the streams concatenated (image tokens first),
/// head-major and committed.
fn project_joint(
    b: &mut BlockBuilder<'_>,
    x_img: Ref,
    x_txt: Ref,
    w_img: &QLinearRefs,
    w_txt: &QLinearRefs,
    (t, h, dh): (u32, u32, u32),
) -> Ref {
    let a = lower_linear_codes(b, x_img, w_img);
    let c = lower_linear_codes(b, x_txt, w_txt);
    let joint = b.concat(&[a, c], 0); // [T, d]
    let hm = head_major(b, joint, t, h, dh);
    b.commit(hm)
}

/// **The joint attention**: `x_img:[N, d]` and `x_txt:[L, d]` `i16` codes (the modulated, normalised streams) to
/// the image stream's output `[N, d]` and, unless `r.add_o` is `None`, the text stream's `[L, d]`, both through the
/// output projections `(lo, hi, dtype)`. `score` and `ctx` are [`attn_narrowings`]'s pairs.
pub fn lower_joint_attention(
    b: &mut BlockBuilder<'_>,
    x_img: Ref,
    x_txt: Ref,
    r: &JointAttnRefs,
    spec: &JointAttnSpec,
    score: (i64, i8),
    ctx: (i64, i8),
    out: (i64, i64, DType),
) -> (Ref, Option<Ref>) {
    let (t, h, dh, d) = (spec.tokens() as u32, spec.heads as u32, spec.head_dim as u32, spec.width() as u32);
    let q = project_joint(b, x_img, x_txt, &r.q, &r.add_q, (t, h, dh));
    let k = project_joint(b, x_img, x_txt, &r.k, &r.add_k, (t, h, dh));
    let v = project_joint(b, x_img, x_txt, &r.v, &r.add_v, (t, h, dh));
    let kt = b.transpose(k, &[0, 2, 1]); // [h, dh, T]
    let scores = b.matmul(q, kt, DType::I64); // [h, T, T]
    let m = b.c(DType::I64, score.0 as i128);
    let s = b.c(DType::I8, score.1 as i128);
    let logits = b.narrow(scores, &Narrowing::new(m, s, None), i32::MIN as i64, i32::MAX as i64, DType::I32);
    let p = softmax_committed(b, logits, 24 - LOGIT_Q);
    let o = b.matmul(p, v, DType::I64); // [h, T, dh]
    let cm = b.c(DType::I64, ctx.0 as i128);
    let cs = b.c(DType::I8, ctx.1 as i128);
    let codes = b.narrow(o, &Narrowing::new(cm, cs, None), -32_767, 32_767, DType::I16);
    let codes = b.commit(codes);
    let tok = b.transpose(codes, &[1, 0, 2]); // [T, h, dh]
    let joint = b.reshape_fixed(tok, &[t, d]);
    let img = b.slice(joint, 0, 0, spec.n_img as u32);
    let out_img = lower_linear(b, img, &r.o, out.0, out.1, out.2);
    // The text slice exists only when its projection does (a node nothing reads is refused by normal form).
    let out_txt = r.add_o.map(|w| {
        let txt = b.slice(joint, 0, spec.n_img as u32, spec.n_txt as u32);
        lower_linear(b, txt, &w, out.0, out.1, out.2)
    });
    (out_img, out_txt)
}

#[cfg(test)]
mod tests {
    use super::super::linear::QLinear;
    use super::super::testkit::{Lcg, run_multi};
    use super::*;

    /// A float joint attention over `xi:[N,d]`, `xt:[L,d]` with the given float weights (all `[d, d]` + `[d]` bias).
    #[allow(clippy::type_complexity)]
    fn float_attention(spec: &JointAttnSpec, xi: &[f64], xt: &[f64], w: &[(Vec<f32>, Vec<f32>); 8]) -> (Vec<f64>, Vec<f64>) {
        let (n, l, d, h, dh) = (spec.n_img, spec.n_txt, spec.width(), spec.heads, spec.head_dim);
        let lin = |x: &[f64], rows: usize, (wm, bv): &(Vec<f32>, Vec<f32>)| -> Vec<f64> {
            (0..rows)
                .flat_map(|r| (0..d).map(move |o| (r, o)))
                .map(|(r, o)| bv[o] as f64 + (0..d).map(|i| x[r * d + i] * wm[o * d + i] as f64).sum::<f64>())
                .collect()
        };
        let cat = |a: Vec<f64>, b: Vec<f64>| [a, b].concat();
        let q = cat(lin(xi, n, &w[0]), lin(xt, l, &w[3]));
        let k = cat(lin(xi, n, &w[1]), lin(xt, l, &w[4]));
        let v = cat(lin(xi, n, &w[2]), lin(xt, l, &w[5]));
        let t = n + l;
        let mut ctx = vec![0f64; t * d];
        for head in 0..h {
            for i in 0..t {
                let sc: Vec<f64> = (0..t)
                    .map(|j| (0..dh).map(|e| q[i * d + head * dh + e] * k[j * d + head * dh + e]).sum::<f64>() / (dh as f64).sqrt())
                    .collect();
                let mx = sc.iter().cloned().fold(f64::MIN, f64::max);
                let ex: Vec<f64> = sc.iter().map(|s| (s - mx).exp()).collect();
                let z: f64 = ex.iter().sum();
                for e in 0..dh {
                    ctx[i * d + head * dh + e] = (0..t).map(|j| ex[j] / z * v[j * d + head * dh + e]).sum();
                }
            }
        }
        (lin(&ctx[..n * d], n, &w[6]), lin(&ctx[n * d..], l, &w[7]))
    }

    fn rand_w(rng: &mut Lcg, d: usize) -> (Vec<f32>, Vec<f32>) {
        ((0..d * d).map(|_| rng.unit() as f32 / (d as f32).sqrt() * 1.5).collect(), (0..d).map(|_| rng.unit() as f32 * 0.1).collect())
    }

    #[test]
    fn the_joint_attention_is_the_float_one_to_the_codes_precision() {
        let mut rng = Lcg(21);
        let spec = JointAttnSpec { heads: 2, head_dim: 4, n_img: 3, n_txt: 2 };
        let d = spec.width();
        let w: [(Vec<f32>, Vec<f32>); 8] = std::array::from_fn(|_| rand_w(&mut rng, d));
        let (sx, sq, sk, sv, sctx, so) = (1.0 / 4096.0, 1.0 / 4096.0, 1.0 / 4096.0, 1.0 / 4096.0, 1.0 / 4096.0, 1.0 / 4096.0);
        let qs: Vec<QLinear> = w
            .iter()
            .enumerate()
            .map(|(i, (wm, bv))| {
                let (xs, ys) = match i {
                    0 | 3 => (sx, sq),
                    1 | 4 => (sx, sk),
                    2 | 5 => (sx, sv),
                    _ => (sctx, so),
                };
                QLinear::new(wm, d, d, Some(bv), xs, ys)
            })
            .collect();
        let (score, ctxn) = attn_narrowings(&spec, sq, sk, sv, sctx);
        let xi: Vec<i128> = (0..spec.n_img * d).map(|_| rng.range(-6_000, 6_000)).collect();
        let xt: Vec<i128> = (0..spec.n_txt * d).map(|_| rng.range(-6_000, 6_000)).collect();
        let (xi2, xt2) = (xi.clone(), xt.clone());
        let q = &qs;
        let outs = run_multi(
            |pb, sink| {
                let a = sink.put(pb, "x_img", DType::I16, &[spec.n_img as u32, d as u32], xi2.clone());
                let c = sink.put(pb, "x_txt", DType::I16, &[spec.n_txt as u32, d as u32], xt2.clone());
                let names = ["q", "k", "v", "add_q", "add_k", "add_v", "o", "add_o"];
                let refs: Vec<QLinearRefs> = names.iter().enumerate().map(|(i, n)| q[i].declare(pb, sink, n)).collect();
                (a, c, refs)
            },
            |b, (a, c, refs)| {
                let r = JointAttnRefs {
                    q: refs[0],
                    k: refs[1],
                    v: refs[2],
                    add_q: refs[3],
                    add_k: refs[4],
                    add_v: refs[5],
                    o: refs[6],
                    add_o: Some(refs[7]),
                };
                let (i, t) = lower_joint_attention(b, a, c, &r, &spec, score, ctxn, (-32_767, 32_767, DType::I16));
                vec![i, t.expect("a text output")]
            },
            1,
        );
        let (img, txt) = (&outs[0][0], &outs[0][1]);
        let (fi, ft) = float_attention(
            &spec,
            &xi.iter().map(|v| *v as f64 * sx).collect::<Vec<_>>(),
            &xt.iter().map(|v| *v as f64 * sx).collect::<Vec<_>>(),
            &w,
        );
        for (name, got, want) in [("image", img, &fi), ("text", txt, &ft)] {
            let amax = want.iter().fold(0f64, |m, v| m.max(v.abs()));
            for (i, wv) in want.iter().enumerate() {
                let g = got.data[i] as f64 * so;
                assert!((g - wv).abs() <= 0.02 * amax + 6.0 * so, "{name}[{i}]: integer {g} vs float {wv} (amax {amax})");
            }
        }
    }
}
