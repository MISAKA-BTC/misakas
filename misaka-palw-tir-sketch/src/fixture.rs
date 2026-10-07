//! **Tiny classes for the soundness tests and the measurement tool** — built with the IR's own
//! builder and library templates, the shapes the corpus programs have (`misaka-palw-tir`'s
//! `tests/common/models.rs`), at widths a test runs in milliseconds.
//!
//! * [`dense_moe_v1`] — a two-layer decoder: a dense GQA attention layer with RoPE over a KV history
//!   and a SwiGLU MLP, then a top-2-of-4 MoE layer whose expert matrices are GATHERED by the
//!   committed `TopK` (`routed_expert_matmul` structurally) with a sigmoid-gated shared expert, and a
//!   head. Every kind of `MatMul` a check meets: dense weights on the left, routed experts with a
//!   broadcast and a batched activation, `Q·Kᵀ` and `P·V` over the history, the combine.
//! * [`wide_v1`] — one layer whose accumulators the refined plan cannot narrow: an `i64` product of
//!   an `i32`-wide activation (two moduli) and an `i128` product of `i64` operands (three).
//! * [`wide128_v1`] — one layer whose ranges ARE proven (it passes the exact-result rule) and whose `i64`-weight product needs an
//!   `i128` accumulator: one Mersenne modulus cannot hold its error span, a multi-modulus relation can.
//!
//! Weights are drawn uniformly from each param's stated range by a seeded ChaCha8 stream; nothing
//! here is a model.

use std::collections::BTreeMap;

use misaka_palw_tir::arith::ONE;
use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS, INPUT_TOKEN, Ref, TirProgramV1};
use misaka_palw_tir::{DType, MapParams, Rounding, Tensor, TensorType};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// A fixture class: its program and one artifact's params.
#[derive(Clone, Debug)]
pub struct TirSketchFixtureV1 {
    pub program: TirProgramV1,
    pub params: MapParams,
}

impl TirSketchFixtureV1 {
    /// The params `keep` names, every instance — what a seat holding only those would have.
    pub fn params_only(&self, keep: &std::collections::BTreeSet<u16>) -> MapParams {
        MapParams {
            tensors: self.params.tensors.iter().filter(|((j, _), _)| keep.contains(j)).map(|(k, v)| (*k, v.clone())).collect(),
        }
    }
}

struct Model {
    pb: ProgramBuilder,
    ranges: BTreeMap<u16, (i128, i128)>,
}

impl Model {
    fn new(token_bound: u32) -> Self {
        Self { pb: ProgramBuilder::new(token_bound, HISTORY_BOUND_V1_SMALL), ranges: BTreeMap::new() }
    }

    fn p(&mut self, name: &str, dtype: DType, shape: &[u32], per_layer: bool, lo: i128, hi: i128) -> Ref {
        let r = self.pb.param(name, dtype, shape, per_layer);
        let Ref::Param(j) = r else { unreachable!("a param ref") };
        self.ranges.insert(j, (lo, hi));
        r
    }

    /// An A16 projection: `w [out, in]` codes, a per-channel multiplier and zero.
    fn proj(&mut self, name: &str, out: u32, inp: u32, per_layer: bool, m: (i128, i128)) -> (Ref, Ref, Ref) {
        (
            self.p(&format!("{name}.w"), DType::I8, &[out, inp], per_layer, -128, 127),
            self.p(&format!("{name}.m"), DType::I64, &[out], per_layer, m.0, m.1),
            self.p(&format!("{name}.z"), DType::I64, &[out], per_layer, -40, 40),
        )
    }

    fn finish(self, program: TirProgramV1, seed: u64) -> TirSketchFixtureV1 {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut tensors = BTreeMap::new();
        for (j, d) in program.params.iter().enumerate() {
            let layers: Vec<Option<u16>> = if d.per_layer {
                program
                    .schedule
                    .layers
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| uses(&program, **b, j as u16))
                    .map(|(l, _)| Some(l as u16))
                    .collect()
            } else {
                vec![None]
            };
            let (lo, hi) = self.ranges[&(j as u16)];
            for l in layers {
                let n: usize = d.shape.iter().map(|x| *x as usize).product();
                let data: Vec<i128> = (0..n).map(|_| rng.gen_range(lo..=hi)).collect();
                let shape = d.shape.iter().map(|x| *x as usize).collect();
                tensors.insert((j as u16, l), Tensor::new(d.dtype, shape, data).expect("drawn in range"));
            }
        }
        TirSketchFixtureV1 { program, params: MapParams { tensors } }
    }
}

