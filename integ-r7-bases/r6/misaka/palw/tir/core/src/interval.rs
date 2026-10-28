//! Interval types and the range transfer functions of spec 04b §7 (PALW-TIR-9).
//!
//! Every tensor gets one integer interval `[lo, hi]` covering every element at every position. The
//! transfer function of each primitive is sound for every partial sum in every order (the MatMul and
//! ReduceSum rules bound the sum of the positive and of the negative terms separately), and each
//! primitive carries its obligations: the output interval must lie inside the declared dtype (the
//! exact-result rule can then never fire), a `Div`'s divisor interval must be `≥ 1`, a `Gather`'s
//! index interval must lie inside the gathered axis.
//!
//! This module is the range half of `tir_admit_v1`; admission (Gate 2) adds costs, court cones and
//! the size ceilings. [`analyze_ranges`] runs it over a validated program: after it succeeds, no
//! exact primitive of the program can overflow on any params, any token and any position — which
//! `tests/intervals.rs` checks against the reference evaluator.

use crate::arith::{EXP_ZERO_AT, LN2_Q, ONE, div_round, int_exp_max, log2_floor};
use crate::error::{TirError, TirErrorKind, TirResult, err};
use crate::prim::Prim;
use crate::program::{Ref, StateKind, TirProgramV1};
use crate::types::{DType, Dim, TensorType};
use crate::validate::validate;

/// `IntRsqrt(1)`, the maximum of `IntRsqrt` over every input (spec 04b §6.5).
pub const INT_RSQRT_MAX: i128 = 68_719_472_640;

/// An integer interval `[lo, hi]`, `lo ≤ hi`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Interval {
    pub lo: i128,
    pub hi: i128,
}

impl Interval {
    pub const fn new(lo: i128, hi: i128) -> Self {
        Self { lo, hi }
    }
    pub const fn point(v: i128) -> Self {
        Self { lo: v, hi: v }
    }
    pub const fn of(d: DType) -> Self {
        Self { lo: d.min_value(), hi: d.max_value() }
    }
    pub fn union(self, o: Self) -> Self {
        Self { lo: self.lo.min(o.lo), hi: self.hi.max(o.hi) }
    }
    pub fn within(self, d: DType) -> bool {
        d.contains(self.lo) && d.contains(self.hi)
    }
    pub fn contains(self, v: i128) -> bool {
        self.lo <= v && v <= self.hi
    }
}

fn overflow<T>(what: &str) -> TirResult<T> {
    err(TirErrorKind::Overflow, format!("range: {what}"))
}

fn fits(i: Interval, d: DType, what: &str) -> TirResult<Interval> {
    if i.within(d) { Ok(i) } else { overflow(&format!("{what} [{}, {}] is not inside {}", i.lo, i.hi, d.name())) }
}

fn corners(a: Interval, b: Interval, f: impl Fn(i128, i128) -> Option<i128>) -> Option<Interval> {
    let vs = [f(a.lo, b.lo)?, f(a.lo, b.hi)?, f(a.hi, b.lo)?, f(a.hi, b.hi)?];
    Some(Interval::new(*vs.iter().min().unwrap(), *vs.iter().max().unwrap()))
}

/// The worst-case extent of a dimension.
fn extent(d: Dim, window: Option<u32>) -> i128 {
    match d {
        Dim::Fixed(n) => n as i128,
        Dim::H => window.unwrap_or(1) as i128,
    }
}

/// `[n·min(lo, 0), n·max(hi, 0)]`: every partial sum of `n` terms from `t`, in every order.
fn partial_sums(t: Interval, n: i128) -> Option<Interval> {
    Some(Interval::new(t.lo.min(0).checked_mul(n)?, t.hi.max(0).checked_mul(n)?))
}

