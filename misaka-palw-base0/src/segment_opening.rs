//! **ADR-0133 S1, authenticated (SEAT-S4): a segment opening is the CLAIM's before a partial seat
//! compares.**
//!
//! A partial seat replays one V2 segment from the checkpoint at its start and signs `Valid` for
//! that segment's bit of the mask. Until this module the opening it replayed (`SC01`) carried the
//! checkpoint leaf, its chunks, the covered call, the seed token, the leaf range and the committed
//! leaf hashes — and the seat compared its recomputed hashes with `committed_leaf_hashes`, so BOTH
//! sides of the comparison came from the opening. Nothing tied any field to the claim: a producer
//! whose claimed execution was a lie could serve the honest run's opening (or any self-consistent
//! one) and every partial seat licensed its segment. A partial `Valid` proved nothing about the
//! claim, and F2 makes a partial mask liable for exactly the leaves in its segments.
//!
//! # What an opening proves now (`SC02`)
//!
//! Every link is checked against the claim — the roots the SEAT read off chain and the job the SEAT
//! derived — before one step is replayed:
//!
//! 1. **The binding** rides the opening and must pass `verify_binding_v1` (which rebuilds
//!    `committed_execution_root` from every field), with `committed_execution_root` and
//!    `full_logits_trace_root` equal to the claim's, its job context equal to the seat's own job
//!    (field for field, so the identity questions are the seat's derivation's), its profile the
//!    class's, and its price the geometry's.
//! 2. **The segment** is the seat's: `(segments, segment_index)` are the seat's assignment and
//!    `[leaf_start, leaf_end)` is `palw_segment_leaf_range_v2` of the binding's leaf count.
//! 3. **The checkpoint** the replay resumes from is a committed one: its leaf opens against
//!    `checkpoint_merkle_root` at its own index, its counter is the cadence's canonical value for
//!    that index, and its chunks re-derive the leaf's `state_chunks_root` under the class's map; on
//!    a hybrid it must be a leaf that carries the recurrence (a per-position leaf between the
//!    recurrence's spacing commits the attention half only, and nothing resumes from it). It must
//!    precede the proven range. No checkpoint is genesis. Whenever the window reads the prompt, the
//!    prompt must hash to the job's `prompt_token_ids_hash`.
//! 4. **The seed** — the id the first resumed decode call consumes — is never declared. The opening
//!    carries the claim's decode pin (the flat rows and ids, or the tiled rows root and ids), which
//!    must reproduce `full_logits_trace_root`; the seat reads the id off it.
//! 5. **The leaves.** No leaf hash rides. The opening carries only the sibling path of a proven
//!    range `[first, end) ⊇ [leaf_start, leaf_end)`; the seat recomputes every leaf of that range
//!    and folds them — edges, whole-block digests, edges (`Base0RangeFoldV1`) — into the range's
//!    root under the served path, which must be `step_merkle_root`. A retention that holds every
//!    leaf proves the segment exactly (`align_level` 0). A FOLD keeps no leaf below its retained
//!    level, so it proves the segment rounded out to its `2^align_level`-leaf blocks and the seat
//!    replays up to one block past each edge (`align_level` at most the class's own fold level);
//!    every sibling of such a range is a retained-level node, served from the kept vector alone.
//!
//! A link that fails is a refusal ([`Base0SegmentRefusalV1`], never `Valid`): the opening is not
//! this claim's, which is the same as nothing served. A replay whose leaves do not root to the claim
//! is `matches == false`: the committed leaves are not what the committed state computes.
//!
//! # Sizes
//!
//! The opening is the binding (the profile rides whole), a path of at most two siblings a level,
//! and — only for a resume — the checkpoint's chunks and the decode pin. The held attempt rows
//! (A16 graph-v7 at 8,192 and 2,097,152 positions) serve genesis openings from their fold: no
//! chunks, no pin, and siblings only above level 12 — a 5,131-byte binding and at most ~6.2 KB
//! (8k) / ~6.7 KB (2M) an opening, inside the interval lane's 4 MiB by three orders. The seat's
//! fold holds one 64-byte digest per 4,096 leaves of its range: at five seats (four segments) at
//! most ~0.4 MiB at 8k and ~101 MiB at 2M (`segment_opening_sizes_at_the_t12_held_rows`). The
//! floor's resumed segment (chunks and the flat pin) is ~42 KB.

use crate::fp_capture::{
    Base0RangeFoldV1, PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1, PALW_BASE0_SPARSE_RETAIN_LEVEL_V1, base0_range_sibling_count_v1,
    palw_base0_sparse_retain_level_for_class_v1,
};
use crate::fp_interval::{Base0FpIntervalKernelsV1, Base0FpIntervalStartV1, Base0FpWindowV1, base0_fp_binding_step_space_v1};
use crate::produce::{Base0RetentionV1, ProduceError, base0_material_decode_any_v1};
use kaspa_consensus_core::palw_context_ladder::{
    palw_checkpoint_covered_at_index_v1, palw_checkpoint_leaf_carries_recurrence_v1, palw_checkpoint_positions_at_v1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1};
