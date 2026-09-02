//! **The checkpoint state chunk map for the deterministic-integer family** — ADR-0030 §3's
//! `state_chunk_map_id`, made a network fact for the one family whose state is a function of its
//! geometry.
//!
//! # Why this was opaque, and why it stops being opaque HERE
//!
//! [`crate::palw_legs`] left `state_layout_id` and `state_chunk_map_id` deliberately unpinned,
//! with a reason that was correct at the time and is worth quoting rather than paraphrasing:
//! *"the runtime's KV cache is not even f32"*. That is a fact about **llama.cpp**, whose cache
//! dtype, padding and head interleave are measurement questions no schema should guess at.
//!
//! It is not a fact about `PALW-BASE-0`. That class's profile pins `kv_cache_f16 = 0` and its
//! engine holds the replay state as `[layer][position][kv_dim]` **`int8` codes** — one byte per
//! element, no padding, no dtype choice, no interleave. Every number the layout needs is already
//! in the shape profile, which is already inside the class id. So for this family the map is
//! DERIVED, and a registration that could pick it could pick which bytes the court adjudicates
//! against (the `CeilingNotDerived` pattern, applied to state).
//!
//! Classes outside the family keep the opaque id. This module refuses them by name rather than
//! producing a layout that would describe bytes their runtime does not hold.
//!
//! # What a map has to make true
//!
//! An opening proves "these bytes are at this position under this root". It proves nothing about
//! what is NOT opened unless the layout is pinned too — the lesson [`crate::palw_artifact`]
//! already wrote down for weights, and state is the same shape of problem. So the enumeration
//! here covers **every byte of the replay state exactly once, in one order**, and
//! [`tests::the_map_covers_every_element_exactly_once`] is what makes that a property rather than
//! a hope: a gap would let a producer omit state a replay needs, an overlap would let it commit
//! two different values for one element and open whichever the dispute favours.
//!
//! # What this module deliberately is not
//!
//! It is the **map**, not the capture: nothing here reads an engine or writes a checkpoint. It is
//! also not the anchor policy — how far back a court walks before it replays is
//! [`crate::palw_bisect`]'s and [`crate::palw_step_refute`]'s business. This answers one question,
//! which nothing could answer before: *given a checkpoint's chunk index, which elements of the
//! state are in it, and where.*

use crate::Hash64;
use crate::palw_step::{PalwLayerKindV1, PalwShapeProfileV3, PalwStepLaneV1, state_chunk_map_id_v1};
use crate::palw_step_leg::{PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES, PALW_STEP_LEG_MAX_STATE_CHUNKS};
use crate::palw_v2::PalwJobContextV2;

/// The registration preimage of the integer family's `state_chunk_map_id`.
///
/// Every degree of freedom the layout has is named: the element dtype, the outer ordering, the
/// row width's derivation and the chunk width's derivation. A reader who disagrees with any of
/// them is describing a different map and must mint a different id — which is the entire purpose
/// of the string being the preimage rather than a comment.
pub const PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V1: &str = "palw-integer-kv/i8/kind-major(k,v)/layer-asc/position-asc/row=attn_kv_heads*attn_head_dim/\
     chunk=floor(1048576/row)/v1";

/// **The same map at the width an `i32` cache actually has (v2).**
///
/// The v1 name says `i8`, and it is exact for a class whose KV cache is `Vec<Vec<Vec<i8>>>` —
/// BASE-0's is. `A16Cache` holds `Vec<Vec<Vec<i32>>>`, and a class that declares v1 over that
/// state declares a layout it does not hold: `row_bytes` and the element COUNT come out equal, so
/// the length guard in a serializer passes and every value outside `i8` is lost. The producer
/// signs a checkpoint that opens to a state it never had.
///
/// This is additive. v1 is untouched, its id is unchanged, and no shipped class moves — because
/// `state_chunk_map_id` is a field of `PalwShapeProfileV3` and the shape profile id IS the class
/// id, so a class adopting v2 is a DIFFERENT class from the one testnet-11 carries. That is the
/// decision this constant exists to make available, not one it makes.
///
/// The alternative was narrowing the cache to `i8`, which changes what the model computes and
/// therefore every answer it gives. Describing the state correctly changes nothing but the
/// description.
pub const PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V2: &str = "palw-integer-kv/i32-le/kind-major(k,v)/layer-asc/position-asc/row=attn_kv_heads*attn_head_dim*4/\
     chunk=floor(1048576/row)/v2";

/// `state_chunk_map_id` for an integer class whose KV elements are 32-bit.
pub fn integer_kv_state_chunk_map_id_v2() -> Hash64 {
    state_chunk_map_id_v1(PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V2)
}

/// The v2 geometry: identical to v1 in every rule except the row width, which is four bytes per
/// element rather than one. Chunking still derives from the width, so a wider row simply covers
/// fewer positions per chunk.
pub fn integer_kv_state_geometry_v2(
    profile: &PalwShapeProfileV3,
    positions: u32,
) -> Result<PalwStateChunkGeometryV1, PalwStateChunkMapError> {
    let mut geometry = integer_kv_state_geometry_v1(profile, positions)?;
    let row_bytes = (profile.attn_kv_heads as u64).saturating_mul(profile.attn_head_dim as u64).saturating_mul(4);
    if row_bytes > PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 {
        return Err(PalwStateChunkMapError::RowExceedsChunk { row_bytes, max: PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES });
    }
    // Rebuilt rather than patched: `positions_per_chunk` and the chunk count are FUNCTIONS of the
    // width, and widening the row while leaving them at v1's values would describe chunks four
    // times larger than the cap this leg enforces.
    let positions_per_chunk = (PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 / row_bytes).min(positions as u64) as u32;
    geometry.row_bytes = row_bytes as u32;
    geometry.positions_per_chunk = positions_per_chunk;
    geometry.chunks_per_slice = positions.div_ceil(positions_per_chunk);
    Ok(geometry)
}

