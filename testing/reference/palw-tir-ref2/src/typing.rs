//! The type rules of 04b §6, symbolic in `H`.
//!
//! `check_type` answers NF-16's second half: does the declared `out` equal the type the primitive's
//! rule gives for its operand types? For the primitives whose target is only stated by `out`
//! (`Reshape`, `Broadcast`, `Iota`, `Cast`, `Slice`'s length — §4.3), the rule is a set of
//! constraints between the operands and `out`. Every violation is class `Shape`.

use crate::error::{Class, Res, err};
use crate::program::{Prim, StateDecl, StateKind};
use crate::types::{DType, Dim, TensorType, broadcast_shapes};

fn bad<T>(prim: &Prim, why: impl Into<String>) -> Res<T> {
    err(Class::Shape, format!("{}: {}", prim.name(), why.into()))
}

fn want(prim: &Prim, ok: bool, why: &str) -> Res<()> {
    if ok { Ok(()) } else { bad(prim, why) }
}

/// Product of the Fixed extents of a shape without `H` (as u128; ≤ 2^96).
fn product(dims: &[Dim]) -> u128 {
    dims.iter()
        .map(|d| match d {
            Dim::Fixed(n) => *n as u128,
            Dim::H => 1,
        })
        .product()
}

fn state_decl<'a>(prim: &Prim, states: &'a [StateDecl], j: u16) -> Res<&'a StateDecl> {
    match states.get(j as usize) {
        Some(s) => Ok(s),
        None => bad(prim, format!("state {j} does not exist")),
    }
}

