//! ISOLATED owner15/schema10. Exact-birth same-execution-root membership only.
//! No fee, Final, approval, complete-world root, or funded capability is minted.
//! A complete source snapshot is published only from a complete native census.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeHeapV1, NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
const ROOTS: u32 = 1;
const MEMBERS: u32 = 2;
const SNAPSHOTS: u32 = 3;
const SCRATCH: u32 = 4;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 15, table, schema: 10, key_bytes: if table == MEMBERS { 144 } else { 64 }, value_bytes: u64::MAX }
}
fn bad(s: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(s)
}
fn err(e: ContextErrorV2) -> PalwStateV2Error {
    super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn native(e: PalwStateV2Error) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(r) => ContextErrorV2::Resource(r),
        _ => bad("cohort native writer refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("cohort body length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut raw = Vec::with_capacity(n);
    v.serialize(&mut raw).map_err(|_| bad("cohort body encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(raw, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("cohort canonical typed body"),
    })
}
fn digest<T: BorshSerialize>(tag: u8, v: &T) -> Result<Hash64, ContextErrorV2> {
    struct W(blake2b_simd::State);
    impl std::io::Write for W {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut w = W(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/execution-cohort/v15").to_state());
    tag.serialize(&mut w).map_err(|_| bad("cohort hash tag"))?;
    v.serialize(&mut w).map_err(|_| bad("cohort hash body"))?;
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
        Some(scope) => r.owned_current(scope, spec(table)),
        None => r.current(spec(table)),
    }
}
fn reference(s: &PalwChainStateV2, table: u32, scope: Option<Hash64>, key: &[u8]) -> Result<Option<RowValueV2>, ContextErrorV2> {
    nodes::lookup(&StateNodesV2(s), root(s, table, scope)?, key)
}
fn body<T: BorshDeserialize + NativeWireScanV1>(s: &PalwChainStateV2, v: RowValueV2) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    let raw = StateNodesV2(s).body(v)?;
    decode(&raw)
}
fn put_raw(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Option<Hash64>,
    key: &[u8],
    raw: Option<&[u8]>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let before = root(&b.state, table, scope)?;
    let r = StateNodesV2(&b.state);
    let old = nodes::lookup(&r, before, key)?;
    let plan = nodes::update(&r, before, key, old, raw.map(value))?;
    if let Some(scope) = scope {
        store::publish_owned_component_v2(b, scope, &plan, raw)?
    } else {
        store::publish_component_v2(b, &plan, raw)?
    }
    Ok(plan.after())
}
fn put<T: BorshSerialize>(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Option<Hash64>,
    key: &[u8],
    v: Option<&T>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let raw = v.map(encode).transpose()?;
    put_raw(b, table, scope, key, raw.as_deref().map(Vec::as_slice))
}
fn literal<T: BorshSerialize>(b: &mut TransitionBuilder<'_>, v: &T) -> Result<RowValueV2, ContextErrorV2> {
    let raw = encode(v)?;
    let rv = value(&raw);
    put_raw(b, SCRATCH, None, Hash64::default().as_byte_slice(), Some(&raw))?;
    put_raw(b, SCRATCH, None, Hash64::default().as_byte_slice(), None)?;
    Ok(rv)
}
fn atomic<T>(
    b: &mut TransitionBuilder<'_>,
    f: impl FnOnce(&mut TransitionBuilder<'_>) -> Result<T, ContextErrorV2>,
) -> Result<T, ContextErrorV2> {
    let first = b.entries.len();
    let gc = b.claim_local_gc_sequence_v3;
    let result = f(b);
    if result.is_err() {
        super::palw_claim_local_context_gc_v3::rollback_aux_v3(b, first, gc, |table, key| {
            table == PALW_CLAIM_LOCAL_TABLE_V1 && matches!(key.first(), Some(b'N' | b'B' | b'I' | b'A'))
        })?
    }
    result
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct StoredBirthV15 {
    block: BlockHash,
    daa: u64,
    blue: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(StoredBirthV15;BlockHash,u64,u64);
impl StoredBirthV15 {
    fn of(c: &PalwClaimStateV2) -> Self {
        Self { block: c.accepted_block, daa: c.accepted_daa, blue: c.accepted_blue_score }
    }
}
fn member_key(id: Hash64, c: &PalwClaimStateV2) -> [u8; 144] {
    let mut key = [0; 144];
    key[..64].copy_from_slice(id.as_byte_slice());
    key[64..128].copy_from_slice(c.accepted_block.as_byte_slice());
    key[128..136].copy_from_slice(&c.accepted_daa.to_le_bytes());
    key[136..].copy_from_slice(&c.accepted_blue_score.to_le_bytes());
    key
}
fn member_id(key: &[u8], c: &PalwClaimStateV2) -> Result<Hash64, ContextErrorV2> {
    if key.len() != 144 {
        return Err(bad("cohort member key width"));
    }
    let id = Hash64::from_bytes(key[..64].try_into().map_err(|_| bad("cohort member id"))?);
    if key != member_key(id, c) {
        return Err(bad("cohort member birth alias"));
    }
    Ok(id)
}
fn same_identity(a: &PalwClaimStateV2, b: &PalwClaimStateV2) -> bool {
    StoredBirthV15::of(a) == StoredBirthV15::of(b)
        && a.execution_root == b.execution_root
        && a.class_id == b.class_id
        && a.bond == b.bond
        && a.output_root == b.output_root
        && a.trace_root == b.trace_root
        && match (&a.source, &b.source) {
            (PalwClaimSourceV2::Attempt, PalwClaimSourceV2::Attempt) => true,
            (PalwClaimSourceV2::FreePrompt { quanta: a, .. }, PalwClaimSourceV2::FreePrompt { quanta: b, .. }) => a == b,
            _ => false,
        }
}
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
struct RootRecordV15 {
    version: u16,
    execution_root: Hash64,
    scope: Hash64,
    members: ComponentRootV2,
    captured_point: PalwBlockContextV2,
    complete: bool,
}
crate::palw_native_resource_v1::wire::wire_struct!(RootRecordV15;u16,Hash64,Hash64,ComponentRootV2,PalwBlockContextV2,bool);
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
struct SnapshotV15 {
    version: u16,
    claim: Hash64,
    stored: StoredBirthV15,
    point: PalwBlockContextV2,
    execution_root: Hash64,
    members: ComponentRootV2,
    original: RowValueV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(SnapshotV15;u16,Hash64,StoredBirthV15,PalwBlockContextV2,Hash64,ComponentRootV2,RowValueV2);
fn scope(execution_root: Hash64) -> Result<Hash64, ContextErrorV2> {
    digest(1, &execution_root)
}
fn snapshot_key(id: Hash64, born: StoredBirthV15) -> Result<Hash64, ContextErrorV2> {
    digest(2, &(id, born))
}
fn archive_key(id: Hash64) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = b'A';
    key[1..].copy_from_slice(id.as_byte_slice());
    key
}
fn archive(snap: &SnapshotV15, manifest: RowValueV2) -> store::ArchiveReferencesV2 {
    store::ArchiveReferencesV2 { version: 2, components: vec![snap.members], literals: vec![manifest, snap.original] }
}
fn archive_demand() -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentRootV2>(1).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<RowValueV2>(2).map_err(ContextErrorV2::Resource)?;
    reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)
}
fn check_root(record: &RootRecordV15) -> Result<(), ContextErrorV2> {
    if record.version != 15
        || record.captured_point.block == Hash64::default()
        || record.execution_root == Hash64::default()
        || record.scope != scope(record.execution_root)?
        || record.members.spec != spec(MEMBERS)
        || !record.complete
    {
        return Err(bad("cohort current root authority"));
    }
    Ok(())
}
fn read_root(s: &PalwChainStateV2, execution_root: Hash64) -> Result<Option<NativeRetainedV1<RootRecordV15>>, ContextErrorV2> {
    let Some(rv) = reference(s, ROOTS, None, execution_root.as_byte_slice())? else {
        return Ok(None);
    };
    let record = body::<RootRecordV15>(s, rv)?;
    check_root(&record)?;
    if record.execution_root != execution_root || root(s, MEMBERS, Some(record.scope))? != record.members {
        return Err(bad("cohort current root alias"));
    }
    Ok(Some(record))
}
fn visit_member_root_until(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    members: ComponentRootV2,
    mut visit: impl FnMut(Hash64, &PalwClaimStateV2) -> Result<std::ops::ControlFlow<()>, PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    if members.spec != spec(MEMBERS) {
        return Err(err(bad("cohort member root profile")));
    }
    let mut callback = None;
    let mut stopped = false;
    let result = nodes::complete_rows(&StateNodesV2(s), members, |key, value| {
        let claim = body::<PalwClaimStateV2>(s, value)?;
        let id = member_id(key, &claim)?;
        if claim.execution_root != execution_root {
            return Err(bad("cohort member substituted execution root"));
        }
        match visit(id, &claim) {
            Ok(std::ops::ControlFlow::Continue(())) => Ok(()),
            Ok(std::ops::ControlFlow::Break(())) => {
                stopped = true;
                Err(bad("cohort semantic traversal stopped"))
            }
            Err(error) => {
                callback = Some(error);
                Err(bad("cohort member callback"))
            }
        }
    });
    if let Some(error) = callback.take() {
        return Err(error);
    }
    if stopped {
        return Ok(true);
    }
    result.map(|_| false).map_err(err)
}
fn visit_member_root(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    members: ComponentRootV2,
    mut visit: impl FnMut(Hash64, &PalwClaimStateV2) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    visit_member_root_until(s, execution_root, members, |id, c| {
        visit(id, c)?;
        Ok(std::ops::ControlFlow::Continue(()))
    })
    .map(|_| ())
}
/// Cold/global census supplement only. This authenticates stored records; it
/// does not consult the census, so restore has no circular trust dependency.
pub(super) fn visit_current_cohort_members_v15(
    s: &PalwChainStateV2,
    mut visit: impl FnMut(Hash64, Hash64, &PalwClaimStateV2) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let roots = root(s, ROOTS, None).map_err(err)?;
    let mut callback = None;
    let result = nodes::complete_rows(&StateNodesV2(s), roots, |key, rv| {
        let record = body::<RootRecordV15>(s, rv)?;
        check_root(&record)?;
        if key != record.execution_root.as_byte_slice() || root(s, MEMBERS, Some(record.scope))? != record.members {
            return Err(bad("cohort directory/current mismatch"));
        }
        if let Err(error) = visit_member_root(s, record.execution_root, record.members, |id, c| {
            if c.accepted_daa > record.captured_point.daa_score || c.accepted_blue_score > record.captured_point.blue_score {
                return Err(err(bad("cohort current member after capture point")));
            }
            visit(record.execution_root, id, c)
        }) {
            callback = Some(error);
            return Err(bad("cohort current callback"));
        }
        Ok(())
    });
    if let Some(error) = callback.take() {
        return Err(error);
    }
    result.map_err(err)
}
pub(super) fn current_cohort_root_is_complete_v15(s: &PalwChainStateV2, execution_root: Hash64) -> Result<bool, PalwStateV2Error> {
    Ok(read_root(s, execution_root).map_err(err)?.is_some())
}
/// Absence uses the unchanged legacy reader. A malformed present snapshot is
/// never converted into an empty set, a successful source, or a legacy fallback.
fn read_snapshot(
    s: &PalwChainStateV2,
    id: Hash64,
    target: &PalwClaimStateV2,
) -> Result<Option<NativeRetainedV1<SnapshotV15>>, PalwStateV2Error> {
    let key = snapshot_key(id, StoredBirthV15::of(target)).map_err(err)?;
    let Some(rv) = reference(s, SNAPSHOTS, None, key.as_byte_slice()).map_err(err)? else {
        return Ok(None);
    };
    let snapshot = body::<SnapshotV15>(s, rv).map_err(err)?;
    if snapshot.version != 15
        || snapshot.claim != id
        || snapshot.stored != StoredBirthV15::of(target)
        || snapshot.execution_root != target.execution_root
        || snapshot.point.daa_score != target.accepted_daa
        || snapshot.point.blue_score != target.accepted_blue_score
        || snapshot.members.spec != spec(MEMBERS)
    {
        return Err(err(bad("cohort snapshot birth/root authority")));
    }
    let original = body::<PalwClaimStateV2>(s, snapshot.original).map_err(err)?;
    if !same_identity(&original, target) {
        return Err(err(bad("cohort target original identity")));
    }
    let self_ref = nodes::lookup(&StateNodesV2(s), snapshot.members, &member_key(id, &original))
        .map_err(err)?
        .ok_or_else(|| err(bad("complete cohort omitted its actual target birth")))?;
    let self_claim = body::<PalwClaimStateV2>(s, self_ref).map_err(err)?;
    if !same_identity(&self_claim, &original) {
        return Err(err(bad("cohort target member original differs")));
    }
    if let Some(origin) = super::palw_claim_local_source_admission_v8::for_stored_claim_v8(s, id, &original)?
        && (origin.accepting_point() != snapshot.point || !same_identity(origin.original_claim(), &original))
    {
        return Err(err(bad("cohort target substitutes actual acceptance")));
    }
    let _archive = archive_demand().map_err(err)?;
    let inventory = archive(&snapshot, rv);
    let encoded = encode(&inventory).map_err(err)?;
    let aid = value(&encoded).data;
    let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    let stored = s
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, archive_key(aid).to_vec())))
        .ok_or_else(|| err(ContextErrorV2::Unavailable("cohort snapshot inventory missing")))?;
    crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&inventory, stored)
        .map_err(|_| err(bad("cohort snapshot inventory differs")))?;
    Ok(Some(snapshot))
}
pub(super) fn snapshot_point_v15(
    s: &PalwChainStateV2,
    id: Hash64,
    target: &PalwClaimStateV2,
) -> Result<Option<PalwBlockContextV2>, PalwStateV2Error> {
    Ok(read_snapshot(s, id, target)?.map(|snapshot| snapshot.point))
}
pub(super) fn visit_snapshot_members_v15(
    s: &PalwChainStateV2,
    id: Hash64,
    target: &PalwClaimStateV2,
    mut visit: impl FnMut(Hash64, &PalwClaimStateV2) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let Some(snapshot) = read_snapshot(s, id, target)? else {
        return Ok(false);
    };
    visit_member_root(s, snapshot.execution_root, snapshot.members, |source, c| {
        if c.accepted_daa > target.accepted_daa || c.accepted_blue_score > target.accepted_blue_score {
            return Err(err(bad("cohort contains a future source")));
        }
        visit(source, c)
    })?;
    Ok(true)
}
#[derive(Clone)]
struct MemberCopyV15 {
    id: Hash64,
    claim: PalwClaimStateV2,
}
fn collect_members(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    point: PalwBlockContextV2,
) -> Result<Option<NativeRetainedV1<Vec<MemberCopyV15>>>, PalwStateV2Error> {
    let mut q = TirNativeRequestsV1::new();
    let mut count = 0u64;
    let complete = super::palw_claim_local_execution_cohort_census_v15::visit_birth_eligible_members_v15(
        s,
        execution_root,
        point,
        |_, claim| {
            count = count.checked_add(1).ok_or(PalwStateV2Error::Overflow("cohort source count"))?;
            let heap = claim.native_heap_v1("cohort source copy");
            q.add_v1(heap.bytes().ok_or_else(|| PalwStateV2Error::NativeResource(heap.clone()))?)
                .map_err(PalwStateV2Error::NativeResource)?;
            Ok(())
        },
    )?;
    if !complete {
        return Ok(None);
    }
    q.exact_v1::<MemberCopyV15>(count).map_err(PalwStateV2Error::NativeResource)?;
    let owner = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut copies = Vec::with_capacity(usize::try_from(count).map_err(|_| err(bad("cohort source host count")))?);
    let complete = super::palw_claim_local_execution_cohort_census_v15::visit_birth_eligible_members_v15(
        s,
        execution_root,
        point,
        |id, claim| {
            if copies.len() == copies.capacity() {
                return Err(err(bad("cohort census changed between passes")));
            }
            copies.push(MemberCopyV15 { id, claim: claim.clone() });
            Ok(())
        },
    )?;
    if !complete || copies.len() as u64 != count {
        return Err(err(bad("cohort census changed completeness")));
    }
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(copies, owner)))
}
fn synchronize_root(
    b: &mut TransitionBuilder<'_>,
    execution_root: Hash64,
    point: PalwBlockContextV2,
    members: &[MemberCopyV15],
) -> Result<ComponentRootV2, ContextErrorV2> {
    let scope = scope(execution_root)?;
    let existing = read_root(&b.state, execution_root)?;
    if existing
        .as_ref()
        .is_some_and(|old| old.captured_point.daa_score > point.daa_score || old.captured_point.blue_score > point.blue_score)
    {
        return Err(bad("cohort capture moved before actual prior observation"));
    }
    let before = if let Some(r) = &existing { r.members } else { root(&b.state, MEMBERS, Some(scope))? };
    if existing.is_none() && before.rows != 0 {
        return Err(bad("cohort orphan current members"));
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<[u8; 144]>(before.rows).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<[u8; 144], ()>>(members.len() as u64).map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut old: Vec<[u8; 144]> = Vec::with_capacity(usize::try_from(before.rows).map_err(|_| bad("cohort current count"))?);
    nodes::complete_rows(&StateNodesV2(&b.state), before, |key, rv| {
        let c = body::<PalwClaimStateV2>(&b.state, rv)?;
        member_id(key, &c)?;
        if c.execution_root != execution_root {
            return Err(bad("cohort current member root"));
        }
        old.push(key.try_into().map_err(|_| bad("cohort current key"))?);
        Ok(())
    })?;
    let mut wanted = BTreeSet::new();
    for member in members {
        if member.claim.execution_root != execution_root {
            return Err(bad("cohort census mixed execution roots"));
        }
        let key = member_key(member.id, &member.claim);
        if !wanted.insert(key) {
            return Err(bad("cohort census duplicate birth"));
        }
        if let Some(prior) = reference(&b.state, MEMBERS, Some(scope), &key)? {
            let prior = body::<PalwClaimStateV2>(&b.state, prior)?;
            if !same_identity(&prior, &member.claim) {
                return Err(bad("cohort member immutable identity changed"));
            }
        } else {
            put(b, MEMBERS, Some(scope), &key, Some(&member.claim))?;
        }
    }
    for key in old {
        if !wanted.contains(&key) {
            put_raw(b, MEMBERS, Some(scope), &key, None)?;
        }
    }
    let current = root(&b.state, MEMBERS, Some(scope))?;
    let record = RootRecordV15 { version: 15, execution_root, scope, members: current, captured_point: point, complete: true };
    put(b, ROOTS, None, execution_root.as_byte_slice(), Some(&record))?;
    Ok(current)
}
struct BornV15 {
    id: Hash64,
    claim: PalwClaimStateV2,
}
fn collect_actual_births(
    b: &TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    requested: &BTreeSet<Hash64>,
) -> Result<NativeRetainedV1<Vec<BornV15>>, PalwStateV2Error> {
    let mut count = 0u64;
    let mut q = TirNativeRequestsV1::new();
    for e in &b.entries {
        if let PalwDeltaEntryV2::Claim { key, new: Some(c), old: None } = e
            && requested.contains(key)
        {
            count = count.checked_add(1).ok_or(PalwStateV2Error::Overflow("cohort actual births"))?;
            let h = c.native_heap_v1("cohort born source");
            q.add_v1(h.bytes().ok_or_else(|| PalwStateV2Error::NativeResource(h.clone()))?)
                .map_err(PalwStateV2Error::NativeResource)?;
        }
    }
    q.exact_v1::<BornV15>(count).map_err(PalwStateV2Error::NativeResource)?;
    let owner = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut births = Vec::with_capacity(usize::try_from(count).map_err(|_| err(bad("cohort born host count")))?);
    for e in &b.entries {
        if let PalwDeltaEntryV2::Claim { key, new: Some(c), old: None } = e {
            if !requested.contains(key) {
                continue;
            }
            let source = super::palw_claim_local_source_admission_v8::for_stored_claim_v8(&b.state, *key, c)?
                .ok_or_else(|| err(ContextErrorV2::Unavailable("actual cohort birth lacks Source8")))?;
            if source.accepting_point() != point || source.original_claim() != c {
                return Err(err(bad("cohort actual birth journal/original changed")));
            }
            births.push(BornV15 { id: *key, claim: c.clone() });
        }
    }
    if births.len() != requested.len() {
        return Err(err(bad("cohort target is not exactly one actual accepted birth")));
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(births, owner))
}
#[derive(Default)]
pub(super) struct CohortCaptureReportV15 {
    pub complete_snapshots: u64,
    pub missing_legacy_census: u64,
    pub no_cohort_needed: u64,
}
/// Native data factory only, not a funded installer. The caller first updates
/// the census with this ENTIRE real postblock batch. Root's future signed-plan
/// installer must rederive/charge its measured scope before registering a route.
/// target_ids are requested DATA scopes, never a claim of funded status. Only
/// those real accepted targets create snapshots; unrelated block births are
/// not cloned or archived. All RELATED same-block peers still enter each root.
pub(super) fn capture_postblock_cohorts_v15(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    target_ids: &[Hash64],
) -> Result<CohortCaptureReportV15, PalwStateV2Error> {
    let first = b.entries.len();
    let gc = b.claim_local_gc_sequence_v3;
    let result = capture_postblock_inner(b, point, target_ids);
    if result.is_err() {
        super::palw_claim_local_context_gc_v3::rollback_aux_v3(b, first, gc, |table, key| {
            table == PALW_CLAIM_LOCAL_TABLE_V1 && matches!(key.first(), Some(b'N' | b'B' | b'I' | b'A'))
        })
        .map_err(err)?;
    }
    result
}
fn capture_postblock_inner(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    target_ids: &[Hash64],
) -> Result<CohortCaptureReportV15, PalwStateV2Error> {
    let mut selected_quote = TirNativeRequestsV1::new();
    selected_quote.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(target_ids.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _selected = reserve_native_requests_v1(selected_quote.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut requested = BTreeSet::new();
    for id in target_ids {
        if !requested.insert(*id) {
            return Err(err(bad("cohort duplicate prospective target")));
        }
    }
    let births = collect_actual_births(b, point, &requested)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(births.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ComponentRootV2>>(births.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _owner = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut roots = BTreeSet::new();
    let mut completed = BTreeMap::new();
    for born in births.iter() {
        if born.claim.execution_root != Hash64::default() {
            roots.insert(born.claim.execution_root);
        }
    }
    let mut report = CohortCaptureReportV15::default();
    // PASS ONE: every actual new peer is already in the census; synchronize all
    // matching member roots BEFORE any consumer snapshot is pinned.
    for execution_root in &roots {
        let Some(members) = collect_members(&b.state, *execution_root, point)? else {
            report.missing_legacy_census =
                report.missing_legacy_census.checked_add(1).ok_or(PalwStateV2Error::Overflow("cohort incomplete roots"))?;
            continue;
        };
        let actual = atomic(b, |b| synchronize_root(b, *execution_root, point, &members)).map_err(err)?;
        completed.insert(*execution_root, actual);
    }
    // A Complete census must contain EVERY actual birth, including a transient
    // insert/remove peer. Missing one cannot turn into an empty/partial snapshot.
    for entry in &b.entries {
        if let PalwDeltaEntryV2::Claim { key, new: Some(claim), old: None } = entry
            && let Some(members) = completed.get(&claim.execution_root)
        {
            let rv = nodes::lookup(&StateNodesV2(&b.state), *members, &member_key(*key, claim))
                .map_err(err)?
                .ok_or_else(|| err(bad("complete cohort omitted actual same-block birth")))?;
            let actual = body::<PalwClaimStateV2>(&b.state, rv).map_err(err)?;
            if !same_identity(&actual, claim) {
                return Err(err(bad("complete cohort altered actual same-block birth")));
            }
        }
    }

    // PASS TWO: every same-block consumer pins the same complete postbatch tree.
    for born in births.iter() {
        if born.claim.execution_root == Hash64::default() {
            report.no_cohort_needed =
                report.no_cohort_needed.checked_add(1).ok_or(PalwStateV2Error::Overflow("cohort unneeded scopes"))?;
            continue;
        }
        // A transient insert/void/remove still belongs to OTHER same-block peers'
        // member root. It has no live target requiring its own new snapshot; its
        // actual native closed-status owner remains authoritative after removal.
        if !b.state.claims.get(&born.id).is_some_and(|current| same_identity(current, &born.claim)) {
            continue;
        }
        // Consume only this batch's successful complete synchronization result.
        // An older directory root cannot license a missing current census, and
        // same-block peers do not repeat or clone the full cohort for each pin.
        let Some(members) = completed.get(&born.claim.execution_root).copied() else {
            continue;
        };
        atomic(b, |b| {
            let key = snapshot_key(born.id, StoredBirthV15::of(&born.claim))?;
            if reference(&b.state, SNAPSHOTS, None, key.as_byte_slice())?.is_some() {
                return Err(bad("cohort actual snapshot duplicated"));
            }
            let original = literal(b, &born.claim)?;
            let native_source = super::palw_claim_local_source_admission_v8::for_stored_claim_v8(&b.state, born.id, &born.claim)
                .map_err(native)?
                .ok_or(bad("cohort actual Source8 alias missing"))?;
            if native_source.original_reference_v18() != original {
                return Err(bad("cohort original literal differs from actual Source8 birth"));
            }
            let snapshot = SnapshotV15 {
                version: 15,
                claim: born.id,
                stored: StoredBirthV15::of(&born.claim),
                point,
                execution_root: born.claim.execution_root,
                members,
                original,
            };
            let rv = literal(b, &snapshot)?;
            let _a = archive_demand()?;
            let inventory = archive(&snapshot, rv);
            let raw = encode(&inventory)?;
            let aid = value(&raw).data;
            b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &archive_key(aid), Some(&inventory)).map_err(native)?;
            put(b, SNAPSHOTS, None, key.as_byte_slice(), Some(&snapshot))?;
            Ok(())
        })
        .map_err(err)?;
        report.complete_snapshots = report.complete_snapshots.checked_add(1).ok_or(PalwStateV2Error::Overflow("cohort snapshots"))?;
    }
    Ok(report)
}
/// Exact native physical target removal releases its own snapshot only. The
/// shared current catalogue is a separate native owner; no H-based closure or
/// deletion of a newer raw-ID cohort is inferred here.
pub(super) fn release_snapshot_v15(b: &mut TransitionBuilder<'_>, id: Hash64, old: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let key = snapshot_key(id, StoredBirthV15::of(old))?;
        let Some(rv) = reference(&b.state, SNAPSHOTS, None, key.as_byte_slice())? else {
            return Ok(());
        };
        // Releasing an exact pointer requires its genuine canonical manifest,
        // original/self membership and A inventory. It does not re-open every
        // other member merely to decrement a root pin; cold owns full closure.
        let snap = read_snapshot(&b.state, id, old).map_err(native)?.ok_or_else(|| bad("cohort removal lost present snapshot"))?;
        if snap.claim != id || snap.stored != StoredBirthV15::of(old) {
            return Err(bad("cohort removal birth alias"));
        }
        let _a = archive_demand()?;
        let a = archive(&snap, rv);
        let raw = encode(&a)?;
        let aid = value(&raw).data;
        put_raw(b, SNAPSHOTS, None, key.as_byte_slice(), None)?;
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &archive_key(aid), None)
            .map_err(native)?;
        Ok(())
    })
    .map_err(err)
}
/// Scoped data materials, including immutable component nodes and literal
/// member bodies. These are data openings, never a partial complete state.
pub(super) fn visit_checkpoint_material_v15(
    s: &PalwChainStateV2,
    id: Hash64,
    target: &PalwClaimStateV2,
    mut visit: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    if !visit_snapshot_members_v15(s, id, target, |_, _| Ok(()))? {
        return Ok(false);
    }
    let key = snapshot_key(id, StoredBirthV15::of(target)).map_err(err)?;
    let rv = reference(s, SNAPSHOTS, None, key.as_byte_slice())
        .map_err(err)?
        .ok_or_else(|| err(bad("cohort material descriptor disappeared")))?;
    let snapshot = body::<SnapshotV15>(s, rv).map_err(err)?;
    for literal in [rv, snapshot.original] {
        let raw = StateNodesV2(s).body(literal).map_err(err)?;
        visit(&raw)?;
    }
    let mut callback = None;
    let result = nodes::visit_nodes(&StateNodesV2(s), snapshot.members, |_, node| {
        let raw = encode(node)?;
        if let Err(error) = visit(&raw) {
            callback = Some(error);
            return Err(bad("cohort node material callback"));
        }
        Ok(())
    });
    if let Some(error) = callback.take() {
        return Err(error);
    }
    result.map_err(err)?;
    let result = nodes::complete_rows(&StateNodesV2(s), snapshot.members, |_, v| {
        let raw = StateNodesV2(s).body(v)?;
        if let Err(error) = visit(&raw) {
            callback = Some(error);
            return Err(bad("cohort body material callback"));
        }
        Ok(())
    });
    if let Some(error) = callback.take() {
        return Err(error);
    }
    result.map_err(err)?;
    Ok(true)
}
/// Native cold validation of this owner's schemas/pointers/archive closure.
/// All sources/metadata remain independently rooted; no whole-world synthesis.
pub(super) fn validate_cohort_archives_v15(s: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    visit_current_cohort_members_v15(s, |_, _, _| Ok(()))?;
    let snapshots = root(s, SNAPSHOTS, None).map_err(err)?;
    let mut callback = None;
    let result = nodes::complete_rows(&StateNodesV2(s), snapshots, |key, rv| {
        let snap = body::<SnapshotV15>(s, rv)?;
        if key != snapshot_key(snap.claim, snap.stored)?.as_byte_slice() {
            return Err(bad("cohort snapshot directory key"));
        }
        let original = body::<PalwClaimStateV2>(s, snap.original)?;
        if let Err(error) = visit_snapshot_members_v15(s, snap.claim, &original, |_, _| Ok(())) {
            callback = Some(error);
            return Err(bad("cohort cold snapshot"));
        }
        Ok(())
    });
    if let Some(error) = callback.take() {
        return Err(error);
    }
    result.map_err(err)?;
    if root(s, SCRATCH, None).map_err(err)?.rows != 0 {
        return Err(err(bad("cohort partial scratch publication")));
    }
    // Only the NEW owner15/schema10 MEMBERS profile reserves this generic-A2
    // shape. Other owners and independently typed A3+ archives stay unchanged.
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'A') || !raw.starts_with(&[2, 0]) {
                continue;
            }
            let a = decode::<store::ArchiveReferencesV2>(raw).map_err(err)?;
            if !a.components.iter().any(|r| r.spec.owner == 15 && r.spec.schema == 10 && r.spec.table == MEMBERS) {
                continue;
            }
            if a.components.len() != 1
                || a.components[0].spec != spec(MEMBERS)
                || a.literals.len() != 2
                || key.as_slice() != archive_key(value(raw).data)
            {
                return Err(err(bad("cohort A2 reserved inventory shape/key")));
            }
            let manifest = a.literals[0];
            let snap = body::<SnapshotV15>(s, manifest).map_err(err)?;
            if snap.members != a.components[0] || snap.original != a.literals[1] {
                return Err(err(bad("cohort A2 cross-snapshot binding")));
            }
            let expected = snapshot_key(snap.claim, snap.stored).map_err(err)?;
            if reference(s, SNAPSHOTS, None, expected.as_byte_slice()).map_err(err)? != Some(manifest) {
                return Err(err(bad("cohort orphan A2 snapshot manifest")));
            }
            let _a = archive_demand().map_err(err)?;
            let canonical = archive(&snap, manifest);
            crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&canonical, raw)
                .map_err(|_| err(bad("cohort A2 canonical target inventory")))?;
        }
    }
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'I') {
                continue;
            }
            let at = match key.len() {
                18 => 1,
                84 if key[1..3] == [2, 0] => 67,
                _ => continue,
            };
            if key[at] != 15 {
                continue;
            }
            let r = decode::<ComponentRootV2>(raw).map_err(err)?;
            if r.spec.owner != 15
                || r.spec.schema != 10
                || r.spec != spec(r.spec.table)
                || !matches!(r.spec.table, ROOTS | MEMBERS | SNAPSHOTS | SCRATCH)
            {
                return Err(err(bad("cohort current root schema")));
            }
            if r.spec.table == MEMBERS {
                if key.len() != 84 {
                    return Err(err(bad("cohort member root lacks scope")));
                }
                let scope = Hash64::from_bytes(key[3..67].try_into().map_err(|_| err(bad("cohort scope width")))?);
                let mut found = false;
                nodes::complete_rows(&StateNodesV2(s), root(s, ROOTS, None).map_err(err)?, |_, v| {
                    let record = body::<RootRecordV15>(s, v)?;
                    if record.scope == scope && record.members == *r {
                        found = true
                    }
                    Ok(())
                })
                .map_err(err)?;
                if !found {
                    return Err(err(bad("cohort orphan owned current")));
                }
            } else if key.len() != 18 {
                return Err(err(bad("cohort global root has owned alias")));
            }
        }
    }
    Ok(())
}
#[derive(Clone, Copy, Debug)]
pub(super) struct MissingCohortCostV15 {
    pub legacy_census: bool,
    pub global_census_native_holding: bool,
    pub current_catalogue_release_authority: bool,
    pub future_rewrite_event_population: bool,
    pub source_readiness_legacy_fallback: bool,
    pub compact_denial_projection: bool,
    pub native_gc_metadata: bool,
    pub identity_and_readiness_cpu_tariff: bool,
}
/// No rates/fees/token. This is prospective physical geometry for one actual
/// matching-root universe at a native candidate point, before any funding.
pub(super) struct CohortCostQuoteV15 {
    pub execution_root: Hash64,
    pub native_members: u64,
    pub prior_members: u64,
    pub member_literal_bytes: u64,
    /// Actual current eligible universe only; not a future producer-signed cap.
    pub maximum_current_target_claim_bytes: u64,
    pub maximum_mutation_nodes: u64,
    pub publication_wire_max: u64,
    pub undo_wire_max: u64,
    pub native_requests_max: u64,
    pub missing: MissingCohortCostV15,
}
fn plus(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
    a.checked_add(b).ok_or(PalwStateV2Error::Overflow("cohort quote sum"))
}
fn times(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
    a.checked_mul(b).ok_or(PalwStateV2Error::Overflow("cohort quote product"))
}
pub(super) fn quote_cohort_cost_v15(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    point: PalwBlockContextV2,
) -> Result<CohortCostQuoteV15, PalwStateV2Error> {
    // Existing readiness never couples zero execution roots. This means this
    // particular row family is not required, NOT an empty historical universe.
    if execution_root == Hash64::default() {
        return Ok(CohortCostQuoteV15 {
            execution_root,
            native_members: 0,
            prior_members: 0,
            member_literal_bytes: 0,
            maximum_current_target_claim_bytes: 0,
            maximum_mutation_nodes: 0,
            publication_wire_max: 0,
            undo_wire_max: 0,
            native_requests_max: 0,
            missing: MissingCohortCostV15 {
                legacy_census: false,
                global_census_native_holding: false,
                current_catalogue_release_authority: false,
                future_rewrite_event_population: false,
                source_readiness_legacy_fallback: false,
                compact_denial_projection: false,
                native_gc_metadata: false,
                identity_and_readiness_cpu_tariff: false,
            },
        });
    }
    let mut count = 0u64;
    let mut literals = 0u64;
    let mut maximum_target = 0u64;
    let mut requests = 0u64;
    let mut max_nodes = 0u64;
    let mut wire = 0u64;
    let complete =
        super::palw_claim_local_execution_cohort_census_v15::visit_birth_eligible_members_v15(s, execution_root, point, |_, c| {
            count = plus(count, 1)?;
            let n = borsh::object_length(c).map_err(|_| err(bad("cohort quote native Claim length")))? as u64;
            literals = plus(literals, n)?;
            maximum_target = maximum_target.max(n);
            let g = nodes::mutation_geometry(spec(MEMBERS), n).map_err(err)?;
            max_nodes = plus(max_nodes, g.max_nodes)?;
            wire = plus(wire, plus(g.retained_wire, 4)?)?;
            requests = plus(requests, g.retained_heap)?;
            let h = c.native_heap_v1("cohort quote copied native member");
            requests = plus(requests, h.bytes().ok_or_else(|| PalwStateV2Error::NativeResource(h.clone()))?)?;
            Ok(())
        })?;
    let prior = read_root(s, execution_root).map_err(err)?.map_or(0, |r| r.members.rows);
    let deletion = nodes::mutation_geometry(spec(MEMBERS), 0).map_err(err)?;
    max_nodes = plus(max_nodes, times(prior, deletion.max_nodes)?)?;
    wire = plus(wire, times(prior, deletion.retained_wire)?)?;
    requests = plus(requests, times(prior, deletion.retained_heap)?)?;
    // Actual codec widths: current309, snapshot460, A2(1root,2literal)=244.
    // Directory updates use bounded64byte paths; no global population scan.
    for (table, n) in [(ROOTS, 309u64), (SNAPSHOTS, 460u64)] {
        let g = nodes::mutation_geometry(spec(table), n).map_err(err)?;
        max_nodes = plus(max_nodes, g.max_nodes)?;
        wire = plus(wire, plus(g.retained_wire, 4)?)?;
        requests = plus(requests, g.retained_heap)?;
    }
    // literal(original) and literal(Snapshot460) each insert AND remove a
    // SCRATCH component. Even a shared B literal still builds the temporary
    // component paths and current-I/undo rows. Bound the actual requested
    // target by the maximum eligible native Claim (capture requires its self
    // membership). This is current-universe geometry; future event/member and
    // global/GC/holding limits remain explicitly Missing below.
    for n in [maximum_target, 460u64] {
        for bytes in [n, 0u64] {
            let g = nodes::mutation_geometry(spec(SCRATCH), bytes).map_err(err)?;
            max_nodes = plus(max_nodes, g.max_nodes)?;
            let outer = if bytes == 0 { 0 } else { 4 };
            wire = plus(wire, plus(g.retained_wire, outer)?)?;
            requests = plus(requests, g.retained_heap)?;
        }
    }
    wire = plus(wire, 65 + 244 + 65 + 4 + 460 + 65 + 4 + 309 + 84 + 90)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<MemberCopyV15>(count).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<[u8; 144]>(prior).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<[u8; 144], ()>>(count).map_err(PalwStateV2Error::NativeResource)?;
    requests = plus(requests, q.bytes_v1())?;
    Ok(CohortCostQuoteV15 {
        execution_root,
        native_members: count,
        prior_members: prior,
        member_literal_bytes: literals,
        maximum_current_target_claim_bytes: maximum_target,
        maximum_mutation_nodes: max_nodes,
        publication_wire_max: wire,
        undo_wire_max: times(wire, 4)?,
        native_requests_max: requests,
        missing: MissingCohortCostV15 {
            legacy_census: !complete,
            global_census_native_holding: true,
            current_catalogue_release_authority: true,
            future_rewrite_event_population: true,
            source_readiness_legacy_fallback: true,
            compact_denial_projection: true,
            native_gc_metadata: true,
            identity_and_readiness_cpu_tariff: true,
        },
    })
}
/// Exact matching-root lookup for the census's current authority refresh.
/// Returns a genuine retained canonical member; an old cache flag is no proof.
pub(super) fn current_cohort_member_v15(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    id: Hash64,
    claim: &PalwClaimStateV2,
) -> Result<Option<NativeRetainedV1<PalwClaimStateV2>>, PalwStateV2Error> {
    let Some(r) = read_root(s, execution_root).map_err(err)? else {
        return Ok(None);
    };
    let key = member_key(id, claim);
    let Some(rv) = nodes::lookup(&StateNodesV2(s), r.members, &key).map_err(err)? else {
        return Ok(None);
    };
    let actual = body::<PalwClaimStateV2>(s, rv).map_err(err)?;
    if actual.execution_root != execution_root
        || !same_identity(&actual, claim)
        || actual.accepted_daa > r.captured_point.daa_score
        || actual.accepted_blue_score > r.captured_point.blue_score
    {
        return Err(err(bad("cohort exact current member identity")));
    }
    Ok(Some(actual))
}
pub(super) fn current_cohort_contains_member_v15(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    id: Hash64,
    claim: &PalwClaimStateV2,
) -> Result<bool, PalwStateV2Error> {
    Ok(current_cohort_member_v15(s, execution_root, id, claim)?.is_some())
}

