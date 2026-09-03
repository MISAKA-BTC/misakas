//! **The A16 op tier — sixteen-bit activations for the classes int8 cannot carry (ADR-0040 W).**
//!
//! # Why this tier exists, in one table
//!
//! The int8-activation engine's fidelity against the exact float model was reproduced EXACTLY by
//! the float model with activations fake-quantized to six dynamic bits — and the regime ladder
//! around it is steep (top-1 agreement over a 57-position prompt, Qwen2.5-1.5B):
//!
//! | activations (float-simulated) | top-1 |
//! | --- | --- |
//! | dynamic 8-bit                 | 48/57 |
//! | dynamic 7-bit                 | 31/57 |
//! | dynamic 6-bit                 | 4/57 — the static-int8 engine's exact score |
//! | 15-bit                        | 57/57 |
//!
//! Static calibration, power-of-two scale rounding and multi-stage requantization together cost
//! about two bits, so a static int8-activation pipeline lands at effective six bits NO MATTER how
//! well it is calibrated — measured, one plumbing repair at a time, until every repair moved the
//! score by a token or two and none of them was the missing budget. Sixteen-bit activations with
//! eight-bit weights close the gate with margin (top-1 50/57 calibrated, 44/48 held out,
//! ρ ≈ 0.91, both sides of the fidelity bar).
//!
//! # The design rule: `i64` never crosses a step boundary
//!
//! Step tiles ride the leg as 4-byte lanes, and that container is not negotiable here. So every
//! op whose intermediate exceeds 32 bits — a matmul accumulator (`127·32767·8960 < 2^45`), a
//! wide attention logit — is FUSED with the narrowing that brings it back: committed rows are
//! always `i16` codes or Q[`K`] `i32` values, and the wide accumulator lives and dies inside one
//! adjudicable op. This is why the tier's matmuls are `MatMulRequant`/`MatMulRescale` rather
//! than a bare dot: an unfused dot would need an `i64` row at a boundary, which the tile format
//! cannot carry and the court could not open.
//!
//! # What is reused rather than redefined
//!
//! * **`rope_table` (op 4)** — it maps `i32` code rows at any scale; `i16` codes are `i32`
//!   values, and the rotation is scale-preserving. Same op, same pinned table, same id.
//! * **`silu` (op 6)** — defined on Q[`K`] `i32`, which is exactly what [`a16_matmul_rescale`]
//!   commits. Same op, same id.
//! * **`softmax_shifted` (op 5W)** — the wide softmax was added for the int8 engine's split
//!   logits and is precisely what an A16 logit row (i16 codes + a declared `up_bits`) needs.
//!
//! # Associativity carries over (ADR-0040 Decision E)
//!
//! Every accumulation here is exact integer addition in `i64` within a bound that cannot
//! overflow ([`A16_MAX_DOT_LEN`] · max|product| < 2^62), so the order of accumulation cannot
//! change the result — across tile shapes, lane counts, compilers or vendors. The differential
//! tests in this module run every reduction shape an optimized backend would use and assert
//! bit-identity, exactly as `optimized.rs` does for the int8 tier.

use crate::palw_base0::{K, int_rsqrt};

/// The activation code bound: A16 codes are `i16` with the symmetric range, `±32767`.
pub const A16_CODE_MAX: i64 = i16::MAX as i64;

/// Longest reduction any A16 dot may accumulate.
///
/// A premise of Decision E at this width, not a nicety: `|w·x| ≤ 127 · 32767 < 2^22`, so a sum
/// of `2^40` terms stays inside `i64`'s `2^63`. The bound is set far under that — one power of
/// two above the largest real reduction in the family (Qwen2.5's `d_ff` and vocabulary) — so an
/// adversarial length fails fast instead of being priced.
pub const A16_MAX_DOT_LEN: usize = 1 << 18;

/// `shift` domain for the wide requantizations: `m / 2^shift` with `shift ≤ 62`.
pub const A16_MAX_SHIFT: u8 = 62;

/// Why an A16 op refused its operands. Total: every arm is a refusal, never a panic — these run
/// on peer-influenced bytes on the refutation path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwA16OpError {
    Empty,
    LengthMismatch { a: usize, b: usize },
    DotTooLong { got: usize },
    ShiftOutOfDomain { got: u8 },
    NotAMultiple { got: usize, unit: usize },
}

/// Per-channel wide requantization parameters: `out = clamp16(round(acc · m / 2^shift) + zero)`.
///
/// `m` is a SIGNED 64-bit multiplier — the norm gain γ rides here, sign included — and `zero` is
/// the additive registered term at the OUTPUT scale (the projection biases, exactly as the int8
/// tier's G2 amendment carries them). 17 bytes on the wire: `m` LE, `shift`, `zero` LE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct A16QuantParams {
    pub multiplier: i64,
    pub shift: u8,
    pub zero: i64,
}

impl A16QuantParams {
    pub const WIRE_BYTES: usize = 17;

    /// Parse one 17-byte wire triple. Refuses an out-of-domain shift rather than clamping: a
    /// committed shift past 62 is malformed by construction, and recomputing with clamped
    /// arithmetic would compare against a function the specification does not define.
    pub fn from_wire(bytes: &[u8]) -> Result<Self, PalwA16OpError> {
        if bytes.len() != Self::WIRE_BYTES {
            return Err(PalwA16OpError::LengthMismatch { a: bytes.len(), b: Self::WIRE_BYTES });
        }
        let multiplier = i64::from_le_bytes(bytes[0..8].try_into().expect("8 bytes"));
        let shift = bytes[8];
        if shift > A16_MAX_SHIFT {
            return Err(PalwA16OpError::ShiftOutOfDomain { got: shift });
        }
        let zero = i64::from_le_bytes(bytes[9..17].try_into().expect("8 bytes"));
        Ok(Self { multiplier, shift, zero })
    }

    pub fn to_wire(&self) -> [u8; Self::WIRE_BYTES] {
        let mut out = [0u8; Self::WIRE_BYTES];
        out[0..8].copy_from_slice(&self.multiplier.to_le_bytes());
        out[8] = self.shift;
        out[9..17].copy_from_slice(&self.zero.to_le_bytes());
        out
    }
}

