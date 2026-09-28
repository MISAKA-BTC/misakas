//! A small builder for writing programs by hand, and the first composite templates.
//!
//! The builder infers every node's output shape the way `validate` checks it, so a program built
//! here carries fully inferred `out` types. It PANICS on misuse (a shape that does not fit): it is
//! a construction API for tests, tools and lowerers, whose output is validated by
//! [`crate::validate::validate`] before anything runs it. The interpreter never panics.
//!
//! The composite templates ([`BlockBuilder::narrow_a16`], [`BlockBuilder::softmax_shifted`], …)
//! expand into primitives exactly as the library `tir_library_v1` will (Gate 2); where a template
//! reproduces a legacy kernel, its doc names the kernel, and `tests/legacy_mirror.rs` checks the
//! expansion against a verbatim transcription of that kernel.

use crate::arith::{K, ONE};
use crate::prim::{Cmp, Prim, Rounding};
use crate::program::*;
use crate::types::{DType, Dim, TensorType, broadcast_shapes};

pub struct ProgramBuilder {
    pub token_bound: u32,
    pub history_bound: u32,
    pub params: Vec<ParamDecl>,
    pub consts: Vec<ConstDecl>,
    pub states: Vec<StateDecl>,
    pub blocks: Vec<Block>,
}

fn fixed(shape: &[u32]) -> Vec<Dim> {
    shape.iter().map(|d| Dim::Fixed(*d)).collect()
}

impl ProgramBuilder {
    pub fn new(token_bound: u32, history_bound: u32) -> Self {
        Self { token_bound, history_bound, params: Vec::new(), consts: Vec::new(), states: Vec::new(), blocks: Vec::new() }
    }

    pub fn param(&mut self, name: &str, dtype: DType, shape: &[u32], per_layer: bool) -> Ref {
        assert!(!self.params.iter().any(|p| p.name == name), "param {name} declared twice");
        self.params.push(ParamDecl { name: name.to_string(), dtype, shape: shape.to_vec(), per_layer });
        Ref::Param((self.params.len() - 1) as u16)
    }

    /// A const; an identical earlier const is reused (normal form forbids duplicates).
    pub fn konst(&mut self, dtype: DType, shape: &[u32], values: &[i128]) -> Ref {
        assert_eq!(values.len(), shape.iter().map(|d| *d as usize).product::<usize>(), "const element count");
        let mut data = Vec::new();
        for v in values {
            assert!(dtype.contains(*v), "{v} is not a {}", dtype.name());
            dtype.encode_le(*v, &mut data);
        }
        let decl = ConstDecl { dtype, shape: shape.to_vec(), data };
        if let Some(i) = self.consts.iter().position(|c| *c == decl) {
            return Ref::Const(i as u16);
        }
        self.consts.push(decl);
        Ref::Const((self.consts.len() - 1) as u16)
    }

    /// A rank-0 const.
    pub fn scalar(&mut self, dtype: DType, v: i128) -> Ref {
        self.konst(dtype, &[], &[v])
    }

    pub fn fixed_state(&mut self, name: &str, dtype: DType, shape: &[u32], lo: i64, hi: i64, per_layer: bool) -> u16 {
        self.states.push(StateDecl {
            name: name.to_string(),
            kind: StateKind::Fixed { lo, hi },
            dtype,
            shape: shape.to_vec(),
            per_layer,
        });
        (self.states.len() - 1) as u16
    }

    pub fn hist_state(&mut self, name: &str, dtype: DType, row: &[u32], window: u32, per_layer: bool) -> u16 {
        self.states.push(StateDecl {
            name: name.to_string(),
            kind: StateKind::Hist { window },
            dtype,
            shape: row.to_vec(),
            per_layer,
        });
        (self.states.len() - 1) as u16
    }

    pub fn block(&mut self, name: &str, carry_in: Vec<TensorType>) -> BlockBuilder<'_> {
        BlockBuilder { pb: self, name: name.to_string(), carry_in, nodes: Vec::new(), window: None }
    }

    pub fn finish(self, pre: u8, layers: Vec<u8>, post: u8, logits: u16) -> TirProgramV1 {
        TirProgramV1 {
            version: TIR_PROGRAM_VERSION_V1,
            prim_set_id: [0u8; 64],
            token_bound: self.token_bound,
            history_bound: self.history_bound,
            params: self.params,
            consts: self.consts,
            states: self.states,
            blocks: self.blocks,
            schedule: Schedule { pre, layers, post },
            logits,
            logits_scheme_id: [0u8; 64],
        }
    }
}

pub struct BlockBuilder<'a> {
    pub pb: &'a mut ProgramBuilder,
    name: String,
    carry_in: Vec<TensorType>,
    nodes: Vec<Node>,
    window: Option<u32>,
}

impl<'a> BlockBuilder<'a> {
    /// The type of any operand.
    pub fn ty(&self, r: Ref) -> TensorType {
        match r {
            Ref::Node(i) => self.nodes[i as usize].out.clone(),
            Ref::CarryIn(k) => self.carry_in[k as usize].clone(),
            Ref::Param(j) => {
                let p = &self.pb.params[j as usize];
                TensorType::new(p.dtype, fixed(&p.shape))
            }
            Ref::Const(j) => {
                let c = &self.pb.consts[j as usize];
                TensorType::new(c.dtype, fixed(&c.shape))
            }
            Ref::State(j) => {
                let s = &self.pb.states[j as usize];
                TensorType::new(s.dtype, fixed(&s.shape))
            }
            Ref::Input(_) => TensorType::scalar(DType::Idx),
        }
    }

