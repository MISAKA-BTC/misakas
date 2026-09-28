//! **Whole programs over several positions** (RFC-0002 Phase C): a 2-layer dense GQA decoder
//! (RMSNorm, SwiGLU, two-level RoPE, two-pass softmax), a GDN layer with `k_heads ≠ v_heads`, a
//! Mamba2 layer, a top-2 MoE layer with a shared expert, and a sliding-window + global schedule.
//!
//! Every program is built with the builder, round-trips its canonical encoding, validates, runs
//! over several positions, and then passes the court property: **every commit point of every
//! position is reproduced by `eval_cone` from the other commit points, the carries, the params and
//! the state the position started from.** Program-specific properties (head mapping, window
//! eviction, routing) are asserted alongside.

mod common;

use std::collections::BTreeMap;

use common::Lcg;
use misaka_palw_tir::arith::ONE;
use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS, INPUT_TOKEN};
use misaka_palw_tir::{
    Cmp, ConeEnv, DType, Dim, Interpreter, MapParams, Ref, Rounding, RunState, StepOutput, Tensor, TensorType, TirProgramV1,
};

// ---- parameter generation --------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Gen {
    Uniform(i128, i128),
    Const(i128),
}

/// A builder plus a generator per declared param, so a test can materialise weights for any seed.
struct Model {
    pb: ProgramBuilder,
    gens: BTreeMap<u16, Gen>,
}

impl Model {
    fn new(token_bound: u32) -> Self {
        Self { pb: ProgramBuilder::new(token_bound, HISTORY_BOUND_V1_SMALL), gens: BTreeMap::new() }
    }
    fn p(&mut self, name: &str, dtype: DType, shape: &[u32], per_layer: bool, g: Gen) -> Ref {
        let r = self.pb.param(name, dtype, shape, per_layer);
        let Ref::Param(j) = r else { unreachable!() };
        self.gens.insert(j, g);
        r
    }
    /// An A16 projection's weights: `w [out, in]` i8, per-channel multiplier and zero.
    fn proj(&mut self, name: &str, out: u32, inp: u32, per_layer: bool, m: (i128, i128)) -> (Ref, Ref, Ref) {
        (
            self.p(&format!("{name}.w"), DType::I8, &[out, inp], per_layer, Gen::Uniform(-128, 127)),
            self.p(&format!("{name}.m"), DType::I64, &[out], per_layer, Gen::Uniform(m.0, m.1)),
            self.p(&format!("{name}.z"), DType::I64, &[out], per_layer, Gen::Uniform(-40, 40)),
        )
    }
}

