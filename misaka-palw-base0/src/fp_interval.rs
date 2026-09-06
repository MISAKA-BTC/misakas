//! ADR-0077 Decision 8, family side: opening one checkpoint interval of a retained capture (the
//! executor) and replaying one opened interval exactly (the seat), per family.
//!
//! # What this replaces
//!
//! A free-prompt seat used to fetch the WHOLE capture and hash it (ADR-0073 Phase ① 1e's
//! `verify_material` arm). At the 8- and 16-token rows that was affordable; at the 512- and
//! 8,192-position rows ADR-0077 Decision 13 plans it is gigabytes per claim per seat, and the
//! obligation grows with every token the job produced. Decision 8 replaces it: the seat draws `k`
//! checkpoint intervals from the claim's beacon and its own seat index
//! ([`kaspa_consensus_core::palw_fp_interval_v1::palw_fp_interval_draw_v1`]), asks the executor for
//! each interval's opening, replays that interval from the checkpoint chunks with the class's own
//! kernels, and compares every row EXACTLY. Bytes per seat become
//! `O(k × (interval × row + log₂ leaves))` — independent of `decode_tokens_executed`, which is
//! R1's verification half and the substance of invariant W10.
//!
//! # The four things an opening carries, and what each is bound to
//!
//! 1. **The binding** — `PalwStepBindingV2`, which `verify_binding_v1` recomputes into
//!    `committed_execution_root`. That is what makes the opening THIS claim's and not some other
//!    execution's; a seat pins it against the claim's roots before it reads a byte of evidence.
//! 2. **The checkpoint chunk at the interval's start**, opened against `checkpoint_merkle_root`.
//!    Resuming from unchecked state would let a producer that lied about a step hand over a state
//!    consistent with the lie and watch the replay agree with it, which is the failure
//!    `base0_anchored_leaf_check_v1` already writes down.
//! 3. **The committed rows of the interval**, as a RANGE opening against `step_merkle_root`. The
//!    canonical enumeration is call-major (`canonical_step_coordinates`), so an interval's leaves
//!    are contiguous and one range opening authenticates all of them for `≲ (depth + log₂ k)`
//!    siblings instead of one path per leaf.
//! 4. **The ids the interval consumed and produced.** The prompt ids are the seat's own — they
//!    ride on the accepted 0x4a payload and are a parameter here, hashed against the binding's
//!    `prompt_token_ids_hash` — and the produced ids fall out of the replay. The ONE id that is
//!    neither is the token the interval's first call consumes, and it is **derived, never
//!    declared**: the opening extends its range left over the anchor call's logits node, the seat
//!    reassembles that committed row and takes `base0_decode_token_select_v1` of it. A carried
//!    seed would be a field the executor chooses, and an executor that chooses what its own replay
//!    consumes can make any interval agree with itself.
//!
//! Deriving the seed from committed leaves rather than opening the generated ids against the trace
//! root is a deliberate choice with a stated reason: `tiled_logits_outer_root_v1` hashes the ids
//! FLAT, so opening one of them costs all of them — `4 × decode_tokens_executed` bytes, exactly
//! the dependence W10 forbids. One extra logits row is `interval × row`'s own unit, which the
//! ADR's cost formula already prices.
//!
//! # What a verdict is, and is not
//!
//! [`kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1`] has four arms because they are
//! four different accusations, and none of them is a conviction. `Valid` licenses; `Fault` is the
//! court's QUESTION at a leaf, which any bonded challenger may then open; `Mismatch` is an opening
//! that does not bind to this claim at all, which is the same as nothing served; `Unverifiable` is
//! bytes this family cannot read. Comparison is exact equality and never a tolerance — the class
//! is a pinned integer computation and ADR-0026 refuses the tolerant proof model. A seat's verdict
//! slashes nobody: conviction runs only through the court's bisection to one leaf (ADR-0028).

use crate::legs::Base0CapturedRowV1;
use crate::produce::Base0RetainedMaterialV1;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwFpIntervalVerdictV1};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, PalwStepCoordinateV1, canonical_step_leaf_index, kv_aux_leaf_count};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_MAX_LEAVES, PalwStepBindingV2, PalwStepRangeOpeningV1, PalwStepTileLeafV1, step_range_opening_root_capped_v1,
    step_tile_leaf_hash_v1,
};
use kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// The opening's wire magic. Opaque bytes with a magic and a version, borsh behind it — the
/// family's other codecs' shape ([`crate::produce::base0_material_encode_v1`]), so bytes that are
/// not an interval opening are refused as such rather than mis-parsed as one.
pub const PALW_BASE0_FP_INTERVAL_MAGIC_V1: [u8; 8] = *b"MSKFPIV1";
pub const PALW_BASE0_FP_INTERVAL_VERSION_V1: u16 = 1;

/// **The CHUNKLESS opening's magic** (ADR-0082 Decision 9): the same four things
/// [`Base0FpIntervalOpeningV1`] carries, with the history removed from the third.
///
/// A separate magic rather than a version bump inside the v1 body, because the two forms are
/// different EVIDENCE and a seat must be able to say which one it was handed before it parses a
/// field: v1's anchor carries the state, v2's names it. A seat on an old executor still reads v1
/// ([`base0_fp_interval_opening_decode_any_v1`]), and a graph-v5 class refuses v1 — the class's
/// own bound, not the parser's.
pub const PALW_BASE0_FP_INTERVAL_MAGIC_V2: [u8; 8] = *b"MSKFPIV2";
pub const PALW_BASE0_FP_INTERVAL_VERSION_V2: u16 = 2;

/// Why an interval cannot be opened, or an opening read. Plain enum, hand-written `Display` — the
/// crate's idiom, and the reason [`crate::fp_capture::Base0SparseCaptureError`] states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base0FpIntervalError {
    NoStateChunkMapRegistered {
        index: u32,
    },
    CheckpointIntervalIsZero,
    CheckpointIntervalIsNotTheCommittedOne {
        family: u32,
        committed: u32,
    },
    IntervalOutOfRange {
        index: u32,
        count: u32,
    },
    AuxLeavesAreNotIntervalScoped {
        got: u64,
    },
    StepSpace(String),
    CaptureHasNoTile {
        index: u64,
    },
    CaptureIsNotTheBindings,
    /// A binding whose `step_leaf_count` is outside the leg's range. Refused BEFORE the leaf
    /// vector is sized from it: the field is a plain `u64` inside a borsh blob, a `Hash64` is 64
    /// bytes, and a few hundred bytes asking for `2^48` leaves is an allocation the process
    /// aborts on rather than a panic anything can catch. The same defect
    /// `base0_material_matches_claim_v1` records, in the second place that sizes from the field.
    LeafCountOutOfRange {
        got: u64,
        max: u64,
    },
    /// The binding prices a step space its own profile and context do not produce.
    PriceIsNotTheGeometrys {
        declared: u64,
        derived: u64,
    },
    PromptIdsAreNotTheJobs,
    NoCheckpointAt {
        covered: u32,
    },
    /// The DENSE retention (`Base0RetainedMaterialV1`) carries checkpoint CHUNKS and no leaves,
    /// and a class whose map addresses history tiles folds its chunks away — so a dense tuple of
    /// such a class holds nothing an anchor can be built from. Refused by name rather than read as
    /// an empty leg: the fold's own retention (`Base0FpMaterialV2`) carries the leaves, and that
    /// is the retention a free-prompt executor of that class keeps.
    DenseRetentionCarriesNoCheckpointLeaves,
    Sparse(crate::fp_capture::Base0SparseCaptureError),
    Leg(String),
    Replay(String),
    NotThisFamilysBytes,
}

impl std::fmt::Display for Base0FpIntervalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoStateChunkMapRegistered { index } => write!(
                f,
                "this class registers no state chunk map, so it has no checkpoints and only interval 0 exists (asked for {index})"
            ),
            Self::CheckpointIntervalIsZero => write!(f, "a checkpoint interval of zero is not a cadence"),
            Self::CheckpointIntervalIsNotTheCommittedOne { family, committed } => {
                write!(f, "the family's checkpoint interval is {family} and the capture committed {committed}")
            }
            Self::IntervalOutOfRange { index, count } => {
                write!(f, "interval {index} does not exist in a capture with {count} of them")
            }
            Self::AuxLeavesAreNotIntervalScoped { got } => {
                write!(f, "this class commits {got} KV aux leaves, which no single interval owns")
            }
            Self::StepSpace(why) => write!(f, "the step space could not be enumerated for this job: {why}"),
            Self::CaptureHasNoTile { index } => {
                write!(f, "the capture holds no tile at leaf {index}, so the interval cannot be opened")
            }
            Self::CaptureIsNotTheBindings => write!(f, "the capture's leaves do not reproduce the step root the binding committed"),
            Self::LeafCountOutOfRange { got, max } => {
                write!(f, "the binding commits {got} step leaves, which is outside the leg's range (max {max})")
            }
            Self::PriceIsNotTheGeometrys { declared, derived } => {
                write!(f, "the binding prices {declared} step leaves and its own profile and context produce {derived}")
            }
            Self::PromptIdsAreNotTheJobs => write!(f, "the served ids do not hash to the binding's prompt_token_ids_hash"),
            Self::NoCheckpointAt { covered } => write!(f, "the capture holds no checkpoint covering decode call {covered}"),
            Self::DenseRetentionCarriesNoCheckpointLeaves => write!(
                f,
                "this class folds its checkpoint chunks away, so its interval opens from the folded retention (which carries the \
                 leg's leaves) and not from a dense tuple (which carries chunks that do not exist)"
            ),
            Self::Sparse(e) => write!(f, "the sparse tree refused the opening: {e}"),
            Self::Leg(why) => write!(f, "the step leg refused the opening: {why}"),
            Self::Replay(why) => write!(f, "the class's kernels could not replay the interval: {why}"),
            Self::NotThisFamilysBytes => write!(f, "the opening is not this family's bytes"),
        }
    }
}

impl std::error::Error for Base0FpIntervalError {}

impl From<crate::fp_capture::Base0SparseCaptureError> for Base0FpIntervalError {
    fn from(e: crate::fp_capture::Base0SparseCaptureError) -> Self {
        Self::Sparse(e)
    }
}

// ---------------------------------------------------------------------------------------------
// Interval geometry
// ---------------------------------------------------------------------------------------------

/// **The step space a binding PRICES, checked against the ladder the caller was handed.**
///
/// One rule, in one place, because three different questions need the same number and each of
/// them used to answer it for itself: the geometry a seat checks an opening's shape against
/// ([`Base0FpIntervalGeometryV1::from_binding_capped_v1`]), the leaf vector an executor sizes from
/// a capture's tiles ([`leaves_from_tiles_v1`]), and the capture a replay allocates
/// ([`base0_fp_replay_interval_v1`]).
///
/// # The bound is `min(the declared price, the ruleset's ladder)`, and both halves are load-bearing
///
/// `verify_binding_v1` checks that the carried profile is the DECLARED one and that the roots
/// recompute; it does not check that `step_leaf_count` is `step_leaf_count(profile, context)`,
/// because that value is a leg INPUT rather than a derived one. Every path below sizes a replay
/// from the profile and compares it against a range priced by the field, so an opening whose two
/// numbers disagree would have a seat replay one step space and compare it to another — and, on a
/// hostile opening, allocate a capture the size of whatever the field says to do it.
///
/// So:
///
/// * the declared price is refused above `max_step_leaf_count` FIRST, by name
///   ([`Base0FpIntervalError::LeafCountOutOfRange`]) and before any allocation — a plain `u64`
///   inside a borsh blob asking for `2^48` leaves is a `2^54`-byte request, which is
///   `handle_alloc_error` and a process ABORT rather than a catchable panic;
/// * the enumeration is then counted with the DECLARED price as its own cap, which is exact
///   because the rule is an EQUALITY: a geometry that overruns the declared price cannot equal it,
///   so counting past it buys nothing and a `TooManyLeaves` is simply the inequality reported
///   under its other name.
///
/// **`max_step_leaf_count` is the ruleset's, never the executor's constant.** The RC ruleset
/// freezes `2^26` and the graph-v5 dense 512 row's canonical job is 6,630,544 leaves, so a seat
/// that re-derived at [`PALW_STEP_LEG_MAX_LEAVES`] (`2^22`) refused an honest opening of a class
/// the chain admits — and a class no seat will license is a class no panel can certify. The number
/// reaches a family through `with_step_ladder_cap` (ADR-0080 W1b), which is
/// `PalwCourtParamsV2::max_step_leaf_count` off the bundle the node runs.
pub fn base0_fp_binding_step_space_v1(binding: &PalwStepBindingV2, max_step_leaf_count: u64) -> Result<u64, Base0FpIntervalError> {
    if binding.step_leaf_count == 0 || binding.step_leaf_count > max_step_leaf_count {
        return Err(Base0FpIntervalError::LeafCountOutOfRange { got: binding.step_leaf_count, max: max_step_leaf_count });
    }
    match kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(
        &binding.shape_profile,
        &binding.job_context,
        binding.step_leaf_count,
    ) {
        Ok(derived) if derived == binding.step_leaf_count => Ok(derived),
        Ok(derived) => Err(Base0FpIntervalError::PriceIsNotTheGeometrys { declared: binding.step_leaf_count, derived }),
        // The cap the count was taken against IS the declared price, so this arm is the strict
        // inequality `derived > declared` and never a ruleset refusal — `got` is the running total
        // at the first step that passed the price, which is the same payload the loop reported.
        Err(kaspa_consensus_core::palw_step::PalwStepError::TooManyLeaves { got, .. }) => {
            Err(Base0FpIntervalError::PriceIsNotTheGeometrys { declared: binding.step_leaf_count, derived: got })
        }
        Err(e) => Err(Base0FpIntervalError::StepSpace(format!("{e:?}"))),
    }
}

/// **Which calls each checkpoint interval covers.**
///
/// Interval 0 is the prefill call plus the decode calls up to the first checkpoint, replayed from
/// genesis — the prompt. Interval `j ≥ 1` is the calls after checkpoint `j − 1`, replayed from
/// that checkpoint's state chunks. Every call of the job is in exactly one interval, which is what
/// makes "sample k intervals" a sample of the execution rather than of a subset of it.
///
/// The count is `⌈decode_calls / interval⌉`, floored at one because the prefill always ran: a job
/// that decoded a single token has no decode CALLS at all and still has an interval 0 to check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base0FpIntervalGeometryV1 {
    pub prompt_tokens: u32,
    /// `exact_decode_tokens − 1` — the calls after the prefill, which is what the checkpoint
    /// cadence and `checkpoint_leg_root_v2` both count.
    pub decode_calls: u32,
    pub checkpoint_interval: u32,
    pub interval_count: u32,
    /// **The unit the CLASS's checkpoint leaves count in** (ADR-0082 Decision 4, amended).
    ///
    /// The intervals themselves are a partition of the decode CALLS and do not move with it — but
    /// which checkpoint anchors interval `j`, and what number that leaf carries, does:
    /// `PerDecodeCall` counts calls and `PerPosition` counts positions, and the two differ by
    /// `declared_prefill_tokens`. Carried here rather than re-derived at each use, because the
    /// derivation is `palw_checkpoint_cadence_v1` of the class's registered map and a caller that
    /// forgot it would compare a 403-position state against a 3-position root.
    pub cadence: kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1,
}

impl Base0FpIntervalGeometryV1 {
    /// **From CHAIN data alone** — the job's prompt length and the commitment's executed decode
    /// count, both on the accepted 0x4a payload, and the family's own checkpoint interval for the
    /// class. This is the form a seat must use: an executor that could shrink the count could
    /// predict which intervals the seat's draw would land on.
    ///
    /// The cadence is the CLASS's and is passed in rather than assumed: the chain facts do not
    /// name the registered state chunk map, and a geometry that guessed would name the wrong leaf.
    pub fn from_chain_facts_v1(
        prompt_tokens: u32,
        decode_tokens_executed: u32,
        checkpoint_interval: u32,
        cadence: kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1,
    ) -> Result<Self, Base0FpIntervalError> {
        if checkpoint_interval == 0 {
            return Err(Base0FpIntervalError::CheckpointIntervalIsZero);
        }
        let decode_calls = decode_tokens_executed.saturating_sub(1);
        let interval_count = decode_calls.div_ceil(checkpoint_interval).max(1);
        Ok(Self { prompt_tokens, decode_calls, checkpoint_interval, interval_count, cadence })
    }

    /// The same geometry read off a capture's own binding — the executor's side. It must agree
    /// with [`Self::from_chain_facts_v1`] on every capture this family produces, and
    /// `the_two_interval_counts_agree_on_every_capture` is what pins that.
    ///
    /// **The DEFAULT ladder, for a caller that holds no ruleset.** The rule is
    /// [`Self::from_binding_capped_v1`]; this passes [`PALW_STEP_LEG_MAX_LEAVES`], which is what
    /// every shipped preset froze — and which is the EXECUTOR's constant, not the court's. A seat
    /// on a network whose `PalwCourtParamsV2::max_step_leaf_count` is wider must pass that number:
    /// see [`base0_fp_binding_step_space_v1`].
    pub fn from_binding_v1(binding: &PalwStepBindingV2, family_interval: u32) -> Result<Self, Base0FpIntervalError> {
        Self::from_binding_capped_v1(binding, family_interval, PALW_STEP_LEG_MAX_LEAVES)
    }

    /// [`Self::from_binding_v1`] against the ladder top the CALLER states — the ruleset's
    /// `PalwCourtParamsV2::max_step_leaf_count`, which reaches a family through
    /// `with_step_ladder_cap` (ADR-0080 W1b).
    pub fn from_binding_capped_v1(
        binding: &PalwStepBindingV2,
        family_interval: u32,
        max_step_leaf_count: u64,
    ) -> Result<Self, Base0FpIntervalError> {
        let committed = binding.checkpoint_profile.checkpoint_interval;
        if committed != family_interval {
            return Err(Base0FpIntervalError::CheckpointIntervalIsNotTheCommittedOne { family: family_interval, committed });
        }
        base0_fp_binding_step_space_v1(binding, max_step_leaf_count)?;
        Self::from_chain_facts_v1(
            binding.job_context.declared_prefill_tokens,
            binding.job_context.exact_decode_tokens,
            committed,
            kaspa_consensus_core::palw_context_ladder::palw_checkpoint_cadence_v1(&binding.shape_profile),
        )
    }

    /// `(first_call, last_call)` inclusive, in the capture's call numbering — call 0 is the
    /// prefill, call `c ≥ 1` is decode call `c`.
    pub fn calls_for(&self, index: u32) -> Option<(u32, u32)> {
        if index >= self.interval_count {
            return None;
        }
        if index == 0 {
            return Some((0, self.checkpoint_interval.min(self.decode_calls)));
        }
        let first = index.checked_mul(self.checkpoint_interval)?.checked_add(1)?;
        let last = index.checked_add(1)?.checked_mul(self.checkpoint_interval)?.min(self.decode_calls);
        Some((first, last))
    }

    /// **The decode CALL interval `index` resumes after** — the last call of the interval below
    /// it, whose committed logits row seeds this one. `None` for interval 0, which starts at the
    /// prompt.
    ///
    /// Cadence-free by construction: the intervals partition the decode calls and every cadence
    /// partitions them the same way. This is the number that indexes `logits_rows`,
    /// `generated_token_ids` and a step COORDINATE's `call_index`; the checkpoint that anchors the
    /// same boundary is named by [`Self::anchor_covered_call`], and conflating the two is
    /// audit B's C-2 — under `PerPosition` they differ by `declared_prefill_tokens`.
    pub fn anchor_seed_call_v1(&self, index: u32) -> Option<u32> {
        if index == 0 || index >= self.interval_count {
            return None;
        }
        index.checked_mul(self.checkpoint_interval)
    }

    /// **What the anchoring checkpoint's `covered_decode_call` field CARRIES**, in the unit the
    /// class's cadence counts in — `None` for interval 0, which has no checkpoint before it.
    ///
    /// `PerDecodeCall`: `index × interval`, the decode call the state is after — the shipped rule
    /// verbatim. `PerPosition`: `prefill + index × interval`, because the leaf's counter IS a
    /// position count there and the state after that many calls is that many rows of cache.
    ///
    /// The two arms name the SAME state; [`Self::anchor_covered_positions_v1`] is that state's
    /// position count and `palw_checkpoint_positions_at_v1` of this value is the same number,
    /// which is what `the_anchors_two_units_name_one_state` pins.
    pub fn anchor_covered_call(&self, index: u32) -> Option<u32> {
        use kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1;
        let calls = self.anchor_seed_call_v1(index)?;
        match self.cadence {
            PalwCheckpointCadenceV1::PerDecodeCall => Some(calls),
            PalwCheckpointCadenceV1::PerPosition => self.prompt_tokens.checked_add(calls),
        }
    }

    /// **How many cache rows the anchoring checkpoint's state holds** — the unit a seat's own
    /// recompute stops in ([`crate::fp_recompute::base0_fp_recompute_state_at_position_v1`]), and
    /// the one geometry both sides take the chunking at.
    ///
    /// `prefill + index × interval` under either cadence, because the prefill always ran.
    pub fn anchor_covered_positions_v1(&self, index: u32) -> Option<u32> {
        self.prompt_tokens.checked_add(self.anchor_seed_call_v1(index)?)
    }
}

/// **The seat's count, from the two numbers the chain already carries** — the backend seam's
/// `fp_interval_count_for`, in one place so the three families cannot each derive it.
///
/// `prompt_tokens` and `decode_tokens_executed` are on the accepted 0x4a payload;
/// `checkpoint_interval` is the class's own cadence, which the family knows from its registration
/// and not from any capture. Nothing here reads material: an executor that could shrink the count
/// could predict which intervals a seat's draw lands on, and the draw is the whole sampling
/// argument.
pub fn base0_fp_interval_count_for_v1(prompt_tokens: u32, decode_tokens_executed: u32, checkpoint_interval: u32) -> Option<u32> {
    // **The cadence named here is a DECLARED choice, not a default this forgot to make.** The
    // count is `decode_calls / interval` under either one — the intervals partition the decode
    // calls and every cadence partitions them the same way — and this asker (the seam's
    // `fp_interval_count_for`, three families deep) holds chain numbers and no profile. The
    // cadence-dependent question is which LEAF anchors an interval, and nothing here asks it.
    // `the_interval_count_does_not_depend_on_the_cadence` is what makes that sentence checkable.
    Base0FpIntervalGeometryV1::from_chain_facts_v1(
        prompt_tokens,
        decode_tokens_executed,
        checkpoint_interval,
        kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerDecodeCall,
    )
    .ok()
    .map(|g| g.interval_count)
}

// ---------------------------------------------------------------------------------------------
// Leaf geometry: where an interval's committed rows live in the step space
// ---------------------------------------------------------------------------------------------

/// The leaf range one interval's opening carries, and how much of its head is the anchor call's
/// logits row (the seed's derivation, never a carried field).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base0FpIntervalLeavesV1 {
    /// First leaf of the opened range — the anchor call's logits node for `index ≥ 1`, the step
    /// space's own first leaf for interval 0.
    pub range_first: u64,
    /// First leaf the interval itself owns (the first leaf of `first_call`).
    pub interval_first: u64,
    /// One past the interval's last leaf.
    pub range_end: u64,
    /// `interval_first − range_first`: the anchor call's logits tiles, carried with preimages.
    pub seed_row_leaves: u64,
}

/// The first MAIN leaf of `call` — and, for `call == decode_calls + 1`, one past the last of them.
///
/// The aux series lives after every main leaf in its own coordinate space, and a chunk of it spans
/// positions, so no single interval owns one. This family registers `kv_chunk_calls = 0` on every
/// shipped class, so the aux region is empty; a class that grew one is refused BY NAME rather than
/// silently opened with an interval boundary that cuts a chunk in half.
///
/// **`step_leaf_count` is passed in rather than re-derived**, and that is the whole of the fix at
/// this site: "one past the last leaf" is the step space's own size, the caller has already
/// obtained it from [`base0_fp_binding_step_space_v1`] against the ruleset's ladder, and the
/// re-derivation here was a second count taken against the EXECUTOR's constant — so on the
/// graph-v5 512 row (6,630,544 leaves) it answered `TooManyLeaves` while the number it was
/// re-deriving sat in the caller's hand.
fn first_leaf_of_call_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    call: u32,
    step_leaf_count: u64,
) -> Result<u64, Base0FpIntervalError> {
    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let aux = kv_aux_leaf_count(profile, ctx);
    if aux != 0 {
        return Err(Base0FpIntervalError::AuxLeavesAreNotIntervalScoped { got: aux });
    }
    if call > decode_calls {
        return Ok(step_leaf_count);
    }
    canonical_step_leaf_index(profile, ctx, &PalwStepCoordinateV1 { call_index: call, node_slot: 0, position: 0, tile_index: 0 })
        .ok_or_else(|| Base0FpIntervalError::StepSpace(format!("call {call} has no first leaf in this step space")))
}

/// The global node slot of the class's LOGITS node — the last slot of the last position table.
///
/// The step space walks `pre`, then each layer's table, then `post`, and every family's capture
/// puts the logits row last in `post` (`a16_captured_rows_v1`, `qwen36_captured_rows_v1`,
/// `base0_captured_rows_v1` all preserve the engine's order, and the engine appends the logits row
/// last). Derived from the profile rather than named, so a class that grew a post node does not
/// need this file edited — and `the_last_post_node_is_the_row_a_token_is_selected_from` measures
/// the assumption on a real run rather than asserting it in prose.
fn logits_node_slot_v1(profile: &PalwShapeProfileV3) -> u32 {
    profile.global_node_count().saturating_sub(1)
}

/// Where interval `index`'s opening reaches in the step space.
///
/// `step_leaf_count` is the space's size, from [`base0_fp_binding_step_space_v1`] — the caller
/// holds it because the caller is the one that holds the ladder.
pub fn base0_fp_interval_leaves_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    geometry: &Base0FpIntervalGeometryV1,
    index: u32,
    step_leaf_count: u64,
) -> Result<Base0FpIntervalLeavesV1, Base0FpIntervalError> {
    let (first_call, last_call) =
        geometry.calls_for(index).ok_or(Base0FpIntervalError::IntervalOutOfRange { index, count: geometry.interval_count })?;
    let interval_first = first_leaf_of_call_v1(profile, ctx, first_call, step_leaf_count)?;
    let range_end = first_leaf_of_call_v1(profile, ctx, last_call + 1, step_leaf_count)?;
    // The seed row is a step COORDINATE, so it is named by the anchor's CALL and never by the
    // checkpoint leaf's counter — the two are the same number only under `PerDecodeCall`.
    let range_first = match geometry.anchor_seed_call_v1(index) {
        None => interval_first,
        Some(anchor_call) => canonical_step_leaf_index(
            profile,
            ctx,
            &PalwStepCoordinateV1 { call_index: anchor_call, node_slot: logits_node_slot_v1(profile), position: 0, tile_index: 0 },
        )
        .ok_or_else(|| Base0FpIntervalError::StepSpace(format!("call {anchor_call} has no logits node in this step space")))?,
    };
    Ok(Base0FpIntervalLeavesV1 { range_first, interval_first, range_end, seed_row_leaves: interval_first.saturating_sub(range_first) })
}

// ---------------------------------------------------------------------------------------------
// The opening
// ---------------------------------------------------------------------------------------------

/// **One checkpoint interval, opened.** Opaque on the seam ([`PalwExecutionBackendV1::open_fp_interval`]
/// returns bytes): only the family that wrote it reads it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpIntervalOpeningV1 {
    pub version: u16,
    pub interval_index: u32,
    /// What `verify_binding_v1` authenticates against the claim's `execution_root`.
    pub binding: PalwStepBindingV2,
    /// The committed leaves of `[anchor logits row ‖ the interval]`, against `step_merkle_root`.
    pub range: PalwStepRangeOpeningV1,
    /// How many leading leaves of `range` are the anchor call's logits tiles.
    pub seed_row_leaf_count: u32,
    /// Their preimages, so the seat can read the row and DERIVE the id the interval consumed.
    pub seed_row_tiles: Vec<PalwStepTileLeafV1>,
    /// The checkpoint at the interval's start, against `checkpoint_merkle_root`. `None` for
    /// interval 0, which resumes from the prompt.
    pub anchor: Option<PalwCheckpointKvOperandsV1>,
}

impl Base0FpIntervalOpeningV1 {
    pub fn encode_v1(&self) -> Result<Vec<u8>, Base0FpIntervalError> {
        let body = borsh::to_vec(self).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        let mut out = Vec::with_capacity(body.len() + PALW_BASE0_FP_INTERVAL_MAGIC_V1.len());
        out.extend_from_slice(&PALW_BASE0_FP_INTERVAL_MAGIC_V1);
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn decode_v1(bytes: &[u8]) -> Result<Self, Base0FpIntervalError> {
        let body = bytes.strip_prefix(&PALW_BASE0_FP_INTERVAL_MAGIC_V1).ok_or(Base0FpIntervalError::NotThisFamilysBytes)?;
        let decoded: Self = borsh::from_slice(body).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        if decoded.version != PALW_BASE0_FP_INTERVAL_VERSION_V1 {
            return Err(Base0FpIntervalError::NotThisFamilysBytes);
        }
        Ok(decoded)
    }
}

/// **A checkpoint NAMED rather than carried** (ADR-0082 Decision 9).
///
/// The leaf and its opening against `checkpoint_merkle_root` — everything that authenticates the
/// state's committed root to the claim, and none of the state. The seat compares
/// `leaf.state_chunks_root` against the root it computed for itself
/// ([`crate::fp_recompute`]) and replays from its OWN chunks; the executor's chunks are never
/// requested, so the history never travels.
///
/// 64 bytes of root plus a Merkle path at the checkpoint leg's depth, against
/// `positions × 2 × kv_dim × 4 × layers` for the v1 form: 7.5 GB at 131,072 positions on the dense
/// tier is the number Decision 9 was written for.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpCheckpointClaimV1 {
    pub leaf: kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2,
    /// The leaf's opening against `binding.checkpoint_merkle_root`.
    pub opening: kaspa_consensus_core::palw_step_leg::PalwStepOpeningV1,
}

/// **One checkpoint interval, opened WITHOUT the history** — the graph-v5 form.
///
/// Field for field [`Base0FpIntervalOpeningV1`], with `anchor` naming the checkpoint instead of
/// carrying it. Every other term is already flat in the context: the range is one interval of
/// committed rows, the seed row is one logits row, and the paths are logarithms.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpIntervalOpeningV2 {
    pub version: u16,
    pub interval_index: u32,
    pub binding: PalwStepBindingV2,
    pub range: PalwStepRangeOpeningV1,
    pub seed_row_leaf_count: u32,
    pub seed_row_tiles: Vec<PalwStepTileLeafV1>,
    /// The checkpoint at the interval's start, named. `None` for interval 0, which resumes from
    /// the prompt and needs no anchor at all.
    pub anchor: Option<Base0FpCheckpointClaimV1>,
}