/// **The RECURRENCE's map: a state, not a history** (ADR-0077 Decision 10).
///
/// The two integer-KV maps above chunk the *cache*, and a cache is the whole history: at position
/// `p` it holds `p` rows, so an anchor over it costs `O(p)` bytes however it is chunked. That is
/// not a defect of the map — attention genuinely reads every prior key — and it is exactly why
/// ADR-0077 Decision 11 does not make an attention class's close flat in `n_ctx`.
///
/// A `GatedDeltaNet` layer is the other kind. Its replay state is a `k_dim × v_dim` delta matrix
/// per head plus the convolution's window, and neither depends on how many positions have been
/// folded into them. So a checkpoint over the recurrence is a genuine SUMMARY, the anchored replay
/// after it is `interval` positions of arithmetic, and both are constant in the context — which is
/// the half of Decision 11 that actually buys a wider row.
///
/// **The string is the EXECUTOR's, verbatim.** `misaka-palw-base0` captures and restores against
/// this layout (`base0_gdn_state_geometry_v1`, `Base0CheckpointCaptureV1::push`,
/// `A16Cache::from_state_chunks_v1`), and the id is `H(name)`: a court that spelled the layout its
/// own way would mint a second id, and a class whose capture and whose adjudicator disagree about
/// their map id is a class no dispute can open. The consensus crate is the lower one, so the
/// spelling belongs here and the engine crate should reference it rather than restate it.
pub const PALW_GDN_STATE_CHUNK_MAP_NAME_V1: &str = "palw-gdn-state/i32-le/kind-major(delta,conv)/layer-asc/head-asc/\
     row-asc/delta-row=gdn_head_k_dim*4/conv-row=(2*gdn_head_k_dim+gdn_head_v_dim)*gdn_heads*4/chunk<=2^20/v1";

/// `state_chunk_map_id` for a class whose recurrence checkpoints its own state.
///
/// A class that adopts it is a DIFFERENT class from one that does not — `state_chunk_map_id` is a
/// field of `PalwShapeProfileV3` and the shape profile id IS the class id — so this registers no
/// map on any shipped row and repairs none of them. That is the decision it makes available, not
/// one it makes.
pub fn gdn_state_chunk_map_id_v1() -> Hash64 {
    state_chunk_map_id_v1(PALW_GDN_STATE_CHUNK_MAP_NAME_V1)
}

/// **A HYBRID's map names both halves, because a hybrid has both kinds of layer.**
///
/// `state_chunk_map_id` is one field and a Qwen3.6-shaped class holds an attention cache AND a
/// recurrence state (`full_attention_interval` 4: every fourth layer is attention). Registering
/// only [`PALW_GDN_STATE_CHUNK_MAP_NAME_V1`] would leave the attention half unanchored — a
/// refutation at an attention site would carry a checkpoint the court cannot read the geometry of,
/// which is `Unadjudicable` on honest material — and registering only the KV map would leave the
/// recurrence at its genesis-anchored `O(n_ctx)` replay, which is the ceiling Decision 10 exists to
/// lift.
///
/// So the hybrid's map is the COMPOSITION, and it is spelled as the two names rather than as a
/// third description of the same bytes: a reader who disagrees with either half is disagreeing
/// with a layout that is already written down, and the composition cannot drift from its parts.
/// A function rather than a `const` only because Rust has no `const` string concatenation without
/// a dependency; the value is fixed by its two parts and is a compile-time fact in every sense
/// that matters.
pub fn palw_hybrid_state_chunk_map_name_v1() -> String {
    format!("palw-hybrid-state/attn={PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V2}/gdn={PALW_GDN_STATE_CHUNK_MAP_NAME_V1}/v1")
}

/// `state_chunk_map_id` for a class with both kinds of layer.
pub fn hybrid_state_chunk_map_id_v1() -> Hash64 {
    state_chunk_map_id_v1(&palw_hybrid_state_chunk_map_name_v1())
}

/// **The same recurrence state, enumerated so ONE HEAD's window is one opening (v2).**
///
/// # The measurement v1 lost the argument to
///
/// v1's delta half is head-sliced — a court replaying `KDESC_Q36_GDN_STEP` needs one head's
/// `k_dim × v_dim` matrix and opens `v_dim` rows of `k_dim × 4` — and its convolution half is not:
/// `conv-row = (2·k_dim + v_dim) · gdn_heads · 4` spans EVERY head, so a court that needs one
/// head's four-tap window opens the whole layer's and pays for thirty-one heads it will not read.
/// On Qwen3.6's geometry (32 heads of 128) that is 196,608 bytes against a 65,536-byte delta and
/// an 81,920-byte carrier: the window alone is two and a half closes, and it is the term that used
/// to make `palw_context_ladder`'s `a_hybrid_row_does_not_fit_the_carrier` true — the test now
/// reads `a_hybrid_row_fits_the_carrier`, and its first assertion is the one that flipped.
///
/// # What v2 changes, and what it deliberately does not
///
/// **Exactly the conv enumeration.** The same bytes, in the same little-endian `i32`, covering the
/// same state — re-ordered so the outer key is the HEAD rather than the window row, which makes a
/// per-head opening expressible. `gdn_conv_head_slice_bytes_v2` is then
/// `gdn_conv_kernel · (2·k_dim + v_dim) · 4` = 6,144 bytes on that geometry, and one anchored
/// recurrence opening is 71,680 — inside the carrier for the first time.
///
/// The delta half is byte-identical to v1's, because it was already right.
///
/// # The gather, spelled out rather than left to a reader
///
/// A conv window row is the concatenated projections `[q | k | v]`, region-major and head-major
/// inside each region — the executor's own `current.extend(q); extend(k); extend(v)`. Head `h`'s
/// channels are therefore three disjoint ranges, not one, and the name says which three: a map
/// whose gather a reader has to reconstruct is a map two readers reconstruct differently.
///
/// **A class that adopts it is a DIFFERENT class.** `state_chunk_map_id` is a field of
/// `PalwShapeProfileV3` and the shape profile id IS the class id, so this registers nothing on any
/// shipped row and repairs none of them, exactly as v1 registered nothing.
pub const PALW_GDN_STATE_CHUNK_MAP_NAME_V2: &str = "palw-gdn-state/i32-le/kind-major(delta,conv)/layer-asc/head-asc/row-asc/\
     delta-row=gdn_head_k_dim*4/conv-head-row=(2*gdn_head_k_dim+gdn_head_v_dim)*4/\
     conv-head-gather=[q:h*k,k:heads*k+h*k,v:2*heads*k+h*v]/chunk<=2^20/v2";

