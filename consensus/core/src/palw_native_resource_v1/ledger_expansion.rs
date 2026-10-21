//! Actual persisted-row expansion inventory. Raw byte residency is not approval
//! for the TIR/pipeline/flat objects those byte vectors encode.
use super::*;
use misaka_palw_kernel::{ledger::KernelLedgerV1, rows::LedgerRowsV1};
use std::{
    io::{self, Read},
    mem::size_of,
    ops::Deref,
};

/// The lease travels with the consumer and, when cached, with its shared Arc.
/// Deep clones use try_clone_v1; Clone cannot express a reservation refusal.
pub struct ReservedKernelLedgerV1 {
    pub(crate) ledger: KernelLedgerV1,
    pub(crate) lease: Option<NativeDemandLeaseV1>,
    projection_lease: Option<NativeDemandLeaseV1>,
}
impl Deref for ReservedKernelLedgerV1 {
    type Target = KernelLedgerV1;
    fn deref(&self) -> &Self::Target {
        &self.ledger
    }
}
// RFC-0006: there is deliberately NO `DerefMut`. A consensus fold mutates the ledger only through the typed, admitted methods
// (`kernel_ledger_mutation.rs`, and the fixed-membership `try_*` methods here); off-chain previews and tests say so explicitly with
// `preview_mut_v1`.
impl std::fmt::Debug for ReservedKernelLedgerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.ledger, f)
    }
}
impl ReservedKernelLedgerV1 {
    pub(crate) fn from_preflight_v1(ledger: KernelLedgerV1, lease: Option<NativeDemandLeaseV1>) -> Self {
        Self { ledger, lease, projection_lease: None }
    }
    /// Prevent method-resolution through Deref from silently selecting the
    /// underlying infallible deep Clone. Callers must handle capacity refusal.
    pub fn clone(&self) -> Result<Self, NativeCoverageV1> {
        self.try_clone_v1()
    }
    pub(crate) fn reserve_projection_v1(
        &mut self,
        route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    ) -> Result<(), NativeCoverageV1> {
        if enabled() {
            self.projection_lease = reserve_native_requests_v1(inventory_ledger_aux_inputs_v1(route)?)?;
        }
        Ok(())
    }
    pub fn try_clone_v1(&self) -> Result<Self, NativeCoverageV1> {
        let lease = reserve_native_requests_v1(ledger_clone_requests_v1(&self.ledger)?)?;
        Ok(Self { ledger: self.ledger.clone(), lease, projection_lease: None })
    }
    /// Reserve before an audited fixed-membership mutation. Keep every growth
    /// request in the same owner ticket; no detached temporary lease can expire
    /// while the new BTree membership is still retained by this ledger.
    pub(super) fn reserve_membership_v1<K, V>(&mut self, absent: bool) -> Result<(), NativeCoverageV1> {
        if !absent {
            return Ok(());
        }
        let bytes = size_of::<typed::BTreeNodeEnvelopeV1<K, V>>() as u64;
        if let Some(lease) = self.lease.as_mut() {
            lease.grow_memory_v1(bytes)
        } else {
            self.lease = reserve_native_requests_v1(bytes)?;
            Ok(())
        }
    }
    /// Kernel sync_bond only inserts one fixed BondRow; it never changes any
    /// other membership. A refusal leaves collateral and the map untouched.
    pub fn try_sync_bond_v1(&mut self, bond: [u8; 64], collateral: u64) -> Result<(), NativeCoverageV1> {
        self.reserve_membership_v1::<[u8; 64], misaka_palw_kernel::ledger::BondRowV1>(!self.ledger.bonds.contains_key(&bond))?;
        self.ledger.sync_bond(bond, collateral);
        Ok(())
    }
    pub fn try_attest_artifact_v1(&mut self, artifact: [u8; 64]) -> Result<(), NativeCoverageV1> {
        self.reserve_membership_v1::<[u8; 64], ()>(!self.ledger.attested_artifacts.contains(&artifact))?;
        self.ledger.attest_artifact(artifact);
        Ok(())
    }
    pub fn try_admit_optimistic_class_v1(&mut self, class: [u8; 64]) -> Result<(), String> {
        self.try_admit_optimistic_class_checked_v1(class).map_err(|why| why.to_string())
    }
    pub fn try_admit_optimistic_class_checked_v1(&mut self, class: [u8; 64]) -> Result<(), NativeDecodeErrorV1<String>> {
        // Match the kernel's refusal before reserving an insertion it cannot
        // perform. Its policy check and single admitted-set insert are audited.
        if self.ledger.opv.policy.is_none() {
            return self.ledger.admit_optimistic_class(class).map_err(|e| NativeDecodeErrorV1::Decode(e.to_string()));
        }
        self.reserve_membership_v1::<[u8; 64], ()>(!self.ledger.opv.admitted.contains(&class))
            .map_err(NativeDecodeErrorV1::Resource)?;
        self.ledger.admit_optimistic_class(class).map_err(|e| NativeDecodeErrorV1::Decode(e.to_string()))
    }
    /// Both tally implementations can insert only this existing claim's job
    /// pointer. Reserve it before the kernel mutates lifecycle/consumer state.
    /// Semantic refusals may retain an unused request conservatively.
    pub fn prepare_panel_tally_v1(&mut self, claim: &[u8; 64]) -> Result<(), NativeCoverageV1> {
        if let Some(job) = self.ledger.claims.get(claim).map(|row| row.job_id) {
            self.reserve_membership_v1::<[u8; 64], [u8; 64]>(!self.ledger.job_claims.contains_key(&job))?;
        }
        Ok(())
    }
    pub fn try_apply_paid_claim_tally_v2(&mut self, claim: &[u8; 64]) -> Result<(), String> {
        self.try_apply_paid_claim_tally_checked_v2(claim).map_err(|why| why.to_string())
    }
    pub fn try_apply_paid_claim_tally_checked_v2(&mut self, claim: &[u8; 64]) -> Result<(), NativeDecodeErrorV1<String>> {
        self.prepare_panel_tally_v1(claim).map_err(NativeDecodeErrorV1::Resource)?;
        self.ledger.apply_paid_claim_tally_v2(claim).map_err(NativeDecodeErrorV1::Decode)
    }
    /// **RFC-0006 (RFC06-1): the typed memory this ledger's tickets hold** — its admitted expansion plus every admitted growth since.
    /// Every row the ledger holds was admitted as at least one map-node envelope, so this bounds the rows a flush can materialize
    /// from it (`palw_kernel_route_fold_v1::flush`). `0` with tracking off.
    pub fn admitted_memory_v1(&self) -> u64 {
        self.lease
            .as_ref()
            .map_or(0, |lease| lease.memory_v1())
            .saturating_add(self.projection_lease.as_ref().map_or(0, |lease| lease.memory_v1()))
    }
    pub fn as_rebuilt_from_v1(self, rows: &LedgerRowsV1) -> Self {
        // The ticket remains alive while the derived indexes are reconstructed.
        let Self { ledger, lease, projection_lease } = self;
        Self { ledger: ledger.as_rebuilt_from_v1(rows), lease, projection_lease }
    }
}
fn enabled() -> bool {
    native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1()
}
fn ledger_clone_requests_v1(ledger: &KernelLedgerV1) -> Result<u64, NativeCoverageV1> {
    if !enabled() {
        return Ok(0);
    }
    let bound = ledger.native_heap_v1("KernelLedger consumer deep clone");
    let bound = bound.add::<KernelLedgerV1>(
        NativeCoverageV1::Bounded { bytes: (size_of::<ReservedKernelLedgerV1>() + size_of::<[usize; 3]>()) as u64 },
        "ledger inline",
    );
    let bytes = bound.bytes().ok_or(bound)?;
    // as_rebuilt reconstructs attested/opv-admitted/live indexes while the old
    // ones are present. Count their actual typed sets/maps, not serialized bytes.
    let scratch = ledger
        .attested_artifacts
        .native_heap_v1("ledger cache attested rebuild")
        .add::<KernelLedgerV1>(ledger.opv.native_heap_v1("ledger cache OPV live rebuild"), "ledger rebuild indexes");
    let scratch = scratch.bytes().ok_or(scratch)?;
    bytes.checked_add(scratch).ok_or_else(|| NativeCoverageV1::overflow::<KernelLedgerV1>("clone and rebuild indexes"))
}

