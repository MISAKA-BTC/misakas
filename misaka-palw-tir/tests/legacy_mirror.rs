//! **Legacy conformance, second evidence: A16 and Q36 kernels as IR segments.**
//!
//! The functions in `mod legacy` are VERBATIM transcriptions (test-only) of the live kernels in
//! `kaspa-consensus-core` at `rcore/int-6` 08481e720 — `palw_base0.rs`, `palw_base0_ops.rs`,
//! `palw_base0_a16.rs`, `palw_qwen36_ops.rs` — copied so this leaf crate need not depend on the
//! consensus crate. Each test runs the builder's composite template through the reference evaluator and
//! requires byte identity with the transcription on seeded random inputs AND on the type
//! extremes (i32::MIN, ±32767, adversarial multipliers and zero points).
//!
//! Where the legacy kernel REFUSES an input (a non-A16 lane, a decay outside `[0, ONE]`, a
//! multiplier past `2^30`), the IR segment computes a defined value; conformance is claimed on the
//! legacy kernel's own domain, which is what a fused kernel needs (RFC F-1).

mod common;

use common::{Lcg, arg, eval_graph};
use misaka_palw_tir::arith::{K as TK, ONE as TONE};
use misaka_palw_tir::{DType, Rounding};

#[allow(clippy::all)]
mod legacy {
    pub const K: u32 = 24;
    pub const ONE: i64 = 1i64 << K;
    pub const LN2_Q: i32 = 11_629_080;
    pub const POLY2_A: i64 = 6_014_632;
    pub const POLY2_B: i64 = 22_699_573;
    pub const POLY2_C: i64 = 5_771_362;
    pub const Z_MAX: i32 = 31;
    pub const RSQRT_SEED: [i64; 16] = [
        15_395_829, 14_307_657, 13_421_772, 12_682_383, 12_053_107, 11_509_075, 11_032_629, 10_610_843, 10_234_005, 9_894_662,
        9_586_980, 9_306_325, 9_048_957, 8_811_825, 8_592_409, 8_388_608,
    ];
    pub const A16_CODE_MAX: i64 = i16::MAX as i64;

