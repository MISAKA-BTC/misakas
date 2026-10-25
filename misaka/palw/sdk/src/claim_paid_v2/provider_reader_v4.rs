//! Authenticated provider acquisition under the new complete material codec.
//! Fetching bytes authenticates data identity only. It grants neither a checker
//! seat, a successful receipt, computation credit, nor native Finality.
use super::Refusal;
use kaspa_consensus_core::{
    palw_kernel_parameter_material_v4::{
        PALW_KERNEL_MATERIAL_UNIT_BYTES_V4, PALW_KERNEL_PARAMETER_STAGE_V4, PalwKernelParameterOpeningV4, PalwKernelParameterRowV4,
        classify_kernel_parameter_v4, visit_kernel_parameters_v4,
    },
    palw_material_capacity_v1::kernel_material_unit_requests_v4,
    palw_native_resource_v1::{
        NativeDemandLeaseV1, NativeRetainedV1, native_kernel_parameter_tensor_requests_v4, preflight_native_value_v1,
        reserve_native_requests_v1, retain_native_value_decode_classified_v1,
    },
};
use misaka_palw_kernel::{hash::Digest, ledger::KernelLedgerV1};
use misaka_palw_tir::Tensor;

pub use kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialFetchErrorV4 as ProviderReaderErrorV4;
#[path = "provider_reader_v4_errors.rs"]
mod errors;
use errors::{capacity, classified_decode, invalid, shape_error, auth_requests, projection_error};
fn bad(e: impl ToString) -> ProviderReaderErrorV4 {
    ProviderReaderErrorV4::Unknown(e.to_string())
}
#[path = "provider_reader_v4_failure.rs"]
mod failure;
pub use failure::{ProviderAcquisitionReportV4, ProviderReaderFailureV4};

fn compatibility(error: ProviderReaderErrorV4) -> Refusal {
    Refusal::new("PROVIDER_READER_V4", error.to_string())
}
fn add(a: u64, b: u64) -> Result<u64, ProviderReaderErrorV4> {
    a.checked_add(b).ok_or_else(|| capacity("provider reader capacity overflow"))
}

/// Explicit local resource inputs, independent of unapproved market/security
/// policy values. No default declares a model or a network safe.
#[derive(Clone, Copy, Debug)]
pub struct ProviderReaderLimitsV4 {
    pub max_unit_bytes: u64,
    pub max_total_fetch_bytes: u64,
    pub max_workspace_bytes: u64,
    pub max_units: u32,
}
impl ProviderReaderLimitsV4 {
    fn validate(&self) -> Result<(), ProviderReaderErrorV4> {
        if self.max_unit_bytes == 0
            || self.max_unit_bytes > PALW_KERNEL_MATERIAL_UNIT_BYTES_V4
            || self.max_total_fetch_bytes == 0
            || self.max_workspace_bytes == 0
            || self.max_units == 0
        {
            return Err(bad("explicit finite provider acquisition limits required"));
        }
        Ok(())
    }
}
/// A fetched full parameter cannot detach its native construction reservation.
/// Checker adapters borrow the tensor while this owner remains alive.
pub struct ProviderParameterV4 {
    tensor: Tensor,
    _lease: Option<NativeDemandLeaseV1>,
}
impl ProviderParameterV4 {
    pub fn tensor(&self) -> &Tensor {
        &self.tensor
    }
}

