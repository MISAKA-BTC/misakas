//! **RFC-0004 Part II: the typed-roots fence — `palw_typed_roots_v1`** (dormant, no height).
//!
//! MISAKA registers, executes and settles independently verifiable useful AI computation, not weight files: a kernel-route class binds
//! a computation specification with typed roots (`misaka-palw-kernel::spec` — Weights byte for byte as today, Memory updated per step
//! and carried across jobs, Retrieval over a public snapshot with a deterministic integer rule, Composite pipelines of verified
//! kernels). Under this fence the route's ledger schedules the typed-roots extension descriptor `K2-TR-v1`
//! (`misaka_palw_kernel::spec::k2_tr_v1_descriptor`) as Active from the fence's height, and the acceptance walk carries route objects of
//! inner kind 19 (`Spec`); below it, or where it is absent, a `Spec` object is dropped by name and the block stands, and the route's
//! schedule, `config_root` and every root are exactly what they were.
//!
//! **Dormant**: `None` on every preset and in no flag-day list; hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())`, its activation alone visited by `for_each_fence`, and refused when
//! armed ([`Params::validate_palw_typed_roots_v1`]) — RFC-0004 §II.4: nothing in Part II is armed on its own; the single
//! full-activation release names its height with the others. A test arms it through the harness's `Config` seam only.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;

impl Params {
    /// The fence's activation, where the network declares one (`None`: absent or `never()`) — a genesis constant of the route.
    pub fn palw_typed_roots_activation(&self) -> Option<u64> {
        self.palw_typed_roots_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// Whether typed-root objects are in force at `daa_score` — never, in this binary's presets.
    pub fn palw_typed_roots_active_at(&self, daa_score: u64) -> bool {
        self.palw_typed_roots_v1.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **The fence's refusal**: Part II is armed only by the single full-activation release, so any armed height is refused here.
    pub fn validate_palw_typed_roots_v1(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_typed_roots_v1 {
            Some(f) if f != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_typed_roots_v1 cannot be armed: RFC-0004 Part II is armed only by the single full-activation release",
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
    fn the_fence_is_dormant_everywhere_refused_when_armed_and_hashed_some_only() {
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_typed_roots_v1, None);
            assert!(!p.palw_typed_roots_active_at(u64::MAX));
            assert_eq!(p.palw_typed_roots_activation(), None);
            p.validate_palw_typed_roots_v1().unwrap();
        }
        let mut p = TESTNET_PARAMS;
        let (id, schedule) = (p.consensus_params_id(), p.consensus_schedule_id());
        p.palw_typed_roots_v1 = Some(ForkActivation::never());
        p.validate_palw_typed_roots_v1().unwrap();
        assert_eq!(p.palw_typed_roots_activation(), None);
        p.palw_typed_roots_v1 = Some(ForkActivation::new(1_000));
        assert!(p.validate_palw_typed_roots_v1().is_err());
        assert_eq!(p.palw_typed_roots_activation(), Some(1_000));
        assert!(p.palw_typed_roots_active_at(1_000) && !p.palw_typed_roots_active_at(999));
        assert_ne!(p.consensus_params_id(), id, "an armed height would be a different network");
        assert_ne!(p.consensus_schedule_id(), schedule, "and a different schedule");
    }
}
