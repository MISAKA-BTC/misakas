//! **ADR-0152 V-1…V-8: the vesting row and the rules that move it** (testnet-12, R-core+).
//!
//! A Final claim's reward is not paid at Final. Past `Params::palw_rcore_plus`, `finalize_claim`
//! writes one row per claim into the rooted map `PalwChainStateV2::vesting` (keyed by `claim_id`,
//! separate from `claims`, outliving claim retirement), and the fold's step 3d moves a row's legs
//! into `pending_payouts` only once the row is mature (V-4) — so a conviction inside the conviction
//! window burns the row instead (V-5), and the reward counts as recoverable value only because
//! nothing can move it out early (V-6: a row is not a UTXO, not collateral, not committed stake).
//!
//! **The v22 skeleton (ADR-0152 v3.1 §6 rows 10, 17, 25) declared the layout**: the rooted map
//! `PalwChainStateV2::vesting` (`claim_id → PalwVestingRowV1`) and the three counters
//! ([`PalwVestingCountersV1`]) sit after S's five rooted items in the one R-core+ root block and
//! carriage tail, and the delta journal carries `Vesting` (71), `VestingNote` (72, apply and revert
//! are no-ops; payload [`PalwVestingNoteV1`], phase2-plan §2.5) and `VestingCounters` (73).
//!
//! **This module now also carries the PURE half of the rules** (the vesting work, B in S's window,
//! ADR §3.3's ownership note; phase2-plan §2.3–§2.5 with the v3.1 amendments): the A-KEY queue keys
//! ([`palw_vesting_payout_key_v1`], [`palw_reporter_payout_key_v1`]), V-4's maturity
//! ([`palw_vesting_row_maturity_v1`], [`palw_chain_vesting_halted_v1`]), V-7's matured-move
//! iterator and planner ([`palw_vesting_matured_moves_v1`], and [`palw_vesting_mint_plan_v1`] in
//! ADR §7.3's three-argument form: the budget in new queue keys, with the market reserve,
//! [`palw_vesting_budget_v1`]), the committed-state question [`palw_vesting_next_block_plan_v1`],
//! B-3's payee question ([`palw_bond_is_payee_of_unmatured_row_v1`], defined in `palw_state_v2`
//! where S-1 declares it) and V-3's consistency ([`palw_vesting_consistency_v1`]). Every function
//! here is a function of `(params, state)` and the raw second-clock depth the fold's extras carry
//! (I-8), so the fold's step 3d, the RPC and the Phase 2 coinbase harness run the SAME rules. **Two
//! entry points, by the state the caller holds**: step 3d plans the state it has just latched with
//! [`palw_vesting_mint_plan_v1`]; a caller holding a committed state asks what the next block's 3d
//! will move with [`palw_vesting_next_block_plan_v1`], which replays that block's drain and latch
//! first. The writers — the row at Final, the latch, the move and the burn hook S-4 calls — are
//! the fold's, in `palw_state_v2.rs`, and run only past `palw_rcore_plus`; below it this module is
//! never reached and no row exists. The payee's legs for RPC are the state's
//! `vesting_legs_of_payee` (phase2-plan §2.3).
//!
//! **The attribution fields are copies** (v3.1 N8, agreed with the audit): `job_identity`,
//! `free_prompt`, `trace_root` and `segment_count` are copied at Final from the claim record and its
//! liability record, in the same funnel and under the same write rule as M2's
//! `PalwPanelLiabilityRecordV1` (non-zero only where `offence_attribution_active`). The row never
//! resolves them through the liability row, so the row alone can bind a conviction after the claim
//! retires (J-2).

use std::collections::BTreeSet;

use kaspa_hashes::Hash64;

use crate::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use crate::palw_offence_v1::PalwOffenceKindV1;
use crate::palw_panel_var_v1::palw_second_clock_holds_v1;
use crate::palw_state_v2::{
    PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX, PALW_V2_MAX_PAYOUTS_PER_BLOCK, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2,
    PalwDeltaEntryV2, PalwPayoutV2, PalwStateDeltaV2, PalwStateParamsV2, palw_panel_payout_key_v1, palw_second_clock_depth_v1,
};

/// B-3's vesting term. It is defined in `palw_state_v2` beside the withdrawal gates, where S-1
/// declared it with this exact signature and a `false` body; the integration (rcore/int-1) deleted
/// that stub for this body, so there is one definition and v6 reads it by name. Re-exported here
/// with the rest of the vesting rules.
pub use crate::palw_state_v2::palw_bond_is_payee_of_unmatured_row_v1;

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
    /// **V-3's buyback slice** (appended by the vesting work, tag 5): the ADR-0091 slice
    /// `finalize_claim` executed at this claim's Final past `palw_rcore_plus`. It is NOT vested
    /// (V-2: it leaves at Final as a market move into the pair's `msk_reserve`, priced into
    /// `G_res` as `s`), and the row records it as `buyback_bound`; the note names it so T03's
    /// coinbase identity closes from deltas alone (phase2-plan §5.3). Emitted only when positive.
    BuybackAtFinal {
        claim_id: Hash64,
        sompi: u64,
    },
    /// **V-3's reserve credit** (appended by the vesting work, tag 6): the row's `reserve` credited
    /// to `panel_reserve_sompi` by step 3d when the row moved — the identity's
    /// `Δ panel_reserve_sompi` term. The same sompi is the `Reserve` leg of the `Moved` note beside
    /// it; this names it so a reader of the identity need not decode legs. Emitted only when
    /// positive.
    ReserveCredited {
        claim_id: Hash64,
        sompi: u64,
    },
}