/// Reconstruct one registered immutable parameter from every native row/tile.
/// The callback must use own current native query authority, including consent,
/// provider key and requester key; its returned raw allocation keeps its lease.
/// `ledger` must be the caller's authenticated selected-state projection.
/// Each opening and the final *full* native commitment are checked; accepting
/// mutually inconsistent row/column trees by checking rows alone is forbidden.
pub fn fetch_parameter_from_provider_classified_v4(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    program: u16,
    parameter: u16,
    layer: Option<u16>,
    limits: ProviderReaderLimitsV4,
    fetch: impl FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>,
) -> Result<ProviderParameterV4, ProviderReaderErrorV4> {
    fetch_parameter_observed_v4(ledger, claim, program, parameter, layer, limits, fetch, |_, _| {})
}
#[allow(clippy::too_many_arguments)]
fn fetch_parameter_observed_v4(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    program: u16,
    parameter: u16,
    layer: Option<u16>,
    limits: ProviderReaderLimitsV4,
    mut fetch: impl FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>,
    invalid_unit: impl Fn(u8, u32),
) -> Result<ProviderParameterV4, ProviderReaderErrorV4> {
    limits.validate()?;
    let mut offset = 0u64;
    let mut found = None;
    visit_kernel_parameters_v4(ledger, claim, |spec| {
        if (spec.program, spec.parameter, spec.layer) == (program, parameter, layer) {
            found = Some((offset, spec));
        }
        offset = offset.checked_add(spec.units()?).ok_or("parameter ordinal overflow")?;
        Ok(())
    })
    .map_err(bad)?;
    let (first, spec) = found.ok_or_else(|| bad("parameter is not immutable material of this accepted claim"))?;
    let units = spec.units().map_err(capacity)?;
    let maximum_unit = spec.maximum_unit_bytes().map_err(capacity)?;
    if units == 0
        || units > u64::from(limits.max_units)
        || maximum_unit > limits.max_unit_bytes
        || first.checked_add(units).is_none_or(|n| n > u64::from(u32::MAX) + 1)
    {
        return Err(capacity("registered parameter exceeds explicit row/tile acquisition limits"));
    }
    let work = native_kernel_parameter_tensor_requests_v4(&spec).map_err(capacity)?;
    if work > limits.max_workspace_bytes {
        return Err(capacity("full parameter and native commitment workspace exceeds capacity"));
    }
    // Reserve the tensor, shape and dual-tree commitment before allocating any
    // destination. Unit decode/auth reservations below live concurrently.
    let lease = reserve_native_requests_v1(work).map_err(capacity)?;
    let elements = spec.rows.checked_mul(spec.row_len).ok_or_else(|| capacity("parameter elements overflow"))?;
    let mut values = Vec::new();
    values.try_reserve_exact(usize::try_from(elements).map_err(capacity)?).map_err(capacity)?;
    let mut fetched = 0u64;
    for local in 0..units {
        let ordinal = u32::try_from(add(first, local)?).map_err(bad)?;
        let unit_result = (|| {
            let bytes = fetch(PALW_KERNEL_PARAMETER_STAGE_V4, ordinal)?;
            fetched = add(fetched, bytes.len() as u64)?;
            if bytes.len() as u64 > limits.max_unit_bytes || fetched > limits.max_total_fetch_bytes {
                return Err(capacity("provider parameter acquisition exceeds byte limits"));
            }
            // Always walk actual lengths before decode, even if native accounting
            // is disabled in a private runtime. A frame cap alone cannot bound Vec.
            let shape = preflight_native_value_v1::<PalwKernelParameterRowV4>(&bytes[..], bytes.len() as u64).map_err(shape_error)?;
            let codec = kernel_material_unit_requests_v4(PALW_KERNEL_PARAMETER_STAGE_V4, &bytes)
                .map_err(bad)?
                .bytes()
                .ok_or_else(|| capacity("parameter codec coverage overflow"))?;
            if add(add(work, bytes.len() as u64)?, add(shape.logical_requests_v1(), codec)?)? > limits.max_workspace_bytes {
                return Err(capacity("parameter codec exceeds concurrent workspace before authentication"));
            }
            let auth = auth_requests(PALW_KERNEL_PARAMETER_STAGE_V4, &bytes, ledger, claim)?;
            let concurrent = add(add(work, bytes.len() as u64)?, add(shape.logical_requests_v1(), add(codec, auth)?)?)?;
            if concurrent > limits.max_workspace_bytes {
                return Err(capacity("parameter unit decode/auth exceeds concurrent workspace"));
            }
            let _auth_lease = reserve_native_requests_v1(add(codec, auth)?).map_err(capacity)?;
            let unit = retain_native_value_decode_classified_v1(&bytes, || borsh::from_slice::<PalwKernelParameterRowV4>(&bytes)).map_err(classified_decode)?;
            classify_kernel_parameter_v4(ledger, claim, ordinal, &unit).map_err(invalid)?;
            let part = match &unit.opening {
                PalwKernelParameterOpeningV4::RowV2(o) => &o.values,
                PalwKernelParameterOpeningV4::TileV3(o) => &o.values,
            };
            let tile = local % spec.row_tiles;
            let start = if spec.suite == 3 {
                tile.checked_mul(misaka_palw_kernel::merkle3::TILE_V3).ok_or_else(|| capacity("parameter tile offset overflow"))?
            } else {
                0
            };
            let expected =
                if spec.suite == 3 { spec.row_len.saturating_sub(start).min(misaka_palw_kernel::merkle3::TILE_V3) } else { spec.row_len };
            if part.len() as u64 != expected || add(values.len() as u64, expected)? > elements {
                return Err(invalid("parameter row/tile cardinality differs"));
            }
            values.extend_from_slice(part);
            Ok(())
        })();
        if matches!(&unit_result, Err(ProviderReaderErrorV4::InvalidMaterial(_))) {
            invalid_unit(PALW_KERNEL_PARAMETER_STAGE_V4, ordinal);
        }
        unit_result?;
    }
    if values.len() as u64 != elements {
        return Err(invalid("parameter reconstruction is incomplete"));
    }
    let mut shape = Vec::new();
    shape.try_reserve_exact(spec.declaration.shape.len()).map_err(capacity)?;
    for d in &spec.declaration.shape {
        shape.push(usize::try_from(*d).map_err(bad)?);
    }
    let tensor = Tensor::new(spec.declaration.dtype, shape, values).map_err(bad)?;
    if !full_parameter_matches_v4(&tensor, spec.suite, &spec.commitment)? {
        return Err(invalid("full fetched parameter does not reproduce registered tensor commitment"));
    }
    Ok(ProviderParameterV4 { tensor, _lease: lease })
}

