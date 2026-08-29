//! Frozen scoring rules (ADR-0027 §3, §6.2, §11-A). Every value here is part of
//! the per-epoch `rules_hash` so a ledger is reproducible from published rules.
//!
//! All arithmetic in the crate is **integer** (points are carried in
//! milli-points = points × 1000, multipliers as exact `(num, den)` rationals)
//! so scoring is bit-reproducible on every platform — no floating point.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Points are carried internally as milli-points (1 point = 1000 milli-points).
pub type MilliPoints = u64;
/// 1 point in milli-points.
pub const POINT: MilliPoints = 1000;

/// Rules schema version — bump on any change to the values below.
///
/// v2: the BPS-escalation ladder was retired. See [`Stage`].
/// v3: the scored public network moved from retired testnet-10 to testnet-200.
/// v4: [`Rules`] split into [`ScoringRules`] (hashed into every ledger) and
///     [`AllocationRules`] (NOT hashed — see the split's rationale below), and C5
///     gained an automatic per-accepted-replica rule. Not applied retroactively:
///     ledgers published under v3 keep their own `rules_hash` and their own numbers.
pub const RULES_VERSION: u16 = 4;

/// BPS stage coefficient (ADR-0027 §3).
///
/// **The ladder is retired.** ADR-0027 scored a planned BPS escalation
/// (testnet-10 → 25 → 40 → 50) with a rising coefficient, because a higher block rate is a
/// harsher test and was worth more points. The block rate is now **fixed at 10 BPS**, split
/// 2 (hash lane) + 8 (PALW replica lane) — see `LaneDifficultyParams::INERT`
/// (`hash_target_time_ms: 500` / `replica_target_time_ms: 125`). There is no second rung to
/// climb, so `B` (×1.25) and `C` (×1.5) are gone rather than left as unreachable variants that
/// imply a ladder that is not coming.
///
/// This also removes three networks that never existed: `testnet-25`, `testnet-40` and
/// `testnet-50` had no consensus preset, and the one the PALW rationale did plan was named
/// `testnet-palw-40`, which did not even match the name the scorer scoped.
///
/// `A` is kept (rather than dropping the concept) so the ledger schema keeps an explicit,
/// signed coefficient field instead of an implicit ×1.0 — if a stressnet is ever revived it
/// re-enters here with a new variant and another `RULES_VERSION` bump.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Stage {
    /// The single scored public network (testnet-20 since the 2026-07-30 re-genesis; testnet-200
    /// before it), at the fixed 10 BPS (2 + 8). The scope list itself lives in the service's
    /// `config::NETWORKS` — the stage is the signed coefficient, not the network name.
    A,
}

impl Stage {
    /// The stage multiplier as `(num, den)`: A=1/1.
    pub const fn factor(self) -> (u64, u64) {
        match self {
            Stage::A => (1, 1),
        }
    }
}

/// Bug severity → base points (ADR-0027 §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Severity {
    /// consensus split, fund loss, PQ soundness, remote crash.
    S0,
    /// node crash/DoS, EVM state divergence, overlay finality break.
    S1,
    /// sync edge case, RPC inconsistency, resource leak.
    S2,
    /// minor bug, docs, UX.
    S3,
}

impl Severity {
    /// Base points for a first, accepted report.
    pub const fn base_points(self) -> u64 {
        match self {
            Severity::S0 => 5_000,
            Severity::S1 => 2_000,
            Severity::S2 => 500,
            Severity::S3 => 100,
        }
    }
}