/// A later native observation cannot stand in for an earlier consumer world.
/// The actual point includes subsidy and is never manufactured from STORED.
pub(super) fn current_cohort_root_is_complete_at_v15(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    consumer_point: PalwBlockContextV2,
) -> Result<bool, PalwStateV2Error> {
    let Some(record) = read_root(s, execution_root).map_err(err)? else {
        return Ok(false);
    };
    let p = record.captured_point;
    Ok(p.daa_score <= consumer_point.daa_score
        && p.blue_score <= consumer_point.blue_score
        && ((p.daa_score, p.blue_score) != (consumer_point.daa_score, consumer_point.blue_score) || p == consumer_point))
}
/// Matching-root source census supplement after cold/reorg restore. Traverses
/// the ACTUAL restored component, not only cache members that survived a GC.
pub(super) fn visit_current_root_cohort_members_v15(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    consumer_point: PalwBlockContextV2,
    mut visit: impl FnMut(Hash64, &PalwClaimStateV2) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    if !current_cohort_root_is_complete_at_v15(s, execution_root, consumer_point)? {
        return Ok(false);
    }
    let r = read_root(s, execution_root).map_err(err)?.ok_or_else(|| err(bad("cohort covered root disappeared")))?;
    visit_member_root(s, execution_root, r.members, |id, c| {
        if c.accepted_daa > r.captured_point.daa_score || c.accepted_blue_score > r.captured_point.blue_score {
            return Err(err(bad("cohort member after actual root capture")));
        }
        visit(id, c)
    })?;
    Ok(true)
}

