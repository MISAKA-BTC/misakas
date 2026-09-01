//! ADR-0038 Decision D / ADR-0039 Decision 5: where a block's PWU comes from.
//!
//! `pwu` is the term that carries 90–99 % of fork-choice weight under ADR-0038. Until this
//! module it was a **self-declared `u64` on the miner's own commitment, checked only for
//! non-zero** (`PalwBlockCommitmentV1::pwu_claim`; re-audit 2026-08-17 blocker 6). A miner
//! could claim `u64::MAX` and, once the ramp matured the block, outweigh the honest chain by
//! an arbitrary factor at no cost. Nothing anywhere derived it, and no registry field held it.
//!
//! ## The derivation
//!
//! A PALW block is one won lottery. Its work is therefore the work of the whole attempt
//! sequence that produced it, which factors exactly:
//!
//! ```text
//! pwu(B) = expected_attempts(class_target) × pwu_per_inference(class)
//!          \__________________________/     \_______________________/
//!            chain state: the class's          registration: a frozen,
//!            own DAA target                    normative operation count
//! ```
//!
//! Both factors are facts about the class and the chain. **Neither is a miner input**, so the
//! correct admission rule is not "is the claim plausible" but "is the claim EQUAL to the
//! derivation" — [`check_pwu_claim_v1`]. The field survives on the commitment (so the ticket
//! root and the executor's signature still bind it, and a stale claim cannot be swapped in
//! post-hoc), but it has exactly one legal value.
//!
//! The first factor is the standard work identity, applied to inferences rather than hashes:
//! a ticket is uniform over [`PALW_TICKET_SPACE_BITS`] bits and admits when it lands under the
//! class target, so the expected number of canonical inferences is `2^bits / (target + 1)`.
//! This is what makes the measure self-normalizing **within** a class: a class whose DAA has
//! tightened its target is spending proportionally more inference per block, and its blocks
//! weigh proportionally more, with no table to maintain.
//!
//! ## What `pwu_per_inference` must be, and must not be
//!
//! It is the **normative operation count of one canonical inference** under the class's frozen
//! kernel graph — a countable consequence of the registered model shape, the pinned kernel
//! graph, and the frozen decode budget ([`crate::pow_layer0::POW_L1_PALW_N_PREDICT_V1`], whose
//! own doc records that changing it is a hard fork). Every ticket in a class has the same job
//! shape, so this is one number per class, fixed when the class registers.
//!
//! It must **not** be:
//!
//! * **wall-clock cost.** The registry's `replay_cost` / `ms_per_decode_token` is a measured
//!   millisecond figure — host-dependent, hardware-dependent and trivially self-reported. It is
//!   the right basis for sizing *dispute windows* and the wrong basis for *consensus weight*.
//!   ADR-0038 Decision D says it in one line: static intra-class, never wall-clock.
//! * **an operator-tuned price.** A hand-set per-class multiplier ("70B = 100, 8B CPU = 8") is
//!   the cross-class coefficient table ADR-0038 Decision D rejects: any class priced even
//!   slightly high becomes a standing arbitrage, every miner migrates to it, and the multi-class
//!   design collapses into the monoculture it exists to prevent.
//!
//! ## The boundary this module does NOT cross
//!
//! An operation count is not an economic cost — a GPU buys its FLOPs far cheaper than a scalar
//! CPU does. So **`pwu` magnitude must never be read as a cross-class price.** Cross-class
//! fairness is the epoch share cap's job (ADR-0039 Decision 5:
//! `Σ_{b ∈ class c, epoch e} weight(b) ≤ s_c(e) · W_e`, enforced at admission by rejection),
//! and the per-class DAA's. This module makes `pwu` honest *within* a class and removes the
//! miner's freedom entirely; it deliberately does not attempt to price classes against each
//! other, and a future change that starts using `pwu` for that has reintroduced the coefficient
//! table by the back door.
//!
//! Consensus-inert: nothing calls these yet. Wiring is the ADR-0039 W4′ weight derivation.

