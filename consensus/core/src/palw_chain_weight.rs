//! ADR-0039 W4′: the two derived chain weights, as pure functions.
//!
//! ```text
//! safe(C) = Σ pwu(b)                      over b ∈ C with stage == Final
//! live(C) = safe(C) + β · Σ pwu(b)        over b ∈ C with stage ∈ {Provisional, ReceiptLicensed}
//! ```
//!
//! * **safe** governs IBD, deep-reorg bounds and economic finality. A private fork cannot
//!   accumulate it: `Final` now requires a receipt quorum ([`crate::palw_weight::ramp_stage_v1`]),
//!   and a fork nobody could see collects no receipts. This is the half that kills fabricated
//!   chains.
//! * **live** governs tip selection only. Its purpose is the opposite: to stop assigned
//!   verifiers holding an honest chain hostage. Withholding receipts must delay *maturity*,
//!   never *production*, so published bonded work counts — at a bounded fraction β fixed at
//!   registration, never at par with matured work.
//!
//! `Voided` contributes zero to both, forever.
//!
//! ## Why neither is `blue_work`
//!
//! `header.blue_work` is miner-declared, sits in the pre-PoW preimage, and is validated by exact
//! equality against the recomputed GHOSTDAG value; the pruning proof, difficulty and window
//! machinery all read it. A value that matures after the fact cannot also be a value fixed under
//! a PoW commitment and re-derived identically by a pruning proof — so PALW weight is **never
//! serialized into a header**. It is derived here, per node, from facts, and `blue_work` is left
//! to be exactly what it already is.
//!
//! ## The determinism contract (ADR-0039 §3d) — what these functions guarantee
//!
//! Everything here is a function of a *set* of facts. Summation is commutative and associative
//! over `u128`, so the **order the facts arrived in cannot change the answer** — which is the
//! whole of "equal DAGs ⟹ equal weights" at this layer, and is what
//! [`tests::equal_dags_give_equal_weights_under_every_permutation`] pins.
//!
//! What these functions therefore do NOT do, deliberately: read a clock, read receipt arrival
//! order, read peer-observation order, or use floating point. The remaining obligation is the
//! caller's — it must assemble the fact set from chain state alone, at blue-score or
//! finalized-epoch boundaries rather than "when I saw it". [`chain_weights_v1`] helps by refusing
//! a fact it cannot resolve instead of treating it as zero: a `None` entry is an error, not an
//! absence. That is old blocker 6's root cause (pruned acceptance read as "nothing") made
//! unrepresentable at this boundary.
//!
//! Consensus-inert: nothing calls these yet. Wiring to tip selection is a separate change, and
//! ADR-0039 requires an adversarial suite before any value network.

use crate::palw_weight::PalwWorkRampStageV1;
use thiserror::Error;

/// β denominator: the immature bound is permille, integer math only.
pub const PALW_CHAIN_WEIGHT_PERMILLE_DENOMINATOR: u128 = 1000;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwChainWeightError {
    #[error("immature_bound_permille is {got}‰, above the denominator 1000‰")]
    BoundOutOfRange { got: u16 },
    #[error("block at index {index} has no resolved weight facts — a fact this node cannot resolve is an error, never a zero")]
    UnresolvedBlock { index: usize },
}

/// One block's resolved contribution: its derived pwu ([`crate::palw_pwu`]) and its ramp stage
/// ([`crate::palw_weight::ramp_stage_v1`]). Both are facts about chain state; neither is an input
/// the block's producer chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBlockWeightV1 {
    pub pwu: u64,
    pub stage: PalwWorkRampStageV1,
}

/// The registration-time bound on how much immature work may count toward live weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwChainWeightParamsV1 {
    /// β — the fraction of an immature block's pwu that counts toward LIVE weight, in permille.
    ///
    /// Deliberately a registration constant and not a tunable: two nodes that disagree about
    /// nothing else must not disagree about this, or they compute different tips. Small by
    /// intent — its job is to let honest production continue while receipts are outstanding,
    /// not to let unverified work compete with verified work.
    pub immature_bound_permille: u16,
}

