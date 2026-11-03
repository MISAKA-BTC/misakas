//! Logical backing reservations for the local typed-to-legacy adapter. These
//! quantities do not allocate memory, authorize a service, or measure RSS. In
//! particular the owners of source authentication, evaluation and signatures
//! must supply their own simultaneous reservations explicitly.

use std::mem::size_of;

use super::{Error, RecordKey};
use crate::{
    Buf,
    node::positive_typed_capture::{PrototypeTypedCapture, TypedRecord},
};
use kaspa_consensus_core::{
    Hash64,
    palw_positive_check_v2::expanded_plan::ExpandedPlan,
    palw_step_leg::step_leg_max_opening_siblings_v1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1},
    palw_tir_step_v1::PalwTirStepBindingV1,
    palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2},
};
use misaka_palw_tir::program::{MAX_BLOCKS, MAX_NODES_PER_BLOCK, MAX_PROGRAM_BYTES, MAX_STATES};

#[derive(Clone, Copy, Debug)]
pub struct TypedAdapterLimits {
    pub max_index_records: u64,
    pub max_index_bytes: u64,
    pub max_lookup_visits: u64,
    pub max_proof_attempts: u64,
    pub max_proof_leaf_reads: u64,
    pub max_hash_work: u64,
    pub max_legacy_leaves: u64,
    pub max_tile_lanes: u64,
    pub max_legacy_payload_bytes: u64,
    pub max_path_siblings: u32,
    pub max_claims_heap_bytes: u64,
    pub max_material_bytes: u64,
    pub max_additional_heap_bytes: u64,
}

/// Explicit reservations supplied by the owning subsystems, with no successful
/// default. A zero dimension is only appropriate when that stage is absent.
/// Values are caller declarations, not verified allocations or guarantees about
/// cryptographic scratch, native admission, allocator overhead or process RSS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageReservations {
    pub retained_preparation_bytes: u64,
    pub source_auth_bytes: u64,
    pub evaluator_bytes: u64,
    pub signature_workspace_bytes: u64,
}

impl StageReservations {
    /// Keep each SAME named owner's largest envelope across successive calls.
    /// A later zero/smaller declaration cannot release its earlier reservation.
    /// Independent owners in one dimension must be pre-summed by the caller;
    /// this operation provides no proof that unrelated buffers share backing.
    pub(crate) fn joined(self, other: Self) -> Self {
        Self {
            retained_preparation_bytes: self.retained_preparation_bytes.max(other.retained_preparation_bytes),
            source_auth_bytes: self.source_auth_bytes.max(other.source_auth_bytes),
            evaluator_bytes: self.evaluator_bytes.max(other.evaluator_bytes),
            signature_workspace_bytes: self.signature_workspace_bytes.max(other.signature_workspace_bytes),
        }
    }
}

/// Current single material stage. `material_bytes` reserves both material
/// buffers (2 * the body cap); evaluator_bytes already includes target/index
/// results in A1's workspace quote, so those are not counted a second time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LiveMaterialStage {
    pub(crate) material_bytes: u64,
    pub(crate) source_auth_bytes: u64,
    pub(crate) evaluator_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypedAdapterQuote {
    pub capture_retained_bytes: u64,
    pub run_retained_bytes: u64,
    pub index_bytes: u64,
    pub all_claims_bytes: u64,
    pub legacy_scratch_bytes: u64,
    pub path_scratch_bytes: u64,
    /// Two bindings (frozen A1 record and current material) plus generated IDs.
    pub binding_clone_bytes: u64,
    /// Conservative temporary program/layout/context identity buffers, admitted
    /// before encoding or hashing and separately from retained binding clones.
    pub input_auth_scratch_bytes: u64,
    /// All four persistent owner envelopes PLUS all three live-stage buffers.
    /// Equal dimension names do not establish shared allocation or ownership.
    pub external_stage_bytes: u64,
    /// Additional adapter/stage backing while the original capture stays live.
    pub additional_peak_bytes: u64,
    /// Capture and original run backing plus all additional simultaneous stages.
    pub total_live_bytes: u64,
}

fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or(Error::Budget)
}
fn mul(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_mul(b).ok_or(Error::Budget)
}
/// Persistent source/evaluation buffers may coexist independently of the live
/// material stage. Add both: no allocation-aliasing proof permits a maximum.
/// This validates arithmetic only; owner declarations are not RSS guarantees.
pub(crate) fn stage_backing(stages: StageReservations, live: LiveMaterialStage) -> Result<u64, Error> {
    [
        stages.retained_preparation_bytes,
        stages.signature_workspace_bytes,
        stages.source_auth_bytes,
        stages.evaluator_bytes,
        live.material_bytes,
        live.source_auth_bytes,
        live.evaluator_bytes,
    ]
    .into_iter()
    .try_fold(0, add)
}

