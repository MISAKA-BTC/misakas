//! **The history dissection as a court phase — ADR-0082 Decision 2, and the window gate its
//! arity has to clear (Z4).**
//!
//! [`crate::palw_attn_dissect`] is the ARITHMETIC: the range claim, the fold, the pinned cut, the
//! round bound, a move's bytes. This module is the PROTOCOL: who moves, in what order, against
//! what clock, and what ends it. It is the second half of the sentence the contracts module ends
//! with — "the court arm that plays a round … is ADR-0082 U-03's".
//!
//! # The phase, and why it is a phase
//!
//! A dispute over a fused attention leaf reaches this module the way every other dispute reaches
//! its terminal: the ladder narrows to one leaf and hands off. For every other op the handoff is
//! `check_execution_step_refutation_v1` — one recompute, one comparison. For `AttnFused` the
//! leaf's neighbourhood is the whole history (ADR-0082 §1.7), so the handoff is a short
//! interactive protocol instead:
//!
//! ```text
//!   ladder Terminal ──▶ root claim (m*, S*, V*)  ─── checked against the OPENED output tile
//!                            │                       with a16_attn_finalize_v1, before a round
//!                            ▼
//!                       AwaitDisclosure ──▶ k range claims, fold-checked ──▶ AwaitVerdict
//!                            ▲                                                    │
//!                            └──────── the challenger names a child ──────────────┘
//!                            ▼
//!                       Terminal (one tile) ──▶ the bottom: open q, K, V; recompute; compare
//! ```
//!
//! It is ONE phase of the existing court session and not a second session: the same
//! `session_id`, the same parties, the same `PalwBisectTurnV1` vocabulary — so
//! `court_next_deadline_v2` indexes it with the arithmetic it already has — and the same rule at
//! every move: **silence past the deadline is the objective offense**, charged to whoever was due.
//! The one asymmetry is the one the shipped court already documents: at `Terminal` the move is a
//! CLOSE, which the accused has no reason to file against itself, so the rung clock stops and the
//! whole-session backstop ends it on the challenger's side.
//!
//! # What convicts
//!
//! * A disclosure that does not FOLD to the parent's claim is a conviction, by the named field
//!   (`palw_attn_fold_check_v1` returns which of `max`, `exp_sum` or a `v_acc` lane disagreed).
//!   The responder is claiming a decomposition of its own claim; a decomposition that does not
//!   recompose is a self-contradiction, and no execution is needed to see it.
//! * A root claim whose `V*` does not finalize to the opened output tile is refused before a
//!   round is played — it is not about the execution under dispute.
//! * At the bottom, the recompute either reproduces the child's triple or it does not. Every one
//!   of the three fields is compared, because each catches a different lie: `max` catches a
//!   forged `m*`, `exp_sum` a forged `S*`, `v_acc` a forged output.
//!
//! And what ACQUITS is the same machinery with the comparison passing — an honest responder walks
//! every round, is convicted by no fold, and the bottom's recompute agrees with its claim.
//!
//! # Nothing here is armed
//!
//! The phase is admissible only under `Params::palw_kary_court`, `None` on every shipped preset;
//! a caller that reaches it without the fence is refused BY NAME
//! ([`PalwAttnCourtError::FenceDormant`]). Consensus-inert until the object arms land.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::palw_attn_dissect::{
    PALW_ATTN_DISSECT_MAX_LANES, PalwAttnDissectError, PalwAttnDissectRoundV1, PalwAttnRangeClaimV1, PalwAttnRootClaimV1,
    palw_attn_arity_is_legal_v1, palw_attn_child_ranges_v1, palw_attn_dissect_move_bytes_v1, palw_attn_dissection_rounds_v1,
    palw_attn_fold_check_v1,
};
use crate::palw_base0_a16::{A16AttnFusedParamsV1, PalwA16OpError, a16_attn_finalize_v1, a16_attn_tile_triple_v1};
use crate::palw_bisect::{PalwBisectNoShowV1, PalwBisectPartyV1, PalwBisectTurnV1};
use crate::palw_mode_v2::PalwCourtParamsV2;
use crate::palw_step_leg::{PalwStepLegError, PalwStepOpeningV1, PalwStepTileLeafV1, step_opening_root_capped_v1, step_tile_leaf_hash_v1};

/// Wire version of every object in this module.
pub const PALW_ATTN_COURT_OBJECT_VERSION_V1: u16 = 1;

// =================================================================================================
// Refusals
// =================================================================================================

/// Why a dissection move is refused, or what it convicted on. Total, and every arm names the
/// quantity — a court finding that reads "assertion failed" is a finding nobody can sequence.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwAttnCourtError {
    #[error("the k-ary court fence is dormant on this network: a fused-attention dissection is not admissible")]
    FenceDormant,
    #[error("unsupported dissection object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("the message does not belong to this dissection")]
    SessionMismatch,
    #[error("the message is for round {got}, but the dissection is at round {expected}")]
    RoundMismatch { got: u32, expected: u32 },
    #[error("a {expected} move is required next, got {got}")]
    TurnMismatch { expected: &'static str, got: &'static str },
    #[error("the dissection is terminal or abandoned; no further round is legal")]
    AlreadyTerminal,
    #[error("w_round is zero — every move would be instantly overdue")]
    ZeroRungWindow,
    #[error("no-show can only be declared after the deadline ({deadline}); observed DAA is {observed}")]
    DeadlineNotReached { deadline: u64, observed: u64 },
    #[error("round budget exceeded — a legal dissection reaches one tile within {bound} rounds")]
    RoundBudgetExceeded { bound: u32 },
    #[error("the round discloses {got} children; this range has {expected}")]
    ChildCountMismatch { got: usize, expected: usize },
    #[error("the verdict names child {got} of {children}")]
    ChildOutOfRange { got: u8, children: usize },
    #[error("the dissection's arithmetic refused the move: {0}")]
    Dissect(#[from] PalwAttnDissectError),
    // `PalwA16OpError` carries no `Display` (it is a kernel-side enum), so it is shown by its
    // Debug form rather than given a second spelling of its message here.
    #[error("the recompute refused its operands: {0:?}")]
    Kernel(PalwA16OpError),
    #[error("an opening did not verify: {0}")]
    Leg(#[from] PalwStepLegError),
    #[error("a root claim of {history_positions} positions at {lanes} lanes is outside the court's bounds")]
    RootClaimOutOfRange { history_positions: u32, lanes: usize },
    #[error("the root claim's value partials finalize to a tile the execution did not commit")]
    RootDoesNotFinalize,
    #[error("the bottom opens tile {got} and the dissection narrowed to tile {expected}")]
    WrongTile { got: u64, expected: u64 },
    #[error("the bottom opens {got} positions; tile {tile} of a {history_positions}-position history has {expected}")]
    WrongTileWidth { got: usize, expected: usize, tile: u64, history_positions: u32 },
    #[error("an opened row hashes to a leaf the claim's step tree does not contain")]
    RowNotCommitted,
    #[error("an opened row carries {got} lanes where the geometry says {expected}")]
    RowWidthMismatch { got: usize, expected: usize },
    #[error("this court admits no arity that fits its window: the widest row needs more moves than {window_court} DAA buys")]
    NoAdmissibleArity { window_court: u64 },
    #[error(
        "the dissection does not fit the court window: {moves} moves x {deadline} DAA + {reserve} reserve against window_court {window_court}"
    )]
    OverrunsWindow { moves: u64, deadline: u64, reserve: u64, window_court: u64 },
}

impl From<PalwA16OpError> for PalwAttnCourtError {
    fn from(e: PalwA16OpError) -> Self {
        Self::Kernel(e)
    }
}

