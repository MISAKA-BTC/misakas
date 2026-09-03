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

/// **How many DISTINCT OPERATORS could take a seat right now** — the live registry measured with
/// the same two predicates [`derive_panel_v2_with_maturity`] uses to build its ticket list.
///
/// This exists so an operator-facing warning cannot drift from the rule it warns about. It is not
/// a second implementation of the draw: it calls
/// [`crate::palw_state_v2::palw_bond_may_take_work_v2`] and applies the identical maturity
/// comparison, and it counts operators rather than bonds because the draw seats one bond per
/// operator — a registry of ten bonds under two operators seats two.
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
    derive_panel_v2_with_capability_proof(state, params, claim_id, anchor_block, min_collateral_sompi, registered_by_daa, false)
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
#[allow(clippy::too_many_arguments)]
pub fn derive_panel_v2_with_capability_proof(
    state: &PalwChainStateV2,
    params: &PalwPanelParamsV2,
    claim_id: &Hash64,
    anchor_block: BlockHash,
    min_collateral_sompi: u64,
    registered_by_daa: Option<u64>,
    capability_proof: bool,
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
        if !crate::palw_state_v2::palw_bond_may_judge_class_v3(state, bond_key, bond, &claim.class_id, capability_proof) {
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

    let derived = derive_panel_v2_with_capability_proof(
        state,
        params,
        claim_id,
        anchor.anchor_block,
        state_params.min_collateral_sompi(),
        palw_seat_maturity_floor_v1(anchor.anchor_daa, bond_maturity_daa),
        capability_proof,
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

        let retiring = PalwBondStateV2 { status: PalwBondStatusV2::Retiring { since_daa: 5 }, ..live };
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

        let err = derive_panel_v2_with_capability_proof(&state, &panel_params(), &claim_id, anchor, 0, None, true)
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
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
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