// ---------------------------------------------------------------------------------------------
// A-KEY: the queue keys a moved leg lands on (ADR-0152 V-7, phase2-plan F2)
// ---------------------------------------------------------------------------------------------

/// **The first byte of every key a moved producer or reporter leg is written under** (ADR-0152 V-7,
/// A-KEY; phase2-plan F2). The drain and `palw_v2_payout_outputs` take "the first
/// `PALW_V2_MAX_PAYOUTS_PER_BLOCK` of `pending_payouts` in key order", and V-7's queue lemma needs
/// every non-market row to sort before every market row (`0xFF`,
/// [`PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX`]). A producer leg keyed by its raw `claim_id` breaks
/// that for the 1/256 of claims whose id begins `0xFF` (T47), and a reporter leg keyed by a
/// uniform hash does the same; forcing `0x00` puts both at the very front, before the seat rows'
/// `0xFE` ([`crate::palw_state_v2::PALW_STATE_V2_PANEL_PAYOUT_KEY_PREFIX`]).
pub const PALW_STATE_V2_VESTING_PAYOUT_KEY_PREFIX: u8 = 0x00;

/// A-KEY: the domain of a moved producer leg's queue key (`H(domain ‖ claim_id)`, byte 0 forced).
/// A ROW key, kept out of `PALW_STATE_V2_ALL_DOMAINS` exactly as the market payout's and the
/// refund's row-key domains are (`PALW_STATE_V2_DOMAIN_MODEL_PAYOUT`,
/// `PALW_STATE_V2_DOMAIN_MODEL_REFUND`), so nothing a live network derives from that list moves.
pub const PALW_STATE_V2_DOMAIN_VESTING_PAYOUT: &[u8] = b"misaka-palw/state-v2/vesting-payout/v1";

/// A-KEY: the domain of a moved reporter leg's queue key (`H(domain ‖ offence_key)`, byte 0
/// forced). Kept out of `PALW_STATE_V2_ALL_DOMAINS` for the producer key's reason.
pub const PALW_STATE_V2_DOMAIN_REPORTER_PAYOUT: &[u8] = b"misaka-palw/state-v2/reporter-payout/v1";

/// **V-7's per-block move budget, counted in NEW QUEUE KEYS** (ADR-0152 v3.1 V-7, the backlog
/// amendment; the name is the contract's — ADR §6 row 30 and phase2-plan §2.2 keep
/// `PALW_V2_VESTING_LEGS_PER_BLOCK` = 8 "(new keys)" — although the unit is no longer the leg): a
/// move costs the number of distinct `pending_payouts` keys its legs land on that no earlier move
/// of this block's plan created ([`PalwVestingMoveV1::new_keys`]). Seat legs to one payee share one
/// key (`add_panel_payout` accumulates); the reserve is not a key. At most the drain's width, so
/// every key 3d creates is drained — and minted — by the next block (the queue lemma, T58).
pub const PALW_V2_VESTING_LEGS_PER_BLOCK: usize = 8;

/// **V-7's market reserve**: while the market has rows queued the budget is
/// `8 − min(2, market rows waiting)`, so the market keeps at least two of the next block's eight
/// drain slots through a post-halt backlog. A 6-key row (a producer and five credited seats, t12's
/// panel) still fits (6 ≤ 6).
pub const PALW_V2_VESTING_MARKET_RESERVE: usize = 2;

const _: () = assert!(PALW_V2_VESTING_LEGS_PER_BLOCK <= PALW_V2_MAX_PAYOUTS_PER_BLOCK);
const _: () = assert!(PALW_V2_VESTING_MARKET_RESERVE < PALW_V2_VESTING_LEGS_PER_BLOCK);