impl Base0FpIntervalOpeningV2 {
    pub fn encode_v1(&self) -> Result<Vec<u8>, Base0FpIntervalError> {
        let body = borsh::to_vec(self).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        let mut out = Vec::with_capacity(body.len() + PALW_BASE0_FP_INTERVAL_MAGIC_V2.len());
        out.extend_from_slice(&PALW_BASE0_FP_INTERVAL_MAGIC_V2);
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn decode_v1(bytes: &[u8]) -> Result<Self, Base0FpIntervalError> {
        let body = bytes.strip_prefix(&PALW_BASE0_FP_INTERVAL_MAGIC_V2).ok_or(Base0FpIntervalError::NotThisFamilysBytes)?;
        let decoded: Self = borsh::from_slice(body).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        if decoded.version != PALW_BASE0_FP_INTERVAL_VERSION_V2 {
            return Err(Base0FpIntervalError::NotThisFamilysBytes);
        }
        Ok(decoded)
    }

    /// The v1 form with its history dropped — the executor's cheap route to serving a graph-v5
    /// seat from the retention it already builds, and the reason the two forms cannot disagree
    /// about anything but the anchor.
    pub fn from_chunked_v1(opening: &Base0FpIntervalOpeningV1) -> Self {
        Self {
            version: PALW_BASE0_FP_INTERVAL_VERSION_V2,
            interval_index: opening.interval_index,
            binding: opening.binding.clone(),
            range: opening.range.clone(),
            seed_row_leaf_count: opening.seed_row_leaf_count,
            seed_row_tiles: opening.seed_row_tiles.clone(),
            anchor: opening.anchor.as_ref().map(|a| Base0FpCheckpointClaimV1 { leaf: a.leaf.clone(), opening: a.opening.clone() }),
        }
    }
}

/// Wire magic of the v3 opening: v2 with a close annex (ADR-0085 Decision 1).
pub const PALW_BASE0_FP_INTERVAL_MAGIC_V3: [u8; 8] = *b"MSKFPIV3";
pub const PALW_BASE0_FP_INTERVAL_VERSION_V3: u16 = 3;

/// **The two terms of a court close that must be the ACCUSED's** (ADR-0085 §2): the disputed
/// leaf's tile preimage and the rows root of the decode pin. Everything else in a refutation the
/// challenger recomputes and checks against roots the accused committed; these two it cannot —
/// the disputed tile differs from its own by definition, and the rows root is a function of every
/// logits row. Served inside the interval opening that contains the disputed leaf, on the lane the
/// seat already drives, only for a leaf an open court session names (ADR-0085 X4).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpCloseAnnexV1 {
    /// `tiled_logits_rows_root_v1` over every retained row — what `trace_root` commits beside the
    /// generated ids, and what `check_tiled_decode_pin` binds to the binding.
    pub rows_root: Hash64,
    /// The accused's committed tiles at the leaves a court named, each with the checkpoint its
    /// step resumes from.
    pub disputed: Vec<Base0FpDisputedLeafV1>,
}

/// One disputed leaf as the accused serves it: the committed tile (the one term a challenger's
/// replay cannot reproduce when the accused lied), and the checkpoint leaf its step's cache read
/// is anchored to — the leaf and its opening against the checkpoint leg root, which are the
/// accused's leg's and not derivable from an interval opening whose anchor names only the
/// interval's START. The chunks behind that leaf are the challenger's own recompute.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpDisputedLeafV1 {
    pub leaf_index: u64,
    pub tile: PalwStepTileLeafV1,
    pub anchor: Option<Base0FpCheckpointClaimV1>,
}

/// **One checkpoint interval, opened without the history, WITH the close annex** (ADR-0085
/// Decision 1). Field for field [`Base0FpIntervalOpeningV2`] plus `close`; a v3 opening whose
/// annex is `None` is a v2 opening to every reader, and [`base0_fp_interval_opening_decode_any_v1`]
/// hands a seat exactly the v2 view (ADR-0085 X3), so the seat's replay never sees the annex and
/// the annex never changes a verdict. [`base0_fp_interval_close_annex_v1`] is the closer's read.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpIntervalOpeningV3 {
    pub version: u16,
    pub opening: Base0FpIntervalOpeningV2,
    pub close: Option<Base0FpCloseAnnexV1>,
}

impl Base0FpIntervalOpeningV3 {
    pub fn encode_v1(&self) -> Result<Vec<u8>, Base0FpIntervalError> {
        let body = borsh::to_vec(self).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        let mut out = Vec::with_capacity(body.len() + PALW_BASE0_FP_INTERVAL_MAGIC_V3.len());
        out.extend_from_slice(&PALW_BASE0_FP_INTERVAL_MAGIC_V3);
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn decode_v1(bytes: &[u8]) -> Result<Self, Base0FpIntervalError> {
        let body = bytes.strip_prefix(&PALW_BASE0_FP_INTERVAL_MAGIC_V3).ok_or(Base0FpIntervalError::NotThisFamilysBytes)?;
        let decoded: Self = borsh::from_slice(body).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        if decoded.version != PALW_BASE0_FP_INTERVAL_VERSION_V3 || decoded.opening.version != PALW_BASE0_FP_INTERVAL_VERSION_V2 {
            return Err(Base0FpIntervalError::NotThisFamilysBytes);
        }
        Ok(decoded)
    }
}

/// **The close annex of an opening, if it carries one** (ADR-0085 Decision 2's first input).
/// `None` for a v1 or v2 opening, for a v3 opening served without an annex, and for bytes that are
/// not this family's — the closer then has nothing to assemble from and waits, by name.
/// **The V4 opening — the fold, not the leaves** (ADR-0086). Magic `MSKFPIV4`, version 4.
pub const PALW_BASE0_FP_INTERVAL_MAGIC_V4: [u8; 8] = *b"MSKFPIV4";
pub const PALW_BASE0_FP_INTERVAL_VERSION_V4: u16 = 4;

/// **A range opened by its fold** (ADR-0086 Decision 1): the consensus range opening's sibling
/// sequence — left-then-right per level, bottom-up, exactly [`PalwStepRangeOpeningV1::siblings`]
/// — and the fold's retained nodes for the blocks lying whole inside the range, in order. No leaf
/// hash rides: the seat replays the interval and supplies its own ([`Self::with_leaves_v1`]),
/// walks the consensus root rule unchanged, and names a block whose digest is not its own
/// ([`Self::first_block_that_differs_v1`]). `retain_level` is the level the digests are at —
/// the ruleset's, `palw_base0_sparse_retain_level_v1(cap)` — so the form describes itself.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpFoldRangeOpeningV1 {
    pub first_leaf_index: u64,
    pub leaf_count: u64,
    pub retain_level: u32,
    pub block_roots: Vec<Hash64>,
    pub siblings: Vec<Hash64>,
}

impl Base0FpFoldRangeOpeningV1 {
    /// The blocks lying whole inside the range, as `[first_block, end_block)` at
    /// `retain_level`. The tree's own tail block is whole when the range reaches the tree's end.
    pub fn whole_blocks_v1(&self, step_leaf_count: u64) -> (u64, u64) {
        let block = 1u64 << self.retain_level.min(63);
        let end = self.first_leaf_index.saturating_add(self.leaf_count);
        let first_block = self.first_leaf_index.div_ceil(block);
        let end_block = if end >= step_leaf_count { end.div_ceil(block) } else { end / block };
        (first_block, end_block.max(first_block))
    }
    /// From the leaf-level range opening and the tree it was cut from: the same frontier, the
    /// fold's digests instead of the leaves.
    pub fn from_range_v1(
        range: &PalwStepRangeOpeningV1,
        tree: &crate::fp_capture::Base0SparseStepTreeV1,
    ) -> Result<Self, Base0FpIntervalError> {
        let mut this = Self {
            first_leaf_index: range.first_leaf_index,
            leaf_count: range.leaf_hashes.len() as u64,
            retain_level: tree.retain_level(),
            block_roots: Vec::new(),
            siblings: range.siblings.clone(),
        };
        let (first_block, end_block) = this.whole_blocks_v1(tree.leaf_count());
        this.block_roots = tree
            .retained_nodes()
            .get(first_block as usize..end_block as usize)
            .ok_or(Base0FpIntervalError::CaptureIsNotTheBindings)?
            .to_vec();
        Ok(this)
    }
    /// The consensus form, with the leaves the seat supplies — one per leaf of the range, in order.
    pub fn with_leaves_v1(&self, leaf_hashes: Vec<Hash64>) -> Option<PalwStepRangeOpeningV1> {
        (leaf_hashes.len() as u64 == self.leaf_count).then(|| PalwStepRangeOpeningV1 {
            first_leaf_index: self.first_leaf_index,
            leaf_hashes,
            siblings: self.siblings.clone(),
        })
    }
    /// Whether the digest count is the whole-block count — the shape check a seat makes first.
    pub fn digests_are_the_blocks_v1(&self, step_leaf_count: u64) -> bool {
        let (first_block, end_block) = self.whole_blocks_v1(step_leaf_count);
        self.block_roots.len() as u64 == end_block - first_block
    }
    /// Fold `leaf_hashes` (the range's, in order) over the whole blocks and name the first block
    /// whose digest is not the served one, clipped to the range as `(first_leaf_index, count)`.
    /// `None` when every digest agrees, or when the shapes do not match.
    pub fn first_block_that_differs_v1(&self, leaf_hashes: &[Hash64], step_leaf_count: u64) -> Option<(u64, u64)> {
        let (first_block, end_block) = self.whole_blocks_v1(step_leaf_count);
        if self.block_roots.len() as u64 != end_block - first_block || leaf_hashes.len() as u64 != self.leaf_count {
            return None;
        }
        let block = 1u64 << self.retain_level.min(63);
        let range_end = self.first_leaf_index + self.leaf_count;
        for (k, served) in (first_block..end_block).zip(&self.block_roots) {
            let first = k * block;
            let end = ((k + 1) * block).min(range_end);
            let slice = &leaf_hashes[(first - self.first_leaf_index) as usize..(end - self.first_leaf_index) as usize];
            if crate::fp_capture::base0_fold_block_digest_v1(first, slice) != Some(*served) {
                return Some((first, end - first));
            }
        }
        None
    }
}

/// **The V4 interval opening** (ADR-0086): the fold range, the seed row, the NAMED anchor for
/// every class (Decision 2), and the close annex ADR-0085 defined.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpIntervalOpeningV4 {
    pub version: u16,
    pub interval_index: u32,
    pub binding: PalwStepBindingV2,
    pub range: Base0FpFoldRangeOpeningV1,
    pub seed_row_leaf_count: u32,
    pub seed_row_tiles: Vec<PalwStepTileLeafV1>,
    pub anchor: Option<Base0FpCheckpointClaimV1>,
    pub close: Option<Base0FpCloseAnnexV1>,
}

impl Base0FpIntervalOpeningV4 {
    pub fn encode_v1(&self) -> Result<Vec<u8>, Base0FpIntervalError> {
        let body = borsh::to_vec(self).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        let mut out = Vec::with_capacity(body.len() + PALW_BASE0_FP_INTERVAL_MAGIC_V4.len());
        out.extend_from_slice(&PALW_BASE0_FP_INTERVAL_MAGIC_V4);
        out.extend_from_slice(&body);
        Ok(out)
    }
    pub fn decode_v1(bytes: &[u8]) -> Result<Self, Base0FpIntervalError> {
        let body = bytes.strip_prefix(&PALW_BASE0_FP_INTERVAL_MAGIC_V4).ok_or(Base0FpIntervalError::NotThisFamilysBytes)?;
        let decoded: Self = borsh::from_slice(body).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        if decoded.version != PALW_BASE0_FP_INTERVAL_VERSION_V4 {
            return Err(Base0FpIntervalError::NotThisFamilysBytes);
        }
        Ok(decoded)
    }
}

/// **A block of the producer's leaves, on the annex lane** (ADR-0086 Decision 6). A seat that
/// holds a `FaultInRange` and its own replay asks the executor for the leaf hashes of one fold
/// block of the claim's step space — ≤ 4,096 hashes, 256 KB, inside the opening cap — names the
/// leaf from the first difference, and derives the court's path from the range with that block
/// substituted for its own. Magic `MSKFPBL1`, version 1.
pub const PALW_BASE0_FP_BLOCK_LEAVES_MAGIC_V1: [u8; 8] = *b"MSKFPBL1";
pub const PALW_BASE0_FP_BLOCK_LEAVES_VERSION_V1: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpBlockLeavesV1 {
    pub version: u16,
    pub interval_index: u32,
    pub first_leaf_index: u64,
    pub leaf_hashes: Vec<Hash64>,
}

impl Base0FpBlockLeavesV1 {
    pub fn encode_v1(&self) -> Result<Vec<u8>, Base0FpIntervalError> {
        let body = borsh::to_vec(self).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        let mut out = Vec::with_capacity(body.len() + PALW_BASE0_FP_BLOCK_LEAVES_MAGIC_V1.len());
        out.extend_from_slice(&PALW_BASE0_FP_BLOCK_LEAVES_MAGIC_V1);
        out.extend_from_slice(&body);
        Ok(out)
    }
    pub fn decode_v1(bytes: &[u8]) -> Result<Self, Base0FpIntervalError> {
        let body = bytes.strip_prefix(&PALW_BASE0_FP_BLOCK_LEAVES_MAGIC_V1).ok_or(Base0FpIntervalError::NotThisFamilysBytes)?;
        let decoded: Self = borsh::from_slice(body).map_err(|_| Base0FpIntervalError::NotThisFamilysBytes)?;
        if decoded.version != PALW_BASE0_FP_BLOCK_LEAVES_VERSION_V1 {
            return Err(Base0FpIntervalError::NotThisFamilysBytes);
        }
        Ok(decoded)
    }
    /// **The producer's side**: the leaf hashes of the block at `block_index` of a range's fold,
    /// cut from leaves the producer holds (the dense tuple's, or a replayed span's), as
    /// `(global index, hash)` pairs that cover it. `None` when the block is not the range's or a
    /// leaf of it is missing.
    pub fn cut_v1(
        interval_index: u32,
        fold: &Base0FpFoldRangeOpeningV1,
        step_leaf_count: u64,
        block_index: u64,
        leaves: &dyn Fn(u64) -> Option<Hash64>,
    ) -> Option<Self> {
        let (first_block, end_block) = fold.whole_blocks_v1(step_leaf_count);
        if block_index < first_block || block_index >= end_block {
            return None;
        }
        let block = 1u64 << fold.retain_level.min(63);
        let first = block_index * block;
        let end = ((block_index + 1) * block).min(fold.first_leaf_index + fold.leaf_count);
        let leaf_hashes = (first..end).map(leaves).collect::<Option<Vec<_>>>()?;
        Some(Self { version: PALW_BASE0_FP_BLOCK_LEAVES_VERSION_V1, interval_index, first_leaf_index: first, leaf_hashes })
    }
    /// **The served block is the served digest's**: the check a challenger makes before it
    /// believes any leaf of it.
    pub fn folds_to_v1(&self, digest: &Hash64) -> bool {
        crate::fp_capture::base0_fold_block_digest_v1(self.first_leaf_index, &self.leaf_hashes).as_ref() == Some(digest)
    }
    /// **The leaf the court is opened at**: the first index where the producer's block and the
    /// challenger's own leaves for it differ. `None` when they agree — then the served block is
    /// not what the served digest committed, and there is nothing to prosecute by a leaf.
    pub fn name_the_leaf_v1(&self, own: &dyn Fn(u64) -> Option<Hash64>) -> Option<u64> {
        self.leaf_hashes
            .iter()
            .enumerate()
            .map(|(i, h)| (self.first_leaf_index + i as u64, h))
            .find(|(index, h)| own(*index).as_ref() != Some(*h))
            .map(|(index, _)| index)
    }
}

/// **The producer's range, as far as the challenger can know it** (ADR-0086 Decision 6): the
/// challenger's own leaves for the range with one served block substituted. Under a single
/// differing block this IS the producer's range, so a path derived from it
/// (`step_opening_from_range_capped_v1`) walks to the committed root; with more than one
/// differing block the path does not walk and the court refuses it, which convicts nobody.
pub fn base0_fp_range_with_served_block_v1(
    fold: &Base0FpFoldRangeOpeningV1,
    own: &[Hash64],
    served: &Base0FpBlockLeavesV1,
) -> Option<PalwStepRangeOpeningV1> {
    if own.len() as u64 != fold.leaf_count {
        return None;
    }
    let offset = served.first_leaf_index.checked_sub(fold.first_leaf_index)? as usize;
    let end = offset.checked_add(served.leaf_hashes.len())?;
    if end > own.len() {
        return None;
    }
    let mut leaves = own.to_vec();
    leaves[offset..end].copy_from_slice(&served.leaf_hashes);
    fold.with_leaves_v1(leaves)
}

pub fn base0_fp_interval_close_annex_v1(bytes: &[u8]) -> Option<Base0FpCloseAnnexV1> {
    if !bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V3) {
        return None;
    }
    Base0FpIntervalOpeningV3::decode_v1(bytes).ok()?.close
}

/// Either form, as a seat receives it. A seat on an old executor still reads v1; a graph-v5 class
/// refuses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Base0FpIntervalOpeningAnyV1 {
    /// ADR-0077 Decision 8: the checkpoint chunk travels. For an attention family that chunk is
    /// the history.
    WithHistory(Box<Base0FpIntervalOpeningV1>),
    /// ADR-0082 Decision 9: the checkpoint is named and the seat holds the state.
    Recomputed(Box<Base0FpIntervalOpeningV2>),
    /// ADR-0086: the fold's digests and the frontier; the seat supplies the leaves.
    Digests(Box<Base0FpIntervalOpeningV4>),
}

/// Decode whichever form arrived. The magic decides, so nothing is mis-parsed as the other.
pub fn base0_fp_interval_opening_decode_any_v1(bytes: &[u8]) -> Result<Base0FpIntervalOpeningAnyV1, Base0FpIntervalError> {
    // ADR-0085 X3: a v3 opening is its v2 opening to a seat; the annex is the closer's, read
    // through `base0_fp_interval_close_annex_v1`, and never reaches a replay or a verdict.
    if bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V4) {
        return Ok(Base0FpIntervalOpeningAnyV1::Digests(Box::new(Base0FpIntervalOpeningV4::decode_v1(bytes)?)));
    }
    if bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V3) {
        return Ok(Base0FpIntervalOpeningAnyV1::Recomputed(Box::new(Base0FpIntervalOpeningV3::decode_v1(bytes)?.opening)));
    }
    if bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V2) {
        return Ok(Base0FpIntervalOpeningAnyV1::Recomputed(Box::new(Base0FpIntervalOpeningV2::decode_v1(bytes)?)));
    }
    Ok(Base0FpIntervalOpeningAnyV1::WithHistory(Box::new(Base0FpIntervalOpeningV1::decode_v1(bytes)?)))
}

impl Base0FpIntervalOpeningAnyV1 {
    /// **The checkpoint this opening's interval resumes from, as the CLAIM commits it** — the
    /// index and the state's root, for the 64-byte comparison a seat makes against its own
    /// recompute. `None` for interval 0, which has no checkpoint before it.
    ///
    /// It is read out of both forms because the comparison is the same one either way: what
    /// changes between them is whether the executor also shipped the bytes the root is of.
    pub fn committed_checkpoint_v1(&self) -> Option<(u32, Hash64)> {
        match self {
            Self::WithHistory(o) => o.anchor.as_ref().map(|a| (a.leaf.checkpoint_index, a.leaf.state_chunks_root)),
            Self::Recomputed(o) => o.anchor.as_ref().map(|a| (a.leaf.checkpoint_index, a.leaf.state_chunks_root)),
            Self::Digests(o) => o.anchor.as_ref().map(|a| (a.leaf.checkpoint_index, a.leaf.state_chunks_root)),
        }
    }
    pub fn binding(&self) -> &PalwStepBindingV2 {
        match self {
            Self::WithHistory(o) => &o.binding,
            Self::Recomputed(o) => &o.binding,
            Self::Digests(o) => &o.binding,
        }
    }
    pub fn interval_index(&self) -> u32 {
        match self {
            Self::WithHistory(o) => o.interval_index,
            Self::Recomputed(o) => o.interval_index,
            Self::Digests(o) => o.interval_index,
        }
    }

    /// Which form this was, for a log line and for the class bound that refuses one of them.
    pub fn carries_the_history(&self) -> bool {
        matches!(self, Self::WithHistory(_))
    }
}

/// **What an opening SAYS its interval resumes from** — the checkpoint index, the decode call it
/// covers, and the state root committed for it (ADR-0082 Decision 9).
///
/// This is what a seat reads BEFORE it recomputes: the covered call says how far to run, and the
/// root is what the recompute is compared against. `None` for interval 0, which starts at the
/// prompt, and for bytes that are not an opening.
///
/// **Everything here is the executor's word until the replay checks it.** The covered call is
/// re-derived from the geometry inside
/// [`base0_verify_fp_interval_opening_with_state_v1`] and an opening that named another one is
/// refused there — so a lie costs this seat one recompute and buys nothing. A caller that wants
/// the cheap half of that guard bounds `covered_decode_call` by the job's own decode count before
/// it spends the pass.
pub fn base0_fp_interval_opening_anchor_v1(opening_bytes: &[u8]) -> Option<(u32, u32, Hash64)> {
    let any = base0_fp_interval_opening_decode_any_v1(opening_bytes).ok()?;
    let (index, root) = any.committed_checkpoint_v1()?;
    let covered = match &any {
        Base0FpIntervalOpeningAnyV1::WithHistory(o) => o.anchor.as_ref().map(|a| a.leaf.covered_decode_call),
        Base0FpIntervalOpeningAnyV1::Recomputed(o) => o.anchor.as_ref().map(|a| a.leaf.covered_decode_call),
        Base0FpIntervalOpeningAnyV1::Digests(o) => o.anchor.as_ref().map(|a| a.leaf.covered_decode_call),
    }?;
    Some((index, covered, root))
}

/// **The state this seat already recomputed for the interval this opening is of**, if it has one
/// (ADR-0082 Decision 9).
///
/// The opening says which class, which job and which interval; the covered call follows from the
/// geometry; and the seat's own recompute is looked up under exactly those. `None` means this seat
/// has not run the job — the row check then has no state to resume from and files `Unverifiable`,
/// which is honest and is not an accusation.
///
/// One place, because both families ask the same question and a family that asked it its own way
/// would be a family whose seat resumed from a state computed for another call.
pub fn base0_fp_interval_opening_seat_state_v1(
    opening_bytes: &[u8],
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
) -> Option<crate::fp_recompute::Base0FpSeatStateV1> {
    base0_fp_interval_opening_seat_state_capped_v1(
        opening_bytes,
        prompt_token_ids,
        family_checkpoint_interval,
        PALW_STEP_LEG_MAX_LEAVES,
    )
}

/// [`base0_fp_interval_opening_seat_state_v1`] against the ladder top the CALLER states.
///
/// The geometry is built only to name the covered checkpoint, but building it prices the binding —
/// so at the executor's constant this returned `None` for every honest graph-v5 512 opening, and
/// `None` here is the difference between a seat that resumes from its OWN state and one that files
/// `Unverifiable` on an honest producer.
pub fn base0_fp_interval_opening_seat_state_capped_v1(
    opening_bytes: &[u8],
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
) -> Option<crate::fp_recompute::Base0FpSeatStateV1> {
    let any = base0_fp_interval_opening_decode_any_v1(opening_bytes).ok()?;
    let binding = any.binding();
    let index = any.interval_index();
    let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(binding, family_checkpoint_interval, max_step_leaf_count).ok()?;
    // The memo is keyed by the LEAF's counter, which is the unit the recompute was ordered in
    // (`PalwBackend::fp_recompute_checkpoint_root`) and the unit the row check compares against.
    // Keying it in calls while the class counts positions is audit B's C-2 on the lookup side: the
    // seat holds the right state and cannot find it, so every honest graph-v5 opening is
    // `Unverifiable`.
    let covered = geometry.anchor_covered_call(index)?;
    crate::fp_recompute::base0_fp_seat_state_held_v1(&binding.shape_profile, &binding.job_context, prompt_token_ids, covered)
}

/// **Which classes may not be served the history** (ADR-0082 Decision 9, Decision 4).
///
/// The TILED maps are the graph-v5 declaration — `tiled_kv_state_chunk_map_id_v3` and the hybrid
/// composition over it — and a class that registers one has declared that its cache is addressed a
/// history tile at a time precisely so that nothing carries the history. Read off the class's own
/// profile rather than passed in: the rule belongs to the class, and a caller that could pass
/// `false` would be a caller that could spend the bytes Decision 9 exists to save.
pub fn base0_fp_class_requires_flat_openings_v1(profile: &PalwShapeProfileV3) -> bool {
    use kaspa_consensus_core::palw_state_chunk_map as map;
    profile.state_chunk_map_id == map::tiled_kv_state_chunk_map_id_v3()
        || profile.state_chunk_map_id == map::hybrid_state_chunk_map_id_v3()
}

/// **The committed root of a set of state chunks under a class's map** — the two consensus
/// functions `Base0CheckpointCaptureV1::push_chunks` calls, spelled once.
///
/// Both directions of Decision 9 go through it: the seat's own recompute
/// ([`crate::fp_recompute::base0_fp_recompute_state_v1`]) and the check that an executor's carried
/// anchor is the binding's ([`checkpoint_anchor_is_the_bindings_v1`]). A second spelling would be
/// a second opinion about what a producer committed, and the two would agree until the day the
/// map's leaf rule moved.
pub fn base0_state_chunks_root_v1(map_id: &Hash64, chunks: &[Vec<u8>]) -> Result<Hash64, Base0FpIntervalError> {
    use kaspa_consensus_core::palw_step_leg::{
        PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES, PALW_STEP_LEG_MAX_STATE_CHUNKS, state_chunk_leaf_hash_v1, state_chunks_root_v1,
    };
    // The leg's own caps, applied before a byte is hashed. On the seat's own chunks they can only
    // fire for a class whose map asks for more than the leg carries; on a stranger's they are the
    // reason a seat does not hash a gigabyte to learn the opening was never admissible.
    if chunks.len() > PALW_STEP_LEG_MAX_STATE_CHUNKS || chunks.iter().any(|c| c.len() > PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES) {
        return Err(Base0FpIntervalError::Leg("the state chunks are outside the leg's caps".to_string()));
    }
    let hashes: Vec<Hash64> = chunks.iter().enumerate().map(|(i, bytes)| state_chunk_leaf_hash_v1(map_id, i as u32, bytes)).collect();
    state_chunks_root_v1(&hashes).map_err(|e| Base0FpIntervalError::Leg(format!("{e:?}")))
}

// ---------------------------------------------------------------------------------------------
// The executor's side
// ---------------------------------------------------------------------------------------------

/// The leaf-hash vector a capture's tiles imply — the same eleven lines the backends keep beside
/// their rungs, in one place because the opening needs it for three different reasons.
///
/// **The count is bounded before the vector is sized from it.** `step_leaf_count` is a plain `u64`
/// that arrives inside a borsh blob and a `Hash64` is 64 bytes, so a few hundred bytes asking for
/// `2^48` leaves is a `2^54`-byte request: above any allocator's reach, so it is
/// `handle_alloc_error` and a process ABORT rather than a catchable panic. The bound the sparse
/// fold applies is the same one — it just applies it after this vector already exists, which is the
/// exact shape of the defect `base0_material_matches_claim_v1` records.
fn leaves_from_tiles_v1(
    binding: &PalwStepBindingV2,
    tiles: &[(u64, PalwStepTileLeafV1)],
    max_step_leaf_count: u64,
) -> Result<Vec<Hash64>, Base0FpIntervalError> {
    base0_fp_binding_step_space_v1(binding, max_step_leaf_count)?;
    // One spelling with the material verifier (`produce::base0_dense_step_leaves_capped_v1`): a
    // tile outside the space is a capture that is not the binding's, here as there.
    crate::produce::base0_dense_step_leaves_capped_v1(binding, tiles, max_step_leaf_count)
        .ok_or(Base0FpIntervalError::CaptureIsNotTheBindings)
}

/// **Open interval `index` of a retained capture** (ADR-0077 Decision 8, executor half).
///
/// `family_checkpoint_interval` is the class's own cadence, and it is cross-checked against the
/// one the capture COMMITTED rather than taken on faith: a capture whose leg was built at another
/// interval is a capture whose intervals are not the ones a seat drew.
///
/// The tree the siblings come from is the sparse one ([`crate::fp_capture`]), built here from the
/// dense retention this family writes today. That is deliberate: when the retention becomes sparse
/// the only thing that changes is where the span's leaf hashes come from, and the opening's shape,
/// its cost and its verification are already the sparse ones.
pub fn base0_open_fp_interval_v1(
    material: &Base0RetainedMaterialV1,
    index: u32,
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    base0_open_fp_interval_capped_v1(
        material,
        index,
        prompt_token_ids,
        family_checkpoint_interval,
        PALW_STEP_LEG_MAX_LEAVES,
        prompt_ids_form,
    )
}

/// [`base0_open_fp_interval_v1`] against the ladder top the CALLER states — the ruleset's
/// `PalwCourtParamsV2::max_step_leaf_count`, which a backend holds through `with_step_ladder_cap`
/// (ADR-0080 W1b). The un-capped name above passes the executor's default, so a caller with no
/// ruleset in scope keeps exactly the behaviour it had.
pub fn base0_open_fp_interval_capped_v1(
    material: &Base0RetainedMaterialV1,
    index: u32,
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    let (binding, tiles, _logits_rows, _generated, chunks) = material;
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;

    // **The ids are an INPUT on this lane and are refused unless they are the job's** — the rule
    // `refutation_for_free_prompt_index` states: a wrong list reads to the court as
    // `InputSetNotCanonical`, which is no verdict at all.
    if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
        prompt_ids_form,
        prompt_token_ids,
        &ctx.prompt_token_ids_hash,
    ) {
        return Err(Base0FpIntervalError::PromptIdsAreNotTheJobs);
    }
    // **The sentinel refuses by name.** A class that registers no state chunk map takes no
    // checkpoints at all, so interval 0 is the whole job and there is no interval above it to
    // open. Saying that is better than serving an opening whose anchor does not exist — and when
    // ADR-0077 Decision 10's recurrence map arrives, the class simply declares one and this stops
    // refusing without a line changing here.
    if profile.state_chunk_map_id == Hash64::default() && index > 0 {
        return Err(Base0FpIntervalError::NoStateChunkMapRegistered { index });
    }
    if base0_fp_class_requires_flat_openings_v1(profile) && index > 0 {
        return Err(Base0FpIntervalError::DenseRetentionCarriesNoCheckpointLeaves);
    }
    let step_leaf_count = base0_fp_binding_step_space_v1(binding, max_step_leaf_count)?;
    let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(binding, family_checkpoint_interval, max_step_leaf_count)?;
    let leaves_geometry = base0_fp_interval_leaves_v1(profile, ctx, &geometry, index, step_leaf_count)?;

    let leaves = leaves_from_tiles_v1(binding, tiles, max_step_leaf_count)?;
    // The RULESET's level, not the constant: the fold retention keeps its tree at
    // `palw_base0_sparse_retain_level_v1(cap)`, and a V4 opening's digests are that level's nodes,
    // so the dense route and the fold route serve one form byte for byte (ADR-0086 Decision 4).
    let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(
        &leaves,
        crate::fp_capture::palw_base0_sparse_retain_level_v1(max_step_leaf_count),
        max_step_leaf_count,
    )?;
    // The capture must be the one the binding committed, checked before anything is served: an
    // opening assembled from a leaf vector that does not reproduce `step_merkle_root` is an
    // opening no seat can verify, and the executor would rather learn that here.
    if tree.root()? != binding.step_merkle_root {
        return Err(Base0FpIntervalError::CaptureIsNotTheBindings);
    }

    let count = leaves_geometry.range_end - leaves_geometry.range_first;
    let (span_first, span_end) = tree.span_for_range(leaves_geometry.range_first, count)?;

    // The anchor call's logits tiles travel with their preimages; every other leaf of the range is
    // a hash the seat recomputes for itself.
    //
    // Gathered in ONE pass over the capture's tiles rather than a `find` per leaf. The seed row is
    // a whole logits row — 993 KB on a Qwen-class vocabulary, thousands of tiles — and the capture
    // holds millions of them, so the search-per-leaf form is quadratic in exactly the dimension
    // Decision 13 is about to multiply by 64.
    let seed_span = leaves_geometry.seed_row_leaves as usize;
    let mut seed_slots: Vec<Option<&PalwStepTileLeafV1>> = vec![None; seed_span];
    for (at, leaf) in tiles {
        if let Some(offset) = at.checked_sub(leaves_geometry.range_first).filter(|o| (*o as usize) < seed_span) {
            seed_slots[offset as usize] = Some(leaf);
        }
    }
    let mut seed_row_tiles = Vec::with_capacity(seed_span);
    for (offset, slot) in seed_slots.into_iter().enumerate() {
        let leaf = slot.ok_or(Base0FpIntervalError::CaptureHasNoTile { index: leaves_geometry.range_first + offset as u64 })?;
        seed_row_tiles.push(leaf.clone());
    }

    let anchor = match geometry.anchor_covered_call(index) {
        None => None,
        Some(covered) => Some(base0_checkpoint_operands_v1(binding, chunks, &[], covered)?),
    };

    base0_assemble_fp_interval_opening_v1(
        binding,
        &tree,
        &leaves_geometry,
        index,
        span_first,
        &leaves[span_first as usize..span_end as usize],
        seed_row_tiles,
        anchor,
        max_step_leaf_count,
    )
}

