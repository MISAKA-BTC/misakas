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
use crate::tx::TransactionOutpoint;
use blake2b_simd::Params;
use kaspa_hashes::{Hash64, ZERO_HASH64};
use std::collections::{BTreeMap, BTreeSet};

/// Version 2 (ADR-0045): the root preimage gained `class_shares` and `epoch_budgets` in their
/// declared field positions — ADR-0043's rule for a consensus change to the root: a new
/// version, never a silent re-reading of old bytes.
pub const PALW_STATE_V2_VERSION: u16 = 2;

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
        })
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
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwClassStatusV2 {
    Active,
    Frozen { since_daa: u64 },
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
    /// inference — the class's step-leaf count, the same number the court's ladder walks.
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

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClaimStateV2 {
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
    },
    ClassFrozen {
        class_id: Hash64,
    },
    ClassUnfrozen {
        class_id: Hash64,
    },
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
    },
    CourtClosed {
        session_id: Hash64,
        verdict: PalwCourtVerdictV2,
    },
    /// Decision 7's producer default: a data obligation missed its deadline.
    ProducerDefaulted {
        claim: Hash64,
        receipts: Vec<PalwSeatVerdictV2>,
    },
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
    capabilities: BTreeMap<Hash64, PalwCapabilityStateV2>,
    claims: BTreeMap<Hash64, PalwClaimStateV2>,
    panels: BTreeMap<Hash64, PalwPanelStateV2>,
    court_sessions: BTreeMap<Hash64, PalwCourtSessionStateV2>,
    epoch_counters: BTreeMap<Hash64, PalwEpochCounterV2>,
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
            capabilities: BTreeMap::new(),
            claims: BTreeMap::new(),
            panels: BTreeMap::new(),
            court_sessions: BTreeMap::new(),
            epoch_counters: BTreeMap::new(),
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

    pub fn claim(&self, id: &Hash64) -> Option<&PalwClaimStateV2> {
        self.claims.get(id)
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
    /// the scalars. The exact ordering is frozen in ADR-0043; changing it — or what any
    /// collection's entry encoding covers — is a consensus change and needs a new domain string.
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
        state.update(collection_root(b"capabilities", &self.capabilities).as_byte_slice());
        state.update(collection_root(b"claims", &self.claims).as_byte_slice());
        state.update(collection_root(b"panels", &self.panels).as_byte_slice());
        state.update(collection_root(b"court_sessions", &self.court_sessions).as_byte_slice());
        state.update(collection_root(b"epoch_counters", &self.epoch_counters).as_byte_slice());
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
    pub fn assert_internal_consistency(&self) -> Result<(), PalwStateV2Error> {
        let mut exposure: BTreeMap<PalwBondKeyV2, u128> = BTreeMap::new();
        let mut safe: u128 = 0;
        let mut immature: u128 = 0;
        let mut unresolved: BTreeSet<(u64, Hash64)> = BTreeSet::new();
        for (id, claim) in &self.claims {
            match claim.phase {
                PalwClaimPhaseV2::Final { .. } => {
                    safe = safe.checked_add(claim.pwu as u128).ok_or(PalwStateV2Error::Overflow("consistency safe"))?;
                }
                PalwClaimPhaseV2::Voided { .. } => {}
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
        // boot. Registration grants and nothing revokes, so classes and shares are the same key
        // set; the donation arithmetic conserves the denominator, so a populated table sums to
        // exactly 1000‰. A state failing either was not built by the transition.
        if !self.classes.keys().eq(self.class_shares.keys()) {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "the class set and the share table disagree — a registered class holds a share, a share names a registered class".into(),
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
        let court_deadlines: BTreeSet<(u64, Hash64)> = self.court_sessions.iter().map(|(id, s)| (s.deadline_daa, *id)).collect();
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
        if self.deadlines.len() != expected_deadlines.len() {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "deadline index holds {} entries, the claims imply {}",
                self.deadlines.len(),
                expected_deadlines.len()
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
                PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => {}
            }
        }
        if expected != self.deadlines {
            return Err(PalwStateV2Error::CarriageInconsistent("deadline index differs from the claims' recomputed deadlines".into()));
        }
        Ok(())
    }
}

/// Shape-level helper for `assert_internal_consistency` (which runs without params): does this
/// claim, in this phase with this many open courts, owe the index a deadline entry at all?
fn expected_deadline(claim: &PalwClaimStateV2, open_courts: u32) -> Option<u64> {
    match claim.phase {
        PalwClaimPhaseV2::Provisional => Some(claim.accepted_daa),
        PalwClaimPhaseV2::PanelBound { bound_daa } => Some(bound_daa),
        PalwClaimPhaseV2::ReceiptLicensed { .. } if open_courts > 0 => None,
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => Some(licensed_daa),
        PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => None,
    }
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwDeltaEntryV2 {
    Bond { key: PalwBondKeyV2, old: Option<PalwBondStateV2>, new: Option<PalwBondStateV2> },
    Exposure { key: PalwBondKeyV2, old: Option<u128>, new: Option<u128> },
    Class { key: Hash64, old: Option<PalwClassStateV2>, new: Option<PalwClassStateV2> },
    Target { key: Hash64, old: Option<PalwClassTargetV2>, new: Option<PalwClassTargetV2> },
    Share { key: Hash64, old: Option<u16>, new: Option<u16> },
    EpochBudgets { old: Option<PalwEpochBudgetsV2>, new: Option<PalwEpochBudgetsV2> },
    Capability { key: Hash64, old: Option<PalwCapabilityStateV2>, new: Option<PalwCapabilityStateV2> },
    Claim { key: Hash64, old: Option<PalwClaimStateV2>, new: Option<PalwClaimStateV2> },
    Panel { key: Hash64, old: Option<PalwPanelStateV2>, new: Option<PalwPanelStateV2> },
    Court { key: Hash64, old: Option<PalwCourtSessionStateV2>, new: Option<PalwCourtSessionStateV2> },
    Epoch { key: Hash64, old: Option<PalwEpochCounterV2>, new: Option<PalwEpochCounterV2> },
    Weights { old: (u128, u128), new: (u128, u128) },
    Frontier { old: (u64, BlockHash), new: (u64, BlockHash) },
    LastPoint { old: Option<PalwBlockContextV2>, new: Option<PalwBlockContextV2> },
}

/// The full effect one block application had on the state, in application order. Applying it to
/// the same parent reproduces the transition's output exactly ([`apply_delta_v2`]); reverting it
/// from the child reproduces the parent ([`revert_delta_v2`]). Both are tested equal, which is
/// what makes a store layer built on deltas unable to drift from the transition.
#[derive(Clone, Debug, PartialEq, Eq)]
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
            self.state.court_deadlines.remove(&(previous.deadline_daa, key));
        }
        if let Some(record) = &new {
            *self.state.open_courts_by_claim.entry(record.claim).or_insert(0) += 1;
            self.state.court_deadlines.insert((record.deadline_daa, key));
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

    fn finalize_claim(&mut self, id: Hash64, claim: &PalwClaimStateV2, final_daa: u64) -> Result<(), PalwStateV2Error> {
        self.release_for_claim(claim)?;
        self.state.safe_weight =
            self.state.safe_weight.checked_add(claim.pwu as u128).ok_or(PalwStateV2Error::Overflow("safe_weight"))?;
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
        self.release_for_claim(claim)?;
        let mut voided = claim.clone();
        voided.phase = PalwClaimPhaseV2::Voided { voided_daa, reason };
        self.write_claim(id, Some(voided));
        self.disarm_deadline(id);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The transition
// ---------------------------------------------------------------------------------------------

/// Apply one chain block's PALW content to its parent state. Pure: `(parent, params, ctx,
/// objects, attempt) → (child, delta)`, or an error that rejects the block's PALW content
/// wholesale — a partial application is a state nobody else can recompute.
pub fn apply_palw_transition_v2(
    parent: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
    current_attempt: Option<&PalwAttemptEnvelopeV2>,
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

    // 4. The block's own attempt.
    if let Some(envelope) = current_attempt {
        apply_attempt(&mut builder, ctx, envelope)?;
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
fn sweep_court_deadlines(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    while let Some(&(deadline, session_id)) = builder.state.court_deadlines.iter().next() {
        if deadline >= ctx.daa_score {
            break;
        }
        let session = builder.state.court_sessions.get(&session_id).ok_or(PalwStateV2Error::MissingSession(session_id))?.clone();
        builder.write_court(session_id, None);
        let claim = builder.state.claims.get(&session.claim).ok_or(PalwStateV2Error::MissingClaim(session.claim))?.clone();
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
    let total: u64 = builder
        .state
        .epoch_counters
        .values()
        .filter(|counter| counter.epoch_index == closed_epoch)
        .map(|counter| counter.produced_blocks)
        .sum();
    if total == 0 {
        return Ok(());
    }
    // Snapshot the iteration set: the writes below mutate `class_targets`, never `classes`, but
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
    // Snapshot the census before any write, so the plan is a pure function of the parent state.
    let produced_in_span: BTreeMap<Hash64, u64> = class_ids
        .iter()
        .map(|id| {
            let blocks = builder
                .state
                .epoch_counters
                .get(id)
                .filter(|counter| counter.epoch_index == closed_epoch)
                .map(|counter| counter.produced_blocks)
                .unwrap_or(0);
            (*id, blocks)
        })
        .collect();
    let competing_permille: u64 = class_ids
        .iter()
        .filter(|id| !matches!(builder.state.classes.get(id).map(|c| &c.status), Some(PalwClassStatusV2::Frozen { .. })))
        .filter(|id| produced_in_span.get(id).copied().unwrap_or(0) > 0)
        // ADR-0045 Decision 3: shares are chain state now — the same fold, one lookup key over.
        .filter_map(|id| builder.state.class_shares.get(id).copied())
        .map(u64::from)
        .sum();
    if competing_permille == 0 {
        // Blocks were produced, but by no share-bearing unfrozen class. Nothing here is a
        // statement about any class's difficulty.
        return Ok(());
    }

    for class_id in class_ids {
        let class = builder.state.classes.get(&class_id).expect("iterating the map's own keys");
        if matches!(class.status, PalwClassStatusV2::Frozen { .. }) {
            continue;
        }
        let Some(share) = builder.state.class_shares.get(&class_id).copied() else { continue };
        let observed = produced_in_span.get(&class_id).copied().unwrap_or(0);
        if observed == 0 {
            // It was not in this span. Measuring it as an under-producer would ease its target on
            // every span it sits out, which is the same unbounded walk in the other direction.
            continue;
        }
        // Renormalized share, saturating at the whole denominator: a sole competing class is
        // expected to produce the whole span, which is what "its share of what happened" means.
        let share = u16::try_from((u64::from(share) * 1000 / competing_permille).min(1000))
            .expect("the value is clamped to 1000, which fits u16");
        let current = builder
            .state
            .class_targets
            .get(&class_id)
            .ok_or_else(|| PalwStateV2Error::Retarget(format!("class {class_id} has no target slot")))?
            .target;
        let census = crate::palw_class_daa::PalwClassSpanCensusV1 { class_daa_blocks: observed, total_daa_blocks: total };
        let next = crate::palw_class_daa::retarget_over_span_v1(current, &census, share, builder.params.class_daa_max_factor())
            .map_err(|e| PalwStateV2Error::Retarget(e.to_string()))?
            // A target of zero is not "impossibly hard", it is unrecoverable: the next retarget
            // returns `ZeroPreviousTarget` and every block after it is rejected, deterministically,
            // forever. One is the floor. With the normalization above nothing should walk here, and
            // this exists so that "should" is not what stands between the chain and a hard stop.
            .max(1);
        if next != current {
            builder.write_target(class_id, Some(PalwClassTargetV2 { target: next }));
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
                builder.void_claim(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ReceiptTimeout)?;
            }
            PalwClaimPhaseV2::ReceiptLicensed { .. } => {
                debug_assert!(
                    !builder.state.open_courts_by_claim.contains_key(&claim_id),
                    "a claim under court holds no final deadline"
                );
                builder.finalize_claim(claim_id, &claim, ctx.daa_score)?;
            }
            PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => {
                // A terminal claim owns no deadline; finding one is index corruption.
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
        PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, collateral } => {
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
            builder.write_bond(
                *bond,
                Some(PalwBondStateV2 {
                    pubkey: pubkey.clone(),
                    operator_id: palw_operator_id_v2(operator_pubkey),
                    collateral: *collateral,
                    slashed: 0,
                    status: PalwBondStatusV2::Active,
                    registered_daa: ctx.daa_score,
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
        PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, slash_value_per_pwu, pwu_rule, initial_target, share_permille } => {
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
            // ADR-0045 Decision 3: the share table mutates HERE and nowhere else. The first
            // class funds the liveness floor whole; every later entrant is funded by donation,
            // and the writes below are the only way a permille moves.
            let table = granted_share_table_v2(builder.params, &builder.state.class_shares, *class_id, *share_permille)?;
            for (id, share) in table {
                if builder.state.class_shares.get(&id).copied() != Some(share) {
                    builder.write_share(id, Some(share));
                }
            }
            builder.write_class(
                *class_id,
                Some(PalwClassStateV2 {
                    artifact_root: *artifact_root,
                    slash_value_per_pwu: *slash_value_per_pwu,
                    pwu_rule: *pwu_rule,
                    status: PalwClassStatusV2::Active,
                    registered_daa: ctx.daa_score,
                }),
            );
            builder.write_target(*class_id, Some(PalwClassTargetV2 { target: *initial_target }));
        }
        PalwConsensusObjectV2::ClassFrozen { class_id } => {
            let record = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?.clone();
            match record.status {
                PalwClassStatusV2::Frozen { .. } => return Err(PalwStateV2Error::FrozenClass(*class_id)),
                PalwClassStatusV2::Active => {
                    let mut frozen = record;
                    frozen.status = PalwClassStatusV2::Frozen { since_daa: ctx.daa_score };
                    builder.write_class(*class_id, Some(frozen));
                }
            }
        }
        PalwConsensusObjectV2::ClassUnfrozen { class_id } => {
            let record = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?.clone();
            match record.status {
                PalwClassStatusV2::Active => return Err(PalwStateV2Error::ClassNotFrozen(*class_id)),
                PalwClassStatusV2::Frozen { .. } => {
                    let mut thawed = record;
                    thawed.status = PalwClassStatusV2::Active;
                    builder.write_class(*class_id, Some(thawed));
                }
            }
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
        PalwConsensusObjectV2::CourtOpened { session_id, claim: claim_id, challenger_bond } => {
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
            builder.write_court(
                *session_id,
                Some(PalwCourtSessionStateV2 {
                    claim: *claim_id,
                    challenger_bond: *challenger_bond,
                    opened_daa: ctx.daa_score,
                    deadline_daa,
                }),
            );
            // An open court freezes the path to Final: the claim keeps no deadline while any
            // session is open (void-by-timeout of the COURT is PR-07's deadline system).
            if matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
                builder.disarm_deadline(*claim_id);
            }
        }
        PalwConsensusObjectV2::CourtClosed { session_id, verdict } => {
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
        PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id, receipts } => {
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            if claim.phase.is_terminal() {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "ProducerDefaulted" });
            }
            // Symmetric to the licensing arm: here the quorum says the producer withheld, so a
            // seat that signed `Valid` is the contradicted one. Punishing only one direction
            // would make the cheap lie obvious.
            builder.slash_dissenting_seats(&claim, receipts, false)?;
            // Decision 7's default is the producer's fault by construction — the panel answered
            // and it did not — so this void takes the stake, unlike the two timeouts, which void
            // a claim nobody was in a position to blame the producer for.
            builder.void_and_slash(*claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ProducerWithholding)?;
        }
    }
    Ok(())
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
        PalwDeltaEntryV2::Capability { key, old, new } => swap_write!(state.capabilities, key, old, new),
        PalwDeltaEntryV2::Claim { key, old, new } => swap_write!(state.claims, key, old, new),
        PalwDeltaEntryV2::Panel { key, old, new } => swap_write!(state.panels, key, old, new),
        PalwDeltaEntryV2::Court { key, old, new } => swap_write!(state.court_sessions, key, old, new),
        PalwDeltaEntryV2::Epoch { key, old, new } => swap_write!(state.epoch_counters, key, old, new),
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
        state.court_deadlines.insert((session.deadline_daa, *id));
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
    pub capabilities: BTreeMap<Hash64, PalwCapabilityStateV2>,
    pub claims: BTreeMap<Hash64, PalwClaimStateV2>,
    pub panels: BTreeMap<Hash64, PalwPanelStateV2>,
    pub court_sessions: BTreeMap<Hash64, PalwCourtSessionStateV2>,
    pub epoch_counters: BTreeMap<Hash64, PalwEpochCounterV2>,
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
            capabilities: state.capabilities.clone(),
            claims: state.claims.clone(),
            panels: state.panels.clone(),
            court_sessions: state.court_sessions.clone(),
            epoch_counters: state.epoch_counters.clone(),
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
            capabilities: self.capabilities,
            claims: self.claims,
            panels: self.panels,
            court_sessions: self.court_sessions,
            epoch_counters: self.epoch_counters,
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
        state.assert_internal_consistency()?;
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
        let parent = self.states.get(&parent_block).ok_or(PalwStateV2Error::MissingParentState(parent_block))?;
        let (child, delta) = apply_palw_transition_v2(parent, &self.params, &ctx, objects, attempt)?;
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
mod tests {
    use super::*;
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptUnsignedV2, challenge_v2};
    use crate::palw_fork_choice::compare_palw_candidates_v1;
    use crate::tx::TransactionId;

    fn params() -> PalwStateParamsV2 {
        // base = h64(1), max_factor = 4, tolerance = 1000‰ (grant floor: 1‰ at E = 1000).
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100).unwrap()
    }

    /// Operator identities are DERIVED from a key now, so the fixtures carry a key and let the
    /// state machine mint the id — the same path a real registration takes.
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
        PalwBlockContextV2 { block: block(block_word), daa_score: daa, blue_score: blue }
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
            },
            PalwConsensusObjectV2::BondRegistered { bond: bond_key(1), pubkey: vec![7; 4], operator_pubkey: op_key(21), collateral: 1_000 },
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
        state.assert_internal_consistency().expect("internal consistency after apply");
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

        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None);
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
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None);
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
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None);
        let (s5, _) = apply(
            &s4,
            &p,
            &ctx(5, 104, 5),
            &[PalwConsensusObjectV2::CourtOpened { session_id: h64(500), claim: claim_id, challenger_bond: bond_key(1) }],
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
            &[PalwConsensusObjectV2::CourtClosed { session_id: h64(500), verdict: PalwCourtVerdictV2::ChallengerDefeated }],
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
            &[PalwConsensusObjectV2::CourtOpened { session_id: h64(500), claim: claim_id, challenger_bond: bond_key(1) }],
            None,
        );
        let (s4, _) = apply(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::CourtClosed { session_id: h64(500), verdict: PalwCourtVerdictV2::ExecutorGuilty }],
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
                PalwConsensusObjectV2::CourtOpened { session_id: h64(600), claim: claim_id, challenger_bond: bond_key(1) },
                PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id, receipts: Vec::new() },
            ],
            None,
        );
        let (s6, _) = apply(
            &s5,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::CourtClosed { session_id: h64(600), verdict: PalwCourtVerdictV2::ExecutorGuilty }],
            None,
        );
        match s6.claim(&claim_id).unwrap().phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, voided_daa } => {
                assert_eq!(voided_daa, 102, "the earlier void stands; the late verdict only closed its session");
            }
            ref other => panic!("expected the standing void, got {other:?}"),
        }
        assert!(s6.court_session(&h64(600)).is_none());
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
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None);
        let (s5, _) = apply(
            &s4,
            &p,
            &ctx(5, 104, 5),
            &[PalwConsensusObjectV2::CourtOpened { session_id: h64(500), claim: claim_id, challenger_bond: bond_key(1) }],
            None,
        );
        // Inside the court budget (opened at 104, window 500 → deadline 604): frozen.
        let (s6, _) = apply(&s5, &p, &ctx(6, 600, 6), &[], None);
        assert!(matches!(s6.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        assert!(s6.court_session(&h64(500)).is_some());
        // Past it: the sweep closes the session challenger-side and re-arms Final at this point…
        let (s7, _) = apply(&s6, &p, &ctx(7, 605, 7), &[], None);
        assert!(s7.court_session(&h64(500)).is_none(), "the abandoned session expired");
        assert!(matches!(s7.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        // …and the next block past the re-armed deadline finals the claim.
        let (s8, _) = apply(&s7, &p, &ctx(8, 606, 8), &[], None);
        assert!(matches!(s8.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the freeze ended with the challenge");
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
                &[PalwConsensusObjectV2::CourtClosed { session_id: h64(1), verdict: PalwCourtVerdictV2::ExecutorGuilty }],
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

        // Frozen class refuses the attempt.
        let (frozen, _) = apply(&s1, &p, &ctx(2, 101, 2), &[PalwConsensusObjectV2::ClassFrozen { class_id: h64(1) }], None);
        assert!(matches!(
            apply_palw_transition_v2(&frozen, &p, &ctx(3, 102, 3), &[], Some(&attempt(40, 1))),
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
        let (m3, _) = apply(&m2, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None);
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
        let (h3, _) = apply(&h2, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None);
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
            &[PalwConsensusObjectV2::BondRegistered { bond: bond_key(2), pubkey: vec![8], operator_pubkey: op_key(22), collateral: 1_000 }],
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
            }],
            None,
        );
        let (with_claim, _) = apply(&base, &p, &ctx(2, 101, 2), &[], Some(&attempt(40, 1)));
        let (with_retire, _) =
            apply(&base, &p, &ctx(2, 101, 2), &[PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(1) }], None);
        let (with_freeze, _) = apply(&base, &p, &ctx(2, 101, 2), &[PalwConsensusObjectV2::ClassFrozen { class_id: h64(1) }], None);
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
            (4, 103, 4, vec![PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None),
            (
                5,
                104,
                5,
                vec![PalwConsensusObjectV2::CourtOpened { session_id: h64(500), claim: claim_id, challenger_bond: bond_key(1) }],
                None,
            ),
            (
                6,
                150,
                6,
                vec![PalwConsensusObjectV2::CourtClosed { session_id: h64(500), verdict: PalwCourtVerdictV2::ChallengerDefeated }],
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
            state.assert_internal_consistency().unwrap();
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
        });
        objects.push(PalwConsensusObjectV2::ClassFrozen { class_id: h64(2) });
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
        });
        objects.push(PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(2),
            pubkey: vec![8; 4],
            operator_pubkey: op_key(22),
            collateral: 1_000,
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
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(2),
                pubkey: vec![8; 4],
                operator_pubkey: op_key(0xAA),
                collateral: 1_000,
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
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }],
            None,
        );
        let sid = h64(0xC0)             ;
        let (s5, _) = apply(
            &s4,
            &p,
            &ctx(5, 104, 5),
            &[PalwConsensusObjectV2::CourtOpened { session_id: sid, claim: claim_id, challenger_bond: bond_key(1) }],
            None,
        );
        let (s6, _) = apply(
            &s5,
            &p,
            &ctx(6, 105, 6),
            &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict: PalwCourtVerdictV2::ExecutorGuilty }],
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
        assert!(PalwStateParamsV2::new(1001, 1, 1, 1, 1, 1, h64(1), 4, 1000, 100).is_err(), "β > 1");
        assert!(PalwStateParamsV2::new(100, 0, 1, 1, 1, 1, h64(1), 4, 1000, 100).is_err(), "zero bind window");
        assert!(PalwStateParamsV2::new(100, 1, 0, 1, 1, 1, h64(1), 4, 1000, 100).is_err(), "zero receipt window");
        assert!(PalwStateParamsV2::new(100, 1, 1, 0, 1, 1, h64(1), 4, 1000, 100).is_err(), "zero challenge window");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 0, 1, h64(1), 4, 1000, 100).is_err(), "zero court window");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 0, h64(1), 4, 1000, 100).is_err(), "zero epoch length");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, Hash64::default(), 4, 1000, 100).is_err(), "zero base class id");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, h64(1), 1, 1000, 100).is_err(), "max_factor below 2");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, h64(1), 4, 999, 100).is_err(), "tolerance below unity");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, h64(1), 4, 4_001, 100).is_err(), "tolerance above the ceiling");
        assert!(PalwStateParamsV2::new(1000, 1, 1, 1, 1, 1, h64(1), 4, 1000, 100).is_ok(), "β = 1 exactly is the boundary");
        // The grant floor tracks the epoch geometry: at E = 1000 · tol = 1000 the floor is 1‰,
        // and shrinking the epoch to 100 raises it to 10‰ — the share too small to buy one
        // worst-case block per epoch is not grantable, which is what keeps a mid-flight zero
        // budget unrepresentable.
        assert_eq!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 1000, h64(1), 4, 1000, 100).unwrap().min_grantable_share_permille(), 1);
        assert_eq!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 100, h64(1), 4, 1000, 100).unwrap().min_grantable_share_permille(), 10);
    }

    #[test]
    fn the_beta_rounding_is_floor_and_only_floor() {
        let p = PalwStateParamsV2::new(333, 10, 10, 10, 10, 10, h64(1), 4, 1000, 100).unwrap();
        assert_eq!(immature_contribution_v2(&p, 10), 3, "⌊10·333/1000⌋ = 3, never 4");
        assert_eq!(immature_contribution_v2(&p, 1), 0, "⌊1·333/1000⌋ = 0: a tiny claim may contribute nothing");
        assert_eq!(immature_contribution_v2(&p, 3), 0);
        let full = PalwStateParamsV2::new(1000, 10, 10, 10, 10, 10, h64(1), 4, 1000, 100).unwrap();
        assert_eq!(immature_contribution_v2(&full, 40), 40, "β = 1 is identity");
    }
}
