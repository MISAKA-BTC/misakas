//! **`tir-exec-bench`: single-position decode of a Qwen2.5-1.5B-shaped PALW-TIR program** on the
//! typed backend, and (with `--legacy`, feature `legacy-bench`) the legacy A16 engine at the same
//! geometry — the comparison of RFC-0002 Phase F step F9.
//!
//! The program is written with `tir_library_v1` in the dense lowering's conventions
//! (`misaka-palw-tir-lower`, `lower/mod.rs`): an `i32` residual carry; `i8` weights `[out, in]` read
//! by `MatMul(W, x[in, 1]) → i64` and narrowed per channel `(m, s, z)`; the wide RMS norm; RoPE by
//! `rotate_half` from two-level angle tables; grouped-query attention over `k`/`v` histories with a
//! two-pass Q24 softmax; SiLU as a 65,536-entry code table; the GLU product narrowed to codes; the
//! LM head over the whole vocabulary. Weights are synthetic, of the real shapes (1.54 G `i8`).
//!
//! ```text
//! cargo run --release -p misaka-palw-tir-exec --bin tir-exec-bench -- [--layers 28] [--prefill 64] [--decode 32] [--profile]
//! cargo run --release -p misaka-palw-tir-exec --features legacy-bench --bin tir-exec-bench -- --legacy [...]
//! ```
//!
//! One engine per process (each holds ~1.6–1.8 GB of weights).

use std::borrow::Cow;
use std::time::Instant;

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::library::attn::AttnCfg;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS, INPUT_TOKEN, TirProgramV1};
use misaka_palw_tir::{DType, Ref, TensorType};
use misaka_palw_tir_exec::{NoSink, ParamData, TirExecutor, TirParams, TirPlan};

#[derive(Clone, Copy, Debug)]
struct Geo {
    layers: u32,
    d: u32,
    heads: u32,
    kv: u32,
    hd: u32,
    ff: u32,
    vocab: u32,
}

/// Qwen2.5-1.5B.
const QWEN25_1_5B: Geo = Geo { layers: 28, d: 1536, heads: 12, kv: 2, hd: 128, ff: 8960, vocab: 151_936 };

/// How a param is filled.
#[derive(Clone, Copy, Debug)]
enum Fill {
    /// `i8` weight codes, uniform in `[−127, 127]`.
    W8,
    /// A multiplier, uniform in `[lo, hi]`.
    M(i64, i64),
    /// A shift amount.
    S(i8),
    /// A bias at the output scale, small.
    Z,
    /// A 65,536-entry `i16` activation table (a SiLU-like curve on the code grid).
    Table,
    /// A two-level RoPE table (Q24 in `[−ONE, ONE]`).
    Rope,
}

struct Decl {
    fills: Vec<Fill>,
}

impl Decl {
    fn p(&mut self, pb: &mut ProgramBuilder, name: &str, dt: DType, shape: &[u32], per_layer: bool, f: Fill) -> Ref {
        self.fills.push(f);
        pb.param(name, dt, shape, per_layer)
    }

    /// A per-channel narrowing `(m, s, z)` of `n` channels.
    #[allow(clippy::too_many_arguments)]
    fn narrowing(
        &mut self,
        pb: &mut ProgramBuilder,
        name: &str,
        n: u32,
        per_layer: bool,
        m: (i64, i64),
        s: i8,
        bias: bool,
    ) -> Narrowing {
        let mm = self.p(pb, &format!("{name}.m"), DType::I64, &[n], per_layer, Fill::M(m.0, m.1));
        let ss = self.p(pb, &format!("{name}.s"), DType::I8, &[n], per_layer, Fill::S(s));
        let z = bias.then(|| self.p(pb, &format!("{name}.z"), DType::I64, &[n], per_layer, Fill::Z));
        Narrowing::new(mm, ss, z)
    }
}

/// `W·x` for `W:i8[rows, cols]` and a code row `x[cols]`: the `i64` accumulator `[rows]`.
fn proj(b: &mut BlockBuilder<'_>, w: Ref, x: Ref, rows: u32, cols: u32) -> Ref {
    let xc = b.reshape_fixed(x, &[cols, 1]);
    let acc = b.matmul(w, xc, DType::I64);
    b.reshape_fixed(acc, &[rows])
}

fn residual(b: &mut BlockBuilder<'_>, x: Ref, y: Ref) -> Ref {
    let s = b.add(x, y, DType::I64);
    b.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32)
}