    pub fn rounding_shift_right(x: i32, s: u8) -> i32 {
        let s = s.min(31);
        if s == 0 {
            return x;
        }
        let (divisor, half) = (1i64 << s, 1i64 << (s - 1));
        let magnitude = (x as i64).abs();
        let rounded = (magnitude + half) / divisor;
        (if x < 0 { -rounded } else { rounded }) as i32
    }
    pub fn rounding_shift_right_64(x: i64, s: u8) -> i64 {
        if s == 0 {
            return x;
        }
        let (divisor, half) = (1i128 << s, 1i128 << (s - 1));
        let magnitude = (x as i128).abs();
        let rounded = (magnitude + half) / divisor;
        (if x < 0 { -rounded } else { rounded }) as i64
    }
    fn poly2(p: i32) -> i32 {
        let t = p as i64 + POLY2_B;
        let square = (t * t) >> K;
        (((POLY2_A * square) >> K) + POLY2_C) as i32
    }
    pub fn int_exp(x: i32) -> i32 {
        let x = x.min(0);
        let z = (-(x as i64) / LN2_Q as i64).min(Z_MAX as i64) as i32;
        if z >= Z_MAX {
            return 0;
        }
        rounding_shift_right(poly2(x + z * LN2_Q), z as u8)
    }
    pub fn int_rsqrt(v: i64) -> i64 {
        if v <= 0 {
            return 0;
        }
        let bit = 63 - v.leading_zeros() as i32;
        let mut e = (bit - K as i32).div_euclid(2);
        let mut m = if 2 * e >= 0 { v >> (2 * e) } else { v << (-2 * e) };
        while m >= 4 * ONE {
            m >>= 2;
            e += 1;
        }
        while m < ONE {
            m <<= 2;
            e -= 1;
        }
        let index = (((m - ONE) * 16) / (3 * ONE)).clamp(0, 15) as usize;
        let mut y = RSQRT_SEED[index];
        for _ in 0..3 {
            let y2 = (y * y) >> K;
            let my2 = (m * y2) >> K;
            y = (y * (3 * ONE - my2)) >> (K + 1);
            if y <= 0 {
                y = 1;
            }
        }
        if e >= 0 { y >> e } else { y << (-e) }
    }
    pub fn int_recip(v: i64) -> i64 {
        let r = int_rsqrt(v) as i128;
        ((r * r) >> K) as i64
    }
    pub fn int_sigmoid(x_q: i32) -> i32 {
        let e = int_exp(-(x_q.saturating_abs())) as i64;
        let denominator = ONE + e;
        let recip = int_recip(denominator);
        let numerator = if x_q <= 0 { e } else { ONE };
        ((numerator * recip) >> K) as i32
    }
    pub fn silu(x_q: &[i32]) -> Vec<i32> {
        x_q.iter().map(|v| (((*v as i64) * (int_sigmoid(*v) as i64)) >> K) as i32).collect()
    }
    pub fn softmax_shifted_diff_v1(v: i32, max: i64, up: i64) -> i32 {
        let floor = (i32::MIN as i64) >> up;
        (((v as i64 - max).max(floor)) << up).clamp(i32::MIN as i64, 0) as i32
    }
    pub fn softmax_shifted(logits: &[i32], up_bits: u8) -> Vec<i32> {
        let up = up_bits.min(62) as i64;
        let max = *logits.iter().max().unwrap() as i64;
        let exps: Vec<i64> = logits.iter().map(|v| int_exp(softmax_shifted_diff_v1(*v, max, up)) as i64).collect();
        let sum: i64 = exps.iter().sum();
        if sum <= 0 {
            let uniform = ONE / (logits.len() as i64);
            return vec![uniform as i32; logits.len()];
        }
        let recip = int_recip(sum);
        exps.iter().map(|e| ((e * recip) >> K) as i32).collect()
    }
    pub fn a16_scale_round(x: i64, multiplier: i64, shift: u8) -> i64 {
        let shift = shift.min(62) as u32;
        let p = (x as i128) * (multiplier as i128);
        if shift == 0 {
            return p.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        }
        let half = 1i128 << (shift - 1);
        let rounded = (p.abs() + half) >> shift;
        let signed = if p < 0 { -rounded } else { rounded };
        signed.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }
    pub fn clamp16(x: i64) -> i32 {
        x.clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
    }
    pub fn a16_rms_norm(x: &[i32], eps_q: i64) -> Vec<i32> {
        let sum_sq: i64 = x.iter().map(|v| *v as i64 * *v as i64).sum();
        let mean_q = ((sum_sq as i128) << K) / (x.len() as i128);
        let r = int_rsqrt(mean_q.clamp(0, i64::MAX as i128) as i64 + eps_q);
        x.iter().map(|v| ((*v as i128 * r as i128).clamp(i32::MIN as i128, i32::MAX as i128)) as i32).collect()
    }
    pub fn a16_rope(x: &[i32], cos_q: &[i32], sin_q: &[i32]) -> Vec<i32> {
        let mut out = Vec::with_capacity(x.len());
        for p in 0..x.len() / 2 {
            let (a, b) = (x[2 * p] as i128, x[2 * p + 1] as i128);
            let (c, s) = (cos_q[p] as i128, sin_q[p] as i128);
            out.push(clamp16((((a * c) - (b * s)) >> K).clamp(i64::MIN as i128, i64::MAX as i128) as i64));
            out.push(clamp16((((a * s) + (b * c)) >> K).clamp(i64::MIN as i128, i64::MAX as i128) as i64));
        }
        out
    }
    pub fn q36_int_ln(x: i64) -> Option<i64> {
        if x <= 0 {
            return None;
        }
        let mut m = x;
        let mut s: i64 = 0;
        while m >= 2 * ONE {
            m >>= 1;
            s += 1;
        }
        while m < ONE {
            m <<= 1;
            s -= 1;
        }
        let t = (((m - ONE) as i128) << K) / ((m + ONE) as i128);
        let t = t as i64;
        let t2 = ((t as i128 * t as i128) >> K) as i64;
        let mut term = t;
        let mut sum = t;
        for odd in [3i64, 5, 7, 9, 11] {
            term = ((term as i128 * t2 as i128) >> K) as i64;
            sum += term / odd;
        }
        Some(2 * sum + s * LN2_Q as i64)
    }
    pub fn q36_exp_refined(x: i32) -> i64 {
        let y0 = int_exp(x) as i64;
        if y0 <= 0 {
            return 0;
        }
        let Some(ln_y) = q36_int_ln(y0) else { return y0 };
        let correction = (x as i64 - ln_y).clamp(-(ONE / 4), ONE / 4);
        let adjusted = (y0 as i128) + ((y0 as i128 * correction as i128) >> K);
        adjusted.clamp(0, ONE as i128) as i64
    }
    pub fn q36_softplus(x_q: i32) -> i64 {
        let magnitude = x_q.saturating_abs();
        let e = int_exp(-magnitude) as i64;
        let tail = q36_int_ln(ONE + e).unwrap_or(0);
        if x_q <= 0 { tail } else { x_q as i64 + tail }
    }
    pub fn q36_decay(dt_q: i32, c_q: i64) -> i64 {
        if c_q <= 0 {
            return ONE;
        }
        let sp = q36_softplus(dt_q);
        let arg = ((c_q as i128 * sp as i128) >> K).clamp(0, -(i32::MIN as i128)) as i128;
        q36_exp_refined((-arg) as i32).clamp(0, ONE)
    }
    pub fn q36_l2_norm(x: &[i32]) -> Vec<i32> {
        let sum: i64 = x.iter().map(|v| *v as i64 * *v as i64).sum();
        if sum <= 0 {
            return vec![0; x.len()];
        }
        let bit = 63 - sum.leading_zeros() as i32;
        let e = bit.div_euclid(2);
        let two_e = 2 * e;
        let m_q = if two_e >= K as i32 { sum >> (two_e - K as i32) } else { sum << (K as i32 - two_e) };
        let rsqrt = int_rsqrt(m_q);
        let shift = K as i32 - 15 + e;
        x.iter()
            .map(|v| {
                let product = *v as i128 * rsqrt as i128;
                let scaled = if shift >= 0 { product >> shift } else { product << (-shift) };
                scaled.clamp(-(A16_CODE_MAX as i128), A16_CODE_MAX as i128) as i32
            })
            .collect()
    }
    pub fn q36_rms_norm_wide(x: &[i32], eps_zero: i64, eps_shift: u8) -> Option<Vec<i32>> {
        let n = x.len() as i128;
        let sum: i128 = x.iter().map(|v| (*v as i128) * (*v as i128)).sum();
        if eps_zero < 0 {
            return None;
        }
        let mut mean = ((sum << K) / n) + ((eps_zero as i128) << eps_shift.min(96));
        if mean <= 0 {
            return Some(vec![0; x.len()]);
        }
        let mut halvings: i32 = 0;
        while mean >= 4 * ONE as i128 {
            mean >>= 2;
            halvings += 1;
        }
        while mean < ONE as i128 {
            mean <<= 2;
            halvings -= 1;
            if halvings < -40 {
                return None;
            }
        }
        let r = int_rsqrt(mean as i64) as i128;
        Some(
            x.iter()
                .map(|v| {
                    let product = (*v as i128) * r;
                    let scaled = if halvings >= 0 { product >> halvings } else { product << (-halvings) };
                    scaled.clamp(i32::MIN as i128, i32::MAX as i128) as i32
                })
                .collect(),
        )
    }
    /// `(expert, weight_q)`, sorted by expert.
    pub fn q36_router_topk(logits: &[i32], k: usize, up_bits: u8) -> Vec<(u16, i32)> {
        let experts = logits.len();
        let probs = softmax_shifted(logits, up_bits);
        let mut chosen = Vec::with_capacity(k);
        let mut taken = vec![false; experts];
        for _ in 0..k {
            let mut best = usize::MAX;
            for (i, p) in probs.iter().enumerate() {
                if taken[i] {
                    continue;
                }
                if best == usize::MAX || *p > probs[best] {
                    best = i;
                }
            }
            taken[best] = true;
            chosen.push(best);
        }
        chosen.sort_unstable();
        let sum: i64 = chosen.iter().map(|i| probs[*i] as i64).sum();
        if sum <= 0 {
            let uniform = (ONE / k as i64) as i32;
            return chosen.iter().map(|i| (*i as u16, uniform)).collect();
        }
        let recip = int_recip(sum);
        chosen.iter().map(|i| (*i as u16, ((probs[*i] as i64 * recip) >> K) as i32)).collect()
    }
    pub fn q36_moe_combine(outputs: &[i32], weights: &[i32], width: usize, p: P) -> Vec<i32> {
        let k = outputs.len() / width;
        let mut out = Vec::with_capacity(width);
        for lane in 0..width {
            let acc: i64 = (0..k).map(|e| weights[e] as i64 * outputs[e * width + lane] as i64).sum();
            out.push(a16_scale_round(acc, p.m, p.s).saturating_add(p.z).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32);
        }
        out
    }
    #[derive(Clone, Copy)]
    pub struct P {
        pub m: i64,
        pub s: u8,
        pub z: i64,
    }
    pub fn q36_gdn_step(
        state: &mut [i32],
        d_v: usize,
        d_k: usize,
        k: &[i32],
        v: &[i32],
        q: &[i32],
        decay_q: i64,
        beta_q: i64,
        read: P,
        write_shift: i32,
        delta: P,
        out: P,
    ) -> Vec<i32> {
        const SMAX: i64 = i32::MAX as i64;
        const DMAX: i64 = (1 << 24) - 1;
        for slot in state.iter_mut() {
            *slot = rounding_shift_right_64(*slot as i64 * decay_q, K as u8).clamp(-SMAX, SMAX) as i32;
        }
        let mut u = Vec::with_capacity(d_v);
        for (row, vi) in state.chunks_exact(d_k).zip(v) {
            let acc: i64 = row.iter().zip(k).map(|(a, b)| *a as i64 * *b as i64).sum();
            let w = a16_scale_round(acc, read.m, read.s).saturating_add(read.z);
            let delta_ = (*vi as i64).saturating_sub(w);
            let scaled = rounding_shift_right_64(delta_.saturating_mul(beta_q), K as u8);
            let narrowed = a16_scale_round(scaled, delta.m, delta.s).saturating_add(delta.z);
            u.push(narrowed.clamp(-DMAX, DMAX) as i32);
        }
        for (row, ui) in state.chunks_exact_mut(d_k).zip(&u) {
            let ui = *ui as i64;
            if ui == 0 {
                continue;
            }
            for (slot, kj) in row.iter_mut().zip(k) {
                let product = ui * *kj as i64;
                let write = if write_shift >= 0 {
                    product.saturating_mul(1i64 << write_shift)
                } else {
                    rounding_shift_right_64(product, (-write_shift) as u8)
                };
                *slot = (*slot as i64).saturating_add(write).clamp(-SMAX, SMAX) as i32;
            }
        }
        let _ = d_v;
        state
            .chunks_exact(d_k)
            .map(|row| {
                let acc: i64 = row.iter().zip(q).map(|(a, b)| *a as i64 * *b as i64).sum();
                a16_scale_round(acc, out.m, out.s).saturating_add(out.z).clamp(i32::MIN as i64, i32::MAX as i64) as i32
            })
            .collect()
    }
}

