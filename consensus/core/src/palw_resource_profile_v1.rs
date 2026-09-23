//! **The resource profile of a class: what a ROLE needs in order to execute it, derived from the
//! class's registered profile and the job — never measured, never a chain fact** (ADR-0151
//! follow-up, item 1).
//!
//! # The confusion this module ends
//!
//! The item 6 acceptance run priced a 2M seat's replay at the artifact's FILE SIZE plus 512 MiB
//! (3.17 GiB) and the attempt then allocated ~16 GiB of K/V rows in its first minute and was
//! OOM-killed. The file size is a fact about the weights; the working set is a fact about the
//! CONTEXT — `layers × 2 × kv_dim × positions × bytes-per-element` — and at 262,143 positions the
//! second is five times the first. Nothing in the producer's path had asked the second question,
//! the panel's pre-check asked the first, and the court asked neither. Three roles, three
//! estimates, none of them the working set.
//!
//! So this module is the ONE derivation. Every figure here is a pure function of
//! `(class profile, job, runtime profile, role, node-local limits)`:
//!
//! * the class profile and the job are chain facts — the same ones a seat replays with;
//! * the [runtime profile](PalwRuntimeProfileV1) is the node-local REPRESENTATION of the cache
//!   (`A16-KV-i32`, `A16-KV-i16`), named and versioned, reported in telemetry, never registered:
//!   the same canonical execution under any of them commits the same rows and the same roots;
//! * the [role](PalwResourceRoleV1) is what the node is about to do — produce, replay a whole job
//!   for a verdict, or resume one V2 segment;
//! * the [limits](PalwRuntimeLimitsV1) are this process's thread count and prefill run width, which
//!   size the kernels' scratch and are the only inputs a node chooses.
//!
//! Nothing measured at runtime feeds back in. A gate compares this figure with what the host has
//! (`replay_memory_budget_v1`, the reservation ledger); the figure itself does not move with the
//! host, which is what makes two nodes agree about what a role costs.
//!
//! # What is separated from what
//!
//! * **Capacity is not capability.** Whether this host can hold a role is decided here and in the
//!   node; whether the class is admissible, what it earns and what it must lock are decided by the
//!   chain from the class's declared and derived work. `the_economic_derivations_do_not_read_the_
//!   resource_profile` holds the wall: no economic module names anything in this one.
//! * **The working set is not the artifact.** The figures are per role and per context; the
//!   artifact's bytes are a separate term the caller adds (`holding_replay_bytes_v1`), because a
//!   mapped artifact is paged by the kernel and a decoded one is resident — a fact about the
//!   holding, not about the class.
//! * **The representation is not the class.** `A16-KV-i16` halves `kv_resident_bytes` and changes
//!   no committed byte: every element the cache holds is an A16 code (`±32,767`) by construction,
//!   so an `i16` is a lossless repack and the map's `i32` little-endian rows regenerate exactly.
//!   Measured, not assumed, on the shipped dense row (2026-09-23, 511 positions, 28 layers):
//!   `min −19,149, max +22,166`, 100 % of 7,325,696 elements inside `±32,767`, 10.6 % inside
//!   `±127`, and 54 of 114,464 64-code blocks packable at one byte. That last pair is why there
//!   is no `A16-KV-i8`: an `i8` cache is a QUANTIZATION of 89 % of the elements, attention would
//!   reduce over different codes, the committed rows and the execution root would move — a
//!   different class, not a runtime profile — and a lossless block-packed form would save 0.05 %.
//!   The name is reserved and refused, with those numbers as the reason.

use crate::palw_context_ladder::{PalwCheckpointCadenceV1, palw_absolute_position_v1, palw_checkpoint_cadence_v1};
use crate::palw_segment_resume_v1::palw_segment_resume_window_v1;
use crate::palw_state_chunk_map::{PALW_ATTN_HISTORY_TILE_V4, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1, palw_profile_is_held_v4};
use crate::palw_step::{
    PalwLayerKindV1, PalwShapeProfileV3, PalwStepCoordinateV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1,
    canonical_step_coordinates, canonical_step_leaf_index,
};
use crate::palw_v2::PalwJobContextV2;
use crate::palw_verification_v2::palw_segment_count_v2;

/// **What a role's capture RETAINS while it runs** — the term that killed the 2M producer twice
/// (2026-09-23, host 5.104.81.23, `dmesg`: `total-vm:1702360980kB` on a 23 GiB host) and that no
/// figure named. The attempt lane's dense sink keeps every tile of every position and a leaf-hash
/// vector of the whole step space: `64 B × 2^34.6` leaves is 1.70 TB of address space, touched a
/// position at a time as the prefill writes it, beside ~10 MB of tiles a position — 9 GB a minute
/// at 16 positions a second, whatever width the K/V cache was held at. The fold keeps one hash per
/// `2^retain_level` leaves and a block of `2^retain_level` in flight. A replaying seat keeps no
/// capture, but the S1 replay keeps `(index, hash)` for every leaf of the window it walks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCaptureRetentionV1 {
    /// Every tile and every leaf hash — the attempt lane's sink for a class outside the held regime.
    DenseTiles { tile_len: u32 },
    /// One hash per `2^retain_level` leaves (ADR-0082 Decision 7) — the free-prompt lane's sink,
    /// the verdict replay's, and a held class's attempt lane's.
    Fold { retain_level: u32 },
    /// No capture: an S1 segment replay hashes its window and keeps the hashes.
    ReplayHashes,
}

/// **Which sink a class's ATTEMPT lane captures with**: the fold for a class under the held regime
/// — whose step space at any real job is terabytes of tiles by construction and whose court is the
/// streamed one (ADR-0103, ADR-0121) — and the dense tiles for every other. A class fact, read off
/// the profile, so the producer, the seat that prices it and the court cannot disagree about what
/// an attempt retains.
pub fn palw_attempt_capture_folds_v1(profile: &PalwShapeProfileV3) -> bool {
    palw_profile_is_held_v4(profile)
}