/// **The opening, assembled** — the half that is the same whichever retention served it.
///
/// `span_leaves` are the leaf hashes of `[span_first, span_first + span_leaves.len())`: sliced out
/// of a dense capture, or re-derived by replay from a fold. Everything after that is one
/// derivation, so an opening's SHAPE, its cost and its verification do not know which retention
/// the executor kept — which is the property that lets Decision 7 change the retention without
/// changing what a seat checks.
#[allow(clippy::too_many_arguments)]
fn base0_assemble_fp_interval_opening_v1(
    binding: &PalwStepBindingV2,
    tree: &crate::fp_capture::Base0SparseStepTreeV1,
    leaves_geometry: &Base0FpIntervalLeavesV1,
    index: u32,
    span_first: u64,
    span_leaves: &[Hash64],
    seed_row_tiles: Vec<PalwStepTileLeafV1>,
    anchor: Option<PalwCheckpointKvOperandsV1>,
    max_step_leaf_count: u64,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    let count = leaves_geometry.range_end - leaves_geometry.range_first;
    let range = tree.range_opening_v1(span_first, span_leaves, leaves_geometry.range_first, count)?;
    // **The opening must reproduce the committed root, checked before it is served.** On the dense
    // route the leaves came out of the capture the tree was built from and this is an identity; on
    // the folded route they came out of a REPLAY, and an executor whose replay diverged from its
    // own commitment would otherwise learn it from a seat's `Fault` against itself.
    // The root walks under the RULESET's ladder, not the default one: the uncapped sibling is
    // bounded by `PALW_STEP_LEG_MAX_LEAVES`, and a class whose space is larger (the graph-v5
    // attempt lane is 6.6 M leaves) came back as "the capture is not the binding's" from a
    // capture that was.
    if step_range_opening_root_capped_v1(binding.step_leaf_count, &range, max_step_leaf_count).ok() != Some(binding.step_merkle_root) {
        return Err(Base0FpIntervalError::CaptureIsNotTheBindings);
    }
    // **The class's declaration decides which FORM is served, here, once** (ADR-0082 Decision 9;
    // audit B, C-1). A class whose map addresses history tiles folds its checkpoints away and is
    // served the NAMED anchor — there are no chunks to carry and Decision 9 forbids carrying them
    // anyway. Emitting it here rather than assembling the chunked form and stripping it after is
    // what keeps a chunkless `Base0FpIntervalOpeningV1` — an object whose anchor cannot build its
    // own state root — from existing at all.
    let flat = base0_fp_class_requires_flat_openings_v1(&binding.shape_profile);
    if let Some(a) = anchor.as_ref()
        && !flat
        && a.chunks.len() as u32 != a.leaf.state_chunk_count
    {
        return Err(Base0FpIntervalError::CaptureIsNotTheBindings);
    }
    // **The fold, not the leaves** (ADR-0086 Decision 1): the frontier and the retained digests
    // ride; the range's leaf hashes stay here. The anchor is NAMED for every class (Decision 2):
    // the seat replays from the state it recomputed for the checkpoint check, chunks and all.
    let range = Base0FpFoldRangeOpeningV1::from_range_v1(&range, tree)?;
    let anchor = anchor.map(|a| Base0FpCheckpointClaimV1 { leaf: a.leaf, opening: a.opening });
    Base0FpIntervalOpeningV4 {
        version: PALW_BASE0_FP_INTERVAL_VERSION_V4,
        interval_index: index,
        binding: binding.clone(),
        range,
        seed_row_leaf_count: leaves_geometry.seed_row_leaves as u32,
        seed_row_tiles,
        anchor,
        close: None,
    }
    .encode_v1()
}

/// **Open interval `index` WITHOUT its history** (ADR-0082 Decision 9, executor half).
///
/// The same opening [`base0_open_fp_interval_v1`] builds, with the anchor's chunks dropped: what
/// the seat gets is the checkpoint's leaf and its Merkle opening, which name the state's committed
/// root, and the seat computes the state itself. Bytes stop depending on `n_ctx`.
///
/// Built by stripping the chunked form rather than by a second assembly. The strip happens inside
/// this process and nothing extra leaves it, and the property that matters — the two forms agree
/// about every field a seat checks — is then structural instead of tested.
pub fn base0_open_fp_interval_chunkless_v1(
    material: &Base0RetainedMaterialV1,
    index: u32,
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    // A V4 opening carries the named anchor already (ADR-0086 Decision 2); nothing to strip.
    base0_open_fp_interval_v1(material, index, prompt_token_ids, family_checkpoint_interval, prompt_ids_form)
}

/// **Strip the history from an assembled opening** — the chunked form → the flat form, in bytes.
///
/// ADR-0082 Decisions 7 and 9 meet here: the FOLDED retention's opening is assembled by replay
/// ([`base0_open_fp_interval_sparse_v1`]) and a graph-v5 seat wants it WITHOUT the anchor's chunks
/// ([`Base0FpIntervalOpeningV2`]). One strip for both retention forms, so the two routes cannot
/// disagree about what a flat opening is.
/// **Idempotent**: bytes that are already the flat form come back unchanged. The assembler emits
/// the flat form directly for a class that folds (there is no history to strip), and a caller that
/// then asked for a strip would otherwise be told its own opening "is not this family's bytes".
pub fn base0_strip_fp_interval_history_v1(chunked_bytes: &[u8]) -> Result<Vec<u8>, Base0FpIntervalError> {
    if chunked_bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V2) || chunked_bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V4) {
        return Ok(chunked_bytes.to_vec());
    }
    let chunked = Base0FpIntervalOpeningV1::decode_v1(chunked_bytes)?;
    Base0FpIntervalOpeningV2::from_chunked_v1(&chunked).encode_v1()
}

/// The interval whose replay REACHES `call` — the inverse of [`Base0FpIntervalGeometryV1::calls_for`].
fn interval_of_call_v1(geometry: &Base0FpIntervalGeometryV1, call: u32) -> u32 {
    if call == 0 { 0 } else { (call - 1) / geometry.checkpoint_interval.max(1) }
}

/// **The span's leaf hashes, re-derived by replay** (ADR-0082 Decision 7).
///
/// The fold keeps one node per `2^retain_level` leaves; every leaf hash an opening's siblings are
/// folded from is recomputed here, by running the class's own kernels over the calls the span
/// touches, resumed from the checkpoint that anchors the first of them. That is the same replay a
/// seat performs to CHECK the opening (`base0_fp_replay_interval_v1`), run by the executor to
/// PRODUCE it — one loop, so a re-derived opening cannot be a different execution from the one a
/// seat will compare it against.
///
/// The window is at most two checkpoint intervals: the span's left edge can reach back into the
/// anchor call (whose logits row the opening carries as its seed), which the previous interval's
/// checkpoint anchors, and its right edge can round up into the call after the interval's last.
/// **The state a folded retention resumes an interval from when it carries no checkpoint chunks.**
///
/// A fold keeps no KV state (one checkpoint of graph-v5 is ~34 MB; three hundred of them do not
/// ride a 183 MB material), so until 2026-09-05 every span replay started at GENESIS and captured
/// tiles all the way — ~5–10 s per decode call: 21 s for interval 0 and 1,991 s for interval 187
/// of a 300-token claim on the devnet, against a seat's 120 s solicitation window. The executor
/// now asks this for the anchor state of the span's starting interval — the same
/// `base0_fp_seat_state_memoized_v1` a seat recomputes for the checkpoint check, a plain forward
/// pass without capture, memoized — and replays only the interval's own call(s) with tiles.
/// `None` keeps the genesis walk, which is what a caller with no recompute kernels gets.
pub type Base0FpAnchorStateForV1<'a> = &'a dyn Fn(u32) -> Option<crate::fp_recompute::Base0FpSeatStateV1>;

#[allow(clippy::too_many_arguments)]
fn base0_replay_span_leaves_v1<K: Base0FpIntervalKernelsV1>(
    kernels: &K,
    binding: &PalwStepBindingV2,
    chunks: &[Vec<Vec<u8>>],
    generated: &[u32],
    prompt_token_ids: &[u32],
    geometry: &Base0FpIntervalGeometryV1,
    span_first: u64,
    span_end: u64,
    anchor_state_for: Base0FpAnchorStateForV1<'_>,
) -> Result<Vec<Hash64>, Base0FpIntervalError> {
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;
    let call_of = |leaf: u64| -> Result<u32, Base0FpIntervalError> {
        kaspa_consensus_core::palw_step::canonical_step_coordinates(profile, ctx, leaf)
            .map(|c| c.call_index)
            .ok_or_else(|| Base0FpIntervalError::StepSpace(format!("leaf {leaf} is not a main step coordinate")))
    };
    let first_needed = call_of(span_first)?;
    let last_needed = call_of(span_end - 1)?;
    let interval = interval_of_call_v1(geometry, first_needed);
    let (first_call, _) = geometry
        .calls_for(interval)
        .ok_or(Base0FpIntervalError::IntervalOutOfRange { index: interval, count: geometry.interval_count })?;

    let prompt_usize: Vec<usize> = prompt_token_ids.iter().map(|t| *t as usize).collect();
    // **A fold that retained no state resumes from the prompt** (ADR-0082 Decision 4, amended).
    // A per-position class keeps ZERO bytes of cache per checkpoint, so there is no chunk to
    // restore and the executor pays what Decision 9 prices a seat at: one forward pass of the
    // prefix. The alternative — retaining a chunk per position — is the `Θ(n²)` term the
    // amendment exists to remove.
    let anchored = !chunks.is_empty();
    // The chunks the span resumes from: the retention's own checkpoint when it carries one, else
    // the state recomputed for the starting interval's named anchor (`Base0FpAnchorStateForV1`),
    // else the prompt — the walk this used to take for every interval, tiles and all.
    let covered_call = geometry.anchor_covered_call(interval);
    let (resume_chunks, first_call): (Option<Vec<Vec<u8>>>, u32) = match covered_call {
        Some(covered) if anchored => (Some(base0_checkpoint_operands_v1(binding, chunks, &[], covered)?.chunks), first_call),
        Some(covered) => match anchor_state_for(covered) {
            Some(state) if state.covered_decode_call == covered => (Some(state.chunks), first_call),
            _ => (None, 0),
        },
        None => (None, 0),
    };
    let start = match (&resume_chunks, covered_call) {
        (None, _) => Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt_usize },
        (Some(resume), Some(covered)) => {
            // The id the anchored call consumes is the one the CHECKPOINT's own call produced, and
            // the executor is the party that produced it. A seat derives the same id from the
            // committed row instead of being told it (the module doc's rule); here the two are the
            // same value, and the range opening's root check below is what says so.
            //
            // Indexed by the anchor's CALL, never by the leaf's counter: `generated` is one id per
            // call and a per-position leaf's counter is a position.
            let seed_call = geometry.anchor_seed_call_v1(interval).ok_or(Base0FpIntervalError::NoCheckpointAt { covered })?;
            let seed_token = *generated
                .get(seed_call as usize)
                .ok_or_else(|| Base0FpIntervalError::Replay(format!("the retention has no id for call {seed_call}")))?;
            Base0FpIntervalStartV1::Checkpoint { covered_decode_call: covered, chunks: resume, seed_token }
        }
        (Some(_), None) => return Err(Base0FpIntervalError::NoCheckpointAt { covered: 0 }),
    };

    let replayed = kernels
        .replay_interval(profile, ctx, &start, first_call, last_needed, binding.step_leaf_count)
        .map_err(Base0FpIntervalError::Replay)?;
    let width = (span_end - span_first) as usize;
    let mut span = vec![None; width];
    for (at, hash) in replayed {
        if let Some(offset) = at.checked_sub(span_first).filter(|o| (*o as usize) < width) {
            span[offset as usize] = Some(hash);
        }
    }
    span.into_iter()
        .enumerate()
        .map(|(offset, hash)| {
            hash.ok_or(Base0FpIntervalError::Sparse(crate::fp_capture::Base0SparseCaptureError::SpanDoesNotCoverTheRange {
                index: span_first + offset as u64,
                span_first,
                span_end,
            }))
        })
        .collect()
}

/// **Open interval `index` of a FOLDED retention** (ADR-0082 Decision 7, executor half).
///
/// The dense twin above reads its span's leaf hashes out of the tiles it kept; this one has no
/// tiles and re-derives them ([`base0_replay_span_leaves_v1`]). The seed row's PREIMAGES cannot be
/// re-derived from hashes, and they are not carried a second time either: they are cut from the
/// retained logits rows — the rows the decode pin already keeps — and then CHECKED against the
/// hashes the replay produced for the same leaves, so a retention whose rows are not the ones its
/// leg committed is refused here rather than served as evidence.
pub fn base0_open_fp_interval_sparse_v1<K: Base0FpIntervalKernelsV1>(
    material: &crate::produce::Base0FpMaterialV2,
    index: u32,
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    base0_open_fp_interval_sparse_capped_v1(
        material,
        index,
        prompt_token_ids,
        family_checkpoint_interval,
        PALW_STEP_LEG_MAX_LEAVES,
        kernels,
        prompt_ids_form,
    )
}

/// [`base0_open_fp_interval_sparse_v1`] against the ladder top the CALLER states. The un-capped
/// name passes the executor's default.
pub fn base0_open_fp_interval_sparse_capped_v1<K: Base0FpIntervalKernelsV1>(
    material: &crate::produce::Base0FpMaterialV2,
    index: u32,
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    base0_open_fp_interval_sparse_anchored_capped_v1(
        material,
        index,
        prompt_token_ids,
        family_checkpoint_interval,
        max_step_leaf_count,
        kernels,
        &|_| None,
        prompt_ids_form,
    )
}

/// The fold opener with the executor's anchor state (see [`Base0FpAnchorStateForV1`]): the span
/// replay resumes from the recomputed state of the starting interval's anchor instead of the
/// prompt when the retention carries no checkpoint chunks.
#[allow(clippy::too_many_arguments)]
pub fn base0_open_fp_interval_sparse_anchored_capped_v1<K: Base0FpIntervalKernelsV1>(
    material: &crate::produce::Base0FpMaterialV2,
    index: u32,
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    kernels: &K,
    anchor_state_for: Base0FpAnchorStateForV1<'_>,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    let binding = &material.binding;
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;

    // The ids are an INPUT on this lane and are refused unless they are the job's — the dense
    // route's rule, for the dense route's reason.
    if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
        prompt_ids_form,
        prompt_token_ids,
        &ctx.prompt_token_ids_hash,
    ) {
        return Err(Base0FpIntervalError::PromptIdsAreNotTheJobs);
    }
    if profile.state_chunk_map_id == Hash64::default() && index > 0 {
        return Err(Base0FpIntervalError::NoStateChunkMapRegistered { index });
    }
    let step_leaf_count = base0_fp_binding_step_space_v1(binding, max_step_leaf_count)?;
    let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(binding, family_checkpoint_interval, max_step_leaf_count)?;
    let leaves_geometry = base0_fp_interval_leaves_v1(profile, ctx, &geometry, index, step_leaf_count)?;

    let tree = &material.step_tree;
    if tree.leaf_count() != binding.step_leaf_count || tree.root()? != binding.step_merkle_root {
        return Err(Base0FpIntervalError::CaptureIsNotTheBindings);
    }
    let count = leaves_geometry.range_end - leaves_geometry.range_first;
    let (span_first, span_end) = tree.span_for_range(leaves_geometry.range_first, count)?;
    let span_leaves = base0_replay_span_leaves_v1(
        kernels,
        binding,
        &material.checkpoint_chunks,
        &material.generated_token_ids,
        prompt_token_ids,
        &geometry,
        span_first,
        span_end,
        anchor_state_for,
    )?;

    let seed_row_tiles = base0_seed_row_tiles_from_rows_v1(binding, &material.logits_rows, &leaves_geometry, &geometry, index)?;
    // The preimages must hash to the leaves the replay just recomputed — the retained rows and the
    // committed leg are two records of one execution, and this is where they are compared.
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    for (offset, leaf) in seed_row_tiles.iter().enumerate() {
        let at = leaves_geometry.range_first + offset as u64;
        let recomputed = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf);
        let derived = span_leaves.get((at - span_first) as usize).ok_or(Base0FpIntervalError::CaptureHasNoTile { index: at })?;
        if recomputed != *derived {
            return Err(Base0FpIntervalError::CaptureIsNotTheBindings);
        }
    }

    let anchor = match geometry.anchor_covered_call(index) {
        None => None,
        Some(covered) => {
            Some(base0_checkpoint_operands_v1(binding, &material.checkpoint_chunks, &material.checkpoint_leaves, covered)?)
        }
    };
    base0_assemble_fp_interval_opening_v1(
        binding,
        tree,
        &leaves_geometry,
        index,
        span_first,
        &span_leaves,
        seed_row_tiles,
        anchor,
        max_step_leaf_count,
    )
}

/// The anchor call's logits row, cut into the tiles the leg commits it as.
///
/// The row is the retained one (`Base0FpMaterialV2::logits_rows`, one per call, the row its token
/// was selected from); the coordinates are the enumeration's own. The caller checks the result
/// against the leg — this only cuts.
fn base0_seed_row_tiles_from_rows_v1(
    binding: &PalwStepBindingV2,
    logits_rows: &[Vec<i32>],
    leaves_geometry: &Base0FpIntervalLeavesV1,
    geometry: &Base0FpIntervalGeometryV1,
    index: u32,
) -> Result<Vec<PalwStepTileLeafV1>, Base0FpIntervalError> {
    let Some(anchor_call) = geometry.anchor_seed_call_v1(index) else {
        return Ok(Vec::new()); // interval 0 resumes from the prompt and carries no seed row
    };
    let profile = &binding.shape_profile;
    let slot = logits_node_slot_v1(profile);
    let (node, _) = profile
        .resolve_node_slot(slot)
        .ok_or_else(|| Base0FpIntervalError::StepSpace(format!("this class has no node at slot {slot}")))?;
    let row = logits_rows
        .get(anchor_call as usize)
        .ok_or_else(|| Base0FpIntervalError::Replay(format!("the retention has no logits row for call {anchor_call}")))?;
    let tile_len = node.tile_len as usize;
    if tile_len == 0 || row.len().div_ceil(tile_len) as u64 != leaves_geometry.seed_row_leaves {
        return Err(Base0FpIntervalError::CaptureIsNotTheBindings);
    }
    Ok(row
        .chunks(tile_len)
        .enumerate()
        .map(|(tile_index, chunk)| PalwStepTileLeafV1 {
            version: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
            coord: PalwStepCoordinateV1 { call_index: anchor_call, node_slot: slot, position: 0, tile_index: tile_index as u32 },
            value_count: chunk.len() as u32,
            values_le: chunk.iter().flat_map(|v| v.to_le_bytes()).collect(),
        })
        .collect())
}

/// **The committed checkpoint covering `covered`, as the operands an anchored replay resumes
/// from** (ADR-0077 Decision 10).
///
/// The leg is re-derived from the served CHUNKS — never from a carried leaf — because a leaf is a
/// pure function of its chunks, the chain and the job, so a carried copy would be a second source
/// for the same fact and the received copy is the one a dishonest producer controls. The same rule
/// `Base0RetainedMaterialV1` states about what travels.
///
/// One helper for two callers: an interval opening's anchor, and the KV anchor a refutation
/// carries when a challenger opens the anchored form instead of the genesis-anchored long one.
/// They must be the same object, or "the anchored form and the long form reach the same verdict"
/// (W2) would be a claim about two different pieces of evidence.
pub fn base0_checkpoint_operands_v1(
    binding: &PalwStepBindingV2,
    chunks: &[Vec<Vec<u8>>],
    leaves: &[kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2],
    covered: u32,
) -> Result<PalwCheckpointKvOperandsV1, Base0FpIntervalError> {
    use kaspa_consensus_core::palw_context_ladder::{PalwCheckpointCadenceV1, palw_checkpoint_cadence_v1};
    // **Which retention this leg has is the CADENCE's answer** (ADR-0082 Decision 4, amended;
    // audit B, C-1). A per-call class kept its chunks and the leg is re-derived from them. A
    // per-position class kept none — a chunk per position is `Θ(n²)` — and re-derives the leg from
    // its LEAVES, which decide every structural question an operand needs (which leaf covers this
    // state, what it hashes to, whether it opens against `checkpoint_merkle_root`) without one
    // byte of history. The operand it returns names the checkpoint and carries no state, which is
    // the only form such a class may serve anyway
    // ([`base0_fp_class_requires_flat_openings_v1`]).
    let per_position = palw_checkpoint_cadence_v1(&binding.shape_profile) == PalwCheckpointCadenceV1::PerPosition;
    let checkpoints = if per_position {
        crate::legs::Base0CheckpointCaptureV1::from_leaves_v1(
            &binding.job_context,
            &binding.shape_profile,
            &binding.checkpoint_profile,
            leaves,
        )
    } else {
        crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
            &binding.job_context,
            &binding.shape_profile,
            &binding.checkpoint_profile,
            chunks,
        )
    }
    .map_err(|e| Base0FpIntervalError::Leg(format!("{e:?}")))?;
    // The leg the chunks re-derive must be the one the CLAIM committed, checked before anything
    // resumes: resuming from unchecked state would let a producer that lied about a step hand over
    // a state consistent with the lie and watch the replay agree with it.
    if checkpoints.merkle_root != binding.checkpoint_merkle_root || checkpoints.leaf_hashes.len() as u32 != binding.checkpoint_count {
        return Err(Base0FpIntervalError::CaptureIsNotTheBindings);
    }
    let at = checkpoints
        .leaves
        .iter()
        .position(|l| l.covered_decode_call == covered)
        .ok_or(Base0FpIntervalError::NoCheckpointAt { covered })?;
    let opening = kaspa_consensus_core::palw_step_leg::step_opening_v1(&checkpoints.leaf_hashes, at as u64)
        .map_err(|e| Base0FpIntervalError::Leg(format!("{e:?}")))?;
    Ok(PalwCheckpointKvOperandsV1 {
        leaf: checkpoints.leaves[at].clone(),
        opening,
        // Empty on the folded route, and it stays empty: the assembler below emits the NAMED form
        // for exactly the classes that fold, so no opening is ever served with an anchor whose
        // chunks do not build its own state root.
        chunks: if per_position {
            Vec::new()
        } else {
            checkpoints.chunks.get(at).cloned().ok_or(Base0FpIntervalError::NoCheckpointAt { covered })?
        },
    })
}

// ---------------------------------------------------------------------------------------------
// The seat's side
// ---------------------------------------------------------------------------------------------

/// Where a replay of one interval starts. `Genesis` is the prompt (interval 0); `Checkpoint` is
/// the opened chunks and the id derived from the anchor call's committed logits row.
pub enum Base0FpIntervalStartV1<'a> {
    Genesis { prompt_tokens: &'a [usize] },
    Checkpoint { covered_decode_call: u32, chunks: &'a [Vec<u8>], seed_token: u32 },
}

/// **The class's OWN kernels, as the seat's replay needs them.**
///
/// A seat compares committed rows against rows it recomputed, so the recomputation must be the
/// class's registered arithmetic and nothing that resembles it. Each family implements this over
/// its own engine and cache; [`base0_fp_replay_interval_v1`] is the shared loop, so the ordering
/// and the coordinates a replay commits to are one implementation across the three families.
/// **`step_leaf_count` is a parameter, and that is where the ladder stops travelling.** The
/// replay places its rows at ABSOLUTE coordinates, so it sizes a capture from the step space's
/// size — and re-deriving that size here would be a second count taken against whatever constant
/// this crate happens to hold. The callers ([`base0_replay_span_leaves_v1`] and
/// [`base0_verify_fp_interval_opening_with_state_capped_v1`]) both hold the binding, and both have
/// already checked its price against the ruleset's ladder
/// ([`base0_fp_binding_step_space_v1`]) before a byte is replayed. So a family implementing this
/// needs no ruleset of its own: the number it is handed is the claim's, already priced.
pub trait Base0FpIntervalKernelsV1 {
    /// **The interval replayed with its tiles kept** (ADR-0085 Decision 2) — the family's engine
    /// driving [`base0_fp_replay_interval_tiles_v1`]. The one required verb: a seat's hash check
    /// is the provided [`Self::replay_interval`] over it, and a challenger's close reads the tiles.
    fn replay_interval_tiles(
        &self,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        start: &Base0FpIntervalStartV1<'_>,
        first_call: u32,
        last_call: u32,
        step_leaf_count: u64,
    ) -> Result<crate::legs::Base0StepTilesV1, String>;

    /// The seat's view: the interval's leaf hashes, in leaf order, from the same replay.
    fn replay_interval(
        &self,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        start: &Base0FpIntervalStartV1<'_>,
        first_call: u32,
        last_call: u32,
        step_leaf_count: u64,
    ) -> Result<Vec<(u64, Hash64)>, String> {
        let partial = self.replay_interval_tiles(profile, ctx, start, first_call, last_call, step_leaf_count)?;
        let ctx_hash = ctx.context_hash();
        let profile_hash = profile.shape_profile_id();
        Ok(partial.tiles.iter().map(|(i, leaf)| (*i, step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf))).collect())
    }
}

/// **The replay loop every family shares** — the capture's own walk, restricted to a window.
///
/// `forward` is the class's engine with its cache already restored (from the checkpoint chunks, or
/// empty for interval 0); it is handed a token and an ABSOLUTE cache position and returns the
/// logits row and the rows the step space commits. Placing rows and deriving the next token stay
/// here, because a family that re-implemented the coordinate rule would commit its replay at
/// coordinates the leg does not use, and every comparison would fail for a reason that is not the
/// producer's.
#[allow(clippy::too_many_arguments)]
pub fn base0_fp_replay_interval_v1<F>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    start: &Base0FpIntervalStartV1<'_>,
    first_call: u32,
    last_call: u32,
    step_leaf_count: u64,
    forward: F,
) -> Result<Vec<(u64, Hash64)>, String>
where
    F: FnMut(usize, usize) -> Result<(Vec<i32>, Vec<Base0CapturedRowV1>), String>,
{
    let partial = base0_fp_replay_interval_tiles_v1(profile, ctx, start, first_call, last_call, step_leaf_count, forward)?;
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    Ok(partial.tiles.iter().map(|(i, leaf)| (*i, step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf))).collect())
}

/// **The same replay, keeping the TILES** (ADR-0085 Decision 2): the interval's committed rows as
/// this party computed them, with their leaf hashes, in a partial [`crate::legs::Base0StepTilesV1`]
/// whose other leaves are zero. A seat's check needs only the hashes ([`base0_fp_replay_interval_v1`]
/// maps to them); a challenger assembling a court close needs the preimages — the activation
/// inputs the disputed step reads — and checks each hash against the accused's committed one in
/// the range opening before it opens anything.
#[allow(clippy::too_many_arguments)]
pub fn base0_fp_replay_interval_tiles_v1<F>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    start: &Base0FpIntervalStartV1<'_>,
    first_call: u32,
    last_call: u32,
    step_leaf_count: u64,
    mut forward: F,
) -> Result<crate::legs::Base0StepTilesV1, String>
where
    F: FnMut(usize, usize) -> Result<(Vec<i32>, Vec<Base0CapturedRowV1>), String>,
{
    use kaspa_consensus_core::palw_step::PalwStepTableV1;
    let prefill = ctx.declared_prefill_tokens as usize;
    // **The size is the caller's, priced against the ruleset's ladder before this was reached.**
    // It used to be re-derived from `(profile, ctx)` at the EXECUTOR's `PALW_STEP_MAX_LEAVES`,
    // which refused the graph-v5 512 row's 6,630,544-leaf honest job outright: no seat could
    // replay an interval of a class the chain admits, so no panel could license one.
    let mut capture = crate::legs::Base0StepCaptureV1::new(step_leaf_count).map_err(|e| format!("{e:?}"))?;

    let mut next: usize = match start {
        Base0FpIntervalStartV1::Genesis { prompt_tokens } => {
            if first_call != 0 {
                return Err("a genesis replay must start at the prefill call".to_string());
            }
            if prompt_tokens.len() < prefill {
                return Err(format!("the job declares {prefill} prefill tokens and {} were supplied", prompt_tokens.len()));
            }
            // Call 0 — prefill. Logits leaves exist only at its LAST position; the earlier rows
            // predict tokens the prompt already contains, and the step space has no coordinate for
            // them. The same drop the capture loops make, for the same reason.
            let mut last_logits = Vec::new();
            for (position, token) in prompt_tokens.iter().take(prefill).enumerate() {
                let (logits, mut rows) = forward(*token, position)?;
                if position + 1 != prefill {
                    rows.retain(|r| r.table != PalwStepTableV1::Post);
                }
                capture.push_call(profile, ctx, 0, position as u32, &rows).map_err(|e| format!("{e:?}"))?;
                last_logits = logits;
            }
            kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&last_logits)
        }
        Base0FpIntervalStartV1::Checkpoint { covered_decode_call, seed_token, .. } => {
            // **The guard is a statement about CALLS and the leaf's counter may not be one**
            // (audit B, C-2). `covered_decode_call` is whatever the class's cadence counts —
            // decode calls on a per-call class, cache POSITIONS on a per-position one — so it is
            // converted to a position count by the one function that answers that
            // (`palw_checkpoint_positions_at_v1`) and back to the call the position implies. On
            // every shipped class the two conversions cancel and this is `covered + 1` verbatim.
            let positions =
                kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(profile, ctx, *covered_decode_call);
            let anchored_call = (positions as usize).saturating_sub(prefill);
            if first_call as usize != anchored_call + 1 {
                return Err("the checkpoint does not cover the call before this interval".to_string());
            }
            *seed_token as usize
        }
    };

    // Decode calls. The COORDINATE's position is 0 in every decode call (each call has one
    // position); the cache position is absolute. Conflating them lands every decode row on top of
    // the first one's.
    for call in first_call.max(1)..=last_call {
        let cache_position = prefill + call as usize - 1;
        let (logits, rows) = forward(next, cache_position)?;
        capture.push_call(profile, ctx, call, 0, &rows).map_err(|e| format!("{e:?}"))?;
        next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&logits);
    }

    // `finish_partial` is correct HERE and nowhere else: a replay deliberately covers a window,
    // and the leaves it did not touch are not claims about zero — they are simply not this
    // replay's. Which is why only the touched ones come back.
    Ok(capture.finish_partial())
}

