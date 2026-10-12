//! Verification for `PALW-BASE-0`'s arithmetic: a second implementation of ADR-0040's primitives,
//! **and** vendored upstream gemmlowp as a third-party oracle.
//!
//! # What a second implementation is for
//!
//! ADR-0040's Consequences claim that "two honest implementations cannot disagree by one ulp", and
//! that claim is what lets ADR-0027's court treat a disagreement as a *lie* rather than as noise.
//! If it is false, every honest operator running a different-but-conforming implementation is
//! convictable. So the claim has to be tested against something, and it cannot be tested against
//! the implementation it is a claim about.
//!
//! It found three, immediately. All are recorded below because the shape of them is the argument
//! for this crate existing.
//!
//! # Two kinds of independence, because they catch different things
//!
//! * [`primitives`] re-derives all seven primitives. Its independence is **structural** — same
//!   author, different formulation — so it catches a coding mistake and cannot catch a misreading
//!   of the specification.
//! * [`gemmlowp`] is Google's gemmlowp, vendored byte-identically. Its independence is
//!   **authorship**, so for the two primitives it defines it is evidence that ADR-0040 C1 and C2 are
//!   *right*, not merely that this repository is self-consistent.
//!
//! gemmlowp is the authority for `SRDHM` and `RoundingShiftRight` and defines nothing else. The
//! other five — `IntExp`, `IntRsqrt`, `IntRecip`, `Rescale`, the 64-bit shift — exist only in
//! ADR-0040, so structural independence is all that is available for them.
//!
//! # The structural discipline: **no shift operator**
//!
//! `misaka-palw-reference2` gets its independence from authorship alone, vendoring Berkeley
//! SoftFloat. Five of these primitives have no upstream to vendor, so their second implementation
//! is held to one mechanical rule instead:
//!
//! > No `>>`, no `<<`, and no `leading_zeros` appears anywhere in this crate's implementations.
//!
//! Every scaling is an explicit `i128` multiply or a `div_euclid`/`/` with the rounding direction
//! written out. That is not a stylistic preference — it is chosen to be *orthogonal to the axis
//! the first implementation is most likely to be wrong on*. An arithmetic shift silently floors,
//! which is invisible while all operands are positive and wrong for every negative one, and a
//! shift of a value near the type's limit silently wraps. Both defects found so far are exactly
//! that, and neither could survive a formulation that has no shift to hide in.
//!
//! # What the differential found
//!
//! **1. `SRDHM` disagreed with gemmlowp on 50.1 % of inputs.** Upstream divides by `1 << 31` with
//! C integer division, which truncates toward zero, and its asymmetric nudge (`1 − 2^30` rather
//! than `−2^30`) exists precisely to make truncation round half away from zero. The first
//! implementation paired that nudge with an arithmetic shift, which floors — applying the
//! correction twice. Every negative product came out one unit further from zero:
//! `srdhm(−2^30, 2^30)` returned `−2^29 − 1` where the exact value is `−2^29`.
//!
//! This one was serious out of proportion to its size. ADR-0040 C2 chose SRDHM *because* it is
//! already implemented identically in several independent codebases — so a third party writing
//! BASE-0 against real gemmlowp would have disagreed with the reference on half of all inputs,
//! and under an optimistic-verification court a systematic disagreement is not a rounding
//! difference. It is a conviction, and a bond.
//!
//! **2. `RoundingShiftRight` was not round-half-away-from-zero.** `(x ± 2^(s−1)) >> s` floors
//! after nudging, so for negatives the nudge and the floor push the same way: `RSR(−64, 1)`
//! returned `−33` where the exact quotient is `−32` and needs no rounding at all. It disagreed
//! with gemmlowp's `RoundingDivideByPOT` on 50 % of random pairs, and separately overflowed `i32`
//! on 3.2 % of them, wrapping the sign of the largest accumulators. ADR-0040 C1 *stated* the rule
//! correctly; the pseudocode under it did not implement the rule it stated.
//!
//! Both had passed the first implementation's own tests, because those tests exercised positive
//! values — where the shift form is correct.
//!
//! # What this crate does NOT establish
//!
//! Structural independence is weaker than authorship independence: one person wrote both sides,
//! so a misreading of the *specification* would be reproduced here rather than caught. The
//! differential proves the two derivations agree; it cannot prove either matches what a third
//! party would build. Closing that gap needs an implementation this project did not write —
//! for `SRDHM` and `RoundingShiftRight` upstream gemmlowp is directly vendorable and is the
//! obvious next step, since those two are where a third party is most likely to differ.

#![deny(clippy::arithmetic_side_effects)]

pub mod gemmlowp;
pub mod primitives;

pub use gemmlowp::{OracleError, gemmlowp_rounding_divide_by_pot, gemmlowp_srdhm};
pub use primitives::{
    ref2_int_exp, ref2_int_recip, ref2_int_rsqrt, ref2_requantize, ref2_rescale_q, ref2_rounding_shift_right,
    ref2_rounding_shift_right_64, ref2_srdhm,
};
