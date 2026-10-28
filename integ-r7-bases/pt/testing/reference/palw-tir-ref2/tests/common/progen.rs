//! A random generator of well-formed PALW-TIR programs, written from 04b's own rules (NF-1..22 and
//! the §6 type rules) — this crate's generator, not the first implementation's builder.
//!
//! Every node is checked by this crate's type rule and legality check before it is kept, so a
//! generated program is in normal form by construction (the tests assert this crate accepts it);
//! whether the first implementation accepts it too is the question the differential asks.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use misaka_palw_tir_ref2 as ref2;
use rand::Rng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use ref2::eval::Params;
use ref2::normal_form::Role;
use ref2::typing::check_type;
use ref2::{
    Block, Cmp, ConstDecl, DType, Dim, Node, ParamDecl, Prim, Program, Ref, Rounding, Schedule, StateDecl, StateKind, Tensor,
    TensorType,
};

pub type R = ChaCha8Rng;

#[derive(Clone, Copy, Debug)]
pub struct GenCfg {
    /// Let post write states (StateWrite, HistAppend, also of a state pre writes): programs NF-19
    /// (revision 2) refuses, generated to check that both implementations refuse them.
    pub post_writes: bool,
    pub max_nodes: usize,
    /// Keep only nodes whose §7 obligations hold (this crate's transfer functions), so that the
    /// program passes admission's range analysis and reaches the cones and the replay.
    pub range_safe: bool,
}

impl Default for GenCfg {
    fn default() -> Self {
        GenCfg { post_writes: false, max_nodes: 18, range_safe: false }
    }
}

pub struct Generated {
    pub prog: Program,
    pub params: Params,
}

// ---------------------------------------------------------------- values

pub fn pick<T: Clone>(rng: &mut R, v: &[T]) -> T {
    v[rng.gen_range(0..v.len())].clone()
}

fn clamp_to(dt: DType, v: i128) -> i128 {
    v.clamp(dt.min(), dt.max())
}