fn qwen2_program(g: &Geo) -> (TirProgramV1, Vec<Fill>) {
    let (d, kvd, qd) = (g.d, g.kv * g.hd, g.heads * g.hd);
    let mut pb = ProgramBuilder::new(g.vocab, HISTORY_BOUND_V1_SMALL);
    let mut c = Decl { fills: Vec::new() };
    let tok = c.p(&mut pb, "tok_embd", DType::I8, &[g.vocab, d], false, Fill::W8);
    let lift = c.narrowing(&mut pb, "tok_embd.lift", g.vocab, false, (1 << 10, 1 << 11), 0, false);
    let half = g.hd / 2;
    let rope = [
        c.p(&mut pb, "rope.cos_hi", DType::I32, &[512, half], false, Fill::Rope),
        c.p(&mut pb, "rope.sin_hi", DType::I32, &[512, half], false, Fill::Rope),
        c.p(&mut pb, "rope.cos_lo", DType::I32, &[512, half], false, Fill::Rope),
        c.p(&mut pb, "rope.sin_lo", DType::I32, &[512, half], false, Fill::Rope),
    ];
    let attn_norm = c.narrowing(&mut pb, "blk.attn_norm", d, true, (1 << 12, 1 << 13), 22, false);
    let wq = c.p(&mut pb, "blk.attn_q.weight", DType::I8, &[qd, d], true, Fill::W8);
    let nq = c.narrowing(&mut pb, "blk.attn_q", qd, true, (1 << 8, 1 << 9), 19, true);
    let wk = c.p(&mut pb, "blk.attn_k.weight", DType::I8, &[kvd, d], true, Fill::W8);
    let nk = c.narrowing(&mut pb, "blk.attn_k", kvd, true, (1 << 8, 1 << 9), 19, true);
    let wv = c.p(&mut pb, "blk.attn_v.weight", DType::I8, &[kvd, d], true, Fill::W8);
    let nv = c.narrowing(&mut pb, "blk.attn_v", kvd, true, (1 << 8, 1 << 9), 19, true);
    let score = c.narrowing(&mut pb, "blk.attn_score", 1, true, (1, 2), 7, false);
    let value = c.narrowing(&mut pb, "blk.attn_value", 1, true, (1, 2), 24, false);
    let wo = c.p(&mut pb, "blk.attn_output.weight", DType::I8, &[d, qd], true, Fill::W8);
    let no = c.narrowing(&mut pb, "blk.attn_output", d, true, (1 << 8, 1 << 9), 10, false);
    let ffn_norm = c.narrowing(&mut pb, "blk.ffn_norm", d, true, (1 << 12, 1 << 13), 22, false);
    let wg = c.p(&mut pb, "blk.ffn_gate.weight", DType::I8, &[g.ff, d], true, Fill::W8);
    let ng = c.narrowing(&mut pb, "blk.ffn_gate", g.ff, true, (1 << 8, 1 << 9), 19, false);
    let act = c.p(&mut pb, "blk.ffn_act.table", DType::I16, &[65_536], true, Fill::Table);
    let wu = c.p(&mut pb, "blk.ffn_up.weight", DType::I8, &[g.ff, d], true, Fill::W8);
    let nu = c.narrowing(&mut pb, "blk.ffn_up", g.ff, true, (1 << 8, 1 << 9), 19, false);
    let nmul = c.narrowing(&mut pb, "blk.ffn_mul", g.ff, true, (1 << 8, 1 << 9), 23, false);
    let wd = c.p(&mut pb, "blk.ffn_down.weight", DType::I8, &[d, g.ff], true, Fill::W8);
    let nd = c.narrowing(&mut pb, "blk.ffn_down", d, true, (1 << 8, 1 << 9), 12, false);
    let out_norm = c.narrowing(&mut pb, "output_norm", d, false, (1 << 12, 1 << 13), 22, false);
    let wout = c.p(&mut pb, "output.weight", DType::I8, &[g.vocab, d], false, Fill::W8);
    let nout = c.narrowing(&mut pb, "output", g.vocab, false, (1 << 8, 1 << 9), 12, false);
    let k_cache = pb.hist_state("k_cache", DType::I16, &[kvd], HISTORY_BOUND_V1_SMALL, true);
    let v_cache = pb.hist_state("v_cache", DType::I16, &[kvd], HISTORY_BOUND_V1_SMALL, true);

    let carry = vec![TensorType::fixed(DType::I32, &[d])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let m = b.gather(lift.m, Ref::Input(INPUT_TOKEN), 0, 0);
        let s = b.gather(lift.s, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.narrow_wide(row, &Narrowing::new(m, s, None));
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("qwen2.dense", carry.clone());
        let x = Ref::CarryIn(0);
        let eps_zero = b.c(DType::I64, 1);
        let eps_shift = b.c(DType::I8, 0);
        let (cos, sin) = b.rope_angles_two_level(Ref::Input(INPUT_POS), rope[0], rope[1], rope[2], rope[3], 9);
        let u = b.rms_norm_wide_q36(x, eps_zero, eps_shift);
        let h = b.narrow_codes(u, &attn_norm);
        let h = b.commit(h);
        let q = proj(&mut b, wq, h, qd, d);
        let q = b.narrow_codes(q, &nq);
        let q = b.commit(q);
        let k = proj(&mut b, wk, h, kvd, d);
        let k = b.narrow_codes(k, &nk);
        let k = b.commit(k);
        let v = proj(&mut b, wv, h, kvd, d);
        let v = b.narrow_codes(v, &nv);
        let v = b.commit(v);
        let q = b.reshape_fixed(q, &[g.heads, g.hd]);
        let q = b.rope_half(q, cos, sin, -32767, 32767, DType::I16);
        let q = b.reshape_fixed(q, &[qd]);
        let k = b.reshape_fixed(k, &[g.kv, g.hd]);
        let k = b.rope_half(k, cos, sin, -32767, 32767, DType::I16);
        let k = b.reshape_fixed(k, &[kvd]);
        let kh = b.hist_append(k_cache, k);
        let vh = b.hist_append(v_cache, v);
        let cfg = AttnCfg {
            heads: g.heads,
            kv_heads: g.kv,
            head_dim: g.hd,
            score,
            softcap: None,
            alibi: None,
            sink: None,
            up_bits: 0,
            value,
        };
        let ctx = b.attention(q, kh, vh, &cfg);
        let ctx = b.commit(ctx);
        let o = proj(&mut b, wo, ctx, d, qd);
        let o = b.narrow_wide(o, &no);
        let x1 = residual(&mut b, x, o);
        let x1 = b.commit(x1);
        let u2 = b.rms_norm_wide_q36(x1, eps_zero, eps_shift);
        let h2 = b.narrow_codes(u2, &ffn_norm);
        let h2 = b.commit(h2);
        let gate = proj(&mut b, wg, h2, g.ff, d);
        let gate = b.narrow_codes(gate, &ng);
        let gate = b.commit(gate);
        let a = b.act_table(gate, act);
        let up = proj(&mut b, wu, h2, g.ff, d);
        let up = b.narrow_codes(up, &nu);
        let up = b.commit(up);
        let prod = b.mul(a, up, DType::I32);
        let glu = b.narrow_codes(prod, &nmul);
        let glu = b.commit(glu);
        let dn = proj(&mut b, wd, glu, d, g.ff);
        let dn = b.narrow_wide(dn, &nd);
        let x2 = residual(&mut b, x1, dn);
        b.finish(&[x2])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let eps_zero = b.c(DType::I64, 1);
        let eps_shift = b.c(DType::I8, 0);
        let u = b.rms_norm_wide_q36(Ref::CarryIn(0), eps_zero, eps_shift);
        let h = b.narrow_codes(u, &out_norm);
        let h = b.commit(h);
        let l = proj(&mut b, wout, h, g.vocab, d);
        let l = b.narrow_wide(l, &nout);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let program = pb.finish(pre, vec![layer; g.layers as usize], post, logits);
    (program, c.fills)
}

/// xorshift64*: fast deterministic bytes for 1.5 GB of synthetic weights.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

fn fill(f: Fill, n: usize, rng: &mut Rng) -> ParamData<'static> {
    match f {
        Fill::W8 => {
            let mut v = vec![0i8; n];
            for chunk in v.chunks_mut(8) {
                let r = rng.next().to_le_bytes();
                for (x, b) in chunk.iter_mut().zip(r) {
                    // Uniform in [−127, 127]: an A16 weight code never takes −128.
                    *x = ((b as u16 * 255 / 256) as i16 - 127) as i8;
                }
            }
            ParamData::I8(Cow::Owned(v))
        }
        Fill::M(lo, hi) => ParamData::I64(Cow::Owned((0..n).map(|_| rng.range(lo, hi)).collect())),
        Fill::S(s) => ParamData::I8(Cow::Owned(vec![s; n])),
        Fill::Z => ParamData::I64(Cow::Owned((0..n).map(|_| rng.range(-64, 64)).collect())),
        Fill::Table => ParamData::I16(Cow::Owned(
            (0..n)
                .map(|i| {
                    // x·σ(x) on the code grid, as a lowerer would tabulate SiLU (float at build time only).
                    let x = (i as f64 - 32768.0) / 4096.0;
                    ((x / (1.0 + (-x).exp())) * 4096.0).round().clamp(-32767.0, 32767.0) as i16
                })
                .collect(),
        )),
        Fill::Rope => ParamData::I32(Cow::Owned((0..n).map(|_| rng.range(-(1 << 24), 1 << 24) as i32).collect())),
    }
}

fn arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn rss_mib() -> Option<u64> {
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok()?;
    String::from_utf8(out.stdout).ok()?.trim().parse::<u64>().ok().map(|kib| kib / 1024)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut g = QWEN25_1_5B;
    g.layers = arg(&args, "--layers", g.layers);
    let prefill: usize = arg(&args, "--prefill", 64);
    let decode: usize = arg(&args, "--decode", 32);
    let profile = args.iter().any(|a| a == "--profile");
    println!(
        "geometry: {} layers, d {}, heads {}/{} × {}, ff {}, vocab {}; {} prefill positions stepped, then {} decode positions; {} threads",
        g.layers,
        g.d,
        g.heads,
        g.kv,
        g.hd,
        g.ff,
        g.vocab,
        prefill,
        decode,
        rayon::current_num_threads()
    );
    if args.iter().any(|a| a == "--legacy") {
        legacy(&g, prefill, decode);
        return;
    }
    if args.iter().any(|a| a == "--kernel") {
        kernel(&g);
        return;
    }
    if args.iter().any(|a| a == "--elementwise") {
        elementwise_probe(&g);
        return;
    }
    if args.iter().any(|a| a == "--mirror") {
        mirror(&g, prefill, decode, profile);
        return;
    }
    if args.iter().any(|a| a == "--fused-kernels") {
        fused_kernels(arg(&args, "--steps", 24));
        return;
    }
    let t = Instant::now();
    let (program, fills) = qwen2_program(&g);
    let plan = TirPlan::compile(&program).expect("the program validates");
    let nodes: usize = program.blocks.iter().map(|b| b.nodes.len()).sum();
    println!(
        "program: {} bytes, {} blocks, {} nodes ({} per position), compiled in {:.1} ms",
        program.encode().len(),
        program.blocks.len(),
        nodes,
        plan.slots_per_position(),
        t.elapsed().as_secs_f64() * 1e3
    );
    match misaka_palw_tir::interval::analyze_ranges(&program) {
        Ok(_) => println!("range analysis (spec 04b §7): admissible"),
        Err(e) => println!("range analysis (spec 04b §7): NOT admissible: {e}"),
    }
    let t = Instant::now();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut params = TirParams::new(&plan);
    let mut weight_bytes = 0usize;
    for &(j, layer) in &plan.param_instances {
        let d = &program.params[j as usize];
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        weight_bytes += n * d.dtype.width();
        params.insert(&plan, j, layer, fill(fills[j as usize], n, &mut rng)).expect("a param of its declaration");
    }
    println!("params: {:.2} GiB synthetic, filled in {:.1} s", weight_bytes as f64 / (1u64 << 30) as f64, t.elapsed().as_secs_f64());
    let mut exec = TirExecutor::new(&plan, &params).expect("every param bound");
    if args.iter().any(|a| a == "--fused") {
        exec.set_fused(true);
        println!("fused kernels on: {:?}", exec.fused_summary());
    }
    let tokens: Vec<u32> = (0..prefill + decode).map(|i| ((i as u64 * 7919 + 1013) % g.vocab as u64) as u32).collect();
    let t = Instant::now();
    for &tok in &tokens[..prefill] {
        exec.step(tok, &mut NoSink).expect("a step");
    }
    let pre = t.elapsed();
    exec.set_profile(profile);
    let t = Instant::now();
    let mut times = Vec::with_capacity(decode);
    for &tok in &tokens[prefill..] {
        let s = Instant::now();
        exec.step(tok, &mut NoSink).expect("a step");
        times.push(s.elapsed().as_secs_f64());
    }
    let dec = t.elapsed();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (_, logits) = exec.logits();
    let checksum = logits.to_i128s().iter().fold(0i128, |a, v| a.wrapping_mul(31).wrapping_add(*v));
    println!(
        "TIR backend: prefill {:.1} ms/position; decode {:.1} ms/token median ({:.1} min, {:.1} max) → {:.2} tok/s; logits checksum {checksum:x}; RSS {} MiB",
        pre.as_secs_f64() * 1e3 / prefill.max(1) as f64,
        times[times.len() / 2] * 1e3,
        times[0] * 1e3,
        times[times.len() - 1] * 1e3,
        decode as f64 / dec.as_secs_f64(),
        rss_mib().unwrap_or(0)
    );
    if let Some(prof) = exec.profile() {
        let total: u64 = prof.iter().map(|e| e.0).sum();
        let mut rows: Vec<(usize, u64, u64)> = prof.iter().enumerate().filter(|(_, e)| e.1 > 0).map(|(i, e)| (i, e.0, e.1)).collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        println!("profile over {decode} decode tokens (per token):");
        for (tag, ns, count) in rows {
            println!(
                "  {:>10} {:7.2} ms {:5.1}%  {:6} nodes",
                misaka_palw_tir::prim::PRIM_NAMES_V1[tag],
                ns as f64 / 1e6 / decode as f64,
                100.0 * ns as f64 / total as f64,
                count / decode as u64
            );
        }
    }
}

/// The projection GEMV alone: `[ff, d] · [d]` and `[vocab, d] · [d]`, the backend's MatMul node
/// against the legacy engine's `a16_matmul_rescale_fast` (with `legacy-bench`).
fn kernel(g: &Geo) {
    use misaka_palw_tir::Prim;
    use misaka_palw_tir_exec::kernels::{Opd, Scratch, matmul};
    use misaka_palw_tir_exec::layout::Layout;
    use misaka_palw_tir_exec::{Buf, Slice};
    for (rows, cols) in [(g.ff as usize, g.d as usize), (g.d as usize, g.ff as usize), (g.vocab as usize, g.d as usize)] {
        let mut rng = Rng(7);
        let w: Vec<i8> = (0..rows * cols).map(|_| (rng.next() % 255) as i8).collect();
        let x: Vec<i16> = (0..cols).map(|_| (rng.next() % 32767) as i16).collect();
        let a = Opd { data: Slice::I8(&w), layout: Layout::contiguous(&[rows, cols]) };
        let b = Opd { data: Slice::I16(&x), layout: Layout::contiguous(&[cols, 1]) };
        let node = misaka_palw_tir_exec::plan::NodePlan::for_kernel(Prim::MatMul, DType::I64, misaka_palw_tir_exec::plan::Acc::Fast64);
        let mut out = Buf::default();
        let mut scratch = Scratch::default();
        let iters = (2_000_000_000 / (rows * cols)).max(3);
        matmul::matmul(&node, &a, &b, &[rows, 1], &mut out, &mut scratch).unwrap();
        let t = Instant::now();
        for _ in 0..iters {
            matmul::matmul(&node, &a, &b, &[rows, 1], &mut out, &mut scratch).unwrap();
        }
        let per = t.elapsed().as_secs_f64() / iters as f64;
        println!("TIR MatMul [{rows}, {cols}]·[{cols}, 1]: {:.3} ms, {:.1} GMAC/s", per * 1e3, (rows * cols) as f64 / per / 1e9);
        legacy_kernel(&w, &x, rows, cols, iters);
    }
}

/// A `[ff]` Clamp alone, and the same right after a pooled GEMV (rayon workers still spinning).
fn elementwise_probe(g: &Geo) {
    use misaka_palw_tir::Prim;
    use misaka_palw_tir_exec::kernels::{Opd, Scratch, elementwise, matmul};
    use misaka_palw_tir_exec::layout::Layout;
    use misaka_palw_tir_exec::plan::{Acc, NodePlan};
    use misaka_palw_tir_exec::{Buf, Slice};
    let n = g.ff as usize;
    let x: Vec<i8> = (0..n).map(|i| (i % 50) as i8).collect();
    let xo = Opd { data: Slice::I8(&x), layout: Layout::contiguous(&[n]) };
    let mut node = NodePlan::for_kernel(Prim::Clamp { lo: 0, hi: 62 }, DType::Idx, Acc::Fast64);
    node.out = TensorType::fixed(DType::Idx, &[n as u32]);
    let mut out = Buf::default();
    let mut scratch = Scratch::default();
    let iters = 20_000;
    let t = Instant::now();
    for _ in 0..iters {
        elementwise::unary(&node, &xo, &mut out, &mut scratch, &[]).unwrap();
    }
    println!("Clamp [{n}] i8→idx alone: {:.2} ns/element", t.elapsed().as_secs_f64() * 1e9 / (iters * n) as f64);
    let mut rng = Rng(7);
    let (rows, cols) = (g.ff as usize, g.d as usize);
    let w: Vec<i8> = (0..rows * cols).map(|_| (rng.next() % 255) as i8).collect();
    let xs: Vec<i16> = (0..cols).map(|_| (rng.next() % 32767) as i16).collect();
    let a = Opd { data: Slice::I8(&w), layout: Layout::contiguous(&[rows, cols]) };
    let b = Opd { data: Slice::I16(&xs), layout: Layout::contiguous(&[cols, 1]) };
    let mm = NodePlan::for_kernel(Prim::MatMul, DType::I64, Acc::Fast64);
    let mut mo = Buf::default();
    let (mut t_mm, mut t_ew) = (0f64, 0f64);
    for _ in 0..200 {
        let s = Instant::now();
        matmul::matmul(&mm, &a, &b, &[rows, 1], &mut mo, &mut scratch).unwrap();
        t_mm += s.elapsed().as_secs_f64();
        let s = Instant::now();
        for _ in 0..10 {
            elementwise::unary(&node, &xo, &mut out, &mut scratch, &[]).unwrap();
        }
        t_ew += s.elapsed().as_secs_f64();
    }
    println!("after each pooled GEMV: Clamp {:.2} ns/element (GEMV {:.3} ms)", t_ew * 1e9 / (200 * 10 * n) as f64, t_mm * 1e3 / 200.0);
}

#[cfg(feature = "legacy-bench")]
fn legacy_kernel(w: &[i8], x: &[i16], rows: usize, _cols: usize, iters: usize) {
    use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
    let xs: Vec<i32> = x.iter().map(|v| *v as i32).collect();
    let p = vec![A16QuantParams { multiplier: 1, shift: 0, zero: 0 }; rows];
    misaka_palw_base0::kernels::a16_matmul_rescale_fast(w, &xs, &p).unwrap();
    let t = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(misaka_palw_base0::kernels::a16_matmul_rescale_fast(w, &xs, &p).unwrap());
    }
    let per = t.elapsed().as_secs_f64() / iters as f64;
    println!("legacy a16_matmul_rescale_fast: {:.3} ms, {:.1} GMAC/s", per * 1e3, (w.len()) as f64 / per / 1e9);
}