fn full_parameter_matches_v4(tensor: &Tensor, suite: u32, want: &Digest) -> Result<bool, ProviderReaderErrorV4> {
    let commitment = match suite {
        2 => misaka_palw_kernel::merkle::tensor_commitment_v2(tensor),
        3 => misaka_palw_kernel::merkle3::tensor_commitment_v3(tensor),
        _ => return Err(bad("unsupported native tensor commitment suite")),
    };
    Ok(commitment == *want)
}

use super::{PaidClaimMaterialWireV2, VerifiedPaidClaimSourcesV2};
use kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1;
use kaspa_consensus_core::palw_prospective_kernel_material_v4::typed_kernel_classify_unit_v4;
use misaka_palw_kernel::{
    claim_paid_v2::{PaidClaimCheckAuthorityV2, PaidClaimTargetV2, PaidClaimVerificationResultV2},
    ledger::{OutsiderFindingV1, PublicArtifactV1, PublicSourceV1},
    public::{MaterialResponseV1, PositionResponseV1},
    spec::retrieval::{RetrievalItemV1, SliceResponseV1},
};
use std::cell::{Cell, RefCell};

/// Keep every fetched-value reservation until all checker temporaries and its
/// finding are gone. The finding can be borrowed but not detached from this
/// owner. These reservations cover acquisition, not an unmeasured whole-model
/// checker/court/RSS or database bound.
pub struct ProviderCheckedResultV4 {
    result: PaidClaimVerificationResultV2,
    report: ProviderAcquisitionReportV4,
    _acquisition: Option<NativeDemandLeaseV1>,
}
impl ProviderCheckedResultV4 {
    /// Same typed acquisition account for all four adapter targets.
    pub fn acquisition_report(&self) -> ProviderAcquisitionReportV4 {
        self.report
    }
    pub fn finding(&self) -> &OutsiderFindingV1 {
        &self.result.finding
    }
    pub fn receipt(
        &self,
        sources: &VerifiedPaidClaimSourcesV2,
        signer: Digest,
        now: u64,
    ) -> Result<misaka_palw_kernel::route::KernelRouteObjectV1, Refusal> {
        let checked = self.result.complete.as_ref().ok_or_else(|| compatibility(bad("provider check has no complete selected-seat result")))?;
        // The existing authority checks native enrollment/context/seed and
        // assigned signer. Query signatures/local replay cannot bypass it.
        sources.receipt(checked, signer, now)
    }
}
struct Reader<'a, F> {
    ledger: &'a KernelLedgerV1,
    claim: Digest,
    fetch: RefCell<F>,
    limits: ProviderReaderLimitsV4,
    calls: Cell<u32>,
    bytes: Cell<u64>,
    charged: Cell<u64>,
    lease: RefCell<Option<NativeDemandLeaseV1>>,
    error: RefCell<Option<ProviderReaderErrorV4>>,
    invalid_unit: Cell<Option<(u8, u32)>>,
}
impl<F: FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>> Reader<'_, F> {
    fn charge(&self, n: u64) -> Result<(), ProviderReaderErrorV4> {
        let total = add(self.charged.get(), n)?;
        if total > self.limits.max_workspace_bytes {
            return Err(capacity("cumulative provider checker acquisition exceeds capacity"));
        }
        if let Some(lease) = self.lease.borrow_mut().as_mut() {
            lease.grow_memory_v1(n).map_err(capacity)?;
        }
        self.charged.set(total);
        Ok(())
    }
    fn fetch(&self, stage: u8, ordinal: u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4> {
        let next = self.calls.get().checked_add(1).ok_or_else(|| capacity("provider acquisition count overflow"))?;
        if next > self.limits.max_units {
            return Err(capacity("provider acquisition exceeds explicit request count"));
        }
        self.calls.set(next);
        let raw = (self.fetch.borrow_mut())(stage, ordinal)?;
        let bytes = self.bytes.get().checked_add(raw.len() as u64).ok_or_else(|| capacity("provider acquisition byte overflow"))?;
        if raw.len() as u64 > self.limits.max_unit_bytes || bytes > self.limits.max_total_fetch_bytes {
            return Err(capacity("provider acquisition exceeds explicit byte limits"));
        }
        self.bytes.set(bytes);
        Ok(raw)
    }
    fn decoded<T: NativeWireScanV1 + borsh::BorshDeserialize, R>(
        &self,
        stage: u8,
        ordinal: u32,
        read: impl FnOnce(&T) -> Result<R, ProviderReaderErrorV4>,
    ) -> Result<R, ProviderReaderErrorV4> {
        let result = (|| {
            let raw = self.fetch(stage, ordinal)?;
            preflight_native_value_v1::<T>(&raw[..], raw.len() as u64).map_err(shape_error)?;
            let codec =
                kernel_material_unit_requests_v4(stage, &raw).map_err(bad)?.bytes().ok_or_else(|| capacity("provider codec size overflow"))?;
            // Admit actual codec storage before the auth quote's own decode. Both
            // helpers reserve before allocation; their temporary owner is distinct.
            self.charge(add(raw.len() as u64, codec)?)?;
            let auth = auth_requests(stage, &raw, self.ledger, &self.claim)?;
            self.charge(auth)?;
            typed_kernel_classify_unit_v4(self.ledger, &self.claim, stage, ordinal, &raw).map_err(invalid)?;
            let value = retain_native_value_decode_classified_v1(&raw, || borsh::from_slice::<T>(&raw)).map_err(classified_decode)?;
            read(&value)
        })();
        if matches!(&result, Err(ProviderReaderErrorV4::InvalidMaterial(_))) {
            self.invalid_unit.set(Some((stage, ordinal)));
        }
        result
    }
    fn attempt<T>(&self, call: impl FnOnce() -> Result<T, ProviderReaderErrorV4>) -> Option<T> {
        if self.error.borrow().is_some() {
            return None;
        }
        match call() {
            Ok(value) => Some(value),
            Err(error) => {
                *self.error.borrow_mut() = Some(error);
                None
            }
        }
    }
}
impl<F: FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>> PublicSourceV1 for Reader<'_, F> {
    fn node(&self, stage: u8, position: u32, occurrence: u16, node: u16) -> Option<Tensor> {
        self.attempt(|| {
            self.decoded::<PositionResponseV1, _>(stage, position, |r| {
                match r.values.get(occurrence as usize).and_then(|o| o.get(node as usize)) {
                    Some(MaterialResponseV1::Whole(w)) => w.decode().map_err(invalid),
                    _ => Err(invalid("required provider node is absent/derived/partial")),
                }
            })
        })
    }
    fn input(&self, stage: u8, position: u32, index: u16) -> Option<Tensor> {
        self.attempt(|| {
            self.decoded::<PositionResponseV1, _>(stage, position, |r| match r.inputs.get(index as usize) {
                Some(MaterialResponseV1::Whole(w)) => w.decode().map_err(invalid),
                _ => Err(invalid("required provider input is absent/partial")),
            })
        })
    }
}
impl<F: FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>> PublicArtifactV1 for Reader<'_, F> {
    fn param(&self, program: u16, parameter: u16, layer: Option<u16>) -> Option<Tensor> {
        // Mutable memory slots are deliberately absent from immutable provider
        // parameter rows. The typed checker first tries M0 in the artifact and
        // then falls back to its authenticated entry-state unit. That optional
        // miss must not poison a successful fallback with a sticky read error.
        if program == 0
            && self.ledger.claims.get(&self.claim).is_some_and(|row| {
                matches!(&row.body, misaka_palw_kernel::ledger::ClaimBodyV1::Spec(_))
                    && self.ledger.typed.classes.get(&row.class_binding_id).is_some_and(|class| {
                        matches!(&class.kind, misaka_palw_kernel::spec::SpecClassKindV1::Memory { root, .. }
                            if root.slots.iter().any(|slot| slot.param == (parameter, layer)))
                    })
            })
        {
            return None;
        }
        self.attempt(|| {
            // Reserve the destination clone before fetching. The standalone
            // parameter owner separately covers its construction and full roots.
            let mut spec_found = None;
            visit_kernel_parameters_v4(self.ledger, &self.claim, |spec| {
                if (spec.program, spec.parameter, spec.layer) == (program, parameter, layer) {
                    spec_found = Some(spec);
                }
                Ok(())
            })
            .map_err(bad)?;
            let spec = spec_found.ok_or_else(|| bad("requested immutable parameter absent"))?;
            self.charge(native_kernel_parameter_tensor_requests_v4(&spec).map_err(capacity)?)?;
            let remaining =
                self.limits.max_workspace_bytes.checked_sub(self.charged.get()).ok_or_else(|| capacity("parameter capacity exhausted"))?;
            let limits = ProviderReaderLimitsV4 { max_workspace_bytes: remaining, ..self.limits };
            let value = fetch_parameter_observed_v4(self.ledger, &self.claim, program, parameter, layer, limits, |s, p| {
                self.fetch(s, p)
            }, |stage, ordinal| self.invalid_unit.set(Some((stage, ordinal))))?;
            Ok(value.tensor().clone())
        })
    }
}
impl<F: FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>> misaka_palw_kernel::spec::outsider::SnapshotSourceV1
    for Reader<'_, F>
{
    fn item(&self, snapshot: &Digest, id: u64) -> Option<RetrievalItemV1> {
        self.attempt(|| {
            let row = self.ledger.claims.get(&self.claim).ok_or_else(|| bad("typed checker claim absent"))?;
            // Namespaces reserve at most 64 components. Select from the actual
            // class, never a provider's self-asserted snapshot root.
            let mut found = None;
            for component in 0..64u8 {
                if let Ok((root, _)) = super::provider_handoff_v4::retrieval(self.ledger, row, component) {
                    if root.snapshot.root() == *snapshot && id < root.snapshot.items {
                        if root.snapshot.slice_items == 0 {
                            return Err(bad("native snapshot has zero slice size"));
                        }
                        found =
                            Some((component, id / u64::from(root.snapshot.slice_items), id % u64::from(root.snapshot.slice_items)));
                        break;
                    }
                }
            }
            let (component, slice, index) = found.ok_or_else(|| bad("snapshot item is outside authenticated claim"))?;
            self.decoded::<SliceResponseV1, _>(
                misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1 + component,
                u32::try_from(slice).map_err(bad)?,
                |r| r.items.get(index as usize).cloned().ok_or_else(|| invalid("provider snapshot slice omitted required item")),
            )
        })
    }
}

