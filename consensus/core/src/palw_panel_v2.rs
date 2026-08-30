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
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwPanelSeatV2, PalwStateParamsV2,
};
use blake2b_simd::Params;

pub const PALW_PANEL_V2_DOMAIN_SEAT_TICKET: &[u8] = b"misaka-palw/panel-v2/seat-ticket/v1";
pub const PALW_RECEIPT_V2_DOMAIN_MESSAGE: &[u8] = b"misaka-palw/receipt-v2/message/v1";
/// ML-DSA-87 signing context for a V2 seat receipt — its own family domain (audit P0-6).
pub const PALW_RECEIPT_V2_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/receipt-v2/mldsa87/v1";

pub const PALW_PANEL_V2_ALL_DOMAINS: &[&[u8]] =
    &[PALW_PANEL_V2_DOMAIN_SEAT_TICKET, PALW_RECEIPT_V2_DOMAIN_MESSAGE, PALW_RECEIPT_V2_MLDSA87_CONTEXT];

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
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let executor_bond = claim.bond;
    let executor = state.bond(&executor_bond).ok_or(PalwPanelV2Error::SeatBondMissing(executor_bond))?;
    let executor_operator = executor.operator_id;
    let executor_key = executor.pubkey.clone();

    // Ticket every eligible bond. Exclusions per Decision 7: the executor's bond, the executor's
    // operator, the executor's key — all three read from the ONE registry, so no second
    // namespace exists for them to diverge in (the P0-7 defect).
    let mut tickets: Vec<(Hash64, PalwBondKeyV2, Hash64)> = Vec::new();
    for (bond_key, bond) in state.bonds_iter() {
        // **A seat has to have something left to lose** — status AND balance, through the one
        // predicate, so the RPC that reports eligibility and the sortition that decides it cannot
        // answer differently.
        if !crate::palw_state_v2::palw_bond_may_take_work_v2(bond, min_collateral_sompi) {
            continue;
        }
        // ADR-0065 D1: a bond has to have been standing for a while before it may judge.
        if registered_by_daa.is_some_and(|by| bond.registered_daa > by) {
            continue;
        }
        if *bond_key == executor_bond || bond.operator_id == executor_operator || bond.pubkey == executor_key {
            continue;
        }
        let mut ticket = keyed(PALW_PANEL_V2_DOMAIN_SEAT_TICKET);
        ticket.update(anchor_block.as_byte_slice());
        ticket.update(claim_id.as_byte_slice());
        ticket.update(&borsh::to_vec(bond_key).expect("bond keys are borsh-serializable"));
        tickets.push((finish(ticket), *bond_key, bond.operator_id));
    }
    tickets.sort();

    // One seat per operator, in ticket order, first `seat_count` win.
    let mut seats: Vec<PalwPanelSeatV2> = Vec::new();
    let mut seated_operators: Vec<Hash64> = Vec::new();
    for (_, bond_key, operator_id) in tickets {
        if seats.len() == params.seat_count as usize {
            break;
        }
        if seated_operators.contains(&operator_id) {
            continue;
        }
        seated_operators.push(operator_id);
        seats.push(PalwPanelSeatV2 { bond: bond_key, operator_id });
    }
    if seats.len() < params.seat_count as usize {
        return Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: params.seat_count, available: seats.len() as u16 });
    }
    Ok(seats)
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

    let derived = derive_panel_v2_with_maturity(
        state,
        params,
        claim_id,
        anchor.anchor_block,
        state_params.min_collateral_sompi(),
        palw_seat_maturity_floor_v1(anchor.anchor_daa, bond_maturity_daa),
    )?;
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