/// A value of `dt` from one of several profiles: small, full-range, extreme, powers of two.
pub fn rand_value(rng: &mut R, dt: DType, profile: u8) -> i128 {
    match profile {
        0 => clamp_to(dt, rng.gen_range(-8i128..=8)),
        1 => {
            // Uniform bits over the dtype.
            let bits = (8 * dt.width()) as u32;
            let u: u128 = ((rng.r#gen::<u64>() as u128) << 64) | rng.r#gen::<u64>() as u128;
            if dt == DType::Idx {
                (u & 0xFFFF_FFFF) as i128
            } else if bits == 128 {
                u as i128
            } else {
                let m = u & ((1u128 << bits) - 1);
                if m >> (bits - 1) == 1 { m as i128 - (1i128 << bits) } else { m as i128 }
            }
        }
        2 => pick(
            rng,
            &[dt.min(), dt.max(), clamp_to(dt, dt.min() + 1), clamp_to(dt, dt.max() - 1), 0, clamp_to(dt, 1), clamp_to(dt, -1)],
        ),
        _ => {
            let maxk = (8 * dt.width() - 2) as u32;
            let k = rng.gen_range(0..=maxk.min(126));
            let v = 1i128 << k;
            let v = v + pick(rng, &[-1i128, 0, 1]);
            clamp_to(dt, if rng.gen_bool(0.5) { v } else { -v })
        }
    }
}

pub fn rand_profile(rng: &mut R) -> u8 {
    let x = rng.gen_range(0..100);
    if x < 70 {
        0
    } else if x < 75 {
        1
    } else if x < 90 {
        2
    } else {
        3
    }
}

pub fn rand_tensor(rng: &mut R, dt: DType, shape: &[u64]) -> Tensor {
    let n: u64 = shape.iter().product();
    let prof = rand_profile(rng);
    let data = (0..n)
        .map(|_| {
            let pr = if rng.gen_bool(0.9) { prof } else { rand_profile(rng) };
            rand_value(rng, dt, pr)
        })
        .collect();
    Tensor::new(dt, shape.to_vec(), data).unwrap()
}

fn le_bytes(dt: DType, vals: &[i128]) -> Vec<u8> {
    let mut out = Vec::new();
    for &v in vals {
        let u = v as u128;
        for i in 0..dt.width() {
            out.push((u >> (8 * i)) as u8);
        }
    }
    out
}

// ---------------------------------------------------------------- the generator

struct BB {
    b: usize,
    role: Role,
    window: Option<u32>,
    nodes: Vec<Node>,
    /// The §7 interval of every node (for range-safe generation).
    ivs: Vec<ref2::admit::Interval>,
    pool: Vec<(Ref, TensorType)>,
    written: BTreeSet<u16>,
    appended: BTreeSet<u16>,
}

struct G<'r> {
    rng: &'r mut R,
    cfg: GenCfg,
    params: Vec<ParamDecl>,
    consts: Vec<ConstDecl>,
    states: Vec<StateDecl>,
    history_bound: u32,
    token_bound: u32,
    /// Global state → the block that writes / appends it.
    global_writer: BTreeMap<u16, usize>,
}

fn arith_dtype(rng: &mut R) -> DType {
    let x = rng.gen_range(0..100);
    match x {
        0..=29 => DType::I64,
        30..=59 => DType::I128,
        60..=84 => DType::I32,
        85..=89 => DType::I16,
        90..=94 => DType::I8,
        _ => DType::Idx,
    }
}

fn any_dtype(rng: &mut R) -> DType {
    pick(rng, &DType::ALL)
}

fn narrow_dtype(rng: &mut R) -> DType {
    pick(rng, &[DType::I8, DType::I16, DType::I32])
}

fn committable_dtype(rng: &mut R) -> DType {
    pick(rng, &[DType::I8, DType::I16, DType::I32, DType::Idx])
}

fn has_h(t: &TensorType) -> bool {
    t.shape.contains(&Dim::H)
}

fn fixed_dims(t: &TensorType) -> Option<Vec<u32>> {
    t.shape
        .iter()
        .map(|d| match d {
            Dim::Fixed(n) => Some(*n),
            Dim::H => None,
        })
        .collect()
}

/// A random factorisation of `count` into at most `max_rank` dimensions.
fn factor(rng: &mut R, count: u32, max_rank: usize) -> Vec<Dim> {
    if max_rank == 0 {
        return vec![];
    }
    let rank = rng.gen_range(if count == 1 { 0 } else { 1 }..=max_rank);
    if rank == 0 {
        return vec![];
    }
    let mut dims = Vec::new();
    let mut rem = count;
    for _ in 0..rank - 1 {
        let divs: Vec<u32> = (1..=rem).filter(|d| rem.is_multiple_of(*d)).collect();
        let d = pick(rng, &divs);
        dims.push(Dim::Fixed(d));
        rem /= d;
    }
    dims.push(Dim::Fixed(rem));
    dims.shuffle(rng);
    dims
}

impl<'r> G<'r> {
    fn small_dim(&mut self) -> u32 {
        pick(self.rng, &[1, 1, 2, 2, 3, 4])
    }

    fn rand_fixed_shape(&mut self, max_rank: usize) -> Vec<u32> {
        let r = self.rng.gen_range(0..=max_rank);
        (0..r).map(|_| self.small_dim()).collect()
    }

    fn new_param(&mut self, dtype: DType, shape: &[u32], per_layer: bool) -> (Ref, TensorType) {
        let name = format!("p{}", self.params.len());
        self.params.push(ParamDecl { name, dtype, shape: shape.to_vec(), per_layer });
        (Ref::Param((self.params.len() - 1) as u16), TensorType::fixed(dtype, shape))
    }

    fn new_const(&mut self, dtype: DType, shape: &[u32], vals: &[i128]) -> (Ref, TensorType) {
        let c = ConstDecl { dtype, shape: shape.to_vec(), data: le_bytes(dtype, vals) };
        let j = match self.consts.iter().position(|x| *x == c) {
            Some(j) => j,
            None => {
                self.consts.push(c);
                self.consts.len() - 1
            }
        };
        (Ref::Const(j as u16), TensorType::fixed(dtype, shape))
    }

    fn rand_const(&mut self, dtype: DType, shape: &[u32]) -> (Ref, TensorType) {
        let n: u32 = shape.iter().product();
        let prof = rand_profile(self.rng);
        let vals: Vec<i128> = (0..n).map(|_| rand_value(self.rng, dtype, prof)).collect();
        self.new_const(dtype, shape, &vals)
    }

    /// A leaf of the given shape (a new param or const), dtype random unless given.
    fn new_leaf(&mut self, bb: &BB, dtype: Option<DType>, shape: &[u32]) -> (Ref, TensorType) {
        let dt = dtype.unwrap_or_else(|| any_dtype(self.rng));
        if self.rng.gen_bool(0.5) && dt != DType::I128 {
            let per_layer = bb.role == Role::Layer && self.rng.gen_bool(0.5);
            self.new_param(dt, shape, per_layer)
        } else {
            self.rand_const(dt, shape)
        }
    }

    /// A Fixed state readable by this block (new or reused), as an operand.
    fn state_read(&mut self, bb: &BB) -> (Ref, TensorType) {
        let per_layer = bb.role == Role::Layer;
        let cands: Vec<usize> = (0..self.states.len())
            .filter(|&j| self.states[j].per_layer == per_layer && matches!(self.states[j].kind, StateKind::Fixed { .. }))
            .collect();
        let j = if !cands.is_empty() && (self.rng.gen_bool(0.6) || !self.may_add_state(per_layer)) {
            pick(self.rng, &cands)
        } else if !self.may_add_state(per_layer) {
            let shape = self.rand_fixed_shape(3);
            return self.new_leaf(bb, None, &shape);
        } else {
            let dtype = narrow_dtype(self.rng);
            let shape = self.rand_fixed_shape(3);
            let (lo, hi) = self.rand_state_range(dtype);
            self.states.push(StateDecl {
                name: format!("s{}", self.states.len()),
                kind: StateKind::Fixed { lo, hi },
                dtype,
                shape,
                per_layer,
            });
            self.states.len() - 1
        };
        let s = &self.states[j];
        (Ref::State(j as u16), TensorType::fixed(s.dtype, &s.shape))
    }

    fn may_add_state(&self, per_layer: bool) -> bool {
        let n = self.states.iter().filter(|s| s.per_layer == per_layer).count();
        n < if per_layer { 12 } else { 40 }
    }

    fn rand_state_range(&mut self, dtype: DType) -> (i64, i64) {
        let lo = pick(self.rng, &[dtype.min(), -100, -1, 0, -7]).max(dtype.min()) as i64;
        let hi = pick(self.rng, &[dtype.max(), 100, 1, 0, 9]).min(dtype.max()) as i64;
        (lo, hi)
    }

    /// Any operand: mostly from the pool, sometimes a new leaf or a state read.
    fn operand(&mut self, bb: &BB) -> (Ref, TensorType) {
        let x = self.rng.gen_range(0..100);
        if x < 75 && !bb.pool.is_empty() {
            // Prefer recent values (deeper graphs).
            let n = bb.pool.len();
            let i = if self.rng.gen_bool(0.5) { n - 1 - self.rng.gen_range(0..n.min(4)) } else { self.rng.gen_range(0..n) };
            return bb.pool[i].clone();
        }
        if x < 85 {
            return self.state_read(bb);
        }
        let shape = self.rand_fixed_shape(3);
        self.new_leaf(bb, None, &shape)
    }

    fn operand_where(&mut self, bb: &BB, f: impl Fn(&TensorType) -> bool) -> Option<(Ref, TensorType)> {
        let c: Vec<(Ref, TensorType)> = bb.pool.iter().filter(|(_, t)| f(t)).cloned().collect();
        if c.is_empty() { None } else { Some(pick(self.rng, &c)) }
    }

    /// Pushes a node if this crate's type rule and legality check accept it.
    fn push(
        &mut self,
        bb: &mut BB,
        prim: Prim,
        inputs: Vec<(Ref, TensorType)>,
        out: TensorType,
        commit: bool,
    ) -> Option<(Ref, TensorType)> {
        if out.check_legal(bb.window).is_err() {
            return None;
        }
        let tys: Vec<TensorType> = inputs.iter().map(|(_, t)| t.clone()).collect();
        if check_type(&prim, &tys, &out, &self.states).is_err() {
            return None;
        }
        if commit && !out.dtype.committable() {
            return None;
        }
        let idx = bb.nodes.len();
        if idx >= 500 {
            return None;
        }
        // §7: the node's interval from its operands'; a broken obligation discards it when
        // generating range-safe programs.
        let ins: Vec<ref2::admit::Interval> = inputs.iter().map(|(r, t)| self.iv_of_ref(bb, r, t)).collect();
        let h = bb.window.map(|w| w as u64).unwrap_or(1);
        let iv = match ref2::admit::transfer(&prim, &ins, &tys, &out, h, &self.states, 0, idx) {
            Ok(iv) => iv,
            Err(_) if self.cfg.range_safe => return None,
            Err(_) => ref2::admit::Interval { lo: out.dtype.min(), hi: out.dtype.max() },
        };
        bb.ivs.push(iv);
        bb.nodes.push(Node { prim, inputs: inputs.iter().map(|(r, _)| *r).collect(), out: out.clone(), commit });
        let r = (Ref::Node(idx as u16), out);
        bb.pool.push(r.clone());
        Some(r)
    }

    /// The §7 interval of an operand.
    fn iv_of_ref(&self, bb: &BB, r: &Ref, t: &TensorType) -> ref2::admit::Interval {
        use ref2::admit::Interval;
        match *r {
            Ref::Node(k) => bb.ivs[k as usize],
            Ref::Const(j) => {
                let c = &self.consts[j as usize];
                let v = Tensor::from_le_bytes(c.dtype, c.shape.iter().map(|&d| d as u64).collect(), &c.data).unwrap();
                Interval { lo: *v.data.iter().min().unwrap(), hi: *v.data.iter().max().unwrap() }
            }
            Ref::State(j) => match self.states[j as usize].kind {
                StateKind::Fixed { lo, hi } => Interval { lo: lo as i128, hi: hi as i128 },
                StateKind::Hist { .. } => Interval { lo: t.dtype.min(), hi: t.dtype.max() },
            },
            Ref::Input(0) => Interval { lo: 0, hi: self.token_bound as i128 - 1 },
            Ref::Input(_) => Interval { lo: 0, hi: self.history_bound as i128 - 1 },
            Ref::CarryIn(_) | Ref::Param(_) => Interval { lo: t.dtype.min(), hi: t.dtype.max() },
        }
    }

    /// A shape broadcast-compatible with `s` (dims turned to 1, leading dims dropped or added).
    fn compatible_shape(&mut self, s: &[Dim]) -> Vec<Dim> {
        let drop = if s.is_empty() { 0 } else { self.rng.gen_range(0..=s.len()) };
        let mut out: Vec<Dim> = s[drop..].iter().map(|d| if self.rng.gen_bool(0.3) { Dim::Fixed(1) } else { *d }).collect();
        if out.len() < 4 && self.rng.gen_bool(0.15) {
            out.insert(0, Dim::Fixed(1));
        }
        out
    }

    /// A partner operand broadcast-compatible with `s`: from the pool, or a new leaf.
    fn partner(&mut self, bb: &BB, s: &[Dim], dtype: Option<DType>) -> (Ref, TensorType) {
        if self.rng.gen_bool(0.5)
            && let Some(v) =
                self.operand_where(bb, |t| ref2::types::broadcast_shapes(&t.shape, s).is_ok() && dtype.is_none_or(|d| d == t.dtype))
        {
            return v;
        }
        let cs = self.compatible_shape(s);
        let fixed: Vec<u32> = cs.iter().map(|d| if let Dim::Fixed(n) = d { *n } else { 1 }).collect();
        self.new_leaf(bb, dtype, &fixed)
    }

    fn try_prim(&mut self, bb: &mut BB) -> Option<(Ref, TensorType)> {
        let k = self.rng.gen_range(0..26);
        match k {
            0 => {
                // Reshape
                let (r, t) = self.operand(bb);
                let shape = match t.shape.iter().position(|d| *d == Dim::H) {
                    None => {
                        let c: u32 = fixed_dims(&t)?.iter().product();
                        factor(self.rng, c, 4)
                    }
                    Some(hp) => {
                        let before: u32 = t.shape[..hp].iter().map(|d| if let Dim::Fixed(n) = d { *n } else { 1 }).product();
                        let after: u32 = t.shape[hp + 1..].iter().map(|d| if let Dim::Fixed(n) = d { *n } else { 1 }).product();
                        let mut s = factor(self.rng, before, 2);
                        s.push(Dim::H);
                        let room = 4 - s.len();
                        s.extend(factor(self.rng, after, room.min(2)));
                        s
                    }
                };
                self.push(bb, Prim::Reshape, vec![(r, t.clone())], TensorType { dtype: t.dtype, shape }, false)
            }
            1 => {
                let (r, t) = self.operand(bb);
                let mut perm: Vec<u8> = (0..t.shape.len() as u8).collect();
                perm.shuffle(self.rng);
                let shape = perm.iter().map(|&p| t.shape[p as usize]).collect();
                self.push(bb, Prim::Transpose { perm }, vec![(r, t.clone())], TensorType { dtype: t.dtype, shape }, false)
            }
            2 => {
                let (r, t) = self.operand(bb);
                let axes: Vec<usize> = (0..t.shape.len()).filter(|&a| matches!(t.shape[a], Dim::Fixed(_))).collect();
                if axes.is_empty() {
                    return None;
                }
                let a = pick(self.rng, &axes);
                let Dim::Fixed(n) = t.shape[a] else { return None };
                let len = self.rng.gen_range(1..=n);
                let start = self.rng.gen_range(0..=n - len);
                let mut shape = t.shape.clone();
                shape[a] = Dim::Fixed(len);
                self.push(bb, Prim::Slice { axis: a as u8, start }, vec![(r, t.clone())], TensorType { dtype: t.dtype, shape }, false)
            }
            3 => {
                let (r, t) = self.operand(bb);
                let axes: Vec<usize> = (0..t.shape.len()).filter(|&a| matches!(t.shape[a], Dim::Fixed(_))).collect();
                if axes.is_empty() {
                    return None;
                }
                let a = pick(self.rng, &axes);
                let mut ins = vec![(r, t.clone())];
                let m = self.rng.gen_range(2..=4);
                while ins.len() < m {
                    let same = |u: &TensorType| {
                        u.dtype == t.dtype
                            && u.shape.len() == t.shape.len()
                            && matches!(u.shape[a], Dim::Fixed(_))
                            && (0..t.shape.len()).all(|d| d == a || u.shape[d] == t.shape[d])
                    };
                    if let Some(v) = self.operand_where(bb, same).filter(|_| self.rng.gen_bool(0.6)) {
                        ins.push(v);
                    } else if let Some(mut dims) = fixed_dims(&t) {
                        dims[a] = self.small_dim();
                        if t.dtype == DType::I128 {
                            let c = self.rand_const(t.dtype, &dims);
                            ins.push(c);
                        } else {
                            let l = self.new_leaf(bb, Some(t.dtype), &dims);
                            ins.push(l);
                        }
                    } else {
                        ins.push((r, t.clone()));
                    }
                }
                let total: u32 = ins.iter().map(|(_, u)| if let Dim::Fixed(n) = u.shape[a] { n } else { 0 }).sum();
                let mut shape = t.shape.clone();
                shape[a] = Dim::Fixed(total);
                self.push(bb, Prim::Concat { axis: a as u8 }, ins, TensorType { dtype: t.dtype, shape }, false)
            }
            4 => {
                let (r, t) = self.operand(bb);
                let extra = self.rng.gen_range(0..=4 - t.shape.len());
                let mut h_used = has_h(&t);
                let mut shape = Vec::new();
                for _ in 0..extra {
                    if !h_used && bb.window.is_some() && self.rng.gen_bool(0.3) {
                        shape.push(Dim::H);
                        h_used = true;
                    } else {
                        let d = self.small_dim();
                        shape.push(Dim::Fixed(d));
                    }
                }
                for d in &t.shape {
                    if *d == Dim::Fixed(1) && self.rng.gen_bool(0.6) {
                        if !h_used && bb.window.is_some() && self.rng.gen_bool(0.3) {
                            shape.push(Dim::H);
                            h_used = true;
                        } else {
                            let d = self.small_dim();
                            shape.push(Dim::Fixed(d));
                        }
                    } else {
                        shape.push(*d);
                    }
                }
                self.push(bb, Prim::Broadcast, vec![(r, t.clone())], TensorType { dtype: t.dtype, shape }, false)
            }
            5 => {
                let rank = self.rng.gen_range(1..=3);
                let mut shape: Vec<Dim> = (0..rank).map(|_| Dim::Fixed(self.small_dim())).collect();
                if bb.window.is_some() && self.rng.gen_bool(0.5) {
                    let i = self.rng.gen_range(0..rank);
                    shape[i] = Dim::H;
                }
                let axis = self.rng.gen_range(0..rank) as u8;
                let (start, step) = if self.rng.gen_bool(0.8) {
                    (pick(self.rng, &[0i64, 1, -3, 5, -100, 17]), pick(self.rng, &[0i64, 1, -1, 2, 7, -3]))
                } else {
                    (
                        pick(self.rng, &[i64::MIN, i64::MAX, -1000, 1 << 40, 1 << 31]),
                        pick(self.rng, &[-100, i64::MAX, i64::MIN, 1 << 33, 1 << 31]),
                    )
                };
                let dtype = any_dtype(self.rng);
                self.push(bb, Prim::Iota { axis, start, step }, vec![], TensorType { dtype, shape }, false)
            }
            6 => self.try_gather(bb),
            7 => {
                let (r, t) = self.operand(bb);
                let dtype = any_dtype(self.rng);
                self.push(bb, Prim::Cast, vec![(r, t.clone())], TensorType { dtype, shape: t.shape.clone() }, false)
            }
            8..=10 => {
                let (a, ta) = self.operand(bb);
                let (b, tb) = self.partner(bb, &ta.shape, None);
                let (a, ta, b, tb) = if self.rng.gen_bool(0.5) { (a, ta, b, tb) } else { (b, tb, a, ta) };
                let shape = ref2::types::broadcast_shapes(&ta.shape, &tb.shape).ok()?;
                let prim = pick(self.rng, &[Prim::Add, Prim::Sub, Prim::Mul]);
                let dtype = arith_dtype(self.rng);
                self.push(bb, prim, vec![(a, ta), (b, tb)], TensorType { dtype, shape }, false)
            }
            11 => {
                // Div: divisors mostly ≥ 1 (positive consts, clamped values), sometimes anything.
                let (x, tx) = self.operand(bb);
                let (d, td) = match self.rng.gen_range(0..4) {
                    0 => {
                        let s = self.compatible_shape(&tx.shape);
                        let dims: Vec<u32> = s.iter().map(|d| if let Dim::Fixed(n) = d { *n } else { 1 }).collect();
                        let n: u32 = dims.iter().product();
                        let dt = pick(self.rng, &[DType::I32, DType::I64, DType::Idx, DType::I128, DType::I8]);
                        let vals: Vec<i128> = (0..n)
                            .map(|_| {
                                clamp_to(
                                    dt,
                                    pick(self.rng, &[1i128, 2, 3, 4, 7, 16, 1 << 24, 1 << 31, (1 << 62) + 1, i64::MAX as i128]),
                                )
                                .max(1)
                            })
                            .collect();
                        self.new_const(dt, &dims, &vals)
                    }
                    1 => {
                        let (v, tv) = self.partner(bb, &tx.shape, None);
                        let dt = pick(self.rng, &[DType::I32, DType::I64, DType::Idx]);
                        let hi = clamp_to(dt, pick(self.rng, &[2i128, 16, 1 << 20, 1 << 31, i64::MAX as i128]));
                        let out = TensorType { dtype: dt, shape: tv.shape.clone() };
                        self.push(bb, Prim::Clamp { lo: 1, hi: hi as i64 }, vec![(v, tv)], out, false)?
                    }
                    _ => self.partner(bb, &tx.shape, None),
                };
                let shape = ref2::types::broadcast_shapes(&tx.shape, &td.shape).ok()?;
                let rule = pick(self.rng, &[Rounding::Floor, Rounding::HalfUp, Rounding::HalfAwayFromZero]);
                let dtype = arith_dtype(self.rng);
                self.push(bb, Prim::Div { rule }, vec![(x, tx), (d, td)], TensorType { dtype, shape }, false)
            }
            12 => self.try_matmul(bb),
            13 | 14 => {
                let (r, t) = self.operand(bb);
                if t.shape.is_empty() {
                    return None;
                }
                let axis = self.rng.gen_range(0..t.shape.len());
                let mut shape = t.shape.clone();
                shape[axis] = Dim::Fixed(1);
                if k == 13 {
                    let dtype = arith_dtype(self.rng);
                    self.push(bb, Prim::ReduceSum { axis: axis as u8 }, vec![(r, t)], TensorType { dtype, shape }, false)
                } else {
                    let dtype = t.dtype;
                    self.push(bb, Prim::ReduceMax { axis: axis as u8 }, vec![(r, t)], TensorType { dtype, shape }, false)
                }
            }
            15 | 16 => {
                let (r, t) = self.operand(bb);
                let dtype = any_dtype(self.rng);
                let (lo, hi) = self.clamp_bounds(dtype);
                self.push(bb, Prim::Clamp { lo, hi }, vec![(r, t.clone())], TensorType { dtype, shape: t.shape.clone() }, false)
            }
            17 => {
                let (r, t) = self.operand(bb);
                let dtype = any_dtype(self.rng);
                self.push(bb, Prim::Log2Floor, vec![(r, t.clone())], TensorType { dtype, shape: t.shape.clone() }, false)
            }
            18 => {
                let (r, t) = self.operand(bb);
                let prim = pick(self.rng, &[Prim::IntExp, Prim::IntRsqrt, Prim::IntLn]);
                let dtype = pick(self.rng, &[DType::I32, DType::I64, DType::I64, DType::I128, DType::Idx, DType::I16]);
                self.push(bb, prim, vec![(r, t.clone())], TensorType { dtype, shape: t.shape.clone() }, false)
            }
            19 => {
                let (a, ta) = self.operand(bb);
                let (b, tb) = self.partner(bb, &ta.shape, None);
                let shape = ref2::types::broadcast_shapes(&ta.shape, &tb.shape).ok()?;
                let cmp = pick(self.rng, &[Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge]);
                self.push(bb, Prim::Compare { cmp }, vec![(a, ta), (b, tb)], TensorType { dtype: DType::I8, shape }, false)
            }
            20 => {
                let (c, tc) = self.operand(bb);
                let (a, ta) = self.partner(bb, &tc.shape, None);
                let s = ref2::types::broadcast_shapes(&tc.shape, &ta.shape).ok()?;
                let (b, tb) = self.partner(bb, &s, None);
                let shape = ref2::types::broadcast_shapes(&s, &tb.shape).ok()?;
                let wide = arith_dtype(self.rng);
                let dtype = pick(self.rng, &[ta.dtype, tb.dtype, wide]);
                self.push(bb, Prim::Select, vec![(c, tc), (a, ta), (b, tb)], TensorType { dtype, shape }, false)
            }
            21 => {
                let (r, t) = self.operand(bb);
                let axes: Vec<usize> = (0..t.shape.len()).filter(|&a| matches!(t.shape[a], Dim::Fixed(_))).collect();
                if axes.is_empty() {
                    return None;
                }
                let a = pick(self.rng, &axes);
                let Dim::Fixed(n) = t.shape[a] else { return None };
                let k = self.rng.gen_range(1..=n);
                let mut shape = t.shape.clone();
                shape[a] = Dim::Fixed(k);
                self.push(bb, Prim::TopK { axis: a as u8, k }, vec![(r, t)], TensorType { dtype: DType::Idx, shape }, true)
            }
            22 => self.try_state_write(bb),
            23 => self.try_hist_append(bb),
            _ => self.try_attention(bb),
        }
    }

    fn clamp_bounds(&mut self, dtype: DType) -> (i64, i64) {
        let lo_c = dtype.min().max(i64::MIN as i128);
        let hi_c = dtype.max().min(i64::MAX as i128);
        let a = clamp_to(dtype, pick(self.rng, &[lo_c, -1000, -1, 0, 1, 5, -(1 << 20)]).clamp(lo_c, hi_c));
        let b = clamp_to(dtype, pick(self.rng, &[hi_c, 1000, 0, 1, 255, 1 << 24, 1 << 40]).clamp(lo_c, hi_c));
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        (lo as i64, hi as i64)
    }

    fn try_gather(&mut self, bb: &mut BB) -> Option<(Ref, TensorType)> {
        // data: pool value or a new param/const, rank ≥ 1, with a Fixed axis.
        let (dr, dt) = match self.operand_where(bb, |t| !t.shape.is_empty() && t.shape.iter().any(|d| matches!(d, Dim::Fixed(_)))) {
            Some(v) if self.rng.gen_bool(0.5) => v,
            _ => {
                let rank = self.rng.gen_range(1..=3);
                let shape: Vec<u32> = (0..rank).map(|_| pick(self.rng, &[2u32, 3, 4, 8])).collect();
                self.new_leaf(bb, None, &shape)
            }
        };
        let axes: Vec<usize> = (0..dt.shape.len()).filter(|&a| matches!(dt.shape[a], Dim::Fixed(_))).collect();
        let a = pick(self.rng, &axes);
        let Dim::Fixed(extent) = dt.shape[a] else { return None };
        let b = self.rng.gen_range(0..=a);
        // indices: shape data[..b] ++ tail (rank ≤ 4 overall).
        let mut ishape: Vec<Dim> = dt.shape[..b].to_vec();
        let tail_rank = self.rng.gen_range(0..=2usize);
        for _ in 0..tail_rank {
            let d = self.small_dim();
            ishape.push(Dim::Fixed(d));
        }
        let (ir, it) = match self.rng.gen_range(0..3) {
            0 if !has_h(&TensorType { dtype: DType::I8, shape: ishape.clone() }) => {
                // a const of in-range (sometimes out-of-range) indices
                let dims = fixed_dims(&TensorType { dtype: DType::I8, shape: ishape.clone() })?;
                let n: u32 = dims.iter().product();
                let idt = pick(self.rng, &[DType::Idx, DType::I32, DType::I64, DType::I8]);
                let vals: Vec<i128> = (0..n)
                    .map(|_| {
                        if self.rng.gen_bool(0.95) {
                            self.rng.gen_range(0..extent) as i128
                        } else {
                            pick(self.rng, &[-1i128, extent as i128])
                        }
                    })
                    .map(|v| clamp_to(idt, v))
                    .collect();
                self.new_const(idt, &dims, &vals)
            }
            1 if self.token_bound as u64 <= extent as u64 && b == 0 && self.rng.gen_bool(0.7) => {
                (Ref::Input(0), TensorType { dtype: DType::Idx, shape: vec![] })
            }
            _ => {
                // Clamp{0, extent−1} of something whose leading b dims match.
                let src = self.operand_where(bb, |t| t.shape.len() >= b && t.shape[..b] == dt.shape[..b] && t.shape.len() - b <= 2);
                let (sr, st) = match src {
                    Some(v) => v,
                    None => {
                        let dims = fixed_dims(&TensorType { dtype: DType::I8, shape: ishape.clone() })?;
                        self.new_leaf(bb, None, &dims)
                    }
                };
                let idt = pick(self.rng, &[DType::Idx, DType::I32, DType::I64]);
                let hi = (extent - 1) as i64;
                let lo = if self.rng.gen_bool(0.05) { -1 } else { 0 };
                let lo = if idt == DType::Idx { 0 } else { lo };
                self.push(
                    bb,
                    Prim::Clamp { lo, hi },
                    vec![(sr, st.clone())],
                    TensorType { dtype: idt, shape: st.shape.clone() },
                    false,
                )?
            }
        };
        if it.shape.len() < b || it.shape[..b] != dt.shape[..b] {
            return None;
        }
        let mut shape = dt.shape[..a].to_vec();
        shape.extend_from_slice(&it.shape[b..]);
        shape.extend_from_slice(&dt.shape[a + 1..]);
        self.push(
            bb,
            Prim::Gather { axis: a as u8, batch_dims: b as u8 },
            vec![(dr, dt.clone()), (ir, it)],
            TensorType { dtype: dt.dtype, shape },
            false,
        )
    }

    fn try_matmul(&mut self, bb: &mut BB) -> Option<(Ref, TensorType)> {
        let ok_dt = |d: DType| d != DType::I128 && d != DType::Idx;
        let (ar, at) = match self.operand_where(bb, |t| t.shape.len() >= 2 && ok_dt(t.dtype)) {
            Some(v) => v,
            None => {
                let shape = vec![self.small_dim(), self.small_dim()];
                let dt = pick(self.rng, &[DType::I8, DType::I16, DType::I32, DType::I64]);
                self.new_leaf(bb, Some(dt), &shape)
            }
        };
        let ra = at.shape.len();
        let kdim = at.shape[ra - 1];
        let batch_a = at.shape[..ra - 2].to_vec();
        let cand = self.operand_where(bb, |t| {
            t.shape.len() >= 2
                && ok_dt(t.dtype)
                && t.shape[t.shape.len() - 2] == kdim
                && ref2::types::broadcast_shapes(&batch_a, &t.shape[..t.shape.len() - 2]).is_ok()
        });
        let use_cand = cand.is_some() && (self.rng.gen_bool(0.6) || !matches!(kdim, Dim::Fixed(_)));
        let (br, bt) = match cand {
            Some(v) if use_cand => v,
            _ => {
                let Dim::Fixed(k) = kdim else {
                    return None;
                };
                let bb_batch = self.compatible_shape(&batch_a);
                let mut dims: Vec<u32> = bb_batch.iter().map(|d| if let Dim::Fixed(n) = d { *n } else { 1 }).collect();
                dims.push(k);
                let n = self.small_dim();
                dims.push(n);
                if dims.len() > 4 {
                    return None;
                }
                let dt = pick(self.rng, &[DType::I8, DType::I16, DType::I32, DType::I64]);
                self.new_leaf(bb, Some(dt), &dims)
            }
        };
        let rb = bt.shape.len();
        let mut shape = ref2::types::broadcast_shapes(&batch_a, &bt.shape[..rb - 2]).ok()?;
        shape.push(at.shape[ra - 2]);
        shape.push(bt.shape[rb - 1]);
        let dtype = pick(self.rng, &[DType::I32, DType::I64, DType::I128, DType::I64, DType::I16]);
        self.push(bb, Prim::MatMul, vec![(ar, at), (br, bt)], TensorType { dtype, shape }, false)
    }

    /// The attention pattern over a history: scores against the window, a per-position
    /// transcendental, a reduction along H and a MatMul contracting H.
    fn try_attention(&mut self, bb: &mut BB) -> Option<(Ref, TensorType)> {
        let ok_dt = |d: DType| d != DType::I128 && d != DType::Idx;
        let (kr, kt) = self.operand_where(bb, |t| t.shape.len() == 2 && t.shape[0] == Dim::H && ok_dt(t.dtype))?;
        let d = kt.shape[1];
        // K^T: [d, H]
        let (ktr, ktt) = self.push(
            bb,
            Prim::Transpose { perm: vec![1, 0] },
            vec![(kr, kt.clone())],
            TensorType { dtype: kt.dtype, shape: vec![d, Dim::H] },
            false,
        )?;
        let Dim::Fixed(dn) = d else { return None };
        let rows = self.small_dim();
        let qdt = pick(self.rng, &[DType::I8, DType::I16, DType::I32]);
        let (qr, qt) = match self.operand_where(bb, |t| t.shape.len() == 2 && t.shape[1] == d && ok_dt(t.dtype) && !has_h(t)) {
            Some(v) if self.rng.gen_bool(0.5) => v,
            _ => self.new_leaf(bb, Some(qdt), &[rows, dn]),
        };
        let m = qt.shape[0];
        let sdt = pick(self.rng, &[DType::I32, DType::I64, DType::I64]);
        let (sr, st) =
            self.push(bb, Prim::MatMul, vec![(qr, qt), (ktr, ktt)], TensorType { dtype: sdt, shape: vec![m, Dim::H] }, false)?;
        // max along H, shifted scores, IntExp, sum along H.
        let (mr, mt) = self.push(
            bb,
            Prim::ReduceMax { axis: 1 },
            vec![(sr, st.clone())],
            TensorType { dtype: sdt, shape: vec![m, Dim::Fixed(1)] },
            false,
        )?;
        let (dr, dt) = self.push(
            bb,
            Prim::Sub,
            vec![(sr, st.clone()), (mr, mt)],
            TensorType { dtype: DType::I64, shape: vec![m, Dim::H] },
            false,
        )?;
        let (er, et) = self.push(bb, Prim::IntExp, vec![(dr, dt)], TensorType { dtype: DType::I32, shape: vec![m, Dim::H] }, false)?;
        if self.rng.gen_bool(0.5) {
            let _ = self.push(
                bb,
                Prim::ReduceSum { axis: 1 },
                vec![(er, et.clone())],
                TensorType { dtype: DType::I64, shape: vec![m, Dim::Fixed(1)] },
                true,
            );
        }
        // weights · V, contracting H.
        let odt = pick(self.rng, &[DType::I64, DType::I128]);
        self.push(bb, Prim::MatMul, vec![(er, et), (kr, kt)], TensorType { dtype: odt, shape: vec![m, d] }, false)
    }

    fn global_ok(&self, bb: &BB, j: u16) -> bool {
        if self.states[j as usize].per_layer {
            return true;
        }
        match self.global_writer.get(&j) {
            None => true,
            Some(&b) => b == bb.b || self.cfg.post_writes,
        }
    }

    fn try_state_write(&mut self, bb: &mut BB) -> Option<(Ref, TensorType)> {
        let per_layer = bb.role == Role::Layer;
        // NF-19 (revision 2): post writes no state — unless generating refused programs.
        if bb.role == Role::Post && !self.cfg.post_writes {
            return None;
        }
        // With post_writes, post prefers a global state pre already writes (same shape).
        let prefer: Option<u16> = if self.cfg.post_writes && bb.role == Role::Post {
            self.global_writer
                .iter()
                .find(|(j, b)| {
                    **b != bb.b && !bb.written.contains(j) && matches!(self.states[**j as usize].kind, StateKind::Fixed { .. })
                })
                .map(|(j, _)| *j)
        } else {
            None
        };
        if let Some(j) = prefer {
            let want = TensorType::fixed(self.states[j as usize].dtype, &self.states[j as usize].shape);
            if let Some(v) = self.operand_where(bb, |t| t.shape == want.shape) {
                let out = want.clone();
                let r = self.push(bb, Prim::StateWrite { state: j }, vec![v], out, false)?;
                bb.written.insert(j);
                return Some(r);
            }
        }
        let (r, t) = self.operand(bb);
        let dims = fixed_dims(&t)?;
        let cands: Vec<u16> = (0..self.states.len() as u16)
            .filter(|&j| {
                let s = &self.states[j as usize];
                s.per_layer == per_layer
                    && matches!(s.kind, StateKind::Fixed { .. })
                    && s.shape == dims
                    && !bb.written.contains(&j)
                    && self.global_ok(bb, j)
            })
            .collect();
        let j = if !cands.is_empty() && (self.rng.gen_bool(0.7) || !self.may_add_state(per_layer)) {
            pick(self.rng, &cands)
        } else if !self.may_add_state(per_layer) {
            return None;
        } else {
            let dtype = narrow_dtype(self.rng);
            let (lo, hi) = self.rand_state_range(dtype);
            self.states.push(StateDecl {
                name: format!("s{}", self.states.len()),
                kind: StateKind::Fixed { lo, hi },
                dtype,
                shape: dims.clone(),
                per_layer,
            });
            (self.states.len() - 1) as u16
        };
        let s = &self.states[j as usize];
        let out = TensorType::fixed(s.dtype, &s.shape);
        let v = self.push(bb, Prim::StateWrite { state: j }, vec![(r, t)], out, false)?;
        bb.written.insert(j);
        if !per_layer {
            self.global_writer.entry(j).or_insert(bb.b);
        }
        Some(v)
    }

    /// A HistAppend of a rank-1 row (so that `[H, d]` windows exist for the attention pattern).
    fn try_hist_append_vec(&mut self, bb: &mut BB) -> Option<(Ref, TensorType)> {
        bb.window?;
        let (v, tv) = self.operand(bb);
        let dims = fixed_dims(&tv)?;
        let n: u32 = dims.iter().product();
        let (v, tv) = if dims.len() == 1 {
            (v, tv)
        } else if n > 1 {
            self.push(bb, Prim::Reshape, vec![(v, tv.clone())], TensorType { dtype: tv.dtype, shape: vec![Dim::Fixed(n)] }, false)?
        } else {
            let d = pick(self.rng, &[2u32, 3, 4]);
            self.push(bb, Prim::Broadcast, vec![(v, tv.clone())], TensorType { dtype: tv.dtype, shape: vec![Dim::Fixed(d)] }, false)?
        };
        let dtype = narrow_dtype(self.rng);
        let lim = pick(self.rng, &[7i64, 100, 1000]);
        let row = self.push(
            bb,
            Prim::Clamp { lo: -lim, hi: lim },
            vec![(v, tv.clone())],
            TensorType { dtype, shape: tv.shape.clone() },
            true,
        )?;
        self.hist_append_row(bb, row)
    }

    fn try_hist_append(&mut self, bb: &mut BB) -> Option<(Ref, TensorType)> {
        let w = bb.window?;
        // The row: a committed node or a carry-in of i8/i16/i32, no H, rank ≤ 3.
        let row_ok = |t: &TensorType| matches!(t.dtype, DType::I8 | DType::I16 | DType::I32) && !has_h(t) && t.shape.len() <= 3;
        let cands: Vec<(Ref, TensorType)> =
            bb.pool.iter().filter(|(r, t)| row_ok(t) && matches!(r, Ref::Node(_) | Ref::CarryIn(_))).cloned().collect();
        let (rr, rt) = if !cands.is_empty() && self.rng.gen_bool(0.5) {
            pick(self.rng, &cands)
        } else {
            let (v, tv) = self.operand(bb);
            if has_h(&tv) || tv.shape.len() > 3 {
                return None;
            }
            let dtype = narrow_dtype(self.rng);
            let (lo, hi) = self.clamp_bounds(dtype);
            self.push(bb, Prim::Clamp { lo, hi }, vec![(v, tv.clone())], TensorType { dtype, shape: tv.shape.clone() }, true)?
        };
        let _ = w;
        self.hist_append_row(bb, (rr, rt))
    }

    fn hist_append_row(&mut self, bb: &mut BB, row: (Ref, TensorType)) -> Option<(Ref, TensorType)> {
        let w = bb.window?;
        if bb.role == Role::Post && !self.cfg.post_writes {
            return None;
        }
        let (rr, rt) = row;
        if let Ref::Node(i) = rr {
            bb.nodes[i as usize].commit = true;
        }
        let dims = fixed_dims(&rt)?;
        let per_layer = bb.role == Role::Layer;
        let cands: Vec<u16> = (0..self.states.len() as u16)
            .filter(|&j| {
                let s = &self.states[j as usize];
                s.per_layer == per_layer
                    && s.kind == StateKind::Hist { window: w }
                    && s.shape == dims
                    && s.dtype == rt.dtype
                    && !bb.appended.contains(&j)
                    && self.global_ok(bb, j)
            })
            .collect();
        let j = if !cands.is_empty() && (self.rng.gen_bool(0.7) || !self.may_add_state(per_layer)) {
            pick(self.rng, &cands)
        } else if !self.may_add_state(per_layer) {
            return None;
        } else {
            self.states.push(StateDecl {
                name: format!("s{}", self.states.len()),
                kind: StateKind::Hist { window: w },
                dtype: rt.dtype,
                shape: dims.clone(),
                per_layer,
            });
            (self.states.len() - 1) as u16
        };
        let mut shape = vec![Dim::H];
        shape.extend(dims.iter().map(|&n| Dim::Fixed(n)));
        let v = self.push(bb, Prim::HistAppend { state: j }, vec![(rr, rt.clone())], TensorType { dtype: rt.dtype, shape }, false)?;
        bb.appended.insert(j);
        if !per_layer {
            self.global_writer.entry(j).or_insert(bb.b);
        }
        Some(v)
    }

    /// A committed node of exactly type `want` (committable dtype, no H).
    fn make_exact(&mut self, bb: &mut BB, want: &TensorType) -> Option<u16> {
        let dims = fixed_dims(want)?;
        for _ in 0..8 {
            // Something broadcastable to the shape, optionally combined, then clamped.
            let (v, tv) = match self.operand_where(bb, |t| {
                !has_h(t) && ref2::types::broadcast_shapes(&t.shape, &want.shape).ok().as_deref() == Some(&want.shape[..])
            }) {
                Some(v) => v,
                None => self.new_leaf(bb, None, &dims),
            };
            let (v, tv) = if tv.shape != want.shape {
                match self.push(
                    bb,
                    Prim::Broadcast,
                    vec![(v, tv.clone())],
                    TensorType { dtype: tv.dtype, shape: want.shape.clone() },
                    false,
                ) {
                    Some(x) => x,
                    None => continue,
                }
            } else {
                (v, tv)
            };
            let (v, tv) = if self.rng.gen_bool(0.5) {
                let (o, to) = self.partner(bb, &want.shape, None);
                match self.push(
                    bb,
                    Prim::Add,
                    vec![(v, tv), (o, to)],
                    TensorType { dtype: DType::I128, shape: want.shape.clone() },
                    false,
                ) {
                    Some(x) => x,
                    None => continue,
                }
            } else {
                (v, tv)
            };
            let lo = want.dtype.min().max(i64::MIN as i128) as i64;
            let hi = want.dtype.max().min(i64::MAX as i128) as i64;
            if let Some((Ref::Node(i), _)) = self.push(bb, Prim::Clamp { lo, hi }, vec![(v, tv)], want.clone(), true) {
                return Some(i);
            }
        }
        None
    }

    fn gen_block(&mut self, b: usize, role: Role, carry_in: &[TensorType], window: Option<u32>) -> BB {
        let mut bb =
            BB { b, role, window, nodes: vec![], ivs: vec![], pool: vec![], written: BTreeSet::new(), appended: BTreeSet::new() };
        for (k, t) in carry_in.iter().enumerate() {
            bb.pool.push((Ref::CarryIn(k as u8), t.clone()));
        }
        bb.pool.push((Ref::Input(0), TensorType { dtype: DType::Idx, shape: vec![] }));
        bb.pool.push((Ref::Input(1), TensorType { dtype: DType::Idx, shape: vec![] }));
        // A window needs a HistAppend; make one early so H-typed values exist.
        if window.is_some() {
            let mut ok = false;
            for _ in 0..20 {
                let r = if self.rng.gen_bool(0.7) { self.try_hist_append_vec(&mut bb) } else { self.try_hist_append(&mut bb) };
                if r.is_some() {
                    ok = true;
                    break;
                }
            }
            if ok {
                for _ in 0..self.rng.gen_range(0..=2) {
                    let _ = self.try_attention(&mut bb);
                }
            }
            if !ok {
                // No history after all: the block has no window, and no H may appear in it.
                bb.window = None;
                bb.nodes.clear();
                bb.ivs.clear();
                bb.pool.retain(|(r, _)| !matches!(r, Ref::Node(_)));
            }
        }
        let target = self.rng.gen_range(3..=self.cfg.max_nodes);
        let mut attempts = 0;
        while bb.nodes.len() < target && attempts < target * 20 {
            attempts += 1;
            let _ = self.try_prim(&mut bb);
        }
        // Random commits.
        for n in bb.nodes.iter_mut() {
            if n.out.dtype.committable() && self.rng.gen_bool(0.25) {
                n.commit = true;
            }
        }
        // At least one root, or pruning would empty the block (NF-12).
        let has_root = bb.nodes.iter().any(|n| n.commit || matches!(n.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. }));
        if !has_root {
            let shape = self.rand_fixed_shape(2);
            let want = TensorType::fixed(committable_dtype(self.rng), &shape);
            if self.make_exact(&mut bb, &want).is_none() {
                let _ =
                    self.push(&mut bb, Prim::Iota { axis: 0, start: 1, step: 1 }, vec![], TensorType::fixed(DType::I32, &[2]), true);
            }
        }
        bb
    }
}

