//! The whole programs of `tests/programs.rs`, shared with the golden-vector generator.
#![allow(dead_code)]

use std::collections::BTreeMap;

use super::Lcg;
use misaka_palw_tir::arith::ONE;
use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS, INPUT_TOKEN};
use misaka_palw_tir::{DType, Dim, MapParams, Ref, Rounding, Tensor, TensorType, TirProgramV1};

// ---- parameter generation --------------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum Gen {
    Uniform(i128, i128),
    Const(i128),
}

/// A builder plus a generator per declared param, so a test can materialise weights for any seed.
pub struct Model {
    pub pb: ProgramBuilder,
    pub gens: BTreeMap<u16, Gen>,
}

impl Model {
    pub fn new(token_bound: u32) -> Self {
        Self { pb: ProgramBuilder::new(token_bound, HISTORY_BOUND_V1_SMALL), gens: BTreeMap::new() }
    }
    pub fn p(&mut self, name: &str, dtype: DType, shape: &[u32], per_layer: bool, g: Gen) -> Ref {
        let r = self.pb.param(name, dtype, shape, per_layer);
        let Ref::Param(j) = r else { unreachable!() };
        self.gens.insert(j, g);
        r
    }
    /// An A16 projection's weights: `w [out, in]` i8, per-channel multiplier and zero.
    pub fn proj(&mut self, name: &str, out: u32, inp: u32, per_layer: bool, m: (i128, i128)) -> (Ref, Ref, Ref) {
        (
            self.p(&format!("{name}.w"), DType::I8, &[out, inp], per_layer, Gen::Uniform(-128, 127)),
            self.p(&format!("{name}.m"), DType::I64, &[out], per_layer, Gen::Uniform(m.0, m.1)),
            self.p(&format!("{name}.z"), DType::I64, &[out], per_layer, Gen::Uniform(-40, 40)),
        )
    }
}

pub fn materialize(program: &TirProgramV1, gens: &BTreeMap<u16, Gen>, seed: u64) -> MapParams {
    let mut rng = Lcg(seed);
    let mut out = MapParams::default();
    for (j, d) in program.params.iter().enumerate() {
        let layers: Vec<Option<u16>> =
            if d.per_layer { (0..program.schedule.layers.len() as u16).map(Some).collect() } else { vec![None] };
        for l in layers {
            let n: usize = d.shape.iter().map(|x| *x as usize).product();
            let g = gens[&(j as u16)];
            let data: Vec<i128> = (0..n)
                .map(|_| match g {
                    Gen::Uniform(lo, hi) => rng.range(lo, hi),
                    Gen::Const(v) => v,
                })
                .collect();
            let shape = d.shape.iter().map(|x| *x as usize).collect();
            out.tensors.insert((j as u16, l), Tensor::new(d.dtype, shape, data).expect("generated in range"));
        }
    }
    out
}

// ---- composite helpers used by the programs ----------------------------------------------------

/// `narrow(W x)`: MatMul into an `i64` accumulator, then the A16 narrowing at a static shift.
#[allow(clippy::too_many_arguments)]
pub fn linear(b: &mut BlockBuilder<'_>, x: Ref, w: (Ref, Ref, Ref), shift: u32, lo: i64, hi: i64, dt: DType) -> Ref {
    let ws = b.shape(w.0);
    let (Dim::Fixed(out), Dim::Fixed(inp)) = (ws[0], ws[1]) else { unreachable!() };
    let xc = b.reshape_fixed(x, &[inp, 1]);
    let acc = b.matmul(w.0, xc, DType::I64);
    let acc = b.reshape_fixed(acc, &[out]);
    let p2 = b.c(DType::I64, 1i128 << shift);
    b.narrow_a16(acc, w.1, p2, w.2, lo, hi, dt)
}

pub fn codes16(b: &mut BlockBuilder<'_>, x: Ref, w: (Ref, Ref, Ref), shift: u32) -> Ref {
    linear(b, x, w, shift, -32767, 32767, DType::I16)
}

/// The residual add of two A16 rows, re-clamped to codes.
pub fn residual(b: &mut BlockBuilder<'_>, x: Ref, y: Ref) -> Ref {
    let s = b.add(x, y, DType::I32);
    b.clamp(s, -32767, 32767, DType::I16)
}

