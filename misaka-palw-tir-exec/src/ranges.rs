//! Per-node intervals that decide how a node may be computed (not whether a program is admitted).
//!
//! The reference evaluator checks every exact result against its dtype (PALW-TIR-23/24) and fails
//! the step where one does not fit. This backend must fail on exactly the same inputs — but it
//! must not pay for a check where none can fire. So every node gets two intervals, derived with
//! the transfer rules of spec 04b §7 (the rules `misaka_palw_tir::interval` implements and
//! `misaka-palw-tir/tests/intervals.rs` tests sound):
//!
//! * `exact` — every mathematically exact result the primitive can produce from operands inside
//!   their intervals (for `MatMul` and `ReduceSum`: every partial sum in every order), or `None`
//!   when that interval does not fit `i128`;
//! * `out` — the values the node can hold after it SUCCEEDED: `exact ∩ dtype`, because a result
//!   outside the dtype fails the step and nothing downstream runs.
//!
//! Unlike admission, nothing here refuses a program: a node whose obligation is unmet is simply
//! computed with the reference's runtime checks. Leaves take the intervals of spec 04b §7 — params
//! and carry-ins their dtype's full range, states their declared `[lo, hi]`, the token
//! `[0, token_bound − 1]`, the position `[0, history_bound − 1]` — each of which the executor
//! enforces on every value it holds, so the intervals are sound for the values it computes on.

use misaka_palw_tir::arith::{EXP_ZERO_AT, LN2_Q, ONE, div_round, int_exp_max, log2_floor};
use misaka_palw_tir::interval::{INT_RSQRT_MAX, Interval};
use misaka_palw_tir::program::{StateKind, TirProgramV1};
use misaka_palw_tir::{DType, Dim, Prim, TensorType};

pub fn dtype_iv(d: DType) -> Interval {
    Interval::of(d)
}

pub const I64_IV: Interval = Interval { lo: i64::MIN as i128, hi: i64::MAX as i128 };

pub fn within(i: Interval, o: Interval) -> bool {
    o.lo <= i.lo && i.hi <= o.hi
}

pub fn in_i64(i: Interval) -> bool {
    within(i, I64_IV)
}

/// `a ∩ b`, or `None` when empty.
pub fn meet(a: Interval, b: Interval) -> Option<Interval> {
    let lo = a.lo.max(b.lo);
    let hi = a.hi.min(b.hi);
    (lo <= hi).then_some(Interval::new(lo, hi))
}

fn corners(a: Interval, b: Interval, f: impl Fn(i128, i128) -> Option<i128>) -> Option<Interval> {
    let vs = [f(a.lo, b.lo)?, f(a.lo, b.hi)?, f(a.hi, b.lo)?, f(a.hi, b.hi)?];
    Some(Interval::new(*vs.iter().min()?, *vs.iter().max()?))
}

/// Every partial sum of `n` terms from `t`, in every order.
fn partial_sums(t: Interval, n: i128) -> Option<Interval> {
    Some(Interval::new(t.lo.min(0).checked_mul(n)?, t.hi.max(0).checked_mul(n)?))
}

fn extent(d: Dim, window: Option<u32>) -> i128 {
    match d {
        Dim::Fixed(n) => n as i128,
        Dim::H => window.unwrap_or(1) as i128,
    }
}

/// What the interval rules say about one node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    /// The exact results (see the module note); `None` when unbounded in `i128`.
    pub exact: Option<Interval>,
    /// The values the node holds when it succeeded.
    pub out: Interval,
    /// A `Gather` whose index interval is not inside its axis, or a `Div` whose divisor interval
    /// reaches below 1: the per-element check stays.
    pub needs_operand_check: bool,
}

impl Facts {
    fn of(exact: Option<Interval>, dtype: DType) -> Self {
        let d = dtype_iv(dtype);
        // An empty meet means the node can never succeed; any interval is then sound downstream.
        let out = exact.and_then(|e| meet(e, d)).unwrap_or(d);
        Facts { exact, out, needs_operand_check: false }
    }

    /// Does every exact result fit the declared dtype, so the result check can never fire?
    pub fn fits(&self, dtype: DType) -> bool {
        self.exact.is_some_and(|e| within(e, dtype_iv(dtype)))
    }
}