#[cfg(not(feature = "legacy-bench"))]
fn legacy_kernel(_: &[i8], _: &[i16], _: usize, _: usize, _: usize) {}

#[cfg(feature = "legacy-bench")]
fn legacy(g: &Geo, prefill: usize, decode: usize) {
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::engine_a16::{A16Cache, A16Engine, derived_a16_store};
    let shape = Base0ShapeV1 {
        n_layers: g.layers as usize,
        n_heads: g.heads as usize,
        n_kv_heads: g.kv as usize,
        d_head: g.hd as usize,
        d_ff: g.ff as usize,
        vocab: g.vocab as usize,
        max_position: (prefill + decode).next_power_of_two().max(64),
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: 1,
    };
    let t = Instant::now();
    let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
        .expect("a valid shape")
        .with_a16_params(derived_a16_store(&shape))
        .expect("the derived store");
    println!("legacy artifact derived in {:.1} s", t.elapsed().as_secs_f64());
    let engine = A16Engine::new(&artifact).expect("the store resolves");
    let mut cache = A16Cache::new(shape.n_layers);
    let tokens: Vec<usize> = (0..prefill + decode).map(|i| (i * 7919 + 1013) % shape.vocab).collect();
    let t = Instant::now();
    for (pos, tok) in tokens[..prefill].iter().enumerate() {
        engine.forward_token(&mut cache, *tok, pos).expect("a step");
    }
    let pre = t.elapsed();
    let t = Instant::now();
    let mut times = Vec::with_capacity(decode);
    let mut last = Vec::new();
    for (i, tok) in tokens[prefill..].iter().enumerate() {
        let s = Instant::now();
        last = engine.forward_token(&mut cache, *tok, prefill + i).expect("a step");
        times.push(s.elapsed().as_secs_f64());
    }
    let dec = t.elapsed();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let checksum = last.iter().fold(0i128, |a, v| a.wrapping_mul(31).wrapping_add(*v as i128));
    println!(
        "legacy A16 engine: prefill {:.1} ms/position; decode {:.1} ms/token median ({:.1} min, {:.1} max) → {:.2} tok/s; logits checksum {checksum:x}; RSS {} MiB",
        pre.as_secs_f64() * 1e3 / prefill.max(1) as f64,
        times[times.len() / 2] * 1e3,
        times[0] * 1e3,
        times[times.len() - 1] * 1e3,
        decode as f64 / dec.as_secs_f64(),
        rss_mib().unwrap_or(0)
    );
}