/// What a completed dissection decided. The same two outcomes the shipped court has, named here
/// so an arm can map them without depending on the state module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwAttnCourtVerdictV1 {
    /// The recompute contradicted the responder's claim, or a disclosure failed to fold.
    ExecutorGuilty,
    /// The recompute reproduced the claim: the challenge failed on the merits.
    ChallengerDefeated,
}

// =================================================================================================
// Wire objects
// =================================================================================================

/// The challenger's move: the INDEX of the child range it disputes, into
/// [`palw_attn_child_ranges_v1`]'s list. One byte at every arity — a challenger names a child, it
/// never names a range, so there is nothing to disagree about but the number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnDissectChoiceV1 {
    pub version: u16,
    pub session_id: Hash64,
    pub round: u32,
    pub child: u8,
}

/// One opened committed row: the leaf preimage and its membership proof in the claim's step tree.
///
/// The preimage is the leaf the executor committed, so the lane values are read back out of it
/// rather than carried beside it — a second copy of the same numbers is a second thing to
/// disagree with.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnRowOpeningV1 {
    pub leaf: PalwStepTileLeafV1,
    pub opening: PalwStepOpeningV1,
}

/// **The bottom of the dissection**: the head's query slice, the tile's K and V rows, and the
/// output tile the root claim was checked against — every one of them an opening against the
/// claim's own committed step root.
///
/// # What this carries and what it does not
///
/// It carries ONE tile: `PALW_ATTN_HISTORY_TILE_V4` positions of K and of V for the disputed
/// head's slice, the query row, and the paths that prove them. That is flat in the context, which
/// is the whole of ADR-0082 R4 at this site. It does NOT carry the history, the probability row,
/// the score row, or any checkpoint chunk list.
///
/// # The route that is here, and the one that is not
///
/// ADR-0082 Decision 2 names two ways to reach a tile's K and V rows: the cache-write leaves of
/// the step tree (this object), and the graph-v4 checkpoint chunk at or before the tile. Only the
/// first is implemented, and deliberately: `PalwCheckpointKvOperandsV1` carries `chunks:
/// Vec<Vec<u8>>` — **every** chunk of the checkpoint — so the checkpoint route as it exists today
/// would carry the whole history to open one tile, which is the thing this ADR exists to stop.
/// Tiling the MAP (`tiled_kv_state_geometry_v3`) made the chunk a tile; it did not make a chunk
/// individually openable, and the missing primitive is a membership proof into
/// `state_chunks_root_v1`'s tree. Until that exists the bottom uses the step tree, which already
/// has openings, and the refusal is honest rather than silent.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnDissectBottomV1 {
    pub version: u16,
    pub session_id: Hash64,
    /// The tile index the dissection narrowed to — checked against the phase, never trusted.
    pub tile: u64,
    /// The head's rotated query slice: `d_head` codes.
    pub query: PalwAttnRowOpeningV1,
    /// The tile's K rows, position-major, one opening per position.
    pub k_rows: Vec<PalwAttnRowOpeningV1>,
    /// The tile's V rows, in the same order.
    pub v_rows: Vec<PalwAttnRowOpeningV1>,
    /// The committed output tile the root claim finalizes to.
    pub out_tile: PalwAttnRowOpeningV1,
}

/// What the bottom's openings are verified against: the claim's committed step root, the leaf
/// count that root was built over, and the two hashes the leaf preimage is bound to.
///
/// Taken as a struct rather than read from chain state so this module stays a pure checker — the
/// same discipline `palw_bisect` states about deadlines ("this machine takes numbers, it does not
/// derive them").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAttnBottomBindingV1 {
    pub job_context_hash: Hash64,
    pub shape_profile_hash: Hash64,
    pub step_root: Hash64,
    pub step_leaf_count: u64,
    pub max_step_leaf_count: u64,
}

// =================================================================================================
// The phase
// =================================================================================================

/// **One court session's dissection phase** (ADR-0082 Decision 2).
///
/// Entered at the ladder's `Terminal` when the leaf is an `AttnFused` site, carried in the session
/// record beside the ladder, and read by the same sweep: `turn()` says who owes a move and
/// `last_deadline_daa()` says by when, which is exactly the pair `court_next_deadline_v2`
/// consumes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnDissectPhaseV1 {
    session_id: Hash64,
    /// The dissection's arity — the court's, frozen at the phase's opening so a ruleset move
    /// mid-dispute cannot change the children both parties already derived.
    arity: u8,
    /// Positions a history tile holds (`PALW_ATTN_HISTORY_TILE_V4` for the graph-v4 map).
    tile_positions: u32,
    /// The head under dispute and the lanes of its output tile.
    head: u16,
    lane_first: u16,
    lane_count: u16,
    /// The history the site reads at the disputed position.
    history_positions: u32,
    /// The root's `(m*, S*)` — used at EVERY level, which is what makes the levels comparable.
    m_star: i32,
    s_star: i64,
    /// The claim of the range currently disputed: the root's at round 0, thereafter the child the
    /// challenger named.
    claim: PalwAttnRangeClaimV1,
    /// The disputed range, in TILES.
    tile_first: u64,
    tile_count: u64,
    round: u32,
    turn: PalwBisectTurnV1,
    last_deadline_daa: u64,
    /// The children the current round disclosed, awaiting the challenger's index.
    pending: Vec<PalwAttnRangeClaimV1>,
}