/// The scoring categories (ADR-0027 §3, §6.2; **C5 added by ADR-0040 §16″**). Order is canonical and
/// APPEND-ONLY — a new category must be added at the end, because [`Self::index`] is the ledger's
/// column order and reordering would silently re-attribute historical points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Category {
    /// C1 node operation.
    Node,
    /// C2 bug reports.
    Bug,
    /// C3 verification / feedback.
    Verify,
    /// C4 infrastructure.
    Infra,
    /// **C5 LLM mining (PALW proof-of-LLM provider work)** — ADR-0040 §16″.
    ///
    /// Distinct from C1 `Node`: C1 rewards running a node (uptime, validator attestation, IBD benches,
    /// drills), which any VPS can do. C5 rewards contributing *inference* — running the canonical
    /// runtime as an A/B replica or as an auditor — which requires the GPU capacity the compute lane
    /// actually needs. Folding it into C1 would price a 4090 the same as a $5 VPS and buy no supply.
    ///
    /// # C5 records points automatically, but settles NO tokens
    ///
    /// The original objection to an automatic C5 was that *a path with no defence is not an
    /// allocation, it is a faucet* — testnet points are a futures claim on TGE value, so an open pipe
    /// on a stub network is a nearly free sybil harvest.
    ///
    /// That objection is about **token entitlement**, not about **measurement**, and the two are now
    /// separate gates ([`c5_points_collection_enabled`] / [`c5_token_settlement_enabled`]). Recording
    /// which verified k=2 job each identity actually completed is a measurement, and it is only
    /// possible while the evidence is still on the selected chain. How many MSK C5 is worth — and
    /// whether the ratio survives at all — stays undecided in [`AllocationRules`], which is
    /// deliberately NOT part of `rules_hash`, so publishing a point total promises no share.
    Llm,
}

/// ADR-0040 §16″ — what must hold before C5 may be auto-awarded from chain facts.
///
/// Each line is a defence that has to exist BEFORE leaf/receipt data may mint points, because once the
/// pipe is open the incentive to farm it is immediate and the points are already claimed.
pub const C5_AUTO_AWARD_PRECONDITIONS: &[&str] = &[
    // Without dedup, one computation can be presented repeatedly for points — the MTP-side twin of the
    // consensus P1-9 job-nullifier gap.
    "global job-nullifier dedup (ADR-0040 P1-9) enforced on the awarding path",
    // Unmatched work is unverified work; paying for it prices a claim rather than a computation.
    "k=2 replica exact-match passed (only matched work is creditable)",
    // Without a per-credential ceiling, a sybil fleet converts credential count directly into points.
    "per-credential epoch cap on C5 points",
    // While selection is unweighted (SEL-01) and tickets are re-mintable (AUTH-02), any C5 total is
    // provisional and must be marked as such rather than settled.
    "SEL-01 (bond-weighted selection) and AUTH-02 (block authorization) closed",
];

/// **Gate 1 of 2 — may C5 points be COLLECTED from chain facts?**
///
/// `true`. Collecting is a measurement of work that already happened and is provable from the
/// selected chain: an accepted PALW leaf whose k=2 replica pair exact-matched, deduplicated by
/// execution nullifier, attributed through the provider bond's owner to a registered MTP id.
///
/// The [`C5_AUTO_AWARD_PRECONDITIONS`] that are still open all bound what a point is *worth*, not
/// whether the work occurred — so they gate [`c5_token_settlement_enabled`], not this. Waiting for
/// them before recording anything would lose the evidence instead of protecting it: leaves are
/// pruned, and no later fix reconstructs who did which job.
pub const fn c5_points_collection_enabled() -> bool {
    true
}

/// **Gate 2 of 2 — may C5 points be converted into TOKENS?**
///
/// `false`, and it stays false until every [`C5_AUTO_AWARD_PRECONDITIONS`] line closes. This is the
/// gate the faucet objection is actually about: a point is a ratio inside C5, and a ratio becomes a
/// claim only when a pool is fixed against it. Until then C5 has a share of nothing, which is why
/// [`settle`](crate::settle::settle) must not be called with a C5 pool.
pub const fn c5_token_settlement_enabled() -> bool {
    false
}

/// ADR-0040 §16″ — C5 points earned while the stub gates are open are **provisional**.
///
/// The same lineage as the Q4_K_M receipts and the bond-0 / simple-beacon testnet period: calibration
/// artefacts, not settled entitlements. Recording the status alongside the points is what allows them
/// to be discounted later without arguing about what was promised.
pub const fn c5_is_provisional() -> bool {
    !c5_token_settlement_enabled()
}

impl Category {
    /// All categories in canonical order (C1..C5).
    pub const ALL: [Category; 5] = [Category::Node, Category::Bug, Category::Verify, Category::Infra, Category::Llm];
    /// Canonical index 0..5 (C1..C5). **Append-only**: these indices are the ledger's column order.
    pub const fn index(self) -> usize {
        match self {
            Category::Node => 0,
            Category::Bug => 1,
            Category::Verify => 2,
            Category::Infra => 3,
            Category::Llm => 4,
        }
    }
}