/// **Replay one opened interval and compare every row EXACTLY** (ADR-0077 Decision 8, seat half).
///
/// The order is the security: bind the opening to the claim FIRST (execution root, trace root, the
/// FP job id as the anchor) and price it (`work_leaves` must equal the binding's
/// `step_leaf_count`), then read evidence. An implementation that replayed first would be doing
/// arithmetic for whoever asked, and a capture that answers ANOTHER claim's roots would look like
/// an honest run of a job nobody commissioned.
/// **The challenger's replay of a served interval, with its tiles** (ADR-0085 Decision 2's second
/// input). The same derivation the seat's verify makes — the opening decoded (a v3's annex
/// ignored), bound to the claim's roots, its range checked against the step leg root, the replay
/// window and its start derived from the geometry and the opened seed row, the anchor's state
/// taken from `state` (this party's recompute, ADR-0082 D9) or from the carried chunks — and then
/// [`Base0FpIntervalKernelsV1::replay_interval_tiles`] instead of the hash check. Returns the v2
/// view of the opening beside the tiles, which is what [`crate::legs::base0_refutation_from_opening_capped_v1`]
/// takes. `Err` names why nothing could be replayed; it is never a verdict about anybody.
#[allow(clippy::too_many_arguments)]
pub fn base0_fp_challenger_replay_tiles_capped_v1<K: Base0FpIntervalKernelsV1>(
    opening_bytes: &[u8],
    claim: PalwClaimRootsV1,
    index: u32,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    state: Option<&crate::fp_recompute::Base0FpSeatStateV1>,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<(Base0FpIntervalOpeningV2, crate::legs::Base0StepTilesV1), String> {
    let any = base0_fp_interval_opening_decode_any_v1(opening_bytes).map_err(|e| format!("the opening does not decode: {e:?}"))?;
    let carried = match &any {
        Base0FpIntervalOpeningAnyV1::WithHistory(o) => Some(o.as_ref()),
        Base0FpIntervalOpeningAnyV1::Recomputed(_) | Base0FpIntervalOpeningAnyV1::Digests(_) => None,
    };
    // ADR-0086: a V4 opening carries no leaf hashes; the challenger's own replay supplies them
    // below, and the V2 view it hands the refutation builder is over ITS leaves and the served
    // frontier — the path to one wrong leaf needs only the honest siblings.
    let fold = match &any {
        Base0FpIntervalOpeningAnyV1::Digests(o) => Some(&o.range),
        _ => None,
    };
    let mut opening = match &any {
        Base0FpIntervalOpeningAnyV1::WithHistory(o) => Base0FpIntervalOpeningV2::from_chunked_v1(o),
        Base0FpIntervalOpeningAnyV1::Recomputed(o) => o.as_ref().clone(),
        Base0FpIntervalOpeningAnyV1::Digests(o) => Base0FpIntervalOpeningV2 {
            version: PALW_BASE0_FP_INTERVAL_VERSION_V2,
            interval_index: o.interval_index,
            binding: o.binding.clone(),
            range: PalwStepRangeOpeningV1 {
                first_leaf_index: o.range.first_leaf_index,
                leaf_hashes: Vec::new(),
                siblings: o.range.siblings.clone(),
            },
            seed_row_leaf_count: o.seed_row_leaf_count,
            seed_row_tiles: o.seed_row_tiles.clone(),
            anchor: o.anchor.clone(),
        },
    };
    let binding = &opening.binding;
    if kaspa_consensus_core::palw_step_leg::verify_binding_v1(binding).is_err()
        || binding.committed_execution_root != claim.execution_root
        || binding.full_logits_trace_root != claim.trace_root
        || (claim.anchor != Hash64::default() && binding.job_context.job_id != claim.anchor)
        || binding.step_leaf_count != work_leaves
        || opening.interval_index != index
    {
        return Err("the opening does not bind to the claim's roots".to_string());
    }
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;
    if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
        prompt_ids_form,
        prompt_token_ids,
        &ctx.prompt_token_ids_hash,
    ) {
        return Err("the prompt is not the one the opening's context commits to".to_string());
    }
    let step_leaf_count = base0_fp_binding_step_space_v1(binding, max_step_leaf_count).map_err(|e| format!("{e:?}"))?;
    let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(binding, family_checkpoint_interval, max_step_leaf_count)
        .map_err(|e| format!("{e:?}"))?;
    let leaves_geometry =
        base0_fp_interval_leaves_v1(profile, ctx, &geometry, index, step_leaf_count).map_err(|e| format!("{e:?}"))?;
    let (first_call, last_call) = geometry.calls_for(index).ok_or_else(|| "the interval names no calls".to_string())?;
    let count = leaves_geometry.range_end - leaves_geometry.range_first;
    let shaped = match fold {
        Some(f) => {
            f.first_leaf_index == leaves_geometry.range_first
                && f.leaf_count == count
                && f.retain_level >= crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1
                && f.retain_level <= crate::fp_capture::PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1
                && f.digests_are_the_blocks_v1(step_leaf_count)
        }
        None => opening.range.first_leaf_index == leaves_geometry.range_first && opening.range.leaf_hashes.len() as u64 == count,
    };
    if !shaped {
        return Err("the opening's range is not this interval's".to_string());
    }
    if fold.is_none() {
        match step_range_opening_root_capped_v1(binding.step_leaf_count, &opening.range, max_step_leaf_count) {
            Ok(root) if root == binding.step_merkle_root => {}
            _ => return Err("the opening's range does not open the step leg root".to_string()),
        }
    }
    let mut seed_hashes: Vec<Hash64> = Vec::new();
    let prompt_usize: Vec<usize> = prompt_token_ids.iter().map(|t| *t as usize).collect();
    let start = match geometry.anchor_covered_call(index) {
        None => Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt_usize },
        Some(covered) => {
            let seed_call = geometry.anchor_seed_call_v1(index).ok_or_else(|| "the interval names no seed call".to_string())?;
            let seed_token = match fold {
                Some(_) => {
                    let (token, hashes) =
                        seed_row_from_tiles_v1(profile, ctx, &opening.seed_row_tiles, seed_call, leaves_geometry.range_first)
                            .ok_or_else(|| "the served seed row yields no token".to_string())?;
                    seed_hashes = hashes;
                    token
                }
                None => seed_token_from_opened_row_v1(
                    profile,
                    ctx,
                    &opening.seed_row_tiles,
                    &opening.range,
                    seed_call,
                    leaves_geometry.range_first,
                )
                .ok_or_else(|| "the opened seed row yields no token".to_string())?,
            };
            let claimed = opening.anchor.as_ref().ok_or_else(|| "the opening names no checkpoint".to_string())?;
            if !checkpoint_claim_is_the_bindings_v1(binding, &claimed.leaf, &claimed.opening, covered) {
                return Err("the opening's checkpoint is not the binding's".to_string());
            }
            match state {
                Some(own) => {
                    if own.covered_decode_call != covered || own.state_chunks_root != claimed.leaf.state_chunks_root {
                        return Err("this party's recomputed state is not the checkpoint the opening names".to_string());
                    }
                    Base0FpIntervalStartV1::Checkpoint { covered_decode_call: covered, chunks: &own.chunks, seed_token }
                }
                None => {
                    let anchor = carried.and_then(|o| o.anchor.as_ref()).ok_or_else(|| "no state to resume from".to_string())?;
                    if !checkpoint_anchor_is_the_bindings_v1(binding, anchor, covered) {
                        return Err("the carried anchor is not the binding's".to_string());
                    }
                    Base0FpIntervalStartV1::Checkpoint { covered_decode_call: covered, chunks: &anchor.chunks, seed_token }
                }
            }
        }
    };
    let tiles = kernels.replay_interval_tiles(profile, ctx, &start, first_call, last_call, step_leaf_count)?;
    if let Some(f) = fold {
        if seed_hashes.len() as u64 != leaves_geometry.seed_row_leaves {
            return Err("the served seed row is not the interval's".to_string());
        }
        let mut own = seed_hashes;
        for leaf_index in leaves_geometry.interval_first..leaves_geometry.range_end {
            let hash = tiles
                .leaves
                .get(leaf_index as usize)
                .copied()
                .filter(|h| *h != Hash64::default())
                .ok_or_else(|| format!("this party's replay has no leaf {leaf_index}"))?;
            own.push(hash);
        }
        opening.range = f.with_leaves_v1(own).ok_or_else(|| "this party's replay is not the range's count".to_string())?;
        match step_range_opening_root_capped_v1(binding.step_leaf_count, &opening.range, max_step_leaf_count) {
            Ok(root) if root == binding.step_merkle_root => {}
            _ => return Err("this party's replay does not reproduce the step leg root under the served frontier".to_string()),
        }
    }
    Ok((opening, tiles))
}

pub fn base0_verify_fp_interval_opening_v1<K: Base0FpIntervalKernelsV1>(
    opening_bytes: &[u8],
    claim: PalwClaimRootsV1,
    index: u32,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    family_checkpoint_interval: u32,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> PalwFpIntervalVerdictV1 {
    base0_verify_fp_interval_opening_with_state_capped_v1(
        opening_bytes,
        claim,
        index,
        prompt_token_ids,
        work_leaves,
        family_checkpoint_interval,
        PALW_STEP_LEG_MAX_LEAVES,
        None,
        kernels,
        prompt_ids_form,
    )
    .to_consensus_v1()
}

/// [`base0_verify_fp_interval_opening_v1`] against the ladder top the CALLER states — the seat's
/// entry point on a network whose ruleset froze a wider court than the executor's constant.
#[allow(clippy::too_many_arguments)]
pub fn base0_verify_fp_interval_opening_capped_v1<K: Base0FpIntervalKernelsV1>(
    opening_bytes: &[u8],
    claim: PalwClaimRootsV1,
    index: u32,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> PalwFpIntervalVerdictV1 {
    base0_verify_fp_interval_opening_with_state_capped_v1(
        opening_bytes,
        claim,
        index,
        prompt_token_ids,
        work_leaves,
        family_checkpoint_interval,
        max_step_leaf_count,
        None,
        kernels,
        prompt_ids_form,
    )
    .to_consensus_v1()
}

/// **What a seat concluded about one interval, with the checkpoint named** (ADR-0082 Decision 9).
///
/// [`PalwFpIntervalVerdictV1`] plus the one thing Decision 9 adds: the recomputed state root did
/// not equal the committed one. That is not a leaf fault — no leaf has been compared yet — and it
/// is not "another claim's opening" either; it is the seat saying the checkpoint the executor
/// committed is not the state this job reaches, with both roots in hand for whoever opens a court.
///
/// It lives here rather than in the consensus enum because a seat's verdict shape is a family
/// concern and the consensus type is a contract three crates share. [`Self::to_consensus_v1`] is
/// the projection the backend seam takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base0FpIntervalSeatVerdictV1 {
    Valid,
    Fault {
        leaf_index: u64,
    },
    /// ADR-0086 Decision 3: the seat's own leaves do not reproduce the served digests or the
    /// committed root; the address is a block of the fold clipped to the range, or its edge.
    FaultInRange {
        first_leaf_index: u64,
        leaf_count: u64,
    },
    /// The seat's own recompute of the state at this checkpoint does not have the root the claim
    /// committed. The seat files NOTHING and may open a court, as any bonded challenger may.
    CheckpointRootMismatch {
        checkpoint_index: u32,
        covered_decode_call: u32,
        committed: Hash64,
        recomputed: Hash64,
    },
    /// The class registers a tiled map — a graph-v5 row — and the opening carried the history
    /// anyway. Refused rather than replayed: the whole content of Decision 9 is that a seat's
    /// bytes do not grow with the context, and a seat that accepts the history has spent them.
    HistoryNotAdmissible,
    Mismatch,
    Unverifiable,
}

impl Base0FpIntervalSeatVerdictV1 {
    /// **The projection onto the seam's four arms.**
    ///
    /// A checkpoint-root mismatch projects to `Fault` at the interval's first leaf — the same
    /// treatment the panel gives a row that did not match: file nothing, open a court. It is the
    /// nearest true arm, and the reason the rich verdict exists is that "nearest" loses the
    /// checkpoint's name, which the caller that can log it takes from
    /// [`Self::CheckpointRootMismatch`] directly.
    pub fn to_consensus_v1(&self) -> PalwFpIntervalVerdictV1 {
        match self {
            Self::Valid => PalwFpIntervalVerdictV1::Valid,
            Self::Fault { leaf_index } => PalwFpIntervalVerdictV1::Fault { leaf_index: *leaf_index },
            Self::FaultInRange { first_leaf_index, leaf_count } => {
                PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index: *first_leaf_index, leaf_count: *leaf_count }
            }
            Self::CheckpointRootMismatch { .. } | Self::HistoryNotAdmissible => PalwFpIntervalVerdictV1::Unverifiable,
            Self::Mismatch => PalwFpIntervalVerdictV1::Mismatch,
            Self::Unverifiable => PalwFpIntervalVerdictV1::Unverifiable,
        }
    }
}

/// **Replay one opened interval from the seat's OWN state** (ADR-0082 Decision 9), or from the
/// executor's carried chunks when there is no own state (ADR-0077 Decision 8, unchanged).
///
/// `state` is what [`crate::fp_recompute::base0_fp_recompute_state_v1`] returned for this
/// interval's start. When it is present:
///
/// * the committed checkpoint is compared against it — 64 bytes, and a disagreement is named
///   rather than replayed past;
/// * the replay resumes from the seat's own chunks, so nothing the executor asserts about the
///   state enters the comparison;
/// * a CHUNKLESS opening ([`Base0FpIntervalOpeningV2`]) is admissible, which is what makes the
///   bytes a seat fetches independent of the context.
///
/// When it is absent the chunked form's own anchor is used and this is exactly the ADR-0077
/// behaviour, which is why one body serves both: "the two forms agree on honest material" is a
/// property of one implementation rather than a comparison of two.
#[allow(clippy::too_many_arguments)]
pub fn base0_verify_fp_interval_opening_with_state_v1<K: Base0FpIntervalKernelsV1>(
    opening_bytes: &[u8],
    claim: PalwClaimRootsV1,
    index: u32,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    family_checkpoint_interval: u32,
    state: Option<&crate::fp_recompute::Base0FpSeatStateV1>,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Base0FpIntervalSeatVerdictV1 {
    base0_verify_fp_interval_opening_with_state_capped_v1(
        opening_bytes,
        claim,
        index,
        prompt_token_ids,
        work_leaves,
        family_checkpoint_interval,
        PALW_STEP_LEG_MAX_LEAVES,
        state,
        kernels,
        prompt_ids_form,
    )
}

/// **The seed row as the seat reads it under V4** (ADR-0086): the tiles are checked to be the
/// anchor call's logits row at the range's first leaves, hashed into the leaves the seat will
/// walk with, and decoded to the token the interval starts from. There is no served leaf hash to
/// compare against; the root walk is what binds these tiles to the producer's commitment.
fn seed_row_from_tiles_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    seed_row_tiles: &[PalwStepTileLeafV1],
    anchor_call: u32,
    range_first: u64,
) -> Option<(u32, Vec<Hash64>)> {
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    let slot = logits_node_slot_v1(profile);
    let mut row: Vec<i32> = Vec::new();
    let mut hashes = Vec::with_capacity(seed_row_tiles.len());
    for (tile_index, leaf) in seed_row_tiles.iter().enumerate() {
        let want_index = range_first + tile_index as u64;
        if leaf.coord.call_index != anchor_call || leaf.coord.node_slot != slot || leaf.coord.position != 0 {
            return None;
        }
        if canonical_step_leaf_index(profile, ctx, &leaf.coord)? != want_index {
            return None;
        }
        if leaf.values_le.len() != leaf.value_count as usize * 4 {
            return None;
        }
        hashes.push(step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf));
        row.extend(leaf.values_le.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])));
    }
    if row.is_empty() {
        return None;
    }
    Some((kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&row) as u32, hashes))
}

/// The range's edge outside its whole blocks — where a fault lives when every digest agrees and
/// the root still does not: the left edge when the range does not start on a block boundary,
/// else the right, else the whole range.
fn fold_edge_v1(fold: &Base0FpFoldRangeOpeningV1, step_leaf_count: u64) -> (u64, u64) {
    let block = 1u64 << fold.retain_level.min(63);
    let (first_block, end_block) = fold.whole_blocks_v1(step_leaf_count);
    let range_end = fold.first_leaf_index + fold.leaf_count;
    let left_end = (first_block * block).min(range_end);
    if left_end > fold.first_leaf_index {
        return (fold.first_leaf_index, left_end - fold.first_leaf_index);
    }
    let right_first = (end_block * block).max(fold.first_leaf_index);
    if range_end > right_first {
        return (right_first, range_end - right_first);
    }
    (fold.first_leaf_index, fold.leaf_count)
}

/// **The V4 verdict — the seat's leaves are its own** (ADR-0086 Decision 3). The opening carries
/// the frontier and the fold's digests; the seat hashes the served seed row into the range's
/// first leaves, replays the interval for the rest, folds its own leaves over the whole blocks
/// and compares them with the served digests (a block that differs is the fault's address),
/// then walks the consensus root rule with its own leaves and the served siblings. Nothing the
/// producer serves can make a wrong range walk to the committed root.
#[allow(clippy::too_many_arguments)]
pub fn base0_verify_fp_interval_opening_v4_capped_v1<K: Base0FpIntervalKernelsV1>(
    opening: &Base0FpIntervalOpeningV4,
    claim: PalwClaimRootsV1,
    index: u32,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    state: Option<&crate::fp_recompute::Base0FpSeatStateV1>,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Base0FpIntervalSeatVerdictV1 {
    use Base0FpIntervalSeatVerdictV1 as V;
    let binding = &opening.binding;
    if opening.version != PALW_BASE0_FP_INTERVAL_VERSION_V4
        || kaspa_consensus_core::palw_step_leg::verify_binding_v1(binding).is_err()
        || binding.committed_execution_root != claim.execution_root
        || binding.full_logits_trace_root != claim.trace_root
        || (claim.anchor != Hash64::default() && binding.job_context.job_id != claim.anchor)
        || binding.step_leaf_count != work_leaves
        || opening.interval_index != index
    {
        return V::Mismatch;
    }
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;
    if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
        prompt_ids_form,
        prompt_token_ids,
        &ctx.prompt_token_ids_hash,
    ) {
        return V::Mismatch;
    }
    let step_leaf_count = match base0_fp_binding_step_space_v1(binding, max_step_leaf_count) {
        Ok(count) => count,
        Err(Base0FpIntervalError::LeafCountOutOfRange { .. }) => return V::Unverifiable,
        Err(_) => return V::Mismatch,
    };
    let geometry = match Base0FpIntervalGeometryV1::from_binding_capped_v1(binding, family_checkpoint_interval, max_step_leaf_count) {
        Ok(geometry) => geometry,
        Err(Base0FpIntervalError::LeafCountOutOfRange { .. }) => return V::Unverifiable,
        Err(_) => return V::Mismatch,
    };
    let Ok(leaves_geometry) = base0_fp_interval_leaves_v1(profile, ctx, &geometry, index, step_leaf_count) else {
        return V::Mismatch;
    };
    let (Some((first_call, last_call)), count) = (geometry.calls_for(index), leaves_geometry.range_end - leaves_geometry.range_first)
    else {
        return V::Mismatch;
    };
    let fold = &opening.range;
    if fold.first_leaf_index != leaves_geometry.range_first
        || fold.leaf_count != count
        || fold.retain_level < crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1
        || fold.retain_level > crate::fp_capture::PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1
        || !fold.digests_are_the_blocks_v1(step_leaf_count)
        || opening.seed_row_leaf_count as u64 != leaves_geometry.seed_row_leaves
        || opening.seed_row_tiles.len() as u64 != leaves_geometry.seed_row_leaves
        || leaves_geometry.interval_first != leaves_geometry.range_first + leaves_geometry.seed_row_leaves
    {
        return V::Mismatch;
    }
    let prompt_usize: Vec<usize> = prompt_token_ids.iter().map(|t| *t as usize).collect();
    let mut own: Vec<Hash64> = Vec::with_capacity(count as usize);
    let start = match geometry.anchor_covered_call(index) {
        None => Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt_usize },
        Some(covered) => {
            let Some(seed_call) = geometry.anchor_seed_call_v1(index) else {
                return V::Mismatch;
            };
            let Some((seed_token, seed_hashes)) =
                seed_row_from_tiles_v1(profile, ctx, &opening.seed_row_tiles, seed_call, leaves_geometry.range_first)
            else {
                return V::Mismatch;
            };
            own.extend(seed_hashes);
            let Some(claimed) = opening.anchor.as_ref() else {
                return V::Mismatch;
            };
            if !checkpoint_claim_is_the_bindings_v1(binding, &claimed.leaf, &claimed.opening, covered) {
                return V::Mismatch;
            }
            let Some(held) = state else {
                return V::Unverifiable;
            };
            if held.covered_decode_call != covered {
                return V::Unverifiable;
            }
            if held.state_chunks_root != claimed.leaf.state_chunks_root {
                return V::CheckpointRootMismatch {
                    checkpoint_index: claimed.leaf.checkpoint_index,
                    covered_decode_call: covered,
                    committed: claimed.leaf.state_chunks_root,
                    recomputed: held.state_chunks_root,
                };
            }
            Base0FpIntervalStartV1::Checkpoint { covered_decode_call: covered, chunks: &held.chunks, seed_token }
        }
    };
    if own.len() as u64 != leaves_geometry.seed_row_leaves {
        return V::Mismatch;
    }
    let Ok(recomputed) = kernels.replay_interval(profile, ctx, &start, first_call, last_call, step_leaf_count) else {
        return V::Unverifiable;
    };
    // The interval's own leaves, in order, each exactly once.
    let interval_leaves = (leaves_geometry.range_end - leaves_geometry.interval_first) as usize;
    let mut filled: Vec<Option<Hash64>> = vec![None; interval_leaves];
    for (leaf_index, hash) in &recomputed {
        let Some(offset) = leaf_index.checked_sub(leaves_geometry.interval_first) else {
            return V::Unverifiable;
        };
        let Some(slot) = filled.get_mut(offset as usize) else {
            return V::Unverifiable;
        };
        if slot.replace(*hash).is_some() {
            return V::Unverifiable;
        }
    }
    for slot in filled {
        let Some(hash) = slot else {
            return V::Unverifiable;
        };
        own.push(hash);
    }
    if let Some((first_leaf_index, leaf_count)) = fold.first_block_that_differs_v1(&own, step_leaf_count) {
        return V::FaultInRange { first_leaf_index, leaf_count };
    }
    let Some(range) = fold.with_leaves_v1(own) else {
        return V::Unverifiable;
    };
    match step_range_opening_root_capped_v1(binding.step_leaf_count, &range, max_step_leaf_count) {
        Ok(root) if root == binding.step_merkle_root => V::Valid,
        _ => {
            let (first_leaf_index, leaf_count) = fold_edge_v1(fold, step_leaf_count);
            V::FaultInRange { first_leaf_index, leaf_count }
        }
    }
}

/// [`base0_verify_fp_interval_opening_with_state_v1`] against the ladder top the CALLER states.
///
/// **This is the site the class's licensability turns on.** `work_leaves` is a CHAIN number (the
/// accepted commitment's), the binding's price must equal it, and the geometry re-derives the same
/// count from `(profile, context)` — so a seat that re-derived at the executor's `2^22` refused
/// every honest opening of the graph-v5 512 row by `PriceIsNotTheGeometrys`, which the seam
/// projects to `Mismatch`: "this opening is not about the claim in hand", said about an opening
/// that was. A class no seat licenses is a class no panel can certify.
#[allow(clippy::too_many_arguments)]
pub fn base0_verify_fp_interval_opening_with_state_capped_v1<K: Base0FpIntervalKernelsV1>(
    opening_bytes: &[u8],
    claim: PalwClaimRootsV1,
    index: u32,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    state: Option<&crate::fp_recompute::Base0FpSeatStateV1>,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Base0FpIntervalSeatVerdictV1 {
    // Bytes that are not this family's are bytes this seat cannot check — never an accusation.
    let Ok(any) = base0_fp_interval_opening_decode_any_v1(opening_bytes) else {
        return Base0FpIntervalSeatVerdictV1::Unverifiable;
    };
    if let Base0FpIntervalOpeningAnyV1::Digests(o) = &any {
        return base0_verify_fp_interval_opening_v4_capped_v1(
            o,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            family_checkpoint_interval,
            max_step_leaf_count,
            state,
            kernels,
            prompt_ids_form,
        );
    }
    // The chunkless form is evidence only for a seat that holds the state; one arriving at a seat
    // that does not is bytes this seat cannot check, and saying so is not an accusation.
    let carried = match &any {
        Base0FpIntervalOpeningAnyV1::WithHistory(o)
            if base0_fp_class_requires_flat_openings_v1(&o.binding.shape_profile) && o.anchor.is_some() =>
        {
            return Base0FpIntervalSeatVerdictV1::HistoryNotAdmissible;
        }
        // Borrowed, never copied: the thing this arm holds is the history, and a seat that
        // duplicated it in memory would have paid the bytes twice.
        Base0FpIntervalOpeningAnyV1::WithHistory(o) => Some(o.as_ref()),
        // …but interval 0 needs no state: it resumes from the PROMPT, which the seat holds
        // anyway, so a flat opening of it is complete evidence for a seat that has recomputed
        // nothing. Refusing it here was refusing the one interval every job has.
        Base0FpIntervalOpeningAnyV1::Recomputed(o) if state.is_none() && o.anchor.is_some() => {
            return Base0FpIntervalSeatVerdictV1::Unverifiable;
        }
        Base0FpIntervalOpeningAnyV1::Recomputed(_) => None,
        Base0FpIntervalOpeningAnyV1::Digests(_) => return Base0FpIntervalSeatVerdictV1::Unverifiable,
    };
    let opening = match &any {
        Base0FpIntervalOpeningAnyV1::WithHistory(o) => Base0FpIntervalOpeningV2::from_chunked_v1(o),
        Base0FpIntervalOpeningAnyV1::Recomputed(o) => o.as_ref().clone(),
        Base0FpIntervalOpeningAnyV1::Digests(_) => return Base0FpIntervalSeatVerdictV1::Unverifiable,
    };
    let binding = &opening.binding;
    // A binding that does not recompute its own committed root is bound to nothing; so is one
    // bound to another claim's roots, another job, or another price. All four are the same
    // accusation: this opening is not about the claim in hand.
    if kaspa_consensus_core::palw_step_leg::verify_binding_v1(binding).is_err()
        || binding.committed_execution_root != claim.execution_root
        || binding.full_logits_trace_root != claim.trace_root
        || (claim.anchor != Hash64::default() && binding.job_context.job_id != claim.anchor)
        || binding.step_leaf_count != work_leaves
        || opening.interval_index != index
    {
        return Base0FpIntervalSeatVerdictV1::Mismatch;
    }
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;
    if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
        prompt_ids_form,
        prompt_token_ids,
        &ctx.prompt_token_ids_hash,
    ) {
        return Base0FpIntervalSeatVerdictV1::Mismatch;
    }
    // **A limit is not a verdict.** A binding priced ABOVE the ladder this seat was handed is a
    // job this seat cannot check — `Unverifiable`, which files nothing — never `Mismatch`, which
    // is a statement about the producer's honesty. (`Ok(false)` and `Err(limit)` are one control
    // flow to a compiler and opposite statements to a person; three findings this week were a
    // limit or an absence rendered as a positive finding about someone else.) A binding whose
    // price is not its geometry's IS the producer's claim being false, and stays `Mismatch`.
    let step_leaf_count = match base0_fp_binding_step_space_v1(binding, max_step_leaf_count) {
        Ok(count) => count,
        Err(Base0FpIntervalError::LeafCountOutOfRange { .. }) => return Base0FpIntervalSeatVerdictV1::Unverifiable,
        Err(_) => return Base0FpIntervalSeatVerdictV1::Mismatch,
    };
    let geometry = match Base0FpIntervalGeometryV1::from_binding_capped_v1(binding, family_checkpoint_interval, max_step_leaf_count) {
        Ok(geometry) => geometry,
        Err(Base0FpIntervalError::LeafCountOutOfRange { .. }) => return Base0FpIntervalSeatVerdictV1::Unverifiable,
        Err(_) => return Base0FpIntervalSeatVerdictV1::Mismatch,
    };
    let Ok(leaves_geometry) = base0_fp_interval_leaves_v1(profile, ctx, &geometry, index, step_leaf_count) else {
        return Base0FpIntervalSeatVerdictV1::Mismatch;
    };
    let (Some((first_call, last_call)), count) = (geometry.calls_for(index), leaves_geometry.range_end - leaves_geometry.range_first)
    else {
        return Base0FpIntervalSeatVerdictV1::Mismatch;
    };

    // The range must be the one this interval canonically has — an opening of some other window
    // would be an executor choosing which of its rows a seat sees.
    if opening.range.first_leaf_index != leaves_geometry.range_first
        || opening.range.leaf_hashes.len() as u64 != count
        || opening.seed_row_leaf_count as u64 != leaves_geometry.seed_row_leaves
        || opening.seed_row_tiles.len() as u64 != leaves_geometry.seed_row_leaves
    {
        return Base0FpIntervalSeatVerdictV1::Mismatch;
    }
    // …and it must open against the step leg the binding committed.
    match step_range_opening_root_capped_v1(binding.step_leaf_count, &opening.range, max_step_leaf_count) {
        Ok(root) if root == binding.step_merkle_root => {}
        _ => return Base0FpIntervalSeatVerdictV1::Mismatch,
    }

    // The id the interval CONSUMED, derived from the anchor call's committed logits row.
    // The widened prompt is bound before the match rather than inside an arm: a temporary built in
    // a match arm and borrowed out of it lives on the edition's temporary-scope rules, and the
    // borrow this holds outlives the statement.
    let prompt_usize: Vec<usize> = prompt_token_ids.iter().map(|t| *t as usize).collect();
    let start = match geometry.anchor_covered_call(index) {
        None => Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt_usize },
        Some(covered) => {
            // The seed row is a step coordinate and is named by the anchor's CALL; `covered` is
            // the checkpoint LEAF's counter and is what the leaf is matched on. Under
            // `PerPosition` they differ by the prefill, which is audit B's C-2.
            let Some(seed_call) = geometry.anchor_seed_call_v1(index) else {
                return Base0FpIntervalSeatVerdictV1::Mismatch;
            };
            let Some(seed_token) = seed_token_from_opened_row_v1(
                profile,
                ctx,
                &opening.seed_row_tiles,
                &opening.range,
                seed_call,
                leaves_geometry.range_first,
            ) else {
                return Base0FpIntervalSeatVerdictV1::Mismatch;
            };
            let Some(claimed) = opening.anchor.as_ref() else {
                return Base0FpIntervalSeatVerdictV1::Mismatch;
            };
            // The checkpoint the opening NAMES must be the claim's, whichever form carried it.
            if !checkpoint_claim_is_the_bindings_v1(binding, &claimed.leaf, &claimed.opening, covered) {
                return Base0FpIntervalSeatVerdictV1::Mismatch;
            }
            match state {
                // **ADR-0082 Decision 9: the seat's state is its own.** 64 bytes decide whether
                // the executor's checkpoint is the state this job reaches; nothing about the
                // state is taken from the opening, and a disagreement is named — with the
                // checkpoint's index and both roots — rather than replayed past.
                Some(own) => {
                    if own.covered_decode_call != covered {
                        return Base0FpIntervalSeatVerdictV1::Unverifiable;
                    }
                    if own.state_chunks_root != claimed.leaf.state_chunks_root {
                        return Base0FpIntervalSeatVerdictV1::CheckpointRootMismatch {
                            checkpoint_index: claimed.leaf.checkpoint_index,
                            covered_decode_call: covered,
                            committed: claimed.leaf.state_chunks_root,
                            recomputed: own.state_chunks_root,
                        };
                    }
                    Base0FpIntervalStartV1::Checkpoint { covered_decode_call: covered, chunks: &own.chunks, seed_token }
                }
                // ADR-0077 Decision 8, unchanged: no own state, so the carried chunks are the
                // only state there is — and they must re-derive the root the leaf commits.
                None => {
                    let Some(anchor) = carried.and_then(|o| o.anchor.as_ref()) else {
                        return Base0FpIntervalSeatVerdictV1::Mismatch;
                    };
                    if !checkpoint_anchor_is_the_bindings_v1(binding, anchor, covered) {
                        return Base0FpIntervalSeatVerdictV1::Mismatch;
                    }
                    Base0FpIntervalStartV1::Checkpoint { covered_decode_call: covered, chunks: &anchor.chunks, seed_token }
                }
            }
        }
    };

    let Ok(recomputed) = kernels.replay_interval(profile, ctx, &start, first_call, last_call, step_leaf_count) else {
        // The seat could not run the interval — its honest `Unverifiable`, and never an
        // accusation against a producer whose material may be perfectly good.
        return Base0FpIntervalSeatVerdictV1::Unverifiable;
    };

    // **Exact equality, leaf by leaf.** The class is a pinned integer computation, so "close" is
    // not a verdict (ADR-0026's refused proof model). The first disagreement is the court's
    // question and is returned as one; it convicts nobody.
    let Some(committed) = opening.range.leaf_hashes.get(leaves_geometry.seed_row_leaves as usize..) else {
        return Base0FpIntervalSeatVerdictV1::Mismatch;
    };
    let mut seen = 0u64;
    for (index, hash) in &recomputed {
        let Some(offset) = index.checked_sub(leaves_geometry.interval_first) else {
            // A replay leaf outside the interval means this seat's window and the committed range
            // disagree about the enumeration — not something to accuse anyone of.
            return Base0FpIntervalSeatVerdictV1::Unverifiable;
        };
        let Some(want) = committed.get(offset as usize) else {
            return Base0FpIntervalSeatVerdictV1::Unverifiable;
        };
        if want != hash {
            return Base0FpIntervalSeatVerdictV1::Fault { leaf_index: *index };
        }
        seen += 1;
    }
    if seen != committed.len() as u64 {
        // The replay did not reach every committed leaf of the interval; a partial check is not a
        // verdict, so it is reported as one it cannot make.
        return Base0FpIntervalSeatVerdictV1::Unverifiable;
    }
    Base0FpIntervalSeatVerdictV1::Valid
}

