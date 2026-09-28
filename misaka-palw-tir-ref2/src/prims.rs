//! The values of the primitives (04b §6) on concrete tensors.
//!
//! Every exact result is computed as a mathematical integer ([`Wide`]) and then tested against the
//! output dtype (§6.1, the exact-result rule). Index maps follow §0 literally: an output linear
//! position is unravelled into its multi-index, the operand's multi-index is derived from it by the
//! primitive's definition, and that is ravelled into the operand's linear position.

use std::cmp::Ordering;

use crate::error::{Class, Res, err};
use crate::program::{Cmp, Prim, Rounding, StateDecl, StateKind};
use crate::tensor::{Tensor, count, ravel, unravel};
use crate::transcendental::{floor_divmod, int_exp, int_ln, int_rsqrt, log2_floor};
use crate::types::DType;
use crate::wide::Wide;

fn shape_err<T>(prim: &Prim, why: &str) -> Res<T> {
    err(Class::Shape, format!("{} at evaluation: {why}", prim.name()))
}

/// The exact-result rule: `v` must be a value of `dt`.
fn fit(v: Wide, dt: DType) -> Res<i128> {
    if v.within(dt.min(), dt.max()) {
        Ok(v.to_i128().unwrap_or(0))
    } else {
        err(Class::Overflow, format!("result outside {}", dt.name()))
    }
}

fn fit_i(v: i128, dt: DType) -> Res<i128> {
    fit(Wide::from_i128(v), dt)
}

/// The operand multi-index read for output multi-index `o` under §2.3 broadcasting: trailing
/// alignment, and an operand extent of 1 reads index 0.
fn broadcast_index(o: &[u64], in_shape: &[u64], idx: &mut Vec<u64>) {
    idx.clear();
    let off = o.len() - in_shape.len();
    for (k, &d) in in_shape.iter().enumerate() {
        idx.push(if d == 1 { 0 } else { o[k + off] });
    }
}

/// Checks that `in_shape` broadcasts to `out` (concretely).
fn broadcasts_to(in_shape: &[u64], out: &[u64]) -> bool {
    if in_shape.len() > out.len() {
        return false;
    }
    let off = out.len() - in_shape.len();
    in_shape.iter().enumerate().all(|(k, &d)| d == 1 || d == out[k + off])
}

/// Order-free sum (§6.3, PALW-TIR-24): `P` = sum of the positive terms, `N` = sum of the negative
/// terms, both exact; fail unless `N ≥ min` and `P ≤ max`; the value is `P + N`.
struct OrderFree {
    p: Wide,
    n: Wide,
    overflow: bool,
}

impl OrderFree {
    fn new() -> Self {
        OrderFree { p: Wide::ZERO, n: Wide::ZERO, overflow: false }
    }
    fn push(&mut self, t: Wide) {
        let slot = if t.is_negative() { &mut self.n } else { &mut self.p };
        match slot.checked_add(t) {
            Some(s) => *slot = s,
            None => self.overflow = true,
        }
    }
    fn finish(self, dt: DType) -> Res<i128> {
        if self.overflow {
            return err(Class::Overflow, "order-free sum beyond 256 bits");
        }
        let lo = Wide::from_i128(dt.min());
        let hi = Wide::from_i128(dt.max());
        if self.n.cmp_wide(&lo) == Ordering::Less {
            return err(Class::Overflow, format!("negative terms sum below min({})", dt.name()));
        }
        if self.p.cmp_wide(&hi) == Ordering::Greater {
            return err(Class::Overflow, format!("positive terms sum above max({})", dt.name()));
        }
        fit(self.p.checked_add(self.n).unwrap_or(Wide::ZERO), dt)
    }
}

