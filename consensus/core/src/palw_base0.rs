//! ADR-0040: `PALW-BASE-0`'s integer-only arithmetic — the six primitives, normatively.
//!
//! Everything a conforming `PALW-BASE-0` implementation must reproduce bit-for-bit reduces to the
//! functions here:
//!
//! ```text
//! integer add, integer multiply    (native, exact — not wrapped in a function)
//! rounding_shift_right             loses information, rounding half AWAY from zero (one of two lossy sites)
//! srdhm                            the ONE fixed-point multiply; loses information too, rounding half UP (gemmlowp's rule)
//! int_exp                          softmax, and (through sigmoid) SiLU
//! int_rsqrt                        RMS norm
//! ```
//!
//! All are total functions on `i32`/`i64`. There is no rounding mode, no denormal, no `errno`, no
//! libm version and no contraction flag anywhere in this module, which is the entire reason
//! ADR-0039 orders this class first: its kernel catalog can actually close, and until a class's
//! catalog closes it may not carry fork-choice weight.
//!
//! # No floating point, at all
//!
//! Not one `f32`/`f64` appears below, including in the constants — those are integer literals,
//! derived once and pinned by [`tests::constants_are_the_pinned_derivations`] rather than
//! recomputed from a float expression at runtime. ADR-0040 Decision A: a build that links `libm`
//! for this class is not a conforming implementation.
//!
//! # The associativity that makes this class worth building
//!
//! Integer addition is exactly associative and commutative, so the order a dot product, a norm
//! sum or a softmax denominator is accumulated in **cannot change the result** — across thread
//! counts, SIMD widths, tile shapes, compilers or vendors (ADR-0040 Decision E). That holds only
//! while accumulation cannot overflow, because saturating addition is *not* associative at the
//! boundary — which is why [`MAX_DOT_LEN`] is a premise of the property rather than a safety
//! nicety.

/// Fixed-point fraction bits. Every `Qk` value below is an integer scaled by `2^K`.
pub const K: u32 = 24;
/// `1.0` in Qk.
pub const ONE: i64 = 1i64 << K;

/// `round(ln 2 · 2^K)` — the range-reduction step for [`int_exp`].
pub const LN2_Q: i32 = 11_629_080;

/// [`int_exp`]'s shifted-square coefficients: `Poly2(p) = A·(p + B)² + C`, all Qk.
///
/// **These are not Horner coefficients.** `A(p+B)² + C` and `Ap² + Bp + C` read the same three
/// numbers and are different algorithms: the shifted-square form hits `Poly2(0) ≈ 1.0` and
/// `Poly2(−ln2) ≈ 0.5`, the two endpoints `exp` must reproduce, while the Horner reading gives
/// `Poly2(0) = 0.344` — every result low by a factor of three, uniformly enough to look like a
/// scale bug rather than a wrong algorithm. ADR-0040 F1 records the error because it was made.
pub const POLY2_A: i64 = 6_014_632;
pub const POLY2_B: i64 = 22_699_573;
pub const POLY2_C: i64 = 5_771_362;

/// Largest range-reduction shift. Beyond it `exp` is below the Qk floor and the result is 0
/// exactly; capping here also keeps the shift a legal one for `i32`.
pub const Z_MAX: i32 = 31;

/// Newton iterations in [`int_rsqrt`]. Pinned, not convergence-tested: a loop that stops when it
/// converges stops at different times on different inputs, and "different times" is a divergence.
/// Measured at 3; 4 and 5 are not more accurate (ADR-0040 F2).
pub const RSQRT_ITERS: u32 = 3;

/// [`int_rsqrt`]'s seed table, indexed by the top four bits of the normalized mantissa.
///
/// **A correctness requirement, not an optimisation.** Newton for `1/√v` converges only from
/// `y₀ ≤ √(3/m)`; a seed above that basin diverges *to zero* rather than oscillating, so the
/// failure is silent and total. Every entry is the reciprocal square root of its bucket's UPPER
/// end, which is conservative by construction. A first draft seeded from the leading bit alone
/// landed exactly on the boundary at `m = 3` and returned 0 (ADR-0040 F2).
pub const RSQRT_SEED: [i64; 16] = [
    15_395_829, 14_307_657, 13_421_772, 12_682_383, 12_053_107, 11_509_075, 11_032_629, 10_610_843, 10_234_005, 9_894_662, 9_586_980,
    9_306_325, 9_048_957, 8_811_825, 8_592_409, 8_388_608,
];

/// The longest `int8 × int8` dot product that provably cannot overflow an `i32` accumulator:
/// `|product| ≤ 128 × 128 = 16_384`, so `MAX_DOT_LEN × 16_384 ≤ i32::MAX`.
///
/// # The worst case is `-128`, and this constant read `133_144` until it was measured
///
/// ADR-0040 C3 derives the bound from `127 × 127 = 16_129` — and states two sentences later that
/// every narrowing "saturates to `[-128, 127]`". Both cannot be true, and the type wins:
/// [`requantize`] really does return `-128` (`requantize(i32::MIN, i32::MAX, 0)`, pinned by
/// `shift_and_multiply_round_and_saturate_as_specified`), an artifact's weight tensors are raw
/// `i8` that nothing range-checks, and the refutation path decodes operands with `i8::try_from`,
/// which accepts it.
/// So `(-128)² = 16_384` is the product a conforming implementation must survive and `131_071` is
/// the length that licenses — 1.6 % below the old figure, and three orders of magnitude above any
/// real shape, which is why nothing ever reached it.
///
/// **Excluding `-128` at the narrowing sites was the alternative, and is the wrong repair.** It
/// changes frozen catalog op 2 for every input that currently narrows to `-128`, contradicting
/// C3's own saturation sentence; and it would still leave the artifact and refutation entry points
/// open, each missed one being a panic inside an adjudicator rather than a wrong answer.
///
/// ADR-0040 C3. This bound is what licenses Decision E's free reduction order, so a graph that
/// exceeds it does not merely risk overflow — it loses the property the class exists for, and
/// must accumulate in `i64` and say so. It is therefore stated over the TYPE, with no side
/// condition on which `i8` values a producer happens to emit: a premise that holds only for a
/// subset of the operand type is not a premise Decision E can use.
pub const MAX_DOT_LEN: usize = 131_071;

