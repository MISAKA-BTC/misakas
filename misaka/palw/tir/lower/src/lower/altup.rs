//! **The Gemma-3n family of features** (`RESIDUAL_ALTUP_V1`, `FFN_ACTIVATION_SPARSITY_V1`): a data-dependent mix of streams, a
//! magnitude match and a Gaussian top-k — each a combination of the 25 primitives. No new primitive and no new court kernel.
//!
//! * [`stream_mix`] is one batched product of the `K × K` coefficient matrix with the `K` stream rows (an exact `i64` product per
//!   term, the sum over the streams in `i64`, one narrowing).
//! * [`rms_match`] is `x · rms(r) / √max(mean x², floor)`: the two means in `i128`, their ratio in Q40, one integer square root
//!   ([`isqrt`]), one product with the (code) row and one narrowing.
//! * [`gaussian_topk`] centres exactly (`c = n·x − Σx`, so `c = n·(x − mean)` with no division), takes `Σc² = n³·Var` in `i128`,
//!   the cutoff `z·√(Σc²/n) = z·n·std` through the same integer square root, and keeps `relu(c − cutoff)`.
//!
//! Every value is data, never a shape: the whole row is always computed.

use super::*;
use tir::Cmp;
use tir::arith::{K, ONE};

/// `√V` for a non-negative `V ≤ 2^63 − 1` (an `i128` vector of one lane), to ~24 bits: with `v = V·2^24` (its Q24 form),
/// `h = clamp(⌊(⌊log2 v⌋ − 24)/2⌋, 0, 51)` and `r = IntRsqrt(v / 2^(2h)) = 2^36/√(v/2^2h)`, `√V = V·r / 2^(h + 24)`.
/// The exponent is taken out exactly as the library's `rms_unit_q24` does; `V = 0` gives 0.
pub(super) fn isqrt(b: &mut BlockBuilder<'_>, v: tir::Ref) -> tir::Ref {
    let v = b.clamp(v, 0, i64::MAX, DType::I128);
    let one = b.c(DType::I64, ONE);
    let q = b.mul(v, one, DType::I128);
    let bit = b.log2_floor(q, DType::I32);
    let k = b.c(DType::I32, K as i128);
    let t = b.sub(bit, k, DType::I32);
    let two = b.c(DType::I32, 2);
    let h = b.div(t, two, Rounding::Floor, DType::I32);
    let h = b.clamp(h, 0, 51, DType::I32);
    let h2 = b.mul(h, two, DType::I32);
    let p2 = b.pow2_128_of(h2, 102);
    let m = b.div(q, p2, Rounding::Floor, DType::I128);
    let m = b.clamp(m, 0, i64::MAX, DType::I64);
    let r = b.int_rsqrt(m);
    let prod = b.mul(v, r, DType::I128);
    let off = b.c(DType::I32, K as i128);
    let sh = b.add(h, off, DType::I32);
    let p1 = b.pow2_128_of(sh, 75);
    let y = b.div(prod, p1, Rounding::Floor, DType::I128);
    // √(2^63) < 2^32: the bound the products below are analysed with.
    b.clamp(y, 0, 1 << 32, DType::I64)
}

/// The mean of squares of a vector of `n` values (`i16` or `i32` codes), `⌊Σx² / n⌋`, `i128`, one lane.
fn mean_square(b: &mut BlockBuilder<'_>, x: tir::Ref, n: usize) -> tir::Ref {
    let sq = b.mul(x, x, DType::I64);
    let sum = b.reduce_sum(sq, 0, DType::I128);
    let nn = b.c(DType::I64, n as i128);
    b.div(sum, nn, Rounding::Floor, DType::I128)
}

