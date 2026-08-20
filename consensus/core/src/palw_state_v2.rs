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

/// Version 2: ADR-0044 (FP-03) added the free-prompt claim source, the per-quantum spend ledger,
/// and the receipt lane's target/census collections. Bumped BECAUSE the root's collection list
/// and the claim record's encoding both moved — nothing had persisted a version-1 root anywhere
/// (the module was consensus-inert), and the version sits inside both the root and the carriage
/// digest, so a v1 and a v2 state can never be mistaken for one another.
pub const PALW_STATE_V2_VERSION: u16 = 2;

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

/// Per-class DAA constants for the V2 retarget (ADR-0042 release-gate item 11, PR-09): the
/// share table ADR-0039 D5's expectation derives from, and the clamp factor. Constructed only
/// through [`PalwClassDaaV2Params::new`]. The startup gate (Decision 1, PR-10) additionally
/// demands the table sum to exactly 1000‰ with BASE-0 non-zero; here each entry is validated to
/// be a real share and the sum not to exceed the denominator.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassDaaV2Params {
    shares_permille: BTreeMap<Hash64, u16>,
    max_factor: u32,
}

impl PalwClassDaaV2Params {
    pub fn new(shares_permille: BTreeMap<Hash64, u16>, max_factor: u32) -> Result<Self, PalwStateV2Error> {
        if max_factor < 2 {
            return Err(PalwStateV2Error::InvalidParams("max_factor below 2 freezes the retarget"));
        }
        let mut sum: u32 = 0;
        for share in shares_permille.values() {
            if *share == 0 || *share > 1000 {
                return Err(PalwStateV2Error::InvalidParams("a class share must be 1..=1000 permille"));
            }
            sum += *share as u32;
        }
        if sum > 1000 {
            return Err(PalwStateV2Error::InvalidParams("class shares exceed the denominator"));
        }
        Ok(Self { shares_permille, max_factor })
    }

    pub fn share_permille(&self, class_id: &Hash64) -> Option<u16> {
        self.shares_permille.get(class_id).copied()
    }

    pub fn max_factor(&self) -> u32 {
        self.max_factor
    }

    /// Total allocation, for the startup gate's exactly-1000 rule (the constructor allows
    /// partial tables so tests can isolate one class; a live bundle does not get that latitude).
    pub fn shares_sum_permille(&self) -> u32 {
        self.shares_permille.values().map(|s| *s as u32).sum()
    }