/// `round(x · m / 2^shift)`, half away from zero, in `i128` so no operand can overflow.
///
/// The ONE rounding rule of the tier (C1 discipline: enumerated, not accumulated): every
/// narrowing in this module rounds half away from zero, matching `rounding_shift_right`'s rule
/// at the widths the int8 tier uses.
#[inline]
pub fn a16_scale_round(x: i64, multiplier: i64, shift: u8) -> i64 {
    let shift = shift.min(A16_MAX_SHIFT) as u32;
    let p = (x as i128) * (multiplier as i128);
    if shift == 0 {
        return p.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
    }
    let half = 1i128 << (shift - 1);
    let rounded = (p.abs() + half) >> shift;
    let signed = if p < 0 { -rounded } else { rounded };
    signed.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

#[inline]
fn clamp16(x: i64) -> i32 {
    x.clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
}

/// Check a row is A16 codes (each lane inside ±32767); the lanes arrive as `i32`.
fn as_a16(row: &[i32]) -> Result<&[i32], PalwA16OpError> {
    if row.is_empty() {
        return Err(PalwA16OpError::Empty);
    }
    if row.iter().any(|v| (*v as i64).abs() > A16_CODE_MAX) {
        return Err(PalwA16OpError::LengthMismatch { a: row.len(), b: row.len() });
    }
    Ok(row)
}

/// **Op W1: `MatMulRequant` — the fused projection.**
///
/// `out[c] = clamp16(round((Σ_i w[c·n+i] · x[i]) · m_c / 2^shift_c) + zero_c)` with `w` int8 and
/// `x` A16 codes. The `i64` accumulator never leaves the op — that is the tier's boundary rule —
/// and the per-channel triple carries the per-row weight scale, the site's scale change, and the
/// projection bias, exactly the three things the int8 tier spread across three seams.
pub fn a16_matmul_requant(weights: &[i8], x: &[i32], params: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    let x = as_a16(x)?;
    let n = x.len();
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
    Ok((0..out_dim)
        .map(|c| {
            let acc: i64 = weights[c * n..(c + 1) * n].iter().zip(x).map(|(w, v)| *w as i64 * *v as i64).sum();
            let p = params[c];
            clamp16(a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero))
        })
        .collect())
}

/// **Op W2: `MatMulRequantRow` — the fused dot against a second OPENED row.**
///
/// The attention arms: `q·Kᵀ` (per key) and `p·V` (per output element) multiply an activation by
/// committed rows the leg has already opened, not by a registered weight. Same accumulator, same
/// fusion, same params; the second operand is A16 codes rather than int8. The per-term product
/// is `≤ 2^30`, so [`A16_MAX_DOT_LEN`] keeps this sum exact in `i64` with room.
pub fn a16_matmul_requant_row(operand: &[i32], x: &[i32], params: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    let x = as_a16(x)?;
    let operand = as_a16(operand)?;
    let n = x.len();
    if n > A16_MAX_DOT_LEN {
        return Err(PalwA16OpError::DotTooLong { got: n });
    }
    let out_dim = params.len();
    if out_dim == 0 {
        return Err(PalwA16OpError::Empty);
    }
    if operand.len() != out_dim * n {
        return Err(PalwA16OpError::LengthMismatch { a: operand.len(), b: out_dim * n });
    }
    Ok((0..out_dim)
        .map(|c| {
            let acc: i64 = operand[c * n..(c + 1) * n].iter().zip(x).map(|(w, v)| *w as i64 * *v as i64).sum();
            let p = params[c];
            clamp16(a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero))
        })
        .collect())
}

/// **Op W3: `MatMulRescale` — the fused projection into Q[`K`].**
///
/// The gate projection must reach `Silu` (op 6) as a Q[`K`] VALUE, exactly — SiLU is nonlinear,
/// so its input scale is not a convention but part of the function. Out lanes are `i32` Qk;
/// saturation at the `i32` rail is defined (nothing wraps, C3), and a gate value near ±128 is
/// already at the edge of the op-6 domain.
pub fn a16_matmul_rescale(weights: &[i8], x: &[i32], params: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    let x = as_a16(x)?;
    let n = x.len();
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
    Ok((0..out_dim)
        .map(|c| {
            let acc: i64 = weights[c * n..(c + 1) * n].iter().zip(x).map(|(w, v)| *w as i64 * *v as i64).sum();
            let p = params[c];
            a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero).clamp(i32::MIN as i64, i32::MAX as i64) as i32
        })
        .collect())
}

/// **Op W4: `RmsNormA16` — the unit row in Q[`K`], from A16 codes.**
///
/// Scale-invariant like its int8 sibling: the input is codes at whatever scale the stream
/// carries, the output is the unit-RMS row in Qk. The gain γ is NOT here — it rides the
/// following [`a16_requant`]'s signed multipliers, because folding it into int8 weight columns
/// zeroes the small-γ channels (measured; the reason the int8 tier moved it too).
///
/// Output bound: `|unit| ≤ √n` (one hot lane against a silent row), so Qk `i32` holds every row
/// up to `n = 16,384` exactly and the wider rows of this family (`d ≤ 8,960`) with margin.
pub fn a16_rms_norm(x: &[i32], eps_q: i64) -> Result<Vec<i32>, PalwA16OpError> {
    let x = as_a16(x)?;
    if x.len() > A16_MAX_DOT_LEN {
        return Err(PalwA16OpError::DotTooLong { got: x.len() });
    }
    let sum_sq: i64 = x.iter().map(|v| *v as i64 * *v as i64).sum();
    let mean_q = ((sum_sq as i128) << K) / (x.len() as i128);
    let r = int_rsqrt(mean_q.clamp(0, i64::MAX as i128) as i64 + eps_q);
    Ok(x.iter().map(|v| ((*v as i128 * r as i128).clamp(i32::MIN as i128, i32::MAX as i128)) as i32).collect())
}

/// **Op W5: `RequantA16` — narrow an `i32` row to A16 codes, per channel.**
///
/// The seam op: norm-unit-Qk → codes (γ in the signed multipliers), softmax-Qk → 15-bit
/// probability codes, elementwise products → codes. `zero` is at the output scale, as always.
pub fn a16_requant(x: &[i32], params: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    if x.is_empty() {
        return Err(PalwA16OpError::Empty);
    }
    if params.len() != x.len() {
        return Err(PalwA16OpError::LengthMismatch { a: params.len(), b: x.len() });
    }
    Ok(x.iter().zip(params).map(|(v, p)| clamp16(a16_scale_round(*v as i64, p.multiplier, p.shift).saturating_add(p.zero))).collect())
}

/// **Op W8: `RopeA16` — the pinned rotation, saturated back to A16 codes.**
///
/// `rope_table` (op 4) preserves scale but not RANGE: a rotated pair reaches `√2 ·` its input,
/// so an A16 code row can leave the op at ±46,340 — inside the lane, outside the code range the
/// next dot requires. The int8 tier clamps after rotation for the same reason; here the clamp is
/// INSIDE the op, because an op whose output can be un-inputtable to its own successor is a seam
/// two implementations can disagree about. Calibration sizes the scale on the post-rotation
/// absmax with headroom, so the clamp is quiet on honest rows; on adversarial ones it saturates
/// (C3: nothing wraps).
pub fn a16_rope(x: &[i32], cos_q: &[i32], sin_q: &[i32]) -> Result<Vec<i32>, PalwA16OpError> {
    let x = as_a16(x)?;
    if x.len() % 2 != 0 {
        return Err(PalwA16OpError::NotAMultiple { got: x.len(), unit: 2 });
    }
    let pairs = x.len() / 2;
    if cos_q.len() != pairs || sin_q.len() != pairs {
        return Err(PalwA16OpError::LengthMismatch { a: cos_q.len(), b: pairs });
    }
    let mut out = Vec::with_capacity(x.len());
    for p in 0..pairs {
        let (a, b) = (x[2 * p] as i128, x[2 * p + 1] as i128);
        let (c, s) = (cos_q[p] as i128, sin_q[p] as i128);
        out.push(clamp16((((a * c) - (b * s)) >> K).clamp(i64::MIN as i128, i64::MAX as i128) as i64));
        out.push(clamp16((((a * s) + (b * c)) >> K).clamp(i64::MIN as i128, i64::MAX as i128) as i64));
    }
    Ok(out)
}

