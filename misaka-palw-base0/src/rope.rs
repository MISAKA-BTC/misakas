//! The pinned integer rotary table (ADR-0040 Decision D).
//!
//! # Why this file exists at all
//!
//! ADR-0040 removed `sinf`/`cosf` from the BASE-0 op catalog and replaced them with a table
//! lookup, because a per-step transcendental is the one op whose value cannot be pinned by a
//! ruleset — every libm rounds it differently. The table is what replaced them, so the table has
//! to come from somewhere, and that somewhere is here.
//!
//! # Generation is integer-only, and that is not decoration
//!
//! ADR-0040 permits a transcendental evaluated **at registration** to be treated as data: the
//! artifact is hash-pinned, so nothing downstream re-derives it. Under that permission this file
//! could have called `f64::exp` and `f64::sin`.
//!
//! It does not, for a reason that shows up the first time someone disagrees about an artifact.
//! A float generator makes the table reproducible only by *distribution*: you can check the hash
//! you were handed, but you cannot independently rebuild the artifact and get the same bytes,
//! because `exp` and `sin` are not bit-identical across libms or `-ffast-math` settings. Every
//! dispute then bottoms out in "trust the published blob". Generating with integers instead makes
//! the artifact reproducible by *derivation*: anyone, on any machine, rebuilds byte-identical
//! bytes from the shape parameters alone, and a disagreement about the table becomes a
//! disagreement about a short integer program rather than about whose libm is authoritative.
//!
//! # The two precisions here are different on purpose
//!
//! * The **runtime** Q is [`kaspa_consensus_core::palw_base0::K`] = 24 — the table's stored width.
//! * **Generation** works at Q48 internally ([`GEN_Q`]) and only narrows at the end.
//!
//! The reason is that generation must be faithful to the float definition
//! `inv_freq_i = θ^(−2i/d)` that pretrained weights were trained against. Computing that with the
//! consensus [`kaspa_consensus_core::palw_base0::int_exp`] — which exists to be cheap and
//! two-implementable, not accurate — lands 3.1e−3 off in relative terms, which at position 4095
//! is a rotation error of whole radians. The Q48 series here lands 1.1e−11 off instead. This is a
//! *quality* argument, not a consensus one: any pinned table is equally consensus-safe, and a
//! wrong one is merely a class whose model does not work.
//!
//! # What is NOT here
//!
//! Nothing in this module is on the block-validation path. The consensus code reads
//! [`RopeTableV1::cos_q`]/[`sin_q`] as opaque pinned data via
//! `kaspa_consensus_core::palw_base0_ops::rope_table`. If this file were deleted after an artifact
//! were registered, every existing block would still validate.

use kaspa_consensus_core::palw_base0::K;

/// Working scale for generation. Wider than the runtime Q so the narrowing step is the only
/// rounding that survives into the artifact.
pub const GEN_Q: u32 = 48;
const GEN_ONE: i128 = 1i128 << GEN_Q;

/// `ln 2` at [`GEN_Q`]. Used for range reduction, not by the runtime — the runtime's own `ln 2`
/// is `palw_base0::LN2_Q` at Q24 and the two are deliberately separate constants.
pub const LN2_GEN_Q: i128 = 195_103_586_505_167;

/// `2π` at [`GEN_Q`], for reducing a rotation angle before the CORDIC.
pub const TWO_PI_GEN_Q: i128 = 1_768_559_438_007_110;

/// `π/2` at [`GEN_Q`], the quadrant width.
pub const HALF_PI_GEN_Q: i128 = 442_139_859_501_778;

/// `1/Π√(1+2^−2i)` over [`CORDIC_ITERS`] iterations, at Q[`K`] — the CORDIC gain, pre-divided out
/// so the rotation loop needs no final scaling.
pub const CORDIC_INV_GAIN: i64 = 10_188_014;

/// CORDIC rotations. 24 is not arbitrary: the i-th rotation contributes `atan(2^−i)`, and at
/// i = 24 that is `2^−24`, exactly the runtime resolution. Further rotations would move nothing.
pub const CORDIC_ITERS: usize = 24;

