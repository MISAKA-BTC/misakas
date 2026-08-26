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
//!
//! # The dot-product path, and why it needs no chunk at all
//!
//! `vmlal_s16` does four multiply-accumulates per instruction. ARM's `sdot` does sixteen, and
//! `usdot` (FEAT_I8MM) does sixteen mixed-sign — but both are `int8 × int8`, and this tier's
//! activation is `i16`. The split is algebraic and exact:
//!
//! ```text
//! x = 256·hi + lo      hi = x >> 8  (arithmetic, so hi ∈ [-128, 127])
//!                      lo = x & 255 (so lo ∈ [0, 255], unsigned — hence usdot)
//! Σ w·x = 256·Σ(w·hi) + Σ(w·lo)
//! ```
//!
//! `hi` is signed and pairs with `sdot`; `lo` is unsigned and pairs with `usdot`, whose operand
//! order is (unsigned, signed). Two instructions replace four and the widening of `w` disappears.
//!
//! The lane bound is looser than the `vmlal` path's, not tighter: `sdot` accumulates `n/4`
//! products per lane, so the worst case is `(n/4) · 128 · 255 = n · 8160`, which stays inside an
//! `i32` for every `n` the tier admits — `A16_MAX_DOT_LEN` is 262,144 and the bound breaks at
//! 263,192. So this path has **no chunking**, asserted at compile time below.
//!
//! The intrinsics (`vdotq_s32`, `vusdotq_s32`) are still unstable in Rust 1.94, so the two
//! instructions are written as inline assembly, which is stable. Availability is detected at
//! runtime — `dotprod` is ARMv8.2 and `i8mm` is ARMv8.6, and an Apple M1 has the first and not
//! the second — with the `vmlal` path as the fallback that every machine has.

use kaspa_consensus_core::palw_qwen36_ops::PalwQwen36OpError;
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

const _: () = assert!(
    (A16_MAX_DOT_LEN as i64 / 4) * 128 * 255 < i32::MAX as i64,
    "an sdot/usdot lane accumulates n/4 products of at most 128·255; past this the whole tier's \
     longest reduction would wrap and the dot path would need a chunk like the vmlal path"
);

/// The activation row, split once for the whole projection.
///
/// Splitting per channel would redo `n` shifts and masks `out_dim` times — 8,960 lanes against
/// 1,536 channels on Qwen2.5's down-projection — which is most of what the fast path saves.
pub struct A16Operand {
    codes: Vec<i16>,
    hi: Vec<i8>,
    lo: Vec<u8>,
}

impl A16Operand {
    pub fn new(codes: Vec<i16>) -> Self {
        let hi = codes.iter().map(|v| (*v >> 8) as i8).collect();
        let lo = codes.iter().map(|v| (*v as u16 & 0xFF) as u8).collect();
        Self { codes, hi, lo }
    }
    pub fn len(&self) -> usize {
        self.codes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }
}

/// Whether this machine has both dot-product extensions. Read once: `is_aarch64_feature_detected`
/// is cheap but not free, and it is asked per channel otherwise.
#[cfg(target_arch = "aarch64")]
fn has_dot_extensions() -> bool {
    use std::sync::OnceLock;
    static DETECTED: OnceLock<bool> = OnceLock::new();
    *DETECTED.get_or_init(|| {
        // A diagnostic, not a consensus switch: the two paths are asserted bit-identical, so
        // turning one off changes speed and nothing else. It exists because "the fast path is
        // faster" is a claim that needs an A/B on the same machine, and rebuilding to get one
        // measures the build as well as the kernel.
        if std::env::var_os("MISAKA_PALW_NO_DOTPROD").is_some() {
            return false;
        }
        std::arch::is_aarch64_feature_detected!("dotprod") && std::arch::is_aarch64_feature_detected!("i8mm")
    })
}

/// `Σ w·x` through the split operand, using whichever path this machine has.
#[inline]
pub fn dot_operand(w: &[i8], x: &A16Operand) -> i64 {
    debug_assert_eq!(w.len(), x.codes.len());
    #[cfg(target_arch = "aarch64")]
    if has_dot_extensions() {
        // SAFETY: the extensions were detected above, and the block count keeps every load
        // inside all three slices.
        return unsafe { dot_i8_a16_dotprod(w, x) };
    }
    dot_i8_a16(w, &x.codes)
}