/// `state_chunk_map_id` for a class whose recurrence checkpoints its own state head by head.
pub fn gdn_state_chunk_map_id_v2() -> Hash64 {
    state_chunk_map_id_v1(PALW_GDN_STATE_CHUNK_MAP_NAME_V2)
}

/// **The hybrid's map over the head-sliced recurrence** — [`palw_hybrid_state_chunk_map_name_v1`]
/// with its `gdn=` half at v2, spelled as its two parts for the same reason the first composition
/// is.
///
/// A separate name and therefore a separate id, because the composition of two named layouts is
/// not the same layout when one of them changes: a class that registered the v1 composition
/// commits its convolution row-major, and reading those chunks head-major would restore a state
/// the producer never held.
pub fn palw_hybrid_state_chunk_map_name_v2() -> String {
    format!("palw-hybrid-state/attn={PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V2}/gdn={PALW_GDN_STATE_CHUNK_MAP_NAME_V2}/v2")
}

/// `state_chunk_map_id` for a hybrid class whose recurrence half is [v2](gdn_state_chunk_map_id_v2).
pub fn hybrid_state_chunk_map_id_v2() -> Hash64 {
    state_chunk_map_id_v1(&palw_hybrid_state_chunk_map_name_v2())
}

/// **What a court opening of ONE head's convolution window costs under v2**:
/// `gdn_conv_kernel` rows of `(2·k_dim + v_dim) × 4` bytes.
///
/// The head-sliced twin of [`gdn_conv_window_bytes_v1`], and the whole of what v2 buys. A court
/// replaying one head's recurrence needs that head's four taps and nothing else; v1 made it carry
/// the layer's.
pub fn gdn_conv_head_slice_bytes_v2(profile: &PalwShapeProfileV3) -> Option<u64> {
    let k = profile.gdn_head_k_dim as u64;
    let v = profile.gdn_head_v_dim as u64;
    let kernel = profile.gdn_conv_kernel as u64;
    if profile.gdn_heads == 0 || k == 0 || v == 0 {
        return None;
    }
    let row = 2u64.checked_mul(k)?.checked_add(v)?.checked_mul(4)?;
    (row <= PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64).then_some(())?;
    row.checked_mul(kernel)
}

/// The whole recurrence opening a court pays at one anchored step under v2: one head's delta plus
/// that head's convolution window.
pub fn gdn_state_row_bytes_v2(profile: &PalwShapeProfileV3) -> Option<u64> {
    gdn_delta_head_slice_bytes_v1(profile)?.checked_add(gdn_conv_head_slice_bytes_v2(profile)?)
}

/// **What THIS class's recurrence opening costs, read off the map it registered** — the delta term
/// and the convolution term, separately, so a caller can see which half is the expensive one.
///
/// One dispatch, here, because the alternative is every caller writing `if map == v2 { … }` and
/// the first one to forget prices a v2 class at v1's window (or the reverse, which is worse: a
/// price below what the evidence costs admits a class whose disputes nobody can carry). `None` for
/// a class that registers neither recurrence map, which is the honest answer to "what does its
/// recurrence anchor cost" — it has none.
pub fn gdn_state_terms_for_map_v1(profile: &PalwShapeProfileV3) -> Option<(u64, u64)> {
    let declared = profile.state_chunk_map_id;
    let delta = gdn_delta_head_slice_bytes_v1(profile)?;
    if declared == gdn_state_chunk_map_id_v2() || declared == hybrid_state_chunk_map_id_v2() {
        Some((delta, gdn_conv_head_slice_bytes_v2(profile)?))
    } else if declared == gdn_state_chunk_map_id_v1() || declared == hybrid_state_chunk_map_id_v1() {
        Some((delta, gdn_conv_window_bytes_v1(profile)?))
    } else {
        None
    }
}

/// [`gdn_state_terms_for_map_v1`]'s total: what one anchored recurrence step opens on the map this
/// class actually registered.
pub fn gdn_state_row_bytes_for_map_v1(profile: &PalwShapeProfileV3) -> Option<u64> {
    let (delta, conv) = gdn_state_terms_for_map_v1(profile)?;
    delta.checked_add(conv)
}

/// **What a court opening of ONE head's delta state costs**, from the executor's own geometry:
/// `v_dim` rows of `k_dim × 4` bytes.
///
/// One HEAD, not all of them, because the recurrence arm is head-sliced — the refutation replays
/// one head's `k_dim × v_dim` state (`KDESC_Q36_GDN_STEP`), which is what lets a 40-layer hybrid
/// have a context at all. Four bytes an element: the state the adjudicator holds is `u32` f32 bit
/// patterns, and describing it as anything narrower is the defect
/// [`PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V2`] was minted to correct.
pub fn gdn_delta_head_slice_bytes_v1(profile: &PalwShapeProfileV3) -> Option<u64> {
    let k = profile.gdn_head_k_dim as u64;
    let v = profile.gdn_head_v_dim as u64;
    if profile.gdn_heads == 0 || k == 0 || v == 0 {
        return None;
    }
    let row = k.checked_mul(4)?;
    (row <= PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64).then_some(())?;
    row.checked_mul(v)
}