use kaspa_consensus_core::palw_segment_resume_v1::{PalwSegmentClaimV1, PalwSegmentReplayV1, PalwSegmentResumeWindowV1};
use kaspa_consensus_core::palw_step::{PalwLayerKindV1, PalwShapeProfileV3, canonical_step_coordinates, kv_aux_leaf_count};
use kaspa_consensus_core::palw_step_leg::{
    PalwStepBindingV2, step_merkle_range_siblings_capped_v1, step_tile_leaf_hash_v1, verify_binding_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    PalwBase0DecodeTokensV1, PalwCheckpointKvOperandsV1, PalwTiledDecodeTokensV1, base0_logits_trace_root_v1,
    flat_logits_scheme_id_v1, tiled_logits_outer_root_v1, tiled_logits_rows_root_v1, tiled_logits_scheme_id_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_verification_v2::{palw_segment_count_v2, palw_segment_leaf_range_v2};
use kaspa_hashes::Hash64;

/// Wire magic of the authenticated segment opening. `SC01` (the unauthenticated form) is gone: the
/// producer and every seat ship in one binary, and a seat must not read a form that proves nothing.
pub const PALW_SEGMENT_OPENING_MAGIC_V2: [u8; 4] = *b"SC02";
pub const PALW_SEGMENT_OPENING_VERSION_V2: u16 = 2;

/// **What a replay keeps of its recomputed leaf hashes for its caller** — diagnostics only; the
/// verdict is the fold's root. Past this many the list is dropped whole (a held segment is tens of
/// millions of leaves) rather than kept partially, so a caller never reads a prefix as the whole.
pub const PALW_SEGMENT_REPLAY_KEPT_LEAVES_V1: usize = 1 << 16;

/// **The committed range an opening proves, and its path to `step_merkle_root`.**
///
/// `[first_leaf_index, first_leaf_index + leaf_count)` is the segment's range aligned to
/// `2^align_level`-leaf blocks (the tree's end is its own boundary): `align_level` 0 is the segment
/// exactly. `siblings` is the consensus range path (`step_merkle_range_siblings_v1`'s order); the
/// leaves are the seat's own.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0SegmentProofV1 {
    pub align_level: u32,
    pub first_leaf_index: u64,
    pub leaf_count: u64,
    pub siblings: Vec<Hash64>,
}

/// **The claim's decode pin** — what `full_logits_trace_root` is recomputed from, in the class's
/// scheme. The seed a resumed replay consumes is read off it, never declared.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum Base0SegmentSeedPinV1 {
    /// The flat scheme: every logits row and every id (`base0_logits_trace_root_v1`).
    Flat(PalwBase0DecodeTokensV1) = 0,
    /// The tiled scheme: the rows tree's root and every id (`tiled_logits_outer_root_v1`).
    Tiled(PalwTiledDecodeTokensV1) = 1,
}

/// **One V2 segment, opened for a partial seat** (`SC02`). See the module doc for what each field
/// is checked against.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0SegmentOpeningV2 {
    pub version: u16,
    pub segments: u16,
    pub segment_index: u16,
    pub leaf_start: u64,
    pub leaf_end: u64,
    /// The claim's binding.
    pub binding: PalwStepBindingV2,
    pub proof: Base0SegmentProofV1,
    /// The committed checkpoint the replay resumes from, with its opening and its chunks. `None`
    /// is genesis: the replay starts at the prompt.
    pub anchor: Option<PalwCheckpointKvOperandsV1>,
    /// Present exactly when the anchor resumes at a decode call.
    pub seed_pin: Option<Base0SegmentSeedPinV1>,
}