/// §6.4 `Div` of one element, `d ≥ 1`, exactly.
pub fn divide(x: i128, d: i128, rule: Rounding) -> Wide {
    match rule {
        Rounding::Floor => Wide::from_i128(floor_divmod(x, d).0),
        Rounding::HalfUp => {
            let (q, r) = floor_divmod(x, d);
            let bump = 2 * (r as u128) >= d as u128;
            let q = Wide::from_i128(q);
            if bump { q.checked_add(Wide::from_i128(1)).unwrap_or(q) } else { q }
        }
        Rounding::HalfAwayFromZero => {
            let m = x.unsigned_abs();
            let du = d as u128;
            let (q, r) = (m / du, m % du);
            let q = Wide::from_u128(q).checked_add(Wide::from_i128(if 2 * r >= du { 1 } else { 0 })).unwrap_or(Wide::ZERO);
            if x < 0 { q.negate() } else { q }
        }
    }
}

fn cmp_holds(c: Cmp, a: i128, b: i128) -> bool {
    match c {
        Cmp::Eq => a == b,
        Cmp::Ne => a != b,
        Cmp::Lt => a < b,
        Cmp::Le => a <= b,
        Cmp::Gt => a > b,
        Cmp::Ge => a >= b,
    }
}

/// An elementwise primitive over operands broadcast to `out_shape`.
fn elementwise(prim: &Prim, ins: &[&Tensor], out_dtype: DType, out_shape: &[u64], f: impl Fn(&[i128]) -> Res<i128>) -> Res<Tensor> {
    for t in ins {
        if !broadcasts_to(&t.shape, out_shape) {
            return shape_err(prim, "operand does not broadcast to out");
        }
    }
    let n = count(out_shape) as usize;
    let mut data = Vec::with_capacity(n);
    let mut o = vec![0u64; out_shape.len()];
    let mut idx = Vec::with_capacity(out_shape.len());
    let mut vals = vec![0i128; ins.len()];
    for lin in 0..n as u64 {
        unravel(lin, out_shape, &mut o);
        for (k, t) in ins.iter().enumerate() {
            broadcast_index(&o, &t.shape, &mut idx);
            vals[k] = t.data[ravel(&idx, &t.shape) as usize];
        }
        data.push(f(&vals)?);
    }
    Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
}

