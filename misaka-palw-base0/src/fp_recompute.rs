//! **ADR-0082 Decision 9: a seat recomputes the cache from the prompt it holds; it never fetches
//! the history.**
//!
//! # What this replaces, and the number that forced it
//!
//! ADR-0077 Decision 8 has the seat ask the executor for "the checkpoint chunk at the interval's
//! start" ([`crate::fp_interval`]). For an attention family that chunk IS the history:
//! `positions × 2 caches × kv_dim × 4 bytes × layers` — 7.5 GB on the dense tier at 131,072
//! positions, 5.4 GB on the hybrid's ten attention layers. A seat whose bytes are the history is
//! the seat ADR-0077 R1 and W10 forbid, and no chunking of the map repairs it: chunking changes
//! how finely the history is named, never how much of it travels.
//!
//! The seat already holds everything the state is a function of. The prompt ids ride on the
//! accepted 0x4a payload (under `PublicDa`) or in the served material (under `PanelDa`); the
//! output ids are committed under the decode pin. So the seat RUNS the job — the prefill and the
//! decode calls up to the interval's start, teacher-forced with the committed ids — and compares
//! the 64-byte state root it computes against the checkpoint root the executor committed. Bytes
//! for the state: none. Compute: one forward pass of the job, which is Ambient's validator cost
//! (ADR-0026 §1.7) with Ambient's tolerance replaced by equality.
//!
//! # Teacher-forced, and why that is not trusting the executor
//!
//! The ids fed here are the executor's committed output ids, and the seat does not select tokens
//! of its own. That is not a concession: the ids are bound to the claim (the decode pin over
//! `tiled_logits_outer_root_v1`, and the prompt hash inside the job context), and a lie in any of
//! them lands the recompute in a different state whose root does not match the committed
//! checkpoint — which is exactly the comparison this module exists to make. What the seat must
//! never do is take the STATE from the executor, and it never does: every byte of the state
//! returned here was computed by the seat's own kernels.
//!
//! # One spelling of the commitment
//!
//! The root is not hashed here. The chunks go through the same two consensus functions the
//! executor's checkpoint leg uses — `state_chunk_leaf_hash_v1` under the class's declared map id,
//! then `state_chunks_root_v1` — spelled once in
//! [`crate::fp_interval::base0_state_chunks_root_v1`] and shared with the check that authenticates
//! an executor's anchor. `the_seats_root_is_the_executors_leaf_root` pins the two against
//! `Base0CheckpointCaptureV1`, the producer's own path, on a real capture.
//!
//! # What is NOT decided here
//!
//! The composition of a HYBRID class's checkpoint — the attention half's chunks beside the
//! recurrence's — has no committed spelling anywhere in the tree, because the shipped hybrid class
//! registers the checkpoint sentinel and takes no checkpoints at all
//! ([`crate::qwen36_backend::qwen36_checkpoint_profile_v1`]). This module refuses such a class BY
//! NAME rather than inventing an order: an enumeration invented on the seat side would be a second
//! opinion about a consensus object, and the class that registers the composed map is the side
//! that gets to spell it.

use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// Why a seat cannot recompute a class's state. Every arm is a REFUSAL a seat files as
/// `Incapable`, never an accusation: a seat that cannot run the class has learned nothing about
/// the producer (ADR-0082 Decision 9's third consequence, ADR-0075).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base0FpRecomputeError {
    /// The class takes no checkpoints, so it has no committed state root to compare against.
    NoStateChunkMapRegistered,
    /// The ids served are not the ones the job's context binds.
    PromptIdsAreNotTheJobs,
    /// The job declares one prefill length and another was supplied.
    PromptIsNotTheJobsLength { declared: u32, got: usize },
    /// The recompute was asked for more decode calls than the claim executed.
    DecodeCallsBeyondTheJob { asked: u32, job: u32 },
    /// Fewer committed output ids than the calls being teacher-forced need.
    OutputIdsTooShort { need: usize, got: usize },
    /// The state does not fit the map the class declares — the same refusal
    /// `A16Cache::state_chunk_bytes_v1` makes, surfaced with the chunk named.
    StateIsNotTheMaps { chunk_index: u64 },
    /// The class's map has no geometry this family can read.
    Map(String),
    /// The class's kernels refused the run.
    Engine(String),
    /// This family has no state serializer for the map the class registered — the hybrid
    /// composition, which no side of the tree spells yet.
    NoStateSerializer { why: &'static str },
}