/// **What a court opening of the convolution window costs**, from the same geometry:
/// `gdn_conv_kernel` rows of `(2·k_dim + v_dim) · heads × 4` bytes.
///
/// **Not head-sliced, and that is the executor's layout rather than a choice made here**: a conv
/// row spans every head (`conv-row=(2*gdn_head_k_dim+gdn_head_v_dim)*gdn_heads*4`), so a court that
/// needs one head's window opens the row that carries it and gets the rest. It is the dominant term
/// on a wide hybrid and `a_hybrid_row_does_not_fit_the_carrier` is what measures it rather than
/// asserting it fits.
pub fn gdn_conv_window_bytes_v1(profile: &PalwShapeProfileV3) -> Option<u64> {
    let k = profile.gdn_head_k_dim as u64;
    let v = profile.gdn_head_v_dim as u64;
    let heads = profile.gdn_heads as u64;
    let kernel = profile.gdn_conv_kernel as u64;
    if heads == 0 || k == 0 || v == 0 {
        return None;
    }
    let row = 2u64.checked_mul(k)?.checked_add(v)?.checked_mul(heads)?.checked_mul(4)?;
    (row <= PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64).then_some(())?;
    row.checked_mul(kernel)
}

/// The whole recurrence opening a court pays at one anchored step: one head's delta plus the
/// convolution window.
pub fn gdn_state_row_bytes_v1(profile: &PalwShapeProfileV3) -> Option<u64> {
    gdn_delta_head_slice_bytes_v1(profile)?.checked_add(gdn_conv_window_bytes_v1(profile)?)
}

/// The registration preimage of the integer family's `state_layout_id`.
///
/// The map's companion. `PalwCheckpointProfileV1` carries a `state_layout_id` inside its
/// `profile_hash`, and the v2 checkpoint leg carries the chunk map beside it — two identities over
/// one set of bytes, which is two chances to drift. They are minted from the SAME descriptor here,
/// under their own domains, so "the layout" and "the chunking of the layout" cannot disagree about
/// which bytes they are talking about.
pub const PALW_INTEGER_KV_STATE_LAYOUT_NAME_V1: &str = PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V1;

/// `state_layout_id` for every class in the deterministic-integer family.
pub fn integer_kv_state_layout_id_v1() -> Hash64 {
    crate::palw_legs::state_layout_id_v1(PALW_INTEGER_KV_STATE_LAYOUT_NAME_V1)
}

/// **The checkpoint profile a producer in this family must file**, at the interval its class
/// registered.
///
/// A constructor rather than a constant because the interval is the one free parameter; the layout
/// is not. Before this there was no canonical `state_layout_id` to file at all, so every producer
/// would have invented one and every one of them would have been a different class of checkpoint.
/// **The interval the deterministic-integer family actually runs**, and the only one its court
/// will accept.
///
/// The interval was the profile's one free parameter, carried inside `committed_execution_root`
/// and checked by nothing. Two things followed. A producer could name an interval larger than its
/// decode count and file a binding with ZERO checkpoints — opting out of the leg the checkpoint
/// evidence exists to provide — and pass, because `checkpoint_count == decode_calls / interval`
/// was satisfied at zero. And an honest producer and an honest challenger picking different
/// intervals computed different execution roots for the SAME job, so the challenger read a
/// reproducible execution as fraud.
///
/// One is what every producer in this tree files. Pinning it is the interim; naming it per class
/// in the catalog is the fuller answer and moves the catalog root.
pub const PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1: u32 = 1;

pub fn integer_kv_checkpoint_profile_v1(checkpoint_interval: u32) -> crate::palw_legs::PalwCheckpointProfileV1 {
    crate::palw_legs::PalwCheckpointProfileV1 {
        version: crate::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
        checkpoint_interval,
        state_layout_id: integer_kv_state_layout_id_v1(),
    }
}

/// `state_chunk_map_id` for every class in the deterministic-integer family.
///
/// One id for the whole family, not one per class: the map is a function of the profile, and the
/// profile is already bound beside this id everywhere the id appears (`checkpoint_leaf_hash_v2`
/// takes the checkpoint profile hash, and the step binding carries the shape profile in full). A
/// per-class id would add a second name for a fact the first one already fixes, and two names for
/// one fact is how the `rms_eps` split happened.
pub fn integer_kv_state_chunk_map_id_v1() -> Hash64 {
    state_chunk_map_id_v1(PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V1)
}

#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PalwStateChunkMapError {
    /// The profile is not in the family this map describes. Refused by NAME, not silently
    /// approximated: a float-lane runtime's cache is the measurement question this module's
    /// header explains it is not answering.
    #[error("the state chunk map v1 is defined for the integer lane only; this profile is {lane:?}")]
    NotTheIntegerFamily { lane: PalwStepLaneV1 },
    /// No attention layer means no KV state, and a map over nothing is not an empty map — it is a
    /// profile that should never have declared a checkpoint leg.
    #[error("the profile declares no attention layer, so it has no KV replay state to chunk")]
    NoAttentionLayers,
    #[error("the profile's kv row width is zero (attn_kv_heads {kv_heads} × attn_head_dim {head_dim})")]
    ZeroRowWidth { kv_heads: u16, head_dim: u32 },
    /// One row must fit one chunk. Nothing in the family comes close, and a class that did would
    /// need a map that splits a row — a different map, with a different id.
    #[error("one kv row is {row_bytes} bytes and a state chunk holds at most {max}")]
    RowExceedsChunk { row_bytes: u64, max: usize },
    #[error("a state at zero positions has nothing to commit")]
    ZeroPositions,
    #[error("the layout needs {got} chunks and the leg admits at most {max}")]
    TooManyChunks { got: u64, max: usize },
}

/// Which half of the cache a chunk belongs to. The discriminants are the enumeration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum PalwStateChunkKindV1 {
    Key = 0,
    Value = 1,
}

impl PalwStateChunkKindV1 {
    pub const ALL: [PalwStateChunkKindV1; 2] = [PalwStateChunkKindV1::Key, PalwStateChunkKindV1::Value];
}

