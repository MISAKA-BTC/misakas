//! `Gather`, `TopK`, `Concat` and `Iota`.

use misaka_palw_tir::{DType, Prim, TirError, TirErrorKind, TirResult};

use super::{Opd, Scratch, out_vec, overflow, widen};
use crate::elem::{Buf, Elem, Slice};
use crate::layout::{Joint, Layout, numel, row_major};
use crate::plan::NodePlan;
use crate::with_slice;
use std::{cmp::Reverse, collections::BinaryHeap};

fn index_err(v: i64, n: usize) -> TirError {
    TirError::new(TirErrorKind::Index, format!("Gather: index {v} outside [0, {n})"))
}

/// `Gather { axis, batch_dims }` (spec 04b §6.2), copying. (A rank-0 index along a non-batch
/// axis is a view instead — the executor's business.)
pub fn gather(node: &NodePlan, data: &Opd<'_>, idx: &Opd<'_>, out: &mut Buf, scratch: &mut Scratch) -> TirResult<()> {
    let Prim::Gather { axis, batch_dims } = node.prim else { return Err(TirError::new(TirErrorKind::Shape, "not a Gather")) };
    let (a, b) = (axis as usize, batch_dims as usize);
    // Indices are never i128 (type rule); idx and every narrower type fit i64.
    let [s0, _, _] = &mut scratch.w64;
    let iv = widen::<i64>(idx, s0);
    let dsh = data.shape();
    let ish = idx.shape();
    let n = dsh[a];
    if node.check_operand
        && let Some(bad) = iv.iter().find(|v| **v < 0 || **v as u128 >= n as u128)
    {
        return Err(index_err(*bad, n));
    }
    let pre: usize = dsh[..a].iter().product();
    let batch_n: usize = dsh[..b].iter().product();
    let tail: usize = ish[b..].iter().product();
    let post_layout = {
        // The trailing block data[.., v, a+1..] as a layout with the data's own strides.
        let mut l = data.layout;
        l = l.sliced(a, 0, 1);
        let r = l.rank as usize;
        let mut sub = Layout { rank: (r - a - 1) as u8, shape: [1; 4], strides: [0; 4], offset: 0 };
        for (k, d) in (a + 1..r).enumerate() {
            sub.shape[k] = l.shape[d];
            sub.strides[k] = l.strides[d];
        }
        sub
    };
    let post_n = post_layout.numel();
    let post_contiguous = post_layout.is_contiguous();
    let pre_rm = row_major(&dsh[..a]);
    let dst_ = data.layout.strides();
    with_slice!(data.data, dv => {
        let o = out_vec(out);
        o.clear();
        o.reserve(pre * tail * post_layout.numel());
        for p in 0..pre {
            // The data offset of the leading index p (over data.shape[..a]) and its batch part.
            let mut off = data.layout.offset;
            let mut rem = p;
            for d in 0..a {
                let i = rem / pre_rm[d];
                rem %= pre_rm[d];
                off += i * dst_[d];
            }
            let batch_lin = if b == 0 { 0 } else { p / (pre / batch_n) };
            let idx = &iv[batch_lin * tail..(batch_lin + 1) * tail];
            let sa = dst_[a];
            if post_n == 1 {
                // One element per index: a table lookup (`2^s`, an activation table).
                if idx.len() >= *super::PAR_ELEMS {
                    use rayon::prelude::*;
                    let start = o.len();
                    o.resize(start + idx.len(), Default::default());
                    let chunk = super::par_chunk(idx.len());
                    o[start..].par_chunks_mut(chunk).zip(idx.par_chunks(chunk)).for_each(|(o, ix)| {
                        for (d, v) in o.iter_mut().zip(ix) {
                            *d = dv[off + *v as usize * sa];
                        }
                    });
                } else {
                    o.extend(idx.iter().map(|v| dv[off + *v as usize * sa]));
                }
            } else if post_contiguous {
                // One contiguous row per index (embedding rows, expert matrices).
                for v in idx {
                    let src = off + *v as usize * sa;
                    o.extend_from_slice(&dv[src..src + post_n]);
                }
            } else {
                for v in idx {
                    let mut l = post_layout;
                    l.offset = off + *v as usize * sa;
                    l.for_each_run(|s, len, st| {
                        if st == 1 {
                            o.extend_from_slice(&dv[s..s + len]);
                        } else {
                            o.extend((0..len).map(|t| dv[s + t * st]));
                        }
                    });
                }
            }
        }
    });
    Ok(())
}

/// Check one scalar gather index, returning it as an offset along the axis.
pub fn scalar_index(idx: &Opd<'_>, n: usize) -> TirResult<usize> {
    let v = idx.data.get(idx.layout.offset);
    if v < 0 || v >= n as i128 {
        return Err(index_err(v as i64, n));
    }
    Ok(v as usize)
}