/// `dot_operand` over a sub-range of the operand: `w` against `x.codes[from..from + w.len()]`.
///
/// The hi/lo split path needs the range's own hi/lo slices, so this stays on the plain NEON dot
/// — a 32-element group is two ladder passes and the sdot setup would not amortize anyway.
#[inline]
fn dot_operand_range(w: &[i8], x: &A16Operand, from: usize) -> i64 {
    dot_i8_a16(w, &x.codes[from..from + w.len()])
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "dotprod")]
#[target_feature(enable = "i8mm")]
unsafe fn dot_i8_a16_dotprod(w: &[i8], x: &A16Operand) -> i64 {
    let blocks = w.len() / 16;
    let mut sum_hi = [0i32; 4];
    let mut sum_lo = [0i32; 4];
    if blocks > 0 {
        // SAFETY: `blocks · 16 <= len` for all three slices, and the loop advances each pointer
        // by exactly 16 bytes per iteration.
        unsafe {
            std::arch::asm!(
                "movi v0.4s, #0",
                "movi v1.4s, #0",
                "2:",
                "ld1 {{v2.16b}}, [{w}], #16",
                "ld1 {{v3.16b}}, [{hi}], #16",
                "ld1 {{v4.16b}}, [{lo}], #16",
                // `sdot Vd.4S, Vn.16B, Vm.16B` — four byte-products summed into each lane.
                "sdot v0.4s, v2.16b, v3.16b",
                // `usdot Vd.4S, Vn.16B, Vm.16B` — Vn is the UNSIGNED operand, so the low bytes
                // come first and the weights second. The other order computes a different sum.
                "usdot v1.4s, v4.16b, v2.16b",
                "subs {n}, {n}, #1",
                "b.ne 2b",
                "st1 {{v0.4s}}, [{oh}]",
                "st1 {{v1.4s}}, [{ol}]",
                w = inout(reg) w.as_ptr() => _,
                hi = inout(reg) x.hi.as_ptr() => _,
                lo = inout(reg) x.lo.as_ptr() => _,
                n = inout(reg) blocks => _,
                oh = in(reg) sum_hi.as_mut_ptr(),
                ol = in(reg) sum_lo.as_mut_ptr(),
                out("v0") _,
                out("v1") _,
                out("v2") _,
                out("v3") _,
                out("v4") _,
                options(nostack)
            );
        }
    }
    let head = blocks * 16;
    let hi: i64 = sum_hi.iter().map(|v| *v as i64).sum();
    let lo: i64 = sum_lo.iter().map(|v| *v as i64).sum();
    // The tail is at most fifteen elements and is the same addition.
    256 * hi + lo + dot_i8_a16_scalar(&w[head..], &x.codes[head..])
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

/// **Channels are handed out in blocks, not one at a time.**
///
/// The first version mapped `into_par_iter()` over `0..out_dim`, which on the unembedding is
/// 151,936 tasks each doing a 1,536-length dot — a few microseconds of work behind a work-stealing
/// deque. Measured, the whole engine sat at 39 tok/s while the arithmetic said it should be
/// bandwidth-bound, and switching the dot kernel from `vmlal` to `sdot`/`usdot` made it *slower*,
/// which is the signature of an overhead-bound loop rather than an instruction-bound one.
///
/// Blocks are sized so that every thread gets a handful — enough to balance a ragged tail, few
/// enough that the deque is not the workload.
fn block_size(out_dim: usize) -> usize {
    let target = rayon::current_num_threads().max(1) * 4;
    out_dim.div_ceil(target).max(32)
}

/// Fill `out` with `channel(c)` for every `c`, in blocks, on the pool when it is worth it.
fn fill_channels<F>(out_dim: usize, channel: F) -> Vec<i32>
where
    F: Fn(usize) -> i32 + Sync + Send,
{
    let mut out = vec![0i32; out_dim];
    if out_dim < PARALLEL_MIN_CHANNELS {
        for (c, slot) in out.iter_mut().enumerate() {
            *slot = channel(c);
        }
        return out;
    }
    let block = block_size(out_dim);
    out.par_chunks_mut(block).enumerate().for_each(|(bi, slice)| {
        let base = bi * block;
        for (i, slot) in slice.iter_mut().enumerate() {
            *slot = channel(base + i);
        }
    });
    out
}

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
    let codes = A16Operand::new(as_a16_codes(x)?);
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
        let acc = dot_operand(&weights[c * n..(c + 1) * n], &codes);
        let p = params[c];
        a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
    };
    Ok(fill_channels(out_dim, channel))
}

