//! Upstream gemmlowp as an **authorship-independent** oracle for two of ADR-0040's primitives.
//!
//! # Why this is different in kind from [`crate::primitives`]
//!
//! [`crate::primitives`] is a second *derivation* of the specification, written by the same author
//! as the first. It catches a coding mistake and cannot catch a misreading of the specification —
//! a wrong understanding is simply reproduced on both sides.
//!
//! This module has no such limitation for the two primitives it covers. `vendor/gemmlowp/` is
//! Google's gemmlowp, byte-identical to upstream commit
//! `16e8662c34917be0065110bfcd9cc27d30f52fdf`, written years before this project and with no
//! knowledge of it. When [`gemmlowp_srdhm`] and [`crate::ref2_srdhm`] agree, that is evidence about
//! ADR-0040 C2 being *right*, not merely about this repository being self-consistent.
//!
//! # This is precisely where a third party mattered most
//!
//! ADR-0040 C2 chose `SRDHM` on the grounds that it is "already implemented identically in several
//! independent codebases". That reasoning only holds if the reference actually matches those
//! codebases — and it did not: the first implementation used an arithmetic shift where upstream
//! uses a truncating division, and disagreed with it on 50.1 % of inputs, every negative product.
//! A third-party BASE-0 built against real gemmlowp would have been convicted for being correct.
//!
//! So the two functions here are exactly the two the earlier differential named as the outstanding
//! authorship gap. That gap is now closed for them, and remains open for the other five primitives
//! — `IntExp`, `IntRsqrt`, `IntRecip`, `Rescale` and the 64-bit shift have no upstream to vendor,
//! because ADR-0040 F1/F2 and H define them for this project.
//!
//! # What upstream actually computes
//!
//! Quoted rather than paraphrased, because the two details that were wrong are both visible in one
//! line each:
//!
//! ```text
//! SaturatingRoundingDoublingHighMul(a, b):
//!     nudge = ab_64 >= 0 ? (1 << 30) : (1 - (1 << 30));
//!     ab_x2_high32 = static_cast<int32>((ab_64 + nudge) / (1ll << 31));   // DIVISION
//!
//! RoundingDivideByPOT(x, exponent):
//!     remainder = x & mask;                                   // mask = 2^exponent - 1
//!     threshold = (mask >> 1) + (x < 0 ? 1 : 0);              // the +1 is the sign correction
//!     return (x >> exponent) + (remainder > threshold ? 1 : 0);
//! ```
//!
//! The `/` in the first is not a `>>`, and the `+ (x < 0)` in the second is what makes the rule
//! symmetric about zero. Those are the two defects, stated in upstream's own terms.

/// Why a call cannot be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OracleError {
    /// Upstream `RoundingDivideByPOT` asserts `0 <= exponent <= 31`. Outside that range its
    /// behaviour depends on whether `NDEBUG` was set at build time, and an oracle that answers
    /// differently in release than in debug is not an oracle — so the range is refused here rather
    /// than left to a C++ assert.
    ExponentOutOfRange { got: i32 },
}

unsafe extern "C" {
    fn misaka_gemmlowp_srdhm(a: i32, b: i32) -> i32;
    fn misaka_gemmlowp_rounding_divide_by_pot(x: i32, exponent: i32) -> i32;
}

/// Upstream `gemmlowp::SaturatingRoundingDoublingHighMul(a, b)`.
///
/// Total: every `(a, b)` is in domain, including the `i32::MIN × i32::MIN` case upstream saturates.
pub fn gemmlowp_srdhm(a: i32, b: i32) -> i32 {
    // SAFETY: the shim is a call into a header-only C++ function over two `i32` by value returning
    // an `i32`. No pointers cross the boundary, so there is nothing to alias, own, or outlive.
    unsafe { misaka_gemmlowp_srdhm(a, b) }
}

