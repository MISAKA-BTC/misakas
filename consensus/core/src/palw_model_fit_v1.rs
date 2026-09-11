//! **ADR-0097 — a model's fit is a lookup.** Every wall a class meets on a ruleset, named with the
//! number it needs and the number the ruleset has, from the SAME predicates admission and the court
//! run — so "can this chain carry model X at context C" is a table a person reads and never a
//! conversation a person has.
//!
//! What this module is not: a second admission gate. `verify_class_admission_v5` is the gate, and
//! it refuses by name at the first wall it meets. A registration wants exactly that. A person
//! deciding whether to convert a 2.8-trillion-parameter model wants the opposite — every wall at
//! once, so that fixing the first one is not mistaken for fixing the row. Each row here is one of
//! the gate's own bounds, computed by the gate's own function, and the report is what the gate
//! would have said had it kept going.
//!
//! Every quantity is a **generated artifact** (ADR-0092 §5): nothing in this file is a number
//! transcribed from a document, and `misaka-palw-base0 --bin palw-model-fit` prints the table.
//!
//! The walls, in the order a class meets them:
//!
//! | wall | the predicate | where the ceiling lives |
//! |---|---|---|
//! | [`PalwFitWallV1::GeometryCeiling`] | `n_ctx × layer_count ≤ PALW_STEP_MAX_ENUMERATION` | `PalwShapeProfileV3::validate_geometry` — a constant inside shape validation, so moving it moves what a `ClassRegistered` may carry: a ruleset move |
//! | [`PalwFitWallV1::Ladder`] | the class's worst-case leaves ≤ `max_step_leaf_count` | `PalwCourtParamsV2`, inside `palw_ruleset_id_v2` (ADR-0092 Decision 4: minted once) |
//! | [`PalwFitWallV1::CloseBytes`] and its three siblings | the widest close ≤ the court's ceilings | `PalwCourtParamsV2`, the same ruleset id |
//! | [`PalwFitWallV1::CourtWindow`] | `moves × deadline + reserve < window_court` at the arity the court plays | the lattice windows, the same ruleset id (ADR-0082 Z4) |
//! | [`PalwFitWallV1::StateChunks`] | the attention cache at `n_ctx` ≤ `PALW_STEP_LEG_MAX_STATE_CHUNKS` chunks | `palw_step_leg`, a code constant the checkpoint leg enforces |
//! | [`PalwFitWallV1::PublicDaPayload`] | the prompt ids on a `PublicDa` commitment ≤ one standard transaction | `PALW_STANDARD_TX_BYTES` (ADR-0077 Decision 16: under `PanelDa` the ids do not ride) |
//!
//! Two things are reported beside the walls and are not walls: what one job may ANSWER
//! ([`PalwModelFitReportV1::answer_tokens_per_job`] — a bound on an answer, which ADR-0096
//! Decision 5 chains, never on a class) and what a SEAT must hold to replay the row
//! ([`PalwSeatFootprintV1`] — a host fact, against which no ruleset states a number).
//!
//! **ADR-0103 Decision 8: every wall prints its order.** Each row carries how its need grows with
//! the context ([`PalwFitOrderV1`]) — classified from the row's own predicate at the doublings of
//! the context, never assigned — and the report prints the HELD terms beside the walls
//! ([`PalwHeldTermV1`]: the executor's retention, the seat's fetch, the seat's replay) with their
//! orders and the budget each answers to. Under [`PalwFitRegimeV1::Held`] the walls are read as the
//! held regime reads them — the per-position budget, the ladder as a depth, the window with no leaf
//! ladder, the state tree's depth, the ids off the commitment — and a class whose chain wall still
//! reads `Linear` there is refused by name (`verify_class_admission_v8`).

use crate::palw_attn_court_v1::{PalwAttnCourtError, palw_attn_court_admits_row_held_v1, palw_attn_court_admits_row_v1};
use crate::palw_class_admission_v2::{
    PalwCourtCostShapeV1, PalwKaryCourtV1, derive_court_cost_rows_v1, derive_court_cost_shaped_v1, palw_profile_has_fused_attention_v1,
};
use crate::palw_context_ladder::{palw_class_ladder_rules_for_court_v1, palw_close_assembly_daa_v1};
use crate::palw_held_context_v1::{
    PALW_HELD_SEAT_INTERVAL_OPENING_CAP_BYTES_V1, PalwHeldSeatRouteV1, palw_held_replay_row_v1, palw_held_seat_fetch_bytes_v1,
    palw_held_seat_interval_positions_v1, palw_held_seat_route_v1,
};
use crate::palw_mode_v2::{
    PALW_STANDARD_TX_BYTES, PalwConsensusParamsV2, PalwCourtParamsV2, palw_close_chunks_for_bytes_v1, palw_court_arity_held_v1,
    palw_court_arity_v1,
};
use crate::palw_prompt_ids_v1::{PALW_PROMPT_IDS_OPENING_HEADER_BYTES, PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1};
use crate::palw_state_chunk_map::{
    PALW_ATTN_HISTORY_TILE_V4, PalwStateChunkMapError, gdn_delta_head_slice_bytes_v1, tiled_kv_state_depth_v4,
    tiled_kv_state_geometry_v3,
};
use crate::palw_step::{
    PALW_STEP_MAX_ENUMERATION, PALW_STEP_MAX_LAYERS, PALW_STEP_MAX_NODES_PER_POSITION, PalwLayerKindV1, PalwShapeProfileV3,
    PalwStepOpKindV1, worst_case_step_leaf_count_capped_v1,
};
use crate::palw_step_leg::{PALW_STEP_LEG_MAX_STATE_CHUNKS, PALW_STEP_LEG_MAX_STATE_DEPTH_V4};
use crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS;

/// One wall a class meets on its way to being admitted. Ordered as a class meets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PalwFitWallV1 {
    /// `n_ctx × layer_count ≤ PALW_STEP_MAX_ENUMERATION` (and `layer_count ≤ PALW_STEP_MAX_LAYERS`).
    /// The first wall, and the only one a profile cannot be BUILT past: every family builder runs
    /// `validate_shape`, so a row over this ceiling has no profile to price.
    GeometryCeiling,
    /// The class's longest job — the whole context as prefill — in leaves, against the ruleset's
    /// `max_step_leaf_count`. Inside the ruleset id; ADR-0092 Decision 4.
    Ladder,
    /// The widest close any node of the graph can be prosecuted at, in bytes, against
    /// `max_close_bytes`.
    CloseBytes,
    /// The same close as a count of carriers, against `max_close_chunks`.
    CloseChunks,
    /// The multiply-accumulates a full node redoes at the terminal, against `max_terminal_macs`.
    TerminalMacs,
    /// Rows one disputed step reads, against `max_operand_count`.
    OperandCount,
    /// `moves × turn_deadline + assembly_reserve < window_court` at the arity the court plays,
    /// with the history dissection a fused row adds (ADR-0082 Z4).
    CourtWindow,
    /// The attention cache at `n_ctx`, tiled at `PALW_ATTN_HISTORY_TILE_V4`, as a chunk count
    /// against `PALW_STEP_LEG_MAX_STATE_CHUNKS`.
    StateChunks,
    /// The prompt ids a `PublicDa` job carries on its commitment, against one standard
    /// transaction's bytes.
    PublicDaPayload,
}