impl PalwChainWeightParamsV1 {
    pub fn validate(&self) -> Result<(), PalwChainWeightError> {
        if self.immature_bound_permille as u128 > PALW_CHAIN_WEIGHT_PERMILLE_DENOMINATOR {
            return Err(PalwChainWeightError::BoundOutOfRange { got: self.immature_bound_permille });
        }
        Ok(())
    }
}

/// The two weights of one chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PalwChainWeightsV1 {
    /// Matured work only. IBD, deep reorg, finality.
    pub safe: u128,
    /// `safe` plus bounded immature work. Tip selection only. Always `>= safe`.
    pub live: u128,
}

/// Both weights over a chain's resolved facts.
///
/// `chain` is the complete fact set for the window under consideration — one entry per block,
/// `None` where this node could not resolve that block's facts. A `None` is refused rather than
/// skipped: silently omitting an unresolvable block is exactly how a pruned node and an archival
/// node come to disagree about a tip.
pub fn chain_weights_v1(
    chain: &[Option<PalwBlockWeightV1>],
    params: &PalwChainWeightParamsV1,
) -> Result<PalwChainWeightsV1, PalwChainWeightError> {
    params.validate()?;
    let mut safe: u128 = 0;
    let mut immature: u128 = 0;
    for (index, entry) in chain.iter().enumerate() {
        let Some(block) = entry else {
            return Err(PalwChainWeightError::UnresolvedBlock { index });
        };
        match block.stage {
            // Matured: counts in full, in both weights.
            PalwWorkRampStageV1::Final => safe = safe.saturating_add(block.pwu as u128),
            // Published but not matured: live only, and bounded.
            PalwWorkRampStageV1::Provisional | PalwWorkRampStageV1::ReceiptLicensed => {
                immature = immature.saturating_add(block.pwu as u128)
            }
            // Convicted: nothing, forever, in either weight.
            PalwWorkRampStageV1::Voided => {}
        }
    }
    // β applied to the immature TOTAL rather than per block: per-block flooring would let a
    // producer split the same work across many small blocks to round each one down, and would
    // make the result depend on how the work happened to be partitioned.
    let bounded = immature.saturating_mul(params.immature_bound_permille as u128) / PALW_CHAIN_WEIGHT_PERMILLE_DENOMINATOR;
    Ok(PalwChainWeightsV1 { safe, live: safe.saturating_add(bounded) })
}

/// The fork-choice order over two candidate tips: **safe first, then live.**
///
/// ADR-0039 §3c states the rule procedurally — take the highest safe-weight block, then select
/// among its descendants by live weight. As a total order on candidates that is exactly
/// lexicographic `(safe, live)`: a candidate extending a more-matured chain wins outright, and
/// live weight only ever decides between candidates whose matured history is equal. Because
/// maturity requires a closed challenge window, two competing recent tips share all their mature
/// ancestors and therefore their safe weight — so in the common case live decides, and a deep
/// reorg must bring matured work, which it cannot fabricate.
///
/// `Equal` means the PALW layer is indifferent and the caller's existing tie-break applies.
pub fn compare_tips_v1(a: &PalwChainWeightsV1, b: &PalwChainWeightsV1) -> core::cmp::Ordering {
    a.safe.cmp(&b.safe).then(a.live.cmp(&b.live))
}

#[cfg(test)]
mod tests {
    use super::*;
    use PalwWorkRampStageV1 as S;

    const PARAMS: PalwChainWeightParamsV1 = PalwChainWeightParamsV1 { immature_bound_permille: 100 };

    fn b(pwu: u64, stage: S) -> Option<PalwBlockWeightV1> {
        Some(PalwBlockWeightV1 { pwu, stage })
    }

    fn weights(chain: &[Option<PalwBlockWeightV1>]) -> PalwChainWeightsV1 {
        chain_weights_v1(chain, &PARAMS).expect("fixture chains resolve")
    }

    /// A deterministic permutation generator — no `rand`, so the suite is reproducible and the
    /// test itself cannot introduce the nondeterminism it is checking for.
    fn permute<T: Clone>(items: &[T], seed: u64) -> Vec<T> {
        let mut out = items.to_vec();
        let mut state = seed | 1;
        // Fisher-Yates driven by a xorshift; same seed, same permutation, on every machine.
        for i in (1..out.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            out.swap(i, (state % (i as u64 + 1)) as usize);
        }
        out
    }

