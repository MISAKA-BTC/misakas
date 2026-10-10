//! **ECON's M\*-49 verifier pay and the held default share — `palw_verifier_pay_v1`** (dormant, no height; readiness §3f, the user's
//! rulings of 2026-10-10; G14R round 3).
//!
//! Past this fence the kernel route's ledger (`misaka_palw_kernel::verifier_pay`) escrows a per-check fee beside every job's reward
//! escrow, pays drawn verifiers on their attestation or on the claim's fate (first out of the unchanged 49 % bounty, up to `B_cap`),
//! exempts their served demand bonds from the horizon burn, and holds a pre-Final default's demanders' share to the liability horizon
//! (ECON's O2), so the one 49 % pool pays an honest accuser after a self-inflicted default as if no default had come first.
//!
//! **Dormant**: `None` on every preset; hashed Some-only into `consensus_params_id` and `consensus_schedule_id`, collapsed from
//! `Some(never())`, visited by `for_each_fence`, refused when armed ([`Params::validate_palw_verifier_pay_v1`]). It needs the kernel
//! route and OPV at or below it. The terms below are INTERIM and unapproved (POLICY); the draw is OPVB's v3 `ClaimVerification`
//! beacon and the attestation an object not yet allocated, so the node injects the terms but no draw yet.

use crate::config::params::{ForkActivation, Params};
use crate::constants::SOMPI_PER_KASPA;
use crate::palw_mode_v2::PalwModeV2Error;

/// INTERIM `F_min` (POLICY, ECON §5e at 1 BILI per CPU-hour): the per-check cost floor, 0.86 BILI.
pub const PALW_VERIFIER_PAY_F_MIN_SOMPI_V1: u64 = 86 * SOMPI_PER_KASPA / 100;
/// INTERIM `r_w` (POLICY): the watcher stake's capital cost per epoch, 100 ppm.
pub const PALW_VERIFIER_PAY_R_W_PPM_V1: u64 = 100;
/// INTERIM `S_pool` (POLICY): the watcher pool's stake, 10,000 BILI.
pub const PALW_VERIFIER_PAY_S_POOL_SOMPI_V1: u64 = 10_000 * SOMPI_PER_KASPA;
/// INTERIM `q` (POLICY): the checked share of claims, 50 %.
pub const PALW_VERIFIER_PAY_Q_PPM_V1: u64 = 500_000;
/// INTERIM `N` (POLICY): claims per epoch.
pub const PALW_VERIFIER_PAY_CLAIMS_PER_EPOCH_V1: u64 = 1;
/// INTERIM `F` (C9): `F_min + ⌈r_w · S_pool / (q·m·N)⌉` = 0.86 + 0.50 = 1.36 BILI per drawn check.
pub const PALW_VERIFIER_PAY_CHECK_FEE_SOMPI_V1: u64 = 136 * SOMPI_PER_KASPA / 100;
/// INTERIM `d_src = d*` (ECON §4.4, S3): the beacon-source seal's deposit, 28.6 BILI.
pub const PALW_VERIFIER_PAY_SOURCE_DEPOSIT_SOMPI_V1: u64 = 286 * SOMPI_PER_KASPA / 10;
/// INTERIM `N_max` (ECON §4.4, S4): open attempts one class's beacon-source seals may feed at once.
pub const PALW_VERIFIER_PAY_MAX_OPEN_ATTEMPTS_PER_CLASS_V1: u8 = 2;
/// INTERIM `m` (POLICY): drawn slots per checked claim.
pub const PALW_VERIFIER_PAY_SLOTS_V1: u8 = 4;
/// INTERIM `B_cap = m·G` (ECON T5, `G` = the OPV terms' external gain bound, 10 BILI): 40 BILI of the bounty to drawn sealers first.
pub const PALW_VERIFIER_PAY_BOUNTY_CAP_SOMPI_V1: u64 = 40 * SOMPI_PER_KASPA;

/// The kernel ledger's verifier-pay terms at the fence's height.
pub fn palw_verifier_pay_interim_policy_v1(activation_daa: u64) -> misaka_palw_kernel::verifier_pay::VerifierPayPolicyV1 {
    misaka_palw_kernel::verifier_pay::VerifierPayPolicyV1 {
        activation_daa,
        check_fee: PALW_VERIFIER_PAY_CHECK_FEE_SOMPI_V1,
        slots: PALW_VERIFIER_PAY_SLOTS_V1,
        bounty_cap: PALW_VERIFIER_PAY_BOUNTY_CAP_SOMPI_V1,
        source_deposit: PALW_VERIFIER_PAY_SOURCE_DEPOSIT_SOMPI_V1,
        max_open_attempts_per_class: PALW_VERIFIER_PAY_MAX_OPEN_ATTEMPTS_PER_CLASS_V1,
    }
}