/// `out[i·D + d] = Σ_j C[i,j]·x[j·D + d]` (`RESIDUAL_ALTUP_V1`'s prediction): the `i64` products of the coefficient codes with the stream
/// values (`i32` at the residual scale or `i16` codes), summed over the streams, narrowed once.
#[allow(clippy::too_many_arguments)]
pub(super) fn stream_mix(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    c: &Val,
    n_in: usize,
    n_out: usize,
    transpose: bool,
    site: &str,
    want: &Want,
) -> Result<Val> {
    super::generic::unsplit(x, site)?;
    super::generic::unsplit(c, site)?;
    let n = super::generic::len_of(b, x.r)?;
    if n % n_in != 0 || super::generic::len_of(b, c.r)? != n_in * n_out {
        return Err(LowerError::eval("internal: a stream mix of mismatched widths"));
    }
    let d = (n / n_in) as u32;
    let (ni, no) = (n_in as u32, n_out as u32);
    // `C[i, j]` at `[i, j]`, summing over `j` (axis 1) — or stored `[j, i]`, summing over `j` (axis 0).
    let (cr, xr, axis) = if transpose {
        (b.reshape_fixed(c.r, &[ni, no, 1]), b.reshape_fixed(x.r, &[ni, 1, d]), 0)
    } else {
        (b.reshape_fixed(c.r, &[no, ni, 1]), b.reshape_fixed(x.r, &[1, ni, d]), 1)
    };
    let p = b.mul(cr, xr, DType::I64);
    let s = b.reduce_sum(p, axis, DType::I64);
    let s = b.reshape_fixed(s, &[no * d]);
    let (kx, kc) = (x.key.clone(), c.key.clone());
    super::generic::narrow_to(b, cx, lb, s, n_out * d as usize, Arc::new(move |f| Ok(f.scale(&kx)? * f.scale(&kc)?)), site, want)
}

/// `x · rms(r) / √max(mean(x²), floor)` (`RESIDUAL_ALTUP_V1`'s magnitude match). `x` is `i16` codes, `r` codes or `i32`.
///
/// In code units `x_f = s_x·x`, `r_f = s_r·r`: the result is `x_f · √ρ` with `ρ = (M_r·s_r²) / (max(M_x, F)·s_x²)` the ratio of the
/// mean squares in the float domain (`M` the integer mean squares, `F = floor / s_x²`, both params filled from the calibrated
/// scales). `ρ` is taken in Q40 and clamped to `i64`, so a row `2^11` times larger in magnitude than its reference saturates
/// instead of wrapping.
#[allow(clippy::too_many_arguments)]
pub(super) fn rms_match(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    r: &Val,
    floor: f64,
    site: &str,
    want: &Want,
) -> Result<Val> {
    super::generic::unsplit(x, site)?;
    super::generic::unsplit(r, site)?;
    if x.dt != DType::I16 {
        return Err(LowerError::eval("internal: a magnitude match reads i16 codes"));
    }
    let nx = super::generic::len_of(b, x.r)?;
    let nr = super::generic::len_of(b, r.r)?;
    let mx = mean_square(b, x.r, nx);
    let mr = mean_square(b, r.r, nr);
    // F = floor / s_x², at least 1 (a mean square of zero would divide by zero).
    let kx = x.key.clone();
    let f = decl(
        b,
        cx,
        lb,
        &format!("{site}.floor"),
        DType::I64,
        &[1],
        per_layer(lb),
        Arc::new(move |c| {
            let sx = c.scale(&kx)?;
            let v = (floor / (sx * sx)).round().clamp(1.0, i64::MAX as f64) as i64;
            Ok(IntTensor::i64(vec![1], vec![v]))
        }),
    )?;
    let f = b.cast(f, DType::I128);
    let ge = b.compare(mx, f, Cmp::Ge);
    let m = b.select(ge, mx, f, DType::I128);
    let m = b.clamp(m, 1, i64::MAX, DType::I128);
    // The ratio of the means in the FLOAT domain, in Q40: `g = 2^40·(s_r/s_x)²` (a param) takes the two code scales out, so a
    // reference at the residual's wide scale and a row of 16-bit codes still give a ratio near one.
    let (kx, kr) = (x.key.clone(), r.key.clone());
    let g = decl(
        b,
        cx,
        lb,
        &format!("{site}.ratio"),
        DType::I64,
        &[1],
        per_layer(lb),
        Arc::new(move |c| {
            let q = c.scale(&kr)? / c.scale(&kx)?;
            let v = ((1u64 << 40) as f64 * q * q).round().clamp(1.0, (1u64 << 62) as f64) as i64;
            Ok(IntTensor::i64(vec![1], vec![v]))
        }),
    )?;
    let num = b.mul(mr, g, DType::I128);
    let q = b.div(num, m, Rounding::Floor, DType::I128);
    let rt = isqrt(b, q);
    let p = b.mul(x.r, rt, DType::I64);
    let kx = x.key.clone();
    super::generic::narrow_to(b, cx, lb, p, nx, Arc::new(move |c| Ok(c.scale(&kx)? / (1u64 << 20) as f64)), site, want)
}