pub(crate) fn reserve_ledger_expansion_v1(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    projection_v9: bool,
) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    if !enabled() {
        return Ok(None);
    }
    let mut requests = inventory_ledger_expansion_v1(&route.rows, projection_v9)?;
    requests = requests
        .checked_add(inventory_ledger_aux_inputs_v1(route)?)
        .ok_or_else(|| NativeCoverageV1::overflow::<KernelLedgerV1>("ledger plus actual aux consumer projection"))?;
    reserve_native_requests_v1(requests)
}

/// Coverage for the actual row codecs. Full validation remains the kernel's
/// responsibility; this reader never constructs a native program or accepts a
/// peer supplied estimate. Unsupported derived analysis is reported explicitly.
pub fn inventory_ledger_expansion_v1(rows: &LedgerRowsV1, projection_v9: bool) -> Result<u64, NativeCoverageV1> {
    if usize::BITS != 64 {
        return Err(NativeCoverageV1::unsupported::<KernelLedgerV1>(
            "ledger expansion",
            "pinned tree/Vec layout requires 64-bit target",
        ));
    }
    let mut sum = NativeWireShapeV1::default();
    for ((table, key), bytes) in rows {
        if projection_v9 && *table == misaka_palw_kernel::rows::TABLE_FLAT_CONFORMANCE_CLASSES_V9 {
            // No program or scope decode on the authenticated native projection;
            // the fixed owner/class IDs still enter the derived reverse indexes.
            sum = sum
                .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], ()>>())
                .and_then(|s| s.allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], std::collections::BTreeSet<[u8; 64]>>>()))
                .map_err(shape_error)?;
            continue;
        }
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let shape = scan_ledger_row_v1(*table, &mut reader).map_err(shape_error)?;
        if reader.remaining() != 0 {
            return Err(shape_error(wire::wire_bad("ledger row has trailing bytes")));
        }
        // The original keys are byte vectors. from_rows constructs decoded keys;
        // a conservative fixed-key node envelope covers ordinary row keys.
        if key.len() > 137 {
            return Err(NativeCoverageV1::unsupported::<KernelLedgerV1>(
                "ledger key shape",
                "key exceeds all audited ledger tuple keys",
            ));
        }
        sum = sum.merge(shape).map_err(shape_error)?;
    }
    // A composite can reference the same class at each of its actual stages.
    // Kernel forbids composite nesting and memory components, so one copy of
    // the complete primary decoded pool per stage conservatively covers every
    // permitted Model/Retrieval component, independent of peer-declared bytes.
    // Read the pre-composite pool once: repeated references never recursively
    // multiply other composites' already-expanded component caches.
    let primary_pool = sum.heap;
    sum = sum
        .allocation(
            sum.composite_components,
            usize::try_from(primary_pool)
                .map_err(|_| NativeCoverageV1::overflow::<KernelLedgerV1>("composite primary pool address width"))?,
        )
        .map_err(shape_error)?;
    // Kernel from_rows also rebuilds OPV live indexes, verifier-pay books and
    // conformance reverse indexes. Each is keyed by fixed ledger IDs; bounded
    // by the actual count of primary rows, with one node per referenced row.
    let count = rows.len() as u64;
    let indexes = size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], std::collections::BTreeSet<[u8; 64]>>>()
        + size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], ()>>()
        + size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], u64>>();
    sum = sum.allocation(count, indexes).map_err(shape_error)?;
    // from_rows clones the binary-owned template schedule/descriptors. Its
    // owner profile is separate from remote data and encoded byte limits.
    let row_copy = rows.native_heap_v1("ledger projection row copies");
    let row_copy = row_copy.bytes().ok_or(row_copy)?;
    sum.heap
        .checked_add(row_copy)
        .and_then(|v| v.checked_add(template_requests_v1()?))
        .ok_or_else(|| NativeCoverageV1::overflow::<KernelLedgerV1>("ledger/template/projection coexisting requests"))
}
fn shape_error(error: io::Error) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<KernelLedgerV1>("actual persisted ledger codec", &error.to_string())
}

