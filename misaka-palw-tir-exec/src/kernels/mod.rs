//! The primitive kernels. Each computes exactly the values spec 04b §6 defines and fails exactly
//! where the reference fails; the plan ([`crate::plan`]) tells each one which checks can fire.
//!
//! Elementwise primitives compute in a *working type* `W` (`i64` or `i128`, [`Wide`]): operands
//! are widened (or borrowed, when already contiguous in `W`), the operation runs in `W`, and the
//! result is narrowed to the declared dtype — with the dtype check only where the plan could not
//! prove it away.

pub mod elementwise;
pub mod matmul;
pub mod misc;
pub mod reduce;

use misaka_palw_tir::{DType, Rounding, TirError, TirErrorKind, TirResult};

use crate::elem::{Buf, Elem, Slice};
use crate::layout::{Layout, gather_strided};
use crate::scalar;
use crate::with_slice;

/// An operand: typed storage plus the layout that names its elements.
#[derive(Clone, Copy, Debug)]
pub struct Opd<'a> {
    pub data: Slice<'a>,
    pub layout: Layout,
}

impl<'a> Opd<'a> {
    pub fn shape(&self) -> &[usize] {
        self.layout.shape()
    }
    pub fn dtype(&self) -> DType {
        self.data.dtype()
    }
    pub fn numel(&self) -> usize {
        self.layout.numel()
    }
    /// The elements as a contiguous typed slice, when the layout is contiguous.
    pub fn contiguous<T: Elem>(&self) -> Option<&'a [T]> {
        let s = T::slice_of(self.data)?;
        if self.layout.is_contiguous() { Some(&s[self.layout.offset..self.layout.offset + self.numel()]) } else { None }
    }
    /// Element at a row-major linear index (slow; small reads only).
    pub fn get_linear(&self, mut lin: usize) -> i128 {
        let sh = self.layout.shape();
        let mut off = self.layout.offset;
        for d in (0..sh.len()).rev() {
            off += (lin % sh[d]) * self.layout.strides[d];
            lin /= sh[d];
        }
        self.data.get(off)
    }
}

pub(crate) fn overflow<T>(what: &str) -> TirResult<T> {
    Err(TirError::new(TirErrorKind::Overflow, what.to_string()))
}

/// Elementwise kernels at or above this many outputs split across the pool (every output element
/// is its own exact function, so the split cannot change a value). Only the vocabulary-wide rows
/// qualify: below this a split costs more in wake-ups than it saves, and on a loaded host (a node
/// shares its cores) a preempted worker stalls the whole node. `TIR_EXEC_PAR_ELEMS` overrides it
/// (measurement only; it cannot change a value).
pub static PAR_ELEMS: std::sync::LazyLock<usize> =
    std::sync::LazyLock::new(|| std::env::var("TIR_EXEC_PAR_ELEMS").ok().and_then(|v| v.parse().ok()).unwrap_or(1 << 16));

/// The chunk of a parallel elementwise split: a few per thread, never tiny.
pub fn par_chunk(n: usize) -> usize {
    n.div_ceil(rayon::current_num_threads().max(1) * 2).max(2048)
}

/// A working type: the integer width an elementwise node computes in.
pub trait Wide: Elem + std::ops::Add<Output = Self> {
    const WMIN: Self;
    const WMAX: Self;
    fn widen<S: Elem>(s: S) -> Self;
    fn wadd(self, o: Self) -> Self;
    fn wsub(self, o: Self) -> Self;
    fn wmul(self, o: Self) -> Self;
    fn cadd(self, o: Self) -> Option<Self>;
    fn csub(self, o: Self) -> Option<Self>;
    fn cmul(self, o: Self) -> Option<Self>;
    /// `self` clamped into an `i128` interval.
    fn clamp_i128(self, lo: i128, hi: i128) -> Self;
    /// `round_rule(x / d)` for `d ≥ 1` (checked by the caller).
    fn div_rule(x: Self, d: Self, rule: Rounding) -> Self;
    /// `round_rule(x / 2^s)`.
    fn shr_rule(x: Self, s: u32, rule: Rounding) -> Self;
    /// `Some(s)` when `d = 2^s`.
    fn pow2_shift(d: Self) -> Option<u32>;
    fn log2_floor(self) -> Self;
    fn int_exp(self) -> Self;
    fn int_rsqrt(self) -> Self;
    fn int_ln(self) -> Self;
    /// The dtype bounds, clamped into this width.
    fn bounds(d: DType) -> (Self, Self) {
        let lo = d.min_value().max(Self::WMIN.to_i128());
        let hi = d.max_value().min(Self::WMAX.to_i128());
        (Self::from_i128(lo), Self::from_i128(hi))
    }
}

