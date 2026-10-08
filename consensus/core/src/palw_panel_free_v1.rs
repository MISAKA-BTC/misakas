//! **RFC-0015: the Panel=0 fence — `palw_panel_free_v1`** (dormant, no height).
//!
//! RFC-0015's `OptimisticPublicVerification` replaces a fixed Panel's coverage with permissionless verifiers, objective fraud proofs, a
//! fixed public challenge window and a producer-centred reservation (`misaka-palw-kernel`: `OpvPolicyV1`, route tags 13 / 14,
//! `ClaimStateV1::Challengeable`). The consumer derives the kernel ledger's OPV activation height from this fence. RFC-0015 §13.3
//! forbids arming it until `ACTIVATION_ALLOWED` — G14 PASS on a real node for every rewarded profile, the new lifecycle / court / Final
//! implementation, the panel-free collateral review, supported monitoring and inclusion assumptions, an independent adversarial node
//! E2E with recovery and migration, and a separately coordinated schedule — and none of it exists.
//!
//! **Dormant**: `None` on every preset and in no testnet-12 flag-day list; hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())`, its activation alone visited by `for_each_fence`. Nothing in
//! consensus reads it. Arming it is refused by [`Params::validate_palw_panel_free_v1`]: the kernel route is not wired into the node
//! (no carrier, fold or RPC), so a network cannot switch on a verification mode this binary has no acceptance rule for.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;

impl Params {
    /// Whether the Panel-free mode is in force at `daa_score` — never, in this binary.
    pub fn palw_panel_free_active_at(&self, daa_score: u64) -> bool {
        self.palw_panel_free_v1.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **The fence's refusal**: RFC-0015 §13.3 `ACTIVATION_ALLOWED` is not evidenced and the node carries no Panel=0 acceptance rule,
    /// so any armed height is refused.
    pub fn validate_palw_panel_free_v1(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_panel_free_v1 {
            Some(f) if f != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_panel_free_v1 cannot be armed: RFC-0015 §13.3 ACTIVATION_ALLOWED (G14 on a real node, the panel-free lifecycle and collateral review, a coordinated schedule) is not evidenced and this binary has no Panel=0 acceptance rule",
            )),
            _ => Ok(()),
        }
    }
}
