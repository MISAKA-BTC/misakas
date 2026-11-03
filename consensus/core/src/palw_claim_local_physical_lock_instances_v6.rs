//! ISOLATED state_v2 child candidate; not registered or activated.
//! Actual physical native lock generations, not claim-admission authority.
//! A source population references shared physical instances, never raw slots.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_resource_v1::{NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_panel_var_v1::PalwSlashableLockV1 as Lock;
use borsh::{BorshDeserialize, BorshSerialize};
const SLOT: u32 = 1;
const INSTANCE: u32 = 2;
const MEMBER: u32 = 3;
const POPULATION: u32 = 4;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 7, table, schema: 6, key_bytes: 64, value_bytes: u64::MAX }
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
        _ => bad("physical lock journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("physical lock encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out).map_err(|_| bad("physical lock encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("physical lock typed body codec"),
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
    let mut w = Writer(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/physical-lock/v6").to_state());
    domain.serialize(&mut w).map_err(|_| bad("physical domain"))?;
    v.serialize(&mut w).map_err(|_| bad("physical identity"))?;
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
/// Stored claim tuple can name an Attempt's carrying block. It is intentionally
/// NOT an accepting-point PalwBlockContext and cannot authenticate an E.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) struct StoredClaimBirthV6 {
    pub accepted_block: BlockHash,
    pub accepted_daa: u64,
    pub accepted_blue_score: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(StoredClaimBirthV6;BlockHash,u64,u64);
impl StoredClaimBirthV6 {
    fn of(c: &PalwClaimStateV2) -> Self {
        Self { accepted_block: c.accepted_block, accepted_daa: c.accepted_daa, accepted_blue_score: c.accepted_blue_score }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum OriginV6 {
    Born { source: Hash64, stored_birth: StoredClaimBirthV6 },
    // Explicit observation of an already-existing physical row. No owner birth
    // is guessed from whichever claim currently occupies its raw ID.
    ObservedLegacyPhysical,
}
impl NativeWireScanV1 for OriginV6 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0 => Hash64::scan_native_wire_v1(r)?.merge(StoredClaimBirthV6::scan_native_wire_v1(r)?),
            1 => Ok(Default::default()),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "physical origin")),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct InstanceV6 {
    version: u16,
    id: Hash64,
    key: (PalwBondKeyV2, Hash64),
    origin: OriginV6,
    created_at: PalwBlockContextV2,
    ordinal: u64,
    current: Option<Lock>,
    references: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(InstanceV6;u16,Hash64,(PalwBondKeyV2,Hash64),OriginV6,PalwBlockContextV2,u64,Option<Lock>,u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct SlotV6 {
    instance: Hash64,
    key: (PalwBondKeyV2, Hash64),
}
crate::palw_native_resource_v1::wire::wire_struct!(SlotV6;Hash64,(PalwBondKeyV2,Hash64));
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct MemberV6 {
    instance: Hash64,
    key: (PalwBondKeyV2, Hash64),
}
crate::palw_native_resource_v1::wire::wire_struct!(MemberV6;Hash64,(PalwBondKeyV2,Hash64));
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct PopulationV6 {
    version: u16,
    budget: Hash64,
    source: Hash64,
    accepting_point: PalwBlockContextV2,
    stored_birth: StoredClaimBirthV6,
    members: ComponentRootV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(PopulationV6;u16,Hash64,Hash64,PalwBlockContextV2,StoredClaimBirthV6,ComponentRootV2);
fn slot(s: &PalwChainStateV2, key: (PalwBondKeyV2, Hash64)) -> Result<Option<SlotV6>, ContextErrorV2> {
    let p = lookup::<SlotV6>(s, SLOT, None, hash(1, &key)?)?;
    if p.as_ref().is_some_and(|p| p.key != key) {
        return Err(bad("physical slot collision"));
    }
    Ok(p.map(|p| *p))
}
fn instance(s: &PalwChainStateV2, id: Hash64) -> Result<NativeRetainedV1<InstanceV6>, ContextErrorV2> {
    let r = lookup::<InstanceV6>(s, INSTANCE, None, id)?.ok_or(ContextErrorV2::Unavailable("physical instance missing"))?;
    if r.version != 6
        || r.id != id
        || r.id != hash(2, &(r.key, r.origin, r.created_at, r.ordinal))?
        || r.current.is_some_and(|l| l.claim != r.key.1)
    {
        return Err(bad("physical instance identity"));
    }
    Ok(r)
}
fn put_instance(b: &mut TransitionBuilder<'_>, r: &InstanceV6) -> Result<(), ContextErrorV2> {
    put(b, INSTANCE, None, r.id, Some(r))?;
    Ok(())
}
fn fresh(b: &mut TransitionBuilder<'_>, key: (PalwBondKeyV2, Hash64), origin: OriginV6, lock: Lock) -> Result<Hash64, ContextErrorV2> {
    let point = b.claim_local_lock_point_v6.ok_or(bad("physical capture lacks actual executing point"))?;
    let ordinal = u64::try_from(b.entries.len()).map_err(|_| bad("physical journal ordinal overflow"))?;
    let id = hash(2, &(key, origin, point, ordinal))?;
    if slot(&b.state, key)?.is_some() || lookup::<InstanceV6>(&b.state, INSTANCE, None, id)?.is_some() {
        return Err(bad("physical mint reused an instance"));
    }
    let r = InstanceV6 { version: 6, id, key, origin, created_at: point, ordinal, current: Some(lock), references: 0 };
    put_instance(b, &r)?;
    put(b, SLOT, None, hash(1, &key)?, Some(&SlotV6 { instance: id, key }))?;
    Ok(id)
}
fn observe_legacy(b: &mut TransitionBuilder<'_>, key: (PalwBondKeyV2, Hash64), lock: Lock) -> Result<Hash64, ContextErrorV2> {
    if let Some(p) = slot(&b.state, key)? {
        let r = instance(&b.state, p.instance)?;
        if r.current != Some(lock) {
            return Err(bad("physical observation disagrees with native old row"));
        }
        return Ok(p.instance);
    }
    fresh(b, key, OriginV6::ObservedLegacyPhysical, lock)
}
/// Invoke only after the three creation helpers' Q4 and collateral checks,
/// immediately before their actual native writer. Existing-slot Q4 does not
/// call this, does not create another generation and does not transfer origin.
pub(super) fn mint_and_write_v6(
    b: &mut TransitionBuilder<'_>,
    key: (PalwBondKeyV2, Hash64),
    claim: &PalwClaimStateV2,
    lock: Lock,
) -> Result<(), PalwStateV2Error> {
    if b.claim_local_lock_point_v6.is_none() {
        return b.write_slashable_lock(key, Some(lock));
    }
    atomic(b, |b| {
        if b.state.slashable_locks.contains_key(&key) || lock.claim != key.1 {
            return Err(bad("physical mint is not an actual new native slot"));
        }
        fresh(b, key, OriginV6::Born { source: key.1, stored_birth: StoredClaimBirthV6::of(claim) }, lock)?;
        // Source reference is established while native is still absent. Every
        // fallible metadata operation remains inside this outer rollback batch.
        super::palw_claim_local_prefix_financial_archive_v4::reference_actual_mint_v4(b, key, claim, lock).map_err(native_err)?;
        // Reserve failures in the native writer leave no staged pointer. On success
        // its final native mutation/journal append are infallible after observation.
        b.write_slashable_lock(key, Some(lock)).map_err(native_err)
    })
    .map_err(err)
}
/// Called after reserve_map_row and old read, BEFORE old==new. Every actual
/// update/deletion observes exactly the existing physical instance. No generic
/// current-claim lookup, population copying or deletion-as-expiry inference.
pub(super) fn observe_existing_v6(
    b: &mut TransitionBuilder<'_>,
    key: (PalwBondKeyV2, Hash64),
    old: Option<Lock>,
    new: Option<Lock>,
) -> Result<(), PalwStateV2Error> {
    if b.claim_local_lock_point_v6.is_none() {
        return Ok(());
    }
    atomic(b, |b| {
        (|| {
            let id = match old {
                Some(lock) => observe_legacy(b, key, lock)?,
                None => {
                    if let Some(lock) = new {
                        let p = slot(&b.state, key)?.ok_or(bad("new physical row bypassed actual mint helper"))?;
                        let r = instance(&b.state, p.instance)?;
                        if r.current != Some(lock)
                            || Some(r.created_at) != b.claim_local_lock_point_v6
                            || !matches!(r.origin,OriginV6::Born{source,..} if source==key.1)
                            || b.state.slashable_locks.contains_key(&key)
                        {
                            return Err(bad("new physical pointer differs from staged actual mint"));
                        }
                        // refs may already be positive: actual E populations
                        // join BEFORE native mutation, never as a fallible tail.

                        return Ok(());
                    }
                    if slot(&b.state, key)?.is_some() {
                        return Err(bad("absent native slot has live physical pointer"));
                    }
                    return Ok(());
                }
            };
            let r = instance(&b.state, id)?;
            if r.current != old {
                return Err(bad("physical writer old state differs"));
            }
            let mut r = *r;
            r.current = new;
            if new.is_none() {
                put::<SlotV6>(b, SLOT, None, hash(1, &key)?, None)?;
            }
            if new.is_none() && r.references == 0 {
                put::<InstanceV6>(b, INSTANCE, None, id, None)?;
            } else {
                put_instance(b, &r)?;
            }
            Ok(())
        })()?;
        // Metadata publication may consume the journal slot admitted by the
        // native writer before this hook. Re-admit its final append AFTER all
        // aux writes, while failure can still roll this metadata batch back.
        if old != new {
            b.reserve_mutation_sequence_v1(0, 1).map_err(native_err)?;
        }
        Ok(())
    })
    .map_err(err)
}
fn population(s: &PalwChainStateV2, budget: Hash64) -> Result<Option<NativeRetainedV1<PopulationV6>>, ContextErrorV2> {
    let p = lookup::<PopulationV6>(s, POPULATION, None, budget)?;
    if let Some(p) = &p
        && (p.version != 6 || p.budget != budget || p.members != root(s, MEMBER, Some(budget))?)
    {
        return Err(bad("physical source population root differs"));
    }
    Ok(p)
}
fn add_member(b: &mut TransitionBuilder<'_>, p: &mut PopulationV6, id: Hash64) -> Result<(), ContextErrorV2> {
    if let Some(m) = lookup::<MemberV6>(&b.state, MEMBER, Some(p.budget), id)? {
        if m.instance != id {
            return Err(bad("physical member identity"));
        }
        return Ok(());
    }
    let r = instance(&b.state, id)?;
    if r.key.1 != p.source {
        return Err(bad("physical member source differs"));
    }
    let mut r = *r;
    r.references = r.references.checked_add(1).ok_or(bad("physical reference overflow"))?;
    put_instance(b, &r)?;
    p.members = put(b, MEMBER, Some(p.budget), id, Some(&MemberV6 { instance: id, key: r.key }))?;
    Ok(())
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn inventory(p: &PopulationV6) -> Result<NativeRetainedV1<store::ArchiveReferencesV2>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentRootV2>(1).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<RowValueV2>(1).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let encoded_population = encode(p)?;
    let a = store::ArchiveReferencesV2 { version: 2, components: vec![p.members], literals: vec![value(&encoded_population)] };
    Ok(NativeRetainedV1::from_owner_reserved_v1(a, lease))
}
fn inventory_id(a: &store::ArchiveReferencesV2) -> Result<Hash64, ContextErrorV2> {
    let raw = encode(a)?;
    Ok(value(&raw).data)
}
fn put_population(b: &mut TransitionBuilder<'_>, p: &PopulationV6, old: Option<&PopulationV6>) -> Result<(), ContextErrorV2> {
    let a = inventory(p)?;
    let id = inventory_id(&a)?;
    // The new A2 literal must already exist when the native archive writer
    // classifies it. Its current component root and the old A2 keep both
    // versions live until the new pin has been installed atomically.
    put(b, POPULATION, None, p.budget, Some(p))?;
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &akey(id), Some(&*a)).map_err(native_err)?;
    if let Some(old) = old {
        let old_inventory = inventory(old)?;
        let old_id = inventory_id(&old_inventory)?;
        if old_id != id {
            b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(old_id), None)
                .map_err(native_err)?;
        }
    }
    Ok(())
}
/// Capture the actual COMPLETE native keys before persist_panel_liability's
/// first redate. The A4 adapter validates actual E/claim attribution before this
/// call, and supplies the accepting point; never derive it from carrying birth.
/// Repeated mint captures union membership, so deleted/replaced instances are
/// not silently dropped. Only actual source-E closure releases membership.
pub(super) fn capture_population_v6(
    b: &mut TransitionBuilder<'_>,
    budget: Hash64,
    source: Hash64,
    accepting_point: PalwBlockContextV2,
    claim: &PalwClaimStateV2,
    keys: &[(PalwBondKeyV2, Hash64)],
) -> Result<ComponentRootV2, PalwStateV2Error> {
    if b.claim_local_lock_point_v6.is_none() {
        return Err(err(bad("physical source capture before actual fence")));
    }
    atomic(b, |b| {
        let old = population(&b.state, budget)?;
        if old.is_none() && root(&b.state, MEMBER, Some(budget))?.rows != 0 {
            return Err(bad("orphan physical population root"));
        }
        let mut p = old.as_deref().copied().unwrap_or(PopulationV6 {
            version: 6,
            budget,
            source,
            accepting_point,
            stored_birth: StoredClaimBirthV6::of(claim),
            members: root(&b.state, MEMBER, Some(budget))?,
        });
        if (p.source, p.accepting_point, p.stored_birth) != (source, accepting_point, StoredClaimBirthV6::of(claim)) {
            return Err(bad("physical source population changed birth"));
        }
        // Keys are the existing real native collection, not a producer list. Verify
        // complete count/order here as a defense against a future incomplete caller.
        let mut actual = b.state.slashable_locks.keys().filter(|(_, c)| *c == source);
        for key in keys {
            if actual.next() != Some(key) {
                return Err(bad("physical source capture incomplete native order"));
            }
        }
        if actual.next().is_some() {
            return Err(bad("physical source capture omitted native locks"));
        }
        for key in keys {
            let lock = *b.state.slashable_locks.get(key).ok_or(bad("physical capture native key vanished"))?;
            let id = observe_legacy(b, *key, lock)?;
            add_member(b, &mut p, id)?;
        }
        put_population(b, &p, old.as_deref())?;
        Ok(p.members)
    })
    .map_err(err)
}
/// Borrow every complete source member's latest ACTUAL physical row/tombstone.
/// Updating one Q4-shared instance changes one body; no per-source fanout copy.
pub(super) fn visit_population_v6(
    s: &PalwChainStateV2,
    budget: Hash64,
    mut f: impl FnMut(Hash64, (PalwBondKeyV2, Hash64), Option<&Lock>) -> Result<(), ContextErrorV2>,
) -> Result<Option<ComponentRootV2>, ContextErrorV2> {
    let Some(p) = population(s, budget)? else {
        return Ok(None);
    };
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, p.members, |key, v| {
        let raw = reader.body(v)?;
        let m = decode::<MemberV6>(&raw)?;
        if key != m.instance.as_byte_slice() {
            return Err(bad("physical member leaf identity"));
        }
        let r = instance(s, m.instance)?;
        if m.key != r.key || r.key.1 != p.source || r.references == 0 {
            return Err(bad("physical member/instance binding"));
        }
        f(r.id, r.key, r.current.as_ref())
    })?;
    Ok(Some(p.members))
}
/// This is ONLY the all-source-physical-lock part of the existing predicate.
/// Caller must also prove born-qualified court/staged closure, liability's own
/// clocks and real epoch-crossing/audit policy. None is missing, never success.
pub(super) fn population_locks_prunable_v6(
    s: &PalwChainStateV2,
    budget: Hash64,
    now: u64,
    settled_now: u64,
    depth: Option<u64>,
    window_court: u64,
) -> Result<Option<bool>, ContextErrorV2> {
    let mut all = true;
    let root = visit_population_v6(s, budget, |_, _, lock| {
        if let Some(l) = lock {
            all &= crate::palw_panel_var_v1::palw_panel_obligation_prunable_v1(
                l.expiry_daa,
                l.settled_at_final,
                now,
                settled_now,
                depth,
                window_court,
            )
        }
        Ok(())
    })?;
    Ok(root.map(|_| all))
}
/// Invoked by the existing E collector AFTER all genuine duty-release checks.
/// There is no independent time cap, unsigned fee or guessed native expiry.
/// Remove exactly this population's references; keep live-slot instances and
/// other sources' refs. Ordinary N/B reachability sweep reclaims old versions.
pub(super) fn release_population_v6(b: &mut TransitionBuilder<'_>, budget: Hash64) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let Some(p) = population(&b.state, budget)? else {
            return Ok(());
        };
        let n = usize::try_from(p.members.rows).map_err(|_| bad("physical population count"))?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Hash64>(p.members.rows).map_err(ContextErrorV2::Resource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut ids = Vec::with_capacity(n);
        visit_population_v6(&b.state, budget, |id, _, _| {
            ids.push(id);
            Ok(())
        })?;
        for id in ids {
            let r = instance(&b.state, id)?;
            let mut r = *r;
            r.references = r.references.checked_sub(1).ok_or(bad("physical negative references"))?;
            let live = slot(&b.state, r.key)?.is_some_and(|p| p.instance == id);
            if live && r.current.is_none() {
                return Err(bad("physical tombstone has current pointer"));
            }
            if !live && r.current.is_some() {
                return Err(bad("physical detached instance was not tombstoned"));
            }
            if r.references == 0 && !live {
                put::<InstanceV6>(b, INSTANCE, None, id, None)?;
            } else {
                put_instance(b, &r)?;
            }
            put::<MemberV6>(b, MEMBER, Some(budget), id, None)?;
        }
        // Remove even the empty owned-I root: one empty map per expired source is
        // permanent growth. Exact old aux bytes in journal provide reorg restoration.
        let mut key = [0u8; 84];
        key[0] = b'I';
        key[1..3].copy_from_slice(&2u16.to_le_bytes());
        key[3..67].copy_from_slice(budget.as_byte_slice());
        spec(MEMBER).serialize(&mut &mut key[67..]).map_err(|_| bad("physical owned current key"))?;
        b.write_kernel_row_encoded_v1::<ComponentRootV2>(PALW_CLAIM_LOCAL_TABLE_V1, &key, None).map_err(native_err)?;
        put::<PopulationV6>(b, POPULATION, None, budget, None)?;
        let old_inventory = inventory(&p)?;
        let id = inventory_id(&old_inventory)?;
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(id), None).map_err(native_err)?;
        Ok(())
    })
    .map_err(err)
}
/// Aux-only batch rollback uses existing journal-owned buffers. Native lock
/// mutation occurs after successful observation; its outer attempt checkpoint
/// remains the native writer's rollback owner.
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