impl PalwFitWallV1 {
    pub const ALL: [Self; 9] = [
        Self::GeometryCeiling,
        Self::Ladder,
        Self::CloseBytes,
        Self::CloseChunks,
        Self::TerminalMacs,
        Self::OperandCount,
        Self::CourtWindow,
        Self::StateChunks,
        Self::PublicDaPayload,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::GeometryCeiling => "geometry ceiling",
            Self::Ladder => "ladder",
            Self::CloseBytes => "close bytes",
            Self::CloseChunks => "close chunks",
            Self::TerminalMacs => "terminal macs",
            Self::OperandCount => "operand count",
            Self::CourtWindow => "court window",
            Self::StateChunks => "state chunks",
            Self::PublicDaPayload => "public-da payload",
        }
    }

    /// Where the ceiling this wall compares against is written, and therefore what it costs to
    /// move: a ruleset move (inside `palw_ruleset_id_v2` — a re-mint or a flag day), a code
    /// constant a shipped build enforces (a build, and a ruleset move where the constant gates a
    /// consensus object), or a transport bound.
    pub fn ceiling_lives_in(self) -> &'static str {
        match self {
            Self::GeometryCeiling => "PalwShapeProfileV3::validate_geometry (PALW_STEP_MAX_ENUMERATION; gates ClassRegistered)",
            Self::Ladder => "PalwCourtParamsV2::max_step_leaf_count (inside palw_ruleset_id_v2)",
            Self::CloseBytes | Self::CloseChunks | Self::TerminalMacs | Self::OperandCount => {
                "PalwCourtParamsV2 cost ceilings (inside palw_ruleset_id_v2)"
            }
            Self::CourtWindow => "PalwStateParamsV2::window_court and the court's turn deadline (inside palw_ruleset_id_v2)",
            Self::StateChunks => "palw_step_leg::PALW_STEP_LEG_MAX_STATE_CHUNKS (the checkpoint leg's cap)",
            Self::PublicDaPayload => "palw_mode_v2::PALW_STANDARD_TX_BYTES (the mirrored standard-transaction mass)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFitVerdictV1 {
    /// `need ≤ have` (strict where the gate is strict).
    Admitted,
    /// The wall refuses this row, by this name.
    Refused,
    /// The predicate could not price the row at all — which admission treats as a refusal, and
    /// which this report keeps apart because "too wide to price" and "priced and too wide" send a
    /// reader to different places.
    Unpriced,
}

/// One row of the report: a wall, what the class needs, what the ruleset has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFitRowV1 {
    pub wall: PalwFitWallV1,
    pub need: u64,
    pub have: u64,
    pub unit: &'static str,
    pub verdict: PalwFitVerdictV1,
    /// **How the need grows with the context** (ADR-0103 Decision 8) — classified by
    /// [`palw_fit_order_v1`] from this row's own predicate at the doublings of the context, and
    /// set on every row a report returns.
    pub order: PalwFitOrderV1,
    /// Which term bound, or the refusal the predicate returned. Human-readable and never parsed.
    pub note: String,
}

impl PalwFitRowV1 {
    fn compare(wall: PalwFitWallV1, need: u64, have: u64, unit: &'static str, note: String) -> Self {
        let verdict = if need <= have { PalwFitVerdictV1::Admitted } else { PalwFitVerdictV1::Refused };
        // The order is the sweep's to set; until it runs it is not known, which is what
        // `Unpriced` says. No row leaves `palw_model_fit_v2` without the sweep's answer.
        Self { wall, need, have, unit, verdict, order: PalwFitOrderV1::Unpriced, note }
    }

    fn unpriced(wall: PalwFitWallV1, have: u64, unit: &'static str, note: String) -> Self {
        Self { wall, need: u64::MAX, have, unit, verdict: PalwFitVerdictV1::Unpriced, order: PalwFitOrderV1::Unpriced, note }
    }
}

/// **What a seat must HOLD to replay one job of this row** — the cache the attention layers
/// write at `n_ctx`, the recurrence state the linear layers keep, and the ids. Bytes, from the
/// profile's own geometry and the state map's own row widths; no verdict, because no ruleset
/// states what a seat's host has. The artifact is not here: its size is a property of the FILE,
/// measured by the converter, and a profile does not know it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSeatFootprintV1 {
    pub attention_layers: u32,
    pub recurrent_layers: u32,
    /// One position of one layer's K (or V), in the i32 cache the integer family holds.
    pub kv_row_bytes: u64,
    /// `attention_layers × 2 × kv_row_bytes × n_ctx`.
    pub kv_cache_bytes: u64,
    /// `recurrent_layers × gdn_heads × (k_dim × v_dim × 4)` — constant in `n_ctx`, which is why a
    /// hybrid is the structurally better long-context candidate (ADR-0081 §1.1).
    pub recurrent_state_bytes: u64,
    /// `n_ctx × 4`.
    pub prompt_ids_bytes: u64,
}

/// The report: every wall, plus the two things that are reported and are not walls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwModelFitReportV1 {
    pub n_ctx: u32,
    pub layer_count: u16,
    /// Whether the row has a fused attention site (graph v5) — the rows whose court is the
    /// history dissection and whose window wall has a history term.
    pub fused: bool,
    /// The rules the walls were read under (ADR-0103).
    pub regime: PalwFitRegimeV1,
    /// The arity the court plays against this row: the caller's `PalwKaryCourtV1` where the k-ary
    /// court is armed, else the bundle's stored value.
    pub arity_played: u8,
    /// **What `palw_court_arity_v1` would derive for THIS row's history** — the arity a ruleset
    /// that registered this row would play (ADR-0092 Decision 3), or `None` when no legal arity
    /// fits the window and the carrier. Reported so a reader can see whether the window wall is a
    /// wall at every arity or only at the played one.
    pub arity_derived_for_this_row: Option<u8>,
    /// The prompt-id form the close was priced under, and its per-close cost in bytes: `n_ctx × 4`
    /// flat, one tile plus a path under ADR-0081 Decision 3's root.
    pub prompt_ids_form: PalwPromptIdsFormV1,
    pub prompt_ids_term_on_close_bytes: u64,
    /// The most one JOB of the free-prompt lane can answer: `min(max_decode_tokens, the trace
    /// event cap)`. A bound on an answer, not on a class — a longer answer is a chain of jobs
    /// (ADR-0096 Decision 5) — and reported here because it is the number a person asking for a
    /// "2M context" is usually also asking about.
    pub answer_tokens_per_job: u32,
    pub rows: Vec<PalwFitRowV1>,
    /// **What grows with the context and is held, not carried** (ADR-0103 Decision 8): the
    /// executor's retention, the seat's fetch and the seat's replay, each with its order and the
    /// budget it answers to. Never a wall — no ruleset states a number for any of them.
    pub held_terms: Vec<PalwHeldTermRowV1>,
    pub seat: PalwSeatFootprintV1,
}

impl PalwModelFitReportV1 {
    /// Every wall admits the row. The gate would admit it too, on these bounds — the gate also
    /// checks certification, the registrant's bond and the catalog roots, which are facts about a
    /// registration and not about a model, and which this report does not pretend to know.
    pub fn admitted(&self) -> bool {
        self.rows.iter().all(|r| r.verdict == PalwFitVerdictV1::Admitted)
    }

    /// The walls that refuse, by name, in the order the class meets them.
    pub fn refusing_walls(&self) -> Vec<PalwFitWallV1> {
        self.rows.iter().filter(|r| r.verdict == PalwFitVerdictV1::Refused).map(|r| r.wall).collect()
    }

    /// The walls that could not price the row.
    pub fn unpriced_walls(&self) -> Vec<PalwFitWallV1> {
        self.rows.iter().filter(|r| r.verdict == PalwFitVerdictV1::Unpriced).map(|r| r.wall).collect()
    }

    pub fn row(&self, wall: PalwFitWallV1) -> Option<&PalwFitRowV1> {
        self.rows.iter().find(|r| r.wall == wall)
    }