fn palw_vesting_row_key_v1(domain: &[u8], id: &Hash64) -> Hash64 {
    // The state module's own keyed construction (`keyed`/`finish`): BLAKE2b-512 keyed by the domain.
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    h.update(id.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    out[0] = PALW_STATE_V2_VESTING_PAYOUT_KEY_PREFIX;
    Hash64::from_bytes(out)
}

/// **A-KEY: the queue key a claim's moved producer leg lands on** — `H(DOMAIN_VESTING_PAYOUT ‖
/// claim_id)` with byte 0 forced to [`PALW_STATE_V2_VESTING_PAYOUT_KEY_PREFIX`] (never the raw
/// `claim_id`, phase2-plan §2.7). One per claim, so two rows moved in one block never share it.
pub fn palw_vesting_payout_key_v1(claim_id: &Hash64) -> Hash64 {
    palw_vesting_row_key_v1(PALW_STATE_V2_DOMAIN_VESTING_PAYOUT, claim_id)
}

/// **A-KEY: the queue key a moved reporter reward lands on** — `H(DOMAIN_REPORTER_PAYOUT ‖
/// offence_key)` with byte 0 forced to [`PALW_STATE_V2_VESTING_PAYOUT_KEY_PREFIX`].
pub fn palw_reporter_payout_key_v1(offence_key: &Hash64) -> Hash64 {
    palw_vesting_row_key_v1(PALW_STATE_V2_DOMAIN_REPORTER_PAYOUT, offence_key)
}

// ---------------------------------------------------------------------------------------------
// The row's legs (phase2-plan §2.2–§2.3)
// ---------------------------------------------------------------------------------------------

impl PalwVestingRowV1 {
    /// **The row's legs, in the order step 3d writes them**: the producer (A-KEY key), each
    /// credited seat in seat order (`palw_panel_payout_key_v1(payload)`, accumulated per payee),
    /// then the reserve (no key; credited to `panel_reserve_sompi`). Zero-amount legs are yielded
    /// and take no budget ([`PalwVestingLegV1::takes_budget`]); the move writes nothing for them.
    pub fn legs(&self) -> impl Iterator<Item = PalwVestingLegV1> + '_ {
        let producer = PalwVestingLegV1 {
            kind: PalwVestingLegKindV1::Producer,
            payee_bond: Some(self.producer_bond),
            payload: self.producer.payload,
            amount: self.producer.amount,
            queue_key: Some(palw_vesting_payout_key_v1(&self.claim_id)),
        };
        let seats = self.seats.iter().map(|(bond, payout)| PalwVestingLegV1 {
            kind: PalwVestingLegKindV1::Seat,
            payee_bond: Some(*bond),
            payload: payout.payload,
            amount: payout.amount,
            queue_key: Some(palw_panel_payout_key_v1(&payout.payload)),
        });
        let reserve = PalwVestingLegV1 {
            kind: PalwVestingLegKindV1::Reserve,
            payee_bond: None,
            payload: Hash64::default(),
            amount: self.reserve,
            queue_key: None,
        };
        std::iter::once(producer).chain(seats).chain(std::iter::once(reserve))
    }

    /// Σ [`PalwVestingLegV1::takes_budget`] over [`Self::legs`]: at most `1 + seats`.
    pub fn leg_count(&self) -> usize {
        self.legs().filter(PalwVestingLegV1::takes_budget).count()
    }

    /// **V-7's cost of the row as a block's first move** (ADR-0152 §7.3, IMPL-10: `leg_count`
    /// restated in keys): the distinct queue keys its budget-taking legs land on — the producer's
    /// A-KEY key and one per distinct seat payload (`add_panel_payout` accumulates two credited
    /// seats paid at one payload into one key). At most `1 + seats`; at most 6 on testnet-12's
    /// five-seat panel, so it fits the budget the market reserve leaves (6).
    pub fn key_count(&self) -> usize {
        self.legs().filter(PalwVestingLegV1::takes_budget).filter_map(|leg| leg.queue_key).collect::<BTreeSet<_>>().len()
    }

    /// Every sompi the row holds: producer + seats + reserve, exactly what `finalize_claim` named.
    pub fn total_sompi_u128(&self) -> u128 {
        self.seats.iter().fold(self.producer.amount as u128 + self.reserve as u128, |sum, (_, payout)| sum + payout.amount as u128)
    }

    /// [`Self::total_sompi_u128`] as `u64` (a row splits one `u64` reward, so it always fits;
    /// saturating for a hand-built row that does not).
    pub fn total_sompi(&self) -> u64 {
        u64::try_from(self.total_sompi_u128()).unwrap_or(u64::MAX)
    }

    /// The bonds a row pays — its producer, then each credited seat — which B-3 holds while the
    /// row is unmatured by V-4(a) ([`palw_bond_is_payee_of_unmatured_row_v1`]). A bond may appear
    /// twice (a producer that also sat); the payee index is a set.
    pub fn payee_bonds(&self) -> impl Iterator<Item = PalwBondKeyV2> + '_ {
        std::iter::once(self.producer_bond).chain(self.seats.iter().map(|(bond, _)| *bond))
    }
}

// ---------------------------------------------------------------------------------------------
// Moves and the plan (phase2-plan §2.2, §2.4; ADR-0152 v3.1 V-7)
// ---------------------------------------------------------------------------------------------

/// One whole move (a reporter reward, or a vesting row with all its legs), in V-7 order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwVestingMoveV1 {
    pub source: PalwVestingSourceV1,
    pub legs: Vec<PalwVestingLegV1>,
}

impl PalwVestingMoveV1 {
    /// Σ [`PalwVestingLegV1::takes_budget`] (phase2-plan I-4's unit, kept for RPC).
    pub fn leg_count(&self) -> usize {
        self.legs.iter().filter(|leg| leg.takes_budget()).count()
    }

    /// Every sompi the move carries, reserve included.
    pub fn total_sompi(&self) -> u128 {
        self.legs.iter().map(|leg| leg.amount as u128).sum()
    }

    /// **V-7's cost of this move** (ADR-0152 v3.1 V-7, "a move costs the number of distinct keys it
    /// creates that no earlier move of this block created"): the distinct queue keys its
    /// budget-taking legs land on, less those an earlier move of the same plan created
    /// (`created`). **It does not read the queue** (review of the vesting work, finding 1): a
    /// cost that excluded keys the queue already holds made the planner answer differently on a
    /// committed state — whose queue still holds the keys the last step 3d wrote — than on the
    /// state step 3d sees after step 1b drained them, so the RPC and the Phase 2 harness could not
    /// call the fold's function and get the fold's answer. Under the queue lemma the exclusion was
    /// a no-op in the fold anyway (the non-market part is empty at every step 3d, and a leg's key
    /// is never a market key), and where the lemma does not hold it only makes a move cost more.
    pub fn new_keys(&self, created: &BTreeSet<Hash64>) -> BTreeSet<Hash64> {
        self.legs
            .iter()
            .filter(|leg| leg.takes_budget())
            .filter_map(|leg| leg.queue_key)
            .filter(|key| !created.contains(key))
            .collect()
    }

