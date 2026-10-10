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

/// INTERIM `F` (POLICY, unapproved): 1 BILI per drawn check.
pub const PALW_VERIFIER_PAY_CHECK_FEE_SOMPI_V1: u64 = SOMPI_PER_KASPA;
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
        assert_eq!((policy.check_fee, policy.slots, policy.bounty_cap), (SOMPI_PER_KASPA, 4, 40 * SOMPI_PER_KASPA));
    }
}
