//! TGE settlement (ADR-0027 §6.3–6.4). Deterministic, integer-only. Turns
//! per-identity category points into an MSK (sompi) allocation from a fixed pool.
//!
//! Algorithm (§6.3), literally: (1) provisional per-category share
//! `cat_pool_c · pts_{i,c} / Σ_j pts_{j,c}`; (2) clip each ID's TOTAL to the
//! per-ID cap (`per_id_cap_bps` of the pool); (3) redistribute each capped ID's
//! excess to the *uncapped* participants of the SAME category by point ratio,
//! **once** (a recipient may thereby exceed the cap — this is the "1 回のみ"
//! rule); (4) floor; the un-allocated remainder (weight/floor rounding + excess
//! with no uncapped recipient) goes to the ecosystem fund.

use crate::rules::{AllocationRules, MilliPoints};

/// Per-identity category points, C1..C4 in canonical order.
/// ADR-0040 §16″: five categories (C1..C5) — the array is the ledger's column order, append-only.
pub type CategoryPoints = [MilliPoints; 5];

/// The number of scoring categories, taken from [`crate::Category::ALL`] so the settle loops and the
/// category table can never drift apart.
///
/// They HAD drifted: adding C5 to `Category`/`weight_bps` while `settle` still looped `0..4` made the
/// 30 % LLM pool silently drain to the ecosystem remainder every epoch — a weight that looks like an
/// allocation in the rules but pays nobody. Binding the bound to `Category::ALL` makes that failure
/// impossible rather than merely fixed once.
pub const NCAT: usize = crate::Category::ALL.len();

/// The outcome of a settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settlement {
    /// `(identity, reward_sompi)`, sorted by identity. Lossless w.r.t. the pool
    /// together with `ecosystem_remainder`.
    pub rewards: Vec<(String, u64)>,
    /// Un-allocated remainder returned to the ecosystem fund.
    pub ecosystem_remainder: u64,
}

/// Settle `pool` sompi over `ids_points` under `alloc`. Input order is
/// irrelevant — the result is sorted by identity and fully deterministic.
///
/// Takes [`AllocationRules`], never [`crate::ScoringRules`]: settlement is the only place a
/// distribution is applied, and keeping the two types apart is what stops a pool choice from
/// reaching `rules_hash` (see the v4 split). C5 must not be settled while
/// [`crate::rules::c5_token_settlement_enabled`] is `false`.
pub fn settle(pool: u64, alloc: &AllocationRules, ids_points: &[(String, CategoryPoints)]) -> Settlement {
    let mut items: Vec<(String, CategoryPoints)> = ids_points.to_vec();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let n = items.len();
    let pool = pool as u128;

    // (0) category pools + total points per category.
    let mut cat_pool = [0u128; NCAT];
    for (c, cp) in cat_pool.iter_mut().enumerate() {
        *cp = pool * alloc.weight_bps[c] as u128 / 10_000;
    }
    let mut total_pts = [0u128; NCAT];
    for (_, p) in &items {
        for c in 0..NCAT {
            total_pts[c] += p[c] as u128;
        }
    }

    // (1) provisional per-category reward.
    let mut base = vec![[0u128; NCAT]; n];
    for (i, (_, p)) in items.iter().enumerate() {
        for c in 0..NCAT {
            if total_pts[c] > 0 {
                base[i][c] = cat_pool[c] * p[c] as u128 / total_pts[c];
            }
        }
    }

    // (2) clip each ID's total to the per-ID cap; collect the removed excess per category.
    let cap = pool * alloc.per_id_cap_bps as u128 / 10_000;
    let mut kept = vec![[0u128; NCAT]; n];
    let mut capped = vec![false; n];
    let mut excess = [0u128; NCAT];
    for i in 0..n {
        let tot: u128 = base[i].iter().sum();
        if tot > cap && tot > 0 {
            capped[i] = true;
            for c in 0..NCAT {
                let k = base[i][c] * cap / tot;
                kept[i][c] = k;
                excess[c] += base[i][c] - k;
            }
        } else {
            kept[i] = base[i];
        }
    }

    // (3) redistribute excess within each category to the uncapped, by point ratio (once).
    let mut uncapped_pts = [0u128; NCAT];
    for i in 0..n {
        if !capped[i] {
            for (c, up) in uncapped_pts.iter_mut().enumerate() {
                *up += items[i].1[c] as u128;
            }
        }
    }
    for i in 0..n {
        if capped[i] {
            continue;
        }
        for c in 0..NCAT {
            if excess[c] > 0 && uncapped_pts[c] > 0 {
                kept[i][c] += excess[c] * items[i].1[c] as u128 / uncapped_pts[c];
            }
        }
    }

    // (4) finalize; remainder → ecosystem.
    let mut distributed = 0u128;
    let rewards: Vec<(String, u64)> = items
        .iter()
        .enumerate()
        .map(|(i, (id, _))| {
            let r: u128 = kept[i].iter().sum();
            distributed += r;
            (id.clone(), r as u64)
        })
        .collect();
    Settlement { rewards, ecosystem_remainder: (pool - distributed) as u64 }
}