/// Upstream `gemmlowp::RoundingDivideByPOT<std::int32_t>(x, exponent)`.
///
/// This is the function ADR-0040 C1's rule is supposed to describe, so it is the authority on what
/// `RoundingShiftRight` must return.
pub fn gemmlowp_rounding_divide_by_pot(x: i32, exponent: i32) -> Result<i32, OracleError> {
    if !(0..=31).contains(&exponent) {
        return Err(OracleError::ExponentOutOfRange { got: exponent });
    }
    // SAFETY: as above, plus `exponent` is checked into upstream's asserted range immediately
    // above, so the vendored assert cannot fire and the answer cannot depend on `NDEBUG`.
    Ok(unsafe { misaka_gemmlowp_rounding_divide_by_pot(x, exponent) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The oracle must be reached at all. A silently-failed link, a name mangled by a missing
    /// `extern "C"`, or a shim that returned its own argument would leave every differential below
    /// passing against a constant — so pin values that only the real function produces.
    ///
    /// `srdhm(2^30, 2^30) = 2^29` is `0.5 × 0.5 = 0.25` in Q31, and `srdhm(-1, 1) = 0` is the case
    /// the first implementation got wrong. A stub returning `a`, `b`, or `0` fails at least one.
    #[test]
    fn the_vendored_oracle_is_actually_being_called() {
        assert_eq!(gemmlowp_srdhm(1 << 30, 1 << 30), 1 << 29, "0.5 x 0.5 = 0.25 in Q31");
        assert_eq!(gemmlowp_srdhm(i32::MIN, i32::MIN), i32::MAX, "upstream's one saturating case");
        assert_eq!(gemmlowp_srdhm(-1, 1), 0, "a product far below the resolution rounds toward zero");
        assert_eq!(gemmlowp_srdhm(-(1 << 30), 1 << 30), -(1 << 29), "-0.25 exactly, no rounding");
        assert_eq!(gemmlowp_rounding_divide_by_pot(-64, 1), Ok(-32), "an exact quotient is not rounded");
        assert_eq!(gemmlowp_rounding_divide_by_pot(3, 1), Ok(2), "half rounds away from zero");
        assert_eq!(gemmlowp_rounding_divide_by_pot(-3, 1), Ok(-2), "and symmetrically for negatives");
        assert_eq!(gemmlowp_rounding_divide_by_pot(7, 0), Ok(7), "a zero exponent is the identity");
    }

    /// Outside upstream's asserted range the answer is refused rather than taken. Left to the C++
    /// assert this would abort in debug and return an arbitrary shift result in release.
    #[test]
    fn an_out_of_range_exponent_is_refused_not_asserted() {
        assert_eq!(gemmlowp_rounding_divide_by_pot(1, 32), Err(OracleError::ExponentOutOfRange { got: 32 }));
        assert_eq!(gemmlowp_rounding_divide_by_pot(1, -1), Err(OracleError::ExponentOutOfRange { got: -1 }));
        assert!(gemmlowp_rounding_divide_by_pot(1, 31).is_ok());
        assert!(gemmlowp_rounding_divide_by_pot(1, 0).is_ok());
    }
}

/// Integrity of the vendored tree, checked rather than asserted in prose.
///
/// The README says every file under `vendor/gemmlowp/` is byte-identical to upstream commit
/// `16e8662c34917be0065110bfcd9cc27d30f52fdf`. A README cannot enforce that. These hashes can:
/// an edit to a vendored header — a warning silenced, a macro tweaked to make a build pass —
/// fails here by filename instead of quietly turning the third-party oracle into a local one.
///
/// That is the whole value at stake. An oracle this project has edited is not an oracle.
#[cfg(test)]
mod integrity {
    /// SHA-256 of each vendored file, as reported by `shasum -a 256` at vendoring time. Cross-check
    /// against upstream by cloning the commit and hashing the same paths.
    const VENDORED: [(&str, &str, &[u8]); 9] = [
        ("AUTHORS", "916234caa03bbb2769b278e165515a8ca9fa9d8f60b7b57a5dd6a4f026208ce2", include_bytes!("../vendor/gemmlowp/AUTHORS")),
        ("LICENSE", "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30", include_bytes!("../vendor/gemmlowp/LICENSE")),
        (
            "fixedpoint/fixedpoint.h",
            "f1b11e756ba138b42abd2f39095fd4d740a26b10ff1a8c682c2eb4d273658cf6",
            include_bytes!("../vendor/gemmlowp/fixedpoint/fixedpoint.h"),
        ),
        (
            "fixedpoint/fixedpoint_avx.h",
            "e6a6fa2a5fcf5207e152eb0aff459003890d9632e9743377e291d57a3b4379c3",
            include_bytes!("../vendor/gemmlowp/fixedpoint/fixedpoint_avx.h"),
        ),
        (
            "fixedpoint/fixedpoint_msa.h",
            "71985120ddeeacfc8b3eed81f084c7fc35b5c47bd2b62241505bb2834f91402f",
            include_bytes!("../vendor/gemmlowp/fixedpoint/fixedpoint_msa.h"),
        ),
        (
            "fixedpoint/fixedpoint_neon.h",
            "83f64af6555d6c59b916f59ee2a837b1cd4140c0b7c26791ac3f0b36975f0ad0",
            include_bytes!("../vendor/gemmlowp/fixedpoint/fixedpoint_neon.h"),
        ),
        (
            "fixedpoint/fixedpoint_sse.h",
            "c729d7abe8c52829be63bc0b51def7aee7a2b700aaaedbcd9bc3605605d9ce2e",
            include_bytes!("../vendor/gemmlowp/fixedpoint/fixedpoint_sse.h"),
        ),
        (
            "fixedpoint/fixedpoint_wasmsimd.h",
            "17552be58bf100860f5a131491d126e795d2225af4a9f467d5bf1735aaa26d62",
            include_bytes!("../vendor/gemmlowp/fixedpoint/fixedpoint_wasmsimd.h"),
        ),
        (
            "internal/detect_platform.h",
            "bfa61d487156c68cb11fd2b114e0aa68ac048ec15b4e44631da2bc6b033a3f10",
            include_bytes!("../vendor/gemmlowp/internal/detect_platform.h"),
        ),
    ];

    /// A dependency-free SHA-256. Written out rather than pulled in because a hash used to police
    /// vendored code should not itself arrive through the dependency graph it is policing.
    // **The index arithmetic below stays in SHA-256's own notation.** `w[i - 15]`, `w[i - 2]`,
    // `w[i - 16]` and `w[i - 7]` are the message schedule as FIPS 180-4 writes it, and `i` runs
    // `16..64`, so every one of those is in range by the loop bound. Rewriting them as
    // `wrapping_sub` would satisfy the lint and lose the property that makes this file worth
    // having: that it reads like the specification it is a reference for. The additions already
    // say `wrapping_add`, which is SHA-256's semantics rather than an oversight.
    #[allow(clippy::arithmetic_side_effects)]
    fn sha256(message: &[u8]) -> String {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
            0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
            0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
            0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
            0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
            0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];
        let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
        let mut padded = message.to_vec();
        let bit_length = (message.len() as u64).checked_mul(8).expect("a message shorter than 2^61 bytes");
        padded.push(0x80);
        while padded.len() % 64 != 56 {
            padded.push(0);
        }
        padded.extend_from_slice(&bit_length.to_be_bytes());
        for block in padded.chunks(64) {
            let mut w = [0u32; 64];
            for (i, word) in block.chunks(4).enumerate() {
                w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
            }
            let mut v = h;
            for i in 0..64 {
                let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
                let choose = (v[4] & v[5]) ^ (!v[4] & v[6]);
                let t1 = v[7].wrapping_add(s1).wrapping_add(choose).wrapping_add(K[i]).wrapping_add(w[i]);
                let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
                let majority = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
                let t2 = s0.wrapping_add(majority);
                v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
            }
            for (slot, add) in h.iter_mut().zip(v.iter()) {
                *slot = slot.wrapping_add(*add);
            }
        }
        h.iter().map(|word| format!("{word:08x}")).collect()
    }

    /// The hash function must be right before it can police anything. NIST's two published test
    /// vectors, so a transcription error in the round constants fails here rather than by making
    /// every vendored file look modified.
    #[test]
    fn the_hash_function_is_correct() {
        assert_eq!(sha256(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            "a multi-block message, since the padding is where a hand-written SHA-256 goes wrong"
        );
    }

    /// Every vendored file is still the upstream byte sequence.
    #[test]
    fn the_vendored_tree_is_unmodified() {
        for (path, expected, bytes) in VENDORED {
            assert_eq!(
                sha256(bytes),
                expected,
                "vendor/gemmlowp/{path} has been modified — the third-party oracle is no longer third-party"
            );
        }
    }
}