impl Wide for i64 {
    const WMIN: Self = i64::MIN;
    const WMAX: Self = i64::MAX;
    #[inline(always)]
    fn widen<S: Elem>(s: S) -> Self {
        s.to_i64()
    }
    #[inline(always)]
    fn wadd(self, o: Self) -> Self {
        self.wrapping_add(o)
    }
    #[inline(always)]
    fn wsub(self, o: Self) -> Self {
        self.wrapping_sub(o)
    }
    #[inline(always)]
    fn wmul(self, o: Self) -> Self {
        self.wrapping_mul(o)
    }
    #[inline(always)]
    fn cadd(self, o: Self) -> Option<Self> {
        self.checked_add(o)
    }
    #[inline(always)]
    fn csub(self, o: Self) -> Option<Self> {
        self.checked_sub(o)
    }
    #[inline(always)]
    fn cmul(self, o: Self) -> Option<Self> {
        self.checked_mul(o)
    }
    #[inline(always)]
    fn clamp_i128(self, lo: i128, hi: i128) -> Self {
        let lo = lo.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        let hi = hi.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        self.clamp(lo, hi)
    }
    #[inline(always)]
    fn div_rule(x: i64, d: i64, rule: Rounding) -> i64 {
        scalar::div_round_i64(x, d, rule)
    }
    #[inline(always)]
    fn shr_rule(x: i64, s: u32, rule: Rounding) -> i64 {
        scalar::shr_round_i64(x, s, rule)
    }
    #[inline(always)]
    fn pow2_shift(d: i64) -> Option<u32> {
        (d >= 1 && d & (d - 1) == 0).then(|| d.trailing_zeros())
    }
    #[inline(always)]
    fn log2_floor(self) -> i64 {
        scalar::log2_floor_i64(self)
    }
    #[inline(always)]
    fn int_exp(self) -> i64 {
        scalar::int_exp_i64(self)
    }
    #[inline(always)]
    fn int_rsqrt(self) -> i64 {
        scalar::int_rsqrt_i64(self)
    }
    #[inline(always)]
    fn int_ln(self) -> i64 {
        scalar::int_ln_i64(self)
    }
}

impl Wide for i128 {
    const WMIN: Self = i128::MIN;
    const WMAX: Self = i128::MAX;
    #[inline(always)]
    fn widen<S: Elem>(s: S) -> Self {
        s.to_i128()
    }
    #[inline(always)]
    fn wadd(self, o: Self) -> Self {
        self.wrapping_add(o)
    }
    #[inline(always)]
    fn wsub(self, o: Self) -> Self {
        self.wrapping_sub(o)
    }
    #[inline(always)]
    fn wmul(self, o: Self) -> Self {
        self.wrapping_mul(o)
    }
    #[inline(always)]
    fn cadd(self, o: Self) -> Option<Self> {
        self.checked_add(o)
    }
    #[inline(always)]
    fn csub(self, o: Self) -> Option<Self> {
        self.checked_sub(o)
    }
    #[inline(always)]
    fn cmul(self, o: Self) -> Option<Self> {
        self.checked_mul(o)
    }
    #[inline(always)]
    fn clamp_i128(self, lo: i128, hi: i128) -> Self {
        self.clamp(lo, hi)
    }
    #[inline(always)]
    fn div_rule(x: i128, d: i128, rule: Rounding) -> i128 {
        scalar::div_round_i128(x, d, rule)
    }
    #[inline(always)]
    fn shr_rule(x: i128, s: u32, rule: Rounding) -> i128 {
        scalar::shr_round_i128(x, s, rule)
    }
    #[inline(always)]
    fn pow2_shift(d: i128) -> Option<u32> {
        (d >= 1 && d & (d - 1) == 0).then(|| d.trailing_zeros())
    }
    // The transcendentals never see an i128 operand (type rule); these exist for totality.
    #[inline(always)]
    fn log2_floor(self) -> i128 {
        misaka_palw_tir::arith::log2_floor(self)
    }
    #[inline(always)]
    fn int_exp(self) -> i128 {
        misaka_palw_tir::arith::int_exp(self)
    }
    #[inline(always)]
    fn int_rsqrt(self) -> i128 {
        misaka_palw_tir::arith::int_rsqrt(self)
    }
    #[inline(always)]
    fn int_ln(self) -> i128 {
        misaka_palw_tir::arith::int_ln(self)
    }
}

