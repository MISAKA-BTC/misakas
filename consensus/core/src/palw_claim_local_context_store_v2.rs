//! Journal-backed immutable scoped-context index. Not a source authority by
//! itself: the owner archive writer must bind the actual executed point/rules.
//! This adapter never turns a projected context into a full global state root.
use super::*;
use crate::palw_claim_local_context_nodes_v2::{
    self as nodes, ComponentRootV2, ComponentSpecV2, ComponentUpdateV2, ContextErrorV2, ContextNodeReaderV2, ContextNodeV2,
};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1};
use borsh::{BorshDeserialize, BorshSerialize};

// Distinct versioned namespace. The root/schema dispatcher must explicitly
// register these before production invocation; old F1 keys/bytes never change.
pub(super) const CONTEXT_NODE_PREFIX_V2: u8 = b'N';
pub(super) const CONTEXT_BODY_PREFIX_V2: u8 = b'B';
pub(super) const CONTEXT_CURRENT_PREFIX_V2: u8 = b'I';
fn content_key(prefix: u8, id: Hash64) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = prefix;
    key[1..].copy_from_slice(id.as_byte_slice());
    key
}
fn bad(why: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(why)
}
fn map_state(e: PalwStateV2Error) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(e) => ContextErrorV2::Resource(e),
        _ => bad("scoped context journal refused"),
    }
}
fn decode<T: BorshDeserialize + crate::palw_native_resource_v1::NativeWireScanV1>(
    bytes: &[u8],
) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(bytes, || borsh::from_slice::<T>(bytes)).map_err(|e| {
        match e {
            NativeDecodeErrorV1::Resource(e) => ContextErrorV2::Resource(e),
            NativeDecodeErrorV1::Decode(_) => bad("scoped context row codec"),
        }
    })
}
/// Scoped roots do not overwrite another subject's current index. Source
/// owners derive scope from actual namespaced subject/birth. No authority is
/// granted merely because a caller names an index scope hash.
fn owned_current_key(scope: Hash64, spec: ComponentSpecV2) -> Result<[u8; 84], ContextErrorV2> {
    let mut key = [0u8; 84];
    key[0] = CONTEXT_CURRENT_PREFIX_V2;
    key[1..3].copy_from_slice(&2u16.to_le_bytes());
    key[3..67].copy_from_slice(scope.as_byte_slice());
    spec.serialize(&mut &mut key[67..]).map_err(|_| bad("owned current key"))?;
    Ok(key)
}
fn current_key_spec_offset(key: &[u8]) -> Result<usize, ContextErrorV2> {
    if key.first() != Some(&CONTEXT_CURRENT_PREFIX_V2) {
        return Err(bad("current key prefix"));
    }
    match key.len() {
        18 => Ok(1),
        84 if key[1..3] == 2u16.to_le_bytes() => Ok(67),
        _ => Err(bad("current key version/width")),
    }
}
fn check_current_key(key: &[u8], root: ComponentRootV2) -> Result<(), ContextErrorV2> {
    let at = current_key_spec_offset(key)?;
    let mut expected = [0u8; 17];
    root.spec.serialize(&mut &mut expected[..]).map_err(|_| bad("current component codec"))?;
    if key[at..] != expected {
        return Err(bad("current component key/spec mismatch"));
    }
    Ok(())
}
pub(super) struct StateNodesV2<'a>(pub &'a PalwChainStateV2);
impl StateNodesV2<'_> {
    fn row(&self, key: &[u8]) -> Result<Option<&[u8]>, ContextErrorV2> {
        let Some(route) = self.0.kernel_route.as_ref() else { return Ok(None) };
        let _key = reserve_native_requests_v1(key.len() as u64).map_err(ContextErrorV2::Resource)?;
        Ok(route.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).map(Vec::as_slice))
    }
    /// Body ownership is kept while the typed caller uses or copies it. Missing
    /// committed storage is Unavailable, never an authenticated absent value.
    pub(super) fn body(&self, value: nodes::RowValueV2) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
        let raw = self
            .row(&content_key(CONTEXT_BODY_PREFIX_V2, value.data))?
            .ok_or(ContextErrorV2::Unavailable("scoped context body missing"))?;
        let body = decode::<Vec<u8>>(raw)?;
        if body.len() as u64 != value.bytes || crate::palw_claim_local_v1::palw_liability_material_id_v1(&body) != value.data {
            return Err(bad("scoped context body identity"));
        }
        Ok(body)
    }
    pub(super) fn current(&self, spec: ComponentSpecV2) -> Result<ComponentRootV2, ContextErrorV2> {
        let mut key = [0u8; 18];
        key[0] = CONTEXT_CURRENT_PREFIX_V2;
        spec.serialize(&mut &mut key[1..]).map_err(|_| bad("scoped current component key"))?;
        self.current_at(&key, spec)
    }
    pub(super) fn owned_current(&self, scope: Hash64, spec: ComponentSpecV2) -> Result<ComponentRootV2, ContextErrorV2> {
        self.current_at(&owned_current_key(scope, spec)?, spec)
    }
    fn current_at(&self, key: &[u8], spec: ComponentSpecV2) -> Result<ComponentRootV2, ContextErrorV2> {
        current_key_spec_offset(key)?;
        let Some(raw) = self.row(key)? else { return ComponentRootV2::empty(spec) };
        let root = decode::<ComponentRootV2>(raw)?;
        if root.spec != spec {
            return Err(bad("scoped current component specification"));
        }
        check_current_key(key, *root)?;
        Ok(*root)
    }
}
impl ContextNodeReaderV2 for StateNodesV2<'_> {
    fn node(&self, id: Hash64) -> Result<NativeRetainedV1<ContextNodeV2>, ContextErrorV2> {
        let raw =
            self.row(&content_key(CONTEXT_NODE_PREFIX_V2, id))?.ok_or(ContextErrorV2::Unavailable("scoped context node missing"))?;
        decode(raw)
    }
}