/// Norm (unit row, Q24) then a per-channel gain narrowing back to codes.
pub fn norm_gain(b: &mut BlockBuilder<'_>, x: Ref, gain: Ref) -> Ref {
    let u = b.rms_norm_a16(x, 1);
    let p2 = b.c(DType::I64, 1i128 << 24);
    let z = b.c(DType::I64, 0);
    b.narrow_a16(u, gain, p2, z, -32767, 32767, DType::I16)
}

pub struct AttnCfg {
    pub heads: u32,
    pub kv_heads: u32,
    pub d: u32,
    pub window: u32,
}

/// Grouped-query attention over a KV history: RoPE on q and k, both rows appended (committed),
/// scores `[kv, G, H]`, the two-pass softmax, probabilities narrowed to Q15 codes, `P·V`.
#[allow(clippy::too_many_arguments)]
pub fn gqa(
    b: &mut BlockBuilder<'_>,
    x: Ref,
    wq: (Ref, Ref, Ref),
    wk: (Ref, Ref, Ref),
    wv: (Ref, Ref, Ref),
    rope: (Ref, Ref),
    cfg: &AttnCfg,
    caches: (u16, u16),
) -> Ref {
    let (h, kv, d) = (cfg.heads, cfg.kv_heads, cfg.d);
    let g = h / kv;
    let q = codes16(b, x, wq, 22);
    let k = codes16(b, x, wk, 22);
    let v = codes16(b, x, wv, 22);
    let q = b.reshape_fixed(q, &[h, d]);
    let k = b.reshape_fixed(k, &[kv, d]);
    let v = b.reshape_fixed(v, &[kv, d]);
    let q = b.rope_pairs(q, rope.0, rope.1, -32767, 32767, DType::I16);
    let k = b.rope_pairs(k, rope.0, rope.1, -32767, 32767, DType::I16);
    let kh = b.hist_append(caches.0, k);
    let vh = b.hist_append(caches.1, v);
    // q head `h` reads kv head `h / G` (repeat_kv's grouping): [kv, G, d].
    let qg = b.reshape_fixed(q, &[kv, g, d]);
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
    let _ = cfg.window;
    b.reshape_fixed(o, &[h * d])
}

// ---- 1. a 2-layer dense GQA decoder ------------------------------------------------------------

pub const V: u32 = 24;
pub const D: u32 = 16;
pub const HQ: u32 = 4;
pub const HKV: u32 = 2;
pub const DH: u32 = 4;
pub const F: u32 = 24;