    /// **The chain walls whose need is linear in the context** (ADR-0103 Decision 8), in the order
    /// a class meets them — what `verify_class_admission_v8` refuses a held class on, by the first
    /// one's name, before any number is compared.
    pub fn linear_chain_walls(&self) -> Vec<PalwFitWallV1> {
        self.rows.iter().filter(|r| r.order == PalwFitOrderV1::Linear).map(|r| r.wall).collect()
    }

    /// The chain walls whose order the sweep could not read.
    pub fn unordered_chain_walls(&self) -> Vec<PalwFitWallV1> {
        self.rows.iter().filter(|r| r.order == PalwFitOrderV1::Unpriced).map(|r| r.wall).collect()
    }

    pub fn held_term(&self, term: PalwHeldTermV1) -> Option<&PalwHeldTermRowV1> {
        self.held_terms.iter().find(|t| t.term == term)
    }
}

// =================================================================================================
// The geometry ceiling — pure arithmetic, no profile needed
// =================================================================================================

/// **The first wall, from the two numbers alone.** `validate_geometry` refuses a shape whose
/// `n_ctx × layer_count` exceeds `PALW_STEP_MAX_ENUMERATION` (and a `layer_count` over
/// `PALW_STEP_MAX_LAYERS`); this is that predicate, callable before any profile exists, so the
/// question "does a 2M context fit" has an answer that does not first require a graph.
pub fn palw_geometry_ceiling_fit_v1(n_ctx: u32, layer_count: u16) -> PalwFitRowV1 {
    let need = (n_ctx as u64).saturating_mul(layer_count as u64);
    let mut row = PalwFitRowV1::compare(
        PalwFitWallV1::GeometryCeiling,
        need,
        PALW_STEP_MAX_ENUMERATION,
        "positions × layers",
        format!("n_ctx {n_ctx} × layer_count {layer_count}"),
    );
    if layer_count > PALW_STEP_MAX_LAYERS {
        row.verdict = PalwFitVerdictV1::Refused;
        row.note = format!("layer_count {layer_count} exceeds PALW_STEP_MAX_LAYERS {PALW_STEP_MAX_LAYERS}");
    }
    if n_ctx == 0 || layer_count == 0 {
        row.verdict = PalwFitVerdictV1::Refused;
        row.note = "a zero context or a zero layer count is not a shape".to_string();
    }
    row
}

/// The widest context the geometry ceiling admits at `layer_count` layers:
/// `⌊PALW_STEP_MAX_ENUMERATION / layer_count⌋`, or 0 for a layer count the ceiling refuses outright.
pub fn palw_widest_context_under_the_geometry_ceiling_v1(layer_count: u16) -> u32 {
    if layer_count == 0 || layer_count > PALW_STEP_MAX_LAYERS {
        return 0;
    }
    (PALW_STEP_MAX_ENUMERATION / layer_count as u64).min(u32::MAX as u64) as u32
}

/// The smallest layer count the geometry ceiling refuses at `n_ctx`: the first `L` with
/// `n_ctx × L > PALW_STEP_MAX_ENUMERATION`. `1` means no model of any depth fits at that context;
/// `None` means every legal depth fits.
pub fn palw_fewest_layers_refused_at_context_v1(n_ctx: u32) -> Option<u16> {
    if n_ctx == 0 {
        return None;
    }
    let fewest = PALW_STEP_MAX_ENUMERATION / n_ctx as u64 + 1;
    (fewest <= PALW_STEP_MAX_LAYERS as u64).then_some(fewest as u16)
}

// =================================================================================================
// ADR-0103 Decision 8 — every wall prints its order
// =================================================================================================

/// **The order of a wall's need in the context** (ADR-0103 Decision 8), read off the need at the
/// row's context and at successive doublings of it by the row's own predicate — never assigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PalwFitOrderV1 {
    /// The need did not move over the sweep's last doubling: a node count, a tile, a width the
    /// window has already saturated.
    Constant,
    /// It grows, and its growth per doubling does not: a Merkle path, a dissection's rounds.
    Logarithmic,
    /// Its growth per doubling grows with the context: a count of positions, of chunks, of ids — or
    /// anything faster, reported here too (a quadratic term is at least linear, and that is what a
    /// gate refusing linear terms needs to know).
    Linear,
    /// The sweep could not price the need at every point it reads — a context past `u32`, or a
    /// predicate that refused to price — so the order is not known. Kept apart from the three
    /// orders for the reason [`PalwFitVerdictV1::Unpriced`] is kept apart from a refusal, and the
    /// gate refuses on it rather than guess.
    Unpriced,
}

impl PalwFitOrderV1 {
    pub fn name(self) -> &'static str {
        match self {
            Self::Constant => "constant",
            Self::Logarithmic => "logarithmic",
            Self::Linear => "linear",
            Self::Unpriced => "unpriced",
        }
    }
}

/// The doublings the order sweep reads above a row's context: the need at `C, 2C, …, 2^6·C`.
///
/// **Seven points, not the three ADR-0103 Decision 8 wrote.** The ADR classified from `C`, `2C`
/// and `4C` "by the second difference". Three points cannot tell a logarithm with a ceiling from a
/// doubling term: a dissection at arity 64 gains one round every six doublings, so its first
/// differences read `0, 1` — exactly the shape of a linear term's `d, 2d` at `d = 0`. Over six
/// doublings a linear need's last growth is 32 times its first (8 times even when it first grows
/// three doublings in), and a sum of fewer than eight logarithms never reaches 8.
pub const PALW_FIT_ORDER_DOUBLINGS: u32 = 6;
/// The fewest doublings the classifier will read before it calls an order: at four, a linear
/// need's ratio is exactly [`PALW_FIT_ORDER_LINEAR_RATIO`]; at fewer, no ratio separates it.
pub const PALW_FIT_ORDER_MIN_DOUBLINGS: u32 = 4;
/// Growth over the last doubling at least this many times the growth over the first doubling that
/// grew is linear growth.
pub const PALW_FIT_ORDER_LINEAR_RATIO: u64 = 8;

/// **The classifier.** `needs` are one wall's need at the row's context and at each doubling of it,
/// in order. `u64::MAX` is the sweep's unpriced marker (a row whose predicate refused to price).
pub fn palw_fit_order_v1(needs: &[u64]) -> PalwFitOrderV1 {
    if needs.len() < PALW_FIT_ORDER_MIN_DOUBLINGS as usize + 1 || needs.contains(&u64::MAX) {
        return PalwFitOrderV1::Unpriced;
    }
    let growth: Vec<u64> = needs.windows(2).map(|w| w[1].saturating_sub(w[0])).collect();
    let last = growth[growth.len() - 1];
    if last == 0 {
        return PalwFitOrderV1::Constant;
    }
    let first = growth.iter().copied().find(|&g| g > 0).unwrap_or(last);
    if last >= first.saturating_mul(PALW_FIT_ORDER_LINEAR_RATIO) { PalwFitOrderV1::Linear } else { PalwFitOrderV1::Logarithmic }
}

/// **The same graph at another context** — the experiment the order column runs: every field the
/// class registered, with `n_ctx` moved (and the batch sizes, where the family tied them to it).
/// Not a class: its id is not the registered one and nothing may register it. The only question
/// asked of it is how each wall's need grows when nothing but the context does.
pub fn palw_profile_at_context_v1(profile: &PalwShapeProfileV3, n_ctx: u32) -> PalwShapeProfileV3 {
    let mut scaled = profile.clone();
    if scaled.n_batch == profile.n_ctx {
        scaled.n_batch = n_ctx;
    }
    if scaled.n_ubatch == profile.n_ctx {
        scaled.n_ubatch = n_ctx;
    }
    scaled.n_ctx = n_ctx;
    scaled
}

