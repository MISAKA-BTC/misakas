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
pub(super) const HOLD_CONSENT: u8 = 0;
pub(super) const HOLD_JOB: u8 = 1;
pub(super) const HOLD_MASTER: u8 = 2;
pub(super) const HOLD_MODEL: u8 = 3;
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
    // Owning39 has separate paid catalog extents for complete models. A
    // Mistral tower/projector/decoder can exceed eight components, while only
    // eight uploads may be retained concurrently. Frozen old catalogs differ.
    if raw.len() > 2 + 12 + 8 + (64 + 64 + 8) * 64 {
        return Err(refused("source39 catalog byte bound"));
    }
    use std::io::Cursor;
    let mut reader = Cursor::new(raw.as_slice());
    let version = u16::deserialize_reader(&mut reader).map_err(|_| refused("source39 catalog version"))?;
    let mut read_ids = |cap: usize| -> Result<Vec<Digest>, PalwStateV2Error> {
        let count = u32::deserialize_reader(&mut reader).map_err(|_| refused("source39 catalog count"))? as usize;
        if count > cap || count > raw.len().saturating_sub(reader.position() as usize) / 64 {
            return Err(refused("source39 catalog original bounded prefix"));
        }
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            let id = Digest::deserialize_reader(&mut reader).map_err(|_| refused("source39 catalog id"))?;
            if id == [0; 64] || ids.contains(&id) {
                return Err(refused("source39 duplicate/zero catalog identity"));
            }
            ids.push(id);
        }
        Ok(ids)
    };
    let sources = read_ids(64)?;
    let classes = read_ids(64)?;
    let uploads = read_ids(8)?;
    drop(read_ids);
    let next_upload = u64::deserialize_reader(&mut reader).map_err(|_| refused("source39 catalog original sequence"))?;
    if version != 39 || reader.position() as usize != raw.len() {
        return Err(refused("source39 catalog canonical extent/version"));
    }
    let c = CatalogV39 { version, sources, classes, uploads, next_upload };
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
/// Read only explicitly selected, fully authenticated stream rows. This
/// reconstructs data identity and class/resource derivation, never a grant of
/// execution validity, Final, reward eligibility or consensus weight.
pub(super) fn restore_audited_class<R: crate::palw_native_audit_proof_v39::NativeAuditRowReaderV39>(
    rows: &crate::palw_native_audit_proof_v39::VerifiedNativeAuxRowsV39<R>,
    p: &NativeExecutionPoliciesV39,
    id: Digest,
    remaining: &mut usize,
    archive: Option<&dyn NativeOriginalSourceArchiveV39>,
) -> Result<CheckedNativeClassV39, String> {
    let get = |kind: u8, id: Digest, cap: usize, remaining: &mut usize| -> Result<Vec<u8>, String> {
        let raw = rows.row(TABLE, &key(kind, &id), cap.min(*remaining))?.ok_or("audit39 original source/class row absent")?;
        *remaining = remaining.checked_sub(raw.len()).ok_or("audit39 complete source restoration budget exhausted")?;
        Ok(raw)
    };
    let raw = get(2, id, 2048, remaining)?;
    let (root, h): (Digest, ClassHeaderV39) = borsh::from_slice(&raw).map_err(|_| "audit39 original class header grammar")?;
    if h.version != 39
        || h.id != id
        || h.record.root() != id
        || h.root() != root
        || h.source != h.record.source
        || h.source_bytes == 0
        || h.source_bytes as usize > SOURCE_UPLOAD_MAX_BYTES_V39
        || h.class_bytes != 262
    {
        return Err("audit39 original class header identity differs".into());
    }
    let raw = get(0, h.source, 1024, remaining)?;
    let (root, sh): (Digest, SourceHeaderV39) = borsh::from_slice(&raw).map_err(|_| "audit39 original source header grammar")?;
    if sh.version != 39 || sh.id != h.source || sh.root() != root || sh.bytes != h.source_bytes {
        return Err("audit39 original source header identity differs".into());
    }
    let raw = if rows.contains(TABLE, &key(1, &h.source)) {
        let raw = get(1, h.source, SOURCE_UPLOAD_MAX_BYTES_V39 + 4, remaining)?;
        if raw.len() < 4 || u32::from_le_bytes(raw[..4].try_into().unwrap()) != sh.bytes || raw.len() != sh.bytes as usize + 4 {
            return Err("audit39 original source body exact extent differs".into());
        }
        borsh::from_slice::<Vec<u8>>(&raw).map_err(|_| "audit39 original source body grammar")?
    } else {
        let raw = archive
            .ok_or("audit39 collected original source requires its immutable body archive")?
            .source(h.source, (sh.bytes as usize).min(*remaining))?
            .ok_or("audit39 original source archive body absent")?;
        *remaining = remaining.checked_sub(raw.len()).ok_or("audit39 original source archive reservation exhausted")?;
        raw
    };
    if raw.len() != sh.bytes as usize || object_id(b"misaka-palw/native/source-body/v39", &raw) != sh.body {
        return Err("audit39 original source body hash differs".into());
    }
    let source = FlatSourceWireV39::from_bytes(&raw)?.check()?;
    drop(raw);
    if source.root() != h.source {
        return Err("audit39 original source canonical identity differs".into());
    }
    let raw = if rows.contains(TABLE, &key(3, &id)) {
        let raw = get(3, id, 266, remaining)?;
        if raw.len() != h.class_bytes as usize + 4 || u32::from_le_bytes(raw[..4].try_into().unwrap()) != h.class_bytes {
            return Err("audit39 original class body exact extent differs".into());
        }
        borsh::from_slice::<Vec<u8>>(&raw).map_err(|_| "audit39 original class body grammar")?
    } else {
        let raw = archive
            .ok_or("audit39 collected original class requires its immutable body archive")?
            .class(id, (h.class_bytes as usize).min(*remaining))?
            .ok_or("audit39 original class archive body absent")?;
        *remaining = remaining.checked_sub(raw.len()).ok_or("audit39 original class archive reservation exhausted")?;
        raw
    };
    if raw.len() != h.class_bytes as usize {
        return Err("audit39 archived original class exact extent differs".into());
    }
    let class = NativeClassWireV39::from_bytes(&raw)?.check(&source, p)?;
    if class.root() != h.id || class.record() != &h.record {
        return Err("audit39 original complete class/resource derivation differs".into());
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
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(TABLE, key(1, &id))) {
        return Ok(());
    }
    if r.aux.contains_key(&(TABLE, key(0, &id))) {
        let h = source_header(r, id)?;
        if h.bytes as usize != body.len() || h.body != object_id(b"misaka-palw/native/source-body/v39", &body) {
            return Err(refused("source39 rehydration changes immutable original bytes"));
        }
        catalog.sources.push(id);
        builder.write_kernel_row(TABLE, key(1, &id), Some(bytes(&body)))?;
        builder.write_kernel_row(TABLE, key(96, &id), Some(bytes(signer)))?;
        builder.write_kernel_row(TABLE, key(5, signer), Some(bytes(&catalog)))?;
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
    builder.write_kernel_row(TABLE, key(0, &id), Some(bytes(&(h.root(), &h))))?;
    builder.write_kernel_row(TABLE, key(1, &id), Some(bytes(&body.to_vec())))?;
    builder.write_kernel_row(TABLE, key(96, &id), Some(bytes(signer)))?;
    builder.write_kernel_row(TABLE, key(5, signer), Some(bytes(&catalog)))?;
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
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(TABLE, key(3, &id))) {
        return Ok(());
    }
    let old = if r.aux.contains_key(&(TABLE, key(2, &id))) {
        let old = class_header(r, id)?;
        if old.record != *class.record()
            || old.source != source.root()
            || old.source_bytes != sh.bytes
            || old.class_bytes as usize != body.len()
        {
            return Err(refused("source39 class rehydration changes immutable derivation"));
        }
        Some(old)
    } else {
        None
    };
    let source_references =
        count(r, 98, source.root())?.checked_add(1).ok_or_else(|| refused("source39 active class count overflow"))?;
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
    if old.is_none() {
        builder.write_kernel_row(TABLE, key(2, &id), Some(bytes(&(h.root(), &h))))?;
    }
    builder.write_kernel_row(TABLE, key(3, &id), Some(bytes(&body.to_vec())))?;
    builder.write_kernel_row(TABLE, key(97, &id), Some(bytes(signer)))?;
    builder.write_kernel_row(TABLE, key(98, &source.root()), Some(bytes(&source_references)))?;
    builder.write_kernel_row(TABLE, key(5, signer), Some(bytes(&catalog)))?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ClassHold {
    version: u16,
    class: Digest,
    kind: u8,
    object: Digest,
    until: u64,
}
impl ClassHold {
    fn id(&self) -> Digest {
        class_hold_id(self.kind, self.object)
    }
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-class-retention/v39", self)
    }
}
pub(super) fn class_hold_id(kind: u8, object: Digest) -> Digest {
    object_id(b"misaka-palw/native/original-class-holder/v39", &(kind, object))
}
pub(super) fn master_hold_object(master: Digest, stage: u16) -> Digest {
    object_id(b"misaka-palw/native/original-Master-class-holder/v39", &(master, stage))
}
fn count(r: &PalwKernelRouteStateV1, kind: u8, id: Digest) -> Result<u64, PalwStateV2Error> {
    let Some(raw) = r.aux.get(&(TABLE, key(kind, &id))) else { return Ok(0) };
    if raw.len() != 8 {
        return Err(refused("source39 exact retention counter grammar"));
    }
    Ok(u64::from_le_bytes(raw.as_slice().try_into().unwrap()))
}
/// Called only inside the real consent/Fund/Master/model transition, after its
/// original metadata debit. No SDK-supplied reference count is accepted.
pub(super) fn retain_class(
    builder: &mut TransitionBuilder<'_>,
    class: Digest,
    kind: u8,
    object: Digest,
    until: u64,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    class_header(r, class)?;
    let hold = ClassHold { version: 39, class, kind, object, until };
    if kind > HOLD_MODEL
        || object == [0; 64]
        || until == 0
        || !r.aux.contains_key(&(TABLE, key(3, &class)))
        || r.aux.contains_key(&(TABLE, key(94, &hold.id())))
    {
        return Err(refused("source39 original class hold absent/replayed/nonlive"));
    }
    let n = count(r, 95, class)?.checked_add(1).ok_or_else(|| refused("source39 original class reference overflow"))?;
    builder.write_kernel_row(TABLE, key(94, &hold.id()), Some(bytes(&(hold.root(), &hold))))?;
    builder.write_kernel_row(TABLE, key(95, &class), Some(bytes(&n)))?;
    Ok(())
}
pub(super) fn release_class(
    builder: &mut TransitionBuilder<'_>,
    class: Digest,
    kind: u8,
    object: Digest,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let id = class_hold_id(kind, object);
    let (root, hold): (Digest, ClassHold) = read(r, 94, &id, 256)?;
    if hold.version != 39 || hold.class != class || hold.kind != kind || hold.object != object || hold.root() != root {
        return Err(refused("source39 original class release reference differs"));
    }
    let n = count(r, 95, class)?.checked_sub(1).ok_or_else(|| refused("source39 original class reference underflow"))?;
    builder.write_kernel_row(TABLE, key(94, &id), None)?;
    builder.write_kernel_row(TABLE, key(95, &class), Some(bytes(&n)))?;
    Ok(())
}
fn retention_action(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    action: u8,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if action == 72 {
        let id: Digest = raw.try_into().map_err(|_| refused("source39 expired hold exact id"))?;
        let (root, hold): (Digest, ClassHold) = read(builder.state.kernel_route.as_ref().unwrap(), 94, &id, 256)?;
        if hold.version != 39 || hold.id() != id || hold.root() != root || hold.kind != HOLD_CONSENT || ctx.daa_score <= hold.until {
            return Err(refused("source39 only actually expired producer consent holds may be reaped"));
        }
        if !prepay(builder, ctx, signer, p, 0, raw.len() as u64, 0)? {
            return Ok(());
        }
        return release_class(builder, hold.class, hold.kind, hold.object);
    }
    if raw.len() != 67 || raw[..2] != 39u16.to_le_bytes() || raw[2] > 1 {
        return Err(refused("source39 release exact kind/id grammar"));
    }
    let class = raw[2] == 1;
    let id: Digest = raw[3..].try_into().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let (owner, extent, source) = if class {
        let h = class_header(r, id)?;
        if count(r, 95, id)? != 0 || !r.aux.contains_key(&(TABLE, key(3, &id))) {
            return Err(refused("source39 live consent/job/Master/model still retains this class"));
        }
        let owner: PalwBondKeyV2 = read(r, 97, &id, 128)?;
        (owner, h.class_bytes as u64, Some(h.source))
    } else {
        let h = source_header(r, id)?;
        if count(r, 98, id)? != 0 || !r.aux.contains_key(&(TABLE, key(1, &id))) {
            return Err(refused("source39 an active class still retains this source"));
        }
        let owner: PalwBondKeyV2 = read(r, 96, &id, 128)?;
        (owner, h.bytes as u64, None)
    };
    if owner != *signer {
        return Err(refused("source39 release is owned by another retention payer"));
    }
    if !prepay(builder, ctx, signer, p, extent, raw.len() as u64, 0)? {
        return Ok(());
    }
    let mut c = catalog(builder.state.kernel_route.as_ref().unwrap(), signer)?;
    let ids = if class { &mut c.classes } else { &mut c.sources };
    let index = ids.iter().position(|v| *v == id).ok_or_else(|| refused("source39 original active catalog entry absent"))?;
    ids.remove(index);
    if let Some(source) = source {
        let n = count(builder.state.kernel_route.as_ref().unwrap(), 98, source)?
            .checked_sub(1)
            .ok_or_else(|| refused("source39 original active source count underflow"))?;
        builder.write_kernel_row(TABLE, key(98, &source), Some(bytes(&n)))?;
    }
    builder.write_kernel_row(TABLE, key(if class { 3 } else { 1 }, &id), None)?;
    builder.write_kernel_row(TABLE, key(if class { 97 } else { 96 }, &id), None)?;
    builder.write_kernel_row(TABLE, key(5, signer), Some(bytes(&c)))?;
    // Immutable headers survive. Rehydration must reproduce their exact bytes
    // and original class derivation; freeing a slot never adds Q/B/R/F rights.
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
        builder.write_kernel_row(TABLE, key(7, &(h.id, i)), None)?;
    }
    builder.write_kernel_row(TABLE, key(6, &h.id), None)?;
    builder.write_kernel_row(TABLE, key(5, &h.owner), Some(bytes(&c)))?;
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
    if c.next_upload != start.sequence || c.uploads.len() >= 8 || c.sources.len() >= p.catalog_cap as usize {
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
    builder.write_kernel_row(TABLE, key(5, signer), Some(bytes(&c)))?;
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
        builder.write_kernel_row(TABLE, key(7, &(id, part.index)), Some(bytes(&part.bytes.to_vec())))?;
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
    if !builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(TABLE, key(1, &h.start.source))) {
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
        71 | 72 => retention_action(builder, ctx, signer, &p, action, body),
        SOURCE_START_ACTION_V39 => upload_start(builder, ctx, signer, &p, body),
        SOURCE_PART_ACTION_V39 | SOURCE_ASSEMBLE_ACTION_V39 | SOURCE_COLLECT_ACTION_V39 => {
            upload_action(builder, ctx, signer, &p, action, body)
        }
        _ => Err(refused("unknown source39 registration/upload action")),
    }
}
