//! RFC-0007 explicit verifier-assumption report for one paid claim (PRINCIPLES §8.3, user decision 2026-10-15 §6).
//!
//! A drawn-verifier approval is authentication **under an explicit honest-fraction assumption**, never a cryptographic proof
//! that the model computation is correct. This report states, from the claim's own native authority (the frozen checker
//! snapshot and the drawn lock), everything that assumption rests on:
//! - the drawn seats `m` and positive quorum `k`, and the candidate population they were drawn from;
//! - correlation evidence that IS on chain: distinct registered operators and the largest single operator's weight share
//!   (a registered operator ID is an identity, not evidence of independence: several operators may be one party);
//! - the false-approval probability at named adversarial weight fractions (1/3, 0.6, the largest single operator's share,
//!   and 1 — an all-adversary pool approves anything), from the binomial tail over drawn seats;
//! - what is NOT known: the honest fraction itself, actual participation, material-acquisition success and the time each
//!   verifier needs. Those stay `None` (UNKNOWN) until measured or assumed by the user; nothing here defaults them.
//! - the deadlines the approval must meet (receipt window), and that a direct check's conditional error (e.g. Freivalds per
//!   repetition) covers only the relation and inputs it read, never the whole model.
//!
//! Reporting only: no consensus rule, receipt, credit or Final reads this. Consensus credit is RFC-0018's Gate C alone.
use super::VerifiedPaidClaimSourcesV2;
use std::collections::BTreeMap;

/// Named adversarial weight fractions every report evaluates (permille). The largest-operator share is added per claim.
pub const PAID_CLAIM_ASSUMPTION_SCENARIOS_PERMILLE_V1: [u32; 3] = [333, 600, 1000];

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaidClaimAssumptionReportV1 {
    pub version: u16,
    pub statement: &'static str,
    pub drawn_seats: u32,
    pub positive_quorum: u32,
    pub candidate_population: u32,
    pub distinct_registered_operators: u32,
    /// The largest single registered operator's share of the candidate weight (correlation that is visible on chain).
    pub largest_operator_weight_permille: u32,
    /// `(adversarial weight fraction permille, probability that >= quorum drawn seats are adversarial, ppm)`. Binomial
    /// approximation of weighted drawing with replacement; reporting only.
    pub false_approval_ppm_at: Vec<(u32, u64)>,
    /// UNKNOWN until the user states it: the chain cannot prove which stake is honest.
    pub assumed_honest_weight_permille: Option<u32>,
    /// UNKNOWN until measured on real nodes: the share of drawn seats that actually attest in time.
    pub measured_participation_permille: Option<u32>,
    /// UNKNOWN until measured: the share of drawn seats that obtained the complete material from a provider.
    pub measured_material_acquisition_permille: Option<u32>,
    pub receipt_window_daa: (u64, u64),
    pub challenge_until_daa: u64,
    pub direct_check_scope: &'static str,
}

fn binomial_tail_ppm(m: u32, f_permille: u32, at_least: u32) -> u64 {
    let p = f64::from(f_permille.min(1000)) / 1000.0;
    let mut total = 0.0f64;
    for x in at_least..=m {
        let mut c = 1.0f64;
        for i in 0..x {
            c = c * f64::from(m - i) / f64::from(i + 1);
        }
        total += c * p.powi(x as i32) * (1.0 - p).powi((m - x) as i32);
    }
    (total.clamp(0.0, 1.0) * 1_000_000.0).round() as u64
}

