//! **Lane F03k2: the Panel=1 kernel-route class's censorship-cost relation** (part of the dormant `palw_kernel_route_panel_one_v1`).
//!
//! An OptimisticPublicVerification class registers only if saturating the court budget for the claim's whole exposure costs more
//! than the claim's maximum gain (`misaka_palw_kernel::opv::OpvPolicyV1::censorship_cost`). Before this, a Panel-licensed class had no
//! such check. Under Panel=1 the producer and the one seat may collude, so the only thing between a lie and its payment is an
//! outsider's `FileProof`. Colluding Sybil bonds could keep that filing out of the chain by filling the court of EVERY block in the
//! claim's exposure with dismissed junk. Past the fence a Panel class therefore registers only where
//!
//! `runs_per_block × dismissal_fee(worst court work) × (check window + challenge window + liability) > claim_reward + external gain`.
//!
//! * `runs_per_block` = `min(max_adjudications_per_block, ⌈max_court_work_per_block / worst⌉)`: how many filings must be dismissed to
//!   spend one block's court (a `FileProof` may spend the whole budget, the proof reserve included), as in OPV.
//! * The fee is what the ledger actually charges a dismissed filing of the class's worst work (`LedgerPolicyV1::dismissal_fee_v1`).
//! * Exposure counts one block per DAA, as in OPV.
//! * The external gain bound (what a lie can be worth outside the chain's own reward) has no adopted value:
//!   [`crate::palw_g14_values_v1::PALW_G14_KERNEL_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI_V1`] is `None`. While it is `None`, every Panel
//!   class past the fence is REFUSED as typed-missing. It is never treated as zero.
//!
//! Below the fence nothing here runs (byte-identical). This is an economic relation over hashes-and-fees only. It is no proof of
//! computation (PRINCIPLES §8) and no per-model review (§9): it bounds a class's court geometry, the same rule for every tier.

use misaka_palw_kernel::ledger::LedgerPolicyV1;

/// Why a Panel=1 class registration is refused by the relation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelOneCensorshipRefusalV1 {
    /// No external gain bound has been adopted (typed-missing, not zero).
    ExternalGainBoundMissing,
    /// Saturating the court for the claim's whole exposure costs no more than the claim's maximum gain.
    CensorshipTooCheap { cost: u128, gain: u128 },
}

/// The cost of saturating every block's court for a claim's exposure (see the module doc).
pub fn panel_one_censorship_cost_v1(policy: &LedgerPolicyV1, worst_court_work: u64) -> u128 {
    let by_work = (policy.max_court_work_per_block as u128).div_ceil(worst_court_work.max(1) as u128);
    let runs = (policy.max_adjudications_per_block as u128).min(by_work).max(1);
    let exposure =
        policy.check_window_daa as u128 + policy.challenge_window_daa as u128 + policy.liability_daa as u128;
    runs.saturating_mul(policy.dismissal_fee_v1(worst_court_work) as u128).saturating_mul(exposure)
}

/// **The relation**: `Ok` iff the external gain bound is adopted and the cost exceeds `claim_reward + external gain bound`.
pub fn panel_one_censorship_check_v1(
    policy: &LedgerPolicyV1,
    worst_court_work: u64,
    external_gain_bound: Option<u64>,
) -> Result<(), PanelOneCensorshipRefusalV1> {
    let external = external_gain_bound.ok_or(PanelOneCensorshipRefusalV1::ExternalGainBoundMissing)?;
    let cost = panel_one_censorship_cost_v1(policy, worst_court_work);
    let gain = policy.claim_reward as u128 + external as u128;
    if cost <= gain {
        return Err(PanelOneCensorshipRefusalV1::CensorshipTooCheap { cost, gain });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> LedgerPolicyV1 {
        let mut p = crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Default::default(), Default::default());
        p.max_adjudications_per_block = 4;
        p.max_court_work_per_block = 1 << 20;
        p.dismissed_proof_fee = 10;
        p.check_window_daa = 10;
        p.challenge_window_daa = 100;
        p.liability_daa = 890;
        p.claim_reward = 1_000;
        p
    }

    #[test]
    fn f03k2_cost_counts_every_block_of_the_exposure() {
        // Worst work = the whole block: one run per block, fee scaled to 4 shares, 1,000 DAA of exposure.
        assert_eq!(panel_one_censorship_cost_v1(&policy(), 1 << 20), 4 * 10 * 1_000);
        // A quarter-block worst: four runs, one share each.
        assert_eq!(panel_one_censorship_cost_v1(&policy(), 1 << 18), 4 * 10 * 1_000);
    }

    #[test]
    fn f03k2_missing_bound_is_refused_typed_not_zero() {
        assert_eq!(panel_one_censorship_check_v1(&policy(), 1 << 20, None), Err(PanelOneCensorshipRefusalV1::ExternalGainBoundMissing));
    }

    #[test]
    fn f03k2_cheap_censorship_is_refused_and_dear_censorship_passes() {
        let p = policy();
        assert_eq!(panel_one_censorship_check_v1(&p, 1 << 20, Some(1_000)), Ok(()));
        assert_eq!(
            panel_one_censorship_check_v1(&p, 1 << 20, Some(39_000)),
            Err(PanelOneCensorshipRefusalV1::CensorshipTooCheap { cost: 40_000, gain: 40_000 })
        );
    }
}
