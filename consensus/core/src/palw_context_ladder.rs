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
//! **And one row that is ADR-0080's, not ADR-0077's:** a court close no longer fits one carrier,
//! so the blocks a mover spends ASSEMBLING one are blocks of the court window that carry no move.
//! [`palw_close_assembly_daa_v1`] is that term, counted for both sides, subtracted before a clock
//! is derived and added before a window is checked. It is the difference between a court whose
//! deadline closes on the party that is answering and one whose deadline closes on the party that
//! is assembling — audit M2-24's shape, restated for a split close.
//!
//! **And it is a RULESET term, so every derivation here takes it as a parameter.** `max_close_chunks`
//! is a `PalwCourtParamsV2` field — one value per network — and the two rulesets that ship carry
//! different ones: testnet-11 pays for a 27-carrier close and the devnet lattice for a one-carrier
//! close. A derivation that read the DEFAULT instead would hand the minutes lattice the hours
//! lattice's reserve, 216 DAA against a 300-DAA window, and the clock it derived would be one no
//! drill could run.
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
use crate::palw_class_admission_v2::{
    PalwClassAdmissionError, PalwCourtCostRowV1, PalwCourtCostShapeV1, PalwCourtCostV1, derive_court_cost_rows_v1,
    derive_court_cost_shaped_v1,
};
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
// ADR-0080 W4 — assembling a split close spends the mover's own clock
// =================================================================================================

/// **The carriers one court close may be split across** (ADR-0080).
///
/// A close is a transaction and the carrier is `DEFAULT_MAX_CLOSE_BYTES` — 80 KiB, the mempool's
/// standard-transaction mass mirrored. ADR-0080 §1 measures a close an order of magnitude past it
/// (`the_512_breakdown` pins 2,240,241 bytes on the hybrid row against an 81,920-byte carrier), so
/// a close no longer rides one transaction: it is assembled over several blocks and the mover
/// spends those blocks out of its own turn.
///
/// **The widest step binding a shipped class carries, in serialized bytes** — measured.
///
/// A close carries a `PalwStepBindingV2`, which carries the whole `PalwShapeProfileV3`, and
/// `check_close_cost_v2` charges for NONE of it: the cost rule prices the close's payload against
/// `max_close_bytes` and the binding rides free. So the bytes a CARRIER must hold are the priced
/// ceiling plus this, and a derivation that forgets it under-counts the carrier by fourteen
/// kilobytes.
///
/// 13,996 is the hybrid `Qwen3.6-35B-A3B` row, widest of the three shipped classes (A16 5,631,
/// BASE-0 6,346), measured by serializing an EMPTY close on each — ADR-0080 W13, branch
/// `palw-adr0080-w13-carriage` (`429c8157`), `the_binding_is_the_untolled_part_of_a_carrier`.
/// A measurement carried across a crate boundary, so it is named here rather than inlined.
pub const PALW_WIDEST_STEP_BINDING_BYTES_V1: u64 = 13_996;

/// **How many carriers a legal close can need — DERIVED from the ceilings, never pinned.**
///
/// An earlier draft of this module pinned `27` here, and 27 was impossible: the chunk rules cap a
/// group at `PALW_OBJECT_CHUNK_MAX_COUNT` = 8 parts, enforced twice — the builder refuses at
/// `palw_state_v2.rs:2917` with `ObjectTooLargeToChunk`, and the transition refuses again at 7362.
/// A 27-part object cannot exist on any preset, so a reserve sized for one bought nothing and cost
/// the devnet its shipped clock. **The lesson is the shape, not the number**: a quantity with no
/// derivation is a quantity nobody can check, and it compiled, went green, and moved a fingerprint.
///
/// The derivation is the two ceilings over the carrier: `⌈(max_close_bytes + widest binding) /
/// PALW_OBJECT_CHUNK_MAX_BYTES⌉`, saturated at the chunk cap. At the shipped ceilings that is
/// `⌈(81,920 + 13,996) / 100,000⌉ = 1` — every admissible close is ONE carrier, with 4,084 bytes
/// to spare.
///
/// **This is a floor from the transport that exists TODAY, and it is NOT the rule.** The rule is a
/// ruleset quantity — `PalwCourtParamsV2::max_close_chunks`, ADR-0080 W3 — because the worst case a
/// court session may be asked to assemble is what the ruleset admits, not what the rows registered
/// this morning happen to need. Deriving it from the live ceilings makes it 1 now and silently
/// stale the day a wide row registers, which is the same shape as the constant it replaced: a
/// number that was true when it was written.
///
/// **W3 has landed that field, and this now reads it.** `PalwCourtParamsV2::max_close_chunks` is the
/// ruleset's own answer to "what may a session be asked to assemble", and it is what the reserve is
/// derived from. The previous derivation — from `max_close_bytes` and the widest binding, i.e. what
/// a close needs at TODAY's registered rows — is gone, because it would have under-reserved every
/// window whose ruleset admits more than today's rows need, and it would have done so silently.
///
/// One thing that reads like a contradiction and is not: `PALW_OBJECT_CHUNK_MAX_COUNT` = 8 caps
/// `ObjectChunk`, the CERTIFICATION lane's transport, and it does not bind the court. ADR-0080
/// design A gives a close's group its own table at `(session_id, side)` — `court_close_groups`,
/// built by W5 — so a chunk count of 27 is not impossible, it merely has no transport until W5
/// lands. The reserve is sized for the rule the ruleset states, not for the transport that exists
/// this morning; that is the whole lesson of the number this replaced.
///
/// **What was owed here came due at once, and this is what it cost.** An earlier form of this
/// module let the two derivations below read this CONSTANT — the ruleset DEFAULT — on the argument
/// that every shipped preset carried it. That was true for one commit. The default is 27 carriers,
/// which is 216 DAA of reserve and a 27-DAA floor under every honest move; on testnet-11's frozen
/// 120 s cadence that is a 54-minute move inside a 100-hour window, which is correct for a real
/// chain and fatal to a drill. Funding it out of the devnet's 300-DAA window would need
/// `window_court ≥ 1998` — 66 hours — and the devnet lattice exists precisely to be crossable in a
/// session.
///
/// The repair is not a wider devnet window. It is that `max_close_chunks` is a RULESET quantity and
/// the devnet's is not the RC's: devnet registers no row whose close needs a second carrier, so its
/// bundle sets `max_close_bytes` at one carrier's worth and its `max_close_chunks` derives to 1,
/// its reserve to 8, and its shipped move clock survives untouched.
///
/// So this constant is the RC's reading and nothing derives from it implicitly. Both derivations
/// below take `max_close_chunks` as a parameter and are given the value the network's own ruleset
/// carries — [`crate::palw_fp_devnet_v3::PalwLatticeWindowsV1::max_close_chunks`] at assembly time,
/// `PalwCourtParamsV2::max_close_chunks` once a bundle exists.
pub const PALW_COURT_MAX_CLOSE_CHUNKS_V1: u64 = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS;

pub const PALW_COURT_CHUNKED_CLOSES_PER_SIDE_V1: u64 = 4;

