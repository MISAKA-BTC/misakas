//! Elementwise primitives with broadcasting: `Add`, `Sub`, `Mul`, `Div`, `Compare`, `Select`, and
//! the unary `Cast`, `Clamp`, `Log2Floor`, `IntExp`, `IntRsqrt`, `IntLn`, `StateWrite`.
//!
//! Every output element is its own exact function of its operands (spec 04b §6.1), so the order
//! elements are produced in is free; each value is computed in the node's working type exactly as
//! the reference defines it.

use misaka_palw_tir::program::StateKind;
use misaka_palw_tir::{Cmp, DType, Prim, Rounding, TirError, TirErrorKind, TirResult};

use super::{Opd, Scratch, Wide, WideScratch, narrow, out_vec, overflow, widen};
use crate::elem::{Buf, Elem};
use crate::layout::{Joint, MAX_RANK, numel, row_major};
use crate::plan::{NodePlan, Work};

/// Strides of a contiguous operand of shape `s` read under broadcasting into `out`.
pub fn bcast_strides(s: &[usize], out: &[usize]) -> [usize; MAX_RANK] {
    let rm = row_major(s);
    let mut st = [0usize; MAX_RANK];
    let off = out.len() - s.len();
    for i in 0..s.len() {
        st[off + i] = if s[i] == 1 && out[off + i] != 1 { 0 } else { rm[i] };
    }
    st
}

/// One inner run of [`map2`]: `o[i] = f(a[ab + i·ia], b[bb + i·ib])`.
#[inline(always)]
fn run2<W: Wide>(o: &mut [W], a: &[W], ab: usize, ia: usize, b: &[W], bb: usize, ib: usize, f: &impl Fn(W, W) -> Option<W>) -> bool {
    let len = o.len();
    let mut ok = true;
    match (ia, ib) {
        (1, 1) => {
            for ((r, x), y) in o.iter_mut().zip(&a[ab..ab + len]).zip(&b[bb..bb + len]) {
                match f(*x, *y) {
                    Some(v) => *r = v,
                    None => ok = false,
                }
            }
        }
        (1, 0) => {
            let y = b[bb];
            for (r, x) in o.iter_mut().zip(&a[ab..ab + len]) {
                match f(*x, y) {
                    Some(v) => *r = v,
                    None => ok = false,
                }
            }
        }
        (0, 1) => {
            let x = a[ab];
            for (r, y) in o.iter_mut().zip(&b[bb..bb + len]) {
                match f(x, *y) {
                    Some(v) => *r = v,
                    None => ok = false,
                }
            }
        }
        _ => {
            for (i, r) in o.iter_mut().enumerate() {
                match f(a[ab + i * ia], b[bb + i * ib]) {
                    Some(v) => *r = v,
                    None => ok = false,
                }
            }
        }
    }
    ok
}

/// `res[i] = f(a[·], b[·])` over the broadcast index space; `false` if `f` failed anywhere.
/// Every element is independent, so a large single-run space is split across the pool.
#[inline(always)]
fn map2<W: Wide>(
    res: &mut Vec<W>,
    shape: &[usize],
    a: &[W],
    sa: &[usize],
    b: &[W],
    sb: &[usize],
    f: impl Fn(W, W) -> Option<W> + Sync,
) -> bool {
    use rayon::prelude::*;
    let n = numel(shape);
    res.clear();
    res.resize(n, W::default());
    let j = Joint::<2>::new(shape, [sa, sb]);
    if n >= *super::PAR_ELEMS
        && let Some(([ab, bb], _, [ia, ib])) = j.single_run()
    {
        let chunk = super::par_chunk(n);
        return res
            .par_chunks_mut(chunk)
            .enumerate()
            .map(|(ci, o)| {
                let c0 = ci * chunk;
                run2(o, a, ab + c0 * ia, ia, b, bb + c0 * ib, ib, &f)
            })
            .reduce(|| true, |x, y| x && y);
    }
    let mut ok = true;
    j.for_each(|pos, [ab, bb], len, [ia, ib]| {
        ok &= run2(&mut res[pos..pos + len], a, ab, ia, b, bb, ib, &f);
    });
    ok
}

/// `Add`, `Sub`, `Mul`, `Div`, `Compare`.
pub fn binary(node: &NodePlan, a: &Opd<'_>, b: &Opd<'_>, out_shape: &[usize], out: &mut Buf, scratch: &mut Scratch) -> TirResult<()> {
    match node.work {
        Work::I64 => binary_w::<i64>(node, a, b, out_shape, out, scratch),
        Work::I128 => binary_w::<i128>(node, a, b, out_shape, out, scratch),
    }
}