fn i128s(v: &[i32]) -> Vec<i128> {
    v.iter().map(|x| *x as i128).collect()
}

fn i64s(v: &[i64]) -> Vec<i128> {
    v.iter().map(|x| *x as i128).collect()
}

fn codes(rng: &mut Lcg, n: usize) -> Vec<i32> {
    (0..n)
        .map(|i| match i % 13 {
            0 => 32767,
            1 => -32767,
            2 => 0,
            _ => rng.range(-32767, 32767) as i32,
        })
        .collect()
}

fn q24s(rng: &mut Lcg, n: usize) -> Vec<i32> {
    (0..n)
        .map(|i| match i % 11 {
            0 => i32::MIN,
            1 => i32::MAX,
            2 => 0,
            3 => 1,
            4 => -1,
            5 => rng.range(-(16 << 24), 16 << 24) as i32,
            _ => rng.range(-(8 << 24), 8 << 24) as i32,
        })
        .collect()
}

#[test]
fn the_a16_narrowing_is_a16_scale_round_then_the_saturating_zero() {
    let mut rng = Lcg(7);
    let n = 600;
    let mut x = Vec::new();
    let mut m = Vec::new();
    let mut s = Vec::new();
    let mut z = Vec::new();
    let extremes_x = [i64::MIN, i64::MAX, 0, 1, -1, 1 << 53, -(1 << 53)];
    let extremes_m = [i64::MIN, i64::MAX, 1, -1, 0, 1 << 30];
    let extremes_z = [i64::MIN, i64::MAX, 0, -5, 32767];
    for i in 0..n {
        x.push(if i % 5 == 0 { extremes_x[i % 7] } else { rng.range(-(1 << 60), 1 << 60) as i64 });
        m.push(if i % 3 == 0 { extremes_m[i % 6] } else { rng.range(-(1 << 40), 1 << 40) as i64 });
        s.push((i % 63) as i8);
        z.push(if i % 4 == 0 { extremes_z[i % 5] } else { rng.range(-40000, 40000) as i64 });
    }
    for (lo, hi, dt) in [(-32767i64, 32767i64, DType::I16), (i32::MIN as i64, i32::MAX as i64, DType::I32)] {
        let want: Vec<i128> =
            (0..n).map(|i| (legacy::a16_scale_round(x[i], m[i], s[i] as u8).saturating_add(z[i])).clamp(lo, hi) as i128).collect();
        let got = eval_graph(
            &[
                arg("x", DType::I64, i64s(&x)),
                arg("m", DType::I64, i64s(&m)),
                arg("s", DType::I8, s.iter().map(|v| *v as i128).collect()),
                arg("z", DType::I64, i64s(&z)),
            ],
            |b, r| {
                let p2 = b.pow2_of(r[2]);
                b.narrow_a16(r[0], r[1], p2, r[3], lo, hi, dt)
            },
        )
        .unwrap();
        assert_eq!(got.data, want);
    }
}