/// Semantic reader may stop after a real denial without hydrating later
/// unrelated source bodies. Cold validation still traverses the entire tree.
pub(super) fn visit_snapshot_members_until_v15(
    s: &PalwChainStateV2,
    id: Hash64,
    target: &PalwClaimStateV2,
    mut visit: impl FnMut(Hash64, &PalwClaimStateV2) -> Result<std::ops::ControlFlow<()>, PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let Some(snapshot) = read_snapshot(s, id, target)? else {
        return Ok(false);
    };
    visit_member_root_until(s, snapshot.execution_root, snapshot.members, |source, c| {
        if c.accepted_daa > target.accepted_daa || c.accepted_blue_score > target.accepted_blue_score {
            return Err(err(bad("cohort semantic future source")));
        }
        visit(source, c)
    })?;
    Ok(true)
}
/// Metadata-only matching-root allocation bound, never an empty-world claim.
pub(super) fn current_cohort_member_count_at_v15(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    consumer_point: PalwBlockContextV2,
) -> Result<Option<u64>, PalwStateV2Error> {
    if !current_cohort_root_is_complete_at_v15(s, execution_root, consumer_point)? {
        return Ok(None);
    }
    Ok(read_root(s, execution_root).map_err(err)?.map(|root| root.members.rows))
}
pub(super) fn visit_current_root_cohort_members_until_v15(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    consumer_point: PalwBlockContextV2,
    mut visit: impl FnMut(Hash64, &PalwClaimStateV2) -> Result<std::ops::ControlFlow<()>, PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    if !current_cohort_root_is_complete_at_v15(s, execution_root, consumer_point)? {
        return Ok(false);
    }
    let root = read_root(s, execution_root).map_err(err)?.ok_or_else(|| err(bad("cohort semantic current root disappeared")))?;
    visit_member_root_until(s, execution_root, root.members, |id, c| {
        if c.accepted_daa > root.captured_point.daa_score || c.accepted_blue_score > root.captured_point.blue_score {
            return Err(err(bad("cohort semantic current future member")));
        }
        visit(id, c)
    })?;
    Ok(true)
}
#[cfg(test)]
pub(super) fn assert_snapshot_integrity_negatives_v15(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    first: Hash64,
    second: Hash64,
) {
    let first_claim = state.claims.get(&first).unwrap();
    let second_claim = state.claims.get(&second).unwrap();
    let first_key = snapshot_key(first, StoredBirthV15::of(first_claim)).unwrap();
    let first_ref = reference(state, SNAPSHOTS, None, first_key.as_byte_slice()).unwrap().unwrap();
    let first_snapshot = body::<SnapshotV15>(state, first_ref).unwrap();
    let second_key = snapshot_key(second, StoredBirthV15::of(second_claim)).unwrap();
    let second_ref = reference(state, SNAPSHOTS, None, second_key.as_byte_slice()).unwrap().unwrap();
    let second_snapshot = body::<SnapshotV15>(state, second_ref).unwrap();
    assert_eq!(first_snapshot.members, second_snapshot.members, "same-block peers share the complete root");
    // Exercise a REAL post-release native journal admission refusal, with
    // enough checker memory for the metadata writers. usize::MAX is an
    // impossible remaining-slot request: checked_add fails before allocation,
    // after release has appended its genuine NBIA journal. This tests the
    // exact atomic boundary, not a failpoint or disabled admission.
    {
        use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1};
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: u64::MAX });
        let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
        b.native_reservation = crate::palw_native_resource_v1::NativeTransitionReservationV1::for_checker_v1(&checker);
        let before = b.state.state_root();
        let first_entry = b.entries.len();
        let sequence = b.claim_local_gc_sequence_v3;
        let refusal = release_current_before_removal_v15(&mut b, first, usize::MAX);
        assert!(
            matches!(refusal, Err(PalwStateV2Error::NativeResource(_))),
            "remaining journal admission must refuse before the Claim cascade"
        );
        assert_eq!(b.state.state_root(), before, "post-release refusal restores all archive rows");
        assert_eq!(b.entries.len(), first_entry, "post-release refusal exactly truncates aux journal");
        assert_eq!(b.claim_local_gc_sequence_v3, sequence, "post-release refusal restores GC sequence");
        assert!(has_snapshot_v15(&b.state, first, first_claim).unwrap());
        // The same genuine metadata release succeeds with its real one-slot
        // remaining Claim write and leaves that capacity available.
        release_current_before_removal_v15(&mut b, first, 1).unwrap();
        assert!(b.entries.len() > first_entry);
        assert!(b.entries.capacity() >= b.entries.len() + 1);
        assert!(!has_snapshot_v15(&b.state, first, first_claim).unwrap());
        assert_eq!(b.state.claims.get(&first), Some(first_claim));
    }
    let _archive_owner = archive_demand().unwrap();
    let first_inventory = archive(&first_snapshot, first_ref);
    let raw = encode(&first_inventory).unwrap();
    let aid = value(&raw).data;
    let akey = archive_key(aid);
    {
        let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey, None).unwrap();
        let before = b.state.state_root();
        let entries = b.entries.len();
        assert!(release_snapshot_v15(&mut b, first, first_claim).is_err(), "a missing required A is not a successful release");
        assert_eq!(b.state.state_root(), before);
        assert_eq!(b.entries.len(), entries);
    }
    {
        let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
        put_raw(&mut b, SNAPSHOTS, None, first_key.as_byte_slice(), None).unwrap();
        assert!(validate_cohort_archives_v15(&b.state).is_err(), "orphan A cannot pin a hidden snapshot indefinitely");
    }
    {
        // Deliberate corrupt-store clone: the normal native writer already
        // rejects an A key that disagrees with its literal content hash.
        let mut broken = state.clone();
        let mut wrong = akey;
        wrong[1] ^= 1;
        broken.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, wrong.to_vec()), raw.to_vec());
        assert!(validate_cohort_archives_v15(&broken).is_err(), "wrong A key cannot alias a valid snapshot");
    }
    {
        let _cross_owner = archive_demand().unwrap();
        let cross = store::ArchiveReferencesV2 {
            version: 2,
            components: vec![first_snapshot.members],
            literals: vec![first_ref, second_snapshot.original],
        };
        assert_ne!(first_snapshot.original, second_snapshot.original, "genuine RED carriers have different stored births");
        let encoded = encode(&cross).unwrap();
        let key = archive_key(value(&encoded).data);
        let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&cross)).unwrap();
        assert!(validate_cohort_archives_v15(&b.state).is_err(), "shared member root does not authorize another target's original");
    }
    {
        let mut broken = state.clone();
        let mut key = [0; 65];
        key[0] = b'B';
        key[1..].copy_from_slice(first_ref.data.as_byte_slice());
        assert!(broken.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).is_some());
        assert!(
            visit_snapshot_members_v15(&broken, first, first_claim, |_, _| Ok(())).is_err(),
            "missing descriptor is unavailable, never absent/empty"
        );
    }
}
/// Metadata existence only for read-plan coverage, NOT body validity or status.
/// The following real stream still validates each body before invoking users.
pub(super) fn current_cohort_member_reference_at_v15(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    id: Hash64,
    claim: &PalwClaimStateV2,
    consumer_point: PalwBlockContextV2,
) -> Result<bool, PalwStateV2Error> {
    if !current_cohort_root_is_complete_at_v15(s, execution_root, consumer_point)? {
        return Ok(false);
    }
    let Some(root) = read_root(s, execution_root).map_err(err)? else {
        return Ok(false);
    };
    Ok(nodes::lookup(&StateNodesV2(s), root.members, &member_key(id, claim)).map_err(err)?.is_some())
}

