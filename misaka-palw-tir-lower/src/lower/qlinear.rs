//! **Pre-quantised projections** (GPTQ, AWQ): the checkpoint's integers, multiplied as stored.
//!
//! A pre-quantised weight is `W[o, i] = s[g, o] · (q[i, o] − z[g, o])` over groups `g` of input
//! columns (`crate::prequant`). Nothing is re-quantised: the program computes
//!
//! ```text
//! xg   = Reshape(Gather(x, order), [G, gs, 1])             -- GPTQ: every group contiguous
//! acc  = MatMul(codes:i8[out, G, 1, gs], xg) → i64 [out, G]  -- Σ_{i∈g} code[i, o]·x[i], exact
//! t    = a[g, o] · acc  (− c[g, o] · Σ_{i∈g} x[i])         -- the per-group scales, exact integers
//! T[o] = Σ_g t[g, o]  (+ the outlier columns, see below)
//! y    = N(T; m, s, z)                                     -- the one rounding, as every projection
//! ```
//!
//! * **Codes.** `q − z` when it fits `i8` (every format up to 7 bits, and symmetric 8-bit: the
//!   zero point is absorbed, and the `c` term does not exist); otherwise (8-bit asymmetric) `q − 128`
//!   with the zero point in `c = s · (z − 128)`, applied through the group sums of `x`.
//! * **Scales.** Each row's fp16 scales become integers at one unit `2^e[o]` per row: `a =
//!   s / 2^e[o]` with the row's largest `|a|` in `(2^19, 2^20]`. An fp16 scale has 11 significant
//!   bits, so every scale within `2^9` of its row's largest is represented exactly; a smaller one
//!   is rounded at `2^−20` of the row's largest (counted: [`QInts::inexact`], zero on every
//!   checkpoint seen so far). `2^e[o]` goes into the output narrowing's multiplier.
//! * **Outlier channels.** A split input (a few channels at their own coarser activation scale,
//!   `lower_linear`) gives those columns code 0 — with an offset term their input is masked out of
//!   the main product as well (`qkeep`) — and routes them through `wo:i32[out, k]`: the exact
//!   weights times the channel's scale ratio, in the row's unit `2^(e[o] + g[o])`, added as
//!   `acco · 2^g[o]`.
//!
//! Ranges (spec 04b §7, with `|a| ≤ 2^20`, `|c| ≤ 2^28`, `|wo| ≤ 2^24` stated by clamps): the sum
//! stays within `in · 2^43.6 + 2^62`, inside `i64` for every `in ≤ 2^16`.

use super::{Cx, Lb, Val, Want, decl, decl_ms, narrow, note_resid, per_layer};
use crate::error::{LowerError, Result};
use crate::lower::fill::{FillCtx, IntTensor};
use crate::prequant::{QLayout, QWeight};
use misaka_palw_tir as tir;
use std::sync::Arc;
use tir::DType;
use tir::builder::BlockBuilder;

/// `|a| ≤ 2^A_BITS` (per-group multipliers at the row's unit).
pub const A_BITS: i32 = 20;
/// `|c| ≤ 2^C_BITS` (per-group offsets, 8-bit asymmetric).
pub const C_BITS: i32 = 28;
/// `|wo| ≤ 2^WO_BITS` (outlier columns).
pub const WO_BITS: i32 = 24;
/// The largest outlier shift `g[o]`.
pub const G_MAX: u32 = 19;

/// A quantised projection's integers in its program layout.
#[derive(Clone, Debug)]
pub struct QInts {
    pub groups: usize,
    pub gs: usize,
    /// `[out, G, gs]`, the columns in the gathered order. Row-major, so a tile of rows is one
    /// contiguous span of the param (its close opens only those rows).
    pub codes: Vec<i8>,
    /// `[out, G]`.
    pub a: Vec<i32>,
    /// `[out, G]` (empty without the offset term).
    pub c: Vec<i32>,
    /// `[in]`: the column order (empty when the layout has none).
    pub order: Vec<u32>,
    /// `[out]`: each row's unit exponent.
    pub e: Vec<i32>,
    /// `[out, k]`, `[out]`: the outlier columns.
    pub wo: Vec<i32>,
    pub og: Vec<i8>,
    /// `(row, group)` multipliers or offsets that are not exact multiples of their row's unit.
    pub inexact: usize,
}