fn binary_w<W: Wide>(
    node: &NodePlan,
    a: &Opd<'_>,
    b: &Opd<'_>,
    out_shape: &[usize],
    out: &mut Buf,
    scratch: &mut Scratch,
) -> TirResult<()>
where
    Scratch: WideScratch<W>,
{
    let (ws, res) = scratch.parts();
    let [s0, s1, _] = ws;
    let av = widen::<W>(a, s0);
    let bv = widen::<W>(b, s1);
    let r = out_shape.len();
    let sa = bcast_strides(a.shape(), out_shape);
    let sb = bcast_strides(b.shape(), out_shape);
    let (sa, sb) = (&sa[..r], &sb[..r]);
    let name = node.prim.name();
    let ok = match &node.prim {
        Prim::Add if node.checked_arith => map2(res, out_shape, av, sa, bv, sb, |x, y| x.cadd(y)),
        Prim::Add => map2(res, out_shape, av, sa, bv, sb, |x, y| Some(x.wadd(y))),
        Prim::Sub if node.checked_arith => map2(res, out_shape, av, sa, bv, sb, |x, y| x.csub(y)),
        Prim::Sub => map2(res, out_shape, av, sa, bv, sb, |x, y| Some(x.wsub(y))),
        Prim::Mul if node.checked_arith => map2(res, out_shape, av, sa, bv, sb, |x, y| x.cmul(y)),
        Prim::Mul => map2(res, out_shape, av, sa, bv, sb, |x, y| Some(x.wmul(y))),
        Prim::Div { rule } => {
            if !div::<W>(res, out_shape, av, sa, bv, sb, *rule) {
                return Err(div_failure(out_shape, av, sa, bv, sb, *rule, node.check_out.then_some(node.out.dtype)));
            }
            true
        }
        Prim::Compare { cmp } => {
            let c = *cmp;
            map2(res, out_shape, av, sa, bv, sb, |x, y| Some(W::from_i64(holds(c, x, y) as i64)))
        }
        _ => return Err(TirError::new(TirErrorKind::Shape, format!("{name} is not binary"))),
    };
    if !ok {
        return overflow(&format!("{name}: past i128"));
    }
    narrow(res, out, node.store, node.check_out.then_some(node.out.dtype))
}

#[inline(always)]
fn holds<W: Wide>(c: Cmp, a: W, b: W) -> bool {
    match c {
        Cmp::Eq => a == b,
        Cmp::Ne => a != b,
        Cmp::Lt => a < b,
        Cmp::Le => a <= b,
        Cmp::Gt => a > b,
        Cmp::Ge => a >= b,
    }
}

/// The class of a failed `Div`: that of its first failing element in output order, as the
/// reference reports it — a divisor below 1 (`Divisor`) or a quotient outside the dtype
/// (`Overflow`, when `check` names the dtype). Only on the failure path.
fn div_failure<W: Wide>(
    shape: &[usize],
    x: &[W],
    sx: &[usize],
    d: &[W],
    sd: &[usize],
    rule: Rounding,
    check: Option<DType>,
) -> TirError {
    let mut first: Option<TirError> = None;
    Joint::<2>::new(shape, [sx, sd]).for_each(|_, [xb, db], len, [ix, id]| {
        for i in 0..len {
            if first.is_some() {
                return;
            }
            let dv = d[db + i * id];
            if dv < W::from_i64(1) {
                first = Some(TirError::new(TirErrorKind::Divisor, format!("Div: divisor {dv:?} < 1")));
            } else if let Some(dt) = check {
                let q = W::div_rule(x[xb + i * ix], dv, rule).to_i128();
                if !dt.contains(q) {
                    first = Some(TirError::new(TirErrorKind::Overflow, format!("Div: {q} does not fit {}", dt.name())));
                }
            }
        }
    });
    first.unwrap_or_else(|| TirError::new(TirErrorKind::Divisor, "Div: a divisor below 1"))
}

/// `Div` over the broadcast space; `false` when a divisor is below 1. A single divisor that is a
/// power of two — every right shift of a lowered program — becomes one shift loop.
fn div<W: Wide>(res: &mut Vec<W>, shape: &[usize], x: &[W], sx: &[usize], d: &[W], sd: &[usize], rule: Rounding) -> bool {
    if d.len() == 1 {
        let dv = d[0];
        if dv < W::from_i64(1) {
            return false;
        }
        return match W::pow2_shift(dv) {
            Some(s) => map2(res, shape, x, sx, d, sd, move |a, _| Some(W::shr_rule(a, s, rule))),
            None => map2(res, shape, x, sx, d, sd, move |a, _| Some(W::div_rule(a, dv, rule))),
        };
    }
    let one = W::from_i64(1);
    map2(res, shape, x, sx, d, sd, move |a, dv| if dv < one { None } else { Some(W::div_rule(a, dv, rule)) })
}

/// `Select(c, a, b)`.
pub fn select(
    node: &NodePlan,
    c: &Opd<'_>,
    a: &Opd<'_>,
    b: &Opd<'_>,
    out_shape: &[usize],
    out: &mut Buf,
    scratch: &mut Scratch,
) -> TirResult<()> {
    match node.work {
        Work::I64 => select_w::<i64>(node, c, a, b, out_shape, out, scratch),
        Work::I128 => select_w::<i128>(node, c, a, b, out_shape, out, scratch),
    }
}

