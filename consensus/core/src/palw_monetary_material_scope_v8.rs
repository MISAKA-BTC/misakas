//! UNHASHED native derived direct membership index. No signed/serialized terms,
//! fees, state roots or duty horizons change. Reverse EXEC remains separate.
use super::*;
use crate::palw_monetary_material_v1::{PALW_MONETARY_MATERIAL_TABLE_V1 as TABLE, PalwMonetaryMaterialRowV1 as Row};
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1 as Node, NativeRetainedV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, TirNativeRequestsV1,
    reserve_native_requests_v1,
};
fn bad(s: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.into())
}
fn decode(raw: &[u8]) -> Result<NativeRetainedV1<Row>, PalwStateV2Error> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        _ => bad("monetary member native row codec"),
    })
}
fn members(raw: Option<&Vec<u8>>, root: Hash64) -> Result<NativeRetainedV1<Vec<Hash64>>, PalwStateV2Error> {
    let row = raw.map(|r| decode(r)).transpose()?;
    let count = row.as_ref().map_or(0, |r| r.terms.dependencies.len() + 1 + usize::from(r.native_binding.is_some()));
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Hash64>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut out = Vec::with_capacity(count);
    if let Some(r) = row {
        if r.terms.root != root {
            return Err(bad("monetary member root/key differs"));
        }
        out.push(root);
        out.extend(r.terms.dependencies.iter().copied());
        if let Some(binding) = &r.native_binding {
            out.push(binding.claim)
        }
    }
    out.sort_unstable();
    out.dedup();
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
pub(super) struct MemberUpdateV8 {
    root: Hash64,
    old: NativeRetainedV1<Vec<Hash64>>,
    new: NativeRetainedV1<Vec<Hash64>>,
    absent: u64,
}
fn plan(s: &PalwChainStateV2, key: &[u8], old: Option<&Vec<u8>>, new: Option<&Vec<u8>>) -> Result<MemberUpdateV8, PalwStateV2Error> {
    let root = borsh::from_slice::<Hash64>(key).map_err(|_| bad("monetary member native key codec"))?;
    let old = members(old, root)?;
    let new = members(new, root)?;
    for member in old.iter() {
        if !s.material_contracts_by_member_v8.contains(&(*member, root)) {
            return Err(bad("monetary member old derived row missing"));
        }
    }
    let absent = new.iter().filter(|member| !s.material_contracts_by_member_v8.contains(&(**member, root))).count() as u64;
    Ok(MemberUpdateV8 { root, old, new, absent })
}
pub(super) fn prepare_writer_v8(
    b: &mut TransitionBuilder<'_>,
    table: u8,
    key: &[u8],
    new: &Option<Vec<u8>>,
) -> Result<Option<MemberUpdateV8>, PalwStateV2Error> {
    if table != TABLE || b.state.kernel_route.is_none() {
        return Ok(None);
    }
    let _key = reserve_native_requests_v1(key.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let old = b.state.kernel_route.as_ref().and_then(|r| r.aux.get(&(table, key.to_vec())));
    let p = plan(&b.state, key, old, new.as_ref())?;
    let bytes = p
        .absent
        .checked_mul(std::mem::size_of::<Node<(Hash64, Hash64), ()>>() as u64)
        .ok_or_else(|| bad("monetary member growth overflow"))?;
    b.native_reservation.grow_v1(bytes).map_err(PalwStateV2Error::NativeResource)?;
    Ok(Some(p))
}
pub(super) fn prepare_delta_v8(
    s: &PalwChainStateV2,
    table: u8,
    key: &[u8],
    expected: &Option<Vec<u8>>,
    install: &Option<Vec<u8>>,
) -> Result<Option<NativeRetainedV1<MemberUpdateV8>>, PalwStateV2Error> {
    if table != TABLE {
        return Ok(None);
    }
    let _key = reserve_native_requests_v1(key.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    if s.kernel_route.as_ref().and_then(|r| r.aux.get(&(table, key.to_vec()))) != expected.as_ref() {
        return Err(PalwStateV2Error::DeltaMismatch("monetary member expected native row differs"));
    }
    let p = plan(s, key, expected.as_ref(), install.as_ref())?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Node<(Hash64, Hash64), ()>>(p.absent).map_err(PalwStateV2Error::NativeResource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(p, lease)))
}
pub(super) fn apply_v8(s: &mut PalwChainStateV2, p: &MemberUpdateV8) {
    for m in p.old.iter() {
        if p.new.binary_search(m).is_err() {
            s.material_contracts_by_member_v8.remove(&(*m, p.root));
        }
    }
    for m in p.new.iter() {
        if p.old.binary_search(m).is_err() {
            s.material_contracts_by_member_v8.insert((*m, p.root));
        }
    }
}
pub(super) fn direct_roots_v8<'a>(s: &'a PalwChainStateV2, source: Hash64) -> impl Iterator<Item = Hash64> + 'a {
    s.material_contracts_by_member_v8
        .range((source, Hash64::default())..=(source, Hash64::from_bytes([255; 64])))
        .map(|(_, root)| *root)
}
/// Cold restore only. The up-front retained carriage scanner includes exact
/// parsed direct-member node demand; the local per-row lease admits workspace.
pub(super) fn rebuild_v8(s: &mut PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    let mut next = BTreeSet::new();
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table == TABLE {
                let root = borsh::from_slice::<Hash64>(key).map_err(|_| bad("cold monetary member key codec"))?;
                let ms = members(Some(raw), root)?;
                let mut q = TirNativeRequestsV1::new();
                q.exact_v1::<Node<(Hash64, Hash64), ()>>(ms.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
                let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
                for m in ms.iter() {
                    next.insert((*m, root));
                }
            }
        }
    }
    s.material_contracts_by_member_v8 = next;
    Ok(())
}
pub(super) fn validate_v8(s: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    let mut count = 0u64;
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table == TABLE {
                let root = borsh::from_slice::<Hash64>(key).map_err(|_| bad("monetary member native key codec"))?;
                let ms = members(Some(raw), root)?;
                count = count.checked_add(ms.len() as u64).ok_or_else(|| bad("monetary member count overflow"))?;
                for m in ms.iter() {
                    if !s.material_contracts_by_member_v8.contains(&(*m, root)) {
                        return Err(bad("monetary direct member index incomplete"));
                    }
                }
            }
        }
    }
    if count != s.material_contracts_by_member_v8.len() as u64 {
        return Err(bad("monetary direct member index has orphan membership"));
    }
    Ok(())
}
/// Same literal Row codec as generated WireScan. Returning the direct-member
/// count separately avoids confusing nested provider/signature bytes with nodes.
pub(crate) fn scan_material_member_index_wire_v8<R: std::io::Read>(
    r: &mut NativeWireReaderV1<R>,
) -> std::io::Result<(NativeWireShapeV1, u64)> {
    use crate::palw_monetary_material_v1::*;
    let mut sum = NativeWireShapeV1::default();
    macro_rules! scan {
        ($t:ty) => {
            sum = sum.merge(<$t as NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        };
    }
    scan!(u16);
    scan!(Hash64);
    let deps = <Vec<Hash64> as NativeWireScanV1>::scan_native_wire_v1(r)?;
    let members = deps.outer_entries.checked_add(1).ok_or_else(|| std::io::Error::other("monetary direct member overflow"))?;
    sum = sum.merge(deps)?;
    scan!(Option<Hash64>);
    scan!(PalwBondKeyV2);
    scan!(Hash64);
    scan!(Hash64);
    scan!(u64);
    scan!(Option<crate::palw_material_capacity_v1::PalwMaterialManifestV1>);
    scan!(Option<crate::palw_attempt_material_v3::PalwNativeMaterialSessionV3>);
    scan!(u64);
    scan!(u64);
    scan!(bool);
    scan!(Hash64);
    scan!(u64);
    scan!(u64);
    scan!(bool);
    r.step()?;
    let binding = r.byte()?;
    let members = match binding {
        0 => members,
        1 => {
            scan!(crate::palw_attempt_material_v3::PalwNativeMaterialBindingV3);
            members.checked_add(1).ok_or_else(|| std::io::Error::other("monetary binding member overflow"))?
        }
        _ => {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "monetary native binding Option"));
        }
    };
    scan!(Vec<PalwMonetaryMaterialProviderV1>);
    scan!(Vec<PalwMonetaryMaterialChallengeV1>);
    scan!(Option<u64>);
    scan!(Option<Hash64>);
    scan!(Option<u64>);
    scan!(Option<PalwMonetaryMaterialPaymentObservationV1>);
    scan!(Hash64);
    scan!(u64);
    scan!(u128);
    scan!(u128);
    scan!(Hash64);
    scan!(BTreeMap<Hash64,Hash64>);
    Ok((sum, members))
}
pub(crate) fn material_member_replay_shape_v8(bytes: &[u8]) -> std::io::Result<NativeWireShapeV1> {
    let mut r = NativeWireReaderV1::new(bytes, bytes.len() as u64);
    let (_, members) = scan_material_member_index_wire_v8(&mut r)?;
    if r.remaining() != 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "monetary derived member trailing bytes"));
    }
    NativeWireShapeV1::default().allocation(members, std::mem::size_of::<Node<(Hash64, Hash64), ()>>())
}