impl VerifiedPaidClaimSourcesV2 {
    /// The explicit assumption report of this claim's drawn approval, from its own authenticated authority and lock.
    pub fn assumption_report_v1(&self) -> PaidClaimAssumptionReportV1 {
        let authority = self.authority();
        let lock = self.lock();
        let candidates = &authority.checker_candidates;
        let total_weight: u128 = candidates.iter().map(|c| u128::from(c.weight)).sum();
        let mut by_operator: BTreeMap<_, u128> = BTreeMap::new();
        for c in candidates {
            *by_operator.entry(c.operator).or_default() += u128::from(c.weight);
        }
        let largest = by_operator.values().copied().max().unwrap_or(0);
        let largest_permille = if total_weight == 0 { 0 } else { (largest * 1000 / total_weight) as u32 };
        let m = lock.seats.len() as u32;
        let k = lock.quorum;
        let mut scenarios: Vec<u32> = PAID_CLAIM_ASSUMPTION_SCENARIOS_PERMILLE_V1.to_vec();
        scenarios.push(largest_permille);
        scenarios.sort_unstable();
        scenarios.dedup();
        let lifetime = &authority.enrollment.lifetime;
        PaidClaimAssumptionReportV1 {
            version: 1,
            statement: "drawn-verifier approval under an explicit honest-fraction ASSUMPTION; not a cryptographic proof of computation \
                        (PRINCIPLES §8). Optimistic Final or a matching local replay never substitutes for it.",
            drawn_seats: m,
            positive_quorum: k,
            candidate_population: candidates.len() as u32,
            distinct_registered_operators: by_operator.len() as u32,
            largest_operator_weight_permille: largest_permille,
            false_approval_ppm_at: scenarios.into_iter().map(|f| (f, binomial_tail_ppm(m, f, k))).collect(),
            assumed_honest_weight_permille: None,
            measured_participation_permille: None,
            measured_material_acquisition_permille: None,
            receipt_window_daa: (lifetime.committed_daa, lifetime.checking_until),
            challenge_until_daa: lifetime.challenge_until,
            direct_check_scope: "a direct check (exact recomputation, Freivalds/linear evaluation) covers only the relation and the \
                                 authenticated inputs it read; its per-repetition error is conditional on those inputs and is never \
                                 extended to the whole model",
        }
    }
}

/// RFC-0007 R7-ECON: the verifier/producer books of one paid claim with every unmeasured quantity left UNKNOWN (`None`).
/// The chain supplies only the producer's currently collectible collateral (an upper bound on what a conviction can take at
/// this tip, before competing liabilities); check, fetch, filing, idle, first-acquisition and capital costs, the per-check
/// fee and the lazy-verifier deterrent are measured or decided elsewhere and never defaulted here.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaidClaimEconomicsReportV1 {
    pub version: u16,
    pub producer_collateral_sompi: Option<u64>,
    pub producer_reserved_sompi: Option<u64>,
    /// `collateral - reserved` at this tip: what is physically collectible now, not a promise for the liability horizon.
    pub producer_collectible_now_sompi: Option<u64>,
    pub liability_until_daa: u64,
    pub check_cost_sompi: Option<u64>,
    pub material_fetch_cost_sompi: Option<u64>,
    pub filing_cost_sompi: Option<u64>,
    pub idle_cost_per_daa_sompi: Option<u64>,
    pub first_acquisition_cost_sompi: Option<u64>,
    pub capital_rate_ppm_per_period: Option<u64>,
    pub unpaid_cost_sompi: Option<u64>,
    pub per_check_fee_sompi: Option<u64>,
    /// One deposit must not back many attempts: the per-attempt source deposit and its concurrency cap are named policy
    /// values (d_src, N_max) - USER DECISION; reported here as UNKNOWN until fixed.
    pub per_attempt_source_deposit_sompi: Option<u64>,
    pub attempt_concurrency_cap: Option<u32>,
    pub lazy_verifier_deterrent: &'static str,
}

impl VerifiedPaidClaimSourcesV2 {
    /// The economic books of this claim: on-chain collectible collateral, everything else UNKNOWN until measured/decided.
    pub fn economics_report_v1(&self, ledger: &misaka_palw_kernel::ledger::KernelLedgerV1) -> PaidClaimEconomicsReportV1 {
        let authority = self.authority();
        let bond = ledger.bonds.get(&authority.enrollment.producer);
        PaidClaimEconomicsReportV1 {
            version: 1,
            producer_collateral_sompi: bond.map(|b| b.collateral),
            producer_reserved_sompi: bond.map(|b| b.reserved),
            producer_collectible_now_sompi: bond.map(|b| b.free()),
            liability_until_daa: authority.enrollment.lifetime.liability_until,
            check_cost_sompi: None,
            material_fetch_cost_sompi: None,
            filing_cost_sompi: None,
            idle_cost_per_daa_sompi: None,
            first_acquisition_cost_sompi: None,
            capital_rate_ppm_per_period: None,
            unpaid_cost_sompi: None,
            per_check_fee_sompi: None,
            per_attempt_source_deposit_sompi: None,
            attempt_concurrency_cap: None,
            lazy_verifier_deterrent: "UNKNOWN: an attestation alone does not deter a lazy verifier (T6 open); only a public conviction of a wrong approval does, under the same honest-fraction assumption",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_false_approval_tail_is_monotone_and_saturates_at_an_all_adversary_pool() {
        let at = |f| binomial_tail_ppm(47, f, 24);
        assert!(at(30) < 10, "3 % adversarial weight: practically never");
        assert!(at(333) < at(600));
        assert_eq!(at(1000), 1_000_000, "an all-adversary pool approves anything: the residual assumption, stated");
        assert_eq!(at(0), 0);
    }
}
