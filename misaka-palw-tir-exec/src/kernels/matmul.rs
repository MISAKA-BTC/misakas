//! `MatMul` (spec 04b §6.3): `out[β, r, c] = Σ_t a[β_a, r, t] · b[β_b, t, c]`, exact.
//!
//! **Why this may reorder and parallelise.** The value of an exact sum does not depend on the
//! order of its terms, and success depends only on the sum of the positive terms and the sum of
//! the negative terms (the order-free rule, PALW-TIR-24) — so an implementation may reassociate,
//! vectorise and thread it freely provided its own accumulator never wraps. The plan's `Acc::Fast64`
//! is exactly that proof: every partial sum in every order lies in `i64` AND in the declared dtype,
//! so any association in wrapping `i64` arithmetic computes the exact value and no check can fire.
//! A NEON lane that holds a partial sum in `i32` is bounded separately (see [`dot_i8_i16`]).
//! Without the proof the reference's own checked accumulation runs (`Acc::Pn64`/`Acc::Pn128`).
//!
//! Operands are strided views: a weight matrix consumed through `Transpose`, a KV window read as
//! `[kv, d, H]`, a broadcast batch — none is copied. Three loop forms cover them:
//! * **dot** — the contraction is contiguous in both operands (a projection `W[N,K]·x[K,1]`, the
//!   attention scores `q[kv,g,d]·Kᵀ[kv,d,H]`): one dot product per output element;
//! * **axpy** — `b`'s rows are contiguous (`P[kv,g,H]·V[kv,H,d]`, an expert combine): each output
//!   row accumulates scaled rows of `b`;
//! * **general** — any other strides, element by element.

use rayon::prelude::*;

use misaka_palw_tir::{TirError, TirErrorKind, TirResult};

use super::{Opd, Scratch, narrow, overflow};
use crate::elem::{Buf, Elem, Slice};
use crate::layout::row_major;
use crate::plan::{Acc, NodePlan};

/// Below this many multiply-accumulates a call runs on the calling thread: the pool costs more
/// to wake than the work.
const PAR_MIN_WORK: usize = 1 << 16;

/// The index geometry of one call.
pub struct Geo {
    pub m: usize,
    pub k: usize,
    pub n: usize,
    pub a_m: usize,
    pub a_k: usize,
    pub b_k: usize,
    pub b_n: usize,
    /// Per output batch index: the storage offsets of `a`'s and `b`'s `[M, K]`/`[K, N]` matrices.
    pub batch: Vec<(usize, usize)>,
}

impl Geo {
    pub fn new(a: &Opd<'_>, b: &Opd<'_>, out_shape: &[usize]) -> Self {
        let (ash, bsh) = (a.shape(), b.shape());
        let (ra, rb, ro) = (ash.len(), bsh.len(), out_shape.len());
        let (ast, bst) = (a.layout.strides(), b.layout.strides());
        let batch_shape = &out_shape[..ro - 2];
        let nb: usize = batch_shape.iter().product();
        let rm = row_major(batch_shape);
        let mut batch = Vec::with_capacity(nb);
        for bi in 0..nb {
            let (mut ao, mut bo) = (a.layout.offset, b.layout.offset);
            for (d, st) in rm[..batch_shape.len()].iter().enumerate() {
                let idx = (bi / st) % batch_shape[d];
                // Batch dimensions align on the right; a missing or unit one broadcasts.
                if let Some(k) = (d + (ra - 2)).checked_sub(ro - 2)
                    && ash[k] != 1
                {
                    ao += idx * ast[k];
                }
                if let Some(k) = (d + (rb - 2)).checked_sub(ro - 2)
                    && bsh[k] != 1
                {
                    bo += idx * bst[k];
                }
            }
            batch.push((ao, bo));
        }
        Geo {
            m: ash[ra - 2],
            k: ash[ra - 1],
            n: bsh[rb - 1],
            a_m: ast[ra - 2],
            a_k: ast[ra - 1],
            b_k: bst[rb - 2],
            b_n: bst[rb - 1],
            batch,
        }
    }

    fn outputs(&self) -> usize {
        self.batch.len() * self.m * self.n
    }

    /// `(batch, row, col)` of a row-major output index.
    #[inline(always)]
    fn unravel(&self, lin: usize) -> (usize, usize, usize) {
        let c = lin % self.n;
        let rest = lin / self.n;
        (rest / self.m, rest % self.m, c)
    }
}

fn chunk_len(total: usize) -> usize {
    let target = rayon::current_num_threads().max(1) * 4;
    total.div_ceil(target).max(1)
}