/// One node's output interval from its operands' intervals and types (spec 04b §7), or the
/// obligation that fails.
pub fn transfer(
    prim: &Prim,
    ins: &[Interval],
    in_types: &[TensorType],
    out: &TensorType,
    window: Option<u32>,
    program: &TirProgramV1,
) -> TirResult<Interval> {
    let name = prim.name();
    let o = out.dtype;
    match prim {
        Prim::Reshape | Prim::Transpose { .. } | Prim::Slice { .. } | Prim::Broadcast | Prim::ReduceMax { .. } => Ok(ins[0]),
        Prim::Concat { .. } => Ok(ins.iter().copied().reduce(Interval::union).expect("arity ≥ 2")),
        Prim::Iota { axis, start, step } => {
            let n = extent(out.shape[*axis as usize], window) - 1;
            let last = (*step as i128).checked_mul(n).and_then(|v| v.checked_add(*start as i128));
            let Some(last) = last else { return overflow(name) };
            fits(Interval::new((*start as i128).min(last), (*start as i128).max(last)), o, name)
        }
        Prim::Gather { axis, .. } => {
            let d = &in_types[0];
            let n = extent(d.shape[*axis as usize], window);
            if ins[1].lo < 0 || ins[1].hi >= n {
                return err(TirErrorKind::Index, format!("range: gather indices [{}, {}] outside [0, {})", ins[1].lo, ins[1].hi, n));
            }
            Ok(ins[0])
        }
        Prim::Cast => fits(ins[0], o, name),
        Prim::Add => match (ins[0].lo.checked_add(ins[1].lo), ins[0].hi.checked_add(ins[1].hi)) {
            (Some(lo), Some(hi)) => fits(Interval::new(lo, hi), o, name),
            _ => overflow(name),
        },
        Prim::Sub => match (ins[0].lo.checked_sub(ins[1].hi), ins[0].hi.checked_sub(ins[1].lo)) {
            (Some(lo), Some(hi)) => fits(Interval::new(lo, hi), o, name),
            _ => overflow(name),
        },
        Prim::Mul => match corners(ins[0], ins[1], |a, b| a.checked_mul(b)) {
            Some(i) => fits(i, o, name),
            None => overflow(name),
        },
        Prim::MatMul => {
            let a = &in_types[0];
            let k = extent(a.shape[a.rank() - 1], window);
            let Some(t) = corners(ins[0], ins[1], |a, b| a.checked_mul(b)) else { return overflow(name) };
            match partial_sums(t, k) {
                Some(i) => fits(i, o, name),
                None => overflow(name),
            }
        }
        Prim::ReduceSum { axis } => {
            let n = extent(in_types[0].shape[*axis as usize], window);
            match partial_sums(ins[0], n) {
                Some(i) => fits(i, o, name),
                None => overflow(name),
            }
        }
        Prim::Div { rule } => {
            if ins[1].lo < 1 {
                return err(TirErrorKind::Divisor, format!("range: divisor interval [{}, {}] reaches below 1", ins[1].lo, ins[1].hi));
            }
            match corners(ins[0], ins[1], |x, d| div_round(x, d, *rule)) {
                Some(i) => fits(i, o, name),
                None => overflow(name),
            }
        }
        Prim::Clamp { lo, hi } => {
            let c = |v: i128| v.clamp(*lo as i128, *hi as i128);
            Ok(Interval::new(c(ins[0].lo), c(ins[0].hi)))
        }
        Prim::Log2Floor => fits(Interval::new(log2_floor(ins[0].lo), log2_floor(ins[0].hi)), o, name),
        Prim::IntExp => fits(if ins[0].hi <= EXP_ZERO_AT { Interval::point(0) } else { Interval::new(0, int_exp_max()) }, o, name),
        Prim::IntRsqrt => {
            // `IntRsqrt(v) = y · 2^(−e(v))` with the Newton value `y ∈ [1, ONE]` (checked
            // exhaustively, `tests/transcendental_ranges.rs`) and `e(v) = ⌊(Log2Floor(v) − 24)/2⌋`
            // non-decreasing in `v`: so `ONE · 2^(−e(max(lo, 1)))` bounds every input of the interval.
            let i = if ins[0].hi <= 0 {
                Interval::point(0)
            } else {
                let e = (log2_floor(ins[0].lo.max(1)) - 24).div_euclid(2);
                let hi = if e >= 0 { ONE >> e } else { ONE << (-e) };
                Interval::new(0, hi.min(INT_RSQRT_MAX))
            };
            fits(i, o, name)
        }
        Prim::IntLn => {
            let x = ins[0];
            let i = if x.hi <= 0 {
                Interval::point(0)
            } else {
                let s_lo = log2_floor(x.lo.max(1)) - 24;
                let s_hi = log2_floor(x.hi) - 24;
                let mut i = Interval::new(s_lo * LN2_Q, (s_hi + 1) * LN2_Q - 1);
                if x.lo <= 0 {
                    i = i.union(Interval::point(0));
                }
                i
            };
            fits(i, o, name)
        }
        Prim::Compare { .. } => Ok(Interval::new(0, 1)),
        Prim::Select => fits(ins[1].union(ins[2]), o, name),
        Prim::TopK { axis, .. } => Ok(Interval::new(0, extent(in_types[0].shape[*axis as usize], window) - 1)),
        Prim::StateWrite { state } => {
            let Some(StateKind::Fixed { lo, hi }) = program.states.get(*state as usize).map(|s| s.kind) else {
                return err(TirErrorKind::Shape, "range: StateWrite names no Fixed state");
            };
            let c = |v: i128| v.clamp(lo as i128, hi as i128);
            Ok(Interval::new(c(ins[0].lo), c(ins[0].hi)))
        }
        Prim::HistAppend { .. } => Ok(ins[0]),
    }
}