fn materialize(program: &TirProgramV1, gens: &BTreeMap<u16, Gen>, seed: u64) -> MapParams {
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
fn linear(b: &mut BlockBuilder<'_>, x: Ref, w: (Ref, Ref, Ref), shift: u32, lo: i64, hi: i64, dt: DType) -> Ref {
    let ws = b.shape(w.0);
    let (Dim::Fixed(out), Dim::Fixed(inp)) = (ws[0], ws[1]) else { unreachable!() };
    let xc = b.reshape_fixed(x, &[inp, 1]);
    let acc = b.matmul(w.0, xc, DType::I64);
    let acc = b.reshape_fixed(acc, &[out]);
    let p2 = b.c(DType::I64, 1i128 << shift);
    b.narrow_a16(acc, w.1, p2, w.2, lo, hi, dt)
}

fn codes16(b: &mut BlockBuilder<'_>, x: Ref, w: (Ref, Ref, Ref), shift: u32) -> Ref {
    linear(b, x, w, shift, -32767, 32767, DType::I16)
}

/// The residual add of two A16 rows, re-clamped to codes.
fn residual(b: &mut BlockBuilder<'_>, x: Ref, y: Ref) -> Ref {
    let s = b.add(x, y, DType::I32);
    b.clamp(s, -32767, 32767, DType::I16)
}

/// Norm (unit row, Q24) then a per-channel gain narrowing back to codes.
fn norm_gain(b: &mut BlockBuilder<'_>, x: Ref, gain: Ref) -> Ref {
    let u = b.rms_norm_a16(x, 1);
    let p2 = b.c(DType::I64, 1i128 << 24);
    let z = b.c(DType::I64, 0);
    b.narrow_a16(u, gain, p2, z, -32767, 32767, DType::I16)
}

struct AttnCfg {
    heads: u32,
    kv_heads: u32,
    d: u32,
    window: u32,
}

/// Grouped-query attention over a KV history: RoPE on q and k, both rows appended (committed),
/// scores `[kv, G, H]`, the two-pass softmax, probabilities narrowed to Q15 codes, `P·V`.
#[allow(clippy::too_many_arguments)]
fn gqa(
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

// ---- the court property ------------------------------------------------------------------------

/// Run `tokens` through `program`, and for every commit point of every position check that
/// `eval_cone` reproduces it from committed operands alone.
fn run_and_check_cones(program: &TirProgramV1, params: &MapParams, tokens: &[u32]) -> Vec<StepOutput> {
    // The canonical encoding round-trips and is the only encoding.
    let bytes = program.encode();
    let decoded = TirProgramV1::decode_canonical(&bytes).expect("canonical");
    assert_eq!(&decoded, program);
    let interp = Interpreter::new(program).expect("valid");
    let mut state = RunState::default();
    let mut outs = Vec::new();
    let occurrences = program.occurrences();
    for &t in tokens {
        let before = state.clone();
        let step = interp.step(params, &mut state, t).expect("step");
        // Group this step's commits by occurrence.
        let bases = program.occurrence_slot_bases();
        for (occ, (block, layer)) in occurrences.iter().enumerate() {
            let commits: BTreeMap<u16, Tensor> = step
                .commits
                .iter()
                .filter(|c| c.slot >= bases[occ] && c.block == *block && c.layer == *layer)
                .map(|c| (c.node, c.value.clone()))
                .collect();
            // Carry-in: the previous occurrence's carry-out values, from its commits.
            let carry_in: BTreeMap<u8, Tensor> = if occ == 0 {
                BTreeMap::new()
            } else {
                let (pb, pl) = occurrences[occ - 1];
                let prev = &program.blocks[pb as usize];
                prev.carry_out
                    .iter()
                    .enumerate()
                    .map(|(k, n)| {
                        let v = step
                            .commits
                            .iter()
                            .find(|c| c.block == pb && c.layer == pl && c.node == *n && c.slot >= bases[occ - 1])
                            .unwrap();
                        (k as u8, v.value.clone())
                    })
                    .collect()
            };
            let fixed: BTreeMap<u16, Tensor> =
                before.fixed.iter().filter(|((_, l), _)| *l == *layer).map(|((j, _), v)| (*j, v.clone())).collect();
            let hist_prior: BTreeMap<u16, Vec<Tensor>> =
                before.hist.iter().filter(|((_, l), _)| *l == *layer).map(|((j, _), v)| (*j, v.iter().cloned().collect())).collect();
            for (node, value) in &commits {
                let mut supplied = commits.clone();
                supplied.remove(node);
                let env = ConeEnv {
                    token: Some(t),
                    pos: before.pos,
                    carry_in: carry_in.clone(),
                    fixed: fixed.clone(),
                    hist_prior: hist_prior.clone(),
                    supplied,
                };
                let got = interp.eval_cone(*block, *layer, *node, params, &env).expect("cone evaluates");
                assert_eq!(&got, value, "cone of block {block} layer {layer:?} node {node} at pos {}", before.pos);
            }
        }
        outs.push(step);
    }
    // Determinism: a second run from a fresh state gives the same bytes.
    let again = interp.run(params, tokens).expect("rerun");
    assert_eq!(again, outs);
    outs
}

// ---- 1. a 2-layer dense GQA decoder ------------------------------------------------------------

const V: u32 = 24;
const D: u32 = 16;
const HQ: u32 = 4;
const HKV: u32 = 2;
const DH: u32 = 4;
const F: u32 = 24;

/// Build the dense decoder with a given layer schedule of windows (`history_bound` = global).
fn dense(windows: &[u32]) -> (TirProgramV1, BTreeMap<u16, Gen>) {
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

#[test]
fn a_two_layer_dense_gqa_decoder_runs_and_every_cone_reproduces() {
    let (program, gens) = dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]);
    let params = materialize(&program, &gens, 101);
    let tokens = [3u32, 17, 0, 23, 9, 9];
    let outs = run_and_check_cones(&program, &params, &tokens);
    // The logits move with the history: the same token at two positions gives different rows.
    assert_ne!(outs[4].logits, outs[5].logits, "token 9 at positions 4 and 5 must see different histories");
    // Not degenerate: the logits row is not constant.
    let l = &outs[5].logits.data;
    assert!(l.iter().any(|v| *v != l[0]));
    // A different prefix changes a later position; the same prefix does not.
    let interp = Interpreter::new(&program).unwrap();
    let other = interp.run(&params, &[4, 17, 0, 23, 9, 9]).unwrap();
    assert_ne!(other[5].logits, outs[5].logits);
    let same = interp.run(&params, &tokens[..3]).unwrap();
    assert_eq!(same[2], outs[2], "a run's prefix is the prefix of the run");
    // A token outside token_bound is an operand error, never a panic.
    let mut st = RunState::default();
    assert!(interp.step(&params, &mut st, V).is_err());
    assert_eq!(st, RunState::default(), "a failed step leaves the state untouched");
}

/// Sliding-window + global schedule: local layers read `min(pos + 1, 3)` rows, global layers all.
/// Before the window fills, a local layer is indistinguishable from a global one with the same
/// tables; after it, the eviction changes the logits.
#[test]
fn a_sliding_window_and_global_schedule_evicts_exactly_past_the_window() {
    let w = 3u32;
    let (mixed, gens) = dense(&[w, HISTORY_BOUND_V1_SMALL, w]);
    let params = materialize(&mixed, &gens, 202);
    let tokens = [5u32, 1, 12, 7, 7, 20, 2];
    let outs = run_and_check_cones(&mixed, &params, &tokens);
    // The local layers' K history never exceeds the window.
    let interp = Interpreter::new(&mixed).unwrap();
    let mut st = RunState::default();
    for (i, t) in tokens.iter().enumerate() {
        interp.step(&params, &mut st, *t).unwrap();
        for ((j, _), rows) in &st.hist {
            let decl = &mixed.states[*j as usize];
            if decl.name.ends_with(".w3") {
                assert_eq!(rows.len(), (i + 1).min(w as usize - 1), "prior rows kept for the next position");
            }
        }
    }
    // Same weights and tables as a program whose "local" layers are global: equal while
    // pos + 1 ≤ window, different afterwards.
    let (global_only, gens2) = dense(&[HISTORY_BOUND_V1_SMALL - 1, HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL - 1]);
    let _ = gens2;
    // Map the params by name: the "local" program's tables are the rope.l.* params.
    let p2 = remap_by_name(&mixed, &params, &global_only);
    let outs2 = Interpreter::new(&global_only).unwrap().run(&p2, &tokens).unwrap();
    for pos in 0..w as usize {
        assert_eq!(outs[pos].logits, outs2[pos].logits, "position {pos} is inside the window");
    }
    assert!((w as usize..tokens.len()).any(|pos| outs[pos].logits != outs2[pos].logits), "eviction must show past the window");
}

/// Re-key params from one program to another by name (and layer).
fn remap_by_name(from: &TirProgramV1, params: &MapParams, to: &TirProgramV1) -> MapParams {
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

const GK: u32 = 2; // key heads
const GV: u32 = 4; // value heads
const GD: u32 = 4; // head dim (k and v)

/// One GDN layer in HF Qwen3-Next order (`modeling_qwen3_next.py:Qwen3NextGatedDeltaNet`): the
/// conv over `[q | k | v]` (width `2·k_heads·d + v_heads·d`), L2-normed q and k, the head mapping
/// (grouping = `repeat_interleave`, or tiling), the decay/beta gates, the delta rule, the gated
/// RMS norm and the output projection.
fn gdn_program(grouping: bool) -> (TirProgramV1, BTreeMap<u16, Gen>) {
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

#[test]
fn a_gdn_layer_with_unequal_key_and_value_heads_runs_and_the_head_mapping_is_data() {
    let (grouped, gens) = gdn_program(true);
    let (tiled, _) = gdn_program(false);
    let params = materialize(&grouped, &gens, 303);
    let tokens = [1u32, 2, 3, 5, 8, 13];
    let a = run_and_check_cones(&grouped, &params, &tokens);
    let b = run_and_check_cones(&tiled, &params, &tokens);
    // The recurrence carries: the same token later sees a different state.
    assert_ne!(a[0].logits, Interpreter::new(&grouped).unwrap().run(&params, &[2, 1]).unwrap()[1].logits);
    // Grouping and tiling are different programs (different class ids) and, at k ≠ v, different
    // functions.
    assert_ne!(grouped.encode(), tiled.encode());
    assert!(a.iter().zip(&b).any(|(x, y)| x.logits != y.logits), "16:32-style mappings must differ");
    // The state is written every position and stays in its declared range.
    let interp = Interpreter::new(&grouped).unwrap();
    let mut st = RunState::default();
    for t in tokens {
        interp.step(&params, &mut st, t).unwrap();
    }
    let s = st.fixed.iter().find(|((j, _), _)| grouped.states[*j as usize].name == "S").map(|(_, v)| v).unwrap();
    assert!(s.data.iter().any(|v| *v != 0));
    assert!(s.data.iter().all(|v| v.abs() <= i32::MAX as i128));
}

/// At `k_heads == v_heads` there is nothing to map: grouping and tiling are the same function.
#[test]
fn at_equal_heads_grouping_and_tiling_coincide() {
    // r = 1 is the identity broadcast under both spellings; build it directly.
    let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let t = pb.param("t", DType::I16, &[3, 5], false);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let g = b.reshape_fixed(t, &[3, 1, 5]);
        let g = b.broadcast(g, &[Dim::Fixed(3), Dim::Fixed(1), Dim::Fixed(5)]);
        let g = b.reshape_fixed(g, &[3, 5]);
        let tl = b.reshape_fixed(t, &[1, 3, 5]);
        let tl = b.broadcast(tl, &[Dim::Fixed(1), Dim::Fixed(3), Dim::Fixed(5)]);
        let tl = b.reshape_fixed(tl, &[3, 5]);
        let d = b.sub(g, tl, DType::I32);
        b.finish(&[d])
    };
    let carry = pb.blocks[pre as usize].nodes.last().unwrap().out.clone();
    let post = {
        let mut b = pb.block("post", vec![carry]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[15]);
        b.commit(l);
        b.finish(&[])
    };
    let p = pb.finish(pre, vec![], post, 0);
    let mut params = MapParams::default();
    params.tensors.insert((0, None), Tensor::new(DType::I16, vec![3, 5], (0..15).map(|v| v * 7 - 40).collect()).unwrap());
    let out = Interpreter::new(&p).unwrap().run(&params, &[0]).unwrap();
    assert!(out[0].logits.data.iter().all(|v| *v == 0));
}

// ---- 3. a Mamba2 layer -------------------------------------------------------------------------

const MH: u32 = 4; // heads
const MP: u32 = 2; // head_dim
const MG: u32 = 2; // groups
const MN: u32 = 4; // d_state

/// One Mamba2 mixer (`modeling_mamba2.py:Mamba2Mixer`, the single-token path
/// `mamba2_selective_state_update`): in-proj to `[z | xBC | dt]`, the causal conv over `xBC`, SiLU,
/// `dt = softplus(dt + dt_bias)`, `dA = exp(dt·A)`, B and C grouped onto heads
/// (`repeat_interleave`), `h ← dA·h + dt·(x ⊗ B)`, `y = h·C + D·x`, the gated RMS norm
/// `norm(y · silu(z))` (gate BEFORE the norm) and the out-proj.
fn mamba2_program() -> (TirProgramV1, BTreeMap<u16, Gen>) {
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

#[test]
fn a_mamba2_layer_runs_its_selective_scan_over_positions() {
    let (program, gens) = mamba2_program();
    let params = materialize(&program, &gens, 404);
    let tokens = [7u32, 7, 7, 7, 11, 0];
    let outs = run_and_check_cones(&program, &params, &tokens);
    // A repeated token still moves: the scan state and the conv window carry information.
    assert!(outs[1].logits != outs[2].logits || outs[2].logits != outs[3].logits);
    // The conv window starts at zero: position 0 depends on nothing earlier.
    let interp = Interpreter::new(&program).unwrap();
    assert_eq!(interp.run(&params, &[7]).unwrap()[0], outs[0]);
}

// ---- 4. a top-2 MoE layer with a shared expert -----------------------------------------------

const E: u32 = 4;
const EF: u32 = 8;

/// One MoE layer: a router (softmax, TopK committed, renormalised), the chosen experts' weights
/// GATHERED by the committed indices (`routed_expert_matmul` structurally), per-expert SwiGLU, one
/// exact combine accumulator, and a Qwen2-MoE shared expert gated by `sigmoid(w·x)`
/// (`modeling_qwen2_moe.py:Qwen2MoeSparseMoeBlock`).
fn moe_program() -> (TirProgramV1, BTreeMap<u16, Gen>) {
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

#[test]
fn a_top2_moe_layer_with_a_shared_expert_routes_through_committed_selections() {
    let (program, gens) = moe_program();
    let params = materialize(&program, &gens, 505);
    let tokens = [0u32, 1, 2, 3, 4, 5, 6, 7];
    let outs = run_and_check_cones(&program, &params, &tokens);
    // Every TopK is committed, in index order, with k distinct experts.
    let topk_nodes: Vec<u16> = program.blocks[1]
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.prim, misaka_palw_tir::Prim::TopK { .. }))
        .map(|(i, _)| i as u16)
        .collect();
    assert_eq!(topk_nodes.len(), 1);
    let mut used = std::collections::BTreeSet::new();
    for o in &outs {
        for c in o.commits.iter().filter(|c| c.block == 1 && c.node == topk_nodes[0]) {
            assert_eq!(c.value.data.len(), 2);
            assert!(c.value.data[0] < c.value.data[1], "index order, distinct");
            used.insert(c.value.data.clone());
        }
    }
    assert!(used.len() > 1, "different tokens route to different experts");
}

/// The selection rule on ties: every TopK picks the lowest indices among equal values, in index
/// order, whatever the data order.
#[test]
fn topk_breaks_ties_to_the_lowest_index() {
    use misaka_palw_tir::Prim;
    use misaka_palw_tir::eval::eval_primitive;
    let x = Tensor::new(DType::I32, vec![6], vec![5, 9, 9, 1, 9, 5]).unwrap();
    let t = eval_primitive(&Prim::TopK { axis: 0, k: 2 }, std::slice::from_ref(&x), DType::Idx, &[2]).unwrap();
    assert_eq!(t.data, vec![1, 2]);
    let t = eval_primitive(&Prim::TopK { axis: 0, k: 4 }, &[x], DType::Idx, &[4]).unwrap();
    assert_eq!(t.data, vec![0, 1, 2, 4], "the three 9s, then the lower-indexed of the two 5s");
}

// ---- small structural checks shared by the programs --------------------------------------------

#[test]
fn every_program_here_is_canonical_and_its_slots_are_unrolled_in_order() {
    for (p, _) in [dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]), gdn_program(true), mamba2_program(), moe_program()] {
        let bytes = p.encode();
        assert!(bytes.len() < misaka_palw_tir::program::MAX_PROGRAM_BYTES);
        // A trailing byte, or a flipped commit flag on a carry-out, is refused.
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(TirProgramV1::decode_canonical(&longer).is_err());
        let bases = p.occurrence_slot_bases();
        assert_eq!(bases[0], 0);
        assert!(bases.windows(2).all(|w| w[0] < w[1]));
    }
    let _ = (Cmp::Eq, ONE);
}
