//! **Row-range evaluation of a [`Src`]** — the streaming loader's expression evaluator.
//!
//! [`eval_src`] evaluates an HL param's source expression to the whole tensor. A conversion that
//! must not hold a vocabulary-sized table (or a stack of experts) in memory needs the same value
//! in blocks: [`eval_src_rows`] gives rows `a..b` of it — *every axis but the last flattened into
//! rows* — reading only the checkpoint rows those need, and [`src_row_space`] says whether an
//! expression can be evaluated that way at all. The block-wise value is, row for row, the whole
//! evaluation's (`tests::row_ranges_equal_the_whole_evaluation` pins it over every fixture binding
//! and every block size), so a lowering that quantises per row (W8 codes, `i16` table codes, row
//! scales) produces the same integers from blocks as from the whole.
//!
//! Streamable: a checkpoint tensor (rank ≥ 2), a row slice of one (`Pick::Range`, `Strided`,
//! `PerLayer`), a column slice (rows pass through), a stack of streamable values (experts), an
//! element-wise map, and a reshape that keeps the last axis. Not streamable — a transpose (the
//! output's rows are the input's columns), a quantised weight (its integers are unpacked whole,
//! `crate::prequant`), a reshape that changes the columns, a slice of a middle axis — and then the
//! caller loads the param whole, as it always did.

use super::{MapFn, Resolver, Src, Tensor, expand, src_shape};
use crate::error::{LowerError, Result};
use std::collections::BTreeMap;
use std::ops::Range;

/// `(rows, cols)` of `src`'s value, every axis but the last flattened into rows — or `None` when
/// the value cannot be produced by row ranges (see the module docs).
pub fn src_row_space(src: &Src, r: &Resolver, layer: Option<usize>, vars: &BTreeMap<char, usize>) -> Result<Option<(usize, usize)>> {
    let shape = src_shape(src, r, layer, vars)?;
    if shape.len() < 2 {
        return Ok(None);
    }
    if !streamable(src, &shape, r, layer, vars)? {
        return Ok(None);
    }
    let (cols, lead) = shape.split_last().expect("rank ≥ 2");
    Ok(Some((lead.iter().product(), *cols)))
}

/// Whether the expression's structure can be evaluated by row ranges (its value has rank ≥ 2 and
/// `shape`).
fn streamable(src: &Src, shape: &[usize], r: &Resolver, layer: Option<usize>, vars: &BTreeMap<char, usize>) -> Result<bool> {
    Ok(match src {
        Src::Tensor(_) => true,
        Src::Take { src: inner, axis, .. } => {
            let ishape = src_shape(inner, r, layer, vars)?;
            ishape.len() >= 2 && (*axis == 0 || *axis == ishape.len() - 1) && streamable(inner, &ishape, r, layer, vars)?
        }
        Src::Transpose(_) | Src::Quant { .. } => false,
        Src::Stack { src: inner, var, .. } => {
            let mut v = vars.clone();
            v.insert(*var, 0);
            let ishape = src_shape(inner, r, layer, &v)?;
            ishape.len() >= 2 && streamable(inner, &ishape, r, layer, &v)?
        }
        Src::Map { src: inner, .. } => streamable(inner, shape, r, layer, vars)?,
        Src::Reshape { src: inner, shape: out } => {
            let ishape = src_shape(inner, r, layer, vars)?;
            ishape.len() >= 2 && ishape.last() == out.last() && streamable(inner, &ishape, r, layer, vars)?
        }
    })
}

/// Rows `rows` of `src`'s value (see [`src_row_space`]): a `[rows.len(), cols]` tensor equal to
/// [`slice_rows`] of the whole evaluation, reading only the checkpoint rows it needs. Errors if the
/// expression is not streamable.
pub fn eval_src_rows(
    src: &Src,
    r: &Resolver,
    layer: Option<usize>,
    vars: &BTreeMap<char, usize>,
    rows: Range<usize>,
) -> Result<Tensor> {
    let shape = src_shape(src, r, layer, vars)?;
    if !streamable(src, &shape, r, layer, vars)? || shape.len() < 2 {
        return Err(LowerError::eval(format!("{src:?} cannot be evaluated by row ranges")));
    }
    let cols = *shape.last().expect("rank ≥ 2");
    let total: usize = shape[..shape.len() - 1].iter().product();
    if rows.start > rows.end || rows.end > total {
        return Err(LowerError::weights(format!("rows {rows:?} of a value of {total} rows (shape {shape:?})")));
    }
    rows_of(src, r, layer, vars, rows, cols)
}