/// Decision E's premise, enforced by the compiler rather than by a test that has to be run.
///
/// A test can pin this and still be deleted, skipped, or — as happened here — written against the
/// wrong end of the operand type and pass for four days. A `const` assertion cannot: raising
/// [`MAX_DOT_LEN`] past what an `i32` accumulator survives stops the crate from building, and the
/// only honest way past it is to move the accumulator to `i64` and say so, which is exactly what
/// ADR-0040 C3 requires of a class whose graph outgrows the bound.
const _: () = assert!(
    MAX_DOT_LEN as i64 * (i8::MIN as i64) * (i8::MIN as i64) <= i32::MAX as i64,
    "MAX_DOT_LEN worst-case products must fit an i32 accumulator (ADR-0040 C3, premise of Decision E)"
);

/// Round-half-away-from-zero shift — one of the class's named lossy sites (ADR-0040 C1).
///
/// It is NOT the only one, and the others do NOT share its rule: [`srdhm`] rounds half UP, and the
/// internal `>> K` steps inside the ops and the transcendentals floor. C1 tabulates all three.
/// Products, accumulations and adds are exact.
///
/// # It is written on the magnitude, and that is the whole correctness argument
///
/// The obvious form, `(x ± 2^(s−1)) >> s`, is **wrong for every negative input**, and the first
/// version of this function and of ADR-0040 C1's pseudocode both used it. An arithmetic shift
/// floors, so for `x < 0` the nudge and the floor push the same way instead of opposing: it
/// returns `−33` for `RSR(−64, 1)`, where the exact quotient is `−32` and needs no rounding at
/// all. Measured against gemmlowp's `RoundingDivideByPOT`, the two disagreed on **50 % of random
/// `(x, s)` pairs** — every negative one — always by one unit away from zero. The same form also
/// overflowed `i32` on 3.2 % of pairs, wrapping the sign.
///
/// Rounding the **magnitude** and reapplying the sign is symmetric by construction, so there is
/// no negative case to get wrong. The arithmetic is done in `i64`: `|x| + 2^(s−1)` can exceed
/// `i32` while the result never does, and a `wrapping_add` there turns the largest positive
/// accumulator into a negative answer.
#[inline]
pub fn rounding_shift_right(x: i32, s: u8) -> i32 {
    // The `debug_assert` documents the C1 domain and catches a caller bug in tests; the `min`
    // makes the function TOTAL in release. Without it a shift decoded from oracle bytes on the
    // refutation path (`palw_step_refute` reads `shift: c[4]`, 0..=255, unvalidated) panics for
    // `s >= 64` under `overflow-checks = true` — a `pub` total-arithmetic function must never
    // panic on peer-influenced input. This mirrors `rescale_q`'s `shift.min(RESCALE_MAX_SHIFT)`.
    // Beyond the Qk resolution the result is 0 anyway, so clamping the shift to 31 loses nothing.
    debug_assert!(s <= 31);
    let s = s.min(31);
    if s == 0 {
        return x;
    }
    let (divisor, half) = (1i64 << s, 1i64 << (s - 1));
    let magnitude = (x as i64).abs();
    let rounded = (magnitude + half) / divisor;
    (if x < 0 { -rounded } else { rounded }) as i32
}

/// Saturating rounding doubling high multiply — the ONE fixed-point multiply (ADR-0040 C2).
///
/// gemmlowp's primitive verbatim, deliberately: it is already implemented identically in several
/// independent codebases, which is exactly the property a second implementation needs.
///
/// **Rounds half UP (toward +∞), NOT half-away-from-zero** — the asymmetric nudge `1 − 2^30`
/// composed with truncating division gives `srdhm(-1, 2^30) = 0` where half-away gives `-1`. This
/// differs from [`rounding_shift_right`]'s rule on the negative exact-half products, and it is
/// intentional: matching gemmlowp bit-for-bit is C2's whole purpose. ADR-0040 C1 records why the
/// distinction matters (a third party rounding half-away here would be convicted).
#[inline]
pub fn srdhm(a: i32, b: i32) -> i32 {
    // The single saturating case: -2^31 × -2^31 doubled is 2^63, one past `i64`'s range.
    if a == i32::MIN && b == i32::MIN {
        return i32::MAX;
    }
    // `a · b`, NOT `2 · a · b`. The "doubling" in the name describes the relationship to the
    // hardware VQRDMULH — `(a·b) >> 31` IS `(2·a·b) >> 32` — it is not an extra factor to apply.
    // Writing the 2 explicitly and still shifting by 31 doubles every product and overflows `i32`
    // at the top of the range: 0.5 × 0.5 returns 0.5 instead of 0.25.
    let product = (a as i64) * (b as i64);
    // The asymmetric nudge is `1 - 2^30` for negatives, and it is asymmetric for ONE reason: it
    // compensates for a division that TRUNCATES toward zero. Pairing it with an arithmetic shift
    // — which floors — applies the correction twice, and that is what the first version of this
    // function did. Measured against upstream gemmlowp, `>> 31` disagreed on 50.1 % of random
    // `(a, b)` pairs, every one of them a negative product, always one unit further from zero:
    // `srdhm(-2^30, 2^30)` returned `-2^29 - 1` where the exact value is `-2^29`.
    //
    // That mattered more here than a one-unit error usually does. C2 chose SRDHM *because* it is
    // already implemented identically in several codebases, and a second implementation written
    // against real gemmlowp would then have disagreed with this one on half of all inputs — which
    // under ADR-0027's court is not a rounding difference, it is a conviction.
    ((product + nudge_for(product)) / (1i64 << 31)) as i32
}

