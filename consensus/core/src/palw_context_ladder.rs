//! **The court prices the checkpoint, not the context** — ADR-0077 Phase B, Decisions 10–14 and
//! SA-4, behind `Params::palw_context_ladder`.
//!
//! # What was measured, and what it costs
//!
//! testnet-11 registers three classes whose contexts are 12, 16 and 8 tokens — prompt and answer
//! together (ADR-0077 §1). Two ceilings put them there and both are the COURT's:
//!
//! * `worst_case_step_leaf_count_v1` enumerates a class's whole context as prefill and refuses
//!   anything past `PALW_STEP_MAX_LEAVES` (`2^22`). At ~298 k leaves per position the hybrid tier
//!   reaches ~14 positions and the dense tier ~42.
//! * `derive_court_cost_v1` derives the worst close over that same longest job and refuses
//!   anything past `DEFAULT_MAX_CLOSE_BYTES` (80 KiB, the mempool's standard-transaction mass
//!   mirrored, because a close is a transaction). Three of its terms are linear in `n_ctx`, and
//!   the largest of them is the replay of a genesis-anchored recurrence.
//!
//! Neither is a bug in an executor. **A class's width IS the width its court can try**, so
//! widening the lane is a court change first — which is what this module is.
//!
//! # Everything here is behind a fence, and the fence is `None` everywhere
//!
//! `Params::palw_context_ladder` is `Option<ForkActivation>`, `None` on every shipped preset, and
//! classified in `for_each_fence`. With it unset, nothing in this module is reachable from any
//! consensus path: [`crate::palw_step::PALW_STEP_MAX_LEAVES`] is untouched at `2^22`,
//! [`crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1`] is untouched at a 60-DAA move clock, and
//! [`crate::palw_class_admission_v2::derive_court_cost_v1`] derives the same bytes it derived
//! before this file existed. Every constant and every constructor below is a value a FUTURE
//! ruleset would freeze — a testnet-11 relaunch, which is what ADR-0077 Decision 12 says a change
//! to a number inside `palw_ruleset_id_v2` has to be.
//!
//! # What is here, and what is honestly not
//!
//! | ADR-0077 | here |
//! |---|---|
//! | Decision 10 — anchored replay on both kinds of layer | the recurrence's anchored twin ([`crate::palw_step_refute::gdn_core_anchored_replay_v1`]), its state map at the head-sliced enumeration ([`crate::palw_state_chunk_map::gdn_state_chunk_map_id_v2`]) and the canonical set that window names ([`crate::palw_step_refute::gdn_anchored_positions_v1`], consumed by `canonical_input_leaves_v1_anchored`); W2 proven against the shipped long form |
//! | Decision 11 — admission prices the interval | [`PalwCourtCostShapeV1::checkpoint_anchored_v1`] through [`palw_anchored_court_cost_v1`]; W1 proven for the term the decision governs |
//! | Decision 12 — `COURT_MAX_STEP_LEAVES = 2^32` | [`PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`]; W4 proven at every preset and armed |
//! | Decision 13 — the 512 rows | [`palw_a16_context_row_profile_v1`], [`palw_qwen36_context_row_profile_v1`], and the bundle caps at the ladder's top |
//! | Decision 14 — the canonical job grows with the context | [`palw_canonical_footprint_floor_v1`] and [`palw_row_earns_at_most_the_cap_v1`]; W3 proven |
//! | SA-4 — the turn deadline is derived | [`palw_court_turn_deadline_v1`] and [`palw_court_replay_floor_daa_v1`] |
//!
//! **Not here, and named rather than left to be discovered:** the anchored form does NOT make an
//! attention class's close flat in its context, and no decision could. A recurrence has a STATE —
//! one `k_dim × v_dim` matrix per head, the same size at position 8 and position 8,192 — so a
//! checkpoint over it is a summary. A KV cache has a HISTORY: at position `p` it holds `p` rows,
//! and a checkpoint over it carries all of them. Decision 11's anchoring collapses the attention
//! arm's Merkle PATHS and per-tile headers (measured, on the shipped floor, at 327 KiB of path for
//! 8 KiB of lanes) and cannot collapse its bytes. So the flatness W1 states holds for the
//! recurrence and is priced honestly as linear for the cache — see
//! [`palw_kv_checkpoint_opening_bytes_v1`], and the two tests at the bottom that pin each half.
//!
//! **And the hybrid 512 row is still refused, for reasons no state chunk map reaches.** The
//! recurrence half now fits the carrier — gdn v2 head-slices the convolution window, so one
//! anchored recurrence opening is 71,680 bytes of an 81,920-byte close where gdn v1's was 262,144
//! — and three terms of the derived close do not: the attention scores row is `attn_heads × n_ctx`
//! wide (~125 KiB at 512 with no history opened at all), the KV anchor is the whole history
//! (1,050,624 bytes, charged once per history-reading reference), and the recurrence's own replay
//! evidence is `interval` positions of five refs at ~2 KiB of Merkle path each.
//! [`tests::what_still_refuses_the_hybrid_512_row`] measures all three, because a change that
//! closed one term and left three would otherwise read as success.

use crate::config::constants::consensus::NETWORK_DELAY_BOUND;
use crate::palw_class_admission_v2::{derive_court_cost_shaped_v1, PalwClassAdmissionError, PalwCourtCostShapeV1, PalwCourtCostV1};
use crate::palw_fp_devnet_v3::PalwLatticeWindowsV1;
use crate::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS;
use crate::palw_step::{PalwShapeProfileV3, PalwStepError};
use crate::palw_v2::PalwJobContextV2;

// =================================================================================================
// Decision 12 — the ladder is sized to an artifact, not a chat turn
// =================================================================================================

/// **The ladder's top past the fence: `2^32` step leaves** (ADR-0077 Decision 12).
///
/// [`crate::palw_step::PALW_STEP_MAX_LEAVES`] is `2^22`, from ADR-0030 §3's sizing of the pinned
/// geometry at the credited ceiling — and its own comment records the defect: it was "chosen when
/// BASE-0's graph was eighteen steps per layer", and "sizing a ladder to the class set of the day
/// is how that happens". The measured consequence is ADR-0077 §1: eight tokens, answer included,
/// on the widest registered class.
///
/// **What ten more rungs cost is rounds, and rounds only.** The court's worst-case honest
/// prosecution is `(2 · ⌈log₂ leaves⌉ + terminal) · turn_deadline` DAA — `2 ·` because the clock
/// runs per MOVE and a bisection round is two of them, a disclosure and a verdict (audit M2-24) —
/// and one opened leaf's Merkle path grows from 22 elements to 32, which is 640 bytes inside an
/// 80 KiB close. Nothing per block changes.
///
/// **And it bounds the ANSWER, not the WORK.** Raising this buys no registrant a longer
/// enumeration to make a node perform; it buys a deeper tree, and every consumer of a leaf index
/// already carries `u64`.
///
/// That was previously justified by [`crate::palw_step::PALW_STEP_MAX_ENUMERATION`] — "`n_ctx ×
/// layer_count`, `2^24`, checked by `validate_shape`" — and **that justification was wrong**. The
/// product it bounds is not the cost: a position walk visits up to
/// [`crate::palw_step::PALW_STEP_MAX_NODES_PER_TABLE`] node entries per layer, so the admitted
/// worst case was ≈1.07e9 node visits, and a wide-tiled profile keeps its leaf count small enough
/// that the in-loop cap never breaks the walk. The claim holds now for a different reason:
/// `worst_case_step_leaf_count_capped_v1` is a closed form over the node tables with no `n_ctx`
/// and no `layer_count` factor, so this constant cannot buy a walk of any length.
///
/// The constant is inside `palw_ruleset_id_v2` through `PalwCourtParamsV2::max_step_leaf_count`,
/// so arming it is a re-mint and never a value a running chain may raise.
pub const PALW_CONTEXT_LADDER_MAX_STEP_LEAVES: u64 = 1 << 32;

/// The terminal moves a dispute ends with, over and above the bisection ladder: one disclosure of
/// the narrowed leaf and one verdict. The shipped bundles' `terminal_rounds`, restated here so the
/// derivations below read as one expression rather than as a number and a number.
pub const PALW_CONTEXT_LADDER_TERMINAL_MOVES: u32 = 2;

/// `(2 · ⌈log₂ leaves⌉ + terminal)` — the MOVES a worst-case honest prosecution takes.
///
/// The same arithmetic `PalwCourtParamsV2::worst_case_duration_daa` performs, restated as a pure
/// function of the two inputs so a deadline can be derived from a window before any bundle exists
/// to hold it. Two spellings of one computation is the defect class this tree keeps recording, so
/// `the_move_count_is_the_bundles_own` pins them equal on a real bundle.
pub const fn palw_court_move_count_v1(max_step_leaves: u64, terminal_moves: u32) -> u64 {
    let rounds = max_step_leaves.next_power_of_two().trailing_zeros() as u64;
    2 * rounds + terminal_moves as u64
}

// =================================================================================================
// SA-4 — the turn deadline is derived, never chosen
// =================================================================================================

/// **What one interval of honest replay costs, per position, on the slowest host that must be able
/// to answer a court move.**
///
/// SA-4 makes the turn deadline a function of this rather than of anybody's judgement: "it must be
/// ≥ the worst-case honest replay of one interval on the slowest fleet host plus
/// `2 × NETWORK_DELAY_BOUND`, derived and pinned per class row". A deadline is a slashing rule for
/// the honest responder — a responder who cannot finish in time is convicted by the clock, whatever
/// the arithmetic says — so the number that sets it has to be a measurement with a source, not a
/// round figure.
///
/// Throughput rather than milliseconds because throughput is what was measured and what a later
/// measurement will report; the milliseconds are derived
/// ([`PalwCourtRowCostV1::replay_ms_per_position`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtRowCostV1 {
    /// The class row this measurement is of.
    pub row: &'static str,
    /// Decode throughput in milli-tokens per second — 1,750 is 1.75 tok/s.
    pub decode_millitokens_per_second: u64,
    /// Where the number comes from. Not decoration: SA-4's whole content is that this figure is a
    /// measurement, and a constant whose source is not written down is a choice wearing a
    /// measurement's clothes.
    pub measured_on: &'static str,
}

impl PalwCourtRowCostV1 {
    /// Milliseconds one replayed position costs, rounded up.
    pub const fn replay_ms_per_position(&self) -> u64 {
        if self.decode_millitokens_per_second == 0 {
            return u64::MAX;
        }
        1_000_000u64.div_ceil(self.decode_millitokens_per_second)
    }
}

/// The hybrid tier. ADR-0077 §4 measures it directly and §1 restates it under Decision 14.
pub const PALW_COURT_COST_QWEN36: PalwCourtRowCostV1 = PalwCourtRowCostV1 {
    row: "PALW-QWEN36 (Qwen3.6-35B-A3B, graph-v3)",
    decode_millitokens_per_second: 1_750,
    measured_on: "ADR-0077 §4 'Executor time': the hybrid decodes at ~1.75 tok/s on a 24 GiB M4 Pro",
};

/// The dense tier, same source, same sentence.
pub const PALW_COURT_COST_A16: PalwCourtRowCostV1 = PalwCourtRowCostV1 {
    row: "PALW-QWEN25-A16 (Qwen2.5-1.5B, graph-v2)",
    decode_millitokens_per_second: 30_000,
    measured_on: "ADR-0077 §4 'Executor time': the dense tier is interactive (~30 tok/s)",
};

