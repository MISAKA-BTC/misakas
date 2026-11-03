//! Local source-only capture experiment. No public wire, hash domain, record ID,
//! signed commitment or computation/admission capability is assigned here.
//! All values come from one ordinary Panel replay of the ORIGINAL program.
//! The pending commitment is deliberately incompatible with PC2R1REC/Digest.

use super::run::{TirClassRunnerV1, TirJobRunV1, TirRunObserverV1};
use crate::{Buf, Elem, NodeValue, Slice, with_slice};
use kaspa_consensus_core::{
    Hash64,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
    palw_step_refute::{flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1},
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1},
    palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2},
};
use misaka_palw_tir::{
    DType,
    demand::history_length_v1,
    prim::Prim,
    program::{MAX_BLOCKS, MAX_NODES_PER_BLOCK, MAX_PROGRAM_BYTES, MAX_STATES, StateKind},
};

#[derive(Clone, Copy, Debug)]
pub struct TypedCaptureLimits {
    pub max_positions: u32,
    pub max_geometry_visits: u64,
    pub max_records: u64,
    pub max_lanes: u64,
    pub max_payload_bytes: u64,
    /// Additional collector/geometry backing only, not executor/model/RSS.
    pub max_peak_capture_heap_bytes: u64,
    /// Logical local storage estimate only: no file codec is allocated.
    pub max_local_storage_bytes: u64,
    pub max_legacy_leaves: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedCaptureError {
    Binding,
    Budget,
    Overflow,
    Geometry,
    Value,
    UnsupportedOutputProfile,
    Replay(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedCaptureOrigin {
    pub class: Hash64,
    pub artifact: Hash64,
    pub execution: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedRecordKind {
    Node {
        position: u32,
        occurrence: u16,
        slot: u32,
        block: u8,
        layer: Option<u16>,
        node: u16,
        committed: bool,
    },
    FixedAfter {
        position: u32,
        state: u16,
        layer: Option<u16>,
    },
    /// This position's appended row, including window=1, before eviction.
    HistoryAppendRow {
        position: u32,
        state: u16,
        layer: Option<u16>,
    },
    LastOutput {
        position: u32,
        selected: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedRecord {
    pub kind: TypedRecordKind,
    pub dtype: DType,
    pub shape: Vec<usize>,
    /// Native storage of the DECLARED dtype, including exact i64/i128 lanes.
    pub values: Buf,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypedCaptureSummary {
    pub positions: u32,
    pub records: u64,
    pub node_records: u64,
    pub fixed_records: u64,
    pub history_records: u64,
    pub output_records: u64,
    pub lanes: u64,
    pub payload_bytes: u64,
    /// Conservative format-ceiling scratch for input authentication and a fresh
    /// ordinary runner's tile metadata, included in peak_capture_heap_bytes.
    pub input_auth_heap_bytes: u64,
    pub peak_capture_heap_bytes: u64,
    pub local_storage_bytes: u64,
    pub geometry_visits: u64,
}

/// An unassigned contract requirement, NOT a digest, authentication token or
/// cache key. A0 must assign any future Panel-only commitment separately.
/// No equality/serialization/conversion to an old execution root is provided.
pub struct PendingTypedRecordCommitment {
    _unassigned: (),
}

pub struct PrototypeTypedCapture {
    origin: TypedCaptureOrigin,
    entries: Vec<TypedRecord>,
    summary: TypedCaptureSummary,
    run: TirJobRunV1,
    pending: PendingTypedRecordCommitment,
}
impl PrototypeTypedCapture {
    pub fn origin(&self) -> &TypedCaptureOrigin {
        &self.origin
    }
    pub fn entries(&self) -> &[TypedRecord] {
        &self.entries
    }
    pub fn summary(&self) -> TypedCaptureSummary {
        self.summary
    }
    pub fn run(&self) -> &TirJobRunV1 {
        &self.run
    }
    pub fn pending_commitment(&self) -> &PendingTypedRecordCommitment {
        &self.pending
    }
    /// Full local copy comparison only. It does not certify arithmetic or make
    /// a peer-supplied copy authenticated. Caller owns/reserves candidate memory.
    pub fn validate_local_copy(
        &self,
        origin: &TypedCaptureOrigin,
        entries: &[TypedRecord],
        generated: &[u32],
    ) -> Result<(), TypedCaptureError> {
        if origin != &self.origin || entries != self.entries || generated != self.run.generated {
            return Err(TypedCaptureError::Binding);
        }
        Ok(())
    }
}

struct Expected {
    kind: TypedRecordKind,
    dtype: DType,
    shape: Vec<usize>,
    lanes: usize,
}
fn add(a: u64, b: u64) -> Result<u64, TypedCaptureError> {
    a.checked_add(b).ok_or(TypedCaptureError::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, TypedCaptureError> {
    a.checked_mul(b).ok_or(TypedCaptureError::Overflow)
}
fn lanes(shape: &[usize]) -> Result<usize, TypedCaptureError> {
    shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d).ok_or(TypedCaptureError::Overflow))
}
fn origin(
    runner: &TirClassRunnerV1<'_>,
    registered: &PalwTirClassV1,
    artifact: Hash64,
    job: &PalwJobContextV2,
    expected_execution: Hash64,
    limits: TypedCaptureLimits,
) -> Result<TypedCaptureOrigin, TypedCaptureError> {
    // Bound foreign typed DTOs before encoding/hashing any dynamic fields.
    if registered.version != PALW_TIR_CLASS_VERSION_V1
        || registered.layout.version != PALW_TIR_LAYOUT_VERSION_V1
        || registered.program.len() > MAX_PROGRAM_BYTES
        || registered.layout.commit_tiles.len() > MAX_BLOCKS * MAX_NODES_PER_BLOCK
        || registered.layout.state_tiles.len() > MAX_STATES
        || job.version != PALW_TRACE_COMMITMENT_VERSION_V2
        || job.network_id.is_empty()
        || job.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES
        || job.declared_prefill_tokens == 0
        || job.exact_decode_tokens == 0
        || job.declared_prefill_tokens.checked_add(job.exact_decode_tokens).is_none_or(|n| n > job.max_context_tokens)
    {
        return Err(TypedCaptureError::Binding);
    }
    if input_auth_heap(registered, job)? > limits.max_peak_capture_heap_bytes {
        return Err(TypedCaptureError::Budget);
    }
    let scheme = Hash64::from_bytes(runner.plan.program.logits_scheme_id);
    if job.trace_scheme_id != scheme || (scheme != flat_logits_scheme_id_v1() && scheme != tiled_logits_scheme_id_v1()) {
        return Err(TypedCaptureError::UnsupportedOutputProfile);
    }
    if registered.layout != runner.space.layout
        || job.shape_profile_id != runner.class_id
        || runner.space.program != runner.plan.program
    {
        return Err(TypedCaptureError::Binding);
    }
    if registered.program != runner.plan.program.encode() || registered.class_id(&artifact) != runner.class_id {
        return Err(TypedCaptureError::Binding);
    }
    let dtype =
        runner.plan.program.blocks[runner.plan.program.schedule.post as usize].nodes[runner.plan.program.logits as usize].out.dtype;
    if dtype != DType::I32 {
        // The adopted wide-final-output profile is separate and not installed
        // by this experiment. Do not run it through old i32 selection/roots.
        return Err(TypedCaptureError::UnsupportedOutputProfile);
    }
    // Registration tokenizer is authenticated inside class_id. The canonical
    // job tokenizer may be zero and remains literal in its independently pinned
    // context; copying the registration tokenizer would change this identity.
    let execution = job.context_hash();
    if execution != expected_execution {
        return Err(TypedCaptureError::Binding);
    }
    Ok(TypedCaptureOrigin { class: runner.class_id, artifact, execution })
}

fn input_auth_heap(registered: &PalwTirClassV1, job: &PalwJobContextV2) -> Result<u64, TypedCaptureError> {
    // Encoding the trusted program, layout/context CanonicalWriter growth and
    // Flat prompt hashing allocate outside the typed-value vectors. Use the
    // format ceiling even for a mismatching tiny class DTO, BEFORE encoding.
    let layout = mul(add(registered.layout.commit_tiles.len() as u64, registered.layout.state_tiles.len() as u64)?, 4)?;
    let prompt = add(mul(job.declared_prefill_tokens as u64, 4)?, 4)?;
    let writers = mul(add(add(MAX_PROGRAM_BYTES as u64, layout)?, add(prompt, 4096)?)?, 2)?;
    let fresh_tiles =
        add(mul((MAX_BLOCKS * MAX_NODES_PER_BLOCK) as u64, 4)?, mul(MAX_BLOCKS as u64, std::mem::size_of::<Vec<u32>>() as u64)?)?;
    add(writers, fresh_tiles)
}

fn geometry(
    runner: &TirClassRunnerV1<'_>,
    job: &PalwJobContextV2,
    limits: TypedCaptureLimits,
    mut emit: impl FnMut(Expected) -> Result<(), TypedCaptureError>,
) -> Result<(u32, u64), TypedCaptureError> {
    let shape = runner.space.job_shape(job).map_err(|_| TypedCaptureError::Binding)?;
    if shape.positions == 0 || shape.positions > limits.max_positions {
        return Err(TypedCaptureError::Budget);
    }
    runner.space.leaf_count_capped(job, limits.max_legacy_leaves).map_err(|_| TypedCaptureError::Budget)?;
    let plan = runner.plan;
    let nodes = plan.occurrences.iter().try_fold(0u64, |n, (b, _)| add(n, plan.program.blocks[*b as usize].nodes.len() as u64))?;
    // A conservative structural bound is checked BEFORE per-position loops.
    // Two geometry passes plus observed node callbacks and two state scans.
    // Empty occurrences still cost visits even when they produce no nodes.
    let per_position =
        add(add(mul(plan.occurrences.len() as u64, 2)?, mul(nodes, 3)?)?, add(mul(plan.instances.len() as u64, 4)?, 3)?)?;
    let mut visits = mul(shape.positions as u64, per_position)?;
    for inst in &plan.instances {
        if let StateKind::Hist { window } = inst.kind {
            let p = shape.positions as u64;
            let w = u64::from(window.saturating_sub(1));
            let k = p.min(w);
            visits = add(visits, add(mul(k, add(k, 1)?)? / 2, mul(p - k, w)?)?)?;
        }
    }
    if visits > limits.max_geometry_visits {
        return Err(TypedCaptureError::Budget);
    }
    for position in 0..shape.positions {
        let occ_count = plan.occurrences.len() - usize::from(!shape.runs_post(position));
        for occurrence in 0..occ_count {
            let (block, layer) = plan.occurrences[occurrence];
            let h = history_length_v1(&plan.info, block, position).ok_or(TypedCaptureError::Geometry)?;
            for (node, value) in plan.program.blocks[block as usize].nodes.iter().enumerate() {
                let actual = value.out.resolve(h);
                let count = lanes(&actual)?;
                emit(Expected {
                    kind: TypedRecordKind::Node {
                        position,
                        occurrence: u16::try_from(occurrence).map_err(|_| TypedCaptureError::Overflow)?,
                        slot: plan.slot_bases[occurrence] + node as u32,
                        block,
                        layer,
                        node: node as u16,
                        committed: value.commit,
                    },
                    dtype: value.out.dtype,
                    shape: actual,
                    lanes: count,
                })?;
                if let Prim::HistAppend { state } = value.prim {
                    let decl = &plan.program.states[state as usize];
                    let row: Vec<usize> = decl.shape.iter().map(|n| *n as usize).collect();
                    let count = lanes(&row)?;
                    emit(Expected {
                        kind: TypedRecordKind::HistoryAppendRow { position, state, layer: if decl.per_layer { layer } else { None } },
                        dtype: decl.dtype,
                        shape: row,
                        lanes: count,
                    })?;
                }
            }
        }
        for inst in plan.instances.iter().filter(|s| matches!(s.kind, StateKind::Fixed { .. })) {
            emit(Expected {
                kind: TypedRecordKind::FixedAfter { position, state: inst.state, layer: inst.layer },
                dtype: inst.dtype,
                shape: inst.shape.clone(),
                lanes: lanes(&inst.shape)?,
            })?;
        }
        if shape.runs_post(position) {
            let block = plan.program.schedule.post;
            let value = &plan.program.blocks[block as usize].nodes[plan.program.logits as usize];
            let actual = value.out.resolve(history_length_v1(&plan.info, block, position).ok_or(TypedCaptureError::Geometry)?);
            let count = lanes(&actual)?;
            emit(Expected {
                kind: TypedRecordKind::LastOutput { position, selected: 0 },
                dtype: value.out.dtype,
                shape: actual,
                lanes: count,
            })?;
        }
    }
    Ok((shape.positions, visits))
}

pub fn quote_typed_prototype(
    runner: &TirClassRunnerV1<'_>,
    registered: &PalwTirClassV1,
    artifact: Hash64,
    job: &PalwJobContextV2,
    expected_execution: Hash64,
    limits: TypedCaptureLimits,
) -> Result<TypedCaptureSummary, TypedCaptureError> {
    origin(runner, registered, artifact, job, expected_execution, limits)?;
    let mut s = TypedCaptureSummary::default();
    let mut shapes = 0u64;
    let (positions, visits) = geometry(runner, job, limits, |e| {
        if e.shape.capacity() != e.shape.len() {
            return Err(TypedCaptureError::Budget);
        }
        s.records = add(s.records, 1)?;
        match e.kind {
            TypedRecordKind::Node { .. } => s.node_records += 1,
            TypedRecordKind::FixedAfter { .. } => s.fixed_records += 1,
            TypedRecordKind::HistoryAppendRow { .. } => s.history_records += 1,
            TypedRecordKind::LastOutput { .. } => s.output_records += 1,
        }
        s.lanes = add(s.lanes, e.lanes as u64)?;
        s.payload_bytes = add(s.payload_bytes, mul(e.lanes as u64, e.dtype.width() as u64)?)?;
        shapes = add(shapes, mul(e.shape.len() as u64, std::mem::size_of::<usize>() as u64)?)?;
        if s.records > limits.max_records || s.lanes > limits.max_lanes || s.payload_bytes > limits.max_payload_bytes {
            return Err(TypedCaptureError::Budget);
        }
        Ok(())
    })?;
    s.positions = positions;
    s.geometry_visits = visits;
    s.input_auth_heap_bytes = input_auth_heap(registered, job)?;
    // Two coexisting header/shape owners: expected geometry and captured data.
    // Include bounded scratch and collector inline storage. Executor, original
    // run leaves/rows/generated, allocator headers and RSS are separate costs.
    s.peak_capture_heap_bytes = add(
        add(mul(s.records, (std::mem::size_of::<Expected>() + std::mem::size_of::<TypedRecord>()) as u64)?, mul(shapes, 2)?)?,
        add(s.payload_bytes, add(s.input_auth_heap_bytes, 1024)?)?,
    )?;
    // A declared logical storage model, NOT a wire codec: 64 bytes/entry plus
    // 8 bytes/shape dimension, native payload, origin and generated IDs.
    s.local_storage_bytes =
        add(add(add(mul(s.records, 64)?, shapes)?, s.payload_bytes)?, add(192, mul(job.exact_decode_tokens as u64, 4)?)?)?;
    if s.peak_capture_heap_bytes > limits.max_peak_capture_heap_bytes || s.local_storage_bytes > limits.max_local_storage_bytes {
        return Err(TypedCaptureError::Budget);
    }
    Ok(s)
}

struct Collector<'a> {
    runner: &'a TirClassRunnerV1<'a>,
    expected: Vec<Expected>,
    entries: Vec<TypedRecord>,
}
fn normalize(dtype: DType, data: Slice<'_>, from: usize, count: usize) -> Result<Buf, String> {
    let end = from.checked_add(count).filter(|n| *n <= data.len()).ok_or("typed lane extent")?;
    macro_rules! native {
        ($t:ty, $v:ident) => {{
            let mut out = Vec::<$t>::new(); out.try_reserve_exact(count).map_err(|_| "typed value allocation")?;
            if out.capacity() != count { return Err("typed allocation capacity exceeds quote".into()); }
            with_slice!(data, src => { for value in &src[from..end] { out.push(<$t>::try_from(value.to_i128()).map_err(|_| "typed value truncation")?); } });
            Buf::$v(out)
        }};
    }
    Ok(match dtype {
        DType::I8 => native!(i8, I8),
        DType::I16 => native!(i16, I16),
        DType::I32 => native!(i32, I32),
        DType::I64 => native!(i64, I64),
        DType::I128 => native!(i128, I128),
        DType::Idx => native!(u32, Idx),
    })
}
impl Collector<'_> {
    fn push(
        &mut self,
        kind: TypedRecordKind,
        dtype: DType,
        shape: &[usize],
        data: Slice<'_>,
        from: usize,
        count: usize,
    ) -> Result<(), String> {
        let expected = self.expected.get(self.entries.len()).ok_or("extra typed record")?;
        let same_kind = match (&expected.kind, &kind) {
            (TypedRecordKind::LastOutput { position: a, .. }, TypedRecordKind::LastOutput { position: b, .. }) => a == b,
            (a, b) => a == b,
        };
        if !same_kind || expected.dtype != dtype || expected.shape != shape || expected.lanes != count {
            return Err("typed record geometry mismatch".into());
        }
        let values = normalize(dtype, data, from, count)?;
        let mut owned_shape = Vec::new();
        owned_shape.try_reserve_exact(shape.len()).map_err(|_| "typed shape allocation")?;
        if owned_shape.capacity() != shape.len() {
            return Err("typed shape capacity exceeds quote".into());
        }
        owned_shape.extend_from_slice(shape);
        self.entries.push(TypedRecord { kind, dtype, shape: owned_shape, values });
        Ok(())
    }
}
impl TirRunObserverV1 for Collector<'_> {
    fn node(&mut self, position: u32, value: &NodeValue<'_>) -> Result<(), String> {
        let e = self.expected.get(self.entries.len()).ok_or("extra node")?;
        let occurrence = match e.kind {
            TypedRecordKind::Node { occurrence, .. } => occurrence,
            _ => return Err("expected non-node".into()),
        };
        self.push(
            TypedRecordKind::Node {
                position,
                occurrence,
                slot: value.slot,
                block: value.block,
                layer: value.layer,
                node: value.node,
                committed: value.commit,
            },
            value.dtype,
            value.shape,
            value.data,
            0,
            value.data.len(),
        )?;
        if let Prim::HistAppend { state } = self.runner.plan.program.blocks[value.block as usize].nodes[value.node as usize].prim {
            let decl = &self.runner.plan.program.states[state as usize];
            let row: Vec<usize> = decl.shape.iter().map(|n| *n as usize).collect();
            let count = lanes(&row).map_err(|e| format!("{e:?}"))?;
            let from = value.data.len().checked_sub(count).ok_or("history row missing")?;
            self.push(
                TypedRecordKind::HistoryAppendRow { position, state, layer: if decl.per_layer { value.layer } else { None } },
                decl.dtype,
                &row,
                value.data,
                from,
                count,
            )?;
        }
        Ok(())
    }
    fn state(
        &mut self,
        position: u32,
        state: u16,
        layer: Option<u16>,
        history_row: Option<u32>,
        dtype: DType,
        shape: &[usize],
        data: Slice<'_>,
    ) -> Result<(), String> {
        if history_row.is_some() {
            // History source rows are already preserved at their HistAppend,
            // including rows an end-step window=1 immediately evicts. Do not
            // multiply retained payload by a full history window each step.
            return Ok(());
        }
        self.push(TypedRecordKind::FixedAfter { position, state, layer }, dtype, shape, data, 0, data.len())
    }
    fn last_output(&mut self, position: u32, dtype: DType, shape: &[usize], data: Slice<'_>, selected: u32) -> Result<(), String> {
        self.push(TypedRecordKind::LastOutput { position, selected }, dtype, shape, data, 0, data.len())
    }
}

/// This local prototype explicitly accepts Flat committed prompt IDs only.
/// Merkle prompt delivery and the adopted wide-output profile need their own
/// already assigned adapters; neither is inferred or silently reinterpreted.
/// `expected_execution` must come from the independently resolved expected job,
/// not from an untrusted delivery's context hash.
pub fn capture_typed_prototype<'a>(
    runner: &'a TirClassRunnerV1<'a>,
    registered: &PalwTirClassV1,
    artifact: Hash64,
    job: &PalwJobContextV2,
    expected_execution: Hash64,
    prompt: &[u32],
    limits: TypedCaptureLimits,
) -> Result<PrototypeTypedCapture, TypedCaptureError> {
    let origin = origin(runner, registered, artifact, job, expected_execution, limits)?;
    let summary = quote_typed_prototype(runner, registered, artifact, job, expected_execution, limits)?;
    if prompt.len() != job.declared_prefill_tokens as usize
        || prompt.iter().any(|id| *id >= runner.plan.program.token_bound)
        || !prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, prompt, &job.prompt_token_ids_hash)
    {
        return Err(TypedCaptureError::Binding);
    }
    let count = usize::try_from(summary.records).map_err(|_| TypedCaptureError::Overflow)?;
    let mut expected = Vec::new();
    expected.try_reserve_exact(count).map_err(|_| TypedCaptureError::Budget)?;
    if expected.capacity() != count {
        return Err(TypedCaptureError::Budget);
    }
    geometry(runner, job, limits, |e| {
        if e.shape.capacity() != e.shape.len() {
            return Err(TypedCaptureError::Budget);
        }
        expected.push(e);
        Ok(())
    })?;
    let mut entries = Vec::new();
    entries.try_reserve_exact(count).map_err(|_| TypedCaptureError::Budget)?;
    if entries.capacity() != count {
        return Err(TypedCaptureError::Budget);
    }
    let mut collector = Collector { runner, expected, entries };
    // Ignore inherited custom selector/drill/fused options: one ordinary replay.
    let ordinary =
        TirClassRunnerV1::new(runner.space, runner.plan, runner.params, runner.class_id).map_err(TypedCaptureError::Replay)?;
    let run = ordinary
        .run_observed(job, prompt, limits.max_legacy_leaves, true, &mut |_| {}, &mut collector)
        .map_err(TypedCaptureError::Replay)?;
    if collector.entries.len() != count {
        return Err(TypedCaptureError::Geometry);
    }
    Ok(PrototypeTypedCapture {
        origin,
        entries: collector.entries,
        summary,
        run,
        pending: PendingTypedRecordCommitment { _unassigned: () },
    })
}
