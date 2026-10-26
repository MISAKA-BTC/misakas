//! **RFC-0011 §15.7: the probabilistic-constraint route's fence — `palw_probabilistic_constraints_v1`** (dormant, no height).
//!
//! ADR-0172's kernel route (versioned kernel descriptors, declarative `VerificationPlanV1`s, post-commit challenges, constraint
//! receipts and public fault proofs; the reference implementation is `misaka-palw-kernel`) would register kernel-bound classes and
//! finalize their claims under this separately named fence. RFC-0011 §15.7 introduces the fence **without an activation height**:
//! the beacon/transcript review, the soundness review of the composition, shadow comparison against the Panel route, the
//! RFC-0014/ADR-0173 public-prosecution gate and the 9B-8k / long-context measurements gate any proposal to arm it.
//!
//! **Dormant**: `None` on every preset and in no flag-day list; hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())`, its activation alone visited by `for_each_fence`. Since G14 lane D
//! the virtual processor reads it: where it is in force, kernel route objects (tags 110–119) and onboarding objects (104–108) are
//! folded into `PalwChainStateV2` with their rows, deltas, tail 0xEC and root (`g14_kernel_route_e2e`, `g14_registration_e2e`);
//! below it they are dropped by name and the block stands. Arming it is gated by
//! [`Params::validate_palw_probabilistic_constraints_v1`]: the acceptance rule exists; since lane g14p/kernel-route (2026-10-22) the
//! refusal is no longer by name but by an arm-readiness record (`crate::palw_kernel_route_arming_v1`) carrying the reviews,
//! shadow comparison, measurements and drill above. The pinned list is empty, so no network can switch it on yet.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;

impl Params {
    /// Whether the probabilistic-constraint route is in force at `daa_score` (never on a shipped preset).
    pub fn palw_probabilistic_constraints_active_at(&self, daa_score: u64) -> bool {
        self.palw_probabilistic_constraints_v1.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **The fence's record gate** (lane g14p/kernel-route, U01): the kernel-route acceptance rule is built (the fold); arming is
    /// refused until an arm-readiness record is pinned (`crate::palw_kernel_route_arming_v1`; the pinned list is empty).
    pub fn validate_palw_probabilistic_constraints_v1(&self) -> Result<(), PalwModeV2Error> {
        self.validate_palw_probabilistic_constraints_in_v1(crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORDS_V1)
    }

    /// The gate against `records` in place of the pinned list (the drill record path; the node's shipped path passes the pinned list).
    pub fn validate_palw_probabilistic_constraints_in_v1(
        &self,
        records: &[crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1],
    ) -> Result<(), PalwModeV2Error> {
        if self.has_isolated_native_calibration_v34() {
            return self.validate_isolated_native_calibration_v34().map_err(|_| {
                PalwModeV2Error::Invalid("native calibration34 owning isolated genesis or complete original policies are invalid")
            });
        }
        self.validate_palw_kernel_route_fence_record_in_v1(
            crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1,
            records,
        )
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