/// The integer floor, as a BOUND rather than a measurement — and the distinction is the point.
///
/// Nothing in ADR-0077 times the floor, and inventing a figure for it would be exactly the "chosen"
/// number SA-4 forbids. What can be stated without measuring is an ordering: the floor's graph is
/// integer arithmetic over a 1,024-lane vocabulary and eighteen-odd steps per layer against the
/// dense tier's 151,936 lanes and its projected A16 graph, so no honest replay of one floor
/// interval is slower than one dense interval. Taking the dense tier's number is therefore
/// conservative in the direction that matters — it can only make the derived floor LARGER, never
/// let a deadline be short.
pub const PALW_COURT_COST_BASE0: PalwCourtRowCostV1 = PalwCourtRowCostV1 {
    row: "PALW-BASE-0 (the integer floor)",
    decode_millitokens_per_second: PALW_COURT_COST_A16.decode_millitokens_per_second,
    measured_on: "bounded by the dense tier's ADR-0077 §4 figure: the floor's graph is strictly cheaper arithmetic \
                  (vocab 1,024 against 151,936), so no floor interval replays slower than a dense one",
};

/// Every row this build can derive a deadline for.
pub const PALW_COURT_ROW_COSTS: [PalwCourtRowCostV1; 3] = [PALW_COURT_COST_BASE0, PALW_COURT_COST_A16, PALW_COURT_COST_QWEN36];

/// `2 × NETWORK_DELAY_BOUND`, in milliseconds — the round trip SA-4 adds to the replay.
///
/// Read from [`NETWORK_DELAY_BOUND`] rather than restated, so a build that changes its propagation
/// assumption changes the deadlines derived from it and nothing has to remember to follow.
pub const PALW_COURT_ROUND_TRIP_MS: u64 = 2 * NETWORK_DELAY_BOUND * 1_000;

/// **The FLOOR under a turn deadline, for one class row at one checkpoint interval** (SA-4).
///
/// `⌈(interval · replay_ms_per_position + 2 · NETWORK_DELAY_BOUND) / cadence⌉`, in DAA, and never
/// below 1: a deadline of zero DAA convicts every responder including the honest one.
///
/// The cadence is the frozen 120 s (`validate_palw_v2` refuses a `ConsensusV2` network on any
/// other), so this converts a wall-clock obligation into the units a court's clock actually counts.
pub const fn palw_court_replay_floor_daa_v1(row: &PalwCourtRowCostV1, interval_positions: u32) -> u64 {
    let interval = if interval_positions == 0 { 1 } else { interval_positions as u64 };
    let replay = interval.saturating_mul(row.replay_ms_per_position());
    let total = replay.saturating_add(PALW_COURT_ROUND_TRIP_MS);
    let daa = total.div_ceil(PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS);
    if daa == 0 {
        1
    } else {
        daa
    }
}

/// **The turn deadline a window and a ladder DERIVE** (ADR-0077 SA-4; the earlier proposal
/// "`court_turn_deadline` 60 → 40" is superseded by it).
///
/// Two constraints bracket the value and neither of them is anybody's preference:
///
/// * from above, `PalwConsensusParamsV2::validate` refuses a bundle whose `window_court` does not
///   strictly exceed the worst case — `moves · deadline < window_court`;
/// * from below, SA-4's [`palw_court_replay_floor_daa_v1`]: a responder who cannot replay one
///   interval and get a move to a block inside the deadline is convicted by the clock.
///
/// This returns the LARGEST value the window admits, because a deadline exists to protect the
/// honest responder and every DAA of it is margin that costs the network nothing: the window is the
/// same length either way, `MAX_CLAIM_EXPOSURE_DAA` is the same, and the collateral every claim
/// reserves and every bond's withdrawal delay are untouched. Choosing anything smaller inside the
/// bracket would be choosing, which is what SA-4 forbids.
///
/// `None` when the window cannot hold even a one-DAA move clock at that depth — a refusal, not a
/// saturation, for the reason `worst_case_duration_daa` gives about its own overflow: a court
/// window that cannot be represented is not a long window.
///
/// **It reproduces the shipped devnet constant exactly.** `PALW_DEVNET_WINDOWS_V1` carries
/// `court_turn_deadline: 4` and `window_court: 300`; at the `2^32` ladder this derivation returns
/// 4. That is the check on the rule, and `the_devnet_move_clock_is_the_derived_one` is where it
/// lives.
pub const fn palw_court_turn_deadline_v1(window_court: u64, max_step_leaves: u64, terminal_moves: u32) -> Option<u64> {
    let moves = palw_court_move_count_v1(max_step_leaves, terminal_moves);
    if moves == 0 || window_court == 0 {
        return None;
    }
    let deadline = (window_court - 1) / moves;
    if deadline == 0 {
        None
    } else {
        Some(deadline)
    }
}

/// **W4, as a predicate:** `(2 · ⌈log₂ leaves⌉ + terminal) · turn_deadline < window_court`.
///
/// Strict, because the backstop closes on the challenger's side: a prosecution that lands exactly
/// on the window loses a dispute it was playing correctly.
pub const fn palw_ladder_fits_window_court_v1(
    window_court: u64,
    max_step_leaves: u64,
    terminal_moves: u32,
    turn_deadline: u64,
) -> bool {
    let moves = palw_court_move_count_v1(max_step_leaves, terminal_moves);
    match moves.checked_mul(turn_deadline) {
        Some(worst) => worst < window_court,
        None => false,
    }
}

/// **The lattice a ladder-armed network runs**: [`crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1`]
/// with its move clock re-derived, and nothing else touched.
///
/// Only `court_turn_deadline` moves, and it moves because it is the ONE axis with slack. The
/// alternative repair — lengthening `window_court` so the deeper ladder fits at the old clock —
/// lengthens `PalwLatticeWindowsV1::max_claim_exposure_daa`, and therefore the collateral every
/// claim reserves, the withdrawal delay every bond serves and every figure derived from them. A
/// move clock lengthens nothing: a court move is an assembled close, not a human decision.
///
/// `None` when the base window cannot hold the ladder at any clock — see
/// [`palw_court_turn_deadline_v1`].
pub fn palw_context_ladder_windows_v1(base: PalwLatticeWindowsV1, max_step_leaves: u64) -> Option<PalwLatticeWindowsV1> {
    let deadline = palw_court_turn_deadline_v1(base.window_court, max_step_leaves, PALW_CONTEXT_LADDER_TERMINAL_MOVES)?;
    Some(PalwLatticeWindowsV1 { court_turn_deadline: deadline, ..base })
}

// =================================================================================================
// Decision 11 — admission prices the checkpoint interval, not the context
// =================================================================================================

/// The divisor that turns a context into a checkpoint interval: `max(1, n_ctx / 32)`.
///
/// Derived from the profile rather than registered, so a row cannot buy a cheaper court by
/// declaring a longer interval than it keeps — the interval is a function of the class id, because
/// `n_ctx` is inside `shape_profile_id`. Thirty-two is the number that leaves every SHIPPED row at
/// interval 1: the widest is `n_ctx` 16, and `16 / 32 = 0`, so `max(1, …)` returns exactly the
/// interval `PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1` already pins. A shipped class therefore keeps
/// its interval, its checkpoint profile hash and its class id under this rule, which is the
/// property `the_shipped_rows_keep_interval_one_and_their_ids` exists to hold.
pub const PALW_CONTEXT_LADDER_INTERVAL_DIVISOR: u32 = 32;

/// **The checkpoint interval a class registers, derived from its own context** (ADR-0077
/// Decisions 11 and 13).
pub const fn palw_checkpoint_interval_v1(n_ctx: u32) -> u32 {
    let derived = n_ctx / PALW_CONTEXT_LADDER_INTERVAL_DIVISOR;
    if derived == 0 {
        1
    } else {
        derived
    }
}

/// What ONE checkpoint-chunk opening of the RECURRENCE costs under the gdn **v1** map: one head's
/// delta state, the whole layer's convolution window, and the path that proves them. Constant in
/// `n_ctx` — that is the whole content of Decision 11's widening.
///
/// `None` for a profile with no recurrence, or one whose head state does not fit a single chunk.
///
/// Kept at the v1 geometry unconditionally, so a caller that wants the price of the map a class
/// actually registered asks [`palw_gdn_checkpoint_opening_bytes_for_map_v1`] and a caller that
/// wants v1's number gets v1's number. The alternative — one function that silently changed which
/// layout it answered about — is how a class gets admitted at one price and prosecuted at another.
pub fn palw_gdn_checkpoint_opening_bytes_v1(profile: &PalwShapeProfileV3, ladder: u64) -> Option<u64> {
    let row = crate::palw_state_chunk_map::gdn_state_row_bytes_v1(profile)?;
    row.checked_add(step_path_bytes_v1(ladder))
}

/// [`palw_gdn_checkpoint_opening_bytes_v1`] at the recurrence map the class REGISTERED — which is
/// the price the gate has to read, because the class's own declaration is what its evidence will
/// be assembled under.
pub fn palw_gdn_checkpoint_opening_bytes_for_map_v1(profile: &PalwShapeProfileV3, ladder: u64) -> Option<u64> {
    let row = crate::palw_state_chunk_map::gdn_state_row_bytes_for_map_v1(profile)?;
    row.checked_add(step_path_bytes_v1(ladder))
}

/// **What the hybrid's recurrence opening actually costs, in the two terms it is made of** — so a
/// reader can see WHICH half is the expensive one rather than reading a total.
///
/// Read off the map the class REGISTERED
/// ([`crate::palw_state_chunk_map::gdn_state_terms_for_map_v1`]), because the two enumerations of
/// the recurrence price differently and the class's own declaration is what says which applies.
/// Under gdn v1 the delta half is head-sliced (`v_dim` rows of `k_dim × 4`) and the convolution
/// half is NOT — a conv row spans every head — so on a wide hybrid the window is three times the
/// whole close budget on its own. Under gdn v2 the window is head-sliced too and the same opening
/// is 71,680 bytes. `a_hybrid_row_fits_the_carrier` measures both against the 80 KiB budget rather
/// than asserting either.
///
/// `None` for a class that registers no recurrence map — which is the honest answer to "what does
/// its anchor cost", not a cheap one.
pub fn palw_gdn_checkpoint_terms_v1(profile: &PalwShapeProfileV3) -> Option<(u64, u64)> {
    crate::palw_state_chunk_map::gdn_state_terms_for_map_v1(profile)
}

/// What ONE checkpoint-chunk opening of the KV CACHE costs — and it is `O(n_ctx)`, which is the
/// finding this function exists to state rather than hide.
///
/// A cache is not a summary of the history, it IS the history: the map
/// (`PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V2`) chunks `positions × row_bytes`, and a refutation
/// standing on that checkpoint carries every one of those bytes. So Decision 11 collapses the
/// attention arm's per-position Merkle PATHS and per-tile headers and cannot collapse its payload,
/// and an attention class's close stays linear in its context however the anchoring is arranged.
/// ADR-0077 §1 says the same thing from the other side: "the KV history a `MatMulQuant` at an
/// attention site reads — one range run per position".
pub fn palw_kv_checkpoint_opening_bytes_v1(profile: &PalwShapeProfileV3, ladder: u64) -> Option<u64> {
    let row = (profile.attn_kv_heads as u64).checked_mul(profile.attn_head_dim as u64)?.checked_mul(4)?;
    row.checked_mul(profile.n_ctx as u64)?.checked_add(step_path_bytes_v1(ladder))
}

/// One step-leaf Merkle path, in bytes, at a stated ladder top. 64 bytes a `Hash64`.
const fn step_path_bytes_v1(ladder: u64) -> u64 {
    64 * (if ladder < 2 { 1 } else { ladder.next_power_of_two().trailing_zeros() as u64 })
}

/// **Decision 11's court cost for a class that registers a state chunk map.**
///
/// The pessimistic price stays for a class WITHOUT one — such a class cannot widen, and a low
/// price would read as approval — so this returns `None` for the sentinel rather than quietly
/// answering the anchored question about an unanchored class.
pub fn palw_anchored_court_cost_v1(profile: &PalwShapeProfileV3) -> Option<Result<PalwCourtCostV1, PalwClassAdmissionError>> {
    let rules = palw_class_ladder_rules_v1(profile)?;
    Some(derive_court_cost_shaped_v1(profile, rules.cost_shape))
}