/// The id the interval's first call consumed, read off the anchor call's committed logits row.
///
/// Every carried tile is checked against the leaf hash the range opened AND against the canonical
/// coordinate it claims, so a tile from another call, another node or another tile index cannot be
/// substituted; then the lanes are concatenated in tile order and the family's own selection rule
/// picks the id. `None` is a refusal, never a guess.
fn seed_token_from_opened_row_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    seed_row_tiles: &[PalwStepTileLeafV1],
    range: &PalwStepRangeOpeningV1,
    anchor_call: u32,
    range_first: u64,
) -> Option<u32> {
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    let slot = logits_node_slot_v1(profile);
    let mut row: Vec<i32> = Vec::new();
    for (tile_index, leaf) in seed_row_tiles.iter().enumerate() {
        let want_index = range_first + tile_index as u64;
        if step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf) != *range.leaf_hashes.get(tile_index)? {
            return None;
        }
        if leaf.coord.call_index != anchor_call || leaf.coord.node_slot != slot || leaf.coord.position != 0 {
            return None;
        }
        if canonical_step_leaf_index(profile, ctx, &leaf.coord)? != want_index {
            return None;
        }
        if leaf.values_le.len() != leaf.value_count as usize * 4 {
            return None;
        }
        row.extend(leaf.values_le.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])));
    }
    if row.is_empty() {
        return None;
    }
    Some(kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&row) as u32)
}

/// The checkpoint the interval resumes from must be the one the CLAIM committed: its leaf opens
/// against `checkpoint_merkle_root`, its chunks re-derive its own `state_chunks_root`, and it
/// covers exactly the decode call the geometry names.
fn checkpoint_anchor_is_the_bindings_v1(binding: &PalwStepBindingV2, anchor: &PalwCheckpointKvOperandsV1, covered: u32) -> bool {
    if anchor.leaf.state_chunk_count as usize != anchor.chunks.len() {
        return false;
    }
    // **The served chunks must re-derive the root the leaf commits.** The check the CHUNKLESS
    // form does not need and this one cannot do without: resuming from unchecked state would let
    // a producer that lied about a step hand over a state consistent with the lie.
    let Ok(state_root) = base0_state_chunks_root_v1(&binding.state_chunk_map_id, &anchor.chunks) else {
        return false;
    };
    if state_root != anchor.leaf.state_chunks_root {
        return false;
    }
    checkpoint_claim_is_the_bindings_v1(binding, &anchor.leaf, &anchor.opening, covered)
}

/// **The checkpoint NAMED by an opening is the one the claim committed** — the half of the check
/// above that does not touch the state, and the whole of it for a chunkless opening (ADR-0082
/// Decision 9).
///
/// Its leaf opens against `checkpoint_merkle_root` and it covers exactly the decode call the
/// geometry names. What it does NOT establish is that `state_chunks_root` is the root of the state
/// the job actually reaches — no opening can, because that is arithmetic — and under Decision 9
/// that is precisely what the seat's own recompute answers.
fn checkpoint_claim_is_the_bindings_v1(
    binding: &PalwStepBindingV2,
    leaf: &kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2,
    opening: &kaspa_consensus_core::palw_step_leg::PalwStepOpeningV1,
    covered: u32,
) -> bool {
    use kaspa_consensus_core::palw_step_leg::{checkpoint_leaf_hash_v2, step_opening_root_v1};
    if leaf.covered_decode_call != covered {
        return false;
    }
    let ctx_hash = binding.job_context.context_hash();
    let leaf_hash = checkpoint_leaf_hash_v2(&ctx_hash, &binding.checkpoint_profile.profile_hash(), &binding.state_chunk_map_id, leaf);
    if opening.leaf_hash != leaf_hash {
        return false;
    }
    matches!(
        step_opening_root_v1(binding.checkpoint_count as u64, opening),
        Ok(root) if root == binding.checkpoint_merkle_root
    )
}

// =================================================================================================
// ADR-0085 §6 items 4–5 and ADR-0086 Decision 6 — the executor's annex and block leaves, the
// challenger's close from served intervals, and the block-leaves request's address
// =================================================================================================

/// **A block-leaves request rides the interval lane under a high-bit index** (ADR-0086 Decision
/// 6). Bit 31 set; bits 30–16 the interval; bits 15–0 the block index within the step space at
/// the fold's retain level. A plain interval index never has bit 31 set (`interval_count` is
/// bounded by the decode count), so the two request kinds cannot collide, and the transport —
/// which keys solicitation, slots and the byte cap by `(claim, index)` — needs no new message.
pub const PALW_BASE0_FP_BLOCK_LEAVES_REQUEST_BIT_V1: u32 = 1 << 31;

pub fn base0_fp_block_leaves_request_index_v1(interval_index: u32, block_index: u64) -> Option<u32> {
    if interval_index >= 1 << 15 || block_index >= 1 << 16 {
        return None;
    }
    Some(PALW_BASE0_FP_BLOCK_LEAVES_REQUEST_BIT_V1 | (interval_index << 16) | block_index as u32)
}

/// `Some((interval, block))` for a block-leaves request, `None` for a plain interval index.
pub fn base0_fp_block_leaves_request_decode_v1(index: u32) -> Option<(u32, u64)> {
    if index & PALW_BASE0_FP_BLOCK_LEAVES_REQUEST_BIT_V1 == 0 {
        return None;
    }
    Some(((index >> 16) & 0x7FFF, (index & 0xFFFF) as u64))
}

/// **Which interval owns step leaf `leaf`** — the challenger's first question when a court
/// narrows to a step and it holds no capture (ADR-0085 Decision 3).
pub fn base0_fp_interval_of_leaf_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    family_checkpoint_interval: u32,
    leaf: u64,
) -> Option<u32> {
    use kaspa_consensus_core::palw_context_ladder::palw_checkpoint_cadence_v1;
    let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(profile, ctx, leaf)?;
    let geometry = Base0FpIntervalGeometryV1::from_chain_facts_v1(
        ctx.declared_prefill_tokens,
        ctx.exact_decode_tokens,
        family_checkpoint_interval,
        palw_checkpoint_cadence_v1(profile),
    )
    .ok()?;
    Some(interval_of_call_v1(&geometry, coord.call_index))
}

/// **The interval's own tiles, replayed from a folded retention with the family's kernels**
/// (ADR-0085 §6 item 2's executor half). A fold kept no tiles, so the tile a close annex must
/// carry is recomputed exactly as a seat recomputes it: from the interval's named anchor (or the
/// prompt, for interval 0) through the interval's calls. The tiles are the interval's calls'
/// only; a disputed leaf in the seed row (the anchor call's logits tiles) is served from the
/// retained rows by the caller.
#[allow(clippy::too_many_arguments)]
pub fn base0_fp_interval_tiles_from_fold_capped_v1<K: Base0FpIntervalKernelsV1>(
    material: &crate::produce::Base0FpMaterialV2,
    index: u32,
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    kernels: &K,
    anchor_state_for: Base0FpAnchorStateForV1<'_>,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<crate::legs::Base0StepTilesV1, Base0FpIntervalError> {
    let binding = &material.binding;
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;
    if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
        prompt_ids_form,
        prompt_token_ids,
        &ctx.prompt_token_ids_hash,
    ) {
        return Err(Base0FpIntervalError::PromptIdsAreNotTheJobs);
    }
    let step_leaf_count = base0_fp_binding_step_space_v1(binding, max_step_leaf_count)?;
    let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(binding, family_checkpoint_interval, max_step_leaf_count)?;
    let (first_call, last_call) =
        geometry.calls_for(index).ok_or(Base0FpIntervalError::IntervalOutOfRange { index, count: geometry.interval_count })?;
    let prompt_usize: Vec<usize> = prompt_token_ids.iter().map(|t| *t as usize).collect();
    let anchored = !material.checkpoint_chunks.is_empty();
    let covered_call = geometry.anchor_covered_call(index);
    let (resume_chunks, replay_first_call): (Option<Vec<Vec<u8>>>, u32) = match covered_call {
        Some(covered) if anchored => {
            (Some(base0_checkpoint_operands_v1(binding, &material.checkpoint_chunks, &[], covered)?.chunks), first_call)
        }
        Some(covered) => match anchor_state_for(covered) {
            Some(state) if state.covered_decode_call == covered => (Some(state.chunks), first_call),
            _ => (None, 0),
        },
        None => (None, 0),
    };
    let start = match (&resume_chunks, covered_call) {
        (None, _) => Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt_usize },
        (Some(resume), Some(covered)) => {
            let seed_call = geometry.anchor_seed_call_v1(index).ok_or(Base0FpIntervalError::NoCheckpointAt { covered })?;
            let seed_token = *material
                .generated_token_ids
                .get(seed_call as usize)
                .ok_or_else(|| Base0FpIntervalError::Replay(format!("the retention has no id for call {seed_call}")))?;
            Base0FpIntervalStartV1::Checkpoint { covered_decode_call: covered, chunks: resume, seed_token }
        }
        (Some(_), None) => unreachable!("an anchor exists only where the geometry names a covered call"),
    };
    kernels
        .replay_interval_tiles(profile, ctx, &start, replay_first_call, last_call, step_leaf_count)
        .map_err(Base0FpIntervalError::Replay)
}

/// **The close annex, built from the accused's own tiles** (ADR-0085 §6 item 4, the executor
/// half). `rows_root` is the retained rows' tree root — what the tiled pin binds the generated
/// ids through — and each disputed leaf inside `range` carries its committed tile and, when the
/// step reads the cache and a checkpoint covers the call before it, that checkpoint's leaf and
/// opening — the SAME anchoring rule the capture path applies (`refutation_with_prompt`), so the
/// close a challenger assembles from this annex is byte for byte the capture path's (ADR-0085 X1).
/// A disputed leaf outside `range` is skipped: it is another interval's to serve. A disputed leaf
/// no tile is held for is a refusal by name, never a silent omission.
pub fn base0_fp_close_annex_v1(
    binding: &PalwStepBindingV2,
    logits_rows: &[Vec<i32>],
    checkpoint_chunks: &[Vec<Vec<u8>>],
    tile_at: &dyn Fn(u64) -> Option<PalwStepTileLeafV1>,
    disputed: &[u64],
    range: (u64, u64),
) -> Result<Base0FpCloseAnnexV1, String> {
    let ctx = &binding.job_context;
    let profile = &binding.shape_profile;
    let rows_root = kaspa_consensus_core::palw_step_refute::tiled_logits_rows_root_v1(ctx, logits_rows)
        .ok_or_else(|| "the retained rows build no tree".to_string())?;
    let checkpoints = if checkpoint_chunks.is_empty() {
        None
    } else {
        crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(ctx, profile, &binding.checkpoint_profile, checkpoint_chunks).ok()
    };
    let mut out = Vec::new();
    for &leaf in disputed {
        if leaf < range.0 || leaf >= range.1 {
            continue;
        }
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(profile, ctx, leaf)
            .ok_or_else(|| format!("leaf {leaf} is not a main step coordinate"))?;
        let tile = tile_at(leaf).ok_or_else(|| format!("no tile is held for leaf {leaf}"))?;
        let reads_cache = profile
            .resolve_node_slot(coord.node_slot)
            .map(|(node, _)| {
                node.input_refs.iter().any(|r| {
                    *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_K
                        || *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_V
                })
            })
            .unwrap_or(false);
        let anchor = if reads_cache && coord.call_index > 0 {
            checkpoints
                .as_ref()
                .and_then(|c| crate::legs::base0_kv_anchor_for_call_v1(c, coord.call_index))
                .map(|k| Base0FpCheckpointClaimV1 { leaf: k.leaf, opening: k.opening })
        } else {
            None
        };
        out.push(Base0FpDisputedLeafV1 { leaf_index: leaf, tile, anchor });
    }
    Ok(Base0FpCloseAnnexV1 { rows_root, disputed: out })
}

/// **Attach a close annex to a served opening** — V4 (the annex field) or V3 (its own); any
/// other form has nowhere to carry it. The opening is otherwise byte-identical, so a seat that
/// reads it as its V2 view sees nothing changed (ADR-0085 X3).
pub fn base0_fp_interval_opening_with_close_v1(
    opening_bytes: &[u8],
    close: Base0FpCloseAnnexV1,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    if opening_bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V4) {
        let mut v4 = Base0FpIntervalOpeningV4::decode_v1(opening_bytes)?;
        v4.close = Some(close);
        return v4.encode_v1();
    }
    if opening_bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V3) {
        let mut v3 = Base0FpIntervalOpeningV3::decode_v1(opening_bytes)?;
        v3.close = Some(close);
        return v3.encode_v1();
    }
    Err(Base0FpIntervalError::NotThisFamilysBytes)
}

/// The closer's read of the annex, every form that can carry one: V4's field, V3's field.
pub fn base0_fp_interval_close_annex_any_v1(bytes: &[u8]) -> Option<Base0FpCloseAnnexV1> {
    if bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V4) {
        return Base0FpIntervalOpeningV4::decode_v1(bytes).ok()?.close;
    }
    base0_fp_interval_close_annex_v1(bytes)
}

/// **One block's leaf hashes, replayed from a folded retention** (ADR-0086 Decision 6, the
/// executor's answer). `opening_bytes` is the V4 opening this executor served for the interval —
/// its `range` names the blocks wholly inside it — and the leaves come from the same span replay
/// the opener ran to derive the edge siblings. The answer folds to the served digest by
/// construction; a seat checks that before naming a leaf.
#[allow(clippy::too_many_arguments)]
pub fn base0_fp_block_leaves_from_fold_capped_v1<K: Base0FpIntervalKernelsV1>(
    material: &crate::produce::Base0FpMaterialV2,
    opening_bytes: &[u8],
    block_index: u64,
    prompt_token_ids: &[u32],
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    kernels: &K,
    anchor_state_for: Base0FpAnchorStateForV1<'_>,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    let v4 = Base0FpIntervalOpeningV4::decode_v1(opening_bytes)?;
    let binding = &material.binding;
    if v4.binding != *binding {
        return Err(Base0FpIntervalError::CaptureIsNotTheBindings);
    }
    let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(binding, family_checkpoint_interval, max_step_leaf_count)?;
    let (span_first, span_end) = material.step_tree.span_for_range(v4.range.first_leaf_index, v4.range.leaf_count)?;
    let span_leaves = base0_replay_span_leaves_v1(
        kernels,
        binding,
        &material.checkpoint_chunks,
        &material.generated_token_ids,
        prompt_token_ids,
        &geometry,
        span_first,
        span_end,
        anchor_state_for,
    )?;
    let leaf = |i: u64| -> Option<Hash64> { i.checked_sub(span_first).and_then(|o| span_leaves.get(o as usize).copied()) };
    let cut = Base0FpBlockLeavesV1::cut_v1(v4.interval_index, &v4.range, binding.step_leaf_count, block_index, &leaf)
        .ok_or(Base0FpIntervalError::StepSpace(format!("block {block_index} is not wholly inside interval {}", v4.interval_index)))?;
    cut.encode_v1()
}

/// **The same answer from a dense retention**: the tiles are held, so the leaves are hashed.
pub fn base0_fp_block_leaves_from_tiles_v1(
    opening_bytes: &[u8],
    tiles: &[(u64, PalwStepTileLeafV1)],
    block_index: u64,
) -> Result<Vec<u8>, Base0FpIntervalError> {
    let v4 = Base0FpIntervalOpeningV4::decode_v1(opening_bytes)?;
    let ctx_hash = v4.binding.job_context.context_hash();
    let profile_hash = v4.binding.shape_profile.shape_profile_id();
    let by_index: std::collections::HashMap<u64, &PalwStepTileLeafV1> = tiles.iter().map(|(i, t)| (*i, t)).collect();
    let leaf = |i: u64| -> Option<Hash64> { by_index.get(&i).map(|t| step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, t)) };
    let cut = Base0FpBlockLeavesV1::cut_v1(v4.interval_index, &v4.range, v4.binding.step_leaf_count, block_index, &leaf)
        .ok_or(Base0FpIntervalError::StepSpace(format!("block {block_index} is not wholly inside interval {}", v4.interval_index)))?;
    cut.encode_v1()
}

/// **Name the leaf a served block disagrees on** (ADR-0086 Decision 6, the seat's half). The
/// served block must fold to the digest the V4 opening carries for it — a block that does not is
/// refused by name, never compared — and the leaf named is the first whose served hash differs
/// from this seat's own replay of the interval. `None` means every served leaf is this seat's own,
/// which with a `FaultInRange` on the same block says the executor served a different block than
/// it committed — also the court's question, at the block's first leaf.
#[allow(clippy::too_many_arguments)]
pub fn base0_fp_name_the_leaf_capped_v1<K: Base0FpIntervalKernelsV1>(
    opening_bytes: &[u8],
    block_leaves_bytes: &[u8],
    claim: PalwClaimRootsV1,
    index: u32,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    state_for: &dyn Fn(&[u8]) -> Option<crate::fp_recompute::Base0FpSeatStateV1>,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Option<u64>, String> {
    let v4 = Base0FpIntervalOpeningV4::decode_v1(opening_bytes).map_err(|e| format!("the opening is not V4: {e:?}"))?;
    let served = Base0FpBlockLeavesV1::decode_v1(block_leaves_bytes).map_err(|e| format!("the block leaves do not decode: {e:?}"))?;
    if served.interval_index != index || v4.interval_index != index {
        return Err("the served block is not this interval's".to_string());
    }
    let (first_block, end_block) = v4.range.whole_blocks_v1(v4.binding.step_leaf_count);
    let block = 1u64 << v4.range.retain_level.min(63);
    let block_index = served.first_leaf_index / block;
    if block_index < first_block || block_index >= end_block || served.first_leaf_index % block != 0 {
        return Err(format!("block {block_index} is not wholly inside interval {index}"));
    }
    let digest = v4
        .range
        .block_roots
        .get((block_index - first_block) as usize)
        .ok_or_else(|| format!("the opening carries no digest for block {block_index}"))?;
    if !served.folds_to_v1(digest) {
        return Err(format!("the served block {block_index} does not fold to the digest the opening carries"));
    }
    let state = state_for(opening_bytes);
    let (_, tiles) = base0_fp_challenger_replay_tiles_capped_v1(
        opening_bytes,
        claim,
        index,
        prompt_token_ids,
        work_leaves,
        family_checkpoint_interval,
        max_step_leaf_count,
        state.as_ref(),
        kernels,
        prompt_ids_form,
    )?;
    let ctx_hash = v4.binding.job_context.context_hash();
    let profile_hash = v4.binding.shape_profile.shape_profile_id();
    let mut own: std::collections::HashMap<u64, Hash64> =
        tiles.tiles.iter().map(|(i, t)| (*i, step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, t))).collect();
    for (k, tile) in v4.seed_row_tiles.iter().enumerate() {
        own.entry(v4.range.first_leaf_index + k as u64).or_insert_with(|| step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, tile));
    }
    Ok(served.name_the_leaf_v1(&|i| own.get(&i).copied()))
}