fn nested_blob<T: NativeWireScanV1, R: Read>(reader: &mut NativeWireReaderV1<R>, cap: u64) -> io::Result<NativeWireShapeV1> {
    let bytes = reader.word()? as u64;
    if bytes > cap || bytes > reader.remaining() {
        return Err(wire::wire_bad("nested ledger codec exceeds exact frame/cap"));
    }
    let mut frame = NativeWireReaderV1::new((&mut *reader).take(bytes), bytes);
    let shape = T::scan_native_wire_v1(&mut frame)?;
    if frame.remaining() != 0 {
        return Err(wire::wire_bad("nested ledger codec trailing bytes"));
    }
    shape.allocation(bytes.checked_mul(2).ok_or_else(|| wire::wire_bad("nested byte Vec overflow"))?, 1)
}
impl NativeWireScanV1 for misaka_palw_kernel::state::ClassRecordV1 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        <[u8; 64]>::scan_native_wire_v1(r)?;
        let sum = program_blob::<misaka_palw_tir::program::TirProgramV1, _>(r, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)?;
        sum.merge(misaka_palw_kernel::plan::VerificationPlanV1::scan_native_wire_v1(r)?)?
            .merge(misaka_palw_kernel::trace::ParamCommitmentsV1::scan_native_wire_v1(r)?)
            .and_then(class_record_copies_v1)
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::state::PipelineClassRecordV1 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        <[u8; 64]>::scan_native_wire_v1(r)?;
        let mut sum =
            nested_blob::<misaka_palw_tir::pipeline::TirPipelineV1, _>(r, misaka_palw_tir::pipeline::MAX_PIPELINE_BYTES as u64)?;
        let count = r.word()? as u64;
        if count > misaka_palw_tir::pipeline::MAX_STAGES as u64 {
            return Err(wire::wire_bad("pipeline program count exceeds stage cap"));
        }
        for _ in 0..count {
            sum = sum.merge(program_blob::<misaka_palw_tir::program_v2::TirProgramV2, _>(
                r,
                misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64,
            )?)?;
        }
        sum = sum.allocation(
            count.checked_mul(2).ok_or_else(|| wire::wire_bad("pipeline program allocation overflow"))?,
            size_of::<(Vec<u8>, misaka_palw_tir::program_v2::TirProgramV2)>(),
        )?;
        sum = sum.merge(misaka_palw_kernel::pipeline::PipelinePlanV1::scan_native_wire_v1(r)?)?;
        sum = sum.merge(Vec::<misaka_palw_kernel::trace::ParamCommitmentsV1>::scan_native_wire_v1(r)?)?;
        sum.merge(Option::<misaka_palw_kernel::job::DecodeRuleV1>::scan_native_wire_v1(r)?).and_then(class_record_copies_v1)
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::flat_candidate7::FlatCandidateRecordV7 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        <[u8; 64]>::scan_native_wire_v1(r)?;
        let sum = misaka_palw_kernel::flat_candidate7::FlatCandidatePlanV7::scan_native_wire_v1(r)?;
        sum.merge(flat_program_blob_v1::<misaka_palw_tir::program::TirProgramV1, _>(
            r,
            misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64,
        )?)?
        .merge(misaka_palw_kernel::trace::ParamCommitmentsV1::scan_native_wire_v1(r)?)
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::flat_candidate9::FlatCandidateRecordV9 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        <[u8; 64]>::scan_native_wire_v1(r)?;
        let mut sum = misaka_palw_kernel::flat_candidate9::FlatCandidatePlanV9::scan_native_wire_v1(r)?;
        sum = sum.merge(flat_program_blob_v1::<misaka_palw_tir::program::TirProgramV1, _>(
            r,
            misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64,
        )?)?;
        sum = sum.merge(misaka_palw_kernel::trace::ParamCommitmentsV1::scan_native_wire_v1(r)?)?;
        sum = sum.merge(nested_blob::<misaka_palw_kernel::flat_scope9::FlatResourceScopeV9, _>(
            r,
            misaka_palw_kernel::flat_candidate9::FLAT_CANDIDATE_METADATA_MAX_BYTES_V9 as u64,
        )?)?;
        sum.merge(nested_blob::<misaka_palw_kernel::flat_replay9::FlatReplayEnvelopeV9, _>(
            r,
            misaka_palw_kernel::flat_candidate9::FLAT_CANDIDATE_METADATA_MAX_BYTES_V9 as u64,
        )?)
    }
}
include!("ledger_rows_generated.rs");

/// Constructor profile for palw_kernel_route_template_typed_v1 and from_rows:
/// 5 known descriptors plus one transient typed descriptor; constructor family
/// Vecs are at most twice ALL.len after the MediaPipeline push. Four simultaneous
/// schedule/known copies cover template, rebuilt ledger, config-root encoding
/// and local constructor temporaries. Update with constructor changes.
fn template_requests_v1() -> Option<u64> {
    use misaka_palw_kernel::{
        descriptor::{FamilySupportV1, KernelDescriptorV1, KernelStatusV1},
        family::ConstraintFamilyV1,
    };
    let descriptors = 6u64;
    let families = (ConstraintFamilyV1::ALL.len() as u64).checked_mul(2)?;
    let descriptor_vectors = descriptors.checked_mul(size_of::<KernelDescriptorV1>() as u64)?;
    let family_vectors = descriptors.checked_mul(families)?.checked_mul(size_of::<FamilySupportV1>() as u64)?;
    let schedule = 16u64.checked_mul(size_of::<([u8; 64], KernelStatusV1)>() as u64)?;
    let fields = descriptor_vectors.checked_add(family_vectors)?.checked_add(schedule)?;
    fields.checked_mul(4)?.checked_add(size_of::<KernelLedgerV1>() as u64)
}
pub(crate) fn inventory_ledger_aux_inputs_v1(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
) -> Result<u64, NativeCoverageV1> {
    let mut shape = NativeWireShapeV1::default();
    for ((table, key), bytes) in &route.aux {
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let row = super::wire_kernel_aux::scan_native_aux_row_v1(*table, key, &mut reader).map_err(shape_error)?;
        if reader.remaining() != 0 {
            return Err(shape_error(wire::wire_bad("consumer aux row trailing bytes")));
        }
        shape = shape
            .merge(row)
            .and_then(|s| {
                s.allocation(
                    1,
                    size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::claim_paid_v2::PaidClaimConsumerV2>>(),
                )
            })
            .map_err(shape_error)?;
    }
    // Current row plus authenticated class/job/round/lock dependency projection.
    let projections =
        shape.heap.checked_mul(2).ok_or_else(|| NativeCoverageV1::overflow::<KernelLedgerV1>("coexisting aux projections"))?;
    // Restore's independent-source pending set borrows one Digest per actual
    // persisted source row (table30). It is a distinct simultaneous workspace.
    let sources = route.rows.iter().filter(|((table, _), _)| *table == 30).count() as u64;
    let pending = sources
        .checked_mul(size_of::<typed::BTreeNodeEnvelopeV1<&[u8; 64], ()>>() as u64)
        .ok_or_else(|| NativeCoverageV1::overflow::<KernelLedgerV1>("pending independent source restore set"))?;
    projections
        .checked_add(pending)
        .ok_or_else(|| NativeCoverageV1::overflow::<KernelLedgerV1>("coexisting aux and pending-source projections"))
}

/// Called by the optional cache only; reservation refusal must skip caching.
pub(crate) fn ledger_cache_clone_v1(ledger: &KernelLedgerV1) -> Result<ReservedKernelLedgerV1, NativeCoverageV1> {
    let lease = reserve_native_requests_v1(ledger_clone_requests_v1(ledger)?)?;
    Ok(ReservedKernelLedgerV1 { ledger: ledger.clone(), lease, projection_lease: None })
}

