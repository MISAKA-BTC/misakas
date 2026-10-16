//! V2 panel derivation and receipt quorum — the acceptance side of ADR-0042 Decisions 2 and 7
//! (PR-06), over the candidate-scoped state PR-03 built and the lattice edges it already walks.
//!
//! The division of labor: `palw_state_v2` OWNS the lattice (what a `PanelBound` or
//! `ReceiptLicensed` object DOES to a claim); this module owns whether such an object may be
//! ACCEPTED at all — is this panel the one the anchor derives, is this quorum real. Both read
//! only the candidate chain's own state, per the P0-4 discipline.
//!
//! ## The draw (Decision 7, closing P0-7's wiring half)
//!
//! P0-7 was never the panel module — it was the CALLER passing
//! `executor_bond_outpoint.transaction_id` where candidates carried `validator_pubkey_hash`:
//! two namespaces, so the executor was never actually excluded. Here the exclusion facts come
//! from the same bond registry the seats do, so there is no second namespace to diverge into:
//! a candidate is excluded if it IS the executor's bond, if it carries the executor's
//! **operator id** (splitting collateral across bonds must not manufacture seats — operator_id
//! is a required registration field), or if it holds the executor's **key**.
//!
//! The draw is a deterministic sortition: every eligible bond gets the ticket
//! `H(anchor ‖ claim ‖ bond)`, tickets sort ascending, one seat per operator, first
//! `seat_count` win. The anchor is a chain block chosen by the chain — the first block at
//! `accepted_daa + anchor_delay` — so it exists only after the attempt was fixed, and neither
//! the executor nor the binder can grind it.
//!
//! **Past the panel-economy fence the lottery is one entry per OPERATOR (ADR-0130).** Below it an
//! operator holding ten eligible bonds held ten tickets and kept the best, so splitting collateral
//! across bonds bought draws even though it could never buy a second seat. Past it each operator's
//! candidate is its eligible bond with the lowest bond ticket, and the operator draws ONE ticket
//! `H(operator-ticket domain ‖ anchor ‖ claim ‖ operator_id)` — a function of who the operator is,
//! never of how many bonds it holds ([`palw_panel_operator_lottery_v1`]).
//!
//! **Past `palw_rcore_plus` the lottery is a stake-weighted race (ADR-0152 v3.1 SW, testnet-12).**
//! `PalwPanelDrawPolicyV1::stake` is `Some`: every eligible operator keeps its one entry and its
//! candidate bond, but its key is `−log2(u) / W`, `W` its posted collateral in whole MSK capped at
//! 1,000,000, and the smallest keys sit — successive sampling without replacement
//! ([`palw_panel_stake_race_of_v1`]). ADR-0147's outsider races the same way under its own domain; its
//! admission jury does not (SW-A4). A draw whose eligible operators weigh less than 875‰ of the
//! base does not bind (SW-10). `stake: None` is the lottery above, byte for byte.
//!
//! ## Receipts and the four DA states (Decision 7, the DA half of P0-8's wiring)
//!
//! A seat answers with a signed verdict: **`Valid`** (the trace opened and verified) or
//! **`Unavailable`** (the producer did not serve the data). The two quorums license OPPOSITE
//! transitions — a `Valid` quorum licenses `ReceiptLicensed`; an `Unavailable` quorum justifies
//! `ProducerDefaulted` (claim void, Decision 7's "silence can never pin a block at Provisional
//! forever") — and keeping them distinct verdicts under one signing context is what lets a
//! panel member report withheld data without being punished as a no-show, and without a
//! producer's silence reading as the panel's.
//!
//! A seat that signs NEITHER by the receipt deadline is the no-show: the claim voids by the
//! lattice's `ReceiptTimeout` sweep, and the panel record beside the voided claim names exactly
//! who owed a verdict — the chain-scoped fact ADR-0042 requires for the no-show penalty, whose
//! collateral consequence is the slash machinery's (PR-07/PR-09), not this module's.

use crate::BlockHash;
use crate::Hash64;
use crate::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwBondStateV2, PalwChainStateV2, PalwClaimPhaseV2, PalwPanelSeatV2, PalwStateParamsV2,
};
use blake2b_simd::Params;

pub const PALW_PANEL_V2_DOMAIN_SEAT_TICKET: &[u8] = b"misaka-palw/panel-v2/seat-ticket/v1";
/// **ADR-0130: the operator's lottery entry.** Past the panel-economy fence an operator draws one
/// ticket under this domain, over the anchor, the claim and its operator id — never over a bond.
pub const PALW_PANEL_V2_DOMAIN_OPERATOR_TICKET: &[u8] = b"misaka-palw/panel-v2/operator-ticket/v1";
/// **ADR-0147: the outsider seat's lottery entry.** An outsider-judged claim draws its first seat
/// from the network's base-class population under THIS domain, so an operator's outsider ticket is
/// independent of its ticket in the class's own draw — a single domain would hand the outsider seat
/// to whichever operator already led the class draw.
pub const PALW_PANEL_V2_DOMAIN_OUTSIDER_TICKET: &[u8] = b"misaka-palw/panel-v2/outsider-ticket/v1";
/// **ADR-0147: the admission jury's lottery entry** — the jury a `Candidate` class meets at an
/// audit boundary, drawn from the same network population under its own domain.
pub const PALW_PANEL_V2_DOMAIN_ADMISSION_JURY_TICKET: &[u8] = b"misaka-palw/panel-v2/admission-jury-ticket/v1";
/// **ADR-0152 SW-3: an operator's entry in the stake-weighted race for a claim's class seats.**
/// `u` is the first eight bytes (little-endian) of `H(this domain ‖ anchor ‖ claim ‖ operator_id)`
/// and the key is `−log2((u + 1) / 2^64) / W` ([`palw_draw_neg_log2_q64_v1`]). A domain of its own,
/// not ADR-0130's operator ticket reused: the two draws must never be the same function of the
/// same inputs, or a fence-crossing replay could not tell which one a panel came from.
pub const PALW_PANEL_V2_DOMAIN_STAKE_TICKET: &[u8] = b"misaka-palw/panel-v2/stake-ticket/v1";
/// **ADR-0152 SW-3/SW-5: the ADR-0147 outsider's entry in the stake-weighted race**, under its own
/// domain for ADR-0147's reason — an operator's outsider key must be independent of its key in the
/// class's own draw. There is no stake-jury domain: the admission jury is NOT weighted (SW-A4), and
/// [`palw_admission_jury_v1`] stays ADR-0147's.
pub const PALW_PANEL_V2_DOMAIN_STAKE_OUTSIDER_TICKET: &[u8] = b"misaka-palw/panel-v2/stake-outsider-ticket/v1";
/// **The panel seed past `Params::palw_panel_seed_execution` (lane F1, the post-launch fence for the
/// 2026-09-25 anchor-identity re-roll):** `seed = H(this domain ‖ the anchor ATTEMPT's execution
/// commitment ‖ claim)` ([`palw_panel_draw_seed_v1`]). Hashed into `consensus_params_id` beside the
/// fence's height (the rule's version rides its fence), so two builds that arm the fence at one height
/// with different seed rules announce different rulesets; a build that leaves it dormant hashes
/// nothing new.
pub const PALW_PANEL_V2_DOMAIN_DRAW_SEED_V1: &[u8] = b"misaka-palw/panel-v2/draw-seed/v1";
/// **C-02 (mainnet audit 2026-09-11 deep fence): the ceiling on a bond's stake-weighted
/// sub-tickets.** Past the fence a bond draws `floor(collateral / min_collateral)` sub-tickets, but
/// a premine-scale bond would otherwise mint hundreds of thousands (t11's `min_collateral` is
/// 400,000 sompi, so a 1,000-MSK bond is ~250,000 sub-tickets) and every node recomputes the draw
/// at each `PanelBound` — an unbounded per-draw cost a malicious high-collateral registrant could
/// weaponize. The count is capped here, so a bond at or above `cap × min_collateral` draws the
/// maximum weight and no more: it bounds the recompute at `cap` hashes per eligible bond while
/// still giving stake a wide (4096×) advantage. A pure code constant — it enters no fingerprint —
/// and it only bites past the fence, where the whole weighted draw is fenced. Tunable if the
/// weighting granularity ceiling proves too low for a network's collateral distribution.
pub const PALW_V2_MAX_SEAT_TICKETS_PER_BOND: u64 = 4096;

/// **How a panel is drawn at a claim's anchor, as one value** — resolved by the processor at the
/// ANCHOR (where every rule that decides a panel is resolved) and carried to the draw and to the
/// acceptance layer alike, so build and validate recompute one identical panel.
///
/// `weighted` is C-02's stake-weighted sortition (`Params::palw_audit_2026_09_11_deep`); `economy`
/// is `Some` past ADR-0124's `Params::palw_panel_economy` and carries the seat floor, the exposure
/// ceiling and ADR-0130's reward multiple the eligibility predicate reads. **Past the panel-economy
/// fence the draw is one lottery entry per eligible OPERATOR whatever `weighted` says** (ADR-0124
/// Decision 5, ADR-0130): once a seat's risk is the exposure it reserves and a bond is eligible only
/// while its free collateral covers it, stake no longer needs to buy probability — it buys the
/// capacity to hold more seats at once — and splitting it across bonds buys nothing.
///
/// A caller may build one with a hypothetical `economy` (a shadow reader's `λ`) and hand it to
/// [`derive_panel_v2_with_policy`] or [`palw_panel_eligible_bonds_v2`] against a state snapshot:
/// both are pure and write nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwPanelDrawPolicyV1 {
    pub weighted: bool,
    pub economy: Option<crate::palw_panel_economy_v1::PalwSeatEconomyV1>,
    /// ADR-0135: under the registry a seat judges a class by a fresh possession proof, not by its
    /// declaration (`palw_bond_may_judge_class_v4`); `None` below the fence.
    pub readiness: Option<crate::palw_model_registry_v1::PalwReadinessPolicyV1>,
    /// **ADR-0147: independence by population** (`Params::palw_admission_independence`). `None`
    /// where the fence is not configured, which is byte-identical to the draw before it existed.
    pub independence: Option<PalwPanelIndependenceV1>,
    /// **The bind's Valid-lock question, asked at the draw** (the 2026-09-23 route-matrix audit's
    /// #3). `None` where the lock ledger is not armed at the binding block — byte-identical to the
    /// draw before it existed.
    pub valid_lock: Option<PalwPanelValidLockV1>,
    /// **ADR-0152 SW: the stake-weighted draw** (Q4, v3.1), resolved at the claim's ANCHOR by
    /// `palw_panel_draw_policy_at`: `Some` iff `Params::palw_rcore_plus` is active at the anchor DAA.
    /// `None` is the draw before it, byte for byte — ADR-0130's operator lottery and ADR-0147's
    /// outsider ticket — on testnet-11, devnet and mainnet. The admission jury never reads it
    /// (SW-A4: the jury stays ADR-0147's).
    pub stake: Option<PalwPanelStakeDrawV1>,
}

/// **ADR-0152 SW-2: an operator's weight is capped here**, in whole MSK. Above it an operator gains
/// nothing by staying whole (SW-A5), and one heavy ready operator cannot cut a class's
/// effective-ready count below what the cap allows (SW-A6).
pub const PALW_DRAW_WEIGHT_CAP_MSK_V1: u64 = 1_000_000;

/// **ADR-0152 SW-10: the eligible-stake floor**, in permille of the base weight. A draw whose
/// eligible operators weigh less than this share of every operator that could sit (Active, at the
/// floor, registered before the anchor, capable) refuses with `InsufficientEligibleStake`: a
/// saturated honest population halts binding instead of leaving the seats to idle Sybils.
pub const PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1: u16 = 875;

/// **ADR-0152 SW-2/SW-3: the arithmetic bound on a weight**, `2^40` whole MSK — not a policy value.
/// The key comparison multiplies `L ≤ 2^70` by a weight in `u128`, so a weight must stay below
/// `2^58`; `2^40` is the bound §3.14 states (the 10B supply cap is below `2^34`, so it never binds,
/// and [`PALW_DRAW_WEIGHT_CAP_MSK_V1`] binds long before it). Applied under whatever cap a policy
/// carries, so a hand-built policy with an absurd cap cannot overflow the comparison.
pub const PALW_DRAW_WEIGHT_MAX_MSK_V1: u64 = 1 << 40;

/// **ADR-0152 SW (v3.1): the stake-weighted draw's terms**, carried on [`PalwPanelDrawPolicyV1`].
/// Not Borsh and not state — a policy value, resolved at the anchor like every other field, so the
/// policy stays `Copy`.
///
/// Where it is `Some`, each eligible operator's lottery entry is weighted by its one bond's POSTED
/// collateral in whole MSK, capped at `weight_cap_msk` (operator ids are unique on testnet-12, so
/// an operator is one bond): the key is `L / W` with `L = −log2((u + 1) / 2^64)` from an integer
/// routine, and the smallest keys sit — successive sampling without replacement. The Valid lock
/// still decides WHETHER a bond is drawn: past `palw_rcore_plus`, where the processor arms this
/// draw, through S-3's one-ledger seat filter (`PalwPanelValidLockV1::rcore`, L-4b), and the
/// panel economy's headroom (ADR-0124 Decision 4) is not asked — only a policy without that
/// filter asks it (`Eligible { headroom: true }`). Posted stake decides HOW OFTEN (SW-2).
/// ADR-0147's outsider seat is weighted the same way under its own domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelStakeDrawV1 {
    /// SW-2's cap, in whole MSK ([`PALW_DRAW_WEIGHT_CAP_MSK_V1`] on every network that arms it).
    pub weight_cap_msk: u64,
    /// SW-10's floor, in permille of the base weight ([`PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1`]).
    pub eligible_floor_permille: u16,
    /// **Lane PL part E (ADR-0166): the DAA the class seats' availability factor is read at** — `Some(anchor_daa)` where
    /// `Params::palw_seat_availability` is in force at the anchor, `None` everywhere else (the draw before it, byte for byte).
    /// Only the class seats' RACE weight is scaled by it ([`palw_panel_stake_entries_availability_v1`]); SW-10's floor, the
    /// outsider's race and every security weight stay stake-only.
    pub availability_at: Option<u64>,
}

impl PalwPanelStakeDrawV1 {
    /// The terms ADR-0152 v3.1 fixes: the 1,000,000 MSK cap and the 875‰ floor.
    pub const V1: Self = Self {
        weight_cap_msk: PALW_DRAW_WEIGHT_CAP_MSK_V1,
        eligible_floor_permille: PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1,
        availability_at: None,
    };
}

/// **What a seat must be able to lock to be drawn at all**, resolved by the processor for ONE claim
/// at the binding block (the 2026-09-23 route-matrix audit's #3).
///
/// The bind demands that every seat can post the lock one `Valid` signature on the claim takes
/// (`palw_panel_valid_lock_required_v1`: 112.56 MSK for a testnet-12 floor claim), while the draw
/// only asked for the panel floor and the exposure headroom — so a permissionless bond of 0.004 MSK
/// was drawn, failed the bind, and held the claim unbound until its bind window voided it (one
/// 10 MSK bond blocked 17 of 20 floor claims in the audit's probe). Here the draw skips such a bond,
/// with the bind's own expression on the same state: `palw_slashable_available_v1(bond, now_daa,
/// settled_anchor_depth, window_court) >= required` — per lock (`is_live_v3`) and on the ESCAPED
/// depth (2026-09-24 DoS audit), as the fold reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelValidLockV1 {
    /// What one `Valid` signature on this claim must lock, as the binding block's fold computes it.
    pub required: u128,
    /// The binding block's DAA — the clock a lock's liveness is read at.
    pub now_daa: u64,
    /// The second clock's depth at the binding block AFTER the liveness escape
    /// (`palw_second_clock_depth_of_v1`, 2026-09-24 DoS audit), so the draw and the fold's lock
    /// check agree after a stall.
    pub settled_anchor_depth: Option<u64>,
    /// `window_court`, the bound `is_live_v3` puts on the second clock per lock.
    pub window_court: u64,
    /// **ADR-0152 L-4b / SR-7 (S-SPEC §3.3): the one-ledger seat filter** — `Some` where
    /// `Params::palw_rcore_plus` is active at the binding block. Then a bond is drawn iff
    /// `eligibility` fits its work room — `committed + eligibility ≤ collateral × ceiling_permille /
    /// 1000` and `committed + accuser + eligibility ≤ collateral` (the bind's own test) — and
    /// the panel economy's headroom test is not asked; `required` and the 100% `slashable_available`
    /// ledger are not read. The stake-weighted draw (M4) keeps this as its eligibility filter.
    pub rcore: Option<PalwRcoreSeatFilterV1>,
}

/// **ADR-0152 L-4b: what a seat must have room for to be drawn** on one claim — the bind's
/// `max(duty_bind, lock_2)` (`crate::palw_state_v2::palw_rcore_bind_prices_v1`) and the ceiling it
/// is measured under (`fp_max_exposure_ratio_permille`, 500‰ on testnet-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRcoreSeatFilterV1 {
    pub eligibility: u128,
    pub ceiling_permille: u32,
    /// **Lane V02 (post-launch, 2026-09-26): `Params::palw_final_lock_full_collateral` in force at the
    /// binding block** (the bundle's mirror, `final_lock_full_collateral_active_at(now_daa)`). Where
    /// `true` a bond's locks on resolved claims (`palw_bond_resolved_locks_v1`) leave the ceiling and
    /// stay in the 100% term — the bind's own `gate_room`; `false` below the fence and everywhere else.
    pub resolved_locks_off_ceiling: bool,
    /// **Lane V02 (review HIGH): the accuser reserve the relief never spends** —
    /// `palw_bond_accuser_reserve_v1` at the binding block (`0` below the fence), as the bind's
    /// `gate_room` reads it.
    pub accuser_reserve: u128,
    /// **Lane V02 (review MEDIUM): the held-charge floor of the accuser ledger** —
    /// `palw_v02_held_charge_floor_v1` at the binding block: `Some(min_collateral)` past the fence where
    /// the block folds held dissections at their charge (the fold's `accuser_ledger_v1`), `None` below
    /// it (the shipped count, `palw_accuser_exposure_v1`).
    pub held_charge_floor: Option<u64>,
    /// **ADR-0160 stage 3 (F1): the scale `G`'s weight term reads** (the held surplus prices the sessions
    /// a bond challenges at their claims' full weight) — the binding block's params'
    /// (`PalwCapacityGainScaleV1::of`); the identity below F-W / F-L.
    pub gain_scale: crate::palw_weight_cap_v1::PalwCapacityGainScaleV1,
}

impl PalwPanelValidLockV1 {
    /// Whether `bond` can post the lock on `state`.
    ///
    /// A bond whose POSTED collateral is below `required` is refused before its locks are read:
    /// what it has free is at most what it posted, so no lock can change the answer, and the draw
    /// asks this of every eligible bond of every pending claim (the route-matrix re-audit's #3).
    pub fn admits(&self, state: &PalwChainStateV2, bond: &PalwBondKeyV2) -> bool {
        if let Some(filter) = self.rcore {
            // The one invariant's work gate (the S review's M1): the room is the 500‰ ceiling less
            // `committed`, never past `collateral − committed − accuser`, as the bind measures it.
            return self.rcore_room_v1(&filter, state, bond).is_some_and(|room| filter.eligibility <= room);
        }
        let posted = state.bond(bond).map(|b| b.collateral as u128).unwrap_or(0);
        if posted < self.required {
            return false;
        }
        state.palw_slashable_available_v1(bond, self.now_daa, self.settled_anchor_depth, self.window_court) >= self.required
    }

    /// The one-ledger seat filter's work room for `bond` (`None` for a bond the state does not hold) —
    /// [`Self::admits`]'s measure under `filter`, which reads nothing of the claim but
    /// `filter.eligibility`.
    fn rcore_room_v1(&self, filter: &PalwRcoreSeatFilterV1, state: &PalwChainStateV2, bond: &PalwBondKeyV2) -> Option<u128> {
        let record = state.bond(bond)?;
        let committed =
            crate::palw_state_v2::palw_bond_committed_v1(state, bond, self.now_daa, self.settled_anchor_depth, self.window_court);
        // Lane V02: past `palw_final_lock_full_collateral` the resolved locks leave the ceiling,
        // exactly as the bind's `gate_room` reads them (same DAA, same escaped depth).
        let off_ceiling = if filter.resolved_locks_off_ceiling {
            crate::palw_state_v2::palw_bond_resolved_locks_v1(state, bond, self.now_daa, self.settled_anchor_depth, self.window_court)
        } else {
            0
        };
        Some(crate::palw_state_v2::palw_rcore_gate_room_split_of_v1(
            record.collateral,
            filter.ceiling_permille,
            committed,
            off_ceiling,
            filter.accuser_reserve,
            // Lane V02: past the fence the fold's accuser ledger (held dissections at their charge).
            crate::palw_state_v2::palw_accuser_ledger_v1(state, &filter.gain_scale, bond, filter.held_charge_floor),
            crate::palw_state_v2::PalwRcoreGateV1::Work,
        ))
    }

    /// **Lane F2: the lock's claim-independent half** — this lock with its two per-claim prices
    /// (`required`, and the one-ledger filter's `eligibility`) zeroed. Every claim a block binds reads
    /// the same frame (the block's clocks and the filter's terms); two locks with one frame admit a
    /// bond exactly when each one's [`Self::threshold_v1`] is at or below its [`Self::room_v1`].
    pub fn frame_v1(&self) -> Self {
        Self { required: 0, rcore: self.rcore.map(|filter| PalwRcoreSeatFilterV1 { eligibility: 0, ..filter }), ..*self }
    }

    /// Lane F2: the per-claim price [`Self::admits`] compares — the one-ledger filter's `eligibility`
    /// where it is armed, else `required`.
    pub fn threshold_v1(&self) -> u128 {
        self.rcore.map_or(self.required, |filter| filter.eligibility)
    }

    /// **Lane F2: what `bond` has for this lock, whatever the claim** — `admits(state, bond) ==
    /// room_v1(state, bond).is_some_and(|room| threshold_v1() <= room)` for every lock of one frame:
    /// under the one-ledger filter its work room (`None` for a bond the state does not hold), else the
    /// lesser of its posted collateral and its slashable-available amount. Computed once per bond
    /// and anchor block by [`PalwStakeDrawCensusV1`], never per claim.
    pub fn room_v1(&self, state: &PalwChainStateV2, bond: &PalwBondKeyV2) -> Option<u128> {
        if let Some(filter) = self.rcore {
            return self.rcore_room_v1(&filter, state, bond);
        }
        let posted = state.bond(bond).map(|b| b.collateral as u128).unwrap_or(0);
        Some(posted.min(state.palw_slashable_available_v1(bond, self.now_daa, self.settled_anchor_depth, self.window_court)))
    }
}

/// **ADR-0147: what the draw needs to seat an outsider and to fix its population before its
/// randomness.** Resolved by the processor at the claim's ANCHOR like every other policy field.
///
/// It governs a claim only when the claim's own `accepted_daa` is at or past `from_daa` — the
/// fence's height, compared against the claim rather than against the anchor, because the licence
/// rule reads the same height against the same claim and the two must agree about which claims
/// have an outsider ([`crate::palw_state_v2::palw_claim_is_outsider_judged_v1`]).
///
/// For every governed claim, two rules:
///
/// 1. **The population is fixed before the anchor.** A bond may sit only if the chain block that
///    registered it precedes the anchor (`registered_daa < anchor_daa`). The anchor's hash is the
///    draw's randomness, and without this a party that has seen it grinds keys offline for an
///    operator ticket below everyone else's and registers that bond before the binding — a seat for
///    the price of a key search. `Params::palw_bond_maturity` closes the same hole with a window,
///    and it is dormant on every preset; this closes it at the one DAA that matters for sortition,
///    whatever the window says (the two combine as the stricter).
/// 2. **A bought class is judged with an outsider.** The panel's first seat is drawn from the
///    network's base-class population — every bond eligible to judge the liveness floor — under
///    [`PALW_PANEL_V2_DOMAIN_OUTSIDER_TICKET`], and the remaining `seat_count - 1` from the class's
///    own population as before, without the outsider's operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelIndependenceV1 {
    /// `Params::palw_admission_independence`'s height.
    pub from_daa: u64,
    /// The liveness floor: its population is the network's.
    pub base_class_id: Hash64,
    /// The claim's anchor DAA — the draw's randomness, which every seated bond must predate.
    pub anchor_daa: u64,
}

impl PalwPanelIndependenceV1 {
    /// Whether this claim is drawn under ADR-0147 at all (its population cut at the anchor).
    pub fn governs(&self, claim: &crate::palw_state_v2::PalwClaimStateV2) -> bool {
        claim.accepted_daa >= self.from_daa
    }

    /// The registration floor a governed draw applies: the stricter of the maturity window's and
    /// "registered by a chain block before the anchor".
    pub fn registered_by_daa(&self, maturity_floor: Option<u64>) -> u64 {
        let before_anchor = self.anchor_daa.saturating_sub(1);
        maturity_floor.map_or(before_anchor, |floor| floor.min(before_anchor))
    }
}
pub const PALW_RECEIPT_V2_DOMAIN_MESSAGE: &[u8] = b"misaka-palw/receipt-v2/message/v1";
/// ML-DSA-87 signing context for a V2 seat receipt — its own family domain (audit P0-6).
pub const PALW_RECEIPT_V2_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/receipt-v2/mldsa87/v1";
/// ADR-0133 Verification V2: a segment-scoped receipt signs the V2 message and the segment mask.
pub const PALW_RECEIPT_V3_DOMAIN_MESSAGE: &[u8] = b"misaka-palw/receipt-v3/message/v1";
pub const PALW_RECEIPT_V3_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/receipt-v3/mldsa87/v1";

pub const PALW_PANEL_V2_ALL_DOMAINS: &[&[u8]] = &[
    PALW_PANEL_V2_DOMAIN_SEAT_TICKET,
    PALW_PANEL_V2_DOMAIN_OPERATOR_TICKET,
    PALW_PANEL_V2_DOMAIN_OUTSIDER_TICKET,
    PALW_PANEL_V2_DOMAIN_ADMISSION_JURY_TICKET,
    PALW_RECEIPT_V2_DOMAIN_MESSAGE,
    PALW_RECEIPT_V2_MLDSA87_CONTEXT,
    // ADR-0152 SW-3: hashing domains, not ML-DSA contexts (they do not end `mldsa87/v1`), so the
    // committed signature-context set does not move.
    PALW_PANEL_V2_DOMAIN_STAKE_TICKET,
    PALW_PANEL_V2_DOMAIN_STAKE_OUTSIDER_TICKET,
    // The 2026-09-25 panel seed: a hashing domain too.
    PALW_PANEL_V2_DOMAIN_DRAW_SEED_V1,
];

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Panel-side network constants (ADR-0042 Decision 7), constructed only through
/// [`PalwPanelParamsV2::new`] and cross-checked against the state windows by
/// [`PalwPanelParamsV2::validate_against_state_params`] — both feed the Decision 1 startup gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPanelParamsV2 {
    /// Seats per panel.
    seat_count: u16,
    /// Same-verdict signatures required for a quorum, `1 ≤ quorum ≤ seat_count`.
    quorum: u16,
    /// DAA distance from a claim's acceptance to its anchor slot. The anchor is the FIRST chain
    /// block at or past `accepted_daa + anchor_delay`.
    anchor_delay: u64,
}