/// **The same function on both engines**: the legacy A16 artifact at this geometry, the TIR program
/// that mirrors `A16Engine::forward_token` (`misaka_palw_base0::tir_a16`, tir/lower F3) over the
/// converted tensors, and every logit of every position compared — then the speed of each.
#[cfg(feature = "legacy-bench")]
fn mirror(g: &Geo, prefill: usize, decode: usize, profile: bool) {
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::engine_a16::{A16Cache, A16Engine, derived_a16_store};
    use misaka_palw_base0::tir_a16::{a16_mirror_program, a16_tir_tensor_bytes};
    let shape = Base0ShapeV1 {
        n_layers: g.layers as usize,
        n_heads: g.heads as usize,
        n_kv_heads: g.kv as usize,
        d_head: g.hd as usize,
        d_ff: g.ff as usize,
        vocab: g.vocab as usize,
        max_position: (prefill + decode).next_power_of_two().max(64),
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: 1,
    };
    let t = Instant::now();
    let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
        .expect("a valid shape")
        .with_a16_params(derived_a16_store(&shape))
        .expect("the derived store");
    let program = a16_mirror_program(&shape, HISTORY_BOUND_V1_SMALL).expect("the mirror program");
    let bytes = a16_tir_tensor_bytes(&artifact, &program).expect("the conversion");
    println!(
        "legacy artifact derived and converted in {:.1} s; mirror program {} bytes, {} nodes",
        t.elapsed().as_secs_f64(),
        program.encode().len(),
        program.blocks.iter().map(|b| b.nodes.len()).sum::<usize>()
    );
    let tokens: Vec<usize> = (0..prefill + decode).map(|i| (i * 7919 + 1013) % shape.vocab).collect();
    // The legacy engine first; its logits kept, its artifact dropped before the backend runs.
    let mut want: Vec<Vec<i32>> = Vec::with_capacity(tokens.len());
    let legacy_decode = {
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let mut cache = A16Cache::new(shape.n_layers);
        let mut times = Vec::new();
        for (pos, tok) in tokens.iter().enumerate() {
            let s = Instant::now();
            want.push(engine.forward_token(&mut cache, *tok, pos).expect("a legacy step"));
            if pos >= prefill {
                times.push(s.elapsed().as_secs_f64());
            }
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        times
    };
    drop(artifact);
    let plan = TirPlan::compile(&program).expect("the mirror program validates");
    let mut params = TirParams::new(&plan);
    for ((j, layer), b) in &bytes {
        let d = &program.params[*j as usize];
        params
            .insert(&plan, *j, *layer, ParamData::from_le_bytes(d.dtype, b).expect("whole elements"))
            .expect("a param of its declaration");
    }
    let mut exec = TirExecutor::new(&plan, &params).expect("every param bound");
    exec.set_profile(profile);
    let mut times = Vec::new();
    for (pos, tok) in tokens.iter().enumerate() {
        let s = Instant::now();
        exec.step(*tok as u32, &mut NoSink).expect("a TIR step");
        if pos >= prefill {
            times.push(s.elapsed().as_secs_f64());
        }
        let (_, got) = exec.logits();
        let got: Vec<i128> = got.to_i128s();
        let w: Vec<i128> = want[pos].iter().map(|v| *v as i128).collect();
        assert_eq!(got, w, "position {pos}: the TIR backend's logits differ from the legacy engine's");
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("logits identical to the legacy engine at all {} positions ({} logits each)", tokens.len(), shape.vocab);
    let med = |t: &[f64]| t[t.len() / 2] * 1e3;
    println!(
        "decode median: legacy A16 engine {:.1} ms/token ({:.2} tok/s), TIR backend on the mirror program {:.1} ms/token ({:.2} tok/s) → {:.2}× ; RSS {} MiB",
        med(&legacy_decode),
        1e3 / med(&legacy_decode),
        med(&times),
        1e3 / med(&times),
        med(&times) / med(&legacy_decode),
        rss_mib().unwrap_or(0)
    );
    // The same program with the fused kernels on (RFC-0002 Phase G): every logit again, then the speed.
    let mut fx = TirExecutor::new(&plan, &params).expect("every param bound");
    fx.set_fused(true);
    let mut ftimes = Vec::new();
    for (pos, tok) in tokens.iter().enumerate() {
        let s = Instant::now();
        fx.step(*tok as u32, &mut NoSink).expect("a fused TIR step");
        if pos >= prefill {
            ftimes.push(s.elapsed().as_secs_f64());
        }
        let got: Vec<i128> = fx.logits().1.to_i128s();
        let w: Vec<i128> = want[pos].iter().map(|v| *v as i128).collect();
        assert_eq!(got, w, "position {pos}: the fused backend's logits differ from the legacy engine's");
    }
    ftimes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "fused kernels {:?}: logits identical at all {} positions; decode median {:.1} ms/token → {:.2}× the generic backend, {:.2}× the legacy engine",
        fx.fused_summary(),
        tokens.len(),
        med(&ftimes),
        med(&ftimes) / med(&times),
        med(&ftimes) / med(&legacy_decode)
    );
    if std::env::var_os("TIR_EXEC_NODE_PROFILE").is_some() {
        let mut rows: Vec<(u64, usize, usize)> = Vec::new();
        for (bi, b) in exec.node_profile().iter().enumerate() {
            for (ni, ns) in b.iter().enumerate() {
                rows.push((*ns, bi, ni));
            }
        }
        rows.sort_by(|a, b| b.0.cmp(&a.0));
        let refined = plan.refine(&|j, l| params.range(j, l));
        for (ns, bi, ni) in rows.into_iter().take(25) {
            let occ = plan.occurrences.iter().position(|(b, _)| *b as usize == bi).unwrap();
            let np = &refined[occ].nodes[ni];
            println!(
                "  block {bi} node {ni:3} {:>10} {:7.3} ms/pos  out {:?} {:?} store {:?} identity {} work {:?} check {}",
                np.prim.name(),
                ns as f64 / 1e6 / tokens.len() as f64,
                np.out.dtype,
                np.out.shape,
                np.store,
                np.identity,
                np.work,
                np.check_out
            );
        }
    }
    if let Some(prof) = exec.profile() {
        let total: u64 = prof.iter().map(|e| e.0).sum();
        let n = tokens.len() as f64;
        let mut rows: Vec<(usize, u64, u64)> = prof.iter().enumerate().filter(|(_, e)| e.1 > 0).map(|(i, e)| (i, e.0, e.1)).collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        println!("TIR profile over all {} positions (per position):", tokens.len());
        for (tag, ns, count) in rows.into_iter().take(10) {
            println!(
                "  {:>10} {:7.2} ms {:5.1}%  {:6} nodes",
                misaka_palw_tir::prim::PRIM_NAMES_V1[tag],
                ns as f64 / 1e6 / n,
                100.0 * ns as f64 / total as f64,
                (count as f64 / n) as u64
            );
        }
    }
}

/// Records the committed values of a step.
struct Commits(Vec<Vec<i128>>);
impl misaka_palw_tir_exec::StepSink for Commits {
    fn node(&mut self, v: &misaka_palw_tir_exec::NodeValue<'_>) {
        if v.commit {
            self.0.push(v.data.to_i128s());
        }
    }
}

/// `steps` positions of `program` over `params`, generic or fused: the per-step times (sorted) and
/// every step's committed values.
fn run_steps(plan: &TirPlan, params: &TirParams<'_>, fused: bool, steps: usize) -> (Vec<f64>, Vec<Vec<Vec<i128>>>) {
    let mut exec = TirExecutor::new(plan, params).expect("every param bound");
    exec.set_fused(fused);
    if fused {
        assert!(exec.fused_summary().iter().any(|(_, c)| *c > 0), "the kernel matched: {:?}", exec.fused_summary());
    }
    let (mut times, mut commits) = (Vec::with_capacity(steps), Vec::with_capacity(steps));
    for i in 0..steps {
        let mut sink = Commits(Vec::new());
        let s = Instant::now();
        exec.step((i % 16) as u32, &mut sink).expect("a step");
        times.push(s.elapsed().as_secs_f64());
        commits.push(sink.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (times, commits)
}

/// Params for a kernel program, by name: plausible magnitudes of a calibrated artifact.
fn kernel_param(name: &str, n: usize, rng: &mut Rng) -> ParamData<'static> {
    let tail = name.rsplit('.').next().unwrap_or(name);
    let r = |rng: &mut Rng, lo: i64, hi: i64| rng.range(lo, hi);
    match (name, tail) {
        (_, "m") => ParamData::I64(Cow::Owned((0..n).map(|_| r(rng, 1 << 8, 1 << 9)).collect())),
        (_, "s") => ParamData::I8(Cow::Owned(vec![19; n])),
        (_, "z") => ParamData::I64(Cow::Owned((0..n).map(|_| r(rng, -64, 64)).collect())),
        ("w", _) => fill(Fill::W8, n, rng),
        ("write_shift", _) => ParamData::I32(Cow::Owned((0..n).map(|_| r(rng, -3, 3) as i32).collect())),
        ("decay.table" | "beta.table", _) => ParamData::I32(Cow::Owned((0..n).map(|_| r(rng, 0, 1 << 24) as i32).collect())),
        ("w.table", _) => ParamData::I32(Cow::Owned((0..n).map(|_| r(rng, 0, 1 << 25) as i32).collect())),
        ("y.table" | "v.table", _) => ParamData::I32(Cow::Owned((0..n).map(|_| r(rng, -(1 << 20), 1 << 20) as i32).collect())),
        _ => ParamData::I16(Cow::Owned((0..n).map(|_| r(rng, -32767, 32767) as i16).collect())),
    }
}

const KERNEL_VOCAB: u32 = 16;

/// `a16_matmul` as a one-layer program (the gate's, `misaka-palw-tir-lower/tests/fused_gate.rs`).
fn a16_matmul_kernel_program(rows: u32, k: u32, wide: bool, z: bool) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(KERNEL_VOCAB, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("x.table", DType::I16, &[KERNEL_VOCAB, k], false);
    let w = pb.param("w", DType::I8, &[rows, k], true);
    let m = pb.param("n.m", DType::I64, &[rows], true);
    let s = pb.param("n.s", DType::I8, &[rows], true);
    let zp = z.then(|| pb.param("n.z", DType::I64, &[rows], true));
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(table, Ref::Input(INPUT_TOKEN), 0, 0);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", vec![TensorType::fixed(DType::I16, &[k])]);
        let y = b.a16_matmul(w, Ref::CarryIn(0), &Narrowing::new(m, s, zp), wide);
        b.commit(y);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[k]);
        b.finish(&[x])
    };
    let post = {
        let mut b = pb.block("post", vec![TensorType::fixed(DType::I16, &[k])]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[k]);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![layer], post, 0)
}