impl<F: FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>> Reader<'_, F> {
    fn segmented_range(&self, position: u32) -> Result<(u32, u32), ProviderReaderErrorV4> {
        let row = self.ledger.claims.get(&self.claim).ok_or_else(|| bad("segmented checker claim absent"))?;
        let misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { evidence, .. } = &row.body else {
            return Err(bad("not a segmented checker claim"));
        };
        if position >= evidence.positions {
            return Err(bad("segmented checker position outside native inventory"));
        }
        let class = self.ledger.classes.get(&row.class_binding_id).ok_or_else(|| bad("segmented checker class absent"))?;
        let mut first = 0u64;
        for p in 0..=position {
            let count = misaka_palw_kernel::seg_da::position_part_bytes_v1(&class.program, p).map_err(bad)?.len() as u64;
            if p == position {
                return Ok((u32::try_from(first).map_err(bad)?, u32::try_from(count).map_err(bad)?));
            }
            first = add(first, count)?;
        }
        Err(bad("segmented native part range absent"))
    }
    fn segment_path(&self, position: u32) -> Result<(Digest, Vec<Digest>), ProviderReaderErrorV4> {
        use kaspa_consensus_core::palw_kernel_parameter_material_v4::{
            PALW_KERNEL_SEGMENTED_PART_STAGE_V4, PalwKernelSegmentedPartV4,
        };
        let (first, _) = self.segmented_range(position)?;
        self.decoded::<PalwKernelSegmentedPartV4, _>(PALW_KERNEL_SEGMENTED_PART_STAGE_V4, first, |unit| {
            preflight_native_value_v1::<misaka_palw_kernel::seg_da::SegPartResponseV1>(&unit.bytes[..], unit.bytes.len() as u64)
                .map_err(shape_error)?;
            let part = retain_native_value_decode_classified_v1(&unit.bytes, || {
                borsh::from_slice::<misaka_palw_kernel::seg_da::SegPartResponseV1>(&unit.bytes)
            })
            .map_err(classified_decode)?;
            self.charge((part.position_siblings.len() as u64).checked_mul(64).ok_or_else(|| capacity("segment path size overflow"))?)?;
            Ok((part.position_root, part.position_siblings.clone()))
        })
    }
    fn assembled(
        &self,
        position: u32,
    ) -> Result<(misaka_palw_kernel::seg_da::AssembledPositionV1, misaka_palw_kernel::seg_da::HiddenLeavesV1), ProviderReaderErrorV4> {
        use kaspa_consensus_core::{
            palw_kernel_parameter_material_v4::{PALW_KERNEL_SEGMENTED_PART_STAGE_V4, PalwKernelSegmentedPartV4},
            palw_native_resource_v1::native_kernel_segment_assembly_requests_v4,
        };
        let (first, count) = self.segmented_range(position)?;
        if count == 0 || count > self.limits.max_units {
            return Err(capacity("segmented position exceeds explicit acquisition request limit"));
        }
        self.charge(
            u64::from(count)
                .checked_mul(std::mem::size_of::<Vec<u8>>() as u64)
                .ok_or_else(|| capacity("segmented part frames overflow"))?,
        )?;
        let mut parts = Vec::new();
        parts.try_reserve_exact(count as usize).map_err(capacity)?;
        for part in 0..count {
            let ordinal = first.checked_add(part).ok_or_else(|| capacity("segmented part ordinal overflow"))?;
            // Clone one already authenticated inner frame. The acquisition
            // reservation covers raw/decoded/copied frames while all parts are
            // retained; the assembly quote below covers its own parsed bodies.
            let bytes = self.decoded::<PalwKernelSegmentedPartV4, _>(PALW_KERNEL_SEGMENTED_PART_STAGE_V4, ordinal, |unit| {
                if unit.position != position || unit.part != part {
                    return Err(invalid("segmented provider part metadata differs"));
                }
                self.charge(unit.bytes.len() as u64)?;
                Ok(unit.bytes.clone())
            })?;
            parts.push(bytes);
        }
        let row = self.ledger.claims.get(&self.claim).ok_or_else(|| bad("segmented checker claim absent"))?;
        let misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { evidence, segment_roots, .. } = &row.body else {
            return Err(bad("not a segmented checker claim"));
        };
        let class = self.ledger.classes.get(&row.class_binding_id).ok_or_else(|| bad("segmented checker class absent"))?;
        let requests = native_kernel_segment_assembly_requests_v4(&class.program, position, &parts).map_err(capacity)?;
        self.charge(requests)?;
        misaka_palw_kernel::seg_da::assemble_position_full_v1(&class.program, segment_roots, evidence.positions, position, &parts)
            .ok_or_else(|| invalid("authenticated segmented parts do not form the native full position"))
    }
}
impl<F: FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>> misaka_palw_kernel::element::SegMaterialV1 for Reader<'_, F> {
    fn position(&self, position: u32) -> Option<Vec<Vec<Tensor>>> {
        self.attempt(|| self.assembled(position).map(|((values, _, _), _)| values))
    }
    fn position_siblings(&self, position: u32) -> Option<Vec<Digest>> {
        self.attempt(|| self.segment_path(position).map(|(_, path)| path))
    }
    fn position_path(&self, _: &misaka_palw_tir::program::TirProgramV1, position: u32) -> Option<(Digest, Vec<Digest>)> {
        self.attempt(|| self.segment_path(position))
    }
    fn commitments(&self, _: &misaka_palw_tir::program::TirProgramV1, position: u32) -> Option<Vec<Vec<Digest>>> {
        self.attempt(|| self.assembled(position).map(|((_, _, commitments), _)| commitments))
    }
    fn hidden_leaves(
        &self,
        _: &misaka_palw_tir::program::TirProgramV1,
        position: u32,
    ) -> Option<misaka_palw_kernel::seg_da::HiddenLeavesV1> {
        self.attempt(|| self.assembled(position).map(|(_, hidden)| hidden))
    }
}