/// The widest tile the graph declares — the dense sink's per-tile payload is `4 × tile_len`.
pub fn palw_profile_max_tile_len_v1(profile: &PalwShapeProfileV3) -> u32 {
    profile.pre_nodes.iter().chain(&profile.attn_nodes).chain(&profile.post_nodes).map(|n| n.tile_len).max().unwrap_or(0)
}

/// Bytes the dense sink holds for `leaf_count` leaves at `tile_len`: the leaf-hash vector (64 B a
/// leaf, allocated whole) and one tile a leaf — `4 × tile_len` of values plus the tile object (its
/// coordinate, its count, its version and the vector's own header: 56 B).
pub fn palw_dense_capture_bytes_v1(leaf_count: u64, tile_len: u32) -> u64 {
    const TILE_OBJECT_BYTES: u64 = 56;
    leaf_count.saturating_mul(64u64.saturating_add(4u64.saturating_mul(u64::from(tile_len))).saturating_add(TILE_OBJECT_BYTES))
}

/// Bytes the fold holds: the retained vector (one hash per `2^level` leaves, at up to twice its
/// length because it grows by doubling past its `2^20` initial capacity) and the block in flight.
pub fn palw_fold_capture_bytes_v1(leaf_count: u64, retain_level: u32) -> u64 {
    let block = 1u64 << retain_level.min(40);
    let retained = leaf_count.div_ceil(block.max(1));
    retained.saturating_mul(64).saturating_mul(2).saturating_add(block.saturating_mul(64))
}

/// Bytes an S1 replay keeps for the `leaves` it walks: `(u64, Hash64)` a leaf.
pub fn palw_replay_hashes_bytes_v1(leaves: u64) -> u64 {
    leaves.saturating_mul(72)
}

/// **Bytes per element of the COMMITTED cache** — the map's `i32` little-endian row
/// (`palw_state_chunk_map`: `row = attn_kv_heads × attn_head_dim × 4`). A checkpoint, an opening
/// and a retained chunk are this wide whatever the runtime holds the cache in; a runtime profile
/// changes only the resident term.
pub const PALW_KV_COMMITTED_BYTES_PER_ELEMENT_V1: u64 = 4;

/// **The runtime representation of the attention cache** — a node-local choice with a name, so a
/// log line, a telemetry field and a test can say which one a node ran. Not a chain fact: the
/// class id, the artifact root and the work derivation do not read it, and a node may change it
/// without re-registering anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PalwRuntimeProfileV1 {
    /// `A16-KV-i32`: one `i32` lane per code — the lane the catalog ops read, and the oracle every
    /// other representation is proven against.
    A16KvI32,
    /// `A16-KV-i16`: one `i16` per code — the code's own width. Lossless (see the module doc), and
    /// half of `A16-KV-i32`'s resident bytes.
    A16KvI16,
}

impl PalwRuntimeProfileV1 {
    pub const ALL: [PalwRuntimeProfileV1; 2] = [PalwRuntimeProfileV1::A16KvI32, PalwRuntimeProfileV1::A16KvI16];

    /// The name a log, a telemetry field or an operator's flag spells.
    pub const fn name(self) -> &'static str {
        match self {
            PalwRuntimeProfileV1::A16KvI32 => "A16-KV-i32",
            PalwRuntimeProfileV1::A16KvI16 => "A16-KV-i16",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// Bytes one cached code occupies under this representation.
    pub const fn kv_bytes_per_element(self) -> u64 {
        match self {
            PalwRuntimeProfileV1::A16KvI32 => 4,
            PalwRuntimeProfileV1::A16KvI16 => 2,
        }
    }
}

/// **What the node is about to do with the class.** The three roles the acceptance ladder found
/// priced by three different estimates, now priced by one function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PalwResourceRoleV1 {
    /// One attempt: the canonical job executed and captured, the cache held to the last position.
    Producer,
    /// A whole-job replay for a verdict (`execute_for_verdict`): the same rows the producer held.
    FullSeat,
    /// One V2 segment of `seat_count − 1` (ADR-0133 S1): rows before the segment come from the
    /// checkpoint at its start, the segment's own positions are re-executed.
    PartialSeat { seat_count: u16, segment_index: u16 },
}

impl PalwResourceRoleV1 {
    pub const fn name(self) -> &'static str {
        match self {
            PalwResourceRoleV1::Producer => "producer",
            PalwResourceRoleV1::FullSeat => "full-seat",
            PalwResourceRoleV1::PartialSeat { .. } => "partial-seat",
        }
    }
}

/// **The two node-local numbers the scratch terms are sized by.** `threads` is this process's
/// pool (each thread of the fused attention kernel keeps two history-long rows of scratch);
/// `prefill_run_positions` is how many prompt positions the dense engine walks through a layer at
/// once, whose committed traces are held together until captured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRuntimeLimitsV1 {
    pub threads: u32,
    pub prefill_run_positions: u32,
}