/// `moe_combine_q36` as a one-layer program.
fn moe_kernel_program(k: u32, width: u32) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(KERNEL_VOCAB, HISTORY_BOUND_V1_SMALL);
    let yt = pb.param("y.table", DType::I32, &[KERNEL_VOCAB, k * width], false);
    let wt = pb.param("w.table", DType::I32, &[KERNEL_VOCAB, k], false);
    let m = pb.param("n.m", DType::I64, &[1], true);
    let s = pb.param("n.s", DType::I8, &[1], true);
    let z = pb.param("n.z", DType::I64, &[width], true);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let y = b.gather(yt, Ref::Input(INPUT_TOKEN), 0, 0);
        b.finish(&[y])
    };
    let layer = {
        let mut b = pb.block("layer", vec![TensorType::fixed(DType::I32, &[k * width])]);
        let y = b.reshape_fixed(Ref::CarryIn(0), &[k, width]);
        let w = b.gather(wt, Ref::Input(INPUT_TOKEN), 0, 0);
        let p2 = b.pow2_of(s);
        let r = b.moe_combine_q36(y, w, m, p2, z, -32767, 32767, DType::I16);
        b.commit(r);
        let c = b.reshape_fixed(Ref::CarryIn(0), &[k * width]);
        b.finish(&[c])
    };
    let post = {
        let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[k * width])]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[k * width]);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![layer], post, 0)
}