impl VerifiedPaidClaimSourcesV2 {
    /// Use actual authenticated provider nodes/inputs/parameters/snapshot rows
    /// in the existing all-target checker. The original canonical opening is
    /// still required for V3/V4 message-root compatibility and segmented native
    /// assembly; it is not silently reinterpreted as a provider manifest.
    ///
    /// This ordinary-round entry point rejects V4 until its separately rooted
    /// direct-check audit record is supplied via the V4 entry point. Retrieval
    /// errors are acquisition failures, never arithmetic convictions or Clean.
    /// Preserve local native capacity and remote acquisition causes through
    /// every optional checker adapter. Failures cannot produce a complete result.
    pub fn verify_using_provider_classified_v4(
        &self,
        canonical_opening: &PaidClaimMaterialWireV2,
        limits: ProviderReaderLimitsV4,
        fetch: impl FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>,
    ) -> Result<ProviderCheckedResultV4, ProviderReaderErrorV4> {
        self.verify_using_provider_located_v4(canonical_opening, limits, fetch).map_err(ProviderReaderFailureV4::into_cause)
    }
    /// The classified reader plus exact-unit provenance for a failed unit's own
    /// decode/authentication. Cross-unit reconstruction/checker errors have no
    /// unit. This metadata authorizes neither a challenge nor a conviction;
    /// the caller must resolve the same current native provider generation.
    pub fn verify_using_provider_located_v4(
        &self,
        canonical_opening: &PaidClaimMaterialWireV2,
        limits: ProviderReaderLimitsV4,
        fetch: impl FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>,
    ) -> Result<ProviderCheckedResultV4, ProviderReaderFailureV4> {
        limits.validate()?;
        let check = self.check.bind_opening_v3(canonical_opening).map_err(bad)?;
        self.check_provider_located_v4(canonical_opening, &check, limits, fetch)
    }
    fn check_provider_located_v4<F: FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>>(
        &self,
        canonical_opening: &PaidClaimMaterialWireV2,
        check: &PaidClaimCheckAuthorityV2,
        limits: ProviderReaderLimitsV4,
        fetch: F,
    ) -> Result<ProviderCheckedResultV4, ProviderReaderFailureV4> {
        limits.validate()?;
        let mut ledger = self.route.ledger_checked_v1().map_err(projection_error)?;
        check_provider_material_located_v4(
            &mut ledger,
            self.authority.enrollment.base_claim,
            self.authority.class.target,
            canonical_opening,
            check,
            limits,
            fetch,
        )
    }
}