impl PalwAttnDissectPhaseV1 {
    /// **Open the phase from the responder's root claim** (ADR-0082 Decision 2, step 1).
    ///
    /// `out_tile` is the OPENED committed output tile of the fused node — the lanes the claim is
    /// about. The claim is admitted only if `a16_attn_finalize_v1(V*)` reproduces it exactly: a
    /// root claim that finalizes to something else is not a claim about this execution, and
    /// playing rounds against it would narrow toward a divergence the claim invented.
    ///
    /// `kary_court_active` is `Params::palw_kary_court_active_at(daa)`. It is an ARGUMENT because
    /// the fence is the caller's to read; what this module owns is refusing to run without it.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        session_id: Hash64,
        root: &PalwAttnRootClaimV1,
        out_tile: &[i32],
        values: crate::palw_base0_a16::A16QuantParams,
        court: &PalwCourtParamsV2,
        tile_positions: u32,
        opened_at_daa: u64,
        w_round: u64,
        kary_court_active: bool,
    ) -> Result<Self, PalwAttnCourtError> {
        if !kary_court_active {
            return Err(PalwAttnCourtError::FenceDormant);
        }
        if root.version != crate::palw_attn_dissect::PALW_ATTN_DISSECT_OBJECT_VERSION_V1 {
            return Err(PalwAttnCourtError::UnsupportedVersion {
                got: root.version,
                expected: crate::palw_attn_dissect::PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            });
        }
        let lanes = root.claim.v_acc.len();
        if lanes == 0
            || lanes > PALW_ATTN_DISSECT_MAX_LANES
            || lanes != root.lane_count as usize
            || root.history_positions == 0
            || tile_positions == 0
        {
            return Err(PalwAttnCourtError::RootClaimOutOfRange { history_positions: root.history_positions, lanes });
        }
        if w_round == 0 {
            return Err(PalwAttnCourtError::ZeroRungWindow);
        }
        // **The claim is checked against the execution before it is played against.**
        if out_tile.len() != lanes || a16_attn_finalize_v1(&root.claim.v_acc, values) != out_tile {
            return Err(PalwAttnCourtError::RootDoesNotFinalize);
        }
        let tile_count = (root.history_positions as u64).div_ceil(tile_positions as u64);
        let arity = court.dissection_arity();
        if !palw_attn_arity_is_legal_v1(arity) {
            return Err(PalwAttnCourtError::Dissect(PalwAttnDissectError::ArityOutOfRange { got: arity }));
        }
        Ok(Self {
            session_id,
            arity,
            tile_positions,
            head: root.head,
            lane_first: root.lane_first,
            lane_count: root.lane_count,
            history_positions: root.history_positions,
            m_star: root.claim.max,
            s_star: root.claim.exp_sum,
            claim: root.claim.clone(),
            tile_first: 0,
            tile_count,
            round: 0,
            turn: if tile_count <= 1 { PalwBisectTurnV1::Terminal } else { PalwBisectTurnV1::AwaitDisclosure },
            last_deadline_daa: opened_at_daa.saturating_add(w_round),
            pending: Vec::new(),
        })
    }

    pub fn session_id(&self) -> Hash64 {
        self.session_id
    }
    pub fn turn(&self) -> PalwBisectTurnV1 {
        self.turn
    }
    pub fn round(&self) -> u32 {
        self.round
    }
    pub fn arity(&self) -> u8 {
        self.arity
    }
    pub fn last_deadline_daa(&self) -> u64 {
        self.last_deadline_daa
    }
    /// The claim the current range stands behind.
    pub fn claim(&self) -> &PalwAttnRangeClaimV1 {
        &self.claim
    }
    /// The disputed range in TILES: `(first, count)`.
    pub fn tile_range(&self) -> (u64, u64) {
        (self.tile_first, self.tile_count)
    }
    /// The root's `(m*, S*)`, which every level is computed against.
    pub fn root_scale(&self) -> (i32, i64) {
        (self.m_star, self.s_star)
    }

    /// The rounds this dissection is allowed to take — the contract's own recurrence over the
    /// tile count, so the bound and the cut can never disagree.
    pub fn round_budget(&self) -> u32 {
        palw_attn_dissection_rounds_v1(self.history_positions as u64, self.tile_positions, self.arity).unwrap_or(u32::MAX)
    }

    /// The pinned children of the disputed range: `(first_tile, tile_count)`, in order.
    pub fn child_ranges(&self) -> Vec<(u64, u64)> {
        palw_attn_child_ranges_v1(self.tile_first, self.tile_count, self.arity).unwrap_or_default()
    }

    /// The tile the dissection has narrowed to, once it has: `Some` only at `Terminal`.
    pub fn terminal_tile(&self) -> Option<u64> {
        (self.turn == PalwBisectTurnV1::Terminal && self.tile_count == 1).then_some(self.tile_first)
    }

    /// The positions the terminal tile covers — ragged at the end of the history, which is the
    /// case a dissection meets on every context that is not a multiple of the tile.
    pub fn terminal_tile_positions(&self) -> Option<(u64, usize)> {
        let tile = self.terminal_tile()?;
        let first = tile.checked_mul(self.tile_positions as u64)?;
        let width = (self.history_positions as u64).saturating_sub(first).min(self.tile_positions as u64);
        (width > 0).then_some((first, width as usize))
    }

    /// **The responder's round: the claims of the `k` children of the disputed range.**
    ///
    /// Fold-checked BEFORE the challenger moves ([`palw_attn_fold_check_v1`]). A disclosure that
    /// does not fold is returned as the fold's own error, naming the field and both values — that
    /// is the conviction, and the caller mints it rather than this machine, for the same reason
    /// `palw_bisect` does not mint its own no-show.
    pub fn apply_round(&mut self, msg: &PalwAttnDissectRoundV1, accepted_daa: u64, w_round: u64) -> Result<(), PalwAttnCourtError> {
        if msg.version != crate::palw_attn_dissect::PALW_ATTN_DISSECT_OBJECT_VERSION_V1 {
            return Err(PalwAttnCourtError::UnsupportedVersion {
                got: msg.version,
                expected: crate::palw_attn_dissect::PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            });
        }
        match self.turn {
            PalwBisectTurnV1::AwaitDisclosure => {}
            PalwBisectTurnV1::AwaitVerdict => return Err(PalwAttnCourtError::TurnMismatch { expected: "choice", got: "round" }),
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => return Err(PalwAttnCourtError::AlreadyTerminal),
        }
        if w_round == 0 {
            return Err(PalwAttnCourtError::ZeroRungWindow);
        }
        let expected = self.child_ranges();
        if msg.children.len() != expected.len() {
            return Err(PalwAttnCourtError::ChildCountMismatch { got: msg.children.len(), expected: expected.len() });
        }
        // The fold is the check, and it runs before anything moves.
        palw_attn_fold_check_v1(&self.claim, &msg.children)?;
        self.pending = msg.children.clone();
        self.last_deadline_daa = accepted_daa.saturating_add(w_round);
        self.turn = PalwBisectTurnV1::AwaitVerdict;
        Ok(())
    }

    /// **The challenger's move: the child it disagrees with.** The named child's claim becomes the
    /// disputed claim and its range the disputed range; one tile is the bottom.
    pub fn apply_choice(&mut self, msg: &PalwAttnDissectChoiceV1, accepted_daa: u64, w_round: u64) -> Result<(), PalwAttnCourtError> {
        if msg.version != PALW_ATTN_COURT_OBJECT_VERSION_V1 {
            return Err(PalwAttnCourtError::UnsupportedVersion { got: msg.version, expected: PALW_ATTN_COURT_OBJECT_VERSION_V1 });
        }
        if msg.session_id != self.session_id {
            return Err(PalwAttnCourtError::SessionMismatch);
        }
        match self.turn {
            PalwBisectTurnV1::AwaitVerdict => {}
            PalwBisectTurnV1::AwaitDisclosure => return Err(PalwAttnCourtError::TurnMismatch { expected: "round", got: "choice" }),
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => return Err(PalwAttnCourtError::AlreadyTerminal),
        }
        if msg.round != self.round {
            return Err(PalwAttnCourtError::RoundMismatch { got: msg.round, expected: self.round });
        }
        if w_round == 0 {
            return Err(PalwAttnCourtError::ZeroRungWindow);
        }
        // Everything that can refuse is decided before a field moves.
        let bound = self.round_budget();
        let next_round = self.round + 1;
        if next_round > bound {
            return Err(PalwAttnCourtError::RoundBudgetExceeded { bound });
        }
        let ranges = self.child_ranges();
        let idx = msg.child as usize;
        let Some(&(first, count)) = ranges.get(idx) else {
            return Err(PalwAttnCourtError::ChildOutOfRange { got: msg.child, children: ranges.len() });
        };
        let Some(claim) = self.pending.get(idx).cloned() else {
            return Err(PalwAttnCourtError::TurnMismatch { expected: "a round before the choice", got: "none" });
        };
        self.claim = claim;
        self.tile_first = first;
        self.tile_count = count;
        self.round = next_round;
        self.pending = Vec::new();
        self.last_deadline_daa = accepted_daa.saturating_add(w_round);
        self.turn = if count <= 1 { PalwBisectTurnV1::Terminal } else { PalwBisectTurnV1::AwaitDisclosure };
        Ok(())
    }

    /// Silence past a move's deadline, with the shipped court's rule verbatim: at `Terminal` the
    /// move is a close the accused has no reason to file, so the responder is not charged there —
    /// the backstop ends it on the challenger's side.
    pub fn declare_no_show(&mut self, observed_daa: u64) -> Result<PalwBisectNoShowV1, PalwAttnCourtError> {
        if observed_daa <= self.last_deadline_daa {
            return Err(PalwAttnCourtError::DeadlineNotReached { deadline: self.last_deadline_daa, observed: observed_daa });
        }
        let silent_party = match self.turn {
            PalwBisectTurnV1::AwaitDisclosure => PalwBisectPartyV1::Responder,
            PalwBisectTurnV1::AwaitVerdict => PalwBisectPartyV1::Challenger,
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => return Err(PalwAttnCourtError::AlreadyTerminal),
        };
        self.turn = PalwBisectTurnV1::Abandoned;
        Ok(PalwBisectNoShowV1 {
            version: crate::palw_bisect::PALW_BISECT_OBJECT_VERSION_V1,
            session_id: self.session_id,
            round: self.round,
            silent_party,
            deadline_daa: self.last_deadline_daa,
            observed_daa,
        })
    }
}

