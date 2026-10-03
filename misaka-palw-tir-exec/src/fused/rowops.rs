//! **The two unit-row normalisations of the library** (`tir_library_v1`, legacy): [`L2UnitQ15`] (`l2_unit_q15`, 17 nodes) and
//! [`RmsUnitQ24`] (`rms_unit_q24`, 21 nodes) — `x / ‖x‖` in Q15 codes and `x / √(mean(x²) + ε)` in Q24 along the last axis.
//!
//! Both templates are a chain of per-row scalar steps (an exact sum of squares, a `Log2Floor`, a floor division by a power of two
//! the shift's gather picks, `IntRsqrt`, a product and a second floor division) that the generic backend runs as a dozen
//! separate passes over `rows`-long tensors, each through a table gather. The implementation walks each row once: the exact
//! sum in `i128` (any order, the order-free rule), and then the same scalar steps on the same values — every floor division by
//! `2^s` is the arithmetic shift `>> s` (a floor, for either sign), every clamp is the template's own.
//!
//! The operands are static-shaped rows (`[n]` or `[rows, n]`), `x` an `i16` (L2) or `i16`/`i32` (RMS) tensor, `ε` an `i64`
//! one-element tensor. Where the plan proves a node of the region can fail it is not fused ([`super::enable`]).

use misaka_palw_tir::arith;
use misaka_palw_tir::builder::BlockBuilder;
use crate::elem::Buf;
use misaka_palw_tir::program::Node;
use misaka_palw_tir::{DType, Ref, TensorType, TirResult};
use rayon::prelude::*;

use super::{Bound, FusedIo, FusedKernelV1, Variant, static_shape, store, values};
use crate::scalar::int_rsqrt_i64;

const K: i128 = arith::K as i128;

/// `max(0, ⌊(⌊log2 v⌋ − K) / 2⌋)` up to `cap` — the exponent the templates take out before `IntRsqrt`.
#[inline(always)]
fn half_exponent(v: i128, cap: i128) -> i128 {
    (arith::log2_floor(v) - K).div_euclid(2).clamp(0, cap)
}

fn rows_of(shape: &[usize]) -> Option<(usize, usize)> {
    let n = *shape.last()?;
    if n == 0 || shape.len() > 2 {
        return None;
    }
    Some((shape.iter().product::<usize>() / n, n))
}