impl Base0SegmentOpeningV2 {
    pub fn encode_v2(&self) -> Result<Vec<u8>, ProduceError> {
        let body = borsh::to_vec(self).map_err(|_| ProduceError::Internal("a segment opening does not serialize"))?;
        let mut out = Vec::with_capacity(body.len() + PALW_SEGMENT_OPENING_MAGIC_V2.len());
        out.extend_from_slice(&PALW_SEGMENT_OPENING_MAGIC_V2);
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn decode_v2(bytes: &[u8]) -> Result<Self, Base0SegmentRefusalV1> {
        let body = bytes.strip_prefix(&PALW_SEGMENT_OPENING_MAGIC_V2).ok_or(Base0SegmentRefusalV1::NotAnOpening)?;
        let decoded: Self = borsh::from_slice(body).map_err(|_| Base0SegmentRefusalV1::NotAnOpening)?;
        if decoded.version != PALW_SEGMENT_OPENING_VERSION_V2 {
            return Err(Base0SegmentRefusalV1::NotAnOpening);
        }
        Ok(decoded)
    }
}

/// **Why a seat refuses a segment opening** — one variant a link, so a test (and an operator's log)
/// names which link failed. None of them is a verdict about the producer's arithmetic; each says the
/// served bytes are not this claim's segment, which is the same as nothing served.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Base0SegmentRefusalV1 {
    NotAnOpening,
    BindingDoesNotVerify,
    NotTheClaimsExecution,
    NotTheClaimsTrace,
    NotTheSeatsJob,
    NotThisClass,
    PriceIsNotTheGeometrys,
    NotTheSeatsSegment,
    NotTheSegmentsRange,
    ProofNotTheSegments,
    ProofPathNotTheRanges,
    NotMainStepLeaves,
    AnchorNotCommitted,
    AnchorNotCanonical,
    AnchorCarriesNoState,
    AnchorNotAResumePoint,
    AnchorPastTheRange,
    SeedPinMissing,
    SeedPinNotCommitted,
    SeedPinUnexpected,
    PromptNotTheJobs,
    Replay(String),
}

impl std::fmt::Display for Base0SegmentRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::NotAnOpening => "the bytes are not an SC02 segment opening",
            Self::BindingDoesNotVerify => "the opening's binding does not rebuild its own execution root",
            Self::NotTheClaimsExecution => "the opening's binding commits another execution root than the claim's",
            Self::NotTheClaimsTrace => "the opening's binding commits another trace root than the claim's",
            Self::NotTheSeatsJob => "the opening's binding answers another job than the one this seat derived",
            Self::NotThisClass => "the opening's binding carries another profile than the class's",
            Self::PriceIsNotTheGeometrys => "the opening's binding prices a step space its geometry does not produce",
            Self::NotTheSeatsSegment => "the opening is another cut's or another segment's than this seat's",
            Self::NotTheSegmentsRange => "the opening's leaf range is not the segment's canonical range",
            Self::ProofNotTheSegments => "the proven range is not the segment, nor its span at a level the class folds at",
            Self::ProofPathNotTheRanges => "the proof's path is not the proven range's",
            Self::NotMainStepLeaves => "the proven range reaches a leaf no replay produces",
            Self::AnchorNotCommitted => "the anchor does not open against the claim's checkpoint leg, or its chunks are not its state",
            Self::AnchorNotCanonical => "the anchor's counter is not the cadence's for its index",
            Self::AnchorCarriesNoState => "the anchor names a checkpoint and carries none of its state",
            Self::AnchorNotAResumePoint => {
                "the anchor's checkpoint does not commit the recurrence the class carries, so no replay resumes from it"
            }
            Self::AnchorPastTheRange => "the anchor resumes after the first leaf the opening proves",
            Self::SeedPinMissing => "a resume at a decode call needs the claim's decode pin",
            Self::SeedPinNotCommitted => "the decode pin does not reproduce the claim's trace root",
            Self::SeedPinUnexpected => "a decode pin rides an opening that resumes inside the prefill",
            Self::PromptNotTheJobs => "the prompt is not the one the job commits to",
            Self::Replay(why) => return write!(f, "the segment could not be replayed: {why}"),
        };
        f.write_str(s)
    }
}

impl std::error::Error for Base0SegmentRefusalV1 {}

impl From<Base0SegmentRefusalV1> for String {
    fn from(refusal: Base0SegmentRefusalV1) -> Self {
        refusal.to_string()
    }
}

/// The step a leaf belongs to — `(call, position)` in steps (`Base0FpWindowV1`'s numbering).
fn step_of_leaf_v1(profile: &PalwShapeProfileV3, ctx: &PalwJobContextV2, leaf: u64) -> Option<u64> {
    let coord = canonical_step_coordinates(profile, ctx, leaf)?;
    Some(Base0FpWindowV1::step_of_coordinate_v1(ctx.declared_prefill_tokens, coord.call_index, coord.position))
}

/// `[first, end)` rounded out to `2^level`-leaf blocks, the tree's end being its own boundary.
fn aligned_span_v1(first: u64, end: u64, leaf_count: u64, level: u32) -> Option<(u64, u64)> {
    let block = 1u64.checked_shl(level)?;
    let span_first = first - first % block;
    let span_end = end.div_ceil(block).checked_mul(block)?.min(leaf_count);
    Some((span_first, span_end))
}

// ---------------------------------------------------------------------------------------------
// The producer's side
// ---------------------------------------------------------------------------------------------

