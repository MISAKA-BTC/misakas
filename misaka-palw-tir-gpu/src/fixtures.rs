//! **Fixture programs shared by the benchmarks and the RFC-0006 prototype.**
//!
//! [`qwen2_program`] is `tir-exec-bench`'s Qwen2.5-shaped program (`misaka-palw-tir-exec/src/bin/
//! tir-exec-bench.rs`, the dense lowering's conventions) at any geometry: an `i32` residual carry,
//! `i8` weights `[out, in]` read by `MatMul(W, x[in, 1]) → i64` and narrowed per channel `(m, s, z)`,
//! the wide RMS norm, RoPE by `rotate_half` from two-level angle tables, grouped-query attention over
//! `k`/`v` histories with a two-pass Q24 softmax, SiLU as a 65,536-entry code table, the GLU product
//! narrowed to codes, and the LM head. [`fill`] makes synthetic params of the declared shapes.

use std::borrow::Cow;

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::library::attn::AttnCfg;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS, INPUT_TOKEN, TirProgramV1};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType};
use misaka_palw_tir_exec::ParamData;

/// The Qwen2.5-1.5B geometry.
pub const QWEN25_1_5B: Geo = Geo { layers: 28, d: 1536, heads: 12, kv: 2, hd: 128, ff: 8960, vocab: 151_936 };

#[derive(Clone, Copy, Debug)]
pub struct Geo {
    pub layers: u32,
    pub d: u32,
    pub heads: u32,
    pub kv: u32,
    pub hd: u32,
    pub ff: u32,
    pub vocab: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum Fill {
    W8,
    M(i64, i64),
    S(i8),
    Z,
    Table,
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

fn proj(b: &mut BlockBuilder<'_>, w: Ref, x: Ref, rows: u32, cols: u32) -> Ref {
    let xc = b.reshape_fixed(x, &[cols, 1]);
    let acc = b.matmul(w, xc, DType::I64);
    b.reshape_fixed(acc, &[rows])
}

fn residual(b: &mut BlockBuilder<'_>, x: Ref, y: Ref) -> Ref {
    let s = b.add(x, y, DType::I64);
    b.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32)
}

/// `tir-exec-bench`'s `qwen2_program`, verbatim in structure.
pub fn qwen2_program(g: &Geo) -> (TirProgramV1, Vec<Fill>) {
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

pub struct Rng(pub u64);
impl Rng {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

pub fn fill(f: Fill, n: usize, rng: &mut Rng) -> ParamData<'static> {
    match f {
        Fill::W8 => {
            let mut v = vec![0i8; n];
            for chunk in v.chunks_mut(8) {
                let r = rng.next().to_le_bytes();
                for (x, b) in chunk.iter_mut().zip(r) {
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
                    let x = (i as f64 - 32768.0) / 4096.0;
                    ((x / (1.0 + (-x).exp())) * 4096.0).round().clamp(-32767.0, 32767.0) as i16
                })
                .collect(),
        )),
        Fill::Rope => ParamData::I32(Cow::Owned((0..n).map(|_| rng.range(-(1 << 24), 1 << 24) as i32).collect())),
    }
}

/// Every param instance of `program` filled by `fills`, seeded deterministically, as the reference
/// evaluator's map.
pub fn map_params(program: &TirProgramV1, fills: &[Fill], seed: u64) -> MapParams {
    let mut rng = Rng(seed);
    let mut map = MapParams::default();
    let layers = program.schedule.layers.len() as u16;
    for (j, d) in program.params.iter().enumerate() {
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
        let instances: Vec<Option<u16>> = if d.per_layer { (0..layers).map(Some).collect() } else { vec![None] };
        for l in instances {
            let data = fill(fills[j], n, &mut rng).slice().to_i128s();
            map.tensors.insert((j as u16, l), Tensor { dtype: d.dtype, shape: shape.clone(), data });
        }
    }
    map
}
