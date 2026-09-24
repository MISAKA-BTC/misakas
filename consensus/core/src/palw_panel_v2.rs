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
}

impl PalwPanelStakeDrawV1 {
    /// The terms ADR-0152 v3.1 fixes: the 1,000,000 MSK cap and the 875‰ floor.
    pub const V1: Self =
        Self { weight_cap_msk: PALW_DRAW_WEIGHT_CAP_MSK_V1, eligible_floor_permille: PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1 };
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
            let Some(record) = state.bond(bond) else { return false };
            let committed =
                crate::palw_state_v2::palw_bond_committed_v1(state, bond, self.now_daa, self.settled_anchor_depth, self.window_court);
            let room = crate::palw_state_v2::palw_rcore_gate_room_of_v1(
                record.collateral,
                filter.ceiling_permille,
                committed,
                crate::palw_state_v2::palw_accuser_exposure_v1(state, bond),
                crate::palw_state_v2::PalwRcoreGateV1::Work,
            );
            return filter.eligibility <= room;
        }
        let posted = state.bond(bond).map(|b| b.collateral as u128).unwrap_or(0);
        if posted < self.required {
            return false;
        }
        state.palw_slashable_available_v1(bond, self.now_daa, self.settled_anchor_depth, self.window_court) >= self.required
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
        matches!(self, Self::NoQuorum { .. } | Self::CoverageShort { .. } | Self::OutsiderHasNotAnswered { .. })
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