/// Removes dead nodes of a block (NF-22), renumbering refs; `keep` are extra roots.
fn prune_block(block: &mut Block, keep: &[u16]) -> Vec<Option<u16>> {
    let n = block.nodes.len();
    let mut live = vec![false; n];
    for (i, node) in block.nodes.iter().enumerate() {
        if node.commit || matches!(node.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. }) {
            live[i] = true;
        }
    }
    for &c in block.carry_out.iter().chain(keep.iter()) {
        live[c as usize] = true;
    }
    for i in (0..n).rev() {
        if live[i] {
            for r in &block.nodes[i].inputs {
                if let Ref::Node(k) = r {
                    live[*k as usize] = true;
                }
            }
        }
    }
    let mut map = vec![None; n];
    let mut next = 0u16;
    for i in 0..n {
        if live[i] {
            map[i] = Some(next);
            next += 1;
        }
    }
    let old = std::mem::take(&mut block.nodes);
    for (i, mut node) in old.into_iter().enumerate() {
        if live[i] {
            for r in node.inputs.iter_mut() {
                if let Ref::Node(k) = r {
                    *k = map[*k as usize].unwrap();
                }
            }
            block.nodes.push(node);
        }
    }
    block.carry_out = block.carry_out.iter().map(|&c| map[c as usize].unwrap()).collect();
    map
}

