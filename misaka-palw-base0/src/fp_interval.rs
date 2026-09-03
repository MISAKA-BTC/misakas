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
    PALW_STEP_LEG_MAX_LEAVES, PalwStepBindingV2, PalwStepRangeOpeningV1, PalwStepTileLeafV1, step_range_opening_root_v1,
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
    pub fn from_binding_v1(binding: &PalwStepBindingV2, family_interval: u32) -> Result<Self, Base0FpIntervalError> {
        let committed = binding.checkpoint_profile.checkpoint_interval;
        if committed != family_interval {
            return Err(Base0FpIntervalError::CheckpointIntervalIsNotTheCommittedOne { family: family_interval, committed });
        }
        // **The binding's price must be the price its own geometry implies.** `verify_binding_v1`
        // checks that the carried profile is the DECLARED one and that the roots recompute; it
        // does not check that `step_leaf_count` is `step_leaf_count(profile, context)`, because
        // that value is a leg input rather than a derived one. Every path below sizes a replay
        // from the profile and compares against a range priced by the field, so an opening whose
        // two numbers disagree would have a seat replay one step space and compare it to another —
        // and, on a hostile opening, allocate a 2^22-leaf capture to do it.
        let derived = kaspa_consensus_core::palw_step::step_leaf_count(&binding.shape_profile, &binding.job_context)
            .map_err(|e| Base0FpIntervalError::StepSpace(format!("{e:?}")))?;
        if derived != binding.step_leaf_count {
            return Err(Base0FpIntervalError::PriceIsNotTheGeometrys { declared: binding.step_leaf_count, derived });
        }
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
fn first_leaf_of_call_v1(profile: &PalwShapeProfileV3, ctx: &PalwJobContextV2, call: u32) -> Result<u64, Base0FpIntervalError> {
    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let aux = kv_aux_leaf_count(profile, ctx);
    if aux != 0 {
        return Err(Base0FpIntervalError::AuxLeavesAreNotIntervalScoped { got: aux });
    }
    if call > decode_calls {
        let total = kaspa_consensus_core::palw_step::step_leaf_count(profile, ctx)
            .map_err(|e| Base0FpIntervalError::StepSpace(format!("{e:?}")))?;
        return Ok(total);
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
pub fn base0_fp_interval_leaves_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    geometry: &Base0FpIntervalGeometryV1,
    index: u32,
) -> Result<Base0FpIntervalLeavesV1, Base0FpIntervalError> {
    let (first_call, last_call) =
        geometry.calls_for(index).ok_or(Base0FpIntervalError::IntervalOutOfRange { index, count: geometry.interval_count })?;
    let interval_first = first_leaf_of_call_v1(profile, ctx, first_call)?;
    let range_end = first_leaf_of_call_v1(profile, ctx, last_call + 1)?;
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

/// Either form, as a seat receives it. A seat on an old executor still reads v1; a graph-v5 class
/// refuses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Base0FpIntervalOpeningAnyV1 {
    /// ADR-0077 Decision 8: the checkpoint chunk travels. For an attention family that chunk is
    /// the history.
    WithHistory(Box<Base0FpIntervalOpeningV1>),
    /// ADR-0082 Decision 9: the checkpoint is named and the seat holds the state.
    Recomputed(Box<Base0FpIntervalOpeningV2>),
}

/// Decode whichever form arrived. The magic decides, so nothing is mis-parsed as the other.
pub fn base0_fp_interval_opening_decode_any_v1(bytes: &[u8]) -> Result<Base0FpIntervalOpeningAnyV1, Base0FpIntervalError> {
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
    let any = base0_fp_interval_opening_decode_any_v1(opening_bytes).ok()?;
    let binding = match &any {
        Base0FpIntervalOpeningAnyV1::WithHistory(o) => &o.binding,
        Base0FpIntervalOpeningAnyV1::Recomputed(o) => &o.binding,
    };
    let index = match &any {
        Base0FpIntervalOpeningAnyV1::WithHistory(o) => o.interval_index,
        Base0FpIntervalOpeningAnyV1::Recomputed(o) => o.interval_index,
    };
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, family_checkpoint_interval).ok()?;
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
) -> Result<Vec<Hash64>, Base0FpIntervalError> {
    if binding.step_leaf_count == 0 || binding.step_leaf_count > PALW_STEP_LEG_MAX_LEAVES {
        return Err(Base0FpIntervalError::LeafCountOutOfRange { got: binding.step_leaf_count, max: PALW_STEP_LEG_MAX_LEAVES });
    }
    let ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    let mut leaves = vec![Hash64::default(); binding.step_leaf_count as usize];
    for (index, leaf) in tiles {
        if let Some(slot) = leaves.get_mut(*index as usize) {
            *slot = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf);
        }
    }
    Ok(leaves)
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
) -> Result<Vec<u8>, Base0FpIntervalError> {
    let (binding, tiles, _logits_rows, _generated, chunks) = material;
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;

    // **The ids are an INPUT on this lane and are refused unless they are the job's** — the rule
    // `refutation_for_free_prompt_index` states: a wrong list reads to the court as
    // `InputSetNotCanonical`, which is no verdict at all.
    if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids) != ctx.prompt_token_ids_hash {
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
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, family_checkpoint_interval)?;
    let leaves_geometry = base0_fp_interval_leaves_v1(profile, ctx, &geometry, index)?;

    let leaves = leaves_from_tiles_v1(binding, tiles)?;
    let tree =
        crate::fp_capture::Base0SparseStepTreeV1::from_leaves_v1(&leaves, crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1)?;
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
) -> Result<Vec<u8>, Base0FpIntervalError> {
    let count = leaves_geometry.range_end - leaves_geometry.range_first;
    let range = tree.range_opening_v1(span_first, span_leaves, leaves_geometry.range_first, count)?;
    // **The opening must reproduce the committed root, checked before it is served.** On the dense
    // route the leaves came out of the capture the tree was built from and this is an identity; on
    // the folded route they came out of a REPLAY, and an executor whose replay diverged from its
    // own commitment would otherwise learn it from a seat's `Fault` against itself.
    if step_range_opening_root_v1(binding.step_leaf_count, &range).ok() != Some(binding.step_merkle_root) {
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
    let opening = Base0FpIntervalOpeningV1 {
        version: PALW_BASE0_FP_INTERVAL_VERSION_V1,
        interval_index: index,
        binding: binding.clone(),
        range,
        seed_row_leaf_count: leaves_geometry.seed_row_leaves as u32,
        seed_row_tiles,
        anchor,
    };
    if flat {
        return Base0FpIntervalOpeningV2::from_chunked_v1(&opening).encode_v1();
    }
    opening.encode_v1()
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
) -> Result<Vec<u8>, Base0FpIntervalError> {
    let bytes = base0_open_fp_interval_v1(material, index, prompt_token_ids, family_checkpoint_interval)?;
    let chunked = Base0FpIntervalOpeningV1::decode_v1(&bytes)?;
    Base0FpIntervalOpeningV2::from_chunked_v1(&chunked).encode_v1()
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
    if chunked_bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V2) {
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
fn base0_replay_span_leaves_v1<K: Base0FpIntervalKernelsV1>(
    kernels: &K,
    binding: &PalwStepBindingV2,
    chunks: &[Vec<Vec<u8>>],
    generated: &[u32],
    prompt_token_ids: &[u32],
    geometry: &Base0FpIntervalGeometryV1,
    span_first: u64,
    span_end: u64,
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
    let (operands, first_call) = match geometry.anchor_covered_call(interval).filter(|_| anchored) {
        None => (None, 0),
        Some(covered) => (Some(base0_checkpoint_operands_v1(binding, chunks, &[], covered)?), first_call),
    };
    let start = match (&operands, geometry.anchor_covered_call(interval)) {
        (None, _) => Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt_usize },
        (Some(anchor), Some(covered)) => {
            // The id the anchored call consumes is the one the CHECKPOINT's own call produced, and
            // the executor is the party that produced it. A seat derives the same id from the
            // committed row instead of being told it (the module doc's rule); here the two are the
            // same value, and the range opening's root check below is what says so.
            //
            // Indexed by the anchor's CALL, never by the leaf's counter: `generated` is one id per
            // call and a per-position leaf's counter is a position.
            let seed_call = geometry
                .anchor_seed_call_v1(interval)
                .ok_or(Base0FpIntervalError::NoCheckpointAt { covered })?;
            let seed_token = *generated
                .get(seed_call as usize)
                .ok_or_else(|| Base0FpIntervalError::Replay(format!("the retention has no id for call {seed_call}")))?;
            Base0FpIntervalStartV1::Checkpoint { covered_decode_call: covered, chunks: &anchor.chunks, seed_token }
        }
        (Some(_), None) => return Err(Base0FpIntervalError::NoCheckpointAt { covered: 0 }),
    };

    let replayed = kernels.replay_interval(profile, ctx, &start, first_call, last_needed).map_err(Base0FpIntervalError::Replay)?;
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
) -> Result<Vec<u8>, Base0FpIntervalError> {
    let binding = &material.binding;
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;

    // The ids are an INPUT on this lane and are refused unless they are the job's — the dense
    // route's rule, for the dense route's reason.
    if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids) != ctx.prompt_token_ids_hash {
        return Err(Base0FpIntervalError::PromptIdsAreNotTheJobs);
    }
    if profile.state_chunk_map_id == Hash64::default() && index > 0 {
        return Err(Base0FpIntervalError::NoStateChunkMapRegistered { index });
    }
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, family_checkpoint_interval)?;
    let leaves_geometry = base0_fp_interval_leaves_v1(profile, ctx, &geometry, index)?;

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
    base0_assemble_fp_interval_opening_v1(binding, tree, &leaves_geometry, index, span_first, &span_leaves, seed_row_tiles, anchor)
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
pub trait Base0FpIntervalKernelsV1 {
    fn replay_interval(
        &self,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        start: &Base0FpIntervalStartV1<'_>,
        first_call: u32,
        last_call: u32,
    ) -> Result<Vec<(u64, Hash64)>, String>;
}

