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
use crate::palw_state_chunk_map::{
    PalwStateChunkGeometryV1, PalwStateChunkKindV1, integer_kv_state_chunk_entry_v1, integer_kv_state_locate_v1,
    integer_kv_state_row_v1,
};
use crate::palw_step_leg::{
    PalwCheckpointLeafV2, PalwStepLegError, PalwStepOpeningV1, PalwStepTileLeafV1, checkpoint_leaf_hash_v2, state_chunk_leaf_hash_v1,
    state_chunk_opening_root_v1, step_opening_root_capped_v1, step_tile_leaf_hash_v1,
};

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
    /// **The cache-write route is not a sound court route, and for a class that checkpoints every
    /// position it is not a necessary one** (ADR-0082 Decision 4, amended).
    ///
    /// The route is now BOUND — every cache-write row is compared with the full coordinate the
    /// class puts it at, node slot included, and the slot is the layer's `KCacheWrite` /
    /// `VCacheWrite` node (audit A C-3/H-1) — so the wholesale K/V swap this refusal was first
    /// written for is refused by `WrongCoordinate` rather than admitted. What the route still
    /// cannot do is carry a cache row that is SEVERAL leaves (`CacheRowIsNotOneLeaf`), and what
    /// the checkpoint route still has that it does not is a kind, layer and position that all come
    /// from the map, with the map inside the class id — one fact instead of three comparisons.
    ///
    /// The refusal therefore stands where the class has an alternative, and it is now the cheaper
    /// of two sound routes rather than the only sound one.
    ///
    /// Refused only where the class has an alternative. A per-decode-call leg commits no checkpoint
    /// over the prefill at all, so refusing the cache-write route there would leave a prefill
    /// dispute with no route; a class whose map addresses history tiles has a checkpoint after
    /// every position, so there is no position the cache-write route is needed for.
    #[error(
        "this class commits a checkpoint at every position, so a dissection bottom must open the checkpoint route: the \
         cache-write route does not bind a row to its own K or V series and is refused"
    )]
    CacheWriteRouteRefused,
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
    #[error(
        "the root claim is about head {got_head} lanes {got_first}..+{got_count}; the ladder terminated on head {head} lanes {first}..+{count}"
    )]
    WrongSite { got_head: u16, got_first: u16, got_count: u16, head: u16, first: u16, count: u16 },
    #[error("the root claim states a history of {got} positions; the leaf the ladder terminated on reads {expected}")]
    WrongHistory { got: u32, expected: u32 },
    #[error(
        "the root claim's exponent sum {got} is outside the band {min}..={max} that {positions} positions of an honest row can produce"
    )]
    ExponentSumOutOfBand { got: i64, min: i64, max: i64, positions: u32 },
    #[error("the bottom's {what} opening is committed at {got:?}; the class puts it at {expected:?}")]
    WrongCoordinate {
        what: &'static str,
        got: crate::palw_step::PalwStepCoordinateV1,
        expected: crate::palw_step::PalwStepCoordinateV1,
    },
    #[error("the bottom's K evidence and its V evidence take different routes; a bottom reads one route")]
    MixedEvidenceRoutes,
    #[error("the bottom anchors at the checkpoint covering {got}; this step's anchor is the one covering {expected}")]
    WrongAnchor { got: u32, expected: u32 },
    #[error(
        "this class commits a checkpoint at every position, so a tile is wholly inside its anchor; {got} rows were opened after it"
    )]
    RowsAfterOnAPerPositionClass { got: usize },
    #[error("the class names no node holding the {kind} cache role, so a cache-write row cannot be bound to its series")]
    CacheSeriesNotDeclared { kind: &'static str },
    #[error("a cache row is {kv_dim} lanes and the node that writes it tiles at {tile}: one opened leaf is not one row")]
    CacheRowIsNotOneLeaf { kv_dim: usize, tile: usize },
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
    #[error("the K rows and the V rows of this tile are the same committed series — a tile has two")]
    KeyAndValueAreTheSameSeries,
    #[error("the bottom reads a tile from a checkpoint and carries no anchor for it")]
    AnchorMissing,
    #[error("the bottom reads a tile from a checkpoint and the class's tiled geometry was not supplied")]
    AnchorGeometryMissing,
    #[error("the anchor's checkpoint leaf is not in the claim's checkpoint leg")]
    AnchorNotCommitted,
    #[error("the anchor covers {positions} positions and the geometry describes {geometry} — two histories, not one")]
    AnchorGeometryDoesNotDescribeTheAnchor { positions: u32, geometry: u32 },
    #[error("the checkpoint declares {declared} chunks and the class's map has {derived}")]
    AnchorChunkCountMismatch { declared: u32, derived: u64 },
    #[error("the class's map tiles the cache {map} positions at a time and the court dissects {court}")]
    MapTileIsNotTheCourtTile { map: u32, court: u32 },
    #[error("the bottom opens chunk {got}; the tile at position {position} lives in chunk {expected}")]
    WrongChunk { got: u32, expected: u64, position: u64 },
    #[error("chunk {chunk} starts at position {start} and the disputed tile starts at {tile_start}")]
    ChunkIsNotTheTile { chunk: u32, start: u32, tile_start: u64 },
    #[error("the opened chunk is not in the checkpoint's state: it rebuilds {folded}, the leaf says {claimed}")]
    ChunkNotInCheckpoint { folded: Hash64, claimed: Hash64 },
    #[error("the opened chunk does not hold position {position} in the shape the map declares")]
    ChunkRowUnreadable { position: u64 },
    #[error("the checkpoint covers {covered} of the tile's {width} positions and {got} rows were opened after it")]
    RowsAfterMismatch { covered: usize, width: usize, got: usize },
    #[error("the checkpoint covers none of the disputed tile — the cache-write route is the only one here")]
    AnchorCoversNothing,
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

/// **One chunk of a checkpoint's state, opened by membership in its `state_chunks_root`.**
///
/// Under the graph-v4 tiled map (`tiled_kv_state_geometry_v3`) a chunk IS a history tile, so this
/// is the whole of what the bottom needs from a checkpoint: `tile × kv_row` bytes and a path of
/// `⌈log₂ chunks⌉` siblings, at every context. `state_chunk_opening_root_v1` is what makes it
/// checkable without the other chunks — before it existed, the only evidence about a checkpoint's
/// state was `PalwCheckpointKvOperandsV1`, which carries every chunk of it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnChunkOpeningV1 {
    pub chunk_index: u32,
    pub chunk_bytes: Vec<u8>,
    /// The path to the checkpoint leaf's `state_chunks_root`.
    pub siblings: Vec<Hash64>,
}

/// **The checkpoint a tile is read from**: the leaf, and its opening against the claim's
/// checkpoint leg root.
///
/// One anchor serves both K and V — they are two slices of the same checkpoint — so it rides the
/// bottom once rather than once per kind.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnCheckpointAnchorV1 {
    pub leaf: PalwCheckpointLeafV2,
    pub opening: PalwStepOpeningV1,
}

/// **Where a tile's K (or V) rows come from** — ADR-0082 Decision 2's two routes, as one type.
///
/// A history tile is either wholly after the last checkpoint (every row is a committed
/// cache-write leaf) or reaches into it (one chunk opening, plus the rows the checkpoint does not
/// cover when the tile STRADDLES its edge — which is the case whenever the anchor's position count
/// is not a multiple of the map's tile).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwAttnTileEvidenceV1 {
    /// Every row of the tile, opened as a committed cache-write leaf of the step tree.
    CacheWrites { rows: Vec<PalwAttnRowOpeningV1> },
    /// The tile read from ONE chunk of the anchor, plus the rows past the anchor's edge.
    Checkpoint { chunk: PalwAttnChunkOpeningV1, rows_after: Vec<PalwAttnRowOpeningV1> },
}

/// **The bottom of the dissection**: the head's query slice, the tile's K and V rows, and the
/// output tile the root claim was checked against — every one of them opened against something
/// the claim committed.
///
/// # What this carries and what it does not
///
/// It carries ONE tile: `PALW_ATTN_HISTORY_TILE_V4` positions of K and of V, the query row, the
/// output tile, and the paths that prove them. That is flat in the context, which is the whole of
/// ADR-0082 R4 at this site. It does NOT carry the history, the probability row, the score row,
/// or a checkpoint's chunk list.
///
/// # Both routes are here
///
/// ADR-0082 Decision 2 names two ways to reach a tile's K and V rows and this object carries
/// both, per kind ([`PalwAttnTileEvidenceV1`]): the cache-write leaves of the step tree, and one
/// chunk of the checkpoint at or before the tile under the class's tiled map. The second is what
/// Decision 4 is FOR — "the bottom of the dissection opens tiles, so the anchor is tile-addressed
/// and priced as such" — and it is the cheaper of the two whenever it applies, because one chunk
/// opening replaces sixteen row openings and their sixteen paths.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnDissectBottomV1 {
    pub version: u16,
    pub session_id: Hash64,
    /// The tile index the dissection narrowed to — checked against the phase, never trusted.
    pub tile: u64,
    /// The head's rotated query slice: `d_head` codes.
    pub query: PalwAttnRowOpeningV1,
    /// The checkpoint both tile evidences read from, when either of them does.
    pub anchor: Option<PalwAttnCheckpointAnchorV1>,
    /// The tile's K rows, position-major.
    pub k: PalwAttnTileEvidenceV1,
    /// The tile's V rows, in the same order.
    pub v: PalwAttnTileEvidenceV1,
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
    /// The claim's checkpoint leg: the root an anchor's opening proves against, the leaf count
    /// that root was built over, and the two hashes a checkpoint leaf is bound to.
    pub checkpoint_merkle_root: Hash64,
    pub checkpoint_leaf_count: u64,
    pub checkpoint_profile_hash: Hash64,
    pub state_chunk_map_id: Hash64,
}

