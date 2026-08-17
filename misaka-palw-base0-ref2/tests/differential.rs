//! The bit-exact differential between `kaspa_consensus_core::palw_base0` and the second
//! implementation.
//!
//! # Exact equality, never a tolerance
//!
//! Every assertion here is `==`. A tolerance would defeat the purpose: ADR-0040's whole claim is
//! that two conforming implementations agree *bit for bit*, and ADR-0027's court converts a
//! single-unit disagreement into a conviction and a slashed bond. A differential that accepted
//! "close" would pass on precisely the defects that matter.
//!
//! # Negatives are enumerated deliberately
//!
//! Both defects this crate found were negative-input-only, and both survived the first
//! implementation's own tests because those tests used positive values. So the sampling here is
//! not uniform-and-hope: every case sweeps signs explicitly, small ranges are exhaustive, and the
//! boundary tables include the extremes of every type involved.

use kaspa_consensus_core::palw_base0 as spec;
use misaka_palw_base0_ref2 as ref2;

/// A deterministic LCG. Not for statistical quality — for reproducibility: a failing seed is a
/// failing seed on every machine and in every future run, which a thread-seeded RNG would not be.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        // The high bits: an LCG's low bits have short periods and would under-sample the sign.
        self.0.rotate_right(24)
    }
    fn i32(&mut self) -> i32 {
        self.next_u64() as u32 as i32
    }
    fn i64(&mut self) -> i64 {
        self.next_u64() as i64
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// The values most likely to separate two implementations: type limits, powers of two and their
/// neighbours, and the sign boundary.
fn boundary_i32() -> Vec<i32> {
    let mut v = vec![0, 1, -1, 2, -2, 3, -3, i32::MAX, i32::MIN, i32::MAX - 1, i32::MIN + 1];
    for bit in 0..31u32 {
        let p = 1i32 << bit;
        v.extend_from_slice(&[p, p - 1, p + 1, -p, -p - 1, -p + 1]);
    }
    v
}

fn boundary_i64() -> Vec<i64> {
    let mut v = vec![0, 1, -1, 2, -2, i64::MAX, i64::MIN, i64::MAX - 1, i64::MIN + 1];
    for bit in 0..62u32 {
        let p = 1i64 << bit;
        v.extend_from_slice(&[p, p - 1, p + 1, -p, -p - 1, -p + 1]);
    }
    v
}

/// `RoundingShiftRight`: exhaustive on a small window across every shift, then the boundary table,
/// then a large random sweep.
///
/// The exhaustive window is what would have caught defect 2 on the first run: `RSR(−64, 1)` is
/// inside it, and the two forms differ there by one.
#[test]
fn rounding_shift_right_agrees_bit_for_bit() {
    for s in 0..=31u8 {
        for x in -2048..=2048i32 {
            assert_eq!(spec::rounding_shift_right(x, s), ref2::ref2_rounding_shift_right(x, s), "RSR({x}, {s})");
        }
    }
    for x in boundary_i32() {
        for s in 0..=31u8 {
            assert_eq!(spec::rounding_shift_right(x, s), ref2::ref2_rounding_shift_right(x, s), "RSR({x}, {s})");
        }
    }
    let mut rng = Lcg::new(0x5EED_0001);
    for _ in 0..400_000 {
        let (x, s) = (rng.i32(), rng.below(32) as u8);
        assert_eq!(spec::rounding_shift_right(x, s), ref2::ref2_rounding_shift_right(x, s), "RSR({x}, {s})");
    }
}

/// The 64-bit rule, same treatment. Its shift range reaches 62, where the nudge is larger than
/// most operands and a wrapping formulation shows up immediately.
#[test]
fn rounding_shift_right_64_agrees_bit_for_bit() {
    for s in 0..=62u8 {
        for x in -1024..=1024i64 {
            assert_eq!(spec::rounding_shift_right_64(x, s), ref2::ref2_rounding_shift_right_64(x, s), "RSR64({x}, {s})");
        }
    }
    for x in boundary_i64() {
        for s in 0..=62u8 {
            assert_eq!(spec::rounding_shift_right_64(x, s), ref2::ref2_rounding_shift_right_64(x, s), "RSR64({x}, {s})");
        }
    }
    let mut rng = Lcg::new(0x5EED_0002);
    for _ in 0..400_000 {
        let (x, s) = (rng.i64(), rng.below(63) as u8);
        assert_eq!(spec::rounding_shift_right_64(x, s), ref2::ref2_rounding_shift_right_64(x, s), "RSR64({x}, {s})");
    }
}

/// `SRDHM`: the primitive where defect 1 lived, and the one ADR-0040 C2 justifies by pointing at
/// other codebases — so the one where a third party is most likely to disagree.
///
/// The `MIN × MIN` saturating case is checked explicitly: it is the only input where the two
/// implementations agree by a shared early return rather than by arithmetic, so agreement there
/// is not evidence and must be pinned separately.
#[test]
fn srdhm_agrees_bit_for_bit() {
    assert_eq!(spec::srdhm(i32::MIN, i32::MIN), i32::MAX);
    assert_eq!(ref2::ref2_srdhm(i32::MIN, i32::MIN), i32::MAX);

    let boundary = boundary_i32();
    for &a in boundary.iter() {
        for &b in boundary.iter() {
            assert_eq!(spec::srdhm(a, b), ref2::ref2_srdhm(a, b), "SRDHM({a}, {b})");
        }
    }
    // Exhaustive over a small signed window — every sign combination, including the exact-half
    // products where truncation and floor part company.
    for a in -300..=300i32 {
        for b in -300..=300i32 {
            assert_eq!(spec::srdhm(a, b), ref2::ref2_srdhm(a, b), "SRDHM({a}, {b})");
        }
    }
    // The exact-half family: a·b = ±2^30·n makes the nudge land precisely on the boundary.
    for n in -64..=64i32 {
        for shift in 0..=15u32 {
            let a = n.saturating_mul(1 << shift);
            let b = 1 << (30 - shift.min(30));
            assert_eq!(spec::srdhm(a, b), ref2::ref2_srdhm(a, b), "SRDHM({a}, {b})");
        }
    }
    let mut rng = Lcg::new(0x5EED_0003);
    for _ in 0..500_000 {
        let (a, b) = (rng.i32(), rng.i32());
        assert_eq!(spec::srdhm(a, b), ref2::ref2_srdhm(a, b), "SRDHM({a}, {b})");
    }
}

/// `Requantize` composes both repaired primitives, so it is where a partial fix would show.
#[test]
fn requantize_agrees_bit_for_bit() {
    for &acc in boundary_i32().iter() {
        for &mult in [i32::MAX, i32::MIN, 1 << 30, -(1 << 30), 1, -1, 0].iter() {
            for shift in 0..=31u8 {
                assert_eq!(
                    spec::requantize(acc, mult, shift),
                    ref2::ref2_requantize(acc, mult, shift),
                    "Requantize({acc}, {mult}, {shift})"
                );
            }
        }
    }
    let mut rng = Lcg::new(0x5EED_0004);
    for _ in 0..400_000 {
        let (acc, mult, shift) = (rng.i32(), rng.i32(), rng.below(32) as u8);
        assert_eq!(
            spec::requantize(acc, mult, shift),
            ref2::ref2_requantize(acc, mult, shift),
            "Requantize({acc}, {mult}, {shift})"
        );
    }
}

/// `Rescale` — ADR-0040 H's op 9, including its `i32` saturation at both ends.
#[test]
fn rescale_agrees_bit_for_bit() {
    for &acc in boundary_i32().iter() {
        for &mult in [i32::MAX, i32::MIN, 1 << 30, -(1 << 30), 1, -1, 0].iter() {
            for shift in 0..=spec::RESCALE_MAX_SHIFT {
                assert_eq!(
                    spec::rescale_q(acc, mult, shift),
                    ref2::ref2_rescale_q(acc, mult, shift),
                    "Rescale({acc}, {mult}, {shift})"
                );
            }
        }
    }
    let mut rng = Lcg::new(0x5EED_0005);
    for _ in 0..400_000 {
        let (acc, mult, shift) = (rng.i32(), rng.i32(), rng.below(spec::RESCALE_MAX_SHIFT as u64 + 1) as u8);
        assert_eq!(
            spec::rescale_q(acc, mult, shift),
            ref2::ref2_rescale_q(acc, mult, shift),
            "Rescale({acc}, {mult}, {shift})"
        );
    }
}

/// `IntExp` over its entire meaningful domain, exhaustively at the ends and densely in between.
///
/// The domain is `x ≤ 0` down to `−Z_MAX·ln2`, past which the result is 0 — so "exhaustive at the
/// ends" means both the near-zero region, where `Poly2` does the work, and the cutoff, where the
/// range-reduction count saturates.
#[test]
fn int_exp_agrees_bit_for_bit() {
    for x in -4096..=4096i32 {
        assert_eq!(spec::int_exp(x), ref2::ref2_int_exp(x), "IntExp({x})");
    }
    // Every range-reduction bucket, and both sides of each boundary.
    for z in 0..=spec::Z_MAX + 2 {
        for delta in -3..=3i32 {
            let x = -(z.saturating_mul(spec::LN2_Q)).saturating_add(delta);
            assert_eq!(spec::int_exp(x), ref2::ref2_int_exp(x), "IntExp({x}) at z={z}");
        }
    }
    for &x in boundary_i32().iter() {
        assert_eq!(spec::int_exp(x), ref2::ref2_int_exp(x), "IntExp({x})");
    }
    let mut rng = Lcg::new(0x5EED_0006);
    for _ in 0..400_000 {
        let x = rng.i32();
        assert_eq!(spec::int_exp(x), ref2::ref2_int_exp(x), "IntExp({x})");
        // And densely inside the domain the op is actually used on.
        let inside = -((rng.below(spec::Z_MAX as u64 * spec::LN2_Q as u64)) as i32);
        assert_eq!(spec::int_exp(inside), ref2::ref2_int_exp(inside), "IntExp({inside})");
    }
}

/// `IntRsqrt`, including the seed-basin boundary that returned 0 in an earlier draft.
///
/// The mantissa decomposition is compared independently of the final value: two implementations
/// can agree on `1/√v` while disagreeing about `(mantissa, exponent)` for inputs where the error
/// happens to cancel, and the decomposition is what a bisection would land on.
#[test]
fn int_rsqrt_agrees_bit_for_bit() {
    for v in 1..=20_000i64 {
        assert_eq!(spec::int_rsqrt(v), ref2::ref2_int_rsqrt(v), "IntRsqrt({v})");
    }
    // Non-positive inputs are defined as 0 rather than left to diverge.
    for v in [0i64, -1, -1000, i64::MIN] {
        assert_eq!(spec::int_rsqrt(v), 0);
        assert_eq!(ref2::ref2_int_rsqrt(v), 0);
    }
    // Exact powers of four are where the normalisation loop and a `leading_zeros` computation are
    // most likely to disagree about which side of the boundary they are on.
    for e in 0..30u32 {
        for delta in -2..=2i64 {
            let v = (1i64 << (2 * e)).saturating_add(delta);
            if v > 0 {
                assert_eq!(spec::int_rsqrt(v), ref2::ref2_int_rsqrt(v), "IntRsqrt({v}) at 4^{e}");
                assert!(ref2::primitives::ref2_normalize(v).is_some());
            }
        }
    }
    let mut rng = Lcg::new(0x5EED_0007);
    for _ in 0..300_000 {
        let v = (rng.i64().abs() % (1i64 << 50)).max(1);
        assert_eq!(spec::int_rsqrt(v), ref2::ref2_int_rsqrt(v), "IntRsqrt({v})");
        assert_eq!(spec::int_recip(v), ref2::ref2_int_recip(v), "IntRecip({v})");
    }
}

/// The mantissa of a normalised value must land in `[1, 4)` and reconstruct the input. Checked on
/// the second implementation because it is the one that normalises by a loop; if the loop were
/// off by a step the reconstruction would fail here rather than showing up as a wrong reciprocal
/// three Newton iterations later.
#[test]
fn the_mantissa_decomposition_is_well_formed() {
    let one = spec::ONE as i128;
    let mut rng = Lcg::new(0x5EED_0008);
    for _ in 0..50_000 {
        let v = (rng.i64().abs() % (1i64 << 50)).max(1);
        let (mantissa, exponent) = ref2::primitives::ref2_normalize(v).expect("v > 0");
        assert!((one..4 * one).contains(&mantissa), "mantissa {mantissa} outside [1, 4) for v={v}");
        // v ≈ mantissa · 4^exponent, within the floor divisions the normalisation performs.
        let reconstructed = if exponent >= 0 {
            mantissa * 4i128.pow(exponent as u32)
        } else {
            mantissa / 4i128.pow(exponent.unsigned_abs())
        };
        let error = (reconstructed - v as i128).abs();
        assert!(error * 1000 <= v as i128 + 1000, "v={v} reconstructed to {reconstructed}");
    }
}

/// The differential must be able to fail. Every test above asserts equality, so a harness that
/// silently compared a function with itself — a copy-paste of the spec path into both sides — would
/// pass everything and prove nothing.
///
/// This pins the two defects the differential actually found, as the *wrong* answers, so the
/// harness is demonstrably sensitive to exactly the class of error it exists to detect.
#[test]
fn the_differential_can_distinguish_the_defects_it_found() {
    // Defect 2: `(x - 2^(s-1)) >> s`, the form ADR-0040 C1's pseudocode used.
    let shift_form = |x: i32, s: u8| -> i32 {
        if s == 0 {
            return x;
        }
        let round = 1i32 << (s - 1);
        (x.wrapping_add(if x >= 0 { round } else { -round })) >> s
    };
    assert_eq!(shift_form(-64, 1), -33, "the defective form is being reproduced faithfully");
    assert_eq!(spec::rounding_shift_right(-64, 1), -32);
    assert_eq!(ref2::ref2_rounding_shift_right(-64, 1), -32);
    let disagreements = (-2048..=2048i32)
        .flat_map(|x| (1..=8u8).map(move |s| (x, s)))
        .filter(|(x, s)| shift_form(*x, *s) != spec::rounding_shift_right(*x, *s))
        .count();
    assert!(disagreements > 4000, "the defective form should differ on roughly every negative input, got {disagreements}");

    // Defect 1: gemmlowp's nudge paired with a floor instead of a truncation.
    let floor_srdhm = |a: i32, b: i32| -> i32 {
        if a == i32::MIN && b == i32::MIN {
            return i32::MAX;
        }
        let product = (a as i64) * (b as i64);
        let nudge: i64 = if product >= 0 { 1 << 30 } else { 1 - (1 << 30) };
        ((product + nudge) >> 31) as i32
    };
    assert_eq!(floor_srdhm(-(1 << 30), 1 << 30), -(1 << 29) - 1, "the defective form is being reproduced faithfully");
    assert_eq!(spec::srdhm(-(1 << 30), 1 << 30), -(1 << 29));
    assert_eq!(ref2::ref2_srdhm(-(1 << 30), 1 << 30), -(1 << 29));
    let mut rng = Lcg::new(0x5EED_0009);
    let mut differed = 0;
    for _ in 0..10_000 {
        let (a, b) = (rng.i32(), rng.i32());
        if floor_srdhm(a, b) != spec::srdhm(a, b) {
            differed += 1;
        }
    }
    assert!(differed > 4000, "the defective SRDHM should differ on about half of all inputs, got {differed}");
}