    /// The move's cost as a block's first move: [`Self::new_keys`] against nothing created yet.
    pub fn key_count(&self) -> usize {
        self.new_keys(&BTreeSet::new()).len()
    }
}

/// Why a plan stopped (phase2-plan §2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwVestingStopV1 {
    /// The next row in `(expiry_daa, claim_id)` order is not latched: stop, never skip.
    NotLatched,
    /// The next move does not fit the block's budget of new keys.
    BudgetFull,
    /// Nothing is left to move.
    Empty,
}

/// **Step 3d's plan** (phase2-plan §2.2, with the v3.1 budget in new keys).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwVestingMintPlanV1 {
    pub moves: Vec<PalwVestingMoveV1>,
    /// Σ [`PalwVestingMoveV1::leg_count`] (legs, kept for RPC).
    pub legs: usize,
    /// Σ new queue keys — what the budget counts (V-7, v3.1).
    pub new_keys: usize,
    pub stopped: PalwVestingStopV1,
    /// The move or row the plan stopped at (`None` when `Empty`).
    pub stopped_at: Option<PalwVestingSourceV1>,
}

/// **V-4 for one row, pure** (phase2-plan §2.2; ADR-0152 V-4, F11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwVestingMaturityV1 {
    /// The latch, if set (X29): once set the row is mature whatever the clocks say later.
    pub matured_at: Option<u64>,
    /// `now >= expiry_daa`.
    pub daa_clock_met: bool,
    /// `settled_now − settled_at_final`.
    pub licences_since_final: u64,
    /// The RAW second-clock depth; `None` = no second clock configured (not a halt, F11).
    pub licences_needed: Option<u64>,
    /// The DAA at which `palw_second_clock_holds_v1`'s per-obligation bound releases the row
    /// (`expiry_daa + 2 × window_court`); `None` with no second clock configured.
    pub second_clock_bound_daa: Option<u64>,
    /// V-4(b): the raw depth is `Some` and the escaped depth `None` — a licence halt.
    pub halted: bool,
    /// V-4(c): an open DA session on the row's claim ([`palw_vesting_row_has_open_da_session_v1`]).
    pub da_session_open: bool,
    /// `(a) && !(b) && !(c)`, or latched.
    pub mature_now: bool,
}

/// **V-4(b): is the chain in a licence halt?** `raw_depth.is_some()` and the escaped depth
/// ([`palw_second_clock_depth_v1`]) `None` — no anchor has settled for `2 × window_court`
/// (phase2-plan F11: "no second clock configured" is NOT "halted", or rows would never mature on a
/// preset without the anchor depth). `raw_depth` is the fold's `extras.settled_anchor_depth` as it
/// reads it (`None` below `palw_audit_2026_09_23`).
pub fn palw_chain_vesting_halted_v1(state: &PalwChainStateV2, raw_depth: Option<u64>, now_daa: u64, window_court: u64) -> bool {
    raw_depth.is_some() && palw_second_clock_depth_v1(raw_depth, state.recent_anchor_daas(), now_daa, window_court).is_none()
}

/// **V-4(a)'s lock predicate over a row's two clocks**: `PalwSlashableLockV1::is_live_v3` spelled on
/// `(expiry_daa, settled_at_final)` — the DAA clock, or the second clock bounded per obligation at
/// `2 × window_court` past the DAA expiry ([`palw_second_clock_holds_v1`]). `escaped_depth` is the
/// depth AFTER the liveness escape. The same two clocks as the lock (V-4, v1 decision 4); pinned
/// equal to `is_live_v3` by `the_row_lock_predicate_is_the_locks`.
pub fn palw_vesting_lock_is_live_v1(
    expiry_daa: u64,
    settled_at_final: u64,
    now_daa: u64,
    settled_now: u64,
    escaped_depth: Option<u64>,
    window_court: u64,
) -> bool {
    now_daa < expiry_daa || palw_second_clock_holds_v1(escaped_depth, settled_now, settled_at_final, expiry_daa, now_daa, window_court)
}

/// **V-4(c)'s seam — M3 fills it** (ADR-0152 DA-5): whether a DA session is open on `claim_id`.
/// `false` until M3's `da_sessions` / `da_claims` land (the v22 layout appends them from delta 74);
/// M3 replaces this body with its own lookup. Past DA-5 a session re-keys the row at opening to at
/// least its deadline + `window_challenge_at`, so (c) is implied by (a) and
/// [`palw_vesting_row_maturity_v1`] asserts the implication (debug) rather than relying on it — and
/// still refuses to mature a row this says is under a session.
pub fn palw_vesting_row_has_open_da_session_v1(_state: &PalwChainStateV2, _claim_id: &Hash64) -> bool {
    false
}