/// **Which rules the walls are read under** (ADR-0103).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFitRegimeV1 {
    /// Every shipped ruleset: ADR-0097's walls as they stand.
    Shipped,
    /// `Params::palw_held_context` armed and the class registers NO held map: its walls are the
    /// shipped ones except the two the regime moves for every class on the network — the window
    /// runs with no leaf ladder (no dispute bisects once the fence is armed; the acceptance path
    /// refuses `CourtOpened` for every claim) and the generated-token pin is priced at the trace
    /// cap. What the gate reads such a class under; it is never refused for an order.
    HeldNetwork,
    /// `Params::palw_held_context` armed and the class registers a held (v4) map: the geometry
    /// ceiling is the per-position budget (Decision 6), the ladder is a depth (Decision 1), the
    /// window runs with no leaf ladder in it (Decision 5), the state wall is the proof's depth
    /// (Decision 3), and the ids ride only where the network has not armed `PanelDa`
    /// (Decision 4) — `panel_da` is `Params::palw_panel_da_at` at the same point.
    Held { panel_da: bool },
}

/// **The regime a class's walls are read under, from the gate's own reading of the fences**:
/// held exactly when the network armed the regime at the point of judgement AND the class
/// registered a held map — `verify_class_admission_v8`'s rule, spelled once for every reader
/// that is not the gate (the generator, the panel's preflight, a test).
pub fn palw_fit_regime_for_v1(
    held: crate::palw_class_admission_v2::PalwHeldAdmissionV1,
    profile: &PalwShapeProfileV3,
) -> PalwFitRegimeV1 {
    match (held.armed, crate::palw_state_chunk_map::palw_profile_is_held_v4(profile)) {
        (true, true) => PalwFitRegimeV1::Held { panel_da: held.panel_da },
        (true, false) => PalwFitRegimeV1::HeldNetwork,
        (false, _) => PalwFitRegimeV1::Shipped,
    }
}

/// **What grows with the context and is HELD rather than carried** (ADR-0103 Decision 8): printed
/// beside the chain's walls with its order and the budget it is checked against, so "constant on
/// the chain, linear where it is held" is a table and not a sentence. No ruleset number bounds any
/// of these; each is a host fact the plan or the drill prices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PalwHeldTermV1 {
    /// The cache, the recurrence state and the ids the executor retains for the claim's life.
    ExecutorRetention,
    /// What a seat fetches to start its interval — the state at the interval's start, for one
    /// seat holding every layer (a shard plan divides it; ADR-0103 Decision 7's column). Zero on
    /// the recompute route.
    SeatFetch,
    /// The positions a seat replays: the whole prefill where interval 0 is the prefill (the
    /// shipped unit), `P` where the unit is an interval of positions (ADR-0103 Decision 2).
    SeatReplay,
}

impl PalwHeldTermV1 {
    pub const ALL: [Self; 3] = [Self::ExecutorRetention, Self::SeatFetch, Self::SeatReplay];

    pub fn name(self) -> &'static str {
        match self {
            Self::ExecutorRetention => "executor retention",
            Self::SeatFetch => "seat fetch",
            Self::SeatReplay => "seat replay",
        }
    }

    /// The budget the term is checked against — never a ruleset number.
    pub fn checked_against(self) -> &'static str {
        match self {
            Self::ExecutorRetention => "the executor's disk for the claim's life (claim_retirement); a host fact",
            Self::SeatFetch => "window_receipt × the seat's bandwidth (ADR-0103 Decision 7: the shard plan's fetch column)",
            Self::SeatReplay => {
                "window_receipt at the family's replay rate over the drill's margin (ADR-0103 Decision 2; the certification drill)"
            }
        }
    }
}

/// One held term: what it is, how much, its order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwHeldTermRowV1 {
    pub term: PalwHeldTermV1,
    pub need: u64,
    pub unit: &'static str,
    pub order: PalwFitOrderV1,
    pub note: String,
}

// =================================================================================================
// The report
// =================================================================================================

/// The output lanes the widest fused site of this row disputes at — what
/// `palw_attn_widest_registered_site_v2` reads off a genesis set, read off ONE profile.
fn fused_site_lanes_v1(profile: &PalwShapeProfileV3) -> usize {
    let mut lanes = 0usize;
    for slot in 0..profile.global_node_count() {
        let Some((node, _)) = profile.resolve_node_slot(slot) else { continue };
        if node.op_kind == PalwStepOpKindV1::AttnFused {
            lanes = lanes.max((node.tile_len as usize).min(profile.attn_head_dim as usize));
        }
    }
    lanes
}

/// The per-close cost of the prompt ids under `form` at `n_ctx` ids: the whole sequence flat, or
/// one tile plus its path under the tiled root (ADR-0081 §1.2 and Decision 3).
pub fn palw_prompt_ids_term_on_close_v1(form: PalwPromptIdsFormV1, n_ctx: u32) -> u64 {
    match form {
        PalwPromptIdsFormV1::Flat => (n_ctx as u64).saturating_mul(4),
        PalwPromptIdsFormV1::MerkleV1 => {
            let tiles = (n_ctx as u64).div_ceil(PALW_PROMPT_IDS_TILE_LEN as u64).max(1);
            let path = tiles.next_power_of_two().trailing_zeros() as u64;
            PALW_PROMPT_IDS_OPENING_HEADER_BYTES + PALW_PROMPT_IDS_TILE_LEN as u64 * 4 + path * 64
        }
    }
}

/// `⌈log₂ n⌉`, with 1 for `n ≤ 1` so a one-leaf tree still reads one level — the step path's own
/// convention (`step_path_bytes_v1`).
fn levels_v1(n: u64) -> u64 {
    if n <= 1 { 1 } else { u64::from(64 - (n - 1).leading_zeros()) }
}

/// Everything the report computes at ONE context, before the sweep classifies it.
struct PalwFitAtV1 {
    rows: Vec<PalwFitRowV1>,
    held_terms: Vec<PalwHeldTermRowV1>,
    arity_played: u8,
    arity_derived_for_this_row: Option<u8>,
    seat: PalwSeatFootprintV1,
}

/// **Every wall, for one row, on one ruleset** — under the shipped rules
/// ([`palw_model_fit_v2`] with [`PalwFitRegimeV1::Shipped`]).
///
/// `court` is the caller's reading of the fence, exactly as `verify_class_admission_v5` takes it
/// (`palw_admission_shape_at_v1` is the one spelling): `Some` where `palw_kary_court` is armed at
/// the point of judgement, carrying the arity the ruleset derives; `None` where it is dormant.
/// `prompt_ids_form` is `Params::palw_prompt_ids_form_at` at the same point. Neither is inferred
/// here, for ADR-0082 Decision 5's reason: they move the price in opposite directions.
pub fn palw_model_fit_v1(
    profile: &PalwShapeProfileV3,
    bundle: &PalwConsensusParamsV2,
    court: Option<PalwKaryCourtV1>,
    prompt_ids_form: PalwPromptIdsFormV1,
) -> PalwModelFitReportV1 {
    palw_model_fit_v2(profile, bundle, court, prompt_ids_form, PalwFitRegimeV1::Shipped)
}

