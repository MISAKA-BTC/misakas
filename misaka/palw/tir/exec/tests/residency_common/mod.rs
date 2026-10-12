//! The fixtures the residency's tests share (`tests/residency.rs`, `tests/residency_node.rs`).
#![allow(dead_code)]

use std::collections::BTreeMap;

use crate::tircommon::models::*;
use misaka_palw_tir::program::INPUT_TOKEN;
use misaka_palw_tir::{DType, Ref, Rounding, TensorType, TirProgramV1};

/// **A mixture with many experts** — 64 a layer over four layers, two routed a token, with the
/// gate's per-(expert, row) multipliers a flat `[E·EF]` vector reshaped to `[E, EF]` and gathered by
/// the route, as the lowerer writes a narrowing's scales. The fixture the residency's identity is
/// held at when a budget holds a fraction of the experts.
pub fn many_expert_moe(experts: u32, k: u32, layers: usize) -> (TirProgramV1, BTreeMap<u16, Gen>) {
    moe_with(experts, k, layers, false)
}

/// **A composite candidate of [`many_expert_moe`]** (RFC-0004 §6.3): the parent's params first,
/// unchanged and at the same instances, then an adapter — a logits bias. Its post block also reads
/// the parent's embedding WHOLE, as a tied unembedding: a parent param the parent's residency serves
/// by rows that this candidate reads densely, which the candidate pins for itself.
pub fn many_expert_moe_candidate(experts: u32, k: u32, layers: usize) -> (TirProgramV1, BTreeMap<u16, Gen>) {
    moe_with(experts, k, layers, true)
}

fn moe_with(experts: u32, k: u32, layers: usize, candidate: bool) -> (TirProgramV1, BTreeMap<u16, Gen>) {
    const V: u32 = 48;
    const D: u32 = 16;
    const EF: u32 = 8;
    let mut m = Model::new(V);
    let tok = m.p("tok_embd", DType::I8, &[V, D], false, Gen::Uniform(-128, 127));
    let lift = m.p("tok_embd.lift", DType::I64, &[D], false, Gen::Uniform(200, 300));
    let norm = m.p("blk.norm.g", DType::I64, &[D], true, Gen::Uniform(1 << 13, 1 << 14));
    let wr = m.proj("blk.router", experts, D, true, (1 << 8, 1 << 10));
    let gate = m.p("blk.gate_exps.w", DType::I8, &[experts, EF, D], true, Gen::Uniform(-128, 127));
    let gate_m = m.p("blk.gate_exps.m", DType::I64, &[experts * EF], true, Gen::Uniform(1 << 10, 1 << 12));
    let up = m.p("blk.up_exps.w", DType::I8, &[experts, EF, D], true, Gen::Uniform(-128, 127));
    let down = m.p("blk.down_exps.w", DType::I8, &[experts, D, EF], true, Gen::Uniform(-128, 127));
    let out_norm = m.p("output_norm.g", DType::I64, &[D], false, Gen::Uniform(1 << 13, 1 << 14));
    let lm = m.proj("output", V, D, false, (1 << 8, 1 << 10));
    let carry = vec![TensorType::fixed(DType::I16, &[D])];
    let pre = {
        let mut b = m.pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let p2 = b.c(DType::I64, 1);
        let z = b.c(DType::I64, 0);
        let x = b.narrow_a16(row, lift, p2, z, -32767, 32767, DType::I16);
        b.finish(&[x])
    };
    let layer = {
        let mut b = m.pb.block("moe", carry.clone());
        let x = Ref::CarryIn(0);
        let h = norm_gain(&mut b, x, norm);
        let logits = codes16(&mut b, h, wr, 22);
        let (idx, w) = b.router_topk_q36(logits, k, 4);
        let g = b.gather(gate, idx, 0, 0);
        let u = b.gather(up, idx, 0, 0);
        let hc = b.reshape_fixed(h, &[D, 1]);
        let ga = b.matmul(g, hc, DType::I64);
        let ga = b.reshape_fixed(ga, &[k, EF]);
        let rows = b.reshape_fixed(gate_m, &[experts, EF]);
        let mk = b.gather(rows, idx, 0, 0);
        let ga = b.mul(ga, mk, DType::I64);
        let ga = b.shr(ga, 22, Rounding::HalfAwayFromZero, DType::I64);
        let ga = b.clamp(ga, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let ga = b.reshape_fixed(ga, &[k, EF, 1]);
        let ua = b.matmul(u, hc, DType::I64);
        let ua = b.shr(ua, 10, Rounding::HalfAwayFromZero, DType::I64);
        let ua = b.clamp(ua, -32767, 32767, DType::I16);
        let act = b.silu(ga);
        let pm = b.mul(act, ua, DType::I64);
        let pm = b.shr(pm, 24, Rounding::HalfAwayFromZero, DType::I64);
        let pm = b.clamp(pm, -32767, 32767, DType::I16);
        let dn = b.gather(down, idx, 0, 0);
        let y = b.matmul(dn, pm, DType::I64);
        let y = b.reshape_fixed(y, &[k, D]);
        let y = b.shr(y, 8, Rounding::HalfAwayFromZero, DType::I64);
        let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let one = b.c(DType::I64, 1);
        let p24 = b.c(DType::I64, 1 << 24);
        let zero = b.c(DType::I64, 0);
        let routed = b.moe_combine_q36(y, w, one, p24, zero, -32767, 32767, DType::I16);
        let x2 = residual(&mut b, x, routed);
        b.finish(&[x2])
    };
    let bias = candidate.then(|| m.p("adapter.logits_bias", DType::I32, &[V], false, Gen::Uniform(-64, 64)));
    let post = {
        let mut b = m.pb.block("post", carry);
        let h = norm_gain(&mut b, Ref::CarryIn(0), out_norm);
        let l = linear(&mut b, h, lm, 16, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = match bias {
            Some(bias) => {
                let hc = b.reshape_fixed(h, &[D, 1]);
                let tied = b.matmul(tok, hc, DType::I64);
                let tied = b.reshape_fixed(tied, &[V]);
                let tied = b.shr(tied, 16, Rounding::HalfAwayFromZero, DType::I64);
                let tied = b.clamp(tied, -(1 << 20), 1 << 20, DType::I32);
                let l = b.add(l, tied, DType::I64);
                let l = b.add(l, bias, DType::I64);
                b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32)
            }
            None => l,
        };
        b.commit(l);
        b.finish(&[])
    };
    let logits = (m.pb.blocks[post as usize].nodes.len() - 1) as u16;
    (m.pb.finish(pre, vec![layer; layers], post, logits), m.gens)
}