/// **Op W3 at speed** — bit-identical to `palw_base0_a16::a16_matmul_rescale`.
///
/// The same projection with the Q[`K`] tail: no `clamp16`, saturation at the `i32` rail instead,
/// because `Silu` is defined on Q[`K`] values and narrowing here would change the function.
pub fn a16_matmul_rescale_fast(weights: &[i8], x: &[i32], params: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    let codes = A16Operand::new(as_a16_codes(x)?);
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
        let acc = dot_operand(&weights[c * n..(c + 1) * n], &codes);
        let p = params[c];
        a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero).clamp(i32::MIN as i64, i32::MAX as i64) as i32
    };
    Ok(fill_channels(out_dim, channel))
}

// -------------------------------------------------------------------------------------------
// The batched projections — the same weight row against many tokens
// -------------------------------------------------------------------------------------------

/// **Op W1 over a batch** — `B` activation rows through one projection.
///
/// Decode reads 1.65 GiB of weights to produce one token, so it is bandwidth-bound and no kernel
/// can fix that: the model has to be read. Prefill does not have to be. Every prompt token needs
/// the same weight row, so reading it once and using it `B` times raises the arithmetic per byte
/// by a factor of `B` and turns a bandwidth problem into an arithmetic one.
///
/// Output is `B` rows of `out_dim`, bit-identical to calling `a16_matmul_requant_fast` on each
/// row — which it must be, because prefill and decode meet in the same KV cache and a prompt
/// prefilled in a batch has to leave the state a token-at-a-time prefill would have left.
///
/// The channel loop writes channel-major and transposes at the end. Writing `out[b][c]` directly
/// would need `B` disjoint mutable rows per channel block, which is a borrow the channel-parallel
/// shape does not have; the transpose is `B · out_dim` `i32` copies and is not where this goes.
pub fn a16_matmul_requant_batch(
    weights: &[i8],
    rows: &[Vec<i32>],
    params: &[A16QuantParams],
) -> Result<Vec<Vec<i32>>, PalwA16OpError> {
    batch_projection(weights, rows, params, true)
}

/// **Op W3 over a batch** — the Q[`K`] tail rather than the `int8`-code one.
pub fn a16_matmul_rescale_batch(
    weights: &[i8],
    rows: &[Vec<i32>],
    params: &[A16QuantParams],
) -> Result<Vec<Vec<i32>>, PalwA16OpError> {
    batch_projection(weights, rows, params, false)
}

fn batch_projection(
    weights: &[i8],
    rows: &[Vec<i32>],
    params: &[A16QuantParams],
    narrow_to_codes: bool,
) -> Result<Vec<Vec<i32>>, PalwA16OpError> {
    if rows.is_empty() {
        return Err(PalwA16OpError::Empty);
    }
    let operands: Vec<A16Operand> = rows.iter().map(|r| as_a16_codes(r).map(A16Operand::new)).collect::<Result<_, _>>()?;
    let n = operands[0].len();
    if operands.iter().any(|o| o.len() != n) {
        return Err(PalwA16OpError::LengthMismatch { a: n, b: operands.iter().map(|o| o.len()).max().unwrap_or(0) });
    }
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
    let batch = operands.len();

    let mut channel_major = vec![0i32; out_dim * batch];
    let block = block_size(out_dim);
    let write = |slice: &mut [i32], base: usize| {
        for (i, chunk) in slice.chunks_exact_mut(batch).enumerate() {
            let c = base + i;
            let w = &weights[c * n..(c + 1) * n];
            let p = params[c];
            for (b, slot) in chunk.iter_mut().enumerate() {
                let acc = dot_operand(w, &operands[b]);
                let scaled = a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero);
                *slot = if narrow_to_codes {
                    scaled.clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
                } else {
                    scaled.clamp(i32::MIN as i64, i32::MAX as i64) as i32
                };
            }
        }
    };
    if out_dim < PARALLEL_MIN_CHANNELS {
        write(&mut channel_major, 0);
    } else {
        channel_major.par_chunks_mut(block * batch).enumerate().for_each(|(bi, slice)| write(slice, bi * block));
    }

    let mut out = vec![vec![0i32; out_dim]; batch];
    for (c, chunk) in channel_major.chunks_exact(batch).enumerate() {
        for (b, v) in chunk.iter().enumerate() {
            out[b][c] = *v;
        }
    }
    Ok(out)
}

