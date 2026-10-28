//! **RFC08 08-B / ADR-0184 §1: a native claim's fee is held from its Final to its liability horizon** (dormant
//! `palw_native_liability_horizon_v1`; kernel route aux 144). User decision 2026-10-17, horizon rule 2026-10-18.
//!
//! ```text
//! horizon     Final floor + window_court, derived from the producer-signed commit inputs (no wire change): the family's signed
//!             deadline bounds the Final floor (RFC08 08-A, `native_claim_hard_deadline_v1`). Only claims committed at or past the
//!             fence take the rule; an older claim keeps its old ruleset byte for byte.
//! hold        at Final the producer's credit is not queued: one held row (aux 144) records the payee, the amount, the requester's
//!             refund payee and the horizon; the producer's bond-budget reservation stays open with it.
//! release     the closing tick releases a held row whose horizon has passed and whose job has no open dispute: the payout row is
//!             written, the native income paid and the reservation closed — exactly what the Final would have done, later.
//! conviction  a conviction (court verdict or objective DA default) of the Final claim before the horizon — the family's own
//!             evidence, no new proof of any kind (PRINCIPLES §8) — deletes the held row, refunds the unpaid fee to the requester
//!             and slashes the producer's reserve as a pre-Final conviction does.
//! ```
//!
//! Nothing here grants consensus credit or weight; the held amount is the producer's own fee, never issuance.
use borsh::{BorshDeserialize, BorshSerialize};

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_v2::PalwBondKeyV2;

/// The kernel route aux table of held native fees (allocated 2026-10-18: aux 144).
pub const PALW_NATIVE_LIABILITY_HOLD_TABLE_V1: u8 = 144;
/// `0x01 ‖ family ‖ job` (66 bytes): one held native fee ([`PalwNativeHeldFeeRowV1`]).
pub const PALW_NATIVE_HELD_FEE_PREFIX_V1: u8 = 0x01;
/// The rows' version.
pub const PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1: u8 = 1;
/// Payments a block's tick releases. This bounds payments, not the number of retained rows inspected.
pub const PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1: usize = 256;

/// The native families the hold covers (ADR-0184 §1). The mining family holds nothing of its own: its claim is accepted only over
/// Final components whose fees are held to their own horizons, and its Master closes only after they release.
pub const PALW_NATIVE_FAMILY_JOB_V39: u8 = 39;
pub const PALW_NATIVE_FAMILY_TEXT_V29: u8 = 29;
pub const PALW_NATIVE_FAMILY_FUNDED_V11: u8 = 11;

/// **A held native fee** (aux 144, `0x01 ‖ family ‖ job`). `queue` / `payload` / `amount` are exactly what the Final would have
/// written to the payout queue; `income_*` the native income the Final would have paid (`pay_native_recipient_v1`'s root and kind,
/// the bond is `producer`); `budget` the reservation it would have closed. `refund_payload` is the requester's payee for a refund.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativeHeldFeeRowV1 {
    pub version: u8,
    pub family: u8,
    pub job: [u8; 64],
    pub claim: [u8; 64],
    pub producer: PalwBondKeyV2,
    pub queue: [u8; 64],
    pub payload: [u8; 64],
    pub amount: u64,
    pub income_root: [u8; 64],
    pub income_kind: u8,
    pub budget: [u8; 64],
    pub refund_payload: [u8; 64],
    pub final_daa: u64,
    /// Released once this DAA has passed (the claim's liability horizon); accusable through it.
    pub horizon_daa: u64,
}

pub fn palw_native_held_fee_key_v1(family: u8, job: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(66);
    key.push(PALW_NATIVE_HELD_FEE_PREFIX_V1);
    key.push(family);
    key.extend_from_slice(job);
    key
}

impl Params {
    /// The fence is dormant (`None` on every preset). Arming it requires the once-only settlement registry at or below it (RFC08
    /// 08-A's sourced native admissions ride the same fence and the release settles once).
    pub fn validate_palw_native_liability_horizon_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_native_liability_horizon_v1.filter(|at| *at != ForkActivation::never()) else { return Ok(()) };
        if !self.palw_settlement_once_v1.is_some_and(|s| s != ForkActivation::never() && s.daa_score() <= at.daa_score()) {
            return Err(PalwModeV2Error::Invalid("palw_native_liability_horizon_v1 requires palw_settlement_once_v1 at or below it"));
        }
        Ok(())
    }

    /// The state params' mirror of the fence.
    pub fn sync_palw_native_liability_horizon_v1(&mut self) {
        let from = self.palw_native_liability_horizon_v1.filter(|at| *at != ForkActivation::never()).map(|at| at.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_native_liability_horizon_from(from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_family_and_job_and_the_table_is_its_own() {
        let key = palw_native_held_fee_key_v1(PALW_NATIVE_FAMILY_JOB_V39, &[7; 64]);
        assert_eq!(key.len(), 66);
        assert_eq!((key[0], key[1]), (PALW_NATIVE_HELD_FEE_PREFIX_V1, 39));
        assert_ne!(palw_native_held_fee_key_v1(PALW_NATIVE_FAMILY_TEXT_V29, &[7; 64]), key);
        for other in [
            crate::palw_settlement_once_v1::PALW_SETTLEMENT_ONCE_TABLE_V1,
            crate::palw_paid_claim_trap_v1::PALW_PAID_CLAIM_TRAP_TABLE_V1,
            crate::palw_g14_lane_closure_v1::PALW_G14_LANE_CLOSURE_TABLE_V1,
            crate::palw_class_registration_origin_v1::PALW_CLASS_REGISTRATION_ORIGIN_TABLE_V1,
        ] {
            assert_ne!(other, PALW_NATIVE_LIABILITY_HOLD_TABLE_V1);
        }
    }

    #[test]
    fn every_preset_leaves_the_fence_dormant() {
        for p in [
            crate::config::params::MAINNET_PARAMS,
            crate::config::params::TESTNET_PARAMS,
            crate::config::params::SIMNET_PARAMS,
            crate::config::params::DEVNET_PARAMS,
        ] {
            assert!(p.palw_native_liability_horizon_v1.is_none());
        }
    }
}