/// **What each identity earns.** Hashed into every ledger as `rules_hash`.
///
/// # Why this is separate from [`AllocationRules`]
///
/// These two answer different questions, and only one of them is decidable today:
///
/// | | question | decided |
/// |---|---|---|
/// | [`ScoringRules`] | who earns how many points | **now** — it is a rule about work |
/// | [`AllocationRules`] | how many tokens the points are worth | later — it is a rule about supply |
///
/// While both lived in one struct, `weight_bps: [.., 3000]` was inside `rules_hash`, so every
/// published ledger carried a signed "C5 gets 30 %". Nothing *used* it for scoring (`score_epoch`
/// never reads a weight), but a signed number is read as a promise, and people preserve numbers they
/// find as entitlements. Splitting the struct makes the honest state expressible: the point rules are
/// frozen and signed; the distribution is not written down anywhere a participant can mistake for a
/// commitment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ScoringRules {
    pub version: u16,
    // --- C1 base points ---
    pub node_uptime_base: u64, // 100 · u · m_geo · m_ver · d_n
    pub validator_base: u64,   // 200 · a
    pub ibd_bench_points: u64, // 50 / submission
    pub drill_points: u64,     // 100 / event
    // --- C1 multipliers as (num, den) ---
    pub m_geo_num: u64,
    pub m_geo_den: u64,
    pub m_ver_num: u64,
    pub m_ver_den: u64,
    /// Per-ID node decrement d_n by rank (1st, 2nd, 3rd, 4th+), each as (num, den).
    pub d_n: [(u64, u64); 4],
    // --- C2 duplicate factor (num, den) ---
    pub bug_dup_num: u64,
    pub bug_dup_den: u64,
    // --- C5 ---
    /// Points for ONE accepted, k=2-matched replica slot (ADR-0040 §16″).
    ///
    /// Flat on purpose. A 4090 and a Mac earn the same for the same verified job; faster hardware
    /// earns more by completing more jobs, which is already the reward. Adding a hardware, power, or
    /// bond multiplier on top would count capability twice — fairness here is *equal pay for equal
    /// verified work*, not equal totals.
    ///
    /// Not yet proportional to `canonical_compute_units`: A and B agreeing on a CU value proves they
    /// ran the same thing, not that the value is honest, and `ReceiptV3Expectations` does not yet
    /// carry a node-side expected CU to check it against. The CU is recorded as evidence so the
    /// switch to CU-proportional scoring is a later `RULES_VERSION` bump, never a re-scoring of
    /// history.
    pub c5_points_per_accepted_replica: u64,
}

/// **What the points are worth.** Deliberately NOT hashed into any ledger, and not read by any
/// scoring path — see [`ScoringRules`] for why the split exists.
///
/// Every field here is provisional until a distribution is actually decided. `settle` is the only
/// consumer, and it must not run for C5 while [`c5_token_settlement_enabled`] is `false`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AllocationRules {
    /// Category weights in basis points (sum 10000), C1..C5.
    pub weight_bps: [u16; 5],
    pub per_id_cap_bps: u16, // 500 = 5%
    /// Vesting threshold as bps of the pool (10 = 0.1%); above it, cliff+linear.
    pub vesting_threshold_bps: u16,
    pub vesting_cliff_bps: u16, // 2500 = 25% at TGE
}

/// The scoring rules, under the name the pre-v4 code used.
///
/// Kept so the ~60 `&Rules` scoring call sites did not all churn in the split commit. New code should
/// say [`ScoringRules`] when it means scoring — which, since allocation moved out, is everywhere the
/// name still appears.
pub type Rules = ScoringRules;