/// The derived layout of one job's replay state. Every field is a function of the profile and the
/// position count; none is a choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStateChunkGeometryV1 {
    /// `attn_kv_heads × attn_head_dim`, one `i8` per element.
    pub row_bytes: u32,
    /// How many positions ride one chunk: as many as fit under the leg's per-chunk cap.
    pub positions_per_chunk: u32,
    /// The attention layers, ascending — the only layers that hold KV state.
    pub attn_layers: Vec<u16>,
    /// Positions the state covers.
    pub positions: u32,
    /// `positions.div_ceil(positions_per_chunk)` — chunks in one `(kind, layer)` slice.
    pub chunks_per_slice: u32,
}

impl PalwStateChunkGeometryV1 {
    /// Total chunks in the map: two kinds × attention layers × chunks per slice.
    pub fn chunk_count(&self) -> u64 {
        2 * self.attn_layers.len() as u64 * self.chunks_per_slice as u64
    }

    /// Total bytes the state occupies. The map covers exactly this many.
    pub fn total_bytes(&self) -> u64 {
        2 * self.attn_layers.len() as u64 * self.positions as u64 * self.row_bytes as u64
    }
}

/// One chunk of the map: a contiguous run of positions of one layer's K or V history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwStateChunkEntryV1 {
    pub kind: PalwStateChunkKindV1,
    /// The layer index in the PROFILE's numbering, not the ordinal among attention layers — a
    /// court reading this entry is about to index a layer table.
    pub attn_layer: u16,
    pub position_start: u32,
    pub position_count: u32,
    pub row_bytes: u32,
}

impl PalwStateChunkEntryV1 {
    pub fn byte_len(&self) -> u64 {
        self.position_count as u64 * self.row_bytes as u64
    }
}

/// **The layout of the replay state at `positions`, derived from the profile.**
pub fn integer_kv_state_geometry_v1(
    profile: &PalwShapeProfileV3,
    positions: u32,
) -> Result<PalwStateChunkGeometryV1, PalwStateChunkMapError> {
    if profile.lane != PalwStepLaneV1::Int32 {
        return Err(PalwStateChunkMapError::NotTheIntegerFamily { lane: profile.lane });
    }
    if positions == 0 {
        return Err(PalwStateChunkMapError::ZeroPositions);
    }
    let row_bytes = profile.attn_kv_heads as u64 * profile.attn_head_dim as u64;
    if row_bytes == 0 {
        return Err(PalwStateChunkMapError::ZeroRowWidth { kv_heads: profile.attn_kv_heads, head_dim: profile.attn_head_dim });
    }
    if row_bytes > PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 {
        return Err(PalwStateChunkMapError::RowExceedsChunk { row_bytes, max: PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES });
    }
    let attn_layers: Vec<u16> = (0..profile.layer_count).filter(|&l| profile.layer_kind(l) == PalwLayerKindV1::Attention).collect();
    if attn_layers.is_empty() {
        return Err(PalwStateChunkMapError::NoAttentionLayers);
    }
    // Derived, not chosen: the widest run of rows that fits the leg's per-chunk cap. `row_bytes`
    // is known non-zero and ≤ the cap, so this is ≥ 1 without a clamp that could hide a zero.
    let positions_per_chunk = (PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 / row_bytes).min(positions as u64) as u32;
    let chunks_per_slice = positions.div_ceil(positions_per_chunk);
    let geometry =
        PalwStateChunkGeometryV1 { row_bytes: row_bytes as u32, positions_per_chunk, attn_layers, positions, chunks_per_slice };
    let chunk_count = geometry.chunk_count();
    if chunk_count > PALW_STEP_LEG_MAX_STATE_CHUNKS as u64 {
        return Err(PalwStateChunkMapError::TooManyChunks { got: chunk_count, max: PALW_STEP_LEG_MAX_STATE_CHUNKS });
    }
    Ok(geometry)
}

/// How many positions the state covers at a checkpoint that has covered `covered_decode_call`
/// decode calls.
///
/// The cache holds one row per position written, which is the prefill plus one per decode call
/// that has run — the same `prefill + decode_calls` count [`crate::palw_step::kv_aux_leaf_count`]
/// derives for the KV aux series. Stating it once, here, is what keeps the checkpoint leg and the
/// aux series from disagreeing about how long the history is.
pub fn integer_kv_positions_at_v1(context: &PalwJobContextV2, covered_decode_call: u32) -> u32 {
    context.declared_prefill_tokens.saturating_add(covered_decode_call)
}

/// The entry at `chunk_index`, or `None` past the end of the map.
pub fn integer_kv_state_chunk_entry_v1(geometry: &PalwStateChunkGeometryV1, chunk_index: u64) -> Option<PalwStateChunkEntryV1> {
    if chunk_index >= geometry.chunk_count() {
        return None;
    }
    let per_kind = geometry.attn_layers.len() as u64 * geometry.chunks_per_slice as u64;
    let kind = if chunk_index < per_kind { PalwStateChunkKindV1::Key } else { PalwStateChunkKindV1::Value };
    let within_kind = chunk_index % per_kind;
    let layer_ordinal = (within_kind / geometry.chunks_per_slice as u64) as usize;
    let block = (within_kind % geometry.chunks_per_slice as u64) as u32;
    let position_start = block * geometry.positions_per_chunk;
    let position_count = (geometry.positions - position_start).min(geometry.positions_per_chunk);
    Some(PalwStateChunkEntryV1 {
        kind,
        attn_layer: geometry.attn_layers[layer_ordinal],
        position_start,
        position_count,
        row_bytes: geometry.row_bytes,
    })
}

/// **Where one element of the state lives**: the chunk that holds `(kind, attn_layer, position)`
/// and the byte offset of its row inside that chunk.
///
/// This is the direction a court actually walks — it knows which row a disputed step read and
/// needs the opening that proves it — and it is the inverse of
/// [`integer_kv_state_chunk_entry_v1`], which the round-trip test pins.
pub fn integer_kv_state_locate_v1(
    geometry: &PalwStateChunkGeometryV1,
    kind: PalwStateChunkKindV1,
    attn_layer: u16,
    position: u32,
) -> Option<(u64, u32)> {
    if position >= geometry.positions {
        return None;
    }
    let layer_ordinal = geometry.attn_layers.iter().position(|&l| l == attn_layer)? as u64;
    let per_kind = geometry.attn_layers.len() as u64 * geometry.chunks_per_slice as u64;
    let block = position / geometry.positions_per_chunk;
    let chunk_index = (kind as u64) * per_kind + layer_ordinal * geometry.chunks_per_slice as u64 + block as u64;
    let byte_offset = (position % geometry.positions_per_chunk) * geometry.row_bytes;
    Some((chunk_index, byte_offset))
}