impl PalwPanelParamsV2 {
    pub fn new(seat_count: u16, quorum: u16, anchor_delay: u64) -> Result<Self, PalwPanelV2Error> {
        if seat_count == 0 {
            return Err(PalwPanelV2Error::InvalidParams("a zero-seat panel judges nothing"));
        }
        if quorum == 0 || quorum > seat_count {
            return Err(PalwPanelV2Error::InvalidParams("quorum must satisfy 1 ≤ quorum ≤ seat_count"));
        }
        // **Audit C5 — the exclusivity invariant.** One `quorum` licenses BOTH directions:
        // `Valid` moves the claim to `ReceiptLicensed`, `Unavailable` voids it as
        // `ProducerDefaulted`. Without a majority requirement the two are simultaneously
        // satisfiable — `seat_count = 4, quorum = 2` lets two seats license while two others
        // default the same claim — and which one happens is decided by the ORDER the counts are
        // checked in `validate_receipt_quorum_v2`, an implementation accident standing in for a
        // rule. Requiring `2·quorum > seat_count` makes the two quorums provably disjoint, so at
        // most one verdict can ever form. This is the discipline `vlt.rs` has carried since its
        // own audit (`quorum_is_strictly_above_two_thirds`); the panel had no analogue.
        if 2 * (quorum as u32) <= seat_count as u32 {
            return Err(PalwPanelV2Error::InvalidParams(
                "quorum must be a strict majority (2·quorum > seat_count), or Valid and Unavailable can both reach quorum at once",
            ));
        }
        if anchor_delay == 0 {
            return Err(PalwPanelV2Error::InvalidParams("a zero anchor delay lets the attempt's own block seed its panel"));
        }
        Ok(Self { seat_count, quorum, anchor_delay })
    }

    pub fn seat_count(&self) -> u16 {
        self.seat_count
    }

    pub fn quorum(&self) -> u16 {
        self.quorum
    }

    pub fn anchor_delay(&self) -> u64 {
        self.anchor_delay
    }

    /// The cross-parameter invariant Decision 1's startup gate must hold: the anchor slot lies
    /// strictly inside the bind window, or every claim voids `BindTimeout` before its panel can
    /// legally exist — a network that finalizes nothing, configured rather than attacked.
    pub fn validate_against_state_params(&self, state_params: &PalwStateParamsV2) -> Result<(), PalwPanelV2Error> {
        // (`window_bind` is a per-network constant; both live in the atomic bundle.)
        if self.anchor_delay >= state_params.window_bind() {
            return Err(PalwPanelV2Error::InvalidParams("anchor_delay must be strictly inside the bind window"));
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwPanelV2Error {
    #[error("invalid panel params: {0}")]
    InvalidParams(&'static str),
    // ADR-0124 Decision 2 — the supplementary door's refusals, by name.
    #[error(
        "claim {0}: no seat is on duty, so no supplementary receipt can be credited (the panel was bound below the panel-economy fence)"
    )]
    NotOnDuty(Hash64),
    #[error("seat {0:?} is already credited on this claim")]
    SeatAlreadyCredited(PalwBondKeyV2),
    #[error("a supplementary receipt set is refused: {0}")]
    SupplementaryRefused(&'static str),
    #[error("claim {0} does not exist at this chain point")]
    MissingClaim(Hash64),
    #[error("claim {claim} is in the wrong phase for {edge}")]
    WrongPhase { claim: Hash64, edge: &'static str },
    #[error("not enough eligible bonds for a panel: need {needed}, found {available} after exclusions and operator dedup")]
    InsufficientEligibleBonds { needed: u16, available: u16 },
    #[error("the anchor fact does not name the claim's anchor slot: {0}")]
    AnchorMismatch(&'static str),
    #[error("panel binding arrived outside its legal window: {0}")]
    BindOutsideWindow(&'static str),
    #[error("the proposed panel is not the one the anchor derives")]
    PanelMismatch,
    #[error("receipt names claim {got}, quorum is being formed for {expected}")]
    ReceiptClaimMismatch { got: Hash64, expected: Hash64 },
    #[error("no panel is bound for claim {0}")]
    NoPanel(Hash64),
    #[error("receipt signer {0:?} holds no seat on this panel")]
    NotASeat(PalwBondKeyV2),
    #[error("seat {0:?} answered more than once")]
    DuplicateSeat(PalwBondKeyV2),
    #[error("seat bond {0:?} no longer exists at this chain point")]
    SeatBondMissing(PalwBondKeyV2),
    #[error("receipt signature does not verify under the seat bond's key")]
    ReceiptSignatureInvalid,
    #[error("receipt from seat {seat:?} is outside the receipt window: {why}")]
    ReceiptOutsideWindow { seat: PalwBondKeyV2, why: &'static str },
    #[error("an Unavailable receipt from seat {seat:?} does not name an obligation the producer had: {why}")]
    UnmetObligationNotProven { seat: PalwBondKeyV2, why: &'static str },
    #[error("no quorum: {valid} valid and {unavailable} unavailable of {needed} needed")]
    NoQuorum { valid: u16, unavailable: u16, needed: u16 },
    #[error("verification V2: segment {segment} has {have} valid attestation(s), {need} needed")]
    CoverageShort { segment: u16, have: u16, need: u16 },
    #[error("verification V2: seat {seat:?} attested mask {got:?}, assignment is {expected:?}")]
    MaskNotAssigned { seat: PalwBondKeyV2, got: u32, expected: u32 },
    #[error("claim {0} does not license by parts: its panel is not a declared plan's stratified shape")]
    NotLicensedByParts(Hash64),
    #[error("claim {0} licenses by parts; a whole-object licence or default does not apply to it")]
    LicensedByParts(Hash64),
    #[error("the part names a {part}-shard plan and the class declared {declared}")]
    ShardPlanMismatch { declared: u32, part: u32 },
    #[error("shard {shard} of a {count}-shard plan")]
    ShardOutOfRange { shard: u32, count: u32 },
    #[error("shard {shard} has already licensed")]
    ShardAlreadyLicensed { shard: u32 },
    #[error("shard {shard}: {needed} seats needed and {available} eligible bonds declared it")]
    InsufficientEligibleShardBonds { shard: u32, needed: u16, available: u16 },
    #[error("the stratified draw refused: {0}")]
    ShardDraw(String),
    /// ADR-0147: no bond of the network's base-class population, other than the executor's and the
    /// registrant's own, is eligible to sit as this claim's outsider. The claim waits, and voids at
    /// its bind deadline if the network never offers one.
    #[error("claim {0}: no bond outside the class's own population is eligible to sit as its outsider")]
    NoOutsider(Hash64),
    /// ADR-0147: the receipts reach the quorum but the outsider has not answered `Valid`. Not a
    /// refusal of the set — an assembler keeps collecting, exactly as for `NoQuorum` — and not a
    /// licence: the claim licenses when its outsider says `Valid`, or voids at its deadline.
    #[error("claim {claim}: the quorum is reached but its outsider {seat:?} has not answered Valid")]
    OutsiderHasNotAnswered { claim: Hash64, seat: PalwBondKeyV2 },
    /// **ADR-0152 SW-10: the eligible-stake floor.** The operators the stake draw may seat weigh
    /// `eligible` whole MSK (capped, SW-2) against a base of `base` — every operator that could sit
    /// on this claim but for the load-dependent filters (the route-matrix Valid lock — past
    /// `palw_rcore_plus`, its one-ledger seat filter, S-3's L-4b — and, only where that filter
    /// is absent, the panel economy's headroom) — and `1000 · eligible < floor‰ · base`. The
    /// claim does not bind here: a saturated honest population halts binding instead of leaving
    /// the seats to whoever is idle (SW-A1: without the floor the collusion threshold falls to
    /// 0.52M MSK at one genesis seat eligible and to any five idle Sybils at none). Fail closed,
    /// like `InsufficientEligibleBonds`, and checked after it, so that refusal keeps exactly the
    /// operator lottery's condition (T92).
    #[error(
        "the eligible operators weigh {eligible} MSK of a base of {base} MSK, under the eligible-stake floor: the draw does not bind"
    )]
    InsufficientEligibleStake { eligible: u128, base: u128 },
    /// ADR-0152 Q-1: a `Sampled` receipt where `Params::palw_rcore_plus` is not in force. It poisons
    /// any set it is in, so every other network's receipt sets are exactly what they were.
    #[error("seat {0:?} answered Sampled, a verdict only R-core+ admits (ADR-0152 Q-1)")]
    SampledBelowRcore(PalwBondKeyV2),
    /// ADR-0152 SR-10: the V3 supplementary door's refusals, by name.
    #[error("a V3 supplementary receipt set is refused: {0}")]
    SupplementaryV3Refused(&'static str),
    /// RFC-0006: a layer-sharded part is refused (the fence, the claim's plan, the shard, a receipt).
    #[error("a layer-sharded part is refused: {0}")]
    TirShardPartRefused(String),
    /// RFC-0006: the part's receipts are sound but do not license the shard yet — the assembler keeps collecting.
    #[error("the shard's receipts are not yet a licence: {0}")]
    TirShardPartShort(&'static str),
}

impl PalwPanelV2Error {
    /// **The set is sound; it is just not a licence.** `validate_receipt_coverage_v2` returns these
    /// three only after every receipt in the set has passed its own checks (a seat, once, signed,
    /// inside the window, its assigned mask, a real obligation): too few `Valid` (`NoQuorum`), a
    /// segment attested fewer than twice (`CoverageShort`), or a quorum still waiting on its outsider
    /// (`OutsiderHasNotAnswered`). An assembler that adds one receipt to a sound set and gets one of
    /// them back therefore knows the receipt is sound, and keeps it. Every other refusal names a
    /// receipt that poisons any set it is in — or a claim no set can license.
    pub fn is_receipt_set_shortfall(&self) -> bool {
        matches!(
            self,
            Self::NoQuorum { .. } | Self::CoverageShort { .. } | Self::OutsiderHasNotAnswered { .. } | Self::TirShardPartShort(_)
        )
    }
}

/// **ADR-0100 Decision 4: the stratified draw, from chain state.** The eligible bonds are the flat
/// draw's (`palw_panel_eligible_bonds_v2`, one spelling), each carrying the shards of the claim's
/// class it declared (`BondShardsDeclared`); a bond that declared none is not a candidate. The
/// draw per shard is `derive_shard_panel_v1` — its own ticket domain with the shard mixed in, one
/// seat per operator per shard, `params.seat_count` seats and `params.quorum` a shard — and the
/// result is stored flat, shard-major, which is what makes a stratified panel recognisable by its
/// length alone (`palw_claim_licenses_by_parts_v1`). A shard short of operators refuses the whole
/// draw by name: a claim one of whose shards nobody can judge is not a claim anybody can license.
#[allow(clippy::too_many_arguments)]
pub fn derive_stratified_panel_v2(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    shard_count: u32,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    let class_id = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?.class_id;
    let candidates: Vec<crate::palw_shard_panel_v1::PalwShardCandidateV1> =
        // **The stratified (shard) draw does not read ADR-0124's seat economy yet.** It is dormant
        // on every network (`palw_shard_licensing` is `None` everywhere), so the economy rides the
        // flat draw below; when the shard fence is armed, this draw needs the same floor and
        // headroom — and ADR-0130's exposure floor and one-entry-per-operator lottery with them,
        // per shard — a follow-up gated behind the shard fence, not this one (C-02's precedent).
        palw_panel_eligible_bonds_v2(state, claim_id, min_collateral_sompi, registered_by_daa, capability_proof, None, None, params.seat_count)?
            .into_iter()
            .filter_map(|(bond_key, bond)| {
                state.shards_of_bond(bond_key, &class_id).map(|shards| crate::palw_shard_panel_v1::PalwShardCandidateV1 {
                    bond: *bond_key,
                    operator_id: bond.operator_id,
                    shards: shards.to_vec(),
                })
            })
            .collect();
    let panel = crate::palw_shard_panel_v1::derive_shard_panel_v1(
        &candidates,
        shard_count,
        params.seat_count,
        params.quorum,
        claim_id,
        anchor_block,
    )
    .map_err(|e| match e {
        crate::palw_shard_panel_v1::PalwShardPanelError::InsufficientEligibleBonds { shard, needed, available } => {
            PalwPanelV2Error::InsufficientEligibleShardBonds { shard, needed, available }
        }
        other => PalwPanelV2Error::ShardDraw(other.to_string()),
    })?;
    Ok(panel.seats.into_iter().flatten().collect())
}

/// **RFC-0006 §4.1/§4.2: the per-shard draw of an IR claim** (past `Params::palw_tir_shard_v1`).
///
/// For each shard `i` of the class's plan, [`derive_panel_v2_with_policy_judged_v1`] over the shard's own seed
/// ([`crate::palw_tir_shard_v1::palw_tir_shard_seed_v1`]) and its readiness class
/// ([`crate::palw_tir_shard_v1::palw_tir_shard_ready_class_v1`]): `s_shard` class seats from the bonds that proved they
/// hold the shard, one per operator, drawn by the draw policy in force (the stake race past `palw_rcore_plus`), and, where
/// the claim is outsider-judged, the shard's OUTSIDER first — drawn from the network's base-class population exactly as
/// ADR-0147 draws the claim's, the shard in the ticket. A bond may sit in several shards (distinct duties, distinct masks) and
/// an operator holds at most one outsider seat of the claim. The result is stored flat, shard-major, `[outsider?] ++ class seats` a shard
/// ([`crate::palw_tir_shard_v1::palw_tir_panel_stride_v1`]), which is what makes it recognisable by its length. A shard
/// short of seats refuses the whole draw by name: a sharded class never falls back to a flat panel.
#[allow(clippy::too_many_arguments)]
pub fn derive_tir_shard_panel_v1(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    seed: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    policy: PalwPanelDrawPolicyV1,
    s_l: u16,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let outsider = policy
        .independence
        .filter(|independence| independence.governs(claim))
        .is_some_and(|independence| crate::palw_state_v2::palw_claim_is_outsider_judged_v1(state, claim, Some(independence.from_daa)));
    let stride = crate::palw_tir_shard_v1::palw_tir_panel_stride_v1(outsider);
    let mut seats: Vec<PalwPanelSeatV2> = Vec::with_capacity(usize::from(s_l) * usize::from(stride));
    let mut outsider_operators: Vec<Hash64> = Vec::new();
    for shard in 0..s_l {
        let judged = PalwShardJudgedV1 {
            judged_class: crate::palw_tir_shard_v1::palw_tir_shard_ready_class_v1(&claim.class_id, s_l, shard),
            seats: stride,
            excluded_outsider_operators: &outsider_operators,
            // The same bond may sit in several shards (RFC §4.1): its seats are distinct duties with distinct masks, and its lock
            // accumulates the shards' prices.
            excluded_bonds: &[],
        };
        let drawn = derive_panel_v2_with_policy_judged_v1(
            state,
            params,
            claim_id,
            crate::palw_tir_shard_v1::palw_tir_shard_seed_v1(&seed, claim_id, shard),
            min_collateral_sompi,
            registered_by_daa,
            capability_proof,
            policy,
            Some(&judged),
        )
        .map_err(|e| match e {
            PalwPanelV2Error::InsufficientEligibleBonds { needed, available } => {
                PalwPanelV2Error::InsufficientEligibleShardBonds { shard: u32::from(shard), needed, available }
            }
            other => other,
        })?;
        if outsider && let Some(first) = drawn.first() {
            outsider_operators.push(first.operator_id);
        }
        seats.extend(drawn);
    }
    Ok(seats)
}

/// The chain fact that fixes a claim's anchor, supplied by the pipeline from its own candidate
/// chain (the same trust class as [`PalwBlockContextV2`]): `anchor_block` is the FIRST chain
/// block whose DAA score reached `accepted_daa + anchor_delay`, and `predecessor_daa` is its
/// selected parent's DAA score — carried so "first" is checkable: the predecessor must still be
/// short of the slot.
///
/// `anchor_execution` is the anchor ATTEMPT's execution commitment
/// ([`palw_panel_anchor_execution_v1`] of the anchor block's header) when the network keys its panel
/// seed on it — for a claim whose anchor block is at or past `Params::palw_panel_seed_execution`
/// (lane F1; only attempt blocks anchor there, since the fence needs `palw_rcore_plus`) — and `None`
/// below it, where the seed is `anchor_block` byte for byte as testnet-12 launched
/// ([`Self::panel_seed`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAnchorFactV2 {
    pub anchor_block: BlockHash,
    pub anchor_daa: u64,
    pub predecessor_daa: u64,
    pub anchor_execution: Option<Hash64>,
}

impl PalwAnchorFactV2 {
    /// **The one seed a claim's panel is drawn from — and its segment assignment and S3 sample.**
    ///
    /// `Some(execution)`: [`palw_panel_draw_seed_v1`]`(execution, claim)`. `None`: the anchor block's
    /// identity, as every claim anchored below `Params::palw_panel_seed_execution` has always drawn.
    ///
    /// The seed is what a `PanelBound` names as its `anchor` and what the fold stores as
    /// `PalwPanelStateV2::anchor`, and every later draw of the claim reads THAT field — the segment
    /// assignment (`palw_segment_assignment_v2`), the S3 sample (`palw_layer_sample_v3`), the S2
    /// optimistic door and the leaf-evidence interval draw — so the three are keyed on one value by
    /// construction and cannot drift apart.
    pub fn panel_seed(&self, claim_id: &Hash64) -> Hash64 {
        match self.anchor_execution {
            Some(execution) => palw_panel_draw_seed_v1(&execution, claim_id),
            None => self.anchor_block,
        }
    }
}

/// **The panel seed past `Params::palw_panel_seed_execution` (lane F1): `H(draw-seed domain ‖ anchor
/// execution ‖ claim)`** (2026-09-25, the anchor-identity re-roll, `wf_72c1a397-e23`).
///
/// **What was wrong.** The seed was the anchor BLOCK's identity hash, and the identity covers the
/// carrier bytes of `palw_commitment` — the attempt's ML-DSA-87 signature among them — plus the
/// timestamp and the full nonce, none of which the class lottery prices: `execution_commitment_v3`
/// blanks the challenge, the pre-PoW hash zeroes nonce and timestamp and excludes the commitment, and
/// the nonce enters only through its `2^22` bucket. So the producer of the anchor block computed the
/// panel privately and re-rolled it for the cost of one re-signature (~1.2 ms), a timestamp tick or a
/// nonce inside its bucket, with the same winning ticket: a coverage lie reached `Final` with 0 slash
/// on two Sybil seats in ~1,000 re-signs, and a silent panel could be dealt to an honest claim.
///
/// **What it is now.** `anchor_execution` is the anchor attempt's execution commitment — the value
/// its class ticket is drawn from (`class_ticket_v3`) and the fold dedups on — so every identity one
/// lottery win can take (any signature, any timestamp, any nonce of the bucket) draws ONE panel, and
/// another panel costs another execution commitment: another lottery win. `claim` makes the seed
/// per-claim (one anchor block anchors several claims; each still draws its own panel). Both are fixed
/// consensus data every node reads from the anchor block's header and the claim record, so the seed
/// is a pure function of the chain: reorg, IBD and a pruning-proof sync recompute it bit for bit.
///
/// **What it does not close.** While audit P0-10 is open a lottery win itself is cheap — a junk trace
/// root is one BLAKE2b ticket, ~279 expected draws on the testnet-12 floor — so a new panel costs
/// ~279 hashes, not an inference: in wall time about what a re-signature cost. The fence is the
/// half of the fix that makes a re-roll cost a lottery WIN; only an inference-bound win (P0-10) makes
/// that win expensive. docs/history/testnet-12/t12-panel-seed-2026-09-25.md states why no seed rule alone can close it.
pub fn palw_panel_draw_seed_v1(anchor_execution: &Hash64, claim_id: &Hash64) -> Hash64 {
    let mut seed = keyed(PALW_PANEL_V2_DOMAIN_DRAW_SEED_V1);
    seed.update(anchor_execution.as_byte_slice());
    seed.update(claim_id.as_byte_slice());
    finish(seed)
}

/// **The anchor attempt's execution commitment, from the anchor block's header alone** —
/// `execution_commitment_v3(attempt, execution_anchor_v3(network, pre_pow(header), class, bond,
/// header.nonce))`, exactly the processor's `palw_execution_key_v1` for that block (the key the fold
/// dedups on and the anchor its class ticket was drawn under). `None` for a header that carries no
/// attempt, or an envelope that does not decode (the header stage refuses such a block, so a stored
/// chain block never answers `None` past `palw_rcore_plus`, where only attempt blocks anchor — which
/// `Params::palw_panel_seed_execution` requires at or below itself).
///
/// Invariant under everything the class lottery does not price: the signature (outside the attempt),
/// the timestamp and the nonce inside its bucket (outside the pre-PoW hash and the bucket index), and
/// the challenge (blanked). Moved by anything it does price.
pub fn palw_panel_anchor_execution_v1(network_domain: Hash64, header: &crate::header::Header) -> Option<Hash64> {
    if !crate::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id) {
        return None;
    }
    let envelope = crate::palw_attempt_material_v3::native_attempt_base_v3(&header.palw_commitment).ok()?;
    let attempt = &envelope.attempt;
    let execution_anchor = crate::palw_attempt_v2::execution_anchor_v3(
        network_domain,
        crate::hashing::header::pre_pow_hash_64(header),
        attempt.class_id,
        &attempt.executor_bond,
        header.nonce,
    );
    Some(crate::palw_attempt_v2::execution_commitment_v3(attempt, execution_anchor))
}

/// **How many DISTINCT OPERATORS could take a seat right now** — the live registry measured with
/// the same two predicates [`derive_panel_v2_with_maturity`] uses to build its ticket list.
///
/// This exists so an operator-facing warning cannot drift from the rule it warns about. It is not
/// a second implementation of the draw: it calls
/// [`crate::palw_state_v2::palw_bond_may_take_work_v2`] and applies the identical maturity
/// comparison, and it counts operators rather than bonds because the draw seats one bond per
/// operator — a registry of ten bonds under two operators seats two. Past the panel economy the
/// operator is the draw's own unit (ADR-0130: one lottery entry each), so this count is the number
/// of entrants, less whatever the seat floor and the headroom exclude.
///
/// **The executor exclusions are deliberately NOT applied**, and that is the whole reason the
/// answer is comparable to `palw_v2_maturity_armable_bonds_v1()`. That bar is derived as
/// `seat_count` + one for the executor + one for a departure + one of margin, so the executor is
/// already priced INTO the number being compared against; excluding it here as well would count it
/// twice and make the warning fire a seat early. A caller asking "can THIS claim seat a panel"
/// must use the draw itself, which does exclude it.
///
/// `registered_by_daa` is [`palw_seat_maturity_floor_v1`]'s output: `None` when ADR-0065 D1 is off,
/// in which case maturity excludes nobody.
pub fn palw_seatable_operators_v1(state: &PalwChainStateV2, min_collateral_sompi: u64, registered_by_daa: Option<u64>) -> usize {
    let mut operators: std::collections::BTreeSet<Hash64> = std::collections::BTreeSet::new();
    for (_, bond) in state.bonds_iter() {
        // Status AND balance, through the one predicate — a bond slashed to nothing keeps `Active`
        // for the rest of the chain's life, and counting it here would report a seat that the
        // draw will not fill.
        if !crate::palw_state_v2::palw_bond_may_take_work_v2(bond, min_collateral_sompi) {
            continue;
        }
        // ADR-0065 D1, the same comparison the draw makes.
        if registered_by_daa.is_some_and(|by| bond.registered_daa > by) {
            continue;
        }
        operators.insert(bond.operator_id);
    }
    operators.len()
}

/// **Is a live-registry shortfall report due at `daa`?** — the rate limiter for the ADR-0065 D1
/// warning, as a pure function so it can be tested.
///
/// `last` is the DAA the shortfall was last reported at, or [`PALW_SHORTFALL_NEVER_REPORTED`].
/// **Log state only**: no consensus path reads the answer, so two nodes that report at different
/// moments still agree about every block.
///
/// Four arms, and three of them are each a way this went wrong before it was a function:
/// * `worsened` → the situation got strictly worse since the last report, so say so at once. The
///   interval is a bind window (~20 h at the live cadence); without this arm an operator who was
///   told "the margin is gone" would hear nothing for that long while the chain actually stopped
///   binding claims, which is the transition they most need to see.
/// * never reported → report now. `last + interval` saturates at `u64::MAX`, so comparing against
///   it alone would suppress the FIRST report for ever, which is the one that matters most.
/// * `daa < last` → the virtual moved to a different branch. The caller runs per chain block
///   ADDED, so a reorg replays lower scores; suppressing on the forward test alone would go quiet
///   from the reorg until the new branch climbed past the old tip's window — exactly the stretch
///   an operator is trying to understand.
/// * otherwise report only once the interval has elapsed.
pub fn palw_shortfall_report_is_due_v1(last: u64, daa: u64, interval: u64, worsened: bool) -> bool {
    worsened || last == PALW_SHORTFALL_NEVER_REPORTED || daa < last || daa >= last.saturating_add(interval)
}

/// The sentinel [`palw_shortfall_report_is_due_v1`] reads as "nothing reported yet".
pub const PALW_SHORTFALL_NEVER_REPORTED: u64 = u64::MAX;

/// **ADR-0065 D2's fast path, and it is a proof rather than a shortcut.**
///
/// `true` when `minted` newly-registered bonds COULD form a quorum on some panel, so the
/// deep-reorg gate must go and look. `false` means no panel can be majority-new and the scan is
/// provably unnecessary.
///
/// A panel seats distinct bonds, so it can hold at most `minted` new ones, and reaching quorum out
/// of them needs `minted >= quorum`. That is the exact condition. The threshold used here —
/// `seat_count - quorum` — is strictly smaller, because [`PalwPanelParamsV2::new`] enforces
/// `2 * quorum > seat_count` (the invariant that stops `Valid` and `Unavailable` both reaching
/// quorum), which rearranges to `seat_count - quorum < quorum`.
///
/// Deliberately the smaller one: it means the gate sometimes scans when it need not, and **never
/// skips when it must**. `the_provenance_fast_path_never_skips_a_reachable_quorum` checks that
/// direction exhaustively over every legal panel shape, because it is the one that matters — a
/// `false` that should have been `true` is a rule that silently does not apply.
///
/// On the shipped `(5, 3)` the threshold is 2: a branch that registered at most two bonds since
/// the fork cannot have seated a majority-new panel, so a static registry never pays for the walk.
pub fn palw_minted_seats_can_reach_quorum_v1(minted: usize, params: &PalwPanelParamsV2) -> bool {
    minted > params.seat_count().saturating_sub(params.quorum()) as usize
}

/// **ADR-0065 D1's floor, computed in one place.** `Some(anchor_daa - window)`, saturating, or
/// `None` when the rule is off.
///
/// It is a function rather than an inline subtraction because two callers need it — the acceptance
/// layer that validates a proposed `PanelBound` and the node that assembles one — and a panel is
/// accepted only if it equals the derived one exactly. Two subtractions that could disagree by one
/// would be a node proposing a panel its own peers refuse.
pub fn palw_seat_maturity_floor_v1(anchor_daa: u64, bond_maturity_daa: Option<u64>) -> Option<u64> {
    bond_maturity_daa.map(|window| anchor_daa.saturating_sub(window))
}

/// **The second clock's maturity floor** (2026-09-23 heartbeat audit): the DAA at which the
/// `depth`-th most recent settled anchor before `anchor_daa` was accepted — a `Final` attempt
/// claim, each a won class draw and a licensed panel. A bond registered AFTER that DAA has fewer
/// than `depth` anchors between its registration and this draw, and ADR-0065 D1's window alone
/// would let it judge after `window_daa` of bondless heartbeats. `None` when fewer than `depth`
/// such anchors exist before the anchor at all: the bootstrap waiver, because the first anchors
/// of a chain are produced by panels drawn before any anchor could have settled, and a floor that
/// forbade them would forbid the chain its first `Final`.
///
/// **Read from the anchor ring** (2026-09-24 DoS audit, fixes #2 and #13). This used to walk every
/// retained claim for its `Final` attempts — once per candidate claim per template, quadratic in
/// the claim table — and it counted `Final`s, which the DAA sweep produces on heartbeat-only
/// blocks. It now reads `recent_anchor_daas`: the anchors the second clock counts past the fence
/// (licences, each a quorum's live signatures), sorted, and kept for every anchor a live panel can
/// still be validated at. When fewer than `depth` anchors lie before `anchor_daa` in the ring but
/// the chain has settled more than the ring holds, older ones were pruned and the answer is not in
/// the state: the floor is then `0`, the conservative end (only genesis bonds are mature), never
/// the bootstrap waiver.
pub fn palw_settled_anchor_floor_daa_v1(state: &PalwChainStateV2, anchor_daa: u64, depth: u64) -> Option<u64> {
    if depth == 0 {
        return None;
    }
    let ring = state.recent_anchor_daas();
    let before = ring.partition_point(|&daa| daa < anchor_daa);
    match usize::try_from(depth) {
        Ok(depth) if before >= depth => Some(ring[before - depth]),
        _ if state.settled_attempt_finals() > ring.len() as u64 => Some(0),
        _ => None,
    }
}

/// **ADR-0065 D1's window on both clocks** — the window the draw and the validator are handed,
/// widened so that `anchor_daa - window` is no later than the second clock's floor. With the floor
/// `None` (below the fence, no second clock, or the bootstrap waiver) this is `window` itself, so
/// every existing caller is byte-identical.
pub fn palw_bond_maturity_window_v2(anchor_daa: u64, window: u64, settled_floor_daa: Option<u64>) -> u64 {
    match settled_floor_daa {
        None => window,
        Some(floor) => window.max(anchor_daa.saturating_sub(floor)),
    }
}

/// The deterministic sortition. Reads ONLY the candidate-scoped bond registry; returns seats in
/// ticket order (the canonical panel order — validation compares exactly).
pub fn derive_panel_v2(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    derive_panel_v2_with_maturity(state, params, claim_id, anchor_block, min_collateral_sompi, None)
}

/// [`derive_panel_v2`] with ADR-0065 D1's seat maturity.
///
/// `registered_by_daa` is `Some(anchor_daa - bond_maturity_daa)` past the fence, and a bond may
/// take a seat only if its `registered_daa` is at or before it. `None` — every shipped preset —
/// is byte-identical to the draw before the parameter existed.
///
/// **Measured against the ANCHOR, not against the binding block.** The panel is a pure function of
/// the claim (`validate_panel_bound_v2` recomputes it and demands exact equality), and the anchor
/// is the claim's own — so maturity computed from `anchor_daa` keeps that property, while
/// computing it from `ctx.daa_score` would make the derived panel change block by block and refuse
/// any `PanelBound` that missed the block it was assembled for.
///
/// **What it is for.** `registered_daa` is written from `ctx.daa_score` at the fold
/// (`palw_state_v2.rs`'s `BondRegistered` arm) and is not registrant-chosen, so there is no
/// grinding surface. What the window buys is that a bond cannot be minted and used in the same
/// breath: on a private fork, sybil bonds folded into the fork's own blocks are unusable until the
/// fork itself has advanced `bond_maturity_daa`, which is work the fork has to actually do.
///
/// **The liveness trap this must not walk into.** A short draw is not a smaller panel — it is
/// `InsufficientEligibleBonds`, so the claim never binds and voids at `BindTimeout`. The shipped
/// genesis registers exactly `PALW_V2_PANEL_SEATS + 1` bonds and the draw excludes the executor,
/// so there is ZERO slack: if a maturity window made even one genesis bond ineligible, every claim
/// on a fresh network would void, `safe_frontier` would stay at 0 and pruning would never start.
/// Genesis bonds carry `registered_daa = genesis.daa_score = 0`, so what keeps them eligible is
/// the fence's own arming height, and `Params::validate_palw_v2` refuses a fence armed before its
/// own window has elapsed — the trap is closed by construction rather than by a deployment note.
pub fn derive_panel_v2_with_maturity(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    // `weighted: false` — the pre-C-02 unweighted draw. The fence-aware callers reach
    // `derive_panel_v2_with_capability_proof` directly with the resolved flag.
    derive_panel_v2_with_capability_proof(state, params, claim_id, anchor_block, min_collateral_sompi, registered_by_daa, false, false)
}

/// [`derive_panel_v2_with_maturity`] with **ADR-0071 SA-3's production proof**.
///
/// `capability_proof` is `Params::palw_capability_bound` resolved at the anchor, and `false` —
/// every shipped preset — is byte-identical to the draw before the parameter existed.
///
/// **Measured at the ANCHOR like maturity, and for the same reason**: the panel is a pure function
/// of the claim (`validate_panel_bound_v2` recomputes it and demands exact equality), so a
/// predicate resolved at the binding block would make the derived panel change block by block.
/// The production FACTS it reads are chain state at the point the draw runs, which is the same
/// state `validate_panel_bound_v2` holds when it recomputes.
///
/// **The liveness trap, closed by the genesis-class exemption.** A short draw is not a smaller
/// panel — it is `InsufficientEligibleBonds`, and every claim then voids at `BindTimeout` with its
/// escrow burned. The shipped genesis registry has zero slack, and no genesis bond has produced
/// anything at block 0; `palw_bond_may_judge_class_v3` exempts a class with no registrant bond,
/// which is exactly the set a genesis assembly registers.
/// **The bonds that may sit on a claim's panel, in the registry's order.** Exclusions per Decision
/// 7: the executor's bond, the executor's operator, the executor's key — all three read from the
/// ONE registry, so no second namespace exists for them to diverge in (the P0-7 defect). Shared by
/// the flat draw and the stratified one (ADR-0100 Decision 4), so a seat predicate added to one
/// cannot be missing from the other.
///
/// `seat_count` is the panel the claim would be drawn with; past the economy it prices ADR-0130's
/// exposure floor (`economy.seat_exposure(claim.reserved, claim.escrowed_reward, seat_count)`), which
/// is the amount the bond must be able to reserve under its ceiling to be drawn at all. Pure and
/// public: a shadow reader passes a hand-built `economy` carrying a hypothetical reward multiple and
/// reads which bonds — and so which operators — WOULD be eligible against a state snapshot.
pub fn palw_panel_eligible_bonds_v2<'a>(
    state: &'a PalwChainStateV2,
    claim_id: &Hash64,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    readiness: Option<crate::palw_model_registry_v1::PalwReadinessPolicyV1>,
    economy: Option<crate::palw_panel_economy_v1::PalwSeatEconomyV1>,
    seat_count: u16,
) -> Result<Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>, PalwPanelV2Error> {
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    palw_panel_eligible_bonds_judging_v1(
        state,
        claim_id,
        &claim.class_id,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        readiness,
        economy,
        seat_count,
    )
}

/// [`palw_panel_eligible_bonds_v2`] over the population of `judged_class` rather than of the
/// claim's own class — every other predicate (the executor's exclusions, the floor, the headroom
/// priced by THIS claim, the maturity floor) exactly as the claim's own draw applies it.
///
/// ADR-0147 calls it with the liveness floor's id to list the network's population for an
/// outsider seat: the bonds that serve the class every node runs, which is a set no registrant can
/// make class-specific. Pure and public, like the function it generalises.
#[allow(clippy::too_many_arguments)]
pub fn palw_panel_eligible_bonds_judging_v1<'a>(
    state: &'a PalwChainStateV2,
    claim_id: &Hash64,
    judged_class: &Hash64,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    readiness: Option<crate::palw_model_registry_v1::PalwReadinessPolicyV1>,
    economy: Option<crate::palw_panel_economy_v1::PalwSeatEconomyV1>,
    seat_count: u16,
) -> Result<Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>, PalwPanelV2Error> {
    palw_panel_eligible_bonds_judging_v2(
        state,
        claim_id,
        judged_class,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        readiness,
        economy,
        seat_count,
        true,
    )
}

/// [`palw_panel_eligible_bonds_judging_v1`] with the panel economy's headroom test optional:
/// `headroom = false` where the draw's Valid-lock carries ADR-0152's one-ledger seat filter
/// (`PalwPanelValidLockV1::rcore`), which asks the bind's own room question instead — the economy's
/// `3 × reserved` / `λ` stake is not what the seat reserves past `palw_rcore_plus`. The floor, the
/// exclusions and every other predicate are unchanged.
#[allow(clippy::too_many_arguments)]
pub fn palw_panel_eligible_bonds_judging_v2<'a>(
    state: &'a PalwChainStateV2,
    claim_id: &Hash64,
    judged_class: &Hash64,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    readiness: Option<crate::palw_model_registry_v1::PalwReadinessPolicyV1>,
    economy: Option<crate::palw_panel_economy_v1::PalwSeatEconomyV1>,
    seat_count: u16,
    headroom: bool,
) -> Result<Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>, PalwPanelV2Error> {
    palw_panel_bonds_judging_v1(
        state,
        claim_id,
        judged_class,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        readiness,
        economy,
        seat_count,
        PalwPanelPopulationV1::Eligible { headroom },
    )
}

/// **ADR-0152 SW-10: the base population** — [`palw_panel_eligible_bonds_judging_v1`] with every
/// filter EXCEPT the load-dependent one: the panel economy's exposure headroom (ADR-0124 Decision 4,
/// ADR-0130's floor). What stays is structural for this claim — `Active` at the floor (the panel's
/// floor past the economy), registered by the maturity floor (and, under ADR-0147, before the
/// anchor), not the executor's bond, operator or key, and able to run the judged class (capability
/// and readiness). The route-matrix Valid-lock filter ([`PalwPanelValidLockV1`]) is load-dependent
/// too, and the callers leave it off this list as they apply it to the eligible one. Past
/// `palw_rcore_plus` — the only place the stake draw runs — the one ledger's load question IS that
/// lock's seat filter (`PalwPanelValidLockV1::rcore`, S-3's L-4b), and the economy's headroom is asked
/// of neither list ([`palw_panel_eligible_bonds_judging_v2`] with `headroom = false`), so the base is
/// exactly the eligible list before the lock: the free stake decides WHETHER a bond is drawn, the
/// posted stake HOW OFTEN (SW-2).
///
/// The base is the weight the stake draw WOULD draw from if nobody's collateral were committed:
/// SW-10 refuses a draw whose eligible operators weigh less than `eligible_floor_permille` of it,
/// because what empties the eligible list while the base stays is honest seats filling up with the
/// locks of the work they do, and an idle Sybil never fills. `v31_review_numbers.py`'s
/// `worst_under_floor` prices the floor on exactly this split (base = every genesis seat and every
/// Sybil; eligible = the unsaturated seats and every Sybil).
#[allow(clippy::too_many_arguments)]
pub fn palw_panel_stake_base_bonds_judging_v1<'a>(
    state: &'a PalwChainStateV2,
    claim_id: &Hash64,
    judged_class: &Hash64,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    readiness: Option<crate::palw_model_registry_v1::PalwReadinessPolicyV1>,
    economy: Option<crate::palw_panel_economy_v1::PalwSeatEconomyV1>,
    seat_count: u16,
) -> Result<Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>, PalwPanelV2Error> {
    palw_panel_bonds_judging_v1(
        state,
        claim_id,
        judged_class,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        readiness,
        economy,
        seat_count,
        PalwPanelPopulationV1::StakeBase,
    )
}

/// **ADR-0152 SW-10's executor term** (M4 review, 2026-09-24, finding 1): the claim's executor
/// OPERATOR's own bonds under the base population's structural predicates — `Active` at the floor,
/// registered by the maturity floor, able to run the judged class — with the executor clause
/// inverted: exactly the bonds [`palw_panel_stake_base_bonds_judging_v1`] drops for being the
/// executor's operator, and only those. Empty where the executor could not sit even if it were not
/// the executor (below the panel floor, unregistered by the floor, incapable of the class), which
/// is every harness executor below the panel floor and every producer at the producer floor alone.
///
/// **Why the draw reads it.** SW-10 was priced on a population of eight (`v31_review_numbers.py`,
/// §3.14's table: "an honest-only 8-seat population still binds with one seat saturated, 7 of 8
/// is exactly 875‰"), and that holds only for an executor OUTSIDE the population. testnet-12's
/// producers are its eight genesis cards, so a genesis card's claim draws from the other seven:
/// one of them saturated leaves `6/7 = 857‰ < 875‰`, the draw refuses `InsufficientEligibleStake`,
/// and — a panel binding only in its anchor block (SW-8) — the claim voids. The busiest producer's
/// own claims fill its own headroom first, so one working card could halt every other card's
/// binding. [`derive_panel_v2_with_policy`] adds this term's capped weight to BOTH sides of the
/// floor (`(E + X) · 1000 ≥ (B + X) · 875`), which is SW-10 computed over the population the draw
/// would have if the executor were not excluded, the executor counted eligible: it cannot sit on
/// its own panel, so its load says nothing about who does. A genesis executor is back at 8 with one
/// other seat saturated (7/8 binds, 6/8 refuses); an executor outside the population adds nothing.
///
/// **What it gives an attacker.** `X` is the executor operator's SW-2 weight, capped at
/// `weight_cap_msk` (1,000,000 MSK), and it relaxes the floor exactly as the same capital posted as
/// an idle Sybil would (`E` and `B` both rise by it) while buying no seat, so in TOTAL attacker
/// capital no §4.3 threshold moves. Counted in Sybil stake alone (§4.3's unit) a floor-bound row
/// falls by at most `X`: the worst admitted state for P2 with filing stays 12.74M (k = 6 is bound
/// by the race, not the floor; k = 5's floor, 116 operators / 15.08M, still needs 108 / 14.04M
/// beside a capped executor), but P2 with a free redraw at 6 of 8 eligible — floor-bound at 58
/// operators / 7.54M, the race alone needing 5.98M — falls to 51 / 6.63M beside an executor bond
/// posted at the cap, below that row's stated worst of 7.15M (7 of 8). ADR-0152 §3.14 SW-10 and
/// §4.3 are B's to amend (A reviews): the executor term in SW-10's rule, and every floor-bound row
/// re-run through `v31_review_numbers.py` with `X` up to the cap (the free-redraw worst above is
/// the one this arithmetic finds moving); the code does not edit the ADR.
#[allow(clippy::too_many_arguments)]
pub fn palw_panel_stake_executor_bonds_judging_v1<'a>(
    state: &'a PalwChainStateV2,
    claim_id: &Hash64,
    judged_class: &Hash64,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    readiness: Option<crate::palw_model_registry_v1::PalwReadinessPolicyV1>,
    economy: Option<crate::palw_panel_economy_v1::PalwSeatEconomyV1>,
    seat_count: u16,
) -> Result<Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>, PalwPanelV2Error> {
    palw_panel_bonds_judging_v1(
        state,
        claim_id,
        judged_class,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        readiness,
        economy,
        seat_count,
        PalwPanelPopulationV1::StakeExecutor,
    )
}

/// Which list [`palw_panel_bonds_judging_v1`] returns — the three populations one spelling of the
/// seat predicates serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PalwPanelPopulationV1 {
    /// Every seat predicate: the draw's eligible list. `headroom: true` is byte for byte the list it
    /// always returned; `headroom: false` leaves the load question to the one-ledger seat filter
    /// (`PalwPanelValidLockV1::rcore`) the caller applies after (S-3's L-4b).
    Eligible { headroom: bool },
    /// SW-10's base: every predicate but the panel economy's headroom (the callers apply the Valid
    /// lock, and with it the one-ledger seat filter, to the eligible list only).
    StakeBase,
    /// SW-10's executor term: the base's structural predicates over the executor operator's own
    /// bonds (the executor clause inverted), headroom unread.
    StakeExecutor,
}

