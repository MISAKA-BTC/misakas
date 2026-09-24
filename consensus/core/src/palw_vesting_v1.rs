//! **ADR-0152 V-1: the vesting row** — the type alone (testnet-12, R-core+).
//!
//! A Final claim's reward is not paid at Final. `finalize_claim` writes one row per claim into the
//! rooted map `PalwChainStateV2::vesting` (keyed by `claim_id`, separate from `claims`, outliving
//! claim retirement), and the fold's step 3d moves a row's legs into `pending_payouts` only once
//! the row is mature (V-4) — so a conviction inside the conviction window burns the row instead
//! (V-5), and the reward counts as recoverable value only because nothing can move it out early
//! (V-6: a row is not a UTXO, not collateral, not committed stake).
//!
//! This module carries the TYPES only, so the audit session's S can write its stubs
//! (`vesting_row(&claim_id) -> Option<&PalwVestingRowV1>`, `burn_vesting_row`) against them before
//! the rows themselves land (the same arrangement as P7's `PalwPanelStakeDrawV1`).
//!
//! **The v22 skeleton (ADR-0152 v3.1 §6 rows 10, 17, 25) declared the layout around them**: the
//! rooted map `PalwChainStateV2::vesting` (`claim_id → PalwVestingRowV1`) and the three counters
//! ([`PalwVestingCountersV1`]) sit after S's five rooted items in the one R-core+ root block and
//! carriage tail, and the delta journal carries `Vesting` (71), `VestingNote` (72, apply and revert
//! are no-ops; payload [`PalwVestingNoteV1`], phase2-plan §2.5) and `VestingCounters` (73). Every
//! writer is dormant: the map is empty and the counters are zero on every network until the
//! vesting work (V-1…V-8, step 3d, A-KEY) lands, so none of it is hashed or carried yet.
//!
//! **The attribution fields are copies** (v3.1 N8, agreed with the audit): `job_identity`,
//! `free_prompt`, `trace_root` and `segment_count` are copied at Final from the claim record and its
//! panel, in the same funnel and under the same write rule as M2's `PalwPanelLiabilityRecordV1`
//! (non-zero only where `offence_attribution_active`). The row never resolves them through the
//! liability row, so the row alone can bind a conviction after the claim retires (J-2).

use kaspa_hashes::Hash64;

use crate::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use crate::palw_offence_v1::PalwOffenceKindV1;
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

/// **The vesting counters** (ADR-0152 v3.1 V-3; v22 row 17's `vesting_created_sompi`,
/// `vesting_moved_sompi`, `vesting_burned_sompi`): every sompi a row was created with, moved into
/// `pending_payouts` by step 3d, or burned by a conviction, in that order. One struct, encoded and
/// hashed as the three `u128`s in that order; rooted in the R-core+ block after S's items, and
/// journaled whole by `PalwDeltaEntryV2::VestingCounters` (73). Zero on every network until the
/// vesting writers land — and zero is not hashed (the block is Some-only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVestingCountersV1 {
    pub created: u128,
    pub moved: u128,
    pub burned: u128,
}

impl PalwVestingCountersV1 {
    /// All three zero: the dormant value, which the R-core+ root block does not hash.
    pub fn is_zero(&self) -> bool {
        self.created == 0 && self.moved == 0 && self.burned == 0
    }
}

/// Which leg of a vesting move a [`PalwVestingLegV1`] is (phase2-plan §2.2). Appended only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwVestingLegKindV1 {
    Producer,
    Seat,
    Reserve,
    Reporter,
}

/// **One leg of a vesting move** (phase2-plan §2.2): one queue write, or the reserve. Carried in
/// the journal-only [`PalwVestingNoteV1`], so its encoding is part of the v22 delta layout.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVestingLegV1 {
    pub kind: PalwVestingLegKindV1,
    /// `None` for the reserve.
    pub payee_bond: Option<PalwBondKeyV2>,
    /// Fixed at Final (a row's legs) or at conviction (a reporter's).
    pub payload: Hash64,
    pub amount: u64,
    /// The `pending_payouts` key the leg lands on; `None` for the reserve.
    pub queue_key: Option<Hash64>,
}

impl PalwVestingLegV1 {
    /// I-4: a leg spends the per-block budget iff it is a queue write of a positive amount.
    pub fn takes_budget(&self) -> bool {
        self.queue_key.is_some() && self.amount > 0
    }
}