/// Where the opening's replay resumes.
#[derive(Clone, Debug)]
pub enum Base0SegmentAnchorV1 {
    /// From the prompt.
    Genesis,
    /// From the latest committed checkpoint whose state this retention carries and which precedes
    /// the proven range — genesis when there is none (a held class's retention keeps no state).
    Committed,
    /// From this checkpoint, whose state the caller holds (a server that keeps the cache re-derives
    /// a held class's chunks; `crate::fp_interval::base0_checkpoint_operands_v1` names the leaf).
    Given(PalwCheckpointKvOperandsV1),
}

/// **S1 (1), authenticated: the producer's opening of segment `segment_index` of a `seat_count`
/// panel, from its retained capture** — what `open_segment_checkpoint_v1` serves.
///
/// `class_profile` is the serving backend's class: a capture of another profile is refused before
/// anything is derived from it (a node also opens captures it pooled from the network, and a
/// stranger's profile is not one to enumerate a step space under). `None` trusts the capture's own
/// (fixtures).
pub fn base0_open_segment_checkpoint_capped_v2(
    capture: &[u8],
    seat_count: u16,
    segment_index: u16,
    class_profile: Option<&PalwShapeProfileV3>,
    max_step_leaf_count: u64,
) -> Result<Vec<u8>, ProduceError> {
    let retention = base0_material_decode_any_v1(capture)?;
    base0_segment_opening_v2(
        &retention,
        seat_count,
        segment_index,
        Base0SegmentAnchorV1::Committed,
        class_profile,
        max_step_leaf_count,
    )?
    .encode_v2()
}

/// The opening, built. A dense retention proves the segment exactly (it holds every leaf); a fold
/// proves the segment's span at its retained level from its kept vector (it holds no leaf below it).
pub fn base0_segment_opening_v2(
    retention: &Base0RetentionV1,
    seat_count: u16,
    segment_index: u16,
    anchor: Base0SegmentAnchorV1,
    class_profile: Option<&PalwShapeProfileV3>,
    max_step_leaf_count: u64,
) -> Result<Base0SegmentOpeningV2, ProduceError> {
    let binding = retention.binding();
    if class_profile.is_some_and(|class| *class != binding.shape_profile) {
        return Err(ProduceError::Internal("the capture is not this class's"));
    }
    let (profile, ctx) = (&binding.shape_profile, &binding.job_context);
    let leaf_count = binding.step_leaf_count;
    if leaf_count == 0 || leaf_count > max_step_leaf_count {
        return Err(ProduceError::Internal("the capture's step space is outside the ladder"));
    }
    let segments = palw_segment_count_v2(seat_count);
    let (leaf_start, leaf_end) = palw_segment_leaf_range_v2(leaf_count, segments, segment_index)
        .filter(|(start, end)| end > start)
        .ok_or(ProduceError::Internal("this claim has no such non-empty V2 segment"))?;
    let proof = match retention {
        Base0RetentionV1::Dense((_, tiles, ..)) => {
            // One tile a leaf — so the leaf vector below is sized by bytes that were served, never
            // by a count a stranger's binding states.
            if tiles.len() as u64 != leaf_count {
                return Err(ProduceError::Internal("the dense retention does not carry one tile a leaf"));
            }
            let leaves = crate::produce::base0_dense_step_leaves_capped_v1(binding, tiles, max_step_leaf_count)
                .ok_or(ProduceError::Internal("the dense retention's tiles are not its step space"))?;
            let siblings = step_merkle_range_siblings_capped_v1(
                &leaves,
                leaf_start as usize,
                (leaf_end - leaf_start) as usize,
                max_step_leaf_count,
            )
            .map_err(|_| ProduceError::Internal("the segment's range has no path in the dense tree"))?;
            Base0SegmentProofV1 { align_level: 0, first_leaf_index: leaf_start, leaf_count: leaf_end - leaf_start, siblings }
        }
        Base0RetentionV1::Folded(material) => {
            let tree = &material.step_tree;
            if tree.leaf_count() != leaf_count {
                return Err(ProduceError::Internal("the retained tree is not the binding's step space"));
            }
            let (first, end) = tree
                .span_for_range(leaf_start, leaf_end - leaf_start)
                .map_err(|_| ProduceError::Internal("the segment is not inside the retained tree"))?;
            let siblings = tree
                .aligned_range_siblings_v1(first, end - first)
                .map_err(|_| ProduceError::Internal("the retained tree cannot open the segment's span"))?;
            Base0SegmentProofV1 { align_level: tree.retain_level(), first_leaf_index: first, leaf_count: end - first, siblings }
        }
    };
    let proven_end = proof.first_leaf_index + proof.leaf_count;
    let main =
        leaf_count.checked_sub(kv_aux_leaf_count(profile, ctx)).ok_or(ProduceError::Internal("the aux series exceeds the space"))?;
    if proven_end > main {
        return Err(ProduceError::Internal("the segment reaches the KV aux series, which no replay produces"));
    }
    let first_step = step_of_leaf_v1(profile, ctx, proof.first_leaf_index)
        .ok_or(ProduceError::Internal("the proven range does not start at a main step coordinate"))?;
    let anchor = match anchor {
        Base0SegmentAnchorV1::Genesis => None,
        Base0SegmentAnchorV1::Given(anchor) => Some(anchor),
        Base0SegmentAnchorV1::Committed => base0_committed_anchor_before_v1(retention, first_step)?,
    };
    let resume_step = match &anchor {
        None => 1,
        Some(a) => u64::from(palw_checkpoint_positions_at_v1(profile, ctx, a.leaf.covered_decode_call)) + 1,
    };
    if resume_step > first_step {
        return Err(ProduceError::Internal("the anchor resumes after the first leaf the opening proves"));
    }
    let seed_pin =
        if resume_step > u64::from(ctx.declared_prefill_tokens) { Some(base0_segment_seed_pin_v1(retention)?) } else { None };
    Ok(Base0SegmentOpeningV2 {
        version: PALW_SEGMENT_OPENING_VERSION_V2,
        segments,
        segment_index,
        leaf_start,
        leaf_end,
        binding: binding.clone(),
        proof,
        anchor,
        seed_pin,
    })
}