/// The one spelling of the seat predicates behind [`palw_panel_eligible_bonds_judging_v1`]
/// (`Eligible { headroom: true }`, byte for byte the list it always returned), S-3's
/// [`palw_panel_eligible_bonds_judging_v2`] (`Eligible { headroom }`), SW-10's
/// [`palw_panel_stake_base_bonds_judging_v1`] (`StakeBase`: no headroom) and its executor term
/// [`palw_panel_stake_executor_bonds_judging_v1`] (`StakeExecutor`: no headroom, the executor clause
/// inverted), so the lists cannot drift apart in anything but the filters that separate them.
#[allow(clippy::too_many_arguments)]
fn palw_panel_bonds_judging_v1<'a>(
    state: &'a PalwChainStateV2,
    claim_id: &Hash64,
    judged_class: &Hash64,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    readiness: Option<crate::palw_model_registry_v1::PalwReadinessPolicyV1>,
    economy: Option<crate::palw_panel_economy_v1::PalwSeatEconomyV1>,
    seat_count: u16,
    population: PalwPanelPopulationV1,
) -> Result<Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>, PalwPanelV2Error> {
    let headroom = matches!(population, PalwPanelPopulationV1::Eligible { headroom: true });
    // **Lane MU (ADR-0173)**: past `palw_audit_1004_v1` the draw judges the root the claim named (founding root when absent).
    let claim_root = readiness.filter(|policy| policy.root_keyed).and_then(|_| state.claim_root(claim_id));
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let executor_bond = claim.bond;
    let executor = state.bond(&executor_bond).ok_or(PalwPanelV2Error::SeatBondMissing(executor_bond))?;
    let executor_operator = executor.operator_id;
    let executor_key = executor.pubkey.clone();
    // **ADR-0124 Decisions 3 and 4, past the panel-economy fence.** The floor a seat must hold is
    // the panel's (ten producer floors), and a bond is drawn only while its free collateral covers
    // the exposure the seat would reserve — the same ceiling every other reservation on the bond
    // lives under, so the collateral behind a claim it produces cannot double as the collateral
    // behind a claim it judges. Below the fence: the registry's floor, and no headroom question.
    // **ADR-0130:** the exposure is the one the fold reserves at binding — the claim-priced stake,
    // raised to `λ` × the seat's most where the floor is in force at the anchor.
    let floor = economy.map(|economy| economy.panel_floor_sompi).unwrap_or(min_collateral_sompi);
    let seat_stake = economy
        .map(|economy| economy.seat_exposure(claim.reserved, claim.escrowed_reward, seat_count as usize))
        .unwrap_or_else(|| crate::palw_panel_economy_v1::palw_seat_exposure_v1(claim.reserved));
    let mut eligible = Vec::new();
    for (bond_key, bond) in state.bonds_iter() {
        // **A seat has to have something left to lose** — status AND balance, through the one
        // predicate, so the RPC that reports eligibility and the sortition that decides it cannot
        // answer differently.
        if !crate::palw_state_v2::palw_bond_may_take_work_v2(bond, floor) {
            continue;
        }
        // **ADR-0160 lane liab (AG-3): a frozen bond is not drawn** — in every population, SW-10's base
        // included, so a frozen stake neither sits nor counts against the eligible-stake floor. The map
        // is empty below `Params::palw_capacity_aggregate_liability`, so every draw there is unchanged.
        if crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(state, bond_key) {
            continue;
        }
        // (SW-10's base, and the eligible list where the one-ledger seat filter asks the room
        // question instead, skip only this test: the headroom is the load.)
        if let Some(economy) = economy.filter(|_| headroom) {
            let backed = state.reserved_exposure(bond_key).saturating_add(state.registration_exposure(bond_key));
            if !crate::palw_panel_economy_v1::palw_seat_has_headroom_v1(
                bond.collateral,
                backed,
                seat_stake,
                economy.max_exposure_ratio_permille,
            ) {
                continue;
            }
        }
        // ADR-0065 D1: a bond has to have been standing for a while before it may judge.
        if registered_by_daa.is_some_and(|by| bond.registered_daa > by) {
            continue;
        }
        match population {
            PalwPanelPopulationV1::Eligible { .. } | PalwPanelPopulationV1::StakeBase => {
                if *bond_key == executor_bond || bond.operator_id == executor_operator || bond.pubkey == executor_key {
                    continue;
                }
            }
            // SW-10's executor term: only the executor operator's own bonds, which the two lists
            // above drop for exactly that reason.
            PalwPanelPopulationV1::StakeExecutor => {
                if bond.operator_id != executor_operator {
                    continue;
                }
            }
        }
        // **ADR-0071 Decision 3: a seat must be able to RUN the class it is drawn to judge.**
        //
        // A seat's job is re-execution. Drawn blind to capability, a bond holding none of the
        // class's artifact can only abstain, and a claim whose panel cannot reach quorum voids —
        // so the draw could only ever license the classes every node happens to hold. That was
        // every class while the floor held all the weight; ADR-0068 gave the model tiers 97.8% of
        // cadence, and a 33 GiB artifact is not something a seat holds by default.
        //
        // Undeclared is excluded, never defaulted — the rule the V1 job panel already states. A
        // permissive default converts an operator's silence into its conviction, because the duty
        // accounting charges exactly the seats this function names.
        //
        // **ADR-0071 SA-3, past `capability_proof`: declaring is a claim, producing is a proof.**
        // A declaration costs a signature and (SA-2) some reserved collateral; an accepted attempt
        // or free-prompt claim on the class is the chain having seen this bond actually run it.
        // Silence stays unjudged — a bond that declared and never produced is simply not drawn,
        // never charged for the omission and never convicted of it (ADR-0065 D4).
        let judge_root = if judged_class == &claim.class_id { claim_root.as_ref() } else { None };
        if !crate::palw_state_v2::palw_bond_may_judge_class_v5(
            state,
            bond_key,
            bond,
            judged_class,
            judge_root,
            capability_proof,
            readiness,
        ) {
            continue;
        }
        eligible.push((bond_key, bond));
    }
    Ok(eligible)
}

#[allow(clippy::too_many_arguments)]
pub fn derive_panel_v2_with_capability_proof(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    weighted: bool,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    derive_panel_v2_with_policy(
        state,
        params,
        claim_id,
        anchor_block,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        PalwPanelDrawPolicyV1 { weighted, economy: None, readiness: None, independence: None, valid_lock: None, stake: None },
    )
}

/// [`derive_panel_v2_with_capability_proof`] with the whole draw policy (ADR-0124): the seat
/// floor and the exposure headroom past `Params::palw_panel_economy`, and one lottery entry per
/// operator there whatever the deep fence says (ADR-0130, [`palw_panel_operator_lottery_v1`]).
/// `economy: None` is byte-identical to the draw before the policy existed.
///
/// Pure: a shadow reader may call it with a hand-built policy (a hypothetical reward multiple)
/// against a state snapshot to ask whether a claim's panel WOULD be drawable, and nothing is written.
#[allow(clippy::too_many_arguments)]
pub fn derive_panel_v2_with_policy(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    policy: PalwPanelDrawPolicyV1,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    derive_panel_v2_with_policy_judged_v1(
        state,
        params,
        claim_id,
        anchor_block,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        policy,
        None,
    )
}

/// **RFC-0006 §4.1: what a shard's draw judges** — the readiness class its seats must be ready for (the class
/// and the shard, [`crate::palw_tir_shard_v1::palw_tir_shard_ready_class_v1`]), how many seats it seats (the
/// class seats, plus the outsider where the claim is outsider-judged) and the operators that already hold the
/// claim's outsider seat in an earlier shard (one operator holds at most one outsider seat of a claim).
#[derive(Clone, Copy, Debug)]
pub struct PalwShardJudgedV1<'a> {
    pub judged_class: Hash64,
    pub seats: u16,
    pub excluded_outsider_operators: &'a [Hash64],
    /// Bonds left out of this shard's draw (none for a per-shard draw: a bond may sit in several shards).
    pub excluded_bonds: &'a [PalwBondKeyV2],
}

/// [`derive_panel_v2_with_policy`] for a SHARD's draw (RFC-0006) when `shard` is `Some`: the same eligibility,
/// lottery or stake race and outsider rule, over the shard's readiness class, with the shard's seat count and
/// the claim's earlier outsiders left out of the outsider's population.
#[allow(clippy::too_many_arguments)]
pub fn derive_panel_v2_with_policy_judged_v1(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    policy: PalwPanelDrawPolicyV1,
    shard: Option<&PalwShardJudgedV1<'_>>,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    // RFC-0006: a shard's draw judges the shard's readiness class and seats `shard.seats`; the flat draw is `shard: None`.
    let judged_class = shard.map(|j| j.judged_class).unwrap_or_else(|| state.claim(claim_id).map(|c| c.class_id).unwrap_or_default());
    let seat_total = shard.map(|j| j.seats).unwrap_or(params.seat_count);
    let no_operators: &[Hash64] = &[];
    let excluded_outsiders = shard.map(|j| j.excluded_outsider_operators).unwrap_or(no_operators);
    let no_bonds: &[PalwBondKeyV2] = &[];
    let excluded_bonds = shard.map(|j| j.excluded_bonds).unwrap_or(no_bonds);
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    // **ADR-0147.** A governed claim's population is fixed before its anchor, and a governed claim
    // of a BOUGHT class sits an outsider first. Neither applies to a claim accepted below the
    // fence, so every panel such a claim can bind is the one it always had.
    let independence = policy.independence.filter(|independence| independence.governs(claim));
    let registered_by_daa = match independence {
        Some(independence) => Some(independence.registered_by_daa(registered_by_daa)),
        None => registered_by_daa,
    };
    // Under the stake draw the outsider's SW-10 floor is checked after the class seats' operator
    // count (below), so `InsufficientEligibleBonds` keeps exactly the lottery's condition (T92);
    // `None` carries no floor and this is the call it always was.
    let (outsider, outsider_floor) = match independence {
        Some(independence) if crate::palw_state_v2::palw_claim_is_outsider_judged_v1(state, claim, Some(independence.from_daa)) => {
            let (seat, floor) = palw_panel_outsider_draw_excluding_v1(
                state,
                params,
                claim_id,
                anchor_block,
                min_collateral_sompi,
                registered_by_daa,
                capability_proof,
                &policy,
                &independence,
                excluded_outsiders,
                excluded_bonds,
            )?;
            (Some(seat), floor)
        }
        _ => (None, None),
    };
    // Every eligible bond (`palw_panel_eligible_bonds_v2`: the exclusions per Decision 7 and every
    // seat predicate, spelled once for this draw and the stratified one). ADR-0152: under the
    // one-ledger seat filter the room question is the filter's, not the economy's headroom.
    let eligible = palw_panel_eligible_bonds_judging_v2(
        state,
        claim_id,
        &judged_class,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        policy.readiness,
        policy.economy,
        params.seat_count,
        policy.valid_lock.and_then(|lock| lock.rcore).is_none(),
    )?;
    // The 2026-09-23 route-matrix audit's #3: a bond that cannot post the bind's Valid lock is not
    // a candidate — drawn, it failed the bind and held the claim unbound.
    let eligible: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = match policy.valid_lock {
        Some(lock) => eligible.into_iter().filter(|(key, _)| lock.admits(state, key)).collect(),
        None => eligible,
    };
    // RFC-0006: a bond already seated in an earlier shard of this claim is not a candidate of this one.
    let eligible: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> =
        eligible.into_iter().filter(|(key, _)| !excluded_bonds.contains(key)).collect();
    // The outsider's operator holds one seat, never two: an operator that serves the network AND
    // proved it holds the class is drawn for the class's seats only when it is not the outsider.
    let (eligible, needed) = match &outsider {
        Some(outsider) => (
            eligible.into_iter().filter(|(_, bond)| bond.operator_id != outsider.operator_id).collect::<Vec<_>>(),
            seat_total.saturating_sub(1),
        ),
        None => (eligible, seat_total),
    };
    // **ADR-0130: past the panel economy, one lottery entry per operator.** The eligibility is the
    // same list; only how it is ticketed changes.
    let mut seats = if let Some(stake) = policy.stake {
        // **ADR-0152 SW (v3.1): the stake-weighted race replaces the operator lottery**, whatever
        // `weighted` says — C-02's sub-tickets and ADR-0130's one ticket alike. The same eligible
        // list, one entry per operator, keyed `L / W` (SW-3); and SW-10's floor over the class's
        // base population, which excludes exactly what the eligible list excludes structurally —
        // the outsider's operator included, since it holds its one seat already.
        let base = palw_panel_stake_base_bonds_judging_v1(
            state,
            claim_id,
            &judged_class,
            min_collateral_sompi,
            registered_by_daa,
            capability_proof,
            policy.readiness,
            policy.economy,
            params.seat_count,
        )?;
        let base: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = match &outsider {
            Some(outsider) => base.into_iter().filter(|(_, bond)| bond.operator_id != outsider.operator_id).collect(),
            None => base,
        };
        let base: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> =
            base.into_iter().filter(|(key, _)| !excluded_bonds.contains(key)).collect();
        // SW-10's executor term (M4 review finding 1): the executor operator's capped weight where
        // it could sit on this class but for being the executor, on both sides of the floor.
        let executor = palw_panel_stake_executor_bonds_judging_v1(
            state,
            claim_id,
            &judged_class,
            min_collateral_sompi,
            registered_by_daa,
            capability_proof,
            policy.readiness,
            policy.economy,
            params.seat_count,
        )?;
        let executor_weight = palw_panel_stake_weight_v1(&executor, &stake);
        palw_panel_stake_race_with_v1(
            needed,
            claim_id,
            anchor_block,
            &eligible,
            &base,
            executor_weight,
            &stake,
            outsider_floor,
            Some(state),
        )?
    } else if policy.economy.is_some() {
        palw_panel_operator_lottery_of_v1(needed, claim_id, anchor_block, &eligible)?
    } else {
        // ADR-0124 Decision 5: the panel economy retires stake weighting (see the policy's doc) —
        // and below it `weighted` is the deep fence's.
        palw_panel_bond_lottery_v1(needed, claim_id, anchor_block, &eligible, policy.weighted, min_collateral_sompi)?
    };
    // The canonical order: the outsider first, then the class's seats in lottery order. Position
    // zero is how every licensing arm finds the outsider (`palw_licence_names_its_outsider_v1`),
    // and `validate_panel_bound_v2_with_policy` compares this exact order.
    if let Some(outsider) = outsider {
        seats.insert(0, outsider);
    }
    Ok(seats)
}