// -------------------------------------------------------------------------------------------
// The attention arms — the same ops, on the pool
// -------------------------------------------------------------------------------------------

/// `Σ a·b` over two rows of A16 codes carried in `i32` lanes.
///
/// Both operands are `i16` here, not `int8 × i16` like a projection, so `sdot` does not apply: a
/// single product reaches `32767² = 1.07e9` and only the accumulator's width saves it. The sum is
/// `i64` and the products are exact, so the reduction order is free here for the same reason it is
/// everywhere else.
#[inline]
pub fn dot_codes(a: &[i32], b: &[i32]) -> i64 {
    a.iter().zip(b).map(|(x, y)| *x as i64 * *y as i64).sum()
}

/// **Op W9 on the pool** — `q · Kᵀ` per head, per key.
///
/// The reference walks `heads × kv_len` independent dots in one serial loop. At a 360-token
/// history that is 4,320 dots of 128 elements per layer per token, and it was the whole reason a
/// decode after a long prefill ran at 2.5 tok/s while the projections ran at 56 GMAC/s: attention
/// is the part that grows with the context, and it was the part with no kernel.
///
/// Every output element depends on one query head and one key, so nothing here is shared and the
/// only question was scheduling. Bit-identical to `a16_attn_scores` by construction — same
/// products, same order within a dot, same narrowing.
pub fn a16_attn_scores_fast(
    q: &[i32],
    k_series: &[i32],
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    params: &[A16QuantParams],
) -> Result<Vec<i32>, PalwA16OpError> {
    check_codes(q)?;
    check_codes(k_series)?;
    if heads == 0 || kv_heads == 0 || d_head == 0 || !heads.is_multiple_of(kv_heads) {
        return Err(PalwA16OpError::Empty);
    }
    if d_head > A16_MAX_DOT_LEN {
        return Err(PalwA16OpError::DotTooLong { got: d_head });
    }
    if q.len() != heads * d_head {
        return Err(PalwA16OpError::LengthMismatch { a: q.len(), b: heads * d_head });
    }
    let kv_dim = kv_heads * d_head;
    if k_series.is_empty() || !k_series.len().is_multiple_of(kv_dim) {
        return Err(PalwA16OpError::NotAMultiple { got: k_series.len(), unit: kv_dim });
    }
    let kv_len = k_series.len() / kv_dim;
    if params.len() != heads * kv_len {
        return Err(PalwA16OpError::LengthMismatch { a: params.len(), b: heads * kv_len });
    }
    let group = heads / kv_heads;
    Ok(fill_channels(heads * kv_len, |i| {
        let (h, j) = (i / kv_len, i % kv_len);
        let qh = &q[h * d_head..(h + 1) * d_head];
        let kv_off = (h / group) * d_head;
        let kh = &k_series[j * kv_dim + kv_off..j * kv_dim + kv_off + d_head];
        let p = params[i];
        a16_scale_round(dot_codes(qh, kh), p.multiplier, p.shift).saturating_add(p.zero).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
    }))
}

/// **Op W10 on the pool** — `p · V` per head, per output lane.
///
/// The reduction is over the history, so this one gets LONGER as the context grows while the
/// scores arm gets WIDER. Both are `heads × …` independent outputs and both were serial.
pub fn a16_attn_values_fast(
    probs: &[i32],
    v_series: &[i32],
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    params: &[A16QuantParams],
) -> Result<Vec<i32>, PalwA16OpError> {
    check_codes(probs)?;
    check_codes(v_series)?;
    if heads == 0 || kv_heads == 0 || d_head == 0 || !heads.is_multiple_of(kv_heads) {
        return Err(PalwA16OpError::Empty);
    }
    let kv_dim = kv_heads * d_head;
    if v_series.is_empty() || !v_series.len().is_multiple_of(kv_dim) {
        return Err(PalwA16OpError::NotAMultiple { got: v_series.len(), unit: kv_dim });
    }
    let kv_len = v_series.len() / kv_dim;
    if kv_len > A16_MAX_DOT_LEN {
        return Err(PalwA16OpError::DotTooLong { got: kv_len });
    }
    if probs.len() != heads * kv_len {
        return Err(PalwA16OpError::LengthMismatch { a: probs.len(), b: heads * kv_len });
    }
    if params.len() != heads * d_head {
        return Err(PalwA16OpError::LengthMismatch { a: params.len(), b: heads * d_head });
    }
    let group = heads / kv_heads;
    Ok(fill_channels(heads * d_head, |idx| {
        let (h, i) = (idx / d_head, idx % d_head);
        let ph = &probs[h * kv_len..(h + 1) * kv_len];
        let kv_off = (h / group) * d_head;
        // `V` is position-major, so this reduction strides by `kv_dim` — the one place in this
        // module where the inner loop is not contiguous, and the reason it is written as an
        // explicit sum rather than through `dot_codes`.
        let acc: i64 = (0..kv_len).map(|j| ph[j] as i64 * v_series[j * kv_dim + kv_off + i] as i64).sum();
        let p = params[idx];
        a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
    }))
}