/// **[`palw_model_fit_v1`] under a stated regime, with every wall's order and the held terms**
/// (ADR-0103 Decision 8).
///
/// The rows at the class's own context are the report ADR-0097 printed; the order of each is
/// classified by [`palw_fit_order_v1`] from the same row recomputed on
/// [`palw_profile_at_context_v1`] at `2C … 2^6·C` (as many doublings as `u32` holds). The close's
/// chunk count takes the order of the close's bytes: it is the same quantity on a 120,000-byte
/// grid, and a grid that coarse hides a linear term for several doublings.
pub fn palw_model_fit_v2(
    profile: &PalwShapeProfileV3,
    bundle: &PalwConsensusParamsV2,
    court: Option<PalwKaryCourtV1>,
    prompt_ids_form: PalwPromptIdsFormV1,
    regime: PalwFitRegimeV1,
) -> PalwModelFitReportV1 {
    let n_ctx = profile.n_ctx;
    let at = palw_fit_at_v1(profile, bundle, court, prompt_ids_form, regime, None);
    let marker = |r: &PalwFitRowV1| if r.verdict == PalwFitVerdictV1::Unpriced { u64::MAX } else { r.need };
    let mut wall_needs: Vec<Vec<u64>> = at.rows.iter().map(|r| vec![marker(r)]).collect();
    let mut term_needs: Vec<Vec<u64>> = at.held_terms.iter().map(|t| vec![t.need]).collect();
    for doubling in 1..=PALW_FIT_ORDER_DOUBLINGS {
        let wider = u64::from(n_ctx) << doubling;
        if wider > u64::from(u32::MAX) {
            break;
        }
        // **Each doubling is priced under a ladder that holds it** — the ruleset's, or the power of
        // two above the wider row's leaves where the ruleset's is shallower. Under the ruleset's
        // own ladder a wider row is refused at the ladder and every later wall reads "unpriced",
        // which says nothing about how THAT wall grows; the ladder's own growth is the Ladder row's.
        let scaled = palw_profile_at_context_v1(profile, wider as u32);
        let holding = worst_case_step_leaf_count_capped_v1(&scaled, u64::MAX)
            .ok()
            .map(|worst| worst.checked_next_power_of_two().unwrap_or(u64::MAX))
            .filter(|&ladder| ladder > bundle.court.max_step_leaf_count());
        let next = palw_fit_at_v1(&scaled, bundle, court, prompt_ids_form, regime, holding);
        for (needs, row) in wall_needs.iter_mut().zip(&next.rows) {
            needs.push(marker(row));
        }
        for (needs, term) in term_needs.iter_mut().zip(&next.held_terms) {
            needs.push(term.need);
        }
    }
    let mut rows = at.rows;
    for (row, needs) in rows.iter_mut().zip(&wall_needs) {
        row.order = palw_fit_order_v1(needs);
    }
    if let Some(bytes_order) = rows.iter().find(|r| r.wall == PalwFitWallV1::CloseBytes).map(|r| r.order) {
        for row in rows.iter_mut().filter(|r| r.wall == PalwFitWallV1::CloseChunks) {
            row.order = bytes_order;
        }
    }
    let mut held_terms = at.held_terms;
    for (term, needs) in held_terms.iter_mut().zip(&term_needs) {
        term.order = palw_fit_order_v1(needs);
    }
    PalwModelFitReportV1 {
        n_ctx,
        layer_count: profile.layer_count,
        fused: palw_profile_has_fused_attention_v1(profile),
        regime,
        arity_played: at.arity_played,
        arity_derived_for_this_row: at.arity_derived_for_this_row,
        prompt_ids_form,
        prompt_ids_term_on_close_bytes: palw_prompt_ids_term_on_close_v1(prompt_ids_form, n_ctx),
        answer_tokens_per_job: bundle.freeprompt.max_decode_tokens().min(PALW_V2_MAX_TRACE_EVENTS as u32),
        rows,
        held_terms,
        seat: at.seat,
    }
}