/// The interval of every node of every block (spec 04b §7), or the first unmet obligation. The
/// program must be in normal form (it is validated first).
pub fn analyze_ranges(p: &TirProgramV1) -> TirResult<Vec<Vec<Interval>>> {
    let info = validate(p)?;
    let mut out = Vec::with_capacity(p.blocks.len());
    for (bi, b) in p.blocks.iter().enumerate() {
        let window = info.blocks[bi].window;
        let mut iv: Vec<Interval> = Vec::with_capacity(b.nodes.len());
        for (ni, n) in b.nodes.iter().enumerate() {
            let mut ins = Vec::with_capacity(n.inputs.len());
            let mut tys = Vec::with_capacity(n.inputs.len());
            for r in &n.inputs {
                let (i, t) = match *r {
                    Ref::Node(j) => (iv[j as usize], b.nodes[j as usize].out.clone()),
                    Ref::CarryIn(k) => (Interval::of(b.carry_in[k as usize].dtype), b.carry_in[k as usize].clone()),
                    Ref::Param(j) => {
                        let d = &p.params[j as usize];
                        (Interval::of(d.dtype), TensorType::fixed(d.dtype, &d.shape))
                    }
                    Ref::Const(j) => {
                        let c = &p.consts[j as usize];
                        let vals: Vec<i128> = c.data.chunks_exact(c.dtype.width()).map(|e| c.dtype.decode_le(e)).collect();
                        (Interval::new(*vals.iter().min().unwrap(), *vals.iter().max().unwrap()), TensorType::fixed(c.dtype, &c.shape))
                    }
                    Ref::State(j) => {
                        let s = &p.states[j as usize];
                        let StateKind::Fixed { lo, hi } = s.kind else { unreachable!("validated") };
                        (Interval::new(lo as i128, hi as i128), TensorType::fixed(s.dtype, &s.shape))
                    }
                    Ref::Input(0) => (Interval::new(0, p.token_bound as i128 - 1), TensorType::scalar(DType::Idx)),
                    Ref::Input(_) => (Interval::new(0, p.history_bound as i128 - 1), TensorType::scalar(DType::Idx)),
                };
                ins.push(i);
                tys.push(t);
            }
            let r = transfer(&n.prim, &ins, &tys, &n.out, window, p)
                .map_err(|e| TirError::new(e.kind, format!("block {bi} node {ni} ({}): {}", n.prim.name(), e.msg)))?;
            iv.push(r);
        }
        out.push(iv);
    }
    Ok(out)
}
