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
    let attempts = palw_expected_attempts_v1(class_target) as u128;
    let product = attempts.saturating_mul(pwu_per_inference as u128);
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
        // Half the space: two inferences' worth.
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
        assert_eq!(
            check_pwu_claim_v1(u64::MAX, target, cost),
            Err(PalwPwuError::ClaimMismatch { claimed: u64::MAX, derived })
        );
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
        let easy = u128::MAX >> 10; // 1_024 expected attempts
        let hard = u128::MAX >> 13; // 8_192 expected attempts
        assert_eq!(palw_expected_attempts_v1(easy), 1_024);
        assert_eq!(palw_expected_attempts_v1(hard), 8_192);

        // Cost scales pwu at a fixed target.
        assert_eq!(palw_pwu_v1(easy, 1_000), 10 * palw_pwu_v1(easy, 100));
        // Difficulty scales pwu at a fixed cost — 8x tighter target, 8x the work.
        assert_eq!(palw_pwu_v1(hard, 100), 8 * palw_pwu_v1(easy, 100));
        // A cheap class cannot reach an expensive one's per-block weight by difficulty alone
        // unless it actually spends the inferences; that it CAN by spending them is why
        // cross-class fairness is the epoch share cap's job, never pwu magnitude's.
        assert_eq!(palw_pwu_v1(hard, 100), palw_pwu_v1(easy, 800));
    }
}
