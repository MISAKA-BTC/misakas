//! ADR-0042 Decision 9 (P0-5): **one comparator, and every chain-selection site calls it.**
//!
//! The defect is not that the header processor orders tips by blue work — it is that a node then
//! holds two canonical-chain views. The header-selected-tip store stays blue-work-ordered even once
//! the virtual sink uses PALW weight, so pruning, IBD and finality can disagree with acceptance
//! about which chain is real. Two authorities inside one node is a fork that needs no attacker.
//!
//! V2 forbids the second authority rather than trying to make both compute the same thing. A
//! header-only processor genuinely cannot compute PALW weight — the weight is a function of
//! ACCEPTED TRANSACTIONS and header-first sync exists precisely to order headers whose bodies have
//! not arrived. So it keeps blue work, and what changes is its standing: a **download-ordering
//! hint**, never chain authority. `compare_palw_candidates_v1` is the authority, and the sites that
//! must call it are named in the ADR.
//!
//! ## The order, and why `live_total` is a sum
//!
//! ```text
//! 1. safe frontier      — deeper is better; a chain that matured further has more nobody disputes
//! 2. safe weight        — matured work only
//! 3. live total         — safe + bounded immature, among candidates sharing a frontier
//! 4. deterministic tie-break
//! ```
//!
//! `live_total = safe + bounded_immature` rather than "immature alone" for a reason that bites in
//! practice: when a claim matures it leaves the immature set and joins `safe`. If `live` counted
//! only immature work, maturing would LOWER a chain's live total and a node would reorg away from
//! the chain that just got better. Summing makes maturing monotone.

use crate::Hash64;
use core::cmp::Ordering;

/// The two weights plus the frontier they were measured against.
///
/// A frontier without weights, or weights without a frontier, is half an answer — and the half that
/// is missing is the one that decides. Carried together so no caller can compare across mismatched
/// halves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCandidateOrderV1 {
    /// Blue score of the deepest block on this chain whose PALW work is `Final`.
    ///
    /// The ordering's first key, ahead of weight, because matured work is the thing an attacker
    /// cannot manufacture privately: a fork nobody could see collects no receipts, so it has no
    /// frontier however much unproven work it piles up.
    pub safe_frontier_blue_score: u64,
    /// Σ pwu over `Final` blocks.
    pub safe_weight: u128,
    /// `safe_weight` + the bounded immature contribution. Always `>= safe_weight`.
    pub live_total: u128,
    /// The tie-break of last resort — the candidate's own hash. Present so the order is TOTAL:
    /// two candidates that agree on everything else must still order deterministically, or two
    /// nodes with different insertion histories pick different tips.
    pub candidate: Hash64,
}

impl PalwCandidateOrderV1 {
    /// `live_total` can never be below `safe_weight`; this constructs the pair so it cannot be.
    ///
    /// Taking `bounded_immature` and adding rather than taking `live_total` directly is the point:
    /// a caller that computed live as "immature only" would produce a total that FALLS when a claim
    /// matures, and a node would reorg away from the chain that just improved. The type refuses to
    /// represent that.
    pub fn new(safe_frontier_blue_score: u64, safe_weight: u128, bounded_immature: u128, candidate: Hash64) -> Self {
        Self { safe_frontier_blue_score, safe_weight, live_total: safe_weight.saturating_add(bounded_immature), candidate }
    }
}