/// One actual component mutation. Bodies, changed path nodes and current root
/// are journalled atomically under the builder's existing allocation owner.
/// No source archive can reference a partially published component generation.
pub(super) fn publish_component_v2(
    b: &mut TransitionBuilder<'_>,
    plan: &ComponentUpdateV2,
    body: Option<&[u8]>,
) -> Result<(), ContextErrorV2> {
    let mut key = [0u8; 18];
    key[0] = CONTEXT_CURRENT_PREFIX_V2;
    plan.before().spec.serialize(&mut &mut key[1..]).map_err(|_| bad("scoped component key"))?;
    publish_at_v2(b, plan, body, &key)
}
pub(super) fn publish_owned_component_v2(
    b: &mut TransitionBuilder<'_>,
    scope: Hash64,
    plan: &ComponentUpdateV2,
    body: Option<&[u8]>,
) -> Result<(), ContextErrorV2> {
    publish_at_v2(b, plan, body, &owned_current_key(scope, plan.before().spec)?)
}
fn publish_at_v2(
    b: &mut TransitionBuilder<'_>,
    plan: &ComponentUpdateV2,
    body: Option<&[u8]>,
    current_key: &[u8],
) -> Result<(), ContextErrorV2> {
    if b.state.kernel_route.is_none() {
        return Err(ContextErrorV2::Unavailable("scoped context requires actual kernel route"));
    }
    if StateNodesV2(&b.state).current_at(current_key, plan.before().spec)? != plan.before() {
        return Err(ContextErrorV2::Stale);
    }
    // All allocations are admitted before encoded key/node/body construction;
    // write_kernel_row_encoded_v1 separately admits retained journal/state rows.
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(65 + current_key.len() as u64).map_err(ContextErrorV2::Resource)?;
    let _keys = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    if let Some(body) = body
        && plan.next_value()
            != Some(nodes::RowValueV2 {
                data: crate::palw_claim_local_v1::palw_liability_material_id_v1(body),
                bytes: body.len() as u64,
            })
    {
        return Err(bad("scoped supplied body differs from actual update"));
    }
    let first = b.entries.len();
    let gc_point = b.claim_local_gc_sequence_v3;
    let apply = (|| {
        if let Some(body) = body {
            let id = crate::palw_claim_local_v1::palw_liability_material_id_v1(body);
            let key = content_key(CONTEXT_BODY_PREFIX_V2, id);
            if let Some(raw) = StateNodesV2(&b.state).row(&key)? {
                // Canonical stream compare: avoids cloning a large body solely
                // to test whether an immutable content version already exists.
                crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&body, raw)
                    .map_err(|_| bad("scoped immutable body collision"))?;
            } else {
                b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&body)).map_err(map_state)?;
            }
        }
        for (id, node) in plan.new_nodes() {
            if nodes::digest(plan.before().spec, node) != *id {
                return Err(bad("scoped plan node identity"));
            }
            let key = content_key(CONTEXT_NODE_PREFIX_V2, *id);
            if let Some(raw) = StateNodesV2(&b.state).row(&key)? {
                crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(node, raw)
                    .map_err(|_| bad("scoped immutable node collision"))?;
            } else {
                b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(node)).map_err(map_state)?;
            }
        }
        // The sealed plan can only come from nodes::update, which verified the
        // changed path and exact old root. Validate new leaf bodies; untouched
        // branches stay shared. Whole-source complete traversal belongs to
        // restore/integrity and actual full-population readers, not every row
        // update (which would multiply unrelated world work).
        for (_, node) in plan.new_nodes() {
            if let ContextNodeV2::Leaf { value, .. } = node {
                StateNodesV2(&b.state).body(*value)?;
            }
        }
        if let Some(value) = plan.next_value() {
            StateNodesV2(&b.state).body(value)?;
        }
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, current_key, Some(&plan.after())).map_err(map_state)?;
        Ok(())
    })();
    if apply.is_err() {
        rollback_component_v2(b, first, gc_point)?
    }
    apply
}

