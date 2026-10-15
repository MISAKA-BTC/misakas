//! Owning39 source/class journal. Fixed authenticated extents are readable
//! before payment; complete decoding and maximum derivation follow a debit.
use super::*;
use crate::{palw_kernel_route_v1::PalwKernelRouteStateV1, palw_native_execution_v39::*};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_quota11::{DerivedOrdinaryMetadataV11, TrustedEpochV11},
    flat_resources10::{ResourceAxisV10 as A, ResourceQuantitiesV10},
    flat_source39::{CheckedSourceV39, FlatSourceWireV39},
    hash::{Digest, object_id},
};
const TABLE: u8 = 112;
fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
fn bytes<T: BorshSerialize>(v: &T) -> Vec<u8> {
    borsh::to_vec(v).expect("source39 bounded canonical row")
}
fn key<T: BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    bytes(&(39u8, kind, id))
}
fn read<T: BorshDeserialize>(
    r: &PalwKernelRouteStateV1,
    kind: u8,
    id: &impl BorshSerialize,
    cap: usize,
) -> Result<T, PalwStateV2Error> {
    let raw = r
        .aux
        .get(&(TABLE, key(kind, id)))
        .filter(|v| v.len() <= cap)
        .ok_or_else(|| refused("source39 original bounded row absent"))?;
    borsh::from_slice(raw).map_err(|_| refused("source39 original row canonical grammar"))
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) struct SourceHeaderV39 {
    pub version: u16,
    pub id: Digest,
    pub bytes: u32,
    pub body: Digest,
    pub owner: PalwBondKeyV2,
    pub registered: u64,
}
impl SourceHeaderV39 {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/source-header/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) struct ClassHeaderV39 {
    pub version: u16,
    pub id: Digest,
    pub source: Digest,
    pub source_bytes: u32,
    pub class_bytes: u32,
    pub record: NativeClassRecordV39,
    pub owner: PalwBondKeyV2,
    pub registered: u64,
}
impl ClassHeaderV39 {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/class-header/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Default)]
struct CatalogV39 {
    version: u16,
    sources: Vec<Digest>,
    classes: Vec<Digest>,
    uploads: Vec<Digest>,
    next_upload: u64,
}
fn catalog(r: &PalwKernelRouteStateV1, owner: &PalwBondKeyV2) -> Result<CatalogV39, PalwStateV2Error> {
    let Some(raw) = r.aux.get(&(TABLE, key(5, owner))) else {
        return Ok(CatalogV39 { version: 39, ..Default::default() });
    };
    if raw.len() > 4096 {
        return Err(refused("source39 catalog byte bound"));
    }
    let c: CatalogV39 = borsh::from_slice(raw).map_err(|_| refused("source39 catalog grammar"))?;
    if c.version != 39
        || [&c.sources, &c.classes, &c.uploads]
            .iter()
            .any(|v| v.len() > 8 || v.iter().enumerate().any(|(i, id)| *id == [0; 64] || v[..i].contains(id)))
    {
        return Err(refused("source39 original bounded catalog differs"));
    }
    Ok(c)
}
pub(super) fn source_header(r: &PalwKernelRouteStateV1, id: Digest) -> Result<SourceHeaderV39, PalwStateV2Error> {
    let (root, h): (Digest, SourceHeaderV39) = read(r, 0, &id, 512)?;
    if h.version != 39
        || h.id != id
        || h.root() != root
        || h.bytes == 0
        || h.bytes as usize > SOURCE_UPLOAD_MAX_BYTES_V39
        || h.body == [0; 64]
    {
        return Err(refused("source39 original fixed header differs"));
    }
    Ok(h)
}
pub(super) fn class_header(r: &PalwKernelRouteStateV1, id: Digest) -> Result<ClassHeaderV39, PalwStateV2Error> {
    let (root, h): (Digest, ClassHeaderV39) = read(r, 2, &id, 2048)?;
    if h.version != 39
        || h.id != id
        || h.record.root() != id
        || h.root() != root
        || h.source != h.record.source
        || h.source_bytes == 0
        || h.source_bytes as usize > SOURCE_UPLOAD_MAX_BYTES_V39
        || h.class_bytes != 262
    {
        return Err(refused("source39 original class fixed header differs"));
    }
    Ok(h)
}
/// The caller authenticates these original fixed extents and debits its
/// current ordinary or original funded restoration entitlement before calling.
pub(super) fn restore_class(
    r: &PalwKernelRouteStateV1,
    p: &NativeExecutionPoliciesV39,
    id: Digest,
) -> Result<CheckedNativeClassV39, PalwStateV2Error> {
    let h = class_header(r, id)?;
    let sh = source_header(r, h.source)?;
    let raw: Vec<u8> = read(r, 1, &h.source, SOURCE_UPLOAD_MAX_BYTES_V39 + 4)?;
    if raw.len() != sh.bytes as usize
        || sh.bytes != h.source_bytes
        || object_id(b"misaka-palw/native/source-body/v39", &raw) != sh.body
    {
        return Err(refused("source39 original body extent/hash differs"));
    }
    let source = FlatSourceWireV39::from_bytes(&raw).map_err(refused)?.check().map_err(refused)?;
    if source.root() != h.source {
        return Err(refused("source39 original canonical identity differs"));
    }
    let raw: Vec<u8> = read(r, 3, &id, 266)?;
    let class = NativeClassWireV39::from_bytes(&raw).map_err(refused)?.check(&source, p).map_err(refused)?;
    if raw.len() != h.class_bytes as usize || class.root() != h.id || class.record() != &h.record {
        return Err(refused("source39 complete original class derivation differs"));
    }
    Ok(class)
}
pub(super) fn policy(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<NativeExecutionPoliciesV39, PalwStateV2Error> {
    let e = builder.extras.kernel_route.as_ref().ok_or_else(|| refused("source39 original route extras absent"))?;
    let f = e
        .native_execution39
        .as_ref()
        .filter(|f| f.activation.is_active(ctx.daa_score))
        .ok_or_else(|| refused("source39 owning fence inactive"))?;
    let final_policy = e
        .native_final
        .as_ref()
        .filter(|f| f.activation.is_active(ctx.daa_score))
        .ok_or_else(|| refused("source39 independent Final fence inactive"))?;
    let p = f.policies.clone();
    let funded = e
        .funded_execution
        .as_ref()
        .filter(|f| f.activation.is_active(ctx.daa_score))
        .ok_or_else(|| refused("source39 original shared execution policy absent"))?;
    if funded.policies != p.execution
        || final_policy.policy.root() != p.final_policy
        || e.native_carriage_service.as_ref().map(|s| s.root()) != Some(p.physical_service)
    {
        return Err(refused("source39 original shared/final/physical policies differ"));
    }
    super::palw_flat_model_vector_fold_v3::enabled(builder, ctx)?;
    p.check(builder.state.kernel_route.as_ref().unwrap().header.policy.network_domain).map_err(refused)?;
    Ok(p)
}
fn prepay(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    restore: u64,
    body: u64,
    held: u64,
) -> Result<bool, PalwStateV2Error> {
    if restore > SOURCE_UPLOAD_MAX_BYTES_V39 as u64 || body > SOURCE_UPLOAD_MAX_BYTES_V39 as u64 {
        return Err(refused("source39 original restoration/body byte profile exceeded"));
    }
    let total = restore.checked_add(body).ok_or_else(|| refused("source39 metadata extent overflow"))?;
    let work =
        total.checked_mul(4096).and_then(|n| n.checked_add(65536)).ok_or_else(|| refused("source39 actual decode tariff overflow"))?;
    let Some(fee) = super::palw_flat_model_vector_fold_v3::charge(builder, ctx, signer, work, false)? else {
        return Ok(false);
    };
    builder.slash_bond(*signer, fee)?;
    let owned = total
        .checked_mul(8)
        .and_then(|n| n.checked_add(held))
        .and_then(|n| n.checked_add(65536))
        .ok_or_else(|| refused("source39 complete retained metadata extent overflow"))?;
    let mut quantities = ResourceQuantitiesV10::default();
    for (axis, value) in [
        (A::MetadataWork, work as u128),
        (A::ScalarSteps, work as u128),
        (A::IndexSteps, total as u128 * 64),
        (A::ValidatedElements, total as u128),
        (A::CopiedReferenceBytes, total as u128 * 8),
        (A::OwnedPeakBytes, owned as u128),
        (A::CourtScratchBytes, owned as u128),
        (A::StoredEvidenceBytes, held as u128),
        (A::ContainerAllocatorAllowance, total as u128 * 4 + 65536),
        (A::Attempts, 1),
    ] {
        quantities.set(axis, value);
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let actor = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(signer);
    let mut shard = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, actor, [0; 64])?;
    shard
        .debit_ordinary(&p.execution.quota, &epoch, &DerivedOrdinaryMetadataV11::from_native_envelope(quantities))
        .map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, [0; 64]);
    Ok(true)
}
fn register_source(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.is_empty() || body.len() > SOURCE_UPLOAD_MAX_BYTES_V39 {
        return Err(refused("source39 canonical source body extent"));
    }
    let mut catalog = catalog(builder.state.kernel_route.as_ref().unwrap(), signer)?;
    if catalog.sources.len() >= p.catalog_cap as usize {
        return Err(refused("source39 original owner source catalog full"));
    }
    if !prepay(builder, ctx, signer, p, 0, body.len() as u64, body.len() as u64)? {
        return Ok(());
    }
    let source = FlatSourceWireV39::from_bytes(body).map_err(refused)?.check().map_err(refused)?;
    let id = source.root();
    if builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(TABLE, key(0, &id))) {
        return Ok(());
    }
    let h = SourceHeaderV39 {
        version: 39,
        id,
        bytes: body.len() as u32,
        body: object_id(b"misaka-palw/native/source-body/v39", &body.to_vec()),
        owner: *signer,
        registered: ctx.daa_score,
    };
    catalog.sources.push(id);
    builder.write_kernel_row(TABLE, key(0, &id), Some(bytes(&(h.root(), &h))));
    builder.write_kernel_row(TABLE, key(1, &id), Some(bytes(&body.to_vec())));
    builder.write_kernel_row(TABLE, key(5, signer), Some(bytes(&catalog)));
    Ok(())
}
fn register_class(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let wire = NativeClassWireV39::from_bytes(body).map_err(refused)?;
    let sh = source_header(builder.state.kernel_route.as_ref().unwrap(), wire.source)?;
    let mut catalog = catalog(builder.state.kernel_route.as_ref().unwrap(), signer)?;
    if catalog.classes.len() >= p.catalog_cap as usize {
        return Err(refused("source39 original owner class catalog full"));
    }
    if !prepay(builder, ctx, signer, p, sh.bytes as u64, body.len() as u64, 4096)? {
        return Ok(());
    }
    let raw: Vec<u8> = read(builder.state.kernel_route.as_ref().unwrap(), 1, &wire.source, SOURCE_UPLOAD_MAX_BYTES_V39 + 4)?;
    if raw.len() != sh.bytes as usize || object_id(b"misaka-palw/native/source-body/v39", &raw) != sh.body {
        return Err(refused("source39 class original source bytes differ"));
    }
    let source = FlatSourceWireV39::from_bytes(&raw).map_err(refused)?.check().map_err(refused)?;
    let class = wire.check(&source, p).map_err(refused)?;
    let id = class.root();
    if builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(TABLE, key(2, &id))) {
        return Ok(());
    }
    let h = ClassHeaderV39 {
        version: 39,
        id,
        source: source.root(),
        source_bytes: sh.bytes,
        class_bytes: body.len() as u32,
        record: class.record().clone(),
        owner: *signer,
        registered: ctx.daa_score,
    };
    catalog.classes.push(id);
    builder.write_kernel_row(TABLE, key(2, &id), Some(bytes(&(h.root(), &h))));
    builder.write_kernel_row(TABLE, key(3, &id), Some(bytes(&body.to_vec())));
    builder.write_kernel_row(TABLE, key(5, signer), Some(bytes(&catalog)));
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct UploadHeaderV39 {
    version: u16,
    id: Digest,
    start: NativeSourceUploadStartV39,
    owner: PalwBondKeyV2,
    operator: Digest,
    admitted: u64,
    accepted: Vec<u8>,
    attempts: Vec<u8>,
    assembly_attempts: u8,
}
impl UploadHeaderV39 {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/source-upload-progress/v39", self)
    }
}
fn upload_header(r: &PalwKernelRouteStateV1, id: Digest) -> Result<UploadHeaderV39, PalwStateV2Error> {
    let (root, h): (Digest, UploadHeaderV39) = read(r, 6, &id, 4096)?;
    if h.version != 39
        || h.id != id
        || h.start.id(h.owner) != id
        || h.root() != root
        || h.start.count == 0
        || h.start.count as usize > SOURCE_UPLOAD_MAX_PARTS_V39
        || h.attempts.len() != h.start.count as usize
        || h.accepted.len() != (h.start.count as usize).div_ceil(8)
        || h.attempts.iter().any(|n| *n > 2)
        || h.assembly_attempts > 2
    {
        return Err(refused("source39 upload original progress differs"));
    }
    Ok(h)
}
fn write_upload(builder: &mut TransitionBuilder<'_>, h: &UploadHeaderV39) {
    builder.write_kernel_row(TABLE, key(6, &h.id), Some(bytes(&(h.root(), h))));
}
fn cleanup_upload(builder: &mut TransitionBuilder<'_>, h: &UploadHeaderV39) -> Result<(), PalwStateV2Error> {
    let mut c = catalog(builder.state.kernel_route.as_ref().unwrap(), &h.owner)?;
    let index = c.uploads.iter().position(|id| *id == h.id).ok_or_else(|| refused("source39 upload original owner slot absent"))?;
    c.uploads.remove(index);
    for i in 0..h.start.count {
        builder.write_kernel_row(TABLE, key(7, &(h.id, i)), None);
    }
    builder.write_kernel_row(TABLE, key(6, &h.id), None);
    builder.write_kernel_row(TABLE, key(5, &h.owner), Some(bytes(&c)));
    Ok(())
}
fn upload_start(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let start = NativeSourceUploadStartV39::from_bytes(body).map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let until = epoch
        .interval_end(&p.execution.quota)
        .map_err(refused)?
        .min(ctx.daa_score.checked_add(p.maximum_horizon).ok_or_else(|| refused("source39 upload horizon overflow"))?);
    start.check(ctx.daa_score, until).map_err(refused)?;
    let bond = builder.state.bonds.get(signer).ok_or_else(|| refused("source39 original upload bond absent"))?;
    if bond.status != PalwBondStatusV2::Active
        || ctx.daa_score.saturating_sub(bond.registered_daa) < p.execution.execution.horizons.seal_maturity
    {
        return Err(refused("source39 original upload owner/operator immature"));
    }
    let operator = bond.operator_id.as_bytes();
    let mut c = catalog(builder.state.kernel_route.as_ref().unwrap(), signer)?;
    if c.next_upload != start.sequence || c.uploads.len() >= p.catalog_cap as usize || c.sources.len() >= p.catalog_cap as usize {
        return Err(refused("source39 original upload sequence/catalog exhausted"));
    }
    let next = c.next_upload.checked_add(1).ok_or_else(|| refused("source39 permanent upload sequence overflow"))?;
    // Reserve the WHOLE declared storage/copy bound before accepting any page.
    // Current ordinary accounting is cumulative and is never refunded on GC.
    let held = (start.bytes as u64)
        .checked_mul(6)
        .and_then(|n| n.checked_add(start.count as u64 * 2048))
        .ok_or_else(|| refused("source39 original held extent overflow"))?;
    if !prepay(builder, ctx, signer, p, 0, body.len() as u64, held)? {
        return Ok(());
    }
    let id = start.id(*signer);
    let count = start.count as usize;
    let h = UploadHeaderV39 {
        version: 39,
        id,
        start,
        owner: *signer,
        operator,
        admitted: ctx.daa_score,
        accepted: vec![0; count.div_ceil(8)],
        attempts: vec![0; count],
        assembly_attempts: 0,
    };
    c.next_upload = next;
    c.uploads.push(id);
    write_upload(builder, &h);
    builder.write_kernel_row(TABLE, key(5, signer), Some(bytes(&c)));
    Ok(())
}
fn upload_action(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let part = if action == SOURCE_PART_ACTION_V39 {
        Some(borrowed_source_upload_part_v39(body).ok_or_else(|| refused("source39 original bounded page metadata absent"))?)
    } else {
        None
    };
    let id = match part {
        Some(part) => part.id,
        None => body.try_into().map_err(|_| refused("source39 upload original id exact width"))?,
    };
    let mut h = upload_header(builder.state.kernel_route.as_ref().unwrap(), id)?;
    if action == SOURCE_COLLECT_ACTION_V39 {
        if ctx.daa_score <= h.start.deadline {
            return Err(refused("source39 original upload has not expired"));
        }
        if !prepay(builder, ctx, signer, p, 0, body.len() as u64, 4096)? {
            return Ok(());
        }
        return cleanup_upload(builder, &h);
    }
    let owner = builder.state.bonds.get(signer).ok_or_else(|| refused("source39 upload active original signer absent"))?;
    if h.owner != *signer
        || owner.status != PalwBondStatusV2::Active
        || owner.operator_id.as_bytes() != h.operator
        || ctx.daa_score < h.admitted
        || ctx.daa_score > h.start.deadline
    {
        return Err(refused("source39 upload original owner/operator/clock differs"));
    }
    if let Some(part) = part {
        if part.index >= h.start.count {
            return Err(refused("source39 page outside original count"));
        }
        let index = part.index as usize;
        let expected = (h.start.bytes as usize - index * SOURCE_PART_BYTES_V39).min(SOURCE_PART_BYTES_V39);
        if part.bytes.len() != expected || h.attempts[index] >= 2 || h.accepted[index / 8] & (1 << (index % 8)) != 0 {
            return Err(refused("source39 original paid page size/attempt differs"));
        }
        if !prepay(builder, ctx, signer, p, 0, body.len() as u64, part.bytes.len() as u64 + 2048)? {
            return Ok(());
        }
        h.attempts[index] += 1;
        write_upload(builder, &h); // persistent before copying the page
        builder.write_kernel_row(TABLE, key(7, &(id, part.index)), Some(bytes(&part.bytes.to_vec())));
        h.accepted[index / 8] |= 1 << (index % 8);
        write_upload(builder, &h);
        return Ok(());
    }
    if action != SOURCE_ASSEMBLE_ACTION_V39
        || h.assembly_attempts >= 2
        || (0..h.start.count as usize).any(|i| h.accepted[i / 8] & (1 << (i % 8)) == 0)
    {
        return Err(refused("source39 original assembly attempt/coverage unavailable"));
    }
    if !prepay(builder, ctx, signer, p, h.start.bytes as u64, body.len() as u64, h.start.bytes as u64)? {
        return Ok(());
    }
    h.assembly_attempts += 1;
    write_upload(builder, &h); // failure cannot restore this debit
    let mut raw = Vec::new();
    raw.try_reserve_exact(h.start.bytes as usize).map_err(|_| refused("source39 original assembly allocation refused"))?;
    for i in 0..h.start.count {
        let page: Vec<u8> = read(builder.state.kernel_route.as_ref().unwrap(), 7, &(id, i), SOURCE_PART_BYTES_V39 + 4)?;
        let expected = (h.start.bytes as usize - i as usize * SOURCE_PART_BYTES_V39).min(SOURCE_PART_BYTES_V39);
        if page.len() != expected {
            return Err(refused("source39 stored original page extent differs"));
        }
        raw.extend_from_slice(&page);
    }
    if raw.len() != h.start.bytes as usize || object_id(b"misaka-palw/native/source-body/v39", &raw) != h.start.body {
        return Err(refused("source39 original assembly hash differs"));
    }
    let checked = FlatSourceWireV39::from_bytes(&raw).map_err(refused)?.check().map_err(refused)?;
    if checked.root() != h.start.source {
        return Err(refused("source39 original assembly source identity differs"));
    }
    register_source(builder, ctx, signer, p, &raw)?;
    if !builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(TABLE, key(0, &h.start.source))) {
        return Err(refused("source39 paid source registration did not establish original root"));
    }
    cleanup_upload(builder, &h)
}
impl PalwKernelRouteStateV1 {
    pub fn native_source_upload_owner_v39(&self, id: Digest, index: Option<u16>, now: u64) -> Option<PalwBondKeyV2> {
        let h = upload_header(self, id).ok()?;
        if now < h.admitted || now > h.start.deadline {
            return None;
        }
        if let Some(i) = index {
            let i = i as usize;
            if i >= h.attempts.len() || h.attempts[i] >= 2 || h.accepted[i / 8] & (1 << (i % 8)) != 0 {
                return None;
            }
        } else if h.assembly_attempts >= 2 || (0..h.start.count as usize).any(|i| h.accepted[i / 8] & (1 << (i % 8)) == 0) {
            return None;
        }
        Some(h.owner)
    }
}
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let p = policy(builder, ctx)?;
    match action {
        REGISTER_SOURCE_ACTION_V39 => register_source(builder, ctx, signer, &p, body),
        REGISTER_CLASS_ACTION_V39 => register_class(builder, ctx, signer, &p, body),
        SOURCE_START_ACTION_V39 => upload_start(builder, ctx, signer, &p, body),
        SOURCE_PART_ACTION_V39 | SOURCE_ASSEMBLE_ACTION_V39 | SOURCE_COLLECT_ACTION_V39 => {
            upload_action(builder, ctx, signer, &p, action, body)
        }
        _ => Err(refused("unknown source39 registration/upload action")),
    }
}