/// Evaluates one primitive. `prior` is the history for a `HistAppend` (the rows before this
/// position's, oldest first); `states` resolves the `StateWrite` range.
pub fn eval_prim(
    prim: &Prim,
    ins: &[&Tensor],
    out_dtype: DType,
    out_shape: &[u64],
    states: &[StateDecl],
    prior: Option<&[Tensor]>,
) -> Res<Tensor> {
    let (amin, amax) = prim.arity();
    if ins.len() < amin || ins.len() > amax {
        return shape_err(prim, "arity");
    }
    let n_out = count(out_shape);
    if n_out > (1 << 28) {
        return shape_err(prim, "output above 2^28 elements");
    }
    let n_out = n_out as usize;
    let out_rank = out_shape.len();
    let mut o = vec![0u64; out_rank];
    match prim {
        Prim::Reshape => {
            let x = ins[0];
            if count(&x.shape) != n_out as u64 {
                return shape_err(prim, "element counts differ");
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data: x.data.clone() })
        }
        Prim::Transpose { perm } => {
            let x = ins[0];
            if perm.len() != x.shape.len() || out_rank != x.shape.len() {
                return shape_err(prim, "rank");
            }
            for (i, &p) in perm.iter().enumerate() {
                if x.shape.get(p as usize) != Some(&out_shape[i]) {
                    return shape_err(prim, "extent");
                }
            }
            let mut j = vec![0u64; out_rank];
            let mut data = Vec::with_capacity(n_out);
            for lin in 0..n_out as u64 {
                unravel(lin, out_shape, &mut o);
                // j[perm[k]] = i_k for every k.
                for (k, &p) in perm.iter().enumerate() {
                    j[p as usize] = o[k];
                }
                data.push(x.data[ravel(&j, &x.shape) as usize]);
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::Slice { axis, start } => {
            let x = ins[0];
            let a = *axis as usize;
            if a >= out_rank || x.shape.len() != out_rank || *start as u64 + out_shape[a] > x.shape[a] {
                return shape_err(prim, "bounds");
            }
            let mut j = vec![0u64; out_rank];
            let mut data = Vec::with_capacity(n_out);
            for lin in 0..n_out as u64 {
                unravel(lin, out_shape, &mut o);
                j.copy_from_slice(&o);
                j[a] += *start as u64;
                data.push(x.data[ravel(&j, &x.shape) as usize]);
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::Concat { axis } => {
            let a = *axis as usize;
            if a >= out_rank {
                return shape_err(prim, "axis");
            }
            let mut total = 0u64;
            for t in ins {
                if t.shape.len() != out_rank || (0..out_rank).any(|d| d != a && t.shape[d] != out_shape[d]) {
                    return shape_err(prim, "extent");
                }
                total += t.shape[a];
            }
            if total != out_shape[a] {
                return shape_err(prim, "extents do not sum");
            }
            let mut j = vec![0u64; out_rank];
            let mut data = Vec::with_capacity(n_out);
            for lin in 0..n_out as u64 {
                unravel(lin, out_shape, &mut o);
                let mut at = o[a];
                for t in ins {
                    if at < t.shape[a] {
                        j.copy_from_slice(&o);
                        j[a] = at;
                        data.push(t.data[ravel(&j, &t.shape) as usize]);
                        break;
                    }
                    at -= t.shape[a];
                }
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::Broadcast => elementwise(prim, ins, out_dtype, out_shape, |v| Ok(v[0])),
        Prim::Iota { axis, start, step } => {
            let a = *axis as usize;
            if a >= out_rank {
                return shape_err(prim, "axis");
            }
            let mut data = Vec::with_capacity(n_out);
            for lin in 0..n_out as u64 {
                unravel(lin, out_shape, &mut o);
                let v = Wide::from_i128(*start as i128).checked_add(Wide::mul_i128(*step as i128, o[a] as i128)).unwrap_or(Wide::ZERO);
                data.push(fit(v, out_dtype)?);
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::Gather { axis, batch_dims } => {
            let (data_t, idx_t) = (ins[0], ins[1]);
            let (a, b) = (*axis as usize, *batch_dims as usize);
            let rd = data_t.shape.len();
            let ri = idx_t.shape.len();
            if a >= rd || b > a || b > ri {
                return shape_err(prim, "axes");
            }
            let m = ri - b;
            let mut want = data_t.shape[..a].to_vec();
            want.extend_from_slice(&idx_t.shape[b..]);
            want.extend_from_slice(&data_t.shape[a + 1..]);
            if want != out_shape || data_t.shape[..b] != idx_t.shape[..b] {
                return shape_err(prim, "shape");
            }
            let extent = data_t.shape[a] as i128;
            let mut ii = Vec::with_capacity(ri);
            let mut dj = Vec::with_capacity(rd);
            let mut out = Vec::with_capacity(n_out);
            for lin in 0..n_out as u64 {
                unravel(lin, out_shape, &mut o);
                // ii = o[0..B] ++ o[A .. A+m]
                ii.clear();
                ii.extend_from_slice(&o[..b]);
                ii.extend_from_slice(&o[a..a + m]);
                let v = idx_t.data[ravel(&ii, &idx_t.shape) as usize];
                if !(0 <= v && v < extent) {
                    return err(Class::Index, format!("Gather index {v} outside [0, {extent})"));
                }
                // data[ o[0..A] ++ [v] ++ o[A+m ..] ]
                dj.clear();
                dj.extend_from_slice(&o[..a]);
                dj.push(v as u64);
                dj.extend_from_slice(&o[a + m..]);
                out.push(data_t.data[ravel(&dj, &data_t.shape) as usize]);
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data: out })
        }
        Prim::Cast => {
            if ins[0].shape != out_shape {
                return shape_err(prim, "shape");
            }
            let data = ins[0].data.iter().map(|&v| fit_i(v, out_dtype)).collect::<Res<Vec<_>>>()?;
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::Add => elementwise(prim, ins, out_dtype, out_shape, |v| {
            fit(Wide::from_i128(v[0]).checked_add(Wide::from_i128(v[1])).unwrap_or(Wide::ZERO), out_dtype)
        }),
        Prim::Sub => elementwise(prim, ins, out_dtype, out_shape, |v| {
            fit(Wide::from_i128(v[0]).checked_sub(Wide::from_i128(v[1])).unwrap_or(Wide::ZERO), out_dtype)
        }),
        Prim::Mul => elementwise(prim, ins, out_dtype, out_shape, |v| fit(Wide::mul_i128(v[0], v[1]), out_dtype)),
        Prim::Div { rule } => elementwise(prim, ins, out_dtype, out_shape, |v| {
            if v[1] < 1 {
                return err(Class::Divisor, format!("divisor {} below 1", v[1]));
            }
            fit(divide(v[0], v[1], *rule), out_dtype)
        }),
        Prim::Clamp { lo, hi } => {
            if ins[0].shape != out_shape {
                return shape_err(prim, "shape");
            }
            let data = ins[0]
                .data
                .iter()
                .map(|&v| fit_i(v.clamp(*lo as i128, (*hi as i128).max(*lo as i128)), out_dtype))
                .collect::<Res<Vec<_>>>()?;
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::Log2Floor => unary(prim, ins[0], out_dtype, out_shape, log2_floor),
        Prim::IntExp => unary(prim, ins[0], out_dtype, out_shape, int_exp),
        Prim::IntRsqrt => unary(prim, ins[0], out_dtype, out_shape, int_rsqrt),
        Prim::IntLn => unary(prim, ins[0], out_dtype, out_shape, int_ln),
        Prim::Compare { cmp } => {
            elementwise(prim, ins, out_dtype, out_shape, |v| fit_i(cmp_holds(*cmp, v[0], v[1]) as i128, out_dtype))
        }
        Prim::Select => elementwise(prim, ins, out_dtype, out_shape, |v| fit_i(if v[0] != 0 { v[1] } else { v[2] }, out_dtype)),
        Prim::MatMul => {
            let (at, bt) = (ins[0], ins[1]);
            let (ra, rb) = (at.shape.len(), bt.shape.len());
            if ra < 2 || rb < 2 || out_rank < 2 || at.shape[ra - 1] != bt.shape[rb - 2] {
                return shape_err(prim, "contraction");
            }
            let (m, kk, nn) = (at.shape[ra - 2], at.shape[ra - 1], bt.shape[rb - 1]);
            if out_shape[out_rank - 2] != m || out_shape[out_rank - 1] != nn {
                return shape_err(prim, "M, N");
            }
            let batch = &out_shape[..out_rank - 2];
            if !broadcasts_to(&at.shape[..ra - 2], batch) || !broadcasts_to(&bt.shape[..rb - 2], batch) {
                return shape_err(prim, "batch");
            }
            let mut ba = Vec::new();
            let mut bb = Vec::new();
            let mut ia = vec![0u64; ra];
            let mut ib = vec![0u64; rb];
            let mut data = Vec::with_capacity(n_out);
            for lin in 0..n_out as u64 {
                unravel(lin, out_shape, &mut o);
                let (beta, r, c) = (&o[..out_rank - 2], o[out_rank - 2], o[out_rank - 1]);
                broadcast_index(beta, &at.shape[..ra - 2], &mut ba);
                broadcast_index(beta, &bt.shape[..rb - 2], &mut bb);
                ia[..ra - 2].copy_from_slice(&ba);
                ib[..rb - 2].copy_from_slice(&bb);
                ia[ra - 2] = r;
                ib[rb - 1] = c;
                let mut acc = OrderFree::new();
                for t in 0..kk {
                    ia[ra - 1] = t;
                    ib[rb - 2] = t;
                    let x = at.data[ravel(&ia, &at.shape) as usize];
                    let y = bt.data[ravel(&ib, &bt.shape) as usize];
                    acc.push(Wide::mul_i128(x, y));
                }
                data.push(acc.finish(out_dtype)?);
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
            let x = ins[0];
            let a = *axis as usize;
            if a >= x.shape.len()
                || out_rank != x.shape.len()
                || out_shape[a] != 1
                || (0..out_rank).any(|d| d != a && out_shape[d] != x.shape[d])
            {
                return shape_err(prim, "shape");
            }
            let mut j = vec![0u64; out_rank];
            let mut data = Vec::with_capacity(n_out);
            for lin in 0..n_out as u64 {
                unravel(lin, out_shape, &mut o);
                j.copy_from_slice(&o);
                if matches!(prim, Prim::ReduceSum { .. }) {
                    let mut acc = OrderFree::new();
                    for t in 0..x.shape[a] {
                        j[a] = t;
                        acc.push(Wide::from_i128(x.data[ravel(&j, &x.shape) as usize]));
                    }
                    data.push(acc.finish(out_dtype)?);
                } else {
                    let mut best: Option<i128> = None;
                    for t in 0..x.shape[a] {
                        j[a] = t;
                        let v = x.data[ravel(&j, &x.shape) as usize];
                        best = Some(match best {
                            Some(b) if b >= v => b,
                            _ => v,
                        });
                    }
                    data.push(fit_i(best.unwrap_or(0), out_dtype)?);
                }
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::TopK { axis, k } => {
            let x = ins[0];
            let a = *axis as usize;
            let kk = *k as u64;
            if a >= x.shape.len()
                || out_rank != x.shape.len()
                || out_shape[a] != kk
                || kk < 1
                || kk > x.shape[a]
                || (0..out_rank).any(|d| d != a && out_shape[d] != x.shape[d])
            {
                return shape_err(prim, "shape");
            }
            let n = x.shape[a];
            let mut data = vec![0i128; n_out];
            // One row per output index with o[a] = 0.
            let mut j = vec![0u64; out_rank];
            for lin in 0..n_out as u64 {
                unravel(lin, out_shape, &mut o);
                if o[a] != 0 {
                    continue;
                }
                j.copy_from_slice(&o);
                let mut row: Vec<(i128, u64)> = (0..n)
                    .map(|t| {
                        j[a] = t;
                        (x.data[ravel(&j, &x.shape) as usize], t)
                    })
                    .collect();
                // (value descending, index ascending), keep k, then index order.
                row.sort_by(|p, q| q.0.cmp(&p.0).then(p.1.cmp(&q.1)));
                let mut kept: Vec<u64> = row[..kk as usize].iter().map(|e| e.1).collect();
                kept.sort_unstable();
                for (slot, &idx) in kept.iter().enumerate() {
                    j.copy_from_slice(&o);
                    j[a] = slot as u64;
                    data[ravel(&j, out_shape) as usize] = fit_i(idx as i128, out_dtype)?;
                }
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::StateWrite { state } => {
            let Some(StateDecl { kind: StateKind::Fixed { lo, hi }, .. }) = states.get(*state as usize) else {
                return shape_err(prim, "target is not a Fixed state");
            };
            if ins[0].shape != out_shape {
                return shape_err(prim, "shape");
            }
            let data = ins[0]
                .data
                .iter()
                .map(|&v| fit_i(v.clamp(*lo as i128, (*hi as i128).max(*lo as i128)), out_dtype))
                .collect::<Res<Vec<_>>>()?;
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
        Prim::HistAppend { .. } => {
            let row = ins[0];
            let prior = prior.unwrap_or(&[]);
            if out_rank == 0 || out_shape[0] != prior.len() as u64 + 1 || out_shape[1..] != row.shape[..] {
                return shape_err(prim, "window");
            }
            let mut data = Vec::with_capacity(n_out);
            for r in prior {
                if r.shape != row.shape || r.dtype != out_dtype {
                    return err(Class::Operand, "a history row has the wrong dtype or shape");
                }
                data.extend_from_slice(&r.data);
            }
            for &v in &row.data {
                data.push(fit_i(v, out_dtype)?);
            }
            Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
        }
    }
}

fn unary(prim: &Prim, x: &Tensor, out_dtype: DType, out_shape: &[u64], f: fn(i128) -> i128) -> Res<Tensor> {
    if x.shape != out_shape {
        return shape_err(prim, "shape");
    }
    // §6.5: the transcendentals read their input as a 64-bit quantity; i128 is refused by the type
    // rule, and this guard keeps the evaluator total even when called without it.
    if !matches!(prim, Prim::Log2Floor) && x.dtype == DType::I128 {
        return shape_err(prim, "i128 input");
    }
    let data = x.data.iter().map(|&v| fit_i(f(v), out_dtype)).collect::<Res<Vec<_>>>()?;
    Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
}
