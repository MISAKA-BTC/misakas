//! RFC08 checker consumer for the versioned complete kernel codec (TypedKernelV4): every row/tile/part/slice/entry unit of
//! an assembled bundle is first authenticated against the claim's native commitments (`PalwTypedKernelMaterialBundleV4::
//! verify`, which also proves the inventory is complete), then reconstructed into the ordinary `PaidClaimMaterialWireV2`
//! the existing deterministic checker reads (`VerifiedPaidClaimSourcesV2::verify_material`).
//!
//! Reconstruction is the exact inverse of the producer's `build_kernel_provider_handoff_*`:
//! - stage 255: authenticated parameter rows (suite 2) or row tiles (suite 3) reassembled into whole artifact tensors;
//! - stage 254: segmented (position, part) responses reassembled with `seg_da::assemble_position_v1` (withheld/hidden values
//!   stay the empty placeholders the native verifier replaces with its own value);
//! - committed positions (incl. the memory pre-state): the served values/inputs of `classify_served_position_v1`;
//! - snapshot slices and returned entries: the authenticated items keyed by (snapshot root, id).
//! Tokens are public job/claim data the caller supplies; they are not part of the provider's liability.
//!
//! Merkle openings authenticate data membership only; the verdict is the checker's deterministic recomputation
//! (PRINCIPLES §8). A reconstructed material is never a receipt, credit or Final by itself.
use super::{PaidClaimMaterialWireV2, Refusal, provider_handoff_v4::retrieval};
use kaspa_consensus_core::{
    palw_kernel_parameter_material_v4::{
        PALW_KERNEL_PARAMETER_STAGE_V4, PALW_KERNEL_SEGMENTED_PART_STAGE_V4, PalwKernelParameterOpeningV4, PalwKernelParameterRowV4,
        PalwKernelSegmentedPartV4, kernel_parameter_at_v4,
    },
    palw_native_resource_v1::{NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_prospective_kernel_material_v4::PalwTypedKernelMaterialBundleV4,
};
use misaka_palw_kernel::{
    hash::Digest,
    ledger::{ClaimBodyV1, KernelLedgerV1},
    public::TensorWireV1,
    spec::{
        RETRIEVAL_ENTRY_STAGE_BASE_V1, SNAPSHOT_STAGE_BASE_V1,
        retrieval::{EntryResponseV1, SliceResponseV1},
    },
};
use std::collections::BTreeMap;

fn bad(e: impl ToString) -> Refusal {
    Refusal::new("TYPED_MATERIAL_V4", e.to_string())
}
fn add(a: u64, b: u64) -> Result<u64, Refusal> {
    a.checked_add(b).ok_or_else(|| bad("typed material reconstruction size overflow"))
}
fn mul(a: u64, b: u64) -> Result<u64, Refusal> {
    a.checked_mul(b).ok_or_else(|| bad("typed material reconstruction size overflow"))
}

/// Explicit checker-side caps fixed before any decode.
#[derive(Clone, Copy, Debug)]
pub struct TypedMaterialLimitsV4 {
    /// The signed per-contract material byte cap (the bundle's encoded size bound).
    pub max_material_bytes: u64,
    /// The decoded reconstruction workspace (whole tensors, maps, placeholders) this checker allows.
    pub max_workspace_bytes: u64,
}

/// The reconstructed material plus the reservations that cover its decoded allocations; keep it alive while checking.
pub struct ReconstructedPaidMaterialV4 {
    pub material: PaidClaimMaterialWireV2,
    _leases: Vec<Option<NativeDemandLeaseV1>>,
}

struct Budget {
    used: u64,
    cap: u64,
    leases: Vec<Option<NativeDemandLeaseV1>>,
}
impl Budget {
    fn reserve(&mut self, n: u64) -> Result<(), Refusal> {
        let next = add(self.used, n)?;
        if next > self.cap {
            return Err(bad("typed material reconstruction exceeds the checker's explicit workspace"));
        }
        self.leases.push(reserve_native_requests_v1(n).map_err(bad)?);
        self.used = next;
        Ok(())
    }
}

/// Authenticate and reconstruct. `ledger` must come from the checker's own authenticated selected chain (the same rows
/// `paid_claim_sources_from_kernel_rows_v2` was built from).
pub fn reconstruct_paid_material_v4(
    ledger: &KernelLedgerV1,
    bundle: &PalwTypedKernelMaterialBundleV4,
    tokens: Vec<u32>,
    limits: TypedMaterialLimitsV4,
) -> Result<ReconstructedPaidMaterialV4, Refusal> {
    // Authentication and completeness first: every unit against its native commitment, exact inventory, canonical order.
    bundle.verify(ledger, limits.max_material_bytes).map_err(bad)?;
    let claim = bundle.claim.as_bytes();
    let row = ledger.claims.get(&claim).ok_or_else(|| bad("typed material claim absent"))?;
    let mut budget = Budget { used: 0, cap: limits.max_workspace_bytes, leases: Vec::new() };
    // Map/node envelopes for every unit, before any insertion.
    budget.reserve(mul(bundle.units.len() as u64, 256)?)?;
    let mut material = PaidClaimMaterialWireV2 { tokens, ..Default::default() };
    // stage 255: (program, parameter, layer) -> (dtype, shape, row_len, assembled i128 values, filled count)
    let mut parameters: BTreeMap<(u16, u16, Option<u16>), (misaka_palw_tir::DType, Vec<usize>, u64, Vec<i128>, u64)> = BTreeMap::new();
    let mut segmented: BTreeMap<u32, Vec<Vec<u8>>> = BTreeMap::new();
    for (stage, position, bytes) in &bundle.units {
        let (stage, position) = (*stage, *position);
        if stage == PALW_KERNEL_PARAMETER_STAGE_V4 {
            let unit: PalwKernelParameterRowV4 = borsh::from_slice(bytes).map_err(bad)?;
            let (spec, at) = kernel_parameter_at_v4(ledger, &claim, position).map_err(bad)?;
            let key = (spec.program, spec.parameter, spec.layer);
            if !parameters.contains_key(&key) {
                let elements = mul(spec.rows, spec.row_len)?;
                // i128 assembly buffer + the final dtype-width wire bytes + the shape vectors.
                budget.reserve(add(mul(elements, 16)?, add(mul(elements, spec.declaration.dtype.width() as u64)?, 256)?)?)?;
                let mut values = Vec::new();
                values.try_reserve_exact(usize::try_from(elements).map_err(bad)?).map_err(bad)?;
                values.resize(elements as usize, 0i128);
                let shape = spec.declaration.shape.iter().map(|d| *d as usize).collect();
                parameters.insert(key, (spec.declaration.dtype, shape, spec.row_len, values, 0));
            }
            let (_, _, row_len, values, filled) = parameters.get_mut(&key).ok_or_else(|| bad("parameter buffer absent"))?;
            let (offset, slice): (u64, &[i128]) = match &unit.opening {
                PalwKernelParameterOpeningV4::RowV2(opening) => {
                    if opening.values.len() as u64 != *row_len || at != opening.index {
                        return Err(bad("authenticated parameter row has a foreign length or index"));
                    }
                    (mul(opening.index, *row_len)?, &opening.values)
                }
                PalwKernelParameterOpeningV4::TileV3(opening) => {
                    let start = mul(opening.tile, misaka_palw_kernel::merkle3::TILE_V3)?;
                    let expected = row_len.saturating_sub(start).min(misaka_palw_kernel::merkle3::TILE_V3);
                    if opening.values.len() as u64 != expected || expected == 0 {
                        return Err(bad("authenticated parameter tile has a foreign length"));
                    }
                    (add(mul(opening.line, *row_len)?, start)?, &opening.values)
                }
            };
            let end = add(offset, slice.len() as u64)?;
            if end > values.len() as u64 {
                return Err(bad("authenticated parameter unit lies outside its tensor"));
            }
            values[offset as usize..end as usize].copy_from_slice(slice);
            *filled = add(*filled, slice.len() as u64)?;
        } else if stage == PALW_KERNEL_SEGMENTED_PART_STAGE_V4 {
            let unit: PalwKernelSegmentedPartV4 = borsh::from_slice(bytes).map_err(bad)?;
            budget.reserve(add(unit.bytes.len() as u64, 64)?)?;
            segmented.entry(unit.position).or_default().push(unit.bytes);
        } else if row.body.position(stage, position).is_some() {
            let served = ledger.classify_served_position_v1(&claim, stage, position, bytes).map_err(bad)?;
            budget.reserve(add(mul(bytes.len() as u64, 2)?, 256)?)?;
            for (o, nodes) in served.values.into_iter().enumerate() {
                for (n, value) in nodes.into_iter().enumerate() {
                    // A derived value is never served; the checker rebuilds it from its own recomputation.
                    if let Some(value) = value {
                        material.nodes.insert((stage, position, o as u16, n as u16), value);
                    }
                }
            }
            for (i, input) in served.inputs.into_iter().enumerate() {
                material.inputs.insert((stage, position, i as u16), input);
            }
        } else if stage >= RETRIEVAL_ENTRY_STAGE_BASE_V1 {
            let (root, result) = retrieval(ledger, row, stage - RETRIEVAL_ENTRY_STAGE_BASE_V1)?;
            let entry = result.get(position as usize).ok_or_else(|| bad("authenticated entry outside the claim's result"))?;
            let response: EntryResponseV1 = borsh::from_slice(bytes).map_err(bad)?;
            budget.reserve(add(mul(bytes.len() as u64, 2)?, 128)?)?;
            material.snapshot_items.insert((root.snapshot.root(), entry.id), response.item);
        } else if stage >= SNAPSHOT_STAGE_BASE_V1 {
            let (root, _) = retrieval(ledger, row, stage - SNAPSHOT_STAGE_BASE_V1)?;
            let range = root.snapshot.slice_range(u64::from(position)).ok_or_else(|| bad("authenticated slice outside the snapshot"))?;
            let response: SliceResponseV1 = borsh::from_slice(bytes).map_err(bad)?;
            if response.items.len() as u64 != range.end - range.start {
                return Err(bad("authenticated slice has a foreign item count"));
            }
            budget.reserve(add(mul(bytes.len() as u64, 2)?, 128)?)?;
            let snapshot = root.snapshot.root();
            for (id, item) in range.zip(response.items) {
                material.snapshot_items.insert((snapshot, id), item);
            }
        } else {
            return Err(bad("typed unit namespace has no reconstruction consumer"));
        }
    }
    for ((program, parameter, layer), (dtype, shape, _, values, filled)) in parameters {
        if filled != values.len() as u64 {
            return Err(bad("an artifact tensor was not fully covered by its authenticated units"));
        }
        let tensor = misaka_palw_tir::Tensor::new(dtype, shape, values).map_err(bad)?;
        material.params.insert((program, parameter, layer), TensorWireV1::of(&tensor));
    }
    if !segmented.is_empty() {
        let ClaimBodyV1::Segmented { evidence, segment_roots, .. } = &row.body else {
            return Err(bad("segmented parts on a claim of another kind"));
        };
        let class = ledger.classes.get(&row.class_binding_id).ok_or_else(|| bad("segmented class absent"))?;
        for (position, parts) in segmented {
            let (values, siblings, _commitments) =
                misaka_palw_kernel::seg_da::assemble_position_v1(&class.program, segment_roots, evidence.positions, position, &parts)
                    .ok_or_else(|| bad("segmented position is incomplete or unauthentic"))?;
            let wires: Vec<Vec<TensorWireV1>> = values.iter().map(|o| o.iter().map(TensorWireV1::of).collect()).collect();
            material.segmented_positions.insert(position, wires);
            material.segmented_siblings.insert(position, siblings);
        }
    }
    Ok(ReconstructedPaidMaterialV4 { material, _leases: budget.leases })
}

/// Digest of the reconstructed material for audit logs (identity only).
pub fn reconstructed_material_digest_v4(material: &PaidClaimMaterialWireV2) -> Result<Digest, Refusal> {
    Ok(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/RECONSTRUCTED-PAID-MATERIAL/V4", material))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_reconstruction_workspace_is_an_explicit_cap_reserved_before_growth() {
        let mut budget = Budget { used: 0, cap: 100, leases: Vec::new() };
        budget.reserve(60).unwrap();
        assert!(budget.reserve(41).is_err(), "a reservation past the explicit cap is refused");
        assert!(budget.reserve(u64::MAX).is_err(), "overflow is refused");
    }
}