/// The latest committed checkpoint whose state `retention` carries and whose state precedes step
/// `first_step` — chosen from the leg the chunks re-derive, which must be the binding's.
fn base0_committed_anchor_before_v1(
    retention: &Base0RetentionV1,
    first_step: u64,
) -> Result<Option<PalwCheckpointKvOperandsV1>, ProduceError> {
    let chunks = retention.checkpoint_chunks();
    if chunks.is_empty() {
        return Ok(None);
    }
    let binding = retention.binding();
    let leaves: &[kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2] = match retention {
        Base0RetentionV1::Folded(material) => &material.checkpoint_leaves,
        Base0RetentionV1::Dense(_) => &[],
    };
    let leg = crate::legs::base0_checkpoint_leg_of_retention_v1(binding, chunks, leaves).map_err(ProduceError::Leg)?;
    let best = leg
        .leaves
        .iter()
        .filter(|leaf| {
            u64::from(palw_checkpoint_positions_at_v1(&binding.shape_profile, &binding.job_context, leaf.covered_decode_call))
                < first_step
        })
        .max_by_key(|leaf| leaf.covered_decode_call);
    let Some(best) = best else {
        return Ok(None);
    };
    let operands = crate::fp_interval::base0_checkpoint_operands_v1(binding, chunks, leaves, best.covered_decode_call)
        .map_err(|_| ProduceError::Internal("the retained checkpoint leg is not the binding's"))?;
    Ok((!operands.chunks.is_empty()).then_some(operands))
}

/// The claim's decode pin in its class's scheme, from the retained rows and ids.
fn base0_segment_seed_pin_v1(retention: &Base0RetentionV1) -> Result<Base0SegmentSeedPinV1, ProduceError> {
    let binding = retention.binding();
    let scheme = binding.shape_profile.logits_scheme_id;
    let (rows, ids) = (retention.logits_rows(), retention.generated_token_ids());
    if scheme == flat_logits_scheme_id_v1() {
        Ok(Base0SegmentSeedPinV1::Flat(PalwBase0DecodeTokensV1 { logits_rows: rows.to_vec(), generated_token_ids: ids.to_vec() }))
    } else if scheme == tiled_logits_scheme_id_v1() {
        let rows_root = tiled_logits_rows_root_v1(&binding.job_context, rows)
            .ok_or(ProduceError::Internal("the retained rows do not build the tiled trace"))?;
        Ok(Base0SegmentSeedPinV1::Tiled(PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: ids.to_vec() }))
    } else {
        Err(ProduceError::Internal("this class commits its trace under a scheme no decode pin names"))
    }
}

// ---------------------------------------------------------------------------------------------
// The seat's side
// ---------------------------------------------------------------------------------------------

/// **What an authenticated opening lets a seat replay** — derived by [`base0_segment_opening_plan_v2`]
/// from the claim, the seat's job and the opening, before any step runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base0SegmentPlanV1 {
    pub leaf_count: u64,
    /// The segment `[start, end)` the seat attests.
    pub segment: (u64, u64),
    /// The range `[first, end)` the replay must root, `⊇ segment`.
    pub proven: (u64, u64),
    /// The level the seat folds the proven range at.
    pub fold_level: u32,
    /// The steps replayed.
    pub window: Base0FpWindowV1,
    /// The id the first resumed decode call consumes, read off the claim's decode pin.
    pub seed: Option<u32>,
}

