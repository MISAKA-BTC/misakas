//! **Activations**, in two forms.
//!
//! * **Narrow inputs — a table.** An activation on A16 codes is data: `Gather(T, code + 32768)`
//!   over a 65,536-entry param holding the float function rounded on the code grid
//!   ([`BlockBuilder::act_table`]; PALW-EX-5: a transcendental at registration is data). Every
//!   activation — SiLU, GELU (erf or tanh), quick-GELU, ReLU², sigmoid, tanh, softplus — is then the
//!   same two nodes, and a wide input is narrowed to codes first ([`BlockBuilder::act_table_wide`]:
//!   one lossy site before the table).
//! * **Wide inputs — composed.** On a Q24 `i32` value the activation is written out of `IntExp`,
//!   `IntRsqrt` and exact arithmetic: [`BlockBuilder::silu`] (legacy), [`BlockBuilder::tanh_q24`],
//!   [`BlockBuilder::gelu_tanh_q24`], [`BlockBuilder::gelu_erf_q24`] (Abramowitz–Stegun 7.1.26 —
//!   there is no `erf` primitive), [`BlockBuilder::quick_gelu_q24`], [`BlockBuilder::relu2_q24`] and
//!   gpt-oss's clamped SwiGLU.
//!
//! The Q24 constants are the float constants rounded to nearest at `2^24`, computed once when the
//! template runs; they are consts of the program and part of its identity.

use crate::arith::ONE;
use crate::builder::BlockBuilder;
use crate::library::Narrowing;
use crate::prim::{Cmp, Rounding};
use crate::program::Ref;
use crate::types::DType;

/// A float constant in Q24, rounded to nearest.
pub fn q24(v: f64) -> i128 {
    (v * ONE as f64).round() as i128
}