    /// **The determinism theorem at this layer: equal DAGs give equal weights.**
    ///
    /// Two nodes that agree on the fact set must agree on both weights no matter what order they
    /// learned the facts in — receipts arriving on different successor blocks, convictions
    /// observed at different depths, a pruned node walking a different range. Summation is
    /// commutative, and this pins that nothing else sneaks in.
    #[test]
    fn equal_dags_give_equal_weights_under_every_permutation() {
        let chain = vec![
            b(1_000, S::Final),
            b(2_000, S::ReceiptLicensed),
            b(4_000, S::Provisional),
            b(8_000, S::Voided),
            b(16_000, S::Final),
            b(32_000, S::ReceiptLicensed),
            b(64_000, S::Voided),
            b(128_000, S::Provisional),
        ];
        let expected = weights(&chain);
        for seed in 1..200u64 {
            assert_eq!(weights(&permute(&chain, seed)), expected, "seed {seed}: order changed the weights");
        }
        // Reversal and rotation, the two orders a real implementation is most likely to produce
        // (walking a chain tip-first vs genesis-first).
        let mut reversed = chain.clone();
        reversed.reverse();
        assert_eq!(weights(&reversed), expected);
        let mut rotated = chain.clone();
        rotated.rotate_left(3);
        assert_eq!(weights(&rotated), expected);
    }

    /// The stage table, at the chain layer: Final counts in both, immature in live only and
    /// bounded, Voided in neither.
    #[test]
    fn stages_contribute_exactly_where_the_adr_says() {
        assert_eq!(weights(&[b(1_000, S::Final)]), PalwChainWeightsV1 { safe: 1_000, live: 1_000 });
        // β = 100‰ ⇒ a 1_000-pwu immature block adds 100 to live and nothing to safe.
        assert_eq!(weights(&[b(1_000, S::Provisional)]), PalwChainWeightsV1 { safe: 0, live: 100 });
        assert_eq!(weights(&[b(1_000, S::ReceiptLicensed)]), PalwChainWeightsV1 { safe: 0, live: 100 });
        assert_eq!(weights(&[b(u64::MAX, S::Voided)]), PalwChainWeightsV1 { safe: 0, live: 0 });
        // live is never below safe.
        let mixed = weights(&[b(5, S::Final), b(1_000, S::Provisional), b(7, S::Voided)]);
        assert!(mixed.live >= mixed.safe);
        assert_eq!(mixed, PalwChainWeightsV1 { safe: 5, live: 105 });
    }

    /// **A private fork of fabricated blocks has zero safe weight**, whatever pwu it claims —
    /// the property the whole two-weight split exists for. It cannot mature because it cannot
    /// collect receipts while private, so it competes only on the bounded live term.
    #[test]
    fn a_private_fork_has_no_safe_weight() {
        let fabricated: Vec<_> = (0..64).map(|_| b(u64::MAX, S::Provisional)).collect();
        let forged = weights(&fabricated);
        assert_eq!(forged.safe, 0, "unmatured work is not safe weight at any magnitude");

        // One matured honest block outranks the entire fabricated fork under the fork-choice
        // order, because safe is compared first.
        let honest = weights(&[b(1, S::Final)]);
        assert_eq!(compare_tips_v1(&honest, &forged), core::cmp::Ordering::Greater);
    }

    /// Fork choice: safe strictly dominates live, and the order is a proper total order.
    #[test]
    fn fork_choice_is_safe_then_live() {
        use core::cmp::Ordering::*;
        let more_safe = PalwChainWeightsV1 { safe: 10, live: 10 };
        let more_live = PalwChainWeightsV1 { safe: 9, live: 1_000_000 };
        assert_eq!(compare_tips_v1(&more_safe, &more_live), Greater, "live can never outrank safe");

        // Equal safe ⇒ live decides. This is the common case: competing recent tips share every
        // matured ancestor.
        let a = PalwChainWeightsV1 { safe: 10, live: 12 };
        let z = PalwChainWeightsV1 { safe: 10, live: 11 };
        assert_eq!(compare_tips_v1(&a, &z), Greater);
        // Indifference is Equal — the caller's existing tie-break applies, not a coin flip here.
        assert_eq!(compare_tips_v1(&a, &a), Equal);
        // Antisymmetry and transitivity on a sample, so a sort over candidates is well-defined.
        assert_eq!(compare_tips_v1(&z, &a), Less);
        assert_eq!(compare_tips_v1(&more_safe, &a), Less);
        assert_eq!(compare_tips_v1(&more_live, &z), Less);
    }

