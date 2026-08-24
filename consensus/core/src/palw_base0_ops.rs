//! ADR-0040 Decision D: `PALW-BASE-0`'s nine ops, on the six primitives.
//!
//! Nine kinds against the float vocabulary's seventeen ([`crate::palw_step::PalwStepOpKindV1`]),
//! chosen for closability rather than for parity with the float classes' graph. Integerising
//! GatedDeltaNet, interleaved-multimodal RoPE and fused SwiGLU would reproduce the catalog problem
//! this class exists to escape.
//!
//! Two absences carry most of the value:
//!
//! * [`rope_table`] takes its rotations as **data**. The angles depend only on
//!   (position, dimension), both bounded by the registered shape, so the table is precomputed once
//!   and pinned like the weights. A transcendental evaluated at registration is an artifact; the
//!   same transcendental evaluated at inference is normative arithmetic every implementation must
//!   reproduce. This is what removes `sinf`/`cosf` from the class entirely.
//! * There is no `CpyF32F16`, because no cache holds floats.
//!
//! # Every op is total, and nothing panics
//!
//! Shape errors are [`PalwBase0OpError`], not assertions. A panic reachable from block validation
//! is a remote chain-halt — the failure mode the 2026-08-17 audit found in the float PoW path
//! (B7) — so this module has none. The graph's shapes are fixed at registration, so a conforming
//! caller never sees these errors; they exist so a non-conforming one is refused rather than
//! crashing the node that refuses it.

use crate::palw_base0::{K, MAX_DOT_LEN, ONE, int_exp, int_recip, int_rsqrt, requantize_with_zero, rescale_q};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwBase0OpError {
    #[error(
        "dot length {got} exceeds MAX_DOT_LEN {MAX_DOT_LEN} — beyond it an i32 accumulator can overflow, and overflow costs the free reduction order (ADR-0040 C3/E)"
    )]
    DotTooLong { got: usize },
    #[error("operand lengths differ: {a} vs {b}")]
    LengthMismatch { a: usize, b: usize },
    #[error("length {got} is not a multiple of {unit} as this op requires")]
    NotAMultiple { got: usize, unit: usize },
    #[error("empty operand — an op over nothing has no defined result")]
    Empty,
    #[error("token id {got} is outside the embedding table's {rows} rows")]
    TokenOutOfRange { got: usize, rows: usize },
}

/// Per-tensor (or per-channel) requantization parameters, frozen at registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuantParams {
    pub multiplier: i32,
    pub shift: u8,
    /// **The additive term (ADR-0040 amendment, G2).** Added in `i32` after the shift and BEFORE
    /// the int8 saturation, which is what makes a projection bias expressible in a class that had
    /// no additive registered term anywhere. `0` is the pre-amendment behaviour, bit for bit.
    pub zero: i32,
}

/// Scale-change parameters for [`rescale_row`], frozen at registration.
///
/// The gain is `multiplier · 2^−shift`. Unity is `shift = 31` with `multiplier = i32::MAX`,
/// because the multiplier is read as a Q31 fraction; **below 31 is amplification**. That is the
/// whole difference from [`QuantParams`], whose gain cannot exceed 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleParams {
    pub multiplier: i32,
    pub shift: u8,
}

impl ScaleParams {
    /// The shift that leaves a value unchanged.
    pub const UNITY_SHIFT: u8 = 31;

    /// A gain of `2^bits`, as a shift off unity. Refuses a gain the shift range cannot express
    /// rather than clamping to a different gain than the caller asked for.
    pub fn gain_pow2(bits: i8) -> Option<Self> {
        let shift = Self::UNITY_SHIFT as i16 - bits as i16;
        if !(0..=crate::palw_base0::RESCALE_MAX_SHIFT as i16).contains(&shift) {
            return None;
        }
        Some(Self { multiplier: i32::MAX, shift: shift as u8 })
    }
}

// -------------------------------------------------------------------------------------------
// 0. EmbedLookup — row gather, no arithmetic
// -------------------------------------------------------------------------------------------

