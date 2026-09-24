//! **ADR-0152 V-1: the vesting row** — the type alone (testnet-12, R-core+).
//!
//! A Final claim's reward is not paid at Final. `finalize_claim` writes one row per claim into the
//! rooted map `PalwChainStateV2::vesting` (keyed by `claim_id`, separate from `claims`, outliving
//! claim retirement), and the fold's step 3d moves a row's legs into `pending_payouts` only once
//! the row is mature (V-4) — so a conviction inside the conviction window burns the row instead
//! (V-5), and the reward counts as recoverable value only because nothing can move it out early
//! (V-6: a row is not a UTXO, not collateral, not committed stake).
//!
//! This module carries the TYPE only, so the audit session's S can write its stubs
//! (`vesting_row(&claim_id) -> Option<&PalwVestingRowV1>`, `burn_vesting_row`) against it before
//! the rows themselves land (the same arrangement as P7's `PalwPanelStakeDrawV1`). The map, the
//! counters, the `Vesting`/`VestingNote` delta entries (71–73, after S's 66–70), step 3d and A-KEY
//! land with the vesting work; nothing here is in any state root yet.
//!
//! **The attribution fields are copies** (v3.1 N8, agreed with the audit): `job_identity`,
//! `free_prompt`, `trace_root` and `segment_count` are copied at Final from the claim record and its
//! panel, in the same funnel and under the same write rule as M2's `PalwPanelLiabilityRecordV1`
//! (non-zero only where `offence_attribution_active`). The row never resolves them through the
//! liability row, so the row alone can bind a conviction after the claim retires (J-2).

use kaspa_hashes::Hash64;

use crate::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use crate::palw_state_v2::{PalwBondKeyV2, PalwPayoutV2};

/// **One Final claim's vested reward** (ADR-0152 V-1, v3.1), exactly the ADR's field list and order.
/// Borsh encodes it field by field in this order; the order is part of the v22 layout.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVestingRowV1 {
    pub claim_id: Hash64,
    /// The claim's producer (executor) bond — the bond S3's action tier debits on a conviction
    /// after Final, and the payee of `producer`.
    pub producer_bond: PalwBondKeyV2,
    pub class_id: Hash64,
    pub execution_root: Hash64,
    /// R-core's own copy: the artifact the execution check reads after the claim retires.
    pub artifact_root: Hash64,
    /// Copied from the liability record (M2, SPEC §4.1): 0 = not recorded, never convicts.
    pub job_identity: Hash64,
    /// Copied: the lane, for the identity checks J1/J5.
    pub free_prompt: bool,
    /// Copied: the identity check J4.
    pub trace_root: Hash64,
    /// Copied: a V3 receipt's liability after retirement; 0 = unknown.
    pub segment_count: u16,
    /// The door of the Final-basis licence set (X3). A Final claim always has one, so it is bare
    /// here, unlike the liability record's `Option` (a claim voided before any licence has none).
    pub licence_door: PalwLicenceDoorTagV1,
    /// F4's recount of the Final-basis set (Q-3): 2 or 3.
    pub basis_k: u8,
    pub escrowed_reward: u64,
    /// The ADR-0091 buyback bound `s`, priced into `G_res`.
    pub buyback_bound: u64,
    /// The producer's leg; its payload is fixed at Final.
    pub producer: PalwPayoutV2,
    /// The credited seats' legs, per claim, in seat order.
    pub seats: Vec<(PalwBondKeyV2, PalwPayoutV2)>,
    pub reserve: u64,
    pub final_daa: u64,
    /// `palw_panel_liability_expiry_v1(final_daa, window_court)`; a DA session may extend it (DA-5).
    pub expiry_daa: u64,
    /// The anchor's settled count at Final — the second clock the row matures against (V-4).
    pub settled_at_final: u64,
    /// The maturity latch (X29): set once, never cleared.
    pub matured_at: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn row() -> PalwVestingRowV1 {
        let bond = |i: u8| PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([i; 64]), u32::from(i)));
        let payout = |i: u8, amount: u64| PalwPayoutV2 { payload: Hash64::from_bytes([i; 64]), amount };
        PalwVestingRowV1 {
            claim_id: Hash64::from_bytes([1; 64]),
            producer_bond: bond(2),
            class_id: Hash64::from_bytes([3; 64]),
            execution_root: Hash64::from_bytes([4; 64]),
            artifact_root: Hash64::from_bytes([5; 64]),
            job_identity: Hash64::from_bytes([6; 64]),
            free_prompt: true,
            trace_root: Hash64::from_bytes([7; 64]),
            segment_count: 4,
            licence_door: PalwLicenceDoorTagV1::Coverage,
            basis_k: 2,
            escrowed_reward: 8,
            buyback_bound: 9,
            producer: payout(10, 11),
            seats: vec![(bond(12), payout(13, 14)), (bond(15), payout(16, 17))],
            reserve: 18,
            final_daa: 19,
            expiry_daa: 20,
            settled_at_final: 21,
            matured_at: Some(22),
        }
    }

    /// The row round-trips, and its encoding is the ADR's field order and nothing else: the
    /// fixed-width prefix up to `seats` is pinned by length, so a field inserted or reordered
    /// before `seats` moves it.
    #[test]
    fn the_vesting_row_encodes_its_fields_in_the_adr_order() {
        let r = row();
        let bytes = borsh::to_vec(&r).unwrap();
        assert_eq!(borsh::from_slice::<PalwVestingRowV1>(&bytes).unwrap(), r);
        let outpoint = borsh::to_vec(&r.producer_bond).unwrap().len();
        let payout = 64 + 8;
        // claim_id, producer_bond, class_id, execution_root, artifact_root, job_identity,
        // free_prompt, trace_root, segment_count, licence_door (Coverage: one tag byte), basis_k,
        // escrowed_reward, buyback_bound, producer
        let prefix = 64 + outpoint + 64 + 64 + 64 + 64 + 1 + 64 + 2 + 1 + 1 + 8 + 8 + payout;
        assert_eq!(&bytes[prefix..prefix + 4], &2u32.to_le_bytes(), "`seats` starts right after `producer`");
        let seats = 4 + 2 * (outpoint + payout);
        // reserve, final_daa, expiry_daa, settled_at_final, matured_at (Some: 1 + 8)
        assert_eq!(bytes.len(), prefix + seats + 8 + 8 + 8 + 8 + 1 + 8);
        assert_eq!(bytes[prefix - payout - 8 - 8 - 1 - 1], 1, "licence_door Coverage is tag 1");
    }
}