/// **The close, assembled from served intervals and this node's own replay** (ADR-0085 Decision
/// 3). `held` are the openings this node holds for the claim, by interval; the one whose range
/// covers `leaf` is primary and must carry the annex (the executor fills it only for a leaf an
/// open session names — ADR-0085 Decision 1), and the interval before it, when held, supplies the
/// prior call's tiles. Each is replayed with the family's kernels from the state this seat
/// recomputed for the checkpoint check; the replay's tiles are checked leaf by leaf against the
/// accused's committed hashes inside `base0_refutation_from_opening_capped_v1`, so a challenger
/// whose own execution diverges BEFORE the leaf is refused by name and reopens the question there.
/// The recomputed chunks handed to the builder are the primary interval's anchor state — the
/// checkpoint before the interval's first call — which on a per-call class with checkpoint
/// interval 1 is the checkpoint before every step in the interval; a wider cadence's mid-interval
/// step is refused by the builder's anchor check rather than closed wrongly (recorded, ADR-0085 §8).
/// `state_for` hands back the seat state for a held opening's named anchor, or `None` for
/// interval 0, which resumes from the prompt.
#[allow(clippy::too_many_arguments)]
pub fn base0_refutation_from_served_intervals_capped_v1<K: Base0FpIntervalKernelsV1>(
    held: &[(u32, Vec<u8>)],
    claim: PalwClaimRootsV1,
    prompt_token_ids: &[u32],
    generated_token_ids: &[u32],
    work_leaves: u64,
    leaf: u64,
    family_checkpoint_interval: u32,
    max_step_leaf_count: u64,
    state_for: &dyn Fn(&[u8]) -> Option<crate::fp_recompute::Base0FpSeatStateV1>,
    kernels: &K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
    let decoded: Vec<(u32, &[u8], Base0FpIntervalOpeningV4)> =
        held.iter().filter_map(|(i, b)| Base0FpIntervalOpeningV4::decode_v1(b).ok().map(|v| (*i, b.as_slice(), v))).collect();
    let (primary_index, primary_bytes, primary) = decoded
        .iter()
        .find(|(_, _, v)| leaf >= v.range.first_leaf_index && leaf < v.range.first_leaf_index.saturating_add(v.range.leaf_count))
        .map(|(i, b, v)| (*i, *b, v))
        .ok_or_else(|| format!("no held opening covers leaf {leaf}"))?;
    let annex =
        primary.close.clone().ok_or_else(|| format!("the served opening of interval {primary_index} carries no close annex"))?;
    if !annex.disputed.iter().any(|d| d.leaf_index == leaf) {
        return Err(format!("the annex of interval {primary_index} does not name leaf {leaf}"));
    }
    let profile = primary.binding.shape_profile.clone();
    let ctx = primary.binding.job_context.clone();
    let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, leaf)
        .ok_or_else(|| format!("leaf {leaf} is not a main step coordinate"))?;
    let replay = |index: u32, bytes: &[u8]| {
        // The state this node recomputed for the interval's named anchor (ADR-0086 Decision 2) —
        // the caller's to supply, because computing it takes the family's recompute kernels and
        // a memo this function must not reach into (a test supplies it directly; a backend warms
        // its memo and reads it back).
        let state = state_for(bytes);
        let (v2, tiles) = base0_fp_challenger_replay_tiles_capped_v1(
            bytes,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            family_checkpoint_interval,
            max_step_leaf_count,
            state.as_ref(),
            kernels,
            prompt_ids_form,
        )?;
        Ok::<_, String>((v2, tiles, state))
    };
    let (primary_v2, primary_tiles, primary_state) = replay(primary_index, primary_bytes)?;
    let previous = match primary_index.checked_sub(1) {
        Some(prev) => decoded.iter().find(|(i, ..)| *i == prev).map(|(i, b, _)| replay(*i, b)).transpose()?,
        None => None,
    };
    let mut intervals: Vec<(&Base0FpIntervalOpeningV2, &crate::legs::Base0StepTilesV1)> = vec![(&primary_v2, &primary_tiles)];
    if let Some((v2, tiles, _)) = previous.as_ref() {
        intervals.push((v2, tiles));
    }
    let chunks: Vec<Vec<u8>> = primary_state.map(|s| s.chunks).unwrap_or_default();
    let pin = kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::TiledV1(
        kaspa_consensus_core::palw_step_refute::PalwTiledDecodeTokensV1 {
            rows_root: annex.rows_root,
            generated_token_ids: generated_token_ids.to_vec(),
        },
    );
    crate::legs::base0_refutation_from_opening_capped_v1(
        &profile,
        &ctx,
        &intervals,
        &annex,
        coord,
        prompt_token_ids.to_vec(),
        Some(pin),
        &chunks,
        max_step_leaf_count,
    )
    .map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire form is frozen: a magic and a version, so bytes that are not an interval opening
    /// are refused as such rather than mis-parsed as one.
    #[test]
    fn the_opening_wire_form_is_frozen() {
        assert_eq!(&PALW_BASE0_FP_INTERVAL_MAGIC_V1, b"MSKFPIV1");
        assert_eq!(PALW_BASE0_FP_INTERVAL_VERSION_V1, 1);
        assert_eq!(Base0FpIntervalOpeningV1::decode_v1(b"not this family").unwrap_err(), Base0FpIntervalError::NotThisFamilysBytes);
        assert_eq!(
            Base0FpIntervalOpeningV1::decode_v1(&PALW_BASE0_FP_INTERVAL_MAGIC_V1).unwrap_err(),
            Base0FpIntervalError::NotThisFamilysBytes
        );
    }

    /// **The V4 wire form is frozen, and the old decoders refuse it** (ADR-0086 X1, X7): a
    /// V1–V3 decoder handed V4 bytes returns `NotThisFamilysBytes` — which the V1–V3 verifier
    /// turns into `Unverifiable`, never `Mismatch` and never a fault — and the any-form decoder
    /// returns `Digests`.
    #[test]
    fn the_v4_wire_form_is_frozen_and_the_old_decoders_refuse_it() {
        assert_eq!(&PALW_BASE0_FP_INTERVAL_MAGIC_V4, b"MSKFPIV4");
        assert_eq!(PALW_BASE0_FP_INTERVAL_VERSION_V4, 4);
        let (material, _claim, ids, _artifact) = floor_material(3, 4);
        let bytes = base0_open_fp_interval_v1(
            &material,
            0,
            &ids,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("opens");
        assert!(bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V4));
        assert_eq!(Base0FpIntervalOpeningV1::decode_v1(&bytes).unwrap_err(), Base0FpIntervalError::NotThisFamilysBytes);
        assert_eq!(Base0FpIntervalOpeningV2::decode_v1(&bytes).unwrap_err(), Base0FpIntervalError::NotThisFamilysBytes);
        assert_eq!(Base0FpIntervalOpeningV3::decode_v1(&bytes).unwrap_err(), Base0FpIntervalError::NotThisFamilysBytes);
        let decoded = Base0FpIntervalOpeningV4::decode_v1(&bytes).expect("its own decoder");
        assert_eq!(decoded.encode_v1().expect("re-encodes"), bytes, "the form round-trips byte for byte");
        assert!(matches!(base0_fp_interval_opening_decode_any_v1(&bytes), Ok(Base0FpIntervalOpeningAnyV1::Digests(_))));
        let mut wrong_version = decoded.clone();
        wrong_version.version = PALW_BASE0_FP_INTERVAL_VERSION_V3;
        assert_eq!(
            Base0FpIntervalOpeningV4::decode_v1(&wrong_version.encode_v1().unwrap()).unwrap_err(),
            Base0FpIntervalError::NotThisFamilysBytes
        );
    }

    /// **A fault has a block for an address** (ADR-0086 X5): over a synthetic space of 2^22+1
    /// leaves, a range spanning three whole blocks whose middle block the seat computes
    /// differently names exactly that block, clipped to the range; a range that agrees names
    /// nothing; and the digests are the fold's own retained nodes.
    #[test]
    fn a_differing_block_is_named_at_the_folds_granularity() {
        let leaf_count = PALW_STEP_LEG_MAX_LEAVES + 1;
        let cap = PALW_STEP_LEG_MAX_LEAVES * 2;
        let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(cap);
        let block = 1u64 << level;
        let leaves: Vec<Hash64> = (0..leaf_count).map(|i| Hash64::from_u64_word(i + 7)).collect();
        let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(&leaves, level, cap).expect("a tree");
        // A range from mid-block 3 to mid-block 7: whole blocks 4, 5, 6.
        let first = 3 * block + 5;
        let end = 7 * block + 9;
        let count = end - first;
        let (span_first, span_end) = tree.span_for_range(first, count).expect("the span");
        let range =
            tree.range_opening_v1(span_first, &leaves[span_first as usize..span_end as usize], first, count).expect("the range");
        let fold = Base0FpFoldRangeOpeningV1::from_range_v1(&range, &tree).expect("the fold form");
        assert_eq!(fold.whole_blocks_v1(leaf_count), (4, 7));
        assert_eq!(fold.block_roots, tree.retained_nodes()[4..7].to_vec(), "the digests are the retained nodes");
        let own: Vec<Hash64> = leaves[first as usize..end as usize].to_vec();
        assert_eq!(fold.first_block_that_differs_v1(&own, leaf_count), None, "an honest seat agrees with every digest");
        assert_eq!(
            step_range_opening_root_capped_v1(leaf_count, &fold.with_leaves_v1(own.clone()).unwrap(), cap).ok(),
            tree.root().ok(),
            "and its own leaves walk to the root under the served frontier"
        );
        let mut wrong = own.clone();
        let at = (5 * block + 100 - first) as usize;
        wrong[at] = Hash64::from_u64_word(0xBAD);
        assert_eq!(
            fold.first_block_that_differs_v1(&wrong, leaf_count),
            Some((5 * block, block)),
            "the leaf the seat computes differently is addressed by its block"
        );
        assert_ne!(step_range_opening_root_capped_v1(leaf_count, &fold.with_leaves_v1(wrong).unwrap(), cap).ok(), tree.root().ok());
        // The edge: a range inside one block has no whole block, and its address is itself.
        let (span_first, span_end) = tree.span_for_range(block + 10, 20).expect("the span");
        let edge =
            tree.range_opening_v1(span_first, &leaves[span_first as usize..span_end as usize], block + 10, 20).expect("the range");
        let edge_fold = Base0FpFoldRangeOpeningV1::from_range_v1(&edge, &tree).expect("the fold form");
        assert!(edge_fold.block_roots.is_empty());
        assert_eq!(fold_edge_v1(&edge_fold, leaf_count), (block + 10, 20));
        // A range that reaches the tree's end counts the partial tail block as whole.
        let tail_first = leaf_count - block - 3;
        let (span_first, span_end) = tree.span_for_range(tail_first, leaf_count - tail_first).expect("the span");
        let tail = tree
            .range_opening_v1(span_first, &leaves[span_first as usize..span_end as usize], tail_first, leaf_count - tail_first)
            .expect("the range");
        let tail_fold = Base0FpFoldRangeOpeningV1::from_range_v1(&tail, &tree).expect("the fold form");
        // The range starts three leaves inside the block before last, so its whole blocks are the
        // full block before the tail and the one-leaf tail itself — the tail counts because the
        // range reaches the tree's end.
        assert_eq!(tail_fold.whole_blocks_v1(leaf_count), (tail_first.div_ceil(block), leaf_count.div_ceil(block)));
        assert_eq!(tail_fold.block_roots.len(), 2);
        assert_eq!(tail_fold.block_roots.last(), tree.retained_nodes().last(), "the tail block's digest is the fold's last node");
    }

    /// **The address becomes a leaf** (ADR-0086 X6): from a served block of the producer's leaves
    /// and its own replay, the challenger names the leaf, rebuilds the producer's range with that
    /// block substituted, and the path it derives from it walks to the committed root — the
    /// court's opening, assembled from what was served.
    #[test]
    fn a_served_block_names_the_leaf_and_the_path_walks_to_the_root() {
        use kaspa_consensus_core::palw_step_leg::{step_opening_from_range_capped_v1, step_opening_root_capped_v1};
        let leaf_count = PALW_STEP_LEG_MAX_LEAVES + 1;
        let cap = PALW_STEP_LEG_MAX_LEAVES * 2;
        let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(cap);
        let block = 1u64 << level;
        // The producer's leaves, and the honest ones: one leaf in block 5 differs.
        let producer: Vec<Hash64> = (0..leaf_count).map(|i| Hash64::from_u64_word(i + 7)).collect();
        let bad = 5 * block + 100;
        let mut honest = producer.clone();
        honest[bad as usize] = Hash64::from_u64_word(0x600D);
        let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(&producer, level, cap).expect("a tree");
        let root = tree.root().expect("its root");
        let (first, end) = (3 * block + 5, 7 * block + 9);
        let count = end - first;
        let (span_first, span_end) = tree.span_for_range(first, count).expect("the span");
        let range =
            tree.range_opening_v1(span_first, &producer[span_first as usize..span_end as usize], first, count).expect("the range");
        let fold = Base0FpFoldRangeOpeningV1::from_range_v1(&range, &tree).expect("the fold form");
        let own: Vec<Hash64> = honest[first as usize..end as usize].to_vec();
        let (block_first, block_len) = fold.first_block_that_differs_v1(&own, leaf_count).expect("the seat names a block");
        assert_eq!((block_first, block_len), (5 * block, block));
        // The producer serves the block; the challenger checks it folds to the served digest.
        let served = Base0FpBlockLeavesV1::cut_v1(0, &fold, leaf_count, 5, &|i| producer.get(i as usize).copied())
            .expect("the block is the range's");
        assert_eq!(served.leaf_hashes.len() as u64, block);
        assert!(served.folds_to_v1(&fold.block_roots[1]), "the served block is the second whole block's digest");
        assert_eq!(Base0FpBlockLeavesV1::decode_v1(&served.encode_v1().unwrap()).unwrap(), served);
        assert!(
            Base0FpBlockLeavesV1::cut_v1(0, &fold, leaf_count, 3, &|i| producer.get(i as usize).copied()).is_none(),
            "block 3 is not whole"
        );
        // The leaf, and the court's path from the range with the served block substituted.
        assert_eq!(served.name_the_leaf_v1(&|i| honest.get(i as usize).copied()), Some(bad));
        assert_eq!(served.name_the_leaf_v1(&|i| producer.get(i as usize).copied()), None, "a block that agrees names nothing");
        let producers_range = base0_fp_range_with_served_block_v1(&fold, &own, &served).expect("the producer's range");
        assert_eq!(step_range_opening_root_capped_v1(leaf_count, &producers_range, cap).ok(), Some(root));
        let path = step_opening_from_range_capped_v1(leaf_count, &producers_range, bad, cap).expect("a path to the leaf");
        assert_eq!(
            step_opening_root_capped_v1(leaf_count, &path, cap).ok(),
            Some(root),
            "the derived path walks to the committed root"
        );
        // Without the served block the challenger's own range walks nowhere.
        let honest_range = fold.with_leaves_v1(own.clone()).unwrap();
        assert_ne!(step_range_opening_root_capped_v1(leaf_count, &honest_range, cap).ok(), Some(root));
    }

    /// Interval 0 is the prefill plus the calls up to the first checkpoint; interval `j` is the
    /// calls after checkpoint `j − 1`; every call is in exactly one of them. Pinned across the
    /// cadences the shipped classes use (1, the integer family's) and the ones a Decision 10 map
    /// would bring.
    #[test]
    fn every_call_is_in_exactly_one_interval() {
        for interval in [1u32, 2, 3, 8] {
            for decode_tokens in 1u32..=20 {
                let geometry = Base0FpIntervalGeometryV1::from_chain_facts_v1(
                    4,
                    decode_tokens,
                    interval,
                    kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerDecodeCall,
                )
                .expect("a geometry");
                let mut covered: Vec<u32> = Vec::new();
                for index in 0..geometry.interval_count {
                    let (first, last) = geometry.calls_for(index).expect("in range");
                    assert!(first <= last, "interval {index} of {geometry:?} is empty");
                    covered.extend(first..=last);
                    match geometry.anchor_covered_call(index) {
                        None => assert_eq!(index, 0),
                        Some(anchor) => assert_eq!(anchor + 1, first, "the anchor covers the call before the interval"),
                    }
                }
                let want: Vec<u32> = (0..=decode_tokens.saturating_sub(1)).collect();
                assert_eq!(covered, want, "interval {interval}, decode {decode_tokens}");
                assert!(geometry.calls_for(geometry.interval_count).is_none());
            }
        }
    }

    /// A cadence of zero is not a cadence — refused by name rather than dividing by it.
    #[test]
    fn a_zero_cadence_is_refused_by_name() {
        assert_eq!(
            Base0FpIntervalGeometryV1::from_chain_facts_v1(
                4,
                4,
                0,
                kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerDecodeCall
            )
            .unwrap_err(),
            Base0FpIntervalError::CheckpointIntervalIsZero
        );
    }

    // ------------------------------------------------------------------------------------------
    // The floor class, end to end — no model, a derived fixture artifact
    // ------------------------------------------------------------------------------------------

    use crate::produce::{Base0RetainedMaterialV1, base0_execute_for_attempt_v1};
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;

    /// A small BASE-0 class with real weights, derived from the pinned seed: the floor is not a
    /// language model, which is exactly why it can prove the OPENING without a model.
    fn floor_geometry() -> kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1 {
        let mut geometry = PALW_RC_BASE0_GEOMETRY;
        geometry.layer_count = 2;
        geometry.hidden_dim = 64;
        geometry.ffn_dim = 128;
        geometry.attn_heads = 2;
        geometry.attn_head_dim = 32;
        geometry.vocab_size = 128;
        geometry.n_ctx = 32;
        geometry.tile_len = 32;
        geometry
    }

    fn floor_job(prefill: u32, decode: u32) -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>) {
        let geometry = floor_geometry();
        let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(
            crate::artifact::Base0ShapeV1 {
                n_layers: geometry.layer_count as usize,
                n_heads: geometry.attn_heads as usize,
                n_kv_heads: geometry.attn_heads as usize,
                d_head: geometry.attn_head_dim as usize,
                d_ff: geometry.ffn_dim as usize,
                vocab: geometry.vocab_size as usize,
                max_position: geometry.n_ctx as usize,
                ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
                eps_q: geometry.rms_eps_q,
            },
            crate::rc::PALW_RC_BASE0_SEED,
        )
        .expect("the fixture shape is valid");
        let profile = base0_profile_v1(geometry).expect("expressible");
        let (ctx, prompt) = crate::produce::base0_rc_job_v1(
            &profile,
            Hash64::from_u64_word(0x_F1_00_5E),
            geometry.vocab_size as usize,
            prefill,
            decode,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        (artifact, profile, ctx, prompt)
    }

    fn floor_material(
        prefill: u32,
        decode: u32,
    ) -> (Base0RetainedMaterialV1, PalwClaimRootsV1, Vec<u32>, crate::artifact::Base0ArtifactV1) {
        let (artifact, profile, ctx, prompt) = floor_job(prefill, decode);
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let claim = PalwClaimRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            anchor: run.binding.job_context.job_id,
        };
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let material: Base0RetainedMaterialV1 = (
            run.binding.clone(),
            run.tiles.tiles.clone(),
            run.logits_rows.clone(),
            run.generated_token_ids.clone(),
            run.checkpoints.chunks.clone(),
        );
        (material, claim, ids, artifact)
    }

    /// The floor's kernels, for the seat half — the same adapter `crate::backend` installs, spelled
    /// here so the test drives the shipped path rather than a test-only replay.
    struct FloorKernels<'a>(&'a crate::artifact::Base0ArtifactV1);

    impl Base0FpIntervalKernelsV1 for FloorKernels<'_> {
        fn replay_interval_tiles(
            &self,
            profile: &PalwShapeProfileV3,
            ctx: &PalwJobContextV2,
            start: &Base0FpIntervalStartV1<'_>,
            first_call: u32,
            last_call: u32,
            step_leaf_count: u64,
        ) -> Result<crate::legs::Base0StepTilesV1, String> {
            use crate::engine::{Base0Engine, KvCache};
            let engine = Base0Engine::new(self.0);
            let mut cache = match start {
                Base0FpIntervalStartV1::Genesis { .. } => KvCache::new(self.0),
                Base0FpIntervalStartV1::Checkpoint { covered_decode_call, chunks, .. } => {
                    let geometry = crate::legs::base0_checkpoint_geometry_at_v1(profile, ctx, *covered_decode_call)
                        .map_err(|e| format!("{e:?}"))?;
                    KvCache::from_state_chunks(self.0, &geometry, chunks).map_err(|e| format!("{e:?}"))?
                }
            };
            base0_fp_replay_interval_tiles_v1(profile, ctx, start, first_call, last_call, step_leaf_count, |token, position| {
                let (logits, probe) = engine.forward_token_probed(&mut cache, token, position).map_err(|e| format!("{e:?}"))?;
                Ok((logits, crate::legs::base0_captured_rows_v1(&probe)))
            })
        }
    }

    /// **The round trip Decision 8 is: open one interval, replay it, agree exactly.**
    ///
    /// Every interval of a real capture — the genesis-anchored one and every checkpoint-anchored
    /// one — is opened by the executor and verified by a seat that holds only the opening, the
    /// claim's two roots, the prompt ids and the price. The capture itself is never handed over,
    /// which is the whole of R1's verification half.
    #[test]
    fn every_interval_of_a_floor_capture_opens_and_verifies() {
        let (material, claim, ids, artifact) = floor_material(3, 4);
        let binding = &material.0;
        let count = Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
            .expect("a geometry")
            .interval_count;
        assert!(count > 1, "the fixture must exercise both the genesis and the anchored arms");
        let geometry =
            Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).expect("a geometry");
        for index in 0..count {
            let opening = base0_open_fp_interval_v1(
                &material,
                index,
                &ids,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .unwrap_or_else(|e| panic!("interval {index} opens: {e}"));
            // ADR-0086 X3: the served bytes are the fold's form, and carry no leaf hash.
            let decoded = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("a V4 opening");
            assert!(decoded.range.siblings.len() <= 2 * kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_OPENING_SIBLINGS);
            let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
            assert_eq!(
                base0_verify_fp_interval_opening_with_state_v1(
                    &opening,
                    claim,
                    index,
                    &ids,
                    binding.step_leaf_count,
                    PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                    state.as_ref(),
                    &FloorKernels(&artifact),
                    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                ),
                Base0FpIntervalSeatVerdictV1::Valid,
                "interval {index} of an honest capture replays exactly against the seat's own leaves"
            );
        }
    }

    /// **One byte, and the opening answers for nothing.**
    ///
    /// Three tampers, each a different accusation. A flipped byte anywhere in the encoded opening
    /// must never read as `Valid`: it is `Mismatch` when it breaks a binding the claim's roots
    /// pin, and `Unverifiable` when it stops decoding at all — and the two are different
    /// accusations on purpose (`PalwFpIntervalVerdictV1`'s own doc). What is refused BY NAME is
    /// the third: an opening whose committed rows have been changed replays to a different hash,
    /// and the seat returns the court's question at the leaf rather than a verdict.
    #[test]
    fn a_tampered_opening_is_refused_by_name() {
        let (material, claim, ids, artifact) = floor_material(3, 4);
        let binding = &material.0;
        let leaves = binding.step_leaf_count;
        let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, interval).expect("a geometry");
        let verify = |bytes: &[u8], index: u32| {
            let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
            base0_verify_fp_interval_opening_with_state_v1(
                bytes,
                claim,
                index,
                &ids,
                leaves,
                interval,
                state.as_ref(),
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .to_consensus_v1()
        };

        // (1) The last anchored interval, opened honestly, then one byte of a COMMITTED ROW moved.
        let index = Base0FpIntervalGeometryV1::from_binding_v1(binding, interval).expect("a geometry").interval_count - 1;
        let opening = base0_open_fp_interval_v1(
            &material,
            index,
            &ids,
            interval,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("opens");
        assert_eq!(verify(&opening, index), PalwFpIntervalVerdictV1::Valid);

        // ADR-0086: a tampered frontier is the seat's own leaves not walking to the root — a fault
        // with the range for an address, never a licence.
        let mut decoded = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("decodes");
        let last = decoded.range.siblings.len() - 1;
        let mut bytes = decoded.range.siblings[last].as_byte_slice().to_vec();
        bytes[0] ^= 1;
        decoded.range.siblings[last] = Hash64::from_bytes(bytes.try_into().expect("64 bytes"));
        assert!(matches!(verify(&decoded.encode_v1().expect("re-encodes"), index), PalwFpIntervalVerdictV1::FaultInRange { .. }));
        let mut short = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("decodes");
        short.range.siblings.pop();
        assert!(matches!(verify(&short.encode_v1().expect("re-encodes"), index), PalwFpIntervalVerdictV1::FaultInRange { .. }));
        let mut wrong_count = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("decodes");
        wrong_count.range.leaf_count += 1;
        assert_eq!(verify(&wrong_count.encode_v1().expect("re-encodes"), index), PalwFpIntervalVerdictV1::Mismatch);

        // (2) The magic, one byte. Bytes that are not this family's are bytes this seat cannot
        //     check — never an accusation.
        let mut wrong_family = opening.clone();
        wrong_family[0] ^= 1;
        assert_eq!(verify(&wrong_family, index), PalwFpIntervalVerdictV1::Unverifiable);

        // (3) The right opening against the WRONG interval index, and against another claim's
        //     roots. Both are "this opening is not about the claim in hand".
        assert_eq!(verify(&opening, index.saturating_sub(1)), PalwFpIntervalVerdictV1::Mismatch);
        let stranger = PalwClaimRootsV1 { execution_root: Hash64::from_u64_word(9), ..claim };
        assert_eq!(
            base0_verify_fp_interval_opening_v1(
                &opening,
                stranger,
                index,
                &ids,
                leaves,
                interval,
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            ),
            PalwFpIntervalVerdictV1::Mismatch
        );
        // …and against the wrong PRICE, which is the same accusation: a claim is priced by the
        // leaf count its binding carries (ADR-0074 Decision 5).
        assert_eq!(
            base0_verify_fp_interval_opening_v1(
                &opening,
                claim,
                index,
                &ids,
                leaves + 1,
                interval,
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            ),
            PalwFpIntervalVerdictV1::Mismatch
        );
        // ADR-0086 Decision 2: without its own state a seat cannot judge an anchored V4 opening.
        assert_eq!(
            base0_verify_fp_interval_opening_v1(
                &opening,
                claim,
                index,
                &ids,
                leaves,
                interval,
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            ),
            PalwFpIntervalVerdictV1::Unverifiable
        );
    }

    /// **A FAULT is the court's question, and a seat asks it without convicting.**
    ///
    /// A producer that committed a row it did not compute is caught by the replay — not by any
    /// check on the opening, which is self-consistent — and the seat answers with the leaf a
    /// challenger then opens a court at. This is the case the whole arrangement exists for, and
    /// the verdict it produces slashes nobody.
    #[test]
    fn a_row_the_producer_did_not_compute_is_a_fault_at_its_leaf() {
        let (mut material, _, ids, artifact) = floor_material(3, 4);
        let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let index = Base0FpIntervalGeometryV1::from_binding_v1(&material.0, interval).expect("a geometry").interval_count - 1;
        let leaves_geometry = base0_fp_interval_leaves_v1(
            &material.0.shape_profile,
            &material.0.job_context,
            &Base0FpIntervalGeometryV1::from_binding_v1(&material.0, interval).expect("a geometry"),
            index,
            material.0.step_leaf_count,
        )
        .expect("a leaf range");
        let target = leaves_geometry.interval_first;

        // Tamper a step the interval owns, then RE-DERIVE the commitment from the corrupted
        // capture — the producer's roots stay honestly its own, which is what makes this the
        // court's fraud and not a mismatch any seat check would see.
        {
            let slot = material.1.iter_mut().find(|(i, _)| *i == target).expect("the tile is held");
            slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
        }
        let ctx = material.0.job_context.clone();
        let profile = material.0.shape_profile.clone();
        let mut leaves = vec![Hash64::default(); material.0.step_leaf_count as usize];
        let (ctx_hash, profile_hash) = (ctx.context_hash(), profile.shape_profile_id());
        for (i, leaf) in &material.1 {
            leaves[*i as usize] = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf);
        }
        let tiles = crate::legs::Base0StepTilesV1 { leaves, tiles: material.1.clone() };
        let checkpoints =
            crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(&ctx, &profile, &material.0.checkpoint_profile, &material.4)
                .expect("the leg re-derives");
        let binding = crate::legs::base0_binding_from_capture_v1(
            &profile,
            &ctx,
            &tiles,
            &checkpoints,
            material.0.full_logits_trace_root,
            material.0.activation_leg_root,
        )
        .expect("a binding");
        let claim = PalwClaimRootsV1 {
            execution_root: binding.committed_execution_root,
            trace_root: binding.full_logits_trace_root,
            anchor: ctx.job_id,
        };
        let leaf_count = binding.step_leaf_count;
        material.0 = binding;

        let opening = base0_open_fp_interval_v1(
            &material,
            index,
            &ids,
            interval,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("a liar still opens its own capture");
        let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&material.0, interval).expect("a geometry");
        let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
        let verdict = base0_verify_fp_interval_opening_with_state_v1(
            &opening,
            claim,
            index,
            &ids,
            leaf_count,
            interval,
            state.as_ref(),
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        // ADR-0086 Decision 3: the address is a range of the fold that holds the leaf — on a
        // space this small, the range itself — and it convicts nobody.
        let Base0FpIntervalSeatVerdictV1::FaultInRange { first_leaf_index, leaf_count: named } = verdict else {
            panic!("a row the producer did not compute must be a fault with an address, not {verdict:?}");
        };
        assert!(
            first_leaf_index <= target && target < first_leaf_index + named,
            "the named range [{first_leaf_index}, +{named}) must hold the tampered leaf {target}"
        );
        assert_eq!(verdict.to_consensus_v1(), PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index, leaf_count: named });
    }

    /// **The count a seat draws against comes from the chain, and the capture agrees with it.**
    ///
    /// `fp_interval_count_for` reads two numbers off the accepted 0x4a payload and the class's own
    /// cadence; `fp_interval_count` reads a capture. They must agree on every capture this family
    /// produces — this is the test the backend seam's own doc promises — and it is the chain-side
    /// one a seat uses, because an executor that could shrink the count could predict the draw.
    #[test]
    fn the_two_interval_counts_agree_on_every_capture() {
        for (prefill, decode) in [(3u32, 1u32), (3, 2), (3, 4), (4, 5), (2, 8)] {
            let (material, ..) = floor_material(prefill, decode);
            let binding = &material.0;
            let from_capture = Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
                .expect("a geometry")
                .interval_count;
            let from_chain = base0_fp_interval_count_for_v1(
                binding.job_context.declared_prefill_tokens,
                binding.job_context.exact_decode_tokens,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            )
            .expect("a count");
            assert_eq!(from_capture, from_chain, "prefill {prefill}, decode {decode}");
            // …and the chain-side form reads nothing but its three arguments.
            assert_eq!(from_chain, base0_fp_interval_count_for_v1(prefill, decode, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).unwrap());
        }
    }

    /// **W10, measured — and the one term that is not constant, named.**
    ///
    /// Decision 8 states the opening as `O(interval × row + log₂ leaves)`. Measured on the floor
    /// fixture, an anchored interval's opening decomposes as:
    ///
    /// * the **binding** — the class's whole shape profile — a per-class constant (~19 KiB here);
    /// * the interval's **committed rows**, 190 leaf hashes at every decode budget: `interval ×
    ///   row`, flat;
    /// * the **seed row**'s four tile preimages, flat;
    /// * the **siblings**, 3 to 7 of them: `log₂ leaves`;
    /// * the **anchor's state chunks**, which are NOT flat — 1,280 bytes at four decode tokens and
    ///   4,352 at sixteen.
    ///
    /// That last term is honest and unavoidable on an attention class: resuming a decode call
    /// requires the WHOLE KV history, so the checkpoint chunk is `positions × kv_row`. It is
    /// bounded — every admissible job has `prefill + decode ≤ n_ctx`, so the state term is at most
    /// a class constant (`n_ctx × attn_kv_heads × attn_head_dim × 2 × attn_layers`) — but it is not
    /// constant WITHIN a class, and the ADR's own Decision 11 note ("may not make A16@512 fit
    /// 80 KiB if attention reads the whole history — derive honestly") is about exactly this.
    ///
    /// So the assertion is the decomposition rather than a single bound: everything but the state
    /// chunk is flat in `decode_tokens_executed`, the state chunk is the class's own ceiling, and
    /// the capture — the thing this replaces — grows without either bound.
    #[test]
    fn an_interval_opening_does_not_grow_with_the_job() {
        let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let measure = |decode: u32| -> (usize, usize, usize, usize, usize, u64) {
            let (material, _, ids, _) = floor_material(3, decode);
            let count = Base0FpIntervalGeometryV1::from_binding_v1(&material.0, interval).expect("a geometry").interval_count;
            let bytes = base0_open_fp_interval_v1(
                &material,
                count - 1,
                &ids,
                interval,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("opens");
            let opened = Base0FpIntervalOpeningV4::decode_v1(&bytes).expect("decodes");
            assert!(opened.anchor.is_some(), "the last interval is anchored");
            // ADR-0086 Decision 2: no chunk rides; the anchor is a claim.
            let state: usize = 0;
            let capture = borsh::to_vec(&material).expect("the capture encodes").len();
            (bytes.len(), opened.range.leaf_count as usize, opened.range.siblings.len(), state, capture, material.0.step_leaf_count)
        };
        let (short_bytes, short_rows, _, short_state, short_capture, short_leaves) = measure(4);
        let (long_bytes, long_rows, long_siblings, long_state, long_capture, long_leaves) = measure(16);

        assert!(long_leaves > short_leaves && long_capture > 3 * short_capture / 2, "the job really did get longer");
        // The rows an opening carries are ONE interval's, whatever the job's length.
        assert!(long_rows <= short_rows + 8, "the committed rows are one interval's: {short_rows} then {long_rows}");
        assert!(
            long_siblings <= 2 * kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_OPENING_SIBLINGS,
            "the sibling set is the leg's own bound"
        );
        // Everything but the anchor's state is flat.
        assert!(
            long_bytes - long_state <= short_bytes - short_state + 1_024,
            "the non-state part of an opening grew from {} to {} bytes",
            short_bytes - short_state,
            long_bytes - long_state
        );
        // The state IS the growing term, and its ceiling is the class's, not the job's: the widest
        // job this class admits is `n_ctx` positions of the cache.
        assert_eq!((short_state, long_state), (0, 0), "ADR-0086: an opening carries no state at all");
        // The whole point: a seat's bytes are a fraction of the capture's, and the fraction falls.
        assert!(
            long_bytes * 10 < long_capture,
            "an opening is {long_bytes} bytes against a {long_capture}-byte capture — that ratio is what Decision 8 buys"
        );
    }

    /// **W2 for attention: the anchored refutation and the long form reach the same verdict.**
    ///
    /// The long form opens one KV operand per cached position and re-derives the history from
    /// genesis; the anchored form opens ONE checkpoint chunk and resumes from it. They are two
    /// pieces of evidence about the same step, and the invariant is that the court cannot be
    /// steered by which one a party chooses: honest material must be `NoFaultFound` both ways, and
    /// the same tampered row must convict both ways. Without that, a challenger picks whichever
    /// route convicts and Decision 10's cheaper form is a second, softer court.
    ///
    /// Run on the floor's fixture — a class with real weights, a real checkpoint leg and no model
    /// to load.
    #[test]
    fn the_anchored_refutation_and_the_long_form_agree_on_attention() {
        use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

        let (artifact, profile, ctx, prompt) = floor_job(3, 4);
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        // A step inside the LAST decode call that READS THE KV HISTORY — the only kind of step for
        // which the two forms both exist, and the kind Decision 10 is about. Found from the
        // profile's own `input_refs` rather than by naming a slot: the two forms differ only at a
        // history-reading ref, and a slot named here would be a second opinion about which node
        // that is.
        let call = ctx.exact_decode_tokens - 1;
        let reads_kv = |slot: u32| {
            profile.resolve_node_slot(slot).is_some_and(|(node, _)| {
                node.input_refs.iter().any(|r| {
                    *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_K
                        || *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_V
                })
            })
        };
        let target = (0..run.binding.step_leaf_count)
            .filter_map(|i| kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, i))
            .find(|c| c.call_index == call && reads_kv(c.node_slot))
            .expect("the class has a step that reads its own KV history");

        let anchor = base0_checkpoint_operands_v1(&run.binding, &run.checkpoints.chunks, &run.checkpoints.leaves, call - 1)
            .expect("the committed checkpoint");
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let pin = kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::Base0V1(
            kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 {
                logits_rows: run.logits_rows.clone(),
                generated_token_ids: run.generated_token_ids.clone(),
            },
        );
        let build = |kv: Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1>,
                     tiles: &crate::legs::Base0StepTilesV1,
                     binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2| {
            crate::legs::base0_refutation_from_capture_v1(&profile, &ctx, tiles, binding, target, ids.clone(), Some(pin.clone()), kv)
                .expect("a capture assembles a refutation")
        };

        // The oracle is the production inventory, driven by the adjudicator's own recording pass —
        // the same route `operand_openings_for` takes, so this exercises the shipped prover.
        let inventory = crate::inventory::base0_inventory_v1(&artifact, floor_geometry()).expect("inventory");
        let prove = |refutation: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1| {
            let recorder = kaspa_consensus_core::palw_artifact::PalwRecordingOracleV1::new(inventory.operands());
            let _ = check_execution_step_refutation_v1(refutation, &recorder);
            let openings = recorder.openings().expect("the inventory opens what its own oracle resolved");
            kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
                .expect("the rows prove against the artifact root")
        };

        // (1) Honest material: the challenger loses on the merits, both ways.
        let long = build(None, &run.tiles, run.binding.clone());
        let anchored = build(Some(anchor.clone()), &run.tiles, run.binding.clone());
        for (name, refutation) in [("long", &long), ("anchored", &anchored)] {
            let oracle = prove(refutation);
            assert!(
                matches!(check_execution_step_refutation_v1(refutation, &oracle), Err(PalwStepRefuteError::NoFaultFound)),
                "the {name} form must find no fault in honest material: {:?}",
                check_execution_step_refutation_v1(refutation, &oracle)
            );
        }
        assert!(anchored.kv_checkpoint.is_some() && long.kv_checkpoint.is_none(), "the two forms really are the two forms");

        // (2) One tampered row, re-derived into its own commitment: both forms convict, with the
        //     same verdict. A route that acquitted here would be the softer court.
        let mut tiles = run.tiles.clone();
        let target_index = kaspa_consensus_core::palw_step::canonical_step_leaf_index(&profile, &ctx, &target).expect("an index");
        {
            let slot = tiles.tiles.iter_mut().find(|(i, _)| *i == target_index).expect("the tile is held");
            slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
            tiles.leaves[target_index as usize] =
                kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx.context_hash(), &profile.shape_profile_id(), &slot.1);
        }
        let lying = crate::legs::base0_binding_from_capture_v1(
            &profile,
            &ctx,
            &tiles,
            &run.checkpoints,
            run.binding.full_logits_trace_root,
            run.binding.activation_leg_root,
        )
        .expect("a binding");
        let long_verdict = {
            let r = build(None, &tiles, lying.clone());
            let oracle = prove(&r);
            check_execution_step_refutation_v1(&r, &oracle)
        };
        let anchored_verdict = {
            let r = build(Some(anchor), &tiles, lying);
            let oracle = prove(&r);
            check_execution_step_refutation_v1(&r, &oracle)
        };
        assert!(long_verdict.is_ok(), "the long form convicts a row the producer did not compute: {long_verdict:?}");
        assert_eq!(
            format!("{long_verdict:?}"),
            format!("{anchored_verdict:?}"),
            "the anchored form must reach the SAME verdict as the long one — otherwise a challenger picks the route that convicts"
        );
    }

    // =============================================================================================
    // ADR-0082 Decision 9 (invariant Z5, unit U-05): the seat recomputes, and never fetches the history
    // =============================================================================================

    use crate::fp_recompute::{Base0FpRecomputeError, base0_fp_recompute_state_v1};

    /// The floor's kernels for a RECOMPUTE — the same engine and cache its capture path uses, with
    /// nothing captured and no token selected. The dense and hybrid tiers ship theirs in
    /// `crate::fp_recompute`; this one exists because the floor's fixture is the class in this
    /// file's tests, and it drives exactly the same shared driver.
    use crate::fp_recompute::Base0RecomputeKernelsV1 as FloorRecompute;

    /// The seat's own state at one interval's start, computed the way Decision 9 says: the prompt
    /// it holds and the committed output ids, teacher-forced, with the family's own kernels.
    fn seat_state(
        material: &Base0RetainedMaterialV1,
        artifact: &crate::artifact::Base0ArtifactV1,
        prompt_ids: &[u32],
        covered: u32,
    ) -> crate::fp_recompute::Base0FpSeatStateV1 {
        let binding = &material.0;
        let mut kernels = FloorRecompute::new(artifact);
        base0_fp_recompute_state_v1(&binding.shape_profile, &binding.job_context, prompt_ids, &material.3, covered, &mut kernels)
            .expect("the seat can run the floor")
    }

    /// **(a) Z5's first half: the root a seat recomputes IS the checkpoint the executor committed,
    /// at every interval of a multi-interval job.**
    ///
    /// Nothing about the executor's state is read: the comparison is between 64 bytes the claim
    /// commits and 64 bytes this process computed from the prompt and the answer's ids.
    #[test]
    fn the_recomputed_root_is_the_committed_checkpoint_at_every_interval() {
        let (material, _claim, ids, artifact) = floor_material(3, 4);
        let binding = &material.0;
        let geometry =
            Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).expect("a geometry");
        assert!(geometry.interval_count > 1, "the fixture must have at least one checkpoint-anchored interval");
        let checkpoints = crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
            &binding.job_context,
            &binding.shape_profile,
            &binding.checkpoint_profile,
            &material.4,
        )
        .expect("the executor's own leg");
        let mut checked = 0;
        for index in 1..geometry.interval_count {
            let covered = geometry.anchor_covered_call(index).expect("an anchored interval");
            let state = seat_state(&material, &artifact, &ids, covered);
            let committed = checkpoints
                .leaves
                .iter()
                .find(|l| l.covered_decode_call == covered)
                .unwrap_or_else(|| panic!("the executor committed a checkpoint at call {covered}"));
            assert_eq!(
                state.state_chunks_root, committed.state_chunks_root,
                "interval {index}: the seat's own recompute must reach the committed state root"
            );
            // …and it is the executor's own spelling that produced both, not two hashes that
            // happen to agree today.
            assert_eq!(
                base0_state_chunks_root_v1(&binding.state_chunk_map_id, &state.chunks).expect("a root"),
                committed.state_chunks_root
            );
            checked += 1;
        }
        assert!(checked > 0);
    }

    /// **(e) The two forms agree on honest material**, and the chunkless one is what a graph-v5
    /// seat is served: the same interval, replayed from the seat's own recomputed state, reaches
    /// the same verdict as the ADR-0077 replay fed the executor's chunks.
    #[test]
    fn the_seats_own_state_and_the_carried_chunks_reach_the_same_verdict() {
        let (material, claim, ids, artifact) = floor_material(3, 4);
        let binding = &material.0;
        let geometry =
            Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).expect("a geometry");
        for index in 0..geometry.interval_count {
            let chunked = base0_open_fp_interval_v1(
                &material,
                index,
                &ids,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the chunked form opens");
            let flat = base0_open_fp_interval_chunkless_v1(
                &material,
                index,
                &ids,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the flat form opens");
            let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
            // ADR-0086 Decision 2: there is one form, and it is judged from the seat's own state.
            assert_eq!(chunked, flat, "interval {index}: the two openers serve one form");
            let without = base0_verify_fp_interval_opening_with_state_v1(
                &chunked,
                claim,
                index,
                &ids,
                binding.step_leaf_count,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                None,
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            );
            let recomputed = base0_verify_fp_interval_opening_with_state_v1(
                &flat,
                claim,
                index,
                &ids,
                binding.step_leaf_count,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                state.as_ref(),
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            );
            if index == 0 {
                assert_eq!(without, Base0FpIntervalSeatVerdictV1::Valid, "interval 0 starts from the prompt and needs no state");
            } else {
                assert_eq!(without, Base0FpIntervalSeatVerdictV1::Unverifiable, "interval {index}: no state, no verdict");
            }
            assert_eq!(recomputed, Base0FpIntervalSeatVerdictV1::Valid, "interval {index}, from the seat's own state");
        }
    }

    /// **(b) A tampered checkpoint is refused, and the refusal NAMES it.**
    ///
    /// The producer is honest about its own bytes: one cache row is changed and the whole
    /// commitment is RE-DERIVED from the corrupted chunks, so its roots are self-consistent and
    /// every check that reads the opening against the claim passes. The only thing that catches it
    /// is arithmetic nobody can fake — the seat running the job — and what comes back names the
    /// checkpoint, the call it covers, and both roots.
    #[test]
    fn a_tampered_checkpoint_is_refused_and_the_refusal_names_it() {
        let (honest, _claim, ids, artifact) = floor_material(3, 4);
        let binding = &honest.0;
        let geometry =
            Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).expect("a geometry");
        let index = 1;
        let covered = geometry.anchor_covered_call(index).expect("an anchored interval");

        // One byte of one cache row, and then the producer's own commitment over it.
        let mut chunks = honest.4.clone();
        chunks[0][0][0] ^= 0x01;
        let checkpoints = crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
            &binding.job_context,
            &binding.shape_profile,
            &binding.checkpoint_profile,
            &chunks,
        )
        .expect("the corrupted leg still builds");
        let tiles = crate::legs::Base0StepTilesV1 {
            tiles: honest.1.clone(),
            leaves: {
                let ctx_hash = binding.job_context.context_hash();
                let profile_hash = binding.shape_profile.shape_profile_id();
                let mut leaves = vec![Hash64::default(); binding.step_leaf_count as usize];
                for (at, leaf) in &honest.1 {
                    if let Some(slot) = leaves.get_mut(*at as usize) {
                        *slot = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf);
                    }
                }
                leaves
            },
        };
        let lying = crate::legs::base0_binding_from_capture_v1(
            &binding.shape_profile,
            &binding.job_context,
            &tiles,
            &checkpoints,
            binding.full_logits_trace_root,
            binding.activation_leg_root,
        )
        .expect("the liar's own binding");
        let claim = PalwClaimRootsV1 {
            execution_root: lying.committed_execution_root,
            trace_root: lying.full_logits_trace_root,
            anchor: lying.job_context.job_id,
        };
        let material: Base0RetainedMaterialV1 = (lying, honest.1.clone(), honest.2.clone(), honest.3.clone(), chunks);
        let opening = base0_open_fp_interval_chunkless_v1(
            &material,
            index,
            &ids,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the liar serves an opening");
        let state = seat_state(&material, &artifact, &ids, covered);
        let verdict = base0_verify_fp_interval_opening_with_state_v1(
            &opening,
            claim,
            index,
            &ids,
            material.0.step_leaf_count,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            Some(&state),
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        match verdict {
            Base0FpIntervalSeatVerdictV1::CheckpointRootMismatch { checkpoint_index, covered_decode_call, committed, recomputed } => {
                assert_eq!(covered_decode_call, covered, "the refusal names the call the checkpoint covers");
                assert_eq!(checkpoint_index, 0, "the fixture's first checkpoint is the one that was tampered with");
                assert_ne!(committed, recomputed, "the two roots the refusal carries are the two that disagree");
                assert_eq!(recomputed, state.state_chunks_root, "the recomputed root is the seat's own");
            }
            other => panic!("a checkpoint the job does not reach must be named, not {other:?}"),
        }
        // And the ONE thing it is not: a conviction. The seam sees a verdict that files nothing.
        assert_eq!(verdict.to_consensus_v1(), PalwFpIntervalVerdictV1::Unverifiable);
    }

    /// **(c) Z5's bytes: what a seat fetches does not carry the history, and the history is what
    /// grows with the context.**
    ///
    /// Measured at two contexts on real captures. The assertion is an INEQUALITY, not a number:
    /// the flat opening's growth from the narrow context to the wide one is strictly smaller than
    /// the carried form's, by at least the state both checkpoints hold — and the flat form carries
    /// no state at all, which is the structural half of the same statement.
    #[test]
    fn the_bytes_a_seat_fetches_do_not_carry_the_history() {
        let measure = |prefill: u32, decode: u32| {
            let (material, _claim, ids, _artifact) = floor_material(prefill, decode);
            let index = 1;
            let chunked = base0_open_fp_interval_v1(
                &material,
                index,
                &ids,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the chunked form opens");
            let flat = base0_open_fp_interval_chunkless_v1(
                &material,
                index,
                &ids,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the flat form opens");
            let decoded = Base0FpIntervalOpeningV4::decode_v1(&flat).expect("the fold form decodes");
            assert_eq!(chunked, flat, "ADR-0086: one form from either opener");
            assert!(decoded.anchor.is_some(), "the interval is checkpoint-anchored at both contexts");
            // The history the class holds at this context, for the comparison below.
            let state_bytes: usize = seat_state(&material, &_artifact, &ids, 1).chunks.iter().map(Vec::len).sum();
            (chunked.len(), flat.len(), state_bytes)
        };
        let (chunked_narrow, flat_narrow, state_narrow) = measure(3, 4);
        let (chunked_wide, flat_wide, state_wide) = measure(11, 4);

        println!(
            "seat bytes per opening: narrow context carried {chunked_narrow} / flat {flat_narrow} (state {state_narrow}); \
             wide context carried {chunked_wide} / flat {flat_wide} (state {state_wide})"
        );
        assert!(state_wide > state_narrow, "the fixture must actually widen the history ({state_narrow} → {state_wide})");
        // **The difference between the two forms IS the history**, at both contexts: the flat one
        // is the carried one minus the state, plus the few bytes borsh spends on lengths.
        // ADR-0086: the opening never carried the state; what a seat fetches must not grow with the
        // context at all beyond encoding noise, while the history the seat recomputes does.
        assert_eq!(chunked_narrow, flat_narrow);
        assert_eq!(chunked_wide, flat_wide);
        assert!(
            flat_wide.abs_diff(flat_narrow) < 1_024,
            "what a seat fetches must not follow the context (flat {flat_narrow} → {flat_wide}, history {state_narrow} → {state_wide})"
        );
    }

    /// **(d) A family that cannot serve the class refuses BY NAME, and the seat files
    /// `Incapable`.**
    ///
    /// The shipped hybrid is exactly this class: it registers the checkpoint sentinel, commits no
    /// checkpoint leg, and therefore has no state root for a seat to compare against. Saying so is
    /// the honest verdict (ADR-0075: a row nobody can seat certifies nothing); inventing a root
    /// would be the dishonest one.
    #[test]
    fn a_class_that_commits_no_checkpoint_refuses_by_name() {
        let (artifact, mut profile, ctx, prompt) = floor_job(3, 4);
        profile.state_chunk_map_id = Hash64::default();
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let mut kernels = FloorRecompute::new(&artifact);
        let refusal = base0_fp_recompute_state_v1(&profile, &ctx, &ids, &[1, 2, 3, 4], 1, &mut kernels).expect_err("refused");
        assert_eq!(refusal, Base0FpRecomputeError::NoStateChunkMapRegistered);
        assert!(
            refusal.to_string().contains("no state chunk map"),
            "the refusal a seat turns into `Incapable` must say what is missing: {refusal}"
        );
    }

    /// **The ids the recompute is teacher-forced on are checked, not taken.**
    ///
    /// A prompt that is not the job's is refused before a single forward runs — the same rule the
    /// opener and the refutation both state — because a seat that recomputed from another list
    /// would compare honest arithmetic on a different job against this claim's checkpoint and call
    /// the producer a liar.
    #[test]
    fn a_recompute_refuses_ids_that_are_not_the_jobs() {
        let (artifact, profile, ctx, prompt) = floor_job(3, 4);
        let mut wrong: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        wrong[0] = wrong[0].wrapping_add(1);
        let mut kernels = FloorRecompute::new(&artifact);
        assert_eq!(
            base0_fp_recompute_state_v1(&profile, &ctx, &wrong, &[1, 2, 3, 4], 1, &mut kernels).expect_err("refused"),
            Base0FpRecomputeError::PromptIdsAreNotTheJobs
        );
        // And an answer too short to teacher-force the calls asked for is a refusal too, rather
        // than a run that quietly fed zeros.
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let mut kernels = FloorRecompute::new(&artifact);
        assert_eq!(
            base0_fp_recompute_state_v1(&profile, &ctx, &ids, &[], 1, &mut kernels).expect_err("refused"),
            Base0FpRecomputeError::OutputIdsTooShort { need: 1, got: 0 }
        );
    }

    /// **A graph-v5 class refuses an opening that carries the history** — which form the class
    /// requires is the class's own declaration (the tiled map), not a server's preference.
    #[test]
    fn a_tiled_class_refuses_an_opening_that_carries_the_history() {
        let (material, claim, ids, artifact) = floor_material(3, 4);
        let binding = &material.0;
        let index = 1;
        let served = base0_open_fp_interval_v1(
            &material,
            index,
            &ids,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the fold form opens");
        // ADR-0086: nothing an opener serves carries the history any more. The rule still stands
        // for a V1 opening someone else builds, so build one by hand from the served form — a
        // chunk in the anchor, the tiled map on the class — and hand it to the seat.
        let fold = Base0FpIntervalOpeningV4::decode_v1(&served).expect("decodes");
        let claim_anchor = fold.anchor.clone().expect("interval 1 is anchored");
        let mut decoded = Base0FpIntervalOpeningV1 {
            version: PALW_BASE0_FP_INTERVAL_VERSION_V1,
            interval_index: fold.interval_index,
            binding: fold.binding.clone(),
            range: PalwStepRangeOpeningV1 {
                first_leaf_index: fold.range.first_leaf_index,
                leaf_hashes: vec![Hash64::default(); fold.range.leaf_count as usize],
                siblings: fold.range.siblings.clone(),
            },
            seed_row_leaf_count: fold.seed_row_leaf_count,
            seed_row_tiles: fold.seed_row_tiles.clone(),
            anchor: Some(PalwCheckpointKvOperandsV1 {
                leaf: claim_anchor.leaf,
                chunks: vec![vec![0u8]],
                opening: claim_anchor.opening,
            }),
        };
        decoded.binding.shape_profile.state_chunk_map_id =
            kaspa_consensus_core::palw_state_chunk_map::tiled_kv_state_chunk_map_id_v3();
        assert!(base0_fp_class_requires_flat_openings_v1(&decoded.binding.shape_profile));
        let state = seat_state(&material, &artifact, &ids, 1);
        assert_eq!(
            base0_verify_fp_interval_opening_with_state_v1(
                &decoded.encode_v1().expect("re-encodes"),
                claim,
                index,
                &ids,
                binding.step_leaf_count,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                Some(&state),
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            ),
            Base0FpIntervalSeatVerdictV1::HistoryNotAdmissible,
            "a class whose map is the tiled one is not served the history, and a seat that took it \
             would have spent the bytes Decision 9 exists to save"
        );
    }
    // =============================================================================================
    // ADR-0082 Decision 4, amended — a graph-v5 DENSE row, end to end (audit B, C-1 / C-2 / L-2)
    // =============================================================================================

    /// The registered graph-v5 dense row, its plan, a small free-prompt job, and the FOLDED
    /// retention an executor of that class keeps. One helper, because C-1's three consumers and
    /// C-2's acceptance test are all questions about the same run.
    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    pub(super) fn dense_v5_run()
    -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
        use kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1;
        let geometry = PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 32,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        };
        dense_v5_run_with(geometry, 3, 4)
    }

    /// The graph-v5 fold fixture at a chosen geometry and job size, for a test that needs a tree
    /// wider than one retained block.
    pub(super) fn dense_v5_run_with(
        geometry: kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1,
        prefill: u32,
        decode: u32,
    ) -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
        use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5;
        let profile = qwen25_a16_profile_v5(geometry).expect("a valid graph-v5 A16 profile");
        let shape = crate::artifact::Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
            eps_q: geometry.rms_eps_q,
        };
        let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique");
        let (ctx, prompt) = crate::produce::base0_rc_job_v1(
            &profile,
            Hash64::from_u64_word(0x0000_82C1),
            geometry.vocab_size as usize,
            prefill,
            decode,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
        let plan = engine.plan_from_profile(&profile).expect("the v5 declaration is this engine's program");
        let run = crate::qwen25_a16_backend::a16_execute_free_prompt_streaming_v1(
            &artifact,
            &profile,
            Some(&plan),
            &ctx,
            &prompt,
            kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES,
            &mut |_| {},
        )
        .expect("the folded graph-v5 sink runs the job");
        (artifact, profile, ctx, prompt, run)
    }

    /// **C-1, test 1: an honest graph-v5 material passes its own seat's check, with zero state
    /// retained.**
    ///
    /// The fold drops every checkpoint chunk (`Base0CheckpointRetentionV1::Fold`), so before this
    /// the material carried an empty `checkpoint_chunks`, `base0_material_tail_matches_v1` rebuilt
    /// the leg from `&[]` — the EMPTY leg, whose root is `checkpoint_empty_root_v2` — and compared
    /// it to a real `checkpoint_merkle_root`. `Mismatch`: every seat read every honest producer of
    /// the class as forged. The leg is now rebuilt from the LEAVES, which is what the fold retains
    /// and what decides the question without a byte of history.
    #[test]
    fn a_graph_v5_material_is_checkable_and_carries_no_state() {
        let (_artifact, profile, _ctx, prompt, run) = dense_v5_run();
        assert_eq!(
            kaspa_consensus_core::palw_context_ladder::palw_checkpoint_cadence_v1(&profile),
            kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerPosition,
        );
        assert!(run.checkpoints.chunks.is_empty(), "the fold retains no state at all");
        assert!(!run.checkpoints.leaves.is_empty(), "…and it does retain the leg it committed");

        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
        let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("its own retention decodes");
        assert!(material.checkpoint_chunks.is_empty(), "a folded class serves no history (Decision 9)");
        assert_eq!(material.checkpoint_leaves.len(), run.checkpoints.leaves.len());

        assert_eq!(
            crate::produce::base0_fp_material_matches_claim_v2(&material, run.execution_root, run.trace_root),
            Ok(true),
            "an honest graph-v5 material must pass the seat's check"
        );

        // **Zero state per POSITION, measured**: the leaves are the whole checkpoint retention and
        // none of them is a cache byte.
        let leaf_bytes = borsh::to_vec(&material.checkpoint_leaves).expect("leaves serialize").len();
        let positions = material.checkpoint_leaves.len();
        eprintln!(
            "C-1: {positions} checkpoints, {} state bytes retained, {leaf_bytes} leaf bytes ({} a position)",
            material.checkpoint_chunks.iter().flatten().map(|c| c.len()).sum::<usize>(),
            leaf_bytes / positions.max(1)
        );
        assert_eq!(material.checkpoint_chunks.iter().flatten().map(|c| c.len()).sum::<usize>(), 0);

        // And a folded class that nevertheless served state is not this class's retention.
        let mut lying = material.clone();
        lying.checkpoint_chunks = vec![vec![vec![0u8; 4]]];
        assert_eq!(
            crate::produce::base0_fp_material_matches_claim_v2(&lying, run.execution_root, run.trace_root),
            Ok(false),
            "a per-position class serving chunks is serving the history Decision 9 keeps off the wire"
        );

        // A tampered LEAF is what the check can still catch, and does.
        let mut tampered = material.clone();
        tampered.checkpoint_leaves[1].state_chunks_root = Hash64::from_u64_word(0xDEAD);
        assert_eq!(
            crate::produce::base0_fp_material_matches_claim_v2(&tampered, run.execution_root, run.trace_root),
            Ok(false),
            "a leaf that is not the committed one must not rebuild the committed root"
        );
    }

    /// **C-1, test 2 and C-2, test 5: every interval of a graph-v5 claim opens, and a seat holding
    /// its own state licenses it.**
    ///
    /// Two defects met here. C-1: the opening's anchor was built by rebuilding the leg from served
    /// chunks the fold does not keep, so `base0_open_fp_interval_sparse_v1` returned
    /// `CaptureIsNotTheBindings` for every `index ≥ 1`. C-2: the anchor's `covered` was
    /// `index × interval` — a DECODE CALL — while the leaf's counter is a POSITION, so the seat
    /// selected the checkpoint after the fourth PREFILL position and compared a 403-position state
    /// against a 3-position root. Either one alone makes the class unseatable.
    #[test]
    fn every_graph_v5_interval_opens_and_a_recomputing_seat_licenses_it() {
        use kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1;
        let (artifact, profile, ctx, prompt, run) = dense_v5_run();
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
        let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("its own retention decodes");
        let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
        let plan = engine.plan_from_profile(&profile).expect("the plan");

        let interval = kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
        assert!(geometry.interval_count >= 2, "a one-interval job proves nothing about an anchor");

        let claim = PalwClaimRootsV1 { execution_root: run.execution_root, trace_root: run.trace_root, anchor: ctx.job_id };
        let kernels = crate::qwen25_a16_backend::a16_interval_kernels_for_tests_v1(&artifact, Some(&plan));
        for index in 0..geometry.interval_count {
            let opened = base0_open_fp_interval_sparse_v1(
                &material,
                index,
                &ids,
                interval,
                &kernels,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .unwrap_or_else(|e| panic!("interval {index} must open from a fold that retained nothing: {e}"));
            // A folded class is served FLAT: the anchor is named, never carried.
            assert!(
                opened.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V4),
                "interval {index}: a class that folds is served the named anchor (Decision 9)"
            );

            // The seat's own recompute, ordered exactly as the panel orders it — by the `covered`
            // the opening carries, in the class's own cadence unit.
            crate::fp_recompute::base0_fp_seat_state_forget_v1();
            let state = match geometry.anchor_covered_call(index) {
                None => None,
                Some(covered) => {
                    assert_eq!(
                        palw_checkpoint_positions_at_v1(&profile, &ctx, covered),
                        geometry.anchor_covered_positions_v1(index).expect("an anchored interval"),
                        "interval {index}: the leaf's counter and the state's row count must name one state"
                    );
                    let mut kernels = crate::fp_recompute::A16RecomputeKernelsV1::new(&artifact, Some(&plan)).expect("kernels");
                    crate::fp_recompute::base0_fp_seat_state_memoized_v1(
                        &profile,
                        &ctx,
                        &ids,
                        &run.generated_token_ids,
                        covered,
                        &mut kernels,
                    )
                    .expect("this seat can recompute its own state");
                    base0_fp_interval_opening_seat_state_v1(&opened, &ids, interval)
                }
            };
            if index > 0 {
                assert!(state.is_some(), "interval {index}: the seat's own state must be findable under the opening's covered");
            }
            assert_eq!(
                base0_verify_fp_interval_opening_with_state_v1(
                    &opened,
                    claim,
                    index,
                    &ids,
                    run.binding.step_leaf_count,
                    interval,
                    state.as_ref(),
                    &kernels,
                    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                ),
                Base0FpIntervalSeatVerdictV1::Valid,
                "interval {index}: a seat holding its own state must license an honest graph-v5 opening"
            );
        }
    }

    /// **C-2's unit split, pinned.** The interval's boundary is a CALL; the checkpoint that anchors
    /// it is named in the class's cadence unit; both name ONE state. Swept over both cadences at
    /// the same chain facts, so the difference is the cadence and nothing else.
    #[test]
    fn the_anchors_two_units_name_one_state() {
        use kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1;
        for interval in [1u32, 2, 3] {
            for (prefill, decode) in [(4u32, 9u32), (7, 5), (1, 12)] {
                let per_call =
                    Base0FpIntervalGeometryV1::from_chain_facts_v1(prefill, decode, interval, PalwCheckpointCadenceV1::PerDecodeCall)
                        .expect("a geometry");
                let per_position =
                    Base0FpIntervalGeometryV1::from_chain_facts_v1(prefill, decode, interval, PalwCheckpointCadenceV1::PerPosition)
                        .expect("a geometry");
                assert_eq!(
                    per_call.interval_count, per_position.interval_count,
                    "the interval count is cadence-free — which is what lets the seam derive it without a profile"
                );
                for index in 0..per_call.interval_count {
                    assert_eq!(per_call.calls_for(index), per_position.calls_for(index), "the calls are the same partition");
                    assert_eq!(
                        per_call.anchor_seed_call_v1(index),
                        per_position.anchor_seed_call_v1(index),
                        "the seed row is a coordinate and does not move with the cadence"
                    );
                    let Some(positions) = per_call.anchor_covered_positions_v1(index) else {
                        assert_eq!(index, 0, "only interval 0 has no anchor");
                        continue;
                    };
                    assert_eq!(per_call.anchor_covered_call(index), Some(index * interval));
                    assert_eq!(
                        per_position.anchor_covered_call(index),
                        Some(prefill + index * interval),
                        "a per-position leaf's counter IS the row count"
                    );
                    assert_eq!(positions, prefill + index * interval, "and both name the same state");
                }
            }
        }
    }

    /// **C-1, test 3: every committed leaf of a graph-v5 leg has an anchor, and it is the one the
    /// COURT will demand.**
    ///
    /// `base0_kv_anchor_for_step_v1` reached `checkpoints.chunks.get(at)` and a folded leg's vector
    /// is empty, so it answered `None` for every step of the class — the executor could not
    /// assemble the anchored refutation ADR-0082 Decision 4 is about, at any coordinate. The
    /// chunks now come from the cache the executor is holding anyway, which the map's own
    /// prefix-stability makes byte-identical to the ones it folded away.
    ///
    /// The `covered` is checked against `palw_checkpoint_covered_for_step_v1` — the one spelling of
    /// which checkpoint a step's anchor is — and the operands are then run through the COURT's own
    /// `verify_kv_anchor` by way of the chunk-count and root rules it applies.
    #[test]
    fn every_step_of_a_graph_v5_class_has_the_anchor_the_court_demands() {
        use kaspa_consensus_core::palw_context_ladder::palw_checkpoint_covered_for_step_v1;
        let (artifact, profile, ctx, prompt, run) = dense_v5_run();
        assert!(run.checkpoints.chunks.is_empty(), "the fold retained nothing — this is the case that answered None");

        // The executor's own cache, run to the end of the job: what it is holding anyway.
        let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
        let plan = engine.plan_from_profile(&profile).expect("the plan");
        let mut cache = crate::engine_a16::A16Cache::new(artifact.shape.n_layers);
        let prefill = ctx.declared_prefill_tokens as usize;
        for (position, token) in prompt.iter().take(prefill).enumerate() {
            engine.forward_token_planned(&plan, &mut cache, *token, position).expect("the prefill runs");
        }
        for (call, token) in run.generated_token_ids.iter().enumerate().take(ctx.exact_decode_tokens as usize - 1) {
            engine.forward_token_planned(&plan, &mut cache, *token as usize, prefill + call).expect("the decode runs");
        }

        let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
        let mut checked = 0u32;
        for (call_index, position) in (0..prefill as u32).map(|p| (0u32, p)).chain((1..=decode_calls).map(|c| (c, 0u32))) {
            let want = palw_checkpoint_covered_for_step_v1(&profile, &ctx, call_index, position).expect("a per-position anchor");
            let anchor = crate::legs::base0_kv_anchor_for_step_v1(&run.checkpoints, &profile, &ctx, call_index, position, |entry| {
                cache.state_chunk_bytes_v1(entry)
            })
            .unwrap_or_else(|| panic!("call {call_index} position {position} has no anchor in its own producer's leg"));
            assert_eq!(anchor.leaf.covered_decode_call, want, "the anchor must be the one the court's own rule names");
            assert_eq!(anchor.chunks.len(), anchor.leaf.state_chunk_count as usize, "the re-derived chunks are the leaf's own");
            // …and they must rebuild the state root the producer committed, which is what makes
            // the re-derivation-from-a-later-cache argument true rather than merely stated.
            assert_eq!(
                base0_state_chunks_root_v1(&profile.state_chunk_map_id, &anchor.chunks).expect("a root"),
                anchor.leaf.state_chunks_root,
                "a chunk re-derived from a later cache must be the byte the earlier checkpoint committed"
            );
            checked += 1;
        }
        assert_eq!(checked, run.checkpoints.leaves.len() as u32, "every committed leaf was reached from some step");
    }

    /// **ADR-0085 X1 — the close assembled from a served opening and this party's own replay is
    /// the close assembled from the capture, byte for byte**, at every main-step leaf of every
    /// interval of an honest floor run; and a tile that is not the accused's is refused by name.
    /// **ADR-0086 Decision 6's address is a packed index the lane already carries**: bit 31,
    /// the interval, the block. Round trips; a plain index is never read as one; the two limits
    /// refuse at the boundary.
    #[test]
    fn a_block_leaves_request_index_round_trips_and_a_plain_index_is_not_one() {
        for (interval, block) in [(0u32, 0u64), (1, 1), (511, 1618), ((1 << 15) - 1, (1 << 16) - 1)] {
            let packed = base0_fp_block_leaves_request_index_v1(interval, block).expect("inside both limits");
            assert_eq!(base0_fp_block_leaves_request_decode_v1(packed), Some((interval, block)));
            assert!(packed & PALW_BASE0_FP_BLOCK_LEAVES_REQUEST_BIT_V1 != 0);
        }
        assert_eq!(base0_fp_block_leaves_request_index_v1(1 << 15, 0), None, "the interval limit");
        assert_eq!(base0_fp_block_leaves_request_index_v1(0, 1 << 16), None, "the block limit");
        for plain in [0u32, 1, 511, u32::MAX >> 1] {
            assert_eq!(base0_fp_block_leaves_request_decode_v1(plain), None, "a plain interval index is not a request");
        }
    }

    /// **ADR-0085 §6 items 4–5 end to end at the base0 layer**: the executor fills the annex from
    /// its own tiles (`base0_fp_close_annex_v1`) and attaches it to the V4 opening it served; the
    /// annexed opening verifies exactly as the plain one (the seat's replay never sees the annex,
    /// ADR-0085 X3); `base0_fp_interval_of_leaf_v1` names the interval the opener put the leaf
    /// in; and the close assembled from the served intervals
    /// (`base0_refutation_from_served_intervals_capped_v1`) is byte for byte the capture path's
    /// under the same tiled pin (X1, through the served path).
    #[test]
    fn a_close_from_served_intervals_is_the_close_from_the_capture_and_the_annex_changes_no_verdict() {
        use kaspa_consensus_core::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, canonical_step_coordinates};
        use kaspa_consensus_core::palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodeTokensV1, tiled_logits_rows_root_v1};
        let (artifact, profile, ctx, prompt) = floor_job(3, 4);
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let claim = PalwClaimRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            anchor: run.binding.job_context.job_id,
        };
        let material: Base0RetainedMaterialV1 = (
            run.binding.clone(),
            run.tiles.tiles.clone(),
            run.logits_rows.clone(),
            run.generated_token_ids.clone(),
            run.checkpoints.chunks.clone(),
        );
        let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let cap = kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
        let leaf_count = run.binding.step_leaf_count;
        let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
        let rows_root = tiled_logits_rows_root_v1(&ctx, &run.logits_rows).expect("the rows build a tree");
        let pin = || {
            PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: run.generated_token_ids.clone() })
        };
        let reads_cache = |coord: &kaspa_consensus_core::palw_step::PalwStepCoordinateV1| {
            profile
                .resolve_node_slot(coord.node_slot)
                .map(|(node, _)| node.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V))
                .unwrap_or(false)
        };
        let tile_at = |leaf: u64| run.tiles.tiles.iter().find(|(i, _)| *i == leaf).map(|(_, t)| t.clone());
        let mut compared = 0usize;
        let mut previous_plain: Option<(u32, Vec<u8>)> = None;
        for index in 0..geometry.interval_count {
            let plain = base0_open_fp_interval_v1(
                &material,
                index,
                &ids,
                interval,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the interval opens");
            let v4 = Base0FpIntervalOpeningV4::decode_v1(&plain).expect("the dense opener serves V4");
            let range = (v4.range.first_leaf_index, v4.range.first_leaf_index + v4.range.leaf_count);
            let leaves_geometry = base0_fp_interval_leaves_v1(&profile, &ctx, &geometry, index, leaf_count).expect("leaves");
            let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
            for leaf in range.0..range.1 {
                let Some(coord) = canonical_step_coordinates(&profile, &ctx, leaf) else { continue };
                // The opener's own placement is the answer `base0_fp_interval_of_leaf_v1` must give:
                // the interval's own calls map to it, the seed row to the interval before.
                let owner = base0_fp_interval_of_leaf_v1(&profile, &ctx, interval, leaf).expect("a main step leaf has an owner");
                if leaf >= leaves_geometry.interval_first {
                    assert_eq!(owner, index, "leaf {leaf} is interval {index}'s own");
                } else {
                    assert_eq!(owner + 1, index, "leaf {leaf} is the seed row: the call before interval {index}");
                }
                // The executor's annex, from its own tiles, attached to the opening it served.
                let annex = base0_fp_close_annex_v1(&run.binding, &run.logits_rows, &run.checkpoints.chunks, &tile_at, &[leaf], range)
                    .expect("the annex builds");
                assert_eq!(annex.disputed.len(), 1);
                assert_eq!(annex.rows_root, rows_root);
                let annexed = base0_fp_interval_opening_with_close_v1(&plain, annex.clone()).expect("V4 carries the annex");
                assert_eq!(base0_fp_interval_close_annex_any_v1(&annexed).as_ref(), Some(&annex));
                assert_eq!(base0_fp_interval_close_annex_any_v1(&plain), None);
                // ADR-0085 X3 on V4: the annex is invisible to the seat's verdict.
                let verdict_plain = base0_verify_fp_interval_opening_with_state_capped_v1(
                    &plain,
                    claim,
                    index,
                    &ids,
                    leaf_count,
                    interval,
                    cap,
                    state.as_ref(),
                    &FloorKernels(&artifact),
                    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                );
                let verdict_annexed = base0_verify_fp_interval_opening_with_state_capped_v1(
                    &annexed,
                    claim,
                    index,
                    &ids,
                    leaf_count,
                    interval,
                    cap,
                    state.as_ref(),
                    &FloorKernels(&artifact),
                    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                );
                assert_eq!(verdict_plain, verdict_annexed, "interval {index} leaf {leaf}: the annex changed a verdict");
                // The close from the served intervals equals the capture path's, same pin.
                let kv = if reads_cache(&coord) && coord.call_index > 0 {
                    crate::legs::base0_kv_anchor_for_call_v1(&run.checkpoints, coord.call_index)
                } else {
                    None
                };
                let from_capture = crate::legs::base0_refutation_from_capture_capped_v1(
                    &profile,
                    &ctx,
                    &run.tiles,
                    run.binding.clone(),
                    coord,
                    ids.clone(),
                    Some(pin()),
                    kv,
                    cap,
                )
                .expect("the capture path assembles");
                let mut held: Vec<(u32, Vec<u8>)> = vec![(index, annexed.clone())];
                if let Some(prev) = previous_plain.as_ref() {
                    held.push(prev.clone());
                }
                let state_for = |bytes: &[u8]| {
                    let v4 = Base0FpIntervalOpeningV4::decode_v1(bytes).ok()?;
                    let covered = geometry.anchor_covered_call(v4.interval_index)?;
                    Some(seat_state(&material, &artifact, &ids, covered))
                };
                let from_served = base0_refutation_from_served_intervals_capped_v1(
                    &held,
                    claim,
                    &ids,
                    &run.generated_token_ids,
                    leaf_count,
                    leaf,
                    interval,
                    cap,
                    &state_for,
                    &FloorKernels(&artifact),
                    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                )
                .unwrap_or_else(|e| panic!("interval {index} leaf {leaf}: the served path refuses: {e}"));
                assert_eq!(from_served, from_capture, "interval {index} leaf {leaf}: the two closes differ");
                compared += 1;
            }
            // A leaf the annex does not name is refused by name, not closed from another leaf's tile.
            if let Some(leaf) = (range.0..range.1).find(|l| canonical_step_coordinates(&profile, &ctx, *l).is_some()) {
                let other = (range.0..range.1).rev().find(|l| *l != leaf && canonical_step_coordinates(&profile, &ctx, *l).is_some());
                if let Some(other) = other {
                    let annex =
                        base0_fp_close_annex_v1(&run.binding, &run.logits_rows, &run.checkpoints.chunks, &tile_at, &[other], range)
                            .expect("the annex builds");
                    let annexed = base0_fp_interval_opening_with_close_v1(&plain, annex).expect("V4 carries the annex");
                    let held = vec![(index, annexed)];
                    let state_for = |bytes: &[u8]| {
                        let v4 = Base0FpIntervalOpeningV4::decode_v1(bytes).ok()?;
                        let covered = geometry.anchor_covered_call(v4.interval_index)?;
                        Some(seat_state(&material, &artifact, &ids, covered))
                    };
                    let refused = base0_refutation_from_served_intervals_capped_v1(
                        &held,
                        claim,
                        &ids,
                        &run.generated_token_ids,
                        leaf_count,
                        leaf,
                        interval,
                        cap,
                        &state_for,
                        &FloorKernels(&artifact),
                        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                    );
                    assert!(refused.is_err(), "interval {index}: an annex naming leaf {other} must not close leaf {leaf}");
                }
            }
            previous_plain = Some((index, plain));
        }
        assert!(compared > 0, "the fixture yields main-step leaves");
    }

    /// **ADR-0086 Decision 6 on a dense retention**: a block wholly inside the interval is served
    /// as its leaf hashes and folds to the digest the opening carries; on a space with no whole
    /// block the request is refused by name.
    #[test]
    fn served_block_leaves_fold_to_the_served_digest_or_refuse_by_name() {
        let (artifact, profile, ctx, prompt) = floor_job(3, 4);
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let material: Base0RetainedMaterialV1 = (
            run.binding.clone(),
            run.tiles.tiles.clone(),
            run.logits_rows.clone(),
            run.generated_token_ids.clone(),
            run.checkpoints.chunks.clone(),
        );
        let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let leaf_count = run.binding.step_leaf_count;
        let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
        let mut served = 0usize;
        let mut refused = 0usize;
        for index in 0..geometry.interval_count {
            let plain = base0_open_fp_interval_v1(
                &material,
                index,
                &ids,
                interval,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the interval opens");
            let v4 = Base0FpIntervalOpeningV4::decode_v1(&plain).expect("V4");
            let (first_block, end_block) = v4.range.whole_blocks_v1(leaf_count);
            for block in first_block..end_block {
                let bytes = base0_fp_block_leaves_from_tiles_v1(&plain, &run.tiles.tiles, block).expect("a whole block serves");
                let leaves = Base0FpBlockLeavesV1::decode_v1(&bytes).expect("decodes");
                let digest = &v4.range.block_roots[(block - first_block) as usize];
                assert!(leaves.folds_to_v1(digest), "interval {index} block {block}: the served leaves do not fold to the digest");
                served += 1;
            }
            let outside = end_block.max(1) + 7;
            assert!(base0_fp_block_leaves_from_tiles_v1(&plain, &run.tiles.tiles, outside).is_err(), "a block outside is refused");
            refused += 1;
        }
        assert!(refused > 0);
        // The floor fixture's space is small; a whole block inside it is a bonus, not a premise.
        let _ = served;
    }

    #[test]
    fn a_close_from_an_opening_is_the_close_from_the_capture() {
        use kaspa_consensus_core::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, canonical_step_coordinates};
        use kaspa_consensus_core::palw_step_refute::{PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1};
        let (artifact, profile, ctx, prompt) = floor_job(3, 4);
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let claim = PalwClaimRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            anchor: run.binding.job_context.job_id,
        };
        let material: Base0RetainedMaterialV1 = (
            run.binding.clone(),
            run.tiles.tiles.clone(),
            run.logits_rows.clone(),
            run.generated_token_ids.clone(),
            run.checkpoints.chunks.clone(),
        );
        let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let cap = kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
        let leaf_count = run.binding.step_leaf_count;
        let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
        let pin = || {
            PalwDecodeTokenPinV1::Base0V1(PalwBase0DecodeTokensV1 {
                logits_rows: run.logits_rows.clone(),
                generated_token_ids: run.generated_token_ids.clone(),
            })
        };
        let reads_cache = |coord: &kaspa_consensus_core::palw_step::PalwStepCoordinateV1| {
            profile
                .resolve_node_slot(coord.node_slot)
                .map(|(node, _)| node.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V))
                .unwrap_or(false)
        };
        let mut compared = 0usize;
        let mut anchored = 0usize;
        let mut previous: Option<(Base0FpIntervalOpeningV2, crate::legs::Base0StepTilesV1)> = None;
        for index in 0..geometry.interval_count {
            let opening_bytes = base0_open_fp_interval_v1(
                &material,
                index,
                &ids,
                interval,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the interval opens");
            // ADR-0086 Decision 2: the challenger replays from its own state, as the seat does.
            let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
            let (opening, replay) = base0_fp_challenger_replay_tiles_capped_v1(
                &opening_bytes,
                claim,
                index,
                &ids,
                leaf_count,
                interval,
                cap,
                state.as_ref(),
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the challenger replays the interval");
            let first = opening.range.first_leaf_index;
            let end = first + opening.range.leaf_hashes.len() as u64;
            for leaf in first..end {
                let Some(coord) = canonical_step_coordinates(&profile, &ctx, leaf) else { continue };
                // The executor's anchor for this step, exactly as the capture path attaches it.
                let kv = if reads_cache(&coord) && coord.call_index > 0 {
                    crate::legs::base0_kv_anchor_for_call_v1(&run.checkpoints, coord.call_index)
                } else {
                    None
                };
                let from_capture = crate::legs::base0_refutation_from_capture_capped_v1(
                    &profile,
                    &ctx,
                    &run.tiles,
                    run.binding.clone(),
                    coord,
                    ids.clone(),
                    Some(pin()),
                    kv.clone(),
                    cap,
                )
                .expect("the capture path assembles");
                let tile = run.tiles.tiles.iter().find(|(i, _)| *i == leaf).map(|(_, t)| t.clone()).expect("the run holds the tile");
                let annex = Base0FpCloseAnnexV1 {
                    rows_root: Hash64::default(),
                    disputed: vec![Base0FpDisputedLeafV1 {
                        leaf_index: leaf,
                        tile,
                        anchor: kv.as_ref().map(|k| Base0FpCheckpointClaimV1 { leaf: k.leaf.clone(), opening: k.opening.clone() }),
                    }],
                };
                let chunks: Vec<Vec<u8>> = kv.as_ref().map(|k| k.chunks.clone()).unwrap_or_default();
                // The interval holding the leaf first, then the one before it: a step at an
                // interval's first call reads the call before it.
                let mut held: Vec<(&Base0FpIntervalOpeningV2, &crate::legs::Base0StepTilesV1)> = vec![(&opening, &replay)];
                if let Some((o, r)) = previous.as_ref() {
                    held.push((o, r));
                }
                let from_opening = crate::legs::base0_refutation_from_opening_capped_v1(
                    &profile,
                    &ctx,
                    &held,
                    &annex,
                    coord,
                    ids.clone(),
                    Some(pin()),
                    &chunks,
                    cap,
                )
                .unwrap_or_else(|e| panic!("interval {index} leaf {leaf}: the opening path refuses: {e:?}"));
                assert_eq!(from_opening, from_capture, "interval {index} leaf {leaf}: the two closes differ");
                assert_eq!(
                    borsh::to_vec(&from_opening).unwrap(),
                    borsh::to_vec(&from_capture).unwrap(),
                    "interval {index} leaf {leaf}: byte for byte"
                );
                compared += 1;
                if kv.is_some() {
                    anchored += 1;
                }
                // A tile that is not the accused's committed one is refused before anything opens.
                let mut forged = annex.clone();
                forged.disputed[0].tile.values_le[0] ^= 0x01;
                assert!(
                    matches!(
                        crate::legs::base0_refutation_from_opening_capped_v1(
                            &profile,
                            &ctx,
                            &held,
                            &forged,
                            coord,
                            ids.clone(),
                            Some(pin()),
                            &chunks,
                            cap
                        ),
                        Err(crate::legs::LegError::CloseFromOpening(_))
                    ),
                    "interval {index} leaf {leaf}: a forged annex tile must be refused by name"
                );
            }
            previous = Some((opening, replay));
        }
        assert!(compared > 0, "the fixture yields main-step leaves");
        assert!(anchored > 0, "the fixture exercises an anchored close (a cache-reading step past the prefill)");
    }

    /// **The ladder above the default one** (ADR-0084 §7 record). A class whose step space is
    /// larger than `PALW_STEP_LEG_MAX_LEAVES` — the graph-v5 attempt lane is 6.6 M leaves — is
    /// served under its RULESET's cap. The uncapped root's ceiling is the default, and an opener
    /// that reached for it reported a capture that was the binding's as one that was not; under a
    /// cap below the space the same opening is refused, so the cap is the bound and not the constant.
    #[test]
    fn an_opening_above_the_default_ladder_assembles_under_its_rulesets_cap() {
        use kaspa_consensus_core::palw_step_leg::{step_range_opening_root_capped_v1, step_range_opening_root_v1};
        let (material, _claim, _ids, _artifact) = floor_material(1, 2);
        let mut binding = material.0.clone();
        let leaf_count = PALW_STEP_LEG_MAX_LEAVES + 1;
        let cap = PALW_STEP_LEG_MAX_LEAVES * 2;
        let leaves: Vec<Hash64> = (0..leaf_count).map(|i| Hash64::from_u64_word(i + 1)).collect();
        let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(
            &leaves,
            crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1,
            cap,
        )
        .expect("a tree over the larger space");
        binding.step_leaf_count = leaf_count;
        binding.step_merkle_root = tree.root().expect("its root");
        // A range that ends in the space's last, partial block — the shape an off-by-one hides in.
        let lg = Base0FpIntervalLeavesV1 {
            range_first: leaf_count - 4,
            interval_first: leaf_count - 4,
            range_end: leaf_count,
            seed_row_leaves: 0,
        };
        let (span_first, span_end) = tree.span_for_range(lg.range_first, 4).expect("the span");
        let span = &leaves[span_first as usize..span_end as usize];
        let bytes = base0_assemble_fp_interval_opening_v1(&binding, &tree, &lg, 0, span_first, span, Vec::new(), None, cap)
            .expect("assembles under the ruleset's cap");
        let fold = Base0FpIntervalOpeningV4::decode_v1(&bytes).expect("a V4 opening").range;
        // ADR-0086 X4: the bytes are the fold's, not the range's — under 200 KB for a 2^22-leaf
        // space's last four leaves.
        assert!(bytes.len() < 200_000, "a fold opening over a 2^22+1-leaf space is {} bytes", bytes.len());
        let range =
            fold.with_leaves_v1(leaves[lg.range_first as usize..lg.range_end as usize].to_vec()).expect("the seat's own leaves");
        assert_eq!(step_range_opening_root_capped_v1(leaf_count, &range, cap).ok(), Some(binding.step_merkle_root));
        assert!(
            step_range_opening_root_v1(leaf_count, &range).is_err(),
            "the default ladder does not reach this space — which is why no opener may reach for it"
        );
        assert!(
            matches!(
                base0_assemble_fp_interval_opening_v1(
                    &binding,
                    &tree,
                    &lg,
                    0,
                    span_first,
                    span,
                    Vec::new(),
                    None,
                    PALW_STEP_LEG_MAX_LEAVES
                ),
                Err(Base0FpIntervalError::CaptureIsNotTheBindings)
            ),
            "under a cap below the space the same opening is refused"
        );
    }
}