/// Only this exact adapter's aux writes are reversed. New immutable rows are
/// removed; an overwritten current root gets its OLD owned buffers moved back.
/// No world clone, payload clone, fallible allocation or source/credit mutation.
fn rollback_component_v2(b: &mut TransitionBuilder<'_>, first: usize, gc_point: u64) -> Result<(), ContextErrorV2> {
    super::palw_claim_local_context_gc_v3::rollback_aux_v3(b, first, gc_point, |table, key| {
        table == PALW_CLAIM_LOCAL_TABLE_V1 && matches!(key.first().copied(), Some(b'N' | b'B' | b'I'))
    })
}

/// Root references retained by an owner archive. This is only storage liveness,
/// NOT an assertion that its subject computed correctly or earned any credit.
/// The eventual typed source archive embeds this exact reference inventory.
#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct ArchiveReferencesV2 {
    pub version: u16,
    pub components: Vec<ComponentRootV2>,
    pub literals: Vec<nodes::RowValueV2>,
}
pub(super) const CONTEXT_ARCHIVE_PREFIX_V2: u8 = b'A';
crate::palw_native_resource_v1::wire::wire_struct!(ArchiveReferencesV2;u16,Vec<ComponentRootV2>,Vec<nodes::RowValueV2>);

// The A namespace is versioned: v2 is liveness-only, v3 is an actual typed
// paid source. GC reads both without reinterpreting one as the other's codec.
pub(super) enum ArchiveInventoryV2 {
    Storage(NativeRetainedV1<ArchiveReferencesV2>),
    Paid(NativeRetainedV1<super::palw_claim_local_paid_archive_v2::PaidSourceArchiveV3>),
    Financial(NativeRetainedV1<super::palw_claim_local_prefix_financial_archive_v4::PrefixFinancialArchiveV4>),
    ExactClosed(NativeRetainedV1<super::palw_claim_local_exact_closed_source_v5::ExactClosedSourceArchiveV5>),
}
impl ArchiveInventoryV2 {
    pub(super) fn components(&self) -> &[ComponentRootV2] {
        match self {
            Self::Storage(a) => &a.components,
            Self::Paid(a) => &a.components,
            Self::Financial(_) | Self::ExactClosed(_) => &[],
        }
    }
    pub(super) fn literals(&self) -> &[nodes::RowValueV2] {
        match self {
            Self::Storage(a) => &a.literals,
            Self::Paid(a) => std::slice::from_ref(&a.manifest),
            Self::Financial(a) => &a.literals,
            Self::ExactClosed(a) => std::slice::from_ref(&a.body),
        }
    }
}
pub(super) fn archive_inventory(raw: &[u8]) -> Result<ArchiveInventoryV2, ContextErrorV2> {
    match raw.get(..2) {
        Some([2, 0]) => Ok(ArchiveInventoryV2::Storage(decode(raw)?)),
        Some([3, 0]) => Ok(ArchiveInventoryV2::Paid(decode(raw)?)),
        Some([4, 0]) => Ok(ArchiveInventoryV2::Financial(decode(raw)?)),
        Some([5, 0]) => Ok(ArchiveInventoryV2::ExactClosed(decode(raw)?)),
        _ => Err(bad("scoped archive inventory version")),
    }
}