/// Vesting split (§6.4): rewards above `vesting_threshold_bps` of the pool vest
/// `vesting_cliff_bps` at TGE and the rest linearly over 6 months; smaller ones
/// are a TGE lump. Returns `(tge_amount, linear_amount)`.
pub fn vesting_split(reward: u64, pool: u64, alloc: &AllocationRules) -> (u64, u64) {
    let threshold = (pool as u128 * alloc.vesting_threshold_bps as u128 / 10_000) as u64;
    if reward <= threshold {
        return (reward, 0);
    }
    let tge = (reward as u128 * alloc.vesting_cliff_bps as u128 / 10_000) as u64;
    (tge, reward - tge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Category;
    use crate::rules::AllocationRules;

    fn ids(v: &[(&str, CategoryPoints)]) -> Vec<(String, CategoryPoints)> {
        v.iter().map(|(a, p)| (a.to_string(), *p)).collect()
    }

    #[test]
    fn equal_participants_split_their_category_pool() {
        // Disable the per-ID cap to isolate the proportional split (each would take
        // 20% of the pool here, far above the 5% cap — see the cap test below).
        let r = AllocationRules { per_id_cap_bps: 10_000, ..AllocationRules::default() };
        // Two IDs, equal Node points only. ADR-0040 §16″ weights: Node = 25% of 1000 = 250 → 125 each.
        let s = settle(1000, &r, &ids(&[("a", [10, 0, 0, 0, 0]), ("b", [10, 0, 0, 0, 0])]));
        assert_eq!(s.rewards, vec![("a".into(), 125), ("b".into(), 125)]);
        // Bug/Verify/Infra/LLM pools (2500+1000+1000+3000 = 7500) had no points → all to ecosystem.
        assert_eq!(s.ecosystem_remainder, 750);
        // lossless.
        assert_eq!(s.rewards.iter().map(|(_, r)| r).sum::<u64>() + s.ecosystem_remainder, 1000);
    }

    /// ADR-0040 §16″ — the C5 LLM-mining pool is 30 % of the epoch and is actually REACHABLE.
    ///
    /// A weight without a contribution path is not an allocation: the pool would simply drain to the
    /// ecosystem remainder every epoch while appearing, in the rules, to reward LLM work. This asserts
    /// both halves — the pool is 30 %, and `Contribution::Fixed { category: Llm }` (the manual-award
    /// path, since PALW is not live and cannot yet be auto-scored from chain facts) lands in it.
    #[test]
    fn llm_category_pool_is_thirty_percent_and_reachable() {
        let r = AllocationRules { per_id_cap_bps: 10_000, ..AllocationRules::default() };
        assert_eq!(r.weight_bps[Category::Llm.index()], 3_000, "C5 must be 30 % of the epoch");
        assert!(r.weights_sum_to_full());

        // Only LLM points exist ⇒ exactly the C5 pool is distributed, the other four drain to ecosystem.
        let s = settle(1000, &r, &ids(&[("a", [0, 0, 0, 0, 10]), ("b", [0, 0, 0, 0, 10])]));
        assert_eq!(s.rewards, vec![("a".into(), 150), ("b".into(), 150)], "the 300-point C5 pool splits evenly");
        assert_eq!(s.ecosystem_remainder, 700);
        assert_eq!(s.rewards.iter().map(|(_, r)| r).sum::<u64>() + s.ecosystem_remainder, 1000);

        // C5 is the LARGEST single category — the point of the rebalance.
        let max_other = (0..4).map(|i| r.weight_bps[i]).max().unwrap();
        assert!(r.weight_bps[Category::Llm.index()] > max_other, "LLM mining must outweigh every other category");
    }

    /// ADR-0040 §16″ — **C5 is the largest category, therefore the largest sybil target.**
    ///
    /// The settle-loop bug proved that a weight without a path is not an allocation. The converse is
    /// what this pins: **a path without defences is not an allocation, it is a faucet.** Testnet points
    /// are a futures claim on TGE value, so an auto-award pipe opened before dedup / k=2 / per-credential
    /// caps exist would be a nearly free sybil harvest — and at 30 % it is the most valuable one on
    /// offer.
    ///
    /// Manual award is the correct initial state. This test exists so that flipping to auto-award is a
    /// deliberate, reviewable act rather than the quiet appearance of a collector.
    #[test]
    fn c5_auto_award_stays_closed_until_its_preconditions_are_met() {
        use crate::rules::{C5_AUTO_AWARD_PRECONDITIONS, c5_is_provisional, c5_points_collection_enabled, c5_token_settlement_enabled};

        assert!(c5_points_collection_enabled(), "measuring verified work is on — the chain evidence is perishable");
        assert!(!c5_token_settlement_enabled(), "C5 must not settle tokens while the gates below are open");
        assert!(c5_is_provisional(), "points earned under stub gates are calibration artefacts, not entitlements");
        assert!(
            C5_AUTO_AWARD_PRECONDITIONS.len() >= 4,
            "each precondition is a defence that must exist BEFORE the pipe opens — do not shorten this list \
             without closing the corresponding gate"
        );
        for pre in C5_AUTO_AWARD_PRECONDITIONS {
            assert!(!pre.is_empty());
        }

        // C5 is the largest pool, which is exactly why it is the one held back the longest.
        let r = AllocationRules::default();
        let c5 = r.weight_bps[Category::Llm.index()];
        assert!((0..4).all(|i| r.weight_bps[i] <= c5), "C5 is the largest category ⇒ the largest farming target");
    }

    #[test]
    fn per_id_cap_clips_and_redistributes_once() {
        let r = AllocationRules::default(); // cap 5% of 1000 = 50
        // A holds 90% of Node points, B 10%. Node pool 250 → base A=225, B=25.
        // A total 225 > cap 50 → clipped to 50, excess 175 → to uncapped B (only one).
        // B: 25 + 175 = 200 (exceeds cap — the "once" rule does not re-cap).
        let s = settle(1000, &r, &ids(&[("a", [90, 0, 0, 0, 0]), ("b", [10, 0, 0, 0, 0])]));
        assert_eq!(s.rewards, vec![("a".into(), 50), ("b".into(), 200)]);
        assert_eq!(s.rewards.iter().map(|(_, r)| r).sum::<u64>() + s.ecosystem_remainder, 1000);
    }

    #[test]
    fn deterministic_regardless_of_input_order() {
        let r = AllocationRules::default();
        let s1 = settle(1_000_000, &r, &ids(&[("z", [5, 2, 0, 1, 0]), ("a", [3, 0, 4, 0, 0])]));
        let s2 = settle(1_000_000, &r, &ids(&[("a", [3, 0, 4, 0, 0]), ("z", [5, 2, 0, 1, 0])]));
        assert_eq!(s1, s2);
        assert_eq!(s1.rewards[0].0, "a", "output is sorted by identity");
    }

    #[test]
    fn vesting_only_above_threshold() {
        let r = AllocationRules::default(); // threshold 0.1% of pool
        let pool = 100_000_000u64; // threshold = 100_000
        assert_eq!(vesting_split(50_000, pool, &r), (50_000, 0), "below threshold = TGE lump");
        // above: 25% TGE, 75% linear.
        assert_eq!(vesting_split(200_000, pool, &r), (50_000, 150_000));
    }
}