// Shared by the authenticated source caller and focused adapter regressions.
// This is private: supplying a ledger/check here does not install native source
// authority or make a selected receipt. The public caller derives both above.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn check_provider_material_classified_v4<F: FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>>(
    ledger: &mut kaspa_consensus_core::palw_native_resource_v1::ReservedKernelLedgerV1,
    claim: Digest,
    target: PaidClaimTargetV2,
    canonical_opening: &PaidClaimMaterialWireV2,
    check: &PaidClaimCheckAuthorityV2,
    limits: ProviderReaderLimitsV4,
    fetch: F,
) -> Result<ProviderCheckedResultV4, ProviderReaderErrorV4> {
    check_provider_material_located_v4(ledger, claim, target, canonical_opening, check, limits, fetch)
        .map_err(ProviderReaderFailureV4::into_cause)
}
#[allow(clippy::too_many_arguments)]
fn check_provider_material_located_v4<F: FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>>(
    ledger: &mut kaspa_consensus_core::palw_native_resource_v1::ReservedKernelLedgerV1,
    claim: Digest,
    target: PaidClaimTargetV2,
    canonical_opening: &PaidClaimMaterialWireV2,
    check: &PaidClaimCheckAuthorityV2,
    limits: ProviderReaderLimitsV4,
    fetch: F,
) -> Result<ProviderCheckedResultV4, ProviderReaderFailureV4> {
    limits.validate()?;
    ledger.try_reserve_kernel_material_v4(&claim).map_err(capacity)?;
    let reader = Reader {
        ledger: &*ledger,
        claim,
        fetch: RefCell::new(fetch),
        limits,
        calls: Cell::new(0),
        bytes: Cell::new(0),
        charged: Cell::new(0),
        lease: RefCell::new(reserve_native_requests_v1(0).map_err(capacity)?),
        error: RefCell::new(None),
        invalid_unit: Cell::new(None),
    };
    let outsider =
        misaka_palw_kernel::ledger::OutsiderV1 { ledger: &*ledger, claim, material: &reader, artifact: &reader, salt: [0; 64] };
    let checked = match target {
        PaidClaimTargetV2::Program | PaidClaimTargetV2::Pipeline => check.verify_program_or_pipeline(&outsider),
        PaidClaimTargetV2::TypedSpec => check.verify_typed(&misaka_palw_kernel::spec::outsider::SpecOutsiderV1 {
            ledger: &*ledger,
            claim,
            material: &reader,
            artifact: &reader,
            snapshots: &reader,
            salt: [0; 64],
        }),
        PaidClaimTargetV2::Segmented => check.verify_segmented(&outsider, &reader, &canonical_opening.tokens),
    };
    let report = ProviderAcquisitionReportV4 {
        target,
        unit_requests: reader.calls.get(),
        fetched_bytes: reader.bytes.get(),
        workspace_bytes_charged: reader.charged.get(),
    };
    if let Some(error) = reader.error.into_inner() {
        return Err(ProviderReaderFailureV4::from_reader(error, reader.invalid_unit.get(), report));
    }
    let result = checked.map_err(bad)?;
    Ok(ProviderCheckedResultV4 { result, report, _acquisition: reader.lease.into_inner() })
}