/// `atan(2^−i)` at Q[`K`] for i in 0..[`CORDIC_ITERS`].
pub const ATAN_Q: [i64; CORDIC_ITERS] = [
    13_176_795, 7_778_716, 4_110_060, 2_086_331, 1_047_214, 524_117, 262_123, 131_069, 65_536, 32_768, 16_384, 8_192, 4_096,
    2_048, 1_024, 512, 256, 128, 64, 32, 16, 8, 4, 2,
];

/// Why a table can be refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RopeGenError {
    /// `d_head` must be even — RoPE rotates *pairs*, so an odd width has a component with no
    /// partner. Refused rather than silently dropping the last component.
    OddHeadDim { got: usize },
    /// A zero dimension or zero position count has no table; refused rather than returning an
    /// empty one that would later read as "generated, nothing in it".
    Empty,
    /// The table would exceed what a registration artifact may carry. See [`MAX_TABLE_ENTRIES`].
    TooLarge { entries: u128 },
}

/// Ceiling on `max_position × d_head/2`. Chosen so a table is bounded well below the point where
/// `usize` arithmetic on the flattened index could be in question on a 32-bit target, and so a
/// malformed shape is refused before allocating.
pub const MAX_TABLE_ENTRIES: u128 = 1 << 28;

/// `exp(x)` for `x ≤ 0` at [`GEN_Q`], by range reduction to `(−ln2, 0]` and a Maclaurin series.
///
/// Generation-time only. The series runs to a fixed 18 terms with an early exit once a term
/// underflows the scale: at `|r| < ln 2` the 18th term is below `2^−48`, so the fixed bound is
/// reached only by the early exit and the loop count never depends on the value in a way that
/// could differ between builds.
pub fn exp_nonpos_gen_q(x: i128) -> i128 {
    if x > 0 {
        return GEN_ONE;
    }
    let z = (-x) / LN2_GEN_Q;
    if z >= 96 {
        // Below anything representable at Q48 once halved z times.
        return 0;
    }
    let r = x + z * LN2_GEN_Q; // r ∈ (−ln2, 0]
    let mut term = GEN_ONE;
    let mut acc = GEN_ONE;
    for n in 1..=18i128 {
        term = (term * r) / GEN_ONE / n;
        if term == 0 {
            break;
        }
        acc += term;
    }
    acc >> z
}

/// `θ^(−2i/d)` at [`GEN_Q`], as `exp(−(2i/d)·ln θ)`.
///
/// `ln_theta_gen_q` is carried by the shape rather than hardcoded so a class may pin a base other
/// than 10000 without a code change — the artifact records which one it used.
pub fn inv_freq_gen_q(i: usize, d_head: usize, ln_theta_gen_q: i128) -> i128 {
    debug_assert!(d_head > 0);
    exp_nonpos_gen_q(-((2 * i as i128) * ln_theta_gen_q) / d_head as i128)
}

/// `(cos z, sin z)` at Q[`K`] for `z ∈ [0, π/2]` given at Q[`K`], by CORDIC in circular rotation
/// mode.
///
/// The domain bound is real: CORDIC converges only for `|z| ≤ Σ atan(2^−i) ≈ 1.7433`, and π/2 is
/// 1.5708. Callers reach it through [`cos_sin_q`], which folds the quadrant first.
pub fn cordic_cos_sin_q(z_q: i64) -> (i64, i64) {
    let (mut x, mut y, mut z) = (CORDIC_INV_GAIN, 0i64, z_q);
    for (i, atan) in ATAN_Q.iter().enumerate() {
        let d = if z >= 0 { 1 } else { -1 };
        let (nx, ny) = (x - d * (y >> i), y + d * (x >> i));
        x = nx;
        y = ny;
        z -= d * atan;
    }
    (x, y)
}