/// SRDHM's rounding nudge. Named rather than inlined so the asymmetry has somewhere to be
/// explained: `1 - 2^30` rather than `-2^30` is what makes truncation round half away from zero.
#[inline]
fn nudge_for(product: i64) -> i64 {
    if product >= 0 { 1 << 30 } else { 1 - (1 << 30) }
}

/// `i32` accumulator → `int8`, the explicit narrowing (ADR-0040 D op 2). Saturates; nothing in
/// this class wraps.
#[inline]
pub fn requantize(acc: i32, multiplier: i32, shift: u8) -> i8 {
    requantize_with_zero(acc, multiplier, shift, 0)
}

/// [`requantize`] with the ZERO POINT the standard int8 form carries (ADR-0040 amendment, G2).
///
/// `Saturate8(RoundingShiftRight(SRDHM(acc, mult), shift) + zero)`.
///
/// **Why the class needed an additive term at all.** BASE-0's ten ops had none: `QuantParams` was
/// `{ multiplier, shift }`, `ScaleParams` likewise, and `MulElem`/`AddElem` both take two OPENED
/// rows, so nothing could add a registered vector. That makes a real transformer's projection
/// biases inexpressible — Qwen2.5 carries one on each of q, k and v — and the usual workaround
/// (a constant lane in the activation row, a bias column in the weight) needs a row with that
/// lane in it, which no BASE-0 op produces: `rms_norm` returns exactly its input width, and a
/// constant prepended before the norm would be summed into the scale.
///
/// A zero point is the smaller of the two available amendments: it changes one op instead of
/// adding an eleventh to a set whose closedness is the class's selling point, and it is what
/// asymmetric int8 quantization normally carries anyway — its absence was the anomaly.
///
/// **The saturation order is the specification, not an implementation detail.** The zero is added
/// BEFORE the clamp, in `i32`, so a bias that would push a value past the int8 range saturates
/// exactly as any other overflow does. Adding after the clamp would let the sum leave the range,
/// and a value outside `[-128, 127]` is not an int8 activation.
///
/// `zero = 0` reproduces [`requantize`] bit for bit, which is what keeps every existing BASE-0
/// class — the RC's liveness floor included — byte-identical across this amendment.
#[inline]
pub fn requantize_with_zero(acc: i32, multiplier: i32, shift: u8, zero: i32) -> i8 {
    rounding_shift_right(srdhm(acc, multiplier), shift).saturating_add(zero).clamp(-128, 127) as i8
}

/// [`rounding_shift_right`] at 64 bits — the same round-half-away-from-zero rule, one rule at two
/// widths. (It is NOT the only rounding rule in the class: [`srdhm`] rounds half UP, and the
/// internal `>> K` steps floor. ADR-0040 C1 enumerates them.)
#[inline]
pub fn rounding_shift_right_64(x: i64, s: u8) -> i64 {
    debug_assert!(s <= 62);
    if s == 0 {
        return x;
    }
    // Same magnitude-then-sign construction as the 32-bit rule, and for the same reason: the
    // shift form is wrong on every negative input.
    //
    // Widened to `i128`, and that is not belt-and-braces. `|x| + 2^(s-1)` overflows `i64` for `x`
    // near the type's ends at large `s`, and `x.abs()` overflows outright at `i64::MIN`. An
    // earlier version of this function did the arithmetic in `i64` and justified it by what
    // `rescale_q` passes in — but this is a public total function, and a panic reachable from one
    // is the remote-halt failure mode `palw_base0_ops` refuses by construction. The second
    // implementation found it on its first run.
    let (divisor, half) = (1i128 << s, 1i128 << (s - 1));
    let magnitude = (x as i128).abs();
    let rounded = (magnitude + half) / divisor;
    (if x < 0 { -rounded } else { rounded }) as i64
}

/// The largest right shift [`rescale_q`] accepts. `acc · multiplier` occupies at most 62 bits, so
/// shifting further can only produce 0 or −1 and is refused as a caller error rather than
/// silently returning it.
pub const RESCALE_MAX_SHIFT: u8 = 62;