/// `relu(x − (mean + z·std))` over the row (`FFN_ACTIVATION_SPARSITY_V1`), `x` `i16` codes: `c = n·x − Σx` exactly, the cutoff
/// `z·√(Σc²/n)` (`= z·n·std`, Q24 `z`), `relu(c − cutoff)` narrowed at `s_x / n` per unit. `z` and whether the layer sparsifies at
/// all are the layer's data (`[zq, active]`, a param filled from `layers`); a dense layer passes `x` through, re-expressed at the
/// output's scale — the two branches are both computed and one is selected.
#[allow(clippy::too_many_arguments)]
pub(super) fn gaussian_topk(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    layers: &[(usize, Option<f64>)],
    site: &str,
    want: &Want,
) -> Result<Val> {
    super::generic::unsplit(x, site)?;
    if x.dt != DType::I16 {
        return Err(LowerError::eval("internal: a Gaussian top-k reads i16 codes"));
    }
    if layers.is_empty() {
        return Err(LowerError::eval("internal: a Gaussian top-k that no layer runs"));
    }
    let n = super::generic::len_of(b, x.r)?;
    // The layer's data: `z` in Q24 and the on/off flag.
    let table: Vec<(usize, i64, i64)> =
        layers.iter().map(|(l, z)| (*l, z.map_or(0, |z| (z * ONE as f64).round() as i64), i64::from(z.is_some()))).collect();
    let zp = decl(
        b,
        cx,
        lb,
        &format!("{site}.z"),
        DType::I64,
        &[2],
        per_layer(lb),
        Arc::new(move |c| {
            let l = c.hl.model_layer(c.layer.ok_or_else(|| LowerError::eval("a Gaussian top-k constant is filled outside a layer"))?);
            let (_, zq, on) = table.iter().find(|(ml, _, _)| *ml == l).ok_or_else(|| LowerError::eval(format!("layer {l} does not run this Gaussian top-k")))?;
            Ok(IntTensor::i64(vec![2], vec![*zq, *on]))
        }),
    )?;
    let zc = b.slice(zp, 0, 0, 1);
    let zc = b.clamp(zc, -(1 << 30), 1 << 30, DType::I64);
    let on = b.slice(zp, 0, 1, 1);
    let nn = b.c(DType::I64, n as i128);
    let nx = b.mul(x.r, nn, DType::I64);
    let sum = b.reduce_sum(x.r, 0, DType::I64);
    let c = b.sub(nx, sum, DType::I64);
    let c2 = b.mul(c, c, DType::I64);
    let ss = b.reduce_sum(c2, 0, DType::I128);
    let nn128 = b.c(DType::I64, n as i128);
    let v = b.div(ss, nn128, Rounding::Floor, DType::I128);
    let root = isqrt(b, v);
    let cut = b.mul(root, zc, DType::I64);
    let cut = b.shr(cut, K, Rounding::HalfAwayFromZero, DType::I64);
    let d = b.sub(c, cut, DType::I64);
    let relu = b.clamp(d, 0, i64::MAX, DType::I64);
    let kx = x.key.clone();
    let sparse = super::generic::narrow_to(b, cx, lb, relu, n, Arc::new(move |f| Ok(f.scale(&kx)? / n as f64)), site, want)?;
    // A dense layer: the row itself at the output's scale.
    let dense = coerce(b, cx, lb, x, DType::I16, &want.key)?;
    let zero = b.c(DType::I64, 0);
    let active = b.compare(on, zero, Cmp::Gt);
    let r = b.select(active, sparse.r, dense.r, DType::I16);
    b.commit(r);
    Ok(Val { r, ..sparse })
}