fn rows_of(src: &Src, r: &Resolver, layer: Option<usize>, vars: &BTreeMap<char, usize>, rows: Range<usize>, cols: usize) -> Result<Tensor> {
    if rows.is_empty() {
        return Ok(Tensor::new(vec![0, cols], Vec::new()));
    }
    match src {
        Src::Tensor(t) => {
            let n = expand(t, layer, vars)?;
            let rn = r.resolve(&n).ok_or_else(|| LowerError::weights(format!("missing tensor `{n}`")))?;
            r.touched.borrow_mut().insert(rn.clone());
            r.src.load_rows(&rn, rows)
        }
        Src::Take { src: inner, axis, pick } => {
            let ishape = src_shape(inner, r, layer, vars)?;
            let idx = pick.indices_at(layer)?;
            if *axis == ishape.len() - 1 {
                // Columns: the rows pass through, the picked columns of each are kept.
                let t = rows_of(inner, r, layer, vars, rows.clone(), *ishape.last().expect("rank ≥ 2"))?;
                let ic = *ishape.last().expect("rank ≥ 2");
                if let Some(bad) = idx.iter().find(|i| **i >= ic) {
                    return Err(LowerError::weights(format!("take index {bad} ≥ axis length {ic} (shape {ishape:?})")));
                }
                let mut data = Vec::with_capacity(rows.len() * idx.len());
                for row in t.data.chunks_exact(ic) {
                    data.extend(idx.iter().map(|j| row[*j]));
                }
                return Ok(Tensor::new(vec![rows.len(), idx.len()], data));
            }
            // Axis 0: output row `k` is inner row `idx[k / per] · per + k % per`.
            let per: usize = ishape[1..ishape.len() - 1].iter().product();
            let n0 = ishape[0];
            if let Some(bad) = idx.iter().find(|i| **i >= n0) {
                return Err(LowerError::weights(format!("take index {bad} ≥ axis length {n0} (shape {ishape:?})")));
            }
            let mut data = Vec::with_capacity(rows.len() * cols);
            let mut k = rows.start;
            while k < rows.end {
                // The longest run of consecutive inner rows starting at output row `k`.
                let start = idx[k / per] * per + k % per;
                let mut len = 1;
                while k + len < rows.end {
                    let nk = k + len;
                    if idx[nk / per] * per + nk % per == start + len {
                        len += 1;
                    } else {
                        break;
                    }
                }
                data.extend(rows_of(inner, r, layer, vars, start..start + len, cols)?.data);
                k += len;
            }
            Ok(Tensor::new(vec![rows.len(), cols], data))
        }
        Src::Stack { src: inner, var, .. } => {
            let mut v0 = vars.clone();
            v0.insert(*var, 0);
            let ishape = src_shape(inner, r, layer, &v0)?;
            let per: usize = ishape[..ishape.len() - 1].iter().product();
            let mut data = Vec::with_capacity(rows.len() * cols);
            let mut e = rows.start / per;
            while e * per < rows.end {
                let (lo, hi) = (rows.start.max(e * per) - e * per, rows.end.min((e + 1) * per) - e * per);
                let mut v = vars.clone();
                v.insert(*var, e);
                data.extend(rows_of(inner, r, layer, &v, lo..hi, cols)?.data);
                e += 1;
            }
            Ok(Tensor::new(vec![rows.len(), cols], data))
        }
        Src::Map { src: inner, f } => {
            let mut t = rows_of(inner, r, layer, vars, rows, cols)?;
            apply_map(&mut t, f, layer)?;
            Ok(t)
        }
        Src::Reshape { src: inner, .. } => rows_of(inner, r, layer, vars, rows, cols),
        Src::Transpose(_) | Src::Quant { .. } => Err(LowerError::eval("a transpose or a quantised weight is not evaluated by row ranges")),
    }
}

/// Apply an element-wise map in place (the one definition: [`super::eval_src`] and the row-range
/// evaluation both call it).
pub(super) fn apply_map(t: &mut Tensor, f: &MapFn, layer: Option<usize>) -> Result<()> {
    match f {
        MapFn::NegExp => t.data.iter_mut().for_each(|x| *x = -((*x as f64).exp() as f32)),
        MapFn::Scale(c) => t.data.iter_mut().for_each(|x| *x = (*x as f64 * c) as f32),
        MapFn::RescaleByLayer { every } => {
            let l = layer.ok_or_else(|| LowerError::eval("rescale needs a layer"))?;
            let div = 2f64.powi((l / every.max(&1)) as i32);
            t.data.iter_mut().for_each(|x| *x = (*x as f64 / div) as f32);
        }
    }
    Ok(())
}

/// Blocks of at most `block_rows` rows covering `0..rows`.
pub fn row_blocks(rows: usize, block_rows: usize) -> impl Iterator<Item = Range<usize>> {
    let step = block_rows.max(1);
    (0..rows.div_ceil(step)).map(move |i| i * step..((i + 1) * step).min(rows))
}
