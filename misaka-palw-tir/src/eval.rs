//! One primitive on concrete tensors (spec 04b §5). Straightforward loops in canonical (row-major,
//! ascending-index) order, no SIMD, no reordering. Every exact primitive checks its result — and
//! every partial sum of an exact reduction, in every order — against the declared type and returns
//! [`TirErrorKind::Overflow`] instead of wrapping or saturating: that is how a range-analysis
//! defect will surface (PALW-TIR-9).

use crate::arith::{div_round, int_exp, int_ln, int_rsqrt, log2_floor};
use crate::error::{TirErrorKind, TirResult, err};
use crate::prim::Prim;
use crate::program::{StateDecl, StateKind};
use crate::tensor::Tensor;
use crate::types::{DType, element_count, strides};

fn unravel(mut i: usize, st: &[usize]) -> Vec<usize> {
    st.iter()
        .map(|s| {
            let q = i / s;
            i %= s;
            q
        })
        .collect()
}

fn ravel(ix: &[usize], st: &[usize]) -> usize {
    ix.iter().zip(st).map(|(a, b)| a * b).sum()
}

/// For every element of `out_shape` (row-major), the linear index of the element of `in_shape`
/// it reads under numpy broadcasting.
fn broadcast_map(out_shape: &[usize], in_shape: &[usize]) -> Vec<usize> {
    let ost = strides(out_shape);
    let ist = strides(in_shape);
    let off = out_shape.len() - in_shape.len();
    (0..element_count(out_shape))
        .map(|i| {
            let o = unravel(i, &ost);
            (0..in_shape.len()).map(|k| if in_shape[k] == 1 { 0 } else { o[off + k] * ist[k] }).sum()
        })
        .collect()
}

fn fit(v: i128, dtype: DType, what: &str) -> TirResult<i128> {
    if dtype.contains(v) { Ok(v) } else { err(TirErrorKind::Overflow, format!("{what}: {v} does not fit {}", dtype.name())) }
}

fn checked(v: Option<i128>, what: &str) -> TirResult<i128> {
    v.ok_or_else(|| crate::error::TirError::new(TirErrorKind::Overflow, format!("{what}: past i128")))
}

/// Exact sum of `terms` whose every partial sum, in every order, lies in `dtype`: the sum of the
/// positive terms must not exceed `dtype.max` and the sum of the negative terms must not go below
/// `dtype.min` (those two are the extreme partial sums over all orders and all groupings).
fn order_free_sum(terms: impl Iterator<Item = TirResult<i128>>, dtype: DType, what: &str) -> TirResult<i128> {
    let (mut pos, mut neg) = (0i128, 0i128);
    for t in terms {
        let t = t?;
        if t > 0 {
            pos = checked(pos.checked_add(t), what)?;
            if pos > dtype.max() {
                return err(TirErrorKind::Overflow, format!("{what}: a partial sum reaches {pos} > {}", dtype.max()));
            }
        } else {
            neg = checked(neg.checked_add(t), what)?;
            if neg < dtype.min() {
                return err(TirErrorKind::Overflow, format!("{what}: a partial sum reaches {neg} < {}", dtype.min()));
            }
        }
    }
    Ok(pos + neg)
}

fn binary(a: &Tensor, b: &Tensor, out_shape: &[usize], mut f: impl FnMut(i128, i128) -> TirResult<i128>) -> TirResult<Vec<i128>> {
    let ma = broadcast_map(out_shape, &a.shape);
    let mb = broadcast_map(out_shape, &b.shape);
    ma.iter().zip(&mb).map(|(i, j)| f(a.data[*i], b.data[*j])).collect()
}