    pub fn shape(&self, r: Ref) -> Vec<Dim> {
        self.ty(r).shape
    }

    pub fn push(&mut self, prim: Prim, inputs: Vec<Ref>, out: TensorType) -> Ref {
        assert!(self.nodes.len() < MAX_NODES_PER_BLOCK, "block {} exceeds {MAX_NODES_PER_BLOCK} nodes", self.name);
        self.nodes.push(Node { prim, inputs, out, commit: false });
        Ref::Node((self.nodes.len() - 1) as u16)
    }

    /// Mark a node as a commit point.
    pub fn commit(&mut self, r: Ref) -> Ref {
        if let Ref::Node(i) = r {
            self.nodes[i as usize].commit = true;
        }
        r
    }

    pub fn finish(self, carry_out: &[Ref]) -> u8 {
        let carry_out = carry_out
            .iter()
            .map(|r| match r {
                Ref::Node(i) => *i,
                other => panic!("carry-out must be a node, got {other:?}"),
            })
            .collect::<Vec<_>>();
        let mut nodes = self.nodes;
        for i in &carry_out {
            nodes[*i as usize].commit = true;
        }
        self.pb.blocks.push(Block { name: self.name, carry_in: self.carry_in, nodes, carry_out });
        (self.pb.blocks.len() - 1) as u8
    }

    // ---- primitives, with inference --------------------------------------------------------

    pub fn reshape(&mut self, x: Ref, shape: &[Dim]) -> Ref {
        let dt = self.ty(x).dtype;
        self.push(Prim::Reshape, vec![x], TensorType::new(dt, shape.to_vec()))
    }

    pub fn reshape_fixed(&mut self, x: Ref, shape: &[u32]) -> Ref {
        self.reshape(x, &fixed(shape))
    }

    pub fn transpose(&mut self, x: Ref, perm: &[u8]) -> Ref {
        let t = self.ty(x);
        let s = perm.iter().map(|p| t.shape[*p as usize]).collect();
        self.push(Prim::Transpose { perm: perm.to_vec() }, vec![x], TensorType::new(t.dtype, s))
    }

    pub fn slice(&mut self, x: Ref, axis: usize, start: u32, len: u32) -> Ref {
        let mut t = self.ty(x);
        t.shape[axis] = Dim::Fixed(len);
        self.push(Prim::Slice { axis: axis as u8, start }, vec![x], t)
    }

    pub fn concat(&mut self, xs: &[Ref], axis: usize) -> Ref {
        let mut t = self.ty(xs[0]);
        let total: u32 = xs
            .iter()
            .map(|x| match self.ty(*x).shape[axis] {
                Dim::Fixed(n) => n,
                Dim::H => panic!("concat along H"),
            })
            .sum();
        t.shape[axis] = Dim::Fixed(total);
        self.push(Prim::Concat { axis: axis as u8 }, xs.to_vec(), t)
    }

    pub fn broadcast(&mut self, x: Ref, shape: &[Dim]) -> Ref {
        let dt = self.ty(x).dtype;
        self.push(Prim::Broadcast, vec![x], TensorType::new(dt, shape.to_vec()))
    }

    pub fn iota(&mut self, dtype: DType, shape: &[Dim], axis: usize, start: i64, step: i64) -> Ref {
        self.push(Prim::Iota { axis: axis as u8, start, step }, vec![], TensorType::new(dtype, shape.to_vec()))
    }

    pub fn gather(&mut self, data: Ref, idx: Ref, axis: usize, batch_dims: usize) -> Ref {
        let d = self.ty(data);
        let ix = self.ty(idx);
        let mut s: Vec<Dim> = d.shape[..axis].to_vec();
        s.extend_from_slice(&ix.shape[batch_dims..]);
        s.extend_from_slice(&d.shape[axis + 1..]);
        self.push(Prim::Gather { axis: axis as u8, batch_dims: batch_dims as u8 }, vec![data, idx], TensorType::new(d.dtype, s))
    }

    pub fn cast(&mut self, x: Ref, dtype: DType) -> Ref {
        let s = self.shape(x);
        self.push(Prim::Cast, vec![x], TensorType::new(dtype, s))
    }

    fn binary(&mut self, prim: Prim, a: Ref, b: Ref, dtype: DType) -> Ref {
        let s =
            broadcast_shapes(&self.shape(a), &self.shape(b)).unwrap_or_else(|| panic!("{} operands do not broadcast", prim.name()));
        self.push(prim, vec![a, b], TensorType::new(dtype, s))
    }

    pub fn add(&mut self, a: Ref, b: Ref, dtype: DType) -> Ref {
        self.binary(Prim::Add, a, b, dtype)
    }
    pub fn sub(&mut self, a: Ref, b: Ref, dtype: DType) -> Ref {
        self.binary(Prim::Sub, a, b, dtype)
    }
    pub fn mul(&mut self, a: Ref, b: Ref, dtype: DType) -> Ref {
        self.binary(Prim::Mul, a, b, dtype)
    }
    pub fn div(&mut self, x: Ref, d: Ref, rule: Rounding, dtype: DType) -> Ref {
        self.binary(Prim::Div { rule }, x, d, dtype)
    }
    pub fn compare(&mut self, a: Ref, b: Ref, cmp: Cmp) -> Ref {
        self.binary(Prim::Compare { cmp }, a, b, DType::I8)
    }

