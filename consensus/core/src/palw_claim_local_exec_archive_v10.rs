//! Actual native EXEC source records; no world/frame projection or credit gate.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::{PALW_CLAIM_LOCAL_TABLE_V1, PalwClaimLocalNativeEventV1, PalwClaimLocalWorkRefV1 as Work};
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use crate::palw_work_slice_v2::{PalwWorkRootPhaseV2 as Phase, PalwWorkRootV2 as Root, PalwWorkSliceRowV2 as Slice};
use borsh::{BorshDeserialize, BorshSerialize};
const SOURCES: u32 = 1;
const BIRTHS: u32 = 2;
const EVENTS: u32 = 3;
const CONSUMED: u32 = 4;
const SCRATCH: u32 = 5;
const BY_SCOPE: u32 = 6;
const EVENT_OWNER: u32 = 7;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 14, table, schema: 10, key_bytes: 64, value_bytes: u64::MAX }
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
        _ => bad("EXEC source archive journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("EXEC source archive encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out).map_err(|_| bad("EXEC source archive encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("EXEC source archive typed body codec"),
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
    let mut w = Writer(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/exec-source-archive/v10").to_state());
    domain.serialize(&mut w).map_err(|_| bad("EXEC source archive domain"))?;
    v.serialize(&mut w).map_err(|_| bad("EXEC source archive identity"))?;
    let mut bytes = [0; 64];
    bytes.copy_from_slice(w.0.finalize().as_bytes());
    Ok(Hash64::from_bytes(bytes))
}
fn value(raw: &[u8]) -> RowValueV2 {
    RowValueV2 { data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw), bytes: raw.len() as u64 }
}
fn root(s: &PalwChainStateV2, table: u32, scope: Option<Hash64>) -> Result<ComponentRootV2, ContextErrorV2> {
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
    key: Hash64,
) -> Result<Option<NativeRetainedV1<T>>, ContextErrorV2> {
    let r = StateNodesV2(s);
    nodes::lookup(&r, root(s, table, scope)?, key.as_byte_slice())?
        .map(|v| {
            let body = r.body(v)?;
            decode(&body)
        })
        .transpose()
}
fn put<T: BorshSerialize>(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Option<Hash64>,
    key: Hash64,
    row: Option<&T>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let raw = row.map(encode).transpose()?;
    let r = StateNodesV2(&b.state);
    let before = root(&b.state, table, scope)?;
    let old = nodes::lookup(&r, before, key.as_byte_slice())?;
    let plan = nodes::update(&r, before, key.as_byte_slice(), old, raw.as_ref().map(|r| value(r)))?;
    match scope {
        Some(id) => store::publish_owned_component_v2(b, id, &plan, raw.as_deref().map(Vec::as_slice))?,
        None => store::publish_component_v2(b, &plan, raw.as_deref().map(Vec::as_slice))?,
    };
    Ok(plan.after())
}
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
struct Source {
    version: u16,
    root: Hash64,
    scope: Hash64,
    identity: Hash64,
    birth: Option<PalwBlockContextV2>,
    captured_at: PalwBlockContextV2,
    captured_ordinal: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(Source;u16,Hash64,Hash64,Hash64,Option<PalwBlockContextV2>,PalwBlockContextV2,u64);
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
struct Born {
    version: u16,
    work: Work,
    scope: Hash64,
    point: PalwBlockContextV2,
    ordinal: u64,
    original: RowValueV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(Born;u16,Work,Hash64,PalwBlockContextV2,u64,RowValueV2);
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
struct Settled {
    version: u16,
    work: Work,
    scope: Hash64,
    point: PalwBlockContextV2,
    ordinal: u64,
    before: RowValueV2,
    after: RowValueV2,
    consumed: ComponentRootV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(Settled;u16,Work,Hash64,PalwBlockContextV2,u64,RowValueV2,RowValueV2,ComponentRootV2);
fn work_root(work: Work) -> Result<Hash64, ContextErrorV2> {
    match work {
        Work::ExecSession { root } | Work::ExecSlice { root, .. } | Work::ExecSettlement { root, .. } => Ok(root),
        _ => Err(bad("non EXEC archive work")),
    }
}
fn work_key(work: Work) -> Result<Hash64, ContextErrorV2> {
    hash(1, &work)
}
fn physical(root: Hash64, row: &Root) -> Result<Hash64, ContextErrorV2> {
    hash(
        2,
        &(
            root,
            row.anchor,
            row.opened_daa,
            row.class_id,
            row.canonical_job_id,
            row.job_work_id,
            row.kernel_version,
            row.plan_root,
            row.total_work,
            &row.boundaries,
            row.initial_state_root,
            row.evidence_policy_root,
            row.root_bond,
            &row.extra_executors,
            row.executor_exposure,
            row.expiry_daa,
            row.prefix_claim,
        ),
    )
}
fn source_scope(row: &Source) -> Result<Hash64, ContextErrorV2> {
    hash(3, &(row.root, row.identity, row.birth.is_some(), row.captured_at, row.captured_ordinal))
}
fn source(s: &PalwChainStateV2, id: Hash64) -> Result<Option<NativeRetainedV1<Source>>, ContextErrorV2> {
    let v = lookup::<Source>(s, SOURCES, None, id)?;
    if let Some(v) = &v
        && (v.version != 10 || v.root != id || v.scope != source_scope(v)?)
    {
        return Err(bad("EXEC source catalogue identity"));
    }
    Ok(v)
}
fn ensure_source(
    b: &mut TransitionBuilder<'_>,
    id: Hash64,
    native: &Root,
    birth: Option<PalwBlockContextV2>,
    captured_at: PalwBlockContextV2,
    captured_ordinal: u64,
) -> Result<Source, ContextErrorV2> {
    let identity = physical(id, native)?;
    if let Some(v) = source(&b.state, id)? {
        if v.identity != identity || birth.is_some() {
            return Err(bad("EXEC actual source changed without native deletion"));
        }
        return Ok(*v);
    }
    let row = Source { version: 10, root: id, scope: Hash64::default(), identity, birth, captured_at, captured_ordinal };
    let row = Source { scope: source_scope(&row)?, ..row };
    put(b, SOURCES, None, id, Some(&row))?;
    put(b, BY_SCOPE, None, row.scope, Some(&id))?;
    Ok(row)
}
fn literal<T: BorshSerialize>(b: &mut TransitionBuilder<'_>, row: &T) -> Result<RowValueV2, ContextErrorV2> {
    let raw = encode(row)?;
    let v = value(&raw);
    put(b, SCRATCH, None, Hash64::default(), Some(row))?;
    put::<T>(b, SCRATCH, None, Hash64::default(), None)?;
    Ok(v)
}
fn inventory(
    row: RowValueV2,
    extra: &[RowValueV2],
    component: Option<ComponentRootV2>,
) -> Result<NativeRetainedV1<store::ArchiveReferencesV2>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<RowValueV2>(1 + extra.len() as u64).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<ComponentRootV2>(u64::from(component.is_some())).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut literals = Vec::with_capacity(1 + extra.len());
    literals.push(row);
    literals.extend_from_slice(extra);
    let mut components = Vec::with_capacity(usize::from(component.is_some()));
    if let Some(c) = component {
        components.push(c)
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(store::ArchiveReferencesV2 { version: 2, components, literals }, lease))
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn archive_id(row: RowValueV2, extra: &[RowValueV2], component: Option<ComponentRootV2>) -> Result<Hash64, ContextErrorV2> {
    let a = inventory(row, extra, component)?;
    let raw = encode(&*a)?;
    Ok(value(&raw).data)
}
fn retain_archive(
    b: &mut TransitionBuilder<'_>,
    row: RowValueV2,
    extra: &[RowValueV2],
    component: Option<ComponentRootV2>,
) -> Result<(), ContextErrorV2> {
    let a = inventory(row, extra, component)?;
    let raw = encode(&*a)?;
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &akey(value(&raw).data), Some(&*a)).map_err(native_err)
}
fn owned_key(scope: Hash64, table: u32) -> Result<[u8; 84], ContextErrorV2> {
    let mut k = [0; 84];
    k[0] = b'I';
    k[1..3].copy_from_slice(&2u16.to_le_bytes());
    k[3..67].copy_from_slice(scope.as_byte_slice());
    spec(table).serialize(&mut &mut k[67..]).map_err(|_| bad("EXEC owned root key"))?;
    Ok(k)
}
fn slice_key(index: u32) -> Hash64 {
    let mut b = [0; 64];
    b[..4].copy_from_slice(&index.to_be_bytes());
    Hash64::from_bytes(b)
}
fn slice_index(key: &[u8]) -> Result<u32, ContextErrorV2> {
    if key.len() != 64 || key[4..].iter().any(|b| *b != 0) {
        return Err(bad("EXEC consumed slice key"));
    }
    Ok(u32::from_be_bytes(key[..4].try_into().map_err(|_| bad("EXEC consumed slice index"))?))
}
fn birth(
    b: &mut TransitionBuilder<'_>,
    source: Source,
    work: Work,
    point: PalwBlockContextV2,
    ordinal: u64,
    original: RowValueV2,
) -> Result<(), ContextErrorV2> {
    let key = work_key(work)?;
    if lookup::<Born>(&b.state, BIRTHS, Some(source.scope), key)?.is_some() {
        return Err(bad("EXEC actual birth duplicated in physical source"));
    }
    let row = Born { version: 10, work, scope: source.scope, point, ordinal, original };
    let raw = encode(&row)?;
    put(b, BIRTHS, Some(source.scope), key, Some(&row))?;
    retain_archive(b, value(&raw), &[original], None)
}
fn root_at(
    b: &TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    end: usize,
    id: Hash64,
) -> Result<Option<NativeRetainedV1<Root>>, ContextErrorV2> {
    for e in b.entries[..end].iter().rev() {
        if let PalwDeltaEntryV2::ExecV2Row { table, key, new, .. } = e
            && *table == PALW_EXEC_V2_TABLE_ROOTS_V1
            && key.as_slice() == id.as_byte_slice()
        {
            return new.as_ref().map(|raw| decode(raw)).transpose();
        }
    }
    parent
        .exec_v2
        .roots
        .get(&id)
        .map(|row| {
            let raw = encode(row)?;
            decode(&raw)
        })
        .transpose()
}
fn slice_at(
    b: &TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    end: usize,
    id: Hash64,
    index: u32,
) -> Result<Option<NativeRetainedV1<Slice>>, ContextErrorV2> {
    for e in b.entries[..end].iter().rev() {
        if let PalwDeltaEntryV2::ExecV2Row { table, key, new, .. } = e
            && *table == PALW_EXEC_V2_TABLE_SLICES_V1
        {
            let k: (Hash64, u32) = borsh::from_slice(key).map_err(|_| bad("EXEC actual slice key"))?;
            if k == (id, index) {
                return new.as_ref().map(|raw| decode(raw)).transpose();
            }
        }
    }
    parent
        .exec_v2
        .slices
        .get(&(id, index))
        .map(|row| {
            let raw = encode(row)?;
            decode(&raw)
        })
        .transpose()
}
fn settle(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    index: usize,
    source: Source,
    point: PalwBlockContextV2,
    ordinal: u64,
    before: &Root,
    after: &Root,
) -> Result<(), ContextErrorV2> {
    let Phase::Settled { settled_daa } = after.phase else {
        return Err(bad("EXEC event is not settled"));
    };
    if settled_daa != point.daa_score || before.opened_daa > settled_daa {
        return Err(bad("EXEC settlement actual point differs"));
    }
    let work = Work::ExecSettlement { root: source.root, block: point.block };
    let key = work_key(work)?;
    if lookup::<Settled>(&b.state, EVENTS, Some(source.scope), key)?.is_some() {
        return Err(bad("EXEC actual settlement duplicated"));
    }
    let event_scope = hash(4, &(source.scope, work, ordinal))?;
    for i in 0..after.next_index {
        let row = slice_at(b, parent, index, source.root, i)?.ok_or(bad("EXEC actual settlement consumes missing slice"))?;
        if row.accepted_daa > settled_daa {
            return Err(bad("EXEC actual settlement consumes future slice"));
        }
        put(b, CONSUMED, Some(event_scope), slice_key(i), Some(&*row))?;
    }
    let before = literal(b, before)?;
    let after = literal(b, after)?;
    let consumed = root(&b.state, CONSUMED, Some(event_scope))?;
    let row = Settled { version: 10, work, scope: source.scope, point, ordinal, before, after, consumed };
    let raw = encode(&row)?;
    put(b, EVENTS, Some(source.scope), key, Some(&row))?;
    put(b, EVENT_OWNER, None, event_scope, Some(&(source.root, work)))?;
    retain_archive(b, value(&raw), &[before, after], Some(consumed))
}
fn release_source(b: &mut TransitionBuilder<'_>, id: Hash64) -> Result<(), ContextErrorV2> {
    let Some(source) = source(&b.state, id)? else {
        return Ok(());
    };
    let births = root(&b.state, BIRTHS, Some(source.scope))?;
    let events = root(&b.state, EVENTS, Some(source.scope))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Hash64>(
        births
            .rows
            .checked_add(events.rows.checked_mul(2).ok_or(bad("EXEC GC event count overflow"))?)
            .ok_or(bad("EXEC GC source count overflow"))?,
    )
    .map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut archives = Vec::with_capacity(usize::try_from(births.rows + events.rows).map_err(|_| bad("EXEC GC source count"))?);
    let mut event_scopes = Vec::with_capacity(usize::try_from(events.rows).map_err(|_| bad("EXEC GC event count"))?);
    {
        let r = StateNodesV2(&b.state);
        nodes::complete_rows(&r, births, |_, v| {
            let raw = r.body(v)?;
            let row = decode::<Born>(&raw)?;
            archives.push(archive_id(v, &[row.original], None)?);
            Ok(())
        })?;
        nodes::complete_rows(&r, events, |_, v| {
            let raw = r.body(v)?;
            let row = decode::<Settled>(&raw)?;
            archives.push(archive_id(v, &[row.before, row.after], Some(row.consumed))?);
            event_scopes.push(hash(4, &(source.scope, row.work, row.ordinal))?);
            Ok(())
        })?;
    }
    for id in archives {
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(id), None).map_err(native_err)?;
    }
    for scope in event_scopes {
        put::<(Hash64, Work)>(b, EVENT_OWNER, None, scope, None)?;
        b.write_kernel_row_encoded_v1::<ComponentRootV2>(PALW_CLAIM_LOCAL_TABLE_V1, &owned_key(scope, CONSUMED)?, None)
            .map_err(native_err)?;
    }
    for table in [BIRTHS, EVENTS] {
        b.write_kernel_row_encoded_v1::<ComponentRootV2>(PALW_CLAIM_LOCAL_TABLE_V1, &owned_key(source.scope, table)?, None)
            .map_err(native_err)?;
    }
    put::<Hash64>(b, BY_SCOPE, None, source.scope, None)?;
    put::<Source>(b, SOURCES, None, id, None)?;
    Ok(())
}
fn atomic<T>(
    b: &mut TransitionBuilder<'_>,
    f: impl FnOnce(&mut TransitionBuilder<'_>) -> Result<T, ContextErrorV2>,
) -> Result<T, ContextErrorV2> {
    let first = b.entries.len();
    let gc_point = b.claim_local_gc_sequence_v3;
    let answer = f(b);
    if answer.is_err() {
        super::palw_claim_local_context_gc_v3::rollback_aux_v3(b, first, gc_point, |table, key| {
            table == PALW_CLAIM_LOCAL_TABLE_V1 && matches!(key.first().copied(), Some(b'N' | b'B' | b'I' | b'A'))
        })?;
    }
    answer
}
/// Once, before whole-frame serialization. Only original ordered native events
/// produce births/settlements. No post-event current raw root supplies history.
pub(super) fn capture_executed_exec_v10(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    point: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(());
    }
    let end = b.entries.len();
    atomic(b, |b| {
        let mut ordinal = 0u64;
        for index in 0..end {
            let native = matches!(&b.entries[index], PalwDeltaEntryV2::ExecV2Row { .. } | PalwDeltaEntryV2::Claim { .. })
                || matches!(&b.entries[index],PalwDeltaEntryV2::KernelRouteRow{table,..}if *table!=PALW_CLAIM_LOCAL_TABLE_V1);
            let at = ordinal;
            if native {
                ordinal = ordinal.checked_add(1).ok_or(bad("EXEC native ordinal overflow"))?;
            }
            let PalwDeltaEntryV2::ExecV2Row { table, key, old, new } = &b.entries[index] else { continue };
            if *table == PALW_EXEC_V2_TABLE_ROOTS_V1 {
                let id: Hash64 = borsh::from_slice(key).map_err(|_| bad("EXEC actual root key"))?;
                let previous = old.as_ref().map(|v| decode::<Root>(v)).transpose()?;
                let next = new.as_ref().map(|v| decode::<Root>(v)).transpose()?;
                match (previous, next) {
                    (None, Some(row)) => {
                        if matches!(row.phase, Phase::Settled { .. }) {
                            return Err(bad("EXEC settled root lacks actual prior session"));
                        }
                        if row.opened_daa != point.daa_score {
                            return Err(bad("EXEC original root differs from actual admission clock"));
                        }
                        let source = ensure_source(b, id, &row, Some(*point), *point, at)?;
                        let original = literal(b, &*row)?;
                        birth(b, source, Work::ExecSession { root: id }, *point, at, original)?;
                    }
                    (Some(_), None) => release_source(b, id)?,
                    (Some(previous), Some(next)) => {
                        if physical(id, &previous)? != physical(id, &next)? {
                            return Err(bad("EXEC physical root immutable identity changed"));
                        }
                        if matches!(next.phase, Phase::Settled { .. }) && !matches!(previous.phase, Phase::Settled { .. }) {
                            let source = ensure_source(b, id, &previous, None, *point, at)?;
                            settle(b, parent, index, source, *point, at, &previous, &next)?;
                        }
                    }
                    (None, None) => return Err(bad("empty actual EXEC root write")),
                }
            } else if *table == PALW_EXEC_V2_TABLE_SLICES_V1 && old.is_none() {
                let Some(raw) = new else { continue };
                let (id, slice_index): (Hash64, u32) = borsh::from_slice(key).map_err(|_| bad("EXEC actual slice key"))?;
                let row = decode::<Slice>(raw)?;
                if row.accepted_daa != point.daa_score {
                    return Err(bad("EXEC slice differs from actual admission clock"));
                }
                let root = root_at(b, parent, index, id)?.ok_or(bad("EXEC slice lacks actual active root"))?;
                let source = ensure_source(b, id, &root, None, *point, at)?;
                let original = literal(b, &*row)?;
                birth(b, source, Work::ExecSlice { root: id, index: slice_index }, *point, at, original)?;
            }
        }
        Ok(())
    })
    .map_err(err)
}
fn body<T: BorshDeserialize + NativeWireScanV1>(s: &PalwChainStateV2, v: RowValueV2) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    let raw = StateNodesV2(s).body(v)?;
    decode(&raw)
}
fn borrowed_body(s: &PalwChainStateV2, v: RowValueV2) -> Result<&Vec<u8>, ContextErrorV2> {
    let _checked = StateNodesV2(s).body(v)?;
    let mut key = [0; 65];
    key[0] = b'B';
    key[1..].copy_from_slice(v.data.as_byte_slice());
    let _lease = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
    s.kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())))
        .ok_or(bad("EXEC typed archive body disappeared"))
}
fn check_source(s: &PalwChainStateV2, row: &Source) -> Result<(), ContextErrorV2> {
    let native = s.exec_v2.roots.get(&row.root).ok_or(bad("EXEC source lacks actual native root"))?;
    if row.version != 10 || row.identity != physical(row.root, native)? || row.scope != source_scope(row)? {
        return Err(bad("EXEC scope differs from actual native source"));
    }
    if row.captured_at.daa_score < native.opened_daa
        || row.birth.is_some_and(|p| p != row.captured_at || p.daa_score != native.opened_daa)
    {
        return Err(bad("EXEC actual source birth point"));
    }
    Ok(())
}
fn slice_identity(row: &Slice) -> Result<Hash64, ContextErrorV2> {
    hash(
        5,
        &(
            row.slice_id,
            row.range,
            row.executor,
            row.carrier,
            row.accepted_daa,
            row.predecessor_state_root,
            row.result_state_root,
            row.evidence_root,
            row.da_root,
        ),
    )
}
fn check_observed_order(source: &Source, point: PalwBlockContextV2, ordinal: u64) -> Result<(), ContextErrorV2> {
    if point.daa_score < source.captured_at.daa_score
        || point.blue_score < source.captured_at.blue_score
        || (point == source.captured_at && ordinal < source.captured_ordinal)
    {
        return Err(bad("EXEC record precedes actual physical source observation"));
    }
    Ok(())
}
fn check_born(s: &PalwChainStateV2, source: &Source, row: &Born) -> Result<(), ContextErrorV2> {
    check_observed_order(source, row.point, row.ordinal)?;
    if row.version != 10 || row.scope != source.scope || work_root(row.work)? != source.root {
        return Err(bad("EXEC birth owner or actual clock"));
    }
    if source.birth.is_some_and(|point| row.point.daa_score < point.daa_score || row.point.blue_score < point.blue_score) {
        return Err(bad("EXEC subject precedes actual root birth"));
    }
    match row.work {
        Work::ExecSession { .. } => {
            let original = body::<Root>(s, row.original)?;
            if source.birth != Some(row.point)
                || source.captured_ordinal != row.ordinal
                || physical(source.root, &original)? != source.identity
                || original.opened_daa != row.point.daa_score
            {
                return Err(bad("EXEC original root birth differs"));
            }
        }
        Work::ExecSlice { .. } => {
            let original = body::<Slice>(s, row.original)?;
            if original.accepted_daa != row.point.daa_score {
                return Err(bad("EXEC original slice birth differs"));
            }
            if let Work::ExecSlice { root, index } = row.work
                && let Some(current) = s.exec_v2.slices.get(&(root, index))
                && slice_identity(current)? != slice_identity(&original)?
            {
                return Err(bad("EXEC current slice differs from original physical instance"));
            }
        }
        _ => return Err(bad("EXEC birth kind differs")),
    }
    Ok(())
}
fn event_scope(row: &Settled) -> Result<Hash64, ContextErrorV2> {
    hash(4, &(row.scope, row.work, row.ordinal))
}
fn check_event(s: &PalwChainStateV2, source: &Source, row: &Settled) -> Result<(), ContextErrorV2> {
    check_observed_order(source, row.point, row.ordinal)?;
    if source.birth == Some(row.point) && row.ordinal <= source.captured_ordinal {
        return Err(bad("EXEC settlement is not after actual root admission"));
    }
    if row.version != 10
        || row.scope != source.scope
        || row.work != (Work::ExecSettlement { root: source.root, block: row.point.block })
        || row.consumed.spec != spec(CONSUMED)
    {
        return Err(bad("EXEC settlement source/scope differs"));
    }
    if source.birth.is_some_and(|point| row.point.daa_score < point.daa_score || row.point.blue_score < point.blue_score) {
        return Err(bad("EXEC settlement precedes actual root birth"));
    }
    let before = body::<Root>(s, row.before)?;
    let after = body::<Root>(s, row.after)?;
    if physical(source.root, &before)? != source.identity
        || physical(source.root, &after)? != source.identity
        || matches!(before.phase, Phase::Settled { .. })
        || after.phase != (Phase::Settled { settled_daa: row.point.daa_score })
        || row.consumed.rows != after.next_index as u64
    {
        return Err(bad("EXEC settlement original native transition differs"));
    }
    if row.point.daa_score < before.opened_daa {
        return Err(bad("EXEC settlement actual clock differs"));
    }
    let scope = event_scope(row)?;
    if root(s, CONSUMED, Some(scope))? != row.consumed {
        return Err(bad("EXEC event lost source-owned slice component"));
    }
    let reader = StateNodesV2(s);
    let mut next = 0u32;
    nodes::complete_rows(&reader, row.consumed, |key, v| {
        if slice_index(key)? != next {
            return Err(bad("EXEC settlement slice indices not complete"));
        }
        next = next.checked_add(1).ok_or(bad("EXEC consumed slice count overflow"))?;
        let slice = body::<Slice>(s, v)?;
        if slice.accepted_daa > row.point.daa_score {
            return Err(bad("EXEC settlement consumes future native slice"));
        }
        Ok(())
    })?;
    Ok(())
}
pub(super) struct ExecBirthViewV10 {
    row: NativeRetainedV1<Born>,
}
impl ExecBirthViewV10 {
    pub(super) fn work(&self) -> Work {
        self.row.work
    }
    pub(super) fn point(&self) -> PalwBlockContextV2 {
        self.row.point
    }
    pub(super) fn journal_ordinal(&self) -> u64 {
        self.row.ordinal
    }
}
/// The raw body envelope is typed Born, NOT a liability frame. Only actual
/// physical current sources are visited; original bytes remain shared B rows.
pub(super) fn visit_births_v10<'a>(
    s: &'a PalwChainStateV2,
    mut f: impl FnMut(ExecBirthViewV10, &'a Vec<u8>) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, root(s, SOURCES, None).map_err(err)?, |key, v| {
        let source = body::<Source>(s, v)?;
        if key != source.root.as_byte_slice() {
            return Err(bad("EXEC source directory key"));
        }
        check_source(s, &source)?;
        nodes::complete_rows(&reader, root(s, BIRTHS, Some(source.scope))?, |key, v| {
            let row = body::<Born>(s, v)?;
            if key != work_key(row.work)?.as_byte_slice() {
                return Err(bad("EXEC birth work key"));
            }
            check_born(s, &source, &row)?;
            if let Work::ExecSlice { root, index } = row.work {
                let original = body::<Slice>(s, row.original)?;
                if s.exec_v2
                    .slices
                    .get(&(root, index))
                    .is_none_or(|current| current.slice_id != original.slice_id || current.accepted_daa != original.accepted_daa)
                {
                    return Ok(());
                }
            }
            let raw = borrowed_body(s, v)?;
            f(ExecBirthViewV10 { row }, raw).map_err(native_err)
        })
    })
    .map_err(err)
}
/// Exact event input set is built from the source-owned consumed component,
/// not from today's root.next_index or a cloned global EXEC universe.
pub(super) fn visit_settlement_events_v10<'a>(
    s: &'a PalwChainStateV2,
    mut f: impl FnMut(NativeRetainedV1<PalwClaimLocalNativeEventV1>, &'a Vec<u8>) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, root(s, SOURCES, None).map_err(err)?, |_, v| {
        let source = body::<Source>(s, v)?;
        check_source(s, &source)?;
        nodes::complete_rows(&reader, root(s, EVENTS, Some(source.scope))?, |key, v| {
            let row = body::<Settled>(s, v)?;
            if key != work_key(row.work)?.as_byte_slice() {
                return Err(bad("EXEC event work key"));
            }
            check_event(s, &source, &row)?;
            let mut q = TirNativeRequestsV1::new();
            q.exact_v1::<BTreeNodeEnvelopeV1<Work, ()>>(
                row.consumed.rows.checked_add(1).ok_or(bad("EXEC event input count overflow"))?,
            )
            .map_err(ContextErrorV2::Resource)?;
            let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
            let mut inputs = BTreeSet::new();
            inputs.insert(Work::ExecSession { root: source.root });
            nodes::complete_rows(&reader, row.consumed, |key, _| {
                inputs.insert(Work::ExecSlice { root: source.root, index: slice_index(key)? });
                Ok(())
            })?;
            let event = PalwClaimLocalNativeEventV1 { work: row.work, point: row.point, journal_ordinal: row.ordinal, inputs };
            f(NativeRetainedV1::from_owner_reserved_v1(event, lease), borrowed_body(s, v)?).map_err(native_err)
        })
    })
    .map_err(err)
}
/// Source-only status/event material. Explicit source type binds provenance;
/// none of these bytes claim to be a complete chain state or Court archive.
pub(super) fn visit_material_v10(
    s: &PalwChainStateV2,
    work: Work,
    mut f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let id = work_root(work).map_err(err)?;
    let Some(source) = source(s, id).map_err(err)? else {
        return Ok(false);
    };
    check_source(s, &source).map_err(err)?;
    match work {
        Work::ExecSession { .. } | Work::ExecSlice { .. } => {
            let Some(row) = lookup::<Born>(s, BIRTHS, Some(source.scope), work_key(work).map_err(err)?).map_err(err)? else {
                return Ok(false);
            };
            check_born(s, &source, &row).map_err(err)?;
            let source_proof = encode(&*source).map_err(err)?;
            f(&source_proof)?;
            let proof = encode(&*row).map_err(err)?;
            f(&proof)?;
            let original = StateNodesV2(s).body(row.original).map_err(err)?;
            f(&original)?;
            match work {
                Work::ExecSession { root } => {
                    let raw = encode(s.exec_v2.roots.get(&root).ok_or_else(|| err(bad("EXEC current root material missing")))?)
                        .map_err(err)?;
                    f(&raw)?;
                }
                Work::ExecSlice { root, index } => {
                    let raw =
                        encode(s.exec_v2.slices.get(&(root, index)).ok_or_else(|| err(bad("EXEC current slice material missing")))?)
                            .map_err(err)?;
                    f(&raw)?;
                }
                _ => unreachable!(),
            }
        }
        Work::ExecSettlement { .. } => {
            let Some(row) = lookup::<Settled>(s, EVENTS, Some(source.scope), work_key(work).map_err(err)?).map_err(err)? else {
                return Ok(false);
            };
            check_event(s, &source, &row).map_err(err)?;
            let source_proof = encode(&*source).map_err(err)?;
            f(&source_proof)?;
            let proof = encode(&*row).map_err(err)?;
            f(&proof)?;
            for v in [row.before, row.after] {
                let raw = StateNodesV2(s).body(v).map_err(err)?;
                f(&raw)?;
            }
            let reader = StateNodesV2(s);
            nodes::complete_rows(&reader, row.consumed, |_, v| {
                let raw = reader.body(v)?;
                f(&raw).map_err(native_err)
            })
            .map_err(err)?;
        }
        _ => return Err(err(bad("non EXEC material kind"))),
    }
    Ok(true)
}
fn check_tip(s: &PalwChainStateV2, p: PalwBlockContextV2) -> Result<(), ContextErrorV2> {
    if s.last_point.is_none_or(|tip| p.daa_score > tip.daa_score || p.blue_score > tip.blue_score) {
        return Err(bad("EXEC source point beyond restored tip"));
    }
    Ok(())
}
fn verify_inventory(
    s: &PalwChainStateV2,
    row: RowValueV2,
    extra: &[RowValueV2],
    component: Option<ComponentRootV2>,
) -> Result<(), ContextErrorV2> {
    let a = inventory(row, extra, component)?;
    let raw = encode(&*a)?;
    let key = akey(value(&raw).data);
    let _key = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
    let stored = s
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())))
        .ok_or(bad("EXEC source archive inventory missing"))?;
    crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&*a, stored)
        .map_err(|_| bad("EXEC source archive inventory differs"))
}
/// Global cold-restore/integrity boundary only. All owning scopes/archives and
/// native physical identities are checked; no dangling owned root can survive
/// actual source retirement, and an observed legacy source grants no birth.
pub(super) fn validate_exec_archive_v10(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, root(s, SOURCES, None)?, |key, v| {
        let source = body::<Source>(s, v)?;
        if key != source.root.as_byte_slice() {
            return Err(bad("EXEC source directory key"));
        }
        check_source(s, &source)?;
        check_tip(s, source.captured_at)?;
        if let Some(point) = source.birth {
            check_tip(s, point)?;
        }
        if lookup::<Hash64>(s, BY_SCOPE, None, source.scope)?.as_deref() != Some(&source.root) {
            return Err(bad("EXEC source reverse scope absent"));
        }
        if source.birth.is_some()
            && lookup::<Born>(s, BIRTHS, Some(source.scope), work_key(Work::ExecSession { root: source.root })?)?.is_none()
        {
            return Err(bad("EXEC actual root birth missing"));
        }
        nodes::complete_rows(&reader, root(s, BIRTHS, Some(source.scope))?, |key, v| {
            let row = body::<Born>(s, v)?;
            if key != work_key(row.work)?.as_byte_slice() {
                return Err(bad("EXEC birth work identity"));
            }
            check_born(s, &source, &row)?;
            check_tip(s, row.point)?;
            verify_inventory(s, v, &[row.original], None)
        })?;
        nodes::complete_rows(&reader, root(s, EVENTS, Some(source.scope))?, |key, v| {
            let row = body::<Settled>(s, v)?;
            if key != work_key(row.work)?.as_byte_slice() {
                return Err(bad("EXEC event work identity"));
            }
            check_event(s, &source, &row)?;
            check_tip(s, row.point)?;
            if lookup::<(Hash64, Work)>(s, EVENT_OWNER, None, event_scope(&row)?)?.as_deref() != Some(&(source.root, row.work)) {
                return Err(bad("EXEC event reverse owner absent"));
            }
            verify_inventory(s, v, &[row.before, row.after], Some(row.consumed))
        })?;
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, BY_SCOPE, None)?, |key, v| {
        let id = body::<Hash64>(s, v)?;
        let source = source(s, *id)?.ok_or(bad("EXEC reverse scope orphan"))?;
        if key != source.scope.as_byte_slice() {
            return Err(bad("EXEC reverse scope differs"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, EVENT_OWNER, None)?, |key, v| {
        let owner = body::<(Hash64, Work)>(s, v)?;
        let source = source(s, owner.0)?.ok_or(bad("EXEC reverse event source orphan"))?;
        let row = lookup::<Settled>(s, EVENTS, Some(source.scope), work_key(owner.1)?)?.ok_or(bad("EXEC reverse event missing"))?;
        if key != event_scope(&row)?.as_byte_slice() {
            return Err(bad("EXEC reverse event scope differs"));
        }
        Ok(())
    })?;
    if root(s, SCRATCH, None)?.rows != 0 {
        return Err(bad("EXEC transient literal index retained"));
    }
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'I') {
                continue;
            }
            let r = decode::<ComponentRootV2>(raw)?;
            if r.spec.owner != 14 {
                continue;
            }
            if r.spec != spec(r.spec.table) {
                return Err(bad("EXEC component spec substituted"));
            }
            match r.spec.table {
                SOURCES | SCRATCH | BY_SCOPE | EVENT_OWNER if key.len() == 18 => {}
                BIRTHS | EVENTS if key.len() == 84 => {
                    let scope = Hash64::from_bytes(key[3..67].try_into().map_err(|_| bad("EXEC component owner scope"))?);
                    if lookup::<Hash64>(s, BY_SCOPE, None, scope)?.is_none() {
                        return Err(bad("EXEC owned component orphan"));
                    }
                }
                CONSUMED if key.len() == 84 => {
                    let scope = Hash64::from_bytes(key[3..67].try_into().map_err(|_| bad("EXEC component event scope"))?);
                    if lookup::<(Hash64, Work)>(s, EVENT_OWNER, None, scope)?.is_none() {
                        return Err(bad("EXEC consumed component orphan"));
                    }
                }
                _ => return Err(bad("unallocated EXEC current component namespace")),
            }
        }
    }
    Ok(())
}
/// Retention-accounting owner can inspect only this physical source's actual
/// immutable B refs/component roots. This never invents a fee, payer or paid
/// bit; existing signed native retention geometry must authorize these bytes.
pub(super) fn visit_source_retention_inventory_v10(
    s: &PalwChainStateV2,
    id: Hash64,
    mut f: impl FnMut(Option<ComponentRootV2>, &[RowValueV2]) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let Some(source) = source(s, id).map_err(err)? else {
        return Ok(false);
    };
    check_source(s, &source).map_err(err)?;
    let reader = StateNodesV2(s);
    // Only the two exact global directory leaves, NEVER the global root graph.
    let directory = nodes::lookup(&reader, root(s, SOURCES, None).map_err(err)?, id.as_byte_slice())
        .map_err(err)?
        .ok_or_else(|| err(bad("EXEC source retention directory missing")))?;
    let reverse = nodes::lookup(&reader, root(s, BY_SCOPE, None).map_err(err)?, source.scope.as_byte_slice())
        .map_err(err)?
        .ok_or_else(|| err(bad("EXEC source retention reverse missing")))?;
    f(None, &[directory, reverse])?;
    for table in [BIRTHS, EVENTS] {
        let component = root(s, table, Some(source.scope)).map_err(err)?;
        f(Some(component), &[])?;
        nodes::complete_rows(&reader, component, |_, v| {
            if table == BIRTHS {
                let row = body::<Born>(s, v)?;
                f(None, &[v, row.original]).map_err(native_err)
            } else {
                let row = body::<Settled>(s, v)?;
                let scope = event_scope(&row)?;
                let reverse = nodes::lookup(&reader, root(s, EVENT_OWNER, None)?, scope.as_byte_slice())?
                    .ok_or(bad("EXEC event retention owner missing"))?;
                f(Some(row.consumed), &[v, row.before, row.after, reverse]).map_err(native_err)
            }
        })
        .map_err(err)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_work_slice_v2::{PalwWorkPrefixStageV2 as Prefix, PalwWorkSliceStageV2 as Stage};
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn point(n: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h(100 + n), daa_score: n, blue_score: n, subsidy: 77 }
    }
    fn root_row() -> Root {
        Root {
            class_id: h(10),
            canonical_job_id: h(11),
            job_work_id: h(12),
            kernel_version: 1,
            plan_root: h(13),
            total_work: 2,
            boundaries: vec![1, 2],
            initial_state_root: h(14),
            evidence_policy_root: h(15),
            root_bond: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(16), 0)),
            extra_executors: vec![],
            executor_exposure: 0,
            anchor: point(2).block,
            opened_daa: 2,
            expiry_daa: 100,
            next_index: 0,
            last_state_root: h(14),
            accepted_work: 0,
            verified_work: 0,
            pending: 0,
            phase: Phase::Open,
            prefix_claim: ZERO_HASH64,
            prefix: Prefix::Unbound,
        }
    }
    fn slice_row() -> Slice {
        Slice {
            slice_id: h(20),
            range: crate::palw_exec_v2::PalwWorkRangeV1 { start: 1, end: 2 },
            executor: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(16), 0)),
            carrier: h(21),
            accepted_daa: 2,
            predecessor_state_root: h(14),
            result_state_root: h(22),
            evidence_root: h(23),
            da_root: h(24),
            stage: Stage::Verified { verified_daa: 2, leg_cap: 4, route_reward: 0 },
        }
    }
    fn fixture() -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1) {
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_claim_local_settlement_from_daa(Some(0));
        let mut state = PalwChainStateV2::genesis();
        state.last_point = Some(point(1));
        state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(1), h(2)),
            None,
            misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
        ));
        let extras = PalwTransitionExtrasV1 {
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(1),
                ruleset_digest: h(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        (state, params, extras)
    }
    #[test]
    fn rfc23s2_exec_scoped_actual_ordered_writer_no_f_cold_and_inverse() {
        // Actual native writer/lifecycle provenance seam, not signed EXEC admission
        // or selected-approval conformance. No fullframe is injected or synthesized.
        let (state, params, extras) = fixture();
        let id = h(30);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_exec_job(h(12), Some(id)).unwrap(); // real native ordinal0, not birth
        let original = root_row();
        b.write_exec_root(id, Some(original.clone())).unwrap();
        let slice = slice_row();
        b.write_exec_slice((id, 0), Some(slice.clone())).unwrap();
        let mut settled = original.clone();
        settled.next_index = 1;
        settled.accepted_work = 1;
        settled.verified_work = 1;
        settled.phase = Phase::Settled { settled_daa: 2 };
        b.write_exec_root(id, Some(settled.clone())).unwrap();
        // A later actual journal update cannot overwrite the slice that the earlier
        // settlement consumed, even though it is today's current native slice.
        let mut later = slice.clone();
        later.stage = Stage::ProvenFalse { daa: 2 };
        b.write_exec_slice((id, 0), Some(later)).unwrap();
        let mut voided = settled.clone();
        voided.phase = Phase::Voided { from_index: 0, voided_daa: 2 };
        b.write_exec_root(id, Some(voided)).unwrap();
        capture_executed_exec_v10(&mut b, &state, &point(2)).unwrap();
        b.state.last_point = Some(point(2));
        validate_exec_archive_v10(&b.state).unwrap();
        // Every actual birth/progress/settlement event remains represented by
        // its exact scoped owner, even when a later native write changes the
        // current phase in this same block. No projected F1 is constructed.
        let mut ordinal = 0u64;
        let mut checked = 0usize;
        for entry in &b.entries {
            if let PalwDeltaEntryV2::ExecV2Row { table, key, old, new } = entry {
                assert!(covers_native_event_v21(&b.state, point(2), ordinal, *table, key, old.as_deref(), new.as_deref()).unwrap());
                if old.is_none() {
                    assert!(!covers_native_event_v21(&b.state, point(3), ordinal, *table, key, None, new.as_deref()).unwrap());
                    assert!(!covers_native_event_v21(&b.state, point(2), ordinal + 1, *table, key, None, new.as_deref()).unwrap());
                }
                checked += 1;
            }
            if matches!(entry, PalwDeltaEntryV2::Claim { .. } | PalwDeltaEntryV2::ExecV2Row { .. })
                || matches!(entry,PalwDeltaEntryV2::KernelRouteRow{table,..}if *table!=PALW_CLAIM_LOCAL_TABLE_V1)
            {
                ordinal += 1;
            }
        }
        assert_eq!(checked, 5);
        b.claim_local_lock_point_v6 = Some(point(2));
        let audit_root = b.state.state_root();
        let audit_journal = b.entries.len();
        let audit = super::super::palw_claim_local_native_event_coverage_v21::audit_captured_native_events_v21(&b, &point(2)).unwrap();
        assert_eq!(audit.point_v21(), point(2));
        assert_eq!(audit.counts_v21(), (6, 6));
        assert_eq!(
            audit.first_gap_v21(),
            None,
            "actual captured EXEC events have exact scoped owners; the audit still exposes no lifecycle/skip authority"
        );
        let tail = audit.tail_boundary_v21().unwrap();
        drop(audit);
        tail.verify_suffix_v21(&b, &point(2)).unwrap();
        assert!(matches!(tail.verify_suffix_v21(&b, &point(3)), Err(PalwStateV2Error::NativeContextUnavailable(_))));
        assert!(super::super::palw_claim_local_native_event_coverage_v21::audit_captured_native_events_v21(&b, &point(3)).is_err());
        assert_eq!(b.state.state_root(), audit_root);
        assert_eq!(b.entries.len(), audit_journal);
        assert_eq!(super::super::palw_claim_local_rewards_v1::frames(&b.state).count(), 0);
        let mut births = BTreeMap::new();
        visit_births_v10(&b.state, |view, _| {
            births.insert(view.work(), (view.point(), view.journal_ordinal()));
            Ok(())
        })
        .unwrap();
        assert_eq!(births[&Work::ExecSession { root: id }], (point(2), 1));
        assert_eq!(births[&Work::ExecSlice { root: id, index: 0 }], (point(2), 2));
        let mut events = 0;
        visit_settlement_events_v10(&b.state, |event, _| {
            events += 1;
            assert_eq!(event.journal_ordinal, 3);
            assert_eq!(event.inputs, BTreeSet::from([Work::ExecSession { root: id }, Work::ExecSlice { root: id, index: 0 }]));
            Ok(())
        })
        .unwrap();
        assert_eq!(events, 1);
        let source_record = source(&b.state, id).unwrap().unwrap();
        let event = lookup::<Settled>(
            &b.state,
            EVENTS,
            Some(source_record.scope),
            work_key(Work::ExecSettlement { root: id, block: point(2).block }).unwrap(),
        )
        .unwrap()
        .unwrap();
        let reader = StateNodesV2(&b.state);
        nodes::complete_rows(&reader, event.consumed, |_, v| {
            assert_eq!(body::<Slice>(&b.state, v)?.stage, slice.stage);
            Ok(())
        })
        .unwrap();
        let original_state_root = b.state.state_root();
        let encoded = borsh::to_vec(b.state.kernel_route().unwrap()).unwrap();
        let mut cold = b.state.clone();
        cold.kernel_route = Some(borsh::from_slice(&encoded).unwrap());
        validate_exec_archive_v10(&cold).unwrap();
        assert_eq!(cold.state_root(), original_state_root);
        let prior = cold.clone();
        let mut d = TransitionBuilder::new(&prior, &params, false, false, false, false, &extras);
        d.write_exec_slice((id, 0), None).unwrap();
        d.write_exec_job(h(12), None).unwrap();
        d.write_exec_root(id, None).unwrap();
        capture_executed_exec_v10(&mut d, &prior, &point(3)).unwrap();
        d.state.last_point = Some(point(3));
        validate_exec_archive_v10(&d.state).unwrap();
        assert!(source(&d.state, id).unwrap().is_none());
        assert!(
            !covers_native_event_v21(&d.state, point(3), 2, PALW_EXEC_V2_TABLE_ROOTS_V1, id.as_byte_slice(), None, None).unwrap(),
            "physical removal cannot borrow the vanished current archive"
        );
        d.claim_local_lock_point_v6 = Some(point(3));
        let removal =
            super::super::palw_claim_local_native_event_coverage_v21::audit_captured_native_events_v21(&d, &point(3)).unwrap();
        assert_eq!(removal.counts_v21(), (3, 1));
        assert_eq!(
            removal.first_gap_v21(),
            Some((0, super::super::palw_claim_local_native_event_coverage_v21::NativeCoverageGapV21::ExecLifecycle)),
            "removed scopes cannot become covered from final absence"
        );
        assert!(
            matches!(removal.tail_boundary_v21(), Err(PalwStateV2Error::NativeContextUnavailable(_))),
            "actual uncovered native removals cannot mint even a suffix boundary"
        );
        drop(removal);
        assert_eq!(root(&d.state, EVENT_OWNER, None).unwrap().rows, 0);
        // Move auxiliary/native rows back with their ORIGINAL owning replay helpers.
        for e in d.entries.iter().rev() {
            match e {
                PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } => {
                    super::super::apply_kernel_route_row_v1(&mut d.state, *table, key, old, new, true).unwrap()
                }
                PalwDeltaEntryV2::ExecV2Row { table, key, old, new } => {
                    super::super::palw_exec_v2_fold::apply_exec_v2_row_v1(&mut d.state, *table, key, old, new, true).unwrap()
                }
                _ => panic!("native/aux journal only"),
            }
        }
        d.state.last_point = prior.last_point;
        assert_eq!(d.state.state_root(), original_state_root);
        validate_exec_archive_v10(&d.state).unwrap();
    }
    #[test]
    fn rfc23s2_exec_missing_settlement_slice_rolls_back_only_new_metadata() {
        let (state, params, extras) = fixture();
        let id = h(30);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let original = root_row();
        b.write_exec_root(id, Some(original.clone())).unwrap();
        let mut settled = original;
        settled.next_index = 1;
        settled.phase = Phase::Settled { settled_daa: 2 };
        b.write_exec_root(id, Some(settled)).unwrap();
        let before = b.state.state_root();
        let entries = b.entries.len();
        assert!(capture_executed_exec_v10(&mut b, &state, &point(2)).is_err());
        assert_eq!(b.entries.len(), entries);
        assert_eq!(b.state.state_root(), before);
    }
}
/// Narrow coverage checks let the checkpoint owner bypass historical full-EXEC
/// replay only for an EXACT source event already authenticated by this archive.
pub(super) fn has_birth_v10(s: &PalwChainStateV2, work: Work, point: PalwBlockContextV2) -> Result<bool, PalwStateV2Error> {
    let id = work_root(work).map_err(err)?;
    let Some(source) = source(s, id).map_err(err)? else {
        return Ok(false);
    };
    check_source(s, &source).map_err(err)?;
    let Some(row) = lookup::<Born>(s, BIRTHS, Some(source.scope), work_key(work).map_err(err)?).map_err(err)? else {
        return Ok(false);
    };
    check_born(s, &source, &row).map_err(err)?;
    Ok(row.work == work && row.point == point)
}
pub(super) fn has_settlement_v10(
    s: &PalwChainStateV2,
    work: Work,
    point: PalwBlockContextV2,
    ordinal: u64,
) -> Result<bool, PalwStateV2Error> {
    let id = work_root(work).map_err(err)?;
    let Some(source) = source(s, id).map_err(err)? else {
        return Ok(false);
    };
    check_source(s, &source).map_err(err)?;
    let Some(row) = lookup::<Settled>(s, EVENTS, Some(source.scope), work_key(work).map_err(err)?).map_err(err)? else {
        return Ok(false);
    };
    check_event(s, &source, &row).map_err(err)?;
    Ok(row.work == work && row.point == point && row.ordinal == ordinal)
}
/// Reads only the committed native event prefix. False means all eligible EXEC
/// birth/settlement facts in this frame have exact scoped counterparts; OTHER
/// owners still require their own coverage decision. Missing F1 returns None.
pub(super) fn legacy_exec_needed_v10(s: &PalwChainStateV2, block: Hash64) -> Result<Option<bool>, PalwStateV2Error> {
    use crate::palw_claim_local_v1::PalwClaimLocalRowOwnerV1 as Owner;
    let mut needed = false;
    let present =
        super::palw_claim_local_native_frame_events_v10::visit_rooted_native_events_v10(s, block, |point, ordinal, event| {
            if event.owner != Owner::ExecV2 {
                return Ok(());
            }
            if event.table == PALW_EXEC_V2_TABLE_ROOTS_V1 {
                let id: Hash64 = borsh::from_slice(event.key.bytes).map_err(|_| err(bad("EXEC frame root key")))?;
                let Some(raw) = event.new else { return Ok(()) };
                let next = decode::<Root>(raw.bytes).map_err(err)?;
                let Some(current) = s.exec_v2.roots.get(&id) else {
                    return Ok(());
                };
                if physical(id, current).map_err(err)? != physical(id, &next).map_err(err)? {
                    return Ok(());
                }
                let actual_birth = current_birth_event_v10(s, Work::ExecSession { root: id })?;
                if let Some((birth, born_ordinal)) = actual_birth {
                    // A current exact-born source supersedes an older raw-coincident source,
                    // even when their DAA/anchor/plan are equal. No newer raw row is guessed.
                    if point.blue_score < birth.blue_score || (point == birth && ordinal < born_ordinal) {
                        return Ok(());
                    }
                    if event.old.is_none() && (point, ordinal) != (birth, born_ordinal) {
                        return Ok(());
                    }
                } else if event.old.is_none() {
                    needed = true
                }
                if matches!(next.phase, Phase::Settled { .. }) {
                    let previous = event.old.map(|v| decode::<Root>(v.bytes).map_err(err)).transpose()?;
                    if previous.as_ref().is_none_or(|old| !matches!(old.phase, Phase::Settled { .. }))
                        && !has_settlement_v10(s, Work::ExecSettlement { root: id, block: point.block }, point, ordinal)?
                    {
                        needed = true
                    }
                }
            } else if event.table == PALW_EXEC_V2_TABLE_SLICES_V1 && event.old.is_none() {
                let Some(raw) = event.new else { return Ok(()) };
                let (id, index): (Hash64, u32) = borsh::from_slice(event.key.bytes).map_err(|_| err(bad("EXEC frame slice key")))?;
                let next = decode::<Slice>(raw.bytes).map_err(err)?;
                if s.exec_v2
                    .slices
                    .get(&(id, index))
                    .is_some_and(|current| current.slice_id == next.slice_id && current.accepted_daa == next.accepted_daa)
                    && current_birth_event_v10(s, Work::ExecSlice { root: id, index })?.is_none()
                {
                    needed = true
                }
            }
            Ok(())
        })?;
    Ok(present.then_some(needed))
}
/// Used only while enumerating real native F1 rows. Missing enumerated material
/// is a storage fault, never a fabricated complete/scoped result.
pub(super) fn legacy_exec_frame_needed_v10(s: &PalwChainStateV2, block: Hash64) -> Result<bool, PalwStateV2Error> {
    legacy_exec_needed_v10(s, block)?.ok_or_else(|| err(ContextErrorV2::Unavailable("enumerated native EXEC frame disappeared")))
}
/// Exact current admitted event including ordinal. An observation of a legacy
/// physical source is deliberately not returned as a root/session birth.
pub(super) fn current_birth_event_v10(
    s: &PalwChainStateV2,
    work: Work,
) -> Result<Option<(PalwBlockContextV2, u64)>, PalwStateV2Error> {
    let id = work_root(work).map_err(err)?;
    let Some(source) = source(s, id).map_err(err)? else {
        return Ok(None);
    };
    check_source(s, &source).map_err(err)?;
    let Some(row) = lookup::<Born>(s, BIRTHS, Some(source.scope), work_key(work).map_err(err)?).map_err(err)? else {
        return Ok(None);
    };
    check_born(s, &source, &row).map_err(err)?;
    Ok(Some((row.point, row.ordinal)))
}

