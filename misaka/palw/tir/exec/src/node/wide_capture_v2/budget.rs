//! Checked logical backing envelopes for one capture operation. These are not
//! physical RSS measurements or native host leases. Independent caller-owned
//! inputs, verified spaces and retained exports must be pre-summed in
//! `external_retained_bytes` unless the operation already counts that owner.

use std::mem::size_of;

use kaspa_consensus_core::{
    palw_step_leg::PalwStepTileLeafV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1},
    palw_tir_step_v1::PalwTirStepBindingV1,
    palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_MAX_NETWORK_ID_BYTES},
};
use misaka_palw_tir::program::{MAX_BLOCKS, MAX_NODES_PER_BLOCK, MAX_PROGRAM_BYTES, MAX_STATES};

use super::TirWideCaptureV2;
use crate::node::run_wide_q24_v2::TirJobRunWideQ24V2;

pub(crate) const PARSER_SCRATCH_BYTES: u64 = 8192;

/// Explicit local resource ceilings; no successful default is supplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideCaptureLimitsV2 {
    pub max_frame_bytes: u64,
    pub max_decoded_bytes: u64,
    pub max_rows: u64,
    pub max_row_lanes: u64,
    pub max_prompt_ids: u64,
    pub max_leaves: u64,
    pub max_leaf_payload_bytes: u64,
    pub max_total_leaf_payload_bytes: u64,
    pub max_operation_bytes: u64,
    pub external_retained_bytes: u64,
}

/// Authentication admission is separate from decoding/copying admission.
/// Expected binding/space construction and all independent live owners belong
/// to the caller's external reservation; this does not admit program decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideCaptureAuthLimitsV2 {
    pub max_step_leaves: u64,
    pub max_work: u64,
    pub max_scratch_bytes: u64,
    pub max_operation_bytes: u64,
    pub external_retained_bytes: u64,
}

/// Components of one logical peak. `moved_leaf_bytes` is an informational
/// subset of `capture_bytes`, never an additional summand. Encoded frame bytes
/// describe the eventual frame even when a build operation does not allocate it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideCaptureQuoteV2 {
    pub input_frame_bytes: u64,
    pub source_run_bytes: u64,
    pub moved_leaf_bytes: u64,
    pub capture_bytes: u64,
    pub scratch_bytes: u64,
    pub encoded_frame_bytes: u64,
    pub external_retained_bytes: u64,
    pub operation_peak_bytes: u64,
}

fn count(n: usize) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "wide capture native count exceeds u64".into())
}

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "wide capture backing addition overflow".into())
}

fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "wide capture backing multiplication overflow".into())
}

fn storage<T>(n: usize) -> Result<u64, String> {
    mul(count(n)?, count(size_of::<T>())?)
}

fn wire_count(n: usize) -> Result<u64, String> {
    u32::try_from(n).map(u64::from).map_err(|_| "wide capture vector exceeds its u32 wire count".into())
}

fn ceiling(value: u64, limit: u64, what: &str) -> Result<(), String> {
    if value > limit {
        return Err(format!("wide capture {what} ceiling"));
    }
    Ok(())
}

/// Only cheap scalar/vector checks: no hash, program decode, class construction
/// or judgement of disclosed values occurs here.
fn check_binding_format(binding: &PalwTirStepBindingV1) -> Result<(), String> {
    let class = &binding.class;
    let job = &binding.job_context;
    if binding.version != 1
        || class.version != PALW_TIR_CLASS_VERSION_V1
        || class.layout.version != PALW_TIR_LAYOUT_VERSION_V1
        || job.version != PALW_TRACE_COMMITMENT_VERSION_V2
    {
        return Err("wide capture binding/class/layout/context version".into());
    }
    if job.network_id.is_empty()
        || job.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES
        || class.program.len() > MAX_PROGRAM_BYTES
        || class.layout.commit_tiles.len() > MAX_BLOCKS * MAX_NODES_PER_BLOCK
        || class.layout.state_tiles.len() > MAX_STATES
    {
        return Err("wide capture existing metadata format ceiling".into());
    }
    wire_count(job.network_id.len())?;
    wire_count(class.program.len())?;
    wire_count(class.layout.commit_tiles.len())?;
    wire_count(class.layout.state_tiles.len())?;
    Ok(())
}