/// **Every link of the opening, checked against the claim — no step replayed.** `Err` names the
/// first link that fails.
pub fn base0_segment_opening_plan_v2(
    opening: &Base0SegmentOpeningV2,
    class_profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    claim: PalwSegmentClaimV1,
    max_step_leaf_count: u64,
) -> Result<Base0SegmentPlanV1, Base0SegmentRefusalV1> {
    use Base0SegmentRefusalV1 as R;
    let binding = &opening.binding;
    // 1. The binding is the claim's, for the seat's job, under the class.
    verify_binding_v1(binding).map_err(|_| R::BindingDoesNotVerify)?;
    if binding.committed_execution_root != claim.execution_root {
        return Err(R::NotTheClaimsExecution);
    }
    if binding.full_logits_trace_root != claim.trace_root {
        return Err(R::NotTheClaimsTrace);
    }
    if binding.job_context != *job {
        return Err(R::NotTheSeatsJob);
    }
    if binding.shape_profile != *class_profile {
        return Err(R::NotThisClass);
    }
    let leaf_count = base0_fp_binding_step_space_v1(binding, max_step_leaf_count).map_err(|_| R::PriceIsNotTheGeometrys)?;
    // 2. The segment is the seat's own, cut from the claim's step space.
    let segments = palw_segment_count_v2(claim.seat_count);
    if opening.segments != segments || opening.segment_index != claim.segment_index {
        return Err(R::NotTheSeatsSegment);
    }
    let (start, end) = palw_segment_leaf_range_v2(leaf_count, segments, claim.segment_index).ok_or(R::NotTheSeatsSegment)?;
    if (opening.leaf_start, opening.leaf_end) != (start, end) || end <= start {
        return Err(R::NotTheSegmentsRange);
    }
    // 5 (the shape half). The proven range is the segment, or its span at a level no deeper than
    // the class's own fold, and its path is that range's.
    let proof = &opening.proof;
    let class_level = palw_base0_sparse_retain_level_for_class_v1(class_profile, max_step_leaf_count);
    if proof.align_level > class_level {
        return Err(R::ProofNotTheSegments);
    }
    let (first, proven_end) = aligned_span_v1(start, end, leaf_count, proof.align_level).ok_or(R::ProofNotTheSegments)?;
    if (proof.first_leaf_index, proof.first_leaf_index.checked_add(proof.leaf_count)) != (first, Some(proven_end)) {
        return Err(R::ProofNotTheSegments);
    }
    if base0_range_sibling_count_v1(leaf_count, first, proven_end - first) != Some(proof.siblings.len()) {
        return Err(R::ProofPathNotTheRanges);
    }
    let main = leaf_count.checked_sub(kv_aux_leaf_count(class_profile, job)).ok_or(R::NotMainStepLeaves)?;
    if proven_end > main {
        return Err(R::NotMainStepLeaves);
    }
    let first_step = step_of_leaf_v1(class_profile, job, first).ok_or(R::NotMainStepLeaves)?;
    let last_step = step_of_leaf_v1(class_profile, job, proven_end - 1).ok_or(R::NotMainStepLeaves)?;
    // 3. The anchor is a committed checkpoint, with its own state, before the proven range.
    let resume_step = match &opening.anchor {
        None => 1,
        Some(anchor) => {
            if anchor.chunks.is_empty() {
                return Err(R::AnchorCarriesNoState);
            }
            if anchor.opening.leaf_index != u64::from(anchor.leaf.checkpoint_index)
                || palw_checkpoint_covered_at_index_v1(
                    class_profile,
                    anchor.leaf.checkpoint_index,
                    binding.checkpoint_profile.checkpoint_interval,
                ) != Some(anchor.leaf.covered_decode_call)
            {
                return Err(R::AnchorNotCanonical);
            }
            if !crate::fp_interval::checkpoint_anchor_is_the_bindings_v1(binding, anchor, anchor.leaf.covered_decode_call) {
                return Err(R::AnchorNotCommitted);
            }
            let positions = palw_checkpoint_positions_at_v1(class_profile, job, anchor.leaf.covered_decode_call);
            // A hybrid's per-position leaf commits the attention half at every position and the
            // recurrence only at its derived spacing: a leaf without it names a state the class's
            // own continuation cannot be computed from (the first `SsmConv` after it diverges), so
            // it is a committed checkpoint and not a resume point.
            let recurrent = (0..class_profile.layer_count).any(|l| class_profile.layer_kind(l) == PalwLayerKindV1::GatedDeltaNet);
            if recurrent && !palw_checkpoint_leaf_carries_recurrence_v1(class_profile, positions) {
                return Err(R::AnchorNotAResumePoint);
            }
            u64::from(positions) + 1
        }
    };
    if resume_step > first_step {
        return Err(R::AnchorPastTheRange);
    }
    // 4. The seed, read off the claim's own decode pin.
    let prefill = u64::from(job.declared_prefill_tokens);
    let seed = if resume_step > prefill {
        let pin = opening.seed_pin.as_ref().ok_or(R::SeedPinMissing)?;
        let ids = base0_segment_seed_pin_ids_v1(binding, pin).ok_or(R::SeedPinNotCommitted)?;
        let seed = *ids.get((resume_step - prefill - 1) as usize).ok_or(R::SeedPinNotCommitted)?;
        if u64::from(seed) >= u64::from(class_profile.vocab_size) {
            return Err(R::SeedPinNotCommitted);
        }
        Some(seed)
    } else {
        if opening.seed_pin.is_some() {
            return Err(R::SeedPinUnexpected);
        }
        None
    };
    Ok(Base0SegmentPlanV1 {
        leaf_count,
        segment: (start, end),
        proven: (first, proven_end),
        fold_level: proof.align_level.clamp(PALW_BASE0_SPARSE_RETAIN_LEVEL_V1, PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1),
        window: Base0FpWindowV1 { first_step: resume_step, last_step },
        seed,
    })
}