/// `gdn_step_q36` as a one-layer program over a `Fixed` state `[h, dv, dk]`.
fn gdn_kernel_program(h: u32, dv: u32, dk: u32) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(KERNEL_VOCAB, HISTORY_BOUND_V1_SMALL);
    let kt = pb.param("k.table", DType::I16, &[KERNEL_VOCAB, h * dk], false);
    let qt = pb.param("q.table", DType::I16, &[KERNEL_VOCAB, h * dk], false);
    let vt = pb.param("v.table", DType::I32, &[KERNEL_VOCAB, h * dv], false);
    let dect = pb.param("decay.table", DType::I32, &[KERNEL_VOCAB, h], false);
    let bett = pb.param("beta.table", DType::I32, &[KERNEL_VOCAB, h], false);
    let triple = |pb: &mut ProgramBuilder, name: &str| {
        (
            pb.param(&format!("{name}.m"), DType::I64, &[h], true),
            pb.param(&format!("{name}.s"), DType::I8, &[h], true),
            pb.param(&format!("{name}.z"), DType::I64, &[h], true),
        )
    };
    let read = triple(&mut pb, "read");
    let delta = triple(&mut pb, "delta");
    let out = triple(&mut pb, "out");
    let ws = pb.param("write_shift", DType::I32, &[h], true);
    let smax = i32::MAX as i64;
    let state = pb.fixed_state("S", DType::I32, &[h, dv, dk], -smax, smax, true);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let v = b.gather(vt, Ref::Input(INPUT_TOKEN), 0, 0);
        b.finish(&[v])
    };
    let layer = {
        let mut b = pb.block("layer", vec![TensorType::fixed(DType::I32, &[h * dv])]);
        let tok = Ref::Input(INPUT_TOKEN);
        let k = b.gather(kt, tok, 0, 0);
        let k = b.reshape_fixed(k, &[h, dk]);
        let q = b.gather(qt, tok, 0, 0);
        let q = b.reshape_fixed(q, &[h, dk]);
        let v = b.reshape_fixed(Ref::CarryIn(0), &[h, dv]);
        let decay = b.gather(dect, tok, 0, 0);
        let beta = b.gather(bett, tok, 0, 0);
        let narrowing = |b: &mut BlockBuilder<'_>, (m, s, z): (Ref, Ref, Ref)| (m, b.pow2_of(s), z);
        let (r, d, o) = (narrowing(&mut b, read), narrowing(&mut b, delta), narrowing(&mut b, out));
        let y = b.gdn_step_q36(state, k, v, q, decay, beta, r, d, ws, o);
        let y = b.reshape_fixed(y, &[h * dv]);
        let y = b.commit(y);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[h * dv])]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[h * dv]);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![layer], post, 0)
}

