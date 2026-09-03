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
}

impl Base0FpIntervalGeometryV1 {
    /// **From CHAIN data alone** — the job's prompt length and the commitment's executed decode
    /// count, both on the accepted 0x4a payload, and the family's own checkpoint interval for the
    /// class. This is the form a seat must use: an executor that could shrink the count could
    /// predict which intervals the seat's draw would land on.
    pub fn from_chain_facts_v1(
        prompt_tokens: u32,
        decode_tokens_executed: u32,
        checkpoint_interval: u32,
    ) -> Result<Self, Base0FpIntervalError> {
        if checkpoint_interval == 0 {
            return Err(Base0FpIntervalError::CheckpointIntervalIsZero);
        }
        let decode_calls = decode_tokens_executed.saturating_sub(1);
        let interval_count = decode_calls.div_ceil(checkpoint_interval).max(1);
        Ok(Self { prompt_tokens, decode_calls, checkpoint_interval, interval_count })
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
        Self::from_chain_facts_v1(binding.job_context.declared_prefill_tokens, binding.job_context.exact_decode_tokens, committed)
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

    /// The decode call the interval's anchoring checkpoint covers, or `None` for interval 0 (which
    /// has no checkpoint before it — it starts at the prompt).
    pub fn anchor_covered_call(&self, index: u32) -> Option<u32> {
        if index == 0 || index >= self.interval_count {
            return None;
        }
        index.checked_mul(self.checkpoint_interval)
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
    Base0FpIntervalGeometryV1::from_chain_facts_v1(prompt_tokens, decode_tokens_executed, checkpoint_interval)
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
    let range_first = match geometry.anchor_covered_call(index) {
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
        Some(covered) => Some(base0_checkpoint_operands_v1(binding, chunks, covered)?),
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
    Base0FpIntervalOpeningV1 {
        version: PALW_BASE0_FP_INTERVAL_VERSION_V1,
        interval_index: index,
        binding: binding.clone(),
        range,
        seed_row_leaf_count: leaves_geometry.seed_row_leaves as u32,
        seed_row_tiles,
        anchor,
    }
    .encode_v1()
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
    let operands = match geometry.anchor_covered_call(interval) {
        None => None,
        Some(covered) => Some(base0_checkpoint_operands_v1(binding, chunks, covered)?),
    };
    let start = match (&operands, geometry.anchor_covered_call(interval)) {
        (None, _) => Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt_usize },
        (Some(anchor), Some(covered)) => {
            // The id the anchored call consumes is the one the CHECKPOINT's own call produced, and
            // the executor is the party that produced it. A seat derives the same id from the
            // committed row instead of being told it (the module doc's rule); here the two are the
            // same value, and the range opening's root check below is what says so.
            let seed_token = *generated
                .get(covered as usize)
                .ok_or_else(|| Base0FpIntervalError::Replay(format!("the retention has no id for call {covered}")))?;
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
        Some(covered) => Some(base0_checkpoint_operands_v1(binding, &material.checkpoint_chunks, covered)?),
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
    let Some(anchor_call) = geometry.anchor_covered_call(index) else {
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
    covered: u32,
) -> Result<PalwCheckpointKvOperandsV1, Base0FpIntervalError> {
    let checkpoints = crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
        &binding.job_context,
        &binding.shape_profile,
        &binding.checkpoint_profile,
        chunks,
    )
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
        chunks: checkpoints.chunks.get(at).cloned().ok_or(Base0FpIntervalError::NoCheckpointAt { covered })?,
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
            if first_call != covered_decode_call + 1 {
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
    // Bytes that are not this family's are bytes this seat cannot check — never an accusation.
    let Ok(opening) = Base0FpIntervalOpeningV1::decode_v1(opening_bytes) else {
        return PalwFpIntervalVerdictV1::Unverifiable;
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
        return PalwFpIntervalVerdictV1::Mismatch;
    }
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;
    if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids) != ctx.prompt_token_ids_hash {
        return PalwFpIntervalVerdictV1::Mismatch;
    }
    let Ok(geometry) = Base0FpIntervalGeometryV1::from_binding_v1(binding, family_checkpoint_interval) else {
        return PalwFpIntervalVerdictV1::Mismatch;
    };
    let Ok(leaves_geometry) = base0_fp_interval_leaves_v1(profile, ctx, &geometry, index) else {
        return PalwFpIntervalVerdictV1::Mismatch;
    };
    let (Some((first_call, last_call)), count) = (geometry.calls_for(index), leaves_geometry.range_end - leaves_geometry.range_first)
    else {
        return PalwFpIntervalVerdictV1::Mismatch;
    };

    // The range must be the one this interval canonically has — an opening of some other window
    // would be an executor choosing which of its rows a seat sees.
    if opening.range.first_leaf_index != leaves_geometry.range_first
        || opening.range.leaf_hashes.len() as u64 != count
        || opening.seed_row_leaf_count as u64 != leaves_geometry.seed_row_leaves
        || opening.seed_row_tiles.len() as u64 != leaves_geometry.seed_row_leaves
    {
        return PalwFpIntervalVerdictV1::Mismatch;
    }
    // …and it must open against the step leg the binding committed.
    match step_range_opening_root_v1(binding.step_leaf_count, &opening.range) {
        Ok(root) if root == binding.step_merkle_root => {}
        _ => return PalwFpIntervalVerdictV1::Mismatch,
    }

    // The id the interval CONSUMED, derived from the anchor call's committed logits row.
    // The widened prompt is bound before the match rather than inside an arm: a temporary built in
    // a match arm and borrowed out of it lives on the edition's temporary-scope rules, and the
    // borrow this holds outlives the statement.
    let prompt_usize: Vec<usize> = prompt_token_ids.iter().map(|t| *t as usize).collect();
    let start = match geometry.anchor_covered_call(index) {
        None => Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt_usize },
        Some(covered) => {
            let Some(seed_token) = seed_token_from_opened_row_v1(profile, ctx, &opening, covered, leaves_geometry.range_first) else {
                return PalwFpIntervalVerdictV1::Mismatch;
            };
            let Some(anchor) = opening.anchor.as_ref() else {
                return PalwFpIntervalVerdictV1::Mismatch;
            };
            if !checkpoint_anchor_is_the_bindings_v1(binding, anchor, covered) {
                return PalwFpIntervalVerdictV1::Mismatch;
            }
            Base0FpIntervalStartV1::Checkpoint { covered_decode_call: covered, chunks: &anchor.chunks, seed_token }
        }
    };

    let Ok(recomputed) = kernels.replay_interval(profile, ctx, &start, first_call, last_call) else {
        // The seat could not run the interval — its honest `Unverifiable`, and never an
        // accusation against a producer whose material may be perfectly good.
        return PalwFpIntervalVerdictV1::Unverifiable;
    };

    // **Exact equality, leaf by leaf.** The class is a pinned integer computation, so "close" is
    // not a verdict (ADR-0026's refused proof model). The first disagreement is the court's
    // question and is returned as one; it convicts nobody.
    let Some(committed) = opening.range.leaf_hashes.get(leaves_geometry.seed_row_leaves as usize..) else {
        return PalwFpIntervalVerdictV1::Mismatch;
    };
    let mut seen = 0u64;
    for (index, hash) in &recomputed {
        let Some(offset) = index.checked_sub(leaves_geometry.interval_first) else {
            // A replay leaf outside the interval means this seat's window and the committed range
            // disagree about the enumeration — not something to accuse anyone of.
            return PalwFpIntervalVerdictV1::Unverifiable;
        };
        let Some(want) = committed.get(offset as usize) else {
            return PalwFpIntervalVerdictV1::Unverifiable;
        };
        if want != hash {
            return PalwFpIntervalVerdictV1::Fault { leaf_index: *index };
        }
        seen += 1;
    }
    if seen != committed.len() as u64 {
        // The replay did not reach every committed leaf of the interval; a partial check is not a
        // verdict, so it is reported as one it cannot make.
        return PalwFpIntervalVerdictV1::Unverifiable;
    }
    PalwFpIntervalVerdictV1::Valid
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
    opening: &Base0FpIntervalOpeningV1,
    anchor_call: u32,
    range_first: u64,
) -> Option<u32> {
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    let slot = logits_node_slot_v1(profile);
    let mut row: Vec<i32> = Vec::new();
    for (tile_index, leaf) in opening.seed_row_tiles.iter().enumerate() {
        let want_index = range_first + tile_index as u64;
        if step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf) != *opening.range.leaf_hashes.get(tile_index)? {
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
    use kaspa_consensus_core::palw_step_leg::{
        PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES, PALW_STEP_LEG_MAX_STATE_CHUNKS, checkpoint_leaf_hash_v2, state_chunk_leaf_hash_v1,
        state_chunks_root_v1, step_opening_root_v1,
    };
    if anchor.leaf.covered_decode_call != covered || anchor.leaf.state_chunk_count as usize != anchor.chunks.len() {
        return false;
    }
    // **The leg's own caps, applied before the chunks are hashed and long before they are
    // restored into a cache.** They arrived inside a stranger's opening; `state_chunks_root_v1`
    // enforces the count and the engine's restore enforces the geometry, but both are downstream
    // of the work, and a seat should not do a gigabyte of hashing to learn that an opening was
    // never admissible.
    if anchor.chunks.len() > PALW_STEP_LEG_MAX_STATE_CHUNKS
        || anchor.chunks.iter().any(|c| c.len() > PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES)
    {
        return false;
    }
    let chunk_hashes: Vec<Hash64> = anchor
        .chunks
        .iter()
        .enumerate()
        .map(|(i, bytes)| state_chunk_leaf_hash_v1(&binding.state_chunk_map_id, i as u32, bytes))
        .collect();
    let Ok(state_root) = state_chunks_root_v1(&chunk_hashes) else {
        return false;
    };
    if state_root != anchor.leaf.state_chunks_root {
        return false;
    }
    let ctx_hash = binding.job_context.context_hash();
    let leaf_hash =
        checkpoint_leaf_hash_v2(&ctx_hash, &binding.checkpoint_profile.profile_hash(), &binding.state_chunk_map_id, &anchor.leaf);
    if anchor.opening.leaf_hash != leaf_hash {
        return false;
    }
    matches!(
        step_opening_root_v1(binding.checkpoint_count as u64, &anchor.opening),
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
                let geometry = Base0FpIntervalGeometryV1::from_chain_facts_v1(4, decode_tokens, interval).expect("a geometry");
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
            Base0FpIntervalGeometryV1::from_chain_facts_v1(4, 4, 0).unwrap_err(),
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
                    let positions = kaspa_consensus_core::palw_state_chunk_map::integer_kv_positions_at_v1(ctx, *covered_decode_call);
                    let geometry = crate::legs::base0_state_chunk_geometry_v1(profile, positions).map_err(|e| format!("{e:?}"))?;
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

        let anchor = base0_checkpoint_operands_v1(&run.binding, &run.checkpoints.chunks, call - 1).expect("the committed checkpoint");
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
}