impl std::fmt::Display for Base0FpRecomputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoStateChunkMapRegistered => {
                write!(f, "this class registers no state chunk map, so it commits no checkpoint state to recompute")
            }
            Self::PromptIdsAreNotTheJobs => write!(f, "the served ids do not hash to the job's prompt_token_ids_hash"),
            Self::PromptIsNotTheJobsLength { declared, got } => {
                write!(f, "the job declares {declared} prefill tokens and {got} were supplied")
            }
            Self::DecodeCallsBeyondTheJob { asked, job } => {
                write!(f, "the recompute was asked for {asked} decode calls and the claim executed {job}")
            }
            Self::OutputIdsTooShort { need, got } => {
                write!(f, "teacher-forcing {need} calls needs {need} committed output ids and {got} were carried")
            }
            Self::StateIsNotTheMaps { chunk_index } => {
                write!(f, "this cache does not fit the state map the class declares (chunk {chunk_index})")
            }
            Self::Map(why) => write!(f, "the class's state chunk map has no geometry here: {why}"),
            Self::Engine(why) => write!(f, "the class's kernels could not run the job: {why}"),
            Self::NoStateSerializer { why } => write!(f, "this family cannot serialize the state this class commits: {why}"),
        }
    }
}

impl std::error::Error for Base0FpRecomputeError {}

/// **The state a seat computed for itself**, at one checkpoint's covered call.
///
/// `chunks` are the seat's OWN bytes in the class's map order, so an interval replay that resumes
/// from them resumes from arithmetic this node performed — which is the whole of Decision 9. They
/// are never compared against an executor's chunks, because no executor chunk is ever fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base0FpSeatStateV1 {
    /// The decode call this state is the state after — `index × checkpoint_interval` for the
    /// interval a seat drew.
    pub covered_decode_call: u32,
    /// Positions folded into it: `declared_prefill_tokens + covered_decode_call`.
    pub positions: u32,
    /// The committed root of those bytes under the class's map — the 64 bytes that replace the
    /// history on the wire.
    pub state_chunks_root: Hash64,
    pub chunks: Vec<Vec<u8>>,
}

/// **The class's own kernels, as a RECOMPUTE needs them** — a forward with no capture, and the
/// state serializer the class's map names.
///
/// Deliberately not [`crate::fp_interval::Base0FpIntervalKernelsV1`]: that one replays a WINDOW
/// and returns committed leaf hashes, which is `interval` calls of hashing a seat does not want
/// over the whole prefix of the job. This one runs the prefix for its state alone. A family
/// implements both over one engine.
pub trait Base0FpRecomputeKernelsV1 {
    /// One teacher-forced forward at an absolute cache position. The logits are dropped: the seat
    /// does not select tokens, and hashing rows over the prefix is work Decision 9 does not spend.
    fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), Base0FpRecomputeError>;

    /// The class's committed state after the run so far, chunked in the CLASS's own map order.
    ///
    /// `positions` is derived from the job and the covered call, never from the cache — a cache a
    /// row short would otherwise serialize as a shorter state and the shortfall would look like a
    /// job that ran fewer calls (`Base0CheckpointCaptureV1::push`'s rule).
    fn state_chunks(&self, profile: &PalwShapeProfileV3, positions: u32) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError>;
}