/// An operand's rows as native `i32` lanes: a contiguous `i16`/`i32` tensor is borrowed in place, anything else is read once.
enum Lanes<'a> {
    I16(&'a [i16]),
    I32(&'a [i32]),
    Owned(Vec<i32>),
}

impl<'a> Lanes<'a> {
    fn of(op: &crate::kernels::Opd<'a>) -> Lanes<'a> {
        if let Some(v) = op.contiguous::<i16>() {
            return Lanes::I16(v);
        }
        if let Some(v) = op.contiguous::<i32>() {
            return Lanes::I32(v);
        }
        // The domain is `i16`/`i32` operands, so every value fits.
        Lanes::Owned(values(op).into_iter().map(|v| v as i32).collect())
    }
}

/// Run `f` over the lanes' type — one monomorphised body per storage type.
macro_rules! with_lanes {
    ($lanes:expr, $x:ident => $body:expr) => {
        match $lanes {
            Lanes::I16($x) => $body,
            Lanes::I32($x) => $body,
            Lanes::Owned(v) => {
                let $x: &[i32] = &v;
                $body
            }
        }
    };
}

fn par_rows<T: Copy + Sync, O: Copy + Send>(x: &[T], n: usize, out: &mut [O], row: impl Fn(&[T], &mut [O]) + Sync) {
    if x.len() >= 1 << 17 {
        x.par_chunks(n).zip(out.par_chunks_mut(n)).for_each(|(x, o)| row(x, o));
    } else {
        for (x, o) in x.chunks(n).zip(out.chunks_mut(n)) {
            row(x, o);
        }
    }
}

/// The deliberately broken variant's one-lane move (`FusedIo::fault`), kept inside `[-hi, hi]` or the type.
fn fault_lane<O: Copy + PartialOrd + std::ops::Add<Output = O> + std::ops::Sub<Output = O> + From<i8>>(out: &mut [O], hi: O) {
    if let Some(v) = out.first_mut() {
        *v = if *v < hi { *v + O::from(1) } else { *v - O::from(1) };
    }
}

#[inline(always)]
fn sum_sq<T: Copy + Into<i64>>(x: &[T]) -> i64 {
    x.iter().fold(0i64, |a, v| {
        let v: i64 = (*v).into();
        a.wrapping_add(v.wrapping_mul(v))
    })
}

#[inline(always)]
fn max_abs<T: Copy + Into<i64>>(x: &[T]) -> i64 {
    x.iter().fold(0i64, |a, v| a.max(Into::<i64>::into(*v).abs()))
}

// ---- l2_unit_q15 --------------------------------------------------------------------------------

/// `l2_unit_q15`: `i16` codes in, `i16` Q15 codes out.
pub struct L2UnitQ15;

impl FusedKernelV1 for L2UnitQ15 {
    fn name(&self) -> &'static str {
        "l2_unit_q15"
    }

    fn variants(&self) -> Vec<Variant> {
        vec![Variant { bound: Bound::L2UnitQ15, probe: vec![TensorType::fixed(DType::I16, &[2, 4])], states: Vec::new() }]
    }

    fn derive(&self, _variant: &Bound, holes: &[TensorType], _output: &Node) -> Option<Bound> {
        let shape = static_shape(holes.first()?)?;
        (holes.len() == 1 && rows_of(&shape).is_some()).then_some(Bound::L2UnitQ15)
    }

    fn emit(&self, b: &mut BlockBuilder<'_>, holes: &[Ref], _bound: &Bound) -> Ref {
        b.l2_unit_q15(holes[0])
    }

    fn domain(&self, _bound: &Bound, holes: &[TensorType], out: &TensorType) -> bool {
        holes[0].dtype == DType::I16 && out.dtype == DType::I16
    }

    fn run(&self, _bound: &Bound, io: &mut FusedIo<'_, '_>) -> TirResult<()> {
        let n = *io.out_shape.last().unwrap_or(&1);
        if super::metal::metal_enabled() {
            let x = values(&io.holes[0]);
            if let Some(mut out) = super::metal::unit_rows(0, &x, n, 0) {
                store(&mut out, io.out, io.out_store, io.fault, 32767);
                return Ok(());
            }
        }
        let lanes = Lanes::of(&io.holes[0]);
        let mut out = vec![0i16; io.holes[0].numel()];
        with_lanes!(&lanes, x => par_rows(x, n, &mut out, |x, o| {
            // The exact sum is an `i64` (the plan proved it: `Fast64`); the scalar chain is the template's.
            let sum = sum_sq(x);
            let h = half_exponent(sum as i128, 20) as i64;
            let m = sum >> (2 * h).clamp(0, 40);
            let r = int_rsqrt_i64(m);
            let sh = (h + 21).clamp(0, 41);
            for (o, v) in o.iter_mut().zip(x) {
                let v: i64 = (*v).into();
                *o = ((v * r) >> sh).clamp(-32767, 32767) as i16;
            }
        }));
        if io.fault {
            fault_lane(&mut out, 32767);
        }
        *io.out = Buf::I16(out);
        Ok(())
    }
}

// ---- rms_unit_q24 -------------------------------------------------------------------------------

/// `rms_unit_q24`: `i16`/`i32` codes and one `i64` `ε` in, `i32` Q24 out.
pub struct RmsUnitQ24;

impl FusedKernelV1 for RmsUnitQ24 {
    fn name(&self) -> &'static str {
        "rms_unit_q24"
    }

    fn variants(&self) -> Vec<Variant> {
        vec![Variant {
            bound: Bound::RmsUnitQ24,
            probe: vec![TensorType::fixed(DType::I32, &[2, 4]), TensorType::fixed(DType::I64, &[1])],
            states: Vec::new(),
        }]
    }

    fn derive(&self, _variant: &Bound, holes: &[TensorType], _output: &Node) -> Option<Bound> {
        let shape = static_shape(holes.first()?)?;
        let eps = static_shape(holes.get(1)?)?;
        (holes.len() == 2 && rows_of(&shape).is_some() && eps.iter().product::<usize>() == 1).then_some(Bound::RmsUnitQ24)
    }

    fn emit(&self, b: &mut BlockBuilder<'_>, holes: &[Ref], _bound: &Bound) -> Ref {
        b.rms_unit_q24(holes[0], holes[1])
    }

    fn domain(&self, _bound: &Bound, holes: &[TensorType], out: &TensorType) -> bool {
        matches!(holes[0].dtype, DType::I16 | DType::I32) && holes[1].dtype == DType::I64 && out.dtype == DType::I32
    }

    fn run(&self, _bound: &Bound, io: &mut FusedIo<'_, '_>) -> TirResult<()> {
        let eps = values(&io.holes[1]).first().copied().unwrap_or(0).clamp(0, i64::MAX as i128);
        let n = *io.out_shape.last().unwrap_or(&1);
        if super::metal::metal_enabled() {
            let x = values(&io.holes[0]);
            if let Some(mut out) = super::metal::unit_rows(1, &x, n, eps) {
                store(&mut out, io.out, io.out_store, io.fault, i32::MAX as i128);
                return Ok(());
            }
        }
        let lanes = Lanes::of(&io.holes[0]);
        let mut out = vec![0i32; io.holes[0].numel()];
        with_lanes!(&lanes, x => par_rows(x, n, &mut out, |x, o| {
            let sum = sum_sq(x) as i128;
            let mean = (sum * arith::ONE).div_euclid(n as i128) + eps;
            let h = half_exponent(mean, 51);
            let m = (mean >> (2 * h).clamp(0, 102)).clamp(0, i64::MAX as i128);
            let r = int_rsqrt_i64(m as i64);
            let p = h.clamp(0, 51) as u32;
            // One machine word where the product provably fits it, two otherwise (the same integers).
            if (max_abs(x) as i128) * (r as i128) < 1 << 62 {
                for (o, v) in o.iter_mut().zip(x) {
                    let v: i64 = (*v).into();
                    *o = ((v * r) >> p).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                }
            } else {
                for (o, v) in o.iter_mut().zip(x) {
                    let v: i64 = (*v).into();
                    *o = ((v as i128 * r as i128) >> p).clamp(i32::MIN as i128, i32::MAX as i128) as i32;
                }
            }
        }));
        if io.fault {
            fault_lane(&mut out, i32::MAX);
        }
        *io.out = Buf::I32(out);
        Ok(())
    }
}