fn check_codes(row: &[i32]) -> Result<(), PalwA16OpError> {
    if row.is_empty() {
        return Err(PalwA16OpError::Empty);
    }
    if row.iter().any(|v| (*v as i64).abs() > A16_CODE_MAX) {
        return Err(PalwA16OpError::LengthMismatch { a: row.len(), b: row.len() });
    }
    Ok(())
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

    /// The dot-product path against the scalar one, over the lengths and both code rails. The
    /// matmul differential covers it too, but this names a length when it fails and it is also
    /// the only test that runs when the machine HAS the extensions and the fallback does not.
    #[test]
    fn the_dot_extension_path_equals_the_scalar_one() {
        let mut rng = Lcg(0xA16_0004);
        let w: Vec<i8> = (0..20_000).map(|_| rng.i8()).collect();
        let codes: Vec<i16> = (0..20_000).map(|_| rng.code() as i16).collect();
        for n in [0usize, 1, 15, 16, 17, 31, 512, 1536, 8960, 20_000] {
            let operand = A16Operand::new(codes[..n].to_vec());
            assert_eq!(dot_operand(&w[..n], &operand), dot_i8_a16_scalar(&w[..n], &codes[..n]), "n={n}");
        }
        // Both rails at the longest projection width: the worst case for an sdot/usdot lane.
        for (weight, code) in [(-128i8, -32_767i16), (-128, 32_767), (127, 32_767), (127, -32_767)] {
            let n = 8960;
            let operand = A16Operand::new(vec![code; n]);
            assert_eq!(dot_operand(&vec![weight; n], &operand), n as i64 * weight as i64 * code as i64, "w={weight} code={code}");
        }
    }

    /// The split itself: `256·hi + lo` must reconstruct every `i16`, negatives included. This is
    /// the identity the whole path rests on, and it is the one a reader will doubt.
    #[test]
    fn the_high_low_split_is_exact() {
        for v in [i16::MIN, -32_767, -256, -255, -1, 0, 1, 255, 256, 32_767, i16::MAX] {
            let operand = A16Operand::new(vec![v]);
            assert_eq!(256 * operand.hi[0] as i32 + operand.lo[0] as i32, v as i32, "v={v}");
            assert!((-128..=127).contains(&(operand.hi[0] as i32)), "hi must fit an i8 for v={v}");
        }
    }

    /// A batched projection must equal the same rows one at a time. Prefill and decode meet in
    /// one KV cache, so a prompt prefilled in a batch has to leave the state a token-at-a-time
    /// prefill would have left — not a close one.
    #[test]
    fn the_batched_projection_equals_the_rows_one_at_a_time() {
        let mut rng = Lcg(0xA16_0005);
        for (n, out_dim) in [(16usize, 8usize), (1536, 64), (1536, 1536), (8960, 1536), (1536, 8960)] {
            let weights: Vec<i8> = (0..out_dim * n).map(|_| rng.i8()).collect();
            let params: Vec<A16QuantParams> =
                (0..out_dim).map(|_| A16QuantParams { multiplier: 1 << 20, shift: 34, zero: rng.code() as i64 % 8 }).collect();
            for batch in [1usize, 2, 7, 16] {
                let rows: Vec<Vec<i32>> = (0..batch).map(|_| (0..n).map(|_| rng.code()).collect()).collect();
                let batched = a16_matmul_requant_batch(&weights, &rows, &params).expect("well-formed");
                let batched_rescale = a16_matmul_rescale_batch(&weights, &rows, &params).expect("well-formed");
                for (b, row) in rows.iter().enumerate() {
                    assert_eq!(
                        batched[b],
                        a16_matmul_requant_fast(&weights, row, &params).expect("well-formed"),
                        "n={n} out_dim={out_dim} batch={batch} row={b}"
                    );
                    assert_eq!(
                        batched_rescale[b],
                        a16_matmul_rescale_fast(&weights, row, &params).expect("well-formed"),
                        "rescale n={n} batch={batch} row={b}"
                    );
                }
            }
        }
    }

    /// The attention arms against the catalog ops, at the head geometry Qwen2.5 uses (12 query
    /// heads over 2 kv heads) and at histories on both sides of the parallel threshold.
    #[test]
    fn the_attention_arms_equal_the_catalog_ops() {
        use kaspa_consensus_core::palw_base0_a16::{a16_attn_scores, a16_attn_values};
        let mut rng = Lcg(0xA16_0006);
        let (heads, kv_heads, d_head) = (12usize, 2usize, 128usize);
        let kv_dim = kv_heads * d_head;
        for kv_len in [1usize, 5, 63, 64, 65, 360] {
            let q: Vec<i32> = (0..heads * d_head).map(|_| rng.code()).collect();
            let k: Vec<i32> = (0..kv_len * kv_dim).map(|_| rng.code()).collect();
            let v: Vec<i32> = (0..kv_len * kv_dim).map(|_| rng.code()).collect();
            let probs: Vec<i32> = (0..heads * kv_len).map(|_| rng.code().abs() % 32_768).collect();
            let score_params: Vec<A16QuantParams> =
                (0..heads * kv_len).map(|_| A16QuantParams { multiplier: 1, shift: 26, zero: 0 }).collect();
            let value_params: Vec<A16QuantParams> =
                (0..heads * d_head).map(|_| A16QuantParams { multiplier: 3, shift: 27, zero: 1 }).collect();
            assert_eq!(
                a16_attn_scores_fast(&q, &k, heads, kv_heads, d_head, &score_params),
                a16_attn_scores(&q, &k, heads, kv_heads, d_head, &score_params),
                "scores kv_len={kv_len}"
            );
            assert_eq!(
                a16_attn_values_fast(&probs, &v, heads, kv_heads, d_head, &value_params),
                a16_attn_values(&probs, &v, heads, kv_heads, d_head, &value_params),
                "values kv_len={kv_len}"
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

// -------------------------------------------------------------------------------------------
// The grouped projections — Qwen3.6's weight representation, at the same speed as the tier's
// -------------------------------------------------------------------------------------------

/// `Σ_g (Σ_{i∈g} w·x) << e_g` — the grouped dot, NEON per group and exact.
///
/// A group is 32 weights, which is two 16-lane passes of the same `vmlal_s16` ladder `dot_i8_a16`
/// uses; the group's partial is shifted by its own exponent BEFORE joining the row sum, exactly as
/// the reference does it, so the two are the same arithmetic and not merely close. The bound is
/// the reference's: it validated `per_group · groups ≤ i64::MAX` at the call's entry, so nothing
/// here can wrap.
#[inline]
fn dot_grouped(w: &[i8], x: &A16Operand, exps: &[i8]) -> i64 {
    let n = w.len();
    let mut acc: i64 = 0;
    for (g, exp) in exps.iter().enumerate() {
        let from = g * 32;
        let to = (from + 32).min(n);
        if from >= to {
            break;
        }
        let partial = dot_operand_range(&w[from..to], x, from);
        acc += partial << *exp;
    }
    acc
}

/// **`q36_matmul_grouped` at speed** — bit-identical to the reference on every admitted input.
///
/// The engine's every projection is this op (the checkpoint is Q4_K, whose per-32 scales the
/// artifact keeps as per-32 exponents), and it was the one matmul still running the scalar
/// reference: single-threaded, `i64` multiplies per element, re-reading the activation row per
/// channel. Same three repairs as `a16_matmul_requant_fast`, same differential holding them
/// honest.
pub fn q36_matmul_grouped_fast(
    weights: &[i8],
    exps: &[i8],
    x: &[i32],
    params: &[A16QuantParams],
) -> Result<Vec<i32>, PalwQwen36OpError> {
    grouped_fast(weights, exps, x, params, false)
}

/// **`q36_matmul_grouped_wide` at speed** — the Q[`K`] tail, saturating at the `i32` rail.
pub fn q36_matmul_grouped_wide_fast(
    weights: &[i8],
    exps: &[i8],
    x: &[i32],
    params: &[A16QuantParams],
) -> Result<Vec<i32>, PalwQwen36OpError> {
    grouped_fast(weights, exps, x, params, true)
}

fn grouped_fast(
    weights: &[i8],
    exps: &[i8],
    x: &[i32],
    params: &[A16QuantParams],
    wide: bool,
) -> Result<Vec<i32>, PalwQwen36OpError> {
    // The reference IS the validator: every shape rule and the accumulator bound run there, on a
    // one-channel probe, so this path cannot admit anything the reference refuses.
    let n = x.len();
    let out_dim = params.len();
    if out_dim == 0 || n == 0 {
        return Err(PalwQwen36OpError::Empty);
    }
    if weights.len() != out_dim * n {
        return Err(PalwQwen36OpError::LengthMismatch { a: weights.len(), b: out_dim * n });
    }
    let groups = n.div_ceil(32);
    if exps.len() != out_dim * groups {
        return Err(PalwQwen36OpError::LengthMismatch { a: exps.len(), b: out_dim * groups });
    }
    // One-channel probe through the reference to run ITS validation (A16 range, exponent domain,
    // the i64 bound), then the fast path for the row.
    if wide {
        kaspa_consensus_core::palw_qwen36_ops::q36_matmul_grouped_wide(&weights[..n], &exps[..groups], x, &params[..1])?;
    } else {
        kaspa_consensus_core::palw_qwen36_ops::q36_matmul_grouped(&weights[..n], &exps[..groups], x, &params[..1])?;
    }
    let codes = A16Operand::new(x.iter().map(|v| *v as i16).collect());
    let (lo, hi) = if wide { (i32::MIN as i64, i32::MAX as i64) } else { (-A16_CODE_MAX, A16_CODE_MAX) };
    let channel = |c: usize| -> i32 {
        let acc = dot_grouped(&weights[c * n..(c + 1) * n], &codes, &exps[c * groups..(c + 1) * groups]);
        let p = params[c];
        a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero).clamp(lo, hi) as i32
    };
    Ok(fill_channels(out_dim, channel))
}

#[cfg(test)]
mod grouped_tests {
    use super::*;
    use kaspa_consensus_core::palw_qwen36_ops::{q36_matmul_grouped, q36_matmul_grouped_wide};

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
    }

    /// The fast grouped pair against the reference, exact over shapes, exponents and signs —
    /// including a ragged final group, which is where a range-based rewrite goes wrong first.
    #[test]
    fn the_fast_grouped_matmuls_are_bit_identical_to_the_reference() {
        let mut rng = Lcg(0x36_57EED);
        for (out_dim, n) in [(1usize, 32usize), (3, 40), (17, 96), (64, 2048), (5, 33)] {
            let groups = n.div_ceil(32);
            let weights: Vec<i8> = (0..out_dim * n).map(|_| (rng.next() % 255) as i8).collect();
            let exps: Vec<i8> = (0..out_dim * groups).map(|_| (rng.next() % 21) as i8).collect();
            let x: Vec<i32> = (0..n).map(|_| (rng.next() % 65535) as i32 - 32767).collect();
            let params: Vec<A16QuantParams> = (0..out_dim)
                .map(|_| A16QuantParams { multiplier: 1 + (rng.next() % (1 << 30)) as i64, shift: 30 + (rng.next() % 20) as u8, zero: 0 })
                .collect();
            let want = q36_matmul_grouped(&weights, &exps, &x, &params).expect("the reference admits the shape");
            let got = q36_matmul_grouped_fast(&weights, &exps, &x, &params).expect("the fast path admits it too");
            assert_eq!(want, got, "grouped diverged at {out_dim}x{n}");
            let want = q36_matmul_grouped_wide(&weights, &exps, &x, &params).expect("the reference admits the shape");
            let got = q36_matmul_grouped_wide_fast(&weights, &exps, &x, &params).expect("the fast path admits it too");
            assert_eq!(want, got, "grouped-wide diverged at {out_dim}x{n}");
        }
    }
}