/// Op 0: the embedding row for `token_id`. No arithmetic at all, which is why it is the one op
/// with nothing to differential-test beyond its bounds.
pub fn embed_lookup(table: &[i8], rows: usize, dim: usize, token_id: usize) -> Result<&[i8], PalwBase0OpError> {
    if dim == 0 || rows == 0 {
        return Err(PalwBase0OpError::Empty);
    }
    if table.len() != rows * dim {
        return Err(PalwBase0OpError::LengthMismatch { a: table.len(), b: rows * dim });
    }
    if token_id >= rows {
        return Err(PalwBase0OpError::TokenOutOfRange { got: token_id, rows });
    }
    Ok(&table[token_id * dim..(token_id + 1) * dim])
}

// -------------------------------------------------------------------------------------------
// 1. MatMulQuant — exact int8 × int8 → i32
// -------------------------------------------------------------------------------------------

/// Op 1: one exact `int8 × int8 → i32` dot product.
///
/// No rounding, no saturation, no accumulation order: within [`MAX_DOT_LEN`] the sum cannot
/// overflow, so it is exactly associative and every kernel shape produces the identical `i32`
/// (ADR-0040 E). That is the property, and the length check is its premise rather than a
/// defensive nicety.
pub fn dot_i8(a: &[i8], b: &[i8]) -> Result<i32, PalwBase0OpError> {
    if a.len() != b.len() {
        return Err(PalwBase0OpError::LengthMismatch { a: a.len(), b: b.len() });
    }
    if a.len() > MAX_DOT_LEN {
        return Err(PalwBase0OpError::DotTooLong { got: a.len() });
    }
    let mut acc: i32 = 0;
    for (x, y) in a.iter().zip(b) {
        acc += (*x as i32) * (*y as i32);
    }
    Ok(acc)
}

/// Op 1, batched: `out[i] = dot(weights_row_i, x)`. `weights` is row-major `[out_dim × in_dim]`.
pub fn matmul_quant(weights: &[i8], x: &[i8], out_dim: usize) -> Result<Vec<i32>, PalwBase0OpError> {
    if x.is_empty() || out_dim == 0 {
        return Err(PalwBase0OpError::Empty);
    }
    if weights.len() != out_dim * x.len() {
        return Err(PalwBase0OpError::LengthMismatch { a: weights.len(), b: out_dim * x.len() });
    }
    let in_dim = x.len();
    (0..out_dim).map(|row| dot_i8(&weights[row * in_dim..(row + 1) * in_dim], x)).collect()
}

// -------------------------------------------------------------------------------------------
// 2. Requantize — the explicit narrowing
// -------------------------------------------------------------------------------------------

/// Op 2: `i32` accumulators → `int8`, per-channel. The narrowing is an op rather than an implicit
/// cast because it is the only place besides the shifts inside the primitives where information
/// is lost, and an implicit one is a place two implementations can differ.
pub fn requantize_row(acc: &[i32], params: &[QuantParams]) -> Result<Vec<i8>, PalwBase0OpError> {
    if acc.len() != params.len() {
        return Err(PalwBase0OpError::LengthMismatch { a: acc.len(), b: params.len() });
    }
    Ok(acc.iter().zip(params).map(|(a, q)| requantize_with_zero(*a, q.multiplier, q.shift, q.zero)).collect())
}

/// Op 2, per-tensor: one `QuantParams` for the whole row.
pub fn requantize_row_uniform(acc: &[i32], params: QuantParams) -> Vec<i8> {
    acc.iter().map(|a| requantize_with_zero(*a, params.multiplier, params.shift, params.zero)).collect()
}

// -------------------------------------------------------------------------------------------
// 9. Rescale — the scale change that is allowed to amplify (ADR-0040 H)
// -------------------------------------------------------------------------------------------

/// Op 9: `i32` accumulators → `i32` at a different scale, per-tensor, **gain unbounded above**.
///
/// This is the op that lets an accumulator reach the Qk domain [`softmax`] and [`silu`] are
/// defined on. [`requantize_row_uniform`] cannot: its gain is at most 1 at every parameter, so
/// before this op existed an attention logit arrived at `softmax` around 0.002 in Qk and the
/// result was uniform to four decimals — attention was flat and SwiGLU's gate was the linear
/// `x/2`. See `palw_base0::rescale_q` for the measurements and for why this is not simply
/// `Requantize` with the clamp removed.
pub fn rescale_row(acc: &[i32], params: ScaleParams) -> Vec<i32> {
    acc.iter().map(|a| rescale_q(*a, params.multiplier, params.shift)).collect()
}

