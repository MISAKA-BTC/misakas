//! **The legacy kernels as segments** (spec 04b §11.1): every integer kernel of the live court's
//! catalogue — BASE-0, the A16 dense tier, Qwen3.6's own ops, and the fenced `RequantizeByToken` —
//! written as plain PALW-TIR.
//!
//! Each template names the live function it reproduces. Byte identity is claimed on the live
//! kernel's own domain (the inputs it accepts rather than refuses) and is TESTED against the live
//! code by `misaka-palw-tir-conformance`. Outside that domain the live kernel refuses (a non-A16
//! lane, a shift past its range) and the segment computes a defined value; in an IR class that
//! question does not arise, because a committed operand outside its node's proven interval is a
//! malformed commitment (PALW-TIR-33).
//!
//! Two legacy rules have no node at all: `softmax`'s and the router's uniform fallback for a
//! non-positive sum (the row maximum contributes `IntExp(0) > 0`, so the branch is dead), and
//! `int_rsqrt`'s renormalisation loops (the first shift already lands the mantissa in range).

use crate::arith::{K, ONE};
use crate::builder::BlockBuilder;
use crate::library::Narrowing;
use crate::prim::{Cmp, Rounding};
use crate::program::Ref;
use crate::types::{DType, Dim};

/// `q36_matmul_grouped`'s group width (Q4_K's granularity).
pub const Q36_WEIGHT_GROUP: u32 = 32;
/// `QWEN36_MAX_GROUP_EXP`: the largest per-group exponent the live kernel accepts.
pub const Q36_MAX_GROUP_EXP: u32 = 20;

fn fixed_last(b: &BlockBuilder<'_>, x: Ref) -> u32 {
    match b.shape(x).last() {
        Some(Dim::Fixed(n)) => *n,
        _ => panic!("a row operation needs a static last axis"),
    }
}