/// **The fused kernels at a real model's sizes** (RFC-0002 Phase G): each kernel's template as a
/// one-layer program, stepped `steps` times generic and fused (every commit compared), and — with
/// `legacy-bench` — the legacy kernel on operands of the same sizes.
fn fused_kernels(steps: usize) {
    // `a16_matmul` and `moe_combine_q36` have no fused form (see
    // `misaka_palw_tir_exec::fused::kernels_v1`): their rows are the generic backend against the
    // legacy kernel, the measurement that decided it.
    let cases: Vec<(String, TirProgramV1, &str)> = vec![
        ("a16_matmul [1536, 1536] codes+z (q/o)".into(), a16_matmul_kernel_program(1536, 1536, false, true), "a16/1536x1536"),
        ("a16_matmul [8960, 1536] codes (gate/up)".into(), a16_matmul_kernel_program(8960, 1536, false, false), "a16/8960x1536"),
        ("a16_matmul [1536, 8960] wide (down)".into(), a16_matmul_kernel_program(1536, 8960, true, false), "a16/1536x8960"),
        ("a16_matmul [151936, 1536] wide (LM head)".into(), a16_matmul_kernel_program(151_936, 1536, true, false), "a16/151936x1536"),
        ("gdn_step_q36 h 32, d_v = d_k = 128 (Qwen3.6-35B)".into(), gdn_kernel_program(32, 128, 128), "gdn/32x128x128"),
        ("gdn_step_q36 h 4, d_v = d_k = 8 (tiny hybrid)".into(), gdn_kernel_program(4, 8, 8), "gdn/4x8x8"),
        ("moe_combine_q36 k 8, width 2048 (Qwen3.6-35B)".into(), moe_kernel_program(8, 2048), "moe/8x2048"),
    ];
    println!(
        "{:<52} {:>18} {:>18} {:>8} {:>12}",
        "kernel (one step of a one-layer program)", "generic µs (min)", "fused µs (min)", "min ratio", "legacy µs"
    );
    for (label, program, key) in cases {
        let fusable = key.starts_with("gdn/");
        let plan = TirPlan::compile(&program).expect("the kernel program validates");
        let mut rng = Rng(0xF05E);
        let mut params = TirParams::new(&plan);
        for &(j, layer) in &plan.param_instances {
            let d = &program.params[j as usize];
            let n: usize = d.shape.iter().map(|x| *x as usize).product();
            params.insert(&plan, j, layer, kernel_param(&d.name, n, &mut rng)).expect("a param of its declaration");
        }
        let (g, gc) = run_steps(&plan, &params, false, steps);
        // The median over the steps and, beside it, the minimum: on a shared host the minimum is the
        // kernel and the median is the kernel plus whoever else was running.
        let med = |t: &[f64]| t[t.len() / 2] * 1e6;
        let (fused, ratio) = if fusable {
            let (f, fc) = run_steps(&plan, &params, true, steps);
            assert_eq!(gc, fc, "{label}: the fused kernel's commits differ from the generic backend's");
            (format!("{:.1} ({:.1})", med(&f), f[0] * 1e6), format!("{:.2}×", g[0] / f[0]))
        } else {
            ("—".to_string(), "—".to_string())
        };
        let legacy = legacy_kernel_us(key);
        println!(
            "{label:<52} {:>18} {:>18} {:>8} {:>12}",
            format!("{:.1} ({:.1})", med(&g), g[0] * 1e6),
            fused,
            ratio,
            legacy.map_or("—".to_string(), |us| format!("{us:.1}"))
        );
    }
}