/// Build the dense decoder with a given layer schedule of windows (`history_bound` = global).
pub fn dense(windows: &[u32]) -> (TirProgramV1, BTreeMap<u16, Gen>) {
    let mut m = Model::new(V);
    let tok = m.p("tok_embd", DType::I8, &[V, D], false, Gen::Uniform(-128, 127));
    let lift = m.p("tok_embd.lift", DType::I64, &[D], false, Gen::Uniform(200, 300));
    // Two-level RoPE tables: 512 rows each for 2^18 positions; values are Q24 in [-ONE, ONE].
    let q24 = Gen::Uniform(-ONE, ONE);
    let global = (
        m.p("rope.g.cos_hi", DType::I32, &[512, DH / 2], false, q24),
        m.p("rope.g.sin_hi", DType::I32, &[512, DH / 2], false, q24),
        m.p("rope.g.cos_lo", DType::I32, &[512, DH / 2], false, q24),
        m.p("rope.g.sin_lo", DType::I32, &[512, DH / 2], false, q24),
    );
    let local = if windows.iter().any(|w| *w < HISTORY_BOUND_V1_SMALL) {
        Some((
            m.p("rope.l.cos_hi", DType::I32, &[512, DH / 2], false, q24),
            m.p("rope.l.sin_hi", DType::I32, &[512, DH / 2], false, q24),
            m.p("rope.l.cos_lo", DType::I32, &[512, DH / 2], false, q24),
            m.p("rope.l.sin_lo", DType::I32, &[512, DH / 2], false, q24),
        ))
    } else {
        None
    };
    let gain = Gen::Uniform(1 << 13, 1 << 14);
    let attn_norm = m.p("blk.attn_norm.g", DType::I64, &[D], true, gain);
    let ffn_norm = m.p("blk.ffn_norm.g", DType::I64, &[D], true, gain);
    let wq = m.proj("blk.attn_q", HQ * DH, D, true, (1 << 8, 1 << 10));
    let wk = m.proj("blk.attn_k", HKV * DH, D, true, (1 << 8, 1 << 10));
    let wv = m.proj("blk.attn_v", HKV * DH, D, true, (1 << 8, 1 << 10));
    let wo = m.proj("blk.attn_o", D, HQ * DH, true, (1 << 8, 1 << 10));
    let wg = m.proj("blk.ffn_gate", F, D, true, (1 << 10, 1 << 12));
    let wu = m.proj("blk.ffn_up", F, D, true, (1 << 8, 1 << 10));
    let wd = m.proj("blk.ffn_down", D, F, true, (1 << 8, 1 << 10));
    let wmul = m.p("blk.ffn_mul.m", DType::I64, &[F], true, Gen::Uniform(1 << 12, 1 << 13));
    let out_norm = m.p("output_norm.g", DType::I64, &[D], false, gain);
    let lm = m.proj("output", V, D, false, (1 << 8, 1 << 10));

    let carry = vec![TensorType::fixed(DType::I16, &[D])];
    // pre: the embedding row, lifted to codes.
    let pre = {
        let mut b = m.pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let p2 = b.c(DType::I64, 1);
        let z = b.c(DType::I64, 0);
        let x = b.narrow_a16(row, lift, p2, z, -32767, 32767, DType::I16);
        b.finish(&[x])
    };
    let mut kinds = Vec::new();
    let mut block_of_window: BTreeMap<u32, u8> = BTreeMap::new();
    for &w in windows {
        if let Some(bk) = block_of_window.get(&w) {
            kinds.push(*bk);
            continue;
        }
        let tables = if w < HISTORY_BOUND_V1_SMALL { local.unwrap() } else { global };
        let kc = m.pb.hist_state(&format!("k_cache.w{w}"), DType::I16, &[HKV, DH], w, true);
        let vc = m.pb.hist_state(&format!("v_cache.w{w}"), DType::I16, &[HKV, DH], w, true);
        let mut b = m.pb.block(&format!("dense.w{w}"), carry.clone());
        let x = Ref::CarryIn(0);
        let (cos, sin) = b.rope_angles_two_level(Ref::Input(INPUT_POS), tables.0, tables.1, tables.2, tables.3, 9);
        let h = norm_gain(&mut b, x, attn_norm);
        let cfg = AttnCfg { heads: HQ, kv_heads: HKV, d: DH, window: w };
        let o = gqa(&mut b, h, wq, wk, wv, (cos, sin), &cfg, (kc, vc));
        let a = codes16(&mut b, o, wo, 22);
        let x1 = residual(&mut b, x, a);
        let x1 = b.commit(x1);
        let h2 = norm_gain(&mut b, x1, ffn_norm);
        let gate = linear(&mut b, h2, wg, 14, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let up = codes16(&mut b, h2, wu, 22);
        let act = b.silu(gate);
        let prod = b.mul(act, up, DType::I64);
        let p2 = b.c(DType::I64, 1i128 << 36);
        let z = b.c(DType::I64, 0);
        let mu = b.narrow_a16(prod, wmul, p2, z, -32767, 32767, DType::I16);
        let dn = codes16(&mut b, mu, wd, 22);
        let x2 = residual(&mut b, x1, dn);
        let bk = b.finish(&[x2]);
        block_of_window.insert(w, bk);
        kinds.push(bk);
    }
    let post = {
        let mut b = m.pb.block("post", carry.clone());
        let h = norm_gain(&mut b, Ref::CarryIn(0), out_norm);
        let l = linear(&mut b, h, lm, 16, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (m.pb.blocks[post as usize].nodes.len() - 1) as u16;
    let program = m.pb.finish(pre, kinds, post, logits);
    (program, m.gens)
}

/// Re-key params from one program to another by name (and layer).
pub fn remap_by_name(from: &TirProgramV1, params: &MapParams, to: &TirProgramV1) -> MapParams {
    let mut out = MapParams::default();
    for ((j, l), t) in &params.tensors {
        let name = &from.params[*j as usize].name;
        if let Some(k) = to.param_index(name) {
            out.tensors.insert((k, *l), t.clone());
        }
    }
    out
}

// ---- 2. a GDN layer with k_heads ≠ v_heads ---------------------------------------------------

pub const GK: u32 = 2; // key heads
pub const GV: u32 = 4; // value heads
pub const GD: u32 = 4; // head dim (k and v)

/// One GDN layer in HF Qwen3-Next order (`modeling_qwen3_next.py:Qwen3NextGatedDeltaNet`): the
/// conv over `[q | k | v]` (width `2·k_heads·d + v_heads·d`), L2-normed q and k, the head mapping
/// (grouping = `repeat_interleave`, or tiling), the decay/beta gates, the delta rule, the gated
/// RMS norm and the output projection.
pub fn gdn_program(grouping: bool) -> (TirProgramV1, BTreeMap<u16, Gen>) {
    let conv_dim = 2 * GK * GD + GV * GD;
    let mut m = Model::new(V);
    let tok = m.p("tok_embd", DType::I8, &[V, D], false, Gen::Uniform(-128, 127));
    let lift = m.p("tok_embd.lift", DType::I64, &[D], false, Gen::Uniform(200, 300));
    let norm = m.p("blk.norm.g", DType::I64, &[D], true, Gen::Uniform(1 << 13, 1 << 14));
    let wqkv = m.proj("blk.in_qkv", conv_dim, D, true, (1 << 8, 1 << 10));
    let wz = m.proj("blk.in_z", GV * GD, D, true, (1 << 10, 1 << 12));
    let wb = m.proj("blk.in_b", GV, D, true, (1 << 10, 1 << 12));
    let wa = m.proj("blk.in_a", GV, D, true, (1 << 10, 1 << 12));
    let taps = m.p("blk.conv.taps", DType::I8, &[conv_dim, 4], true, Gen::Uniform(-128, 127));
    let conv_m = m.p("blk.conv.m", DType::I64, &[conv_dim], true, Gen::Uniform(1 << 12, 1 << 14));
    let conv_z = m.p("blk.conv.z", DType::I64, &[conv_dim], true, Gen::Const(0));
    let dt_bias = m.p("blk.dt_bias", DType::I32, &[GV], true, Gen::Uniform(-(2 << 24), 2 << 24));
    let c = m.p("blk.decay_c", DType::I64, &[GV], true, Gen::Uniform(1 << 22, 1 << 25));
    let trip = |m: &mut Model, n: &str, mm: (i128, i128), s: i128| {
        (
            m.p(&format!("blk.{n}.m"), DType::I64, &[GV], true, Gen::Uniform(mm.0, mm.1)),
            m.p(&format!("blk.{n}.s"), DType::I8, &[GV], true, Gen::Const(s)),
            m.p(&format!("blk.{n}.z"), DType::I64, &[GV], true, Gen::Const(0)),
        )
    };
    let read = trip(&mut m, "read", (1, 2), 15);
    let delta = trip(&mut m, "delta", (1, 2), 0);
    let out = trip(&mut m, "out", (1, 2), 15);
    let ws = m.p("blk.write_shift", DType::I32, &[GV], true, Gen::Const(-15));
    let onorm_eps = m.p("blk.onorm.eps", DType::I64, &[GV, 1], true, Gen::Const(1));
    let onorm_es = m.p("blk.onorm.es", DType::I8, &[GV, 1], true, Gen::Const(0));
    let onorm_g = m.p("blk.onorm.g", DType::I64, &[GD], true, Gen::Uniform(1 << 13, 1 << 14));
    let wo = m.proj("blk.out_proj", D, GV * GD, true, (1 << 8, 1 << 10));
    let out_norm = m.p("output_norm.g", DType::I64, &[D], false, Gen::Uniform(1 << 13, 1 << 14));
    let lm = m.proj("output", V, D, false, (1 << 8, 1 << 10));
    let conv_state = m.pb.fixed_state("conv", DType::I16, &[3, conv_dim], -32767, 32767, true);
    let s_state = m.pb.fixed_state("S", DType::I32, &[GV, GD, GD], -(i32::MAX as i64), i32::MAX as i64, true);

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
        let mut b = m.pb.block("gdn", carry.clone());
        let x = Ref::CarryIn(0);
        let h = norm_gain(&mut b, x, norm);
        let qkv = codes16(&mut b, h, wqkv, 22);
        let qkv = b.commit(qkv);
        // The causal conv window: 3 prior rows (state) and this one; the new state drops the oldest.
        let row = b.reshape_fixed(qkv, &[1, conv_dim]);
        let window = b.concat(&[Ref::State(conv_state), row], 0);
        let keep = b.slice(window, 0, 1, 3);
        b.state_write(conv_state, keep);
        let tt = b.transpose(taps, &[1, 0]);
        let prod = b.mul(window, tt, DType::I32);
        let acc = b.reduce_sum(prod, 0, DType::I64);
        let acc = b.reshape_fixed(acc, &[conv_dim]);
        let p2 = b.c(DType::I64, 1 << 8);
        let conv = b.narrow_a16(acc, conv_m, p2, conv_z, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let act = b.silu(conv);
        let s8 = b.c(DType::I64, 1 << 10);
        let one = b.c(DType::I64, 1);
        let zero64 = b.c(DType::I64, 0);
        let act = b.narrow_a16(act, one, s8, zero64, -32767, 32767, DType::I16);
        let q = b.slice(act, 0, 0, GK * GD);
        let k = b.slice(act, 0, GK * GD, GK * GD);
        let v = b.slice(act, 0, 2 * GK * GD, GV * GD);
        let q = b.reshape_fixed(q, &[GK, GD]);
        let k = b.reshape_fixed(k, &[GK, GD]);
        let v = b.reshape_fixed(v, &[GV, GD]);
        let q = b.l2_norm_q15(q);
        let k = b.l2_norm_q15(k);
        // The head mapping, as data: grouping (HF repeat_interleave) or tiling (the live kernel).
        let r = GV / GK;
        let map = |b: &mut BlockBuilder<'_>, t: Ref| {
            if grouping {
                let t = b.reshape_fixed(t, &[GK, 1, GD]);
                let t = b.broadcast(t, &[Dim::Fixed(GK), Dim::Fixed(r), Dim::Fixed(GD)]);
                b.reshape_fixed(t, &[GV, GD])
            } else {
                let t = b.reshape_fixed(t, &[1, GK, GD]);
                let t = b.broadcast(t, &[Dim::Fixed(r), Dim::Fixed(GK), Dim::Fixed(GD)]);
                b.reshape_fixed(t, &[GV, GD])
            }
        };
        let qv = map(&mut b, q);
        let kv = map(&mut b, k);
        // Gates, per value head, Q24: beta = sigmoid(b), decay = exp(-c · softplus(a + dt_bias)).
        let braw = linear(&mut b, h, wb, 18, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let beta = b.int_sigmoid(braw);
        let araw = linear(&mut b, h, wa, 18, -(1 << 30), 1 << 30, DType::I32);
        let dt = b.add(araw, dt_bias, DType::I32);
        let decay = b.decay_q36(dt, c);
        let (rsp, dsp, osp) = (b.pow2_of(read.1), b.pow2_of(delta.1), b.pow2_of(out.1));
        let o =
            b.gdn_step_q36(s_state, kv, v, qv, decay, beta, (read.0, rsp, read.2), (delta.0, dsp, delta.2), ws, (out.0, osp, out.2));
        // The gated RMS norm per value head, then the gate silu(z).
        let o = b.rms_norm_wide_q36(o, onorm_eps, onorm_es);
        let p24 = b.c(DType::I64, 1 << 24);
        let o = b.narrow_a16(o, onorm_g, p24, zero64, -32767, 32767, DType::I16);
        let zq = linear(&mut b, h, wz, 14, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let zg = b.silu(zq);
        let zg = b.reshape_fixed(zg, &[GV, GD]);
        let gated = b.mul(o, zg, DType::I64);
        let gated = b.narrow_a16(gated, one, p24, zero64, -32767, 32767, DType::I16);
        let gated = b.reshape_fixed(gated, &[GV * GD]);
        let y = codes16(&mut b, gated, wo, 22);
        let x2 = residual(&mut b, x, y);
        b.finish(&[x2])
    };
    let post = {
        let mut b = m.pb.block("post", carry.clone());
        let h = norm_gain(&mut b, Ref::CarryIn(0), out_norm);
        let l = linear(&mut b, h, lm, 16, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (m.pb.blocks[post as usize].nodes.len() - 1) as u16;
    (m.pb.finish(pre, vec![layer, layer], post, logits), m.gens)
}

// ---- 3. a Mamba2 layer -------------------------------------------------------------------------

pub const MH: u32 = 4; // heads
pub const MP: u32 = 2; // head_dim
pub const MG: u32 = 2; // groups
pub const MN: u32 = 4; // d_state

/// One Mamba2 mixer (`modeling_mamba2.py:Mamba2Mixer`, the single-token path
/// `mamba2_selective_state_update`): in-proj to `[z | xBC | dt]`, the causal conv over `xBC`, SiLU,
/// `dt = softplus(dt + dt_bias)`, `dA = exp(dt·A)`, B and C grouped onto heads
/// (`repeat_interleave`), `h ← dA·h + dt·(x ⊗ B)`, `y = h·C + D·x`, the gated RMS norm
/// `norm(y · silu(z))` (gate BEFORE the norm) and the out-proj.
pub fn mamba2_program() -> (TirProgramV1, BTreeMap<u16, Gen>) {
    let di = MH * MP;
    let conv_dim = di + 2 * MG * MN;
    let mut m = Model::new(V);
    let tok = m.p("tok_embd", DType::I8, &[V, D], false, Gen::Uniform(-128, 127));
    let lift = m.p("tok_embd.lift", DType::I64, &[D], false, Gen::Uniform(200, 300));
    let norm = m.p("blk.norm.g", DType::I64, &[D], true, Gen::Uniform(1 << 13, 1 << 14));
    let wz = m.proj("blk.in_z", di, D, true, (1 << 10, 1 << 12));
    let wx = m.proj("blk.in_xbc", conv_dim, D, true, (1 << 8, 1 << 10));
    let wdt = m.proj("blk.in_dt", MH, D, true, (1 << 10, 1 << 12));
    let taps = m.p("blk.conv.taps", DType::I8, &[conv_dim, 4], true, Gen::Uniform(-128, 127));
    let conv_m = m.p("blk.conv.m", DType::I64, &[conv_dim], true, Gen::Uniform(1 << 12, 1 << 14));
    let conv_z = m.p("blk.conv.z", DType::I64, &[conv_dim], true, Gen::Const(0));
    let dt_bias = m.p("blk.dt_bias", DType::I32, &[MH], true, Gen::Uniform(-(1 << 24), 1 << 24));
    // A = -exp(A_log), per head, Q24 and negative (registration-time data).
    let a = m.p("blk.A", DType::I32, &[MH, 1], true, Gen::Uniform(-(8 << 24), -(1 << 22)));
    let dskip = m.p("blk.D", DType::I32, &[MH, 1], true, Gen::Uniform(0, 1 << 24));
    let norm_g = m.p("blk.gnorm.g", DType::I64, &[di], true, Gen::Uniform(1 << 13, 1 << 14));
    let wo = m.proj("blk.out", D, di, true, (1 << 8, 1 << 10));
    let out_norm = m.p("output_norm.g", DType::I64, &[D], false, Gen::Uniform(1 << 13, 1 << 14));
    let lm = m.proj("output", V, D, false, (1 << 8, 1 << 10));
    let conv_state = m.pb.fixed_state("conv", DType::I16, &[3, conv_dim], -32767, 32767, true);
    let h_state = m.pb.fixed_state("h", DType::I32, &[MH, MP, MN], -(1 << 30), 1 << 30, true);

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
        let mut b = m.pb.block("mamba2", carry.clone());
        let x = Ref::CarryIn(0);
        let h = norm_gain(&mut b, x, norm);
        let zero64 = b.c(DType::I64, 0);
        let one = b.c(DType::I64, 1);
        let xbc = codes16(&mut b, h, wx, 22);
        let xbc = b.commit(xbc);
        let row = b.reshape_fixed(xbc, &[1, conv_dim]);
        let window = b.concat(&[Ref::State(conv_state), row], 0);
        let keep = b.slice(window, 0, 1, 3);
        b.state_write(conv_state, keep);
        let tt = b.transpose(taps, &[1, 0]);
        let prod = b.mul(window, tt, DType::I32);
        let acc = b.reduce_sum(prod, 0, DType::I64);
        let acc = b.reshape_fixed(acc, &[conv_dim]);
        let p20 = b.c(DType::I64, 1 << 8);
        let conv = b.narrow_a16(acc, conv_m, p20, conv_z, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let act = b.silu(conv);
        let p9 = b.c(DType::I64, 1 << 10);
        let act = b.narrow_a16(act, one, p9, zero64, -32767, 32767, DType::I16);
        let xs = b.slice(act, 0, 0, di);
        let bs = b.slice(act, 0, di, MG * MN);
        let cs = b.slice(act, 0, di + MG * MN, MG * MN);
        let xs = b.reshape_fixed(xs, &[MH, MP, 1]);
        // B, C grouped onto heads: head hh reads group hh / (MH/MG) (repeat_interleave).
        let r = MH / MG;
        let group = |b: &mut BlockBuilder<'_>, t: Ref| {
            let t = b.reshape_fixed(t, &[MG, 1, MN]);
            let t = b.broadcast(t, &[Dim::Fixed(MG), Dim::Fixed(r), Dim::Fixed(MN)]);
            b.reshape_fixed(t, &[MH, 1, MN])
        };
        let bh = group(&mut b, bs);
        let ch = group(&mut b, cs);
        // dt = softplus(dt + bias) (Q24, per head); dA = exp((dt·A) >> 24).
        let dtraw = linear(&mut b, h, wdt, 18, -(1 << 30), 1 << 30, DType::I32);
        let dtb = b.add(dtraw, dt_bias, DType::I32);
        let dt = b.softplus_q36(dtb);
        let dt = b.clamp(dt, 0, 1 << 30, DType::I32);
        let dt = b.reshape_fixed(dt, &[MH, 1]);
        let dta = b.mul(dt, a, DType::I64);
        let dta = b.shr(dta, 24, Rounding::Floor, DType::I64);
        let dta = b.clamp(dta, i32::MIN as i64, 0, DType::I32);
        let da = b.int_exp(dta);
        let da = b.reshape_fixed(da, &[MH, 1, 1]);
        // h ← dA·h + dt·(x ⊗ B): the decay rounds half away from zero; dt·x·B is Q24·code·code.
        let hd = b.mul(Ref::State(h_state), da, DType::I64);
        let hd = b.shr(hd, 24, Rounding::HalfAwayFromZero, DType::I64);
        let xb = b.mul(xs, bh, DType::I64);
        let dtr = b.reshape_fixed(dt, &[MH, 1, 1]);
        let dxb = b.mul(xb, dtr, DType::I128);
        let dxb = b.shr(dxb, 30, Rounding::HalfAwayFromZero, DType::I64);
        let hn = b.add(hd, dxb, DType::I64);
        let hn = b.state_write(h_state, hn);
        // y = h·C + D·x.
        let ct = b.transpose(ch, &[0, 2, 1]);
        let y = b.matmul(hn, ct, DType::I64);
        let y = b.reshape_fixed(y, &[MH, MP]);
        let xs2 = b.reshape_fixed(xs, &[MH, MP]);
        let dx = b.mul(xs2, dskip, DType::I64);
        let dx = b.shr(dx, 14, Rounding::Floor, DType::I64);
        let y = b.add(y, dx, DType::I64);
        let y = b.reshape_fixed(y, &[di]);
        let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        // Gated RMS norm: silu(z) BEFORE the norm (MambaRMSNormGated with a gate).
        let zq = linear(&mut b, h, wz, 14, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let zg = b.silu(zq);
        let g = b.mul(y, zg, DType::I64);
        let g = b.shr(g, 24, Rounding::Floor, DType::I64);
        let g = b.clamp(g, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let eps0 = b.c(DType::I64, 1);
        let es0 = b.c(DType::I8, 0);
        let n = b.rms_norm_wide_q36(g, eps0, es0);
        let p24 = b.c(DType::I64, 1 << 24);
        let n = b.narrow_a16(n, norm_g, p24, zero64, -32767, 32767, DType::I16);
        let o = codes16(&mut b, n, wo, 22);
        let x2 = residual(&mut b, x, o);
        b.finish(&[x2])
    };
    let post = {
        let mut b = m.pb.block("post", carry.clone());
        let h = norm_gain(&mut b, Ref::CarryIn(0), out_norm);
        let l = linear(&mut b, h, lm, 16, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (m.pb.blocks[post as usize].nodes.len() - 1) as u16;
    (m.pb.finish(pre, vec![layer], post, logits), m.gens)
}

// ---- 4. a top-2 MoE layer with a shared expert -----------------------------------------------

pub const E: u32 = 4;
pub const EF: u32 = 8;

/// One MoE layer: a router (softmax, TopK committed, renormalised), the chosen experts' weights
/// GATHERED by the committed indices (`routed_expert_matmul` structurally), per-expert SwiGLU, one
/// exact combine accumulator, and a Qwen2-MoE shared expert gated by `sigmoid(w·x)`
/// (`modeling_qwen2_moe.py:Qwen2MoeSparseMoeBlock`).
pub fn moe_program() -> (TirProgramV1, BTreeMap<u16, Gen>) {
    let mut m = Model::new(V);
    let tok = m.p("tok_embd", DType::I8, &[V, D], false, Gen::Uniform(-128, 127));
    let lift = m.p("tok_embd.lift", DType::I64, &[D], false, Gen::Uniform(200, 300));
    let norm = m.p("blk.norm.g", DType::I64, &[D], true, Gen::Uniform(1 << 13, 1 << 14));
    let wr = m.proj("blk.router", E, D, true, (1 << 8, 1 << 10));
    let gate_exps = m.p("blk.gate_exps.w", DType::I8, &[E, EF, D], true, Gen::Uniform(-128, 127));
    let up_exps = m.p("blk.up_exps.w", DType::I8, &[E, EF, D], true, Gen::Uniform(-128, 127));
    let down_exps = m.p("blk.down_exps.w", DType::I8, &[E, D, EF], true, Gen::Uniform(-128, 127));
    let wsg = m.proj("blk.shared_gate", EF, D, true, (1 << 10, 1 << 12));
    let wsu = m.proj("blk.shared_up", EF, D, true, (1 << 8, 1 << 10));
    let wsd = m.proj("blk.shared_down", D, EF, true, (1 << 8, 1 << 10));
    let wsgate = m.proj("blk.shared_expert_gate", 1, D, true, (1 << 10, 1 << 12));
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
        let (idx, w) = b.router_topk_q36(logits, 2, 4);
        // The chosen experts' matrices: a Gather over the per-layer expert params by the
        // committed selection.
        let g = b.gather(gate_exps, idx, 0, 0);
        let u = b.gather(up_exps, idx, 0, 0);
        let dn = b.gather(down_exps, idx, 0, 0);
        let hc = b.reshape_fixed(h, &[D, 1]);
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
        let y = b.reshape_fixed(y, &[2, D]);
        let y = b.shr(y, 8, Rounding::HalfAwayFromZero, DType::I64);
        let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let one = b.c(DType::I64, 1);
        let p24 = b.c(DType::I64, 1 << 24);
        let zero64 = b.c(DType::I64, 0);
        let routed = b.moe_combine_q36(y, w, one, p24, zero64, -32767, 32767, DType::I16);
        // The shared expert, gated by sigmoid of a scalar projection.
        let sg = linear(&mut b, h, wsg, 14, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let su = codes16(&mut b, h, wsu, 22);
        let sa = b.silu(sg);
        let sp = b.mul(sa, su, DType::I64);
        let sp = b.shr(sp, 24, Rounding::HalfAwayFromZero, DType::I64);
        let sp = b.clamp(sp, -32767, 32767, DType::I16);
        let sd = codes16(&mut b, sp, wsd, 22);
        let gl = linear(&mut b, h, wsgate, 16, i32::MIN as i64, i32::MAX as i64, DType::I32);
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
        let l = linear(&mut b, h, lm, 16, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (m.pb.blocks[post as usize].nodes.len() - 1) as u16;
    (m.pb.finish(pre, vec![layer, layer], post, logits), m.gens)
}