/// Collect only UNREACHABLE content, after the responsible source owners have
/// actually released their archive. Owner-duty/fee/court/reorg expiry is not
/// inferred here: an A row remains a root until its authenticated owner deletes
/// it through that separate finite-duty policy. Current I roots also protect it.
/// This is mark/sweep over actual rooted rows, not a caller-supplied whitelist.
pub(super) fn collect_unreachable_v2(b: &mut TransitionBuilder<'_>) -> Result<usize, ContextErrorV2> {
    super::palw_claim_local_context_gc_v3::collect_v3(b)
}
#[cfg(test)]
pub(super) fn collect_unreachable_reference_v3(b: &mut TransitionBuilder<'_>) -> Result<usize, ContextErrorV2> {
    use crate::palw_native_resource_v1::BTreeNodeEnvelopeV1;
    let mut components = 0u64;
    let mut content = 0u64;
    let mut direct = 0u64;
    let route = b.state.kernel_route.as_ref().ok_or(ContextErrorV2::Unavailable("context collection route missing"))?;
    for ((table, key), raw) in &route.aux {
        if *table != PALW_CLAIM_LOCAL_TABLE_V1 {
            continue;
        }
        match key.first().copied() {
            Some(CONTEXT_NODE_PREFIX_V2 | CONTEXT_BODY_PREFIX_V2) => {
                if key.len() != 65 {
                    return Err(bad("scoped content key width"));
                }
                content = content.checked_add(1).ok_or_else(|| bad("scoped content count overflow"))?;
            }
            Some(CONTEXT_CURRENT_PREFIX_V2) => {
                current_key_spec_offset(key)?;
                components = components.checked_add(1).ok_or_else(|| bad("scoped root count overflow"))?;
            }
            Some(CONTEXT_ARCHIVE_PREFIX_V2) => {
                if key.len() != 65 {
                    return Err(bad("scoped archive key width"));
                }
                let archive = archive_inventory(raw)?;
                components =
                    components.checked_add(archive.components().len() as u64).ok_or_else(|| bad("scoped root count overflow"))?;
                direct = direct.checked_add(archive.literals().len() as u64).ok_or_else(|| bad("scoped literal count overflow"))?;
            }
            _ => {}
        }
    }
    // A unique owner12 record leaf references at most three child roots.
    // Include ALL current/archive roots; repeated immutable nodes are shared.
    let root_capacity = components
        .checked_add(content.checked_mul(3).ok_or_else(|| bad("persistent checkpoint root count overflow"))?)
        .ok_or_else(|| bad("persistent checkpoint roots overflow"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentRootV2>(root_capacity).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<nodes::RowValueV2>(direct).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<(u8, Hash64), ()>>(content).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<[u8; 65]>(content).map_err(ContextErrorV2::Resource)?;
    // Rollback of a deletion may restore a map membership. Its owned old key
    // and bytes move from the admitted journal; only that node can be new.
    q.exact_v1::<BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>(content).map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut roots = Vec::with_capacity(usize::try_from(root_capacity).map_err(|_| bad("scoped root count host width"))?);
    let mut literals = Vec::with_capacity(usize::try_from(direct).map_err(|_| bad("scoped literal count host width"))?);
    for ((table, key), raw) in &route.aux {
        if *table != PALW_CLAIM_LOCAL_TABLE_V1 {
            continue;
        }
        match key.first().copied() {
            Some(CONTEXT_CURRENT_PREFIX_V2) => {
                let root = decode::<ComponentRootV2>(raw)?;
                check_current_key(key, *root)?;
                roots.push(*root);
            }
            Some(CONTEXT_ARCHIVE_PREFIX_V2) => {
                let archive = archive_inventory(raw)?;
                let id = crate::palw_claim_local_v1::palw_liability_material_id_v1(raw);
                if key.as_slice() != content_key(CONTEXT_ARCHIVE_PREFIX_V2, id) {
                    return Err(bad("scoped archive identity"));
                }
                roots.extend_from_slice(archive.components());
                literals.extend_from_slice(archive.literals());
            }
            _ => {}
        }
    }
    let mut live = std::collections::BTreeSet::new();
    let mut root_at = 0usize;
    while root_at < roots.len() {
        let root = roots[root_at];
        root_at += 1;
        nodes::visit_nodes(&StateNodesV2(&b.state), root, |id, node| {
            if live.insert((CONTEXT_NODE_PREFIX_V2, id)) {
                if let ContextNodeV2::Leaf { key, value } = node {
                    StateNodesV2(&b.state).body(*value)?;
                    live.insert((CONTEXT_BODY_PREFIX_V2, value.data));
                    super::palw_claim_local_checkpoint_descriptor_v3::gc_child_roots_v3(&b.state, root, key, *value, |child| {
                        if roots.len() == roots.capacity() {
                            return Err(bad("persistent checkpoint GC root bound exhausted"));
                        }
                        roots.push(child);
                        Ok(())
                    })?;
                }
            }
            Ok(())
        })?;
    }
    for value in literals {
        StateNodesV2(&b.state).body(value)?;
        live.insert((CONTEXT_BODY_PREFIX_V2, value.data));
    }
    let mut dead: Vec<[u8; 65]> = Vec::with_capacity(content as usize);
    for ((table, key), _) in &route.aux {
        if *table == PALW_CLAIM_LOCAL_TABLE_V1 && key.len() == 65 && matches!(key[0], CONTEXT_NODE_PREFIX_V2 | CONTEXT_BODY_PREFIX_V2)
        {
            let id = Hash64::from_bytes(key[1..].try_into().expect("content key"));
            if !live.contains(&(key[0], id)) {
                dead.push(key.as_slice().try_into().expect("content key"));
            }
        }
    }
    let first = b.entries.len();
    let gc_point = b.claim_local_gc_sequence_v3;
    for key in &dead {
        if let Err(error) = b.write_kernel_row_encoded_v1::<Vec<u8>>(PALW_CLAIM_LOCAL_TABLE_V1, key, None) {
            rollback_component_v2(b, first, gc_point)?;
            return Err(map_state(error));
        }
    }
    Ok(dead.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn params() -> PalwStateParamsV2 {
        let h = Hash64::from_u64_word;
        PalwStateParamsV2::new(100, 100, 10, 20, 50, 1000, h(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_claim_local_settlement_from_daa(Some(0))
    }
    fn extras() -> PalwTransitionExtrasV1 {
        let h = Hash64::from_u64_word;
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
    fn prepared(p: &PalwStateParamsV2, e: &PalwTransitionExtrasV1) -> PalwChainStateV2 {
        let point = PalwBlockContextV2 { block: Hash64::from_u64_word(5), daa_score: 1, blue_score: 1, subsidy: 0 };
        apply_palw_transition_v2_with_extras(&PalwChainStateV2::genesis(), p, &point, &[], None, false, false, false, false, e)
            .unwrap()
            .0
    }
    fn spec() -> ComponentSpecV2 {
        ComponentSpecV2 { owner: 1, table: 1, schema: 2, key_bytes: 8, value_bytes: 8 }
    }
    fn value(body: &[u8]) -> nodes::RowValueV2 {
        nodes::RowValueV2 { data: crate::palw_claim_local_v1::palw_liability_material_id_v1(body), bytes: body.len() as u64 }
    }
    fn set(b: &mut TransitionBuilder<'_>, key: &[u8], body: &[u8]) -> ComponentRootV2 {
        let reader = StateNodesV2(&b.state);
        let root = reader.current(spec()).unwrap();
        let old = nodes::lookup(&reader, root, key).unwrap();
        let plan = nodes::update(&reader, root, key, old, Some(value(body))).unwrap();
        publish_component_v2(b, &plan, Some(body)).unwrap();
        plan.after()
    }
    #[test]
    fn rfc23s2_scoped_store_owner_current_roots_are_independent_and_gc_protected() {
        let p = params();
        let e = extras();
        let state = prepared(&p, &e);
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        let a = Hash64::from_u64_word(101);
        let c = Hash64::from_u64_word(102);
        let empty = StateNodesV2(&b.state).owned_current(a, spec()).unwrap();
        let first = nodes::update(&StateNodesV2(&b.state), empty, b"a", None, Some(value(b"owner A"))).unwrap();
        publish_owned_component_v2(&mut b, a, &first, Some(b"owner A")).unwrap();
        let empty = StateNodesV2(&b.state).owned_current(c, spec()).unwrap();
        let second = nodes::update(&StateNodesV2(&b.state), empty, b"a", None, Some(value(b"owner B"))).unwrap();
        publish_owned_component_v2(&mut b, c, &second, Some(b"owner B")).unwrap();
        assert_eq!(StateNodesV2(&b.state).owned_current(a, spec()).unwrap(), first.after());
        assert_eq!(StateNodesV2(&b.state).owned_current(c, spec()).unwrap(), second.after());
        assert_eq!(StateNodesV2(&b.state).current(spec()).unwrap().rows, 0, "owned contexts are not one whole-table root");
        collect_unreachable_v2(&mut b).unwrap();
        assert_eq!(nodes::lookup(&StateNodesV2(&b.state), first.after(), b"a").unwrap(), Some(value(b"owner A")));
        assert_eq!(nodes::lookup(&StateNodesV2(&b.state), second.after(), b"a").unwrap(), Some(value(b"owner B")));
        let mut wrong = owned_current_key(a, spec()).unwrap();
        wrong[1] = 9;
        assert!(StateNodesV2(&b.state).current_at(&wrong, spec()).is_err());
    }
    #[test]
    fn rfc23s2_scoped_store_full_carriage_roundtrip_validates_allocated_namespace() {
        let p = params();
        let e = extras();
        let state = prepared(&p, &e);
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        let body = b"sourcev2";
        assert_eq!(body.len() as u64, spec().value_bytes);
        let root = set(&mut b, b"a", body);
        let committed = value(body);
        assert_eq!(nodes::lookup(&StateNodesV2(&b.state), root, b"a").unwrap(), Some(committed));
        assert_eq!(StateNodesV2(&b.state).body(committed).unwrap().as_slice(), body);
        // An A2 STORAGE liveness inventory, not selected approval or a source
        // authority. A3 genuine selected signatures are exercised on the node.
        let archive = ArchiveReferencesV2 { version: 2, components: vec![root], literals: vec![] };
        let raw = borsh::to_vec(&archive).unwrap();
        let key = content_key(CONTEXT_ARCHIVE_PREFIX_V2, crate::palw_claim_local_v1::palw_liability_material_id_v1(&raw));
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&archive)).unwrap();
        let expected = b.state.state_root();
        let wire = borsh::to_vec(&PalwStateCarriageV2::from_state(&b.state)).unwrap();
        crate::palw_native_resource_v1::preflight_native_carriage_v1(wire.as_slice(), wire.len() as u64).unwrap();
        let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).unwrap();
        let restored = carriage.into_state_v3(&p, Some(expected), false, None).unwrap();
        assert_eq!(restored.state_root(), expected);
        assert_eq!(StateNodesV2(&restored).current(spec()).unwrap(), root);
        assert_eq!(
            nodes::lookup(&StateNodesV2(&restored), root, b"a").unwrap(),
            Some(committed),
            "restore retains the exact source body commitment and byte count"
        );
        assert_eq!(StateNodesV2(&restored).body(committed).unwrap().as_slice(), body);
        let mut missing_body = restored.clone();
        assert!(
            missing_body
                .kernel_route
                .as_mut()
                .unwrap()
                .aux
                .remove(&(PALW_CLAIM_LOCAL_TABLE_V1, content_key(CONTEXT_BODY_PREFIX_V2, committed.data).to_vec()))
                .is_some()
        );
        assert!(
            PalwStateCarriageV2::from_state(&missing_body).into_state_v3(&p, Some(missing_body.state_root()), false, None).is_err(),
            "a retained component root cannot restore without its actual immutable body"
        );
        let mut missing = restored.clone();
        missing
            .kernel_route
            .as_mut()
            .unwrap()
            .aux
            .remove(&(PALW_CLAIM_LOCAL_TABLE_V1, content_key(CONTEXT_NODE_PREFIX_V2, root.node.unwrap()).to_vec()));
        assert!(
            PalwStateCarriageV2::from_state(&missing).into_state_v3(&p, Some(missing.state_root()), false, None).is_err(),
            "allocated namespace still requires its complete node/body closure"
        );
        let mut unknown = restored.clone();
        unknown.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, vec![b'Z'; 65]), vec![1]);
        assert!(
            PalwStateCarriageV2::from_state(&unknown).into_state_v3(&p, Some(unknown.state_root()), false, None).is_err(),
            "recognizing scoped rows does not allocate arbitrary native144 prefixes"
        );
    }
    #[test]
    fn rfc23s2_scoped_store_atomic_missing_body_rollback_and_stale_refusal() {
        let p = params();
        let e = extras();
        let state = prepared(&p, &e);
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        let root = set(&mut b, b"a", b"first");
        let before = b.state.state_root();
        let entries = b.entries.len();
        let gc_before = b.state.claim_local_gc_v3.clone();
        let gc_sequence = b.claim_local_gc_sequence_v3;
        let plan = nodes::update(&StateNodesV2(&b.state), root, b"b", None, Some(value(b"missing"))).unwrap();
        assert!(matches!(publish_component_v2(&mut b, &plan, None), Err(ContextErrorV2::Unavailable(_))));
        assert_eq!(b.state.state_root(), before, "partial immutable node writes must roll back");
        assert_eq!(b.entries.len(), entries);
        assert_eq!(StateNodesV2(&b.state).current(spec()).unwrap(), root);
        assert_eq!(b.state.claim_local_gc_v3, gc_before);
        assert_eq!(b.claim_local_gc_sequence_v3, gc_sequence);
        let stale = nodes::update(&StateNodesV2(&b.state), root, b"b", None, Some(value(b"second"))).unwrap();
        set(&mut b, b"c", b"third");
        let before = b.state.state_root();
        assert!(matches!(publish_component_v2(&mut b, &stale, Some(b"second")), Err(ContextErrorV2::Stale)));
        assert_eq!(b.state.state_root(), before);
    }
    #[test]
    fn rfc23s2_scoped_store_actual_roots_protect_shared_nodes_and_release_old_versions() {
        let p = params();
        let e = extras();
        let state = prepared(&p, &e);
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        let old = set(&mut b, b"a", b"first");
        // This is a storage-root liveness fixture, NOT a fabricated claim,
        // selected approval, positive credit or authentic source archive.
        let archive = ArchiveReferencesV2 { version: 2, components: vec![old], literals: vec![] };
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<u8>(1024).unwrap();
        let _owner = reserve_native_requests_v1(q.bytes_v1()).unwrap();
        let raw = borsh::to_vec(&archive).unwrap();
        let key = content_key(CONTEXT_ARCHIVE_PREFIX_V2, crate::palw_claim_local_v1::palw_liability_material_id_v1(&raw));
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&archive)).unwrap();
        let new = set(&mut b, b"a", b"second");
        set(&mut b, b"b", b"third");
        set(&mut b, b"a", b"fourth");
        collect_unreachable_v2(&mut b).unwrap();
        assert_eq!(nodes::lookup(&StateNodesV2(&b.state), old, b"a").unwrap(), Some(value(b"first")));
        b.write_kernel_row_encoded_v1::<ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &key, None).unwrap();
        assert!(collect_unreachable_v2(&mut b).unwrap() > 0);
        assert!(matches!(nodes::lookup(&StateNodesV2(&b.state), old, b"a"), Err(ContextErrorV2::Unavailable(_))));
        assert_eq!(
            nodes::lookup(&StateNodesV2(&b.state), StateNodesV2(&b.state).current(spec()).unwrap(), b"a").unwrap(),
            Some(value(b"fourth"))
        );
        assert!(
            matches!(nodes::lookup(&StateNodesV2(&b.state), new, b"a"), Err(ContextErrorV2::Unavailable(_))),
            "an unretained intermediate root is reclaimable"
        );
    }
}

