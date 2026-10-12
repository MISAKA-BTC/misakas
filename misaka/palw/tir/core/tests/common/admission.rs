//! Programs that pin `tir_admit_v1`'s derived numbers where a reading of spec 04b §10.3 could
//! differ (the second implementation's A1–A7): which replays split into groups, what one group's
//! replay costs, the value a `C_j` of 0 is refused with, and a `Fixed` state no block writes. Shared
//! by `tests/admit.rs` (the assertions) and `tests/golden.rs` (`admission.json`).
#![allow(dead_code)]

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS};
use misaka_palw_tir::{DType, Dim, Ref, TensorType, TirProgramV1};

/// The state range every program here keeps its `Fixed` values in.
pub const R: i64 = 1 << 20;

/// `pre` (a committed `[1]` carry), one layer block built by `layer`, and `post` (the carry,
/// committed, as the logits). `layer` gets the block builder and returns nothing: its roots are its
/// `StateWrite`s and commit points, and the carry passes through.
pub fn one_layer(pb: ProgramBuilder, layer: impl FnOnce(&mut BlockBuilder<'_>)) -> TirProgramV1 {
    let mut pb = pb;
    let carry = TensorType::fixed(DType::I32, &[1]);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let c = b.iota(DType::I32, &[Dim::Fixed(1)], 0, 0, 0);
        b.finish(&[c])
    };
    let block = {
        let mut b = pb.block("layer", vec![carry.clone()]);
        layer(&mut b);
        let out = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.finish(&[out])
    };
    let post = {
        let mut b = pb.block("post", vec![carry]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![block], post, 0)
}

fn builder() -> ProgramBuilder {
    ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL)
}

/// **A1, a free update**: `S[4, 8] ← Clamp(P)`. The update reads no closure state, so every node
/// is free and the replay splits four ways — each group computing the free nodes whole.
pub fn free_update() -> TirProgramV1 {
    let mut pb = builder();
    let p = pb.param("p", DType::I32, &[4, 8], false);
    let s = pb.fixed_state("S", DType::I32, &[4, 8], -R, R, true);
    one_layer(pb, |b| {
        let x = b.clamp(p, -R, R, DType::I32);
        b.state_write(s, x);
    })
}

/// **A1, a free node inside an aligned update**: `S ← S + Clamp(P)`. The `Clamp` is free, the `Add`
/// and the `StateWrite` aligned: four groups, each paying a quarter of the aligned cost and the
/// whole `Clamp`.
pub fn free_node_in_an_aligned_update() -> TirProgramV1 {
    let mut pb = builder();
    let p = pb.param("p", DType::I32, &[4, 8], false);
    let s = pb.fixed_state("S", DType::I32, &[4, 8], -R, R, true);
    one_layer(pb, |b| {
        let x = b.clamp(p, -R, R, DType::I32);
        let y = b.add(Ref::State(s), x, DType::I32);
        b.state_write(s, y);
    })
}

/// **A1, a commit point is a free leaf — even another member's committed `StateWrite`**. `S1 ← S1 +
/// Clamp(P)` with its `StateWrite` committed; `S2 ← S2 + S1 + Reshape(Transpose(w1))`. `S2`'s closure
/// is `{S1, S2}` (it reads `S1`), and its cone stops at the committed `w1`, which the court opens at
/// every position: the `Transpose` of an opened value is free, so the replay splits. Followed as a
/// node instead, the `Transpose` (axis 0 moved) would block it.
pub fn a_committed_member_write_is_a_free_leaf() -> TirProgramV1 {
    let mut pb = builder();
    let p = pb.param("p", DType::I32, &[4, 8], false);
    let s1 = pb.fixed_state("S1", DType::I32, &[4, 8], -R, R, true);
    let s2 = pb.fixed_state("S2", DType::I32, &[4, 8], -R, R, true);
    one_layer(pb, |b| {
        let x = b.clamp(p, -R, R, DType::I32);
        let a1 = b.add(Ref::State(s1), x, DType::I32);
        let w1 = b.state_write(s1, a1);
        b.commit(w1);
        let t = b.transpose(w1, &[1, 0]);
        let t = b.reshape_fixed(t, &[4, 8]);
        let a2 = b.add(Ref::State(s2), Ref::State(s1), DType::I32);
        let a2 = b.add(a2, t, DType::I32);
        b.state_write(s2, a2);
    })
}

/// **A1, a reduction across groups**: `S ← Broadcast(ReduceSum(S, axis 0))`. The sum mixes the
/// groups, so the replay is whole (`G = 1`).
pub fn a_reduction_across_groups() -> TirProgramV1 {
    let mut pb = builder();
    let s = pb.fixed_state("S", DType::I32, &[4, 8], -R, R, true);
    one_layer(pb, |b| {
        let r = b.reduce_sum(Ref::State(s), 0, DType::I64);
        let r = b.broadcast(r, &[Dim::Fixed(4), Dim::Fixed(8)]);
        let r = b.clamp(r, -R, R, DType::I32);
        b.state_write(s, r);
    })
}

