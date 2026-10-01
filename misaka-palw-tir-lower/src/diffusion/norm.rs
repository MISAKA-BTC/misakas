//! **`NORM_GROUP_SPATIAL_V1`** — GroupNorm over `[C, H, W]` (the VAE's norm), with the statistics taken from
//! committed ROW PARTIALS so that no cone reads a whole tensor (RFC-0003 §5.5).
//!
//! The input `x:[C, H, W]` `i16` codes (scale `sx`) is cut into `G` groups of `Cg = C/G` channels and, per
//! spatial row `h` and group `g`, the exact partials `Σx` and `Σx²` over the row's `Cg · W` elements are formed
//! and COMMITTED as `[H, G, 3]` `i32` lanes: `Σx`, and `Σx²` split at bit 31 into a signed high part and a
//! non-negative low part (a 64-bit sum is not a committable lane; the split is exact and a cone recomputing a
//! partial reads one row). The totals are the exact sums of the partials over `H`; with `n = Cg · H · W`,
//!
//! ```text
//!   c_i = n·x_i − T1                  (= n·(x_i − μ), exact)
//!   V   = n·T2 − T1²                  (= n²·Var, exact, i128)
//!   x̂_i = c_i / √V                    (the unit row; Σc² = n·V)
//! ```
//!
//! The mean's exponent is taken out by `Log2Floor` before `IntRsqrt`, exactly as the library's wide RMSNorm does
//! (`rms_norm_wide_q36`; the tail below is its own, over a mean supplied rather than summed). `ε` is data: it
//! enters `V` as `n²·ε/sx²`. The channel's gain and shift are the narrowing: `m_c/2^s_c = γ_c / (2^24 · sy)`,
//! `z_c = round(β_c / sy)`, out in `i16` codes at `sy` — one lossy site, beside the unit row's own (`IntRsqrt`).

use misaka_palw_tir::arith::{K, ONE};
use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::{Cmp, DType, Ref, Rounding};

use super::sink::ParamSink;
use crate::quant::mul_shift;

/// A GroupNorm's geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupNormSpec {
    pub c: usize,
    pub groups: usize,
    pub h: usize,
    pub w: usize,
}

impl GroupNormSpec {
    pub fn cg(&self) -> usize {
        self.c / self.groups
    }
    /// The elements a group normalises over.
    pub fn n(&self) -> u64 {
        (self.cg() * self.h * self.w) as u64
    }
    /// The elements of one partial's row, `Cg · W`.
    pub fn row(&self) -> usize {
        self.cg() * self.w
    }
}

/// A quantised GroupNorm: `ε` and the per-channel gain and shift as integers.
#[derive(Clone, Debug)]
pub struct QGroupNorm {
    pub spec: GroupNormSpec,
    /// `n²·ε / sx²`, the epsilon in `V`'s units.
    pub eps_v: i128,
    pub m: Vec<i64>,
    pub s: Vec<i8>,
    pub z: Vec<i64>,
}