fn select_w<W: Wide>(
    node: &NodePlan,
    c: &Opd<'_>,
    a: &Opd<'_>,
    b: &Opd<'_>,
    out_shape: &[usize],
    out: &mut Buf,
    scratch: &mut Scratch,
) -> TirResult<()>
where
    Scratch: WideScratch<W>,
{
    let (ws, res) = scratch.parts();
    let [s0, s1, s2] = ws;
    let cv = widen::<W>(c, s0);
    let av = widen::<W>(a, s1);
    let bv = widen::<W>(b, s2);
    let r = out_shape.len();
    let sc = bcast_strides(c.shape(), out_shape);
    let sa = bcast_strides(a.shape(), out_shape);
    let sb = bcast_strides(b.shape(), out_shape);
    let n = numel(out_shape);
    res.clear();
    res.resize(n, W::default());
    let zero = W::default();
    Joint::<3>::new(out_shape, [&sc[..r], &sa[..r], &sb[..r]]).for_each(|pos, [cb, ab, bb], len, [ic, ia, ib]| {
        for (i, o) in res[pos..pos + len].iter_mut().enumerate() {
            *o = if cv[cb + i * ic] != zero { av[ab + i * ia] } else { bv[bb + i * ib] };
        }
    });
    narrow(res, out, node.store, node.check_out.then_some(node.out.dtype))
}

/// The unary primitives: out shape = in shape, one value per element.
pub fn unary(
    node: &NodePlan,
    x: &Opd<'_>,
    out: &mut Buf,
    scratch: &mut Scratch,
    states: &[misaka_palw_tir::StateDecl],
) -> TirResult<()> {
    match node.work {
        Work::I64 => unary_w::<i64>(node, x, out, scratch, states),
        Work::I128 => unary_w::<i128>(node, x, out, scratch, states),
    }
}

fn unary_w<W: Wide>(
    node: &NodePlan,
    x: &Opd<'_>,
    out: &mut Buf,
    scratch: &mut Scratch,
    states: &[misaka_palw_tir::StateDecl],
) -> TirResult<()>
where
    Scratch: WideScratch<W>,
{
    let (ws, _) = scratch.parts();
    let [s0, _, _] = ws;
    let xv = widen::<W>(x, s0);
    let dt = node.store;
    let check = node.check_out.then_some(node.out.dtype);
    match &node.prim {
        Prim::Cast => write_mapped(out, dt, xv, check, |v| v),
        Prim::Clamp { lo, hi } => {
            let (lo, hi) = (*lo as i128, *hi as i128);
            write_mapped(out, dt, xv, None, move |v| v.clamp_i128(lo, hi))
        }
        Prim::StateWrite { state } => {
            let StateKind::Fixed { lo, hi } = states[*state as usize].kind else {
                return Err(TirError::new(TirErrorKind::Shape, "StateWrite on a Hist state"));
            };
            let (lo, hi) = (lo as i128, hi as i128);
            write_mapped(out, dt, xv, None, move |v| v.clamp_i128(lo, hi))
        }
        Prim::Log2Floor => write_mapped(out, dt, xv, check, W::log2_floor),
        Prim::IntExp => write_mapped(out, dt, xv, check, W::int_exp),
        Prim::IntRsqrt => write_mapped(out, dt, xv, check, W::int_rsqrt),
        Prim::IntLn => write_mapped(out, dt, xv, check, W::int_ln),
        other => Err(TirError::new(TirErrorKind::Shape, format!("{} is not unary", other.name()))),
    }
}

/// `out[i] = f(x[i])`, stored as `store`, checking the declared dtype when `check` names it.
fn write_mapped<W: Wide>(out: &mut Buf, store: DType, x: &[W], check: Option<DType>, f: impl Fn(W) -> W + Sync) -> TirResult<()> {
    crate::with_dtype!(store, T => write_mapped_t::<W, T>(out_vec::<T>(out), x, check, f))
}

#[inline(always)]
fn write_mapped_t<W: Wide, T: Elem>(o: &mut Vec<T>, x: &[W], check: Option<DType>, f: impl Fn(W) -> W + Sync) -> TirResult<()> {
    use rayon::prelude::*;
    o.clear();
    o.resize(x.len(), T::default());
    let (lo, hi) = check.map(W::bounds).unwrap_or((W::WMIN, W::WMAX));
    let body = |o: &mut [T], x: &[W]| -> bool {
        let mut bad = false;
        for (r, v) in o.iter_mut().zip(x) {
            let y = f(*v);
            bad |= y < lo || y > hi;
            *r = T::from_i128(y.to_i128());
        }
        bad
    };
    let bad = if x.len() >= *super::PAR_ELEMS {
        let chunk = super::par_chunk(x.len());
        o.par_chunks_mut(chunk).zip(x.par_chunks(chunk)).map(|(o, x)| body(o, x)).reduce(|| false, |a, b| a || b)
    } else {
        body(o, x)
    };
    if bad {
        return overflow(&format!("a result outside {}", check.map(|d| d.name()).unwrap_or("its dtype")));
    }
    Ok(())
}
