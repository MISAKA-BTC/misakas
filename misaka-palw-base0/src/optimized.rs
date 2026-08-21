//! **A second implementation, written to disagree — and it cannot (condition 7).**
//!
//! ADR-0040 Decision E is the class's central claim: integer addition is exactly associative and
//! commutative, so the order a dot product, a norm sum or a softmax denominator is accumulated in
//! **cannot change the result** — across thread counts, SIMD widths, tile shapes, compilers or
//! vendors. A float class needs a pinned reduction order, a pinned FMA policy and a pinned thread
//! count to say anything like that. This one needs none.
//!
//! A claim like that is worth exactly as much as the attempt to break it. So the kernels here are
//! deliberately built the way an optimized backend would build them — and every structural choice
//! is one that would change a float result:
//!
//! * **interleaved lanes**, as a SIMD implementation accumulates: `L` partial sums advanced in
//!   parallel and combined at the end, which is a completely different association than a serial
//!   left fold;
//! * **blocked/tiled traversal**, as a cache-blocked implementation does: the reduction split
//!   into chunks, each summed separately, the chunk results summed after;
//! * **reverse order**, the cheapest way to expose an order dependence that a forward-only test
//!   would miss.
//!
//! Run against the scalar reference over the same inputs, all four must be bit-identical. They
//! are, and the reason is not luck: `i32` addition wraps in release and is checked in debug, and
//! within [`MAX_DOT_LEN`] no accumulation can overflow — which is why that bound is a premise of
//! Decision E rather than a safety nicety.
//!
//! **What this does not claim.** It is not a GPU backend. It is the property a GPU backend would
//! have to preserve, tested at the one place a GPU backend could break it: the reduction. Nothing
//! here is on the block-validation path.

use kaspa_consensus_core::palw_base0_ops::{PalwBase0OpError, dot_i8};

/// `dot_i8` with `lanes` partial accumulators advanced in parallel, combined at the end.
///
/// The shape a SIMD kernel has: eight or sixteen lanes each summing every `lanes`-th term, then
/// a horizontal add. In floats this is the canonical source of "same code, different machine,
/// different answer"; in integers it is the same number.
pub fn dot_i8_interleaved(a: &[i8], b: &[i8], lanes: usize) -> Result<i32, PalwBase0OpError> {
    if a.len() != b.len() {
        return Err(PalwBase0OpError::LengthMismatch { a: a.len(), b: b.len() });
    }
    if a.len() > kaspa_consensus_core::palw_base0::MAX_DOT_LEN {
        return Err(PalwBase0OpError::DotTooLong { got: a.len() });
    }
    let lanes = lanes.max(1);
    let mut partial = vec![0i32; lanes];
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        partial[i % lanes] += (*x as i32) * (*y as i32);
    }
    // Horizontal combine, itself in a different order than the lanes were filled.
    Ok(partial.iter().rev().fold(0i32, |acc, p| acc + p))
}

/// `dot_i8` in cache-sized blocks: each block summed on its own, the block results summed after.
pub fn dot_i8_blocked(a: &[i8], b: &[i8], block: usize) -> Result<i32, PalwBase0OpError> {
    if a.len() != b.len() {
        return Err(PalwBase0OpError::LengthMismatch { a: a.len(), b: b.len() });
    }
    if a.len() > kaspa_consensus_core::palw_base0::MAX_DOT_LEN {
        return Err(PalwBase0OpError::DotTooLong { got: a.len() });
    }
    let block = block.max(1);
    let mut total = 0i32;
    for (x, y) in a.chunks(block).zip(b.chunks(block)) {
        let mut sub = 0i32;
        for (p, q) in x.iter().zip(y) {
            sub += (*p as i32) * (*q as i32);
        }
        total += sub;
    }
    Ok(total)
}

/// `dot_i8` from the far end. The cheapest order change there is, and the one a forward-only
/// differential would never notice.
pub fn dot_i8_reversed(a: &[i8], b: &[i8]) -> Result<i32, PalwBase0OpError> {
    if a.len() != b.len() {
        return Err(PalwBase0OpError::LengthMismatch { a: a.len(), b: b.len() });
    }
    if a.len() > kaspa_consensus_core::palw_base0::MAX_DOT_LEN {
        return Err(PalwBase0OpError::DotTooLong { got: a.len() });
    }
    let mut acc = 0i32;
    for (x, y) in a.iter().rev().zip(b.iter().rev()) {
        acc += (*x as i32) * (*y as i32);
    }
    Ok(acc)
}

/// `matmul_quant` with the OUTPUT rows traversed back to front and each dot blocked.
///
/// Two independent order changes at once — which rows are computed when, and how each row's
/// reduction associates — because a real optimized matmul changes both.
pub fn matmul_quant_blocked(weights: &[i8], x: &[i8], out_dim: usize, block: usize) -> Result<Vec<i32>, PalwBase0OpError> {
    if x.is_empty() || out_dim == 0 {
        return Err(PalwBase0OpError::Empty);
    }
    if weights.len() != out_dim * x.len() {
        return Err(PalwBase0OpError::LengthMismatch { a: weights.len(), b: out_dim * x.len() });
    }
    let in_dim = x.len();
    let mut out = vec![0i32; out_dim];
    for row in (0..out_dim).rev() {
        out[row] = dot_i8_blocked(&weights[row * in_dim..(row + 1) * in_dim], x, block)?;
    }
    Ok(out)
}