/// Reusable working buffers of one executor.
#[derive(Default)]
pub struct Scratch {
    pub w64: [Vec<i64>; 3],
    pub w128: [Vec<i128>; 3],
    pub r64: Vec<i64>,
    pub r128: Vec<i128>,
}

/// Scratch for one working type.
pub trait WideScratch<W> {
    fn parts(&mut self) -> (&mut [Vec<W>; 3], &mut Vec<W>);
}
impl WideScratch<i64> for Scratch {
    fn parts(&mut self) -> (&mut [Vec<i64>; 3], &mut Vec<i64>) {
        (&mut self.w64, &mut self.r64)
    }
}
impl WideScratch<i128> for Scratch {
    fn parts(&mut self) -> (&mut [Vec<i128>; 3], &mut Vec<i128>) {
        (&mut self.w128, &mut self.r128)
    }
}

/// The operand's elements in `W`, contiguous in its own shape: borrowed when it already is,
/// otherwise gathered (and converted) into `scratch`.
pub fn widen<'s, W: Wide>(op: &Opd<'s>, scratch: &'s mut Vec<W>) -> &'s [W] {
    if let Some(v) = op.contiguous::<W>() {
        return v;
    }
    with_slice!(op.data, v => gather_strided(v, &op.layout, scratch, |x| W::widen(x)));
    scratch
}

/// Move `res` (values of `W`) into `out`, stored as `store`. With `check = Some(dtype)`, every
/// value must lie in the declared `dtype`, else the step fails (PALW-TIR-23) — the plan proved the
/// check away otherwise, and proved that every value fits `store`.
pub fn narrow<W: Wide>(res: &mut Vec<W>, out: &mut Buf, store: DType, check: Option<DType>) -> TirResult<()> {
    if let Some(dtype) = check {
        // Bounds clamped into W: a dtype wider than W cannot be left by a value of W.
        let (lo, hi) = W::bounds(dtype);
        let mut bad = false;
        for v in res.iter() {
            bad |= *v < lo || *v > hi;
        }
        if bad {
            return overflow(&format!("a result outside {}", dtype.name()));
        }
    }
    if W::DTYPE == store {
        if let Some(o) = W::vec_of(out) {
            std::mem::swap(o, res);
            return Ok(());
        }
        *out = W::into_buf(std::mem::take(res));
        return Ok(());
    }
    crate::with_dtype!(store, T => {
        use rayon::prelude::*;
        let o = out_vec::<T>(out);
        o.clear();
        o.resize(res.len(), T::default());
        if res.len() >= *PAR_ELEMS {
            let chunk = par_chunk(res.len());
            o.par_chunks_mut(chunk).zip(res.par_chunks(chunk)).for_each(|(o, r)| {
                for (d, v) in o.iter_mut().zip(r) {
                    *d = T::from_i128(v.to_i128());
                }
            });
        } else {
            for (d, v) in o.iter_mut().zip(res.iter()) {
                *d = T::from_i128(v.to_i128());
            }
        }
    });
    Ok(())
}

/// Copy an operand into `out` as a contiguous buffer of its own dtype.
pub fn materialize(op: &Opd<'_>, out: &mut Buf) {
    with_slice!(op.data, v => materialize_typed(v, &op.layout, out));
}

fn materialize_typed<T: Elem>(src: &[T], layout: &Layout, out: &mut Buf) {
    if T::vec_of(out).is_none() {
        *out = T::into_buf(Vec::new());
    }
    let dst = T::vec_of(out).expect("typed above");
    gather_strided(src, layout, dst, |x| x);
}

/// The typed output vector of `out`, retyped to `T` if needed (contents unspecified).
pub fn out_vec<T: Elem>(out: &mut Buf) -> &mut Vec<T> {
    if T::vec_of(out).is_none() {
        *out = T::into_buf(Vec::new());
    }
    T::vec_of(out).expect("typed above")
}
