//! **Fast kernels for the A16 tier — the same numbers, two orders of magnitude of headroom.**
//!
//! # Why this is allowed to exist at all
//!
//! In a float runtime this module would be a fork of the specification. Vectorising a dot product
//! changes which partial sums are formed; threading it changes them again; and in floating point
//! each of those is a different answer, which is why llama.cpp cannot promise that two machines
//! agree and why every float verification scheme has to pin a reduction order, a thread count and
//! an FMA policy.
//!
//! ADR-0040 Decision E removes the whole category. Integer addition is exactly associative and
//! commutative inside the no-overflow bound, so **lanes, tiles and threads cannot change the
//! result** — and `optimized.rs` already tested that claim on the int8 tier by writing kernels
//! specifically to break it. This module is what the claim was worth: the first kernel that goes
//! fast *because* the arithmetic said it could.
//!
//! # The one thing that is not free
//!
//! The premise of Decision E is that nothing overflows. The reference accumulates in `i64`, where
//! `A16_MAX_DOT_LEN` leaves room to spare; a SIMD kernel wants `i32` lanes, where it does not.
//! With `|w| ≤ 128` and `|x| ≤ 32_767` a single product reaches 4.19e6, so an `i32` lane holds
//! 512 terms and no more. The reduction is therefore **chunked at 512 elements** — each chunk
//! summed in `i32` lanes, each chunk's total widened into `i64` — which is a different
//! association than the reference's left fold and, for exactly the reason above, the same number.
//!
//! Get the chunk wrong and nothing tells you: the wrap is silent in release, and the differential
//! below is what stands between that and a producer whose blocks are refutable.

use kaspa_consensus_core::palw_base0_a16::{A16_CODE_MAX, A16_MAX_DOT_LEN, A16QuantParams, PalwA16OpError, a16_scale_round};
use rayon::prelude::*;

/// Terms per `i32` accumulation chunk. `512 · 128 · 32_767 = 2.15e9`… which is over `i32::MAX`,
/// so the real bound is per LANE: four lanes carry 128 terms each, 128 · 4.19e6 = 5.4e8, a
/// four-fold margin. The constant is the element count because that is what the loop counts.
const CHUNK: usize = 512;

const _: () = assert!(
    (CHUNK as i64 / 4) * 128 * A16_CODE_MAX < i32::MAX as i64,
    "an i32 lane must hold CHUNK/4 worst-case products — past that the sum wraps silently and \
     Decision E's associativity, which is conditional on no overflow, stops holding"
);

/// `Σ w[i] · x[i]` in `i64`, with `w` int8 and `x` A16 codes.
///
/// Bit-identical to the reference's `.map(|(w, v)| *w as i64 * *v as i64).sum()` on every input
/// the tier admits.
#[cfg(target_arch = "aarch64")]
#[inline]
pub fn dot_i8_a16(w: &[i8], x: &[i16]) -> i64 {
    debug_assert_eq!(w.len(), x.len());
    // SAFETY: NEON is part of the aarch64 baseline, so these intrinsics are always available on
    // this target. Every load inside is bounded by the chunk arithmetic.
    unsafe { dot_i8_a16_neon(w, x) }
}

/// Every other target, for now. A `vpdpbusd`/`__dp4a` path belongs here and would be held to the
/// same differential.
#[cfg(not(target_arch = "aarch64"))]
#[inline]
pub fn dot_i8_a16(w: &[i8], x: &[i16]) -> i64 {
    debug_assert_eq!(w.len(), x.len());
    dot_i8_a16_scalar(w, x)
}