/// **What one role needs, term by term.** Every field is derived; `working_set_bytes` is the sum a
/// gate or a ledger reserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwResourceProfileV1 {
    pub runtime: PalwRuntimeProfileV1,
    pub role: PalwResourceRoleV1,
    /// Layers that hold a K/V history (a hybrid's recurrence layers hold none).
    pub attention_layers: u64,
    /// `attn_kv_heads × attn_head_dim`, codes per cached row.
    pub kv_dim: u64,
    pub heads: u64,
    /// Rows `[0, resume_rows)` are LOADED from a checkpoint; `[resume_rows, end_rows)` are
    /// re-executed. Zero for a role that starts at the prompt.
    pub resume_rows: u64,
    /// Rows of every attention layer's K and V resident when the role finishes — the widest the
    /// history gets. Dense attention reads every row before a position, so no role can hold fewer
    /// than this while it runs: a partial seat's minimum state is the whole prefix up to its
    /// segment's end, not its segment.
    pub end_rows: u64,
    /// The cache at `end_rows` under the runtime profile.
    pub kv_resident_bytes: u64,
    /// One checkpoint of the state at `end_rows` as the map commits it (`i32` LE): what a resume
    /// from this role's last row would have to carry.
    pub checkpoint_bytes: u64,
    /// The served opening a resuming role decodes from — the checkpoint at `resume_rows`, at the
    /// COMMITTED width — held beside the cache while it is decoded. Zero for a genesis start.
    pub opening_bytes: u64,
    /// Checkpoint chunks the capture RETAINS while the role runs. Under the per-decode-call cadence
    /// the capture keeps every checkpoint's bytes (a state that is not prefix-stable in general has
    /// no other copy), which is quadratic in the decode length; under the per-position cadence it
    /// keeps none and re-derives them from the cache. A replaying partial seat captures no leg.
    pub retained_checkpoint_bytes: u64,
    /// The fused attention kernel's per-thread scratch at the widest history: `threads × 2 rows ×
    /// heads × end_rows × 4`. Zero for a graph without a fused site — its context-wide rows are
    /// committed trace rows and priced in `trace_scratch_bytes`.
    pub attention_scratch_bytes: u64,
    /// The committed traces of one prefill run, held until captured: `prefill_run_positions × one
    /// position's committed trace at the widest history × [`PALW_PREFILL_RUN_COPIES_V1`]`.
    pub trace_scratch_bytes: u64,
    /// What the role's capture sink retains ([`PalwCaptureRetentionV1`]): the whole dense capture,
    /// the fold's hashes, or a replay's leaf hashes. The term the 2M producer died of.
    pub capture: PalwCaptureRetentionV1,
    pub capture_retained_bytes: u64,
    /// The checkpoint LEG's own retention beside the chunks: one leaf and one hash per checkpoint,
    /// and — under the per-position cadence — one hash per chunk of the previous push and the held
    /// map's frontiers.
    pub checkpoint_leg_bytes: u64,
    /// Step leaves the role hashes (its capture's, or the window it replays).
    pub leaves: u64,
}

/// **How many copies of a prefill run's rows are live at once** — measured, not chosen. The
/// one-pass walk holds the run's rows per node (`rows`), moves them into the traces, and the
/// capture then converts each trace into captured rows (`a16_captured_rows_v1`, a copy) and tiles
/// them (a copy per tile in flight): three whole copies of the run's committed rows plus the tile
/// in flight. Pinned to what the 2026-09-23 acceptance run's brackets show over `anon − (cache +
/// capture + checkpoint leg + baseline)`, in `the_2m_attempt_by_role_and_runtime_profile`.
pub const PALW_PREFILL_RUN_COPIES_V1: u64 = 3;

impl PalwResourceProfileV1 {
    /// Rows this role re-executes.
    pub fn replayed_rows(&self) -> u64 {
        self.end_rows.saturating_sub(self.resume_rows)
    }

    /// **The figure a gate compares and a ledger reserves**: everything the role holds at its peak.
    pub fn working_set_bytes(&self) -> u64 {
        self.kv_resident_bytes
            .saturating_add(self.opening_bytes)
            .saturating_add(self.retained_checkpoint_bytes)
            .saturating_add(self.attention_scratch_bytes)
            .saturating_add(self.trace_scratch_bytes)
            .saturating_add(self.capture_retained_bytes)
            .saturating_add(self.checkpoint_leg_bytes)
    }

    /// The same role started at the prompt instead of at its checkpoint: no opening to hold, every
    /// row re-executed. What a partial seat pays when the opening it is served carries no chunks
    /// (a folded retention cannot serve one), and what it holds either way is `end_rows`.
    pub fn from_genesis(self) -> Self {
        Self { resume_rows: 0, opening_bytes: 0, ..self }
    }
}

/// Bytes of the K and V series of `attention_layers` layers at `rows` rows, `bytes_per_element`
/// each. Saturating: this is priced from gossiped counts.
pub fn palw_kv_series_bytes_v1(attention_layers: u64, kv_dim: u64, rows: u64, bytes_per_element: u64) -> u64 {
    attention_layers.saturating_mul(2).saturating_mul(kv_dim).saturating_mul(rows).saturating_mul(bytes_per_element)
}

/// The layers of `profile` that hold a K/V history.
pub fn palw_attention_layers_v1(profile: &PalwShapeProfileV3) -> u64 {
    (0..profile.layer_count).filter(|&l| profile.layer_kind(l) == PalwLayerKindV1::Attention).count() as u64
}

/// **Bytes of one position's committed trace under `profile`**, counted the way the dense
/// interpreter spends them: one `i32` row per declared node, the layer table once per layer, and
/// `max_kv_len` standing in for a kv-scaled row's longest form. The ONE spelling of this number —
/// the engine's memory ceiling (`misaka-palw-base0::engine_a16::interpreted_trace_bytes_v1`) and
/// the trace-scratch term below both read it, so the ceiling and the profile cannot disagree
/// about what a token's trace costs. Saturating: called on adversarial input.
pub fn palw_committed_trace_bytes_v1(profile: &PalwShapeProfileV3, max_kv_len: u64) -> u64 {
    let row_elems = |node: &PalwStepNodeV1| -> u64 {
        match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => elements as u64,
            PalwStepOutLenV1::KvScaled { multiplier } => (multiplier as u64).saturating_mul(max_kv_len),
        }
    };
    let table = |nodes: &[PalwStepNodeV1]| -> u64 { nodes.iter().fold(0u64, |acc, n| acc.saturating_add(row_elems(n))) };
    let elems = table(&profile.pre_nodes)
        .saturating_add(table(&profile.attn_nodes).saturating_mul(profile.layer_count as u64))
        .saturating_add(table(&profile.post_nodes));
    elems.saturating_mul(std::mem::size_of::<i32>() as u64)
}

