//! A Qwen2.5-1.5B-shaped decoder built with this crate's own builder (not the first
//! implementation's library): 28 layers, d = 1536, 12 query / 2 key-value heads of 128, FFN 8960,
//! vocabulary 151,936, window 2^18. A16-style integer lowering with the range-stating clamps of
//! §7, so that it passes the range analysis; the numbers are shapes, not a real checkpoint.
#![allow(dead_code)]

use misaka_palw_tir_ref2 as ref2;
use ref2::build::{ProgBuilder, fixed, ty};
use ref2::{DType, Dim, Prim, Program, Ref, Rounding};

pub struct Shape {
    pub layers: usize,
    pub d: u32,
    pub q_heads: u32,
    pub kv_heads: u32,
    pub head: u32,
    pub ffn: u32,
    pub vocab: u32,
    pub window: u32,
    pub commit_attention: bool,
}

pub const QWEN25_1_5B: Shape = Shape {
    layers: 28,
    d: 1536,
    q_heads: 12,
    kv_heads: 2,
    head: 128,
    ffn: 8960,
    vocab: 151_936,
    window: 1 << 18,
    commit_attention: true,
};

struct B<'a> {
    b: &'a mut ProgBuilder,
    blk: usize,
}

impl B<'_> {
    fn n(&mut self, prim: Prim, ins: &[Ref], out: ref2::TensorType, commit: bool) -> Ref {
        Ref::Node(self.b.node(self.blk, prim, ins, out, commit))
    }
    fn k(&mut self, dtype: DType, v: i128) -> Ref {
        Ref::Const(self.b.konst(dtype, &[1], &[v]))
    }
    /// Narrow an i64/i128 value to i16 by `2^s` (half away from zero) and a clamp.
    fn narrow(&mut self, x: Ref, shape: &[Dim], s: u32, commit: bool) -> Ref {
        let two = self.k(DType::I64, 1i128 << s);
        let q = self.n(Prim::Div { rule: Rounding::HalfAwayFromZero }, &[x, two], ty(DType::I64, shape), false);
        self.n(Prim::Clamp { lo: -32768, hi: 32767 }, &[q], ty(DType::I16, shape), commit)
    }
    /// RMSNorm of an i16 row `[d]` (A16 style): Σx², mean·2^24 / d, IntRsqrt, x·r, a gain.
    fn rms_norm(&mut self, x: Ref, d: u32, gain: Ref) -> Ref {
        let sq = self.n(Prim::Mul, &[x, x], fixed(DType::I32, &[d]), false);
        let ss = self.n(Prim::ReduceSum { axis: 0 }, &[sq], fixed(DType::I64, &[1]), false);
        let one = self.k(DType::I64, 1 << 24);
        let scaled = self.n(Prim::Mul, &[ss, one], fixed(DType::I128, &[1]), false);
        let n = self.k(DType::I64, d as i128);
        let mean = self.n(Prim::Div { rule: Rounding::Floor }, &[scaled, n], fixed(DType::I128, &[1]), false);
        let meanc = self.n(Prim::Clamp { lo: 1, hi: i64::MAX }, &[mean], fixed(DType::I64, &[1]), false);
        let r = self.n(Prim::IntRsqrt, &[meanc], fixed(DType::I64, &[1]), false);
        let xr = self.n(Prim::Mul, &[x, r], fixed(DType::I64, &[d]), false);
        let xn = self.narrow(xr, &[Dim::Fixed(d)], 24, false);
        let g = self.n(Prim::Mul, &[xn, gain], fixed(DType::I32, &[d]), false);
        self.narrow(g, &[Dim::Fixed(d)], 14, false)
    }
}