/// `acc · multiplier · 2^−shift`, saturating into `i32` (ADR-0040 H op 9).
///
/// # Why this exists and [`requantize`] could not do it
///
/// `requantize` is `RoundingShiftRight(SRDHM(acc, mult), shift)`, and `SRDHM` *contains* a `>> 31`.
/// With `multiplier ≤ i32::MAX` and `shift ≥ 0`, that composition can only ever **attenuate**:
/// its gain is at most 1. But `IntExp`'s two consumers — `SoftMax` and `Silu` — are defined on Qk
/// inputs, and the accumulators that feed them do not reach Qk. Measured on random `int8` rows:
/// an attention logit over `d_head = 64` lands near 3.6e4, which is **0.002** in Qk, and an FFN
/// pre-activation over `d_model = 2048` lands near 1.8e5, which is **0.011**. At those magnitudes
/// `SoftMax` returns 0.1248…0.1255 against a uniform 0.125 — attention is flat, the keys are
/// indistinguishable — and `IntSigmoid` returns 0.501, so `Silu` degenerates to the linear `x/2`
/// and the SwiGLU gate stops gating. A class whose attention and gating are both inoperative can
/// still be executed and audited; it simply cannot compute anything. So the gap was structural,
/// not cosmetic.
///
/// The fix is to stop composing two shifts and do the multiply-and-shift **once** in `i64`. The
/// `>> 31` is then no longer baked in, so a `shift` below 31 is amplification and one at 31 is
/// unity gain — the full range without a new arithmetic concept: an `i64` multiply and one
/// rounding shift, both already in the catalog.
///
/// # This is deliberately NOT `requantize` without the clamp
///
/// `requantize` rounds twice (inside `SRDHM` at bit 31, then again at `shift`); this rounds once.
/// They therefore differ by up to one unit and are **not** interchangeable. `requantize` keeps its
/// exact frozen behaviour on the `int8` narrowing path — re-expressing it through this function
/// would change the value of every already-pinned narrowing.
#[inline]
pub fn rescale_q(acc: i32, multiplier: i32, shift: u8) -> i32 {
    debug_assert!(shift <= RESCALE_MAX_SHIFT);
    let shift = shift.min(RESCALE_MAX_SHIFT);
    let product = (acc as i64) * (multiplier as i64);
    rounding_shift_right_64(product, shift).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// `Poly2(p) = A·(p + B)² + C` on `p ∈ (−LN2_Q, 0]`, Qk in and out.
#[inline]
fn poly2(p: i32) -> i32 {
    let t = p as i64 + POLY2_B;
    let square = (t * t) >> K;
    (((POLY2_A * square) >> K) + POLY2_C) as i32
}

/// `exp(x)` for `x ≤ 0`, Qk in and out (ADR-0040 F1).
///
/// Softmax subtracts the row max first, so `x ≤ 0` always holds — that subtraction is part of the
/// op, not an optimisation. A positive input is clamped to 0 rather than being undefined, because
/// an undefined case is a place two implementations can differ.
#[inline]
pub fn int_exp(x: i32) -> i32 {
    let x = x.min(0);
    let z = (-(x as i64) / LN2_Q as i64).min(Z_MAX as i64) as i32;
    if z >= Z_MAX {
        // Below the Qk floor: exp(−31·ln2) ≈ 4.6e−10 against a resolution of 2^−24 ≈ 6e−8.
        return 0;
    }
    rounding_shift_right(poly2(x + z * LN2_Q), z as u8)
}

/// `1/√v` for `v > 0`, Qk in and out (ADR-0040 F2). Returns 0 for `v ≤ 0` — the norm paths add
/// `eps` before calling, so a non-positive input is already a caller error rather than a value to
/// invent an answer for.
pub fn int_rsqrt(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    // Normalize v = m · 2^(2e) with m ∈ [1, 4), so the seed table indexes a fixed range.
    let bit = 63 - v.leading_zeros() as i32;
    let mut e = (bit - K as i32).div_euclid(2);
    // `div_euclid` floors toward negative infinity, which is what keeps m in [1,4) for v < 1.
    let mut m = if 2 * e >= 0 { v >> (2 * e) } else { v << (-2 * e) };
    // Guard the boundary: rounding can leave m one step outside [1,4) for extreme inputs.
    while m >= 4 * ONE {
        m >>= 2;
        e += 1;
    }
    while m < ONE {
        m <<= 2;
        e -= 1;
    }
    let index = (((m - ONE) * 16) / (3 * ONE)).clamp(0, 15) as usize;
    let mut y = RSQRT_SEED[index];
    for _ in 0..RSQRT_ITERS {
        let y2 = (y * y) >> K;
        let my2 = (m * y2) >> K;
        y = (y * (3 * ONE - my2)) >> (K + 1);
        if y <= 0 {
            y = 1;
        }
    }
    if e >= 0 { y >> e } else { y << (-e) }
}

/// `1/v` for `v > 0`, Qk in and out — ADR-0040 F2's composed form, `1/v = (1/√v)²`.
///
/// Composed rather than given its own Newton iteration so the class keeps ONE reciprocal
/// algorithm. ADR-0040 admits either; what it forbids is drifting between them, so the choice is
/// made here and frozen.
///
/// This is the only division-shaped operation applied to **data**. Division by a graph constant
/// (a row length, `LN2_Q`) stays a plain integer division: it is exact, deterministic, and the
/// divisor is frozen at registration. Division by a value that came from the activations is the
/// case that needs a pinned algorithm, and it gets one.
#[inline]
pub fn int_recip(v: i64) -> i64 {
    // `r = int_rsqrt(v)` can be as large as ~2^36 (at v = 1, the smallest in-domain input), so
    // `r * r` reaches ~2^72 and overflows `i64` — a release panic under `overflow-checks = true`
    // for EVERY v in 1..=511, on a function whose declared domain is v > 0. The final `>> K`
    // result always fits `i64` (`1/v` in Qk is at most 2^48), so the fix is to widen the square to
    // `i128` and narrow back. `misaka-palw-base0-ref2` already computes it this way; this makes the
    // spec total on its own domain and keeps the two bit-identical there.
    let r = int_rsqrt(v) as i128;
    ((r * r) >> K) as i64
}

#[cfg(test)]
mod tests {
    /// **The zero point carries a projection bias, and `zero = 0` changes nothing (G2).**
    ///
    /// BASE-0 had no additive registered term in any of its ten ops, which makes a real
    /// transformer's q/k/v biases inexpressible — and the usual workaround needs an activation row
    /// with a constant lane, which no BASE-0 op produces. So `Requantize` gained the term the
    /// standard int8 form always had.
    ///
    /// Two properties matter and both are asserted here: the amendment is INERT at zero, so every
    /// class registered before it — the RC's liveness floor included — is byte-identical across
    /// it; and the addition happens before the int8 saturation, so a bias that would push a value
    /// out of range saturates like any other overflow instead of escaping it.
    #[test]
    fn the_zero_point_is_a_bias_and_is_inert_at_zero() {
        // Inert at zero, across the interesting corners of the accumulator space.
        for acc in [0i32, 1, -1, 1_000, -1_000, i32::MAX, i32::MIN, i32::MAX / 3, i32::MIN / 3] {
            for (m, s) in [(i32::MAX, 0u8), (i32::MAX, 10), (1 << 30, 5), (1, 31)] {
                assert_eq!(
                    requantize_with_zero(acc, m, s, 0),
                    requantize(acc, m, s),
                    "zero = 0 must reproduce the pre-amendment result at acc={acc}, m={m}, s={s}"
                );
            }
        }

        // It really adds. A value that requantizes to 0 lands on the bias.
        assert_eq!(requantize_with_zero(0, i32::MAX, 0, 7), 7);
        assert_eq!(requantize_with_zero(0, i32::MAX, 0, -7), -7);
        // …and it composes with the value rather than replacing it. The accumulator is chosen so
        // the sum stays inside the range: `(base + 5) as i8` would WRAP where the function
        // saturates, which is the difference this test is about and not a place to be sloppy.
        let base = requantize(100_000, i32::MAX, 10) as i32;
        assert!((-120..120).contains(&base), "the fixture must leave headroom, got {base}");
        assert_eq!(requantize_with_zero(100_000, i32::MAX, 10, 5) as i32, base + 5);

        // **Before the clamp, not after.** A bias past the int8 range saturates; it does not
        // wrap, and it does not leave the range. An i8 that is not in [-128, 127] is not an i8.
        assert_eq!(requantize_with_zero(0, i32::MAX, 0, 10_000), 127);
        assert_eq!(requantize_with_zero(0, i32::MAX, 0, -10_000), -128);
        assert_eq!(requantize_with_zero(i32::MAX, i32::MAX, 0, 127), 127, "already at the ceiling, and it stays");
        assert_eq!(requantize_with_zero(i32::MIN, i32::MAX, 0, -127), -128);
        // The additive step itself cannot overflow into a wrap either.
        assert_eq!(requantize_with_zero(0, i32::MAX, 0, i32::MAX), 127);
        assert_eq!(requantize_with_zero(0, i32::MAX, 0, i32::MIN), -128);
    }

    use super::*;

    /// Qk → a rational check without floats: assert `value` is within `tol_num/tol_den` of
    /// `want_num/want_den`, in integer arithmetic only.
    fn close(value: i64, want_num: i64, want_den: i64, tol_ppm: i64) -> bool {
        // |value·want_den − want_num·ONE| · 1e6 ≤ tol_ppm · |want_num·ONE|
        let lhs = (value * want_den - want_num * ONE).abs();
        let rhs = want_num.abs() * ONE;
        lhs * 1_000_000 <= tol_ppm * rhs
    }

    /// The constants are the pinned derivations, checked in integers so the module never needs a
    /// float expression to justify itself.
    #[test]
    fn constants_are_the_pinned_derivations() {
        assert_eq!(ONE, 16_777_216);
        assert_eq!(MAX_DOT_LEN as i64, (i32::MAX as i64) / 16_384);
        // ln2 in Qk, bracketed: 0.693147 < LN2_Q/ONE < 0.693148
        assert!((LN2_Q as i64) * 1_000_000 > 693_147 * ONE);
        assert!((LN2_Q as i64) * 1_000_000 < 693_148 * ONE);
        assert_eq!(RSQRT_SEED.len(), 16);
        // The seed table is monotonically decreasing: bucket upper ends increase.
        for w in RSQRT_SEED.windows(2) {
            assert!(w[0] > w[1], "seeds must decrease across buckets");
        }
    }

    /// **The endpoints that distinguish the shifted-square form from the Horner reading.**
    ///
    /// `Poly2(0) ≈ 1.0` and `Poly2(−ln2) ≈ 0.5` are the two values `exp` must hit. Reading
    /// `A, B, C` as Horner coefficients gives `Poly2(0) = 0.344` — this test is what catches that,
    /// and it is the regression for the error ADR-0040 F1 records.
    #[test]
    fn poly2_hits_the_two_endpoints_exp_must() {
        assert!(close(poly2(0) as i64, 1, 1, 1_000), "poly2(0) must be 1.0, not 0.344");
        assert!(close(poly2(-LN2_Q) as i64, 1, 2, 1_000), "poly2(-ln2) must be 0.5");
    }

    /// `int_exp` against known values, to a tolerance ADR-0040 F1 measured (0.5 %).
    #[test]
    fn int_exp_matches_known_values() {
        assert!(close(int_exp(0) as i64, 1, 1, 5_000), "exp(0) = 1");
        assert!(close(int_exp(-LN2_Q) as i64, 1, 2, 5_000), "exp(-ln2) = 1/2");
        assert!(close(int_exp(-2 * LN2_Q) as i64, 1, 4, 5_000), "exp(-2ln2) = 1/4");
        assert!(close(int_exp(-10 * LN2_Q) as i64, 1, 1024, 5_000), "exp(-10ln2) = 1/1024");
        // Monotone non-increasing across the whole domain, and never negative.
        let mut previous = int_exp(0);
        for step in 1..4_000i64 {
            // i64 intermediate: `step · LN2_Q` passes i32::MAX well before the sweep ends.
            let value = int_exp((-(step * LN2_Q as i64) / 64) as i32);
            assert!(value <= previous, "exp must not increase as x decreases (step {step})");
            assert!(value >= 0, "exp is never negative");
            previous = value;
        }
        // Far below the Qk floor is exactly zero, not garbage.
        assert_eq!(int_exp(i32::MIN), 0);
        // A positive input is clamped, never undefined.
        assert_eq!(int_exp(1_000_000), int_exp(0));
    }

    /// **The seed-basin regression.** Newton for `1/√v` diverges to ZERO from a seed above
    /// `√(3/m)`, and `m = 3` is exactly where a leading-bit-only seed lands. ADR-0040 F2.
    #[test]
    fn int_rsqrt_does_not_collapse_at_the_basin_boundary() {
        // v = 3.0 — the input that returned 0 before the seed table existed.
        let y = int_rsqrt(3 * ONE);
        assert!(y > 0, "rsqrt(3) must not be zero — that is the divergence-to-zero failure");
        assert!(close(y, 57_735, 100_000, 10_000), "1/sqrt(3) ≈ 0.57735");
    }

    /// Accuracy and range: ADR-0040 F2 measured 6.4e−6 at N = 3.
    #[test]
    fn int_rsqrt_is_accurate_across_the_range() {
        // 1/sqrt(1) = 1, 1/sqrt(4) = 1/2, 1/sqrt(16) = 1/4, 1/sqrt(1/4) = 2.
        assert!(close(int_rsqrt(ONE), 1, 1, 1_000));
        assert!(close(int_rsqrt(4 * ONE), 1, 2, 1_000));
        assert!(close(int_rsqrt(16 * ONE), 1, 4, 1_000));
        assert!(close(int_rsqrt(ONE / 4), 2, 1, 1_000));
        // Never zero and monotonically non-increasing over a wide sweep.
        let mut previous = i64::MAX;
        for step in 1..20_000i64 {
            let v = step * ONE / 100;
            let y = int_rsqrt(v);
            assert!(y > 0, "rsqrt must never collapse to zero (v = {v})");
            assert!(y <= previous, "rsqrt must not increase as v increases (step {step})");
            previous = y;
        }
    }

    /// ADR-0040 C1/C2: the two lossy/fixed-point primitives, at their defining cases.
    #[test]
    fn shift_and_multiply_round_and_saturate_as_specified() {
        // Round half AWAY from zero, symmetric about zero.
        assert_eq!(rounding_shift_right(3, 1), 2);
        assert_eq!(rounding_shift_right(-3, 1), -2);
        assert_eq!(rounding_shift_right(1, 1), 1);
        assert_eq!(rounding_shift_right(-1, 1), -1);
        assert_eq!(rounding_shift_right(7, 0), 7);
        // The four assertions above are ALL exact halves, and the defective `(x ± h) >> s` form
        // agrees with the correct one on every one of them — which is why this test passed while
        // half of all inputs were wrong. What separates the two forms is a negative input that
        // needs NO rounding, because there the shift form still subtracts and still floors.
        assert_eq!(rounding_shift_right(-64, 1), -32, "an exact quotient must not be rounded at all");
        assert_eq!(rounding_shift_right(-1024, 4), -64);
        assert_eq!(rounding_shift_right(-100, 2), -25);
        // Symmetry, stated as the property rather than as a list: the rule is defined on the
        // magnitude, so no input may disagree with its own negation.
        for x in [-4096i32, -1000, -257, -128, -7, -2, 0, 2, 7, 128, 257, 1000, 4096] {
            for s in 0..=12u8 {
                assert_eq!(
                    rounding_shift_right(x, s),
                    -rounding_shift_right(-x, s),
                    "RSR is not symmetric at ({x}, {s}) — the rounding rule has a sign branch"
                );
            }
        }
        // Total at the ends: an earlier version overflowed `i32` here, and at `i64::MIN` for the
        // 64-bit rule, both of which are reachable from a public function.
        assert_eq!(rounding_shift_right(i32::MAX, 31), 1);
        assert_eq!(rounding_shift_right(i32::MIN, 31), -1);
        assert_eq!(rounding_shift_right_64(i64::MIN, 62), -2);
        assert_eq!(rounding_shift_right_64(i64::MAX, 62), 2);
        // SRDHM on negatives, which is the half of the input space that was wrong. The nudge is
        // asymmetric to compensate for a TRUNCATING division; pairing it with a floor double-counts.
        assert_eq!(srdhm(-(1 << 30), 1 << 30), -(1 << 29), "-0.5 x 0.5 = -0.25 exactly, no rounding");
        assert_eq!(srdhm(-1, 1), 0, "a product far below the resolution rounds to zero, not away from it");
        assert_eq!(srdhm(1, -1), 0);
        assert_eq!(srdhm(-(1 << 20), 1 << 20), -512);
        // SRDHM rounds half UP (toward +∞), NOT half-away — ADR-0040 C1. Pin it on the negative
        // exact-half products, where the two rules diverge. `-(2m+1)·2^30 / 2^31 = -(2m+1)/2` is an
        // exact half; half-up gives -m, half-away gives -(m+1). These freeze the gemmlowp rule so a
        // "correction" toward symmetry (which would convict honest gemmlowp-based third parties)
        // fails here rather than silently.
        assert_eq!(srdhm(-1, 1 << 30), 0, "-1/2 rounds UP to 0, not away to -1");
        assert_eq!(srdhm(-3, 1 << 30), -1, "-3/2 rounds UP to -1, not away to -2");
        assert_eq!(srdhm(-5, 1 << 30), -2, "-5/2 rounds UP to -2, not away to -3");
        // The positive exact-halves round the same way under both rules (up == away for x>0), so
        // they must NOT change: this is what makes the divergence negative-only.
        assert_eq!(srdhm(1, 1 << 30), 1, "1/2 rounds to 1 under both rules");
        assert_eq!(srdhm(3, 1 << 30), 2, "3/2 rounds to 2 under both rules");
        // SRDHM is `(a·b) >> 31`. These two identities pin the factor: an extra 2 makes the
        // first return 0.5 and the second overflow i32 (the bug this test caught).
        assert_eq!(srdhm(1 << 30, 1 << 30), 1 << 29, "0.5 x 0.5 = 0.25 in Q31");
        let one_q31 = i32::MAX;
        assert!(srdhm(one_q31, one_q31) >= one_q31 - 2, "~1.0 x ~1.0 stays in range");
        assert_eq!(srdhm(i32::MIN, i32::MIN), i32::MAX);
        assert_eq!(srdhm(0, 12345), 0);
        // The declared domain 0..=31 keeps its exact values — the §2.3 clamp must not disturb it.
        assert_eq!(rounding_shift_right(1 << 20, 31), 0);
        assert_eq!(rounding_shift_right(1 << 20, 10), 1024);
        assert_eq!(rounding_shift_right(-(1 << 20), 10), -1024);
        // Out-of-domain shifts are covered by `the_shift_clamp_makes_release_total` below, which is
        // release-only because in debug the `debug_assert` is deliberately the louder contract.
        // Requantize saturates rather than wrapping.
        assert_eq!(requantize(i32::MAX, i32::MAX, 0), 127);
        assert_eq!(requantize(i32::MIN, i32::MAX, 0), -128);
    }

    /// **ADR-0040 Decision E, pinned: reduction order cannot change a result.**
    ///
    /// The property the whole class is built for. Summed forward, backward, and in interleaved
    /// halves — the three shapes a threaded or SIMD kernel actually produces — every order gives
    /// the identical accumulator, because integer addition is exactly associative within the
    /// no-overflow bound.
    #[test]
    fn reduction_order_cannot_change_an_accumulator() {
        let a: Vec<i32> = (0..4_096).map(|i| ((i * 37) % 255) - 127).collect();
        let b: Vec<i32> = (0..4_096).map(|i| ((i * 101) % 255) - 127).collect();

        let forward: i32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        let backward: i32 = a.iter().zip(&b).rev().map(|(x, y)| x * y).sum();
        let products: Vec<i32> = a.iter().zip(&b).map(|(x, y)| x * y).collect();
        let even: i32 = products.iter().step_by(2).sum();
        let odd: i32 = products.iter().skip(1).step_by(2).sum();
        let interleaved: i32 = even + odd;

        assert_eq!(forward, backward, "reversal must not change the sum");
        assert_eq!(forward, interleaved, "splitting into lanes must not change the sum");
    }

    /// The bound that licenses the property above: `MAX_DOT_LEN` worst-case products still fit an
    /// `i32`, and one more does not. Decision E is conditional on this, so it is pinned.
    ///
    /// **The worst case is asserted to be `i8::MIN`, not assumed to be `i8::MAX`.** This test read
    /// `127 * 127` while [`requantize`] was already emitting `-128`, so it pinned the bound against
    /// a subset of the operand type and passed. The first two assertions are the ones that make the
    /// rest mean something: the wide end is reachable, and it is the negative one.
    #[test]
    fn the_no_overflow_bound_is_exactly_at_the_edge() {
        assert_eq!(requantize(i32::MIN, i32::MAX, 0), i8::MIN, "-128 is a value this class produces");
        let worst = (i8::MIN as i64) * (i8::MIN as i64);
        assert_eq!(worst, 16_384);
        assert!(worst > (i8::MAX as i64) * (i8::MAX as i64), "the negative end is the wider one");

        assert!(MAX_DOT_LEN as i64 * worst <= i32::MAX as i64, "the bound must fit");
        assert!((MAX_DOT_LEN as i64 + 1) * worst > i32::MAX as i64, "and must be the largest that does");
    }

    /// ADR-0040 C1 / audit §2.3: `rounding_shift_right` is TOTAL in release for every `u8` shift.
    ///
    /// Two contracts by build mode, and this pins the release one. In debug the `debug_assert!(s <=
    /// 31)` is the contract — a caller passing an out-of-domain shift is a bug and must be loud. In
    /// release the clamp is the contract, because the refutation path decodes `shift` from an
    /// unvalidated oracle byte (`palw_step_refute`, 0..=255) and a `pub` arithmetic function on
    /// peer-influenced input must not panic: without `s.min(31)`, `1i64 << s` overflows for s >= 64
    /// under `overflow-checks = true`, which the release profile sets.
    ///
    /// Release-only for that reason, and it matters that it exists at all: an adversarial verifier
    /// deleted the clamp and found every suite green, i.e. the fix was correct and entirely
    /// uncovered. Run with `cargo test --release -p kaspa-consensus-core`.
    #[test]
    #[cfg(not(debug_assertions))]
    fn the_shift_clamp_makes_release_total() {
        for s in [32u8, 63, 64, 127, 200, 255] {
            // Past the Qk resolution an in-range accumulator shifts away to nothing; the requirement
            // is that it RETURNS that rather than panicking.
            assert_eq!(rounding_shift_right(1 << 20, s), 0, "shift {s} must be total, not a panic");
            assert_eq!(rounding_shift_right(-(1 << 20), s), 0, "shift {s} must be total for negatives too");
            // Every out-of-domain shift collapses onto s = 31, the domain's edge.
            assert_eq!(rounding_shift_right(i32::MAX, s), rounding_shift_right(i32::MAX, 31));
            assert_eq!(rounding_shift_right(i32::MIN, s), rounding_shift_right(i32::MIN, 31));
        }
    }

    /// The 64-bit shift obeys the same round-half-away-from-zero rule as the 32-bit one. If it did
    /// not, ADR-0040 C1 would be describing two rounding rules while claiming one.
    #[test]
    fn the_64_bit_shift_rounds_like_the_32_bit_one() {
        for x in [-9i64, -8, -7, -5, -4, -3, -1, 0, 1, 3, 4, 5, 7, 8, 9, 1 << 40, -(1 << 40)] {
            for s in [0u8, 1, 2, 3, 31] {
                if let Ok(x32) = i32::try_from(x)
                    && s <= 31
                {
                    assert_eq!(rounding_shift_right_64(x, s), rounding_shift_right(x32, s) as i64, "widths disagree at x={x} s={s}");
                }
            }
        }
        // Half rounds AWAY from zero on both signs — the property that makes the rule symmetric.
        assert_eq!(rounding_shift_right_64(3, 1), 2);
        assert_eq!(rounding_shift_right_64(-3, 1), -2);
    }

    /// The whole reason `rescale_q` was added: gain above 1 must be reachable. `requantize` cannot
    /// do this at any `(multiplier, shift)`, which is what left `SoftMax` and `Silu` unfeedable.
    #[test]
    fn rescale_can_amplify_and_requantize_cannot() {
        // Unity is shift 31, because the multiplier is a Q31 fraction.
        let unity = rescale_q(1_000_000, i32::MAX, 31);
        assert!((unity - 1_000_000).abs() <= 1, "shift 31 must be unity gain, got {unity}");
        // Shift below 31 amplifies, by exactly the power of two it is short by.
        assert!((rescale_q(1_000_000, i32::MAX, 24) - 128_000_000).abs() <= 128);
        assert!((rescale_q(100_000, i32::MAX, 21) - 102_400_000).abs() <= 1024);
        // And above 31 it still attenuates, so it is a strict generalisation rather than a swap.
        assert!((rescale_q(1_000_000, i32::MAX, 35) - 62_500).abs() <= 1);

        // The negative claim, exhaustively over the shift range: `requantize`'s gain never exceeds
        // 1, so no parameter choice could have fed a Qk consumer from a sub-Qk accumulator.
        for shift in 0..=31u8 {
            let out = requantize(100, i32::MAX, shift) as i32;
            assert!(out.abs() <= 100, "requantize amplified at shift {shift}: {out}");
        }
    }

    /// Amplification is what makes softmax discriminate. Without it the row below returns uniform;
    /// this pins the actual repair rather than only the arithmetic that enables it.
    #[test]
    fn amplification_restores_a_discriminating_softmax() {
        // Accumulator magnitudes measured from random int8 dots over d_head = 64.
        let logits = [-23_006i32, 74_627, -15_901, 23_366, 26_776, 17_070, -29_402, -26_712];
        let uniform = (ONE / logits.len() as i64) as i32;

        let flat: Vec<i32> = logits.iter().map(|l| crate::palw_base0_ops::softmax(&[*l]).unwrap()[0]).collect();
        assert!(!flat.is_empty());
        let raw = crate::palw_base0_ops::softmax(&logits).unwrap();
        let raw_spread = raw.iter().max().unwrap() - raw.iter().min().unwrap();
        assert!(raw_spread * 100 < uniform, "unscaled logits must be indistinguishable from uniform: {raw:?}");

        // Shift 23 is a gain of 2^8, which lifts these accumulators to O(1) in Qk.
        let scaled: Vec<i32> = logits.iter().map(|l| rescale_q(*l, i32::MAX, 23)).collect();
        let good = crate::palw_base0_ops::softmax(&scaled).unwrap();
        let good_spread = good.iter().max().unwrap() - good.iter().min().unwrap();
        assert!(good_spread > uniform, "amplified logits must discriminate: {good:?}");
        assert_eq!(
            good.iter().enumerate().max_by_key(|(_, p)| **p).unwrap().0,
            1,
            "the largest logit must take the largest probability"
        );
    }

    /// Saturation, not wrapping: an overflowing rescale pins at the `i32` ends. A wrap would turn
    /// the largest activation in a row into the most negative one.
    #[test]
    fn rescale_saturates_at_both_ends() {
        assert_eq!(rescale_q(i32::MAX, i32::MAX, 0), i32::MAX);
        assert_eq!(rescale_q(i32::MIN, i32::MAX, 0), i32::MIN);
        assert_eq!(rescale_q(i32::MAX, i32::MIN, 0), i32::MIN);
        // Zero is zero at every setting.
        for s in 0..=RESCALE_MAX_SHIFT {
            assert_eq!(rescale_q(0, i32::MAX, s), 0);
        }
    }

    /// `rescale_q` rounds once and `requantize` rounds twice, so they are close but NOT equal.
    /// Pinning the difference stops a later "simplification" that re-expresses one through the
    /// other and silently moves every already-frozen narrowing by a unit.
    #[test]
    fn rescale_is_not_requantize_without_the_clamp() {
        let mut differed = false;
        for acc in [3i32, 5, 7, 11, 13, 101, 12_345, -7, -13, -12_345] {
            for shift in [1u8, 2, 3] {
                let via_requantize = requantize(acc, i32::MAX, shift) as i32;
                let via_rescale = rescale_q(acc, i32::MAX, 31 + shift).clamp(-128, 127);
                assert!((via_requantize - via_rescale).abs() <= 1, "the two paths must stay within a unit");
                differed |= via_requantize != via_rescale;
            }
        }
        assert!(differed, "if these never differ the double-rounding note is wrong and should be deleted");
    }
}