/// `(cos θ, sin θ)` at Q[`K`] for an arbitrary non-negative angle given at [`GEN_Q`].
///
/// Reduction is mod 2π at Q48 and then by quadrant, because CORDIC's convergence domain is
/// narrower than a full turn. The quadrant map is the ordinary one — folding rather than
/// extending the rotation table keeps the iteration count fixed for every angle.
pub fn cos_sin_q(angle_gen_q: i128) -> (i64, i64) {
    let a = angle_gen_q.rem_euclid(TWO_PI_GEN_Q);
    if a == 0 {
        // Exact, not approximated. CORDIC lands ~1e−6 short of unit magnitude, which for every
        // other angle is below the table's resolution — but a position-0 row that is not exactly
        // (1, 0) rotates the first token of every sequence by a small fixed amount, and that error
        // is systematic rather than random. `cos 0 = 1` and `sin 0 = 0` are known exactly, so the
        // table stores them exactly.
        return (1i64 << K, 0);
    }
    let quadrant = a / HALF_PI_GEN_Q;
    let rem = a - quadrant * HALF_PI_GEN_Q;
    let (c, s) = cordic_cos_sin_q((rem >> (GEN_Q - K)) as i64);
    match quadrant {
        0 => (c, s),
        1 => (-s, c),
        2 => (-c, -s),
        _ => (s, -c),
    }
}

/// A generated rotary table: `max_position × (d_head/2)` cosines and sines at Q[`K`], flattened
/// row-major by position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RopeTableV1 {
    pub d_head: usize,
    pub max_position: usize,
    /// Row-major `[position][pair]`, Q[`K`].
    pub cos_q: Vec<i32>,
    /// Row-major `[position][pair]`, Q[`K`].
    pub sin_q: Vec<i32>,
}

impl RopeTableV1 {
    /// Derive the table from the shape parameters alone. Deterministic on every target: no float
    /// is evaluated anywhere in the call graph.
    pub fn generate(d_head: usize, max_position: usize, ln_theta_gen_q: i128) -> Result<Self, RopeGenError> {
        if d_head == 0 || max_position == 0 {
            return Err(RopeGenError::Empty);
        }
        if !d_head.is_multiple_of(2) {
            return Err(RopeGenError::OddHeadDim { got: d_head });
        }
        let pairs = d_head / 2;
        let entries = (max_position as u128) * (pairs as u128);
        if entries > MAX_TABLE_ENTRIES {
            return Err(RopeGenError::TooLarge { entries });
        }
        let mut cos_q = Vec::with_capacity(entries as usize);
        let mut sin_q = Vec::with_capacity(entries as usize);
        // Hoisted: the inverse frequencies depend on the pair index only, so the series runs
        // `pairs` times rather than `max_position × pairs` times.
        let inv: Vec<i128> = (0..pairs).map(|i| inv_freq_gen_q(i, d_head, ln_theta_gen_q)).collect();
        for pos in 0..max_position {
            for &iv in inv.iter() {
                let (c, s) = cos_sin_q(pos as i128 * iv);
                cos_q.push(c as i32);
                sin_q.push(s as i32);
            }
        }
        Ok(Self { d_head, max_position, cos_q, sin_q })
    }

    /// The `(cos, sin)` half-rows for one position, in the layout
    /// `palw_base0_ops::rope_table` expects.
    pub fn row(&self, position: usize) -> Option<(&[i32], &[i32])> {
        if position >= self.max_position {
            return None;
        }
        let pairs = self.d_head / 2;
        let lo = position * pairs;
        let hi = lo + pairs;
        Some((&self.cos_q[lo..hi], &self.sin_q[lo..hi]))
    }