/// Evaluate one node. `HistAppend` is the evaluator's (it needs the history); everything else is
/// here. `out_shape` is the declared shape resolved at the running `H`.
pub(crate) fn eval_prim(
    prim: &Prim,
    ins: &[&Tensor],
    out_dtype: DType,
    out_shape: &[usize],
    states: &[StateDecl],
) -> TirResult<Tensor> {
    let n_out = element_count(out_shape);
    let name = prim.name();
    let data: Vec<i128> = match prim {
        Prim::Reshape | Prim::Cast => {
            if ins[0].len() != n_out {
                return err(TirErrorKind::Shape, format!("{name}: {} elements into {n_out}", ins[0].len()));
            }
            ins[0].data.iter().map(|v| fit(*v, out_dtype, name)).collect::<TirResult<_>>()?
        }
        Prim::Transpose { perm } => {
            let x = ins[0];
            let ist = strides(&x.shape);
            let ost = strides(out_shape);
            (0..n_out)
                .map(|i| {
                    let o = unravel(i, &ost);
                    let mut j = vec![0usize; o.len()];
                    for (k, p) in perm.iter().enumerate() {
                        j[*p as usize] = o[k];
                    }
                    x.data[ravel(&j, &ist)]
                })
                .collect()
        }
        Prim::Slice { axis, start } => {
            let x = ins[0];
            let ist = strides(&x.shape);
            let ost = strides(out_shape);
            (0..n_out)
                .map(|i| {
                    let mut o = unravel(i, &ost);
                    o[*axis as usize] += *start as usize;
                    x.data[ravel(&o, &ist)]
                })
                .collect()
        }
        Prim::Concat { axis } => {
            let a = *axis as usize;
            let ost = strides(out_shape);
            (0..n_out)
                .map(|i| {
                    let mut o = unravel(i, &ost);
                    for t in ins {
                        if o[a] < t.shape[a] {
                            return Ok(t.data[ravel(&o, &strides(&t.shape))]);
                        }
                        o[a] -= t.shape[a];
                    }
                    err(TirErrorKind::Shape, "Concat: the inputs do not cover the output")
                })
                .collect::<TirResult<_>>()?
        }
        Prim::Broadcast => broadcast_map(out_shape, &ins[0].shape).iter().map(|i| ins[0].data[*i]).collect(),
        Prim::Iota { axis, start, step } => {
            let ost = strides(out_shape);
            (0..n_out)
                .map(|i| {
                    let o = unravel(i, &ost);
                    fit(*start as i128 + (*step as i128) * (o[*axis as usize] as i128), out_dtype, name)
                })
                .collect::<TirResult<_>>()?
        }
        Prim::Gather { axis, batch_dims } => {
            let (d, ix) = (ins[0], ins[1]);
            let (a, b) = (*axis as usize, *batch_dims as usize);
            let dst = strides(&d.shape);
            let xst = strides(&ix.shape);
            let ost = strides(out_shape);
            let m = ix.shape.len() - b;
            (0..n_out)
                .map(|i| {
                    let o = unravel(i, &ost);
                    let mut xi: Vec<usize> = o[..b].to_vec();
                    xi.extend_from_slice(&o[a..a + m]);
                    let v = ix.data[ravel(&xi, &xst)];
                    if v < 0 || v >= d.shape[a] as i128 {
                        return err(TirErrorKind::Index, format!("Gather: index {v} outside [0, {})", d.shape[a]));
                    }
                    let mut di: Vec<usize> = o[..a].to_vec();
                    di.push(v as usize);
                    di.extend_from_slice(&o[a + m..]);
                    Ok(d.data[ravel(&di, &dst)])
                })
                .collect::<TirResult<_>>()?
        }
        Prim::Add => binary(ins[0], ins[1], out_shape, |x, y| fit(checked(x.checked_add(y), name)?, out_dtype, name))?,
        Prim::Sub => binary(ins[0], ins[1], out_shape, |x, y| fit(checked(x.checked_sub(y), name)?, out_dtype, name))?,
        Prim::Mul => binary(ins[0], ins[1], out_shape, |x, y| fit(checked(x.checked_mul(y), name)?, out_dtype, name))?,
        Prim::Div { rule } => binary(ins[0], ins[1], out_shape, |x, d| {
            let q = div_round(x, d, *rule)
                .ok_or_else(|| crate::error::TirError::new(TirErrorKind::Divisor, format!("Div: divisor {d} < 1")))?;
            fit(q, out_dtype, name)
        })?,
        Prim::Compare { cmp } => binary(ins[0], ins[1], out_shape, |x, y| Ok(cmp.holds(x, y) as i128))?,
        Prim::Select => {
            let mc = broadcast_map(out_shape, &ins[0].shape);
            let ma = broadcast_map(out_shape, &ins[1].shape);
            let mb = broadcast_map(out_shape, &ins[2].shape);
            (0..n_out)
                .map(|i| fit(if ins[0].data[mc[i]] != 0 { ins[1].data[ma[i]] } else { ins[2].data[mb[i]] }, out_dtype, name))
                .collect::<TirResult<_>>()?
        }
        Prim::MatMul => {
            let (x, y) = (ins[0], ins[1]);
            let (xr, yr, or) = (x.shape.len(), y.shape.len(), out_shape.len());
            let (m, kk, n) = (x.shape[xr - 2], x.shape[xr - 1], y.shape[yr - 1]);
            let batch = &out_shape[..or - 2];
            let nb = element_count(batch);
            let xb = broadcast_map(batch, &x.shape[..xr - 2]);
            let yb = broadcast_map(batch, &y.shape[..yr - 2]);
            let mut out = Vec::with_capacity(n_out);
            for bi in 0..nb {
                let (xo, yo) = (xb[bi] * m * kk, yb[bi] * kk * n);
                for r in 0..m {
                    for c in 0..n {
                        let terms = (0..kk).map(|t| checked(x.data[xo + r * kk + t].checked_mul(y.data[yo + t * n + c]), name));
                        out.push(order_free_sum(terms, out_dtype, name)?);
                    }
                }
            }
            out
        }
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
            let x = ins[0];
            let a = *axis as usize;
            let ist = strides(&x.shape);
            let ost = strides(out_shape);
            (0..n_out)
                .map(|i| {
                    let o = unravel(i, &ost);
                    let at = |t: usize| {
                        let mut j = o.clone();
                        j[a] = t;
                        x.data[ravel(&j, &ist)]
                    };
                    if matches!(prim, Prim::ReduceSum { .. }) {
                        order_free_sum((0..x.shape[a]).map(|t| Ok(at(t))), out_dtype, name)
                    } else {
                        Ok((0..x.shape[a]).map(at).max().expect("dimensions are at least 1"))
                    }
                })
                .collect::<TirResult<_>>()?
        }
        Prim::Clamp { lo, hi } => ins[0].data.iter().map(|v| (*v).clamp(*lo as i128, *hi as i128)).collect(),
        Prim::Log2Floor => ins[0].data.iter().map(|v| fit(log2_floor(*v), out_dtype, name)).collect::<TirResult<_>>()?,
        Prim::IntExp => ins[0].data.iter().map(|v| fit(int_exp(*v), out_dtype, name)).collect::<TirResult<_>>()?,
        Prim::IntRsqrt => ins[0].data.iter().map(|v| fit(int_rsqrt(*v), out_dtype, name)).collect::<TirResult<_>>()?,
        Prim::IntLn => ins[0].data.iter().map(|v| fit(int_ln(*v), out_dtype, name)).collect::<TirResult<_>>()?,
        Prim::TopK { axis, k } => {
            let x = ins[0];
            let a = *axis as usize;
            let ist = strides(&x.shape);
            let ost = strides(out_shape);
            let mut out = vec![0i128; n_out];
            // One selection per row of the axis: rank by (value descending, index ascending), keep
            // the first k, emit them in ascending index order.
            let mut rows_done = std::collections::BTreeSet::new();
            for i in 0..n_out {
                let mut o = unravel(i, &ost);
                o[a] = 0;
                let row_key = ravel(&o, &ost);
                if !rows_done.insert(row_key) {
                    continue;
                }
                let at = |t: usize| {
                    let mut j = o.clone();
                    j[a] = t;
                    x.data[ravel(&j, &ist)]
                };
                let mut order: Vec<usize> = (0..x.shape[a]).collect();
                order.sort_by(|p, q| at(*q).cmp(&at(*p)).then(p.cmp(q)));
                let mut chosen: Vec<usize> = order[..*k as usize].to_vec();
                chosen.sort_unstable();
                for (slot, idx) in chosen.iter().enumerate() {
                    let mut j = o.clone();
                    j[a] = slot;
                    out[ravel(&j, &ost)] = *idx as i128;
                }
            }
            out
        }
        Prim::StateWrite { state } => {
            let StateKind::Fixed { lo, hi } = states[*state as usize].kind else {
                return err(TirErrorKind::Shape, "StateWrite on a Hist state");
            };
            ins[0].data.iter().map(|v| (*v).clamp(lo as i128, hi as i128)).collect()
        }
        Prim::HistAppend { .. } => return err(TirErrorKind::Shape, "HistAppend is evaluated by the reference evaluator"),
    };
    debug_assert_eq!(data.len(), n_out);
    Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
}

