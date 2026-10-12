//! The exported Known-Answer Test set, replayed through the second implementation.
//!
//! # Why this test and not just the differential
//!
//! `differential.rs` already compares the two implementations directly, so this cannot find an
//! arithmetic disagreement the differential would miss. It answers a different question: **is the
//! artifact we hand a third party a statement about the class, or a dump of implementation #1?**
//!
//! Those come apart in a specific and quiet way. `misaka_palw_base0::kat` computes every expected
//! output by calling the specification's own functions. If a vector's coverage is wrong — a group
//! that never reaches a range-reduction bucket, an argument table that omits a sign — the digest
//! still pins it and the differential still passes, because both implementations agree everywhere
//! the differential samples. What would be published is then a KAT set that a *non-conforming*
//! implementation could also pass. Running the exported vectors through the independent
//! implementation is what makes the published file mean "both implementations produce these
//! outputs" rather than "this is what one of them printed".
//!
//! # gemmlowp is checked separately and only where it is authoritative
//!
//! gemmlowp defines `SRDHM` and `RoundingDivideByPOT` and nothing else, so it is the oracle for
//! exactly two of the nine groups. Its independence is by authorship, which is the only kind that
//! is evidence ADR-0040 is *right* rather than that this repository is self-consistent — so those
//! two groups are worth asserting against it even though the other seven cannot be.

use misaka_palw_base0::kat;
use misaka_palw_base0_ref2 as ref2;

/// Replay every vector. The mapping from group name to the second implementation is written out
/// rather than derived, so a group added to the KAT set without a `ref2` counterpart fails here
/// instead of being skipped.
#[test]
fn the_second_implementation_reproduces_every_kat_vector() {
    let groups = kat::groups();
    assert_eq!(groups.len(), 9, "nine primitives; a tenth needs a case below");
    let mut replayed = 0usize;
    for group in &groups {
        for vector in &group.vectors {
            let a = &vector.args;
            let got = match group.op {
                "RoundingShiftRight" => ref2::ref2_rounding_shift_right(a[0] as i32, a[1] as u8) as i64,
                "RoundingShiftRight64" => ref2::ref2_rounding_shift_right_64(a[0], a[1] as u8),
                "SRDHM" => ref2::ref2_srdhm(a[0] as i32, a[1] as i32) as i64,
                "Requantize" => ref2::ref2_requantize(a[0] as i32, a[1] as i32, a[2] as u8) as i64,
                // The second implementation has no zero-point form of its own, so the vectors are
                // checked through the composition ADR-0040 G2 specifies, built from `ref2`'s own
                // primitives: `Saturate8(RSR(SRDHM(acc, mult), shift) + zero)`, with the zero added
                // in i32 BEFORE the clamp. Reusing `ref2_requantize` here would clamp twice and
                // silently agree on every vector that does not saturate — which is every vector
                // the zero point exists for.
                "RequantizeWithZero" => {
                    let narrowed = ref2::ref2_rounding_shift_right(ref2::ref2_srdhm(a[0] as i32, a[1] as i32), a[2] as u8);
                    narrowed.saturating_add(a[3] as i32).clamp(-128, 127) as i64
                }
                "Rescale" => ref2::ref2_rescale_q(a[0] as i32, a[1] as i32, a[2] as u8) as i64,
                "IntExp" => ref2::ref2_int_exp(a[0] as i32) as i64,
                "IntRsqrt" => ref2::ref2_int_rsqrt(a[0]),
                "IntRecip" => ref2::ref2_int_recip(a[0]),
                other => panic!("KAT group {other} has no second implementation in this test"),
            };
            assert_eq!(got, vector.out, "{}{:?}: the KAT says {} and the second implementation says {got}", group.op, a, vector.out);
            replayed += 1;
        }
    }
    assert!(replayed > 10_000, "only {replayed} vectors replayed; the set shrank");
}

/// The two groups gemmlowp is authoritative for. `RoundingDivideByPOT` is upstream's name for
/// `RoundingShiftRight`, and its `Result` is the oracle's own domain guard, not a disagreement.
#[test]
fn the_upstream_oracle_reproduces_the_two_groups_it_defines() {
    let groups = kat::groups();
    let mut checked = 0usize;
    for group in &groups {
        match group.op {
            "SRDHM" => {
                for vector in &group.vectors {
                    assert_eq!(
                        ref2::gemmlowp::gemmlowp_srdhm(vector.args[0] as i32, vector.args[1] as i32) as i64,
                        vector.out,
                        "SRDHM{:?}",
                        vector.args
                    );
                    checked += 1;
                }
            }
            "RoundingShiftRight" => {
                for vector in &group.vectors {
                    let exponent = vector.args[1] as i32;
                    let Ok(upstream) = ref2::gemmlowp::gemmlowp_rounding_divide_by_pot(vector.args[0] as i32, exponent) else {
                        panic!("the KAT set must not contain a shift outside the oracle's domain: {:?}", vector.args);
                    };
                    assert_eq!(upstream as i64, vector.out, "RoundingShiftRight{:?}", vector.args);
                    checked += 1;
                }
            }
            _ => {}
        }
    }
    assert!(checked > 2_000, "only {checked} vectors reached the upstream oracle");
}