/// Where a vesting move came from (phase2-plan §2.2).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwVestingSourceV1 {
    Reporter { offence_id: Hash64 },
    Row { claim_id: Hash64 },
}

/// **The cause of a vesting change, journaled and never applied** (phase2-plan §2.5; the payload of
/// `PalwDeltaEntryV2::VestingNote`, 72). A row deletion looks the same whether the row moved or
/// burned, and both can happen in one block; the note says which, the way
/// `palw_escrow_destroyed_by_delta_v2` reads facts off the delta instead of keeping running totals
/// in the root. Apply and revert are no-ops. Declared by the v22 skeleton; no writer emits one yet.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwVestingNoteV1 {
    Latched {
        claim_id: Hash64,
        matured_at: u64,
    },
    /// Queue keys included.
    Moved {
        source: PalwVestingSourceV1,
        legs: Vec<PalwVestingLegV1>,
    },
    Burned {
        claim_id: Hash64,
        offence_id: Hash64,
        kind: PalwOffenceKindV1,
        sompi: u64,
        legs: Vec<PalwVestingLegV1>,
    },
    /// S4′: one seat's share of a row burned.
    ShareBurned {
        claim_id: Hash64,
        seat: PalwBondKeyV2,
        offence_id: Hash64,
        sompi: u64,
    },
    ReporterAwarded {
        offence_id: Hash64,
        reporter: PalwBondKeyV2,
        payload: Hash64,
        sompi: u64,
    },
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

    /// The counters encode as the ADR's three `u128`s, in the order created, moved, burned.
    #[test]
    fn the_vesting_counters_encode_as_three_u128_in_the_adr_order() {
        let c = PalwVestingCountersV1 { created: 1, moved: 2, burned: 3 };
        let bytes = borsh::to_vec(&c).unwrap();
        let mut want = Vec::new();
        for v in [1u128, 2, 3] {
            want.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(bytes, want);
        assert_eq!(borsh::from_slice::<PalwVestingCountersV1>(&bytes).unwrap(), c);
        assert!(PalwVestingCountersV1::default().is_zero() && !c.is_zero());
    }

    /// Every note variant round-trips, at its positional tag (phase2-plan §2.5's order).
    #[test]
    fn every_vesting_note_round_trips_at_its_tag() {
        let r = row();
        let leg = PalwVestingLegV1 {
            kind: PalwVestingLegKindV1::Seat,
            payee_bond: Some(r.producer_bond),
            payload: Hash64::from_bytes([9; 64]),
            amount: 5,
            queue_key: Some(Hash64::from_bytes([8; 64])),
        };
        assert!(leg.takes_budget());
        let reserve = PalwVestingLegV1 { kind: PalwVestingLegKindV1::Reserve, payee_bond: None, queue_key: None, ..leg.clone() };
        assert!(!reserve.takes_budget());
        let notes = [
            (0u8, PalwVestingNoteV1::Latched { claim_id: r.claim_id, matured_at: 3 }),
            (
                1,
                PalwVestingNoteV1::Moved {
                    source: PalwVestingSourceV1::Row { claim_id: r.claim_id },
                    legs: vec![leg.clone(), reserve],
                },
            ),
            (
                2,
                PalwVestingNoteV1::Burned {
                    claim_id: r.claim_id,
                    offence_id: Hash64::from_bytes([4; 64]),
                    kind: PalwOffenceKindV1::CourtConviction,
                    sompi: 7,
                    legs: vec![leg.clone()],
                },
            ),
            (
                3,
                PalwVestingNoteV1::ShareBurned {
                    claim_id: r.claim_id,
                    seat: r.producer_bond,
                    offence_id: Hash64::from_bytes([4; 64]),
                    sompi: 1,
                },
            ),
            (
                4,
                PalwVestingNoteV1::ReporterAwarded {
                    offence_id: Hash64::from_bytes([4; 64]),
                    reporter: r.producer_bond,
                    payload: Hash64::from_bytes([5; 64]),
                    sompi: 2,
                },
            ),
        ];
        for (tag, note) in notes {
            let bytes = borsh::to_vec(&note).unwrap();
            assert_eq!(bytes[0], tag, "{note:?}");
            assert_eq!(borsh::from_slice::<PalwVestingNoteV1>(&bytes).unwrap(), note);
        }
        let source = PalwVestingSourceV1::Reporter { offence_id: Hash64::from_bytes([1; 64]) };
        assert_eq!(borsh::from_slice::<PalwVestingSourceV1>(&borsh::to_vec(&source).unwrap()).unwrap(), source);
    }
}