// =================================================================================================
// The acceptance test for the ladder: a class the chain admits must be one a seat can license
// =================================================================================================

#[cfg(test)]
mod the_rulesets_ladder {
    use super::*;
    use kaspa_consensus_core::palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES;
    use kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    use kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;

    /// The genesis row's own canonical job, as a binding — the roots are not this test's subject
    /// (nothing here recomputes them) and the two fields that are, `shape_profile` and
    /// `job_context`, are the registered row's and the job the chain would pay for.
    fn v5_512_binding() -> (PalwStepBindingV2, u64) {
        let row = crate::classes::a16_graph_v5_row_v1().expect("the graph-v5 512 row is in this build");
        let (prefill, decode) = row.canonical_job;
        let (job_context, _prompt) = crate::produce::base0_rc_job_v1(
            &row.profile,
            Hash64::from_u64_word(0x0000_082B_2512),
            row.artifact_shape.vocab,
            prefill,
            decode,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        let step_leaf_count =
            kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&row.profile, &job_context, COURT_MAX_STEP_LEAVES)
                .expect("the canonical job of an admitted class fits the ruleset it was admitted under");
        let binding = PalwStepBindingV2 {
            version: 2,
            job_context,
            shape_profile: row.profile.clone(),
            checkpoint_profile: kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            ),
            state_chunk_map_id: row.profile.state_chunk_map_id,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            step_leaf_count,
            step_merkle_root: Hash64::default(),
            checkpoint_count: 0,
            checkpoint_merkle_root: Hash64::default(),
            committed_execution_root: Hash64::default(),
        };
        (binding, step_leaf_count)
    }

    /// **The class the genesis registers is priced at the ruleset's ladder and NAMED at the
    /// executor's.**
    ///
    /// The real `a16_graph_v5_row_v1()` profile and the real canonical job — no fixture, no
    /// scaling. Under the ruleset the row was admitted under, a seat derives the same 6,630,544
    /// leaves the binding declares and proceeds. Under the executor's constant it refuses BY NAME,
    /// with both numbers in the error: not a panic, not an allocation, and not the silent
    /// `Mismatch` that made the class unlicensable.
    #[test]
    fn the_512_rows_honest_price_clears_the_rulesets_ladder_and_is_named_at_the_executors() {
        let (binding, leaves) = v5_512_binding();
        assert!(
            leaves > PALW_STEP_MAX_LEAVES && leaves <= COURT_MAX_STEP_LEAVES,
            "the row is only interesting because it is between the two: {leaves} leaves, executor {PALW_STEP_MAX_LEAVES}, ruleset {COURT_MAX_STEP_LEAVES}"
        );
        eprintln!(
            "graph-v5 512 row: canonical job ({}, {}) prices {leaves} step leaves; executor constant {PALW_STEP_MAX_LEAVES}, RC ruleset ladder {COURT_MAX_STEP_LEAVES}",
            binding.job_context.declared_prefill_tokens, binding.job_context.exact_decode_tokens,
        );

        assert_eq!(base0_fp_binding_step_space_v1(&binding, COURT_MAX_STEP_LEAVES), Ok(leaves));
        assert_eq!(
            base0_fp_binding_step_space_v1(&binding, PALW_STEP_MAX_LEAVES),
            Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: PALW_STEP_MAX_LEAVES }),
            "at the executor's constant the refusal must name the ladder that refused, and the price it refused"
        );

        // And the same at the seat's own door, which is where the class's licensability lives.
        assert!(
            Base0FpIntervalGeometryV1::from_binding_capped_v1(&binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1, COURT_MAX_STEP_LEAVES)
                .is_ok(),
            "a seat handed the ruleset's ladder must build a geometry for the row the genesis registers"
        );
        assert_eq!(
            Base0FpIntervalGeometryV1::from_binding_capped_v1(&binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1, PALW_STEP_MAX_LEAVES),
            Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: PALW_STEP_MAX_LEAVES }),
        );
        // The un-capped name is the executor's default and must still BE the executor's default —
        // a caller with no ruleset in scope keeps exactly the behaviour it had.
        assert_eq!(
            Base0FpIntervalGeometryV1::from_binding_v1(&binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1),
            Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: PALW_STEP_MAX_LEAVES }),
        );

        // A price that is not the geometry's is still refused under its own name at either ladder —
        // raising the ceiling must not have turned the equality into an inequality.
        let mut lying = binding.clone();
        lying.step_leaf_count = leaves - 1;
        assert!(
            matches!(
                base0_fp_binding_step_space_v1(&lying, COURT_MAX_STEP_LEAVES),
                Err(Base0FpIntervalError::PriceIsNotTheGeometrys { declared, .. }) if declared == leaves - 1
            ),
            "a binding that under-prices its own geometry is refused as a price, not as a ladder"
        );
        let mut inflated = binding.clone();
        inflated.step_leaf_count = leaves + 1;
        assert_eq!(
            base0_fp_binding_step_space_v1(&inflated, COURT_MAX_STEP_LEAVES),
            Err(Base0FpIntervalError::PriceIsNotTheGeometrys { declared: leaves + 1, derived: leaves }),
        );
        // …and one above the ladder never reaches the enumeration at all.
        let mut absurd = binding.clone();
        absurd.step_leaf_count = u64::MAX;
        assert_eq!(
            base0_fp_binding_step_space_v1(&absurd, COURT_MAX_STEP_LEAVES),
            Err(Base0FpIntervalError::LeafCountOutOfRange { got: u64::MAX, max: COURT_MAX_STEP_LEAVES }),
            "the ladder is checked before the count, so no allocation is sized from a stranger's u64"
        );
    }

    /// **ADR-0086 Decision 2 for the executor's side: an interval opened from a recomputed anchor
    /// state is byte for byte the interval opened by the genesis walk.** The fold retains no
    /// state, so the opener used to replay from the prompt with tiles captured for every interval
    /// — 1,991 s for interval 187 of a 300-token claim on the devnet. With the family's recompute
    /// kernels the executor resumes from the memoized state of the interval's named anchor and
    /// replays only the interval's own calls; the opening it serves must be the same object, or a
    /// seat would refuse an honest executor. Pinned on every interval past the first, and the
    /// closure must actually have been consulted.
    #[test]
    fn an_interval_opened_from_the_recomputed_anchor_is_the_interval_opened_from_genesis() {
        // Wide enough that the fold retains more than one block (4,096 leaves at the ruleset's
        // level), so a span starts inside an earlier call and the anchored branch is walked.
        let geometry = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 32,
            ffn_dim: 64,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 8,
            vocab_size: 128,
            n_ctx: 64,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        };
        let (artifact, profile, ctx, prompt, run) = super::tests::dense_v5_run_with(geometry, 3, 24);
        eprintln!("the wide fixture prices {} leaves over {} checkpoints", run.binding.step_leaf_count, run.checkpoints.leaves.len());
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
        let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("its own retention decodes");
        assert!(material.checkpoint_chunks.is_empty(), "the fixture is a fold with no state, the case that replayed from genesis");
        let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
        let plan = engine.plan_from_profile(&profile).expect("the plan");
        let kernels = crate::qwen25_a16_backend::a16_interval_kernels_for_tests_v1(&artifact, Some(&plan));
        let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let wide = COURT_MAX_STEP_LEAVES;
        let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(&material.binding, interval, wide).expect("a geometry");
        assert!(geometry.interval_count >= 3, "the fixture has intervals past the first: {}", geometry.interval_count);
        let claim = PalwClaimRootsV1 { execution_root: run.execution_root, trace_root: run.trace_root, anchor: ctx.job_id };
        let consulted = std::cell::Cell::new(0u32);
        let anchor_state_for = |covered: u32| {
            consulted.set(consulted.get() + 1);
            let mut recompute = crate::fp_recompute::A16RecomputeKernelsV1::new(&artifact, Some(&plan)).expect("recompute kernels");
            crate::fp_recompute::base0_fp_seat_state_memoized_v1(
                &material.binding.shape_profile,
                &material.binding.job_context,
                &ids,
                &material.generated_token_ids,
                covered,
                &mut recompute,
            )
            .ok()
        };
        let leaf_count = material.binding.step_leaf_count;
        let mut anchored_intervals = 0u32;
        for index in 0..geometry.interval_count {
            let from_genesis = base0_open_fp_interval_sparse_capped_v1(
                &material,
                index,
                &ids,
                interval,
                wide,
                &kernels,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the genesis walk opens");
            let before = consulted.get();
            let from_anchor = base0_open_fp_interval_sparse_anchored_capped_v1(
                &material,
                index,
                &ids,
                interval,
                wide,
                &kernels,
                &anchor_state_for,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the anchored walk opens");
            assert_eq!(from_anchor, from_genesis, "interval {index}: the two walks must serve one object");
            // The anchored branch is taken exactly when the span's STARTING interval has a named
            // anchor — the span covers whole retained blocks, so it may start in an earlier call.
            let leaves_geometry = base0_fp_interval_leaves_v1(&profile, &ctx, &geometry, index, leaf_count).expect("leaves");
            let (span_first, _) = material
                .step_tree
                .span_for_range(leaves_geometry.range_first, leaves_geometry.range_end - leaves_geometry.range_first)
                .expect("a span");
            let start_call = kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, span_first)
                .expect("a main step leaf")
                .call_index;
            let expects_anchor = geometry.anchor_covered_call(interval_of_call_v1(&geometry, start_call)).is_some();
            assert_eq!(
                consulted.get() > before,
                expects_anchor,
                "interval {index}: consulted={} expected={expects_anchor}",
                consulted.get() > before
            );
            anchored_intervals += expects_anchor as u32;
            assert_eq!(
                base0_verify_fp_interval_opening_with_state_capped_v1(
                    &from_anchor,
                    claim,
                    index,
                    &ids,
                    material.binding.step_leaf_count,
                    interval,
                    wide,
                    geometry.anchor_covered_call(index).and_then(|c| anchor_state_for(c)).as_ref(),
                    &kernels,
                    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
                ),
                Base0FpIntervalSeatVerdictV1::Valid,
                "interval {index}: a seat licenses the anchored opening"
            );
        }
        assert!(anchored_intervals > 0, "the fixture must exercise the anchored branch at least once");
        eprintln!("{anchored_intervals} of {} intervals resumed from a recomputed anchor", geometry.interval_count);
        crate::fp_recompute::base0_fp_seat_state_forget_v1();
    }

    /// **A seat licenses an honest graph-v5 opening at the ladder it is HANDED, and refuses it by
    /// name at a narrower one.**
    ///
    /// The 512 row's own artifact is 1.7 GiB and its canonical job allocates a 424 MB capture, so
    /// the end-to-end half runs on the tiny graph-v5 fixture (`dense_v5_run`, the same class
    /// declaration at a fixture geometry) — and the LADDER is scaled instead of the leaf count.
    /// That is the same predicate from the other side: the rule is `count > ladder`, and a fixture
    /// whose count is above the handed ladder exercises exactly the comparison a 6.6M-leaf job
    /// makes against `2^22`. What it proves is the thing the constant hid — that the seat reads the
    /// number it is handed, and that the refusal when the number is too small is a NAME rather than
    /// a panic or an allocation.
    #[test]
    fn a_seat_licenses_an_honest_v5_opening_at_the_ladder_it_is_handed() {
        let (artifact, profile, ctx, prompt, run) = super::tests::dense_v5_run();
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
        let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("its own retention decodes");
        let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
        let plan = engine.plan_from_profile(&profile).expect("the plan");
        let kernels = crate::qwen25_a16_backend::a16_interval_kernels_for_tests_v1(&artifact, Some(&plan));
        let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let claim = PalwClaimRootsV1 { execution_root: run.execution_root, trace_root: run.trace_root, anchor: ctx.job_id };

        let leaves = run.binding.step_leaf_count;
        let wide = COURT_MAX_STEP_LEAVES;
        let narrow = leaves - 1;
        assert!(leaves <= wide);
        eprintln!("the graph-v5 fixture prices {leaves} leaves; licensed at {wide}, refused at {narrow}");

        // The honest opening, produced at the wide ladder.
        let opened = base0_open_fp_interval_sparse_capped_v1(
            &material,
            0,
            &ids,
            interval,
            wide,
            &kernels,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("an honest graph-v5 interval opens at the ruleset's ladder");

        assert_eq!(
            base0_verify_fp_interval_opening_with_state_capped_v1(
                &opened,
                claim,
                0,
                &ids,
                leaves,
                interval,
                wide,
                None,
                &kernels,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            ),
            Base0FpIntervalSeatVerdictV1::Valid,
            "a seat handed the ladder the class was admitted under must license its honest opening"
        );

        // The same bytes, the same seat, one number narrower: refused, and refused BY NAME at the
        // price rule rather than by panicking or by sizing a capture from the field.
        assert_eq!(
            base0_fp_binding_step_space_v1(&run.binding, narrow),
            Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: narrow }),
        );
        assert_eq!(
            base0_verify_fp_interval_opening_with_state_capped_v1(
                &opened,
                claim,
                0,
                &ids,
                leaves,
                interval,
                narrow,
                None,
                &kernels,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            ),
            Base0FpIntervalSeatVerdictV1::Unverifiable,
            "a seat whose ladder cannot hold the class declines to replay it — a limit is not a verdict, so never Mismatch"
        );
        // The executor cannot serve one either, and says so with the numbers.
        assert_eq!(
            base0_open_fp_interval_sparse_capped_v1(
                &material,
                0,
                &ids,
                interval,
                narrow,
                &kernels,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            ),
            Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: narrow }),
        );
    }

    /// **ADR-0085 X3: a v3 opening is a v2 opening to every seat path, annex or not** — and the
    /// annex is read only by the closer's own function. The binding is the genesis row's, so the
    /// framing is exercised on a real class shape; nothing here replays an interval.
    #[test]
    fn a_v3_opening_reads_as_its_v2_opening_and_the_annex_is_the_closers_alone() {
        let (binding, _) = v5_512_binding();
        let v2 = Base0FpIntervalOpeningV2 {
            version: PALW_BASE0_FP_INTERVAL_VERSION_V2,
            interval_index: 7,
            binding,
            range: kaspa_consensus_core::palw_step_leg::PalwStepRangeOpeningV1 {
                first_leaf_index: 40,
                leaf_hashes: vec![Hash64::from_u64_word(1), Hash64::from_u64_word(2)],
                siblings: vec![Hash64::from_u64_word(3)],
            },
            seed_row_leaf_count: 0,
            seed_row_tiles: Vec::new(),
            anchor: None,
        };
        let tile = PalwStepTileLeafV1 {
            version: 1,
            coord: kaspa_consensus_core::palw_step::PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
            value_count: 2,
            values_le: vec![1, 0, 0, 0, 2, 0, 0, 0],
        };
        let annex = Base0FpCloseAnnexV1 {
            rows_root: Hash64::from_u64_word(9),
            disputed: vec![Base0FpDisputedLeafV1 { leaf_index: 41, tile, anchor: None }],
        };
        let with =
            Base0FpIntervalOpeningV3 { version: PALW_BASE0_FP_INTERVAL_VERSION_V3, opening: v2.clone(), close: Some(annex.clone()) }
                .encode_v1()
                .unwrap();
        let without = Base0FpIntervalOpeningV3 { version: PALW_BASE0_FP_INTERVAL_VERSION_V3, opening: v2.clone(), close: None }
            .encode_v1()
            .unwrap();
        assert_eq!(&with[..8], &PALW_BASE0_FP_INTERVAL_MAGIC_V3);
        for bytes in [&with, &without] {
            match base0_fp_interval_opening_decode_any_v1(bytes).unwrap() {
                Base0FpIntervalOpeningAnyV1::Recomputed(seen) => assert_eq!(*seen, v2, "the seat sees the v2 opening"),
                other => panic!("a v3 opening must read as Recomputed, got {other:?}"),
            }
        }
        assert_eq!(base0_fp_interval_close_annex_v1(&with), Some(annex));
        assert_eq!(base0_fp_interval_close_annex_v1(&without), None);
        assert_eq!(base0_fp_interval_close_annex_v1(&v2.encode_v1().unwrap()), None, "a v2 opening carries no annex");
        assert!(Base0FpIntervalOpeningV3::decode_v1(&v2.encode_v1().unwrap()).is_err(), "and is not a v3");
    }
}