/// A4 invokes this when a supplementary actual mint occurs after its original
/// liability capture. The exact existing E budget is supplied by A4, never
/// reverse-engineered from the generic raw key. Q4 retains can use this too;
/// that adds a population reference and never creates/reassigns the instance.
pub(super) fn reference_current_v6(
    b: &mut TransitionBuilder<'_>,
    budget: Hash64,
    key: (PalwBondKeyV2, Hash64),
    claim: &PalwClaimStateV2,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let old = population(&b.state, budget)?.ok_or(ContextErrorV2::Unavailable("physical population not captured"))?;
        if old.source != key.1 || old.stored_birth != StoredClaimBirthV6::of(claim) {
            return Err(bad("physical reference source/birth differs"));
        }
        let native =
            *b.state.slashable_locks.get(&key).ok_or(ContextErrorV2::Unavailable("physical reference actual native slot missing"))?;
        let instance_id = observe_legacy(b, key, native)?;
        let r = instance(&b.state, instance_id)?;
        if r.current.as_ref() != b.state.slashable_locks.get(&key) {
            return Err(bad("physical reference native row differs"));
        }
        let mut next = *old;
        add_member(b, &mut next, instance_id)?;
        put_population(b, &next, Some(&old))?;
        Ok(())
    })
    .map_err(err)
}
/// Restore/import semantic closure in addition to the generic N/B/I/A codec
/// closure. Recompute all population reference counts from complete streams;
/// no producer count, partial-world state, or current raw-claim birth is used.
pub(super) fn validate_physical_instances_v6(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    let reader = StateNodesV2(s);
    let ir = root(s, INSTANCE, None)?;
    let bytes = ir
        .rows
        .checked_mul(crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<Hash64, u64>() as u64)
        .ok_or(bad("physical restore reference reservation overflow"))?;
    let _lease = reserve_native_requests_v1(bytes).map_err(ContextErrorV2::Resource)?;
    let mut counts = std::collections::BTreeMap::<Hash64, u64>::new();
    nodes::complete_rows(&reader, ir, |key, v| {
        let raw = reader.body(v)?;
        let r = decode::<InstanceV6>(&raw)?;
        if key != r.id.as_byte_slice() {
            return Err(bad("physical instance leaf key"));
        }
        let checked = instance(s, r.id)?;
        if *r != *checked {
            return Err(bad("physical instance lookup disagrees"));
        }
        counts.insert(r.id, 0);
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, SLOT, None)?, |key, v| {
        let raw = reader.body(v)?;
        let p = decode::<SlotV6>(&raw)?;
        if key != hash(1, &p.key)?.as_byte_slice() {
            return Err(bad("physical slot leaf key"));
        }
        let r = instance(s, p.instance)?;
        if r.key != p.key || r.current.is_none() || r.current.as_ref() != s.slashable_locks.get(&p.key) {
            return Err(bad("physical current slot/native binding"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, POPULATION, None)?, |key, v| {
        let raw = reader.body(v)?;
        let p = decode::<PopulationV6>(&raw)?;
        if key != p.budget.as_byte_slice() {
            return Err(bad("physical population leaf key"));
        }
        let checked = population(s, p.budget)?.ok_or(bad("physical population missing"))?;
        if *p != *checked {
            return Err(bad("physical population lookup disagrees"));
        }
        let a = inventory(&p)?;
        let id = inventory_id(&a)?;
        let route = s.kernel_route.as_ref().ok_or(ContextErrorV2::Unavailable("physical restore route missing"))?;
        let _key = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
        let stored = route
            .aux
            .get(&(PALW_CLAIM_LOCAL_TABLE_V1, akey(id).to_vec()))
            .ok_or(ContextErrorV2::Unavailable("physical population inventory missing"))?;
        crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&*a, stored)
            .map_err(|_| bad("physical population inventory differs"))?;
        visit_population_v6(s, p.budget, |id, _, _| {
            let n = counts.get_mut(&id).ok_or(bad("physical population names absent instance"))?;
            *n = n.checked_add(1).ok_or(bad("physical restored reference overflow"))?;
            Ok(())
        })?;
        Ok(())
    })?;
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'I') {
                continue;
            }
            let rooted = decode::<ComponentRootV2>(raw)?;
            if rooted.spec.owner != 7 {
                continue;
            }
            if rooted.spec != spec(rooted.spec.table) {
                return Err(bad("physical root complete specification"));
            }
            if rooted.spec.table == MEMBER {
                if key.len() != 84 || key[1..3] != [2, 0] {
                    return Err(bad("physical membership requires owned root"));
                }
                let mut bytes = [0; 64];
                bytes.copy_from_slice(&key[3..67]);
                let budget = Hash64::from_bytes(bytes);
                let p = population(s, budget)?.ok_or(bad("orphan physical membership root"))?;
                if p.members != *rooted {
                    return Err(bad("physical owned root differs from source"));
                }
            } else if !matches!(rooted.spec.table, SLOT | INSTANCE | POPULATION) || key.len() != 18 {
                return Err(bad("physical global root namespace"));
            }
        }
    }
    for (id, count) in counts {
        let r = instance(s, id)?;
        if r.references != count {
            return Err(bad("physical reference count differs from complete population"));
        }
        let live = slot(s, r.key)?.is_some_and(|p| p.instance == id);
        if r.current.is_some() != live || (!live && count == 0) {
            return Err(bad("physical instance live/tombstone liveness"));
        }
    }
    Ok(())
}