impl QGroupNorm {
    /// For inputs at `sx`, an output at `sy`, gain `gamma:[C]`, shift `beta:[C]` and `eps` (diffusers' `1e-6`).
    pub fn new(spec: GroupNormSpec, gamma: &[f32], beta: &[f32], eps: f64, sx: f64, sy: f64) -> Self {
        assert!(spec.c % spec.groups == 0 && gamma.len() == spec.c && beta.len() == spec.c, "GroupNorm: groups divide the channels");
        assert!(spec.row() <= 65_535, "a partial row of {} elements overflows the i32 lane of Σx", spec.row());
        let n = spec.n() as f64;
        let eps_v = (n * n * eps / (sx * sx)).round() as i128;
        let (mut m, mut s, mut z) = (Vec::new(), Vec::new(), Vec::new());
        for c in 0..spec.c {
            let (mc, sc) = mul_shift(gamma[c] as f64 / (ONE as f64 * sy));
            m.push(mc);
            s.push(sc);
            z.push((beta[c] as f64 / sy).round() as i64);
        }
        Self { spec, eps_v, m, s, z }
    }

    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink, name: &str) -> QGroupNormRefs {
        let c = self.spec.c as u32;
        let m = sink.put(pb, &format!("{name}.m"), DType::I64, &[c, 1, 1], self.m.iter().map(|v| *v as i128).collect());
        let s = sink.put(pb, &format!("{name}.s"), DType::I8, &[c, 1, 1], self.s.iter().map(|v| *v as i128).collect());
        let z = sink.put(pb, &format!("{name}.z"), DType::I64, &[c, 1, 1], self.z.iter().map(|v| *v as i128).collect());
        QGroupNormRefs { m, s, z }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct QGroupNormRefs {
    pub m: Ref,
    pub s: Ref,
    pub z: Ref,
}

/// **`x / √mean` in Q24** for a wide `x` (`i64`, any shape) and a Q24-scaled `mean:i128` that broadcasts against
/// it, `mean = 0` giving a zero row — the tail of the library's wide RMSNorm, over a mean supplied.
pub fn unit_over_mean(b: &mut BlockBuilder<'_>, x: Ref, mean: Ref) -> Ref {
    let bit = b.log2_floor(mean, DType::I32);
    let k = b.c(DType::I32, K as i128);
    let t = b.sub(bit, k, DType::I32);
    let two = b.c(DType::I32, 2);
    let h = b.div(t, two, Rounding::Floor, DType::I32);
    let two_h = b.mul(h, two, DType::I32);
    let zero = b.c(DType::I32, 0);
    let rp = b.pow2_128_of(two_h, 126);
    let right = b.div(mean, rp, Rounding::Floor, DType::I128);
    let neg2h = b.sub(zero, two_h, DType::I32);
    let lp = b.pow2_128_of(neg2h, 24);
    let small = b.clamp(mean, 0, ONE as i64, DType::I64);
    let left = b.mul(small, lp, DType::I128);
    let ge = b.compare(two_h, zero, Cmp::Ge);
    let m = b.select(ge, right, left, DType::I128);
    let m = b.clamp(m, 0, i64::MAX, DType::I64);
    let r = b.int_rsqrt(m);
    let prod = b.mul(x, r, DType::I128);
    let dp = b.pow2_128_of(h, 126);
    let rshift = b.div(prod, dp, Rounding::Floor, DType::I128);
    let negh = b.sub(zero, h, DType::I32);
    let up = b.pow2_128_of(negh, 12);
    let lshift = b.mul(prod, up, DType::I128);
    let hge = b.compare(h, zero, Cmp::Ge);
    let y = b.select(hge, rshift, lshift, DType::I128);
    let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let empty = b.compare(mean, zero, Cmp::Le);
    b.select(empty, zero, y, DType::I32)
}

/// **The committed partials** of `x:[C, H, W]` `i16`: `[H, G, 3]` `i32` — `Σx`, and `Σx²` as a signed high part
/// and a non-negative low part at bit 31. The returned node is committed.
pub fn group_partials(b: &mut BlockBuilder<'_>, x: Ref, spec: &GroupNormSpec) -> Ref {
    let (g, cg, h, w) = (spec.groups as u32, spec.cg() as u32, spec.h as u32, spec.w as u32);
    let xg = b.reshape_fixed(x, &[g, cg, h, w]);
    let xt = b.transpose(xg, &[2, 0, 1, 3]); // [H, G, Cg, W]
    let rows = b.reshape_fixed(xt, &[h, g, cg * w]);
    let s1 = b.reduce_sum(rows, 2, DType::I64); // [H, G, 1]
    let sq = b.mul(rows, rows, DType::I32);
    let s2 = b.reduce_sum(sq, 2, DType::I64);
    let base = b.c(DType::I64, 1 << 31);
    let hi = b.div(s2, base, Rounding::Floor, DType::I64);
    let back = b.mul(hi, base, DType::I64);
    let lo = b.sub(s2, back, DType::I64);
    // `lo = s2 mod 2^31` — a fact the interval analysis cannot see through the subtraction.
    let lo = b.clamp(lo, 0, i32::MAX as i64, DType::I64);
    let s1 = b.cast(s1, DType::I32);
    let hi = b.cast(hi, DType::I32);
    let lo = b.cast(lo, DType::I32);
    let part = b.concat(&[s1, hi, lo], 2);
    b.commit(part)
}

/// **GroupNorm**: `x:[C, H, W]` `i16` codes at `sx` to `[C, H, W]` `i16` codes at `sy`.
pub fn lower_group_norm(b: &mut BlockBuilder<'_>, x: Ref, q: &QGroupNorm, r: &QGroupNormRefs) -> Ref {
    let spec = q.spec;
    let (g, cg, h, w) = (spec.groups as u32, spec.cg() as u32, spec.h as u32, spec.w as u32);
    let part = group_partials(b, x, &spec);
    // The totals: the exact sums of the partials over H.
    let p64 = b.cast(part, DType::I64);
    let tot = b.reduce_sum(p64, 0, DType::I64); // [1, G, 3]
    // A slice carries its tensor's one interval, the union of three lanes of very different ranges; each total is
    // clamped to what its lane can hold (`i16` inputs: `|x| ≤ 32767`, a row of `Cg·W` elements, `H` rows) — a clamp
    // that never fires and makes the range analysis total, the library's convention.
    let (rows, row) = (spec.h as i64, spec.row() as i64);
    let t1 = b.slice(tot, 2, 0, 1);
    let t1 = b.clamp(t1, -rows * row * 32_767, rows * row * 32_767, DType::I64);
    let thi = b.slice(tot, 2, 1, 1);
    let thi = b.clamp(thi, 0, rows * (row / 2 + 1), DType::I64);
    let tlo = b.slice(tot, 2, 2, 1);
    let tlo = b.clamp(tlo, 0, rows * i32::MAX as i64, DType::I64);
    let base = b.c(DType::I64, 1 << 31);
    let t2h = b.mul(thi, base, DType::I64);
    let t2 = b.add(t2h, tlo, DType::I64); // [1, G, 1]
    let n = b.c(DType::I128, spec.n() as i128);
    let nt2 = b.mul(t2, n, DType::I128);
    let t1sq = b.mul(t1, t1, DType::I128);
    let v = b.sub(nt2, t1sq, DType::I128); // n²·Var
    let eps = b.c(DType::I128, q.eps_v);
    let v = b.add(v, eps, DType::I128);
    let one = b.c(DType::I64, ONE);
    let mean = b.mul(v, one, DType::I128);
    let mean = b.reshape_fixed(mean, &[g, 1, 1, 1]);
    // c = n·x − T1, per element of its group.
    let xg = b.reshape_fixed(x, &[g, cg, h, w]);
    let nx = b.mul(xg, n, DType::I128);
    let t1b = b.reshape_fixed(t1, &[g, 1, 1, 1]);
    let c = b.sub(nx, t1b, DType::I128);
    let c = b.clamp(c, i64::MIN, i64::MAX, DType::I64);
    let unit = unit_over_mean(b, c, mean); // [G, Cg, H, W] Q24
    let unit = b.reshape_fixed(unit, &[spec.c as u32, h, w]);
    b.narrow(unit, &Narrowing::new(r.m, r.s, Some(r.z)), -32_767, 32_767, DType::I16)
}

#[cfg(test)]
mod tests {
    use super::super::testkit::{Lcg, run_one_block};
    use super::*;

    fn check(spec: GroupNormSpec, seed: u64) {
        let mut rng = Lcg(seed);
        let (sx, sy) = (1.0 / 2048.0, 1.0 / 4096.0);
        let gamma: Vec<f32> = (0..spec.c).map(|_| 0.5 + rng.unit() as f32 * 0.4).collect();
        let beta: Vec<f32> = (0..spec.c).map(|_| rng.unit() as f32 * 0.3).collect();
        let eps = 1e-6;
        let q = QGroupNorm::new(spec, &gamma, &beta, eps, sx, sy);
        let x: Vec<i128> = (0..spec.c * spec.h * spec.w).map(|_| rng.range(-12_000, 12_000)).collect();
        let x2 = x.clone();
        let y = run_one_block(
            |pb, sink| {
                let xs = sink.put(pb, "x", DType::I16, &[spec.c as u32, spec.h as u32, spec.w as u32], x2);
                (xs, q.declare(pb, sink, "gn"))
            },
            |b, (xs, r)| lower_group_norm(b, xs, &q, &r),
        );
        assert_eq!(y.shape, vec![spec.c, spec.h, spec.w]);
        let (cg, hw) = (spec.cg(), spec.h * spec.w);
        for g in 0..spec.groups {
            let idx = |c: usize, p: usize| (g * cg + c) * hw + p;
            let vals: Vec<f64> = (0..cg).flat_map(|c| (0..hw).map(move |p| (c, p))).map(|(c, p)| x[idx(c, p)] as f64 * sx).collect();
            let n = vals.len() as f64;
            let mean = vals.iter().sum::<f64>() / n;
            let var = vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
            for c in 0..cg {
                for p in 0..hw {
                    let ch = g * cg + c;
                    let xhat = (x[idx(c, p)] as f64 * sx - mean) / (var + eps).sqrt();
                    let want = gamma[ch] as f64 * xhat + beta[ch] as f64;
                    let got = y.data[idx(c, p)] as f64 * sy;
                    assert!((got - want).abs() <= 2.5 * sy + 1e-3 * want.abs(), "[{ch},{p}]: integer {got} vs float {want}");
                }
            }
        }
    }

    #[test]
    fn group_norm_is_the_float_one() {
        check(GroupNormSpec { c: 4, groups: 2, h: 3, w: 5 }, 1);
        check(GroupNormSpec { c: 8, groups: 4, h: 4, w: 4 }, 2);
        check(GroupNormSpec { c: 6, groups: 1, h: 2, w: 3 }, 3);
    }

    #[test]
    fn the_partials_are_committed_and_exact() {
        let spec = GroupNormSpec { c: 4, groups: 2, h: 2, w: 3 };
        let x: Vec<i128> = (0..24).map(|i| (i as i128 - 11) * 997).collect();
        let x2 = x.clone();
        let p = run_one_block(|pb, sink| sink.put(pb, "x", DType::I16, &[4, 2, 3], x2), |b, xs| group_partials(b, xs, &spec));
        assert_eq!(p.shape, vec![2, 2, 3]);
        for h in 0..2 {
            for g in 0..2 {
                let vals: Vec<i128> =
                    (0..2).flat_map(|c| (0..3).map(move |w| (c, w))).map(|(c, w)| x[((g * 2 + c) * 2 + h) * 3 + w]).collect();
                let s1: i128 = vals.iter().sum();
                let s2: i128 = vals.iter().map(|v| v * v).sum();
                let at = |k: usize| p.data[(h * 2 + g) * 3 + k];
                assert_eq!(at(0), s1);
                assert_eq!(at(1) * (1 << 31) + at(2), s2);
                assert!(at(2) >= 0);
            }
        }
    }
}
