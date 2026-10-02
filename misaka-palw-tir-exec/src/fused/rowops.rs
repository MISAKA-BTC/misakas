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

fn run_rows(vals: &[i128], n: usize, out: &mut [i128], row: impl Fn(&[i128], &mut [i128]) + Sync) {
    if vals.len() >= 1 << 14 {
        vals.par_chunks(n).zip(out.par_chunks_mut(n)).for_each(|(x, o)| row(x, o));
    } else {
        for (x, o) in vals.chunks(n).zip(out.chunks_mut(n)) {
            row(x, o);
        }
    }
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
        let x = values(&io.holes[0]);
        let n = *io.out_shape.last().unwrap_or(&1);
        let mut out = super::metal::unit_rows(0, &x, n, 0).unwrap_or_default();
        if out.is_empty() {
            out = vec![0i128; x.len()];
            run_rows(&x, n, &mut out, |x, o| {
                let sum: i128 = x.iter().map(|v| v * v).sum();
                let h = half_exponent(sum, 20);
                let m = sum >> (2 * h).clamp(0, 40);
                let r = int_rsqrt_i64(m as i64) as i128;
                let sh = (h + 21).clamp(0, 41);
                for (o, v) in o.iter_mut().zip(x) {
                    *o = ((v * r) >> sh).clamp(-32767, 32767);
                }
            });
        }
        store(&mut out, io.out, io.out_store, io.fault, 32767);
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
        let x = values(&io.holes[0]);
        let eps = values(&io.holes[1]).first().copied().unwrap_or(0).clamp(0, i64::MAX as i128);
        let n = *io.out_shape.last().unwrap_or(&1);
        let mut out = super::metal::unit_rows(1, &x, n, eps).unwrap_or_default();
        if out.is_empty() {
            out = vec![0i128; x.len()];
            run_rows(&x, n, &mut out, |x, o| {
                let sum: i128 = x.iter().map(|v| v * v).sum();
                let mean = ((sum * arith::ONE).div_euclid(n as i128)) + eps;
                let h = half_exponent(mean, 51);
                let m = (mean >> (2 * h).clamp(0, 102)).clamp(0, i64::MAX as i128);
                let r = int_rsqrt_i64(m as i64) as i128;
                let p = h.clamp(0, 51);
                for (o, v) in o.iter_mut().zip(x) {
                    *o = ((v * r) >> p).clamp(i32::MIN as i128, i32::MAX as i128);
                }
            });
        }
        store(&mut out, io.out, io.out_store, io.fault, i32::MAX as i128);
        Ok(())
    }
}