    pub fn select(&mut self, c: Ref, a: Ref, b: Ref, dtype: DType) -> Ref {
        let s = broadcast_shapes(&self.shape(c), &self.shape(a))
            .and_then(|s| broadcast_shapes(&s, &self.shape(b)))
            .expect("select broadcasts");
        self.push(Prim::Select, vec![c, a, b], TensorType::new(dtype, s))
    }

    pub fn matmul(&mut self, a: Ref, b: Ref, dtype: DType) -> Ref {
        let (x, y) = (self.shape(a), self.shape(b));
        assert_eq!(x[x.len() - 1], y[y.len() - 2], "matmul contraction");
        let mut s = broadcast_shapes(&x[..x.len() - 2], &y[..y.len() - 2]).expect("matmul batch broadcasts");
        s.push(x[x.len() - 2]);
        s.push(y[y.len() - 1]);
        self.push(Prim::MatMul, vec![a, b], TensorType::new(dtype, s))
    }

    pub fn reduce_sum(&mut self, x: Ref, axis: usize, dtype: DType) -> Ref {
        let mut s = self.shape(x);
        s[axis] = Dim::Fixed(1);
        self.push(Prim::ReduceSum { axis: axis as u8 }, vec![x], TensorType::new(dtype, s))
    }

    pub fn reduce_max(&mut self, x: Ref, axis: usize) -> Ref {
        let mut t = self.ty(x);
        t.shape[axis] = Dim::Fixed(1);
        self.push(Prim::ReduceMax { axis: axis as u8 }, vec![x], t)
    }

    pub fn clamp(&mut self, x: Ref, lo: i64, hi: i64, dtype: DType) -> Ref {
        let s = self.shape(x);
        self.push(Prim::Clamp { lo, hi }, vec![x], TensorType::new(dtype, s))
    }

    fn unary(&mut self, prim: Prim, x: Ref, dtype: DType) -> Ref {
        let s = self.shape(x);
        self.push(prim, vec![x], TensorType::new(dtype, s))
    }
    pub fn log2_floor(&mut self, x: Ref, dtype: DType) -> Ref {
        self.unary(Prim::Log2Floor, x, dtype)
    }
    pub fn int_exp(&mut self, x: Ref) -> Ref {
        self.unary(Prim::IntExp, x, DType::I32)
    }
    pub fn int_rsqrt(&mut self, x: Ref) -> Ref {
        self.unary(Prim::IntRsqrt, x, DType::I64)
    }
    pub fn int_ln(&mut self, x: Ref) -> Ref {
        self.unary(Prim::IntLn, x, DType::I32)
    }

    /// `TopK` is always a commit point (PALW-TIR-11).
    pub fn topk(&mut self, x: Ref, axis: usize, k: u32) -> Ref {
        let mut s = self.shape(x);
        s[axis] = Dim::Fixed(k);
        let r = self.push(Prim::TopK { axis: axis as u8, k }, vec![x], TensorType::new(DType::Idx, s));
        self.commit(r)
    }

    pub fn state_write(&mut self, state: u16, x: Ref) -> Ref {
        let st = &self.pb.states[state as usize];
        let t = TensorType::new(st.dtype, fixed(&st.shape));
        self.push(Prim::StateWrite { state }, vec![x], t)
    }

    /// Append a committed row; returns the window `[H, ..row]`.
    pub fn hist_append(&mut self, state: u16, row: Ref) -> Ref {
        let st = &self.pb.states[state as usize];
        let StateKind::Hist { window } = st.kind else { panic!("HistAppend on a Fixed state") };
        assert!(self.window.is_none_or(|w| w == window), "a block's histories share one window");
        self.window = Some(window);
        let mut s = vec![Dim::H];
        s.extend(fixed(&st.shape));
        let t = TensorType::new(st.dtype, s);
        self.commit(row);
        self.push(Prim::HistAppend { state }, vec![row], t)
    }

    // ---- small helpers -----------------------------------------------------------------------

    pub fn c(&mut self, dtype: DType, v: i128) -> Ref {
        self.pb.scalar(dtype, v)
    }

    /// `round_rule(x / 2^n)`: the shifts of 04a as `Div` by a constant power of two.
    pub fn shr(&mut self, x: Ref, n: u32, rule: Rounding, dtype: DType) -> Ref {
        let d = if n <= 62 { self.c(DType::I64, 1i128 << n) } else { self.c(DType::I128, 1i128 << n) };
        self.div(x, d, rule, dtype)
    }

    /// `2^s` for a tensor of shift amounts known to lie in `[0, 62]`: a gather from the pinned
    /// table `[2^0, …, 2^62]` (the IR has no shift primitive; a variable shift is a gather and a
    /// multiply or a divide).
    pub fn pow2_of(&mut self, s: Ref) -> Ref {
        let table: Vec<i128> = (0..=62).map(|i| 1i128 << i).collect();
        let t = self.pb.konst(DType::I64, &[63], &table);
        let s = self.clamp(s, 0, 62, DType::Idx);
        self.gather(t, s, 0, 0)
    }

    // ---- composite templates (seed of tir_library_v1) -----------------------------------------

