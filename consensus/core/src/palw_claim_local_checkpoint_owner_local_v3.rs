//! ISOLATED child of checkpoint_descriptor_v3. The first bounded path handles
//! actual signed non-credit material blocks in authenticated non-final Chain worlds.
use super::*;
const HOT: u32 = 14;
const BASE: u32 = 15;
const REFS: u32 = 16;
const BASES: u32 = 17;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct HotV3 {
    version: u16,
    point: PalwBlockContextV2,
    records: ComponentRootV2,
    bases: ComponentRootV2,
    refs: ComponentRootV2,
    eligible: bool,
}
crate::palw_native_resource_v1::wire::wire_struct!(HotV3;u16,PalwBlockContextV2,ComponentRootV2,ComponentRootV2,ComponentRootV2,bool);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct BaseV3 {
    version: u16,
    work: Work,
    scope: Hash64,
    root: ComponentRootV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(BaseV3;u16,Work,Hash64,ComponentRootV2);
/// No caller constructor: only the full expected fold's pre-projection callback
/// creates exact base membership, including content shared with projected roles.
pub(crate) struct PreparedBasesV3 {
    rows: NativeRetainedV1<BTreeMap<Work, BTreeSet<Hash64>>>,
    eligible: bool,
}
fn eligible_record(r: &Record) -> bool {
    matches!(r.work, Work::ChainClaim { .. })
        && !r.final_claim
        && !r.voided
        && !r.certified
        && !r.liability_closed
        && r.reservations.is_empty()
}
pub(super) fn prepare(
    records: &BTreeMap<Work, Record>,
) -> Result<PreparedBasesV3, PalwStateV2Error> {
    let eligible = records.values().all(eligible_record);
    let n = if eligible { records.len() as u64 } else { 0 };
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Work, BTreeSet<Hash64>>>(n)
        .map_err(PalwStateV2Error::NativeResource)?;
    if eligible {
        for r in records.values() {
            q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(
                r.material.len() as u64,
            )
            .map_err(PalwStateV2Error::NativeResource)?;
        }
    }
    let lease =
        reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut rows = BTreeMap::new();
    if eligible {
        for r in records.values() {
            rows.insert(r.work, r.material.clone());
        }
    }
    Ok(PreparedBasesV3 {
        rows: NativeRetainedV1::from_owner_reserved_v1(rows, lease),
        eligible,
    })
}
fn hot(s: &PalwChainStateV2) -> Result<Option<HotV3>, ContextErrorV2> {
    let r = lookup::<HotV3>(s, HOT, None, Hash64::default().as_byte_slice())?;
    if let Some(r) = r {
        if r.version != 3
            || r.records.spec != spec(RECORDS)
            || r.bases.spec != spec(BASES)
            || r.refs.spec != spec(REFS)
        {
            return Err(bad("owner-local summary profile"));
        }
        Ok(Some(*r))
    } else {
        Ok(None)
    }
}
/// Actual full checkpoint writer only. All counts derive complete logical owner
/// sets; one source's ID counts once globally even if several local roles share it.
pub(super) fn publish_full(
    b: &mut TransitionBuilder<'_>,
    cp: &Checkpoint,
    prepared: &PreparedBasesV3,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let old = keys(&b.state, BASES, None)?;
        for key in old.iter() {
            let row = lookup::<BaseV3>(&b.state, BASES, None, key)?
                .ok_or(bad("base directory absent"))?;
            if !prepared.eligible
                || !prepared.rows.contains_key(&row.work)
                || cp.records.get(&row.work).map(scalar_scope).transpose()? != Some(row.scope)
            {
                drop_owned(b, BASE, row.scope)?;
                put_raw(b, BASES, None, key, None)?;
            }
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, u64>>(
            if prepared.eligible {
                cp.material.len() as u64
            } else {
                0
            },
        )
        .map_err(ContextErrorV2::Resource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut refs = BTreeMap::<Hash64, u64>::new();
        if prepared.eligible {
            for record in cp.records.values() {
                if !eligible_record(record) {
                    return Err(bad("prepared base record flags changed"));
                }
                let ids = prepared
                    .rows
                    .get(&record.work)
                    .ok_or(bad("prepared base missing source"))?;
                if !ids.is_subset(&record.material) {
                    return Err(bad("prepared base lost membership"));
                }
                let scope = scalar_scope(record)?;
                let root = sync_set(b, BASE, scope, ids)?;
                let row = BaseV3 {
                    version: 3,
                    work: record.work,
                    scope,
                    root,
                };
                put(b, BASES, None, scope.as_byte_slice(), Some(&row))?;
                for id in &record.material {
                    let n = refs.entry(*id).or_default();
                    *n = n
                        .checked_add(1)
                        .ok_or(bad("material owner references overflow"))?;
                }
            }
        }
        let old = keys(&b.state, REFS, None)?;
        for key in old.iter() {
            let id: Hash64 =
                borsh::from_slice(key).map_err(|_| bad("material owner reference key"))?;
            if !refs.contains_key(&id) {
                put_raw(b, REFS, None, key, None)?;
            }
        }
        for (id, n) in refs {
            put(b, REFS, None, id.as_byte_slice(), Some(&n))?;
        }
        let h = HotV3 {
            version: 3,
            point: cp.point,
            records: root(&b.state, RECORDS, None)?,
            bases: root(&b.state, BASES, None)?,
            refs: root(&b.state, REFS, None)?,
            eligible: prepared.eligible,
        };
        put(b, HOT, None, Hash64::default().as_byte_slice(), Some(&h))?;
        store::collect_unreachable_v2(b)?;
        Ok(())
    })
    .map_err(err)
}
/// Cold/import comparison uses the same actual pre-projection native generation;
/// an attacker cannot hide a base role behind an equal projected body ID.
pub(super) fn validate_actual_bases(
    s: &PalwChainStateV2,
    actual: &BTreeMap<Work, Record>,
) -> Result<(), PalwStateV2Error> {
    let Some(h) = hot(s).map_err(err)? else {
        return Ok(());
    };
    let eligible = actual.values().all(eligible_record);
    if eligible != h.eligible {
        return Err(err(bad("actual owner-local eligibility differs")));
    }
    if !eligible {
        return Ok(());
    }
    if h.bases.rows != actual.len() as u64 {
        return Err(err(bad("actual base source population differs")));
    }
    for record in actual.values() {
        let sc = scalar_scope(record).map_err(err)?;
        let row = lookup::<BaseV3>(s, BASES, None, sc.as_byte_slice())
            .map_err(err)?
            .ok_or_else(|| err(bad("actual source base absent")))?;
        if row.work != record.work
            || row.scope != sc
            || row.root.rows != record.material.len() as u64
        {
            return Err(err(bad("actual source base identity/count")));
        }
        for id in &record.material {
            if nodes::lookup(&StateNodesV2(s), row.root, id.as_byte_slice())
                .map_err(err)?
                .is_none()
            {
                return Err(err(bad("actual base material missing")));
            }
        }
    }
    Ok(())
}
fn change_ref(b: &mut TransitionBuilder<'_>, id: Hash64, add: bool) -> Result<(), ContextErrorV2> {
    let n = lookup::<u64>(&b.state, REFS, None, id.as_byte_slice())?.map_or(0, |n| *n);
    let next = if add {
        n.checked_add(1)
            .ok_or(bad("owner material reference overflow"))?
    } else {
        n.checked_sub(1)
            .ok_or(bad("owner material reference underflow"))?
    };
    if next == 0 {
        put_raw(b, REFS, None, id.as_byte_slice(), None)?;
        for table in [LITERALS, NATIVE, COORD_REFS] {
            put_raw(b, table, None, id.as_byte_slice(), None)?;
        }
    } else {
        put(b, REFS, None, id.as_byte_slice(), Some(&next))?;
    }
    Ok(())
}
fn member_add(
    b: &mut TransitionBuilder<'_>,
    scope: Hash64,
    id: Hash64,
) -> Result<(), ContextErrorV2> {
    if nodes::lookup(
        &StateNodesV2(&b.state),
        root(&b.state, MATERIALS, Some(scope))?,
        id.as_byte_slice(),
    )?
    .is_none()
    {
        put_raw(b, MATERIALS, Some(scope), id.as_byte_slice(), Some(&[1]))?;
        change_ref(b, id, true)?;
    }
    Ok(())
}
fn member_remove_unused(
    b: &mut TransitionBuilder<'_>,
    base: BaseV3,
    coords: ComponentRootV2,
    id: Hash64,
    index: Option<Hash64>,
) -> Result<(), ContextErrorV2> {
    if index == Some(id)
        || nodes::lookup(&StateNodesV2(&b.state), base.root, id.as_byte_slice())?.is_some()
    {
        return Ok(());
    }
    let mut used = false;
    nodes::complete_rows(&StateNodesV2(&b.state), coords, |_, v| {
        if decode::<CoordinateV3>(body(&b.state, v)?)?.id == id {
            used = true
        }
        Ok(())
    })?;
    if !used {
        let r = root(&b.state, MATERIALS, Some(base.scope))?;
        if nodes::lookup(&StateNodesV2(&b.state), r, id.as_byte_slice())?.is_some() {
            put_raw(b, MATERIALS, Some(base.scope), id.as_byte_slice(), None)?;
            change_ref(b, id, false)?;
        }
    }
    Ok(())
}
fn storage_only_row(key: &[u8], raw: Option<&Vec<u8>>) -> Result<bool, ContextErrorV2> {
    match key.first().copied() {
        Some(b'N' | b'B') => Ok(key.len() == 65),
        Some(b'I') => {
            let at = match key.len() {
                18 => 1,
                84 if key.get(1..3) == Some(&[2, 0]) => 67,
                _ => return Ok(false),
            };
            let k: ComponentSpecV2 =
                borsh::from_slice(&key[at..]).map_err(|_| bad("material-only current key"))?;
            if !matches!(k.owner, 8 | 10 | 11) {
                return Ok(false);
            }
            if let Some(raw) = raw {
                let r = decode::<ComponentRootV2>(raw)?;
                if r.spec != k {
                    return Err(bad("material-only current key/spec"));
                }
            }
            Ok(true)
        }
        Some(b'A') => {
            let Some(raw) = raw else { return Ok(true) };
            if raw.get(..2) != Some(&[2, 0]) {
                return Ok(false);
            }
            let a = decode::<store::ArchiveReferencesV2>(raw)?;
            Ok(a.version == 2
                && a.literals.is_empty()
                && a.components
                    .iter()
                    .all(|r| matches!(r.spec.owner, 8 | 10 | 11)))
        }
        _ => Ok(false),
    }
}
fn strict_material_journal(b: &TransitionBuilder<'_>) -> Result<bool, ContextErrorV2> {
    for e in &b.entries {
        if let PalwDeltaEntryV2::KernelRouteRow {
            table,
            key,
            old,
            new,
        } = e
        {
            if *table == PALW_CLAIM_LOCAL_TABLE_V1
                && (!storage_only_row(key, old.as_ref())? || !storage_only_row(key, new.as_ref())?)
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn current_coordinate_reference(
    s: &PalwChainStateV2,
    row: &ScalarV3,
) -> Result<Option<CoordinateRefV3>, ContextErrorV2> {
    let mut answer = None;
    nodes::complete_rows(&StateNodesV2(s), row.material, |key, _| {
        if let Some(r) = lookup::<CoordinateRefV3>(s, COORD_REFS, None, key)? {
            if r.scope == row.scope {
                if answer.replace(*r).is_some() {
                    return Err(bad("multiple current coordinate images per owner"));
                }
            }
        }
        Ok(())
    })?;
    Ok(answer)
}
fn publish_literal_if_missing(
    b: &mut TransitionBuilder<'_>,
    raw: &[u8],
) -> Result<(), ContextErrorV2> {
    let id = material_id(raw);
    let reader = StateNodesV2(&b.state);
    if nodes::lookup(&reader, root(&b.state, LITERALS, None)?, id.as_byte_slice())?.is_none()
        && nodes::lookup(&reader, root(&b.state, NATIVE, None)?, id.as_byte_slice())?.is_none()
        && nodes::lookup(
            &reader,
            root(&b.state, COORD_REFS, None)?,
            id.as_byte_slice(),
        )?
        .is_none()
    {
        put_raw(b, LITERALS, None, id.as_byte_slice(), Some(raw))?;
    }
    Ok(())
}
fn stabilize_deleted_native(
    b: &mut TransitionBuilder<'_>,
    table: u8,
    key: &[u8],
    old: &[u8],
    new: Option<&[u8]>,
) -> Result<(), ContextErrorV2> {
    let id = material_id(old);
    if new == Some(old) {
        return Ok(());
    }
    if let Some(r) = lookup::<NativeRefV3>(&b.state, NATIVE, None, id.as_byte_slice())? {
        if r.table == table && r.key.as_slice() == key {
            put_raw(b, NATIVE, None, id.as_byte_slice(), None)?;
            put_raw(b, LITERALS, None, id.as_byte_slice(), Some(old))?;
        }
    }
    Ok(())
}
struct DirtyCoordinateV3 {
    owner: Hash64,
    table: u8,
    key: Vec<u8>,
    old: Option<Vec<u8>>,
    new: Option<Vec<u8>>,
}
/// Copy only actual dirty source coordinates after exact key/body demand is
/// admitted; their owning native journal remains intact throughout publication.
fn exact_dirty_encode<T: BorshSerialize>(value: &T) -> Result<Vec<u8>, ContextErrorV2> {
    let n = borsh::object_length(value).map_err(|_| bad("dirty exact length"))?;
    // Caller admitted this precise buffer in the complete dirty-coordinate quote.
    let mut out = Vec::with_capacity(n);
    value
        .serialize(&mut out)
        .map_err(|_| bad("dirty exact encoding"))?;
    if out.len() != n {
        return Err(bad("dirty exact encoding length changed"));
    }
    Ok(out)
}
fn dirty_coordinates(
    b: &TransitionBuilder<'_>,
) -> Result<NativeRetainedV1<Vec<DirtyCoordinateV3>>, ContextErrorV2> {
    let mut count = 0u64;
    let mut bytes = 0u64;
    for e in &b.entries {
        match e {
            PalwDeltaEntryV2::KernelRouteRow {
                table: 169 | 192,
                key,
                old,
                new,
            } => {
                count = count.checked_add(1).ok_or(bad("dirty coordinate count"))?;
                bytes = bytes
                    .checked_add(key.len() as u64)
                    .and_then(|n| n.checked_add(old.as_ref().map_or(0, |r| r.len()) as u64))
                    .and_then(|n| n.checked_add(new.as_ref().map_or(0, |r| r.len()) as u64))
                    .ok_or(bad("dirty coordinate bytes"))?;
            }
            PalwDeltaEntryV2::StagedCourtSeed { key, old, new } if staged_owner(key).is_some() => {
                count = count.checked_add(1).ok_or(bad("dirty staged count"))?;
                bytes = bytes
                    .checked_add(
                        borsh::object_length(key).map_err(|_| bad("dirty staged key length"))?
                            as u64,
                    )
                    .ok_or(bad("dirty staged key bytes"))?;
                for r in [old.as_ref(), new.as_ref()].into_iter().flatten() {
                    bytes = bytes
                        .checked_add(
                            borsh::object_length(&(key, r))
                                .map_err(|_| bad("dirty staged pair length"))?
                                as u64,
                        )
                        .ok_or(bad("dirty staged body bytes"))?;
                }
            }
            _ => {}
        }
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<DirtyCoordinateV3>(count)
        .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<u8>(bytes).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out =
        Vec::with_capacity(usize::try_from(count).map_err(|_| bad("dirty coordinate host count"))?);
    for (ordinal, e) in b.entries.iter().enumerate() {
        let Some(span) = b
            .claim_local_family_spans
            .iter()
            .find(|s| s.first <= ordinal && ordinal < s.end)
        else {
            continue;
        };
        match e {
            PalwDeltaEntryV2::KernelRouteRow {
                table: 169 | 192,
                key,
                old,
                new,
            } => {
                let PalwDeltaEntryV2::KernelRouteRow { table, .. } = e else {
                    unreachable!()
                };
                out.push(DirtyCoordinateV3 {
                    owner: span.claim,
                    table: *table,
                    key: key.clone(),
                    old: old.clone(),
                    new: new.clone(),
                });
            }
            PalwDeltaEntryV2::StagedCourtSeed { key, old, new } if staged_owner(key).is_some() => {
                if staged_owner(key) != Some(span.claim) {
                    return Err(bad("actual staged source/span differs"));
                }
                out.push(DirtyCoordinateV3 {
                    owner: span.claim,
                    table: 255,
                    key: exact_dirty_encode(key)?,
                    old: old
                        .as_ref()
                        .map(|r| exact_dirty_encode(&(key, r)))
                        .transpose()?,
                    new: new
                        .as_ref()
                        .map(|r| exact_dirty_encode(&(key, r)))
                        .transpose()?,
                });
            }
            _ => {}
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn actual_aux<'a>(
    b: &'a TransitionBuilder<'_>,
    table: u8,
    key: &[u8],
) -> Result<Option<&'a [u8]>, ContextErrorV2> {
    let _key = reserve_native_requests_v1(key.len() as u64).map_err(ContextErrorV2::Resource)?;
    Ok(b.state
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(table, key.to_vec())))
        .map(Vec::as_slice)
        .or_else(|| {
            b.entries.iter().rev().find_map(|e| match e {
                PalwDeltaEntryV2::KernelRouteRow {
                    table: t,
                    key: k,
                    old: Some(old),
                    ..
                } if *t == table && k.as_slice() == key => Some(old.as_slice()),
                _ => None,
            })
        }))
}
fn at_hash(raw: &[u8], at: usize) -> Result<Hash64, ContextErrorV2> {
    Ok(Hash64::from_bytes(
        raw.get(at..at + 64)
            .ok_or(bad("material owner identity width"))?
            .try_into()
            .map_err(|_| bad("material owner identity"))?,
    ))
}
fn contract_owner(b: &TransitionBuilder<'_>, id: Hash64) -> Result<Hash64, ContextErrorV2> {
    let raw = actual_aux(b, 192, id.as_byte_slice())?.ok_or(ContextErrorV2::Unavailable(
        "actual material supplier missing",
    ))?;
    let r = decode::<crate::palw_court_family_supplier_v1::FamilySupplierRowV1>(raw)?;
    if r.consent
        .id_v1()
        .map_err(|_| bad("actual supplier contract ID"))?
        != id
    {
        return Err(bad("actual supplier identity differs"));
    }
    Ok(r.consent.terms.claim)
}
fn row_owner(
    b: &TransitionBuilder<'_>,
    table: u8,
    key: &[u8],
    raw: &[u8],
) -> Result<Hash64, ContextErrorV2> {
    match table {
        169 => match (key.len(), key.first().copied()) {
            (65, Some(4)) => {
                let claim = at_hash(raw, 0)?;
                if at_hash(key, 1)? != claim {
                    return Err(bad("material table source differs"));
                }
                Ok(claim)
            }
            (65, Some(1 | 3 | 5 | 6)) => contract_owner(b, at_hash(raw, 0)?),
            (73, Some(2)) => {
                let draft = at_hash(raw, 0)?;
                if at_hash(key, 1)? != draft {
                    return Err(bad("material part draft differs"));
                }
                // Stack key; actual_aux admits its independent lookup Vec.
                let mut k = [0u8; 65];
                k[0] = 1;
                k[1..].copy_from_slice(draft.as_byte_slice());
                let raw = actual_aux(b, 169, &k)?
                    .ok_or(ContextErrorV2::Unavailable("actual material draft missing"))?;
                contract_owner(b, at_hash(raw, 0)?)
            }
            _ => Err(bad("material-only unallocated body")),
        },
        192 => {
            let id = match key.len() {
                64 => at_hash(key, 0)?,
                65 if matches!(key[0], 1 | 4) => {
                    let id = at_hash(raw, 2)?;
                    if id != at_hash(key, 1)? {
                        return Err(bad("material status contract differs"));
                    }
                    id
                }
                129 if matches!(key[0], 2 | 3) => {
                    let id = at_hash(raw, 2)?;
                    if id != at_hash(key, 1)? {
                        return Err(bad("material completion contract differs"));
                    }
                    id
                }
                _ => return Err(bad("material-only unallocated finance")),
            };
            contract_owner(b, id)
        }
        _ => Err(bad("material-only owner table")),
    }
}
fn replace_source(
    b: &mut TransitionBuilder<'_>,
    source: Hash64,
    dirty: &[DirtyCoordinateV3],
) -> Result<(), ContextErrorV2> {
    let key = work_key(Work::ChainClaim { claim: source })?;
    let mut row = *lookup::<ScalarV3>(&b.state, RECORDS, None, &key)?
        .ok_or(bad("dirty source has no prior record"))?;
    if row.final_claim
        || row.voided
        || row.certified
        || row.liability_closed
        || row.reservations.rows != 0
    {
        return Err(bad("dirty source is not stable noncredit"));
    }
    let base = *lookup::<BaseV3>(&b.state, BASES, None, row.scope.as_byte_slice())?
        .ok_or(bad("dirty source exact base absent"))?;
    if base.version != 3
        || base.work != row.work
        || base.scope != row.scope
        || base.root.spec != spec(BASE)
    {
        return Err(bad("dirty source base identity"));
    }
    let prior = current_coordinate_reference(&b.state, &row)?;
    let before = prior.map_or(root(&b.state, COORDS, Some(row.scope))?, |r| r.root);
    if root(&b.state, COORDS, Some(row.scope))? != before {
        return Err(bad("dirty source prior coordinate pointer"));
    }
    let changes = dirty.iter().filter(|d| d.owner == source);
    let count = changes.clone().count() as u64;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(count)
        .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<u8>(131).map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut old_ids = BTreeSet::new();
    let mut coordinate_key = Vec::with_capacity(131);
    for d in changes {
        coordinate_key.clear();
        coordinate_key.push(d.table);
        coordinate_key.extend_from_slice(&d.key);
        let previous = nodes::lookup(
            &StateNodesV2(&b.state),
            root(&b.state, COORDS, Some(row.scope))?,
            &coordinate_key,
        )?
        .map(|v| decode::<CoordinateV3>(body(&b.state, v)?))
        .transpose()?;
        let expected = d.old.as_ref().map(|raw| CoordinateV3 {
            id: material_id(raw),
            len: raw.len() as u64,
        });
        if previous.as_deref().copied() != expected {
            return Err(bad("actual dirty coordinate inverse mismatch"));
        }
        if let Some(raw) = &d.old {
            old_ids.insert(material_id(raw));
            if d.table != 255 {
                stabilize_deleted_native(b, d.table, &d.key, raw, d.new.as_deref())?;
            }
        }
        let next = d.new.as_ref().map(|raw| CoordinateV3 {
            id: material_id(raw),
            len: raw.len() as u64,
        });
        put(b, COORDS, Some(row.scope), &coordinate_key, next.as_ref())?;
        if let Some(raw) = &d.new {
            publish_literal_if_missing(b, raw)?;
            member_add(b, row.scope, material_id(raw))?;
        }
    }
    let coords = root(&b.state, COORDS, Some(row.scope))?;
    let next = if coords.rows == 0 {
        None
    } else {
        let provisional = CoordinateRefV3 {
            version: 3,
            claim: source,
            scope: row.scope,
            root: coords,
            id: Hash64::default(),
            len: 0,
        };
        let mut w = CanonicalWriterV3::new();
        write_index(
            source,
            coords.rows,
            |out| {
                visit_coordinates(&b.state, &provisional, |table, key, v| {
                    (table, key, v.id, v.len)
                        .serialize(out)
                        .map_err(|_| bad("dirty coordinate logical hash"))
                })
            },
            &mut w,
        )?;
        let (id, len) = w.finish();
        Some(CoordinateRefV3 {
            id,
            len,
            ..provisional
        })
    };
    let new_id = next.map(|r| r.id);
    // Remove only the old INDEX role. Equal base/native-coordinate content remains
    // a member and is reclassified as a literal before its old index ref is gone.
    if let Some(old) = prior {
        if new_id != Some(old.id) {
            member_remove_unused(b, base, coords, old.id, new_id)?;
            if lookup::<u64>(&b.state, REFS, None, old.id.as_byte_slice())?.is_some() {
                let bytes = coordinate_bytes(&b.state, &old)?;
                put_raw(b, COORD_REFS, None, old.id.as_byte_slice(), None)?;
                put_raw(b, LITERALS, None, old.id.as_byte_slice(), Some(&bytes))?;
            }
        }
    }
    for id in old_ids {
        member_remove_unused(b, base, coords, id, new_id)?;
    }
    if let Some(r) = next {
        for table in [LITERALS, NATIVE] {
            put_raw(b, table, None, r.id.as_byte_slice(), None)?;
        }
        put(b, COORD_REFS, None, r.id.as_byte_slice(), Some(&r))?;
        member_add(b, row.scope, r.id)?;
    }
    if coords.rows == 0 {
        drop_owned(b, COORDS, row.scope)?;
    }
    row.material = root(&b.state, MATERIALS, Some(row.scope))?;
    put(b, RECORDS, None, &key, Some(&row))?;
    Ok(())
}
/// Returns false before mutation for unsupported blocks/cohorts. A real update
/// either publishes atomically or propagates its resource/integrity error.
pub(super) fn try_material(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    merged: &[PalwMergedWorkV1<'_>],
) -> Result<bool, PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) || objects.is_empty() {
        return Ok(false);
    }
    // Material-only path never owns frontier mutation or its journal.
    if (b.state.safe_frontier_blue_score, b.state.safe_frontier)
        != (parent.safe_frontier_blue_score, parent.safe_frontier)
    {
        return Ok(false);
    }
    if !super::super::palw_claim_local_material_projection_v2::noncredit_only(
        b, objects, work, merged,
    )? {
        return Ok(false);
    }
    let Some(previous) = descriptor(parent).map_err(err)? else {
        return Ok(false);
    };
    let Some(current) = descriptor(&b.state).map_err(err)? else {
        return Ok(false);
    };
    if previous != current {
        return Ok(false);
    }
    let Some(summary) = hot(parent).map_err(err)? else {
        return Ok(false);
    };
    if !summary.eligible
        || summary.point != previous.point
        || summary.records != previous.records
        || hot(&b.state).map_err(err)? != Some(summary)
    {
        return Ok(false);
    }
    // A future-committed raw kernel row must not appear merely because DAA
    // advanced. Valid ordinary admissions are current, but the explicit local
    // guard also prevents a malformed imported hidden row from using this path.
    if b.state.kernel_route.as_ref().is_some_and(|r| {
        r.rows
            .range((misaka_palw_kernel::rows::TABLE_CLAIMS_V1, Vec::new())..)
            .next()
            .is_some_and(|((t, _), _)| *t == misaka_palw_kernel::rows::TABLE_CLAIMS_V1)
    }) {
        return Ok(false);
    }
    // The prior checkpoint predates end-fold native39 promotion/settlement.
    // This first Chain-only local path does not certify latent native vertices.
    // Keep their ordinary complete expected fold, including its actual clocks.
    if !b.state.exec_v2.roots.is_empty() || !b.state.exec_v2.slices.is_empty() {
        return Ok(false);
    }
    if b.state.kernel_route.as_ref().is_some_and(|r| {
        [
            crate::palw_native_mining_v39::MINING_TABLE_V39,
            crate::palw_native39_frontier_v1::NATIVE39_FRONTIER_TABLE_V1,
            crate::palw_native_closed_facts_v1::NATIVE_CLOSED_FACTS_TABLE_V1,
        ].into_iter().any(|table| r.aux.range((table, Vec::new())..).next()
            .is_some_and(|((actual, _), _)| *actual == table))
    }) {
        return Ok(false);
    }
    if !strict_material_journal(b).map_err(err)? {
        return Ok(false);
    }
    let dirty = dirty_coordinates(b).map_err(err)?;
    if dirty.is_empty() {
        return Ok(false);
    }
    for d in dirty.iter() {
        if d.table != 255 {
            for raw in [d.old.as_ref(), d.new.as_ref()].into_iter().flatten() {
                if row_owner(b, d.table, &d.key, raw).map_err(err)? != d.owner {
                    return Err(err(bad("actual material source/span differs")));
                }
            }
        }
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(
        dirty.len() as u64
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    let _lease =
        reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let owners: BTreeSet<_> = dirty.iter().map(|d| d.owner).collect();
    atomic(b, |b| {
        for owner in owners {
            replace_source(b, owner, &dirty)?;
        }
        let d = DescriptorV3 {
            point: *ctx,
            records: root(&b.state, RECORDS, None)?,
            literals: root(&b.state, LITERALS, None)?,
            native: root(&b.state, NATIVE, None)?,
            coordinates: root(&b.state, COORD_REFS, None)?,
            ..current
        };
        let summary = HotV3 {
            point: *ctx,
            records: d.records,
            refs: root(&b.state, REFS, None)?,
            ..summary
        };
        put(
            b,
            HOT,
            None,
            Hash64::default().as_byte_slice(),
            Some(&summary),
        )?;
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &dkey(), Some(&d))
            .map_err(native_err)?;
        store::collect_unreachable_v2(b)?;
        Ok(())
    })
    .map_err(err)?;
    // Frontier bytes and entries remain owned by the full checkpoint writer.
    Ok(true)
}
/// Typed current/archived metadata closure. Actual base authenticity is checked
/// by validate_actual_bases during the ordinary cold expected fold.
pub(super) fn gc(
    s: &PalwChainStateV2,
    c: ComponentRootV2,
    key: &[u8],
    v: RowValueV2,
    mut keep: impl FnMut(ComponentRootV2) -> Result<(), ContextErrorV2>,
) -> Result<bool, ContextErrorV2> {
    match c.spec.table {
        HOT => {
            if key != Hash64::default().as_byte_slice() {
                return Err(bad("hot summary key"));
            }
            let h = decode::<HotV3>(body(s, v)?)?;
            if h.version != 3
                || h.records.spec != spec(RECORDS)
                || h.bases.spec != spec(BASES)
                || h.refs.spec != spec(REFS)
            {
                return Err(bad("hot summary rooted profile"));
            }
            for r in [h.records, h.bases, h.refs] {
                keep(r)?;
            }
        }
        BASES => {
            let r = decode::<BaseV3>(body(s, v)?)?;
            if r.version != 3
                || key != r.scope.as_byte_slice()
                || r.root.spec != spec(BASE)
                || !matches!(r.work, Work::ChainClaim { .. })
            {
                return Err(bad("base directory rooted profile"));
            }
            keep(r.root)?;
        }
        BASE => {
            if key.len() != 64 || body(s, v)? != [1] {
                return Err(bad("base membership profile"));
            }
        }
        REFS => {
            if key.len() != 64 || *decode::<u64>(body(s, v)?)? == 0 {
                return Err(bad("material owner reference profile"));
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
pub(super) fn current_owned(
    s: &PalwChainStateV2,
    table: u32,
    scope: Hash64,
) -> Result<Option<ComponentRootV2>, ContextErrorV2> {
    if table != BASE {
        return Ok(None);
    }
    let r = lookup::<BaseV3>(s, BASES, None, scope.as_byte_slice())?
        .ok_or(bad("orphan base owned pointer"))?;
    if r.scope != scope {
        return Err(bad("base owned source differs"));
    }
    Ok(Some(r.root))
}
pub(super) fn validate(s: &PalwChainStateV2, d: &DescriptorV3) -> Result<(), ContextErrorV2> {
    let Some(h) = hot(s)? else {
        for t in [HOT, BASES, REFS] {
            if root(s, t, None)?.rows != 0 {
                return Err(bad("owner-local roots without summary"));
            }
        }
        return Ok(());
    };
    if root(s, HOT, None)?.rows != 1
        || h.point != d.point
        || h.records != d.records
        || root(s, BASES, None)? != h.bases
        || root(s, REFS, None)? != h.refs
    {
        return Err(bad("hot summary current binding"));
    }
    let reader = StateNodesV2(s);
    let mut eligibility = true;
    let mut total = 0u64;
    nodes::complete_rows(&reader, d.records, |key, v| {
        let r = decode::<ScalarV3>(body(s, v)?)?;
        checked_scalar(&r, key)?;
        eligibility &= matches!(r.work, Work::ChainClaim { .. })
            && !r.final_claim
            && !r.voided
            && !r.certified
            && !r.liability_closed
            && r.reservations.rows == 0;
        total = total
            .checked_add(r.material.rows)
            .ok_or(bad("material owner population sum"))?;
        Ok(())
    })?;
    if h.eligible != eligibility || (!h.eligible && (h.bases.rows != 0 || h.refs.rows != 0)) {
        return Err(bad("hot summary eligibility/population"));
    }
    if !h.eligible {
        return Ok(());
    }
    if h.bases.rows != d.records.rows {
        return Err(bad("base directory complete population"));
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, u64>>(total)
        .map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut actual = BTreeMap::<Hash64, u64>::new();
    nodes::complete_rows(&reader, d.records, |key, v| {
        let r = decode::<ScalarV3>(body(s, v)?)?;
        checked_scalar(&r, key)?;
        let base = lookup::<BaseV3>(s, BASES, None, r.scope.as_byte_slice())?
            .ok_or(bad("record exact base absent"))?;
        if base.work != r.work
            || base.scope != r.scope
            || root(s, BASE, Some(r.scope))? != base.root
        {
            return Err(bad("record exact base current binding"));
        }
        stream_set_valid::<Hash64>(s, base.root)?;
        nodes::complete_rows(&reader, base.root, |id, _| {
            if nodes::lookup(&reader, r.material, id)?.is_none() {
                return Err(bad("base is not logical material member"));
            }
            Ok(())
        })?;
        nodes::complete_rows(&reader, r.material, |key, _| {
            let id: Hash64 =
                borsh::from_slice(key).map_err(|_| bad("material owner member key"))?;
            let n = actual.entry(id).or_default();
            *n = n
                .checked_add(1)
                .ok_or(bad("material owner count overflow"))?;
            Ok(())
        })?;
        Ok(())
    })?;
    if actual.len() as u64 != h.refs.rows {
        return Err(bad("material owner reference population differs"));
    }
    for (id, n) in actual {
        if lookup::<u64>(s, REFS, None, id.as_byte_slice())?
            .as_deref()
            .copied()
            != Some(n)
        {
            return Err(bad("material owner exact reference count differs"));
        }
    }
    Ok(())
}
pub(super) fn allocated(table: u32) -> bool {
    matches!(table, HOT | BASE | REFS | BASES)
}
pub(super) fn global(table: u32) -> bool {
    matches!(table, HOT | REFS | BASES)
}
#[cfg(test)]
mod role_tests {
    use super::*;
    #[test]
    fn rfc23s2_equal_content_base_projection_and_two_owners_preserve_exact_membership() {
        // A storage-role regression, deliberately not source admission/approval.
        // The SAME actual bytes/data-ID occupy both roles and two owners; no
        // inequality assumption between typed application encodings is used.
        let h = Hash64::from_u64_word;
        let params =
            PalwStateParamsV2::new(100, 100, 10, 20, 50, 1000, h(1), 4, 1000, 100, 1000, 0)
                .unwrap()
                .with_claim_local_settlement_from_daa(Some(0));
        let extras = PalwTransitionExtrasV1::default();
        let mut state = PalwChainStateV2::genesis();
        state.kernel_route = Some(
            crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new_typed(
                crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(2), h(3)),
                None,
                None,
                misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 0, burned: 0 },
            ),
        );
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let bytes = b"one actual body shared by independent storage roles";
        let id = material_id(bytes);
        let a = h(10);
        let z = h(11);
        publish_literal_if_missing(&mut b, bytes).unwrap();
        member_add(&mut b, a, id).unwrap();
        member_add(&mut b, z, id).unwrap();
        put_raw(&mut b, BASE, Some(a), id.as_byte_slice(), Some(&[1])).unwrap();
        let mut base = BaseV3 {
            version: 3,
            work: Work::ChainClaim { claim: h(12) },
            scope: a,
            root: root(&b.state, BASE, Some(a)).unwrap(),
        };
        let coordinate_key = [3u8; 66];
        let cv = CoordinateV3 {
            id,
            len: bytes.len() as u64,
        };
        put(&mut b, COORDS, Some(a), &coordinate_key, Some(&cv)).unwrap();
        put_raw(&mut b, COORDS, Some(a), &coordinate_key, None).unwrap();
        let empty = root(&b.state, COORDS, Some(a)).unwrap();
        member_remove_unused(&mut b, base, empty, id, None).unwrap();
        assert_eq!(
            *lookup::<u64>(&b.state, REFS, None, id.as_byte_slice())
                .unwrap()
                .unwrap(),
            2
        );
        assert!(
            nodes::lookup(
                &StateNodesV2(&b.state),
                root(&b.state, MATERIALS, Some(a)).unwrap(),
                id.as_byte_slice()
            )
            .unwrap()
            .is_some(),
            "removing projected role must preserve equal base role"
        );
        put_raw(&mut b, BASE, Some(a), id.as_byte_slice(), None).unwrap();
        base.root = root(&b.state, BASE, Some(a)).unwrap();
        member_remove_unused(&mut b, base, empty, id, None).unwrap();
        assert_eq!(
            *lookup::<u64>(&b.state, REFS, None, id.as_byte_slice())
                .unwrap()
                .unwrap(),
            1
        );
        let literal = nodes::lookup(
            &StateNodesV2(&b.state),
            root(&b.state, LITERALS, None).unwrap(),
            id.as_byte_slice(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            body(&b.state, literal).unwrap(),
            bytes,
            "other owner's physical body remains readable"
        );
        let second = BaseV3 {
            version: 3,
            work: Work::ChainClaim { claim: h(13) },
            scope: z,
            root: root(&b.state, BASE, Some(z)).unwrap(),
        };
        member_remove_unused(&mut b, second, empty, id, None).unwrap();
        assert!(
            lookup::<u64>(&b.state, REFS, None, id.as_byte_slice())
                .unwrap()
                .is_none()
        );
        assert!(
            nodes::lookup(
                &StateNodesV2(&b.state),
                root(&b.state, LITERALS, None).unwrap(),
                id.as_byte_slice()
            )
            .unwrap()
            .is_none()
        );
        let mut key = [0; 65];
        key[0] = b'B';
        key[1..].copy_from_slice(id.as_byte_slice());
        assert_eq!(b.entries.iter().filter(|e|matches!(e,PalwDeltaEntryV2::KernelRouteRow{table,key:k,old:None,new:Some(_)}if *table==PALW_CLAIM_LOCAL_TABLE_V1&&k.as_slice()==key)).count(),1,"equal bodies are physically published once");
    }
}