/// Exact last native event coverage for the separate v21 partition. A scoped
/// birth/material getter is required; source presence alone is insufficient.
/// Physical removals deliberately remain unsupported until their own complete
/// retired checkpoint owner is integrated. No F1 path is suppressed here.
pub(super) fn covers_native_event_v21(
    s: &PalwChainStateV2,
    point: PalwBlockContextV2,
    ordinal: u64,
    table: u8,
    key: &[u8],
    old: Option<&[u8]>,
    new: Option<&[u8]>,
) -> Result<bool, PalwStateV2Error> {
    let Some(raw) = new else { return Ok(false) };
    let work = if table == PALW_EXEC_V2_TABLE_ROOTS_V1 {
        let id: Hash64 = borsh::from_slice(key).map_err(|_| err(bad("coverage EXEC root key")))?;
        let next = decode::<Root>(raw).map_err(err)?;
        let Some(current) = s.exec_v2.roots.get(&id) else { return Ok(false) };
        if physical(id, current).map_err(err)? != physical(id, &next).map_err(err)? {
            return Ok(false);
        }
        if let Some(old_raw) = old {
            let prior = decode::<Root>(old_raw).map_err(err)?;
            if physical(id, &prior).map_err(err)? != physical(id, &next).map_err(err)? {
                return Ok(false);
            }
            if matches!(next.phase, Phase::Settled { .. }) && !matches!(prior.phase, Phase::Settled { .. }) {
                let Some(src) = source(s, id).map_err(err)? else { return Ok(false) };
                let work = Work::ExecSettlement { root: id, block: point.block };
                let Some(event) = lookup::<Settled>(s, EVENTS, Some(src.scope), work_key(work).map_err(err)?).map_err(err)? else {
                    return Ok(false);
                };
                check_event(s, &src, &event).map_err(err)?;
                if event.point != point || event.ordinal != ordinal || event.before != value(old_raw) || event.after != value(raw) {
                    return Ok(false);
                }
            }
        }
        Work::ExecSession { root: id }
    } else if table == PALW_EXEC_V2_TABLE_SLICES_V1 {
        let (root, index): (Hash64, u32) = borsh::from_slice(key).map_err(|_| err(bad("coverage EXEC slice key")))?;
        let next = decode::<Slice>(raw).map_err(err)?;
        let Some(current) = s.exec_v2.slices.get(&(root, index)) else { return Ok(false) };
        if slice_identity(current).map_err(err)? != slice_identity(&next).map_err(err)? {
            return Ok(false);
        }
        if let Some(raw) = old {
            let prior = decode::<Slice>(raw).map_err(err)?;
            if slice_identity(&prior).map_err(err)? != slice_identity(&next).map_err(err)? {
                return Ok(false);
            }
        }
        Work::ExecSlice { root, index }
    } else {
        return Ok(false);
    };
    let Some(src) = source(s, work_root(work).map_err(err)?).map_err(err)? else { return Ok(false) };
    let Some(born) = lookup::<Born>(s, BIRTHS, Some(src.scope), work_key(work).map_err(err)?).map_err(err)? else { return Ok(false) };
    check_born(s, &src, &born).map_err(err)?;
    if old.is_none() && (born.point != point || born.ordinal != ordinal || born.original != value(raw)) {
        return Ok(false);
    }
    visit_material_v10(s, work, |_| Ok(()))
}