    /// An unresolvable fact is an ERROR, not a zero — old blocker 6's root cause (pruned
    /// acceptance data read as "nothing"), made unrepresentable at this boundary. A node that
    /// cannot resolve a block must refuse to weigh the chain, not quietly weigh it lighter.
    #[test]
    fn an_unresolvable_block_is_refused_not_skipped() {
        let chain = vec![b(1_000, S::Final), None, b(2_000, S::Final)];
        assert_eq!(chain_weights_v1(&chain, &PARAMS), Err(PalwChainWeightError::UnresolvedBlock { index: 1 }));
        // The dangerous alternative, stated so the difference is visible: dropping the entry
        // yields a perfectly plausible lighter chain that another node would never compute.
        let silently_skipped = weights(&[b(1_000, S::Final), b(2_000, S::Final)]);
        assert_eq!(silently_skipped.safe, 3_000);
    }

    /// Overflow saturates rather than wrapping: a wrap would turn the heaviest possible chain
    /// into the lightest, which is the one arithmetic accident fork choice cannot survive.
    #[test]
    fn accumulation_saturates_and_never_wraps() {
        let huge: Vec<_> = (0..1_000).map(|_| b(u64::MAX, S::Final)).collect();
        let w = weights(&huge);
        assert!(w.safe > u64::MAX as u128, "u128 accumulator must hold more than one u64 block");
        assert_eq!(w.live, w.safe, "no immature work ⇒ live == safe");

        // Immature at full β, at the same magnitude, still cannot exceed the accumulator.
        let full = PalwChainWeightParamsV1 { immature_bound_permille: 1_000 };
        let all_immature: Vec<_> = (0..1_000).map(|_| b(u64::MAX, S::Provisional)).collect();
        let w2 = chain_weights_v1(&all_immature, &full).unwrap();
        assert_eq!(w2.safe, 0);
        assert_eq!(w2.live, w.safe, "β = 1000‰ makes immature count like matured — in LIVE only");
    }

    /// β is bounded and validated: above 1000‰ immature work would outweigh the matured work it
    /// is derived from, which inverts the entire point of the split.
    #[test]
    fn the_immature_bound_is_validated() {
        assert!(PARAMS.validate().is_ok());
        assert!(PalwChainWeightParamsV1 { immature_bound_permille: 1_000 }.validate().is_ok());
        assert_eq!(
            PalwChainWeightParamsV1 { immature_bound_permille: 1_001 }.validate(),
            Err(PalwChainWeightError::BoundOutOfRange { got: 1_001 })
        );
        // And the bound is enforced by the weight function, not only by an explicit validate call.
        assert_eq!(
            chain_weights_v1(&[b(1, S::Final)], &PalwChainWeightParamsV1 { immature_bound_permille: 1_001 }),
            Err(PalwChainWeightError::BoundOutOfRange { got: 1_001 })
        );
    }

    /// β applies to the immature TOTAL, so splitting the same work across more blocks cannot
    /// change the answer — otherwise a producer could pick a partition that rounds in its favour,
    /// and two nodes summing in different groupings could disagree.
    #[test]
    fn the_bound_does_not_depend_on_how_work_is_partitioned() {
        let one_big = weights(&[b(1_000, S::Provisional)]);
        let many_small: Vec<_> = (0..100).map(|_| b(10, S::Provisional)).collect();
        assert_eq!(weights(&many_small), one_big);
        // The floor-per-block alternative would have produced 0 here (10 × 100‰ = 1 each, but
        // 7 × 100‰ floors to 0), so pin the case that exposes it.
        let lossy: Vec<_> = (0..100).map(|_| b(7, S::Provisional)).collect();
        assert_eq!(weights(&lossy).live, 70, "700 pwu at 100‰ is 70, however it was split");
    }
}