/// **A1, a member of another width — and A7, a `Fixed` state no block writes.** `S[4, 8] ← S +
/// Broadcast(ReduceMax(T, axis 0))` with `T[2, 8]` read and never written: `T` is in `S`'s closure
/// and its axis 0 is not 4, so the replay is whole; `T` has no `C_j` of its own and does not enter
/// `C`.
pub fn a_member_of_another_width_and_a_state_nobody_writes() -> TirProgramV1 {
    let mut pb = builder();
    let s = pb.fixed_state("S", DType::I32, &[4, 8], -R, R, true);
    let t = pb.fixed_state("T", DType::I32, &[2, 8], -R, R, true);
    one_layer(pb, |b| {
        let m = b.reduce_max(Ref::State(t), 0);
        let m = b.broadcast(m, &[Dim::Fixed(4), Dim::Fixed(8)]);
        let y = b.add(Ref::State(s), m, DType::I32);
        b.state_write(s, y);
    })
}

/// **A3, a replay past the MAC ceiling**: `S[2, 4, 4] ← Clamp(S · P)`, a product batched over the
/// groups (`P[4, 4]` free), 128 MACs a position and 64 a group; the committed outputs read nothing
/// of it, so no tile is past a ceiling and only the replay is.
pub fn a_replay_of_matmuls() -> TirProgramV1 {
    let mut pb = builder();
    let p = pb.param("p", DType::I16, &[4, 4], false);
    let s = pb.fixed_state("S", DType::I16, &[2, 4, 4], -(1 << 14), 1 << 14, true);
    one_layer(pb, |b| {
        let y = b.matmul(Ref::State(s), p, DType::I64);
        let y = b.clamp(y, -(1 << 14), 1 << 14, DType::I16);
        b.state_write(s, y);
    })
}

/// **A3, a replay past the transcendental ceiling**: `S[2, 4] ← IntExp(Clamp(S))`, 8 evaluations a
/// position and 4 a group, no MACs.
pub fn a_replay_of_transcendentals() -> TirProgramV1 {
    let mut pb = builder();
    let s = pb.fixed_state("S", DType::I32, &[2, 4], -R, R, true);
    one_layer(pb, |b| {
        let x = b.clamp(Ref::State(s), -R, 0, DType::I32);
        let e = b.int_exp(x);
        b.state_write(s, e);
    })
}

/// A delta-rule layer whose per-position operands arrive committed (here: params, gathered by
/// position): the update of `S[heads, d_v, d_k]` is head-local, so its replay splits per head.
pub fn head_local_delta_rule(heads: u32, dv: u32, dk: u32) -> TirProgramV1 {
    let t = 16u32;
    let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let k = pb.param("k", DType::I16, &[t, heads * dk], false);
    let v = pb.param("v", DType::I16, &[t, heads * dv], false);
    let q = pb.param("q", DType::I16, &[t, heads * dk], false);
    let dec = pb.param("decay", DType::I32, &[t, heads], false);
    let beta = pb.param("beta", DType::I32, &[t, heads], false);
    let tri: Vec<Ref> = ["r.m", "r.s", "r.z", "d.m", "d.s", "d.z", "ws", "o.m", "o.s", "o.z"]
        .iter()
        .map(|n| {
            pb.param(
                n,
                if n.ends_with(".s") {
                    DType::I8
                } else if *n == "ws" {
                    DType::I32
                } else {
                    DType::I64
                },
                &[heads],
                true,
            )
        })
        .collect();
    let s = pb.fixed_state("S", DType::I32, &[heads, dv, dk], -(i32::MAX as i64), i32::MAX as i64, true);
    let carry = vec![TensorType::fixed(DType::I32, &[heads * dv])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let z = b.iota(DType::I32, &[Dim::Fixed(heads * dv)], 0, 0, 0);
        let z = b.commit(z);
        b.finish(&[z])
    };
    let layer = {
        let mut b = pb.block("delta", carry.clone());
        let pos = b.clamp(Ref::Input(INPUT_POS), 0, t as i64 - 1, DType::Idx);
        let at = |b: &mut BlockBuilder<'_>, table: Ref, shape: &[u32]| {
            let r = b.gather(table, pos, 0, 0);
            let r = b.reshape_fixed(r, shape);
            b.commit(r)
        };
        let (kk, vv, qq) = (at(&mut b, k, &[heads, dk]), at(&mut b, v, &[heads, dv]), at(&mut b, q, &[heads, dk]));
        let dd = at(&mut b, dec, &[heads]);
        let dd = b.clamp(dd, 0, 1 << 24, DType::I32);
        let bb = at(&mut b, beta, &[heads]);
        let bb = b.clamp(bb, 0, 1 << 24, DType::I32);
        let rs = b.pow2_of(tri[1]);
        let ds = b.pow2_of(tri[4]);
        let os = b.pow2_of(tri[8]);
        let o = b.gdn_step_q36(s, kk, vv, qq, dd, bb, (tri[0], rs, tri[2]), (tri[3], ds, tri[5]), tri[6], (tri[7], os, tri[9]));
        let o = b.reshape_fixed(o, &[heads * dv]);
        let out = b.add(o, Ref::CarryIn(0), DType::I64);
        let out = b.clamp(out, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.finish(&[out])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[heads * dv]);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![layer, layer], post, 0)
}