/// The smallest `e` with `v ≤ 2^(bits + e)` (`None` for `v = 0`).
fn unit_exp(v: f64, bits: i32) -> Option<i32> {
    if v == 0.0 || !v.is_finite() {
        return None;
    }
    let mut e = v.log2().ceil() as i32 - bits;
    while v > 2f64.powi(bits + e) {
        e += 1;
    }
    while v <= 2f64.powi(bits + e - 1) {
        e -= 1;
    }
    Some(e)
}

fn round_exact(v: f64, inexact: &mut usize) -> i64 {
    let r = v.round();
    if r != v {
        *inexact += 1;
    }
    r as i64
}

/// Build the layout's integers from the stored weight. `outliers` are the split input's outlier
/// columns and `ratio[j]` the scale of column `outliers[j]` over the main scale.
pub fn build(q: &QWeight, layout: QLayout, outliers: &[usize], ratio: &[f64]) -> Result<QInts> {
    let (out, inp) = (q.out, q.inp);
    let gs = if layout.group == 0 { inp } else { layout.group };
    if gs == 0 || inp % gs != 0 {
        return Err(LowerError::not_lowerable(format!("{inp} input columns are not a multiple of the group size {gs}")));
    }
    let groups = inp / gs;
    let order: Vec<u32> = if layout.order { q.order()? } else { (0..inp as u32).collect() };
    let fg = q.groups();
    // Every program group lies inside one stored group.
    let mut fmt_group = Vec::with_capacity(groups);
    for k in 0..groups {
        let f = q.gidx[order[k * gs] as usize];
        if (0..gs).any(|j| q.gidx[order[k * gs + j] as usize] != f) {
            return Err(LowerError::not_lowerable(format!(
                "quantised weight ({}): program group {k} straddles stored groups (group size {gs}, stored {})",
                q.label, q.group
            )));
        }
        fmt_group.push(f as usize);
    }
    // With the offset term, codes are the stored integers shifted into `i8` (unsigned 8-bit ones
    // by 128) and the zero point and float offset move into `c`.
    let off: i16 = if layout.offset_term && q.bits == 8 && !q.signed { 128 } else { 0 };
    if !layout.offset_term && q.min.is_some() {
        return Err(LowerError::eval(format!("internal: quantised weight ({}) has float offsets but its layout has no offset term", q.label)));
    }
    let mut is_outlier = vec![false; inp];
    for c in outliers {
        is_outlier[*c] = true;
    }
    let mut codes = vec![0i8; groups * out * gs];
    let mut af = vec![0f64; groups * out];
    let mut cf = vec![0f64; if layout.offset_term { groups * out } else { 0 }];
    for k in 0..groups {
        let f = fmt_group[k];
        for o in 0..out {
            let (s, z) = (q.scale[o * fg + f], q.zero[o * fg + f]);
            let m = q.min.as_ref().map_or(0.0, |m| m[o * fg + f]);
            af[o * groups + k] = s;
            let shift = if layout.offset_term {
                cf[o * groups + k] = s * (z - off) as f64 + m;
                off
            } else {
                z
            };
            for j in 0..gs {
                let col = order[k * gs + j] as usize;
                // An outlier column goes through `wo`: its code is 0, and with an offset term its
                // input is zeroed for the main product too (the `qkeep` mask), so `c · Σx` skips it.
                let v = if is_outlier[col] { 0 } else { q.q[o * inp + col] - shift };
                codes[(o * groups + k) * gs + j] = i8::try_from(v).map_err(|_| {
                    LowerError::not_lowerable(format!("quantised weight ({}): code {v} outside i8 (layout {layout:?})", q.label))
                })?;
            }
        }
    }
    let mut e = vec![0i32; out];
    let mut a = vec![0i32; groups * out];
    let mut c = vec![0i32; cf.len()];
    let mut inexact = 0usize;
    for o in 0..out {
        let amax = (0..groups).fold(0f64, |m, k| m.max(af[o * groups + k].abs()));
        let cmax = (0..cf.len() / out.max(1)).fold(0f64, |m, k| m.max(cf[o * groups + k].abs()));
        let eo = match (unit_exp(amax, A_BITS), unit_exp(cmax, C_BITS)) {
            (Some(x), Some(y)) => x.max(y),
            (Some(x), None) | (None, Some(x)) => x,
            (None, None) => 0,
        };
        e[o] = eo;
        let unit = 2f64.powi(eo);
        let mut inexact_row = 0usize;
        for k in 0..groups {
            a[o * groups + k] = round_exact(af[o * groups + k] / unit, &mut inexact_row) as i32;
            if !cf.is_empty() {
                c[o * groups + k] = round_exact(cf[o * groups + k] / unit, &mut inexact_row) as i32;
            }
        }
        inexact += inexact_row;
    }
    // Outlier columns: the exact weight times the channel's scale ratio, at the row's unit.
    let kk = outliers.len();
    let mut wo = vec![0i32; out * kk];
    let mut og = vec![0i8; out];
    for o in 0..out {
        let unit = 2f64.powi(e[o]);
        let vals: Vec<f64> = outliers.iter().zip(ratio).map(|(col, r)| q.value(o, *col) * r / unit).collect();
        let mx = vals.iter().fold(0f64, |m, v| m.max(v.abs()));
        let g = unit_exp(mx, WO_BITS).unwrap_or(0).max(0);
        if g as u32 > G_MAX {
            return Err(LowerError::not_lowerable(format!(
                "quantised weight ({}): an outlier channel's scale ratio needs a shift of {g} (> {G_MAX})",
                q.label
            )));
        }
        og[o] = g as i8;
        for (j, v) in vals.iter().enumerate() {
            wo[o * kk + j] = (v / 2f64.powi(g)).round() as i32;
        }
    }
    Ok(QInts {
        groups,
        gs,
        codes,
        a,
        c,
        order: if layout.order { order } else { Vec::new() },
        e,
        wo,
        og,
        inexact,
    })
}