/// The interval of a leaf operand (spec 04b §7, "Leaves").
pub fn leaf_interval(p: &TirProgramV1, r: &misaka_palw_tir::Ref) -> Option<Interval> {
    use misaka_palw_tir::Ref;
    Some(match *r {
        Ref::Param(j) => dtype_iv(p.params[j as usize].dtype),
        Ref::Const(j) => {
            let c = &p.consts[j as usize];
            let vals = c.data.chunks_exact(c.dtype.width()).map(|e| c.dtype.decode_le(e));
            let (lo, hi) = vals.fold((i128::MAX, i128::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            Interval::new(lo, hi)
        }
        Ref::State(j) => match p.states[j as usize].kind {
            StateKind::Fixed { lo, hi } => Interval::new(lo as i128, hi as i128),
            StateKind::Hist { .. } => dtype_iv(p.states[j as usize].dtype),
        },
        Ref::Input(0) => Interval::new(0, p.token_bound as i128 - 1),
        Ref::Input(_) => Interval::new(0, p.history_bound as i128 - 1),
        Ref::Node(_) | Ref::CarryIn(_) => return None,
    })
}

/// The facts of one node from its operands' `out` intervals and types.
pub fn node_facts(
    prim: &Prim,
    ins: &[Interval],
    in_types: &[TensorType],
    out: &TensorType,
    window: Option<u32>,
    p: &TirProgramV1,
) -> Facts {
    let o = out.dtype;
    match prim {
        Prim::Reshape | Prim::Transpose { .. } | Prim::Slice { .. } | Prim::Broadcast | Prim::ReduceMax { .. } => {
            Facts::of(Some(ins[0]), o)
        }
        Prim::Concat { .. } => Facts::of(ins.iter().copied().reduce(Interval::union), o),
        Prim::Iota { axis, start, step } => {
            let n = extent(out.shape[*axis as usize], window) - 1;
            // |step·n| ≤ 2^63·2^24: always an i128.
            let last = *start as i128 + *step as i128 * n;
            Facts::of(Some(Interval::new((*start as i128).min(last), (*start as i128).max(last))), o)
        }
        Prim::Gather { axis, .. } => {
            let n = extent(in_types[0].shape[*axis as usize], window);
            let mut f = Facts::of(Some(ins[0]), o);
            f.needs_operand_check = ins[1].lo < 0 || ins[1].hi >= n;
            f
        }
        Prim::Cast => Facts::of(Some(ins[0]), o),
        Prim::Add => {
            Facts::of(ins[0].lo.checked_add(ins[1].lo).zip(ins[0].hi.checked_add(ins[1].hi)).map(|(l, h)| Interval::new(l, h)), o)
        }
        Prim::Sub => {
            Facts::of(ins[0].lo.checked_sub(ins[1].hi).zip(ins[0].hi.checked_sub(ins[1].lo)).map(|(l, h)| Interval::new(l, h)), o)
        }
        Prim::Mul => Facts::of(corners(ins[0], ins[1], |a, b| a.checked_mul(b)), o),
        Prim::MatMul => {
            let a = &in_types[0];
            let k = extent(a.shape[a.rank() - 1], window);
            Facts::of(corners(ins[0], ins[1], |a, b| a.checked_mul(b)).and_then(|t| partial_sums(t, k)), o)
        }
        Prim::ReduceSum { axis } => {
            let n = extent(in_types[0].shape[*axis as usize], window);
            Facts::of(partial_sums(ins[0], n), o)
        }
        Prim::Div { rule } => {
            let d = ins[1];
            if d.hi < 1 {
                // Every divisor is below 1: the node fails whenever it runs.
                let mut f = Facts::of(None, o);
                f.needs_operand_check = true;
                return f;
            }
            let dd = Interval::new(d.lo.max(1), d.hi);
            let mut f = Facts::of(corners(ins[0], dd, |x, d| div_round(x, d, *rule)), o);
            f.needs_operand_check = d.lo < 1;
            f
        }
        Prim::Clamp { lo, hi } => {
            let c = |v: i128| v.clamp(*lo as i128, *hi as i128);
            Facts::of(Some(Interval::new(c(ins[0].lo), c(ins[0].hi))), o)
        }
        Prim::Log2Floor => Facts::of(Some(Interval::new(log2_floor(ins[0].lo), log2_floor(ins[0].hi))), o),
        Prim::IntExp => {
            Facts::of(Some(if ins[0].hi <= EXP_ZERO_AT { Interval::point(0) } else { Interval::new(0, int_exp_max()) }), o)
        }
        Prim::IntRsqrt => {
            let i = if ins[0].hi <= 0 {
                Interval::point(0)
            } else {
                let e = (log2_floor(ins[0].lo.max(1)) - 24).div_euclid(2);
                let hi = if e >= 0 { ONE >> e } else { ONE << (-e) };
                Interval::new(0, hi.min(INT_RSQRT_MAX))
            };
            Facts::of(Some(i), o)
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
            Facts::of(Some(i), o)
        }
        Prim::Compare { .. } => Facts::of(Some(Interval::new(0, 1)), o),
        Prim::Select => Facts::of(Some(ins[1].union(ins[2])), o),
        Prim::TopK { axis, .. } => Facts::of(Some(Interval::new(0, extent(in_types[0].shape[*axis as usize], window) - 1)), o),
        Prim::StateWrite { state } => match p.states[*state as usize].kind {
            StateKind::Fixed { lo, hi } => {
                let c = |v: i128| v.clamp(lo as i128, hi as i128);
                Facts::of(Some(Interval::new(c(ins[0].lo), c(ins[0].hi))), o)
            }
            StateKind::Hist { .. } => Facts::of(None, o),
        },
        // The window holds rows of earlier positions too, possibly from another appender of the
        // same global history: the state's dtype is the interval that covers every one of them.
        Prim::HistAppend { state } => Facts::of(Some(dtype_iv(p.states[*state as usize].dtype)), o),
    }
}