pub fn build(s: &Shape) -> Program {
    let hb = 1u32 << 18;
    let mut pb = ProgBuilder::new(hb, s.vocab);
    let emb = pb.param("embed", DType::I16, &[s.vocab, s.d], false);
    let final_gain = pb.param("final_norm", DType::I16, &[s.d], false);
    let lm = pb.param("lm_head", DType::I8, &[s.d, s.vocab], false);
    let g1 = pb.param("attn_norm", DType::I16, &[s.d], true);
    let wq = pb.param("wq", DType::I8, &[s.d, s.q_heads * s.head], true);
    let wk = pb.param("wk", DType::I8, &[s.d, s.kv_heads * s.head], true);
    let wv = pb.param("wv", DType::I8, &[s.d, s.kv_heads * s.head], true);
    let wo = pb.param("wo", DType::I8, &[s.q_heads * s.head, s.d], true);
    let g2 = pb.param("ffn_norm", DType::I16, &[s.d], true);
    let wg = pb.param("w_gate", DType::I8, &[s.d, s.ffn], true);
    let wu = pb.param("w_up", DType::I8, &[s.d, s.ffn], true);
    let wd = pb.param("w_down", DType::I8, &[s.ffn, s.d], true);
    let kh = pb.hist_state("k_cache", DType::I16, &[s.kv_heads, s.head], s.window, true);
    let vh = pb.hist_state("v_cache", DType::I16, &[s.kv_heads, s.head], s.window, true);
    let d = s.d;
    let (g, hd, kvh) = (s.q_heads / s.kv_heads, s.head, s.kv_heads);

    // pre: the embedding row.
    let pre = pb.block("pre", vec![]);
    let e = pb.node(pre, Prim::Gather { axis: 0, batch_dims: 0 }, &[Ref::Param(emb), Ref::Input(0)], fixed(DType::I16, &[d]), true);
    pb.carry_out(pre, &[e]);

    // the decoder layer
    let lay = pb.block("decoder_layer", vec![fixed(DType::I16, &[d])]);
    let carry_out = {
        let mut b = B { b: &mut pb, blk: lay };
        let x = Ref::CarryIn(0);
        let xn = b.rms_norm(x, d, Ref::Param(g1));
        let xr = b.n(Prim::Reshape, &[xn], fixed(DType::I16, &[1, d]), false);
        // q, k, v
        let q = b.n(Prim::MatMul, &[xr, Ref::Param(wq)], fixed(DType::I64, &[1, s.q_heads * hd]), false);
        let q16 = b.narrow(q, &[Dim::Fixed(1), Dim::Fixed(s.q_heads * hd)], 12, s.commit_attention);
        let q3 = b.n(Prim::Reshape, &[q16], fixed(DType::I16, &[kvh, g, hd]), false);
        let k = b.n(Prim::MatMul, &[xr, Ref::Param(wk)], fixed(DType::I64, &[1, kvh * hd]), false);
        let k16 = b.narrow(k, &[Dim::Fixed(1), Dim::Fixed(kvh * hd)], 12, false);
        let krow = b.n(Prim::Reshape, &[k16], fixed(DType::I16, &[kvh, hd]), true);
        let v = b.n(Prim::MatMul, &[xr, Ref::Param(wv)], fixed(DType::I64, &[1, kvh * hd]), false);
        let v16 = b.narrow(v, &[Dim::Fixed(1), Dim::Fixed(kvh * hd)], 12, false);
        let vrow = b.n(Prim::Reshape, &[v16], fixed(DType::I16, &[kvh, hd]), true);
        let kall = b.n(Prim::HistAppend { state: kh }, &[krow], ty(DType::I16, &[Dim::H, Dim::Fixed(kvh), Dim::Fixed(hd)]), false);
        let vall = b.n(Prim::HistAppend { state: vh }, &[vrow], ty(DType::I16, &[Dim::H, Dim::Fixed(kvh), Dim::Fixed(hd)]), false);
        // scores = q · Kᵀ per kv head, then the two-pass softmax over H
        let kt =
            b.n(Prim::Transpose { perm: vec![1, 2, 0] }, &[kall], ty(DType::I16, &[Dim::Fixed(kvh), Dim::Fixed(hd), Dim::H]), false);
        let sc = b.n(Prim::MatMul, &[q3, kt], ty(DType::I64, &[Dim::Fixed(kvh), Dim::Fixed(g), Dim::H]), false);
        let m = b.n(Prim::ReduceMax { axis: 2 }, &[sc], fixed(DType::I64, &[kvh, g, 1]), false);
        // The maximum, narrowed to a committable dtype (committed: the softmax's first pass).
        let m32 =
            b.n(Prim::Clamp { lo: i32::MIN as i64, hi: i32::MAX as i64 }, &[m], fixed(DType::I32, &[kvh, g, 1]), s.commit_attention);
        let dlt = b.n(Prim::Sub, &[sc, m32], ty(DType::I64, &[Dim::Fixed(kvh), Dim::Fixed(g), Dim::H]), false);
        let dc =
            b.n(Prim::Clamp { lo: i32::MIN as i64, hi: 0 }, &[dlt], ty(DType::I64, &[Dim::Fixed(kvh), Dim::Fixed(g), Dim::H]), false);
        let ex = b.n(Prim::IntExp, &[dc], ty(DType::I32, &[Dim::Fixed(kvh), Dim::Fixed(g), Dim::H]), false);
        let sum = b.n(Prim::ReduceSum { axis: 2 }, &[ex], fixed(DType::I64, &[kvh, g, 1]), false);
        let vt =
            b.n(Prim::Transpose { perm: vec![1, 0, 2] }, &[vall], ty(DType::I16, &[Dim::Fixed(kvh), Dim::H, Dim::Fixed(hd)]), false);
        let ctx = b.n(Prim::MatMul, &[ex, vt], fixed(DType::I64, &[kvh, g, hd]), false);
        let sum1 = b.n(Prim::Clamp { lo: 1, hi: i64::MAX }, &[sum], fixed(DType::I64, &[kvh, g, 1]), false);
        let att = b.n(Prim::Div { rule: Rounding::Floor }, &[ctx, sum1], fixed(DType::I64, &[kvh, g, hd]), false);
        let att16 = b.n(Prim::Clamp { lo: -32768, hi: 32767 }, &[att], fixed(DType::I16, &[kvh, g, hd]), s.commit_attention);
        let attr = b.n(Prim::Reshape, &[att16], fixed(DType::I16, &[1, s.q_heads * hd]), false);
        let o = b.n(Prim::MatMul, &[attr, Ref::Param(wo)], fixed(DType::I64, &[1, d]), false);
        let o16 = b.narrow(o, &[Dim::Fixed(1), Dim::Fixed(d)], 12, false);
        let or = b.n(Prim::Reshape, &[o16], fixed(DType::I16, &[d]), false);
        let h = b.n(Prim::Add, &[x, or], fixed(DType::I32, &[d]), false);
        let h16 = b.n(Prim::Clamp { lo: -32768, hi: 32767 }, &[h], fixed(DType::I16, &[d]), s.commit_attention);
        // MLP
        let hn = b.rms_norm(h16, d, Ref::Param(g2));
        let hr = b.n(Prim::Reshape, &[hn], fixed(DType::I16, &[1, d]), false);
        let gate = b.n(Prim::MatMul, &[hr, Ref::Param(wg)], fixed(DType::I64, &[1, s.ffn]), false);
        let up = b.n(Prim::MatMul, &[hr, Ref::Param(wu)], fixed(DType::I64, &[1, s.ffn]), false);
        let gate16 = b.narrow(gate, &[Dim::Fixed(1), Dim::Fixed(s.ffn)], 12, false);
        let act = b.n(Prim::Clamp { lo: 0, hi: 32767 }, &[gate16], fixed(DType::I16, &[1, s.ffn]), false);
        let up16 = b.narrow(up, &[Dim::Fixed(1), Dim::Fixed(s.ffn)], 12, false);
        let pr = b.n(Prim::Mul, &[act, up16], fixed(DType::I32, &[1, s.ffn]), false);
        let pr16 = b.narrow(pr, &[Dim::Fixed(1), Dim::Fixed(s.ffn)], 15, s.commit_attention);
        let dn = b.n(Prim::MatMul, &[pr16, Ref::Param(wd)], fixed(DType::I64, &[1, d]), false);
        let dn16 = b.narrow(dn, &[Dim::Fixed(1), Dim::Fixed(d)], 12, false);
        let dr = b.n(Prim::Reshape, &[dn16], fixed(DType::I16, &[d]), false);
        let y = b.n(Prim::Add, &[h16, dr], fixed(DType::I32, &[d]), false);
        let Ref::Node(out) = b.n(Prim::Clamp { lo: -32768, hi: 32767 }, &[y], fixed(DType::I16, &[d]), true) else { unreachable!() };
        out
    };
    pb.carry_out(lay, &[carry_out]);

    // post: the final norm and the LM head.
    let post = pb.block("post", vec![fixed(DType::I16, &[d])]);
    let logits = {
        let mut b = B { b: &mut pb, blk: post };
        let xn = b.rms_norm(Ref::CarryIn(0), d, Ref::Param(final_gain));
        let xr = b.n(Prim::Reshape, &[xn], fixed(DType::I16, &[1, d]), false);
        let l = b.n(Prim::MatMul, &[xr, Ref::Param(lm)], fixed(DType::I64, &[1, s.vocab]), false);
        let two = b.k(DType::I64, 1 << 8);
        let q = b.n(Prim::Div { rule: Rounding::HalfAwayFromZero }, &[l, two], fixed(DType::I64, &[1, s.vocab]), false);
        let Ref::Node(i) = b.n(Prim::Clamp { lo: i32::MIN as i64, hi: i32::MAX as i64 }, &[q], fixed(DType::I32, &[1, s.vocab]), true)
        else {
            unreachable!()
        };
        i
    };
    let layers = vec![lay; s.layers];
    pb.schedule(pre, &layers, post, logits);
    pb.finish()
}