// -------------------------------------------------------------------------------------------
// 3. RmsNorm — scale-invariant, so it needs no input scale at all
// -------------------------------------------------------------------------------------------

/// Op 3: RMS normalization in the quantized domain, returning Qk values.
///
/// **The input scale cancels and is therefore not a parameter.** `x / rms(x)` is invariant under
/// `x → c·x`, so the whole op runs on the raw `int8` codes and only the OUTPUT scale matters. One
/// fewer registration constant is one fewer thing two implementations can disagree about, and the
/// cancellation is exact here in a way it is not in floating point.
///
/// `eps_q` is Qk and is added to the mean of squares before the reciprocal square root, so a
/// zero row is defined rather than a division by zero.
pub fn rms_norm(x: &[i8], eps_q: i64) -> Result<Vec<i32>, PalwBase0OpError> {
    if x.is_empty() {
        return Err(PalwBase0OpError::Empty);
    }
    if x.len() > MAX_DOT_LEN {
        return Err(PalwBase0OpError::DotTooLong { got: x.len() });
    }
    // Exact sum of squares in i64: 128² × MAX_DOT_LEN is far inside the range.
    let sum_squares: i64 = x.iter().map(|v| (*v as i64) * (*v as i64)).sum();
    // Mean of squares in Qk. The divisor is a graph constant (the row length), so a plain integer
    // division is exact and frozen — see `int_recip`'s note on which divisions need an algorithm.
    let mean_q = (sum_squares << K) / (x.len() as i64);
    let r = int_rsqrt(mean_q + eps_q);
    // `x_i` is a plain integer code and `r` is Qk, so the product is already Qk — a further
    // `>> K` would divide every output by 2^24 and collapse the row to a handful of units,
    // which is precisely what an earlier draft did and what a too-loose test let through.
    //
    // SATURATE the narrowing, do not `as i32`. For a sparse row (one large code, the rest zero) the
    // mean of squares shrinks with the row width, so `r = 1/rms` grows: at the widest legal row a
    // 127 code times `r` reaches ~4.3e9, past `i32::MAX`.
    //
    // The product itself is safe — `r` is bounded by ~2^36 and the code by 127, so `i64` holds it
    // with ~20 bits to spare and there is no overflow to trap. The defect was purely the `as i32`,
    // which reduces mod 2^32 and so reports a WRONG VALUE: 4_328_781_792 came back as 33_814_496,
    // a 128× understatement of the largest activation in the row (and, for a product landing in
    // [2^31, 2^32), a sign flip instead). Silent and unbounded either way. ADR-0040 C3 is explicit
    // that nothing wraps anywhere; every narrowing saturates.
    Ok(x.iter().map(|v| ((*v as i64) * r).clamp(i32::MIN as i64, i32::MAX as i64) as i32).collect())
}

// -------------------------------------------------------------------------------------------
// 4. RopeTable — rotation by pinned integers, no sinf/cosf anywhere
// -------------------------------------------------------------------------------------------

/// Op 4: rotary position embedding by a **pinned** cos/sin table, applied to adjacent pairs.
///
/// `cos_q` and `sin_q` are Qk and are registration artifacts, one entry per pair. The angles are a
/// function of (position, dimension) alone, both bounded by the registered shape — so they are
/// computed once, hashed, and shipped, exactly like the weights. This is the single change that
/// removes `sinf`/`cosf` from the class, and with them ADR-0031's hardest surface.
pub fn rope_table(x: &[i32], cos_q: &[i32], sin_q: &[i32]) -> Result<Vec<i32>, PalwBase0OpError> {
    if x.is_empty() {
        return Err(PalwBase0OpError::Empty);
    }
    if !x.len().is_multiple_of(2) {
        return Err(PalwBase0OpError::NotAMultiple { got: x.len(), unit: 2 });
    }
    let pairs = x.len() / 2;
    if cos_q.len() != pairs {
        return Err(PalwBase0OpError::LengthMismatch { a: cos_q.len(), b: pairs });
    }
    if sin_q.len() != pairs {
        return Err(PalwBase0OpError::LengthMismatch { a: sin_q.len(), b: pairs });
    }
    let mut out = Vec::with_capacity(x.len());
    for pair in 0..pairs {
        // Widen to `i128` before multiplying. `a·s + b·c` is a sum of two `i32×i32` products, each
        // up to ~2^62, so in `i64` the sum reaches ~2^63 and overflows — a release panic under
        // overflow-checks — the instant a caller (e.g. the refutation path) hands this op arbitrary
        // `cos_q`/`sin_q` bytes rather than a bounded pinned table. `i128` cannot overflow here, and
        // the narrowing saturates (ADR-0040 C3: nothing wraps). On a real Qk rotation table every
        // value fits `i32` after `>> K`, so this is bit-identical on the valid domain.
        let (a, b) = (x[2 * pair] as i128, x[2 * pair + 1] as i128);
        let (c, s) = (cos_q[pair] as i128, sin_q[pair] as i128);
        out.push((((a * c) - (b * s)) >> K).clamp(i32::MIN as i128, i32::MAX as i128) as i32);
        out.push((((a * s) + (b * c)) >> K).clamp(i32::MIN as i128, i32::MAX as i128) as i32);
    }
    Ok(out)
}

