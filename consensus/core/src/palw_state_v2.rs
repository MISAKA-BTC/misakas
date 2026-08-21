//! `PalwChainStateV2` — candidate-scoped PALW state, its delta, and its root (ADR-0042
//! Decision 5, PR-03; hash ordering recorded in ADR-0043).
//!
//! P0-4's defect was never one bad read — it was that PALW facts lived in the node's CURRENT-SINK
//! mutable stores, so two nodes holding the same DAG weighed the same candidate differently
//! depending on what else they had applied. This module is the replacement substrate: a pure
//! state machine in which **every** weight input is a function of the candidate chain point, and
//! nothing else.
//!
//! The gate this module exists to pass (ADR-0039 §3d, ADR-0042 Decision 5):
//!
//! ```text
//! same genesis · same DAG · same block bodies · same evidence
//!     ⟹ identical palw_state_root, safe/live weight, and selected tip
//! ```
//!
//! independent of block/evidence arrival order, prior sink, DB insertion order, restart count,
//! archival-vs-pruned, IBD start point, thread count, or ISA. The differential tests at the
//! bottom of this file are that sentence, executable — including the audit register's
//! `palw_v2_weight_invariant_under_prior_sink`.
//!
//! ## What is state, what is cache, what is parameter
//!
//! * **Primary data** — the collections and scalars the root commits to. Private fields;
//!   mutation happens only inside [`apply_palw_transition_v2`], so no caller can poke a fact in
//!   from the sink.
//! * **Indices** (`deadlines`, `unresolved`, `open_courts_by_claim`) — derivable caches for the
//!   sweeps. Never serialized, never hashed, rebuilt on load, and cross-checked against the
//!   primary data by [`PalwChainStateV2::assert_internal_consistency`] (which carriage loading
//!   runs unconditionally).
//! * **Parameters** ([`PalwStateParamsV2`]) — network constants (β, windows, epoch length). They
//!   are part of the ruleset fingerprint (Decision 11), NOT of the per-block state, so the root
//!   does not re-commit them.
//!
//! ## Determinism rules this file is written under
//!
//! * `BTreeMap`/`BTreeSet` only — nothing whose iteration order is a hash seed.
//! * No floats; β is integer permille and its rounding is FLOOR, stated once, at the one place
//!   the contribution is computed ([`immature_contribution_v2`]).
//! * Checked arithmetic — an overflowing chain is an invalid chain, never a saturated one.
//! * **A missing fact is an error, never a permissive zero** (Decision 5). An object that names
//!   an absent bond/class/claim/session, a wrong-phase edge, a duplicate id, or a non-monotone
//!   chain point rejects the transition; nothing is skipped.
//! * Amounts released must be amounts reserved: a claim snapshots its `reserved` collateral value
//!   and its `immature_contribution` at creation and releases exactly those bytes-for-bytes, so a
//!   later change to class or params can never make release drift from reserve.
//!
//! ## The claim lattice (ADR-0042 Decision 2)
//!
//! ```text
//! Provisional ──PanelBound──▶ PanelBound ──ReceiptLicensed──▶ ReceiptLicensed ──window──▶ Final
//!      │ bind timeout              │ receipt timeout                 │ court fraud / producer default
//!      ▼                           ▼                                 ▼
//!    Voided                      Voided                            Voided
//! ```
//!
//! `Final` and `Voided` are terminal and permanent. Weight per the Decision 2 table: an immature
//! claim contributes `⌊β·pwu/1000⌋` to `bounded_immature`; a `Final` claim contributes its full
//! `pwu` to `safe_weight`; `Voided` contributes nothing. `live_total` is never stored — the state
//! hands `safe` and `bounded_immature` to [`PalwCandidateOrderV1::new`], which constructs the sum
//! so maturing cannot lower it.
//!
//! ## The safe frontier, precisely
//!
//! The frontier is the last chain point at which the ENTIRE past was resolved: it advances to
//! `(ctx.blue_score, ctx.block)` exactly when, at the end of an apply, no unresolved claim
//! exists, and holds still otherwise. It is monotone by construction and observed lazily (only at
//! block boundaries). This is the anti-fabrication key ordered first by the comparator: a private
//! fork can pile up attempts without limit, but its claims cannot mature, so from its fork point
//! its frontier never moves again.
//!
//! ## Ordering inside one apply (fixed; changing it is a consensus change)
//!
//! 1. context monotonicity check;
//! 2. deadline sweep for deadlines strictly before `ctx.daa_score`, in `(deadline, claim)` order;
//! 3. `accepted_objects`, in the given (consensus acceptance) order;
//! 4. `current_attempt` — the block's own claim, last;
//! 5. frontier observation; weight/frontier/last-point delta entries.
//!
//! A block may therefore still bind a panel to a claim whose deadline IS this block's DAA score,
//! and its own attempt may reference a class or bond registered by an object in the same block —
//! both directions are deterministic because the order above is.
//!
//! What this module deliberately does NOT do (and which PR owns it): validate evidence
//! (signatures, quorums, draws — PR-04/PR-06), enforce the exposure CEILING and epoch BUDGET
//! (admission predicates read what is accounted here — PR-04), move collateral value on slash
//! (PR-07/PR-09), or wire any of this into the pipeline (PR-08). The transition trusts that
//! `accepted_objects` were accepted; it enforces referential integrity and the lattice, and it
//! accounts.

use crate::BlockHash;
use crate::palw_attempt_v2::{PalwAttemptEnvelopeV2, attempt_id_v2};
use crate::palw_fork_choice::PalwCandidateOrderV1;
use crate::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3;
use crate::tx::TransactionOutpoint;
use blake2b_simd::Params;
use kaspa_hashes::{Hash64, ZERO_HASH64};
use std::collections::{BTreeMap, BTreeSet};

/// Version 3: the integration of two independent version-2 bumps, neither of whose roots
/// survives. ADR-0045 added `class_shares` and `epoch_budgets` to the root preimage in their
/// declared field positions; ADR-0044 (FP-03) added the free-prompt claim source, the
/// per-quantum spend ledger, and the receipt lane's target/census collections. The merged
/// preimage carries BOTH collection sets, so it differs from both parents' version-2 roots —
/// ADR-0043's rule for a consensus change to the root: a new version, never a silent
/// re-reading of old bytes. Nothing has persisted any earlier root on a shipped preset.
///
/// Version 4: ADR-0042 Decision 10's escrow. The root preimage gains `pending_payouts` and every
/// claim gains `escrowed_reward`; the carriage carries the queue because the root hashes it, and
/// `PalwBlockContextV2` — serialized into the root through `last_point` — gains the block's
/// subsidy. Three separate reasons the version had to move, all of them the same rule: a
/// consensus change to the root gets a new version rather than a silent re-reading of old bytes.
pub const PALW_STATE_V2_VERSION: u16 = 4;

pub const PALW_STATE_V2_DOMAIN_OPERATOR_ID: &[u8] = b"misaka-palw/state-v2/operator-id/v1";

/// `H(operator_pubkey)` — the operator identity panel dedup runs on.
///
/// Domain-separated so an operator id can never collide with a class id, a claim id or any other
/// `Hash64` this ruleset mints, and derived rather than carried so two bonds share an operator
/// exactly when they name the same key.
pub fn palw_operator_id_v2(operator_pubkey: &[u8]) -> Hash64 {
    let mut state = keyed(PALW_STATE_V2_DOMAIN_OPERATOR_ID);
    state.update(&(operator_pubkey.len() as u64).to_le_bytes());
    state.update(operator_pubkey);
    finish(state)
}

pub const PALW_STATE_V2_DOMAIN_STATE_ROOT: &[u8] = b"misaka-palw/state-v2/state-root/v1";
pub const PALW_STATE_V2_DOMAIN_COLLECTION: &[u8] = b"misaka-palw/state-v2/collection/v1";
pub const PALW_STATE_V2_DOMAIN_CARRIAGE: &[u8] = b"misaka-palw/state-v2/carriage/v1";

/// Every domain this module keys, so the cross-family uniqueness test can see them.
pub const PALW_STATE_V2_ALL_DOMAINS: &[&[u8]] =
    &[PALW_STATE_V2_DOMAIN_STATE_ROOT, PALW_STATE_V2_DOMAIN_COLLECTION, PALW_STATE_V2_DOMAIN_CARRIAGE];

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------------------------

// ADR-0045 Decision 3 deleted `PalwClassDaaV2Params` here: the share table it carried is chain
// state now (`PalwChainStateV2::class_shares`, granted by `ClassRegistered`, conserved to 1000‰
// at every mutation), and what actually was a network constant — the base class's identity, the
// retarget clamp, the budget tolerance — lives directly on [`PalwStateParamsV2`].

/// The state machine's network constants. Constructed only through [`PalwStateParamsV2::new`],
/// which refuses out-of-range values — there is no `Default`, because a defaulted consensus
/// parameter is a flipped fence wearing a convenience API.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwStateParamsV2 {
    /// Immature live-weight fraction in permille, `β ≤ 1000` (ADR-0042 Decision 2).
    beta_permille: u16,
    /// DAA-score window from acceptance within which a claim must become `PanelBound`.
    window_bind: u64,
    /// DAA-score window from panel binding within which a claim must become `ReceiptLicensed`.
    window_receipt: u64,
    /// DAA-score window from receipt licensing after which, with no open court session, the
    /// claim becomes `Final`.
    window_challenge: u64,
    /// DAA-score budget one court session gets from opening to verdict — the liveness backstop
    /// (ADR-0042 Decision 8 items 3/4 as the state machine sees them). A session still open past
    /// it closes CHALLENGER-side: prosecution is the challenger's burden, and a challenge nobody
    /// finishes must not freeze an honest claim forever. Decision 1's startup gate must hold
    /// `window_court > worst-case ladder duration` so an honest prosecution always fits.
    window_court: u64,
    /// Length of a class-production epoch in DAA score units (ADR-0039 D5 counters), and the
    /// retarget span: each global epoch boundary the chain crosses closes one span and retargets
    /// every share-bearing, unfrozen class against it.
    epoch_length: u64,
    /// The permanently-Active liveness floor's class id (ADR-0039 W6′). The first registration
    /// on a chain must be this class, at the whole 1000‰ — the transition enforces it — and the
    /// atomic bundle carries the same id, cross-checked at the startup gate (the C5 pattern:
    /// the value the ruleset id commits to must be the value the machine enforces).
    base_class_id: Hash64,
    /// Per-adjustment retarget clamp (≥ 2), passed to `retarget_over_span_v1`.
    class_daa_max_factor: u32,
    /// ADR-0045 Decision 2: the epoch-budget tolerance, in permille of a class's cadence share.
    /// Fenced `[1000‰, PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE]` — below unity a budget starves
    /// its own class, above the ceiling a cap no epoch can approach is a cap in name only.
    budget_tolerance_permille: u32,
    /// Minimum slashable collateral a bond must register with. It lives here, where
    /// registrations are applied, because that is the only place the rule can bite; the atomic
    /// bundle carries the same value in `PalwBondParamsV2` and the startup gate requires the two
    /// to agree.
    min_collateral_sompi: u64,
    /// ADR-0044 Decision 1's source split, as the retarget consumes it: the attempt lane's
    /// permille of combined production; the receipt lane gets the remainder. **1000 = no receipt
    /// lane** — the pure-attempt configuration, under which the two-lane retarget is byte-for-byte
    /// the single-lane rule (every pre-FP fixture passes 1000 and changes nothing). The FP
    /// bundle's startup gate additionally demands `0 < split < 1000` on a live FP network
    /// (a zero floor has no beacons; 1000 has no receipts). Class shares are NOT here: ADR-0045
    /// Decision 3 made them chain state (`PalwChainStateV2::class_shares`), granted by
    /// registration — the lane split composes with those granted shares at retarget.
    fp_attempt_share_permille: u16,
    /// **One rung's window: how long the party whose turn it is has to move (P0-9).**
    ///
    /// It lives here, not only on `PalwCourtParamsV2`, because it decides STATE: the ladder is
    /// chain state now, and its deadlines are what the rung sweep reads. Two structs holding one
    /// number is the audit-C5 shape, so `PalwConsensusParamsV2::validate` requires the bundle's
    /// declared court window to equal this one.
    ///
    /// **Defaults to `window_court`** — one rung may take the whole session. The sweep treats a
    /// rung window that is not STRICTLY inside the session budget as no clock at all, so the
    /// default reproduces exactly the behavior of every network built before the ladder was
    /// carried: the backstop stays the only thing that fires, and it still closes
    /// challenger-side. That distinction matters because the two verdicts are opposite — a
    /// first-rung silence is the RESPONDER's default, the backstop is the CHALLENGER's — so a
    /// default that let the rung fire would have inverted the outcome of every unfinished
    /// challenge on every network that never configured a ladder. A
    /// network that wants an interactive ladder tightens it through
    /// [`PalwStateParamsV2::with_turn_deadline_daa`]. It is never zero, because a zero window
    /// defaults whichever party the block order happens to reach first.
    turn_deadline_daa: u64,
    /// **The producer's carve of a block's subsidy, in permille — what a claim ESCROWS.**
    ///
    /// It lives here, in the state machine's own parameters, because it decides STATE: the
    /// escrowed amount is snapshotted into the claim at creation and is what the chain later
    /// pays. Deriving it at payout time instead — from whatever subsidy the maturing block
    /// happens to have — is what `palw_v2_matured_carves` did before this field existed, and it
    /// breaks ADR-0042 Decision 10's "never an addition to the schedule": two claims maturing in
    /// one block would each draw a full carve of that block's subsidy, minting up to N × the
    /// carve out of one subsidy.
    ///
    /// **Zero is the honest default and means "this network pays no PALW reward".** Every
    /// pre-reward fixture leaves it zero and keeps its exact prior meaning; a network that
    /// carries value sets it through [`PalwStateParamsV2::with_worker_carve_permille`], and
    /// Decision 1's startup gate is where a live network's non-zero requirement belongs.
    worker_carve_permille: u16,
    /// **Audit C5's free panel re-roll, priced (the free-prompt lane's half).**
    ///
    /// On the attempt lane, abandoning a claim at `BindTimeout` already costs three things and
    /// `abandoning_a_panel_costs_a_block_its_reward_and_its_epoch_budget` measures all three. All
    /// three rest on the same fact: an attempt CLAIM IS A BLOCK. A free-prompt commitment is not —
    /// it rides a transaction — so on that lane every one of them evaporates, which the merge that
    /// brought the two lanes together made measurable: after a `BindTimeout` the reservation was
    /// released in full, no counter moved, no bond was debited, and the next commitment was
    /// accepted in the very next block. A producer who disliked its drawn panel could redraw for
    /// the price of a transaction fee, indefinitely.
    ///
    /// So an abandoned free-prompt claim keeps its collateral RESERVED for this many DAA after the
    /// void. Nothing is confiscated — the producer is not guilty of anything, it declined to
    /// proceed — but N concurrent redraws now need N × the reservation, which converts a
    /// fee-priced attack into a collateral-priced one. That is the currency Decision 7's Sybil
    /// bound is already denominated in, so the two compose: an attacker's redraw rate is bounded
    /// by the same collateral that bounds its identity count.
    ///
    /// `0` disables the hold (the pre-FP configuration, and what every attempt-only fixture runs
    /// at — on that lane the block cost already does this job).
    fp_abandon_hold_daa: u64,
}

impl PalwStateParamsV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        beta_permille: u16,
        window_bind: u64,
        window_receipt: u64,
        window_challenge: u64,
        window_court: u64,
        epoch_length: u64,
        base_class_id: Hash64,
        class_daa_max_factor: u32,
        budget_tolerance_permille: u32,
        min_collateral_sompi: u64,
        fp_attempt_share_permille: u16,
        fp_abandon_hold_daa: u64,
    ) -> Result<Self, PalwStateV2Error> {
        if beta_permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("beta_permille exceeds 1000 (β ≤ 1)"));
        }
        if window_bind == 0 || window_receipt == 0 || window_challenge == 0 || window_court == 0 {
            return Err(PalwStateV2Error::InvalidParams("every lattice window must be at least one DAA unit"));
        }
        if epoch_length == 0 {
            return Err(PalwStateV2Error::InvalidParams("epoch_length must be at least one DAA unit"));
        }
        if base_class_id == Hash64::default() {
            return Err(PalwStateV2Error::InvalidParams("a zero base class id names no liveness floor"));
        }
        if class_daa_max_factor < 2 {
            return Err(PalwStateV2Error::InvalidParams("max_factor below 2 freezes the retarget"));
        }
        if budget_tolerance_permille < 1000 {
            return Err(PalwStateV2Error::InvalidParams("a budget tolerance below unity starves every class of its own cadence"));
        }
        if budget_tolerance_permille > crate::palw_class_daa::PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE {
            return Err(PalwStateV2Error::InvalidParams("a budget tolerance above the ceiling is a cap no epoch can approach"));
        }
        if min_collateral_sompi == 0 {
            return Err(PalwStateV2Error::InvalidParams("a zero minimum collateral bonds nothing — panel dedup would be free to defeat"));
        }
        if fp_attempt_share_permille == 0 || fp_attempt_share_permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("the attempt share must be 1..=1000 permille — a zero floor has no beacons"));
        }
        Ok(Self {
            beta_permille,
            window_bind,
            window_receipt,
            window_challenge,
            window_court,
            epoch_length,
            base_class_id,
            class_daa_max_factor,
            budget_tolerance_permille,
            min_collateral_sompi,
            fp_attempt_share_permille,
            fp_abandon_hold_daa,
            // Zero: `new` builds a network that pays no PALW reward, which is what every caller
            // predating the reward wiring meant and still means. See
            // `with_worker_carve_permille`.
            worker_carve_permille: 0,
            // The whole session as one rung: the identity that leaves the backstop as the only
            // clock. See the field doc.
            turn_deadline_daa: window_court,
        })
    }

    /// Set the rung window (P0-9). Refuses zero for the reason `PalwCourtParamsV2::new` does, and
    /// refuses a window longer than the session it must fit inside — a rung that outlives the
    /// backstop is a rung whose deadline can never be reached.
    pub fn with_turn_deadline_daa(mut self, turn_deadline_daa: u64) -> Result<Self, PalwStateV2Error> {
        if turn_deadline_daa == 0 {
            return Err(PalwStateV2Error::InvalidParams("a zero rung window defaults whichever party the block order reaches first"));
        }
        if turn_deadline_daa > self.window_court {
            return Err(PalwStateV2Error::InvalidParams("a rung window longer than the court window can never come due"));
        }
        self.turn_deadline_daa = turn_deadline_daa;
        Ok(self)
    }

    pub fn turn_deadline_daa(&self) -> u64 {
        self.turn_deadline_daa
    }

    /// Set the producer carve (ADR-0042 Decision 10). Consuming-builder rather than a `new`
    /// argument so that adding reward to this machine could not silently change what an existing
    /// caller's params mean — a thirteenth positional `u16` next to `fp_attempt_share_permille`
    /// and `beta_permille` is exactly the argument a reader mis-binds.
    pub fn with_worker_carve_permille(mut self, worker_carve_permille: u16) -> Result<Self, PalwStateV2Error> {
        if worker_carve_permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("the worker carve cannot exceed the whole subsidy"));
        }
        self.worker_carve_permille = worker_carve_permille;
        Ok(self)
    }

    pub fn worker_carve_permille(&self) -> u16 {
        self.worker_carve_permille
    }

    pub fn min_collateral_sompi(&self) -> u64 {
        self.min_collateral_sompi
    }

    pub fn beta_permille(&self) -> u16 {
        self.beta_permille
    }

    /// The epoch arithmetic admission's budget predicate must share with the state's counters —
    /// two epoch definitions would let a claim be inside one and outside the other.
    pub fn epoch_length(&self) -> u64 {
        self.epoch_length
    }

    /// The bind window, exposed for the acceptance layers that must agree with the sweep about
    /// when a claim's panel may still legally bind.
    pub fn window_bind(&self) -> u64 {
        self.window_bind
    }

    /// The receipt window, exposed for the startup gate's liability-period arithmetic.
    pub fn window_receipt(&self) -> u64 {
        self.window_receipt
    }

    /// The challenge window, exposed so court acceptance can agree with the Final sweep about
    /// when a claim is still challengeable.
    pub fn window_challenge(&self) -> u64 {
        self.window_challenge
    }

    /// The per-session court budget (see the field's doc).
    pub fn window_court(&self) -> u64 {
        self.window_court
    }

    pub fn base_class_id(&self) -> Hash64 {
        self.base_class_id
    }

    pub fn class_daa_max_factor(&self) -> u32 {
        self.class_daa_max_factor
    }

    pub fn budget_tolerance_permille(&self) -> u32 {
        self.budget_tolerance_permille
    }

    /// The attempt lane's permille of combined production (see the field's doc).
    pub fn fp_attempt_share_permille(&self) -> u16 {
        self.fp_attempt_share_permille
    }

    /// How long an abandoned free-prompt claim's collateral stays reserved (see the field's doc).
    pub fn fp_abandon_hold_daa(&self) -> u64 {
        self.fp_abandon_hold_daa
    }

    /// ADR-0045's grant floor: the smallest share whose WORST-CASE epoch budget
    /// (denominator = the whole 1000‰) is still at least one block — `⌈10⁶ / (tol · E)⌉`.
    /// Enforced at every grant, which is what makes a mid-flight zero budget unrepresentable
    /// instead of an epoch-time cliff (the V1 derivation's `ZeroBudget`, moved to where the
    /// share is chosen).
    pub fn min_grantable_share_permille(&self) -> u16 {
        let denom = (self.budget_tolerance_permille as u128) * (self.epoch_length as u128);
        // denom ≥ 1000 (tolerance floor × epoch ≥ 1), so the ceiling division is ≤ 1000 and a
        // whole-table base grant is always fundable.
        (1_000_000u128.div_ceil(denom)).max(1) as u16
    }
}

/// `⌊β · pwu / 1000⌋` — THE definition of an immature claim's live contribution. Floor, so the
/// immature side is never rounded up into weight nobody earned; every node computes the same
/// integer or the chain forks on a rounding mode.
/// The structural half of a class-contradiction proof — everything a PURE transition can decide
/// about whether a freeze is earned (gate item 12, ADR-0027 §5).
///
/// Signature verification is deliberately NOT here, for the reason `BondRegistered` states: the
/// key registry is chain state the acceptance layer owns, and crypto lives outside
/// `consensus-core`. `crate::palw_slash::adjudicate_class_contradiction_v1` is the whole
/// adjudication, run where a verifier is in hand; this is the subset the state machine must not
/// admit a freeze without, so that a block carrying a well-formed-looking but empty certificate
/// cannot halt a class on its own.
///
/// The three facts:
///
/// 1. **The certificate is about THIS class.** `runtime_class_id` and `execution_class_id` are one
///    namespace (ADR-0038), so the comparison is direct. Without it, evidence about any class
///    could freeze any other — including a real contradiction in a disposable class being used to
///    freeze the liveness floor.
/// 2. **Both attestations bind the certificate's own job context.** A pair that talks about two
///    different jobs is two facts, not a contradiction.
/// 3. **They actually disagree.** Two attestations that match are a class working correctly; a
///    freeze on them would let anyone halt a class by quoting it agreeing with itself.
pub fn check_class_contradiction_shape_v2(
    class_id: Hash64,
    certificate: &crate::palw_slash::PalwClassContradictionCertificateV1,
) -> Result<(), PalwStateV2Error> {
    if certificate.version != crate::palw_slash::PALW_S_OBJECT_VERSION_V3 {
        return Err(PalwStateV2Error::ContradictionNotProven("the certificate is of an unsupported version"));
    }
    if certificate.job_context.runtime_class_id != class_id {
        return Err(PalwStateV2Error::ContradictionNamesAnotherClass {
            frozen: class_id,
            evidenced: certificate.job_context.runtime_class_id,
        });
    }
    let context_hash = certificate.job_context.context_hash();
    if certificate.attestation_a.job_context_hash != context_hash || certificate.attestation_b.job_context_hash != context_hash {
        return Err(PalwStateV2Error::ContradictionNotProven("an attestation binds a different job context"));
    }
    let same_logits = certificate.attestation_a.full_logits_trace_root == certificate.attestation_b.full_logits_trace_root;
    let same_committed = certificate.attestation_a.committed_root == certificate.attestation_b.committed_root;
    if same_logits && same_committed {
        return Err(PalwStateV2Error::ContradictionNotProven("the two attestations agree — there is no contradiction to act on"));
    }
    Ok(())
}

/// Whether a claim record is a free-prompt commitment abandoned at `BindTimeout` whose
/// collateral hold has not yet elapsed at `at_daa` (audit C5, free-prompt half).
///
/// A pure function of the RECORD and the params, which is what lets the deadline index and the
/// exposure accumulator both be rebuilt from the claims alone — a hold that needed its own stored
/// flag would be a fact two structures could disagree about.
pub fn palw_claim_is_on_abandon_hold_v2(claim: &PalwClaimStateV2, params: &PalwStateParamsV2, at_daa: u64) -> bool {
    if params.fp_abandon_hold_daa == 0 {
        return false;
    }
    if !matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }) {
        return false;
    }
    let PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::BindTimeout } = claim.phase else {
        return false;
    };
    match voided_daa.checked_add(params.fp_abandon_hold_daa) {
        // Strict: the block AT the release point is the one that releases, matching the sweep's
        // `deadline < daa_score` boundary.
        Some(release_at) => at_daa < release_at,
        None => true,
    }
}

pub fn immature_contribution_v2(params: &PalwStateParamsV2, pwu: u64) -> u128 {
    (pwu as u128) * (params.beta_permille as u128) / 1000
}

// ---------------------------------------------------------------------------------------------
// Keys and records
// ---------------------------------------------------------------------------------------------

/// A bond identity — its registration outpoint — with the total order `TransactionOutpoint`
/// itself does not carry. The order is `(transaction_id, index)`, both fixed-width, so map
/// iteration (and therefore every root) is identical on every ISA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBondKeyV2(pub TransactionOutpoint);

impl Ord for PalwBondKeyV2 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.0.transaction_id, self.0.index).cmp(&(other.0.transaction_id, other.0.index))
    }
}

impl PartialOrd for PalwBondKeyV2 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwBondStatusV2 {
    Active,
    /// Retirement requested; the bond backs its existing claims to resolution but may take no
    /// new ones (ADR-0042 Decision 6 item 9). The withdrawal-delay policy itself is admission's
    /// and PR-09's; the state records the fact.
    Retiring {
        since_daa: u64,
    },
}

/// **Audit C-08, second half: may this bond's collateral outpoint be spent at `now_daa`?**
///
/// A `PalwBondKeyV2` IS the outpoint holding the collateral (the genesis gate checks that it
/// really holds it, `palw_genesis_v2`). Existence is not custody, though: without this predicate
/// nothing stopped the owner spending that output in the very next block, so the chain's whole
/// notion of stake was "an outpoint that held the money once". Every ceiling, every slash and
/// Decision 7's Sybil bound would have been denominated in a balance the bond no longer had.
///
/// Locked in two cases:
///
/// * **`Active`** — the bond is backing claims and may take more. Nothing to argue about.
/// * **`Retiring`, inside the withdrawal delay** — the delay exists so a bond cannot leave before
///   its fraud is provable (`palw_fp_devnet_v3` asserts the delay exceeds that liability), and a
///   delay whose collateral is spendable throughout is a delay that delays nothing.
///
/// A slashed bond used to be locked FOREVER here, as the fail-closed placeholder for a burn rule
/// that did not exist. It exists now ([`palw_bond_burn_obligation_v2`]): the collateral releases on
/// the ordinary schedule and the spend must DESTROY what the bond lost, so the remainder is the
/// owner's and the slashed sompi are nobody's. Freezing a whole collateral over one sompi of slash
/// was the honest reading while the alternative was under-punishing to exactly zero; it stops
/// being the honest reading once the money can actually be destroyed.
pub fn palw_bond_collateral_is_locked_v2(bond: &PalwBondStateV2, now_daa: u64, withdrawal_delay_daa: u64) -> bool {
    match bond.status {
        PalwBondStatusV2::Active => true,
        PalwBondStatusV2::Retiring { since_daa } => match since_daa.checked_add(withdrawal_delay_daa) {
            // An overflowing delay never elapses, which is the safe direction.
            None => true,
            Some(release_at) => now_daa < release_at,
        },
    }
}

/// **Audit C-08, third part: what a released bond's spend must DESTROY.**
///
/// `PalwBondStateV2::slashed` is documented as burned — "it leaves `collateral` and enters
/// circulation nowhere" — and the first two parts made that half true: the collateral is money the
/// chain can see (the genesis gate) and money nobody can move while the bond lives (the spend
/// gate). Neither of them destroys anything. A bond that was slashed and then retired would have
/// walked away with its whole outpoint, and the sentence would have been false at the only moment
/// it mattered.
///
/// The design that closes it without moving the bond's identity: the KEY is the outpoint, so
/// re-minting a remainder would re-key the bond and break every claim that names it. The
/// obligation rides the SPEND instead — a transaction releasing this collateral must leave at
/// least `slashed` sompi unclaimed by any output, and the block's fee pool loses the same amount,
/// so the miner cannot collect what the chain destroyed. Burned by don't-mint, which is the
/// mechanism the §F service share and every unspent validator remainder already use.
///
/// `0` for a bond that never lost anything, which is a spend with no obligation at all.
pub fn palw_bond_burn_obligation_v2(bond: &PalwBondStateV2) -> u64 {
    bond.slashed
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBondStateV2 {
    /// The key admission verifies commitment signatures under (Decision 6 item 2).
    pub pubkey: Vec<u8>,
    /// Derived from the operator's key at registration ([`palw_operator_id_v2`]), never
    /// declared. Decision 7 rests panel dedup on it: splitting collateral across bonds must not
    /// manufacture extra panel seats. A self-declared label made that false for free — one
    /// registrant writes N different ids and takes N seats (audit C5). A key commitment does not
    /// make Sybil impossible, but it makes each extra identity a distinct key that must ALSO
    /// carry [`PalwStateParamsV2::min_collateral_sompi`] of its own, which is the cost the ADR's
    /// claim was always leaning on.
    pub operator_id: Hash64,
    /// Slashable collateral in sompi, NET of everything this bond has lost. What the exposure
    /// ceiling is measured against, and what a slash debits.
    pub collateral: u64,
    /// Cumulative slashed sompi. Burned: it leaves `collateral` and enters circulation nowhere.
    /// Recorded rather than merely subtracted so the loss is auditable from the state alone and
    /// a reader can tell a bond that never staked much from one that was convicted.
    pub slashed: u64,
    pub status: PalwBondStatusV2,
    pub registered_daa: u64,
    /// **Where this bond's matured rewards are paid — a 64-byte owner payload, not a script.**
    ///
    /// The chain DERIVES the output script from it
    /// ([`crate::dns_finality::p2pkh_mldsa87_spk`]), exactly as the validator-reward path and
    /// [`crate::palw_credit_batch`] do. Carrying a payload instead of a `ScriptPublicKey` is the
    /// difference between "the registrant names an address" and "the registrant writes an
    /// arbitrary script into a coinbase output": the second is a way to mint UTXOs whose spend
    /// conditions consensus never classified, on a chain whose whole input policy is PQ-only.
    /// With a payload there is exactly one script it can become.
    pub payout_payload: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwClassStatusV2 {
    Active,
    Frozen { since_daa: u64 },
    /// **Registered, adjudicable, and carrying no weight yet (conditions 12 and 13).**
    ///
    /// A class used to become `Active` the instant it was registered, which took cadence share
    /// from every incumbent at that instant — including from the liveness floor. There was no way
    /// to put a class on a chain, watch it, and only then let it carry weight; and a soak that
    /// cannot be run before activation is a soak that proves nothing.
    ///
    /// So a registration can name a future DAA score. Until then the class is in the registry and
    /// in the catalog — its artifact is committed and a dispute against it adjudicates exactly as
    /// one against an active class — but it holds NO share, and admission refuses its attempts. At
    /// the score, the transition grants `pending_share_permille` and the class becomes `Active`.
    ///
    /// The flip is a CLOCK, not an object. Nobody submits it, so there is nothing to forge and no
    /// authority question — the same shape the rung no-show uses, and the reason both are safe.
    Registered { activation_daa: u64, pending_share_permille: u16 },
}

/// The class's PWU rule — what Decision 6 item 6 checks an attempt's claimed `pwu` against.
///
/// The derivation record ADR-0039 left open and ADR-0042 required "before any class carries
/// weight" is ADR-0045 Decision 1, and it is the second variant. `MaxPerAttempt` survives as
/// pre-derivation scaffolding — fixtures and cheap test classes need a shape whose pwu they can
/// choose — but a value network registers only `DerivedV1` classes, and the register says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwPwuRuleV2 {
    /// An attempt may claim at most this many pwu (and at least 1, enforced statelessly).
    MaxPerAttempt(u64),
    /// ADR-0045 Decision 1: an attempt's `pwu` has exactly ONE legal value —
    /// `palw_pwu_v1(class_target at the candidate point, pwu_per_inference)` — and admission
    /// item 6 is equality, not a bound. Neither factor is a miner input: the target is rooted
    /// candidate state (which is what voids the ADR-0039 amendment's altitude objection), and
    /// `pwu_per_inference` is the registered normative operation count of one canonical
    /// inference — the class's step-leaf count, the same number the court's ladder walks. The
    /// registration DECLARES it; `palw_genesis_v2::verify_palw_genesis_v2` is what makes the
    /// declaration true, by demanding it equal the catalog's counted
    /// `canonical_step_leaf_count`. Without that gate this number is a direct, permanent
    /// multiplier on the class's fork-choice weight that breaks no other rule.
    DerivedV1 { pwu_per_inference: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassStateV2 {
    /// What artifact openings prove against; an attempt whose `artifact_root` differs is
    /// admission-rejected (Decision 6 item 5).
    pub artifact_root: Hash64,
    /// Sompi of collateral one pwu of this class puts at stake — the multiplier in Decision 6's
    /// reserved-exposure formula.
    pub slash_value_per_pwu: u64,
    /// Decision 6 item 6's referent (see [`PalwPwuRuleV2`]).
    pub pwu_rule: PalwPwuRuleV2,
    pub status: PalwClassStatusV2,
    pub registered_daa: u64,
}

/// Per-class difficulty slot — the full-width target `retarget_over_span_v1` folds (PR-09). The
/// state carries it so the root commits to it and the carriage moves it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassTargetV2 {
    pub target: u128,
}

/// Carried and rooted, but no PR-03 object writes it: capability issuance semantics arrive with
/// the admission/routing wiring (ADR-0034 lineage, PR-04+). Present now so adding the collection
/// later does not change the root derivation of every earlier state.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCapabilityStateV2 {
    pub class_id: Hash64,
    pub bond: PalwBondKeyV2,
    pub issued_daa: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwVoidReasonV2 {
    /// No panel bound within `window_bind` of acceptance.
    BindTimeout,
    /// No quorum receipt within `window_receipt` of panel binding.
    ReceiptTimeout,
    /// A court session ended in `ExecutorGuilty` (Decision 8).
    CourtFraud,
    /// The producer failed a data-availability obligation (Decision 7): claim void, and silence
    /// can never pin a block at `Provisional` forever.
    ProducerWithholding,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwClaimPhaseV2 {
    Provisional,
    PanelBound { bound_daa: u64 },
    ReceiptLicensed { licensed_daa: u64 },
    Final { final_daa: u64 },
    Voided { voided_daa: u64, reason: PalwVoidReasonV2 },
}

impl PalwClaimPhaseV2 {
    pub fn is_terminal(&self) -> bool {
        matches!(self, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. })
    }
}

/// What kind of work a claim stands for — and, for a free-prompt claim, its spend ledger.
///
/// ADR-0044 (FP-03): a free-prompt claim IS a claim in this lattice — same phases, same windows,
/// same exposure accounting, same court — with two deliberate divergences the variant carries the
/// data for:
///
/// * **No weight of its own.** An attempt claim's `Final` adds its `pwu` to `safe_weight`
///   (the attempt IS a block's work). A free-prompt claim's `Final` adds nothing — it *licenses*:
///   weight arrives only when a certified quantum is spent into a receipt block, `pwu/quanta` per
///   spend. Its `immature_contribution` is likewise zero, so commitment-stuffing cannot pump a
///   chain's live weight without blocks.
/// * **A per-quantum spend ledger.** `spent` records which quanta this candidate chain has
///   converted into blocks — branch-scoped by construction, because the claim lives in the
///   branch-scoped state (the UTXO double-spend analogy: a fork may spend the same quantum, and
///   fork choice, never a node-global cache, resolves it).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwClaimSourceV2 {
    /// The block's own chain-challenge attempt (algo 6) — the V2 lane, unchanged.
    Attempt,
    /// A free-prompt execution commitment (ADR-0044). `quanta ≥ 1` always: sub-quantum work is
    /// refused at acceptance rather than parked in state it can never act from.
    FreePrompt { quanta: u32, spent: BTreeSet<u32> },
}

/// **One released escrow, waiting for the next block's coinbase to mint it.**
///
/// The queue exists because a payout has to be edge-triggered. A claim that reached `Final` STAYS
/// `Final`, so "pay every Final claim" pays the same claim in every block forever; and the block
/// that finalizes a claim cannot pay it, because its coinbase is fixed before its own PALW
/// transition runs. So finalization ENQUEUES here, and the next block's coinbase drains the queue
/// its parent state carries — a set that is already committed when that coinbase is built, which
/// is what lets construction and validation agree byte-for-byte.
///
/// The payload is snapshotted at release rather than looked up at payment: the bond can retire
/// between the two blocks, and a reward must not become unpayable because its earner left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPayoutV2 {
    /// The 64-byte P2PKH-ML-DSA-87 owner payload, copied from the bond at release.
    pub payload: Hash64,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClaimStateV2 {
    pub source: PalwClaimSourceV2,
    pub class_id: Hash64,
    pub bond: PalwBondKeyV2,
    pub pwu: u64,
    pub accepted_daa: u64,
    pub accepted_blue_score: u64,
    /// The chain block that carried the attempt.
    pub accepted_block: BlockHash,
    /// The attempt's committed step-trace root — what the court adjudicates against (PR-07) and
    /// what DA obligations serve chunks of. Copied into the record because the state IS the
    /// candidate-scoped source consumers read; sending them back to the envelope would be a
    /// second lookup path to diverge in.
    pub trace_root: Hash64,
    /// The attempt's committed output root (output-side disputes bind it).
    pub output_root: Hash64,
    /// The attempt's committed execution root — what a court refutation's binding must equal
    /// before any fault is read out of it (audit C3). Copied for the same reason `trace_root` is:
    /// the state IS the candidate-scoped source consumers read.
    pub execution_root: Hash64,
    /// Number of chunks behind the attempt's trace manifest. An `Unavailable` receipt must name
    /// one of them (audit C5): an accusation that cannot say WHAT was not served is not evidence.
    pub trace_chunk_count: u32,
    /// DAA score through which the producer owes openings and chunks. An `Unavailable` receipt
    /// whose request falls outside it accuses the producer of breaking an obligation it did not
    /// have.
    pub trace_retention_daa: u64,
    /// Collateral value reserved at creation: `pwu × slash_value_per_pwu(class at creation)`.
    /// Snapshotted so release always returns exactly what reserve took, whatever happens to the
    /// class record afterwards.
    pub reserved: u128,
    /// `⌊β·pwu/1000⌋` at creation, for the same snapshot reason.
    pub immature_contribution: u128,
    /// **The producer's carve of the accepting block's subsidy, snapshotted (ADR-0042
    /// Decision 10).**
    ///
    /// Escrowed here at creation and paid out only when the claim reaches `Final`; a `Voided`
    /// claim's escrow is never minted at all. Snapshotting rather than recomputing at payout is
    /// what makes "never an addition to the schedule" true: the carve is taken from the subsidy
    /// that existed when the work was accepted, so no later block's subsidy can be drawn twice.
    pub escrowed_reward: u64,
    pub phase: PalwClaimPhaseV2,
}

/// One seat's answer, as the state machine sees it: who said it, and which way.
///
/// The signature and the DA-obligation proof live in `palw_panel_v2`'s
/// `PalwSeatReceiptV2` and are checked there. What crosses into the transition is the pair the
/// accounting needs — a seat and its verdict — so that a minority which reported against the
/// quorum can be charged for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSeatVerdictV2 {
    pub seat_bond: PalwBondKeyV2,
    /// `true` = the seat said the trace was served and verified; `false` = it said the producer
    /// withheld.
    pub served: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPanelSeatV2 {
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
}

/// A bound panel, keyed by the claim it judges. Draw validity (exclusions, dedup, anchor
/// derivation) is PR-06's acceptance problem; the state records the accepted draw.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPanelStateV2 {
    pub anchor: Hash64,
    pub seats: Vec<PalwPanelSeatV2>,
    pub bound_daa: u64,
}

/// An open court session, keyed by its session id (Decision 8: the id binds the attempt and both
/// bonds; the binding itself is validated where sessions are accepted, PR-07).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCourtSessionStateV2 {
    pub claim: Hash64,
    pub challenger_bond: PalwBondKeyV2,
    pub opened_daa: u64,
    /// `opened_daa + window_court` — the backstop. Past it the session closes challenger-side
    /// at the sweep (prosecution is the challenger's burden; see the params field doc).
    pub deadline_daa: u64,
    /// **The interactive ladder, on chain (P0-9's remaining half).**
    ///
    /// It lives here because `PalwBisectNoShowV1` is "only mintable from the machine's own
    /// state", and until that state was the CHAIN's, a validating node could not tell a real
    /// default from a forged one — so `palw_court_v2` refused ladder defaults outright, and a
    /// dispute could only end arithmetically or by the whole-session backstop.
    ///
    /// With the ladder here, nobody submits a default at all: the rung deadline is a machine
    /// fact, silence past it is visible to every node computing the same state, and the SWEEP
    /// closes the session against whoever was due to move. An offense that is produced by
    /// absence cannot be forged by presence.
    pub ladder: crate::palw_bisect::PalwBisectLadderV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwCourtVerdictV2 {
    /// Fraud proven: the claim is `Voided { CourtFraud }` (unless already terminal — a verdict
    /// arriving after a default-void closes the session and changes nothing else).
    ExecutorGuilty,
    /// The challenge failed; the claim resumes its path to `Final`.
    ChallengerDefeated,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwEpochCounterV2 {
    pub epoch_index: u64,
    pub produced_pwu: u128,
    pub produced_blocks: u64,
}

/// ADR-0045 Decision 2: one epoch's per-class production budgets, in **blocks** — never in pwu
/// (whose currency cancels the share out of its own inequality, amendment defect (e)) and never
/// in ramped weight (defect (a)). Derived once, at the boundary that opens `epoch_index`, from
/// the boundary's own facts; constant for the epoch whatever registrations land mid-flight.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwEpochBudgetsV2 {
    /// The epoch these budgets bound. A budget is only ever comparable within its own epoch.
    pub epoch_index: u64,
    /// Ceiling on each class's `produced_blocks` for the epoch. A class absent here — frozen at
    /// the boundary, or registered mid-epoch — admits nothing until the next boundary; a missing
    /// budget is a missing fact, never a permissive zero.
    pub budget_blocks: BTreeMap<Hash64, u64>,
}

// ---------------------------------------------------------------------------------------------
// Objects and context
// ---------------------------------------------------------------------------------------------

/// One chain point, as the transition consumes it. `blue_score` must strictly increase along a
/// chain and `daa_score` must not decrease; the transition enforces both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBlockContextV2 {
    pub block: BlockHash,
    pub daa_score: u64,
    pub blue_score: u64,
    /// **The block's own coinbase subsidy, in sompi — the pool a claim's escrow is carved FROM.**
    ///
    /// A fact about this block, like `daa_score`, and it must be this block's rather than the
    /// paying block's: ADR-0042 Decision 10 escrows the reward at acceptance and releases it at
    /// `Final`, and "PALW reward is a carve of the fixed subsidy, never an addition to it" is only
    /// true if the carve is taken from the subsidy that actually existed when the work was
    /// accepted. The emission schedule lives in `kaspa-consensus`' coinbase manager, so the value
    /// crosses in rather than being recomputed here — one schedule, not two.
    ///
    /// Zero is legal and means the block funds no escrow.
    pub subsidy: u64,
}

/// **The material a post-genesis class registration must carry** (ADR-0049 Decision H).
///
/// The graph says what the class computes; the canonical job says what one unit of its work is.
/// `verify_class_admission_v2` needs both and can derive neither: the profile id IS the class id,
/// so a chain that did not hold the profile could not tell whether the id was earned, and the
/// canonical job is the registrant's own declaration of what it is paid per.
///
/// Boxed inside the object because it is by far the largest thing a lifecycle transaction carries.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassAdmissionCarriageV2 {
    /// The class's graph. `shape_profile_id()` must equal the registration's `class_id`.
    pub profile: crate::palw_step::PalwShapeProfileV3,
    /// The job the class is paid per, from which `pwu_per_inference` is counted rather than
    /// believed.
    pub canonical: crate::palw_v2::PalwJobContextV2,
}

/// The consensus objects a block can carry into the state, in the block's deterministic
/// acceptance order. ACCEPTANCE (who may say this, with what proof) belongs to later PRs; the
/// transition enforces referential integrity and the lattice.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwConsensusObjectV2 {
    /// Carries the operator's KEY, not an operator id: the id is derived
    /// ([`palw_operator_id_v2`]) so it names something someone must hold rather than a label
    /// anyone can invent. The acceptance layer additionally verifies the registration under that
    /// key; the transition derives the identity so the two cannot disagree about who registered.
    BondRegistered {
        bond: PalwBondKeyV2,
        pubkey: Vec<u8>,
        operator_pubkey: Vec<u8>,
        collateral: u64,
        /// The 64-byte P2PKH-ML-DSA-87 owner payload matured rewards are paid to. Zero is
        /// refused: a bond that names no payee is a bond whose every reward would be minted to a
        /// script nobody can open, and the place to find that out is registration.
        payout_payload: Hash64,
    },
    BondRetireRequested {
        bond: PalwBondKeyV2,
    },
    ClassRegistered {
        class_id: Hash64,
        artifact_root: Hash64,
        slash_value_per_pwu: u64,
        pwu_rule: PalwPwuRuleV2,
        initial_target: u128,
        /// ADR-0045 Decision 3: the entrant's cadence share, in permille — funded by
        /// largest-remainder donation from every incumbent, so the table stays at exactly
        /// 1000‰ through every registration. The FIRST class on a chain must be the base class
        /// at the whole 1000‰. The share rides the same authorized object the class does:
        /// whoever may register a class may fund it, and nobody else may move a permille.
        share_permille: u16,
        /// The DAA score at which the share is actually granted (conditions 12/13).
        ///
        /// `0` means "now", which is what every registration meant before this field and what the
        /// genesis floor still means. A future score registers the class WEIGHTLESS — adjudicable,
        /// disputable, and holding no cadence — so it can be soaked on a live chain before it
        /// takes a permille from anyone. See [`PalwClassStatusV2::Registered`].
        activation_daa: u64,
        /// **What a POST-GENESIS registration must carry to be checkable (ADR-0049 Decision H).**
        ///
        /// Three policies coexisted: the lifecycle carriage refused `ClassRegistered` outright,
        /// `verify_class_admission_v2` would have admitted it at the minimum grantable share, and
        /// the state machine implements a weightless activation clock. Consensus does not benefit
        /// from three answers, and the carriage's objection was the correct one — but it is a
        /// statement about CHECKING, not about forbidding: a class entering a live chain moves the
        /// share table and brings its own `pwu_rule`, and nothing checked either. Decisions C and D
        /// are that check, so the refusal is replaced by the gate rather than removed.
        ///
        /// `None` is a GENESIS registration, whose class is checked against the catalog the ruleset
        /// id commits to (`verify_palw_genesis_v2`) — the catalog IS the profile in committed form,
        /// so carrying it again would be a second copy to disagree with.
        ///
        /// `Some` is a registration on a running chain, where there is no catalog to check
        /// against: the object carries the graph and the canonical job, and
        /// `verify_class_admission_v2` decides. It has to carry them anyway — nothing else on a
        /// running chain can tell the court what the class computes.
        admission: Option<Box<PalwClassAdmissionCarriageV2>>,
    },
    /// **The emergency off-switch, and it carries its own proof (gate item 12).**
    ///
    /// This used to be `ClassFrozen { class_id }` — a bare instruction any block's object list
    /// could contain, checked only for "the class exists and is Active". A network running it
    /// could be halted by one block naming the liveness floor: freeze BASE-0 and no class can
    /// produce, which is the whole chain. An off-switch anyone may pull is not a safety
    /// mechanism, it is the attack it was built to survive.
    ///
    /// The freeze is OBJECTIVE instead (ADR-0027 §5, the gate ledger §2): the evidence is a
    /// class-contradiction certificate — two attestations binding one job context under one
    /// class, disagreeing about what that job produced. That is the class's own determinism
    /// claim refuted by its own participants, and it needs no governance step because nobody
    /// decided it. The transition checks the structural half here; signatures are the acceptance
    /// layer's, exactly as they are for `BondRegistered`.
    ClassFrozen {
        class_id: Hash64,
        certificate: crate::palw_slash::PalwClassContradictionCertificateV1,
    },
    // **There is deliberately no `ClassUnfrozen`, and its absence is the design.**
    //
    // The variant that stood here accepted `{ class_id }` with no evidence and no authority, so
    // the pair composed into a switch anyone could flip in either direction. The gate ledger's
    // rule is that re-activation "re-runs the full §12 gate from zero-credit" — a coordinated
    // release action with an audit trail, not something a block can assert. And a chain-level
    // unfreeze is the freeze's own undoing: it turns an objective, permanent consequence into a
    // temporary one, which is exactly what an attacker holding the emit path would want. A class
    // whose determinism has been refuted ON THIS CHAIN stays refuted; bringing the model back
    // means registering a NEW class id, with its own catalog entry and registration — which IS
    // the audit trail the ledger asks for, expressed as chain state rather than a promise.
    PanelBound {
        claim: Hash64,
        anchor: Hash64,
        seats: Vec<PalwPanelSeatV2>,
    },
    /// Carries the receipt set the acceptance layer validated (audit C5). The transition does
    /// not re-verify signatures — that is `palw_panel_v2`'s job and the module boundary is
    /// deliberate — but it READS the verdicts, because a seat that reported the opposite of what
    /// the panel concluded is contradicted by the panel's own quorum, and a contradiction with
    /// no cost is an invitation.
    ReceiptLicensed {
        claim: Hash64,
        receipts: Vec<PalwSeatVerdictV2>,
    },
    CourtOpened {
        session_id: Hash64,
        claim: Hash64,
        challenger_bond: PalwBondKeyV2,
        /// The index space the dispute is over, and its size. Carried because the ladder is
        /// opened here now: both are already inside `session_id`
        /// (`court_session_id_v2`), so a mismatch cannot pass the acceptance layer's id
        /// derivation — they are declared for the transition to build from, not trusted.
        space: crate::palw_bisect::PalwBisectSpaceV1,
        space_size: u64,
    },
    CourtClosed {
        session_id: Hash64,
        /// The verdict this close asserts. DECLARED, and therefore checked: the acceptance layer
        /// re-derives it from `proof` and refuses the object if the two disagree. It stays on the
        /// object because the transition acts on it, and the transition does not run arithmetic.
        verdict: PalwCourtVerdictV2,
        /// **What makes the verdict checkable (ADR-0042 Decision 8).**
        ///
        /// A close used to carry a verdict and nothing else, so a validating node had no way to
        /// tell a proven fault from an assertion — and refused every close rather than trust one.
        /// The proof rides here: operand openings against the CLASS's registered artifact root,
        /// and a refutation bound to the CLAIM's committed roots. A proof that does not
        /// adjudicate mints neither verdict; it refuses the close.
        proof: crate::palw_court_v2::PalwCourtVerdictProofV2,
    },
    /// The responder's rung: "my execution's state at the disputed midpoint is `mid_state`".
    ///
    /// The signature is the acceptance layer's to check, exactly as it is for `BondRegistered` —
    /// but here it is not a formality: an unsigned disclosure would let the CHALLENGER write the
    /// responder's answers, bind it to states it never claimed, and win the terminal check
    /// against an honest execution. `palw_court_v2` verifies it under the claim's bond key.
    CourtDisclosed {
        session_id: Hash64,
        disclosure: crate::palw_bisect::PalwBisectDisclosureV1,
        signature: Vec<u8>,
    },
    /// The challenger's rung verdict, and the mirror-image danger: an unsigned verdict would let
    /// the RESPONDER steer the interval away from its own divergence. Verified under the
    /// challenger bond's key.
    CourtVerdictPosted {
        session_id: Hash64,
        verdict: crate::palw_bisect::PalwBisectVerdictV1,
        signature: Vec<u8>,
    },
    /// Decision 7's producer default: a data obligation missed its deadline.
    ProducerDefaulted {
        claim: Hash64,
        receipts: Vec<PalwSeatVerdictV2>,
    },
    /// ADR-0044 (FP-03): a free-prompt execution commitment, accepted on chain. Creates a claim
    /// at `Provisional` with `source: FreePrompt` — from there the lattice runs unmodified
    /// (panel from a beacon anchor, quorum, court, sweeps). `pwu` is the claim's TOTAL potential
    /// block weight (`quanta × per-quantum weight`, enforced uniform here); the acceptance layer
    /// derived it and `quanta` from the commitment's CU under the bundle's rule and is the layer
    /// that checked them — this transition enforces referential integrity, uniformity, and the
    /// accounting, exactly as it trusts an attempt's admission-checked `pwu`.
    FreePromptCommitted {
        claim: Hash64,
        class_id: Hash64,
        bond: PalwBondKeyV2,
        pwu: u64,
        quanta: u32,
        trace_root: Hash64,
        output_root: Hash64,
        /// The DA/court trio an attempt carries in its envelope, carried here instead because a
        /// free-prompt claim reaches the SAME panel and the same court (audit C3/C5): a claim
        /// whose record cannot say what the producer owes, or what a refutation's binding must
        /// equal, is a claim no accusation can be evidence about.
        execution_root: Hash64,
        trace_chunk_count: u32,
        trace_retention_daa: u64,
    },
}

/// The block's own work slot, as the V3 transition consumes it (ADR-0044): a chain-challenge
/// attempt (algo 6), a certified-receipt quantum spend (algo 7), or none (a hash-lane or
/// object-only application). Exactly one per block — a block IS one unit of work.
#[derive(Clone, Copy, Debug)]
pub enum PalwBlockWorkV3<'a> {
    None,
    Attempt(&'a PalwAttemptEnvelopeV2),
    ReceiptSpend(&'a PalwReceiptSpendUnsignedV3),
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwStateV2Error {
    #[error("invalid state params: {0}")]
    InvalidParams(&'static str),
    #[error("chain context is not monotone: {0}")]
    NonMonotonicContext(&'static str),
    #[error("bond {0:?} does not exist at this chain point")]
    MissingBond(PalwBondKeyV2),
    #[error("bond {0:?} already exists")]
    DuplicateBond(PalwBondKeyV2),
    #[error("bond {0:?} is already retiring")]
    BondAlreadyRetiring(PalwBondKeyV2),
    #[error("bond {0:?} is retiring and may take no new claims")]
    RetiringBond(PalwBondKeyV2),
    #[error("class {0} does not exist at this chain point")]
    MissingClass(Hash64),
    #[error("class {0} already exists")]
    DuplicateClass(Hash64),
    #[error("class {0} is frozen")]
    FrozenClass(Hash64),
    #[error("class {0} is not frozen")]
    ClassNotFrozen(Hash64),
    #[error("class {0} is the liveness floor and may not be frozen (ADR-0039 W6′) — freezing it ends the chain")]
    BaseClassMayNotFreeze(Hash64),
    #[error("claim {0} does not exist at this chain point")]
    MissingClaim(Hash64),
    #[error("claim {0} already exists (one attempt id, one claim)")]
    DuplicateClaim(Hash64),
    #[error("claim {claim} is in the wrong phase for {edge}")]
    WrongPhase { claim: Hash64, edge: &'static str },
    #[error("court session {0} does not exist at this chain point")]
    MissingSession(Hash64),
    #[error("court session {0} already exists")]
    DuplicateSession(Hash64),
    #[error("arithmetic overflow in {0} — an overflowing chain is invalid, not saturated")]
    Overflow(&'static str),
    #[error("panel has no seats")]
    EmptyPanel,
    #[error("carriage is inconsistent with itself: {0}")]
    CarriageInconsistent(String),
    #[error("delta does not apply to this state: {0}")]
    DeltaMismatch(&'static str),
    #[error("no state stored for parent block {0}")]
    MissingParentState(BlockHash),
    #[error("class {0} registered with a zero target — an unmeetable difficulty is a dead class in costume")]
    ZeroClassTarget(Hash64),
    #[error("bond {bond:?} registered {got} sompi, below the network's minimum collateral")]
    CollateralBelowMinimum { bond: PalwBondKeyV2, got: u64 },
    #[error("bond {0:?} registered an empty operator key — an operator identity must name a key someone holds")]
    EmptyOperatorKey(PalwBondKeyV2),
    #[error("bond {0:?} registered a zero payout payload — every reward it matured would be minted to a script nobody can open")]
    EmptyPayoutPayload(PalwBondKeyV2),
    #[error("court session {0} refused a ladder move: {1}")]
    LadderRefused(Hash64, String),
    #[error("court session {0} was opened over a space its own session id does not name")]
    SessionIdMismatch(Hash64),
    #[error("class {0} was registered with a zero slash value — its work risks no collateral, so the exposure ceiling is not a ceiling")]
    ZeroSlashValue(Hash64),
    #[error(
        "class {0} was registered with a zero pwu_per_inference — a class whose canonical inference costs nothing is not a PALW class"
    )]
    ZeroPwuPerInference(Hash64),
    #[error("class {0} was registered with a zero pwu ceiling — a rule that licenses no attempt is a frozen class in costume")]
    ZeroPwuCeiling(Hash64),
    #[error("per-class retarget failed: {0} — the closed span's facts must satisfy the rule or the block is invalid")]
    Retarget(String),
    #[error("the first class on a chain must be the base class {base}, not {class_id} — the liveness floor exists before anything else does (ADR-0039 W6′)")]
    FirstClassMustBeTheBase { class_id: Hash64, base: Hash64 },
    #[error("the first class must take the whole 1000‰, got {got}‰ — an unallocated permille is a half-funded floor")]
    FirstShareMustBeWhole { got: u16 },
    #[error("share {got}‰ is outside the table's denominator")]
    ShareOutOfRange { got: u16 },
    #[error(
        "class {class_id} was granted {share}‰, below the grant floor {floor}‰ — a share whose worst-case epoch budget is zero blocks is a class that cannot exist (ADR-0045 Decision 2)"
    )]
    ShareBelowGrantFloor { class_id: Hash64, share: u16, floor: u16 },
    #[error(
        "the grant would leave donor {donor} holding {would_hold}‰, below the grant floor {floor}‰ — a registration may not starve an incumbent to fund itself"
    )]
    DonationBreaksGrantFloor { donor: Hash64, would_hold: u16, floor: u16 },
    #[error("claim {0} is not a free-prompt claim — an attempt's work was already weighed at its own block")]
    NotFreePromptClaim(Hash64),
    #[error("free-prompt claim {claim} has {quanta} quanta; index {index} does not exist")]
    QuantumOutOfRange { claim: Hash64, index: u32, quanta: u32 },
    #[error("free-prompt claim {claim} quantum {index} is already spent on this chain")]
    QuantumAlreadySpent { claim: Hash64, index: u32 },
    #[error("a free-prompt commitment with zero quanta licenses nothing and does not enter the state")]
    ZeroQuanta,
    #[error("free-prompt pwu {pwu} does not divide into {quanta} uniform non-zero quanta")]
    NonUniformQuanta { pwu: u64, quanta: u32 },
    #[error("class {frozen} cannot be frozen on evidence about class {evidenced}")]
    ContradictionNamesAnotherClass { frozen: Hash64, evidenced: Hash64 },
    #[error("the class-contradiction certificate proves nothing: {0}")]
    ContradictionNotProven(&'static str),
    #[error(
        "free-prompt claim {0} carries a null execution root — the court would have nothing to bind a refutation to,          so this claim could never be convicted of arithmetic fraud (audit C3, free-prompt lane)"
    )]
    UnadjudicableCommitment(Hash64),
}

// ---------------------------------------------------------------------------------------------
// The state
// ---------------------------------------------------------------------------------------------

/// The candidate-scoped PALW state at one chain point. Fields are private on purpose: the ONLY
/// writer is [`apply_palw_transition_v2`] (and delta application, which cross-checks itself), so
/// the type cannot acquire a fact from anywhere but the candidate chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwChainStateV2 {
    // ---- primary data: everything the root commits to ----
    bonds: BTreeMap<PalwBondKeyV2, PalwBondStateV2>,
    /// Maintained accumulator of Decision 6's `reserved_exposure(bond)`. Derivable from `claims`;
    /// kept so admission's ceiling check is O(log n), and cross-checked by
    /// [`Self::assert_internal_consistency`]. An entry exists iff its value is non-zero.
    reserved_exposure: BTreeMap<PalwBondKeyV2, u128>,
    classes: BTreeMap<Hash64, PalwClassStateV2>,
    class_targets: BTreeMap<Hash64, PalwClassTargetV2>,
    /// ADR-0045 Decision 3: the difficulty-domain share table, in permille. Granted by
    /// `ClassRegistered` (the first must be the base class at 1000‰; every later entrant is
    /// funded by largest-remainder donation), conserved to exactly 1000‰ at every mutation, and
    /// keyed identically to `classes` — a registered class holds a share, a share names a
    /// registered class, and `assert_internal_consistency` refuses a state where either
    /// direction fails. Freeze and unfreeze move NOTHING here: absence is answered by the H1
    /// census at the two places absence is measured (the retarget's expectation and the epoch
    /// budget's denominator), never by mutating the allocation of record.
    class_shares: BTreeMap<Hash64, u16>,
    /// ADR-0045 Decision 2: the current epoch's block-denominated budgets, frozen at the
    /// boundary that opened the epoch. Rooted now, in this field position, so the Decision 2
    /// wiring (the boundary derivation and the admission predicate reading it) lands without
    /// changing the root derivation of every earlier state — the same reasoning `capabilities`
    /// used. Written by nothing until that wiring; `None` from genesis.
    epoch_budgets: Option<PalwEpochBudgetsV2>,
    /// The receipt lane's per-class difficulty (ADR-0044 Decision 5's `receipt_target[class]`).
    /// Seeded at class registration from the same initial target as the attempt lane; the
    /// per-lane retargets separate them from there.
    receipt_targets: BTreeMap<Hash64, PalwClassTargetV2>,
    capabilities: BTreeMap<Hash64, PalwCapabilityStateV2>,
    claims: BTreeMap<Hash64, PalwClaimStateV2>,
    panels: BTreeMap<Hash64, PalwPanelStateV2>,
    court_sessions: BTreeMap<Hash64, PalwCourtSessionStateV2>,
    epoch_counters: BTreeMap<Hash64, PalwEpochCounterV2>,
    /// Receipt-lane production census per class (spent quanta as blocks/pwu), feeding the
    /// receipt-lane retarget exactly as `epoch_counters` feeds the attempt lane's.
    receipt_epoch_counters: BTreeMap<Hash64, PalwEpochCounterV2>,
    /// Escrows released by the LAST applied block, keyed by claim — the exact set the next
    /// block's coinbase must pay, and nothing else. Cleared at the head of every transition (the
    /// block being applied is the one that paid them), then refilled by whatever that block
    /// finalizes. Hashed into `state_root`, so a miner cannot pay a queue nobody else has.
    pending_payouts: BTreeMap<Hash64, PalwPayoutV2>,
    safe_weight: u128,
    bounded_immature: u128,
    safe_frontier_blue_score: u64,
    safe_frontier: BlockHash,
    last_point: Option<PalwBlockContextV2>,

    // ---- indices: rebuildable, never serialized, never hashed ----
    /// `(deadline_daa, claim)` — the sweep queue. A claim has at most one live deadline.
    deadlines: BTreeSet<(u64, Hash64)>,
    /// `(accepted_blue_score, claim)` for every non-terminal claim — what the frontier reads.
    unresolved: BTreeSet<(u64, Hash64)>,
    /// Open court sessions per claim — what gates `ReceiptLicensed → Final`.
    open_courts_by_claim: BTreeMap<Hash64, u32>,
    /// `(deadline_daa, session)` — the court backstop sweep queue. Exactly rebuildable from the
    /// session records (each carries its deadline), so never serialized, never hashed.
    court_deadlines: BTreeSet<(u64, Hash64)>,
}

impl PalwChainStateV2 {
    /// The empty state a V2 network boots from. No bonds, no classes, no weight, frontier at the
    /// zero point.
    pub fn genesis() -> Self {
        Self {
            bonds: BTreeMap::new(),
            reserved_exposure: BTreeMap::new(),
            classes: BTreeMap::new(),
            class_targets: BTreeMap::new(),
            class_shares: BTreeMap::new(),
            epoch_budgets: None,
            receipt_targets: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            pending_payouts: BTreeMap::new(),
            claims: BTreeMap::new(),
            panels: BTreeMap::new(),
            court_sessions: BTreeMap::new(),
            epoch_counters: BTreeMap::new(),
            receipt_epoch_counters: BTreeMap::new(),
            safe_weight: 0,
            bounded_immature: 0,
            safe_frontier_blue_score: 0,
            safe_frontier: ZERO_HASH64,
            last_point: None,
            deadlines: BTreeSet::new(),
            unresolved: BTreeSet::new(),
            open_courts_by_claim: BTreeMap::new(),
            court_deadlines: BTreeSet::new(),
        }
    }

    // ---- read access (no mutators are public) ----

    pub fn bond(&self, key: &PalwBondKeyV2) -> Option<&PalwBondStateV2> {
        self.bonds.get(key)
    }

    pub fn class(&self, id: &Hash64) -> Option<&PalwClassStateV2> {
        self.classes.get(id)
    }

    pub fn class_target(&self, id: &Hash64) -> Option<&PalwClassTargetV2> {
        self.class_targets.get(id)
    }

    /// The class's share of cadence, in permille (ADR-0045 Decision 3). `None` for a class this
    /// chain never registered — and only for those: registration grants, nothing revokes.
    pub fn class_share_permille(&self, id: &Hash64) -> Option<u16> {
        self.class_shares.get(id).copied()
    }

    /// The whole share table, in canonical order — what the boundary derivations fold over.
    pub fn class_shares_iter(&self) -> impl Iterator<Item = (&Hash64, &u16)> {
        self.class_shares.iter()
    }

    /// The current epoch's frozen budgets, if a boundary has written them (ADR-0045 Decision 2).
    pub fn epoch_budgets(&self) -> Option<&PalwEpochBudgetsV2> {
        self.epoch_budgets.as_ref()
    }

    /// The receipt lane's per-class target — what a quantum ticket is admitted against (FP-04).
    pub fn receipt_target(&self, id: &Hash64) -> Option<&PalwClassTargetV2> {
        self.receipt_targets.get(id)
    }

    pub fn receipt_epoch_counter(&self, class_id: &Hash64) -> Option<&PalwEpochCounterV2> {
        self.receipt_epoch_counters.get(class_id)
    }

    /// Every claim this state holds, in canonical order — what a fold over the lattice reads
    /// (ADR-0042 Decision 10's maturity walk, and any consumer that must ask a question of the
    /// whole claim set rather than of one id).
    pub fn claims_iter(&self) -> impl Iterator<Item = (&Hash64, &PalwClaimStateV2)> {
        self.claims.iter()
    }

    pub fn claim(&self, id: &Hash64) -> Option<&PalwClaimStateV2> {
        self.claims.get(id)
    }

    /// The escrows this state released — what the NEXT block's coinbase must pay, in claim-id
    /// order. Empty on every state whose last block finalized nothing, which is most of them.
    pub fn pending_payouts_iter(&self) -> impl Iterator<Item = (&Hash64, &PalwPayoutV2)> {
        self.pending_payouts.iter()
    }

    pub fn panel(&self, claim: &Hash64) -> Option<&PalwPanelStateV2> {
        self.panels.get(claim)
    }

    pub fn court_session(&self, id: &Hash64) -> Option<&PalwCourtSessionStateV2> {
        self.court_sessions.get(id)
    }

    pub fn epoch_counter(&self, class_id: &Hash64) -> Option<&PalwEpochCounterV2> {
        self.epoch_counters.get(class_id)
    }

    /// Iterate the bond registry in canonical key order — what the panel sortition tickets.
    pub fn bonds_iter(&self) -> impl Iterator<Item = (&PalwBondKeyV2, &PalwBondStateV2)> {
        self.bonds.iter()
    }

    /// Decision 6's `reserved_exposure(bond)` — what the admission ceiling is checked against.
    pub fn reserved_exposure(&self, key: &PalwBondKeyV2) -> u128 {
        self.reserved_exposure.get(key).copied().unwrap_or(0)
    }

    pub fn safe_weight(&self) -> u128 {
        self.safe_weight
    }

    pub fn bounded_immature(&self) -> u128 {
        self.bounded_immature
    }

    pub fn safe_frontier(&self) -> (u64, BlockHash) {
        (self.safe_frontier_blue_score, self.safe_frontier)
    }

    pub fn last_point(&self) -> Option<&PalwBlockContextV2> {
        self.last_point.as_ref()
    }

    /// This state's standing in fork choice, through the one comparator (Decision 9).
    /// `live_total` is constructed by [`PalwCandidateOrderV1::new`], never stored here, so a
    /// maturing claim cannot lower it.
    pub fn candidate_order(&self, candidate: Hash64) -> PalwCandidateOrderV1 {
        PalwCandidateOrderV1::new(self.safe_frontier_blue_score, self.safe_weight, self.bounded_immature, candidate)
    }

    // ---- the root ----

    /// The state root: version, then every collection root in the struct's declared order, then
    /// the scalars. The exact ordering is frozen in ADR-0043 — extended in place (still pre-wire,
    /// nothing had committed a root) by ADR-0044 with the two receipt-lane collections, each
    /// placed directly after its attempt-lane counterpart, and again by ADR-0045 with the share
    /// table and epoch budgets — the two extensions merged under the version bump to 3. Changing
    /// it again — or what any collection's entry encoding covers — is a consensus change and
    /// needs a new domain string or version.
    pub fn state_root(&self) -> Hash64 {
        let mut state = keyed(PALW_STATE_V2_DOMAIN_STATE_ROOT);
        state.update(&PALW_STATE_V2_VERSION.to_le_bytes());
        state.update(collection_root(b"bonds", &self.bonds).as_byte_slice());
        state.update(collection_root(b"reserved_exposure", &self.reserved_exposure).as_byte_slice());
        state.update(collection_root(b"classes", &self.classes).as_byte_slice());
        state.update(collection_root(b"class_targets", &self.class_targets).as_byte_slice());
        state.update(collection_root(b"class_shares", &self.class_shares).as_byte_slice());
        match &self.epoch_budgets {
            None => {
                state.update(&[0u8]);
            }
            Some(budgets) => {
                state.update(&[1u8]);
                state.update(&borsh::to_vec(budgets).expect("PalwEpochBudgetsV2 is borsh-serializable"));
            }
        }
        state.update(collection_root(b"receipt_targets", &self.receipt_targets).as_byte_slice());
        state.update(collection_root(b"capabilities", &self.capabilities).as_byte_slice());
        state.update(collection_root(b"claims", &self.claims).as_byte_slice());
        state.update(collection_root(b"pending_payouts", &self.pending_payouts).as_byte_slice());
        state.update(collection_root(b"panels", &self.panels).as_byte_slice());
        state.update(collection_root(b"court_sessions", &self.court_sessions).as_byte_slice());
        state.update(collection_root(b"epoch_counters", &self.epoch_counters).as_byte_slice());
        state.update(collection_root(b"receipt_epoch_counters", &self.receipt_epoch_counters).as_byte_slice());
        state.update(&self.safe_weight.to_le_bytes());
        state.update(&self.bounded_immature.to_le_bytes());
        state.update(&self.safe_frontier_blue_score.to_le_bytes());
        state.update(self.safe_frontier.as_byte_slice());
        match &self.last_point {
            None => {
                state.update(&[0u8]);
            }
            Some(p) => {
                state.update(&[1u8]);
                state.update(&borsh::to_vec(p).expect("PalwBlockContextV2 is borsh-serializable"));
            }
        }
        finish(state)
    }

    // ---- consistency ----

    /// Recomputes every derivable fact from the primary data and cross-checks the maintained
    /// copies: the exposure accumulator, both weights, and all three indices. Carriage loading
    /// runs this unconditionally; the differential tests run it after every apply.
    /// Takes `params` because one derivable fact needs them: audit C5's abandon hold is a
    /// function of the claim record AND the configured hold span, so an accumulator that could be
    /// recomputed without them would be one the hold is invisible to.
    pub fn assert_internal_consistency(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        let mut exposure: BTreeMap<PalwBondKeyV2, u128> = BTreeMap::new();
        let mut safe: u128 = 0;
        let mut immature: u128 = 0;
        let mut unresolved: BTreeSet<(u64, Hash64)> = BTreeSet::new();
        for (id, claim) in &self.claims {
            // Free-prompt structural facts hold in EVERY phase: quanta ≥ 1, uniform quanta, the
            // ledger inside range, spends only on a certified (Final) claim, and zero immature
            // contribution (a commitment is not a block's work).
            if let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source {
                if *quanta == 0 || claim.pwu % (*quanta as u64) != 0 || claim.pwu / (*quanta as u64) == 0 {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!("free-prompt claim {id} has non-uniform quanta")));
                }
                if spent.iter().any(|q| *q >= *quanta) {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!("free-prompt claim {id} spent an absent quantum")));
                }
                if !spent.is_empty() && !matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!("free-prompt claim {id} spent before Final")));
                }
                if claim.immature_contribution != 0 {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "free-prompt claim {id} carries immature contribution — commitments must not pump live weight"
                    )));
                }
            }
            match claim.phase {
                PalwClaimPhaseV2::Final { .. } => match &claim.source {
                    // An attempt's Final IS its block's certified work.
                    PalwClaimSourceV2::Attempt => {
                        safe = safe.checked_add(claim.pwu as u128).ok_or(PalwStateV2Error::Overflow("consistency safe"))?;
                    }
                    // A free-prompt Final licenses; only SPENT quanta weighed blocks.
                    PalwClaimSourceV2::FreePrompt { quanta, spent } => {
                        let per_quantum = (claim.pwu / (*quanta as u64)) as u128;
                        let spent_weight = per_quantum
                            .checked_mul(spent.len() as u128)
                            .ok_or(PalwStateV2Error::Overflow("consistency spent weight"))?;
                        safe = safe.checked_add(spent_weight).ok_or(PalwStateV2Error::Overflow("consistency safe"))?;
                    }
                },
                // A voided claim weighs nothing and holds no place in the frontier queue. It may
                // still hold COLLATERAL: audit C5's abandon hold keeps an abandoned free-prompt
                // commitment's reservation for a span, so a panel redraw costs collateral rather
                // than a transaction fee. Recomputed here from the record and the current point,
                // never from a stored flag, so the accumulator and the record cannot disagree.
                PalwClaimPhaseV2::Voided { .. } => {
                    if let Some(point) = &self.last_point
                        && palw_claim_is_on_abandon_hold_v2(claim, params, point.daa_score)
                    {
                        let entry = exposure.entry(claim.bond).or_insert(0);
                        *entry = entry.checked_add(claim.reserved).ok_or(PalwStateV2Error::Overflow("consistency exposure"))?;
                    }
                }
                _ => {
                    immature =
                        immature.checked_add(claim.immature_contribution).ok_or(PalwStateV2Error::Overflow("consistency immature"))?;
                    let entry = exposure.entry(claim.bond).or_insert(0);
                    *entry = entry.checked_add(claim.reserved).ok_or(PalwStateV2Error::Overflow("consistency exposure"))?;
                    unresolved.insert((claim.accepted_blue_score, *id));
                }
            }
        }
        let mut open_courts: BTreeMap<Hash64, u32> = BTreeMap::new();
        for session in self.court_sessions.values() {
            *open_courts.entry(session.claim).or_insert(0) += 1;
        }
        // ADR-0045 Decision 3: the share table's invariants hold at EVERY state, not once at
        // boot. The donation arithmetic conserves the denominator, so a populated table sums to
        // exactly 1000‰, and every share names a class.
        //
        // The key sets are NOT equal, and the difference is exactly the weightless registrations
        // (conditions 12/13): a class awaiting its activation edge is in the registry — it is
        // adjudicable, and a dispute against it must resolve — while holding no permille. What
        // must hold is the two directions separately: every SHARE names a class, and every
        // ACTIVE-or-FROZEN class holds a share. A `Registered` one holds none, by construction.
        let share_bearing: BTreeSet<&Hash64> = self
            .classes
            .iter()
            .filter(|(_, c)| !matches!(c.status, PalwClassStatusV2::Registered { .. }))
            .map(|(id, _)| id)
            .collect();
        if !self.class_shares.keys().collect::<BTreeSet<_>>().eq(&share_bearing) {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "the class set and the share table disagree — every share names a class, and every class past its activation edge holds a share".into(),
            ));
        }
        if !self.class_shares.is_empty() {
            let sum: u32 = self.class_shares.values().map(|s| *s as u32).sum();
            if sum != 1000 {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "the share table sums to {sum}‰ — the donation arithmetic conserves exactly 1000‰"
                )));
            }
        }
        if exposure != self.reserved_exposure {
            return Err(PalwStateV2Error::CarriageInconsistent("reserved_exposure differs from the claims it summarizes".into()));
        }
        if safe != self.safe_weight {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "safe_weight {} differs from the Final claims' sum {safe}",
                self.safe_weight
            )));
        }
        if immature != self.bounded_immature {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "bounded_immature {} differs from the immature claims' sum {immature}",
                self.bounded_immature
            )));
        }
        if unresolved != self.unresolved {
            return Err(PalwStateV2Error::CarriageInconsistent("unresolved index differs from the claims".into()));
        }
        if open_courts != self.open_courts_by_claim {
            return Err(PalwStateV2Error::CarriageInconsistent("open-court index differs from the sessions".into()));
        }
        // The SAME key the writer and the rebuild use — whichever of the session's two clocks
        // runs out first. Recomputing it any other way here would make the checker disagree with
        // the index it is checking, which is how a ladder move looked like corruption.
        let court_deadlines: BTreeSet<(u64, Hash64)> =
            self.court_sessions.iter().map(|(id, s)| (court_next_deadline_v2(s), *id)).collect();
        if court_deadlines != self.court_deadlines {
            return Err(PalwStateV2Error::CarriageInconsistent("court-deadline index differs from the sessions".into()));
        }
        // Every deadline belongs to a live, non-terminal claim in the phase its kind implies —
        // and every non-terminal claim without an open court has exactly one deadline.
        let mut expected_deadlines: BTreeSet<(u64, Hash64)> = BTreeSet::new();
        for (id, claim) in &self.claims {
            if let Some(deadline) = expected_deadline(claim, self.open_courts_by_claim.get(id).copied().unwrap_or(0)) {
                expected_deadlines.insert((deadline_with_params(&claim.phase, deadline), *id));
            }
        }
        // `expected_deadline` needs params to compute the exact daa; consistency without params
        // checks membership shape only. See `assert_deadline_consistency` for the parameterized
        // check used everywhere a params handle exists.
        //
        // Audit C5's abandon hold widens this into a RANGE rather than an equality: a
        // `BindTimeout`-voided free-prompt claim owns a deadline while its hold runs and none
        // afterwards, and without params this check cannot tell which. The exact count is the
        // parameterized check's business; here the abandoned claims are the slack.
        let holdable = self.claims.values().filter(|c| abandon_hold_may_hold_a_deadline(c)).count();
        let low = expected_deadlines.len();
        let high = low + holdable;
        if self.deadlines.len() < low || self.deadlines.len() > high {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "deadline index holds {} entries, the claims imply {low}..={high}",
                self.deadlines.len()
            )));
        }
        Ok(())
    }

    /// The parameterized deadline check: with params in hand the exact deadline of every
    /// non-terminal claim is recomputable, and the index must match it entry for entry.
    pub fn assert_deadline_consistency(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        let mut expected: BTreeSet<(u64, Hash64)> = BTreeSet::new();
        for (id, claim) in &self.claims {
            let open_courts = self.open_courts_by_claim.get(id).copied().unwrap_or(0);
            match claim.phase {
                PalwClaimPhaseV2::Provisional => {
                    expected.insert((
                        claim.accepted_daa.checked_add(params.window_bind).ok_or(PalwStateV2Error::Overflow("bind deadline"))?,
                        *id,
                    ));
                }
                PalwClaimPhaseV2::PanelBound { bound_daa } => {
                    expected.insert((
                        bound_daa.checked_add(params.window_receipt).ok_or(PalwStateV2Error::Overflow("receipt deadline"))?,
                        *id,
                    ));
                }
                PalwClaimPhaseV2::ReceiptLicensed { .. } if open_courts > 0 => {
                    // Final is gated by the court; no deadline until the last session closes.
                }
                PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => {
                    // The stored deadline may be LATER than licensed+window when a court cleared
                    // after the window had already passed (the re-arm rule). Membership by claim,
                    // with the stored daa at least the licensed floor, is the checkable fact.
                    let floor =
                        licensed_daa.checked_add(params.window_challenge).ok_or(PalwStateV2Error::Overflow("challenge deadline"))?;
                    let stored = self
                        .deadlines
                        .iter()
                        .find(|(_, claim_id)| claim_id == id)
                        .ok_or_else(|| PalwStateV2Error::CarriageInconsistent(format!("claim {id} has no final deadline")))?;
                    if stored.0 < floor {
                        return Err(PalwStateV2Error::CarriageInconsistent(format!(
                            "claim {id} final deadline {} sits below its licensed floor {floor}",
                            stored.0
                        )));
                    }
                    expected.insert(*stored);
                }
                // Audit C5's abandon hold: the ONE terminal phase that owes a deadline. It is
                // `voided_daa + hold`, derived from the record exactly as `void_claim` armed it —
                // which is what keeps the index rebuildable from the claims alone.
                PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::BindTimeout }
                    if params.fp_abandon_hold_daa > 0 && matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }) =>
                {
                    let release_at =
                        voided_daa.checked_add(params.fp_abandon_hold_daa).ok_or(PalwStateV2Error::Overflow("abandon hold"))?;
                    // Swept holds leave no entry: after the release block the claim is an ordinary
                    // terminal record. `last_point` is what says whether the sweep has happened.
                    if self.last_point.as_ref().is_none_or(|point| point.daa_score <= release_at) {
                        expected.insert((release_at, *id));
                    }
                }
                PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => {}
            }
        }
        if expected != self.deadlines {
            return Err(PalwStateV2Error::CarriageInconsistent("deadline index differs from the claims' recomputed deadlines".into()));
        }
        Ok(())
    }
}

/// Shape-level helper for `assert_internal_consistency`: does this
/// claim, in this phase with this many open courts, owe the index a deadline entry at all?
fn expected_deadline(claim: &PalwClaimStateV2, open_courts: u32) -> Option<u64> {
    match claim.phase {
        PalwClaimPhaseV2::Provisional => Some(claim.accepted_daa),
        PalwClaimPhaseV2::PanelBound { bound_daa } => Some(bound_daa),
        PalwClaimPhaseV2::ReceiptLicensed { .. } if open_courts > 0 => None,
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => Some(licensed_daa),
        // The shape-level check counts entries without params, so it cannot tell a live abandon
        // hold from a swept one — see `abandon_hold_may_hold_a_deadline` below, which is the
        // parameterless check's whole knowledge of the hold.
        PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => None,
    }
}

/// Whether a claim MAY own a deadline entry despite being terminal (audit C5's abandon hold).
/// Params-free by necessity — the shape check has none — so it answers "possible", not "exact".
fn abandon_hold_may_hold_a_deadline(claim: &PalwClaimStateV2) -> bool {
    matches!(claim.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. })
        && matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. })
}

/// Placeholder anchor for the shape-level count (the parameterless check counts entries; the
/// parameterized one recomputes them exactly).
fn deadline_with_params(_phase: &PalwClaimPhaseV2, anchor: u64) -> u64 {
    anchor
}

/// One collection's root: `H(domain ‖ label ‖ count ‖ (len(key) ‖ key ‖ len(record) ‖ record)*)`
/// over the map's (already canonical) key order. Length prefixes keep adjacent entries from
/// bleeding into each other; the label keeps two same-shaped collections from colliding.
fn collection_root<K: borsh::BorshSerialize, V: borsh::BorshSerialize>(label: &[u8], map: &BTreeMap<K, V>) -> Hash64 {
    let mut state = keyed(PALW_STATE_V2_DOMAIN_COLLECTION);
    state.update(&(label.len() as u64).to_le_bytes());
    state.update(label);
    state.update(&(map.len() as u64).to_le_bytes());
    for (key, value) in map {
        let key_bytes = borsh::to_vec(key).expect("state keys are borsh-serializable");
        let value_bytes = borsh::to_vec(value).expect("state records are borsh-serializable");
        state.update(&(key_bytes.len() as u64).to_le_bytes());
        state.update(&key_bytes);
        state.update(&(value_bytes.len() as u64).to_le_bytes());
        state.update(&value_bytes);
    }
    finish(state)
}

// ---------------------------------------------------------------------------------------------
// Delta
// ---------------------------------------------------------------------------------------------

/// One entry of a block's state delta: which key changed, from what, to what. `old` is carried so
/// application can verify it is being applied to the state it was computed from, and so a reorg
/// can revert without recomputing the branch.
/// Borsh-serializable because Unit C persists deltas per chain block: the reorg walk reverts
/// them newest-first from disk, so a delta that could not round-trip would be a reorg that
/// silently did nothing.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwDeltaEntryV2 {
    Bond { key: PalwBondKeyV2, old: Option<PalwBondStateV2>, new: Option<PalwBondStateV2> },
    Exposure { key: PalwBondKeyV2, old: Option<u128>, new: Option<u128> },
    Class { key: Hash64, old: Option<PalwClassStateV2>, new: Option<PalwClassStateV2> },
    Target { key: Hash64, old: Option<PalwClassTargetV2>, new: Option<PalwClassTargetV2> },
    Share { key: Hash64, old: Option<u16>, new: Option<u16> },
    EpochBudgets { old: Option<PalwEpochBudgetsV2>, new: Option<PalwEpochBudgetsV2> },
    ReceiptTarget { key: Hash64, old: Option<PalwClassTargetV2>, new: Option<PalwClassTargetV2> },
    Capability { key: Hash64, old: Option<PalwCapabilityStateV2>, new: Option<PalwCapabilityStateV2> },
    Claim { key: Hash64, old: Option<PalwClaimStateV2>, new: Option<PalwClaimStateV2> },
    Payout { key: Hash64, old: Option<PalwPayoutV2>, new: Option<PalwPayoutV2> },
    Panel { key: Hash64, old: Option<PalwPanelStateV2>, new: Option<PalwPanelStateV2> },
    Court { key: Hash64, old: Option<PalwCourtSessionStateV2>, new: Option<PalwCourtSessionStateV2> },
    Epoch { key: Hash64, old: Option<PalwEpochCounterV2>, new: Option<PalwEpochCounterV2> },
    ReceiptEpoch { key: Hash64, old: Option<PalwEpochCounterV2>, new: Option<PalwEpochCounterV2> },
    Weights { old: (u128, u128), new: (u128, u128) },
    Frontier { old: (u64, BlockHash), new: (u64, BlockHash) },
    LastPoint { old: Option<PalwBlockContextV2>, new: Option<PalwBlockContextV2> },
}

/// The full effect one block application had on the state, in application order. Applying it to
/// the same parent reproduces the transition's output exactly ([`apply_delta_v2`]); reverting it
/// from the child reproduces the parent ([`revert_delta_v2`]). Both are tested equal, which is
/// what makes a store layer built on deltas unable to drift from the transition.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwStateDeltaV2 {
    pub point: PalwBlockContextV2,
    pub entries: Vec<PalwDeltaEntryV2>,
}

// ---------------------------------------------------------------------------------------------
// Transition internals: a builder that records every write it makes
// ---------------------------------------------------------------------------------------------

struct TransitionBuilder<'a> {
    params: &'a PalwStateParamsV2,
    state: PalwChainStateV2,
    entries: Vec<PalwDeltaEntryV2>,
}

impl<'a> TransitionBuilder<'a> {
    fn new(parent: &PalwChainStateV2, params: &'a PalwStateParamsV2) -> Self {
        Self { params, state: parent.clone(), entries: Vec::new() }
    }

    // Every write goes through one of these, so the delta cannot miss a change.

    fn write_bond(&mut self, key: PalwBondKeyV2, new: Option<PalwBondStateV2>) {
        let old = match &new {
            Some(record) => self.state.bonds.insert(key, record.clone()),
            None => self.state.bonds.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Bond { key, old, new });
    }

    fn write_exposure(&mut self, key: PalwBondKeyV2, new: Option<u128>) {
        let old = match new {
            Some(value) => self.state.reserved_exposure.insert(key, value),
            None => self.state.reserved_exposure.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Exposure { key, old, new });
    }

    fn write_class(&mut self, key: Hash64, new: Option<PalwClassStateV2>) {
        let old = match &new {
            Some(record) => self.state.classes.insert(key, record.clone()),
            None => self.state.classes.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Class { key, old, new });
    }

    fn write_target(&mut self, key: Hash64, new: Option<PalwClassTargetV2>) {
        let old = match &new {
            Some(record) => self.state.class_targets.insert(key, record.clone()),
            None => self.state.class_targets.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Target { key, old, new });
    }

    fn write_share(&mut self, key: Hash64, new: Option<u16>) {
        let old = match new {
            Some(value) => self.state.class_shares.insert(key, value),
            None => self.state.class_shares.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Share { key, old, new });
    }

    #[allow(dead_code)] // ADR-0045 Decision 2's boundary derivation is the writer; it lands next.
    fn write_epoch_budgets(&mut self, new: Option<PalwEpochBudgetsV2>) {
        let old = std::mem::replace(&mut self.state.epoch_budgets, new.clone());
        self.entries.push(PalwDeltaEntryV2::EpochBudgets { old, new });
    }

    fn write_receipt_target(&mut self, key: Hash64, new: Option<PalwClassTargetV2>) {
        let old = match &new {
            Some(record) => self.state.receipt_targets.insert(key, record.clone()),
            None => self.state.receipt_targets.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::ReceiptTarget { key, old, new });
    }

    fn write_claim(&mut self, key: Hash64, new: Option<PalwClaimStateV2>) {
        let old = match &new {
            Some(record) => self.state.claims.insert(key, record.clone()),
            None => self.state.claims.remove(&key),
        };
        // The unresolved index tracks non-terminal claims; keep it in lockstep with every write.
        if let Some(previous) = &old
            && !previous.phase.is_terminal()
        {
            self.state.unresolved.remove(&(previous.accepted_blue_score, key));
        }
        if let Some(record) = &new
            && !record.phase.is_terminal()
        {
            self.state.unresolved.insert((record.accepted_blue_score, key));
        }
        self.entries.push(PalwDeltaEntryV2::Claim { key, old, new });
    }

    fn write_payout(&mut self, key: Hash64, new: Option<PalwPayoutV2>) {
        let old = match new {
            Some(record) => self.state.pending_payouts.insert(key, record),
            None => self.state.pending_payouts.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Payout { key, old, new });
    }

    fn write_panel(&mut self, key: Hash64, new: Option<PalwPanelStateV2>) {
        let old = match &new {
            Some(record) => self.state.panels.insert(key, record.clone()),
            None => self.state.panels.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Panel { key, old, new });
    }

    fn write_court(&mut self, key: Hash64, new: Option<PalwCourtSessionStateV2>) {
        let old = match &new {
            Some(record) => self.state.court_sessions.insert(key, record.clone()),
            None => self.state.court_sessions.remove(&key),
        };
        if let Some(previous) = &old {
            let count = self.state.open_courts_by_claim.get_mut(&previous.claim).expect("index tracks every session");
            *count -= 1;
            if *count == 0 {
                self.state.open_courts_by_claim.remove(&previous.claim);
            }
            self.state.court_deadlines.remove(&(court_next_deadline_v2(previous), key));
        }
        if let Some(record) = &new {
            *self.state.open_courts_by_claim.entry(record.claim).or_insert(0) += 1;
            self.state.court_deadlines.insert((court_next_deadline_v2(record), key));
        }
        self.entries.push(PalwDeltaEntryV2::Court { key, old, new });
    }

    fn write_epoch(&mut self, key: Hash64, new: Option<PalwEpochCounterV2>) {
        let old = match &new {
            Some(record) => self.state.epoch_counters.insert(key, record.clone()),
            None => self.state.epoch_counters.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Epoch { key, old, new });
    }

    fn write_receipt_epoch(&mut self, key: Hash64, new: Option<PalwEpochCounterV2>) {
        let old = match &new {
            Some(record) => self.state.receipt_epoch_counters.insert(key, record.clone()),
            None => self.state.receipt_epoch_counters.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::ReceiptEpoch { key, old, new });
    }

    // ---- deadline index (never in the delta: rebuilt facts, not primary data) ----

    fn arm_deadline(&mut self, deadline: u64, claim: Hash64) {
        self.state.deadlines.insert((deadline, claim));
    }

    fn disarm_deadline(&mut self, claim: Hash64) {
        // A claim holds at most one deadline; the scan is over a set that shrinks as claims
        // resolve. (An index keyed the other way would dodge the scan but double the index
        // surface; measured need decides later, behavior cannot change.)
        if let Some(entry) = self.state.deadlines.iter().find(|(_, id)| *id == claim).copied() {
            self.state.deadlines.remove(&entry);
        }
    }

    // ---- shared accounting moves ----

    fn reserve_for_claim(&mut self, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        let current = self.state.reserved_exposure.get(&claim.bond).copied().unwrap_or(0);
        let next = current.checked_add(claim.reserved).ok_or(PalwStateV2Error::Overflow("reserved_exposure"))?;
        self.write_exposure(claim.bond, Some(next));
        self.state.bounded_immature = self
            .state
            .bounded_immature
            .checked_add(claim.immature_contribution)
            .ok_or(PalwStateV2Error::Overflow("bounded_immature"))?;
        Ok(())
    }

    /// Release an immature claim's reservations (on `Final` and `Voided` alike — the exposure and
    /// the immature contribution both belong only to non-terminal claims).
    fn release_for_claim(&mut self, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        let current = self.state.reserved_exposure.get(&claim.bond).copied().unwrap_or(0);
        let next = current.checked_sub(claim.reserved).ok_or(PalwStateV2Error::Overflow("reserved_exposure underflow"))?;
        self.write_exposure(claim.bond, if next == 0 { None } else { Some(next) });
        self.state.bounded_immature = self
            .state
            .bounded_immature
            .checked_sub(claim.immature_contribution)
            .ok_or(PalwStateV2Error::Overflow("bounded_immature underflow"))?;
        Ok(())
    }

    /// Debit `amount` from a bond's collateral and record it as slashed. **The only place value
    /// leaves a bond in this ruleset.**
    ///
    /// Audit C5 found no slash primitive anywhere in the tree: `void_claim` released the
    /// reservation and wrote a phase, so every "bond slash" in ADR-0042 — the court's
    /// `ExecutorGuilty`, Decision 7's producer default, a seat that reports falsely — cost
    /// exactly nothing. A ruleset whose every penalty is a state label is a ruleset with no
    /// penalties.
    ///
    /// Saturating on the collateral side rather than erroring: a bond may owe more than it holds
    /// (its exposure ceiling bounds work at risk, not the sum of every way it can be convicted),
    /// and a conviction that cannot be recorded because the debtor is already empty must still
    /// be recorded. What it cannot do is go negative.
    ///
    /// The slashed value is BURNED. Where it might otherwise go — redistribution to the panel,
    /// to the challenger, to the next producer — is a live design question this deliberately does
    /// not answer; burning is the only destination that needs no policy and cannot be gamed by
    /// whoever would have received it.
    fn slash_bond(&mut self, bond: PalwBondKeyV2, amount: u128) -> Result<(), PalwStateV2Error> {
        let record = self.state.bonds.get(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.clone();
        let debit = u64::try_from(amount.min(record.collateral as u128)).expect("clamped to a u64 collateral");
        if debit == 0 {
            return Ok(());
        }
        let mut slashed = record;
        slashed.collateral -= debit;
        slashed.slashed = slashed.slashed.checked_add(debit).ok_or(PalwStateV2Error::Overflow("cumulative slash"))?;
        self.write_bond(bond, Some(slashed));
        Ok(())
    }

    /// Void a claim AND take the collateral it put at risk.
    ///
    /// `claim.reserved` is exactly `pwu × slash_value_per_pwu` — the number the exposure ceiling
    /// exists to bound — so the penalty is the stake the claim itself named. No new parameter,
    /// and the punishment scales with the work claimed rather than with a constant somebody
    /// would have to pick.
    fn void_and_slash(
        &mut self,
        id: Hash64,
        claim: &PalwClaimStateV2,
        voided_daa: u64,
        reason: PalwVoidReasonV2,
    ) -> Result<(), PalwStateV2Error> {
        self.void_claim(id, claim, voided_daa, reason)?;
        self.slash_bond(claim.bond, claim.reserved)
    }

    /// Charge every seat whose verdict the panel's own quorum refuted.
    ///
    /// `served_won` is what the quorum concluded; a seat that reported the other way is the one
    /// contradicted. Iterated in the object's order, which is the order the acceptance layer
    /// validated, so two nodes charge the same seats in the same sequence.
    fn slash_dissenting_seats(
        &mut self,
        claim: &PalwClaimStateV2,
        receipts: &[PalwSeatVerdictV2],
        served_won: bool,
    ) -> Result<(), PalwStateV2Error> {
        for receipt in receipts {
            if receipt.served != served_won {
                self.slash_bond(receipt.seat_bond, claim.reserved)?;
            }
        }
        Ok(())
    }

    /// **P0-7: an assigned seat that answered nothing loses collateral.**
    ///
    /// `slash_dissenting_seats` charges a seat that answered the WRONG way. This charges the one
    /// that did not answer at all, which was free: a bond could take panel seats forever, never
    /// file, and pay nothing — so the exposure a seat is supposed to put behind its verdict was
    /// only ever at risk if it chose to speak.
    ///
    /// `answered` is the seat set the concluding object carried. Everything on the panel and not
    /// in it is a no-show. A seat with something to say is never here: `Valid` and `Unavailable`
    /// are both answers, and reporting withheld data is what the `Unavailable` verdict is for.
    ///
    /// It charges the same `reserved` a dissent costs — an unanswered seat and a refuted one both
    /// failed the same duty, and pricing silence below a lie would make silence the better play.
    fn slash_silent_seats(
        &mut self,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
        answered: &[PalwSeatVerdictV2],
    ) -> Result<(), PalwStateV2Error> {
        let Some(panel) = self.state.panels.get(claim_id).cloned() else {
            // No panel: nothing was assigned, so nobody owed an answer. A claim voided before it
            // ever bound is the `BindTimeout` case, and that is the producer's failure alone.
            return Ok(());
        };
        for seat in &panel.seats {
            if !answered.iter().any(|r| r.seat_bond == seat.bond) {
                self.slash_bond(seat.bond, claim.reserved)?;
            }
        }
        Ok(())
    }

    fn finalize_claim(&mut self, id: Hash64, claim: &PalwClaimStateV2, final_daa: u64) -> Result<(), PalwStateV2Error> {
        self.release_for_claim(claim)?;
        // The weight divergence between the lanes (ADR-0044): an attempt's Final IS its block's
        // certified work; a free-prompt Final only LICENSES — its weight arrives per spent
        // quantum, at the receipt block that spends it.
        if matches!(claim.source, PalwClaimSourceV2::Attempt) {
            self.state.safe_weight =
                self.state.safe_weight.checked_add(claim.pwu as u128).ok_or(PalwStateV2Error::Overflow("safe_weight"))?;
        }
        // ADR-0042 Decision 10: `Final` is where escrow becomes payable, so it is where the
        // release is recorded. Nothing is minted here — this only names an amount and a payee for
        // the next block's coinbase, which is the block that can actually carry an output.
        if claim.escrowed_reward > 0 {
            // The bond must still be there to name a payee. It is, unless the registry dropped a
            // bond that still had live claims — an invariant break, not a payout policy — so this
            // errors rather than skipping: a silently unpaid producer is worse than a refused
            // block, and the refusal names the claim.
            let bond = self.state.bonds.get(&claim.bond).ok_or(PalwStateV2Error::MissingBond(claim.bond))?;
            self.write_payout(id, Some(PalwPayoutV2 { payload: bond.payout_payload, amount: claim.escrowed_reward }));
        }
        let mut finalized = claim.clone();
        finalized.phase = PalwClaimPhaseV2::Final { final_daa };
        self.write_claim(id, Some(finalized));
        self.disarm_deadline(id);
        Ok(())
    }

    fn void_claim(
        &mut self,
        id: Hash64,
        claim: &PalwClaimStateV2,
        voided_daa: u64,
        reason: PalwVoidReasonV2,
    ) -> Result<(), PalwStateV2Error> {
        let mut voided = claim.clone();
        voided.phase = PalwClaimPhaseV2::Voided { voided_daa, reason };
        // Audit C5, free-prompt half: an abandoned commitment holds its reservation for the
        // configured span instead of releasing it here, so a redraw costs collateral rather than
        // a transaction fee. The hold is a DELAY, never a confiscation — `release_abandon_hold`
        // gives every sompi back when the span elapses, and the claim is terminal throughout, so
        // it weighs nothing and can never resume.
        //
        // Immature weight is released either way and immediately: a voided claim contributes no
        // live weight the instant it is voided, which is W5, and the hold is about admission
        // headroom alone.
        if palw_claim_is_on_abandon_hold_v2(&voided, self.params, voided_daa) {
            self.state.bounded_immature = self
                .state
                .bounded_immature
                .checked_sub(claim.immature_contribution)
                .ok_or(PalwStateV2Error::Overflow("bounded_immature underflow"))?;
            self.write_claim(id, Some(voided));
            // The deadline is re-armed rather than disarmed: the sweep's terminal arm releases
            // the hold when it fires. Deriving it from the record (`voided_daa + hold`) is what
            // keeps the index rebuildable, which `assert_deadline_consistency` checks.
            self.disarm_deadline(id);
            let release_at =
                voided_daa.checked_add(self.params.fp_abandon_hold_daa).ok_or(PalwStateV2Error::Overflow("abandon hold"))?;
            self.arm_deadline(release_at, id);
            return Ok(());
        }
        self.release_for_claim(claim)?;
        self.write_claim(id, Some(voided));
        self.disarm_deadline(id);
        Ok(())
    }

    /// Give back what [`Self::void_claim`] held. Exposure only — the immature contribution was
    /// released at the void.
    fn release_abandon_hold(&mut self, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        let current = self.state.reserved_exposure.get(&claim.bond).copied().unwrap_or(0);
        let next = current.checked_sub(claim.reserved).ok_or(PalwStateV2Error::Overflow("reserved_exposure underflow"))?;
        self.write_exposure(claim.bond, if next == 0 { None } else { Some(next) });
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The transition
// ---------------------------------------------------------------------------------------------

/// The attempt-only face of [`apply_palw_transition_v3`], kept so every pre-FP caller and test
/// reads exactly as before: an `Option<attempt>` is the two work shapes a V2-only network has.
pub fn apply_palw_transition_v2(
    parent: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
    current_attempt: Option<&PalwAttemptEnvelopeV2>,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    let work = match current_attempt {
        Some(envelope) => PalwBlockWorkV3::Attempt(envelope),
        None => PalwBlockWorkV3::None,
    };
    apply_palw_transition_v3(parent, params, ctx, accepted_objects, work)
}

/// Apply one chain block's PALW content to its parent state. Pure: `(parent, params, ctx,
/// objects, work) → (child, delta)`, or an error that rejects the block's PALW content
/// wholesale — a partial application is a state nobody else can recompute.
pub fn apply_palw_transition_v3(
    parent: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
    block_work: PalwBlockWorkV3<'_>,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    // 1. Context monotonicity: blue score strictly increases along a chain, DAA never decreases.
    if let Some(last) = &parent.last_point {
        if ctx.blue_score <= last.blue_score {
            return Err(PalwStateV2Error::NonMonotonicContext("blue_score must strictly increase along a chain"));
        }
        if ctx.daa_score < last.daa_score {
            return Err(PalwStateV2Error::NonMonotonicContext("daa_score must not decrease along a chain"));
        }
    }

    let mut builder = TransitionBuilder::new(parent, params);

    // 1b. Drain the payout queue THIS block's coinbase paid. It must happen before the sweeps,
    //     because the sweeps are what refill it: a claim finalized by this block is paid by the
    //     NEXT one, and clearing after would erase it. Every node applying this block clears the
    //     same set, because the set is the parent state's — already committed, already hashed
    //     into the root this block's header names.
    for claim_id in builder.state.pending_payouts.keys().copied().collect::<Vec<_>>() {
        builder.write_payout(claim_id, None);
    }

    // 2. Deadline sweeps — everything strictly past is resolved before this block says anything.
    //    (A deadline equal to ctx.daa_score is still actionable by this block's objects.) Claims
    //    first, then the court backstop: the order is fixed, and fixed IS the requirement.
    sweep_deadlines(&mut builder, ctx)?;
    sweep_court_deadlines(&mut builder, ctx)?;

    // 2b. Per-class retarget (PR-09): crossing a global epoch boundary closes the previous
    //     epoch as one span and retargets every share-bearing, unfrozen class against it. Runs
    //     between the sweeps and the objects — a fixed slot, because fixed IS the requirement.
    apply_class_retargets(&mut builder, parent, ctx)?;

    // 3. The block's accepted objects, in consensus acceptance order.
    for object in accepted_objects {
        apply_object(&mut builder, ctx, object)?;
    }

    // 3a. Condition 12/13: any class whose activation score this block reaches becomes `Active`
    //     and takes its share. A CLOCK, not an object — nobody submits it, so there is nothing to
    //     forge and no authority question. Runs before the budgets, because a class that becomes
    //     active in this block must be in the share table the budgets derive from.
    activate_due_classes(&mut builder, ctx)?;

    // 3b. ADR-0045 Decision 2's epoch budgets, in blocks. AFTER the objects, because the genesis
    //     block's own registrations are what create the share table this derives from — deriving
    //     before them would give the first epoch an empty budget map, which refuses every attempt
    //     on the network's first epoch.
    ensure_epoch_budgets(&mut builder, ctx);

    // 4. The block's own work — an attempt, a certified-quantum spend, or none.
    match block_work {
        PalwBlockWorkV3::None => {}
        PalwBlockWorkV3::Attempt(envelope) => apply_attempt(&mut builder, ctx, envelope)?,
        PalwBlockWorkV3::ReceiptSpend(spend) => apply_receipt_spend(&mut builder, ctx, spend)?,
    }

    // 5. Frontier observation — the definition `palw_fork_choice` states and this used to miss:
    //    **the deepest block on this chain whose PALW work is `Final`, with nothing unresolved
    //    below it.**
    //
    // What stood here asked `unresolved.is_empty()` — whether the chain, tip included, held no
    // open claim — and named THIS block. Two failures fell out of it (audit C2), and they pull in
    // opposite directions, which is why one rule has to answer both:
    //
    // * On a chain that produces work the condition is never true. Step 4 above just inserted
    //   this block's own `Provisional` claim, so at every block forever the frontier stayed
    //   wherever the chain last went idle. `pruning_ceiling_v2` froze with it (the node never
    //   prunes again, and the carriage grows one claim per block without bound), and the
    //   comparator's first key stopped discriminating between honest chains.
    // * On a chain that produces NOTHING the condition is trivially true, so a fork carrying no
    //   attempts at all advanced its frontier once per block and OUTRANKED a chain that had
    //   matured real work. Reproduced: 60 attempt-less blocks beat a 60-block honest chain on
    //   key 1, and the deep-reorg gate allowed it. That is precisely the fabrication that
    //   ordering by frontier before weight exists to refuse.
    //
    // Both close by measuring MATURED WORK rather than the absence of open claims. `unresolved`
    // is ordered by `accepted_blue_score`, so the resolved prefix ends immediately below its
    // first entry; inside that prefix the frontier is the deepest `Final` claim — a claim, so it
    // names its own accepting block, and a chain with no matured work has no frontier to stand
    // on however long it grows. `Voided` claims are resolved (they do not hold the prefix back)
    // but confer no frontier: a block whose work was thrown out matured nothing.
    let old_frontier = (builder.state.safe_frontier_blue_score, builder.state.safe_frontier);
    let resolved_through = match builder.state.unresolved.iter().next() {
        // Nothing open anywhere: the whole chain including this block is a resolved prefix.
        None => u64::MAX,
        // Everything strictly below the oldest open claim is resolved. Its own point is not —
        // pruning there would delete history a live claim still needs.
        Some((oldest_open, _)) => oldest_open.saturating_sub(1),
    };
    if let Some(claim) = builder
        .state
        .claims
        .values()
        .filter(|c| matches!(c.phase, PalwClaimPhaseV2::Final { .. }) && c.accepted_blue_score <= resolved_through)
        .max_by_key(|c| c.accepted_blue_score)
        && claim.accepted_blue_score > old_frontier.0
    {
        builder.state.safe_frontier_blue_score = claim.accepted_blue_score;
        builder.state.safe_frontier = claim.accepted_block;
    }
    let new_frontier = (builder.state.safe_frontier_blue_score, builder.state.safe_frontier);
    debug_assert!(new_frontier.0 >= old_frontier.0, "the frontier never retreats");

    // Weight / frontier / position entries, exactly once, at the end.
    if (parent.safe_weight, parent.bounded_immature) != (builder.state.safe_weight, builder.state.bounded_immature) {
        builder.entries.push(PalwDeltaEntryV2::Weights {
            old: (parent.safe_weight, parent.bounded_immature),
            new: (builder.state.safe_weight, builder.state.bounded_immature),
        });
    }
    if old_frontier != new_frontier {
        builder.entries.push(PalwDeltaEntryV2::Frontier { old: old_frontier, new: new_frontier });
    }
    builder.entries.push(PalwDeltaEntryV2::LastPoint { old: parent.last_point, new: Some(*ctx) });
    builder.state.last_point = Some(*ctx);

    let delta = PalwStateDeltaV2 { point: *ctx, entries: builder.entries };
    Ok((builder.state, delta))
}

/// After the LAST open session on a licensed claim ends challenger-side (an explicit
/// `ChallengerDefeated` verdict, or the backstop sweep), re-arm the claim's path to `Final`:
/// never earlier than the licensed floor, never in this block's past.
fn rearm_after_challenger_side_close(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    claim: &PalwClaimStateV2,
) -> Result<(), PalwStateV2Error> {
    if !builder.state.open_courts_by_claim.contains_key(&claim_id)
        && let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = claim.phase
    {
        let floor =
            licensed_daa.checked_add(builder.params.window_challenge).ok_or(PalwStateV2Error::Overflow("challenge deadline"))?;
        builder.arm_deadline(floor.max(ctx.daa_score), claim_id);
    }
    Ok(())
}

/// The court backstop (runs AFTER the claim sweep, in `(deadline, session)` order — the fixed
/// ordering is consensus): a session still open past `deadline_daa` closes CHALLENGER-side.
/// Prosecution is the challenger's burden — a ladder default against the executor is provable
/// and closable by ANYONE as `CourtClosed(ExecutorGuilty)` long before this fires, so the only
/// sessions that reach the backstop are ones nobody could or would finish, and an honest claim
/// must not stay frozen for them.
/// **The DAA score at which a session next needs attention — whichever clock runs out first.**
///
/// A session has two: the rung window (the party whose turn it is must move) and the
/// whole-session backstop. Indexing on the minimum keeps ONE sweep queue, and keeps the rebuilt
/// index exactly derivable from the session records — which is what lets the index be dropped
/// from the state root and rebuilt on load. A ladder that has been abandoned or has reached
/// `Terminal` has no rung clock left to run, so only the backstop applies.
pub(crate) fn court_next_deadline_v2(session: &PalwCourtSessionStateV2) -> u64 {
    use crate::palw_bisect::PalwBisectTurnV1;
    match session.ladder.turn() {
        PalwBisectTurnV1::AwaitDisclosure | PalwBisectTurnV1::AwaitVerdict | PalwBisectTurnV1::Terminal => {
            session.deadline_daa.min(session.ladder.last_deadline_daa())
        }
        PalwBisectTurnV1::Abandoned => session.deadline_daa,
    }
}

fn sweep_court_deadlines(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    while let Some(&(deadline, session_id)) = builder.state.court_deadlines.iter().next() {
        if deadline >= ctx.daa_score {
            break;
        }
        let mut session =
            builder.state.court_sessions.get(&session_id).ok_or(PalwStateV2Error::MissingSession(session_id))?.clone();
        let claim = builder.state.claims.get(&session.claim).ok_or(PalwStateV2Error::MissingClaim(session.claim))?.clone();

        // **P0-9: silence at a rung decides the dispute, and no object says so.**
        //
        // `declare_no_show` reads whose turn it was from the machine's own state, which is this
        // chain's state — so every node reaches the same verdict from the same absence, and there
        // is nothing for an attacker to forge. That is the whole reason the ladder had to be
        // carried before defaults could be honoured.
        //
        // A silent RESPONDER loses: it announced a root and would not stand behind it at the
        // index it was asked about. A silent CHALLENGER loses: prosecution is its burden, which
        // is the same rule the backstop below applies. Either way the session ends here.
        // A rung clock counts only if it runs out STRICTLY inside the session budget. When the
        // two coincide — which is what `turn_deadline_daa`'s default makes them — the rung can
        // decide nothing the backstop would not already have decided, and letting it try would
        // silently invert the outcome: the backstop closes challenger-side, a first-rung silence
        // closes against the responder. So a network without a tighter rung window keeps exactly
        // the pre-ladder behavior, and one with a real ladder gets the rung verdict it configured.
        let rung_fired = session.ladder.last_deadline_daa() < ctx.daa_score
            && session.ladder.last_deadline_daa() < session.deadline_daa;
        if rung_fired {
            if let Ok(no_show) = session.ladder.declare_no_show(ctx.daa_score) {
                builder.write_court(session_id, None);
                match no_show.silent_party {
                    crate::palw_bisect::PalwBisectPartyV1::Responder => {
                        // Same treatment a proven fault gets, and for the same reason: an
                        // executor that will not stand behind its own announced root at the
                        // index it was asked about has not been convicted of arithmetic, but it
                        // HAS defaulted on the only defence available to it. A late default
                        // against an already-terminal claim closes the session and changes
                        // nothing else, exactly as a late verdict does.
                        if !claim.phase.is_terminal() {
                            builder.void_and_slash(session.claim, &claim, ctx.daa_score, PalwVoidReasonV2::CourtFraud)?;
                        }
                    }
                    crate::palw_bisect::PalwBisectPartyV1::Challenger => {
                        rearm_after_challenger_side_close(builder, ctx, session.claim, &claim)?;
                    }
                }
                continue;
            }
            // The ladder is already `Abandoned` — its rung clock is spent and only the backstop
            // can still act. Rewriting the record re-indexes it under the backstop, so the queue
            // moves forward instead of spinning on a deadline nothing consumes.
            if session.deadline_daa >= ctx.daa_score {
                let refreshed = session.clone();
                builder.write_court(session_id, Some(refreshed));
                continue;
            }
        }
        // The backstop (or an abandoned ladder whose session budget is also spent): closed
        // challenger-side, because an unfinished challenge must not freeze an honest claim.
        builder.write_court(session_id, None);
        rearm_after_challenger_side_close(builder, ctx, session.claim, &claim)?;
    }
    Ok(())
}

/// ADR-0045 Decision 3: the share table after granting `entrant` its `share` — the ONLY
/// arithmetic that ever moves a permille.
///
/// The first grant must be the base class at the whole 1000‰ (the liveness floor exists before
/// anything else does). Every later grant is funded by every incumbent proportionally:
/// incumbents are scaled to `1000 − share` and the truncation residue is returned by largest
/// remainder — remainder descending, class id ascending on ties — so the table sums to exactly
/// 1000‰ by construction, deterministically, on every node. Refused: a share outside the
/// denominator, a share below the grant floor (whose worst-case epoch budget would be zero
/// blocks — the V1 derivation's `ZeroBudget`, moved to where the share is chosen), and a grant
/// that would push any donor below that same floor — a registration may not starve an incumbent
/// (the base class included) to fund itself.
fn granted_share_table_v2(
    params: &PalwStateParamsV2,
    current: &BTreeMap<Hash64, u16>,
    entrant: Hash64,
    share: u16,
) -> Result<BTreeMap<Hash64, u16>, PalwStateV2Error> {
    let floor = params.min_grantable_share_permille();
    if current.is_empty() {
        if entrant != params.base_class_id {
            return Err(PalwStateV2Error::FirstClassMustBeTheBase { class_id: entrant, base: params.base_class_id });
        }
        if share != 1000 {
            return Err(PalwStateV2Error::FirstShareMustBeWhole { got: share });
        }
        return Ok([(entrant, 1000u16)].into_iter().collect());
    }
    if share > 1000 {
        return Err(PalwStateV2Error::ShareOutOfRange { got: share });
    }
    if share < floor {
        return Err(PalwStateV2Error::ShareBelowGrantFloor { class_id: entrant, share, floor });
    }
    let keep = 1000u32 - share as u32;
    // Scale every incumbent to the kept permille, undivided residues first: `s_k · keep` is at
    // most 10⁶, so the arithmetic is exact in u32 and the residue distribution is what makes
    // Σ = 1000 a construction rather than an assertion.
    let mut scaled: Vec<(Hash64, u32, u32)> =
        current.iter().map(|(id, s)| (*id, (*s as u32) * keep / 1000, (*s as u32) * keep % 1000)).collect();
    let distributed: u32 = scaled.iter().map(|(_, base, _)| *base).sum();
    let deficit = keep - distributed;
    // Largest remainder, class-id order on ties. The sort is total (remainder, then id), so two
    // nodes cannot hand the same residue permille to different classes.
    scaled.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let mut table: BTreeMap<Hash64, u16> = BTreeMap::new();
    for (index, (id, base, _)) in scaled.iter().enumerate() {
        let granted = base + u32::from((index as u32) < deficit);
        let granted = u16::try_from(granted).expect("a scaled share is at most 1000");
        if granted < floor {
            return Err(PalwStateV2Error::DonationBreaksGrantFloor { donor: *id, would_hold: granted, floor });
        }
        table.insert(*id, granted);
    }
    table.insert(entrant, share);
    debug_assert_eq!(table.values().map(|s| *s as u32).sum::<u32>(), 1000, "the donation arithmetic conserves the denominator");
    Ok(table)
}

/// The V2 face of ADR-0039 D5's per-class retarget, driven by the state's own counters and the
/// chain's own epochs — no clock, no store, no second timeline.
///
/// When this block's epoch exceeds the parent chain point's, the PARENT's epoch has closed:
/// its realized total is Σ produced_blocks over counters still standing at that epoch (a
/// counter from an older epoch means the class produced nothing in the closed one — observed
/// zero). With zero realized total there is nothing to measure and no target moves (the empty
/// epochs of a gap measure nothing rather than easing everyone). Otherwise every registered
/// class retargets through `retarget_over_span_v1` — the V1 rule, reused whole: expectation is
/// the class's SHARE of realized production, so the loop only redistributes between classes and
/// a one-class network at 1000‰ is a deliberate no-op. Classes skip the span if they are frozen
/// (the target freezes with the class — easing while frozen would hand the unfreeze a burst) or
/// carry no share in the table (a share-less class cannot be admitted anyway).
/// **ADR-0045 Decision 2: one epoch's per-class budgets, in blocks, derived from the share table.**
///
/// This is the sizing basis the static pwu budget never had. A class's share is a permille of
/// CADENCE, so its budget is a count of BLOCKS: the epoch's expected block count (its DAA span —
/// one block per DAA unit is the cadence the retarget itself targets) times the class's share,
/// times the tolerance that says how far above its share a class may run before the cap bites.
///
/// The tolerance is what keeps this from being the "permanent hard stop" the audit named. Fenced
/// at or above unity, a class holding the whole table gets a budget at least as large as the
/// epoch's expected production, so the cap cannot bind on an honest chain running at cadence —
/// it binds on a class producing well beyond its share, which is the only thing it is for.
///
/// Blocks, never pwu: under `DerivedV1` an attempt's pwu is a function of the class TARGET, so a
/// pwu budget shrinks in block terms exactly as a class gets harder — a popular class would stall
/// its own chain for getting popular. The share also cancels out of a pwu inequality entirely
/// (ADR-0045's amendment defect (e)).
pub fn derive_epoch_budgets_v2(
    shares: &BTreeMap<Hash64, u16>,
    frozen: &BTreeSet<Hash64>,
    competing: &BTreeSet<Hash64>,
    epoch_length: u64,
    tolerance_permille: u32,
    epoch_index: u64,
) -> PalwEpochBudgetsV2 {
    // ADR-0045 Decision 2's fallback denominator: "an empty census (fresh chain, gap epochs)
    // competes everyone unfrozen". A frozen class keeps its permille in the table ("freeze and
    // unfreeze move no share"), so the fold is over the table MINUS the frozen set, not over the
    // table.
    let all_unfrozen: u64 = shares.iter().filter(|(id, _)| !frozen.contains(*id)).map(|(_, s)| u64::from(*s)).sum();
    let census: u64 = competing.iter().filter_map(|id| shares.get(id)).map(|s| u64::from(*s)).sum();
    let budget_blocks = shares
        .iter()
        .map(|(class_id, share)| {
            // **ADR-0045 Decision 2, with the denominator the Decision specifies.**
            //
            //     budget_c(e) = ⌊ tol‰ · E · s_c / (1000 · denom_c) ⌋
            //
            // `denom_c = Σ shares over the competing set`: the closed epoch's producers among
            // unfrozen share-bearing classes; a non-producer that re-enters is measured against
            // the set plus itself; an empty census competes everyone unfrozen.
            //
            // This code read `denom_c = 1000‰` — the whole table, always — which is the H1 defect
            // the Decision explicitly imports the census to close: "a class whose permille sits
            // idle must not strangle the classes that are actually producing, and a cap that
            // ignored absence would reintroduce H1's unbounded walk as a hard refusal instead of a
            // slow one." A hard refusal is worse than the slow one, because on a `ConsensusV2`
            // network the attempt lane is the ONLY block type
            // (`required_algo_id_for_mode` demands `algorithm_id` of every header), so refusing
            // every attempt stops the chain — and DAA advances only with blocks, so the epoch that
            // would refill the budget never ends. The retarget got this fix; its second consumer
            // did not.
            let denom = if competing.contains(class_id) { census } else { census.saturating_add(u64::from(*share)) };
            let denom = if census == 0 { all_unfrozen } else { denom };
            // A denominator of zero is not a division, it is "nothing bears share on this chain" —
            // which the grant floor makes unreachable, and which the whole-table reading covers.
            let denom = if denom == 0 { 1000 } else { denom };
            let scaled = (epoch_length as u128) * (*share as u128) * (tolerance_permille as u128) / (1000u128 * denom as u128);
            // Never zero: a class with a share is a class that may produce, and a zero budget
            // would freeze it under the name of a cap. One block is the floor.
            (*class_id, scaled.max(1).min(u64::MAX as u128) as u64)
        })
        .collect();
    PalwEpochBudgetsV2 { epoch_index, budget_blocks }
}

/// Install the budgets for `ctx`'s epoch when the state carries none, or carries another epoch's.
///
/// Runs on every transition rather than only at boundaries, and the two are the same answer: the
/// share table is genesis-fixed (a class cannot enter through a transaction —
/// `palw_lifecycle_objects_v2`), so a budget derived mid-epoch equals the one the boundary would
/// have derived. Running it everywhere is what gives epoch 0 a budget, which a
/// boundary-only rule cannot: no boundary has been crossed when the first block lands, and a
/// missing budget refuses every attempt.
/// Flip every `Registered` class whose activation score this block has reached (condition 13).
///
/// The share is granted at the edge, by the same `granted_share_table_v2` a registration would
/// have used — so a class that soaked weightless enters the table exactly as it would have on day
/// one, funded by donation from every incumbent.
///
/// Ordered by class id, because two classes activating in one block must be granted in an order
/// every node reproduces: the second grant is computed against the table the first left behind.
fn activate_due_classes(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let due: Vec<(Hash64, u16)> = builder
        .state
        .classes
        .iter()
        .filter_map(|(id, record)| match record.status {
            PalwClassStatusV2::Registered { activation_daa, pending_share_permille } if activation_daa <= ctx.daa_score => {
                Some((*id, pending_share_permille))
            }
            _ => None,
        })
        .collect();
    for (class_id, share) in due {
        let table = granted_share_table_v2(builder.params, &builder.state.class_shares, class_id, share)?;
        for (id, granted) in table {
            if builder.state.class_shares.get(&id).copied() != Some(granted) {
                builder.write_share(id, Some(granted));
            }
        }
        let mut record = builder.state.classes.get(&class_id).expect("just listed").clone();
        record.status = PalwClassStatusV2::Active;
        builder.write_class(class_id, Some(record));
    }
    Ok(())
}

fn ensure_epoch_budgets(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    let epoch_index = ctx.daa_score / builder.params.epoch_length;
    if builder.state.epoch_budgets.as_ref().is_some_and(|b| b.epoch_index == epoch_index) {
        return;
    }
    if builder.state.class_shares.is_empty() {
        // Nothing is registered yet — the genesis block before its own object list applies. A
        // budget over an empty table is not "no budget", it is a fact that does not exist yet.
        return;
    }
    // The H1 census, for the budget's denominator (ADR-0045 Decision 2). Same rule the retarget
    // applies one step earlier in this transition, on the same counters: the classes that produced
    // in the CLOSED epoch — the parent's — among the unfrozen ones. The attempt lane specifically,
    // because the budget caps attempt blocks and reads the attempt counter; a class that produced
    // only receipts is not in this lane's span and is measured against the set plus itself.
    let closed_epoch = builder.state.last_point.map(|point| point.daa_score / builder.params.epoch_length);
    let frozen: BTreeSet<Hash64> = builder
        .state
        .classes
        .iter()
        .filter(|(_, record)| matches!(record.status, PalwClassStatusV2::Frozen { .. }))
        .map(|(id, _)| *id)
        .collect();
    let competing: BTreeSet<Hash64> = match closed_epoch {
        None => BTreeSet::new(),
        Some(closed) => builder
            .state
            .epoch_counters
            .iter()
            .filter(|(id, counter)| counter.epoch_index == closed && counter.produced_blocks > 0 && !frozen.contains(*id))
            .map(|(id, _)| *id)
            .collect(),
    };
    let budgets = derive_epoch_budgets_v2(
        &builder.state.class_shares,
        &frozen,
        &competing,
        builder.params.epoch_length,
        builder.params.budget_tolerance_permille,
        epoch_index,
    );
    builder.write_epoch_budgets(Some(budgets));
}

fn apply_class_retargets(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let Some(last) = &parent.last_point else { return Ok(()) };
    let epoch_length = builder.params.epoch_length;
    let closed_epoch = last.daa_score / epoch_length;
    if ctx.daa_score / epoch_length <= closed_epoch {
        return Ok(());
    }
    let lane_total = |counters: &BTreeMap<Hash64, PalwEpochCounterV2>| -> u64 {
        counters.values().filter(|counter| counter.epoch_index == closed_epoch).map(|counter| counter.produced_blocks).sum()
    };
    let attempt_total = lane_total(&builder.state.epoch_counters);
    let receipt_total = lane_total(&builder.state.receipt_epoch_counters);
    let combined = attempt_total.checked_add(receipt_total).ok_or(PalwStateV2Error::Overflow("combined census"))?;
    if combined == 0 {
        return Ok(());
    }
    // ADR-0044 Decision 5/9: ONE combined census, and per-lane expectations formed by COMPOSING
    // the class share with the source split — `share × split / 1000` for the attempt lane,
    // `share × (1000 − split) / 1000` for the receipt lane, half-up. The census total stays the
    // REAL combined count for both lanes, so the rule's own invariant (a class cannot exceed its
    // span) holds even when one lane over-produces its split — an over-producing lane simply
    // measures above its composed expectation and tightens. (Scaling the TOTAL instead was the
    // first draft, and it broke exactly there: a lane's observed blocks exceeded its synthetic
    // span.) At split = 1000 the attempt lane's composed share is the class share and the receipt
    // lane's is zero — the pure-attempt configuration is the old single-lane rule, byte for byte.
    let split = builder.params.fp_attempt_share_permille as u32;
    let compose = |share: u16, lane_permille: u32| -> u16 { ((share as u32 * lane_permille + 500) / 1000) as u16 };

    // Snapshot the iteration set: the writes below mutate the target maps, never `classes`, but
    // borrowing rules want the plan separated from the writes anyway.
    let class_ids: Vec<Hash64> = builder.state.classes.keys().copied().collect();

    // **Audit H1 — normalize the expectation over the classes that actually competed.**
    //
    // `retarget_over_span_v1` expects `share · total / 1000` blocks of a class, where `total` is
    // what the span REALLY produced. The two disagree whenever some permille belongs to a class
    // that produced nothing — frozen, unstaffed, or simply idle. Its permille stays in the
    // denominator while its blocks are missing from `total`, so every class that DID produce looks
    // like an over-producer and has its target divided. That verdict repeats every boundary, in
    // the same direction, with `max_factor` bounding each step and nothing bounding the walk:
    // measured at 4^12 over twelve boundaries, ending at zero, where `ZeroPreviousTarget` rejects
    // every subsequent block deterministically and no node can rejoin.
    //
    // The rule below measures a span against the classes that were IN it. Expectations sum back to
    // the realized total, so holding the whole of what happened is not over-producing; and a class
    // that produced nothing is skipped rather than measured as zero, because a span it sat out
    // says nothing about its difficulty in either direction. Genuine competition is untouched:
    // when two classes both produce, both keep their table shares and the feedback works as
    // designed — the survivor's windfall only exists when there is no competitor to give it to.
    // The two corrections compose per lane. For each lane: snapshot that lane's census, fold the
    // competing permille over the classes that produced IN THAT LANE (H1 — chain-state shares,
    // ADR-0045 Decision 3), renormalize each competitor's share over that fold, and only then
    // compose with the lane's split permille. At split = 1000 the receipt lane's permille is 0
    // (skipped whole), one class renormalizes to 1000‰, and the attempt arm is the single-lane
    // rule byte for byte — the compatibility claim both parents made, preserved through both fixes.
    for (lane_permille, counters_are_receipts) in [(split, false), (1000 - split, true)] {
        if lane_permille == 0 {
            // A lane the split allots nothing measures nothing (the pure-attempt configuration's
            // receipt arm, and only that on a live network — FP-05 refuses interior zeros).
            continue;
        }
        // Snapshot the lane census before any write, so the plan is a pure function of the
        // parent state.
        let produced_in_lane: BTreeMap<Hash64, u64> = {
            let counters =
                if counters_are_receipts { &builder.state.receipt_epoch_counters } else { &builder.state.epoch_counters };
            class_ids
                .iter()
                .map(|id| {
                    let blocks = counters
                        .get(id)
                        .filter(|counter| counter.epoch_index == closed_epoch)
                        .map(|counter| counter.produced_blocks)
                        .unwrap_or(0);
                    (*id, blocks)
                })
                .collect()
        };
        let competing_permille: u64 = class_ids
            .iter()
            .filter(|id| !matches!(builder.state.classes.get(id).map(|c| &c.status), Some(PalwClassStatusV2::Frozen { .. })))
            .filter(|id| produced_in_lane.get(id).copied().unwrap_or(0) > 0)
            // ADR-0045 Decision 3: shares are chain state now — the same fold, one lookup key over.
            .filter_map(|id| builder.state.class_shares.get(id).copied())
            .map(u64::from)
            .sum();
        if competing_permille == 0 {
            // Blocks may exist in the lane, but from no share-bearing unfrozen class. Nothing
            // here is a statement about any class's difficulty.
            continue;
        }
        for class_id in &class_ids {
            let class_id = *class_id;
            let class = builder.state.classes.get(&class_id).expect("iterating the map's own keys");
            if matches!(class.status, PalwClassStatusV2::Frozen { .. }) {
                continue;
            }
            let Some(share) = builder.state.class_shares.get(&class_id).copied() else { continue };
            let observed = produced_in_lane.get(&class_id).copied().unwrap_or(0);
            if observed == 0 {
                // It was not in this lane's span. Measuring it as an under-producer would ease
                // its target on every span it sits out, which is the same unbounded walk in the
                // other direction.
                continue;
            }
            // Renormalized share, saturating at the whole denominator: a sole competing class is
            // expected to produce the whole of its lane, which is what "its share of what
            // happened" means.
            let share = u16::try_from((u64::from(share) * 1000 / competing_permille).min(1000))
                .expect("the value is clamped to 1000, which fits u16");
            let composed_share = compose(share, lane_permille);
            if composed_share == 0 {
                // A share the composition rounds to nothing measures nothing — the split itself
                // said so.
                continue;
            }
            let targets = if counters_are_receipts { &builder.state.receipt_targets } else { &builder.state.class_targets };
            let lane = if counters_are_receipts { "receipt" } else { "attempt" };
            let current = targets
                .get(&class_id)
                .ok_or_else(|| PalwStateV2Error::Retarget(format!("class {class_id} has no {lane} target slot")))?
                .target;
            let census = crate::palw_class_daa::PalwClassSpanCensusV1 { class_daa_blocks: observed, total_daa_blocks: combined };
            let next = crate::palw_class_daa::retarget_over_span_v1(current, &census, composed_share, builder.params.class_daa_max_factor())
                .map_err(|e| PalwStateV2Error::Retarget(e.to_string()))?
                // A target of zero is not "impossibly hard", it is unrecoverable: the next
                // retarget returns `ZeroPreviousTarget` and every block after it is rejected,
                // deterministically, forever. One is the floor. With the normalization above
                // nothing should walk here, and this exists so that "should" is not what stands
                // between the chain and a hard stop.
                .max(1);
            if next != current {
                if counters_are_receipts {
                    builder.write_receipt_target(class_id, Some(PalwClassTargetV2 { target: next }));
                } else {
                    builder.write_target(class_id, Some(PalwClassTargetV2 { target: next }));
                }
            }
        }
    }
    Ok(())
}

fn sweep_deadlines(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    // One at a time, smallest (deadline, claim) first: resolving a claim mutates the set, and
    // the (deadline, claim) order is what makes the sweep identical on every node.
    while let Some(&(deadline, claim_id)) = builder.state.deadlines.iter().next() {
        if deadline >= ctx.daa_score {
            break;
        }
        builder.state.deadlines.remove(&(deadline, claim_id));
        let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
        match claim.phase {
            PalwClaimPhaseV2::Provisional => {
                builder.void_claim(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::BindTimeout)?;
            }
            PalwClaimPhaseV2::PanelBound { .. } => {
                // P0-7's sharpest case: the window closed with NO concluding object at all, so
                // every seat on the panel is a no-show. The producer is not charged here — the
                // two timeouts void a claim nobody was in a position to blame the producer for
                // (the `ProducerDefaulted` arm is where that blame is proven) — but the panel is,
                // because producing an answer within the window is the whole duty a seat holds.
                //
                // A single honest seat cannot license alone, which is why the charge is on
                // silence rather than on failing to reach quorum: filing is always available to
                // it, and a filed answer is never a no-show.
                builder.slash_silent_seats(&claim_id, &claim, &[])?;
                builder.void_claim(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ReceiptTimeout)?;
            }
            PalwClaimPhaseV2::ReceiptLicensed { .. } => {
                debug_assert!(
                    !builder.state.open_courts_by_claim.contains_key(&claim_id),
                    "a claim under court holds no final deadline"
                );
                builder.finalize_claim(claim_id, &claim, ctx.daa_score)?;
            }
            // The ONE terminal claim that legitimately owns a deadline: a free-prompt commitment
            // abandoned at `BindTimeout`, whose collateral hold expires here (audit C5). The
            // deadline fired, so the hold is over by construction — the predicate is asserted
            // rather than re-tested, because a deadline armed by `void_claim` and a hold computed
            // from the record are the same arithmetic.
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }
                if matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }) && builder.params.fp_abandon_hold_daa > 0 =>
            {
                debug_assert!(
                    !palw_claim_is_on_abandon_hold_v2(&claim, builder.params, ctx.daa_score),
                    "the hold's own deadline fired while the record still says it is held"
                );
                builder.release_abandon_hold(&claim)?;
            }
            PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => {
                // Any other terminal claim owns no deadline; finding one is index corruption.
                return Err(PalwStateV2Error::CarriageInconsistent(format!("terminal claim {claim_id} held a deadline")));
            }
        }
    }
    Ok(())
}

fn apply_object(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &PalwConsensusObjectV2,
) -> Result<(), PalwStateV2Error> {
    match object {
        PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, collateral, payout_payload } => {
            if builder.state.bonds.contains_key(bond) {
                return Err(PalwStateV2Error::DuplicateBond(*bond));
            }
            // Audit C5: a bond identity has to cost something, or panel dedup is a formality.
            // `min_collateral_sompi` existed in the atomic bundle and was read by nobody.
            if *collateral < builder.params.min_collateral_sompi {
                return Err(PalwStateV2Error::CollateralBelowMinimum { bond: *bond, got: *collateral });
            }
            if operator_pubkey.is_empty() {
                return Err(PalwStateV2Error::EmptyOperatorKey(*bond));
            }
            // A zero payload is the P2PKH template over an all-zero address hash — a perfectly
            // well-formed script that nobody holds the preimage to. Every reward this bond ever
            // matured would be minted straight into an unspendable output, and the producer would
            // find out block by block. Registration is where that is knowable, so it is where it
            // is refused.
            if *payout_payload == Hash64::default() {
                return Err(PalwStateV2Error::EmptyPayoutPayload(*bond));
            }
            builder.write_bond(
                *bond,
                Some(PalwBondStateV2 {
                    pubkey: pubkey.clone(),
                    operator_id: palw_operator_id_v2(operator_pubkey),
                    collateral: *collateral,
                    slashed: 0,
                    status: PalwBondStatusV2::Active,
                    registered_daa: ctx.daa_score,
                    payout_payload: *payout_payload,
                }),
            );
        }
        PalwConsensusObjectV2::BondRetireRequested { bond } => {
            let record = builder.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?.clone();
            match record.status {
                PalwBondStatusV2::Retiring { .. } => return Err(PalwStateV2Error::BondAlreadyRetiring(*bond)),
                PalwBondStatusV2::Active => {
                    let mut retiring = record;
                    retiring.status = PalwBondStatusV2::Retiring { since_daa: ctx.daa_score };
                    builder.write_bond(*bond, Some(retiring));
                }
            }
        }
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root,
            slash_value_per_pwu,
            pwu_rule,
            initial_target,
            share_permille,
            activation_daa,
            // The admission carriage is the ACCEPTANCE layer's (ADR-0049 Decision H): running
            // `verify_class_admission_v2` here would put a graph walk inside a pure transition,
            // which is the same boundary `BondRegistered` draws for signatures. The transition
            // enforces referential integrity and the lattice; the gate decides whether the object
            // may be folded at all.
            admission: _,
        } => {
            if builder.state.classes.contains_key(class_id) {
                return Err(PalwStateV2Error::DuplicateClass(*class_id));
            }
            if *initial_target == 0 {
                return Err(PalwStateV2Error::ZeroClassTarget(*class_id));
            }
            // **Audit H3.** `reserved = pwu × slash_value_per_pwu` is the collateral a claim puts
            // at risk, and admission's ceiling (Decision 6 item 8) compares it against the bond's
            // headroom. At zero every claim reserves zero, so ANY number of immature claims fits
            // under ANY ceiling — the per-bond exposure cap, which is the whole of P0-10's remedy,
            // silently evaluates to no cap at all. A class whose work cannot be slashed is not a
            // cheap class, it is an unbonded one.
            if *slash_value_per_pwu == 0 {
                return Err(PalwStateV2Error::ZeroSlashValue(*class_id));
            }
            // ADR-0045 Decision 1: both rule shapes must license SOMETHING. A `DerivedV1` with a
            // zero per-inference cost would derive pwu = 0 for every attempt while the stateless
            // layer requires pwu ≥ 1 — a class nobody can ever mine, registered as if it worked.
            // A zero ceiling is the same dead class in the older costume.
            match pwu_rule {
                PalwPwuRuleV2::MaxPerAttempt(0) => return Err(PalwStateV2Error::ZeroPwuCeiling(*class_id)),
                PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 0 } => {
                    return Err(PalwStateV2Error::ZeroPwuPerInference(*class_id));
                }
                _ => {}
            }
            // **A registration with a future activation takes no share yet (condition 12).**
            //
            // The share table is validated here either way — a registration whose share could
            // never be granted must fail at registration, not silently at the activation edge
            // where nobody is watching — but it is only WRITTEN when the class becomes active.
            let table = granted_share_table_v2(builder.params, &builder.state.class_shares, *class_id, *share_permille)?;
            let weightless = *activation_daa > ctx.daa_score;
            if !weightless {
                // ADR-0045 Decision 3: the share table mutates HERE and at the activation edge,
                // and nowhere else. The first class funds the liveness floor whole; every later
                // entrant is funded by donation, and these writes are the only way a permille
                // moves.
                for (id, share) in table {
                    if builder.state.class_shares.get(&id).copied() != Some(share) {
                        builder.write_share(id, Some(share));
                    }
                }
            }
            builder.write_class(
                *class_id,
                Some(PalwClassStateV2 {
                    artifact_root: *artifact_root,
                    slash_value_per_pwu: *slash_value_per_pwu,
                    pwu_rule: *pwu_rule,
                    status: if weightless {
                        PalwClassStatusV2::Registered {
                            activation_daa: *activation_daa,
                            pending_share_permille: *share_permille,
                        }
                    } else {
                        PalwClassStatusV2::Active
                    },
                    registered_daa: ctx.daa_score,
                }),
            );
            builder.write_target(*class_id, Some(PalwClassTargetV2 { target: *initial_target }));
            // The receipt lane seeds from the same initial target (ADR-0044): the two lanes'
            // retargets separate them from here, against their own censuses. One registration
            // field, two slots — a second declared number would be a second fact to drift.
            builder.write_receipt_target(*class_id, Some(PalwClassTargetV2 { target: *initial_target }));
        }
        PalwConsensusObjectV2::ClassFrozen { class_id, certificate } => {
            let record = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?.clone();
            if let PalwClassStatusV2::Frozen { .. } = record.status {
                return Err(PalwStateV2Error::FrozenClass(*class_id));
            }
            // **ADR-0039 W6′: the liveness floor may never be absent.** Nothing refused freezing
            // it, and on a `ConsensusV2` network the consequence is total: the attempt lane is the
            // only block type, admission refuses a frozen class, and BASE-0 is the class every
            // operator can run — so one accepted `ClassFrozen` naming the floor ends the chain,
            // with no path back because the object that would unfreeze it needs a block. A
            // contradiction certificate against the floor is real evidence and it belongs on
            // chain, but its remedy is a new ruleset, not a switch any block may throw.
            if *class_id == builder.params.base_class_id {
                return Err(PalwStateV2Error::BaseClassMayNotFreeze(*class_id));
            }
            // The structural half of the proof, checked HERE because it is the half a pure
            // transition can decide. Signatures are the acceptance layer's, the same split
            // `BondRegistered` uses — and `adjudicate_class_contradiction_v1` is the function
            // that runs the whole thing where a verifier is in hand.
            check_class_contradiction_shape_v2(*class_id, certificate)?;
            let mut frozen = record;
            frozen.status = PalwClassStatusV2::Frozen { since_daa: ctx.daa_score };
            builder.write_class(*class_id, Some(frozen));
        }
        PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor, seats } => {
            if seats.is_empty() {
                return Err(PalwStateV2Error::EmptyPanel);
            }
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            let PalwClaimPhaseV2::Provisional = claim.phase else {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "PanelBound" });
            };
            builder.write_panel(*claim_id, Some(PalwPanelStateV2 { anchor: *anchor, seats: seats.clone(), bound_daa: ctx.daa_score }));
            let mut bound = claim;
            bound.phase = PalwClaimPhaseV2::PanelBound { bound_daa: ctx.daa_score };
            builder.write_claim(*claim_id, Some(bound));
            builder.disarm_deadline(*claim_id);
            let deadline =
                ctx.daa_score.checked_add(builder.params.window_receipt).ok_or(PalwStateV2Error::Overflow("receipt deadline"))?;
            builder.arm_deadline(deadline, *claim_id);
        }
        PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts } => {
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            let PalwClaimPhaseV2::PanelBound { .. } = claim.phase else {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "ReceiptLicensed" });
            };
            // The panel concluded the data WAS served. A seat that signed `Unavailable` accused
            // the producer of withholding what a quorum of its own panel then verified — and the
            // majority invariant (`2·quorum > seat_count`) is what makes that a contradiction
            // rather than a disagreement: both verdicts cannot reach quorum, so exactly one of
            // them is refuted by the record. It pays what it tried to take: the same `reserved`
            // the producer would have lost.
            builder.slash_dissenting_seats(&claim, receipts, true)?;
            // …and the seats that said nothing while their panel concluded without them (P0-7).
            builder.slash_silent_seats(claim_id, &claim, receipts)?;
            let mut licensed = claim;
            licensed.phase = PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: ctx.daa_score };
            builder.write_claim(*claim_id, Some(licensed));
            builder.disarm_deadline(*claim_id);
            if !builder.state.open_courts_by_claim.contains_key(claim_id) {
                let deadline = ctx
                    .daa_score
                    .checked_add(builder.params.window_challenge)
                    .ok_or(PalwStateV2Error::Overflow("challenge deadline"))?;
                builder.arm_deadline(deadline, *claim_id);
            }
        }
        PalwConsensusObjectV2::CourtOpened { session_id, claim: claim_id, challenger_bond, space, space_size } => {
            if builder.state.court_sessions.contains_key(session_id) {
                return Err(PalwStateV2Error::DuplicateSession(*session_id));
            }
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            if claim.phase.is_terminal() {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "CourtOpened" });
            }
            builder.state.bonds.get(challenger_bond).ok_or(PalwStateV2Error::MissingBond(*challenger_bond))?;
            let deadline_daa =
                ctx.daa_score.checked_add(builder.params.window_court).ok_or(PalwStateV2Error::Overflow("court deadline"))?;
            // The responder's first rung window. Sized from the state machine's own
            // `turn_deadline_daa` so the ladder and the backstop are measured on one clock.
            let first_deadline_daa = ctx
                .daa_score
                .checked_add(builder.params.turn_deadline_daa)
                .ok_or(PalwStateV2Error::Overflow("rung deadline"))?;
            // The ladder's id is derived from the same six inputs `court_session_id_v2` uses, so
            // a ladder whose id is not `session_id` means the object's declared space does not
            // match the id it was opened under — refused here, not merely at the acceptance
            // layer, because the transition is what every node recomputes.
            let ladder = crate::palw_bisect::PalwBisectLadderV1::open(
                claim_id,
                &claim.trace_root,
                &crate::palw_court_v2::court_party_id_v2(challenger_bond),
                &crate::palw_court_v2::court_party_id_v2(&claim.bond),
                *space,
                *space_size,
                ctx.daa_score,
                first_deadline_daa,
            )
            .map_err(|e| PalwStateV2Error::LadderRefused(*session_id, e.to_string()))?;
            if ladder.session_id() != *session_id {
                return Err(PalwStateV2Error::SessionIdMismatch(*session_id));
            }
            builder.write_court(
                *session_id,
                Some(PalwCourtSessionStateV2 {
                    claim: *claim_id,
                    challenger_bond: *challenger_bond,
                    opened_daa: ctx.daa_score,
                    deadline_daa,
                    ladder,
                }),
            );
            // An open court freezes the path to Final: the claim keeps no deadline while any
            // session is open (void-by-timeout of the COURT is PR-07's deadline system).
            if matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
                builder.disarm_deadline(*claim_id);
            }
        }
        PalwConsensusObjectV2::CourtClosed { session_id, verdict, proof: _ } => {
            let session = builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            builder.write_court(*session_id, None);
            let claim_id = session.claim;
            let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
            match verdict {
                PalwCourtVerdictV2::ExecutorGuilty => {
                    // A verdict against an already-terminal claim closes the session and changes
                    // nothing else: a late verdict must not brick the block that carries it, and
                    // Final-vs-guilty contradictions are the deep-reorg surface PR-07/PR-08 own.
                    if !claim.phase.is_terminal() {
                        // A proven arithmetic fault is the one conviction the ruleset can make
                        // from evidence alone, so it is the one that most obviously must cost
                        // the executor its stake (audit C5: it did not).
                        builder.void_and_slash(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::CourtFraud)?;
                    }
                }
                PalwCourtVerdictV2::ChallengerDefeated => {
                    rearm_after_challenger_side_close(builder, ctx, claim_id, &claim)?;
                }
            }
        }
        PalwConsensusObjectV2::CourtDisclosed { session_id, disclosure, signature: _ } => {
            let mut session =
                builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            session
                .ladder
                .apply_disclosure(disclosure, ctx.daa_score, builder.params.turn_deadline_daa)
                .map_err(|e| PalwStateV2Error::LadderRefused(*session_id, e.to_string()))?;
            builder.write_court(*session_id, Some(session));
        }
        PalwConsensusObjectV2::CourtVerdictPosted { session_id, verdict, signature: _ } => {
            let mut session =
                builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            session
                .ladder
                .apply_verdict(verdict, ctx.daa_score, builder.params.turn_deadline_daa)
                .map_err(|e| PalwStateV2Error::LadderRefused(*session_id, e.to_string()))?;
            builder.write_court(*session_id, Some(session));
        }
        PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id, receipts } => {
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            if claim.phase.is_terminal() {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "ProducerDefaulted" });
            }
            // Symmetric to the licensing arm: here the quorum says the producer withheld, so a
            // seat that signed `Valid` is the contradicted one. Punishing only one direction
            // would make the cheap lie obvious.
            builder.slash_dissenting_seats(&claim, receipts, false)?;
            builder.slash_silent_seats(claim_id, &claim, receipts)?;
            // Decision 7's default is the producer's fault by construction — the panel answered
            // and it did not — so this void takes the stake, unlike the two timeouts, which void
            // a claim nobody was in a position to blame the producer for.
            builder.void_and_slash(*claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ProducerWithholding)?;
        }
        PalwConsensusObjectV2::FreePromptCommitted {
            claim: claim_id,
            class_id,
            bond,
            pwu,
            quanta,
            trace_root,
            output_root,
            execution_root,
            trace_chunk_count,
            trace_retention_daa,
        } => {
            if builder.state.claims.contains_key(claim_id) {
                return Err(PalwStateV2Error::DuplicateClaim(*claim_id));
            }
            let bond_record = builder.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?;
            if let PalwBondStatusV2::Retiring { .. } = bond_record.status {
                return Err(PalwStateV2Error::RetiringBond(*bond));
            }
            let class = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?;
            if let PalwClassStatusV2::Frozen { .. } = class.status {
                return Err(PalwStateV2Error::FrozenClass(*class_id));
            }
            if *quanta == 0 {
                return Err(PalwStateV2Error::ZeroQuanta);
            }
            if *pwu % (*quanta as u64) != 0 || *pwu / (*quanta as u64) == 0 {
                return Err(PalwStateV2Error::NonUniformQuanta { pwu: *pwu, quanta: *quanta });
            }
            // **Fail-closed on the one field the court cannot do without (audit C3).**
            // `adjudicate_court_close_v2` binds a refutation to the CLAIM's `execution_root`; a
            // claim carrying none has nothing to bind against, so every dispute about it dies at
            // `ExecutionRootMismatch` and its producer can commit arithmetic fraud with impunity.
            // Refusing the claim is the only safe reading — admitting it and hoping no fraud
            // occurs is precisely the fail-open shape the consumer-layer audit found ten of.
            //
            // Today this refuses every commitment the free-prompt worker can build: its v3
            // execution path emits a schedule commitment and a trace root but captures no legs,
            // so it has no `PalwStepBindingV2` to recompute a root from and deliberately emits
            // the null one rather than a fabricated value. That is the honest state of the lane —
            // the remaining work is legs capture on the free-prompt execution path, and until it
            // lands the chain says so at admission instead of at a dispute nobody can win.
            if *execution_root == Hash64::default() {
                return Err(PalwStateV2Error::UnadjudicableCommitment(*claim_id));
            }
            let reserved =
                (*pwu as u128).checked_mul(class.slash_value_per_pwu as u128).ok_or(PalwStateV2Error::Overflow("reserve"))?;
            let claim = PalwClaimStateV2 {
                source: PalwClaimSourceV2::FreePrompt { quanta: *quanta, spent: BTreeSet::new() },
                class_id: *class_id,
                bond: *bond,
                pwu: *pwu,
                accepted_daa: ctx.daa_score,
                accepted_blue_score: ctx.blue_score,
                accepted_block: ctx.block,
                trace_root: *trace_root,
                output_root: *output_root,
                execution_root: *execution_root,
                trace_chunk_count: *trace_chunk_count,
                trace_retention_daa: *trace_retention_daa,
                reserved,
                // Zero, deliberately (ADR-0044): a commitment riding a transaction is not a
                // block's work — β credit here would let commitment-stuffing pump a chain's live
                // weight without producing anything.
                immature_contribution: 0,
                // Zero for the SAME reason, and it is what keeps the subsidy bound structural: a
                // block may carry many commitments but only one attempt, so letting a commitment
                // draw the producer carve would let N of them mint N carves out of one subsidy —
                // precisely the "addition to the schedule" ADR-0042 Decision 10 forbids. The
                // receipt lane is paid by the spends it serves, not by the block that carried it.
                escrowed_reward: 0,
                phase: PalwClaimPhaseV2::Provisional,
            };
            builder.reserve_for_claim(&claim)?;
            builder.write_claim(*claim_id, Some(claim));
            let deadline =
                ctx.daa_score.checked_add(builder.params.window_bind).ok_or(PalwStateV2Error::Overflow("bind deadline"))?;
            builder.arm_deadline(deadline, *claim_id);
            // No production census here: commitments are not blocks. The receipt lane's counters
            // move when a quantum is SPENT.
        }
    }
    Ok(())
}

/// Apply a receipt block's own work: spend one certified quantum (ADR-0044 Decision 6 as the
/// state sees it). Admission (FP-04) already checked the beacon fact, the ticket, the use
/// window, the producer bond identity and the signature — this transition enforces what the
/// STATE alone can know: the claim exists, is free-prompt, is certified, and this quantum is
/// unspent on this chain. The weight a spend adds is the claim's uniform per-quantum weight,
/// re-derived — never carried.
fn apply_receipt_spend(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    spend: &PalwReceiptSpendUnsignedV3,
) -> Result<(), PalwStateV2Error> {
    let claim_id = spend.claim_id;
    let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
    let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source else {
        return Err(PalwStateV2Error::NotFreePromptClaim(claim_id));
    };
    if !matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) {
        return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "ReceiptSpend" });
    }
    if spend.quantum_index >= *quanta {
        return Err(PalwStateV2Error::QuantumOutOfRange { claim: claim_id, index: spend.quantum_index, quanta: *quanta });
    }
    if spent.contains(&spend.quantum_index) {
        return Err(PalwStateV2Error::QuantumAlreadySpent { claim: claim_id, index: spend.quantum_index });
    }
    let per_quantum = claim.pwu / (*quanta as u64);
    builder.state.safe_weight =
        builder.state.safe_weight.checked_add(per_quantum as u128).ok_or(PalwStateV2Error::Overflow("safe_weight"))?;
    let mut updated = claim.clone();
    let PalwClaimSourceV2::FreePrompt { spent: ledger, .. } = &mut updated.source else { unreachable!("matched above") };
    ledger.insert(spend.quantum_index);
    builder.write_claim(claim_id, Some(updated));

    // Receipt-lane production census — what the receipt retarget measures.
    let epoch_index = ctx.daa_score / builder.params.epoch_length;
    let previous = builder.state.receipt_epoch_counters.get(&claim.class_id).cloned();
    let counter = match previous {
        Some(counter) if counter.epoch_index == epoch_index => PalwEpochCounterV2 {
            epoch_index,
            produced_pwu: counter
                .produced_pwu
                .checked_add(per_quantum as u128)
                .ok_or(PalwStateV2Error::Overflow("receipt epoch produced_pwu"))?,
            produced_blocks: counter
                .produced_blocks
                .checked_add(1)
                .ok_or(PalwStateV2Error::Overflow("receipt epoch produced_blocks"))?,
        },
        _ => PalwEpochCounterV2 { epoch_index, produced_pwu: per_quantum as u128, produced_blocks: 1 },
    };
    builder.write_receipt_epoch(claim.class_id, Some(counter));
    Ok(())
}

/// `⌊subsidy · worker_carve / 1000⌋` — the producer's escrow, floored.
///
/// Floor on the producer side for the reason [`crate::palw_reward_v2::palw_reward_carve_v2`]
/// floors: rounding must never mint a sompi the emission schedule does not contain. The two agree
/// by construction because this IS that function, called with the state's own carve — a second
/// arithmetic here is a second answer waiting to disagree.
fn worker_carve_v2(params: &PalwStateParamsV2, subsidy: u64) -> u64 {
    crate::palw_reward_v2::palw_reward_carve_v2(
        subsidy,
        // Infallible: the field is fenced at `with_worker_carve_permille`, which is the only way
        // to set it, so it is always a legal permille.
        &crate::palw_reward_v2::PalwRewardParamsV2::new(params.worker_carve_permille)
            .expect("worker_carve_permille is fenced at construction"),
    )
    .worker
}

fn apply_attempt(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    envelope: &PalwAttemptEnvelopeV2,
) -> Result<(), PalwStateV2Error> {
    let attempt = &envelope.attempt;
    let claim_id = attempt_id_v2(attempt);
    if builder.state.claims.contains_key(&claim_id) {
        return Err(PalwStateV2Error::DuplicateClaim(claim_id));
    }
    let bond_key = PalwBondKeyV2(attempt.executor_bond);
    let bond = builder.state.bonds.get(&bond_key).ok_or(PalwStateV2Error::MissingBond(bond_key))?;
    if let PalwBondStatusV2::Retiring { .. } = bond.status {
        return Err(PalwStateV2Error::RetiringBond(bond_key));
    }
    let class = builder.state.classes.get(&attempt.class_id).ok_or(PalwStateV2Error::MissingClass(attempt.class_id))?;
    if let PalwClassStatusV2::Frozen { .. } = class.status {
        return Err(PalwStateV2Error::FrozenClass(attempt.class_id));
    }
    let reserved =
        (attempt.pwu as u128).checked_mul(class.slash_value_per_pwu as u128).ok_or(PalwStateV2Error::Overflow("reserve"))?;

    let claim = PalwClaimStateV2 {
        source: PalwClaimSourceV2::Attempt,
        class_id: attempt.class_id,
        bond: bond_key,
        pwu: attempt.pwu,
        accepted_daa: ctx.daa_score,
        accepted_blue_score: ctx.blue_score,
        accepted_block: ctx.block,
        trace_root: attempt.trace_root,
        output_root: attempt.output_root,
        execution_root: attempt.execution_root,
        trace_chunk_count: attempt.trace_chunk_count,
        trace_retention_daa: attempt.trace_retention_daa,
        reserved,
        immature_contribution: immature_contribution_v2(builder.params, attempt.pwu),
        // An attempt claim IS this block, so the block's carve funds exactly one claim and the
        // "never exceeds the subsidy" bound is structural rather than arithmetic.
        escrowed_reward: worker_carve_v2(builder.params, ctx.subsidy),
        phase: PalwClaimPhaseV2::Provisional,
    };
    builder.reserve_for_claim(&claim)?;
    builder.write_claim(claim_id, Some(claim));
    let deadline = ctx.daa_score.checked_add(builder.params.window_bind).ok_or(PalwStateV2Error::Overflow("bind deadline"))?;
    builder.arm_deadline(deadline, claim_id);

    // Epoch production counter (ADR-0039 D5 as a counter; the budget PREDICATE reads it in PR-04).
    let epoch_index = ctx.daa_score / builder.params.epoch_length;
    let previous = builder.state.epoch_counters.get(&attempt.class_id).cloned();
    let counter = match previous {
        Some(counter) if counter.epoch_index == epoch_index => PalwEpochCounterV2 {
            epoch_index,
            produced_pwu: counter
                .produced_pwu
                .checked_add(attempt.pwu as u128)
                .ok_or(PalwStateV2Error::Overflow("epoch produced_pwu"))?,
            produced_blocks: counter.produced_blocks.checked_add(1).ok_or(PalwStateV2Error::Overflow("epoch produced_blocks"))?,
        },
        // A new epoch starts its count from this attempt; an absent counter is the same case.
        _ => PalwEpochCounterV2 { epoch_index, produced_pwu: attempt.pwu as u128, produced_blocks: 1 },
    };
    builder.write_epoch(attempt.class_id, Some(counter));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Delta application and reversal
// ---------------------------------------------------------------------------------------------

fn expect_matches<T: PartialEq + std::fmt::Debug>(found: &Option<T>, expected: &Option<T>) -> Result<(), PalwStateV2Error> {
    if found == expected { Ok(()) } else { Err(PalwStateV2Error::DeltaMismatch("an entry's old value is not this state's value")) }
}

/// Apply a recorded delta to the exact parent it was computed from. Every entry checks the value
/// it replaces, so applying to any other state is an error, not a quiet divergence. All indices —
/// deadlines included, which is why params are required — are rebuilt, so the result is
/// bit-identical to the transition that produced the delta (tested below); a state with a stale
/// sweep queue is not a state, it is a landmine.
pub fn apply_delta_v2(
    parent: &PalwChainStateV2,
    delta: &PalwStateDeltaV2,
    params: &PalwStateParamsV2,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let mut state = parent.clone();
    for entry in &delta.entries {
        apply_delta_entry(&mut state, entry, false)?;
    }
    rebuild_deadline_free_indices(&mut state);
    rebuild_deadline_index_v2(&mut state, params)?;
    Ok(state)
}

/// Revert a recorded delta from the exact child it produced, restoring the parent bit-for-bit
/// (tested below) — the reorg primitive a store layer needs.
pub fn revert_delta_v2(
    child: &PalwChainStateV2,
    delta: &PalwStateDeltaV2,
    params: &PalwStateParamsV2,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let mut state = child.clone();
    for entry in delta.entries.iter().rev() {
        apply_delta_entry(&mut state, entry, true)?;
    }
    rebuild_deadline_free_indices(&mut state);
    rebuild_deadline_index_v2(&mut state, params)?;
    Ok(state)
}

fn apply_delta_entry(state: &mut PalwChainStateV2, entry: &PalwDeltaEntryV2, revert: bool) -> Result<(), PalwStateV2Error> {
    // In revert mode the roles swap: verify `new`, install `old`.
    macro_rules! swap_write {
        ($map:expr, $key:expr, $old:expr, $new:expr) => {{
            let (expected, install) = if revert { ($new, $old) } else { ($old, $new) };
            expect_matches(&$map.get($key).cloned(), expected)?;
            match install {
                Some(value) => $map.insert(*$key, value.clone()),
                None => $map.remove($key),
            };
        }};
    }
    match entry {
        PalwDeltaEntryV2::Bond { key, old, new } => swap_write!(state.bonds, key, old, new),
        PalwDeltaEntryV2::Exposure { key, old, new } => swap_write!(state.reserved_exposure, key, old, new),
        PalwDeltaEntryV2::Class { key, old, new } => swap_write!(state.classes, key, old, new),
        PalwDeltaEntryV2::Target { key, old, new } => swap_write!(state.class_targets, key, old, new),
        PalwDeltaEntryV2::Share { key, old, new } => swap_write!(state.class_shares, key, old, new),
        PalwDeltaEntryV2::EpochBudgets { old, new } => {
            let (expected, install) = if revert { (new, old) } else { (old, new) };
            if state.epoch_budgets != *expected {
                return Err(PalwStateV2Error::DeltaMismatch("epoch budgets do not match the delta's expectation"));
            }
            state.epoch_budgets = install.clone();
        }
        PalwDeltaEntryV2::ReceiptTarget { key, old, new } => swap_write!(state.receipt_targets, key, old, new),
        PalwDeltaEntryV2::Capability { key, old, new } => swap_write!(state.capabilities, key, old, new),
        PalwDeltaEntryV2::Claim { key, old, new } => swap_write!(state.claims, key, old, new),
        PalwDeltaEntryV2::Payout { key, old, new } => swap_write!(state.pending_payouts, key, old, new),
        PalwDeltaEntryV2::Panel { key, old, new } => swap_write!(state.panels, key, old, new),
        PalwDeltaEntryV2::Court { key, old, new } => swap_write!(state.court_sessions, key, old, new),
        PalwDeltaEntryV2::Epoch { key, old, new } => swap_write!(state.epoch_counters, key, old, new),
        PalwDeltaEntryV2::ReceiptEpoch { key, old, new } => swap_write!(state.receipt_epoch_counters, key, old, new),
        PalwDeltaEntryV2::Weights { old, new } => {
            let (expected, install) = if revert { (new, old) } else { (old, new) };
            if (state.safe_weight, state.bounded_immature) != *expected {
                return Err(PalwStateV2Error::DeltaMismatch("weights do not match the delta's expectation"));
            }
            state.safe_weight = install.0;
            state.bounded_immature = install.1;
        }
        PalwDeltaEntryV2::Frontier { old, new } => {
            let (expected, install) = if revert { (new, old) } else { (old, new) };
            if (state.safe_frontier_blue_score, state.safe_frontier) != *expected {
                return Err(PalwStateV2Error::DeltaMismatch("frontier does not match the delta's expectation"));
            }
            state.safe_frontier_blue_score = install.0;
            state.safe_frontier = install.1;
        }
        PalwDeltaEntryV2::LastPoint { old, new } => {
            let (expected, install) = if revert { (new, old) } else { (old, new) };
            if state.last_point != *expected {
                return Err(PalwStateV2Error::DeltaMismatch("last point does not match the delta's expectation"));
            }
            state.last_point = *install;
        }
    }
    Ok(())
}

/// Rebuild the indices that delta entries do not carry (they are derivable). The deadline index
/// is rebuilt exactly only under params; delta application rebuilds the parameterless two and
/// leaves deadlines to [`rebuild_indices_v2`], which every store-facing load path calls.
fn rebuild_deadline_free_indices(state: &mut PalwChainStateV2) {
    state.unresolved = state
        .claims
        .iter()
        .filter(|(_, claim)| !claim.phase.is_terminal())
        .map(|(id, claim)| (claim.accepted_blue_score, *id))
        .collect();
    state.open_courts_by_claim = BTreeMap::new();
    state.court_deadlines = BTreeSet::new();
    for (id, session) in &state.court_sessions {
        *state.open_courts_by_claim.entry(session.claim).or_insert(0) += 1;
        state.court_deadlines.insert((court_next_deadline_v2(session), *id));
    }
}

/// Rebuild ALL indices from primary data. The deadline index needs params (window arithmetic);
/// the stored deadline of a court-cleared claim is reconstructed as `max(floor, cleared point)`…
/// which is not recoverable from primary data alone — which is exactly why delta application
/// carries deadlines implicitly: **this function is only valid on states whose claims are all in
/// canonical deadline positions**, i.e. anything loaded from carriage (whose consistency check
/// enforces it) or rebuilt through deltas that were verified against a transition.
fn rebuild_deadline_index_v2(state: &mut PalwChainStateV2, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    let mut deadlines = BTreeSet::new();
    for (id, claim) in &state.claims {
        let open_courts = state.open_courts_by_claim.get(id).copied().unwrap_or(0);
        match claim.phase {
            PalwClaimPhaseV2::Provisional => {
                deadlines.insert((
                    claim.accepted_daa.checked_add(params.window_bind).ok_or(PalwStateV2Error::Overflow("bind deadline"))?,
                    *id,
                ));
            }
            PalwClaimPhaseV2::PanelBound { bound_daa } => {
                deadlines.insert((
                    bound_daa.checked_add(params.window_receipt).ok_or(PalwStateV2Error::Overflow("receipt deadline"))?,
                    *id,
                ));
            }
            PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::BindTimeout }
                if params.fp_abandon_hold_daa > 0 && matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }) =>
            {
                // Audit C5's abandon hold, rebuilt on the same rule `void_claim` armed and
                // `assert_deadline_consistency` recomputes.
                let release_at =
                    voided_daa.checked_add(params.fp_abandon_hold_daa).ok_or(PalwStateV2Error::Overflow("abandon hold"))?;
                if state.last_point.as_ref().is_none_or(|point| point.daa_score <= release_at) {
                    deadlines.insert((release_at, *id));
                }
            }
            PalwClaimPhaseV2::ReceiptLicensed { .. } if open_courts > 0 => {}
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => {
                let floor =
                    licensed_daa.checked_add(params.window_challenge).ok_or(PalwStateV2Error::Overflow("challenge deadline"))?;
                // A court-cleared claim's re-armed deadline is `max(floor, clearing daa)`, and the
                // clearing daa itself is not primary data — but `max(floor, last_point.daa)`
                // reproduces the stored value EXACTLY for every state at rest:
                //
                // * never courted, still licensed ⇒ its floor has not been swept, so
                //   `floor ≥ last_daa` and the max is the floor — the stored value;
                // * cleared at daa C with no later block ⇒ `last_daa = C`, max(floor, C) — the
                //   stored value by the re-arm rule;
                // * cleared at C with later blocks at daa D > C ⇒ still licensed means the stored
                //   `max(floor, C)` survived the sweep at D, so `max(floor, C) ≥ D`, which with
                //   `D > C` forces `floor ≥ D` — both maxes are the floor.
                let last_daa = state.last_point.map(|p| p.daa_score).unwrap_or(0);
                deadlines.insert((floor.max(last_daa), *id));
            }
            PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => {}
        }
    }
    state.deadlines = deadlines;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Carriage
// ---------------------------------------------------------------------------------------------

/// The serializable snapshot of a state's PRIMARY data — what the pruning proof carries
/// (Decision 5) so a pruned node can continue exactly where an archival node stands. Indices are
/// never serialized: [`PalwStateCarriageV2::into_state`] rebuilds them and refuses a snapshot
/// whose derivable facts disagree with its claims.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwStateCarriageV2 {
    pub version: u16,
    pub bonds: BTreeMap<PalwBondKeyV2, PalwBondStateV2>,
    pub reserved_exposure: BTreeMap<PalwBondKeyV2, u128>,
    pub classes: BTreeMap<Hash64, PalwClassStateV2>,
    pub class_targets: BTreeMap<Hash64, PalwClassTargetV2>,
    pub class_shares: BTreeMap<Hash64, u16>,
    pub epoch_budgets: Option<PalwEpochBudgetsV2>,
    pub receipt_targets: BTreeMap<Hash64, PalwClassTargetV2>,
    pub capabilities: BTreeMap<Hash64, PalwCapabilityStateV2>,
    pub claims: BTreeMap<Hash64, PalwClaimStateV2>,
    /// Carried because `state_root` hashes it: a snapshot that dropped the queue would restore a
    /// state whose root does not match the chain's, and a node loading it would find every
    /// subsequent block's coinbase wrong.
    pub pending_payouts: BTreeMap<Hash64, PalwPayoutV2>,
    pub panels: BTreeMap<Hash64, PalwPanelStateV2>,
    pub court_sessions: BTreeMap<Hash64, PalwCourtSessionStateV2>,
    pub epoch_counters: BTreeMap<Hash64, PalwEpochCounterV2>,
    pub receipt_epoch_counters: BTreeMap<Hash64, PalwEpochCounterV2>,
    pub safe_weight: u128,
    pub bounded_immature: u128,
    pub safe_frontier_blue_score: u64,
    pub safe_frontier: BlockHash,
    pub last_point: Option<PalwBlockContextV2>,
}

impl PalwStateCarriageV2 {
    pub fn from_state(state: &PalwChainStateV2) -> Self {
        Self {
            version: PALW_STATE_V2_VERSION,
            bonds: state.bonds.clone(),
            reserved_exposure: state.reserved_exposure.clone(),
            classes: state.classes.clone(),
            class_targets: state.class_targets.clone(),
            class_shares: state.class_shares.clone(),
            epoch_budgets: state.epoch_budgets.clone(),
            receipt_targets: state.receipt_targets.clone(),
            capabilities: state.capabilities.clone(),
            claims: state.claims.clone(),
            pending_payouts: state.pending_payouts.clone(),
            panels: state.panels.clone(),
            court_sessions: state.court_sessions.clone(),
            epoch_counters: state.epoch_counters.clone(),
            receipt_epoch_counters: state.receipt_epoch_counters.clone(),
            safe_weight: state.safe_weight,
            bounded_immature: state.bounded_immature,
            safe_frontier_blue_score: state.safe_frontier_blue_score,
            safe_frontier: state.safe_frontier,
            last_point: state.last_point,
        }
    }

    /// `H(canonical(carriage))` — what a pruning proof commits to when it carries the snapshot.
    pub fn digest(&self) -> Hash64 {
        let bytes = borsh::to_vec(self).expect("PalwStateCarriageV2 is borsh-serializable");
        let mut state = keyed(PALW_STATE_V2_DOMAIN_CARRIAGE);
        state.update(&(bytes.len() as u64).to_le_bytes());
        state.update(&bytes);
        finish(state)
    }

    /// Reconstruct the live state: rebuild every index, then refuse the snapshot unless all of
    /// its derivable facts agree with its primary data AND its state root matches `expected_root`
    /// when one is supplied.
    ///
    /// `expected_root: None` is for LOCAL, already-trusted snapshots only (a node reloading its
    /// own disk). A peer-supplied carriage MUST come with the root the chain committed to:
    /// self-consistency alone cannot catch a tamper that adjusts a claim and nothing derivable
    /// from it — the claim's `reserved`/`immature_contribution` snapshots are the accounting
    /// basis by design, so a coherent lie about `pwu` is coherent. The root is what makes it a
    /// lie about a DIFFERENT state; "load then notice later" is how a poisoned summary becomes a
    /// sink.
    pub fn into_state(self, params: &PalwStateParamsV2, expected_root: Option<Hash64>) -> Result<PalwChainStateV2, PalwStateV2Error> {
        if self.version != PALW_STATE_V2_VERSION {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "carriage version {} is not {}",
                self.version, PALW_STATE_V2_VERSION
            )));
        }
        let mut state = PalwChainStateV2 {
            bonds: self.bonds,
            reserved_exposure: self.reserved_exposure,
            classes: self.classes,
            class_targets: self.class_targets,
            class_shares: self.class_shares,
            epoch_budgets: self.epoch_budgets,
            receipt_targets: self.receipt_targets,
            capabilities: self.capabilities,
            claims: self.claims,
            pending_payouts: self.pending_payouts,
            panels: self.panels,
            court_sessions: self.court_sessions,
            epoch_counters: self.epoch_counters,
            receipt_epoch_counters: self.receipt_epoch_counters,
            safe_weight: self.safe_weight,
            bounded_immature: self.bounded_immature,
            safe_frontier_blue_score: self.safe_frontier_blue_score,
            safe_frontier: self.safe_frontier,
            last_point: self.last_point,
            deadlines: BTreeSet::new(),
            unresolved: BTreeSet::new(),
            open_courts_by_claim: BTreeMap::new(),
            court_deadlines: BTreeSet::new(),
        };
        rebuild_deadline_free_indices(&mut state);
        rebuild_deadline_index_v2(&mut state, params)?;
        state.assert_internal_consistency(params)?;
        state.assert_deadline_consistency(params)?;
        if let Some(expected) = expected_root {
            let got = state.state_root();
            if got != expected {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "carriage state root {got} does not match the committed root {expected}"
                )));
            }
        }
        Ok(state)
    }
}

// ---------------------------------------------------------------------------------------------
// The state book — the store shim the differential gate runs against
// ---------------------------------------------------------------------------------------------

/// Per-block states keyed by chain block — the shape a store layer (PR-08) persists. It exists in
/// PR-03 so the sink-independence gate can be exercised against the access pattern a real node
/// will use: apply many branches into ONE book, then ask for a candidate's standing, and get an
/// answer that is a function of that candidate's chain alone.
pub struct PalwStateBookV2 {
    params: PalwStateParamsV2,
    states: BTreeMap<BlockHash, PalwChainStateV2>,
    deltas: BTreeMap<BlockHash, PalwStateDeltaV2>,
}

impl PalwStateBookV2 {
    pub fn new(params: PalwStateParamsV2) -> Self {
        Self { params, states: BTreeMap::new(), deltas: BTreeMap::new() }
    }

    /// Install the genesis point.
    pub fn insert_genesis(&mut self, genesis_block: BlockHash) {
        self.states.insert(genesis_block, PalwChainStateV2::genesis());
    }

    /// Apply one block on top of its selected parent's stored state.
    pub fn apply_block(
        &mut self,
        parent_block: BlockHash,
        ctx: PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        attempt: Option<&PalwAttemptEnvelopeV2>,
    ) -> Result<Hash64, PalwStateV2Error> {
        let work = match attempt {
            Some(envelope) => PalwBlockWorkV3::Attempt(envelope),
            None => PalwBlockWorkV3::None,
        };
        self.apply_block_with_work(parent_block, ctx, objects, work)
    }

    /// [`Self::apply_block`], with the full V3 work slot (ADR-0044) — the shape the FP wiring
    /// persists.
    pub fn apply_block_with_work(
        &mut self,
        parent_block: BlockHash,
        ctx: PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
    ) -> Result<Hash64, PalwStateV2Error> {
        let parent = self.states.get(&parent_block).ok_or(PalwStateV2Error::MissingParentState(parent_block))?;
        let (child, delta) = apply_palw_transition_v3(parent, &self.params, &ctx, objects, work)?;
        let root = child.state_root();
        self.states.insert(ctx.block, child);
        self.deltas.insert(ctx.block, delta);
        Ok(root)
    }

    pub fn state_of(&self, block: &BlockHash) -> Option<&PalwChainStateV2> {
        self.states.get(block)
    }

    pub fn delta_of(&self, block: &BlockHash) -> Option<&PalwStateDeltaV2> {
        self.deltas.get(block)
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptUnsignedV2, challenge_v2};
    use crate::palw_fork_choice::compare_palw_candidates_v1;
    use crate::tx::TransactionId;

    fn params() -> PalwStateParamsV2 {
        // base = h64(1), max_factor = 4, tolerance = 1000‰ (grant floor: 1‰ at E = 1000),
        // fp split = 1000 (pure-attempt: the receipt lane measures nothing — the V1 identity).
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap()
    }

    /// **Audit C-08's lock, in every state a bond can be in.**
    ///
    /// Nothing about a slash cost anything while this was missing: the collateral outpoint was
    /// spendable throughout, so `slashed` — documented as "it leaves `collateral` and enters
    /// circulation nowhere" — described a field and not a sompi.
    #[test]
    fn a_bonds_collateral_is_locked_while_the_bond_can_still_lose_it() {
        const DELAY: u64 = 6_000;
        let base = PalwBondStateV2 {
            pubkey: vec![7; 4],
            operator_id: h64(0xE0),
            collateral: 400_000,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: h64(0x9A11),
        };

        // Active: locked at every score, because it is backing claims and may take more.
        for now in [0, 1, 10_000, u64::MAX] {
            assert!(palw_bond_collateral_is_locked_v2(&base, now, DELAY), "an Active bond is locked at {now}");
        }

        // Retiring: locked until the withdrawal delay elapses, and the release side is inclusive —
        // the delay is over AT `since + delay`.
        let retiring = PalwBondStateV2 { status: PalwBondStatusV2::Retiring { since_daa: 1_000 }, ..base.clone() };
        assert!(palw_bond_collateral_is_locked_v2(&retiring, 1_000, DELAY));
        assert!(palw_bond_collateral_is_locked_v2(&retiring, 6_999, DELAY));
        assert!(!palw_bond_collateral_is_locked_v2(&retiring, 7_000, DELAY), "the delay is over at since + delay");
        assert!(!palw_bond_collateral_is_locked_v2(&retiring, u64::MAX, DELAY));

        // A delay that would overflow never elapses — the safe direction, not a wrap.
        let late = PalwBondStateV2 { status: PalwBondStatusV2::Retiring { since_daa: u64::MAX - 1 }, ..base.clone() };
        assert!(palw_bond_collateral_is_locked_v2(&late, u64::MAX, DELAY));

        // **Slashed: the lock is the ordinary one, and the burn is what makes the slash cost
        // something.** This arm used to hold a slashed bond forever, as the fail-closed placeholder
        // for a rule that did not exist. The rule exists, so the collateral releases on schedule
        // and the SPEND carries the obligation — the remainder is the owner's and the slashed sompi
        // are nobody's.
        let slashed = PalwBondStateV2 { slashed: 7, status: PalwBondStatusV2::Retiring { since_daa: 1_000 }, ..base.clone() };
        assert!(palw_bond_collateral_is_locked_v2(&slashed, 6_999, DELAY), "the delay still applies to a slashed bond");
        assert!(!palw_bond_collateral_is_locked_v2(&slashed, 7_000, DELAY), "and it ends on the ordinary schedule");
        assert_eq!(palw_bond_burn_obligation_v2(&slashed), 7, "what it lost is what its spend must destroy");
        assert_eq!(palw_bond_burn_obligation_v2(&base), 0, "a bond that never lost anything owes nothing");
    }

    /// Operator identities are DERIVED from a key now, so the fixtures carry a key and let the
    /// state machine mint the id — the same path a real registration takes.
    /// A structurally-valid class-contradiction certificate naming `class_id`: two attestations
    /// on one job context that disagree about what the job produced. Signature verification is
    /// the acceptance layer's, so the fixture carries the shape the transition actually reads.
    pub(crate) fn contradiction(class_id: Hash64) -> crate::palw_slash::PalwClassContradictionCertificateV1 {
        let mut ctx = crate::palw_step_refute::tests::skeleton_refutation().binding.job_context;
        ctx.runtime_class_id = class_id;
        let context_hash = ctx.context_hash();
        let att = |root: Hash64| crate::palw_slash::PalwExecutionAttestationV1 {
            version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
            executor_id: h64(0xE1),
            job_context_hash: context_hash,
            full_logits_trace_root: root,
            committed_root: root,
            bond_outpoint: bond_key(1).0,
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        };
        crate::palw_slash::PalwClassContradictionCertificateV1 {
            version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
            attestation_a: att(h64(0x1A)),
            attestation_b: att(h64(0x2B)),
            job_context: ctx,
        }
    }

    /// An entrant class — what a test freezes now that ADR-0039 W6′ refuses freezing the floor.
    pub(crate) fn entrant_class(class_id: Hash64, share_permille: u16) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: h64(12),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille,
            activation_daa: 0,
            admission: None,
        }
    }

    pub(crate) fn freeze(class_id: Hash64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassFrozen { class_id, certificate: contradiction(class_id) }
    }

    fn op_key(v: u64) -> Vec<u8> {
        vec![v as u8; 8]
    }

    fn op_id(v: u64) -> Hash64 {
        palw_operator_id_v2(&op_key(v))
    }

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn block(v: u64) -> BlockHash {
        BlockHash::from_u64_word(v)
    }

    fn bond_key(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
    }

    fn ctx(block_word: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: block(block_word), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    /// A `CourtOpened` whose `session_id` is the one `court_session_id_v2` derives — which the
    /// transition now REQUIRES, because the ladder it builds carries that id. The fixtures used
    /// to invent an id (`h64(500)`); no acceptance layer would ever have admitted one, so the
    /// helper is what those tests always meant.
    fn court_open(claim_id: Hash64, trace_root: Hash64, executor: PalwBondKeyV2, challenger: PalwBondKeyV2) -> PalwConsensusObjectV2 {
        const SPACE: crate::palw_bisect::PalwBisectSpaceV1 = crate::palw_bisect::PalwBisectSpaceV1::StepLeaves;
        const SIZE: u64 = 16;
        PalwConsensusObjectV2::CourtOpened {
            session_id: crate::palw_court_v2::court_session_id_v2(&claim_id, &trace_root, &executor, &challenger, SPACE, SIZE),
            claim: claim_id,
            challenger_bond: challenger,
            space: SPACE,
            space_size: SIZE,
        }
    }

    /// The id `court_open` produced, for the tests that close or sweep the session afterwards.
    fn court_session_of(claim_id: Hash64, trace_root: Hash64, executor: PalwBondKeyV2, challenger: PalwBondKeyV2) -> Hash64 {
        let PalwConsensusObjectV2::CourtOpened { session_id, .. } = court_open(claim_id, trace_root, executor, challenger) else {
            unreachable!("court_open builds a CourtOpened")
        };
        session_id
    }

    /// A SECOND session on the same dispute. With derived ids, "another court" means another
    /// index space — the same six inputs cannot produce two ids.
    fn second_court_open(claim_id: Hash64, trace_root: Hash64, executor: PalwBondKeyV2, challenger: PalwBondKeyV2) -> PalwConsensusObjectV2 {
        const SPACE: crate::palw_bisect::PalwBisectSpaceV1 = crate::palw_bisect::PalwBisectSpaceV1::StepLeaves;
        const SIZE: u64 = 32;
        PalwConsensusObjectV2::CourtOpened {
            session_id: crate::palw_court_v2::court_session_id_v2(&claim_id, &trace_root, &executor, &challenger, SPACE, SIZE),
            claim: claim_id,
            challenger_bond: challenger,
            space: SPACE,
            space_size: SIZE,
        }
    }

    fn second_court_session_of(claim_id: Hash64, trace_root: Hash64, executor: PalwBondKeyV2, challenger: PalwBondKeyV2) -> Hash64 {
        let PalwConsensusObjectV2::CourtOpened { session_id, .. } = second_court_open(claim_id, trace_root, executor, challenger) else {
            unreachable!("second_court_open builds a CourtOpened")
        };
        session_id
    }

    /// The concurring receipt the fixtures' single panel seat files.
    ///
    /// The fixtures used to license with `receipts: Vec::new()` — a panel concluding without a
    /// word from the seat it bound. That is now a no-show and costs the seat its stake (P0-7), so
    /// the fixtures say what a real licensing says: the seat that was assigned answered.
    fn seat_says(served: bool) -> Vec<PalwSeatVerdictV2> {
        vec![PalwSeatVerdictV2 { seat_bond: bond_key(1), served }]
    }

    fn register_class_and_bond() -> Vec<PalwConsensusObjectV2> {
        vec![
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
            PalwConsensusObjectV2::BondRegistered { bond: bond_key(1), pubkey: vec![7; 4], operator_pubkey: op_key(21), collateral: 1_000, payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11) },
        ]
    }

    #[allow(clippy::too_many_arguments)]
    fn attempt_for_class(
        pwu: u64,
        nonce: u64,
        class_id: Hash64,
        bond: PalwBondKeyV2,
        pubkey: Vec<u8>,
        operator_id: Hash64,
        artifact_root: Hash64,
    ) -> PalwAttemptEnvelopeV2 {
        let mut env = attempt(pwu, nonce);
        env.attempt.class_id = class_id;
        env.attempt.executor_bond = bond.0;
        env.attempt.executor_pubkey = pubkey;
        env.attempt.operator_id = operator_id;
        env.attempt.artifact_root = artifact_root;
        env.attempt.challenge = challenge_v2(env.attempt.network_domain, h64(5), 1_700, nonce, class_id, &bond.0);
        env
    }

    fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
        let network_domain = h64(999);
        let bond = bond_key(1).0;
        let challenge = challenge_v2(network_domain, h64(5), 1_700, nonce, h64(1), &bond);
        PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain,
                challenge,
                class_id: h64(1),
                executor_bond: bond,
                executor_pubkey: vec![7; 4],
                operator_id: op_id(21),
                artifact_root: h64(11),
                trace_root: h64(31),
                output_root: h64(32),
                pwu,
                trace_manifest_root: h64(33),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
                execution_root: h64(41),
            },
            signature: vec![0; 8],
        }
    }

    /// Convenience: apply and then run BOTH consistency checkers, so no test can quietly leave a
    /// state whose caches drifted.
    fn apply(
        parent: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        c: &PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        att: Option<&PalwAttemptEnvelopeV2>,
    ) -> (PalwChainStateV2, PalwStateDeltaV2) {
        let (state, delta) = apply_palw_transition_v2(parent, p, c, objects, att).expect("transition applies");
        state.assert_internal_consistency(p).expect("internal consistency after apply");
        state.assert_deadline_consistency(p).expect("deadline consistency after apply");
        (state, delta)
    }

    // ---- lattice ----

    /// The whole happy path, with the Decision 2 weight table checked at every stop.
    #[test]
    fn the_lattice_walks_provisional_to_final_and_weights_follow_the_table() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        assert_eq!(s2.bounded_immature(), 4, "β=100‰ of 40 pwu");
        assert_eq!(s2.safe_weight(), 0);
        assert_eq!(s2.reserved_exposure(&bond_key(1)), 200, "40 pwu × 5 sompi/pwu");
        assert!(matches!(s2.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Provisional));

        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s3, _) =
            apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        assert!(matches!(s3.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }));
        assert_eq!((s3.safe_weight(), s3.bounded_immature()), (0, 4), "binding moves no weight");

        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        assert!(matches!(s4.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        assert_eq!((s4.safe_weight(), s4.bounded_immature()), (0, 4), "licensing moves no weight");

        // The challenge window (20) from licensing at daa 103 ends at 123; the first block past
        // it sweeps the claim Final.
        let (s5, _) = apply(&s4, &p, &ctx(5, 124, 5), &[], None);
        assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        assert_eq!((s5.safe_weight(), s5.bounded_immature()), (40, 0), "Final: full pwu safe, immature released");
        assert_eq!(s5.reserved_exposure(&bond_key(1)), 0, "exposure released on Final");
        // The frontier names the block whose WORK matured — block 2, which carried the attempt —
        // not block 5, where the last transition happened to land. "Deepest block whose PALW work
        // is Final" is the definition `palw_fork_choice` states, and it is what makes the key
        // unforgeable: a fork can produce blocks freely, but it cannot produce a matured claim.
        assert_eq!(s5.safe_frontier(), (2, block(2)), "the frontier is the block whose work reached Final");
    }

    /// **ADR-0042 Decision 10, end to end: escrow at acceptance, release at `Final`, paid ONCE.**
    ///
    /// The lattice walk is the same one `palw_v2_claim_lifecycle` walks; what this measures is the
    /// money. Four separate claims, each of which the earlier "pay every Spendable claim" shape
    /// got wrong:
    ///
    /// 1. the escrow is the ACCEPTING block's carve, snapshotted — not the paying block's;
    /// 2. nothing is payable before `Final`;
    /// 3. the release lands in the queue for exactly one block, then is gone;
    /// 4. the payee is the bond's registered payload.
    #[test]
    fn palw_v2_escrow_is_carved_once_and_paid_once() {
        let p = params().with_worker_carve_permille(620).unwrap();
        let payout_payload = kaspa_hashes::Hash64::from_u64_word(0x9A11);
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        // The accepting block's subsidy is 1_000; 62% of it is 620.
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let accept = PalwBlockContextV2 { subsidy: 1_000, ..ctx(2, 101, 2) };
        let (s2, _) = apply(&s1, &p, &accept, &[], Some(&env));
        assert_eq!(s2.claim(&claim_id).unwrap().escrowed_reward, 620, "escrow is the accepting block's carve");
        assert!(s2.pending_payouts_iter().next().is_none(), "a Provisional claim is not payable");

        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) }];
        let (s3, _) =
            apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (s4, _) =
            apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        assert!(s4.pending_payouts_iter().next().is_none(), "still nothing payable while the claim is refutable");

        // The sweep that makes it Final. Note the subsidy here is 9_999_999 and DIFFERENT from the
        // accepting block's — if the payout were recomputed at maturity it would show up here.
        let mature = PalwBlockContextV2 { subsidy: 9_999_999, ..ctx(5, 124, 5) };
        let (s5, _) = apply(&s4, &p, &mature, &[], None);
        assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        let released: Vec<_> = s5.pending_payouts_iter().map(|(id, pay)| (*id, *pay)).collect();
        assert_eq!(
            released,
            vec![(claim_id, PalwPayoutV2 { payload: payout_payload, amount: 620 })],
            "the release is the ACCEPTING block's carve, paid to the bond's registered payload"
        );

        // The very next block pays it, and the queue empties. Any block after that pays nothing —
        // the claim is still `Final` forever, which is exactly why the queue exists.
        let (s6, _) = apply(&s5, &p, &ctx(6, 125, 6), &[], None);
        assert!(s6.pending_payouts_iter().next().is_none(), "the queue drains in the block that pays it");
        let (s7, _) = apply(&s6, &p, &ctx(7, 126, 7), &[], None);
        assert!(s7.pending_payouts_iter().next().is_none(), "and a Final claim is never paid a second time");
        assert!(matches!(s7.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "still Final, still unpaid again");
    }

    /// A zero carve — the default every pre-reward network keeps — escrows nothing and enqueues
    /// nothing, so the whole payout path stays invisible on a network that does not pay.
    #[test]
    fn palw_v2_a_zero_carve_never_enqueues_anything() {
        let p = params();
        assert_eq!(p.worker_carve_permille(), 0, "zero is the default");
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &PalwBlockContextV2 { subsidy: 1_000_000, ..ctx(2, 101, 2) }, &[], Some(&env));
        assert_eq!(s2.claim(&claim_id).unwrap().escrowed_reward, 0, "no carve, no escrow, whatever the subsidy");

        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) }];
        let (s3, _) =
            apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (s4, _) =
            apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        let (s5, _) = apply(&s4, &p, &ctx(5, 124, 5), &[], None);
        assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the lattice still runs");
        assert!(s5.pending_payouts_iter().next().is_none(), "and nothing was ever payable");
    }

    /// **A voided claim's escrow is never minted** — the property Decision 10 exists for.
    ///
    /// The `Provisional → Voided` path is the one a reward-at-acceptance design pays out on and
    /// then cannot claw back. Here the escrow is carved, sits in the claim, and dies with it.
    #[test]
    fn palw_v2_a_voided_claim_pays_nothing() {
        let p = params().with_worker_carve_permille(620).unwrap();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &PalwBlockContextV2 { subsidy: 1_000, ..ctx(2, 101, 2) }, &[], Some(&env));
        assert_eq!(s2.claim(&claim_id).unwrap().escrowed_reward, 620, "the escrow exists");

        // Never bound: the bind window (10) from daa 101 closes at 111, and the first block past
        // it sweeps the claim to Voided.
        let (s3, _) = apply(&s2, &p, &ctx(3, 112, 3), &[], None);
        assert!(matches!(s3.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { .. }), "swept to Voided");
        assert!(s3.pending_payouts_iter().next().is_none(), "a voided claim releases nothing");
        let (s4, _) = apply(&s3, &p, &ctx(4, 113, 4), &[], None);
        assert!(s4.pending_payouts_iter().next().is_none(), "and never will");
    }

    /// **The bound the earlier implementation broke.** Two claims maturing in one block must not
    /// mint two carves of one subsidy.
    ///
    /// A free-prompt commitment rides a transaction, so a block may carry many; only the attempt
    /// is the block's own work. `escrowed_reward: 0` on the commitment lane is what makes the
    /// bound structural — at most one attempt per block, so at most one carve per subsidy.
    ///
    /// So the block here carries BOTH lanes at once — three commitments and an attempt — because
    /// a test that measured only the attempt would pass no matter what the commitment lane
    /// escrowed, and the commitment lane is the half that can be repeated.
    #[test]
    fn palw_v2_only_the_block_s_own_work_draws_the_carve() {
        let p = params().with_worker_carve_permille(620).unwrap();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let subsidy = 1_000u64;
        let env = attempt(40, 1);
        let commitments = [fp_commit(0xF01, 8, 2), fp_commit(0xF02, 8, 2), fp_commit(0xF03, 8, 2)];
        let (s2, _) = apply(&s1, &p, &PalwBlockContextV2 { subsidy, ..ctx(2, 101, 2) }, &commitments, Some(&env));

        assert_eq!(s2.claims_iter().count(), 4, "three commitments and one attempt are all in state");
        let escrowed: u64 = s2.claims_iter().map(|(_, c)| c.escrowed_reward).sum();
        assert_eq!(escrowed, 620, "one attempt, one carve — the three commitments escrow nothing");
        assert!(escrowed <= subsidy, "so the block's escrow never exceeds the subsidy it came from");
        for word in [0xF01u64, 0xF02, 0xF03] {
            assert_eq!(s2.claim(&h64(word)).unwrap().escrowed_reward, 0, "commitment {word:#x} draws no carve");
        }
    }

    /// **The audit register's P0-3 red test.** A candidate tip whose claim has no descendants —
    /// no panel, no anchor, nothing after it — is `Provisional` and carries positive live
    /// weight IMMEDIATELY. The old weigher demanded a future anchor a fresh tip cannot have, so
    /// every live tip read `Unresolved`, PALW weight was never consulted, and fork choice
    /// silently fell back to blue work.
    #[test]
    fn palw_v2_fresh_tip_is_provisional_weighted() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (tip, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));

        assert!(
            matches!(tip.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Provisional),
            "a fresh tip is Provisional, not Unresolved"
        );
        let before = s1.candidate_order(h64(0xF1));
        let after = tip.candidate_order(h64(0xF1));
        assert!(after.live_total > before.live_total, "the fresh tip's claim weighs NOW — β·pwu, no panel required");
        assert_eq!(after.safe_weight, before.safe_weight, "and none of it is safe yet — the ramp is live-only");
    }

    /// Maturing never lowers the candidate order — the state and the comparator agree end to end.
    #[test]
    fn maturing_raises_the_candidate_order_through_the_state() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s3, _) =
            apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        let (s5, _) = apply(&s4, &p, &ctx(5, 124, 5), &[], None);

        let before = s4.candidate_order(h64(1000));
        let after = s5.candidate_order(h64(1000));
        assert!(after.live_total >= before.live_total, "maturing lowered live_total");
        assert_eq!(compare_palw_candidates_v1(&after, &before), std::cmp::Ordering::Greater);
    }

    // ---- timeouts and voids ----

    #[test]
    fn a_claim_nobody_binds_voids_at_its_bind_deadline_and_releases_everything() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        // Bind window 10 from daa 101 → deadline 111. A block at 111 could still bind; 112 sweeps.
        let (s3, _) = apply(&s2, &p, &ctx(3, 112, 3), &[], None);
        match s3.claim(&claim_id).unwrap().phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. } => {}
            ref other => panic!("expected BindTimeout void, got {other:?}"),
        }
        assert_eq!(s3.bounded_immature(), 0);
        assert_eq!(s3.reserved_exposure(&bond_key(1)), 0);
        assert_eq!(s3.safe_weight(), 0, "a voided claim mints nothing");
        // Voiding RESOLVES the past (it stops holding the prefix back) but matures nothing, so it
        // confers no frontier. A chain whose only claim was thrown out has done no provable work
        // and must not outrank one that has — the frontier stays where it started.
        assert_eq!(s3.safe_frontier(), PalwChainStateV2::genesis().safe_frontier(), "a voided claim matures nothing");
    }

    #[test]
    fn a_panel_that_never_receipts_voids_at_the_receipt_deadline() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s3, _) =
            apply(&s2, &p, &ctx(3, 105, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (s4, _) = apply(&s3, &p, &ctx(4, 116, 4), &[], None);
        match s4.claim(&claim_id).unwrap().phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. } => {}
            ref other => panic!("expected ReceiptTimeout void, got {other:?}"),
        }
    }

    #[test]
    fn a_producer_default_voids_the_claim_in_any_immature_phase() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        let (s3, _) = apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id, receipts: Vec::new() }], None);
        match s3.claim(&claim_id).unwrap().phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. } => {}
            ref other => panic!("expected ProducerWithholding void, got {other:?}"),
        }
    }

    // ---- courts ----

    #[test]
    fn an_open_court_blocks_final_and_a_cleared_court_rearms_it() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s3, _) =
            apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        let (s5, _) = apply(
            &s4,
            &p,
            &ctx(5, 104, 5),
            &[court_open(claim_id, h64(31), bond_key(1), bond_key(1))],
            None,
        );
        // Far past the challenge window: the claim must NOT final while the court is open.
        let (s6, _) = apply(&s5, &p, &ctx(6, 200, 6), &[], None);
        assert!(
            matches!(s6.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
            "an open court froze the path to Final"
        );
        // The court clears at daa 210; the deadline re-arms at max(floor=123, 210)=210 and the
        // NEXT block past it finals the claim.
        let (s7, _) = apply(
            &s6,
            &p,
            &ctx(7, 210, 7),
            &[PalwConsensusObjectV2::CourtClosed { session_id: court_session_of(claim_id, h64(31), bond_key(1), bond_key(1)), verdict: PalwCourtVerdictV2::ChallengerDefeated, proof: crate::palw_court_v2::PalwCourtVerdictProofV2::Arithmetic { refutation: crate::palw_step_refute::tests::skeleton_refutation(), operand_openings: Vec::new(),} }],
            None,
        );
        assert!(matches!(s7.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        let (s8, _) = apply(&s7, &p, &ctx(8, 211, 8), &[], None);
        assert!(matches!(s8.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    }

    #[test]
    fn a_guilty_verdict_voids_the_claim_and_a_late_verdict_changes_nothing() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        let (s3, _) = apply(
            &s2,
            &p,
            &ctx(3, 102, 3),
            &[court_open(claim_id, h64(31), bond_key(1), bond_key(1))],
            None,
        );
        let (s4, _) = apply(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::CourtClosed { session_id: court_session_of(claim_id, h64(31), bond_key(1), bond_key(1)), verdict: PalwCourtVerdictV2::ExecutorGuilty, proof: crate::palw_court_v2::PalwCourtVerdictProofV2::Arithmetic { refutation: crate::palw_step_refute::tests::skeleton_refutation(), operand_openings: Vec::new(),} }],
            None,
        );
        match s4.claim(&claim_id).unwrap().phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. } => {}
            ref other => panic!("expected CourtFraud void, got {other:?}"),
        }
        // A second court on the same (now voided) claim: opening is refused as WrongPhase — but a
        // session opened BEFORE the void closes cleanly without touching the terminal claim.
        let (s5, _) = apply(
            &s2,
            &p,
            &ctx(3, 102, 3),
            &[
                second_court_open(claim_id, h64(31), bond_key(1), bond_key(1)),
                PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id, receipts: Vec::new() },
            ],
            None,
        );
        let (s6, _) = apply(
            &s5,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::CourtClosed {
                session_id: second_court_session_of(claim_id, h64(31), bond_key(1), bond_key(1)),
                verdict: PalwCourtVerdictV2::ExecutorGuilty,
                proof: crate::palw_court_v2::PalwCourtVerdictProofV2::Arithmetic {
                    refutation: crate::palw_step_refute::tests::skeleton_refutation(),
                    operand_openings: Vec::new(),
                },
            }],
            None,
        );
        match s6.claim(&claim_id).unwrap().phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, voided_daa } => {
                assert_eq!(voided_daa, 102, "the earlier void stands; the late verdict only closed its session");
            }
            ref other => panic!("expected the standing void, got {other:?}"),
        }
        assert!(s6.court_session(&second_court_session_of(claim_id, h64(31), bond_key(1), bond_key(1))).is_none());
    }

    /// The backstop: a session nobody closes expires challenger-side at `opened + window_court`,
    /// and the frozen claim's path to Final re-arms — an unfinished challenge is not a freeze ray.
    #[test]
    fn an_abandoned_court_expires_challenger_side_and_the_claim_finals() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s3, _) =
            apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        let (s5, _) = apply(
            &s4,
            &p,
            &ctx(5, 104, 5),
            &[court_open(claim_id, h64(31), bond_key(1), bond_key(1))],
            None,
        );
        // Inside the court budget (opened at 104, window 500 → deadline 604): frozen.
        let (s6, _) = apply(&s5, &p, &ctx(6, 600, 6), &[], None);
        assert!(matches!(s6.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        assert!(s6.court_session(&court_session_of(claim_id, h64(31), bond_key(1), bond_key(1))).is_some());
        // Past it: the sweep closes the session challenger-side and re-arms Final at this point…
        let (s7, _) = apply(&s6, &p, &ctx(7, 605, 7), &[], None);
        assert!(
            s7.court_session(&court_session_of(claim_id, h64(31), bond_key(1), bond_key(1))).is_none(),
            "the abandoned session expired"
        );
        assert!(matches!(s7.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        // …and the next block past the re-armed deadline finals the claim.
        let (s8, _) = apply(&s7, &p, &ctx(8, 606, 8), &[], None);
        assert!(matches!(s8.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the freeze ended with the challenge");
    }

    // ---- conditions 12/13: a class registers weightless, soaks, then carries weight ----

    /// **A second class can be registered without taking a permille from the floor.**
    ///
    /// A class used to become `Active` the instant it was registered, which moved cadence share
    /// at that instant — including away from BASE-0, the liveness floor. So there was no way to
    /// put a class on a chain, watch it, and only then let it carry weight; and a soak that
    /// cannot run before activation proves nothing about what activation will do.
    ///
    /// This walks the whole arrangement: the floor keeps the whole table while the entrant is
    /// weightless, the entrant's attempts are refused with a reason that names the edge, and at
    /// the edge — reached by a CLOCK, with no object and nobody to authorise it — the share moves
    /// and the class becomes active.
    #[test]
    fn palw_v2_a_class_registers_weightless_and_activates_on_a_clock() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let entrant = h64(0x9E1);

        // Genesis: the floor at the whole table, plus a second class that activates at daa 500.
        let mut objects = register_class_and_bond();
        objects.push(PalwConsensusObjectV2::ClassRegistered {
            class_id: entrant,
            artifact_root: h64(0xA7),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 300,
            activation_daa: 500,
            admission: None,
        });
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &objects, None);

        // Registered, adjudicable, and holding nothing.
        assert!(s1.class(&entrant).is_some(), "the class is in the registry");
        assert_eq!(
            s1.class(&entrant).unwrap().status,
            PalwClassStatusV2::Registered { activation_daa: 500, pending_share_permille: 300 }
        );
        assert_eq!(s1.class_share_permille(&entrant), None, "and no share at all");
        assert_eq!(s1.class_share_permille(&h64(1)), Some(1000), "the floor keeps the whole table");
        // Its target and its receipt target ARE seeded, because a weightless class is still a
        // class: a dispute against it must adjudicate exactly as one against an active class.
        assert!(s1.class_target(&entrant).is_some());

        // Its attempts are refused, and the refusal names the edge rather than reading as a
        // missing budget three checks later.
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let mut env = attempt(40, 1);
        env.attempt.class_id = entrant;
        let err = crate::palw_admission_v2::check_palw_attempt_admission_v2(&s1, &p, &admission, &ctx(2, 101, 2), &env)
            .expect_err("a weightless class admits nothing");
        assert!(
            matches!(err, crate::palw_admission_v2::PalwAdmissionV2Error::ClassNotYetActive { activation_daa: 500, .. }),
            "got {err:?}"
        );

        // Blocks pass. Still weightless at 499 — the edge is `>=`, and one DAA short is short.
        let (s2, _) = apply(&s1, &p, &ctx(2, 499, 2), &[], None);
        assert_eq!(s2.class_share_permille(&entrant), None, "one DAA short of the edge is short");

        // And at the edge the clock does it: no object, nobody to authorise, nothing to forge.
        let (s3, _) = apply(&s2, &p, &ctx(3, 500, 3), &[], None);
        assert_eq!(s3.class(&entrant).unwrap().status, PalwClassStatusV2::Active);
        assert_eq!(s3.class_share_permille(&entrant), Some(300), "the entrant takes its share");
        assert_eq!(s3.class_share_permille(&h64(1)), Some(700), "funded by donation from the incumbent");
        // The table is exactly 1000 through the move — that is the invariant, not an assertion
        // about these two numbers.
        let total: u32 = [h64(1), entrant].iter().filter_map(|id| s3.class_share_permille(id)).map(|s| s as u32).sum();
        assert_eq!(total, 1000);

        // …and now it admits. The class had to become active for this to change, which is what
        // makes the refusal above a real gate rather than an unrelated failure.
        assert!(
            crate::palw_admission_v2::check_palw_attempt_admission_v2(&s3, &p, &admission, &ctx(4, 501, 4), &env).is_ok()
                || !matches!(
                    crate::palw_admission_v2::check_palw_attempt_admission_v2(&s3, &p, &admission, &ctx(4, 501, 4), &env),
                    Err(crate::palw_admission_v2::PalwAdmissionV2Error::ClassNotYetActive { .. })
                ),
            "after activation the class is no longer refused FOR BEING INACTIVE"
        );
    }

    /// `activation_daa: 0` is "now", which is what every registration meant before the field
    /// existed and what the genesis floor still means. Without this the addition would have made
    /// every existing registration weightless.
    #[test]
    fn palw_v2_a_zero_activation_is_immediate() {
        let p = params();
        let (s1, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        assert_eq!(s1.class(&h64(1)).unwrap().status, PalwClassStatusV2::Active);
        assert_eq!(s1.class_share_permille(&h64(1)), Some(1000));
    }

    // ---- P0-11: the lifecycle objects reach a chain through transactions ----

    /// **The liveness this network did not have.** A claim finalizes because a BLOCK can carry
    /// the objects that advance it.
    ///
    /// The lattice was complete and every edge tested, but every one of those tests handed the
    /// transition an object list it built in-process — the one thing a chain cannot do. On a real
    /// chain the objects are whatever an extractor produces from accepted transactions, and the
    /// only extractor produced `FreePromptCommitted`. So no block could carry a `PanelBound`,
    /// every claim sat `Provisional` until `window_bind` lapsed and voided as `BindTimeout`,
    /// `safe_weight` never grew, and PALW weight — the whole fork choice — was permanently zero.
    ///
    /// So this test refuses to invent the objects a block carries: the licensing comes out of
    /// `palw_lifecycle_objects_from_accepted_txs_v2`, from a transaction, which is the only way a
    /// carried object arrives on a chain. The panel binding is not carried at all — the CHAIN
    /// derives it (`palw_v2_derived_panel_bindings`, audit C5's tail), so what stands in for it
    /// here is the derivation's own output shape, and the assertion that a carried one is
    /// refused.
    #[test]
    fn palw_v2_a_claim_finalizes_from_objects_a_block_can_actually_carry() {
        use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        use crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
        use crate::tx::{ScriptPublicKey, Transaction, TransactionOutput};

        // The ONLY way an object enters this test: through a transaction, through the extractor.
        let via_block = |object: PalwConsensusObjectV2| -> Vec<PalwConsensusObjectV2> {
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object })
                .expect("borsh-serializable");
            let tx = Transaction::new(
                0,
                Vec::new(),
                vec![TransactionOutput::new(1, ScriptPublicKey::from_vec(0, vec![0x51]))],
                0,
                SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                payload,
            );
            let out = crate::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2(&[tx]);
            assert!(out.skipped.is_empty(), "the carrier must produce an object: {:?}", out.skipped);
            out.objects.into_iter().map(|c| c.object).collect()
        };

        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));

        // The binding the chain derives. It is NOT carried, and the carriage says so — the
        // pipeline synthesizes exactly this shape from the anchor and the registry.
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) }];
        let derived = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats };
        assert!(
            crate::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&derived).is_err(),
            "a panel binding must not be carriable: the chain derives it, and one question gets one answer"
        );
        let (s3, _) = apply(&s2, &p, &ctx(3, 102, 3), &[derived], None);
        assert!(matches!(s3.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "the chain bound the panel");

        let (s4, _) = apply(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &via_block(PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }),
            None,
        );
        assert!(matches!(s4.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));

        // The challenge window lapses with no court, and the claim certifies its work.
        let (s5, _) = apply(&s4, &p, &ctx(5, 124, 5), &[], None);
        assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the claim finalized");
        assert_eq!(s5.safe_weight(), 40, "and its work is certified — the number that was permanently zero");
        assert_eq!(s5.safe_frontier(), (2, block(2)), "the frontier names the block whose work matured");
    }

    // ---- P0-7: an assigned seat that answers nothing pays for it ----

    /// A claim walked to `PanelBound` with a THREE-seat panel, so silence is attributable to a
    /// particular seat rather than to "the panel".
    fn panel_bound_with_three_seats(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64) {
        let genesis = PalwChainStateV2::genesis();
        let mut objects = register_class_and_bond();
        for n in 2..=3u64 {
            objects.push(PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(n),
                pubkey: vec![7; 4],
                operator_pubkey: op_key(20 + n),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            });
        }
        let (s1, _) = apply(&genesis, p, &ctx(1, 100, 1), &objects, None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, p, &ctx(2, 101, 2), &[], Some(&env));
        let seats = vec![
            PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) },
            PalwPanelSeatV2 { bond: bond_key(2), operator_id: op_id(22) },
            PalwPanelSeatV2 { bond: bond_key(3), operator_id: op_id(23) },
        ];
        let (s3, _) =
            apply(&s2, p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        (s3, claim_id)
    }

    /// **P0-7's named red test.** An assigned seat past its deadline loses collateral.
    ///
    /// Three paths reach the end of a panel's duty, and silence had to be free on all three
    /// before this: the panel licenses without you, the panel defaults the producer without you,
    /// or the window simply closes. A bond could take seats forever, file nothing, and pay
    /// nothing — so the exposure a seat is supposed to put behind its verdict was only ever at
    /// risk if it chose to speak.
    #[test]
    fn palw_v2_panel_noshow_is_slashed() {
        let p = params();

        // (1) The panel licenses. Seat 1 says served, seat 2 says withheld and is refuted, seat 3
        //     says nothing at all — and all three now cost something.
        let (bound, claim_id) = panel_bound_with_three_seats(&p);
        let reserved = bound.claim(&claim_id).unwrap().reserved;
        assert!(reserved > 0, "the fixture must risk something or nothing below is measurable");
        let (licensed, _) = apply(
            &bound,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ReceiptLicensed {
                claim: claim_id,
                receipts: vec![
                    PalwSeatVerdictV2 { seat_bond: bond_key(1), served: true },
                    PalwSeatVerdictV2 { seat_bond: bond_key(2), served: false },
                ],
            }],
            None,
        );
        assert_eq!(licensed.bond(&bond_key(1)).unwrap().slashed, 0, "the seat that answered with the quorum keeps its stake");
        assert!(licensed.bond(&bond_key(2)).unwrap().slashed > 0, "the refuted seat pays — that part already worked");
        assert!(
            licensed.bond(&bond_key(3)).unwrap().slashed > 0,
            "and the seat that never answered pays too — this is what was free"
        );
        assert_eq!(
            licensed.bond(&bond_key(3)).unwrap().slashed,
            licensed.bond(&bond_key(2)).unwrap().slashed,
            "silence costs exactly what a refuted answer costs; pricing it lower makes silence the better play"
        );

        // (2) The panel defaults the producer. Same rule, other direction.
        let (bound, claim_id) = panel_bound_with_three_seats(&p);
        let (defaulted, _) = apply(
            &bound,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ProducerDefaulted {
                claim: claim_id,
                receipts: vec![
                    PalwSeatVerdictV2 { seat_bond: bond_key(1), served: false },
                    PalwSeatVerdictV2 { seat_bond: bond_key(2), served: false },
                ],
            }],
            None,
        );
        // Bond 1 is the executor here as well as a seat, and `ProducerDefaulted` charges the
        // producer by construction — so the seat that proves the rule is bond 2: it answered with
        // the quorum, it is not the producer, and it keeps its stake.
        assert_eq!(defaulted.bond(&bond_key(2)).unwrap().slashed, 0, "a seat that answered with the quorum keeps its stake");
        assert!(defaulted.bond(&bond_key(3)).unwrap().slashed > 0, "the absent seat pays on the default path too");

        // (3) Nobody concludes anything and the window closes: every seat was silent.
        let (bound, claim_id) = panel_bound_with_three_seats(&p);
        let (swept, _) = apply(&bound, &p, &ctx(4, 113, 4), &[], None);
        assert!(
            matches!(swept.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. }),
            "the receipt window closed"
        );
        for n in 1..=3u64 {
            assert!(swept.bond(&bond_key(n)).unwrap().slashed > 0, "seat {n} was assigned, answered nothing, and pays");
        }
        // The PRODUCER is not charged by a timeout — that blame belongs to `ProducerDefaulted`,
        // which requires a panel that actually answered. Here bond 1 is both, so the check is
        // that its charge is the SEAT's one and not two.
        assert_eq!(swept.bond(&bond_key(1)).unwrap().slashed, swept.bond(&bond_key(3)).unwrap().slashed);
    }

    /// A claim voided before it ever bound a panel charges nobody: nothing was assigned, so
    /// nobody owed an answer. Without this the no-show rule would reach back into `BindTimeout`,
    /// where the only party at fault is the producer.
    #[test]
    fn palw_v2_a_claim_that_never_bound_a_panel_slashes_no_seats() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        let (s3, _) = apply(&s2, &p, &ctx(3, 112, 3), &[], None);
        assert!(
            matches!(s3.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }),
            "swept for never binding"
        );
        assert_eq!(s3.bond(&bond_key(1)).unwrap().slashed, 0, "no panel, no seat, no charge");
    }

    // ---- P0-9: the ladder is chain state, so silence at a rung decides the dispute ----

    /// Params with a rung window STRICTLY inside the court budget — the configuration that turns
    /// the interactive ladder on. The default leaves rung == backstop, which the sweep treats as
    /// no rung clock at all.
    fn params_with_ladder() -> PalwStateParamsV2 {
        params().with_turn_deadline_daa(20).unwrap()
    }

    fn licensed_with_court(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64, Hash64) {
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&s1, p, &ctx(2, 101, 2), &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) }];
        let (s3, _) =
            apply(&s2, p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (s4, _) =
            apply(&s3, p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        let (s5, _) = apply(&s4, p, &ctx(5, 104, 5), &[court_open(claim_id, h64(31), bond_key(1), bond_key(1))], None);
        let sid = court_session_of(claim_id, h64(31), bond_key(1), bond_key(1));
        (s5, claim_id, sid)
    }

    fn disclose(sid: Hash64, round: u32, midpoint: u64, state_word: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::CourtDisclosed {
            session_id: sid,
            disclosure: crate::palw_bisect::PalwBisectDisclosureV1 {
                version: crate::palw_bisect::PALW_BISECT_OBJECT_VERSION_V1,
                session_id: sid,
                round,
                midpoint,
                mid_state: h64(state_word),
            },
            // The acceptance layer checks it (`check_court_disclosure_acceptance_v2`); the
            // transition applies the MOVE. Same split as `BondRegistered`.
            signature: vec![0xAA; 8],
        }
    }

    fn rung_verdict(sid: Hash64, round: u32, agree: bool) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::CourtVerdictPosted {
            session_id: sid,
            verdict: crate::palw_bisect::PalwBisectVerdictV1 {
                version: crate::palw_bisect::PALW_BISECT_OBJECT_VERSION_V1,
                session_id: sid,
                round,
                agree,
            },
            signature: vec![0xBB; 8],
        }
    }

    /// **The ladder narrows on chain.** Rung moves ride blocks, the state machine applies them,
    /// and the interval collapses to one index — at which point the ladder is `Terminal` and only
    /// an arithmetic close can finish the job.
    #[test]
    fn palw_v2_the_ladder_narrows_across_blocks() {
        let p = params_with_ladder();
        let (s5, _claim_id, sid) = licensed_with_court(&p);
        let ladder = &s5.court_session(&sid).unwrap().ladder;
        assert_eq!(ladder.interval(), (0, 16), "the dispute opens over the whole space");
        assert_eq!(ladder.turn(), crate::palw_bisect::PalwBisectTurnV1::AwaitDisclosure, "the responder moves first");
        assert_eq!(ladder.last_deadline_daa(), 124, "opened at 104 + a 20-DAA rung window");

        // Round 0: disclose at midpoint 8, challenger disagrees ⇒ the divergence is below.
        let (s6, _) = apply(&s5, &p, &ctx(6, 105, 6), &[disclose(sid, 0, 8, 0xD0)], None);
        assert_eq!(s6.court_session(&sid).unwrap().ladder.turn(), crate::palw_bisect::PalwBisectTurnV1::AwaitVerdict);
        let (s7, _) = apply(&s6, &p, &ctx(7, 106, 7), &[rung_verdict(sid, 0, false)], None);
        assert_eq!(s7.court_session(&sid).unwrap().ladder.interval(), (0, 8), "disagreement takes the lower half");

        // Three more rungs collapse [0,8) to a single index.
        let (s8, _) = apply(&s7, &p, &ctx(8, 107, 8), &[disclose(sid, 1, 4, 0xD1)], None);
        let (s9, _) = apply(&s8, &p, &ctx(9, 108, 9), &[rung_verdict(sid, 1, true)], None);
        assert_eq!(s9.court_session(&sid).unwrap().ladder.interval(), (4, 8), "agreement takes the upper half");
        let (s10, _) = apply(&s9, &p, &ctx(10, 109, 10), &[disclose(sid, 2, 6, 0xD2)], None);
        let (s11, _) = apply(&s10, &p, &ctx(11, 110, 11), &[rung_verdict(sid, 2, false)], None);
        let (s12, _) = apply(&s11, &p, &ctx(12, 111, 12), &[disclose(sid, 3, 5, 0xD3)], None);
        let (s13, _) = apply(&s12, &p, &ctx(13, 112, 13), &[rung_verdict(sid, 3, false)], None);

        let ladder = &s13.court_session(&sid).unwrap().ladder;
        assert_eq!(ladder.interval(), (4, 5), "one index wide");
        assert_eq!(ladder.terminal_index(), Some(4), "and the dispute is located");
        assert_eq!(ladder.turn(), crate::palw_bisect::PalwBisectTurnV1::Terminal, "only an arithmetic close finishes it now");
    }

    /// **A silent responder loses, and no object says so.** This is the whole reason the ladder
    /// had to become chain state: the verdict comes from the chain's own clock plus an absence,
    /// so there is no message for an attacker to forge.
    #[test]
    fn palw_v2_a_silent_responder_loses_the_dispute() {
        let p = params_with_ladder();
        let (s5, claim_id, sid) = licensed_with_court(&p);
        // The rung deadline is 124; the backstop is 604. Nothing is due yet at 124.
        let (s6, _) = apply(&s5, &p, &ctx(6, 124, 6), &[], None);
        assert!(s6.court_session(&sid).is_some(), "the rung window has not lapsed");
        assert!(matches!(s6.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));

        // One DAA past it, with the responder never having disclosed.
        let (s7, _) = apply(&s6, &p, &ctx(7, 125, 7), &[], None);
        assert!(s7.court_session(&sid).is_none(), "the session is decided and gone");
        match s7.claim(&claim_id).unwrap().phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. } => {}
            ref other => panic!("a responder that would not answer must lose the claim, got {other:?}"),
        }
        // And it cost the executor its stake, exactly as a proven fault does.
        assert!(s7.bond(&bond_key(1)).unwrap().slashed > 0, "a default is not free");
    }

    /// **A silent challenger loses too** — prosecution is its burden. The direction matters: the
    /// two outcomes are opposite, so a sweep that could not tell whose turn it was would decide
    /// half of all disputes backwards.
    #[test]
    fn palw_v2_a_silent_challenger_loses_the_dispute() {
        let p = params_with_ladder();
        let (s5, claim_id, sid) = licensed_with_court(&p);
        // The responder answers; now the challenger owes a verdict by 105 + 20 = 125.
        let (s6, _) = apply(&s5, &p, &ctx(6, 105, 6), &[disclose(sid, 0, 8, 0xD0)], None);
        assert_eq!(s6.court_session(&sid).unwrap().ladder.last_deadline_daa(), 125);

        let (s7, _) = apply(&s6, &p, &ctx(7, 126, 7), &[], None);
        assert!(s7.court_session(&sid).is_none(), "the session is decided and gone");
        assert!(
            matches!(s7.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
            "the claim survives an abandoned prosecution and resumes its path to Final"
        );
        assert_eq!(s7.bond(&bond_key(1)).unwrap().slashed, 0, "and the executor is not punished for the challenger's silence");
    }

    /// The default configuration has no ladder clock, so an unfinished challenge still ends at
    /// the BACKSTOP and still ends challenger-side. Without this, turning the ladder on by
    /// default would have silently inverted every such outcome.
    #[test]
    fn palw_v2_without_a_rung_window_the_backstop_still_decides() {
        let p = params();
        assert_eq!(p.turn_deadline_daa(), p.window_court(), "the default is one rung per session");
        let (s5, claim_id, sid) = licensed_with_court(&p);
        // Opened at 104: both clocks read 604. One past them, and the CHALLENGER-side close runs.
        let (s6, _) = apply(&s5, &p, &ctx(6, 605, 6), &[], None);
        assert!(s6.court_session(&sid).is_none());
        assert!(
            matches!(s6.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
            "the pre-ladder outcome, unchanged"
        );
        assert_eq!(s6.bond(&bond_key(1)).unwrap().slashed, 0, "and nobody is slashed for it");
    }

    // ---- strictness: a missing fact is an error ----

    #[test]
    fn absent_facts_are_errors_never_zeros() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let c = ctx(1, 100, 1);
        // Attempt against an empty state: no bond, no class.
        let env = attempt(40, 1);
        assert!(matches!(apply_palw_transition_v2(&genesis, &p, &c, &[], Some(&env)), Err(PalwStateV2Error::MissingBond(_))));
        // Panel for a claim that does not exist.
        assert!(matches!(
            apply_palw_transition_v2(
                &genesis,
                &p,
                &c,
                &[PalwConsensusObjectV2::PanelBound {
                    claim: h64(1),
                    anchor: h64(2),
                    seats: vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(3) }]
                }],
                None
            ),
            Err(PalwStateV2Error::MissingClaim(_))
        ));
        // Receipt for a claim that does not exist.
        assert!(matches!(
            apply_palw_transition_v2(&genesis, &p, &c, &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(1), receipts: Vec::new() }], None),
            Err(PalwStateV2Error::MissingClaim(_))
        ));
        // Closing a session that does not exist.
        assert!(matches!(
            apply_palw_transition_v2(
                &genesis,
                &p,
                &c,
                &[PalwConsensusObjectV2::CourtClosed { session_id: h64(1), verdict: PalwCourtVerdictV2::ExecutorGuilty, proof: crate::palw_court_v2::PalwCourtVerdictProofV2::Arithmetic { refutation: crate::palw_step_refute::tests::skeleton_refutation(), operand_openings: Vec::new(),} }],
                None
            ),
            Err(PalwStateV2Error::MissingSession(_))
        ));
        // Retiring a bond that does not exist.
        assert!(matches!(
            apply_palw_transition_v2(&genesis, &p, &c, &[PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(9) }], None),
            Err(PalwStateV2Error::MissingBond(_))
        ));
    }

    #[test]
    fn frozen_classes_and_retiring_bonds_take_no_new_claims_and_duplicates_are_refused() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        // Duplicate registrations.
        assert!(matches!(
            apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &register_class_and_bond()[..1], None),
            Err(PalwStateV2Error::DuplicateClass(_))
        ));
        assert!(matches!(
            apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &register_class_and_bond()[1..], None),
            Err(PalwStateV2Error::DuplicateBond(_))
        ));

        // Frozen class refuses the attempt — an ENTRANT, because ADR-0039 W6′ refuses freezing
        // the liveness floor (on a V2 network that would end the chain).
        let (with_entrant, _) = apply(&s1, &p, &ctx(2, 101, 2), &[entrant_class(h64(2), 500)], None);
        let (frozen, _) = apply(&with_entrant, &p, &ctx(3, 102, 3), &[freeze(h64(2))], None);
        let entrant_attempt = attempt_for_class(40, 1, h64(2), bond_key(1), vec![7; 4], op_id(21), h64(12));
        assert!(matches!(
            apply_palw_transition_v2(&frozen, &p, &ctx(4, 103, 4), &[], Some(&entrant_attempt)),
            Err(PalwStateV2Error::FrozenClass(_))
        ));

        // Retiring bond refuses the attempt.
        let (retiring, _) = apply(&s1, &p, &ctx(2, 101, 2), &[PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(1) }], None);
        assert!(matches!(
            apply_palw_transition_v2(&retiring, &p, &ctx(3, 102, 3), &[], Some(&attempt(40, 1))),
            Err(PalwStateV2Error::RetiringBond(_))
        ));

        // The same attempt twice is one claim id twice.
        let env = attempt(40, 1);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        assert!(matches!(
            apply_palw_transition_v2(&s2, &p, &ctx(3, 102, 3), &[], Some(&env)),
            Err(PalwStateV2Error::DuplicateClaim(_))
        ));
    }

    #[test]
    fn the_chain_context_must_be_monotone() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 5), &register_class_and_bond(), None);
        assert!(matches!(
            apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 5), &[], None),
            Err(PalwStateV2Error::NonMonotonicContext(_))
        ));
        assert!(matches!(apply_palw_transition_v2(&s1, &p, &ctx(2, 99, 6), &[], None), Err(PalwStateV2Error::NonMonotonicContext(_))));
    }

    // ---- the frontier ----

    /// A private fork cannot mature claims, so from its fork point its frontier never advances —
    /// and the comparator prefers the shorter chain that matured (the anti-fabrication ordering,
    /// end to end through the state).
    #[test]
    fn an_unmatured_pile_never_advances_the_frontier_and_loses_to_a_matured_chain() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (base, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        // Chain M: one claim, walked to Final.
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (m1, _) = apply(&base, &p, &ctx(2, 101, 2), &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (m2, _) =
            apply(&m1, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (m3, _) = apply(&m2, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        let (matured, _) = apply(&m3, &p, &ctx(5, 124, 5), &[], None);

        // Chain P: three heavier claims, none ever bound — a pile. (Blocks close enough together
        // that no bind deadline passes, so the pile keeps its immature weight.)
        let (p1, _) = apply(&base, &p, &ctx(12, 101, 2), &[], Some(&attempt(1000, 11)));
        let (p2, _) = apply(&p1, &p, &ctx(13, 102, 3), &[], Some(&attempt(1000, 12)));
        let (pile, _) = apply(&p2, &p, &ctx(14, 103, 4), &[], Some(&attempt(1000, 13)));

        assert_eq!(matured.safe_frontier(), (2, block(2)), "the matured chain's frontier is the block whose work Finaled");
        assert_eq!(pile.safe_frontier().0, 0, "the pile matured nothing, so it has no frontier at all");
        assert!(pile.bounded_immature() > matured.safe_weight(), "the pile really is heavier in raw immature weight");

        let matured_order = matured.candidate_order(block(5));
        let pile_order = pile.candidate_order(block(14));
        assert_eq!(
            compare_palw_candidates_v1(&matured_order, &pile_order),
            std::cmp::Ordering::Greater,
            "the matured chain outranks the heavier unproven pile"
        );
    }

    /// **Audit C2, half one: the frontier must advance on a chain that is DOING WORK.**
    ///
    /// The old rule advanced only when `unresolved` was globally empty — and step 4 of every
    /// apply inserts the block's own claim, so on a chain that produces a claim per block the
    /// condition was false forever and the frontier never left its starting point. Everything
    /// keyed on it went with it: `pruning_ceiling_v2` froze (a node that never prunes again) and
    /// the comparator's first key stopped separating honest chains.
    ///
    /// Steady state here: one claim accepted per block, bound the next block, licensed the one
    /// after, Final `window_challenge` later. So the frontier must trail the tip by a bounded
    /// lag — the liability window — and must NOT stall.
    #[test]
    fn the_frontier_advances_on_a_chain_that_keeps_producing_claims() {
        let p = params();
        let (mut state, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        let mut accepted: Vec<(u64, Hash64)> = Vec::new(); // (block word, claim id)
        let mut frontier_trace: Vec<u64> = Vec::new();
        for n in 2..=60u64 {
            let env = attempt(4, n);
            let claim_id = attempt_id_v2(&env.attempt);
            let mut objects = Vec::new();
            // Bind the claim accepted one block ago, license the one bound one block ago.
            if let Some((_, prev)) = accepted.last().copied() {
                objects.push(PalwConsensusObjectV2::PanelBound {
                    claim: prev,
                    anchor: h64(77),
                    seats: vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(21) }],
                });
            }
            if accepted.len() >= 2 {
                let (_, older) = accepted[accepted.len() - 2];
                objects.push(PalwConsensusObjectV2::ReceiptLicensed { claim: older, receipts: Vec::new() });
            }
            let (next, _) = apply(&state, &p, &ctx(n, 100 + n, n), &objects, Some(&env));
            state = next;
            accepted.push((n, claim_id));
            frontier_trace.push(state.safe_frontier().0);
        }

        let (frontier, frontier_block) = state.safe_frontier();
        assert!(frontier > 0, "the frontier never left the start — this is the defect (trace: {frontier_trace:?})");
        assert!(frontier < 60, "the frontier cannot reach the tip: the newest claims are still open");
        // The lag is the liability window, not the chain's age: it must not grow with n. Bind and
        // receipt take one block each here and `window_challenge` is 20 DAA = 20 blocks.
        assert!(60 - frontier <= 25, "the frontier lagged {} blocks — that is a stall, not a window", 60 - frontier);
        // It names the block that carried the matured claim, and that claim really is Final.
        let (word, claim_id) = accepted.iter().find(|(w, _)| *w == frontier).copied().expect("the frontier names an accepting block");
        assert_eq!(frontier_block, block(word));
        assert!(matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        // Monotone the whole way — a frontier that retreats is a reorg with no reorg.
        assert!(frontier_trace.windows(2).all(|w| w[1] >= w[0]), "the frontier retreated: {frontier_trace:?}");
        // And it genuinely moved, repeatedly, rather than jumping once.
        assert!(frontier_trace.windows(2).filter(|w| w[1] > w[0]).count() > 10, "the frontier barely moved: {frontier_trace:?}");
    }

    /// **Audit C2, half two: a fork that does NO work must not outrank one that did.**
    ///
    /// The old rule made this backwards. A fork carrying no attempts at all has an empty
    /// `unresolved` set at every block, so it advanced its frontier once per block for free and
    /// won key 1 against a chain that had matured real work — the exact fabrication that ordering
    /// by frontier before weight exists to refuse, arriving through the frontier itself.
    /// Reproduced by the audit at 60 blocks; asserted here at both ends of the comparator.
    #[test]
    fn a_fork_that_carries_no_work_never_outranks_a_matured_chain() {
        let p = params();
        let (base, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        // Honest: one claim, walked to Final.
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (h1, _) = apply(&base, &p, &ctx(2, 101, 2), &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(21) }];
        let (h2, _) =
            apply(&h1, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (h3, _) = apply(&h2, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        let (honest, _) = apply(&h3, &p, &ctx(5, 124, 5), &[], None);
        assert!(matches!(honest.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));

        // Attacker: sixty blocks from the same base, not one of them carrying an attempt. Free to
        // produce, and under the old rule each one advanced its frontier.
        let mut workless = base.clone();
        for n in 100..160u64 {
            let (next, _) = apply(&workless, &p, &ctx(n, 100 + n, n), &[], None);
            workless = next;
        }

        assert_eq!(workless.safe_frontier().0, 0, "sixty empty blocks matured nothing, so they buy no frontier");
        assert_eq!(workless.safe_weight(), 0);
        assert!(honest.safe_frontier().0 > workless.safe_frontier().0, "the chain that matured work has the deeper frontier");

        let honest_order = honest.candidate_order(block(5));
        let workless_order = workless.candidate_order(block(159));
        assert_eq!(
            compare_palw_candidates_v1(&honest_order, &workless_order),
            std::cmp::Ordering::Greater,
            "a workless fork outranked a matured chain — the C2 attack"
        );
        // And the gate that would actually let it happen says no, on the same comparator.
        assert_eq!(
            crate::palw_fork_authority_v2::decide_deep_reorg_v2(&honest_order, &workless_order),
            crate::palw_fork_authority_v2::PalwDeepReorgV2::Refuse,
            "the deep-reorg gate must refuse a challenger that matured nothing"
        );
    }

    // ---- delta ----

    #[test]
    fn a_delta_replays_to_the_same_child_and_reverts_to_the_same_parent() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, d1) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let (s2, d2) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));

        // Replay: delta over the same parent is the same child — full-struct equality, so the
        // rebuilt indices are covered too, not just the root.
        let replayed_1 = apply_delta_v2(&genesis, &d1, &p).expect("d1 applies to genesis");
        assert_eq!(replayed_1, s1);
        assert_eq!(replayed_1.state_root(), s1.state_root());

        let replayed_2 = apply_delta_v2(&s1, &d2, &p).expect("d2 applies to s1");
        assert_eq!(replayed_2, s2);
        assert_eq!(replayed_2.state_root(), s2.state_root());

        // Revert: the child minus its delta is the parent.
        let reverted = revert_delta_v2(&s2, &d2, &p).expect("d2 reverts from s2");
        assert_eq!(reverted, s1);
        assert_eq!(reverted.state_root(), s1.state_root());

        // A delta refuses any state that is not its parent.
        assert!(matches!(apply_delta_v2(&s2, &d2, &p), Err(PalwStateV2Error::DeltaMismatch(_))));
        assert!(matches!(revert_delta_v2(&s1, &d2, &p), Err(PalwStateV2Error::DeltaMismatch(_))));
    }

    // ---- root coverage ----

    /// Every collection and scalar moves the root. A field the root misses is a field two states
    /// can differ in while claiming to be the same state — the state-level twin of PR-01's
    /// identity rule.
    #[test]
    fn every_state_surface_moves_the_root() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (base, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let root = base.state_root();

        // A different bond, class, target, claim, panel, court, epoch counter, weight, frontier —
        // each built through real transitions so no test reaches into private fields.
        let (with_bond, _) = apply(
            &base,
            &p,
            &ctx(2, 101, 2),
            &[PalwConsensusObjectV2::BondRegistered { bond: bond_key(2), pubkey: vec![8], operator_pubkey: op_key(22), collateral: 1_000, payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11) }],
            None,
        );
        let (with_class, _) = apply(
            &base,
            &p,
            &ctx(2, 101, 2),
            &[PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(2),
                artifact_root: h64(12),
                slash_value_per_pwu: 1,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 500,
                activation_daa: 0,
                admission: None,
            }],
            None,
        );
        let (with_claim, _) = apply(&base, &p, &ctx(2, 101, 2), &[], Some(&attempt(40, 1)));
        let (with_retire, _) =
            apply(&base, &p, &ctx(2, 101, 2), &[PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(1) }], None);
        // The freeze rides an entrant (the floor may not be frozen), which is still a distinct
        // surface: the registration alone and the registration-plus-freeze must differ.
        let (with_freeze, _) = apply(&base, &p, &ctx(2, 101, 2), &[entrant_class(h64(2), 500), freeze(h64(2))], None);
        let (position_only, _) = apply(&base, &p, &ctx(2, 101, 2), &[], None);

        let roots = [
            with_bond.state_root(),
            with_class.state_root(),
            with_claim.state_root(),
            with_retire.state_root(),
            with_freeze.state_root(),
            position_only.state_root(),
        ];
        for (i, candidate) in roots.iter().enumerate() {
            assert_ne!(*candidate, root, "surface {i} did not move the root");
        }
        // And they are pairwise distinct — different changes are different states.
        for i in 0..roots.len() {
            for j in (i + 1)..roots.len() {
                assert_ne!(roots[i], roots[j], "surfaces {i} and {j} collided");
            }
        }
    }

    // ---- carriage ----

    #[test]
    fn carriage_round_trips_bit_for_bit_and_refuses_tampering() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&attempt(40, 1)));

        let carriage = PalwStateCarriageV2::from_state(&s2);
        let root = s2.state_root();
        let restored = carriage.clone().into_state(&p, Some(root)).expect("an honest carriage loads");
        assert_eq!(restored, s2, "the restored state is the state, indices included");
        assert_eq!(restored.state_root(), root);

        // Borsh round-trip of the carriage itself.
        let bytes = borsh::to_vec(&carriage).unwrap();
        let decoded: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded, carriage);
        assert_eq!(decoded.digest(), carriage.digest());

        // Tamper with the exposure summary: the load refuses — a poisoned snapshot never becomes
        // a state.
        let mut poisoned = carriage.clone();
        if let Some(value) = poisoned.reserved_exposure.values_mut().next() {
            *value += 1;
        }
        assert!(matches!(poisoned.into_state(&p, None), Err(PalwStateV2Error::CarriageInconsistent(_))));

        // Tamper with a claim's pwu, keeping the snapshots it carries coherent: self-consistency
        // deliberately cannot catch this (the snapshots ARE the accounting basis) — the committed
        // root is what does, which is why a peer-supplied carriage must never load with `None`.
        let mut poisoned = carriage.clone();
        if let Some(claim) = poisoned.claims.values_mut().next() {
            claim.pwu += 1;
        }
        assert_ne!(poisoned.digest(), carriage.digest(), "the tamper moved the carriage digest");
        assert!(matches!(poisoned.into_state(&p, Some(root)), Err(PalwStateV2Error::CarriageInconsistent(_))));

        // Right data, wrong committed root: refused.
        assert!(matches!(carriage.into_state(&p, Some(h64(1))), Err(PalwStateV2Error::CarriageInconsistent(_))));
    }

    // ---- the differential gate ----

    /// Build one full scenario chain (register → attempt → panel → receipt → court → clear →
    /// final, plus a second claim that void-times-out) through whatever application strategy the
    /// closure implements, and hand back the final root.
    fn scenario_blocks() -> Vec<(u64, u64, u64, Vec<PalwConsensusObjectV2>, Option<PalwAttemptEnvelopeV2>)> {
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let orphan = attempt(7, 2); // never bound; voids by bind timeout at daa > 112
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        vec![
            (1, 100, 1, register_class_and_bond(), None),
            (2, 101, 2, vec![], Some(env)),
            (3, 102, 3, vec![PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], Some(orphan)),
            (4, 103, 4, vec![PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None),
            (
                5,
                104,
                5,
                vec![court_open(claim_id, h64(31), bond_key(1), bond_key(1))],
                None,
            ),
            (
                6,
                150,
                6,
                vec![PalwConsensusObjectV2::CourtClosed { session_id: court_session_of(claim_id, h64(31), bond_key(1), bond_key(1)), verdict: PalwCourtVerdictV2::ChallengerDefeated, proof: crate::palw_court_v2::PalwCourtVerdictProofV2::Arithmetic { refutation: crate::palw_step_refute::tests::skeleton_refutation(), operand_openings: Vec::new(),} }],
                None,
            ),
            (7, 151, 7, vec![], None),
            (8, 152, 8, vec![], None),
            // Cross the global epoch boundary (epoch_length 1000): every differential below —
            // restart, IBD cut points, prior-sink invariance, reorg — now exercises the
            // per-class retarget path too.
            (9, 1_001, 9, vec![], None),
        ]
    }

    /// **The audit register's red test** (P0-4, threat model `palw-rc-threat-model.md`): weigh
    /// candidate C after having lived through branch A, and after having lived through branch B —
    /// the answer must be identical, because the state is a function of C's chain and of nothing
    /// the node did before.
    #[test]
    fn palw_v2_weight_invariant_under_prior_sink() {
        let p = params();
        let genesis_block = block(0);

        // Node 1's history: an A-branch full of unrelated claims, applied FIRST.
        let mut node1 = PalwStateBookV2::new(p.clone());
        node1.insert_genesis(genesis_block);
        node1.apply_block(genesis_block, ctx(21, 100, 1), &register_class_and_bond(), None).unwrap();
        node1.apply_block(block(21), ctx(22, 101, 2), &[], Some(&attempt(1000, 91))).unwrap();
        node1.apply_block(block(22), ctx(23, 102, 3), &[], Some(&attempt(1000, 92))).unwrap();

        // Node 2's history: a different B-branch, applied FIRST.
        let mut node2 = PalwStateBookV2::new(p.clone());
        node2.insert_genesis(genesis_block);
        node2.apply_block(genesis_block, ctx(31, 100, 1), &register_class_and_bond(), None).unwrap();
        node2.apply_block(block(31), ctx(32, 105, 2), &[], Some(&attempt(3, 93))).unwrap();

        // Now both nodes apply the SAME candidate chain C and weigh it.
        let mut roots = Vec::new();
        for node in [&mut node1, &mut node2] {
            let mut parent = genesis_block;
            let mut last_root = None;
            for (b, daa, blue, objects, att) in scenario_blocks() {
                last_root = Some(node.apply_block(parent, ctx(b, daa, blue), &objects, att.as_ref()).unwrap());
                parent = block(b);
            }
            let state = node.state_of(&block(9)).unwrap();
            state.assert_internal_consistency(&p).unwrap();
            state.assert_deadline_consistency(&p).unwrap();
            roots.push((last_root.unwrap(), state.candidate_order(block(9))));
        }
        assert_eq!(roots[0].0, roots[1].0, "same candidate chain, different prior sink ⇒ different root: the P0-4 partition");
        assert_eq!(roots[0].1, roots[1].1, "the candidate order must not see the sink either");
    }

    /// Restart invariance: serializing to carriage and reloading between EVERY block is the same
    /// chain of states as never restarting.
    #[test]
    fn a_restart_between_every_block_changes_nothing() {
        let p = params();
        let mut straight = PalwChainStateV2::genesis();
        let mut restarted = PalwChainStateV2::genesis();
        for (b, daa, blue, objects, att) in scenario_blocks() {
            let (next, _) = apply(&straight, &p, &ctx(b, daa, blue), &objects, att.as_ref());
            straight = next;

            let (next, _) = apply(&restarted, &p, &ctx(b, daa, blue), &objects, att.as_ref());
            let carried = PalwStateCarriageV2::from_state(&next);
            restarted = carried.into_state(&p, Some(next.state_root())).expect("the restart snapshot loads");
        }
        assert_eq!(straight, restarted);
        assert_eq!(straight.state_root(), restarted.state_root());
    }

    /// IBD-start-point invariance: a node that starts from a mid-chain carriage finishes with the
    /// same root as a node that walked from genesis.
    #[test]
    fn continuing_from_a_mid_chain_carriage_equals_from_genesis() {
        let p = params();
        let blocks = scenario_blocks();
        let mut from_genesis = PalwChainStateV2::genesis();
        for (b, daa, blue, objects, att) in &blocks {
            let (next, _) = apply(&from_genesis, &p, &ctx(*b, *daa, *blue), objects, att.as_ref());
            from_genesis = next;
        }

        for cut in 1..blocks.len() {
            let mut prefix = PalwChainStateV2::genesis();
            for (b, daa, blue, objects, att) in &blocks[..cut] {
                let (next, _) = apply(&prefix, &p, &ctx(*b, *daa, *blue), objects, att.as_ref());
                prefix = next;
            }
            let mut joined =
                PalwStateCarriageV2::from_state(&prefix).into_state(&p, Some(prefix.state_root())).expect("mid-chain carriage loads");
            for (b, daa, blue, objects, att) in &blocks[cut..] {
                let (next, _) = apply(&joined, &p, &ctx(*b, *daa, *blue), objects, att.as_ref());
                joined = next;
            }
            assert_eq!(joined.state_root(), from_genesis.state_root(), "IBD start at block {cut} diverged");
        }
    }

    /// Reorg invariance: rewinding a branch through its recorded deltas and applying another
    /// lands bit-for-bit where a fresh walk of the other branch lands.
    #[test]
    fn a_reorg_through_deltas_equals_a_fresh_walk() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (base, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        // Branch A: two claims.
        let (a1, da1) = apply(&base, &p, &ctx(21, 101, 2), &[], Some(&attempt(1000, 91)));
        let (a2, da2) = apply(&a1, &p, &ctx(22, 102, 3), &[], Some(&attempt(1000, 92)));

        // Rewind A, then walk branch B.
        let back1 = revert_delta_v2(&a2, &da2, &p).unwrap();
        let back0 = revert_delta_v2(&back1, &da1, &p).unwrap();
        assert_eq!(back0, base, "rewinding the branch restored the fork point exactly");

        let (b1_via_reorg, _) = apply(&back0, &p, &ctx(31, 101, 2), &[], Some(&attempt(3, 93)));
        let (b1_fresh, _) = apply(&base, &p, &ctx(31, 101, 2), &[], Some(&attempt(3, 93)));
        assert_eq!(b1_via_reorg, b1_fresh);
        assert_eq!(b1_via_reorg.state_root(), b1_fresh.state_root());
    }

    // ---- per-class retarget (PR-09) ----

    /// The V1 rule through the V2 driver: at 1000‰ with every block in the class, observed
    /// equals expected and the target does not move — the one-class no-op that catches any
    /// mutation toward a cadence-based expectation. And with a 500‰ share producing ALL blocks,
    /// the class out-produced its expectation and its target HARDENS, while a frozen co-class's
    /// target does not move at all (it freezes with the class), and a boundary nobody produced
    /// across measures nothing.
    #[test]
    fn crossing_an_epoch_boundary_retargets_by_share_and_skips_the_frozen() {
        let boot = u128::MAX / 2;

        // Case 1: single class at 1000‰ — no-op.
        let p_full = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p_full, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let (s2, _) = apply(&s1, &p_full, &ctx(2, 101, 2), &[], Some(&attempt(40, 1)));
        let (s3, _) = apply(&s2, &p_full, &ctx(3, 1_001, 3), &[], None);
        assert_eq!(s3.class_target(&h64(1)).unwrap().target, boot, "1000‰ with every block: observed == expected, no move");

        // Case 2: class 1 at 500‰ produces everything; class 2 (500‰) is frozen before the
        // boundary. Class 1 hardens; class 2 does not move. The 500/500 table arrives the
        // ADR-0045 way: class 2's grant is donated out of class 1's whole.
        let p_half = params();
        let mut objects = register_class_and_bond();
        objects.push(PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            artifact_root: h64(12),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: boot,
            share_permille: 500,
            activation_daa: 0,
            admission: None,
        });
        objects.push(freeze(h64(2)));
        let (s1, _) = apply(&genesis, &p_half, &ctx(1, 100, 1), &objects, None);
        // ADR-0045 Decision 3: freeze moves NO share — the table is the allocation of record,
        // and absence is the census's business, not a mutation's.
        assert_eq!(s1.class_share_permille(&h64(1)), Some(500), "the working class's share is untouched by a sibling's freeze");
        assert_eq!(s1.class_share_permille(&h64(2)), Some(500), "the frozen class keeps its permille for its unfreeze");
        let (s2, _) = apply(&s1, &p_half, &ctx(2, 101, 2), &[], Some(&attempt(40, 1)));
        let (s3, _) = apply(&s2, &p_half, &ctx(3, 102, 3), &[], Some(&attempt(40, 2)));
        let (s4, _) = apply(&s3, &p_half, &ctx(4, 1_001, 4), &[], None);
        // **Audit H1.** This used to assert that class 1 HARDENS, and that assertion was the bug
        // written down: class 2's 500‰ is frozen, so it cannot produce, so demanding half the span
        // of class 1 makes the only class still working a permanent over-producer whose target is
        // divided at every boundary — with no floor, until `ZeroPreviousTarget` rejects every
        // block on the chain forever. The expectation is now normalized over the ELIGIBLE
        // permille, so a frozen class's share is redistributed rather than silently demanded:
        // class 1 is expected to produce the whole span, it did, and nothing moves.
        assert_eq!(
            s4.class_target(&h64(1)).unwrap().target,
            boot,
            "the sole eligible class is expected to produce the whole span — producing it is not over-producing"
        );
        assert_eq!(s4.class_target(&h64(2)).unwrap().target, boot, "a frozen class's target freezes with it");

        // Case 3: a boundary crossed with nothing produced (the counters are stale after case
        // 2's crossing) measures nothing and moves nothing.
        let hardened = s4.class_target(&h64(1)).unwrap().target;
        let (s5, _) = apply(&s4, &p_half, &ctx(5, 2_001, 5), &[], None);
        assert_eq!(s5.class_target(&h64(1)).unwrap().target, hardened, "an empty epoch measures nothing");
    }

    /// **Audit H1: an idle co-class must not strangle the class that is still working.**
    ///
    /// The failure the normalization prevents is not a one-epoch inaccuracy, it is a ratchet: the
    /// same over-production verdict every boundary, in the same direction, with `max_factor`
    /// bounding each step but nothing bounding the walk. Twelve boundaries at `max_factor = 4` is
    /// 4^12 ≈ 1.7e7 harder — and it does not stop there; it stops at zero, where
    /// `ZeroPreviousTarget` makes every subsequent block invalid on a chain no node can rejoin.
    ///
    /// Here class 2 holds 400‰ and never produces a block (registered, Active, simply idle — it
    /// does not even need to be frozen). Class 1 produces every block of every span.
    #[test]
    fn an_idle_co_class_does_not_ratchet_the_working_class_toward_zero() {
        let boot = u128::MAX / 2;
        let p = params();

        // 600/400 by donation: class 2's 400‰ grant leaves class 1 holding 600‰.
        let mut objects = register_class_and_bond();
        objects.push(PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            artifact_root: h64(12),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: boot,
            share_permille: 400,
            activation_daa: 0,
            admission: None,
        });
        let (mut state, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None);
        assert_eq!(state.class_share_permille(&h64(1)), Some(600), "the grant is funded by donation");
        assert_eq!(state.class_share_permille(&h64(2)), Some(400));

        // Twelve epoch boundaries, class 1 producing throughout.
        let mut nonce = 0u64;
        let mut targets = Vec::new();
        for epoch in 1..=12u64 {
            for step in 0..3u64 {
                nonce += 1;
                let daa = epoch * 1000 + step + 1;
                let (next, _) = apply(&state, &p, &ctx(nonce + 10, daa, nonce + 10), &[], Some(&attempt(4, nonce)));
                state = next;
            }
            targets.push(state.class_target(&h64(1)).unwrap().target);
        }

        assert_eq!(targets.last().copied().unwrap(), boot, "the working class was not ratcheted: {targets:?}");
        assert!(targets.iter().all(|t| *t == boot), "the target moved on some boundary: {targets:?}");
        // The idle class keeps its own target — it is not punished for the span it sat out either.
        assert_eq!(state.class_target(&h64(2)).unwrap().target, boot, "an idle class is not retargeted on production it never made");
    }

    /// The other half of H1's fix: **real competition must still retarget.** A normalization that
    /// silenced the mechanism would be a worse bug than the ratchet it replaced, so this pins that
    /// when both classes produce, both keep their table shares and the feedback bites.
    #[test]
    fn two_producing_classes_keep_their_table_shares_and_still_retarget() {
        let boot = u128::MAX / 2;
        let p = params();

        let mut objects = register_class_and_bond();
        objects.push(PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            artifact_root: h64(12),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: boot,
            share_permille: 500,
            activation_daa: 0,
            admission: None,
        });
        objects.push(PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(2),
            pubkey: vec![8; 4],
            operator_pubkey: op_key(22),
            collateral: 1_000,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
        });
        let (s1, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None);

        // Class 1 takes three blocks of the span, class 2 takes one: 750/250 against a 500/500
        // table, so class 1 over-produced and class 2 under-produced — both by real competition.
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&attempt(4, 1)));
        let (s3, _) = apply(&s2, &p, &ctx(3, 102, 3), &[], Some(&attempt(4, 2)));
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[], Some(&attempt(4, 3)));
        let (s5, _) = apply(&s4, &p, &ctx(5, 104, 5), &[], Some(&attempt_for_class(4, 4, h64(2), bond_key(2), vec![8; 4], h64(22), h64(12))));
        let (s6, _) = apply(&s5, &p, &ctx(6, 1_001, 6), &[], None);

        assert!(s6.class_target(&h64(1)).unwrap().target < boot, "the over-producing class hardens");
        assert!(s6.class_target(&h64(2)).unwrap().target > boot, "the under-producing class eases");
    }

    /// **Audit C5: an operator identity names a key, and costs collateral.**
    ///
    /// Decision 7 rests panel dedup on `operator_id` — "splitting collateral across bonds does
    /// not manufacture extra panel seats". It was a self-declared label, so the claim was false
    /// for free: one registrant writes N different ids and takes N seats. Two things changed.
    /// The id is derived from a KEY, so two bonds share an operator exactly when they name the
    /// same key and cannot pretend otherwise; and `min_collateral_sompi` — which the atomic
    /// bundle already carried and nobody read — is enforced where registrations are applied, so
    /// each extra identity costs a real bond floor.
    #[test]
    fn an_operator_identity_is_a_key_and_a_bond_floor() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();

        // Same key, two bonds: ONE operator. This is the case dedup exists for, and it is now a
        // property of the key rather than of what the registrant chose to write down.
        let shared = vec![
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(1),
                pubkey: vec![7; 4],
                operator_pubkey: op_key(0xAA),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(2),
                pubkey: vec![8; 4],
                operator_pubkey: op_key(0xAA),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
        ];
        let (state, _) = apply(&genesis, &p, &ctx(1, 100, 1), &shared, None);
        assert_eq!(
            state.bond(&bond_key(1)).unwrap().operator_id,
            state.bond(&bond_key(2)).unwrap().operator_id,
            "two bonds under one key are one operator"
        );
        assert_eq!(state.bond(&bond_key(1)).unwrap().operator_id, op_id(0xAA), "and the id is the key's, not a label");

        // Different keys are different operators — Sybil is still possible, as it must be; what
        // it now costs is one full bond floor per identity.
        let (state2, _) = apply(
            &state,
            &p,
            &ctx(2, 101, 2),
            &[PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(3),
                pubkey: vec![9; 4],
                operator_pubkey: op_key(0xBB),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            }],
            None,
        );
        assert_ne!(state2.bond(&bond_key(3)).unwrap().operator_id, op_id(0xAA));

        // A bond below the floor is refused, so collateral cannot be split into dust identities.
        let dust = apply_palw_transition_v2(
            &state2,
            &p,
            &ctx(3, 102, 3),
            &[PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(4),
                pubkey: vec![1; 4],
                operator_pubkey: op_key(0xCC),
                collateral: p.min_collateral_sompi() - 1,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            }],
            None,
        );
        assert!(matches!(dust, Err(PalwStateV2Error::CollateralBelowMinimum { .. })), "got {dust:?}");

        // Exactly the floor is fine — the rule is a floor, not a margin.
        let (_, _) = apply(
            &state2,
            &p,
            &ctx(3, 102, 3),
            &[PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(4),
                pubkey: vec![1; 4],
                operator_pubkey: op_key(0xCC),
                collateral: p.min_collateral_sompi(),
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            }],
            None,
        );

        // An operator identity has to name SOMETHING.
        let empty = apply_palw_transition_v2(
            &state2,
            &p,
            &ctx(3, 102, 3),
            &[PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(5),
                pubkey: vec![1; 4],
                operator_pubkey: Vec::new(),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            }],
            None,
        );
        assert!(matches!(empty, Err(PalwStateV2Error::EmptyOperatorKey(_))), "got {empty:?}");

        // The derivation is injective in the key and domain-separated from every other Hash64
        // this ruleset mints — an operator id must not collide with a class id or a claim id.
        assert_ne!(op_id(1), op_id(2));
        assert_ne!(op_id(1), h64(1), "the id is not the raw label it was derived from");
    }

    /// **Audit C5: a conviction has to cost something.**
    ///
    /// There was no slash primitive anywhere in the tree. `void_claim` released the reservation
    /// and wrote a phase, so every penalty ADR-0042 describes — the court's `ExecutorGuilty`,
    /// Decision 7's producer default — moved no value in either direction. A ruleset whose every
    /// punishment is a state label has no punishments, and every game-theoretic argument built on
    /// top of it is about a different system.
    ///
    /// The amount is `claim.reserved` — `pwu × slash_value_per_pwu`, the exact number the
    /// exposure ceiling exists to bound — so the penalty is the stake the claim itself named,
    /// with no new parameter to pick and no constant to get wrong.
    #[test]
    fn a_convicted_claim_costs_its_bond_the_stake_it_named() {
        let p = params();
        let (base, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let collateral = base.bond(&bond_key(1)).unwrap().collateral;

        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&base, &p, &ctx(2, 101, 2), &[], Some(&env));
        let reserved = s2.claim(&claim_id).unwrap().reserved;
        assert_eq!(reserved, 40 * 5, "pwu x slash_value_per_pwu");
        assert_eq!(s2.bond(&bond_key(1)).unwrap().collateral, collateral, "reserving is not taking");
        assert_eq!(s2.bond(&bond_key(1)).unwrap().slashed, 0);

        // Court conviction: the stake is taken, and recorded as taken.
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s3, _) = apply(
            &s2,
            &p,
            &ctx(3, 102, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }],
            None,
        );
        let (s4, _) = apply(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }],
            None,
        );
        let sid = court_session_of(claim_id, h64(31), bond_key(1), bond_key(1));
        let (s5, _) = apply(
            &s4,
            &p,
            &ctx(5, 104, 5),
            &[court_open(claim_id, h64(31), bond_key(1), bond_key(1))],
            None,
        );
        let (s6, _) = apply(
            &s5,
            &p,
            &ctx(6, 105, 6),
            &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict: PalwCourtVerdictV2::ExecutorGuilty, proof: crate::palw_court_v2::PalwCourtVerdictProofV2::Arithmetic { refutation: crate::palw_step_refute::tests::skeleton_refutation(), operand_openings: Vec::new(),} }],
            None,
        );
        let bond = s6.bond(&bond_key(1)).unwrap();
        assert_eq!(bond.collateral, collateral - reserved as u64, "a guilty verdict debits the stake");
        assert_eq!(bond.slashed, reserved as u64, "and records it, so the loss is auditable from the state alone");
        assert_eq!(s6.reserved_exposure(&bond_key(1)), 0, "the reservation is released as well as taken — exactly once");

        // A bind timeout is NOT the producer's fault: nobody bound a panel, and binding is
        // permissionless. It voids without taking.
        let (b2, _) = apply(&base, &p, &ctx(20, 200, 20), &[], Some(&attempt(40, 2)));
        let (b3, _) = apply(&b2, &p, &ctx(21, 400, 21), &[], None);
        assert!(matches!(
            b3.claim(&attempt_id_v2(&attempt(40, 2).attempt)).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }
        ));
        assert_eq!(b3.bond(&bond_key(1)).unwrap().slashed, 0, "a timeout nobody could blame the producer for takes nothing");
    }

    /// **Audit C5: reporting against your own panel's conclusion is not free.**
    ///
    /// An `Unavailable` verdict voids an honest producer's claim, and it cost its signers
    /// nothing — so a minority could try it on every claim and lose only the gas. The majority
    /// invariant (`2·quorum > seat_count`, enforced in `PalwPanelParamsV2`) is what turns a
    /// dissent into a CONTRADICTION: both verdicts cannot reach quorum, so the record refutes
    /// exactly one side. That side pays what it tried to take.
    #[test]
    fn a_seat_that_reports_against_the_quorum_pays_what_it_tried_to_take() {
        let p = params();
        let mut objects = register_class_and_bond();
        objects.push(PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(9),
            pubkey: vec![9; 4],
            operator_pubkey: op_key(0x99),
            collateral: 1_000,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
        });
        let (base, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None);

        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&base, &p, &ctx(2, 101, 2), &[], Some(&env));
        let reserved = s2.claim(&claim_id).unwrap().reserved as u64;
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(9), operator_id: h64(0x99) }];
        let (s3, _) = apply(
            &s2,
            &p,
            &ctx(3, 102, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }],
            None,
        );

        // The quorum said the data WAS served; seat 9 said it was withheld.
        let dissent = vec![PalwSeatVerdictV2 { seat_bond: bond_key(9), served: false }];
        let (licensed, _) = apply(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: dissent }],
            None,
        );
        assert_eq!(licensed.bond(&bond_key(9)).unwrap().slashed, reserved, "the refuted seat pays the stake it attacked");
        assert_eq!(licensed.bond(&bond_key(1)).unwrap().slashed, 0, "the producer, vindicated, pays nothing");

        // Symmetric: on a producer default, a seat that insisted the data was served is the
        // contradicted one — and the producer is charged too, because Decision 7's default IS
        // the producer's fault by construction.
        let agreeing = vec![PalwSeatVerdictV2 { seat_bond: bond_key(9), served: true }];
        let (defaulted, _) = apply(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id, receipts: agreeing }],
            None,
        );
        assert_eq!(defaulted.bond(&bond_key(9)).unwrap().slashed, reserved, "the refuted seat pays in this direction too");
        assert_eq!(defaulted.bond(&bond_key(1)).unwrap().slashed, reserved, "and the producer pays for withholding");

        // A seat that voted WITH the quorum is untouched, in both directions.
        let concurring = vec![PalwSeatVerdictV2 { seat_bond: bond_key(9), served: true }];
        let (clean, _) = apply(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: concurring }],
            None,
        );
        assert_eq!(clean.bond(&bond_key(9)).unwrap().slashed, 0, "agreeing with the record costs nothing");
    }

    /// **Gate item 12: the emergency off-switch is objective, and nobody can pull it by hand.**
    ///
    /// The variant this replaces was `ClassFrozen { class_id }` — a bare instruction, checked
    /// only for "the class exists and is Active". On a running network that is a halt button with
    /// no lock on it: freeze the liveness floor and no class can produce, which is the whole
    /// chain, at the cost of one block's object list. An off-switch anyone may pull is not a
    /// safety mechanism; it is the attack it was built to survive.
    ///
    /// The freeze needs the class's own determinism claim refuted BY ITS OWN PARTICIPANTS — two
    /// attestations on one job context that disagree about what that job produced. Nobody decides
    /// that, which is why it needs no governance step and why it cannot be manufactured against a
    /// class that is behaving.
    #[test]
    fn a_class_freezes_only_on_a_contradiction_about_itself() {
        let p = params();
        let mut objects = register_class_and_bond();
        objects.push(PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            artifact_root: h64(12),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 100,
            activation_daa: 0,
            admission: None,
        });
        let (base, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None);

        // (a) Evidence about ANOTHER class cannot freeze this one — manufacture a real
        //     contradiction inside a disposable class, then quote it at the class you want
        //     stopped. Aimed at the entrant here; aimed at BASE-0 it does not even reach this
        //     rule, because (f) refuses a floor freeze whatever the evidence says.
        let cross = PalwConsensusObjectV2::ClassFrozen { class_id: h64(2), certificate: contradiction(h64(3)) };
        let err = apply_palw_transition_v2(&base, &p, &ctx(2, 101, 2), &[cross], None);
        assert!(
            matches!(err, Err(PalwStateV2Error::ContradictionNamesAnotherClass { frozen, evidenced }) if frozen == h64(2) && evidenced == h64(3)),
            "got {err:?}"
        );

        // (b) Two attestations that AGREE are a class working correctly. Freezing on them would
        //     let anyone halt a class by quoting it agreeing with itself.
        let mut agreeing = contradiction(h64(2));
        agreeing.attestation_b = agreeing.attestation_a.clone();
        let err = apply_palw_transition_v2(
            &base,
            &p,
            &ctx(2, 101, 2),
            &[PalwConsensusObjectV2::ClassFrozen { class_id: h64(2), certificate: agreeing }],
            None,
        );
        assert!(matches!(err, Err(PalwStateV2Error::ContradictionNotProven(_))), "got {err:?}");

        // (c) An attestation that binds a different job context is a second fact, not a
        //     contradiction.
        let mut foreign_job = contradiction(h64(2));
        foreign_job.attestation_b.job_context_hash = h64(0xDEAD);
        let err = apply_palw_transition_v2(
            &base,
            &p,
            &ctx(2, 101, 2),
            &[PalwConsensusObjectV2::ClassFrozen { class_id: h64(2), certificate: foreign_job }],
            None,
        );
        assert!(matches!(err, Err(PalwStateV2Error::ContradictionNotProven(_))), "got {err:?}");

        // (d) The real thing freezes, and the freeze is what every consumer already reads: the
        //     class stops producing, stops being retargeted, and stops admitting attempts. An
        //     entrant (already registered above), because (f) refuses freezing the floor at all.
        let (frozen, _) = apply(&base, &p, &ctx(2, 101, 2), &[freeze(h64(2))], None);
        assert!(matches!(frozen.class(&h64(2)).unwrap().status, PalwClassStatusV2::Frozen { since_daa: 101 }));
        let entrant_attempt = attempt_for_class(40, 1, h64(2), bond_key(1), vec![7; 4], op_id(21), h64(12));
        let refused = apply_palw_transition_v2(&frozen, &p, &ctx(3, 102, 3), &[], Some(&entrant_attempt));
        assert!(matches!(refused, Err(PalwStateV2Error::FrozenClass(id)) if id == h64(2)), "got {refused:?}");

        // (e) One-way. There is no edge back to Active anywhere in this machine — a chain-level
        //     unfreeze would turn an objective, permanent consequence into a temporary one, which
        //     is precisely what an attacker holding the emit path would want. Freezing again is
        //     refused rather than being a no-op, so a second certificate cannot quietly restamp
        //     `since_daa` and move the record's own history.
        let again = apply_palw_transition_v2(&frozen, &p, &ctx(3, 102, 3), &[freeze(h64(2))], None);
        assert!(matches!(again, Err(PalwStateV2Error::FrozenClass(id)) if id == h64(2)), "got {again:?}");
        // The share stays where it was: ADR-0045 Decision 3 keeps the table the allocation of
        // record, and absence is the census's business.
        assert_eq!(frozen.class_share_permille(&h64(2)), base.class_share_permille(&h64(2)));

        // (f) **The liveness floor may not be frozen at all (ADR-0039 W6′).** A `ClassFrozen`
        //     naming BASE-0 was accepted, and on a `ConsensusV2` network the consequence is
        //     terminal: the attempt lane is the only block type, admission refuses a frozen
        //     class, and the floor is the class every operator can run — so one object ends the
        //     chain, with no path back, because the object that would undo it needs a block.
        let floor = apply_palw_transition_v2(&base, &p, &ctx(2, 101, 2), &[freeze(h64(1))], None);
        assert!(matches!(floor, Err(PalwStateV2Error::BaseClassMayNotFreeze(id)) if id == h64(1)), "got {floor:?}");
    }

    /// **A free-prompt claim the court could never bind a refutation to is refused (audit C3,
    /// free-prompt lane).**
    ///
    /// The integration found the attempt lane's C3 fix — the claim carries the executor's
    /// `committed_execution_root`, and `adjudicate_court_close_v2` pins a refutation's binding to
    /// it — with no counterpart on the free-prompt lane. A free-prompt claim built without one
    /// had to borrow some other field, and the nearest (`schedule_root`) is a different quantity
    /// no honest binding can recompute to, so EVERY dispute about a free-prompt claim would have
    /// died at `ExecutionRootMismatch`. Fail-closed, and useless: a producer no court can convict
    /// is a producer that can commit arithmetic fraud with impunity.
    ///
    /// The commitment carries the real root now, and a null one is refused at admission rather
    /// than admitted and discovered at a dispute nobody could win. That refusal currently rejects
    /// every commitment the free-prompt worker can build — its v3 execution path captures no legs
    /// — which is the honest state of the lane stated where it can be acted on.
    #[test]
    fn a_free_prompt_claim_without_an_execution_root_is_refused() {
        let p = params();
        let (base, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        let mut null_root = fp_commit(0xFC, 60, 3);
        if let PalwConsensusObjectV2::FreePromptCommitted { execution_root, .. } = &mut null_root {
            *execution_root = Hash64::default();
        }
        let refused = apply_palw_transition_v2(&base, &p, &ctx(2, 101, 2), &[null_root], None);
        assert!(
            matches!(refused, Err(PalwStateV2Error::UnadjudicableCommitment(id)) if id == h64(0xFC)),
            "got {refused:?}"
        );

        // The same claim with a real root is admitted — so the refusal is about the root and not
        // about free-prompt claims in general.
        let (ok, _) = apply(&base, &p, &ctx(2, 101, 2), &[fp_commit(0xFC, 60, 3)], None);
        assert!(matches!(ok.claim(&h64(0xFC)).unwrap().phase, PalwClaimPhaseV2::Provisional));
        assert_ne!(ok.claim(&h64(0xFC)).unwrap().execution_root, Hash64::default(), "the record carries what the court will bind");
    }

    /// **Audit C5's free re-roll, the free-prompt half — the cost the block-based costs cannot
    /// provide.**
    ///
    /// The attempt lane's three costs all rest on one fact: a claim is a BLOCK. A free-prompt
    /// commitment rides a transaction, so on that lane the merge that brought the two lanes
    /// together left every one of them at zero, and this test was written by MEASURING that
    /// first: after a `BindTimeout` the reservation came back in full, no counter moved, no bond
    /// was debited, and the next commitment was accepted in the very next block. A producer who
    /// disliked its drawn panel could redraw for a transaction fee, indefinitely.
    ///
    /// The hold does not confiscate — declining to bind is not an offence — it DELAYS. What that
    /// buys is the denominator: N concurrent redraws need N × the reservation, so the redraw rate
    /// is bounded by collateral, which is the same currency Decision 7's Sybil bound already
    /// speaks. This pins both halves: the hold binds while it lasts, and every sompi comes back
    /// when it elapses.
    #[test]
    fn an_abandoned_free_prompt_claim_holds_its_collateral() {
        // 300 collateral-units of exposure per claim (pwu 60 x slash_value 5), a ceiling of
        // 500%o of 100_000, and a hold of 50 DAA.
        let p = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 50).unwrap();
        let (base, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let (s2, _) = apply(&base, &p, &ctx(2, 101, 2), &[fp_commit(0xFC, 60, 3)], None);
        let reserved = s2.reserved_exposure(&bond_key(1));
        assert_eq!(reserved, 300, "the commitment reserves against its bond");

        // The bind window lapses at 111; a block past it voids the claim.
        let (voided, _) = apply(&s2, &p, &ctx(3, 200, 3), &[], None);
        assert!(matches!(
            voided.claim(&h64(0xFC)).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }
        ));
        // The claim is terminal: no weight, live or safe, from the moment it is voided.
        assert_eq!((voided.safe_weight(), voided.bounded_immature()), (0, 0), "an abandoned claim weighs nothing");
        // …and yet the collateral is still committed. THIS is the whole fix.
        assert_eq!(
            voided.reserved_exposure(&bond_key(1)),
            reserved,
            "the abandoned claim keeps its reservation — a redraw costs collateral, not a fee"
        );
        assert_eq!(voided.bond(&bond_key(1)).unwrap().slashed, 0, "a hold is a delay, never a confiscation");

        // A redraw inside the hold competes with the held reservation for the same ceiling. Two
        // more claims fit (3 x 300 = 900 <= 500%o x 100_000 / 1000 ... the ceiling is generous
        // here), so the assertion that matters is the accumulation itself: each concurrent
        // attempt adds its own reservation on top of the held one.
        let (r1, _) = apply(&voided, &p, &ctx(4, 201, 4), &[fp_commit(0xFD, 60, 3)], None);
        assert_eq!(r1.reserved_exposure(&bond_key(1)), reserved * 2, "the redraw stacks on the hold, it does not recycle it");
        let (r2, _) = apply(&r1, &p, &ctx(5, 202, 5), &[fp_commit(0xFE, 60, 3)], None);
        assert_eq!(r2.reserved_exposure(&bond_key(1)), reserved * 3);

        // The hold elapses at 200 + 50 = 250, and the sweep gives every sompi back. The two live
        // claims (0xFD, 0xFE) keep theirs — the release is per-claim, not a reset.
        let (released, _) = apply(&r2, &p, &ctx(6, 260, 6), &[], None);
        assert_eq!(
            released.reserved_exposure(&bond_key(1)),
            reserved * 2,
            "the hold expires and returns exactly what it held, and nothing else"
        );
        // The abandoned claim is still terminal and still abandoned — releasing collateral is not
        // resurrection.
        assert!(matches!(
            released.claim(&h64(0xFC)).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }
        ));

        // And the pre-FP configuration is untouched: at hold = 0 the reservation comes straight
        // back, which is what every attempt-only fixture in this module runs at.
        let no_hold = params();
        assert_eq!(no_hold.fp_abandon_hold_daa(), 0);
        let (b0, _) = apply(&PalwChainStateV2::genesis(), &no_hold, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let (c0, _) = apply(&b0, &no_hold, &ctx(2, 101, 2), &[fp_commit(0xFC, 60, 3)], None);
        let (v0, _) = apply(&c0, &no_hold, &ctx(3, 200, 3), &[], None);
        assert_eq!(v0.reserved_exposure(&bond_key(1)), 0, "hold = 0 releases at the void, as before");
    }

    /// **Audit C5's "free panel re-roll", measured rather than assumed.**
    ///
    /// The finding was that a producer who dislikes its drawn panel abandons the claim at
    /// `BindTimeout` and re-attempts, "for free". Three costs say otherwise, and this pins all
    /// three so nobody has to re-derive them from the finding's wording:
    ///
    /// 1. **A claim is one block.** `accepted_block` is the carrying block and a duplicate
    ///    `attempt_id` is refused, so a re-roll is another block — another solved PoW. It is the
    ///    most expensive thing on the network.
    /// 2. **The abandoned block earns nothing.** The void takes its pwu out of both weights
    ///    permanently, and every void reason forfeits the reward escrow
    ///    (`palw_reward_v2::palw_reward_status_v2`).
    /// 3. **The class's epoch budget is spent anyway.** Production is counted at acceptance and
    ///    a void never gives it back, so re-rolling burns the class's own admission headroom.
    ///
    /// And there is nothing to shop for at mining time: the panel needs an anchor that does not
    /// exist yet (`anchor_delay > 0` is enforced precisely so "the attempt's own block cannot
    /// seed its panel"). What remains is a CHOICE WITH A PRICE — decline to bind and forfeit a
    /// block — not a free re-roll. What is genuinely open, and stated in the register rather than
    /// papered over here: binding is permissionless but nobody is paid to bind someone else's
    /// claim, so in practice the producer decides.
    #[test]
    fn abandoning_a_panel_costs_a_block_its_reward_and_its_epoch_budget() {
        let p = params();
        let (base, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply(&base, &p, &ctx(2, 101, 2), &[], Some(&env));
        let produced_after_first = s2.epoch_counter(&h64(1)).unwrap().produced_blocks;
        assert_eq!(produced_after_first, 1);

        // (1) The same attempt cannot be re-submitted: one claim, one block.
        let again = apply_palw_transition_v2(&s2, &p, &ctx(3, 102, 3), &[], Some(&env));
        assert!(matches!(again, Err(PalwStateV2Error::DuplicateClaim(_))), "got {again:?}");

        // Let the bind window lapse — the producer declining to bind a panel it dislikes.
        let (voided, _) = apply(&s2, &p, &ctx(3, 200, 3), &[], None);
        assert!(matches!(
            voided.claim(&claim_id).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }
        ));

        // (2) The block earns nothing, in either weight, and its reward is forfeit.
        assert_eq!(voided.safe_weight(), 0);
        assert_eq!(voided.bounded_immature(), 0);
        assert_eq!(
            crate::palw_reward_v2::palw_reward_status_v2(&voided.claim(&claim_id).unwrap().phase),
            crate::palw_reward_v2::PalwRewardStatusV2::Forfeited,
            "an abandoned claim's carve never enters circulation"
        );

        // (3) The epoch budget was spent at acceptance and the void does not refund it, so the
        // re-roll competes with the producer's own future claims in the same epoch.
        assert_eq!(
            voided.epoch_counter(&h64(1)).unwrap().produced_blocks,
            produced_after_first,
            "voiding releases exposure, never production"
        );

        // (4) A claim cannot be re-bound: one panel per claim, so the only re-roll is a new block.
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (bound, _) = apply(
            &s2,
            &p,
            &ctx(3, 102, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats: seats.clone() }],
            None,
        );
        let rebind = apply_palw_transition_v2(
            &bound,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(78), seats }],
            None,
        );
        assert!(matches!(rebind, Err(PalwStateV2Error::WrongPhase { .. })), "got {rebind:?}");
    }

    #[test]
    fn class_daa_params_refuse_broken_tables() {
        // ADR-0045 Decision 3: the table's validity is the TRANSITION's job now. The grant
        // refusals stand where the old params constructor's checks stood.
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let register = |class_id: u64, share: u16| PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(class_id),
            artifact_root: h64(19),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(10),
            initial_target: u128::MAX / 2,
            share_permille: share,
            activation_daa: 0,
            admission: None,
        };

        // The first class must be the base, at the whole 1000‰.
        let err = apply_palw_transition_v2(&genesis, &p, &ctx(1, 100, 1), &[register(9, 1000)], None);
        assert!(matches!(err, Err(PalwStateV2Error::FirstClassMustBeTheBase { .. })), "got {err:?}");
        let err = apply_palw_transition_v2(&genesis, &p, &ctx(1, 100, 1), &[register(1, 900)], None);
        assert!(matches!(err, Err(PalwStateV2Error::FirstShareMustBeWhole { got: 900 })), "got {err:?}");

        let (funded, _) = apply_palw_transition_v2(&genesis, &p, &ctx(1, 100, 1), &[register(1, 1000)], None).unwrap();
        assert_eq!(funded.class_share_permille(&h64(1)), Some(1000));

        // A later grant outside the denominator, or below the grant floor, refuses.
        let err = apply_palw_transition_v2(&funded, &p, &ctx(2, 101, 2), &[register(2, 1001)], None);
        assert!(matches!(err, Err(PalwStateV2Error::ShareOutOfRange { got: 1001 })), "got {err:?}");
        let err = apply_palw_transition_v2(&funded, &p, &ctx(2, 101, 2), &[register(2, 0)], None);
        assert!(matches!(err, Err(PalwStateV2Error::ShareBelowGrantFloor { .. })), "got {err:?}");

        // A grant that would starve every donor to zero refuses too — the base class may never
        // be pushed below the floor, and 1000‰ for an entrant leaves it exactly nothing.
        let err = apply_palw_transition_v2(&funded, &p, &ctx(2, 101, 2), &[register(2, 1000)], None);
        assert!(matches!(err, Err(PalwStateV2Error::DonationBreaksGrantFloor { .. })), "got {err:?}");

        // Donation conserves the denominator with largest-remainder exactness: 333‰ granted out
        // of 1000 leaves the base 667‰ (666 by truncation, +1 by remainder), and a third class
        // splits the residue deterministically.
        let (two, _) = apply_palw_transition_v2(&funded, &p, &ctx(2, 101, 2), &[register(2, 333)], None).unwrap();
        assert_eq!(two.class_share_permille(&h64(1)), Some(667));
        assert_eq!(two.class_share_permille(&h64(2)), Some(333));
        let (three, _) = apply_palw_transition_v2(&two, &p, &ctx(3, 102, 3), &[register(3, 100)], None).unwrap();
        let (s1, s2, s3) = (
            three.class_share_permille(&h64(1)).unwrap(),
            three.class_share_permille(&h64(2)).unwrap(),
            three.class_share_permille(&h64(3)).unwrap(),
        );
        assert_eq!(s1 as u32 + s2 as u32 + s3 as u32, 1000, "the table sums to the denominator at every mutation");
        assert_eq!(s3, 100);
        assert_eq!((s1, s2), (600, 300), "667→600.3 and 333→299.7: one residue permille, to the larger remainder");
        // Registration refuses a zero boot target — an unmeetable difficulty is a dead class.
        let p = params();
        let err = apply_palw_transition_v2(
            &PalwChainStateV2::genesis(),
            &p,
            &ctx(1, 100, 1),
            &[PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(9),
                artifact_root: h64(19),
                slash_value_per_pwu: 1,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(10),
                initial_target: 0,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            }],
            None,
        );
        assert!(matches!(err, Err(PalwStateV2Error::ZeroClassTarget(_))));
        // ADR-0045 Decision 1: both rule shapes must license something. A zero per-inference
        // cost derives pwu = 0 against a stateless floor of 1 — a class nobody can mine; a zero
        // ceiling is the same dead class in the older costume.
        for (rule, name) in [
            (PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 0 }, "zero per-inference cost"),
            (PalwPwuRuleV2::MaxPerAttempt(0), "zero ceiling"),
        ] {
            let err = apply_palw_transition_v2(
                &PalwChainStateV2::genesis(),
                &p,
                &ctx(1, 100, 1),
                &[PalwConsensusObjectV2::ClassRegistered {
                    class_id: h64(9),
                    artifact_root: h64(19),
                    slash_value_per_pwu: 1,
                    pwu_rule: rule,
                    initial_target: u128::MAX / 2,
                    share_permille: 1000,
                    activation_daa: 0,
                    admission: None,
                }],
                None,
            );
            assert!(
                matches!(err, Err(PalwStateV2Error::ZeroPwuPerInference(_)) | Err(PalwStateV2Error::ZeroPwuCeiling(_))),
                "{name} must refuse registration"
            );
        }
    }

    // ---- params ----

    #[test]
    fn params_refuse_out_of_range_values() {
        assert!(PalwStateParamsV2::new(1001, 1, 1, 1, 1, 1, h64(1), 4, 1000, 100, 1000, 0).is_err(), "β > 1");
        assert!(PalwStateParamsV2::new(100, 0, 1, 1, 1, 1, h64(1), 4, 1000, 100, 1000, 0).is_err(), "zero bind window");
        assert!(PalwStateParamsV2::new(100, 1, 0, 1, 1, 1, h64(1), 4, 1000, 100, 1000, 0).is_err(), "zero receipt window");
        assert!(PalwStateParamsV2::new(100, 1, 1, 0, 1, 1, h64(1), 4, 1000, 100, 1000, 0).is_err(), "zero challenge window");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 0, 1, h64(1), 4, 1000, 100, 1000, 0).is_err(), "zero court window");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 0, h64(1), 4, 1000, 100, 1000, 0).is_err(), "zero epoch length");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, Hash64::default(), 4, 1000, 100, 1000, 0).is_err(), "zero base class id");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, h64(1), 1, 1000, 100, 1000, 0).is_err(), "max_factor below 2");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, h64(1), 4, 999, 100, 1000, 0).is_err(), "tolerance below unity");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, h64(1), 4, 4_001, 100, 1000, 0).is_err(), "tolerance above the ceiling");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, h64(1), 4, 1000, 100, 0, 0).is_err(), "zero attempt share");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, h64(1), 4, 1000, 100, 1001, 0).is_err(), "attempt share above 1000");
        assert!(PalwStateParamsV2::new(1000, 1, 1, 1, 1, 1, h64(1), 4, 1000, 100, 1000, 0).is_ok(), "β = 1 exactly is the boundary");
        // The grant floor tracks the epoch geometry: at E = 1000 · tol = 1000 the floor is 1‰,
        // and shrinking the epoch to 100 raises it to 10‰ — the share too small to buy one
        // worst-case block per epoch is not grantable, which is what keeps a mid-flight zero
        // budget unrepresentable.
        assert_eq!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap().min_grantable_share_permille(), 1);
        assert_eq!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 100, h64(1), 4, 1000, 100, 1000, 0).unwrap().min_grantable_share_permille(), 10);
    }

    #[test]
    fn the_beta_rounding_is_floor_and_only_floor() {
        let p = PalwStateParamsV2::new(333, 10, 10, 10, 10, 10, h64(1), 4, 1000, 100, 1000, 0).unwrap();
        assert_eq!(immature_contribution_v2(&p, 10), 3, "⌊10·333/1000⌋ = 3, never 4");
        assert_eq!(immature_contribution_v2(&p, 1), 0, "⌊1·333/1000⌋ = 0: a tiny claim may contribute nothing");
        assert_eq!(immature_contribution_v2(&p, 3), 0);
        let full = PalwStateParamsV2::new(1000, 10, 10, 10, 10, 10, h64(1), 4, 1000, 100, 1000, 0).unwrap();
        assert_eq!(immature_contribution_v2(&full, 40), 40, "β = 1 is identity");
    }

    // ---- ADR-0044 (FP-03): free-prompt claims, certification, and quantum spends ----

    fn fp_commit(claim_word: u64, pwu: u64, quanta: u32) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::FreePromptCommitted {
            claim: h64(claim_word),
            class_id: h64(1),
            bond: bond_key(1),
            pwu,
            quanta,
            trace_root: h64(41),
            output_root: h64(42),
            execution_root: h64(43),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
        }
    }

    fn fp_spend(claim_word: u64, quantum_index: u32) -> PalwReceiptSpendUnsignedV3 {
        // The state machine never reads the position binding (that is the header's business), but
        // a fixture carrying a default there would be a shape the wire cannot carry.
        let bond = bond_key(1).0;
        PalwReceiptSpendUnsignedV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: h64(999),
            challenge: crate::palw_freeprompt_v3::spend_challenge_v3(
                h64(999),
                h64(0xB0),
                1_700,
                7,
                h64(claim_word),
                quantum_index,
                &bond,
            ),
            claim_id: h64(claim_word),
            quantum_index,
            beacon_block: h64(0xBEAC),
            producer_bond: bond,
            producer_pubkey: vec![7; 4],
        }
    }

    /// [`apply`] for the V3 work slot, with both consistency checkers.
    fn apply_work(
        parent: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        c: &PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
    ) -> (PalwChainStateV2, PalwStateDeltaV2) {
        let (state, delta) = apply_palw_transition_v3(parent, p, c, objects, work).expect("transition applies");
        state.assert_internal_consistency(p).expect("internal consistency after apply");
        state.assert_deadline_consistency(p).expect("deadline consistency after apply");
        (state, delta)
    }

    /// Drive one committed FP claim through the lattice to `Final`: commit at daa 101, bind at
    /// 102, license at 103, sweep past the challenge window at 124. Returns the certified state.
    fn certify_fp_claim(p: &PalwStateParamsV2, pwu: u64, quanta: u32) -> PalwChainStateV2 {
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let (s2, _) = apply(&s1, p, &ctx(2, 101, 2), &[fp_commit(0xFC, pwu, quanta)], None);
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s3, _) =
            apply(&s2, p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: h64(0xFC), anchor: h64(77), seats }], None);
        let (s4, _) = apply(&s3, p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC), receipts: Vec::new() }], None);
        let (s5, _) = apply(&s4, p, &ctx(5, 124, 5), &[], None);
        assert!(matches!(s5.claim(&h64(0xFC)).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the fixture certifies");
        s5
    }

    /// **ADR-0044's weight divergence, end to end.** A free-prompt claim walks the SAME lattice
    /// as an attempt, but: it carries zero immature weight while pending, its `Final` adds
    /// NOTHING to safe weight (certification licenses, it does not weigh), an unspent certified
    /// receipt does not hold the frontier back — and each spent quantum adds exactly the uniform
    /// per-quantum weight, once, on this chain.
    #[test]
    fn fp_claim_certifies_without_weight_and_spends_add_it_per_quantum() {
        let p = params();
        let certified = certify_fp_claim(&p, 60, 3);

        // Certification moved NO weight, released the exposure, and freed the frontier.
        assert_eq!((certified.safe_weight(), certified.bounded_immature()), (0, 0), "Final licenses; it does not weigh");
        assert_eq!(certified.reserved_exposure(&bond_key(1)), 0, "exposure released at Final");
        // The frontier measures MATURED WORK, not the absence of open claims (the audit fix that
        // stopped an attempt-less fork from out-ranking an honest chain), so a certified receipt
        // moves it to the receipt's OWN accepting block — blue 2, where the commitment landed —
        // not to the sweeping block. What this asserts is the FP property: an unspent certified
        // receipt does not HOLD the frontier back at 0, it releases it.
        assert_eq!(
            certified.safe_frontier(),
            (2, block(2)),
            "an unspent certified receipt does not block the frontier — it matures at its own point"
        );

        // Spend quantum 0: exactly pwu/quanta = 20 safe weight, and the receipt census counts it.
        let spend0 = fp_spend(0xFC, 0);
        let (s6, _) = apply_work(&certified, &p, &ctx(6, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&spend0));
        assert_eq!(s6.safe_weight(), 20, "one quantum = pwu/quanta, re-derived, never carried");
        let census = s6.receipt_epoch_counter(&h64(1)).expect("the spend is counted");
        assert_eq!((census.produced_blocks, census.produced_pwu), (1, 20));
        assert!(s6.epoch_counter(&h64(1)).is_none(), "a spend is receipt-lane production, not attempt-lane");

        // Spend quantum 2 on top: weight accumulates linearly.
        let spend2 = fp_spend(0xFC, 2);
        let (s7, _) = apply_work(&s6, &p, &ctx(7, 131, 7), &[], PalwBlockWorkV3::ReceiptSpend(&spend2));
        assert_eq!(s7.safe_weight(), 40);

        // The same quantum cannot be spent twice on this chain…
        let again = apply_palw_transition_v3(&s7, &p, &ctx(8, 132, 8), &[], PalwBlockWorkV3::ReceiptSpend(&spend0));
        assert_eq!(again.unwrap_err(), PalwStateV2Error::QuantumAlreadySpent { claim: h64(0xFC), index: 0 });
        // …and a quantum that never existed is named, not modulo-wrapped.
        let ghost = fp_spend(0xFC, 3);
        let out_of_range = apply_palw_transition_v3(&s7, &p, &ctx(8, 132, 8), &[], PalwBlockWorkV3::ReceiptSpend(&ghost));
        assert_eq!(out_of_range.unwrap_err(), PalwStateV2Error::QuantumOutOfRange { claim: h64(0xFC), index: 3, quanta: 3 });
    }

    /// The two lanes share ONE exposure ceiling (invariant F13): an FP commitment reserves
    /// against the same bond accumulator an attempt does — and contributes zero immature weight,
    /// so commitment-stuffing cannot pump a chain's live total without blocks.
    #[test]
    fn fp_commitments_share_the_bond_exposure_and_carry_no_immature_weight() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        let env = attempt(40, 1);
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        assert_eq!(s2.reserved_exposure(&bond_key(1)), 200, "the attempt reserves 40 × 5");
        assert_eq!(s2.bounded_immature(), 4);

        let (s3, _) = apply(&s2, &p, &ctx(3, 102, 3), &[fp_commit(0xFC, 60, 3)], None);
        assert_eq!(s3.reserved_exposure(&bond_key(1)), 500, "the FP commitment adds 60 × 5 to the SAME ceiling");
        assert_eq!(s3.bounded_immature(), 4, "the FP commitment adds NO immature weight");
        assert_eq!(s3.safe_weight(), 0);

        // Keep the attempt claim alive past the FP claim's bind deadline (bind its panel: its
        // receipt window re-arms to 105 + 10), so the sweep at 113 voids ONLY the FP claim…
        let attempt_claim = attempt_id_v2(&attempt(40, 1).attempt);
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s4, _) = apply(
            &s3,
            &p,
            &ctx(4, 105, 4),
            &[PalwConsensusObjectV2::PanelBound { claim: attempt_claim, anchor: h64(77), seats }],
            None,
        );
        // …the FP bind deadline is 102 + 10 = 112; the attempt's receipt deadline is 115.
        let (s5, _) = apply(&s4, &p, &ctx(5, 113, 5), &[], None);
        assert!(matches!(
            s5.claim(&h64(0xFC)).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }
        ));
        assert!(!s5.claim(&attempt_claim).unwrap().phase.is_terminal(), "the attempt claim is still pending");
        assert_eq!(s5.reserved_exposure(&bond_key(1)), 200, "the void releases the FP reserve, byte for byte");
    }

    /// A spend licenses only what is certified: wrong phase, wrong source, absent claim — each
    /// refusal is named.
    #[test]
    fn fp_spends_require_a_final_free_prompt_claim() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        // Absent claim.
        let spend = fp_spend(0xFC, 0);
        let missing = apply_palw_transition_v3(&s1, &p, &ctx(2, 101, 2), &[], PalwBlockWorkV3::ReceiptSpend(&spend));
        assert_eq!(missing.unwrap_err(), PalwStateV2Error::MissingClaim(h64(0xFC)));

        // Provisional (committed, not yet certified).
        let (s2, _) = apply(&s1, &p, &ctx(2, 101, 2), &[fp_commit(0xFC, 60, 3)], None);
        let premature = apply_palw_transition_v3(&s2, &p, &ctx(3, 102, 3), &[], PalwBlockWorkV3::ReceiptSpend(&spend));
        assert_eq!(premature.unwrap_err(), PalwStateV2Error::WrongPhase { claim: h64(0xFC), edge: "ReceiptSpend" });

        // An attempt claim, even Final, is not spendable — its work was weighed at its own block.
        let env = attempt(40, 1);
        let attempt_claim = attempt_id_v2(&env.attempt);
        let (s3, _) = apply(&s2, &p, &ctx(3, 102, 3), &[], Some(&env));
        let wrong_source = fp_spend(0, 0);
        let mut wrong = wrong_source.clone();
        wrong.claim_id = attempt_claim;
        let refused = apply_palw_transition_v3(&s3, &p, &ctx(4, 103, 4), &[], PalwBlockWorkV3::ReceiptSpend(&wrong));
        assert_eq!(refused.unwrap_err(), PalwStateV2Error::NotFreePromptClaim(attempt_claim));
    }

    /// Malformed commitments are refused at the door: zero quanta parks nothing in state, and a
    /// pwu that does not divide into uniform non-zero quanta is not a commitment.
    #[test]
    fn fp_commitments_with_broken_quantization_are_refused() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);

        let zero = apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &[fp_commit(0xFC, 60, 0)], None);
        assert_eq!(zero.unwrap_err(), PalwStateV2Error::ZeroQuanta);

        let ragged = apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &[fp_commit(0xFC, 61, 3)], None);
        assert_eq!(ragged.unwrap_err(), PalwStateV2Error::NonUniformQuanta { pwu: 61, quanta: 3 });

        let hollow = apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &[fp_commit(0xFC, 0, 3)], None);
        assert_eq!(hollow.unwrap_err(), PalwStateV2Error::NonUniformQuanta { pwu: 0, quanta: 3 });
    }

    /// The spend's delta replays and reverts bit-for-bit — the reorg primitive holds for the new
    /// entry kinds (Claim ledger mutation, Weights, ReceiptEpoch).
    #[test]
    fn fp_spend_delta_replays_and_reverts_exactly() {
        let p = params();
        let certified = certify_fp_claim(&p, 60, 3);
        let spend0 = fp_spend(0xFC, 0);
        let (child, delta) = apply_work(&certified, &p, &ctx(6, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&spend0));

        let replayed = apply_delta_v2(&certified, &delta, &p).expect("the delta applies to its own parent");
        assert_eq!(replayed, child, "replay reproduces the transition bit-for-bit");
        let reverted = revert_delta_v2(&child, &delta, &p).expect("the delta reverts from its own child");
        assert_eq!(reverted, certified, "revert restores the parent bit-for-bit");
        assert!(
            apply_delta_v2(&child, &delta, &p).is_err(),
            "the spend delta cannot double-apply — the ledger's old value no longer matches"
        );
    }

    /// **The two-lane retarget (ADR-0044 Decision 5/9).** One combined census, split once by the
    /// attempt share, each lane retargeted by the SAME rule against its scaled expectation — and
    /// at split = 1000 the receipt lane measures nothing and its target never moves.
    #[test]
    fn two_lane_retarget_splits_one_census() {
        // Split 800‰: epoch_length 100, so daa 100..200 is epoch 1, closed when a block lands at
        // daa ≥ 200.
        let split = PalwStateParamsV2::new(100, 60, 60, 20, 500, 100, h64(1), 4, 1000, 100, 800, 0).unwrap();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply(&genesis, &split, &ctx(1, 100, 1), &register_class_and_bond(), None);

        // Epoch 1 production: 1 attempt block…
        let env = attempt(40, 1);
        let (s2, _) = apply(&s1, &split, &ctx(2, 110, 2), &[], Some(&env));
        // …and 2 receipt blocks, from a claim committed and certified inside the epoch (bind at
        // 112, license at 113, challenge window ends 133, swept Final at 140).
        let (s3, _) = apply(&s2, &split, &ctx(3, 111, 3), &[fp_commit(0xFC, 60, 3)], None);
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s4, _) =
            apply(&s3, &split, &ctx(4, 112, 4), &[PalwConsensusObjectV2::PanelBound { claim: h64(0xFC), anchor: h64(77), seats }], None);
        let (s5, _) = apply(&s4, &split, &ctx(5, 113, 5), &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC), receipts: Vec::new() }], None);
        let (s6, _) = apply(&s5, &split, &ctx(6, 140, 6), &[], None);
        let spend0 = fp_spend(0xFC, 0);
        let spend1 = fp_spend(0xFC, 1);
        let (s7, _) = apply_work(&s6, &split, &ctx(7, 141, 7), &[], PalwBlockWorkV3::ReceiptSpend(&spend0));
        let (s8, _) = apply_work(&s7, &split, &ctx(8, 142, 8), &[], PalwBlockWorkV3::ReceiptSpend(&spend1));

        let boot = s8.class_target(&h64(1)).unwrap().target;
        assert_eq!(s8.receipt_target(&h64(1)).unwrap().target, boot, "both lanes still sit at the registration seed");

        // Cross the epoch boundary: ONE combined census of 3 blocks; the attempt lane retargets
        // its 1 observed block against composed share 1000 × 800‰ = 800‰, the receipt lane its 2
        // observed against 200‰ — the receipt lane over-produced its split (2 of 3 ≫ 200‰) and
        // the rule simply measures it, which is exactly the case that broke the scaled-total
        // draft (a synthetic span smaller than a lane's real production is not a census).
        let (s9, _) = apply(&s8, &split, &ctx(9, 205, 9), &[], None);
        let expected_attempt = crate::palw_class_daa::retarget_over_span_v1(
            boot,
            &crate::palw_class_daa::PalwClassSpanCensusV1 { class_daa_blocks: 1, total_daa_blocks: 3 },
            800,
            4,
        )
        .unwrap();
        let expected_receipt = crate::palw_class_daa::retarget_over_span_v1(
            boot,
            &crate::palw_class_daa::PalwClassSpanCensusV1 { class_daa_blocks: 2, total_daa_blocks: 3 },
            200,
            4,
        )
        .unwrap();
        assert_eq!(s9.class_target(&h64(1)).unwrap().target, expected_attempt, "the attempt lane retargets against its split");
        assert_eq!(s9.receipt_target(&h64(1)).unwrap().target, expected_receipt, "the receipt lane retargets against the remainder");
        assert_ne!(expected_attempt, expected_receipt, "the fixture actually separates the lanes");

        // And at split = 1000 (every pre-FP fixture), the receipt lane never moves: same walk,
        // attempt production only.
        let pure = PalwStateParamsV2::new(100, 60, 60, 20, 500, 100, h64(1), 4, 1000, 100, 1000, 0).unwrap();
        let (t1, _) = apply(&PalwChainStateV2::genesis(), &pure, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let (t2, _) = apply(&t1, &pure, &ctx(2, 110, 2), &[], Some(&attempt(40, 1)));
        let boot_pure = t2.receipt_target(&h64(1)).unwrap().target;
        let (t3, _) = apply(&t2, &pure, &ctx(3, 205, 3), &[], None);
        assert_eq!(t3.receipt_target(&h64(1)).unwrap().target, boot_pure, "split = 1000: the receipt lane measures nothing");
        // …and the attempt lane runs the old rule exactly: one class holding 1000‰ producing the
        // whole span is the V1 rule's deliberate no-op, so the target holds still here too.
        assert_eq!(t3.class_target(&h64(1)).unwrap().target, boot_pure, "one class at its exact share: the old rule's no-op");
    }

    /// The carriage round-trips the FP additions: a certified-and-partly-spent state serializes,
    /// reloads under its committed root, and refuses a tampered spend ledger.
    #[test]
    fn fp_state_carriage_roundtrips_and_refuses_a_tampered_ledger() {
        let p = params();
        let certified = certify_fp_claim(&p, 60, 3);
        let spend0 = fp_spend(0xFC, 0);
        let (state, _) = apply_work(&certified, &p, &ctx(6, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&spend0));

        let root = state.state_root();
        let carriage = PalwStateCarriageV2::from_state(&state);
        let reloaded = carriage.clone().into_state(&p, Some(root)).expect("the honest carriage reloads under its root");
        assert_eq!(reloaded.state_root(), root);
        assert_eq!(reloaded, state);

        // A tampered ledger (the spend quietly erased, weight kept) is caught by the
        // self-consistency check — safe weight no longer equals what the claims imply.
        let mut tampered = carriage;
        let claim = tampered.claims.get_mut(&h64(0xFC)).unwrap();
        let PalwClaimSourceV2::FreePrompt { spent, .. } = &mut claim.source else { panic!("fp claim") };
        spent.clear();
        assert!(tampered.into_state(&p, Some(root)).is_err(), "an erased spend ledger cannot reload");
    }

    /// The delta is the store's row (Unit C): every entry kind round-trips through borsh, so a
    /// reorg reading from disk reverts exactly what the transition wrote.
    #[test]
    fn every_delta_entry_kind_round_trips_through_borsh() {
        let p = params();
        // One walk that touches every entry kind: registrations (Bond/Class/Target/ReceiptTarget),
        // an attempt (Claim/Exposure/Epoch/Weights/Frontier/LastPoint), a panel (Panel), a court
        // (Court), and an FP spend (ReceiptEpoch).
        let g = PalwChainStateV2::genesis();
        let (s1, d1) = apply(&g, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, d2) = apply(&s1, &p, &ctx(2, 101, 2), &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s3, d3) =
            apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        let (s4, d4) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
        let (_, d5) = apply(
            &s4,
            &p,
            &ctx(5, 104, 5),
            &[court_open(claim_id, h64(31), bond_key(1), bond_key(1))],
            None,
        );
        let certified = certify_fp_claim(&p, 60, 3);
        let spend = fp_spend(0xFC, 0);
        let (_, d6) = apply_work(&certified, &p, &ctx(6, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&spend));

        let mut kinds: BTreeSet<&'static str> = BTreeSet::new();
        for delta in [&d1, &d2, &d3, &d4, &d5, &d6] {
            let bytes = borsh::to_vec(delta).expect("a delta serializes");
            let decoded: PalwStateDeltaV2 = borsh::from_slice(&bytes).expect("and decodes");
            assert_eq!(&decoded, delta, "the row is the delta, byte for byte");
            for entry in &delta.entries {
                kinds.insert(match entry {
                    PalwDeltaEntryV2::Bond { .. } => "bond",
                    PalwDeltaEntryV2::Exposure { .. } => "exposure",
                    PalwDeltaEntryV2::Class { .. } => "class",
                    PalwDeltaEntryV2::Target { .. } => "target",
                    PalwDeltaEntryV2::Share { .. } => "share",
                    PalwDeltaEntryV2::EpochBudgets { .. } => "epoch_budgets",
                    PalwDeltaEntryV2::ReceiptTarget { .. } => "receipt_target",
                    PalwDeltaEntryV2::Capability { .. } => "capability",
                    PalwDeltaEntryV2::Claim { .. } => "claim",
                    PalwDeltaEntryV2::Payout { .. } => "payout",
                    PalwDeltaEntryV2::Panel { .. } => "panel",
                    PalwDeltaEntryV2::Court { .. } => "court",
                    PalwDeltaEntryV2::Epoch { .. } => "epoch",
                    PalwDeltaEntryV2::ReceiptEpoch { .. } => "receipt_epoch",
                    PalwDeltaEntryV2::Weights { .. } => "weights",
                    PalwDeltaEntryV2::Frontier { .. } => "frontier",
                    PalwDeltaEntryV2::LastPoint { .. } => "last_point",
                });
            }
        }
        // The walk above is only worth trusting if it actually exercised the variety it claims.
        assert!(kinds.len() >= 10, "the fixture walk covered {} entry kinds, expected at least 10", kinds.len());
    }

    // ---- FP-08: the reorg-equivalence gate — the walk a real reorg executes, on FP state ----
    //
    // The audit register is unanimous that P0-3/4/5 were born in the virtual processor's
    // reorg walk (`calculate_utxo_state_relatively`): revert the deltas from the old sink down
    // to the fork point, apply the new branch's deltas up. These tests drive THAT exact
    // primitive pair (`revert_delta_v2`/`apply_delta_v2`) over free-prompt commitments and
    // quantum spends, and pin the three properties a wiring layer must not break:
    //
    //   1. reorg-by-delta reaches the SAME state as building the winning branch fresh
    //      (`apply_delta` is the true inverse of the transition — P0-4's sink-independence);
    //   2. a quantum spent on the losing branch is UNSPENT after the reorg, and free to spend
    //      on the winning branch (spends are candidate-scoped, the UTXO double-spend analogy);
    //   3. a certified receipt from BEFORE the fork survives the reorg with its ledger intact.

    /// Build [prefix → branch] by delta, then reorg to a sibling branch by reverting the first
    /// branch's deltas (newest-first) and applying the sibling's — and assert the result is
    /// bit-identical to building `prefix → sibling` from scratch. This is the exact access
    /// pattern `calculate_utxo_state_relatively` runs, on FP-bearing blocks.
    #[test]
    fn fp_reorg_by_delta_equals_building_the_winning_branch_fresh() {
        let p = params();
        // Shared prefix: register, commit an FP claim, certify it to Final (so both branches
        // inherit a spendable receipt), reaching daa 124 with the claim at Final.
        let g = PalwChainStateV2::genesis();
        let (s1, d1) = apply(&g, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let (s2, d2) = apply(&s1, &p, &ctx(2, 101, 2), &[fp_commit(0xFC, 60, 3)], None);
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
        let (s3, d3) = apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: h64(0xFC), anchor: h64(77), seats }], None);
        let (s4, d4) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC), receipts: Vec::new() }], None);
        let (fork, d5) = apply(&s4, &p, &ctx(5, 124, 5), &[], None);
        assert!(matches!(fork.claim(&h64(0xFC)).unwrap().phase, PalwClaimPhaseV2::Final { .. }));

        // Losing branch A: spends quanta 0 and 1 at daa 130/131.
        let spend_a0 = fp_spend(0xFC, 0);
        let spend_a1 = fp_spend(0xFC, 1);
        let (a1, da1) = apply_work(&fork, &p, &ctx(0xA1, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&spend_a0));
        let (a2, da2) = apply_work(&a1, &p, &ctx(0xA2, 131, 7), &[], PalwBlockWorkV3::ReceiptSpend(&spend_a1));
        assert_eq!(a2.safe_weight(), 40, "branch A weighed two 20-pwu quanta");

        // Winning branch B off the SAME fork: spends quantum 0 only, but at a heavier blue score.
        let spend_b0 = fp_spend(0xFC, 0);
        let (b1, db1) = apply_work(&fork, &p, &ctx(0xB1, 130, 20), &[], PalwBlockWorkV3::ReceiptSpend(&spend_b0));

        // The reorg: from sink A2, revert da2 then da1 (newest-first) to reach the fork, then
        // apply db1 up branch B — the literal loop of `calculate_utxo_state_relatively`.
        let back_to_a1 = revert_delta_v2(&a2, &da2, &p).unwrap();
        assert_eq!(back_to_a1, a1, "revert of the last delta restores its parent exactly");
        let back_to_fork = revert_delta_v2(&back_to_a1, &da1, &p).unwrap();
        assert_eq!(back_to_fork, fork, "reverting down to the fork reproduces the fork state");
        let reorged = apply_delta_v2(&back_to_fork, &db1, &p).unwrap();

        // The gate: the delta-walked state equals branch B built fresh, byte for byte and root
        // for root. A wiring layer that drifts here is P0-4.
        assert_eq!(reorged, b1, "reorg-by-delta reaches the freshly-built winning branch");
        assert_eq!(reorged.state_root(), b1.state_root());
        reorged.assert_internal_consistency(&p).unwrap();
        reorged.assert_deadline_consistency(&p).unwrap();

        // Property 2: quantum 0 is spent on B (it was spent there), and quantum 1 — spent only on
        // the abandoned branch A — is UNSPENT after the reorg, free to spend again on B.
        let PalwClaimSourceV2::FreePrompt { spent, .. } = &reorged.claim(&h64(0xFC)).unwrap().source else { panic!("fp") };
        assert!(spent.contains(&0) && !spent.contains(&1), "the reorg carries B's spends, not A's");
        assert_eq!(reorged.safe_weight(), 20, "only B's single spend weighs after the reorg");
        let spend_b1 = fp_spend(0xFC, 1);
        apply_palw_transition_v3(&reorged, &p, &ctx(0xB2, 131, 21), &[], PalwBlockWorkV3::ReceiptSpend(&spend_b1))
            .expect("a quantum spent only on the abandoned branch is free on the winning one");

        // Silence a warning without weakening the test: the prefix deltas are the branch both
        // sides share, exercised by construction.
        let _ = (&d1, &d2, &d3, &d4, &d5, &da1, &db1);
    }

    /// Property 3, and the UTXO analogy stated as a test: the SAME spend envelope is valid on
    /// BOTH branches of a fork (a producer legally follows whichever wins), yet each branch's
    /// spent-set is its own — there is no node-global "already spent" cache, and reverting one
    /// branch never leaks its ledger into the other.
    #[test]
    fn fp_same_quantum_spends_on_both_forks_and_the_sets_stay_scoped() {
        let p = params();
        let certified = certify_fp_claim(&p, 40, 2);
        let spend = fp_spend(0xFC, 0);

        // Two sibling branches off the certified fork, each spending quantum 0.
        let (branch_a, _) = apply_work(&certified, &p, &ctx(0xA1, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&spend));
        let (branch_b, delta_b) = apply_work(&certified, &p, &ctx(0xB1, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&spend));

        for branch in [&branch_a, &branch_b] {
            let PalwClaimSourceV2::FreePrompt { spent, .. } = &branch.claim(&h64(0xFC)).unwrap().source else { panic!("fp") };
            assert_eq!(spent.iter().copied().collect::<Vec<_>>(), vec![0], "each branch spent quantum 0 on its own chain");
            assert_eq!(branch.safe_weight(), 20);
        }
        // The branches differ only in their accepting block, so their roots differ — the spend is
        // the same fact on two chains, not one shared mutable cell.
        assert_ne!(branch_a.state_root(), branch_b.state_root(), "same spend, two candidate chains, two roots");

        // Reverting branch B restores the certified fork with quantum 0 UNSPENT — B's ledger did
        // not persist into the shared parent, which is what makes A's identical spend sound.
        let reverted = revert_delta_v2(&branch_b, &delta_b, &p).unwrap();
        assert_eq!(reverted, certified, "revert restores the pre-spend fork");
        let PalwClaimSourceV2::FreePrompt { spent, .. } = &reverted.claim(&h64(0xFC)).unwrap().source else { panic!("fp") };
        assert!(spent.is_empty(), "the reverted fork has no spends — the ledger is branch-scoped");
    }

    /// Sink-independence for FP state, the P0-4 property in the `PalwStateBookV2` access pattern:
    /// apply BOTH branches into ONE book (as a real node holds many branches in one store), then
    /// ask each candidate for its standing — and get an answer that is a function of that
    /// candidate's chain alone, whatever order the branches were inserted.
    #[test]
    fn fp_book_answers_each_candidate_from_its_own_chain_regardless_of_insert_order() {
        let p = params();
        let genesis_block = block(0);

        // Build a reference: the certified fork's standing, and each branch's own weight, computed
        // in isolation.
        let certified = certify_fp_claim(&p, 60, 3);
        let (iso_a, _) = apply_work(&certified, &p, &ctx(0xA1, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0)));
        let (iso_b0, _) = apply_work(&certified, &p, &ctx(0xB1, 130, 20), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0)));
        let (iso_b, _) = apply_work(&iso_b0, &p, &ctx(0xB2, 131, 21), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 1)));

        // Now drive the SAME blocks through one book, inserting branch B before branch A (the
        // reverse of the isolated order) to prove the answer does not depend on it.
        let build_book = |insert_a_first: bool| -> PalwStateBookV2 {
            let mut book = PalwStateBookV2::new(p.clone());
            book.insert_genesis(genesis_block);
            book.apply_block(genesis_block, ctx(1, 100, 1), &register_class_and_bond(), None).unwrap();
            book.apply_block(block(1), ctx(2, 101, 2), &[fp_commit(0xFC, 60, 3)], None).unwrap();
            let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
            book.apply_block(block(2), ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: h64(0xFC), anchor: h64(77), seats }], None).unwrap();
            book.apply_block(block(3), ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC), receipts: Vec::new() }], None).unwrap();
            book.apply_block(block(4), ctx(5, 124, 5), &[], None).unwrap();
            let mut do_a = |book: &mut PalwStateBookV2| {
                book.apply_block_with_work(block(5), ctx(0xA1, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0))).unwrap();
            };
            let mut do_b = |book: &mut PalwStateBookV2| {
                book.apply_block_with_work(block(5), ctx(0xB1, 130, 20), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0))).unwrap();
                book.apply_block_with_work(block(0xB1), ctx(0xB2, 131, 21), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 1))).unwrap();
            };
            if insert_a_first {
                do_a(&mut book);
                do_b(&mut book);
            } else {
                do_b(&mut book);
                do_a(&mut book);
            }
            book
        };

        for insert_a_first in [true, false] {
            let book = build_book(insert_a_first);
            // Each candidate's stored state equals the one built in isolation — insert order does
            // not move it (P0-4).
            assert_eq!(book.state_of(&block(0xA1)).unwrap(), &iso_a, "branch A standing is chain-local (a_first={insert_a_first})");
            assert_eq!(book.state_of(&block(0xB2)).unwrap(), &iso_b, "branch B standing is chain-local (a_first={insert_a_first})");
            // And fork choice reads them through the one comparator: B (blue score 21, one Final
            // spend) outranks A (blue score 6) on the safe frontier.
            let order_a = book.state_of(&block(0xA1)).unwrap().candidate_order(block(0xA1));
            let order_b = book.state_of(&block(0xB2)).unwrap().candidate_order(block(0xB2));
            assert_eq!(
                compare_palw_candidates_v1(&order_b, &order_a),
                std::cmp::Ordering::Greater,
                "the heavier-frontier branch wins, whatever the insert order"
            );
        }
    }
}