/// **Op W6: `AddElemA16` — exact elementwise sum of two A16 rows, `i32` out.**
pub fn a16_add_elem(a: &[i32], b: &[i32]) -> Result<Vec<i32>, PalwA16OpError> {
    let a = as_a16(a)?;
    let b = as_a16(b)?;
    if a.len() != b.len() {
        return Err(PalwA16OpError::LengthMismatch { a: a.len(), b: b.len() });
    }
    Ok(a.iter().zip(b).map(|(x, y)| x + y).collect())
}

/// **Op W7: `MulElemA16` — exact elementwise product of two A16 rows, `i32` out.**
///
/// `32767² < 2^30`, so the product is exact in the lane it rides.
pub fn a16_mul_elem(a: &[i32], b: &[i32]) -> Result<Vec<i32>, PalwA16OpError> {
    let a = as_a16(a)?;
    let b = as_a16(b)?;
    if a.len() != b.len() {
        return Err(PalwA16OpError::LengthMismatch { a: a.len(), b: b.len() });
    }
    Ok(a.iter().zip(b).map(|(x, y)| x * y).collect())
}

/// **Op W9: `AttnScores` — the q·Kᵀ logits row, GQA-aware, fused to logit codes.**
///
/// `q` is the rotated query row (`heads × d_head`), `k_series` the cache series exactly as the
/// court's canonical input set concatenates it — the K-cache node's FULL `kv_dim` row per
/// position, position-major — and the output is head-major: `out[h·kv_len + j]` reduces `q`'s
/// head `h` against key `j`'s grouped slice (`kv_off = (h / group) · d_head`). The head mapping
/// is INSIDE the op because it is part of the function: an adjudicator that had to guess the
/// grouping would be guessing the model.
#[allow(clippy::too_many_arguments)]
pub fn a16_attn_scores(
    q: &[i32],
    k_series: &[i32],
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    params: &[A16QuantParams],
) -> Result<Vec<i32>, PalwA16OpError> {
    let q = as_a16(q)?;
    let k_series = as_a16(k_series)?;
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
    if k_series.is_empty() || k_series.len() % kv_dim != 0 {
        return Err(PalwA16OpError::NotAMultiple { got: k_series.len(), unit: kv_dim });
    }
    let kv_len = k_series.len() / kv_dim;
    if params.len() != heads * kv_len {
        return Err(PalwA16OpError::LengthMismatch { a: params.len(), b: heads * kv_len });
    }
    let group = heads / kv_heads;
    let mut out = Vec::with_capacity(heads * kv_len);
    for h in 0..heads {
        let qh = &q[h * d_head..(h + 1) * d_head];
        let kv_off = (h / group) * d_head;
        for j in 0..kv_len {
            let kh = &k_series[j * kv_dim + kv_off..j * kv_dim + kv_off + d_head];
            let acc: i64 = qh.iter().zip(kh).map(|(a, b)| *a as i64 * *b as i64).sum();
            let p = params[h * kv_len + j];
            out.push(clamp16(a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero)));
        }
    }
    Ok(out)
}

/// **Op W10: `AttnValues` — the probability-weighted value sum, GQA-aware, fused to codes.**
///
/// `p` is the head-major probability-code row (`heads × kv_len`), `v_series` the V-cache series
/// in the same position-major layout as W9's keys, and `out[h·d_head + i]` reduces head `h`'s
/// probabilities against value lane `kv_off + i` over the history.
#[allow(clippy::too_many_arguments)]
pub fn a16_attn_values(
    p: &[i32],
    v_series: &[i32],
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    params: &[A16QuantParams],
) -> Result<Vec<i32>, PalwA16OpError> {
    let p = as_a16(p)?;
    let v_series = as_a16(v_series)?;
    if heads == 0 || kv_heads == 0 || d_head == 0 || !heads.is_multiple_of(kv_heads) {
        return Err(PalwA16OpError::Empty);
    }
    let kv_dim = kv_heads * d_head;
    if v_series.is_empty() || v_series.len() % kv_dim != 0 {
        return Err(PalwA16OpError::NotAMultiple { got: v_series.len(), unit: kv_dim });
    }
    let kv_len = v_series.len() / kv_dim;
    if kv_len > A16_MAX_DOT_LEN {
        return Err(PalwA16OpError::DotTooLong { got: kv_len });
    }
    if p.len() != heads * kv_len {
        return Err(PalwA16OpError::LengthMismatch { a: p.len(), b: heads * kv_len });
    }
    if params.len() != heads * d_head {
        return Err(PalwA16OpError::LengthMismatch { a: params.len(), b: heads * d_head });
    }
    let group = heads / kv_heads;
    let mut out = Vec::with_capacity(heads * d_head);
    for h in 0..heads {
        let ph = &p[h * kv_len..(h + 1) * kv_len];
        let kv_off = (h / group) * d_head;
        for i in 0..d_head {
            let acc: i64 = (0..kv_len).map(|j| ph[j] as i64 * v_series[j * kv_dim + kv_off + i] as i64).sum();
            let prm = params[h * d_head + i];
            out.push(clamp16(a16_scale_round(acc, prm.multiplier, prm.shift).saturating_add(prm.zero)));
        }
    }
    Ok(out)
}

/// **Op W11: the row-wise wide softmax — op 5W applied per head segment.**
///
/// The committed logits row is head-major (`rows × row_len`), and softmax normalizes WITHIN a
/// head: applying op 5W across the concatenation would normalize across heads, which is a
/// different (and wrong) function that a whole-row adjudicator would have silently computed.
pub fn a16_softmax_rows(logits: &[i32], row_len: usize, up_bits: u8) -> Result<Vec<i32>, PalwA16OpError> {
    if logits.is_empty() || row_len == 0 || !logits.len().is_multiple_of(row_len) {
        return Err(PalwA16OpError::NotAMultiple { got: logits.len(), unit: row_len.max(1) });
    }
    let mut out = Vec::with_capacity(logits.len());
    for seg in logits.chunks_exact(row_len) {
        let probs = crate::palw_base0_ops::softmax_shifted(seg, up_bits).map_err(|_| PalwA16OpError::Empty)?;
        out.extend(probs);
    }
    Ok(out)
}

// =============================================================================================
// ADR-0082 Decisions 1 and 2 — the fused attention site, and the tile arithmetic its dissection
// recomputes at the bottom
// =============================================================================================