/// **The site under dispute, as the registered CLASS describes it** — never as the wire does.
///
/// Every field is read off the class's profile and its registered map by the caller: the fused
/// site's four narrowings, the cache row's width and the head's slice within it, the attention
/// layer the node sits at, and — when a checkpoint is in evidence — the layout that checkpoint's
/// chunks are enumerated under, together with the position count the anchor covers.
///
/// `anchor_positions` is `palw_checkpoint_positions_at_v1(profile, job context, covered)` — the
/// class's OWN cadence: on a per-decode-call map that is `integer_kv_positions_at_v1`, on a
/// per-position map the counter IS the position count (the two differ by `prefill`, so naming
/// the per-call rule here would describe a history `prefill` rows too long for a graph-v5 row).
/// It is here because a geometry and an anchor that describe different histories would let a
/// chunk index point at another position's rows, and this is the field that says they are one.
///
/// **WHICH checkpoint is this step's anchor is decided by the CALLER and refused here by name.**
/// The rule is `palw_context_ladder::palw_checkpoint_covered_for_step_v1` — the one function that
/// says which checkpoint a step's evidence must carry — evaluated in
/// `palw_court_v2::palw_attn_dispute_site_v2` and carried on [`Self::anchor_covered_decode_call`].
/// This module used to say the rule "stays" in `palw_step_refute::verify_kv_anchor`; that
/// function has exactly one caller, on the ARITHMETIC refutation path, and nothing on the
/// dissection path ever asked it. A challenger free to choose among anchors is a challenger
/// choosing which positions the court never sees — and, on a per-position class, choosing a
/// STRADDLE that admits the unsound cache-write rows the straddle's residue is opened with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAttnBottomSiteV1 {
    pub params: A16AttnFusedParamsV1,
    /// `attn_kv_heads × attn_head_dim` — the cache row's width in codes.
    pub kv_dim: usize,
    /// The disputed head's slice within a cache row.
    pub kv_off: usize,
    pub d_head: usize,
    /// The attention layer in the PROFILE's numbering, which is what the map indexes by.
    pub attn_layer: u16,
    /// **The disputed leaf's OWN coordinate**, from the canonical enumeration — the call, the
    /// fused node's slot, the position within that call and the output tile. Every other
    /// coordinate the bottom opens is derived from it, and the committed output tile the bottom
    /// carries must BE it.
    pub disputed: crate::palw_step::PalwStepCoordinateV1,
    /// The job's declared prefill, so a HISTORY position's own `(call, position)` follows:
    /// `j < prefill` is `(0, j)` and `j ≥ prefill` is `(j − prefill + 1, 0)` — the canonical
    /// enumeration's rule, not a second one.
    pub prefill_positions: u32,
    /// **The rotated-query node**: its global slot, the tile of its committed row that holds this
    /// head's slice, that tile's width in lanes, and where the head's `d_head` codes start inside
    /// it. Without these the bottom read ANY committed `d_head`-wide leaf as the query — another
    /// head's row, another position's — and convicted an honest executor on genuine material.
    pub query_slot: u32,
    pub query_tile_index: u32,
    pub query_tile_lanes: usize,
    pub query_lane_offset: usize,
    /// **The nodes that WRITE the K and the V cache**, as global slots — the `KCacheWrite` /
    /// `VCacheWrite` roles of the layer's own table, which is the same resolution the K/V input
    /// sentinels take in `palw_step_refute`. `None` when the class declares no such node, and
    /// then the cache-write route is refused by name rather than left unbound.
    pub k_slot: Option<u32>,
    pub v_slot: Option<u32>,
    /// The tile width of those nodes' committed rows. One opened leaf is one cache row only when
    /// it equals `kv_dim`; a narrower tile means a row is several leaves, which this bottom's
    /// one-opening-per-position shape cannot carry, and it is refused by name.
    pub kv_tile_lanes: usize,
    /// **The checkpoint this step's evidence must anchor at** —
    /// `palw_checkpoint_covered_for_step_v1` of the disputed coordinate. `None` where the class
    /// commits no checkpoint covering the disputed position at all (the prefill of a
    /// per-decode-call leg), and then a `Checkpoint` arm has no legal anchor.
    pub anchor_covered_decode_call: Option<u32>,
    /// The anchor's layout under the class's TILED map (`tiled_kv_state_geometry_v3`), at
    /// `anchor_positions` positions. `None` when no checkpoint precedes the disputed position, and
    /// then a `Checkpoint` evidence arm is refused by name rather than guessed at.
    pub anchor_geometry: Option<PalwStateChunkGeometryV1>,
    pub anchor_positions: u32,
    /// **Does this class commit a checkpoint after EVERY position?**
    /// (`palw_context_ladder::palw_checkpoint_cadence_v1` of the profile — read by the caller, like
    /// every other field here.)
    ///
    /// When it does, the cache-write evidence route is refused
    /// ([`PalwAttnCourtError::CacheWriteRouteRefused`]): it is unsound, and the class has a sound
    /// route at every position it could be disputed at. When it does not, the route stays open,
    /// because a per-decode-call leg has prefill positions no checkpoint covers and a court that
    /// refused both routes there would refuse to try the leaf at all.
    pub every_position_is_checkpointed: bool,
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
    ///
    /// **`site` is the leaf the LADDER terminated on**, as `(head, lane_first, lane_count)` read
    /// off that leaf's step coordinate by the caller. Without it a responder could answer a
    /// dispute about one head with a truthful claim about another and be acquitted for it: the
    /// root claim names its own head, so something outside the claim has to say which head was
    /// asked about. The caller derives it from the terminal leaf and the class's profile; this
    /// machine refuses the mismatch by name.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        session_id: Hash64,
        root: &PalwAttnRootClaimV1,
        site: (u16, u16, u16),
        history_positions: u32,
        out_tile: &[i32],
        values: crate::palw_base0_a16::A16QuantParams,
        court: &PalwCourtParamsV2,
        tile_positions: u32,
        opened_at_daa: u64,
        w_round: u64,
        kary_court_active: bool,
    ) -> Result<Self, PalwAttnCourtError> {
        Self::open_with_arity(
            session_id,
            root,
            site,
            history_positions,
            out_tile,
            values,
            court.dissection_arity(),
            tile_positions,
            opened_at_daa,
            w_round,
            kary_court_active,
        )
    }

    /// [`Self::open`] with the arity stated directly rather than read off a court.
    ///
    /// The chain's transition has the arity — the object declares it and the acceptance layer
    /// refuses any value but the ruleset's derived one — and does NOT have a
    /// `PalwCourtParamsV2`, which is a bundle quantity the fold never receives. Building a
    /// throwaway court there to read one field back out of it would be a second, fake ruleset
    /// inside the transition; this is the same constructor without it, and `open` is now its one
    /// caller plus a name for the court-shaped call.
    #[allow(clippy::too_many_arguments)]
    pub fn open_with_arity(
        session_id: Hash64,
        root: &PalwAttnRootClaimV1,
        site: (u16, u16, u16),
        history_positions: u32,
        out_tile: &[i32],
        values: crate::palw_base0_a16::A16QuantParams,
        arity: u8,
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
        if (root.head, root.lane_first, root.lane_count) != site {
            return Err(PalwAttnCourtError::WrongSite {
                got_head: root.head,
                got_first: root.lane_first,
                got_count: root.lane_count,
                head: site.0,
                first: site.1,
                count: site.2,
            });
        }
        // **The history is the CHAIN's, not the responder's** (audit A C-1 / E C-2).
        //
        // `history_positions` is the length of the history the whole dissection is played over —
        // the tile space, the round budget, and which positions the bottom must open. The caller
        // derives it from the terminal leaf's own coordinate and the job context (the canonical
        // enumeration's `p + 1` inside the prefill, `prefill + call` afterwards) and it was
        // computed and thrown away: the phase took the length from the wire. A responder that
        // TRUNCATED it answered honestly about a shorter row and was acquitted; one that inflated
        // it dissected into positions no committed leaf and no checkpoint chunk can serve, so the
        // honest challenger could never close, and the backstop closes challenger-side. Neither
        // needs any arithmetic skill. The whole-row arm for this same node states the rule and the
        // reason ("the series must be the challenged position's OWN history, exactly").
        if root.history_positions != history_positions {
            return Err(PalwAttnCourtError::WrongHistory { got: root.history_positions, expected: history_positions });
        }
        // **`S*` is a divisor, and a divisor off the wire is an unbounded one** (audit A C-2 /
        // E C-1). The band is derivable from pinned constants and the history just pinned above:
        // the row max contributes exactly `int_exp(0)`, so an honest sum is at least that, and no
        // position contributes more than `int_exp(0)`, so it is at most `positions × int_exp(0)`.
        // Below the band `int_recip(S*)` is large enough that the bottom's `e × recip` leaves
        // `i64` — a panic in block validation before the kernel was made total — and a
        // non-positive one made the close undeliverable, so the accuser lost a case it had won.
        let unit = i64::from(crate::palw_base0::int_exp(0));
        let ceiling = unit.saturating_mul(i64::from(root.history_positions));
        if root.claim.exp_sum < unit || root.claim.exp_sum > ceiling {
            return Err(PalwAttnCourtError::ExponentSumOutOfBand {
                got: root.claim.exp_sum,
                min: unit,
                max: ceiling,
                positions: root.history_positions,
            });
        }
        // **The claim is checked against the execution before it is played against.**
        if out_tile.len() != lanes || a16_attn_finalize_v1(&root.claim.v_acc, values) != out_tile {
            return Err(PalwAttnCourtError::RootDoesNotFinalize);
        }
        let tile_count = (root.history_positions as u64).div_ceil(tile_positions as u64);
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

    /// Positions a history tile holds — the court's tile, which the class's map must agree with
    /// for a checkpoint chunk to serve the bottom (ADR-0082 Decision 4).
    pub fn tile_positions(&self) -> u32 {
        self.tile_positions
    }

    /// **Pull this rung's deadline back inside the session's own assembly reserve** (ADR-0080 W4),
    /// with `PalwBisectLadderV1::cap_deadline_to_session_v1`'s rule verbatim.
    ///
    /// The dissection's bottom is a CLOSE, and a close may have to be assembled over several
    /// blocks. Those blocks come out of the session, so a rung allowed to run to the whole-session
    /// backstop spends the assembly room the move it is waiting for will need. Never raised, only
    /// lowered; the deadline in force after the cap is returned so a caller can index on it
    /// without reading the field back.
    pub fn cap_deadline_to_session_v1(&mut self, session_deadline_daa: u64, assembly_reserve_daa: u64) -> u64 {
        let cap = session_deadline_daa.saturating_sub(assembly_reserve_daa);
        if cap < self.last_deadline_daa {
            self.last_deadline_daa = cap;
        }
        self.last_deadline_daa
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

/// **The same reading, for the caller that opens the committed OUTPUT row before a round is
/// played** (ADR-0082 Decision 2, step 1).
///
/// The root claim is admitted only if its `V*` finalizes to the row the execution committed, and
/// "the row the execution committed" is a leaf of the claim's step tree like any other — so the
/// arm that opens the phase reads it through the same function the bottom reads its operands
/// with. A second spelling of "opened" is a second definition of what the court trusts.
pub fn palw_attn_opened_lanes_v1(
    row: &PalwAttnRowOpeningV1,
    binding: &PalwAttnBottomBindingV1,
    expected_lanes: usize,
) -> Result<Vec<i32>, PalwAttnCourtError> {
    opened_lanes_v1(row, binding, expected_lanes)
}

/// **The `(call, position)` a HISTORY position was written at** — the canonical enumeration's own
/// rule, not a second one: the prefill call holds positions `0..P`, and decode call `c` holds the
/// single position `P + c − 1`.
///
/// The check this replaces compared `coord.position` alone, which is right inside the prefill and
/// WRONG at every decode position — where the canonical position is `0` and the call carries the
/// history index. A tile straddling the prefill boundary therefore refused honest evidence on one
/// side and accepted a row from the wrong call on the other.
fn history_row_coord_v1(prefill: u32, slot: u32, j: u64) -> crate::palw_step::PalwStepCoordinateV1 {
    let (call_index, position) = if j < u64::from(prefill) { (0u32, j as u32) } else { ((j - u64::from(prefill) + 1) as u32, 0u32) };
    crate::palw_step::PalwStepCoordinateV1 { call_index, node_slot: slot, position, tile_index: 0 }
}

/// **Every row of a series, checked to BE the row it stands for** (ADR-0082 Decision 2, step 3).
///
/// `opened_lanes_v1` proves a leaf is IN the claim's step tree. It does not prove WHICH row of the
/// cache it is, and the recompute below is position-ordered — so without this a challenger could
/// open sixteen committed rows from anywhere in the history, recompute a triple that is not the
/// disputed tile's, and take `ExecutorGuilty` against an execution that was honest. The evidence
/// has to say what it is, not merely that it exists.
///
/// What is compared is the WHOLE committed coordinate — call, node slot, position and tile — of
/// every row, against the coordinate the CLASS puts that history position at. The node slot is
/// the class's `KCacheWrite` / `VCacheWrite` node (`site.k_slot` / `site.v_slot`), resolved by the
/// same role lookup the K and V input sentinels take in `palw_step_refute`. The module used to
/// say a graph "never names the nodes that WRITE" its caches and that a bottom swapping the two
/// series wholesale was therefore admissible; the roles are exactly that name, and with the slot
/// compared the swap opens rows at coordinates the site refuses. A class that declares no such
/// node has no cache-write route at all and is refused by name.
fn opened_series_v1(
    rows: &[PalwAttnRowOpeningV1],
    binding: &PalwAttnBottomBindingV1,
    site: &PalwAttnBottomSiteV1,
    slot: u32,
    what: &'static str,
    first_position: u64,
) -> Result<Vec<i32>, PalwAttnCourtError> {
    // One opened leaf is one cache row only when the writing node's tile IS the row. A narrower
    // tile makes a row several leaves, which this bottom's one-opening-per-position shape cannot
    // carry — said here, by name, rather than discovered as a width mismatch.
    if !rows.is_empty() && site.kv_tile_lanes != site.kv_dim {
        return Err(PalwAttnCourtError::CacheRowIsNotOneLeaf { kv_dim: site.kv_dim, tile: site.kv_tile_lanes });
    }
    let mut out = Vec::with_capacity(rows.len() * site.kv_dim);
    for (i, row) in rows.iter().enumerate() {
        let expected = history_row_coord_v1(site.prefill_positions, slot, first_position + i as u64);
        if row.leaf.coord != expected {
            return Err(PalwAttnCourtError::WrongCoordinate { what, got: row.leaf.coord, expected });
        }
        out.extend(opened_lanes_v1(row, binding, site.kv_dim)?);
    }
    Ok(out)
}

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

/// **Verify the anchor's checkpoint leaf against the claim's checkpoint leg**, and answer with
/// the leaf. What it does NOT answer is whether this is the RIGHT checkpoint for the disputed
/// step — see [`PalwAttnBottomSiteV1`].
fn verified_anchor_v1<'a>(
    anchor: Option<&'a PalwAttnCheckpointAnchorV1>,
    binding: &PalwAttnBottomBindingV1,
    site: &'a PalwAttnBottomSiteV1,
) -> Result<(&'a PalwCheckpointLeafV2, &'a PalwStateChunkGeometryV1), PalwAttnCourtError> {
    let anchor = anchor.ok_or(PalwAttnCourtError::AnchorMissing)?;
    let geometry = site.anchor_geometry.as_ref().ok_or(PalwAttnCourtError::AnchorGeometryMissing)?;
    // **The anchor is the DERIVED checkpoint, exactly** (audit A C-4). Nothing bound
    // `covered_decode_call` to `palw_checkpoint_covered_for_step_v1`, so a bottom could anchor at
    // any checkpoint of the leg whose geometry happened to fit — and on a per-position class,
    // where one exists at every position, an anchor one position INSIDE the disputed tile makes
    // the tile straddle its edge and re-opens the residue as cache-write rows, which is the route
    // the class is refused. Exact and not "at most", for `verify_kv_anchor`'s own reason: a
    // challenger choosing among anchors is choosing which positions the court never sees.
    let want = site.anchor_covered_decode_call.ok_or(PalwAttnCourtError::AnchorCoversNothing)?;
    if anchor.leaf.covered_decode_call != want {
        return Err(PalwAttnCourtError::WrongAnchor { got: anchor.leaf.covered_decode_call, expected: want });
    }
    if geometry.positions != site.anchor_positions {
        return Err(PalwAttnCourtError::AnchorGeometryDoesNotDescribeTheAnchor {
            positions: site.anchor_positions,
            geometry: geometry.positions,
        });
    }
    if u64::from(anchor.leaf.state_chunk_count) != geometry.chunk_count() {
        return Err(PalwAttnCourtError::AnchorChunkCountMismatch {
            declared: anchor.leaf.state_chunk_count,
            derived: geometry.chunk_count(),
        });
    }
    if anchor.opening.leaf_index != u64::from(anchor.leaf.checkpoint_index) {
        return Err(PalwAttnCourtError::AnchorNotCommitted);
    }
    if checkpoint_leaf_hash_v2(&binding.job_context_hash, &binding.checkpoint_profile_hash, &binding.state_chunk_map_id, &anchor.leaf)
        != anchor.opening.leaf_hash
    {
        return Err(PalwAttnCourtError::AnchorNotCommitted);
    }
    let root = step_opening_root_capped_v1(binding.checkpoint_leaf_count, &anchor.opening, binding.max_step_leaf_count)?;
    if root != binding.checkpoint_merkle_root {
        return Err(PalwAttnCourtError::AnchorNotCommitted);
    }
    Ok((&anchor.leaf, geometry))
}

/// **The tile's rows of one kind, however the bottom reaches them** (ADR-0082 Decision 2, step 3).
///
/// Returns `width × kv_dim` codes, position-major — the layout `a16_attn_tile_triple_v1` reads,
/// and the layout the cache itself has, so a tile opened from the checkpoint is handed over
/// unchanged.
fn tile_rows_v1(
    evidence: &PalwAttnTileEvidenceV1,
    kind: PalwStateChunkKindV1,
    anchor: Option<&PalwAttnCheckpointAnchorV1>,
    binding: &PalwAttnBottomBindingV1,
    site: &PalwAttnBottomSiteV1,
    first_position: u64,
    width: usize,
    court_tile: u32,
) -> Result<Vec<i32>, PalwAttnCourtError> {
    let (slot, what) = match kind {
        PalwStateChunkKindV1::Key => (site.k_slot, "K"),
        PalwStateChunkKindV1::Value => (site.v_slot, "V"),
    };
    let slot = slot.ok_or(PalwAttnCourtError::CacheSeriesNotDeclared { kind: what })?;
    match evidence {
        PalwAttnTileEvidenceV1::CacheWrites { rows } => {
            // **Refused where a sound route exists at every position** — see
            // `PalwAttnCourtError::CacheWriteRouteRefused` for why the route is unsound and why the
            // refusal is conditional rather than absolute.
            if site.every_position_is_checkpointed {
                return Err(PalwAttnCourtError::CacheWriteRouteRefused);
            }
            if rows.len() != width {
                return Err(PalwAttnCourtError::RowsAfterMismatch { covered: 0, width, got: rows.len() });
            }
            opened_series_v1(rows, binding, site, slot, what, first_position)
        }
        PalwAttnTileEvidenceV1::Checkpoint { chunk, rows_after } => {
            let (leaf, geometry) = verified_anchor_v1(anchor, binding, site)?;
            // **The map's tile IS the court's tile.** Decision 4 in one comparison: a class whose
            // map chunks the cache at another granularity cannot serve a dissection's bottom from
            // it, and saying so here is cheaper than reading the wrong rows out of a chunk that
            // happens to contain the right ones.
            if geometry.positions_per_chunk != court_tile {
                return Err(PalwAttnCourtError::MapTileIsNotTheCourtTile { map: geometry.positions_per_chunk, court: court_tile });
            }
            if geometry.row_bytes as usize != site.kv_dim * 4 {
                return Err(PalwAttnCourtError::RowWidthMismatch { got: geometry.row_bytes as usize / 4, expected: site.kv_dim });
            }
            // How much of the tile this checkpoint actually holds. The rest is past its edge —
            // the STRADDLE, which happens whenever the anchor's position count is not a multiple
            // of the tile.
            let covered = (site.anchor_positions as u64).saturating_sub(first_position).min(width as u64) as usize;
            if covered == 0 {
                return Err(PalwAttnCourtError::AnchorCoversNothing);
            }
            if rows_after.len() != width - covered {
                return Err(PalwAttnCourtError::RowsAfterMismatch { covered, width, got: rows_after.len() });
            }
            // **A per-position class has no residue** (audit A C-4, second guard). The derived
            // anchor covers `p + 1` positions and attention at `p` reads exactly `0..=p`, so the
            // tile is wholly inside it; a non-empty `rows_after` there is the cache-write route
            // arriving under the checkpoint arm's name, and it is refused as such. Independent of
            // the anchor check above on purpose — two guards on one hole, because either alone
            // would be an argument rather than a rule.
            if site.every_position_is_checkpointed && !rows_after.is_empty() {
                return Err(PalwAttnCourtError::RowsAfterOnAPerPositionClass { got: rows_after.len() });
            }
            let position =
                u32::try_from(first_position).map_err(|_| PalwAttnCourtError::ChunkRowUnreadable { position: first_position })?;
            let (expected, _) = integer_kv_state_locate_v1(geometry, kind, site.attn_layer, position)
                .ok_or(PalwAttnCourtError::ChunkRowUnreadable { position: first_position })?;
            if u64::from(chunk.chunk_index) != expected {
                return Err(PalwAttnCourtError::WrongChunk { got: chunk.chunk_index, expected, position: first_position });
            }
            // Membership: this chunk, at this index, is in THIS checkpoint's state — proved
            // without the other chunks, which is the whole of Decision 4.
            let chunk_hash = state_chunk_leaf_hash_v1(&binding.state_chunk_map_id, chunk.chunk_index, &chunk.chunk_bytes);
            let folded =
                state_chunk_opening_root_v1(geometry.chunk_count() as usize, chunk.chunk_index, &chunk_hash, &chunk.siblings)?;
            if folded != leaf.state_chunks_root {
                return Err(PalwAttnCourtError::ChunkNotInCheckpoint { folded, claimed: leaf.state_chunks_root });
            }
            let entry = integer_kv_state_chunk_entry_v1(geometry, expected)
                .ok_or(PalwAttnCourtError::ChunkRowUnreadable { position: first_position })?;
            if u64::from(entry.position_start) != first_position {
                return Err(PalwAttnCourtError::ChunkIsNotTheTile {
                    chunk: chunk.chunk_index,
                    start: entry.position_start,
                    tile_start: first_position,
                });
            }
            let mut out = Vec::with_capacity(width * site.kv_dim);
            for i in 0..covered {
                let p = position + i as u32;
                let bytes = integer_kv_state_row_v1(&entry, &chunk.chunk_bytes, p)
                    .ok_or(PalwAttnCourtError::ChunkRowUnreadable { position: u64::from(p) })?;
                out.extend(bytes.chunks_exact(4).map(|q| i32::from_le_bytes([q[0], q[1], q[2], q[3]])));
            }
            let tail = opened_series_v1(rows_after, binding, site, slot, what, first_position + covered as u64)?;
            out.extend(tail);
            Ok(out)
        }
    }
}

/// **The bottom of the dissection: open one tile and recompute its triple** (ADR-0082 Decision 2,
/// step 3).
///
/// The site is the registered CLASS's description ([`PalwAttnBottomSiteV1`]) — the cache row's
/// width, the head's slice, the layer, the fused narrowings, and the anchor's tiled layout — never
/// the wire's.
///
/// The recompute is [`a16_attn_tile_triple_v1`] — the shipped kernels — against the ROOT's
/// `(m*, S*)`, and all three fields are compared. Returns `ExecutorGuilty` when any of them
/// disagrees with the claim the dissection narrowed to, and `ChallengerDefeated` when all three
/// agree: at that point the responder has stood behind its claim at every level and reproduced it
/// from committed material, which is what an honest execution looks like from the court's side.
///
/// **The two routes give the same answer or neither is used.** Whether a row arrives as a
/// cache-write leaf or out of a checkpoint chunk, it is the same bytes — the engine pushes one
/// `k_rot` into the cache and records the same vector as a step row — so the verdict cannot depend
/// on which one the challenger could afford. `the_two_routes_convict_and_acquit_alike` is what
/// says so.
pub fn check_attn_dissect_bottom_v1(
    phase: &PalwAttnDissectPhaseV1,
    bottom: &PalwAttnDissectBottomV1,
    binding: &PalwAttnBottomBindingV1,
    site: &PalwAttnBottomSiteV1,
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
    let (first_position, width) =
        phase.terminal_tile_positions().ok_or(PalwAttnCourtError::WrongTile { got: bottom.tile, expected: expected_tile })?;

    // **A tile has two series, not one twice.** The two series are the class's `KCacheWrite` and
    // `VCacheWrite` nodes, and a graph in which one node holds both roles commits one series that
    // the recompute would read as two — a conviction built out of honest rows read twice. Asked
    // of the SITE, before any evidence is walked, because it is a property of the class and not
    // of the bottom, and it is the same answer on both routes.
    if site.k_slot.is_some() && site.k_slot == site.v_slot {
        return Err(PalwAttnCourtError::KeyAndValueAreTheSameSeries);
    }
    // **One bottom reads one route** (audit A H-1). A mixed bottom — one kind from the checkpoint,
    // the other from cache writes — is not a case the two routes' equality argument covers, and it
    // was the shape in which the checkpoint arm's `None` slot silently disabled the series
    // comparison that stood at the time. There is no honest reason to file one: both kinds live in
    // the same anchor and are written by the same step.
    if std::mem::discriminant(&bottom.k) != std::mem::discriminant(&bottom.v) {
        return Err(PalwAttnCourtError::MixedEvidenceRoutes);
    }

    // Every operand is opened against something the claim committed before a multiply happens, and
    // "committed" means AT ITS OWN COORDINATE — the class's, derived from the leaf the ladder
    // narrowed to (audit A C-3 / E C-3).
    let q_expected = crate::palw_step::PalwStepCoordinateV1 {
        call_index: site.disputed.call_index,
        node_slot: site.query_slot,
        position: site.disputed.position,
        tile_index: site.query_tile_index,
    };
    if bottom.query.leaf.coord != q_expected {
        return Err(PalwAttnCourtError::WrongCoordinate { what: "query", got: bottom.query.leaf.coord, expected: q_expected });
    }
    let q_row = opened_lanes_v1(&bottom.query, binding, site.query_tile_lanes)?;
    let qh = q_row
        .get(site.query_lane_offset..site.query_lane_offset + site.d_head)
        .ok_or(PalwAttnCourtError::RowWidthMismatch { got: q_row.len(), expected: site.query_lane_offset + site.d_head })?;
    // **The committed output tile is opened too, and it is the disputed leaf** (audit A L-1). The
    // type's own doc says every field of the bottom is "opened against something the claim
    // committed"; this one was carried and never read, `4 × lane_count` bytes plus a whole path of
    // payload the cost gate charges for and nothing checks. It is the fused node's OWN row, so its
    // coordinate is the disputed coordinate exactly.
    if bottom.out_tile.leaf.coord != site.disputed {
        return Err(PalwAttnCourtError::WrongCoordinate {
            what: "output tile",
            got: bottom.out_tile.leaf.coord,
            expected: site.disputed,
        });
    }
    opened_lanes_v1(&bottom.out_tile, binding, phase.lane_count as usize)?;

    let tile = phase.tile_positions();
    let k_tile =
        tile_rows_v1(&bottom.k, PalwStateChunkKindV1::Key, bottom.anchor.as_ref(), binding, site, first_position, width, tile)?;
    let v_tile =
        tile_rows_v1(&bottom.v, PalwStateChunkKindV1::Value, bottom.anchor.as_ref(), binding, site, first_position, width, tile)?;

    let (m_star, s_star) = phase.root_scale();
    let lanes = (phase.lane_first as usize, phase.lane_count as usize);
    let recomputed = a16_attn_tile_triple_v1(qh, &k_tile, &v_tile, site.kv_dim, site.kv_off, lanes, site.params, m_star, s_star)?;
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
    use crate::palw_state_chunk_map::PalwStateChunkKindV1;
    use crate::palw_step::PalwStepCoordinateV1;
    use crate::palw_step_leg::{
        PALW_STEP_LEG_OBJECT_VERSION_V1, checkpoint_genesis_prev_v2, state_chunk_path_v1, state_chunks_root_v1, step_merkle_path_v1,
        step_merkle_root_v1,
    };

    const TILE: u32 = PALW_ATTN_HISTORY_TILE_V4;
    /// The RC's close-chunk count — the assembly reserve the arity derivation now searches under
    /// (audit D H-2c): the derivation and `palw_attn_court_admits_row_v1` must apply ONE
    /// inequality, and the reserve is a property of the window both of them read.
    const CHUNKS: u64 = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS;

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
        kv_dim: usize,
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
            kv_dim,
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
                // Filled in per anchor: a fixture with no checkpoint carries a leg root nothing
                // opens against, which is exactly what `AnchorNotCommitted` is for.
                checkpoint_merkle_root: h64(0),
                checkpoint_leaf_count: 2,
                checkpoint_profile_hash: h64(CKPT),
                state_chunk_map_id: crate::palw_state_chunk_map::tiled_kv_state_chunk_map_id_v3(),
            },
            leaf_hashes,
        }
    }

    const CKPT: u8 = 0x33;

    /// A checkpoint over the fixture's first `anchor_positions` positions, enumerated under the
    /// class's TILED map — the material ADR-0082 Decision 4's bottom reads one chunk of.
    struct Anchor {
        geometry: PalwStateChunkGeometryV1,
        chunk_bytes: Vec<Vec<u8>>,
        chunk_hashes: Vec<Hash64>,
        anchor: PalwAttnCheckpointAnchorV1,
        checkpoint_merkle_root: Hash64,
        positions: u32,
    }

    impl Fixture {
        /// Build the checkpoint. The geometry is the v3 map's shape stated for this synthetic
        /// head: `positions_per_chunk` is the registered tile, the row is `kv_dim × 4` i32-LE
        /// bytes, and the chunks are kind-major then layer-asc then position-asc — the enumeration
        /// `integer_kv_state_chunk_entry_v1` walks, which is what builds the bytes below.
        fn anchor(&self, anchor_positions: u32) -> Anchor {
            let geometry = PalwStateChunkGeometryV1 {
                row_bytes: (self.kv_dim * 4) as u32,
                positions_per_chunk: TILE,
                attn_layers: vec![LAYER],
                positions: anchor_positions,
                chunks_per_slice: anchor_positions.div_ceil(TILE),
            };
            let mut chunk_bytes = Vec::new();
            for index in 0..geometry.chunk_count() {
                let entry = integer_kv_state_chunk_entry_v1(&geometry, index).expect("in range");
                let series = match entry.kind {
                    PalwStateChunkKindV1::Key => &self.k,
                    PalwStateChunkKindV1::Value => &self.v,
                };
                let mut bytes = Vec::with_capacity(entry.byte_len() as usize);
                for p in entry.position_start..entry.position_start + entry.position_count {
                    for lane in &series[p as usize * self.kv_dim..(p as usize + 1) * self.kv_dim] {
                        bytes.extend_from_slice(&lane.to_le_bytes());
                    }
                }
                chunk_bytes.push(bytes);
            }
            let map_id = crate::palw_state_chunk_map::tiled_kv_state_chunk_map_id_v3();
            let chunk_hashes: Vec<Hash64> =
                chunk_bytes.iter().enumerate().map(|(i, b)| state_chunk_leaf_hash_v1(&map_id, i as u32, b)).collect();
            let leaf = PalwCheckpointLeafV2 {
                version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                checkpoint_index: 0,
                covered_decode_call: 0,
                prev_checkpoint_leaf_hash: checkpoint_genesis_prev_v2(&h64(JOB)),
                state_chunk_count: chunk_hashes.len() as u32,
                state_chunks_root: state_chunks_root_v1(&chunk_hashes).expect("a chunks root"),
            };
            let leaf_hash = checkpoint_leaf_hash_v2(&h64(JOB), &h64(CKPT), &map_id, &leaf);
            // Two checkpoints in the leg, so the opening carries a real sibling.
            let leg = vec![leaf_hash, h64(0xee)];
            Anchor {
                checkpoint_merkle_root: step_merkle_root_v1(&leg).expect("a leg root"),
                anchor: PalwAttnCheckpointAnchorV1 {
                    leaf,
                    opening: PalwStepOpeningV1 { leaf_index: 0, leaf_hash, siblings: step_merkle_path_v1(&leg, 0).expect("a path") },
                },
                geometry,
                chunk_bytes,
                chunk_hashes,
                positions: anchor_positions,
            }
        }

        /// The class's description of the disputed site, with no checkpoint in evidence.
        fn site(&self) -> PalwAttnBottomSiteV1 {
            PalwAttnBottomSiteV1 {
                every_position_is_checkpointed: false,
                params: params(),
                kv_dim: self.kv_dim,
                kv_off: 0,
                d_head: self.d_head,
                attn_layer: LAYER,
                // The fixture's leaf layout, as the class would describe it: the query at slot 0,
                // the K series at 1, the V series at 2 and the fused row at 3, one tile each. All
                // of its history is PREFILL, so every history position's own coordinate is
                // `(call 0, position j)`.
                disputed: PalwStepCoordinateV1 { call_index: 0, node_slot: 3, position: 0, tile_index: 0 },
                prefill_positions: self.positions as u32,
                query_slot: 0,
                query_tile_index: 0,
                query_tile_lanes: self.d_head,
                query_lane_offset: 0,
                k_slot: Some(1),
                v_slot: Some(2),
                kv_tile_lanes: self.kv_dim,
                anchor_covered_decode_call: None,
                anchor_geometry: None,
                anchor_positions: 0,
            }
        }

        /// The same, with the anchor's tiled layout.
        fn site_anchored(&self, anchor: &Anchor) -> PalwAttnBottomSiteV1 {
            PalwAttnBottomSiteV1 {
                anchor_geometry: Some(anchor.geometry.clone()),
                anchor_positions: anchor.positions,
                anchor_covered_decode_call: Some(anchor.anchor.leaf.covered_decode_call),
                ..self.site()
            }
        }

        /// The binding, with the anchor's checkpoint leg root.
        fn binding_anchored(&self, anchor: &Anchor) -> PalwAttnBottomBindingV1 {
            PalwAttnBottomBindingV1 { checkpoint_merkle_root: anchor.checkpoint_merkle_root, ..self.binding }
        }

        /// **The bottom on the CHECKPOINT route**: one chunk opening per kind, plus the rows past
        /// the anchor's edge when the tile straddles it.
        fn bottom_anchored(&self, session_id: Hash64, tile: u64, anchor: &Anchor) -> PalwAttnDissectBottomV1 {
            let first = tile as usize * TILE as usize;
            let width = (self.positions - first).min(TILE as usize);
            let covered = (anchor.positions as usize).saturating_sub(first).min(width);
            let evidence = |kind: PalwStateChunkKindV1, series: &[i32], slot: u32| {
                let (index, _) = integer_kv_state_locate_v1(&anchor.geometry, kind, LAYER, first as u32).expect("in the map");
                let chunk = PalwAttnChunkOpeningV1 {
                    chunk_index: index as u32,
                    chunk_bytes: anchor.chunk_bytes[index as usize].clone(),
                    siblings: state_chunk_path_v1(&anchor.chunk_hashes, index as usize).expect("a chunk path"),
                };
                let rows_after = (first + covered..first + width)
                    .map(|p| {
                        let leaf_index = if slot == 1 { leaf_index_k(p) } else { leaf_index_v(p) };
                        self.row_opening(leaf_index, &series[p * self.kv_dim..(p + 1) * self.kv_dim], slot, p as u32)
                    })
                    .collect();
                PalwAttnTileEvidenceV1::Checkpoint { chunk, rows_after }
            };
            PalwAttnDissectBottomV1 {
                version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                session_id,
                tile,
                query: self.row_opening(leaf_index_query(), &self.q, 0, 0),
                anchor: Some(anchor.anchor.clone()),
                k: evidence(PalwStateChunkKindV1::Key, &self.k, 1),
                v: evidence(PalwStateChunkKindV1::Value, &self.v, 2),
                out_tile: self.row_opening(self.leaf_hashes.len() - 1, &self.out_tile, 3, 0),
            }
        }
    }

    /// The one attention layer these fixtures have. It is the PROFILE's numbering, which is what
    /// the map indexes by — a fact the checker reads off the site rather than counting.
    const LAYER: u16 = 0;

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
                anchor: None,
                k: PalwAttnTileEvidenceV1::CacheWrites { rows: k_rows },
                v: PalwAttnTileEvidenceV1::CacheWrites { rows: v_rows },
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
        let site = (root.head, root.lane_first, root.lane_count);
        PalwAttnDissectPhaseV1::open(
            h64(9),
            &root,
            site,
            fx.positions as u32,
            &fx.out_tile,
            params().values,
            &court_at(arity),
            TILE,
            100,
            30,
            true,
        )
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
                .apply_choice(
                    &PalwAttnDissectChoiceV1 {
                        version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                        session_id: h64(9),
                        round: phase.round(),
                        child,
                    },
                    daa,
                    30,
                )
                .expect("a choice inside the arity");
            assert!(phase.round() <= phase.round_budget(), "arity {arity}: the dissection outran its own round bound");
        }
        let tile = phase.terminal_tile().expect("a narrowed dissection");
        let bottom = fx.bottom(h64(9), tile);
        match check_attn_dissect_bottom_v1(phase, &bottom, &fx.binding, &fx.site(), true).expect("the bottom's openings verify") {
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
                let played = play(fx, arity, &mut phase, &claims);
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
        let PalwAttnTileEvidenceV1::CacheWrites { rows } = &mut bottom.k else { panic!("the cache-write route") };
        rows[0].leaf.values_le[0] ^= 0xff;
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &bottom, &fx.binding, &fx.site(), true),
            Err(PalwAttnCourtError::RowNotCommitted)
        );
        // And the same bottom against the wrong tile is refused by name rather than recomputed.
        let mut elsewhere = fx.bottom(h64(9), tile);
        elsewhere.tile = tile + 1;
        assert!(matches!(
            check_attn_dissect_bottom_v1(&phase, &elsewhere, &fx.binding, &fx.site(), true),
            Err(PalwAttnCourtError::WrongTile { .. })
        ));
    }

    /// **The cache-write route swaps K for V undetected — and a class that checkpoints every
    /// position no longer has to offer it** (ADR-0082 Decision 4, amended; stream I's finding).
    ///
    /// A graph declares its cache reads as the `KV_K` / `KV_V` input SENTINELS and never names the
    /// nodes that WRITE them, so a `CacheWrites` bottom carries no binding between an opened row
    /// and the series it is supposed to be. Every row in the swapped bottom below is a row the
    /// claim really committed, at a coordinate the claim really has — the openings all verify —
    /// and the recompute is handed V where it asked for K.
    ///
    /// The first half of this test MEASURES that gap, so it is a fact rather than an argument. The
    /// second half is the answer this stream makes available: with a checkpoint after every
    /// position the class has a sound route at every position it could be disputed at, and the
    /// unsound one is refused BY NAME rather than left as the challenger's choice.
    #[test]
    fn the_cache_write_route_cannot_bind_a_row_to_its_series_and_is_refused_where_a_checkpoint_exists() {
        let fx = fixture(8, 50, 4, 23);
        let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
        let (m, s) = phase.root_scale();
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

        // The honest bottom acquits on the shipped (per-decode-call) cadence.
        let honest = fx.bottom(h64(9), tile);
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &fx.binding, &fx.site(), true),
            Ok(PalwAttnCourtVerdictV1::ChallengerDefeated),
            "the honest execution must be acquitted"
        );

        // **The gap, CLOSED — and this assertion is the record of the move** (audit A C-3/H-1).
        // Until the rows were bound to their committed coordinates, swapping the two series
        // wholesale verified every opening (nothing said which series a row belonged to) and
        // convicted an honest executor. The class names its cache writers by role, so the K rows
        // now have to stand at the K writer's slot, and the swap is refused by name at the first
        // row instead of producing a verdict.
        let mut swapped = fx.bottom(h64(9), tile);
        std::mem::swap(&mut swapped.k, &mut swapped.v);
        let verdict = check_attn_dissect_bottom_v1(&phase, &swapped, &fx.binding, &fx.site(), true);
        assert!(
            matches!(verdict, Err(PalwAttnCourtError::WrongCoordinate { what: "K", .. })),
            "the swapped bottom must be refused by the K series' own coordinate, got {verdict:?}"
        );

        // **The answer.** A class that commits a checkpoint after every position is refused the
        // cache-write route entirely, so the swap has nothing to ride on. The honest bottom is
        // refused too, and that is the point: the sound route is available at EVERY position of
        // such a class, so there is nothing the refused route was needed for.
        let per_position = PalwAttnBottomSiteV1 { every_position_is_checkpointed: true, ..fx.site() };
        for (what, bottom) in [("the swapped bottom", &swapped), ("the honest one", &honest)] {
            assert_eq!(
                check_attn_dissect_bottom_v1(&phase, bottom, &fx.binding, &per_position, true),
                Err(PalwAttnCourtError::CacheWriteRouteRefused),
                "{what} must be refused by name on a per-position class"
            );
        }

        // And the CHECKPOINT route is unaffected by the flag — it is the route the refusal points
        // at, and its rows carry `(kind, layer, position)` from the map rather than from the wire.
        let anchor = fx.anchor(fx.positions as u32);
        let anchored = fx.bottom_anchored(h64(9), tile, &anchor);
        let anchored_site = PalwAttnBottomSiteV1 { every_position_is_checkpointed: true, ..fx.site_anchored(&anchor) };
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &anchored, &fx.binding_anchored(&anchor), &anchored_site, true),
            Ok(PalwAttnCourtVerdictV1::ChallengerDefeated),
            "the checkpoint route must still acquit an honest execution on a per-position class"
        );
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
        let site = (0u16, 0u16, 4u16);
        assert_eq!(
            PalwAttnDissectPhaseV1::open(h64(9), &root, site, 32, &fx.out_tile, params().values, &court_at(16), TILE, 100, 30, false),
            Err(PalwAttnCourtError::FenceDormant)
        );
        // A root claim that does not finalize to the opened tile never reaches a round.
        let mut wrong = root.clone();
        wrong.claim.v_acc[0] = i64::MAX / 4;
        assert_eq!(
            PalwAttnDissectPhaseV1::open(h64(9), &wrong, site, 32, &fx.out_tile, params().values, &court_at(16), TILE, 100, 30, true),
            Err(PalwAttnCourtError::RootDoesNotFinalize)
        );
        // And a truthful claim about ANOTHER head does not answer this dispute.
        assert_eq!(
            PalwAttnDissectPhaseV1::open(
                h64(9),
                &root,
                (1, 0, 4),
                32,
                &fx.out_tile,
                params().values,
                &court_at(16),
                TILE,
                100,
                30,
                true
            ),
            Err(PalwAttnCourtError::WrongSite { got_head: 0, got_first: 0, got_count: 4, head: 1, first: 0, count: 4 })
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
        assert_eq!(
            silent.declare_no_show(200),
            Err(PalwAttnCourtError::AlreadyTerminal),
            "an abandoned phase charges no second offense"
        );

        let (m, s) = phase.root_scale();
        let children: Vec<_> = phase.child_ranges().iter().map(|&(f, c)| fx.range_claim(f, c, m, s)).collect();
        phase
            .apply_round(&PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children }, 200, 30)
            .expect("an honest round");
        assert_eq!(phase.turn(), PalwBisectTurnV1::AwaitVerdict);
        assert_eq!(phase.declare_no_show(231).expect("the challenger is now due").silent_party, PalwBisectPartyV1::Challenger);
    }

    // ---------------------------------------------------------------------------------------
    // ADR-0082 Decision 4 — the bottom opens a TILE of the checkpoint, and the anchored fused
    // refutation is driven end to end
    // ---------------------------------------------------------------------------------------

    /// Walk an honest dissection to a NAMED tile — the challenger steering toward the tile the
    /// test wants to open, which is what a real one does toward its divergence.
    fn walk_to(fx: &Fixture, phase: &mut PalwAttnDissectPhaseV1, target: u64) {
        let (m, s) = phase.root_scale();
        let mut daa = 400u64;
        while phase.turn() != PalwBisectTurnV1::Terminal {
            let ranges = phase.child_ranges();
            let children: Vec<_> = ranges.iter().map(|&(f, c)| fx.range_claim(f, c, m, s)).collect();
            daa += 1;
            phase
                .apply_round(&PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children }, daa, 30)
                .expect("an honest round folds");
            let child = ranges.iter().position(|&(f, c)| target >= f && target < f + c).expect("the target is inside the range") as u8;
            daa += 1;
            phase
                .apply_choice(
                    &PalwAttnDissectChoiceV1 {
                        version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                        session_id: h64(9),
                        round: phase.round(),
                        child,
                    },
                    daa,
                    30,
                )
                .expect("a child inside the arity");
        }
        assert_eq!(phase.terminal_tile(), Some(target), "the walk did not reach the tile it steered for");
    }

    /// **The anchored fused refutation, driven** — ADR-0082 Decision 4, and the worry stream D
    /// left open ("an anchored fused refutation was never driven").
    ///
    /// The checkpoint covers 40 of the job's 50 positions, so of the four history tiles: tile 0
    /// and tile 1 are wholly inside it and open as ONE chunk each; **tile 2 STRADDLES its edge** —
    /// eight rows out of the chunk and eight from cache-write leaves after it — and tile 3 is
    /// wholly past it and has only the cache-write route. Every one of them recomputes the same
    /// triple the dissection narrowed to, so an honest execution is acquitted through whichever
    /// route the evidence took.
    #[test]
    fn the_checkpoint_route_opens_one_tile_at_the_anchor_and_across_its_edge() {
        let fx = fixture(8, 50, 4, 53);
        assert_eq!(fx.tile_count(), 4);
        let anchor = fx.anchor(40);
        let binding = fx.binding_anchored(&anchor);
        let site = fx.site_anchored(&anchor);
        for (tile, straddles) in [(0u64, false), (1, false), (2, true)] {
            let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
            walk_to(&fx, &mut phase, tile);
            let bottom = fx.bottom_anchored(h64(9), tile, &anchor);
            let PalwAttnTileEvidenceV1::Checkpoint { chunk, rows_after } = &bottom.k else { panic!("the checkpoint route") };
            assert_eq!(!rows_after.is_empty(), straddles, "tile {tile}: the straddle is not where the anchor's edge is");
            if straddles {
                assert_eq!(rows_after.len(), 8, "tile 2 reaches eight positions past a 40-position checkpoint");
                assert_eq!(chunk.chunk_bytes.len(), 8 * fx.kv_dim * 4, "the last chunk of a ragged checkpoint is short");
            } else {
                assert_eq!(chunk.chunk_bytes.len(), TILE as usize * fx.kv_dim * 4, "a full chunk is one whole tile of rows");
            }
            assert_eq!(
                check_attn_dissect_bottom_v1(&phase, &bottom, &binding, &site, true).expect("the anchored openings verify"),
                PalwAttnCourtVerdictV1::ChallengerDefeated,
                "tile {tile}: an honest execution was convicted through the checkpoint route"
            );
        }
        // The tile past the anchor has only the cache-write route, and it acquits too.
        let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
        walk_to(&fx, &mut phase, 3);
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &fx.bottom(h64(9), 3), &binding, &site, true).expect("verifies"),
            PalwAttnCourtVerdictV1::ChallengerDefeated
        );
    }

    /// **The two routes convict and acquit alike.** A row is the same bytes whether it arrives as
    /// a cache-write leaf or out of a checkpoint chunk — the engine pushes one `k_rot` into the
    /// cache and records the same vector as a step row — so a verdict may not depend on which
    /// route the challenger could afford. Honest and forged, at every tile inside the anchor.
    #[test]
    fn the_two_routes_convict_and_acquit_alike() {
        let fx = fixture(8, 50, 4, 57);
        let anchor = fx.anchor(48);
        let binding = fx.binding_anchored(&anchor);
        let site = fx.site_anchored(&anchor);
        // Honest.
        for tile in [0u64, 1, 2] {
            let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
            walk_to(&fx, &mut phase, tile);
            let cache = check_attn_dissect_bottom_v1(&phase, &fx.bottom(h64(9), tile), &binding, &site, true).expect("cache-write");
            let ckpt = check_attn_dissect_bottom_v1(&phase, &fx.bottom_anchored(h64(9), tile, &anchor), &binding, &site, true)
                .expect("chunk");
            assert_eq!((cache, ckpt), (PalwAttnCourtVerdictV1::ChallengerDefeated, PalwAttnCourtVerdictV1::ChallengerDefeated));
        }
        // Forged: the lie is moved into tile 1 and compensated in tile 0, so every level folds
        // and only a recompute can tell — through either route.
        let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
        let (m, s) = phase.root_scale();
        let claims = |f: u64, c: u64| {
            let mut claim = fx.range_claim(f, c, m, s);
            if f <= 1 && 1 < f + c {
                claim.v_acc[0] += 1_000;
            }
            if f == 0 {
                claim.v_acc[0] -= 1_000;
            }
            claim
        };
        let mut daa = 500u64;
        while phase.turn() != PalwBisectTurnV1::Terminal {
            let ranges = phase.child_ranges();
            let children: Vec<_> = ranges.iter().map(|&(f, c)| claims(f, c)).collect();
            daa += 1;
            phase
                .apply_round(
                    &PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children: children.clone() },
                    daa,
                    30,
                )
                .expect("a compensated round folds");
            let child = ranges
                .iter()
                .zip(&children)
                .position(|(&(f, c), claimed)| *claimed != fx.range_claim(f, c, m, s))
                .expect("a forged child exists") as u8;
            daa += 1;
            phase
                .apply_choice(
                    &PalwAttnDissectChoiceV1 {
                        version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                        session_id: h64(9),
                        round: phase.round(),
                        child,
                    },
                    daa,
                    30,
                )
                .expect("a child");
        }
        let tile = phase.terminal_tile().expect("narrowed");
        assert!(tile <= 1, "the forgery lives in tile 0 or 1, inside the checkpoint");
        let cache = check_attn_dissect_bottom_v1(&phase, &fx.bottom(h64(9), tile), &binding, &site, true).expect("cache-write");
        let ckpt =
            check_attn_dissect_bottom_v1(&phase, &fx.bottom_anchored(h64(9), tile, &anchor), &binding, &site, true).expect("chunk");
        assert_eq!(
            (cache, ckpt),
            (PalwAttnCourtVerdictV1::ExecutorGuilty, PalwAttnCourtVerdictV1::ExecutorGuilty),
            "the two routes disagreed about a forgery"
        );
    }

    /// **What the checkpoint route may not be**, each refused by name and before any arithmetic:
    /// a chunk that is not in the checkpoint, a chunk from another index, an anchor that is not in
    /// the claim's leg, a missing anchor or geometry, a map whose tile is not the court's, and a
    /// checkpoint that covers none of the disputed tile.
    #[test]
    fn the_checkpoint_route_refuses_what_the_checkpoint_does_not_hold() {
        let fx = fixture(8, 50, 4, 61);
        let anchor = fx.anchor(40);
        let binding = fx.binding_anchored(&anchor);
        let site = fx.site_anchored(&anchor);
        let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
        walk_to(&fx, &mut phase, 1);
        let honest = fx.bottom_anchored(h64(9), 1, &anchor);
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &binding, &site, true).expect("honest"),
            PalwAttnCourtVerdictV1::ChallengerDefeated
        );

        // A flipped byte is a chunk the checkpoint never held.
        let mut forged = honest.clone();
        let PalwAttnTileEvidenceV1::Checkpoint { chunk, .. } = &mut forged.k else { panic!() };
        chunk.chunk_bytes[0] ^= 0xff;
        assert!(matches!(
            check_attn_dissect_bottom_v1(&phase, &forged, &binding, &site, true),
            Err(PalwAttnCourtError::ChunkNotInCheckpoint { .. })
        ));

        // Another chunk of the SAME checkpoint, at its own index, is still the wrong tile.
        let mut elsewhere = honest.clone();
        let PalwAttnTileEvidenceV1::Checkpoint { chunk, .. } = &mut elsewhere.k else { panic!() };
        chunk.chunk_index = 0;
        chunk.chunk_bytes = anchor.chunk_bytes[0].clone();
        chunk.siblings = state_chunk_path_v1(&anchor.chunk_hashes, 0).expect("a path");
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &elsewhere, &binding, &site, true),
            Err(PalwAttnCourtError::WrongChunk { got: 0, expected: 1, position: 16 })
        );

        // The anchor must be in the claim's own checkpoint leg. `fx.binding` carries a leg root
        // nothing opens against.
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &fx.binding, &site, true),
            Err(PalwAttnCourtError::AnchorNotCommitted)
        );
        // And no anchor, and no geometry, are two different absences.
        let mut orphan = honest.clone();
        orphan.anchor = None;
        assert_eq!(check_attn_dissect_bottom_v1(&phase, &orphan, &binding, &site, true), Err(PalwAttnCourtError::AnchorMissing));
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &binding, &fx.site(), true),
            Err(PalwAttnCourtError::AnchorGeometryMissing)
        );

        // A geometry that describes another history than the anchor's.
        let mut mismatched = site.clone();
        mismatched.anchor_positions = 41;
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &binding, &mismatched, true),
            Err(PalwAttnCourtError::AnchorGeometryDoesNotDescribeTheAnchor { positions: 41, geometry: 40 })
        );

        // A map whose tile is not the court's cannot serve a dissection's bottom (Decision 4).
        let mut narrow = site.clone();
        let g = narrow.anchor_geometry.as_mut().expect("a geometry");
        g.positions_per_chunk = 8;
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &binding, &narrow, true),
            Err(PalwAttnCourtError::MapTileIsNotTheCourtTile { map: 8, court: TILE })
        );

        // A checkpoint that covers none of the disputed tile: the cache-write route is the only
        // one there, and saying so is cheaper than reading rows out of a chunk that predates it.
        let short = fx.anchor(16);
        let mut past = fx.bottom_anchored(h64(9), 0, &short);
        past.tile = 1;
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &past, &fx.binding_anchored(&short), &fx.site_anchored(&short), true),
            Err(PalwAttnCourtError::AnchorCoversNothing)
        );
    }

    /// **A bottom's wire size, pinned PER ROUTE.** The checkpoint route is what Decision 4 buys:
    /// one chunk opening and one path replace sixteen row openings and sixteen paths, per kind.
    #[test]
    fn a_bottoms_wire_size_is_pinned_per_route() {
        let carrier = palw_close_bytes_for_chunks_v1(1);
        let mut sizes = Vec::new();
        for d_head in [128usize, 256] {
            let fx = fixture(d_head, 64, d_head.min(PALW_ATTN_DISSECT_MAX_LANES), 59);
            let anchor = fx.anchor(64);
            let cache = borsh::to_vec(&fx.bottom(h64(9), 1)).expect("serializes").len() as u64;
            let ckpt = borsh::to_vec(&fx.bottom_anchored(h64(9), 1, &anchor)).expect("serializes").len() as u64;
            assert!(cache <= carrier, "d_head {d_head}: the cache-write bottom is {cache} against a carrier of {carrier}");
            assert!(ckpt <= carrier, "d_head {d_head}: the anchored bottom is {ckpt} against a carrier of {carrier}");
            assert!(ckpt < cache, "d_head {d_head}: the tile route must be the cheaper one, and it is {ckpt} against {cache}");
            sizes.push((d_head, cache, ckpt));
        }
        assert_eq!(sizes, vec![(128usize, 37_985u64, 19_027u64), (256, 55_393, 36_435)]);
        // **What the derivation must be evaluated at.** ADR-0082 §4 sizes the bottom's tiles as
        // `2 × 16 × 4 × d_head` — one HEAD's slice — and stream F's derived 25,120 / 42,016 follow
        // it. The object carries `2 × 16 × 4 × kv_dim`, because a checkpoint chunk holds the whole
        // cache ROW and cannot be narrowed to one head: the map addresses `(kind, layer,
        // position)`, not `(kind, layer, position, head)`. These fixtures set `kv_heads = 1`, so
        // the two coincide here and the measured object is inside the derived bound; on a
        // registered row with `kv_heads > 1` the chunk is `kv_heads` times wider and the
        // derivation has to read `attn_kv_heads × attn_head_dim`, not `attn_head_dim`.
        for d_head in [128usize, 256] {
            let fx = fixture(d_head, 64, d_head.min(PALW_ATTN_DISSECT_MAX_LANES), 59);
            let anchor = fx.anchor(64);
            let PalwAttnTileEvidenceV1::Checkpoint { chunk, .. } = &fx.bottom_anchored(h64(9), 1, &anchor).k else { panic!() };
            assert_eq!(chunk.chunk_bytes.len() as u64, u64::from(TILE) * fx.kv_dim as u64 * 4);
            assert_eq!(fx.kv_dim, d_head, "these fixtures are one kv head, which is where the two sizings agree");
        }
    }

    /// **The dissection and the whole-row arm are the same arithmetic** — stream D's hook comment
    /// in `palw_step_refute`'s `Qwen36Op::AttnFused` arm, as a test: "neither may convict where the
    /// other acquits".
    ///
    /// The output tile the dissection's root claim finalizes to IS the slice
    /// `a16_attn_fused_reference_v1` computes for the same head, byte for byte, at every history
    /// length and every ragged tail — so an execution the whole-row arm would acquit is one the
    /// dissection acquits, and the responder's claim is checked against the same row either way.
    #[test]
    fn the_dissection_and_the_whole_row_arm_are_the_same_arithmetic() {
        for d_head in [8usize, 16] {
            for positions in [16usize, 50] {
                let fx = fixture(d_head, positions, 4, 47);
                let row = crate::palw_base0_a16::a16_attn_fused_reference_v1(&fx.q, &fx.k, &fx.v, 1, 1, d_head, params())
                    .expect("the whole-row arm recomputes");
                assert_eq!(
                    fx.out_tile.as_slice(),
                    &row[..fx.lanes.1],
                    "d_head {d_head}, positions {positions}: the dissection's tile is not the whole row's slice"
                );
                let mut phase = open_phase(&fx, 16, fx.root.clone()).expect("an honest root");
                let (m, s) = phase.root_scale();
                assert_eq!(
                    play(&fx, 16, &mut phase, &|f, c| fx.range_claim(f, c, m, s)),
                    Played::Acquitted,
                    "d_head {d_head}, positions {positions}: the dissection convicted what the whole-row arm acquits"
                );
            }
        }
    }

    /// **What it costs a challenger to name a child — the ADR's own worry, counted.**
    ///
    /// The fold binds a disclosure to the parent's claim and to nothing else: a responder is free
    /// to state children that fold but are not the tiles', moving a lie from one child into
    /// another. Nothing catches that at the fold, and nothing is meant to — what catches it is
    /// the challenger, who recomputes each child and names the first that is not what it
    /// computed, and then the bottom, where a recompute is exact.
    ///
    /// So the question is what that recompute costs. The children PARTITION the disputed range,
    /// so computing all `k` of them is ONE pass over the range — not `k` passes — and the ranges
    /// shrink geometrically. The total is `n + n/k + n/k² + … < n·k/(k−1)`, which is at most TWO
    /// passes over the history at the worst arity (2) and 1.07 at sixteen. This test counts the
    /// tile recomputes a whole dissection costs the challenger and holds that bound.
    ///
    /// It is also the argument that a wider `k` is not paid for by the challenger: sixteen
    /// children cost it strictly LESS total work than two, because the rounds fall faster than
    /// the per-round work rises (it does not rise at all).
    #[test]
    fn the_challengers_cost_to_name_a_child_is_under_two_passes_over_the_history() {
        use std::cell::Cell;
        let fx = fixture(8, 4_096, 4, 43);
        let tiles = fx.tile_count();
        assert_eq!(tiles, 256);
        let mut costs = Vec::new();
        for arity in [2u8, 4, 16, 64] {
            let mut phase = open_phase(&fx, arity, fx.root.clone()).expect("an honest root");
            let (m, s) = phase.root_scale();
            let counted = Cell::new(0u64);
            while phase.turn() != PalwBisectTurnV1::Terminal {
                let ranges = phase.child_ranges();
                let children: Vec<_> = ranges.iter().map(|&(f, c)| fx.range_claim(f, c, m, s)).collect();
                phase
                    .apply_round(&PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children }, 300, 30)
                    .expect("an honest round folds");
                // The challenger's move: recompute every child. That is one pass over the range.
                for &(_, c) in &ranges {
                    counted.set(counted.get() + c);
                }
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
            let passes = counted.get();
            assert!(
                passes < 2 * tiles,
                "arity {arity}: naming a child cost {passes} tile recomputes against a history of {tiles} tiles — over two passes"
            );
            costs.push((arity, passes));
        }
        // A wider round is CHEAPER for the challenger, not dearer: the rounds fall and the
        // per-round work does not rise.
        assert_eq!(costs, vec![(2u8, 510u64), (4, 340), (16, 272), (64, 260)]);
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

        // The BOTTOM, at both head widths, with the real openings and the real paths — PINNED,
        // because "it fits" is a claim a later change can quietly stop honouring and a number is
        // one it cannot.
        let mut bottoms = Vec::new();
        for d_head in [128usize, 256] {
            let fx = fixture(d_head, 64, d_head.min(PALW_ATTN_DISSECT_MAX_LANES), 37);
            let bottom = fx.bottom(h64(9), 1);
            let measured = borsh::to_vec(&bottom).expect("a bottom serializes").len() as u64;
            assert!(measured <= carrier, "d_head {d_head}: a bottom of {measured} bytes is over one carrier of {carrier}");
            // The ADR's own arithmetic: one K tile, one V tile, the query row and the output tile,
            // plus the paths. The tile rows dominate and they are flat in the context.
            let rows = 2 * TILE as u64 * d_head as u64 * 4;
            assert!(measured > rows, "d_head {d_head}: the measured bottom must at least carry its tiles");
            bottoms.push((d_head, measured));
        }
        // Measured, not predicted: 34 opened rows (one query, sixteen K, sixteen V, one output
        // tile) of `4 × lanes` payload and an eight-sibling path each, plus the object's frame.
        // The 256-lane row costs exactly 512 bytes more, thirty-four times over.
        assert_eq!(
            bottoms,
            vec![(128usize, 37_985u64), (256, 55_393)],
            "the bottom's measured wire size at the two registered head widths"
        );
        assert_eq!(55_393 - 37_985, 34 * 128 * 4, "the whole difference between the two tiers is the lanes");
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
    // The bindings — every quantity the chain can derive is compared with the one on the wire
    // ---------------------------------------------------------------------------------------

    /// **The history the dissection is played over is the CHAIN's** (audit A C-1 / E C-2).
    ///
    /// Both directions of the same hole, because they are two different winning strategies and
    /// neither needs any arithmetic: a TRUNCATED history is a truthful answer about a shorter row
    /// (every fold folds, the bottom recomputes, the honest accuser is slashed), and an INFLATED
    /// one buys tiles no committed leaf and no checkpoint chunk can serve, so the accuser can
    /// never close and the backstop ends the session on its side.
    #[test]
    fn a_root_claim_about_another_history_is_refused_by_name() {
        let fx = fixture(8, 50, 4, 71);
        let mut root = PalwAttnRootClaimV1 {
            version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            head: 0,
            lane_first: 0,
            lane_count: 4,
            history_positions: 50,
            claim: fx.root.clone(),
        };
        let site = (0u16, 0u16, 4u16);
        let open = |root: &PalwAttnRootClaimV1| {
            PalwAttnDissectPhaseV1::open(h64(9), root, site, 50, &fx.out_tile, params().values, &court_at(4), TILE, 100, 30, true)
        };
        assert!(open(&root).is_ok(), "the derived history is the honest one");
        root.history_positions = 8;
        assert_eq!(open(&root), Err(PalwAttnCourtError::WrongHistory { got: 8, expected: 50 }));
        root.history_positions = 8_192;
        assert_eq!(open(&root), Err(PalwAttnCourtError::WrongHistory { got: 8_192, expected: 50 }));
    }

    /// **`S*` is a divisor, and its band is derivable** (audit A C-2 / E C-1).
    ///
    /// The band is `int_exp(0) ..= positions × int_exp(0)`: the row max contributes exactly the
    /// unit and no position contributes more. Below it `int_recip(S*)` grows until the bottom's
    /// `e × recip` leaves `i64` — an overflow panic inside block validation on every node, bought
    /// for one court move — and a non-positive one made the close undeliverable, which acquitted
    /// the forger and slashed the accuser.
    #[test]
    fn an_exponent_sum_outside_the_derivable_band_is_refused_by_name() {
        let fx = fixture(8, 50, 4, 73);
        let site = (0u16, 0u16, 4u16);
        let unit = i64::from(crate::palw_base0::int_exp(0));
        let open = |sum: i64| {
            let mut root = PalwAttnRootClaimV1 {
                version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
                head: 0,
                lane_first: 0,
                lane_count: 4,
                history_positions: 50,
                claim: fx.root.clone(),
            };
            root.claim.exp_sum = sum;
            PalwAttnDissectPhaseV1::open(h64(9), &root, site, 50, &fx.out_tile, params().values, &court_at(4), TILE, 100, 30, true)
        };
        assert!(fx.root.exp_sum >= unit && fx.root.exp_sum <= unit * 50, "the honest sum is inside its own band");
        for bad in [0i64, 1, 512, unit - 1, unit * 50 + 1, i64::MAX] {
            assert_eq!(
                open(bad),
                Err(PalwAttnCourtError::ExponentSumOutOfBand { got: bad, min: unit, max: unit * 50, positions: 50 }),
                "S* {bad} is outside the band and must be refused before a round is played"
            );
        }
        assert!(open(unit).is_ok() && open(unit * 50).is_ok(), "both ends of the band are admissible");
        // The refusal has to NAME the field, not merely refuse: a court finding that reads
        // "assertion failed" is a finding nobody can sequence.
        let message = open(1).expect_err("a small S* is refused").to_string();
        assert!(message.contains("exponent sum") && message.contains('1'), "the refusal must name the field: {message}");
    }

    /// **Every opening the bottom carries is bound to its own coordinate** (audit A C-3 / E C-3,
    /// and A L-1 for the output tile).
    ///
    /// Each substitution below opens a leaf the honest claim REALLY committed, at a coordinate the
    /// claim really has — no forgery at all — and each one used to be accepted: the query was
    /// checked for width alone, the series for `position` alone (so another call's row at the same
    /// position passed), and the output tile was never read.
    #[test]
    fn the_bottoms_openings_are_bound_to_the_coordinates_the_class_derives() {
        let fx = fixture(8, 50, 4, 79);
        let site = fx.site();
        let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
        walk_to(&fx, &mut phase, 1);
        let honest = fx.bottom(h64(9), 1);
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &fx.binding, &site, true).expect("the honest bottom verifies"),
            PalwAttnCourtVerdictV1::ChallengerDefeated
        );
        let coord = |slot: u32, position: u32| PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position, tile_index: 0 };

        // The query, replaced by another committed row of the same width — the K row at position
        // 0, which is `kv_dim == d_head` lanes wide and is genuinely in the tree.
        let mut swapped = honest.clone();
        swapped.query = fx.row_opening(leaf_index_k(0), &fx.k[..fx.kv_dim], 1, 0);
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &swapped, &fx.binding, &site, true),
            Err(PalwAttnCourtError::WrongCoordinate { what: "query", got: coord(1, 0), expected: coord(0, 0) })
        );

        // The K series, supplied as the V series' rows — the wholesale swap the module used to
        // document as admissible. The slot the class registers for the K cache says otherwise.
        let mut swapped = honest.clone();
        swapped.k = swapped.v.clone();
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &swapped, &fx.binding, &site, true),
            Err(PalwAttnCourtError::WrongCoordinate { what: "K", got: coord(2, 16), expected: coord(1, 16) })
        );

        // The output tile, replaced by the query row: `lane_count` is 4 and the query is 8 lanes,
        // so before the coordinate check this field was not even read.
        let mut swapped = honest.clone();
        swapped.out_tile = fx.row_opening(leaf_index_query(), &fx.q, 0, 0);
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &swapped, &fx.binding, &site, true),
            Err(PalwAttnCourtError::WrongCoordinate { what: "output tile", got: coord(0, 0), expected: coord(3, 0) })
        );

        // A class that names no cache-write node has no cache-write route, and says so.
        let unnamed = PalwAttnBottomSiteV1 { k_slot: None, ..fx.site() };
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &fx.binding, &unnamed, true),
            Err(PalwAttnCourtError::CacheSeriesNotDeclared { kind: "K" })
        );
        // Nor does one whose two roles resolve to the same node: a tile has two series, not one
        // twice, and that is a fact of the CLASS rather than of the evidence.
        let doubled = PalwAttnBottomSiteV1 { v_slot: Some(1), ..fx.site() };
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &fx.binding, &doubled, true),
            Err(PalwAttnCourtError::KeyAndValueAreTheSameSeries)
        );
        // And a class whose cache row is several leaves cannot be served by a bottom that carries
        // one opening per position — named, rather than surfacing as a width mismatch.
        let split = PalwAttnBottomSiteV1 { kv_tile_lanes: fx.kv_dim / 2, ..fx.site() };
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &honest, &fx.binding, &split, true),
            Err(PalwAttnCourtError::CacheRowIsNotOneLeaf { kv_dim: fx.kv_dim, tile: fx.kv_dim / 2 })
        );
    }

    /// **A history position past the prefill is committed at its own CALL, not at `position`**
    /// (audit A C-3, the half that refuses honest evidence rather than admitting forged).
    ///
    /// The canonical enumeration gives every decode call one position, numbered `0`. The check
    /// this replaced compared `coord.position` with the history index, so on a job with any decode
    /// at all it demanded a coordinate no conforming executor ever commits — and, in the other
    /// direction, `(call 3, position 0)` and `(call 0, position 0)` differed only in the field it
    /// did not read.
    #[test]
    fn a_decode_positions_row_is_addressed_by_its_call() {
        let prefill = 12u32;
        let slot = 7u32;
        assert_eq!(
            history_row_coord_v1(prefill, slot, 5),
            PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position: 5, tile_index: 0 }
        );
        assert_eq!(
            history_row_coord_v1(prefill, slot, 11),
            PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position: 11, tile_index: 0 }
        );
        assert_eq!(
            history_row_coord_v1(prefill, slot, 12),
            PalwStepCoordinateV1 { call_index: 1, node_slot: slot, position: 0, tile_index: 0 },
            "the first decode call writes history position `prefill`"
        );
        assert_eq!(
            history_row_coord_v1(prefill, slot, 14),
            PalwStepCoordinateV1 { call_index: 3, node_slot: slot, position: 0, tile_index: 0 }
        );
    }

    /// **The anchor is the checkpoint the class derives for this step, and a per-position class
    /// has no residue** (audit A C-4).
    ///
    /// Two independent guards on one hole. The first refuses an anchor that is not
    /// `palw_checkpoint_covered_for_step_v1`'s: with one checkpoint at every position, an anchor
    /// chosen INSIDE the disputed tile makes the tile straddle its edge. The second refuses the
    /// straddle's residue on such a class outright, so neither guard is load-bearing alone.
    #[test]
    fn the_anchor_must_be_the_derived_checkpoint_and_a_per_position_class_has_no_residue() {
        let fx = fixture(8, 50, 4, 83);
        let anchor = fx.anchor(40);
        let binding = fx.binding_anchored(&anchor);
        let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
        walk_to(&fx, &mut phase, 2);
        let bottom = fx.bottom_anchored(h64(9), 2, &anchor);
        // The anchor in evidence covers decode call 0; a site that derives another one refuses it.
        let elsewhere = PalwAttnBottomSiteV1 { anchor_covered_decode_call: Some(3), ..fx.site_anchored(&anchor) };
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &bottom, &binding, &elsewhere, true),
            Err(PalwAttnCourtError::WrongAnchor { got: 0, expected: 3 })
        );
        // A class with no checkpoint over the disputed position has no checkpoint route at all.
        let none = PalwAttnBottomSiteV1 { anchor_covered_decode_call: None, ..fx.site_anchored(&anchor) };
        assert_eq!(check_attn_dissect_bottom_v1(&phase, &bottom, &binding, &none, true), Err(PalwAttnCourtError::AnchorCoversNothing));
        // Tile 2 straddles the 40-position edge, so it carries eight rows after the anchor — which
        // on a per-position class is the cache-write route wearing the checkpoint arm's name.
        let per_position = PalwAttnBottomSiteV1 { every_position_is_checkpointed: true, ..fx.site_anchored(&anchor) };
        assert_eq!(
            check_attn_dissect_bottom_v1(&phase, &bottom, &binding, &per_position, true),
            Err(PalwAttnCourtError::RowsAfterOnAPerPositionClass { got: 8 })
        );
    }

    /// **One bottom reads one route** (audit A H-1).
    ///
    /// The mixed bottom is the shape in which the old series comparison silently switched off: the
    /// checkpoint arm returned `None` for its slot, so the `if let (Some, Some)` guard against
    /// "one series read twice" never ran. There is no honest reason to file one — both kinds live
    /// in the same anchor and are written by the same step — so it is refused by name.
    #[test]
    fn a_bottom_whose_kinds_take_different_routes_is_refused() {
        let fx = fixture(8, 50, 4, 89);
        let anchor = fx.anchor(48);
        let binding = fx.binding_anchored(&anchor);
        let site = fx.site_anchored(&anchor);
        let mut phase = open_phase(&fx, 4, fx.root.clone()).expect("an honest root");
        walk_to(&fx, &mut phase, 1);
        let mut mixed = fx.bottom_anchored(h64(9), 1, &anchor);
        mixed.v = fx.bottom(h64(9), 1).v;
        assert_eq!(check_attn_dissect_bottom_v1(&phase, &mixed, &binding, &site, true), Err(PalwAttnCourtError::MixedEvidenceRoutes));
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
        // The same court with a 131,072-position history: 22 BINARY ladder rounds (the ladder a
        // session plays is `PalwBisectLadderV1`, whatever the dissection's arity — audit D H-2)
        // and 13 history rounds at arity 2, plus the terminal moves and the ROOT CLAIM (audit A
        // M-2). The RC window still holds it at this fixture's 20-DAA clock; a narrower one does
        // not, with the arithmetic in the refusal.
        let wide = palw_attn_court_admits_row_v1(&court, 131_072, TILE, 3_000).expect("73 moves at 20 DAA is 1,460");
        assert_eq!(wide, (2 * (22 + 13) + 2 + 1) * 20);
        assert_eq!(
            palw_attn_court_admits_row_v1(&court, 131_072, TILE, 1_500),
            Err(PalwAttnCourtError::OverrunsWindow { moves: 73, deadline: 20, reserve: 216, window_court: 1_500 }),
            "the reserve is the ruleset's 27 carriers, and it is what puts 1,460 over 1,500"
        );
        // Sixteen children a round buys the same row 9 fewer history rounds — 55 moves instead of
        // 73 — which is the whole content of Decision 3 once the leaf ladder is priced honestly.
        let sixteen = court_at(16);
        let worst = palw_attn_court_admits_row_v1(&sixteen, 131_072, TILE, 3_000).expect("16-ary fits");
        assert_eq!(worst, (2 * (22 + 4) + 2 + 1) * 20, "22 binary ladder rounds at 2^22 and 4 history rounds at 8,192 tiles");
        assert_eq!(palw_attn_court_admits_row_v1(&sixteen, 131_072, TILE, 1_500), Ok(1_100), "and it fits the narrow window too");
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
        // The table, from the contract's own recurrence — with the LEAF ladder at the arity a
        // session actually plays (2) and the root claim counted. ADR-0082 §3's own table reads
        // `log_k` on both search spaces; the tree has one k-ary search, and pricing the other one
        // k-ary reported 34 moves for a dispute that takes 60 (audit D H-2, audit A M-2).
        let moves = |k: u8| -> u64 {
            2 * (u64::from(palw_kary_rounds_v1(LADDER, crate::palw_mode_v2::PALW_COURT_LEAF_LADDER_ARITY_V1).unwrap())
                + u64::from(crate::palw_attn_dissect::palw_attn_dissection_rounds_v1(HISTORY, TILE, k).unwrap()))
                + 2
                + 1
        };
        assert_eq!((moves(2), moves(4), moves(8), moves(16), moves(32), moves(64)), (93, 81, 77, 75, 73, 73));
        assert_eq!(moves(16) * 45, 3_375, "75 moves at the 45-DAA deadline — past the 3,000-DAA window, where the ADR read 1,170");
        assert_eq!(moves(2) * 45, 4_185, "and the widest search at the same deadline is further over it");

        // The derivation, at a 128-lane output tile where the carrier bound binds nothing.
        let pick = |deadline: u64, lanes: usize| palw_court_arity_v1(WINDOW, deadline, LADDER, HISTORY, TILE, 2, lanes, CHUNKS);
        // **The budget the boundaries are cut from is the window MINUS the assembly reserve**
        // (audit D H-2c): the derivation now selects on the same inequality
        // `palw_attn_court_admits_row_v1` admits on, so the clock a given arity can afford is
        // `(window − reserve) / moves`, not `window / moves`. Written as the derivation rather
        // than as 2,784, because the reserve is `palw_close_assembly_daa_v1(CHUNKS)`.
        let budget = WINDOW - crate::palw_context_ladder::palw_close_assembly_daa_v1(CHUNKS);
        assert_eq!(budget, 2_784, "3,000 less the RC's 27-carrier reserve of 216");
        // The boundaries, stated as the one inequality they now are.
        assert_eq!(pick(budget / moves(16), 128), Some(16));
        assert_eq!(pick(budget / moves(8) + 1, 128), Some(16), "one DAA past what 8 can afford, 16 is the smallest that fits");
        assert_eq!(pick(budget / moves(8), 128), Some(8));
        assert_eq!(pick(budget / moves(4), 128), Some(4));
        assert_eq!(pick(budget / moves(2), 128), Some(2), "a short enough clock needs no wider round");
        assert_eq!(pick(WINDOW, 128), None, "no arity fits a deadline the size of the whole window");
        // The carrier bound still refuses from ABOVE, and the deepest history search is where it
        // binds: 64 children of a 256-lane tile is 132,102 bytes a move against an 83,333-byte
        // carrier. It no longer BITES at these deadlines, because the honest move count never
        // needs an arity above 32 — 32 at 256 lanes is 66,054 bytes and fits.
        assert_eq!(pick(budget / moves(32), 256), Some(32), "arity 32 at 256 lanes is 66,054 bytes a move");
        assert!(
            palw_attn_dissect_move_bytes_v1(64, 256) > palw_close_bytes_for_chunks_v1(1),
            "and 64 at 256 lanes is still past one carrier, which is what the pair bound is for"
        );
        // A ruleset with no fused site derives from the ladder alone.
        // A ruleset with no fused site derives from the ladder alone — at the clock that ladder's
        // own window buys, which is what `palw_court_turn_deadline_v1` returns for it.
        let ladder_clock = crate::palw_context_ladder::palw_court_turn_deadline_v1(WINDOW, LADDER, 2, CHUNKS).expect("a clock");
        assert_eq!(ladder_clock, 42);
        assert_eq!(palw_court_arity_v1(WINDOW, ladder_clock, LADDER, 0, TILE, 2, 128, CHUNKS), Some(2), "66 moves at 42 DAA is 2,772, and 2,772 + 216 is inside 3,000");
        assert_eq!(palw_court_arity_v1(WINDOW, 45, LADDER, 0, TILE, 2, 128, CHUNKS), None, "66 moves at 45 DAA is 2,970, and the 216-DAA reserve puts it past the window");
    }

    /// **The RC's own numbers, derived rather than quoted — and what they now select.**
    ///
    /// ADR-0082 Decision 3 works its table at `k = 16` and says the derivation "selects" it for
    /// the RC windows; the previous version of this test found 4 instead and pinned that. Both
    /// numbers came from a formula that priced the LEAF ladder k-ary. It is not: a session plays
    /// `PalwBisectLadderV1`, binary (audit D H-2), and the root claim is a move nobody counted
    /// (audit A M-2). With both corrected the answers move, and they move in the direction that
    /// matters — the derivation now reports what the dispute actually costs.
    ///
    /// Two configurations, because they give opposite answers and only one of them ships:
    ///
    /// * **The RC as it is** — its own `2^26` ladder and the widest row its genesis registers, a
    ///   512-position graph-v5 row (32 history tiles) — admits **arity 2**: `2 × (26 + 5) + 2 + 1
    ///   = 65` moves at the RC's 42-DAA clock is 2,730, and with the 216-DAA assembly reserve
    ///   2,946 against a 3,000-DAA window. It fits, with 54 DAA of room.
    /// * **The `2^32` context-ladder fence with a 131,072-position row** — the configuration
    ///   ADR-0082 §3's table is worked in — admits **NO arity at all**: the cheapest honest move
    ///   count is 73 (arity 32 or 64), and `73 × 42 = 3,066` is already past 3,000 before the
    ///   reserve. That is a genesis decision, not a bug in this function: at that ladder and that
    ///   width the window has to grow, the clock has to shrink, or the leaf ladder has to become
    ///   k-ary for real. The refusal is `None`, which is what Z4 asks for.
    #[test]
    fn the_rcs_derived_deadline_selects_an_arity_for_its_own_row_and_none_for_the_fences() {
        use crate::palw_context_ladder::palw_close_assembly_daa_v1;
        use crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1;
        let window = PALW_RC_WINDOWS_V1.window_court;
        let deadline = PALW_RC_WINDOWS_V1.court_turn_deadline;
        assert_eq!((window, deadline), (3_000, 42));
        let rc_ladder = crate::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT;
        assert_eq!(rc_ladder, 1 << 26);

        // The RC's own row: 512 positions, 32 tiles of 16.
        let arity = palw_court_arity_v1(window, deadline, rc_ladder, 512, TILE, 2, 128, CHUNKS).expect("the RC's own row admits an arity");
        assert_eq!(arity, 2, "26 binary ladder rounds and 5 history rounds is the cheapest exchange that fits");
        let court = PalwCourtParamsV2::new(rc_ladder, deadline, 2).expect("a court").with_dissection_arity(arity).expect("legal");
        assert_eq!(court.worst_case_duration_with_history_daa(512, TILE), Some((2 * (26 + 5) + 2 + 1) * deadline));
        assert_eq!(court.worst_case_duration_with_history_daa(512, TILE), Some(2_730));
        assert_eq!(palw_close_assembly_daa_v1(court.max_close_chunks()), 216, "the RC's 27-carrier reserve");
        assert_eq!(palw_attn_court_admits_row_v1(&court, 512, TILE, window), Ok(2_730), "2,730 + 216 is inside 3,000");

        // **The genesis row, at the lane count it actually disputes.** The dense graph-v5 512 row
        // is a genesis row (stream J), and `palw_court_params_at_v2` derives its arity from these
        // same numbers through the transition's own path.
        assert_eq!(palw_court_arity_v1(window, deadline, rc_ladder, 512, TILE, 2, 8, CHUNKS), Some(2));

        // **The CLOCK is the other half of the same inequality, and it came from a different move
        // count.** `palw_court_turn_deadline_v1(3000, 2^26, 2, 27)` returns 51 — the clock for the
        // LADDER alone, with no history rounds and no root claim in it — while the RC's shipped
        // constant is 42. At 51 the honest dispute does not fit at ANY arity: the cheapest is 32
        // (57 moves, 2,907 DAA), and 2,907 + 216 is past 3,000.
        //
        // Both halves of that are now repaired (audit D H-2b/H-2c): the derivation refuses 51
        // rather than returning an arity admission would reject, and
        // `palw_court_turn_deadline_for_history_v1` is the clock a ruleset with a fused row
        // actually derives — the joint form, which counts the history rounds and the root claim in
        // its divisor. `palw_court_turn_deadline_v1` keeps returning 51 for a ruleset with NO
        // history to dissect, which is what it is asked here.
        assert_eq!(crate::palw_context_ladder::palw_court_turn_deadline_v1(window, rc_ladder, 2, 27), Some(51));
        assert_eq!(
            palw_court_arity_v1(window, 51, rc_ladder, 512, TILE, 2, 8, CHUNKS),
            None,
            "no arity fits a 51-DAA move on this window once the 216-DAA assembly reserve is counted"
        );
        assert_eq!(
            crate::palw_context_ladder::palw_court_turn_deadline_for_history_v1(window, rc_ladder, 2, 27, 512, TILE),
            Some((2, 42)),
            "the joint clock is the RC's own shipped 42, at the binary arity its 512 row plays"
        );
        let at_51 = PalwCourtParamsV2::new(rc_ladder, 51, 2).expect("a court").with_dissection_arity(32).expect("legal");
        assert_eq!(at_51.worst_case_duration_with_history_daa(512, TILE), Some(57 * 51));
        assert_eq!(
            palw_attn_court_admits_row_v1(&at_51, 512, TILE, window),
            Err(PalwAttnCourtError::OverrunsWindow { moves: 57, deadline: 51, reserve: 216, window_court: window })
        );

        // **A 4,096-position row was where the derivation and the GATE were two inequalities.**
        // `palw_court_arity_v1` selected on `moves x deadline <= window_court` while
        // `palw_attn_court_admits_row_v1` admits on `moves x deadline + assembly_reserve <
        // window_court` (Z4's own form). At 4,096 positions arity 2 cleared the first and failed
        // the second by 198 DAA, while arity 4 clears both — so the derivation handed admission a
        // value admission then refused, and the row was rejected although a legal arity for it
        // exists. The derivation now carries the reserve (audit D H-2c) and answers 4.
        assert_eq!(palw_court_arity_v1(window, deadline, rc_ladder, 4_096, TILE, 2, 128, CHUNKS), Some(4));
        let at_two = PalwCourtParamsV2::new(rc_ladder, deadline, 2).expect("a court").with_dissection_arity(2).expect("legal");
        assert_eq!(at_two.worst_case_duration_with_history_daa(4_096, TILE), Some((2 * (26 + 8) + 2 + 1) * deadline));
        assert_eq!(
            palw_attn_court_admits_row_v1(&at_two, 4_096, TILE, window),
            Err(PalwAttnCourtError::OverrunsWindow { moves: 71, deadline, reserve: 216, window_court: window }),
            "2,982 + 216 is past 3,000, which is why the derivation above no longer selects this arity"
        );
        let at_four = PalwCourtParamsV2::new(rc_ladder, deadline, 2).expect("a court").with_dissection_arity(4).expect("legal");
        assert_eq!(palw_attn_court_admits_row_v1(&at_four, 4_096, TILE, window), Ok(2_646), "and four would have fitted");

        // The fence's ladder at the ADR's own width: nothing fits, and the derivation says so.
        const FENCE_LADDER: u64 = 1 << 32;
        assert_eq!(palw_court_arity_v1(window, deadline, FENCE_LADDER, 131_072, TILE, 2, 128, CHUNKS), None);
        assert_eq!(palw_court_arity_v1(window, 45, FENCE_LADDER, 131_072, TILE, 2, 128, CHUNKS), None);
        // The cheapest honest count at that ladder, so the gap is a number rather than a verdict.
        let cheapest = PalwCourtParamsV2::new(FENCE_LADDER, deadline, 2).expect("a court").with_dissection_arity(32).expect("legal");
        assert_eq!(cheapest.worst_case_duration_with_history_daa(131_072, TILE), Some(73 * deadline));
        assert_eq!(73 * deadline, 3_066, "past the 3,000-DAA window before the assembly reserve is counted");
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
