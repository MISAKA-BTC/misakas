//! **RFC-0011 §15.7: the probabilistic-constraint route's fence — `palw_probabilistic_constraints_v1`** (dormant, no height).
//!
//! ADR-0172's kernel route (versioned kernel descriptors, declarative `VerificationPlanV1`s, post-commit challenges, constraint
//! receipts and public fault proofs; the reference implementation is `misaka-palw-kernel`) would register kernel-bound classes and
//! finalize their claims under this separately named fence. RFC-0011 §15.7 introduces the fence **without an activation height**:
//! the beacon/transcript review, the soundness review of the composition, shadow comparison against the Panel route, the
//! RFC-0014/ADR-0173 public-prosecution gate and the 9B-8k / long-context measurements gate any proposal to arm it.
//!
//! **Dormant**: `None` on every preset and in no flag-day list; hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())`, its activation alone visited by `for_each_fence`. Nothing in
//! consensus reads it. Arming it is refused by [`Params::validate_palw_probabilistic_constraints_v1`]: an unknown kernel is never
//! success, so a network cannot switch on a route this binary has no acceptance rule for.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;

impl Params {
    /// Whether the probabilistic-constraint route is in force at `daa_score` — never, in this binary.
    pub fn palw_probabilistic_constraints_active_at(&self, daa_score: u64) -> bool {
        self.palw_probabilistic_constraints_v1.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **The fence's refusal**: this binary carries no acceptance rule for the kernel route, so any armed height is refused.
    pub fn validate_palw_probabilistic_constraints_v1(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_probabilistic_constraints_v1 {
            Some(f) if f != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_probabilistic_constraints_v1 cannot be armed: RFC-0011 §15.7 assigns it no height and this binary has no kernel-route acceptance rule",
            )),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};

    #[test]
    fn the_fence_is_dormant_everywhere_and_refused_when_armed() {
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_probabilistic_constraints_v1, None);
            assert!(!p.palw_probabilistic_constraints_active_at(u64::MAX));
            p.validate_palw_probabilistic_constraints_v1().unwrap();
        }
        let mut p = TESTNET_PARAMS;
        let id = p.consensus_params_id();
        p.palw_probabilistic_constraints_v1 = Some(ForkActivation::never());
        p.validate_palw_probabilistic_constraints_v1().unwrap();
        p.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(1_000));
        assert!(p.validate_palw_probabilistic_constraints_v1().is_err());
        assert_ne!(p.consensus_params_id(), id, "an armed height would be a different network");
    }
}