/// Cheap selector only; the release/reader validates every required own row.
pub(super) fn has_snapshot_v15(s: &PalwChainStateV2, id: Hash64, target: &PalwClaimStateV2) -> Result<bool, PalwStateV2Error> {
    let key = snapshot_key(id, StoredBirthV15::of(target)).map_err(err)?;
    Ok(reference(s, SNAPSHOTS, None, key.as_byte_slice()).map_err(err)?.is_some())
}
/// Native writer adapter: only exact physical deletion, after the existing
/// claim mutation sequence was admitted. No archive => no Claim clone. The
/// immutable old birth is retained across its own aux-only release operation.
pub(super) fn release_current_before_removal_v15(
    b: &mut TransitionBuilder<'_>,
    id: Hash64,
    remaining_claim_slots: usize,
) -> Result<(), PalwStateV2Error> {
    let Some(old) = b.state.claims.get(&id) else {
        return Ok(());
    };
    if !b.params.claim_local_settlement_active_at(old.accepted_daa) || !has_snapshot_v15(&b.state, id, old)? {
        return Ok(());
    }
    let request = crate::palw_native_resource_v1::native_value_requests_v1(old).map_err(PalwStateV2Error::NativeResource)?;
    let owner = reserve_native_requests_v1(request).map_err(PalwStateV2Error::NativeResource)?;
    let old = NativeRetainedV1::from_owner_reserved_v1(old.clone(), owner);
    let first = b.entries.len();
    let gc = b.claim_local_gc_sequence_v3;
    let result = (|| {
        release_snapshot_v15(b, id, &old)?;
        // The Claim cascade's value bytes were already reserved and remain
        // admitted. Archive appends may consume its old journal capacity, so
        // re-admit remaining slots from the NEW journal length before the
        // first Claim/cascade mutation. Do not charge its value bytes twice.
        b.reserve_mutation_sequence_v1(0, remaining_claim_slots)
    })();
    if result.is_err() {
        // This adapter has only published NBIA rows, never a Claim/cascade.
        // Refusal of the final journal admission must restore the exact
        // archive and derived GC sequence as well as truncating its journal.
        super::palw_claim_local_context_gc_v3::rollback_aux_v3(b, first, gc, |table, key| {
            table == PALW_CLAIM_LOCAL_TABLE_V1 && matches!(key.first(), Some(b'N' | b'B' | b'I' | b'A'))
        })
        .map_err(err)?;
    }
    result
}
/// Current catalogue DATA footprint only. No retention duration, tariff,
/// irreversible closure or funded export authority is inferred by this quote.
/// Member B occurrences deliberately overcount content sharing. The global
/// ROOTS directory contributes one bounded point path, never its whole tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CatalogueScopedGeometryV15 {
    pub actual_member_nodes: u64,
    pub member_body_occurrences: u64,
    pub member_logical_body_bytes: u64,
    pub retained_wire_upper: u64,
    pub gc_resident_memberships_upper: u64,
    pub gc_resident_requests_upper: u64,
    /// All A2 snapshot pins are independent owners and excluded here.
    pub excludes_snapshot_pins: bool,
}
pub(super) fn quote_current_catalogue_scoped_geometry_v15(
    s: &PalwChainStateV2,
    execution_root: Hash64,
) -> Result<Option<CatalogueScopedGeometryV15>, PalwStateV2Error> {
    let Some(record) = read_root(s, execution_root).map_err(err)? else {
        return Ok(None);
    };
    let mut node_count = 0u64;
    let mut node_wire = 0u64;
    let mut bodies = 0u64;
    let mut body_bytes = 0u64;
    let add = |a: u64, b: u64| a.checked_add(b).ok_or(bad("cohort scoped geometry overflow"));
    nodes::visit_nodes(&StateNodesV2(s), record.members, |_, node| {
        node_count = add(node_count, 1)?;
        let bytes = borsh::object_length(node).map_err(|_| bad("cohort scoped node length"))? as u64;
        node_wire = add(node_wire, add(65, bytes)?)?;
        if let nodes::ContextNodeV2::Leaf { value, .. } = node {
            // This is the actual immutable B row, hash/length checked by its
            // native reader. Do not clone it, enumerate unrelated aux, or
            // synthesize a historical complete state to count it.
            let raw = StateNodesV2(s).body(*value)?;
            bodies = add(bodies, 1)?;
            body_bytes = add(body_bytes, raw.len() as u64)?;
        }
        Ok(())
    })
    .map_err(err)?;
    if bodies != record.members.rows {
        return Err(err(bad("cohort scoped body count differs from rooted members")));
    }
    // A mutation path envelope dominates a read path in the SAME fixed64-byte
    // ROOTS profile. It is a bounded path even in a global many-root directory.
    let directory = nodes::mutation_geometry(spec(ROOTS), 309).map_err(err)?;
    let root_literal = borsh::object_length(&*record).map_err(|_| err(bad("cohort scoped root record length")))? as u64;
    let member_b_wire = plus(body_bytes, times(bodies, 65 + 4)?)?;
    let root_path_wire = times(directory.max_nodes, plus(65, directory.max_node_wire)?)?;
    // Actual owned MEMBERS I key84/root90, global ROOTS I key18/root90,
    // and exact RootRecord B key65 +outerVec4. Snapshot A2/manifest/original
    // pins are intentionally neither removed nor attributed to this owner.
    let retained_wire_upper =
        plus(plus(node_wire, member_b_wire)?, plus(root_path_wire, plus(root_literal, 84 + 90 + 18 + 90 + 65 + 4)?)?)?;
    let gc_resident_memberships_upper = plus(
        plus(node_count, bodies)?,
        plus(directory.max_nodes, 3)?, // RootRecord B plus two current I rows.
    )?;
    let gc_resident_requests_upper = super::palw_claim_local_context_gc_v3::resident_requests_v3(gc_resident_memberships_upper)
        .map_err(PalwStateV2Error::NativeResource)?;
    Ok(Some(CatalogueScopedGeometryV15 {
        actual_member_nodes: node_count,
        member_body_occurrences: bodies,
        member_logical_body_bytes: body_bytes,
        retained_wire_upper,
        gc_resident_memberships_upper,
        gc_resident_requests_upper,
        excludes_snapshot_pins: true,
    }))
}