// =================================================================================================
// The bottom
// =================================================================================================

/// Read an opened row's `i32` lanes out of its leaf preimage, after proving the leaf is the
/// claim's. One function for every row the bottom opens, so "opened" means the same thing four
/// times.
fn opened_lanes_v1(
    row: &PalwAttnRowOpeningV1,
    binding: &PalwAttnBottomBindingV1,
    expected_lanes: usize,
) -> Result<Vec<i32>, PalwAttnCourtError> {
    if step_tile_leaf_hash_v1(&binding.job_context_hash, &binding.shape_profile_hash, &row.leaf) != row.opening.leaf_hash {
        return Err(PalwAttnCourtError::RowNotCommitted);
    }
    let root = step_opening_root_capped_v1(binding.step_leaf_count, &row.opening, binding.max_step_leaf_count)?;
    if root != binding.step_root {
        return Err(PalwAttnCourtError::RowNotCommitted);
    }
    if row.leaf.value_count as usize != expected_lanes || row.leaf.values_le.len() != 4 * expected_lanes {
        return Err(PalwAttnCourtError::RowWidthMismatch { got: row.leaf.value_count as usize, expected: expected_lanes });
    }
    Ok(row.leaf.values_le.chunks_exact(4).map(|q| i32::from_le_bytes([q[0], q[1], q[2], q[3]])).collect())
}

/// **The bottom of the dissection: open one tile and recompute its triple** (ADR-0082 Decision 2,
/// step 3).
///
/// `kv_dim` is the cache row's width and `kv_off` the disputed head's slice within it — both
/// facts about the registered class, read from its profile by the caller, never from the wire.
/// `params` are the class's registered narrowings for the fused site, for the same reason.
///
/// The recompute is [`a16_attn_tile_triple_v1`] — the shipped kernels — against the ROOT's
/// `(m*, S*)`, and all three fields are compared. Returns `ExecutorGuilty` when any of them
/// disagrees with the claim the dissection narrowed to, and `ChallengerDefeated` when all three
/// agree: at that point the responder has stood behind its claim at every level and reproduced it
/// from committed material, which is what an honest execution looks like from the court's side.
#[allow(clippy::too_many_arguments)]
pub fn check_attn_dissect_bottom_v1(
    phase: &PalwAttnDissectPhaseV1,
    bottom: &PalwAttnDissectBottomV1,
    binding: &PalwAttnBottomBindingV1,
    params: A16AttnFusedParamsV1,
    kv_dim: usize,
    kv_off: usize,
    d_head: usize,
    kary_court_active: bool,
) -> Result<PalwAttnCourtVerdictV1, PalwAttnCourtError> {
    if !kary_court_active {
        return Err(PalwAttnCourtError::FenceDormant);
    }
    if bottom.version != PALW_ATTN_COURT_OBJECT_VERSION_V1 {
        return Err(PalwAttnCourtError::UnsupportedVersion { got: bottom.version, expected: PALW_ATTN_COURT_OBJECT_VERSION_V1 });
    }
    if bottom.session_id != phase.session_id() {
        return Err(PalwAttnCourtError::SessionMismatch);
    }
    let Some(expected_tile) = phase.terminal_tile() else {
        return Err(PalwAttnCourtError::TurnMismatch { expected: "a narrowed dissection", got: "a live one" });
    };
    if bottom.tile != expected_tile {
        return Err(PalwAttnCourtError::WrongTile { got: bottom.tile, expected: expected_tile });
    }
    let (_first_position, width) = phase.terminal_tile_positions().ok_or(PalwAttnCourtError::WrongTile {
        got: bottom.tile,
        expected: expected_tile,
    })?;
    if bottom.k_rows.len() != width || bottom.v_rows.len() != width {
        return Err(PalwAttnCourtError::WrongTileWidth {
            got: bottom.k_rows.len(),
            expected: width,
            tile: bottom.tile,
            history_positions: phase.history_positions,
        });
    }

    // Every operand is opened against the claim's own step root before a multiply happens.
    let qh = opened_lanes_v1(&bottom.query, binding, d_head)?;
    let mut k_tile = Vec::with_capacity(width * kv_dim);
    let mut v_tile = Vec::with_capacity(width * kv_dim);
    for (k_row, v_row) in bottom.k_rows.iter().zip(&bottom.v_rows) {
        k_tile.extend(opened_lanes_v1(k_row, binding, kv_dim)?);
        v_tile.extend(opened_lanes_v1(v_row, binding, kv_dim)?);
    }

    let (m_star, s_star) = phase.root_scale();
    let lanes = (phase.lane_first as usize, phase.lane_count as usize);
    let recomputed = a16_attn_tile_triple_v1(&qh, &k_tile, &v_tile, kv_dim, kv_off, lanes, params, m_star, s_star)?;
    Ok(if recomputed == *phase.claim() { PalwAttnCourtVerdictV1::ChallengerDefeated } else { PalwAttnCourtVerdictV1::ExecutorGuilty })
}

// =================================================================================================
// Z4 — the window gate a graph-v5 row is admitted against
// =================================================================================================

/// **Does this court's dissection of an `n_ctx`-wide row fit its own window?** (ADR-0082 Z4.)
///
/// `(2 × (⌈log_k L⌉ + ⌈log_k (n_ctx / tile)⌉) + terminal) × turn_deadline + assembly_reserve <
/// window_court`, with every input read from the ruleset: `k`, `L`, `terminal` and the deadline
/// from [`PalwCourtParamsV2`], the reserve from the court's own `max_close_chunks` (ADR-0080 W4's
/// term, taken from the ruleset being checked and never from a default), and `n_ctx` from the row.
/// Nothing is typed here.
///
/// Strict, for `palw_ladder_fits_window_court_v1`'s reason: the backstop closes on the
/// challenger's side, so a prosecution that lands exactly on the window loses a dispute it was
/// playing correctly.
///
/// `history_positions = 0` is a row with no fused attention site — a graph v2/v3 row — and the
/// answer is the ladder's own worst case, unchanged.
///
/// Returns the worst case in DAA so a caller can PIN the number rather than only learn that it
/// passed. This is the function `palw_class_admission_v2` calls at admission (stream F owns the
/// call site; the rule lives here, beside the protocol whose cost it is).
pub fn palw_attn_court_admits_row_v1(
    court: &PalwCourtParamsV2,
    history_positions: u64,
    tile: u32,
    window_court: u64,
) -> Result<u64, PalwAttnCourtError> {
    let reserve = crate::palw_context_ladder::palw_close_assembly_daa_v1(court.max_close_chunks());
    let worst = court
        .worst_case_duration_with_history_daa(history_positions, tile)
        .ok_or(PalwAttnCourtError::NoAdmissibleArity { window_court })?;
    let moves = worst / court.turn_deadline_daa().max(1);
    match worst.checked_add(reserve) {
        Some(total) if total < window_court => Ok(worst),
        _ => Err(PalwAttnCourtError::OverrunsWindow { moves, deadline: court.turn_deadline_daa(), reserve, window_court }),
    }
}

/// **What one round of this court weighs on the wire**, at the widest output tile a row disputes —
/// the quantity Z3 bounds and [`crate::palw_mode_v2::palw_court_arity_v1`] refuses an arity for.
pub fn palw_attn_court_move_bytes_v1(court: &PalwCourtParamsV2, lane_count: usize) -> u64 {
    palw_attn_dissect_move_bytes_v1(court.dissection_arity(), lane_count)
}