/// TIR normal-form/gate workspace: actual parsed collection count bounds nodes,
/// params, states, tensor axes and input references. Validated schedules contain
/// at most MAX_LAYERS+2 occurrences. No tensor values/parameter artifact bytes
/// are loaded by class reconstruction. Error paths stop before gate traversal.
/// Each cone has at most N nodes and N*MAX_NODE_INPUTS distinct operand leaves;
/// all cone and state closures are therefore a checked typed quadratic sum.
fn program_blob<T: NativeWireScanV1, R: Read>(r: &mut NativeWireReaderV1<R>, cap: u64) -> io::Result<NativeWireShapeV1> {
    use misaka_palw_kernel::plan::{PlanRelationV1, StateBoundaryV1};
    use misaka_palw_tir::{
        admit::{ConeV1, CostV1, LeafV1, StateCkptV1},
        interval::Interval,
        program::{MAX_LAYERS, MAX_NODE_INPUTS},
        types::{Dim, MAX_RANK, TensorType},
        validate::BlockInfo,
    };
    let before = r.remaining();
    let mut shape = nested_blob::<T, R>(r, cap)?;
    let encoded = before
        .checked_sub(r.remaining())
        .and_then(|n| n.checked_sub(4))
        .ok_or_else(|| wire::wire_bad("program canonical frame accounting"))?;
    // decode_canonical re-encodes for exact equality. Pinned Borsh 1.x to_vec
    // starts with DEFAULT_SERIALIZER_CAPACITY=1024, then grows geometrically.
    shape = shape.allocation(
        encoded.max(1024).checked_mul(2).ok_or_else(|| wire::wire_bad("canonical program re-encoding capacity overflow"))?,
        1,
    )?;
    let n = shape.collection_entries.checked_add(1).ok_or_else(|| wire::wire_bad("program workspace count overflow"))?;
    let twice = n.checked_mul(2).ok_or_else(|| wire::wire_bad("program Vec growth overflow"))?;
    let occurrences = (MAX_LAYERS as u64).checked_add(2).ok_or_else(|| wire::wire_bad("program schedule overflow"))?;
    let rank = MAX_RANK;
    let inputs = MAX_NODE_INPUTS;
    let per_node = size_of::<BlockInfo>()
        + size_of::<PlanRelationV1>()
        + size_of::<StateBoundaryV1>()
        + size_of::<Interval>()
        + size_of::<CostV1>()
        + size_of::<ConeV1>()
        + size_of::<StateCkptV1>()
        + (inputs + 4) * size_of::<TensorType>()
        + (inputs + 4) * rank * size_of::<Dim>()
        + inputs * (size_of::<u64>() + size_of::<u8>())
        + size_of::<usize>()
        + size_of::<bool>()
        + size_of::<typed::BTreeNodeEnvelopeV1<u16, ()>>()
        + size_of::<typed::BTreeNodeEnvelopeV1<(u8, u16), ()>>()
        + size_of::<typed::BTreeNodeEnvelopeV1<&str, std::collections::BTreeSet<u16>>>()
        + size_of::<typed::BTreeNodeEnvelopeV1<(misaka_palw_tir::types::DType, Vec<u32>, Vec<u8>), ()>>();
    shape = shape.allocation(twice, per_node)?;
    // Cone node/leaf/closure vectors and their temporary deduplicating trees.
    let cones = twice.checked_mul(n).ok_or_else(|| wire::wire_bad("program cone workspace overflow"))?;
    shape = shape.allocation(
        cones,
        2 * size_of::<u16>() + inputs * size_of::<LeafV1>() + inputs * size_of::<typed::BTreeNodeEnvelopeV1<LeafV1, ()>>(),
    )?;
    // Occurrence modes, gate masks, prepared wiring and occurrence/node price
    // memoization retain geometry only. Nested shape Vecs have rank<=MAX_RANK.
    let unrolled = twice.checked_mul(occurrences).ok_or_else(|| wire::wire_bad("program unrolled workspace overflow"))?;
    shape = shape.allocation(
        unrolled,
        size_of::<(u8, u16)>()
            + size_of::<Vec<bool>>()
            + size_of::<bool>()
            + size_of::<u8>()
            + size_of::<u64>()
            + size_of::<typed::BTreeNodeEnvelopeV1<(u8, u16, Vec<Option<u8>>), (u64, u64)>>()
            + inputs * size_of::<Option<u8>>(),
    )?;
    // Interval analysis decodes constants into i128 lanes; the actual parsed
    // byte-vector element count bounds lane count even for 1-byte constants.
    shape = shape
        .allocation(shape.byte_entries.checked_mul(2).ok_or_else(|| wire::wire_bad("constant lanes overflow"))?, size_of::<i128>())?;
    // v2 validation lifts a v1 view and the gate independently derives a plan;
    // retain up to four typed codec/analysis passes plus their parsed program.
    shape.heap = shape.heap.checked_mul(4).ok_or_else(|| wire::wire_bad("coexisting program analysis overflow"))?;
    Ok(shape)
}

/// Actual field counts from an allocation-free second pass over the same wire.
/// These are allocation bounds, not semantic validation or scheduling authority.
#[derive(Clone, Copy, Debug)]
struct StepSpaceShapeV1 {
    blocks: u64,
    nodes: u64,
    layers: u64,
    fixed_global: u64,
    fixed_layer: u64,
    hist_global: u64,
    hist_layer: u64,
    params: u64,
}
fn step_space_shape_v1(bytes: &[u8]) -> io::Result<StepSpaceShapeV1> {
    use misaka_palw_tir::{program::{ParamDecl, ConstDecl, Node}, types::{DType, TensorType}};
    let mut r = NativeWireReaderV1::new(bytes, bytes.len() as u64);
    if r.short()? != misaka_palw_tir::program::TIR_PROGRAM_VERSION_V1 {
        return Err(wire::wire_bad("unknown TIR program version for StepSpace shape"));
    }
    r.skip(64 + 4 + 4)?;
    let mut out = StepSpaceShapeV1 { blocks:0,nodes:0,layers:0,fixed_global:0,fixed_layer:0,hist_global:0,hist_layer:0,params:0 };
    out.params = u64::from(r.word()?);
    for _ in 0..out.params { ParamDecl::scan_native_wire_v1(&mut r)?; }
    let constants = r.word()?;
    for _ in 0..constants { ConstDecl::scan_native_wire_v1(&mut r)?; }
    let states = r.word()?;
    for _ in 0..states {
        String::scan_native_wire_v1(&mut r)?;
        let fixed = match r.byte()? {
            0 => { r.skip(16)?; true },
            1 => { r.skip(4)?; false },
            _ => return Err(wire::wire_bad("unknown state kind for StepSpace shape")),
        };
        DType::scan_native_wire_v1(&mut r)?;
        Vec::<u32>::scan_native_wire_v1(&mut r)?;
        let per_layer = match r.byte()? { 0=>false,1=>true,_=>return Err(wire::wire_bad("invalid state per_layer boolean")) };
        let count=match (fixed,per_layer) {
            (true,false)=>&mut out.fixed_global,(true,true)=>&mut out.fixed_layer,
            (false,false)=>&mut out.hist_global,(false,true)=>&mut out.hist_layer,
        };
        *count=count.checked_add(1).ok_or_else(||wire::wire_bad("StepSpace state count overflow"))?;
    }
    out.blocks=u64::from(r.word()?);
    for _ in 0..out.blocks {
        String::scan_native_wire_v1(&mut r)?;
        Vec::<TensorType>::scan_native_wire_v1(&mut r)?;
        let nodes=u64::from(r.word()?);
        out.nodes=out.nodes.checked_add(nodes).ok_or_else(||wire::wire_bad("StepSpace node count overflow"))?;
        for _ in 0..nodes { Node::scan_native_wire_v1(&mut r)?; }
        Vec::<u16>::scan_native_wire_v1(&mut r)?;
    }
    r.byte()?; // schedule.pre
    out.layers=u64::from(r.word()?);
    r.skip(out.layers)?;
    r.byte()?; // schedule.post
    r.skip(2+64)?; // logits and logits scheme
    if r.remaining()!=0 { return Err(wire::wire_bad("StepSpace shape trailing bytes")); }
    Ok(out)
}