/// **V-4: is `row` mature at `now_daa`?** (a) the lock predicate is spent at the escaped depth
/// ([`palw_vesting_lock_is_live_v1`]), (b) the chain is not in a licence halt
/// ([`palw_chain_vesting_halted_v1`]), (c) no DA session is open on the claim — or the row is
/// already latched, since a later re-arm can never un-mature it (X29). The fold's step 3d latches
/// exactly the rows this says are mature and not yet latched; RPC calls the same function.
///
/// **Mainnet Decision A is not a term here and must never become one** (V-4's box): a row is PALW
/// state, not a coinbase output. Its minted output obeys Decision A unchanged; the row itself
/// matures on the lock's two clocks alone.
pub fn palw_vesting_row_maturity_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    row: &PalwVestingRowV1,
    now_daa: u64,
    raw_depth: Option<u64>,
) -> PalwVestingMaturityV1 {
    let window_court = params.window_court();
    let settled_now = state.settled_attempt_finals();
    let escaped = palw_second_clock_depth_v1(raw_depth, state.recent_anchor_daas(), now_daa, window_court);
    let halted = raw_depth.is_some() && escaped.is_none();
    let lock_live = palw_vesting_lock_is_live_v1(row.expiry_daa, row.settled_at_final, now_daa, settled_now, escaped, window_court);
    let da_session_open = palw_vesting_row_has_open_da_session_v1(state, &row.claim_id);
    debug_assert!(
        row.matured_at.is_some() || !da_session_open || lock_live,
        "V-4(c) ⇒ (a): an open DA session re-keys its row past the session (DA-5), so the lock predicate holds it"
    );
    PalwVestingMaturityV1 {
        matured_at: row.matured_at,
        daa_clock_met: now_daa >= row.expiry_daa,
        licences_since_final: settled_now.saturating_sub(row.settled_at_final),
        licences_needed: raw_depth,
        second_clock_bound_daa: raw_depth.map(|_| row.expiry_daa.saturating_add(window_court.saturating_mul(2))),
        halted,
        da_session_open,
        mature_now: row.matured_at.is_some() || (!lock_live && !halted && !da_session_open),
    }
}

fn palw_reporter_move_v1(offence_key: &Hash64, payout: &PalwPayoutV2) -> PalwVestingMoveV1 {
    PalwVestingMoveV1 {
        source: PalwVestingSourceV1::Reporter { offence_id: *offence_key },
        legs: vec![PalwVestingLegV1 {
            kind: PalwVestingLegKindV1::Reporter,
            // `reporter_rewards` stores the payload fixed at conviction, not the bond (row 11); the
            // bond is named by S-7's `ReporterAwarded` note.
            payee_bond: None,
            payload: payout.payload,
            amount: payout.amount,
            queue_key: Some(palw_reporter_payout_key_v1(offence_key)),
        }],
    }
}

fn palw_row_move_v1(row: &PalwVestingRowV1) -> PalwVestingMoveV1 {
    PalwVestingMoveV1 { source: PalwVestingSourceV1::Row { claim_id: row.claim_id }, legs: row.legs().collect() }
}

/// **The matured-move iterator** (phase2-plan §2.4; ADR-0152 V-7). Yields, in order:
/// 1. every `reporter_rewards` entry as a one-leg `Reporter` move, in key order (S-7 writes them at
///    step 2; 3d moves them first);
/// 2. then the vesting rows with `matured_at` set, in `(expiry_daa, claim_id)` order —
///
/// and ENDS at the first row whose `matured_at` is `None`, even if later rows are latched. Latched
/// rows behind an unlatched head are possible (latching reads each row's settled clock while the
/// order is by expiry), and stop-never-skip is structural here rather than a rule the caller
/// could forget.
pub fn palw_vesting_matured_moves_v1(state: &PalwChainStateV2) -> impl Iterator<Item = PalwVestingMoveV1> + '_ {
    palw_vesting_moves_while_v1(state, |row| row.matured_at.is_some())
}

/// [`palw_vesting_matured_moves_v1`] with "is this row latched?" asked of `latched` rather than read
/// off `matured_at`: the fold's plan asks the row (its latch has run), the next-block question
/// ([`palw_vesting_next_block_plan_v1`]) asks what the next block's latch will write.
fn palw_vesting_moves_while_v1<'s, F>(state: &'s PalwChainStateV2, latched: F) -> impl Iterator<Item = PalwVestingMoveV1> + 's
where
    F: Fn(&PalwVestingRowV1) -> bool + 's,
{
    let reporters = state.reporter_rewards_iter().map(|(offence_key, payout)| palw_reporter_move_v1(offence_key, payout));
    let rows = state.vesting_iter_by_expiry().take_while(move |row| latched(row)).map(palw_row_move_v1);
    reporters.chain(rows)
}

/// Market rows waiting in `state`'s queue (`0xFF` keys), counted up to
/// [`PALW_V2_VESTING_MARKET_RESERVE`] — all V-7's reserve asks. **The fold's count**: step 3d
/// passes it on the state it plans, after step 1b's drain and the market's own writers (3′, 3c).
/// The non-market prefix is walked first; it is at most a block's moves.
pub fn palw_vesting_market_rows_waiting_v1(state: &PalwChainStateV2) -> usize {
    state
        .pending_payouts_iter()
        .filter(|(key, _)| key.as_byte_slice()[0] == PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX)
        .take(PALW_V2_VESTING_MARKET_RESERVE)
        .count()
}

/// **The same count as the NEXT block's step 3d will see it, read off a committed state**
/// ([`palw_vesting_next_block_plan_v1`]'s input): the market rows left once the next block's step
/// 1b has drained the first [`PALW_V2_MAX_PAYOUTS_PER_BLOCK`] keys, counted up to
/// [`PALW_V2_VESTING_MARKET_RESERVE`]. Market rows the next block's own objects write are not
/// known to anybody before it, and are not counted.
pub fn palw_vesting_market_rows_waiting_after_drain_v1(state: &PalwChainStateV2) -> usize {
    state
        .pending_payouts_iter()
        .skip(PALW_V2_MAX_PAYOUTS_PER_BLOCK)
        .filter(|(key, _)| key.as_byte_slice()[0] == PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX)
        .take(PALW_V2_VESTING_MARKET_RESERVE)
        .count()
}