/// **The per-bond draw below the panel economy**: every eligible bond draws one ticket (C-02's
/// `weighted` gives it one per `min_collateral` of stake, capped), tickets sort, one seat per
/// operator, the first `needed` win.
fn palw_panel_bond_lottery_v1(
    needed: u16,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
    weighted: bool,
    min_collateral_sompi: u64,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    let mut tickets: Vec<(Hash64, PalwBondKeyV2, Hash64)> = Vec::new();
    for (bond_key, bond) in eligible {
        // **C-02 (deep fence): stake-weighted sortition by bucketed sub-tickets.** Below the fence
        // (`weighted == false`) each eligible bond draws ONE ticket, so the draw ignores how much
        // collateral is at stake and a min-collateral bond has the same seat odds as a whale. Past
        // the fence a bond draws `floor(collateral / min_collateral)` sub-tickets — one per
        // min_collateral unit of stake, capped at `PALW_V2_MAX_SEAT_TICKETS_PER_BOND` — so its chance
        // of landing a low (winning) ticket scales with its stake. Every eligible bond has
        // `collateral >= min_collateral` (the eligibility predicate), so the count is >= 1 and the
        // eligible set never shrinks: the shipped genesis's zero-slack registry (`PANEL_SEATS + 1`
        // bonds) keeps every bond a candidate, so the liveness cliff is untouched. The per-operator
        // dedup below still grants a bond AT MOST one seat, so weighting cannot let one staker own a
        // quorum — it only makes a higher-stake operator likelier to win its single seat, and the
        // stake a seat then risks is `claim.reserved` (BC-SYBIL), so the two findings compose.
        let sub_tickets: u64 =
            if weighted { (bond.collateral / min_collateral_sompi.max(1)).clamp(1, PALW_V2_MAX_SEAT_TICKETS_PER_BOND) } else { 1 };
        for j in 0..sub_tickets {
            let mut ticket = palw_panel_seat_ticket_state_v1(anchor_block, claim_id, bond_key);
            // The sub-ticket index is mixed in ONLY past the fence: below it the loop runs once and
            // this line does not execute, so the digest is `H(domain ‖ anchor ‖ claim ‖ bond)` byte
            // for byte as before — a fenced build above the height and an unfenced one below it
            // never hash the same bond to a different ticket.
            if weighted {
                ticket.update(&j.to_le_bytes());
            }
            tickets.push((finish(ticket), **bond_key, bond.operator_id));
        }
    }
    tickets.sort();

    // One seat per operator, in ticket order, first `needed` win.
    let mut seats: Vec<PalwPanelSeatV2> = Vec::new();
    let mut seated_operators: Vec<Hash64> = Vec::new();
    for (_, bond_key, operator_id) in tickets {
        if seats.len() == needed as usize {
            break;
        }
        if seated_operators.contains(&operator_id) {
            continue;
        }
        seated_operators.push(operator_id);
        seats.push(PalwPanelSeatV2 { bond: bond_key, operator_id });
    }
    if seats.len() < needed as usize {
        return Err(PalwPanelV2Error::InsufficientEligibleBonds { needed, available: seats.len() as u16 });
    }
    Ok(seats)
}

/// **ADR-0147: the outsider seat of an outsider-judged claim.**
///
/// The population is [`palw_panel_eligible_bonds_judging_v1`] for the liveness floor — every bond
/// eligible to judge the class every node runs, under the same executor exclusions, floor,
/// headroom and (already cut at the anchor by the caller) registration floor as the claim's own
/// draw — minus the class registrant's own bond and operator. The one entry per operator is ranked
/// by [`palw_panel_outsider_ticket_v1`], and the lowest sits.
///
/// **Excluding the registrant is not what makes the seat independent**, and must not be read as
/// the rule. It is the one party the chain KNOWS holds this class, so leaving it in would only
/// hand the registrant a draw it has not had to pay for; its other bonds are indistinguishable from
/// anybody's, and what bounds them is that they are a share of the network's population rather
/// than all of the class's.
///
/// Pure and public: a shadow reader can ask, against a state snapshot, which operator WOULD sit as
/// a claim's outsider and how often the registrant's own would.
///
/// **ADR-0152 SW-5: under `policy.stake` the outsider is the smallest stake-outsider key** over the
/// same population ([`palw_panel_stake_entries_under_v1`] with
/// [`palw_panel_stake_outsider_ticket_v1`]) — the one seat meant to be independent of the class's own
/// population would otherwise be the cheapest seat on the panel to capture — and SW-10's floor
/// holds over the base-class population minus the registrant: below it the claim does not bind
/// (`InsufficientEligibleStake`). An empty population is `NoOutsider` first, as today.
#[allow(clippy::too_many_arguments)]
pub fn palw_panel_outsider_seat_v1(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    policy: &PalwPanelDrawPolicyV1,
    independence: &PalwPanelIndependenceV1,
) -> Result<PalwPanelSeatV2, PalwPanelV2Error> {
    let (seat, floor) = palw_panel_outsider_draw_v1(
        state,
        params,
        claim_id,
        anchor_block,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        policy,
        independence,
    )?;
    if let (Some((eligible, base)), Some(stake)) = (floor, policy.stake) {
        palw_panel_stake_floor_v1(eligible, base, stake.eligible_floor_permille)?;
    }
    Ok(seat)
}

/// [`palw_panel_outsider_seat_v1`] with SW-10's floor returned rather than applied —
/// `Some((eligible weight, base weight))` under `policy.stake`, `None` otherwise — so the class draw
/// can apply it after its own operator count ([`derive_panel_v2_with_policy`]).
#[allow(clippy::too_many_arguments)]
fn palw_panel_outsider_draw_v1(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    policy: &PalwPanelDrawPolicyV1,
    independence: &PalwPanelIndependenceV1,
) -> Result<(PalwPanelSeatV2, Option<(u128, u128)>), PalwPanelV2Error> {
    palw_panel_outsider_draw_excluding_v1(
        state,
        params,
        claim_id,
        anchor_block,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        policy,
        independence,
        &[],
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn palw_panel_outsider_draw_excluding_v1(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
    policy: &PalwPanelDrawPolicyV1,
    independence: &PalwPanelIndependenceV1,
    excluded_operators: &[Hash64],
    excluded_bonds: &[PalwBondKeyV2],
) -> Result<(PalwPanelSeatV2, Option<(u128, u128)>), PalwPanelV2Error> {
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let population = palw_panel_eligible_bonds_judging_v2(
        state,
        claim_id,
        &independence.base_class_id,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        policy.readiness,
        policy.economy,
        params.seat_count,
        policy.valid_lock.and_then(|lock| lock.rcore).is_none(),
    )?;
    let registrant = state.class(&claim.class_id).and_then(|record| record.registrant_bond);
    let registrant_operator = registrant.and_then(|key| state.bond(&key)).map(|bond| bond.operator_id);
    // RFC-0006 §4.2: one operator holds at most one outsider seat of a claim, so an earlier shard's outsider is left out.
    let not_registrant = |(key, bond): &(&PalwBondKeyV2, &PalwBondStateV2)| {
        Some(**key) != registrant
            && Some(bond.operator_id) != registrant_operator
            && !excluded_operators.contains(&bond.operator_id)
            && !excluded_bonds.contains(*key)
    };
    let population: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = population
        .into_iter()
        .filter(not_registrant)
        // The route-matrix audit's #3, for the outsider too: its seat is bound like every other.
        .filter(|(key, _)| policy.valid_lock.is_none_or(|lock| lock.admits(state, key)))
        .collect();
    let Some(stake) = policy.stake else {
        return palw_panel_operator_entries_under_v1(claim_id, anchor_block, &population, palw_panel_outsider_ticket_v1)
            .into_iter()
            .next()
            .map(|entry| (PalwPanelSeatV2 { bond: entry.bond, operator_id: entry.operator_id }, None))
            .ok_or(PalwPanelV2Error::NoOutsider(*claim_id));
    };
    let seat = palw_panel_stake_entries_under_v1(claim_id, anchor_block, &population, palw_panel_stake_outsider_ticket_v1, &stake)
        .into_iter()
        .next()
        .map(|entry| PalwPanelSeatV2 { bond: entry.bond, operator_id: entry.operator_id })
        .ok_or(PalwPanelV2Error::NoOutsider(*claim_id))?;
    // SW-10 over the outsider's own population: the base class's bonds but for the load-dependent
    // filters, minus the registrant as the eligible list is.
    let base: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = palw_panel_stake_base_bonds_judging_v1(
        state,
        claim_id,
        &independence.base_class_id,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        policy.readiness,
        policy.economy,
        params.seat_count,
    )?
    .into_iter()
    .filter(not_registrant)
    .collect();
    // SW-10's executor term over the same population (M4 review finding 1): the executor operator's
    // base-class bonds, minus the registrant as the lists above are — so a genesis card's claim on
    // a bought class keeps the outsider population's tolerance of one saturated seat too.
    let executor = palw_panel_stake_executor_bonds_judging_v1(
        state,
        claim_id,
        &independence.base_class_id,
        min_collateral_sompi,
        registered_by_daa,
        capability_proof,
        policy.readiness,
        policy.economy,
        params.seat_count,
    )?
    .into_iter()
    .filter(not_registrant)
    .collect::<Vec<_>>();
    let executor_weight = palw_panel_stake_weight_v1(&executor, &stake);
    Ok((
        seat,
        Some((
            palw_panel_stake_weight_v1(&population, &stake).saturating_add(executor_weight),
            palw_panel_stake_weight_v1(&base, &stake).saturating_add(executor_weight),
        )),
    ))
}

/// **ADR-0147: an operator's ticket on a `Candidate` class's admission jury** —
/// `H(jury domain ‖ seed ‖ operator_id)`, the seed being `palw_admission_jury_seed_v1`'s (which
/// already names the class and the audit span).
pub fn palw_admission_jury_ticket_v1(seed: &Hash64, operator_id: &Hash64) -> Hash64 {
    let mut ticket = keyed(PALW_PANEL_V2_DOMAIN_ADMISSION_JURY_TICKET);
    ticket.update(seed.as_byte_slice());
    ticket.update(operator_id.as_byte_slice());
    finish(ticket)
}

/// **ADR-0147: the admission jury** — the `seats` operators of `population` with the lowest jury
/// tickets, in ticket order, one entry per operator whatever it holds (ADR-0130's rule, so a
/// registrant splitting collateral across bonds buys no second entry). Fewer operators than seats
/// is a short jury, returned short: the caller admits nothing on it.
pub fn palw_admission_jury_v1(seed: &Hash64, population: &[(&PalwBondKeyV2, &PalwBondStateV2)], seats: u16) -> Vec<Hash64> {
    let operators: std::collections::BTreeSet<Hash64> = population.iter().map(|(_, bond)| bond.operator_id).collect();
    let mut ranked: Vec<(Hash64, Hash64)> =
        operators.into_iter().map(|operator| (palw_admission_jury_ticket_v1(seed, &operator), operator)).collect();
    ranked.sort();
    ranked.into_iter().take(seats as usize).map(|(_, operator)| operator).collect()
}

/// **ADR-0147: an operator's outsider ticket on a claim** —
/// `H(outsider-ticket domain ‖ anchor ‖ claim ‖ operator_id)`. The operator's odds of sitting as the
/// outsider are the same whether it holds one eligible bond or a thousand, as in ADR-0130's draw.
pub fn palw_panel_outsider_ticket_v1(anchor_block: BlockHash, claim_id: &Hash64, operator_id: &Hash64) -> Hash64 {
    let mut ticket = keyed(PALW_PANEL_V2_DOMAIN_OUTSIDER_TICKET);
    ticket.update(anchor_block.as_byte_slice());
    ticket.update(claim_id.as_byte_slice());
    ticket.update(operator_id.as_byte_slice());
    finish(ticket)
}

/// `H(seat-ticket domain ‖ anchor ‖ claim ‖ bond)` before it is finished — the bond ticket's one
/// spelling. The deep fence's weighted draw mixes a sub-ticket index into it; everything else
/// finishes it as it is ([`palw_panel_seat_ticket_v1`]).
fn palw_panel_seat_ticket_state_v1(anchor_block: BlockHash, claim_id: &Hash64, bond_key: &PalwBondKeyV2) -> blake2b_simd::State {
    let mut ticket = keyed(PALW_PANEL_V2_DOMAIN_SEAT_TICKET);
    ticket.update(anchor_block.as_byte_slice());
    ticket.update(claim_id.as_byte_slice());
    ticket.update(&borsh::to_vec(bond_key).expect("bond keys are borsh-serializable"));
    ticket
}

/// **A bond's ticket on a claim's panel** — `H(seat-ticket domain ‖ anchor ‖ claim ‖ bond)`, the
/// unweighted draw's ticket. `anchor_block` here, and in every ticket of the draw, is the panel SEED
/// ([`PalwAnchorFactV2::panel_seed`]): past `Params::palw_panel_seed_execution` the anchor attempt's
/// execution commitment hashed with the claim, below it the anchor block. Past the panel economy it
/// no longer decides who sits; it decides which of an operator's eligible bonds the operator sits
/// with.
pub fn palw_panel_seat_ticket_v1(anchor_block: BlockHash, claim_id: &Hash64, bond_key: &PalwBondKeyV2) -> Hash64 {
    finish(palw_panel_seat_ticket_state_v1(anchor_block, claim_id, bond_key))
}

/// **ADR-0130: an operator's one lottery entry on a claim's panel** —
/// `H(operator-ticket domain ‖ anchor ‖ claim ‖ operator_id)`. Nothing about the operator's bonds
/// enters it, so an operator's odds are the same whether it holds one eligible bond or a thousand.
pub fn palw_panel_operator_ticket_v1(anchor_block: BlockHash, claim_id: &Hash64, operator_id: &Hash64) -> Hash64 {
    let mut ticket = keyed(PALW_PANEL_V2_DOMAIN_OPERATOR_TICKET);
    ticket.update(anchor_block.as_byte_slice());
    ticket.update(claim_id.as_byte_slice());
    ticket.update(operator_id.as_byte_slice());
    finish(ticket)
}

/// One operator's standing in a claim's lottery: its candidate bond (its eligible bond with the
/// lowest bond ticket, ties by bond key) and its one operator ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PalwPanelOperatorEntryV1 {
    /// Sorts first: the lottery's order.
    pub operator_ticket: Hash64,
    pub operator_id: Hash64,
    pub bond: PalwBondKeyV2,
}

/// **ADR-0130: every eligible operator's lottery entry, in lottery order.** `eligible` is
/// [`palw_panel_eligible_bonds_v2`]'s list. Each operator is entered ONCE — with its eligible bond
/// whose [`palw_panel_seat_ticket_v1`] is lowest (ties broken by the bond key) — under its
/// [`palw_panel_operator_ticket_v1`]; the entries sort by `(operator ticket, operator id)`. Pure, so a
/// shadow reader can list which operators would stand in a draw and where.
pub fn palw_panel_operator_entries_v1(
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
) -> Vec<PalwPanelOperatorEntryV1> {
    palw_panel_operator_entries_under_v1(claim_id, anchor_block, eligible, palw_panel_operator_ticket_v1)
}

/// [`palw_panel_operator_entries_v1`] under any operator ticket — ADR-0130's for the class draw,
/// ADR-0147's outsider ticket for the outsider seat. Each operator is entered once, with its
/// eligible bond whose [`palw_panel_seat_ticket_v1`] is lowest (ties broken by the bond key), and
/// the entries sort by `(operator ticket, operator id)`.
pub fn palw_panel_operator_entries_under_v1(
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
    operator_ticket: fn(BlockHash, &Hash64, &Hash64) -> Hash64,
) -> Vec<PalwPanelOperatorEntryV1> {
    let mut candidates: std::collections::BTreeMap<Hash64, (Hash64, PalwBondKeyV2)> = std::collections::BTreeMap::new();
    for (bond_key, bond) in eligible {
        let ranked = (palw_panel_seat_ticket_v1(anchor_block, claim_id, bond_key), **bond_key);
        candidates
            .entry(bond.operator_id)
            .and_modify(|best| {
                if ranked < *best {
                    *best = ranked;
                }
            })
            .or_insert(ranked);
    }
    let mut entries: Vec<PalwPanelOperatorEntryV1> = candidates
        .into_iter()
        .map(|(operator_id, (_, bond))| PalwPanelOperatorEntryV1 {
            operator_ticket: operator_ticket(anchor_block, claim_id, &operator_id),
            operator_id,
            bond,
        })
        .collect();
    entries.sort();
    entries
}

/// **ADR-0130: the draw past the panel economy — one lottery entry per operator.** The first
/// `seat_count` entries of [`palw_panel_operator_entries_v1`] sit, each with its candidate bond, in
/// lottery order (the canonical panel order `validate_panel_bound_v2_with_policy` compares). Fewer
/// eligible operators than seats is `InsufficientEligibleBonds`, exactly as the per-bond draw
/// refused a short jury.
pub fn palw_panel_operator_lottery_v1(
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    palw_panel_operator_lottery_of_v1(params.seat_count, claim_id, anchor_block, eligible)
}

/// [`palw_panel_operator_lottery_v1`] for `needed` seats — the whole panel, or (ADR-0147) the
/// class's `seat_count - 1` beside an outsider.
pub fn palw_panel_operator_lottery_of_v1(
    needed: u16,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    let entries = palw_panel_operator_entries_v1(claim_id, anchor_block, eligible);
    if entries.len() < needed as usize {
        return Err(PalwPanelV2Error::InsufficientEligibleBonds { needed, available: entries.len() as u16 });
    }
    Ok(entries
        .into_iter()
        .take(needed as usize)
        .map(|entry| PalwPanelSeatV2 { bond: entry.bond, operator_id: entry.operator_id })
        .collect())
}

// ---- ADR-0152 v3.1 SW: the stake-weighted panel draw (the pure half, §3.14) ----------------------

/// **ADR-0152 SW-3: `−log2((u + 1) / 2^64)` in Q64.64, by integer arithmetic only** — the
/// normative routine, spelled as §3.14 gives it:
///
/// ```text
/// m    = u + 1                                              // 1 ..= 2^64, as u128
/// n    = 127 − m.leading_zeros()                            // floor(log2 m), 0 ..= 64
/// y    = if n >= 63 { m >> (n − 63) } else { m << (63 − n) } // Q63, 2^63 <= y < 2^64
/// frac = 0
/// for i in 0..64 { y = (y * y) >> 63; if y >= 2^64 { y >>= 1; frac |= 1 << (63 − i) } }
/// L    = (64 << 64) − ((n << 64) | frac)
/// ```
///
/// `y < 2^64` on entry to every step, so `y * y < 2^128` never overflows; `L` runs from
/// `64 · 2^64 = 2^70` (`u = 0`) down to `0` (`u = 2^64 − 1`) and never rises as `u` rises. The
/// fraction's bits are the binary digits of `log2 y`, each truncated, so `L` sits at most a few
/// units of `2^-64` above the exact value (T85: under `1.5 × 10^-19` over its vectors, checked
/// against an 80-digit decimal logarithm; §3.14 states `4 × 10^-15` against a float `log2`, whose
/// own error dominates that figure). Only the routine's determinism is consensus: its accuracy
/// shapes the draw's law, never whether two nodes agree on it.
pub fn palw_draw_neg_log2_q64_v1(u: u64) -> u128 {
    let m = u as u128 + 1;
    let n = 127 - m.leading_zeros() as u128;
    let mut y = if n >= 63 { m >> (n - 63) } else { m << (63 - n) };
    let mut frac: u128 = 0;
    for i in 0..64u32 {
        y = (y * y) >> 63;
        if y >= 1u128 << 64 {
            y >>= 1;
            frac |= 1u128 << (63 - i);
        }
    }
    (64u128 << 64) - ((n << 64) | frac)
}

/// **ADR-0152 SW-3: a ticket's uniform draw** — the first eight bytes of the digest, little-endian.
pub fn palw_draw_ticket_u64_v1(ticket: &Hash64) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(&ticket.as_byte_slice()[..8]);
    u64::from_le_bytes(word)
}

/// **ADR-0152 SW-2: an operator's weight** from the whole-MSK posted collateral of its eligible
/// bonds (`Σ ⌊collateral / SOMPI_PER_KASPA⌋`, summed by the caller): capped at the policy's
/// `weight_cap_msk` (SW-A5: above 1,000,000 MSK an operator gains nothing by staying whole, and one
/// heavy operator cannot cut a class's room below what the cap allows, SW-A6), under the arithmetic
/// bound [`PALW_DRAW_WEIGHT_MAX_MSK_V1`], and never below 1 — every eligible operator has a key. At
/// the t12 seat floor (130,000 MSK) the floor of 1 never binds; on a network whose floor is below one
/// MSK it makes such an operator weigh what one MSK weighs rather than vanish from the race.
pub fn palw_draw_operator_weight_msk_v1(posted_msk: u128, weight_cap_msk: u64) -> u64 {
    posted_msk.min(weight_cap_msk.min(PALW_DRAW_WEIGHT_MAX_MSK_V1) as u128).max(1) as u64
}

/// **ADR-0152 SW-3: the race's order.** Operator `i` sorts before `j` iff `L_i / W_i < L_j / W_j`,
/// compared without division as `L_i · W_j < L_j · W_i` in `u128` (`L ≤ 2^70`, `W ≤ 2^40`, so each
/// product is below `2^110`); equal keys go by `operator_id`, ascending — a total order, so every
/// node sorts one population into one sequence.
pub fn palw_draw_key_cmp_v1(l_i: u128, w_i: u64, op_i: &Hash64, l_j: u128, w_j: u64, op_j: &Hash64) -> std::cmp::Ordering {
    let w_i = w_i.min(PALW_DRAW_WEIGHT_MAX_MSK_V1) as u128;
    let w_j = w_j.min(PALW_DRAW_WEIGHT_MAX_MSK_V1) as u128;
    (l_i * w_j).cmp(&(l_j * w_i)).then_with(|| op_i.cmp(op_j))
}

/// **ADR-0152 SW-3: an operator's stake ticket for a claim's class seats** —
/// `H(stake-ticket domain ‖ anchor ‖ claim ‖ operator_id)`, the same keyed BLAKE2b-512 and the same
/// three inputs as ADR-0130's [`palw_panel_operator_ticket_v1`], under its own domain.
pub fn palw_panel_stake_ticket_v1(anchor_block: BlockHash, claim_id: &Hash64, operator_id: &Hash64) -> Hash64 {
    let mut ticket = keyed(PALW_PANEL_V2_DOMAIN_STAKE_TICKET);
    ticket.update(anchor_block.as_byte_slice());
    ticket.update(claim_id.as_byte_slice());
    ticket.update(operator_id.as_byte_slice());
    finish(ticket)
}

/// **ADR-0152 SW-3/SW-5: an operator's stake ticket for a claim's ADR-0147 outsider seat** —
/// `H(stake-outsider-ticket domain ‖ anchor ‖ claim ‖ operator_id)`.
pub fn palw_panel_stake_outsider_ticket_v1(anchor_block: BlockHash, claim_id: &Hash64, operator_id: &Hash64) -> Hash64 {
    let mut ticket = keyed(PALW_PANEL_V2_DOMAIN_STAKE_OUTSIDER_TICKET);
    ticket.update(anchor_block.as_byte_slice());
    ticket.update(claim_id.as_byte_slice());
    ticket.update(operator_id.as_byte_slice());
    finish(ticket)
}

/// One operator's standing in a claim's stake race: its key `neg_log2_q64 / weight_msk` and the
/// candidate bond it sits with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelStakeEntryV1 {
    /// `L = palw_draw_neg_log2_q64_v1(u)`, Q64.64 — a function of the seed and the operator id only.
    pub neg_log2_q64: u128,
    /// `W`, SW-2's capped whole-MSK weight over the operator's eligible bonds, `>= 1`.
    pub weight_msk: u64,
    pub operator_id: Hash64,
    /// Today's candidate rule, unchanged: the operator's eligible bond with the lowest
    /// [`palw_panel_seat_ticket_v1`], ties by bond key.
    pub bond: PalwBondKeyV2,
}

impl PalwPanelStakeEntryV1 {
    /// [`palw_draw_key_cmp_v1`] on two entries.
    pub fn key_cmp(&self, other: &Self) -> std::cmp::Ordering {
        palw_draw_key_cmp_v1(
            self.neg_log2_q64,
            self.weight_msk,
            &self.operator_id,
            other.neg_log2_q64,
            other.weight_msk,
            &other.operator_id,
        )
    }
}

/// **ADR-0152 SW-2/SW-3: every eligible operator's stake entry, in race order** — smallest key
/// first. `eligible` is the draw's list for this seat (the class population, or the outsider's).
///
/// **Per operator, not per bond.** Each operator is entered once, with its candidate bond chosen
/// by today's rule, and its weight SUMS the whole-MSK posted collateral of all its bonds on the
/// list before the cap. On testnet-12 `palw_operator_id_unique` is armed at genesis, so an operator
/// is exactly one bond and the sum is that bond's collateral (SW-A3); where ids are not unique the
/// sum makes splitting stake across one operator's bonds buy nothing, as ADR-0130 made it buy
/// nothing in the lottery.
///
/// **Posted collateral, never the free stake.** The free stake decides WHETHER a bond is on the
/// list — past `palw_rcore_plus`, where the processor arms this race, through the Valid lock's
/// one-ledger seat filter (S-3's L-4b), not the panel economy's headroom; posted stake decides HOW
/// OFTEN it sits (SW-2). So an operator's key reads the seed, its own id and its own posted
/// collateral and nothing else: adding or removing any other operator, or any change to another
/// bond's commitments, never moves it (T87, T93). The first `needed` keys are a weighted sample
/// without replacement — successive sampling, the exponential race — which a walk over cumulative
/// weight intervals would not be.
pub fn palw_panel_stake_entries_under_v1(
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
    stake_ticket: fn(BlockHash, &Hash64, &Hash64) -> Hash64,
    stake: &PalwPanelStakeDrawV1,
) -> Vec<PalwPanelStakeEntryV1> {
    let mut candidates: std::collections::BTreeMap<Hash64, ((Hash64, PalwBondKeyV2), u128)> = std::collections::BTreeMap::new();
    for (bond_key, bond) in eligible {
        let ranked = (palw_panel_seat_ticket_v1(anchor_block, claim_id, bond_key), **bond_key);
        let posted = (bond.collateral / crate::constants::SOMPI_PER_KASPA) as u128;
        candidates
            .entry(bond.operator_id)
            .and_modify(|(best, sum)| {
                if ranked < *best {
                    *best = ranked;
                }
                *sum = sum.saturating_add(posted);
            })
            .or_insert((ranked, posted));
    }
    let mut entries: Vec<PalwPanelStakeEntryV1> = candidates
        .into_iter()
        .map(|(operator_id, ((_, bond), posted))| PalwPanelStakeEntryV1 {
            neg_log2_q64: palw_draw_neg_log2_q64_v1(palw_draw_ticket_u64_v1(&stake_ticket(anchor_block, claim_id, &operator_id))),
            weight_msk: palw_draw_operator_weight_msk_v1(posted, stake.weight_cap_msk),
            operator_id,
            bond,
        })
        .collect();
    entries.sort_by(|a, b| a.key_cmp(b));
    entries
}

