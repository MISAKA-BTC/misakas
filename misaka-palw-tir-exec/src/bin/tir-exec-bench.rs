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

#[cfg(not(feature = "legacy-bench"))]
fn legacy(_: &Geo, _: usize, _: usize) {
    eprintln!("--legacy needs `--features legacy-bench` (the legacy engine lives in misaka-palw-base0)");
    std::process::exit(2);
}