/// The three quantities a range of history positions contributes to one head's fused attention,
/// all computed against the root claim's `(m*, S*)`. ONE type with the dissection's range claim,
/// so the fold the court checks between rounds and the fold the executor folds its own tiles
/// with are the same function (`palw_attn_dissect::palw_attn_fold_v1`).
pub type A16AttnTileTripleV1 = crate::palw_attn_dissect::PalwAttnRangeClaimV1;

/// The registered narrowings a fused site reads: W9's score triple, the probability requant
/// triple and W10's value triple — one of each, tiled over the row exactly as the adjudicator
/// tiles them today (`attn_params_count_v1`, `ReqSlot::Probs`) — plus the softmax's widening
/// byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct A16AttnFusedParamsV1 {
    pub scores: A16QuantParams,
    pub probs: A16QuantParams,
    pub values: A16QuantParams,
    pub up_bits: u8,
}

/// One head's requantized score against key row `j` — W9's per-key arithmetic, verbatim.
#[inline]
fn a16_attn_score_one(qh: &[i32], k_row: &[i32], p: A16QuantParams) -> i32 {
    let acc: i64 = qh.iter().zip(k_row).map(|(a, b)| *a as i64 * *b as i64).sum();
    clamp16(a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero))
}

/// W11's per-element exponent given the row max — `softmax_shifted`'s map, one element: the
/// difference widened FIRST in `i64`, clamped SECOND, then the pinned `int_exp`.
#[inline]
fn a16_attn_exp_one(score: i32, max: i64, up_bits: u8) -> i64 {
    let up = up_bits.min(62) as i64;
    let diff = ((score as i64 - max) << up).clamp(i32::MIN as i64, 0);
    crate::palw_base0::int_exp(diff as i32) as i64
}

/// W11's per-element probability given the exponent and the row's reciprocal sum —
/// `((e · recip) >> K)` — narrowed by the probs requantization (W5), one code.
#[inline]
fn a16_attn_prob_code_one(e: i64, recip: i64, probs: A16QuantParams) -> i32 {
    let p = ((e * recip) >> crate::palw_base0::K) as i32;
    clamp16(a16_scale_round(p as i64, probs.multiplier, probs.shift).saturating_add(probs.zero))
}

/// **The bottom of the dissection: one head's triple over a tile of history rows.**
///
/// `qh` is the head's rotated query slice (`d_head` codes); `k_tile` / `v_tile` are the tile's
/// K and V rows, position-major, each row `kv_dim` wide — the cache's own layout, so a tile
/// opened from the state chunk map is handed over unchanged; `kv_off` is the head's GQA slice
/// within a row; `lanes` is the disputed `(first, count)` within the head. `m_star` and `s_star`
/// are the ROOT's claims. The max is recomputed from the scores alone — so a wrong `m*` is caught
/// at the tile whose true max exceeds it — while the exponent sum and the value partials are
/// computed AGAINST `(m*, S*)`, which is what makes every level of the dissection consistent with
/// every other and lets the tile be recomputed without the row it sits in.
#[allow(clippy::too_many_arguments)]
pub fn a16_attn_tile_triple_v1(
    qh: &[i32],
    k_tile: &[i32],
    v_tile: &[i32],
    kv_dim: usize,
    kv_off: usize,
    lanes: (usize, usize),
    params: A16AttnFusedParamsV1,
    m_star: i32,
    s_star: i64,
) -> Result<A16AttnTileTripleV1, PalwA16OpError> {
    let qh = as_a16(qh)?;
    let k_tile = as_a16(k_tile)?;
    let v_tile = as_a16(v_tile)?;
    let d_head = qh.len();
    if kv_dim == 0 || kv_off + d_head > kv_dim || lanes.1 == 0 || lanes.0 + lanes.1 > d_head {
        return Err(PalwA16OpError::Empty);
    }
    if d_head > A16_MAX_DOT_LEN {
        return Err(PalwA16OpError::DotTooLong { got: d_head });
    }
    if k_tile.len() % kv_dim != 0 {
        return Err(PalwA16OpError::NotAMultiple { got: k_tile.len(), unit: kv_dim });
    }
    if v_tile.len() != k_tile.len() {
        return Err(PalwA16OpError::LengthMismatch { a: v_tile.len(), b: k_tile.len() });
    }
    let positions = k_tile.len() / kv_dim;
    if positions > A16_MAX_DOT_LEN {
        return Err(PalwA16OpError::DotTooLong { got: positions });
    }
    // An honest row's sum is positive — its max contributes `int_exp(0)`, which is positive (the
    // pinned polynomial's value at zero, within a rounding of `ONE`) — so a non-positive `S*` is a
    // claim no execution produced: refused, never divided by.
    if s_star <= 0 {
        return Err(PalwA16OpError::Empty);
    }
    let recip = crate::palw_base0::int_recip(s_star);
    let mut max = i32::MIN;
    let mut exp_sum = 0i64;
    let mut v_acc = vec![0i64; lanes.1];
    for j in 0..positions {
        let base = j * kv_dim + kv_off;
        let s = a16_attn_score_one(qh, &k_tile[base..base + d_head], params.scores);
        max = max.max(s);
        let e = a16_attn_exp_one(s, m_star as i64, params.up_bits);
        // `e ≤ 2^24` and `positions ≤ 2^18`: the sum stays under 2^42.
        exp_sum += e;
        let c = a16_attn_prob_code_one(e, recip, params.probs) as i64;
        let v_row = &v_tile[base + lanes.0..base + lanes.0 + lanes.1];
        for (acc, v) in v_acc.iter_mut().zip(v_row) {
            // `|c · v| < 2^30` over at most 2^18 positions: under 2^48.
            *acc += c * (*v as i64);
        }
    }
    Ok(A16AttnTileTripleV1 { max, exp_sum, v_acc })
}

/// **The finalization**: W10's narrowing of the folded value partials — one triple for every
/// lane, exactly as `a16_attn_values` narrows its accumulator.
pub fn a16_attn_finalize_v1(v_acc: &[i64], values: A16QuantParams) -> Vec<i32> {
    v_acc.iter().map(|acc| clamp16(a16_scale_round(*acc, values.multiplier, values.shift).saturating_add(values.zero))).collect()
}

/// Cut a position-major series of `kv_dim`-wide rows into tiles of `tile` rows (the last ragged).
fn a16_series_tiles(series: &[i32], kv_dim: usize, tile: usize) -> impl Iterator<Item = &[i32]> {
    series.chunks(kv_dim * tile.max(1))
}