/// [`palw_panel_stake_entries_under_v1`] for the class seats ([`palw_panel_stake_ticket_v1`]).
pub fn palw_panel_stake_entries_v1(
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
    stake: &PalwPanelStakeDrawV1,
) -> Vec<PalwPanelStakeEntryV1> {
    palw_panel_stake_entries_under_v1(claim_id, anchor_block, eligible, palw_panel_stake_ticket_v1, stake)
}

/// **Lane PL part E (ADR-0166): the class seats' race with the availability factor.** [`palw_panel_stake_entries_v1`] and, where
/// `stake.availability_at` is `Some(now)` and a `state` is given, each entry's weight scaled by its candidate bond's factor
/// ([`crate::palw_seat_availability_v1`]: 0.5x new or inactive, 1.0x normal, 1.2x long high availability; integer permille)
/// and the race re-sorted. The factor reads rooted state only (the bond's availability row at the draw's state) and `now`,
/// so every node sorts one population into one sequence; with `availability_at: None` this IS `palw_panel_stake_entries_v1`.
pub fn palw_panel_stake_entries_availability_v1(
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
    stake: &PalwPanelStakeDrawV1,
    state: Option<&PalwChainStateV2>,
) -> Vec<PalwPanelStakeEntryV1> {
    let mut entries = palw_panel_stake_entries_v1(claim_id, anchor_block, eligible, stake);
    if let (Some(now), Some(state)) = (stake.availability_at, state) {
        for entry in entries.iter_mut() {
            let factor =
                crate::palw_seat_availability_v1::palw_seat_availability_factor_v1(state.seat_availability_of_v1(&entry.bond), now);
            entry.weight_msk = crate::palw_seat_availability_v1::palw_seat_availability_scale_weight_v1(entry.weight_msk, factor);
        }
        entries.sort_by(|a, b| a.key_cmp(b));
    }
    entries
}

/// **ADR-0152 SW-10: a population's weight** — the sum, over its operators, of SW-2's capped
/// weights (the same `W` the race keys on, so the floor and the race cannot weigh one operator
/// two ways). Operators are counted once however many bonds they hold on the list.
pub fn palw_panel_stake_weight_v1(population: &[(&PalwBondKeyV2, &PalwBondStateV2)], stake: &PalwPanelStakeDrawV1) -> u128 {
    palw_panel_stake_operator_weights_v1(population.iter().map(|(_, bond)| *bond), stake).into_values().map(u128::from).sum()
}

/// **SW-10's weight, per operator** — each operator of `population` with SW-2's capped whole-MSK
/// weight over its bonds on the list; [`palw_panel_stake_weight_v1`] is the sum of the values. Lane
/// F2's census reads the map to take one operator out of a population without re-walking it.
pub fn palw_panel_stake_operator_weights_v1<'a>(
    population: impl IntoIterator<Item = &'a PalwBondStateV2>,
    stake: &PalwPanelStakeDrawV1,
) -> std::collections::BTreeMap<Hash64, u64> {
    let mut posted: std::collections::BTreeMap<Hash64, u128> = std::collections::BTreeMap::new();
    for bond in population {
        let sum = posted.entry(bond.operator_id).or_insert(0);
        *sum = sum.saturating_add((bond.collateral / crate::constants::SOMPI_PER_KASPA) as u128);
    }
    posted.into_iter().map(|(operator, sum)| (operator, palw_draw_operator_weight_msk_v1(sum, stake.weight_cap_msk))).collect()
}

/// **ADR-0152 SW-10: the eligible-stake floor**, `1000 · eligible ≥ floor‰ · base`, else
/// `InsufficientEligibleStake`. Exactly at the floor binds: seven of the eight genesis seats
/// eligible is `7/8 = 875‰`, which the review's numbers keep binding (one honest seat may be
/// saturated or offline; two may not). The draw passes both weights with SW-10's executor term
/// already added ([`palw_panel_stake_executor_bonds_judging_v1`]), so a genesis card's own claim is
/// measured over the eight as well, not over the seven that may sit.
pub fn palw_panel_stake_floor_v1(eligible: u128, base: u128, floor_permille: u16) -> Result<(), PalwPanelV2Error> {
    if eligible.saturating_mul(1000) >= base.saturating_mul(floor_permille as u128) {
        Ok(())
    } else {
        Err(PalwPanelV2Error::InsufficientEligibleStake { eligible, base })
    }
}

/// **ADR-0152 SW (v3.1): the stake-weighted draw of `needed` class seats** — the first `needed`
/// entries of [`palw_panel_stake_entries_v1`] over `eligible`, in key order (the canonical panel
/// order `validate_panel_bound_v2_with_policy` compares).
///
/// Two refusals, in this order: fewer eligible operators than `needed` is
/// `InsufficientEligibleBonds`, under exactly the operator lottery's condition (SW-9: the stake draw
/// changes who sits, never whether enough operators exist, T92); then SW-10's floor of `eligible`'s
/// weight against `base`'s is `InsufficientEligibleStake`. `base` is
/// [`palw_panel_stake_base_bonds_judging_v1`]'s list for the same claim, with the same per-claim
/// exclusions the caller applied to `eligible`.
pub fn palw_panel_stake_race_of_v1(
    needed: u16,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
    base: &[(&PalwBondKeyV2, &PalwBondStateV2)],
    stake: &PalwPanelStakeDrawV1,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    palw_panel_stake_race_with_v1(needed, claim_id, anchor_block, eligible, base, 0, stake, None, None)
}

/// [`palw_panel_stake_race_of_v1`] with an outsider's floor `(eligible, base)` checked between the
/// operator count and the class's own floor, and SW-10's executor term `executor_weight`
/// ([`palw_panel_stake_executor_bonds_judging_v1`]'s capped weight, `0` where the executor could not
/// sit) added to both sides of the class's floor — the reported `InsufficientEligibleStake` weights
/// include it.
#[allow(clippy::too_many_arguments)]
fn palw_panel_stake_race_with_v1(
    needed: u16,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    eligible: &[(&PalwBondKeyV2, &PalwBondStateV2)],
    base: &[(&PalwBondKeyV2, &PalwBondStateV2)],
    executor_weight: u128,
    stake: &PalwPanelStakeDrawV1,
    outsider_floor: Option<(u128, u128)>,
    availability: Option<&PalwChainStateV2>,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    let entries = palw_panel_stake_entries_availability_v1(claim_id, anchor_block, eligible, stake, availability);
    if entries.len() < needed as usize {
        return Err(PalwPanelV2Error::InsufficientEligibleBonds { needed, available: entries.len() as u16 });
    }
    if let Some((outsider_eligible, outsider_base)) = outsider_floor {
        palw_panel_stake_floor_v1(outsider_eligible, outsider_base, stake.eligible_floor_permille)?;
    }
    palw_panel_stake_floor_v1(
        palw_panel_stake_weight_v1(eligible, stake).saturating_add(executor_weight),
        palw_panel_stake_weight_v1(base, stake).saturating_add(executor_weight),
        stake.eligible_floor_permille,
    )?;
    Ok(entries
        .into_iter()
        .take(needed as usize)
        .map(|entry| PalwPanelSeatV2 { bond: entry.bond, operator_id: entry.operator_id })
        .collect())
}

// ---- lane F2 (post-launch, 2026-09-27): a stake draw refused for eligibility at every seed --------

/// **Lane F2 (`Params::palw_floor_refusal_retry`): one draw point of an anchor block** — the inputs the
/// processor's derivation resolves at a claim's anchor fact, for the claims whose anchor slot falls
/// at or below `max_slot` and above the previous point's. `draw_daa` is the fact's `anchor_daa`, the
/// DAA every DAA-keyed draw input is read at: the anchor block's own below lane A, past it the operator
/// attempt the claim's seed is read off (`palw_operator_seed_source_v1`), so one anchor block that
/// merges a displaced operator attempt has two points. Carried on
/// `PalwTransitionExtrasV1::sw8_draw` so the fold asks the draw's own question on the draw's own inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSw8DrawPointV1 {
    /// The highest anchor slot this point draws.
    pub max_slot: u64,
    /// The anchor fact's `anchor_daa` for those slots.
    pub draw_daa: u64,
    /// `palw_panel_draw_policy_at(draw_daa)`, `valid_lock` left `None`: the lock is the BINDING
    /// block's, priced per claim ([`crate::palw_state_v2::palw_panel_valid_lock_v1`]).
    pub policy: PalwPanelDrawPolicyV1,
    /// `Params::palw_bond_maturity`'s window in force at `draw_daa`, before the second clock widens it
    /// (the processor's `palw_bond_maturity_at`).
    pub bond_maturity_window: Option<u64>,
    /// The second clock's RAW depth at `draw_daa` (`None` below `palw_audit_2026_09_23`).
    pub settled_anchor_depth: Option<u64>,
    /// ADR-0071 SA-3 at `draw_daa`.
    pub capability_proof: bool,
    /// ADR-0100's per-shard licensing at `draw_daa`: a class with a shard plan draws stratified there,
    /// and this lane never retries a stratified draw.
    pub shard_licensing: bool,
}

/// **Lane F2: an anchor block's draw points and the panel they draw** — `Some` on
/// `PalwTransitionExtrasV1::sw8_draw` only where `sw8_anchor_delay` is and the floor-refusal retry is in
/// force at the block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwSw8DrawInputsV1 {
    /// `PalwPanelParamsV2::seat_count`.
    pub seat_count: u16,
    /// Ascending `max_slot`; the last one's is the block's reach.
    pub points: Vec<PalwSw8DrawPointV1>,
}

impl PalwSw8DrawInputsV1 {
    /// The point that draws a claim whose anchor slot is `slot` — the first whose `max_slot` is at or
    /// above it — and its index; `None` past the block's reach.
    pub fn point_of(&self, slot: u64) -> Option<(usize, &PalwSw8DrawPointV1)> {
        self.points.iter().enumerate().find(|(_, point)| slot <= point.max_slot)
    }
}

/// One bond of a census: what every list of the draw asks of it, whatever the claim.
struct PalwStakeCensusBondV1<'a> {
    key: &'a PalwBondKeyV2,
    bond: &'a PalwBondStateV2,
    /// `reserved_exposure + registration_exposure` — the economy's headroom question reads it.
    backed: u128,
    /// The lock's room ([`PalwPanelValidLockV1::room_v1`]); `None` where no lock is armed.
    room: Option<u128>,
}

/// What decides one claim's verdict on a census: claims that agree on it share it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PalwStakeCensusKeyV1 {
    root: Option<Hash64>,
    class_id: Hash64,
    executor: PalwBondKeyV2,
    governed: bool,
    threshold: Option<u128>,
    seat_stake: Option<u128>,
}

/// **Lane F2: one anchor block's census of the stake draw's population, at one draw point, and the
/// draw's eligibility verdict from it** (`Params::palw_floor_refusal_retry`).
///
/// [`derive_panel_v2_with_policy`] under `policy.stake` refuses for eligibility in exactly three ways —
/// no eligible outsider (`NoOutsider`), fewer eligible operators than seats after the load filters
/// (`InsufficientEligibleBonds`), and SW-10's floor over the class's or the outsider's population
/// (`InsufficientEligibleStake`) — and none of them reads the seed, save through WHICH operator sits as
/// an outsider-judged claim's outsider (the class's lists are drawn without it). So "refuses at every
/// seed" is decidable without a ticket: for a claim with no outsider it is the draw's own refusal; for
/// an outsider-judged one it is an empty outsider population, the outsider's floor, or — for EVERY
/// operator that could sit as the outsider — the class's count or floor without that operator.
/// [`Self::refuses_at_every_seed`] answers exactly that, so a `true` names a claim no seed could have
/// seated: there was no panel for anyone, the anchor producer included, to see and discard.
///
/// **The cost is the census, once per anchor block** (per draw point: one past the operator anchor
/// only when the block merges a displaced operator attempt). [`Self::new`] walks the bonds once,
/// taking each bond's floor test, exposure and lock room — the lock room is the one-ledger
/// `palw_bond_committed_v1` walk of the bond's locks, the only per-bond cost that is not a lookup — and
/// each class's seat predicate (`palw_bond_may_judge_class_v4`) is taken once, on first use. A claim
/// then costs one pass of integer filters over the census and two operator maps, and claims that
/// agree on class, executor, outsider rule and prices share one verdict. The draw itself pays the lock
/// walk once per bond per list per claim (three lists, six with an outsider).
pub struct PalwStakeDrawCensusV1<'a> {
    state: &'a PalwChainStateV2,
    seat_count: u16,
    policy: PalwPanelDrawPolicyV1,
    maturity_floor: Option<u64>,
    capability_proof: bool,
    shard_licensing: bool,
    frame: Option<PalwPanelValidLockV1>,
    bonds: Vec<PalwStakeCensusBondV1<'a>>,
    judges: std::collections::BTreeMap<(Hash64, Option<Hash64>), Vec<bool>>,
    verdicts: std::collections::BTreeMap<PalwStakeCensusKeyV1, bool>,
}

impl<'a> PalwStakeDrawCensusV1<'a> {
    /// The census of `state` for a draw at `point`: `maturity_floor` is the draw's
    /// `palw_seat_maturity_floor_v1(point.draw_daa, window)` on `state`, and `frame` the binding block's
    /// lock with its prices zeroed ([`PalwPanelValidLockV1::frame_v1`]; `None` where no lock is armed).
    pub fn new(
        state: &'a PalwChainStateV2,
        seat_count: u16,
        min_collateral_sompi: u64,
        point: &PalwSw8DrawPointV1,
        maturity_floor: Option<u64>,
        frame: Option<PalwPanelValidLockV1>,
    ) -> Self {
        let policy = PalwPanelDrawPolicyV1 { valid_lock: None, ..point.policy };
        // The seat floor every list asks first (`palw_panel_bonds_judging_v1`): the panel's past the
        // economy, the registry's below it.
        let floor = policy.economy.map(|economy| economy.panel_floor_sompi).unwrap_or(min_collateral_sompi);
        let bonds = state
            .bonds_iter()
            .filter(|(_, bond)| crate::palw_state_v2::palw_bond_may_take_work_v2(bond, floor))
            .map(|(key, bond)| PalwStakeCensusBondV1 {
                key,
                bond,
                backed: state.reserved_exposure(key).saturating_add(state.registration_exposure(key)),
                room: frame.and_then(|lock| lock.room_v1(state, key)),
            })
            .collect();
        Self {
            state,
            seat_count,
            policy,
            maturity_floor,
            capability_proof: point.capability_proof,
            shard_licensing: point.shard_licensing,
            frame: frame.map(|lock| lock.frame_v1()),
            bonds,
            judges: std::collections::BTreeMap::new(),
            verdicts: std::collections::BTreeMap::new(),
        }
    }

    /// Each census bond's seat predicate for `class_id`, taken once.
    fn ensure_judges(&mut self, class_id: &Hash64, root: Option<&Hash64>) {
        if self.judges.contains_key(&(*class_id, root.copied())) {
            return;
        }
        let (state, capability, readiness) = (self.state, self.capability_proof, self.policy.readiness);
        let judged = self
            .bonds
            .iter()
            .map(|b| crate::palw_state_v2::palw_bond_may_judge_class_v5(state, b.key, b.bond, class_id, root, capability, readiness))
            .collect();
        self.judges.insert((*class_id, root.copied()), judged);
    }

    /// **Would `claim_id`'s stake draw refuse for ELIGIBILITY at every seed?** `lock` is the claim's own
    /// Valid lock at the binding block (`crate::palw_state_v2::palw_panel_valid_lock_v1`), which must be
    /// of this census's frame. `false` — never a retry — below the stake draw, for a claim the state
    /// does not hold or whose executor bond it does not (`SeatBondMissing`), for a stratified draw, and
    /// for a lock of another frame.
    pub fn refuses_at_every_seed(&mut self, claim_id: &Hash64, lock: Option<&PalwPanelValidLockV1>) -> bool {
        let Some(stake) = self.policy.stake else { return false };
        let state = self.state;
        let Some(claim) = state.claim(claim_id) else { return false };
        if self.shard_licensing && state.class_shard_plan(&claim.class_id).is_some() {
            return false;
        }
        if lock.map(PalwPanelValidLockV1::frame_v1) != self.frame {
            return false;
        }
        let Some(executor) = state.bond(&claim.bond) else { return false };
        let independence = self.policy.independence.filter(|independence| independence.governs(claim));
        // The economy's headroom is asked only where the one-ledger filter is not
        // (`derive_panel_v2_with_policy`'s `headroom`), of the seat stake THIS claim prices.
        let headroom = self.policy.economy.filter(|_| self.frame.and_then(|lock| lock.rcore).is_none()).map(|economy| {
            (
                economy.max_exposure_ratio_permille,
                economy.seat_exposure(claim.reserved, claim.escrowed_reward, self.seat_count as usize),
            )
        });
        // Lane MU (ADR-0173): the root the claim named, where the draw is root-keyed — claims of one class that name different roots
        // do not share a verdict.
        let claim_root = self.policy.readiness.filter(|policy| policy.root_keyed).and_then(|_| state.claim_root(claim_id));
        let key = PalwStakeCensusKeyV1 {
            root: claim_root,
            class_id: claim.class_id,
            executor: claim.bond,
            governed: independence.is_some(),
            threshold: lock.map(PalwPanelValidLockV1::threshold_v1),
            seat_stake: headroom.map(|(_, seat_stake)| seat_stake),
        };
        if let Some(verdict) = self.verdicts.get(&key) {
            return *verdict;
        }
        let outsider = independence
            .filter(|independence| crate::palw_state_v2::palw_claim_is_outsider_judged_v1(state, claim, Some(independence.from_daa)));
        self.ensure_judges(&claim.class_id, claim_root.as_ref());
        if let Some(independence) = outsider {
            self.ensure_judges(&independence.base_class_id, None);
        }
        let registered_by = match independence {
            Some(independence) => Some(independence.registered_by_daa(self.maturity_floor)),
            None => self.maturity_floor,
        };
        let bonds = &self.bonds;
        // `palw_panel_bonds_judging_v1`'s predicates, over the census: the floor was taken at `new`.
        let structural = |judges: &[bool], i: usize| judges[i] && registered_by.is_none_or(|by| bonds[i].bond.registered_daa <= by);
        let not_executor = |b: &PalwStakeCensusBondV1| {
            !(*b.key == claim.bond || b.bond.operator_id == executor.operator_id || b.bond.pubkey == executor.pubkey)
        };
        let has_headroom = |b: &PalwStakeCensusBondV1| {
            headroom.is_none_or(|(ratio, seat_stake)| {
                crate::palw_panel_economy_v1::palw_seat_has_headroom_v1(b.bond.collateral, b.backed, seat_stake, ratio)
            })
        };
        let threshold = key.threshold;
        let admits = |b: &PalwStakeCensusBondV1| threshold.is_none_or(|threshold| b.room.is_some_and(|room| threshold <= room));
        // The three lists of one judged class: the eligible list (every predicate, the lock applied),
        // SW-10's base (no load filter) and its executor term (the executor clause inverted).
        let lists = |judges: &[bool], keep: &dyn Fn(&PalwStakeCensusBondV1) -> bool| {
            let mut eligible = Vec::new();
            let mut base = Vec::new();
            let mut executor_term = Vec::new();
            for (i, b) in bonds.iter().enumerate() {
                if !structural(judges, i) || !keep(b) {
                    continue;
                }
                if b.bond.operator_id == executor.operator_id {
                    executor_term.push(b.bond);
                }
                if not_executor(b) {
                    base.push(b.bond);
                    if has_headroom(b) && admits(b) {
                        eligible.push(b.bond);
                    }
                }
            }
            (
                palw_panel_stake_operator_weights_v1(eligible, &stake),
                palw_panel_stake_operator_weights_v1(base, &stake),
                palw_panel_stake_operator_weights_v1(executor_term, &stake).into_values().map(u128::from).sum::<u128>(),
            )
        };
        let sum = |weights: &std::collections::BTreeMap<Hash64, u64>| weights.values().copied().map(u128::from).sum::<u128>();
        let floor_holds =
            |eligible: u128, base: u128| palw_panel_stake_floor_v1(eligible, base, stake.eligible_floor_permille).is_ok();
        let (eligible, base, executor_weight) = lists(&self.judges[&(claim.class_id, claim_root)], &|_| true);
        let verdict = match outsider {
            // No outsider: the draw's own refusal, which reads no seed.
            None => {
                eligible.len() < self.seat_count as usize
                    || !floor_holds(sum(&eligible).saturating_add(executor_weight), sum(&base).saturating_add(executor_weight))
            }
            Some(independence) => {
                let registrant = state.class(&claim.class_id).and_then(|record| record.registrant_bond);
                let registrant_operator = registrant.and_then(|key| state.bond(&key)).map(|bond| bond.operator_id);
                let not_registrant =
                    |b: &PalwStakeCensusBondV1| Some(*b.key) != registrant && Some(b.bond.operator_id) != registrant_operator;
                let (outsiders, outsider_base, outsider_executor) =
                    lists(&self.judges[&(independence.base_class_id, None)], &not_registrant);
                let needed = self.seat_count.saturating_sub(1) as usize;
                let (eligible_weight, base_weight) = (sum(&eligible), sum(&base));
                // No outsider (`NoOutsider`), or the outsider's own floor — whoever sits — refuses;
                // else the class draw must refuse without EVERY operator that could sit as the outsider.
                outsiders.is_empty()
                    || !floor_holds(
                        sum(&outsiders).saturating_add(outsider_executor),
                        sum(&outsider_base).saturating_add(outsider_executor),
                    )
                    || outsiders.keys().all(|operator| {
                        let count = eligible.len() - usize::from(eligible.contains_key(operator));
                        let without = |weights: &std::collections::BTreeMap<Hash64, u64>, total: u128| {
                            total - weights.get(operator).copied().map_or(0, u128::from)
                        };
                        count < needed
                            || !floor_holds(
                                without(&eligible, eligible_weight).saturating_add(executor_weight),
                                without(&base, base_weight).saturating_add(executor_weight),
                            )
                    })
            }
        };
        self.verdicts.insert(key, verdict);
        verdict
    }
}

/// **ADR-0152 SW-9: the rate room's effective ready count** —
/// `min(ready operators, max(seat_count, ⌊ΣW / W_max⌋))` over the class's ready operators' SW-2
/// weights (capped, [`palw_draw_operator_weight_msk_v1`]); `0` with no ready operator.
///
/// Under the stake draw the replay load lands on the heaviest operators: with equal compute per
/// operator, capacity is set by the largest inclusion probability, and under successive sampling
/// `π_max ≤ min(1, seat_count · W_max / ΣW)`, so `ΣW / W_max` operators' worth of capacity is a
/// lower bound (the room errs toward refusing). Wired by M4: the fold's `panel_rate_v1` /
/// `panel_room_v1` and op 186 read it past `palw_rcore_plus` for every class outside C7, through
/// [`palw_panel_ready_eff_of_bonds_v1`] and [`crate::palw_state_v2::palw_panel_room_ready_count_v1`]
/// (T91).
/// Counting operators also removes the over-count of an operator's several ready bonds.
///
/// The cap is what bounds the lever SW-A6 found: uncapped, one ready 20M operator beside the eight
/// genesis seats gave `5`; capped at 1,000,000 MSK it gives `8`.
pub fn palw_panel_ready_eff_v1(ready_operator_weights: &[u64], seat_count: u64) -> u64 {
    let ready = ready_operator_weights.len() as u64;
    let Some(heaviest) = ready_operator_weights.iter().copied().max() else {
        return 0;
    };
    let total: u128 = ready_operator_weights.iter().map(|w| *w as u128).sum();
    // Weights are >= 1 by SW-2; a list of zeros is read as equal weights rather than divided by.
    let spread = if heaviest == 0 { ready } else { (total / heaviest as u128).min(ready as u128) as u64 };
    ready.min(seat_count.max(spread))
}

/// **ADR-0152 SW-9 (M4): [`palw_panel_ready_eff_v1`] over a class's ready BONDS** — grouped by
/// operator, each operator's whole-MSK posted collateral summed over its ready bonds and capped by
/// SW-2's [`palw_draw_operator_weight_msk_v1`] (the weight the draw keys on, so the room and the draw
/// cannot weigh one operator two ways), then `min(ready operators, max(seat_count, ⌊ΣW / W_max⌋))`.
///
/// The caller hands in the bonds its own readiness predicate calls ready — the fold's
/// (`model_registry_seat_is_ready`) or op 186's (`palw_model_registry_room_ready_v1`) — and decides
/// WHETHER to count this way at all through
/// [`crate::palw_state_v2::palw_panel_room_ready_count_v1`]; this function only counts. On
/// testnet-12 an operator is one bond (`palw_operator_id_unique`), so the grouping only removes the
/// over-count where ids are not unique.
pub fn palw_panel_ready_eff_of_bonds_v1<'a>(
    ready_bonds: impl IntoIterator<Item = &'a PalwBondStateV2>,
    seat_count: u64,
    stake: &PalwPanelStakeDrawV1,
) -> u64 {
    let mut posted: std::collections::BTreeMap<Hash64, u128> = std::collections::BTreeMap::new();
    for bond in ready_bonds {
        let sum = posted.entry(bond.operator_id).or_insert(0);
        *sum = sum.saturating_add((bond.collateral / crate::constants::SOMPI_PER_KASPA) as u128);
    }
    let weights: Vec<u64> = posted.into_values().map(|sum| palw_draw_operator_weight_msk_v1(sum, stake.weight_cap_msk)).collect();
    palw_panel_ready_eff_v1(&weights, seat_count)
}

/// May THIS `PanelBound` object be accepted at THIS chain point? Everything is recomputed:
/// the claim's phase, the anchor slot, the binding window, and the panel itself — a proposed
/// panel is either exactly the derived one, in the derived order, or it is refused.
pub fn validate_panel_bound_v2(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    anchor: &PalwAnchorFactV2,
    proposed_anchor: Hash64,
    proposed_seats: &[PalwPanelSeatV2],
) -> Result<(), PalwPanelV2Error> {
    validate_panel_bound_v2_with_maturity(state, params, state_params, ctx, claim_id, anchor, proposed_anchor, proposed_seats, None)
}

/// [`validate_panel_bound_v2`] with ADR-0065 D1's seat maturity.
///
/// `bond_maturity_daa` is `Params::palw_bond_maturity`'s window resolved at this block, and the
/// floor it implies is derived HERE from the claim's own anchor rather than by the caller — the
/// acceptance layer and the assembler must not be able to subtract differently.
#[allow(clippy::too_many_arguments)]
pub fn validate_panel_bound_v2_with_maturity(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    anchor: &PalwAnchorFactV2,
    proposed_anchor: Hash64,
    proposed_seats: &[PalwPanelSeatV2],
    bond_maturity_daa: Option<u64>,
) -> Result<(), PalwPanelV2Error> {
    validate_panel_bound_v2_with_capability_proof(
        state,
        params,
        state_params,
        ctx,
        claim_id,
        anchor,
        proposed_anchor,
        proposed_seats,
        bond_maturity_daa,
        false,
        false,
    )
}

/// [`validate_panel_bound_v2_with_maturity`] with **ADR-0071 SA-3's production proof**.
///
/// `capability_proof` is `Params::palw_capability_bound` resolved at this block, and it must be
/// the value the assembler used — a `PanelBound` is accepted only if it equals the panel this
/// function derives, so an acceptance layer and a producer that disagree about the fence refuse
/// every panel the other builds. `false` — every shipped preset — is byte-identical to the check
/// before the parameter existed.
#[allow(clippy::too_many_arguments)]
pub fn validate_panel_bound_v2_with_capability_proof(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    anchor: &PalwAnchorFactV2,
    proposed_anchor: Hash64,
    proposed_seats: &[PalwPanelSeatV2],
    bond_maturity_daa: Option<u64>,
    capability_proof: bool,
    weighted: bool,
) -> Result<(), PalwPanelV2Error> {
    validate_panel_bound_v2_with_shards(
        state,
        params,
        state_params,
        ctx,
        claim_id,
        anchor,
        proposed_anchor,
        proposed_seats,
        bond_maturity_daa,
        capability_proof,
        weighted,
        None,
    )
}

