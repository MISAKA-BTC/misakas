//! RFC23 V2 physical checkpoint: constant-size descriptor, normalized work
//! records, and material references into the SAME authenticated state. The old
//! `C`/V1 codec is not reinterpreted. Logical readers still receive the complete
//! V1 checkpoint, with every referenced byte resolved under a retained lease.
use super::*;
use crate::palw_claim_local_v1::*;
use crate::palw_native_resource_v1::{BTreeNodeEnvelopeV1, NativeRetainedV1, NativeWireScanV1};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_CLAIM_DESCRIPTOR_PREFIX_V2: u8 = b'D';
pub const PALW_CLAIM_RECORD_PREFIX_V2: u8 = b'V';
pub const PALW_CLAIM_REFERENCE_PREFIX_V2: u8 = b'Q';
pub const PALW_CLAIM_BLOB_PREFIX_V2: u8 = b'M';
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PalwClaimDescriptorV2 {
    pub version: u16,
    pub point: PalwBlockContextV2,
    pub records_root: Hash64,
    pub records_count: u64,
    pub material_root: Hash64,
    pub material_count: u64,
    pub liability: Option<Hash64>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PalwClaimRecordRowV2 {
    pub version: u16,
    pub record: PalwClaimLocalRecordV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PalwClaimMaterialReferenceV2 {
    pub version: u16,
    pub id: Hash64,
    pub len: u64,
    /// None addresses immutable `M || id`; Some addresses an existing immutable
    /// native aux row. This is availability/identity, not computation approval.
    pub auxiliary: Option<(u8, Vec<u8>)>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PalwClaimMaterialBlobV2 {
    pub version: u16,
    pub bytes: Vec<u8>,
}
crate::palw_native_resource_v1::native_struct!(PalwClaimDescriptorV2;version,point,records_root,records_count,material_root,material_count,liability);
crate::palw_native_resource_v1::native_struct!(PalwClaimRecordRowV2;version,record);
crate::palw_native_resource_v1::native_struct!(PalwClaimMaterialReferenceV2;version,id,len,auxiliary);
crate::palw_native_resource_v1::native_struct!(PalwClaimMaterialBlobV2;version,bytes);
crate::palw_native_resource_v1::wire::wire_struct!(PalwClaimDescriptorV2;u16,PalwBlockContextV2,Hash64,u64,Hash64,u64,Option<Hash64>);
crate::palw_native_resource_v1::wire::wire_struct!(PalwClaimRecordRowV2;u16,PalwClaimLocalRecordV1);
crate::palw_native_resource_v1::wire::wire_struct!(PalwClaimMaterialReferenceV2;u16,Hash64,u64,Option<(u8,Vec<u8>)>);
crate::palw_native_resource_v1::wire::wire_struct!(PalwClaimMaterialBlobV2;u16,Vec<u8>);
#[derive(BorshSerialize)]
struct BorrowedRecord<'a> {
    version: u16,
    record: &'a PalwClaimLocalRecordV1,
}
#[derive(BorshSerialize)]
struct BorrowedBlob<'a> {
    version: u16,
    bytes: &'a [u8],
}
fn bad(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
}
fn wire_fault(error: std::io::Error) -> PalwStateV2Error {
    match error.kind() {
        std::io::ErrorKind::OutOfMemory => PalwStateV2Error::NativeResource(
            crate::palw_native_resource_v1::NativeCoverageV1::overflow::<PalwClaimDescriptorV2>("checkpoint V2 wire"),
        ),
        std::io::ErrorKind::Unsupported | std::io::ErrorKind::TimedOut => {
            PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<PalwClaimDescriptorV2>(
                "checkpoint V2 wire",
                &error.to_string(),
            ))
        }
        _ => bad(error),
    }
}
fn decode<T: BorshDeserialize + BorshSerialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, PalwStateV2Error> {
    let shape = crate::palw_native_resource_v1::preflight_native_value_v1::<T>(raw, raw.len() as u64).map_err(wire_fault)?;
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(shape.logical_requests_v1())
        .map_err(PalwStateV2Error::NativeResource)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(borsh::from_slice(raw).map_err(bad)?, lease))
}
pub(super) fn is_prefix(prefix: u8) -> bool {
    matches!(
        prefix,
        PALW_CLAIM_DESCRIPTOR_PREFIX_V2 | PALW_CLAIM_RECORD_PREFIX_V2 | PALW_CLAIM_REFERENCE_PREFIX_V2 | PALW_CLAIM_BLOB_PREFIX_V2
    )
}
pub(super) fn validate_key(key: &[u8]) -> Result<(), PalwStateV2Error> {
    let valid = match key.first().copied() {
        Some(PALW_CLAIM_DESCRIPTOR_PREFIX_V2) => key == palw_claim_descriptor_key_v2(),
        Some(PALW_CLAIM_RECORD_PREFIX_V2) => {
            matches!(key.len(), 66 | 70 | 130 | 134) && borsh::from_slice::<PalwClaimLocalWorkRefV1>(&key[1..]).is_ok()
        }
        Some(PALW_CLAIM_REFERENCE_PREFIX_V2 | PALW_CLAIM_BLOB_PREFIX_V2) => key.len() == 65,
        _ => false,
    };
    if !valid {
        return Err(bad("checkpoint V2 allocated key namespace"));
    }
    Ok(())
}

