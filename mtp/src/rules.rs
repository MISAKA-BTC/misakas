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
pub const RULES_VERSION: u16 = 2;

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
    /// testnet-10 — the only scored network, at the fixed 10 BPS (2 + 8).
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
    /// # C5 is MANUAL-AWARD ONLY, deliberately, until [`C5_AUTO_AWARD_PRECONDITIONS`] are met
    ///
    /// The settle-loop bug caught during this change proved one direction — *a weight with no path is
    /// not an allocation*. The converse is equally true and more dangerous: **a path with no defence is
    /// not an allocation, it is a faucet.** Testnet points are a futures claim on TGE value, so farming
    /// them on a stub network is a nearly free sybil harvest, and 30 % makes C5 the largest single
    /// target in the program.
    ///
    /// Manual award is therefore the correct *initial* state, not a limitation to be routed around.
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

/// ADR-0040 §16″ — is C5 eligible for automatic scoring on this network yet?
///
/// Hard-wired `false`: every precondition above is open. Kept as a function rather than a comment so
/// the awarding path has something to branch on, and so flipping it is a visible, reviewable diff
/// rather than the quiet appearance of a new collector.
pub const fn c5_auto_award_enabled() -> bool {
    false
}

/// ADR-0040 §16″ — C5 points earned while the stub gates are open are **provisional**.
///
/// The same lineage as the Q4_K_M receipts and the bond-0 / simple-beacon testnet period: calibration
/// artefacts, not settled entitlements. Recording the status alongside the points is what allows them
/// to be discounted later without arguing about what was promised.
pub const fn c5_is_provisional() -> bool {
    !c5_auto_award_enabled()
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

/// The frozen rule parameters that define a score. Hashed into every ledger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Rules {
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
    // --- category weights, basis points (sum 10000) ---
    pub weight_bps: [u16; 5],
    // --- settlement ---
    pub per_id_cap_bps: u16, // 500 = 5%
    /// Vesting threshold as bps of the pool (10 = 0.1%); above it, cliff+linear.
    pub vesting_threshold_bps: u16,
    pub vesting_cliff_bps: u16, // 2500 = 25% at TGE
}

impl Default for Rules {
    /// The ADR-0027 v1 defaults (§3, §6.2, §6.3).
    fn default() -> Self {
        Rules {
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
            // ADR-0040 §16″ — C5 LLM mining at 30 %.
            //
            // The rebalance takes the 30 points from C1 (25 → and C2/C3/C4 5 collectively), because the
            // program's purpose has shifted: the scarce contribution is no longer "run a node" (cheap,
            // already saturated) but "run the canonical inference runtime" (GPU-bound, and the compute
            // lane cannot start without it). C1 keeps the largest non-LLM share so validator/uptime work
            // is still worth doing.
            //
            //                 C1 Node  C2 Bug  C3 Verify  C4 Infra  C5 LLM
            weight_bps: [2500, 2500, 1000, 1000, 3000],
            per_id_cap_bps: 500,       // 5%
            vesting_threshold_bps: 10, // 0.1%
            vesting_cliff_bps: 2500,   // 25% TGE, 75% linear/6mo
        }
    }
}

impl Rules {
    /// `Hash64_k("misaka-mtp-v1/rules", borsh(self))` — the value pinned in each
    /// epoch ledger. Anyone with the same `Rules` recomputes the same hash.
    pub fn rules_hash(&self) -> kaspa_hashes::Hash64 {
        let bytes = borsh::to_vec(self).expect("borsh of in-memory Rules is infallible");
        kaspa_hashes::blake2b_512_keyed(crate::MTP_RULES_CONTEXT, &bytes)
    }

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
        let r = Rules::default();
        assert!(r.weights_sum_to_full(), "category weights must sum to 100%");
        assert_eq!(r.per_id_cap_bps, 500);
        // rules_hash is deterministic + non-trivial.
        assert_eq!(r.rules_hash(), Rules::default().rules_hash());
        assert_ne!(r.rules_hash().as_bytes(), [0u8; 64]);
    }

    #[test]
    fn a_rule_change_changes_the_hash() {
        let mut r = Rules::default();
        let h0 = r.rules_hash();
        r.node_uptime_base = 101;
        assert_ne!(h0, r.rules_hash(), "any rule edit must change rules_hash");
    }
}
