//! ISOLATED state_v2 child candidate; not registered or activated.
//! Actual material-income receipt lineages; not claim-admission authority.
//! A source population references shared physical instances, never raw slots.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_bond_budget_v1::{PalwBudgetWriteV1, PalwMaterialIncomeV1 as Income, PalwMaterialPaymentV1 as Payment};
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_resource_v1::{NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1};
use borsh::{BorshDeserialize, BorshSerialize};
const SLOT: u32 = 1;
const INSTANCE: u32 = 2;
const MEMBER: u32 = 3;
const LINEAGE: u32 = 5;
const ANCESTRY: u32 = 6;
const CONTROL: u32 = 7;
const POPULATION: u32 = 4;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 8, table, schema: 7, key_bytes: 64, value_bytes: u64::MAX }
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
        _ => bad("material lineage journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("material lineage encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out).map_err(|_| bad("material lineage encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("material lineage typed body codec"),
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
    let mut w = Writer(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/material-lineage/v7").to_state());
    domain.serialize(&mut w).map_err(|_| bad("lineage domain"))?;
    v.serialize(&mut w).map_err(|_| bad("lineage identity"))?;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct SlotV7 {
    instance: Hash64,
}
crate::palw_native_resource_v1::wire::wire_struct!(SlotV7;Hash64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct InstanceV7 {
    id: Hash64,
    key: Hash64,
    lineage: Hash64,
    created_at: PalwBlockContextV2,
    ordinal: u64,
    initial: Hash64,
    current: Income,
}
crate::palw_native_resource_v1::wire::wire_struct!(InstanceV7;Hash64,Hash64,Hash64,PalwBlockContextV2,u64,Hash64,Income);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct LineageV7 {
    id: Hash64,
    active: u64,
    references: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(LineageV7;Hash64,u64,u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct PopulationV7 {
    version: u16,
    budget: Hash64,
    source: Hash64,
    birth: PalwBlockContextV2,
    members: ComponentRootV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(PopulationV7;u16,Hash64,Hash64,PalwBlockContextV2,ComponentRootV2);
/// Remove only an EMPTY owned current-directory pointer. Immutable N/B rows
/// stay available to authenticated archive roots and ordinary context GC.
fn drop_empty_owned(b: &mut TransitionBuilder<'_>, table: u32, scope: Hash64) -> Result<(), ContextErrorV2> {
    let current = root(&b.state, table, Some(scope))?;
    if current.rows != 0 || current.node.is_some() {
        return Err(bad("material directory still has rows"));
    }
    let mut key = [0u8; 84];
    key[0] = b'I';
    key[1..3].copy_from_slice(&2u16.to_le_bytes());
    key[3..67].copy_from_slice(scope.as_byte_slice());
    spec(table).serialize(&mut &mut key[67..]).map_err(|_| bad("material owned directory codec"))?;
    let _lease = reserve_native_requests_v1(key.len() as u64).map_err(ContextErrorV2::Resource)?;
    b.write_kernel_row(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec(), None).map_err(native_err)
}
fn slot(s: &PalwChainStateV2, key: Hash64) -> Result<Option<InstanceV7>, ContextErrorV2> {
    let Some(p) = lookup::<SlotV7>(s, SLOT, None, key)? else {
        return Ok(None);
    };
    let i = lookup::<InstanceV7>(s, INSTANCE, None, p.instance)?.ok_or(bad("material slot lost instance"))?;
    if i.id != p.instance || i.key != key {
        return Err(bad("material slot instance identity"));
    }
    Ok(Some(*i))
}
fn lineage(s: &PalwChainStateV2, id: Hash64) -> Result<LineageV7, ContextErrorV2> {
    let r = lookup::<LineageV7>(s, LINEAGE, None, id)?.ok_or(bad("material lineage absent"))?;
    if r.id != id || root(s, MEMBER, Some(id))?.rows != r.active {
        return Err(bad("material lineage active set differs"));
    }
    Ok(*r)
}
fn fresh(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    ordinal: u64,
    key: Hash64,
    row: Income,
    inherit: Option<Hash64>,
) -> Result<InstanceV7, ContextErrorV2> {
    if slot(&b.state, key)?.is_some() {
        return Err(bad("material generation has occupied native slot"));
    }
    let initial = hash(2, &row)?;
    let id = hash(1, &(point, ordinal, key, initial))?;
    if lookup::<InstanceV7>(&b.state, INSTANCE, None, id)?.is_some() {
        return Err(bad("material generation reused"));
    }
    let lin = inherit.unwrap_or(id);
    if inherit.is_none() {
        put(b, LINEAGE, None, lin, Some(&LineageV7 { id: lin, active: 0, references: 0 }))?;
    }
    let mut l = lineage(&b.state, lin)?;
    let i = InstanceV7 { id, key, lineage: lin, created_at: point, ordinal, initial, current: row };
    put(b, INSTANCE, None, id, Some(&i))?;
    put(b, SLOT, None, key, Some(&SlotV7 { instance: id }))?;
    put(b, MEMBER, Some(lin), id, Some(&key))?;
    put(b, ANCESTRY, Some(id), lin, Some(&lin))?;
    l.active = l.active.checked_add(1).ok_or(bad("material active count overflow"))?;
    put(b, LINEAGE, None, lin, Some(&l))?;
    Ok(i)
}
fn ancestry(s: &PalwChainStateV2, id: Hash64) -> Result<NativeRetainedV1<Vec<Hash64>>, ContextErrorV2> {
    let count = root(s, ANCESTRY, Some(id))?.rows;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Hash64>(count).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut ids = Vec::with_capacity(usize::try_from(count).map_err(|_| bad("material ancestry capacity"))?);
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, root(s, ANCESTRY, Some(id))?, |k, v| {
        let raw = reader.body(v)?;
        let id = decode::<Hash64>(&raw)?;
        if k != id.as_byte_slice() {
            return Err(bad("material ancestry key differs"));
        }
        ids.push(*id);
        Ok(())
    })?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(ids, lease))
}
fn link(b: &mut TransitionBuilder<'_>, i: &InstanceV7, lin: Hash64) -> Result<(), ContextErrorV2> {
    if lookup::<Hash64>(&b.state, ANCESTRY, Some(i.id), lin)?.is_some() {
        return Ok(());
    }
    let mut l = lineage(&b.state, lin)?;
    put(b, MEMBER, Some(lin), i.id, Some(&i.key))?;
    put(b, ANCESTRY, Some(i.id), lin, Some(&lin))?;
    l.active = l.active.checked_add(1).ok_or(bad("material merged active count overflow"))?;
    put(b, LINEAGE, None, lin, Some(&l))?;
    Ok(())
}
fn cleanup_lineage(b: &mut TransitionBuilder<'_>, id: Hash64) -> Result<(), ContextErrorV2> {
    if let Some(l) = lookup::<LineageV7>(&b.state, LINEAGE, None, id)?
        && l.active == 0
        && l.references == 0
    {
        drop_empty_owned(b, MEMBER, id)?;
        put::<LineageV7>(b, LINEAGE, None, id, None)?;
    }
    Ok(())
}
fn touch(b: &mut TransitionBuilder<'_>, ids: &mut Vec<Hash64>, id: Hash64) -> Result<(), ContextErrorV2> {
    if ids.len() == ids.capacity() {
        b.native_reservation.grow_v1(std::mem::size_of::<Hash64>() as u64).map_err(ContextErrorV2::Resource)?;
        ids.try_reserve_exact(1).map_err(|_| bad("material touched lineage allocation"))?;
    }
    ids.push(id);
    Ok(())
}
fn write_existing(b: &mut TransitionBuilder<'_>, i: InstanceV7, next: Option<Income>) -> Result<(), ContextErrorV2> {
    match next {
        Some(current) => {
            put(b, INSTANCE, None, i.id, Some(&InstanceV7 { current, ..i }))?;
        }
        None => {
            let ids = ancestry(&b.state, i.id)?;
            for lin in ids.iter() {
                let mut l = lineage(&b.state, *lin)?;
                put::<Hash64>(b, MEMBER, Some(*lin), i.id, None)?;
                put::<Hash64>(b, ANCESTRY, Some(i.id), *lin, None)?;
                l.active = l.active.checked_sub(1).ok_or(bad("material active count underflow"))?;
                put(b, LINEAGE, None, *lin, Some(&l))?;
            }
            drop_empty_owned(b, ANCESTRY, i.id)?;
            put::<InstanceV7>(b, INSTANCE, None, i.id, None)?;
            put::<SlotV7>(b, SLOT, None, i.key, None)?;
        }
    }
    Ok(())
}
fn key(raw: &[u8]) -> Result<Hash64, ContextErrorV2> {
    borsh::from_slice(raw).map_err(|_| bad("material journal key"))
}
fn row(raw: Option<&Vec<u8>>) -> Result<Option<Income>, ContextErrorV2> {
    raw.map(|r| decode::<Income>(r).map(|r| *r)).transpose()
}
fn material(write: &PalwBudgetWriteV1) -> Option<(&[u8], Option<&Vec<u8>>, Option<&Vec<u8>>)> {
    match write {
        PalwBudgetWriteV1::Row { table: crate::palw_bond_budget_v1::PALW_BUDGET_TABLE_MATERIAL_INCOME_V1, key, old, new } => {
            Some((key, old.as_ref(), new.as_ref()))
        }
        _ => None,
    }
}
#[derive(Clone, Copy)]
struct SeenPendingV7 {
    key: Hash64,
    row: Income,
    lineage: Hash64,
}
/// Observe ONLY an actual engine operation's completed row journal. This never
/// uses current raw source IDs to assign an old detached source's duty.
/// Call inside the original operation rollback owner before publishing its delta.
pub(super) fn observe_operation_v7(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    writes: &[PalwBudgetWriteV1],
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(());
    }
    if lookup::<u16>(&b.state, CONTROL, None, Hash64::default()).map_err(err)?.is_none() {
        return Ok(());
    }
    atomic(b, |b| {
        let count = writes.iter().filter(|w| material(w).is_some()).count();
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<SeenPendingV7>(count as u64).map_err(ContextErrorV2::Resource)?;
        q.exact_v1::<Hash64>((count as u64).checked_mul(2).ok_or(bad("material operation count"))?)
            .map_err(ContextErrorV2::Resource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut seen = Vec::<SeenPendingV7>::with_capacity(count);
        let mut touched = Vec::<Hash64>::with_capacity(count.saturating_mul(2));
        let base = u64::try_from(b.entries.len()).map_err(|_| bad("material journal ordinal"))?;
        for (n, w) in writes.iter().enumerate() {
            let Some((raw, old, new)) = material(w) else {
                continue;
            };
            let k = key(raw)?;
            let old = row(old)?;
            let new = row(new)?;
            if old == new {
                continue;
            }
            let ordinal = base.checked_add(n as u64).ok_or(bad("material ordinal overflow"))?;
            let existing = slot(&b.state, k)?;
            if let (Some(old), Some(i)) = (old, existing) {
                if i.current != old {
                    return Err(bad("material update old bytes differ"));
                }
                if matches!(old.payment, Payment::Pending { .. }) {
                    seen.push(SeenPendingV7 { key: k, row: old, lineage: i.lineage });
                }
            } else if old.is_none() && existing.is_some() {
                return Err(bad("material create occupied slot"));
            }
            // Child identity is the EXACT current native codec's domain/key,
            // authenticated by the same operation's old Pending and new queue.
            let mut inherited = None;
            if let Some(next) = new
                && let Payment::Queued { income: Some(queue), queued_daa, .. } = next.payment
            {
                for prior in writes.iter() {
                    let Some((pk, po, _)) = material(prior) else {
                        continue;
                    };
                    let Some(parent) = row(po)? else { continue };
                    let Payment::Pending { income } = parent.payment else {
                        continue;
                    };
                    let parent_key = key(pk)?;
                    if parent_key != crate::palw_bond_budget_v1::material_pending_id_v1(income) {
                        return Err(bad("material pending identity differs"));
                    }
                    if k != crate::palw_bond_budget_v1::material_queue_receipt_id_v1(income, queue, queued_daa) {
                        continue;
                    }
                    if parent.root != next.root || parent.accepted_daa != next.accepted_daa {
                        return Err(bad("material transfer source differs"));
                    }
                    let added =
                        next.amount.checked_sub(old.map_or(0, |r| r.amount)).ok_or(bad("material transfer amount decreased"))?;
                    if added > parent.amount {
                        return Err(bad("material transfer exceeds actual parent"));
                    }
                    let lin = if let Some(p) = seen.iter().rev().find(|p| p.key == parent_key && p.row == parent) {
                        p.lineage
                    } else if let Some(p) = slot(&b.state, parent_key)? {
                        if p.current != parent {
                            return Err(bad("material transfer parent body differs"));
                        }
                        p.lineage
                    } else {
                        continue;
                    }; // unrelated native operation has no source owner

                    if inherited.is_some_and(|other| other != lin) {
                        return Err(bad("material transfer has conflicting origins"));
                    }
                    inherited = Some(lin);
                }
            }
            match existing {
                Some(i) => {
                    // Native receipt keys can legitimately coalesce equal root/DAA
                    // tranches. Preserve ALL proven parent lineages, not a new refusal.
                    if let Some(lin) = inherited {
                        link(b, &i, lin)?;
                    }
                    let ids = ancestry(&b.state, i.id)?;
                    for lin in ids.iter() {
                        touch(b, &mut touched, *lin)?;
                    }
                    write_existing(b, i, new)?;
                }
                None => {
                    if let (Some(next), Some(inherited)) = (new, inherited) {
                        let i = fresh(b, point, ordinal, k, next, Some(inherited))?;
                        touch(b, &mut touched, i.lineage)?;
                    }
                }
            }
        }
        for id in touched {
            if let Some(line) = lookup::<LineageV7>(&b.state, LINEAGE, None, id)?
                && line.active == 0
            {
                let proof = ActualLineageClosedV9 { lineage: id };
                super::palw_claim_local_kernel_contract_duties_v9::note_actual_lineage_closed_v9(b, &proof).map_err(native_err)?;
            }
            cleanup_lineage(b, id)?;
        }
        // Metadata consumed entry slots owned by the original native quote.
        // Admit ALL remaining actual native journal pushes while AUX rollback
        // is still armed, including an operation with no material-table rows.
        b.reserve_mutation_sequence_v1(0, writes.len()).map_err(native_err)?;
        Ok(())
    })
    .map_err(err)
}
/// Capture exact native income population before retirement clears consumers and
/// EXEC paths. E authenticates this source birth; the income population does not.
/// This version intentionally does not incorporate provider-covenant release.
pub(super) fn capture_source_v7(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    birth: PalwBlockContextV2,
    source: Hash64,
) -> Result<Hash64, PalwStateV2Error> {
    atomic(b, |b| {
        let c = b.state.claims.get(&source).ok_or(bad("source income capture missing native claim"))?;
        if (c.accepted_block, c.accepted_daa, c.accepted_blue_score) != (birth.block, birth.daa_score, birth.blue_score) {
            return Err(bad("source income capture wrong birth"));
        }
        let e = b
            .state
            .prefix_export_reservation_v1(&birth, source)
            .map_err(native_err)?
            .ok_or(bad("source income capture missing actual E"))?;
        let budget = e.budget_id;
        if let Some(p) = lookup::<PopulationV7>(&b.state, POPULATION, None, budget)?
            && (p.birth != birth || p.source != source)
        {
            return Err(bad("source income population collision"));
        }
        let legacy = b.params.material_income_from().is_some_and(|from| birth.daa_score < from);
        let scope = super::palw_exec_v2_material_scope_v7::source_roots_v7(&b.state, source).map_err(native_err)?;
        let count = if let Some(book) = &b.state.bond_budget {
            let mut count = if legacy { book.material_root_count_v7(None) } else { 0 };
            for root in scope.iter() {
                count = count.checked_add(book.material_root_count_v7(Some(*root))).ok_or(bad("scoped income count overflow"))?;
            }
            count
        } else {
            0
        };
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<(Hash64, Income)>(count as u64).map_err(ContextErrorV2::Resource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut rows = Vec::with_capacity(count);
        if let Some(book) = &b.state.bond_budget {
            if legacy {
                for row in book.material_root_rows_v7(None) {
                    rows.push(row.map_err(bad)?);
                }
            }
            for root in scope.iter() {
                for row in book.material_root_rows_v7(Some(*root)) {
                    rows.push(row.map_err(bad)?);
                }
            }
        }
        if rows.len() != count {
            return Err(bad("scoped native root index count differs"));
        }
        // Marker is written only for a genuine source-owned observation.
        if lookup::<u16>(&b.state, CONTROL, None, Hash64::default())?.is_none() {
            put(b, CONTROL, None, Hash64::default(), Some(&8u16))?;
        }
        let base = b.entries.len() as u64;
        for (n, (k, row)) in rows.into_iter().enumerate() {
            let i = match slot(&b.state, k)? {
                Some(i) => i,
                None => fresh(b, point, base.checked_add(n as u64).ok_or(bad("capture ordinal overflow"))?, k, row, None)?,
            };
            if i.current != row {
                return Err(bad("capture physical income differs"));
            }
            let ids = ancestry(&b.state, i.id)?;
            for lin in ids.iter() {
                if lookup::<Hash64>(&b.state, POPULATION, Some(budget), *lin)?.is_none() {
                    let mut l = lineage(&b.state, *lin)?;
                    l.references = l.references.checked_add(1).ok_or(bad("income population references overflow"))?;
                    put(b, POPULATION, Some(budget), *lin, Some(lin))?;
                    put(b, LINEAGE, None, *lin, Some(&l))?;
                }
            }
        }
        let p = PopulationV7 { version: 7, budget, source, birth, members: root(&b.state, POPULATION, Some(budget))? };
        put_population(b, &p)?;
        Ok(budget)
    })
    .map_err(err)
}
/// Reports ONLY the captured population, not closure of all future duty authority.
/// Caller must separately prove old contractual creation authority closed.
/// Captured population empties only after actual native removal of all members.
/// No current raw-ID lookup, payment-clock guess or H truncation is performed.
pub(super) fn captured_income_pending_v7(
    s: &PalwChainStateV2,
    budget: Hash64,
    source: Hash64,
    birth: PalwBlockContextV2,
) -> Result<bool, ContextErrorV2> {
    let p = lookup::<PopulationV7>(s, POPULATION, None, budget)?
        .ok_or(ContextErrorV2::Unavailable("source income population not captured"))?;
    if p.version != 7 || p.budget != budget || p.source != source || p.birth != birth {
        return Err(bad("source income query changed birth"));
    }
    let reader = StateNodesV2(s);
    let mut pending = false;
    nodes::complete_rows(&reader, root(s, POPULATION, Some(budget))?, |key, v| {
        let raw = reader.body(v)?;
        let id = decode::<Hash64>(&raw)?;
        if key != id.as_byte_slice() {
            return Err(bad("income population lineage key"));
        }
        pending |= lineage(s, *id)?.active != 0;
        Ok(())
    })?;
    Ok(pending)
}
/// Caller must already prove E/source/frame closure using their real owners.
/// Removes empty owned current pointers. Immutable rows use normal context GC.
pub(super) fn drop_closed_population_v7(b: &mut TransitionBuilder<'_>, budget: Hash64) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let Some(p) = lookup::<PopulationV7>(&b.state, POPULATION, None, budget)? else {
            return Ok(());
        };
        if captured_income_pending_v7(&b.state, budget, p.source, p.birth)? {
            return Err(bad("cannot drop pending source income population"));
        }
        let count = root(&b.state, POPULATION, Some(budget))?.rows;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Hash64>(count).map_err(ContextErrorV2::Resource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut ids = Vec::with_capacity(usize::try_from(count).map_err(|_| bad("income population capacity"))?);
        let r = StateNodesV2(&b.state);
        nodes::complete_rows(&r, root(&b.state, POPULATION, Some(budget))?, |k, _| {
            ids.push(key(k)?);
            Ok(())
        })?;
        for id in ids {
            let mut l = lineage(&b.state, id)?;
            l.references = l.references.checked_sub(1).ok_or(bad("income population references underflow"))?;
            put::<Hash64>(b, POPULATION, Some(budget), id, None)?;
            if l.references == 0 && l.active == 0 {
                drop_empty_owned(b, MEMBER, id)?;
                put::<LineageV7>(b, LINEAGE, None, id, None)?;
            } else {
                put(b, LINEAGE, None, id, Some(&l))?;
            }
        }
        let a = inventory(&p)?;
        let id = inventory_id(&a)?;
        drop_empty_owned(b, POPULATION, budget)?;
        put::<PopulationV7>(b, POPULATION, None, budget, None)?;
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(id), None).map_err(native_err)?;
        Ok(())
    })
    .map_err(err)
}
/// Error path for the isolated hook: reverse the real engine journal instead of
/// cloning the whole native book. The caller's existing operation admission must
/// cover inverse typed row decode/reindex before installing this hook.
pub(super) fn restore_engine_v7(
    book: &mut crate::palw_bond_budget_v1::PalwBondBudgetStateV1,
    writes: &[PalwBudgetWriteV1],
) -> Result<(), PalwStateV2Error> {
    for write in writes.iter().rev() {
        match write {
            PalwBudgetWriteV1::Row { table, key, old, new } => {
                book.apply_row(*table, key, old, new, true).map_err(|e| PalwStateV2Error::BondBudgetRefused(e.into()))?
            }
            PalwBudgetWriteV1::Header { old, new } => {
                if new.as_ref() != Some(&book.header) {
                    return Err(PalwStateV2Error::BondBudgetRefused("material inverse header mismatch".into()));
                }
                book.header = old
                    .clone()
                    .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("material inverse removed existing engine".into()))?;
            }
        }
    }
    Ok(())
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn inventory(p: &PopulationV7) -> Result<NativeRetainedV1<store::ArchiveReferencesV2>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentRootV2>(1).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<RowValueV2>(1).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let raw = encode(p)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        store::ArchiveReferencesV2 { version: 2, components: vec![p.members], literals: vec![value(&raw)] },
        lease,
    ))
}
fn inventory_id(a: &store::ArchiveReferencesV2) -> Result<Hash64, ContextErrorV2> {
    Ok(value(&encode(a)?).data)
}
fn put_population(b: &mut TransitionBuilder<'_>, p: &PopulationV7) -> Result<(), ContextErrorV2> {
    let prior = lookup::<PopulationV7>(&b.state, POPULATION, None, p.budget)?;
    let a = inventory(p)?;
    let id = inventory_id(&a)?;
    // Publish the population body before the archive adds its literal pin.
    put(b, POPULATION, None, p.budget, Some(p))?;
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &akey(id), Some(&*a)).map_err(native_err)?;
    if let Some(prior) = prior {
        let old_id = {
            let retained_inventory = inventory(&prior)?;
            inventory_id(&retained_inventory)?
        };
        if old_id != id {
            b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(old_id), None)
                .map_err(native_err)?;
        }
    }
    Ok(())
}
/// Small targeted-observation format marker; no global native rows are copied.
/// Marker7 remains the legacy COMPLETE tracker format; new marker8 is explicitly
/// scoped and validates only its actual observed physical population.
pub(super) fn bootstrap_current_v7(b: &mut TransitionBuilder<'_>, point: PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(());
    }
    if let Some(marker) = lookup::<u16>(&b.state, CONTROL, None, Hash64::default()).map_err(err)? {
        if !matches!(*marker, 7 | 8) {
            return Err(err(bad("material tracker format marker")));
        }
        return Ok(());
    }
    atomic(b, |b| put(b, CONTROL, None, Hash64::default(), Some(&8u16)).map(|_| ())).map_err(err)
}
/// Typed complete cold-restore closure. Generic N/B hashing/root validation is
/// still required by context_store; this adds owner-specific native and source
/// authority/inventory equality, bidirectional memberships and reference counts.
pub(super) fn validate_v7(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    use crate::palw_native_resource_v1::BTreeNodeEnvelopeV1 as Node;
    let bootstrap = lookup::<u16>(s, CONTROL, None, Hash64::default())?;
    if bootstrap.as_ref().is_some_and(|v| !matches!(**v, 7 | 8)) {
        return Err(bad("material lineage bootstrap codec"));
    }
    let mut has_owner = false;
    if let Some(route) = &s.kernel_route {
        for ((table, k), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 {
                continue;
            }
            if k.first() == Some(&b'I') {
                let r = decode::<ComponentRootV2>(raw)?;
                let keyed = match k.len() {
                    18 => borsh::from_slice::<ComponentSpecV2>(&k[1..]).ok(),
                    84 if k[1..3] == [2, 0] => borsh::from_slice::<ComponentSpecV2>(&k[67..]).ok(),
                    _ => None,
                };
                if r.spec.owner == 8 || keyed.is_some_and(|spec| spec.owner == 8) {
                    has_owner = true;
                }
            } else if k.first() == Some(&b'A') && raw.get(..2) == Some(&[2, 0]) {
                let a = decode::<store::ArchiveReferencesV2>(raw)?;
                if a.components.iter().any(|c| c.spec.owner == 8) {
                    has_owner = true;
                }
            }
        }
    }
    if bootstrap.is_none() {
        return if has_owner { Err(bad("material lineage owner rows lack bootstrap")) } else { Ok(()) };
    }
    if root(s, CONTROL, None)?.rows != 1 {
        return Err(bad("material lineage extra control row"));
    }
    let physical = root(s, INSTANCE, None)?.rows;
    let lines = root(s, LINEAGE, None)?.rows;
    let pops = root(s, POPULATION, None)?.rows;
    let native = s.bond_budget.as_ref().map_or(0, |b| b.material_income.len()) as u64;
    let complete = bootstrap.as_ref().is_some_and(|v| **v == 7);
    if root(s, SLOT, None)?.rows != physical || physical > native || (complete && physical != native) {
        return Err(bad("material observed/native population count differs"));
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Node<Hash64, u64>>(physical).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<Node<Hash64, (u64, u64)>>(lines).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<Node<Hash64, ()>>(pops).map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut slots = BTreeMap::<Hash64, u64>::new();
    let mut counts = BTreeMap::<Hash64, (u64, u64)>::new();
    let mut archives = BTreeSet::new();
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, root(s, LINEAGE, None)?, |k, v| {
        let raw = reader.body(v)?;
        let l = decode::<LineageV7>(&raw)?;
        if k != l.id.as_byte_slice() || l.active == 0 && l.references == 0 {
            return Err(bad("orphan or wrong lineage metadata"));
        }
        if counts.insert(l.id, (0, 0)).is_some() {
            return Err(bad("duplicate lineage leaf"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, SLOT, None)?, |k, v| {
        let raw = reader.body(v)?;
        let p = decode::<SlotV7>(&raw)?;
        let native_key = key(k)?;
        let i = slot(s, native_key)?.ok_or(bad("material native slot absent"))?;
        if i.id != p.instance || s.bond_budget.as_ref().and_then(|b| b.material_income.get(&native_key)) != Some(&i.current) {
            return Err(bad("material slot/native row differs"));
        }
        if slots.insert(i.id, 0).is_some() {
            return Err(bad("material instance assigned to multiple slots"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, INSTANCE, None)?, |k, v| {
        let raw = reader.body(v)?;
        let i = decode::<InstanceV7>(&raw)?;
        if k != i.id.as_byte_slice()
            || i.id != hash(1, &(i.created_at, i.ordinal, i.key, i.initial))?
            || slot(s, i.key)?.as_ref() != Some(&*i)
        {
            return Err(bad("material instance/slot bijection differs"));
        }
        *slots.get_mut(&i.id).ok_or(bad("orphan material instance"))? += 1;
        let ids = ancestry(s, i.id)?;
        if ids.is_empty() || !ids.contains(&i.lineage) {
            return Err(bad("material primary ancestry missing"));
        }
        for lin in ids.iter() {
            let member = lookup::<Hash64>(s, MEMBER, Some(*lin), i.id)?.ok_or(bad("material ancestry lacks reverse member"))?;
            if *member != i.key {
                return Err(bad("material reverse member native key differs"));
            }
        }
        Ok(())
    })?;
    for n in slots.values() {
        if *n != 1 {
            return Err(bad("material instance stream incomplete"));
        }
    }
    nodes::complete_rows(&reader, root(s, LINEAGE, None)?, |_, v| {
        let raw = reader.body(v)?;
        let l = decode::<LineageV7>(&raw)?;
        nodes::complete_rows(&reader, root(s, MEMBER, Some(l.id))?, |k, v| {
            let id = key(k)?;
            let raw = reader.body(v)?;
            let native_key = decode::<Hash64>(&raw)?;
            let i = lookup::<InstanceV7>(s, INSTANCE, None, id)?.ok_or(bad("material lineage names missing generation"))?;
            if i.id != id || i.key != *native_key || lookup::<Hash64>(s, ANCESTRY, Some(id), l.id)?.as_deref() != Some(&l.id) {
                return Err(bad("material member/ancestry reverse closure"));
            }
            let n = &mut counts.get_mut(&l.id).ok_or(bad("material lineage counter absent"))?.0;
            *n = n.checked_add(1).ok_or(bad("material active count overflow"))?;
            Ok(())
        })?;
        if root(s, MEMBER, Some(l.id))?.rows != l.active {
            return Err(bad("material lineage count/root differs"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, POPULATION, None)?, |k, v| {
        let raw = reader.body(v)?;
        let p = decode::<PopulationV7>(&raw)?;
        if p.version != 7 || k != p.budget.as_byte_slice() || p.members != root(s, POPULATION, Some(p.budget))? {
            return Err(bad("material source population/root differs"));
        }
        let e =
            s.prefix_export_reservation_v1(&p.birth, p.source).map_err(native_err)?.ok_or(bad("material population lacks exact E"))?;
        if e.budget_id != p.budget || e.point != p.birth || e.source_claim != p.source {
            return Err(bad("material population source birth differs"));
        }
        let a = inventory(&p)?;
        let id = inventory_id(&a)?;
        archives.insert(id);
        let route = s.kernel_route.as_ref().ok_or(bad("material population route absent"))?;
        let _key = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
        let stored =
            route.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, akey(id).to_vec())).ok_or(bad("material source A2 inventory absent"))?;
        crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&*a, stored)
            .map_err(|_| bad("material source A2 inventory differs"))?;
        nodes::complete_rows(&reader, p.members, |k, v| {
            let raw = reader.body(v)?;
            let lin = decode::<Hash64>(&raw)?;
            if k != lin.as_byte_slice() {
                return Err(bad("material source lineage key differs"));
            }
            let n = &mut counts.get_mut(&*lin).ok_or(bad("material source names absent lineage"))?.1;
            *n = n.checked_add(1).ok_or(bad("material source reference overflow"))?;
            Ok(())
        })?;
        Ok(())
    })?;
    if let Some(route) = &s.kernel_route {
        for ((table, k), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 {
                continue;
            }
            if k.first() == Some(&b'I') {
                let r = decode::<ComponentRootV2>(raw)?;
                let key_spec = match k.len() {
                    18 => Some(borsh::from_slice::<ComponentSpecV2>(&k[1..]).map_err(|_| bad("material current key codec"))?),
                    84 if k[1..3] == [2, 0] => {
                        Some(borsh::from_slice::<ComponentSpecV2>(&k[67..]).map_err(|_| bad("material owned current key codec"))?)
                    }
                    _ => None,
                };
                if r.spec.owner != 8 && key_spec.is_none_or(|spec| spec.owner != 8) {
                    continue;
                }
                if key_spec != Some(r.spec) {
                    return Err(bad("material key owner/profile differs from rooted owner"));
                }
                if !matches!(r.spec.table, SLOT | INSTANCE | MEMBER | POPULATION | LINEAGE | ANCESTRY | CONTROL)
                    || r.spec != spec(r.spec.table)
                {
                    return Err(bad("unallocated material owner8 specification"));
                }
                match k.len() {
                    18 => {
                        if !matches!(r.spec.table, SLOT | INSTANCE | POPULATION | LINEAGE | CONTROL) {
                            return Err(bad("material owned table at global key"));
                        }
                        let decoded = borsh::from_slice::<ComponentSpecV2>(&k[1..]).map_err(|_| bad("material global key spec"))?;
                        if decoded != r.spec {
                            return Err(bad("material global key/root spec differs"));
                        }
                    }
                    84 if k[1..3] == [2, 0] => {
                        let scope = key(&k[3..67])?;
                        let decoded = borsh::from_slice::<ComponentSpecV2>(&k[67..]).map_err(|_| bad("material owned key spec"))?;
                        if decoded != r.spec {
                            return Err(bad("material owned key/root spec differs"));
                        }
                        match r.spec.table {
                            MEMBER => {
                                if lookup::<LineageV7>(s, LINEAGE, None, scope)?.is_none() {
                                    return Err(bad("orphan material lineage member root"));
                                }
                            }
                            ANCESTRY => {
                                if lookup::<InstanceV7>(s, INSTANCE, None, scope)?.is_none() {
                                    return Err(bad("orphan material instance ancestry root"));
                                }
                            }
                            POPULATION => {
                                let p =
                                    lookup::<PopulationV7>(s, POPULATION, None, scope)?.ok_or(bad("orphan material source root"))?;
                                if p.members != *r {
                                    return Err(bad("material source owned root differs"));
                                }
                            }
                            _ => return Err(bad("material global table at owned key")),
                        }
                    }
                    _ => return Err(bad("material current directory key width/version")),
                }
            } else if k.first() == Some(&b'A') && raw.get(..2) == Some(&[2, 0]) {
                let a = decode::<store::ArchiveReferencesV2>(raw)?;
                if a.components.iter().any(|c| c.spec.owner == 8) && (k.len() != 65 || !archives.contains(&key(&k[1..])?)) {
                    return Err(bad("orphan material source A2 inventory"));
                }
            }
        }
    }
    for (id, (active, refs)) in counts {
        let l = lineage(s, id)?;
        if l.active != active || l.references != refs {
            return Err(bad("material lineage restored counts differ"));
        }
    }
    Ok(())
}

pub(super) fn captured_income_pending_if_present_v7(
    s: &PalwChainStateV2,
    budget: Hash64,
    source: Hash64,
    birth: PalwBlockContextV2,
) -> Result<Option<bool>, ContextErrorV2> {
    if lookup::<PopulationV7>(s, POPULATION, None, budget)?.is_none() {
        return Ok(None);
    }
    captured_income_pending_v7(s, budget, source, birth).map(Some)
}

fn observe_owned_current_v7(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    key: Hash64,
) -> Result<InstanceV7, ContextErrorV2> {
    let actual =
        *b.state.bond_budget.as_ref().and_then(|book| book.material_income.get(&key)).ok_or(bad("owned native income row absent"))?;
    if let Some(i) = slot(&b.state, key)? {
        if i.current != actual {
            return Err(bad("owned native income differs from observed generation"));
        }
        return Ok(i);
    }
    if lookup::<u16>(&b.state, CONTROL, None, Hash64::default())?.is_none() {
        put(b, CONTROL, None, Hash64::default(), Some(&8u16))?;
    }
    fresh(b, point, b.entries.len() as u64, key, actual, None)
}

#[cfg(test)]
mod targeted_lineage_tests {
    use super::*;
    use crate::palw_bond_budget_v1::*;
    use crate::tx::TransactionOutpoint;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn point(n: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h(1000 + n), daa_score: n, blue_score: n, subsidy: 0 }
    }
    fn policy() -> PalwBondBudgetPolicyV1 {
        PalwBondBudgetPolicyV1 {
            version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
            window_daa: 50,
            capital_unit_sompi: 1000,
            rho: 1,
            claims_per_unit: 2,
            block_units_per_unit: 2 * PALW_BUDGET_BLOCK_UNIT_V1,
            reward_per_unit_sompi: 2000,
            final_weight_per_unit: 1000,
            max_open_claims_per_bond: 1000,
            slice_rights_by_rho: false,
            round_rights: PalwRoundRightsPolicyV1::CountAgainstBlocks,
            liability_hold_daa: PALW_BOND_BUDGET_LIABILITY_HOLD_INTERIM_DAA_V1,
            export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
            weight_value: PalwWeightValuePolicyV1::Unknown,
            hold: None,
        }
    }
    fn fixture() -> (PalwStateParamsV2, PalwTransitionExtrasV1, PalwChainStateV2, PalwBondKeyV2) {
        let p = PalwStateParamsV2::new(100, 100, 10, 20, 50, 1000, h(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_claim_local_settlement_from_daa(Some(0))
            .with_material_income_from(Some(0))
            .with_bond_budget(Some(PalwBondBudgetMirrorV1 { from_daa: 0, policy: policy(), allocation: None }));
        let e = PalwTransitionExtrasV1 {
            claim_local_rule_context_v1: Some(([11; 32], [12; 32])),
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(10),
                ruleset_digest: h(11),
                chain_genesis: h(12),
                ..Default::default()
            }),
            ..Default::default()
        };
        let bond = PalwBondKeyV2(TransactionOutpoint::new(h(30), 0));
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([31; 32]);
        let registration = PalwConsensusObjectV2::BondRegistered {
            bond,
            pubkey: kp.verification_key.as_ref().to_vec(),
            operator_pubkey: kp.verification_key.as_ref().to_vec(),
            collateral: 1000,
            payout_payload: h(32),
            capable_classes: BTreeSet::new(),
            signature: Vec::new(),
        };
        let state = apply_palw_transition_v2_with_extras(
            &PalwChainStateV2::genesis(),
            &p,
            &point(1),
            &[registration],
            None,
            false,
            false,
            false,
            false,
            &e,
        )
        .unwrap()
        .0;
        (p, e, state, bond)
    }
    // This uses the REAL common native engine wrapper and its rollback path. The
    // fixture is a core accounting seam, not a signed public source-admission test.
    fn op(
        b: &mut TransitionBuilder<'_>,
        at: u64,
        f: impl FnOnce(&mut PalwBondBudgetStateV1, &mut Vec<PalwBudgetWriteV1>) -> Result<(), PalwBudgetRefusalV1>,
    ) {
        b.claim_local_lock_point_v6 = Some(point(at));
        b.bond_budget_op(super::super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::point(), |book, _, j| f(book, j))
            .unwrap()
            .unwrap()
            .unwrap();
        let old = b.state.last_point;
        b.state.last_point = Some(point(at));
        let projected =
            super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&b.state, b.params, &point(at))
                .unwrap()
                .unwrap();
        super::super::palw_claim_local_descriptor_v2::write(b, &projected).unwrap();
        b.reserve_mutation_sequence_v1(0, 1).unwrap();
        b.entries.push(PalwDeltaEntryV2::LastPoint { old, new: Some(point(at)) });
    }
    fn hold(b: &mut TransitionBuilder<'_>, bond: PalwBondKeyV2, id: Hash64, amount: u64) {
        let at = b.state.last_point.map_or(1, |p| p.daa_score).max(1);
        op(b, at, |book, j| {
            book.hold_native_income_v1(&policy(), 1000, id, bond, 1, 1, amount, &[], true, j)?;
            book.bind_material_income_v1(h(400), id, j)
        });
    }
    fn own(b: &mut TransitionBuilder<'_>, id: Hash64) -> InstanceV7 {
        let actual = b.claim_local_lock_point_v6.unwrap();
        atomic(b, |b| observe_owned_current_v7(b, actual, material_pending_id_v1(id))).unwrap()
    }
    #[test]
    fn rfc23s2_material_lineage_actual_transfer_coalescence_payment_and_due_delete() {
        let (p, e, state, bond) = fixture();
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        let income = h(401);
        let queue = h(402);
        hold(&mut b, bond, income, 100);
        b.claim_local_lock_point_v6 = Some(point(2));
        let first = own(&mut b, income);
        op(&mut b, 2, |book, j| book.pay_native_income_v1(&policy(), &income, 2, 100, queue, true, j));
        assert!(slot(&b.state, material_pending_id_v1(income)).unwrap().is_none());
        let queued_key = material_queue_receipt_id_v1(income, queue, 2);
        let queued = slot(&b.state, queued_key).unwrap().unwrap();
        assert_eq!(queued.current.amount, 100);
        // A real new native maximum after full transfer reuses the original income
        // key; identical original DAA/root is allowed by the native owner. Its new
        // physical Pending joins the existing receipt at the same actual queue clock.
        hold(&mut b, bond, income, 100);
        let second = own(&mut b, income);
        assert_ne!(first.id, second.id);
        assert_eq!(first.initial, second.initial);
        assert_eq!(first.created_at, second.created_at);
        op(&mut b, 2, |book, j| book.pay_native_income_v1(&policy(), &income, 2, 100, queue, true, j));
        let receipt = slot(&b.state, queued_key).unwrap().unwrap();
        assert_eq!(receipt.current.amount, 200);
        let ancestors = ancestry(&b.state, receipt.id).unwrap();
        assert_eq!(ancestors.len(), 2);
        assert!(ancestors.contains(&first.lineage) && ancestors.contains(&second.lineage));
        op(&mut b, 3, |book, j| book.confirm_native_income_payment_v1(&policy(), &queue, 3, j));
        assert_eq!(slot(&b.state, queued_key).unwrap().unwrap().current.release(), Some(53));
        op(&mut b, 52, |book, j| {
            book.release_material_income_v1(52, j);
            Ok(())
        });
        assert!(slot(&b.state, queued_key).unwrap().is_some());
        op(&mut b, 53, |book, j| {
            book.release_material_income_v1(53, j);
            Ok(())
        });
        assert!(slot(&b.state, queued_key).unwrap().is_none());
        assert_eq!(root(&b.state, LINEAGE, None).unwrap().rows, 0);
        validate_v7(&b.state).unwrap();
    }
    #[test]
    fn rfc23s2_material_lineage_targeted_current_book_and_cold_restore() {
        let (p, e, state, bond) = fixture();
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        hold(&mut b, bond, h(410), 100);
        hold(&mut b, bond, h(411), 80);
        assert_eq!(root(&b.state, INSTANCE, None).unwrap().rows, 0, "unrelated native rows acquire no archive metadata");
        own(&mut b, h(410));
        assert_eq!(root(&b.state, INSTANCE, None).unwrap().rows, 1);
        validate_v7(&b.state).unwrap();
        let expected = b.state.state_root();
        let wire = borsh::to_vec(&PalwStateCarriageV2::from_state(&b.state)).unwrap();
        let decoded: PalwStateCarriageV2 = borsh::from_slice(&wire).unwrap();
        let cold = decoded.into_state_v3(&p, Some(expected), false, None).unwrap();
        assert_eq!(cold.state_root(), expected);
        validate_v7(&cold).unwrap();
        assert_eq!(root(&cold, INSTANCE, None).unwrap().rows, 1);
        assert_eq!(cold.bond_budget.as_ref().unwrap().material_income.len(), 2);
    }
    #[test]
    fn rfc23s2_material_lineage_real_engine_inverse_restores_after_observer_fault() {
        let (p, e, state, bond) = fixture();
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        for n in [420, 421] {
            hold(&mut b, bond, h(n), 50);
            own(&mut b, h(n));
        }
        for n in [420, 421] {
            op(&mut b, 2, |book, j| book.pay_native_income_v1(&policy(), &h(n), 2, 50, h(n + 10), true, j));
        }
        for n in [420, 421] {
            op(&mut b, 3, |book, j| book.confirm_native_income_payment_v1(&policy(), &h(n + 10), 3, j));
        }
        let key = material_queue_receipt_id_v1(h(421), h(431), 2);
        let old = slot(&b.state, key).unwrap().unwrap();
        let corrupt = InstanceV7 { current: Income { amount: old.current.amount + 1, ..old.current }, ..old };
        put(&mut b, INSTANCE, None, old.id, Some(&corrupt)).unwrap(); // intentional local-store fault
        let before = b.state.state_root();
        let native = borsh::to_vec(b.state.bond_budget.as_ref().unwrap()).unwrap();
        let entries = b.entries.len();
        b.claim_local_lock_point_v6 = Some(point(53));
        let result = b.bond_budget_op(super::super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::rows(2), |book, _, j| {
            book.release_material_income_v1(53, j)
        });
        assert!(result.is_err());
        assert_eq!(b.state.state_root(), before);
        assert_eq!(b.entries.len(), entries);
        assert_eq!(
            borsh::to_vec(b.state.bond_budget.as_ref().unwrap()).unwrap(),
            native,
            "actual engine inverse preserves both released receipts"
        );
    }
}

/// Consume an opaque ACTUAL provider-contract generation/source association.
/// This is the only post-detach extension; raw current claim/root is no authority.
pub(super) fn reference_contract_income_v8(
    b: &mut TransitionBuilder<'_>,
    token: &super::palw_claim_local_source_contract_generation_v8::ContractIncomeTokenV8,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let material_root = super::palw_claim_local_source_contract_generation_v8::verify_income_token_v8(&b.state, token)?;
        let p = lookup::<PopulationV7>(&b.state, POPULATION, None, token.budget())?
            .ok_or(ContextErrorV2::Unavailable("contract source income population not captured"))?;
        if p.version != 7
            || p.source != token.source()
            || p.birth != token.birth()
            || p.members != root(&b.state, POPULATION, Some(p.budget))?
        {
            return Err(bad("contract income source birth differs"));
        }
        let key = crate::palw_bond_budget_v1::material_pending_id_v1(token.income());
        let point = b.claim_local_lock_point_v6.ok_or(ContextErrorV2::Unavailable("contract income actual point absent"))?;
        let i = observe_owned_current_v7(b, point, key)?;
        if i.current.root != Some(material_root) || i.current.payment != (Payment::Pending { income: token.income() }) {
            return Err(bad("contract income actual material source differs"));
        }
        let ids = ancestry(&b.state, i.id)?;
        for lin in ids.iter() {
            if lookup::<Hash64>(&b.state, POPULATION, Some(p.budget), *lin)?.is_none() {
                let mut l = lineage(&b.state, *lin)?;
                l.references = l.references.checked_add(1).ok_or(bad("contract income reference overflow"))?;
                put(b, POPULATION, Some(p.budget), *lin, Some(lin))?;
                put(b, LINEAGE, None, *lin, Some(&l))?;
            }
        }
        let next = PopulationV7 { members: root(&b.state, POPULATION, Some(p.budget))?, ..*p };
        let old_id = {
            let retained_inventory = inventory(&p)?;
            inventory_id(&retained_inventory)?
        };
        let next_id = {
            let retained_inventory = inventory(&next)?;
            inventory_id(&retained_inventory)?
        };
        put_population(b, &next)?;
        if old_id != next_id {
            b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(old_id), None)
                .map_err(native_err)?;
        }
        Ok(())
    })
    .map_err(err)
}

/// Only actual final-zero observation after ALL rows/child transfers of this
/// native operation constructs a closure token. No missing-row shortcut.
pub(super) struct ActualLineageClosedV9 {
    lineage: Hash64,
}
impl ActualLineageClosedV9 {
    pub(super) fn lineage(&self) -> Hash64 {
        self.lineage
    }
}
pub(super) fn actual_pending_lineages_v9(
    b: &mut TransitionBuilder<'_>,
    income: Hash64,
    expected_root: Hash64,
) -> Result<NativeRetainedV1<Vec<Hash64>>, ContextErrorV2> {
    let key = crate::palw_bond_budget_v1::material_pending_id_v1(income);
    let point = b.claim_local_lock_point_v6.ok_or(ContextErrorV2::Unavailable("actual checker material point absent"))?;
    let i = observe_owned_current_v7(b, point, key)?;
    if i.current.root != Some(expected_root)
        || i.current.payment != (Payment::Pending { income })
        || b.state.bond_budget.as_ref().and_then(|book| book.material_income.get(&key)) != Some(&i.current)
    {
        return Err(bad("actual checker Pending native binding differs"));
    }
    ancestry(&b.state, i.id)
}
pub(super) fn observed_lineage_active_v9(s: &PalwChainStateV2, id: Hash64) -> Result<Option<u64>, ContextErrorV2> {
    lookup::<LineageV7>(s, LINEAGE, None, id).map(|l| l.map(|l| l.active))
}