/// Independent physical-row measurement on the genuine core capture fixture.
/// This is a data/GC geometry assertion, never a paid retention capability.
#[cfg(test)]
pub(super) fn assert_catalogue_physical_geometry_v15(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    first: Hash64,
    second: Hash64,
) {
    use nodes::ContextNodeReaderV2;
    let first_claim = state.claims.get(&first).unwrap();
    let second_claim = state.claims.get(&second).unwrap();
    let root = first_claim.execution_root;
    assert_eq!(root, second_claim.execution_root);
    let quoted = quote_current_catalogue_scoped_geometry_v15(state, root).unwrap().unwrap();
    let record = read_root(state, root).unwrap().unwrap();
    let mut physical = BTreeSet::<Vec<u8>>::new();
    let mut actual_nodes = 0u64;
    let mut actual_bodies = 0u64;
    let mut logical_body_bytes = 0u64;
    let mut key = |prefix: u8, id: Hash64| {
        let mut k = Vec::with_capacity(65);
        k.push(prefix);
        k.extend_from_slice(id.as_byte_slice());
        k
    };
    nodes::visit_nodes(&StateNodesV2(state), record.members, |id, node| {
        actual_nodes += 1;
        physical.insert(key(b'N', id));
        if let nodes::ContextNodeV2::Leaf { value, .. } = node {
            actual_bodies += 1;
            physical.insert(key(b'B', value.data));
            logical_body_bytes += StateNodesV2(state).body(*value)?.len() as u64;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(actual_bodies, 2, "genuine two-peer cohort");
    assert_eq!(actual_nodes, 3, "two leaves and their actual compressed branch");
    assert_eq!(quoted.actual_member_nodes, actual_nodes);
    assert_eq!(quoted.member_body_occurrences, actual_bodies);
    assert_eq!(quoted.member_logical_body_bytes, logical_body_bytes);
    assert_eq!(
        logical_body_bytes,
        (borsh::object_length(first_claim).unwrap() + borsh::object_length(second_claim).unwrap()) as u64,
        "canonical actual Claims, not synthesized phases or a complete world"
    );
    // Trace the actual ROOTS point read independently instead of charging its
    // entire directory or relying on the quote's mutation envelope.
    struct Trace<'a> {
        state: &'a PalwChainStateV2,
        nodes: std::cell::RefCell<BTreeSet<Hash64>>,
    }
    impl ContextNodeReaderV2 for Trace<'_> {
        fn node(&self, id: Hash64) -> Result<NativeRetainedV1<nodes::ContextNodeV2>, ContextErrorV2> {
            self.nodes.borrow_mut().insert(id);
            StateNodesV2(self.state).node(id)
        }
    }
    let trace = Trace { state, nodes: Default::default() };
    let root_directory = StateNodesV2(state).current(spec(ROOTS)).unwrap();
    let root_ref = nodes::lookup(&trace, root_directory, root.as_byte_slice()).unwrap().unwrap();
    for id in trace.nodes.into_inner() {
        physical.insert(key(b'N', id));
    }
    physical.insert(key(b'B', root_ref.data));
    let mut global_i = vec![b'I'];
    spec(ROOTS).serialize(&mut global_i).unwrap();
    physical.insert(global_i);
    let mut owned_i = vec![b'I'];
    2u16.serialize(&mut owned_i).unwrap();
    record.scope.serialize(&mut owned_i).unwrap();
    spec(MEMBERS).serialize(&mut owned_i).unwrap();
    physical.insert(owned_i);
    let route = state.kernel_route.as_ref().unwrap();
    let observed_wire: u64 = physical
        .iter()
        .map(|k| {
            let raw = route.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, k.clone())).expect("actual retained scoped row");
            (k.len() + raw.len()) as u64
        })
        .sum();
    assert!(quoted.retained_wire_upper >= observed_wire, "quote must dominate independently observed scoped physical rows");
    assert!(quoted.gc_resident_memberships_upper >= physical.len() as u64);
    let observed_gc = super::palw_claim_local_context_gc_v3::resident_requests_v3(physical.len() as u64).unwrap();
    assert!(quoted.gc_resident_requests_upper >= observed_gc);
    // Both target snapshots share member content. Removing their independent
    // A2 pins does not grant permission to discard the current catalogue.
    let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
    release_snapshot_v15(&mut b, first, first_claim).unwrap();
    release_snapshot_v15(&mut b, second, second_claim).unwrap();
    super::palw_claim_local_context_gc_v3::finish_fold_v3(&mut b).unwrap();
    assert!(!has_snapshot_v15(&b.state, first, first_claim).unwrap());
    assert!(!has_snapshot_v15(&b.state, second, second_claim).unwrap());
    assert_eq!(
        quote_current_catalogue_scoped_geometry_v15(&b.state, root).unwrap(),
        Some(quoted),
        "real GC retains current-owned component despite both snapshot releases"
    );
}