/// The reference, for a differential to compare against.
pub fn dot_i8_reference(a: &[i8], b: &[i8]) -> Result<i32, PalwBase0OpError> {
    dot_i8(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_base0_ops::matmul_quant;

    /// A deterministic operand sequence spanning the whole int8 range, including both rails.
    fn codes(n: usize, seed: u64) -> Vec<i8> {
        let mut state = seed | 1;
        (0..n)
            .map(|i| {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                // Force the extremes in periodically: the interesting accumulations are the ones
                // near the accumulator's own limits, and a uniform sample rarely reaches them.
                match i % 17 {
                    0 => 127,
                    1 => -128,
                    _ => ((state >> 33) % 255) as i32 as i8,
                }
            })
            .collect()
    }

    /// **Condition 7: four implementations, one answer, bit for bit.**
    ///
    /// Every variant here is a structural change that WOULD move a float result — interleaved
    /// lanes, blocked traversal, reverse order — applied across lengths that cross every block
    /// and lane boundary. If integer associativity held only approximately, one of these would
    /// disagree.
    #[test]
    fn every_reduction_order_gives_the_same_bits() {
        for len in [1usize, 2, 3, 7, 8, 15, 16, 17, 31, 64, 127, 128, 129, 1_000, 4_096] {
            for seed in [1u64, 0xDEAD_BEEF, 0x5EED] {
                let (a, b) = (codes(len, seed), codes(len, seed ^ 0xFFFF));
                let want = dot_i8_reference(&a, &b).unwrap();
                assert_eq!(dot_i8_reversed(&a, &b).unwrap(), want, "reversed, len {len}");
                for lanes in [1usize, 2, 4, 8, 16, 32] {
                    assert_eq!(dot_i8_interleaved(&a, &b, lanes).unwrap(), want, "{lanes} lanes, len {len}");
                }
                for block in [1usize, 3, 16, 64, 256] {
                    assert_eq!(dot_i8_blocked(&a, &b, block).unwrap(), want, "block {block}, len {len}");
                }
            }
        }
    }

    /// The same, one level up: a whole matmul with its rows traversed backwards and each row's
    /// reduction blocked.
    #[test]
    fn a_blocked_matmul_matches_the_reference_exactly() {
        for (out_dim, in_dim) in [(1usize, 1usize), (3, 7), (8, 64), (16, 128), (5, 1_000)] {
            let w = codes(out_dim * in_dim, 0xA11CE);
            let x = codes(in_dim, 0xB0B);
            let want = matmul_quant(&w, &x, out_dim).unwrap();
            for block in [1usize, 7, 64, 512] {
                assert_eq!(matmul_quant_blocked(&w, &x, out_dim, block).unwrap(), want, "block {block}, {out_dim}x{in_dim}");
            }
        }
    }

    /// **The bound is a premise, not a nicety.** Decision E holds *while accumulation cannot
    /// overflow*, and every implementation here refuses a length past `MAX_DOT_LEN` rather than
    /// computing a number whose value would depend on the order after all.
    #[test]
    fn every_implementation_refuses_a_reduction_it_cannot_associate_freely() {
        let n = kaspa_consensus_core::palw_base0::MAX_DOT_LEN + 1;
        let a = vec![1i8; n];
        let b = vec![1i8; n];
        assert!(matches!(dot_i8_reference(&a, &b), Err(PalwBase0OpError::DotTooLong { .. })));
        assert!(matches!(dot_i8_reversed(&a, &b), Err(PalwBase0OpError::DotTooLong { .. })));
        assert!(matches!(dot_i8_interleaved(&a, &b, 8), Err(PalwBase0OpError::DotTooLong { .. })));
        assert!(matches!(dot_i8_blocked(&a, &b, 64), Err(PalwBase0OpError::DotTooLong { .. })));
        // …and a mismatched pair is a refusal everywhere too, not a shorter answer.
        assert!(dot_i8_interleaved(&[1, 2], &[1], 4).is_err());
        assert!(dot_i8_blocked(&[1, 2], &[1], 4).is_err());
        assert!(dot_i8_reversed(&[1, 2], &[1]).is_err());
    }

    /// **The differential is not vacuous**, and this is what says so: the same variants applied
    /// to `f32` — the arithmetic a float class would use — DO disagree.
    ///
    /// Without this, "every order gives the same bits" could be read as a statement about the
    /// test rather than about integers. It is a statement about integers.
    #[test]
    fn the_same_orders_disagree_in_floating_point() {
        // One term at 1.0 and 4,095 at 1e-8. `ulp(1.0)` in `f32` is 2^-23 ≈ 1.19e-7, so summed
        // forward every small term rounds away and the answer stays exactly 1.0. Summed in blocks
        // of 64 the small terms first accumulate to 6.4e-7 — well above that ulp — and survive.
        // Measured: serial 0x3f800000, blocked 0x3f80013b, reversed 0x3f800158.
        let mut v = vec![1e-8f32; 4_096];
        v[0] = 1.0;
        let serial = v.iter().fold(0f32, |acc, x| acc + x);
        let blocked: f32 = v.chunks(64).map(|c| c.iter().fold(0f32, |a, x| a + x)).sum();
        let reversed = v.iter().rev().fold(0f32, |acc, x| acc + x);
        assert_ne!(
            serial.to_bits(),
            blocked.to_bits(),
            "if this ever passes, the float half of the comparison has stopped being a comparison"
        );
        assert_ne!(serial.to_bits(), reversed.to_bits());

        // The SAME data as integers, at the same orders: identical. This is the whole reason the
        // class is integer arithmetic, in two assertions.
        let ints: Vec<i8> = (0..4_096).map(|i| if i == 0 { 127 } else { 1 }).collect();
        let ones = vec![1i8; 4_096];
        let want = dot_i8_reference(&ints, &ones).unwrap();
        assert_eq!(dot_i8_blocked(&ints, &ones, 64).unwrap(), want);
        assert_eq!(dot_i8_reversed(&ints, &ones).unwrap(), want);
    }
}