#[test]
fn int_sigmoid_and_silu_are_the_base0_ops() {
    let mut rng = Lcg(11);
    let x = q24s(&mut rng, 2000);
    let sig = eval_graph(&[arg("x", DType::I32, i128s(&x))], |b, r| b.int_sigmoid(r[0])).unwrap();
    assert_eq!(sig.data, x.iter().map(|v| legacy::int_sigmoid(*v) as i128).collect::<Vec<_>>());
    let silu = eval_graph(&[arg("x", DType::I32, i128s(&x))], |b, r| b.silu(r[0])).unwrap();
    assert_eq!(silu.data, i128s(&legacy::silu(&x)));
}

#[test]
fn softmax_shifted_is_op_5w_at_every_width() {
    let mut rng = Lcg(13);
    for up in [0u8, 1, 2, 14, 25, 31, 46, 47, 48, 62] {
        for len in [1usize, 2, 7, 33] {
            let row: Vec<i32> = (0..len).map(|i| if i == 0 && len > 1 { -32767 } else { rng.range(-32767, 32767) as i32 }).collect();
            let got = eval_graph(&[arg("x", DType::I32, i128s(&row))], |b, r| b.softmax_shifted(r[0], up as u32)).unwrap();
            assert_eq!(got.data, i128s(&legacy::softmax_shifted(&row, up)), "up {up} len {len}");
        }
    }
    // The audit regression: a key 40,000 below the max at up = 48 must not get the max's weight.
    let got = eval_graph(&[arg("x", DType::I32, vec![32767, -7233])], |b, r| b.softmax_shifted(r[0], 48)).unwrap();
    assert!(got.data[0] > got.data[1]);
}