/// Compatibility entry point for callers whose transport still returns strings.
/// Such opaque errors remain Unknown; text is never parsed for fault classification.
#[allow(clippy::too_many_arguments)]
pub fn fetch_parameter_from_provider_v4(
    ledger: &KernelLedgerV1, claim: &Digest, program: u16, parameter: u16, layer: Option<u16>,
    limits: ProviderReaderLimitsV4,
    mut fetch: impl FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, String>,
) -> Result<ProviderParameterV4, Refusal> {
    fetch_parameter_from_provider_classified_v4(ledger, claim, program, parameter, layer, limits,
        |s, p| fetch(s, p).map_err(bad)).map_err(compatibility)
}
impl VerifiedPaidClaimSourcesV2 {
    pub fn verify_using_provider_v4(
        &self, canonical_opening: &PaidClaimMaterialWireV2, limits: ProviderReaderLimitsV4,
        mut fetch: impl FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, String>,
    ) -> Result<ProviderCheckedResultV4, Refusal> {
        self.verify_using_provider_classified_v4(canonical_opening, limits,
            |s, p| fetch(s, p).map_err(bad)).map_err(compatibility)
    }
}
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn check_provider_material_v4(
    ledger: &mut kaspa_consensus_core::palw_native_resource_v1::ReservedKernelLedgerV1,
    claim: Digest, target: PaidClaimTargetV2, canonical_opening: &PaidClaimMaterialWireV2,
    check: &PaidClaimCheckAuthorityV2, limits: ProviderReaderLimitsV4,
    mut fetch: impl FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, String>,
) -> Result<ProviderCheckedResultV4, Refusal> {
    check_provider_material_classified_v4(ledger, claim, target, canonical_opening, check, limits,
        |s, p| fetch(s, p).map_err(bad)).map_err(compatibility)
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::types::DType;

    #[test]
    fn producer_parameters_round_trip_every_authenticated_row_and_refuse_missing_or_substituted_units() {
        use crate::provider_reader_fixture::ledger_world::World;
        use kaspa_consensus_core::palw_kernel_parameter_material_v4::kernel_parameter_at_v4;
        let mut world = World::new();
        let job = world.post_job(2, &[3, 17], 1, 0x72);
        let made = world.honest(&job, 1);
        let claim = made.claim.id();
        world.block(4, vec![made.tx]);
        assert!(world.l.claims.contains_key(&claim));
        let limits = ProviderReaderLimitsV4 {
            max_unit_bytes: PALW_KERNEL_MATERIAL_UNIT_BYTES_V4,
            max_total_fetch_bytes: 64 << 20,
            max_workspace_bytes: 1 << 30,
            max_units: 10_000,
        };
        let mut parameters = Vec::new();
        visit_kernel_parameters_v4(&world.l, &claim, |spec| {
            parameters.push((spec.program, spec.parameter, spec.layer, spec.units()?));
            Ok(())
        })
        .unwrap();
        assert!(!parameters.is_empty());
        let fetch = |stage, ordinal, substitute| {
            assert_eq!(stage, PALW_KERNEL_PARAMETER_STAGE_V4);
            let (spec, row) = kernel_parameter_at_v4(&world.l, &claim, ordinal).unwrap();
            let tensor = &world.params.tensors[&(spec.parameter, spec.layer)];
            let opening = match spec.suite {
                2 => PalwKernelParameterOpeningV4::RowV2(misaka_palw_kernel::merkle::TensorOpeningV1::row(tensor, row).unwrap()),
                3 => PalwKernelParameterOpeningV4::TileV3(
                    misaka_palw_kernel::merkle3::LeafOpeningV3::of(
                        tensor,
                        misaka_palw_kernel::merkle::AXIS_ROW,
                        row / spec.row_tiles,
                        row % spec.row_tiles,
                    )
                    .unwrap(),
                ),
                suite => panic!("unsupported fixture suite {suite}"),
            };
            let unit = PalwKernelParameterRowV4 {
                program: spec.program,
                parameter: spec.parameter,
                layer: spec.layer,
                row: row + u64::from(substitute),
                opening,
            };
            NativeRetainedV1::retain_existing_v1(borsh::to_vec(&unit).unwrap()).map_err(|e| e.to_string())
        };
        for (program, parameter, layer, units) in parameters {
            let mut calls = 0;
            let actual = fetch_parameter_from_provider_v4(&world.l, &claim, program, parameter, layer, limits, |s, p| {
                calls += 1;
                fetch(s, p, false)
            })
            .unwrap();
            let expected = &world.params.tensors[&(parameter, layer)];
            assert_eq!(actual.tensor().shape, expected.shape);
            assert_eq!(actual.tensor().to_le_bytes(), expected.to_le_bytes());
            assert_eq!(calls, units);
            assert!(
                fetch_parameter_from_provider_v4(&world.l, &claim, program, parameter, layer, limits, |_, _| Err(
                    "provider unavailable".into()
                ))
                .is_err()
            );
            assert!(
                fetch_parameter_from_provider_v4(&world.l, &claim, program, parameter, layer, limits, |s, p| fetch(s, p, true))
                    .is_err()
            );
            assert!(
                fetch_parameter_from_provider_v4(
                    &world.l,
                    &claim,
                    program,
                    parameter,
                    layer,
                    ProviderReaderLimitsV4 { max_workspace_bytes: 1, ..limits },
                    |_, _| panic!("insufficient workspace must refuse before fetching")
                )
                .is_err()
            );
        }
    }

    // All row memberships can agree with a mixed row/column commitment while
    // its unused column root describes no reconstructed tensor. The complete
    // fetched artifact must reproduce BOTH trees, not just authenticate rows.
    #[test]
    fn authentic_v2_rows_do_not_authorize_an_inconsistent_full_parameter() {
        let tensor = Tensor::new(DType::I32, vec![2, 2], vec![1, 2, 3, 4]).unwrap();
        let mut row = misaka_palw_kernel::merkle::TensorOpeningV1::row(&tensor, 0).unwrap();
        row.other_root = [91; 64];
        let mixed = row.recomputed_commitment().unwrap();
        assert!(row.authenticates(&mixed));
        assert!(!full_parameter_matches_v4(&tensor, 2, &mixed).unwrap());
        assert!(full_parameter_matches_v4(&tensor, 2, &misaka_palw_kernel::merkle::tensor_commitment_v2(&tensor)).unwrap());
    }
    #[test]
    fn actual_suite_and_full_parameter_value_are_never_interchanged() {
        let original = Tensor::new(DType::I32, vec![2, 2], vec![1, 2, 3, 4]).unwrap();
        let changed = Tensor::new(DType::I32, vec![2, 2], vec![1, 2, 3, 5]).unwrap();
        let v2 = misaka_palw_kernel::merkle::tensor_commitment_v2(&original);
        let v3 = misaka_palw_kernel::merkle3::tensor_commitment_v3(&original);
        assert!(full_parameter_matches_v4(&original, 3, &v3).unwrap());
        assert!(!full_parameter_matches_v4(&changed, 2, &v2).unwrap());
        assert!(!full_parameter_matches_v4(&changed, 3, &v3).unwrap());
        assert!(!full_parameter_matches_v4(&original, 3, &v2).unwrap());
        assert!(full_parameter_matches_v4(&original, 99, &v2).is_err());
    }
}

#[cfg(test)]
#[path = "provider_reader_v4_memory_tests.rs"]
mod memory_tests;