// =================================================================================================
// Tests — ADR-0082 Z2 (the dissection convicts and acquits), Z3 (a move fits one carrier),
// Z4 (the moves fit the window)
// =================================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_attn_dissect::{
        PALW_ATTN_DISSECT_MAX_ARITY, PALW_ATTN_DISSECT_MAX_CHILDREN, PALW_ATTN_DISSECT_OBJECT_VERSION_V1, palw_attn_fold_v1,
        palw_kary_rounds_v1,
    };
    use crate::palw_base0_a16::{A16QuantParams, a16_attn_root_claim_v1};
    use crate::palw_mode_v2::{PalwCourtParamsV2, palw_close_bytes_for_chunks_v1, palw_court_arity_v1};
    use crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4;
    use crate::palw_step::PalwStepCoordinateV1;
    use crate::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, step_merkle_path_v1, step_merkle_root_v1};

    const TILE: u32 = PALW_ATTN_HISTORY_TILE_V4;

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    /// The `fused` module's own fixture parameters — the same narrowings, so this court is
    /// checking the arithmetic the kernels were frozen with rather than a second calibration.
    fn params() -> A16AttnFusedParamsV1 {
        A16AttnFusedParamsV1 {
            scores: A16QuantParams { multiplier: 1 << 10, shift: 30, zero: 3 },
            probs: A16QuantParams { multiplier: 1 << 15, shift: 24, zero: 0 },
            values: A16QuantParams { multiplier: 1, shift: 22, zero: -5 },
            up_bits: 2,
        }
    }

    fn codes(n: usize, seed: u64) -> Vec<i32> {
        let mut state = seed | 1;
        (0..n)
            .map(|_| {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                ((state >> 33) % 65_535) as i32 - 32_767
            })
            .collect()
    }

    /// One head, one job: the query slice, the K and V series, and the committed step tree they
    /// were committed in.
    struct Fixture {
        d_head: usize,
        positions: usize,
        lanes: (usize, usize),
        q: Vec<i32>,
        k: Vec<i32>,
        v: Vec<i32>,
        root: PalwAttnRangeClaimV1,
        out_tile: Vec<i32>,
        leaf_hashes: Vec<Hash64>,
        binding: PalwAttnBottomBindingV1,
    }

    fn leaf_of(values: &[i32], slot: u32, position: u32) -> PalwStepTileLeafV1 {
        let mut values_le = Vec::with_capacity(values.len() * 4);
        for v in values {
            values_le.extend_from_slice(&v.to_le_bytes());
        }
        PalwStepTileLeafV1 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            coord: PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position, tile_index: 0 },
            value_count: values.len() as u32,
            values_le,
        }
    }

    const JOB: u8 = 0x11;
    const SHAPE: u8 = 0x22;

    /// Leaf layout: `0` the query row, `1 + 2p` position `p`'s K row, `2 + 2p` its V row, and the
    /// last leaf the committed output tile.
    fn leaf_index_query() -> usize {
        0
    }
    fn leaf_index_k(p: usize) -> usize {
        1 + 2 * p
    }
    fn leaf_index_v(p: usize) -> usize {
        2 + 2 * p
    }

    fn fixture(d_head: usize, positions: usize, lane_count: usize, seed: u64) -> Fixture {
        let kv_dim = d_head;
        let q = codes(d_head, seed);
        let k = codes(positions * kv_dim, seed ^ 0xa5a5);
        let v = codes(positions * kv_dim, seed ^ 0x5a5a);
        let lanes = (0usize, lane_count);
        let root = a16_attn_root_claim_v1(&q, &k, &v, kv_dim, 0, lanes, params(), TILE as usize).expect("an honest root claim");
        let out_tile = a16_attn_finalize_v1(&root.v_acc, params().values);

        let mut leaves: Vec<PalwStepTileLeafV1> = vec![leaf_of(&q, 0, 0)];
        for p in 0..positions {
            leaves.push(leaf_of(&k[p * kv_dim..(p + 1) * kv_dim], 1, p as u32));
            leaves.push(leaf_of(&v[p * kv_dim..(p + 1) * kv_dim], 2, p as u32));
        }
        leaves.push(leaf_of(&out_tile, 3, 0));
        let leaf_hashes: Vec<Hash64> = leaves.iter().map(|l| step_tile_leaf_hash_v1(&h64(JOB), &h64(SHAPE), l)).collect();
        let step_root = step_merkle_root_v1(&leaf_hashes).expect("a tree over the committed rows");
        Fixture {
            d_head,
            positions,
            lanes,
            q,
            k,
            v,
            root,
            out_tile,
            binding: PalwAttnBottomBindingV1 {
                job_context_hash: h64(JOB),
                shape_profile_hash: h64(SHAPE),
                step_root,
                step_leaf_count: leaf_hashes.len() as u64,
                max_step_leaf_count: 1 << 32,
            },
            leaf_hashes,
        }
    }

    impl Fixture {
        fn tile_count(&self) -> u64 {
            (self.positions as u64).div_ceil(TILE as u64)
        }

        /// The honest triple of one tile, against a stated `(m*, S*)` — what the executor folds
        /// and what the court recomputes at the bottom.
        fn tile_triple(&self, tile: u64, m_star: i32, s_star: i64) -> PalwAttnRangeClaimV1 {
            let kv_dim = self.d_head;
            let first = tile as usize * TILE as usize;
            let width = (self.positions - first).min(TILE as usize);
            a16_attn_tile_triple_v1(
                &self.q,
                &self.k[first * kv_dim..(first + width) * kv_dim],
                &self.v[first * kv_dim..(first + width) * kv_dim],
                kv_dim,
                0,
                self.lanes,
                params(),
                m_star,
                s_star,
            )
            .expect("an honest tile recomputes")
        }

        /// The honest claim over a RANGE of tiles: the fold of its tiles' triples, in
        /// arity-capped groups exactly as the rounds fold them.
        fn range_claim(&self, first: u64, count: u64, m_star: i32, s_star: i64) -> PalwAttnRangeClaimV1 {
            let mut level: Vec<PalwAttnRangeClaimV1> = (first..first + count).map(|t| self.tile_triple(t, m_star, s_star)).collect();
            while level.len() > 1 {
                let mut next = Vec::new();
                for group in level.chunks(PALW_ATTN_DISSECT_MAX_CHILDREN) {
                    next.push(palw_attn_fold_v1(group).expect("an honest fold"));
                }
                level = next;
            }
            level.pop().expect("a non-empty range")
        }

        fn row_opening(&self, index: usize, values: &[i32], slot: u32, position: u32) -> PalwAttnRowOpeningV1 {
            let leaf = leaf_of(values, slot, position);
            PalwAttnRowOpeningV1 {
                leaf,
                opening: PalwStepOpeningV1 {
                    leaf_index: index as u64,
                    leaf_hash: self.leaf_hashes[index],
                    siblings: step_merkle_path_v1(&self.leaf_hashes, index).expect("a path for a committed leaf"),
                },
            }
        }

        fn bottom(&self, session_id: Hash64, tile: u64) -> PalwAttnDissectBottomV1 {
            let kv_dim = self.d_head;
            let first = tile as usize * TILE as usize;
            let width = (self.positions - first).min(TILE as usize);
            let mut k_rows = Vec::new();
            let mut v_rows = Vec::new();
            for p in first..first + width {
                k_rows.push(self.row_opening(leaf_index_k(p), &self.k[p * kv_dim..(p + 1) * kv_dim], 1, p as u32));
                v_rows.push(self.row_opening(leaf_index_v(p), &self.v[p * kv_dim..(p + 1) * kv_dim], 2, p as u32));
            }
            PalwAttnDissectBottomV1 {
                version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                session_id,
                tile,
                query: self.row_opening(leaf_index_query(), &self.q, 0, 0),
                k_rows,
                v_rows,
                out_tile: self.row_opening(self.leaf_hashes.len() - 1, &self.out_tile, 3, 0),
            }
        }
    }

    fn court_at(arity: u8) -> PalwCourtParamsV2 {
        PalwCourtParamsV2::new(1 << 22, 20, 2).expect("a court").with_dissection_arity(arity).expect("a legal arity")
    }

    fn open_phase(fx: &Fixture, arity: u8, root_claim: PalwAttnRangeClaimV1) -> Result<PalwAttnDissectPhaseV1, PalwAttnCourtError> {
        let root = PalwAttnRootClaimV1 {
            version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            head: 0,
            lane_first: fx.lanes.0 as u16,
            lane_count: fx.lanes.1 as u16,
            history_positions: fx.positions as u32,
            claim: root_claim,
        };
        PalwAttnDissectPhaseV1::open(h64(9), &root, &fx.out_tile, params().values, &court_at(arity), TILE, 100, 30, true)
    }

    /// What one full dissection decided, and how it got there.
    #[derive(Debug, PartialEq, Eq)]
    enum Played {
        /// A disclosure did not fold — the conviction that needs no execution.
        FoldRefused(PalwAttnDissectError),
        /// The bottom recompute contradicted the claim the dissection narrowed to.
        Convicted,
        /// The bottom recompute reproduced it.
        Acquitted,
    }

    /// **Play a whole dissection.** The RESPONDER discloses the claims of `claims_of(first, count)`
    /// for each child; the CHALLENGER recomputes the honest claim of each child and names the
    /// first one that differs (its real strategy — ADR-0082 Decision 9: "a challenger is a seat
    /// that recomputed"), or child 0 when nothing differs.
    fn play(
        fx: &Fixture,
        arity: u8,
        phase: &mut PalwAttnDissectPhaseV1,
        claims_of: &dyn Fn(u64, u64) -> PalwAttnRangeClaimV1,
    ) -> Played {
        let (m_star, s_star) = phase.root_scale();
        let mut daa = 200u64;
        while phase.turn() != PalwBisectTurnV1::Terminal {
            let ranges = phase.child_ranges();
            let children: Vec<_> = ranges.iter().map(|&(f, c)| claims_of(f, c)).collect();
            let round = PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children: children.clone() };
            daa += 1;
            if let Err(PalwAttnCourtError::Dissect(e)) = phase.apply_round(&round, daa, 30) {
                return Played::FoldRefused(e);
            }
            // The challenger recomputes every child and names the first that is not what it
            // computed. It never needs the responder's help to know which one to pick.
            let child = ranges
                .iter()
                .zip(&children)
                .position(|(&(f, c), claimed)| *claimed != fx.range_claim(f, c, m_star, s_star))
                .unwrap_or(0) as u8;
            daa += 1;
            phase
                .apply_choice(&PalwAttnDissectChoiceV1 { version: PALW_ATTN_COURT_OBJECT_VERSION_V1, session_id: h64(9), round: phase.round(), child }, daa, 30)
                .expect("a choice inside the arity");
            assert!(phase.round() <= phase.round_budget(), "arity {arity}: the dissection outran its own round bound");
        }
        let tile = phase.terminal_tile().expect("a narrowed dissection");
        let bottom = fx.bottom(h64(9), tile);
        match check_attn_dissect_bottom_v1(phase, &bottom, &fx.binding, params(), fx.d_head, 0, fx.d_head, true)
            .expect("the bottom's openings verify")
        {
            PalwAttnCourtVerdictV1::ExecutorGuilty => Played::Convicted,
            PalwAttnCourtVerdictV1::ChallengerDefeated => Played::Acquitted,
        }
    }

    // ---------------------------------------------------------------------------------------
    // Z2 — the dissection convicts every forgery and acquits every honest execution
    // ---------------------------------------------------------------------------------------

    /// **An honest responder is never convicted** — at every arity, at a history that is a
    /// multiple of the tile and at one that is not (the ragged last tile).
    #[test]
    fn an_honest_dissection_acquits_at_every_arity_and_every_ragged_tail() {
        for positions in [16usize, 17, 50, 128, 129] {
            let fx = fixture(8, positions, 4, 7);
            for arity in [2u8, 4, 16, 64] {
                let mut phase = open_phase(&fx, arity, fx.root.clone()).expect("the honest root finalizes to the committed tile");
                let (m, s) = phase.root_scale();
                let played = play(&fx, arity, &mut phase, &|f, c| fx.range_claim(f, c, m, s));
                assert_eq!(played, Played::Acquitted, "positions {positions} at arity {arity}: an honest execution was convicted");
                assert_eq!(
                    phase.round(),
                    palw_kary_rounds_v1(fx.tile_count(), arity).expect("a legal arity"),
                    "positions {positions} at arity {arity}: the rounds are not the contract's count"
                );
            }
        }
    }

    /// **A lie in `m*` is found by the max fold** — with honest children, at the first round,
    /// naming the field and both values.
    #[test]
    fn a_lie_in_the_row_max_does_not_fold() {
        let fx = fixture(8, 50, 4, 11);
        for arity in [2u8, 4, 16, 64] {
            let mut lied = fx.root.clone();
            lied.max += 1;
            // The claim still finalizes to the committed tile — only the max moved — so it is
            // admitted and refuted by the protocol rather than by the door.
            let mut phase = open_phase(&fx, arity, lied).expect("a claim that finalizes is admitted");
            let (_, s) = phase.root_scale();
            let played = play(&fx, arity, &mut phase, &|f, c| fx.range_claim(f, c, fx.root.max, s));
            match played {
                Played::FoldRefused(PalwAttnDissectError::MaxDoesNotFold { claimed, folded }) => {
                    assert_eq!((claimed, folded), (fx.root.max + 1, fx.root.max), "arity {arity}");
                }
                other => panic!("arity {arity}: a lied m* was not caught by the max fold: {other:?}"),
            }
        }
    }

    /// **A lie in `S*` is found by the sum fold**, the same way and at the same round.
    #[test]
    fn a_lie_in_the_exponent_sum_does_not_fold() {
        let fx = fixture(8, 50, 4, 13);
        let mut lied = fx.root.clone();
        lied.exp_sum += 1;
        let mut phase = open_phase(&fx, 4, lied).expect("a claim that finalizes is admitted");
        let (m, _) = phase.root_scale();
        // The children are computed against the LIED S*, which is what an executor defending the
        // claim would disclose — and their exponent sums are computed from the scores, so they
        // fold to the true sum whatever `S*` says.
        let played = play(&fx, 4, &mut phase, &|f, c| fx.range_claim(f, c, m, fx.root.exp_sum + 1));
        match played {
            Played::FoldRefused(PalwAttnDissectError::SumDoesNotFold { claimed, folded }) => {
                assert_eq!((claimed, folded), (fx.root.exp_sum + 1, fx.root.exp_sum));
            }
            other => panic!("a lied S* was not caught by the sum fold: {other:?}"),
        }
    }

    /// **A lie carried CONSISTENTLY down one branch reaches the bottom and is convicted there.**
    ///
    /// This is the case the fold alone cannot catch: the responder moves the lie into one child
    /// and compensates in another, so every level folds. The challenger recomputes, names the
    /// child that is not what it computed, and the tile's recompute contradicts the claim.
    #[test]
    fn a_lie_pushed_into_one_child_is_convicted_at_the_bottom() {
        for positions in [50usize, 129] {
            let fx = fixture(8, positions, 4, 17);
            for arity in [2u8, 4, 16, 64] {
                let mut phase = open_phase(&fx, arity, fx.root.clone()).expect("an honest root");
                let (m, s) = phase.root_scale();
                // Lie about the LAST tile's value partial, and compensate in the first, so every
                // fold is exact and only a recompute can tell.
                let last = fx.tile_count() - 1;
                let fx = &fx;
                let claims = move |f: u64, c: u64| {
                    let mut claim = fx.range_claim(f, c, m, s);
                    if f <= last && last < f + c {
                        claim.v_acc[0] += 1_000;
                    }
                    if f == 0 {
                        claim.v_acc[0] -= 1_000;
                    }
                    claim
                };
                // The compensation only cancels when both tiles are in the same range, which they
                // are at the root; below it the challenger follows whichever child moved.
                let played = play(&fx, arity, &mut phase, &claims);
                assert_eq!(played, Played::Convicted, "positions {positions} at arity {arity}: a forged child survived to acquittal");
            }
        }
    }

    /// **A lie in the bottom tile itself is convicted**: the shallowest possible forgery, where
    /// the dissection has one round or none.
    #[test]
    fn a_lie_in_the_only_tile_is_convicted() {
        let fx = fixture(8, 16, 4, 19);
        assert_eq!(fx.tile_count(), 1, "one tile is the bottom without a round");
        let mut lied = fx.root.clone();
        lied.v_acc[0] += 7;
        // A moved `V*` no longer finalizes to the committed tile unless the move is below the
        // narrowing's resolution; either way the court must not play rounds against it.
        let opened = open_phase(&fx, 4, lied.clone());
        match opened {
            Err(PalwAttnCourtError::RootDoesNotFinalize) => {}
            Ok(mut phase) => {
                let played = play(&fx, 4, &mut phase, &|f, c| {
                    let (m, s) = (lied.max, lied.exp_sum);
                    let mut claim = fx.range_claim(f, c, m, s);
                    claim.v_acc[0] += 7;
                    claim
                });
                assert_eq!(played, Played::Convicted, "a forged single tile was acquitted");
            }
            Err(other) => panic!("unexpected refusal: {other}"),
        }
    }

    /// **A tampered bottom opening is refused before any arithmetic**: a K row that is not the
    /// one the claim committed does not verify against the step root, so the recompute never runs
    /// on it.
    #[test]
    fn a_bottom_row_that_is_not_committed_is_refused_by_its_opening() {
        let fx = fixture(8, 50, 4, 23);
        let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
        let (m, s) = phase.root_scale();
        // Walk to the bottom honestly.
        while phase.turn() != PalwBisectTurnV1::Terminal {
            let ranges = phase.child_ranges();
            let children: Vec<_> = ranges.iter().map(|&(f, c)| fx.range_claim(f, c, m, s)).collect();
            phase
                .apply_round(&PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children }, 300, 30)
                .expect("an honest round folds");
            phase
                .apply_choice(
                    &PalwAttnDissectChoiceV1 {
                        version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                        session_id: h64(9),
                        round: phase.round(),
                        child: 0,
                    },
                    310,
                    30,
                )
                .expect("child 0 exists");
        }
        let tile = phase.terminal_tile().expect("narrowed");
        let mut bottom = fx.bottom(h64(9), tile);
        bottom.k_rows[0].leaf.values_le[0] ^= 0xff;
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &bottom, &fx.binding, params(), fx.d_head, 0, fx.d_head, true),
            Err(PalwAttnCourtError::RowNotCommitted)
        );
        // And the same bottom against the wrong tile is refused by name rather than recomputed.
        let mut elsewhere = fx.bottom(h64(9), tile);
        elsewhere.tile = tile + 1;
        assert!(matches!(
            check_attn_dissect_bottom_v1(&phase, &elsewhere, &fx.binding, params(), fx.d_head, 0, fx.d_head, true),
            Err(PalwAttnCourtError::WrongTile { .. })
        ));
    }

    /// **The fence is the door.** Nothing in this module runs on a network whose
    /// `palw_kary_court` is dormant, and the refusal says which fence.
    #[test]
    fn the_dissection_is_refused_by_name_while_the_fence_is_dormant() {
        let fx = fixture(8, 32, 4, 29);
        let root = PalwAttnRootClaimV1 {
            version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            head: 0,
            lane_first: 0,
            lane_count: 4,
            history_positions: 32,
            claim: fx.root.clone(),
        };
        assert_eq!(
            PalwAttnDissectPhaseV1::open(h64(9), &root, &fx.out_tile, params().values, &court_at(16), TILE, 100, 30, false),
            Err(PalwAttnCourtError::FenceDormant)
        );
        // A root claim that does not finalize to the opened tile never reaches a round.
        let mut wrong = root.clone();
        wrong.claim.v_acc[0] = i64::MAX / 4;
        assert_eq!(
            PalwAttnDissectPhaseV1::open(h64(9), &wrong, &fx.out_tile, params().values, &court_at(16), TILE, 100, 30, true),
            Err(PalwAttnCourtError::RootDoesNotFinalize)
        );
    }

    /// **Silence at any move is the same objective offense**, charged to whoever was due — and at
    /// the bottom nobody is charged, because the move there is a close the accused would be
    /// filing against itself (the shipped court's own rule, `court_next_deadline_v2`).
    #[test]
    fn silence_at_a_dissection_move_is_the_objective_offense() {
        let fx = fixture(8, 50, 4, 31);
        let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
        assert_eq!(phase.last_deadline_daa(), 130, "the first deadline is the opening score plus w_round");
        assert_eq!(phase.declare_no_show(129), Err(PalwAttnCourtError::DeadlineNotReached { deadline: 130, observed: 129 }));
        let mut silent = phase.clone();
        assert_eq!(silent.declare_no_show(131).expect("silence past the deadline").silent_party, PalwBisectPartyV1::Responder);
        assert_eq!(silent.turn(), PalwBisectTurnV1::Abandoned);
        assert_eq!(silent.declare_no_show(200), Err(PalwAttnCourtError::AlreadyTerminal), "an abandoned phase charges no second offense");

        let (m, s) = phase.root_scale();
        let children: Vec<_> = phase.child_ranges().iter().map(|&(f, c)| fx.range_claim(f, c, m, s)).collect();
        phase
            .apply_round(&PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children }, 200, 30)
            .expect("an honest round");
        assert_eq!(phase.turn(), PalwBisectTurnV1::AwaitVerdict);
        assert_eq!(phase.declare_no_show(231).expect("the challenger is now due").silent_party, PalwBisectPartyV1::Challenger);
    }

    // ---------------------------------------------------------------------------------------
    // Z3 — a round and a bottom fit ONE framed carrier
    // ---------------------------------------------------------------------------------------

    /// **Every move of this court is inside one framed carrier**, measured on the REAL objects'
    /// borsh size rather than on the byte formula alone — and the formula is checked against the
    /// measurement, which is what keeps `palw_attn_dissect_move_bytes_v1` a price and not a guess.
    ///
    /// Swept at both registered head widths (`d_head` 128 dense, 256 hybrid) and every legal
    /// arity. The carrier bound is a property of the (arity, lanes) PAIR: at 256 lanes the widest
    /// arity a round fits in is smaller than at 128, and the derivation is what must refuse the
    /// rest.
    #[test]
    fn a_round_and_a_bottom_fit_one_framed_carrier() {
        let carrier = palw_close_bytes_for_chunks_v1(1);
        assert_eq!(carrier, 100_000 * 10 / 12, "one carrier's counted bytes are the framed chunk");
        let mut widest_arity = std::collections::BTreeMap::new();
        for lanes in [128usize, 256] {
            let mut arity = 2u8;
            loop {
                let children: Vec<_> =
                    (0..arity).map(|i| PalwAttnRangeClaimV1 { max: i as i32, exp_sum: 1 << 40, v_acc: vec![-1; lanes] }).collect();
                let round = PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children };
                let measured = borsh::to_vec(&round).expect("a round serializes").len() as u64;
                let priced = palw_attn_dissect_move_bytes_v1(arity, lanes);
                assert_eq!(measured, priced, "arity {arity} at {lanes} lanes: the price is not the measurement");
                if priced <= carrier {
                    widest_arity.insert(lanes, arity);
                }
                if arity == PALW_ATTN_DISSECT_MAX_ARITY {
                    break;
                }
                arity *= 2;
            }
        }
        // The pair, pinned: 64 children of a 128-lane tile ride one carrier; 64 of a 256-lane one
        // do not, and 32 do.
        assert_eq!(widest_arity[&128], 64, "every legal arity fits at a 128-lane tile");
        assert_eq!(widest_arity[&256], 32, "arity 64 at 256 lanes is over one carrier");
        assert_eq!(palw_attn_dissect_move_bytes_v1(64, 256), 132_102);
        assert!(palw_attn_dissect_move_bytes_v1(64, 256) > carrier);

        // The BOTTOM, at both head widths, with the real openings and the real paths.
        for d_head in [128usize, 256] {
            let fx = fixture(d_head, 64, d_head.min(PALW_ATTN_DISSECT_MAX_LANES), 37);
            let bottom = fx.bottom(h64(9), 1);
            let measured = borsh::to_vec(&bottom).expect("a bottom serializes").len() as u64;
            assert!(measured <= carrier, "d_head {d_head}: a bottom of {measured} bytes is over one carrier of {carrier}");
            // The ADR's own arithmetic: one K tile, one V tile, the query row and the output tile,
            // plus the paths. The tile rows dominate and they are flat in the context.
            let rows = 2 * TILE as u64 * d_head as u64 * 4;
            assert!(measured > rows, "d_head {d_head}: the measured bottom must at least carry its tiles");
        }
    }

    /// **The bottom does not grow with the context.** The same tile at 64 positions of history and
    /// at 4,096 weighs the same but for the Merkle paths, which grow with `⌈log₂ leaves⌉ — the
    /// logarithm R4 allows and nothing else.
    #[test]
    fn the_bottom_is_flat_in_the_context_but_for_the_paths() {
        let mut sizes = Vec::new();
        for positions in [64usize, 512, 4_096] {
            let fx = fixture(8, positions, 4, 41);
            let bytes = borsh::to_vec(&fx.bottom(h64(9), 1)).expect("serializes").len() as u64;
            sizes.push((positions, bytes));
        }
        let (_, small) = sizes[0];
        let (_, large) = sizes[2];
        let paths = 64u64 * 6 * (2 * TILE as u64 + 2); // six extra levels of tree, on every opened row
        assert!(large - small <= paths, "the bottom grew by {} bytes over a 64x context, past its paths' {paths}", large - small);
    }

    // ---------------------------------------------------------------------------------------
    // Z4 — the moves fit the window, and the arity is derived
    // ---------------------------------------------------------------------------------------

    /// **The window gate**: a row is admitted only if its whole dissection fits `window_court`
    /// with the assembly reserve, and the refusal carries the arithmetic.
    #[test]
    fn the_window_gate_admits_a_row_only_if_its_whole_dissection_fits() {
        // A row with no fused site is the ladder's own worst case, unchanged.
        let court = court_at(2);
        let ladder_only = palw_attn_court_admits_row_v1(&court, 0, TILE, 3_000).expect("the shipped ladder fits the RC window");
        assert_eq!(ladder_only, (2 * 22 + 2) * 20, "22 binary rounds at the fixture's 20-DAA clock");
        // The same court with a 131,072-position history does not fit at arity 2 and does at 16.
        let wide = palw_attn_court_admits_row_v1(&court, 131_072, TILE, 3_000);
        assert!(matches!(wide, Err(PalwAttnCourtError::OverrunsWindow { .. })), "a binary dissection of 8,192 tiles fits nothing");
        let sixteen = court_at(16);
        let worst = palw_attn_court_admits_row_v1(&sixteen, 131_072, TILE, 3_000).expect("16-ary fits");
        assert_eq!(worst, (2 * (6 + 4) + 2) * 20, "6 ladder rounds at 2^22 and 4 history rounds at 8,192 tiles");
    }

    /// **The arity is DERIVED, and the derivation is the ADR's own inequality.**
    ///
    /// The table below is the whole of ADR-0082 Decision 3 as arithmetic: what each candidate
    /// arity costs in moves at the `2^32` ladder with a 131,072-position history at a 16-position
    /// tile, and therefore which deadlines select which arity. It is written as a sweep rather
    /// than as one assertion because the ADR's worked value (16) is only the SMALLEST fitting
    /// arity for part of the deadline range, and a reader deserves to see where the boundary is.
    #[test]
    fn the_court_arity_is_derived_from_the_move_budget() {
        const LADDER: u64 = 1 << 32;
        const HISTORY: u64 = 131_072;
        const WINDOW: u64 = 3_000;
        // The ADR's table, from the contract's own recurrence.
        let moves = |k: u8| -> u64 {
            2 * (u64::from(palw_kary_rounds_v1(LADDER, k).unwrap())
                + u64::from(crate::palw_attn_dissect::palw_attn_dissection_rounds_v1(HISTORY, TILE, k).unwrap()))
                + 2
        };
        assert_eq!((moves(2), moves(4), moves(8), moves(16), moves(32), moves(64)), (92, 48, 34, 26, 22, 20));
        assert_eq!(moves(16) * 45, 1_170, "ADR-0082 §4: 26 moves at the 45-DAA deadline");
        assert_eq!(moves(2) * 45, 4_140, "and the binary ladder at the same deadline is over the 3,000-DAA window");

        // The derivation, at a 128-lane output tile where the carrier bound binds nothing.
        let pick = |deadline: u64, lanes: usize| palw_court_arity_v1(WINDOW, deadline, LADDER, HISTORY, TILE, 2, lanes);
        // The boundaries, stated as the two inequalities they are.
        assert_eq!(pick(WINDOW / moves(16), 128), Some(16));
        assert_eq!(pick(WINDOW / moves(8) + 1, 128), Some(16), "one DAA past what 8 can afford, 16 is the smallest that fits");
        assert_eq!(pick(WINDOW / moves(8), 128), Some(8));
        assert_eq!(pick(WINDOW / moves(4), 128), Some(4));
        assert_eq!(pick(WINDOW / moves(2), 128), Some(2), "a short enough clock needs no wider round");
        assert_eq!(pick(WINDOW, 128), None, "no arity fits a deadline the size of the whole window");
        // And the carrier bound refuses from ABOVE: at 256 lanes the arity a very long deadline
        // would need does not fit a carrier, so the court is refused rather than priced.
        assert_eq!(pick(WINDOW / moves(64), 256), None, "arity 64 at 256 lanes is 132,102 bytes a move");
        assert_eq!(pick(WINDOW / moves(64), 128), Some(64));
        // A ruleset with no fused site derives from the ladder alone.
        assert_eq!(palw_court_arity_v1(WINDOW, 45, LADDER, 0, TILE, 2, 128), Some(2), "66 moves at 45 DAA is 2,970");
    }

    /// **Every shipped preset's court still fits its own window with the dissection at zero
    /// history** — the shipped rows have no fused site, so ADR-0082 changes nothing about them,
    /// and this is the assertion that says so rather than assuming it.
    #[test]
    fn every_shipped_preset_keeps_its_window_under_the_dissection() {
        use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS};
        for (name, preset) in
            [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
        {
            let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &preset.palw_consensus_mode else { continue };
            let window = bundle.state.window_court();
            let worst = palw_attn_court_admits_row_v1(&bundle.court, 0, TILE, window)
                .unwrap_or_else(|e| panic!("{name}: the shipped court no longer fits its own window: {e}"));
            assert_eq!(
                worst,
                bundle.court.worst_case_duration_daa().expect("a representable worst case"),
                "{name}: the gate's answer is not the bundle's own worst case at zero history"
            );
            assert_eq!(bundle.court.dissection_arity(), 2, "{name}: a shipped preset runs the binary ladder");
        }
    }
}
