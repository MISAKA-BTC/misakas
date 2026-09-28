//! **Routing and the mixture** (corpus C3, C8).
//!
//! * scores: softmax ([`BlockBuilder::softmax_shifted`]) or sigmoid ([`BlockBuilder::route_sigmoid`]);
//! * a selection-only bias (DeepSeek-V3's `e_score_correction_bias`): add it to the scores the
//!   `TopK` reads and GATHER THE WEIGHTS FROM THE UNBIASED SCORES — two tensors, one selection;
//! * grouped top-k ([`BlockBuilder::grouped_topk`]): groups ranked by their maximum (V2) or by the
//!   sum of their top two (V3/Kimi), `topk_group` groups kept, the rest masked, then top-k;
//! * renormalisation: exact division ([`BlockBuilder::renormalize_div`], HF's `w / Σw`) or the
//!   legacy `IntRecip` form ([`BlockBuilder::renormalize_recip`]); a routed scaling factor
//!   ([`BlockBuilder::scale_q24`]);
//! * the combine: ONE exact accumulator ([`BlockBuilder::moe_combine_q36`], ADR-0052 C).
//!
//! Every `TopK` is a commit point (PALW-TIR-11); its output is in index order with ties to the
//! lowest index, which makes the committed selection a function of the scores.

use crate::arith::{K, ONE};
use crate::builder::BlockBuilder;
use crate::prim::{Cmp, Rounding};
use crate::program::Ref;
use crate::types::{DType, Dim};

/// How a group of experts is scored for group-limited routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupScore {
    /// The group's maximum score (DeepSeek-V2's `group_limited_greedy`).
    Max,
    /// The sum of the group's two best scores (DeepSeek-V3, Kimi).
    Top2Sum,
}

impl BlockBuilder<'_> {
    /// Sigmoid routing scores (DeepSeek-V3, Kimi): `σ(logits)` on Q24.
    pub fn route_sigmoid(&mut self, logits: Ref) -> Ref {
        self.int_sigmoid(logits)
    }

    /// The scores a selection reads: `scores + bias` (both Q24 `i32`), saturated to `i32`. The
    /// weights are gathered from `scores`, never from this.
    pub fn selection_bias(&mut self, scores: Ref, bias: Ref) -> Ref {
        let s = self.add(scores, bias, DType::I64);
        self.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// **Group-limited top-k** over a score row `sel:[E]` (`i32`): the experts split into `groups`
    /// contiguous groups, each scored by `rule`, the best `topk_group` groups kept (a committed
    /// `TopK`), the others' scores replaced by `fill` (HF 5.x: the dtype minimum, `−∞`; the
    /// remote V3 code: 0), then the committed top `k`. Returns the selected expert indices `[k]`.
    pub fn grouped_topk(&mut self, sel: Ref, groups: u32, topk_group: u32, k: u32, rule: GroupScore, fill: i64) -> Ref {
        let Dim::Fixed(e) = self.shape(sel)[0] else { panic!("static") };
        let per = e / groups;
        assert_eq!(per * groups, e, "the groups divide the experts");
        let sg = self.reshape_fixed(sel, &[groups, per]);
        let gs = match rule {
            GroupScore::Max => self.reduce_max(sg, 1),
            GroupScore::Top2Sum => {
                let i2 = self.topk(sg, 1, 2);
                let v2 = self.gather(sg, i2, 1, 1);
                self.reduce_sum(v2, 1, DType::I64)
            }
        };
        let gs = self.reshape_fixed(gs, &[groups]);
        let gi = self.topk(gs, 0, topk_group);
        let rows = self.iota(DType::Idx, &[Dim::Fixed(groups), Dim::Fixed(1)], 0, 0, 1);
        let gi = self.reshape_fixed(gi, &[1, topk_group]);
        let hit = self.compare(rows, gi, Cmp::Eq);
        let keep = self.reduce_max(hit, 1);
        let keep = self.broadcast(keep, &[Dim::Fixed(groups), Dim::Fixed(per)]);
        let f = self.c(DType::I32, fill as i128);
        let masked = self.select(keep, sg, f, DType::I32);
        let masked = self.reshape_fixed(masked, &[e]);
        self.topk(masked, 0, k)
    }

    /// **Exact renormalisation** `w·ONE / Σw` (floor) of Q24 weights along the last axis — HF's
    /// `w / (Σw + ε)` with the division done exactly. The sum is clamped to `≥ 1` (it is positive
    /// whenever a selected score is), so the division is total.
    pub fn renormalize_div(&mut self, w: Ref) -> Ref {
        let axis = self.shape(w).len() - 1;
        let sum = self.reduce_sum(w, axis, DType::I64);
        let sum = self.clamp(sum, 1, i64::MAX, DType::I64);
        let one = self.c(DType::I64, ONE);
        let num = self.mul(w, one, DType::I64);
        let q = self.div(num, sum, Rounding::Floor, DType::I64);
        self.clamp(q, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// **The legacy renormalisation** `(w · IntRecip(Σw)) >> 24` along the last axis (the form
    /// `q36_router_topk` uses).
    pub fn renormalize_recip(&mut self, w: Ref) -> Ref {
        let axis = self.shape(w).len() - 1;
        let sum = self.reduce_sum(w, axis, DType::I64);
        let recip = self.int_recip(sum);
        let p = self.mul(w, recip, DType::I128);
        let q = self.shr(p, K, Rounding::Floor, DType::I128);
        self.clamp(q, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// `(w · f) >> 24` for a Q24 factor `f` (DeepSeek's `routed_scaling_factor`), `i32` out.
    pub fn scale_q24(&mut self, w: Ref, f: Ref) -> Ref {
        self.mul_q24(w, f, DType::I32)
    }
}