/// Restore/import integrity for the distinct scoped namespace. This validates
/// every rooted component/inventory, without constructing a partial chain state.
pub(super) fn validate_context_rows_v2(state: &PalwChainStateV2, params: &PalwStateParamsV2) -> Result<(), ContextErrorV2> {
    let Some(route) = &state.kernel_route else { return Ok(()) };
    let active = state.last_point.is_some_and(|p| params.claim_local_settlement_active_at(p.daa_score));
    let reader = StateNodesV2(state);
    for ((table, key), raw) in &route.aux {
        if *table != PALW_CLAIM_LOCAL_TABLE_V1
            || !matches!(
                key.first().copied(),
                Some(CONTEXT_NODE_PREFIX_V2 | CONTEXT_BODY_PREFIX_V2 | CONTEXT_CURRENT_PREFIX_V2 | CONTEXT_ARCHIVE_PREFIX_V2)
            )
        {
            continue;
        }
        if !active {
            return Err(bad("scoped context rows precede actual settlement fence"));
        }
        match key[0] {
            CONTEXT_CURRENT_PREFIX_V2 => {
                let root = decode::<ComponentRootV2>(raw)?;
                let spec_offset = match key.len() {
                    18 => 1,
                    84 if key[1..3] == [2, 0] => 67,
                    _ => return Err(bad("scoped current key schema")),
                };
                let mut canonical = [0; 17];
                root.spec.serialize(&mut &mut canonical[..]).map_err(|_| bad("scoped current spec codec"))?;
                if key[spec_offset..] != canonical {
                    return Err(bad("scoped current key/spec binding"));
                }
                nodes::visit_nodes(&reader, *root, |_, node| {
                    if let ContextNodeV2::Leaf { value, .. } = node {
                        reader.body(*value)?;
                    }
                    Ok(())
                })?;
            }
            CONTEXT_ARCHIVE_PREFIX_V2 => {
                if key.len() != 65
                    || key.as_slice()
                        != content_key(CONTEXT_ARCHIVE_PREFIX_V2, crate::palw_claim_local_v1::palw_liability_material_id_v1(raw))
                {
                    return Err(bad("scoped archive key/content binding"));
                }
                let archive = archive_inventory(raw)?;
                for root in archive.components() {
                    nodes::visit_nodes(&reader, *root, |_, node| {
                        if let ContextNodeV2::Leaf { value, .. } = node {
                            reader.body(*value)?;
                        }
                        Ok(())
                    })?;
                }
                for literal in archive.literals() {
                    reader.body(*literal)?;
                }
                super::palw_claim_local_checkpoint_descriptor_v3::validate_archived_roots_v3(state, archive.components())?;
                match archive {
                    ArchiveInventoryV2::Paid(paid) => {
                        super::palw_claim_local_paid_archive_v2::verify_archived_source_v3(state, &paid)?
                    }
                    ArchiveInventoryV2::Financial(a) => {
                        super::palw_claim_local_prefix_financial_archive_v4::verify_archive_v4(state, &a)?
                    }
                    ArchiveInventoryV2::ExactClosed(a) => {
                        super::palw_claim_local_exact_closed_source_v5::verify_archive_v5(state, params, &a)?
                    }
                    ArchiveInventoryV2::Storage(_) => {}
                }
            }
            CONTEXT_NODE_PREFIX_V2 | CONTEXT_BODY_PREFIX_V2 => {
                if key.len() != 65 {
                    return Err(bad("scoped immutable key width"));
                }
            }
            _ => unreachable!(),
        }
    }
    super::palw_claim_local_prefix_financial_archive_v4::validate_indices_v4(state)?;
    super::palw_claim_local_exact_closed_source_v5::validate_indices_v5(state, params)?;
    super::palw_claim_local_physical_lock_instances_v6::validate_physical_instances_v6(state)?;
    super::palw_claim_local_material_income_lineage_v7::validate_v7(state)?;
    super::palw_claim_local_source_admission_v8::validate_source_admissions_v8(state)?;
    super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::validate_original_archive_receipts_v1(state)?;
    super::palw_claim_local_born_tir_source_v16::validate_original_tir_bundles_v16(state)?;
    super::palw_claim_local_carrier_lineage_v9::validate_carrier_lineage_v9(state)?;
    super::palw_claim_local_source_contract_generation_v8::validate_v8(state)?;
    super::palw_claim_local_kernel_contract_duties_v9::validate_v9(state)?;
    super::palw_claim_local_kernel_current_birth_v9::validate_current_births_v9(state)?;
    super::palw_claim_local_checkpoint_descriptor_v3::validate(state)?;
    super::palw_claim_local_exec_archive_v10::validate_exec_archive_v10(state)?;
    super::palw_claim_local_execution_cohort_archive_v15::validate_cohort_archives_v15(state).map_err(|error| match error {
        PalwStateV2Error::NativeResource(resource) => ContextErrorV2::Resource(resource),
        _ => bad("owner15 scoped cohort archive validation"),
    })?;
    Ok(())
}

include!("palw_claim_local_context_marginal_physical_v14.rs");