/// **The DAA a court window must reserve for CLOSE ASSEMBLY, counted for both sides** (ADR-0080).
///
/// `2 × PALW_COURT_CHUNKED_CLOSES_PER_SIDE_V1 × max_close_chunks` — 216 DAA on a ruleset that
/// admits a 27-carrier close and 8 on one that admits a single carrier. It is subtracted from the
/// window before a move clock is derived ([`palw_court_turn_deadline_v1`]) and added to the worst
/// case before the window is checked ([`palw_ladder_fits_window_court_v1`]), so a ladder that fits
/// is a ladder that fits *with* the blocks its closes occupy.
///
/// **The count is the caller's ruleset's, never this module's default.** Two shipped networks carry
/// two different answers, so a reserve derived from the default would be exact for the network that
/// happens to carry it and silently wrong for every other — wrong in the direction that spends a
/// window nobody has, and therefore in the direction that shortens a clock until it convicts.
///
/// **Why it is a term and not a wider clock.** A longer turn deadline pays the mover and the
/// responder equally, which is exactly wrong: assembly is time the MOVER spends before its move
/// exists, and the party waiting for it has its own clock running. Reserving the blocks at the
/// window keeps `moves × deadline` meaning what it says.
pub const fn palw_close_assembly_daa_v1(max_close_chunks: u64) -> u64 {
    2u64.saturating_mul(PALW_COURT_CHUNKED_CLOSES_PER_SIDE_V1).saturating_mul(max_close_chunks)
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
    if daa == 0 { 1 } else { daa }
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
/// **And the window pays for close ASSEMBLY before it pays for moves** (ADR-0080 W4). A close may
/// no longer fit one carrier, so the blocks a mover spends assembling one are blocks of the window
/// that no move gets: [`palw_close_assembly_daa_v1`] comes off the top, for BOTH sides, and the
/// clock is derived from what is left. Without the term the deadline closes on the challenger's
/// side while the mover is still assembling — audit M2-24's shape, a court that convicts an honest
/// party by clock.
///
/// **The reserve is the RULESET's, which is why it is an argument.** `max_close_chunks` is a court
/// param, so two networks give this function two different reserves and it returns two different
/// clocks from one derivation — which is the whole content of the parameter:
///
/// | ruleset | `window_court` | chunks | reserve | moves at 2^32 | derived clock |
/// |---|---|---|---|---|---|
/// | testnet-11 (RC) | 3,000 | 27 | 216 | 66 | `(3000 − 216 − 1) / 66` = **42** |
/// | devnet | 300 | 1 | 8 | 66 | `(300 − 8 − 1) / 66` = **4** |
///
/// At the SHIPPED 2^22 ladder the same expression returns the RC's shipped 60 (`(3000 − 216 − 1) /
/// 46`), so no constant this build boots on is a choice: each is what its own window, its own
/// ladder and its own chunk count produce.
///
/// **What the parameter cost, stated because it is the finding.** Read from the DEFAULT — 27, the
/// RC's — the devnet's reserve is 216, its 300-DAA window holds the ladder at no clock at all
/// (`66 × 4 + 216 = 480`, and even `46 × 4 + 216 = 400`), and funding it would need `window_court ≥
/// 1998`: 66 hours, which is not a drill. A previous commit widened the devnet window to 600 and its
/// clock to 5 to pay for exactly that, and it still did not clear the 27-carrier move floor. The
/// widening is reverted here, because the reserve devnet was being charged for is one its own
/// ruleset never admits.
pub const fn palw_court_turn_deadline_v1(
    window_court: u64,
    max_step_leaves: u64,
    terminal_moves: u32,
    max_close_chunks: u64,
) -> Option<u64> {
    let moves = palw_court_move_count_v1(max_step_leaves, terminal_moves);
    if moves == 0 || window_court == 0 {
        return None;
    }
    let reserve = palw_close_assembly_daa_v1(max_close_chunks);
    // A window that cannot even hold the assembly reserve holds no dispute: refused, not
    // saturated, for the reason `worst_case_duration_daa` gives about its own overflow.
    if window_court <= reserve {
        return None;
    }
    let deadline = (window_court - reserve - 1) / moves;
    if deadline == 0 { None } else { Some(deadline) }
}

/// **W4, as a predicate:**
/// `(2 · ⌈log₂ leaves⌉ + terminal) · turn_deadline + assembly_reserve < window_court`.
///
/// Strict, because the backstop closes on the challenger's side: a prosecution that lands exactly
/// on the window loses a dispute it was playing correctly.
///
/// The reserve is ADR-0080's ([`palw_close_assembly_daa_v1`]): a split close occupies blocks that
/// carry no move, and a window checked without them is a window that fits a prosecution nobody can
/// actually assemble inside it. Counted for both sides, which is why the term is a property of the
/// WINDOW and not of either party's clock — and taken from the ruleset being checked, because a
/// window is only ever asked to hold ITS OWN network's closes.
pub const fn palw_ladder_fits_window_court_v1(
    window_court: u64,
    max_step_leaves: u64,
    terminal_moves: u32,
    turn_deadline: u64,
    max_close_chunks: u64,
) -> bool {
    let moves = palw_court_move_count_v1(max_step_leaves, terminal_moves);
    match moves.checked_mul(turn_deadline) {
        Some(worst) => match worst.checked_add(palw_close_assembly_daa_v1(max_close_chunks)) {
            Some(total) => total < window_court,
            None => false,
        },
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
/// **The window set carries the chunk count the derivation needs**
/// ([`PalwLatticeWindowsV1::max_close_chunks`]), so a lattice is re-derived against ITS OWN court's
/// close ceiling rather than against a default. That is what makes this one function able to answer
/// for both shipped networks: the same expression, two rulesets, two clocks.
///
/// `None` when the base window cannot hold the ladder at any clock — see
/// [`palw_court_turn_deadline_v1`].
pub fn palw_context_ladder_windows_v1(base: PalwLatticeWindowsV1, max_step_leaves: u64) -> Option<PalwLatticeWindowsV1> {
    let deadline =
        palw_court_turn_deadline_v1(base.window_court, max_step_leaves, PALW_CONTEXT_LADDER_TERMINAL_MOVES, base.max_close_chunks())?;
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
    if derived == 0 { 1 } else { derived }
}

// =================================================================================================
// ADR-0082 Decision 4 (amended) — a tiled-map class commits an attention checkpoint at EVERY
// position, so every dissection bottom fits one carrier
// =================================================================================================

/// **What one checkpoint of the ATTENTION cache is taken AFTER**, as the class's own map decides
/// it (ADR-0082 Decision 4, amended).
///
/// Two units, and the class's registered `state_chunk_map_id` is what chooses between them —
/// nothing is declared and nothing is inferred from the graph:
///
/// * [`Self::PerDecodeCall`]: the shipped cadence. A checkpoint after every
///   `checkpoint_interval` DECODE CALLS, and none over the prefill at all
///   (`decode_calls / interval` of them, `palw_step_leg`'s `CheckpointCountNotCanonical` rule).
/// * [`Self::PerPosition`]: one checkpoint after every POSITION of the cache, prefill positions
///   included.
///
/// # Why the second one exists
///
/// Decision 2's dissection bottoms out at one history TILE, and the bottom reaches that tile's K
/// and V rows by one of two routes (`PalwAttnTileEvidenceV1`): one chunk opening out of the
/// checkpoint at or before the tile, or one committed cache-write leaf per row. Measured on the
/// real objects, the first is 41,997 bytes on the dense tier and 75,277 on the hybrid — one
/// carrier — and the second is 175,297 / 139,777, which is over the 83,333-byte carrier and makes
/// the whole close three chunks that the genesis card's ruleset cannot file (ADR-0082 §5, and the
/// close is split at ONE until W5 is in the ruleset).
///
/// Under the per-CALL cadence the cache-write route is not the challenger's choice, it is its only
/// option, at two kinds of position: every PREFILL position (no checkpoint covers any of them),
/// and any tile straddling the last checkpoint's edge. So the amendment is not an optimisation —
/// it is what makes a dispute at those positions FILEABLE.
///
/// # Why it is free to the executor
///
/// The attention cache is prefix-stable: the K or V row written at position `j` is the same bytes
/// in every later checkpoint (`A16Cache::state_chunk_bytes_v1` reads
/// `layer[position_start .. position_start + position_count]` out of whatever cache it is handed).
/// So a per-position cadence costs the executor NOTHING to retain — the cache once, every earlier
/// checkpoint's chunk roots re-derivable from it — and one ragged tile's hash to commit, because
/// every complete tile's chunk hash is the same value it had at the checkpoint before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwCheckpointCadenceV1 {
    /// `(index + 1) × checkpoint_interval` decode calls covered; the prefill is uncovered.
    PerDecodeCall,
    /// `index + 1` POSITIONS covered, counting from the first prefill position.
    PerPosition,
}

/// **The cadence THIS class's checkpoint leg runs at** — the one dispatch, for the reason
/// [`crate::palw_state_chunk_map::palw_map_addresses_history_tiles_v1`] is the one dispatch under
/// it: the alternative is every reader writing `if map == v3 { … }`, and the first one to forget
/// counts a leg at one cadence while the producer filed it at another.
///
/// Read off the registered map, which is inside `shape_profile_id` and therefore inside the class
/// id: a registrant cannot buy a cheaper leg by declaring a coarser cadence, and a court cannot
/// judge one class's leg at another's rule.
pub fn palw_checkpoint_cadence_v1(profile: &PalwShapeProfileV3) -> PalwCheckpointCadenceV1 {
    if crate::palw_state_chunk_map::palw_map_addresses_history_tiles_v1(profile) {
        PalwCheckpointCadenceV1::PerPosition
    } else {
        PalwCheckpointCadenceV1::PerDecodeCall
    }
}

/// **The value a checkpoint leaf's `covered_decode_call` canonically carries at `index`.**
///
/// The field's NAME is the shipped one and its bytes are unchanged — `PalwCheckpointLeafV2` is not
/// re-versioned, and could not usefully be: what the counter counts is a function of
/// `state_chunk_map_id`, which `checkpoint_leaf_hash_v2` already binds into the leaf's preimage
/// beside it. A v2-mapped leaf and a v3-mapped leaf carrying the same number hash differently, so
/// the cadence is authenticated without a second field to disagree with.
///
/// On the shipped classes (`PerDecodeCall`, interval 1) this is `index + 1`, which is exactly what
/// they file today — the property `the_shipped_rows_checkpoint_legs_do_not_move` holds.
pub fn palw_checkpoint_covered_at_index_v1(profile: &PalwShapeProfileV3, index: u32, interval: u32) -> Option<u32> {
    match palw_checkpoint_cadence_v1(profile) {
        PalwCheckpointCadenceV1::PerDecodeCall => index.checked_add(1)?.checked_mul(interval),
        // One position a leaf. The registered `checkpoint_interval` still exists and still means
        // something — it is the RECURRENCE's spacing, [`palw_anchored_interval_for_profile_v1`] —
        // but it does not space the attention cache's leaves.
        PalwCheckpointCadenceV1::PerPosition => index.checked_add(1),
    }
}

/// **How many checkpoints a job of this shape canonically files** — the count
/// `palw_step_leg`'s shape pass recomputes and `Base0CheckpointCaptureV1::finish` is sealed at.
///
/// `PerDecodeCall`: `decode_calls / interval`, the shipped rule verbatim.
/// `PerPosition`: `prefill + decode_calls`, which is every position the cache ever holds — the
/// same `prefill + decode_calls` count [`crate::palw_step::kv_aux_leaf_count`] derives for the KV
/// aux series, because they are counting the same rows.
pub fn palw_checkpoint_count_v1(profile: &PalwShapeProfileV3, context: &PalwJobContextV2, interval: u32) -> u32 {
    let decode_calls = context.exact_decode_tokens.saturating_sub(1);
    match palw_checkpoint_cadence_v1(profile) {
        PalwCheckpointCadenceV1::PerDecodeCall => {
            if interval == 0 {
                0
            } else {
                decode_calls / interval
            }
        }
        PalwCheckpointCadenceV1::PerPosition => context.declared_prefill_tokens.saturating_add(decode_calls),
    }
}

/// **How many POSITIONS of the cache the checkpoint carrying `covered` covers** — the cadence-aware
/// twin of [`crate::palw_state_chunk_map::integer_kv_positions_at_v1`], and the number every
/// geometry a court or a capture derives is taken at.
///
/// `PerDecodeCall`: `prefill + covered`, the shipped rule verbatim (the prefill always ran).
/// `PerPosition`: `covered` — the counter already IS a position count.
pub fn palw_checkpoint_positions_at_v1(profile: &PalwShapeProfileV3, context: &PalwJobContextV2, covered: u32) -> u32 {
    match palw_checkpoint_cadence_v1(profile) {
        PalwCheckpointCadenceV1::PerDecodeCall => crate::palw_state_chunk_map::integer_kv_positions_at_v1(context, covered),
        PalwCheckpointCadenceV1::PerPosition => covered,
    }
}

/// **The absolute cache position a step coordinate sits at.**
///
/// The capture's call numbering is `call 0 = the prefill, position p` and `call c ≥ 1 = decode
/// call c, position 0`; the cache is written in one ascending run over both. Stating the map here
/// keeps the anchor rule, the residue rule and the recompute from each inventing it.
pub fn palw_absolute_position_v1(context: &PalwJobContextV2, call_index: u32, position: u32) -> Option<u32> {
    if call_index == 0 {
        return Some(position);
    }
    context.declared_prefill_tokens.checked_add(call_index.checked_sub(1)?)
}

/// **WHICH checkpoint is this step's anchor** — the `covered` value, exactly, that a refutation of
/// the step at `(call_index, position)` must carry.
///
/// Exactly, not "at most", for `verify_kv_anchor`'s own reason: a further-back checkpoint would
/// leave positions the evidence does not cover, and a challenger choosing among anchors would be
/// choosing which positions the court never sees.
///
/// # The two cadences anchor at two ENDS of the disputed position, and that is the whole saving
///
/// `PerDecodeCall` anchors BEFORE the step: `covered = c − 1`, the state after call `c − 1`. The
/// disputed call's own cache write is not in it and rides as a step opening — the residue. There is
/// no other choice, because a per-call leg has no checkpoint at the disputed call's own position
/// while the prefill has none at all.
///
/// `PerPosition` anchors AFTER it: `covered = p + 1`, the state once position `p`'s own K and V
/// rows have been written. Attention at `p` reads exactly positions `0..=p`, which is exactly what
/// that checkpoint holds, so the residue is EMPTY and the bottom is one chunk opening per kind with
/// nothing beside it. It is sound for the reason the anchored form is sound at all: the anchor's
/// rows and the cache-write leaves are two encodings of the same committed values (a lie in the
/// row at `p` is the cache-write node's fault and is disputed at that leaf, on either route), and
/// it is EXACT rather than "at most" for `verify_kv_anchor`'s own reason — a challenger choosing
/// among anchors would be choosing which positions the court never sees.
///
/// That is the difference between a dense graph-v5 close of 93,367 bytes and one of 82,719: two
/// chunks against one, and the genesis card's ruleset files one.
///
/// `None` where no anchor exists and the long form is the only route: the prefill call under
/// `PerDecodeCall`, which no checkpoint covers at all. Under `PerPosition` there is always one,
/// position 0 included — its checkpoint is the leg's first leaf.
pub fn palw_checkpoint_covered_for_step_v1(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    call_index: u32,
    position: u32,
) -> Option<u32> {
    match palw_checkpoint_cadence_v1(profile) {
        PalwCheckpointCadenceV1::PerDecodeCall => call_index.checked_sub(1).filter(|_| call_index > 0),
        PalwCheckpointCadenceV1::PerPosition => palw_absolute_position_v1(context, call_index, position)?.checked_add(1),
    }
}

/// **The spacing the RECURRENCE half of a checkpoint is committed at, from the PROFILE alone.**
///
/// [`palw_anchored_interval_for_court_v1`] is the same number read off the ruleset, and it is the
/// one the ADMISSION side prices with (it has the ruleset in hand). The refutation side does not:
/// `check_execution_step_refutation_opened_v1` is handed a binding and a weight oracle and no
/// consensus params at all. Deriving it from the profile there rather than passing `None` is what
/// makes the evidence a refutation assembles the evidence the class was charged for — stream F's
/// patch note 2: with `palw_checkpoint_interval_v1` called directly, the GDN window is `n_ctx / 32`
/// positions while the class is priced at one tile.
///
/// The two spellings can only disagree for a fused profile under an UNARMED court, and such a
/// class is refused at admission by name (`PalwClassAdmissionError::FusedSiteWithNoDissection`),
/// so no class that can be prosecuted is priced by one and prosecuted by the other.
/// `the_two_anchored_intervals_agree_wherever_a_class_is_admissible` holds it.
pub fn palw_anchored_interval_for_profile_v1(profile: &PalwShapeProfileV3) -> u32 {
    if crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(profile) {
        return crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4.min(profile.n_ctx.max(1));
    }
    palw_checkpoint_interval_v1(profile.n_ctx)
}

/// **Does the leaf at `index` carry the RECURRENCE section of a hybrid checkpoint?**
///
/// Under `PerPosition` the attention half is committed at every position and the recurrence half
/// is NOT: a `gdn` state is `heads × k_dim × v_dim × 4` bytes that no prefix-stability makes free,
/// so committing one per position would be a hash of the whole state at every token — 2 MiB a
/// position on the shipped hybrid geometry. The recurrence keeps the derived spacing
/// ([`palw_anchored_interval_for_profile_v1`]), which is exactly the window
/// `gdn_anchored_positions_v1` replays after an anchor, so every recurrence dispute still has an
/// anchor at most one window back.
///
/// Always true under `PerDecodeCall`: a leg at the per-call cadence commits the whole composition
/// at every leaf, which is what every shipped reader expects.
pub fn palw_checkpoint_leaf_carries_recurrence_v1(profile: &PalwShapeProfileV3, covered_positions: u32) -> bool {
    match palw_checkpoint_cadence_v1(profile) {
        PalwCheckpointCadenceV1::PerDecodeCall => true,
        PalwCheckpointCadenceV1::PerPosition => {
            let spacing = palw_anchored_interval_for_profile_v1(profile).max(1);
            covered_positions % spacing == 0
        }
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

/// **[`palw_kv_checkpoint_opening_bytes_v1`] at the cache map the class REGISTERED** (ADR-0082
/// Decision 4) — the `_for_map_v1` twin the recurrence half has had since ADR-0077 and the cache
/// half did not.
///
/// The rule is the recurrence twin's, verbatim: *the map the class REGISTERED, never one map
/// unconditionally*. Until graph v4 there was only one cache map whose price could differ, so the
/// unconditional reading was correct by accident; with `tiled_kv_state_chunk_map_id_v3` there are
/// two, and the gap they leave was measured — a class that registers the tile was charged 526,336
/// bytes for an opening its evidence carries in 18,432, **28.6x**, and was therefore admitted at
/// exactly the width it had before the tile existed. The tile bought the dense tier nothing until
/// this function existed.
///
/// * a map that addresses history TILES ([`crate::palw_state_chunk_map::palw_map_addresses_history_tiles_v1`])
///   — one tile: `min(n_ctx, PALW_ATTN_HISTORY_TILE_V4) x kv_row`, plus the path that proves it.
///   Flat in `n_ctx` past the tile, which is the whole content of Decision 4;
/// * every other map — the whole history, because v1's and v2's chunk derivation is "the widest
///   run of rows the leg admits" and on every registered geometry that run IS the history.
///
/// `None` only on overflow. Over-charging is the safe direction and under-charging is not, so the
/// unrecognised case falls to the history price rather than to a cheap one.
pub fn palw_kv_checkpoint_opening_bytes_for_map_v1(profile: &PalwShapeProfileV3, ladder: u64) -> Option<u64> {
    if crate::palw_state_chunk_map::palw_map_addresses_history_tiles_v1(profile) {
        return crate::palw_state_chunk_map::tiled_kv_chunk_bytes_v3(profile)?.checked_add(step_path_bytes_v1(ladder));
    }
    palw_kv_checkpoint_opening_bytes_v1(profile, ladder)
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
    palw_anchored_court_cost_for_court_v1(profile, None)
}

/// [`palw_anchored_court_cost_v1`] under a `palw_kary_court`-armed ruleset (ADR-0082 Decisions 2-5).
///
/// `court` is `None` before the fence and `Some` past it, and the CALLER reads the fence:
/// `params.palw_kary_court_active_at(daa)` decides whether there is a court, and
/// `PalwCourtParamsV2::dissection_arity` / `Params::palw_prompt_ids_form_at` say what it is. A cost
/// derivation that consulted a DAA score itself would price one class two ways depending on when
/// it was asked, which is the shape of every "admitted at one price, prosecuted at another" defect
/// this module records.
pub fn palw_anchored_court_cost_for_court_v1(
    profile: &PalwShapeProfileV3,
    court: Option<crate::palw_class_admission_v2::PalwKaryCourtV1>,
) -> Option<Result<PalwCourtCostV1, PalwClassAdmissionError>> {
    let rules = palw_class_ladder_rules_for_court_v1(profile, court)?;
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
    if floor == 0 { 1 } else { floor }
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

/// **ADR-0082 Decision 10 restates ADR-0077 Decision 14's floor in the unit that now earns**: the
/// canonical job's DECODE CALLS, `exact_decode_tokens − 1`
/// ([`crate::palw_step::job_leaf_split_capped_v1`]).
///
/// # Why the floor has to move with the unit
///
/// Decision 14's floor is `n_ctx / 8` CACHED POSITIONS, and a cached position used to be a unit of
/// earning: the numerator was the whole leaf count, prefill included. Past
/// `Params::palw_fp_decode_rules` it is not — a claim earns on its DECODE leaves — and the floor
/// becomes enterable from the other side. A canonical job of 4,000 prefill and 2 decode tokens has
/// a footprint of 4,001 at `n_ctx` 4,096 and clears the v1 floor of 512 comfortably, on ONE decode
/// call: the row is admitted, and then every honest job on it parks at the 64-quantum cap, which is
/// Decision 14's own 86 % loss arrived at from the opposite direction. Restating the floor in
/// decode calls is not a second gate, it is the SAME gate read in the unit the receipt is now
/// counted in.
pub const fn palw_job_decode_footprint_v1(decode_tokens: u32) -> u64 {
    (decode_tokens as u64).saturating_sub(1)
}

/// **Does this row's canonical job meet Decision 14's floor, in the unit that earns?**
///
/// `decode_rules` is `Params::palw_fp_decode_rules`, read by the CALLER and passed in — never read
/// inside a derivation. Off, this is [`palw_footprint_meets_the_row_v1`] byte for byte, which is
/// what every shipped preset gets. On, the footprint is the decode calls.
pub fn palw_footprint_meets_the_row_for_rules_v1(
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    decode_rules: bool,
) -> bool {
    let footprint = if decode_rules {
        palw_job_decode_footprint_v1(canonical.exact_decode_tokens)
    } else {
        palw_job_footprint_v1(canonical.declared_prefill_tokens, canonical.exact_decode_tokens)
    };
    footprint >= palw_canonical_footprint_floor_v1(profile.n_ctx)
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
    palw_class_ladder_rules_for_court_v1(profile, None)
}

/// **How many positions an anchored replay opens, under the court the ruleset actually runs**
/// (ADR-0077 Decision 11, narrowed by ADR-0082 Decision 4).
///
/// Before the k-ary court: [`palw_checkpoint_interval_v1`], `max(1, n_ctx / 32)` — the spacing a
/// class registers its checkpoints at, and therefore the longest replay a refutation standing on
/// one performs.
///
/// Past it, for a graph-v5 row: [`crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4`]. Decision
/// 4 says "the bottom of the dissection opens tiles, so the anchor is tile-addressed"; a class
/// whose anchor is addressed a tile at a time commits the state at every tile boundary, so the
/// replay after a verified anchor is one tile and not `n_ctx / 32` of them. That is what makes the
/// hybrid's `interval x 5 refs` recurrence evidence — Decision 6's "widest flat term" for that
/// family — actually flat: at `n_ctx / 32` it is linear in the context and the hybrid's v5 close is
/// not two to three carriers at any width above the 512 row (where `512 / 32` and the tile happen
/// to be the same 16).
///
/// **This is the one place the number is spelled, and every reader now reads it.** The recurrence
/// window in `palw_step_refute` (`gdn_anchored_positions_v1`'s interval argument) called
/// [`palw_checkpoint_interval_v1`] directly, so on a graph-v5 hybrid the evidence a refutation
/// assembled was `n_ctx / 32` positions while this priced one tile — an under-charge, which is the
/// direction that admits a class whose disputes nobody can carry. That call site now reads
/// [`palw_anchored_interval_for_profile_v1`], which is this function with the ruleset argument the
/// refutation path does not have; the two can only disagree for a fused profile under an unarmed
/// court, and such a class is refused at admission by name.
pub fn palw_anchored_interval_for_court_v1(
    profile: &PalwShapeProfileV3,
    court: Option<crate::palw_class_admission_v2::PalwKaryCourtV1>,
) -> u32 {
    if court.is_some() {
        return palw_anchored_interval_for_profile_v1(profile);
    }
    palw_checkpoint_interval_v1(profile.n_ctx)
}

/// [`palw_class_ladder_rules_v1`] under a `palw_kary_court`-armed ruleset (ADR-0082 Decisions 2-5).
///
/// Three things move and nothing else does: the anchored interval becomes the history tile
/// ([`palw_anchored_interval_for_court_v1`]), the cost shape carries the court's arity so a fused
/// site is priced by its bottom rather than by the row, and the prompt-id term takes the form the
/// ruleset armed (Decision 5). `None` reproduces the shipped rule byte for byte, which is what
/// makes every existing caller unchanged.
pub fn palw_class_ladder_rules_for_court_v1(
    profile: &PalwShapeProfileV3,
    court: Option<crate::palw_class_admission_v2::PalwKaryCourtV1>,
) -> Option<crate::palw_class_admission_v2::PalwClassLadderRulesV1> {
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
    let interval = palw_anchored_interval_for_court_v1(profile, court);
    let mut cost_shape = PalwCourtCostShapeV1::checkpoint_anchored_v1(profile, interval, ladder, 0);
    // **The map the class REGISTERED, on the cache half too** (ADR-0082 Decision 4). This read
    // `palw_kv_checkpoint_opening_bytes_v1` unconditionally until U-04, which charged a class that
    // registers the history tile the whole history's opening — 526,336 against 18,432 on the dense
    // row — and admitted it at exactly the width it had before the tile existed.
    cost_shape.kv_checkpoint_bytes = palw_kv_checkpoint_opening_bytes_for_map_v1(profile, ladder).unwrap_or(u64::MAX);
    // The map the class REGISTERED, never gdn v1 unconditionally: a v2 class priced at v1's
    // convolution window is charged for thirty-one heads its evidence will not carry, and a v1
    // class priced at v2's is charged less than its evidence costs — the direction that admits a
    // class whose disputes nobody can raise.
    cost_shape.gdn_checkpoint_bytes = palw_gdn_checkpoint_opening_bytes_for_map_v1(profile, ladder).unwrap_or(0);
    // **The dissection, and the id form that rides with it** (Decisions 2, 3 and 5). Both come
    // from the caller's reading of the ruleset — neither is inferred from the other, because they
    // move the price in opposite directions and inferring the cheaper one is how a class is
    // admitted at a price its challengers cannot pay. The dissection is applied only where there
    // is a fused site to price: a court may be k-ary while the class in front of it is graph v2,
    // and then nothing about its cost moves.
    if let Some(k) = court {
        cost_shape = cost_shape.with_prompt_ids_form_v1(k.prompt_ids_form);
        if crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(profile) {
            cost_shape = cost_shape.with_dissection_v1(k.dissection_arity);
        }
    }
    Some(crate::palw_class_admission_v2::PalwClassLadderRulesV1 {
        ladder,
        cost_shape,
        canonical_footprint_floor: palw_canonical_footprint_floor_v1(profile.n_ctx),
    })
}

// =================================================================================================
// ADR-0082 Decision 6 — the close ceiling is a DERIVATION over the rows a genesis set registers
// =================================================================================================

/// One family's builder: a projection from a context width to that family's row.
pub type PalwLadderFamilyV1 = fn(u32) -> Result<PalwShapeProfileV3, PalwStepError>;

/// **The widest close any row of any registered family can be prosecuted at, and WHICH node it is**
/// (ADR-0082 Decision 6).
///
/// A `max` tells you the number and never which term produced it, so this returns the breakdown
/// row — the same walk's own answer (`derive_court_cost_rows_v1`), never a second derivation that
/// merely agrees. A `(family, row)` pair the walk cannot price is not admissible at that width and
/// contributes nothing; `None` means no pair priced at all, which is a genesis set that registers
/// no prosecutable row rather than a cheap one.
pub fn palw_widest_close_over_the_ladder_v1(
    families: &[PalwLadderFamilyV1],
    rows: &[u32],
    court: Option<crate::palw_class_admission_v2::PalwKaryCourtV1>,
) -> Option<(usize, u32, PalwCourtCostRowV1)> {
    let mut best: Option<(usize, u32, PalwCourtCostRowV1)> = None;
    for (index, build) in families.iter().enumerate() {
        for row in rows {
            let Ok(profile) = build(*row) else { continue };
            let Some(rules) = palw_class_ladder_rules_for_court_v1(&profile, court) else { continue };
            let Ok(mut breakdown) = derive_court_cost_rows_v1(&profile, rules.cost_shape) else { continue };
            if breakdown.is_empty() {
                continue;
            }
            // `derive_court_cost_rows_v1` sorts largest close first, so the binding node is row 0.
            let binding = breakdown.remove(0);
            if best.as_ref().is_none_or(|(_, _, b)| binding.close_bytes > b.close_bytes) {
                best = Some((index, *row, binding));
            }
        }
    }
    best
}

/// **`DEFAULT_MAX_CLOSE_CHUNKS`, as the derivation it is supposed to be** (ADR-0082 Decision 6).
///
/// `palw_close_chunks_for_bytes_v1` of the widest close any row of any family the genesis set
/// registers can be prosecuted at. Two genesis sets are on the table and this answers for both,
/// which is the point of stating it as a function of `(families, rows, court)` rather than as a
/// number: for the graph-v2/v3 context rows it returns design A's own count, and for graph-v5 rows
/// under the dissection court it returns the count Decisions 1-5 buy.
///
/// **It is a ceiling the TRANSPORT must reach, not one the transport may ignore.** Decision 6's
/// code condition: until ADR-0080 W5's `court_close_groups` is in the ruleset, `max_close_chunks`
/// is ONE whatever this returns, and the admission gate refuses the rest — an admitted row whose
/// worst close no carrier can file is exactly the 5f state ADR-0082 §1.4 describes.
pub fn palw_close_chunks_for_ladder_v1(
    families: &[PalwLadderFamilyV1],
    rows: &[u32],
    court: Option<crate::palw_class_admission_v2::PalwKaryCourtV1>,
) -> Option<u64> {
    let (_, _, binding) = palw_widest_close_over_the_ladder_v1(families, rows, court)?;
    Some(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(binding.close_bytes))
}

/// The RC's graph-v2/v3 genesis set: the dense A16 row and the hybrid QWEN36 row, the two families
/// ADR-0077 Decision 13's ladder plans and the two `DEFAULT_MAX_CLOSE_CHUNKS`'s own doc names.
pub const PALW_LADDER_FAMILIES_V1: [PalwLadderFamilyV1; 2] = [palw_a16_context_row_profile_v1, palw_qwen36_context_row_profile_v1];

/// **The dense tier at a ladder row, graph v5** (ADR-0082 Decision 1).
///
/// [`palw_a16_context_row_profile_v1`]'s twin, and deliberately built the same way — through
/// `qwen25_a16_artifact_row_profile_v5` — so the two rows differ in exactly what Decision 1 changes
/// (the four attention nodes become one `AttnFused`, stream D's `palw_fuse_attention_site_v5`) and
/// in the map Decision 4 requires (`tiled_kv_state_chunk_map_id_v3`), and in nothing else. A NEW
/// class id by construction: the graph and the map are both inside `shape_profile_id`, so no
/// registered row moves.
pub fn palw_a16_context_row_profile_v5(n_ctx: u32) -> Result<PalwShapeProfileV3, PalwStepError> {
    crate::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5(crate::palw_qwen25_profile::PalwQwen25GeometryV1 {
        n_ctx,
        ..crate::palw_qwen25_profile::QWEN25_1_5B
    })
}

/// **The hybrid tier at a ladder row, graph v5** (ADR-0082 Decisions 1 and 4).
///
/// [`palw_qwen36_context_row_profile_v1`]'s twin. The map is the v3 COMPOSITION — `attn=` the
/// tiled cache the dissection's bottom opens, `gdn=` the head-sliced recurrence v2 already
/// enumerates — and it is set inside `qwen36_profile_v5` rather than here, so a v5 row cannot be
/// projected without it.
pub fn palw_qwen36_context_row_profile_v5(n_ctx: u32) -> Result<PalwShapeProfileV3, PalwStepError> {
    crate::palw_qwen36_profile::qwen36_artifact_row_profile_v5(crate::palw_qwen36_profile::PalwQwen36GeometryV1 {
        n_ctx,
        ..crate::palw_qwen36_profile::QWEN36_35B_A3B
    })
}

/// **The graph-v5 genesis set** — the same two families under ADR-0082 Decisions 1-5, which is the
/// other set Decision 6's derivation has to be evaluated over.
pub const PALW_LADDER_FAMILIES_V5: [PalwLadderFamilyV1; 2] = [palw_a16_context_row_profile_v5, palw_qwen36_context_row_profile_v5];

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
        // And the two constants the fence stands in front of.
        //
        // `PALW_STEP_MAX_LEAVES` is still 2^22 and is no longer the ladder: after ADR-0080 W1b the
        // executor reads `PalwCourtParamsV2::max_step_leaf_count`, so this constant is a default
        // for paths with no ruleset in hand. The RULESET's ladder is 2^26 (2026-09-03), and it is
        // pinned where it is chosen — `PALW_RC_COURT_MAX_STEP_LEAF_COUNT`.
        assert_eq!(PALW_STEP_MAX_LEAVES, 1 << 22, "the executor's default moved");
        assert_eq!(
            crate::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT,
            1 << 26,
            "the RULESET's ladder moved; the fence in front of the 2^32 rows assumes this depth"
        );
        // The move clock is 42: the derivation at the DEEPEST ladder the tree can reach, not at
        // the one shipping today. A clock derived for 2^26 alone would be 51 and would refuse to
        // assemble the moment this fence arms (66 x 51 + 216 = 3,582 > 3,000).
        assert_eq!(PALW_RC_WINDOWS_V1.court_turn_deadline, 42, "the shipped move clock moved without a fence");
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
                    w.court_turn_deadline,
                    // Each set's OWN reserve — 216 for the RC's 27 carriers, 8 for the devnet's
                    // one. Charging either window the other's is how a preset constant moves.
                    w.max_close_chunks()
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
                    armed.court_turn_deadline,
                    armed.max_close_chunks()
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
    ///
    /// The RC's ruleset throughout, because a clock without a ruleset is not a clock: the reserve
    /// is a function of `max_close_chunks` and testnet-11's is 27.
    #[test]
    fn the_derived_move_clock_is_the_largest_the_window_admits() {
        let rc_chunks = PALW_RC_WINDOWS_V1.max_close_chunks();
        assert_eq!(rc_chunks, 27, "the RC's court admits a 27-carrier close");
        // 2^32 leaves: 32 bisection rounds, two moves each, plus two terminal moves.
        assert_eq!(palw_court_move_count_v1(PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, PALW_CONTEXT_LADDER_TERMINAL_MOVES), 66);
        // testnet-11's 3,000-DAA court window, less ADR-0080's 216-DAA assembly reserve:
        // 66 × 42 + 216 = 2,988.
        //
        // **This used to be 45, and the change is the reserve, not an error.** ADR-0077 Decision 12
        // derives `turn_deadline ≤ 45` from the window and the move count in a file that never sees
        // this one, and the two agreeing at 45 was a real check while the reserve was 8. It is not
        // one any more on THIS ruleset: Decision 12's derivation has no assembly term, so the honest
        // statement is that the two agree once the reserve is subtracted from the window Decision 12
        // divides — and the last line of this test shows them agreeing exactly, at the chunk count
        // whose reserve is the 8 Decision 12 was written against.
        assert_eq!(palw_court_turn_deadline_v1(3_000, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2, rc_chunks), Some(42));
        assert_eq!(66 * 42 + palw_close_assembly_daa_v1(rc_chunks), 2_988);
        // One more DAA of clock does not fit, which is what "largest the window admits" means:
        // 66 × 43 + 216 = 3,054.
        assert!(!palw_ladder_fits_window_court_v1(3_000, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2, 43, rc_chunks));
        // A window that cannot hold a one-DAA clock is refused rather than saturated — and a window
        // that cannot even hold the reserve is refused for its own, earlier reason.
        assert_eq!(palw_court_turn_deadline_v1(66, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2, rc_chunks), None);
        assert_eq!(
            palw_court_turn_deadline_v1(palw_close_assembly_daa_v1(rc_chunks), PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2, rc_chunks),
            None
        );
        // **The same window, a one-carrier ruleset, a different clock**: 208 DAA of reserve come
        // back as three DAA of turn, and the number they come back to is Decision 12's own 45. The
        // parameter is not decoration — it is the difference between two networks' courts.
        assert_eq!(palw_court_turn_deadline_v1(3_000, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2, 1), Some(45));
    }

    /// **The devnet set's shipped move clock IS the one this derivation returns** — an equality, at
    /// the numbers the preset has always carried.
    ///
    /// It was weakened to a bracket while the reserve had no source; restored when the reserve was
    /// derived from the live ceilings; moved to `window_court 600 / clock 5` when the reserve read
    /// the ruleset DEFAULT; and is back at `300 / 4` now that it reads the DEVNET's ruleset. Through
    /// all four the property is the one worth keeping: the constant in the preset is not a choice
    /// somebody made, it is what the window, the ladder and this network's own close ceiling
    /// produce. When the derivation really moved, the preset moved with it. When the derivation was
    /// reading the wrong network's number, the preset moved back.
    #[test]
    fn the_devnet_move_clock_is_the_derived_one() {
        let chunks = PALW_DEVNET_WINDOWS_V1.max_close_chunks();
        assert_eq!(chunks, 1, "the devnet court admits a one-carrier close");
        let derived = palw_court_turn_deadline_v1(PALW_DEVNET_WINDOWS_V1.window_court, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2, chunks)
            .expect("the devnet window holds the deeper ladder");
        assert_eq!(
            PALW_DEVNET_WINDOWS_V1.court_turn_deadline, derived,
            "the shipped devnet clock is no longer the one its window derives"
        );
        assert_eq!(derived, 4, "the devnet window's move clock moved");
        assert!(palw_ladder_fits_window_court_v1(
            PALW_DEVNET_WINDOWS_V1.window_court,
            PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            2,
            PALW_DEVNET_WINDOWS_V1.court_turn_deadline,
            chunks
        ));
        assert_eq!(66 * PALW_DEVNET_WINDOWS_V1.court_turn_deadline, 264);
        assert_eq!(66 * derived + palw_close_assembly_daa_v1(chunks), 272);
        // Largest the window admits, here too: 66 × 5 + 8 = 338 > 300.
        assert!(!palw_ladder_fits_window_court_v1(300, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, 2, 5, chunks));
    }

    /// **The reserve reads the RULESET's chunk count — and the ruleset is the one being asked
    /// about.**
    ///
    /// This test has been wrong three times, in three directions, and all three are one error:
    ///
    /// * it first DEMANDED a devnet widening, to fund a reserve sized for 27 carriers by a number
    ///   with no derivation behind it;
    /// * it then FORBADE the widening, from a derivation over `max_close_bytes` and the widest
    ///   binding — real arithmetic over real measurements, answering "what does a close need at
    ///   today's registered rows" when a reserve must answer "what may a SESSION be asked to
    ///   assemble";
    /// * and it then demanded the widening again out of W3's ruleset field, which is the right
    ///   question asked of the WRONG RULESET. `max_close_chunks` is per network. The devnet's is 1.
    ///
    /// So the reserve is a function, the count is its argument, and every assertion below names
    /// whose court it is talking about.
    #[test]
    fn the_reserve_reads_the_rulesets_chunk_count() {
        // The default is the RC's, and the RC's window set is where it is spent — the constant is a
        // reading of one ruleset, not a value any derivation reaches for on its own.
        assert_eq!(PALW_COURT_MAX_CLOSE_CHUNKS_V1, crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS);
        assert_eq!(PALW_COURT_MAX_CLOSE_CHUNKS_V1, 27);
        assert_eq!(PALW_RC_WINDOWS_V1.max_close_chunks(), PALW_COURT_MAX_CLOSE_CHUNKS_V1);
        assert_eq!(PALW_COURT_CHUNKED_CLOSES_PER_SIDE_V1, 4);
        assert_eq!(palw_close_assembly_daa_v1(27), 2 * 4 * 27);
        assert_eq!(palw_close_assembly_daa_v1(27), 216);
        assert_eq!(palw_close_assembly_daa_v1(1), 8);

        // **The transport does not exist yet, and the RC's reserve is sized for the RULE anyway.**
        // `PALW_OBJECT_CHUNK_MAX_COUNT` = 8 caps `ObjectChunk`, the certification lane's transport;
        // the court's group is its own table (ADR-0080 design A, built by W5). Sizing the reserve
        // for what can be carried this morning is exactly the mistake this test recorded twice.
        assert!(PALW_COURT_MAX_CLOSE_CHUNKS_V1 > crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_COUNT as u64);

        let deep = PALW_CONTEXT_LADDER_MAX_STEP_LEAVES;
        let moves = palw_court_move_count_v1(deep, PALW_CONTEXT_LADDER_TERMINAL_MOVES);
        assert_eq!(moves, 66);
        let shipped_moves = palw_court_move_count_v1(PALW_STEP_MAX_LEAVES, PALW_CONTEXT_LADDER_TERMINAL_MOVES);
        assert_eq!(shipped_moves, 46);

        // ---- the RC: its shipped clock still fits the shipped ladder, and Phase B costs it 3 ----
        let rc_chunks = PALW_RC_WINDOWS_V1.max_close_chunks();
        assert_eq!(PALW_RC_WINDOWS_V1.window_court, 3_000);
        // The shipped clock is NOT the derivation at the shallow ladder any more, and that is
        // deliberate: 2^22 would derive 60 and 2^26 would derive 51, and both are values that stop
        // being legal when a deeper ladder arrives. The shipped clock is the derivation at the
        // deepest reachable ladder, so it is legal at every shallower one.
        assert_eq!(palw_court_turn_deadline_v1(3_000, PALW_STEP_MAX_LEAVES, 2, rc_chunks), Some(60));
        assert_eq!(PALW_RC_WINDOWS_V1.court_turn_deadline, 42, "the RC's shipped clock is the DEEP ladder's derivation");
        assert!(
            palw_ladder_fits_window_court_v1(
                3_000,
                crate::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT,
                2,
                42,
                rc_chunks
            ),
            "the shipped clock must fit the ruleset's own 2^26 ladder, not only the fenced one"
        );
        let rc_deep = palw_court_turn_deadline_v1(3_000, deep, 2, rc_chunks).expect("the RC window holds the deep ladder");
        assert_eq!(rc_deep, 42, "the assembly reserve costs the deep ladder three DAA of turn clock");
        assert_eq!(moves * rc_deep + palw_close_assembly_daa_v1(rc_chunks), 2_988);
        assert!(palw_ladder_fits_window_court_v1(3_000, deep, 2, rc_deep, rc_chunks));

        // ---- the devnet: the RC's reserve does not fit its window at any clock, and does not
        //      have to, because the RC's reserve is not the devnet's ----
        assert!(!palw_ladder_fits_window_court_v1(300, deep, 2, 4, rc_chunks), "66 x 4 + 216 = 480 > 300");
        assert!(!palw_ladder_fits_window_court_v1(300, PALW_STEP_MAX_LEAVES, 2, 4, rc_chunks), "46 x 4 + 216 = 400 > 300");
        let devnet_chunks = PALW_DEVNET_WINDOWS_V1.max_close_chunks();
        assert_eq!(PALW_DEVNET_WINDOWS_V1.window_court, 300, "the devnet court window pays for its OWN closes");
        assert!(palw_ladder_fits_window_court_v1(300, deep, 2, PALW_DEVNET_WINDOWS_V1.court_turn_deadline, devnet_chunks));
        assert!(palw_ladder_fits_window_court_v1(
            300,
            PALW_STEP_MAX_LEAVES,
            2,
            PALW_DEVNET_WINDOWS_V1.court_turn_deadline,
            devnet_chunks
        ));
        assert_eq!(moves * PALW_DEVNET_WINDOWS_V1.court_turn_deadline + palw_close_assembly_daa_v1(devnet_chunks), 272);

        // ---- and the clock each one's honest move actually needs is inside it ----
        //
        // On the RC's ruleset one honest move is a single DAA of replay and twenty-six of
        // ASSEMBLY; on the devnet's it is the replay alone. The devnet clock covers its own move
        // with three DAA to spare, and would be short of the RC's by twenty-three — which is the
        // arithmetic behind "not a drill": a devnet carrying the RC's ceiling needs a window that
        // holds `66 × 27 + 216 = 1,998`, so `window_court ≥ 1,999`, and 1,999 DAA at 120 s a block
        // is 66 hours. Devnet's whole purpose is a lattice a session can cross.
        let interval = palw_checkpoint_interval_v1(512);
        let rc_move = crate::palw_court_deadline::palw_court_move_cost_daa_v1(&PALW_COURT_COST_QWEN36, interval, rc_chunks);
        let devnet_move = crate::palw_court_deadline::palw_court_move_cost_daa_v1(&PALW_COURT_COST_QWEN36, interval, devnet_chunks);
        assert_eq!((rc_move, devnet_move), (27, 1));
        assert!(rc_deep >= rc_move, "the RC clock must cover one honest move over a {rc_chunks}-carrier close");
        assert!(
            PALW_DEVNET_WINDOWS_V1.court_turn_deadline >= devnet_move,
            "the devnet clock must cover one honest move over a {devnet_chunks}-carrier close"
        );
        assert_eq!(moves * rc_move + palw_close_assembly_daa_v1(rc_chunks), 1_998);
        assert!(!palw_ladder_fits_window_court_v1(1_998, deep, 2, rc_move, rc_chunks), "the fit is strict: 1,998 is not < 1,998");
        assert!(palw_ladder_fits_window_court_v1(1_999, deep, 2, rc_move, rc_chunks));
    }

    /// **Two rulesets, one derivation, two clocks — and each clock covers an honest move on the
    /// ruleset it came from.** This is the property the whole `max_close_chunks` parameter exists
    /// for, and it is the one a reader should check first.
    ///
    /// Neither number is typed. Both fall out of `(window_court − reserve − 1) / moves` with that
    /// network's own chunk count, re-spelled here from the set's own fields, and each is then
    /// checked against the floor `palw_court_move_cost_daa_v1` derives from the SAME count — the
    /// honest replay plus the carriers the close occupies. Asserting the two numbers without
    /// asserting they are DERIVED would let the next reader hardcode either; deriving them without
    /// checking the floor would let a court convict the responder by clock, which is the failure
    /// SA-4 exists to name.
    #[test]
    fn the_two_rulesets_derive_different_clocks_and_each_covers_its_own_move() {
        use crate::palw_court_deadline::palw_court_move_cost_daa_v1;
        let moves = palw_court_move_count_v1(PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, PALW_CONTEXT_LADDER_TERMINAL_MOVES);
        let mut derived = Vec::new();
        for (name, w) in [("testnet-11 (RC)", PALW_RC_WINDOWS_V1), ("devnet", PALW_DEVNET_WINDOWS_V1)] {
            let chunks = w.max_close_chunks();
            let reserve = palw_close_assembly_daa_v1(chunks);
            let clock = palw_court_turn_deadline_v1(
                w.window_court,
                PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
                PALW_CONTEXT_LADDER_TERMINAL_MOVES,
                chunks,
            )
            .unwrap_or_else(|| panic!("{name}: no move clock fits its own window"));
            // The derivation, re-spelled from this set's own numbers — so the assertion below is
            // about an expression and not about a value somebody could type in.
            assert_eq!(clock, (w.window_court - reserve - 1) / moves, "{name}: the clock is not what the window derives");
            assert!(
                palw_ladder_fits_window_court_v1(
                    w.window_court,
                    PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
                    PALW_CONTEXT_LADDER_TERMINAL_MOVES,
                    clock,
                    chunks
                ),
                "{name}: the derived clock does not fit its own window"
            );
            // And one honest move on THIS ruleset fits inside it, at every row of the ladder and
            // every priced class — the clock's whole purpose.
            for row in PALW_COURT_ROW_COSTS {
                for n_ctx in PALW_CONTEXT_LADDER_ROWS {
                    let floor = palw_court_move_cost_daa_v1(&row, palw_checkpoint_interval_v1(n_ctx), chunks);
                    assert!(
                        clock >= floor,
                        "{name}: {} at n_ctx {n_ctx} needs {floor} DAA over a {chunks}-carrier close and the clock is {clock} ({})",
                        row.row,
                        row.measured_on
                    );
                }
            }
            derived.push((name, chunks, reserve, clock));
        }
        // The two answers. They are DIFFERENT, and that is the point: one expression, two networks,
        // two courts. A later change that makes them equal by giving both rulesets one close
        // ceiling is a decision, and it fails here rather than passing quietly.
        assert_eq!(derived[0], ("testnet-11 (RC)", 27, 216, 42));
        assert_eq!(derived[1], ("devnet", 1, 8, 4));
        assert_ne!(derived[0].3, derived[1].3, "one derivation, two rulesets, one clock — re-read the parameter");
    }

    // ---------------------------------------------------------------------------------------
    // SA-4 — the deadline is derived, and it clears its own floor
    // ---------------------------------------------------------------------------------------

    /// **SA-4's floor, per row, at every ladder row's interval — and the derived deadline clears
    /// it.** A deadline below this convicts the honest responder by the clock, which is the failure
    /// SA-4 exists to name.
    ///
    /// **The floor is the SPLIT close's** (ADR-0080 W4): a move is the honest replay plus the
    /// blocks the close occupies, and `palw_court_move_cost_daa_v1` is the one cost model that
    /// carries both — passed the chunk count rather than adding `(chunks − 1)` here, because two
    /// spellings of one cost model is the defect this pair of files exists to avoid.
    ///
    /// The clock is the RC's, so the count is the RC's: `armed` is `PALW_RC_WINDOWS_V1` re-derived,
    /// and a floor priced at some other network's carriers would be a floor for a court that does
    /// not exist. [`tests::the_two_rulesets_derive_different_clocks_and_each_covers_its_own_move`]
    /// is the same check over both sets.
    #[test]
    fn the_derived_deadline_clears_the_measured_replay_floor_for_every_row() {
        use crate::palw_court_deadline::palw_court_move_cost_daa_v1;
        let armed = palw_context_ladder_windows_v1(PALW_RC_WINDOWS_V1, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES).expect("derivable");
        let chunks = armed.max_close_chunks();
        assert_eq!(chunks, PALW_COURT_MAX_CLOSE_CHUNKS_V1, "the RC set carries the default close ceiling");
        for row in PALW_COURT_ROW_COSTS {
            for n_ctx in PALW_CONTEXT_LADDER_ROWS {
                let interval = palw_checkpoint_interval_v1(n_ctx);
                // The replay alone, and the whole move with its close assembled over this
                // ruleset's `max_close_chunks` carriers.
                let replay = palw_court_replay_floor_daa_v1(&row, interval);
                let floor = palw_court_move_cost_daa_v1(&row, interval, chunks);
                assert_eq!(floor, replay + chunks - 1, "the move cost is not the replay plus its carriers");
                assert!(
                    armed.court_turn_deadline >= floor,
                    "{}: at n_ctx {n_ctx} (interval {interval}) one honest move needs {floor} DAA — {replay} of replay plus \
                     {} blocks of close assembly — and the derived clock is {} — a court that convicts by clock ({})",
                    row.row,
                    chunks - 1,
                    armed.court_turn_deadline,
                    row.measured_on
                );
            }
        }
        // The binding row, in numbers, and the shape of the bound has changed with the reserve:
        // the hybrid at the top rung replays two DAA and then ASSEMBLES twenty-six more, because
        // the ruleset admits a 27-carrier close. 2 + 26 = 28 against a derived clock of 42.
        //
        // Worth reading twice: assembly, not replay, is now most of an honest move's cost at every
        // shipped row — the replay floors are 1 or 2 DAA and the assembly term is 26. So the clock
        // this court needs is set by how many blocks a close occupies, not by how fast a host can
        // recompute, which is the opposite of what SA-4 was written to protect against and is why
        // the reserve had to be a ruleset quantity rather than a measurement of today's rows.
        assert_eq!(armed.court_turn_deadline, 42);
        assert_eq!(palw_court_move_cost_daa_v1(&PALW_COURT_COST_QWEN36, 256, chunks), 28);
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
        use crate::palw_class_admission_v2::{PalwClassAdmissionError, verify_class_admission_v4};
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
                    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                    // No dissection: this row has no fused site, and the long form is what a court
                    // with no bottom to stand on charges.
                    dissection: None,
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
        use crate::palw_step_refute::{KDESC_Q36_GDN_STEP, canonical_input_leaves_v1_anchored};
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
            PALW_GDN_STATE_CHUNK_MAP_NAME_V1, PALW_GDN_STATE_CHUNK_MAP_NAME_V2, gdn_state_chunk_map_id_v1, gdn_state_chunk_map_id_v2,
            hybrid_state_chunk_map_id_v1, hybrid_state_chunk_map_id_v2, integer_kv_state_chunk_map_id_v2,
            palw_hybrid_state_chunk_map_name_v1, palw_hybrid_state_chunk_map_name_v2,
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
        use crate::palw_step_refute::{DotStructure, gdn_core_anchored_replay_v1};
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
        use crate::palw_step_refute::{DotStructure, PalwStepRefuteError, gdn_core_anchored_replay_v1};
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

// =================================================================================================
// U-00 — the graph-v4 tiled attention map, MEASURED rather than assumed
// =================================================================================================

/// **Is a disputed attention leaf's close FLAT in the position under `tiled_kv_state_chunk_map_id_v3`?**
///
/// [`crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4`] says of itself that the tile "is a
/// CONSTANT and not a function of `n_ctx`, which is the property the ladder needs: … the close a v4
/// attention node derives is flat in the context (W1) instead of linear in it." Two ADRs (0080 and
/// 0081) and the competing chunk-the-reductions design were written on that sentence and nothing
/// had measured it.
///
/// **It is not flat, and this module is where that is written down.** The tile flattens exactly one
/// term — the checkpoint-chunk opening — and three others stay linear in `n_ctx`:
///
/// 1. the **interval-scaled history**, because `palw_checkpoint_interval_v1` is `max(1, n_ctx / 32)`
///    and an anchored replay opens that many positions of the node's refs;
/// 2. the **attention probability row**, `attn_heads × n_ctx` lanes, which is a property of the
///    GRAPH and which no state chunk map addresses;
/// 3. the **prompt ids**, four bytes a position on EVERY node, because
///    `prompt_token_ids_hash_v2` (`palw_v2.rs:521`) is a flat digest and no window of ids can be
///    opened against it.
///
/// What the tile does buy is real and large — the widest dense row the 80 KiB carrier admits under
/// the anchored court moves from 30 to 223 — and it is a constant factor, not a change of shape.
/// The measurement binary that produced every number here is
/// `misaka-palw-base0/src/bin/palw-tile-measure.rs`.
///
/// Nothing in this module is armed: `Params::palw_context_ladder` is `None` on every preset and
/// no registered class declares the v3 map.
#[cfg(test)]
mod u00_tiled_attention_measurement {
    use super::*;
    use crate::palw_class_admission_v2::{PalwCourtCostRowV1, derive_court_cost_rows_v1, derive_court_cost_v1};
    use crate::palw_state_chunk_map::{
        PALW_ATTN_HISTORY_TILE_V4, hybrid_state_chunk_map_id_v2, hybrid_state_chunk_map_id_v3, integer_kv_state_chunk_map_id_v2,
        tiled_kv_chunk_bytes_v3, tiled_kv_state_chunk_map_id_v3,
    };
    use crate::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_MAX_LEAVES, worst_case_step_leaf_count_capped_v1};

    const LADDER: u64 = PALW_CONTEXT_LADDER_MAX_STEP_LEAVES;

    /// **The carrier every figure in this module was measured against**, and it is not
    /// `DEFAULT_MAX_CLOSE_BYTES` any more.
    ///
    /// U-00 swept "the widest dense row inside 81,920" (ADR-0082 §1.2's table: 21 / 30 / 223) —
    /// one close in one transaction, the pre-ADR-0080 ceiling. On this branch design A's chunk
    /// group has raised the shipped constant to 2,250,000, and three tests here read the CONSTANT
    /// while their pins describe the CARRIER, so the merge turned all three red without a single
    /// number moving. Named here so the two readings can be stated side by side rather than one
    /// silently standing in for the other: `the_close_the_chunk_group_admits` below is the same
    /// sweep at the shipped ceiling.
    const CARRIER_80K: u64 = 80 * 1024;

    /// The dense ladder row declaring the graph-v4 tiled attention map. A DIFFERENT class from the
    /// v2-mapped one — `state_chunk_map_id` is inside `shape_profile_id` — which is why this is a
    /// projection and not a mutation of anything registered.
    fn dense_v3(n_ctx: u32) -> PalwShapeProfileV3 {
        let mut profile = palw_a16_context_row_profile_v1(n_ctx).expect("the dense row projects");
        profile.state_chunk_map_id = tiled_kv_state_chunk_map_id_v3();
        profile
    }

    /// The same row on the map the shipped rule prices.
    fn dense_v2(n_ctx: u32) -> PalwShapeProfileV3 {
        let mut profile = palw_a16_context_row_profile_v1(n_ctx).expect("the dense row projects");
        profile.state_chunk_map_id = integer_kv_state_chunk_map_id_v2();
        profile
    }

    /// What one v3 chunk opens, plus the path that proves it — the honest
    /// `PalwCourtCostShapeV1::kv_checkpoint_bytes` for a v3 class, which no shipped function
    /// answers (see [`the_v3_map_is_not_priced_by_the_ladder_rule`]).
    fn v3_kv_checkpoint_bytes(profile: &PalwShapeProfileV3) -> u64 {
        tiled_kv_chunk_bytes_v3(profile).expect("the v3 chunk derives") + step_path_bytes_v1(LADDER)
    }

    /// Decision 11's court at the v3 price, with the interval the caller states.
    fn v3_shape(profile: &PalwShapeProfileV3, interval: u32, count_ids: bool) -> PalwCourtCostShapeV1 {
        let mut shape = PalwCourtCostShapeV1::checkpoint_anchored_v1(profile, interval, LADDER, 0);
        shape.kv_checkpoint_bytes = v3_kv_checkpoint_bytes(profile);
        // The v4 composition's `gdn=` half is `PALW_GDN_STATE_CHUNK_MAP_NAME_V2` verbatim, so its
        // recurrence opening IS priceable — the dispatch simply does not reach it. Asked of the
        // half here so an attention measurement never stands on a recurrence charge of zero.
        let mut as_gdn_half = profile.clone();
        if profile.state_chunk_map_id == hybrid_state_chunk_map_id_v3() {
            as_gdn_half.state_chunk_map_id = hybrid_state_chunk_map_id_v2();
        }
        shape.gdn_checkpoint_bytes = palw_gdn_checkpoint_opening_bytes_for_map_v1(&as_gdn_half, LADDER).unwrap_or(0);
        shape.count_ids = count_ids;
        shape
    }

    /// The most expensive node that reads the KV cache — "a disputed attention leaf", by name.
    fn worst_kv_row(profile: &PalwShapeProfileV3, shape: PalwCourtCostShapeV1) -> PalwCourtCostRowV1 {
        let rows = derive_court_cost_rows_v1(profile, shape).expect("the breakdown derives");
        rows.into_iter()
            .find(|r| {
                let table = match r.table {
                    "pre" => &profile.pre_nodes,
                    "gdn" => &profile.gdn_nodes,
                    "attn" => &profile.attn_nodes,
                    _ => &profile.post_nodes,
                };
                table
                    .get(r.index)
                    .is_some_and(|n| n.input_refs.iter().any(|x| *x == PALW_STEP_INPUT_KV_K || *x == PALW_STEP_INPUT_KV_V))
            })
            .expect("the dense graph has a node that reads the cache")
    }

    /// **The headline, and it contradicts the map's own doc comment.** The v3 tile flattens the
    /// checkpoint OPENING and leaves the close growing.
    ///
    /// Stated as three facts in one test because separating them is how a reader comes away with
    /// the wrong one: the opening really is flat, the close really is not, and the second is not a
    /// consequence of the first failing.
    ///
    /// Command: `cargo run -p misaka-palw-base0 --bin palw-tile-measure` (§0, §1–2).
    #[test]
    fn the_v3_tile_flattens_the_opening_and_not_the_close() {
        // 1. The opening IS flat. `tiled_kv_chunk_bytes_v3` is `min(n_ctx, 16) × kv_row`, so past
        //    the tile it does not move; the v2 opening is the whole history and moves with it.
        let openings: Vec<u64> = [1_000u32, 4_096, 32_768].iter().map(|n| v3_kv_checkpoint_bytes(&dense_v3(*n))).collect();
        assert_eq!(openings, vec![18_432, 18_432, 18_432], "the v3 chunk opening is not flat in the context");
        assert_eq!(openings[0], (PALW_ATTN_HISTORY_TILE_V4 as u64) * 2 * 128 * 4 + step_path_bytes_v1(LADDER));
        let v2: Vec<u64> =
            [1_000u32, 4_096].iter().map(|n| palw_kv_checkpoint_opening_bytes_v1(&dense_v2(*n), LADDER).expect("derives")).collect();
        assert_eq!(v2, vec![1_026_048, 4_196_352], "the v2 opening stopped being the whole history");

        // 2. And the CLOSE is not flat — under the same anchored court, at the v3 price, with the
        //    interval the rule derives. Four times the context is 3.53 times the close: a large
        //    constant factor off the v2 reading, and the same SHAPE.
        let close = |n_ctx: u32| {
            let profile = dense_v3(n_ctx);
            let shape = v3_shape(&profile, palw_checkpoint_interval_v1(n_ctx), true);
            worst_kv_row(&profile, shape).close_bytes
        };
        let (narrow, wide) = (close(1_000), close(4_096));
        assert_eq!((narrow, wide), (228_769, 806_577), "the anchored v3 close moved — re-run palw-tile-measure and re-pin");
        assert!(wide > narrow * 3, "the v3 attention close became flat in the context — this module's whole finding moved");

        // 3. Which is not the id term standing in front of the answer: with the ids removed the
        //    growth is 97.9 % of what it was.
        let bare = |n_ctx: u32| {
            let profile = dense_v3(n_ctx);
            let shape = v3_shape(&profile, palw_checkpoint_interval_v1(n_ctx), false);
            worst_kv_row(&profile, shape).close_bytes
        };
        assert_eq!((bare(1_000), bare(4_096)), (224_769, 790_193));
        let id_only = 4 * (4_096u64 - 1_000);
        assert!(
            bare(4_096) - bare(1_000) > 40 * id_only,
            "the residue after the ids is no longer the dominant growth — the finding changed shape"
        );

        // 4. And "not flat" is **LINEAR**, not "sub-linear" — which two points cannot tell apart
        //    and which is the reading the next design inherits. 228,769 → 806,577 is 3.53x over a
        //    4x context, and a ratio-only classifier called that sub-linear; a third context says
        //    the slopes are 185.3 then 187.3, i.e. one straight line carrying a ~40 KiB constant.
        //    The constant is what the tile bought. The slope is what it did not.
        let mid = close(2_000);
        assert_eq!(mid, 414_033, "the v3 close at the midpoint moved — re-run palw-tile-measure and re-pin");
        let (s1, s2) = ((mid - narrow) as f64 / 1_000.0, (wide - mid) as f64 / 2_096.0);
        assert!(
            s2 <= s1 * 1.10 && s1 <= s2 * 1.10,
            "the v3 close stopped being one straight line in n_ctx: {s1} bytes/position then {s2} — \
             if it genuinely became sub-linear, this module's headline is what changed"
        );
    }

    /// **What is still linear, once the interval is held still: the attention PROBABILITY ROW.**
    ///
    /// `palw_checkpoint_interval_v1` is `max(1, n_ctx / 32)`, so the anchored court's own
    /// `history_positions` is `n_ctx`-shaped and a sweep against it cannot tell "the map is not
    /// flat" from "the interval rule is not flat". Held at the tile's own width, the residue is a
    /// clean straight line at ~50.3 bytes a position — and its slope is the graph's:
    /// `attn_heads × 4` for the opened lanes plus the run's per-tile headers.
    ///
    /// This is the term no state chunk map can reach, and the one a long-context design has to
    /// answer for. Command: `palw-tile-measure` §6.
    #[test]
    fn with_the_interval_held_the_residue_is_the_probability_row() {
        let at = |n_ctx: u32| {
            let profile = dense_v3(n_ctx);
            let shape = v3_shape(&profile, PALW_ATTN_HISTORY_TILE_V4, false);
            worst_kv_row(&profile, shape).close_bytes
        };
        let points = [(256u32, at(256)), (512, at(512)), (1_024, at(1_024)), (4_096, at(4_096))];
        assert_eq!(
            points.map(|(_, b)| b),
            [123_889, 136_817, 162_609, 317_105],
            "the constant-interval residue moved — re-run palw-tile-measure and re-pin"
        );
        // One slope, at three widths — which is what "linear" means and what a ratio would hide.
        let heads = palw_a16_context_row_profile_v1(512).expect("projects").attn_heads as f64;
        for pair in points.windows(2) {
            let slope = (pair[1].1 - pair[0].1) as f64 / (pair[1].0 - pair[0].0) as f64;
            assert!(
                slope > 4.0 * heads && slope < 4.0 * heads + 3.0,
                "the residue's slope between n_ctx {} and {} is {slope}, not the probability row's {}",
                pair[0].0,
                pair[1].0,
                4.0 * heads
            );
        }
    }

    /// **The prompt-id term costs four bytes a position on EVERY node** — the prediction the u04
    /// stream's justification rests on, confirmed exactly rather than by inspection of
    /// `palw_v2.rs:521`.
    ///
    /// `prompt_token_ids_hash_v2` is `put_u32_seq(ids); keyed64(DOMAIN)` — a FLAT digest — so a
    /// challenger who addresses any node carries every id, and `derive_court_cost_walk_v1` charges
    /// `n_ctx × 4` unconditionally. The consequence is the one that matters: **no node of the graph
    /// is flat in the context**, whatever the map does, so a design that flattens the cache and
    /// leaves this term has not flattened the close.
    ///
    /// The gather pays it three times over (its own ids, the decode pin's ids, and the
    /// unconditional term), which is why it is excluded from the exact arm rather than folded into
    /// it. Command: `palw-tile-measure` §4.
    #[test]
    fn every_node_pays_four_bytes_a_position_for_the_flat_prompt_id_digest() {
        use crate::palw_step::PalwStepOpKindV1 as Op;
        let (narrow, wide) = (1_000u32, 4_096u32);
        let rows = |n_ctx: u32| {
            let profile = dense_v3(n_ctx);
            derive_court_cost_rows_v1(&profile, v3_shape(&profile, palw_checkpoint_interval_v1(n_ctx), true)).expect("derives")
        };
        let (a, b) = (rows(narrow), rows(wide));
        let id_term = 4 * (wide as u64 - narrow as u64);
        assert_eq!(id_term, 12_384);
        let profile = dense_v3(wide);
        let mut exact = 0usize;
        for row in &b {
            let before = a.iter().find(|r| (r.table, r.index) == (row.table, row.index)).expect("the graphs have the same nodes");
            let delta = row.close_bytes - before.close_bytes;
            assert!(delta >= id_term, "{}[{}] grew {delta}, less than the prompt ids themselves", row.table, row.index);
            let table = match row.table {
                "pre" => &profile.pre_nodes,
                "gdn" => &profile.gdn_nodes,
                "attn" => &profile.attn_nodes,
                _ => &profile.post_nodes,
            };
            let node = table.get(row.index).expect("resolves");
            let reads_history = node.op_kind == Op::GatedDeltaNet
                || node.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V);
            // A node that neither reads the history, nor gathers, nor has an `n_ctx`-wide row pays
            // the ids and NOTHING else — the id term exactly, with no residue to hide in.
            //
            // Context-shaped is read off `PalwStepOutLenV1::KvScaled`, the graph's own declaration
            // that a row's width is a multiple of `n_ctx` — for this node's output OR for any row
            // it opens, since a run's lanes are the SOURCE node's width.
            let kv_scaled =
                |n: &crate::palw_step::PalwStepNodeV1| matches!(n.out_len, crate::palw_step::PalwStepOutLenV1::KvScaled { .. });
            let width_is_context_shaped =
                kv_scaled(node) || node.input_refs.iter().any(|r| table.get(*r as usize).is_some_and(kv_scaled));
            if !reads_history && node.op_kind != Op::EmbedLookup && !width_is_context_shaped {
                assert_eq!(delta, id_term, "{}[{}] {:?} pays more than the prompt ids", row.table, row.index, row.op_kind);
                exact += 1;
            }
        }
        assert!(exact >= 20, "only {exact} nodes were pinned at the bare id term — the classification stopped selecting");
        assert_eq!(
            b.iter()
                .filter(|r| {
                    a.iter().find(|x| (x.table, x.index) == (r.table, r.index)).is_some_and(|x| x.close_bytes == r.close_bytes)
                })
                .count(),
            0,
            "some node became flat in the context — the flat prompt digest stopped being charged everywhere"
        );
    }

    /// **What the tile actually buys, as the number the next reader should inherit: 30 → 223.**
    ///
    /// The widest `n_ctx` the 80 KiB carrier admits, swept rather than solved because the
    /// predicate is not monotone (`palw_checkpoint_interval_v1` steps at every multiple of 32).
    /// Three courts, and the first two reproduce the figures this branch was handed — which is
    /// what says the harness is measuring the same thing they were.
    ///
    /// Command: `palw-tile-measure` §3.
    #[test]
    fn the_tile_moves_the_widest_dense_row_from_thirty_to_two_hundred_and_twenty_three() {
        let budget = CARRIER_80K;
        let widest = |price: &dyn Fn(u32) -> Option<u64>| {
            let mut best = 0u32;
            for n_ctx in 1..=512u32 {
                match price(n_ctx) {
                    Some(bytes) if bytes <= budget => best = n_ctx,
                    _ => break,
                }
            }
            best
        };
        let unfenced = widest(&|n| derive_court_cost_v1(&dense_v2(n)).ok().map(|c| c.max_close_bytes));
        let armed_v2 = widest(&|n| palw_anchored_court_cost_v1(&dense_v2(n)).and_then(|r| r.ok()).map(|c| c.max_close_bytes));
        let armed_v3 = widest(&|n| {
            let profile = dense_v3(n);
            derive_court_cost_shaped_v1(&profile, v3_shape(&profile, palw_checkpoint_interval_v1(n), true))
                .ok()
                .map(|c| c.max_close_bytes)
        });
        assert_eq!(unfenced, 21, "the unfenced dense row is no longer 21 — the established figure moved");
        assert_eq!(armed_v2, 30, "the armed dense row is no longer 30 — the established figure moved");
        assert_eq!(armed_v3, 223, "the tiled dense row moved — re-run palw-tile-measure and re-pin");
        // A constant factor, not a change of shape: 7.4x on a term that is still linear.
        assert!(armed_v3 > armed_v2 * 7 && armed_v3 < armed_v2 * 8);

        // **And 223 is now what the SHIPPED route answers** (ADR-0082 Decision 4, U-04).
        //
        // `armed_v3` above prices the v3 class through `v3_shape`, a shape this test builds,
        // because until U-04 no shipped function answered "what does a tiled class's cache anchor
        // cost" — `palw_class_ladder_rules_v1` charged it the v2 map's whole-history opening,
        // 526,336 against 18,432, and admitted it at exactly the 30 it had before the tile
        // existed. `the_v3_map_is_not_priced_by_the_ladder_rule` was that gap, and its own text
        // said what to do when it closed: delete it, and re-pin these at the cheaper price. The
        // gap is closed by `palw_kv_checkpoint_opening_bytes_for_map_v1`, so the two readings are
        // one number and the 223 is a property of the gate rather than of this test.
        let shipped = |map: fn() -> crate::Hash64| {
            let mut best = 0u32;
            for n_ctx in 1..=512u32 {
                let mut row = palw_a16_context_row_profile_v1(n_ctx).expect("projects");
                row.state_chunk_map_id = map();
                match palw_anchored_court_cost_v1(&row).and_then(|r| r.ok()) {
                    Some(cost) if cost.max_close_bytes <= budget => best = n_ctx,
                    _ => break,
                }
            }
            best
        };
        assert_eq!(shipped(integer_kv_state_chunk_map_id_v2), armed_v2, "the v2 class's shipped price moved");
        assert_eq!(
            shipped(tiled_kv_state_chunk_map_id_v3),
            armed_v3,
            "the ladder rule stopped pricing a v3 class by its own map — the tile buys the dense tier nothing again"
        );
        // The two openings the gate now distinguishes, from the gate's own function.
        let v3 = dense_v3(512);
        assert_eq!(palw_kv_checkpoint_opening_bytes_for_map_v1(&v3, LADDER), Some(18_432), "the honest tiled opening moved");
        assert_eq!(palw_kv_checkpoint_opening_bytes_v1(&v3, LADDER), Some(526_336), "the whole-history opening moved");
        assert_eq!(palw_class_ladder_rules_v1(&v3).expect("mapped").cost_shape.kv_checkpoint_bytes, 18_432);
        assert_eq!(
            palw_class_ladder_rules_v1(&dense_v2(512)).expect("mapped").cost_shape.kv_checkpoint_bytes,
            526_336,
            "a v2 class must still be charged the history its evidence carries"
        );
    }

    /// **The same sweep at the ceiling ADR-0080 design A actually froze**, so the two budgets are
    /// two numbers rather than one number that quietly changed meaning.
    ///
    /// At 2,250,000 counted bytes the close stops being the dense tier's binding gate entirely:
    /// the shipped `2^22` ladder refuses the row at 40 before any price does
    /// (`the_carrier_binds_fifty_times_before_the_ladder_does` measures the same wall from the
    /// leaf side). That is ADR-0080's whole content for this family, and it is why the numbers
    /// above are stated against the carrier they were measured at.
    #[test]
    fn the_close_the_chunk_group_admits() {
        let budget = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
        let widest = |price: &dyn Fn(u32) -> Option<u64>| {
            let mut best = 0u32;
            for n_ctx in 1..=512u32 {
                match price(n_ctx) {
                    Some(bytes) if bytes <= budget => best = n_ctx,
                    _ => break,
                }
            }
            best
        };
        let unfenced = widest(&|n| derive_court_cost_v1(&dense_v2(n)).ok().map(|c| c.max_close_bytes));
        assert_eq!(unfenced, 39, "the unfenced dense row is bound by the 2^22 ladder, not by the chunk group");
        assert!(
            worst_case_step_leaf_count_capped_v1(&dense_v2(40), PALW_STEP_MAX_LEAVES).is_err(),
            "40 is refused by the ladder — if it is the PRICE that refuses, the close became the gate again"
        );
    }

    // **`the_v3_map_is_not_priced_by_the_ladder_rule` lived here, and ADR-0082 U-04 closed the gap
    // it measured.**
    //
    // It asserted that `palw_class_ladder_rules_v1` charged a tiled class the v2 map's
    // whole-history opening — 526,336 bytes against the 18,432 its evidence carries, 28.6x — and
    // its own message said what to do when that stopped being true: *"if it now reads the class's
    // own map, delete this test and re-pin the ones above at the cheaper price."* The cache half
    // has its `_for_map_v1` twin now
    // ([`palw_kv_checkpoint_opening_bytes_for_map_v1`], Decision 4), the ladder rule reads it, and
    // both halves of what this test pinned are asserted from the gate's own functions at the end
    // of `the_tile_moves_the_widest_dense_row_from_thirty_to_two_hundred_and_twenty_three`. The
    // note is kept rather than the test because the next reader of "30 → 223" needs to know that
    // the 223 was once a measurement no shipped function could reproduce.

    // **`the_graph_v4_hybrid_composition_has_no_priced_recurrence_anchor` lived here, and
    // ADR-0082 U-04 closed the gap it measured.**
    //
    // It recorded a **mainnet blocker**: `gdn_state_terms_for_map_v1` dispatched on the whole
    // composition id and knew `hybrid_state_chunk_map_id_v1` and `…_v2` but not `…_v3`, so it
    // answered `None`, `palw_class_ladder_rules_v1` turned that into `.unwrap_or(0)`, and a hybrid
    // class registering the tiled attention map was admitted with its recurrence anchor charged
    // at ZERO — "a v1 class priced at v2's … the direction that admits a class whose disputes
    // nobody can raise", quoting the comment it quoted. Its own message said the remedy: *"the v4
    // composition is priced now — close this gap in `palw_class_ladder_rules_v1` and delete this
    // test."*
    //
    // It was dormant while nothing registered the v3 composition. ADR-0082 Decision 4 makes that
    // composition the map a graph-v5 hybrid registers, which turned it live, and
    // `gdn_state_terms_for_map_v1` has the arm now. What it cost, measured: the hybrid graph-v5
    // row's close moved 200,732 → 274,460 bytes and 3 carriers → 4, the delta being exactly the
    // 71,680-byte head-sliced opening plus its 2,048-byte path. Both halves of what it pinned are
    // asserted from the gate's own functions in
    // `palw_state_chunk_map::hybrid_composition_tests::the_recurrence_chunks_are_what_the_anchor_is_priced_at`.
    //
    // The note is kept rather than the test because ADR-0082 Decision 6 sizes the hybrid v5 row at
    // "two to three" carriers, and the next reader needs to know that the fourth is this charge.

    /// **The carrier binds long before the ladder does** — 223 against 11,477, a factor of 51.
    ///
    /// `derive_court_cost_shaped_v1` calls `worst_case_step_leaf_count_capped_v1` before it prices
    /// anything, so above the ladder a class is not expensive but UNPRICEABLE, and a reader who
    /// saw only "TooManyLeaves" at `n_ctx` 32,768 might conclude the enumeration is what stops a
    /// long context. It is not, at any width the close can pay for. Whatever a long-context design
    /// does, it has to move the CLOSE.
    ///
    /// (`TooManyLeaves`'s `got` is the running total at the position the walk gave up on, not the
    /// class's leaf count — the enumeration returns early by design. These are the true counts,
    /// taken with the cap at `u64::MAX`.) Command: `palw-tile-measure` §7.
    #[test]
    fn the_carrier_binds_fifty_times_before_the_ladder_does() {
        let fits = |n_ctx: u32, cap: u64| worst_case_step_leaf_count_capped_v1(&dense_v3(n_ctx), cap).is_ok();
        assert!(fits(39, PALW_STEP_MAX_LEAVES) && !fits(40, PALW_STEP_MAX_LEAVES), "the shipped ladder's dense ceiling moved");
        assert!(fits(11_477, LADDER) && !fits(11_478, LADDER), "Decision 12's dense ceiling moved");
        assert_eq!(worst_case_step_leaf_count_capped_v1(&dense_v3(11_477), u64::MAX).expect("counts"), 4_294_844_784);
        // 223 is `the_tile_moves_the_widest_dense_row_from_thirty_to_two_hundred_and_twenty_three`'s
        // answer, restated as the comparison rather than re-swept.
        assert!(11_477 > 223 * 51, "the ladder ceiling stopped being fifty times the carrier's");
    }

    /// The measurement binary keeps its own copy of [`step_path_bytes_v1`] because that function is
    /// private to this module. Two spellings of one computation is the defect this tree keeps
    /// recording, so they are pinned equal here.
    #[test]
    fn the_tile_measure_path_constant_is_the_ladders() {
        assert_eq!(step_path_bytes_v1(LADDER), 64 * 32);
        assert_eq!(step_path_bytes_v1(LADDER), 2_048);
    }
}

// =================================================================================================
// U-04 — Decision 6's ceiling as a derivation, and Decision 2's bottom as a measurement
// =================================================================================================

/// **ADR-0082 U-04.** `DEFAULT_MAX_CLOSE_CHUNKS` is `palw_close_chunks_for_ladder_v1` over the
/// genesis set, evaluated for BOTH sets that are on the table; the bottom of a fused dispute is
/// inside one carrier at every context; and a graph-v5 row's close is flat in `n_ctx` except for
/// the prompt-id term (Z0's second half).
#[cfg(test)]
mod u04_flat_close {
    use super::*;
    use crate::palw_class_admission_v2::PalwKaryCourtV1;
    use crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1;
    use crate::palw_mode_v2::{DEFAULT_MAX_CLOSE_CHUNKS, palw_close_bytes_for_chunks_v1, palw_close_chunks_for_bytes_v1};
    use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use crate::palw_state_chunk_map::{PALW_ATTN_HISTORY_TILE_V4, tiled_kv_state_chunk_map_id_v3};

    /// The court the RC would arm: Decision 3's worked arity, Decision 5's id form, the RC's own
    /// court window. The arity is a PARAMETER here rather than a constant — `palw_court_arity_v1`
    /// (stream E) is what derives it from the move budget at genesis.
    fn kary(arity: u8) -> PalwKaryCourtV1 {
        PalwKaryCourtV1 {
            dissection_arity: arity,
            prompt_ids_form: PalwPromptIdsFormV1::MerkleV1,
            window_court_daa: PALW_RC_WINDOWS_V1.window_court,
        }
    }

    /// A job context shaped like the class's, for the cadence rules — every field but the two
    /// token counts is irrelevant to them and is the type's own default.
    fn job() -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-palw-rc".to_vec(),
            job_id: crate::Hash64::default(),
            job_nullifier: crate::Hash64::default(),
            assignment_id: crate::Hash64::default(),
            execution_seed: [0; 32],
            model_profile_id: crate::Hash64::default(),
            runtime_manifest_hash: crate::Hash64::default(),
            runtime_class_id: crate::Hash64::default(),
            shape_profile_id: crate::Hash64::default(),
            trace_scheme_id: crate::Hash64::default(),
            cu_ruleset_id: crate::Hash64::default(),
            tokenizer_id: crate::Hash64::default(),
            prompt_token_ids_hash: crate::Hash64::default(),
            declared_prefill_tokens: 7,
            exact_decode_tokens: 5,
            max_context_tokens: 512,
        }
    }

    /// **Decision 6, for both genesis sets, as a DERIVATION rather than a number.**
    ///
    /// `DEFAULT_MAX_CLOSE_CHUNKS` is `palw_close_chunks_for_ladder_v1` over the rows the genesis
    /// set registers, and the shipped 27 is what that returns for the graph-v2/v3 set at ADR-0077
    /// Decision 13's first row. The value is NOT touched here: it is the 5f genesis card's, and a
    /// derivation that disagreed with it would be a finding and not an edit.
    #[test]
    fn the_close_ceiling_is_the_derivation_over_the_genesis_set() {
        // ---- the graph-v2/v3 set, at Decision 13's first row. The two numbers
        // `DEFAULT_MAX_CLOSE_CHUNKS`'s own doc quotes, reproduced by the walk rather than recited,
        // and the node each of them binds on.
        let v23: Vec<(u64, u64, String)> = PALW_LADDER_FAMILIES_V1
            .iter()
            .map(|build| {
                let (_, _, binding) = palw_widest_close_over_the_ladder_v1(&[*build], &[512], None).expect("the 512 row prices");
                (
                    binding.close_bytes,
                    palw_close_chunks_for_bytes_v1(binding.close_bytes),
                    format!("{}[{}] {:?} {}", binding.table, binding.index, binding.op_kind, binding.weight_name),
                )
            })
            .collect();
        for row in &v23 {
            println!("v2/v3: {} bytes = {} chunks; binds {}", row.0, row.1, row.2);
        }
        assert_eq!((v23[0].0, v23[0].1), (1_154_673, 14), "the dense graph-v2 512 row's close moved");
        assert_eq!((v23[1].0, v23[1].1), (2_240_241, 27), "the hybrid graph-v3 512 row's close moved");
        for row in &v23 {
            assert!(row.2.contains("attn_values.a16"), "the context-linear attention close stopped binding the v2/v3 rows: {}", row.2);
        }
        assert_eq!(
            palw_close_chunks_for_ladder_v1(&PALW_LADDER_FAMILIES_V1, &[512], None),
            Some(DEFAULT_MAX_CLOSE_CHUNKS),
            "the shipped close ceiling is no longer the derivation over the genesis set it was chosen for"
        );

        // ---- the graph-v5 set, under the dissection court. Decision 6 says "one to three"; the
        // derivation says ONE for the dense tier's model width, TWO once the bottom is charged at
        // the route the court can actually play, and FOUR for the hybrid — whose recurrence anchor
        // the v3 composition's dispatch was pricing at zero until this stream added the arm.
        let v5: Vec<(u64, u64, String)> = PALW_LADDER_FAMILIES_V5
            .iter()
            .map(|build| {
                let (_, _, binding) =
                    palw_widest_close_over_the_ladder_v1(&[*build], &[512], Some(kary(16))).expect("the v5 512 row prices");
                (
                    binding.close_bytes,
                    palw_close_chunks_for_bytes_v1(binding.close_bytes),
                    format!("{}[{}] {:?} {}", binding.table, binding.index, binding.op_kind, binding.weight_name),
                )
            })
            .collect();
        for row in &v5 {
            println!("v5: {} bytes = {} chunks; binds {}", row.0, row.1, row.2);
        }
        // **The dense tier is ONE carrier, and this is the number the genesis card turns on.**
        //
        // It was `(216_019, 3)` — three chunks, which the card's ruleset cannot file, because the
        // bottom was charged at the CACHE-WRITE route: under the per-DECODE-CALL cadence a dispute
        // at a prefill position had no checkpoint to anchor on at all, and a decode dispute's tile
        // could straddle the last checkpoint's edge, so the route a challenger could actually file
        // was the long one at 175,297 bytes.
        //
        // ADR-0082 Decision 4 as amended puts a checkpoint after EVERY position of a class whose
        // map addresses history tiles, and anchors a dispute at position `p` on the checkpoint at
        // `p + 1` — the state once `p`'s own K and V rows are written, which is exactly the
        // `0..=p` the attention at `p` reads. So the tile route is available at every position with
        // an EMPTY residue, the bottom is 41,997 bytes, and the close is one chunk. The rule that
        // moved this number is `palw_checkpoint_cadence_v1`; nothing about the graph, the map or
        // the arithmetic changed.
        assert_eq!((v5[0].0, v5[0].1), (82_719, 1), "the dense graph-v5 512 row's close moved");
        assert!(v5[0].2.contains("AttnFused"), "the dense v5 row stopped binding on its fused site: {}", v5[0].2);
        // **And what the row would cost if only the CACHE-WRITE route could be filed** — the number
        // the amendment removes, kept so the size of what it bought stays attributable.
        {
            use crate::palw_class_admission_v2::{palw_attn_bottom_cache_write_bytes_v1, palw_attn_bottom_tile_route_bytes_v1};
            let (d_head, kv_dim, tile, src, path) = (128u64, 256u64, 8u64, 128u64, 64 * 32u64);
            let positions = PALW_ATTN_HISTORY_TILE_V4 as u64;
            let cache = palw_attn_bottom_cache_write_bytes_v1(d_head, kv_dim, positions, tile, src, path).expect("derives");
            let ckpt = palw_attn_bottom_tile_route_bytes_v1(d_head, kv_dim, positions, tile, path).expect("derives");
            let cache_route_close = v5[0].0 - ckpt + cache;
            println!(
                "dense v5 @ 512: {} bytes = {} chunks charged (checkpoint route, per-position cadence); {cache_route_close} bytes \
                 = {} chunks on the cache-write route alone",
                v5[0].0,
                v5[0].1,
                palw_close_chunks_for_bytes_v1(cache_route_close)
            );
            assert_eq!(cache_route_close, 216_019, "the cache-write route's close moved");
            assert_eq!(palw_close_chunks_for_bytes_v1(cache_route_close), 3, "the cache-write route is three chunks");
        }
        // The hybrid: the recurrence's replay evidence, `interval x 5 refs`, plus the head-sliced
        // anchor. Decision 6 names this term and sizes it at two to three; it is four.
        assert_eq!((v5[1].0, v5[1].1), (274_460, 4), "the hybrid graph-v5 512 row's close moved");
        assert!(v5[1].2.contains("GatedDeltaNet"), "the hybrid v5 row stopped binding on its recurrence: {}", v5[1].2);
        assert_eq!(palw_close_chunks_for_ladder_v1(&PALW_LADDER_FAMILIES_V5, &[512], Some(kary(16))), Some(4));

        // ---- and what Decision 6's own numbers ARE, so the difference is attributable rather than
        // a disagreement: the dense model width at the tile route, both id forms.
        let dense = PALW_LADDER_FAMILIES_V5[0](512).expect("projects");
        let rules = palw_class_ladder_rules_for_court_v1(&dense, Some(kary(16))).expect("mapped");
        let rows = derive_court_cost_rows_v1(&dense, rules.cost_shape).expect("derives");
        let ffn = rows.iter().find(|r| r.weight_name.ends_with("ffn_down.weight")).expect("the dense row has a down projection");
        assert_eq!(ffn.close_bytes, 80_504, "ADR-0082 Decision 6's '~80,504 with the Merkle ones' moved");
        let flat =
            derive_court_cost_rows_v1(&dense, rules.cost_shape.with_prompt_ids_form_v1(PalwPromptIdsFormV1::Flat)).expect("derives");
        let ffn_flat = flat.iter().find(|r| r.weight_name.ends_with("ffn_down.weight")).expect("present");
        assert_eq!(ffn_flat.close_bytes, 82_080, "ADR-0082 Decision 6's '82,080 bytes with the flat ids' moved");
        assert_eq!(palw_close_chunks_for_bytes_v1(ffn.close_bytes), 1, "the dense model width is one carrier");
    }

    // =============================================================================================
    // ADR-0082 Decision 4, amended — a tiled-map class checkpoints EVERY position
    // =============================================================================================

    /// **The cadence is the class's own map, and the shipped rows keep theirs.**
    ///
    /// The one property every other test here stands on: nothing a shipped class files moves,
    /// because `palw_map_addresses_history_tiles_v1` is false for every map they register and the
    /// per-call arm is the shipped arithmetic verbatim.
    #[test]
    fn the_checkpoint_cadence_is_the_classs_own_map() {
        use crate::palw_state_chunk_map::{integer_kv_state_chunk_map_id_v1, integer_kv_state_chunk_map_id_v2};

        let ctx = |prefill: u32, decode: u32| PalwJobContextV2 { declared_prefill_tokens: prefill, exact_decode_tokens: decode, ..job() };

        // The shipped maps: per DECODE CALL, and every rule is the one shipped today.
        let mut v2 = PALW_LADDER_FAMILIES_V5[0](512).expect("projects");
        for map in [integer_kv_state_chunk_map_id_v1(), integer_kv_state_chunk_map_id_v2()] {
            v2.state_chunk_map_id = map;
            assert_eq!(palw_checkpoint_cadence_v1(&v2), PalwCheckpointCadenceV1::PerDecodeCall);
            let c = ctx(7, 5); // prefill 7, decode_calls 4
            assert_eq!(palw_checkpoint_count_v1(&v2, &c, 1), 4, "decode_calls / interval, the shipped rule");
            assert_eq!(palw_checkpoint_count_v1(&v2, &c, 2), 2);
            assert_eq!(palw_checkpoint_covered_at_index_v1(&v2, 0, 1), Some(1));
            assert_eq!(palw_checkpoint_covered_at_index_v1(&v2, 3, 2), Some(8), "(index + 1) x interval");
            assert_eq!(palw_checkpoint_positions_at_v1(&v2, &c, 2), 9, "prefill + covered");
            assert_eq!(
                palw_checkpoint_positions_at_v1(&v2, &c, 2),
                crate::palw_state_chunk_map::integer_kv_positions_at_v1(&c, 2),
                "the cadence-aware twin IS integer_kv_positions_at_v1 on a per-call class"
            );
            // The prefill has no anchor at all, which is the whole defect the amendment removes.
            assert_eq!(palw_checkpoint_covered_for_step_v1(&v2, &c, 0, 3), None);
            assert_eq!(palw_checkpoint_covered_for_step_v1(&v2, &c, 1, 0), Some(0));
            assert_eq!(palw_checkpoint_covered_for_step_v1(&v2, &c, 4, 0), Some(3), "disputed_call - 1, exactly");
        }

        // The tiled map: per POSITION, prefill included.
        let v5 = PALW_LADDER_FAMILIES_V5[0](512).expect("projects");
        assert_eq!(v5.state_chunk_map_id, tiled_kv_state_chunk_map_id_v3(), "the dense v5 row registers the tiled map");
        assert_eq!(palw_checkpoint_cadence_v1(&v5), PalwCheckpointCadenceV1::PerPosition);
        let c = ctx(7, 5);
        assert_eq!(palw_checkpoint_count_v1(&v5, &c, 1), 11, "prefill 7 + 4 decode calls = every position the cache holds");
        assert_eq!(
            palw_checkpoint_count_v1(&v5, &c, 16),
            11,
            "the registered interval does not space the ATTENTION leaves — it spaces the recurrence"
        );
        assert_eq!(palw_checkpoint_covered_at_index_v1(&v5, 0, 16), Some(1), "index + 1 POSITIONS");
        assert_eq!(palw_checkpoint_covered_at_index_v1(&v5, 10, 16), Some(11));
        assert_eq!(palw_checkpoint_positions_at_v1(&v5, &c, 6), 6, "the counter already IS a position count");
        // The anchor exists at every position but the first, and it is the same HISTORY the
        // per-call rule names at a decode call — read at a different index.
        // **After the position, not before it.** The checkpoint at `p + 1` holds exactly the
        // `0..=p` rows attention at `p` reads, so the residue is empty and the bottom is one chunk
        // per kind — which is the difference between a two-chunk close and a one-chunk one.
        assert_eq!(palw_checkpoint_covered_for_step_v1(&v5, &c, 0, 0), Some(1), "even position 0 has an anchor: the first leaf");
        assert_eq!(palw_checkpoint_covered_for_step_v1(&v5, &c, 0, 3), Some(4), "a PREFILL position now has an anchor");
        assert_eq!(palw_checkpoint_covered_for_step_v1(&v5, &c, 1, 0), Some(8), "decode call 1 writes position 7");
        // The anchor never names a checkpoint the leg does not have: `p + 1` is at most the count.
        for call in 0..=4u32 {
            let positions: Vec<u32> = if call == 0 { (0..c.declared_prefill_tokens).collect() } else { vec![0] };
            for position in positions {
                let covered = palw_checkpoint_covered_for_step_v1(&v5, &c, call, position).expect("an anchor at every position");
                assert!(covered <= palw_checkpoint_count_v1(&v5, &c, 1), "the anchor names leaf {covered}, past the leg's end");
                // And it holds exactly the rows the step reads: `kv_len` of them.
                let kv_len = if call == 0 { position + 1 } else { c.declared_prefill_tokens + call };
                assert_eq!(palw_checkpoint_positions_at_v1(&v5, &c, covered), kv_len, "the anchor is the step's own kv_len");
            }
        }
        // The hybrid composition answers the same way: what the question is about is the ATTENTION
        // cache, and a hybrid has one.
        let hybrid = PALW_LADDER_FAMILIES_V5[1](512).expect("projects");
        assert_eq!(hybrid.state_chunk_map_id, crate::palw_state_chunk_map::hybrid_state_chunk_map_id_v3());
        assert_eq!(palw_checkpoint_cadence_v1(&hybrid), PalwCheckpointCadenceV1::PerPosition);
    }

    /// **The recurrence keeps the derived spacing, and the composition says so.**
    ///
    /// A `heads x k_dim x v_dim x 4` state is not prefix-stable, so a per-position commitment of it
    /// would hash the whole state at every token. The attention half rides every leaf; the
    /// recurrence half rides the leaves at `palw_anchored_interval_for_profile_v1`.
    #[test]
    fn the_recurrence_half_rides_the_derived_spacing_and_the_attention_half_every_position() {
        let hybrid = PALW_LADDER_FAMILIES_V5[1](512).expect("projects");
        let spacing = palw_anchored_interval_for_profile_v1(&hybrid);
        assert_eq!(spacing, PALW_ATTN_HISTORY_TILE_V4, "a fused row's anchored window is one history tile");
        for positions in 1..=64u32 {
            let carries = palw_checkpoint_leaf_carries_recurrence_v1(&hybrid, positions);
            assert_eq!(carries, positions % spacing == 0, "the recurrence rides the leaves at its own spacing");
            let geometry = crate::palw_state_chunk_map::hybrid_state_geometry_for_covered_v1(&hybrid, positions).expect("derives");
            assert_eq!(geometry.gdn_chunk_count() > 0, carries, "the composition enumerates the section the cadence names");
            assert!(geometry.attn.chunk_count() > 0, "the attention half is on EVERY leaf");
            assert_eq!(
                geometry.attn,
                crate::palw_state_chunk_map::tiled_kv_state_geometry_v3(&hybrid, positions).expect("derives"),
                "the attention half is the standalone map verbatim"
            );
        }
        // A per-CALL class's leaves all carry the whole composition, which is what every shipped
        // reader of `hybrid_state_geometry_v3` expects.
        let mut per_call = hybrid.clone();
        per_call.state_chunk_map_id = crate::palw_state_chunk_map::hybrid_state_chunk_map_id_v2();
        for positions in [1u32, 7, 16, 33] {
            assert!(palw_checkpoint_leaf_carries_recurrence_v1(&per_call, positions));
        }
    }

    /// **The two spellings of the anchored interval agree wherever a class is admissible.**
    ///
    /// The price side reads the ruleset (`palw_anchored_interval_for_court_v1`); the refutation
    /// side has no ruleset and reads the profile. They differ only for a fused profile under an
    /// unarmed court, and such a class is refused at admission by name — so no class that can be
    /// prosecuted is priced by one number and prosecuted at another.
    #[test]
    fn the_two_anchored_intervals_agree_wherever_a_class_is_admissible() {
        for build in PALW_LADDER_FAMILIES_V5 {
            for n_ctx in [512u32, 4_096] {
                let Ok(profile) = build(n_ctx) else { continue };
                assert!(crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&profile));
                assert_eq!(
                    palw_anchored_interval_for_court_v1(&profile, Some(kary(16))),
                    palw_anchored_interval_for_profile_v1(&profile),
                    "armed, the two spellings are one number"
                );
            }
        }
        // A graph-v2 row: both spellings are the shipped interval at every arming.
        let v2 = palw_a16_context_row_profile_v1(512).expect("projects");
        assert!(!crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&v2));
        for court in [None, Some(kary(16))] {
            assert_eq!(palw_anchored_interval_for_court_v1(&v2, court), palw_checkpoint_interval_v1(512));
        }
        assert_eq!(palw_anchored_interval_for_profile_v1(&v2), palw_checkpoint_interval_v1(512));
    }

    /// **The bottom is inside ONE carrier at every position class, at the ruleset's OWN ladder.**
    ///
    /// The position classes are not a sample: they are every shape the disputed position can have
    /// relative to the map's tile and to the job's phases — the first position of all, a prefill
    /// position, the first decode call, a tile-aligned position, one straddling a tile boundary,
    /// and the last. Under the per-position cadence the bottom is the SAME object at all of them,
    /// which is the property being asserted; before the amendment three of the six had no anchor
    /// at all and the seventh column below (the cache-write route) was the only route they had.
    ///
    /// **The ladder is read, never typed.** A Merkle path is `64 × ⌈log₂ ladder⌉` bytes, so every
    /// number here moves with `PalwCourtParamsV2::max_step_leaf_count`. Both are stated: the RC
    /// bundle's own ladder (`PALW_RC_COURT_MAX_STEP_LEAF_COUNT`), which is what a dispute on the
    /// genesis card is actually filed under, and the fence's
    /// [`PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`], which is what `palw_class_ladder_rules_v1`
    /// provisions the close at.
    #[test]
    fn the_bottom_is_one_carrier_at_every_position_class() {
        use crate::palw_class_admission_v2::{
            PALW_RC_COURT_MAX_STEP_LEAF_COUNT, palw_attn_bottom_bytes_for_cadence_v1, palw_attn_bottom_cache_write_bytes_v1,
            palw_attn_bottom_tile_route_bytes_v1,
        };
        let carrier = palw_close_bytes_for_chunks_v1(1);
        assert_eq!(carrier, 83_333, "one carrier's counted budget moved");

        let mut table: Vec<(&str, u64, u64, u64, u64)> = Vec::new();
        for (name, build) in [("dense", PALW_LADDER_FAMILIES_V5[0]), ("hybrid", PALW_LADDER_FAMILIES_V5[1])] {
            let n_ctx = 512u32;
            let profile = build(n_ctx).expect("projects");
            assert_eq!(palw_checkpoint_cadence_v1(&profile), PalwCheckpointCadenceV1::PerPosition);
            let d_head = profile.attn_head_dim as u64;
            let kv_dim = profile.attn_kv_heads as u64 * d_head;
            let node = profile
                .attn_nodes
                .iter()
                .find(|n| n.op_kind == crate::palw_step::PalwStepOpKindV1::AttnFused)
                .expect("a v5 row has a fused site");
            let out_w = match node.out_len {
                crate::palw_step::PalwStepOutLenV1::Fixed { elements } => elements as u64,
                crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * n_ctx as u64,
            };
            let tile = (node.tile_len as u64).min(out_w);
            let src = profile
                .attn_nodes
                .iter()
                .find(|n| n.role == crate::palw_step::PalwStepNodeRoleV1::KCacheWrite)
                .map_or(tile, |n| n.tile_len as u64);
            let history_tile = (PALW_ATTN_HISTORY_TILE_V4 as u64).min(n_ctx as u64);

            for (ladder_name, ladder) in
                [("RC bundle", PALW_RC_COURT_MAX_STEP_LEAF_COUNT), ("ladder fence", PALW_CONTEXT_LADDER_MAX_STEP_LEAVES)]
            {
                let path = step_path_bytes_v1(ladder);
                assert_eq!(path, 64 * u64::from(ladder.next_power_of_two().trailing_zeros()), "a path is 64 bytes a level");
                let per_position =
                    palw_attn_bottom_bytes_for_cadence_v1(d_head, kv_dim, history_tile, tile, src, path, true).expect("derives");
                let per_call =
                    palw_attn_bottom_bytes_for_cadence_v1(d_head, kv_dim, history_tile, tile, src, path, false).expect("derives");
                assert_eq!(
                    per_position,
                    palw_attn_bottom_tile_route_bytes_v1(d_head, kv_dim, history_tile, tile, path).expect("derives"),
                    "the per-position bottom IS the tile route, with no residue"
                );

                // **Every position class, and the anchor each of them stands on.** The job runs the
                // whole row: `n_ctx − 1` prefill positions and one decode call, so every class below
                // is a coordinate this job actually has.
                let ctx = PalwJobContextV2 {
                    declared_prefill_tokens: n_ctx - 1,
                    exact_decode_tokens: 2,
                    max_context_tokens: n_ctx,
                    ..job()
                };
                let prefill = ctx.declared_prefill_tokens;
                let classes: [(&str, u32, u32); 6] = [
                    ("the first position", 0, 0),
                    ("a prefill position", 0, 5),
                    ("tile-aligned", 0, 16 * 7),
                    ("straddling a tile", 0, 16 * 7 + 9),
                    ("the first decode call", 1, 0),
                    ("the last position", 1, 0),
                ];
                for (what, call, position) in classes {
                    let covered = palw_checkpoint_covered_for_step_v1(&profile, &ctx, call, position)
                        .unwrap_or_else(|| panic!("{name} @ {ladder_name}: {what} has no anchor"));
                    // The anchor holds exactly the rows this step reads — no more, no fewer.
                    let kv_len = if call == 0 { position + 1 } else { prefill + call };
                    assert_eq!(palw_checkpoint_positions_at_v1(&profile, &ctx, covered), kv_len, "{name}: {what}");
                    assert!(covered <= palw_checkpoint_count_v1(&profile, &ctx, 1), "{name}: {what} names a leaf past the leg");
                    // And the bottom at that position is the one object, inside one carrier.
                    assert!(
                        per_position <= carrier,
                        "{name} @ {ladder_name}: {what} files a bottom of {per_position} against a carrier of {carrier}"
                    );
                }

                let cache_write =
                    palw_attn_bottom_cache_write_bytes_v1(d_head, kv_dim, history_tile, tile, src, path).expect("derives");
                assert_eq!(per_call, per_position.max(cache_write), "the per-call price is the larger of the two routes");
                println!(
                    "{name} @ {ladder_name} (ladder 2^{}, path {path}): per-position bottom {per_position}, per-call {per_call}, \
                     cache-write route {cache_write}, carrier {carrier}",
                    ladder.next_power_of_two().trailing_zeros()
                );
                table.push((name, ladder, per_position, per_call, cache_write));
            }
        }

        // **The numbers, pinned at BOTH ladders.** The RC bundle's is what a dispute on the genesis
        // card is filed under; the fence's is what the close is provisioned at.
        let at = |family: &str, ladder: u64| {
            *table.iter().find(|(n, l, ..)| *n == family && *l == ladder).unwrap_or_else(|| panic!("{family} @ {ladder}"))
        };
        let (_, _, dense_rc, dense_rc_call, dense_rc_cache) = at("dense", PALW_RC_COURT_MAX_STEP_LEAF_COUNT);
        let (_, _, dense_fence, _, dense_fence_cache) = at("dense", PALW_CONTEXT_LADDER_MAX_STEP_LEAVES);
        let (_, _, hybrid_rc, _, hybrid_rc_cache) = at("hybrid", PALW_RC_COURT_MAX_STEP_LEAF_COUNT);
        let (_, _, hybrid_fence, _, hybrid_fence_cache) = at("hybrid", PALW_CONTEXT_LADDER_MAX_STEP_LEAVES);
        // Every per-position bottom is one carrier, at every ladder and both families.
        for (family, ladder, bottom, ..) in &table {
            assert!(bottom <= &carrier, "{family} @ ladder {ladder}: a bottom of {bottom} is over one carrier of {carrier}");
        }
        // **The cache-write route is over the carrier at every ladder and both families** — which
        // is the finding the per-position cadence answers, not a claim that route B got cheaper.
        for (family, ladder, _, _, cache_write) in &table {
            assert!(
                cache_write > &carrier,
                "{family} @ ladder {ladder}: the cache-write bottom ({cache_write}) is inside one carrier now — the finding moved"
            );
        }
        assert_eq!(
            (dense_fence, dense_fence_cache, hybrid_fence, hybrid_fence_cache),
            (41_997, 175_297, 75_277, 139_777),
            "the bottom's two routes at the ladder FENCE (2^32, a 32-element path) moved"
        );
        assert_eq!(dense_rc_call, dense_rc_cache, "a per-call class is priced at the route it can file");
        // The RC's ladder is shallower, so every path is shorter and every number is smaller. The
        // ORDER is what matters and it does not move: per-position inside the carrier, cache-write
        // outside it, at both.
        assert!(dense_rc < dense_fence && hybrid_rc < hybrid_fence, "a shallower ladder cannot cost more");
        println!(
            "PINNED: dense per-position {dense_rc} (RC) / {dense_fence} (fence); hybrid {hybrid_rc} / {hybrid_fence}; \
             cache-write dense {dense_rc_cache} / {dense_fence_cache}, hybrid {hybrid_rc_cache} / {hybrid_fence_cache}"
        );
    }

    /// **The one number the 5f genesis card's central claim turns on**: what a dispute at a PREFILL
    /// position on the dense graph-v5 row costs on the CACHE-WRITE route, at the RC bundle's own
    /// ladder.
    ///
    /// That is the route a prefill dispute had before ADR-0082 Decision 4 was amended — the only
    /// one, because a per-call leg commits no checkpoint over the prefill at all — and it is stated
    /// as ONE number rather than as an average because the card's claim is that every position is
    /// prosecutable, not that most are.
    #[test]
    fn the_cache_write_bottom_at_a_prefill_position_is_the_cards_number() {
        use crate::palw_class_admission_v2::{
            PALW_RC_COURT_MAX_STEP_LEAF_COUNT, palw_attn_bottom_cache_write_bytes_v1, palw_attn_bottom_tile_route_bytes_v1,
        };
        let carrier = palw_close_bytes_for_chunks_v1(1);
        let profile = PALW_LADDER_FAMILIES_V5[0](512).expect("projects");
        let d_head = profile.attn_head_dim as u64;
        let kv_dim = profile.attn_kv_heads as u64 * d_head;
        let node = profile
            .attn_nodes
            .iter()
            .find(|n| n.op_kind == crate::palw_step::PalwStepOpKindV1::AttnFused)
            .expect("a fused site");
        let tile = node.tile_len as u64;
        let src = profile
            .attn_nodes
            .iter()
            .find(|n| n.role == crate::palw_step::PalwStepNodeRoleV1::KCacheWrite)
            .map_or(tile, |n| n.tile_len as u64);
        let history_tile = PALW_ATTN_HISTORY_TILE_V4 as u64;
        let path = step_path_bytes_v1(PALW_RC_COURT_MAX_STEP_LEAF_COUNT);

        let cache_write = palw_attn_bottom_cache_write_bytes_v1(d_head, kv_dim, history_tile, tile, src, path).expect("derives");
        let per_position = palw_attn_bottom_tile_route_bytes_v1(d_head, kv_dim, history_tile, tile, path).expect("derives");
        println!(
            "THE CARD'S NUMBER: a prefill dispute on the dense graph-v5 row at the RC ladder (2^{}, path {path} bytes) costs \
             {cache_write} bytes on the CACHE-WRITE route against a carrier of {carrier} — {:.2} carriers; with a checkpoint at \
             every position it is {per_position}, one carrier, at that position and at every other.",
            PALW_RC_COURT_MAX_STEP_LEAF_COUNT.next_power_of_two().trailing_zeros(),
            cache_write as f64 / carrier as f64
        );
        assert!(cache_write > carrier, "the cache-write route at a prefill position is over one carrier — the card's premise moved");
        assert!(per_position <= carrier, "the checkpoint route at a prefill position is NOT one carrier — say so plainly");
        // **And at the 2^26 ladder the release branch raises the RC bundle to** — derived here so
        // the card's number is stated for the ruleset that ships whichever of the two lands first,
        // and so this test says the same thing before and after that merge.
        for ladder in [1u64 << 22, 1 << 26, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES] {
            let p = step_path_bytes_v1(ladder);
            let c = palw_attn_bottom_cache_write_bytes_v1(d_head, kv_dim, history_tile, tile, src, p).expect("derives");
            let t = palw_attn_bottom_tile_route_bytes_v1(d_head, kv_dim, history_tile, tile, p).expect("derives");
            println!(
                "  at ladder 2^{} (path {p}): cache-write {c} ({:.2} carriers), per-position {t} ({:.2})",
                ladder.next_power_of_two().trailing_zeros(),
                c as f64 / carrier as f64,
                t as f64 / carrier as f64
            );
            assert!(c > carrier && t <= carrier, "ladder 2^{}", ladder.next_power_of_two().trailing_zeros());
        }
    }

    /// **ADR-0082 Decision 10 restates Decision 14's floor in the unit that earns.**
    ///
    /// Off the fence (every shipped preset) the floor is cached positions and the gate is the one
    /// that ships. On it, a canonical job that clears the floor on its PREFILL alone — the whole
    /// context as prompt and one decode call — is refused, because past `palw_fp_decode_rules` a
    /// claim earns on its decode leaves and such a row would park every honest job at the
    /// 64-quantum cap.
    #[test]
    fn the_footprint_floor_reads_the_unit_that_earns() {
        let profile = palw_a16_context_row_profile_v1(512).expect("projects");
        let floor = palw_canonical_footprint_floor_v1(profile.n_ctx);
        assert_eq!(floor, 64, "n_ctx / 8");
        // A job that is all prompt: 500 prefill, 2 decode tokens = ONE decode call.
        let all_prompt = PalwJobContextV2 {
            declared_prefill_tokens: 500,
            exact_decode_tokens: 2,
            max_context_tokens: 512,
            ..job()
        };
        assert_eq!(palw_job_footprint_v1(500, 2), 501, "it clears the positions floor eight times over");
        assert_eq!(palw_job_decode_footprint_v1(2), 1, "on one decode call");
        assert!(palw_footprint_meets_the_row_for_rules_v1(&profile, &all_prompt, false), "the shipped gate admits it");
        assert!(!palw_footprint_meets_the_row_for_rules_v1(&profile, &all_prompt, true), "the decode-rules gate refuses it");
        // A job whose ANSWER meets the floor passes both.
        let real = PalwJobContextV2 { declared_prefill_tokens: 8, exact_decode_tokens: 65, ..all_prompt.clone() };
        assert_eq!(palw_job_decode_footprint_v1(65), 64);
        for rules in [false, true] {
            assert!(palw_footprint_meets_the_row_for_rules_v1(&profile, &real, rules), "decode_rules {rules}");
        }
        // Off the fence the two forms are the same function, which is what makes every shipped
        // preset unchanged.
        for (prefill, decode) in [(7u32, 5u32), (500, 2), (8, 65)] {
            let c = PalwJobContextV2 { declared_prefill_tokens: prefill, exact_decode_tokens: decode, ..all_prompt.clone() };
            assert_eq!(
                palw_footprint_meets_the_row_for_rules_v1(&profile, &c, false),
                palw_footprint_meets_the_row_v1(&profile, &c),
                "off the fence the restated floor IS the shipped one"
            );
        }
    }

    /// **Z3, the bytes half.** The bottom of a fused dispute, both routes, at every registered
    /// `d_head` and `tile_len`, against one carrier; and the widest move at every legal arity.
    ///
    /// **The tile term is `kv_dim`-wide and not `d_head`-wide.** ADR-0082 §4 sizes it as
    /// `2 x 16 x 4 x d_head` — one HEAD's slice — and a checkpoint chunk cannot be narrowed to a
    /// head: the map addresses `(kind, layer, position)` and a chunk holds the whole cache row
    /// (`palw_attn_court_v1` asserts `chunk_bytes.len() == TILE x kv_dim x 4` on its own object).
    /// Both registered families carry `attn_kv_heads` 2, so the term is twice the ADR's and the
    /// numbers below are the corrected ones.
    #[test]
    fn the_bottom_opening_and_every_move_fit_one_carrier() {
        use crate::palw_class_admission_v2::{
            palw_attn_bottom_bytes_v1, palw_attn_bottom_cache_write_bytes_v1, palw_attn_bottom_tile_route_bytes_v1,
        };
        let carrier = palw_close_bytes_for_chunks_v1(1);
        assert_eq!(carrier, 83_333, "one carrier's counted budget moved");
        // The ladder's own path depth — 32 elements at `PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`.
        let path = 64 * 32u64;
        let mut measured: Vec<(&str, u64, u64, u64)> = Vec::new();
        for (name, build) in [("dense", PALW_LADDER_FAMILIES_V5[0]), ("hybrid", PALW_LADDER_FAMILIES_V5[1])] {
            let mut per_family: Vec<(u64, u64)> = Vec::new();
            for n_ctx in [512u32, 4_096, 32_768] {
                let Ok(profile) = build(n_ctx) else { continue };
                let d_head = profile.attn_head_dim as u64;
                let kv_dim = profile.attn_kv_heads as u64 * d_head;
                let node = profile
                    .attn_nodes
                    .iter()
                    .find(|n| n.op_kind == crate::palw_step::PalwStepOpKindV1::AttnFused)
                    .expect("a v5 row has a fused site");
                let out_w = match node.out_len {
                    crate::palw_step::PalwStepOutLenV1::Fixed { elements } => elements as u64,
                    crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * n_ctx as u64,
                };
                let tile = (node.tile_len as u64).min(out_w);
                let src = profile
                    .attn_nodes
                    .iter()
                    .find(|n| n.role == crate::palw_step::PalwStepNodeRoleV1::KCacheWrite)
                    .map_or(tile, |n| n.tile_len as u64);
                let positions = (PALW_ATTN_HISTORY_TILE_V4 as u64).min(n_ctx as u64);
                let a = palw_attn_bottom_tile_route_bytes_v1(d_head, kv_dim, positions, tile, path).expect("derives");
                let b = palw_attn_bottom_cache_write_bytes_v1(d_head, kv_dim, positions, tile, src, path).expect("derives");
                assert_eq!(palw_attn_bottom_bytes_v1(d_head, kv_dim, positions, tile, src, path), Some(a.max(b)));
                println!("{name} @ {n_ctx}: d_head {d_head} kv_dim {kv_dim} tile {tile} src {src} -> tile-route {a}, cache-write {b}");
                per_family.push((a, b));
                // Every legal arity's move rides one carrier at the lanes this row disputes.
                let lanes = tile.min(d_head);
                for arity in [2u8, 4, 8, 16, 32, 64] {
                    assert!(
                        crate::palw_attn_dissect::palw_attn_dissect_arity_fits_carrier_v1(arity, lanes as usize, carrier),
                        "{name} @ {n_ctx}: a round at arity {arity} over {lanes} lanes is over one carrier"
                    );
                }
            }
            // FLAT: the same numbers at every context, which is Decisions 1-4's whole claim.
            assert!(per_family.windows(2).all(|w| w[0] == w[1]), "{name}: the bottom is not flat in the context: {per_family:?}");
            measured.push((name, per_family[0].0, per_family[0].1, carrier));
        }
        // The corrected numbers, pinned.
        assert_eq!((measured[0].1, measured[0].2), (41_997, 175_297), "the dense bottom's two routes moved");
        assert_eq!((measured[1].1, measured[1].2), (75_277, 139_777), "the hybrid bottom's two routes moved");
        // **The finding.** The tile route is inside one carrier on both families; the cache-write
        // route is not, on either. So `state_chunk_opening_root_v1` is load-bearing rather than an
        // optimisation, and a class admitted while only the cache-write route exists is admitted at
        // a bottom no single carrier files.
        for (name, tile_route, cache_write, carrier) in &measured {
            assert!(tile_route <= carrier, "{name}: the tile route's bottom is {tile_route} against a carrier of {carrier}");
            assert!(
                cache_write > carrier,
                "{name}: the cache-write bottom ({cache_write}) is inside one carrier now — re-read this test, the finding moved"
            );
        }

        // And the derivation BOUNDS the objects stream E's court files, at the same path depth and
        // the same geometry their fixtures use (`kv_heads` 1, so `kv_dim == d_head`): cache-write
        // 37,985 / 55,393 and checkpoint 19,027 / 36,435.
        for (d_head, cache, ckpt) in [(128u64, 37_985u64, 19_027u64), (256, 55_393, 36_435)] {
            let tile = PALW_ATTN_HISTORY_TILE_V4 as u64;
            let derived_cache = palw_attn_bottom_cache_write_bytes_v1(d_head, d_head, tile, d_head, d_head, 8 * 64).expect("derives");
            let derived_ckpt = palw_attn_bottom_tile_route_bytes_v1(d_head, d_head, tile, d_head, 8 * 64).expect("derives");
            println!(
                "E @ d_head {d_head}: cache-write derived {derived_cache} >= {cache}; checkpoint derived {derived_ckpt} >= {ckpt}"
            );
            assert!(
                derived_cache >= cache,
                "d_head {d_head}: the cache-write derivation ({derived_cache}) is BELOW the object the court files ({cache})"
            );
            assert!(
                derived_ckpt >= ckpt,
                "d_head {d_head}: the checkpoint derivation ({derived_ckpt}) is BELOW the object the court files ({ckpt})"
            );
        }
    }

    /// **The derived bottom bounds the REAL object at `kv_heads` 2** — the geometry both registered
    /// families carry, and the one stream E's fixtures (`kv_heads` 1) cannot exhibit.
    ///
    /// Built from `palw_attn_court_v1`'s own public wire types rather than from a size formula, so
    /// what is measured is borsh's answer about the object a challenger files and not a second
    /// opinion about it.
    #[test]
    fn the_derived_bottom_bounds_the_real_bottom_object() {
        use crate::Hash64;
        use crate::palw_attn_court_v1::{
            PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnChunkOpeningV1, PalwAttnDissectBottomV1, PalwAttnRowOpeningV1,
            PalwAttnTileEvidenceV1,
        };
        use crate::palw_class_admission_v2::{palw_attn_bottom_cache_write_bytes_v1, palw_attn_bottom_tile_route_bytes_v1};
        use crate::palw_step::PalwStepCoordinateV1;
        use crate::palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1};

        let h = |w: u64| Hash64::from_u64_word(w);
        let depth = 32usize; // the `2^32` ladder's path
        let opening = |lanes: u64| PalwAttnRowOpeningV1 {
            leaf: PalwStepTileLeafV1 {
                version: 1,
                coord: PalwStepCoordinateV1 { call_index: 0, node_slot: 7, position: 3, tile_index: 0 },
                value_count: lanes as u32,
                values_le: vec![0u8; lanes as usize * 4],
            },
            opening: PalwStepOpeningV1 { leaf_index: 11, leaf_hash: h(1), siblings: (0..depth as u64).map(h).collect() },
        };
        let tile = PALW_ATTN_HISTORY_TILE_V4 as u64;
        for (name, d_head, kv_heads, out_lanes, src_tile) in [("dense", 128u64, 2u64, 8u64, 128u64), ("hybrid", 256, 2, 8, 512)] {
            let kv_dim = d_head * kv_heads;
            // Route B, the cache-write bottom: one opening per committed TILE of every row.
            let rows = |lanes: u64| -> Vec<PalwAttnRowOpeningV1> {
                let leaves = lanes.div_ceil(src_tile).max(1);
                (0..leaves).map(|_| opening(lanes / leaves)).collect()
            };
            let cache_rows: Vec<PalwAttnRowOpeningV1> = (0..tile).flat_map(|_| rows(kv_dim)).collect();
            let cache_bottom = PalwAttnDissectBottomV1 {
                version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                session_id: h(9),
                tile: 1,
                query: opening(d_head),
                anchor: None,
                k: PalwAttnTileEvidenceV1::CacheWrites { rows: cache_rows.clone() },
                v: PalwAttnTileEvidenceV1::CacheWrites { rows: cache_rows },
                out_tile: opening(out_lanes),
            };
            let cache_measured = borsh::to_vec(&cache_bottom).expect("serializes").len() as u64;
            let cache_derived =
                palw_attn_bottom_cache_write_bytes_v1(d_head, kv_dim, tile, out_lanes, src_tile, 64 * depth as u64).expect("derives");

            // Route A, the checkpoint bottom: ONE chunk opening per kind, `tile x kv_dim x 4` bytes.
            let chunk = || PalwAttnChunkOpeningV1 {
                chunk_index: 0,
                chunk_bytes: vec![0u8; (tile * kv_dim * 4) as usize],
                siblings: (0..depth as u64).map(h).collect(),
            };
            let ckpt_bottom = PalwAttnDissectBottomV1 {
                version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                session_id: h(9),
                tile: 1,
                query: opening(d_head),
                anchor: None,
                k: PalwAttnTileEvidenceV1::Checkpoint { chunk: chunk(), rows_after: Vec::new() },
                v: PalwAttnTileEvidenceV1::Checkpoint { chunk: chunk(), rows_after: Vec::new() },
                out_tile: opening(out_lanes),
            };
            let ckpt_measured = borsh::to_vec(&ckpt_bottom).expect("serializes").len() as u64;
            let ckpt_derived =
                palw_attn_bottom_tile_route_bytes_v1(d_head, kv_dim, tile, out_lanes, 64 * depth as u64).expect("derives");

            println!(
                "{name} kv_heads {kv_heads} d_head {d_head}: cache-write derived {cache_derived} >= measured {cache_measured}; \
                 checkpoint derived {ckpt_derived} >= measured {ckpt_measured}"
            );
            assert!(
                cache_derived >= cache_measured,
                "{name}: the cache-write derivation ({cache_derived}) is BELOW the object ({cache_measured})"
            );
            assert!(
                ckpt_derived >= ckpt_measured,
                "{name}: the checkpoint derivation ({ckpt_derived}) is BELOW the object ({ckpt_measured})"
            );
            // The anchor a real checkpoint bottom also carries is NOT counted above and is the one
            // term this derivation leaves to the `kv_checkpoint_bytes` charge the walk adds
            // separately; the object here carries `anchor: None` so the two do not double-count.
            assert!(ckpt_measured < cache_measured, "{name}: the tile route must be the cheaper one");
        }
    }

    /// **Z0's second half.** A graph-v5 class's close is independent of `n_ctx` except for the
    /// prompt-id term — swept at 512, 4,096, 32,768 and 131,072 on both families, under both id
    /// forms, with the ruleset's own form named.
    ///
    /// **It holds to 4,096 and not beyond, and the term that breaks it is not attention.** Past
    /// that width the binding node becomes the GATHER, whose evidence carries the generated token
    /// ids (`n_ctx x 4`) and the decode pin — a SECOND context-linear term, which ADR-0082
    /// Decision 5 does not anchor because Decision 5 anchors the PROMPT ids. The attention terms
    /// Decisions 1-4 flatten stay flat at every width, which is what the per-node reading below
    /// separates out.
    #[test]
    fn a_graph_v5_close_is_flat_in_the_context() {
        for (name, build) in [("dense", PALW_LADDER_FAMILIES_V5[0]), ("hybrid", PALW_LADDER_FAMILIES_V5[1])] {
            for form in [PalwPromptIdsFormV1::MerkleV1, PalwPromptIdsFormV1::Flat] {
                // Per width: the whole close, the FUSED site's close, and the id term.
                let mut seen: Vec<(u32, u64, u64, u64)> = Vec::new();
                for n_ctx in [512u32, 4_096, 32_768, 131_072] {
                    let Ok(profile) = build(n_ctx) else { continue };
                    let court = PalwKaryCourtV1 { prompt_ids_form: form, ..kary(16) };
                    let Some(rules) = palw_class_ladder_rules_for_court_v1(&profile, Some(court)) else { continue };
                    let Ok(rows) = derive_court_cost_rows_v1(&profile, rules.cost_shape) else { continue };
                    let whole = rows.first().expect("a non-empty walk").close_bytes;
                    let fused = rows
                        .iter()
                        .find(|r| r.op_kind == crate::palw_step::PalwStepOpKindV1::AttnFused)
                        .expect("a v5 row has a fused site")
                        .close_bytes;
                    let ids = crate::palw_prompt_ids_v1::prompt_ids_close_bytes_v1(form, n_ctx as u64).expect("prices");
                    println!("{name} {form:?} @ {n_ctx}: close {whole}, fused site {fused}, id term {ids}");
                    seen.push((n_ctx, whole, fused, ids));
                }
                assert!(seen.len() >= 2, "{name} {form:?}: nothing swept");
                // **The FUSED SITE is flat at every width, in both forms**: its close moves by
                // exactly the id term and by nothing else. This is Decisions 1-4's whole claim,
                // isolated from the terms they do not touch.
                for pair in seen.windows(2) {
                    let (a, b) = (&pair[0], &pair[1]);
                    assert_eq!(
                        i128::from(b.2) - i128::from(a.2),
                        i128::from(b.3) - i128::from(a.3),
                        "{name} {form:?}: the fused site's close moved by something other than the id term between \
                         n_ctx {} and {}",
                        a.0,
                        b.0
                    );
                }
                // And the WHOLE close is flat only while the fused site binds it. Where it stops
                // being flat, the node that broke it is named — never waved at.
                for pair in seen.windows(2) {
                    let (a, b) = (&pair[0], &pair[1]);
                    let by_ids = i128::from(b.1) - i128::from(a.1) == i128::from(b.3) - i128::from(a.3);
                    if !by_ids {
                        let profile = build(b.0).expect("projects");
                        let court = PalwKaryCourtV1 { prompt_ids_form: form, ..kary(16) };
                        let rules = palw_class_ladder_rules_for_court_v1(&profile, Some(court)).expect("mapped");
                        let rows = derive_court_cost_rows_v1(&profile, rules.cost_shape).expect("derives");
                        let binding = rows.first().expect("non-empty");
                        assert_eq!(
                            binding.op_kind,
                            crate::palw_step::PalwStepOpKindV1::EmbedLookup,
                            "{name} {form:?}: the close stopped being flat at n_ctx {} on {}[{}] {:?} — if that is not the \
                             gather's generated-id term, ADR-0082 Decisions 1-5 have a second unanchored context term",
                            b.0,
                            binding.table,
                            binding.index,
                            binding.op_kind
                        );
                        println!("{name} {form:?}: flatness ends at n_ctx {} — the gather's generated ids, not attention", b.0);
                    }
                }
            }
        }
    }

    /// **Which id form the ruleset selects, and what each costs.** Z0 allows exactly one growing
    /// term and Decision 5 says which: the Merkle opening, `⌈log₂⌉`-shaped. The flat form is
    /// `n_ctx x 4` and is what every shipped preset still reads, so both are stated rather than one
    /// standing in for the other.
    #[test]
    fn the_id_term_the_ruleset_selects_is_the_merkle_one() {
        let terms = |form: PalwPromptIdsFormV1| -> Vec<u64> {
            [512u64, 4_096, 32_768, 131_072]
                .iter()
                .map(|n| crate::palw_prompt_ids_v1::prompt_ids_close_bytes_v1(form, *n).expect("prices"))
                .collect()
        };
        assert_eq!(terms(PalwPromptIdsFormV1::Flat), vec![2_048, 16_384, 131_072, 524_288], "the flat term is n_ctx x 4");
        assert_eq!(terms(PalwPromptIdsFormV1::MerkleV1), vec![472, 664, 856, 984], "one 64-byte element more per doubling");
        // Decision 5's form is the one a `palw_kary_court`-armed ruleset carries into the shape.
        let dense = PALW_LADDER_FAMILIES_V5[0](512).expect("projects");
        assert_eq!(
            palw_class_ladder_rules_for_court_v1(&dense, Some(kary(16))).expect("mapped").cost_shape.prompt_ids_form,
            PalwPromptIdsFormV1::MerkleV1
        );
        // And with no court the shape is the shipped one, unchanged.
        assert_eq!(palw_class_ladder_rules_v1(&dense).expect("mapped").cost_shape.prompt_ids_form, PalwPromptIdsFormV1::Flat);
    }

    /// **The arity the RC's own window derives, and the moves it buys** (ADR-0082 Decision 3).
    ///
    /// Nothing here is chosen: `palw_court_arity_v1` walks upward from binary until the clock
    /// admits the pair of searches, and refuses an arity whose round no carrier holds.
    #[test]
    fn the_rc_derives_its_own_arity_and_the_moves_it_buys() {
        let windows = PALW_RC_WINDOWS_V1;
        let deadline = palw_court_turn_deadline_v1(
            windows.window_court,
            PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            PALW_CONTEXT_LADDER_TERMINAL_MOVES,
            DEFAULT_MAX_CLOSE_CHUNKS,
        )
        .expect("the RC window admits a move clock");
        let arity = crate::palw_mode_v2::palw_court_arity_v1(
            windows.window_court,
            deadline,
            PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            131_072,
            PALW_ATTN_HISTORY_TILE_V4,
            PALW_CONTEXT_LADDER_TERMINAL_MOVES,
            crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4 as usize * 16,
        );
        println!("RC: window_court {} turn_deadline {deadline} -> arity {arity:?}", windows.window_court);
        let arity = arity.expect("the RC window admits some arity");
        let court = crate::palw_mode_v2::PalwCourtParamsV2::new(PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, deadline, 2)
            .expect("a court")
            .with_dissection_arity(arity)
            .expect("legal");
        let worst = court.worst_case_duration_with_history_daa(131_072, PALW_ATTN_HISTORY_TILE_V4).expect("derives");
        println!(
            "RC: ladder rounds {} history rounds {:?} worst {worst} DAA",
            court.bisection_rounds(),
            court.history_dissection_rounds(131_072, PALW_ATTN_HISTORY_TILE_V4)
        );
        assert!(worst + palw_close_assembly_daa_v1(DEFAULT_MAX_CLOSE_CHUNKS) < windows.window_court, "the derived arity overruns");
    }
}