/// A quantised projection `W·x (+ b)` from the stored integers (see the module docs).
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_linear_q(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    w: u32,
    bias: Option<u32>,
    site: &str,
    want: &Want,
    layout: QLayout,
) -> Result<Val> {
    let hl = cx.hl;
    let d = &hl.params[w as usize];
    let (out, inp) = (d.shape[0], d.shape[1]);
    if x.len != 0 && x.len != inp {
        return Err(LowerError::eval(format!("internal: linear `{}` reads {} values, weight has {inp} columns", d.name, x.len)));
    }
    if x.dt != DType::I16 {
        return Err(LowerError::not_lowerable(format!("the quantised projection `{}` reads a {} input", d.name, x.dt.name())));
    }
    let gs = if layout.group == 0 { inp } else { layout.group };
    if inp % gs != 0 {
        return Err(LowerError::not_lowerable(format!("`{}`: {inp} columns are not a multiple of the group size {gs}", d.name)));
    }
    let groups = inp / gs;
    let pl = per_layer(lb);
    let k = x.key.split();
    let (kx, ky) = (x.key.clone(), want.key.clone());
    // One memo key per (param, outlier set): every fill of this projection reads the same build.
    let ints: Arc<dyn Fn(&FillCtx<'_>) -> Result<Arc<QInts>> + Send + Sync> = {
        let kx = kx.clone();
        Arc::new(move |c| c.qints(w, layout, &kx))
    };
    let z = match bias {
        Some(bp) => {
            let ky = want.key.clone();
            Some(decl(
                b,
                cx,
                lb,
                &format!("{site}.z"),
                DType::I64,
                &[out],
                pl,
                Arc::new(move |c| {
                    let bv = c.f(bp)?;
                    let sy = c.scale_vec(&ky, bv.data.len())?;
                    Ok(IntTensor::i64(
                        vec![bv.data.len()],
                        bv.data.iter().zip(&sy).map(|(v, s)| (*v as f64 / s).round() as i64).collect(),
                    ))
                }),
            )?)
        }
        None => None,
    };
    let f = ints.clone();
    let codes = decl(
        b,
        cx,
        lb,
        &d.name,
        DType::I8,
        &[out, groups, 1, gs],
        d.per_layer,
        Arc::new(move |c| Ok(IntTensor::i8(vec![out, groups, 1, gs], f(c)?.codes.clone()))),
    )?;
    // With an offset term the outlier channels leave the main product entirely (their `c · x`
    // would remain otherwise): `x · keep`, `keep ∈ {0, 1}`.
    let xm = if layout.offset_term && k > 0 {
        let kk = kx.clone();
        let keep = decl(
            b,
            cx,
            lb,
            &format!("{site}.qkeep"),
            DType::I8,
            &[inp],
            pl,
            Arc::new(move |c| {
                let mut v = vec![1i8; inp];
                for o in c.outliers(&kk)? {
                    v[o] = 0;
                }
                Ok(IntTensor::i8(vec![inp], v))
            }),
        )?;
        let keep = b.clamp(keep, 0, 1, DType::I8);
        b.mul(x.r, keep, DType::I16)
    } else {
        x.r
    };
    let xin = if layout.order {
        let f = ints.clone();
        let ord = decl(
            b,
            cx,
            lb,
            &format!("{site}.qorder"),
            DType::Idx,
            &[inp],
            pl,
            Arc::new(move |c| Ok(IntTensor::idx(vec![inp], f(c)?.order.clone()))),
        )?;
        // A param's range is its dtype's: state it for the analysis (never fires).
        let oi = b.clamp(ord, 0, inp as i64 - 1, DType::Idx);
        b.gather(xm, oi, 0, 0)
    } else {
        xm
    };
    // `[out, G, 1, gs] × [G, gs, 1]`: one dot product per (row, group), batched over rows and
    // groups. The codes are declared row-major (axis 0 is the output row), so an inventory leaf is
    // one row and a tile of rows opens only its rows.
    let xg = b.reshape_fixed(xin, &[groups as u32, gs as u32, 1]);
    let acc = b.matmul(codes, xg, DType::I64);
    let acc = b.reshape_fixed(acc, &[out as u32, groups as u32]);
    let f = ints.clone();
    let ap = decl(
        b,
        cx,
        lb,
        &format!("{site}.qa"),
        DType::I32,
        &[out, groups],
        pl,
        Arc::new(move |c| Ok(IntTensor::i32(vec![out, groups], f(c)?.a.clone()))),
    )?;
    let ac = b.clamp(ap, -(1i64 << A_BITS), 1i64 << A_BITS, DType::I32);
    let mut t = b.mul(ac, acc, DType::I64);
    if layout.offset_term {
        let xs = b.reduce_sum(xg, 1, DType::I64);
        let xs = b.reshape_fixed(xs, &[groups as u32]);
        let f = ints.clone();
        let cp = decl(
            b,
            cx,
            lb,
            &format!("{site}.qc"),
            DType::I32,
            &[out, groups],
            pl,
            Arc::new(move |c| Ok(IntTensor::i32(vec![out, groups], f(c)?.c.clone()))),
        )?;
        let cc = b.clamp(cp, -(1i64 << C_BITS), 1i64 << C_BITS, DType::I32);
        let offs = b.mul(cc, xs, DType::I64);
        t = b.sub(t, offs, DType::I64);
    }
    let t = if groups > 1 { b.reduce_sum(t, 1, DType::I64) } else { t };
    let mut total = b.reshape_fixed(t, &[out as u32]);
    if k > 0 {
        let kk = kx.clone();
        let oidx = decl(
            b,
            cx,
            lb,
            &format!("{site}.oidx"),
            DType::Idx,
            &[k],
            pl,
            Arc::new(move |c| Ok(IntTensor::idx(vec![k], c.outliers(&kk)?.iter().map(|c| *c as u32).collect()))),
        )?;
        let f = ints.clone();
        let wo = decl(
            b,
            cx,
            lb,
            &format!("{site}.wo"),
            DType::I32,
            &[out, k],
            pl,
            Arc::new(move |c| Ok(IntTensor::i32(vec![out, k], f(c)?.wo.clone()))),
        )?;
        let f = ints.clone();
        let og = decl(
            b,
            cx,
            lb,
            &format!("{site}.og"),
            DType::I8,
            &[out],
            pl,
            Arc::new(move |c| Ok(IntTensor::i8(vec![out], f(c)?.og.clone()))),
        )?;
        let oi = b.clamp(oidx, 0, inp as i64 - 1, DType::Idx);
        let xo = b.gather(x.r, oi, 0, 0);
        let xo = b.reshape_fixed(xo, &[k as u32, 1]);
        let woc = b.clamp(wo, -(1i64 << WO_BITS), 1i64 << WO_BITS, DType::I32);
        let acco = b.matmul(woc, xo, DType::I64);
        let acco = b.reshape_fixed(acco, &[out as u32]);
        let p2 = b.pow2_128_of(og, G_MAX);
        let sh = b.mul(acco, p2, DType::I64);
        total = b.add(total, sh, DType::I64);
    }
    let f = ints;
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        out,
        Arc::new(move |c| {
            let q = f(c)?;
            let (sx, sy) = (c.scale(&kx)?, c.scale_vec(&ky, out)?);
            Ok(q.e.iter().zip(&sy).map(|(e, sy)| sx * 2f64.powi(*e) / sy).collect())
        }),
    )?;
    let r = narrow(b, total, m, s, z, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    let v = Val { r, dt: want.dt, key: want.key.clone(), len: out, site: site.to_string() };
    note_resid(cx, lb, &v);
    Ok(v)
}