/// [`validate_panel_bound_v2_with_capability_proof`] for a claim whose class may be sharded
/// (ADR-0100 Decision 4): `stratified` is `Some(shard_count)` exactly when the caller's
/// one-place decision says this claim's panel is drawn per shard, and the recomputation is then
/// [`derive_stratified_panel_v2`]'s — the same function the binding came from.
#[allow(clippy::too_many_arguments)]
pub fn validate_panel_bound_v2_with_shards(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    anchor: &PalwAnchorFactV2,
    proposed_anchor: Hash64,
    proposed_seats: &[PalwPanelSeatV2],
    bond_maturity_daa: Option<u64>,
    capability_proof: bool,
    // **C-02 (deep fence): `Params::palw_audit_2026_09_11_deep` resolved at the anchor**, so the
    // acceptance layer recomputes the stake-weighted draw the assembler built and demands exact
    // equality. Resolved at the anchor for the same reason as `capability_proof`: the panel is a pure
    // function of the claim, so a flag read at the binding block would make the derived panel change
    // block to block and refuse a `PanelBound` that missed its block. `false` below the fence.
    weighted: bool,
    stratified: Option<u32>,
) -> Result<(), PalwPanelV2Error> {
    validate_panel_bound_v2_with_policy(
        state,
        params,
        state_params,
        ctx,
        claim_id,
        anchor,
        proposed_anchor,
        proposed_seats,
        bond_maturity_daa,
        capability_proof,
        PalwPanelDrawPolicyV1 { weighted, economy: None, readiness: None, independence: None, valid_lock: None, stake: None },
        stratified,
    )
}

/// [`validate_panel_bound_v2_with_shards`] with the whole draw policy (ADR-0124), resolved at the
/// ANCHOR by the caller for the reason every field of it gives: the panel is a pure function of
/// the claim, and the acceptance layer must recompute exactly the panel the assembler built.
#[allow(clippy::too_many_arguments)]
pub fn validate_panel_bound_v2_with_policy(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    anchor: &PalwAnchorFactV2,
    proposed_anchor: Hash64,
    proposed_seats: &[PalwPanelSeatV2],
    bond_maturity_daa: Option<u64>,
    capability_proof: bool,
    policy: PalwPanelDrawPolicyV1,
    stratified: Option<u32>,
) -> Result<(), PalwPanelV2Error> {
    validate_panel_bound_v2_with_tir_shard_v1(
        state,
        params,
        state_params,
        ctx,
        claim_id,
        anchor,
        proposed_anchor,
        proposed_seats,
        bond_maturity_daa,
        capability_proof,
        policy,
        stratified,
        None,
    )
}

/// [`validate_panel_bound_v2_with_policy`] for a class that may declare a layer-shard plan (RFC-0006): `tir_shard` is
/// `Some(S_L)` exactly when the caller's one-place decision says this claim's panel is drawn per layer shard, and the
/// recomputation is then [`derive_tir_shard_panel_v1`]'s — the same function the binding came from.
#[allow(clippy::too_many_arguments)]
pub fn validate_panel_bound_v2_with_tir_shard_v1(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    anchor: &PalwAnchorFactV2,
    proposed_anchor: Hash64,
    proposed_seats: &[PalwPanelSeatV2],
    bond_maturity_daa: Option<u64>,
    capability_proof: bool,
    policy: PalwPanelDrawPolicyV1,
    stratified: Option<u32>,
    // RFC-0006: `Some(S_L)` exactly when the claim's class declared a layer-shard plan and the fence is in force at the anchor.
    tir_shard: Option<u16>,
) -> Result<(), PalwPanelV2Error> {
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    if !matches!(claim.phase, PalwClaimPhaseV2::Provisional) {
        return Err(PalwPanelV2Error::WrongPhase { claim: *claim_id, edge: "PanelBound" });
    }

    // The anchor slot: first chain block at or past accepted + delay. "First" is the predecessor
    // still being short of the slot.
    // `bind_base_daa`, not `accepted_daa`: a claim revived by a receipt timeout anchors its
    // second panel on the sweep, which is the whole reason the redraw deals different seats.
    let slot = claim
        .bind_base_daa()
        .checked_add(params.anchor_delay)
        .ok_or(PalwPanelV2Error::AnchorMismatch("anchor slot overflows the DAA score"))?;
    if anchor.anchor_daa < slot {
        return Err(PalwPanelV2Error::AnchorMismatch("the named anchor sits before the claim's anchor slot"));
    }
    if anchor.predecessor_daa >= slot {
        return Err(PalwPanelV2Error::AnchorMismatch("the named anchor is not the FIRST block at the slot"));
    }
    // **Lane F1, the panel seed** ([`PalwAnchorFactV2::panel_seed`]): for a claim anchored past
    // `Params::palw_panel_seed_execution` the processor's one anchor walk
    // (`palw_v2_anchor_fact_of_candidate`) supplies the anchor ATTEMPT's execution commitment —
    // resolved at the ANCHOR's DAA, where this gate's policy is resolved too, and supplied to the
    // chain's own derivation by the same walk — and the panel is keyed on `H(execution ‖ claim)`,
    // never on the anchor block's re-signable identity. Below it (`anchor_execution: None`) the seed
    // is the anchor block, byte for byte. The object names the SEED it was drawn from, and the fold
    // stores it: the segment assignment and the S3 sample read it back from there.
    let seed = anchor.panel_seed(claim_id);
    if proposed_anchor != seed {
        return Err(PalwPanelV2Error::AnchorMismatch(if anchor.anchor_execution.is_some() {
            "the object's anchor is not the panel seed of the chain's anchor attempt"
        } else {
            "the object's anchor is not the chain's anchor block"
        }));
    }

    // Binding window: not before the anchor exists, not past the bind deadline (the sweep will
    // void at the deadline anyway; refusing here names the reason at acceptance).
    if ctx.daa_score < anchor.anchor_daa {
        return Err(PalwPanelV2Error::BindOutsideWindow("a panel cannot bind before its anchor exists"));
    }
    // `bind_base_daa()`, matching the anchor slot ten lines up: a redrawn claim's second panel
    // binds inside the window that starts at the REDRAW. Dating this from `accepted_daa` made
    // the redraw inert — the second bind is by construction past `accepted_daa + window_bind`
    // on every shipped bundle, so every revived claim was refused here and voided anyway.
    let deadline = claim
        .bind_base_daa()
        .checked_add(state_params.window_bind())
        .ok_or(PalwPanelV2Error::BindOutsideWindow("bind deadline overflows the DAA score"))?;
    if ctx.daa_score > deadline {
        return Err(PalwPanelV2Error::BindOutsideWindow("the bind window has already lapsed"));
    }
    // **ADR-0152 SW-8: under the stake draw a panel binds only in its own anchor block** (a
    // redraw's panel in the redraw's anchor block). The draw's inputs are the ONE state the anchor
    // block's acceptance reads — its pre-object base, advanced by its earlier bindings in claim-id
    // order — so a `PanelBound` carried by any later block would be a second draw of the same seed on
    // a state someone had time to shape: the retry path (a failed or dropped draw re-derived after
    // the attacker retired its own seated Sybil, the draft's relabelled 8.97M) that this rule closes.
    // A claim not bound here stays `Provisional` and voids `BindTimeout` at its bind deadline (S0: no
    // forfeit; a free-prompt claim keeps its abandon hold). Keyed on `policy.stake`, which the
    // processor's one resolver (`palw_panel_draw_policy_at`) sets iff `palw_rcore_plus` is active at
    // the ANCHOR, like every other draw rule; `None` — testnet-11, devnet, mainnet and testnet-12 with
    // the fence off — never reaches this test, so their windows are the ones above, byte for byte.
    if policy.stake.is_some() && ctx.block != anchor.anchor_block {
        return Err(PalwPanelV2Error::BindOutsideWindow(
            "under the stake-weighted draw a panel binds only in its own anchor block (ADR-0152 SW-8)",
        ));
    }

    let registered_by_daa = palw_seat_maturity_floor_v1(anchor.anchor_daa, bond_maturity_daa);
    let derived = match tir_shard {
        // **RFC-0006: the per-shard draw of an IR claim**, the same function the binding came from.
        Some(s_l) => derive_tir_shard_panel_v1(
            state,
            params,
            claim_id,
            seed,
            state_params.min_collateral_sompi(),
            registered_by_daa,
            capability_proof,
            policy,
            s_l,
        )?,
        None => match stratified {
            // **The stratified (shard) draw is not stake-weighted yet.** It is dormant on every network
            // (`palw_shard_court` / `palw_kary_court` are `None` everywhere, so `stratified` is always
            // `None` here), so C-02's weighting rides the flat draw below. When the shard court is armed,
            // `derive_shard_panel_v1`'s ticketing needs the same bucketing — a follow-up gated behind the
            // shard fence, not this one. `weighted` is deliberately not forwarded here.
            Some(shard_count) => derive_stratified_panel_v2(
                state,
                params,
                claim_id,
                seed,
                state_params.min_collateral_sompi(),
                registered_by_daa,
                capability_proof,
                shard_count,
            )?,
            None => derive_panel_v2_with_policy(
                state,
                params,
                claim_id,
                seed,
                state_params.min_collateral_sompi(),
                registered_by_daa,
                capability_proof,
                policy,
            )?,
        },
    };
    if derived != proposed_seats {
        return Err(PalwPanelV2Error::PanelMismatch);
    }
    Ok(())
}

/// A seat's verdict on one claim (Decision 7's third and fourth DA states, kept distinct).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwReceiptVerdictV2 {
    /// The trace was served, opened against the committed roots, and verified.
    Valid,
    /// The producer failed to serve the committed data. NOT a no-show — the seat answered; it is
    /// the producer who defaulted.
    ///
    /// It names the obligation it says went unmet (audit C5). On-chain nothing can prove that a
    /// byte was not sent; what a rule CAN require is that the accusation be specific and fall
    /// inside an obligation the producer actually had. A bare `Unavailable` was neither — it
    /// accused nothing in particular, at no time in particular, and a quorum of them voided an
    /// honest claim on an assertion with no content.
    Unavailable {
        /// Which chunk of the committed trace manifest was requested. Must be one the attempt
        /// committed to (`< claim.trace_chunk_count`).
        chunk_index: u32,
        /// When it was requested. Must fall inside the producer's retention obligation and not
        /// after the seat signed.
        requested_daa: u64,
    },
    /// **This seat does not hold the class and cannot judge the claim either way.**
    ///
    /// Sortition ignores which classes a node can execute, so a seat routinely lands on a family
    /// it does not have. With only the two verdicts above, such a seat had no honest move: `Valid`
    /// would be a lie, `Unavailable` is a signed accusation against a producer that did nothing
    /// wrong, and silence is charged as a no-show — every road ended in a slash for the offence of
    /// being picked. This is the honest answer, and it is free.
    ///
    /// It counts toward neither side: a seat that cannot judge does not get to decide. And it is
    /// refused on the liveness floor, where no node can truthfully claim it.
    Incapable,
    /// **ADR-0152 v3.1 Q-1: the seat held its S3 sites and recomputed them with no mismatch.**
    ///
    /// Audit, not verification. It counts toward no quorum and no coverage count, takes no lock and
    /// carries no liability (`PanelFalseValidV2` convicts only a `Valid`), and it is not "served"
    /// for SR-1's release (C1): its sites are a public function of the bind, so a producer can serve
    /// an honest seat exactly those and withhold the rest. It is credited for seat pay as a `Valid`
    /// is (§9.1 Q3), is never dissent-slashed, and does not satisfy ADR-0147's outsider veto.
    ///
    /// Appended: Borsh index 3, signed under message tag 4. Refused below `Params::palw_rcore_plus`
    /// by the acceptance layer and by the fold, so no other network's receipt sets change.
    Sampled,
}

/// One seat's signed receipt.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSeatReceiptV2 {
    pub claim: Hash64,
    pub verdict: PalwReceiptVerdictV2,
    pub seat_bond: PalwBondKeyV2,
    /// When the seat answered. Inside the signed message, and checked against the receipt
    /// window: a duty with no deadline is a duty a seat can discharge whenever it suits it, and
    /// `Unavailable` with no deadline is an accusation that can be minted after the fact.
    pub signed_daa: u64,
    pub signature: Vec<u8>,
}

/// **ADR-0133 Verification V2: a receipt that names the segments it attests.** The V2 receipt as it
/// is, plus the mask, signed together over `palw_receipt_message_v3` so a relayer cannot widen or
/// narrow what a seat said it replayed. The full mask is a full attestation (V1's receipt).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSeatReceiptV3 {
    pub receipt: PalwSeatReceiptV2,
    pub segments: crate::palw_verification_v2::PalwSegmentMaskV2,
}

/// `H(network_domain ‖ claim ‖ verdict)` — what a seat signs, in this family's own message
/// domain (the signing CONTEXT is [`PALW_RECEIPT_V2_MLDSA87_CONTEXT`], applied by the verifier
/// call, never caller-chosen).
pub fn palw_receipt_message_v2(network_domain: Hash64, claim: Hash64, verdict: PalwReceiptVerdictV2, signed_daa: u64) -> Hash64 {
    let mut state = keyed(PALW_RECEIPT_V2_DOMAIN_MESSAGE);
    state.update(network_domain.as_byte_slice());
    state.update(claim.as_byte_slice());
    // Every field the verdict carries is signed. A signature over the TAG alone would let a
    // seat's `Unavailable` be replayed against a different chunk or a different request time —
    // the accusation's whole content, swapped underneath a valid signature.
    match verdict {
        PalwReceiptVerdictV2::Valid => state.update(&[1u8]),
        PalwReceiptVerdictV2::Incapable => state.update(&[3u8]),
        PalwReceiptVerdictV2::Unavailable { chunk_index, requested_daa } => {
            state.update(&[2u8]);
            state.update(&chunk_index.to_le_bytes());
            state.update(&requested_daa.to_le_bytes())
        }
        // ADR-0152 Q-1: the next tag, never a reuse, so no signature over another verdict reads as it.
        PalwReceiptVerdictV2::Sampled => state.update(&[4u8]),
    };
    state.update(&signed_daa.to_le_bytes());
    finish(state)
}

/// ADR-0133 Verification V2: the V2 message and the segment mask, under their own domain.
pub fn palw_receipt_message_v3(
    network_domain: Hash64,
    claim: Hash64,
    verdict: PalwReceiptVerdictV2,
    signed_daa: u64,
    segments: crate::palw_verification_v2::PalwSegmentMaskV2,
) -> Hash64 {
    let mut state = keyed(PALW_RECEIPT_V3_DOMAIN_MESSAGE);
    state.update(palw_receipt_message_v2(network_domain, claim, verdict, signed_daa).as_byte_slice());
    state.update(&segments.0.to_le_bytes());
    finish(state)
}

/// What a validated quorum licenses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwReceiptQuorumV2 {
    /// ≥ quorum seats signed `Valid`: the `ReceiptLicensed` object is acceptable.
    Licensed { valid: u16 },
    /// ≥ quorum seats signed `Unavailable`: the producer defaulted on its DA obligation, and the
    /// `ProducerDefaulted` object is acceptable — the panel answered, the producer did not.
    ProducerUnavailable { unavailable: u16 },
    /// **ADR-0124 Decision 2: a supplementary set on a claim already licensed** — `credited`
    /// `Valid` receipts of seats on duty the chain had not credited, all inside the receipt
    /// window. The `ReceiptLicensed` object is acceptable and moves no phase; it only credits.
    Supplementary { credited: u16 },
}

/// Validate a receipt set against the bound panel at this chain point. Every receipt must name
/// this claim, hold a seat, verify under the seat bond's registry key, and each seat answers at
/// most once; then either verdict reaching quorum licenses its transition.
pub fn validate_receipt_quorum_v2<V>(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    claim_id: &Hash64,
    receipts: &[PalwSeatReceiptV2],
    verify_mldsa87: V,
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    validate_receipt_quorum_v2_with_policy(
        state,
        params,
        state_params,
        ctx,
        network_domain,
        claim_id,
        receipts,
        verify_mldsa87,
        false,
        None,
    )
}

/// [`validate_receipt_quorum_v2`] with ADR-0065 D4's verdict policy.
///
/// `unavailable_abstains` is `Params::palw_unavailable_abstains` resolved at `ctx.daa_score`.
/// `false` — every shipped preset — is byte-identical to the tally before the parameter existed.
///
/// **What changes past the fence, and what deliberately does not.** An `Unavailable` receipt is
/// still a well-formed answer: still signed, still checked against the panel and the receipt
/// window, and the seat that files it is still on the record rather than a no-show. It simply
/// decides nothing — the treatment `Incapable` already gets.
///
/// What it is NO LONGER checked against is the obligation it names. That gate exists because a
/// quorum of `Unavailable` voided an honest producer's claim; with no such quorum reachable it
/// guards nothing, and keeping it would let one seat's malformed abstention refuse a whole receipt
/// set and kill a claim three other seats verified. So a panel that cannot be fed reaches no quorum, the claim redraws once
/// (`rebound_daa`) and then voids at `ReceiptTimeout`, which destroys the escrow and slashes
/// nobody, rather than at `ProducerDefaulted`, which takes `claim.reserved` from the bond of the
/// producer that served correctly.
///
/// The verdict is kept rather than refused because a seat MUST have a way to say "I got nothing"
/// — deleting it would push those seats into silence, and silence is the one thing this chain has
/// established it cannot observe.
///
/// **ADR-0100 Decision 4.** A claim whose panel was drawn per shard is refused here — it licenses
/// by parts, through [`validate_shard_receipt_part_v1`]. No plan exists on a network that never
/// armed that fence, so there this door is exactly what it was.
#[allow(clippy::too_many_arguments)]
pub fn validate_receipt_quorum_v2_with_policy<V>(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    claim_id: &Hash64,
    receipts: &[PalwSeatReceiptV2],
    verify_mldsa87: V,
    unavailable_abstains: bool,
    // ADR-0147: `Params::palw_admission_independence`'s height (`None` where it is not configured).
    independence_daa: Option<u64>,
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    validate_receipt_quorum_v2_with_economy(
        state,
        params,
        state_params,
        ctx,
        network_domain,
        claim_id,
        receipts,
        verify_mldsa87,
        unavailable_abstains,
        false,
        independence_daa,
    )
}

/// [`validate_receipt_quorum_v2_with_policy`] with ADR-0124's supplementary door: past
/// `Params::palw_panel_economy` (`panel_economy`, resolved at the carrying block) a
/// `ReceiptLicensed` object on a claim ALREADY licensed is acceptable while its receipt window is
/// open, provided every receipt it carries is a `Valid` one of a seat on duty the chain has not
/// credited — or, on a licence awaiting its replay past `Params::palw_rcore_plus`, one it has not
/// counted over the whole job (ADR-0152 V3S-01) — see [`validate_supplementary_receipts_v1`]. `false`
/// is byte-identical to the check before the door existed.
#[allow(clippy::too_many_arguments)]
pub fn validate_receipt_quorum_v2_with_economy<V>(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    claim_id: &Hash64,
    receipts: &[PalwSeatReceiptV2],
    verify_mldsa87: V,
    unavailable_abstains: bool,
    panel_economy: bool,
    // ADR-0147: `Params::palw_admission_independence`'s height (`None` where it is not configured).
    independence_daa: Option<u64>,
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    if panel_economy {
        let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
        if matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
            return validate_supplementary_receipts_v1(state, state_params, ctx, network_domain, claim_id, receipts, verify_mldsa87);
        }
    }
    let outsider = palw_panel_outsider_bond_v1(state, claim_id, independence_daa);
    receipt_quorum_over_seats_v2(
        state,
        state_params,
        ctx,
        network_domain,
        claim_id,
        "ReceiptQuorum",
        || {
            let panel = state.panel(claim_id).ok_or(PalwPanelV2Error::NoPanel(*claim_id))?;
            if crate::palw_shard_licensing_v1::palw_claim_licenses_by_parts_v1(state, claim_id, params.seat_count).is_some() {
                return Err(PalwPanelV2Error::LicensedByParts(*claim_id));
            }
            Ok(panel.seats.clone())
        },
        params.quorum,
        receipts,
        verify_mldsa87,
        unavailable_abstains,
        outsider,
    )
}

/// **ADR-0147: the bond whose `Valid` a licence of this claim must carry**, or `None` for a claim
/// the rule does not reach. The bound panel's first seat, for exactly the claims
/// [`crate::palw_state_v2::palw_claim_is_outsider_judged_v1`] names — the acceptance layer's half of
/// the rule the fold states in `palw_licence_names_its_outsider_v1`, reading the same predicate so
/// the two cannot disagree about which claims have an outsider.
pub fn palw_panel_outsider_bond_v1(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    independence_daa: Option<u64>,
) -> Option<PalwBondKeyV2> {
    let claim = state.claim(claim_id)?;
    if !crate::palw_state_v2::palw_claim_is_outsider_judged_v1(state, claim, independence_daa) {
        return None;
    }
    state.panel(claim_id).and_then(|panel| panel.seats.first()).map(|seat| seat.bond)
}

/// **ADR-0124 Decision 2: a seat carries its own receipt after the licence.** The licensing
/// object carries whatever receipts its assembler held at one tick; a seat whose receipt arrived a
/// moment later, or that the assembler simply left out, would otherwise be a seat the chain never
/// saw answer. So until the receipt deadline a claim already `ReceiptLicensed` accepts a
/// `ReceiptLicensed` object again, and what it may carry is exactly: one or more `Valid` receipts,
/// each from a seat on duty for this claim that the chain has not credited, each signed by the
/// seat's registered key inside `[bound_daa, bound_daa + window_receipt]` and not after the block
/// that carries it, no seat twice. Nothing else — an `Unavailable` or `Incapable` receipt is not a
/// discharge the pool pays, and a seat that already counted is not counted again. The fold
/// re-derives every structural fact from its own state (`credit_supplementary_receipts`), so the
/// sync walk credits exactly what this layer admitted.
/// **ADR-0133 Verification V2: the licence by coverage.** Every check the V1 quorum makes (the seat,
/// the duplicate, the signature — over the V3 message, the window, the `Unavailable` obligations)
/// and then two conditions: the V1 quorum of `Valid` receipts (three of five), and every segment of
/// the anchor's cut attested `Valid` at least `PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT` times
/// (the full seat plus the unique partial holder). A Valid receipt's mask must be that seat's
/// assignment — a widened, narrowed, or foreign mask is refused.
#[allow(clippy::too_many_arguments)]
pub fn validate_receipt_coverage_v2<V>(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    claim_id: &Hash64,
    receipts: &[PalwSeatReceiptV3],
    verify_mldsa87: V,
    unavailable_abstains: bool,
    // ADR-0147: `Params::palw_admission_independence`'s height (`None` where it is not configured).
    independence_daa: Option<u64>,
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    use crate::palw_verification_v2::{PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT, palw_coverage_v2, palw_segment_assignment_v2};
    let outsider = palw_panel_outsider_bond_v1(state, claim_id, independence_daa);
    let mut outsider_valid = false;
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let PalwClaimPhaseV2::PanelBound { bound_daa } = claim.phase else {
        return Err(PalwPanelV2Error::WrongPhase { claim: *claim_id, edge: "ReceiptCoverageV2" });
    };
    let receipt_deadline = bound_daa
        .checked_add(state_params.receipt_window_for_claim_v1(
            state,
            &claim.class_id,
            crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(claim),
            bound_daa,
        ))
        .ok_or(PalwPanelV2Error::ReceiptOutsideWindow { seat: claim.bond, why: "the receipt deadline overflows the DAA score" })?;
    let panel = state.panel(claim_id).ok_or(PalwPanelV2Error::NoPanel(*claim_id))?;
    if crate::palw_shard_licensing_v1::palw_claim_licenses_by_parts_v1(state, claim_id, params.seat_count()).is_some() {
        return Err(PalwPanelV2Error::LicensedByParts(*claim_id));
    }
    let seats = panel.seats.clone();
    let assignment = palw_segment_assignment_v2(panel.anchor, *claim_id, seats.len() as u16);

    let mut answered: Vec<PalwBondKeyV2> = Vec::new();
    let mut valid: u16 = 0;
    let mut unavailable: u16 = 0;
    let mut valid_masks = Vec::new();
    for signed in receipts {
        let receipt = &signed.receipt;
        if receipt.claim != *claim_id {
            return Err(PalwPanelV2Error::ReceiptClaimMismatch { got: receipt.claim, expected: *claim_id });
        }
        if !seats.iter().any(|seat| seat.bond == receipt.seat_bond) {
            return Err(PalwPanelV2Error::NotASeat(receipt.seat_bond));
        }
        if answered.contains(&receipt.seat_bond) {
            return Err(PalwPanelV2Error::DuplicateSeat(receipt.seat_bond));
        }
        let bond = state.bond(&receipt.seat_bond).ok_or(PalwPanelV2Error::SeatBondMissing(receipt.seat_bond))?;
        let message = palw_receipt_message_v3(network_domain, *claim_id, receipt.verdict, receipt.signed_daa, signed.segments);
        if !verify_mldsa87(&bond.pubkey, message.as_byte_slice(), &receipt.signature, PALW_RECEIPT_V3_MLDSA87_CONTEXT) {
            return Err(PalwPanelV2Error::ReceiptSignatureInvalid);
        }
        if receipt.signed_daa < bound_daa {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed before the panel was bound" });
        }
        if receipt.signed_daa > receipt_deadline {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed past the receipt deadline" });
        }
        if receipt.signed_daa > ctx.daa_score {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed after the block carrying it" });
        }
        answered.push(receipt.seat_bond);
        if matches!(receipt.verdict, PalwReceiptVerdictV2::Valid) {
            let seat_index = seats.iter().position(|seat| seat.bond == receipt.seat_bond).expect("NotASeat already returned") as u16;
            let expected = assignment.mask_of(seat_index);
            if signed.segments != expected {
                return Err(PalwPanelV2Error::MaskNotAssigned {
                    seat: receipt.seat_bond,
                    got: signed.segments.0,
                    expected: expected.0,
                });
            }
        }
        match receipt.verdict {
            PalwReceiptVerdictV2::Valid => {
                valid += 1;
                valid_masks.push(signed.segments);
                outsider_valid |= outsider == Some(receipt.seat_bond);
            }
            PalwReceiptVerdictV2::Incapable => {
                if !crate::palw_state_v2::palw_seat_may_plead_incapable_v2(claim.class_id, state_params.base_class_id()) {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "no node may plead it cannot execute the liveness floor",
                    });
                }
            }
            PalwReceiptVerdictV2::Unavailable { .. } if unavailable_abstains => {
                unavailable += 1;
            }
            PalwReceiptVerdictV2::Unavailable { chunk_index, requested_daa } => {
                if chunk_index >= claim.trace_chunk_count {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "the named chunk is not one the attempt committed to",
                    });
                }
                if requested_daa < bound_daa {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "the request predates the panel that was owed the data",
                    });
                }
                if requested_daa > receipt.signed_daa {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "the request had not happened when the seat signed about it",
                    });
                }
                if requested_daa > claim.trace_retention_daa {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "the request falls past the producer's retention obligation",
                    });
                }
                unavailable += 1
            }
            // ADR-0152 Q-1: audit, not a vote — answered, counted in no quorum and in no segment's
            // coverage, whatever mask it carries. R-core+'s verdict only.
            PalwReceiptVerdictV2::Sampled => {
                if !state_params.rcore_plus_active_at(ctx.daa_score) {
                    return Err(PalwPanelV2Error::SampledBelowRcore(receipt.seat_bond));
                }
            }
        }
    }
    if valid >= params.quorum() {
        let coverage = palw_coverage_v2(assignment.segments, &valid_masks);
        if let Some((segment, have)) = coverage.short(PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT) {
            return Err(PalwPanelV2Error::CoverageShort { segment, have, need: PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT });
        }
        // ADR-0147: coverage by the class's own seats is still the class's own seats.
        if let Some(seat) = outsider
            && !outsider_valid
        {
            return Err(PalwPanelV2Error::OutsiderHasNotAnswered { claim: *claim_id, seat });
        }
        return Ok(PalwReceiptQuorumV2::Licensed { valid });
    }
    if !unavailable_abstains && unavailable >= params.quorum() {
        return Ok(PalwReceiptQuorumV2::ProducerUnavailable { unavailable });
    }
    Err(PalwPanelV2Error::NoQuorum { valid, unavailable, needed: params.quorum() })
}