    /// The A16 narrowing (`palw_base0_a16::a16_scale_round` then `saturating_add(zero)` then a
    /// clamp): `clamp_[lo,hi]( sat64( sat64(HAFZ(x·m / 2^s)) + z ) )`, written as
    /// `Mul→i128, Div(HAFZ)→i128, Clamp→i64, Add→i128, Clamp→out`. `pow2_s` is the divisor `2^s`
    /// (a const, or [`Self::pow2_of`] of a shift tensor).
    #[allow(clippy::too_many_arguments)]
    pub fn narrow_a16(&mut self, x: Ref, m: Ref, pow2_s: Ref, z: Ref, lo: i64, hi: i64, dtype: DType) -> Ref {
        let p = self.mul(x, m, DType::I128);
        let q = self.div(p, pow2_s, Rounding::HalfAwayFromZero, DType::I128);
        let r = self.clamp(q, i64::MIN, i64::MAX, DType::I64);
        let t = self.add(r, z, DType::I128);
        self.clamp(t, lo, hi, dtype)
    }

    /// BASE-0 op 2 (`palw_base0::requantize_with_zero`):
    /// `clamp_[-128,127]( sat32( RSR( SRDHM(acc, m), min(s, 31) ) + z ) )` with SRDHM as
    /// `sat32(HalfUp(acc·m / 2^31))`.
    pub fn requantize_base0(&mut self, acc: Ref, m: Ref, shift: u32, z: Ref) -> Ref {
        let p = self.mul(acc, m, DType::I64);
        let h = self.shr(p, 31, Rounding::HalfUp, DType::I64);
        let srdhm = self.clamp(h, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let r = self.shr(srdhm, shift.min(31), Rounding::HalfAwayFromZero, DType::I32);
        let t = self.add(r, z, DType::I64);
        self.clamp(t, -128, 127, DType::I8)
    }

    /// BASE-0 op 9 (`palw_base0::rescale_q`): `sat32( RSR64(acc·m, min(s, 62)) )`.
    pub fn rescale_base0(&mut self, acc: Ref, m: Ref, shift: u32) -> Ref {
        let p = self.mul(acc, m, DType::I64);
        let r = self.shr(p, shift.min(62), Rounding::HalfAwayFromZero, DType::I64);
        self.clamp(r, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// `IntRecip` of 04a F2, composed: `(IntRsqrt(v)² ) >> 24` (`palw_base0::int_recip`).
    pub fn int_recip(&mut self, v: Ref) -> Ref {
        let r = self.int_rsqrt(v);
        let rr = self.mul(r, r, DType::I128);
        self.shr(rr, K, Rounding::Floor, DType::I64)
    }

    /// `palw_base0_ops::int_sigmoid` on Q24 `i32` values.
    pub fn int_sigmoid(&mut self, x: Ref) -> Ref {
        let zero = self.c(DType::I32, 0);
        let pos = self.compare(x, zero, Cmp::Gt);
        let neg = self.sub(zero, x, DType::I64);
        let neg_abs = self.select(pos, neg, x, DType::I64);
        let e = self.int_exp(neg_abs);
        let one = self.c(DType::I32, ONE);
        let den = self.add(e, one, DType::I64);
        let recip = self.int_recip(den);
        let le = self.compare(x, zero, Cmp::Le);
        let num = self.select(le, e, one, DType::I64);
        let prod = self.mul(num, recip, DType::I64);
        self.shr(prod, K, Rounding::Floor, DType::I32)
    }

    /// `palw_base0_ops::silu` (BASE-0 op 6, `q36/silu`): `(x · IntSigmoid(x)) >> 24`, Q24 in and out.
    pub fn silu(&mut self, x: Ref) -> Ref {
        let s = self.int_sigmoid(x);
        let p = self.mul(x, s, DType::I64);
        let q = self.shr(p, K, Rounding::Floor, DType::I64);
        self.clamp(q, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// `palw_base0_ops::softmax_shifted` along the LAST axis of an `i32`/`i16` row (op 5W;
    /// `up_bits = 0` is op 5): the maximum first, the difference clamped at `i32::MIN >> up` BEFORE
    /// the widening, `IntExp`, an exact sum, `IntRecip`, `(e·recip) >> 24`. Returns Q24 `i32`.
    ///
    /// Legacy's uniform fallback for a non-positive sum is dead code — the row maximum contributes
    /// `IntExp(0) > 0` — and has no node. The final clamp to `[0, 2^25]` never fires for the same
    /// reason; it exists so range analysis can type the probabilities.
    pub fn softmax_shifted(&mut self, x: Ref, up_bits: u32) -> Ref {
        let axis = self.shape(x).len() - 1;
        let up = up_bits.min(62);
        let max = self.reduce_max(x, axis);
        let diff = self.sub(x, max, DType::I64);
        let floor = (i32::MIN as i64) >> up;
        let d = self.clamp(diff, floor, 0, DType::I64);
        let scale = self.c(DType::I64, 1i128 << up);
        let w = self.mul(d, scale, DType::I64);
        let arg = self.clamp(w, i32::MIN as i64, 0, DType::I32);
        let e = self.int_exp(arg);
        let sum = self.reduce_sum(e, axis, DType::I64);
        let recip = self.int_recip(sum);
        let p = self.mul(e, recip, DType::I128);
        let q = self.shr(p, K, Rounding::Floor, DType::I64);
        self.clamp(q, 0, 1 << 25, DType::I32)
    }

    /// `palw_base0_a16::a16_rms_norm` along the last axis: the unit row in Q24 from A16 codes.
    /// `mean = floor((Σx² · 2^24) / n)` in i128, `r = IntRsqrt(clamp(mean) + eps)`, `clamp32(x·r)`.
    pub fn rms_norm_a16(&mut self, x: Ref, eps_q: i64) -> Ref {
        let s = self.shape(x);
        let axis = s.len() - 1;
        let Dim::Fixed(n) = s[axis] else { panic!("norm over H") };
        let sq = self.mul(x, x, DType::I64);
        let sum = self.reduce_sum(sq, axis, DType::I64);
        let one = self.c(DType::I64, ONE);
        let scaled = self.mul(sum, one, DType::I128);
        let nn = self.c(DType::I64, n as i128);
        let mean = self.div(scaled, nn, Rounding::Floor, DType::I128);
        let mean = self.clamp(mean, 0, i64::MAX, DType::I64);
        let eps = self.c(DType::I64, eps_q as i128);
        let v = self.add(mean, eps, DType::I64);
        let r = self.int_rsqrt(v);
        let y = self.mul(x, r, DType::I128);
        self.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// RoPE on adjacent pairs `(2p, 2p+1)` of the last axis (`palw_base0_a16::a16_rope`):
    /// `(a·c − b·s) >> 24`, `(a·s + b·c) >> 24` (floor), clamped to `[lo, hi]`. `cos`/`sin` are
    /// Q24 rows of `last/2` entries, broadcast over the leading axes.
    pub fn rope_pairs(&mut self, x: Ref, cos: Ref, sin: Ref, lo: i64, hi: i64, dtype: DType) -> Ref {
        let s = self.shape(x);
        let r = s.len();
        let Dim::Fixed(d) = s[r - 1] else { panic!("rope over H") };
        let mut pairs_shape = s.clone();
        pairs_shape[r - 1] = Dim::Fixed(d / 2);
        pairs_shape.push(Dim::Fixed(2));
        let xp = self.reshape(x, &pairs_shape);
        let a = self.slice(xp, r, 0, 1);
        let b = self.slice(xp, r, 1, 1);
        let mut half = s.clone();
        half[r - 1] = Dim::Fixed(d / 2);
        let a = self.reshape(a, &half);
        let b = self.reshape(b, &half);
        let ac = self.mul(a, cos, DType::I64);
        let bs = self.mul(b, sin, DType::I64);
        let as_ = self.mul(a, sin, DType::I64);
        let bc = self.mul(b, cos, DType::I64);
        let re = self.sub(ac, bs, DType::I64);
        let im = self.add(as_, bc, DType::I64);
        let re = self.shr(re, K, Rounding::Floor, DType::I64);
        let im = self.shr(im, K, Rounding::Floor, DType::I64);
        let re = self.clamp(re, lo, hi, dtype);
        let im = self.clamp(im, lo, hi, dtype);
        let mut one = half.clone();
        one.push(Dim::Fixed(1));
        let re = self.reshape(re, &one);
        let im = self.reshape(im, &one);
        let both = self.concat(&[re, im], r);
        self.reshape(both, &s)
    }

    /// `palw_qwen36_ops::q36_l2_norm` along the last axis: A16 codes in, Q15 codes out. The sum's
    /// exponent is taken out BEFORE `IntRsqrt` (`Log2Floor`) and applied to the product; a zero row
    /// stays zero.
    pub fn l2_norm_q15(&mut self, x: Ref) -> Ref {
        let s = self.shape(x);
        let axis = s.len() - 1;
        let sq = self.mul(x, x, DType::I64);
        let sum = self.reduce_sum(sq, axis, DType::I64);
        let bit = self.log2_floor(sum, DType::I32);
        let two = self.c(DType::I32, 2);
        let e = self.div(bit, two, Rounding::Floor, DType::I32);
        let two_e = self.mul(e, two, DType::I32);
        let k = self.c(DType::I32, K as i128);
        let sh = self.sub(two_e, k, DType::I32);
        let zero = self.c(DType::I32, 0);
        let sh_nonneg = self.compare(sh, zero, Cmp::Ge);
        let right_amount = self.pow2_of(sh);
        let right = self.div(sum, right_amount, Rounding::Floor, DType::I128);
        let neg_sh = self.sub(zero, sh, DType::I32);
        let left_amount = self.pow2_of(neg_sh);
        let left = self.mul(sum, left_amount, DType::I128);
        let m = self.select(sh_nonneg, right, left, DType::I128);
        let m = self.clamp(m, 0, i64::MAX, DType::I64);
        let rs = self.int_rsqrt(m);
        let off = self.c(DType::I32, (K - 15) as i128);
        let shift = self.add(e, off, DType::I32);
        let prod = self.mul(x, rs, DType::I128);
        let den = self.pow2_of(shift);
        let q = self.div(prod, den, Rounding::Floor, DType::I128);
        let y = self.clamp(q, -32767, 32767, DType::I16);
        let empty = self.compare(sum, zero, Cmp::Le);
        let z16 = self.c(DType::I16, 0);
        self.select(empty, z16, y, DType::I16)
    }

    /// `palw_qwen36_ops::q36_softplus`: `ln(1 + e^x)` split at the sign so `IntExp` only sees
    /// `−|x|`: `x ≤ 0 → IntLn(ONE + e)`, `x > 0 → x + IntLn(ONE + e)`, `e = IntExp(−|x|)`. Q24.
    pub fn softplus_q36(&mut self, x: Ref) -> Ref {
        let zero = self.c(DType::I32, 0);
        let pos = self.compare(x, zero, Cmp::Gt);
        let neg = self.sub(zero, x, DType::I64);
        let neg_abs = self.select(pos, neg, x, DType::I64);
        let e = self.int_exp(neg_abs);
        let one = self.c(DType::I32, ONE);
        let arg = self.add(e, one, DType::I64);
        let tail = self.int_ln(arg);
        let sum = self.add(x, tail, DType::I64);
        let le = self.compare(x, zero, Cmp::Le);
        self.select(le, tail, sum, DType::I64)
    }

    /// `palw_qwen36_ops::q36_exp_refined` for a Q24 `x ≤ 0` (`i32`): one Newton step of the
    /// frozen `IntExp` against `IntLn`, `y ← y + (y·clamp(x − ln y, ±ONE/4)) >> 24`, clamped to
    /// `[0, ONE]`; 0 where `IntExp` is 0.
    pub fn exp_refined_q36(&mut self, x: Ref) -> Ref {
        let y0 = self.int_exp(x);
        let ln_y = self.int_ln(y0);
        let d = self.sub(x, ln_y, DType::I64);
        let corr = self.clamp(d, -(ONE as i64 / 4), ONE as i64 / 4, DType::I32);
        let prod = self.mul(y0, corr, DType::I64);
        let step = self.shr(prod, K, Rounding::Floor, DType::I64);
        let adj = self.add(y0, step, DType::I64);
        let adj = self.clamp(adj, 0, ONE as i64, DType::I32);
        let zero = self.c(DType::I32, 0);
        let dead = self.compare(y0, zero, Cmp::Le);
        self.select(dead, zero, adj, DType::I32)
    }

    /// `palw_qwen36_ops::q36_decay`: `exp(−c · softplus(dt))` for a registered `c ≥ 0` (Q24,
    /// `c ≤ 0` gives `ONE`): `arg = clamp((c·softplus(dt)) >> 24, 0, 2^31)`, then the refined
    /// exponential of `−arg`, clamped to `[0, ONE]`.
    pub fn decay_q36(&mut self, dt: Ref, c: Ref) -> Ref {
        let sp = self.softplus_q36(dt);
        let p = self.mul(c, sp, DType::I128);
        let a = self.shr(p, K, Rounding::Floor, DType::I128);
        let a = self.clamp(a, 0, 1i64 << 31, DType::I64);
        let zero = self.c(DType::I32, 0);
        let neg = self.sub(zero, a, DType::I64);
        let neg = self.clamp(neg, i32::MIN as i64, 0, DType::I32);
        let y = self.exp_refined_q36(neg);
        let one = self.c(DType::I32, ONE);
        let off = self.compare(c, zero, Cmp::Le);
        self.select(off, one, y, DType::I32)
    }

    /// `palw_qwen36_ops::q36_gdn_step`, vectorised over heads: one position of the gated delta
    /// rule for `Fixed` state `state` (`[heads, d_v, d_k]`, range `±(2^31 − 1)`).
    ///
    /// `k`, `q`: `[heads, d_k]` unit codes (already mapped to the value heads — the head mapping is
    /// an explicit Reshape/Broadcast, not part of the rule); `v`: `[heads, d_v]` codes; `decay`,
    /// `beta`: `[heads]` Q24 in `[0, ONE]`. The narrowings are per-head params: `(m, pow2_s, z)` for
    /// the read, the delta and the output, and `write_shift` (`i32`, left if ≥ 0). Returns the
    /// output `[heads, d_v]` as `i32`; the state write is part of the expansion.
    #[allow(clippy::too_many_arguments)]
    pub fn gdn_step_q36(
        &mut self,
        state: u16,
        k: Ref,
        v: Ref,
        q: Ref,
        decay: Ref,
        beta: Ref,
        read: (Ref, Ref, Ref),
        delta: (Ref, Ref, Ref),
        write_shift: Ref,
        out: (Ref, Ref, Ref),
    ) -> Ref {
        let smax = i32::MAX as i64;
        let s_shape = self.ty(Ref::State(state)).shape;
        let (Dim::Fixed(h), Dim::Fixed(dv), Dim::Fixed(dk)) = (s_shape[0], s_shape[1], s_shape[2]) else { panic!("static state") };
        let col = |b: &mut Self, x: Ref, n: u32| b.reshape_fixed(x, &[h, n, 1]);
        // 1. The gate: S1 = clamp(RSR(S · decay, 24), ±(2^31 − 1)).
        let dec = b_reshape3(self, decay, h);
        let sd = self.mul(Ref::State(state), dec, DType::I64);
        let sd = self.shr(sd, K, Rounding::HalfAwayFromZero, DType::I64);
        let s1 = self.clamp(sd, -smax, smax, DType::I32);
        // 2. w = narrow_read(S1 k) — wide, the i64 rail.
        let kc = col(self, k, dk);
        let acc = self.matmul(s1, kc, DType::I64);
        let rm = b_reshape3(self, read.0, h);
        let rs = b_reshape3(self, read.1, h);
        let rz = b_reshape3(self, read.2, h);
        let w = self.narrow_a16(acc, rm, rs, rz, i64::MIN, i64::MAX, DType::I64);
        // 3. u = clamp(narrow_delta(RSR(sat64(sat64(v − w) · beta), 24)), ±(2^24 − 1)).
        let vc = col(self, v, dv);
        let diff = self.sub(vc, w, DType::I128);
        let diff = self.clamp(diff, i64::MIN, i64::MAX, DType::I64);
        let bt = b_reshape3(self, beta, h);
        let db = self.mul(diff, bt, DType::I128);
        let db = self.clamp(db, i64::MIN, i64::MAX, DType::I64);
        let scaled = self.shr(db, K, Rounding::HalfAwayFromZero, DType::I64);
        let dm = b_reshape3(self, delta.0, h);
        let ds = b_reshape3(self, delta.1, h);
        let dz = b_reshape3(self, delta.2, h);
        let u = self.narrow_a16(scaled, dm, ds, dz, -((1 << 24) - 1), (1 << 24) - 1, DType::I32);
        // 4. The rank-one write: S2 = clamp(S1 + write(u ⊗ k), ±(2^31 − 1)).
        let kr = self.reshape_fixed(k, &[h, 1, dk]);
        let prod = self.mul(u, kr, DType::I64);
        let ws = b_reshape3(self, write_shift, h);
        let zero = self.c(DType::I32, 0);
        let left_on = self.compare(ws, zero, Cmp::Ge);
        let lp = self.clamp(ws, 0, 20, DType::I32);
        let lp = self.pow2_of(lp);
        let left = self.mul(prod, lp, DType::I128);
        let left = self.clamp(left, i64::MIN, i64::MAX, DType::I64);
        let nws = self.sub(zero, ws, DType::I32);
        let rp = self.clamp(nws, 0, 62, DType::I32);
        let rp = self.pow2_of(rp);
        let right = self.div(prod, rp, Rounding::HalfAwayFromZero, DType::I64);
        let write = self.select(left_on, left, right, DType::I64);
        let s2 = self.add(s1, write, DType::I128);
        let s2 = self.state_write(state, s2);
        // 5. o = narrow_out(S2 q), wide.
        let qc = col(self, q, dk);
        let acc = self.matmul(s2, qc, DType::I64);
        let om = b_reshape3(self, out.0, h);
        let os = b_reshape3(self, out.1, h);
        let oz = b_reshape3(self, out.2, h);
        let o = self.narrow_a16(acc, om, os, oz, i32::MIN as i64, i32::MAX as i64, DType::I32);
        self.reshape_fixed(o, &[h, dv])
    }

    /// `2^s` as `i128` for shift amounts known to lie in `[0, max]` (`max ≤ 126`): a gather from the
    /// pinned table `[2^0, …, 2^126]`.
    pub fn pow2_128_of(&mut self, s: Ref, max: u32) -> Ref {
        assert!(max <= 126);
        let table: Vec<i128> = (0..=126).map(|i| 1i128 << i).collect();
        let t = self.pb.konst(DType::I128, &[127], &table);
        let s = self.clamp(s, 0, max as i64, DType::Idx);
        self.gather(t, s, 0, 0)
    }

    /// `palw_qwen36_ops::q36_rms_norm_wide` along the last axis: the RMS norm of a WIDE `i32` row,
    /// with `eps = eps_zero · 2^min(eps_shift, 96)` at the caller's scale (`eps_zero ≥ 0`) and the
    /// mean's exponent taken out before `IntRsqrt` — `Log2Floor` finds the even shift that lands
    /// the mean in `[2^24, 2^26)`, and the product is shifted back. A zero mean gives a zero row.
    pub fn rms_norm_wide_q36(&mut self, x: Ref, eps_zero: Ref, eps_shift: Ref) -> Ref {
        let sh = self.shape(x);
        let axis = sh.len() - 1;
        let Dim::Fixed(n) = sh[axis] else { panic!("norm over H") };
        let sq = self.mul(x, x, DType::I64);
        let sum = self.reduce_sum(sq, axis, DType::I128);
        let one = self.c(DType::I64, ONE);
        let scaled = self.mul(sum, one, DType::I128);
        let nn = self.c(DType::I64, n as i128);
        let mean0 = self.div(scaled, nn, Rounding::Floor, DType::I128);
        let ez = self.clamp(eps_zero, 0, i64::MAX, DType::I64);
        let es = self.pow2_128_of(eps_shift, 96);
        let eps = self.mul(ez, es, DType::I128);
        let mean = self.add(mean0, eps, DType::I128);
        let bit = self.log2_floor(mean, DType::I32);
        let k = self.c(DType::I32, K as i128);
        let t = self.sub(bit, k, DType::I32);
        let two = self.c(DType::I32, 2);
        let h = self.div(t, two, Rounding::Floor, DType::I32);
        let two_h = self.mul(h, two, DType::I32);
        let zero = self.c(DType::I32, 0);
        let rp = self.pow2_128_of(two_h, 126);
        let right = self.div(mean, rp, Rounding::Floor, DType::I128);
        let neg2h = self.sub(zero, two_h, DType::I32);
        let lp = self.pow2_128_of(neg2h, 24);
        let small = self.clamp(mean, 0, ONE as i64, DType::I64);
        let left = self.mul(small, lp, DType::I128);
        let ge = self.compare(two_h, zero, Cmp::Ge);
        let m = self.select(ge, right, left, DType::I128);
        let m = self.clamp(m, 0, i64::MAX, DType::I64);
        let r = self.int_rsqrt(m);
        let prod = self.mul(x, r, DType::I128);
        let dp = self.pow2_128_of(h, 126);
        let rshift = self.div(prod, dp, Rounding::Floor, DType::I128);
        let negh = self.sub(zero, h, DType::I32);
        let up = self.pow2_128_of(negh, 12);
        let lshift = self.mul(prod, up, DType::I128);
        let hge = self.compare(h, zero, Cmp::Ge);
        let y = self.select(hge, rshift, lshift, DType::I128);
        let y = self.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let empty = self.compare(mean, zero, Cmp::Le);
        self.select(empty, zero, y, DType::I32)
    }

    /// **LayerNorm without a lossy division** (corpus §6.2.7): `c = n·x − Σx` is exact — it is
    /// `n·(x − μ)` with no rounding — and `LayerNorm(x) = RMSNorm(c)` with `eps' = n²·eps`, because
    /// `Σc² = n³·Var(x)`. The only divisions left are the RMS mean's `÷ n` and `IntRsqrt`. `x` is a
    /// row of codes (`n·x` must fit `i32`); the wide RMSNorm takes the exponent out, so a quiet row
    /// keeps its precision. Returns the unit row in Q24 (`i32`); gain and bias are the caller's
    /// narrowing.
    pub fn layer_norm_exact(&mut self, x: Ref, eps_zero: Ref, eps_shift: Ref) -> Ref {
        let sh = self.shape(x);
        let axis = sh.len() - 1;
        let Dim::Fixed(n) = sh[axis] else { panic!("norm over H") };
        let nn = self.c(DType::I64, n as i128);
        let nx = self.mul(x, nn, DType::I64);
        let s = self.reduce_sum(x, axis, DType::I64);
        let c = self.sub(nx, s, DType::I64);
        let c = self.cast(c, DType::I32);
        self.rms_norm_wide_q36(c, eps_zero, eps_shift)
    }

    /// `palw_qwen36_ops::q36_router_topk` over the last axis of a logit row: `softmax_shifted`,
    /// `TopK` (committed; lowest index on ties, index order), the kept probabilities renormalised
    /// through `IntRecip`. Returns `(indices [k], weights [k] Q24)`. Legacy's uniform fallback for
    /// a zero kept sum is dead — the row maximum is always kept and its probability is positive —
    /// and has no node.
    pub fn router_topk_q36(&mut self, logits: Ref, k: u32, up_bits: u32) -> (Ref, Ref) {
        let axis = self.shape(logits).len() - 1;
        let probs = self.softmax_shifted(logits, up_bits);
        let idx = self.topk(probs, axis, k);
        let kept = self.gather(probs, idx, axis, axis);
        let sum = self.reduce_sum(kept, axis, DType::I64);
        let recip = self.int_recip(sum);
        let p = self.mul(kept, recip, DType::I128);
        let w = self.shr(p, K, Rounding::Floor, DType::I64);
        let w = self.clamp(w, 0, 1 << 25, DType::I32);
        (idx, w)
    }

    /// `palw_qwen36_ops::q36_moe_combine`: `Σ_e w_e · y_e` in ONE exact accumulator (a `MatMul`
    /// of the weights `[k]` against the expert rows `[k, width]`), narrowed once.
    #[allow(clippy::too_many_arguments)]
    pub fn moe_combine_q36(&mut self, y: Ref, w: Ref, m: Ref, pow2_s: Ref, z: Ref, lo: i64, hi: i64, dtype: DType) -> Ref {
        let s = self.shape(y);
        let (Dim::Fixed(k), Dim::Fixed(width)) = (s[0], s[1]) else { panic!("static") };
        let wr = self.reshape_fixed(w, &[1, k]);
        let acc = self.matmul(wr, y, DType::I64);
        let acc = self.reshape_fixed(acc, &[width]);
        self.narrow_a16(acc, m, pow2_s, z, lo, hi, dtype)
    }

    /// RoPE angles for position `pos` from TWO pinned tables of `2^lo_bits` rows each — the
    /// long-context strategy (spec 04b §9.4): `pos = hi·2^lo_bits + lo`, and the angle-addition
    /// formulas combine the rows in Q24 (floor). A `history_bound` of `2^18` needs two 512-row
    /// tables instead of one 262,144-row table. Returns `(cos, sin)`, `i32` Q24.
    pub fn rope_angles_two_level(&mut self, pos: Ref, cos_hi: Ref, sin_hi: Ref, cos_lo: Ref, sin_lo: Ref, lo_bits: u32) -> (Ref, Ref) {
        let d = self.c(DType::I64, 1i128 << lo_bits);
        let hi = self.div(pos, d, Rounding::Floor, DType::Idx);
        let back = self.mul(hi, d, DType::I64);
        let lo = self.sub(pos, back, DType::Idx);
        let ch = self.gather(cos_hi, hi, 0, 0);
        let sh = self.gather(sin_hi, hi, 0, 0);
        let cl = self.gather(cos_lo, lo, 0, 0);
        let sl = self.gather(sin_lo, lo, 0, 0);
        let a = self.mul(ch, cl, DType::I64);
        let b = self.mul(sh, sl, DType::I64);
        let c = self.sub(a, b, DType::I64);
        let c = self.shr(c, K, Rounding::Floor, DType::I64);
        let c = self.clamp(c, -(ONE as i64), ONE as i64, DType::I32);
        let e = self.mul(sh, cl, DType::I64);
        let f = self.mul(ch, sl, DType::I64);
        let s = self.add(e, f, DType::I64);
        let s = self.shr(s, K, Rounding::Floor, DType::I64);
        let s = self.clamp(s, -(ONE as i64), ONE as i64, DType::I32);
        (c, s)
    }
}

/// A per-head vector `[h]` as `[h, 1, 1]`, to broadcast against `[h, rows, cols]`.
fn b_reshape3(b: &mut BlockBuilder<'_>, x: Ref, h: u32) -> Ref {
    b.reshape_fixed(x, &[h, 1, 1])
}