#[test]
fn rms_norm_a16_and_rope_a16_are_the_a16_ops() {
    let mut rng = Lcg(17);
    for n in [1usize, 4, 16, 64] {
        for eps in [1i64, 7, 1 << 20] {
            let x = codes(&mut rng, n);
            let got = eval_graph(&[arg("x", DType::I16, i128s(&x))], |b, r| b.rms_norm_a16(r[0], eps)).unwrap();
            assert_eq!(got.data, i128s(&legacy::a16_rms_norm(&x, eps)), "n {n} eps {eps}");
        }
    }
    // A zero row is eps-defined, not a division by zero.
    let got = eval_graph(&[arg("x", DType::I16, vec![0; 8])], |b, r| b.rms_norm_a16(r[0], 1)).unwrap();
    assert_eq!(got.data, vec![0; 8]);
    for pairs in [1usize, 2, 8] {
        let x = codes(&mut rng, 2 * pairs);
        let cos: Vec<i32> = (0..pairs).map(|i| if i == 0 { i32::MIN } else { rng.range(-(1 << 24), 1 << 24) as i32 }).collect();
        let sin: Vec<i32> = (0..pairs).map(|i| if i == 0 { i32::MAX } else { rng.range(-(1 << 24), 1 << 24) as i32 }).collect();
        let got = eval_graph(
            &[arg("x", DType::I16, i128s(&x)), arg("cos", DType::I32, i128s(&cos)), arg("sin", DType::I32, i128s(&sin))],
            |b, r| b.rope_pairs(r[0], r[1], r[2], -32767, 32767, DType::I16),
        )
        .unwrap();
        assert_eq!(got.data, i128s(&legacy::a16_rope(&x, &cos, &sin)), "pairs {pairs}");
    }
}