/// `validate_receipt_coverage_v2`'s answer on one receipt set, as the licence selection asks it.
pub type PalwCoverageVerdictV2 = Result<PalwReceiptQuorumV2, PalwPanelV2Error>;

/// **The order a node tries receipts in** — the one policy of the licence selection
/// ([`palw_select_coverage_licence_v2`], [`palw_select_optimistic_licence_v2`]).
///
/// The full-replay seat's `Valid` first, because the optimistic door cannot open without it; then
/// the outsider's `Valid`, because an outsider-judged claim's licence must carry it (ADR-0147) and
/// the optimistic door has room for one rider beside the full seat; then every other `Valid`; then
/// every other verdict. Arrival order within each rank, so of two receipts from one seat its `Valid`
/// is tried first.
pub fn palw_licence_candidate_order_v2(
    candidates: &[PalwSeatReceiptV3],
    full_seat: Option<PalwBondKeyV2>,
    outsider: Option<PalwBondKeyV2>,
) -> Vec<&PalwSeatReceiptV3> {
    let rank = |signed: &PalwSeatReceiptV3| match signed.receipt.verdict {
        PalwReceiptVerdictV2::Valid if full_seat == Some(signed.receipt.seat_bond) => 0u8,
        PalwReceiptVerdictV2::Valid if outsider == Some(signed.receipt.seat_bond) => 1,
        PalwReceiptVerdictV2::Valid => 2,
        _ => 3,
    };
    let mut ordered: Vec<&PalwSeatReceiptV3> = candidates.iter().collect();
    // A stable sort: arrival order survives inside a rank.
    ordered.sort_by_key(|signed| rank(signed));
    ordered
}

/// Every sound receipt of `ordered`, with `coverage`'s verdict on the set (`None` when none is).
/// A candidate is kept when the set with it added is a licence or a shortfall
/// ([`PalwPanelV2Error::is_receipt_set_shortfall`]) and dropped when it poisons the set — including
/// a second receipt from a seat already kept (`DuplicateSeat`).
fn palw_sound_receipts_v2<C>(ordered: &[&PalwSeatReceiptV3], coverage: &C) -> (Vec<PalwSeatReceiptV3>, Option<PalwCoverageVerdictV2>)
where
    C: Fn(&[PalwSeatReceiptV3]) -> PalwCoverageVerdictV2,
{
    let mut kept: Vec<PalwSeatReceiptV3> = Vec::with_capacity(ordered.len());
    let mut verdict = None;
    for candidate in ordered {
        kept.push((*candidate).clone());
        let answer = coverage(&kept);
        let sound = match &answer {
            Ok(_) => true,
            Err(refusal) => refusal.is_receipt_set_shortfall(),
        };
        if sound {
            verdict = Some(answer);
        } else {
            kept.pop();
        }
    }
    (kept, verdict)
}

/// **The coverage licence (`ReceiptLicensedV2`) a node offers, if any** — the licence selection's
/// first door (the 2026-09-24 licence-stall fix).
///
/// **Node policy.** The selection decides which set an assembler submits, never what a block
/// accepts, so a fleet that mixes it with the assembler it replaces forks nothing. That assembler
/// added receipts in ARRIVAL order and dropped any whose addition came back as anything but
/// `NoQuorum`. On the shipped panel (five seats, quorum three, four segments needing two
/// attestations each) three or four `Valid`s are `CoverageShort`, so every pool froze at its first
/// two `Valid`s: this licence could never form, and the optimistic one formed only when the
/// full-replay seat was among the first two to arrive. About half the floor claims of testnet-12
/// stayed `PanelBound`, and the pool, append-only, never let one recover.
///
/// **Nothing here counts receipts.** `coverage` is `validate_receipt_coverage_v2` at the carrying
/// point, bound exactly as the acceptance arm binds it, and every set is put to it: which receipts
/// count toward the quorum and which segments they cover is that function's answer, so a change to
/// what counts is made there and the selection follows it. The one policy held here is the order
/// ([`palw_licence_candidate_order_v2`]).
///
/// A sound receipt added to a sound set never takes coverage away — it adds a `Valid` or an
/// abstention, and nothing past the quorum is refused for being too many — so the largest sound set
/// is `Licensed` exactly when some set of these receipts is. It is offered when it is, and when
/// `licenses` (the fold's answer, `palw_v2_object_licenses_claim_v1`: a set carrying a `Valid` whose
/// seat cannot post its lock licenses nothing — inert past the audit fence, refused below it)
/// agrees. This door is tried first, so offering such a set here would keep the optimistic door
/// from ever being tried.
pub fn palw_select_coverage_licence_v2<C, F>(
    candidates: &[PalwSeatReceiptV3],
    coverage: C,
    licenses: F,
) -> Option<Vec<PalwSeatReceiptV3>>
where
    C: Fn(&[PalwSeatReceiptV3]) -> PalwCoverageVerdictV2,
    F: Fn(&[PalwSeatReceiptV3]) -> bool,
{
    let ordered = palw_licence_candidate_order_v2(candidates, None, None);
    let (sound, verdict) = palw_sound_receipts_v2(&ordered, &coverage);
    (matches!(verdict, Some(Ok(PalwReceiptQuorumV2::Licensed { .. }))) && licenses(&sound)).then_some(sound)
}

/// **The optimistic licence (`OptimisticLicensed`) a node offers, if any** — the licence
/// selection's second door; see [`palw_select_coverage_licence_v2`] for what the selection is and
/// why it counts nothing itself.
///
/// The door takes a set with the full-replay seat's `Valid` whose coverage verdict is `Ok` or
/// `NoQuorum` ([`crate::palw_optimistic_licence_v2::palw_optimistic_licence_admits_v2`], the
/// predicate its acceptance arm calls). So the door has a set exactly when the full seat's `Valid`
/// is sound — that receipt alone is one — and a licence exists when `licenses` (the fold) takes one
/// too: with the objective-offence ledger armed, only when the full seat can post the door's
/// whole-gain lock and every other `Valid` of the set its quorum-price lock (and, on an
/// outsider-judged claim, the outsider's `Valid` rides). The selection offers the first of these
/// that the door takes and `licenses` agrees to:
///
/// 1. every sound receipt — all five `Valid`s when all five validated, which cover;
/// 2. the full seat's `Valid`, then each other sound receipt in order that the door still takes and
///    `licenses` still agrees to beside what is kept — on the shipped panel one more `Valid` (the
///    next is `CoverageShort`), passing over a seat that cannot post its lock, and any other
///    verdicts;
/// 3. the full seat's `Valid` alone.
///
/// Never a set the door refuses: not three or four `Valid`s short of coverage, and not a set
/// without the full seat's `Valid`, wherever in the pool it arrived.
pub fn palw_select_optimistic_licence_v2<C, F>(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    // ADR-0147: `Params::palw_admission_independence`'s height, as `coverage` is bound with it.
    independence_daa: Option<u64>,
    candidates: &[PalwSeatReceiptV3],
    coverage: C,
    licenses: F,
) -> Option<Vec<PalwSeatReceiptV3>>
where
    C: Fn(&[PalwSeatReceiptV3]) -> PalwCoverageVerdictV2,
    F: Fn(&[PalwSeatReceiptV3]) -> bool,
{
    use crate::palw_optimistic_licence_v2::{palw_optimistic_full_seat_bond_v2, palw_optimistic_licence_admits_v2};
    let panel = state.panel(claim_id)?;
    let seats: Vec<PalwBondKeyV2> = panel.seats.iter().map(|seat| seat.bond).collect();
    let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(panel.anchor, *claim_id, seats.len() as u16);
    let full = palw_optimistic_full_seat_bond_v2(&assignment, &seats)?;
    let outsider = palw_panel_outsider_bond_v1(state, claim_id, independence_daa);
    let ordered = palw_licence_candidate_order_v2(candidates, Some(full), outsider);
    let (sound, sound_verdict) = palw_sound_receipts_v2(&ordered, &coverage);
    let head = sound
        .first()
        .filter(|signed| signed.receipt.seat_bond == full && matches!(signed.receipt.verdict, PalwReceiptVerdictV2::Valid))?
        .clone();
    let admits = |set: &[PalwSeatReceiptV3], verdict: &PalwCoverageVerdictV2| {
        palw_optimistic_licence_admits_v2(panel.anchor, *claim_id, &seats, set, verdict)
    };

    // 1. Every sound receipt.
    if sound_verdict.as_ref().is_some_and(|verdict| admits(&sound, verdict)) && licenses(&sound) {
        return Some(sound);
    }
    // 2. The full seat, and each rider the door and the fold still take beside what is kept. The
    //    fold is asked only of a set the door takes, so on the shipped panel it is asked of each
    //    `Valid` until one is kept, and of the other verdicts. The full seat starts the set even
    //    when the fold refuses it alone: an outsider-judged claim licenses only once its outsider's
    //    `Valid` rides (ADR-0147), and the order puts that rider next.
    let alone = vec![head];
    let mut kept = alone.clone();
    for rider in &sound[1..] {
        let mut attempt = kept.clone();
        attempt.push(rider.clone());
        if admits(&attempt, &coverage(&attempt)) && licenses(&attempt) {
            kept = attempt;
        }
    }
    // Each rider kept was taken by the door and the fold with everything kept before it.
    if kept.len() > 1 {
        return Some(kept);
    }
    // 3. The full seat alone, unless it was the whole sound set, already refused above.
    (sound.len() > 1 && admits(&alone, &coverage(&alone)) && licenses(&alone)).then_some(alone)
}

/// **What a node's receipt pool reads off the tip, and nothing more** (node policy; the 2026-09-24
/// launch review's receipt-pool flush).
///
/// The pool a node offers [`palw_select_coverage_licence_v2`] and [`palw_select_optimistic_licence_v2`]
/// is filled by gossip, which checks size and nothing else. It held sixteen receipts a claim and
/// evicted the oldest, so sixteen borsh-valid receipts naming a claim with junk signatures — about
/// 150 bytes each, relayed to every node — evicted all five seats' receipts everywhere, each seat's
/// own included. The selection then dropped the junk and found no licence, nothing re-delivered the
/// genuine five, and the claim sat `PanelBound` to its redraw and, on a second flood, to the
/// timeout that voids it and slashes an honest producer. A pool can refuse that only if it knows
/// who may sign for a claim and under which keys: the bound panel's seats and each seat bond's
/// registered key. This answers exactly that, for the claims and bonds asked.
///
/// Read-only and advisory: it decides what a node KEEPS, never what a block accepts. The assembler
/// still puts every receipt it is offered to the acceptance validator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwReceiptPoolFactsV1 {
    /// For each asked claim the tip holds `PanelBound`: its bound panel. A claim the tip does not
    /// hold, or holds in any other phase, is absent.
    pub panels: Vec<PalwReceiptPanelFactV1>,
    /// For each asked bond the registry holds: its registered ML-DSA-87 key.
    pub seat_keys: Vec<(PalwBondKeyV2, Vec<u8>)>,
}

/// One bound panel, as [`PalwReceiptPoolFactsV1`] carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwReceiptPanelFactV1 {
    pub claim_id: Hash64,
    /// When the panel bound. A receipt signed before it counts for no seat of this panel
    /// ([`validate_receipt_coverage_v2`]: "signed before the panel was bound").
    pub bound_daa: u64,
    /// The beacon the panel was drawn from — what tells two sibling panels at one DAA apart.
    pub anchor: Hash64,
    /// The seats' bonds, in seat order.
    pub seats: Vec<PalwBondKeyV2>,
}

/// [`PalwReceiptPoolFactsV1`] at one state: the `PanelBound` claims among `claims` with their panels,
/// and the registered keys of the bonds among `bonds`, each in the order asked.
pub fn palw_receipt_pool_facts_v1(state: &PalwChainStateV2, claims: &[Hash64], bonds: &[PalwBondKeyV2]) -> PalwReceiptPoolFactsV1 {
    let panels = claims
        .iter()
        .filter_map(|claim_id| {
            let PalwClaimPhaseV2::PanelBound { bound_daa } = state.claim(claim_id)?.phase else { return None };
            let panel = state.panel(claim_id)?;
            Some(PalwReceiptPanelFactV1 {
                claim_id: *claim_id,
                bound_daa,
                anchor: panel.anchor,
                seats: panel.seats.iter().map(|seat| seat.bond).collect(),
            })
        })
        .collect();
    let seat_keys = bonds.iter().filter_map(|bond| state.bond(bond).map(|record| (*bond, record.pubkey.clone()))).collect();
    PalwReceiptPoolFactsV1 { panels, seat_keys }
}

pub fn validate_supplementary_receipts_v1<V>(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    claim_id: &Hash64,
    receipts: &[PalwSeatReceiptV2],
    verify_mldsa87: V,
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    if !matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
        return Err(PalwPanelV2Error::WrongPhase { claim: *claim_id, edge: "SupplementaryReceipts" });
    }
    validate_supplementary_receipt_set_v1(state, state_params, ctx, network_domain, claim_id, receipts, verify_mldsa87)
}

/// The new original TIR protocol accepts one signed, backed seat acknowledgement
/// atomically with its distinct raw grant and Ready promise. It is NOT a licence:
/// quorum, claim weight and the ordinary licensing doors remain unchanged.
pub fn validate_public_custody_receipt_v1<V>(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    receipt: &PalwSeatReceiptV2,
    verify_mldsa87: V,
) -> Result<(), PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let claim = state.claim(&receipt.claim).ok_or(PalwPanelV2Error::MissingClaim(receipt.claim))?;
    if crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, receipt.claim).is_none()
        || !matches!(claim.phase, PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::ReceiptLicensed { .. })
    {
        return Err(PalwPanelV2Error::WrongPhase { claim: receipt.claim, edge: "TirPublicGrantReady" });
    }
    validate_supplementary_receipt_set_v1(
        state,
        state_params,
        ctx,
        network_domain,
        &receipt.claim,
        std::slice::from_ref(receipt),
        verify_mldsa87,
    )?;
    Ok(())
}

fn validate_supplementary_receipt_set_v1<V>(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    claim_id: &Hash64,
    receipts: &[PalwSeatReceiptV2],
    verify_mldsa87: V,
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let duties = state.panel_duties_of(claim_id).ok_or(PalwPanelV2Error::NotOnDuty(*claim_id))?;
    let bound_daa = state.panel(claim_id).ok_or(PalwPanelV2Error::NoPanel(*claim_id))?.bound_daa;
    let receipt_deadline = bound_daa
        .checked_add(state_params.receipt_window_for_claim_v1(
            state,
            &claim.class_id,
            crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(claim),
            bound_daa,
        ))
        .ok_or(PalwPanelV2Error::ReceiptOutsideWindow { seat: claim.bond, why: "the receipt deadline overflows the DAA score" })?;
    if receipts.is_empty() {
        return Err(PalwPanelV2Error::SupplementaryRefused("no receipt"));
    }
    if ctx.daa_score > receipt_deadline {
        return Err(PalwPanelV2Error::SupplementaryRefused("the receipt window has closed"));
    }
    let mut answered: Vec<PalwBondKeyV2> = Vec::new();
    for receipt in receipts {
        if receipt.claim != *claim_id {
            return Err(PalwPanelV2Error::ReceiptClaimMismatch { got: receipt.claim, expected: *claim_id });
        }
        // ADR-0152 V3S-01: on a licence awaiting its replay, a credited seat not yet counted over the
        // whole job is taken as well — the fold's one predicate, so this layer and the fold agree.
        match duties.get(&receipt.seat_bond) {
            None => return Err(PalwPanelV2Error::NotASeat(receipt.seat_bond)),
            Some(at)
                if *at != 0 && !crate::palw_state_v2::palw_rcore_v2_widens_seat_v1(state, claim_id, claim, &receipt.seat_bond) =>
            {
                return Err(PalwPanelV2Error::SeatAlreadyCredited(receipt.seat_bond));
            }
            Some(_) => {}
        }
        if answered.contains(&receipt.seat_bond) {
            return Err(PalwPanelV2Error::DuplicateSeat(receipt.seat_bond));
        }
        if !matches!(receipt.verdict, PalwReceiptVerdictV2::Valid) {
            return Err(PalwPanelV2Error::SupplementaryRefused("a receipt whose verdict is not Valid"));
        }
        let bond = state.bond(&receipt.seat_bond).ok_or(PalwPanelV2Error::SeatBondMissing(receipt.seat_bond))?;
        let message = palw_receipt_message_v2(network_domain, *claim_id, receipt.verdict, receipt.signed_daa);
        if !verify_mldsa87(&bond.pubkey, message.as_byte_slice(), &receipt.signature, PALW_RECEIPT_V2_MLDSA87_CONTEXT) {
            return Err(PalwPanelV2Error::ReceiptSignatureInvalid);
        }
        if receipt.signed_daa < bound_daa {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed before the panel was bound" });
        }
        if receipt.signed_daa > receipt_deadline {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed past the receipt deadline" });
        }
        if receipt.signed_daa > ctx.daa_score {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed after the block carrying it" });
        }
        answered.push(receipt.seat_bond);
    }
    Ok(PalwReceiptQuorumV2::Supplementary { credited: answered.len() as u16 })
}

/// **ADR-0152 SR-10: the V3 supplementary door, as the acceptance layer checks it.**
///
/// The V2 door ([`validate_supplementary_receipts_v1`]) takes V2 `Valid`s only, and a partial seat
/// signs V3 receipts over its mask, which cannot be re-presented as V2 — so without this door an S2
/// licence of the full seat and one partial could never recount to `basis_k ≥ 2`, and a late partial
/// seat could never complete a release. Past `Params::palw_rcore_plus` a `ReceiptLicensedV2` on a claim
/// already `ReceiptLicensed` is therefore a supplementary set, not a licence (no new object tag; the
/// processor routes it here on the claim's phase).
///
/// What it may carry, all checked here: one or more V3 receipts (`Valid`, `Unavailable`, `Incapable`,
/// `Sampled`) naming this claim, each from a seat of the bound panel that is on duty, not yet
/// credited and holding no lock on the claim — a counted signer is never counted twice — no seat
/// twice; each verified under `palw_receipt_message_v3` by the seat bond's registered key, signed
/// inside `[bound_daa, bound_daa + window_receipt]` and not after the carrying block; the object
/// itself at or before the receipt deadline. A `Valid` must carry exactly the seat's assigned mask,
/// as the coverage door requires, so the mask its lock records is the assignment (Q-6). `Incapable`
/// is refused on the liveness floor. `Unavailable` is an abstention with no obligation to check:
/// `palw_unavailable_abstains` is a prerequisite of the fence. A set of abstentions only
/// (`Unavailable`, `Incapable`) on a claim whose `unserved_seen` is already latched is refused: an
/// abstention is neither credited nor locked, so the same signed receipt would otherwise pass again
/// and again until the receipt deadline, each time recording nothing, for a block's space and one
/// ML-DSA verify per receipt (M4 review, finding 4). The fold re-derives every structural fact from
/// its own state (`credit_supplementary_receipts_v3`), this refusal included.
pub fn validate_supplementary_receipts_v3<V>(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    claim_id: &Hash64,
    receipts: &[PalwSeatReceiptV3],
    verify_mldsa87: V,
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    if !state_params.rcore_plus_active_at(ctx.daa_score) {
        return Err(PalwPanelV2Error::SupplementaryV3Refused("the V3 door is R-core+'s (ADR-0152 SR-10)"));
    }
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    if !matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
        return Err(PalwPanelV2Error::WrongPhase { claim: *claim_id, edge: "SupplementaryReceiptsV3" });
    }
    let duties = state.panel_duties_of(claim_id).ok_or(PalwPanelV2Error::NotOnDuty(*claim_id))?;
    let panel = state.panel(claim_id).ok_or(PalwPanelV2Error::NoPanel(*claim_id))?;
    let bound_daa = panel.bound_daa;
    let receipt_deadline = bound_daa
        .checked_add(state_params.receipt_window_for_claim_v1(
            state,
            &claim.class_id,
            crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(claim),
            bound_daa,
        ))
        .ok_or(PalwPanelV2Error::ReceiptOutsideWindow { seat: claim.bond, why: "the receipt deadline overflows the DAA score" })?;
    if receipts.is_empty() {
        return Err(PalwPanelV2Error::SupplementaryV3Refused("no receipt"));
    }
    if ctx.daa_score > receipt_deadline {
        return Err(PalwPanelV2Error::SupplementaryV3Refused("the receipt window has closed"));
    }
    let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(panel.anchor, *claim_id, panel.seats.len() as u16);
    let mut answered: Vec<PalwBondKeyV2> = Vec::new();
    for signed in receipts {
        let receipt = &signed.receipt;
        if receipt.claim != *claim_id {
            return Err(PalwPanelV2Error::ReceiptClaimMismatch { got: receipt.claim, expected: *claim_id });
        }
        let Some(seat_index) = panel.seats.iter().position(|seat| seat.bond == receipt.seat_bond) else {
            return Err(PalwPanelV2Error::NotASeat(receipt.seat_bond));
        };
        match duties.get(&receipt.seat_bond) {
            None => return Err(PalwPanelV2Error::NotASeat(receipt.seat_bond)),
            Some(at) if *at != 0 => return Err(PalwPanelV2Error::SeatAlreadyCredited(receipt.seat_bond)),
            Some(_) => {}
        }
        if state.slashable_lock(receipt.seat_bond, *claim_id).is_some() {
            return Err(PalwPanelV2Error::SeatAlreadyCredited(receipt.seat_bond));
        }
        if answered.contains(&receipt.seat_bond) {
            return Err(PalwPanelV2Error::DuplicateSeat(receipt.seat_bond));
        }
        let bond = state.bond(&receipt.seat_bond).ok_or(PalwPanelV2Error::SeatBondMissing(receipt.seat_bond))?;
        let message = palw_receipt_message_v3(network_domain, *claim_id, receipt.verdict, receipt.signed_daa, signed.segments);
        if !verify_mldsa87(&bond.pubkey, message.as_byte_slice(), &receipt.signature, PALW_RECEIPT_V3_MLDSA87_CONTEXT) {
            return Err(PalwPanelV2Error::ReceiptSignatureInvalid);
        }
        if receipt.signed_daa < bound_daa {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed before the panel was bound" });
        }
        if receipt.signed_daa > receipt_deadline {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed past the receipt deadline" });
        }
        if receipt.signed_daa > ctx.daa_score {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed after the block carrying it" });
        }
        match receipt.verdict {
            PalwReceiptVerdictV2::Valid => {
                let expected = assignment.mask_of(seat_index as u16);
                if signed.segments != expected {
                    return Err(PalwPanelV2Error::MaskNotAssigned {
                        seat: receipt.seat_bond,
                        got: signed.segments.0,
                        expected: expected.0,
                    });
                }
            }
            PalwReceiptVerdictV2::Incapable => {
                if !crate::palw_state_v2::palw_seat_may_plead_incapable_v2(claim.class_id, state_params.base_class_id()) {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "no node may plead it cannot execute the liveness floor",
                    });
                }
            }
            PalwReceiptVerdictV2::Unavailable { .. } | PalwReceiptVerdictV2::Sampled => {}
        }
        answered.push(receipt.seat_bond);
    }
    // An abstention is neither credited nor locked and sets no served bit: all it records is the
    // `unserved_seen` latch. Once that is set, a set of abstentions only has nothing left to record.
    // The fold refuses the same set by the same test.
    let answers =
        receipts.iter().any(|signed| matches!(signed.receipt.verdict, PalwReceiptVerdictV2::Valid | PalwReceiptVerdictV2::Sampled));
    if !answers && claim.rcore.unserved_seen {
        return Err(PalwPanelV2Error::SupplementaryV3Refused("abstentions only, on a claim whose unserved_seen is already latched"));
    }
    Ok(PalwReceiptQuorumV2::Supplementary { credited: answered.len() as u16 })
}

/// **The V3 supplementary set a collector offers for a licensed claim, if any** (ADR-0152 SR-10 and
/// Q-7, node policy: it decides what a node carries, never what a block accepts).
///
/// After an S2 licence — the full seat's `Valid` and at most one rider (post-edit 13) — the seats the
/// licence did not carry still file, and their receipts are the only thing that can raise the claim
/// to `basis_k ≥ 2` before DL-1's gate redraws it (Q-5), complete SR-1b's release, or pay a seat that
/// answered. SR-10's door takes them in a `ReceiptLicensedV2` on the licensed claim; this picks the
/// set, and it counts nothing itself:
///
/// 1. **What the door could credit**: this claim's `Valid`s and `Sampled`s, from seats the chain has
///    not counted ([`crate::palw_state_v2::palw_seat_uncounted_on_licence_v1`]), one receipt a seat —
///    its `Valid` before its `Sampled` (a carried `Sampled` credits the seat, so its `Valid` could
///    never ride after it), otherwise in arrival order.
///    **Abstentions are never carried**: an `Unavailable` or `Incapable` is neither credited nor
///    counted, and all the door records of one is `unserved_seen` — the latch that holds the
///    producer's escrow to `Final` (C1), even if that very seat serves later. Accusing the producer
///    is the DA court's (P2-6's `DefaultAccused`), never a collector's.
///    **`Sampled` is filed here** (Q-1): not a licence and not a vote, it is credited for seat pay and
///    latches `unserved_seen` — so it rides a licence set only as a rider and otherwise this door.
/// 2. **Each alone through `validate`** — [`validate_supplementary_receipts_v3`] at the carrying
///    point, bound as the acceptance arm binds it: its signature, window and mask. A receipt it
///    refuses is dropped, never fatal (a poisoned pool must not sink the set), and the next receipt
///    of that seat is tried.
/// 3. **The set through `validate`, then through `effect`** — the fold's own answer
///    ([`crate::palw_state_v2::palw_v2_supplementary_effect_v1`]). Only the seats the fold credits
///    are carried: a `Valid` whose seat cannot post `lock_{max(k,2)}` (SR-6) moves nothing, so it is
///    dropped and the rest are put to the fold once more. `None` when the fold credits nobody.
/// 4. **On a licence awaiting its replay a `Valid` rides only in a set that upgrades it** (Q-5,
///    V3S-01; the M4 review's finding 1). A set that pays partial seats without lifting the claim off
///    DL-1's gate gains nothing there — a redraw drops the credit and releases the locks, and after
///    the upgrade the same receipts ride as pay — while it locks those seats' stake on a claim that
///    may redraw, and before the V2 door widened a counted seat it barred their whole-job replays
///    from the upgrade (the fold now takes them: `palw_rcore_v2_widens_seat_v1`, so a third party's
///    pay set cannot do that harm either). Such a set's `Valid`s are dropped and its `Sampled`s,
///    which count nowhere and are paid, are offered alone.
pub fn palw_select_supplementary_v3_v1<V, E>(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    candidates: &[PalwSeatReceiptV3],
    validate: V,
    effect: E,
) -> Option<(Vec<PalwSeatReceiptV3>, crate::palw_state_v2::PalwSupplementaryEffectV1)>
where
    V: Fn(&[PalwSeatReceiptV3]) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>,
    E: Fn(&[PalwSeatReceiptV3]) -> Option<crate::palw_state_v2::PalwSupplementaryEffectV1>,
{
    let rank = |signed: &PalwSeatReceiptV3| match signed.receipt.verdict {
        PalwReceiptVerdictV2::Valid => Some(0u8),
        PalwReceiptVerdictV2::Sampled => Some(1),
        PalwReceiptVerdictV2::Unavailable { .. } | PalwReceiptVerdictV2::Incapable => None,
    };
    let mut ordered: Vec<(u8, &PalwSeatReceiptV3)> = candidates
        .iter()
        .filter(|signed| signed.receipt.claim == *claim_id)
        .filter(|signed| crate::palw_state_v2::palw_seat_uncounted_on_licence_v1(state, claim_id, &signed.receipt.seat_bond))
        .filter_map(|signed| rank(signed).map(|r| (r, signed)))
        .collect();
    // A stable sort: arrival order survives inside a rank.
    ordered.sort_by_key(|(r, _)| *r);
    let accepts = |set: &[PalwSeatReceiptV3]| matches!(validate(set), Ok(PalwReceiptQuorumV2::Supplementary { .. }));
    let mut kept: Vec<PalwSeatReceiptV3> = Vec::new();
    for (_, signed) in ordered {
        if kept.iter().any(|k| k.receipt.seat_bond == signed.receipt.seat_bond) {
            continue;
        }
        if accepts(std::slice::from_ref(signed)) {
            kept.push(signed.clone());
        }
    }
    if kept.is_empty() || !accepts(&kept) {
        return None;
    }
    let mut folded = effect(&kept)?;
    if folded.credited.len() < kept.len() {
        kept.retain(|signed| folded.credited.contains(&signed.receipt.seat_bond));
        if kept.is_empty() || !accepts(&kept) {
            return None;
        }
        folded = effect(&kept)?;
    }
    // 4. A licence awaiting its replay takes a `Valid` only in a set that upgrades it.
    let awaits = state.claim(claim_id).is_some_and(crate::palw_state_v2::palw_rcore_licence_awaits_replay_v1);
    if awaits && !folded.upgrades && kept.iter().any(|signed| matches!(signed.receipt.verdict, PalwReceiptVerdictV2::Valid)) {
        kept.retain(|signed| matches!(signed.receipt.verdict, PalwReceiptVerdictV2::Sampled));
        if kept.is_empty() || !accepts(&kept) {
            return None;
        }
        folded = effect(&kept)?;
    }
    (!folded.credited.is_empty()).then_some((kept, folded))
}