/// Checks the declared `out` of a node against §6 for operand types `ins`.
pub fn check_type(prim: &Prim, ins: &[TensorType], out: &TensorType, states: &[StateDecl]) -> Res<()> {
    let (lo, hi) = prim.arity();
    if ins.len() < lo || ins.len() > hi {
        return bad(prim, format!("{} inputs, arity {lo}..={hi}", ins.len()));
    }
    let x = ins.first();
    match prim {
        Prim::Reshape => {
            let x = x.unwrap();
            want(prim, out.dtype == x.dtype, "dtype changes")?;
            match (x.h_count(), out.h_count()) {
                (0, 0) => want(prim, product(&x.shape) == product(&out.shape), "element counts differ"),
                (1, 1) => {
                    let hx = x.shape.iter().position(|d| *d == Dim::H).unwrap();
                    let ho = out.shape.iter().position(|d| *d == Dim::H).unwrap();
                    want(prim, product(&x.shape[..hx]) == product(&out.shape[..ho]), "products before H differ")?;
                    want(prim, product(&x.shape[hx + 1..]) == product(&out.shape[ho + 1..]), "products after H differ")
                }
                _ => bad(prim, "H on one side only"),
            }
        }
        Prim::Transpose { perm } => {
            let x = x.unwrap();
            let r = x.shape.len();
            want(prim, perm.len() == r, "perm length is not the rank")?;
            let mut seen = vec![false; r];
            for &p in perm {
                let p = p as usize;
                if p >= r || seen[p] {
                    return bad(prim, "perm is not a permutation");
                }
                seen[p] = true;
            }
            want(prim, out.dtype == x.dtype, "dtype changes")?;
            want(prim, out.shape.len() == r, "rank changes")?;
            for (i, &p) in perm.iter().enumerate() {
                want(prim, out.shape[i] == x.shape[p as usize], "out.shape[i] != x.shape[perm[i]]")?;
            }
            Ok(())
        }
        Prim::Slice { axis, start } => {
            let x = x.unwrap();
            let a = *axis as usize;
            want(prim, a < x.shape.len(), "axis out of range")?;
            want(prim, out.shape.len() == x.shape.len(), "rank changes")?;
            want(prim, out.dtype == x.dtype, "dtype changes")?;
            let (Dim::Fixed(n), Dim::Fixed(len)) = (x.shape[a], out.shape[a]) else {
                return bad(prim, "slice axis is H");
            };
            want(prim, *start as u64 + len as u64 <= n as u64, "start + len past the end")?;
            for d in 0..x.shape.len() {
                if d != a {
                    want(prim, x.shape[d] == out.shape[d], "another dimension differs")?;
                }
            }
            Ok(())
        }
        Prim::Concat { axis } => {
            let a = *axis as usize;
            let r = out.shape.len();
            want(prim, a < r, "axis out of range")?;
            let mut total: u64 = 0;
            for t in ins {
                want(prim, t.dtype == out.dtype, "dtype differs")?;
                want(prim, t.shape.len() == r, "rank differs")?;
                for d in 0..r {
                    if d == a {
                        match t.shape[d] {
                            Dim::Fixed(n) => total += n as u64,
                            Dim::H => return bad(prim, "concat along H"),
                        }
                    } else {
                        want(prim, t.shape[d] == out.shape[d], "another dimension differs")?;
                    }
                }
            }
            want(prim, out.shape[a] != Dim::H && Some(total) == fixed_extent(out.shape[a]), "extents do not sum to out")
        }
        Prim::Broadcast => {
            let x = x.unwrap();
            want(prim, out.dtype == x.dtype, "dtype changes")?;
            let (rx, ro) = (x.shape.len(), out.shape.len());
            want(prim, rx <= ro, "input rank above output rank")?;
            for i in 0..rx {
                let dx = x.shape[i];
                let dout = out.shape[ro - rx + i];
                want(prim, dx == dout || dx == Dim::Fixed(1), "input dimension neither equal nor 1")?;
            }
            Ok(())
        }
        Prim::Iota { axis, .. } => want(prim, (*axis as usize) < out.shape.len(), "axis out of range"),
        Prim::Gather { axis, batch_dims } => {
            let (data, idx) = (&ins[0], &ins[1]);
            let (a, b) = (*axis as usize, *batch_dims as usize);
            want(prim, a < data.shape.len(), "axis out of range")?;
            want(prim, b <= a, "batch_dims above axis")?;
            want(prim, b <= idx.shape.len(), "batch_dims above the indices' rank")?;
            want(prim, data.shape[..b] == idx.shape[..b], "batch dimensions differ")?;
            want(prim, matches!(data.shape[a], Dim::Fixed(_)), "gather along H")?;
            want(prim, idx.dtype != DType::I128, "i128 indices")?;
            want(prim, out.dtype == data.dtype, "dtype changes")?;
            let mut shape = data.shape[..a].to_vec();
            shape.extend_from_slice(&idx.shape[b..]);
            shape.extend_from_slice(&data.shape[a + 1..]);
            want(prim, out.shape == shape, "out.shape is not data[..A] ++ indices[B..] ++ data[A+1..]")
        }
        Prim::Cast | Prim::Log2Floor => want(prim, out.shape == x.unwrap().shape, "shape changes"),
        Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } => {
            let s = broadcast_shapes(&ins[0].shape, &ins[1].shape)?;
            want(prim, out.shape == s, "out.shape is not the broadcast")
        }
        Prim::Compare { .. } => {
            let s = broadcast_shapes(&ins[0].shape, &ins[1].shape)?;
            want(prim, out.shape == s, "out.shape is not the broadcast")?;
            want(prim, out.dtype == DType::I8, "out.dtype is not i8")
        }
        Prim::Select => {
            let s = broadcast_shapes(&ins[0].shape, &ins[1].shape)?;
            let s = broadcast_shapes(&s, &ins[2].shape)?;
            want(prim, out.shape == s, "out.shape is not the broadcast")
        }
        Prim::MatMul => {
            let (a, b) = (&ins[0], &ins[1]);
            let (ra, rb) = (a.shape.len(), b.shape.len());
            want(prim, ra >= 2 && rb >= 2, "operand rank below 2")?;
            for t in [a, b] {
                want(prim, t.dtype != DType::I128 && t.dtype != DType::Idx, "i128 or idx operand")?;
            }
            want(prim, out.dtype != DType::Idx, "idx output")?;
            want(prim, a.shape[ra - 1] == b.shape[rb - 2], "contraction extents differ")?;
            let mut s = broadcast_shapes(&a.shape[..ra - 2], &b.shape[..rb - 2])?;
            s.push(a.shape[ra - 2]);
            s.push(b.shape[rb - 1]);
            want(prim, out.shape == s, "out.shape is not batch ++ [M, N]")
        }
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
            let x = x.unwrap();
            let a = *axis as usize;
            want(prim, a < x.shape.len(), "axis out of range")?;
            let mut s = x.shape.clone();
            s[a] = Dim::Fixed(1);
            want(prim, out.shape == s, "out.shape is not x.shape with axis kept as 1")?;
            if matches!(prim, Prim::ReduceMax { .. }) {
                want(prim, out.dtype == x.dtype, "dtype changes")?;
            }
            Ok(())
        }
        Prim::Clamp { lo, hi } => {
            want(prim, lo <= hi, "lo above hi")?;
            want(prim, out.dtype.contains(*lo as i128) && out.dtype.contains(*hi as i128), "bounds outside out.dtype")?;
            want(prim, out.shape == x.unwrap().shape, "shape changes")
        }
        Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => {
            // §6.5 *Type* (revision 2): 1 input of any dtype but i128; out.shape = x.shape; any
            // out.dtype.
            let x = x.unwrap();
            want(prim, x.dtype != DType::I128, "i128 input")?;
            want(prim, out.shape == x.shape, "shape changes")
        }
        Prim::TopK { axis, k } => {
            let x = x.unwrap();
            let a = *axis as usize;
            want(prim, a < x.shape.len(), "axis out of range")?;
            let Dim::Fixed(n) = x.shape[a] else {
                return bad(prim, "top-k along H");
            };
            want(prim, *k >= 1 && *k <= n, "k outside [1, n]")?;
            let mut s = x.shape.clone();
            s[a] = Dim::Fixed(*k);
            want(prim, out.shape == s, "out.shape is not x.shape with axis = k")?;
            want(prim, out.dtype == DType::Idx, "out.dtype is not idx")
        }
        Prim::StateWrite { state } => {
            let s = state_decl(prim, states, *state)?;
            want(prim, matches!(s.kind, StateKind::Fixed { .. }), "target is not a Fixed state")?;
            let st = TensorType::fixed(s.dtype, &s.shape);
            want(prim, x.unwrap().shape == st.shape, "input shape is not the state's")?;
            want(prim, *out == st, "out is not the state's type")
        }
        Prim::HistAppend { state } => {
            let s = state_decl(prim, states, *state)?;
            want(prim, matches!(s.kind, StateKind::Hist { .. }), "target is not a Hist state")?;
            let row = TensorType::fixed(s.dtype, &s.shape);
            want(prim, *x.unwrap() == row, "row is not the state's dtype and row shape")?;
            let mut shape = vec![Dim::H];
            shape.extend_from_slice(&row.shape);
            want(prim, *out == TensorType { dtype: s.dtype, shape }, "out is not (dtype, [H] ++ row)")
        }
    }
}

fn fixed_extent(d: Dim) -> Option<u64> {
    match d {
        Dim::Fixed(n) => Some(n as u64),
        Dim::H => None,
    }
}