fn count(value: usize) -> Result<u64, Error> {
    u64::try_from(value).map_err(|_| Error::Budget)
}
fn backing<T>(capacity: usize) -> Result<u64, Error> {
    mul(count(capacity)?, count(size_of::<T>())?)
}
fn buffer_backing(buffer: &Buf) -> Result<u64, Error> {
    match buffer {
        Buf::I8(v) => backing::<i8>(v.capacity()),
        Buf::I16(v) => backing::<i16>(v.capacity()),
        Buf::I32(v) => backing::<i32>(v.capacity()),
        Buf::I64(v) => backing::<i64>(v.capacity()),
        Buf::I128(v) => backing::<i128>(v.capacity()),
        Buf::Idx(v) => backing::<u32>(v.capacity()),
    }
}

fn input_auth_backing(registered: &PalwTirClassV1) -> Result<u64, Error> {
    add(
        add(mul(MAX_PROGRAM_BYTES as u64, 2)?, 8192)?,
        mul(add(count(registered.layout.commit_tiles.len())?, count(registered.layout.state_tiles.len())?)?, 8)?,
    )
}
fn binding_backing(registered: &PalwTirClassV1, job: &PalwJobContextV2) -> Result<u64, Error> {
    let mut bytes = size_of::<PalwTirStepBindingV1>() as u64;
    bytes = add(bytes, count(registered.program.len())?)?;
    bytes = add(bytes, backing::<u32>(registered.layout.commit_tiles.len())?)?;
    bytes = add(bytes, backing::<u32>(registered.layout.state_tiles.len())?)?;
    bytes = add(bytes, count(job.network_id.len())?)?;
    add(mul(bytes, 2)?, mul(u64::from(job.exact_decode_tokens), 4)?)
}

/// Same metadata envelope used by both cold quotes and runtime admission:
/// temporary identity buffers + TWO original bindings + generated-ID clone.
pub(crate) fn metadata_backing(registered: &PalwTirClassV1, job: &PalwJobContextV2) -> Result<u64, Error> {
    add(input_auth_backing(registered)?, binding_backing(registered, job)?)
}

/// Quotes ALL original target claims at once: A1's local freeze interface owns
/// `Vec<Vec<i128>>`, so retaining native values does not eliminate this second
/// representation. Streaming one temporary tile alone is not that interface.
fn claims_backing(relations: u64, target_lanes: u64) -> Result<u64, Error> {
    add(mul(target_lanes, 16)?, mul(relations, size_of::<Vec<i128>>() as u64)?)
}

/// Cold-material quote: no adapter material slot is live. Runtime code uses
/// `quote_with_live` with its persisted owner-envelope maxima and current slot.
/// This is logical backing arithmetic, not a physical process/RSS guarantee.
/// No new wire/record digest is assigned. The quote does not authenticate peer
/// records. The immutable capture's execution pin must match the independently
/// resolved job which also derived `plan`; the constructor rechecks these facts.
///
/// Owned capture headers/shapes/native buffers and original run arrays use their
/// actual capacities. The private capture constructor enforces outer entries
/// capacity == length; `entries()` intentionally exposes only the immutable slice.
/// Future capture constructors must preserve that invariant or expose capacity.
/// All new index/claims/binding vectors must likewise use quoted exact capacity
/// or refuse excess actual capacity before retaining them.
#[allow(clippy::too_many_arguments)]
pub fn quote_typed_adapter(
    capture: &PrototypeTypedCapture,
    registered: &PalwTirClassV1,
    job: &PalwJobContextV2,
    plan: &ExpandedPlan,
    stages: StageReservations,
    limits: TypedAdapterLimits,
) -> Result<TypedAdapterQuote, Error> {
    quote_with_live(capture, registered, job, plan, stages, limits, LiveMaterialStage::default())
}

