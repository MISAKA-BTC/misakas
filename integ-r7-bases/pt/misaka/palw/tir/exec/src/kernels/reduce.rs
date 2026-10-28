//! `ReduceSum` and `ReduceMax` along one axis (kept, extent 1). `ReduceSum` is an exact sum with
//! the order-free rule (PALW-TIR-24): under the plan's `Fast*` proof any association is the value;
//! otherwise the positive and negative terms are accumulated apart and checked as the reference
//! does.

use misaka_palw_tir::{Prim, TirError, TirErrorKind, TirResult};

use super::{Opd, Scratch, Wide, WideScratch, narrow, widen};
use crate::elem::Buf;
use crate::plan::{Acc, NodePlan, Work};

/// `(outer, n, inner)` of a reduction along `axis` of `shape`.
fn split(shape: &[usize], axis: usize) -> (usize, usize, usize) {
    (shape[..axis].iter().product(), shape[axis], shape[axis + 1..].iter().product())
}

pub fn reduce(node: &NodePlan, x: &Opd<'_>, out: &mut Buf, scratch: &mut Scratch) -> TirResult<()> {
    match node.prim {
        Prim::ReduceSum { axis } => match node.acc {
            Acc::Fast64 => sum_fast::<i64>(node, x, axis as usize, out, scratch),
            Acc::Fast128 => sum_fast::<i128>(node, x, axis as usize, out, scratch),
            Acc::Pn64 | Acc::Pn128 => sum_checked(node, x, axis as usize, out, scratch),
        },
        Prim::ReduceMax { axis } => match node.work {
            Work::I64 => max::<i64>(node, x, axis as usize, out, scratch),
            Work::I128 => max::<i128>(node, x, axis as usize, out, scratch),
        },
        _ => Err(TirError::new(TirErrorKind::Shape, "not a reduction")),
    }
}

fn sum_fast<W: Wide>(node: &NodePlan, x: &Opd<'_>, axis: usize, out: &mut Buf, scratch: &mut Scratch) -> TirResult<()>
where
    Scratch: WideScratch<W>,
{
    let (ws, res) = scratch.parts();
    let [s0, _, _] = ws;
    let xv = widen::<W>(x, s0);
    let (outer, n, inner) = split(x.shape(), axis);
    res.clear();
    res.resize(outer * inner, W::default());
    for o in 0..outer {
        let dst = &mut res[o * inner..(o + 1) * inner];
        let base = o * n * inner;
        if inner == 1 {
            dst[0] = xv[base..base + n].iter().fold(W::default(), |s, v| s.wadd(*v));
        } else {
            dst.copy_from_slice(&xv[base..base + inner]);
            for t in 1..n {
                let row = &xv[base + t * inner..base + (t + 1) * inner];
                for (d, v) in dst.iter_mut().zip(row) {
                    *d = d.wadd(*v);
                }
            }
        }
    }
    narrow(res, out, node.store, None)
}

fn sum_checked(node: &NodePlan, x: &Opd<'_>, axis: usize, out: &mut Buf, scratch: &mut Scratch) -> TirResult<()> {
    let (ws, res) = <Scratch as WideScratch<i128>>::parts(scratch);
    let [s0, _, _] = ws;
    let xv = widen::<i128>(x, s0);
    let (outer, n, inner) = split(x.shape(), axis);
    let dt = node.out.dtype;
    let (lo, hi) = (dt.min_value(), dt.max_value());
    res.clear();
    res.resize(outer * inner, 0);
    for o in 0..outer {
        for i in 0..inner {
            let (mut pos, mut neg) = (0i128, 0i128);
            for t in 0..n {
                let v = xv[(o * n + t) * inner + i];
                if v > 0 {
                    pos = pos.checked_add(v).filter(|p| *p <= hi).ok_or_else(over)?;
                } else {
                    neg = neg.checked_add(v).filter(|q| *q >= lo).ok_or_else(over)?;
                }
            }
            res[o * inner + i] = pos + neg;
        }
    }
    narrow(res, out, node.store, None)
}

fn over() -> TirError {
    TirError::new(TirErrorKind::Overflow, "ReduceSum: a partial sum leaves the declared type")
}

fn max<W: Wide>(node: &NodePlan, x: &Opd<'_>, axis: usize, out: &mut Buf, scratch: &mut Scratch) -> TirResult<()>
where
    Scratch: WideScratch<W>,
{
    let (ws, res) = scratch.parts();
    let [s0, _, _] = ws;
    let xv = widen::<W>(x, s0);
    let (outer, n, inner) = split(x.shape(), axis);
    res.clear();
    res.resize(outer * inner, W::default());
    for o in 0..outer {
        let dst = &mut res[o * inner..(o + 1) * inner];
        let base = o * n * inner;
        dst.copy_from_slice(&xv[base..base + inner]);
        for t in 1..n {
            let row = &xv[base + t * inner..base + (t + 1) * inner];
            for (d, v) in dst.iter_mut().zip(row) {
                if *v > *d {
                    *d = *v;
                }
            }
        }
    }
    narrow(res, out, node.store, None)
}