/// Borrowed scanner output only. No public fields/constructor: a caller cannot
/// substitute a budget or declared count for the actual canonical-byte scan.
/// This quote owns no lease and certifies neither semantic validity nor Source.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeTirProgramShapeV1 {
    program_requests: u64,
    structural_items: u64,
    step_space: StepSpaceShapeV1,
}
impl NativeTirProgramShapeV1 {
    pub(crate) fn program_requests_v1(&self) -> u64 { self.program_requests }
    pub(crate) fn structural_items_v1(&self) -> u64 { self.structural_items }
    pub(crate) fn step_space_counts_v1(&self) -> (u64,u64,u64,u64,u64,u64,u64) {
        let s=self.step_space;
        (s.blocks,s.nodes,s.layers,s.fixed_global,s.fixed_layer,s.hist_global,s.hist_layer)
    }
    #[cfg(test)]
    pub(crate) fn parameter_count_v1(&self)->u64 { self.step_space.params }
}
/// Same audited program codec/analysis walk used by ledger restoration.
/// The stack prefix supplies framing; actual bytes are scanned without a copy.
pub(crate) fn standalone_tir_program_shape_v1(bytes: &[u8]) -> io::Result<NativeTirProgramShapeV1> {
    let len = u32::try_from(bytes.len()).map_err(|_| wire::wire_bad("TIR program frame exceeds u32"))?;
    let prefix = len.to_le_bytes();
    let source = std::io::Cursor::new(prefix).chain(bytes);
    let mut reader = NativeWireReaderV1::new(source, u64::from(len) + 4);
    let shape = program_blob::<misaka_palw_tir::TirProgramV1, _>(
        &mut reader, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)?;
    if reader.remaining() != 0 { return Err(wire::wire_bad("standalone TIR program trailing bytes")); }
    let program_requests = shape.heap.checked_add(size_of::<NativeRetainedV1<misaka_palw_tir::TirProgramV1>>() as u64)
        .ok_or_else(|| wire::wire_bad("standalone TIR owner framing overflow"))?;
    // Vec<u8> is tracked separately by the existing wire scanner. Include it:
    // in particular schedule.layers is byte-valued and still bounds occurrences.
    let structural_items = shape.collection_entries.checked_add(shape.byte_entries)
        .ok_or_else(|| wire::wire_bad("TIR structural item count overflow"))?;
    Ok(NativeTirProgramShapeV1 { program_requests, structural_items, step_space: step_space_shape_v1(bytes)? })
}
/// Source-only decoded-program + normal-form validator profile. The general
/// program/gate/ledger profile above deliberately keeps its wider semantics.
/// Source executes neither court cone/range analysis nor gate price unrolling.
pub(crate) fn source_tir_program_shape_v1(bytes:&[u8])
    ->Result<(NativeTirProgramShapeV1,u64),NativeDecodeErrorV1<String>> {
    use misaka_palw_tir::{program::{TirProgramV1,MAX_CARRY,MAX_NODE_INPUTS},types::{DType,Dim,TensorType},validate::BlockInfo};
    use std::collections::BTreeSet;
    use super::typed::BTreeNodeEnvelopeV1 as Tree;
    let decode=|e:io::Error| {
        match e.kind() {
            io::ErrorKind::OutOfMemory => NativeDecodeErrorV1::Resource(
                NativeCoverageV1::overflow::<TirProgramV1>("Source program wire shape")),
            io::ErrorKind::TimedOut | io::ErrorKind::Unsupported => NativeDecodeErrorV1::Resource(
                NativeCoverageV1::unsupported::<TirProgramV1>("Source program wire shape", "local wire exploration/platform coverage")),
            _ => NativeDecodeErrorV1::Decode(e.to_string()),
        }
    };
    // Existing complete versioned scanner and metadata authority; no decoded
    // caller count, alternate grammar or hand-selected program subset.
    let shape=standalone_tir_program_shape_v1(bytes).map_err(decode)?;
    let mut r=NativeWireReaderV1::new(bytes,bytes.len() as u64);
    let raw=TirProgramV1::scan_native_wire_v1(&mut r).map_err(decode)?;
    if r.remaining()!=0 {return Err(NativeDecodeErrorV1::Decode("Source program shape trailing bytes".into()));}
    let err=||NativeDecodeErrorV1::Resource(NativeCoverageV1::overflow::<TirProgramV1>("Source normal-form cardinality"));
    // Includes every actual nested collection/byte element. It is a linear
    // bound on any unvalidated shape rank/perm length, not MAX_RANK admission.
    let n=shape.structural_items_v1();
    let (blocks,nodes,_,fg,fl,hg,hl)=shape.step_space_counts_v1();
    let states=fg.checked_add(fl).and_then(|v|v.checked_add(hg)).and_then(|v|v.checked_add(hl)).ok_or_else(err)?;
    let mut q=TirNativeRequestsV1::new();
    let mut add=|bytes|q.add_v1(bytes).map_err(NativeDecodeErrorV1::Resource);
    // Parsed program and validator's cloned constant payloads coexist. A full
    // second raw payload envelope overbounds only those const shape/data clones.
    add(raw.heap.checked_mul(2).ok_or_else(err)?)?;
    q.exact_v1::<TirProgramV1>(1).map_err(NativeDecodeErrorV1::Resource)?;
    q.exact_v1::<u8>((bytes.len() as u64).max(1024).checked_mul(2).ok_or_else(err)?).map_err(NativeDecodeErrorV1::Resource)?;
    // Declaration sets stay live through validation; n bounds each actual
    // declaration family. Node envelopes include the constant key's inline Vecs.
    q.exact_v1::<Tree<&str,()>>(n.checked_add(states).ok_or_else(err)?).map_err(NativeDecodeErrorV1::Resource)?;
    q.exact_v1::<Tree<(DType,Vec<u32>,Vec<u8>),()>>(n).map_err(NativeDecodeErrorV1::Resource)?;
    q.vec_v1::<BlockInfo>(blocks).map_err(NativeDecodeErrorV1::Resource)?;
    // Carry clone occurs BEFORE per-node output shape validation. Rank may be
    // malformed; use n, never MAX_RANK. At most MAX_CARRY carried tensors are
    // checked before the retained carry and temporary layer carry-out clones.
    let carry=(MAX_CARRY as u64).checked_mul(2).ok_or_else(err)?;
    q.vec_v1::<TensorType>(carry).map_err(NativeDecodeErrorV1::Resource)?;
    q.vec_v1::<Dim>(carry.checked_mul(n).ok_or_else(err)?).map_err(NativeDecodeErrorV1::Resource)?;
    // One node's operand shapes, plus inference's overlapping intermediate
    // shape vectors. Gather can temporarily concatenate two actual ranks;
    // Select holds two broadcasts; HistAppend also prepends H to a row clone.
    q.vec_v1::<TensorType>(MAX_NODE_INPUTS as u64).map_err(NativeDecodeErrorV1::Resource)?;
    q.vec_v1::<Dim>((MAX_NODE_INPUTS as u64).checked_mul(n).ok_or_else(err)?).map_err(NativeDecodeErrorV1::Resource)?;
    let inferred=n.checked_mul(2).and_then(|v|v.checked_add(1)).ok_or_else(err)?;
    for _ in 0..4 {q.vec_v1::<Dim>(inferred).map_err(NativeDecodeErrorV1::Resource)?;}
    q.vec_v1::<bool>(n).map_err(NativeDecodeErrorV1::Resource)?; // transpose seen
    // Per-block effect sets; each state index inserts once in either set.
    q.exact_v1::<Tree<u16,()>>(states.checked_mul(2).ok_or_else(err)?).map_err(NativeDecodeErrorV1::Resource)?;
    q.vec_v1::<bool>(nodes).map_err(NativeDecodeErrorV1::Resource)?;
    // Liveness expands each node once. Initial roots <=nodes+MAX_CARRY+1;
    // every pushed edge is one actual input element, whose count <=n.
    let dfs=nodes.checked_add(n).and_then(|v|v.checked_add(MAX_CARRY as u64+1)).ok_or_else(err)?;
    q.vec_v1::<usize>(dfs).map_err(NativeDecodeErrorV1::Resource)?;
    // Three-category used map, its inner ID sets, and one cloned set coexist.
    q.exact_v1::<Tree<&str,BTreeSet<u16>>>(3).map_err(NativeDecodeErrorV1::Resource)?;
    q.exact_v1::<Tree<u16,()>>(n.checked_mul(2).ok_or_else(err)?).map_err(NativeDecodeErrorV1::Resource)?;
    // Failure formatting: a Dim is at most "Fixed(4294967295)" (17 bytes)
    // plus separator <=20; output+Gather inferred shapes total <=3*n. Other
    // variable debug output is u8 perm (<=5 bytes/entry), or checked <=128B
    // names. 1024 covers fixed literals/indices. Four growing writer envelopes
    // cover nested shape_err/TirError/StepError to_string owners and copies.
    let diagnostic=n.checked_mul(3).and_then(|v|v.checked_mul(20)).and_then(|v|v.checked_add(1024)).ok_or_else(err)?;
    for _ in 0..4 {q.vec_v1::<u8>(diagnostic).map_err(NativeDecodeErrorV1::Resource)?;}
    Ok((shape,q.bytes_v1()))
}