/// The chain fact that fixes a claim's anchor, supplied by the pipeline from its own candidate
/// chain (the same trust class as [`PalwBlockContextV2`]): `anchor_block` is the FIRST chain
/// block whose DAA score reached `accepted_daa + anchor_delay`, and `predecessor_daa` is its
/// selected parent's DAA score — carried so "first" is checkable: the predecessor must still be
/// short of the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAnchorFactV2 {
    pub anchor_block: BlockHash,
    pub anchor_daa: u64,
    pub predecessor_daa: u64,
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
        if !crate::palw_state_v2::palw_bond_may_judge_class_v4(state, bond_key, bond, judged_class, capability_proof, readiness) {
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
            let (seat, floor) = palw_panel_outsider_draw_v1(
                state,
                params,
                claim_id,
                anchor_block,
                min_collateral_sompi,
                registered_by_daa,
                capability_proof,
                &policy,
                &independence,
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
        &claim.class_id,
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
    // The outsider's operator holds one seat, never two: an operator that serves the network AND
    // proved it holds the class is drawn for the class's seats only when it is not the outsider.
    let (eligible, needed) = match &outsider {
        Some(outsider) => (
            eligible.into_iter().filter(|(_, bond)| bond.operator_id != outsider.operator_id).collect::<Vec<_>>(),
            params.seat_count.saturating_sub(1),
        ),
        None => (eligible, params.seat_count),
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
            &claim.class_id,
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
        // SW-10's executor term (M4 review finding 1): the executor operator's capped weight where
        // it could sit on this class but for being the executor, on both sides of the floor.
        let executor = palw_panel_stake_executor_bonds_judging_v1(
            state,
            claim_id,
            &claim.class_id,
            min_collateral_sompi,
            registered_by_daa,
            capability_proof,
            policy.readiness,
            policy.economy,
            params.seat_count,
        )?;
        let executor_weight = palw_panel_stake_weight_v1(&executor, &stake);
        palw_panel_stake_race_with_v1(needed, claim_id, anchor_block, &eligible, &base, executor_weight, &stake, outsider_floor)?
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
    let not_registrant =
        |(key, bond): &(&PalwBondKeyV2, &PalwBondStateV2)| Some(**key) != registrant && Some(bond.operator_id) != registrant_operator;
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
/// unweighted draw's ticket. Past the panel economy it no longer decides who sits; it decides which
/// of an operator's eligible bonds the operator sits with.
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

/// **ADR-0152 SW-10: a population's weight** — the sum, over its operators, of SW-2's capped
/// weights (the same `W` the race keys on, so the floor and the race cannot weigh one operator
/// two ways). Operators are counted once however many bonds they hold on the list.
pub fn palw_panel_stake_weight_v1(population: &[(&PalwBondKeyV2, &PalwBondStateV2)], stake: &PalwPanelStakeDrawV1) -> u128 {
    let mut posted: std::collections::BTreeMap<Hash64, u128> = std::collections::BTreeMap::new();
    for (_, bond) in population {
        let sum = posted.entry(bond.operator_id).or_insert(0);
        *sum = sum.saturating_add((bond.collateral / crate::constants::SOMPI_PER_KASPA) as u128);
    }
    posted.into_values().map(|sum| palw_draw_operator_weight_msk_v1(sum, stake.weight_cap_msk) as u128).sum()
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
    palw_panel_stake_race_with_v1(needed, claim_id, anchor_block, eligible, base, 0, stake, None)
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
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    let entries = palw_panel_stake_entries_v1(claim_id, anchor_block, eligible, stake);
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
    if proposed_anchor != anchor.anchor_block {
        return Err(PalwPanelV2Error::AnchorMismatch("the object's anchor is not the chain's anchor block"));
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
    let derived = match stratified {
        // **The stratified (shard) draw is not stake-weighted yet.** It is dormant on every network
        // (`palw_shard_court` / `palw_kary_court` are `None` everywhere, so `stratified` is always
        // `None` here), so C-02's weighting rides the flat draw below. When the shard court is armed,
        // `derive_shard_panel_v1`'s ticketing needs the same bucketing — a follow-up gated behind the
        // shard fence, not this one. `weighted` is deliberately not forwarded here.
        Some(shard_count) => derive_stratified_panel_v2(
            state,
            params,
            claim_id,
            anchor.anchor_block,
            state_params.min_collateral_sompi(),
            registered_by_daa,
            capability_proof,
            shard_count,
        )?,
        None => derive_panel_v2_with_policy(
            state,
            params,
            claim_id,
            anchor.anchor_block,
            state_params.min_collateral_sompi(),
            registered_by_daa,
            capability_proof,
            policy,
        )?,
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
/// credited — see [`validate_supplementary_receipts_v1`]. `false` is byte-identical to the check
/// before the door existed.
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
        .checked_add(state_params.receipt_window_for_claim_v1(state, &claim.class_id, bound_daa))
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
    let duties = state.panel_duties_of(claim_id).ok_or(PalwPanelV2Error::NotOnDuty(*claim_id))?;
    let bound_daa = state.panel(claim_id).ok_or(PalwPanelV2Error::NoPanel(*claim_id))?.bound_daa;
    let receipt_deadline = bound_daa
        .checked_add(state_params.receipt_window_for_claim_v1(state, &claim.class_id, bound_daa))
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
        match duties.get(&receipt.seat_bond) {
            None => return Err(PalwPanelV2Error::NotASeat(receipt.seat_bond)),
            Some(at) if *at != 0 => return Err(PalwPanelV2Error::SeatAlreadyCredited(receipt.seat_bond)),
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
        .checked_add(state_params.receipt_window_for_claim_v1(state, &claim.class_id, bound_daa))
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
mod tests {
    use super::*;
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2};
    use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2, apply_palw_transition_v2};
    use crate::tx::{TransactionId, TransactionOutpoint};

    /// Operator identities are DERIVED from a key now, so the fixtures carry a key and let the
    /// state machine mint the id — the same path a real registration takes.
    fn op_key(v: u64) -> Vec<u8> {
        vec![v as u8; 8]
    }

    fn op_id(v: u64) -> Hash64 {
        crate::palw_state_v2::palw_operator_id_v2(&op_key(v))
    }

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn state_params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap()
    }

    fn panel_params() -> PalwPanelParamsV2 {
        PalwPanelParamsV2::new(3, 2, 4).unwrap()
    }

    fn bond_outpoint(v: u64) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 }
    }

    fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: BlockHash::from_u64_word(block), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    fn register(bond: u64, pubkey: u8, operator: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::BondRegistered {
            bond: PalwBondKeyV2(bond_outpoint(bond)),
            pubkey: vec![pubkey; 4],
            operator_pubkey: op_key(operator),
            collateral: 1_000_000,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            // The fixture's one class. ADR-0071 Decision 3 excludes an undeclared bond from the
            // draw, so a registry of silent bonds seats nobody — which is the rule working, and
            // which is why every fixture that expects a panel has to say what its seats can run.
            capable_classes: std::collections::BTreeSet::from([h64(1)]),
            signature: Vec::new(),
        }
    }

    /// `register`, declaring a class other than the one the fixture claims under — the only thing
    /// that moves between the two halves of the capability test.
    fn register_declaring(bond: u64, pubkey: u8, operator: u64, class_id: Hash64) -> PalwConsensusObjectV2 {
        match register(bond, pubkey, operator) {
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, collateral, payout_payload, signature, .. } => {
                PalwConsensusObjectV2::BondRegistered {
                    bond,
                    pubkey,
                    operator_pubkey,
                    collateral,
                    payout_payload,
                    capable_classes: std::collections::BTreeSet::from([class_id]),
                    signature,
                }
            }
            other => other,
        }
    }

    /// [`populated_state`] with every bond declaring `class_id` instead of the fixture's.
    fn populated_state_declaring(class_id: Hash64) -> (PalwChainStateV2, Hash64) {
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            register_declaring(1, 7, 0x21, class_id),
            register_declaring(2, 8, 0x22, class_id),
            register_declaring(3, 9, 0x23, class_id),
            register_declaring(4, 10, 0x24, class_id),
            register_declaring(5, 11, 0x24, class_id),
            register_declaring(6, 12, 0x21, class_id),
        ];
        let (s1, _) =
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        (s2, claim_id)
    }

    fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
        let bond = bond_outpoint(1);
        PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h64(999),
                challenge: challenge_v2(h64(999), h64(5), 1_700, nonce, h64(1), &bond),
                class_id: h64(1),
                executor_bond: bond,
                executor_pubkey: vec![7; 4],
                operator_id: op_id(0x21),
                artifact_root: h64(11),
                trace_root: h64(31),
                output_root: h64(32),
                pwu,
                trace_manifest_root: h64(33),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
                execution_root: h64(41),
            },
            signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }

    /// Genesis + class + executor (bond 1, operator 0x21) + five more bonds: 2..=6 with distinct
    /// operators, except bond 5 SHARES bond 4's operator (the dedup case) and bond 6 shares the
    /// EXECUTOR's operator (the exclusion case).
    fn populated_state() -> (PalwChainStateV2, Hash64) {
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            register(1, 7, 0x21), // executor
            register(2, 8, 0x22),
            register(3, 9, 0x23),
            register(4, 10, 0x24),
            register(5, 11, 0x24), // same operator as bond 4
            register(6, 12, 0x21), // executor's operator — excluded outright
        ];
        let (s1, _) =
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        (s2, claim_id)
    }

    /// **C-02 (deep fence): the weighted draw seats a full panel, is deterministic, and below the
    /// fence is byte-identical to the pre-C-02 one-ticket sortition.** `populated_state` has exactly
    /// three eligible operators for three seats, so both draws seat all three — which is what pins
    /// the liveness property: bucketing gives every eligible bond `>= 1` sub-ticket, so the eligible
    /// set (and thus a full panel) is untouched.
    #[test]
    fn c02_weighted_draw_is_a_full_deterministic_panel_and_weighted_false_is_the_legacy_draw() {
        let (state, claim_id) = populated_state();
        let params = panel_params();
        let mc = state_params().min_collateral_sompi();
        let anchor = BlockHash::from_u64_word(0x5EA7);
        let unweighted = derive_panel_v2_with_capability_proof(&state, &params, &claim_id, anchor, mc, None, false, false)
            .expect("unweighted seats");
        let weighted =
            derive_panel_v2_with_capability_proof(&state, &params, &claim_id, anchor, mc, None, false, true).expect("weighted seats");
        assert_eq!(unweighted.len(), params.seat_count as usize, "unweighted seats a full panel");
        assert_eq!(
            weighted.len(),
            params.seat_count as usize,
            "weighted seats a full panel too — the eligible set is untouched (liveness)"
        );
        // Below the fence `weighted == false` is exactly the legacy one-ticket sortition.
        assert_eq!(
            unweighted,
            derive_panel_v2(&state, &params, &claim_id, anchor, mc).expect("legacy draw"),
            "weighted=false is byte-identical to the pre-C-02 draw"
        );
        // Deterministic both sides — a pure function of (claim, anchor, registry).
        assert_eq!(
            weighted,
            derive_panel_v2_with_capability_proof(&state, &params, &claim_id, anchor, mc, None, false, true).expect("again"),
            "the weighted draw is deterministic"
        );
    }

    /// **C-02 (deep fence): the weighted draw favours higher-collateral operators.** Four eligible
    /// operators compete for three seats — one is always left out. One holds `min_collateral`, three
    /// hold `4096 × min_collateral`. Below the fence every operator has ONE ticket, so the low one is
    /// left out no more often than any other (seated ~3/4 of draws); past the fence the three whales
    /// draw 4096 sub-tickets each and crowd it out, so it is seated far less often. That is the whole
    /// point of the finding: seat probability tracks stake, so the panel that decides a claim is
    /// composed of the operators with the most to lose (whose seat then risks `claim.reserved`,
    /// BC-SYBIL). Liveness holds throughout — a full three-seat panel every draw.
    #[test]
    fn c02_stake_weighting_favours_higher_collateral_seats_past_the_deep_fence() {
        let mc = 100u64; // state_params()'s min_collateral
        let bond = |b: u64, pk: u8, op: u64, collateral: u64| PalwConsensusObjectV2::BondRegistered {
            bond: PalwBondKeyV2(bond_outpoint(b)),
            pubkey: vec![pk; 4],
            operator_pubkey: op_key(op),
            collateral,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            capable_classes: std::collections::BTreeSet::from([h64(1)]),
            signature: Vec::new(),
        };
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            bond(1, 7, 0x21, 1_000_000),  // executor — excluded from its own panel
            bond(2, 8, 0x22, mc),         // the low-collateral operator
            bond(3, 9, 0x23, mc * 4096),  // whale
            bond(4, 10, 0x24, mc * 4096), // whale
            bond(5, 11, 0x25, mc * 4096), // whale
        ];
        let (s1, _) =
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (state, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        let params = panel_params(); // seat_count 3
        let low_op = state.bond(&PalwBondKeyV2(bond_outpoint(2))).expect("the low bond").operator_id;

        let low_seated = |weighted: bool| {
            (0u64..40)
                .filter(|i| {
                    let anchor = BlockHash::from_u64_word(0xA000 + i);
                    let seats = derive_panel_v2_with_capability_proof(&state, &params, &claim_id, anchor, mc, None, false, weighted)
                        .expect("a full panel");
                    assert_eq!(seats.len(), 3, "liveness: a full three-seat panel under weighting too");
                    seats.iter().any(|s| s.operator_id == low_op)
                })
                .count()
        };
        let unweighted = low_seated(false);
        let weighted = low_seated(true);
        assert!(
            weighted * 3 < unweighted,
            "past the fence the min-collateral operator is crowded out by the whales: seated {weighted}/40 weighted vs {unweighted}/40 unweighted"
        );
    }

    /// **ADR-0124 Decisions 3–5 in the draw.** Past the panel-economy fence a bond is drawn only
    /// while it holds ten producer floors AND its free collateral covers the seat's reservation
    /// under the shared ceiling; and every eligible bond draws one ticket, so the whale is no
    /// likelier than the minimum bond — the deep fence's weighting is retired by the economy.
    #[test]
    fn adr0124_the_economy_draws_by_floor_and_headroom_and_one_ticket_a_bond() {
        let mc = 100u64; // state_params()'s min_collateral: the panel floor is 1,000
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let bond = |b: u64, pk: u8, op: u64, collateral: u64| PalwConsensusObjectV2::BondRegistered {
            bond: PalwBondKeyV2(bond_outpoint(b)),
            pubkey: vec![pk; 4],
            operator_pubkey: op_key(op),
            collateral,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            capable_classes: std::collections::BTreeSet::from([h64(1)]),
            signature: Vec::new(),
        };
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            bond(1, 7, 0x21, 1_000_000),     // executor — excluded from its own panel
            bond(2, 8, 0x22, mc),            // holds the registry's floor, not the panel's
            bond(3, 9, 0x23, 1_000),         // holds the panel floor, but its ceiling (500) is under the seat's 600
            bond(4, 10, 0x24, 2_000),        // eligible: ceiling 1,000 covers 600
            bond(5, 11, 0x25, 2_000),        // eligible
            bond(6, 12, 0x26, 2_000 * 4096), // eligible, a whale
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();
        let env = attempt(40, 1); // reserved = 40 × 5 = 200, so a seat reserves 600
        let claim_id = attempt_id_v2(&env.attempt);
        let (state, _) = apply_palw_transition_v2(&s1, &sp, &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        assert_eq!(state.claim(&claim_id).unwrap().reserved, 200);
        let params = panel_params(); // seat_count 3
        let economy = crate::palw_panel_economy_v1::PalwSeatEconomyV1 {
            panel_floor_sompi: crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(mc),
            max_exposure_ratio_permille: 500,
            reward_multiple_permille: 0,
        };
        assert_eq!(economy.panel_floor_sompi, 1_000);
        let policy = PalwPanelDrawPolicyV1 {
            weighted: true,
            economy: Some(economy),
            readiness: None,
            independence: None,
            valid_lock: None,
            stake: None,
        };

        for i in 0..40u64 {
            let anchor = BlockHash::from_u64_word(0xB000 + i);
            let seats =
                derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, policy).expect("a full panel");
            assert_eq!(seats.len(), 3);
            for seat in &seats {
                assert!(
                    (4..=6u64).any(|n| seat.bond == PalwBondKeyV2(bond_outpoint(n))),
                    "only bonds 4, 5 and 6 may sit: {:?} is under the floor or the ceiling",
                    seat.bond
                );
            }
            // One ticket a bond: the draw past the economy equals the legacy unweighted draw over the
            // same eligible set, whatever `weighted` says.
            let unweighted = PalwPanelDrawPolicyV1 {
                weighted: false,
                economy: Some(economy),
                readiness: None,
                independence: None,
                valid_lock: None,
                stake: None,
            };
            assert_eq!(
                derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, unweighted).unwrap(),
                seats,
                "the economy retires stake weighting"
            );
        }
        // Below the fence the same registry draws bond 2 and bond 3 too (the legacy predicate).
        let legacy = (0..40u64).any(|i| {
            let anchor = BlockHash::from_u64_word(0xB000 + i);
            derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, PalwPanelDrawPolicyV1::default())
                .unwrap()
                .iter()
                .any(|s| (2..=3u64).any(|n| s.bond == PalwBondKeyV2(bond_outpoint(n))))
        });
        assert!(legacy, "below the fence the registry's floor is the only bar");
        // The acceptance layer recomputes the same panel under the same policy.
        let anchor = BlockHash::from_u64_word(0xB007);
        let seats = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, policy).unwrap();
        let fact = PalwAnchorFactV2 { anchor_block: anchor, anchor_daa: 105, predecessor_daa: 104 };
        validate_panel_bound_v2_with_policy(
            &state,
            &params,
            &sp,
            &ctx(3, 106, 3),
            &claim_id,
            &fact,
            anchor,
            &seats,
            None,
            false,
            policy,
            None,
        )
        .expect("the same policy recomputes the same panel");
        assert!(
            matches!(
                validate_panel_bound_v2_with_policy(
                    &state,
                    &params,
                    &sp,
                    &ctx(3, 106, 3),
                    &claim_id,
                    &fact,
                    anchor,
                    &seats,
                    None,
                    false,
                    PalwPanelDrawPolicyV1::default(),
                    None
                ),
                Err(PalwPanelV2Error::PanelMismatch)
            ),
            "a validator below the fence refuses the economy's panel, and vice versa"
        );
    }

    // ---- ADR-0130: one operator, one lottery entry; and the seat exposure floor ----

    fn adr0130_class() -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        }
    }

    fn adr0130_bond(b: u64, pk: u8, op: u64, collateral: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::BondRegistered {
            bond: PalwBondKeyV2(bond_outpoint(b)),
            pubkey: vec![pk; 4],
            operator_pubkey: op_key(op),
            collateral,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            capable_classes: std::collections::BTreeSet::from([h64(1)]),
            signature: Vec::new(),
        }
    }

    /// A registry, then the executor's 40-pwu attempt (`reserved` = 200) in a block of `subsidy`.
    fn adr0130_state(sp: &PalwStateParamsV2, bonds: Vec<PalwConsensusObjectV2>, subsidy: u64) -> (PalwChainStateV2, Hash64) {
        let mut objects = vec![adr0130_class()];
        objects.extend(bonds);
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), sp, &ctx(1, 100, 1), &objects, None).unwrap();
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply_palw_transition_v2(&s1, sp, &PalwBlockContextV2 { subsidy, ..ctx(2, 101, 2) }, &[], Some(&env)).unwrap();
        (s2, claim_id)
    }

    fn adr0130_economy(reward_multiple_permille: u32) -> PalwPanelDrawPolicyV1 {
        PalwPanelDrawPolicyV1 {
            weighted: false,
            readiness: None,
            economy: Some(crate::palw_panel_economy_v1::PalwSeatEconomyV1 {
                panel_floor_sompi: crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(100),
                max_exposure_ratio_permille: 500,
                reward_multiple_permille,
            }),
            independence: None,
            valid_lock: None,
            stake: None,
        }
    }

    /// **ADR-0130: an operator is ONE lottery entry, whatever it holds.** Operator A holds ten
    /// eligible bonds, B, C and D one each, for three seats.
    ///
    /// Deterministically: A is entered once, with its eligible bond whose bond ticket is lowest, under
    /// an operator ticket no bond enters — so the operators seated, and their order, are the same
    /// whether A holds ten bonds or only one, anchor for anchor. Statistically: over many claim ids A
    /// and B are seated equally often (three draws in four each), where the per-bond draw below the
    /// fence seated the ten-bond operator almost always. Splitting collateral buys nothing.
    #[test]
    fn adr0130_an_operator_draws_one_entry_however_many_bonds_it_holds() {
        const A: u64 = 0x30;
        const B: u64 = 0x31;
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let mut ten = vec![adr0130_bond(1, 7, 0x21, 1_000_000)];
        for i in 0..10u64 {
            ten.push(adr0130_bond(10 + i, 40 + i as u8, A, 1_000_000));
        }
        let others =
            [adr0130_bond(20, 60, B, 1_000_000), adr0130_bond(21, 61, 0x32, 1_000_000), adr0130_bond(22, 62, 0x33, 1_000_000)];
        ten.extend(others.iter().cloned());
        let (state, claim_id) = adr0130_state(&sp, ten, 0);
        // The same registry with A holding bond 10 alone.
        let mut one = vec![adr0130_bond(1, 7, 0x21, 1_000_000), adr0130_bond(10, 40, A, 1_000_000)];
        one.extend(others.iter().cloned());
        let (lean, lean_claim) = adr0130_state(&sp, one, 0);
        assert_eq!(lean_claim, claim_id, "one attempt, one claim id, in both registries");
        let params = panel_params(); // seat_count 3
        let (a, b) = (op_id(A), op_id(B));
        let a_bonds: Vec<PalwBondKeyV2> = (10..20u64).map(|n| PalwBondKeyV2(bond_outpoint(n))).collect();
        let policy = adr0130_economy(0);

        let mut lottery_a = 0usize;
        let mut lottery_b = 0usize;
        let mut legacy_a = 0usize;
        let mut legacy_b = 0usize;
        const DRAWS: u64 = 400;
        for i in 0..DRAWS {
            let anchor = BlockHash::from_u64_word(0xC000 + i);
            let seats =
                derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, policy).expect("a full panel");
            let lean_seats =
                derive_panel_v2_with_policy(&lean, &params, &claim_id, anchor, 100, None, false, policy).expect("a full panel");
            assert_eq!(
                seats.iter().map(|s| s.operator_id).collect::<Vec<_>>(),
                lean_seats.iter().map(|s| s.operator_id).collect::<Vec<_>>(),
                "anchor {i}: the operators seated, in order, do not depend on how many bonds A holds"
            );
            let operators: std::collections::BTreeSet<Hash64> = seats.iter().map(|s| s.operator_id).collect();
            assert_eq!(operators.len(), 3, "one seat an operator");
            if let Some(seat) = seats.iter().find(|s| s.operator_id == a) {
                // A sits with the one of its bonds whose bond ticket is lowest.
                let best = a_bonds.iter().min_by_key(|bond| (palw_panel_seat_ticket_v1(anchor, &claim_id, bond), **bond)).unwrap();
                assert_eq!(seat.bond, *best, "anchor {i}: an operator sits with its lowest-ticket eligible bond");
                lottery_a += 1;
            }
            lottery_b += usize::from(operators.contains(&b));
            // The same registry below the economy: one ticket a BOND.
            let legacy =
                derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, PalwPanelDrawPolicyV1::default())
                    .expect("a full legacy panel");
            legacy_a += usize::from(legacy.iter().any(|s| s.operator_id == a));
            legacy_b += usize::from(legacy.iter().any(|s| s.operator_id == b));
        }
        // Four operators, three seats: each is seated three draws in four.
        for (who, count) in [("A (ten bonds)", lottery_a), ("B (one bond)", lottery_b)] {
            assert!((240..=360).contains(&count), "{who} seated {count}/{DRAWS} under the operator lottery; expected about 300");
        }
        assert!(lottery_a.abs_diff(lottery_b) <= 60, "equal odds: A {lottery_a}, B {lottery_b} of {DRAWS}");
        assert!(
            legacy_a >= 390 && legacy_b <= 330,
            "below the fence ten tickets all but guaranteed A a seat: A {legacy_a}, B {legacy_b} of {DRAWS}"
        );

        // Over many CLAIM ids, straight through the lottery over one eligible list.
        let eligible =
            palw_panel_eligible_bonds_v2(&state, &claim_id, 100, None, false, None, policy.economy, params.seat_count).unwrap();
        assert_eq!(eligible.len(), 13, "ten of A's bonds and one each of B, C and D");
        let anchor = BlockHash::from_u64_word(0xD00D);
        let (mut by_claim_a, mut by_claim_b) = (0usize, 0usize);
        for word in 0..DRAWS {
            let claim = h64(0xE000_0000 + word);
            let entries = palw_panel_operator_entries_v1(&claim, anchor, &eligible);
            assert_eq!(entries.len(), 4, "one entry an operator, never one a bond");
            let entry_a = entries.iter().find(|e| e.operator_id == a).unwrap();
            assert_eq!(entry_a.operator_ticket, palw_panel_operator_ticket_v1(anchor, &claim, &a), "no bond enters the ticket");
            let seats = palw_panel_operator_lottery_v1(&params, &claim, anchor, &eligible).unwrap();
            by_claim_a += usize::from(seats.iter().any(|s| s.operator_id == a));
            by_claim_b += usize::from(seats.iter().any(|s| s.operator_id == b));
        }
        assert!((240..=360).contains(&by_claim_a) && (240..=360).contains(&by_claim_b), "A {by_claim_a}, B {by_claim_b} of {DRAWS}");
        assert!(by_claim_a.abs_diff(by_claim_b) <= 60, "equal odds over claim ids: A {by_claim_a}, B {by_claim_b}");

        // The lottery refuses a short jury by name, as the per-bond draw did.
        let three: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> =
            eligible.iter().copied().filter(|(_, bond)| bond.operator_id == a || bond.operator_id == b).collect();
        assert_eq!(
            palw_panel_operator_lottery_v1(&params, &claim_id, anchor, &three),
            Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 3, available: 2 }),
            "eleven bonds under two operators seat nobody on a three-seat panel"
        );
    }

    /// **ADR-0130: below the panel economy the draw is the per-bond sortition, byte for byte** — the
    /// bond ticket's spelling moved into one function, and the legacy draw is still "sort every
    /// eligible bond by it, first bond of each operator sits". And past the economy the acceptance
    /// layer refuses the per-bond panel wherever the lottery deals a different one.
    #[test]
    fn adr0130_below_the_economy_the_draw_is_the_per_bond_sortition_and_past_it_the_per_bond_panel_is_refused() {
        let (state, claim_id) = populated_state();
        let params = panel_params();
        let sp = state_params();
        let mc = sp.min_collateral_sompi();
        // The legacy ticket, spelled out by hand.
        let bond = PalwBondKeyV2(bond_outpoint(2));
        let anchor = BlockHash::from_u64_word(0x5EA7);
        let mut by_hand = keyed(PALW_PANEL_V2_DOMAIN_SEAT_TICKET);
        by_hand.update(anchor.as_byte_slice());
        by_hand.update(claim_id.as_byte_slice());
        by_hand.update(&borsh::to_vec(&bond).unwrap());
        assert_eq!(palw_panel_seat_ticket_v1(anchor, &claim_id, &bond), finish(by_hand), "the bond ticket is the one it always was");

        let mut refused_somewhere = false;
        for i in 0..40u64 {
            let anchor = BlockHash::from_u64_word(0xF000 + i);
            let legacy = derive_panel_v2(&state, &params, &claim_id, anchor, mc).unwrap();
            let eligible = palw_panel_eligible_bonds_v2(&state, &claim_id, mc, None, false, None, None, params.seat_count).unwrap();
            let mut ranked: Vec<(Hash64, PalwBondKeyV2, Hash64)> =
                eligible.iter().map(|(k, b)| (palw_panel_seat_ticket_v1(anchor, &claim_id, k), **k, b.operator_id)).collect();
            ranked.sort();
            let mut expected: Vec<PalwPanelSeatV2> = Vec::new();
            for (_, bond, operator_id) in ranked {
                if expected.len() < params.seat_count as usize && !expected.iter().any(|s| s.operator_id == operator_id) {
                    expected.push(PalwPanelSeatV2 { bond, operator_id });
                }
            }
            assert_eq!(legacy, expected, "anchor {i}: the per-bond sortition, unchanged");

            // Past the economy (no floor), the same registry and anchor.
            let economy = PalwPanelDrawPolicyV1 {
                weighted: false,
                readiness: None,
                economy: Some(crate::palw_panel_economy_v1::PalwSeatEconomyV1 {
                    panel_floor_sompi: mc,
                    max_exposure_ratio_permille: 1000,
                    reward_multiple_permille: 0,
                }),
                independence: None,
                valid_lock: None,
                stake: None,
            };
            let lottery = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, economy).unwrap();
            let fact = PalwAnchorFactV2 { anchor_block: anchor, anchor_daa: 105, predecessor_daa: 104 };
            let validate = |seats: &[PalwPanelSeatV2], policy| {
                validate_panel_bound_v2_with_policy(
                    &state,
                    &params,
                    &sp,
                    &ctx(3, 106, 3),
                    &claim_id,
                    &fact,
                    anchor,
                    seats,
                    None,
                    false,
                    policy,
                    None,
                )
            };
            assert_eq!(validate(&lottery, economy), Ok(()), "anchor {i}: derive and validate agree past the economy");
            assert_eq!(validate(&legacy, PalwPanelDrawPolicyV1::default()), Ok(()), "…and below it");
            if lottery != legacy {
                refused_somewhere = true;
                assert_eq!(
                    validate(&legacy, economy),
                    Err(PalwPanelV2Error::PanelMismatch),
                    "anchor {i}: the per-bond panel is refused"
                );
                assert_eq!(validate(&lottery, PalwPanelDrawPolicyV1::default()), Err(PalwPanelV2Error::PanelMismatch));
            }
        }
        assert!(refused_somewhere, "the two draws must differ somewhere, or the refusal above is vacuous");
    }

    /// **The 2026-09-23 route-matrix audit's #3: a bond that cannot post the bind's Valid lock is
    /// not drawn, and the panel the draw names instead is one the bind takes.**
    ///
    /// Bond 4 clears the registry floor and backs the claim-priced stake, but its whole collateral
    /// is one sompi short of what one `Valid` signature on the claim must lock. Drawn blind to the
    /// lock, it lands on the panel for some anchors, and past the audit fence that `PanelBound` is
    /// inert — the claim stays `Provisional` until its bind window voids it (the audit's probe: one
    /// such bond blocked 17 of 20 floor claims). With the lock in the policy it is never drawn, the
    /// panel is always bonds 2, 3 and 5, and the fold binds it.
    #[test]
    fn route_matrix_3_a_bond_that_cannot_post_the_valid_lock_is_not_drawn() {
        use crate::palw_state_v2::{PalwClaimPhaseV2, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras};
        let sp = state_params();
        let extras =
            PalwTransitionExtrasV1 { objective_offence_daa: Some(0), audit_2026_09_23_active: true, ..Default::default() };
        let registry = |cheap: u64| {
            vec![
                adr0130_bond(1, 7, 0x21, 1_000_000), // executor — excluded from its own panel
                adr0130_bond(2, 8, 0x22, 1_000_000),
                adr0130_bond(3, 9, 0x23, 1_000_000),
                adr0130_bond(4, 10, 0x24, cheap),
                adr0130_bond(5, 11, 0x25, 1_000_000),
            ]
        };
        // A 4,000-pwu attempt, so its lock sits well above the registry's 100-sompi floor.
        let with_claim = |bonds: Vec<PalwConsensusObjectV2>| {
            let mut objects = vec![adr0130_class()];
            objects.extend(bonds);
            let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();
            let env = attempt(4_000, 1);
            let (s2, _) = apply_palw_transition_v2(&s1, &sp, &ctx(2, 101, 2), &[], Some(&env)).unwrap();
            (s2, attempt_id_v2(&env.attempt))
        };
        // The lock is a function of the claim alone, so price it once on an ample registry.
        let (probe, probe_claim) = with_claim(registry(1_000_000));
        let required =
            crate::palw_state_v2::palw_panel_valid_lock_required_v1(&probe, &sp, &extras, probe.claim(&probe_claim).unwrap());
        assert!(required > 101, "the lock ({required}) sits above the registry floor, or this test proves nothing");
        let (state, claim_id) = with_claim(registry(required as u64 - 1));
        assert_eq!(claim_id, probe_claim);
        let lock =
            PalwPanelValidLockV1 { required, now_daa: 103, settled_anchor_depth: None, window_court: sp.window_court(), rcore: None };
        let cheap = PalwBondKeyV2(bond_outpoint(4));
        assert!(!lock.admits(&state, &cheap) && lock.admits(&state, &PalwBondKeyV2(bond_outpoint(2))));

        let params = panel_params(); // seat_count 3
        let with_lock = PalwPanelDrawPolicyV1 { valid_lock: Some(lock), ..Default::default() };
        let bind = |seats: &[PalwPanelSeatV2]| {
            let (next, _) = apply_palw_transition_v2_with_extras(
                &state,
                &sp,
                &ctx(3, 103, 3),
                &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: BlockHash::from_u64_word(0x77), seats: seats.to_vec() }],
                None,
                false,
                false,
                false,
                false,
                &extras,
            )
            .expect("an ineligible panel is inert past the fence, never the block's error");
            matches!(next.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. })
        };
        let mut blind_drew_the_cheap_bond = false;
        for i in 0..24u64 {
            let anchor = BlockHash::from_u64_word(0x3A00 + i);
            let blind = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, PalwPanelDrawPolicyV1::default())
                .unwrap();
            if blind.iter().any(|seat| seat.bond == cheap) {
                blind_drew_the_cheap_bond = true;
                assert!(!bind(&blind), "anchor {i}: the bind refuses the seat that cannot post its lock");
            }
            let seats = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, with_lock).unwrap();
            let mut sat: Vec<PalwBondKeyV2> = seats.iter().map(|seat| seat.bond).collect();
            sat.sort();
            let mut backed: Vec<PalwBondKeyV2> = [2u64, 3, 5].iter().map(|n| PalwBondKeyV2(bond_outpoint(*n))).collect();
            backed.sort();
            assert_eq!(sat, backed, "anchor {i}: only bonds that can post the lock sit");
            assert!(bind(&seats), "anchor {i}: and the bind takes that panel");
        }
        assert!(blind_drew_the_cheap_bond, "the blind draw must seat bond 4 somewhere, or the refusal above is vacuous");
    }

    /// **ADR-0130: a bond is drawn only while its free collateral can reserve the floor.** The claim
    /// escrows 62,000 (620 ‰ of a 100,000 subsidy), so a seat of a three-seat panel is paid at most
    /// 12,400 / 3 = 4,133; its claim-priced stake is 3 × 200 = 600. Under a 500 ‰ ceiling a 2,000-sompi
    /// bond backs 1,000 — enough for 600, not for the floor — and a 10,000-sompi bond backs 5,000.
    #[test]
    fn adr0130_a_bond_that_cannot_reserve_the_floor_is_not_drawn_and_too_few_operators_refuse() {
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap().with_worker_carve_permille(620).unwrap();
        let bonds = vec![
            adr0130_bond(1, 7, 0x21, 1_000_000), // executor — excluded from its own panel
            adr0130_bond(2, 8, 0x22, 2_000),     // backs 1,000: the stake, never the floor
            adr0130_bond(3, 9, 0x23, 2_000),
            adr0130_bond(4, 10, 0x24, 10_000), // backs 5,000
            adr0130_bond(5, 11, 0x25, 10_000),
            adr0130_bond(6, 12, 0x26, 9_000), // backs 4,500
        ];
        let (state, claim_id) = adr0130_state(&sp, bonds, 100_000);
        let claim = state.claim(&claim_id).unwrap();
        assert_eq!((claim.escrowed_reward, claim.reserved), (62_000, 200));
        let params = panel_params(); // seat_count 3
        let seat = |lambda| crate::palw_panel_economy_v1::palw_panel_seat_exposure_v1(200, 62_000, 3, lambda);
        assert_eq!((seat(0), seat(1_000), seat(1_100), seat(2_000)), (600, 4_133, 4_546, 8_266));

        let eligible = |lambda| -> Vec<u64> {
            let mut out: Vec<u64> = palw_panel_eligible_bonds_v2(
                &state,
                &claim_id,
                100,
                None,
                false,
                None,
                adr0130_economy(lambda).economy,
                params.seat_count,
            )
            .unwrap()
            .into_iter()
            .map(|(key, _)| (2..=6u64).find(|n| *key == PalwBondKeyV2(bond_outpoint(*n))).unwrap())
            .collect();
            out.sort();
            out
        };
        assert_eq!(eligible(0), vec![2, 3, 4, 5, 6], "no floor: every bond backs the claim-priced stake");
        assert_eq!(eligible(1_000), vec![4, 5, 6], "λ = 1: the 2,000-sompi bonds cannot reserve 4,133");
        assert_eq!(eligible(1_100), vec![4, 5], "λ = 1.1: nor can the 9,000-sompi bond reserve 4,546");
        assert!(eligible(2_000).is_empty(), "λ = 2: nobody here can reserve 8,266");

        for i in 0..20u64 {
            let anchor = BlockHash::from_u64_word(0xA130 + i);
            let seats = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, adr0130_economy(1_000))
                .expect("three operators can reserve the floor");
            let mut sat: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.bond).collect();
            sat.sort();
            let mut thick: Vec<PalwBondKeyV2> = (4..=6u64).map(|n| PalwBondKeyV2(bond_outpoint(n))).collect();
            thick.sort();
            assert_eq!(sat, thick, "anchor {i}: only the bonds that can reserve the floor sit");
            // The acceptance layer recomputes the same panel under the same floor, and refuses it
            // under none wherever the two differ.
            let fact = PalwAnchorFactV2 { anchor_block: anchor, anchor_daa: 105, predecessor_daa: 104 };
            let validate = |policy| {
                validate_panel_bound_v2_with_policy(
                    &state,
                    &params,
                    &sp,
                    &ctx(3, 106, 3),
                    &claim_id,
                    &fact,
                    anchor,
                    &seats,
                    None,
                    false,
                    policy,
                    None,
                )
            };
            assert_eq!(validate(adr0130_economy(1_000)), Ok(()), "anchor {i}: derive and validate agree");
            let unfloored =
                derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, adr0130_economy(0)).unwrap();
            if unfloored != seats {
                assert_eq!(validate(adr0130_economy(0)), Err(PalwPanelV2Error::PanelMismatch));
            }
        }
        for (lambda, available) in [(1_100u32, 2u16), (2_000, 0)] {
            assert_eq!(
                derive_panel_v2_with_policy(
                    &state,
                    &params,
                    &claim_id,
                    BlockHash::from_u64_word(0xA130),
                    100,
                    None,
                    false,
                    adr0130_economy(lambda)
                ),
                Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 3, available }),
                "λ = {lambda} ‰: too few operators can reserve the floor, and a short jury is refused"
            );
        }
    }

    /// **D2's fast path must never skip a panel that could be majority-new.**
    ///
    /// The gate scans deltas only when this says so, so a `false` that should have been `true` is a
    /// rule that silently does not apply. Checked exhaustively over every legal `(seat_count,
    /// quorum)` this ruleset admits, against the definition it is a shortcut for: "some panel could
    /// seat `quorum` bonds that are all new".
    #[test]
    fn the_provenance_fast_path_never_skips_a_reachable_quorum() {
        for seats in 1u16..=16 {
            for quorum in 1u16..=seats {
                let Ok(params) = PalwPanelParamsV2::new(seats, quorum, 4) else { continue };
                for minted in 0usize..=(seats as usize + 2) {
                    // The thing the scan looks for: a panel of `seats` seats holding `quorum` or
                    // more bonds drawn from the `minted` new ones. It is reachable exactly when
                    // there are at least `quorum` of them.
                    let reachable = minted >= quorum as usize;
                    let scans = palw_minted_seats_can_reach_quorum_v1(minted, &params);
                    assert!(
                        !reachable || scans,
                        "seats {seats} quorum {quorum} minted {minted}: a quorum of new seats is reachable and the gate would not look"
                    );
                }
            }
        }
    }

    /// And on the shipped shape it is the number the design names, so a change to either half of
    /// the panel is visible here rather than only in a comment.
    #[test]
    fn the_shipped_panel_tolerates_two_minted_bonds_without_scanning() {
        let shipped =
            PalwPanelParamsV2::new(crate::palw_fp_devnet_v3::PALW_V2_PANEL_SEATS, crate::palw_fp_devnet_v3::PALW_V2_PANEL_QUORUM, 4)
                .expect("the shipped panel shape is legal");
        assert!(!palw_minted_seats_can_reach_quorum_v1(2, &shipped), "two new bonds cannot be a majority of five seats");
        assert!(palw_minted_seats_can_reach_quorum_v1(3, &shipped), "three can, so the gate looks");
    }

    #[test]
    fn params_refuse_shapes_that_cannot_judge() {
        assert!(PalwPanelParamsV2::new(0, 1, 1).is_err(), "zero seats");
        assert!(PalwPanelParamsV2::new(3, 0, 1).is_err(), "zero quorum");
        assert!(PalwPanelParamsV2::new(3, 4, 1).is_err(), "quorum above seats");
        // Audit C5: a non-majority quorum lets Valid and Unavailable both form on one panel.
        assert!(PalwPanelParamsV2::new(4, 2, 1).is_err(), "2 of 4 is not a majority — both verdicts could reach it");
        assert!(PalwPanelParamsV2::new(3, 1, 1).is_err(), "1 of 3 lets one seat void an honest claim");
        assert!(PalwPanelParamsV2::new(4, 3, 1).is_ok(), "3 of 4 is a strict majority");
        assert!(PalwPanelParamsV2::new(1, 1, 1).is_ok(), "a single-seat panel is degenerate but self-consistent");
        assert!(PalwPanelParamsV2::new(3, 3, 0).is_err(), "zero anchor delay");
        let p = PalwPanelParamsV2::new(3, 2, 4).unwrap();
        assert!(p.validate_against_state_params(&state_params()).is_ok(), "delay 4 inside bind window 10");
        let too_late = PalwPanelParamsV2::new(3, 2, 10).unwrap();
        assert!(too_late.validate_against_state_params(&state_params()).is_err(), "anchor at the deadline binds nothing");
    }

    /// **A seat that cannot judge is not charged for saying so — except on the floor.**
    ///
    /// Sortition never asked which classes a node can execute, so a seat routinely landed on a
    /// family it did not hold. With two verdicts it had no honest move: `Valid` is a lie,
    /// `Unavailable` is a signed accusation against a producer that did nothing wrong, and silence
    /// is charged as a no-show. Every road ended in a slash for the offence of being picked.
    ///
    /// The escape has to be closed on the liveness floor, or a quorum of seats could plead their
    /// way out of judging the one class the chain is guaranteed to have.
    #[test]
    fn a_seat_may_plead_incapable_except_on_the_floor() {
        use crate::palw_state_v2::{PalwSeatAnswerV2, palw_seat_may_plead_incapable_v2, palw_seat_verdicts_of_v2};
        let floor = h64(0xF100);
        assert!(!palw_seat_may_plead_incapable_v2(floor, floor), "no node may claim it cannot run BASE-0");
        assert!(palw_seat_may_plead_incapable_v2(h64(0xC1), floor), "an entrant class is a different matter");

        // And the plea reduces to an answer that takes no side, so the transition can tell it
        // apart from both a vote and a no-show.
        let receipts = vec![
            PalwSeatReceiptV2 {
                claim: h64(1),
                seat_bond: PalwBondKeyV2(bond_outpoint(2)),
                verdict: PalwReceiptVerdictV2::Incapable,
                signed_daa: 1,
                signature: Vec::new(),
            },
            PalwSeatReceiptV2 {
                claim: h64(1),
                seat_bond: PalwBondKeyV2(bond_outpoint(3)),
                verdict: PalwReceiptVerdictV2::Valid,
                signed_daa: 1,
                signature: Vec::new(),
            },
        ];
        let answers = palw_seat_verdicts_of_v2(&receipts);
        assert_eq!(answers[0].answer, PalwSeatAnswerV2::Incapable);
        assert_eq!(answers[1].answer, PalwSeatAnswerV2::Served);
    }

    /// **A bond with nothing left to lose stops being a seat.**
    ///
    /// `Active` is a status, not a balance. `slash_bond` clamps every debit to the collateral that
    /// remains, so once a bond reaches zero every further charge is a silent success — and the
    /// sortition, which only ever asked for `Active`, went on seating it for the rest of the
    /// chain's life. That is a juror who cannot be fined: the one seat a fraud court must not have.
    ///
    /// Asserted on the predicate the draw calls, because the state transition offers no way to
    /// hand a test an exhausted bond without running a whole court to produce one.
    /// **A seat must be able to run the class it is drawn to judge** (ADR-0071 Decision 3).
    ///
    /// Asserted as a difference: the same registry, the same anchor, the same claim — only the
    /// declarations move. A test that checked the positive case alone would pass for a draw that
    /// ignored capability entirely, which is the state this Decision found.
    #[test]
    fn a_bond_that_cannot_run_the_class_is_not_drawn_to_judge_it() {
        let (state, claim_id) = populated_state();
        let anchor = BlockHash::from_u64_word(0xA0C0);

        // Every bond declares the fixture's class: the draw seats, as it always did.
        let full = derive_panel_v2(&state, &panel_params(), &claim_id, anchor, 0).expect("a declaring registry seats");
        assert_eq!(full.len(), 3, "the baseline is the same panel the exclusion test asserts, or this measures nothing");

        // The same registry, the same anchor, the same claim — declaring a class nobody is
        // claiming under. Nothing else moves.
        let (deaf, deaf_claim) = populated_state_declaring(h64(0xDEAD));
        assert_eq!(deaf_claim, claim_id, "the claim is the same one; only the declarations differ");
        let err = derive_panel_v2(&deaf, &panel_params(), &claim_id, anchor, 0)
            .expect_err("a registry that cannot run the class seats nobody");
        assert!(
            matches!(err, PalwPanelV2Error::InsufficientEligibleBonds { .. }),
            "and it fails CLOSED — a short panel is not a smaller panel, it is a claim that never binds: {err:?}"
        );
    }

    #[test]
    fn a_bond_with_nothing_left_to_lose_may_not_take_work() {
        use crate::palw_state_v2::{PalwBondStateV2, PalwBondStatusV2, palw_bond_may_take_work_v2};
        let live = PalwBondStateV2 {
            pubkey: vec![1, 2, 3],
            operator_id: h64(0x21),
            collateral: 1_000,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: Hash64::default(),
            capable_classes: Default::default(),
        };
        assert!(palw_bond_may_take_work_v2(&live, 1_000), "a fully-collateralised Active bond seats");

        let exhausted = PalwBondStateV2 { collateral: 0, slashed: 1_000, ..live.clone() };
        assert!(matches!(exhausted.status, PalwBondStatusV2::Active), "slashing never changed the status — that is the defect");
        assert!(!palw_bond_may_take_work_v2(&exhausted, 0), "and it must not seat even where the floor is zero");

        let thin = PalwBondStateV2 { collateral: 999, slashed: 1, ..live.clone() };
        assert!(!palw_bond_may_take_work_v2(&thin, 1_000), "a bond that could not register today does not seat today");

        let retiring = PalwBondStateV2 { status: PalwBondStatusV2::Retiring { since_daa: 5, settled_at_since: 0 }, ..live };
        assert!(!palw_bond_may_take_work_v2(&retiring, 0), "and retirement still excludes, collateral or not");
    }

    // ---- ADR-0071 SA-3: a seat is drawn only after a production fact -------------------------

    /// [`populated_state`] with class `h64(1)` registered by a REGISTRANT bond rather than by the
    /// genesis assembly, folded past the capability fence.
    ///
    /// The distinction is the whole of SA-3's exemption: a class with `registrant_bond == None` is
    /// one a genesis assembly registered, and those stay drawable on a fresh network; a class an
    /// entrant bought has to be proven by production. This fixture builds the second kind, so the
    /// test can see the rule bite.
    fn populated_state_with_a_registrant_class() -> (PalwChainStateV2, Hash64) {
        use crate::palw_state_v2::{PalwBlockWorkV3, PalwClassAdmissionCarriageV2, apply_palw_transition_v6};
        let profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor's geometry projects");
        let canonical = crate::palw_base0_profile::rc_job_context(&profile, 2, 2);
        // The bonds come first: a registration's carriage names a bond the chain must already hold.
        let bonds = vec![
            register(1, 7, 0x21),
            register(2, 8, 0x22),
            register(3, 9, 0x23),
            register(4, 10, 0x24),
            register(5, 11, 0x24),
            register(6, 12, 0x21),
        ];
        let (s0, ..) = apply_palw_transition_v6(
            &PalwChainStateV2::genesis(),
            &state_params(),
            None,
            &ctx(1, 100, 1),
            &bonds,
            PalwBlockWorkV3::None,
            &[],
            false,
            true,
            false,
            false,
        )
        .expect("the registry loads");
        let class = PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            // The transition reads only `registrant_bond` from the carriage — the graph walk is
            // the acceptance layer's — so this is the cheapest well-formed stand-in.
            admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
                registrant_bond: PalwBondKeyV2(bond_outpoint(1)),
                profile,
                canonical,
                signature: Vec::new(),
            })),
        };
        let (s1, ..) = apply_palw_transition_v6(
            &s0,
            &state_params(),
            None,
            &ctx(2, 101, 2),
            &[class],
            PalwBlockWorkV3::None,
            &[],
            false,
            true,
            false,
            false,
        )
        .expect("an entrant registers the class");
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, ..) = apply_palw_transition_v6(
            &s1,
            &state_params(),
            None,
            &ctx(3, 102, 3),
            &[],
            PalwBlockWorkV3::Attempt(&env),
            &[],
            false,
            true,
            false,
            false,
        )
        .expect("the executor produces on it");
        (s2, claim_id)
    }

    /// **SA-3: declaring is a claim, producing is a proof.**
    ///
    /// Every bond in the fixture declared the class. Only the executor has ever produced on it,
    /// and the executor is excluded from its own panel — so past the fence the draw finds nobody,
    /// and it fails CLOSED, which is `derive_panel_v2`'s standing behaviour for a registry it
    /// cannot seat. Before the fence the same registry seats a full panel, which is the half that
    /// says the rule and not the fixture is what changed.
    ///
    /// Silence is not judged here and that is the point: the five bonds that declared and never
    /// produced are not charged, not convicted and not accused. They are simply not drawn.
    #[test]
    fn a_bond_that_declared_but_never_produced_is_not_drawn_for_a_registrant_class() {
        let (state, claim_id) = populated_state_with_a_registrant_class();
        let anchor = BlockHash::from_u64_word(0xA0C0);

        let before =
            derive_panel_v2(&state, &panel_params(), &claim_id, anchor, 0).expect("without the fence the registry seats a panel");
        assert_eq!(before.len(), 3, "three eligible seats, as in every other fixture");

        let err = derive_panel_v2_with_capability_proof(&state, &panel_params(), &claim_id, anchor, 0, None, true, false)
            .expect_err("past the fence only a bond that produced on this class may judge it");
        assert!(
            matches!(err, PalwPanelV2Error::InsufficientEligibleBonds { available: 0, .. }),
            "and it fails CLOSED rather than seating a short panel: {err:?}"
        );

        // The executor DID produce — the fact is on the chain — and is excluded for a different
        // reason entirely. Asserting it here keeps the two exclusions from being confused.
        let executor = PalwBondKeyV2(bond_outpoint(1));
        assert!(
            crate::palw_state_v2::palw_bond_produced_on_class_v1(&state, &executor, &h64(1)),
            "the producer's own fact was recorded"
        );
        for bond in [2u64, 3, 4, 5, 6] {
            assert!(
                !crate::palw_state_v2::palw_bond_produced_on_class_v1(&state, &PalwBondKeyV2(bond_outpoint(bond)), &h64(1)),
                "and nobody else's was"
            );
        }
    }

    /// **The liveness proof the fence cannot ship without.**
    ///
    /// A short draw is `InsufficientEligibleBonds`, so every claim then voids at `BindTimeout`
    /// with its escrow burned — and the shipped genesis registry has ZERO slack by construction
    /// (`seat_count + 1` bonds, executor excluded) with no bond having produced anything at block
    /// 0. What keeps it drawable past the fence is the genesis-class exemption, and this asserts
    /// it over the REAL shipped assembly rather than a fixture: for every class the card
    /// registers, the fence removes exactly nobody, and what is left is still enough operators to
    /// seat a panel after excluding any one of them as the executor.
    #[test]
    fn the_shipped_registry_still_draws_a_full_panel_with_the_capability_fence_armed() {
        use crate::config::params::palw_rc_shipped_params;
        use crate::palw_mode_v2::PalwConsensusMode;
        use crate::palw_state_v2::{palw_bond_may_judge_class_v3, palw_bond_may_take_work_v2};

        let params = palw_rc_shipped_params();
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("the shipped RC preset carries a V2 bundle");
        };
        let sp = bundle.state.clone();
        let genesis_ctx =
            PalwBlockContextV2 { block: params.genesis.hash, daa_score: params.genesis.daa_score, blue_score: 0, subsidy: 0 };
        let (booted, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &genesis_ctx, &bundle.genesis_objects, None)
            .expect("the shipped genesis applies");

        let seats_needed = bundle.panel.seat_count() as usize;
        let min_collateral = sp.min_collateral_sompi();
        let classes: Vec<Hash64> = booted.classes_iter().map(|(id, _)| *id).collect();
        assert!(!classes.is_empty(), "the shipped card registers classes");

        for class_id in classes {
            let count = |proof: bool| -> usize {
                let mut operators: Vec<Hash64> = booted
                    .bonds_iter()
                    .filter(|(key, bond)| {
                        palw_bond_may_take_work_v2(bond, min_collateral)
                            && palw_bond_may_judge_class_v3(&booted, key, bond, &class_id, proof)
                    })
                    .map(|(_, bond)| bond.operator_id)
                    .collect();
                operators.sort();
                operators.dedup();
                operators.len()
            };
            let open = count(false);
            let fenced = count(true);
            assert_eq!(
                open, fenced,
                "class {class_id:?}: the capability fence removed a seat from the shipped genesis registry — every claim on this \
                 network would void at BindTimeout. The genesis-class exemption is what must keep this equal."
            );
            // The executor is drawn out of this same set, so a panel needs one spare.
            assert!(
                fenced > seats_needed,
                "class {class_id:?}: {fenced} eligible operators past the fence, and a panel of {seats_needed} needs one spare for \
                 the executor it excludes"
            );
        }
    }

    /// **The audit register's P0-7 exclusion red test.** The trio reads the ONE registry: the
    /// executor's bond, its operator (even on a different bond), and its key never seat — and
    /// one operator never seats twice.
    #[test]
    fn palw_v2_executor_excluded_from_own_panel() {
        let (state, claim_id) = populated_state();
        let anchor = BlockHash::from_u64_word(0xA0C0);
        let seats = derive_panel_v2(&state, &panel_params(), &claim_id, anchor, 0).expect("3 eligible seats exist");
        assert_eq!(seats.len(), 3);
        for seat in &seats {
            assert_ne!(seat.bond, PalwBondKeyV2(bond_outpoint(1)), "the executor's bond never seats");
            assert_ne!(seat.bond, PalwBondKeyV2(bond_outpoint(6)), "the executor's OPERATOR never seats, whatever the bond");
            assert_ne!(seat.operator_id, op_id(0x21), "no seat carries the executor's operator id");
        }
        let mut operators: Vec<Hash64> = seats.iter().map(|s| s.operator_id).collect();
        operators.sort();
        operators.dedup();
        assert_eq!(operators.len(), seats.len(), "one operator, one seat");
        // Eligible after exclusions: bonds 2, 3, and ONE of {4, 5} — exactly 3. Which of 4/5 seats
        // is the ticket order's business; that it is exactly one of them is the dedup working.
        assert!(seats.iter().any(|s| s.operator_id == op_id(0x24)), "the shared operator got exactly one of its two bonds seated");
    }

    /// **ADR-0065 D1 — a bond may not be minted and used in the same breath.**
    ///
    /// The safety claim it serves: a holder of one bond could fork from any point, fold sybil
    /// `BondRegistered` objects into the fork's OWN blocks, seat panels from them immediately and
    /// grow a private `safe_frontier`. `palw_fork_choice`'s stated invariant — *a fork nobody could
    /// see collects no receipts, so it has no frontier* — was false, because the fork could mint
    /// its own jurors. With a window, seating them costs the fork the DAA it has to actually
    /// advance.
    ///
    /// Both positions on ONE registry, because the rule is only visible as a difference. And the
    /// third assertion is the one that matters for liveness: the same floor that excludes the
    /// newcomer leaves every older bond exactly where it was — a floor that quietly thinned the
    /// existing registry would stop a live chain, since a short draw is no panel at all.
    #[test]
    fn adr_0065_d1_a_fresh_bond_may_not_take_a_seat_until_it_has_stood() {
        let (state, claim_id) = populated_state();
        // The whole registry stands at DAA 100; one newcomer registers a century later.
        let (late, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(3, 200, 3), &[register(7, 13, 0x27)], None).unwrap();
        let anchor = BlockHash::from_u64_word(0xA1);
        let four = PalwPanelParamsV2::new(4, 3, 4).unwrap();

        // FENCE OFF — today's rule. The newcomer is seatable the instant it is registered, which
        // is exactly the property that makes a private fork's own bonds usable on that fork.
        let seats = derive_panel_v2(&late, &four, &claim_id, anchor, 0).expect("four eligible once the newcomer exists");
        assert!(
            seats.iter().any(|s| s.bond == PalwBondKeyV2(bond_outpoint(7))),
            "without the rule a bond registered a moment ago judges a claim"
        );

        // FENCE ON. Anchor at 250, window 100 ⇒ a seat's bond must date from 150 or earlier.
        let floor = palw_seat_maturity_floor_v1(250, Some(100));
        assert_eq!(floor, Some(150), "the floor is the anchor minus the window, computed in one place");
        assert!(
            matches!(
                derive_panel_v2_with_maturity(&late, &four, &claim_id, anchor, 0, floor),
                Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 4, available: 3 })
            ),
            "the newcomer is not eligible yet, and a short draw is no panel — never a smaller one"
        );

        // …and the rule takes nothing away from the bonds that were already standing: with the
        // floor, the draw is exactly the draw the registry made BEFORE the newcomer existed. (This
        // used to compare against the post-registration draw without the floor, which only agrees
        // when the newcomer happens not to be drawn — a fact about one claim id, not about the
        // rule, and the ADR-0072 attempt-version bump moved the id and exposed it.)
        let unchanged =
            derive_panel_v2_with_maturity(&late, &panel_params(), &claim_id, anchor, 0, floor).expect("the older bonds still seat");
        assert_eq!(unchanged, derive_panel_v2(&state, &panel_params(), &claim_id, anchor, 0).unwrap(), "same seats, same order");
        assert!(unchanged.iter().all(|s| s.bond != PalwBondKeyV2(bond_outpoint(7))), "and none of them is the newcomer");

        // Once the window has actually elapsed the newcomer joins on its own merits.
        let matured = palw_seat_maturity_floor_v1(400, Some(100));
        assert_eq!(matured, Some(300));
        let seats = derive_panel_v2_with_maturity(&late, &four, &claim_id, anchor, 0, matured).expect("the newcomer has now stood");
        assert!(seats.iter().any(|s| s.bond == PalwBondKeyV2(bond_outpoint(7))), "maturity is a delay, not an exclusion");

        // `None` is the rule off, and a window longer than the chain saturates to zero rather than
        // wrapping to `u64::MAX` — which would admit every bond and read as the rule working.
        assert_eq!(palw_seat_maturity_floor_v1(250, None), None);
        assert_eq!(palw_seat_maturity_floor_v1(10, Some(1_000)), Some(0));
    }

    /// **ADR-0065 D1 is armable on the genesis this build actually ships.**
    ///
    /// `arming_bond_maturity_needs_a_registry_with_a_spare_seat` (params.rs) proves the CONFIG
    /// gate accepts the fence on the shipped preset. That is a boot check; it says nothing about
    /// whether panels still draw once the rule is in force, and a fence that validates and then
    /// starves every draw is the same halt with a friendlier error site.
    ///
    /// So this runs the shipped registry: apply the genesis objects exactly as a booting node
    /// does, bind a claim under one of the real cards, and draw with the maturity floor on.
    ///
    /// The third position is the one the registry grew for. D1's guard exists because a seat
    /// LEAVING is what makes a matured registry thin: the replacement is itself immature for a
    /// whole window, so for that window the chain runs one bond short. At `seat_count + 1` the
    /// departure alone is fatal — the remaining bonds cannot fill the panel once the executor is
    /// excluded. At `seat_count + 3` it is absorbed twice over. Both are asserted here, on one
    /// registry, because the margin is only visible as a difference.
    #[test]
    fn the_shipped_registry_draws_under_an_armed_maturity_fence_even_after_a_seat_leaves() {
        use crate::config::params::palw_rc_shipped_params;
        use crate::palw_mode_v2::PalwConsensusMode;

        let params = palw_rc_shipped_params();
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("the shipped RC preset carries a V2 bundle");
        };
        let sp = bundle.state.clone();

        // 1. Boot the genesis registry the way a node does — through the transition, not by
        //    reading the card. A registry that parses and does not apply is the failure this
        //    whole gate line keeps finding.
        let genesis_ctx =
            PalwBlockContextV2 { block: params.genesis.hash, daa_score: params.genesis.daa_score, blue_score: 0, subsidy: 0 };
        let (booted, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &genesis_ctx, &bundle.genesis_objects, None)
            .expect("the shipped genesis registrations apply");

        let cards: Vec<_> = bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => {
                    Some((*bond, pubkey.clone(), operator_pubkey.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            cards.len(),
            crate::palw_fp_devnet_v3::palw_v2_maturity_armable_bonds_v1(),
            "the shipped registry is the size that can carry the fence"
        );

        // 2. A claim under the FIRST shipped card, so the draw has a real executor to exclude by
        //    bond, operator and key — the three exclusions are what turn `seat_count` seats into
        //    a `seat_count + 1` requirement.
        let (exec_bond, exec_pubkey, exec_operator) = cards[0].clone();
        let class_id = sp.base_class_id();
        let artifact_root = bundle
            .genesis_objects
            .iter()
            .find_map(|o| match o {
                PalwConsensusObjectV2::ClassRegistered { class_id: c, artifact_root, .. } if *c == class_id => Some(*artifact_root),
                _ => None,
            })
            .expect("the floor class is registered at genesis");
        let env = PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h64(0xD0),
                challenge: challenge_v2(h64(0xD0), h64(5), 1_700, 1, class_id, &exec_bond.0),
                class_id,
                executor_bond: exec_bond.0,
                executor_pubkey: exec_pubkey,
                operator_id: crate::palw_state_v2::palw_operator_id_v2(&exec_operator),
                artifact_root,
                trace_root: h64(31),
                output_root: h64(32),
                pwu: 1,
                trace_manifest_root: h64(33),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
                execution_root: h64(41),
            },
            signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let claim_id = attempt_id_v2(&env.attempt);
        let bound_daa = params.genesis.daa_score + 1;
        let (live, _) = apply_palw_transition_v2(
            &booted,
            &sp,
            &PalwBlockContextV2 { block: BlockHash::from_u64_word(0xB1), daa_score: bound_daa, blue_score: 1, subsidy: 0 },
            &[],
            Some(&env),
        )
        .expect("a claim binds under a shipped bond");

        // 3. The fence in force, a full window after the genesis registrations. Every shipped card
        //    is mature, so the floor takes nothing away and the panel is the one the rule-off draw
        //    would have produced.
        let anchor = BlockHash::from_u64_word(0xA1);
        let window = 1_000;
        let matured = palw_seat_maturity_floor_v1(params.genesis.daa_score + window + 1, Some(window));
        let armed = derive_panel_v2_with_maturity(&live, &bundle.panel, &claim_id, anchor, 0, matured)
            .expect("the shipped registry seats a panel with the maturity fence armed");
        assert_eq!(armed.len(), bundle.panel.seat_count() as usize, "a full panel, not a short one");
        assert_eq!(
            armed,
            derive_panel_v2(&live, &bundle.panel, &claim_id, anchor, 0).expect("and the same panel with the rule off"),
            "on a matured registry the floor changes no seat"
        );

        // 4. **A seat leaves.** Retire one card that is not the executor's and draw again: the
        //    registry absorbs it. This is the property `seat_count + 3` buys.
        let departing = cards[1].0;
        let (after, _) = apply_palw_transition_v2(
            &live,
            &sp,
            &PalwBlockContextV2 { block: BlockHash::from_u64_word(0xB2), daa_score: bound_daa + 1, blue_score: 2, subsidy: 0 },
            &[PalwConsensusObjectV2::BondRetireRequested { bond: departing, signature: Vec::new() }],
            None,
        )
        .expect("a retire request applies");
        let seats = derive_panel_v2_with_maturity(&after, &bundle.panel, &claim_id, anchor, 0, matured)
            .expect("one seat leaving does not stop the draw");
        assert_eq!(seats.len(), bundle.panel.seat_count() as usize);
        assert!(seats.iter().all(|s| s.bond != departing), "and the departed bond is not on it");

        // 5. The counterfactual, on the same machinery: at the bare `seat_count + 1` the SAME
        //    departure empties the panel. Without this the assertion above would pass on a
        //    registry of any size and prove nothing about the margin.
        let bare = crate::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1();
        let trimmed: Vec<_> = bundle
            .genesis_objects
            .iter()
            .filter({
                let mut kept = 0usize;
                move |o| {
                    if matches!(o, PalwConsensusObjectV2::BondRegistered { .. }) {
                        kept += 1;
                        kept <= bare
                    } else {
                        true
                    }
                }
            })
            .cloned()
            .collect();
        let (small, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &genesis_ctx, &trimmed, None)
            .expect("the trimmed registry applies too");
        let (small_live, _) = apply_palw_transition_v2(
            &small,
            &sp,
            &PalwBlockContextV2 { block: BlockHash::from_u64_word(0xB1), daa_score: bound_daa, blue_score: 1, subsidy: 0 },
            &[],
            Some(&env),
        )
        .unwrap();
        derive_panel_v2_with_maturity(&small_live, &bundle.panel, &claim_id, anchor, 0, matured)
            .expect("at seat_count + 1 the panel still draws while nobody has left");
        let (small_after, _) = apply_palw_transition_v2(
            &small_live,
            &sp,
            &PalwBlockContextV2 { block: BlockHash::from_u64_word(0xB2), daa_score: bound_daa + 1, blue_score: 2, subsidy: 0 },
            &[PalwConsensusObjectV2::BondRetireRequested { bond: departing, signature: Vec::new() }],
            None,
        )
        .unwrap();
        assert!(
            matches!(
                derive_panel_v2_with_maturity(&small_after, &bundle.panel, &claim_id, anchor, 0, matured),
                Err(PalwPanelV2Error::InsufficientEligibleBonds { .. })
            ),
            "at seat_count + 1 the same departure is a halt — which is why the genesis grew"
        );
    }

    /// **The shortfall counter must agree with the draw, or the warning built on it lies.**
    ///
    /// `palw_seatable_operators_v1` exists to let an operator be told that ADR-0065 D1 has outrun
    /// the live registry. A counter that answered differently from `derive_panel_v2_with_maturity`
    /// would produce exactly the failure this whole ADR line keeps finding — a diagnostic that
    /// reports health while the thing it describes is broken, or cries wolf while it is fine.
    ///
    /// So this asserts the AGREEMENT, not the count: across every maturity floor that matters,
    /// "the draw succeeds" and "enough operators remain once the executor's is removed" are the
    /// same predicate. Sharing `palw_bond_may_take_work_v2` and the maturity comparison is what
    /// makes that true; this is what stops it from silently stopping being true.
    #[test]
    fn the_seatable_counter_agrees_with_the_draw_it_warns_about() {
        let (state, claim_id) = populated_state();
        // A newcomer a century after the rest, so some floors admit it and some do not.
        let (late, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(3, 200, 3), &[register(7, 13, 0x27)], None).unwrap();
        let anchor = BlockHash::from_u64_word(0xA1);

        // The executor is bond 1 under operator 0x21, and bond 6 shares that operator — so the
        // draw's three exclusions remove ONE operator from the count, never more.
        let executor_operator = op_id(0x21);

        for (anchor_daa, window) in [(250u64, None), (250, Some(100)), (400, Some(100)), (150, Some(1_000)), (10_000, Some(1))] {
            let floor = palw_seat_maturity_floor_v1(anchor_daa, window);
            let counted = palw_seatable_operators_v1(&late, 0, floor);
            // What the draw has left after the executor's own operator is excluded.
            let available = counted.saturating_sub(1);
            for seats in 1u16..=6 {
                let Ok(params) = PalwPanelParamsV2::new(seats, 1, 4) else { continue };
                let drew = derive_panel_v2_with_maturity(&late, &params, &claim_id, anchor, 0, floor);
                assert_eq!(
                    drew.is_ok(),
                    available >= seats as usize,
                    "floor {floor:?}, {seats} seats: the counter says {counted} operators ({available} after the \
                     executor) and the draw says {drew:?} — these must never disagree"
                );
                if let Ok(panel) = drew {
                    assert_eq!(panel.len(), seats as usize);
                    assert!(panel.iter().all(|s| s.operator_id != executor_operator), "the executor's operator never sits");
                }
            }
        }
    }

    /// **The rate limiter, including the two arms that were wrong before it was a function.**
    ///
    /// It gates a WARNING, so being wrong is not a consensus fault — it is the difference between
    /// an operator hearing why the chain stopped and not hearing it.
    #[test]
    fn the_shortfall_limiter_reports_at_the_start_and_survives_a_reorg() {
        let interval = 600;
        // **An escalation always speaks**, whatever the interval says. The band only ever worsens
        // by the registry losing another operator, and the interval is a bind window — roughly 20
        // hours at the live cadence — so without this the step from "the margin is gone" to "no
        // claim can bind" would be invisible for most of a day.
        assert!(palw_shortfall_report_is_due_v1(1_000, 1_001, interval, true));
        assert!(palw_shortfall_report_is_due_v1(1_000, 1_000, u64::MAX, true));

        // Cold start reports immediately. The saturating add makes `last + interval` equal
        // `u64::MAX`, so without the sentinel arm no first report would ever be due.
        assert!(palw_shortfall_report_is_due_v1(PALW_SHORTFALL_NEVER_REPORTED, 0, interval, false));
        assert!(palw_shortfall_report_is_due_v1(PALW_SHORTFALL_NEVER_REPORTED, u64::MAX, interval, false));

        // Inside the window, going forward: quiet.
        assert!(!palw_shortfall_report_is_due_v1(1_000, 1_000, interval, false));
        assert!(!palw_shortfall_report_is_due_v1(1_000, 1_599, interval, false));
        // At the boundary and past it: due. Exactly at `last + interval` counts, or the effective
        // interval would silently be one longer than it says.
        assert!(palw_shortfall_report_is_due_v1(1_000, 1_600, interval, false));
        assert!(palw_shortfall_report_is_due_v1(1_000, 9_999, interval, false));

        // **A reorg to a lower score re-opens the report.** This is the arm that matters: the
        // caller runs per chain block added, so a reorg replays scores below the old tip. Without
        // it the node goes quiet from the reorg until the new branch passes `last + interval`.
        assert!(palw_shortfall_report_is_due_v1(10_000, 9_000, interval, false));
        assert!(palw_shortfall_report_is_due_v1(10_000, 9_999, interval, false));
        assert!(!palw_shortfall_report_is_due_v1(10_000, 10_000, interval, false), "the same score is not a reorg");

        // An interval large enough to overflow must not wrap into "always due".
        assert!(!palw_shortfall_report_is_due_v1(1_000, 1_001, u64::MAX, false));
        assert!(palw_shortfall_report_is_due_v1(1_000, 999, u64::MAX, false), "…and a reorg still speaks");
    }

    /// **The count is the REGISTRY's, and a draw's requirement is claim-relative — so the warning
    /// built on it must not speak in absolutes.**
    ///
    /// `palw_seatable_operators_v1` counts operators with at least one eligible, mature bond. The
    /// draw then excludes the claim's executor by bond, operator and key. Normally the executor's
    /// operator IS one of the counted ones, so a draw has `counted - 1` to choose from and needs
    /// `seat_count + 1` counted overall.
    ///
    /// But an executor's bond only has to EXIST for the draw to run, not to be eligible — a
    /// producer whose bond retires or is slashed under the floor while its claim is still
    /// Provisional is exactly that case. Its operator is then absent from the count, the draw has
    /// all `counted` to choose from, and `counted == seat_count` is enough.
    ///
    /// Measured here rather than argued: at three eligible operators a three-seat panel DRAWS,
    /// while the `seat_count + 1` threshold the warning alarms on says four are needed. The
    /// threshold is still the right alarm — it is the point where claims from healthy bonds start
    /// failing — but the message may not say "no panel can be drawn", because this is a panel
    /// being drawn. That is why it says "every claim from a still-eligible bond".
    #[test]
    fn a_claim_whose_own_operator_has_left_can_still_seat_a_panel() {
        let (state, claim_id) = populated_state();
        // Retire BOTH bonds under the executor's operator (0x21 holds bonds 1 and 6), so the
        // executor's bond is present-but-ineligible and its operator has nothing seatable.
        let (retired, _) = apply_palw_transition_v2(
            &state,
            &state_params(),
            &ctx(3, 200, 3),
            &[
                PalwConsensusObjectV2::BondRetireRequested { bond: PalwBondKeyV2(bond_outpoint(1)), signature: Vec::new() },
                PalwConsensusObjectV2::BondRetireRequested { bond: PalwBondKeyV2(bond_outpoint(6)), signature: Vec::new() },
            ],
            None,
        )
        .unwrap();
        let counted = palw_seatable_operators_v1(&retired, 0, None);
        assert_eq!(counted, 3, "operators 0x22, 0x23, 0x24 remain; the executor's 0x21 does not");

        let anchor = BlockHash::from_u64_word(0xA1);
        let params = PalwPanelParamsV2::new(3, 2, 4).unwrap();
        let drew = derive_panel_v2_with_maturity(&retired, &params, &claim_id, anchor, 0, None);
        assert!(drew.is_ok(), "three counted operators fill three seats when none of them is the executor's");
        assert_eq!(counted, params.seat_count() as usize, "…at exactly seat_count, one BELOW the warning's threshold");
        assert!(
            counted < params.seat_count() as usize + 1,
            "so the alarm fires here, and its wording must be true anyway — see the warning in \
             `palw_warn_if_maturity_outruns_the_registry`"
        );

        // **And the band BELOW it is unconditional**, which is what lets the warning say "NO claim
        // can seat a panel" there without qualification: at fewer than `seat_count` eligible
        // operators the draw fails even in this most favourable case, where the executor costs
        // nothing because it is not counted.
        let bigger = PalwPanelParamsV2::new(4, 3, 4).unwrap();
        assert!(counted < bigger.seat_count() as usize, "three counted against four seats");
        assert!(
            matches!(
                derive_panel_v2_with_maturity(&retired, &bigger, &claim_id, anchor, 0, None),
                Err(PalwPanelV2Error::InsufficientEligibleBonds { .. })
            ),
            "below seat_count nothing can seat a panel, whoever the executor is — the three bands are \
             seatable < seat_count (never), == seat_count (only an unseatable executor), > seat_count (always)"
        );
    }

    /// The two filters the counter applies, each shown to bite on its own.
    #[test]
    fn the_seatable_counter_dedups_operators_and_respects_maturity() {
        let (state, _) = populated_state();
        // Six bonds over FOUR operators: 0x21 twice (bonds 1 and 6) and 0x24 twice (bonds 4, 5).
        assert_eq!(state.bonds_iter().count(), 6, "six bonds…");
        assert_eq!(palw_seatable_operators_v1(&state, 0, None), 4, "…and four operators, because the draw seats one each");

        // A newcomer under a FRESH operator raises the count once it has matured, and not before.
        let (late, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(3, 200, 3), &[register(7, 13, 0x27)], None).unwrap();
        assert_eq!(palw_seatable_operators_v1(&late, 0, None), 5, "the rule off: it counts immediately");
        assert_eq!(
            palw_seatable_operators_v1(&late, 0, palw_seat_maturity_floor_v1(250, Some(100))),
            4,
            "the rule on and the window unelapsed: the newcomer is not seatable yet"
        );
        assert_eq!(
            palw_seatable_operators_v1(&late, 0, palw_seat_maturity_floor_v1(400, Some(100))),
            5,
            "…and once it has stood, it is"
        );

        // The collateral floor bites too: raise it above what every bond holds and nobody is
        // seatable, which is the slashed-to-nothing case the shared predicate exists for.
        assert_eq!(palw_seatable_operators_v1(&late, u64::MAX, None), 0, "a floor nobody meets leaves no seats");
    }

    #[test]
    fn the_draw_is_a_function_of_the_anchor_and_fails_closed_when_thin() {
        let (state, claim_id) = populated_state();
        let a = derive_panel_v2(&state, &panel_params(), &claim_id, BlockHash::from_u64_word(0xA1), 0).unwrap();
        let b = derive_panel_v2(&state, &panel_params(), &claim_id, BlockHash::from_u64_word(0xA1), 0).unwrap();
        assert_eq!(a, b, "same anchor, same panel — determinism");

        // Ask for more seats than the registry can seat: refused, never quietly smaller.
        // (3 of 4: the majority the exclusivity invariant now requires.)
        let wide = PalwPanelParamsV2::new(4, 3, 4).unwrap();
        assert!(matches!(
            derive_panel_v2(&state, &wide, &claim_id, BlockHash::from_u64_word(0xA1), 0),
            Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 4, available: 3 })
        ));
    }

    #[test]
    fn panel_bound_acceptance_checks_slot_window_and_exact_seats() {
        let (state, claim_id) = populated_state();
        let p = panel_params();
        let sp = state_params();
        // Claim accepted at daa 101; anchor slot = 105; bind deadline = 111.
        let anchor_block = BlockHash::from_u64_word(0xA0C0);
        let anchor = PalwAnchorFactV2 { anchor_block, anchor_daa: 105, predecessor_daa: 104 };
        let seats = derive_panel_v2(&state, &p, &claim_id, anchor_block, 0).unwrap();

        let ok = validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 106, 3), &claim_id, &anchor, anchor_block, &seats);
        assert!(ok.is_ok(), "a conforming binding is accepted: {ok:?}");

        // Anchor before the slot.
        let early = PalwAnchorFactV2 { anchor_block, anchor_daa: 104, predecessor_daa: 103 };
        assert!(matches!(
            validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 106, 3), &claim_id, &early, anchor_block, &seats),
            Err(PalwPanelV2Error::AnchorMismatch(_))
        ));
        // Not the FIRST block at the slot.
        let not_first = PalwAnchorFactV2 { anchor_block, anchor_daa: 106, predecessor_daa: 105 };
        assert!(matches!(
            validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 107, 3), &claim_id, &not_first, anchor_block, &seats),
            Err(PalwPanelV2Error::AnchorMismatch(_))
        ));
        // Binding before the anchor exists.
        assert!(matches!(
            validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 104, 3), &claim_id, &anchor, anchor_block, &seats),
            Err(PalwPanelV2Error::BindOutsideWindow(_))
        ));
        // Binding past the deadline.
        assert!(matches!(
            validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 112, 3), &claim_id, &anchor, anchor_block, &seats),
            Err(PalwPanelV2Error::BindOutsideWindow(_))
        ));
        // A groomed panel (seats reordered) is not the derived panel.
        let mut reordered = seats.clone();
        reordered.reverse();
        assert!(matches!(
            validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 106, 3), &claim_id, &anchor, anchor_block, &reordered),
            Err(PalwPanelV2Error::PanelMismatch)
        ));
    }

    /// A claim with its panel bound at DAA 106 — the starting point for every receipt test.
    #[allow(clippy::type_complexity)]
    fn licensed_fixture() -> (PalwChainStateV2, Hash64, PalwStateParamsV2, PalwPanelParamsV2, Hash64, Vec<PalwPanelSeatV2>, u64) {
        let (state, claim_id) = populated_state();
        let p = panel_params();
        let sp = state_params();
        let anchor_block = BlockHash::from_u64_word(0xA0C0);
        let seats = derive_panel_v2(&state, &p, &claim_id, anchor_block, 0).unwrap();
        let (bound, _) = apply_palw_transition_v2(
            &state,
            &sp,
            &ctx(3, 106, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: anchor_block, seats: seats.clone() }],
            None,
        )
        .unwrap();
        (bound, claim_id, sp, p, h64(999), seats, 106)
    }

    /// The shipped panel: five seats, quorum three. `populated_state` has only three eligible
    /// operators, so the five-seat coverage test registers its own spare operators.
    fn five_seat_licensed_fixture() -> (PalwChainStateV2, Hash64, PalwStateParamsV2, PalwPanelParamsV2, Hash64, Vec<PalwPanelSeatV2>) {
        five_seat_licensed_fixture_posting(40, |_| 1_000_000)
    }

    /// [`five_seat_licensed_fixture`] for a `pwu` claim, each bond posting `collateral(bond)` — what
    /// the objective-offence ledger's lock prices are measured against. The draw is unweighted, so
    /// the seats and the full seat are the same whatever the bonds post.
    #[allow(clippy::type_complexity)]
    fn five_seat_licensed_fixture_posting(
        pwu: u64,
        collateral: impl Fn(PalwBondKeyV2) -> u64,
    ) -> (PalwChainStateV2, Hash64, PalwStateParamsV2, PalwPanelParamsV2, Hash64, Vec<PalwPanelSeatV2>) {
        let register = |bond: u64, pubkey: u8, operator: u64| {
            let mut object = register(bond, pubkey, operator);
            if let PalwConsensusObjectV2::BondRegistered { bond, collateral: posted, .. } = &mut object {
                *posted = collateral(*bond);
            }
            object
        };
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            register(1, 7, 0x21),
            register(2, 8, 0x22),
            register(3, 9, 0x23),
            register(4, 10, 0x24),
            register(7, 13, 0x25),
            register(8, 14, 0x26),
            register(9, 15, 0x27),
        ];
        let (s1, _) =
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
        let env = attempt(pwu, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (state, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        let p = PalwPanelParamsV2::new(
            crate::palw_fp_devnet_v3::PALW_V2_PANEL_SEATS,
            crate::palw_fp_devnet_v3::PALW_V2_PANEL_QUORUM,
            4,
        )
        .unwrap();
        let sp = state_params();
        let anchor_block = BlockHash::from_u64_word(0xA0C0);
        let seats = derive_panel_v2(&state, &p, &claim_id, anchor_block, 0).expect("five eligible operators seat five");
        assert_eq!(seats.len(), 5);
        let (bound, _) = apply_palw_transition_v2(
            &state,
            &sp,
            &ctx(3, 106, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: anchor_block, seats: seats.clone() }],
            None,
        )
        .unwrap();
        (bound, claim_id, sp, p, h64(999), seats)
    }

    /// Receipts: the full path from a bound panel to both quorum outcomes, with every refusal
    /// shape on the way.
    #[test]
    fn receipt_quorum_licenses_and_unavailable_quorum_defaults_the_producer() {
        let (state, claim_id) = populated_state();
        let p = panel_params();
        let sp = state_params();
        let anchor_block = BlockHash::from_u64_word(0xA0C0);
        let seats = derive_panel_v2(&state, &p, &claim_id, anchor_block, 0).unwrap();
        let (bound, _) = apply_palw_transition_v2(
            &state,
            &sp,
            &ctx(3, 106, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: anchor_block, seats: seats.clone() }],
            None,
        )
        .unwrap();

        let net = h64(999);
        // The "signature" fixture: sig = pubkey bytes; the verifier checks exactly that, plus the
        // context (the family's own) and the message (recomputed).
        // The panel bound at daa 106, so the receipt window is [106, 106 + window_receipt].
        const SIGNED_DAA: u64 = 108;
        // An `Unavailable` names the obligation it says went unmet: a chunk the attempt committed
        // to, requested inside the retention window and before the seat signed about it.
        let unavailable = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 107 };
        let sign_as = |seat: &PalwPanelSeatV2, verdict: PalwReceiptVerdictV2| PalwSeatReceiptV2 {
            claim: claim_id,
            verdict,
            seat_bond: seat.bond,
            signed_daa: SIGNED_DAA,
            signature: bound.bond(&seat.bond).unwrap().pubkey.clone(),
        };
        let verify = |key: &[u8], message: &[u8], sig: &[u8], context: &[u8]| {
            assert_eq!(context, PALW_RECEIPT_V2_MLDSA87_CONTEXT, "the family picks its context");
            assert_eq!(message.len(), 64);
            key == sig
        };
        let here = ctx(9, 110, 9);
        let check = |st: &PalwChainStateV2, receipts: &[PalwSeatReceiptV2]| {
            validate_receipt_quorum_v2(st, &p, &sp, &here, net, &claim_id, receipts, verify)
        };

        // Two Valid receipts (quorum 2) license.
        let receipts = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid), sign_as(&seats[1], PalwReceiptVerdictV2::Valid)];
        assert_eq!(check(&bound, &receipts), Ok(PalwReceiptQuorumV2::Licensed { valid: 2 }));

        // Two Unavailable receipts justify the producer default — the seats answered.
        let receipts = vec![sign_as(&seats[0], unavailable), sign_as(&seats[1], unavailable)];
        assert_eq!(check(&bound, &receipts), Ok(PalwReceiptQuorumV2::ProducerUnavailable { unavailable: 2 }));

        // **ADR-0065 D4, the same receipt set past the fence.** It is still a well-formed answer —
        // signed, seated, in-window, naming an obligation the producer had — it simply decides
        // nothing, and the true tally is still reported so an operator can see how many seats got
        // nothing. Asserted here, beside the position it replaces, because a rule tested only in
        // its new position cannot show that anything changed.
        assert!(
            matches!(
                validate_receipt_quorum_v2_with_policy(&bound, &p, &sp, &here, net, &claim_id, &receipts, verify, true, None),
                Err(PalwPanelV2Error::NoQuorum { valid: 0, unavailable: 2, needed: 2 })
            ),
            "past the fence an Unavailable quorum licenses nothing, and the count is still visible"
        );
        // And the licensing direction is untouched — D4 removes one verdict's power, not the panel's.
        let served = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid), sign_as(&seats[1], PalwReceiptVerdictV2::Valid)];
        assert_eq!(
            validate_receipt_quorum_v2_with_policy(&bound, &p, &sp, &here, net, &claim_id, &served, verify, true, None),
            Ok(PalwReceiptQuorumV2::Licensed { valid: 2 })
        );

        // **A malformed abstention must not kill a licensable set.** The obligation gate refuses
        // an `Unavailable` naming a chunk the attempt never committed to, and refuses the WHOLE
        // receipt set with it — which is right while the verdict is an accusation and wrong once
        // it is not: one seat's bad field would otherwise void a claim three seats verified.
        let bad_chunk = PalwReceiptVerdictV2::Unavailable { chunk_index: u32::MAX, requested_daa: SIGNED_DAA };
        let mixed = vec![
            sign_as(&seats[0], PalwReceiptVerdictV2::Valid),
            sign_as(&seats[1], PalwReceiptVerdictV2::Valid),
            sign_as(&seats[2], bad_chunk),
        ];
        assert!(
            matches!(check(&bound, &mixed), Err(PalwPanelV2Error::UnmetObligationNotProven { .. })),
            "with the fence off it is an accusation, and a contentless one poisons the set"
        );
        assert_eq!(
            validate_receipt_quorum_v2_with_policy(&bound, &p, &sp, &here, net, &claim_id, &mixed, verify, true, None),
            Ok(PalwReceiptQuorumV2::Licensed { valid: 2 }),
            "past the fence it accuses nobody, so it is not checked as an accusation and the claim licenses"
        );

        // A split (1 Valid, 1 Unavailable) is no quorum for either transition.
        let receipts = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid), sign_as(&seats[1], unavailable)];
        assert!(matches!(check(&bound, &receipts), Err(PalwPanelV2Error::NoQuorum { valid: 1, unavailable: 1, needed: 2 })));

        // A non-seat cannot vote.
        let outsider = PalwSeatReceiptV2 {
            claim: claim_id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: PalwBondKeyV2(bond_outpoint(1)), // the executor, who is precisely not a seat
            signed_daa: SIGNED_DAA,
            signature: vec![7; 4],
        };
        assert!(matches!(check(&bound, &[outsider]), Err(PalwPanelV2Error::NotASeat(_))));

        // One seat cannot vote twice.
        let receipts = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid), sign_as(&seats[0], PalwReceiptVerdictV2::Valid)];
        assert!(matches!(check(&bound, &receipts), Err(PalwPanelV2Error::DuplicateSeat(_))));

        // A garbage signature refuses the set.
        let mut forged = sign_as(&seats[0], PalwReceiptVerdictV2::Valid);
        forged.signature = vec![0xFF; 4];
        assert!(matches!(check(&bound, &[forged]), Err(PalwPanelV2Error::ReceiptSignatureInvalid)));

        // Before a panel is bound, no quorum can form (wrong phase).
        let receipts = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid)];
        assert!(matches!(check(&state, &receipts), Err(PalwPanelV2Error::WrongPhase { .. })));
    }

    /// **Audit C5: an `Unavailable` must name an obligation the producer actually had, inside a
    /// window it could have discharged.**
    ///
    /// A quorum of `Unavailable` voids an honest producer's claim. It used to be a bare tag: no
    /// request, no chunk, no time — an accusation with no content, mintable whenever it suited
    /// the accuser. Nothing on-chain can prove a byte went unsent, and this does not pretend to.
    /// What it removes is the CONTENTLESS accusation: the receipt must name a chunk the attempt
    /// committed to, a request made after the panel existed and before the seat signed about it,
    /// and one inside the retention window the producer actually owed.
    #[test]
    fn adr0133_v2_a_licence_by_coverage_is_v1_for_full_masks_and_needs_two_attestations_a_segment() {
        use crate::palw_verification_v2::{PalwSegmentMaskV2, palw_segment_assignment_v2};
        let (state, claim_id, sp, p, net, seats) = five_seat_licensed_fixture();
        const SIGNED_DAA: u64 = 108;
        let verify = |key: &[u8], _m: &[u8], sig: &[u8], _c: &[u8]| key == sig;
        let here = ctx(9, 130, 9);
        let k = (seats.len() as u16).saturating_sub(1).max(1);
        let sign = |seat: &PalwPanelSeatV2, verdict: PalwReceiptVerdictV2, segments: PalwSegmentMaskV2| PalwSeatReceiptV3 {
            receipt: PalwSeatReceiptV2 {
                claim: claim_id,
                verdict,
                seat_bond: seat.bond,
                signed_daa: SIGNED_DAA,
                signature: state.bond(&seat.bond).unwrap().pubkey.clone(),
            },
            segments,
        };
        let check =
            |r: Vec<PalwSeatReceiptV3>| validate_receipt_coverage_v2(&state, &p, &sp, &here, net, &claim_id, &r, verify, false, None);
        let quorum = p.quorum() as usize;
        assert!(quorum >= 2 && seats.len() > quorum, "the fixture's panel has room for a missing seat");
        let panel = state.panel(&claim_id).unwrap();
        let a = palw_segment_assignment_v2(panel.anchor, claim_id, seats.len() as u16);

        // A full mask on a partial seat is not that seat's assignment.
        let stolen_full = sign(&seats[(a.full_seat as usize + 1) % seats.len()], PalwReceiptVerdictV2::Valid, PalwSegmentMaskV2::full(k));
        assert!(matches!(check(vec![stolen_full]), Err(PalwPanelV2Error::MaskNotAssigned { .. })));

        // The assigned masks of every seat (full + four disjoint partials) license: quorum of 5 ≥ 3
        // and every segment has two attestations (full seat + unique partial).
        let by_duty: Vec<_> =
            seats.iter().enumerate().map(|(i, s)| sign(s, PalwReceiptVerdictV2::Valid, a.mask_of(i as u16))).collect();
        assert!(matches!(check(by_duty.clone()), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })), "{a:?}");
        // Without the full seat each segment has one partial holder — coverage short, even with 4 Valids.
        let partial_only: Vec<_> =
            by_duty.iter().enumerate().filter(|(i, _)| *i as u16 != a.full_seat).map(|(_, r)| r.clone()).collect();
        assert!(matches!(check(partial_only), Err(PalwPanelV2Error::CoverageShort { have: 1, need: 2, .. })), "{a:?}");
        // Full seat plus two partials: quorum of 3, but two segments have only the full seat.
        let mut few: Vec<_> = vec![by_duty[a.full_seat as usize].clone()];
        few.extend(by_duty.iter().enumerate().filter(|(i, _)| *i as u16 != a.full_seat).map(|(_, r)| r.clone()).take(2));
        assert_eq!(few.len(), quorum);
        assert!(matches!(check(few), Err(PalwPanelV2Error::CoverageShort { have: 1, need: 2, .. })), "{a:?}");
        // A partial that attests a neighbour's segment is refused.
        let neighbour = (a.full_seat as usize + 1) % seats.len();
        let wrong = sign(&seats[neighbour], PalwReceiptVerdictV2::Valid, PalwSegmentMaskV2::single((a.mask_of(neighbour as u16).0.trailing_zeros() as u16 + 1) % k));
        assert!(matches!(check(vec![wrong]), Err(PalwPanelV2Error::MaskNotAssigned { .. })));

        // A widened mask is refused with the signature: the mask is signed.
        // The mask is signed: widening a receipt from one segment to all changes what the seat signed.
        let m_narrow = palw_receipt_message_v3(net, claim_id, PalwReceiptVerdictV2::Valid, SIGNED_DAA, PalwSegmentMaskV2::single(0));
        let m_wide = palw_receipt_message_v3(net, claim_id, PalwReceiptVerdictV2::Valid, SIGNED_DAA, PalwSegmentMaskV2::full(k));
        assert_ne!(m_narrow, m_wide, "the signed message carries the mask");
        assert_ne!(
            m_wide,
            palw_receipt_message_v2(net, claim_id, PalwReceiptVerdictV2::Valid, SIGNED_DAA),
            "…and is not the V2 message"
        );
        // An outsider and a duplicate are refused as in V1.
        let outsider = PalwSeatReceiptV3 {
            receipt: PalwSeatReceiptV2 {
                claim: claim_id,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: PalwBondKeyV2(bond_outpoint(1)),
                signed_daa: SIGNED_DAA,
                signature: vec![7; 4],
            },
            segments: PalwSegmentMaskV2::full(k),
        };
        assert!(matches!(check(vec![outsider]), Err(PalwPanelV2Error::NotASeat(_))));
        assert!(matches!(check(vec![by_duty[0].clone(), by_duty[0].clone()]), Err(PalwPanelV2Error::DuplicateSeat(_))));
    }

    /// The five-seat fixture with each seat's duty receipt (`Valid`, its assigned mask), the full
    /// seat's among them, and the coverage check bound as the processor binds it.
    struct LicenceSelectionFixture {
        state: PalwChainStateV2,
        claim_id: Hash64,
        sp: PalwStateParamsV2,
        p: PalwPanelParamsV2,
        net: Hash64,
        seats: Vec<PalwPanelSeatV2>,
        bonds: Vec<PalwBondKeyV2>,
        anchor: Hash64,
        full_seat: usize,
        by_duty: Vec<PalwSeatReceiptV3>,
        /// The objective-offence ledger's height in the fold's extras: `None` prices no lock.
        objective_offence_daa: Option<u64>,
    }

    impl LicenceSelectionFixture {
        const SIGNED_DAA: u64 = 108;

        fn new() -> Self {
            Self::from_fixture(five_seat_licensed_fixture(), None)
        }

        /// A `pwu` claim whose bonds post `collateral(bond)`, folded with the lock ledger armed from
        /// genesis — testnet-12's `palw_objective_offence` — so each `Valid` must be backed at its
        /// door's price.
        fn posting(pwu: u64, collateral: impl Fn(PalwBondKeyV2) -> u64) -> Self {
            Self::from_fixture(five_seat_licensed_fixture_posting(pwu, collateral), Some(0))
        }

        #[allow(clippy::type_complexity)]
        fn from_fixture(
            fixture: (PalwChainStateV2, Hash64, PalwStateParamsV2, PalwPanelParamsV2, Hash64, Vec<PalwPanelSeatV2>),
            objective_offence_daa: Option<u64>,
        ) -> Self {
            let (state, claim_id, sp, p, net, seats) = fixture;
            let panel = state.panel(&claim_id).unwrap();
            let bonds: Vec<PalwBondKeyV2> = panel.seats.iter().map(|seat| seat.bond).collect();
            let anchor = panel.anchor;
            let a = crate::palw_verification_v2::palw_segment_assignment_v2(anchor, claim_id, seats.len() as u16);
            let by_duty = seats
                .iter()
                .enumerate()
                .map(|(i, seat)| PalwSeatReceiptV3 {
                    receipt: PalwSeatReceiptV2 {
                        claim: claim_id,
                        verdict: PalwReceiptVerdictV2::Valid,
                        seat_bond: seat.bond,
                        signed_daa: Self::SIGNED_DAA,
                        signature: state.bond(&seat.bond).unwrap().pubkey.clone(),
                    },
                    segments: a.mask_of(i as u16),
                })
                .collect();
            Self { state, claim_id, sp, p, net, seats, bonds, anchor, full_seat: a.full_seat as usize, by_duty, objective_offence_daa }
        }

        fn check(&self, receipts: &[PalwSeatReceiptV3]) -> PalwCoverageVerdictV2 {
            let verify = |key: &[u8], _m: &[u8], sig: &[u8], _c: &[u8]| key == sig;
            validate_receipt_coverage_v2(
                &self.state,
                &self.p,
                &self.sp,
                &ctx(9, 130, 9),
                self.net,
                &self.claim_id,
                receipts,
                verify,
                false,
                None,
            )
        }

        /// The optimistic door as the acceptance arm asks it.
        fn door(&self, receipts: &[PalwSeatReceiptV3]) -> bool {
            crate::palw_optimistic_licence_v2::palw_optimistic_licence_admits_v2(
                self.anchor,
                self.claim_id,
                &self.bonds,
                receipts,
                &self.check(receipts),
            )
        }

        /// The fold, asked as the processor asks it before it offers a set.
        fn folds(&self, object: PalwConsensusObjectV2, audit: bool) -> bool {
            let extras = crate::palw_state_v2::PalwTransitionExtrasV1 {
                verification_v2_active: true,
                verification_s2_active: true,
                audit_2026_09_23_active: audit,
                objective_offence_daa: self.objective_offence_daa,
                ..Default::default()
            };
            crate::palw_state_v2::palw_v2_object_licenses_claim_v1(
                &self.state,
                &self.sp,
                &ctx(9, 130, 9),
                &object,
                false,
                false,
                false,
                false,
                &extras,
            )
        }

        fn optimistic<F: Fn(&[PalwSeatReceiptV3]) -> bool>(
            &self,
            pool: &[PalwSeatReceiptV3],
            licenses: F,
        ) -> Option<Vec<PalwSeatReceiptV3>> {
            palw_select_optimistic_licence_v2(
                &self.state,
                &self.claim_id,
                None,
                pool,
                |r: &[PalwSeatReceiptV3]| self.check(r),
                licenses,
            )
        }

        fn coverage(&self, pool: &[PalwSeatReceiptV3]) -> Option<Vec<PalwSeatReceiptV3>> {
            self.coverage_with(pool, |_| true)
        }

        fn coverage_with<F: Fn(&[PalwSeatReceiptV3]) -> bool>(
            &self,
            pool: &[PalwSeatReceiptV3],
            licenses: F,
        ) -> Option<Vec<PalwSeatReceiptV3>> {
            palw_select_coverage_licence_v2(pool, |r: &[PalwSeatReceiptV3]| self.check(r), licenses)
        }

        /// `licenses` as the processor binds it for the optimistic door: the fold, asked of the object.
        fn folds_optimistic(&self, audit: bool) -> impl Fn(&[PalwSeatReceiptV3]) -> bool + '_ {
            move |receipts: &[PalwSeatReceiptV3]| {
                self.folds(PalwConsensusObjectV2::OptimisticLicensed { claim: self.claim_id, receipts: receipts.to_vec() }, audit)
            }
        }

        /// …and for the coverage door.
        fn folds_coverage(&self, audit: bool) -> impl Fn(&[PalwSeatReceiptV3]) -> bool + '_ {
            move |receipts: &[PalwSeatReceiptV3]| {
                self.folds(PalwConsensusObjectV2::ReceiptLicensedV2 { claim: self.claim_id, receipts: receipts.to_vec() }, audit)
            }
        }

        fn valid_count(receipts: &[PalwSeatReceiptV3]) -> usize {
            receipts.iter().filter(|r| matches!(r.receipt.verdict, PalwReceiptVerdictV2::Valid)).count()
        }
    }

    fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
        if items.len() <= 1 {
            return vec![items.to_vec()];
        }
        let mut out = Vec::new();
        for (i, first) in items.iter().enumerate() {
            let mut rest = items.to_vec();
            rest.remove(i);
            for mut tail in permutations(&rest) {
                tail.insert(0, *first);
                out.push(tail);
            }
        }
        out
    }

    /// **The licence stall, reproduced and now closed** (2026-09-24). The audit's verifier replayed
    /// the processor's two greedy loops against this validator, for the pool order a stuck
    /// testnet-12 floor claim had, and found the optimistic licence built only when the full-replay
    /// seat's `Valid` was among the first two in the pool and the coverage licence never. The loops
    /// are gone; this runs the selection that replaced them (`palw_select_*_licence_v2`, which both
    /// processor assemblers call) with the full seat at every pool position, and keeps the acceptance
    /// facts the fix rests on.
    #[test]
    fn verifier_greedy_assembler_drops_a_late_full_seat() {
        let f = LicenceSelectionFixture::new();
        let full = f.by_duty[f.full_seat].clone();
        let partials: Vec<_> = f.by_duty.iter().enumerate().filter(|(i, _)| *i != f.full_seat).map(|(_, r)| r.clone()).collect();

        // The acceptance facts. The optimistic arm takes `Ok` or `NoQuorum`, so {full, one partial}
        // and all five are accepted, and three or four `Valid`s are refused as `CoverageShort`.
        let with = |n: usize| {
            let mut v = vec![full.clone()];
            v.extend(partials.iter().take(n).cloned());
            v
        };
        assert!(matches!(f.check(&with(0)), Err(PalwPanelV2Error::NoQuorum { .. })));
        assert!(matches!(f.check(&with(1)), Err(PalwPanelV2Error::NoQuorum { .. })));
        assert!(matches!(f.check(&with(2)), Err(PalwPanelV2Error::CoverageShort { .. })));
        assert!(matches!(f.check(&with(3)), Err(PalwPanelV2Error::CoverageShort { .. })));
        assert!(matches!(f.check(&with(4)), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })));
        assert!(f.door(&with(0)) && f.door(&with(1)) && !f.door(&with(2)) && !f.door(&with(3)) && f.door(&with(4)));

        for pos in 0..5usize {
            // The whole panel filed, the full seat at `pos`: both doors open, each with all five.
            let mut pool = partials.clone();
            pool.insert(pos, full.clone());
            let optimistic = f.optimistic(&pool, |_| true).unwrap_or_else(|| panic!("full seat at {pos}: no optimistic licence"));
            assert!(f.door(&optimistic), "full seat at {pos}: the optimistic set is one the door takes");
            assert_eq!(optimistic.len(), 5, "full seat at {pos}: all five validated, so all five ride");
            let coverage = f.coverage(&pool).unwrap_or_else(|| panic!("full seat at {pos}: no coverage licence"));
            assert!(matches!(f.check(&coverage), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })));

            // d0815709's pool when it stalled: four receipts, the full seat's arriving late. The
            // optimistic licence forms from the full seat and one partial; coverage cannot.
            let mut stalled = partials[..3].to_vec();
            stalled.insert(pos.min(3), full.clone());
            let optimistic = f.optimistic(&stalled, |_| true).unwrap_or_else(|| panic!("full seat at {pos} of four: no licence"));
            assert_eq!(optimistic.len(), 2, "full seat at {pos} of four: the full seat and one partial");
            assert_eq!(optimistic[0], full, "the full seat's Valid is taken first");
            assert!(f.door(&optimistic));
            assert!(f.coverage(&stalled).is_none(), "four receipts cannot cover a four-segment cut twice");
        }
    }

    /// **The selection offers a licence exactly when some set of the pool is one — and only a set
    /// its door accepts and the fold licenses.** Every arrival order of every subset of the five
    /// duty receipts (325 pools), against a brute force over every subset of each pool with the
    /// acceptance predicates: the optimistic door both as the shared predicate and as the arm's own
    /// spelling (`Ok` or `NoQuorum`), and coverage's `Licensed`.
    #[test]
    fn the_licence_selection_offers_exactly_what_the_doors_accept_in_every_arrival_order() {
        let f = LicenceSelectionFixture::new();
        let quorum = f.p.quorum() as usize;
        let mut pools = 0;
        for mask in 1u32..(1 << 5) {
            let members: Vec<usize> = (0..5).filter(|i| mask & (1 << i) != 0).collect();
            let pick = |sub: u32| -> Vec<PalwSeatReceiptV3> {
                (0..5).filter(|i| sub & (1 << i) != 0).map(|i| f.by_duty[i].clone()).collect()
            };
            let subsets = || (1u32..(1 << 5)).filter(move |sub| sub & !mask == 0);
            let optimistic_exists = subsets().any(|sub| f.door(&pick(sub)));
            let coverage_exists = subsets().any(|sub| matches!(f.check(&pick(sub)), Ok(PalwReceiptQuorumV2::Licensed { .. })));
            let has_full = members.contains(&f.full_seat);
            assert_eq!(optimistic_exists, has_full, "{members:?}: the full seat's Valid is necessary and sufficient");
            assert_eq!(coverage_exists, members.len() == 5, "{members:?}: coverage needs the whole panel");

            for order in permutations(&members) {
                pools += 1;
                let pool: Vec<PalwSeatReceiptV3> = order.iter().map(|i| f.by_duty[*i].clone()).collect();
                let optimistic = f.optimistic(&pool, |_| true);
                assert_eq!(optimistic.is_some(), optimistic_exists, "{order:?}");
                if let Some(set) = &optimistic {
                    assert!(f.door(set), "{order:?}: the shared predicate");
                    assert!(
                        matches!(f.check(set), Ok(_) | Err(PalwPanelV2Error::NoQuorum { .. })),
                        "{order:?}: the arm's own spelling of the door"
                    );
                    let valid = LicenceSelectionFixture::valid_count(set);
                    assert!(valid < quorum || valid == 5, "{order:?}: never {valid} Valids short of coverage");
                    let expected = if members.len() == 5 { 5 } else { members.len().min(2) };
                    assert_eq!(set.len(), expected, "{order:?}");
                    assert_eq!(set[0], f.by_duty[f.full_seat], "{order:?}: the full seat first");
                    if set.len() == 2 {
                        let first_partial = order.iter().find(|i| **i != f.full_seat).unwrap();
                        assert_eq!(set[1], f.by_duty[*first_partial], "{order:?}: the rider is the first partial to arrive");
                    }
                    for audit in [false, true] {
                        let object = PalwConsensusObjectV2::OptimisticLicensed { claim: f.claim_id, receipts: set.clone() };
                        assert!(f.folds(object, audit), "{order:?}: the fold licenses it (audit fence {audit})");
                    }
                }
                let coverage = f.coverage(&pool);
                assert_eq!(coverage.is_some(), coverage_exists, "{order:?}");
                if let Some(set) = &coverage {
                    assert!(matches!(f.check(set), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })), "{order:?}");
                    for audit in [false, true] {
                        let object = PalwConsensusObjectV2::ReceiptLicensedV2 { claim: f.claim_id, receipts: set.clone() };
                        assert!(f.folds(object, audit), "{order:?}: the fold licenses it (audit fence {audit})");
                    }
                }
            }
        }
        assert_eq!(pools, 325, "5 + 20 + 60 + 120 + 120 arrival orders");
    }

    /// **A receipt that poisons the set, or repeats a seat, is dropped — it does not take the
    /// licence with it.** A forged copy of the full seat's `Valid` ahead of the real one, a partial
    /// signed over a neighbour's mask, and a second copy of a partial.
    #[test]
    fn the_licence_selection_drops_a_poisoned_receipt_and_a_repeated_seat() {
        let f = LicenceSelectionFixture::new();
        let full = f.by_duty[f.full_seat].clone();
        let partials: Vec<_> = f.by_duty.iter().enumerate().filter(|(i, _)| *i != f.full_seat).map(|(_, r)| r.clone()).collect();
        let mut forged = full.clone();
        forged.receipt.signature = vec![0xFF; 4];
        let mut stolen = partials[1].clone();
        stolen.segments = partials[0].segments;
        assert!(matches!(f.check(std::slice::from_ref(&forged)), Err(PalwPanelV2Error::ReceiptSignatureInvalid)));
        assert!(matches!(f.check(std::slice::from_ref(&stolen)), Err(PalwPanelV2Error::MaskNotAssigned { .. })));

        let pool = vec![forged.clone(), stolen.clone(), partials[0].clone(), partials[0].clone(), full.clone()];
        assert_eq!(f.optimistic(&pool, |_| true), Some(vec![full.clone(), partials[0].clone()]));
        assert_eq!(f.coverage(&pool), None);

        let mut whole = pool.clone();
        whole.extend(partials.iter().cloned());
        let optimistic = f.optimistic(&whole, |_| true).expect("the whole panel");
        assert_eq!(optimistic.len(), 5);
        assert!(!optimistic.contains(&forged) && !optimistic.contains(&stolen));
        let coverage = f.coverage(&whole).expect("the whole panel covers");
        assert_eq!(coverage.len(), 5);
        assert!(matches!(f.check(&coverage), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })));
    }

    /// **A rider the fold will not license does not cost the claim its licence** — a `Valid` whose
    /// seat cannot post its lock makes the whole set license nothing. The selection passes over it
    /// to the next rider, falls back to the full seat alone when there is none, and offers nothing
    /// only when the fold refuses that too.
    #[test]
    fn the_optimistic_selection_falls_back_past_a_rider_the_fold_refuses() {
        let f = LicenceSelectionFixture::new();
        let full = f.by_duty[f.full_seat].clone();
        let partials: Vec<_> = f.by_duty.iter().enumerate().filter(|(i, _)| *i != f.full_seat).map(|(_, r)| r.clone()).collect();
        let unbacked = partials[0].receipt.seat_bond;
        let fold = |set: &[PalwSeatReceiptV3]| set.iter().all(|r| r.receipt.seat_bond != unbacked);

        // Three `Valid`s (the full seat and two partials), the unbacked seat first to arrive: the
        // next partial rides instead.
        let pool = vec![partials[0].clone(), partials[1].clone(), full.clone()];
        assert_eq!(f.optimistic(&pool, fold), Some(vec![full.clone(), partials[1].clone()]));
        // The unbacked seat the only partial: the full seat alone.
        let pool = vec![partials[0].clone(), full.clone()];
        assert_eq!(f.optimistic(&pool, fold), Some(vec![full.clone()]));
        // The whole panel: all five are refused by the fold, then {full, the unbacked seat}, then
        // {full, the next partial} is taken.
        let mut whole = partials.clone();
        whole.push(full.clone());
        assert_eq!(f.optimistic(&whole, fold), Some(vec![full.clone(), partials[1].clone()]));
        // A rider the fold takes is kept.
        let pool = vec![partials[1].clone(), full.clone()];
        assert_eq!(f.optimistic(&pool, fold), Some(vec![full.clone(), partials[1].clone()]));
        // A full seat the fold refuses (it cannot post the whole gain): nothing is offered, so the
        // collector falls through to the next door.
        assert_eq!(f.optimistic(&whole, |_| false), None);
    }

    /// **With the lock ledger armed, each `Valid` must be backed at its door's price — and the
    /// selection offers exactly what the door and the real fold both take** (the review of the
    /// licence-stall fix). testnet-12 arms `palw_objective_offence` at genesis. On a 400-pwu claim
    /// the optimistic door prices the full seat at the whole gain and every other `Valid` at the
    /// quorum price, and the coverage door prices every `Valid` at the quorum price. Every arrival
    /// order of every subset of the five duty receipts, for six postings, is checked against a
    /// brute force over every subset of the pool with the door predicates and the fold, on both
    /// sides of the audit fence (such a set is inert past it and refused below it, and neither
    /// licenses).
    ///
    /// What it pins past the fence: a full seat that can bind but cannot post the whole gain gets
    /// no optimistic licence in any arrival order, so its claim licenses through coverage with all
    /// five or not at all. A rider that cannot post the quorum price is passed over for the next.
    #[test]
    fn the_licence_selection_prices_each_valid_at_its_door_with_the_lock_ledger_armed() {
        const PWU: u64 = 400;
        let generous = 1_000_000u64;
        let probe = LicenceSelectionFixture::posting(PWU, |_| generous);
        let claim = probe.state.claim(&probe.claim_id).unwrap();
        let facts = crate::palw_panel_var_v1::PalwClaimFraudFactsV1::from_claim(claim, 5);
        let pre_fence = crate::palw_panel_var_v1::PalwClaimFraudFactsV1::from_claim_pre_2026_09_23(claim, 5);
        let quorum_price = crate::palw_panel_var_v1::palw_panel_seat_required_v1(&facts)
            .max(crate::palw_panel_var_v1::palw_panel_seat_required_v1(&pre_fence));
        let full_price = crate::palw_offence_v1::palw_min_slashable_per_colluding_seat_v1(
            crate::palw_panel_var_v1::palw_max_fraud_gain_v1(&facts),
            1,
        );
        // `binds` backs the quorum price and not the whole gain; `floor`, the registry's minimum,
        // backs neither.
        let (binds, floor) = (quorum_price as u64 * 3 / 2, probe.sp.min_collateral_sompi());
        assert!(
            u128::from(floor) < quorum_price && quorum_price < u128::from(binds) && u128::from(binds) < full_price,
            "the postings straddle the prices: floor {floor}, quorum {quorum_price}, binds {binds}, whole gain {full_price}"
        );
        let full = probe.bonds[probe.full_seat];
        let partials: Vec<PalwBondKeyV2> = probe.bonds.iter().copied().filter(|b| *b != full).collect();
        let postings: Vec<(&str, Vec<(PalwBondKeyV2, u64)>)> = vec![
            ("every seat generous", vec![]),
            ("the full seat binds but cannot post the whole gain", vec![(full, binds)]),
            ("the full seat at the floor", vec![(full, floor)]),
            ("one partial at the floor", vec![(partials[0], floor)]),
            ("two partials at the floor", vec![(partials[0], floor), (partials[1], floor)]),
            ("the full seat binds, one partial at the floor", vec![(full, binds), (partials[0], floor)]),
        ];
        for (name, short) in &postings {
            let f = LicenceSelectionFixture::posting(PWU, |bond| short.iter().find(|(b, _)| *b == bond).map_or(generous, |(_, c)| *c));
            assert_eq!((f.bonds.clone(), f.full_seat), (probe.bonds.clone(), probe.full_seat), "{name}: the draw is unweighted");
            for audit in [false, true] {
                let (fold_optimistic, fold_coverage) = (f.folds_optimistic(audit), f.folds_coverage(audit));
                for mask in 1u32..(1 << 5) {
                    let members: Vec<usize> = (0..5).filter(|i| mask & (1 << i) != 0).collect();
                    let pick = |sub: u32| -> Vec<PalwSeatReceiptV3> {
                        (0..5).filter(|i| sub & (1 << i) != 0).map(|i| f.by_duty[i].clone()).collect()
                    };
                    let subsets = || (1u32..(1 << 5)).filter(move |sub| sub & !mask == 0);
                    let optimistic_exists = subsets().any(|sub| f.door(&pick(sub)) && fold_optimistic(&pick(sub)));
                    let coverage_exists = subsets().any(|sub| {
                        matches!(f.check(&pick(sub)), Ok(PalwReceiptQuorumV2::Licensed { .. })) && fold_coverage(&pick(sub))
                    });
                    for order in permutations(&members) {
                        let pool: Vec<PalwSeatReceiptV3> = order.iter().map(|i| f.by_duty[*i].clone()).collect();
                        let optimistic = f.optimistic(&pool, &fold_optimistic);
                        assert_eq!(optimistic.is_some(), optimistic_exists, "{name}, audit {audit}, {order:?}");
                        if let Some(set) = &optimistic {
                            assert!(f.door(set) && fold_optimistic(set), "{name}, audit {audit}, {order:?}: door and fold");
                        }
                        let coverage = f.coverage_with(&pool, &fold_coverage);
                        assert_eq!(coverage.is_some(), coverage_exists, "{name}, audit {audit}, {order:?}");
                        if let Some(set) = &coverage {
                            assert!(matches!(f.check(set), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })) && fold_coverage(set));
                        }
                    }
                }
            }
        }

        // The cases by name, past the fence.
        let posting = |short: Vec<(PalwBondKeyV2, u64)>| {
            LicenceSelectionFixture::posting(PWU, move |bond| short.iter().find(|(b, _)| *b == bond).map_or(generous, |(_, c)| *c))
        };
        let receipt_of =
            |f: &LicenceSelectionFixture, bond: PalwBondKeyV2| f.by_duty.iter().find(|r| r.receipt.seat_bond == bond).unwrap().clone();
        // A full seat that binds but cannot post the whole gain: no optimistic licence from any
        // pool, and the whole panel licenses through coverage.
        let f = posting(vec![(full, binds)]);
        let whole: Vec<PalwSeatReceiptV3> = f.by_duty.clone();
        assert_eq!(f.optimistic(&whole, f.folds_optimistic(true)), None);
        assert_eq!(f.optimistic(&whole[..2], f.folds_optimistic(true)), None);
        assert_eq!(f.coverage_with(&whole, f.folds_coverage(true)).map(|set| set.len()), Some(5));
        // Below the fence every door is the quorum price, which the same seat does post.
        assert_eq!(f.optimistic(&whole, f.folds_optimistic(false)).map(|set| set.len()), Some(5));
        // A partial at the floor, first to arrive: the next partial rides; alone beside the full
        // seat, the full seat licenses alone; and coverage, which needs it, does not form.
        let f = posting(vec![(partials[0], floor)]);
        let (fr, p0, p1) = (receipt_of(&f, full), receipt_of(&f, partials[0]), receipt_of(&f, partials[1]));
        assert_eq!(f.optimistic(&[p0.clone(), p1.clone(), fr.clone()], f.folds_optimistic(true)), Some(vec![fr.clone(), p1.clone()]));
        assert_eq!(f.optimistic(&[p0.clone(), fr.clone()], f.folds_optimistic(true)), Some(vec![fr.clone()]));
        assert_eq!(f.coverage_with(&f.by_duty, f.folds_coverage(true)), None);
    }

    /// **The order is the selection's one policy**: the full seat's `Valid`, the outsider's `Valid`,
    /// every other `Valid`, then everything else — each rank in arrival order.
    #[test]
    fn the_licence_candidate_order_is_full_seat_then_outsider_then_valid_then_the_rest() {
        let f = LicenceSelectionFixture::new();
        let full = f.seats[f.full_seat].bond;
        let others: Vec<PalwBondKeyV2> = f.bonds.iter().copied().filter(|b| *b != full).collect();
        let outsider = others[2];
        let receipt = |seat: PalwBondKeyV2, verdict: PalwReceiptVerdictV2| PalwSeatReceiptV3 {
            receipt: PalwSeatReceiptV2 { claim: f.claim_id, verdict, seat_bond: seat, signed_daa: 108, signature: vec![1] },
            segments: crate::palw_verification_v2::PalwSegmentMaskV2::single(0),
        };
        let unavailable = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 107 };
        let pool = vec![
            receipt(others[0], unavailable),
            receipt(others[1], PalwReceiptVerdictV2::Valid),
            receipt(full, PalwReceiptVerdictV2::Incapable),
            receipt(outsider, PalwReceiptVerdictV2::Valid),
            receipt(others[0], PalwReceiptVerdictV2::Valid),
            receipt(full, PalwReceiptVerdictV2::Valid),
        ];
        let positions = |ordered: Vec<&PalwSeatReceiptV3>| -> Vec<usize> {
            ordered.iter().map(|r| pool.iter().position(|p| std::ptr::eq(p, *r)).unwrap()).collect()
        };
        assert_eq!(positions(palw_licence_candidate_order_v2(&pool, Some(full), Some(outsider))), vec![5, 3, 1, 4, 0, 2]);
        assert_eq!(positions(palw_licence_candidate_order_v2(&pool, Some(full), None)), vec![5, 1, 3, 4, 0, 2]);
        assert_eq!(positions(palw_licence_candidate_order_v2(&pool, None, None)), vec![1, 3, 4, 5, 0, 2]);
    }

    #[test]
    fn an_unavailable_receipt_must_name_an_obligation_the_producer_had() {
        let (state, claim_id, sp, p, net, seats, bound_daa) = licensed_fixture();
        let verify = |key: &[u8], _m: &[u8], sig: &[u8], _c: &[u8]| key == sig;
        let here = ctx(9, 130, 9);
        let claim = state.claim(&claim_id).unwrap();
        let retention = claim.trace_retention_daa;
        let chunks = claim.trace_chunk_count;

        let receipt = |verdict: PalwReceiptVerdictV2, signed_daa: u64| PalwSeatReceiptV2 {
            claim: claim_id,
            verdict,
            seat_bond: seats[0].bond,
            signed_daa,
            signature: state.bond(&seats[0].bond).unwrap().pubkey.clone(),
        };
        let check = |r: Vec<PalwSeatReceiptV2>| validate_receipt_quorum_v2(&state, &p, &sp, &here, net, &claim_id, &r, verify);

        // The well-formed accusation is short of quorum here (one seat of a 3/2 panel), which is
        // the shape we want: it reaches the counting stage rather than being refused.
        let ok = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: bound_daa + 1 };
        assert!(matches!(check(vec![receipt(ok, bound_daa + 2)]), Err(PalwPanelV2Error::NoQuorum { unavailable: 1, .. })));

        // A chunk the attempt never committed to is a demand the producer never owed.
        let bad_chunk = PalwReceiptVerdictV2::Unavailable { chunk_index: chunks, requested_daa: bound_daa + 1 };
        assert!(matches!(check(vec![receipt(bad_chunk, bound_daa + 2)]), Err(PalwPanelV2Error::UnmetObligationNotProven { .. })));

        // A request that predates the panel is not about this panel's duty.
        let early = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: bound_daa - 1 };
        assert!(matches!(check(vec![receipt(early, bound_daa + 2)]), Err(PalwPanelV2Error::UnmetObligationNotProven { .. })));

        // A request the seat had not yet made when it signed about it.
        let ahead = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: bound_daa + 3 };
        assert!(matches!(check(vec![receipt(ahead, bound_daa + 2)]), Err(PalwPanelV2Error::UnmetObligationNotProven { .. })));

        // A request past the retention deadline: the obligation had ended.
        let late = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: retention + 1 };
        assert!(matches!(
            check(vec![receipt(late, retention + 2)]),
            Err(PalwPanelV2Error::UnmetObligationNotProven { .. } | PalwPanelV2Error::ReceiptOutsideWindow { .. })
        ));
    }

    /// **Audit C5: the receipt duty has a clock.**
    ///
    /// `validate_receipt_quorum_v2` took no block context at all, so nothing bounded WHEN a seat
    /// could answer — a receipt could be signed before the panel existed, long after the window
    /// closed, or dated into the future.
    #[test]
    fn a_receipt_outside_its_window_is_not_a_discharge_of_the_duty() {
        let (state, claim_id, sp, p, net, seats, bound_daa) = licensed_fixture();
        let verify = |key: &[u8], _m: &[u8], sig: &[u8], _c: &[u8]| key == sig;
        let receipt = |signed_daa: u64| PalwSeatReceiptV2 {
            claim: claim_id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: seats[0].bond,
            signed_daa,
            signature: state.bond(&seats[0].bond).unwrap().pubkey.clone(),
        };
        let at = |block_daa: u64, r: Vec<PalwSeatReceiptV2>| {
            validate_receipt_quorum_v2(&state, &p, &sp, &ctx(9, block_daa, 9), net, &claim_id, &r, verify)
        };

        // Inside the window: reaches the counting stage.
        assert!(matches!(at(bound_daa + 5, vec![receipt(bound_daa + 1)]), Err(PalwPanelV2Error::NoQuorum { valid: 1, .. })));
        // Before the panel was bound.
        assert!(matches!(at(bound_daa + 5, vec![receipt(bound_daa - 1)]), Err(PalwPanelV2Error::ReceiptOutsideWindow { .. })));
        // Past the receipt deadline.
        let past = bound_daa + sp.window_receipt() + 1;
        assert!(matches!(at(past + 5, vec![receipt(past)]), Err(PalwPanelV2Error::ReceiptOutsideWindow { .. })));
        // Dated after the block that carries it.
        assert!(matches!(at(bound_daa + 1, vec![receipt(bound_daa + 2)]), Err(PalwPanelV2Error::ReceiptOutsideWindow { .. })));
    }

    /// The verdicts sign DIFFERENT messages: an `Unavailable` signature cannot be replayed as a
    /// `Valid` one — the distinctness that lets a seat report withheld data safely.
    ///
    /// And every field an `Unavailable` carries is inside its message. Signing only the verdict
    /// TAG would have let one signature stand behind any chunk index and any request time — the
    /// whole content of the accusation swapped under a signature that stayed valid.
    #[test]
    fn the_two_verdicts_are_two_messages_and_every_field_is_signed() {
        let unavail = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 107 };
        let m_valid = palw_receipt_message_v2(h64(999), h64(1), PalwReceiptVerdictV2::Valid, 108);
        let m_unavail = palw_receipt_message_v2(h64(999), h64(1), unavail, 108);
        assert_ne!(m_valid, m_unavail);
        assert_ne!(palw_receipt_message_v2(h64(998), h64(1), PalwReceiptVerdictV2::Valid, 108), m_valid, "network binds");
        assert_ne!(palw_receipt_message_v2(h64(999), h64(2), PalwReceiptVerdictV2::Valid, 108), m_valid, "claim binds");
        assert_ne!(palw_receipt_message_v2(h64(999), h64(1), PalwReceiptVerdictV2::Valid, 109), m_valid, "the signing time binds");
        assert_ne!(
            palw_receipt_message_v2(h64(999), h64(1), PalwReceiptVerdictV2::Unavailable { chunk_index: 1, requested_daa: 107 }, 108),
            m_unavail,
            "the accused chunk binds"
        );
        assert_ne!(
            palw_receipt_message_v2(h64(999), h64(1), PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 106 }, 108),
            m_unavail,
            "the request time binds"
        );
    }

    #[test]
    fn the_v2_panel_domains_are_distinct() {
        let mut seen: Vec<&[u8]> = PALW_PANEL_V2_ALL_DOMAINS.to_vec();
        seen.sort();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    /// **How many operators a class needs to be alive — as arithmetic, not as folklore.**
    ///
    /// The number governs every deployment decision (how many hosts, how much hardware for a
    /// family whose seats must hold particular hardware) and it had been stated wrongly twice in
    /// one session: once as "5 seats, 5 quorum" and once as "the RC declares no per-class floor".
    /// Both are measured here instead.
    ///
    /// The rule that makes it a hard count rather than a target: `derive_panel_v2` **refuses** a
    /// short draw. It does not seat four of five and carry on — `quorum` is how many must AGREE,
    /// never how many must EXIST. So a class needs its full seat count of eligible operators for
    /// any claim to bind at all, and one operator short is not degraded service, it is a chain
    /// where every claim voids at `BindTimeout` and every block's worker carve burns.
    ///
    /// Eligibility excludes the executor three ways (bond, operator, key) and seats one bond per
    /// operator, so the count is `seat_count + 1` DISTINCT OPERATORS — five extra bonds under one
    /// operator buy exactly one seat.
    #[test]
    fn a_class_needs_seat_count_plus_one_distinct_operators() {
        // Five operators total: one executor + four others, against a 5-seat panel.
        let mut objects = vec![PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 1,
            pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        }];
        objects.push(register(1, 7, 0x21)); // the executor
        for k in 0..4u64 {
            objects.push(register(10 + k, 20 + k as u8, 0x30 + k));
        }
        // Plus three MORE bonds under an operator that already has one: extra collateral, zero
        // extra seats. This is the Sybil bound Decision 7 rests on, and it is why the answer is
        // counted in operators.
        for k in 0..3u64 {
            objects.push(register(50 + k, 50 + k as u8, 0x30));
        }
        let (s1, _) =
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (state, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        let anchor = BlockHash::from_u64_word(0xA1);

        // Four eligible OPERATORS (eight eligible bonds) against five seats: refused, and the
        // error reports the operator count rather than the bond count.
        let five = PalwPanelParamsV2::new(5, 3, 4).unwrap();
        match derive_panel_v2(&state, &five, &claim_id, anchor, 0) {
            Err(PalwPanelV2Error::InsufficientEligibleBonds { needed, available }) => {
                assert_eq!(needed, 5);
                assert_eq!(available, 4, "eight bonds under four operators seat four — one per operator");
            }
            other => panic!("a short panel must be refused, not seated short: {other:?}"),
        }

        // The floor `(2, 2)` a class may thin to needs three distinct operators: executor + two.
        let two = PalwPanelParamsV2::new(2, 2, 4).unwrap();
        let seats = derive_panel_v2(&state, &two, &claim_id, anchor, 0).expect("two seats from four eligible operators");
        assert_eq!(seats.len(), 2);
        let mut ops: Vec<Hash64> = seats.iter().map(|s| s.operator_id).collect();
        ops.sort();
        ops.dedup();
        assert_eq!(ops.len(), 2, "one seat per operator");
        assert!(!ops.contains(&op_id(0x21)), "the executor's operator is never seated");
    }

    // ---- ADR-0100 Decision 4: the stratified draw and a shard's part, from chain state ----

    fn shard_extras() -> crate::palw_state_v2::PalwTransitionExtrasV1 {
        crate::palw_state_v2::PalwTransitionExtrasV1 {
            shard_licensing: Some(crate::palw_shard_licensing_v1::PalwShardLicensingParamsV1 {
                seats_per_shard: 3,
                quorum_per_shard: 2,
            }),
            ..Default::default()
        }
    }

    fn shard_fold(
        parent: &PalwChainStateV2,
        c: &PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        att: Option<&PalwAttemptEnvelopeV2>,
    ) -> PalwChainStateV2 {
        crate::palw_state_v2::apply_palw_transition_v2_with_extras(
            parent,
            &state_params(),
            c,
            objects,
            att,
            false,
            false,
            false,
            false,
            &shard_extras(),
        )
        .expect("the fixture folds")
        .0
    }

    /// Class `h64(1)` registered by bond 9, a two-shard plan, and bonds 2..=8 (distinct operators)
    /// holding shards: 2, 3, 4 → shard 0; 5, 6, 7 → shard 1; 8 → both. `drop` bonds declare none.
    fn sharded_registry(drop: &[u64]) -> (PalwChainStateV2, Hash64) {
        let profile =
            crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).expect("projects");
        let mut objects: Vec<PalwConsensusObjectV2> = (1..=9).map(|v| register(v, v as u8 + 6, 0x20 + v)).collect();
        objects.push(PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: Some(Box::new(crate::palw_state_v2::PalwClassAdmissionCarriageV2 {
                registrant_bond: PalwBondKeyV2(bond_outpoint(9)),
                canonical: crate::palw_base0_profile::rc_job_context(&profile, 2, 2),
                profile,
                signature: Vec::new(),
            })),
        });
        let s1 = shard_fold(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &objects, None);
        let mut declarations =
            vec![PalwConsensusObjectV2::ClassShardPlanDeclared { class_id: h64(1), shard_count: 2, signature: vec![1; 4] }];
        for (bond, shards) in
            [(2u64, vec![0u32]), (3, vec![0]), (4, vec![0]), (5, vec![1]), (6, vec![1]), (7, vec![1]), (8, vec![0, 1])]
        {
            if drop.contains(&bond) {
                continue;
            }
            declarations.push(PalwConsensusObjectV2::BondShardsDeclared {
                bond: PalwBondKeyV2(bond_outpoint(bond)),
                class_id: h64(1),
                shard_count: 2,
                shards,
                signature: vec![1; 4],
            });
        }
        let s2 = shard_fold(&s1, &ctx(2, 101, 2), &declarations, None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let s3 = shard_fold(&s2, &ctx(3, 102, 3), &[], Some(&env));
        (s3, claim_id)
    }

    /// **The stratified draw seats each shard from the bonds that hold it**, one operator a shard,
    /// stored shard-major; the binding validator recomputes it only when told the claim is
    /// sharded, and a shard nobody can fill refuses the whole draw by name.
    #[test]
    fn a_stratified_draw_seats_each_shard_from_the_bonds_that_hold_it() {
        let (state, claim_id) = sharded_registry(&[]);
        let anchor_block = BlockHash::from_u64_word(0xA1);
        let seats =
            derive_stratified_panel_v2(&state, &panel_params(), &claim_id, anchor_block, 0, None, false, 2).expect("both shards fill");
        assert_eq!(seats.len(), 6, "two shards × three seats");
        let holds = |bond: &PalwBondKeyV2, shard: u32| state.shards_of_bond(bond, &h64(1)).is_some_and(|list| list.contains(&shard));
        assert!(seats[..3].iter().all(|seat| holds(&seat.bond, 0)), "shard 0's seats hold shard 0");
        assert!(seats[3..].iter().all(|seat| holds(&seat.bond, 1)), "shard 1's seats hold shard 1");
        assert!(seats.iter().all(|seat| seat.bond != PalwBondKeyV2(bond_outpoint(1))), "the executor is never seated");
        assert_eq!(seats, derive_stratified_panel_v2(&state, &panel_params(), &claim_id, anchor_block, 0, None, false, 2).unwrap());

        let anchor = PalwAnchorFactV2 { anchor_block, anchor_daa: 106, predecessor_daa: 105 };
        let sp = state_params();
        validate_panel_bound_v2_with_shards(
            &state,
            &panel_params(),
            &sp,
            &ctx(4, 107, 4),
            &claim_id,
            &anchor,
            anchor_block,
            &seats,
            None,
            false,
            false,
            Some(2),
        )
        .expect("the stratified binding validates as stratified");
        assert_eq!(
            validate_panel_bound_v2_with_shards(
                &state,
                &panel_params(),
                &sp,
                &ctx(4, 107, 4),
                &claim_id,
                &anchor,
                anchor_block,
                &seats,
                None,
                false,
                false,
                None
            ),
            Err(PalwPanelV2Error::PanelMismatch),
            "and not as flat"
        );

        let (short, short_claim) = sharded_registry(&[3, 4]);
        assert_eq!(
            derive_stratified_panel_v2(&short, &panel_params(), &short_claim, anchor_block, 0, None, false, 2),
            Err(PalwPanelV2Error::InsufficientEligibleShardBonds { shard: 0, needed: 3, available: 2 })
        );
    }

    /// **A part is validated over its own shard's seats**, and the whole-object door refuses a
    /// claim drawn per shard.
    #[test]
    fn a_shard_part_is_validated_over_its_shards_seats_and_the_whole_door_is_shut() {
        let (s3, claim_id) = sharded_registry(&[]);
        let anchor_block = BlockHash::from_u64_word(0xA1);
        let seats = derive_stratified_panel_v2(&s3, &panel_params(), &claim_id, anchor_block, 0, None, false, 2).unwrap();
        let s4 = shard_fold(
            &s3,
            &ctx(4, 107, 4),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: anchor_block, seats: seats.clone() }],
            None,
        );
        let receipt = |seat: &PalwPanelSeatV2| PalwSeatReceiptV2 {
            claim: claim_id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: seat.bond,
            signed_daa: 108,
            signature: vec![1; 8],
        };
        let part = |shard: u32, receipts: Vec<PalwSeatReceiptV2>| crate::palw_shard_licensing_v1::PalwShardReceiptPartV1 {
            claim: claim_id,
            shard_count: 2,
            shard_index: shard,
            receipts,
        };
        let yes = |_: &[u8], _: &[u8], _: &[u8], _: &[u8]| true;
        let no = |_: &[u8], _: &[u8], _: &[u8], _: &[u8]| false;
        let (pp, sp, at) = (panel_params(), state_params(), ctx(5, 109, 5));
        let shard0 = part(0, vec![receipt(&seats[0]), receipt(&seats[1])]);
        assert_eq!(
            validate_shard_receipt_part_v1(&s4, &pp, &sp, &at, h64(999), &shard0, yes, false),
            Ok(PalwReceiptQuorumV2::Licensed { valid: 2 })
        );
        assert_eq!(
            validate_shard_receipt_part_v1(&s4, &pp, &sp, &at, h64(999), &shard0, no, false),
            Err(PalwPanelV2Error::ReceiptSignatureInvalid)
        );
        let stray = part(0, vec![receipt(&seats[0]), receipt(&seats[3])]);
        assert_eq!(
            validate_shard_receipt_part_v1(&s4, &pp, &sp, &at, h64(999), &stray, yes, false),
            Err(PalwPanelV2Error::NotASeat(seats[3].bond))
        );
        let foreign = crate::palw_shard_licensing_v1::PalwShardReceiptPartV1 { shard_count: 3, ..shard0.clone() };
        assert_eq!(
            validate_shard_receipt_part_v1(&s4, &pp, &sp, &at, h64(999), &foreign, yes, false),
            Err(PalwPanelV2Error::ShardPlanMismatch { declared: 2, part: 3 })
        );
        let receipts: Vec<PalwSeatReceiptV2> = seats[..3].iter().map(receipt).collect();
        assert_eq!(
            validate_receipt_quorum_v2_with_policy(&s4, &pp, &sp, &at, h64(999), &claim_id, &receipts, yes, false, None),
            Err(PalwPanelV2Error::LicensedByParts(claim_id)),
            "a claim drawn per shard has no whole-object licence"
        );
        let s5 = shard_fold(&s4, &ctx(5, 109, 5), &[PalwConsensusObjectV2::ShardReceiptLicensed { part: shard0.clone() }], None);
        assert_eq!(
            validate_shard_receipt_part_v1(&s5, &pp, &sp, &ctx(6, 110, 6), h64(999), &shard0, yes, false),
            Err(PalwPanelV2Error::ShardAlreadyLicensed { shard: 0 })
        );
    }

    /// **The receipt pool's read is the tip's bound panel and the registry's keys, and nothing
    /// else** (node policy; the 2026-09-24 launch review's receipt-pool flush). A `PanelBound` claim
    /// answers with its panel as the state holds it; a claim in any other phase, or none at all,
    /// answers nothing — so the pool can never be told a panel the chain does not hold; a bond the
    /// registry holds answers with its registered key, and an invented one with nothing.
    #[test]
    fn the_receipt_pool_reads_only_bound_panels_and_registered_keys() {
        let (state, claim_id, _sp, _p, _net, seats) = five_seat_licensed_fixture();
        let panel = state.panel(&claim_id).unwrap().clone();
        let stranger = PalwBondKeyV2(bond_outpoint(0xDEAD));
        let bonds: Vec<PalwBondKeyV2> = seats.iter().map(|seat| seat.bond).chain([stranger]).collect();
        let facts = palw_receipt_pool_facts_v1(&state, &[h64(0xBAD), claim_id], &bonds);
        assert_eq!(
            facts.panels,
            vec![PalwReceiptPanelFactV1 {
                claim_id,
                bound_daa: panel.bound_daa,
                anchor: panel.anchor,
                seats: panel.seats.iter().map(|seat| seat.bond).collect(),
            }],
            "the bound claim answers with its panel, the invented one with nothing"
        );
        assert_eq!(facts.seat_keys.len(), seats.len(), "every seat's key and no stranger's");
        for (bond, key) in &facts.seat_keys {
            assert_eq!(key, &state.bond(bond).unwrap().pubkey);
        }
        assert!(facts.seat_keys.iter().all(|(bond, _)| *bond != stranger));

        // A claim the state holds but has not bound yet has no panel to name.
        let (provisional, unbound) = populated_state();
        assert!(matches!(provisional.claim(&unbound).unwrap().phase, PalwClaimPhaseV2::Provisional));
        assert!(palw_receipt_pool_facts_v1(&provisional, &[unbound], &[]).panels.is_empty());
    }

    /// **ADR-0152 v3.1 SW: the stake-weighted panel draw's pure half** (§3.14, T85–T88, T92–T94 on
    /// synthetic states). The integration — the anchor-time policy value, the room's `ready_eff` in
    /// the fold, T89–T91 — is M4's, after M3.
    mod adr0152_stake_draw {
        use super::*;
        use crate::palw_panel_economy_v1::{PalwSeatEconomyV1, palw_panel_collateral_floor_v1};

        // ---- T88: the corpus `stake: None` must reproduce byte for byte ----------------------

        /// One state of the T88 corpus: the claim on it, the panel shape, the state params it was
        /// folded under, and every policy it is drawn with.
        struct T88Fixture {
            name: &'static str,
            state: PalwChainStateV2,
            claim_id: Hash64,
            params: PalwPanelParamsV2,
            policies: Vec<PalwPanelDrawPolicyV1>,
        }

        fn t88_bond(b: u64, pk: u8, op: u64, collateral: u64, classes: &[u64]) -> PalwConsensusObjectV2 {
            PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint(b)),
                pubkey: vec![pk; 4],
                operator_pubkey: op_key(op),
                collateral,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                capable_classes: classes.iter().map(|c| h64(*c)).collect(),
                signature: Vec::new(),
            }
        }

        /// ADR-0147's BOUGHT class (id 2): a carriage naming `registrant`, which is what makes the
        /// fold record a `registrant_bond` and so what makes its claims outsider-judged.
        fn t88_bought_class(registrant: PalwBondKeyV2) -> PalwConsensusObjectV2 {
            let profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
                .expect("the floor's geometry projects");
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(2),
                artifact_root: h64(12),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 0,
                activation_daa: 0,
                admission: Some(Box::new(crate::palw_state_v2::PalwClassAdmissionCarriageV2 {
                    registrant_bond: registrant,
                    canonical: crate::palw_base0_profile::rc_job_context(&profile, 2, 2),
                    profile,
                    signature: Vec::new(),
                })),
            }
        }

        /// The floor (class 1) and a bought class 2 registered by bond 9; bonds 2..=5 the
        /// registrant's (they serve both), bonds 11..=18 the network's (the floor only), and the
        /// executor's bond 1; one attempt of class 2 by bond 1 at DAA 101.
        fn t88_bought_state(sp: &PalwStateParamsV2, collateral: impl Fn(u64) -> u64) -> (PalwChainStateV2, Hash64) {
            let mut objects = vec![adr0130_class(), t88_bond(1, 7, 0x21, collateral(1), &[1])];
            objects.push(t88_bond(9, 19, 0x29, collateral(9), &[1, 2]));
            for n in 2..=5u64 {
                objects.push(t88_bond(n, 20 + n as u8, 0x40 + n, collateral(n), &[1, 2]));
            }
            for n in 11..=18u64 {
                objects.push(t88_bond(n, 20 + n as u8, 0x40 + n, collateral(n), &[1]));
            }
            objects.push(t88_bought_class(PalwBondKeyV2(bond_outpoint(9))));
            let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), sp, &ctx(1, 100, 1), &objects, None).unwrap();
            let mut env = attempt(40, 1);
            env.attempt.class_id = h64(2);
            env.attempt.artifact_root = h64(12);
            env.attempt.challenge = challenge_v2(h64(999), h64(5), 1_700, 1, h64(2), &bond_outpoint(1));
            let claim_id = attempt_id_v2(&env.attempt);
            let (s2, _) =
                apply_palw_transition_v2(&s1, sp, &PalwBlockContextV2 { subsidy: 100_000, ..ctx(2, 101, 2) }, &[], Some(&env))
                    .unwrap();
            (s2, claim_id)
        }

        fn t88_economy(floor: u64, ratio: u32, lambda: u32) -> PalwSeatEconomyV1 {
            PalwSeatEconomyV1 { panel_floor_sompi: floor, max_exposure_ratio_permille: ratio, reward_multiple_permille: lambda }
        }

        /// Every combination of `weighted`, `economy`, `independence` and `valid_lock` the corpus
        /// draws a state under, `stake: None` throughout.
        fn t88_policies(
            economies: &[Option<PalwSeatEconomyV1>],
            independences: &[Option<PalwPanelIndependenceV1>],
            locks: &[Option<PalwPanelValidLockV1>],
        ) -> Vec<PalwPanelDrawPolicyV1> {
            let mut out = Vec::new();
            for weighted in [false, true] {
                for economy in economies {
                    for independence in independences {
                        for valid_lock in locks {
                            out.push(PalwPanelDrawPolicyV1 {
                                weighted,
                                economy: *economy,
                                readiness: None,
                                independence: *independence,
                                valid_lock: *valid_lock,
                                stake: None,
                            });
                        }
                    }
                }
            }
            out
        }

        fn t88_corpus() -> Vec<T88Fixture> {
            let lock = |sp: &PalwStateParamsV2, required: u128| PalwPanelValidLockV1 {
                required,
                now_daa: 103,
                settled_anchor_depth: None,
                window_court: sp.window_court(),
                rcore: None,
            };
            let independence =
                |from_daa: u64, anchor_daa: u64| PalwPanelIndependenceV1 { from_daa, base_class_id: h64(1), anchor_daa };
            let mut out = Vec::new();

            // (a) The dedup/exclusion registry: six bonds, bond 5 sharing bond 4's operator, bond 6
            // the executor's operator.
            let sp = state_params();
            let (state, claim_id) = populated_state();
            out.push(T88Fixture {
                name: "populated",
                state,
                claim_id,
                params: panel_params(),
                policies: t88_policies(
                    &[None, Some(t88_economy(100, 1000, 0)), Some(t88_economy(1_000, 500, 0))],
                    &[None, Some(independence(0, 150)), Some(independence(0, 100))],
                    &[None, Some(lock(&sp, 1)), Some(lock(&sp, 2_000_000))],
                ),
            });

            // (b) ADR-0130's λ registry: bonds that back the claim-priced stake and not the floor.
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap().with_worker_carve_permille(620).unwrap();
            let bonds = vec![
                adr0130_bond(1, 7, 0x21, 1_000_000),
                adr0130_bond(2, 8, 0x22, 2_000),
                adr0130_bond(3, 9, 0x23, 2_000),
                adr0130_bond(4, 10, 0x24, 10_000),
                adr0130_bond(5, 11, 0x25, 10_000),
                adr0130_bond(6, 12, 0x26, 9_000),
                adr0130_bond(7, 13, 0x25, 12_000), // a second bond of bond 5's operator
            ];
            let (state, claim_id) = adr0130_state(&sp, bonds, 100_000);
            let floor = palw_panel_collateral_floor_v1(100);
            out.push(T88Fixture {
                name: "adr0130-lambda",
                state,
                claim_id,
                params: panel_params(),
                policies: t88_policies(
                    &[
                        None,
                        Some(t88_economy(floor, 500, 0)),
                        Some(t88_economy(floor, 500, 1_000)),
                        Some(t88_economy(floor, 500, 1_100)),
                    ],
                    &[None, Some(independence(0, 150))],
                    &[None, Some(lock(&sp, 9_500))],
                ),
            });

            // (c) A wide registry on a five-seat panel, collateral spread over three orders.
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let mut bonds = vec![adr0130_bond(1, 7, 0x21, 1_000_000)];
            for n in 2..=13u64 {
                bonds.push(adr0130_bond(n, 20 + n as u8, 0x60 + n, 1_000 * (1 + (n * 37) % 11) * 10u64.pow((n % 3) as u32)));
            }
            let (state, claim_id) = adr0130_state(&sp, bonds, 0);
            out.push(T88Fixture {
                name: "wide",
                state,
                claim_id,
                params: PalwPanelParamsV2::new(5, 3, 4).unwrap(),
                policies: t88_policies(
                    &[None, Some(t88_economy(1_000, 500, 0)), Some(t88_economy(1_000, 1000, 0))],
                    &[None, Some(independence(0, 150))],
                    &[None, Some(lock(&sp, 5_000))],
                ),
            });

            // (d) ADR-0147: a claim of a bought class, so an outsider sits first where the policy
            // configures independence at or below the claim's acceptance.
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (state, claim_id) = t88_bought_state(&sp, |n| 1_000_000 + 7_919 * n);
            out.push(T88Fixture {
                name: "bought",
                state,
                claim_id,
                params: PalwPanelParamsV2::new(3, 2, 4).unwrap(),
                policies: t88_policies(
                    &[None, Some(t88_economy(1_000, 500, 0))],
                    &[None, Some(independence(0, 150)), Some(independence(102, 150)), Some(independence(0, 100))],
                    &[None, Some(lock(&sp, 1_030_000))],
                ),
            });
            out
        }

        /// The corpus drawn, every result folded into one digest: `(fixture, policy index, anchor,
        /// Ok(seats) as Borsh | Err as its Debug)`. Returns the digest and the number of results
        /// that were panels and refusals, so a caller can see the corpus is not vacuous.
        fn t88_digest(with: impl Fn(PalwPanelDrawPolicyV1) -> PalwPanelDrawPolicyV1) -> (String, usize, usize) {
            let mut digest = blake2b_simd::Params::new().hash_length(32).to_state();
            let (mut panels, mut refusals) = (0usize, 0usize);
            for fixture in t88_corpus() {
                for (p, policy) in fixture.policies.iter().enumerate() {
                    for i in 0..24u64 {
                        let anchor = BlockHash::from_u64_word(0x8800 + i);
                        let drawn = derive_panel_v2_with_policy(
                            &fixture.state,
                            &fixture.params,
                            &fixture.claim_id,
                            anchor,
                            100,
                            None,
                            false,
                            with(*policy),
                        );
                        digest.update(fixture.name.as_bytes());
                        digest.update(&(p as u64).to_le_bytes());
                        digest.update(&i.to_le_bytes());
                        match drawn {
                            Ok(seats) => {
                                panels += 1;
                                digest.update(b"ok");
                                digest.update(&borsh::to_vec(&seats).unwrap());
                            }
                            Err(e) => {
                                refusals += 1;
                                digest.update(b"err");
                                digest.update(format!("{e:?}").as_bytes());
                            }
                        }
                    }
                }
            }
            (faster_hex::hex_string(digest.finalize().as_bytes()), panels, refusals)
        }

        /// **T88 (SW-1): `stake: None` is today's draw, byte for byte.** The corpus — four
        /// registries (dedup and executor exclusions, ADR-0130's λ floor with a two-bond operator,
        /// a wide five-seat registry, and ADR-0147's bought class with its outsider) under every
        /// combination of `weighted`, `economy`, `independence` and `valid_lock` — is drawn at 24
        /// anchors each, and the digest of every panel and every refusal is pinned to the value
        /// the draw produced at `f1dfb33b`, before the stake draw existed.
        #[test]
        fn t88_stake_none_is_todays_draw_byte_for_byte() {
            let (digest, panels, refusals) = t88_digest(|policy| policy);
            eprintln!("T88 corpus digest {digest}: {panels} panels, {refusals} refusals");
            assert!(panels > 1_000 && refusals > 100, "the corpus draws panels and refusals both: {panels} / {refusals}");
            assert_eq!(
                digest, "76bb846d28e5007f9224ed883ae6252deea8233ecadfda3b2c60aff428c1062a",
                "stake: None moved a panel or a refusal somewhere in the corpus"
            );
        }

        /// **T88's other half: `stake: Some` replaces the lottery, whatever `weighted` says.** The
        /// same corpus under the stake draw is a different digest (so the pin above is not vacuous),
        /// and `weighted: true` and `false` draw the same panel or the same refusal at every point.
        #[test]
        fn t88_stake_some_replaces_the_lottery_whatever_weighted_says() {
            let stake = |policy: PalwPanelDrawPolicyV1| PalwPanelDrawPolicyV1 { stake: Some(PalwPanelStakeDrawV1::V1), ..policy };
            let (digest, panels, _) = t88_digest(stake);
            assert!(panels > 500, "the stake draw seats panels on the corpus: {panels}");
            assert_ne!(
                digest, "76bb846d28e5007f9224ed883ae6252deea8233ecadfda3b2c60aff428c1062a",
                "the stake draw is a different draw"
            );
            let (flat, _, _) = t88_digest(|policy| stake(PalwPanelDrawPolicyV1 { weighted: false, ..policy }));
            let (bucketed, _, _) = t88_digest(|policy| stake(PalwPanelDrawPolicyV1 { weighted: true, ..policy }));
            assert_eq!(flat, bucketed, "C-02's `weighted` is not read under the stake draw");
        }

        // ---- realistic registries (whole-MSK collateral) -----------------------------------------

        const MSK: u64 = crate::constants::SOMPI_PER_KASPA;
        /// A testnet-12 genesis seat: 939,063.21 MSK (whole-MSK weight 939,063).
        const GENESIS_SEAT: u64 = 939_063 * MSK + 21_000_000;
        /// The testnet-12 seat floor, 130,000 MSK — a floor-sized Sybil operator.
        const FLOOR_SEAT: u64 = 130_000 * MSK;

        fn sw_economy() -> PalwSeatEconomyV1 {
            t88_economy(FLOOR_SEAT, 500, 0)
        }

        /// Past `palw_rcore_plus` on testnet-12: the economy and the stake draw's v3.1 terms.
        fn sw_policy() -> PalwPanelDrawPolicyV1 {
            PalwPanelDrawPolicyV1 { economy: Some(sw_economy()), stake: Some(PalwPanelStakeDrawV1::V1), ..Default::default() }
        }

        /// The same network below the fence: ADR-0130's operator lottery.
        fn lottery_policy() -> PalwPanelDrawPolicyV1 {
            PalwPanelDrawPolicyV1 { economy: Some(sw_economy()), ..Default::default() }
        }

        fn five() -> PalwPanelParamsV2 {
            PalwPanelParamsV2::new(5, 3, 4).unwrap()
        }

        fn anchor(i: u64) -> BlockHash {
            BlockHash::from_u64_word(0x5700_0000 + i)
        }

        /// A class whose attempts may claim `2 × 10^13` pwu, so one of them reserves 1,000,000 MSK
        /// (at the network's 5 sompi a pwu) on its producer — past any genesis seat's 500‰ ceiling
        /// (469,531 MSK) — and one attempt SATURATES a bond's economy headroom. Folded without
        /// the 2026-09-23 audit extras, the only place an attempt is refused at the ceiling, so the
        /// reservation lands whole.
        const HEAVY_PWU: u64 = 20_000_000_000_000;

        fn heavy_class() -> PalwConsensusObjectV2 {
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(3),
                artifact_root: h64(13),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(HEAVY_PWU),
                initial_target: u128::MAX / 2,
                share_permille: 0,
                activation_daa: 0,
                admission: None,
            }
        }

        /// Fold one attempt of `class` by bond `b` (key `pk`, operator `op`) at `daa`, at the first
        /// nonce the fold admits.
        #[allow(clippy::too_many_arguments)]
        fn fold_attempt(
            parent: &PalwChainStateV2,
            sp: &PalwStateParamsV2,
            daa: u64,
            b: u64,
            pk: u8,
            op: u64,
            class_id: Hash64,
            root: Hash64,
            pwu: u64,
        ) -> (PalwChainStateV2, Hash64) {
            for nonce in 1..400u64 {
                let mut env = attempt(pwu, nonce);
                env.attempt.class_id = class_id;
                env.attempt.artifact_root = root;
                env.attempt.executor_bond = bond_outpoint(b);
                env.attempt.executor_pubkey = vec![pk; 4];
                env.attempt.operator_id = op_id(op);
                env.attempt.challenge = challenge_v2(h64(999), h64(5), 1_700, nonce, class_id, &bond_outpoint(b));
                let id = attempt_id_v2(&env.attempt);
                if let Ok((next, _)) = apply_palw_transition_v2(parent, sp, &ctx(daa, daa, daa), &[], Some(&env))
                    && next.claim(&id).is_some()
                {
                    return (next, id);
                }
            }
            panic!("no attempt of bond {b} admits at daa {daa}")
        }

        /// A registry of `(bond, operator, collateral)` rows beside the executor (bond 1, operator
        /// 0x21) at DAA 100, every bond serving the floor; one heavy attempt per bond in `saturate`
        /// (DAA 101, 102, …); then the claim under judgement — the executor's 40-pwu floor attempt,
        /// `reserved` 200, so a seat reserves 600 sompi. Keys are `30 + bond`.
        fn sw_state(rows: &[(u64, u64, u64)], saturate: &[u64]) -> (PalwChainStateV2, Hash64) {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let mut objects = vec![adr0130_class(), heavy_class(), adr0130_bond(1, 7, 0x21, 1_000_000)];
            objects.extend(rows.iter().map(|(b, op, c)| adr0130_bond(*b, 30 + *b as u8, *op, *c)));
            let (mut state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();
            let mut daa = 101;
            for b in saturate {
                let op = rows.iter().find(|row| row.0 == *b).expect("a saturated bond is a row").1;
                state = fold_attempt(&state, &sp, daa, *b, 30 + *b as u8, op, h64(3), h64(13), HEAVY_PWU).0;
                daa += 1;
            }
            fold_attempt(&state, &sp, daa, 1, 7, 0x21, h64(1), h64(11), 40)
        }

        /// The eight testnet-12 genesis seats (bonds 2..=9) and `small` floor-sized operators
        /// (bonds 10, 11, …).
        fn genesis_and_small(small: u64) -> Vec<(u64, u64, u64)> {
            let mut rows: Vec<(u64, u64, u64)> = (2..=9u64).map(|b| (b, 0x40 + b, GENESIS_SEAT)).collect();
            rows.extend((0..small).map(|i| (10 + i, 0x4A + i, FLOOR_SEAT)));
            rows
        }

        /// The class population and SW-10's base for the claim, as `derive_panel_v2_with_policy`
        /// computes them under `sw_policy()` (no outsider).
        #[allow(clippy::type_complexity)]
        fn populations<'a>(
            state: &'a PalwChainStateV2,
            claim_id: &Hash64,
        ) -> (Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>, Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>) {
            let eligible = palw_panel_eligible_bonds_v2(state, claim_id, 100, None, false, None, Some(sw_economy()), 5).unwrap();
            let base = palw_panel_stake_base_bonds_judging_v1(state, claim_id, &h64(1), 100, None, false, None, Some(sw_economy()), 5)
                .unwrap();
            (eligible, base)
        }

        /// **The exact successive-sampling law by enumeration of ordered draws**: every operator's
        /// inclusion probability, and `P(A = a)` for the `marked` operators. Independent of the
        /// race: it draws seat by seat in proportion to weight among those not yet seated.
        fn exact_law(weights: &[f64], seats: usize, marked: &[bool]) -> (Vec<f64>, Vec<f64>) {
            #[allow(clippy::too_many_arguments)]
            fn walk(
                w: &[f64],
                seats: usize,
                marked: &[bool],
                seq: &mut Vec<usize>,
                p: f64,
                rem: f64,
                inc: &mut [f64],
                law: &mut [f64],
            ) {
                if seq.len() == seats || rem <= 0.0 {
                    for &i in seq.iter() {
                        inc[i] += p;
                    }
                    law[seq.iter().filter(|&&i| marked[i]).count()] += p;
                    return;
                }
                for (i, wi) in w.iter().enumerate() {
                    if seq.contains(&i) {
                        continue;
                    }
                    seq.push(i);
                    walk(w, seats, marked, seq, p * wi / rem, rem - wi, inc, law);
                    seq.pop();
                }
            }
            let mut inc = vec![0.0; weights.len()];
            let mut law = vec![0.0; seats + 1];
            walk(weights, seats, marked, &mut Vec::new(), 1.0, weights.iter().sum(), &mut inc, &mut law);
            (inc, law)
        }

        /// `|measured − p| ≤ 4σ` of a binomial proportion over `n` draws.
        fn within_4_sigma(hits: usize, n: usize, p: f64) -> bool {
            let measured = hits as f64 / n as f64;
            (measured - p).abs() <= 4.0 * (p * (1.0 - p) / n as f64).sqrt()
        }

        // ---- T85: the integer −log2, the key order, the cap ---------------------------------------

        /// `(u, palw_draw_neg_log2_q64_v1(u))`: the six edges, then 64 values of splitmix64 from
        /// seed `0x5712_A152`. Generated outside the tree by a Python port of §3.14's routine
        /// (`t85_golden.py`, beside this commit's review notes), which checks every output against
        /// `−log2((u + 1) / 2^64)` from an 80-digit decimal logarithm: worst error `1.44 × 10^-19`
        /// (under three units of `2^-64`, always above the exact value — the fraction bits
        /// truncate), against the `4 × 10^-15` §3.14 states.
        const T85_VECTORS: [(u64, u128); 70] = [
            (0x0000000000000000, 0x400000000000000000),
            (0x0000000000000001, 0x3f0000000000000000),
            (0x7fffffffffffffff, 0x10000000000000000),
            (0x8000000000000000, 0xfffffffffffffffe),
            (0xfffffffffffffffe, 0x2),
            (0xffffffffffffffff, 0x0),
            (0x6f0f403721bcd0aa, 0x1346e6e6b1bf945db),
            (0xcc48eed2a23d2ccc, 0x5357fbf482e9dc53),
            (0xa5d9f964820d1450, 0xa0520e78dc92b8a3),
            (0x1d3811b4b2c524b5, 0x32193f887684f1843),
            (0xc890505e92c5144a, 0x5a221e4aca7d17c9),
            (0x0dd572e79332baca, 0x435ba6f2e7716b368),
            (0x70f4e4352085f99e, 0x12e2d10d7faead78c),
            (0x28164e8dc7cd47ac, 0x2acc861338f9bcf56),
            (0x8e7d596406d58786, 0xd864b605086db2d2),
            (0x789235dd2c34f28f, 0x11615161fc80e9ea0),
            (0xff1cf402f0ab197f, 0x14820c86687c893),
            (0x81f7c1d9eed22496, 0xfa5d860b1161167d),
            (0x3639e8beb8807177, 0x23d34741903cfd8cf),
            (0x4af892770b56e5bf, 0x1c590b92232fc90d7),
            (0x2bab0702e7601b8c, 0x28d2ea447071b093d),
            (0xe00339b41b285a75, 0x314bdeb4fa70f6dd),
            (0xb605ffe5afd25605, 0x7df4ff1090dac063),
            (0x8c145cfd0f1c21d0, 0xdeb19a924a89178f),
            (0xf94f9fb8e8d4708e, 0x9c748e17e97ef88),
            (0x22cd9af9af93a1ab, 0x2e0fc967414b3a5d4),
            (0xfd2f03424842de22, 0x415edbf7292f5ba),
            (0x247d8bfc0540f800, 0x2cf807bdd1c4cfa28),
            (0xe22e0137a0d0e72c, 0x2dbd99ab9518c221),
            (0xc004ddc0c718c35a, 0x6a368991defac57e),
            (0xe8af11918c1297d2, 0x2345119e09dbe26e),
            (0xc78d5daa8ba63461, 0x5c002ac937d1c69d),
            (0xf72f254e22c46335, 0xcf1560c808af770),
            (0x3ee13160bb812e38, 0x20685c23837ca6fb9),
            (0xad1b1d3e35688dba, 0x90825654e2bc48b9),
            (0xe9ba05e099cba1e8, 0x219e4a1466267fbd),
            (0x01480ef3d43358be, 0x7a466a3035c381c3f),
            (0x5f9ed7f03c63fd85, 0x16bb66ae1801f8f26),
            (0x045a675b561e3d3c, 0x5e0c0eb52a9835556),
            (0xaa0f6b9e047b73b9, 0x9710a90ac3fbdb4d),
            (0xad9e506ecb9d3080, 0x8f6ad44a50a48bec),
            (0x910e54e7d6ff0a0f, 0xd1ccfb0f424ae85f),
            (0xf205af685e54574c, 0x14bcb3ea5a890d1f),
            (0xa3340f0d447fae95, 0xa643de728d886c7b),
            (0x10cc420d47dabf92, 0x3ee06e1765a7b9c45),
            (0x5095f9c2ac742948, 0x1aae4431af73682d9),
            (0xbe8eeb226ea39f15, 0x6d088a0d488bbd7b),
            (0x25d6deed77794673, 0x2c21869bfaca1a418),
            (0x50d6646795125df1, 0x1a9bd7f34d623cf30),
            (0x5ab6d28f7e12738e, 0x17f2aa24a6ff67e5e),
            (0x4d11633eafaca04d, 0x1bb6083a0f0daffb0),
            (0x2abf4be66f7daa88, 0x2950db21676dc68e2),
            (0x7c222d3ec2f9dcf5, 0x10b540d12a3d7e077),
            (0x6c798c8fb6389914, 0x13d20f090f5185f25),
            (0x5561907a877fa57b, 0x1958b2e27cee38d56),
            (0x517425c83e98f488, 0x1a6ef7a5e456674b6),
            (0xa571e6f61813d29d, 0xa13a1894d49e771d),
            (0x34e44700758e0471, 0x2466863408dcc3bd2),
            (0x935bbfd164172771, 0xcbfc09d44253174d),
            (0xb79973b7876bb430, 0x7ac5e7bb25c254a7),
            (0x8e5361f7bef9c060, 0xd8d18cc2629af30b),
            (0x7b2dc216fedd9f75, 0x10e2e1266f9b47014),
            (0xf9db28871146c3ed, 0x8f8cdc246c0f101),
            (0xa52f4785c864523f, 0xa1cef02837301c3c),
            (0x90dad5f0caa23611, 0xd2502fc3be7a7583),
            (0x6b712376f16623bb, 0x140a98172e6b25f91),
            (0x4c16f1ed05e6584f, 0x1c018626ae9f2dddc),
            (0xae78235afd59e726, 0x8d9c9849f3a1c036),
            (0x2eebaf871e547def, 0x272a63b53974b38c7),
            (0xb278b4a1367eeb4a, 0x853c36ea74408f0e),
        ];

        fn splitmix64(state: &mut u64) -> u64 {
            *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = *state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        /// **T85 (SW-3): the integer `−log2` is the normative routine, monotone, and never
        /// overflows; the key order is the cross-multiplication with ties by operator id; `W` is
        /// capped.** Debug builds check every `u128` operation for overflow, so running the routine
        /// over the vectors and the sweep IS the no-overflow check.
        #[test]
        fn t85_the_integer_log_the_key_order_and_the_cap() {
            // The golden vectors, and the table's pseudo-random half is what splitmix64 says.
            let mut seed = 0x5712_A152u64;
            for (i, (u, l)) in T85_VECTORS.iter().enumerate() {
                assert_eq!(palw_draw_neg_log2_q64_v1(*u), *l, "vector {i}: u = {u:#x}");
                if i >= 6 {
                    assert_eq!(*u, splitmix64(&mut seed), "vector {i} is splitmix64's");
                }
                // And against the float logarithm, loosely (the tight bound is the script's).
                let float = 64.0 - ((*u as f64) + 1.0).log2();
                assert!((*l as f64 / 2f64.powi(64) - float).abs() < 1e-9, "vector {i}: {l:#x} against {float}");
            }
            assert_eq!(palw_draw_neg_log2_q64_v1(0), 64u128 << 64, "u = 0 is the largest key, 2^70");
            assert_eq!(palw_draw_neg_log2_q64_v1(u64::MAX), 0, "u = 2^64 − 1 is the smallest");

            // Monotone: never rises as u rises — over 20,000 draws, every power of two and its
            // neighbours, and every step across the 2^63 and 2^64 boundaries.
            let mut us: Vec<u64> = (0..20_000).map(|_| splitmix64(&mut seed)).collect();
            for k in 0..64u32 {
                let p = 1u64 << k;
                us.extend([p.wrapping_sub(1), p, p.saturating_add(1)]);
            }
            us.extend((0..512u64).map(|d| (1u64 << 63) - 256 + d));
            us.extend((0..256u64).map(|d| u64::MAX - d));
            us.sort_unstable();
            us.dedup();
            let mut previous = u128::MAX;
            for u in &us {
                let l = palw_draw_neg_log2_q64_v1(*u);
                assert!(l <= previous, "monotone at u = {u:#x}");
                assert!(l <= 64u128 << 64);
                previous = l;
            }

            // The key order: `L_i · W_j < L_j · W_i`, ties by operator id.
            let (a, b) = (h64(1), h64(2));
            assert_eq!(palw_draw_key_cmp_v1(6, 3, &b, 5, 3, &a), std::cmp::Ordering::Greater, "6/3 after 5/3");
            assert_eq!(palw_draw_key_cmp_v1(6, 3, &b, 4, 2, &a), std::cmp::Ordering::Greater, "6/3 = 4/2: the id decides, 2 after 1");
            assert_eq!(palw_draw_key_cmp_v1(6, 3, &a, 4, 2, &b), std::cmp::Ordering::Less, "…and 1 before 2");
            assert_eq!(palw_draw_key_cmp_v1(100, 2, &b, 60, 1, &a), std::cmp::Ordering::Less, "50 before 60: the heavier key wins");
            // The extremes multiply inside u128.
            let top = 64u128 << 64;
            assert_eq!(
                palw_draw_key_cmp_v1(top, PALW_DRAW_WEIGHT_MAX_MSK_V1, &a, top - 1, PALW_DRAW_WEIGHT_MAX_MSK_V1, &b),
                std::cmp::Ordering::Greater
            );
            assert_eq!(palw_draw_key_cmp_v1(top, u64::MAX, &a, top, u64::MAX, &b), std::cmp::Ordering::Less, "clamped, then the id");

            // W: whole MSK, capped at the policy's cap, under 2^40, never below 1.
            let cap = PalwPanelStakeDrawV1::V1.weight_cap_msk;
            assert_eq!(cap, 1_000_000);
            assert_eq!(palw_draw_operator_weight_msk_v1((GENESIS_SEAT / MSK) as u128, cap), 939_063, "a genesis seat, uncapped");
            assert_eq!(palw_draw_operator_weight_msk_v1(20_000_000, cap), 1_000_000, "a 20M operator weighs the cap");
            assert_eq!(palw_draw_operator_weight_msk_v1(0, cap), 1, "every eligible operator has a key");
            assert_eq!(palw_draw_operator_weight_msk_v1(u128::MAX, u64::MAX), PALW_DRAW_WEIGHT_MAX_MSK_V1, "the arithmetic bound");

            // The ticket is H(domain ‖ anchor ‖ claim ‖ operator) under the lottery's hasher, and u
            // is its first eight bytes little-endian.
            let (anchor, claim, operator) = (BlockHash::from_u64_word(9), h64(8), h64(7));
            let mut by_hand = keyed(PALW_PANEL_V2_DOMAIN_STAKE_TICKET);
            by_hand.update(anchor.as_byte_slice());
            by_hand.update(claim.as_byte_slice());
            by_hand.update(operator.as_byte_slice());
            let ticket = finish(by_hand);
            assert_eq!(palw_panel_stake_ticket_v1(anchor, &claim, &operator), ticket);
            assert_eq!(palw_draw_ticket_u64_v1(&ticket).to_le_bytes(), ticket.as_byte_slice()[..8]);
            assert_ne!(palw_panel_stake_outsider_ticket_v1(anchor, &claim, &operator), ticket, "the outsider has its own domain");
            assert_ne!(palw_panel_operator_ticket_v1(anchor, &claim, &operator), ticket, "and the lottery's is not reused");
        }

        // ---- T86: the law ---------------------------------------------------------------------------

        /// **T86 (SW-3, SW-6): the draw is successive sampling.** Eight genesis seats and three
        /// floor-sized operators on a five-seat panel, over 2^16 anchors: every operator's inclusion
        /// is within 4σ of the exact successive-sampling inclusion (0.58477 a genesis seat, 0.10729 a
        /// 130k operator — enumerated here, and `v31_stake_draw.py`'s `dist_A((8, 939,063), (3,
        /// 130,000))` gives the same `E[A]/3`). The S1 assignment is a hash of `(anchor, claim,
        /// seat_count)` that the race does not feed, so the full seat and a segment's holder are a
        /// uniform ordered pair of positions: the measured P2 of the three small operators matches
        /// `E[A(A − 1)] / 20` of the exact law. And with equal weights the race is today's uniform
        /// law, `5/11` each.
        #[test]
        fn t86_the_draw_is_successive_sampling_and_s1_is_untouched() {
            let (state, claim_id) = sw_state(&genesis_and_small(3), &[]);
            let (eligible, base) = populations(&state, &claim_id);
            assert_eq!((eligible.len(), base.len()), (11, 11));
            let stake = PalwPanelStakeDrawV1::V1;
            let operators: Vec<Hash64> = (2..=12u64).map(|b| op_id(if b <= 9 { 0x40 + b } else { 0x4A + b - 10 })).collect();
            let small = |op: &Hash64| operators[8..].contains(op);
            let weights: Vec<f64> = (0..11).map(|i| if i < 8 { 939_063.0 } else { 130_000.0 }).collect();
            let marked: Vec<bool> = (0..11).map(|i| i >= 8).collect();
            let (inclusion, law) = exact_law(&weights, 5, &marked);
            assert!((inclusion[0] - 0.584_767_875_6).abs() < 1e-9 && (inclusion[8] - 0.107_285_665_1).abs() < 1e-9);
            let p2_exact = law.iter().enumerate().map(|(a, p)| (a * a.saturating_sub(1)) as f64 * p).sum::<f64>() / 20.0;
            assert!((p2_exact - 0.002_431_702_46).abs() < 1e-9, "E[A(A − 1)] / 20 = {p2_exact}");

            const N: usize = 1 << 16;
            let mut seated = [0usize; 11];
            let mut p2_hits = 0usize;
            let lottery = lottery_policy();
            let mut differs = 0usize;
            let mut s1_s3 = blake2b_simd::Params::new().hash_length(32).to_state();
            for i in 0..N as u64 {
                let at = anchor(i);
                let seats = palw_panel_stake_race_of_v1(5, &claim_id, at, &eligible, &base, &stake).expect("a full panel");
                for seat in &seats {
                    seated[operators.iter().position(|op| *op == seat.operator_id).unwrap()] += 1;
                }
                // S1: the full seat and segment 0's partial holder, by position.
                let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(at, claim_id, 5);
                let holder = (0..5u16).find(|s| *s != assignment.full_seat && assignment.mask_of(*s).covers(0)).unwrap();
                p2_hits += usize::from(
                    small(&seats[assignment.full_seat as usize].operator_id) && small(&seats[holder as usize].operator_id),
                );
                if i < 64 {
                    // The race IS the draw: `derive_panel_v2_with_policy` seats the same panel.
                    let derived = derive_panel_v2_with_policy(&state, &five(), &claim_id, at, 100, None, false, sw_policy()).unwrap();
                    assert_eq!(derived, seats);
                    // SW-6: the assignment and the S3 sites read the bind — `(anchor, claim,
                    // seat_count)` and `(anchor, claim, seat_index)` — and never the panel, so the race
                    // cannot move them; their outputs are pinned below against today's.
                    let other = derive_panel_v2_with_policy(&state, &five(), &claim_id, at, 100, None, false, lottery).unwrap();
                    differs += usize::from(other != seats);
                    s1_s3.update(format!("{assignment:?}").as_bytes());
                    for s in 0..5u16 {
                        let sites = crate::palw_layer_sample_v3::palw_layer_sample_v3(at, claim_id, s, 4, 64, 3);
                        s1_s3.update(format!("{sites:?}").as_bytes());
                    }
                }
            }
            assert!(differs > 0, "the lottery and the race must disagree somewhere, or the comparison is vacuous");
            assert_eq!(
                faster_hex::hex_string(s1_s3.finalize().as_bytes()),
                "67384c79d5fa0ac675d0bdba2d84e1c5a13872ba841530ec1f5be1cbe33ae303",
                "the S1 assignment and the S3 sites are today's"
            );
            for (i, hits) in seated.iter().enumerate() {
                assert!(
                    within_4_sigma(*hits, N, inclusion[i]),
                    "operator {i}: seated {hits} of {N}, exact inclusion {:.5}",
                    inclusion[i]
                );
            }
            assert!(within_4_sigma(p2_hits, N, p2_exact), "P2 measured {p2_hits} of {N}, exact {p2_exact:.6}");

            // Equal weights: a cap of one MSK makes every weight 1, and the race is uniform.
            let uniform = PalwPanelStakeDrawV1 { weight_cap_msk: 1, ..stake };
            let mut seated = [0usize; 11];
            const M: usize = 1 << 14;
            for i in 0..M as u64 {
                for seat in palw_panel_stake_race_of_v1(5, &claim_id, anchor(i), &eligible, &base, &uniform).unwrap() {
                    seated[operators.iter().position(|op| *op == seat.operator_id).unwrap()] += 1;
                }
            }
            for (i, hits) in seated.iter().enumerate() {
                assert!(within_4_sigma(*hits, M, 5.0 / 11.0), "equal weights: operator {i} seated {hits} of {M}");
            }
        }

        // ---- T87: the weight and the keys ----------------------------------------------------------

        /// **T87 (SW-2, SW-7): an operator's weight sums its eligible bonds, a bond registered at or
        /// after the anchor adds nothing, no other operator moves a key, and splitting leaves the
        /// first seat's law and `P(at least one seat)` where they were.**
        #[test]
        fn t87_the_weight_sums_eligible_bonds_and_keys_are_the_operators_own() {
            let stake = PalwPanelStakeDrawV1::V1;
            // Operator X (0x50) holds two bonds, 400,000 and 300,000 MSK (ids are not unique on this
            // fixture; on testnet-12 they are, and the sum is one bond's collateral).
            let mut rows = genesis_and_small(0);
            rows.truncate(6);
            rows.extend([(20, 0x50, 400_000 * MSK), (21, 0x50, 300_000 * MSK)]);
            let (state, claim_id) = sw_state(&rows, &[]);
            let (eligible, _) = populations(&state, &claim_id);
            let x = op_id(0x50);
            for i in 0..16u64 {
                let entries = palw_panel_stake_entries_v1(&claim_id, anchor(i), &eligible, &stake);
                let entry = entries.iter().find(|e| e.operator_id == x).unwrap();
                assert_eq!(entry.weight_msk, 700_000, "the sum of X's two bonds");
                let best = [20u64, 21]
                    .iter()
                    .map(|n| PalwBondKeyV2(bond_outpoint(*n)))
                    .min_by_key(|bond| (palw_panel_seat_ticket_v1(anchor(i), &claim_id, bond), *bond))
                    .unwrap();
                assert_eq!(entry.bond, best, "X sits with today's candidate bond");
                assert_eq!(entries.iter().filter(|e| e.operator_id == x).count(), 1, "one entry an operator");
            }
            // Past the cap the sum stops.
            let mut heavy = rows.clone();
            heavy.push((22, 0x50, 600_000 * MSK));
            let (heavy_state, heavy_claim) = sw_state(&heavy, &[]);
            assert_eq!(heavy_claim, claim_id);
            let (heavy_eligible, _) = populations(&heavy_state, &heavy_claim);
            let entries = palw_panel_stake_entries_v1(&claim_id, anchor(0), &heavy_eligible, &stake);
            assert_eq!(entries.iter().find(|e| e.operator_id == x).unwrap().weight_msk, 1_000_000, "1.3M capped at 1,000,000");

            // A bond registered AT or after the anchor adds nothing (ADR-0147's cut, SW-8): X's
            // second bond registers at DAA 150; under independence the claim anchored at DAA 150
            // draws exactly the panel the registry without that bond draws, and at 151 it counts.
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let mut early = rows.clone();
            early.pop();
            let (lean, lean_claim) = sw_state(&early, &[]);
            assert_eq!(lean_claim, claim_id);
            let (late, _) =
                apply_palw_transition_v2(&lean, &sp, &ctx(150, 150, 150), &[adr0130_bond(21, 51, 0x50, 300_000 * MSK)], None)
                    .expect("the late bond registers");
            let at = |anchor_daa| PalwPanelDrawPolicyV1 {
                independence: Some(PalwPanelIndependenceV1 { from_daa: 0, base_class_id: h64(1), anchor_daa }),
                ..sw_policy()
            };
            let mut moved = false;
            for i in 0..64u64 {
                let draw = |state: &PalwChainStateV2, policy| {
                    derive_panel_v2_with_policy(state, &five(), &claim_id, anchor(i), 100, None, false, policy).unwrap()
                };
                assert_eq!(draw(&late, at(150)), draw(&lean, at(150)), "anchor {i}: registered at the anchor, it adds nothing");
                moved |= draw(&late, at(151)) != draw(&lean, at(151));
            }
            assert!(moved, "registered before the anchor, the bond adds weight — or the refusal above is vacuous");

            // Adding or removing any OTHER operator never moves an operator's key.
            let (wide, _) = sw_state(&genesis_and_small(3), &[]);
            let (all, _) = populations(&wide, &claim_id);
            let without: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> =
                all.iter().copied().filter(|(_, bond)| bond.operator_id != op_id(0x44)).collect();
            let with_stranger_row = {
                let mut rows = genesis_and_small(3);
                rows.push((30, 0x70, 250_000 * MSK));
                rows
            };
            let (stranger, _) = sw_state(&with_stranger_row, &[]);
            let (with, _) = populations(&stranger, &claim_id);
            for i in 0..32u64 {
                let full = palw_panel_stake_entries_v1(&claim_id, anchor(i), &all, &stake);
                for other in [
                    palw_panel_stake_entries_v1(&claim_id, anchor(i), &without, &stake),
                    palw_panel_stake_entries_v1(&claim_id, anchor(i), &with, &stake),
                ] {
                    for entry in &other {
                        if let Some(same) = full.iter().find(|e| e.operator_id == entry.operator_id) {
                            assert_eq!(same, entry, "anchor {i}: an operator's key is its own");
                        }
                    }
                }
            }

            // Splitting: X of 260,000 MSK against X1 + X2 of 130,000 each, beside the eight genesis
            // seats. The group's first key is distributed as X's (the minimum of two exponentials of
            // rate W/2 is one of rate W), so the first seat's law and P(at least one seat) are the
            // same — exactly in law, and within 4σ here over 2^14 anchors each.
            let mut whole = genesis_and_small(0);
            whole.push((20, 0x50, 260_000 * MSK));
            let mut split = genesis_and_small(0);
            split.extend([(20, 0x50, FLOOR_SEAT), (21, 0x51, FLOOR_SEAT)]);
            let group = [op_id(0x50), op_id(0x51)];
            let mut weights: Vec<f64> = vec![939_063.0; 8];
            weights.push(260_000.0);
            let marked: Vec<bool> = (0..9).map(|i| i == 8).collect();
            let (inclusion, _) = exact_law(&weights, 5, &marked);
            let first = 260_000.0 / (8.0 * 939_063.0 + 260_000.0);
            const N: usize = 1 << 14;
            for (label, rows) in [("whole", whole), ("split", split)] {
                let (state, claim_id) = sw_state(&rows, &[]);
                let (eligible, base) = populations(&state, &claim_id);
                let (mut any, mut head) = (0usize, 0usize);
                for i in 0..N as u64 {
                    let seats = palw_panel_stake_race_of_v1(5, &claim_id, anchor(i), &eligible, &base, &stake).unwrap();
                    any += usize::from(seats.iter().any(|s| group.contains(&s.operator_id)));
                    head += usize::from(group.contains(&seats[0].operator_id));
                }
                assert!(within_4_sigma(any, N, inclusion[8]), "{label}: at least one seat {any} of {N}, exact {:.5}", inclusion[8]);
                assert!(within_4_sigma(head, N, first), "{label}: the first seat {head} of {N}, exact {first:.5}");
            }
        }

        // ---- build = accept ------------------------------------------------------------------------

        /// **Build = accept (SW-1, the pure part of T89).** `validate_panel_bound_v2_with_policy`
        /// recomputes through the same functions: under the stake policy the race's panel is
        /// accepted and the lottery's refused wherever they differ, and under `stake: None` the
        /// reverse — for a genesis class and for an ADR-0147 outsider-judged claim.
        #[test]
        fn build_equals_accept_and_each_draw_refuses_the_others_panel() {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (state, claim_id) = sw_state(&genesis_and_small(3), &[]);
            let check = |state: &PalwChainStateV2, claim_id: &Hash64, params: &PalwPanelParamsV2, stake_policy, lottery| {
                let slot = state.claim(claim_id).unwrap().bind_base_daa() + params.anchor_delay();
                let mut differs = 0usize;
                for i in 0..48u64 {
                    let at = anchor(i);
                    let fact = PalwAnchorFactV2 { anchor_block: at, anchor_daa: slot, predecessor_daa: slot - 1 };
                    // SW-8 (M4): the binding block IS the anchor block — `anchor(i)`'s own word.
                    let in_anchor = ctx(0x5700_0000 + i, slot, 9_000);
                    let later = ctx(9_000, slot + 1, 9_000);
                    let validate_at = |at_block: &PalwBlockContextV2, seats: &[PalwPanelSeatV2], policy| {
                        validate_panel_bound_v2_with_policy(
                            state, params, &sp, at_block, claim_id, &fact, at, seats, None, false, policy, None,
                        )
                    };
                    let validate = |seats: &[PalwPanelSeatV2], policy| validate_at(&in_anchor, seats, policy);
                    let raced = derive_panel_v2_with_policy(state, params, claim_id, at, 100, None, false, stake_policy).unwrap();
                    let drawn = derive_panel_v2_with_policy(state, params, claim_id, at, 100, None, false, lottery).unwrap();
                    assert_eq!(validate(&raced, stake_policy), Ok(()), "anchor {i}: the race's panel is accepted under its policy");
                    assert_eq!(validate(&drawn, lottery), Ok(()), "anchor {i}: and the lottery's under its own");
                    // SW-8: a block after the anchor block — inside the bind window — may carry the
                    // lottery's panel as it always could, and never the race's.
                    assert!(
                        matches!(validate_at(&later, &raced, stake_policy), Err(PalwPanelV2Error::BindOutsideWindow(_))),
                        "anchor {i}: under the stake draw a later block cannot bind the claim"
                    );
                    assert_eq!(validate_at(&later, &drawn, lottery), Ok(()), "anchor {i}: stake None keeps today's window");
                    if raced != drawn {
                        differs += 1;
                        assert_eq!(validate(&drawn, stake_policy), Err(PalwPanelV2Error::PanelMismatch), "anchor {i}");
                        assert_eq!(validate(&raced, lottery), Err(PalwPanelV2Error::PanelMismatch), "anchor {i}");
                    }
                }
                assert!(differs > 0, "the two draws must differ somewhere, or the refusals are vacuous");
            };
            check(&state, &claim_id, &five(), sw_policy(), lottery_policy());

            // The outsider-judged claim: honest network operators (11..=18) at genesis size, the
            // registrant's four Sybils (2..=5) at the floor, the registrant (9) excluded.
            let (bought, bought_claim) = t88_bought_state(&sp, |n| if (11..=18).contains(&n) { GENESIS_SEAT } else { FLOOR_SEAT });
            let independent = |policy: PalwPanelDrawPolicyV1| PalwPanelDrawPolicyV1 {
                independence: Some(PalwPanelIndependenceV1 { from_daa: 0, base_class_id: h64(1), anchor_daa: 150 }),
                ..policy
            };
            let three = PalwPanelParamsV2::new(3, 2, 4).unwrap();
            check(&bought, &bought_claim, &three, independent(sw_policy()), independent(lottery_policy()));
        }

        /// **SW-5: the outsider races by stake over the network's population.** Beside eight honest
        /// genesis-size operators, the registrant's four 130k Sybils sit as the outsider with the
        /// first-key probability `520,000 / 8,032,504 = 0.0647` (the uniform outsider ticket gave
        /// them 4/12); within 4σ over 2^12 anchors. The class's own seats follow, without the
        /// outsider's operator.
        #[test]
        fn sw5_the_outsider_is_the_smallest_stake_outsider_key() {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (state, claim_id) = t88_bought_state(&sp, |n| if (11..=18).contains(&n) { GENESIS_SEAT } else { FLOOR_SEAT });
            let policy = PalwPanelDrawPolicyV1 {
                independence: Some(PalwPanelIndependenceV1 { from_daa: 0, base_class_id: h64(1), anchor_daa: 150 }),
                ..sw_policy()
            };
            let sybil = |bond: &PalwBondKeyV2| (2..=5u64).any(|n| *bond == PalwBondKeyV2(bond_outpoint(n)));
            let honest = |bond: &PalwBondKeyV2| (11..=18u64).any(|n| *bond == PalwBondKeyV2(bond_outpoint(n)));
            const N: usize = 1 << 12;
            let mut sybil_outsiders = 0usize;
            for i in 0..N as u64 {
                let seats = derive_panel_v2_with_policy(
                    &state,
                    &PalwPanelParamsV2::new(3, 2, 4).unwrap(),
                    &claim_id,
                    anchor(i),
                    100,
                    None,
                    false,
                    policy,
                )
                .expect("the outsider and two class seats");
                assert!(sybil(&seats[0].bond) || honest(&seats[0].bond), "the outsider comes from the floor's population");
                assert!(!seats[1..].iter().any(|s| s.operator_id == seats[0].operator_id), "one operator, one seat");
                sybil_outsiders += usize::from(sybil(&seats[0].bond));
                if i < 32 {
                    // The outsider is the population's first stake-outsider entry.
                    let eligible = palw_panel_eligible_bonds_judging_v1(
                        &state,
                        &claim_id,
                        &h64(1),
                        100,
                        Some(149),
                        false,
                        None,
                        Some(sw_economy()),
                        3,
                    )
                    .unwrap()
                    .into_iter()
                    .filter(|(key, _)| **key != PalwBondKeyV2(bond_outpoint(9)))
                    .collect::<Vec<_>>();
                    let first = palw_panel_stake_entries_under_v1(
                        &claim_id,
                        anchor(i),
                        &eligible,
                        palw_panel_stake_outsider_ticket_v1,
                        &PalwPanelStakeDrawV1::V1,
                    )[0];
                    assert_eq!((seats[0].bond, seats[0].operator_id), (first.bond, first.operator_id));
                }
            }
            let p = 520_000.0 / (8.0 * 939_063.0 + 520_000.0);
            assert!(within_4_sigma(sybil_outsiders, N, p), "Sybil outsiders {sybil_outsiders} of {N}, expected {p:.4}");
        }

        // ---- T92: liveness is the lottery's -------------------------------------------------------

        /// **T92 (SW-9): `InsufficientEligibleBonds` under exactly the operator lottery's condition,
        /// over the T88 corpus** — for every draw without an outsider the two refuse together, with
        /// the same `needed` and `available`, and for an outsider-judged claim `NoOutsider` is
        /// shared; any other refusal is the same refusal. **And an operator holding 95% of the
        /// weight sits once and the panel fills.**
        #[test]
        fn t92_liveness_is_the_lotterys_and_a_95_percent_operator_sits_once() {
            let (mut short, mut stake_floor) = (0usize, 0usize);
            for fixture in t88_corpus() {
                for policy in &fixture.policies {
                    let lottery = PalwPanelDrawPolicyV1 { stake: None, ..*policy };
                    let raced = PalwPanelDrawPolicyV1 { stake: Some(PalwPanelStakeDrawV1::V1), ..*policy };
                    let claim = fixture.state.claim(&fixture.claim_id).unwrap();
                    let outsider_judged = policy.independence.filter(|i| i.governs(claim)).is_some_and(|i| {
                        crate::palw_state_v2::palw_claim_is_outsider_judged_v1(&fixture.state, claim, Some(i.from_daa))
                    });
                    for i in 0..8u64 {
                        let at = BlockHash::from_u64_word(0x9200 + i);
                        let draw = |policy| {
                            derive_panel_v2_with_policy(
                                &fixture.state,
                                &fixture.params,
                                &fixture.claim_id,
                                at,
                                100,
                                None,
                                false,
                                policy,
                            )
                        };
                        let (a, b) = (draw(lottery), draw(raced));
                        stake_floor += usize::from(matches!(b, Err(PalwPanelV2Error::InsufficientEligibleStake { .. })));
                        match (&a, &b) {
                            (Err(PalwPanelV2Error::NoOutsider(x)), other) | (other, Err(PalwPanelV2Error::NoOutsider(x))) => {
                                assert_eq!(other, &Err(PalwPanelV2Error::NoOutsider(*x)), "{}: NoOutsider together", fixture.name);
                            }
                            _ if outsider_judged => {}
                            (Err(e @ PalwPanelV2Error::InsufficientEligibleBonds { .. }), other)
                            | (other, Err(e @ PalwPanelV2Error::InsufficientEligibleBonds { .. })) => {
                                short += 1;
                                assert_eq!(other, &Err(e.clone()), "{}: InsufficientEligibleBonds together", fixture.name);
                            }
                            (Ok(_), Ok(_)) | (Ok(_), Err(PalwPanelV2Error::InsufficientEligibleStake { .. })) => {}
                            (x, y) => assert_eq!(x, y, "{}: every other refusal is the same refusal", fixture.name),
                        }
                    }
                }
            }
            assert!(short > 50, "the corpus reaches the short-panel refusal: {short}");
            eprintln!("T92: {short} short draws refused together; {stake_floor} stake-floor refusals where the lottery bound");

            // The 95% operator: 1,000,000 MSK (two bonds, 700k + 600k, capped) against ten operators
            // of 5,263 MSK (52,630 MSK in all), on a network whose floor is 1,000 MSK.
            let mut rows = vec![(20, 0x50, 700_000 * MSK), (21, 0x50, 600_000 * MSK)];
            rows.extend((0..10u64).map(|i| (30 + i, 0x60 + i, 5_263 * MSK)));
            let (state, claim_id) = sw_state(&rows, &[]);
            let low_floor = PalwPanelDrawPolicyV1 { economy: Some(t88_economy(1_000 * MSK, 500, 0)), ..sw_policy() };
            let whale = op_id(0x50);
            for i in 0..256u64 {
                let seats = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, low_floor)
                    .expect("the panel fills");
                assert_eq!(seats.len(), 5, "anchor {i}: five seats");
                let operators: std::collections::BTreeSet<Hash64> = seats.iter().map(|s| s.operator_id).collect();
                assert_eq!(operators.len(), 5, "anchor {i}: one seat an operator");
                assert_eq!(seats.iter().filter(|s| s.operator_id == whale).count(), 1, "anchor {i}: the 95% operator sits, once");
            }
        }

        // ---- T93: the one ledger decides eligibility, never a key ---------------------------------

        /// **T93, the pure part (SW-2, SW-8): changing another bond's commitments moves no eligible
        /// operator's key; only an operator's own eligibility changes the panel.** Nine operators
        /// of 2,000 sompi (weight 1 each under the 1-MSK floor) back a 600-sompi seat under a 500‰
        /// ceiling. Bond 3 produces a 10-pwu claim (reserves 50): every entry is unchanged, key for
        /// key, and so is every panel. Bond 2 produces a 100-pwu claim (reserves 500, so 500 + 600
        /// exceeds its 1,000): its operator leaves the race and every other entry keeps its key and
        /// its order — the panel is the old order without it. Eight of nine operators still clear
        /// SW-10's floor (889‰).
        #[test]
        fn t93_another_bonds_commitments_move_no_key() {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let rows: Vec<(u64, u64, u64)> = (2..=10u64).map(|b| (b, 0x20 + b, 2_000)).collect();
            let (before, claim_id) = sw_state(&rows, &[]);
            let (quiet, _) = fold_attempt(&before, &sp, 102, 3, 33, 0x23, h64(1), h64(11), 10);
            let (loaded, _) = fold_attempt(&quiet, &sp, 103, 2, 32, 0x22, h64(1), h64(11), 100);
            let economy = Some(t88_economy(1_000, 500, 0));
            let policy = PalwPanelDrawPolicyV1 { economy, stake: Some(PalwPanelStakeDrawV1::V1), ..Default::default() };
            let params = panel_params(); // three seats
            let eligible = |state: &PalwChainStateV2| {
                palw_panel_eligible_bonds_v2(state, &claim_id, 100, None, false, None, economy, 3)
                    .unwrap()
                    .into_iter()
                    .map(|(k, b)| (*k, b.clone()))
                    .collect::<Vec<_>>()
            };
            fn as_refs(list: &[(PalwBondKeyV2, PalwBondStateV2)]) -> Vec<(&PalwBondKeyV2, &PalwBondStateV2)> {
                list.iter().map(|(k, b)| (k, b)).collect()
            }
            let (e0, e1, e2) = (eligible(&before), eligible(&quiet), eligible(&loaded));
            assert_eq!((e0.len(), e1.len(), e2.len()), (9, 9, 8), "only bond 2 left the eligible list");
            assert!(
                quiet.reserved_exposure(&PalwBondKeyV2(bond_outpoint(3))) > before.reserved_exposure(&PalwBondKeyV2(bond_outpoint(3)))
            );
            let gone = op_id(0x22);
            let mut changed = 0usize;
            for i in 0..64u64 {
                let at = anchor(i);
                let k0 = palw_panel_stake_entries_v1(&claim_id, at, &as_refs(&e0), &PalwPanelStakeDrawV1::V1);
                let k1 = palw_panel_stake_entries_v1(&claim_id, at, &as_refs(&e1), &PalwPanelStakeDrawV1::V1);
                let k2 = palw_panel_stake_entries_v1(&claim_id, at, &as_refs(&e2), &PalwPanelStakeDrawV1::V1);
                assert_eq!(k0, k1, "anchor {i}: another bond's commitments move no key");
                let without: Vec<PalwPanelStakeEntryV1> = k1.iter().copied().filter(|e| e.operator_id != gone).collect();
                assert_eq!(k2, without, "anchor {i}: the ineligible operator leaves, and nobody else moves");
                let draw = |state| derive_panel_v2_with_policy(state, &params, &claim_id, at, 100, None, false, policy).unwrap();
                assert_eq!(draw(&before), draw(&quiet), "anchor {i}: the same panel");
                let after = draw(&loaded);
                let expected: Vec<PalwPanelSeatV2> =
                    without.iter().take(3).map(|e| PalwPanelSeatV2 { bond: e.bond, operator_id: e.operator_id }).collect();
                assert_eq!(after, expected, "anchor {i}: the old order without bond 2's operator");
                changed += usize::from(after != draw(&quiet));
            }
            assert!(changed > 0, "bond 2 must have sat somewhere, or its leaving is vacuous");
        }

        // ---- T94: SW-10 --------------------------------------------------------------------------

        /// **T94 (SW-10): the eligible-stake floor.** The eight genesis seats beside five idle
        /// floor-sized operators, with `k` genesis seats SATURATED — their economy headroom
        /// spent by work they produced, so the headroom filter drops them while the base keeps them.
        /// `k = 1` binds (7,223,441 of 8,162,504 MSK, 885‰); `k = 2` refuses (6,284,378: 770‰); and
        /// SW-A1's cliff, seven saturated, refuses where the lottery seats at least four idle Sybils
        /// in five seats. Eight genesis seats alone bind at exactly 875‰ (seven eligible) and refuse at
        /// six. `stake: None` never refuses for stake.
        #[test]
        fn t94_the_eligible_stake_floor() {
            let g = 939_063u128;
            for (small, saturated, expected) in [
                (5u64, vec![], Ok(())),
                (5, vec![2u64], Ok(())),
                (5, vec![2, 3], Err((6 * g + 650_000, 8 * g + 650_000))),
                (5, vec![2, 3, 4, 5, 6, 7, 8], Err((g + 650_000, 8 * g + 650_000))),
                (0, vec![2], Ok(())),
                (0, vec![2, 3], Err((6 * g, 8 * g))),
            ] {
                let (state, claim_id) = sw_state(&genesis_and_small(small), &saturated);
                let (eligible, base) = populations(&state, &claim_id);
                assert_eq!(base.len(), 8 + small as usize, "the base keeps every seat");
                assert_eq!(eligible.len(), base.len() - saturated.len(), "the headroom drops exactly the saturated seats");
                for i in 0..8u64 {
                    let raced = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, sw_policy());
                    match expected {
                        Ok(()) => assert!(raced.is_ok(), "{small} small, {saturated:?} saturated: binds, got {raced:?}"),
                        Err((eligible, base)) => assert_eq!(
                            raced,
                            Err(PalwPanelV2Error::InsufficientEligibleStake { eligible, base }),
                            "{small} small, {saturated:?} saturated: refused"
                        ),
                    }
                    // Below the fence the same state draws whenever five operators are eligible.
                    let lottery =
                        derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, lottery_policy());
                    assert!(lottery.is_ok(), "{small} small, {saturated:?} saturated: stake None never refuses for stake");
                    if saturated.len() == 7 {
                        let sybils = lottery.unwrap().iter().filter(|s| s.operator_id != op_id(0x49)).count();
                        assert!(sybils >= 4, "SW-A1: the lottery's panel is the idle Sybils'");
                    }
                }
            }
            // The rule itself: exactly at the floor binds; one short refuses; no overflow at the top.
            assert_eq!(palw_panel_stake_floor_v1(875, 1_000, 875), Ok(()));
            assert_eq!(
                palw_panel_stake_floor_v1(874, 1_000, 875),
                Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 874, base: 1_000 })
            );
            assert_eq!(palw_panel_stake_floor_v1(7 * g, 8 * g, 875), Ok(()), "7 of 8 genesis seats is exactly 875‰");
            assert_eq!(palw_panel_stake_floor_v1(0, 0, 875), Ok(()), "an empty base refuses nothing (the count refused first)");
            assert!(palw_panel_stake_floor_v1(u128::MAX / 2, u128::MAX / 2, 875).is_ok());
        }

        /// **T94 (SW-10): the class's base keeps the seats the Valid lock refuses.** The route
        /// matrix's Valid-lock filter ([`PalwPanelValidLockV1`]) is load-dependent like the
        /// economy headroom: a seat already standing behind the live locks of claims it judged is
        /// exactly the "locked honest seat" SW-A1 is about, so it leaves the eligible list and stays
        /// in the base — the base is never filtered by `lock.admits`. The test above spends seats'
        /// headroom; this one leaves every headroom whole and locks seats instead: a live slashable
        /// lock of `939,063 − 50,000` MSK on genesis seats, under a Valid lock of 100,000 MSK the
        /// idle floor-sized operators can post. Two locked seats refuse with the exact split
        /// (`6 × 939,063` against `8 × 939,063`: 750‰; with five 130k operators beside them 770‰),
        /// one locked seat binds (exactly 875‰; 885‰), and the locked seat never sits. Applying the
        /// lock to the base as well would make base equal eligible and bind every row — SW-A1's
        /// cliff back for locked seats — and fails here. `stake: None` draws every row.
        #[test]
        fn t94_the_class_base_keeps_seats_the_valid_lock_refuses() {
            use crate::palw_panel_var_v1::PalwSlashableLockV1;
            use crate::palw_state_v2::PalwStateCarriageV2;
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let g = 939_063u128;
            let lock = PalwPanelValidLockV1 {
                required: 100_000 * MSK as u128,
                now_daa: 103,
                settled_anchor_depth: None,
                window_court: sp.window_court(),
                rcore: None,
            };
            let with_lock = PalwPanelDrawPolicyV1 { valid_lock: Some(lock), ..sw_policy() };
            let lottery_with_lock = PalwPanelDrawPolicyV1 { valid_lock: Some(lock), ..lottery_policy() };
            for (small, locked, expected) in [
                (0u64, vec![], Ok(())),
                (0, vec![2u64], Ok(())),
                (0, vec![2, 3], Err((6 * g, 8 * g))),
                (5, vec![2], Ok(())),
                (5, vec![2, 3], Err((6 * g + 650_000, 8 * g + 650_000))),
            ] {
                let (unlocked, claim_id) = sw_state(&genesis_and_small(small), &[]);
                // A live lock on each seat in `locked`, standing on all of its posted collateral but
                // 50,000 MSK — below the Valid lock's 100,000, so `lock.admits` refuses it.
                let mut carriage = PalwStateCarriageV2::from_state(&unlocked);
                for b in &locked {
                    carriage.slashable_locks.insert(
                        (PalwBondKeyV2(bond_outpoint(*b)), h64(0x10C4)),
                        PalwSlashableLockV1 {
                            claim: h64(0x10C4),
                            amount: (GENESIS_SEAT - 50_000 * MSK) as u128,
                            expiry_daa: 1_000_000,
                            settled_at_final: 0,
                            // The v22 skeleton's fields (71748a97): unwritten until S-3.
                            attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                            segments: 0,
                        },
                    );
                }
                let state = carriage.into_state(&sp, None).expect("consistent");
                let (eligible, base) = populations(&state, &claim_id);
                assert_eq!(eligible.len(), 8 + small as usize, "the lock spends no headroom: every seat passes it");
                assert_eq!(base.len(), eligible.len());
                let refused: Vec<&PalwBondKeyV2> =
                    eligible.iter().map(|(key, _)| *key).filter(|key| !lock.admits(&state, key)).collect();
                let expected_refused: Vec<PalwBondKeyV2> = locked.iter().map(|b| PalwBondKeyV2(bond_outpoint(*b))).collect();
                assert_eq!(
                    refused,
                    expected_refused.iter().collect::<Vec<_>>(),
                    "{small} small: the Valid lock refuses exactly the locked seats"
                );
                for i in 0..8u64 {
                    let raced = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, with_lock);
                    match expected {
                        Ok(()) => {
                            let seats =
                                raced.unwrap_or_else(|e| panic!("{small} small, {locked:?} locked, anchor {i}: binds, got {e:?}"));
                            assert_eq!(seats.len(), 5);
                            assert!(
                                seats.iter().all(|seat| !expected_refused.contains(&seat.bond)),
                                "{small} small, {locked:?} locked, anchor {i}: a locked seat never sits"
                            );
                        }
                        Err((eligible, base)) => assert_eq!(
                            raced,
                            Err(PalwPanelV2Error::InsufficientEligibleStake { eligible, base }),
                            "{small} small, {locked:?} locked, anchor {i}: refused, the base keeping the locked seats"
                        ),
                    }
                    // Without the lock in the policy the same state binds: the refusal is the lock's.
                    assert!(derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, sw_policy()).is_ok());
                    // Below the fence the lock filters the same seats and six operators still fill five.
                    let lottery =
                        derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, lottery_with_lock);
                    let lottery = lottery
                        .unwrap_or_else(|e| panic!("{small} small, {locked:?} locked: stake None never refuses for stake: {e:?}"));
                    assert!(lottery.iter().all(|seat| !expected_refused.contains(&seat.bond)));
                }
            }
        }

        /// **The integration's rule (S-3 × SW): past `palw_rcore_plus` the one ledger decides WHETHER a
        /// bond is drawn, posted stake decides HOW OFTEN.** Under S-3's one-ledger seat filter
        /// (`PalwPanelValidLockV1::rcore`: `committed + eligibility ≤ collateral × 500‰`) the economy's
        /// headroom is asked of no list, so the eligible list before the lock IS SW-10's base; the
        /// filter then drops exactly the seats whose one-ledger commitments leave no room — a seat
        /// saturated by its own claim's reservation, or standing behind a live lock — and the base keeps
        /// them. So T94's two tables hold unchanged under the filter that replaces the route matrix's
        /// posted-collateral lock: one loaded genesis seat binds (7/8, and 885‰ beside five 130k
        /// operators), two refuse with the exact split, and a loaded seat never sits. The filter's
        /// `eligibility` is 50,000 MSK, which a 130k operator's 65,000 MSK ceiling covers.
        #[test]
        fn t94_under_the_one_ledger_filter_free_stake_decides_whether_and_posted_stake_how_often() {
            use crate::palw_panel_var_v1::PalwSlashableLockV1;
            use crate::palw_state_v2::PalwStateCarriageV2;
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let g = 939_063u128;
            let filter = PalwPanelValidLockV1 {
                // Unread under the one-ledger filter (S-3): a lock no bond could post shows it.
                required: u128::MAX,
                now_daa: 103,
                settled_anchor_depth: None,
                window_court: sp.window_court(),
                rcore: Some(PalwRcoreSeatFilterV1 { eligibility: 50_000 * MSK as u128, ceiling_permille: 500 }),
            };
            let policy = PalwPanelDrawPolicyV1 { valid_lock: Some(filter), ..sw_policy() };
            let key = |b: u64| PalwBondKeyV2(bond_outpoint(b));
            // Loaded by a live lock (T94's second table) or by its own claim's reservation (T94's
            // first): the one ledger reads both.
            let locked = |small: u64, loaded: &[u64]| {
                let (unlocked, claim_id) = sw_state(&genesis_and_small(small), &[]);
                let mut carriage = PalwStateCarriageV2::from_state(&unlocked);
                for b in loaded {
                    carriage.slashable_locks.insert(
                        (key(*b), h64(0x10C4)),
                        PalwSlashableLockV1 {
                            claim: h64(0x10C4),
                            amount: (GENESIS_SEAT - 50_000 * MSK) as u128,
                            expiry_daa: 1_000_000,
                            settled_at_final: 0,
                            attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                            segments: 0,
                        },
                    );
                }
                (carriage.into_state(&sp, None).expect("consistent"), claim_id)
            };
            let saturated = |small: u64, loaded: &[u64]| sw_state(&genesis_and_small(small), loaded);
            for (how, load) in [
                ("locked", &locked as &dyn Fn(u64, &[u64]) -> (PalwChainStateV2, Hash64)),
                ("saturated", &saturated as &dyn Fn(u64, &[u64]) -> (PalwChainStateV2, Hash64)),
            ] {
                for (small, loaded, expected) in [
                    (0u64, vec![], Ok(())),
                    (0, vec![2u64], Ok(())),
                    (0, vec![2, 3], Err((6 * g, 8 * g))),
                    (5, vec![2], Ok(())),
                    (5, vec![2, 3], Err((6 * g + 650_000, 8 * g + 650_000))),
                ] {
                    let label = format!("{how}: {small} small, {loaded:?} loaded");
                    let (state, claim_id) = load(small, &loaded);
                    let before_lock = palw_panel_eligible_bonds_judging_v2(
                        &state,
                        &claim_id,
                        &h64(1),
                        100,
                        None,
                        false,
                        None,
                        Some(sw_economy()),
                        5,
                        false,
                    )
                    .unwrap();
                    let (_, base) = populations(&state, &claim_id);
                    assert_eq!(
                        before_lock.iter().map(|(k, _)| **k).collect::<Vec<_>>(),
                        base.iter().map(|(k, _)| **k).collect::<Vec<_>>(),
                        "{label}: without the economy's headroom the list before the lock is SW-10's base"
                    );
                    assert_eq!(base.len(), 8 + small as usize, "{label}: the base keeps every loaded seat");
                    let refused: Vec<PalwBondKeyV2> = base.iter().map(|(k, _)| **k).filter(|k| !filter.admits(&state, k)).collect();
                    assert_eq!(
                        refused,
                        loaded.iter().map(|b| key(*b)).collect::<Vec<_>>(),
                        "{label}: the filter drops the loaded seats"
                    );
                    for i in 0..8u64 {
                        let raced = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, policy);
                        match expected {
                            Ok(()) => {
                                let seats = raced.unwrap_or_else(|e| panic!("{label}, anchor {i}: binds, got {e:?}"));
                                assert_eq!(seats.len(), 5);
                                assert!(
                                    seats.iter().all(|seat| !refused.contains(&seat.bond)),
                                    "{label}, anchor {i}: a loaded seat never sits"
                                );
                            }
                            Err((eligible, base)) => assert_eq!(
                                raced,
                                Err(PalwPanelV2Error::InsufficientEligibleStake { eligible, base }),
                                "{label}, anchor {i}: refused, the base keeping the loaded seats"
                            ),
                        }
                    }
                }
            }
        }

        /// **SW-10 for the outsider, and the refusal order.** The outsider's population is the
        /// floor's minus the registrant: honest operators of 130k and the registrant's Sybils of
        /// 939k. A Valid lock of 500,000 MSK drops every honest bond (posted below it), which leaves
        /// the outsider's eligible weight at `4 × 939,063` of a base of `4 × 939,063 + 8 × 130,000`;
        /// the executor (bond 1, a 939k floor seat) adds its own term to both sides (M4 review
        /// finding 1), so the floor compares `5 × 939,063` with `5 × 939,063 + 8 × 130,000` (818‰):
        /// refused, where the lottery seats an outsider. And the class's operator count is checked
        /// before either floor, so a short class is `InsufficientEligibleBonds` whatever the floors
        /// say.
        #[test]
        fn sw10_the_outsider_floor_and_the_refusal_order() {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (state, claim_id) = t88_bought_state(&sp, |n| if (11..=18).contains(&n) { FLOOR_SEAT } else { GENESIS_SEAT });
            let lock = PalwPanelValidLockV1 {
                required: 500_000 * MSK as u128,
                now_daa: 103,
                settled_anchor_depth: None,
                window_court: sp.window_court(),
                rcore: None,
            };
            let independence = PalwPanelIndependenceV1 { from_daa: 0, base_class_id: h64(1), anchor_daa: 150 };
            let raced = PalwPanelDrawPolicyV1 { independence: Some(independence), valid_lock: Some(lock), ..sw_policy() };
            let lottery = PalwPanelDrawPolicyV1 { stake: None, ..raced };
            let g = 939_063u128;
            for i in 0..8u64 {
                let three = PalwPanelParamsV2::new(3, 2, 4).unwrap();
                assert_eq!(
                    derive_panel_v2_with_policy(&state, &three, &claim_id, anchor(i), 100, None, false, raced),
                    Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 5 * g, base: 5 * g + 8 * 130_000 }),
                    "anchor {i}: the outsider's floor refuses, the executor's term on both sides"
                );
                assert!(derive_panel_v2_with_policy(&state, &three, &claim_id, anchor(i), 100, None, false, lottery).is_ok());
                assert_eq!(
                    palw_panel_outsider_seat_v1(&state, &three, &claim_id, anchor(i), 100, Some(149), false, &raced, &independence),
                    Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 5 * g, base: 5 * g + 8 * 130_000 }),
                    "the public outsider seat applies its own floor"
                );
            }
            // The order inside the race: a short class is refused by count before any floor.
            let (plain, plain_claim) = sw_state(&genesis_and_small(0), &[]);
            let (eligible, base) = populations(&plain, &plain_claim);
            let stake = PalwPanelStakeDrawV1::V1;
            assert_eq!(
                palw_panel_stake_race_with_v1(9, &plain_claim, anchor(0), &eligible, &base, 0, &stake, Some((0, 1))),
                Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 9, available: 8 })
            );
            assert_eq!(
                palw_panel_stake_race_with_v1(5, &plain_claim, anchor(0), &eligible, &base, 0, &stake, Some((0, 1))),
                Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 0, base: 1 })
            );
        }

        // ---- SW-9: the room's effective ready count ------------------------------------------------

        /// **SW-9 / T91's arithmetic: `ready_eff = min(ready, max(seat_count, ⌊ΣW / W_max⌋))` over
        /// capped weights.** The ADR's three examples: the eight genesis seats give 8; with forty
        /// 130k operators beside them, 13 of 48; and one 20M operator beside the eight genesis seats
        /// — 5 in §3.14's pre-cap text — gives **8** once its weight is capped at 1,000,000
        /// (`⌊8,512,504 / 1,000,000⌋ = 8`; `v31_review_numbers.out`: "one 20M operator + 8 genesis,
        /// capped: ready_eff 8"). SW-A6's residual: a class held by forty 130k operators counts 40,
        /// and one ready operator at the cap cuts it to 6.
        #[test]
        fn sw9_ready_eff_over_capped_weights() {
            let cap = PalwPanelStakeDrawV1::V1.weight_cap_msk;
            let w = |msk: u128| palw_draw_operator_weight_msk_v1(msk, cap);
            let genesis = vec![w(939_063); 8];
            assert_eq!(palw_panel_ready_eff_v1(&genesis, 5), 8);
            let mut wide = genesis.clone();
            wide.extend(vec![w(130_000); 40]);
            assert_eq!(palw_panel_ready_eff_v1(&wide, 5), 13, "8 genesis + 40 × 130k: 13 of 48");
            let mut whale = vec![w(20_000_000)];
            whale.extend(genesis.clone());
            assert_eq!(whale[0], 1_000_000, "the 20M operator weighs the cap");
            assert_eq!(palw_panel_ready_eff_v1(&whale, 5), 8, "one 20M operator + 8 genesis: 8 capped (5 uncapped)");
            assert_eq!(
                palw_panel_ready_eff_v1(&[20_000_000, 939_063, 939_063, 939_063, 939_063, 939_063, 939_063, 939_063, 939_063], 5),
                5,
                "uncapped, as §3.14's text had it"
            );
            let mut wide_whale = wide.clone();
            wide_whale.push(w(20_000_000));
            assert_eq!(palw_panel_ready_eff_v1(&wide_whale, 5), 13, "SW-A6: the cap keeps that lever at 13");
            let smalls = vec![w(130_000); 40];
            assert_eq!(palw_panel_ready_eff_v1(&smalls, 5), 40);
            let mut smalls_whale = smalls.clone();
            smalls_whale.push(w(1_000_000));
            assert_eq!(palw_panel_ready_eff_v1(&smalls_whale, 5), 6, "SW-A6's residual: 40 → 6");
            assert_eq!(palw_panel_ready_eff_v1(&[], 5), 0, "no ready operator");
            assert_eq!(palw_panel_ready_eff_v1(&[w(130_000); 3], 5), 3, "never more than the ready operators");
        }

        // ---- M4: the integration (ADR-0152 §3.14 SW-1, SW-8, SW-9, SW-10 at the fold) ----------

        /// **ADR-0152 M4's core half** — the stake draw where the processor wires it: one state per
        /// block (SW-8), bind only in the anchor block (SW-8), the room's `ready_eff` (SW-9), and the
        /// fold's side of SW-10's halt. The processor half (the resolver, the chain's derivation on a
        /// real testnet-12 chain) is `consensus/src/pipeline/virtual_processor/tests/
        /// t12_stake_draw_integration.rs`; the room through the fold and op 186 on testnet-12's rows is
        /// `consensus/core/tests/adr0152_sw9_ready_eff_room.rs`.
        mod m4 {
            use super::*;
            use crate::palw_state_v2::{
                PalwBondStatusV2, PalwClaimPhaseV2, PalwStateCarriageV2, PalwTransitionExtrasV1, PalwVoidReasonV2,
                apply_palw_transition_v2_with_extras, palw_v2_apply_one_object_v1, palw_v2_pre_object_base_v1, revert_delta_v2,
            };

            fn key(b: u64) -> PalwBondKeyV2 {
                PalwBondKeyV2(bond_outpoint(b))
            }

            /// The fold's extras past the panel economy (seats go on duty and hold their exposure),
            /// everything else dormant — enough for a binding to load the seats it seats.
            fn duty_extras() -> PalwTransitionExtrasV1 {
                PalwTransitionExtrasV1 { panel_economy_active: true, ..Default::default() }
            }

            /// A block that is `word`'s own anchor at `daa`: the anchor fact every claim anchored there
            /// reads (its predecessor below both slots), and the chain point that binds in it.
            fn anchored_at(word: u64, daa: u64, predecessor_daa: u64) -> (PalwAnchorFactV2, PalwBlockContextV2) {
                (
                    PalwAnchorFactV2 { anchor_block: BlockHash::from_u64_word(word), anchor_daa: daa, predecessor_daa },
                    ctx(word, daa, daa),
                )
            }

            #[allow(clippy::too_many_arguments)]
            fn validate(
                state: &PalwChainStateV2,
                sp: &PalwStateParamsV2,
                point: &PalwBlockContextV2,
                claim_id: &Hash64,
                fact: &PalwAnchorFactV2,
                proposed_anchor: Hash64,
                seats: &[PalwPanelSeatV2],
                policy: PalwPanelDrawPolicyV1,
            ) -> Result<(), PalwPanelV2Error> {
                validate_panel_bound_v2_with_policy(
                    state,
                    &five(),
                    sp,
                    point,
                    claim_id,
                    fact,
                    proposed_anchor,
                    seats,
                    None,
                    false,
                    policy,
                    None,
                )
            }

            fn draw(
                state: &PalwChainStateV2,
                claim_id: &Hash64,
                at: BlockHash,
                policy: PalwPanelDrawPolicyV1,
            ) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
                derive_panel_v2_with_policy(state, &five(), claim_id, at, 100, None, false, policy)
            }

            /// **T91 through the counting function (SW-9):** `palw_panel_ready_eff_of_bonds_v1` over
            /// the bonds of a registry reproduces §3.14's examples from the bonds' own posted
            /// collateral, grouped per operator: 8 genesis seats → 8; with forty 130k operators, 13 of
            /// 48; one 20M operator more, still 13 (capped); one 20M operator beside the genesis seats,
            /// 8; forty 130k alone, 40, and one at the cap beside them, 6. One operator's two bonds are
            /// ONE ready operator whose weight sums before the cap.
            #[test]
            fn t91_ready_eff_counts_ready_operators_over_capped_weights() {
                let mut rows = genesis_and_small(40);
                rows.push((60, 0x90, 20_000_000 * MSK));
                rows.push((61, 0x91, 1_000_000 * MSK));
                rows.push((62, 0x92, 600_000 * MSK));
                rows.push((63, 0x92, 600_000 * MSK));
                let (state, _) = sw_state(&rows, &[]);
                let eff = |bonds: &[u64]| {
                    palw_panel_ready_eff_of_bonds_v1(
                        bonds.iter().map(|b| state.bond(&key(*b)).expect("registered")),
                        5,
                        &PalwPanelStakeDrawV1::V1,
                    )
                };
                let genesis: Vec<u64> = (2..=9).collect();
                let smalls: Vec<u64> = (10..50).collect();
                let with = |a: &[u64], b: &[u64]| [a, b].concat();
                assert_eq!(eff(&genesis), 8, "8 genesis seats");
                assert_eq!(eff(&with(&genesis, &smalls)), 13, "8 genesis + 40 × 130k: 13 of 48");
                assert_eq!(eff(&with(&with(&genesis, &smalls), &[60])), 13, "one ready 20M operator more: still 13 (capped)");
                assert_eq!(eff(&with(&[60], &genesis)), 8, "one 20M operator + 8 genesis: 8 (5 uncapped)");
                assert_eq!(eff(&smalls), 40, "40 × 130k");
                assert_eq!(eff(&with(&smalls, &[61])), 6, "SW-A6's residual: one operator at the cap cuts 40 to 6");
                assert_eq!(
                    eff(&with(&smalls, &[62, 63])),
                    6,
                    "one operator's two 600k bonds: one ready operator, 1.2M summed and capped at 1M"
                );
                assert_eq!(eff(&[]), 0);
            }

            /// **T89 (SW-8): two claims bound in one block — the second sees the first's duties.**
            /// Nine operators of equal weight beside the executor; one of them ("thin", bond 2) has
            /// economy headroom for exactly ONE seat of these claims. Both claims anchor at the same
            /// block. The processor derives them the way the acceptance walk validates them: on the
            /// block's pre-object base, advanced by the first binding in claim-id order. Wherever thin
            /// sits on the first panel, it is not eligible for the second (its duty took its
            /// headroom), and:
            ///
            /// * build = accept: the second panel derived on the advanced state is what the gate
            ///   accepts there (and the first on the base);
            /// * the parent-state derivation the chain ran before SW-8 draws thin onto the second panel
            ///   at some anchors, and the gate refuses that panel on the state it actually reads
            ///   (`PanelMismatch`) — the drop that used to send the claim to a later block's retry;
            /// * fold: the block folding both bindings stores exactly the two derived panels.
            #[test]
            fn t89_two_claims_in_one_block_the_second_sees_the_first_s_duties() {
                let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
                let mut rows: Vec<(u64, u64, u64)> = (3..=10u64).map(|b| (b, 0x20 + b, 1_000_000)).collect();
                rows.insert(0, (2, 0x22, 2_000));
                let (s0, first_claim) = sw_state(&rows, &[]);
                let (s0, second_claim) = fold_attempt(&s0, &sp, 102, 1, 7, 0x21, h64(1), h64(11), 40);
                assert_ne!(first_claim, second_claim, "two claims");
                let (a, b) = if first_claim < second_claim { (first_claim, second_claim) } else { (second_claim, first_claim) };
                let economy = Some(t88_economy(1_000, 500, 0));
                let policy = PalwPanelDrawPolicyV1 { economy, stake: Some(PalwPanelStakeDrawV1::V1), ..Default::default() };
                let thin = key(2);
                let (mut thin_first, mut legacy_refused) = (0usize, 0usize);
                for i in 0..48u64 {
                    let word = 0x5A00_0000 + i;
                    // Both claims' slots (105, 106) lie behind one anchor at 106.
                    let (fact, point) = anchored_at(word, 106, 104);
                    let at = fact.anchor_block;
                    let base = palw_v2_pre_object_base_v1(&s0, &sp, &point, false, false, false, false, &duty_extras()).unwrap();
                    let p1 = draw(&base, &a, at, policy).unwrap();
                    assert_eq!(validate(&base, &sp, &point, &a, &fact, at, &p1, policy), Ok(()), "anchor {i}: build = accept, first");
                    let first = PalwConsensusObjectV2::PanelBound { claim: a, anchor: at, seats: p1.clone() };
                    let advanced = palw_v2_apply_one_object_v1(&base, &sp, &point, &first, false, false, false, false, &duty_extras())
                        .expect("the first binding folds");
                    let p2 = draw(&advanced, &b, at, policy).unwrap();
                    assert_eq!(
                        validate(&advanced, &sp, &point, &b, &fact, at, &p2, policy),
                        Ok(()),
                        "anchor {i}: build = accept, second, on the advanced state"
                    );
                    if p1.iter().any(|s| s.bond == thin) {
                        thin_first += 1;
                        assert!(
                            advanced.reserved_exposure(&thin) > base.reserved_exposure(&thin),
                            "the first panel's duty loads thin"
                        );
                        assert!(!p2.iter().any(|s| s.bond == thin), "anchor {i}: thin has no headroom left for the second");
                        let legacy = draw(&base, &b, at, policy).unwrap();
                        if legacy != p2 {
                            legacy_refused += 1;
                            assert_eq!(
                                validate(&advanced, &sp, &point, &b, &fact, at, &legacy, policy),
                                Err(PalwPanelV2Error::PanelMismatch),
                                "anchor {i}: the parent-state panel is refused on the state acceptance reads"
                            );
                        }
                    }
                    // Fold: the block folding both stores both derived panels.
                    let second = PalwConsensusObjectV2::PanelBound { claim: b, anchor: at, seats: p2.clone() };
                    let (folded, _) = apply_palw_transition_v2_with_extras(
                        &s0,
                        &sp,
                        &point,
                        &[first, second],
                        None,
                        false,
                        false,
                        false,
                        false,
                        &duty_extras(),
                    )
                    .expect("the block folds both bindings");
                    assert_eq!(folded.panel(&a).map(|p| &p.seats), Some(&p1), "anchor {i}: fold = build, first");
                    assert_eq!(folded.panel(&b).map(|p| &p.seats), Some(&p2), "anchor {i}: fold = build, second");
                }
                assert!(
                    thin_first > 0 && legacy_refused > 0,
                    "the premise must occur: thin first {thin_first}, legacy refused {legacy_refused}"
                );
            }

            /// **T89 (SW-8): the Valid-lock filter excludes a bond, build = accept = fold.** A live
            /// lock leaves genesis seat 2 unable to post the bind's Valid lock; the draw skips it, the
            /// gate accepts the panel under the same policy, and the fold binds it. (T94's twin in the
            /// pure half shows the base keeps the seat for SW-10.)
            #[test]
            fn t89_the_valid_lock_filter_excludes_a_bond_build_accept_fold() {
                use crate::palw_panel_var_v1::PalwSlashableLockV1;
                let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
                let (unlocked, claim_id) = sw_state(&genesis_and_small(5), &[]);
                let mut carriage = PalwStateCarriageV2::from_state(&unlocked);
                carriage.slashable_locks.insert(
                    (key(2), h64(0x10C4)),
                    PalwSlashableLockV1 {
                        claim: h64(0x10C4),
                        amount: (GENESIS_SEAT - 50_000 * MSK) as u128,
                        expiry_daa: 1_000_000,
                        settled_at_final: 0,
                        attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                        segments: 0,
                    },
                );
                let state = carriage.into_state(&sp, None).expect("consistent");
                let lock = PalwPanelValidLockV1 {
                    required: 100_000 * MSK as u128,
                    now_daa: 103,
                    settled_anchor_depth: None,
                    window_court: sp.window_court(),
                    rcore: None,
                };
                assert!(!lock.admits(&state, &key(2)) && lock.admits(&state, &key(3)));
                let policy = PalwPanelDrawPolicyV1 { valid_lock: Some(lock), ..sw_policy() };
                let slot = state.claim(&claim_id).unwrap().bind_base_daa() + five().anchor_delay();
                for i in 0..24u64 {
                    let (fact, point) = anchored_at(0x5B00_0000 + i, slot, slot - 1);
                    let seats = draw(&state, &claim_id, fact.anchor_block, policy).expect("7 of 8 genesis weight stays eligible");
                    assert!(!seats.iter().any(|s| s.bond == key(2)), "anchor {i}: the locked seat never sits");
                    assert_eq!(validate(&state, &sp, &point, &claim_id, &fact, fact.anchor_block, &seats, policy), Ok(()));
                    let object =
                        PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: fact.anchor_block, seats: seats.clone() };
                    let (folded, _) = apply_palw_transition_v2_with_extras(
                        &state,
                        &sp,
                        &point,
                        &[object],
                        None,
                        false,
                        false,
                        false,
                        false,
                        &duty_extras(),
                    )
                    .unwrap();
                    assert_eq!(folded.panel(&claim_id).map(|p| &p.seats), Some(&seats), "anchor {i}: fold = build");
                }
            }

            /// **T89 (SW-8): a redraw binds in the REDRAW's anchor block, and only there.** A bound
            /// panel whose receipt window closes with no conclusion is revived once (`rebound_daa`),
            /// and its second panel anchors on the sweep. The gate accepts the second panel at the
            /// redraw's anchor block, refuses it at any later block inside the redraw's bind window
            /// (`BindOutsideWindow`, SW-8) and at the FIRST panel's anchor (whose slot the claim has
            /// left); under `stake: None` the later block still binds, as it always could.
            #[test]
            fn t89_a_redraw_binds_only_in_the_redraw_s_anchor_block() {
                let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
                let (s0, claim_id) = sw_state(&genesis_and_small(5), &[]);
                let slot = s0.claim(&claim_id).unwrap().bind_base_daa() + five().anchor_delay();
                let (fact1, point1) = anchored_at(0x5C00_0001, slot, slot - 1);
                let p1 = draw(&s0, &claim_id, fact1.anchor_block, sw_policy()).unwrap();
                let (s1, _) = apply_palw_transition_v2_with_extras(
                    &s0,
                    &sp,
                    &point1,
                    &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: fact1.anchor_block, seats: p1 }],
                    None,
                    false,
                    false,
                    false,
                    false,
                    &duty_extras(),
                )
                .unwrap();
                // The receipt window closes with no conclusion: the sweep revives the claim once.
                let sweep = slot + sp.window_receipt() + 1;
                let (s2, _) = apply_palw_transition_v2_with_extras(
                    &s1,
                    &sp,
                    &ctx(0x5C00_0002, sweep, sweep),
                    &[],
                    None,
                    false,
                    false,
                    false,
                    false,
                    &duty_extras(),
                )
                .unwrap();
                let revived = s2.claim(&claim_id).unwrap();
                assert!(matches!(revived.phase, PalwClaimPhaseV2::Provisional), "{:?}", revived.phase);
                assert_eq!(revived.rebound_daa, Some(sweep));
                let slot2 = sweep + five().anchor_delay();
                let (fact2, point2) = anchored_at(0x5C00_0003, slot2, slot2 - 1);
                let p2 = draw(&s2, &claim_id, fact2.anchor_block, sw_policy()).unwrap();
                assert_eq!(validate(&s2, &sp, &point2, &claim_id, &fact2, fact2.anchor_block, &p2, sw_policy()), Ok(()));
                let later = ctx(0x5C00_0004, slot2 + 1, slot2 + 1);
                assert!(matches!(
                    validate(&s2, &sp, &later, &claim_id, &fact2, fact2.anchor_block, &p2, sw_policy()),
                    Err(PalwPanelV2Error::BindOutsideWindow(_))
                ));
                assert!(
                    matches!(
                        validate(&s2, &sp, &point1, &claim_id, &fact1, fact1.anchor_block, &p2, sw_policy()),
                        Err(PalwPanelV2Error::AnchorMismatch(_) | PalwPanelV2Error::BindOutsideWindow(_))
                    ),
                    "the first panel's anchor is not the redraw's"
                );
                let lottery = draw(&s2, &claim_id, fact2.anchor_block, lottery_policy()).unwrap();
                assert_eq!(validate(&s2, &sp, &later, &claim_id, &fact2, fact2.anchor_block, &lottery, lottery_policy()), Ok(()));
            }

            /// **T89 (SW-8): a reorg across the bind.** The bind folded at anchor A reverts to the
            /// exact parent state; the competing chain's anchor B (the first block at the same slot on
            /// that chain) draws on the same one state, and its gate refuses A's panel (the object
            /// names A) and accepts B's. Nothing about A's draw survives on B's chain.
            #[test]
            fn t89_a_reorg_across_the_bind_re_derives_on_the_new_chain() {
                let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
                let (s0, claim_id) = sw_state(&genesis_and_small(5), &[]);
                let slot = s0.claim(&claim_id).unwrap().bind_base_daa() + five().anchor_delay();
                let (fact_a, point_a) = anchored_at(0x5D00_000A, slot, slot - 1);
                let (fact_b, point_b) = anchored_at(0x5D00_000B, slot, slot - 1);
                let pa = draw(&s0, &claim_id, fact_a.anchor_block, sw_policy()).unwrap();
                let bound_a = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: fact_a.anchor_block, seats: pa.clone() };
                let (sa, delta) = apply_palw_transition_v2_with_extras(
                    &s0,
                    &sp,
                    &point_a,
                    &[bound_a],
                    None,
                    false,
                    false,
                    false,
                    false,
                    &duty_extras(),
                )
                .unwrap();
                assert!(matches!(sa.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }));
                let back = revert_delta_v2(&sa, &delta, &sp).unwrap();
                assert_eq!(back.state_root(), s0.state_root(), "the reorg restores the parent state exactly");
                let pb = draw(&back, &claim_id, fact_b.anchor_block, sw_policy()).unwrap();
                assert_eq!(pb, draw(&s0, &claim_id, fact_b.anchor_block, sw_policy()).unwrap());
                assert!(matches!(
                    validate(&back, &sp, &point_b, &claim_id, &fact_b, fact_a.anchor_block, &pa, sw_policy()),
                    Err(PalwPanelV2Error::AnchorMismatch(_))
                ));
                assert_eq!(validate(&back, &sp, &point_b, &claim_id, &fact_b, fact_b.anchor_block, &pb, sw_policy()), Ok(()));
                let bound_b = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: fact_b.anchor_block, seats: pb.clone() };
                let (sb, _) = apply_palw_transition_v2_with_extras(
                    &back,
                    &sp,
                    &point_b,
                    &[bound_b],
                    None,
                    false,
                    false,
                    false,
                    false,
                    &duty_extras(),
                )
                .unwrap();
                assert_eq!(sb.panel(&claim_id).map(|p| &p.seats), Some(&pb));
            }

            /// **T93 at the fold (SW-8): nothing after the anchor moves a bound panel, and nothing
            /// after it can bind an unbound one.** A panel is derived and folded in its anchor block.
            /// Then, on the chain after it: the attacker's own claims (a Sybil's attempts), a carried
            /// licence's lock on a seat, and a slash of an unrelated bond. The stored panel never
            /// changes, and every operator's key (`L`, `W`) but the slashed one's is what it was — the
            /// one ledger moves eligibility, a slash moves only its own bond's weight. And for a claim
            /// whose draw did NOT bind in its anchor block, the attacker retiring its own seated
            /// Sybil afterwards buys nothing: the panel re-derived on the later state differs, and
            /// the gate refuses it at every later block (`BindOutsideWindow`) — no retry exists.
            #[test]
            fn t93_nothing_after_the_anchor_moves_the_panel() {
                use crate::palw_panel_var_v1::PalwSlashableLockV1;
                let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
                let (s0, claim_id) = sw_state(&genesis_and_small(5), &[]);
                let slot = s0.claim(&claim_id).unwrap().bind_base_daa() + five().anchor_delay();
                let (fact, point) = anchored_at(0x5E00_0001, slot, slot - 1);
                let at = fact.anchor_block;
                let panel = draw(&s0, &claim_id, at, sw_policy()).unwrap();
                let (bound, _) = apply_palw_transition_v2_with_extras(
                    &s0,
                    &sp,
                    &point,
                    &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: at, seats: panel.clone() }],
                    None,
                    false,
                    false,
                    false,
                    false,
                    &duty_extras(),
                )
                .unwrap();
                let keys_of = |state: &PalwChainStateV2| {
                    let (eligible, _) = populations(state, &claim_id);
                    palw_panel_stake_entries_v1(&claim_id, at, &eligible, &PalwPanelStakeDrawV1::V1)
                        .into_iter()
                        .map(|e| (e.operator_id, (e.neg_log2_q64, e.weight_msk)))
                        .collect::<std::collections::BTreeMap<_, _>>()
                };
                let keys0 = keys_of(&s0);
                // The attacker's own claims: a floor-sized Sybil (bond 10, operator 0x4A) produces.
                let (after_claims, _) = fold_attempt(&bound, &sp, slot + 1, 10, 40, 0x4A, h64(1), h64(11), 40);
                // A carried licence's Valid lock on the first seat.
                let seat = panel[0].bond;
                let mut carriage = PalwStateCarriageV2::from_state(&after_claims);
                carriage.slashable_locks.insert(
                    (seat, h64(0x93A1)),
                    PalwSlashableLockV1 {
                        claim: h64(0x93A1),
                        amount: 10_000 * MSK as u128,
                        expiry_daa: 1_000_000,
                        settled_at_final: 0,
                        attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                        segments: 0,
                    },
                );
                // A slash of an unrelated bond: one that does not sit on this panel.
                let unrelated =
                    (2..=14u64).map(key).find(|k| !panel.iter().any(|s| s.bond == *k)).expect("thirteen bonds, five seats");
                let slashed_operator = carriage.bonds.get(&unrelated).unwrap().operator_id;
                {
                    let bond = carriage.bonds.get_mut(&unrelated).unwrap();
                    bond.collateral -= 400_000 * MSK;
                    bond.slashed += 400_000 * MSK;
                }
                let later = carriage.into_state(&sp, None).expect("consistent");
                for (label, state) in [("the attacker's claims", &after_claims), ("a lock and a slash", &later)] {
                    assert_eq!(state.panel(&claim_id).map(|p| &p.seats), Some(&panel), "{label}: the bound panel stands");
                    assert!(matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "{label}");
                }
                let keys1 = keys_of(&later);
                for (operator, k) in &keys0 {
                    if *operator == slashed_operator {
                        assert_eq!(keys1[operator].0, k.0, "the slashed operator's L is its seed's");
                        continue;
                    }
                    assert_eq!(keys1.get(operator), Some(k), "no other operator's key moved");
                }

                // The retry path: a draw that did not bind in its anchor block. The attacker retires
                // its own seated Sybil after seeing the anchor; the re-derived panel on the later
                // state differs, and no later block may carry it.
                let mut retry_blocked = 0usize;
                for i in 0..32u64 {
                    let (fact, point) = anchored_at(0x5E10_0000 + i, slot, slot - 1);
                    let first = draw(&s0, &claim_id, fact.anchor_block, sw_policy()).unwrap();
                    let Some(own) = first.iter().find(|s| (10..=14u64).map(key).any(|k| k == s.bond)) else { continue };
                    let mut c = PalwStateCarriageV2::from_state(&s0);
                    c.bonds.get_mut(&own.bond).unwrap().status = PalwBondStatusV2::Retiring { since_daa: slot, settled_at_since: 0 };
                    let retired = c.into_state(&sp, None).expect("consistent");
                    let redrawn = draw(&retired, &claim_id, fact.anchor_block, sw_policy()).unwrap();
                    assert_ne!(redrawn, first, "anchor {i}: the retirement moved the panel it would re-derive");
                    let after = ctx(0x5E20_0000 + i, slot + 1, slot + 1);
                    assert!(
                        matches!(
                            validate(&retired, &sp, &after, &claim_id, &fact, fact.anchor_block, &redrawn, sw_policy()),
                            Err(PalwPanelV2Error::BindOutsideWindow(_))
                        ),
                        "anchor {i}: a later block cannot carry the retry"
                    );
                    // The anchor block itself reads the state before the retirement.
                    assert_eq!(validate(&s0, &sp, &point, &claim_id, &fact, fact.anchor_block, &first, sw_policy()), Ok(()));
                    retry_blocked += 1;
                }
                assert!(retry_blocked > 0, "a Sybil must have sat somewhere, or the retry case is vacuous");
            }

            /// **T94 at the fold (SW-10 + SW-8): `InsufficientEligibleStake` halts binding, and the
            /// claim voids `BindTimeout` without forfeit — AT its anchor block.** Two of the eight
            /// genesis seats saturated (6/8, 750‰): the draw refuses in the anchor block, and the gate
            /// refuses ANY proposed panel there with the same error (build = accept on the refusal —
            /// the lottery's panel included). Nothing binds, and past `palw_rcore_plus` the anchor
            /// block's own fold voids the claim `BindTimeout` at its step 4c (M4 review finding 3,
            /// DL-1's exact void DAA: the anchor block's), not 600 DAA later at the window: no bond is
            /// slashed, no collateral moves, the executor's reservation is released, and the deadline
            /// index, the carriage rebuild and a revert all agree with it. A block before the slot, or
            /// one past it that may not anchor (`sw8_anchor_delay` `None`: a heartbeat), voids nothing.
            /// The fence-off twin (`None` everywhere) keeps the claim until the bind window's sweep.
            /// `stake: None` binds the same claim.
            #[test]
            fn t94_a_refused_draw_binds_nothing_and_the_claim_voids_bind_timeout_without_forfeit() {
                let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
                let (s0, claim_id) = sw_state(&genesis_and_small(0), &[2, 3]);
                let claim = s0.claim(&claim_id).unwrap().clone();
                let slot = claim.bind_base_daa() + five().anchor_delay();
                let (fact, point) = anchored_at(0x5F00_0001, slot, slot - 1);
                let g = 939_063u128;
                let refusal = PalwPanelV2Error::InsufficientEligibleStake { eligible: 6 * g, base: 8 * g };
                assert_eq!(draw(&s0, &claim_id, fact.anchor_block, sw_policy()), Err(refusal.clone()));
                let lottery = draw(&s0, &claim_id, fact.anchor_block, lottery_policy()).expect("stake None never refuses for stake");
                assert_eq!(
                    validate(&s0, &sp, &point, &claim_id, &fact, fact.anchor_block, &lottery, sw_policy()),
                    Err(refusal),
                    "the gate refuses on the same state for the same reason"
                );
                let fold = |at: &PalwBlockContextV2, extras: &PalwTransitionExtrasV1| {
                    apply_palw_transition_v2_with_extras(&s0, &sp, at, &[], None, false, false, false, false, extras).unwrap()
                };
                let phase = |state: &PalwChainStateV2| state.claim(&claim_id).unwrap().phase.clone();
                let executor_reserved = s0.reserved_exposure(&key(1));
                let no_forfeit = |after: &PalwChainStateV2, label: &str| {
                    assert!(after.panel(&claim_id).is_none(), "{label}: no panel was ever bound");
                    for (k, bond) in s0.bonds_iter() {
                        let now = after.bond(k).unwrap();
                        assert_eq!((now.collateral, now.slashed), (bond.collateral, bond.slashed), "{label}: S0, no forfeit on {k:?}");
                    }
                    assert!(after.reserved_exposure(&key(1)) < executor_reserved, "{label}: the executor's reservation is released");
                };

                // Past the fence, on a block that may anchor a panel.
                let armed = PalwTransitionExtrasV1 { sw8_anchor_delay: Some(five().anchor_delay()), ..duty_extras() };
                let (before, _) = fold(&ctx(0x5F00_0003, slot - 1, slot - 1), &armed);
                assert_eq!(phase(&before), PalwClaimPhaseV2::Provisional, "a block below the slot is not the claim's anchor");
                let (heartbeat, _) = fold(&ctx(0x5F00_0004, slot + 1, slot + 1), &duty_extras());
                assert_eq!(phase(&heartbeat), PalwClaimPhaseV2::Provisional, "a block that may not anchor voids nothing");
                let (voided, delta) = fold(&point, &armed);
                assert_eq!(
                    phase(&voided),
                    PalwClaimPhaseV2::Voided { voided_daa: slot, reason: PalwVoidReasonV2::BindTimeout },
                    "the anchor block voids the claim it did not bind, at its own DAA"
                );
                no_forfeit(&voided, "step 4c");
                voided.assert_deadline_consistency(&sp).expect("the deadline index is the claims' recomputed deadlines");
                let rebuilt = PalwStateCarriageV2::from_state(&voided).into_state(&sp, None).expect("the carriage rebuild accepts it");
                assert_eq!(rebuilt.state_root(), voided.state_root(), "a restart reproduces the void");
                assert_eq!(revert_delta_v2(&voided, &delta, &sp).unwrap().state_root(), s0.state_root(), "a reorg undoes it");

                // The fence-off twin: nothing voids the claim at its anchor; the window's sweep does.
                let (twin_at_anchor, _) = fold(&point, &duty_extras());
                assert_eq!(phase(&twin_at_anchor), PalwClaimPhaseV2::Provisional, "fence off: the claim waits");
                let deadline = claim.bind_base_daa() + sp.window_bind();
                let (swept, _) = fold(&ctx(0x5F00_0002, deadline + 1, deadline + 1), &duty_extras());
                assert!(
                    matches!(phase(&swept), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }),
                    "{:?}",
                    phase(&swept)
                );
                no_forfeit(&swept, "the window's sweep");
            }

            /// `sw_state` with the executor ONE OF the eight genesis seats — testnet-12's shape, where
            /// every producer is a genesis card: bond 1 (key 7, operator 0x21) posts a genesis seat's
            /// collateral beside the seven others (bonds 2..=8). `saturate` as `sw_state`'s, bond 1
            /// included (the executor's own heavy claim fills its own headroom).
            fn genesis_executor_state(saturate: &[u64]) -> (PalwChainStateV2, Hash64) {
                let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
                let rows: Vec<(u64, u64, u64)> = (2..=8u64).map(|b| (b, 0x40 + b, GENESIS_SEAT)).collect();
                let mut objects = vec![adr0130_class(), heavy_class(), adr0130_bond(1, 7, 0x21, GENESIS_SEAT)];
                objects.extend(rows.iter().map(|(b, op, c)| adr0130_bond(*b, 30 + *b as u8, *op, *c)));
                let (mut state, _) =
                    apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();
                let mut daa = 101;
                for b in saturate {
                    let (pk, op) = if *b == 1 { (7, 0x21) } else { (30 + *b as u8, rows.iter().find(|row| row.0 == *b).unwrap().1) };
                    state = fold_attempt(&state, &sp, daa, *b, pk, op, h64(3), h64(13), HEAVY_PWU).0;
                    daa += 1;
                }
                fold_attempt(&state, &sp, daa, 1, 7, 0x21, h64(1), h64(11), 40)
            }

            /// **T94, the M4 review's finding 1: a genesis card's own claim keeps SW-10's one-seat
            /// tolerance.** On testnet-12 the executor is one of the eight genesis seats and may not
            /// sit, so the seven others are the whole population; with one of them saturated they are
            /// `6/7 = 857‰`, which alone would refuse. SW-10's executor term counts the executor's own
            /// capped weight on both sides (`palw_panel_stake_executor_bonds_judging_v1`), so the claim
            /// is measured over the eight:
            ///
            /// * one other seat saturated: `7/8 = 875‰`, the draw binds (and never seats the executor
            ///   or the saturated seat);
            /// * the executor saturated as well — the busiest producer, its own claims filling its own
            ///   headroom — still `7/8`: its load is unread, it cannot sit anyway;
            /// * two other seats saturated: `6/8`, `InsufficientEligibleStake { 6g, 8g }` — the ADR's
            ///   "refuses with two", reported with the term on both sides;
            /// * an executor outside the population (`sw_state`'s, below the panel floor) adds nothing,
            ///   and eight genesis seats with one saturated bind at `7/8` as the pure half pinned;
            /// * `stake: None` binds every one of them (no stake floor).
            #[test]
            fn t94_a_genesis_executor_keeps_the_one_saturated_seat_tolerance() {
                let g = 939_063u128;
                let stake = PalwPanelStakeDrawV1::V1;
                let weight = |list: &[(&PalwBondKeyV2, &PalwBondStateV2)]| palw_panel_stake_weight_v1(list, &stake);
                let executor_weight = |state: &PalwChainStateV2, claim: &Hash64| {
                    weight(
                        &palw_panel_stake_executor_bonds_judging_v1(
                            state,
                            claim,
                            &h64(1),
                            100,
                            None,
                            false,
                            None,
                            Some(sw_economy()),
                            5,
                        )
                        .unwrap(),
                    )
                };

                let (one, c1) = genesis_executor_state(&[2]);
                let (eligible, base) = populations(&one, &c1);
                assert_eq!((weight(&eligible), weight(&base)), (6 * g, 7 * g), "the seven others, one saturated");
                assert!(
                    matches!(palw_panel_stake_floor_v1(6 * g, 7 * g, 875), Err(PalwPanelV2Error::InsufficientEligibleStake { .. })),
                    "the premise: the seven alone refuse at 857‰"
                );
                assert_eq!(executor_weight(&one, &c1), g, "the executor is a genesis seat");

                let (busy, cb) = genesis_executor_state(&[1, 2]);
                assert!(
                    busy.reserved_exposure(&key(1)) > one.reserved_exposure(&key(1)),
                    "the busy executor's own heavy claim loads it"
                );
                assert_eq!(executor_weight(&busy, &cb), g, "the executor term reads no headroom");

                for i in 0..24u64 {
                    for (label, state, claim) in [("one saturated", &one, &c1), ("and the executor busy", &busy, &cb)] {
                        let seats =
                            draw(state, claim, anchor(i), sw_policy()).unwrap_or_else(|e| panic!("{label}, anchor {i}: {e:?}"));
                        assert_eq!(seats.len(), 5, "{label}: a full jury");
                        assert!(!seats.iter().any(|s| s.bond == key(1) || s.bond == key(2)), "{label}, anchor {i}: {seats:?}");
                    }
                }

                let (two, c2) = genesis_executor_state(&[2, 3]);
                for i in 0..8u64 {
                    assert_eq!(
                        draw(&two, &c2, anchor(i), sw_policy()),
                        Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 6 * g, base: 8 * g }),
                        "two other seats saturated: 6/8 refuses"
                    );
                }
                for (state, claim) in [(&one, &c1), (&busy, &cb), (&two, &c2)] {
                    assert!(draw(state, claim, anchor(0), lottery_policy()).is_ok(), "stake None has no stake floor");
                }

                let (outside, co) = sw_state(&genesis_and_small(0), &[2]);
                assert_eq!(executor_weight(&outside, &co), 0, "an executor below the panel floor could not sit: no term");
                assert!(draw(&outside, &co, anchor(0), sw_policy()).is_ok(), "eight genesis seats, one saturated: 7/8 binds");
            }
        }
    }
}