pub fn palw_claim_descriptor_key_v2() -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = PALW_CLAIM_DESCRIPTOR_PREFIX_V2;
    key
}
fn id_key(prefix: u8, id: Hash64) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = prefix;
    key[1..].copy_from_slice(id.as_byte_slice());
    key
}
fn work_key(work: PalwClaimLocalWorkRefV1) -> Vec<u8> {
    let mut key = Vec::with_capacity(134);
    key.push(PALW_CLAIM_RECORD_PREFIX_V2);
    work.serialize(&mut key).expect("fixed WorkRef");
    key
}
fn rows(state: &PalwChainStateV2, prefix: u8) -> impl Iterator<Item = (&Vec<u8>, &Vec<u8>)> {
    state
        .kernel_route
        .iter()
        .flat_map(|r| r.aux.iter())
        .filter(move |((table, key), _)| *table == PALW_CLAIM_LOCAL_TABLE_V1 && key.first() == Some(&prefix))
        .map(|((_, key), value)| (key, value))
}
fn row<'a>(state: &'a PalwChainStateV2, table: u8, key: &[u8]) -> Option<&'a Vec<u8>> {
    state.kernel_route.as_ref()?.aux.iter().find(|((t, k), _)| *t == table && k.as_slice() == key).map(|(_, raw)| raw)
}
pub(super) fn root(state: &PalwChainStateV2, prefix: u8) -> (Hash64, u64) {
    let count = rows(state, prefix).count() as u64;
    let mut hash = misaka_palw_kernel::hash::keyed(b"misaka-palw/claim-checkpoint/normalized-rows/v2");
    hash.update(&[prefix]);
    hash.update(&count.to_le_bytes());
    for (key, value) in rows(state, prefix) {
        hash.update(&(key.len() as u64).to_le_bytes());
        hash.update(key);
        hash.update(&(value.len() as u64).to_le_bytes());
        hash.update(value);
    }
    (Hash64::from_bytes(misaka_palw_kernel::hash::finish(hash)), count)
}
pub(super) fn immutable_aux(table: u8, key: &[u8]) -> bool {
    (table == PALW_CLAIM_LOCAL_TABLE_V1 && key.len() == 65 && key[0] == b'F')
        || (table == crate::palw_court_family_material_v1::FAMILY_MATERIAL_TABLE_V1
            && ((key.len() == 65 && matches!(key[0], 3 | 4)) || (key.len() == 73 && key[0] == 2)))
        || (table == crate::palw_court_family_supplier_v1::FAMILY_SUPPLIER_TABLE_V1
            && (key.len() == 64 || (key.len() == 129 && key[0] == 2)))
}
fn blob(state: &PalwChainStateV2, id: Hash64) -> Result<&[u8], PalwStateV2Error> {
    let raw = row(state, PALW_CLAIM_LOCAL_TABLE_V1, &id_key(PALW_CLAIM_BLOB_PREFIX_V2, id))
        .ok_or_else(|| bad("checkpoint material blob is missing"))?;
    if raw.len() < 6 || u16::from_le_bytes(raw[..2].try_into().unwrap()) != 2 {
        return Err(bad("checkpoint blob codec"));
    }
    let size = u32::from_le_bytes(raw[2..6].try_into().unwrap()) as usize;
    if size != raw.len() - 6 || palw_liability_material_id_v1(&raw[6..]) != id {
        return Err(bad("checkpoint blob length/identity"));
    }
    Ok(&raw[6..])
}
fn resolve<'a>(state: &'a PalwChainStateV2, reference: &PalwClaimMaterialReferenceV2) -> Result<&'a [u8], PalwStateV2Error> {
    if reference.version != 2 {
        return Err(bad("checkpoint reference version"));
    }
    let data = if let Some((table, key)) = &reference.auxiliary {
        if !immutable_aux(*table, key) {
            return Err(bad("checkpoint reference is not an immutable supported native row"));
        }
        {
            let raw = row(state, *table, key).ok_or_else(|| bad("checkpoint native material reference is missing"))?;
            if *table == crate::palw_court_family_supplier_v1::FAMILY_SUPPLIER_TABLE_V1
                && key.len() == 64
                && raw.get(..2) != Some(&2u16.to_le_bytes())
            {
                return Err(bad("checkpoint finance reference is not immutable V2"));
            }
            raw.as_slice()
        }
    } else {
        blob(state, reference.id)?
    };
    if data.len() as u64 != reference.len || palw_liability_material_id_v1(data) != reference.id {
        return Err(bad("checkpoint referenced material length/identity"));
    }
    Ok(data)
}