#[cfg(test)]
mod source_program_profile_tests_v1 {
    use super::*;
    fn local_decode(bytes:&[u8],limit:u64)
        ->Result<NativeRetainedV1<misaka_palw_tir::TirProgramV1>,NativeDecodeErrorV1<String>> {
        let (_,q)=source_tir_program_shape_v1(bytes)?;
        let checker=NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1{memory_bytes:limit,database_bytes:0});
        let result=(|| {
            let lease=checker.reserve_requests_v1(q,0).map_err(NativeDecodeErrorV1::Resource)?;
            let program=misaka_palw_tir::TirProgramV1::decode_canonical(bytes).map_err(|e|NativeDecodeErrorV1::Decode(e.to_string()))?;
            Ok(NativeRetainedV1::from_owner_reserved_v1(program,Some(lease)))
        })();
        if result.is_err() {assert_eq!(checker.outstanding(),(0,0));}
        result
    }
    #[test]
    fn source_normal_form_profile_real_program_and_exact_quota_lifetime() {
        for committed in [false,true] {
            let f=crate::palw_tir_public_claim_v1::palw_tir_public_fixed_history_frame_fixture_v1(
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,committed);
            let bytes=&f.execution.binding.class.program;
            let (shape,q)=source_tir_program_shape_v1(bytes).unwrap();
            assert!(q>bytes.len() as u64);
            assert!(q<shape.program_requests_v1(),"Source-only profile excludes gate/cone/range allocations");
            assert!(matches!(local_decode(bytes,q-1),Err(NativeDecodeErrorV1::Resource(_))));
            let checker=NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1{memory_bytes:q,database_bytes:0});
            let lease=checker.reserve_requests_v1(q,0).unwrap();
            let decoded=misaka_palw_tir::TirProgramV1::decode_canonical(bytes).unwrap();
            let held=NativeRetainedV1::from_owner_reserved_v1(decoded,Some(lease));
            assert_eq!(held.encode(),*bytes);assert_eq!(checker.outstanding(),(q,0));
            assert!(checker.reserve_requests_v1(1,0).is_err());
            drop(held);assert_eq!(checker.outstanding(),(0,0));
            drop(checker.reserve_requests_v1(q,0).unwrap());assert_eq!(checker.outstanding(),(0,0));
        }
    }
    #[test]
    fn source_normal_form_malformed_wire_rank_and_duplicate_carry_release_quota() {
        use misaka_palw_tir::{program::MAX_CARRY,types::{DType,Dim}};
        let x=crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let mut p=x.binding.class.decode_program().unwrap();
        let pre=p.schedule.pre as usize;
        // Valid Borsh, invalid normal form. Carry cloning happens before the
        // node's rank check; repeated indices duplicate this unvalidated shape.
        p.blocks[pre].nodes[0].out.dtype=DType::I32;
        p.blocks[pre].nodes[0].out.shape=vec![Dim::Fixed(1);4096];
        p.blocks[pre].carry_out=vec![0;MAX_CARRY];
        let bytes=p.encode();let (_,q)=source_tir_program_shape_v1(&bytes).unwrap();
        assert!(matches!(local_decode(&bytes,q-1),Err(NativeDecodeErrorV1::Resource(_))));
        assert!(matches!(local_decode(&bytes,q),Err(NativeDecodeErrorV1::Decode(_))));
        let mut unknown=bytes.clone();unknown[..2].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(matches!(source_tir_program_shape_v1(&unknown),Err(NativeDecodeErrorV1::Decode(_))));
        assert!(source_tir_program_shape_v1(&bytes[..bytes.len()-1]).is_err());
    }
    #[test]
    fn source_normal_form_repeated_liveness_edges_are_linear_and_reject_dead_node() {
        use misaka_palw_tir::{DType,Ref,TensorType,builder::ProgramBuilder,program::HISTORY_BOUND_V1_HELD};
        let mut b=ProgramBuilder::new(256,HISTORY_BOUND_V1_HELD);
        let scores=b.param("scores",DType::I32,&[4],false);
        let mut pre=b.block("pre",vec![]);
        let mut x=scores;
        for _ in 0..128 {x=pre.add(x,x,DType::I32);}
        let pre=pre.finish(&[x]);
        let mut post=b.block("post",vec![TensorType::fixed(DType::I32,&[4])]);
        let y=post.cast(Ref::CarryIn(0),DType::I32);post.commit(y);let post=post.finish(&[]);
        let p=b.finish(pre,vec![],post,0);let bytes=p.encode();
        let (_,q)=source_tir_program_shape_v1(&bytes).unwrap();
        assert_eq!(local_decode(&bytes,q).unwrap().encode(),bytes);
        let mut dead=p;
        let mut extra=dead.blocks[dead.schedule.post as usize].nodes[0].clone();extra.commit=false;
        dead.blocks[dead.schedule.post as usize].nodes.push(extra);
        let bytes=dead.encode();let (_,q)=source_tir_program_shape_v1(&bytes).unwrap();
        assert!(matches!(local_decode(&bytes,q),Err(NativeDecodeErrorV1::Decode(s)) if s.contains("dead")));
    }
}

pub(crate) fn standalone_tir_program_requests_v1(bytes: &[u8]) -> io::Result<u64> {
    Ok(standalone_tir_program_shape_v1(bytes)?.program_requests_v1())
}
pub(crate) fn standalone_program_requests_v1<T: NativeWireScanV1>(bytes: &[u8], cap: u64) -> io::Result<u64> {
    let len = u32::try_from(bytes.len()).map_err(|_| wire::wire_bad("TIR program frame exceeds u32"))?;
    let prefix = len.to_le_bytes();
    let source = std::io::Cursor::new(prefix).chain(bytes);
    let mut reader = NativeWireReaderV1::new(source, u64::from(len) + 4);
    let shape = program_blob::<T, _>(&mut reader, cap)?;
    if reader.remaining() != 0 {
        return Err(wire::wire_bad("standalone TIR program trailing bytes"));
    }
    shape
        .heap
        .checked_add(size_of::<NativeRetainedV1<T>>() as u64)
        .ok_or_else(|| wire::wire_bad("standalone TIR owner framing overflow"))
}