/// Non-market rows waiting in the queue — every key below the market's `0xFF`. **Zero at every
/// step 3d on testnet-12** (the queue lemma: the parent's non-market part is ≤ 8 and the 1b drain
/// takes the first eight keys; past `palw_rcore_plus` nothing but 3d writes a non-market key), so
/// the fold's belt that subtracts it from the budget it passes never binds while the lemma holds.
/// On a committed state it is the keys the last step 3d wrote (≤ 8), which the next drain takes.
pub fn palw_vesting_non_market_rows_waiting_v1(state: &PalwChainStateV2) -> usize {
    state.pending_payouts_iter().take_while(|(key, _)| key.as_byte_slice()[0] != PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX).count()
}

/// **V-7's budget for one block's moves, in new queue keys** (ADR-0152 v3.1 V-7, the market
/// reserve): `budget_new_keys − min(2, market_waiting)`. Pure in its two arguments, so a 6-key row
/// fits whenever `budget_new_keys` is the full [`PALW_V2_VESTING_LEGS_PER_BLOCK`] (6 ≤ 8 − 2).
pub fn palw_vesting_budget_v1(budget_new_keys: usize, market_waiting: usize) -> usize {
    budget_new_keys.saturating_sub(market_waiting.min(PALW_V2_VESTING_MARKET_RESERVE))
}

/// **Step 3d's plan, pure** (phase2-plan §2.4; ADR-0152 v3.1 V-7 and §7.3's IMPL-10 signature
/// `palw_vesting_mint_plan_v1(state, budget_new_keys, market_waiting)`). Takes whole moves from
/// [`palw_vesting_matured_moves_v1`] while their new keys fit
/// [`palw_vesting_budget_v1`]`(budget_new_keys, market_waiting)`; stops with `BudgetFull` at the
/// first move that does not fit, and with `NotLatched` / `Empty` when the iterator ends.
///
/// **It reads the rows, their latches and the reporter rewards — never the queue** (review of the
/// vesting work, finding 1): the queue enters only through the caller's two numbers, and a row is
/// "latched" iff its `matured_at` is set. **Which to call:**
/// * **the fold's step 3d calls this**, on the state it plans — after 1b's drain, 3′, 3c and this
///   block's latch — with `budget_new_keys` = `8 − palw_vesting_non_market_rows_waiting_v1` (the
///   belt; 8 under the queue lemma) and `market_waiting` = [`palw_vesting_market_rows_waiting_v1`];
/// * **the RPC and the Phase 2 coinbase harness call [`palw_vesting_next_block_plan_v1`]**, on a
///   committed state, for "what moves next block". Handed a committed state, THIS function answers
///   a question no block asks: it sees the keys the last step 3d wrote (which the next drain takes)
///   and only the rows latched already (not the ones the next block's latch will add).
///
/// **The head always moves when the caller passes the full width** (a liveness belt beside V-7's
/// "a 6-key row always fits"): a row wider than the budget — more than five credited seats, which
/// no testnet-12 panel has (`PALW_V2_PANEL_SEATS` = 5) — would otherwise stall every row behind it
/// forever. Taken only as the plan's first move and only when `budget_new_keys` is the full
/// [`PALW_V2_VESTING_LEGS_PER_BLOCK`], which the fold passes only when the queue holds no
/// non-market row; so it moves such a row at most every other block and the non-market part stays
/// below `8 + the widest row`.
pub fn palw_vesting_mint_plan_v1(state: &PalwChainStateV2, budget_new_keys: usize, market_waiting: usize) -> PalwVestingMintPlanV1 {
    palw_vesting_plan_while_v1(state, budget_new_keys, market_waiting, |row| row.matured_at.is_some())
}

/// [`palw_vesting_mint_plan_v1`]'s rule over [`palw_vesting_moves_while_v1`]`(state, latched)`.
fn palw_vesting_plan_while_v1<F>(
    state: &PalwChainStateV2,
    budget_new_keys: usize,
    market_waiting: usize,
    latched: F,
) -> PalwVestingMintPlanV1
where
    F: Fn(&PalwVestingRowV1) -> bool,
{
    let budget = palw_vesting_budget_v1(budget_new_keys, market_waiting);
    let head_may_overflow = budget_new_keys >= PALW_V2_VESTING_LEGS_PER_BLOCK;
    let mut plan =
        PalwVestingMintPlanV1 { moves: Vec::new(), legs: 0, new_keys: 0, stopped: PalwVestingStopV1::Empty, stopped_at: None };
    let mut created: BTreeSet<Hash64> = BTreeSet::new();
    let mut rows_taken = 0usize;
    for next in palw_vesting_moves_while_v1(state, &latched) {
        let keys = next.new_keys(&created);
        let fits = plan.new_keys + keys.len() <= budget || (plan.moves.is_empty() && head_may_overflow);
        if !fits {
            plan.stopped = PalwVestingStopV1::BudgetFull;
            plan.stopped_at = Some(next.source.clone());
            return plan;
        }
        plan.new_keys += keys.len();
        plan.legs += next.leg_count();
        created.extend(keys);
        if matches!(next.source, PalwVestingSourceV1::Row { .. }) {
            rows_taken += 1;
        }
        plan.moves.push(next);
    }
    // The iterator ended: at an unlatched row, or with nothing left.
    if let Some(head) = state.vesting_iter_by_expiry().nth(rows_taken) {
        debug_assert!(!latched(head), "the iterator ends only at an unlatched row");
        plan.stopped = PalwVestingStopV1::NotLatched;
        plan.stopped_at = Some(PalwVestingSourceV1::Row { claim_id: head.claim_id });
    }
    plan
}