/// **The** PALW fork-choice order. Every chain-selection site calls this one function.
///
/// `Greater` means the first candidate wins. Total on distinct candidates, because the last key is
/// the candidate hash — without it two chains agreeing on frontier and both weights would compare
/// `Equal`, and each node would keep whichever it happened to hold.
pub fn compare_palw_candidates_v1(a: &PalwCandidateOrderV1, b: &PalwCandidateOrderV1) -> Ordering {
    a.safe_frontier_blue_score
        .cmp(&b.safe_frontier_blue_score)
        .then_with(|| a.safe_weight.cmp(&b.safe_weight))
        .then_with(|| a.live_total.cmp(&b.live_total))
        .then_with(|| a.candidate.cmp(&b.candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// The frontier outranks weight, and that ordering is the anti-fabrication rule.
    ///
    /// A private fork can pile up unproven work without limit — it just cannot mature any of it,
    /// because a chain nobody could see collects no receipts. Ordering by weight first would let
    /// that pile outrank a shorter chain that actually matured, which is the fabrication the ramp
    /// exists to stop, arriving through fork choice instead.
    #[test]
    fn a_deeper_matured_frontier_beats_a_heavier_unproven_pile() {
        let matured = PalwCandidateOrderV1::new(100, 10, 0, h(1));
        let pile = PalwCandidateOrderV1::new(50, 9, 1_000_000, h(2));
        assert_eq!(compare_palw_candidates_v1(&matured, &pile), Ordering::Greater);
    }

    /// Among candidates sharing a frontier, safe weight decides before live.
    #[test]
    fn safe_weight_decides_before_live() {
        let safer = PalwCandidateOrderV1::new(100, 20, 0, h(1));
        let livelier = PalwCandidateOrderV1::new(100, 19, 500, h(2));
        assert_eq!(compare_palw_candidates_v1(&safer, &livelier), Ordering::Greater);
    }

    /// **Maturing never lowers the total.**
    ///
    /// The bug this shape prevents: with `live` counting only immature work, a claim maturing moves
    /// its pwu from the immature set into `safe` and the live number DROPS — so a node reorgs away
    /// from the chain that just got better, and does it again every time one matures.
    #[test]
    fn a_claim_maturing_never_lowers_the_total() {
        let before = PalwCandidateOrderV1::new(100, 10, 5, h(1));
        // The same chain one maturity later: 5 pwu moved from immature into safe.
        let after = PalwCandidateOrderV1::new(101, 15, 0, h(1));
        assert!(after.live_total >= before.live_total, "{} < {}", after.live_total, before.live_total);
        assert_eq!(compare_palw_candidates_v1(&after, &before), Ordering::Greater);
    }

    /// `live_total` is constructed, never supplied, so it cannot sit below `safe_weight`.
    #[test]
    fn live_can_never_be_below_safe() {
        for (safe, immature) in [(0u128, 0u128), (10, 0), (0, 10), (u128::MAX, 5), (5, u128::MAX)] {
            let o = PalwCandidateOrderV1::new(1, safe, immature, h(1));
            assert!(o.live_total >= o.safe_weight);
        }
    }

    /// The order is TOTAL on distinct candidates — otherwise two nodes keep different tips.
    #[test]
    fn candidates_that_agree_on_everything_still_order() {
        let a = PalwCandidateOrderV1::new(7, 7, 7, h(1));
        let b = PalwCandidateOrderV1::new(7, 7, 7, h(2));
        assert_eq!(compare_palw_candidates_v1(&a, &b), Ordering::Less);
        assert_eq!(compare_palw_candidates_v1(&b, &a), Ordering::Greater);
        assert_eq!(compare_palw_candidates_v1(&a, &a), Ordering::Equal);
    }

    /// Sorting is permutation-invariant, which is what "two nodes with different insertion
    /// histories pick the same tip" means operationally.
    #[test]
    fn the_winner_does_not_depend_on_the_order_they_arrived_in() {
        let set = [
            PalwCandidateOrderV1::new(100, 10, 3, h(4)),
            PalwCandidateOrderV1::new(100, 10, 3, h(1)),
            PalwCandidateOrderV1::new(101, 1, 0, h(9)),
            PalwCandidateOrderV1::new(100, 11, 0, h(2)),
        ];
        let best = |mut v: Vec<PalwCandidateOrderV1>| {
            v.sort_by(compare_palw_candidates_v1);
            *v.last().unwrap()
        };
        let forward = best(set.to_vec());
        let mut reversed = set.to_vec();
        reversed.reverse();
        assert_eq!(best(reversed), forward);
        let rotated = vec![set[2], set[3], set[0], set[1]];
        assert_eq!(best(rotated), forward);
        assert_eq!(forward.safe_frontier_blue_score, 101, "the deepest frontier wins regardless of arrival order");
    }
}