impl Params {
    /// The fence's activation, where the network declares one (`None`: absent or `never()`).
    pub fn palw_verifier_pay_activation(&self) -> Option<u64> {
        self.palw_verifier_pay_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// **The fence's refusal**: prerequisites by name first (the kernel route and OPV at or below it), then the blanket refusal — M\*-49
    /// is armed only by the single full-activation release.
    pub fn validate_palw_verifier_pay_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_verifier_pay_activation() else { return Ok(()) };
        let route = self.palw_probabilistic_constraints_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if !route.is_some_and(|r| r <= at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_verifier_pay_v1 requires palw_probabilistic_constraints_v1 (the kernel route it pays on) armed at or below it",
            ));
        }
        let opv =
            self.palw_panel_free_v1.as_ref().filter(|f| f.activation != ForkActivation::never()).map(|f| f.activation.daa_score());
        if !opv.is_some_and(|o| o <= at) {
            return Err(PalwModeV2Error::Invalid("palw_verifier_pay_v1 requires palw_panel_free_v1 armed at or below it"));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_verifier_pay_v1 cannot be armed: M*-49 is armed only by the single full-activation release",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};

    #[test]
    fn palw_verifier_pay_the_fence_is_dormant_everywhere_refused_when_armed_and_hashed_some_only() {
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_verifier_pay_v1, None);
            p.validate_palw_verifier_pay_v1().unwrap();
        }
        let mut p = TESTNET_PARAMS;
        let (id, schedule) = (p.consensus_params_id(), p.consensus_schedule_id());
        p.palw_verifier_pay_v1 = Some(ForkActivation::never());
        p.validate_palw_verifier_pay_v1().unwrap();
        assert_eq!(p.palw_verifier_pay_activation(), None);
        p.palw_verifier_pay_v1 = Some(ForkActivation::new(1_000));
        assert!(p.validate_palw_verifier_pay_v1().is_err(), "refused when armed");
        assert_ne!(p.consensus_params_id(), id, "an armed height is a different network");
        assert_ne!(p.consensus_schedule_id(), schedule);
        let policy = palw_verifier_pay_interim_policy_v1(1_000);
        assert_eq!((policy.slots, policy.bounty_cap), (4, 40 * SOMPI_PER_KASPA));
    }

    /// **C9 (ECON §5e)**: the interim fee is the formula's, and an honest watcher's books are non-negative over a fault-free window
    /// at the interim terms (its expected draws earn `F` each and cost `F_min` each plus its stake's capital cost) — for any stake share,
    /// because draws are proportional to stake. A fee of exactly `F_min` loses money (the capital cost unpaid).
    #[test]
    fn palw_verifier_pay_c9_an_honest_watchers_books_are_non_negative_over_a_fault_free_window() {
        use misaka_palw_kernel::verifier_pay::check_fee_with_stake_capital_v1;
        let (f_min, r_w, s_pool, q, m, n) = (
            PALW_VERIFIER_PAY_F_MIN_SOMPI_V1,
            PALW_VERIFIER_PAY_R_W_PPM_V1,
            PALW_VERIFIER_PAY_S_POOL_SOMPI_V1,
            PALW_VERIFIER_PAY_Q_PPM_V1,
            u64::from(PALW_VERIFIER_PAY_SLOTS_V1),
            PALW_VERIFIER_PAY_CLAIMS_PER_EPOCH_V1,
        );
        let fee = check_fee_with_stake_capital_v1(f_min, r_w, s_pool, q, m, n);
        assert_eq!(fee, PALW_VERIFIER_PAY_CHECK_FEE_SOMPI_V1, "the interim F is the formula's: 1.36 BILI");
        // Books over `epochs` fault-free epochs, scaled by 10^12 (ppm × ppm) to stay exact: expected draws = (stake / S_pool)·q·m·N.
        let books = |fee: u64, stake: u64, epochs: u128| -> i128 {
            let draws_scaled = (stake as u128) * (q as u128) * (m as u128) * (n as u128) * epochs; // × 10^6 / S_pool
            let income = draws_scaled * fee as u128 / s_pool as u128;
            let checks = draws_scaled * f_min as u128 / s_pool as u128;
            let capital = (r_w as u128) * (stake as u128) * epochs; // × 10^6
            income as i128 - checks as i128 - capital as i128
        };
        for share_permille in [1u64, 10, 100, 500, 1000] {
            let stake = s_pool / 1000 * share_permille;
            assert!(books(fee, stake, 1_000) >= 0, "a {share_permille}‰ watcher is in the red at F = {fee}");
            assert!(books(f_min, stake, 1_000) < 0, "at F = F_min the {share_permille}‰ watcher's capital is unpaid (C9)");
        }
    }
}