// -------------------------------------------------------------------------------------------
// 5. SoftMax — max, exp, sum, reciprocal
// -------------------------------------------------------------------------------------------

/// Op 5: softmax over Qk logits, returning Qk probabilities that sum to approximately `ONE`.
///
/// The row max is subtracted first and that subtraction is **part of the op, not an
/// optimisation**: it is what makes every `int_exp` argument non-positive, which is the domain
/// the algorithm is defined on. Skipping it for a row that happens to be negative would be a
/// different function on a different domain.
pub fn softmax(logits_q: &[i32]) -> Result<Vec<i32>, PalwBase0OpError> {
    if logits_q.is_empty() {
        return Err(PalwBase0OpError::Empty);
    }
    let max = *logits_q.iter().max().expect("non-empty checked above");
    let exps: Vec<i64> = logits_q.iter().map(|v| int_exp(v.saturating_sub(max)) as i64).collect();
    let sum: i64 = exps.iter().sum();
    if sum <= 0 {
        // Unreachable for a real row (the max element contributes exp(0) ≈ ONE), but defined
        // rather than a division by zero: a uniform distribution is the only answer that keeps
        // the output a distribution.
        let uniform = ONE / (logits_q.len() as i64);
        return Ok(vec![uniform as i32; logits_q.len()]);
    }
    let recip = int_recip(sum);
    Ok(exps.iter().map(|e| ((e * recip) >> K) as i32).collect())
}

// -------------------------------------------------------------------------------------------
// 6. Silu — x · sigmoid(x), sharing int_exp with softmax
// -------------------------------------------------------------------------------------------

/// `sigmoid(x)` in Qk, defined on both signs through a single non-positive `int_exp` call.
///
/// `sigmoid(x) = E/(1+E)` for `x ≤ 0` and `1/(1+E)` for `x > 0`, where `E = exp(−|x|)`. Written
/// this way both branches evaluate `int_exp` on a non-positive argument, so the class needs no
/// second exponential and no positive-domain extension.
pub fn int_sigmoid(x_q: i32) -> i32 {
    let e = int_exp(-(x_q.saturating_abs())) as i64;
    let denominator = ONE + e;
    let recip = int_recip(denominator);
    let numerator = if x_q <= 0 { e } else { ONE };
    ((numerator * recip) >> K) as i32
}

/// Op 6: `SiLU(x) = x · sigmoid(x)`, Qk in and out.
pub fn silu(x_q: &[i32]) -> Vec<i32> {
    x_q.iter().map(|v| (((*v as i64) * (int_sigmoid(*v) as i64)) >> K) as i32).collect()
}

// -------------------------------------------------------------------------------------------
// 7/8. Elementwise
// -------------------------------------------------------------------------------------------

/// Op 7: elementwise multiply of two `int8` rows into `i32`, exact.
pub fn mul_elem(a: &[i8], b: &[i8]) -> Result<Vec<i32>, PalwBase0OpError> {
    if a.len() != b.len() {
        return Err(PalwBase0OpError::LengthMismatch { a: a.len(), b: b.len() });
    }
    Ok(a.iter().zip(b).map(|(x, y)| (*x as i32) * (*y as i32)).collect())
}