/// **The replay loop every family shares** — the capture's own walk, restricted to a window.
///
/// `forward` is the class's engine with its cache already restored (from the checkpoint chunks, or
/// empty for interval 0); it is handed a token and an ABSOLUTE cache position and returns the
/// logits row and the rows the step space commits. Placing rows and deriving the next token stay
/// here, because a family that re-implemented the coordinate rule would commit its replay at
/// coordinates the leg does not use, and every comparison would fail for a reason that is not the
/// producer's.
pub fn base0_fp_replay_interval_v1<F>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    start: &Base0FpIntervalStartV1<'_>,
    first_call: u32,
    last_call: u32,
    mut forward: F,
) -> Result<Vec<(u64, Hash64)>, String>
where
    F: FnMut(usize, usize) -> Result<(Vec<i32>, Vec<Base0CapturedRowV1>), String>,
{
    use kaspa_consensus_core::palw_step::PalwStepTableV1;
    let prefill = ctx.declared_prefill_tokens as usize;
    let leaf_count = kaspa_consensus_core::palw_step::step_leaf_count(profile, ctx).map_err(|e| format!("{e:?}"))?;
    let mut capture = crate::legs::Base0StepCaptureV1::new(leaf_count).map_err(|e| format!("{e:?}"))?;

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
            let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(
                profile,
                ctx,
                *covered_decode_call,
            );
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
    let partial = capture.finish_partial();
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    Ok(partial.tiles.iter().map(|(i, leaf)| (*i, step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf))).collect())
}

