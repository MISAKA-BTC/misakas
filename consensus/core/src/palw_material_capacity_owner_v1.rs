//! Owner-specific pre-allocation and lifetime tickets for provider snapshots.
//! Raw signed material bytes, decoded native ownership and verifier scratch are
//! separate terms. This is logical allocation coverage, not physical RSS approval.
use super::*;
use crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1;
use crate::palw_native_resource_v1::{
    NativeCoverageV1 as C, NativeHeapV1, NativeRetainedV1, NativeWireReaderV1, NativeWireScanV1, preflight_native_value_v1,
    reserve_native_requests_v1,
};
use std::mem::size_of;
type Store = PalwMonetaryMaterialProviderStoreV1;
const COMPONENT: &str = "monetary provider ownership";
fn add(a: C, b: C) -> C {
    a.add::<Store>(b, COMPONENT)
}
fn bytes(n: u64) -> C {
    C::Bounded { bytes: n }
}
fn counted<T: BorshSerialize>(v: &T) -> Result<u64, String> {
    borsh::object_length(v).map(|n| n as u64).map_err(|e| e.to_string())
}
fn fixed(n: u64, unit: usize) -> C {
    n.checked_mul(unit as u64).map(bytes).unwrap_or_else(|| C::overflow::<Store>(COMPONENT))
}
fn shape<T: NativeWireScanV1>(data: &[u8]) -> Result<C, String> {
    let mut reader = NativeWireReaderV1::new(data, data.len() as u64);
    let shape = T::scan_native_wire_v1(&mut reader).map_err(|e| e.to_string())?;
    if reader.remaining() != 0 {
        return Err("provider embedded codec has trailing bytes".into());
    }
    // A typed input/returned response generation, verifier operand projections,
    // and dedup membership. Byte-vector heap is counted by the typed scanner.
    let heap = shape.heap.checked_add(size_of::<T>() as u64).ok_or("provider typed heap overflow")?;
    Ok(add(
        bytes(heap).scale::<T>(3, COMPONENT),
        fixed(shape.collection_entries, size_of::<BTreeNodeEnvelopeV1<PalwHeldMissingV1, ()>>()),
    ))
}
fn bundle_input<T: NativeHeapV1 + BorshSerialize>(bundle: &T) -> Result<C, String> {
    let heap = bundle.native_heap_v1(COMPONENT);
    let members = bundle.native_members_v1().ok_or("provider typed memberships overflow")?;
    let encoded = counted(bundle)?;
    Ok(add(
        add(heap.scale::<T>(3, COMPONENT), fixed(encoded, 3)),
        fixed(members, size_of::<BTreeNodeEnvelopeV1<PalwHeldMissingV1, ()>>()),
    ))
}
fn inner_response(codec: PalwMaterialCodecV1, data: &[u8]) -> Result<C, String> {
    match codec {
        PalwMaterialCodecV1::NativeHeldComplete => shape::<PalwHeldMaterialBundleV1>(data),
        PalwMaterialCodecV1::NativeHeldCompleteV2 => Err("routed V2 response requires typed native authority".into()),
        PalwMaterialCodecV1::KernelPositions => shape::<misaka_palw_kernel::public::PositionResponseV1>(data),
        PalwMaterialCodecV1::SegmentedParts => shape::<misaka_palw_kernel::seg_da::SegPartResponseV1>(data),
        PalwMaterialCodecV1::TypedKernelV4 => Err("typed response preflight requires its signed namespace".into()),
    }
}
fn typed_response(stage: u8, data: &[u8]) -> Result<C, String> {
    use crate::palw_kernel_parameter_material_v4::{
        PALW_KERNEL_PARAMETER_STAGE_V4, PALW_KERNEL_SEGMENTED_PART_STAGE_V4, PalwKernelParameterRowV4, PalwKernelSegmentedPartV4,
    };
    if stage == PALW_KERNEL_PARAMETER_STAGE_V4 {
        shape::<PalwKernelParameterRowV4>(data)
    } else if stage == PALW_KERNEL_SEGMENTED_PART_STAGE_V4 {
        let outer = shape::<PalwKernelSegmentedPartV4>(data)?;
        let mut cursor = Cursor(data);
        cursor.take(8)?;
        let n = cursor.count(u32::MAX as usize)?;
        let inner = cursor.take(n)?;
        if !cursor.0.is_empty() {
            return Err("typed segmented unit trailing bytes".into());
        }
        Ok(add(outer, inner_response(PalwMaterialCodecV1::SegmentedParts, inner)?))
    } else if stage >= misaka_palw_kernel::spec::RETRIEVAL_ENTRY_STAGE_BASE_V1 {
        retrieval_response_requests_v4(data, false)
    } else if stage >= misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1 {
        retrieval_response_requests_v4(data, true)
    } else {
        shape::<misaka_palw_kernel::public::PositionResponseV1>(data)
    }
}
/// Before decoding a checker-fetched codec3 unit. This is allocation coverage,
/// never an assertion about the correctness of model computation.
pub fn kernel_material_unit_requests_v4(stage: u8, data: &[u8]) -> Result<C, String> {
    if data.len() as u64 > crate::palw_kernel_parameter_material_v4::PALW_KERNEL_MATERIAL_UNIT_BYTES_V4 {
        return Err("kernel material unit exceeds its typed wire bound".into());
    }
    typed_response(stage, data)
}
/// Authenticator scratch is derived from the actual returned codec and the
/// caller's authenticated claim/class, separately from the raw decode owner.
pub fn kernel_material_unit_auth_requests_v4(
    stage: u8,
    data: &[u8],
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    claim: &misaka_palw_kernel::hash::Digest,
) -> Result<C, String> {
    use crate::{
        palw_kernel_parameter_material_v4::{
            PALW_KERNEL_PARAMETER_STAGE_V4, PALW_KERNEL_SEGMENTED_PART_STAGE_V4, PalwKernelParameterRowV4, PalwKernelSegmentedPartV4,
        },
        palw_native_resource_v1::{
            native_kernel_parameter_auth_requests_v4, native_kernel_position_auth_requests_v4, native_kernel_segment_auth_requests_v4,
        },
    };
    kernel_material_unit_requests_v4(stage, data)?;
    if stage == PALW_KERNEL_PARAMETER_STAGE_V4 {
        decoded_auth::<PalwKernelParameterRowV4>(data, native_kernel_parameter_auth_requests_v4)
    } else if stage == PALW_KERNEL_SEGMENTED_PART_STAGE_V4 {
        let count = shape::<PalwKernelSegmentedPartV4>(data)?.bytes().ok_or("segment wrapper capacity overflow")?;
        let _codec = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
        let unit: PalwKernelSegmentedPartV4 = borsh::from_slice(data).map_err(|e| e.to_string())?;
        let row = ledger.claims.get(claim).ok_or("unit authentication claim absent")?;
        let class = ledger.classes.get(&row.class_binding_id).ok_or("unit authentication class absent")?;
        decoded_auth::<misaka_palw_kernel::seg_da::SegPartResponseV1>(&unit.bytes, |r| {
            native_kernel_segment_auth_requests_v4(&class.program, unit.position, r)
        })
    } else if stage < misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1 {
        decoded_auth::<misaka_palw_kernel::public::PositionResponseV1>(data, native_kernel_position_auth_requests_v4)
    } else {
        // Slice/Entry i128/output projections were counted by the structural
        // raw-shape scanner; their borrowed Merkle paths do not build a tree.
        Ok(C::zero())
    }
}
/// A structural scan counts the exact key/payload lanes before the native
/// retrieval classifier expands i32/u32 values into i128 tensors and returns
/// `[key,payload]` TensorWire pairs. Merkle authentication hashes borrowed
/// scalar/path bytes and allocates no snapshot-sized tree.
fn retrieval_response_requests_v4(data: &[u8], slice: bool) -> Result<C, String> {
    use misaka_palw_kernel::{
        public::TensorWireV1,
        spec::retrieval::{EntryResponseV1, MAX_RETRIEVAL_DIM_V1, MAX_RETRIEVAL_PAYLOAD_V1, MAX_SLICE_ITEMS_V1, SliceResponseV1},
    };
    let mut requests = if slice { shape::<SliceResponseV1>(data)? } else { shape::<EntryResponseV1>(data)? };
    let mut c = Cursor(data);
    let items = if slice { c.count(MAX_SLICE_ITEMS_V1 as usize)? } else { 1 };
    requests = add(requests, fixed(items as u64, size_of::<Vec<Option<TensorWireV1>>>() + 2 * size_of::<Option<TensorWireV1>>()));
    for _ in 0..items {
        let key = c.count(MAX_RETRIEVAL_DIM_V1 as usize)?;
        c.take(key.checked_mul(4).ok_or("retrieval key extent overflow")?)?;
        let payload = c.count(MAX_RETRIEVAL_PAYLOAD_V1 as usize)?;
        c.take(payload.checked_mul(4).ok_or("retrieval payload extent overflow")?)?;
        requests = add(requests, fixed((key as u64) + (payload as u64), size_of::<i128>() + 4));
        requests = add(requests, fixed(2, size_of::<usize>() + size_of::<u64>()));
    }
    let paths = if slice { c.count(MAX_SLICE_ITEMS_V1 as usize)? } else { 1 };
    if paths != items {
        return Err("retrieval path/item cardinality differs".into());
    }
    for _ in 0..paths {
        let n = c.count(40)?;
        c.take(n.checked_mul(64).ok_or("retrieval path extent overflow")?)?;
    }
    if !c.0.is_empty() {
        return Err("retrieval response trailing bytes".into());
    }
    Ok(requests)
}
fn ledger_scratch(ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>) -> C {
    ledger
        .map(|v| v.native_heap_v1(COMPONENT).scale::<misaka_palw_kernel::ledger::KernelLedgerV1>(2, COMPONENT))
        .unwrap_or_else(C::zero)
}
fn growth(
    manifest: &PalwMaterialManifestV1,
    held: &[PalwHeldMaterialBundleV1],
    positions: &[PalwKernelMaterialBundleV1],
    segmented: &[PalwSegmentedMaterialBundleV1],
    ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
) -> Result<C, String> {
    let mut sum = add(manifest.native_heap_v1(COMPONENT).scale::<PalwMaterialManifestV1>(3, COMPONENT), ledger_scratch(ledger));
    sum = add(sum, fixed(1, size_of::<BTreeNodeEnvelopeV1<Hash64, (Hash64, PalwMaterialManifestV1, BTreeMap<Hash64, Vec<u8>>)>>()));
    sum = add(
        sum,
        fixed(
            manifest.scopes.len() as u64,
            size_of::<BTreeNodeEnvelopeV1<Hash64, Vec<u8>>>() + 2 * size_of::<(PalwMaterialScopeV1, Vec<u8>)>(),
        ),
    );
    for bundle in held {
        sum = add(sum, bundle_input(bundle)?);
    }
    for bundle in positions {
        sum = add(sum, bundle_input(bundle)?);
        for (_, _, data) in &bundle.positions {
            sum = add(sum, inner_response(PalwMaterialCodecV1::KernelPositions, data)?);
        }
    }
    for bundle in segmented {
        sum = add(sum, bundle_input(bundle)?);
        for (_, _, data) in &bundle.parts {
            sum = add(sum, inner_response(PalwMaterialCodecV1::SegmentedParts, data)?);
        }
    }
    Ok(sum)
}

struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let h = self.0.get(..n).ok_or("provider store truncated")?;
        self.0 = &self.0[n..];
        Ok(h)
    }
    fn count(&mut self, cap: usize) -> Result<usize, String> {
        let n = u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize;
        if n > cap { Err("provider nested capacity exceeded".into()) } else { Ok(n) }
    }
}
/// Walk embedded bundle and response bytes without first allocating the outer
/// Store or a manifest. Each frame also passes its generated exact typed scanner.
fn recovery_requests(encoded: &[u8], ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>) -> Result<C, String> {
    let outer = preflight_native_value_v1::<Store>(encoded, encoded.len() as u64).map_err(|e| e.to_string())?;
    let mut sum = add(bytes(outer.logical_requests_v1()).scale::<Store>(3, COMPONENT), ledger_scratch(ledger));
    let mut c = Cursor(encoded);
    let contracts = c.count(crate::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_MAX_CONTRACTS_V1 as usize)?;
    for _ in 0..contracts {
        c.take(128)?; // consent -> (manifest id, manifest root, scopes, data)
        c.take(64)?;
        let scopes = c.count(PALW_MATERIAL_MAX_SCOPES_V1)?;
        let mut codecs = [(Hash64::default(), 0u8); PALW_MATERIAL_MAX_SCOPES_V1];
        for scope in codecs.iter_mut().take(scopes) {
            scope.0 = Hash64::from_bytes(c.take(64)?.try_into().unwrap());
            c.take(64)?;
            scope.1 = c.take(1)?[0];
            c.take(8 + 4 + 64)?;
        }
        let rows = c.count(PALW_MATERIAL_MAX_SCOPES_V1)?;
        for _ in 0..rows {
            let claim = Hash64::from_bytes(c.take(64)?.try_into().unwrap());
            let n = c.count(u32::MAX as usize)?;
            let data = c.take(n)?;
            let codec = codecs[..scopes].iter().find(|(id, _)| *id == claim).ok_or("provider bundle has no signed scope")?.1;
            sum = add(
                sum,
                match codec {
                    0 => shape::<PalwHeldMaterialBundleV1>(data)?,
                    4 => shape::<crate::palw_routed_material_v2::PalwHeldMaterialBundleV2>(data)?,
                    1 | 2 | 3 => {
                        let bundle = if codec == 1 {
                            shape::<PalwKernelMaterialBundleV1>(data)?
                        } else if codec == 3 {
                            shape::<crate::palw_prospective_kernel_material_v4::PalwTypedKernelMaterialBundleV4>(data)?
                        } else {
                            shape::<PalwSegmentedMaterialBundleV1>(data)?
                        };
                        let mut inner = Cursor(data);
                        inner.take(64)?;
                        let count = inner.count(PALW_MATERIAL_MAX_UNITS_V1)?;
                        let mut total = bundle;
                        for _ in 0..count {
                            let stage = inner.take(if codec == 2 { 8 } else { 5 })?[0];
                            let n = inner.count(u32::MAX as usize)?;
                            let body = inner.take(n)?;
                            total = add(
                                total,
                                if codec == 3 {
                                    typed_response(stage, body)?
                                } else {
                                    inner_response(
                                        if codec == 1 {
                                            PalwMaterialCodecV1::KernelPositions
                                        } else {
                                            PalwMaterialCodecV1::SegmentedParts
                                        },
                                        body,
                                    )?
                                },
                            );
                        }
                        if !inner.0.is_empty() {
                            return Err("provider bundle trailing bytes".into());
                        }
                        total
                    }
                    _ => return Err("provider codec is unknown".into()),
                },
            );
        }
    }
    if !c.0.is_empty() {
        return Err("provider outer store trailing bytes".into());
    }
    Ok(sum)
}
pub fn prospective_kernel_bundle_requests_v4(
    bundle: &crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
) -> Result<C, String> {
    use crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4 as B;
    Ok(match bundle {
        B::KernelPositions(b) => {
            let mut n = bundle_input(b)?;
            for (_, _, data) in &b.positions {
                n = add(n, inner_response(PalwMaterialCodecV1::KernelPositions, data)?);
            }
            n
        }
        B::SegmentedParts(b) => {
            let mut n = bundle_input(b)?;
            for (_, _, data) in &b.parts {
                n = add(n, inner_response(PalwMaterialCodecV1::SegmentedParts, data)?);
            }
            n
        }
        B::TypedKernelV4(b) => {
            let mut n = bundle_input(b)?;
            for (stage, _, data) in &b.units {
                n = add(n, typed_response(*stage, data)?);
            }
            n
        }
    })
}
fn decoded_auth<T: NativeWireScanV1 + BorshDeserialize>(
    data: &[u8],
    quote: impl FnOnce(&T) -> Result<u64, crate::palw_native_resource_v1::NativeCoverageV1>,
) -> Result<C, String> {
    let count = shape::<T>(data)?.bytes().ok_or("kernel response codec capacity overflow")?;
    let _codec = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
    let value: T = borsh::from_slice(data).map_err(|e| e.to_string())?;
    quote(&value).map(bytes).map_err(|e| e.to_string())
}
/// Raw decoding has a separate, prior reservation. These requests cover the
/// actual native tensor expansion/Merkle authentication, never model correctness.
pub fn prospective_kernel_bundle_auth_requests_v4(
    bundle: &crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
) -> Result<C, String> {
    use crate::{
        palw_kernel_parameter_material_v4::{
            PALW_KERNEL_PARAMETER_STAGE_V4, PALW_KERNEL_SEGMENTED_PART_STAGE_V4, PalwKernelParameterRowV4, PalwKernelSegmentedPartV4,
        },
        palw_native_resource_v1::{
            native_kernel_parameter_auth_requests_v4, native_kernel_position_auth_requests_v4, native_kernel_segment_auth_requests_v4,
        },
        palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4 as B,
    };
    let position =
        |raw: &[u8]| decoded_auth::<misaka_palw_kernel::public::PositionResponseV1>(raw, native_kernel_position_auth_requests_v4);
    let segment = |claim: Hash64, p: u32, raw: &[u8]| -> Result<C, String> {
        let row = ledger.claims.get(&claim.as_bytes()).ok_or("segment authentication claim absent")?;
        let class = ledger.classes.get(&row.class_binding_id).ok_or("segment authentication class absent")?;
        decoded_auth::<misaka_palw_kernel::seg_da::SegPartResponseV1>(raw, |r| {
            native_kernel_segment_auth_requests_v4(&class.program, p, r)
        })
    };
    let mut total = C::zero();
    match bundle {
        B::KernelPositions(b) => {
            for (_, _, raw) in &b.positions {
                total = add(total, position(raw)?);
            }
        }
        B::SegmentedParts(b) => {
            for (p, _, raw) in &b.parts {
                total = add(total, segment(b.claim, *p, raw)?);
            }
        }
        B::TypedKernelV4(b) => {
            for (stage, _, raw) in &b.units {
                let requests = if *stage == PALW_KERNEL_PARAMETER_STAGE_V4 {
                    decoded_auth::<PalwKernelParameterRowV4>(raw, native_kernel_parameter_auth_requests_v4)?
                } else if *stage == PALW_KERNEL_SEGMENTED_PART_STAGE_V4 {
                    let count = shape::<PalwKernelSegmentedPartV4>(raw)?.bytes().ok_or("segment wrapper capacity overflow")?;
                    let _codec = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
                    let unit: PalwKernelSegmentedPartV4 = borsh::from_slice(raw).map_err(|e| e.to_string())?;
                    segment(b.claim, unit.position, &unit.bytes)?
                } else if *stage < misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1 {
                    position(raw)?
                } else {
                    C::zero()
                };
                total = add(total, requests);
            }
        }
    }
    Ok(total)
}
impl NativeRetainedV1<Store> {
    pub fn provider_kernel_bundle_owned_v4(
        &self,
        consent: Hash64,
        dependency: Hash64,
    ) -> Result<NativeRetainedV1<crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4>, String> {
        use crate::palw_prospective_kernel_material_v4::{PalwProspectiveKernelBundleV4 as B, PalwTypedKernelMaterialBundleV4};
        let (_, manifest, rows) = self.contracts.get(&consent).ok_or("kernel bundle consent absent")?;
        let scope = manifest.scope(dependency).ok_or("kernel bundle signed scope absent")?;
        let raw = rows.get(&dependency).ok_or("kernel bundle bytes absent")?;
        let requests = match scope.codec {
            PalwMaterialCodecV1::KernelPositions => shape::<PalwKernelMaterialBundleV1>(raw)?,
            PalwMaterialCodecV1::SegmentedParts => shape::<PalwSegmentedMaterialBundleV1>(raw)?,
            PalwMaterialCodecV1::TypedKernelV4 => shape::<PalwTypedKernelMaterialBundleV4>(raw)?,
            PalwMaterialCodecV1::NativeHeldComplete | PalwMaterialCodecV1::NativeHeldCompleteV2 => {
                return Err("held bundle is not kernel material".into());
            }
        };
        let requests = add(requests, fixed(1, size_of::<B>()));
        let lease =
            reserve_native_requests_v1(requests.bytes().ok_or("kernel bundle ownership overflow")?).map_err(|e| e.to_string())?;
        let value = match scope.codec {
            PalwMaterialCodecV1::KernelPositions => B::KernelPositions(borsh::from_slice(raw).map_err(|e| e.to_string())?),
            PalwMaterialCodecV1::SegmentedParts => B::SegmentedParts(borsh::from_slice(raw).map_err(|e| e.to_string())?),
            PalwMaterialCodecV1::TypedKernelV4 => B::TypedKernelV4(borsh::from_slice(raw).map_err(|e| e.to_string())?),
            PalwMaterialCodecV1::NativeHeldComplete | PalwMaterialCodecV1::NativeHeldCompleteV2 => unreachable!(),
        };
        Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
    }
    /// Retention-only recovery for kernel frames. The callback authenticates
    /// saved identity metadata, not claim acceptance or permission to answer.
    pub fn recover_provider_retaining_kernel_v4(
        encoded: &[u8],
        max_bytes: u64,
        total_capacity: u64,
        authority: impl FnMut(Hash64) -> Option<PalwMaterialNativeAuthorityV1>,
        mut retained_identity: impl FnMut(Hash64, &PalwMaterialScopeV1, &[u8]) -> Result<(), String>,
    ) -> Result<Self, String> {
        let count = recovery_requests(encoded, None)?.bytes().ok_or("provider retention snapshot native size overflow")?;
        let lease = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
        let value = Store::recover_inner_v4(encoded, max_bytes, total_capacity, None, authority, Some(&mut retained_identity), None)
            .map_err(|e| e.to_string())?;
        Ok(Self::from_owner_reserved_v1(value, lease))
    }
    pub fn provider_admit_prospective_kernel_v4(
        &self,
        consent: Hash64,
        material: &crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialOwnedV4,
        bundle: &crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
        max_bytes: u64,
        total_capacity: u64,
    ) -> Result<Self, String> {
        let manifest = material.manifest();
        let mut requests = growth(manifest, &[], &[], &[], Some(material.candidate_ledger()))?;
        requests = add(requests, prospective_kernel_bundle_requests_v4(bundle)?);
        requests = add(requests, prospective_kernel_bundle_auth_requests_v4(bundle, material.candidate_ledger())?);
        self.try_mutate_owner_v1(requests, |value| {
            value.admit_prospective_kernel_v4(consent, material, bundle, max_bytes, total_capacity)
        })
    }
    pub fn provider_admit_replacement_kernel_v4(
        &self,
        material: &crate::palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementOwnedV4,
        bundle: &crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
        max_bytes: u64,
        total_capacity: u64,
    ) -> Result<Self, String> {
        let consent = material.proposal.bounded_replacement_consent_v4().ok_or("replacement grant lacks bounded generation")?;
        let mut requests = growth(material.manifest(), &[], &[], &[], Some(material.candidate_ledger()))?;
        requests = add(requests, prospective_kernel_bundle_requests_v4(bundle)?);
        requests = add(requests, prospective_kernel_bundle_auth_requests_v4(bundle, material.candidate_ledger())?);
        self.try_mutate_owner_v1(requests, |value| {
            value.admit_replacement_kernel_v4(consent, material, bundle, max_bytes, total_capacity)
        })
    }
    pub fn provider_discard_expired_kernel_capsule_v4(
        &self,
        capsule: &crate::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2,
        witness: impl FnMut(&crate::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2) -> Result<bool, String>,
    ) -> Result<(Self, bool), String> {
        let mut expired = false;
        let next = self.try_mutate_owner_v1(C::zero(), |value| {
            expired = value.discard_expired_kernel_capsule_v4(capsule, witness)?;
            Ok::<_, PalwMaterialCapacityErrorV1>(())
        })?;
        Ok((next, expired))
    }
    pub fn recover_provider_verified_v1(
        encoded: &[u8],
        max_bytes: u64,
        total_capacity: u64,
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        authority: impl FnMut(Hash64) -> Option<PalwMaterialNativeAuthorityV1>,
    ) -> Result<Self, String> {
        let requests = recovery_requests(encoded, ledger)?;
        let count = requests.bytes().ok_or_else(|| requests.to_string())?;
        let lease = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
        let value =
            Store::recover_verified_inner_v1(encoded, max_bytes, total_capacity, ledger, authority).map_err(|e| e.to_string())?;
        Ok(Self::from_owner_reserved_v1(value, lease))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn provider_admit_material_v1(
        &self,
        consent: Hash64,
        manifest: &PalwMaterialManifestV1,
        max_bytes: u64,
        total_capacity: u64,
        held: &[PalwHeldMaterialBundleV1],
        positions: &[PalwKernelMaterialBundleV1],
        segmented: &[PalwSegmentedMaterialBundleV1],
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        authority: impl FnMut(Hash64) -> Option<PalwMaterialNativeAuthorityV1>,
    ) -> Result<Self, String> {
        self.try_mutate_owner_v1(growth(manifest, held, positions, segmented, ledger)?, |value| {
            value.admit_material(consent, manifest, max_bytes, total_capacity, held, positions, segmented, ledger, authority)
        })
    }
    pub fn provider_release_finalized_v1(
        &self,
        root: Hash64,
        stored: Hash64,
        release: Hash64,
        tail: Hash64,
        block: crate::BlockHash,
        witness: impl FnMut(Hash64, Hash64, Hash64, crate::BlockHash) -> Result<bool, String>,
    ) -> Result<(Self, bool), String> {
        let mut released = false;
        let next = self.try_mutate_owner_v1(C::zero(), |value| {
            released = value.release_if_finalized(root, stored, release, tail, block, witness)?;
            Ok::<_, PalwMaterialCapacityErrorV1>(())
        })?;
        Ok((next, released))
    }
    pub fn provider_discard_expired_offer_v1(
        &self,
        enrollment: &crate::palw_monetary_material_v1::PalwMonetaryMaterialEnrollmentV1,
        witness: impl FnMut(Hash64, u64) -> Result<bool, String>,
    ) -> Result<(Self, bool), String> {
        let mut expired = false;
        let next = self.try_mutate_owner_v1(C::zero(), |value| {
            expired = value.discard_expired_offer(enrollment, witness)?;
            Ok::<_, PalwMaterialCapacityErrorV1>(())
        })?;
        Ok((next, expired))
    }
    pub fn provider_answer_action_v1(
        &self,
        consent: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: crate::palw_state_v2::PalwBondKeyV2,
        demand: PalwMonetaryMaterialDemandV1,
    ) -> Result<NativeRetainedV1<crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1>, String> {
        let (_, manifest, rows) = self.contracts.get(&consent).ok_or("provider answer consent absent")?;
        let scope = manifest.scope(dependency).ok_or("provider answer scope absent")?;
        let raw = rows.get(&dependency).ok_or("provider answer bundle absent")?;
        let requests = match scope.codec {
            PalwMaterialCodecV1::NativeHeldComplete => shape::<PalwHeldMaterialBundleV1>(raw)?,
            PalwMaterialCodecV1::NativeHeldCompleteV2 => {
                return Err("V2 retained bytes require a fresh native routing authority and V2 response".into());
            }
            PalwMaterialCodecV1::KernelPositions => shape::<PalwKernelMaterialBundleV1>(raw)?,
            PalwMaterialCodecV1::SegmentedParts => shape::<PalwSegmentedMaterialBundleV1>(raw)?,
            PalwMaterialCodecV1::TypedKernelV4 => {
                shape::<crate::palw_prospective_kernel_material_v4::PalwTypedKernelMaterialBundleV4>(raw)?
            }
        };
        let count = requests.bytes().ok_or_else(|| requests.to_string())?;
        let lease = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
        let action = self.answer_action(consent, root, dependency, provider, demand).map_err(|e| e.to_string())?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(action, lease))
    }
    /// Encoded publication buffer coexists with the retained candidate. Count
    /// its actual Borsh bytes before allocating, not the declared raw capacity.
    pub fn provider_encoded_v1(&self) -> Result<NativeRetainedV1<Vec<u8>>, String> {
        let len = counted(&**self)?;
        let lease =
            reserve_native_requests_v1(len.checked_mul(2).ok_or("provider publication byte overflow")?).map_err(|e| e.to_string())?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(self.encoded().map_err(|e| e.to_string())?, lease))
    }
}

