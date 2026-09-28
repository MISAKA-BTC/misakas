//! **Norms** — the unit rows every family normalises into.
//!
//! The RMS machinery is [`BlockBuilder::rms_norm_wide_q36`] (legacy-exact, and the general RMSNorm
//! of any code or wide row into a Q24 unit row: the mean's exponent comes out by `Log2Floor` before
//! `IntRsqrt`, so a quiet row keeps its precision). Everything here is built on it:
//!
//! * LayerNorm without a lossy division — exact centring `c = n·x − Σx` (corpus §6.2.7);
//! * GroupNorm as LayerNorm per group (RWKV's per-head norm, Mamba-2's grouped norm);
//! * the L2 norm with an epsilon inside the square root (`x / sqrt(Σx² + eps)`, FLA's `l2norm`).
//!
//! Gains and biases are the caller's narrowing out of Q24 (`γ` as `m`, `β` as `z`); Gemma's `1 + w`
//! is data.

use crate::builder::BlockBuilder;
use crate::program::Ref;
use crate::types::{DType, Dim};

impl BlockBuilder<'_> {
    /// **LayerNorm without a lossy division** (corpus §6.2.7): `c = n·x − Σx` is exact — it is
    /// `n·(x − μ)` with no rounding — and `LayerNorm(x) = RMSNorm(c)` with `eps' = n²·eps`, because
    /// `Σc² = n³·Var(x)`. The only divisions left are the RMS mean's `÷ n` and `IntRsqrt`. `x` is a
    /// row of codes (`n·x` must fit `i32`); the wide RMSNorm takes the exponent out, so a quiet row
    /// keeps its precision. Returns the unit row in Q24 (`i32`); gain and bias are the caller's
    /// narrowing. Works for any `n`, a power of two or not (2880, 4544, 5120 …).
    pub fn layer_norm_exact(&mut self, x: Ref, eps_zero: Ref, eps_shift: Ref) -> Ref {
        let sh = self.shape(x);
        let axis = sh.len() - 1;
        let Dim::Fixed(n) = sh[axis] else { panic!("norm over H") };
        let nn = self.c(DType::I64, n as i128);
        let nx = self.mul(x, nn, DType::I64);
        let s = self.reduce_sum(x, axis, DType::I64);
        let c = self.sub(nx, s, DType::I64);
        let c = self.cast(c, DType::I32);
        self.rms_norm_wide_q36(c, eps_zero, eps_shift)
    }

    /// **GroupNorm** over the last axis split into `groups` equal groups: [`Self::layer_norm_exact`]
    /// per group (the group statistics never mix), back in the input's shape. `eps` is per group
    /// (`n_group² · eps` at the input scale², as for LayerNorm).
    pub fn group_norm_exact(&mut self, x: Ref, groups: u32, eps_zero: Ref, eps_shift: Ref) -> Ref {
        let sh = self.shape(x);
        let r = sh.len();
        let Dim::Fixed(n) = sh[r - 1] else { panic!("norm over H") };
        assert_eq!(n % groups, 0, "the groups divide the row");
        let mut g_shape = sh.clone();
        g_shape[r - 1] = Dim::Fixed(groups);
        g_shape.push(Dim::Fixed(n / groups));
        let g = self.reshape(x, &g_shape);
        let y = self.layer_norm_exact(g, eps_zero, eps_shift);
        self.reshape(y, &sh)
    }

    /// **The L2 norm with its epsilon inside the root**, `x / sqrt(Σx² + eps)` (FLA's `l2norm`,
    /// Qwen3-Next's q/k norm, RWKV-7's `k̂`), along the last axis, Q24 out. `eps` is at the input
    /// scale² as `eps_zero · 2^eps_shift` (mantissa `≤ 2^30`, shift `≤ 96`). The same exponent
    /// extraction as the wide RMSNorm, with the sum in place of the mean — no division by `n`.
    pub fn l2_norm_eps(&mut self, x: Ref, eps_zero: Ref, eps_shift: Ref) -> Ref {
        // `RMS(x; eps)` of a row whose "length" is one: the machinery divides by the last axis, so
        // the sum is formed here and handed over as a one-lane mean of squares.
        use crate::arith::{K, ONE};
        use crate::prim::{Cmp, Rounding};
        let sh = self.shape(x);
        let axis = sh.len() - 1;
        let sq = self.mul(x, x, DType::I64);
        let sum = self.reduce_sum(sq, axis, DType::I128);
        let one = self.c(DType::I64, ONE);
        let scaled = self.mul(sum, one, DType::I128);
        let ez = self.clamp(eps_zero, 0, 1 << 30, DType::I64);
        let es = self.pow2_128_of(eps_shift, 96);
        let eps = self.mul(ez, es, DType::I128);
        let eps = self.mul(eps, one, DType::I128);
        let v = self.add(scaled, eps, DType::I128);
        // `v` reaches `2^126·2^24`: bound it where the exponent extraction can take it (a row this
        // loud has a unit direction that no longer depends on eps).
        let v = self.clamp(v, 0, i64::MAX, DType::I64);
        let bit = self.log2_floor(v, DType::I32);
        let k = self.c(DType::I32, K as i128);
        let t = self.sub(bit, k, DType::I32);
        let two = self.c(DType::I32, 2);
        let h = self.div(t, two, Rounding::Floor, DType::I32);
        let two_h = self.mul(h, two, DType::I32);
        let zero = self.c(DType::I32, 0);
        let rp = self.pow2_128_of(two_h, 62);
        let right = self.div(v, rp, Rounding::Floor, DType::I64);
        let neg2h = self.sub(zero, two_h, DType::I32);
        let lp = self.pow2_128_of(neg2h, 24);
        let small = self.clamp(v, 0, ONE as i64, DType::I64);
        let left = self.mul(small, lp, DType::I128);
        let ge = self.compare(two_h, zero, Cmp::Ge);
        let m = self.select(ge, right, left, DType::I128);
        let m = self.clamp(m, 0, i64::MAX, DType::I64);
        let r = self.int_rsqrt(m);
        let prod = self.mul(x, r, DType::I128);
        let dp = self.pow2_128_of(h, 62);
        let rshift = self.div(prod, dp, Rounding::Floor, DType::I128);
        let negh = self.sub(zero, h, DType::I32);
        let up = self.pow2_128_of(negh, 12);
        let lshift = self.mul(prod, up, DType::I128);
        let hge = self.compare(h, zero, Cmp::Ge);
        let y = self.select(hge, rshift, lshift, DType::I128);
        let y = self.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let empty = self.compare(v, zero, Cmp::Le);
        self.select(empty, zero, y, DType::I32)
    }
}