fn uses(p: &TirProgramV1, block: u8, j: u16) -> bool {
    p.blocks[block as usize].nodes.iter().any(|n| n.inputs.contains(&Ref::Param(j)))
}

/// `narrow(W x)`: the A16 projection into an `i64` accumulator, then its narrowing.
#[allow(clippy::too_many_arguments)]
fn linear(b: &mut BlockBuilder<'_>, x: Ref, w: (Ref, Ref, Ref), out: u32, inp: u32, shift: u32, lo: i64, hi: i64, dt: DType) -> Ref {
    let xc = b.reshape_fixed(x, &[inp, 1]);
    let acc = b.matmul(w.0, xc, DType::I64);
    let acc = b.reshape_fixed(acc, &[out]);
    let p2 = b.c(DType::I64, 1i128 << shift);
    b.narrow_a16(acc, w.1, p2, w.2, lo, hi, dt)
}

fn codes16(b: &mut BlockBuilder<'_>, x: Ref, w: (Ref, Ref, Ref), out: u32, inp: u32, shift: u32) -> Ref {
    linear(b, x, w, out, inp, shift, -32767, 32767, DType::I16)
}

fn residual(b: &mut BlockBuilder<'_>, x: Ref, y: Ref) -> Ref {
    let s = b.add(x, y, DType::I32);
    b.clamp(s, -32767, 32767, DType::I16)
}

fn norm_gain(b: &mut BlockBuilder<'_>, x: Ref, gain: Ref) -> Ref {
    let u = b.rms_norm_a16(x, 1);
    let p2 = b.c(DType::I64, 1i128 << 24);
    let z = b.c(DType::I64, 0);
    b.narrow_a16(u, gain, p2, z, -32767, 32767, DType::I16)
}

/// Vocabulary, width, heads, KV heads, head width, MLP width, experts, expert width.
pub const FX_V: u32 = 32;
pub const FX_D: u32 = 16;
pub const FX_HQ: u32 = 4;
pub const FX_HKV: u32 = 2;
pub const FX_DH: u32 = 4;
pub const FX_F: u32 = 24;
pub const FX_E: u32 = 4;
pub const FX_EF: u32 = 8;