/// Run `body(first_index, chunk)` over `res`, on the pool when the work is worth it.
fn for_chunks<T: Send>(res: &mut [T], work_per_output: usize, body: impl Fn(usize, &mut [T]) + Sync) {
    let total = res.len();
    if total >= 2 && total.saturating_mul(work_per_output.max(1)) >= PAR_MIN_WORK {
        let chunk = chunk_len(total);
        res.par_chunks_mut(chunk).enumerate().for_each(|(ci, ch)| body(ci * chunk, ch));
    } else {
        body(0, res);
    }
}

pub fn matmul(node: &NodePlan, a: &Opd<'_>, b: &Opd<'_>, out_shape: &[usize], out: &mut Buf, scratch: &mut Scratch) -> TirResult<()> {
    let g = Geo::new(a, b, out_shape);
    let dt = node.out.dtype;
    match node.acc {
        Acc::Fast64 => {
            let res = &mut scratch.r64;
            res.clear();
            res.resize(g.outputs(), 0);
            fast64(a.data, b.data, &g, res)?;
            narrow(res, out, dt, false)
        }
        Acc::Fast128 => {
            let res = &mut scratch.r128;
            res.clear();
            res.resize(g.outputs(), 0);
            let (a, b) = (a.data, b.data);
            for (lin, r) in res.iter_mut().enumerate() {
                let (bi, row, c) = g.unravel(lin);
                let (ao, bo) = g.batch[bi];
                let mut s = 0i128;
                for t in 0..g.k {
                    let x = a.get(ao + row * g.a_m + t * g.a_k);
                    let y = b.get(bo + t * g.b_k + c * g.b_n);
                    s = s.wrapping_add(x.wrapping_mul(y));
                }
                *r = s;
            }
            narrow(res, out, dt, false)
        }
        Acc::Pn64 | Acc::Pn128 => {
            let res = &mut scratch.r128;
            res.clear();
            res.resize(g.outputs(), 0);
            let (lo, hi) = (dt.min_value(), dt.max_value());
            let (a, b) = (a.data, b.data);
            for (lin, r) in res.iter_mut().enumerate() {
                let (bi, row, c) = g.unravel(lin);
                let (ao, bo) = g.batch[bi];
                // The reference's order-free check: the positive and the negative terms apart,
                // each bounded by the dtype (every partial sum in every order fits iff these do).
                let (mut pos, mut neg) = (0i128, 0i128);
                for t in 0..g.k {
                    let x = a.get(ao + row * g.a_m + t * g.a_k);
                    let y = b.get(bo + t * g.b_k + c * g.b_n);
                    // Operands are at most i64 (type rule): the product is an exact i128.
                    let term = x * y;
                    if term > 0 {
                        pos = pos.checked_add(term).filter(|p| *p <= hi).ok_or_else(|| over("MatMul"))?;
                    } else {
                        neg = neg.checked_add(term).filter(|n| *n >= lo).ok_or_else(|| over("MatMul"))?;
                    }
                }
                *r = pos + neg;
            }
            narrow(res, out, dt, false)
        }
    }
}

fn over(what: &str) -> TirError {
    TirError::new(TirErrorKind::Overflow, format!("{what}: a partial sum leaves the declared type"))
}

/// Both operands' dtypes, for the typed kernels (MatMul operands are i8…i64 by the type rule).
macro_rules! mm_dispatch {
    ($a:expr, $b:expr, |$x:ident, $y:ident| $body:expr) => {
        match ($a, $b) {
            (Slice::I8($x), Slice::I8($y)) => $body,
            (Slice::I8($x), Slice::I16($y)) => $body,
            (Slice::I8($x), Slice::I32($y)) => $body,
            (Slice::I8($x), Slice::I64($y)) => $body,
            (Slice::I16($x), Slice::I8($y)) => $body,
            (Slice::I16($x), Slice::I16($y)) => $body,
            (Slice::I16($x), Slice::I32($y)) => $body,
            (Slice::I16($x), Slice::I64($y)) => $body,
            (Slice::I32($x), Slice::I8($y)) => $body,
            (Slice::I32($x), Slice::I16($y)) => $body,
            (Slice::I32($x), Slice::I32($y)) => $body,
            (Slice::I32($x), Slice::I64($y)) => $body,
            (Slice::I64($x), Slice::I8($y)) => $body,
            (Slice::I64($x), Slice::I16($y)) => $body,
            (Slice::I64($x), Slice::I32($y)) => $body,
            (Slice::I64($x), Slice::I64($y)) => $body,
            _ => return Err(TirError::new(TirErrorKind::Shape, "MatMul operands are i8..i64")),
        }
    };
}

