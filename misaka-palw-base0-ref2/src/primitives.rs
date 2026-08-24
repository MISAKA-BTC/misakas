//! The seven primitives, re-derived. **No `>>`, no `<<`, no `leading_zeros` below this line.**
//!
//! Every value is carried in `i128` so no intermediate can overflow, and every scaling names its
//! rounding direction: [`floor_div`] for the spec's truncating shifts, [`trunc_div`] for the one
//! place gemmlowp truncates toward zero, and an explicit magnitude-then-sign form for
//! round-half-away.
//!
//! Where the specification itself says "arithmetic shift", this file writes `floor_div` — the
//! same function, named. That is deliberate: it forces every such site to be *read* as a floor
//! and answered for, rather than inherited from an operator that does it silently.
//!
//! The rule is **enforced, not merely stated**: [`tests::the_no_shift_discipline_holds`] reads this
//! file's own source and fails if a shift operator appears in any code line. An independence
//! argument that lives only in a doc comment decays on the first edit that finds a shift
//! convenient — and it was already one line from decaying when the check was added.

/// The pinned constants, **re-declared as literals rather than imported**.
///
/// Importing them from the first implementation was this crate's first draft, and the mutation
/// sweep showed what it cost: changing `RSQRT_ITERS` from 3 to 2 moved both sides together and the
/// differential stayed silent. A second implementation that shares the specification's numbers can
/// only check the *code*, never the *constants* — and ADR-0040 F2 is explicit that the iteration
/// count and the seed table are part of the specification, not tuning.
///
/// [`the_pinned_constants_agree`](crate::primitives::consts::the_pinned_constants_agree) compares
/// these against the first implementation, so a changed constant fails loudly and by name instead
/// of surfacing as an unexplained value mismatch three functions away.
pub mod consts {
    pub const K: u32 = 24;
    pub const ONE: i128 = 2i128.pow(K);
    pub const LN2_Q: i128 = 11_629_080;
    pub const POLY2_A: i128 = 6_014_632;
    pub const POLY2_B: i128 = 22_699_573;
    pub const POLY2_C: i128 = 5_771_362;
    pub const Z_MAX: i128 = 31;
    pub const RSQRT_ITERS: u32 = 3;
    pub const RESCALE_MAX_SHIFT: u8 = 62;
    pub const RSQRT_SEED: [i128; 16] = [
        15_395_829, 14_307_657, 13_421_772, 12_682_383, 12_053_107, 11_509_075, 11_032_629, 10_610_843, 10_234_005, 9_894_662,
        9_586_980, 9_306_325, 9_048_957, 8_811_825, 8_592_409, 8_388_608,
    ];

    /// Every constant this crate re-declared must still be the one the specification pins.
    ///
    /// This is the ONE place the two implementations are allowed to touch, and it is a comparison
    /// rather than an import: a specification change has to be acknowledged here before the
    /// differential will run at all.
    #[test]
    pub fn the_pinned_constants_agree() {
        use kaspa_consensus_core::palw_base0 as spec;
        assert_eq!(K, spec::K, "K");
        assert_eq!(ONE, spec::ONE as i128, "ONE");
        assert_eq!(LN2_Q, spec::LN2_Q as i128, "LN2_Q");
        assert_eq!(POLY2_A, spec::POLY2_A as i128, "POLY2_A");
        assert_eq!(POLY2_B, spec::POLY2_B as i128, "POLY2_B");
        assert_eq!(POLY2_C, spec::POLY2_C as i128, "POLY2_C");
        assert_eq!(Z_MAX, spec::Z_MAX as i128, "Z_MAX");
        assert_eq!(RSQRT_ITERS, spec::RSQRT_ITERS, "RSQRT_ITERS — ADR-0040 F2 pins this at 3");
        assert_eq!(RESCALE_MAX_SHIFT, spec::RESCALE_MAX_SHIFT, "RESCALE_MAX_SHIFT");
        for (i, (ours, theirs)) in RSQRT_SEED.iter().zip(spec::RSQRT_SEED.iter()).enumerate() {
            assert_eq!(*ours, *theirs as i128, "RSQRT_SEED[{i}]");
        }
    }
}

use consts::*;

/// `2^n`, by multiplication. The one place a shift would be harmless, written out anyway so the
/// no-shift rule needs no exceptions to check.
fn pow2(n: u32) -> i128 {
    2i128.checked_pow(n).expect("callers bound n at 62")
}

/// Floor division — what an arithmetic right shift does, named. `divisor` must be positive.
fn floor_div(numerator: i128, divisor: i128) -> i128 {
    numerator.div_euclid(divisor)
}

/// Truncating division — what C's `/` does on `int64_t`, which is what upstream gemmlowp relies
/// on. Differs from [`floor_div`] on exactly the negative non-exact quotients, which is where the
/// first implementation's `SRDHM` went wrong.
fn trunc_div(numerator: i128, divisor: i128) -> i128 {
    numerator / divisor
}