fn palw_fit_at_v1(
    profile: &PalwShapeProfileV3,
    bundle: &PalwConsensusParamsV2,
    court: Option<PalwKaryCourtV1>,
    prompt_ids_form: PalwPromptIdsFormV1,
    regime: PalwFitRegimeV1,
    ladder_override: Option<u64>,
) -> PalwFitAtV1 {
    let n_ctx = profile.n_ctx;
    // The ruleset's court — or, for a sweep point, the same court under a ladder deep enough to
    // hold the wider row (every other ceiling the ruleset's own).
    let base_court = match ladder_override {
        Some(deeper) => PalwCourtParamsV2::with_cost_ceilings(
            deeper,
            bundle.court.turn_deadline_daa(),
            bundle.court.terminal_rounds(),
            bundle.court.max_close_bytes(),
            bundle.court.max_terminal_macs(),
            bundle.court.max_operand_count(),
        )
        .and_then(|c| c.with_dissection_arity(bundle.court.dissection_arity()))
        .unwrap_or(bundle.court),
        None => bundle.court,
    };
    let ladder = base_court.max_step_leaf_count();
    let fused = palw_profile_has_fused_attention_v1(profile);
    let window_court = bundle.state.window_court();
    // The CLASS's rules (the held map's walls) and the NETWORK's clock (no bisection, the pin at
    // the trace cap) are two readings: a class with no held map on a held network takes the
    // second and not the first.
    let held = matches!(regime, PalwFitRegimeV1::Held { .. });
    let held_clock = !matches!(regime, PalwFitRegimeV1::Shipped);
    let mut rows = Vec::with_capacity(PalwFitWallV1::ALL.len());

    // 1. The geometry ceiling. A built profile is past it by construction; the row is here so the
    //    table is the whole list and a reader does not learn the first wall from its absence.
    //    Held (ADR-0103 Decision 6): the per-position budget — the nodes a validating node walks
    //    for one position, which no context multiplies.
    if held {
        let nodes = u64::from(profile.global_node_count());
        let mut row = PalwFitRowV1::compare(
            PalwFitWallV1::GeometryCeiling,
            nodes,
            PALW_STEP_MAX_NODES_PER_POSITION,
            "nodes a position",
            format!("{nodes} nodes a position over {} layers; the context multiplies nothing a node walks", profile.layer_count),
        );
        if profile.layer_count > PALW_STEP_MAX_LAYERS {
            row.verdict = PalwFitVerdictV1::Refused;
            row.note = format!("layer_count {} exceeds PALW_STEP_MAX_LAYERS {PALW_STEP_MAX_LAYERS}", profile.layer_count);
        }
        rows.push(row);
    } else {
        rows.push(palw_geometry_ceiling_fit_v1(n_ctx, profile.layer_count));
    }

    // 2. The ladder: the whole context as prefill, uncapped, against the ruleset's number. Held
    //    (Decision 1): the same comparison read as the depth it prices — no round is played, so
    //    what the chain carries for it is a Merkle path.
    match worst_case_step_leaf_count_capped_v1(profile, u64::MAX) {
        Ok(worst) if held => {
            let mut row = PalwFitRowV1::compare(
                PalwFitWallV1::Ladder,
                levels_v1(worst),
                levels_v1(ladder),
                "levels (64 bytes of path each)",
                format!(
                    "the whole context as prefill is {worst} leaves; a path to one is {} levels and no round is played",
                    levels_v1(worst)
                ),
            );
            row.verdict = if worst <= ladder { PalwFitVerdictV1::Admitted } else { PalwFitVerdictV1::Refused };
            rows.push(row);
        }
        Ok(worst) => rows.push(PalwFitRowV1::compare(
            PalwFitWallV1::Ladder,
            worst,
            ladder,
            "leaves",
            format!(
                "the whole context as prefill is {worst} leaves; 2^{} would hold it",
                worst.max(2).checked_next_power_of_two().map_or(64, |p| p.trailing_zeros())
            ),
        )),
        Err(e) => rows.push(PalwFitRowV1::unpriced(PalwFitWallV1::Ladder, ladder, "leaves", format!("{e:?}"))),
    }

    // 3–6. The close, priced for the court the caller named — the SAME shape admission prices
    //      with: the ladder rules under that court for a mapped class, the genesis-anchored form
    //      for a class with no state chunk map.
    let shape = palw_class_ladder_rules_for_court_v1(profile, court, ladder)
        .map(|rules| rules.cost_shape)
        .unwrap_or_else(|| PalwCourtCostShapeV1::genesis_anchored_v1(profile, ladder).with_prompt_ids_form_v1(prompt_ids_form));
    // Held (Decision 4): the generated-token pin at the trace cap, as the gate prices it.
    let shape = if held_clock { shape.with_decode_bound_v1(PALW_V2_MAX_TRACE_EVENTS as u64) } else { shape };
    let court_params: PalwCourtParamsV2 = match court {
        Some(k) => base_court.with_dissection_arity(k.dissection_arity).unwrap_or(base_court),
        None => base_court,
    };
    match derive_court_cost_shaped_v1(profile, shape) {
        Ok(cost) => {
            let binding = derive_court_cost_rows_v1(profile, shape)
                .ok()
                .and_then(|mut r| (!r.is_empty()).then(|| r.remove(0)))
                .map(|r| {
                    format!(
                        "binding node {}[{}] {:?} ({} opening + {} evidence)",
                        r.table, r.index, r.op_kind, r.opening_bytes, r.evidence_bytes
                    )
                })
                .unwrap_or_else(|| "binding node not reported".to_string());
            rows.push(PalwFitRowV1::compare(
                PalwFitWallV1::CloseBytes,
                cost.max_close_bytes,
                court_params.max_close_bytes(),
                "bytes",
                binding.clone(),
            ));
            rows.push(PalwFitRowV1::compare(
                PalwFitWallV1::CloseChunks,
                palw_close_chunks_for_bytes_v1(cost.max_close_bytes),
                court_params.max_close_chunks(),
                "carriers",
                binding,
            ));
            rows.push(PalwFitRowV1::compare(
                PalwFitWallV1::TerminalMacs,
                cost.max_terminal_macs,
                court_params.max_terminal_macs(),
                "multiply-accumulates",
                String::new(),
            ));
            rows.push(PalwFitRowV1::compare(
                PalwFitWallV1::OperandCount,
                cost.max_operand_count as u64,
                court_params.max_operand_count() as u64,
                "rows",
                String::new(),
            ));
        }
        Err(e) => {
            let note = format!("the close does not derive: {e:?}");
            rows.push(PalwFitRowV1::unpriced(PalwFitWallV1::CloseBytes, court_params.max_close_bytes(), "bytes", note.clone()));
            rows.push(PalwFitRowV1::unpriced(PalwFitWallV1::CloseChunks, court_params.max_close_chunks(), "carriers", note.clone()));
            rows.push(PalwFitRowV1::unpriced(
                PalwFitWallV1::TerminalMacs,
                court_params.max_terminal_macs(),
                "multiply-accumulates",
                note.clone(),
            ));
            rows.push(PalwFitRowV1::unpriced(PalwFitWallV1::OperandCount, court_params.max_operand_count() as u64, "rows", note));
        }
    }

    // 7. The window, at the arity the court plays, with the history a fused row adds. Held
    //    (Decision 5): the same inequality with the leaf ladder's rounds at zero — the dissection
    //    opens at the accusation's leaf.
    let history = if fused { u64::from(n_ctx) } else { 0 };
    let reserve = palw_close_assembly_daa_v1(court_params.max_close_chunks());
    let admits = if held_clock {
        palw_attn_court_admits_row_held_v1(&court_params, history, PALW_ATTN_HISTORY_TILE_V4, window_court)
    } else {
        palw_attn_court_admits_row_v1(&court_params, history, PALW_ATTN_HISTORY_TILE_V4, window_court)
    };
    let clock = if held_clock { "no leaf ladder, " } else { "" };
    match admits {
        Ok(worst) => rows.push(PalwFitRowV1::compare(
            PalwFitWallV1::CourtWindow,
            worst.saturating_add(reserve),
            window_court.saturating_sub(1),
            "DAA",
            format!(
                "{} moves × {} DAA + {reserve} reserve at arity {} over {history} history positions ({clock}ADR-0082 Z4)",
                worst / court_params.turn_deadline_daa().max(1),
                court_params.turn_deadline_daa(),
                court_params.dissection_arity()
            ),
        )),
        Err(PalwAttnCourtError::OverrunsWindow { moves, deadline, reserve, window_court }) => rows.push(PalwFitRowV1::compare(
            PalwFitWallV1::CourtWindow,
            moves.saturating_mul(deadline).saturating_add(reserve),
            window_court.saturating_sub(1),
            "DAA",
            format!(
                "{moves} moves × {deadline} DAA + {reserve} reserve at arity {} over {history} history positions ({clock}ADR-0082 Z4)",
                court_params.dissection_arity()
            ),
        )),
        Err(e) => rows.push(PalwFitRowV1::unpriced(PalwFitWallV1::CourtWindow, window_court, "DAA", format!("{e:?}"))),
    }
    let arity_derived_for_this_row = if held_clock {
        palw_court_arity_held_v1(
            window_court,
            court_params.turn_deadline_daa(),
            history,
            PALW_ATTN_HISTORY_TILE_V4,
            court_params.terminal_rounds(),
            fused_site_lanes_v1(profile),
            court_params.max_close_chunks(),
        )
    } else {
        palw_court_arity_v1(
            window_court,
            court_params.turn_deadline_daa(),
            ladder,
            history,
            PALW_ATTN_HISTORY_TILE_V4,
            court_params.terminal_rounds(),
            fused_site_lanes_v1(profile),
            court_params.max_close_chunks(),
        )
    };

    // 8. The attention cache at this context. Shipped: a chunk COUNT on the tiled map, which
    //    the v3 index makes a count. Held (Decision 3): the proof's DEPTH on the v4 tree — the
    //    cap the checkpoint leg enforces there, one spelling with the geometry's.
    let attention_layers = (0..profile.layer_count).filter(|&l| profile.layer_kind(l) == PalwLayerKindV1::Attention).count() as u32;
    let recurrent_layers = profile.layer_count as u32 - attention_layers;
    let max_chunks = PALW_STEP_LEG_MAX_STATE_CHUNKS as u64;
    let max_depth = u64::from(PALW_STEP_LEG_MAX_STATE_DEPTH_V4);
    if held {
        let chunks_per_slice = u64::from(n_ctx.max(1)).div_ceil(u64::from(PALW_ATTN_HISTORY_TILE_V4.min(n_ctx.max(1))));
        let depth = u64::from(tiled_kv_state_depth_v4(chunks_per_slice, u64::from(attention_layers), profile.layer_count));
        rows.push(PalwFitRowV1::compare(
            PalwFitWallV1::StateChunks,
            depth,
            max_depth,
            "levels",
            format!(
                "⌈{n_ctx} / {PALW_ATTN_HISTORY_TILE_V4}⌉ = {chunks_per_slice} blocks a slice under {} slices at most; a block appended moves no index",
                2 * attention_layers + 2 * profile.layer_count as u32
            ),
        ));
    } else if attention_layers == 0 {
        rows.push(PalwFitRowV1::compare(
            PalwFitWallV1::StateChunks,
            0,
            max_chunks,
            "chunks",
            "no attention layers: no cache to chunk".into(),
        ));
    } else {
        match tiled_kv_state_geometry_v3(profile, n_ctx.max(1)) {
            Ok(g) => rows.push(PalwFitRowV1::compare(
                PalwFitWallV1::StateChunks,
                g.chunk_count(),
                max_chunks,
                "chunks",
                format!("{attention_layers} attention layers × 2 slices × ⌈{n_ctx} / {PALW_ATTN_HISTORY_TILE_V4}⌉ tiles"),
            )),
            Err(PalwStateChunkMapError::TooManyChunks { got, max }) => rows.push(PalwFitRowV1::compare(
                PalwFitWallV1::StateChunks,
                got,
                max as u64,
                "chunks",
                format!("{attention_layers} attention layers × 2 slices × ⌈{n_ctx} / {PALW_ATTN_HISTORY_TILE_V4}⌉ tiles"),
            )),
            Err(e) => rows.push(PalwFitRowV1::unpriced(PalwFitWallV1::StateChunks, max_chunks, "chunks", format!("{e:?}"))),
        }
    }

    // 9. The ids on the commitment: `n_ctx × 4` bytes on one standard transaction under
    //    `PublicDa`. Held (Decision 4) on a network that armed `PanelDa`: the widest job commits
    //    with no ids — they are served under the root, never carried.
    match regime {
        PalwFitRegimeV1::Held { panel_da: true } => rows.push(PalwFitRowV1::compare(
            PalwFitWallV1::PublicDaPayload,
            0,
            PALW_STANDARD_TX_BYTES,
            "bytes",
            "the widest job commits under PanelDa (ADR-0077 Decision 16): the ids are served under the root the chain names, never carried (ADR-0103 Decision 4)".into(),
        )),
        _ => rows.push(PalwFitRowV1::compare(
            PalwFitWallV1::PublicDaPayload,
            (n_ctx as u64).saturating_mul(4),
            PALW_STANDARD_TX_BYTES,
            "bytes",
            "the prompt ids ride the commitment under PublicDa; under PanelDa (ADR-0077 Decision 16) they do not".into(),
        )),
    }

    let kv_row_bytes = (profile.attn_kv_heads as u64).saturating_mul(profile.attn_head_dim as u64).saturating_mul(4);
    let seat = PalwSeatFootprintV1 {
        attention_layers,
        recurrent_layers,
        kv_row_bytes,
        kv_cache_bytes: (attention_layers as u64).saturating_mul(2).saturating_mul(kv_row_bytes).saturating_mul(n_ctx as u64),
        recurrent_state_bytes: (recurrent_layers as u64)
            .saturating_mul(profile.gdn_heads as u64)
            .saturating_mul(gdn_delta_head_slice_bytes_v1(profile).unwrap_or(0)),
        prompt_ids_bytes: (n_ctx as u64).saturating_mul(4),
    };

    // The held terms. Nothing here is compared against a ruleset number; each is priced against
    // the budget `PalwHeldTermV1::checked_against` names.
    let replay_ms = palw_held_replay_row_v1(profile).replay_ms_per_position();
    let window_receipt = bundle.state.window_receipt();
    let retention = seat.kv_cache_bytes.saturating_add(seat.recurrent_state_bytes).saturating_add(seat.prompt_ids_bytes);
    let mut held_terms = vec![PalwHeldTermRowV1 {
        term: PalwHeldTermV1::ExecutorRetention,
        need: retention,
        unit: "bytes",
        order: PalwFitOrderV1::Unpriced,
        note: "the attention cache, the recurrence state and the ids, for the claim's life".into(),
    }];
    if held {
        // The route is derived (Decision 2); the fetch is printed at what RESUMING costs at this
        // context whichever route is derived — the recompute route fetches nothing and exists only
        // while the whole context fits the budget, so a sweep that crossed the boundary would read
        // a jump from zero as a logarithm. The note says which route the class takes here.
        let route = palw_held_seat_route_v1(n_ctx, replay_ms, window_receipt);
        let fetch = palw_held_seat_fetch_bytes_v1(profile, u64::from(n_ctx), 0..profile.layer_count);
        let leaves_per_position =
            worst_case_step_leaf_count_capped_v1(profile, u64::MAX).map(|w| w.div_ceil(u64::from(n_ctx.max(1)))).unwrap_or(u64::MAX);
        let width = palw_held_seat_interval_positions_v1(
            n_ctx,
            replay_ms,
            0,
            window_receipt,
            leaves_per_position,
            PALW_HELD_SEAT_INTERVAL_OPENING_CAP_BYTES_V1,
        );
        let route_note = match route {
            PalwHeldSeatRouteV1::Resume => "the Resume route: a seat fetches this and replays from it".to_string(),
            PalwHeldSeatRouteV1::Recompute => format!(
                "the Recompute route: {n_ctx} positions × {replay_ms} ms fit the seat's budget, so a seat recomputes the prefix \
                 and fetches nothing; resuming would fetch this"
            ),
        };
        held_terms.push(PalwHeldTermRowV1 {
            term: PalwHeldTermV1::SeatFetch,
            need: fetch,
            unit: "bytes",
            order: PalwFitOrderV1::Unpriced,
            note: format!("the state at the last interval's start, one seat holding every layer — {route_note}"),
        });
        held_terms.push(PalwHeldTermRowV1 {
            term: PalwHeldTermV1::SeatReplay,
            need: u64::from(width),
            unit: "positions",
            order: PalwFitOrderV1::Unpriced,
            note: format!(
                "P, at zero fetch time, {replay_ms} ms a position and {window_receipt} DAA; a seat's bandwidth narrows it \
                 (Decision 7)"
            ),
        });
    } else {
        held_terms.push(PalwHeldTermRowV1 {
            term: PalwHeldTermV1::SeatFetch,
            need: 0,
            unit: "bytes",
            order: PalwFitOrderV1::Unpriced,
            note: "a seat recomputes from the ids it holds and fetches nothing (ADR-0082 Decision 9)".into(),
        });
        held_terms.push(PalwHeldTermRowV1 {
            term: PalwHeldTermV1::SeatReplay,
            need: u64::from(n_ctx),
            unit: "positions",
            order: PalwFitOrderV1::Unpriced,
            note: "interval 0 is the whole prefill, recomputed from the prompt (ADR-0086 §1)".into(),
        });
    }

    PalwFitAtV1 { rows, held_terms, arity_played: court_params.dissection_arity(), arity_derived_for_this_row, seat }
}