/// Resolve only from this exact rooted state; a descriptor alone is never an
/// owned material capability. Admission precedes all body copies and trees.
pub(super) fn load(state: &PalwChainStateV2) -> Result<Option<NativeRetainedV1<PalwClaimLocalCheckpointV1>>, PalwStateV2Error> {
    let Some(raw) = row(state, PALW_CLAIM_LOCAL_TABLE_V1, &palw_claim_descriptor_key_v2()) else {
        if state
            .kernel_route
            .as_ref()
            .is_some_and(|r| r.aux.keys().any(|(t, k)| *t == PALW_CLAIM_LOCAL_TABLE_V1 && k.first().is_some_and(|p| is_prefix(*p))))
        {
            return Err(bad("orphan normalized checkpoint rows"));
        }
        return Ok(None);
    };
    if row(state, PALW_CLAIM_LOCAL_TABLE_V1, &super::palw_claim_local_checkpoint_fold_v1::palw_claim_local_checkpoint_key_v1())
        .is_some()
    {
        return Err(bad("both checkpoint codecs are present"));
    }
    let descriptor = decode::<PalwClaimDescriptorV2>(raw)?;
    if descriptor.version != 2 || Some(&descriptor.point) != state.last_point() {
        return Err(bad("checkpoint descriptor differs from actual snapshot point"));
    }
    if root(state, PALW_CLAIM_RECORD_PREFIX_V2) != (descriptor.records_root, descriptor.records_count)
        || root(state, PALW_CLAIM_REFERENCE_PREFIX_V2) != (descriptor.material_root, descriptor.material_count)
    {
        return Err(bad("checkpoint normalized row root/count"));
    }
    for prefix in
        [PALW_CLAIM_DESCRIPTOR_PREFIX_V2, PALW_CLAIM_RECORD_PREFIX_V2, PALW_CLAIM_REFERENCE_PREFIX_V2, PALW_CLAIM_BLOB_PREFIX_V2]
    {
        for (key, _) in rows(state, prefix) {
            validate_key(key)?;
        }
    }
    let mut requests = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    requests
        .exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1, PalwClaimLocalRecordV1>>(descriptor.records_count)
        .map_err(PalwStateV2Error::NativeResource)?;
    requests.exact_v1::<BTreeNodeEnvelopeV1<Hash64, Vec<u8>>>(descriptor.material_count).map_err(PalwStateV2Error::NativeResource)?;
    for (key, raw) in rows(state, PALW_CLAIM_RECORD_PREFIX_V2) {
        let shape =
            crate::palw_native_resource_v1::preflight_native_value_v1::<PalwClaimRecordRowV2>(raw.as_slice(), raw.len() as u64)
                .map_err(wire_fault)?;
        requests.add_v1(shape.logical_requests_v1()).map_err(PalwStateV2Error::NativeResource)?;
        if !matches!(key.len(), 66 | 70 | 130 | 134) {
            return Err(bad("checkpoint WorkRef key width"));
        }
    }
    for (key, raw) in rows(state, PALW_CLAIM_REFERENCE_PREFIX_V2) {
        if key.len() != 65 {
            return Err(bad("checkpoint reference key width"));
        }
        let reference = decode::<PalwClaimMaterialReferenceV2>(raw)?;
        if key[1..] != *reference.id.as_byte_slice() {
            return Err(bad("checkpoint reference key identity"));
        }
        let body = resolve(state, &reference)?;
        requests.exact_v1::<u8>(body.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    }
    for (key, _) in rows(state, PALW_CLAIM_BLOB_PREFIX_V2) {
        let id = Hash64::from_bytes(key[1..].try_into().expect("checked blob key"));
        if descriptor.liability != Some(id) {
            let raw = row(state, PALW_CLAIM_LOCAL_TABLE_V1, &id_key(PALW_CLAIM_REFERENCE_PREFIX_V2, id))
                .ok_or_else(|| bad("orphan checkpoint blob"))?;
            if decode::<PalwClaimMaterialReferenceV2>(raw)?.auxiliary.is_some() {
                return Err(bad("unreferenced checkpoint blob version"));
            }
        }
        blob(state, id)?;
    }
    let liability = descriptor.liability.map(|id| blob(state, id)).transpose()?.unwrap_or(&[]);
    requests.exact_v1::<u8>(liability.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let lease =
        crate::palw_native_resource_v1::reserve_native_requests_v1(requests.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut records = BTreeMap::new();
    let mut material = BTreeMap::new();
    for (key, raw) in rows(state, PALW_CLAIM_RECORD_PREFIX_V2) {
        let entry = decode::<PalwClaimRecordRowV2>(raw)?;
        if entry.version != 2 || borsh::from_slice::<PalwClaimLocalWorkRefV1>(&key[1..]).map_err(bad)? != entry.record.work {
            return Err(bad("checkpoint normalized record identity"));
        }
        let (entry, decode_lease) = entry.into_parts_v1();
        records.insert(entry.record.work, entry.record);
        drop(decode_lease);
    }
    for (_, raw) in rows(state, PALW_CLAIM_REFERENCE_PREFIX_V2) {
        let reference = decode::<PalwClaimMaterialReferenceV2>(raw)?;
        material.insert(reference.id, resolve(state, &reference)?.to_vec());
    }
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(
        PalwClaimLocalCheckpointV1 { point: descriptor.point, records, liability_state: liability.to_vec(), material },
        lease,
    )))
}

/// Publish a complete physical generation atomically. Old blob versions are
/// reclaimed; their ordinary delta restores them on reorg, not a permanent copy
/// of every world's carriage in every later material transaction.
pub(super) fn write(b: &mut TransitionBuilder<'_>, checkpoint: &PalwClaimLocalCheckpointV1) -> Result<(), PalwStateV2Error> {
    let save = b.try_checkpoint()?;
    let result = write_inner(b, checkpoint);
    if result.is_err() {
        b.restore(save)
    }
    result
}
fn write_inner(b: &mut TransitionBuilder<'_>, checkpoint: &PalwClaimLocalCheckpointV1) -> Result<(), PalwStateV2Error> {
    let count = b.state.kernel_route.as_ref().map_or(0, |r| r.aux.len()) as u64;
    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, (u8, &Vec<u8>, &Vec<u8>)>>(count).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(checkpoint.material.len() as u64 + 1).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<Vec<u8>>(count).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<PalwClaimMaterialReferenceV2>(checkpoint.material.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<u8>(
        (checkpoint.material.len() as u64).checked_mul(129).ok_or(PalwStateV2Error::Overflow("checkpoint reference keys"))?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<u8>((checkpoint.records.len() as u64).checked_mul(134).ok_or(PalwStateV2Error::Overflow("checkpoint work keys"))?)
        .map_err(PalwStateV2Error::NativeResource)?;
    if let Some(route) = &b.state.kernel_route {
        for (_, key) in route.aux.keys() {
            q.exact_v1::<u8>(key.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        }
    }
    let _scratch =
        crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    // Save only the coordinates. The candidate/state stays borrowed while
    // indexing; after that no body-sized copied lookup structure exists.
    let references = {
        let mut index = BTreeMap::new();
        if let Some(route) = &b.state.kernel_route {
            for ((table, key), raw) in &route.aux {
                if immutable_aux(*table, key)
                    && !(*table == crate::palw_court_family_supplier_v1::FAMILY_SUPPLIER_TABLE_V1
                        && key.len() == 64
                        && raw.get(..2) != Some(&2u16.to_le_bytes()))
                {
                    index.entry(palw_liability_material_id_v1(raw)).or_insert((*table, key, raw));
                }
            }
        }
        let mut refs = Vec::with_capacity(checkpoint.material.len());
        for (id, body) in &checkpoint.material {
            let auxiliary = if let Some((table, key, raw)) = index.get(id) {
                if raw.as_slice() != body.as_slice() {
                    return Err(bad("checkpoint material hash collision"));
                }
                Some((*table, (*key).clone()))
            } else {
                None
            };
            refs.push(PalwClaimMaterialReferenceV2 { version: 2, id: *id, len: body.len() as u64, auxiliary });
        }
        refs
    };
    let mut blobs = BTreeSet::new();
    for reference in &references {
        if reference.auxiliary.is_none() {
            blobs.insert(reference.id);
            put_blob(b, reference.id, &checkpoint.material[&reference.id])?;
        }
        b.write_kernel_row_encoded_v1(
            PALW_CLAIM_LOCAL_TABLE_V1,
            &id_key(PALW_CLAIM_REFERENCE_PREFIX_V2, reference.id),
            Some(reference),
        )?;
    }
    let liability = if checkpoint.liability_state.is_empty() {
        None
    } else {
        let id = palw_liability_material_id_v1(&checkpoint.liability_state);
        blobs.insert(id);
        put_blob(b, id, &checkpoint.liability_state)?;
        Some(id)
    };
    for (work, record) in &checkpoint.records {
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &work_key(*work), Some(&BorrowedRecord { version: 2, record }))?;
    }
    let mut obsolete = Vec::with_capacity(count as usize);
    if let Some(route) = &b.state.kernel_route {
        for (table, key) in route.aux.keys() {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 {
                continue;
            }
            let remove = match key.first().copied() {
                Some(PALW_CLAIM_RECORD_PREFIX_V2) => {
                    let work = borsh::from_slice::<PalwClaimLocalWorkRefV1>(&key[1..]).map_err(bad)?;
                    !checkpoint.records.contains_key(&work)
                }
                Some(PALW_CLAIM_REFERENCE_PREFIX_V2) => {
                    key.len() == 65 && !checkpoint.material.contains_key(&Hash64::from_bytes(key[1..].try_into().unwrap()))
                }
                Some(PALW_CLAIM_BLOB_PREFIX_V2) => {
                    key.len() == 65 && !blobs.contains(&Hash64::from_bytes(key[1..].try_into().unwrap()))
                }
                Some(b'C') => true,
                _ => false,
            };
            if remove {
                obsolete.push(key.clone())
            }
        }
    }
    for key in obsolete {
        b.write_kernel_row(PALW_CLAIM_LOCAL_TABLE_V1, key, None)?;
    }
    let (records_root, records_count) = root(&b.state, PALW_CLAIM_RECORD_PREFIX_V2);
    let (material_root, material_count) = root(&b.state, PALW_CLAIM_REFERENCE_PREFIX_V2);
    let descriptor = PalwClaimDescriptorV2 {
        version: 2,
        point: checkpoint.point,
        records_root,
        records_count,
        material_root,
        material_count,
        liability,
    };
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &palw_claim_descriptor_key_v2(), Some(&descriptor))
}
fn put_blob(b: &mut TransitionBuilder<'_>, id: Hash64, bytes: &[u8]) -> Result<(), PalwStateV2Error> {
    if row(&b.state, PALW_CLAIM_LOCAL_TABLE_V1, &id_key(PALW_CLAIM_BLOB_PREFIX_V2, id)).is_some() {
        if blob(&b.state, id)? != bytes {
            return Err(bad("immutable checkpoint blob substitution"));
        }
        return Ok(());
    }
    b.write_kernel_row_encoded_v1(
        PALW_CLAIM_LOCAL_TABLE_V1,
        &id_key(PALW_CLAIM_BLOB_PREFIX_V2, id),
        Some(&BorrowedBlob { version: 2, bytes }),
    )
}