    /// Every share-bearing class, in canonical order.
    pub fn class_ids(&self) -> Vec<Hash64> {
        self.shares_permille.keys().copied().collect()
    }
}

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
    /// ADR-0044 Decision 1's source split, as the retarget consumes it: the attempt lane's
    /// permille of combined production; the receipt lane gets the remainder. **1000 = no receipt
    /// lane** — the pure-attempt configuration, under which the two-lane retarget is byte-for-byte
    /// the single-lane rule (every pre-FP fixture passes 1000 and changes nothing). The FP
    /// bundle's startup gate additionally demands `0 < split < 1000` on a live FP network
    /// (a zero floor has no beacons; 1000 has no receipts).
    fp_attempt_share_permille: u16,
    /// The per-class DAA constants (see [`PalwClassDaaV2Params`]).
    class_daa: PalwClassDaaV2Params,
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
        fp_attempt_share_permille: u16,
        class_daa: PalwClassDaaV2Params,
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
            fp_attempt_share_permille,
            class_daa,
        })
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

    /// The attempt lane's permille of combined production (see the field's doc).
    pub fn fp_attempt_share_permille(&self) -> u16 {
        self.fp_attempt_share_permille
    }

    pub fn class_daa(&self) -> &PalwClassDaaV2Params {
        &self.class_daa
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
    /// Required at registration (Decision 7): splitting collateral across bonds must not
    /// manufacture extra panel seats.
    pub operator_id: Hash64,
    /// Slashable collateral in sompi. Value MOVEMENT (slash, withdrawal) is PR-07/PR-09; this
    /// records what the exposure ceiling is measured against.
    pub collateral: u64,
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
/// The real per-class PWU DERIVATION is ADR-0039's still-open item, and ADR-0042 requires it
/// before any class carries weight; until that record lands, the only shape a registration can
/// commit to is a ceiling. The enum exists so the derivation arrives as a new variant instead of
/// a schema break — and so admission can already refuse a claim no rule would ever license.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwPwuRuleV2 {
    /// An attempt may claim at most this many pwu (and at least 1, enforced statelessly).
    MaxPerAttempt(u64),
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
    /// Collateral value reserved at creation: `pwu × slash_value_per_pwu(class at creation)`.
    /// Snapshotted so release always returns exactly what reserve took, whatever happens to the
    /// class record afterwards.
    pub reserved: u128,
    /// `⌊β·pwu/1000⌋` at creation, for the same snapshot reason.
    pub immature_contribution: u128,
    pub phase: PalwClaimPhaseV2,
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
    BondRegistered {
        bond: PalwBondKeyV2,
        pubkey: Vec<u8>,
        operator_id: Hash64,
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
    ReceiptLicensed {
        claim: Hash64,
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
    #[error("per-class retarget failed: {0} — the closed span's facts must satisfy the rule or the block is invalid")]
    Retarget(String),
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
            receipt_targets: BTreeMap::new(),
            capabilities: BTreeMap::new(),
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

    /// The receipt lane's per-class target — what a quantum ticket is admitted against (FP-04).
    pub fn receipt_target(&self, id: &Hash64) -> Option<&PalwClassTargetV2> {
        self.receipt_targets.get(id)
    }

    pub fn receipt_epoch_counter(&self, class_id: &Hash64) -> Option<&PalwEpochCounterV2> {
        self.receipt_epoch_counters.get(class_id)
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
    /// the scalars. The exact ordering is frozen in ADR-0043 — extended in place (still pre-wire,
    /// nothing had committed a root) by ADR-0044 with the two receipt-lane collections, each
    /// placed directly after its attempt-lane counterpart, under the version bump to 2. Changing
    /// it again — or what any collection's entry encoding covers — is a consensus change and
    /// needs a new domain string or version.
    pub fn state_root(&self) -> Hash64 {
        let mut state = keyed(PALW_STATE_V2_DOMAIN_STATE_ROOT);
        state.update(&PALW_STATE_V2_VERSION.to_le_bytes());
        state.update(collection_root(b"bonds", &self.bonds).as_byte_slice());
        state.update(collection_root(b"reserved_exposure", &self.reserved_exposure).as_byte_slice());
        state.update(collection_root(b"classes", &self.classes).as_byte_slice());
        state.update(collection_root(b"class_targets", &self.class_targets).as_byte_slice());
        state.update(collection_root(b"receipt_targets", &self.receipt_targets).as_byte_slice());
        state.update(collection_root(b"capabilities", &self.capabilities).as_byte_slice());
        state.update(collection_root(b"claims", &self.claims).as_byte_slice());
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
    pub fn assert_internal_consistency(&self) -> Result<(), PalwStateV2Error> {
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
    ReceiptTarget { key: Hash64, old: Option<PalwClassTargetV2>, new: Option<PalwClassTargetV2> },
    Capability { key: Hash64, old: Option<PalwCapabilityStateV2>, new: Option<PalwCapabilityStateV2> },
    Claim { key: Hash64, old: Option<PalwClaimStateV2>, new: Option<PalwClaimStateV2> },
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

    fn finalize_claim(&mut self, id: Hash64, claim: &PalwClaimStateV2, final_daa: u64) -> Result<(), PalwStateV2Error> {
        self.release_for_claim(claim)?;
        // The weight divergence between the lanes (ADR-0044): an attempt's Final IS its block's
        // certified work; a free-prompt Final only LICENSES — its weight arrives per spent
        // quantum, at the receipt block that spends it.
        if matches!(claim.source, PalwClaimSourceV2::Attempt) {
            self.state.safe_weight =
                self.state.safe_weight.checked_add(claim.pwu as u128).ok_or(PalwStateV2Error::Overflow("safe_weight"))?;
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

    // 4. The block's own work — an attempt, a certified-quantum spend, or none.
    match block_work {
        PalwBlockWorkV3::None => {}
        PalwBlockWorkV3::Attempt(envelope) => apply_attempt(&mut builder, ctx, envelope)?,
        PalwBlockWorkV3::ReceiptSpend(spend) => apply_receipt_spend(&mut builder, ctx, spend)?,
    }

    // 5. Frontier observation: if the whole past is resolved at this point, the frontier is here.
    let old_frontier = (builder.state.safe_frontier_blue_score, builder.state.safe_frontier);
    if builder.state.unresolved.is_empty() {
        builder.state.safe_frontier_blue_score = ctx.blue_score;
        builder.state.safe_frontier = ctx.block;
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
    for class_id in class_ids {
        let class = builder.state.classes.get(&class_id).expect("iterating the map's own keys");
        if matches!(class.status, PalwClassStatusV2::Frozen { .. }) {
            continue;
        }
        let Some(share) = builder.params.class_daa.share_permille(&class_id) else { continue };
        for (lane_permille, counters_are_receipts) in [(split, false), (1000 - split, true)] {
            let composed_share = compose(share, lane_permille);
            if composed_share == 0 {
                // A lane whose composed share rounds to nothing measures nothing — the split
                // itself said so. The FP bundle's startup gate refuses splits that zero a
                // share-bearing class's lane (FP-05), so on a live network this arm is the
                // split = 1000 receipt lane and nothing else.
                continue;
            }
            let counters = if counters_are_receipts { &builder.state.receipt_epoch_counters } else { &builder.state.epoch_counters };
            let observed = counters
                .get(&class_id)
                .filter(|counter| counter.epoch_index == closed_epoch)
                .map(|counter| counter.produced_blocks)
                .unwrap_or(0);
            let targets = if counters_are_receipts { &builder.state.receipt_targets } else { &builder.state.class_targets };
            let lane = if counters_are_receipts { "receipt" } else { "attempt" };
            let current = targets
                .get(&class_id)
                .ok_or_else(|| PalwStateV2Error::Retarget(format!("class {class_id} has no {lane} target slot")))?
                .target;
            let census = crate::palw_class_daa::PalwClassSpanCensusV1 { class_daa_blocks: observed, total_daa_blocks: combined };
            let next =
                crate::palw_class_daa::retarget_over_span_v1(current, &census, composed_share, builder.params.class_daa.max_factor())
                    .map_err(|e| PalwStateV2Error::Retarget(e.to_string()))?;
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
        PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_id, collateral } => {
            if builder.state.bonds.contains_key(bond) {
                return Err(PalwStateV2Error::DuplicateBond(*bond));
            }
            builder.write_bond(
                *bond,
                Some(PalwBondStateV2 {
                    pubkey: pubkey.clone(),
                    operator_id: *operator_id,
                    collateral: *collateral,
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
        PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, slash_value_per_pwu, pwu_rule, initial_target } => {
            if builder.state.classes.contains_key(class_id) {
                return Err(PalwStateV2Error::DuplicateClass(*class_id));
            }
            if *initial_target == 0 {
                return Err(PalwStateV2Error::ZeroClassTarget(*class_id));
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
            // The receipt lane seeds from the same initial target (ADR-0044): the two lanes'
            // retargets separate them from here, against their own censuses. One registration
            // field, two slots — a second declared number would be a second fact to drift.
            builder.write_receipt_target(*class_id, Some(PalwClassTargetV2 { target: *initial_target }));
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
        PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id } => {
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            let PalwClaimPhaseV2::PanelBound { .. } = claim.phase else {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "ReceiptLicensed" });
            };
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
                        builder.void_claim(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::CourtFraud)?;
                    }
                }
                PalwCourtVerdictV2::ChallengerDefeated => {
                    rearm_after_challenger_side_close(builder, ctx, claim_id, &claim)?;
                }
            }
        }
        PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id } => {
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            if claim.phase.is_terminal() {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "ProducerDefaulted" });
            }
            builder.void_claim(*claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ProducerWithholding)?;
        }
        PalwConsensusObjectV2::FreePromptCommitted { claim: claim_id, class_id, bond, pwu, quanta, trace_root, output_root } => {
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
                reserved,
                // Zero, deliberately (ADR-0044): a commitment riding a transaction is not a
                // block's work — β credit here would let commitment-stuffing pump a chain's live
                // weight without producing anything.
                immature_contribution: 0,
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
        PalwDeltaEntryV2::ReceiptTarget { key, old, new } => swap_write!(state.receipt_targets, key, old, new),
        PalwDeltaEntryV2::Capability { key, old, new } => swap_write!(state.capabilities, key, old, new),
        PalwDeltaEntryV2::Claim { key, old, new } => swap_write!(state.claims, key, old, new),
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
    pub receipt_targets: BTreeMap<Hash64, PalwClassTargetV2>,
    pub capabilities: BTreeMap<Hash64, PalwCapabilityStateV2>,
    pub claims: BTreeMap<Hash64, PalwClaimStateV2>,
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
            receipt_targets: state.receipt_targets.clone(),
            capabilities: state.capabilities.clone(),
            claims: state.claims.clone(),
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
            receipt_targets: self.receipt_targets,
            capabilities: self.capabilities,
            claims: self.claims,
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
mod tests {
    use super::*;
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptUnsignedV2, challenge_v2};
    use crate::palw_fork_choice::compare_palw_candidates_v1;
    use crate::tx::TransactionId;

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, 1000, class_daa()).unwrap()
    }

    fn class_daa() -> PalwClassDaaV2Params {
        PalwClassDaaV2Params::new([(h64(1), 1000u16)].into_iter().collect(), 4).unwrap()
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
            },
            PalwConsensusObjectV2::BondRegistered { bond: bond_key(1), pubkey: vec![7; 4], operator_id: h64(21), collateral: 1_000 },
        ]
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
                operator_id: h64(21),
                artifact_root: h64(11),
                trace_root: h64(31),
                output_root: h64(32),
                pwu,
                trace_manifest_root: h64(33),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
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

        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id }], None);
        assert!(matches!(s4.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        assert_eq!((s4.safe_weight(), s4.bounded_immature()), (0, 4), "licensing moves no weight");

        // The challenge window (20) from licensing at daa 103 ends at 123; the first block past
        // it sweeps the claim Final.
        let (s5, _) = apply(&s4, &p, &ctx(5, 124, 5), &[], None);
        assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        assert_eq!((s5.safe_weight(), s5.bounded_immature()), (40, 0), "Final: full pwu safe, immature released");
        assert_eq!(s5.reserved_exposure(&bond_key(1)), 0, "exposure released on Final");
        assert_eq!(s5.safe_frontier(), (5, block(5)), "with the whole past resolved, the frontier is here");
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
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id }], None);
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
        assert_eq!(s3.safe_frontier(), (3, block(3)), "voiding resolves the past too");
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
        let (s3, _) = apply(&s2, &p, &ctx(3, 102, 3), &[PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id }], None);
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
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id }], None);
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
                PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id },
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
        let (s4, _) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id }], None);
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
            apply_palw_transition_v2(&genesis, &p, &c, &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(1) }], None),
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
        let (m3, _) = apply(&m2, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id }], None);
        let (matured, _) = apply(&m3, &p, &ctx(5, 124, 5), &[], None);

        // Chain P: three heavier claims, none ever bound — a pile. (Blocks close enough together
        // that no bind deadline passes, so the pile keeps its immature weight.)
        let (p1, _) = apply(&base, &p, &ctx(12, 101, 2), &[], Some(&attempt(1000, 11)));
        let (p2, _) = apply(&p1, &p, &ctx(13, 102, 3), &[], Some(&attempt(1000, 12)));
        let (pile, _) = apply(&p2, &p, &ctx(14, 103, 4), &[], Some(&attempt(1000, 13)));

        assert_eq!(matured.safe_frontier().0, 5, "the matured chain's frontier reached its tip");
        assert_eq!(pile.safe_frontier().0, 1, "the pile's frontier is stuck at the last fully-resolved point");
        assert!(pile.bounded_immature() > matured.safe_weight(), "the pile really is heavier in raw immature weight");

        let matured_order = matured.candidate_order(block(5));
        let pile_order = pile.candidate_order(block(14));
        assert_eq!(
            compare_palw_candidates_v1(&matured_order, &pile_order),
            std::cmp::Ordering::Greater,
            "the matured chain outranks the heavier unproven pile"
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
            &[PalwConsensusObjectV2::BondRegistered { bond: bond_key(2), pubkey: vec![8], operator_id: h64(22), collateral: 5 }],
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
            (4, 103, 4, vec![PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id }], None),
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
        // boundary. Class 1 hardens; class 2 does not move.
        let shares = PalwClassDaaV2Params::new([(h64(1), 500u16), (h64(2), 500u16)].into_iter().collect(), 4).unwrap();
        let p_half = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, 1000, shares).unwrap();
        let mut objects = register_class_and_bond();
        objects.push(PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            artifact_root: h64(12),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: boot,
        });
        objects.push(PalwConsensusObjectV2::ClassFrozen { class_id: h64(2) });
        let (s1, _) = apply(&genesis, &p_half, &ctx(1, 100, 1), &objects, None);
        let (s2, _) = apply(&s1, &p_half, &ctx(2, 101, 2), &[], Some(&attempt(40, 1)));
        let (s3, _) = apply(&s2, &p_half, &ctx(3, 102, 3), &[], Some(&attempt(40, 2)));
        let (s4, _) = apply(&s3, &p_half, &ctx(4, 1_001, 4), &[], None);
        assert!(s4.class_target(&h64(1)).unwrap().target < boot, "500‰ producing 100% of the span hardens");
        assert_eq!(s4.class_target(&h64(2)).unwrap().target, boot, "a frozen class's target freezes with it");

        // Case 3: a boundary crossed with nothing produced (the counters are stale after case
        // 2's crossing) measures nothing and moves nothing.
        let hardened = s4.class_target(&h64(1)).unwrap().target;
        let (s5, _) = apply(&s4, &p_half, &ctx(5, 2_001, 5), &[], None);
        assert_eq!(s5.class_target(&h64(1)).unwrap().target, hardened, "an empty epoch measures nothing");
    }

    #[test]
    fn class_daa_params_refuse_broken_tables() {
        assert!(PalwClassDaaV2Params::new([(h64(1), 0u16)].into_iter().collect(), 4).is_err(), "zero share");
        assert!(PalwClassDaaV2Params::new([(h64(1), 1001u16)].into_iter().collect(), 4).is_err(), "share above 1000");
        assert!(
            PalwClassDaaV2Params::new([(h64(1), 600u16), (h64(2), 600u16)].into_iter().collect(), 4).is_err(),
            "shares exceeding the denominator"
        );
        assert!(PalwClassDaaV2Params::new([(h64(1), 1000u16)].into_iter().collect(), 1).is_err(), "max_factor below 2");
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
            }],
            None,
        );
        assert!(matches!(err, Err(PalwStateV2Error::ZeroClassTarget(_))));
    }

    // ---- params ----

    #[test]
    fn params_refuse_out_of_range_values() {
        assert!(PalwStateParamsV2::new(1001, 1, 1, 1, 1, 1, 1000, class_daa()).is_err(), "β > 1");
        assert!(PalwStateParamsV2::new(100, 0, 1, 1, 1, 1, 1000, class_daa()).is_err(), "zero bind window");
        assert!(PalwStateParamsV2::new(100, 1, 0, 1, 1, 1, 1000, class_daa()).is_err(), "zero receipt window");
        assert!(PalwStateParamsV2::new(100, 1, 1, 0, 1, 1, 1000, class_daa()).is_err(), "zero challenge window");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 0, 1, 1000, class_daa()).is_err(), "zero court window");
        assert!(PalwStateParamsV2::new(100, 1, 1, 1, 1, 0, 1000, class_daa()).is_err(), "zero epoch length");
        assert!(PalwStateParamsV2::new(1000, 1, 1, 1, 1, 1, 1000, class_daa()).is_ok(), "β = 1 exactly is the boundary");
    }

    #[test]
    fn the_beta_rounding_is_floor_and_only_floor() {
        let p = PalwStateParamsV2::new(333, 10, 10, 10, 10, 10, 1000, class_daa()).unwrap();
        assert_eq!(immature_contribution_v2(&p, 10), 3, "⌊10·333/1000⌋ = 3, never 4");
        assert_eq!(immature_contribution_v2(&p, 1), 0, "⌊1·333/1000⌋ = 0: a tiny claim may contribute nothing");
        assert_eq!(immature_contribution_v2(&p, 3), 0);
        let full = PalwStateParamsV2::new(1000, 10, 10, 10, 10, 10, 1000, class_daa()).unwrap();
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
        }
    }

    fn fp_spend(claim_word: u64, quantum_index: u32) -> PalwReceiptSpendUnsignedV3 {
        PalwReceiptSpendUnsignedV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: h64(999),
            claim_id: h64(claim_word),
            quantum_index,
            beacon_block: h64(0xBEAC),
            producer_bond: bond_key(1).0,
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
        state.assert_internal_consistency().expect("internal consistency after apply");
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
        let (s4, _) = apply(&s3, p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC) }], None);
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
        assert_eq!(certified.safe_frontier(), (5, block(5)), "an unspent certified receipt does not block the frontier");

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
        let split = PalwStateParamsV2::new(100, 60, 60, 20, 500, 100, 800, class_daa()).unwrap();
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
        let (s5, _) = apply(&s4, &split, &ctx(5, 113, 5), &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC) }], None);
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
        let pure = PalwStateParamsV2::new(100, 60, 60, 20, 500, 100, 1000, class_daa()).unwrap();
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
        let (s4, d4) = apply(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC) }], None);
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
        reorged.assert_internal_consistency().unwrap();
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
            book.apply_block(block(3), ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC) }], None).unwrap();
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