/// **Run the job's prefix and return the state a seat holds at `decode_calls`** (Decision 9).
///
/// The walk is the executor's own (`a16_execute_for_attempt_capped_v1`, `qwen36_…`): the prefill
/// over the declared prompt, then decode call `c` consuming the id call `c − 1` produced. Nothing
/// is captured and no token is selected; the ids are the committed ones.
pub fn base0_fp_recompute_state_v1<K: Base0FpRecomputeKernelsV1 + ?Sized>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    output_token_ids: &[u32],
    decode_calls: u32,
    kernels: &mut K,
) -> Result<Base0FpSeatStateV1, Base0FpRecomputeError> {
    // **The ids are an INPUT on this lane and are refused unless they are the job's** — the rule
    // `refutation_for_free_prompt_index` and `base0_open_fp_interval_v1` both state. A seat that
    // recomputed from another list would compare its own honest arithmetic on a different job
    // against this claim's checkpoint and call the producer a liar.
    if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids) != ctx.prompt_token_ids_hash {
        return Err(Base0FpRecomputeError::PromptIdsAreNotTheJobs);
    }
    let prefill = ctx.declared_prefill_tokens as usize;
    if prompt_token_ids.len() != prefill {
        return Err(Base0FpRecomputeError::PromptIsNotTheJobsLength {
            declared: ctx.declared_prefill_tokens,
            got: prompt_token_ids.len(),
        });
    }
    let job_decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    if decode_calls > job_decode_calls {
        return Err(Base0FpRecomputeError::DecodeCallsBeyondTheJob { asked: decode_calls, job: job_decode_calls });
    }
    // Call `c` consumes the id call `c − 1` produced, so `decode_calls` calls need that many ids.
    if (output_token_ids.len() as u64) < decode_calls as u64 {
        return Err(Base0FpRecomputeError::OutputIdsTooShort { need: decode_calls as usize, got: output_token_ids.len() });
    }
    // A class with the checkpoint sentinel commits no state; saying so is the honest `Incapable`
    // rather than serving a root of a leg that does not exist.
    if profile.state_chunk_map_id == Hash64::default() {
        return Err(Base0FpRecomputeError::NoStateChunkMapRegistered);
    }

    for (position, token) in prompt_token_ids.iter().enumerate() {
        kernels.forward_no_capture(*token as usize, position)?;
    }
    for call in 1..=decode_calls {
        let token = output_token_ids[call as usize - 1];
        let position = prefill + call as usize - 1;
        kernels.forward_no_capture(token as usize, position)?;
    }

    let positions = kaspa_consensus_core::palw_state_chunk_map::integer_kv_positions_at_v1(ctx, decode_calls);
    let chunks = kernels.state_chunks(profile, positions)?;
    let state_chunks_root = crate::fp_interval::base0_state_chunks_root_v1(&profile.state_chunk_map_id, &chunks)
        .map_err(|e| Base0FpRecomputeError::Map(e.to_string()))?;
    Ok(Base0FpSeatStateV1 { covered_decode_call: decode_calls, positions, state_chunks_root, chunks })
}

// =================================================================================================
// One forward pass, not two
// =================================================================================================

/// **The last state this process recomputed, keyed by the question it answers.**
///
/// A seat asks twice about the same interval: once for the 64-byte root
/// (`PalwBackend::fp_recompute_checkpoint_root`, the panel's comparison) and once for the replay
/// that resumes from the state. Both are the SAME forward pass, and Decision 9 prices a seat at
/// one — so the second question is answered from here rather than by running the job again.
///
/// One entry, and deliberately: the memo exists to join two calls a panel makes back to back
/// about one duty, not to be a cache of the fleet's claims. The key is every input the state is a
/// function of, so a hit cannot be a state computed for another job, another class or another
/// call — and the value is the seat's own arithmetic either way, never anything received.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SeatStateKeyV1 {
    class_id: Hash64,
    context_hash: Hash64,
    prompt_ids_hash: Hash64,
    output_ids_hash: Hash64,
    decode_calls: u32,
}

static SEAT_STATE_MEMO: std::sync::Mutex<Option<(SeatStateKeyV1, Base0FpSeatStateV1)>> = std::sync::Mutex::new(None);

fn seat_state_key_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    output_token_ids: &[u32],
    decode_calls: u32,
) -> SeatStateKeyV1 {
    SeatStateKeyV1 {
        class_id: profile.shape_profile_id(),
        context_hash: ctx.context_hash(),
        prompt_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids),
        output_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(output_token_ids),
        decode_calls,
    }
}

/// [`base0_fp_recompute_state_v1`], answered from the memo when the same question was just asked.
///
/// `kernels` is built by the caller and only used on a miss, so a family pays for its engine setup
/// once per real recompute.
pub fn base0_fp_seat_state_memoized_v1<K: Base0FpRecomputeKernelsV1 + ?Sized>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    output_token_ids: &[u32],
    decode_calls: u32,
    kernels: &mut K,
) -> Result<Base0FpSeatStateV1, Base0FpRecomputeError> {
    let key = seat_state_key_v1(profile, ctx, prompt_token_ids, output_token_ids, decode_calls);
    if let Ok(guard) = SEAT_STATE_MEMO.lock()
        && let Some((held, state)) = guard.as_ref()
        && *held == key
    {
        return Ok(state.clone());
    }
    let state = base0_fp_recompute_state_v1(profile, ctx, prompt_token_ids, output_token_ids, decode_calls, kernels)?;
    if let Ok(mut guard) = SEAT_STATE_MEMO.lock() {
        *guard = Some((key, state.clone()));
    }
    Ok(state)
}