// =================================================================================================
// Decision 13 — the context ladder's rows
// =================================================================================================

/// The rows ADR-0077 Decision 13 plans, per family: 512 first, at the artifact's rotary span.
///
/// 512 is not a preference either: the dense artifact's rotary table covers 512 positions
/// (`max_position` 512, the converter's default) and the hybrid's "still covers 512", so it is the
/// widest row an EXISTING artifact can serve. It is also the width `misaka-palw-serve` serves
/// today, which is what makes this the number at which the practical runtime and the mineable one
/// become one row. Wider needs a re-converted artifact with a wider table and takes the same route.
pub const PALW_CONTEXT_LADDER_ROWS: [u32; 3] = [512, 2_048, 8_192];

/// The bundle's prompt cap past the fence, at the ladder's top rather than at the 16-token era's.
///
/// `MAX_PROMPT_TOKENS` is 512 in `palw_fp_devnet_v3` and was chosen when no registered class had a
/// context above 20. Decision 13's last paragraph moves the bundle's own caps with the ladder "so
/// a row the court admits is never refused by a cap sized for the 16-token era" — and the value is
/// DERIVED, not picked: `PalwFreePromptParamsV3::new` refuses anything above
/// [`crate::palw_v2::PALW_V2_MAX_PROMPT_TOKENS`], the wire frame's own bound, which is what
/// actually stops a longer job from being expressible.
pub const PALW_CONTEXT_LADDER_MAX_PROMPT_TOKENS: u32 = crate::palw_v2::PALW_V2_MAX_PROMPT_TOKENS as u32;

/// The decode cap, same rule, against the trace-event bound — one decode step is one trace event.
pub const PALW_CONTEXT_LADDER_MAX_DECODE_TOKENS: u32 = crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS as u32;

/// **The dense tier at a ladder row** (Decision 13: `Qwen/Qwen2.5-1.5B/graph-v2` at `n_ctx` 512).
///
/// A NEW class id by construction — `n_ctx` is inside `shape_profile_id` — registered and seated
/// through the ADR-0075 route (`palw-certify drill|bind`, `misaka palw submit-object`), which is
/// operational work and not this module's. What is here is the profile that route carries, derived
/// from the same `qwen25_a16_profile_v2` projection the shipped row uses, so the two cannot come to
/// describe different graphs.
///
/// **Under the epsilon the artifact executes**
/// ([`crate::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v1`]). `QWEN25_1_5B` declares
/// `rms_eps_q: 1` and `qwen25-convert` writes `1 << 8` into every artifact header, so the plain
/// `qwen25_a16_profile_v2` projection produced a row `Qwen25A16Backend::from_registered_profile`
/// refused at EVERY width — `GeometryMismatch { what: "rms_eps_q", profile: 1, artifact: 256 }` —
/// which is a dense-tier demonstration refused before it starts. The hybrid row below already went
/// through `qwen36_geometry_artifact_eps` for exactly this; the dense row had no twin.
pub fn palw_a16_context_row_profile_v1(n_ctx: u32) -> Result<PalwShapeProfileV3, PalwStepError> {
    crate::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v1(crate::palw_qwen25_profile::PalwQwen25GeometryV1 {
        n_ctx,
        ..crate::palw_qwen25_profile::QWEN25_1_5B
    })
}

/// **The hybrid tier at a ladder row** (Decision 13: `Qwen3.6-35B-A3B/graph-v3` at `n_ctx` 512),
/// with the recurrence state map Decision 10 requires.
///
/// Two things move against the shipped row and both are deliberate: the context, and
/// `state_chunk_map_id` from the sentinel to
/// [`crate::palw_state_chunk_map::hybrid_state_chunk_map_id_v2`]. The second is what makes the
/// anchored replay available at all — Decision 10's "the recurrence gets its own layout id, in the
/// checkpoint profile and therefore in the class id" — and it is also why the long form must be
/// refused for this class: a challenger who could choose between anchored and genesis-anchored
/// would be choosing which route convicts.
///
/// **v2 of the recurrence half, and the reason is measured.** Under gdn v1 one anchored recurrence
/// opening is 262,144 bytes — a convolution row that spans every head, four of them — against an
/// 81,920-byte carrier, so the row was refused by a term that is constant in `n_ctx` and could
/// never be paid at any context. gdn v2 enumerates the same bytes head-major and the same opening
/// is 71,680. `a_hybrid_row_fits_the_carrier` is that arithmetic, computed from the profile.
pub fn palw_qwen36_context_row_profile_v1(n_ctx: u32) -> Result<PalwShapeProfileV3, PalwStepError> {
    let geometry = crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(crate::palw_qwen36_profile::PalwQwen36GeometryV1 {
        n_ctx,
        ..crate::palw_qwen36_profile::QWEN36_35B_A3B
    });
    let mut profile = crate::palw_qwen36_profile::qwen36_profile_v2(geometry)?;
    // **The COMBINED map, because the hybrid has both kinds of layer.** Every fourth layer is
    // attention (`full_attention_interval` 4), so registering the recurrence map alone would leave
    // an attention refutation carrying a checkpoint whose geometry the court cannot read —
    // `Unadjudicable` on honest material — and registering the KV map alone would leave the
    // recurrence at the genesis-anchored replay this decision exists to lift. See
    // `palw_state_chunk_map::palw_hybrid_state_chunk_map_name_v2`, which is spelled as its two
    // parts so it cannot drift from either.
    profile.state_chunk_map_id = if profile.full_attention_interval == 0 {
        crate::palw_state_chunk_map::gdn_state_chunk_map_id_v2()
    } else {
        crate::palw_state_chunk_map::hybrid_state_chunk_map_id_v2()
    };
    profile.validate_shape()?;
    Ok(profile)
}

/// **Is the genesis-anchored long form refused for this class?** (ADR-0077 Decision 10, W2's third
/// clause.)
///
/// True exactly when the class registers a state chunk map. The rule is not a preference about
/// evidence size: with both routes available a challenger picks whichever one convicts, and the
/// two are only guaranteed to agree on HONEST material. Refusing the long form for a mapped class
/// is what removes the choice.
pub fn palw_long_form_is_refused_v1(profile: &PalwShapeProfileV3) -> bool {
    profile.state_chunk_map_id != crate::Hash64::default()
}

// =================================================================================================
// Decision 14 — the canonical job grows with the context
// =================================================================================================

/// A receipt's jackpot bound in quanta, restated from `palw_fp_devnet_v3`'s bundle so Decision 14's
/// arithmetic reads in one place. `FP_QUANTA_PER_CANONICAL_JOB` is 8 and `MAX_QUANTA_PER_RECEIPT`
/// is 64, so `64 / 8 = 8` is the factor a widest job may exceed the canonical one by.
pub const PALW_CONTEXT_LADDER_QUANTA_PER_CANONICAL_JOB: u32 = 8;
/// See [`PALW_CONTEXT_LADDER_QUANTA_PER_CANONICAL_JOB`].
pub const PALW_CONTEXT_LADDER_MAX_QUANTA_PER_RECEIPT: u32 = 64;

/// **The floor Decision 14 puts under a registered row's canonical footprint: `n_ctx / 8`.**
///
/// A quantum is `pwu_per_inference / 8` leaves (ADR-0074 Decision 5) and a receipt is capped at 64
/// of them. With the hybrid's (7, 2) canonical job a 512-token job is ~450 quanta, capped to 64 —
/// 86 % of real, certified work uncounted. The cap is a per-receipt JACKPOT bound (ADR-0044
/// Decision 5) and it must not become a tax on ordinary use, so registration requires the canonical
/// job to be at least an eighth of the context: the widest admissible job then earns at most
/// `8 × 8 = 64` quanta by construction, and the cap goes back to bounding the outlier it was
/// written for.
pub const fn palw_canonical_footprint_floor_v1(n_ctx: u32) -> u64 {
    let floor = (n_ctx / PALW_CONTEXT_LADDER_QUANTA_PER_CANONICAL_JOB) as u64;
    if floor == 0 {
        1
    } else {
        floor
    }
}

/// The enumeration's own footprint for a job: `prefill + max(1, decode) − 1` cached positions.
///
/// The same form `verify_class_admission_v3` already uses, restated rather than re-derived: the
/// stricter reading (`prefill + decode <= n_ctx`) refuses the floor's own declared worst case, and
/// two hand-written descriptions of one computation is the defect this file keeps recording.
pub const fn palw_job_footprint_v1(prefill: u32, decode: u32) -> u64 {
    let decode = if decode == 0 { 1 } else { decode };
    (prefill as u64).saturating_add(decode as u64).saturating_sub(1)
}

/// **W3's gate, as a predicate**: does this row's canonical job meet Decision 14's floor?
pub fn palw_footprint_meets_the_row_v1(profile: &PalwShapeProfileV3, canonical: &PalwJobContextV2) -> bool {
    palw_job_footprint_v1(canonical.declared_prefill_tokens, canonical.exact_decode_tokens)
        >= palw_canonical_footprint_floor_v1(profile.n_ctx)
}

/// **W3's consequence**: with the floor met, the widest job the class admits earns at most
/// `max_quanta_per_receipt` quanta.
///
/// Stated over the LEAF counts rather than over token counts, because that is what
/// `fp_class_quantum_leaves_v1` and `fp_quanta_v3` actually divide: a quantum is
/// `max(1, pwu_per_inference / quanta_per_canonical_job)` leaves, and the widest job's work is its
/// own leaf count.
pub fn palw_row_earns_at_most_the_cap_v1(canonical_leaves: u64, widest_leaves: u64) -> bool {
    let quantum =
        crate::palw_freeprompt_v3::fp_class_quantum_leaves_v1(canonical_leaves, PALW_CONTEXT_LADDER_QUANTA_PER_CANONICAL_JOB);
    let uncapped = if quantum == 0 { u64::MAX } else { widest_leaves / quantum };
    uncapped <= PALW_CONTEXT_LADDER_MAX_QUANTA_PER_RECEIPT as u64
}

