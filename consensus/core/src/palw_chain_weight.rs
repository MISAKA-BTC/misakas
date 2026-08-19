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
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
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

/// Which rule a network orders candidate tips by.
///
/// The two rules exist so the OFF case is a value rather than an absence: a network with no PALW
/// fork-choice fence orders by blue work exactly as it does today, and that is stated here rather
/// than implied by skipping a branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTipOrderV1 {
    /// No fence: blue work alone, byte-for-byte today's behaviour.
    BlueWorkOnly,
    /// Fence active: ADR-0039 W4′ — `(safe, live)` first, blue work as the tie-break.
    PalwWeighted,
}

/// The one place a candidate tip order is decided.
///
/// # Precondition: `W` must be TOTAL on distinct candidates
///
/// The seam is only as total as its fallback. If two distinct candidates compare `Equal` — same
/// PALW weights and a fallback that cannot separate them — then a stable sort or a heap keeps
/// whichever arrived first, and the selected tip depends on insertion order. The adversarial
/// suite found this by sorting candidates with duplicate fallbacks and getting different
/// sequences from different permutations.
///
/// At both call sites `W` is [`crate::sortable_block::SortableBlock`], whose `Ord` is
/// `blue_work` then `hash`, so distinct blocks can never tie and the precondition holds. Passing
/// a fallback that is not total on distinct candidates reintroduces insertion-order dependence,
/// which is the partition this seam exists to prevent.
///
/// The `W` fallback is the caller's EXISTING total order, not a work scalar. At both call sites
/// that is `SortableBlock`, whose `Ord` is `blue_work` then `hash` — passing only the blue work
/// would silently drop the hash tie-break and change which of two equal-work tips wins, which is
/// exactly the kind of quiet behaviour change a seam is supposed to prevent. `BlueWorkOnly` is
/// therefore an exact restatement of the existing comparison rather than an approximation of it
/// ([`tests::fence_off_is_exactly_the_blue_work_order`]).
///
/// Under `PalwWeighted`, a candidate whose PALW weights this node could not resolve is ordered as
/// `None` — and `None` loses to any resolved candidate rather than being treated as zero-weight
/// or as an error here, because tip ordering must remain total. Refusing an unresolvable fact is
/// [`chain_weights_v1`]'s job, upstream; by the time a candidate reaches this comparison it has
/// either resolved weights or it is not a candidate this node will build on.
pub fn order_tips_v1<W: Ord>(
    rule: PalwTipOrderV1,
    a: (Option<&PalwChainWeightsV1>, &W),
    b: (Option<&PalwChainWeightsV1>, &W),
) -> core::cmp::Ordering {
    match rule {
        PalwTipOrderV1::BlueWorkOnly => a.1.cmp(b.1),
        PalwTipOrderV1::PalwWeighted => match (a.0, b.0) {
            (Some(x), Some(y)) => compare_tips_v1(x, y).then_with(|| a.1.cmp(b.1)),
            (Some(_), None) => core::cmp::Ordering::Greater,
            (None, Some(_)) => core::cmp::Ordering::Less,
            (None, None) => a.1.cmp(b.1),
        },
    }
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

    /// **The bridge from the block layer's monotonicity to this one's**, and without it the block
    /// layer's guarantee says nothing about fork choice.
    ///
    /// `palw_facts` pins that with the carriage fixed a block only ever moves UP the ladder
    /// `Provisional < ReceiptLicensed < Final`. That is a statement about one block's stage; what
    /// fork choice compares is `(safe, live)` over a whole chain. The two connect only if advancing
    /// any block's stage moves both weights monotonically — so it is asserted here rather than
    /// assumed, over every ordered pair of rungs and with the rest of the chain held fixed.
    ///
    /// `live` is the one that can surprise. A maturing block LEAVES the immature pool and enters
    /// `safe`, so its contribution jumps from `β·pwu` to `pwu` — non-decreasing precisely because
    /// `β <= 1000‰`, which is why `PalwChainWeightParamsV1::validate` rejects a larger bound. With
    /// `β > 1000‰` a block would lower its own chain's live weight by maturing, and a chain could
    /// be reorged away by the very act of its work being verified.
    #[test]
    fn advancing_a_blocks_stage_never_lowers_either_weight() {
        // In ladder order. `Voided` is deliberately absent: the block layer's rule is that a
        // conviction is constant, so no block ever enters or leaves it, and it is not a rung.
        const LADDER: [S; 3] = [S::Provisional, S::ReceiptLicensed, S::Final];

        // Enough context that the moving block is never the only contributor to either weight.
        let context = [b(7_000, S::Final), b(3_000, S::ReceiptLicensed), b(500, S::Voided), b(11, S::Provisional)];

        for (i, from) in LADDER.iter().enumerate() {
            for to in &LADDER[i..] {
                for moving_pwu in [0, 1, 999, 1_000_000] {
                    let with = |stage: S| {
                        let mut chain = context.to_vec();
                        chain.insert(2, b(moving_pwu, stage));
                        weights(&chain)
                    };
                    let (before, after) = (with(*from), with(*to));
                    assert!(
                        after.safe >= before.safe,
                        "pwu {moving_pwu}: {from:?} -> {to:?} lowered safe {} -> {}",
                        before.safe,
                        after.safe
                    );
                    assert!(
                        after.live >= before.live,
                        "pwu {moving_pwu}: {from:?} -> {to:?} lowered live {} -> {}",
                        before.live,
                        after.live
                    );
                }
            }
        }

        // Monotone alone is satisfied by a table that makes every rung identical, so the ladder
        // must also be a ladder: below the neutral bound, maturing real work STRICTLY buys weight.
        // This is what a chain gains by getting its receipts, and it is the reason withholding them
        // delays maturity rather than production.
        let rung = |stage: S| weights(&[b(4_242, stage)]);
        assert_eq!(
            rung(S::ReceiptLicensed).live,
            rung(S::Provisional).live,
            "the two immature rungs are alike here by design — ρ_r is priced inside pwu, not here"
        );
        assert!(rung(S::Final).live > rung(S::ReceiptLicensed).live, "maturing must buy live weight at β = 100‰");
        assert!(rung(S::Final).safe > rung(S::ReceiptLicensed).safe, "and it is the only rung that buys safe weight at all");

        // And the bound is what makes the `live` half true, not an accident of the fixture's
        // numbers: at the largest bound `validate` admits, maturing is exactly weight-neutral for
        // live — the boundary case, and the one a larger bound would push negative.
        let flat = PalwChainWeightParamsV1 { immature_bound_permille: 1_000 };
        let at = |stage: S| chain_weights_v1(&[b(4_242, stage)], &flat).expect("resolves").live;
        assert_eq!(at(S::ReceiptLicensed), at(S::Final), "β = 1000‰ is the neutral boundary");
        assert!(PalwChainWeightParamsV1 { immature_bound_permille: 1_001 }.validate().is_err(), "and nothing past it is admissible");
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

    /// **Fence OFF is not "approximately today" — it is today.** Every shipped preset runs this
    /// arm, so it must reproduce the existing blue-work comparison for every pair, including the
    /// equal case, and must ignore PALW weights entirely even when they are present.
    #[test]
    fn fence_off_is_exactly_the_blue_work_order() {
        let heavy = PalwChainWeightsV1 { safe: u128::MAX, live: u128::MAX };
        let nothing = PalwChainWeightsV1 { safe: 0, live: 0 };
        for a in 0u64..24 {
            for z in 0u64..24 {
                // With PALW weights deliberately contradicting the blue-work order, the OFF arm
                // must still answer exactly what blue work says.
                assert_eq!(
                    order_tips_v1(PalwTipOrderV1::BlueWorkOnly, (Some(&nothing), &a), (Some(&heavy), &z)),
                    a.cmp(&z),
                    "({a},{z}): the fence-off order must be the blue-work order"
                );
                // And with no weights at all, which is the state every preset is actually in.
                assert_eq!(order_tips_v1(PalwTipOrderV1::BlueWorkOnly, (None, &a), (None, &z)), a.cmp(&z));
            }
        }
    }

    /// Fence ON: PALW weights decide, blue work becomes the tie-break, and an unresolved
    /// candidate loses to a resolved one without making the order partial.
    #[test]
    fn fence_on_puts_palw_first_and_blue_work_last() {
        use core::cmp::Ordering::*;
        let more_safe = PalwChainWeightsV1 { safe: 10, live: 10 };
        let less_safe = PalwChainWeightsV1 { safe: 9, live: u128::MAX };
        // PALW outranks blue work: the lighter-blue-work candidate wins on safe weight.
        assert_eq!(order_tips_v1(PalwTipOrderV1::PalwWeighted, (Some(&more_safe), &0u64), (Some(&less_safe), &999u64)), Greater);
        // Equal PALW weights ⇒ blue work breaks the tie, so the order stays total.
        assert_eq!(order_tips_v1(PalwTipOrderV1::PalwWeighted, (Some(&more_safe), &7u64), (Some(&more_safe), &8u64)), Less);
        assert_eq!(order_tips_v1(PalwTipOrderV1::PalwWeighted, (Some(&more_safe), &7u64), (Some(&more_safe), &7u64)), Equal);
        // Unresolved loses to resolved; two unresolved fall back to blue work.
        assert_eq!(order_tips_v1(PalwTipOrderV1::PalwWeighted, (None, &999u64), (Some(&less_safe), &0u64)), Less);
        assert_eq!(order_tips_v1(PalwTipOrderV1::PalwWeighted, (Some(&less_safe), &0u64), (None, &999u64)), Greater);
        assert_eq!(order_tips_v1(PalwTipOrderV1::PalwWeighted, (None, &7u64), (None, &8u64)), Less);
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

/// ADR-0039's release gate: the adversarial suite that must pass before any value network.
///
/// The theorem is one sentence — **equal DAGs give equal weights** — and the suite's job is to
/// try to break it from every direction a real node differs from another: the order facts
/// arrived in, the depth a conviction was observed at, where a pruned node started its walk, and
/// which candidate a tip comparison saw first. ADR-0038 named mutable-weight fork choice as its
/// own hardest correctness target; this makes that a gate rather than an intention.
#[cfg(test)]
mod adversarial_suite {
    use super::*;
    use PalwWorkRampStageV1 as S;

    const PARAMS: PalwChainWeightParamsV1 = PalwChainWeightParamsV1 { immature_bound_permille: 100 };

    fn b(pwu: u64, stage: S) -> Option<PalwBlockWeightV1> {
        Some(PalwBlockWeightV1 { pwu, stage })
    }

    /// A deterministic shuffle — no `rand`, so the suite cannot introduce the nondeterminism it
    /// is checking for, and a failure is reproducible from its seed.
    fn permute<T: Clone>(items: &[T], seed: u64) -> Vec<T> {
        let mut out = items.to_vec();
        let mut state = seed | 1;
        for i in (1..out.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            out.swap(i, (state % (i as u64 + 1)) as usize);
        }
        out
    }

    fn dag() -> Vec<Option<PalwBlockWeightV1>> {
        (0..96)
            .map(|i| {
                let stage = match i % 4 {
                    0 => S::Final,
                    1 => S::ReceiptLicensed,
                    2 => S::Provisional,
                    _ => S::Voided,
                };
                b((i as u64 + 1) * 1_000, stage)
            })
            .collect()
    }

    /// **Receipt arrival order.** Receipts ride successor blocks, so two nodes learn them in
    /// different orders as blocks relay. That must not change either weight.
    #[test]
    fn arrival_order_cannot_change_either_weight() {
        let facts = dag();
        let expected = chain_weights_v1(&facts, &PARAMS).unwrap();
        for seed in 1..500u64 {
            assert_eq!(chain_weights_v1(&permute(&facts, seed), &PARAMS).unwrap(), expected, "seed {seed}");
        }
    }

    /// **Conviction observation depth.** A conviction seen early and one seen late are the same
    /// fact about the same block; only `convicted_before_close` distinguishes them, and that is
    /// already resolved before it reaches here. Moving a Voided block anywhere in the walk must
    /// not move the totals.
    #[test]
    fn where_a_conviction_sits_in_the_walk_does_not_matter() {
        let mut facts = dag();
        let expected = chain_weights_v1(&facts, &PARAMS).unwrap();
        // Move the first Voided entry to every position in turn.
        let voided_at = facts.iter().position(|f| matches!(f, Some(w) if w.stage == S::Voided)).unwrap();
        let voided = facts.remove(voided_at);
        for position in 0..=facts.len() {
            let mut moved = facts.clone();
            moved.insert(position, voided);
            assert_eq!(chain_weights_v1(&moved, &PARAMS).unwrap(), expected, "voided at {position}");
        }
    }

    /// **Pruned start height.** A node that begins its walk later sees a SUFFIX. Its answer must
    /// be the suffix's answer — never the full chain's, and never a silently lighter version of
    /// it. This is the property that makes a pruned node's disagreement visible instead of
    /// plausible.
    #[test]
    fn a_suffix_weighs_the_suffix_and_says_so() {
        let facts = dag();
        let whole = chain_weights_v1(&facts, &PARAMS).unwrap();
        for start in 1..facts.len() {
            let suffix = chain_weights_v1(&facts[start..], &PARAMS).unwrap();
            assert!(suffix.safe <= whole.safe, "a suffix cannot outweigh the whole");
            assert!(suffix.live <= whole.live);
        }
        // And a walk that cannot resolve one of its blocks refuses outright rather than
        // returning the lighter chain that skipping it would produce.
        let mut holed = facts.clone();
        holed[40] = None;
        assert_eq!(chain_weights_v1(&holed, &PARAMS), Err(PalwChainWeightError::UnresolvedBlock { index: 40 }));
    }

    /// **Comparison order.** `order_tips_v1` must be a strict total order: antisymmetric,
    /// transitive, and reflexive-equal — otherwise a `BinaryHeap` built from it can pop a
    /// different tip depending on insertion order, which is the same partition by another route.
    #[test]
    fn the_tip_order_is_a_total_order_under_both_rules() {
        let weights: Vec<Option<PalwChainWeightsV1>> = vec![
            None,
            Some(PalwChainWeightsV1 { safe: 0, live: 0 }),
            Some(PalwChainWeightsV1 { safe: 0, live: 5 }),
            Some(PalwChainWeightsV1 { safe: 10, live: 10 }),
            Some(PalwChainWeightsV1 { safe: 10, live: 99 }),
        ];
        let fallbacks: Vec<u64> = vec![0, 1, 7, 7, 42];
        for rule in [PalwTipOrderV1::BlueWorkOnly, PalwTipOrderV1::PalwWeighted] {
            let items: Vec<_> = weights.iter().flat_map(|w| fallbacks.iter().map(move |f| (w.as_ref(), f))).collect();
            for a in &items {
                assert_eq!(order_tips_v1(rule, (a.0, a.1), (a.0, a.1)), core::cmp::Ordering::Equal, "reflexive");
                for z in &items {
                    // Antisymmetry.
                    assert_eq!(
                        order_tips_v1(rule, (a.0, a.1), (z.0, z.1)),
                        order_tips_v1(rule, (z.0, z.1), (a.0, a.1)).reverse(),
                        "antisymmetric"
                    );
                    for c in &items {
                        // Transitivity, on the strict relation.
                        if order_tips_v1(rule, (a.0, a.1), (z.0, z.1)) == core::cmp::Ordering::Greater
                            && order_tips_v1(rule, (z.0, z.1), (c.0, c.1)) == core::cmp::Ordering::Greater
                        {
                            assert_eq!(
                                order_tips_v1(rule, (a.0, a.1), (c.0, c.1)),
                                core::cmp::Ordering::Greater,
                                "transitive"
                            );
                        }
                    }
                }
            }
        }
    }

    /// **Sorting is insertion-order independent — GIVEN a total fallback.**
    ///
    /// The property a heap actually relies on. It holds exactly when `W` separates distinct
    /// candidates: this suite's first version used duplicate `u64` fallbacks, two distinct
    /// candidates therefore compared `Equal`, and a stable sort kept whichever arrived first —
    /// different permutations, different sequences. That is a real precondition rather than a
    /// bad test, and it is now stated on `order_tips_v1`. At both call sites `W` is
    /// `SortableBlock`, whose hash tie-break makes ties between distinct blocks impossible; the
    /// fallbacks below are distinct for the same reason.
    #[test]
    fn the_same_candidates_sort_the_same_however_they_arrive() {
        let candidates: Vec<(Option<PalwChainWeightsV1>, u64)> = (0..40)
            .map(|i| {
                let w = if i % 3 == 0 { None } else { Some(PalwChainWeightsV1 { safe: (i % 5) as u128, live: (i % 7) as u128 }) };
                // DISTINCT fallbacks — the `SortableBlock` property, modelled.
                (w, i as u64)
            })
            .collect();
        let sort = |v: &[(Option<PalwChainWeightsV1>, u64)], rule| {
            let mut v = v.to_vec();
            v.sort_by(|a, z| order_tips_v1(rule, (a.0.as_ref(), &a.1), (z.0.as_ref(), &z.1)));
            v
        };
        for rule in [PalwTipOrderV1::BlueWorkOnly, PalwTipOrderV1::PalwWeighted] {
            let expected = sort(&candidates, rule);
            for seed in 1..200u64 {
                assert_eq!(sort(&permute(&candidates, seed), rule), expected, "rule {rule:?} seed {seed}");
            }
        }
    }

    /// The precondition, stated as a test so it cannot be forgotten: a fallback that CANNOT
    /// separate two distinct candidates makes them compare `Equal`, and an `Equal` pair is where
    /// insertion order leaks in. `SortableBlock` avoids it by construction.
    #[test]
    fn a_non_total_fallback_produces_ties_and_that_is_the_precondition() {
        let a = (Some(PalwChainWeightsV1 { safe: 1, live: 1 }), 7u64);
        let z = (Some(PalwChainWeightsV1 { safe: 1, live: 1 }), 7u64);
        assert_eq!(
            order_tips_v1(PalwTipOrderV1::PalwWeighted, (a.0.as_ref(), &a.1), (z.0.as_ref(), &z.1)),
            core::cmp::Ordering::Equal,
            "identical weights and an equal fallback tie — which is why the fallback must be total"
        );
        // SortableBlock cannot do this: distinct blocks differ in the hash even at equal work.
        use crate::sortable_block::SortableBlock;
        use kaspa_hashes::Hash64;
        let heavy = SortableBlock::new(Hash64::from_u64_word(1), 100u64.into());
        let other = SortableBlock::new(Hash64::from_u64_word(2), 100u64.into());
        assert_ne!(
            order_tips_v1(PalwTipOrderV1::BlueWorkOnly, (None, &heavy), (None, &other)),
            core::cmp::Ordering::Equal,
            "the hash tie-break is what makes the real fallback total"
        );
    }
}