/// What a validated quorum licenses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwReceiptQuorumV2 {
    /// ≥ quorum seats signed `Valid`: the `ReceiptLicensed` object is acceptable.
    Licensed { valid: u16 },
    /// ≥ quorum seats signed `Unavailable`: the producer defaulted on its DA obligation, and the
    /// `ProducerDefaulted` object is acceptable — the panel answered, the producer did not.
    ProducerUnavailable { unavailable: u16 },
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
    validate_receipt_quorum_v2_with_policy(state, params, state_params, ctx, network_domain, claim_id, receipts, verify_mldsa87, false)
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
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let PalwClaimPhaseV2::PanelBound { bound_daa } = claim.phase else {
        return Err(PalwPanelV2Error::WrongPhase { claim: *claim_id, edge: "ReceiptQuorum" });
    };
    let receipt_deadline = bound_daa
        .checked_add(state_params.window_receipt())
        .ok_or(PalwPanelV2Error::ReceiptOutsideWindow { seat: claim.bond, why: "the receipt deadline overflows the DAA score" })?;
    let panel = state.panel(claim_id).ok_or(PalwPanelV2Error::NoPanel(*claim_id))?;

    let mut answered: Vec<PalwBondKeyV2> = Vec::new();
    let mut valid: u16 = 0;
    let mut unavailable: u16 = 0;
    for receipt in receipts {
        if receipt.claim != *claim_id {
            return Err(PalwPanelV2Error::ReceiptClaimMismatch { got: receipt.claim, expected: *claim_id });
        }
        if !panel.seats.iter().any(|seat| seat.bond == receipt.seat_bond) {
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
            PalwReceiptVerdictV2::Valid => valid += 1,
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
    if valid >= params.quorum {
        return Ok(PalwReceiptQuorumV2::Licensed { valid });
    }
    // ADR-0065 D4: past the fence there is no second quorum to reach. Reported as `NoQuorum` with
    // the true tally, so an operator reading the log still sees how many seats said they got
    // nothing — the number stops being a verdict, it does not stop being visible.
    if !unavailable_abstains && unavailable >= params.quorum {
        return Ok(PalwReceiptQuorumV2::ProducerUnavailable { unavailable });
    }
    Err(PalwPanelV2Error::NoQuorum { valid, unavailable, needed: params.quorum })
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
            signature: Vec::new(),
        }
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
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
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
        };
        assert!(palw_bond_may_take_work_v2(&live, 1_000), "a fully-collateralised Active bond seats");

        let exhausted = PalwBondStateV2 { collateral: 0, slashed: 1_000, ..live.clone() };
        assert!(matches!(exhausted.status, PalwBondStatusV2::Active), "slashing never changed the status — that is the defect");
        assert!(!palw_bond_may_take_work_v2(&exhausted, 0), "and it must not seat even where the floor is zero");

        let thin = PalwBondStateV2 { collateral: 999, slashed: 1, ..live.clone() };
        assert!(!palw_bond_may_take_work_v2(&thin, 1_000), "a bond that could not register today does not seat today");

        let retiring = PalwBondStateV2 { status: PalwBondStatusV2::Retiring { since_daa: 5 }, ..live };
        assert!(!palw_bond_may_take_work_v2(&retiring, 0), "and retirement still excludes, collateral or not");
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

        // …and the rule takes nothing away from the bonds that were already standing.
        let unchanged =
            derive_panel_v2_with_maturity(&late, &panel_params(), &claim_id, anchor, 0, floor).expect("the older bonds still seat");
        assert_eq!(unchanged, derive_panel_v2(&late, &panel_params(), &claim_id, anchor, 0).unwrap(), "same seats, same order");

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
                validate_receipt_quorum_v2_with_policy(&bound, &p, &sp, &here, net, &claim_id, &receipts, verify, true),
                Err(PalwPanelV2Error::NoQuorum { valid: 0, unavailable: 2, needed: 2 })
            ),
            "past the fence an Unavailable quorum licenses nothing, and the count is still visible"
        );
        // And the licensing direction is untouched — D4 removes one verdict's power, not the panel's.
        let served = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid), sign_as(&seats[1], PalwReceiptVerdictV2::Valid)];
        assert_eq!(
            validate_receipt_quorum_v2_with_policy(&bound, &p, &sp, &here, net, &claim_id, &served, verify, true),
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
            validate_receipt_quorum_v2_with_policy(&bound, &p, &sp, &here, net, &claim_id, &mixed, verify, true),
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
}