/// Op 8: elementwise add of two `int8` rows into `i32`, exact.
///
/// The scales are aligned at registration — there is no runtime rescale, because a scale computed
/// from the data would make the arithmetic depend on the data's range, and two implementations
/// that disagree by one unit about a range would diverge on everything downstream (ADR-0040 B).
pub fn add_elem(a: &[i8], b: &[i8]) -> Result<Vec<i32>, PalwBase0OpError> {
    if a.len() != b.len() {
        return Err(PalwBase0OpError::LengthMismatch { a: a.len(), b: b.len() });
    }
    Ok(a.iter().zip(b).map(|(x, y)| (*x as i32) + (*y as i32)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(value: i64, want_num: i64, want_den: i64, tol_ppm: i64) -> bool {
        let lhs = (value * want_den - want_num * ONE).abs();
        let rhs = want_num.abs() * ONE;
        lhs * 1_000_000 <= tol_ppm * rhs
    }

    /// **ADR-0040 E at the op layer**: the property is worthless if it holds for a bare loop and
    /// not for the op a kernel actually calls. A dot product split into tiles — the shape every
    /// blocked or threaded GEMM produces — gives the identical `i32`.
    #[test]
    fn a_tiled_dot_product_equals_the_whole_one() {
        let a: Vec<i8> = (0..1_024).map(|i| (((i * 37) % 255) - 127) as i8).collect();
        let b: Vec<i8> = (0..1_024).map(|i| (((i * 101) % 255) - 127) as i8).collect();
        let whole = dot_i8(&a, &b).unwrap();
        for tile in [1usize, 7, 64, 256, 512] {
            let tiled: i32 = a.chunks(tile).zip(b.chunks(tile)).map(|(x, y)| dot_i8(x, y).unwrap()).sum();
            assert_eq!(whole, tiled, "tile {tile} changed the accumulator");
        }
        // ...and reversed, which is the other order a real kernel produces.
        let reversed: i32 = a.chunks(64).rev().zip(b.chunks(64).rev()).map(|(x, y)| dot_i8(x, y).unwrap()).sum();
        assert_eq!(whole, reversed);
    }

    /// The bound is enforced rather than assumed, and it is an error rather than a panic — a
    /// panic reachable from validation is a remote chain-halt (audit B7).
    #[test]
    fn an_over_long_dot_is_refused_not_panicked() {
        let long = vec![1i8; MAX_DOT_LEN + 1];
        assert_eq!(dot_i8(&long, &long), Err(PalwBase0OpError::DotTooLong { got: MAX_DOT_LEN + 1 }));
        assert_eq!(dot_i8(&[1, 2], &[1]), Err(PalwBase0OpError::LengthMismatch { a: 2, b: 1 }));
        // Worst case at the bound is exact and in range — at BOTH ends of the type. `i8::MIN`
        // is the one that matters: `(-128)² = 16_384 > 127²`, it is what `requantize` returns for
        // a saturating negative accumulator, and nothing range-checks an artifact's weight bytes.
        // Under `overflow-checks = true` a MAX_DOT_LEN derived from `127²` does not return a wrong
        // sum here, it panics — inside an adjudicator, on peer-supplied operands.
        //
        // The expected values are computed in `i64` and the result widened to meet them, so that an
        // over-large MAX_DOT_LEN fails HERE, inside `dot_i8`, rather than while rustc const-folds
        // this test's own arithmetic. The defect being pinned is the accumulator, not the literal.
        let at_bound = vec![i8::MAX; MAX_DOT_LEN];
        assert_eq!(dot_i8(&at_bound, &at_bound).map(i64::from), Ok(MAX_DOT_LEN as i64 * 127 * 127));
        let at_bound_negative = vec![i8::MIN; MAX_DOT_LEN];
        assert_eq!(dot_i8(&at_bound_negative, &at_bound_negative).map(i64::from), Ok(MAX_DOT_LEN as i64 * 128 * 128));
    }

    /// Softmax is a distribution, its max element dominates, and it is invariant to a constant
    /// shift of the logits — the property the max-subtraction exists to guarantee.
    #[test]
    fn softmax_is_a_distribution_and_shift_invariant() {
        let logits: Vec<i32> = [0i64, -1, -2, -5, -9].iter().map(|v| (v * ONE) as i32).collect();
        let p = softmax(&logits).unwrap();
        let total: i64 = p.iter().map(|v| *v as i64).sum();
        assert!(close(total, 1, 1, 20_000), "probabilities must sum to ~1, got {total}");
        assert!(p[0] > p[1] && p[1] > p[2] && p[2] > p[3] && p[3] > p[4], "order must follow the logits");
        // Shifting every logit by the same constant cannot change the result.
        let shifted: Vec<i32> = logits.iter().map(|v| v + 3 * ONE as i32).collect();
        assert_eq!(softmax(&shifted).unwrap(), p, "softmax must be shift invariant");
        // A degenerate row is defined, not a division by zero.
        assert_eq!(softmax(&[0]).unwrap().len(), 1);
        assert_eq!(softmax(&[]), Err(PalwBase0OpError::Empty));
    }

    /// RMS norm is scale invariant, which is why it takes no input scale.
    #[test]
    fn rms_norm_is_scale_invariant_and_defined_on_a_zero_row() {
        // Values chosen so doubling still fits i8 — the fixture, not the op, is what limits this.
        let x: Vec<i8> = vec![10, -20, 30, -40, 50, -60, 62, -63];
        let doubled: Vec<i8> = x.iter().map(|v| v * 2).collect();
        let a = rms_norm(&x, 1).unwrap();
        let b = rms_norm(&doubled, 1).unwrap();
        // The outputs must be Qk-scale, not a handful of units: a normalized row has entries of
        // order 1.0, so the largest must be a meaningful fraction of ONE. Without this the
        // invariance check below passes trivially on a row of zeros.
        let largest = a.iter().map(|v| v.unsigned_abs()).max().unwrap() as i64;
        assert!(largest > ONE / 4, "rms_norm output must be Qk-scale, largest was {largest}");
        for (p, q) in a.iter().zip(&b) {
            let drift = (*p as i64 - *q as i64).abs() * 1_000 / (p.unsigned_abs() as i64).max(1);
            assert!(drift <= 5, "scale invariance: {p} vs {q} drifted {drift}/1000");
        }
        // A zero row is eps-defined rather than a division by zero.
        let zeros = vec![0i8; 8];
        assert_eq!(rms_norm(&zeros, ONE).unwrap(), vec![0i32; 8]);
        assert_eq!(rms_norm(&[], 1), Err(PalwBase0OpError::Empty));
    }

    /// ADR-0040 C3 / audit 2.4: a sparse wide row must SATURATE its narrowing, not wrap it.
    ///
    /// One large code in a long row of zeros makes the mean of squares tiny, so `r = 1/rms` is
    /// enormous and `code · r` blows past `i32::MAX`. A plain `as i32` there wraps the sign — the
    /// single largest activation in the row becomes the most negative — and, under
    /// `overflow-checks = true`, the multiply itself is a release panic. The op must stay total and
    /// monotone in sign.
    #[test]
    fn rms_norm_saturates_a_sparse_wide_row_instead_of_wrapping() {
        // At the widest legal row a single large code makes the mean of squares tiny, so
        // `r = 1/rms` grows until `code · r` passes `i32::MAX`. MEASURED for this row: r = 34_084_896
        // and 127·r = 4_328_781_792, which the old `as i32` reduced mod 2^32 to 33_814_496 — the
        // largest activation understated 128-fold, silently. (The i64 product never overflowed, so
        // nothing trapped; the narrowing was the whole bug.) A saturating narrowing must pin at the
        // i32 ends instead.
        let mut row = vec![0i8; MAX_DOT_LEN];
        row[0] = 127;
        row[1] = -127;
        let out = rms_norm(&row, 1).expect("a MAX_DOT_LEN row is exactly at the length bound");
        assert_eq!(out[0], i32::MAX, "an out-of-range positive entry saturates high, not wraps to 33_814_496");
        assert_eq!(out[1], i32::MIN, "and symmetrically at the low end");
        // Pin the defect itself: the wrapping value must NOT be what comes back.
        assert_ne!(out[0], 33_814_496, "this is the value the old `as i32` produced; saturation must not reproduce it");
        // A row whose product stays in range is untouched: same value the old `as i32` produced.
        let narrow = rms_norm(&[127, -127, 0, 0, 0, 0, 0, 0], 1).unwrap();
        assert!(narrow[0] > 0 && narrow[1] < 0, "an in-range row keeps both signs and is not clamped");
        assert!(narrow[0] < i32::MAX && narrow[1] > i32::MIN, "an in-range row is not at the saturation ends");
    }

    /// RoPE must be a ROTATION, not merely an isometry.
    ///
    /// Length preservation alone cannot catch a flipped cross-term sign, because a reflection
    /// preserves length too — and an input with a zero component makes the cross term vanish
    /// entirely. Mutation testing caught exactly that: flipping `-` to `+` passed a
    /// length-only test. The direction is pinned first, on an input where the cross term is the
    /// whole answer.
    #[test]
    fn rope_rotates_rather_than_reflects() {
        // 90°: cos = 0, sin = 1. Rotating (0, 1) must give (-1, 0); a reflection gives (+1, 0).
        let quarter = rope_table(&[0, ONE as i32], &[0], &[ONE as i32]).unwrap();
        assert_eq!(quarter[0], -(ONE as i32), "the cross term's sign must rotate, not reflect");
        assert_eq!(quarter[1], 0);
        // And (1, 0) -> (0, 1), the other axis.
        let other = rope_table(&[ONE as i32, 0], &[0], &[ONE as i32]).unwrap();
        assert_eq!(other[0], 0);
        assert_eq!(other[1], ONE as i32);
    }

    /// A rotation is also an isometry, so the length check stays — it catches magnitude errors
    /// the direction check above would miss.
    #[test]
    fn rope_is_a_rotation() {
        // 45°: cos = sin = 1/sqrt(2). Pinned as an integer, like the real table would be.
        let c = 11_863_283i32; // round(2^24 / sqrt(2))
        let x = vec![ONE as i32, 0, 0, ONE as i32];
        let y = rope_table(&x, &[c, c], &[c, c]).unwrap();
        for pair in 0..2 {
            let (a0, b0) = (x[2 * pair] as i64, x[2 * pair + 1] as i64);
            let (a1, b1) = (y[2 * pair] as i64, y[2 * pair + 1] as i64);
            let before = a0 * a0 + b0 * b0;
            let after = a1 * a1 + b1 * b1;
            let drift = (before - after).abs() * 1_000 / before.max(1);
            assert!(drift <= 5, "pair {pair}: rotation must preserve length, drifted {drift}/1000");
        }
        assert_eq!(rope_table(&[1, 2, 3], &[1], &[1]), Err(PalwBase0OpError::NotAMultiple { got: 3, unit: 2 }));
    }

    /// Audit 2.4: `rope_table` must stay total even on arbitrary (non-table) `cos_q`/`sin_q`.
    ///
    /// `a·s + b·c` is a sum of two `i32×i32` products; in `i64` it reaches ~2^63 and overflows —
    /// a release panic under `overflow-checks = true` — when a caller (the refutation path decodes
    /// oracle bytes into these slices) supplies the `i32` extremes rather than a bounded Qk table.
    /// The op must saturate, not panic.
    #[test]
    fn rope_table_saturates_on_extreme_inputs_instead_of_overflowing() {
        // The four corners that maximise |a·s + b·c| and |a·c − b·s|.
        let x = vec![i32::MIN, i32::MIN];
        let out = rope_table(&x, &[i32::MIN], &[i32::MIN]).expect("shape is valid; only the magnitudes are extreme");
        // The exact expected values, not a range check. With a = b = c = s = i32::MIN:
        //   a·c − b·s = 2^62 − 2^62 = 0                  → 0 after `>> K`
        //   a·s + b·c = 2^62 + 2^62 = 2^63               → 2^39 after `>> K`, saturating to i32::MAX
        // The second one is `i64::MAX + 1`, so the pre-fix i64 form overflowed here — that IS a
        // release panic under overflow-checks, which is what this input exercises.
        assert_eq!(out, vec![0, i32::MAX], "extreme inputs must saturate to exactly [0, i32::MAX]");

        // And a real Qk table is bit-identical to the old i64 path (this is the no-regression half).
        let c = 11_863_283i32;
        let normal = rope_table(&[ONE as i32, 0, 0, ONE as i32], &[c, c], &[c, c]).unwrap();
        assert_eq!(normal, vec![c, c, -c, c], "the valid domain must be unchanged by the widening");
    }

    /// SiLU's shape: sigmoid crosses 1/2 at zero, saturates both ways, and silu(0) = 0.
    #[test]
    fn silu_and_sigmoid_have_the_right_shape() {
        assert!(close(int_sigmoid(0) as i64, 1, 2, 20_000), "sigmoid(0) = 1/2");
        assert!(int_sigmoid(8 * ONE as i32) > (ONE as i32) - (ONE as i32) / 100, "sigmoid(+8) ≈ 1");
        assert!(int_sigmoid(-8 * ONE as i32) < (ONE as i32) / 100, "sigmoid(-8) ≈ 0");
        // Monotone increasing.
        let mut previous = i32::MIN;
        for step in -80..=80 {
            let s = int_sigmoid((step * ONE / 10) as i32);
            assert!(s >= previous, "sigmoid must be non-decreasing at {step}");
            previous = s;
        }
        assert_eq!(silu(&[0]), vec![0], "silu(0) = 0");
        // Large positive: silu(x) ≈ x.
        let big = 8 * ONE as i32;
        let s = silu(&[big])[0];
        assert!((s as i64 - big as i64).abs() < (ONE / 50), "silu(8) ≈ 8, got {s}");
    }

    /// The shape-only ops: bounds are errors, and the arithmetic is exact.
    #[test]
    fn gather_and_elementwise_are_exact_and_bounded() {
        let table: Vec<i8> = (0..12).map(|i| i as i8).collect();
        assert_eq!(embed_lookup(&table, 4, 3, 2).unwrap(), &[6, 7, 8]);
        assert_eq!(embed_lookup(&table, 4, 3, 4), Err(PalwBase0OpError::TokenOutOfRange { got: 4, rows: 4 }));
        assert_eq!(mul_elem(&[3, -4], &[5, 6]).unwrap(), vec![15, -24]);
        assert_eq!(add_elem(&[127, -128], &[127, -128]).unwrap(), vec![254, -256], "i32 output cannot saturate here");
        assert_eq!(add_elem(&[1], &[1, 2]), Err(PalwBase0OpError::LengthMismatch { a: 1, b: 2 }));
    }

    /// requantize saturates rather than wrapping, per channel and per tensor alike.
    #[test]
    fn requantize_saturates_per_channel() {
        let acc = vec![i32::MAX, i32::MIN, 0];
        let params = vec![
            QuantParams { multiplier: i32::MAX, shift: 0, zero: 0 },
            QuantParams { multiplier: i32::MAX, shift: 0, zero: 0 },
            QuantParams { multiplier: i32::MAX, shift: 0, zero: 0 },
        ];
        assert_eq!(requantize_row(&acc, &params).unwrap(), vec![127i8, -128, 0]);
        assert_eq!(requantize_row_uniform(&acc, params[0]), vec![127i8, -128, 0]);
        assert_eq!(requantize_row(&acc, &params[..1]), Err(PalwBase0OpError::LengthMismatch { a: 3, b: 1 }));
    }

    /// A whole transformer block's worth of ops chained, to show the layer composes and that no
    /// stage panics on realistic shapes.
    #[test]
    fn the_ops_compose_into_a_block_without_panicking() {
        let dim = 64usize;
        let table: Vec<i8> = (0..(16 * dim)).map(|i| ((i % 200) as i32 - 100) as i8).collect();
        let x = embed_lookup(&table, 16, dim, 5).unwrap().to_vec();

        let normed = rms_norm(&x, 1).unwrap();
        assert_eq!(normed.len(), dim);

        let cos: Vec<i32> = vec![(ONE as i32) / 2; dim / 2];
        let sin: Vec<i32> = vec![(ONE as i32) / 3; dim / 2];
        let rotated = rope_table(&normed, &cos, &sin).unwrap();
        assert_eq!(rotated.len(), dim);

        let q = requantize_row_uniform(&rotated, QuantParams { multiplier: i32::MAX, shift: 8, zero: 0 });
        let weights: Vec<i8> = (0..(dim * dim)).map(|i| ((i % 7) as i32 - 3) as i8).collect();
        let projected = matmul_quant(&weights, &q, dim).unwrap();
        assert_eq!(projected.len(), dim);

        let attention = softmax(&projected[..8]).unwrap();
        let total: i64 = attention.iter().map(|v| *v as i64).sum();
        assert!(close(total, 1, 1, 30_000), "attention row must be a distribution");

        let activated = silu(&projected);
        assert_eq!(activated.len(), dim);
    }
}