/// Whether the class's attention site is the fused one (ADR-0082): its context-wide rows live in
/// kernel scratch rather than in the trace.
pub fn palw_profile_has_fused_site_v1(profile: &PalwShapeProfileV3) -> bool {
    profile.attn_nodes.iter().any(|n| n.op_kind == PalwStepOpKindV1::AttnFused)
}

/// Rows the cache holds when the whole job has run: the prefill, then one row per decode call
/// after the first (the last call's logits select a token nobody forwards).
pub fn palw_job_end_rows_v1(job: &PalwJobContextV2) -> u64 {
    u64::from(job.declared_prefill_tokens).saturating_add(u64::from(job.exact_decode_tokens.saturating_sub(1)))
}

/// **The rows a role loads and the rows it ends at**: `(resume_rows, end_rows)`. `None` for a
/// segment the cut does not name or whose leaves are not canonical steps of the job.
///
/// The resume rule is the class's checkpoint cadence, read once here:
///
/// * per position (a tiled or held map): a checkpoint exists after every position, so a segment
///   whose first leaf is at absolute position `a` resumes from the one covering `[0, a)`;
/// * per decode call: checkpoints exist at call boundaries only, so a segment starting inside the
///   prefill starts at the prompt, and one starting at decode call `f ≥ 1` resumes from the last
///   checkpoint at a boundary below `f` — none when that boundary is the prefill itself.
pub fn palw_role_rows_v1(
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    leaf_count: u64,
    role: PalwResourceRoleV1,
) -> Option<(u64, u64)> {
    match role {
        PalwResourceRoleV1::Producer | PalwResourceRoleV1::FullSeat => Some((0, palw_job_end_rows_v1(job))),
        PalwResourceRoleV1::PartialSeat { seat_count, segment_index } => {
            let k = palw_segment_count_v2(seat_count);
            let window = palw_segment_resume_window_v1(profile, job, leaf_count, k, segment_index)?;
            if window.is_empty() {
                return Some((0, 0));
            }
            let first = canonical_step_coordinates(profile, job, window.leaf_start)?;
            let last = canonical_step_coordinates(profile, job, window.leaf_end - 1)?;
            let start = u64::from(palw_absolute_position_v1(job, first.call_index, first.position)?);
            let end = u64::from(palw_absolute_position_v1(job, last.call_index, last.position)?).saturating_add(1);
            let resume = match palw_checkpoint_cadence_v1(profile) {
                PalwCheckpointCadenceV1::PerPosition => start,
                PalwCheckpointCadenceV1::PerDecodeCall => {
                    if first.call_index == 0 {
                        0
                    } else {
                        let interval = u64::from(PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1.max(1));
                        let covered = (u64::from(first.call_index) - 1) / interval * interval;
                        if covered == 0 { 0 } else { u64::from(job.declared_prefill_tokens).saturating_add(covered) }
                    }
                }
            };
            Some((resume, end))
        }
    }
}

/// **The step leaves a role hashes**: the whole job's for a producer or a full seat; for a partial
/// seat, the leaves from its resume point (the first leaf of the position after the checkpoint's
/// rows, or leaf 0 at the prompt) to its segment's last leaf — what `replay_segment_opening_v1`
/// keeps an `(index, hash)` for.
pub fn palw_role_leaves_v1(profile: &PalwShapeProfileV3, job: &PalwJobContextV2, leaf_count: u64, role: PalwResourceRoleV1) -> Option<u64> {
    match role {
        PalwResourceRoleV1::Producer | PalwResourceRoleV1::FullSeat => Some(leaf_count),
        PalwResourceRoleV1::PartialSeat { seat_count, segment_index } => {
            let k = palw_segment_count_v2(seat_count);
            let window = palw_segment_resume_window_v1(profile, job, leaf_count, k, segment_index)?;
            if window.is_empty() {
                return Some(0);
            }
            let (resume_rows, _) = palw_role_rows_v1(profile, job, leaf_count, role)?;
            let first = if resume_rows == 0 {
                0
            } else {
                let prefill = u64::from(job.declared_prefill_tokens);
                let (call_index, position) = if resume_rows < prefill {
                    (0u32, u32::try_from(resume_rows).ok()?)
                } else {
                    (u32::try_from(resume_rows - prefill + 1).ok()?, 0u32)
                };
                canonical_step_leaf_index(profile, job, &PalwStepCoordinateV1 { call_index, node_slot: 0, position, tile_index: 0 })?
            };
            Some(window.leaf_end.saturating_sub(first))
        }
    }
}