fn fast64(a: Slice<'_>, b: Slice<'_>, g: &Geo, res: &mut [i64]) -> TirResult<()> {
    mm_dispatch!(a, b, |x, y| fast64_typed(x, y, g, res));
    Ok(())
}

fn fast64_typed<A: Dot<B>, B: Elem>(a: &[A], b: &[B], g: &Geo, res: &mut [i64]) {
    let contiguous_k = g.k == 1 || (g.a_k == 1 && g.b_k == 1);
    if contiguous_k {
        // One dot product per output.
        for_chunks(res, g.k, |first, chunk| {
            for (i, r) in chunk.iter_mut().enumerate() {
                let (bi, row, c) = g.unravel(first + i);
                let (ao, bo) = g.batch[bi];
                let ar = ao + row * g.a_m;
                let bc = bo + c * g.b_n;
                *r = A::dot(&a[ar..ar + g.k], &b[bc..bc + g.k]);
            }
        });
    } else if g.b_n == 1 || g.n == 1 {
        // Rows of b are contiguous: accumulate scaled rows (restricted to the chunk's columns).
        for_chunks(res, g.k, |first, chunk| {
            let mut i = 0;
            while i < chunk.len() {
                let (bi, row, c0) = g.unravel(first + i);
                let seg = (g.n - c0).min(chunk.len() - i);
                let (ao, bo) = g.batch[bi];
                let acc = &mut chunk[i..i + seg];
                acc.iter_mut().for_each(|v| *v = 0);
                for t in 0..g.k {
                    let coef = a[ao + row * g.a_m + t * g.a_k].to_i64();
                    let br = bo + t * g.b_k + c0;
                    for (v, y) in acc.iter_mut().zip(&b[br..br + seg]) {
                        *v = v.wrapping_add(coef.wrapping_mul(y.to_i64()));
                    }
                }
                i += seg;
            }
        });
    } else {
        for_chunks(res, g.k, |first, chunk| {
            for (i, r) in chunk.iter_mut().enumerate() {
                let (bi, row, c) = g.unravel(first + i);
                let (ao, bo) = g.batch[bi];
                let mut s = 0i64;
                for t in 0..g.k {
                    let x = a[ao + row * g.a_m + t * g.a_k].to_i64();
                    let y = b[bo + t * g.b_k + c * g.b_n].to_i64();
                    s = s.wrapping_add(x.wrapping_mul(y));
                }
                *r = s;
            }
        });
    }
}

/// An exact dot product whose every partial sum the caller proved to fit `i64`.
pub trait Dot<B: Elem>: Elem {
    fn dot(a: &[Self], b: &[B]) -> i64;
}

/// The portable form: products and sums in wrapping `i64`, exact under the caller's proof, in
/// whatever association the vectoriser picks.
#[inline(always)]
pub fn dot_scalar<A: Elem, B: Elem>(a: &[A], b: &[B]) -> i64 {
    a.iter().zip(b).fold(0i64, |s, (x, y)| s.wrapping_add(x.to_i64().wrapping_mul(y.to_i64())))
}

macro_rules! scalar_dots {
    ($($a:ty, $b:ty);* $(;)?) => {
        $(impl Dot<$b> for $a {
            #[inline]
            fn dot(a: &[$a], b: &[$b]) -> i64 {
                dot_scalar(a, b)
            }
        })*
    };
}

scalar_dots!(
    i8, i8; i8, i32; i8, i64;
    i16, i32; i16, i64;
    i32, i8; i32, i16; i32, i32; i32, i64;
    i64, i8; i64, i16; i64, i32; i64, i64;
);

impl Dot<i16> for i8 {
    #[inline]
    fn dot(a: &[i8], b: &[i16]) -> i64 {
        dot_i8_i16(a, b)
    }
}
impl Dot<i8> for i16 {
    #[inline]
    fn dot(a: &[i16], b: &[i8]) -> i64 {
        dot_i8_i16(b, a)
    }
}
impl Dot<i16> for i16 {
    #[inline]
    fn dot(a: &[i16], b: &[i16]) -> i64 {
        dot_i16_i16(a, b)
    }
}

/// Terms per `i32` lane chunk of [`dot_i8_i16`]: each lane takes `CHUNK / 8` products of at most
/// `128 · 32768 = 2^22`, i.e. at most `2^28` — an eight-fold margin below `i32::MAX`. Chunk totals
/// are widened to `i64` before they meet.
const CHUNK_I8_I16: usize = 512;
const _: () = assert!((CHUNK_I8_I16 as i64 / 8) * 128 * 32768 < i32::MAX as i64);