/// Removes unused params, consts and states (NF-11), renumbering every reference.
fn prune_decls(p: &mut Program) {
    let mut up = vec![false; p.params.len()];
    let mut uc = vec![false; p.consts.len()];
    let mut us = vec![false; p.states.len()];
    for b in &p.blocks {
        for n in &b.nodes {
            for r in &n.inputs {
                match r {
                    Ref::Param(j) => up[*j as usize] = true,
                    Ref::Const(j) => uc[*j as usize] = true,
                    Ref::State(j) => us[*j as usize] = true,
                    _ => {}
                }
            }
            if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim {
                us[state as usize] = true;
            }
        }
    }
    let remap = |used: &[bool]| -> Vec<Option<u16>> {
        let mut m = vec![None; used.len()];
        let mut k = 0u16;
        for (i, &u) in used.iter().enumerate() {
            if u {
                m[i] = Some(k);
                k += 1;
            }
        }
        m
    };
    let (mp, mc, ms) = (remap(&up), remap(&uc), remap(&us));
    for b in p.blocks.iter_mut() {
        for n in b.nodes.iter_mut() {
            for r in n.inputs.iter_mut() {
                match r {
                    Ref::Param(j) => *j = mp[*j as usize].unwrap(),
                    Ref::Const(j) => *j = mc[*j as usize].unwrap(),
                    Ref::State(j) => *j = ms[*j as usize].unwrap(),
                    _ => {}
                }
            }
            match &mut n.prim {
                Prim::StateWrite { state } | Prim::HistAppend { state } => *state = ms[*state as usize].unwrap(),
                _ => {}
            }
        }
    }
    fn keep<T>(v: &mut Vec<T>, used: &[bool]) {
        let mut i = 0;
        v.retain(|_| {
            let k = used[i];
            i += 1;
            k
        });
    }
    keep(&mut p.params, &up);
    keep(&mut p.consts, &uc);
    keep(&mut p.states, &us);
    // Names stay unique; renumber them for readability.
    for (i, d) in p.params.iter_mut().enumerate() {
        d.name = format!("p{i}");
    }
    for (i, s) in p.states.iter_mut().enumerate() {
        s.name = format!("s{i}");
    }
}