/// **A6, `Overflow`**: two `i32` params multiplied into an `i32` — the obligation `⊆ out dtype` fails.
pub fn an_overflow() -> TirProgramV1 {
    let mut pb = builder();
    let a = pb.param("a", DType::I32, &[4], false);
    let b2 = pb.param("b", DType::I32, &[4], false);
    one_layer(pb, |b| {
        let m = b.mul(a, b2, DType::I32);
        b.commit(m);
    })
}

/// **A6, `Index`**: a 4-row table gathered by the position, which reaches `history_bound − 1`.
pub fn an_index_out_of_range() -> TirProgramV1 {
    let mut pb = builder();
    let table = pb.param("table", DType::I32, &[4, 2], false);
    one_layer(pb, |b| {
        let r = b.gather(table, Ref::Input(INPUT_POS), 0, 0);
        b.commit(r);
    })
}

/// **A6, `Divisor`**: a division by a param whose interval reaches below 1.
pub fn a_divisor_below_one() -> TirProgramV1 {
    let mut pb = builder();
    let x = pb.param("x", DType::I32, &[4], false);
    let d = pb.param("d", DType::I32, &[4], false);
    one_layer(pb, |b| {
        let q = b.div(x, d, misaka_palw_tir::Rounding::Floor, DType::I32);
        b.commit(q);
    })
}

/// **A1, where the split decides the verdict — free nodes are paid whole.** `S[4, 8] ← Clamp(P1 ·
/// P2)`: the product reads no state, so it is free and every group computes it whole — 512 MACs a
/// group, not 128. Against a 256-MAC tile ceiling the class is refused (`C_j = 0`).
pub fn a_free_product_update() -> TirProgramV1 {
    let mut pb = builder();
    let p1 = pb.param("p1", DType::I16, &[4, 16], false);
    let p2 = pb.param("p2", DType::I16, &[16, 8], false);
    let s = pb.fixed_state("S", DType::I32, &[4, 8], -R, R, true);
    one_layer(pb, |b| {
        let y = b.matmul(p1, p2, DType::I64);
        let y = b.clamp(y, -R, R, DType::I32);
        b.state_write(s, y);
    })
}

/// **A1, where the split decides the verdict — the commit-point rule.** `S1[4, 4, 4] ← Clamp(S1 · P)`
/// with its `StateWrite` `w1` committed, and `S2 ← Clamp(S2 · P + S1 + Reshape(Transpose(w1, [1, 0,
/// 2])))`. `S2`'s closure is `{S1, S2}`: 512 aligned MACs a position, 128 a group when it splits —
/// which it does because `w1`, a commit point, is a free leaf. Against a 128-MAC ceiling it is
/// admitted with `C_j = 1`; with `w1` followed as a node the `Transpose` would mix the groups and the
/// whole 512 would be refused.
pub fn a_committed_member_write_decides_the_split() -> TirProgramV1 {
    let mut pb = builder();
    let p = pb.param("p", DType::I16, &[4, 4], false);
    let s1 = pb.fixed_state("S1", DType::I16, &[4, 4, 4], -(1 << 14), 1 << 14, true);
    let s2 = pb.fixed_state("S2", DType::I16, &[4, 4, 4], -(1 << 14), 1 << 14, true);
    one_layer(pb, |b| {
        let y1 = b.matmul(Ref::State(s1), p, DType::I64);
        let y1 = b.clamp(y1, -(1 << 14), 1 << 14, DType::I16);
        let w1 = b.state_write(s1, y1);
        b.commit(w1);
        let t = b.transpose(w1, &[1, 0, 2]);
        let t = b.reshape_fixed(t, &[4, 4, 4]);
        let y2 = b.matmul(Ref::State(s2), p, DType::I64);
        let y2 = b.add(y2, Ref::State(s1), DType::I64);
        let y2 = b.add(y2, t, DType::I64);
        let y2 = b.clamp(y2, -(1 << 14), 1 << 14, DType::I16);
        b.state_write(s2, y2);
    })
}