#[test]
fn int_ln_is_the_q36_logarithm_and_the_decay_chain_is_q36_decay() {
    let mut rng = Lcg(19);
    let mut xs: Vec<i64> = vec![1, 2, 3, 100, (1 << 24) - 1, 1 << 24, (1 << 24) + 1, 1 << 25, i64::MAX, i64::MAX - 1, 1 << 62];
    for _ in 0..2000 {
        xs.push(rng.range(1, 1 << 40) as i64);
    }
    let got = eval_graph(&[arg("x", DType::I64, i64s(&xs))], |b, r| b.int_ln(r[0])).unwrap();
    assert_eq!(got.data, xs.iter().map(|x| legacy::q36_int_ln(*x).unwrap() as i128).collect::<Vec<_>>());
    // Refined exponential, softplus, decay.
    let x = q24s(&mut rng, 1500);
    let neg: Vec<i32> = x.iter().map(|v| v.saturating_abs().saturating_neg()).collect();
    let got = eval_graph(&[arg("x", DType::I32, i128s(&neg))], |b, r| b.exp_refined_q36(r[0])).unwrap();
    assert_eq!(got.data, neg.iter().map(|v| legacy::q36_exp_refined(*v) as i128).collect::<Vec<_>>());
    let got = eval_graph(&[arg("x", DType::I32, i128s(&x))], |b, r| b.softplus_q36(r[0])).unwrap();
    assert_eq!(got.data, x.iter().map(|v| legacy::q36_softplus(*v) as i128).collect::<Vec<_>>());
    let c: Vec<i64> = (0..x.len())
        .map(|i| match i % 7 {
            0 => 0,
            1 => -5,
            2 => i64::MAX,
            _ => rng.range(1, 64 << 24) as i64,
        })
        .collect();
    let got = eval_graph(&[arg("dt", DType::I32, i128s(&x)), arg("c", DType::I64, i64s(&c))], |b, r| b.decay_q36(r[0], r[1])).unwrap();
    assert_eq!(got.data, x.iter().zip(&c).map(|(d, c)| legacy::q36_decay(*d, *c) as i128).collect::<Vec<_>>());
}

#[test]
fn l2_norm_q15_takes_the_exponent_out_like_q36_l2_norm() {
    let mut rng = Lcg(23);
    for n in [1usize, 2, 4, 16, 128] {
        for trial in 0..6 {
            let x: Vec<i32> = match trial {
                0 => vec![0; n],
                1 => vec![1; n],
                2 => vec![32767; n],
                3 => (0..n).map(|i| if i == 0 { -32767 } else { 0 }).collect(),
                _ => codes(&mut rng, n).iter().map(|v| v >> (trial * 3)).collect(),
            };
            let got = eval_graph(&[arg("x", DType::I16, i128s(&x))], |b, r| b.l2_norm_q15(r[0])).unwrap();
            assert_eq!(got.data, i128s(&legacy::q36_l2_norm(&x)), "n {n} trial {trial}");
        }
    }
}