/// **The dense + MoE fixture** (module note): schedule `[dense, moe]`.
pub fn dense_moe_v1(seed: u64) -> TirSketchFixtureV1 {
    let (v, d, hq, hkv, dh, f, e, ef) = (FX_V, FX_D, FX_HQ, FX_HKV, FX_DH, FX_F, FX_E, FX_EF);
    let mut m = Model::new(v);
    let tok = m.p("tok_embd", DType::I8, &[v, d], false, -128, 127);
    let lift = m.p("tok_embd.lift", DType::I64, &[d], false, 200, 300);
    let q24 = (-ONE, ONE);
    let rope = (
        m.p("rope.cos_hi", DType::I32, &[512, dh / 2], false, q24.0, q24.1),
        m.p("rope.sin_hi", DType::I32, &[512, dh / 2], false, q24.0, q24.1),
        m.p("rope.cos_lo", DType::I32, &[512, dh / 2], false, q24.0, q24.1),
        m.p("rope.sin_lo", DType::I32, &[512, dh / 2], false, q24.0, q24.1),
    );
    let gain = (1i128 << 13, 1i128 << 14);
    let attn_norm = m.p("blk.attn_norm.g", DType::I64, &[d], true, gain.0, gain.1);
    let ffn_norm = m.p("blk.ffn_norm.g", DType::I64, &[d], true, gain.0, gain.1);
    let wq = m.proj("blk.attn_q", hq * dh, d, true, (1 << 8, 1 << 10));
    let wk = m.proj("blk.attn_k", hkv * dh, d, true, (1 << 8, 1 << 10));
    let wv = m.proj("blk.attn_v", hkv * dh, d, true, (1 << 8, 1 << 10));
    let wo = m.proj("blk.attn_o", d, hq * dh, true, (1 << 8, 1 << 10));
    let wg = m.proj("blk.ffn_gate", f, d, true, (1 << 10, 1 << 12));
    let wu = m.proj("blk.ffn_up", f, d, true, (1 << 8, 1 << 10));
    let wd = m.proj("blk.ffn_down", d, f, true, (1 << 8, 1 << 10));
    let wmul = m.p("blk.ffn_mul.m", DType::I64, &[f], true, 1 << 12, 1 << 13);
    let moe_norm = m.p("blk.moe_norm.g", DType::I64, &[d], true, gain.0, gain.1);
    let wr = m.proj("blk.router", e, d, true, (1 << 8, 1 << 10));
    let gate_exps = m.p("blk.gate_exps.w", DType::I8, &[e, ef, d], true, -128, 127);
    let up_exps = m.p("blk.up_exps.w", DType::I8, &[e, ef, d], true, -128, 127);
    let down_exps = m.p("blk.down_exps.w", DType::I8, &[e, d, ef], true, -128, 127);
    let wsg = m.proj("blk.shared_gate", ef, d, true, (1 << 10, 1 << 12));
    let wsu = m.proj("blk.shared_up", ef, d, true, (1 << 8, 1 << 10));
    let wsd = m.proj("blk.shared_down", d, ef, true, (1 << 8, 1 << 10));
    let wsgate = m.proj("blk.shared_expert_gate", 1, d, true, (1 << 10, 1 << 12));
    let out_norm = m.p("output_norm.g", DType::I64, &[d], false, gain.0, gain.1);
    let lm = m.proj("output", v, d, false, (1 << 8, 1 << 10));
    let carry = vec![TensorType::fixed(DType::I16, &[d])];

    let pre = {
        let mut b = m.pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let p2 = b.c(DType::I64, 1);
        let z = b.c(DType::I64, 0);
        let x = b.narrow_a16(row, lift, p2, z, -32767, 32767, DType::I16);
        b.finish(&[x])
    };
    let kc = m.pb.hist_state("k_cache", DType::I16, &[hkv, dh], HISTORY_BOUND_V1_SMALL, true);
    let vc = m.pb.hist_state("v_cache", DType::I16, &[hkv, dh], HISTORY_BOUND_V1_SMALL, true);
    let dense = {
        let mut b = m.pb.block("dense", carry.clone());
        let x = Ref::CarryIn(0);
        let (cos, sin) = b.rope_angles_two_level(Ref::Input(INPUT_POS), rope.0, rope.1, rope.2, rope.3, 9);
        let h = norm_gain(&mut b, x, attn_norm);
        let g = hq / hkv;
        let q = codes16(&mut b, h, wq, hq * dh, d, 22);
        let k = codes16(&mut b, h, wk, hkv * dh, d, 22);
        let vv = codes16(&mut b, h, wv, hkv * dh, d, 22);
        let q = b.reshape_fixed(q, &[hq, dh]);
        let k = b.reshape_fixed(k, &[hkv, dh]);
        let vv = b.reshape_fixed(vv, &[hkv, dh]);
        let q = b.rope_pairs(q, cos, sin, -32767, 32767, DType::I16);
        let k = b.rope_pairs(k, cos, sin, -32767, 32767, DType::I16);
        let kh = b.hist_append(kc, k);
        let vh = b.hist_append(vc, vv);
        let qg = b.reshape_fixed(q, &[hkv, g, dh]);
        let kt = b.transpose(kh, &[1, 2, 0]);
        let scores = b.matmul(qg, kt, DType::I64);
        let sm = b.c(DType::I64, 1);
        let sp = b.c(DType::I64, 1 << 14);
        let sz = b.c(DType::I64, 0);
        let logits = b.narrow_a16(scores, sm, sp, sz, -32767, 32767, DType::I16);
        let probs = b.softmax_shifted(logits, 8);
        let pm = b.c(DType::I64, 1 << 15);
        let pp = b.c(DType::I64, 1 << 24);
        let pc = b.narrow_a16(probs, pm, pp, sz, 0, 32767, DType::I16);
        let vt = b.transpose(vh, &[1, 0, 2]);
        let o = b.matmul(pc, vt, DType::I64);
        let op = b.c(DType::I64, 1 << 15);
        let o = b.narrow_a16(o, sm, op, sz, -32767, 32767, DType::I16);
        let o = b.reshape_fixed(o, &[hq * dh]);
        let a = codes16(&mut b, o, wo, d, hq * dh, 22);
        let x1 = residual(&mut b, x, a);
        let x1 = b.commit(x1);
        let h2 = norm_gain(&mut b, x1, ffn_norm);
        let gate = linear(&mut b, h2, wg, f, d, 14, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let up = codes16(&mut b, h2, wu, f, d, 22);
        let act = b.silu(gate);
        let prod = b.mul(act, up, DType::I64);
        let p2 = b.c(DType::I64, 1i128 << 36);
        let z = b.c(DType::I64, 0);
        let mu = b.narrow_a16(prod, wmul, p2, z, -32767, 32767, DType::I16);
        let dn = codes16(&mut b, mu, wd, d, f, 22);
        let x2 = residual(&mut b, x1, dn);
        b.finish(&[x2])
    };
    let moe = {
        let mut b = m.pb.block("moe", carry.clone());
        let x = Ref::CarryIn(0);
        let h = norm_gain(&mut b, x, moe_norm);
        let logits = codes16(&mut b, h, wr, e, d, 22);
        let (idx, w) = b.router_topk_q36(logits, 2, 4);
        let g = b.gather(gate_exps, idx, 0, 0);
        let u = b.gather(up_exps, idx, 0, 0);
        let dn = b.gather(down_exps, idx, 0, 0);
        let hc = b.reshape_fixed(h, &[d, 1]);
        let ga = b.matmul(g, hc, DType::I64);
        let ua = b.matmul(u, hc, DType::I64);
        let s18 = b.shr(ga, 12, Rounding::HalfAwayFromZero, DType::I64);
        let ga = b.clamp(s18, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let ua = b.shr(ua, 10, Rounding::HalfAwayFromZero, DType::I64);
        let ua = b.clamp(ua, -32767, 32767, DType::I16);
        let act = b.silu(ga);
        let pm = b.mul(act, ua, DType::I64);
        let pm = b.shr(pm, 24, Rounding::HalfAwayFromZero, DType::I64);
        let pm = b.clamp(pm, -32767, 32767, DType::I16);
        let y = b.matmul(dn, pm, DType::I64);
        let y = b.reshape_fixed(y, &[2, d]);
        let y = b.shr(y, 8, Rounding::HalfAwayFromZero, DType::I64);
        let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let one = b.c(DType::I64, 1);
        let p24 = b.c(DType::I64, 1 << 24);
        let zero64 = b.c(DType::I64, 0);
        let routed = b.moe_combine_q36(y, w, one, p24, zero64, -32767, 32767, DType::I16);
        let sg = linear(&mut b, h, wsg, ef, d, 14, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let su = codes16(&mut b, h, wsu, ef, d, 22);
        let sa = b.silu(sg);
        let sp = b.mul(sa, su, DType::I64);
        let sp = b.shr(sp, 24, Rounding::HalfAwayFromZero, DType::I64);
        let sp = b.clamp(sp, -32767, 32767, DType::I16);
        let sd = codes16(&mut b, sp, wsd, d, ef, 22);
        let gl = linear(&mut b, h, wsgate, 1, d, 16, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let gs = b.int_sigmoid(gl);
        let shared = b.mul(sd, gs, DType::I64);
        let shared = b.shr(shared, 24, Rounding::HalfAwayFromZero, DType::I64);
        let shared = b.clamp(shared, -32767, 32767, DType::I16);
        let both = residual(&mut b, routed, shared);
        let x2 = residual(&mut b, x, both);
        b.finish(&[x2])
    };
    let post = {
        let mut b = m.pb.block("post", carry.clone());
        let h = norm_gain(&mut b, Ref::CarryIn(0), out_norm);
        let l = linear(&mut b, h, lm, v, d, 16, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (m.pb.blocks[post as usize].nodes.len() - 1) as u16;
    let program = std::mem::replace(&mut m.pb, ProgramBuilder::new(0, 0)).finish(pre, vec![dense, moe], post, logits);
    m.finish(program, seed)
}

/// **The admissible wide fixture** (module note): an `i64` weight times an `i32`-ranged activation into `i128`, weight on the left.
pub fn wide128_v1(seed: u64) -> TirSketchFixtureV1 {
    let (v, d) = (FX_V, FX_D);
    let mut m = Model::new(v);
    let tok = m.p("tok_embd", DType::I8, &[v, d], false, -128, 127);
    let w64 = m.p("wide.w64", DType::I64, &[d, d], true, -(1i128 << 62), 1i128 << 62);
    let lm = m.p("output.w", DType::I8, &[v, d], false, -128, 127);
    let carry = vec![TensorType::fixed(DType::I32, &[d])];
    let pre = {
        let mut b = m.pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.cast(row, DType::I32);
        b.finish(&[x])
    };
    let wide = {
        let mut b = m.pb.block("wide", carry.clone());
        let xc = b.reshape_fixed(Ref::CarryIn(0), &[d, 1]);
        // |w| ≤ 2^63, |x| ≤ 2^31, k = 16: every partial sum below 2^98 — proven, yet the error span exceeds 2^127 − 1.
        let acc = b.matmul(w64, xc, DType::I128);
        let sh = b.shr(acc, 70, Rounding::HalfAwayFromZero, DType::I128);
        let c = b.clamp(sh, -(1 << 20), 1 << 20, DType::I32);
        let c = b.reshape_fixed(c, &[d]);
        b.finish(&[c])
    };
    let post = {
        let mut b = m.pb.block("post", carry.clone());
        let h = b.clamp(Ref::CarryIn(0), -32767, 32767, DType::I16);
        let hc = b.reshape_fixed(h, &[d, 1]);
        let l = b.matmul(lm, hc, DType::I32);
        let l = b.reshape_fixed(l, &[v]);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (m.pb.blocks[post as usize].nodes.len() - 1) as u16;
    let program = std::mem::replace(&mut m.pb, ProgramBuilder::new(0, 0)).finish(pre, vec![wide], post, logits);
    m.finish(program, seed)
}

/// **The wide fixture** (module note): accumulators of two and three moduli.
pub fn wide_v1(seed: u64) -> TirSketchFixtureV1 {
    let (v, d) = (FX_V, FX_D);
    let mut m = Model::new(v);
    let tok = m.p("tok_embd", DType::I8, &[v, d], false, -128, 127);
    // Weights whose ACTUAL range is wide, so the refined plan cannot narrow the products either.
    let w32 = m.p("wide.w32", DType::I32, &[d, d], true, -(1i128 << 30), 1i128 << 30);
    let w64 = m.p("wide.w64", DType::I64, &[d, d], true, -(1i128 << 62), 1i128 << 62);
    let lm = m.p("output.w", DType::I8, &[v, d], false, -128, 127);
    let carry = vec![TensorType::fixed(DType::I32, &[d])];
    let pre = {
        let mut b = m.pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.cast(row, DType::I32);
        b.finish(&[x])
    };
    let wide = {
        let mut b = m.pb.block("wide", carry.clone());
        // A carry-in takes its dtype's full range (spec 04b §7): an i32-wide activation.
        let x = Ref::CarryIn(0);
        let xc = b.reshape_fixed(x, &[d, 1]);
        let acc64 = b.matmul(w32, xc, DType::I64);
        let d64 = b.shr(acc64, 24, Rounding::HalfAwayFromZero, DType::I64);
        let c64 = b.clamp(d64, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let x64 = b.mul(x, x, DType::I64);
        let x64c = b.reshape_fixed(x64, &[d, 1]);
        let acc128 = b.matmul(w64, x64c, DType::I128);
        let d128 = b.shr(acc128, 60, Rounding::HalfAwayFromZero, DType::I128);
        let c128 = b.clamp(d128, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let s = b.add(c64, c128, DType::I64);
        let s = b.clamp(s, -(1 << 20), 1 << 20, DType::I32);
        let s = b.reshape_fixed(s, &[d]);
        b.finish(&[s])
    };
    let post = {
        let mut b = m.pb.block("post", carry.clone());
        let h = b.clamp(Ref::CarryIn(0), -32767, 32767, DType::I16);
        let hc = b.reshape_fixed(h, &[d, 1]);
        let l = b.matmul(lm, hc, DType::I32);
        let l = b.reshape_fixed(l, &[v]);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (m.pb.blocks[post as usize].nodes.len() - 1) as u16;
    let program = std::mem::replace(&mut m.pb, ProgramBuilder::new(0, 0)).finish(pre, vec![wide], post, logits);
    m.finish(program, seed)
}