fn binding_buffers(binding: &PalwTirStepBindingV1, fresh: bool) -> Result<u64, String> {
    let class = &binding.class;
    let job = &binding.job_context;
    let bytes = if fresh {
        add(count(class.program.len())?, count(job.network_id.len())?)?
    } else {
        add(count(class.program.capacity())?, count(job.network_id.capacity())?)?
    };
    let commits = if fresh { class.layout.commit_tiles.len() } else { class.layout.commit_tiles.capacity() };
    let states = if fresh { class.layout.state_tiles.len() } else { class.layout.state_tiles.capacity() };
    add(bytes, add(storage::<u32>(commits)?, storage::<u32>(states)?)?)
}

/// The embedded binding header plus its actual retained vector capacities.
pub(crate) fn binding_backing(binding: &PalwTirStepBindingV1) -> Result<u64, String> {
    add(count(size_of::<PalwTirStepBindingV1>())?, binding_buffers(binding, false)?)
}

fn leaf_backing(leaves: &[PalwStepTileLeafV1], capacity: usize) -> Result<u64, String> {
    leaves.iter().try_fold(storage::<PalwStepTileLeafV1>(capacity)?, |bytes, leaf| add(bytes, count(leaf.values_le.capacity())?))
}

/// Exact logical backing using all actual retained capacities. The binding's
/// inline header is already included in the capture header.
pub(crate) fn capture_backing(capture: &TirWideCaptureV2) -> Result<u64, String> {
    let mut bytes = add(count(size_of::<TirWideCaptureV2>())?, binding_buffers(&capture.binding, false)?)?;
    bytes = add(bytes, storage::<u32>(capture.prompt.capacity())?)?;
    bytes = add(bytes, storage::<u32>(capture.generated.capacity())?)?;
    bytes = add(bytes, storage::<Vec<i64>>(capture.logits_rows.capacity())?)?;
    for row in &capture.logits_rows {
        bytes = add(bytes, storage::<i64>(row.capacity())?)?;
    }
    add(bytes, leaf_backing(&capture.leaves, capture.leaves.capacity())?)
}

/// Check count/payload ceilings before allocation or identity hashing. This is
/// resource admission, not authentication or evaluation: literal IDs, logits,
/// leaf versions and values are deliberately not interpreted here.
pub(crate) fn check_resources(
    binding: &PalwTirStepBindingV1,
    prompt_len: usize,
    rows: &[Vec<i64>],
    generated_len: usize,
    leaves: &[PalwStepTileLeafV1],
    limits: WideCaptureLimitsV2,
) -> Result<(), String> {
    check_binding_format(binding)?;
    ceiling(wire_count(prompt_len)?, limits.max_prompt_ids, "prompt count")?;
    ceiling(wire_count(rows.len())?, limits.max_rows, "row count")?;
    ceiling(wire_count(generated_len)?, limits.max_rows, "generated count")?;
    ceiling(wire_count(leaves.len())?, limits.max_leaves, "leaf count")?;
    for row in rows {
        ceiling(wire_count(row.len())?, limits.max_row_lanes, "row lane count")?;
        storage::<i64>(row.len())?;
    }
    let total = leaves.iter().try_fold(0, |bytes, leaf| {
        let length = wire_count(leaf.values_le.len())?;
        ceiling(length, limits.max_leaf_payload_bytes, "per-leaf payload")?;
        add(bytes, length)
    })?;
    ceiling(total, limits.max_total_leaf_payload_bytes, "total leaf payload")
}

/// Conservative simultaneous class/layout/context comparison and official
/// identity/Flat-prompt writer scratch. Does not include nested program decode,
/// a new StepSpace or a model run. Those operations are not performed here.
pub(crate) fn metadata_scratch(binding: &PalwTirStepBindingV1, prompt_len: usize) -> Result<u64, String> {
    check_binding_format(binding)?;
    wire_count(prompt_len)?;
    let metadata = add(binding_buffers(binding, true)?, storage::<u32>(prompt_len)?)?;
    add(mul(add(metadata, 4096)?, 2)?, PARSER_SCRATCH_BYTES)
}

fn encoded_frame_size(
    binding: &PalwTirStepBindingV1,
    prompt_len: usize,
    rows: &[Vec<i64>],
    generated_len: usize,
    leaves: &[PalwStepTileLeafV1],
) -> Result<u64, String> {
    // All vector lengths have already been checked for u32 representability.
    let binding_bytes = borsh::object_length(binding).map_err(|e| format!("wide capture binding size: {e}"))?;
    let mut bytes = add(8, count(binding_bytes)?)?;
    bytes = add(bytes, add(4, storage::<u32>(prompt_len)?)?)?;
    bytes = add(bytes, 4)?;
    for row in rows {
        bytes = add(bytes, add(4, storage::<i64>(row.len())?)?)?;
    }
    bytes = add(bytes, add(4, storage::<u32>(generated_len)?)?)?;
    bytes = add(bytes, 4)?;
    for leaf in leaves {
        // u16 version, four u32 coordinates, u32 lane count, u32 byte count.
        bytes = add(bytes, add(26, count(leaf.values_le.len())?)?)?;
    }
    usize::try_from(bytes).map_err(|_| "wide capture frame exceeds native allocation width")?;
    Ok(bytes)
}

