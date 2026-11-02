//! ISOLATED state_v2 child candidate; not registered or activated.
//! Persistent checkpoint R/I/L decomposition; exact released logical bytes.
//! Stage2/private gated; actual source admission authority remains unchanged.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
#[path="palw_claim_local_checkpoint_owner_local_v3.rs"]
mod owner_local;
pub(super) use owner_local::PreparedBasesV3;
pub(super) fn prepare_bases_v3(records:&BTreeMap<Work,Record>)->Result<owner_local::PreparedBasesV3,PalwStateV2Error>{owner_local::prepare(records)}
pub(super) fn validate_actual_bases_v3(s:&PalwChainStateV2,actual:&BTreeMap<Work,Record>)->Result<(),PalwStateV2Error>{owner_local::validate_actual_bases(s,actual)}
pub(super) fn try_material_checkpoint_v3(b:&mut TransitionBuilder<'_>,parent:&PalwChainStateV2,ctx:&PalwBlockContextV2,objects:&[PalwConsensusObjectV2],work:PalwBlockWorkV3<'_>,merged:&[PalwMergedWorkV1<'_>])->Result<bool,PalwStateV2Error>{owner_local::try_material(b,parent,ctx,objects,work,merged)}
pub(super) fn write_prepared_v3(b:&mut TransitionBuilder<'_>,cp:&Checkpoint,bases:&owner_local::PreparedBasesV3)->Result<(),PalwStateV2Error>{
 if !b.params.claim_local_settlement_active_at(cp.point.daa_score){return super::palw_claim_local_descriptor_v2::write(b,cp)}
 atomic(b,|b|{write_inner(b,cp)?;owner_local::publish_full(b,cp,bases).map_err(native_err)?;Ok(())}).map_err(err)
}

use crate::palw_claim_local_context_nodes_v2::{
    self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2,
};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_claim_local_v1::{
    PalwClaimLocalCheckpointV1 as Checkpoint, PalwClaimLocalRecordV1 as Record,
    PalwClaimLocalWorkRefV1 as Work, palw_liability_material_id_v1 as material_id,
};
use crate::palw_native_resource_v1::{
    NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
const RECORDS: u32 = 1;
const LITERALS: u32 = 2;
const NATIVE: u32 = 3;
const COORD_REFS: u32 = 4;
const INPUTS: u32 = 5;
const RESERVATIONS: u32 = 6;
const MATERIALS: u32 = 7;
const COORDS: u32 = 8;
const LIABILITY: u32 = 9;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 {
        owner: 12,
        table,
        schema: 3,
        key_bytes: match table {
            RECORDS | INPUTS => 133,
            COORDS => 130,
            _ => 64,
        },
        value_bytes: u64::MAX,
    }
}
fn bad(s: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(s)
}
fn err(e: ContextErrorV2) -> PalwStateV2Error {
    super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn native_err(e: PalwStateV2Error) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(e) => ContextErrorV2::Resource(e),
        _ => bad("contract generation journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("contract generation encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64)
        .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out)
        .map_err(|_| bad("contract generation encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(
    raw: &[u8],
) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || {
        borsh::from_slice(raw)
    })
    .map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => {
            ContextErrorV2::Resource(r)
        }
        _ => bad("contract generation typed body codec"),
    })
}
fn hash<T: BorshSerialize>(domain: u8, v: &T) -> Result<Hash64, ContextErrorV2> {
    // Distinct typed domain is part of the streamed identity, not an allocation.
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, x: &[u8]) -> std::io::Result<usize> {
            self.0.update(x);
            Ok(x.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut w = Writer(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/checkpoint-persistent/v3")
            .to_state(),
    );
    domain
        .serialize(&mut w)
        .map_err(|_| bad("lineage domain"))?;
    v.serialize(&mut w).map_err(|_| bad("lineage identity"))?;
    let mut bytes = [0; 64];
    bytes.copy_from_slice(w.0.finalize().as_bytes());
    Ok(Hash64::from_bytes(bytes))
}
fn value(raw: &[u8]) -> RowValueV2 {
    RowValueV2 {
        data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw),
        bytes: raw.len() as u64,
    }
}
fn root(
    s: &PalwChainStateV2,
    table: u32,
    scope: Option<Hash64>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let r = StateNodesV2(s);
    match scope {
        Some(id) => r.owned_current(id, spec(table)),
        None => r.current(spec(table)),
    }
}
fn lookup<T: BorshDeserialize + NativeWireScanV1>(
    s: &PalwChainStateV2,
    table: u32,
    scope: Option<Hash64>,
    key: &[u8],
) -> Result<Option<NativeRetainedV1<T>>, ContextErrorV2> {
    let r = StateNodesV2(s);
    nodes::lookup(&r, root(s, table, scope)?, key)?
        .map(|v| {
            let raw = body(s, v)?;
            decode(raw)
        })
        .transpose()
}
fn put_raw(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Option<Hash64>,
    key: &[u8],
    raw: Option<&[u8]>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let r = StateNodesV2(&b.state);
    let before = root(&b.state, table, scope)?;
    let old = nodes::lookup(&r, before, key)?;
    let next = raw.map(value);
    if old == next {
        if let Some(raw) = raw {
            let mut bodykey = [0; 65];
            bodykey[0] = b'B';
            bodykey[1..].copy_from_slice(material_id(raw).as_byte_slice());
            let _lease = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
            let stored = b
                .state
                .kernel_route
                .as_ref()
                .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, bodykey.to_vec())))
                .ok_or(ContextErrorV2::Unavailable(
                    "checkpoint unchanged body missing",
                ))?;
            crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&raw, stored)
                .map_err(|_| bad("checkpoint unchanged body differs"))?;
        }
        return Ok(before);
    }
    let plan = nodes::update(&r, before, key, old, next)?;
    match scope {
        Some(scope) => store::publish_owned_component_v2(b, scope, &plan, raw)?,
        None => store::publish_component_v2(b, &plan, raw)?,
    };
    Ok(plan.after())
}
fn put<T: BorshSerialize>(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Option<Hash64>,
    key: &[u8],
    row: Option<&T>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let raw = row.map(encode).transpose()?;
    put_raw(b, table, scope, key, raw.as_deref().map(Vec::as_slice))
}
fn atomic<T>(
    b: &mut TransitionBuilder<'_>,
    f: impl FnOnce(&mut TransitionBuilder<'_>) -> Result<T, ContextErrorV2>,
) -> Result<T, ContextErrorV2> {
    let first=b.entries.len();let gc_point=b.claim_local_gc_sequence_v3;let answer=f(b);
    if answer.is_err(){super::palw_claim_local_context_gc_v3::rollback_aux_v3(b,first,gc_point,|table,key|table==PALW_CLAIM_LOCAL_TABLE_V1&&matches!(key.first().copied(),Some(b'N'|b'B'|b'I'|b'A'|b'D'|b'C'|b'V'|b'Q'|b'M')))?;}
    answer
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct DescriptorV3 {
    pub version: u16,
    pub point: PalwBlockContextV2,
    pub records: ComponentRootV2,
    pub literals: ComponentRootV2,
    pub native: ComponentRootV2,
    pub coordinates: ComponentRootV2,
    pub liability: ComponentRootV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(DescriptorV3;u16,PalwBlockContextV2,ComponentRootV2,ComponentRootV2,ComponentRootV2,ComponentRootV2,ComponentRootV2);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ScalarV3 {
    version: u16,
    work: Work,
    scope: Hash64,
    accepted_daa: u64,
    accepted_blue_score: u64,
    accepting_block: Hash64,
    anchors: bool,
    execution_root: Hash64,
    final_claim: bool,
    certified: bool,
    voided: bool,
    liability_closed: bool,
    inputs: ComponentRootV2,
    reservations: ComponentRootV2,
    material: ComponentRootV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(ScalarV3;u16,Work,Hash64,u64,u64,Hash64,bool,Hash64,bool,bool,bool,bool,ComponentRootV2,ComponentRootV2,ComponentRootV2);
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct NativeRefV3 {
    version: u16,
    id: Hash64,
    len: u64,
    table: u8,
    key: Vec<u8>,
}
crate::palw_native_resource_v1::wire::wire_struct!(NativeRefV3;u16,Hash64,u64,u8,Vec<u8>);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct CoordinateV3 {
    id: Hash64,
    len: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(CoordinateV3;Hash64,u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct CoordinateRefV3 {
    version: u16,
    claim: Hash64,
    scope: Hash64,
    root: ComponentRootV2,
    id: Hash64,
    len: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(CoordinateRefV3;u16,Hash64,Hash64,ComponentRootV2,Hash64,u64);
// Source-owned logical L decomposition. No complete L body is persisted.
const L_OPEN: u32 = 10;
const L_CLAIMS: u32 = 11;
const L_COURTS: u32 = 12;
const L_KERNEL: u32 = 13;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct LiabilityMetaV3 {
    version: u16,
    open: ComponentRootV2,
    kernels: ComponentRootV2,
    id: Hash64,
    len: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(LiabilityMetaV3;u16,ComponentRootV2,ComponentRootV2,Hash64,u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct LiabilityOpenV3 {
    version: u16,
    claim: Hash64,
    scope: Hash64,
    body: ComponentRootV2,
    courts: ComponentRootV2,
    birth: Option<Hash64>,
}
crate::palw_native_resource_v1::wire::wire_struct!(LiabilityOpenV3;u16,Hash64,Hash64,ComponentRootV2,ComponentRootV2,Option<Hash64>);
fn liability_meta(
    s: &PalwChainStateV2,
    r: ComponentRootV2,
) -> Result<LiabilityMetaV3, ContextErrorV2> {
    if r.rows != 1 {
        return Err(bad("liability meta singleton"));
    }
    let v = nodes::lookup(&StateNodesV2(s), r, Hash64::default().as_byte_slice())?
        .ok_or(bad("liability meta absent"))?;
    let m = decode::<LiabilityMetaV3>(body(s, v)?)?;
    if m.version != 3 || m.open.spec != spec(L_OPEN) || m.kernels.spec != spec(L_KERNEL) {
        return Err(bad("liability meta profile"));
    }
    Ok(*m)
}
fn liability_open(
    s: &PalwChainStateV2,
    key: &[u8],
    v: RowValueV2,
) -> Result<LiabilityOpenV3, ContextErrorV2> {
    let r = decode::<LiabilityOpenV3>(body(s, v)?)?;
    if r.version != 3
        || key != r.claim.as_byte_slice()
        || r.courts.spec != spec(L_COURTS)
        || r.body.spec != spec(L_CLAIMS)
        || r.body.rows != 1
    {
        return Err(bad("liability open profile"));
    }
    Ok(*r)
}
fn emit_liability_v3<W: std::io::Write>(
    s: &PalwChainStateV2,
    m: &LiabilityMetaV3,
    out: &mut W,
) -> Result<(), ContextErrorV2> {
    let reader = StateNodesV2(s);
    let emit = |out: &mut W, b: &[u8]| out.write_all(b).map_err(|_| bad("liability stream write"));
    if m.open.rows != 0 {
        let n = u32::try_from(m.open.rows).map_err(|_| bad("liability chain count width"))?;
        emit(out, &n.to_le_bytes())?;
        let mut births = 0u32;
        nodes::complete_rows(&reader, m.open, |key, v| {
            let r = liability_open(s, key, v)?;
            let cv = nodes::lookup(&reader, r.body, key)?
                .ok_or(bad("liability claim body membership"))?;
            let raw = body(s, cv)?;
            let c = decode::<PalwClaimStateV2>(raw)?;
            if scope(
                Work::ChainClaim { claim: r.claim },
                c.accepted_block,
                c.accepted_daa,
                c.accepted_blue_score,
            )? != r.scope
            {
                return Err(bad("liability exact source birth scope"));
            }
            emit(out, key)?;
            emit(out, raw)?;
            let count =
                u32::try_from(r.courts.rows).map_err(|_| bad("liability court count width"))?;
            emit(out, &count.to_le_bytes())?;
            nodes::complete_rows(&reader, r.courts, |sid, value| {
                let raw = body(s, value)?;
                let pair = decode::<(Hash64, PalwCourtSessionStateV2)>(raw)?;
                if sid != pair.0.as_byte_slice() || pair.1.claim != r.claim {
                    return Err(bad("liability court owner identity"));
                }
                emit(out, raw)
            })?;
            if r.birth.is_some() {
                births = births.checked_add(1).ok_or(bad("liability birth count"))?
            }
            Ok(())
        })?;
        emit(out, &births.to_le_bytes())?;
        nodes::complete_rows(&reader, m.open, |key, v| {
            let r = liability_open(s, key, v)?;
            if let Some(b) = r.birth {
                emit(out, b.as_byte_slice())?
            }
            Ok(())
        })?;
    }
    if m.kernels.rows != 0 {
        let count =
            u32::try_from(m.kernels.rows).map_err(|_| bad("liability kernel count width"))?;
        emit(out, &count.to_le_bytes())?;
        nodes::complete_rows(&reader, m.kernels, |key, v| {
            if key.len() != 64 || body(s, v)? != [1] {
                return Err(bad("liability kernel member"));
            }
            emit(out, key)
        })?;
    }
    Ok(())
}
fn check_liability_image(s: &PalwChainStateV2, m: &LiabilityMetaV3) -> Result<(), ContextErrorV2> {
    let mut w = CanonicalWriterV3::new();
    emit_liability_v3(s, m, &mut w)?;
    if w.finish() != (m.id, m.len) {
        return Err(bad("liability complete logical image"));
    }
    Ok(())
}
fn write_liability_v3(
    b: &mut TransitionBuilder<'_>,
    checkpoint: &Checkpoint,
) -> Result<(), ContextErrorV2> {
    // Scratch belongs to this global checkpoint construction, not a source fee.
    let frame_count = super::palw_claim_local_rewards_v1::frames(&b.state).count() as u64;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(u64, u64), Hash64>>(
        frame_count,
    )
    .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(Hash64, Hash64), ()>>(
        b.state.court_sessions.len() as u64,
    )
    .map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let court_index: BTreeSet<(Hash64, Hash64)> = b
        .state
        .court_sessions
        .iter()
        .map(|(id, c)| (c.claim, *id))
        .collect();
    let mut frames = BTreeMap::new();
    for (_, raw) in super::palw_claim_local_rewards_v1::frames(&b.state) {
        let p = PalwBlockContextV2::deserialize_reader(&mut raw.as_slice())
            .map_err(|_| bad("liability actual frame point"))?;
        frames.insert((p.daa_score, p.blue_score), p.block);
    }
    let old = keys(&b.state, L_OPEN, None)?;
    for key in old.iter() {
        let row = lookup::<LiabilityOpenV3>(&b.state, L_OPEN, None, key)?
            .ok_or(bad("liability old open absent"))?;
        let current = checkpoint
            .records
            .get(&Work::ChainClaim { claim: row.claim });
        let current_scope = current.map(scalar_scope).transpose()?;
        let keep = current.is_some_and(|r| !r.liability_closed) && current_scope == Some(row.scope);
        if !keep {
            drop_owned(b, L_COURTS, row.scope)?;
            put_raw(b, L_OPEN, None, key, None)?;
            drop_owned(b, L_CLAIMS, row.scope)?;
        }
    }
    for record in checkpoint.records.values() {
        let Work::ChainClaim { claim } = record.work else {
            continue;
        };
        if record.liability_closed {
            continue;
        }
        let c = b
            .state
            .claims
            .get(&claim)
            .ok_or(bad("liability actual claim missing"))?;
        if (c.accepted_daa, c.accepted_blue_score, c.accepted_block)
            != (
                record.accepted_daa,
                record.accepted_blue_score,
                record.accepting_block,
            )
        {
            return Err(bad("liability actual claim birth differs"));
        }
        let raw = encode(c)?;
        let id = material_id(&raw);
        if !record.material.contains(&id)
            || checkpoint.material.get(&id).map(Vec::as_slice) != Some(raw.as_slice())
        {
            return Err(bad("liability claim actual material binding"));
        }
        let sc = scalar_scope(record)?;
        put_raw(b, L_CLAIMS, Some(sc), claim.as_byte_slice(), Some(&raw))?;
        let old_courts = keys(&b.state, L_COURTS, Some(sc))?;
        for key in old_courts.iter() {
            let sid: Hash64 = borsh::from_slice(key).map_err(|_| bad("liability old court key"))?;
            if b.state
                .court_sessions
                .get(&sid)
                .is_none_or(|c| c.claim != claim)
            {
                put_raw(b, L_COURTS, Some(sc), key, None)?;
            }
        }
        for (_, sid) in
            court_index.range((claim, Hash64::default())..=(claim, Hash64::from_bytes([255; 64])))
        {
            let c = b
                .state
                .court_sessions
                .get(sid)
                .ok_or(bad("liability court vanished"))?;
            let raw = encode(&(*sid, c))?;
            let id = material_id(&raw);
            if !record.material.contains(&id)
                || checkpoint.material.get(&id).map(Vec::as_slice) != Some(raw.as_slice())
            {
                return Err(bad("liability court actual material binding"));
            }
            put_raw(b, L_COURTS, Some(sc), sid.as_byte_slice(), Some(&raw))?;
        }
        let row = LiabilityOpenV3 {
            version: 3,
            claim,
            scope: sc,
            body: root(&b.state, L_CLAIMS, Some(sc))?,
            courts: root(&b.state, L_COURTS, Some(sc))?,
            birth: frames
                .get(&(record.accepted_daa, record.accepted_blue_score))
                .copied(),
        };
        put(b, L_OPEN, None, claim.as_byte_slice(), Some(&row))?;
    }
    let old = keys(&b.state, L_KERNEL, None)?;
    for key in old.iter() {
        let id: Hash64 = borsh::from_slice(key).map_err(|_| bad("liability kernel old key"))?;
        if checkpoint
            .records
            .get(&Work::KernelClaim { claim: id })
            .is_none_or(|r| r.liability_closed)
        {
            put_raw(b, L_KERNEL, None, key, None)?;
        }
    }
    for r in checkpoint.records.values() {
        if let Work::KernelClaim { claim } = r.work {
            if !r.liability_closed {
                put_raw(b, L_KERNEL, None, claim.as_byte_slice(), Some(&[1]))?;
            }
        }
    }
    let m = LiabilityMetaV3 {
        version: 3,
        open: root(&b.state, L_OPEN, None)?,
        kernels: root(&b.state, L_KERNEL, None)?,
        id: material_id(&checkpoint.liability_state),
        len: checkpoint.liability_state.len() as u64,
    };
    check_liability_image(&b.state, &m)?;
    put(
        b,
        LIABILITY,
        None,
        Hash64::default().as_byte_slice(),
        Some(&m),
    )?;
    Ok(())
}

fn dkey() -> [u8; 65] {
    super::palw_claim_local_descriptor_v2::palw_claim_descriptor_key_v2()
}
fn descriptor(s: &PalwChainStateV2) -> Result<Option<DescriptorV3>, ContextErrorV2> {
    let _lease = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
    let Some(raw) = s
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, dkey().to_vec())))
    else {
        return Ok(None);
    };
    match raw.get(..2) {
        Some([2, 0]) => Ok(None),
        Some([3, 0]) => decode::<DescriptorV3>(raw).map(|r| Some(*r)),
        _ => Err(bad("checkpoint descriptor version")),
    }
}
fn work_key(work: Work) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    encode(&work)
}
fn scope(work: Work, block: Hash64, daa: u64, blue: u64) -> Result<Hash64, ContextErrorV2> {
    hash(1, &(work, block, daa, blue))
}
fn scalar_scope(r: &Record) -> Result<Hash64, ContextErrorV2> {
    scope(
        r.work,
        r.accepting_block,
        r.accepted_daa,
        r.accepted_blue_score,
    )
}
fn row_scalar(
    r: &Record,
    scope: Hash64,
    inputs: ComponentRootV2,
    reservations: ComponentRootV2,
    material: ComponentRootV2,
) -> ScalarV3 {
    ScalarV3 {
        version: 3,
        work: r.work,
        scope,
        accepted_daa: r.accepted_daa,
        accepted_blue_score: r.accepted_blue_score,
        accepting_block: r.accepting_block,
        anchors: r.anchors,
        execution_root: r.execution_root,
        final_claim: r.final_claim,
        certified: r.certified,
        voided: r.voided,
        liability_closed: r.liability_closed,
        inputs,
        reservations,
        material,
    }
}
fn checked_scalar(s: &ScalarV3, key: &[u8]) -> Result<(), ContextErrorV2> {
    if s.version != 3
        || work_key(s.work)?.as_slice() != key
        || s.scope
            != scope(
                s.work,
                s.accepting_block,
                s.accepted_daa,
                s.accepted_blue_score,
            )?
        || s.inputs.spec != spec(INPUTS)
        || s.reservations.spec != spec(RESERVATIONS)
        || s.material.spec != spec(MATERIALS)
    {
        return Err(bad("checkpoint fixed record/scoped set profile"));
    }
    Ok(())
}
fn keys(
    s: &PalwChainStateV2,
    table: u32,
    owned: Option<Hash64>,
) -> Result<NativeRetainedV1<Vec<Vec<u8>>>, ContextErrorV2> {
    let r = root(s, table, owned)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Vec<u8>>(r.rows)
        .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<u8>(
        r.rows
            .checked_mul(u64::from(r.spec.key_bytes))
            .ok_or(bad("checkpoint key count overflow"))?,
    )
    .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out =
        Vec::with_capacity(usize::try_from(r.rows).map_err(|_| bad("checkpoint key capacity"))?);
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, r, |key, _| {
        out.push(key.to_vec());
        Ok(())
    })?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn sync_set<T: BorshSerialize + BorshDeserialize + Ord>(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Hash64,
    want: &BTreeSet<T>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let old = keys(&b.state, table, Some(scope))?;
    for key in old.iter() {
        let t: T = borsh::from_slice(key).map_err(|_| bad("checkpoint set key codec"))?;
        if !want.contains(&t) {
            put_raw(b, table, Some(scope), key, None)?;
        }
    }
    for t in want {
        let key = encode(t)?;
        put_raw(b, table, Some(scope), &key, Some(&[1]))?;
    }
    root(&b.state, table, Some(scope))
}
fn drop_owned(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Hash64,
) -> Result<(), ContextErrorV2> {
    let mut key = [0u8; 84];
    key[0] = b'I';
    key[1..3].copy_from_slice(&2u16.to_le_bytes());
    key[3..67].copy_from_slice(scope.as_byte_slice());
    spec(table)
        .serialize(&mut &mut key[67..])
        .map_err(|_| bad("checkpoint owned pointer codec"))?;
    let _lease = reserve_native_requests_v1(84).map_err(ContextErrorV2::Resource)?;
    b.write_kernel_row(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec(), None)
        .map_err(native_err)
}
fn staged_owner(key: &crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1) -> Option<Hash64> {
    use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 as K;
    match key {
        K::CourtSharedIds(c) | K::CourtMaterial(c, _) => Some(*c),
        _ => None,
    }
}
struct CanonicalWriterV3 {
    hash: blake2b_simd::State,
    len: u64,
}
impl CanonicalWriterV3 {
    fn new() -> Self {
        Self {
            hash: blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/liability-material/v1")
                .to_state(),
            len: 0,
        }
    }
    fn finish(self) -> (Hash64, u64) {
        let mut h = [0; 64];
        h.copy_from_slice(self.hash.finalize().as_bytes());
        (Hash64::from_bytes(h), self.len)
    }
}
impl std::io::Write for CanonicalWriterV3 {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.len = self
            .len
            .checked_add(b.len() as u64)
            .ok_or_else(|| std::io::Error::other("canonical coordinate length overflow"))?;
        self.hash.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn actual_coordinates(
    s: &PalwChainStateV2,
    claim: Hash64,
    coordinates: &[super::palw_claim_local_material_projection_v2::Coordinate<'_>],
) -> Result<NativeRetainedV1<BTreeMap<Vec<u8>, CoordinateV3>>, ContextErrorV2> {
    let n = coordinates.iter().filter(|c| c.claim == claim).count()
        + s.staged_courts
            .keys()
            .filter(|k| staged_owner(k) == Some(claim))
            .count();
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Vec<u8>, CoordinateV3>>(
        n as u64,
    )
    .map_err(ContextErrorV2::Resource)?;
    let mut key_bytes =
        coordinates
            .iter()
            .filter(|c| c.claim == claim)
            .try_fold(0u64, |n, c| {
                n.checked_add(1 + c.key.len() as u64)
                    .ok_or(bad("actual coordinate key count"))
            })?;
    for key in s
        .staged_courts
        .keys()
        .filter(|k| staged_owner(k) == Some(claim))
    {
        key_bytes = key_bytes
            .checked_add(
                1 + borsh::object_length(key).map_err(|_| bad("staged coordinate key length"))?
                    as u64,
            )
            .ok_or(bad("staged coordinate key sum"))?;
    }
    q.exact_v1::<u8>(
        key_bytes
            .checked_mul(2)
            .ok_or(bad("coordinate key workspace overflow"))?,
    )
    .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut map = BTreeMap::new();
    for c in coordinates.iter().filter(|c| c.claim == claim) {
        let mut key = Vec::with_capacity(1 + c.key.len());
        key.push(c.table);
        key.extend_from_slice(c.key);
        let (id, len) = super::palw_claim_local_material_projection_v2::coordinate_identity_v3(c);
        map.insert(key, CoordinateV3 { id, len });
    }
    for (key, row) in &s.staged_courts {
        if staged_owner(key) == Some(claim) {
            let mut k = Vec::with_capacity(
                1 + borsh::object_length(key).map_err(|_| bad("coordinate staged key length"))?,
            );
            k.push(255);
            key.serialize(&mut k)
                .map_err(|_| bad("coordinate staged key encode"))?;
            let mut w = CanonicalWriterV3::new();
            (key, row)
                .serialize(&mut w)
                .map_err(|_| bad("coordinate staged body hash"))?;
            let (id, len) = w.finish();
            map.insert(k, CoordinateV3 { id, len });
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(map, lease))
}
fn write_index<W: std::io::Write>(
    claim: Hash64,
    count: u64,
    mut visit: impl FnMut(&mut W) -> Result<(), ContextErrorV2>,
    out: &mut W,
) -> Result<(), ContextErrorV2> {
    2u16.serialize(out)
        .map_err(|_| bad("coordinate logical version"))?;
    claim
        .serialize(out)
        .map_err(|_| bad("coordinate logical claim"))?;
    count
        .serialize(out)
        .map_err(|_| bad("coordinate logical row count"))?;
    visit(out)
}
fn actual_index_id(
    claim: Hash64,
    map: &BTreeMap<Vec<u8>, CoordinateV3>,
) -> Result<(Hash64, u64), ContextErrorV2> {
    let mut w = CanonicalWriterV3::new();
    write_index(
        claim,
        map.len() as u64,
        |out| {
            for (k, v) in map {
                (k[0], &k[1..], v.id, v.len)
                    .serialize(out)
                    .map_err(|_| bad("coordinate logical entry"))?;
            }
            Ok(())
        },
        &mut w,
    )?;
    Ok(w.finish())
}
fn sync_coordinates(
    b: &mut TransitionBuilder<'_>,
    scope: Hash64,
    map: &BTreeMap<Vec<u8>, CoordinateV3>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let old = keys(&b.state, COORDS, Some(scope))?;
    for key in old.iter() {
        if !map.contains_key(key) {
            put_raw(b, COORDS, Some(scope), key, None)?;
        }
    }
    for (k, v) in map {
        put(b, COORDS, Some(scope), k, Some(v))?;
    }
    root(&b.state, COORDS, Some(scope))
}
fn native_choices(
    s: &PalwChainStateV2,
    checkpoint: &Checkpoint,
    coords: &BTreeMap<Hash64, CoordinateRefV3>,
) -> Result<NativeRetainedV1<BTreeMap<Hash64, NativeRefV3>>, ContextErrorV2> {
    let count = s.kernel_route.as_ref().map_or(0, |r| {
        r.aux
            .iter()
            .filter(|((t, k), raw)| {
                super::palw_claim_local_descriptor_v2::immutable_aux(*t, k)
                    && !(*t == crate::palw_court_family_supplier_v1::FAMILY_SUPPLIER_TABLE_V1
                        && k.len() == 64
                        && raw.get(..2) != Some(&[2, 0]))
            })
            .count()
    });
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64,(u8,&Vec<u8>,&Vec<u8>)>>(count as u64).map_err(ContextErrorV2::Resource)?;
    let _index_lease =
        reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut index = BTreeMap::new();
    if let Some(route) = &s.kernel_route {
        for ((t, k), raw) in &route.aux {
            if super::palw_claim_local_descriptor_v2::immutable_aux(*t, k)
                && !(*t == crate::palw_court_family_supplier_v1::FAMILY_SUPPLIER_TABLE_V1
                    && k.len() == 64
                    && raw.get(..2) != Some(&[2, 0]))
            {
                index.entry(material_id(raw)).or_insert((*t, k, raw));
            }
        }
    }
    let mut count = 0u64;
    let mut bytes = 0u64;
    for (id, body) in &checkpoint.material {
        if coords.contains_key(id) {
            continue;
        }
        if let Some((_, key, raw)) = index.get(id) {
            if raw.as_slice() != body.as_slice() {
                return Err(bad("checkpoint immutable material substitution"));
            }
            count = count
                .checked_add(1)
                .ok_or(bad("native material reference count"))?;
            bytes = bytes
                .checked_add(key.len() as u64)
                .ok_or(bad("native material reference key sum"))?;
        }
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, NativeRefV3>>(count)
        .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<u8>(bytes).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = BTreeMap::new();
    for (id, body) in &checkpoint.material {
        if coords.contains_key(id) {
            continue;
        }
        if let Some((table, key, _)) = index.get(id) {
            out.insert(
                *id,
                NativeRefV3 {
                    version: 3,
                    id: *id,
                    len: body.len() as u64,
                    table: *table,
                    key: (*key).clone(),
                },
            );
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn prune_ids(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    mut keep: impl FnMut(Hash64) -> bool,
) -> Result<(), ContextErrorV2> {
    let old = keys(&b.state, table, None)?;
    for key in old.iter() {
        let id =
            borsh::from_slice::<Hash64>(key).map_err(|_| bad("checkpoint global material key"))?;
        if !keep(id) {
            put_raw(b, table, None, key, None)?;
        }
    }
    Ok(())
}
/// Actual checkpoint caller dispatches here only on the already-existing stage2
/// gate. Old V2 bytes/read semantics remain literal below it and on load fallback.
pub(super) fn write(
    b: &mut TransitionBuilder<'_>,
    checkpoint: &Checkpoint,
) -> Result<(), PalwStateV2Error> {
    if !b
        .params
        .claim_local_settlement_active_at(checkpoint.point.daa_score)
    {
        return super::palw_claim_local_descriptor_v2::write(b, checkpoint);
    }
    atomic(b, |b| write_inner(b, checkpoint)).map_err(err)
}
fn write_inner(
    b: &mut TransitionBuilder<'_>,
    checkpoint: &Checkpoint,
) -> Result<(), ContextErrorV2> {
    if b.state.kernel_route.is_none() {
        return Err(ContextErrorV2::Unavailable(
            "persistent checkpoint requires actual native route",
        ));
    }
    let old_records = keys(&b.state, RECORDS, None)?;
    for key in old_records.iter() {
        let old = lookup::<ScalarV3>(&b.state, RECORDS, None, key)?
            .ok_or(bad("checkpoint old fixed row vanished"))?;
        checked_scalar(&old, key)?;
        let same = checkpoint
            .records
            .get(&old.work)
            .map(scalar_scope)
            .transpose()?
            .is_some_and(|scope| scope == old.scope);
        if !same {
            for table in [INPUTS, RESERVATIONS, MATERIALS, COORDS] {
                drop_owned(b, table, old.scope)?;
            }
        }
        if !checkpoint.records.contains_key(&old.work) {
            put_raw(b, RECORDS, None, key, None)?;
        }
    }
    let coordinates = super::palw_claim_local_material_projection_v2::coordinates(&b.state)
        .map_err(native_err)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, CoordinateRefV3>>(
        checkpoint.records.len() as u64,
    )
    .map_err(ContextErrorV2::Resource)?;
    let _coords_lease =
        reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut coordinate_refs = BTreeMap::new();
    // Actual coordinates are borrowed only while deriving maps. No publication
    // occurs while this native borrowed inventory is held across mutable state.
    let mut plans_q = TirNativeRequestsV1::new();
    plans_q
        .exact_v1::<(
            Hash64,
            Hash64,
            NativeRetainedV1<BTreeMap<Vec<u8>, CoordinateV3>>,
        )>(checkpoint.records.len() as u64)
        .map_err(ContextErrorV2::Resource)?;
    let _plans_lease =
        reserve_native_requests_v1(plans_q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut coordinate_plans = Vec::with_capacity(checkpoint.records.len());
    for record in checkpoint.records.values() {
        if record.liability_closed {
            continue;
        }
        let Work::ChainClaim { claim } = record.work else {
            continue;
        };
        let actual = b
            .state
            .claims
            .get(&claim)
            .ok_or(bad("coordinate owner native claim absent"))?;
        if (
            actual.accepted_block,
            actual.accepted_daa,
            actual.accepted_blue_score,
        ) != (
            record.accepting_block,
            record.accepted_daa,
            record.accepted_blue_score,
        ) {
            return Err(bad("coordinate owner actual birth differs"));
        }
        let map = actual_coordinates(&b.state, claim, &coordinates)?;
        if map.is_empty() {
            continue;
        }
        let (id, len) = actual_index_id(claim, &map)?;
        let body = checkpoint
            .material
            .get(&id)
            .ok_or(bad("actual coordinate index omitted from checkpoint"))?;
        if !record.material.contains(&id) || body.len() as u64 != len || material_id(body) != id {
            return Err(bad("actual canonical coordinate identity differs"));
        }
        coordinate_plans.push((claim, scalar_scope(record)?, map));
    }
    drop(coordinates);
    for (claim, scope, map) in coordinate_plans {
        let (id, len) = actual_index_id(claim, &map)?;
        let root = sync_coordinates(b, scope, &map)?;
        coordinate_refs.insert(
            id,
            CoordinateRefV3 {
                version: 3,
                claim,
                scope,
                root,
                id,
                len,
            },
        );
    }
    let native = native_choices(&b.state, checkpoint, &coordinate_refs)?;
    prune_ids(b, LITERALS, |id| {
        checkpoint.material.contains_key(&id)
            && !native.contains_key(&id)
            && !coordinate_refs.contains_key(&id)
    })?;
    prune_ids(b, NATIVE, |id| native.contains_key(&id))?;
    prune_ids(b, COORD_REFS, |id| coordinate_refs.contains_key(&id))?;
    for (id, body) in &checkpoint.material {
        if material_id(body) != *id {
            return Err(bad("checkpoint material bytes/hash differs"));
        }
        if let Some(reference) = coordinate_refs.get(id) {
            put(b, COORD_REFS, None, id.as_byte_slice(), Some(reference))?;
        } else if let Some(reference) = native.get(id) {
            put(b, NATIVE, None, id.as_byte_slice(), Some(reference))?;
        } else {
            put_raw(b, LITERALS, None, id.as_byte_slice(), Some(body))?;
        }
    }
    for record in checkpoint.records.values() {
        let scope = scalar_scope(record)?;
        let inputs = sync_set(b, INPUTS, scope, &record.inputs)?;
        let reservations = sync_set(b, RESERVATIONS, scope, &record.reservations)?;
        let material = sync_set(b, MATERIALS, scope, &record.material)?;
        let has_coordinate = coordinate_refs.values().any(|r| r.scope == scope);
        if !has_coordinate {
            drop_owned(b, COORDS, scope)?;
        }
        let row = row_scalar(record, scope, inputs, reservations, material);
        let key = work_key(record.work)?;
        put(b, RECORDS, None, &key, Some(&row))?;
    }
    write_liability_v3(b, checkpoint)?;
    // Retire old released codecs only after new roots exist. Their ordinary
    // native delta retains old bytes for exact reorg, not shadow copies forever.
    let count = b.state.kernel_route.as_ref().map_or(0, |r| {
        r.aux
            .keys()
            .filter(|(t, k)| {
                *t == PALW_CLAIM_LOCAL_TABLE_V1
                    && matches!(k.first().copied(), Some(b'C' | b'V' | b'Q' | b'M'))
            })
            .count()
    });
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Vec<u8>>(count as u64)
        .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<u8>(
        (count as u64)
            .checked_mul(134)
            .ok_or(bad("legacy checkpoint key sum"))?,
    )
    .map_err(ContextErrorV2::Resource)?;
    let _legacy_lease =
        reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut legacy = Vec::with_capacity(count);
    if let Some(route) = &b.state.kernel_route {
        for ((t, k), _) in &route.aux {
            if *t == PALW_CLAIM_LOCAL_TABLE_V1
                && matches!(k.first().copied(), Some(b'C' | b'V' | b'Q' | b'M'))
            {
                legacy.push(k.clone());
            }
        }
    }
    for key in legacy {
        b.write_kernel_row(PALW_CLAIM_LOCAL_TABLE_V1, key, None)
            .map_err(native_err)?;
    }
    let d = DescriptorV3 {
        version: 3,
        point: checkpoint.point,
        records: root(&b.state, RECORDS, None)?,
        literals: root(&b.state, LITERALS, None)?,
        native: root(&b.state, NATIVE, None)?,
        coordinates: root(&b.state, COORD_REFS, None)?,
        liability: root(&b.state, LIABILITY, None)?,
    };
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &dkey(), Some(&d))
        .map_err(native_err)?;
    store::collect_unreachable_v2(b)?;
    Ok(())
}
fn body<'a>(s: &'a PalwChainStateV2, v: RowValueV2) -> Result<&'a [u8], ContextErrorV2> {
    let mut key = [0; 65];
    key[0] = b'B';
    key[1..].copy_from_slice(v.data.as_byte_slice());
    let _lease = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
    let raw = s
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())))
        .ok_or(ContextErrorV2::Unavailable(
            "checkpoint immutable body missing",
        ))?;
    if raw.len() < 4
        || u32::from_le_bytes(raw[..4].try_into().expect("body length")) as usize != raw.len() - 4
        || v.bytes != raw.len() as u64 - 4
        || material_id(&raw[4..]) != v.data
    {
        return Err(bad("checkpoint immutable body length/hash"));
    }
    Ok(&raw[4..])
}
fn native_body<'a>(s: &'a PalwChainStateV2, r: &NativeRefV3) -> Result<&'a [u8], ContextErrorV2> {
    if r.version != 3 || !super::palw_claim_local_descriptor_v2::immutable_aux(r.table, &r.key) {
        return Err(bad("checkpoint native material reference profile"));
    }
    let _lease =
        reserve_native_requests_v1(r.key.len() as u64).map_err(ContextErrorV2::Resource)?;
    let raw = s
        .kernel_route
        .as_ref()
        .and_then(|route| route.aux.get(&(r.table, r.key.clone())))
        .ok_or(ContextErrorV2::Unavailable(
            "checkpoint native material missing",
        ))?;
    if r.table == crate::palw_court_family_supplier_v1::FAMILY_SUPPLIER_TABLE_V1
        && r.key.len() == 64
        && raw.get(..2) != Some(&[2, 0])
    {
        return Err(bad("checkpoint native finance reference is mutable codec"));
    }
    if raw.len() as u64 != r.len || material_id(raw) != r.id {
        return Err(bad("checkpoint native material bytes/identity"));
    }
    Ok(raw)
}
fn visit_coordinates(
    s: &PalwChainStateV2,
    r: &CoordinateRefV3,
    mut f: impl FnMut(u8, &[u8], CoordinateV3) -> Result<(), ContextErrorV2>,
) -> Result<(), ContextErrorV2> {
    if r.version != 3 || r.root.spec != spec(COORDS) {
        return Err(bad("checkpoint coordinate reference profile"));
    }
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, r.root, |k, v| {
        if k.len() < 2 {
            return Err(bad("coordinate composite key empty"));
        }
        let raw = body(s, v)?;
        let value = decode::<CoordinateV3>(raw)?;
        match k[0] {
            169 => {
                if !matches!(k.len() - 1, 65 | 73) {
                    return Err(bad("coordinate material key shape"));
                }
            }
            192 => {
                if !matches!(k.len() - 1, 64 | 65 | 129) {
                    return Err(bad("coordinate supplier key shape"));
                }
            }
            255 => {
                let key = borsh::from_slice::<
                    crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
                >(&k[1..])
                .map_err(|_| bad("coordinate staged key codec"))?;
                if staged_owner(&key) != Some(r.claim) {
                    return Err(bad("coordinate staged key source owner differs"));
                }
            }
            _ => return Err(bad("coordinate namespace not actual owner grammar")),
        }
        f(k[0], &k[1..], *value)
    })
}
fn coordinate_len(s: &PalwChainStateV2, r: &CoordinateRefV3) -> Result<u64, ContextErrorV2> {
    let mut len = 74u64;
    visit_coordinates(s, r, |_, key, _| {
        len = len
            .checked_add(77)
            .and_then(|n| n.checked_add(key.len() as u64))
            .ok_or(bad("coordinate logical length overflow"))?;
        Ok(())
    })?;
    if len != r.len {
        return Err(bad("coordinate logical length differs"));
    }
    Ok(len)
}
fn coordinate_bytes(
    s: &PalwChainStateV2,
    r: &CoordinateRefV3,
) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let len = coordinate_len(s, r)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(len).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out =
        Vec::with_capacity(usize::try_from(len).map_err(|_| bad("coordinate logical capacity"))?);
    write_index(
        r.claim,
        r.root.rows,
        |out| {
            visit_coordinates(s, r, |table, key, v| {
                (table, key, v.id, v.len)
                    .serialize(out)
                    .map_err(|_| bad("coordinate logical serialization"))
            })
        },
        &mut out,
    )?;
    if out.len() as u64 != len || material_id(&out) != r.id {
        return Err(bad("coordinate logical byte hash differs"));
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn read_set<T: BorshDeserialize + Ord + Copy>(
    s: &PalwChainStateV2,
    r: ComponentRootV2,
) -> Result<BTreeSet<T>, ContextErrorV2> {
    let mut out = BTreeSet::new();
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, r, |key, v| {
        if body(s, v)? != [1] {
            return Err(bad("checkpoint persistent set membership byte"));
        }
        let t = borsh::from_slice::<T>(key).map_err(|_| bad("checkpoint persistent set key"))?;
        if !out.insert(t) {
            return Err(bad("checkpoint duplicate logical set key"));
        }
        Ok(())
    })?;
    Ok(out)
}
fn validate_header(s: &PalwChainStateV2, d: &DescriptorV3) -> Result<(), ContextErrorV2> {
    if d.version != 3 || s.last_point() != Some(&d.point) {
        return Err(bad("persistent checkpoint actual snapshot point differs"));
    }
    for (table, r) in [
        (RECORDS, d.records),
        (LITERALS, d.literals),
        (NATIVE, d.native),
        (COORD_REFS, d.coordinates),
        (LIABILITY, d.liability),
    ] {
        if r.spec != spec(table) || root(s, table, None)? != r {
            return Err(bad("persistent checkpoint descriptor/current root differs"));
        }
    }
    if d.liability.rows > 1 {
        return Err(bad("persistent checkpoint liability cardinality"));
    }
    if s.kernel_route.as_ref().is_some_and(|r| {
        r.aux.keys().any(|(t, k)| {
            *t == PALW_CLAIM_LOCAL_TABLE_V1
                && matches!(k.first().copied(), Some(b'C' | b'V' | b'Q' | b'M'))
        })
    }) {
        return Err(bad(
            "persistent checkpoint coexists with released V1/V2 rows",
        ));
    }
    Ok(())
}
/// Exact released CheckpointV1 logical reader. All allocations are admitted from
/// complete rooted counts/actual bytes before rebuilding maps/sets/body vectors.
pub(super) fn load(
    s: &PalwChainStateV2,
) -> Result<Option<NativeRetainedV1<Checkpoint>>, PalwStateV2Error> {
    load_inner(s).map_err(err)
}
fn load_inner(
    s: &PalwChainStateV2,
) -> Result<Option<NativeRetainedV1<Checkpoint>>, ContextErrorV2> {
    let Some(d) = descriptor(s)? else {
        return Ok(None);
    };
    validate_header(s, &d)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Work, Record>>(d.records.rows)
        .map_err(ContextErrorV2::Resource)?;
    let materials = d
        .literals
        .rows
        .checked_add(d.native.rows)
        .and_then(|n| n.checked_add(d.coordinates.rows))
        .ok_or(bad("persistent material row count"))?;
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, Vec<u8>>>(materials)
        .map_err(ContextErrorV2::Resource)?;
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, d.records, |key, v| {
        let r = decode::<ScalarV3>(body(s, v)?)?;
        checked_scalar(&r, key)?;
        if root(s, INPUTS, Some(r.scope))? != r.inputs
            || root(s, RESERVATIONS, Some(r.scope))? != r.reservations
            || root(s, MATERIALS, Some(r.scope))? != r.material
        {
            return Err(bad("persistent record child current roots differ"));
        }
        q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Work, ()>>(r.inputs.rows)
            .map_err(ContextErrorV2::Resource)?;
        q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(
            r.reservations
                .rows
                .checked_add(r.material.rows)
                .ok_or(bad("record membership count"))?,
        )
        .map_err(ContextErrorV2::Resource)?;
        Ok(())
    })?;
    nodes::complete_rows(&reader, d.literals, |key, v| {
        if key != v.data.as_byte_slice() {
            return Err(bad("persistent literal key/data identity"));
        }
        let raw = body(s, v)?;
        q.exact_v1::<u8>(raw.len() as u64)
            .map_err(ContextErrorV2::Resource)?;
        Ok(())
    })?;
    nodes::complete_rows(&reader, d.native, |key, v| {
        let r = decode::<NativeRefV3>(body(s, v)?)?;
        if key != r.id.as_byte_slice() {
            return Err(bad("persistent native reference key identity"));
        }
        let raw = native_body(s, &r)?;
        q.exact_v1::<u8>(raw.len() as u64)
            .map_err(ContextErrorV2::Resource)?;
        Ok(())
    })?;
    nodes::complete_rows(&reader, d.coordinates, |key, v| {
        let r = decode::<CoordinateRefV3>(body(s, v)?)?;
        if key != r.id.as_byte_slice() {
            return Err(bad("persistent coordinate reference key identity"));
        }
        q.exact_v1::<u8>(coordinate_len(s, &r)?)
            .map_err(ContextErrorV2::Resource)?;
        Ok(())
    })?;
    let liability_meta = liability_meta(s, d.liability)?;
    check_liability_image(s, &liability_meta)?;
    q.exact_v1::<u8>(liability_meta.len)
        .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut records = BTreeMap::new();
    let mut material = BTreeMap::new();
    let mut liability_state = Vec::with_capacity(
        usize::try_from(liability_meta.len).map_err(|_| bad("liability image host length"))?,
    );
    nodes::complete_rows(&reader, d.records, |key, v| {
        let r = decode::<ScalarV3>(body(s, v)?)?;
        checked_scalar(&r, key)?;
        records.insert(
            r.work,
            Record {
                work: r.work,
                accepted_daa: r.accepted_daa,
                accepted_blue_score: r.accepted_blue_score,
                accepting_block: r.accepting_block,
                anchors: r.anchors,
                inputs: read_set(s, r.inputs)?,
                execution_root: r.execution_root,
                reservations: read_set(s, r.reservations)?,
                final_claim: r.final_claim,
                certified: r.certified,
                voided: r.voided,
                liability_closed: r.liability_closed,
                material: read_set(s, r.material)?,
            },
        );
        Ok(())
    })?;
    nodes::complete_rows(&reader, d.literals, |key, v| {
        let id = borsh::from_slice::<Hash64>(key).map_err(|_| bad("persistent literal id"))?;
        if material.insert(id, body(s, v)?.to_vec()).is_some() {
            return Err(bad("duplicate persistent material"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, d.native, |_, v| {
        let r = decode::<NativeRefV3>(body(s, v)?)?;
        if material
            .insert(r.id, native_body(s, &r)?.to_vec())
            .is_some()
        {
            return Err(bad("material appears in two persistent classes"));
        }
        Ok(())
    })?;
    let native_inventory =
        super::palw_claim_local_material_projection_v2::coordinates(s).map_err(native_err)?;
    nodes::complete_rows(&reader, d.coordinates, |_, v| {
        let r = decode::<CoordinateRefV3>(body(s, v)?)?;
        let work = Work::ChainClaim { claim: r.claim };
        let owner = records
            .get(&work)
            .ok_or(bad("coordinate reference lacks actual owning record"))?;
        if owner.liability_closed
            || scalar_scope(owner)? != r.scope
            || !owner.material.contains(&r.id)
            || root(s, COORDS, Some(r.scope))? != r.root
        {
            return Err(bad("coordinate reference source/scope differs"));
        }
        let current = s
            .claims
            .get(&r.claim)
            .ok_or(bad("coordinate logical owner actual claim absent"))?;
        if (
            current.accepted_block,
            current.accepted_daa,
            current.accepted_blue_score,
        ) != (
            owner.accepting_block,
            owner.accepted_daa,
            owner.accepted_blue_score,
        ) {
            return Err(bad("coordinate logical owner actual birth differs"));
        }
        let actual = actual_coordinates(s, r.claim, &native_inventory)?;
        if actual_index_id(r.claim, &actual)? != (r.id, r.len) {
            return Err(bad(
                "coordinate index differs from actual native owner inventory",
            ));
        }
        let bytes = coordinate_bytes(s, &r)?;
        let (bytes, local) = bytes.into_parts_v1();
        if material.insert(r.id, bytes).is_some() {
            return Err(bad("coordinate material duplicated"));
        }
        drop(local);
        Ok(())
    })?;
    emit_liability_v3(s, &liability_meta, &mut liability_state)?;
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(
        Checkpoint {
            point: d.point,
            records,
            liability_state,
            material,
        },
        lease,
    )))
}
/// Semantic reachability of owner12 references, including archived OLD roots.
/// At most 3 child roots per unique leaf; no full logical I/material Vec created.
pub(super) fn gc_child_roots_v3(
    s: &PalwChainStateV2,
    component: ComponentRootV2,
    key: &[u8],
    v: RowValueV2,
    mut keep: impl FnMut(ComponentRootV2) -> Result<(), ContextErrorV2>,
) -> Result<(), ContextErrorV2> {
    if component.spec.owner != 12 {
        return Ok(());
    }
    if owner_local::allocated(component.spec.table){
        if component.spec!=spec(component.spec.table){return Err(bad("owner-local unallocated profile"))}
        owner_local::gc(s,component,key,v,keep)?;return Ok(())
    }
    if !matches!(
        component.spec.table,
        RECORDS
            | LITERALS
            | NATIVE
            | COORD_REFS
            | INPUTS
            | RESERVATIONS
            | MATERIALS
            | COORDS
            | LIABILITY
            | L_OPEN
            | L_CLAIMS
            | L_COURTS
            | L_KERNEL
    ) || component.spec != spec(component.spec.table)
    {
        return Err(bad("unallocated persistent checkpoint component"));
    }
    match component.spec.table {
        RECORDS => {
            let r = decode::<ScalarV3>(body(s, v)?)?;
            checked_scalar(&r, key)?;
            for child in [r.inputs, r.reservations, r.material] {
                keep(child)?;
            }
        }
        COORD_REFS => {
            let r = decode::<CoordinateRefV3>(body(s, v)?)?;
            if key != r.id.as_byte_slice() {
                return Err(bad("archived coordinate ref key differs"));
            }
            coordinate_len(s, &r)?;
            let mut writer = CanonicalWriterV3::new();
            write_index(
                r.claim,
                r.root.rows,
                |out| {
                    visit_coordinates(s, &r, |table, key, v| {
                        (table, key, v.id, v.len)
                            .serialize(out)
                            .map_err(|_| bad("coordinate archived hash stream"))
                    })
                },
                &mut writer,
            )?;
            if writer.finish() != (r.id, r.len) {
                return Err(bad("archived coordinate canonical identity differs"));
            }
            keep(r.root)?;
        }
        NATIVE => {
            let r = decode::<NativeRefV3>(body(s, v)?)?;
            if r.version != 3
                || key != r.id.as_byte_slice()
                || !super::palw_claim_local_descriptor_v2::immutable_aux(r.table, &r.key)
            {
                return Err(bad("archived checkpoint native reference profile"));
            }
        }
        INPUTS => {
            if body(s, v)? != [1] || borsh::from_slice::<Work>(key).is_err() {
                return Err(bad("archived persistent input membership"));
            }
        }
        RESERVATIONS | MATERIALS => {
            if body(s, v)? != [1] || key.len() != 64 {
                return Err(bad("archived persistent hash membership"));
            }
        }
        COORDS => {
            if key.len() < 2 || !matches!(key[0], 169 | 192 | 255) {
                return Err(bad("archived coordinate row key"));
            }
            decode::<CoordinateV3>(body(s, v)?)?;
        }
        LITERALS => {
            if key != v.data.as_byte_slice() {
                return Err(bad("archived literal key identity"));
            }
            body(s, v)?;
        }
        LIABILITY => {
            if key != Hash64::default().as_byte_slice() {
                return Err(bad("archived liability key"));
            }
            let m = decode::<LiabilityMetaV3>(body(s, v)?)?;
            if m.version != 3 || m.open.spec != spec(L_OPEN) || m.kernels.spec != spec(L_KERNEL) {
                return Err(bad("archived liability meta profile"));
            }
            check_liability_image(s, &m)?;
            for r in [m.open, m.kernels] {
                keep(r)?;
            }
        }
        L_OPEN => {
            let r = liability_open(s, key, v)?;
            keep(r.body)?;
            keep(r.courts)?;
        }
        L_CLAIMS => {
            if key.len() != 64 {
                return Err(bad("archived liability claim key"));
            }
            decode::<PalwClaimStateV2>(body(s, v)?)?;
        }
        L_COURTS => {
            let r = decode::<(Hash64, PalwCourtSessionStateV2)>(body(s, v)?)?;
            if key != r.0.as_byte_slice() {
                return Err(bad("archived liability court key"));
            }
        }
        L_KERNEL => {
            if key.len() != 64 || body(s, v)? != [1] {
                return Err(bad("archived liability kernel member"));
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
fn stream_set_valid<T: BorshDeserialize>(
    s: &PalwChainStateV2,
    r: ComponentRootV2,
) -> Result<(), ContextErrorV2> {
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, r, |key, v| {
        if body(s, v)? != [1] || borsh::from_slice::<T>(key).is_err() {
            return Err(bad("persistent set complete key/value closure"));
        }
        Ok(())
    })
}
/// Full current rooted integrity, without reconstructing all logical body Vecs.
pub(super) fn validate(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    let d = descriptor(s)?;
    if d.is_none() {
        if let Some(route) = &s.kernel_route {
            for ((t, k), raw) in &route.aux {
                if *t == PALW_CLAIM_LOCAL_TABLE_V1
                    && k.first() == Some(&b'I')
                    && decode::<ComponentRootV2>(raw)?.spec.owner == 12
                {
                    return Err(bad(
                        "persistent checkpoint current roots lack V3 descriptor",
                    ));
                }
            }
        }
        return Ok(());
    }
    let d = d.expect("checked descriptor");
    validate_header(s, &d)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ScalarV3>>(
        d.records.rows,
    )
    .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, CoordinateRefV3>>(
        d.coordinates.rows,
    )
    .map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut scopes = BTreeMap::new();
    let mut coordinates = BTreeMap::new();
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, d.records, |key, v| {
        let r = decode::<ScalarV3>(body(s, v)?)?;
        checked_scalar(&r, key)?;
        if root(s, INPUTS, Some(r.scope))? != r.inputs
            || root(s, RESERVATIONS, Some(r.scope))? != r.reservations
            || root(s, MATERIALS, Some(r.scope))? != r.material
        {
            return Err(bad("persistent record/set current binding"));
        }
        stream_set_valid::<Work>(s, r.inputs)?;
        stream_set_valid::<Hash64>(s, r.reservations)?;
        stream_set_valid::<Hash64>(s, r.material)?;
        nodes::complete_rows(&reader, r.material, |id, _| {
            if nodes::lookup(&reader, d.literals, id)?.is_none()
                && nodes::lookup(&reader, d.native, id)?.is_none()
                && nodes::lookup(&reader, d.coordinates, id)?.is_none()
            {
                return Err(ContextErrorV2::Unavailable(
                    "persistent record material body reference missing",
                ));
            }
            Ok(())
        })?;
        if scopes.insert(r.scope, *r).is_some() {
            return Err(bad("persistent record scope reused"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, d.literals, |key, v| {
        if key != v.data.as_byte_slice() {
            return Err(bad("persistent literal material identity"));
        }
        body(s, v)?;
        if nodes::lookup(&reader, d.native, key)?.is_some()
            || nodes::lookup(&reader, d.coordinates, key)?.is_some()
        {
            return Err(bad("persistent material class overlap"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, d.native, |key, v| {
        let r = decode::<NativeRefV3>(body(s, v)?)?;
        if key != r.id.as_byte_slice() {
            return Err(bad("persistent native material identity"));
        }
        native_body(s, &r)?;
        if nodes::lookup(&reader, d.coordinates, key)?.is_some() {
            return Err(bad("persistent native/coordinate class overlap"));
        }
        Ok(())
    })?;
    let inventory =
        super::palw_claim_local_material_projection_v2::coordinates(s).map_err(native_err)?;
    nodes::complete_rows(&reader, d.coordinates, |key, v| {
        let r = decode::<CoordinateRefV3>(body(s, v)?)?;
        if key != r.id.as_byte_slice() {
            return Err(bad("persistent coordinate identity"));
        }
        let owner = scopes
            .get(&r.scope)
            .ok_or(bad("persistent coordinate source record absent"))?;
        if owner.work != (Work::ChainClaim { claim: r.claim })
            || owner.liability_closed
            || root(s, COORDS, Some(r.scope))? != r.root
        {
            return Err(bad("persistent coordinate source/root differs"));
        }
        let c = s
            .claims
            .get(&r.claim)
            .ok_or(bad("persistent coordinate actual source absent"))?;
        if (c.accepted_block, c.accepted_daa, c.accepted_blue_score)
            != (
                owner.accepting_block,
                owner.accepted_daa,
                owner.accepted_blue_score,
            )
        {
            return Err(bad("persistent coordinate actual source birth differs"));
        }
        let member = nodes::lookup(&reader, owner.material, r.id.as_byte_slice())?.ok_or(bad(
            "coordinate logical id missing from owning material set",
        ))?;
        if body(s, member)? != [1] {
            return Err(bad("coordinate logical membership byte differs"));
        }
        let actual = actual_coordinates(s, r.claim, &inventory)?;
        if actual_index_id(r.claim, &actual)? != (r.id, r.len) {
            return Err(bad(
                "persistent coordinate source complete native inventory differs",
            ));
        }
        let mut writer = CanonicalWriterV3::new();
        write_index(
            r.claim,
            r.root.rows,
            |out| {
                visit_coordinates(s, &r, |t, k, v| {
                    (t, k, v.id, v.len)
                        .serialize(out)
                        .map_err(|_| bad("persistent coordinate source stream"))
                })
            },
            &mut writer,
        )?;
        if writer.finish() != (r.id, r.len) {
            return Err(bad("persistent coordinate stored complete image differs"));
        }
        if coordinates.insert(r.scope, *r).is_some() {
            return Err(bad(
                "persistent coordinate scope has duplicate logical images",
            ));
        }
        Ok(())
    })?;
    let mut lq = TirNativeRequestsV1::new();
    lq.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, u64>>(
        s.court_sessions.len() as u64,
    )
    .map_err(ContextErrorV2::Resource)?;
    let _counts_lease =
        reserve_native_requests_v1(lq.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut court_counts = BTreeMap::<Hash64, u64>::new();
    for c in s.court_sessions.values() {
        let n = court_counts.entry(c.claim).or_default();
        *n = n
            .checked_add(1)
            .ok_or(bad("liability court count overflow"))?;
    }
    let lm = liability_meta(s, d.liability)?;
    if root(s, L_OPEN, None)? != lm.open || root(s, L_KERNEL, None)? != lm.kernels {
        return Err(bad("liability current roots differ"));
    }
    check_liability_image(s, &lm)?;
    nodes::complete_rows(&reader, lm.open, |key, v| {
        let row = liability_open(s, key, v)?;
        let record = lookup::<ScalarV3>(
            s,
            RECORDS,
            None,
            &work_key(Work::ChainClaim { claim: row.claim })?,
        )?
        .ok_or(bad("liability claim has no record"))?;
        if record.liability_closed
            || record.scope != row.scope
            || root(s, L_CLAIMS, Some(row.scope))? != row.body
            || root(s, L_COURTS, Some(row.scope))? != row.courts
        {
            return Err(bad("liability current source/root differs"));
        }
        let claim_body = nodes::lookup(&reader, row.body, key)?
            .ok_or(bad("liability current claim body missing"))?;
        let raw = body(s, claim_body)?;
        let actual = s
            .claims
            .get(&row.claim)
            .ok_or(bad("liability actual claim unavailable"))?;
        let actual = encode(actual)?;
        if raw != actual.as_slice() {
            return Err(bad("liability current claim differs"));
        }
        let mut birth = None;
        for (_, bytes) in super::palw_claim_local_rewards_v1::frames(s) {
            let p = PalwBlockContextV2::deserialize_reader(&mut bytes.as_slice())
                .map_err(|_| bad("liability current frame"))?;
            if (p.daa_score, p.blue_score) == (record.accepted_daa, record.accepted_blue_score) {
                birth = Some(p.block)
            }
        }
        if birth != row.birth {
            return Err(bad("liability current birth frame differs"));
        }
        if nodes::lookup(&reader, record.material, claim_body.data.as_byte_slice())?.is_none() {
            return Err(bad("liability claim material membership"));
        }
        nodes::complete_rows(&reader, row.courts, |key, v| {
            let pair = decode::<(Hash64, PalwCourtSessionStateV2)>(body(s, v)?)?;
            if key != pair.0.as_byte_slice()
                || s.court_sessions.get(&pair.0) != Some(&pair.1)
                || pair.1.claim != row.claim
            {
                return Err(bad("liability current court differs"));
            }
            if nodes::lookup(&reader, record.material, v.data.as_byte_slice())?.is_none() {
                return Err(bad("liability court material membership"));
            }
            Ok(())
        })?;
        if court_counts.get(&row.claim).copied().unwrap_or(0) != row.courts.rows {
            return Err(bad("liability current court population"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, d.records, |key, v| {
        let row = decode::<ScalarV3>(body(s, v)?)?;
        checked_scalar(&row, key)?;
        let (table, id) = match row.work {
            Work::ChainClaim { claim } => (L_OPEN, claim),
            Work::KernelClaim { claim } => (L_KERNEL, claim),
            _ => return Ok(()),
        };
        if nodes::lookup(&reader, root(s, table, None)?, id.as_byte_slice())?.is_some()
            == row.liability_closed
        {
            return Err(bad("liability record complete membership"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, lm.kernels, |key, _| {
        let id: Hash64 = borsh::from_slice(key).map_err(|_| bad("liability kernel key"))?;
        let row = lookup::<ScalarV3>(
            s,
            RECORDS,
            None,
            &work_key(Work::KernelClaim { claim: id })?,
        )?
        .ok_or(bad("liability kernel orphan"))?;
        if row.liability_closed {
            return Err(bad("closed kernel in liability"));
        }
        Ok(())
    })?;
    if let Some(route) = &s.kernel_route {
        for ((t, key), raw) in &route.aux {
            if *t != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'I') {
                continue;
            }
            let r = decode::<ComponentRootV2>(raw)?;
            let ks = match key.len() {
                18 => borsh::from_slice::<ComponentSpecV2>(&key[1..]).ok(),
                84 if key[1..3] == [2, 0] => borsh::from_slice::<ComponentSpecV2>(&key[67..]).ok(),
                _ => None,
            };
            if r.spec.owner != 12 && ks.is_none_or(|spec| spec.owner != 12) {
                continue;
            }
            if ks != Some(r.spec)
                || !(owner_local::allocated(r.spec.table)||matches!(
                    r.spec.table,
                    RECORDS
                        | LITERALS
                        | NATIVE
                        | COORD_REFS
                        | INPUTS
                        | RESERVATIONS
                        | MATERIALS
                        | COORDS
                        | LIABILITY
                        | L_OPEN
                        | L_CLAIMS
                        | L_COURTS
                        | L_KERNEL
                ))
                || r.spec != spec(r.spec.table)
            {
                return Err(bad("unallocated persistent descriptor current profile"));
            }
            match key.len() {
                18 if owner_local::global(r.spec.table)||matches!(
                    r.spec.table,
                    RECORDS | LITERALS | NATIVE | COORD_REFS | LIABILITY | L_OPEN | L_KERNEL
                ) => {}
                84 if key[1..3] == [2, 0] => {
                    let scope = borsh::from_slice::<Hash64>(&key[3..67])
                        .map_err(|_| bad("persistent current scope key"))?;
                    let row = scopes
                        .get(&scope)
                        .ok_or(bad("orphan persistent record-owned current root"))?;
                    let table = r.spec.table;
                    let expected = match table {
                        t if owner_local::allocated(t)=>owner_local::current_owned(s,t,scope)?.ok_or(bad("owner-local global at owned key"))?,
                        INPUTS => row.inputs,
                        RESERVATIONS => row.reservations,
                        MATERIALS => row.material,
                        L_CLAIMS | L_COURTS => {
                            let Work::ChainClaim { claim } = row.work else {
                                return Err(bad("nonchain liability courts owner"));
                            };
                            let r =
                                lookup::<LiabilityOpenV3>(s, L_OPEN, None, claim.as_byte_slice())?
                                    .ok_or(bad("orphan liability courts current root"))?;
                            if r.scope != scope {
                                return Err(bad("liability court source scope"));
                            }
                            if table == L_CLAIMS { r.body } else { r.courts }
                        }
                        COORDS => {
                            coordinates
                                .get(&scope)
                                .ok_or(bad("orphan persistent coordinate current root"))?
                                .root
                        }
                        _ => return Err(bad("global persistent profile at owned root")),
                    };
                    if expected != *r {
                        return Err(bad("persistent owned root differs from actual record/ref"));
                    }
                }
                _ => return Err(bad("persistent current namespace allocation")),
            }
        }
    }
    owner_local::validate(s,&d)?;
    Ok(())
}
/// Explicit version dispatch for released D2 and new D3, not a tag alias.
pub(crate) fn scan_checkpoint_descriptor_wire_v3<R: std::io::Read>(
    r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
    use crate::palw_native_resource_v1::{NativeWireScanV1, NativeWireShapeV1};
    let mut shape = NativeWireShapeV1::default();
    r.step()?;
    let version = r.short()?;
    macro_rules! scan {
        ($t:ty) => {
            shape = shape.merge(<$t as NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        };
    }
    scan!(PalwBlockContextV2);
    match version {
        2 => {
            scan!(Hash64);
            scan!(u64);
            scan!(Hash64);
            scan!(u64);
            scan!(Option<Hash64>);
        }
        3 => {
            for _ in 0..5 {
                scan!(ComponentRootV2);
            }
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "checkpoint descriptor version",
            ));
        }
    }
    Ok(shape)
}
/// Validate historical storage-root closure without interpreting it as current
/// native source admission. References stay rooted even after current I moved.
pub(super) fn validate_archived_roots_v3(
    s: &PalwChainStateV2,
    initial: &[ComponentRootV2],
) -> Result<(), ContextErrorV2> {
    if !initial.iter().any(|r| r.spec.owner == 12) {
        return Ok(());
    }
    let content = s.kernel_route.as_ref().map_or(0, |r| {
        r.aux
            .keys()
            .filter(|(t, k)| {
                *t == PALW_CLAIM_LOCAL_TABLE_V1 && matches!(k.first().copied(), Some(b'N' | b'B'))
            })
            .count()
    }) as u64;
    let limit = (initial.len() as u64)
        .checked_add(
            content
                .checked_mul(3)
                .ok_or(bad("archived checkpoint root work count"))?,
        )
        .ok_or(bad("archived checkpoint root count"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentRootV2>(limit)
        .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(content)
        .map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut roots = Vec::with_capacity(
        usize::try_from(limit).map_err(|_| bad("archived checkpoint root capacity"))?,
    );
    roots.extend(initial.iter().filter(|r| r.spec.owner == 12).copied());
    let mut seen = BTreeSet::new();
    let reader = StateNodesV2(s);
    let mut at = 0;
    while at < roots.len() {
        let r = roots[at];
        at += 1;
        nodes::visit_nodes(&reader, r, |id, node| {
            if seen.insert(id) {
                if let nodes::ContextNodeV2::Leaf { key, value } = node {
                    body(s, *value)?;
                    gc_child_roots_v3(s, r, key, *value, |child| {
                        if roots.len() == roots.capacity() {
                            return Err(bad("archived checkpoint root bound exhausted"));
                        }
                        roots.push(child);
                        Ok(())
                    })?;
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
#[cfg(test)]
pub(super) fn record_roots_for_test_v3(
    s: &PalwChainStateV2,
    work: Work,
) -> Result<[ComponentRootV2; 4], ContextErrorV2> {
    let key = work_key(work)?;
    let r = lookup::<ScalarV3>(s, RECORDS, None, &key)?.ok_or(bad("test fixed record absent"))?;
    Ok([
        r.inputs,
        r.reservations,
        r.material,
        root(s, COORDS, Some(r.scope))?,
    ])
}

/// Fixed-key/current-root readonly observation. Not the full checkpoint
/// validator and not an eligibility/settlement API. No global aux traversal.
pub(super) fn observe_current_descriptor_v1(s:&PalwChainStateV2)->Result<Option<DescriptorV3>,PalwStateV2Error>{
    let Some(d)=descriptor(s).map_err(err)?else{return Ok(None)};
    if d.version!=3||s.last_point()!=Some(&d.point){return Err(err(bad("observed D3 actual point/version differs")))}
    for(table,r)in[(RECORDS,d.records),(LITERALS,d.literals),(NATIVE,d.native),(COORD_REFS,d.coordinates),(LIABILITY,d.liability)]{
        if r.spec!=spec(table)||root(s,table,None).map_err(err)?!=r{return Err(err(bad("observed D3 current native root differs")))}
    }
    if d.liability.rows>1{return Err(err(bad("observed D3 liability cardinality")))}
    Ok(Some(d))
}