/// Dynamic source membership root for the A4 borrowed financial view. The
/// population is bound to the actual E accepting point separately from the
/// original stored carrying tuple. A later supplementary member updates this
/// root without cloning the liability or all other physical instance bodies.
pub(super) fn population_for_export_v6(
    state: &PalwChainStateV2,
    e: &crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportReservationV1,
) -> Result<Option<ComponentRootV2>, ContextErrorV2> {
    let Some(p) = population(state, e.budget_id)? else {
        return Ok(None);
    };
    if p.source != e.source_claim || p.accepting_point != e.point {
        return Err(bad("physical population substituted E accepting cohort"));
    }
    Ok(Some(p.members))
}

/// The source fee/retention owner uses these derived fixed-schema envelopes in
/// its EXISTING signed fee profile and actual finite mutation/lifetime budget.
/// This function does not charge a fee, add an unsigned contract or invent a
/// cap. Membership multiplicity is the complete actual native-key count.
pub(super) struct PhysicalGeometryV6 {
    pub instance: nodes::ContextMutationGeometryV2,
    pub slot: nodes::ContextMutationGeometryV2,
    pub member: nodes::ContextMutationGeometryV2,
    pub population: nodes::ContextMutationGeometryV2,
    pub archive_wire: u64,
    pub archive_key: u64,
    pub owned_index_key: u64,
}
pub(super) fn metadata_geometry_v6(
    key: (PalwBondKeyV2, Hash64),
    claim: &PalwClaimStateV2,
    point: PalwBlockContextV2,
    lock: Lock,
    budget: Hash64,
) -> Result<PhysicalGeometryV6, ContextErrorV2> {
    let origin = OriginV6::Born { source: key.1, stored_birth: StoredClaimBirthV6::of(claim) };
    let id = hash(2, &(key, origin, point, 0u64))?;
    let r = InstanceV6 { version: 6, id, key, origin, created_at: point, ordinal: 0, current: Some(lock), references: 0 };
    // Some(node) is the maximal fixed root shape; this is a wire geometry
    // sample, not a published/fabricated authority or a claim population.
    let members = ComponentRootV2 { spec: spec(MEMBER), node: Some(id), rows: 1 };
    let p = PopulationV6 {
        version: 6,
        budget,
        source: key.1,
        accepting_point: point,
        stored_birth: StoredClaimBirthV6::of(claim),
        members,
    };
    let instance_bytes = borsh::object_length(&r).map_err(|_| bad("physical instance geometry"))? as u64;
    let slot_bytes = borsh::object_length(&SlotV6 { instance: id, key }).map_err(|_| bad("physical slot geometry"))? as u64;
    let member_bytes = borsh::object_length(&MemberV6 { instance: id, key }).map_err(|_| bad("physical member geometry"))? as u64;
    let population_bytes = borsh::object_length(&p).map_err(|_| bad("physical population geometry"))? as u64;
    let a = inventory(&p)?;
    let archive_wire = borsh::object_length(&*a).map_err(|_| bad("physical inventory geometry"))? as u64;
    Ok(PhysicalGeometryV6 {
        instance: nodes::mutation_geometry(spec(INSTANCE), instance_bytes)?,
        slot: nodes::mutation_geometry(spec(SLOT), slot_bytes)?,
        member: nodes::mutation_geometry(spec(MEMBER), member_bytes)?,
        population: nodes::mutation_geometry(spec(POPULATION), population_bytes)?,
        archive_wire,
        archive_key: 65,
        owned_index_key: 84,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    pub(super) fn point(n: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h(n), daa_score: n, blue_score: n, subsidy: 0 }
    }
    pub(super) fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 100, 10, 20, 50, 1000, h(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_claim_local_settlement_from_daa(Some(0))
    }
    pub(super) fn extras() -> PalwTransitionExtrasV1 {
        PalwTransitionExtrasV1 {
            claim_local_rule_context_v1: Some(([11; 32], [12; 32])),
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(10),
                ruleset_digest: h(11),
                chain_genesis: h(12),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    pub(super) fn prepared(p: &PalwStateParamsV2, e: &PalwTransitionExtrasV1) -> PalwChainStateV2 {
        apply_palw_transition_v2_with_extras(&PalwChainStateV2::genesis(), p, &point(1), &[], None, false, false, false, false, e)
            .unwrap()
            .0
    }
    pub(super) fn key() -> (PalwBondKeyV2, Hash64) {
        (PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: h(9), index: 0 }), h(8))
    }
    pub(super) fn claim(n: u64) -> PalwClaimStateV2 {
        PalwClaimStateV2 {
            source: PalwClaimSourceV2::Attempt,
            class_id: h(20),
            bond: key().0,
            pwu: 10,
            accepted_daa: n,
            rebound_daa: None,
            accepted_blue_score: n,
            accepted_block: h(n),
            trace_root: h(21),
            output_root: h(22),
            execution_root: h(23),
            trace_chunk_count: 4,
            trace_retention_daa: 1000,
            reserved: 100,
            immature_contribution: 0,
            escrowed_reward: 0,
            work_leaves: 0,
            work_id: None,
            phase: PalwClaimPhaseV2::PanelBound { bound_daa: n },
            rights_reserved: 0,
            job_identity: h(24),
            rcore: PalwClaimRcoreV1::default(),
        }
    }
    pub(super) fn lock() -> Lock {
        Lock {
            claim: key().1,
            amount: 5,
            expiry_daa: 100,
            settled_at_final: 10,
            attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
            segments: 0,
        }
    }
    // Storage/lifecycle unit fixtures only. They neither install fake selected
    // approval nor assert E admission; genuine paid-path conformance is separate.
    #[test]
    fn rfc23s2_physical_same_key_same_bytes_remint_has_distinct_generation_and_rollback() {
        let p = params();
        let e = extras();
        let s = prepared(&p, &e);
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
        b.claim_local_lock_point_v6 = Some(point(2));
        let c = claim(1);
        mint_and_write_v6(&mut b, key(), &c, lock()).unwrap();
        capture_population_v6(&mut b, h(100), key().1, point(1), &c, &[key()]).unwrap();
        let old = slot(&b.state, key()).unwrap().unwrap().instance;
        let root = b.state.state_root();
        let checkpoint = b.try_checkpoint().unwrap();
        b.write_slashable_lock(key(), None).unwrap();
        mint_and_write_v6(&mut b, key(), &c, lock()).unwrap();
        let new = slot(&b.state, key()).unwrap().unwrap().instance;
        assert_ne!(old, new, "same birth and equal serialized bytes do not revive the deleted physical instance");
        assert_eq!(instance(&b.state, old).unwrap().current, None);
        let mut seen = 0;
        visit_population_v6(&b.state, h(100), |id, _, row| {
            seen += 1;
            assert_eq!(id, old);
            assert!(row.is_none());
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, 1);
        validate_physical_instances_v6(&b.state).unwrap();
        b.restore(checkpoint);
        assert_eq!(b.state.state_root(), root);
        assert_eq!(slot(&b.state, key()).unwrap().unwrap().instance, old);
        assert_eq!(b.claim_local_lock_point_v6, Some(point(2)));
        validate_physical_instances_v6(&b.state).unwrap();
    }
    #[test]
    fn rfc23s2_physical_q4_instance_shared_by_source_populations_released_exactly_once() {
        let p = params();
        let e = extras();
        let s = prepared(&p, &e);
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
        b.claim_local_lock_point_v6 = Some(point(2));
        let c = claim(1);
        mint_and_write_v6(&mut b, key(), &c, lock()).unwrap();
        let old = slot(&b.state, key()).unwrap().unwrap().instance;
        capture_population_v6(&mut b, h(100), key().1, point(1), &c, &[key()]).unwrap();
        // Later cohort retains the existing native row under Q4. It references
        // exactly that instance; it does not run the actual creation hook.
        capture_population_v6(&mut b, h(101), key().1, point(2), &claim(2), &[key()]).unwrap();
        assert_eq!(instance(&b.state, old).unwrap().references, 2);
        let extended = Lock { expiry_daa: 200, ..lock() };
        b.write_slashable_lock(key(), Some(extended)).unwrap();
        for budget in [h(100), h(101)] {
            visit_population_v6(&b.state, budget, |id, _, row| {
                assert_eq!(id, old);
                assert_eq!(row, Some(&extended));
                Ok(())
            })
            .unwrap();
        }
        b.write_slashable_lock(key(), None).unwrap();
        mint_and_write_v6(&mut b, key(), &claim(3), lock()).unwrap();
        let new = slot(&b.state, key()).unwrap().unwrap().instance;
        release_population_v6(&mut b, h(100)).unwrap();
        assert_eq!(instance(&b.state, old).unwrap().references, 1);
        assert!(population(&b.state, h(100)).unwrap().is_none());
        release_population_v6(&mut b, h(101)).unwrap();
        assert!(lookup::<InstanceV6>(&b.state, INSTANCE, None, old).unwrap().is_none());
        assert_eq!(instance(&b.state, new).unwrap().current, Some(lock()));
        assert!(lookup::<PopulationV6>(&b.state, POPULATION, None, h(101)).unwrap().is_none());
        validate_physical_instances_v6(&b.state).unwrap();
        store::collect_unreachable_v2(&mut b).unwrap();
        validate_physical_instances_v6(&b.state).unwrap();
    }
    #[test]
    fn rfc23s2_physical_legacy_equal_write_records_observation_and_source_uses_both_clocks() {
        let p = params();
        let e = extras();
        let s = prepared(&p, &e);
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
        // Real legacy native writer before scoped instrumentation is armed.
        b.write_slashable_lock(key(), Some(lock())).unwrap();
        assert!(slot(&b.state, key()).unwrap().is_none());
        b.claim_local_lock_point_v6 = Some(point(2));
        b.write_slashable_lock(key(), Some(lock())).unwrap();
        let id = slot(&b.state, key()).unwrap().unwrap().instance;
        assert_eq!(instance(&b.state, id).unwrap().origin, OriginV6::ObservedLegacyPhysical);
        capture_population_v6(&mut b, h(100), key().1, point(1), &claim(1), &[key()]).unwrap();
        assert_eq!(population_locks_prunable_v6(&b.state, h(100), 115, 10, Some(20), 10).unwrap(), Some(false));
        assert_eq!(population_locks_prunable_v6(&b.state, h(100), 120, 10, Some(20), 10).unwrap(), Some(true));
        assert_eq!(population_locks_prunable_v6(&b.state, h(101), 120, 10, Some(20), 10).unwrap(), None);
        validate_physical_instances_v6(&b.state).unwrap();
    }
}

/// A4 callback from mint_and_write_v6 after fresh(), BEFORE actual native write.
/// Exact existing E owner is supplied by A4. This accepts staged born metadata
/// only; it cannot convert an unrelated existing or observed legacy physical
/// slot into a new mint. Native is absent through every fallible operation.
pub(super) fn reference_minted_v6(
    b: &mut TransitionBuilder<'_>,
    budget: Hash64,
    key: (PalwBondKeyV2, Hash64),
    claim: &PalwClaimStateV2,
    expected: Lock,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let old =
            population(&b.state, budget)?.ok_or(ContextErrorV2::Unavailable("staged physical source population not captured"))?;
        if old.source != key.1 || old.stored_birth != StoredClaimBirthV6::of(claim) || expected.claim != key.1 {
            return Err(bad("staged physical source population actual birth differs"));
        }
        if b.state.slashable_locks.contains_key(&key) {
            return Err(bad("staged physical mint already has native row"));
        }
        let p = slot(&b.state, key)?.ok_or(bad("staged physical mint pointer missing"))?;
        let r = instance(&b.state, p.instance)?;
        let actual_origin = OriginV6::Born { source: key.1, stored_birth: StoredClaimBirthV6::of(claim) };
        if r.key != key
            || r.origin != actual_origin
            || r.current != Some(expected)
            || Some(r.created_at) != b.claim_local_lock_point_v6
        {
            return Err(bad("staged physical reference differs from actual mint"));
        }
        let mut next = *old;
        add_member(b, &mut next, p.instance)?;
        put_population(b, &next, Some(&old))?;
        Ok(())
    })
    .map_err(err)
}