fn class_record_copies_v1(mut shape: NativeWireShapeV1) -> io::Result<NativeWireShapeV1> {
    // Parsed record and stored row clones coexist during from_rows. The known
    // descriptor's family Vec is cloned for each stored derived class row.
    shape.heap = shape.heap.checked_mul(2).ok_or_else(|| wire::wire_bad("class record coexistence overflow"))?;
    shape.allocation(
        (misaka_palw_kernel::family::ConstraintFamilyV1::ALL.len() as u64) * 2,
        size_of::<misaka_palw_kernel::descriptor::FamilySupportV1>(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_cache_keeps_ticket_until_last_arc_and_codec_does_not_include_it() {
        let policy =
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(crate::Hash64::from_u64_word(1), crate::Hash64::from_u64_word(2));
        let mut route = crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(policy, None, Default::default());
        let before = borsh::to_vec(&route).unwrap();
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 32, database_bytes: 0 });
        let ticket = checker.reserve_requests_v1(32, 0).unwrap();
        let ledger = route.template();
        route.ledger_cache.set(ReservedKernelLedgerV1::from_preflight_v1(ledger, Some(ticket)));
        let sibling = route.ledger_cache.clone();
        assert_eq!(borsh::to_vec(&route).unwrap(), before);
        route.ledger_cache.clear();
        assert_eq!(checker.outstanding(), (32, 0));
        drop(sibling);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn actual_nested_frame_rejects_truncate_and_unbounded_count() {
        use misaka_palw_kernel::state::ClassRecordV1;
        let mut bytes = vec![0; 64];
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        assert!(ClassRecordV1::scan_native_wire_v1(&mut reader).is_err());
        bytes.truncate(67);
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        assert!(ClassRecordV1::scan_native_wire_v1(&mut reader).is_err());
    }
    #[test]
    fn ordinary_empty_template_is_bounded_but_unknown_derived_schema_is_not() {
        let rows = LedgerRowsV1::new();
        assert!(inventory_ledger_expansion_v1(&rows, false).unwrap() > 0);
        let mut rows = rows;
        rows.insert((255, vec![]), vec![]);
        assert!(matches!(inventory_ledger_expansion_v1(&rows, false), Err(NativeCoverageV1::Unsupported { .. })));
    }
}

/// Exact allocation-free codec preflight for a row in an authenticated kernel
/// row/aux response. This is a decode-shape bound only: callers still verify the
/// served state/header roots and use inventory_ledger_expansion_v1 before cold
/// program/gate reconstruction. A parsed row cannot establish chain authority.
pub fn preflight_kernel_served_row_v1(table: u8, key: &[u8], bytes: &[u8]) -> io::Result<NativeWireShapeV1> {
    let mut reader = NativeWireReaderV1::new(bytes, bytes.len() as u64);
    let shape = if table < 32 {
        scan_ledger_row_v1(table, &mut reader)?
    } else {
        super::wire_kernel_aux::scan_native_aux_row_v1(table, key, &mut reader)?
    };
    if reader.remaining() != 0 {
        return Err(wire::wire_bad("served kernel row has trailing codec bytes"));
    }
    Ok(shape)
}

/// Flat7/9 class reconstruction builds metadata, never artifact/trace tensors.
/// Both scope derivations visit the validated unrolled nodes; v9 additionally
/// tries up to 23 grammar boundary positions sequentially. Retained relation
/// rows are one per node/occurrence, with at most MAX_NODE_INPUTS leaf counters.
/// Scalar work/scratch QUOTES are not allocations of the quoted tensors.
fn flat_program_blob_v1<T: NativeWireScanV1, R: Read>(r: &mut NativeWireReaderV1<R>, cap: u64) -> io::Result<NativeWireShapeV1> {
    use misaka_palw_kernel::{
        flat_resources10::PrimitiveResourcesV10, flat_scope7::FlatRelationScopeV7, flat_scope9::FlatOccurrenceScopeV9,
    };
    use misaka_palw_tir::program::{MAX_LAYERS, MAX_NODE_INPUTS};
    let mut shape = program_blob::<T, R>(r, cap)?;
    let nodes = shape.collection_entries.checked_add(1).ok_or_else(|| wire::wire_bad("flat metadata nodes overflow"))?;
    let occurrences = (MAX_LAYERS as u64).checked_add(2).ok_or_else(|| wire::wire_bad("flat occurrences overflow"))?;
    let rows = nodes.checked_mul(occurrences).ok_or_else(|| wire::wire_bad("flat metadata rows overflow"))?;
    let relation = size_of::<FlatRelationScopeV7>().max(size_of::<FlatOccurrenceScopeV9>());
    let operands = 2 * MAX_NODE_INPUTS * size_of::<u64>();
    // Scope Vec growth, its canonical comparison/hash byte Vecs, resources10
    // primitive rows, and independently validated prepared wiring coexist.
    let unit = relation
        + operands
        + size_of::<PrimitiveResourcesV10>()
        + 2 * size_of::<typed::BTreeNodeEnvelopeV1<(u16, Option<u16>), (u16, u16)>>()
        + size_of::<typed::BTreeNodeEnvelopeV1<u32, ()>>();
    shape = shape.allocation(rows.checked_mul(8).ok_or_else(|| wire::wire_bad("flat concurrent metadata overflow"))?, unit)?;
    Ok(shape)
}

impl NativeWireScanV1 for misaka_palw_kernel::spec::WeightsRootV1 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        // The four fields have the same exact wire schema as ClassRecordV1,
        // including its independently parsed TIR body and derived gate requests.
        misaka_palw_kernel::state::ClassRecordV1::scan_native_wire_v1(r)
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::spec::composite::CompositeRootV1 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        use misaka_palw_kernel::spec::composite::{ComponentV1, CompositeStageV1, MAX_COMPOSITE_STAGES_V1};
        <[u8; 64]>::scan_native_wire_v1(r)?;
        let count = r.word()? as u64;
        if count > MAX_COMPOSITE_STAGES_V1 as u64 {
            return Err(wire::wire_bad("composite stage count exceeds canonical grammar"));
        }
        let mut shape = NativeWireShapeV1::default();
        for _ in 0..count {
            shape = shape.merge(CompositeStageV1::scan_native_wire_v1(r)?)?;
        }
        shape = shape.allocation(
            count.checked_mul(4).ok_or_else(|| wire::wire_bad("composite stage allocation overflow"))?,
            size_of::<CompositeStageV1>() + size_of::<ComponentV1>(),
        )?;
        shape.composite_components =
            shape.composite_components.checked_add(count).ok_or_else(|| wire::wire_bad("composite component inventory overflow"))?;
        Ok(shape)
    }
}