/// The ids a decode pin commits, when it reproduces the binding's trace root under the class's
/// scheme — the checks `palw_step_refute`'s decode-pin arms make, over the same public roots.
fn base0_segment_seed_pin_ids_v1<'a>(binding: &PalwStepBindingV2, pin: &'a Base0SegmentSeedPinV1) -> Option<&'a [u32]> {
    let decode = binding.job_context.exact_decode_tokens as usize;
    let scheme = binding.shape_profile.logits_scheme_id;
    match pin {
        Base0SegmentSeedPinV1::Flat(pin) => {
            let vocab = binding.shape_profile.vocab_size as usize;
            (scheme == flat_logits_scheme_id_v1()
                && pin.logits_rows.len() == decode
                && pin.generated_token_ids.len() == decode
                && pin.logits_rows.iter().all(|row| row.len() == vocab)
                && base0_logits_trace_root_v1(&binding.job_context, &pin.logits_rows, &pin.generated_token_ids)
                    == binding.full_logits_trace_root)
                .then_some(pin.generated_token_ids.as_slice())
        }
        Base0SegmentSeedPinV1::Tiled(pin) => (scheme == tiled_logits_scheme_id_v1()
            && pin.generated_token_ids.len() == decode
            && tiled_logits_outer_root_v1(&binding.job_context, decode as u64, &pin.rows_root, &pin.generated_token_ids)
                == binding.full_logits_trace_root)
            .then_some(pin.generated_token_ids.as_slice()),
    }
}

/// **S1 (2)(3), authenticated: replay one segment from its opening and root it to the claim.**
///
/// The opening is authenticated first ([`base0_segment_opening_plan_v2`]); a refusal is `Err` and
/// nothing is replayed. Then the class's own kernels replay the window, the proven range's leaves
/// are folded as they stream past, and `matches` is whether they root, under the served path, to
/// the claim's `step_merkle_root`. The prompt is checked against the job only when the window
/// reads it (a resume past the prefill reads none).
#[allow(clippy::too_many_arguments)]
pub fn base0_replay_segment_opening_v2<K: Base0FpIntervalKernelsV1 + ?Sized>(
    kernels: &K,
    class_profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    prompt: &[usize],
    opening_bytes: &[u8],
    claim: PalwSegmentClaimV1,
    max_step_leaf_count: u64,
    prompt_ids_form: PalwPromptIdsFormV1,
) -> Result<PalwSegmentReplayV1, Base0SegmentRefusalV1> {
    let opening = Base0SegmentOpeningV2::decode_v2(opening_bytes)?;
    let plan = base0_segment_opening_plan_v2(&opening, class_profile, job, claim, max_step_leaf_count)?;
    if plan.window.first_step <= u64::from(job.declared_prefill_tokens) {
        let ids: Option<Vec<u32>> = prompt.iter().map(|t| u32::try_from(*t).ok()).collect();
        if !ids.is_some_and(|ids| prompt_token_ids_match_v1(prompt_ids_form, &ids, &job.prompt_token_ids_hash)) {
            return Err(Base0SegmentRefusalV1::PromptNotTheJobs);
        }
    }
    let start = match &opening.anchor {
        None => Base0FpIntervalStartV1::Genesis { prompt_tokens: prompt },
        Some(anchor) => Base0FpIntervalStartV1::Checkpoint {
            covered_decode_call: anchor.leaf.covered_decode_call,
            chunks: &anchor.chunks,
            seed_token: plan.seed.unwrap_or(0),
            prompt_tokens: prompt,
        },
    };
    let (first, end) = plan.proven;
    let mut fold =
        Base0RangeFoldV1::new(plan.leaf_count, first, end - first, plan.fold_level).map_err(Base0SegmentRefusalV1::Replay)?;
    let ctx_hash = job.context_hash();
    let profile_hash = class_profile.shape_profile_id();
    let mut kept: Option<Vec<(u64, Hash64)>> = Some(Vec::new());
    kernels
        .replay_interval_into(class_profile, job, &start, plan.window, plan.leaf_count, &mut |index, tile| {
            let proven = index >= first && index < end;
            if !proven && kept.is_none() {
                return Ok(());
            }
            let hash = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &tile);
            if let Some(list) = kept.as_mut() {
                if list.len() < PALW_SEGMENT_REPLAY_KEPT_LEAVES_V1 {
                    list.push((index, hash));
                } else {
                    kept = None;
                }
            }
            if proven {
                fold.push(index, hash)?;
            }
            Ok(())
        })
        .map_err(Base0SegmentRefusalV1::Replay)?;
    let root = fold.root_v1(&opening.proof.siblings, max_step_leaf_count).map_err(Base0SegmentRefusalV1::Replay)?;
    let (start_leaf, end_leaf) = plan.segment;
    let call_of = |leaf: u64| canonical_step_coordinates(class_profile, job, leaf).map(|c| c.call_index).unwrap_or(0);
    Ok(PalwSegmentReplayV1 {
        window: PalwSegmentResumeWindowV1 {
            segment_index: opening.segment_index,
            leaf_start: start_leaf,
            leaf_end: end_leaf,
            first_call: call_of(start_leaf),
            last_call: call_of(end_leaf - 1),
        },
        leaf_hashes: kept.unwrap_or_default(),
        calls_replayed: u32::try_from(plan.window.last_step - plan.window.first_step + 1).unwrap_or(u32::MAX),
        matches: root == opening.binding.step_merkle_root,
    })
}