/// **Read one position's row out of an opened chunk.**
///
/// The length check is the whole point: a chunk whose byte count is not the canonical one for its
/// entry describes a state of a different shape, and a court that read a row out of it anyway
/// would be adjudicating against bytes the map never promised were there. `None` is
/// "unadjudicable from this material", never a default row.
pub fn integer_kv_state_row_v1<'a>(entry: &PalwStateChunkEntryV1, chunk_bytes: &'a [u8], position: u32) -> Option<&'a [u8]> {
    if chunk_bytes.len() as u64 != entry.byte_len() {
        return None;
    }
    let offset = position.checked_sub(entry.position_start)?;
    if offset >= entry.position_count {
        return None;
    }
    let start = offset as usize * entry.row_bytes as usize;
    chunk_bytes.get(start..start + entry.row_bytes as usize)
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use std::collections::HashSet;

    fn rc_profile() -> PalwShapeProfileV3 {
        base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the RC geometry is a valid profile")
    }

    /// **v2 is four bytes per element, and it is a different map — both halves matter.**
    ///
    /// Four bytes because that is what an `i32` cache holds; a different id because
    /// `state_chunk_map_id` is inside the shape profile id, so a class that adopts v2 is a
    /// different class. If these two ever collided, a class could change the width of the state it
    /// commits to without changing its identity, and two nodes would open the same checkpoint to
    /// different states while agreeing they were the same class.
    #[test]
    fn the_v2_map_is_four_bytes_per_element_and_a_distinct_identity() {
        let profile = rc_profile();
        let v1 = integer_kv_state_geometry_v1(&profile, 64).expect("v1 geometry");
        let v2 = integer_kv_state_geometry_v2(&profile, 64).expect("v2 geometry");

        assert_eq!(v2.row_bytes as u64, v1.row_bytes as u64 * 4);
        assert_eq!(v2.row_bytes as u64, profile.attn_kv_heads as u64 * profile.attn_head_dim as u64 * 4);
        assert_ne!(integer_kv_state_chunk_map_id_v2(), integer_kv_state_chunk_map_id_v1());

        // The chunking is a function of the width, not a constant carried over: a wider row must
        // cover fewer positions per chunk, or the leg's per-chunk cap is exceeded by four times.
        assert!(v2.positions_per_chunk <= v1.positions_per_chunk);
        assert!(
            v2.positions_per_chunk as u64 * v2.row_bytes as u64 <= PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64,
            "a v2 chunk still fits the cap"
        );
        assert!(v2.chunks_per_slice >= v1.chunks_per_slice, "and the same positions need at least as many chunks");
    }

    /// The id is a consensus identity: it may move only when the descriptor moves, and a reader
    /// who changes the string without meaning to change the map finds out here.
    #[test]
    fn the_map_id_is_the_descriptor_and_nothing_else() {
        assert_eq!(integer_kv_state_chunk_map_id_v1(), state_chunk_map_id_v1(PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V1));
        assert_ne!(integer_kv_state_chunk_map_id_v1(), Hash64::default(), "the unregistered sentinel is not a map id");
        assert_ne!(
            integer_kv_state_chunk_map_id_v1(),
            state_chunk_map_id_v1(&format!("{PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V1}x")),
            "a changed descriptor must change the id"
        );
    }

    /// **The property the whole module exists for.** Every element of the state is in exactly one
    /// chunk, at exactly one offset. A gap lets a producer omit state a replay needs; an overlap
    /// lets it commit two values for one element and open whichever the dispute favours.
    #[test]
    fn the_map_covers_every_element_exactly_once() {
        let profile = rc_profile();
        // A width that forces more than one block per slice, so the position-blocking is exercised
        // rather than collapsing to one chunk per slice.
        for positions in [1u32, 2, 7, 127] {
            let mut geometry = integer_kv_state_geometry_v1(&profile, positions).unwrap();
            geometry.positions_per_chunk = 3.min(positions);
            geometry.chunks_per_slice = positions.div_ceil(geometry.positions_per_chunk);

            let mut seen: HashSet<(u8, u16, u32)> = HashSet::new();
            let mut bytes = 0u64;
            for chunk_index in 0..geometry.chunk_count() {
                let entry = integer_kv_state_chunk_entry_v1(&geometry, chunk_index).expect("in range");
                assert!(entry.position_count > 0, "chunk {chunk_index} is empty");
                bytes += entry.byte_len();
                for p in entry.position_start..entry.position_start + entry.position_count {
                    assert!(seen.insert((entry.kind as u8, entry.attn_layer, p)), "element covered twice: {entry:?} at {p}");
                    // The inverse agrees, element by element.
                    let (back, offset) = integer_kv_state_locate_v1(&geometry, entry.kind, entry.attn_layer, p).unwrap();
                    assert_eq!(back, chunk_index, "locate disagrees with enumerate for {entry:?} at {p}");
                    assert_eq!(offset, (p - entry.position_start) * entry.row_bytes);
                }
            }
            let expected = 2 * geometry.attn_layers.len() * positions as usize;
            assert_eq!(seen.len(), expected, "coverage is not total at {positions} positions");
            assert_eq!(bytes, geometry.total_bytes(), "the chunks do not sum to the state size");
        }
    }

    #[test]
    fn one_chunk_per_slice_when_the_history_fits() {
        let profile = rc_profile();
        let geometry = integer_kv_state_geometry_v1(&profile, 127).unwrap();
        // RC BASE-0: 4 kv heads × 64 = 256 bytes a row, so 4096 positions fit one chunk.
        assert_eq!(geometry.row_bytes, 256);
        assert_eq!(geometry.positions_per_chunk, 127, "the whole history fits, so the chunk is the history");
        assert_eq!(geometry.chunks_per_slice, 1);
        assert_eq!(geometry.attn_layers, vec![0, 1, 2, 3], "every RC layer is an attention layer");
        assert_eq!(geometry.chunk_count(), 8, "K and V for four layers");
        assert_eq!(geometry.total_bytes(), 8 * 127 * 256);
    }

    #[test]
    fn the_enumeration_is_kind_major_then_layer_then_position() {
        let profile = rc_profile();
        let mut geometry = integer_kv_state_geometry_v1(&profile, 8).unwrap();
        geometry.positions_per_chunk = 4;
        geometry.chunks_per_slice = 2;
        let kinds: Vec<_> = (0..geometry.chunk_count())
            .map(|i| {
                let e = integer_kv_state_chunk_entry_v1(&geometry, i).unwrap();
                (e.kind, e.attn_layer, e.position_start)
            })
            .collect();
        assert_eq!(kinds[0], (PalwStateChunkKindV1::Key, 0, 0));
        assert_eq!(kinds[1], (PalwStateChunkKindV1::Key, 0, 4));
        assert_eq!(kinds[2], (PalwStateChunkKindV1::Key, 1, 0));
        assert_eq!(kinds[8], (PalwStateChunkKindV1::Value, 0, 0), "the V half starts after every K chunk");
        assert_eq!(integer_kv_state_chunk_entry_v1(&geometry, geometry.chunk_count()), None, "past the end is None");
    }

    #[test]
    fn a_row_reads_only_out_of_a_canonically_sized_chunk() {
        let profile = rc_profile();
        let geometry = integer_kv_state_geometry_v1(&profile, 3).unwrap();
        let entry = integer_kv_state_chunk_entry_v1(&geometry, 0).unwrap();
        let mut bytes = vec![0u8; entry.byte_len() as usize];
        bytes[entry.row_bytes as usize] = 0xAB; // first byte of position 1

        assert_eq!(integer_kv_state_row_v1(&entry, &bytes, 1).unwrap()[0], 0xAB);
        assert_eq!(integer_kv_state_row_v1(&entry, &bytes, 3), None, "past the entry's run");
        bytes.push(0);
        assert_eq!(integer_kv_state_row_v1(&entry, &bytes, 1), None, "a chunk of the wrong length serves nothing");
    }

    #[test]
    fn positions_are_the_prefill_plus_the_calls_that_have_run() {
        let ctx = crate::palw_base0_profile::rc_job_context(&rc_profile(), 8, 4);
        assert_eq!(integer_kv_positions_at_v1(&ctx, 0), 8, "before any decode call the cache is the prefill");
        assert_eq!(integer_kv_positions_at_v1(&ctx, 3), 11);
        // The whole job: `exact_decode_tokens - 1` calls, which is what the KV aux series counts.
        assert_eq!(
            integer_kv_positions_at_v1(&ctx, ctx.exact_decode_tokens - 1),
            ctx.declared_prefill_tokens + ctx.exact_decode_tokens - 1
        );
    }

    /// The two identities over one set of bytes are minted from one descriptor and separated only
    /// by their domains — so they cannot drift, and they cannot collide either.
    #[test]
    fn the_layout_and_the_chunk_map_share_a_descriptor_and_not_a_domain() {
        assert_ne!(integer_kv_state_layout_id_v1(), integer_kv_state_chunk_map_id_v1(), "distinct domains, distinct ids");
        assert_ne!(integer_kv_state_layout_id_v1(), Hash64::default());
        let profile = integer_kv_checkpoint_profile_v1(8);
        assert!(profile.validate_shape().is_ok());
        assert_eq!(profile.checkpoint_interval, 8);
        assert_eq!(profile.state_layout_id, integer_kv_state_layout_id_v1());
        // The interval is the only thing a producer chooses; the layout moves with neither it nor
        // the producer.
        assert_eq!(integer_kv_checkpoint_profile_v1(1).state_layout_id, profile.state_layout_id);
        assert_ne!(integer_kv_checkpoint_profile_v1(1).profile_hash(), profile.profile_hash());
    }

    #[test]
    fn a_float_lane_profile_is_refused_by_name() {
        let mut profile = rc_profile();
        profile.lane = PalwStepLaneV1::Float32;
        assert_eq!(
            integer_kv_state_geometry_v1(&profile, 8),
            Err(PalwStateChunkMapError::NotTheIntegerFamily { lane: PalwStepLaneV1::Float32 })
        );
    }

    #[test]
    fn a_profile_with_no_attention_layer_has_no_state_to_chunk() {
        let mut profile = rc_profile();
        // `full_attention_interval` past the layer count makes every layer a GDN layer.
        profile.full_attention_interval = profile.layer_count + 1;
        assert_eq!(integer_kv_state_geometry_v1(&profile, 8), Err(PalwStateChunkMapError::NoAttentionLayers));
    }

    #[test]
    fn a_zero_length_state_is_refused_rather_than_mapped_empty() {
        assert_eq!(integer_kv_state_geometry_v1(&rc_profile(), 0), Err(PalwStateChunkMapError::ZeroPositions));
    }

    /// The Qwen3.6 recurrence geometry, which is the one every figure below is measured on.
    fn hybrid_profile() -> PalwShapeProfileV3 {
        crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
            crate::palw_qwen36_profile::QWEN36_35B_A3B,
        ))
        .expect("the pinned hybrid geometry projects")
    }

    /// **What v2 changes, in the two numbers that decided it.**
    ///
    /// The delta half is untouched — it was already head-sliced — and the convolution half stops
    /// carrying thirty-one heads a court will not read. Both figures are computed from the profile
    /// rather than recited, so a geometry change moves them and a reader can check the arithmetic.
    #[test]
    fn the_v2_map_head_slices_the_convolution_and_leaves_the_delta_alone() {
        let p = hybrid_profile();
        let (k, v, heads, kernel) = (p.gdn_head_k_dim as u64, p.gdn_head_v_dim as u64, p.gdn_heads as u64, p.gdn_conv_kernel as u64);
        assert_eq!((k, v, heads, kernel), (128, 128, 32, 4), "the hybrid's recurrence geometry moved — re-read the figures below");

        let delta = gdn_delta_head_slice_bytes_v1(&p).expect("a delta slice");
        assert_eq!(delta, v * k * 4);
        assert_eq!(delta, 65_536);

        // v1: one row spans every head, and the window is four of them.
        assert_eq!(gdn_conv_window_bytes_v1(&p).expect("a v1 window"), kernel * (2 * k + v) * heads * 4);
        assert_eq!(gdn_conv_window_bytes_v1(&p).expect("a v1 window"), 196_608);
        // v2: one row is one head's channels.
        assert_eq!(gdn_conv_head_slice_bytes_v2(&p).expect("a v2 window"), kernel * (2 * k + v) * 4);
        assert_eq!(gdn_conv_head_slice_bytes_v2(&p).expect("a v2 window"), 6_144);
        // The whole recurrence opening, which is what the carrier has to hold.
        assert_eq!(gdn_state_row_bytes_v1(&p).expect("v1"), 262_144);
        assert_eq!(gdn_state_row_bytes_v2(&p).expect("v2"), 71_680);
        // v2 covers exactly `heads` times fewer conv bytes and the SAME total state: it is a
        // re-ordering, not a narrowing. A map that dropped bytes would restore a state the
        // producer never held.
        assert_eq!(gdn_conv_head_slice_bytes_v2(&p).expect("v2") * heads, gdn_conv_window_bytes_v1(&p).expect("v1"));
    }

    /// **The map a class registered is the map its opening is priced under**, and a class that
    /// registers neither gets no answer rather than a cheap one.
    #[test]
    fn the_recurrence_terms_are_read_off_the_registered_map() {
        let mut p = hybrid_profile();
        assert_eq!(p.state_chunk_map_id, Hash64::default(), "the shipped hybrid registers a map after all");
        assert!(gdn_state_terms_for_map_v1(&p).is_none(), "an unmapped class was priced as if it had an anchor");

        for (id, want_conv) in [
            (gdn_state_chunk_map_id_v1(), 196_608u64),
            (hybrid_state_chunk_map_id_v1(), 196_608),
            (gdn_state_chunk_map_id_v2(), 6_144),
            (hybrid_state_chunk_map_id_v2(), 6_144),
        ] {
            p.state_chunk_map_id = id;
            let (delta, conv) = gdn_state_terms_for_map_v1(&p).expect("a mapped class is priced");
            assert_eq!(delta, 65_536, "the delta half is the same on every map");
            assert_eq!(conv, want_conv);
            assert_eq!(gdn_state_row_bytes_for_map_v1(&p), Some(delta + conv));
        }
        // The KV maps are not recurrence maps: a class that registers one of them has no anchored
        // recurrence and must not be priced as though it did.
        for id in [integer_kv_state_chunk_map_id_v1(), integer_kv_state_chunk_map_id_v2()] {
            p.state_chunk_map_id = id;
            assert!(gdn_state_terms_for_map_v1(&p).is_none(), "a KV-mapped class was given a recurrence price");
        }
    }

    /// **Six maps, six ids.** The two cache widths, the two recurrence enumerations, and the two
    /// compositions. A collision would make one class's evidence readable as another's — a v1
    /// capture opened head-major, which restores a state nobody folded.
    #[test]
    fn every_state_chunk_map_has_its_own_identity() {
        let ids = [
            ("integer-kv v1", integer_kv_state_chunk_map_id_v1()),
            ("integer-kv v2", integer_kv_state_chunk_map_id_v2()),
            ("gdn v1", gdn_state_chunk_map_id_v1()),
            ("gdn v2", gdn_state_chunk_map_id_v2()),
            ("hybrid v1", hybrid_state_chunk_map_id_v1()),
            ("hybrid v2", hybrid_state_chunk_map_id_v2()),
        ];
        let unique: HashSet<_> = ids.iter().map(|(_, id)| *id).collect();
        assert_eq!(unique.len(), ids.len(), "two state chunk maps share an id");
        for (name, id) in ids {
            assert_ne!(id, Hash64::default(), "{name}: the unregistered sentinel is not a map id");
        }
        // The v1 spellings are FROZEN: every capture ever taken under one is opened by its id.
        assert_eq!(
            PALW_GDN_STATE_CHUNK_MAP_NAME_V1,
            "palw-gdn-state/i32-le/kind-major(delta,conv)/layer-asc/head-asc/\
             row-asc/delta-row=gdn_head_k_dim*4/conv-row=(2*gdn_head_k_dim+gdn_head_v_dim)*gdn_heads*4/chunk<=2^20/v1",
            "the v1 recurrence layout was respelled — every capture taken under the old string is now unopenable"
        );
        // And each composition is spelled as its parts, so it cannot drift from either half.
        let v1 = palw_hybrid_state_chunk_map_name_v1();
        assert!(v1.contains(PALW_GDN_STATE_CHUNK_MAP_NAME_V1) && v1.contains(PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V2));
        let v2 = palw_hybrid_state_chunk_map_name_v2();
        assert!(v2.contains(PALW_GDN_STATE_CHUNK_MAP_NAME_V2) && v2.contains(PALW_INTEGER_KV_STATE_CHUNK_MAP_NAME_V2));
        assert!(!v2.contains(PALW_GDN_STATE_CHUNK_MAP_NAME_V1), "the v2 composition carries the v1 recurrence half");
    }

    #[test]
    fn a_row_wider_than_a_chunk_is_refused() {
        let mut profile = rc_profile();
        profile.attn_head_dim = PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u32;
        let err = integer_kv_state_geometry_v1(&profile, 8).unwrap_err();
        assert!(matches!(err, PalwStateChunkMapError::RowExceedsChunk { .. }), "{err:?}");
    }
}