/// **The honest root claim for one head** — three passes over the history at `tile` positions a
/// pass, each a fold of [`a16_attn_tile_triple_v1`] over the tiles: the max first, then the
/// exponent sum against it, then the value partials against both. What an executor computes to
/// commit the output and what a challenger recomputes to know which child to dispute.
#[allow(clippy::too_many_arguments)]
pub fn a16_attn_root_claim_v1(
    qh: &[i32],
    k_series: &[i32],
    v_series: &[i32],
    kv_dim: usize,
    kv_off: usize,
    lanes: (usize, usize),
    params: A16AttnFusedParamsV1,
    tile: usize,
) -> Result<A16AttnTileTripleV1, PalwA16OpError> {
    use crate::palw_attn_dissect::palw_attn_fold_v1;
    let fold = |m_star: i32, s_star: i64| -> Result<A16AttnTileTripleV1, PalwA16OpError> {
        let mut triples = Vec::new();
        for (k_tile, v_tile) in a16_series_tiles(k_series, kv_dim, tile).zip(a16_series_tiles(v_series, kv_dim, tile)) {
            triples.push(a16_attn_tile_triple_v1(qh, k_tile, v_tile, kv_dim, kv_off, lanes, params, m_star, s_star)?);
        }
        // The dissection's fold caps a ROUND at its arity; a whole history is folded here in one
        // pass, so fold it in arity-sized groups the way the rounds would.
        let mut level = triples;
        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(crate::palw_attn_dissect::PALW_ATTN_DISSECT_MAX_CHILDREN));
            for group in level.chunks(crate::palw_attn_dissect::PALW_ATTN_DISSECT_MAX_CHILDREN) {
                next.push(palw_attn_fold_v1(group).map_err(|_| PalwA16OpError::Empty)?);
            }
            level = next;
        }
        level.pop().ok_or(PalwA16OpError::Empty)
    };
    // Pass 1: the max needs no claim; the other two fields of this pass are discarded.
    let m_star = fold(0, crate::palw_base0::ONE)?.max;
    // Pass 2: the exponent sum against the true max; the value partials are discarded.
    let s_star = fold(m_star, crate::palw_base0::ONE)?.exp_sum;
    // Pass 3: everything against the root — and the max and sum must reproduce themselves.
    let root = fold(m_star, s_star)?;
    debug_assert_eq!((root.max, root.exp_sum), (m_star, s_star));
    Ok(root)
}

/// **The fused site as the four shipped kernels composed** — W9 → W11 → the probs requant → W10,
/// one triple each, tiled as the adjudicator tiles them. This IS the fused node's semantics; the
/// tile route above is only a different way of computing and checking it, and
/// `fused::the_tile_route_is_the_composition` is what says the two never part.
pub fn a16_attn_fused_reference_v1(
    q: &[i32],
    k_series: &[i32],
    v_series: &[i32],
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    params: A16AttnFusedParamsV1,
) -> Result<Vec<i32>, PalwA16OpError> {
    let kv_dim = kv_heads.max(1) * d_head.max(1);
    if k_series.is_empty() || k_series.len() % kv_dim != 0 {
        return Err(PalwA16OpError::NotAMultiple { got: k_series.len(), unit: kv_dim });
    }
    let kv_len = k_series.len() / kv_dim;
    let scores = a16_attn_scores(q, k_series, heads, kv_heads, d_head, &vec![params.scores; heads * kv_len])?;
    let probs = a16_softmax_rows(&scores, kv_len, params.up_bits)?;
    let codes = a16_requant(&probs, &vec![params.probs; probs.len()])?;
    a16_attn_values(&codes, v_series, heads, kv_heads, d_head, &vec![params.values; heads * d_head])
}

/// The fused site through the TILE route: per head, the root claim over all lanes and its
/// finalization — the output row an executor commits, computed the way a court will check it.
#[allow(clippy::too_many_arguments)]
pub fn a16_attn_fused_via_tiles_v1(
    q: &[i32],
    k_series: &[i32],
    v_series: &[i32],
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    params: A16AttnFusedParamsV1,
    tile: usize,
) -> Result<Vec<i32>, PalwA16OpError> {
    if heads == 0 || kv_heads == 0 || d_head == 0 || !heads.is_multiple_of(kv_heads) {
        return Err(PalwA16OpError::Empty);
    }
    if q.len() != heads * d_head {
        return Err(PalwA16OpError::LengthMismatch { a: q.len(), b: heads * d_head });
    }
    let kv_dim = kv_heads * d_head;
    let group = heads / kv_heads;
    let mut out = Vec::with_capacity(heads * d_head);
    for h in 0..heads {
        let qh = &q[h * d_head..(h + 1) * d_head];
        let kv_off = (h / group) * d_head;
        let root = a16_attn_root_claim_v1(qh, k_series, v_series, kv_dim, kv_off, (0, d_head), params, tile)?;
        out.extend(a16_attn_finalize_v1(&root.v_acc, params.values));
    }
    Ok(out)
}

#[cfg(test)]
mod fused {
    use super::*;
    use crate::palw_attn_dissect::{PalwAttnDissectError, palw_attn_child_ranges_v1, palw_attn_fold_check_v1, palw_attn_fold_v1};

    fn codes(n: usize, seed: u64) -> Vec<i32> {
        let mut state = seed | 1;
        (0..n)
            .map(|i| {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                match i % 23 {
                    0 => A16_CODE_MAX as i32,
                    1 => -(A16_CODE_MAX as i32),
                    _ => ((state >> 33) % 65_535) as i32 - 32_767,
                }
            })
            .collect()
    }

    fn params() -> A16AttnFusedParamsV1 {
        A16AttnFusedParamsV1 {
            // Scores: a q·k dot of 8 codes reaches ~2^33; `m / 2^shift` brings it to a code.
            scores: A16QuantParams { multiplier: 1 << 10, shift: 30, zero: 3 },
            // Probabilities: Q24 in, 15-bit codes out — `p · 2^15 / 2^24`.
            probs: A16QuantParams { multiplier: 1 << 15, shift: 24, zero: 0 },
            // Values: a code·code dot over the history, narrowed back to a code.
            values: A16QuantParams { multiplier: 1, shift: 22, zero: -5 },
            up_bits: 2,
        }
    }

    /// **`int_exp(0)` is positive**, so an honest row's sum is positive and the uniform branch of
    /// `softmax_shifted` is unreachable — the premise `a16_attn_tile_triple_v1` refuses on. It is
    /// the pinned polynomial's value at zero, near `ONE` but not `ONE` itself: the premise is the
    /// SIGN, and this pins exactly that.
    #[test]
    fn an_honest_rows_sum_is_positive() {
        let at_zero = crate::palw_base0::int_exp(0) as i64;
        assert!(at_zero > 0, "int_exp(0) = {at_zero}");
        // The pinned polynomial is ~3e-4 above ONE at zero (16,781,800 against 16,777,216); the
        // premise this module rests on is the sign, and the bound below only says the value is
        // exp(0)'s neighbourhood rather than a table artefact.
        assert!((at_zero - crate::palw_base0::ONE).abs() * 100 <= crate::palw_base0::ONE, "int_exp(0) = {at_zero} is not within 1% of ONE");
    }