impl NativeRetainedV1<Store> {
    /// V2 mutation keeps allocation refusal typed instead of passing through
    /// the legacy String-only snapshot mutation adapter.
    pub fn provider_admit_routed_held_v2(
        &self,
        consent: Hash64,
        manifest: &PalwMaterialManifestV1,
        max_bytes: u64,
        total_capacity: u64,
        bundle: &crate::palw_routed_material_v2::PalwHeldMaterialBundleV2,
        authority: &crate::palw_routed_material_v2::PalwMaterialNativeAuthorityV2,
    ) -> Result<Self, PalwMaterialCapacityErrorV1> {
        use crate::palw_native_resource_v1::{TirNativeRequestsV1, native_value_requests_v1};
        use PalwMaterialCapacityErrorV1 as E;
        bundle.verify(authority.record(), max_bytes)?;
        let mut q = TirNativeRequestsV1::new();
        q.add_v1(native_value_requests_v1(&**self).map_err(E::LocalResource)?).map_err(E::LocalResource)?;
        for _ in 0..3 {
            q.add_v1(native_value_requests_v1(bundle).map_err(E::LocalResource)?).map_err(E::LocalResource)?;
            q.add_v1(native_value_requests_v1(manifest).map_err(E::LocalResource)?).map_err(E::LocalResource)?;
        }
        let encoded = borsh::object_length(bundle).map_err(|_| E::Capacity)? as u64;
        for _ in 0..3 {
            q.vec_v1::<u8>(encoded).map_err(E::LocalResource)?;
        }
        q.vec_v1::<BTreeNodeEnvelopeV1<Hash64, (Hash64, PalwMaterialManifestV1, BTreeMap<Hash64, Vec<u8>>)>>(1)
            .map_err(E::LocalResource)?;
        q.vec_v1::<BTreeNodeEnvelopeV1<Hash64, Vec<u8>>>(1).map_err(E::LocalResource)?;
        q.vec_v1::<(PalwMaterialScopeV1, Vec<u8>)>(1).map_err(E::LocalResource)?;
        let count = q.bytes_v1();
        let lease = reserve_native_requests_v1(count).map_err(E::LocalResource)?;
        let mut value = (**self).clone();
        value.admit_routed_held_v2(consent, manifest, max_bytes, total_capacity, bundle, authority)?;
        let actual = native_value_requests_v1(&value).map_err(E::LocalResource)?;
        if actual > count {
            return Err(E::LocalResource(C::unsupported::<Store>(COMPONENT, "V2 mutation request postcondition exceeded")));
        }
        Ok(Self::from_owner_reserved_v1(value, lease))
    }
    pub fn recover_provider_routed_v2(
        encoded: &[u8],
        max_bytes: u64,
        total_capacity: u64,
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        authority_v1: impl FnMut(Hash64) -> Option<PalwMaterialNativeAuthorityV1>,
        mut authority_v2: impl FnMut(
            Hash64,
        ) -> Result<
            NativeRetainedV1<crate::palw_routed_material_v2::PalwMaterialAuthorityRecordV2>,
            PalwMaterialCapacityErrorV1,
        >,
        retained_kernel: Option<&mut dyn FnMut(Hash64, &PalwMaterialScopeV1, &[u8]) -> Result<(), String>>,
    ) -> Result<Self, PalwMaterialCapacityErrorV1> {
        use PalwMaterialCapacityErrorV1 as E;
        let requests = recovery_requests(encoded, ledger).map_err(|e| E::LocalResource(C::unsupported::<Store>(COMPONENT, &e)))?;
        let count = requests.bytes().ok_or_else(|| E::LocalResource(requests))?;
        let lease = reserve_native_requests_v1(count).map_err(E::LocalResource)?;
        let value = Store::recover_inner_v4(
            encoded,
            max_bytes,
            total_capacity,
            ledger,
            authority_v1,
            retained_kernel,
            Some(&mut authority_v2),
        )?;
        Ok(Self::from_owner_reserved_v1(value, lease))
    }
    /// Caller must obtain current own native authority (and current accepted
    /// generation) immediately before this exact publication. Saved metadata
    /// alone never authorizes serving or signing an answer.
    pub fn provider_routed_answer_v2(
        &self,
        consent: Hash64,
        root: Hash64,
        claim: Hash64,
        provider: crate::palw_state_v2::PalwBondKeyV2,
        leaf: u64,
        authority: &crate::palw_routed_material_v2::PalwMaterialNativeAuthorityV2,
        receipt: Option<crate::palw_routed_material_v2::PalwRoutedMaterialReceiptV2>,
        sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
    ) -> Result<NativeRetainedV1<crate::palw_routed_material_v2::PalwRoutedMaterialAnswerV2>, PalwMaterialCapacityErrorV1> {
        use crate::palw_native_resource_v1::{
            NativeDecodeErrorV1, TirNativeRequestsV1, native_value_requests_v1, retain_native_value_decode_classified_v1,
        };
        use crate::palw_routed_material_v2::*;
        use PalwMaterialCapacityErrorV1 as E;
        let (_, manifest, rows) = self.contracts.get(&consent).ok_or(E::Incomplete)?;
        let scope = manifest.scope(claim).ok_or(E::Incomplete)?;
        if manifest.root != root || scope.codec != PalwMaterialCodecV1::NativeHeldCompleteV2 || authority.record().claim != claim {
            return Err(E::Substitution);
        }
        let raw = rows.get(&claim).ok_or(E::Incomplete)?;
        let bundle =
            retain_native_value_decode_classified_v1::<PalwHeldMaterialBundleV2, std::io::Error>(raw, || borsh::from_slice(raw))
                .map_err(|e| match e {
                    NativeDecodeErrorV1::Resource(e) => E::LocalResource(e),
                    NativeDecodeErrorV1::Decode(e) => E::Authentication(e.to_string()),
                })?;
        if bundle.verify(authority.record(), scope.bytes)? != *scope {
            return Err(E::Substitution);
        }
        let wanted = PalwHeldMissingV1::StepLeaf { leaf };
        let at = bundle.units.binary_search_by_key(&wanted, |(m, _)| *m).map_err(|_| E::Incomplete)?;
        let PalwHeldMaterialUnitV2::Routed(r) = &bundle.units[at].1 else {
            return Err(E::Substitution);
        };
        let mut q = TirNativeRequestsV1::new();
        q.add_v1(native_value_requests_v1(r).map_err(E::LocalResource)?).map_err(E::LocalResource)?;
        q.vec_v1::<PalwRoutedMaterialAnswerV2>(1).map_err(E::LocalResource)?;
        q.vec_v1::<u8>(crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64).map_err(E::LocalResource)?;
        let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::LocalResource)?;
        let mut answer = PalwRoutedMaterialAnswerV2 {
            version: 2,
            claim,
            provider,
            target: PalwRoutedMaterialTargetV2::Monetary { root, consent },
            refutation: r.clone(),
            receipt,
            signature: Vec::new(),
        };
        answer.signature = sign(answer.message(authority.record().network).as_byte_slice(), PALW_ROUTED_MATERIAL_CONTEXT_V2)
            .ok_or_else(|| E::Authentication("routed material provider signer unavailable".into()))?;
        if answer.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err(E::Authentication("routed material signature length differs".into()));
        }
        Ok(NativeRetainedV1::from_owner_reserved_v1(answer, lease))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_preflight_visits_embedded_codecs_before_decoding_or_native_authority() {
        // Deliberately malformed persisted bytes, not a valid service contract.
        let consent = Hash64::from_bytes([1; 64]);
        let claim = Hash64::from_bytes([2; 64]);
        let manifest = PalwMaterialManifestV1 {
            root: Hash64::from_bytes([3; 64]),
            scopes: vec![PalwMaterialScopeV1 {
                claim,
                commitment: Hash64::from_bytes([4; 64]),
                codec: PalwMaterialCodecV1::NativeHeldComplete,
                bytes: 68,
                units: 1,
                content: Hash64::from_bytes([5; 64]),
            }],
        };
        let mut malformed = claim.as_byte_slice().to_vec();
        malformed.extend(u32::MAX.to_le_bytes());
        let mut store = Store::default();
        store.contracts.insert(consent, (manifest.id(), manifest, BTreeMap::from([(claim, malformed)])));
        let encoded = borsh::to_vec(&store).unwrap();
        assert!(
            NativeRetainedV1::<Store>::recover_provider_verified_v1(&encoded, 1000, 10000, None, |_| panic!(
                "native authority must not run before structural preflight"
            ))
            .is_err()
        );
        let empty = borsh::to_vec(&Store::default()).unwrap();
        let recovered = NativeRetainedV1::<Store>::recover_provider_verified_v1(&empty, 1000, 10000, None, |_| None).unwrap();
        assert!(recovered.held_contracts_v1().is_empty());
        assert!(recovered.provider_encoded_v1().unwrap().as_slice() == empty.as_slice());
    }
}