/// Round half away from zero, by rounding the magnitude and reapplying the sign.
///
/// Symmetric by construction: there is no negative branch, so there is no negative branch to get
/// wrong. This is the rule ADR-0040 C1 states.
fn round_half_away(numerator: i128, divisor: i128) -> i128 {
    let magnitude = numerator.abs();
    let half = floor_div(divisor, 2);
    let rounded = floor_div(magnitude.checked_add(half).expect("i128 headroom"), divisor);
    if numerator < 0 { -rounded } else { rounded }
}

fn to_i32(v: i128) -> i32 {
    v.clamp(i32::MIN as i128, i32::MAX as i128) as i32
}

/// ADR-0040 C1 at 32 bits.
pub fn ref2_rounding_shift_right(x: i32, s: u8) -> i32 {
    assert!(s <= 31, "C1 bounds s at 31");
    if s == 0 {
        return x;
    }
    to_i32(round_half_away(x as i128, pow2(s as u32)))
}

/// ADR-0040 C1 at 64 bits — the same rule, wider type.
pub fn ref2_rounding_shift_right_64(x: i64, s: u8) -> i64 {
    assert!(s <= 62, "H bounds s at 62");
    if s == 0 {
        return x;
    }
    let v = round_half_away(x as i128, pow2(s as u32));
    v.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// ADR-0040 C2 — gemmlowp's `SaturatingRoundingDoublingHighMul`.
///
/// The division truncates toward zero, matching upstream's `/ (1ll << 31)`. Pairing gemmlowp's
/// asymmetric nudge with a floor instead is the defect this crate found.
pub fn ref2_srdhm(a: i32, b: i32) -> i32 {
    if a == i32::MIN && b == i32::MIN {
        return i32::MAX;
    }
    let product = (a as i128).checked_mul(b as i128).expect("two i32 fit an i128");
    let half = pow2(30);
    let nudge = if product >= 0 { half } else { 1i128.checked_sub(half).expect("i128 headroom") };
    to_i32(trunc_div(product.checked_add(nudge).expect("i128 headroom"), pow2(31)))
}

/// ADR-0040 D op 2.
pub fn ref2_requantize(acc: i32, multiplier: i32, shift: u8) -> i8 {
    ref2_rounding_shift_right(ref2_srdhm(acc, multiplier), shift).clamp(-128, 127) as i8
}

/// ADR-0040 H op 9 — the amplifying rescale.
pub fn ref2_rescale_q(acc: i32, multiplier: i32, shift: u8) -> i32 {
    let shift = shift.min(RESCALE_MAX_SHIFT);
    let product = (acc as i128).checked_mul(multiplier as i128).expect("two i32 fit an i128");
    to_i32(round_half_away(product, pow2(shift as u32)))
}

/// `Poly2(p) = A·(p + B)² + C` — the SHIFTED-SQUARE form. The two `>> K` in the spec are
/// truncating and both operands are non-negative there, so [`floor_div`] is the same function;
/// it is written as a floor rather than a truncation so a future negative operand is answered for
/// rather than assumed away.
fn ref2_poly2(p: i128) -> i128 {
    let k = pow2(K);
    let t = p.checked_add(POLY2_B).expect("i128 headroom");
    let square = floor_div(t.checked_mul(t).expect("i128 headroom"), k);
    floor_div((POLY2_A).checked_mul(square).expect("i128 headroom"), k).checked_add(POLY2_C).expect("i128 headroom")
}

/// ADR-0040 F1 — `exp(x)` for `x ≤ 0`, Qk in and out.
///
/// The range-reduction count is derived by division here rather than by repeated subtraction so
/// that a divergence, if there were one, would be about the *rounding* of `−x / LN2_Q` rather
/// than about loop-exit conditions.
pub fn ref2_int_exp(x: i32) -> i32 {
    let x = (x as i128).min(0);
    let ln2 = LN2_Q;
    let z = floor_div(-x, ln2).min(Z_MAX);
    if z >= Z_MAX {
        return 0;
    }
    let reduced = x.checked_add(z.checked_mul(ln2).expect("i128 headroom")).expect("i128 headroom");
    to_i32(round_half_away(ref2_poly2(reduced), pow2(z as u32)))
}

/// ADR-0040 F2 — `1/√v` for `v > 0`, Qk in and out.
///
/// The mantissa is normalised by an explicit multiply/divide loop rather than by `leading_zeros`,
/// so the two implementations agree on the exponent by construction rather than by both trusting
/// the same intrinsic.
pub fn ref2_int_rsqrt(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    let one = ONE;
    let four = one.checked_mul(4).expect("i128 headroom");
    let mut mantissa = v as i128;
    let mut exponent: i32 = 0;
    // Bring the mantissa into [1, 4) by steps of 4, which is a step of 1 in the halved exponent.
    while mantissa >= four {
        mantissa = floor_div(mantissa, 4);
        exponent = exponent.checked_add(1).expect("bounded by the width of v");
    }
    while mantissa < one {
        mantissa = mantissa.checked_mul(4).expect("i128 headroom");
        exponent = exponent.checked_sub(1).expect("bounded by the width of v");
    }
    let index = floor_div(
        mantissa.checked_sub(one).expect("i128 headroom").checked_mul(16).expect("i128 headroom"),
        one.checked_mul(3).expect("i128 headroom"),
    )
    .clamp(0, 15) as usize;
    let mut y = RSQRT_SEED[index];
    let k = pow2(K);
    for _ in 0..RSQRT_ITERS {
        let y_squared = floor_div(y.checked_mul(y).expect("i128 headroom"), k);
        let m_y_squared = floor_div(mantissa.checked_mul(y_squared).expect("i128 headroom"), k);
        let correction = one.checked_mul(3).expect("i128 headroom").checked_sub(m_y_squared).expect("i128 headroom");
        y = floor_div(y.checked_mul(correction).expect("i128 headroom"), k.checked_mul(2).expect("i128 headroom"));
        if y <= 0 {
            y = 1;
        }
    }
    let scaled = if exponent >= 0 {
        floor_div(y, pow2(exponent as u32))
    } else {
        y.checked_mul(pow2(exponent.unsigned_abs())).expect("i128 headroom")
    };
    scaled.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// ADR-0040 F2's composed reciprocal, `1/v = (1/√v)²`.
pub fn ref2_int_recip(v: i64) -> i64 {
    let r = ref2_int_rsqrt(v) as i128;
    let out = floor_div(r.checked_mul(r).expect("i128 headroom"), pow2(K));
    out.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// The normalisation loop above must halve the exponent the same way the spec's `div_euclid` does.
/// Exposed so the differential can check the decomposition itself, not only the final value.
pub fn ref2_normalize(v: i64) -> Option<(i128, i32)> {
    if v <= 0 {
        return None;
    }
    let one = ONE;
    let four = one.checked_mul(4)?;
    let (mut mantissa, mut exponent) = (v as i128, 0i32);
    while mantissa >= four {
        mantissa = floor_div(mantissa, 4);
        exponent = exponent.checked_add(1)?;
    }
    while mantissa < one {
        mantissa = mantissa.checked_mul(4)?;
        exponent = exponent.checked_sub(1)?;
    }
    Some((mantissa, exponent))
}

#[cfg(test)]
mod tests {
    /// The independence discipline, checked against this file's actual source rather than trusted.
    ///
    /// The whole argument for this crate is that its formulation is *orthogonal to the axis the
    /// first implementation was wrong on*: all three defects the differential found were silent
    /// floors or silent wraps inside a shift. A single `>>` here would reintroduce exactly the
    /// blind spot the crate exists to be free of, and would do it invisibly.
    ///
    /// Comments are excluded, since they legitimately quote the specification's shifts.
    #[test]
    fn the_no_shift_discipline_holds() {
        // The needles are assembled rather than written, so this checker is not itself a hit.
        let right = format!("{0}{0}", '>');
        let left = format!("{0}{0}", '<');
        let intrinsic = format!("leading_{}", "zeros");
        let banned = [right.as_str(), left.as_str(), intrinsic.as_str()];

        // Only the shipped code. The discipline is about the implementations; this module is the
        // thing that verifies them, and scanning it would make the check unwritable.
        let source = include_str!("primitives.rs");
        let shipped = source.split("\n#[cfg(test)]\nmod tests {").next().expect("split yields at least one part");

        let offenders: Vec<String> = shipped
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let code = line.split("//").next().unwrap_or("");
                banned.iter().any(|needle| code.contains(needle))
            })
            .map(|(number, line)| format!("  line {}: {}", number + 1, line.trim()))
            .collect();
        assert!(
            offenders.is_empty(),
            "a shift operator reached the second implementation, which is exactly where the first \
             implementation's three defects hid:\n{}",
            offenders.join("\n")
        );

        // The check must be able to fail, or it is decoration. It very nearly was: `ONE` was
        // defined with a shift until this test was written.
        let planted = "    pub const ONE: i128 = 1i128 >> K;";
        assert!(banned.iter().any(|needle| planted.split("//").next().unwrap_or("").contains(needle)));
        // And a quoted shift inside a comment must NOT trip it, or the file could not describe
        // the specification it implements.
        assert!(!banned.iter().any(|n| "    let k = pow2(K); // the spec writes >> K here".split("//").next().unwrap().contains(n)));
    }
}