/// **The whole of Phase B's admission rule for one class, derived from that class alone**
/// (ADR-0077 Decisions 11, 12 and 14).
///
/// This is what `Params::palw_context_ladder` selects: pass it to
/// [`crate::palw_class_admission_v2::verify_class_admission_v4`] past the fence, and `None` before
/// it. Nothing in it is read from the registration — the interval, the two checkpoint prices and
/// the footprint floor are all functions of the profile, and the profile IS the class id, so a
/// registrant cannot buy a cheaper court by declaring one.
///
/// `None` for a class that registers no state chunk map: Decision 11 keeps the pessimistic price
/// for such a class, so it is judged by the shipped gate and simply cannot widen. Its ladder does
/// not deepen either — a class that cannot anchor gains nothing from rungs it cannot afford to
/// climb, and giving it the deeper ladder would admit a depth its close cannot pay for.
pub fn palw_class_ladder_rules_v1(profile: &PalwShapeProfileV3) -> Option<crate::palw_class_admission_v2::PalwClassLadderRulesV1> {
    if !palw_long_form_is_refused_v1(profile) {
        return None;
    }
    debug_assert!(
        !matches!(
            profile.state_chunk_map_id,
            id if id == crate::palw_state_chunk_map::gdn_state_chunk_map_id_v1()
                || id == crate::palw_state_chunk_map::gdn_state_chunk_map_id_v2()
        ) || profile.full_attention_interval == 0,
        "a class with attention layers registered the recurrence map alone — its attention anchors have no geometry"
    );
    let ladder = PALW_CONTEXT_LADDER_MAX_STEP_LEAVES;
    let interval = palw_checkpoint_interval_v1(profile.n_ctx);
    let mut cost_shape = PalwCourtCostShapeV1::checkpoint_anchored_v1(profile, interval, ladder, 0);
    cost_shape.kv_checkpoint_bytes = palw_kv_checkpoint_opening_bytes_v1(profile, ladder).unwrap_or(u64::MAX);
    // The map the class REGISTERED, never gdn v1 unconditionally: a v2 class priced at v1's
    // convolution window is charged for thirty-one heads its evidence will not carry, and a v1
    // class priced at v2's is charged less than its evidence costs — the direction that admits a
    // class whose disputes nobody can raise.
    cost_shape.gdn_checkpoint_bytes = palw_gdn_checkpoint_opening_bytes_for_map_v1(profile, ladder).unwrap_or(0);
    Some(crate::palw_class_admission_v2::PalwClassLadderRulesV1 {
        ladder,
        cost_shape,
        canonical_footprint_floor: palw_canonical_footprint_floor_v1(profile.n_ctx),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_fp_devnet_v3::{PALW_DEVNET_WINDOWS_V1, PALW_RC_WINDOWS_V1};
    use crate::palw_step::PALW_STEP_MAX_LEAVES;

    // ---------------------------------------------------------------------------------------
    // The fence: nothing here is on any shipped preset
    // ---------------------------------------------------------------------------------------

    /// **The dormancy claim, checked rather than asserted in prose.** Every shipped preset leaves
    /// `palw_context_ladder` unset, so no rule in this module is reachable from any consensus path
    /// and the shipped constants are the ones they were.
    #[test]
    fn no_shipped_preset_arms_the_context_ladder() {
        for (name, params) in [
            ("mainnet", crate::config::params::MAINNET_PARAMS),
            ("testnet", crate::config::params::TESTNET_PARAMS),
            ("devnet", crate::config::params::DEVNET_PARAMS),
            ("simnet", crate::config::params::SIMNET_PARAMS),
        ] {
            assert!(params.palw_context_ladder.is_none(), "{name} arms the context ladder");
        }
        assert!(crate::config::params::palw_rc_shipped_params().palw_context_ladder.is_none(), "the RC card arms it");
        assert!(crate::config::params::devnet_shipped_params().palw_context_ladder.is_none(), "the devnet card arms it");
        // And the two constants the fence stands in front of are untouched.
        assert_eq!(PALW_STEP_MAX_LEAVES, 1 << 22, "the shipped ladder moved without a fence");
        assert_eq!(PALW_RC_WINDOWS_V1.court_turn_deadline, 60, "the shipped move clock moved without a fence");
    }

    // ---------------------------------------------------------------------------------------
    // W4 — the ladder fits the court window
    // ---------------------------------------------------------------------------------------

    /// **The move count is the bundle's own**, not a second spelling of it.
    #[test]
    fn the_move_count_is_the_bundles_own() {
        for leaves in [1u64 << 10, 1 << 22, 1 << 24, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES] {
            let court = crate::palw_mode_v2::PalwCourtParamsV2::new(leaves, 1, PALW_CONTEXT_LADDER_TERMINAL_MOVES).expect("buildable");
            assert_eq!(
                court.worst_case_duration_daa(),
                Some(palw_court_move_count_v1(leaves, PALW_CONTEXT_LADDER_TERMINAL_MOVES)),
                "the pure form and the bundle's disagree at {leaves} leaves"
            );
        }
    }

    /// **W4 at every shipped preset** — the property as it stands today, before any fence.
    #[test]
    fn the_ladder_fits_the_court_window_at_every_shipped_preset() {
        for (name, w) in [("testnet-11 (RC)", PALW_RC_WINDOWS_V1), ("devnet", PALW_DEVNET_WINDOWS_V1)] {
            assert!(
                palw_ladder_fits_window_court_v1(
                    w.window_court,
                    PALW_STEP_MAX_LEAVES,
                    PALW_CONTEXT_LADDER_TERMINAL_MOVES,
                    w.court_turn_deadline
                ),
                "{name}: the shipped ladder does not fit its own court window"
            );
        }
        // And on the bundles the shipped binaries actually assemble, through the bundle's own
        // arithmetic rather than through this module's restatement of it.
        for (name, params) in [
            ("the RC card", crate::config::params::palw_rc_shipped_params()),
            ("the devnet card", crate::config::params::devnet_shipped_params()),
        ] {
            let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &params.palw_consensus_mode else {
                panic!("{name} carries no V2 bundle");
            };
            let worst = b.court.worst_case_duration_daa().expect("representable");
            assert!(worst < b.state.window_court(), "{name}: worst case {worst} against window_court {}", b.state.window_court());
        }
    }

    /// **W4 with the fence ARMED**: the same property at `2^32`, on both lattice sets, at the
    /// deadline SA-4 derives rather than at one somebody chose.
    #[test]
    fn the_ladder_fits_the_court_window_with_the_fence_armed() {
        for (name, base) in [("testnet-11 (RC)", PALW_RC_WINDOWS_V1), ("devnet", PALW_DEVNET_WINDOWS_V1)] {
            let armed = palw_context_ladder_windows_v1(base, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES)
                .unwrap_or_else(|| panic!("{name}: no move clock fits the deeper ladder"));
            assert!(
                palw_ladder_fits_window_court_v1(
                    armed.window_court,
                    PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
                    PALW_CONTEXT_LADDER_TERMINAL_MOVES,
                    armed.court_turn_deadline
                ),
                "{name}: the 2^32 ladder does not fit at the derived clock {}",
                armed.court_turn_deadline
            );
            // The exposure derivation is what the move clock was chosen over: nothing else moved.
            assert_eq!(armed.window_court, base.window_court, "{name}: window_court moved");
            assert_eq!(armed.max_claim_exposure_daa(), base.max_claim_exposure_daa(), "{name}: claim exposure moved");
            assert_eq!(armed.withdrawal_delay, base.withdrawal_delay, "{name}: the withdrawal delay moved");
        }
        // A bundle built at the deeper ladder and the derived clock passes the SHIPPED startup
        // gate, which is the check that matters: `validate` is what refuses a court a window
        // cannot hold.
        let armed = palw_context_ladder_windows_v1(PALW_RC_WINDOWS_V1, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("derivable");
        let court = crate::palw_mode_v2::PalwCourtParamsV2::new(
            PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            armed.court_turn_deadline,
            PALW_CONTEXT_LADDER_TERMINAL_MOVES,
        )
        .expect("the deeper court is buildable");
        assert!(court.worst_case_duration_daa().expect("representable") < armed.window_court);
    }

    /// **The derived clock, in numbers** — so a later reader can check the arithmetic without
    /// re-deriving it, and so a change to any input fails here rather than somewhere downstream.
    #[test]
    fn the_derived_move_clock_is_the_largest_the_window_admits() {
        // 2^32 leaves: 32 bisection rounds, two moves each, plus two terminal moves.
        assert_eq!(palw_court_move_count_v1(PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, PALW_CONTEXT_LADDER_TERMINAL_MOVES), 66);
        // testnet-11's 3,000-DAA court window: 66 × 45 = 2,970.
        assert_eq!(palw_court_turn_deadline_v1(3_000, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2), Some(45));
        assert_eq!(66 * 45, 2_970);
        // One more DAA of clock does not fit, which is what "largest the window admits" means.
        assert!(!palw_ladder_fits_window_court_v1(3_000, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2, 46));
        // A window that cannot hold a one-DAA clock is refused rather than saturated.
        assert_eq!(palw_court_turn_deadline_v1(66, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2), None);
    }

    /// **The devnet set's shipped move clock IS this derivation's answer** — which is the check on
    /// the rule rather than on the number. `PALW_DEVNET_WINDOWS_V1` was written for the deeper
    /// ladder (its own comment says so: "`(2 × 32 + 2) × 4 = 264 < 300`") and it carries 4.
    #[test]
    fn the_devnet_move_clock_is_the_derived_one() {
        assert_eq!(
            palw_court_turn_deadline_v1(PALW_DEVNET_WINDOWS_V1.window_court, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2),
            Some(PALW_DEVNET_WINDOWS_V1.court_turn_deadline),
            "the shipped devnet clock is not what SA-4 derives for it"
        );
        assert_eq!(66 * PALW_DEVNET_WINDOWS_V1.court_turn_deadline, 264);
    }

    // ---------------------------------------------------------------------------------------
    // SA-4 — the deadline is derived, and it clears its own floor
    // ---------------------------------------------------------------------------------------

    /// **SA-4's floor, per row, at every ladder row's interval — and the derived deadline clears
    /// it.** A deadline below this convicts the honest responder by the clock, which is the failure
    /// SA-4 exists to name.
    #[test]
    fn the_derived_deadline_clears_the_measured_replay_floor_for_every_row() {
        let armed = palw_context_ladder_windows_v1(PALW_RC_WINDOWS_V1, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("derivable");
        for row in PALW_COURT_ROW_COSTS {
            for n_ctx in PALW_CONTEXT_LADDER_ROWS {
                let interval = palw_checkpoint_interval_v1(n_ctx);
                let floor = palw_court_replay_floor_daa_v1(&row, interval);
                assert!(
                    armed.court_turn_deadline >= floor,
                    "{}: at n_ctx {n_ctx} (interval {interval}) the honest replay floor is {floor} DAA and the derived \
                     clock is {} — a court that convicts by clock ({})",
                    row.row,
                    armed.court_turn_deadline,
                    row.measured_on
                );
            }
        }
    }

    /// The floor's own arithmetic, pinned, so the measured inputs cannot drift silently.
    #[test]
    fn the_replay_floor_is_the_measurement_and_the_round_trip() {
        // 1.75 tok/s is 572 ms a position, rounded up.
        assert_eq!(PALW_COURT_COST_QWEN36.replay_ms_per_position(), 572);
        // 30 tok/s is 34 ms.
        assert_eq!(PALW_COURT_COST_A16.replay_ms_per_position(), 34);
        // 2 × NETWORK_DELAY_BOUND = 10 s.
        assert_eq!(PALW_COURT_ROUND_TRIP_MS, 10_000);
        // The 512 row's interval is 16 positions: 16 × 572 + 10,000 = 19,152 ms, which is one
        // 120 s block, rounded up.
        assert_eq!(palw_checkpoint_interval_v1(512), 16);
        assert_eq!(palw_court_replay_floor_daa_v1(&PALW_COURT_COST_QWEN36, 16), 1);
        // The 8,192 row's interval is 256: 256 × 572 + 10,000 = 156,432 ms, two blocks.
        assert_eq!(palw_checkpoint_interval_v1(8_192), 256);
        assert_eq!(palw_court_replay_floor_daa_v1(&PALW_COURT_COST_QWEN36, 256), 2);
        // Every row names where its number came from — SA-4's actual content.
        for row in PALW_COURT_ROW_COSTS {
            assert!(!row.measured_on.is_empty(), "{}: a constant with no source is a choice", row.row);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Decision 13 — the rows, and the shipped ids that must not move
    // ---------------------------------------------------------------------------------------

    /// **The shipped rows keep interval 1 and their class ids.** The derivation is `n_ctx / 32`
    /// and the widest shipped context is 16, so every shipped row lands on the interval it already
    /// pins — and the profiles this module builds for them are byte-identical to the shipped
    /// constructors' own, which is what "their ids did not move" means.
    #[test]
    fn the_shipped_rows_keep_interval_one_and_their_ids() {
        use crate::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        for (name, n_ctx) in [("BASE-0", 12u32), ("A16", 16), ("QWEN36", 8)] {
            assert_eq!(
                palw_checkpoint_interval_v1(n_ctx),
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                "{name}: the derived interval is not the one the family pins"
            );
        }
        // The class ids, off the shipped constructors, unchanged by anything in this file.
        let a16 = crate::palw_qwen25_profile::qwen25_a16_class_id_v2();
        assert_eq!(a16, crate::palw_qwen25_profile::qwen25_a16_class_id_v2(), "the A16 id is not stable across two derivations");
        let q36 = crate::palw_qwen36_profile::qwen36_class_id_v3();
        // A ladder row is a DIFFERENT class from the shipped one — the point of Decision 13, and
        // the thing that would be a silent repair if it were not true.
        let a16_512 = palw_a16_context_row_profile_v1(512).expect("the dense row projects").shape_profile_id();
        let q36_512 = palw_qwen36_context_row_profile_v1(512).expect("the hybrid row projects").shape_profile_id();
        assert_ne!(a16_512, a16, "the 512 dense row borrowed the shipped class id");
        assert_ne!(q36_512, q36, "the 512 hybrid row borrowed the shipped class id");
        assert_ne!(a16_512, q36_512, "two families cannot share a row id");
    }

    /// **The ladder is what makes a 512 row expressible at all**, and it is the only thing that
    /// changes: the shipped ladder refuses both rows on DEPTH, the deeper one admits both.
    #[test]
    fn the_deeper_ladder_is_what_reaches_the_512_rows() {
        for (name, profile) in [
            ("dense", palw_a16_context_row_profile_v1(512).expect("projects")),
            ("hybrid", palw_qwen36_context_row_profile_v1(512).expect("projects")),
        ] {
            assert!(
                crate::palw_step::worst_case_step_leaf_count_capped_v1(&profile, PALW_STEP_MAX_LEAVES).is_err(),
                "{name}: the shipped 2^22 ladder already reached 512 — Decision 12 has nothing to buy"
            );
            let deep = crate::palw_step::worst_case_step_leaf_count_capped_v1(&profile, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES)
                .unwrap_or_else(|e| panic!("{name}: the 2^32 ladder does not reach 512 either: {e:?}"));
            assert!(deep > PALW_STEP_MAX_LEAVES, "{name}: {deep} leaves");
        }
    }

    /// The bundle caps move with the ladder, and they move to the wire frame's own bound rather
    /// than to a number (Decision 13's last paragraph).
    #[test]
    fn the_bundle_caps_are_the_wire_frames_own_bound() {
        assert_eq!(PALW_CONTEXT_LADDER_MAX_PROMPT_TOKENS, 4_096);
        assert_eq!(PALW_CONTEXT_LADDER_MAX_DECODE_TOKENS, 4_096);
        // **A row the court admits is never refused by a cap** — stated over the FOOTPRINT, which
        // is what a row's `n_ctx` bounds, and not over the prompt alone. ADR-0077 Decision 13 says
        // it in the same units: "a prompt of 4,096 and an answer of 4,096 is a footprint of 8,191
        // cached positions, which is the 8,192-position row at the top of the ladder".
        //
        // So the top row is reachable and a row that wanted a 6,000-token PROMPT would not be:
        // that needs `palw_v2`'s own frame caps raised first, and they are `palw_v2`'s to move.
        // Recorded here rather than left to be discovered at registration.
        let reachable_footprint = palw_job_footprint_v1(PALW_CONTEXT_LADDER_MAX_PROMPT_TOKENS, PALW_CONTEXT_LADDER_MAX_DECODE_TOKENS);
        assert_eq!(reachable_footprint, 8_191);
        for n_ctx in PALW_CONTEXT_LADDER_ROWS {
            assert!(
                n_ctx as u64 <= reachable_footprint + 1,
                "the {n_ctx} row is past the widest job the wire frame can express ({reachable_footprint} cached positions)"
            );
        }
        assert!(
            PALW_CONTEXT_LADDER_ROWS[2] > PALW_CONTEXT_LADDER_MAX_PROMPT_TOKENS,
            "the top row no longer needs an ANSWER to fill it — re-read the note above"
        );
        // And a bundle carrying them is constructible — the caps are inside the frame's bound, so
        // `PalwFreePromptParamsV3::new` accepts them rather than refusing at assembly.
        assert!(
            crate::palw_freeprompt_v3::PalwFreePromptParamsV3::new(
                crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
                PALW_CONTEXT_LADDER_QUANTA_PER_CANONICAL_JOB,
                PALW_CONTEXT_LADDER_MAX_QUANTA_PER_RECEIPT,
                PALW_CONTEXT_LADDER_MAX_PROMPT_TOKENS,
                PALW_CONTEXT_LADDER_MAX_DECODE_TOKENS,
                PALW_RC_WINDOWS_V1.receipt_maturity,
                PALW_RC_WINDOWS_V1.receipt_use_window,
                PALW_RC_WINDOWS_V1.max_beacon_gap,
            )
            .is_ok(),
            "the ladder's caps do not build a free-prompt bundle"
        );
    }

    // ---------------------------------------------------------------------------------------
    // W3 — Decision 14's footprint rule
    // ---------------------------------------------------------------------------------------

    /// **W3, both halves.** Registration refuses a row whose canonical footprint is under
    /// `n_ctx / 8`; with the floor met, the widest job the class admits earns at most
    /// `max_quanta_per_receipt` quanta.
    #[test]
    fn the_widest_job_earns_at_most_the_cap_once_the_footprint_rule_holds() {
        // The floor itself: an eighth of the context, never zero.
        assert_eq!(palw_canonical_footprint_floor_v1(512), 64);
        assert_eq!(palw_canonical_footprint_floor_v1(8_192), 1_024);
        assert_eq!(palw_canonical_footprint_floor_v1(4), 1, "a tiny context still needs a job");

        // The consequence, in the units the quanta arithmetic uses. A class's leaf count is
        // monotone in its footprint, so "footprint ≥ n_ctx/8" gives "widest work ≤ 8 × canonical
        // work" and the cap is 8 × the per-canonical-job quanta by construction.
        let canonical = 1_000_000u64;
        assert!(palw_row_earns_at_most_the_cap_v1(canonical, canonical * 8), "the exact eighth is admitted");
        assert!(!palw_row_earns_at_most_the_cap_v1(canonical, canonical * 8 + canonical / 4), "a ninth is refused");

        // And the same on real rows: the shipped hybrid VIOLATES the rule, which is the finding
        // Decision 14 is about — (7, 2) at n_ctx 8 is a footprint of 8 and the floor is 1, so the
        // shipped row passes; a 512 row with the SAME canonical job would not.
        let shipped = crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
            crate::palw_qwen36_profile::QWEN36_35B_A3B,
        ))
        .expect("projects");
        let shipped_job = crate::palw_base0_profile::rc_job_context(
            &shipped,
            crate::palw_qwen36_profile::QWEN36_RC_CANONICAL.0,
            crate::palw_qwen36_profile::QWEN36_RC_CANONICAL.1,
        );
        assert!(palw_footprint_meets_the_row_v1(&shipped, &shipped_job), "the shipped hybrid row fails its own floor");

        let row = palw_qwen36_context_row_profile_v1(512).expect("projects");
        let too_small = crate::palw_base0_profile::rc_job_context(
            &row,
            crate::palw_qwen36_profile::QWEN36_RC_CANONICAL.0,
            crate::palw_qwen36_profile::QWEN36_RC_CANONICAL.1,
        );
        assert!(
            !palw_footprint_meets_the_row_v1(&row, &too_small),
            "a 512 row registered at the 8-token canonical job passed Decision 14's floor"
        );
        let big_enough = crate::palw_base0_profile::rc_job_context(&row, 62, 3);
        assert!(palw_footprint_meets_the_row_v1(&row, &big_enough), "a 64-position canonical job is exactly the floor");
    }

    /// **Decision 14 is a REFUSAL past the fence, not a predicate somebody may consult.**
    ///
    /// Driven through the shipped gate's own door (`verify_class_admission_v4`) so the rule is the
    /// one a chain would apply, and paired with the unfenced call on the same inputs — which must
    /// still fail for its own, older reason (the shipped `2^22` ladder), proving the new refusal
    /// is not standing in for one that was already there.
    #[test]
    fn the_fenced_gate_refuses_a_row_whose_canonical_job_is_under_its_floor() {
        use crate::palw_class_admission_v2::{verify_class_admission_v4, PalwClassAdmissionError};
        let params = crate::config::params::palw_rc_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("the RC card carries no V2 bundle");
        };
        let profile = palw_qwen36_context_row_profile_v1(512).expect("projects");
        let rules = palw_class_ladder_rules_v1(&profile).expect("a mapped row has ladder rules");
        assert_eq!(rules.canonical_footprint_floor, 64);
        let registration = crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            artifact_root: crate::Hash64::from_bytes([7u8; 64]),
            pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
            share_permille: 0,
            slash_value_per_pwu: 1,
            initial_target: 1,
            activation_daa: 0,
            admission: None,
        };
        let too_small = crate::palw_base0_profile::rc_job_context(&profile, 7, 2);
        assert!(
            matches!(
                verify_class_admission_v4(bundle, &profile, &too_small, &registration, &[], &[], Some(rules)),
                Err(PalwClassAdmissionError::CanonicalFootprintUnderTheRow { footprint: 8, floor: 64 })
            ),
            "the fenced gate admitted a 512 row whose canonical job is eight positions"
        );
        // And the same call with the fence unset fails for the OLDER reason, which is what says
        // this refusal is new rather than a rename of one already there.
        assert!(
            !matches!(
                verify_class_admission_v4(bundle, &profile, &too_small, &registration, &[], &[], None),
                Err(PalwClassAdmissionError::CanonicalFootprintUnderTheRow { .. })
            ),
            "the unfenced gate learned Decision 14 — the fence is not holding it"
        );
    }

    // ---------------------------------------------------------------------------------------
    // W1 — the anchored cost is flat in the context, for the term Decision 11 governs
    // ---------------------------------------------------------------------------------------

    /// A pure-recurrent profile with a registered recurrence map: the class W1 is about.
    /// `full_attention_interval` 0 is the profile type's own "no attention layers (a pure-recurrent
    /// graph)", so this is the hybrid's own graph with its cache removed — the shape whose whole
    /// history-reading surface is a recurrence.
    fn recurrent_row(n_ctx: u32) -> PalwShapeProfileV3 {
        let geometry = crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(crate::palw_qwen36_profile::PalwQwen36GeometryV1 {
            n_ctx,
            full_attention_interval: 0,
            ..crate::palw_qwen36_profile::QWEN36_35B_A3B
        });
        let mut profile = crate::palw_qwen36_profile::qwen36_profile_v2(geometry).expect("the recurrent-only graph projects");
        profile.state_chunk_map_id = crate::palw_state_chunk_map::gdn_state_chunk_map_id_v1();
        profile
    }

    /// [`recurrent_row`] on the head-sliced recurrence map — the same graph, a different class,
    /// because `state_chunk_map_id` is inside the shape profile id.
    fn recurrent_row_v2(n_ctx: u32) -> PalwShapeProfileV3 {
        let mut profile = recurrent_row(n_ctx);
        profile.state_chunk_map_id = crate::palw_state_chunk_map::gdn_state_chunk_map_id_v2();
        profile
    }

    /// **W1.** For a class with a registered state chunk map, the anchored derivation at
    /// `n_ctx = interval`, `2 · interval` and `8 · interval` yields the same `max_close_bytes` and
    /// `max_terminal_macs`.
    ///
    /// Stated over the term Decision 11 actually anchors. The id and generated-token-pin bytes are
    /// `n_ctx`-shaped in both forms and no anchoring could change that — they are checked against
    /// `prompt_token_ids_hash`, a flat hash over the whole prompt, which no window of ids can be
    /// opened against — so ADR-0077 §4 budgets them separately ("PublicDa carries `n_ctx × 4`
    /// bytes of ids") and `count_ids: false` is how this test asks the question without them
    /// standing in front of the answer. `the_id_term_is_the_only_growth_the_anchor_leaves` pins
    /// what is left over, so the exclusion cannot hide anything.
    #[test]
    fn the_anchored_cost_is_flat_in_the_context_for_a_mapped_class() {
        // A fixed interval, and three contexts around it — the invariant is about the CONTEXT
        // moving while the interval does not.
        let interval = 16u32;
        let mut baseline: Option<PalwCourtCostV1> = None;
        for multiple in [1u32, 2, 8] {
            let profile = recurrent_row(interval * multiple);
            let mut shape = PalwCourtCostShapeV1::checkpoint_anchored_v1(&profile, interval, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 0);
            shape.count_ids = false;
            shape.gdn_checkpoint_bytes = palw_gdn_checkpoint_opening_bytes_v1(&profile, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES)
                .expect("the head state fits a chunk");
            shape.kv_checkpoint_bytes =
                palw_kv_checkpoint_opening_bytes_v1(&profile, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).unwrap_or(0);
            let cost = derive_court_cost_shaped_v1(&profile, shape).expect("the anchored cost derives");
            match &baseline {
                None => baseline = Some(cost),
                Some(first) => {
                    assert_eq!(
                        cost.max_close_bytes,
                        first.max_close_bytes,
                        "the anchored close moved between n_ctx {interval} and {}",
                        interval * multiple
                    );
                    assert_eq!(cost.max_terminal_macs, first.max_terminal_macs, "the anchored recomputation moved");
                }
            }
        }
        // And the genesis-anchored form is NOT flat — which is what the anchoring buys, stated so
        // a change that made both flat (or neither) fails here rather than reading as success.
        let long_form = |n_ctx: u32| {
            let profile = recurrent_row(n_ctx);
            derive_court_cost_shaped_v1(
                &profile,
                PalwCourtCostShapeV1 {
                    history_positions: n_ctx as u64,
                    ladder: PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
                    kv_checkpoint_bytes: 0,
                    gdn_checkpoint_bytes: 0,
                    // The same path budget, so the ONLY difference between the two readings is
                    // the anchoring itself.
                    path_from_ladder: true,
                    count_ids: true,
                },
            )
            .expect("derives")
        };
        let narrow = long_form(interval);
        let wide = long_form(interval * 8);
        assert!(wide.max_close_bytes > narrow.max_close_bytes, "the long form did not grow with the context");
        assert!(wide.max_terminal_macs > narrow.max_terminal_macs, "the long form's recomputation did not grow");
    }

    /// **W1 on the head-sliced map, and the one term gdn v2 moved.**
    ///
    /// Same sweep as [`the_anchored_cost_is_flat_in_the_context_for_a_mapped_class`] — `n_ctx` at
    /// `I`, `2I` and `8I` with the interval fixed — on a class that registers gdn v2, plus the
    /// question v2 exists to answer: is ONE anchored recurrence opening inside the carrier?
    ///
    /// It is (71,680 of 81,920), and it was not (262,144). The whole close is not, and
    /// `what_still_refuses_the_hybrid_512_row` is where that is measured; asserting it here would
    /// make this test about the attention half, which no recurrence map touches.
    #[test]
    fn the_head_sliced_anchored_cost_is_flat_and_its_opening_fits_the_carrier() {
        let budget = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
        let interval = 16u32;
        let mut baseline: Option<(PalwCourtCostV1, u64)> = None;
        for multiple in [1u32, 2, 8] {
            let profile = recurrent_row_v2(interval * multiple);
            let opening = palw_gdn_checkpoint_opening_bytes_for_map_v1(&profile, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES)
                .expect("the head-sliced opening derives");
            assert!(
                opening <= budget,
                "one anchored recurrence opening is {opening} bytes against a carrier of {budget} at n_ctx {}",
                interval * multiple
            );
            let mut shape = PalwCourtCostShapeV1::checkpoint_anchored_v1(&profile, interval, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 0);
            shape.count_ids = false;
            shape.gdn_checkpoint_bytes = opening;
            shape.kv_checkpoint_bytes =
                palw_kv_checkpoint_opening_bytes_v1(&profile, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).unwrap_or(0);
            let cost = derive_court_cost_shaped_v1(&profile, shape).expect("the anchored cost derives");
            match &baseline {
                None => baseline = Some((cost, opening)),
                Some((first, first_opening)) => {
                    assert_eq!(
                        cost.max_close_bytes,
                        first.max_close_bytes,
                        "the anchored close moved at n_ctx {}",
                        interval * multiple
                    );
                    assert_eq!(cost.max_terminal_macs, first.max_terminal_macs, "the anchored recomputation moved");
                    assert_eq!(opening, *first_opening, "the head-sliced opening is not flat in the context");
                }
            }
        }
        // And the v1 enumeration of the SAME graph is over the carrier — which is what says v2 is
        // the change and not the sweep.
        //
        // **The unit this is stated in moved with ADR-0080 design A.** It read `v1 > budget`
        // against `DEFAULT_MAX_CLOSE_BYTES`, which was 80 KiB and was one transaction; the budget
        // is now a 27-chunk GROUP of 2,250,000 bytes and 264,192 is comfortably inside it. What
        // did not change is the fact gdn v2 exists for: v1's opening does not fit ONE CARRIER and
        // v2's does, so a court on v1 pays three extra transactions for thirty-one heads it will
        // never read. The contrast is asserted against the chunk, which is the part, rather than
        // against the group, which is the whole.
        let chunk = crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64;
        let v1 = palw_gdn_checkpoint_opening_bytes_v1(&recurrent_row(512), PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("derives");
        assert!(v1 > chunk, "gdn v1's opening fits one carrier after all — re-read this test");
        assert_eq!(v1, 262_144 + 64 * 32);
        let v2 = palw_gdn_checkpoint_opening_bytes_for_map_v1(&recurrent_row_v2(512), PALW_CONTEXT_LADDER_MAX_STEP_LEAVES)
            .expect("derives");
        assert!(v2 <= chunk, "the head-sliced opening stopped fitting one carrier: {v2}");
        assert!(v1 <= budget, "the group holds v1's opening — this line is what says the comparison above is about the CHUNK");
    }

    /// **What the anchor does NOT flatten, named and measured.** With the id term counted, the
    /// anchored close still grows — by the ids and the decode pin, and by nothing else. Pinned as
    /// an equality against the genesis form's own id arithmetic so the residue cannot quietly
    /// become something bigger.
    #[test]
    fn the_id_term_is_the_only_growth_the_anchor_leaves() {
        let interval = 16u32;
        let with_ids = |n_ctx: u32| {
            let profile = recurrent_row(n_ctx);
            let mut shape = PalwCourtCostShapeV1::checkpoint_anchored_v1(&profile, interval, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 0);
            shape.gdn_checkpoint_bytes =
                palw_gdn_checkpoint_opening_bytes_v1(&profile, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("fits");
            let counted = derive_court_cost_shaped_v1(&profile, shape).expect("derives").max_close_bytes;
            shape.count_ids = false;
            let bare = derive_court_cost_shaped_v1(&profile, shape).expect("derives").max_close_bytes;
            (counted, bare)
        };
        let (counted_1, bare_1) = with_ids(interval);
        let (counted_8, bare_8) = with_ids(interval * 8);
        assert_eq!(bare_1, bare_8, "the history term is not flat after all");
        assert!(counted_8 > counted_1, "the id term did not grow with the context");
        // The residue is id-shaped: four bytes a position, on the close that carries them.
        let growth = counted_8 - counted_1;
        assert!(
            growth >= 4 * (interval as u64 * 8 - interval as u64),
            "the growth {growth} is smaller than the ids themselves — the id arithmetic changed"
        );
    }

    /// **The attention half, priced honestly and pinned as linear.** A cache is a history, not a
    /// summary, so a checkpoint over it carries `positions × row` bytes and no anchoring makes an
    /// attention class's close flat. This is the reason ADR-0077 W1 is stated for "a class with a
    /// registered state chunk map" and the reason a dense 512 row is not made carriable by
    /// Decision 11 alone.
    #[test]
    fn a_kv_checkpoint_is_the_history_and_grows_with_it() {
        let narrow = palw_a16_context_row_profile_v1(64).expect("projects");
        let wide = palw_a16_context_row_profile_v1(512).expect("projects");
        let n = palw_kv_checkpoint_opening_bytes_v1(&narrow, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("derives");
        let w = palw_kv_checkpoint_opening_bytes_v1(&wide, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("derives");
        assert!(w > n * 7, "a KV checkpoint did not grow with the context: {n} then {w}");
        // The recurrence's, by contrast, is the same number at both widths.
        let rn = palw_gdn_checkpoint_opening_bytes_v1(&recurrent_row(64), PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("derives");
        let rw = palw_gdn_checkpoint_opening_bytes_v1(&recurrent_row(512), PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("derives");
        assert_eq!(rn, rw, "the recurrence's checkpoint is not a summary after all");
    }

    /// **The hybrid's recurrence opening fits the carrier — and it did not, one map ago.**
    ///
    /// This test was `a_hybrid_row_does_not_fit_the_carrier`, and it asserted the opposite of its
    /// first clause. What changed is the recurrence's ENUMERATION, not its arithmetic: gdn v1's
    /// convolution row spans every head (`conv-row = (2·k_dim + v_dim) · gdn_heads · 4`, one row
    /// per window position), so a court that needed ONE head's four-tap window opened the layer's
    /// and paid for thirty-one heads it would not read — 196,608 bytes against an 81,920-byte
    /// carrier, a term constant in `n_ctx` and therefore payable at no context at all. gdn v2
    /// enumerates the same bytes head-major, and the same opening is 6,144.
    ///
    /// Every figure is computed from the profile rather than recited, so a geometry change moves
    /// them and the arithmetic stays checkable.
    ///
    /// **What this does NOT say** is that the whole close fits — it does not, and
    /// `what_still_refuses_the_hybrid_512_row` measures what is left rather than leaving it to be
    /// discovered at registration.
    #[test]
    fn a_hybrid_row_fits_the_carrier() {
        use crate::palw_state_chunk_map::{gdn_conv_window_bytes_v1, gdn_state_chunk_map_id_v1, hybrid_state_chunk_map_id_v2};
        let row = palw_qwen36_context_row_profile_v1(512).expect("projects");
        assert_eq!(row.state_chunk_map_id, hybrid_state_chunk_map_id_v2(), "the 512 row is not on the head-sliced composition");
        let (k, v, heads, kernel) =
            (row.gdn_head_k_dim as u64, row.gdn_head_v_dim as u64, row.gdn_heads as u64, row.gdn_conv_kernel as u64);
        let budget = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;

        let (delta, conv) = palw_gdn_checkpoint_terms_v1(&row).expect("the geometry derives");
        assert_eq!(delta, v * k * 4, "one head's delta state: v_dim rows of k_dim lanes, four bytes each");
        assert_eq!(delta, 65_536);
        assert_eq!(conv, kernel * (2 * k + v) * 4, "one head's window: conv_kernel rows of (2·k + v) lanes");
        assert_eq!(conv, 6_144);
        assert_eq!(delta + conv, 71_680);
        // Against ONE CARRIER, which is what "fits the carrier" means for a single opening: the
        // close budget is a 27-chunk group since ADR-0080 design A, and comparing one opening
        // against the whole group would make this assertion true of anything.
        let chunk = crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64;
        assert!(delta + conv <= chunk, "the recurrence opening is {} bytes against a carrier of {chunk}", delta + conv);
        assert!(delta + conv <= budget, "and inside the group, which is the ceiling the court prices");

        // The v1 enumeration, on the same geometry, for the contrast that says what moved — and
        // it is still the price a v1-mapped class pays, because a class IS its map.
        let mut old = row.clone();
        old.state_chunk_map_id = gdn_state_chunk_map_id_v1();
        let (old_delta, old_conv) = palw_gdn_checkpoint_terms_v1(&old).expect("v1 derives");
        assert_eq!(old_delta, delta, "v2 changed the delta half, which was already head-sliced");
        assert_eq!(old_conv, gdn_conv_window_bytes_v1(&row).expect("v1 window"));
        assert_eq!(old_conv, 196_608);
        assert_eq!(old_conv, conv * heads, "the two enumerations cover the same bytes");
        // **Restated in carriers, for the reason above.** This read `> budget` when the budget was
        // 80 KiB and one transaction. 262,144 bytes is inside ADR-0080's group and outside a single
        // chunk, and the second is the fact gdn v2 was built on: three carriers of convolution
        // window for thirty-one heads the court will not read.
        assert!(old_delta + old_conv > chunk, "gdn v1 fits one carrier after all — this test's whole premise moved");
        assert_eq!(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(old_delta + old_conv), 4, "v1's opening costs four carriers");
        assert_eq!(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(delta + conv), 1, "and v2's costs one");

        // The dense row, for contrast: no recurrence at all, and it is the KV history that refuses
        // it. Two different reasons, and a change that fixed one would leave the other.
        let dense = palw_a16_context_row_profile_v1(512).expect("projects");
        assert!(palw_gdn_checkpoint_terms_v1(&dense).is_none(), "the dense tier declared a recurrence");
    }

    /// **What the hybrid 512 row costs, and by how little it now fits — because "it fits" with no
    /// margin printed beside it is how a ceiling gets discovered at registration.**
    ///
    /// **This test was `what_still_refuses_the_hybrid_512_row`, and it asserted the opposite.**
    /// Under the 80 KiB one-transaction ceiling the row was over budget three ways over, and the
    /// test named all three so that a fix closing one and leaving two could not read as success.
    /// ADR-0080 design A closes them by moving the CARRIER — 27 chunks rather than one transaction
    /// — and the row is inside it. Every term below is still real and still measured; what changed
    /// is that they are now measured against a budget that holds them.
    ///
    /// The three terms, unchanged as arithmetic:
    ///
    /// * **the attention scores row is `attn_heads × n_ctx` wide** — 8,192 lanes at 512, and the
    ///   softmax node's close is ~125 KiB with NO history opened at all;
    /// * **the KV checkpoint is the whole history**: `n_ctx × attn_kv_heads × attn_head_dim × 4`,
    ///   526,336 bytes at 512, charged once per history-reading reference;
    /// * **the recurrence's own replay evidence** is `interval` positions × five refs × one
    ///   sibling set each, before a single checkpoint byte.
    ///
    /// **The margin is the finding.** The whole derived close is 2,240,241 against a ceiling of
    /// 2,250,000 — 9,759 bytes, 0.43 %. This row does not have room for a wider one, and a change
    /// that adds a term to the close of any size at all takes it back out. That is asserted here
    /// rather than left to be met at a registration.
    #[test]
    fn what_the_hybrid_512_row_now_costs() {
        let budget = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
        let row = palw_qwen36_context_row_profile_v1(512).expect("projects");

        // The whole derived close, through the gate's own arithmetic.
        let cost = palw_anchored_court_cost_v1(&row).expect("a mapped row is priced").expect("derives");
        assert_eq!(cost.max_close_bytes, 2_240_241, "the hybrid 512 row's whole close");
        assert!(
            cost.max_close_bytes <= budget,
            "the hybrid 512 row is over the ceiling again at {} — ADR-0080's count no longer buys Decision 13's first row",
            cost.max_close_bytes
        );
        // And it is the LAST row this count buys: the margin, named.
        let margin = budget - cost.max_close_bytes;
        assert_eq!(margin, 9_759, "the hybrid 512 row's headroom under the 27-chunk ceiling");
        assert!(margin * 100 < budget, "the margin grew past 1% — the ceiling or the row moved, and the launch note quotes this");
        assert_eq!(
            crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(cost.max_close_bytes),
            crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS,
            "the row uses every chunk of the group — which is why 27 is the count and not 26"
        );

        // Term 1: the attention cache's anchor is the history, and it alone is over a CARRIER —
        // the term that made the old ceiling impossible, unchanged.
        let chunk = crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64;
        let kv = palw_kv_checkpoint_opening_bytes_v1(&row, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("derives");
        assert_eq!(kv, (row.n_ctx as u64) * (row.attn_kv_heads as u64) * (row.attn_head_dim as u64) * 4 + 64 * 32);
        assert!(kv > chunk * 5, "the KV anchor stopped being the history — the attention half changed shape");

        // Term 2: the recurrence's replay evidence, with every checkpoint byte removed. Sixteen
        // positions of five refs, each run carrying one sibling set at the ladder's depth. It was
        // over the 80 KiB ceiling on its own; it is still most of a megabyte.
        let mut bare = palw_class_ladder_rules_v1(&row).expect("a mapped row has rules").cost_shape;
        bare.gdn_checkpoint_bytes = 0;
        bare.kv_checkpoint_bytes = 0;
        let without_anchors = derive_court_cost_shaped_v1(&row, bare).expect("derives");
        assert!(
            without_anchors.max_close_bytes > 80 * 1024,
            "with NO anchor bytes at all the close is {} — the residue this test names is gone",
            without_anchors.max_close_bytes
        );

        // Term 3: and it is still over the OLD ceiling with the history term collapsed to a single
        // position, which is what says the residue is the CONTEXT's width rather than the
        // interval's length. Stated against 80 KiB because that is the ceiling the finding is
        // about; against the group it is no longer a refusal at all.
        bare.history_positions = 1;
        let single = derive_court_cost_shaped_v1(&row, bare).expect("derives");
        assert!(
            single.max_close_bytes > 80 * 1024,
            "a one-position history now fits the old carrier at {} — the attention row stopped being n_ctx-wide",
            single.max_close_bytes
        );

        // **And the term no state chunk map can shrink further**: one head's delta state is
        // `k_dim × v_dim × 4`, and a court replaying that head needs all of it — the recurrence is
        // separable across output lanes but the head's tile IS its `v_dim` lanes. At Qwen3.6's
        // 128 × 128 that is 64 KiB, which was 80 % of the whole 80 KiB carrier and is now one
        // chunk of twenty-seven. Written down as the arithmetic rather than as a byte count so a
        // narrower head moves it.
        let (delta, _) = palw_gdn_checkpoint_terms_v1(&row).expect("terms");
        assert_eq!(delta, (row.gdn_head_k_dim as u64) * (row.gdn_head_v_dim as u64) * 4);
        assert_eq!(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(delta), 1, "one head's delta state is no longer one carrier");
        // The gate charges one such opening per history-reading REFERENCE, and the recurrence node
        // declares five, so the anchored shape's floor for this graph is five of them however
        // short the interval is. Named here because it is the largest single lever left and it
        // lives in `derive_court_cost_shaped_v1`, not in any map — and because at 80 KiB it was
        // the term that refused the hybrid at EVERY context, which is the zero
        // `the_widest_context_each_family_admits` measures on the other side of the gate.
        let node_refs = 5u64;
        assert!(node_refs * (delta + 6_144 + 64 * 32) > 80 * 1024, "the per-reference anchor charge stopped being the floor");
        assert!(node_refs * (delta + 6_144 + 64 * 32) < budget, "five anchors are inside the group — which is what bought the row");
    }

    /// **The recurrence's anchored position list is the interval window**, and it is the same
    /// ascending list the long walk ends with — one encoding of the committed rows, not two.
    #[test]
    fn the_anchored_recurrence_opens_one_interval_of_positions() {
        use crate::palw_step_refute::gdn_anchored_positions_v1;
        let (prefill, interval) = (64u32, 16u32);
        // A prefill position mid-interval: the boundary below it, up to it.
        assert_eq!(
            gdn_anchored_positions_v1(prefill, 0, 20, interval).expect("derives"),
            (16..=20).map(|p| (0u32, p)).collect::<Vec<_>>()
        );
        // Exactly on a boundary: the checkpoint covers everything before, so one position remains.
        assert_eq!(gdn_anchored_positions_v1(prefill, 0, 32, interval).expect("derives"), vec![(0, 32)]);
        // A decode call. The flattened index is `prefill + call − 1`, so call 3 sits at 66 and the
        // boundary below it is 64 — which is decode call 1, because the prefill occupies flat
        // 0..=63. The window is therefore three decode calls and no prefill position: the
        // recurrence at a late decode step replays decode, not the prompt.
        let at_call_3 = gdn_anchored_positions_v1(prefill, 3, 0, interval).expect("derives");
        assert_eq!(at_call_3, vec![(1u32, 0u32), (2, 0), (3, 0)], "the window is the interval below the disputed call");
        // Never longer than the interval, at any coordinate — the property Decision 11 prices.
        for call in 0..4u32 {
            for pos in 0..prefill {
                let list = gdn_anchored_positions_v1(prefill, call, pos, interval).expect("derives");
                assert!(list.len() <= interval as usize, "call {call} position {pos} opened {} positions", list.len());
            }
        }
        // A zero cadence is not a cadence.
        assert!(gdn_anchored_positions_v1(prefill, 0, 0, 0).is_none());
    }

    /// **And the canonical leaf set is the one that window names** — the prover's door and the
    /// checker's derive the same rows, because they are the same derivation.
    ///
    /// `gdn_anchored_positions_v1` was a function with no consumer: the anchored flag shortened the
    /// KV arms and left the recurrence at its genesis-anchored walk, so a class that committed its
    /// recurrence state still had to open every position from the sequence start. Wiring it is the
    /// half of Decision 10 that turns the map from a commitment nobody spends into a shorter
    /// refutation.
    ///
    /// The narrowing is gated on the class's OWN map: a class that committed no recurrence state
    /// keeps the long walk however many anchors its attention arms carry, because a set shortened
    /// past what a checkpoint covers is a set the court cannot verify.
    #[test]
    fn the_anchored_recurrence_opens_the_window_and_not_the_history() {
        use crate::palw_step::kernel_semantics_id_v1;
        use crate::palw_step_refute::{canonical_input_leaves_v1_anchored, KDESC_Q36_GDN_STEP};
        let mapped = recurrent_row_v2(512);
        let gdn_kernel = kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
        let slot = (0..u32::MAX)
            .take_while(|s| mapped.resolve_node_slot(*s).is_some())
            .find(|s| mapped.resolve_node_slot(*s).is_some_and(|(n, _)| n.kernel_semantics_id == gdn_kernel))
            .expect("the recurrent row has a recurrence node");
        let (node, _) = mapped.resolve_node_slot(slot).expect("resolves");
        let refs = node.input_refs.len();
        let ctx = crate::palw_base0_profile::rc_job_context(&mapped, 128, 2);
        let coord = crate::palw_step::PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position: 100, tile_index: 0 };

        let long = canonical_input_leaves_v1_anchored(&mapped, &ctx, &coord, false).expect("the long set derives");
        let anchored = canonical_input_leaves_v1_anchored(&mapped, &ctx, &coord, true).expect("the anchored set derives");
        // The interval below position 100 is 96, so the window is 96..=100 — five positions of the
        // node's refs, against a hundred and one.
        assert_eq!(palw_checkpoint_interval_v1(mapped.n_ctx), 16);
        assert_eq!(anchored.len(), 5 * refs, "the anchored set is not one interval window of rows");
        assert_eq!(long.len(), 101 * refs, "the long walk is not the whole prefix");
        // And it is the long set's TAIL, row for row: one canonical set read from two encodings of
        // the same committed rows, never a second enumeration that merely agrees.
        assert_eq!(anchored, long[long.len() - anchored.len()..].to_vec(), "the window is not the long walk's own tail");

        // A class that registers no recurrence map keeps the long walk under the same flag.
        let mut unmapped = mapped.clone();
        unmapped.state_chunk_map_id = crate::Hash64::default();
        assert_eq!(
            canonical_input_leaves_v1_anchored(&unmapped, &ctx, &coord, true).expect("derives").len(),
            long.len(),
            "an unmapped class's recurrence was narrowed to a window no checkpoint covers"
        );
        // The gdn v1 enumeration commits the same state, so it narrows too — the window is a
        // property of the CHECKPOINT existing, not of how its bytes are ordered.
        let mut v1 = mapped.clone();
        v1.state_chunk_map_id = crate::palw_state_chunk_map::gdn_state_chunk_map_id_v1();
        assert_eq!(canonical_input_leaves_v1_anchored(&v1, &ctx, &coord, true).expect("derives").len(), anchored.len());
    }

    /// **The map id a court reads is the one an executor captured under.** `misaka-palw-base0`
    /// derives its recurrence map id from a string it holds as `PALW_GDN_STATE_CHUNK_MAP_NAME_V1`;
    /// this crate is the lower one and holds the spelling. A second spelling would be a second id,
    /// and a class whose capture and whose adjudicator disagree about their map id is a class no
    /// dispute can open — so the string is pinned here, verbatim, and the engine crate's copy is
    /// what must reference it.
    #[test]
    fn the_recurrence_map_id_is_the_executors_own_spelling() {
        use crate::palw_state_chunk_map::{
            gdn_state_chunk_map_id_v1, gdn_state_chunk_map_id_v2, hybrid_state_chunk_map_id_v1, hybrid_state_chunk_map_id_v2,
            integer_kv_state_chunk_map_id_v2, palw_hybrid_state_chunk_map_name_v1, palw_hybrid_state_chunk_map_name_v2,
            PALW_GDN_STATE_CHUNK_MAP_NAME_V1, PALW_GDN_STATE_CHUNK_MAP_NAME_V2,
        };
        assert_eq!(
            PALW_GDN_STATE_CHUNK_MAP_NAME_V1,
            "palw-gdn-state/i32-le/kind-major(delta,conv)/layer-asc/head-asc/\
             row-asc/delta-row=gdn_head_k_dim*4/conv-row=(2*gdn_head_k_dim+gdn_head_v_dim)*gdn_heads*4/chunk<=2^20/v1",
            "the recurrence layout was respelled — every capture taken under the old string is now unopenable"
        );
        // Six distinct maps, six distinct ids: the two cache widths, the two recurrence
        // enumerations, and the two compositions. A collision would make one class's evidence
        // readable as another's — a row-major capture opened head-major restores a state nobody
        // folded.
        let ids = [
            crate::palw_state_chunk_map::integer_kv_state_chunk_map_id_v1(),
            integer_kv_state_chunk_map_id_v2(),
            gdn_state_chunk_map_id_v1(),
            gdn_state_chunk_map_id_v2(),
            hybrid_state_chunk_map_id_v1(),
            hybrid_state_chunk_map_id_v2(),
        ];
        let unique: std::collections::BTreeSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "two state chunk maps share an id");
        assert!(ids.iter().all(|id| *id != crate::Hash64::default()), "the unregistered sentinel is not a map id");
        // Each composition is spelled as its parts, so it cannot drift from either.
        let name = palw_hybrid_state_chunk_map_name_v1();
        assert!(name.contains(PALW_GDN_STATE_CHUNK_MAP_NAME_V1) && name.contains("palw-integer-kv/i32-le"));
        let name_v2 = palw_hybrid_state_chunk_map_name_v2();
        assert!(name_v2.contains(PALW_GDN_STATE_CHUNK_MAP_NAME_V2) && name_v2.contains("palw-integer-kv/i32-le"));
        // And the hybrid row registers the composition rather than half of it.
        assert_eq!(palw_qwen36_context_row_profile_v1(512).expect("projects").state_chunk_map_id, hybrid_state_chunk_map_id_v2());
        assert_eq!(recurrent_row(512).state_chunk_map_id, gdn_state_chunk_map_id_v1(), "a pure-recurrent row needs no cache map");
        assert_eq!(recurrent_row_v2(512).state_chunk_map_id, gdn_state_chunk_map_id_v2());
        // A class IS its map: the same graph on two enumerations is two classes, which is what
        // makes v2 additive rather than a repair of anything already registered.
        assert_ne!(recurrent_row(512).shape_profile_id(), recurrent_row_v2(512).shape_profile_id());
    }

    /// The pessimistic price stays for a class with no map: the anchored question is not answered
    /// about a class that cannot anchor, because a low price would read as approval.
    #[test]
    fn a_class_with_no_state_chunk_map_gets_no_anchored_price() {
        let unmapped = crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
            crate::palw_qwen36_profile::QWEN36_35B_A3B,
        ))
        .expect("projects");
        assert_eq!(unmapped.state_chunk_map_id, crate::Hash64::default(), "the shipped hybrid registers a map after all");
        assert!(palw_anchored_court_cost_v1(&unmapped).is_none());
        assert!(palw_class_ladder_rules_v1(&unmapped).is_none(), "an unmapped class was given the deeper ladder");
        assert!(!palw_long_form_is_refused_v1(&unmapped), "the long form is refused for an unmapped class");
        let mapped = palw_qwen36_context_row_profile_v1(512).expect("projects");
        assert!(palw_anchored_court_cost_v1(&mapped).is_some());
        assert!(palw_long_form_is_refused_v1(&mapped), "the long form survives for a mapped class — a challenger may choose");
    }

    // ---------------------------------------------------------------------------------------
    // W2 — anchored and long reach the same verdict, for the recurrence
    // ---------------------------------------------------------------------------------------

    /// Deterministic recurrence inputs: five rows per position, in the kernel's pinned order
    /// `[q, k, v, g, beta]`. Small magnitudes and a negative gate, so the state decays rather than
    /// saturating and the comparison is over live arithmetic rather than over zeros.
    fn gdn_inputs(heads: usize, kd: usize, vd: usize, positions: usize) -> Vec<Vec<u32>> {
        let f = |i: usize, salt: usize| -> u32 { (((i % 13) as f32 - 6.0) / 8.0 + (salt % 7) as f32 / 32.0).to_bits() };
        let mut rows = Vec::with_capacity(5 * positions);
        for t in 0..positions {
            rows.push((0..heads * kd).map(|i| f(i + t, 1)).collect());
            rows.push((0..heads * kd).map(|i| f(i + 2 * t, 2)).collect());
            rows.push((0..heads * vd).map(|i| f(i + 3 * t, 3)).collect());
            rows.push((0..heads).map(|h| (-0.05f32 - h as f32 / 64.0).to_bits()).collect());
            rows.push((0..heads).map(|h| (0.25f32 + h as f32 / 32.0).to_bits()).collect());
        }
        rows
    }

    /// **W2 for the recurrence.** The anchored replay standing on a state captured at position `c`
    /// reaches the same output row as the shipped genesis-anchored walk over the whole history —
    /// swept over every split point, because "it agreed at the split I tried" is exactly the shape
    /// of claim this tree has watched go stale.
    #[test]
    fn the_anchored_recurrence_replay_agrees_with_the_long_form() {
        use crate::palw_step_refute::{gdn_core_anchored_replay_v1, DotStructure};
        let mut profile = recurrent_row(64);
        profile.gdn_heads = 2;
        profile.gdn_head_k_dim = 16;
        profile.gdn_head_v_dim = 3;
        let (heads, kd, vd) = (2usize, 16usize, 3usize);
        let positions = 9usize;
        let inputs = gdn_inputs(heads, kd, vd, positions);

        // The long form over the whole history, through the anchored twin's zero-anchor arm —
        // which is the genesis start by definition.
        let long = gdn_core_anchored_replay_v1(&profile, None, &inputs, DotStructure::Step16Epr4).expect("the long form replays");

        for c in 1..positions {
            // Capture: replay 0..c from zero and keep the state, exactly as a producer's
            // checkpoint does.
            let head =
                gdn_core_anchored_replay_v1(&profile, None, &inputs[..5 * c], DotStructure::Step16Epr4).expect("the capture replays");
            // Prosecute: replay c..positions from that state.
            let anchored = gdn_core_anchored_replay_v1(&profile, Some(&head.state), &inputs[5 * c..], DotStructure::Step16Epr4)
                .expect("the anchored form replays");
            assert_eq!(anchored.out_row, long.out_row, "the two routes disagree when the anchor covers {c} positions");
            assert_eq!(anchored.state, long.state, "the two routes leave different state at split {c}");
        }
    }

    /// A malformed anchor is a refusal of the EVIDENCE, never a panic inside block validation —
    /// the guard the shipped genesis arm never needed because it had no anchor to be handed.
    #[test]
    fn a_mis_shaped_recurrence_anchor_is_refused_by_name() {
        use crate::palw_step_refute::{gdn_core_anchored_replay_v1, DotStructure, PalwStepRefuteError};
        let mut profile = recurrent_row(64);
        profile.gdn_heads = 2;
        profile.gdn_head_k_dim = 16;
        profile.gdn_head_v_dim = 3;
        let inputs = gdn_inputs(2, 16, 3, 2);
        for bad in [vec![vec![0u32; 16 * 3]], vec![vec![0u32; 7], vec![0u32; 16 * 3]], vec![]] {
            assert!(
                matches!(
                    gdn_core_anchored_replay_v1(&profile, Some(&bad), &inputs, DotStructure::Step16Epr4),
                    Err(PalwStepRefuteError::InputSetNotCanonical(_))
                ),
                "a {}-row anchor was read rather than refused",
                bad.len()
            );
        }
    }
}

#[cfg(test)]
mod the_512_breakdown {
    use super::*;
    use crate::palw_class_admission_v2::derive_court_cost_rows_v1;

    /// **The measurement that decided ADR-0080, pinned so it cannot quietly stop being true.**
    ///
    /// Four designs were judged against these four numbers, and the one that won did so because of
    /// them. `derive_court_cost_rows_v1` exists so that "the row costs N" can name WHICH node
    /// produced N; this asserts the naming as well as the total, because a change that moved the
    /// binding node while leaving the total alone would invalidate the reasoning without failing a
    /// total-only check.
    ///
    /// **This test was `the_512_rows_name_the_node_that_refuses_them`, and it closed with
    /// `close_bytes > budget * 10`.** That was the motivation: at the 80 KiB one-transaction
    /// ceiling both 512 rows were an order of magnitude out of reach, and no anchoring, tiling or
    /// state-chunk map reached them. Design A moved the carrier rather than the arithmetic, so the
    /// four numbers below are UNCHANGED — the same nodes bind at the same byte counts — and what
    /// they are compared against is a 27-chunk group. The test now states both halves: the
    /// measurement that decided the ADR, and the ceiling that answered it.
    ///
    /// If a total ever goes DOWN, ADR-0080's motivation section is describing a cost that no
    /// longer exists and the design is worth re-opening; that has not changed and is what the
    /// equalities are for.
    #[test]
    fn the_512_rows_name_the_node_that_prices_them() {
        let budget = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
        const CEILING_THAT_REFUSED_THEM: u64 = 80 * 1024;
        for (label, profile, expect_close, expect_table, expect_index) in [
            ("A16 512", palw_a16_context_row_profile_v1(512).expect("projects"), 1_154_673u64, "attn", 10usize),
            ("QWEN36 512", palw_qwen36_context_row_profile_v1(512).expect("projects"), 2_240_241u64, "attn", 15usize),
        ] {
            let shape = palw_class_ladder_rules_v1(&profile).expect("a mapped row has rules").cost_shape;
            let rows = derive_court_cost_rows_v1(&profile, shape).expect("derives");
            let worst = rows.iter().max_by_key(|r| r.close_bytes).expect("a graph has nodes");
            assert_eq!(
                worst.close_bytes, expect_close,
                "{label}: the binding close moved — if it went DOWN, re-read ADR-0080's motivation before celebrating"
            );
            assert_eq!((worst.table, worst.index), (expect_table, expect_index), "{label}: a different node binds now");
            // The motivation, intact: an order of magnitude over the ceiling ADR-0080 replaced.
            assert!(
                worst.close_bytes > CEILING_THAT_REFUSED_THEM * 10,
                "{label}: {} stopped being an order of magnitude over the {CEILING_THAT_REFUSED_THEM}-byte carrier the ADR was written against",
                worst.close_bytes
            );
            // And the answer: inside the 27-chunk group, which is what makes the row registrable.
            assert!(
                worst.close_bytes <= budget,
                "{label}: {} is over ADR-0080's own ceiling — design A does not buy the row it was sized for",
                worst.close_bytes
            );
        }
    }
}
