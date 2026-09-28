//! A small builder for writing programs by hand.
//!
//! The builder infers every node's output shape the way `validate` checks it, so a program built
//! here carries fully inferred `out` types. It PANICS on misuse (a shape that does not fit): it is
//! a construction API for tests, tools and lowerers, whose output is validated by
//! [`crate::validate::validate`] before anything runs it. The interpreter never panics.
//!
//! This module holds the primitives and three pinned helpers (`c`, `shr`, the `Pow2` tables); the
//! composite templates — the narrowing, norms, softmax, RoPE, attention, routing, the recurrences —
//! are `tir_library_v1`, [`crate::library`], as more methods on the same [`BlockBuilder`].

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

    /// `2^s` as `i128` for shift amounts known to lie in `[0, max]` (`max ≤ 126`): a gather from the
    /// pinned table `[2^0, …, 2^max]`.
    ///
    /// The table has exactly `max + 1` entries, so its interval — which is what the range rules
    /// give a `Gather` from it (spec 04b §7) — is `[1, 2^max]`, not `[1, 2^126]`.
    pub fn pow2_128_of(&mut self, s: Ref, max: u32) -> Ref {
        assert!(max <= 126);
        let table: Vec<i128> = (0..=max).map(|i| 1i128 << i).collect();
        let t = self.pb.konst(DType::I128, &[max + 1], &table);
        let s = self.clamp(s, 0, max as i64, DType::Idx);
        self.gather(t, s, 0, 0)
    }
}