/// The gated delta rule over several positions, heads vectorised: the IR segment's state and
/// output equal `q36_gdn_step` run head by head, position by position.
#[test]
fn the_gated_delta_step_is_q36_gdn_step() {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{ConeEnv, Interpreter, MapParams, Ref, Tensor};
    let (h, dv, dk) = (3u32, 4u32, 4u32);
    let smax = i32::MAX as i64;
    let mut rng = Lcg(29);
    // Per-head narrowings inside the legacy domain (|m| ≤ 2^30, ws ∈ [-62, 20]).
    let read: Vec<legacy::P> = (0..h)
        .map(|i| legacy::P { m: [1, 1 << 20, -(1 << 30)][i as usize], s: [15, 30, 40][i as usize], z: [0, -3, 7][i as usize] })
        .collect();
    let delta: Vec<legacy::P> =
        (0..h).map(|i| legacy::P { m: [1, 1 << 10, 3][i as usize], s: [0, 8, 1][i as usize], z: [0, 1, -1][i as usize] }).collect();
    let out: Vec<legacy::P> =
        (0..h).map(|i| legacy::P { m: [1, 1 << 12, -7][i as usize], s: [15, 20, 3][i as usize], z: [0, 5, -5][i as usize] }).collect();
    let ws: Vec<i32> = vec![-9, 3, -62];
    let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let pk = pb.param("k", DType::I16, &[h, dk], false);
    let pv = pb.param("v", DType::I16, &[h, dv], false);
    let pq = pb.param("q", DType::I16, &[h, dk], false);
    let pdec = pb.param("decay", DType::I32, &[h], false);
    let pbeta = pb.param("beta", DType::I32, &[h], false);
    let trip = |pb: &mut ProgramBuilder, n: &str| {
        (
            pb.param(&format!("{n}.m"), DType::I64, &[h], false),
            pb.param(&format!("{n}.s"), DType::I8, &[h], false),
            pb.param(&format!("{n}.z"), DType::I64, &[h], false),
        )
    };
    let (rm, rs, rz) = trip(&mut pb, "read");
    let (dm, ds, dz) = trip(&mut pb, "delta");
    let (om, os, oz) = trip(&mut pb, "out");
    let pws = pb.param("ws", DType::I32, &[h], false);
    let st = pb.fixed_state("S", DType::I32, &[h, dv, dk], -smax, smax, false);
    let (o_node, pre) = {
        let mut b = pb.block("pre", vec![]);
        let (rsp, dsp, osp) = (b.pow2_of(rs), b.pow2_of(ds), b.pow2_of(os));
        let o = b.gdn_step_q36(st, pk, pv, pq, pdec, pbeta, (rm, rsp, rz), (dm, dsp, dz), pws, (om, osp, oz));
        let o = b.commit(o);
        (o, b.finish(&[o]))
    };
    let carry = pb.blocks[pre as usize].nodes[match o_node {
        Ref::Node(i) => i as usize,
        _ => unreachable!(),
    }]
    .out
    .clone();
    let post = {
        let mut b = pb.block("post", vec![carry.clone()]);
        let l = b.reshape(Ref::CarryIn(0), &carry.shape);
        b.commit(l);
        b.finish(&[])
    };
    let program = pb.finish(pre, vec![], post, 0);
    let interp = Interpreter::new(&program).expect("valid");
    let mut legacy_state = vec![vec![0i32; (dv * dk) as usize]; h as usize];
    let mut run = misaka_palw_tir::RunState::default();
    for pos in 0..12 {
        let k = codes(&mut rng, (h * dk) as usize);
        let v = codes(&mut rng, (h * dv) as usize);
        let q = codes(&mut rng, (h * dk) as usize);
        let decay: Vec<i32> = (0..h).map(|i| if pos % 4 == 0 && i == 0 { TONE as i32 } else { rng.range(0, TONE) as i32 }).collect();
        let beta: Vec<i32> = (0..h).map(|_| rng.range(0, TONE) as i32).collect();
        let mut params = MapParams::default();
        let put = |params: &mut MapParams, j: u16, dt: DType, shape: Vec<usize>, v: Vec<i128>| {
            params.tensors.insert((j, None), Tensor::new(dt, shape, v).unwrap());
        };
        put(&mut params, 0, DType::I16, vec![h as usize, dk as usize], i128s(&k));
        put(&mut params, 1, DType::I16, vec![h as usize, dv as usize], i128s(&v));
        put(&mut params, 2, DType::I16, vec![h as usize, dk as usize], i128s(&q));
        put(&mut params, 3, DType::I32, vec![h as usize], i128s(&decay));
        put(&mut params, 4, DType::I32, vec![h as usize], i128s(&beta));
        for (base, t) in [(5u16, &read), (8, &delta), (11, &out)] {
            put(&mut params, base, DType::I64, vec![h as usize], t.iter().map(|p| p.m as i128).collect());
            put(&mut params, base + 1, DType::I8, vec![h as usize], t.iter().map(|p| p.s as i128).collect());
            put(&mut params, base + 2, DType::I64, vec![h as usize], t.iter().map(|p| p.z as i128).collect());
        }
        put(&mut params, 14, DType::I32, vec![h as usize], i128s(&ws));
        let step = interp.step(&params, &mut run, 0).expect("step");
        let mut want = Vec::new();
        for hh in 0..h as usize {
            let (a, b2) = (hh * dk as usize, (hh + 1) * dk as usize);
            let (c, d) = (hh * dv as usize, (hh + 1) * dv as usize);
            want.extend(legacy::q36_gdn_step(
                &mut legacy_state[hh],
                dv as usize,
                dk as usize,
                &k[a..b2],
                &v[c..d],
                &q[a..b2],
                decay[hh] as i64,
                beta[hh] as i64,
                read[hh],
                ws[hh],
                delta[hh],
                out[hh],
            ));
        }
        assert_eq!(step.logits.data, i128s(&want), "output at position {pos}");
        let s = run.fixed.get(&(st, None)).expect("the state was written");
        let flat: Vec<i32> = legacy_state.iter().flatten().copied().collect();
        assert_eq!(s.data, i128s(&flat), "state after position {pos}");
    }
    let _ = (TK, Rounding::Floor, ConeEnv::default());
}