/// **What the next block's step 3d will move, asked of a committed state** (phase2-plan §2.4: the
/// RPC's "what moves next block" and the T03/T58 coinbase harness; review of the vesting work,
/// finding 1). **The function to call on a committed state.** It replays, on `state` as committed
/// and without cloning it, what stands between that state and the next block's plan:
/// 1. **step 1b's drain**: the first [`PALW_V2_MAX_PAYOUTS_PER_BLOCK`] keys leave. So the width is
///    `8 − (non-market keys past them)` (8 under the queue lemma: the keys the last 3d wrote are
///    all drained), and the market count is the rows that drain leaves
///    ([`palw_vesting_market_rows_waiting_after_drain_v1`]);
/// 2. **step 3d's latch at `next_daa`**: a row counts as latched if it is already, or if
///    [`palw_vesting_row_maturity_v1`] calls it mature at `next_daa` and `raw_depth`. Those are
///    exactly the rows the fold's latch walk writes `matured_at` on
///    (`the_latch_walk_latches_exactly_the_rows_v4_calls_mature` pins the walk to that function).
///    Each row is asked in place, and only the rows the plan reaches are asked;
/// 3. **the plan**: [`palw_vesting_mint_plan_v1`]'s rule over those rows.
///
/// `raw_depth` is what the next block's extras will carry (`palw_settled_anchor_depth_v1`: the
/// configured depth past `palw_audit_2026_09_23`, `None` below it), the raw depth every other
/// reader of the second clock takes (I-8). Below `palw_rcore_plus` at `next_daa` the next block
/// has no step 3d and the plan is empty.
///
/// **It equals the next fold's plan (moves, legs, keys and stop) whenever the next block** settles
/// no anchor, writes no market row, awards no reporter reward, burns or re-keys no row (S-4's
/// convictions, M3's DA-5 sessions), and finalizes no claim whose new row the plan reaches. Those
/// are the only inputs to step 3d that the block itself can move, and only its objects and sweeps
/// move them, which the committed state cannot know. On a chain of empty blocks the two agree
/// block after block (`the_next_block_plan_is_the_next_folds_plan_over_a_simulated_chain`).
pub fn palw_vesting_next_block_plan_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    next_daa: u64,
    raw_depth: Option<u64>,
) -> PalwVestingMintPlanV1 {
    if !params.rcore_plus_active_at(next_daa) {
        return PalwVestingMintPlanV1 { moves: Vec::new(), legs: 0, new_keys: 0, stopped: PalwVestingStopV1::Empty, stopped_at: None };
    }
    let non_market_after_drain = palw_vesting_non_market_rows_waiting_v1(state).saturating_sub(PALW_V2_MAX_PAYOUTS_PER_BLOCK);
    palw_vesting_plan_while_v1(
        state,
        PALW_V2_VESTING_LEGS_PER_BLOCK.saturating_sub(non_market_after_drain),
        palw_vesting_market_rows_waiting_after_drain_v1(state),
        |row| palw_vesting_row_maturity_v1(state, params, row, next_daa, raw_depth).mature_now,
    )
}

/// **Where a row stands in V-7's order** (phase2-plan §2.3, for RPC; restated in keys by ADR-0152
/// §7.3, IMPL-10): `(moves ahead, keys ahead)` — every reporter reward (one key each), then every
/// row before it in `(expiry_daa, claim_id)` order, each at [`PalwVestingRowV1::key_count`], its
/// cost as a block's first move. `None` when the claim has no row. A lower bound on its wait in
/// moves, an upper bound on the keys the moves ahead spend: the rows ahead must latch and fit
/// first, and rows sharing a seat payee in one block share its key.
pub fn palw_vesting_mint_position_v1(state: &PalwChainStateV2, claim_id: &Hash64) -> Option<(usize, usize)> {
    palw_vesting_mint_positions_v1(state, &BTreeSet::from([*claim_id])).remove(claim_id)
}

/// [`palw_vesting_mint_position_v1`] for several rows in ONE walk of V-7's order (the RPC's page of
/// rows, phase2-plan §5.7's read-side cost): the walk stops at the last row asked for, so a page
/// costs one pass over the rows ahead of it rather than one pass per row. A claim with no row is
/// absent from the answer. The single-row form is this function, so the two cannot disagree.
pub fn palw_vesting_mint_positions_v1(
    state: &PalwChainStateV2,
    claims: &BTreeSet<Hash64>,
) -> std::collections::BTreeMap<Hash64, (usize, usize)> {
    let mut out = std::collections::BTreeMap::new();
    if claims.is_empty() {
        return out;
    }
    let mut moves = 0usize;
    let mut keys = 0usize;
    for (_, payout) in state.reporter_rewards_iter() {
        moves += 1;
        keys += usize::from(payout.amount > 0);
    }
    for row in state.vesting_iter_by_expiry() {
        if claims.contains(&row.claim_id) {
            out.insert(row.claim_id, (moves, keys));
            if out.len() == claims.len() {
                break;
            }
        }
        moves += 1;
        keys += row.key_count();
    }
    out
}

