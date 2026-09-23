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
use crate::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
use crate::palw_step::{PalwLayerKindV1, PalwShapeProfileV3, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, canonical_step_coordinates};
use crate::palw_v2::PalwJobContextV2;
use crate::palw_verification_v2::palw_segment_count_v2;

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
    /// position's committed trace at the widest history`.
    pub trace_scratch_bytes: u64,
}

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

/// **The resource profile of `role` on `job` under `runtime`** — the derivation, whole.
/// `leaf_count` is the job's step-leaf count (a partial seat's segment is cut over it; the other
/// roles do not read it).
pub fn palw_resource_profile_v1(
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    leaf_count: u64,
    runtime: PalwRuntimeProfileV1,
    role: PalwResourceRoleV1,
    limits: PalwRuntimeLimitsV1,
) -> Option<PalwResourceProfileV1> {
    let attention_layers = palw_attention_layers_v1(profile);
    let kv_dim = u64::from(profile.attn_kv_heads).saturating_mul(u64::from(profile.attn_head_dim));
    let heads = u64::from(profile.attn_heads);
    let (resume_rows, end_rows) = palw_role_rows_v1(profile, job, leaf_count, role)?;
    let committed = PALW_KV_COMMITTED_BYTES_PER_ELEMENT_V1;
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
    let trace_scratch_bytes = u64::from(limits.prefill_run_positions.max(1)).saturating_mul(palw_committed_trace_bytes_v1(profile, end_rows));
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
            palw_resource_profile_v1(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI32, PalwResourceRoleV1::Producer, LIMITS)
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
        assert_eq!(i32_producer.trace_scratch_bytes, 64 * palw_committed_trace_bytes_v1(&profile, rows));

        let i16_producer =
            palw_resource_profile_v1(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI16, PalwResourceRoleV1::Producer, LIMITS)
                .expect("derives");
        assert_eq!(i16_producer.kv_resident_bytes * 2, i32_producer.kv_resident_bytes, "i16 halves the resident term");
        assert_eq!(i16_producer.checkpoint_bytes, i32_producer.checkpoint_bytes, "and moves no committed byte");
        assert_eq!(i16_producer.attention_scratch_bytes, i32_producer.attention_scratch_bytes);

        // The full seat replays the same job, so it holds the same rows.
        let full = palw_resource_profile_v1(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI16, PalwResourceRoleV1::FullSeat, LIMITS)
            .expect("derives");
        assert_eq!((full.resume_rows, full.end_rows, full.working_set_bytes()), (0, rows, i16_producer.working_set_bytes()));

        for p in [&i32_producer, &i16_producer] {
            eprintln!(
                "2M attempt under {}: kv {:.2} GiB + attention scratch {:.2} GiB + trace scratch {:.2} GiB = working set {:.2} GiB \
                 (checkpoint {:.2} GiB)",
                p.runtime.name(),
                p.kv_resident_bytes as f64 / GIB,
                p.attention_scratch_bytes as f64 / GIB,
                p.trace_scratch_bytes as f64 / GIB,
                p.working_set_bytes() as f64 / GIB,
                p.checkpoint_bytes as f64 / GIB
            );
        }
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
        let full = palw_resource_profile_v1(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI16, PalwResourceRoleV1::FullSeat, LIMITS)
            .expect("derives");
        let mut ends = Vec::new();
        for segment in 0..3u16 {
            let role = PalwResourceRoleV1::PartialSeat { seat_count: 4, segment_index: segment };
            let p = palw_resource_profile_v1(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI16, role, LIMITS).expect("derives");
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
        assert!(
            ends[1].working_set_bytes() > full.working_set_bytes(),
            "…but its i32 opening outweighs the i16 rows it saves: {} > {}",
            ends[1].working_set_bytes(),
            full.working_set_bytes()
        );
        assert!(ends[1].from_genesis().working_set_bytes() < full.working_set_bytes(), "started at the prompt, it is smaller than a full seat");
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
        let p = palw_resource_profile_v1(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI32, PalwResourceRoleV1::Producer, LIMITS)
            .expect("derives");
        assert_eq!(p.end_rows, 16, "14 prefill rows and one per decode call after the first");
        // Checkpoints after calls 1 and 2 (interval 1): 15 and 16 rows of i32.
        assert_eq!(p.retained_checkpoint_bytes, palw_kv_series_bytes_v1(28, 256, 15 + 16, 4));
        assert_eq!(p.attention_scratch_bytes, 0, "a v2 graph has no fused site; its context rows are trace rows");
        assert!(p.trace_scratch_bytes > 0);
        // Every segment of this job that starts at a decode call resumes from a call boundary or the prompt.
        for segment in 0..palw_segment_count_v2(4) {
            let role = PalwResourceRoleV1::PartialSeat { seat_count: 4, segment_index: segment };
            let s = palw_resource_profile_v1(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI32, role, LIMITS).expect("derives");
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
        let p = palw_resource_profile_v1(&profile, &job, leaves, PalwRuntimeProfileV1::A16KvI32, PalwResourceRoleV1::Producer, LIMITS)
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