// ---- rms_norm_wide_q36 --------------------------------------------------------------------------

/// `rms_norm_wide_q36` / `rms_norm_wide_q36_exact` (the 39-node form with `ε = eps_zero · 2^eps_shift`): `i16`/`i32` codes, the two
/// `ε` scalars in, `i32` Q24 out. The two forms differ only in the caps of the mantissa and of the shift (`2^30` and 96, or `i64::MAX`
/// and 62), so they are two variants of one kernel and the exact re-emission at the program's own operands tells which one a region is.
pub struct RmsNormWideQ36;

fn wide_caps(exact: bool) -> (i128, i128) {
    if exact { (i64::MAX as i128, 62) } else { (1 << 30, 96) }
}

impl FusedKernelV1 for RmsNormWideQ36 {
    fn name(&self) -> &'static str {
        "rms_norm_wide_q36"
    }

    fn variants(&self) -> Vec<Variant> {
        let probe =
            || vec![TensorType::fixed(DType::I32, &[2, 4]), TensorType::fixed(DType::I64, &[1]), TensorType::fixed(DType::I32, &[1])];
        [false, true].map(|exact| Variant { bound: Bound::RmsNormWideQ36 { exact }, probe: probe(), states: Vec::new() }).into()
    }

    fn derive(&self, variant: &Bound, holes: &[TensorType], _output: &Node) -> Option<Bound> {
        let shape = static_shape(holes.first()?)?;
        let scalar = |i: usize| static_shape(holes.get(i)?).map(|s| s.iter().product::<usize>() == 1);
        (holes.len() == 3 && rows_of(&shape).is_some() && scalar(1)? && scalar(2)?).then(|| variant.clone())
    }

    fn emit(&self, b: &mut BlockBuilder<'_>, holes: &[Ref], bound: &Bound) -> Ref {
        match bound {
            Bound::RmsNormWideQ36 { exact: true } => b.rms_norm_wide_q36_exact(holes[0], holes[1], holes[2]),
            _ => b.rms_norm_wide_q36(holes[0], holes[1], holes[2]),
        }
    }

    fn domain(&self, _bound: &Bound, holes: &[TensorType], out: &TensorType) -> bool {
        matches!(holes[0].dtype, DType::I16 | DType::I32) && out.dtype == DType::I32
    }

    fn run(&self, bound: &Bound, io: &mut FusedIo<'_, '_>) -> TirResult<()> {
        let exact = matches!(bound, Bound::RmsNormWideQ36 { exact: true });
        let (zero_max, shift_max) = wide_caps(exact);
        let ez = values(&io.holes[1]).first().copied().unwrap_or(0).clamp(0, zero_max);
        let es = values(&io.holes[2]).first().copied().unwrap_or(0).clamp(0, shift_max);
        let eps = ez << es;
        let n = *io.out_shape.last().unwrap_or(&1);
        if super::metal::metal_enabled() {
            let x = values(&io.holes[0]);
            if let Some(mut out) = super::metal::rms_wide_rows(&x, n, eps) {
                store(&mut out, io.out, io.out_store, io.fault, i32::MAX as i128);
                return Ok(());
            }
        }
        let lanes = Lanes::of(&io.holes[0]);
        let mut out = vec![0i32; io.holes[0].numel()];
        with_lanes!(&lanes, x => par_rows(x, n, &mut out, |x, o| {
            let sum = sum_sq(x) as i128;
            let mean = (sum * arith::ONE).div_euclid(n as i128) + eps;
            if mean <= 0 {
                return; // a zero mean is a zero row
            }
            let h = (arith::log2_floor(mean) - K).div_euclid(2);
            let two_h = 2 * h;
            let m = if two_h >= 0 { mean >> two_h.clamp(0, 126) } else { mean.clamp(0, arith::ONE) << (-two_h).clamp(0, 24) };
            let r = int_rsqrt_i64(m.clamp(0, i64::MAX as i128) as i64);
            if (max_abs(x) as i128) * (r as i128) < 1 << 48 {
                for (o, v) in o.iter_mut().zip(x) {
                    let prod: i64 = Into::<i64>::into(*v) * r;
                    let y = if h >= 0 { prod >> h.clamp(0, 62) as u32 } else { prod << (-h).clamp(0, 12) as u32 };
                    *o = y.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                }
            } else {
                for (o, v) in o.iter_mut().zip(x) {
                    let prod = Into::<i64>::into(*v) as i128 * r as i128;
                    let y = if h >= 0 { prod >> h.clamp(0, 126) } else { prod << (-h).clamp(0, 12) };
                    *o = y.clamp(i32::MIN as i128, i32::MAX as i128) as i32;
                }
            }
        }));
        if io.fault {
            fault_lane(&mut out, i32::MAX);
        }
        *io.out = Buf::I32(out);
        Ok(())
    }
}