#[test]
fn rms_norm_wide_is_q36_rms_norm_wide() {
    let mut rng = Lcg(31);
    for n in [1usize, 4, 32, 128] {
        for trial in 0..8 {
            let x: Vec<i32> = match trial {
                0 => vec![0; n],
                1 => vec![i32::MIN; n],
                2 => vec![i32::MAX; n],
                3 => (0..n).map(|i| if i == 0 { 1 } else { 0 }).collect(),
                _ => (0..n).map(|_| rng.range(i32::MIN as i128, i32::MAX as i128) as i32 >> (trial * 4)).collect(),
            };
            for (ez, es) in [(0i64, 0u8), (1, 0), (1, 24), (3, 60), (1_000_000, 96), (5, 200)] {
                let want = legacy::q36_rms_norm_wide(&x, ez, es).expect("in the legacy domain");
                let got = eval_graph(
                    &[
                        arg("x", DType::I32, i128s(&x)),
                        arg("ez", DType::I64, vec![ez as i128]),
                        arg("es", DType::I16, vec![es as i128]),
                    ],
                    |b, r| b.rms_norm_wide_q36(r[0], r[1], r[2]),
                );
                // `x` has n lanes and the eps params one: pad eps to n by broadcast inside the graph.
                let got = match got {
                    Ok(t) => t,
                    Err(e) => panic!("n {n} trial {trial} eps ({ez}, {es}): {e}"),
                };
                assert_eq!(got.data, i128s(&want), "n {n} trial {trial} eps ({ez}, {es})");
            }
        }
    }
}

#[test]
fn router_and_combine_are_q36_router_topk_and_q36_moe_combine() {
    let mut rng = Lcg(37);
    for (experts, k) in [(8usize, 2u32), (16, 4), (4, 4), (32, 8)] {
        for up in [0u8, 2, 16, 31] {
            let mut logits: Vec<i32> = (0..experts).map(|_| rng.range(-32767, 32767) as i32).collect();
            if up == 2 {
                // Exact ties and underflowing tails: only the index rule decides.
                for (i, l) in logits.iter_mut().enumerate() {
                    *l = if i % 3 == 0 { 30000 } else { -32767 };
                }
            }
            let want = legacy::q36_router_topk(&logits, k as usize, up);
            // The legacy committed row: expert ids, then weights.
            let row = eval_graph(&[arg("l", DType::I16, i128s(&logits))], |b, r| {
                let (idx, w) = b.router_topk_q36(r[0], k, up as u32);
                let ids = b.cast(idx, DType::I32);
                b.concat(&[ids, w], 0)
            })
            .unwrap();
            let mut want_row: Vec<i128> = want.iter().map(|(e, _)| *e as i128).collect();
            want_row.extend(want.iter().map(|(_, w)| *w as i128));
            assert_eq!(row.data, want_row, "experts {experts} k {k} up {up}");
            // The combine over those weights, with wide expert rows and an adversarial narrowing.
            let width = 6usize;
            let rows: Vec<i32> =
                (0..k as usize * width).map(|i| if i % 7 == 0 { i32::MIN } else { rng.range(-(1 << 30), 1 << 30) as i32 }).collect();
            let ws: Vec<i32> = want.iter().map(|(_, w)| *w).collect();
            for p in [
                legacy::P { m: 1, s: 24, z: 0 },
                legacy::P { m: i64::MAX, s: 3, z: -9 },
                legacy::P { m: -(1 << 40), s: 62, z: i64::MIN },
            ] {
                let want_c = legacy::q36_moe_combine(&rows, &ws, width, p);
                let got = eval_graph(
                    &[
                        arg("rows", DType::I32, i128s(&rows)),
                        arg("w", DType::I32, i128s(&ws)),
                        arg("m", DType::I64, vec![p.m as i128]),
                        arg("s", DType::I8, vec![p.s as i128]),
                        arg("z", DType::I64, vec![p.z as i128]),
                    ],
                    |b, r| {
                        let y = b.reshape_fixed(r[0], &[k, width as u32]);
                        let p2 = b.pow2_of(r[3]);
                        b.moe_combine_q36(y, r[1], r[2], p2, r[4], -32767, 32767, DType::I16)
                    },
                )
                .unwrap();
                assert_eq!(got.data, i128s(&want_c));
            }
        }
    }
}