    /// **The tile route is the composition** — byte for byte, at every history length and every
    /// tile width, ragged tails included. This is ADR-0082 Z1, the equality the whole design
    /// rests on: a fused site computed and checked in tiles IS the four shipped kernels.
    #[test]
    fn the_tile_route_is_the_composition() {
        let (heads, kv_heads, d_head) = (4usize, 2usize, 8usize);
        let kv_dim = kv_heads * d_head;
        let q = codes(heads * d_head, 11);
        for kv_len in [1usize, 2, 5, 16, 17, 33, 64, 100] {
            let k = codes(kv_len * kv_dim, 100 + kv_len as u64);
            let v = codes(kv_len * kv_dim, 200 + kv_len as u64);
            let reference = a16_attn_fused_reference_v1(&q, &k, &v, heads, kv_heads, d_head, params()).expect("the composition runs");
            for tile in [1usize, 3, 4, 16, 64] {
                let tiled = a16_attn_fused_via_tiles_v1(&q, &k, &v, heads, kv_heads, d_head, params(), tile).expect("the tile route runs");
                assert_eq!(tiled, reference, "kv_len {kv_len}, tile {tile}: the tile route parted from the composition");
            }
        }
    }

    /// **A lie at the root is caught by the fold, at the level and field it lives in.** The honest
    /// children fold to the honest root; a root that lies about `m*`, `S*` or the output fails the
    /// FIRST fold check by name — before any tile is opened.
    #[test]
    fn a_lie_at_the_root_fails_the_first_fold_by_name() {
        let (heads, kv_heads, d_head) = (2usize, 1usize, 8usize);
        let kv_dim = kv_heads * d_head;
        let (kv_len, tile) = (70usize, 16usize);
        let q = codes(heads * d_head, 7);
        let k = codes(kv_len * kv_dim, 8);
        let v = codes(kv_len * kv_dim, 9);
        let (h, lanes) = (1usize, (2usize, 3usize));
        let qh = &q[h * d_head..(h + 1) * d_head];
        let root = a16_attn_root_claim_v1(qh, &k, &v, kv_dim, 0, lanes, params(), tile).expect("the honest root");
        // The children of the whole range at arity 4: five tiles cut into [2, 2, 1].
        let tiles: Vec<(u64, u64)> = palw_attn_child_ranges_v1(0, kv_len.div_ceil(tile) as u64, 4).expect("legal");
        assert_eq!(tiles, vec![(0, 2), (2, 2), (4, 1)]);
        let child = |(first, count): (u64, u64)| {
            let lo = first as usize * tile * kv_dim;
            let hi = ((first + count) as usize * tile * kv_dim).min(k.len());
            a16_attn_tile_triple_v1(qh, &k[lo..hi], &v[lo..hi], kv_dim, 0, lanes, params(), root.max, root.exp_sum).expect("a child")
        };
        let children: Vec<_> = tiles.iter().map(|r| child(*r)).collect();
        palw_attn_fold_check_v1(&root, &children).expect("the honest root folds from its children");
        assert_eq!(palw_attn_fold_v1(&children).expect("folds"), root);

        let mut lied = root.clone();
        lied.max += 1;
        assert!(matches!(palw_attn_fold_check_v1(&lied, &children), Err(PalwAttnDissectError::MaxDoesNotFold { .. })));
        let mut lied = root.clone();
        lied.exp_sum -= 1;
        assert!(matches!(palw_attn_fold_check_v1(&lied, &children), Err(PalwAttnDissectError::SumDoesNotFold { .. })));
        let mut lied = root.clone();
        lied.v_acc[2] += 1;
        assert!(matches!(palw_attn_fold_check_v1(&lied, &children), Err(PalwAttnDissectError::ValueDoesNotFold { lane: 2, .. })));

        // And a bottom recomputed against a lied `(m*, S*)` does not reproduce the honest child:
        // the max is the same (it never depended on the claim), the sum and the partials move.
        let honest = child(tiles[0]);
        let against_lie = a16_attn_tile_triple_v1(qh, &k[..2 * tile * kv_dim], &v[..2 * tile * kv_dim], kv_dim, 0, lanes, params(), root.max + 40, root.exp_sum)
            .expect("computes");
        assert_eq!(against_lie.max, honest.max);
        assert_ne!(against_lie.exp_sum, honest.exp_sum);
        // A non-positive sum is a claim no execution produced.
        assert_eq!(
            a16_attn_tile_triple_v1(qh, &k[..tile * kv_dim], &v[..tile * kv_dim], kv_dim, 0, lanes, params(), root.max, 0),
            Err(PalwA16OpError::Empty)
        );
    }