/// `Σ w·x` for `i8` weights and `i16` activations (the projection of every A16 lowering).
#[inline]
pub fn dot_i8_i16(w: &[i8], x: &[i16]) -> i64 {
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is part of the aarch64 baseline; every load is bounded by the chunk loop.
        unsafe { dot_i8_i16_neon(w, x) }
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        dot_scalar(w, x)
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn dot_i8_i16_neon(w: &[i8], x: &[i16]) -> i64 {
    use std::arch::aarch64::*;
    let n = w.len().min(x.len());
    let mut total: i64 = 0;
    let mut base = 0usize;
    while base < n {
        let end = (base + CHUNK_I8_I16).min(n);
        // SAFETY: every access below is at `i..i + 16 <= end <= n` in both slices.
        unsafe {
            let mut acc0 = vdupq_n_s32(0);
            let mut acc1 = vdupq_n_s32(0);
            let mut i = base;
            while i + 16 <= end {
                let wv = vld1q_s8(w.as_ptr().add(i));
                let w_lo = vmovl_s8(vget_low_s8(wv));
                let w_hi = vmovl_high_s8(wv);
                let x0 = vld1q_s16(x.as_ptr().add(i));
                let x1 = vld1q_s16(x.as_ptr().add(i + 8));
                acc0 = vmlal_s16(acc0, vget_low_s16(w_lo), vget_low_s16(x0));
                acc1 = vmlal_high_s16(acc1, w_lo, x0);
                acc0 = vmlal_s16(acc0, vget_low_s16(w_hi), vget_low_s16(x1));
                acc1 = vmlal_high_s16(acc1, w_hi, x1);
                i += 16;
            }
            let widened = vaddq_s64(vpaddlq_s32(acc0), vpaddlq_s32(acc1));
            total = total.wrapping_add(vgetq_lane_s64(widened, 0)).wrapping_add(vgetq_lane_s64(widened, 1));
            total = total.wrapping_add(dot_scalar(&w[i..end], &x[i..end]));
        }
        base = end;
    }
    total
}

/// `Σ a·b` for two `i16` rows (attention scores). A product is at most `2^30`, so it is formed
/// in `i32` and accumulated pairwise into `i64` lanes — no lane can wrap.
#[inline]
pub fn dot_i16_i16(a: &[i16], b: &[i16]) -> i64 {
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON baseline; bounded loads.
        unsafe { dot_i16_i16_neon(a, b) }
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        dot_scalar(a, b)
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn dot_i16_i16_neon(a: &[i16], b: &[i16]) -> i64 {
    use std::arch::aarch64::*;
    let n = a.len().min(b.len());
    let mut i = 0usize;
    // SAFETY: every access is at `i..i + 8 <= n`.
    unsafe {
        let mut acc0 = vdupq_n_s64(0);
        let mut acc1 = vdupq_n_s64(0);
        while i + 8 <= n {
            let av = vld1q_s16(a.as_ptr().add(i));
            let bv = vld1q_s16(b.as_ptr().add(i));
            acc0 = vpadalq_s32(acc0, vmull_s16(vget_low_s16(av), vget_low_s16(bv)));
            acc1 = vpadalq_s32(acc1, vmull_high_s16(av, bv));
            i += 8;
        }
        let s = vaddq_s64(acc0, acc1);
        vgetq_lane_s64(s, 0).wrapping_add(vgetq_lane_s64(s, 1)).wrapping_add(dot_scalar(&a[i..n], &b[i..n]))
    }
}

/// The checked accumulation's error, for callers that surface it (kept for symmetry).
pub fn overflow_err<T>() -> TirResult<T> {
    overflow("MatMul")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_neon_dots_equal_the_scalar_fold_at_the_rails() {
        for n in [0usize, 1, 7, 15, 16, 17, 31, 511, 512, 513, 1536, 8960, 20_000] {
            let w: Vec<i8> = (0..n).map(|i| if i % 3 == 0 { -128 } else { (i as i64 * 37 % 255 - 127) as i8 }).collect();
            let x: Vec<i16> = (0..n).map(|i| if i % 5 == 0 { i16::MIN } else { (i as i64 * 7919 % 65535 - 32767) as i16 }).collect();
            assert_eq!(dot_i8_i16(&w, &x), dot_scalar(&w, &x), "i8·i16 n={n}");
            let all_min_w = vec![-128i8; n];
            let all_min_x = vec![i16::MIN; n];
            assert_eq!(dot_i8_i16(&all_min_w, &all_min_x), 128 * 32768 * n as i64, "i8·i16 rails n={n}");
            assert_eq!(dot_i16_i16(&x, &x), dot_scalar(&x, &x), "i16·i16 n={n}");
            assert_eq!(dot_i16_i16(&all_min_x, &all_min_x), (1i64 << 30) * n as i64, "i16·i16 rails n={n}");
        }
    }
}