/// Param instances for every occurrence that references each param (§3.3), random values.
pub fn gen_params(rng: &mut R, p: &Program) -> Params {
    let mut params = Params::new();
    let mut occ = vec![(p.schedule.pre, None)];
    for (l, &b) in p.schedule.layers.iter().enumerate() {
        occ.push((b, Some(l as u32)));
    }
    occ.push((p.schedule.post, None));
    for (b, layer) in occ {
        let Some(block) = p.blocks.get(b as usize) else { continue };
        for n in &block.nodes {
            for r in &n.inputs {
                if let Ref::Param(j) = *r {
                    let Some(d) = p.params.get(j as usize) else { continue };
                    let key = (j, if d.per_layer { layer } else { None });
                    if let std::collections::btree_map::Entry::Vacant(e) = params.entry(key) {
                        let shape: Vec<u64> = d.shape.iter().map(|&x| x as u64).collect();
                        if shape.iter().product::<u64>() > 4096 {
                            continue;
                        }
                        e.insert(rand_tensor(rng, d.dtype, &shape));
                    }
                }
            }
        }
    }
    params
}

pub fn gen_program(rng: &mut R, cfg: GenCfg) -> Generated {
    let history_bound = if rng.gen_bool(0.8) { 1 << 18 } else { 1 << 21 };
    let token_bound = pick(rng, &[1u32, 2, 4, 5, 8, 16, 1000, u32::MAX]);
    let nl = rng.gen_range(0..=3usize);
    let nblocks = 2 + nl;
    let mut order: Vec<usize> = (0..nblocks).collect();
    order.shuffle(rng);
    let (pre, post) = (order[0], order[1]);
    let layer_blocks: Vec<usize> = order[2..].to_vec();
    let mut layers: Vec<usize> = layer_blocks.clone();
    if nl > 0 {
        for _ in 0..rng.gen_range(0..=3) {
            layers.push(pick(rng, &layer_blocks));
        }
    }
    layers.shuffle(rng);
    let windows = [1u32, 2, 3, 5, history_bound];
    let mut g =
        G { rng, cfg, params: vec![], consts: vec![], states: vec![], history_bound, token_bound, global_writer: BTreeMap::new() };
    let mut blocks: Vec<Option<Block>> = vec![None; nblocks];
    let window_of = |g: &mut G| if g.rng.gen_bool(0.45) { Some(pick(g.rng, &windows)) } else { None };
    // pre
    let w = window_of(&mut g);
    let mut bb = g.gen_block(pre, Role::Pre, &[], w);
    let sig_n = g.rng.gen_range(0..=3usize);
    let mut sig: Vec<TensorType> = Vec::new();
    let mut carry: Vec<u16> = Vec::new();
    for _ in 0..sig_n {
        let cand: Vec<usize> =
            (0..bb.nodes.len()).filter(|&i| bb.nodes[i].out.dtype.committable() && !has_h(&bb.nodes[i].out)).collect();
        if cand.is_empty() || g.rng.gen_bool(0.2) {
            let shape = g.rand_fixed_shape(3);
            let want = TensorType::fixed(committable_dtype(g.rng), &shape);
            if let Some(i) = g.make_exact(&mut bb, &want) {
                carry.push(i);
                sig.push(want);
            }
        } else {
            let i = pick(g.rng, &cand);
            bb.nodes[i].commit = true;
            carry.push(i as u16);
            sig.push(bb.nodes[i].out.clone());
        }
    }
    blocks[pre] = Some(Block { name: "pre".into(), carry_in: vec![], nodes: bb.nodes, carry_out: carry });
    // layer blocks
    for &lb in &layer_blocks {
        let w = window_of(&mut g);
        let mut bb = g.gen_block(lb, Role::Layer, &sig, w);
        let mut carry = Vec::new();
        for t in &sig {
            match g.make_exact(&mut bb, t) {
                Some(i) => carry.push(i),
                None => {
                    // Fall back to Clamp of the carry-in itself.
                    let k = carry.len() as u8;
                    let lo = t.dtype.min().max(i64::MIN as i128) as i64;
                    let hi = t.dtype.max().min(i64::MAX as i128) as i64;
                    let (r, _) = g.push(&mut bb, Prim::Clamp { lo, hi }, vec![(Ref::CarryIn(k), t.clone())], t.clone(), true).unwrap();
                    let Ref::Node(i) = r else { unreachable!() };
                    carry.push(i);
                }
            }
        }
        blocks[lb] = Some(Block { name: format!("layer{lb}"), carry_in: sig.clone(), nodes: bb.nodes, carry_out: carry });
    }
    // post: a window needs a HistAppend, which NF-19 (revision 2) forbids in post.
    let w = if g.cfg.post_writes { window_of(&mut g) } else { None };
    let mut bb = g.gen_block(post, Role::Post, &sig, w);
    let shape = g.rand_fixed_shape(2);
    let want = TensorType::fixed(committable_dtype(g.rng), &shape);
    let logits = match g.make_exact(&mut bb, &want) {
        Some(i) => i,
        None => {
            let (r, _) =
                g.push(&mut bb, Prim::Iota { axis: 0, start: 0, step: 1 }, vec![], TensorType::fixed(DType::I32, &[2]), true).unwrap();
            let Ref::Node(i) = r else { unreachable!() };
            i
        }
    };
    let mut post_block = Block { name: "post".into(), carry_in: sig.clone(), nodes: bb.nodes, carry_out: vec![] };
    let map = prune_block(&mut post_block, &[logits]);
    let logits = map[logits as usize].unwrap();
    blocks[post] = Some(post_block);
    let mut blocks: Vec<Block> = blocks.into_iter().map(|b| b.unwrap()).collect();
    for (i, b) in blocks.iter_mut().enumerate() {
        if i != post {
            prune_block(b, &[]);
        }
    }
    let mut prog = Program {
        version: 1,
        prim_set_id: ref2::prim_set_id_v1(),
        token_bound: g.token_bound,
        history_bound: g.history_bound,
        params: g.params,
        consts: g.consts,
        states: g.states,
        blocks,
        schedule: Schedule { pre: pre as u8, layers: layers.iter().map(|&l| l as u8).collect(), post: post as u8 },
        logits,
        logits_scheme_id: [0; 64],
    };
    prune_decls(&mut prog);
    let params = gen_params(rng_of(&mut g.rng), &prog);
    Generated { prog, params }
}