/// Quote the private exact-sized copies while retaining the source run and
/// moving the caller's existing leaf vectors, including their spare capacity.
#[allow(clippy::ptr_arg)]
pub(crate) fn quote_build(
    expected: &PalwTirStepBindingV1,
    prompt: &[u32],
    run: &TirJobRunWideQ24V2,
    leaves: &Vec<PalwStepTileLeafV1>,
    limits: WideCaptureLimitsV2,
) -> Result<WideCaptureQuoteV2, String> {
    check_resources(expected, prompt.len(), run.logits_rows(), run.generated().len(), leaves, limits)?;
    let moved_leaf_bytes = leaf_backing(leaves, leaves.capacity())?;
    let mut capture_bytes = add(count(size_of::<TirWideCaptureV2>())?, binding_buffers(expected, true)?)?;
    capture_bytes = add(capture_bytes, storage::<u32>(prompt.len())?)?;
    capture_bytes = add(capture_bytes, storage::<u32>(run.generated().len())?)?;
    capture_bytes = add(capture_bytes, storage::<Vec<i64>>(run.logits_rows().len())?)?;
    for row in run.logits_rows() {
        capture_bytes = add(capture_bytes, storage::<i64>(row.len())?)?;
    }
    capture_bytes = add(capture_bytes, moved_leaf_bytes)?;
    ceiling(capture_bytes, limits.max_decoded_bytes, "decoded backing")?;
    let scratch_bytes = metadata_scratch(expected, prompt.len())?;
    let operation_peak_bytes = add(add(run.reserved_bytes(), capture_bytes)?, add(scratch_bytes, limits.external_retained_bytes)?)?;
    ceiling(operation_peak_bytes, limits.max_operation_bytes, "build operation backing")?;
    let encoded_frame_bytes = encoded_frame_size(expected, prompt.len(), run.logits_rows(), run.generated().len(), leaves)?;
    ceiling(encoded_frame_bytes, limits.max_frame_bytes, "encoded frame")?;
    Ok(WideCaptureQuoteV2 {
        input_frame_bytes: 0,
        source_run_bytes: run.reserved_bytes(),
        moved_leaf_bytes,
        capture_bytes,
        scratch_bytes,
        encoded_frame_bytes,
        external_retained_bytes: limits.external_retained_bytes,
        operation_peak_bytes,
    })
}

/// Quote one newly encoded frame coexisting with the existing capture. Further
/// returned frames or other retained exports belong to the caller's reservation.
pub(crate) fn quote_encode(capture: &TirWideCaptureV2, limits: WideCaptureLimitsV2) -> Result<WideCaptureQuoteV2, String> {
    check_resources(&capture.binding, capture.prompt.len(), &capture.logits_rows, capture.generated.len(), &capture.leaves, limits)?;
    let capture_bytes = capture_backing(capture)?;
    ceiling(capture_bytes, limits.max_decoded_bytes, "decoded backing")?;
    // Count-only serialization neither clones buffers nor creates a frame.
    let body = borsh::object_length(capture).map_err(|e| format!("wide capture encoded size: {e}"))?;
    let encoded_frame_bytes = add(8, count(body)?)?;
    usize::try_from(encoded_frame_bytes).map_err(|_| "wide capture frame exceeds native allocation width")?;
    ceiling(encoded_frame_bytes, limits.max_frame_bytes, "encoded frame")?;
    let operation_peak_bytes =
        add(add(capture_bytes, encoded_frame_bytes)?, add(PARSER_SCRATCH_BYTES, limits.external_retained_bytes)?)?;
    ceiling(operation_peak_bytes, limits.max_operation_bytes, "encode operation backing")?;
    Ok(WideCaptureQuoteV2 {
        input_frame_bytes: 0,
        source_run_bytes: 0,
        moved_leaf_bytes: leaf_backing(&capture.leaves, capture.leaves.capacity())?,
        capture_bytes,
        scratch_bytes: PARSER_SCRATCH_BYTES,
        encoded_frame_bytes,
        external_retained_bytes: limits.external_retained_bytes,
        operation_peak_bytes,
    })
}