impl BlockBuilder<'_> {
    // ---- the rounding rules -------------------------------------------------------------------

    /// The A16 narrowing (`palw_base0_a16::a16_scale_round` then `saturating_add(zero)` then a
    /// clamp): `clamp_[lo,hi]( sat64( sat64(HAFZ(x·m / 2^s)) + z ) )`, written as
    /// `Mul→i128, Div(HAFZ)→i128, Clamp→i64, Add→i128, Clamp→out`. `pow2_s` is the divisor `2^s`
    /// (a const, or [`Self::pow2_of`] of a shift tensor); [`Self::narrow`] takes the shift itself.
    #[allow(clippy::too_many_arguments)]
    pub fn narrow_a16(&mut self, x: Ref, m: Ref, pow2_s: Ref, z: Ref, lo: i64, hi: i64, dtype: DType) -> Ref {
        let p = self.mul(x, m, DType::I128);
        let q = self.div(p, pow2_s, Rounding::HalfAwayFromZero, DType::I128);
        let r = self.clamp(q, i64::MIN, i64::MAX, DType::I64);
        let t = self.add(r, z, DType::I128);
        self.clamp(t, lo, hi, dtype)
    }

    /// BASE-0 op 2 (`palw_base0::requantize_with_zero`) at a static shift:
    /// `clamp_[-128,127]( sat32( RSR( SRDHM(acc, m), min(s, 31) ) + z ) )` with SRDHM as
    /// `sat32(HalfUp(acc·m / 2^31))`.
    pub fn requantize_base0(&mut self, acc: Ref, m: Ref, shift: u32, z: Ref) -> Ref {
        let p = self.mul(acc, m, DType::I64);
        let h = self.shr(p, 31, Rounding::HalfUp, DType::I64);
        let srdhm = self.clamp(h, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let r = self.shr(srdhm, shift.min(31), Rounding::HalfAwayFromZero, DType::I32);
        let t = self.add(r, z, DType::I64);
        self.clamp(t, -128, 127, DType::I8)
    }

    /// [`Self::requantize_base0`] with the shift a tensor (per channel): `s` is clamped into
    /// `[0, 31]`, the kernel's own `rounding_shift_right` clamp.
    pub fn requantize_base0_t(&mut self, acc: Ref, m: Ref, s: Ref, z: Ref) -> Ref {
        let p = self.mul(acc, m, DType::I64);
        let h = self.shr(p, 31, Rounding::HalfUp, DType::I64);
        let srdhm = self.clamp(h, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let s = self.clamp(s, 0, 31, DType::I8);
        let d = self.pow2_of(s);
        let r = self.div(srdhm, d, Rounding::HalfAwayFromZero, DType::I32);
        let t = self.add(r, z, DType::I64);
        self.clamp(t, -128, 127, DType::I8)
    }

    /// BASE-0 op 9 (`palw_base0::rescale_q`): `sat32( RSR64(acc·m, min(s, 62)) )`.
    pub fn rescale_base0(&mut self, acc: Ref, m: Ref, shift: u32) -> Ref {
        let p = self.mul(acc, m, DType::I64);
        let r = self.shr(p, shift.min(62), Rounding::HalfAwayFromZero, DType::I64);
        self.clamp(r, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// [`Self::rescale_base0`] with the shift a tensor (clamped into `[0, 62]`).
    pub fn rescale_base0_t(&mut self, acc: Ref, m: Ref, s: Ref) -> Ref {
        let p = self.mul(acc, m, DType::I64);
        let d = self.pow2_of(s);
        let r = self.div(p, d, Rounding::HalfAwayFromZero, DType::I64);
        self.clamp(r, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// `IntRecip` of 04a F2, composed: `(IntRsqrt(v)² ) >> 24` (`palw_base0::int_recip`).
    pub fn int_recip(&mut self, v: Ref) -> Ref {
        let r = self.int_rsqrt(v);
        let rr = self.mul(r, r, DType::I128);
        self.shr(rr, K, Rounding::Floor, DType::I64)
    }

    /// `palw_base0_ops::int_sigmoid` on Q24 `i32` values (and `q36_sigmoid_gate`, lane by lane).
    pub fn int_sigmoid(&mut self, x: Ref) -> Ref {
        let zero = self.c(DType::I32, 0);
        let pos = self.compare(x, zero, Cmp::Gt);
        let neg = self.sub(zero, x, DType::I64);
        let neg_abs = self.select(pos, neg, x, DType::I64);
        let e = self.int_exp(neg_abs);
        let one = self.c(DType::I32, ONE);
        let den = self.add(e, one, DType::I64);
        let recip = self.int_recip(den);
        let le = self.compare(x, zero, Cmp::Le);
        let num = self.select(le, e, one, DType::I64);
        let prod = self.mul(num, recip, DType::I64);
        self.shr(prod, K, Rounding::Floor, DType::I32)
    }

    /// `palw_base0_ops::silu` (BASE-0 op 6, `q36/silu`): `(x · IntSigmoid(x)) >> 24`, Q24 in and out.
    pub fn silu(&mut self, x: Ref) -> Ref {
        let s = self.int_sigmoid(x);
        let p = self.mul(x, s, DType::I64);
        let q = self.shr(p, K, Rounding::Floor, DType::I64);
        self.clamp(q, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// `palw_base0_ops::softmax_shifted` along the LAST axis of an `i32`/`i16` row (op 5W;
    /// `up_bits = 0` is op 5, `softmax`, whose `saturating_sub` is the same clamp): the maximum
    /// first, the difference clamped at `i32::MIN >> up` BEFORE the widening, `IntExp`, an exact
    /// sum, `IntRecip`, `(e·recip) >> 24`. Returns Q24 `i32`. Row-wise over the leading axes, which
    /// is `a16_softmax_rows`.
    ///
    /// The final clamp to `[0, 2^25]` never fires (the row maximum contributes `IntExp(0)` to the
    /// sum); it exists so range analysis can type the probabilities.
    pub fn softmax_shifted(&mut self, x: Ref, up_bits: u32) -> Ref {
        let axis = self.shape(x).len() - 1;
        let up = up_bits.min(62);
        let max = self.reduce_max(x, axis);
        let diff = self.sub(x, max, DType::I64);
        let floor = (i32::MIN as i64) >> up;
        let d = self.clamp(diff, floor, 0, DType::I64);
        let scale = self.c(DType::I64, 1i128 << up);
        let w = self.mul(d, scale, DType::I64);
        let arg = self.clamp(w, i32::MIN as i64, 0, DType::I32);
        let e = self.int_exp(arg);
        let sum = self.reduce_sum(e, axis, DType::I64);
        let recip = self.int_recip(sum);
        let p = self.mul(e, recip, DType::I128);
        let q = self.shr(p, K, Rounding::Floor, DType::I64);
        self.clamp(q, 0, 1 << 25, DType::I32)
    }

    // ---- BASE-0 (ADR-0040 Decision D) ---------------------------------------------------------

    /// BASE-0 op 0 and the A16 gather: the table's row at `index` (`Gather` along axis 0). An `i8`
    /// row commits sign-extended into 4-byte lanes, which is the lane layout the live arms write.
    pub fn embed(&mut self, table: Ref, index: Ref) -> Ref {
        self.gather(table, index, 0, 0)
    }

    /// BASE-0 op 1 (`palw_base0_ops::matmul_quant`): `W:i8[out, n] · x:i8[n]`, exact, `i32`.
    pub fn base0_matmul(&mut self, w: Ref, x: Ref) -> Ref {
        let n = fixed_last(self, x);
        let Dim::Fixed(out) = self.shape(w)[0] else { panic!("static weight") };
        let xc = self.reshape_fixed(x, &[n, 1]);
        let acc = self.matmul(w, xc, DType::I32);
        self.reshape_fixed(acc, &[out])
    }

    /// BASE-0 op 3 (`palw_base0_ops::rms_norm`) along the last axis of an `i8` row: `Σx²` exact,
    /// `mean = (Σx² · 2^24) / n`, `r = IntRsqrt(sat64(mean + eps))`, `sat32(x·r)`.
    pub fn base0_rms_norm(&mut self, x: Ref, eps_q: i64) -> Ref {
        let s = self.shape(x);
        let axis = s.len() - 1;
        let n = fixed_last(self, x);
        let sq = self.mul(x, x, DType::I32);
        let sum = self.reduce_sum(sq, axis, DType::I64);
        let one = self.c(DType::I64, ONE);
        let scaled = self.mul(sum, one, DType::I64);
        let nn = self.c(DType::I64, n as i128);
        let mean = self.div(scaled, nn, Rounding::Floor, DType::I64);
        let eps = self.c(DType::I64, eps_q as i128);
        let v = self.add(mean, eps, DType::I128);
        let v = self.clamp(v, i64::MIN, i64::MAX, DType::I64);
        let r = self.int_rsqrt(v);
        let y = self.mul(x, r, DType::I64);
        self.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// BASE-0 ops 7 and 8 (`mul_elem`, `add_elem`): exact, into `i32`.
    pub fn base0_mul_elem(&mut self, a: Ref, b: Ref) -> Ref {
        self.mul(a, b, DType::I32)
    }
    pub fn base0_add_elem(&mut self, a: Ref, b: Ref) -> Ref {
        self.add(a, b, DType::I32)
    }

    // ---- the A16 dense tier (palw_base0_a16) -------------------------------------------------

    /// `a16_rms_norm` along the last axis: the unit row in Q24 from A16 codes.
    /// `mean = floor((Σx² · 2^24) / n)` in i128, `r = IntRsqrt(clamp(mean) + eps)`, `clamp32(x·r)`.
    /// Applied to a `[heads, d]` reshape it is the head-sliced norm (`KDESC_A16_RMS_NORM` on one
    /// head, the hybrid's QK-norm).
    pub fn rms_norm_a16(&mut self, x: Ref, eps_q: i64) -> Ref {
        let s = self.shape(x);
        let axis = s.len() - 1;
        let n = fixed_last(self, x);
        let sq = self.mul(x, x, DType::I64);
        let sum = self.reduce_sum(sq, axis, DType::I64);
        let one = self.c(DType::I64, ONE);
        let scaled = self.mul(sum, one, DType::I128);
        let nn = self.c(DType::I64, n as i128);
        let mean = self.div(scaled, nn, Rounding::Floor, DType::I128);
        let mean = self.clamp(mean, 0, i64::MAX, DType::I64);
        let eps = self.c(DType::I64, eps_q as i128);
        let v = self.add(mean, eps, DType::I64);
        let r = self.int_rsqrt(v);
        let y = self.mul(x, r, DType::I128);
        self.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// `a16_matmul_requant` (`wide = false`, codes out) and `a16_matmul_rescale` (`wide = true`,
    /// the `i32` rail): `N(W:i8[out, n] · x:code[n]; m, s, z)` per output channel, one exact `i64`
    /// accumulator per row.
    pub fn a16_matmul(&mut self, w: Ref, x: Ref, n: &Narrowing, wide: bool) -> Ref {
        let k = fixed_last(self, x);
        let Dim::Fixed(out) = self.shape(w)[0] else { panic!("static weight") };
        let xc = self.reshape_fixed(x, &[k, 1]);
        let acc = self.matmul(w, xc, DType::I64);
        let acc = self.reshape_fixed(acc, &[out]);
        if wide { self.narrow_wide(acc, n) } else { self.narrow_codes(acc, n) }
    }

    /// `a16_add_elem` / `a16_mul_elem`: exact, into `i32` lanes.
    pub fn a16_add_elem(&mut self, a: Ref, b: Ref) -> Ref {
        self.add(a, b, DType::I32)
    }
    pub fn a16_mul_elem(&mut self, a: Ref, b: Ref) -> Ref {
        self.mul(a, b, DType::I32)
    }

    /// RoPE on adjacent pairs `(2p, 2p+1)` of the last axis (`palw_base0_a16::a16_rope`):
    /// `(a·c − b·s) >> 24`, `(a·s + b·c) >> 24` (floor), clamped to `[lo, hi]`. `cos`/`sin` are
    /// Q24 rows of `last/2` entries, broadcast over the leading axes. The products and sums are
    /// `i64`, which holds them for code-width `x` against any `i32` table.
    pub fn rope_pairs(&mut self, x: Ref, cos: Ref, sin: Ref, lo: i64, hi: i64, dtype: DType) -> Ref {
        self.rope_pairs_at(x, cos, sin, lo, hi, dtype, DType::I64)
    }

    /// BASE-0 op 4 (`palw_base0_ops::rope_table`): [`Self::rope_pairs`] for a FULL-range `i32`
    /// row, whose two products sum to `2^63` — the sums are `i128`, as the kernel's.
    pub fn rope_pairs_wide(&mut self, x: Ref, cos: Ref, sin: Ref, lo: i64, hi: i64, dtype: DType) -> Ref {
        self.rope_pairs_at(x, cos, sin, lo, hi, dtype, DType::I128)
    }

    #[allow(clippy::too_many_arguments)]
    fn rope_pairs_at(&mut self, x: Ref, cos: Ref, sin: Ref, lo: i64, hi: i64, dtype: DType, sum: DType) -> Ref {
        let s = self.shape(x);
        let r = s.len();
        let d = fixed_last(self, x);
        let mut pairs_shape = s.clone();
        pairs_shape[r - 1] = Dim::Fixed(d / 2);
        pairs_shape.push(Dim::Fixed(2));
        let xp = self.reshape(x, &pairs_shape);
        let a = self.slice(xp, r, 0, 1);
        let b = self.slice(xp, r, 1, 1);
        let mut half = s.clone();
        half[r - 1] = Dim::Fixed(d / 2);
        let a = self.reshape(a, &half);
        let b = self.reshape(b, &half);
        let ac = self.mul(a, cos, DType::I64);
        let bs = self.mul(b, sin, DType::I64);
        let as_ = self.mul(a, sin, DType::I64);
        let bc = self.mul(b, cos, DType::I64);
        let re = self.sub(ac, bs, sum);
        let im = self.add(as_, bc, sum);
        let re = self.shr(re, K, Rounding::Floor, sum);
        let im = self.shr(im, K, Rounding::Floor, sum);
        let re = self.clamp(re, lo, hi, dtype);
        let im = self.clamp(im, lo, hi, dtype);
        let mut one = half.clone();
        one.push(Dim::Fixed(1));
        let re = self.reshape(re, &one);
        let im = self.reshape(im, &one);
        let both = self.concat(&[re, im], r);
        self.reshape(both, &s)
    }

    /// `a16_attn_scores`: `q:code[heads·d]` against the key window `K:code[H, kv_heads·d]` (a
    /// `HistAppend`'s value; `[H, kv_heads, d]` also reads), grouped-query — head `h` reads
    /// kv head `h / (heads / kv_heads)` — and narrowed to codes by ONE registered narrowing
    /// (tiled, as the live registration is). Returns `code[heads, H]`, head-major.
    pub fn a16_attn_scores(&mut self, q: Ref, k: Ref, heads: u32, kv_heads: u32, d: u32, n: &Narrowing) -> Ref {
        let g = heads / kv_heads;
        let q3 = self.reshape_fixed(q, &[kv_heads, g, d]);
        let k3 = self.reshape(k, &[Dim::H, Dim::Fixed(kv_heads), Dim::Fixed(d)]);
        let kt = self.transpose(k3, &[1, 2, 0]);
        let s = self.matmul(q3, kt, DType::I64);
        let s = self.narrow_codes(s, n);
        self.reshape(s, &[Dim::Fixed(heads), Dim::H])
    }

    /// `a16_attn_values_within`: the probability codes `p:code[heads, H]` against the value window
    /// `V:code[H, kv_heads·d]`, one exact `i64` sum over the history per lane, narrowed to codes by
    /// ONE registered narrowing. Returns `code[heads·d]`.
    pub fn a16_attn_values(&mut self, p: Ref, v: Ref, heads: u32, kv_heads: u32, d: u32, n: &Narrowing) -> Ref {
        let g = heads / kv_heads;
        let p3 = self.reshape(p, &[Dim::Fixed(kv_heads), Dim::Fixed(g), Dim::H]);
        let v3 = self.reshape(v, &[Dim::H, Dim::Fixed(kv_heads), Dim::Fixed(d)]);
        let vt = self.transpose(v3, &[1, 0, 2]);
        let o = self.matmul(p3, vt, DType::I64);
        let o = self.narrow_codes(o, n);
        self.reshape_fixed(o, &[heads * d])
    }

    /// `a16_attn_fused_reference_within_v1` (`KDESC_A16_ATTN_FUSED`, ADR-0082 D1): the four
    /// shipped kernels composed — scores, the row softmax at `up_bits`, the probability narrowing
    /// to codes, the values.
    #[allow(clippy::too_many_arguments)]
    pub fn a16_attn_fused(
        &mut self,
        q: Ref,
        k: Ref,
        v: Ref,
        heads: u32,
        kv_heads: u32,
        d: u32,
        scores: &Narrowing,
        probs: &Narrowing,
        values: &Narrowing,
        up_bits: u32,
    ) -> Ref {
        let s = self.a16_attn_scores(q, k, heads, kv_heads, d, scores);
        let p = self.softmax_shifted(s, up_bits);
        let pc = self.narrow_codes(p, probs);
        self.a16_attn_values(pc, v, heads, kv_heads, d, values)
    }

    // ---- Qwen3.6's own ops (palw_qwen36_ops) --------------------------------------------------

    /// `q36_matmul_grouped` (`wide = false`) / `q36_matmul_grouped_wide` (`wide = true`): each row's
    /// 32-weight groups are summed exactly, scaled by `2^e` (`e:i8[out, G]`, the kernel's domain
    /// `[0, 20]`), summed, and narrowed once. The groups are a batch dimension of ONE `MatMul`
    /// (`[out, G, 1, 32] × [G, 32, 1]`), a ragged last group a second one — corpus §11's "per-group
    /// weight scales".
    pub fn q36_matmul_grouped(&mut self, w: Ref, e: Ref, x: Ref, n: &Narrowing, wide: bool) -> Ref {
        let k = fixed_last(self, x);
        let Dim::Fixed(out) = self.shape(w)[0] else { panic!("static weight") };
        let gw = Q36_WEIGHT_GROUP;
        let (full, rem) = (k / gw, k % gw);
        let mut parts = Vec::new();
        if full > 0 {
            let (wf, xf) = if rem == 0 {
                (w, x)
            } else {
                let wf = self.slice(w, 1, 0, full * gw);
                let xf = self.slice(x, 0, 0, full * gw);
                (wf, xf)
            };
            let wf = self.reshape_fixed(wf, &[out, full, 1, gw]);
            let xf = self.reshape_fixed(xf, &[full, gw, 1]);
            let pf = self.matmul(wf, xf, DType::I64);
            parts.push(self.reshape_fixed(pf, &[out, full]));
        }
        if rem > 0 {
            let wt = self.slice(w, 1, full * gw, rem);
            let xt = self.slice(x, 0, full * gw, rem);
            let xt = self.reshape_fixed(xt, &[rem, 1]);
            parts.push(self.matmul(wt, xt, DType::I64));
        }
        let partials = if parts.len() == 1 { parts[0] } else { self.concat(&parts, 1) };
        let e = self.clamp(e, 0, Q36_MAX_GROUP_EXP as i64, DType::I8);
        let scale = self.pow2_128_of(e, Q36_MAX_GROUP_EXP);
        let scaled = self.mul(partials, scale, DType::I64);
        let acc = self.reduce_sum(scaled, 1, DType::I64);
        let acc = self.reshape_fixed(acc, &[out]);
        if wide { self.narrow_wide(acc, n) } else { self.narrow_codes(acc, n) }
    }

    /// `q36_rope_partial`: per head of `head_dim` lanes, the first `rotary` rotate in adjacent
    /// pairs by the pinned Q24 table (`cos`, `sin`: `rotary/2` entries) and are narrowed by the
    /// registered clamp triple to codes; the rest pass through unchanged. The output has `x`'s
    /// dtype. For a code-width `x` the sums are `i64`; for a wider one, `i128` (the live kernel's
    /// one overflow corner, `a = b = c = s = i32::MIN`, is past `i64` — corpus §10.3).
    pub fn q36_rope_partial(&mut self, x: Ref, head_dim: u32, rotary: u32, cos: Ref, sin: Ref, clamp: &Narrowing) -> Ref {
        let t = self.ty(x);
        let total = fixed_last(self, x);
        let heads = total / head_dim;
        let x2 = self.reshape_fixed(x, &[heads, head_dim]);
        let rot = if rotary == head_dim { x2 } else { self.slice(x2, 1, 0, rotary) };
        let p = rotary / 2;
        let pairs = self.reshape_fixed(rot, &[heads, p, 2]);
        let a = self.slice(pairs, 2, 0, 1);
        let b = self.slice(pairs, 2, 1, 1);
        let a = self.reshape_fixed(a, &[heads, p]);
        let b = self.reshape_fixed(b, &[heads, p]);
        let sum = if t.dtype.width() <= 2 { DType::I64 } else { DType::I128 };
        let ac = self.mul(a, cos, DType::I64);
        let bs = self.mul(b, sin, DType::I64);
        let as_ = self.mul(a, sin, DType::I64);
        let bc = self.mul(b, cos, DType::I64);
        let re = self.sub(ac, bs, sum);
        let im = self.add(as_, bc, sum);
        let re = self.narrow(re, clamp, -32767, 32767, t.dtype);
        let im = self.narrow(im, clamp, -32767, 32767, t.dtype);
        let re = self.reshape_fixed(re, &[heads, p, 1]);
        let im = self.reshape_fixed(im, &[heads, p, 1]);
        let both = self.concat(&[re, im], 2);
        let rotated = self.reshape_fixed(both, &[heads, rotary]);
        let whole = if rotary == head_dim {
            rotated
        } else {
            let pass = self.slice(x2, 1, rotary, head_dim - rotary);
            self.concat(&[rotated, pass], 1)
        };
        self.reshape_fixed(whole, &[total])
    }

    /// `q36_ssm_conv`: the four-tap depthwise causal convolution. `window:code[4, C]` is
    /// position-major, oldest first (zero rows before the sequence start); `taps:[C, 4]` is
    /// channel-major, as the checkpoint stores them; one exact sum per channel, narrowed per
    /// channel to the `i32` rail (Q24 for the SiLU that follows).
    pub fn q36_ssm_conv(&mut self, window: Ref, taps: Ref, n: &Narrowing) -> Ref {
        let c = fixed_last(self, window);
        let tt = self.transpose(taps, &[1, 0]);
        let prod = self.mul(window, tt, DType::I64);
        let acc = self.reduce_sum(prod, 0, DType::I64);
        let acc = self.reshape_fixed(acc, &[c]);
        self.narrow_wide(acc, n)
    }

    /// `q36_gate_apply`: `N(y · g)` to codes for code `y` and a Q24 gate `g` (ONE triple).
    pub fn q36_gate_apply(&mut self, y: Ref, g: Ref, n: &Narrowing) -> Ref {
        let acc = self.mul(y, g, DType::I64);
        self.narrow_codes(acc, n)
    }

    /// `q36_mul_wide`: `N(a · b)` to codes, per lane, for two `i32` rows.
    pub fn q36_mul_wide(&mut self, a: Ref, b: Ref, n: &Narrowing) -> Ref {
        let acc = self.mul(a, b, DType::I64);
        self.narrow_codes(acc, n)
    }

    /// `q36_rescale_row`: `N(x)` to the `i32` rail, per lane.
    pub fn q36_rescale_row(&mut self, x: Ref, n: &Narrowing) -> Ref {
        self.narrow_wide(x, n)
    }

    /// The fenced `RequantizeByToken` arm (ADR-0102, the embedding lift): the row narrowed to codes
    /// by the TOKEN's triple, gathered from the per-token stores `m`, `s`, `z` (`[vocab]`).
    pub fn requantize_by_token(&mut self, x: Ref, m: Ref, s: Ref, z: Ref, token: Ref) -> Ref {
        let mt = self.gather(m, token, 0, 0);
        let st = self.gather(s, token, 0, 0);
        let zt = self.gather(z, token, 0, 0);
        self.narrow_codes(x, &Narrowing { m: mt, s: st, z: Some(zt) })
    }

    /// `palw_qwen36_ops::q36_l2_norm` along the last axis: A16 codes in, Q15 codes out. The sum's
    /// exponent is taken out BEFORE `IntRsqrt` (`Log2Floor`) and applied to the product; a zero row
    /// stays zero.
    pub fn l2_norm_q15(&mut self, x: Ref) -> Ref {
        let s = self.shape(x);
        let axis = s.len() - 1;
        let sq = self.mul(x, x, DType::I64);
        let sum = self.reduce_sum(sq, axis, DType::I64);
        let bit = self.log2_floor(sum, DType::I32);
        let two = self.c(DType::I32, 2);
        let e = self.div(bit, two, Rounding::Floor, DType::I32);
        let two_e = self.mul(e, two, DType::I32);
        let k = self.c(DType::I32, K as i128);
        let sh = self.sub(two_e, k, DType::I32);
        let zero = self.c(DType::I32, 0);
        let sh_nonneg = self.compare(sh, zero, Cmp::Ge);
        let right_amount = self.pow2_of(sh);
        let right = self.div(sum, right_amount, Rounding::Floor, DType::I128);
        let neg_sh = self.sub(zero, sh, DType::I32);
        let left_amount = self.pow2_of(neg_sh);
        let left = self.mul(sum, left_amount, DType::I128);
        let m = self.select(sh_nonneg, right, left, DType::I128);
        let m = self.clamp(m, 0, i64::MAX, DType::I64);
        let rs = self.int_rsqrt(m);
        let off = self.c(DType::I32, (K - 15) as i128);
        let shift = self.add(e, off, DType::I32);
        let prod = self.mul(x, rs, DType::I128);
        let den = self.pow2_of(shift);
        let q = self.div(prod, den, Rounding::Floor, DType::I128);
        let y = self.clamp(q, -32767, 32767, DType::I16);
        let empty = self.compare(sum, zero, Cmp::Le);
        let z16 = self.c(DType::I16, 0);
        self.select(empty, z16, y, DType::I16)
    }

    /// `palw_qwen36_ops::q36_softplus`: `ln(1 + e^x)` split at the sign so `IntExp` only sees
    /// `−|x|`: `x ≤ 0 → IntLn(ONE + e)`, `x > 0 → x + IntLn(ONE + e)`, `e = IntExp(−|x|)`. Q24.
    pub fn softplus_q36(&mut self, x: Ref) -> Ref {
        let zero = self.c(DType::I32, 0);
        let pos = self.compare(x, zero, Cmp::Gt);
        let neg = self.sub(zero, x, DType::I64);
        let neg_abs = self.select(pos, neg, x, DType::I64);
        let e = self.int_exp(neg_abs);
        let one = self.c(DType::I32, ONE);
        let arg = self.add(e, one, DType::I64);
        let tail = self.int_ln(arg);
        let sum = self.add(x, tail, DType::I64);
        let le = self.compare(x, zero, Cmp::Le);
        self.select(le, tail, sum, DType::I64)
    }

    /// `palw_qwen36_ops::q36_exp_refined` for a Q24 `x ≤ 0` (`i32`): one Newton step of the
    /// frozen `IntExp` against `IntLn`, `y ← y + (y·clamp(x − ln y, ±ONE/4)) >> 24`, clamped to
    /// `[0, ONE]`; 0 where `IntExp` is 0. The exponential that is precise near 1 — a decay of
    /// `1 − 10^−4` keeps its fourth digit.
    pub fn exp_refined_q36(&mut self, x: Ref) -> Ref {
        let y0 = self.int_exp(x);
        let ln_y = self.int_ln(y0);
        let d = self.sub(x, ln_y, DType::I64);
        let corr = self.clamp(d, -(ONE as i64 / 4), ONE as i64 / 4, DType::I32);
        let prod = self.mul(y0, corr, DType::I64);
        let step = self.shr(prod, K, Rounding::Floor, DType::I64);
        let adj = self.add(y0, step, DType::I64);
        let adj = self.clamp(adj, 0, ONE as i64, DType::I32);
        let zero = self.c(DType::I32, 0);
        let dead = self.compare(y0, zero, Cmp::Le);
        self.select(dead, zero, adj, DType::I32)
    }

    /// `palw_qwen36_ops::q36_decay`: `exp(−c · softplus(dt))` for a registered `c ≥ 0` (Q24,
    /// `c ≤ 0` gives `ONE`): `arg = clamp((c·softplus(dt)) >> 24, 0, 2^31)`, then the refined
    /// exponential of `−arg`, clamped to `[0, ONE]`. (The court's `Decay` arm adds the registered
    /// `dt_bias` to the row first, saturating in `i32`.)
    pub fn decay_q36(&mut self, dt: Ref, c: Ref) -> Ref {
        let sp = self.softplus_q36(dt);
        let p = self.mul(c, sp, DType::I128);
        let a = self.shr(p, K, Rounding::Floor, DType::I128);
        let a = self.clamp(a, 0, 1i64 << 31, DType::I64);
        let zero = self.c(DType::I32, 0);
        let neg = self.sub(zero, a, DType::I64);
        let neg = self.clamp(neg, i32::MIN as i64, 0, DType::I32);
        let y = self.exp_refined_q36(neg);
        let one = self.c(DType::I32, ONE);
        let off = self.compare(c, zero, Cmp::Le);
        self.select(off, one, y, DType::I32)
    }

    /// `palw_qwen36_ops::q36_gdn_step`, vectorised over heads: one position of the gated delta
    /// rule for `Fixed` state `state` (`[heads, d_v, d_k]`, range `±(2^31 − 1)`).
    ///
    /// `k`, `q`: `[heads, d_k]` unit codes (already mapped to the value heads — the head mapping is
    /// an explicit Reshape/Broadcast, [`Self::map_heads_group`] / [`Self::map_heads_tile`], not
    /// part of the rule); `v`: `[heads, d_v]` codes; `decay`: `[heads]`, or `[heads, d_k]` for a per-key-channel gate, and `beta`: `[heads]`, Q24 in `[0, ONE]`.
    /// The narrowings are per-head params: `(m, pow2_s, z)` for the read, the delta and the output,
    /// and `write_shift` (`i32`, left if ≥ 0). Returns the output `[heads, d_v]` as `i32`; the
    /// state write is part of the expansion.
    #[allow(clippy::too_many_arguments)]
    pub fn gdn_step_q36(
        &mut self,
        state: u16,
        k: Ref,
        v: Ref,
        q: Ref,
        decay: Ref,
        beta: Ref,
        read: (Ref, Ref, Ref),
        delta: (Ref, Ref, Ref),
        write_shift: Ref,
        out: (Ref, Ref, Ref),
    ) -> Ref {
        let smax = i32::MAX as i64;
        let s_shape = self.ty(Ref::State(state)).shape;
        let (Dim::Fixed(h), Dim::Fixed(dv), Dim::Fixed(dk)) = (s_shape[0], s_shape[1], s_shape[2]) else { panic!("static state") };
        let col = |b: &mut Self, x: Ref, n: u32| b.reshape_fixed(x, &[h, n, 1]);
        // 1. The gate: S1 = clamp(RSR(S · decay, 24), ±(2^31 − 1)).
        // `decay` is `[heads]` (the library's gate: one per head) or `[heads, d_k]` (a forget gate per key channel, Kimi delta attention:
        // the state's last axis is the key channel, so the decay broadcasts as `[heads, 1, d_k]` and nothing else of the step changes).
        let numel: u64 = self.ty(decay).shape.iter().map(|d| if let Dim::Fixed(n) = d { *n as u64 } else { 0 }).product();
        let dec = if dk > 1 && numel == h as u64 * dk as u64 { self.reshape_fixed(decay, &[h, 1, dk]) } else { b_reshape3(self, decay, h) };
        let sd = self.mul(Ref::State(state), dec, DType::I64);
        let sd = self.shr(sd, K, Rounding::HalfAwayFromZero, DType::I64);
        let s1 = self.clamp(sd, -smax, smax, DType::I32);
        // 2. w = narrow_read(S1 k) — wide, the i64 rail.
        let kc = col(self, k, dk);
        let acc = self.matmul(s1, kc, DType::I64);
        let rm = b_reshape3(self, read.0, h);
        let rs = b_reshape3(self, read.1, h);
        let rz = b_reshape3(self, read.2, h);
        let w = self.narrow_a16(acc, rm, rs, rz, i64::MIN, i64::MAX, DType::I64);
        // 3. u = clamp(narrow_delta(RSR(sat64(sat64(v − w) · beta), 24)), ±(2^24 − 1)).
        let vc = col(self, v, dv);
        let diff = self.sub(vc, w, DType::I128);
        let diff = self.clamp(diff, i64::MIN, i64::MAX, DType::I64);
        let bt = b_reshape3(self, beta, h);
        let db = self.mul(diff, bt, DType::I128);
        let db = self.clamp(db, i64::MIN, i64::MAX, DType::I64);
        let scaled = self.shr(db, K, Rounding::HalfAwayFromZero, DType::I64);
        let dm = b_reshape3(self, delta.0, h);
        let ds = b_reshape3(self, delta.1, h);
        let dz = b_reshape3(self, delta.2, h);
        let u = self.narrow_a16(scaled, dm, ds, dz, -((1 << 24) - 1), (1 << 24) - 1, DType::I32);
        // 4. The rank-one write: S2 = clamp(S1 + write(u ⊗ k), ±(2^31 − 1)).
        let kr = self.reshape_fixed(k, &[h, 1, dk]);
        let prod = self.mul(u, kr, DType::I64);
        let ws = b_reshape3(self, write_shift, h);
        let zero = self.c(DType::I32, 0);
        let left_on = self.compare(ws, zero, Cmp::Ge);
        let lp = self.clamp(ws, 0, 20, DType::I32);
        let lp = self.pow2_of(lp);
        let left = self.mul(prod, lp, DType::I128);
        let left = self.clamp(left, i64::MIN, i64::MAX, DType::I64);
        let nws = self.sub(zero, ws, DType::I64);
        let rp = self.clamp(nws, 0, 62, DType::I32);
        let rp = self.pow2_of(rp);
        let right = self.div(prod, rp, Rounding::HalfAwayFromZero, DType::I64);
        let write = self.select(left_on, left, right, DType::I64);
        let s2 = self.add(s1, write, DType::I128);
        let s2 = self.state_write(state, s2);
        // 5. o = narrow_out(S2 q), wide.
        let qc = col(self, q, dk);
        let acc = self.matmul(s2, qc, DType::I64);
        let om = b_reshape3(self, out.0, h);
        let os = b_reshape3(self, out.1, h);
        let oz = b_reshape3(self, out.2, h);
        let o = self.narrow_a16(acc, om, os, oz, i32::MIN as i64, i32::MAX as i64, DType::I32);
        self.reshape_fixed(o, &[h, dv])
    }

    /// **The unit row `x / √(mean(x²) + eps)` in Q24 along the last axis, in 21 nodes** —
    /// [`Self::rms_norm_wide_q36`]'s value (and so `q36_rms_norm_wide`'s) for any `eps` that fits one
    /// `i64` param (the template's `eps_zero · 2^eps_shift`), for rows of `i32` values.
    ///
    /// The mean's exponent is taken out only when it is positive, `h = max(0, ⌊(log2 mean − 24)/2⌋)`:
    /// for a smaller mean `IntRsqrt` normalises internally — its mantissa is the template's
    /// left-shifted one — and returns `y · 2^(−e)` as an exact left shift, which is what the
    /// template's left-shift branch reassembles, so the two give the same integers; a zero mean
    /// gives `IntRsqrt(0) = 0` and a zero row, as the template's select does. (tir/lower's request:
    /// the gated-delta + MoE layers of Qwen3.5-MoE and Qwen3-Next and DeepSeek-V3's MLA + MoE layers
    /// do not fit NF-12's 512 nodes with the 39-node form.)
    pub fn rms_unit_q24(&mut self, x: Ref, eps: Ref) -> Ref {
        let shape = self.shape(x);
        let axis = shape.len() - 1;
        let n = fixed_last(self, x);
        let sq = self.mul(x, x, DType::I64);
        let sum = self.reduce_sum(sq, axis, DType::I128);
        let one = self.c(DType::I64, ONE);
        let scaled = self.mul(sum, one, DType::I128);
        let nn = self.c(DType::I64, n as i128);
        let mean0 = self.div(scaled, nn, Rounding::Floor, DType::I128);
        let e = self.clamp(eps, 0, i64::MAX, DType::I64);
        let mean = self.add(mean0, e, DType::I128);
        let bit = self.log2_floor(mean, DType::I32);
        let k = self.c(DType::I32, K as i128);
        let t = self.sub(bit, k, DType::I32);
        let two = self.c(DType::I32, 2);
        let h = self.div(t, two, Rounding::Floor, DType::I32);
        let h = self.clamp(h, 0, 51, DType::I32);
        let h2 = self.mul(h, two, DType::I32);
        let p2 = self.pow2_128_of(h2, 102);
        let m = self.div(mean, p2, Rounding::Floor, DType::I128);
        let m = self.clamp(m, 0, i64::MAX, DType::I64);
        let r = self.int_rsqrt(m);
        let prod = self.mul(x, r, DType::I128);
        let p1 = self.pow2_128_of(h, 51);
        let y = self.div(prod, p1, Rounding::Floor, DType::I128);
        self.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// **`x / ‖x‖` in Q15 codes along the last axis, in 17 nodes** — [`Self::l2_norm_q15`]'s value
    /// (and `q36_l2_norm`'s) for rows of A16 codes: `IntRsqrt(Σx² / 2^2h) · x / 2^(h + 21)` with
    /// `h = max(0, ⌊(log2 Σx² − 24)/2⌋)`. Below `h = 0` the template shifts the sum left into
    /// `[2^24, 2^26)` and divides by `2^(21 − k)`; `IntRsqrt` normalises the unshifted sum to the same
    /// mantissa and returns the result `k` bits up exactly, so the floors agree; a zero row stays
    /// zero (`IntRsqrt(0) = 0`).
    pub fn l2_unit_q15(&mut self, x: Ref) -> Ref {
        let axis = self.shape(x).len() - 1;
        let sq = self.mul(x, x, DType::I64);
        let sum = self.reduce_sum(sq, axis, DType::I64);
        let bit = self.log2_floor(sum, DType::I32);
        let k = self.c(DType::I32, K as i128);
        let t = self.sub(bit, k, DType::I32);
        let two = self.c(DType::I32, 2);
        let h = self.div(t, two, Rounding::Floor, DType::I32);
        let h = self.clamp(h, 0, 20, DType::I32);
        let h2 = self.mul(h, two, DType::I32);
        let p2 = self.pow2_128_of(h2, 40);
        let m = self.div(sum, p2, Rounding::Floor, DType::I64);
        let r = self.int_rsqrt(m);
        let prod = self.mul(x, r, DType::I64);
        let off = self.c(DType::I32, 21);
        let sh = self.add(h, off, DType::I32);
        let p1 = self.pow2_128_of(sh, 41);
        let y = self.div(prod, p1, Rounding::Floor, DType::I64);
        self.clamp(y, -32767, 32767, DType::I16)
    }

    /// `palw_qwen36_ops::q36_rms_norm_wide` along the last axis for a registered `eps` whose
    /// mantissa is at most `2^30` and whose shift is at most 96 — the form lowerers store: the RMS
    /// norm of a WIDE `i32` row, `eps = eps_zero · 2^eps_shift` at the caller's scale, the mean's
    /// exponent taken out before `IntRsqrt` (`Log2Floor` finds the even shift that lands the mean in
    /// `[2^24, 2^26)`) and the product shifted back. A zero mean gives a zero row.
    pub fn rms_norm_wide_q36(&mut self, x: Ref, eps_zero: Ref, eps_shift: Ref) -> Ref {
        self.rms_norm_wide_eps(x, eps_zero, eps_shift, 1 << 30, 96)
    }

    /// [`Self::rms_norm_wide_q36`] on the live kernel's OWN domain for `eps` — any non-negative
    /// `i64` mantissa and a shift of at most 62, which is what `A16QuantParams::from_wire` admits.
    /// The two agree wherever both apply (mantissa `≤ 2^30`, shift `≤ 62`).
    pub fn rms_norm_wide_q36_exact(&mut self, x: Ref, eps_zero: Ref, eps_shift: Ref) -> Ref {
        self.rms_norm_wide_eps(x, eps_zero, eps_shift, i64::MAX, 62)
    }

    fn rms_norm_wide_eps(&mut self, x: Ref, eps_zero: Ref, eps_shift: Ref, zero_max: i64, shift_max: u32) -> Ref {
        let sh = self.shape(x);
        let axis = sh.len() - 1;
        let n = fixed_last(self, x);
        let sq = self.mul(x, x, DType::I64);
        let sum = self.reduce_sum(sq, axis, DType::I128);
        let one = self.c(DType::I64, ONE);
        let scaled = self.mul(sum, one, DType::I128);
        let nn = self.c(DType::I64, n as i128);
        let mean0 = self.div(scaled, nn, Rounding::Floor, DType::I128);
        // `eps_zero · 2^shift` stays an i128 by bounding the mantissa and the shift together.
        let ez = self.clamp(eps_zero, 0, zero_max, DType::I64);
        let es = self.pow2_128_of(eps_shift, shift_max);
        let eps = self.mul(ez, es, DType::I128);
        let mean = self.add(mean0, eps, DType::I128);
        let bit = self.log2_floor(mean, DType::I32);
        let k = self.c(DType::I32, K as i128);
        let t = self.sub(bit, k, DType::I32);
        let two = self.c(DType::I32, 2);
        let h = self.div(t, two, Rounding::Floor, DType::I32);
        let two_h = self.mul(h, two, DType::I32);
        let zero = self.c(DType::I32, 0);
        let rp = self.pow2_128_of(two_h, 126);
        let right = self.div(mean, rp, Rounding::Floor, DType::I128);
        let neg2h = self.sub(zero, two_h, DType::I32);
        let lp = self.pow2_128_of(neg2h, 24);
        let small = self.clamp(mean, 0, ONE as i64, DType::I64);
        let left = self.mul(small, lp, DType::I128);
        let ge = self.compare(two_h, zero, Cmp::Ge);
        let m = self.select(ge, right, left, DType::I128);
        let m = self.clamp(m, 0, i64::MAX, DType::I64);
        let r = self.int_rsqrt(m);
        let prod = self.mul(x, r, DType::I128);
        let dp = self.pow2_128_of(h, 126);
        let rshift = self.div(prod, dp, Rounding::Floor, DType::I128);
        let negh = self.sub(zero, h, DType::I32);
        let up = self.pow2_128_of(negh, 12);
        let lshift = self.mul(prod, up, DType::I128);
        let hge = self.compare(h, zero, Cmp::Ge);
        let y = self.select(hge, rshift, lshift, DType::I128);
        let y = self.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let empty = self.compare(mean, zero, Cmp::Le);
        self.select(empty, zero, y, DType::I32)
    }

    /// `palw_qwen36_ops::q36_router_topk` over the last axis of a logit row: `softmax_shifted`,
    /// `TopK` (committed; lowest index on ties, index order), the kept probabilities renormalised
    /// through `IntRecip`. Returns `(indices [k], weights [k] Q24)`. Legacy's uniform fallback for
    /// a zero kept sum is dead — the row maximum is always kept and its probability is positive —
    /// and has no node.
    pub fn router_topk_q36(&mut self, logits: Ref, k: u32, up_bits: u32) -> (Ref, Ref) {
        let axis = self.shape(logits).len() - 1;
        let probs = self.softmax_shifted(logits, up_bits);
        let idx = self.topk(probs, axis, k);
        let kept = self.gather(probs, idx, axis, axis);
        let sum = self.reduce_sum(kept, axis, DType::I64);
        let recip = self.int_recip(sum);
        let p = self.mul(kept, recip, DType::I128);
        let w = self.shr(p, K, Rounding::Floor, DType::I64);
        let w = self.clamp(w, 0, 1 << 25, DType::I32);
        (idx, w)
    }

    /// `palw_qwen36_ops::q36_moe_combine`: `Σ_e w_e · y_e` in ONE exact accumulator (a `MatMul`
    /// of the weights `[k]` against the expert rows `[k, width]`), narrowed once.
    #[allow(clippy::too_many_arguments)]
    pub fn moe_combine_q36(&mut self, y: Ref, w: Ref, m: Ref, pow2_s: Ref, z: Ref, lo: i64, hi: i64, dtype: DType) -> Ref {
        let s = self.shape(y);
        let (Dim::Fixed(k), Dim::Fixed(width)) = (s[0], s[1]) else { panic!("static") };
        let wr = self.reshape_fixed(w, &[1, k]);
        let acc = self.matmul(wr, y, DType::I64);
        let acc = self.reshape_fixed(acc, &[width]);
        self.narrow_a16(acc, m, pow2_s, z, lo, hi, dtype)
    }
}

/// A per-head vector `[h]` as `[h, 1, 1]`, to broadcast against `[h, rows, cols]`.
pub(crate) fn b_reshape3(b: &mut BlockBuilder<'_>, x: Ref, h: u32) -> Ref {
    b.reshape_fixed(x, &[h, 1, 1])
}