    /// **The bottom opening is GQA-aware and lane-sliced**: a head reads its own slice of the
    /// shared K/V rows and a lane window of the head, and the finalization of the sliced partials
    /// equals the reference's lanes.
    #[test]
    fn the_bottom_reads_the_heads_slice_and_only_the_disputed_lanes() {
        let (heads, kv_heads, d_head) = (4usize, 2usize, 8usize);
        let kv_dim = kv_heads * d_head;
        let kv_len = 40usize;
        let q = codes(heads * d_head, 3);
        let k = codes(kv_len * kv_dim, 4);
        let v = codes(kv_len * kv_dim, 5);
        let reference = a16_attn_fused_reference_v1(&q, &k, &v, heads, kv_heads, d_head, params()).expect("runs");
        let group = heads / kv_heads;
        for h in 0..heads {
            let qh = &q[h * d_head..(h + 1) * d_head];
            let kv_off = (h / group) * d_head;
            for (first, count) in [(0usize, 8usize), (2, 3), (7, 1)] {
                let root = a16_attn_root_claim_v1(qh, &k, &v, kv_dim, kv_off, (first, count), params(), 16).expect("root");
                let lanes = a16_attn_finalize_v1(&root.v_acc, params().values);
                assert_eq!(lanes, reference[h * d_head + first..h * d_head + first + count].to_vec(), "head {h} lanes {first}+{count}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_of(m: i64, shift: u8, zero: i64) -> A16QuantParams {
        A16QuantParams { multiplier: m, shift, zero }
    }

    /// Deterministic operand rows spanning the full A16 range, rails included.
    fn codes16(n: usize, seed: u64) -> Vec<i32> {
        let mut state = seed | 1;
        (0..n)
            .map(|i| {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                match i % 19 {
                    0 => A16_CODE_MAX as i32,
                    1 => -(A16_CODE_MAX as i32),
                    _ => ((state >> 33) % 65_535) as i32 - 32_767,
                }
            })
            .collect()
    }

    fn weights8(n: usize, seed: u64) -> Vec<i8> {
        let mut state = seed | 1;
        (0..n)
            .map(|i| {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                match i % 17 {
                    0 => 127,
                    1 => -128,
                    _ => ((state >> 40) % 255) as i32 as i8,
                }
            })
            .collect()
    }

    /// **Decision E at this width: every reduction order gives the same bits.**
    ///
    /// The fused matmul is recomputed with interleaved lanes, cache blocks and reversed order —
    /// each a structural change that moves a float result — and must agree bit for bit, because
    /// the accumulator is exact `i64` inside the domain bound.
    #[test]
    fn every_reduction_order_gives_the_same_bits() {
        for len in [1usize, 7, 16, 129, 1_000, 4_096] {
            let x = codes16(len, 0xA16);
            let w = weights8(len * 3, 0xBEEF);
            let params = vec![params_of(3_000_000_007, 40, 5), params_of(-(1 << 30), 33, -9), params_of(1, 0, 0)];
            let want = a16_matmul_requant(&w, &x, &params).unwrap();

            for lanes in [2usize, 4, 8, 16] {
                let interleaved: Vec<i32> = (0..3)
                    .map(|c| {
                        let mut partial = vec![0i64; lanes];
                        for (i, (a, b)) in w[c * len..(c + 1) * len].iter().zip(&x).enumerate() {
                            partial[i % lanes] += *a as i64 * *b as i64;
                        }
                        let acc = partial.iter().rev().sum::<i64>();
                        let p = params[c];
                        (a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero)).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
                    })
                    .collect();
                assert_eq!(interleaved, want, "{lanes} lanes, len {len}");
            }
            for block in [3usize, 64, 512] {
                let blocked: Vec<i32> = (0..3)
                    .map(|c| {
                        let row = &w[c * len..(c + 1) * len];
                        let acc: i64 = row
                            .chunks(block)
                            .zip(x.chunks(block))
                            .map(|(ws, xs)| ws.iter().zip(xs).map(|(a, b)| *a as i64 * *b as i64).sum::<i64>())
                            .sum();
                        let p = params[c];
                        (a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero)).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
                    })
                    .collect();
                assert_eq!(blocked, want, "block {block}, len {len}");
            }
        }
    }

    /// The domain bound is a premise: a longer reduction is refused, never priced.
    #[test]
    fn a_reduction_past_the_bound_is_refused() {
        let x = vec![1i32; A16_MAX_DOT_LEN + 1];
        let w = vec![1i8; A16_MAX_DOT_LEN + 1];
        assert!(matches!(a16_matmul_requant(&w, &x, &[params_of(1, 0, 0)]), Err(PalwA16OpError::DotTooLong { .. })));
        assert!(matches!(a16_rms_norm(&x, 1), Err(PalwA16OpError::DotTooLong { .. })));
    }

    /// A lane outside ±32767 is not an A16 code, and every op refuses the row rather than
    /// computing something about a different class.
    #[test]
    fn an_out_of_range_lane_is_refused_everywhere() {
        let bad = vec![40_000i32, 1];
        let good = vec![1i32, 2];
        assert!(a16_add_elem(&bad, &good).is_err());
        assert!(a16_mul_elem(&good, &bad).is_err());
        assert!(a16_rms_norm(&bad, 1).is_err());
        assert!(a16_matmul_requant(&[1, 2], &bad, &[params_of(1, 0, 0)]).is_err());
        assert!(a16_matmul_requant_row(&bad, &good, &[params_of(1, 0, 0)]).is_err());
    }

    /// The rounding rule, pinned: half away from zero, both signs, and the shift domain refused
    /// past 62 at the WIRE (parse), clamped in the arithmetic (total function).
    #[test]
    fn the_rounding_rule_is_half_away_from_zero() {
        assert_eq!(a16_scale_round(3, 1, 1), 2, "1.5 rounds away to 2");
        assert_eq!(a16_scale_round(-3, 1, 1), -2, "-1.5 rounds away to -2");
        assert_eq!(a16_scale_round(5, 1, 2), 1, "1.25 rounds to 1");
        assert_eq!(a16_scale_round(1, -1, 1), -1, "signed multipliers flow through, -0.5 away to -1");
        assert!(matches!(
            A16QuantParams::from_wire(&params_of(1, 63, 0).to_wire()),
            Err(PalwA16OpError::ShiftOutOfDomain { got: 63 })
        ));
        let p = params_of(-(3 << 20), 41, 7);
        assert_eq!(A16QuantParams::from_wire(&p.to_wire()).unwrap(), p, "wire round-trips");
    }

    /// `zero` is added at the OUTPUT scale, after the scaled rounding, before the clamp — the
    /// same order the int8 tier's G2 amendment fixed, at this width.
    #[test]
    fn the_zero_point_lands_after_the_scale_and_before_the_clamp() {
        let x = vec![1i32];
        // acc = 100 · 1; m/2^shift = 1; zero = -32_760 → far negative, clamped at the rail.
        let out = a16_matmul_requant(&[100], &x, &[params_of(1, 0, -40_000)]).unwrap();
        assert_eq!(out, vec![-(A16_CODE_MAX as i32)]);
        // And a zero large enough to matter is not itself clamped before the sum.
        let out = a16_requant(&[0], &[params_of(1, 0, 40_000)]).unwrap();
        assert_eq!(out, vec![A16_CODE_MAX as i32], "the SUM clamps, not the parts");
    }

    /// **The GQA mapping is the op's, and it is the measured one**: contiguous grouping — query
    /// heads `[g·group, (g+1)·group)` read kv head `g` — with the scores and values arms
    /// agreeing with a hand-computed reference on an asymmetric fixture, and every reduction
    /// order bit-identical (Decision E at this width, attention form).
    #[test]
    fn the_attention_arms_compute_the_grouped_reduction_exactly() {
        let (heads, kv_heads, d_head, kv_len) = (4usize, 2usize, 8usize, 3usize);
        let kv_dim = kv_heads * d_head;
        let q = codes16(heads * d_head, 0xA11CE);
        let k_series = codes16(kv_len * kv_dim, 0xB0B);
        let p_par = vec![params_of(3 << 28, 40, 1); heads * kv_len];
        let scores = a16_attn_scores(&q, &k_series, heads, kv_heads, d_head, &p_par).unwrap();
        // Hand-computed: head 3 reads kv head 1 (group = 2).
        let h = 3usize;
        let j = 2usize;
        let kv_off = (h / (heads / kv_heads)) * d_head;
        let acc: i64 = (0..d_head).map(|i| q[h * d_head + i] as i64 * k_series[j * kv_dim + kv_off + i] as i64).sum();
        let want = (a16_scale_round(acc, 3 << 28, 40) + 1).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32;
        assert_eq!(scores[h * kv_len + j], want);

        let p = codes16(heads * kv_len, 0x5EED);
        let v_par = vec![params_of(-(1 << 27), 39, 0); heads * d_head];
        let values = a16_attn_values(&p, &k_series, heads, kv_heads, d_head, &v_par).unwrap();
        let i = 5usize;
        let acc: i64 = (0..kv_len).map(|j| p[h * kv_len + j] as i64 * k_series[j * kv_dim + kv_off + i] as i64).sum();
        let want = a16_scale_round(acc, -(1 << 27), 39).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32;
        assert_eq!(values[h * d_head + i], want);

        // Reduction order: blocked recompute of the scores agrees bit for bit.
        for block in [2usize, 3, 5] {
            let blocked: i64 = q[h * d_head..(h + 1) * d_head]
                .chunks(block)
                .zip(k_series[j * kv_dim + kv_off..j * kv_dim + kv_off + d_head].chunks(block))
                .map(|(a, b)| a.iter().zip(b).map(|(x, y)| *x as i64 * *y as i64).sum::<i64>())
                .sum();
            let w = (a16_scale_round(blocked, 3 << 28, 40) + 1).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32;
            assert_eq!(w, scores[h * kv_len + j], "block {block}");
        }
    }

    /// Row-wise softmax normalizes WITHIN a segment: two identical segments produce identical
    /// distributions, and the concatenation is not one distribution.
    #[test]
    fn the_rowwise_softmax_normalizes_per_segment() {
        let seg = vec![110i32, 90, 0];
        let two: Vec<i32> = seg.iter().chain(&seg).copied().collect();
        let rows = a16_softmax_rows(&two, 3, 31).unwrap();
        assert_eq!(&rows[..3], &rows[3..], "identical segments, identical distributions");
        let one = crate::palw_base0_ops::softmax_shifted(&seg, 31).unwrap();
        assert_eq!(&rows[..3], &one[..], "each segment IS op 5W");
        let whole = crate::palw_base0_ops::softmax_shifted(&two, 31).unwrap();
        assert_ne!(&rows[..], &whole[..], "and the concatenation is a different function");
        assert!(a16_softmax_rows(&two, 4, 31).is_err(), "a width that does not divide is refused");
    }

    /// **The cross-language differential digest (ADR-0047 follow-through).**
    ///
    /// `scripts/palw_a16_reference.py` implements the CLOSED-FORM A16 ops from this module's
    /// specification text — arbitrary-precision integers, the one rounding rule, the clamp —
    /// with no sight of this Rust. Both sides generate the SAME deterministic case set from the
    /// documented LCG and digest their outputs; the frozen hex below must match the script's
    /// printout. A spec a second implementation cannot reproduce from its text is a spec that
    /// says less than its code, which is precisely the failure this pins. (The seed-table
    /// primitives — IntExp/IntRsqrt/IntRecip — are NOT here: transcribing a table is
    /// replication, not independence; their refuters are the exact-arithmetic bounds in
    /// `palw_base0`.)
    #[test]
    fn the_cross_language_case_digest_is_frozen() {
        let mut out: Vec<u8> = Vec::new();
        let mut emit = |rows: &[i32]| {
            for v in rows {
                out.extend_from_slice(&v.to_le_bytes());
            }
        };
        // Case family 1: scale_round on a lattice of (x, m, shift) including signs and rails.
        let mut state = 0xA16C0DEu64 | 1;
        let mut next = || {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            state
        };
        for _ in 0..512 {
            let x = (next() >> 16) as i64 % (1 << 45);
            let m = ((next() >> 20) as i64 % (1 << 40)) - (1 << 39);
            let shift = (next() % 63) as u8;
            emit(&[(a16_scale_round(x, m, shift) & 0xFFFF_FFFF) as i32]);
        }
        // Case family 2: the fused matmuls + requant + elementwise + rope over LCG rows.
        for len in [1usize, 7, 64] {
            let x = codes16(len, 0xC0FFEE);
            let w = weights8(len * 2, 0xF00D);
            let params = vec![params_of((1 << 41) - 7, 43, 5), params_of(-((1 << 40) + 11), 42, -32_770)];
            emit(&a16_matmul_requant(&w, &x, &params).unwrap());
            emit(&a16_matmul_rescale(&w, &x, &params).unwrap());
            let operand = codes16(len * 2, 0xB16);
            emit(&a16_matmul_requant_row(&operand, &x, &params).unwrap());
            let per: Vec<A16QuantParams> = (0..len).map(|i| params_of((1 << 39) + i as i64, 40, (i as i64 % 5) - 2)).collect();
            emit(&a16_requant(&x, &per).unwrap());
            let y = codes16(len, 0x5CA1E);
            emit(&a16_add_elem(&x, &y).unwrap());
            emit(&a16_mul_elem(&x, &y).unwrap());
        }
        let x = codes16(8, 0x40FE);
        let one = 1i32 << K;
        let c45 = 11_863_283i32;
        emit(&a16_rope(&x, &[one, c45, one, c45], &[0, c45, 0, c45]).unwrap());

        let digest = blake2b_simd::Params::new().hash_length(32).to_state().update(&out).finalize();
        assert_eq!(
            digest.to_hex().to_string(),
            "488efc80bacb98aff21d76436a12bc218ba6161a7d0169927e6c506d45569086",
            "the cross-language case digest moved — the ops and scripts/palw_a16_reference.py must change together"
        );
    }

    /// The rotation matches op 4 exactly wherever op 4's output is already in range, and
    /// saturates (rather than escaping the code range) where it is not.
    #[test]
    fn the_a16_rope_is_op4_plus_the_saturation() {
        let x: Vec<i32> = vec![30_000, -20_000, 5, -7];
        let one = 1i32 << K;
        // cos=1, sin=0: the identity — both ops agree bit for bit.
        let idem = a16_rope(&x, &[one, one], &[0, 0]).unwrap();
        assert_eq!(idem, x);
        let base = crate::palw_base0_ops::rope_table(&x, &[one, one], &[0, 0]).unwrap();
        assert_eq!(idem, base);
        // A 45° rotation pushes the hot pair past ±32767 in op 4; here it saturates.
        let c45 = 11_863_283i32; // cos(π/4) in Qk
        let rot = a16_rope(&x, &[c45, c45], &[c45, c45]).unwrap();
        let unclamped = crate::palw_base0_ops::rope_table(&x, &[c45, c45], &[c45, c45]).unwrap();
        assert!(unclamped[0].abs() > A16_CODE_MAX as i32, "the raw rotation leaves the code range");
        assert_eq!(rot[0], A16_CODE_MAX as i32, "and the a16 op saturates it");
        assert_eq!(rot[2], unclamped[2], "in-range lanes are op 4 exactly");
    }

    /// The unit row is scale-invariant and bounded by √n in Qk — the property the γ requant
    /// downstream is sized against.
    #[test]
    fn the_norm_is_scale_invariant_and_bounded() {
        let a = codes16(256, 0x5EED);
        let halved: Vec<i32> = a.iter().map(|v| v / 2).collect();
        let na = a16_rms_norm(&a, 1).unwrap();
        let nh = a16_rms_norm(&halved, 1).unwrap();
        // Same direction: cosine of the two unit rows ≈ 1 (integer rounding allows tiny drift).
        let dot: i128 = na.iter().zip(&nh).map(|(x, y)| *x as i128 * *y as i128).sum();
        let (la, lh) = (na.iter().map(|v| (*v as i128).pow(2)).sum::<i128>(), nh.iter().map(|v| (*v as i128).pow(2)).sum::<i128>());
        let cos = dot as f64 / ((la as f64).sqrt() * (lh as f64).sqrt());
        assert!(cos > 0.999, "unit rows of scaled rows must align, got {cos}");
        let bound = ((256f64).sqrt() * (1i64 << K) as f64 * 1.01) as i64;
        assert!(na.iter().all(|v| (*v as i64).abs() < bound), "|unit| ≤ √n in Qk");
    }
}