use thiserror::Error;

/// Width of the ticket space: a ticket is the leading 128 bits of the Layer-0 finalized digest,
/// read big-endian, and admits when it is **strictly less than** the class target.
///
/// Pinned here because the expected-attempt count is meaningless without it, and because the
/// class target ([`crate::palw_class_daa::adjust_class_target_v1`]) is a `u128` whose width had
/// never been written down anywhere. Changing this changes every class's work and is a hard fork.
pub const PALW_TICKET_SPACE_BITS: u32 = 128;

/// The ticket one attempt drew: the leading [`PALW_TICKET_SPACE_BITS`] bits of the Layer-0
/// finalized digest, big-endian.
///
/// The digest is already the output of `pow_finalizer_blake2b_512` over the attempt, so the ticket
/// is uniform and no further mixing is needed or wanted — a second hash here would be a second
/// thing to agree about.
pub fn palw_ticket_v1(pow_digest: &[u8; crate::pow_layer0::POW_FINALIZER_BYTES]) -> u128 {
    let mut lead = [0u8; 16];
    lead.copy_from_slice(&pow_digest[..16]);
    u128::from_be_bytes(lead)
}

/// Whether a ticket admits under `class_target` — ADR-0038 Decision A's algo-4 lottery clause.
///
/// **`<=`, not `<`, and the choice is forced twice over.**
///
/// * [`palw_expected_attempts_v1`] computes `2^128 / (target + 1)`, i.e. it counts `target + 1`
///   admitting values. Its own comment says so out loud — "`target == u128::MAX` is the easiest
///   possible target: every ticket admits". Under `<` that sentence is false (`u128::MAX` itself
///   would fail) and the work formula is wrong by `(target+1)/target` — a factor of 2 at
///   `target == 1`, i.e. worst exactly where difficulty is highest.
/// * The Layer-0 PoW this sits beside already admits on `<=`
///   (`StateLayer0::check_pow_layer0`: `pow_512 <= self.target_512`). Two lotteries on one header
///   disagreeing about their boundary is a bug waiting for a boundary block.
///
/// **ADR-0038 §Decision A writes the clause as `palw_ticket < class_target`.** That is the
/// discrepancy, recorded rather than silently resolved: the ADR's inequality is the outlier
/// against both the arithmetic and the house convention, so the ADR text should be amended to
/// `<=`. Nothing depends on the answer yet — this is the first implementation of the clause, which
/// is exactly when it is cheap to settle.
pub fn palw_ticket_admits_v1(ticket: u128, class_target: u128) -> bool {
    ticket <= class_target
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwPwuError {
    #[error("pwu_per_inference is zero — a class whose canonical inference costs nothing is not a PALW class")]
    ZeroPwuPerInference,
    #[error("pwu claim {claimed} is not the derived {derived} — pwu is chain state, not a miner input")]
    ClaimMismatch { claimed: u64, derived: u64 },
}

/// Expected canonical inferences to win one ticket at `class_target`: `2^128 / (target + 1)`.
///
/// Saturates into `u64`. Saturation is monotone and only reachable for targets so tight that the
/// expected attempt count already exceeds 2^64 — a regime no class can mine — so ordering is
/// preserved everywhere a class can actually operate.
pub fn palw_expected_attempts_v1(class_target: u128) -> u64 {
    // target == u128::MAX is the easiest possible target: every ticket admits, one attempt each.
    // Handled first because `target + 1` would overflow.
    if class_target == u128::MAX {
        return 1;
    }
    let d = class_target + 1;
    // ⌊2^128 / d⌋ without a 2^128 that does not fit: since `u128::MAX - target == 2^128 - d`,
    //     ⌊(2^128 - d)/d⌋ + 1 = (⌊2^128/d⌋ - 1) + 1 = ⌊2^128/d⌋.
    // Writing it as the tempting `u128::MAX / d` instead loses exactly one attempt whenever d
    // divides 2^128 — i.e. at every power-of-two target, which is where the retarget clamps
    // land — halving the work of the easiest classes.
    let attempts = ((u128::MAX - class_target) / d).saturating_add(1);
    attempts.min(u64::MAX as u128) as u64
}

/// `pwu(B)` for a block mined in a class with this target and this normative per-inference cost.
///
/// Saturating: an over-large product pins at `u64::MAX` rather than wrapping, because wrapping
/// here would make a very hard class weigh *nothing* — the one arithmetic accident that turns a
/// difficulty increase into a weight collapse.
pub fn palw_pwu_v1(class_target: u128, pwu_per_inference: u64) -> u64 {
    // **The EXECUTIONS the search required — and since ADR-0072, a try IS an execution.**
    //
    // `palw_expected_attempts_v1` counts lottery draws. Under ADR-0071 Decision 2 one execution
    // bought `2^PALW_TICKET_NONCE_BUCKET_LOG2` of them (every nonce in the anchor's bucket drew a
    // fresh ticket), so this divided the draws by the bucket to recover the inferences. ADR-0072
    // took the nonce out of the ticket altogether: `class_ticket_v3` is a function of the
    // EXECUTION commitment, which no nonce inside a bucket moves, so a producer draws exactly once
    // per inference and the division is by one. The bucket still exists — it is what names WHICH
    // execution a nonce was paid for by — but it no longer multiplies tickets.
    //
    // Floored at one execution, because a block always carries the one inference it commits to:
    // rounding a cheap class's work to zero would make its blocks weightless and its share
    // unearnable, which is the "difficulty increase turns into a weight collapse" accident this
    // function's saturation already exists to refuse, arriving from the other side.
    let executions = palw_expected_attempts_v1(class_target).max(1) as u128;
    let product = executions.saturating_mul(pwu_per_inference as u128);
    product.min(u64::MAX as u128) as u64
}

/// The admission rule: a commitment's `pwu_claim` must EQUAL the derivation. There is no
/// tolerance band and no "at most" — both factors are facts the miner does not choose, so any
/// other value is either a mistake or a weight-inflation attempt, and both are rejections.
pub fn check_pwu_claim_v1(claimed: u64, class_target: u128, pwu_per_inference: u64) -> Result<(), PalwPwuError> {
    if pwu_per_inference == 0 {
        return Err(PalwPwuError::ZeroPwuPerInference);
    }
    let derived = palw_pwu_v1(class_target, pwu_per_inference);
    if claimed != derived {
        return Err(PalwPwuError::ClaimMismatch { claimed, derived });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The work identity, at the boundaries where an off-by-one would silently change every
    /// class's weight.
    #[test]
    fn expected_attempts_is_the_work_identity() {
        // Easiest possible target: every ticket wins, one inference each. (target+1 would
        // overflow u128 — the case that makes this a special arm rather than an arithmetic.)
        assert_eq!(palw_expected_attempts_v1(u128::MAX), 1);
        // Half the space: two attempts expected.
        assert_eq!(palw_expected_attempts_v1(u128::MAX / 2), 2);
        // A quarter: four.
        assert_eq!(palw_expected_attempts_v1(u128::MAX / 4), 4);
        // Impossible target: maximum work, saturated rather than a division by zero.
        assert_eq!(palw_expected_attempts_v1(0), u64::MAX);
    }

    /// Harder target ⟹ strictly more work. This is the property that lets a class's own DAA
    /// price its blocks with no table: if it ever inverted, tightening difficulty would REDUCE
    /// a class's weight and the retarget loop would run away.
    #[test]
    fn work_is_monotone_in_difficulty() {
        let mut previous = 0u64;
        // Sweep from very easy to very hard across the whole exponent range.
        for shift in 0..64u32 {
            let target = u128::MAX >> shift;
            let attempts = palw_expected_attempts_v1(target);
            assert!(attempts >= previous, "shift {shift}: work must not decrease as the target tightens");
            previous = attempts;
        }
        assert!(previous > 1, "the sweep must actually move");
    }

    /// pwu = attempts x per-inference cost, saturating rather than wrapping. A wrap would make
    /// the hardest classes weightless, which is the worst possible direction for the error.
    #[test]
    fn pwu_is_the_product_and_saturates() {
        // Easiest target: pwu IS the per-inference cost.
        assert_eq!(palw_pwu_v1(u128::MAX, 7), 7);
        // **Half the space is two inferences' worth** (ADR-0072). This value has moved twice, and
        // both moves are the record: it was `14` originally, ADR-0071 Decision 2 made it `7`
        // because two expected tries fell inside one nonce bucket and the search ran one
        // execution, and ADR-0072 makes it `14` again for the opposite reason — there is no
        // sweep any more, so two expected draws ARE two executions.
        assert_eq!(palw_pwu_v1(u128::MAX / 2, 7), 14);
        // Saturation, not wraparound.
        assert_eq!(palw_pwu_v1(0, u64::MAX), u64::MAX);
        assert_eq!(palw_pwu_v1(u128::MAX / 2, u64::MAX), u64::MAX);
    }

    /// Re-audit blocker 6, pinned: **pwu is chain state, not a miner input.**
    ///
    /// The defect was `pwu_claim: u64` on the miner's own commitment with `!= 0` as its only
    /// check, carrying 90-99% of fork-choice weight. A claim of `u64::MAX` cost nothing and,
    /// once matured, outweighed the honest chain arbitrarily.
    #[test]
    fn a_claim_must_equal_the_derivation() {
        let (target, cost) = (u128::MAX / 1_000, 4_000u64);
        let derived = palw_pwu_v1(target, cost);
        assert!(check_pwu_claim_v1(derived, target, cost).is_ok());

        // The attack, refused: the maximum claim.
        assert_eq!(check_pwu_claim_v1(u64::MAX, target, cost), Err(PalwPwuError::ClaimMismatch { claimed: u64::MAX, derived }));
        // And so is one unit off in either direction — there is no tolerance band, because
        // neither factor is something the miner chooses.
        assert!(check_pwu_claim_v1(derived + 1, target, cost).is_err());
        assert!(check_pwu_claim_v1(derived - 1, target, cost).is_err());
        // A zero-cost class is not a class.
        assert_eq!(check_pwu_claim_v1(0, target, 0), Err(PalwPwuError::ZeroPwuPerInference));
    }

    /// The two factors are independent and each scales pwu on its own — cost from registration,
    /// difficulty from the class's DAA. Power-of-two targets are used so the expected-attempt
    /// count is exact and the relations are arithmetic rather than approximate.
    #[test]
    fn pwu_separates_cost_from_difficulty() {
        // A draw is an execution (ADR-0072), so every tighter target is more inferences and the
        // scaling this test is named for holds at EVERY target — there is no longer a bucket
        // below which eight times the tries is the same one inference.
        let easy = u128::MAX >> 24; // 2^24 expected draws = 2^24 executions
        let hard = u128::MAX >> 27; // 2^27 expected draws = 2^27 executions
        assert_eq!(palw_expected_attempts_v1(easy), 1 << 24);
        assert_eq!(palw_expected_attempts_v1(hard), 1 << 27);
        assert_eq!(palw_pwu_v1(easy, 1), 1 << 24, "one execution per draw: pwu counts the draws");

        // Cost scales pwu at a fixed target.
        assert_eq!(palw_pwu_v1(easy, 1_000), 10 * palw_pwu_v1(easy, 100));
        // Difficulty scales pwu at a fixed cost — 8x tighter target, 8x the executions.
        assert_eq!(palw_pwu_v1(hard, 100), 8 * palw_pwu_v1(easy, 100));
        // A cheap class cannot reach an expensive one's per-block weight by difficulty alone
        // unless it actually spends the inferences; that it CAN by spending them is why
        // cross-class fairness is the epoch share cap's job, never pwu magnitude's.
        assert_eq!(palw_pwu_v1(hard, 100), palw_pwu_v1(easy, 800));

        // **The bucket no longer flattens the curve.** Under ADR-0071 Decision 2 these two were
        // deliberately equal — eight times the tries inside one bucket was one inference. Under
        // ADR-0072 a try IS an inference, so they differ by exactly the eight.
        assert_eq!(palw_pwu_v1(u128::MAX >> 13, 100), 8 * palw_pwu_v1(u128::MAX >> 10, 100));
        // And the floor: the easiest target is still one execution, never zero.
        assert_eq!(palw_pwu_v1(u128::MAX, 100), 100);
    }
}

#[cfg(test)]
mod ticket_tests {
    use super::*;

    /// The two halves of the lottery must agree about where the boundary is: the value that
    /// admits is counted by the work formula.
    ///
    /// Asserted against hand-computed values rather than a re-implementation of the production
    /// expression — a copied formula tests the copy. `2^128 / (target + 1)`:
    ///
    /// | target | admitting values | attempts |
    /// |---|---|---|
    /// | `0` | 1 | `2^128` → saturates |
    /// | `1` | 2 | `2^127` → saturates |
    /// | `2^64 - 1` | `2^64` | `2^64` → saturates (one past `u64::MAX`) |
    /// | `2^127 - 1` | `2^127` | 2 |
    /// | `u128::MAX` | `2^128` | 1 |
    #[test]
    fn admission_and_expected_attempts_agree_about_the_boundary() {
        for (target, attempts) in
            [(0u128, u64::MAX), (1u128, u64::MAX), (u128::MAX >> 64, u64::MAX), (u128::MAX >> 1, 2u64), (u128::MAX, 1u64)]
        {
            assert_eq!(palw_expected_attempts_v1(target), attempts, "target {target}");
            // The boundary value itself admits — that is what makes the count `target + 1` rather
            // than `target`, and what the formula above is built on.
            assert!(palw_ticket_admits_v1(target, target), "the boundary ticket must admit at target {target}");
            if let Some(over) = target.checked_add(1) {
                assert!(!palw_ticket_admits_v1(over, target), "one above the target must not admit");
            }
        }
    }

    /// The easiest possible target admits everything — the sentence `palw_expected_attempts_v1`'s
    /// own comment relies on, and the one that is false under a strict `<`.
    #[test]
    fn the_easiest_target_admits_every_ticket() {
        assert_eq!(palw_expected_attempts_v1(u128::MAX), 1);
        for t in [0u128, 1, 42, u128::MAX - 1, u128::MAX] {
            assert!(palw_ticket_admits_v1(t, u128::MAX), "ticket {t} must admit at the easiest target");
        }
    }

    /// The ticket is the digest's leading 128 bits, big-endian — not its tail, and not reversed.
    #[test]
    fn the_ticket_is_the_leading_128_bits_big_endian() {
        let mut digest = [0u8; crate::pow_layer0::POW_FINALIZER_BYTES];
        digest[0] = 0x01;
        digest[15] = 0xFF;
        digest[16] = 0xAA; // beyond the ticket: must not affect it
        assert_eq!(palw_ticket_v1(&digest), (1u128 << 120) | 0xFF);
        let mut same_lead = digest;
        same_lead[16] = 0x55;
        assert_eq!(palw_ticket_v1(&same_lead), palw_ticket_v1(&digest), "only the leading 16 bytes count");
    }

    /// The hardest target admits only the single zero ticket — the other end of the range.
    #[test]
    fn the_hardest_target_admits_one_ticket() {
        assert!(palw_ticket_admits_v1(0, 0));
        assert!(!palw_ticket_admits_v1(1, 0));
    }
}