/// One projection of the selected experts from their stored integers: `codes:i8[E, rows, G, gs]`,
/// `a`/`c:i32[E, rows, G]` (and GPTQ's `order:idx[E, cols]`) gathered by the `k` expert ids, the
/// grouped MatMul against `input` (`[cols, 1]` shared by every expert, or `[k, cols, 1]`), the
/// per-group scales exact, the groups summed, and one narrowing per (expert, row) — the W8 path's
/// `m`/`s` gathered the same way. An expert's input is never split, so there is no outlier path.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_experts_q(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    p: u32,
    bias: Option<u32>,
    input: tir::Ref,
    in_key: super::ScaleKey,
    name: &str,
    out: super::ScaleKey,
    dt: DType,
    idx: tir::Ref,
    k: usize,
    layout: QLayout,
) -> Result<tir::Ref> {
    let hl = cx.hl;
    let pd = &hl.params[p as usize];
    let (e, rows, cols) = (pd.shape[0], pd.shape[1], pd.shape[2]);
    let gs = if layout.group == 0 { cols } else { layout.group };
    if cols % gs != 0 {
        return Err(LowerError::not_lowerable(format!("`{}`: {cols} columns are not a multiple of the group size {gs}", pd.name)));
    }
    let groups = cols / gs;
    let pl = per_layer(lb);
    let ints: Arc<dyn Fn(&FillCtx<'_>) -> Result<Arc<Vec<QInts>>> + Send + Sync> = Arc::new(move |c| c.qints_stack(p, layout));
    let cat8 = |f: fn(&QInts) -> &Vec<i8>, v: &[QInts]| v.iter().flat_map(|q| f(q).iter().copied()).collect::<Vec<i8>>();
    let f = ints.clone();
    let codes = decl(
        b,
        cx,
        lb,
        &pd.name,
        DType::I8,
        &[e, rows, groups, gs],
        pd.per_layer,
        Arc::new(move |c| Ok(IntTensor::i8(vec![e, rows, groups, gs], cat8(|q| &q.codes, &f(c)?)))),
    )?;
    let f = ints.clone();
    let ap = decl(
        b,
        cx,
        lb,
        &format!("{name}.qa"),
        DType::I32,
        &[e, rows, groups],
        pl,
        Arc::new(move |c| Ok(IntTensor::i32(vec![e, rows, groups], f(c)?.iter().flat_map(|q| q.a.iter().copied()).collect()))),
    )?;
    // The shared input `[cols]`, or one row per selected expert `[k, cols]`.
    let shape = b.shape(input);
    let per_expert = shape.len() == 3;
    let flat = if per_expert { b.reshape_fixed(input, &[k as u32, cols as u32]) } else { b.reshape_fixed(input, &[cols as u32]) };
    let xin = if layout.order {
        let f = ints.clone();
        let ord = decl(
            b,
            cx,
            lb,
            &format!("{name}.qorder"),
            DType::Idx,
            &[e, cols],
            pl,
            Arc::new(move |c| Ok(IntTensor::idx(vec![e, cols], f(c)?.iter().flat_map(|q| q.order.iter().copied()).collect()))),
        )?;
        let ord = b.clamp(ord, 0, cols as i64 - 1, DType::Idx);
        let ok = b.gather(ord, idx, 0, 0);
        if per_expert { b.gather(flat, ok, 1, 1) } else { b.gather(flat, ok, 0, 0) }
    } else {
        flat
    };
    // The codes and scales are stored row-major per expert (`[E, rows, G, gs]`, `[E, rows, G]`:
    // a tile of rows is contiguous in each selected expert); the product runs group-major,
    // `[k, G, rows, gs] × [(k,) G, gs, 1]`, after one transpose of the selected experts.
    let own = per_expert || layout.order;
    let xg = if own {
        b.reshape_fixed(xin, &[k as u32, groups as u32, gs as u32, 1])
    } else {
        b.reshape_fixed(xin, &[groups as u32, gs as u32, 1])
    };
    let sel = b.gather(codes, idx, 0, 0);
    let sel = b.transpose(sel, &[0, 2, 1, 3]);
    let acc = b.matmul(sel, xg, DType::I64);
    let acc = b.reshape_fixed(acc, &[k as u32, groups as u32, rows as u32]);
    let asel = b.gather(ap, idx, 0, 0);
    let asel = b.transpose(asel, &[0, 2, 1]);
    let ac = b.clamp(asel, -(1i64 << A_BITS), 1i64 << A_BITS, DType::I32);
    let mut t = b.mul(ac, acc, DType::I64);
    if layout.offset_term {
        let f = ints.clone();
        let cp = decl(
            b,
            cx,
            lb,
            &format!("{name}.qc"),
            DType::I32,
            &[e, rows, groups],
            pl,
            Arc::new(move |c| Ok(IntTensor::i32(vec![e, rows, groups], f(c)?.iter().flat_map(|q| q.c.iter().copied()).collect()))),
        )?;
        let csel = b.gather(cp, idx, 0, 0);
        let csel = b.transpose(csel, &[0, 2, 1]);
        let cc = b.clamp(csel, -(1i64 << C_BITS), 1i64 << C_BITS, DType::I32);
        let xs = b.reduce_sum(xg, if own { 2 } else { 1 }, DType::I64);
        let xs = if own { b.reshape_fixed(xs, &[k as u32, groups as u32, 1]) } else { b.reshape_fixed(xs, &[groups as u32, 1]) };
        let offs = b.mul(cc, xs, DType::I64);
        t = b.sub(t, offs, DType::I64);
    }
    let t = if groups > 1 { b.reduce_sum(t, 1, DType::I64) } else { t };
    let total = b.reshape_fixed(t, &[k as u32, rows as u32]);
    let f = ints;
    let (ki, ko) = (in_key, out.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        name,
        e * rows,
        Arc::new(move |c| {
            let q = f(c)?;
            let (si, so) = (c.scale(&ki)?, c.scale(&ko)?);
            Ok(q.iter().flat_map(|x| x.e.iter().map(|eu| si * 2f64.powi(*eu) / so).collect::<Vec<_>>()).collect())
        }),
    )?;
    let z = match bias {
        Some(bp) => {
            let ko = out;
            let zp = decl(
                b,
                cx,
                lb,
                &format!("{name}.z"),
                DType::I64,
                &[e * rows],
                pl,
                Arc::new(move |c| {
                    let bv = c.f(bp)?;
                    let so = c.scale(&ko)?;
                    Ok(IntTensor::i64(vec![bv.data.len()], bv.data.iter().map(|v| (*v as f64 / so).round() as i64).collect()))
                }),
            )?;
            let zp = b.reshape_fixed(zp, &[e as u32, rows as u32]);
            Some(b.gather(zp, idx, 0, 0))
        }
        None => None,
    };
    let m = b.reshape_fixed(m, &[e as u32, rows as u32]);
    let s = b.reshape_fixed(s, &[e as u32, rows as u32]);
    let mk = b.gather(m, idx, 0, 0);
    let sk = b.gather(s, idx, 0, 0);
    Ok(narrow(b, total, mk, sk, z, dt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prequant::{QFormat, unpack_gptq};
    use crate::weights::Tensor;

    /// The integers reassemble the stored weight exactly: `a·code − c = W / 2^e`.
    #[test]
    fn integers_reassemble_the_weight() {
        let (inp, out, bits) = (16usize, 8usize, 8u8);
        let pack = 32 / bits as usize;
        let q: Vec<Vec<u32>> = (0..inp).map(|i| (0..out).map(|o| ((i * 29 + o * 13 + 7) % 256) as u32).collect()).collect();
        let mut qw = vec![0u32; inp / pack * out];
        for i in 0..inp {
            for o in 0..out {
                qw[(i / pack) * out + o] |= q[i][o] << (bits as usize * (i % pack));
            }
        }
        let z = |g: usize, o: usize| ((g * 50 + o * 17 + 3) % 256) as u32;
        let mut qz = vec![0u32; 4 * out / pack];
        for g in 0..4 {
            for o in 0..out {
                qz[g * (out / pack) + o / pack] |= ((z(g, o) + 255) % 256) << (bits as usize * (o % pack));
            }
        }
        // fp16-exact scales.
        let scales = Tensor::new(vec![4, out], (0..4 * out).map(|k| (1 + k % 7) as f32 / 1024.0).collect());
        let g_idx: Vec<i32> = (0..inp).map(|i| ((i * 5) % 16 / 4) as i32).collect();
        let fmt = QFormat::Gptq { bits, group: 4, desc_act: true, sym: false, v2: false };
        let qwt: Vec<i32> = qw.into_iter().map(|x| x as i32).collect();
        let qzt: Vec<i32> = qz.into_iter().map(|x| x as i32).collect();
        let w = unpack_gptq(&fmt, (&[inp / pack, out], &qwt), (&[4, out / pack], &qzt), &scales, Some((&[inp], &g_idx))).unwrap();
        let layout = fmt.layout();
        assert!(layout.offset_term && layout.order);
        let outl = [3usize, 9];
        let ratio = [3.5f64, 1.25];
        let qi = build(&w, layout, &outl, &ratio).unwrap();
        assert_eq!(qi.inexact, 0);
        // Σ over the main path + the outlier path, against the exact weight, for a probe input.
        let x: Vec<f64> = (0..inp).map(|i| (i as f64 - 7.5) * 3.0).collect();
        for o in 0..out {
            let unit = 2f64.powi(qi.e[o]);
            let mut t = 0f64;
            for kg in 0..qi.groups {
                let (mut acc, mut xs) = (0f64, 0f64);
                for j in 0..qi.gs {
                    let col = qi.order[kg * qi.gs + j] as usize;
                    // The outlier channels are masked out of the main product (`qkeep`).
                    let xv = if outl.contains(&col) { 0.0 } else { x[col] };
                    acc += qi.codes[(o * qi.groups + kg) * qi.gs + j] as f64 * xv;
                    xs += xv;
                }
                t += qi.a[o * qi.groups + kg] as f64 * acc - qi.c[o * qi.groups + kg] as f64 * xs;
            }
            for (j, col) in outl.iter().enumerate() {
                t += qi.wo[o * 2 + j] as f64 * 2f64.powi(qi.og[o] as i32) * x[*col] / ratio[j];
            }
            let want: f64 = (0..inp).map(|i| w.value(o, i) * x[i]).sum::<f64>() / unit;
            assert!((t - want).abs() <= 1e-6 * want.abs().max(1.0) + 2.0, "row {o}: {t} vs {want}");
        }
    }

    #[test]
    fn unit_exponents_bound_their_values() {
        for v in [1.0, 0.75, 3.0e-4, 1024.0, 1025.0] {
            let e = unit_exp(v, 20).unwrap();
            assert!(v <= 2f64.powi(20 + e) && v > 2f64.powi(19 + e), "{v} → {e}");
        }
        assert_eq!(unit_exp(0.0, 20), None);
    }
}