/// **The resource profile of `role` on `job` under `runtime`** — the derivation, whole.
/// `leaf_count` is the job's step-leaf count: the dense sink's vector is sized by it, the fold's
/// retained set derives from it, and a partial seat's segment is cut over it.
pub fn palw_resource_profile_v1(
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    leaf_count: u64,
    runtime: PalwRuntimeProfileV1,
    role: PalwResourceRoleV1,
    limits: PalwRuntimeLimitsV1,
    capture: PalwCaptureRetentionV1,
) -> Option<PalwResourceProfileV1> {
    let attention_layers = palw_attention_layers_v1(profile);
    let kv_dim = u64::from(profile.attn_kv_heads).saturating_mul(u64::from(profile.attn_head_dim));
    let heads = u64::from(profile.attn_heads);
    let (resume_rows, end_rows) = palw_role_rows_v1(profile, job, leaf_count, role)?;
    let leaves = palw_role_leaves_v1(profile, job, leaf_count, role)?;
    let capture_retained_bytes = match capture {
        PalwCaptureRetentionV1::DenseTiles { tile_len } => palw_dense_capture_bytes_v1(leaves, tile_len),
        PalwCaptureRetentionV1::Fold { retain_level } => palw_fold_capture_bytes_v1(leaves, retain_level),
        PalwCaptureRetentionV1::ReplayHashes => palw_replay_hashes_bytes_v1(leaves),
    };
    let committed = PALW_KV_COMMITTED_BYTES_PER_ELEMENT_V1;
    // The leg's own retention: a leaf (its borsh object, ~144 B) and a hash per checkpoint, and
    // under the per-position cadence a hash per chunk of the previous push plus the held frontiers
    // (one open node and one closed peak per level per slice — bounded by the tree's depth).
    let checkpoint_leg_bytes = match (role, palw_checkpoint_cadence_v1(profile)) {
        (PalwResourceRoleV1::PartialSeat { .. }, _) => 0,
        (_, PalwCheckpointCadenceV1::PerPosition) => {
            let slices = attention_layers.saturating_mul(2);
            let chunks_per_slice = end_rows.div_ceil(u64::from(PALW_ATTN_HISTORY_TILE_V4).max(1));
            end_rows
                .saturating_mul(144 + 64)
                .saturating_add(slices.saturating_mul(chunks_per_slice).saturating_mul(64))
                .saturating_add(slices.saturating_mul(64 * 72))
        }
        (_, PalwCheckpointCadenceV1::PerDecodeCall) => {
            let interval = u64::from(PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1.max(1));
            (u64::from(job.exact_decode_tokens.saturating_sub(1)) / interval).saturating_mul(144 + 64)
        }
    };
    let retained_checkpoint_bytes = match (role, palw_checkpoint_cadence_v1(profile)) {
        (PalwResourceRoleV1::PartialSeat { .. }, _) | (_, PalwCheckpointCadenceV1::PerPosition) => 0,
        (_, PalwCheckpointCadenceV1::PerDecodeCall) => {
            // Checkpoints after calls `interval, 2·interval, …` up to the last call that runs:
            // `m` of them, the `j`-th holding `prefill + j·interval` rows.
            let interval = u64::from(PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1.max(1));
            let calls = u64::from(job.exact_decode_tokens.saturating_sub(1));
            let m = calls / interval;
            let rows = m
                .saturating_mul(u64::from(job.declared_prefill_tokens))
                .saturating_add(interval.saturating_mul(m).saturating_mul(m.saturating_add(1)) / 2);
            palw_kv_series_bytes_v1(attention_layers, kv_dim, rows, committed)
        }
    };
    let attention_scratch_bytes = if palw_profile_has_fused_site_v1(profile) {
        u64::from(limits.threads).saturating_mul(2).saturating_mul(heads).saturating_mul(end_rows).saturating_mul(4)
    } else {
        0
    };
    let trace_scratch_bytes = u64::from(limits.prefill_run_positions.max(1))
        .saturating_mul(palw_committed_trace_bytes_v1(profile, end_rows))
        .saturating_mul(PALW_PREFILL_RUN_COPIES_V1);
    Some(PalwResourceProfileV1 {
        runtime,
        role,
        attention_layers,
        kv_dim,
        heads,
        resume_rows,
        end_rows,
        kv_resident_bytes: palw_kv_series_bytes_v1(attention_layers, kv_dim, end_rows, runtime.kv_bytes_per_element()),
        checkpoint_bytes: palw_kv_series_bytes_v1(attention_layers, kv_dim, end_rows, committed),
        opening_bytes: palw_kv_series_bytes_v1(attention_layers, kv_dim, resume_rows, committed),
        retained_checkpoint_bytes,
        attention_scratch_bytes,
        trace_scratch_bytes,
        capture,
        capture_retained_bytes,
        checkpoint_leg_bytes,
        leaves,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;
    use crate::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_graph_v5_profile_v1, qwen25_a16_profile_v2, qwen25_geometry_artifact_eps};
    use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2};

    const GIB: f64 = (1u64 << 30) as f64;
    const LIMITS: PalwRuntimeLimitsV1 = PalwRuntimeLimitsV1 { threads: 12, prefill_run_positions: 64 };
    /// The held class's fold level (`misaka-palw-base0::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1`), spelled
    /// here because consensus core cannot import the engine crate; the backend passes the real one.
    const HELD_RETAIN_LEVEL: u32 = 12;

    /// The capture a role runs with on `profile`, as the dense backend chooses it.
    fn capture_for(profile: &PalwShapeProfileV3, role: PalwResourceRoleV1) -> PalwCaptureRetentionV1 {
        match role {
            PalwResourceRoleV1::Producer if !palw_attempt_capture_folds_v1(profile) => {
                PalwCaptureRetentionV1::DenseTiles { tile_len: palw_profile_max_tile_len_v1(profile) }
            }
            PalwResourceRoleV1::Producer | PalwResourceRoleV1::FullSeat => PalwCaptureRetentionV1::Fold { retain_level: HELD_RETAIN_LEVEL },
            PalwResourceRoleV1::PartialSeat { .. } => PalwCaptureRetentionV1::ReplayHashes,
        }
    }

    fn derive(
        profile: &PalwShapeProfileV3,
        job: &PalwJobContextV2,
        leaves: u64,
        runtime: PalwRuntimeProfileV1,
        role: PalwResourceRoleV1,
    ) -> Option<PalwResourceProfileV1> {
        palw_resource_profile_v1(profile, job, leaves, runtime, role, LIMITS, capture_for(profile, role))
    }

    fn job(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-palw-rc".to_vec(),
            job_id: Hash64::from_u64_word(1),
            job_nullifier: Hash64::from_u64_word(2),
            assignment_id: Hash64::default(),
            execution_seed: [0; 32],
            model_profile_id: Hash64::from_u64_word(3),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::from_u64_word(3),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::default(),
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::default(),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: profile.n_ctx,
        }
    }

    /// The shipped 2M held row, and the attempt job the fleet produced with (262,143 positions and
    /// the one decode of a prefill draw).
    fn two_m() -> (PalwShapeProfileV3, PalwJobContextV2) {
        let g = qwen25_geometry_artifact_eps(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B });
        let profile = qwen25_a16_artifact_row_profile_v7(g).expect("the 2M row projects");
        let job = job(&profile, 262_143, 1);
        (profile, job)
    }

    fn leaf_count(profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> u64 {
        crate::palw_step::step_leaf_count_capped_v1(profile, job, crate::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1)
            .expect("the job's step space")
    }

    /// **The 2M attempt, by role and by runtime profile — the numbers the acceptance run did not
    /// have.** 28 layers × 2 × 256 codes × 262,143 rows: 14.00 GiB of `i32`, 7.00 GiB of `i16`;
    /// the committed checkpoint of the same state is the `i32` figure whatever the runtime holds.
    #[test]
    fn the_2m_attempt_by_role_and_runtime_profile() {
        let (profile, job) = two_m();
        let leaves = leaf_count(&profile, &job);
        let per_row_i32 = 28u64 * 2 * 256 * 4;
        let rows = 262_143u64;
        let i32_producer =
            derive(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI32, PalwResourceRoleV1::Producer)
                .expect("derives");
        assert_eq!(i32_producer.attention_layers, 28);
        assert_eq!(i32_producer.kv_dim, 256);
        assert_eq!((i32_producer.resume_rows, i32_producer.end_rows), (0, rows), "an attempt holds every prefill row and no decode row");
        assert_eq!(i32_producer.kv_resident_bytes, rows * per_row_i32, "15,032,328,192 bytes of i32 rows");
        assert!((i32_producer.kv_resident_bytes as f64 / GIB - 14.0).abs() < 0.01);
        assert_eq!(i32_producer.checkpoint_bytes, i32_producer.kv_resident_bytes, "the committed width IS i32");
        assert_eq!(i32_producer.opening_bytes, 0);
        assert_eq!(i32_producer.retained_checkpoint_bytes, 0, "a held row's per-position cadence retains no chunk");
        assert!(palw_profile_has_fused_site_v1(&profile), "the 2M row is a fused graph");
        assert_eq!(i32_producer.attention_scratch_bytes, 12 * 2 * 12 * rows * 4, "twelve threads of two history-long rows");
        assert_eq!(i32_producer.trace_scratch_bytes, 64 * 3 * palw_committed_trace_bytes_v1(&profile, rows));


        let i16_producer =
            derive(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI16, PalwResourceRoleV1::Producer)
                .expect("derives");
        assert_eq!(i16_producer.kv_resident_bytes * 2, i32_producer.kv_resident_bytes, "i16 halves the resident term");
        assert_eq!(i16_producer.checkpoint_bytes, i32_producer.checkpoint_bytes, "and moves no committed byte");
        assert_eq!(i16_producer.attention_scratch_bytes, i32_producer.attention_scratch_bytes);

        // **The term that killed the producer, named and pinned to the kill line.** The 2M attempt's step
        // space is 26.6 billion leaves; the dense sink's leaf-hash vector alone is 64 B each — the
        // `total-vm:1702360980kB` `dmesg` printed on 2026-09-23 (1.70 TB; the vector is the whole of it
        // to within the process's other mappings) — and its tiles are 4.4 TB more. The held class's attempt
        // therefore folds, and the fold retains 0.8 GiB at its `2^12` level. Whatever the K/V width.
        assert!(palw_attempt_capture_folds_v1(&profile), "the 2M row is under the held regime, so its attempt folds");
        assert_eq!(leaves, 27_002_845_160, "the 2M attempt is 2^34.65 leaves");
        let leaf_vector_bytes = leaves * 64;
        assert_eq!(leaf_vector_bytes, 1_728_182_090_240, "64 B a leaf: 1.73 TB");
        // `dmesg`, 2026-09-23 06:50:21 on 5.104.81.23: `Killed process 3368025 (kaspad) total-vm:1702360980kB`.
        // The leaf vector is 99.1 % of that address space; the 14 GiB left is the artifact mapping, the
        // node's caches and arenas. The vector is the term, to within a rounding of everything else.
        let dmesg_total_vm_bytes = 1_702_360_980u64 * 1024;
        assert!(leaf_vector_bytes < dmesg_total_vm_bytes);
        assert!((dmesg_total_vm_bytes - leaf_vector_bytes) < 16 * (1u64 << 30), "everything else in the address space was under 16 GiB");
        assert!(leaf_vector_bytes as f64 / dmesg_total_vm_bytes as f64 > 0.99);
        let dense = palw_dense_capture_bytes_v1(leaves, palw_profile_max_tile_len_v1(&profile));
        assert_eq!(palw_profile_max_tile_len_v1(&profile), 128);
        assert!(dense as f64 > 15.0e12, "the dense capture of a 2M attempt is over 15 TB: {dense}");
        assert_eq!(i16_producer.capture, PalwCaptureRetentionV1::Fold { retain_level: HELD_RETAIN_LEVEL });
        assert_eq!(i16_producer.capture_retained_bytes, palw_fold_capture_bytes_v1(leaves, HELD_RETAIN_LEVEL));
        assert!((0.7..0.9).contains(&(i16_producer.capture_retained_bytes as f64 / GIB)), "the fold retains under a GiB");
        assert!(i16_producer.checkpoint_leg_bytes as f64 / GIB < 0.2, "the leg's own retention is small: {}", i16_producer.checkpoint_leg_bytes);
        assert_eq!(i16_producer.leaves, leaves);

        // The full seat replays the same job, so it holds the same rows.
        let full = derive(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI16, PalwResourceRoleV1::FullSeat)
            .expect("derives");
        assert_eq!((full.resume_rows, full.end_rows, full.working_set_bytes()), (0, rows, i16_producer.working_set_bytes()));

        for p in [&i32_producer, &i16_producer] {
            eprintln!(
                "2M attempt under {}: kv {:.2} GiB + attention scratch {:.2} GiB + trace scratch {:.2} GiB + capture {:.2} GiB ({:?}) + \
                 checkpoint leg {:.2} GiB = working set {:.2} GiB (checkpoint {:.2} GiB)",
                p.runtime.name(),
                p.kv_resident_bytes as f64 / GIB,
                p.attention_scratch_bytes as f64 / GIB,
                p.trace_scratch_bytes as f64 / GIB,
                p.capture_retained_bytes as f64 / GIB,
                p.capture,
                p.checkpoint_leg_bytes as f64 / GIB,
                p.working_set_bytes() as f64 / GIB,
                p.checkpoint_bytes as f64 / GIB
            );
        }
        // A 21 GiB share admits the folded i16 attempt; a dense one would never be admitted anywhere.
        assert!(i16_producer.working_set_bytes() < 14 * (1u64 << 30), "the folded i16 attempt fits a stated share: {}", i16_producer.working_set_bytes());
        let dense_attempt = palw_resource_profile_v1(
            &profile,
            &job,
            leaves,
            PalwRuntimeProfileV1::A16KvI16,
            PalwResourceRoleV1::Producer,
            LIMITS,
            PalwCaptureRetentionV1::DenseTiles { tile_len: 128 },
        )
        .expect("derives");
        assert!(dense_attempt.working_set_bytes() > 15 * (1u64 << 40), "priced dense, the same attempt needs over 15 TiB");
        // The old proxy, for the record: the 2.67 GiB artifact plus 512 MiB was 3.17 GiB, which the
        // i32 working set exceeds more than four times over.
        let old_proxy = (2.67 * GIB) as u64 + (512u64 << 20);
        assert!(i32_producer.working_set_bytes() > 4 * old_proxy);
    }

    /// **A partial seat's minimum state is the prefix up to its segment's end, and the opening
    /// that lets it skip re-execution is wider than the rows it skips.** On the 2M attempt cut into
    /// three segments (four seats), segment 1 resumes at about a third of the prefill and ends at
    /// about two thirds: it holds fewer rows than a full seat, but its opening — the checkpoint at
    /// its start, at the committed `i32` width — is as large as the rows it saves under `i16`, so a
    /// resuming partial seat holds MORE than a full one unless it streams the opening. The figures
    /// make that trade visible; they do not hide it in a smaller number.
    #[test]
    fn a_partial_seat_of_the_2m_attempt_holds_the_prefix_to_its_segments_end() {
        let (profile, job) = two_m();
        let leaves = leaf_count(&profile, &job);
        let full = derive(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI16, PalwResourceRoleV1::FullSeat)
            .expect("derives");
        let mut ends = Vec::new();
        for segment in 0..3u16 {
            let role = PalwResourceRoleV1::PartialSeat { seat_count: 4, segment_index: segment };
            let p = derive(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI16, role).expect("derives");
            eprintln!(
                "segment {segment}/3: resume {} end {} rows; kv {:.2} GiB opening {:.2} GiB working set {:.2} GiB (genesis {:.2} GiB)",
                p.resume_rows,
                p.end_rows,
                p.kv_resident_bytes as f64 / GIB,
                p.opening_bytes as f64 / GIB,
                p.working_set_bytes() as f64 / GIB,
                p.from_genesis().working_set_bytes() as f64 / GIB
            );
            assert!(p.end_rows <= full.end_rows);
            assert!(p.resume_rows <= p.end_rows);
            assert_eq!(p.retained_checkpoint_bytes, 0, "a replaying seat captures no leg");
            assert_eq!(p.checkpoint_leg_bytes, 0);
            // **The S1 replay keeps a hash per leaf it walks, and at this width that is the term**:
            // segment 0 walks 8.7 billion leaves from the prompt — 580 GiB of `(index, hash)` — which
            // is why the figure is printed rather than hidden: no seat on this design replays a 2M
            // segment until the replay streams its hashes.
            assert_eq!(p.capture, PalwCaptureRetentionV1::ReplayHashes);
            assert_eq!(p.capture_retained_bytes, p.leaves * 72);
            assert!(p.leaves > 0);
            // The prefix is the state: nothing smaller serves dense attention.
            assert_eq!(p.kv_resident_bytes, palw_kv_series_bytes_v1(28, 256, p.end_rows, 2));
            assert_eq!(p.from_genesis().opening_bytes, 0);
            ends.push(p);
        }
        assert_eq!(ends[0].resume_rows, 0, "segment 0 starts at the prompt");
        assert_eq!(ends[2].end_rows, full.end_rows, "the last segment ends where the job ends");
        assert!(ends[1].resume_rows > 0 && ends[1].resume_rows < ends[1].end_rows, "segment 1 resumes inside the prefill");
        // A checkpoint after every position (the held map): the segment resumes exactly at its first leaf.
        assert_eq!(palw_checkpoint_cadence_v1(&profile), PalwCheckpointCadenceV1::PerPosition);
        assert!(ends[1].end_rows < full.end_rows, "and holds fewer rows than a full seat…");
        // With the replay's hash vector priced, every segment of the 2M attempt is far past any host — the
        // rows and the opening are the smaller terms. Compared without it (the K/V and opening terms alone):
        let without_hashes = |p: &PalwResourceProfileV1| p.working_set_bytes() - p.capture_retained_bytes;
        assert!(
            without_hashes(&ends[1]) > without_hashes(&full) - full.capture_retained_bytes,
            "…but its i32 opening outweighs the i16 rows it saves: {} > {}",
            without_hashes(&ends[1]),
            without_hashes(&full) - full.capture_retained_bytes
        );
        assert!(
            without_hashes(&ends[1].from_genesis()) < without_hashes(&full) - full.capture_retained_bytes,
            "started at the prompt, it is smaller than a full seat"
        );
        assert!(ends[0].working_set_bytes() as f64 / GIB > 500.0, "segment 0's replay hashes alone are hundreds of GiB");
    }

    /// **The per-decode-call cadence retains every checkpoint's bytes, and the derivation says
    /// so** — the graph-v2 512 row (the v2 map) on a 14-prefill, 2-decode job: one checkpoint after
    /// decode call 1, at 15 rows, `i32` wide; a partial seat starting at call 1 resumes from the
    /// prompt (the boundary below call 1 is the prefill, which has no checkpoint).
    #[test]
    fn the_per_call_cadence_prices_its_retained_chunks_and_its_resume_boundaries() {
        let g = PalwQwen25GeometryV1 { n_ctx: 512, ..QWEN25_1_5B };
        let profile = qwen25_a16_profile_v2(g).expect("v2 projects");
        assert_eq!(palw_checkpoint_cadence_v1(&profile), PalwCheckpointCadenceV1::PerDecodeCall);
        let job = job(&profile, 14, 3);
        let leaves = crate::palw_step::step_leaf_count_capped_v1(&profile, &job, crate::palw_step::PALW_STEP_MAX_LEAVES).expect("leaves");
        let p = derive(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI32, PalwResourceRoleV1::Producer)
            .expect("derives");
        assert_eq!(p.end_rows, 16, "14 prefill rows and one per decode call after the first");
        // Checkpoints after calls 1 and 2 (interval 1): 15 and 16 rows of i32.
        assert_eq!(p.retained_checkpoint_bytes, palw_kv_series_bytes_v1(28, 256, 15 + 16, 4));
        assert_eq!(p.attention_scratch_bytes, 0, "a v2 graph has no fused site; its context rows are trace rows");
        assert!(p.trace_scratch_bytes > 0);
        // Every segment of this job that starts at a decode call resumes from a call boundary or the prompt.
        for segment in 0..palw_segment_count_v2(4) {
            let role = PalwResourceRoleV1::PartialSeat { seat_count: 4, segment_index: segment };
            let s = derive(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI32, role).expect("derives");
            assert!(s.resume_rows == 0 || s.resume_rows >= 15, "a per-call resume never lands inside the prefill: {}", s.resume_rows);
            assert_eq!(s.retained_checkpoint_bytes, 0);
        }
    }

    /// The shipped 512 row, at its canonical job, for the record: under a megabyte of cache — which
    /// is why nobody met the working set before the held row.
    #[test]
    fn the_512_row_is_small_and_that_is_why_nobody_asked() {
        let profile = qwen25_a16_graph_v5_profile_v1().expect("the shipped 512 row");
        let job = job(&profile, 14, 2);
        let leaves = crate::palw_step::step_leaf_count_capped_v1(&profile, &job, crate::palw_step::PALW_STEP_MAX_LEAVES).expect("leaves");
        let p = derive(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI32, PalwResourceRoleV1::Producer)
            .expect("derives");
        assert_eq!(p.end_rows, 15);
        assert_eq!(p.kv_resident_bytes, 15 * 28 * 2 * 256 * 4);
        assert!(p.kv_resident_bytes < 1 << 20);
    }

    #[test]
    fn a_runtime_profile_is_named_and_parses_back() {
        for p in PalwRuntimeProfileV1::ALL {
            assert_eq!(PalwRuntimeProfileV1::parse(p.name()), Some(p));
            assert!(p.name().starts_with("A16-KV-"));
        }
        assert_eq!(PalwRuntimeProfileV1::parse("A16-KV-i8"), None, "refused by name, with the measurement in the module doc as the reason");
        assert_eq!(PalwRuntimeProfileV1::A16KvI32.kv_bytes_per_element(), PALW_KV_COMMITTED_BYTES_PER_ELEMENT_V1);
        assert_eq!(PalwRuntimeProfileV1::A16KvI16.kv_bytes_per_element() * 2, PALW_KV_COMMITTED_BYTES_PER_ELEMENT_V1);
    }

    /// **Capacity is not an input to any economic derivation.** The modules that derive work,
    /// quanta, rewards, exposure and the seat lock are scanned for every name this module exports
    /// and for the words a working-set term would arrive under; a hit means a memory figure has
    /// become a price, which is the confusion the module doc names. The needles are split so this
    /// file does not match itself.
    #[test]
    fn the_economic_derivations_do_not_read_the_resource_profile() {
        let sources: [(&str, &str); 10] = [
            ("palw_canonical_work_v1.rs", include_str!("palw_canonical_work_v1.rs")),
            ("palw_execution_quanta_v1.rs", include_str!("palw_execution_quanta_v1.rs")),
            ("palw_reward_v2.rs", include_str!("palw_reward_v2.rs")),
            ("palw_reward_properties_v1.rs", include_str!("palw_reward_properties_v1.rs")),
            ("palw_economic_safety_v1.rs", include_str!("palw_economic_safety_v1.rs")),
            ("palw_exposure.rs", include_str!("palw_exposure.rs")),
            ("palw_economic_payout_v1.rs", include_str!("palw_economic_payout_v1.rs")),
            ("palw_economic_compute_v1.rs", include_str!("palw_economic_compute_v1.rs")),
            ("palw_economics_ledger_v1.rs", include_str!("palw_economics_ledger_v1.rs")),
            ("palw_credit.rs", include_str!("palw_credit.rs")),
        ];
        let needles: [String; 6] = [
            ["palw_resource", "_profile_v1"].concat(),
            ["PalwRuntime", "ProfileV1"].concat(),
            ["PalwResource", "ProfileV1"].concat(),
            ["PalwResource", "RoleV1"].concat(),
            ["working_", "set_bytes"].concat(),
            ["kv_resident", "_bytes"].concat(),
        ];
        for (name, src) in sources {
            for needle in &needles {
                assert!(!src.contains(needle.as_str()), "{name} reads {needle}: a memory figure has become a price");
            }
        }
    }
}