    /// Bytes fed to the artifact digest. Little-endian and length-prefixed so two tables of
    /// different shapes can never produce the same bytes.
    pub fn digest_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + 8 * self.cos_q.len());
        out.extend_from_slice(&(self.d_head as u64).to_le_bytes());
        out.extend_from_slice(&(self.max_position as u64).to_le_bytes());
        // Length-prefix the entries, and prefix BOTH lengths. `zip` stops at the shorter of the two
        // vectors, so a table whose `cos_q` and `sin_q` disagree in length would otherwise hash the
        // same bytes as a correctly-sized shorter table — the truncated tail simply vanishes from
        // the digest. The class id must distinguish those: a malformed artifact has to be a
        // DIFFERENT class, not an alias of a well-formed one (mainnet-readiness audit 2.4).
        out.extend_from_slice(&(self.cos_q.len() as u64).to_le_bytes());
        out.extend_from_slice(&(self.sin_q.len() as u64).to_le_bytes());
        for (c, s) in self.cos_q.iter().zip(self.sin_q.iter()) {
            out.extend_from_slice(&c.to_le_bytes());
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LN_10000_GEN_Q: i128 = 2_592_480_341_699_211;

    /// The property that makes the table a *rotation*: every entry sits on the unit circle.
    /// A generator that drifted off it would still produce a self-consistent pinned table, so
    /// this is the check that separates "deterministic" from "correct".
    #[test]
    fn every_entry_is_on_the_unit_circle() {
        let t = RopeTableV1::generate(64, 512, LN_10000_GEN_Q).unwrap();
        let one = 1i64 << K;
        for (c, s) in t.cos_q.iter().zip(t.sin_q.iter()) {
            let n = (*c as i64) * (*c as i64) + (*s as i64) * (*s as i64);
            let dev = (n - one * one).abs();
            // MEASURED, not guessed: the CORDIC's magnitude error is dominated by the truncation
            // in each `x >> i`, and comes out at 1.1e−6 relative. 2^−18 of unit² is 3.8e−6, which
            // is loose enough not to be brittle and tight enough that a real magnitude bug — a
            // wrong gain constant is a percent, a missing iteration is 1e−4 — still fails.
            assert!(dev < (one * one) >> 18, "cos²+sin² off the circle: c={c} s={s} dev={dev}");
        }
    }

    /// Position 0 is the identity rotation. If it were not, every sequence would start rotated and
    /// the error would be invisible in any relative check.
    #[test]
    fn position_zero_is_the_identity() {
        let t = RopeTableV1::generate(32, 8, LN_10000_GEN_Q).unwrap();
        let (c, s) = t.row(0).unwrap();
        assert!(c.iter().all(|&c| c == 1 << K), "cos(0) must be exactly 1: {c:?}");
        assert!(s.iter().all(|&s| s == 0), "sin(0) must be exactly 0: {s:?}");
    }

    /// Rotations compose: the angle at position `p` is `p` times the angle at position 1, so
    /// `cos(2θ) = 2cos²θ − 1` must hold between rows 1 and 2. This catches a generator that
    /// produced smooth, plausible, but wrongly-scaled angles — which a unit-circle check cannot.
    #[test]
    fn rows_compose_as_multiples_of_the_base_angle() {
        let t = RopeTableV1::generate(64, 16, LN_10000_GEN_Q).unwrap();
        let one = 1i64 << K;
        let (c1, s1) = t.row(1).unwrap();
        let (c2, s2) = t.row(2).unwrap();
        for i in 0..c1.len() {
            let (c, s) = (c1[i] as i64, s1[i] as i64);
            let want_c = (2 * c * c) / one - one;
            let want_s = (2 * s * c) / one;
            assert!((want_c - c2[i] as i64).abs() < 64, "cos double-angle failed at pair {i}");
            assert!((want_s - s2[i] as i64).abs() < 64, "sin double-angle failed at pair {i}");
        }
    }

    /// The frequencies must actually decay across pairs, and the first must be exactly 1 — that
    /// is what makes low pairs rotate fast and high pairs slow. A generator with the exponent
    /// sign flipped passes both tests above and fails this one.
    #[test]
    fn inverse_frequencies_decay_from_one() {
        let d = 64;
        assert_eq!(inv_freq_gen_q(0, d, LN_10000_GEN_Q), GEN_ONE, "θ^0 must be exactly 1");
        let mut prev = i128::MAX;
        for i in 0..d / 2 {
            let f = inv_freq_gen_q(i, d, LN_10000_GEN_Q);
            assert!(f < prev, "frequencies must strictly decay; pair {i} did not");
            prev = f;
        }
        // The last pair is θ^(−(d−2)/d) ≈ 1/θ, within the series' own error.
        let last = inv_freq_gen_q(d / 2 - 1, d, LN_10000_GEN_Q) as f64 / GEN_ONE as f64;
        assert!((last - 10000f64.powf(-(d as f64 - 2.0) / d as f64)).abs() < 1e-9);
    }

    /// Generation is a pure function of the shape: two builds agree byte for byte. This is the
    /// property the whole integer-generator argument is for.
    #[test]
    fn generation_is_reproducible() {
        let a = RopeTableV1::generate(48, 100, LN_10000_GEN_Q).unwrap();
        let b = RopeTableV1::generate(48, 100, LN_10000_GEN_Q).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.digest_bytes(), b.digest_bytes());
    }

    /// A different θ is a different table. Otherwise the shape's θ field would be decorative and
    /// two classes claiming different bases would share an artifact hash.
    #[test]
    fn theta_changes_the_table() {
        let a = RopeTableV1::generate(32, 64, LN_10000_GEN_Q).unwrap();
        let b = RopeTableV1::generate(32, 64, LN_10000_GEN_Q / 2).unwrap();
        assert_ne!(a.digest_bytes(), b.digest_bytes());
    }

    /// Malformed shapes are refused, not rounded into something plausible. An odd head dim would
    /// otherwise silently drop a component from every rotation.
    #[test]
    fn malformed_shapes_are_refused() {
        assert_eq!(RopeTableV1::generate(33, 8, LN_10000_GEN_Q), Err(RopeGenError::OddHeadDim { got: 33 }));
        assert_eq!(RopeTableV1::generate(0, 8, LN_10000_GEN_Q), Err(RopeGenError::Empty));
        assert_eq!(RopeTableV1::generate(8, 0, LN_10000_GEN_Q), Err(RopeGenError::Empty));
        assert!(matches!(RopeTableV1::generate(64, 1 << 26, LN_10000_GEN_Q), Err(RopeGenError::TooLarge { .. })));
    }

    /// A position outside the table is unanswerable rather than wrapping to a valid-looking row.
    /// Wrapping would let a sequence longer than `max_position` execute with silently reused
    /// rotations instead of failing.
    #[test]
    fn an_out_of_range_position_has_no_row() {
        let t = RopeTableV1::generate(16, 4, LN_10000_GEN_Q).unwrap();
        assert!(t.row(3).is_some());
        assert!(t.row(4).is_none());
        assert!(t.row(usize::MAX).is_none());
    }

    /// The quadrant fold must be continuous: crossing π/2, π and 3π/2 may not produce a jump.
    /// A wrong entry in the quadrant map is invisible to every other test here, because each
    /// quadrant is internally self-consistent.
    #[test]
    fn the_quadrant_fold_is_continuous() {
        let step = HALF_PI_GEN_Q / 1000;
        let mut prev = cos_sin_q(0);
        for i in 1..4200i128 {
            let cur = cos_sin_q(i * step);
            let dc = (cur.0 - prev.0).abs();
            let ds = (cur.1 - prev.1).abs();
            // One step is π/2000 ≈ 1.6e−3 rad, so neither component may move more than ~2e−3.
            let bound = (1i64 << K) / 400;
            assert!(dc < bound && ds < bound, "jump at step {i}: {prev:?} -> {cur:?}");
            prev = cur;
        }
    }

    /// A full turn returns to the start, which is what the mod-2π reduction is for. Without it
    /// large positions would leave CORDIC's convergence domain and return garbage that still
    /// looks like a number.
    #[test]
    fn a_full_turn_returns_to_the_start() {
        let at_zero = cos_sin_q(0);
        for turns in 1..8i128 {
            let after = cos_sin_q(turns * TWO_PI_GEN_Q);
            assert!((after.0 - at_zero.0).abs() < 64 && (after.1 - at_zero.1).abs() < 64, "turn {turns} did not close: {after:?}");
        }
    }
}