/// **Replay one opened interval and compare every row EXACTLY** (ADR-0077 Decision 8, seat half).
///
/// The order is the security: bind the opening to the claim FIRST (execution root, trace root, the
/// FP job id as the anchor) and price it (`work_leaves` must equal the binding's
/// `step_leaf_count`), then read evidence. An implementation that replayed first would be doing
/// arithmetic for whoever asked, and a capture that answers ANOTHER claim's roots would look like
/// an honest run of a job nobody commissioned.
pub fn base0_verify_fp_interval_opening_v1<K: Base0FpIntervalKernelsV1>(
    opening_bytes: &[u8],
    claim: PalwClaimRootsV1,
    index: u32,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    family_checkpoint_interval: u32,
    kernels: &K,
) -> PalwFpIntervalVerdictV1 {
    base0_verify_fp_interval_opening_with_state_v1(
        opening_bytes,
        claim,
        index,
        prompt_token_ids,
        work_leaves,
        family_checkpoint_interval,
        None,
        kernels,
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
) -> Base0FpIntervalSeatVerdictV1 {
    // Bytes that are not this family's are bytes this seat cannot check — never an accusation.
    let Ok(any) = base0_fp_interval_opening_decode_any_v1(opening_bytes) else {
        return Base0FpIntervalSeatVerdictV1::Unverifiable;
    };
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
    };
    let opening = match &any {
        Base0FpIntervalOpeningAnyV1::WithHistory(o) => Base0FpIntervalOpeningV2::from_chunked_v1(o),
        Base0FpIntervalOpeningAnyV1::Recomputed(o) => o.as_ref().clone(),
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
    if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids) != ctx.prompt_token_ids_hash {
        return Base0FpIntervalSeatVerdictV1::Mismatch;
    }
    let Ok(geometry) = Base0FpIntervalGeometryV1::from_binding_v1(binding, family_checkpoint_interval) else {
        return Base0FpIntervalSeatVerdictV1::Mismatch;
    };
    let Ok(leaves_geometry) = base0_fp_interval_leaves_v1(profile, ctx, &geometry, index) else {
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
    match step_range_opening_root_v1(binding.step_leaf_count, &opening.range) {
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

    let Ok(recomputed) = kernels.replay_interval(profile, ctx, &start, first_call, last_call) else {
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
        fn replay_interval(
            &self,
            profile: &PalwShapeProfileV3,
            ctx: &PalwJobContextV2,
            start: &Base0FpIntervalStartV1<'_>,
            first_call: u32,
            last_call: u32,
        ) -> Result<Vec<(u64, Hash64)>, String> {
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
            base0_fp_replay_interval_v1(profile, ctx, start, first_call, last_call, |token, position| {
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
        for index in 0..count {
            let opening = base0_open_fp_interval_v1(&material, index, &ids, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
                .unwrap_or_else(|e| panic!("interval {index} opens: {e}"));
            assert_eq!(
                base0_verify_fp_interval_opening_v1(
                    &opening,
                    claim,
                    index,
                    &ids,
                    binding.step_leaf_count,
                    PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                    &FloorKernels(&artifact),
                ),
                PalwFpIntervalVerdictV1::Valid,
                "interval {index} of an honest capture replays exactly"
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
        let verify = |bytes: &[u8], index: u32| {
            base0_verify_fp_interval_opening_v1(bytes, claim, index, &ids, leaves, interval, &FloorKernels(&artifact))
        };

        // (1) The last anchored interval, opened honestly, then one byte of a COMMITTED ROW moved.
        let index = Base0FpIntervalGeometryV1::from_binding_v1(binding, interval).expect("a geometry").interval_count - 1;
        let opening = base0_open_fp_interval_v1(&material, index, &ids, interval).expect("opens");
        assert_eq!(verify(&opening, index), PalwFpIntervalVerdictV1::Valid);

        let mut decoded = Base0FpIntervalOpeningV1::decode_v1(&opening).expect("decodes");
        let last = decoded.range.leaf_hashes.len() - 1;
        let mut bytes = decoded.range.leaf_hashes[last].as_byte_slice().to_vec();
        bytes[0] ^= 1;
        decoded.range.leaf_hashes[last] = Hash64::from_bytes(bytes.try_into().expect("64 bytes"));
        // A changed row no longer opens against the committed step root — which is the seat's
        // FIRST check and the reason a forged row cannot even reach the replay.
        assert_eq!(verify(&decoded.encode_v1().expect("re-encodes"), index), PalwFpIntervalVerdictV1::Mismatch);

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
            base0_verify_fp_interval_opening_v1(&opening, stranger, index, &ids, leaves, interval, &FloorKernels(&artifact)),
            PalwFpIntervalVerdictV1::Mismatch
        );
        // …and against the wrong PRICE, which is the same accusation: a claim is priced by the
        // leaf count its binding carries (ADR-0074 Decision 5).
        assert_eq!(
            base0_verify_fp_interval_opening_v1(&opening, claim, index, &ids, leaves + 1, interval, &FloorKernels(&artifact)),
            PalwFpIntervalVerdictV1::Mismatch
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

        let opening = base0_open_fp_interval_v1(&material, index, &ids, interval).expect("a liar still opens its own capture");
        assert_eq!(
            base0_verify_fp_interval_opening_v1(&opening, claim, index, &ids, leaf_count, interval, &FloorKernels(&artifact)),
            PalwFpIntervalVerdictV1::Fault { leaf_index: target },
            "the seat returns the leaf a court is opened at, and convicts nobody"
        );
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
            let bytes = base0_open_fp_interval_v1(&material, count - 1, &ids, interval).expect("opens");
            let opened = Base0FpIntervalOpeningV1::decode_v1(&bytes).expect("decodes");
            let state: usize = opened.anchor.as_ref().map(|a| a.chunks.iter().map(Vec::len).sum()).unwrap_or(0);
            let capture = borsh::to_vec(&material).expect("the capture encodes").len();
            (bytes.len(), opened.range.leaf_hashes.len(), opened.range.siblings.len(), state, capture, material.0.step_leaf_count)
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
        let profile = floor_job(3, 4).1;
        let ceiling = profile.n_ctx as usize
            * profile.attn_kv_heads as usize
            * profile.attn_head_dim as usize
            * 2
            * (0..profile.layer_count)
                .filter(|l| profile.layer_kind(*l) == kaspa_consensus_core::palw_step::PalwLayerKindV1::Attention)
                .count();
        assert!(long_state > short_state, "the KV anchor is the term that grows: {short_state} then {long_state}");
        assert!(long_state <= ceiling, "and it is bounded by the class's own context: {long_state} ≤ {ceiling}");
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

        let anchor = base0_checkpoint_operands_v1(&run.binding, &run.checkpoints.chunks, &run.checkpoints.leaves, call - 1).expect("the committed checkpoint");
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

    use crate::fp_recompute::{Base0FpRecomputeError, Base0FpRecomputeKernelsV1, base0_fp_recompute_state_v1};

    /// The floor's kernels for a RECOMPUTE — the same engine and cache its capture path uses, with
    /// nothing captured and no token selected. The dense and hybrid tiers ship theirs in
    /// `crate::fp_recompute`; this one exists because the floor's fixture is the class in this
    /// file's tests, and it drives exactly the same shared driver.
    struct FloorRecompute<'a> {
        engine: crate::engine::Base0Engine<'a>,
        cache: crate::engine::KvCache,
        artifact: &'a crate::artifact::Base0ArtifactV1,
    }

    impl<'a> FloorRecompute<'a> {
        fn new(artifact: &'a crate::artifact::Base0ArtifactV1) -> Self {
            Self { engine: crate::engine::Base0Engine::new(artifact), cache: crate::engine::KvCache::new(artifact), artifact }
        }
    }

    impl Base0FpRecomputeKernelsV1 for FloorRecompute<'_> {
        fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), Base0FpRecomputeError> {
            self.engine
                .forward_token(&mut self.cache, token, position)
                .map(|_| ())
                .map_err(|e| Base0FpRecomputeError::Engine(format!("{e:?}")))
        }

        fn state_chunks(&self, profile: &PalwShapeProfileV3, positions: u32) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError> {
            let _ = self.artifact;
            let geometry = crate::legs::base0_state_chunk_geometry_v1(profile, positions)
                .map_err(|e| Base0FpRecomputeError::Map(format!("{e:?}")))?;
            let mut chunks = Vec::with_capacity(geometry.chunk_count() as usize);
            for index in 0..geometry.chunk_count() {
                let entry = kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_entry_v1(&geometry, index)
                    .ok_or(Base0FpRecomputeError::StateIsNotTheMaps { chunk_index: index })?;
                chunks.push(
                    self.cache.state_chunk_bytes(&entry).ok_or(Base0FpRecomputeError::StateIsNotTheMaps { chunk_index: index })?,
                );
            }
            Ok(chunks)
        }
    }

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
            let chunked = base0_open_fp_interval_v1(&material, index, &ids, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
                .expect("the chunked form opens");
            let flat = base0_open_fp_interval_chunkless_v1(&material, index, &ids, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
                .expect("the flat form opens");
            let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
            let with_history = base0_verify_fp_interval_opening_with_state_v1(
                &chunked,
                claim,
                index,
                &ids,
                binding.step_leaf_count,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                None,
                &FloorKernels(&artifact),
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
            );
            assert_eq!(with_history, Base0FpIntervalSeatVerdictV1::Valid, "interval {index}, the carried form");
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
        let opening = base0_open_fp_interval_chunkless_v1(&material, index, &ids, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
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
            let chunked = base0_open_fp_interval_v1(&material, index, &ids, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
                .expect("the chunked form opens");
            let flat = base0_open_fp_interval_chunkless_v1(&material, index, &ids, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
                .expect("the flat form opens");
            let decoded = Base0FpIntervalOpeningV2::decode_v1(&flat).expect("the flat form decodes");
            let state_bytes: usize = Base0FpIntervalOpeningV1::decode_v1(&chunked)
                .expect("decodes")
                .anchor
                .map(|a| a.chunks.iter().map(Vec::len).sum())
                .unwrap_or(0);
            assert!(decoded.anchor.is_some(), "the interval is checkpoint-anchored at both contexts");
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
        const ENCODING_SLACK: usize = 256;
        for (chunked, flat, state) in [(chunked_narrow, flat_narrow, state_narrow), (chunked_wide, flat_wide, state_wide)] {
            assert!(
                chunked - flat >= state && chunked - flat <= state + ENCODING_SLACK,
                "the flat form must drop exactly the state ({chunked} − {flat} against {state})"
            );
        }
        // …so what a seat fetches grows strictly more slowly with the context than the history
        // does. An inequality, not a number: the remaining growth is the committed ROWS and the
        // paths, which are the two terms Z5 names, and neither is the state.
        assert!(
            flat_wide.saturating_sub(flat_narrow) < chunked_wide.saturating_sub(chunked_narrow),
            "what a seat fetches must grow more slowly with the context than the history does \
             (flat {flat_narrow} → {flat_wide}, carried {chunked_narrow} → {chunked_wide})"
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
        let chunked =
            base0_open_fp_interval_v1(&material, index, &ids, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).expect("the chunked form opens");
        let mut decoded = Base0FpIntervalOpeningV1::decode_v1(&chunked).expect("decodes");
        // The class declares the tiled map — Decision 4's graph-v5 declaration.
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
    fn dense_v5_run() -> (
        crate::artifact::Base0ArtifactV1,
        PalwShapeProfileV3,
        PalwJobContextV2,
        Vec<usize>,
        crate::produce::Base0ExecutionV1,
    ) {
        use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v5};
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
        let (ctx, prompt) =
            crate::produce::base0_rc_job_v1(&profile, Hash64::from_u64_word(0x0082_C1), geometry.vocab_size as usize, 3, 4);
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
            let opened = base0_open_fp_interval_sparse_v1(&material, index, &ids, interval, &kernels)
                .unwrap_or_else(|e| panic!("interval {index} must open from a fold that retained nothing: {e}"));
            // A folded class is served FLAT: the anchor is named, never carried.
            assert!(
                opened.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V2),
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
                let per_call = Base0FpIntervalGeometryV1::from_chain_facts_v1(
                    prefill,
                    decode,
                    interval,
                    PalwCheckpointCadenceV1::PerDecodeCall,
                )
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
        for (call_index, position) in
            (0..prefill as u32).map(|p| (0u32, p)).chain((1..=decode_calls).map(|c| (c, 0u32)))
        {
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

}