/// `TopK { axis, k }`: per row, the `k` largest by (value descending, index ascending), emitted
/// in ascending index order.
pub fn topk(node: &NodePlan, x: &Opd<'_>, out: &mut Buf, _scratch: &mut Scratch) -> TirResult<()> {
    let Prim::TopK { axis, k } = node.prim else { return Err(TirError::new(TirErrorKind::Shape, "not a TopK")) };
    // Compare the native storage directly through its view. No whole-source
    // widening/gather or axis-sized candidate vector is needed for selection.
    with_slice!(x.data, values => topk_t(values, &x.layout, axis as usize, k as usize, out))
}
fn topk_t<T: Elem>(values: &[T], layout: &Layout, axis: usize, k: usize, out: &mut Buf) -> TirResult<()> {
    let sh = layout.shape();
    let (outer, n, inner): (usize, usize, usize) = (sh[..axis].iter().product(), sh[axis], sh[axis + 1..].iter().product());
    let rm = row_major(sh);
    let o = out_vec::<u32>(out);
    o.clear();
    o.resize(outer * k * inner, 0);
    let mut kept: BinaryHeap<(Reverse<T>, usize)> = BinaryHeap::with_capacity(if k <= 64 && k < n { k } else { 0 });
    let mut chosen = Vec::with_capacity(if k == n {
        0
    } else if k <= 64 {
        k
    } else {
        (2 * k).min(n)
    });
    for oo in 0..outer {
        for ii in 0..inner {
            if k == n {
                for slot in 0..k {
                    o[(oo * k + slot) * inner + ii] = slot as u32;
                }
                continue;
            }
            let mut rem = oo * n * inner + ii;
            let mut base = layout.offset;
            for (d, stride) in rm.iter().enumerate().take(sh.len()) {
                base += (rem / stride) * layout.strides[d];
                rem %= stride;
            }
            let at = |t: usize| values[base + t * layout.strides[axis]];
            chosen.clear();
            if k <= 64 {
                kept.clear();
                for idx in 0..n {
                    let candidate = (Reverse(at(idx)), idx);
                    if kept.len() < k {
                        kept.push(candidate);
                    } else if candidate < *kept.peek().expect("validated positive k") {
                        *kept.peek_mut().unwrap() = candidate;
                    }
                }
                chosen.extend(kept.drain().map(|(_, idx)| idx));
            } else {
                // Each linear partition folds k new indices into k retained
                // indices. Scores stay in native strided storage throughout.
                for idx in 0..n {
                    chosen.push(idx);
                    if chosen.len() == 2 * k {
                        chosen.select_nth_unstable_by(k - 1, |p, q| at(*q).cmp(&at(*p)).then(p.cmp(q)));
                        chosen.truncate(k);
                    }
                }
                if chosen.len() > k {
                    chosen.select_nth_unstable_by(k - 1, |p, q| at(*q).cmp(&at(*p)).then(p.cmp(q)));
                    chosen.truncate(k);
                }
            }
            chosen.sort_unstable();
            for (slot, idx) in chosen.iter().enumerate() {
                o[(oo * k + slot) * inner + ii] = *idx as u32;
            }
        }
    }
    Ok(())
}

/// `Concat { axis }`: the inputs end to end along `axis`, each copied through its own layout.
pub fn concat(node: &NodePlan, ins: &[Opd<'_>], out_shape: &[usize], out: &mut Buf) -> TirResult<()> {
    let Prim::Concat { axis } = node.prim else { return Err(TirError::new(TirErrorKind::Shape, "not a Concat")) };
    let a = axis as usize;
    let dt = node.out.dtype;
    crate::with_dtype!(dt, T => concat_t::<T>(ins, a, out_shape, out))
}

fn concat_t<T: Elem>(ins: &[Opd<'_>], a: usize, out_shape: &[usize], out: &mut Buf) -> TirResult<()> {
    let o = out_vec::<T>(out);
    o.clear();
    o.resize(numel(out_shape), T::default());
    let whole = Layout::contiguous(out_shape);
    let mut start = 0usize;
    for op in ins {
        let e = op.shape()[a];
        let region = whole.sliced(a, start, e);
        let converted: Vec<T>;
        let (src, soff) = match T::slice_of(op.data) {
            Some(src) => (src, op.layout.offset),
            None => {
                // An input held narrower than its declared dtype (an i128 node stored in i64).
                converted = crate::with_slice!(op.data, v => {
                    let mut c = Vec::new();
                    crate::layout::gather_strided(v, &op.layout, &mut c, |x| T::from_i128(x.to_i128()));
                    c
                });
                (&converted[..], 0)
            }
        };
        let in_strides: Vec<usize> = if T::slice_of(op.data).is_some() {
            op.layout.strides().to_vec()
        } else {
            crate::layout::row_major(op.shape())[..op.shape().len()].to_vec()
        };
        Joint::<2>::new(op.shape(), [region.strides(), &in_strides]).for_each(|_, [ob, ib], len, [os, is]| {
            let (ob, ib) = (region.offset + ob, soff + ib);
            if os == 1 && is == 1 {
                o[ob..ob + len].copy_from_slice(&src[ib..ib + len]);
            } else {
                for t in 0..len {
                    o[ob + t * os] = src[ib + t * is];
                }
            }
        });
        start += e;
    }
    Ok(())
}

/// `Iota { axis, start, step }` at the running `H`.
pub fn iota(node: &NodePlan, out_shape: &[usize], out: &mut Buf) -> TirResult<()> {
    let Prim::Iota { axis, start, step } = node.prim else { return Err(TirError::new(TirErrorKind::Shape, "not an Iota")) };
    let dt = node.out.dtype;
    let a = axis as usize;
    let rm = row_major(out_shape);
    let (stride, ext) = (rm[a], out_shape[a]);
    let n = numel(out_shape);
    let value = |lin: usize| start as i128 + step as i128 * ((lin / stride) % ext) as i128;
    if node.check_out && (0..n).any(|lin| !dt.contains(value(lin))) {
        return overflow("Iota: a value outside the dtype");
    }
    crate::with_dtype!(dt, T => {
        let o = out_vec::<T>(out);
        o.clear();
        o.extend((0..n).map(|lin| T::from_i128(value(lin))));
    });
    Ok(())
}

/// The dtype of a slice, for error messages.
pub fn dtype_of(s: Slice<'_>) -> DType {
    s.dtype()
}