/// **One stateless primitive on concrete operands** — the unit the golden vectors pin
/// (`consensus-vectors/tir-v1/primitives/`). Total: the operands' types are checked by the same
/// rules [`crate::validate::validate`] applies to a node (with every dimension constant), then the
/// primitive runs. `StateWrite` and `HistAppend` need a program and are refused here.
pub fn eval_primitive(prim: &Prim, inputs: &[Tensor], out_dtype: DType, out_shape: &[usize]) -> TirResult<Tensor> {
    use crate::types::{Dim, TensorType};
    if matches!(prim, Prim::StateWrite { .. } | Prim::HistAppend { .. }) {
        return err(TirErrorKind::Shape, "state primitives are evaluated inside a program");
    }
    let (lo, hi) = prim.arity();
    if inputs.len() < lo || inputs.len() > hi {
        return err(TirErrorKind::Shape, format!("{}: {} inputs, want {lo}..={hi}", prim.name(), inputs.len()));
    }
    let fixed = |dtype: DType, shape: &[usize]| -> TirResult<TensorType> {
        if shape.len() > crate::types::MAX_RANK || shape.iter().any(|d| *d == 0 || *d > crate::types::MAX_DIM as usize) {
            return err(TirErrorKind::Shape, format!("shape {shape:?} is not a legal tensor shape"));
        }
        Ok(TensorType::new(dtype, shape.iter().map(|d| Dim::Fixed(*d as u32)).collect()))
    };
    let mut ins = Vec::with_capacity(inputs.len());
    for t in inputs {
        if t.data.len() != element_count(&t.shape) || t.data.iter().any(|v| !t.dtype.contains(*v)) {
            return err(TirErrorKind::Operand, "an operand is not a tensor of its dtype");
        }
        ins.push(fixed(t.dtype, &t.shape)?);
    }
    let out = fixed(out_dtype, out_shape)?;
    if out.elements_at(1) > crate::types::MAX_ELEMENTS {
        return err(TirErrorKind::Shape, "more than 2^28 output elements");
    }
    crate::validate::check_node_type(prim, &ins, &out, &[]).map_err(|m| crate::error::TirError::new(TirErrorKind::Shape, m))?;
    let refs: Vec<&Tensor> = inputs.iter().collect();
    eval_prim(prim, &refs, out_dtype, out_shape, &[])
}