fn rng_of<'a>(r: &'a mut &mut R) -> &'a mut R {
    r
}

/// A structural mutation of a program (every field class), for refusal differentials.
pub fn mutate(rng: &mut R, p: &mut Program) -> String {
    let nb = p.blocks.len();
    let b = rng.gen_range(0..nb);
    let nn = p.blocks[b].nodes.len();
    let i = rng.gen_range(0..nn);
    let k = rng.gen_range(0..40);
    let dim = |rng: &mut R| {
        pick(
            rng,
            &[Dim::Fixed(0), Dim::Fixed(1), Dim::Fixed(2), Dim::Fixed(3), Dim::Fixed(1 << 24), Dim::Fixed((1 << 24) + 1), Dim::H],
        )
    };
    match k {
        0 => p.version = pick(rng, &[0u16, 2, u16::MAX]),
        1 => p.history_bound = pick(rng, &[0u32, 1, 1 << 17, (1 << 18) + 1, 1 << 20, 1 << 22, u32::MAX]),
        2 => p.token_bound = 0,
        3 => p.schedule.pre = rng.gen_range(0..=nb as u8),
        4 => p.schedule.post = rng.gen_range(0..=nb as u8),
        5 => {
            let l = rng.gen_range(0..=nb as u8);
            p.schedule.layers.push(l)
        }
        6 => {
            if !p.schedule.layers.is_empty() {
                p.schedule.layers.pop();
            }
        }
        7 => p.logits = rng.gen_range(0..=p.blocks[p.schedule.post as usize].nodes.len() as u16),
        8 => {
            let n = &mut p.blocks[b].nodes[i];
            n.commit = !n.commit;
        }
        9 => {
            let n = &mut p.blocks[b].nodes[i];
            n.out.dtype = pick(rng, &DType::ALL);
        }
        10 => {
            let n = &mut p.blocks[b].nodes[i];
            if n.out.shape.is_empty() {
                n.out.shape.push(dim(rng));
            } else {
                let j = rng.gen_range(0..n.out.shape.len());
                n.out.shape[j] = dim(rng);
            }
        }
        11 => {
            let n = &mut p.blocks[b].nodes[i];
            if !n.inputs.is_empty() {
                let j = rng.gen_range(0..n.inputs.len());
                n.inputs[j] = match rng.gen_range(0..6) {
                    0 => Ref::Node(rng.gen_range(0..=nn as u16)),
                    1 => Ref::CarryIn(rng.gen_range(0..4)),
                    2 => Ref::Param(rng.gen_range(0..=p.params.len() as u16)),
                    3 => Ref::Const(rng.gen_range(0..=p.consts.len() as u16)),
                    4 => Ref::State(rng.gen_range(0..=p.states.len() as u16)),
                    _ => Ref::Input(rng.gen_range(0..3)),
                };
            }
        }
        12 => {
            let n = &mut p.blocks[b].nodes[i];
            n.inputs.push(Ref::Input(1));
        }
        13 => {
            let n = &mut p.blocks[b].nodes[i];
            n.inputs.pop();
        }
        14 => {
            if !p.params.is_empty() {
                let j = rng.gen_range(0..p.params.len());
                p.params[j].per_layer = !p.params[j].per_layer;
            }
        }
        15 => {
            if !p.states.is_empty() {
                let j = rng.gen_range(0..p.states.len());
                p.states[j].per_layer = !p.states[j].per_layer;
            }
        }
        16 => {
            if !p.states.is_empty() {
                let j = rng.gen_range(0..p.states.len());
                p.states[j].kind = match p.states[j].kind {
                    StateKind::Fixed { .. } => {
                        StateKind::Fixed { lo: pick(rng, &[1, -200, 0]), hi: pick(rng, &[-1, 200, 0, 1 << 40]) }
                    }
                    StateKind::Hist { .. } => StateKind::Hist { window: pick(rng, &[0, 1, 2, 7, 1 << 18, (1 << 18) + 1, 1 << 21]) },
                };
            }
        }
        17 => {
            if !p.states.is_empty() {
                let j = rng.gen_range(0..p.states.len());
                p.states[j].dtype = pick(rng, &DType::ALL);
            }
        }
        18 => p.params.push(ref2::ParamDecl { name: "unused".into(), dtype: DType::I8, shape: vec![1], per_layer: false }),
        19 => p.consts.push(ref2::ConstDecl { dtype: DType::I8, shape: vec![1], data: vec![9] }),
        20 => {
            if let Some(c) = p.consts.first().cloned() {
                p.consts.push(c);
            }
        }
        21 => {
            if !p.consts.is_empty() {
                let j = rng.gen_range(0..p.consts.len());
                if rng.gen_bool(0.5) {
                    p.consts[j].data.push(0);
                } else {
                    p.consts[j].data.pop();
                }
            }
        }
        22 => {
            if !p.params.is_empty() {
                let j = rng.gen_range(0..p.params.len());
                p.params[j].name = pick(rng, &[String::new(), "x".repeat(129), "x".repeat(128), "p0".into()]);
            }
        }
        23 => {
            if !p.states.is_empty() {
                let j = rng.gen_range(0..p.states.len());
                p.states[j].name = pick(rng, &[String::new(), "x".repeat(129), "s0".into()]);
            }
        }
        24 => p.blocks[b].name = pick(rng, &[String::new(), "x".repeat(129), "pre".into()]),
        25 => p.blocks[b].carry_out.push(rng.gen_range(0..=nn as u16)),
        26 => {
            p.blocks[b].carry_out.pop();
        }
        27 => p.blocks[b].carry_in.push(TensorType::fixed(DType::I32, &[2])),
        28 => {
            p.blocks[b].carry_in.pop();
        }
        29 => {
            // An extra dead node.
            p.blocks[b].nodes.push(ref2::Node {
                prim: Prim::Iota { axis: 0, start: 0, step: 1 },
                inputs: vec![],
                out: TensorType::fixed(DType::I32, &[2]),
                commit: false,
            });
        }
        30 => {
            // Change an attribute.
            let n = &mut p.blocks[b].nodes[i];
            let mut reverse = false;
            match n.prim {
                Prim::Transpose { ref mut perm } => {
                    if !perm.is_empty() {
                        perm[0] = perm[0].wrapping_add(1);
                    } else {
                        perm.push(0);
                    }
                }
                Prim::Slice { ref mut start, .. } => {
                    let v = pick(rng, &[start.wrapping_add(1), u32::MAX, 0]);
                    *start = v;
                }
                Prim::Concat { ref mut axis } | Prim::ReduceSum { ref mut axis } | Prim::ReduceMax { ref mut axis } => {
                    *axis = axis.wrapping_add(1)
                }
                Prim::Iota { ref mut axis, ref mut step, .. } => {
                    *axis = axis.wrapping_add(rng.gen_range(0..2));
                    *step = step.wrapping_mul(3);
                }
                Prim::Gather { ref mut axis, ref mut batch_dims } => {
                    if rng.gen_bool(0.5) {
                        *axis = axis.wrapping_add(1)
                    } else {
                        *batch_dims = batch_dims.wrapping_add(1)
                    }
                }
                Prim::Clamp { ref mut lo, ref mut hi } => std::mem::swap(lo, hi),
                Prim::TopK { ref mut k, .. } => {
                    let v = pick(rng, &[0, k.wrapping_add(1), u32::MAX]);
                    *k = v;
                }
                Prim::StateWrite { ref mut state } | Prim::HistAppend { ref mut state } => *state = state.wrapping_add(1),
                Prim::Div { ref mut rule } => *rule = pick(rng, &[Rounding::Floor, Rounding::HalfUp, Rounding::HalfAwayFromZero]),
                _ => reverse = true,
            }
            if reverse {
                n.out.shape.reverse();
            }
        }
        31 => {
            // Swap two nodes (forward refs, reordered slots).
            let j = rng.gen_range(0..nn);
            p.blocks[b].nodes.swap(i, j);
        }
        32 => {
            // Duplicate a StateWrite/HistAppend.
            if let Some(n) =
                p.blocks[b].nodes.iter().find(|n| matches!(n.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. })).cloned()
            {
                p.blocks[b].nodes.push(n);
            }
        }
        33 => {
            if !p.params.is_empty() {
                let j = rng.gen_range(0..p.params.len());
                p.params[j].dtype = pick(rng, &DType::ALL);
            }
        }
        34 => {
            if !p.params.is_empty() {
                let j = rng.gen_range(0..p.params.len());
                p.params[j].shape.push(pick(rng, &[0u32, 1, 2, 1 << 24, (1 << 24) + 1]));
            }
        }
        35 => {
            if !p.states.is_empty() {
                let j = rng.gen_range(0..p.states.len());
                p.states[j].shape.push(pick(rng, &[0u32, 1, 2]));
            }
        }
        36 => {
            let n = &mut p.blocks[b].nodes[i];
            n.out.shape.push(Dim::H);
        }
        37 => {
            // Many blocks / layers.
            p.schedule.layers.extend(std::iter::repeat_n(p.schedule.layers.first().copied().unwrap_or(0), 1025));
        }
        38 => {
            let n = &mut p.blocks[b].nodes[i];
            n.out.shape = vec![Dim::Fixed(1 << 14), Dim::Fixed(1 << 14), Dim::Fixed(2)];
        }
        _ => {
            // Remove a node (dangling refs, carries, logits).
            p.blocks[b].nodes.remove(i);
        }
    }
    format!("mutation {k} block {b} node {i}")
}