/// Opaque alias minted only from the real existing native birth snapshot. Its
/// immutable component/literals may be pinned by the actual retirement owner;
/// no caller-supplied row or current params become an old membership snapshot.
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
pub(super) struct NativeCohortAliasV15 {
    manifest: RowValueV2,
    members: ComponentRootV2,
    original: RowValueV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(NativeCohortAliasV15;RowValueV2,ComponentRootV2,RowValueV2);
crate::palw_native_resource_v1::native_struct!(NativeCohortAliasV15;manifest,members,original);
impl NativeCohortAliasV15 {
    pub(super) fn members_v15(&self) -> ComponentRootV2 {
        self.members
    }
    pub(super) fn literals_v15(&self) -> [RowValueV2; 2] {
        [self.manifest, self.original]
    }
}
pub(super) fn alias_original_snapshot_v15(
    s: &PalwChainStateV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<Option<NativeCohortAliasV15>, PalwStateV2Error> {
    let Some(row) = read_snapshot(s, id, c)? else { return Ok(None) };
    let manifest = reference(s, SNAPSHOTS, None, snapshot_key(id, StoredBirthV15::of(c)).map_err(err)?.as_byte_slice())
        .map_err(err)?
        .ok_or_else(|| err(bad("native cohort alias lost its actual directory")))?;
    Ok(Some(NativeCohortAliasV15 { manifest, members: row.members, original: row.original }))
}
/// Validates an ALREADY persisted alias after its original current directory/A2
/// may have been retired. The receiving native owner must pin these exact roots
/// and literals in its own A2. No import-time creation or status upgrade occurs.
pub(super) fn check_native_alias_v15(
    s: &PalwChainStateV2,
    alias: &NativeCohortAliasV15,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<PalwBlockContextV2, PalwStateV2Error> {
    let row = body::<SnapshotV15>(s, alias.manifest).map_err(err)?;
    if row.version != 15
        || row.claim != id
        || row.stored != StoredBirthV15::of(c)
        || row.execution_root != c.execution_root
        || row.point.daa_score != c.accepted_daa
        || row.point.blue_score != c.accepted_blue_score
        || row.members != alias.members
        || row.original != alias.original
        || row.members.spec != spec(MEMBERS)
    {
        return Err(err(bad("native cohort alias birth/root/source identity")));
    }
    let original = body::<PalwClaimStateV2>(s, row.original).map_err(err)?;
    if !same_identity(&original, c) {
        return Err(err(bad("native cohort alias original identity")));
    }
    let own = nodes::lookup(&StateNodesV2(s), row.members, &member_key(id, &original))
        .map_err(err)?
        .ok_or_else(|| err(bad("native cohort alias omitted actual target")))?;
    let own = body::<PalwClaimStateV2>(s, own).map_err(err)?;
    if !same_identity(&own, &original) {
        return Err(err(bad("native cohort alias self member changed")));
    }
    if let Some(origin) = super::palw_claim_local_source_admission_v8::for_stored_claim_v8(s, id, &original)?
        && (origin.accepting_point() != row.point || !same_identity(origin.original_claim(), &original))
    {
        return Err(err(bad("native cohort alias substitutes actual accepting point")));
    }
    Ok(row.point)
}
pub(super) fn visit_native_alias_members_v15(
    s: &PalwChainStateV2,
    alias: &NativeCohortAliasV15,
    id: Hash64,
    c: &PalwClaimStateV2,
    mut f: impl FnMut(Hash64, &PalwClaimStateV2) -> Result<std::ops::ControlFlow<()>, PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    check_native_alias_v15(s, alias, id, c)?;
    visit_member_root_until(s, c.execution_root, alias.members, |id, m| {
        if m.accepted_daa > c.accepted_daa || m.accepted_blue_score > c.accepted_blue_score {
            return Err(err(bad("native cohort alias contains future member")));
        }
        f(id, m)
    })?;
    Ok(())
}

/// Fixed native own-publication contribution only. Related cohort census reads
/// reserve their real local tickets; existing member bodies/paths are shared.
/// No future roster count, accepted point, fee/paid standing or tariff is asserted.
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
pub(super) struct BirthContributionRowV18 {
    spec: ComponentSpecV2,
    owned: bool,
    body: u64,
    body_alias: u8,
    scratch: bool,
}
crate::palw_native_resource_v1::wire::wire_struct!(BirthContributionRowV18;ComponentSpecV2,bool,u64,u8,bool);
crate::palw_native_resource_v1::native_struct!(BirthContributionRowV18;spec,owned,body,body_alias,scratch);
impl BirthContributionRowV18 {
    pub(super) fn spec_v18(&self) -> ComponentSpecV2 {
        self.spec
    }
    pub(super) fn owned_v18(&self) -> bool {
        self.owned
    }
    pub(super) fn body_bytes_v18(&self) -> u64 {
        self.body
    }
    // Group1 is the SAME Source8 native original body. Group2 is the same
    // snapshot body in SCRATCH and SNAPSHOTS; literal bytes publish only once.
    pub(super) fn body_alias_group_v18(&self) -> u8 {
        self.body_alias
    }
    pub(super) fn transient_scratch_v18(&self) -> bool {
        self.scratch
    }
}
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
pub(super) struct NativeBirthContributionGeometryV18 {
    needed: bool,
    rows: [BirthContributionRowV18; 5],
    a2: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(NativeBirthContributionGeometryV18;bool,[BirthContributionRowV18;5],u64);
crate::palw_native_resource_v1::native_struct!(NativeBirthContributionGeometryV18;needed,rows,a2);
impl NativeBirthContributionGeometryV18 {
    pub(super) fn needed_v18(&self) -> bool {
        self.needed
    }
    pub(super) fn rows_v18(&self) -> &[BirthContributionRowV18; 5] {
        &self.rows
    }
    pub(super) fn a2_bytes_v18(&self) -> u64 {
        self.a2
    }
    pub(super) fn original_body_aliases_source8_v18(&self) -> bool {
        self.needed
    }
}
pub(super) fn quote_birth_contribution_scoped_geometry_v15(
    req: &super::palw_tir_public_fold_v1::PreparedOriginalRequirementsV1,
) -> Result<NativeBirthContributionGeometryV18, PalwStateV2Error> {
    let original = req.frame_v1().original_v1();
    if crate::palw_freeprompt_v3::fp_claim_id_v3(original) != req.frame_v1().plan_v1().original_claim {
        return Err(err(bad("cohort contribution needs native FP preparation")));
    }
    let needed = original.execution_root != Hash64::default();
    let n = if needed { super::palw_tir_public_fold_v1::native_fresh_fp_claim_codec_v11() } else { 0 };
    // Canonical private field streams only: not an accepted future Source,
    // Snapshot or point. The tuple stream is identical to each declared codec.
    let zero = Hash64::default();
    let point = (&zero, 0u64, 0u64, 0u64);
    let stored = (&zero, 0u64, 0u64);
    let root = (&spec(MEMBERS), Some(&zero), 0u64);
    let rv = (&zero, 0u64);
    let current =
        borsh::object_length(&(15u16, &zero, &zero, &root, &point, true)).map_err(|_| err(bad("cohort current shape codec")))? as u64;
    let snapshot = borsh::object_length(&(15u16, &zero, &stored, &point, &zero, &root, &rv))
        .map_err(|_| err(bad("cohort snapshot shape codec")))? as u64;
    let roots = [root];
    let literals = [rv, rv];
    let a2 =
        borsh::object_length(&(2u16, roots.as_slice(), literals.as_slice())).map_err(|_| err(bad("cohort A2 shape codec")))? as u64;

    let rows = [
        BirthContributionRowV18 { spec: spec(MEMBERS), owned: true, body: n, body_alias: 1, scratch: false },
        BirthContributionRowV18 {
            spec: spec(ROOTS),
            owned: false,
            body: if needed { current } else { 0 },
            body_alias: 3,
            scratch: false,
        },
        BirthContributionRowV18 {
            spec: spec(SNAPSHOTS),
            owned: false,
            body: if needed { snapshot } else { 0 },
            body_alias: 2,
            scratch: false,
        },
        BirthContributionRowV18 { spec: spec(SCRATCH), owned: false, body: n, body_alias: 1, scratch: true },
        BirthContributionRowV18 {
            spec: spec(SCRATCH),
            owned: false,
            body: if needed { snapshot } else { 0 },
            body_alias: 2,
            scratch: true,
        },
    ];
    Ok(NativeBirthContributionGeometryV18 { needed, rows, a2: if needed { a2 } else { 0 } })
}