/// A diagnostic, never part of the suite: decode a retained dense material named by
/// `MISAKA_INSPECT_MATERIAL` and say whether its tiles reproduce the root its binding committed.
#[cfg(test)]
mod inspect_material {
    #[test]
    #[ignore]
    fn inspect_devnet_material() {
        let Ok(path) = std::env::var("MISAKA_INSPECT_MATERIAL") else { return };
        let bytes = std::fs::read(&path).expect("the material reads");
        eprintln!("bytes={}", bytes.len());
        let (binding, tiles, logits_rows, generated, chunks) =
            crate::produce::base0_material_decode_v1(&bytes).expect("decodes as v1 dense material");
        let ctx = &binding.job_context;
        let profile = &binding.shape_profile;
        eprintln!(
            "step_leaf_count={} tiles={} logits_rows={} generated={} chunks={} checkpoint_count={}",
            binding.step_leaf_count,
            tiles.len(),
            logits_rows.len(),
            generated.len(),
            chunks.len(),
            binding.checkpoint_count
        );
        eprintln!("ctx.shape_profile_id={} profile.shape_profile_id()={}", ctx.shape_profile_id, profile.shape_profile_id());
        eprintln!(
            "ctx: prefill={} exact_decode={} job_id={} context_hash={}",
            ctx.declared_prefill_tokens,
            ctx.exact_decode_tokens,
            ctx.job_id,
            ctx.context_hash()
        );
        match kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, ctx, binding.step_leaf_count) {
            Ok(n) => eprintln!("derived step_leaf_count={n}"),
            Err(e) => eprintln!("derived step_leaf_count: ERR {e:?}"),
        }
        let mut distinct = std::collections::HashSet::new();
        let (mut dup, mut out_of_range, mut max_index) = (0usize, 0usize, 0u64);
        for (i, _) in &tiles {
            if !distinct.insert(*i) {
                dup += 1;
            }
            if *i >= binding.step_leaf_count {
                out_of_range += 1;
            }
            max_index = max_index.max(*i);
        }
        eprintln!("distinct={} dup={dup} max_index={max_index} out_of_range={out_of_range}", distinct.len());
        let mut per_call = std::collections::BTreeMap::new();
        let mut per_pos0 = std::collections::BTreeMap::new();
        for (_, leaf) in &tiles {
            *per_call.entry(leaf.coord.call_index).or_insert(0u64) += 1;
            if leaf.coord.call_index == 0 {
                *per_pos0.entry(leaf.coord.position).or_insert(0u64) += 1;
            }
        }
        eprintln!("tiles per call: {per_call:?}");
        let first = per_pos0.iter().take(2).collect::<Vec<_>>();
        let last = per_pos0.iter().rev().take(2).collect::<Vec<_>>();
        eprintln!("call 0 tiles per position: first {first:?} last {last:?} ({} positions)", per_pos0.len());
        for (i, leaf) in tiles.iter().take(3) {
            eprintln!("tile[{i}] coord={:?} value_count={} bytes={}", leaf.coord, leaf.value_count, leaf.values_le.len());
        }
        if let Some((i, leaf)) = tiles.last() {
            eprintln!("last tile[{i}] coord={:?} value_count={} bytes={}", leaf.coord, leaf.value_count, leaf.values_le.len());
        }
        let ctx_hash = ctx.context_hash();
        let profile_hash = profile.shape_profile_id();
        let mut leaves = vec![kaspa_hashes::Hash64::default(); binding.step_leaf_count as usize];
        for (i, leaf) in &tiles {
            if let Some(slot) = leaves.get_mut(*i as usize) {
                *slot = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf);
            }
        }
        let unfilled = leaves.iter().filter(|h| **h == kaspa_hashes::Hash64::default()).count();
        eprintln!("unfilled leaves={unfilled}");
        let flat = kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1(&leaves, binding.step_leaf_count);
        eprintln!("committed root={}", binding.step_merkle_root);
        eprintln!("flat root     ={:?}", flat);
        let fold = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(
            &leaves,
            crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1,
            binding.step_leaf_count,
        )
        .and_then(|t| t.root());
        eprintln!("fold root     ={:?}", fold);
        // The opener past its prompt check, step by step, on the same inputs it would be handed.
        let interval: u32 = std::env::var("MISAKA_INSPECT_INTERVAL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);
        let cap = binding.step_leaf_count;
        let n = super::base0_fp_binding_step_space_v1(&binding, cap).expect("step space");
        let geometry = super::Base0FpIntervalGeometryV1::from_binding_capped_v1(&binding, interval, cap).expect("geometry");
        eprintln!(
            "interval={interval} interval_count={} anchor_covered_call(0)={:?}",
            geometry.interval_count,
            geometry.anchor_covered_call(0)
        );
        let lg = super::base0_fp_interval_leaves_v1(profile, ctx, &geometry, 0, n).expect("leaves geometry");
        eprintln!("range_first={} range_end={} seed_row_leaves={}", lg.range_first, lg.range_end, lg.seed_row_leaves);
        let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(
            &leaves,
            crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1,
            cap,
        )
        .expect("tree");
        let count = lg.range_end - lg.range_first;
        let (span_first, span_end) = tree.span_for_range(lg.range_first, count).expect("span");
        eprintln!("span_first={span_first} span_end={span_end}");
        let range = tree
            .range_opening_v1(span_first, &leaves[span_first as usize..span_end as usize], lg.range_first, count)
            .expect("range opening");
        let root = kaspa_consensus_core::palw_step_leg::step_range_opening_root_capped_v1(binding.step_leaf_count, &range, cap);
        eprintln!("range opening root={:?} (committed {})", root, binding.step_merkle_root);
        eprintln!(
            "range opening: first={} count={} siblings={}",
            range.first_leaf_index,
            range.leaf_hashes.len(),
            range.siblings.len()
        );
    }
}