// =================================================================================================
// Stand-ins — geometries that are NOT classes
// =================================================================================================

/// **Geometries this module prices that no chain registers.** A stand-in is a model's public
/// architecture written in the nearest family's geometry so the tree's own predicates can price
/// it; the numbers it produces are the walls' numbers for THAT geometry, and every substitution
/// is named on the constant. No registration may cite a stand-in, and no id derived from one is a
/// class id: the point of a stand-in is the verdict, never the row.
pub mod stand_ins {
    use crate::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B};

    /// **Kimi K3, as the hybrid family's geometry** (ADR-0097 §1.3; the public model card, read
    /// 2026-09-10: 2.8T total / 104B activated parameters, 93 layers as 69 KDA + 24 gated MLA,
    /// hidden 7,168, 96 attention heads, MLA `kv_lora_rank` 512 + `qk_rope_head_dim` 64,
    /// `v_head_dim` 128, 896 experts of 3,072 with 16 routed and 2 shared per token, vocabulary
    /// 163,840, context 1,048,576).
    ///
    /// The substitutions, each in the direction that REFUSES rather than admits:
    /// * **KDA → GatedDeltaNet** at 56 heads of 128 (`7168 / 128`; the card does not state the KDA
    ///   head count). The recurrence's per-head state is `k_dim × v_dim`, the same shape.
    /// * **MLA → grouped-query attention** at 6 KV heads of 128: MLA caches 576 values a position
    ///   (512 + 64); as rows of 128 that is 4.5 heads, and 6 is the first divisor of 96 above it —
    ///   the cache is priced a third wider than the model's.
    /// * **92 layers at interval 4** — 69 recurrent and 23 attention against the card's 24. The
    ///   family's layer rule is `(i + 1) % interval == 0`, which has no 93-layer spelling with 24
    ///   attention layers; one attention layer fewer prices the row LOWER, and the row is refused
    ///   at every wall this understates by orders of magnitude (§1.3), so the direction is safe
    ///   for the verdict and is named here so it cannot be mistaken for the model.
    /// * The rotary base, the epsilon, the thread count and the tile are the hybrid's own.
    ///
    /// `n_ctx` is the card's; a sweep sets it.
    pub const KIMI_K3_AS_HYBRID_V1: PalwQwen36GeometryV1 = PalwQwen36GeometryV1 {
        layer_count: 92,
        full_attention_interval: 4,
        hidden_dim: 7168,
        attn_heads: 96,
        attn_kv_heads: 6,
        attn_head_dim: 128,
        rope_dims: 64,
        gdn_k_heads: 56,
        gdn_v_heads: 56,
        gdn_head_dim: 128,
        gdn_conv_kernel: 4,
        n_experts: 896,
        experts_per_token: 16,
        moe_dim: 3072,
        shared_dim: 6144,
        attn_output_gate: 1,
        vocab_size: 163_840,
        n_ctx: 1_048_576,
        ..QWEN36_35B_A3B
    };

    /// The card's activated-parameter count, for the artifact estimate a report prints beside the
    /// seat footprint: at the integer family's one byte a weight, an artifact is at least the
    /// parameter count in bytes, and a seat that replays one job of a mixture reads the experts
    /// that job routed to — which, across a 1M-position prefill, is all of them.
    pub const KIMI_K3_TOTAL_PARAMETERS: u64 = 2_800_000_000_000;
    pub const KIMI_K3_ACTIVATED_PARAMETERS: u64 = 104_000_000_000;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The geometry ceiling is `2^24` positions × layers, and its arithmetic is exact at the edge.
    #[test]
    fn the_geometry_ceiling_is_exact_at_its_edge() {
        assert_eq!(palw_geometry_ceiling_fit_v1(1 << 24, 1).verdict, PalwFitVerdictV1::Admitted);
        assert_eq!(palw_geometry_ceiling_fit_v1((1 << 24) + 1, 1).verdict, PalwFitVerdictV1::Refused);
        assert_eq!(palw_geometry_ceiling_fit_v1(1 << 21, 8).verdict, PalwFitVerdictV1::Admitted, "2M × 8 is exactly the ceiling");
        assert_eq!(palw_geometry_ceiling_fit_v1(1 << 21, 9).verdict, PalwFitVerdictV1::Refused, "2M × 9 is past it");
        assert_eq!(palw_widest_context_under_the_geometry_ceiling_v1(28), 599_186);
        assert_eq!(palw_widest_context_under_the_geometry_ceiling_v1(93), 180_400);
        assert_eq!(palw_widest_context_under_the_geometry_ceiling_v1(0), 0);
        assert_eq!(palw_widest_context_under_the_geometry_ceiling_v1(PALW_STEP_MAX_LAYERS + 1), 0);
        assert_eq!(palw_fewest_layers_refused_at_context_v1(1 << 21), Some(9));
        assert_eq!(palw_fewest_layers_refused_at_context_v1(1 << 20), Some(17));
        assert_eq!(palw_fewest_layers_refused_at_context_v1(1), None, "every legal depth fits a one-position context");
        assert_eq!(palw_fewest_layers_refused_at_context_v1((1 << 24) + 1), Some(1), "past the ceiling no depth fits");
        assert_eq!(palw_geometry_ceiling_fit_v1(16, PALW_STEP_MAX_LAYERS + 1).verdict, PalwFitVerdictV1::Refused);
    }

    /// **ADR-0103 Decision 8: the classifier reads growth, never size** — and separates what three
    /// points could not: a dissection at arity 64 gains a round every six doublings, so its first
    /// differences can read `0, 1`, the shape of a line's `d, 2d` at `d = 0`.
    #[test]
    fn the_order_classifier_separates_a_ceilinged_logarithm_from_a_line() {
        use PalwFitOrderV1::*;
        let sweep = |f: &dyn Fn(u64) -> u64, c: u64| (0..=PALW_FIT_ORDER_DOUBLINGS).map(|i| f(c << i)).collect::<Vec<u64>>();
        let ceil_log = |x: u64, k: u64| {
            let (mut rounds, mut span) = (0u64, 1u64);
            while span < x {
                span = span.saturating_mul(k);
                rounds += 1;
            }
            rounds
        };
        assert_eq!(palw_fit_order_v1(&sweep(&|_| 677, 512)), Constant, "a node count");
        assert_eq!(palw_fit_order_v1(&sweep(&|c| 300_000 + 4 * c, 512)), Linear, "a line under a large constant");
        assert_eq!(palw_fit_order_v1(&sweep(&|c| 56 * c.div_ceil(16), 512)), Linear, "a chunk count");
        assert_eq!(palw_fit_order_v1(&sweep(&|c| c * c, 512)), Linear, "a square is at least linear");
        assert_eq!(palw_fit_order_v1(&sweep(&|c| 64 * ceil_log(c, 2), 512)), Logarithmic, "a binary path");
        // Arity 64 over 16-position tiles, placed so the round lands on the third point: the
        // three-point reading is `0, 1` — a "second difference" of a line — and the sweep is not.
        let window = |c: u64| 42 * (2 * ceil_log(c / 16, 64) + 3) + 216;
        let c = 16 * 2_048;
        let three = [window(c), window(2 * c), window(4 * c)];
        assert_eq!(three[1] - three[0], 0);
        assert!(three[2] > three[1], "the three-point form sees a zero then a step");
        assert_eq!(palw_fit_order_v1(&three), Unpriced, "three points are not enough to call an order");
        // Over the sweep the round lands once and not again, so its tail is flat: `Constant`, which
        // is what the gate needs to know — never `Linear`.
        assert_eq!(palw_fit_order_v1(&sweep(&window, c)), Constant, "the sweep never reads the step as a line");
        assert_eq!(palw_fit_order_v1(&sweep(&window, 16 * 2)), Logarithmic, "and where it keeps stepping, it is the logarithm");
        // A sum of seven logarithms stepping together is still one.
        assert_eq!(palw_fit_order_v1(&sweep(&|c| (1..=7).map(|k| ceil_log(c, 1 << k)).sum::<u64>(), 512)), Logarithmic);
        assert_eq!(palw_fit_order_v1(&[1, 2, 4, 8, u64::MAX]), Unpriced, "an unpriced point is not a number");
    }

    /// The prompt-id term is linear flat and logarithmic under the root (ADR-0081 §1.2 / D3).
    #[test]
    fn the_prompt_ids_term_is_linear_flat_and_logarithmic_under_the_root() {
        assert_eq!(palw_prompt_ids_term_on_close_v1(PalwPromptIdsFormV1::Flat, 512), 2_048);
        assert_eq!(palw_prompt_ids_term_on_close_v1(PalwPromptIdsFormV1::Flat, 1 << 21), 8 << 20);
        let at = |n| palw_prompt_ids_term_on_close_v1(PalwPromptIdsFormV1::MerkleV1, n);
        assert_eq!(at(1 << 21) - at(1 << 20), 64, "one doubling of the context is one path element");
        assert!(at(1 << 21) < 4_096, "two million ids open in well under a carrier");
    }
}