/// **S1 (4), authenticated: a court (or a sampled-site check) resumes the segment of a capture it
/// holds.** The opening is the capture's own and is authenticated against the capture's OWN binding
/// roots — so this answers "does the capture's committed segment replay", and whether the capture is
/// the claim's is the caller's question, answered first (`verify_material`). `job` defaults to the
/// capture's; a fold's own prompt is used when it carries one.
#[allow(clippy::too_many_arguments)]
pub fn base0_replay_capture_segment_v2<K: Base0FpIntervalKernelsV1 + ?Sized>(
    kernels: &K,
    class_profile: &PalwShapeProfileV3,
    capture: &[u8],
    seat_count: u16,
    segment_index: u16,
    job: Option<&PalwJobContextV2>,
    prompt: &[usize],
    max_step_leaf_count: u64,
    prompt_ids_form: PalwPromptIdsFormV1,
) -> Result<PalwSegmentReplayV1, Base0SegmentRefusalV1> {
    let retention = base0_material_decode_any_v1(capture).map_err(|_| Base0SegmentRefusalV1::NotAnOpening)?;
    let opening = base0_segment_opening_v2(
        &retention,
        seat_count,
        segment_index,
        Base0SegmentAnchorV1::Committed,
        Some(class_profile),
        max_step_leaf_count,
    )
    .and_then(|o| o.encode_v2())
    .map_err(|e| Base0SegmentRefusalV1::Replay(e.to_string()))?;
    let binding = retention.binding();
    let claim = PalwSegmentClaimV1 {
        execution_root: binding.committed_execution_root,
        trace_root: binding.full_logits_trace_root,
        seat_count,
        segment_index,
    };
    let held: Vec<usize> = match &retention {
        Base0RetentionV1::Folded(m) if !m.prompt_token_ids.is_empty() => m.prompt_token_ids.iter().map(|t| *t as usize).collect(),
        _ => prompt.to_vec(),
    };
    base0_replay_segment_opening_v2(
        kernels,
        class_profile,
        job.unwrap_or(&binding.job_context),
        &held,
        &opening,
        claim,
        max_step_leaf_count,
        prompt_ids_form,
    )
}

/// **The prompt a court verb replays a capture's segment under**: the caller's when it carries one
/// (a free-prompt claim's), else the one the capture's anchor derives — an attempt's prompt is a pure
/// function of its anchor, and its dense retention carries none. The replay still checks whichever it
/// gets against the job's `prompt_token_ids_hash` before reading it.
pub fn base0_court_prompt_v1<B: kaspa_consensus_core::palw_backend::PalwExecutionBackendV1 + ?Sized>(
    backend: &B,
    job_id: Hash64,
    prompt: &[usize],
) -> Vec<usize> {
    if !prompt.is_empty() {
        return prompt.to_vec();
    }
    backend.job_for_anchor(job_id).map(|(_, derived)| derived).unwrap_or_default()
}