/// Drop the memo. For tests that measure how many forward passes a sequence costs, and for a node
/// that wants the memory back.
pub fn base0_fp_seat_state_forget_v1() {
    if let Ok(mut guard) = SEAT_STATE_MEMO.lock() {
        *guard = None;
    }
}

// =================================================================================================
// The seat's window bound (Decision 9's first consequence)
// =================================================================================================

/// **How many positions a seat prefills per DAA**, in milli-positions so the quotient survives
/// integer arithmetic — the `rate_seat_prefill` Decision 9 names.
///
/// Derived from a measurement and a cadence, never chosen: `cadence_ms / ms_per_position`, with
/// the cadence the frozen block time the court's own clocks are counted in
/// (`PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`) and `ms_per_position` whatever the measurement
/// reported. `0` for a host that cannot prefill at all, which admits no width.
pub const fn base0_fp_seat_milli_positions_per_daa_v1(ms_per_position: u64) -> u64 {
    if ms_per_position == 0 {
        return u64::MAX;
    }
    (kaspa_consensus_core::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS.saturating_mul(1_000)) / ms_per_position
}

/// **`n_max = window_receipt × rate_seat_prefill`** (Decision 9), in positions.
///
/// The two quantities are the ruleset's receipt window and the measured rate; nothing else enters,
/// and neither number is typed at any call site. A seat that cannot recompute the job inside the
/// window it has to file in files `Incapable` — so a row above this bound is a row nobody seats,
/// and a row nobody seats certifies nothing (ADR-0075).
pub const fn base0_fp_seat_width_bound_v1(window_receipt: u64, milli_positions_per_daa: u64) -> u64 {
    window_receipt.saturating_mul(milli_positions_per_daa) / 1_000
}

/// The milliseconds one prefilled position cost, from a measured wall clock over `positions`.
/// Rounded UP: a rate rounded up is a bound rounded down, and the direction that is safe for a
/// deadline is the one that admits fewer positions.
pub fn base0_fp_seat_ms_per_position_v1(elapsed_ms: u64, positions: u32) -> u64 {
    if positions == 0 {
        return u64::MAX;
    }
    elapsed_ms.div_ceil(positions as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bound is the product of two ruleset quantities and nothing else — the rule BRIEF §7
    /// states, pinned by naming both.
    #[test]
    fn the_width_bound_is_the_window_times_the_rate() {
        let windows = kaspa_consensus_core::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1;
        // A host that prefills a position in one second: the frozen cadence is 120 s, so 120
        // positions per DAA and 600 DAA of receipt window admit 72,000 positions.
        let rate = base0_fp_seat_milli_positions_per_daa_v1(1_000);
        assert_eq!(rate, 120_000, "120 s cadence / 1,000 ms per position, in milli-positions");
        assert_eq!(base0_fp_seat_width_bound_v1(windows.window_receipt, rate), 600 * 120);
        // Ten times slower is ten times fewer positions: the bound tracks the measurement.
        let slow = base0_fp_seat_milli_positions_per_daa_v1(10_000);
        assert_eq!(base0_fp_seat_width_bound_v1(windows.window_receipt, slow), 600 * 12);
    }

    /// A host that cannot prefill at all admits nothing, and the arithmetic says so rather than
    /// dividing by zero.
    #[test]
    fn a_host_that_never_finishes_admits_no_width() {
        assert_eq!(base0_fp_seat_ms_per_position_v1(1_000, 0), u64::MAX);
        assert_eq!(base0_fp_seat_width_bound_v1(600, 0), 0);
    }

    /// The rate rounds in the direction that admits FEWER positions: a measurement of 1.5 ms per
    /// position is charged at 2, never at 1.
    #[test]
    fn the_measured_rate_rounds_against_the_width() {
        assert_eq!(base0_fp_seat_ms_per_position_v1(3, 2), 2);
    }
}