#[cfg(test)]
mod expansion_completion_tests {
    use super::*;
    use misaka_palw_tir::{DType, Ref, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_HELD};
    fn program() -> misaka_palw_tir::program::TirProgramV1 {
        let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
        let scores = b.param("scores", DType::I32, &[16], false);
        let mut pre = b.block("topk", vec![]);
        let x = pre.topk(scores, 0, 2);
        let x = pre.cast(x, DType::I32);
        let pre = pre.finish(&[x]);
        let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
        let y = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(y);
        let post = post.finish(&[]);
        b.finish(pre, vec![], post, 0)
    }
    #[test]
    fn flat_seven_and_nine_use_actual_nested_program_and_metadata_inventories() {
        use misaka_palw_kernel::{
            descriptor::k2_tir_v4_descriptor, flat_candidate7::*, flat_candidate9::*, trace::ParamCommitmentsV1,
        };
        let p = program();
        let encoded = p.encode();
        let params = ParamCommitmentsV1 { by_instance: [((0, None), [8; 64])].into() };
        let descriptor = k2_tir_v4_descriptor().digest();
        let v7 = derive_flat_candidate_v7(&descriptor, &encoded, &params, 32).unwrap();
        let v9 = derive_flat_candidate_v9(&descriptor, &encoded, &params, 32).unwrap();
        let r7 = FlatCandidateRecordV7 {
            binding: [9; 64],
            plan: v7.plan,
            program_bytes: encoded.clone(),
            param_commitments: params.clone(),
        };
        let r9 = FlatCandidateRecordV9 {
            binding: [9; 64],
            plan: v9.plan,
            program_bytes: encoded,
            param_commitments: params,
            scope_bytes: borsh::to_vec(&v9.scope).unwrap(),
            replay_bytes: borsh::to_vec(&v9.replay).unwrap(),
        };
        for (table, key, bytes) in
            // The row's leading root is a 64-BYTE digest (an untyped `[1; 64]` is `[i32; 64]`, 256 bytes the codec rightly refuses).
            [(0, r7.id(), borsh::to_vec(&([1u8; 64], r7)).unwrap()), (19, r9.id(), borsh::to_vec(&([1u8; 64], r9)).unwrap())]
        {
            let rows = LedgerRowsV1::from([((table, borsh::to_vec(&key).unwrap()), bytes.clone())]);
            let request = inventory_ledger_expansion_v1(&rows, false).unwrap();
            assert!(request > bytes.len() as u64, "native construction is explicitly included");
            let mut truncated = rows;
            truncated.values_mut().next().unwrap().pop();
            assert!(inventory_ledger_expansion_v1(&truncated, false).is_err());
        }
    }
    fn fixed_owner(limit: u64) -> (std::sync::Arc<NativeDemandCheckerV1>, ReservedKernelLedgerV1) {
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: limit, database_bytes: 0 });
        let policy = crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(crate::Hash64::default(), crate::Hash64::default());
        let ledger = crate::palw_kernel_route_v1::palw_kernel_route_template_v1(policy);
        // An injected ticket tests growth/refusal without changing process-wide
        // OnceLock admission settings or relying on test execution order.
        let lease = Some(checker.reserve_requests_v1(0, 0).unwrap());
        (checker, ReservedKernelLedgerV1::from_preflight_v1(ledger, lease))
    }
    #[test]
    fn fixed_bond_insertion_reserves_before_mutation_and_reuses_its_owner() {
        let node = size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::ledger::BondRowV1>>() as u64;
        let (checker, mut owner) = fixed_owner(node);
        owner.try_sync_bond_v1([1; 64], 7).unwrap();
        assert_eq!(checker.outstanding(), (node, 0));
        owner.try_sync_bond_v1([1; 64], 9).unwrap();
        assert_eq!(checker.outstanding(), (node, 0));
        assert_eq!(owner.bonds[&[1; 64]].collateral, 9);
        let before = owner.bonds.clone();
        assert!(owner.try_sync_bond_v1([2; 64], 11).is_err());
        assert_eq!(owner.bonds, before, "capacity refusal precedes insertion/collateral mutation");
        assert_eq!(checker.outstanding(), (node, 0));
        drop(owner);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn fixed_attestation_is_idempotent_and_failed_growth_preserves_membership() {
        let node = size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], ()>>() as u64;
        let (checker, mut owner) = fixed_owner(node);
        owner.try_attest_artifact_v1([1; 64]).unwrap();
        owner.try_attest_artifact_v1([1; 64]).unwrap();
        assert_eq!(checker.outstanding(), (node, 0));
        assert!(owner.try_attest_artifact_v1([2; 64]).is_err());
        assert_eq!(owner.attested_artifacts.iter().copied().collect::<Vec<_>>(), vec![[1; 64]]);
        assert!(owner.try_admit_optimistic_class_v1([3; 64]).is_err());
        assert!(owner.opv.admitted.is_empty());
        assert_eq!(checker.outstanding(), (node, 0), "semantic refusal allocates no new membership");
    }

    #[test]
    fn fixed_membership_refusal_keeps_capacity_distinct_from_missing_authority() {
        let (checker, mut owner) = fixed_owner(0);
        assert!(matches!(owner.try_admit_optimistic_class_checked_v1([3; 64]), Err(NativeDecodeErrorV1::Decode(_))));
        // Exercise the existing policy-enabled membership mutation only; this
        // test does not establish model eligibility or selected approval.
        owner.ledger.opv.policy =
            Some(crate::palw_panel_free_v1::PalwPanelFreeFenceV1::at(crate::config::params::ForkActivation::new(7)).opv_policy());
        assert!(matches!(owner.try_admit_optimistic_class_checked_v1([3; 64]), Err(NativeDecodeErrorV1::Resource(_))));
        assert!(owner.opv.admitted.is_empty(), "a failed reservation cannot add execution membership");
        assert!(matches!(owner.try_apply_paid_claim_tally_checked_v2(&[4; 64]), Err(NativeDecodeErrorV1::Decode(_))));
        assert!(owner.job_claims.is_empty(), "missing paid authority cannot create a payment right");
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn actual_composite_stages_are_counted_and_shape_arithmetic_is_checked() {
        use misaka_palw_kernel::spec::composite::*;
        let root = CompositeRootV1 {
            extension: [5; 64],
            stages: vec![
                CompositeStageV1 { component: [6; 64], input: StageInputV1::Query(QuerySourceV1::JobQuery) },
                CompositeStageV1 { component: [7; 64], input: StageInputV1::Query(QuerySourceV1::JobQuery) },
            ],
        };
        let bytes = borsh::to_vec(&root).unwrap();
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let shape = CompositeRootV1::scan_native_wire_v1(&mut reader).unwrap();
        assert_eq!(shape.composite_components, 2);
        assert_eq!(reader.remaining(), 0);
        let saturated = NativeWireShapeV1 { composite_components: u64::MAX, ..Default::default() };
        assert!(saturated.merge(shape).is_err());
        let too_many = CompositeRootV1 { stages: vec![root.stages[0].clone(); MAX_COMPOSITE_STAGES_V1 + 1], ..root };
        let bytes = borsh::to_vec(&too_many).unwrap();
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        assert!(CompositeRootV1::scan_native_wire_v1(&mut reader).is_err());
    }
}