#[cfg(test)]
mod staged_reference_tests {
    use super::*;
    #[test]
    fn rfc23s2_physical_staged_population_reference_precedes_native_and_rolls_back_failure() {
        let p = tests::params();
        let e = tests::extras();
        let s = tests::prepared(&p, &e);
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
        b.claim_local_lock_point_v6 = Some(tests::point(2));
        let key = tests::key();
        let c = tests::claim(1);
        let lock = tests::lock();
        let budget = tests::h(100);
        let population = PopulationV6 {
            version: 6,
            budget,
            source: key.1,
            accepting_point: tests::point(1),
            stored_birth: StoredClaimBirthV6::of(&c),
            members: root(&b.state, MEMBER, Some(budget)).unwrap(),
        };
        put_population(&mut b, &population, None).unwrap();
        let before = b.state.state_root();
        let failed = atomic(&mut b, |b| {
            fresh(b, key, OriginV6::Born { source: key.1, stored_birth: StoredClaimBirthV6::of(&c) }, lock)?;
            reference_minted_v6(b, budget, key, &c, Lock { amount: lock.amount + 1, ..lock }).map_err(native_err)
        });
        assert!(failed.is_err());
        assert_eq!(b.state.state_root(), before);
        assert!(slot(&b.state, key).unwrap().is_none());
        assert!(!b.state.slashable_locks.contains_key(&key));
        atomic(&mut b, |b| {
            fresh(b, key, OriginV6::Born { source: key.1, stored_birth: StoredClaimBirthV6::of(&c) }, lock)?;
            reference_minted_v6(b, budget, key, &c, lock).map_err(native_err)?;
            let id = slot(&b.state, key)?.unwrap().instance;
            assert_eq!(instance(&b.state, id)?.references, 1);
            assert!(!b.state.slashable_locks.contains_key(&key));
            b.write_slashable_lock(key, Some(lock)).map_err(native_err)
        })
        .unwrap();
        validate_physical_instances_v6(&b.state).unwrap();
    }
}

/// Narrow A4 restore/source-binding check, without exposing a synthesized view.
/// A4 invokes this only for its actual FreePrompt E: that source's stored birth
/// must equal the actual accepting point. General Attempt populations keep the
/// separate carrying tuple and are not reinterpreted by this FP-only predicate.
/// Missing is explicitly None, including an empty population with no P record.
pub(super) fn population_matches_source_v6(
    state: &PalwChainStateV2,
    budget: Hash64,
    source: Hash64,
    actual_point: PalwBlockContextV2,
) -> Result<Option<bool>, ContextErrorV2> {
    let Some(p) = population(state, budget)? else {
        return Ok(None);
    };
    Ok(Some(
        p.budget == budget
            && p.source == source
            && p.accepting_point == actual_point
            && (p.stored_birth.accepted_block, p.stored_birth.accepted_daa, p.stored_birth.accepted_blue_score)
                == (actual_point.block, actual_point.daa_score, actual_point.blue_score),
    ))
}