/// The legacy kernel's time on operands of `key`'s sizes, in µs (median of a few runs).
#[cfg(feature = "legacy-bench")]
fn legacy_kernel_us(key: &str) -> Option<f64> {
    use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
    use kaspa_consensus_core::palw_qwen36_ops::{Qwen36GdnParamsV1, Qwen36GdnStateV1, q36_gdn_step, q36_moe_combine};
    let mut rng = Rng(0x1E6A);
    // The minimum of a few runs, as the fused and generic columns' parenthesised figure.
    let median = |mut t: Vec<f64>| {
        t.sort_by(|a, b| a.partial_cmp(b).unwrap());
        t[0] * 1e6
    };
    let (kind, dims) = key.split_once('/')?;
    let dims: Vec<usize> = dims.split('x').map(|d| d.parse().ok()).collect::<Option<_>>()?;
    match kind {
        "a16" => {
            let (rows, cols) = (dims[0], dims[1]);
            let w: Vec<i8> = (0..rows * cols).map(|_| rng.range(-127, 127) as i8).collect();
            let x: Vec<i32> = (0..cols).map(|_| rng.range(-32767, 32767) as i32).collect();
            let p = vec![A16QuantParams { multiplier: 300, shift: 19, zero: 0 }; rows];
            Some(median(
                (0..25)
                    .map(|_| {
                        let s = Instant::now();
                        std::hint::black_box(misaka_palw_base0::kernels::a16_matmul_requant_fast(&w, &x, &p).ok());
                        s.elapsed().as_secs_f64()
                    })
                    .collect(),
            ))
        }
        "gdn" => {
            let (h, dv, dk) = (dims[0], dims[1], dims[2]);
            let mut states: Vec<Qwen36GdnStateV1> = (0..h).map(|_| Qwen36GdnStateV1::zeros(dv, dk)).collect();
            let k: Vec<i32> = (0..dk).map(|_| rng.range(-32767, 32767) as i32).collect();
            let q = k.clone();
            let v: Vec<i32> = (0..dv).map(|_| rng.range(-32767, 32767) as i32).collect();
            let a = A16QuantParams { multiplier: 300, shift: 19, zero: 0 };
            let params = Qwen36GdnParamsV1 { read: a, delta: a, write_shift: 1, out: a };
            Some(median(
                (0..25)
                    .map(|_| {
                        let s = Instant::now();
                        for st in states.iter_mut() {
                            std::hint::black_box(q36_gdn_step(st, &k, &v, &q, 1 << 23, 1 << 22, params).ok());
                        }
                        s.elapsed().as_secs_f64()
                    })
                    .collect(),
            ))
        }
        "moe" => {
            let (k, width) = (dims[0], dims[1]);
            let y: Vec<i32> = (0..k * width).map(|_| rng.range(-32767, 32767) as i32).collect();
            let w: Vec<i32> = (0..k).map(|_| rng.range(0, 1 << 24) as i32).collect();
            let p = A16QuantParams { multiplier: 300, shift: 19, zero: 0 };
            Some(median(
                (0..25)
                    .map(|_| {
                        let s = Instant::now();
                        std::hint::black_box(q36_moe_combine(&y, &w, width, p).ok());
                        s.elapsed().as_secs_f64()
                    })
                    .collect(),
            ))
        }
        _ => None,
    }
}

#[cfg(not(feature = "legacy-bench"))]
fn legacy_kernel_us(_: &str) -> Option<f64> {
    None
}

#[cfg(not(feature = "legacy-bench"))]
fn mirror(_: &Geo, _: usize, _: usize, _: bool) {
    eprintln!("--mirror needs `--features legacy-bench` (the legacy engine and its converter live in misaka-palw-base0)");
    std::process::exit(2);
}

#[cfg(not(feature = "legacy-bench"))]
fn legacy(_: &Geo, _: usize, _: usize) {
    eprintln!("--legacy needs `--features legacy-bench` (the legacy engine lives in misaka-palw-base0)");
    std::process::exit(2);
}