/// Runtime variant: include the existing material stage and explicit retained
/// owner envelopes in the same checked arithmetic as the cold public quote.
/// No source authentication, evaluation, signature or physical allocation is
/// performed or certified here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn quote_with_live(
    capture: &PrototypeTypedCapture,
    registered: &PalwTirClassV1,
    job: &PalwJobContextV2,
    plan: &ExpandedPlan,
    stages: StageReservations,
    limits: TypedAdapterLimits,
    live: LiveMaterialStage,
) -> Result<TypedAdapterQuote, Error> {
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
        || plan.job_context() != job
        || registered.layout != plan.step_space().layout
    {
        return Err(Error::Binding);
    }
    if limits.max_index_records == 0
        || limits.max_index_bytes == 0
        || limits.max_lookup_visits == 0
        || limits.max_proof_attempts == 0
        || limits.max_proof_leaf_reads == 0
        || limits.max_hash_work == 0
        || limits.max_legacy_leaves == 0
        || limits.max_tile_lanes == 0
        || limits.max_legacy_payload_bytes == 0
        || limits.max_claims_heap_bytes == 0
        || limits.max_material_bytes == 0
        || limits.max_additional_heap_bytes == 0
    {
        return Err(Error::Budget);
    }

    // These owners may already coexist when this runtime quote is requested.
    // Admit their COMBINED backing before any identity encoding or hashing;
    // separate per-component caps would permit the sum to exceed the ceiling.
    let records = count(capture.entries().len())?;
    if records > limits.max_index_records {
        return Err(Error::Budget);
    }
    let index_bytes = mul(records, size_of::<(RecordKey, usize)>() as u64)?;
    if index_bytes > limits.max_index_bytes {
        return Err(Error::Budget);
    }
    let external_stage_bytes = stage_backing(stages, live)?;
    let prehash_backing = add(index_bytes, add(metadata_backing(registered, job)?, external_stage_bytes)?)?;
    if prehash_backing > limits.max_additional_heap_bytes {
        return Err(Error::Budget);
    }

    let input_auth_scratch_bytes = input_auth_backing(registered)?;
    if input_auth_scratch_bytes > limits.max_additional_heap_bytes {
        return Err(Error::Budget);
    }

    // Cap the fresh metadata backing before comparing encodings/hashing. Job
    // tokenizer remains independent of the registration tokenizer in class_id.
    let binding_clone_bytes = binding_backing(registered, job)?;
    if binding_clone_bytes > limits.max_additional_heap_bytes {
        return Err(Error::Budget);
    }
    let encoded_length = borsh::object_length(&plan.step_space().program).map_err(|_| Error::Binding)?;
    if encoded_length != registered.program.len()
        || registered.program != plan.step_space().program.encode()
        || registered.class_id(&capture.origin().artifact) != capture.origin().class
        || job.shape_profile_id != capture.origin().class
        || job.context_hash() != capture.origin().execution
    {
        return Err(Error::Binding);
    }

    let summary = plan.summary();
    let run = capture.run();
    if records != capture.summary().records
        || summary.positions != capture.summary().positions
        || summary.node_instances != capture.summary().node_records
        || summary.legacy_relations != run.leaf_count
        || count(run.leaf_hashes.len())? != run.leaf_count
        || summary.relations == 0
        || summary.target_lanes < summary.relations
        || run.generated.len() != job.exact_decode_tokens as usize
        || run.logits_rows.len() != job.exact_decode_tokens as usize
    {
        return Err(Error::Geometry);
    }
    if records > limits.max_index_records || run.leaf_count == 0 || run.leaf_count > limits.max_legacy_leaves {
        return Err(Error::Budget);
    }
    let all_claims_bytes = claims_backing(summary.relations, summary.target_lanes)?;
    if all_claims_bytes > limits.max_claims_heap_bytes {
        return Err(Error::Budget);
    }

    // Expanded geometry already reserved each target as an i128 vector. Its
    // largest target is a conservative legacy-tile ceiling too; no all-leaf
    // traversal is needed to quote one legacy reconstruction.
    if summary.peak_target_bytes == 0 || !summary.peak_target_bytes.is_multiple_of(16) {
        return Err(Error::Geometry);
    }
    let tile = summary.peak_target_bytes / 16;
    let payload = add(26, mul(tile, 4)?)?;
    let depth = step_leg_max_opening_siblings_v1(run.leaf_count) as u64;
    if tile == 0
        || tile > limits.max_tile_lanes
        || payload > limits.max_legacy_payload_bytes
        || depth > u64::from(limits.max_path_siblings)
        || limits.max_proof_leaf_reads < run.leaf_count
        || limits.max_hash_work < add(add(mul(run.leaf_count, 4)?, mul(depth, 4)?)?, 16)?
    {
        return Err(Error::Budget);
    }
    // One i128 tile and its existing four-byte-lane G14 preimage coexist. Fixed
    // slack includes headers and scalar leaf/root comparison workspace.
    let legacy_scratch_bytes = add(mul(tile, 20)?, 512)?;
    // Two edge paths and the range output can retain Vec growth slack: reserve
    // eight path lengths, plus bounded lookup/geometry headers and at least
    // 512 bytes of scalar hash/root scratch. No full tree is reserved.
    let path_scratch_bytes = add(mul(add(depth, 1)?, 8 * 64)?, 4096)?;

    let mut capture_retained_bytes = add(size_of::<PrototypeTypedCapture>() as u64, backing::<TypedRecord>(capture.entries().len())?)?;
    for entry in capture.entries() {
        capture_retained_bytes = add(capture_retained_bytes, backing::<usize>(entry.shape.capacity())?)?;
        capture_retained_bytes = add(capture_retained_bytes, buffer_backing(&entry.values)?)?;
    }
    let mut run_retained_bytes = backing::<Hash64>(run.leaf_hashes.capacity())?;
    run_retained_bytes = add(run_retained_bytes, backing::<Vec<i32>>(run.logits_rows.capacity())?)?;
    for row in &run.logits_rows {
        run_retained_bytes = add(run_retained_bytes, backing::<i32>(row.capacity())?)?;
    }
    run_retained_bytes = add(run_retained_bytes, backing::<u32>(run.generated.capacity())?)?;

    let additional_peak_bytes = [
        index_bytes,
        all_claims_bytes,
        legacy_scratch_bytes,
        path_scratch_bytes,
        binding_clone_bytes,
        input_auth_scratch_bytes,
        external_stage_bytes,
    ]
    .into_iter()
    .try_fold(0, add)?;
    if additional_peak_bytes > limits.max_additional_heap_bytes {
        return Err(Error::Budget);
    }
    let total_live_bytes = add(add(capture_retained_bytes, run_retained_bytes)?, additional_peak_bytes)?;
    Ok(TypedAdapterQuote {
        capture_retained_bytes,
        run_retained_bytes,
        index_bytes,
        all_claims_bytes,
        legacy_scratch_bytes,
        path_scratch_bytes,
        binding_clone_bytes,
        input_auth_scratch_bytes,
        external_stage_bytes,
        additional_peak_bytes,
        total_live_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_i128_claims_coexist_with_headers_and_overflows_refuse() {
        assert_eq!(claims_backing(3, 9).unwrap(), 16 * 9 + 3 * size_of::<Vec<i128>>() as u64);
        assert!(claims_backing(u64::MAX, 1).is_err());
        assert!(claims_backing(1, u64::MAX).is_err());
        assert!(add(u64::MAX, 1).is_err());
        assert!(mul(u64::MAX, 16).is_err());
    }

    #[test]
    fn independent_live_buffers_add_to_persistent_owner_envelopes_and_overflow_refuses() {
        let declared = StageReservations {
            retained_preparation_bytes: 100,
            source_auth_bytes: 200,
            evaluator_bytes: 300,
            signature_workspace_bytes: 400,
        };
        let later = StageReservations {
            retained_preparation_bytes: 90,
            source_auth_bytes: 250,
            evaluator_bytes: 200,
            signature_workspace_bytes: 450,
        };
        let joined = declared.joined(later);
        assert_eq!(
            joined,
            StageReservations {
                retained_preparation_bytes: 100,
                source_auth_bytes: 250,
                evaluator_bytes: 300,
                signature_workspace_bytes: 450,
            }
        );
        let live = LiveMaterialStage { material_bytes: 500, source_auth_bytes: 220, evaluator_bytes: 350 };
        // Each persistent named owner keeps its maximum across calls, but the
        // independently allocated live auth/evaluator buffers are additional.
        // A1's live evaluator workspace includes its own target storage once.
        assert_eq!(stage_backing(joined, live).unwrap(), 100 + 450 + 250 + 300 + 500 + 220 + 350);
        assert_eq!(stage_backing(joined, LiveMaterialStage::default()).unwrap(), 100 + 450 + 250 + 300);
        let independent = StageReservations {
            retained_preparation_bytes: 0,
            source_auth_bytes: 100,
            evaluator_bytes: 100,
            signature_workspace_bytes: 0,
        };
        let independent_live = LiveMaterialStage { material_bytes: 0, source_auth_bytes: 100, evaluator_bytes: 100 };
        // The former component-wise max would claim 200, although four
        // independent 100-byte buffers are simultaneously retained.
        assert_eq!(stage_backing(independent, independent_live).unwrap(), 400);
        let overflow = StageReservations { retained_preparation_bytes: u64::MAX, ..declared };
        assert!(stage_backing(overflow, LiveMaterialStage::default()).is_err());
        assert!(stage_backing(declared, LiveMaterialStage { material_bytes: u64::MAX, ..live }).is_err());
    }

    #[test]
    fn native_retention_counts_capacity_in_its_original_width() {
        let mut values = Vec::<i128>::with_capacity(32);
        values.push(1 << 90);
        let actual = values.capacity() as u64 * 16;
        assert_eq!(buffer_backing(&Buf::I128(values)).unwrap(), actual);
        let mut values = Vec::<i8>::with_capacity(32);
        values.push(1);
        let actual = values.capacity() as u64;
        assert_eq!(buffer_backing(&Buf::I8(values)).unwrap(), actual);
    }
}