impl BlockBuilder<'_> {
    /// `Clamp(x, 0, max)`: ReLU, in `x`'s dtype.
    pub fn relu(&mut self, x: Ref) -> Ref {
        let dt = self.ty(x).dtype;
        let hi = dt.max_value().min(i64::MAX as i128) as i64;
        self.clamp(x, 0, hi, dt)
    }

    /// Squared ReLU on Q24: `(relu(x)²) >> 24`, `i64` (Nemotron, RWKV's channel mix).
    pub fn relu2_q24(&mut self, x: Ref) -> Ref {
        let r = self.relu(x);
        self.mul_q24(r, r, DType::I64)
    }

    /// `tanh(x) = 2·σ(2x) − 1` on Q24 (`i32` in, `i32` out in `[−ONE, ONE]`, up to the sigmoid's
    /// own rounding). `2x` saturates at the `i32` rail, where σ has long saturated.
    pub fn tanh_q24(&mut self, x: Ref) -> Ref {
        let two = self.c(DType::I32, 2);
        let x2 = self.mul(x, two, DType::I64);
        let x2 = self.clamp(x2, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let s = self.int_sigmoid(x2);
        let s2 = self.mul(s, two, DType::I64);
        let one = self.c(DType::I32, ONE);
        self.sub(s2, one, DType::I32)
    }

    /// `x · σ(z)` on Q24, `z` already formed — the shape every σ-gated activation shares.
    fn gated_by_sigmoid(&mut self, x: Ref, z: Ref) -> Ref {
        let z = self.clamp(z, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let s = self.int_sigmoid(z);
        self.mul_q24(x, s, DType::I32)
    }

    /// **GELU, tanh form** (`gelu_new`, `gelu_pytorch_tanh`) on Q24:
    /// `x · σ(2·√(2/π)·(x + 0.044715·x³))`, which is `0.5·x·(1 + tanh(√(2/π)(x + 0.044715x³)))`.
    pub fn gelu_tanh_q24(&mut self, x: Ref) -> Ref {
        let x2 = self.mul_q24(x, x, DType::I64);
        let x3 = self.mul_q24(x2, x, DType::I64);
        let c1 = self.c(DType::I64, q24(0.044715));
        let t = self.mul_q24(x3, c1, DType::I64);
        let inner = self.add(x, t, DType::I64);
        let c2 = self.c(DType::I64, q24(2.0 * (2.0 / std::f64::consts::PI).sqrt()));
        let z = self.mul_q24(inner, c2, DType::I64);
        self.gated_by_sigmoid(x, z)
    }

    /// **quick-GELU** (`x · σ(1.702·x)`, CLIP-style and gpt-oss's gate) on Q24.
    pub fn quick_gelu_q24(&mut self, x: Ref) -> Ref {
        let c = self.c(DType::I64, q24(1.702));
        let z = self.mul_q24(x, c, DType::I64);
        self.gated_by_sigmoid(x, z)
    }

    /// **GELU, exact (erf) form** on Q24: `x · Φ(x)`, `Φ(x) = (1 + erf(x/√2)) / 2`, with `erf` by
    /// Abramowitz–Stegun 7.1.26 — `erf(z) = 1 − t·(a1 + t·(a2 + t·(a3 + t·(a4 + t·a5))))·e^{−z²}`,
    /// `t = 1/(1 + p·z)` for `z ≥ 0` (absolute error `≤ 1.5·10^−7` in exact arithmetic) — built
    /// from `IntRecip`, `IntExp` and a Horner chain. `erf` is odd, so the sign selects `1 ± erf`.
    pub fn gelu_erf_q24(&mut self, x: Ref) -> Ref {
        const P: f64 = 0.327_591_1;
        const A: [f64; 5] = [0.254_829_592, -0.284_496_736, 1.421_413_741, -1.453_152_027, 1.061_405_429];
        let zero = self.c(DType::I32, 0);
        let neg = self.compare(x, zero, Cmp::Lt);
        let nx = self.sub(zero, x, DType::I64);
        let ax = self.select(neg, nx, x, DType::I64);
        let inv_sqrt2 = self.c(DType::I64, q24(std::f64::consts::FRAC_1_SQRT_2));
        let z = self.mul_q24(ax, inv_sqrt2, DType::I64);
        let p = self.c(DType::I64, q24(P));
        let pz = self.mul_q24(p, z, DType::I64);
        let one = self.c(DType::I64, ONE);
        let den = self.add(pz, one, DType::I64);
        let t = self.int_recip(den);
        let t = self.clamp(t, 0, ONE as i64, DType::I32);
        let mut acc = self.c(DType::I64, q24(A[4]));
        for a in A[..4].iter().rev() {
            let ta = self.mul_q24(t, acc, DType::I64);
            let ai = self.c(DType::I64, q24(*a));
            acc = self.add(ta, ai, DType::I64);
        }
        let poly = self.mul_q24(t, acc, DType::I64);
        let z2 = self.mul_q24(z, z, DType::I64);
        let nz2 = self.sub(zero, z2, DType::I64);
        let arg = self.clamp(nz2, i32::MIN as i64, 0, DType::I32);
        let e = self.int_exp(arg);
        let pe = self.mul_q24(poly, e, DType::I64);
        let erf = self.sub(one, pe, DType::I64);
        let erf = self.clamp(erf, 0, ONE as i64, DType::I32);
        let lo = self.sub(one, erf, DType::I64);
        let hi = self.add(one, erf, DType::I64);
        let two_phi = self.select(neg, lo, hi, DType::I64);
        let prod = self.mul(x, two_phi, DType::I128);
        let y = self.shr(prod, crate::arith::K + 1, Rounding::Floor, DType::I128);
        self.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// **gpt-oss's clamped SwiGLU** on Q24 (`GptOssExperts._apply_gate`): `g = min(gate, limit)`,
    /// `u = clamp(up, −limit, limit)`, `(u + 1) · g · σ(α·g)`. `limit` and `α` are Q24 constants
    /// (the model hard-codes 7.0 and 1.702). Output Q24 `i32`.
    pub fn swiglu_clamped_q24(&mut self, gate: Ref, up: Ref, limit: f64, alpha: f64) -> Ref {
        let lim = q24(limit) as i64;
        let g = self.clamp(gate, i32::MIN as i64, lim, DType::I32);
        let u = self.clamp(up, -lim, lim, DType::I32);
        let a = self.c(DType::I64, q24(alpha));
        let z = self.mul_q24(g, a, DType::I64);
        let glu = self.gated_by_sigmoid(g, z);
        let one = self.c(DType::I32, ONE);
        let u1 = self.add(u, one, DType::I64);
        self.mul_q24(u1, glu, DType::I32)
    }

    /// **An activation on codes as a table**: `Gather(table, Cast_idx(x + 32768))` for `i16` codes
    /// `x` and a 65,536-entry `table` param (any dtype — `i16` codes out, or Q24 `i32`). The entry
    /// at `c + 32768` is the float function at code `c`, rounded — the exact function on the grid.
    pub fn act_table(&mut self, x: Ref, table: Ref) -> Ref {
        assert_eq!(self.ty(x).dtype, DType::I16, "a table is indexed by i16 codes");
        let off = self.c(DType::I32, 32768);
        let i = self.add(x, off, DType::I32);
        let i = self.cast(i, DType::Idx);
        self.gather(table, i, 0, 0)
    }

    /// [`Self::act_table`] of a wide value: narrowed to codes first (the one lossy site).
    pub fn act_table_wide(&mut self, x: Ref, n: &Narrowing, table: Ref) -> Ref {
        let c = self.narrow_codes(x, n);
        self.act_table(c, table)
    }
}
