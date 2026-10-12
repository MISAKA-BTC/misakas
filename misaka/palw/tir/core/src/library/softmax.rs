//! **Softmax variants.** The two-pass softmax is [`BlockBuilder::softmax_shifted`] (the maximum,
//! then exact exponent sums against it — the form the court dissects over `H`, ADR-0082). Here:
//!
//! * attention SINKS (gpt-oss): an extra logit per head in the denominator only — its probability
//!   is dropped, so no `Concat` along `H` is needed;
//! * tanh SOFT-CAPPING (Gemma 2): `cap · tanh(x / cap)` on logits or scores, before the maximum.

use crate::arith::{K, ONE};
use crate::builder::BlockBuilder;
use crate::prim::Rounding;
use crate::program::Ref;
use crate::types::DType;

impl BlockBuilder<'_> {
    /// **Softmax with a sink** along the last axis: `m = max(max_j x_j, sink)`, `e_j` as in
    /// [`Self::softmax_shifted`] against `m`, the denominator `Σ_j e_j + IntExp(sink − m)`, and
    /// the probabilities of the row only. `sink` broadcasts against the row's leading axes with
    /// a last axis of 1 (one logit per head, at the row's scale). Q24 out.
    pub fn softmax_with_sink(&mut self, x: Ref, sink: Ref, up_bits: u32) -> Ref {
        let axis = self.shape(x).len() - 1;
        let up = up_bits.min(62);
        let row_max = self.reduce_max(x, axis);
        let row_max = self.cast(row_max, DType::I64);
        let sink = self.cast(sink, DType::I64);
        let m = self.max2(row_max, sink, DType::I64);
        let floor = (i32::MIN as i64) >> up;
        let scale = self.c(DType::I64, 1i128 << up);
        let exp_of = |b: &mut Self, v: Ref| {
            let diff = b.sub(v, m, DType::I64);
            let d = b.clamp(diff, floor, 0, DType::I64);
            let w = b.mul(d, scale, DType::I64);
            let arg = b.clamp(w, i32::MIN as i64, 0, DType::I32);
            b.int_exp(arg)
        };
        let e = exp_of(self, x);
        let es = exp_of(self, sink);
        let sum = self.reduce_sum(e, axis, DType::I64);
        let sum = self.add(sum, es, DType::I64);
        let recip = self.int_recip(sum);
        let p = self.mul(e, recip, DType::I128);
        let q = self.shr(p, K, Rounding::Floor, DType::I64);
        self.clamp(q, 0, 1 << 25, DType::I32)
    }

    /// **Soft-capping** `cap · tanh(x / cap)` on Q24 values (`i32` in and out). `cap` is a Q24
    /// value at `x`'s scale, a const or a param; it is used as a divisor, so it is clamped to `≥ 1`
    /// (a never-firing clamp for any real cap, and what makes the range analysis total).
    pub fn softcap_q24(&mut self, x: Ref, cap: Ref) -> Ref {
        let one = self.c(DType::I64, ONE);
        let xs = self.mul(x, one, DType::I64);
        let c = self.clamp(cap, 1, i32::MAX as i64, DType::I32);
        let u = self.div(xs, c, Rounding::Floor, DType::I64);
        let u = self.clamp(u, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let t = self.tanh_q24(u);
        self.mul_q24(t, c, DType::I32)
    }
}
