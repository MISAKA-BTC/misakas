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
        PalwMaterialCodecV1::KernelPositions => shape::<misaka_palw_kernel::public::PositionResponseV1>(data),
        PalwMaterialCodecV1::SegmentedParts => shape::<misaka_palw_kernel::seg_da::SegPartResponseV1>(data),
    }
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
                    1 | 2 => {
                        let bundle = if codec == 1 {
                            shape::<PalwKernelMaterialBundleV1>(data)?
                        } else {
                            shape::<PalwSegmentedMaterialBundleV1>(data)?
                        };
                        let mut inner = Cursor(data);
                        inner.take(64)?;
                        let count = inner.count(PALW_MATERIAL_MAX_UNITS_V1)?;
                        let mut total = bundle;
                        for _ in 0..count {
                            inner.take(if codec == 1 { 5 } else { 8 })?;
                            let n = inner.count(u32::MAX as usize)?;
                            let body = inner.take(n)?;
                            total = add(
                                total,
                                inner_response(
                                    if codec == 1 {
                                        PalwMaterialCodecV1::KernelPositions
                                    } else {
                                        PalwMaterialCodecV1::SegmentedParts
                                    },
                                    body,
                                )?,
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
impl NativeRetainedV1<Store> {
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
            PalwMaterialCodecV1::KernelPositions => shape::<PalwKernelMaterialBundleV1>(raw)?,
            PalwMaterialCodecV1::SegmentedParts => shape::<PalwSegmentedMaterialBundleV1>(raw)?,
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