impl Default for ScoringRules {
    /// The ADR-0027 v1 defaults (§3, §6.2), plus the ADR-0040 §16″ C5 rule.
    fn default() -> Self {
        ScoringRules {
            version: RULES_VERSION,
            node_uptime_base: 100,
            validator_base: 200,
            ibd_bench_points: 50,
            drill_points: 100,
            m_geo_num: 3,
            m_geo_den: 2, // 1.5
            m_ver_num: 6,
            m_ver_den: 5, // 1.2
            d_n: [(1, 1), (1, 2), (1, 4), (0, 1)],
            bug_dup_num: 1,
            bug_dup_den: 10, // duplicates score 10%
            c5_points_per_accepted_replica: 1,
        }
    }
}

impl Default for AllocationRules {
    /// Placeholder numbers ONLY. Nothing signs these and nothing is owed under them; they exist so
    /// `settle` has something to exercise in tests before a distribution is decided.
    fn default() -> Self {
        AllocationRules {
            //                 C1 Node  C2 Bug  C3 Verify  C4 Infra  C5 LLM
            weight_bps: [2500, 2500, 1000, 1000, 3000],
            per_id_cap_bps: 500,       // 5%
            vesting_threshold_bps: 10, // 0.1%
            vesting_cliff_bps: 2500,   // 25% TGE, 75% linear/6mo
        }
    }
}

impl ScoringRules {
    /// `Hash64_k("misaka-mtp-v1/rules", borsh(self))` — the value pinned in each
    /// epoch ledger. Anyone with the same `ScoringRules` recomputes the same hash.
    pub fn rules_hash(&self) -> kaspa_hashes::Hash64 {
        let bytes = borsh::to_vec(self).expect("borsh of in-memory ScoringRules is infallible");
        kaspa_hashes::blake2b_512_keyed(crate::MTP_RULES_CONTEXT, &bytes)
    }
}

impl AllocationRules {
    /// Canonical `weight_bps` must sum to 10000 (guards a malformed rule set).
    pub fn weights_sum_to_full(&self) -> bool {
        self.weight_bps.iter().map(|&w| w as u32).sum::<u32>() == 10_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rules_are_well_formed() {
        let r = ScoringRules::default();
        let a = AllocationRules::default();
        assert!(a.weights_sum_to_full(), "category weights must sum to 100%");
        assert_eq!(a.per_id_cap_bps, 500);
        // rules_hash is deterministic + non-trivial.
        assert_eq!(r.rules_hash(), ScoringRules::default().rules_hash());
        assert_ne!(r.rules_hash().as_bytes(), [0u8; 64]);
    }

    #[test]
    fn a_rule_change_changes_the_hash() {
        let mut r = ScoringRules::default();
        let h0 = r.rules_hash();
        r.node_uptime_base = 101;
        assert_ne!(h0, r.rules_hash(), "any rule edit must change rules_hash");
    }

    /// The whole point of the v4 split: a ledger must not carry a signed distribution. If an
    /// allocation knob ever leaks back into `ScoringRules`, changing it would move `rules_hash`
    /// and this fails.
    #[test]
    fn allocation_is_not_part_of_the_signed_rules_hash() {
        let baseline = ScoringRules::default().rules_hash();
        for weights in [[10_000u16, 0, 0, 0, 0], [0, 0, 0, 0, 10_000], [2000, 2000, 2000, 2000, 2000]] {
            let alloc = AllocationRules { weight_bps: weights, ..AllocationRules::default() };
            assert!(alloc.weights_sum_to_full());
            assert_eq!(
                ScoringRules::default().rules_hash(),
                baseline,
                "no allocation choice — including C5's share — may move the signed rules_hash"
            );
        }
    }

    /// Collection is open, settlement is shut: points accrue, tokens do not.
    #[test]
    fn the_two_c5_gates_are_independent_and_settlement_stays_shut() {
        assert!(c5_points_collection_enabled(), "C5 measurement is on — the evidence is perishable");
        assert!(!c5_token_settlement_enabled(), "C5 must not settle tokens while the preconditions are open");
        assert!(c5_is_provisional(), "points recorded under an open settlement gate are provisional");
        assert!(!C5_AUTO_AWARD_PRECONDITIONS.is_empty(), "the settlement gate must state what would close it");
    }

    /// C5 pays per verified slot, and pays every provider the same for it.
    #[test]
    fn c5_rule_is_flat_per_accepted_replica() {
        assert_eq!(ScoringRules::default().c5_points_per_accepted_replica, 1);
    }
}