/// The portable path, and the shape every vector path must reproduce.
#[inline]
pub fn dot_i8_a16_scalar(w: &[i8], x: &[i16]) -> i64 {
    w.iter().zip(x).map(|(a, b)| *a as i64 * *b as i64).sum()
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn dot_i8_a16_neon(w: &[i8], x: &[i16]) -> i64 {
    use std::arch::aarch64::*;
    let n = w.len();
    let mut total: i64 = 0;
    let mut base = 0usize;
    while base < n {
        let end = (base + CHUNK).min(n);
        // SAFETY: `base..end` is inside both slices, and every pointer read below stays within
        // `i + 16 <= end`.
        let mut acc0 = unsafe { vdupq_n_s32(0) };
        let mut acc1 = unsafe { vdupq_n_s32(0) };
        let mut i = base;
        while i + 16 <= end {
            // SAFETY: 16 elements remain in both slices from `i`.
            unsafe {
                let wv = vld1q_s8(w.as_ptr().add(i));
                let w_lo = vmovl_s8(vget_low_s8(wv));
                let w_hi = vmovl_high_s8(wv);
                let x0 = vld1q_s16(x.as_ptr().add(i));
                let x1 = vld1q_s16(x.as_ptr().add(i + 8));
                acc0 = vmlal_s16(acc0, vget_low_s16(w_lo), vget_low_s16(x0));
                acc1 = vmlal_high_s16(acc1, w_lo, x0);
                acc0 = vmlal_s16(acc0, vget_low_s16(w_hi), vget_low_s16(x1));
                acc1 = vmlal_high_s16(acc1, w_hi, x1);
            }
            i += 16;
        }
        // Widen the lanes into `i64` before combining chunks: the lanes are the only place an
        // `i32` is allowed to hold a partial sum, and it stops being allowed at `CHUNK`.
        // SAFETY: register-only operations.
        let widened = unsafe { vaddq_s64(vpaddlq_s32(acc0), vpaddlq_s32(acc1)) };
        // SAFETY: lane indices are in range for an int64x2_t.
        total += unsafe { vgetq_lane_s64(widened, 0) + vgetq_lane_s64(widened, 1) };
        // The chunk's tail, scalar. It is at most 15 elements and it is the same addition.
        total += dot_i8_a16_scalar(&w[i..end], &x[i..end]);
        base = end;
    }
    total
}

/// The A16 code check the reference applies through its private `as_a16`, reproduced here so the
/// fast path refuses exactly what the reference refuses.
fn as_a16_codes(row: &[i32]) -> Result<Vec<i16>, PalwA16OpError> {
    if row.is_empty() {
        return Err(PalwA16OpError::Empty);
    }
    if row.iter().any(|v| (*v as i64).abs() > A16_CODE_MAX) {
        return Err(PalwA16OpError::LengthMismatch { a: row.len(), b: row.len() });
    }
    Ok(row.iter().map(|v| *v as i16).collect())
}

/// Below this many output channels the reduction is run on the calling thread: a projection with
/// a handful of rows costs less than the pool costs to reach.
const PARALLEL_MIN_CHANNELS: usize = 64;

/// **Op W1 at speed** — bit-identical to `palw_base0_a16::a16_matmul_requant`.
///
/// The activation row is narrowed to `i16` ONCE for the whole projection rather than per channel:
/// it is the same row for every output, and re-narrowing it `out_dim` times was most of what the
/// scalar version spent on a wide FFN.
///
/// Channels are independent — each writes one output and reads a disjoint weight row — so they
/// run in parallel. That the parallel result equals the serial one is not a hope about scheduling;
/// it is that no channel's arithmetic can observe another's.
pub fn a16_matmul_requant_fast(weights: &[i8], x: &[i32], params: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    let codes = as_a16_codes(x)?;
    let n = codes.len();
    if n > A16_MAX_DOT_LEN {
        return Err(PalwA16OpError::DotTooLong { got: n });
    }
    let out_dim = params.len();
    if out_dim == 0 {
        return Err(PalwA16OpError::Empty);
    }
    if weights.len() != out_dim * n {
        return Err(PalwA16OpError::LengthMismatch { a: weights.len(), b: out_dim * n });
    }
    let channel = |c: usize| -> i32 {
        let acc = dot_i8_a16(&weights[c * n..(c + 1) * n], &codes);
        let p = params[c];
        a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
    };
    if out_dim >= PARALLEL_MIN_CHANNELS {
        Ok((0..out_dim).into_par_iter().map(channel).collect())
    } else {
        Ok((0..out_dim).map(channel).collect())
    }
}

/// **Op W3 at speed** — bit-identical to `palw_base0_a16::a16_matmul_rescale`.
///
/// The same projection with the Q[`K`] tail: no `clamp16`, saturation at the `i32` rail instead,
/// because `Silu` is defined on Q[`K`] values and narrowing here would change the function.
pub fn a16_matmul_rescale_fast(weights: &[i8], x: &[i32], params: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    let codes = as_a16_codes(x)?;
    let n = codes.len();
    if n > A16_MAX_DOT_LEN {
        return Err(PalwA16OpError::DotTooLong { got: n });
    }
    let out_dim = params.len();
    if out_dim == 0 {
        return Err(PalwA16OpError::Empty);
    }
    if weights.len() != out_dim * n {
        return Err(PalwA16OpError::LengthMismatch { a: weights.len(), b: out_dim * n });
    }
    let channel = |c: usize| -> i32 {
        let acc = dot_i8_a16(&weights[c * n..(c + 1) * n], &codes);
        let p = params[c];
        a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero).clamp(i32::MIN as i64, i32::MAX as i64) as i32
    };
    if out_dim >= PARALLEL_MIN_CHANNELS {
        Ok((0..out_dim).into_par_iter().map(channel).collect())
    } else {
        Ok((0..out_dim).map(channel).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_base0_a16::{a16_matmul_requant, a16_matmul_rescale};

    struct Lcg(u64);
    impl Lcg {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            self.0.rotate_right(24)
        }
        fn i8(&mut self) -> i8 {
            self.next_u64() as u8 as i8
        }
        /// The FULL A16 code range including both rails: the worst case for an `i32` lane is the
        /// extreme, and a generator that samples small values would let a wrong `CHUNK` pass.
        fn code(&mut self) -> i32 {
            (self.next_u64() % (2 * A16_CODE_MAX as u64 + 1)) as i64 as i32 - A16_CODE_MAX as i32
        }
    }

    /// The property the module exists for, at the lengths a real projection uses — including
    /// `d_ff = 8960`, where a wrong chunk would wrap.
    #[test]
    fn the_fast_kernel_is_bit_identical_to_the_reference() {
        let mut rng = Lcg(0xA16_0001);
        for n in [1usize, 2, 15, 16, 17, 63, 64, 127, 511, 512, 513, 1536, 8960] {
            for out_dim in [1usize, 3, 64, 65] {
                let weights: Vec<i8> = (0..out_dim * n).map(|_| rng.i8()).collect();
                let x: Vec<i32> = (0..n).map(|_| rng.code()).collect();
                let params: Vec<A16QuantParams> = (0..out_dim)
                    .map(|_| A16QuantParams {
                        multiplier: (rng.next_u64() % (1 << 40)) as i64 - (1 << 39),
                        shift: (rng.next_u64() % 63) as u8,
                        zero: rng.code() as i64,
                    })
                    .collect();
                assert_eq!(
                    a16_matmul_requant_fast(&weights, &x, &params),
                    a16_matmul_requant(&weights, &x, &params),
                    "MatMulRequant n={n} out_dim={out_dim}"
                );
                assert_eq!(
                    a16_matmul_rescale_fast(&weights, &x, &params),
                    a16_matmul_rescale(&weights, &x, &params),
                    "MatMulRescale n={n} out_dim={out_dim}"
                );
            }
        }
    }

    /// The saturating case, constructed rather than sampled: every weight and every code at its
    /// rail, over the longest row the geometry uses. If `CHUNK` were too large this is where the
    /// `i32` lane wraps, and the sign of the answer flips.
    #[test]
    fn the_extremes_do_not_wrap_a_lane() {
        for n in [512usize, 513, 8960, A16_MAX_DOT_LEN] {
            for (w, code) in [(-128i8, -(A16_CODE_MAX as i32)), (-128, A16_CODE_MAX as i32), (127, A16_CODE_MAX as i32)] {
                let weights = vec![w; n];
                let x = vec![code; n];
                let expected = n as i64 * w as i64 * code as i64;
                assert_eq!(
                    dot_i8_a16(&weights, &x.iter().map(|v| *v as i16).collect::<Vec<_>>()),
                    expected,
                    "n={n} w={w} code={code}"
                );
            }
        }
    }

    /// The vector path against the scalar one, directly, at every alignment around the 16-element
    /// block and the 512-element chunk. The matmul differential covers this too, but a failure
    /// there names a channel; a failure here names a length.
    #[test]
    fn the_vector_path_equals_the_scalar_one() {
        let mut rng = Lcg(0xA16_0003);
        let w: Vec<i8> = (0..20_000).map(|_| rng.i8()).collect();
        let x: Vec<i16> = (0..20_000).map(|_| rng.code() as i16).collect();
        for n in [0usize, 1, 7, 15, 16, 17, 31, 511, 512, 513, 1023, 1024, 1536, 8960, 20_000] {
            assert_eq!(dot_i8_a16(&w[..n], &x[..n]), dot_i8_a16_scalar(&w[..n], &x[..n]), "n={n}");
        }
        // And at an offset, so the vector loads are not always 16-byte aligned.
        for offset in [1usize, 3, 8, 9] {
            let n = 4096;
            assert_eq!(
                dot_i8_a16(&w[offset..offset + n], &x[offset..offset + n]),
                dot_i8_a16_scalar(&w[offset..offset + n], &x[offset..offset + n]),
                "offset={offset}"
            );
        }
    }

    /// The refusals must match too: a fast path that accepts a row the reference refuses is a
    /// producer that mints a receipt the court cannot reproduce.
    #[test]
    fn the_fast_kernel_refuses_exactly_what_the_reference_refuses() {
        let params = vec![A16QuantParams { multiplier: 1, shift: 0, zero: 0 }; 2];
        // An empty row.
        assert_eq!(a16_matmul_requant_fast(&[], &[], &params), a16_matmul_requant(&[], &[], &params));
        // A lane outside the A16 code range.
        let x = vec![A16_CODE_MAX as i32 + 1, 0];
        let weights = vec![1i8; 4];
        assert_eq!(a16_matmul_requant_fast(&weights, &x, &params), a16_matmul_requant(&weights, &x, &params));
        // A weight block that is not `out_dim · n`.
        let x = vec![1i32, 2];
        assert_eq!(a16_matmul_requant_fast(&[1i8, 2, 3], &x, &params), a16_matmul_requant(&[1i8, 2, 3], &x, &params));
        // No channels.
        assert_eq!(a16_matmul_requant_fast(&weights, &x, &[]), a16_matmul_requant(&weights, &x, &[]));
    }

    /// Threading is not allowed to matter. Same inputs through the parallel and the serial paths,
    /// with the channel count straddling the threshold.
    #[test]
    fn the_thread_count_cannot_change_the_result() {
        let mut rng = Lcg(0xA16_0002);
        let n = 1536;
        for out_dim in [PARALLEL_MIN_CHANNELS - 1, PARALLEL_MIN_CHANNELS, PARALLEL_MIN_CHANNELS + 1, 8960] {
            let weights: Vec<i8> = (0..out_dim * n).map(|_| rng.i8()).collect();
            let x: Vec<i32> = (0..n).map(|_| rng.code()).collect();
            let params: Vec<A16QuantParams> =
                (0..out_dim).map(|_| A16QuantParams { multiplier: 1 << 20, shift: 30, zero: 0 }).collect();
            let parallel = a16_matmul_requant_fast(&weights, &x, &params).expect("well-formed");
            let serial: Vec<i32> = a16_matmul_requant(&weights, &x, &params).expect("well-formed");
            assert_eq!(parallel, serial, "out_dim={out_dim}");
        }
    }
}