/// **The full-replay V2 `Valid`s a collector carries through the V2 door to lift a licence off Q-5's
/// gate** (ADR-0152 V3S-01, Q-7; node policy — it decides what a node carries, never what a block
/// accepts).
///
/// A V2 `Valid` is a whole-job replay: it covers every segment, so ONE of them from any seat — a
/// partial seat the S2 licence left out, the licence's own rider, a seat a V3 set paid, a seat that
/// filed `Sampled` — raises every segment's count by one and lifts `basis_k` 1 to 2 (Q-3). Past
/// SEAT-R every replaying seat of a floor or 8k panel files its whole-job V2 beside its V3, so it is
/// the route that survives a silent partial seat, where the V3 route needs every segment's holder.
///
/// `None` unless the claim awaits its replay ([`crate::palw_state_v2::palw_rcore_licence_awaits_replay_v1`]).
/// Otherwise the candidates are taken in the caller's order (own first, then heard): this claim's
/// `Valid`s from seats the V2 door takes ([`crate::palw_state_v2::palw_rcore_v2_door_takes_seat_v1`]),
/// one a seat, each alone through `validate` ([`validate_supplementary_receipts_v1`] bound as the
/// acceptance arm binds it — a refused one is dropped, never fatal), then added to the set while the
/// set still validates and the fold still takes it (`effect`: a seat that cannot post its lock
/// refuses the whole object in the fold, SR-6, so it is left out). The first set the fold says
/// UPGRADES is the offer — normally one receipt; more ride only where one could not lift it. A set
/// that never upgrades is not offered: a pay-only V2 is the seat's own carrier's to send (ADR-0124
/// Decision 2), after the claim is replay-backed.
pub fn palw_select_supplementary_v2_upgrade_v1<V, E>(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    candidates: &[PalwSeatReceiptV2],
    validate: V,
    effect: E,
) -> Option<(Vec<PalwSeatReceiptV2>, crate::palw_state_v2::PalwSupplementaryEffectV1)>
where
    V: Fn(&[PalwSeatReceiptV2]) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>,
    E: Fn(&[PalwSeatReceiptV2]) -> Option<crate::palw_state_v2::PalwSupplementaryEffectV1>,
{
    let claim = state.claim(claim_id)?;
    if !crate::palw_state_v2::palw_rcore_licence_awaits_replay_v1(claim) {
        return None;
    }
    let accepts = |set: &[PalwSeatReceiptV2]| matches!(validate(set), Ok(PalwReceiptQuorumV2::Supplementary { .. }));
    let mut kept: Vec<PalwSeatReceiptV2> = Vec::new();
    for receipt in candidates.iter().filter(|receipt| {
        receipt.claim == *claim_id
            && matches!(receipt.verdict, PalwReceiptVerdictV2::Valid)
            && crate::palw_state_v2::palw_rcore_v2_door_takes_seat_v1(state, claim_id, claim, &receipt.seat_bond)
    }) {
        if kept.iter().any(|k| k.seat_bond == receipt.seat_bond) || !accepts(std::slice::from_ref(receipt)) {
            continue;
        }
        let mut trial = kept.clone();
        trial.push(receipt.clone());
        if !accepts(&trial) {
            continue;
        }
        let Some(folded) = effect(&trial) else { continue };
        kept = trial;
        if folded.upgrades {
            return Some((kept, folded));
        }
    }
    None
}

/// **The one supplementary set a collector offers for a licensed claim this tick** (ADR-0152 SR-10,
/// Q-5, Q-7 and V3S-01; node policy). One object a claim a tick — a second one built on the same tip
/// would name seats the first credits, and the acceptance walk would drop it for its fee:
///
/// * the V3 set ([`palw_select_supplementary_v3_v1`]) where it lifts an awaiting licence off the gate
///   — it credits every seat it can and may complete SR-1b's release;
/// * otherwise, on a licence awaiting its replay, the V2 door's upgrade
///   ([`palw_select_supplementary_v2_upgrade_v1`]) — BEFORE any pay set, so a silent partial seat
///   cannot hold an honest S2 claim on the gate while other seats have replayed the whole job;
/// * otherwise the V3 set (pay, a release, or `Sampled`s), if any.
///
/// `validate_v3` / `validate_v2` are the acceptance validators bound as the gate binds them;
/// `effect` is the fold's own answer on the object
/// ([`crate::palw_state_v2::palw_v2_supplementary_effect_v1`]).
#[allow(clippy::too_many_arguments)]
pub fn palw_select_supplementary_offer_v1<V3, V2, E>(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    v3_candidates: &[PalwSeatReceiptV3],
    v2_candidates: &[PalwSeatReceiptV2],
    validate_v3: V3,
    validate_v2: V2,
    effect: E,
) -> Option<crate::palw_state_v2::PalwSupplementaryOfferV1>
where
    V3: Fn(&[PalwSeatReceiptV3]) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>,
    V2: Fn(&[PalwSeatReceiptV2]) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>,
    E: Fn(&crate::palw_state_v2::PalwConsensusObjectV2) -> Option<crate::palw_state_v2::PalwSupplementaryEffectV1>,
{
    use crate::palw_state_v2::{PalwConsensusObjectV2 as Obj, PalwSupplementaryOfferV1 as Offer};
    let claim = *claim_id;
    let awaits = state.claim(claim_id).is_some_and(crate::palw_state_v2::palw_rcore_licence_awaits_replay_v1);
    let v3 = palw_select_supplementary_v3_v1(state, claim_id, v3_candidates, validate_v3, |set| {
        effect(&Obj::ReceiptLicensedV2 { claim, receipts: set.to_vec() })
    });
    if awaits && !v3.as_ref().is_some_and(|(_, folded)| folded.upgrades) {
        let v2 = palw_select_supplementary_v2_upgrade_v1(state, claim_id, v2_candidates, validate_v2, |set| {
            effect(&Obj::ReceiptLicensed { claim, receipts: set.to_vec() })
        });
        if let Some((receipts, folded)) = v2 {
            return Some(Offer { object: Obj::ReceiptLicensed { claim, receipts }, effect: folded });
        }
    }
    v3.map(|(receipts, folded)| Offer { object: Obj::ReceiptLicensedV2 { claim, receipts }, effect: folded })
}

/// **Which licence door a node offers first** (ADR-0152 Q-7's X22; node policy).
///
/// Past `Params::palw_rcore_plus` (`x22`): a set that licenses at `basis_k ≥ 2` — the coverage door,
/// then the V1 quorum door (≥ 3 whole-job V2 `Valid`s and the outsider, so `basis_k` 3) — before the
/// optimistic fast path, and S2 only when neither can be built from what is on hand. S2 recounts to
/// 1 (the full seat and at most one rider) and lands the claim on DL-1's Q-5 gate, where it needs an
/// upgrade before `bound + window_receipt` or it redraws; a V1 set the pool already holds lands it
/// replay-backed. Below the fence an S2 licence simply finalizes, and the order is what it was:
/// coverage, optimistic, then V1.
///
/// Each door is asked lazily, so a door is built only when every door before it had nothing.
/// Returns the object and the door it came through.
pub fn palw_licence_offer_order_v1<O>(
    x22: bool,
    coverage: impl FnOnce() -> Option<O>,
    quorum: impl FnOnce() -> Option<O>,
    optimistic: impl FnOnce() -> Option<O>,
) -> Option<(O, crate::palw_economic_safety_v1::PalwLicenceDoorTagV1)> {
    use crate::palw_economic_safety_v1::PalwLicenceDoorTagV1 as Door;
    let covered = coverage().map(|object| (object, Door::Coverage));
    if x22 {
        covered
            .or_else(|| quorum().map(|object| (object, Door::Quorum)))
            .or_else(|| optimistic().map(|object| (object, Door::Optimistic)))
    } else {
        covered
            .or_else(|| optimistic().map(|object| (object, Door::Optimistic)))
            .or_else(|| quorum().map(|object| (object, Door::Quorum)))
    }
}

/// **The V1 set a node offers past SEAT-R: its quorum and nothing past it, and a different one each
/// time** (the SEAT-R review, MEDIUM — the half a node can do without the processor; node policy).
/// kaspad's licence collector calls it as `palw_licence_offer_order_v1`'s V1 door; it lives here,
/// beside that order, so the processor's T45 (ADR-0152 X22 with SR-6, Phase 2 P2-5) drives the
/// collector's own V1 composition over the real assembler rather than a copy of it.
///
/// Below `Params::palw_rcore_plus`, `palw_v2_receipt_quorum_assemble` keeps every clean candidate
/// past the quorum and asks the fold nothing (past it, see the next paragraph), and past
/// `palw_audit_2026_09_23` a set any one of whose `Valid` signers cannot post its lock folds INERT
/// (`receipt_set_is_backed`): the claim stays `PanelBound` and the node pays a carrier every replan
/// until the window closes. Past SEAT-R every seat of a floor or 8k panel replays and signs `Valid`,
/// so the greedy set carries all five, and one short seat is enough. So the assembler is offered the
/// SHORTEST prefix of the candidates it assembles from — the quorum, and the outsider where the claim
/// has one — and the candidates are rotated by `sent`, the V1 sets the node has already sent for the
/// claim, so a set that went inert is followed by one that leaves other seats out.
///
/// **Past `palw_rcore_plus` the assembler makes the fold-aware choice itself** (ADR-0152 SR-6, Phase
/// 2 P2-5): it offers the fold's backed subset of the set it is handed, and nothing for a set whose
/// backed subset is short of the quorum, so a prefix that carries an unbacked `Valid` is passed over
/// for the next one and the V1 door falls through to S2 (X22) instead of holding the claim on an
/// inert set. The prefix still bounds the set to the quorum and the rotation still varies it; below
/// that fence (a SEAT-R network without R-core+) they are this door's only defence, as before.
pub fn palw_v1_offer_v1<R: Clone, O>(candidates: &[R], sent: u32, mut assemble: impl FnMut(Vec<R>) -> Option<O>) -> Option<O> {
    if candidates.is_empty() {
        return None;
    }
    let mut rotated = candidates.to_vec();
    rotated.rotate_left(sent as usize % candidates.len());
    (1..=rotated.len()).find_map(|n| assemble(rotated[..n].to_vec()))
}

/// **One shard's part** (ADR-0100 Decision 4): the same receipt checks, over the seats of THAT
/// shard's slice of a stratified panel, at the same quorum a shard's seats are drawn for. Refused
/// by name: a claim that does not license by parts, a part of another plan, a shard out of range,
/// a shard already licensed.
#[allow(clippy::too_many_arguments)]
pub fn validate_shard_receipt_part_v1<V>(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    part: &crate::palw_shard_licensing_v1::PalwShardReceiptPartV1,
    verify_mldsa87: V,
    unavailable_abstains: bool,
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    receipt_quorum_over_seats_v2(
        state,
        state_params,
        ctx,
        network_domain,
        &part.claim,
        "ShardReceiptQuorum",
        || {
            let plan = crate::palw_shard_licensing_v1::palw_claim_licenses_by_parts_v1(state, &part.claim, params.seat_count)
                .ok_or(PalwPanelV2Error::NotLicensedByParts(part.claim))?;
            if part.shard_count != plan.shard_count {
                return Err(PalwPanelV2Error::ShardPlanMismatch { declared: plan.shard_count, part: part.shard_count });
            }
            if part.shard_index >= plan.shard_count {
                return Err(PalwPanelV2Error::ShardOutOfRange { shard: part.shard_index, count: plan.shard_count });
            }
            if state.shard_licensing_of(&part.claim).is_some_and(|progress| progress.is_licensed(part.shard_index)) {
                return Err(PalwPanelV2Error::ShardAlreadyLicensed { shard: part.shard_index });
            }
            let panel = state.panel(&part.claim).ok_or(PalwPanelV2Error::NoPanel(part.claim))?;
            let slice = crate::palw_shard_licensing_v1::palw_panel_shard_slice_v1(
                &panel.seats,
                plan.shard_count,
                params.seat_count,
                part.shard_index,
            )
            .ok_or(PalwPanelV2Error::NotLicensedByParts(part.claim))?;
            Ok(slice.to_vec())
        },
        params.quorum,
        &part.receipts,
        verify_mldsa87,
        unavailable_abstains,
        // ADR-0147: a stratified panel seats no outsider, and the fold refuses a part licence of an
        // outsider-judged claim (`OutsiderJudgedClaimLicensedByParts`) — `validate_palw_v2` keeps
        // the two fences from being armed together at all.
        None,
    )
}

/// **RFC-0006 §4.3/§4.5: one shard's part, as the acceptance layer checks it.**
///
/// Past `Params::palw_tir_shard_v1` only, on a `PanelBound` claim drawn per shard (its `tir_shard_claims` record), a shard
/// not yet licensed: every receipt a V4 receipt of THIS claim and shard, from a seat of THIS shard's slice, once, verified
/// under `palw_receipt_message_v4` by the seat bond's registered key, signed inside `[bound_daa, receipt_deadline]` and not
/// after the carrying block; a `Valid` carrying exactly the seat's ASSIGNED mask (Q-6: the lock records what the seat
/// vouched for; the outsider's is the whole shard), `Incapable` and `Unavailable` abstentions, `Sampled` never. Then the
/// shard's verdict ([`crate::palw_tir_shard_v1::palw_tir_shard_part_verdict_v1`]): two class seats on every cell and the
/// outsider's `Valid`, or [`PalwPanelV2Error::TirShardPartShort`] — a sound set an assembler keeps collecting.
pub fn validate_tir_shard_part_v1<V>(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    part: &crate::palw_tir_shard_v1::PalwTirShardPartV1,
    verify_mldsa87: V,
) -> Result<crate::palw_tir_shard_v1::PalwTirShardPartVerdictV1, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    use crate::palw_tir_shard_v1 as shard_rules;
    let refused = |why: String| PalwPanelV2Error::TirShardPartRefused(why);
    if !state_params.tir_shard_active_at(ctx.daa_score) {
        return Err(refused("layer-sharded panels are not in force (palw_tir_shard_v1)".to_string()));
    }
    let claim_id = &part.claim;
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let PalwClaimPhaseV2::PanelBound { bound_daa } = claim.phase else {
        return Err(PalwPanelV2Error::WrongPhase { claim: *claim_id, edge: "TirShardReceiptLicensed" });
    };
    let record = state.tir_shard_claim(claim_id).ok_or(PalwPanelV2Error::NotLicensedByParts(*claim_id))?;
    if part.shard >= record.s_l {
        return Err(PalwPanelV2Error::ShardOutOfRange { shard: u32::from(part.shard), count: u32::from(record.s_l) });
    }
    if record.progress.is_licensed(u32::from(part.shard)) {
        return Err(PalwPanelV2Error::ShardAlreadyLicensed { shard: u32::from(part.shard) });
    }
    let panel = state.panel(claim_id).ok_or(PalwPanelV2Error::NoPanel(*claim_id))?;
    let slice = shard_rules::palw_tir_panel_shard_slice_v1(&panel.seats, record.s_l, record.outsider, part.shard)
        .ok_or(PalwPanelV2Error::NotLicensedByParts(*claim_id))?;
    if part.receipts.is_empty() || part.receipts.len() > shard_rules::PALW_TIR_SHARD_PART_MAX_RECEIPTS_V1 {
        return Err(refused(format!(
            "{} receipts in a part of at most {}",
            part.receipts.len(),
            shard_rules::PALW_TIR_SHARD_PART_MAX_RECEIPTS_V1
        )));
    }
    let receipt_deadline = bound_daa
        .checked_add(state_params.receipt_window_for_claim_v1(
            state,
            &claim.class_id,
            crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(claim),
            bound_daa,
        ))
        .ok_or(PalwPanelV2Error::ReceiptOutsideWindow { seat: claim.bond, why: "the receipt deadline overflows the DAA score" })?;
    let mut answered: Vec<PalwBondKeyV2> = Vec::new();
    let mut counted: Vec<(PalwBondKeyV2, PalwReceiptVerdictV2, crate::palw_verification_v2::PalwSegmentMaskV2)> = Vec::new();
    for signed in &part.receipts {
        let receipt = &signed.receipt;
        if receipt.claim != *claim_id {
            return Err(PalwPanelV2Error::ReceiptClaimMismatch { got: receipt.claim, expected: *claim_id });
        }
        if signed.shard != part.shard {
            return Err(refused(format!("a receipt for shard {} in the part of shard {}", signed.shard, part.shard)));
        }
        let Some(index) = slice.iter().position(|seat| seat.bond == receipt.seat_bond) else {
            return Err(PalwPanelV2Error::NotASeat(receipt.seat_bond));
        };
        if answered.contains(&receipt.seat_bond) {
            return Err(PalwPanelV2Error::DuplicateSeat(receipt.seat_bond));
        }
        let bond = state.bond(&receipt.seat_bond).ok_or(PalwPanelV2Error::SeatBondMissing(receipt.seat_bond))?;
        let message = shard_rules::palw_receipt_message_v4(
            network_domain,
            *claim_id,
            receipt.verdict,
            receipt.signed_daa,
            signed.shard,
            signed.segments,
        );
        if !verify_mldsa87(&bond.pubkey, message.as_byte_slice(), &receipt.signature, shard_rules::PALW_RECEIPT_V4_MLDSA87_CONTEXT) {
            return Err(PalwPanelV2Error::ReceiptSignatureInvalid);
        }
        if receipt.signed_daa < bound_daa {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed before the panel was bound" });
        }
        if receipt.signed_daa > receipt_deadline {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed past the receipt deadline" });
        }
        if receipt.signed_daa > ctx.daa_score {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed after the block carrying it" });
        }
        match receipt.verdict {
            PalwReceiptVerdictV2::Valid => {
                let assigned =
                    shard_rules::palw_tir_shard_seat_mask_v1(&panel.anchor, claim_id, part.shard, record.s_p, record.outsider, index);
                if assigned != Some(signed.segments) {
                    return Err(PalwPanelV2Error::MaskNotAssigned {
                        seat: receipt.seat_bond,
                        got: signed.segments.0,
                        expected: assigned.map_or(0, |m| m.0),
                    });
                }
            }
            // An abstention: on the record, not a no-show, counted nowhere. Refused on the liveness floor.
            PalwReceiptVerdictV2::Incapable => {
                if !crate::palw_state_v2::palw_seat_may_plead_incapable_v2(claim.class_id, state_params.base_class_id()) {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "no node may plead it cannot execute the liveness floor",
                    });
                }
            }
            // `palw_unavailable_abstains` is R-core+'s prerequisite: it accuses nobody and needs no obligation proof.
            PalwReceiptVerdictV2::Unavailable { .. } => {}
            PalwReceiptVerdictV2::Sampled => {
                return Err(refused("a Sampled receipt counts nowhere and has no place in a part".to_string()));
            }
        }
        answered.push(receipt.seat_bond);
        counted.push((receipt.seat_bond, receipt.verdict, signed.segments));
    }
    match shard_rules::palw_tir_shard_part_verdict_v1(
        slice,
        &panel.anchor,
        claim_id,
        part.shard,
        record.s_p,
        record.outsider,
        &counted,
    ) {
        short @ shard_rules::PalwTirShardPartVerdictV1::Short(_) => match short {
            shard_rules::PalwTirShardPartVerdictV1::Short(why) => Err(PalwPanelV2Error::TirShardPartShort(why)),
            _ => unreachable!(),
        },
        licensed => Ok(licensed),
    }
}

/// The receipt checks both doors apply, over the seats `seats_of` supplies once the claim's phase
/// and window are known.
#[allow(clippy::too_many_arguments)]
fn receipt_quorum_over_seats_v2<V, S>(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    claim_id: &Hash64,
    edge: &'static str,
    seats_of: S,
    quorum: u16,
    receipts: &[PalwSeatReceiptV2],
    verify_mldsa87: V,
    unavailable_abstains: bool,
    // ADR-0147: the seat whose `Valid` a licence must carry, where the claim has one.
    outsider: Option<PalwBondKeyV2>,
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    S: FnOnce() -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error>,
{
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let PalwClaimPhaseV2::PanelBound { bound_daa } = claim.phase else {
        return Err(PalwPanelV2Error::WrongPhase { claim: *claim_id, edge });
    };
    let receipt_deadline = bound_daa
        .checked_add(state_params.receipt_window_for_claim_v1(
            state,
            &claim.class_id,
            crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(claim),
            bound_daa,
        ))
        .ok_or(PalwPanelV2Error::ReceiptOutsideWindow { seat: claim.bond, why: "the receipt deadline overflows the DAA score" })?;
    let seats = seats_of()?;

    let mut answered: Vec<PalwBondKeyV2> = Vec::new();
    let mut valid: u16 = 0;
    let mut outsider_valid = false;
    let mut unavailable: u16 = 0;
    for receipt in receipts {
        if receipt.claim != *claim_id {
            return Err(PalwPanelV2Error::ReceiptClaimMismatch { got: receipt.claim, expected: *claim_id });
        }
        if !seats.iter().any(|seat| seat.bond == receipt.seat_bond) {
            return Err(PalwPanelV2Error::NotASeat(receipt.seat_bond));
        }
        if answered.contains(&receipt.seat_bond) {
            return Err(PalwPanelV2Error::DuplicateSeat(receipt.seat_bond));
        }
        // The key comes from the REGISTRY at this chain point, not from a snapshot inside the
        // receipt — a seat cannot rotate itself onto a different key mid-duty. (A bond that
        // entered retirement still serves its standing duties; only a bond that vanished
        // entirely is an error, and bonds never vanish in this ruleset's state.)
        let bond = state.bond(&receipt.seat_bond).ok_or(PalwPanelV2Error::SeatBondMissing(receipt.seat_bond))?;
        let message = palw_receipt_message_v2(network_domain, *claim_id, receipt.verdict, receipt.signed_daa);
        if !verify_mldsa87(&bond.pubkey, message.as_byte_slice(), &receipt.signature, PALW_RECEIPT_V2_MLDSA87_CONTEXT) {
            return Err(PalwPanelV2Error::ReceiptSignatureInvalid);
        }
        // The duty has a clock (audit C5). A receipt signed before the panel existed cannot be
        // about this panel's duty, one signed after the deadline is not a discharge of it, and
        // one signed in the future is not a signature about anything that has happened.
        if receipt.signed_daa < bound_daa {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed before the panel was bound" });
        }
        if receipt.signed_daa > receipt_deadline {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed past the receipt deadline" });
        }
        if receipt.signed_daa > ctx.daa_score {
            return Err(PalwPanelV2Error::ReceiptOutsideWindow { seat: receipt.seat_bond, why: "signed after the block carrying it" });
        }

        answered.push(receipt.seat_bond);
        match receipt.verdict {
            PalwReceiptVerdictV2::Valid => {
                valid += 1;
                outsider_valid |= outsider == Some(receipt.seat_bond);
            }
            // **Answered, but not a vote.** The seat is on the record — so it is not a no-show and
            // is not charged — and it counts toward neither side, because a party that says it
            // cannot judge does not get to decide. Refused on the liveness floor, where the plea
            // cannot be true: BASE-0 is in every binary that can validate a block, so admitting it
            // there would let a quorum of seats stall the one class the chain must always have.
            PalwReceiptVerdictV2::Incapable => {
                if !crate::palw_state_v2::palw_seat_may_plead_incapable_v2(claim.class_id, state_params.base_class_id()) {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "no node may plead it cannot execute the liveness floor",
                    });
                }
            }
            // **ADR-0065 D4: past the fence this verdict accuses nobody, so it is not checked as an
            // accusation.** The obligation gate below exists because a quorum of `Unavailable`
            // voided an honest producer's claim; with no such quorum reachable it guards nothing,
            // and keeping it would let one seat's malformed abstention refuse a whole receipt set
            // — killing an otherwise licensable claim on a field the rule no longer reads.
            // Nothing downstream reads these fields either: the charge that used to is the one D4
            // removes.
            PalwReceiptVerdictV2::Unavailable { .. } if unavailable_abstains => {
                unavailable += 1;
            }
            PalwReceiptVerdictV2::Unavailable { chunk_index, requested_daa } => {
                // An accusation has to name an obligation the producer ACTUALLY HAD. None of
                // this proves a byte went unsent — nothing on-chain can — but it removes the
                // contentless accusation, which is what a quorum of `Unavailable` was built out
                // of before: a chunk the attempt never committed to, or a request made after
                // retention lapsed, is a demand the producer never owed.
                if chunk_index >= claim.trace_chunk_count {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "the named chunk is not one the attempt committed to",
                    });
                }
                if requested_daa < bound_daa {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "the request predates the panel that was owed the data",
                    });
                }
                if requested_daa > receipt.signed_daa {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "the request had not happened when the seat signed about it",
                    });
                }
                if requested_daa > claim.trace_retention_daa {
                    return Err(PalwPanelV2Error::UnmetObligationNotProven {
                        seat: receipt.seat_bond,
                        why: "the request falls past the producer's retention obligation",
                    });
                }
                unavailable += 1
            }
            // **ADR-0152 Q-1: audit, not a vote.** The seat sampled its S3 sites: on the record, not
            // a no-show, and counted toward neither side — it attests nothing a quorum could rest
            // on. R-core+'s verdict only; below the fence it poisons the set.
            PalwReceiptVerdictV2::Sampled => {
                if !state_params.rcore_plus_active_at(ctx.daa_score) {
                    return Err(PalwPanelV2Error::SampledBelowRcore(receipt.seat_bond));
                }
            }
        }
    }
    if valid >= quorum {
        // ADR-0147: a quorum without the outsider's `Valid` is not yet a licence. Reported as its
        // own error, which an assembler reads as "keep collecting" exactly like `NoQuorum`.
        if let Some(seat) = outsider
            && !outsider_valid
        {
            return Err(PalwPanelV2Error::OutsiderHasNotAnswered { claim: *claim_id, seat });
        }
        return Ok(PalwReceiptQuorumV2::Licensed { valid });
    }
    // ADR-0065 D4: past the fence there is no second quorum to reach. Reported as `NoQuorum` with
    // the true tally, so an operator reading the log still sees how many seats said they got
    // nothing — the number stops being a verdict, it does not stop being visible.
    if !unavailable_abstains && unavailable >= quorum {
        return Ok(PalwReceiptQuorumV2::ProducerUnavailable { unavailable });
    }
    Err(PalwPanelV2Error::NoQuorum { valid, unavailable, needed: quorum })
}

#[cfg(test)]
mod tests;