/// **V-3's consistency, the rows' own half** (the fold's debug invariant at the end of step 3d):
/// every row's amount is at most `escrowed_reward − buyback_bound`, and
/// `created = Σ live rows + moved + burned`. Both are facts the vesting writers alone keep.
pub fn palw_vesting_counters_consistent_v1(state: &PalwChainStateV2) -> Result<(), String> {
    let mut live = 0u128;
    for row in state.vesting_iter_by_expiry() {
        let total = row.total_sompi_u128();
        live += total;
        let cap = row.escrowed_reward.saturating_sub(row.buyback_bound) as u128;
        if total > cap {
            return Err(format!(
                "row {} holds {total} > escrowed {} − buyback {}",
                row.claim_id, row.escrowed_reward, row.buyback_bound
            ));
        }
    }
    let c = state.vesting_counters();
    if c.created != live + c.moved + c.burned {
        return Err(format!("created {} != live {live} + moved {} + burned {}", c.created, c.moved, c.burned));
    }
    Ok(())
}

/// **V-3's consistency, whole** (a test invariant): [`palw_vesting_counters_consistent_v1`], and a
/// row exists only for a claim that is `Final` or retired (no claim record). The second half is a
/// fact about S's conviction funnel too — a Final it reverses must burn its row
/// (`burn_vesting_row`) — so the fold asserts only the first half.
pub fn palw_vesting_consistency_v1(state: &PalwChainStateV2) -> Result<(), String> {
    palw_vesting_counters_consistent_v1(state)?;
    for row in state.vesting_iter_by_expiry() {
        if let Some(claim) = state.claim(&row.claim_id)
            && !matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
        {
            return Err(format!("row {} names a claim that is not Final ({:?})", row.claim_id, claim.phase));
        }
    }
    Ok(())
}

/// The vesting notes one block's delta journaled (phase2-plan §2.5), in application order.
pub fn palw_vesting_notes_of_delta_v1(delta: &PalwStateDeltaV2) -> impl Iterator<Item = &PalwVestingNoteV1> {
    delta.entries.iter().filter_map(|entry| match entry {
        PalwDeltaEntryV2::VestingNote(note) => Some(note),
        _ => None,
    })
}

/// Σ `Burned` + `ShareBurned` sompi one block's delta journaled (phase2-plan §2.5).
pub fn palw_vesting_burned_by_delta_v1(delta: &PalwStateDeltaV2) -> u64 {
    palw_vesting_notes_of_delta_v1(delta).fold(0u64, |sum, note| match note {
        PalwVestingNoteV1::Burned { sompi, .. } | PalwVestingNoteV1::ShareBurned { sompi, .. } => sum.saturating_add(*sompi),
        _ => sum,
    })
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
            // Appended by the vesting work (V-3's identity notes).
            (5, PalwVestingNoteV1::BuybackAtFinal { claim_id: r.claim_id, sompi: 3 }),
            (6, PalwVestingNoteV1::ReserveCredited { claim_id: r.claim_id, sompi: 4 }),
        ];
        for (tag, note) in notes {
            let bytes = borsh::to_vec(&note).unwrap();
            assert_eq!(bytes[0], tag, "{note:?}");
            assert_eq!(borsh::from_slice::<PalwVestingNoteV1>(&bytes).unwrap(), note);
        }
        let source = PalwVestingSourceV1::Reporter { offence_id: Hash64::from_bytes([1; 64]) };
        assert_eq!(borsh::from_slice::<PalwVestingSourceV1>(&borsh::to_vec(&source).unwrap()).unwrap(), source);
    }

    /// **A-KEY's construction, restated**: BLAKE2b-512 keyed by the domain over the id, byte 0
    /// forced to `0x00` — the state module's `keyed`/`finish` spelling — and the row's legs carry
    /// exactly those keys, the seat legs the panel's payee key, the reserve none.
    #[test]
    fn a_key_is_the_keyed_hash_with_byte_0_forced_and_the_legs_carry_it() {
        let spell = |domain: &[u8], id: &Hash64| {
            let mut bytes = [0u8; 64];
            bytes.copy_from_slice(blake2b_simd::Params::new().hash_length(64).key(domain).hash(id.as_byte_slice()).as_bytes());
            bytes[0] = 0x00;
            Hash64::from_bytes(bytes)
        };
        let r = row();
        assert_eq!(palw_vesting_payout_key_v1(&r.claim_id), spell(PALW_STATE_V2_DOMAIN_VESTING_PAYOUT, &r.claim_id));
        assert_eq!(palw_reporter_payout_key_v1(&r.claim_id), spell(PALW_STATE_V2_DOMAIN_REPORTER_PAYOUT, &r.claim_id));
        let legs: Vec<PalwVestingLegV1> = r.legs().collect();
        assert_eq!(legs.len(), 1 + r.seats.len() + 1);
        assert_eq!(legs[0].queue_key, Some(palw_vesting_payout_key_v1(&r.claim_id)));
        assert_eq!(legs[1].queue_key, Some(palw_panel_payout_key_v1(&r.seats[0].1.payload)));
        assert_eq!((legs[3].kind, legs[3].queue_key, legs[3].amount), (PalwVestingLegKindV1::Reserve, None, r.reserve));
        assert_eq!(r.total_sompi(), 11 + 14 + 17 + 18);
        assert_eq!(r.leg_count(), 3);
        assert_eq!(r.payee_bonds().collect::<Vec<_>>(), vec![r.producer_bond, r.seats[0].0, r.seats[1].0]);
    }
}
