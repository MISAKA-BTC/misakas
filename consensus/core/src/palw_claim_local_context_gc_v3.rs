//! ISOLATED global UNHASHED cache. No serialized source/schema/fee change.
//! Physical N owns its typed semantic edges once; B never owns profile edges.
//! Unknown nodes are classified only from actual I/A/spec references. Zero
//! rows are freed only at an explicitly permitted GC boundary, never on unpin.
use super::palw_claim_local_context_store_v2 as store;
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, ContextNodeV2};
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, TirNativeRequestsV1,
    reserve_native_requests_v1,
};
use borsh::BorshDeserialize;
type Key = (u8, Hash64);
const N: u8 = b'N';
const B: u8 = b'B';
const I: u8 = b'I';
const A: u8 = b'A';
// Current adopted implicit profiles: D3 at most3 child roots +leaf body;
// all other profiles (incl Kernel13/EXEC14/Carrier9) use explicit A2 pins.
const EDGES: usize = 4;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EdgeV3 {
    key: Key,
    spec: Option<ComponentSpecV2>,
    expected_size: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EntryV3 {
    incoming: u64,
    present: bool,
    spec: Option<ComponentSpecV2>,
    classified: bool,
    activated: bool,
    actual_size: Option<u64>,
    expected_size: Option<u64>,
    edges: [Option<EdgeV3>; EDGES],
}
impl Default for EntryV3 {
    fn default() -> Self {
        Self {
            incoming: 0,
            present: false,
            spec: None,
            classified: false,
            activated: false,
            actual_size: None,
            expected_size: None,
            edges: [None; EDGES],
        }
    }
}
impl NativeHeapV1 for EntryV3 {
    fn native_heap_v1(&self, _: &str) -> NativeCoverageV1 {
        NativeCoverageV1::zero()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ContextGcV3 {
    rows: BTreeMap<Key, EntryV3>,
    zero: BTreeSet<Key>,
    pending: BTreeSet<Key>,
    missing: BTreeSet<Key>,
    ready: bool,
    unknown_nodes: u64,
}
impl Default for ContextGcV3 {
    fn default() -> Self {
        Self {
            rows: BTreeMap::new(),
            zero: BTreeSet::new(),
            pending: BTreeSet::new(),
            missing: BTreeSet::new(),
            ready: true,
            unknown_nodes: 0,
        }
    }
}
crate::palw_native_resource_v1::native_struct!(ContextGcV3;rows,zero,pending,missing,ready,unknown_nodes);
fn bad(s: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(s)
}
fn err(e: ContextErrorV2) -> PalwStateV2Error {
    super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn map_native(e: PalwStateV2Error) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(r) => ContextErrorV2::Resource(r),
        _ => bad("incremental GC native mutation"),
    }
}
fn decode<T: BorshDeserialize + crate::palw_native_resource_v1::NativeWireScanV1>(
    raw: &[u8],
) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("incremental GC typed codec"),
    })
}
fn key_bytes(k: Key) -> [u8; 65] {
    let mut raw = [0; 65];
    raw[0] = k.0;
    raw[1..].copy_from_slice(k.1.as_byte_slice());
    raw
}
fn content_key(raw: &[u8]) -> Result<Key, ContextErrorV2> {
    if raw.len() != 65 || !matches!(raw[0], N | B) {
        return Err(bad("incremental GC content key"));
    }
    Ok((raw[0], Hash64::from_bytes(raw[1..].try_into().map_err(|_| bad("incremental GC key ID"))?)))
}
/// Exact state-index membership bound. Changes include tombstones and queue
/// movements, so reserve every potentially absent map +three queue nodes.
pub(super) fn resident_requests_v3(count: u64) -> Result<u64, NativeCoverageV1> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<BTreeNodeEnvelopeV1<Key, EntryV3>>(count)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Key, ()>>(
        count.checked_mul(3).ok_or_else(|| NativeCoverageV1::overflow::<ContextGcV3>("incremental GC queues"))?,
    )?;
    Ok(q.bytes_v1())
}
#[derive(Clone, Copy)]
struct ChangeV3 {
    key: Key,
    before: Option<EntryV3>,
    after: EntryV3,
}
pub(super) struct GcUndoV3 {
    changes: Vec<ChangeV3>,
    _lease: Option<NativeDemandLeaseV1>,
}
pub(super) struct GcPlanV3 {
    changes: Vec<ChangeV3>,
    _lease: Option<NativeDemandLeaseV1>,
}
impl GcPlanV3 {
    fn new(cap: u64) -> Result<Self, ContextErrorV2> {
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<ChangeV3>(cap).map_err(ContextErrorV2::Resource)?;
        let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        Ok(Self { changes: Vec::with_capacity(usize::try_from(cap).map_err(|_| bad("incremental GC plan capacity"))?), _lease: lease })
    }
    fn edit<'a>(&'a mut self, index: &ContextGcV3, key: Key) -> Result<&'a mut EntryV3, ContextErrorV2> {
        let pos = if let Some(p) = self.changes.iter().position(|p| p.key == key) {
            p
        } else {
            if self.changes.len() == self.changes.capacity() {
                return Err(bad("incremental GC plan bound"));
            }
            let before = index.rows.get(&key).copied();
            let p = self.changes.len();
            self.changes.push(ChangeV3 { key, before, after: before.unwrap_or_default() });
            p
        };
        Ok(&mut self.changes[pos].after)
    }
    fn reference(&mut self, index: &ContextGcV3, edge: EdgeV3, add: bool) -> Result<(), ContextErrorV2> {
        let row = self.edit(index, edge.key)?;
        if add && !row.present {
            return Err(ContextErrorV2::Unavailable("incremental GC new pin target not physically present"));
        }
        if let Some(spec) = edge.spec {
            if edge.key.0 != N || row.spec.is_some_and(|old| old != spec) {
                return Err(bad("incremental GC node profile collision"));
            }
            row.spec = Some(spec)
        }
        if let Some(size) = edge.expected_size {
            if row.expected_size.is_some_and(|n| n != size) || row.actual_size.is_some_and(|n| n != size) {
                return Err(bad("incremental GC size constraint differs"));
            }
            row.expected_size = Some(size)
        }
        row.incoming = if add {
            row.incoming
                .checked_add(1)
                .ok_or_else(|| ContextErrorV2::Resource(NativeCoverageV1::overflow::<ContextGcV3>("incremental GC reference count")))?
        } else {
            row.incoming.checked_sub(1).ok_or(bad("incremental GC reference underflow"))?
        };
        Ok(())
    }
    fn presence(&mut self, index: &ContextGcV3, key: Key, before: bool, after: bool) -> Result<(), ContextErrorV2> {
        let row = self.edit(index, key)?;
        if row.present != before {
            return Err(bad("incremental GC native/index presence differs"));
        }
        let edges = if before && !after { row.edges } else { [None; EDGES] };
        row.present = after;
        if before && !after {
            row.classified = false;
            row.activated = false;
            row.edges = [None; EDGES];
        }
        for edge in edges.into_iter().flatten() {
            self.reference(index, edge, false)?;
        }
        Ok(())
    }
}
fn pins(raw_key: &[u8], raw: &[u8], mut visit: impl FnMut(EdgeV3) -> Result<(), ContextErrorV2>) -> Result<(), ContextErrorV2> {
    match raw_key.first().copied() {
        Some(I) => {
            let root = decode::<ComponentRootV2>(raw)?;
            checked_root(*root)?;
            let at = match raw_key.len() {
                18 => 1,
                84 if raw_key[1..3] == [2, 0] => 67,
                _ => return Err(bad("incremental GC current key")),
            };
            let expected: ComponentSpecV2 = borsh::from_slice(&raw_key[at..]).map_err(|_| bad("incremental GC current profile"))?;
            if root.spec != expected {
                return Err(bad("incremental GC current profile binding"));
            }
            if let Some(id) = root.node {
                visit(EdgeV3 { key: (N, id), spec: Some(root.spec), expected_size: Some(root.rows) })?;
            }
        }
        Some(A) => {
            if raw_key.len() != 65
                || Hash64::from_bytes(raw_key[1..].try_into().map_err(|_| bad("incremental GC archive key"))?)
                    != crate::palw_claim_local_v1::palw_liability_material_id_v1(raw)
            {
                return Err(bad("incremental GC archive identity"));
            }
            let a = store::archive_inventory(raw)?;
            for root in a.components() {
                checked_root(*root)?;
                if let Some(id) = root.node {
                    visit(EdgeV3 { key: (N, id), spec: Some(root.spec), expected_size: Some(root.rows) })?;
                }
            }
            for body in a.literals() {
                visit(EdgeV3 { key: (B, body.data), spec: None, expected_size: Some(body.bytes) })?;
            }
        }
        _ => {}
    }
    Ok(())
}
/// Preparation is read-only and all failure happens before native/derived swap.
pub(super) fn prepare_row_v3(
    state: &PalwChainStateV2,
    table: u8,
    key: &[u8],
    old: Option<&[u8]>,
    new: Option<&[u8]>,
) -> Result<Option<GcPlanV3>, ContextErrorV2> {
    if table != crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1
        || !matches!(key.first().copied(), Some(N | B | I | A))
        || old == new
    {
        return Ok(None);
    }
    if !state.claim_local_gc_v3.ready {
        return Err(ContextErrorV2::Unavailable("incremental GC cache not rebuilt"));
    }
    let mut cap = 5u64;
    for raw in [old, new].into_iter().flatten() {
        pins(key, raw, |_| {
            cap = cap.checked_add(1).ok_or(bad("incremental GC pin count"))?;
            Ok(())
        })?;
    }
    let mut plan = GcPlanV3::new(cap)?;
    match key.first().copied() {
        Some(N | B) => {
            let k = content_key(key)?;
            if old.is_some() && new.is_some() {
                return Err(bad("incremental GC immutable content overwrite"));
            }
            plan.presence(&state.claim_local_gc_v3, k, old.is_some(), new.is_some())?;
        }
        Some(I | A) => {
            if let Some(raw) = old {
                pins(key, raw, |edge| plan.reference(&state.claim_local_gc_v3, edge, false))?;
            }
            if let Some(raw) = new {
                pins(key, raw, |edge| plan.reference(&state.claim_local_gc_v3, edge, true))?;
            }
        }
        _ => unreachable!(),
    }
    Ok(Some(plan))
}
fn install(index: &mut ContextGcV3, key: Key, value: Option<EntryV3>) {
    let old = index.rows.get(&key).copied();
    let old_unknown = old.is_some_and(|r| key.0 == N && r.present && !r.classified);
    let value = value.filter(|r| r.present || r.incoming != 0);
    let new_unknown = value.is_some_and(|r| key.0 == N && r.present && !r.classified);
    index.unknown_nodes = match (old_unknown, new_unknown) {
        (false, true) => index.unknown_nodes.checked_add(1).expect("prepared GC node count"),
        (true, false) => index.unknown_nodes.checked_sub(1).expect("prepared GC node count"),
        _ => index.unknown_nodes,
    };
    if value.is_some_and(|r| r.present && r.incoming == 0) {
        index.zero.insert(key);
    } else {
        index.zero.remove(&key);
    }
    if value.is_some_and(|r| r.present && r.incoming != 0 && !r.classified && (key.0 == B || r.spec.is_some())) {
        index.pending.insert(key);
    } else {
        index.pending.remove(&key);
    }
    if value.is_some_and(|r| !r.present && r.incoming != 0) {
        index.missing.insert(key);
    } else {
        index.missing.remove(&key);
    }
    match value {
        Some(v) => {
            index.rows.insert(key, v);
        }
        None => {
            index.rows.remove(&key);
        }
    }
}
pub(super) fn apply_plan_v3(state: &mut PalwChainStateV2, plan: GcPlanV3) -> GcUndoV3 {
    for change in plan.changes.iter() {
        debug_assert_eq!(state.claim_local_gc_v3.rows.get(&change.key).copied(), change.before);
        install(&mut state.claim_local_gc_v3, change.key, Some(change.after));
    }
    GcUndoV3 { changes: plan.changes, _lease: plan._lease }
}
fn restore_plan(index: &mut ContextGcV3, undo: GcUndoV3) {
    for change in undo.changes.iter().rev() {
        let expected = (change.after.present || change.after.incoming != 0).then_some(change.after);
        debug_assert_eq!(index.rows.get(&change.key).copied(), expected);
        install(index, change.key, change.before);
    }
}
fn actual_raw(state: &PalwChainStateV2, key: Key) -> Result<&[u8], ContextErrorV2> {
    let encoded = key_bytes(key);
    let _key = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
    state
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, encoded.to_vec())))
        .map(Vec::as_slice)
        .ok_or(ContextErrorV2::Unavailable("incremental GC referenced native row missing"))
}
fn checked_body(state: &PalwChainStateV2, id: Hash64) -> Result<&[u8], ContextErrorV2> {
    let raw = actual_raw(state, (B, id))?;
    let n = u32::from_le_bytes(
        raw.get(..4).ok_or(bad("incremental GC B header"))?.try_into().map_err(|_| bad("incremental GC B length"))?,
    ) as usize;
    if raw.len().checked_sub(4) != Some(n) || crate::palw_claim_local_v1::palw_liability_material_id_v1(&raw[4..]) != id {
        return Err(bad("incremental GC B identity/complete EOF"));
    }
    Ok(&raw[4..])
}
fn checked_root(root: ComponentRootV2) -> Result<(), ContextErrorV2> {
    ComponentRootV2::empty(root.spec)?;
    if root.node.is_none() != (root.rows == 0) {
        return Err(bad("incremental GC root empty/count"));
    }
    Ok(())
}
fn node_edges(state: &PalwChainStateV2, id: Hash64, spec: ComponentSpecV2) -> Result<[Option<EdgeV3>; EDGES], ContextErrorV2> {
    let reader = store::StateNodesV2(state);
    let node = nodes::checked_gc_node_v3(&reader, spec, id)?;
    let mut edges = [None; EDGES];
    let mut n = 0usize;
    match &*node {
        ContextNodeV2::Branch { children, .. } => {
            for (side, child) in children.iter().enumerate() {
                let c = nodes::checked_gc_node_v3(&reader, spec, *child)?;
                nodes::checked_gc_child_v3(&node, &c, side)?;
                edges[n] = Some(EdgeV3 { key: (N, *child), spec: Some(spec), expected_size: None });
                n += 1;
            }
        }
        ContextNodeV2::Leaf { key, value } => {
            let raw = checked_body(state, value.data)?;
            if raw.len() as u64 != value.bytes {
                return Err(bad("incremental GC leaf B length"));
            }
            edges[n] = Some(EdgeV3 { key: (B, value.data), spec: None, expected_size: Some(value.bytes) });
            n += 1;
            // The empty component here is a PROFILE argument only; no projected state
            // or partial component is exposed as admission/full-world authority. Its
            // spec was authenticated by the actual N digest above.
            let profile = ComponentRootV2::empty(spec)?;
            super::palw_claim_local_checkpoint_descriptor_v3::gc_child_roots_v3(state, profile, key, *value, |root| {
                checked_root(root)?;
                if let Some(child) = root.node {
                    if n == EDGES {
                        return Err(ContextErrorV2::Resource(NativeCoverageV1::unsupported::<EntryV3>(
                            "incremental GC typed leaf edges",
                            "profile degree not implemented",
                        )));
                    }
                    edges[n] = Some(EdgeV3 { key: (N, child), spec: Some(root.spec), expected_size: Some(root.rows) });
                    n += 1;
                }
                Ok(())
            })?;
        }
    }
    Ok(edges)
}
fn activation(state: &PalwChainStateV2, key: Key) -> Result<GcPlanV3, ContextErrorV2> {
    let current = *state.claim_local_gc_v3.rows.get(&key).ok_or(bad("incremental GC pending entry absent"))?;
    let mut p = GcPlanV3::new(5)?;
    if current.classified || current.activated || !current.present || current.incoming == 0 {
        return Err(bad("incremental GC activation phase"));
    }
    if key.0 == B {
        let bytes = checked_body(state, key.1)?.len() as u64;
        if current.expected_size.is_some_and(|n| n != bytes) {
            return Err(bad("incremental GC direct literal length"));
        }
        let r = p.edit(&state.claim_local_gc_v3, key)?;
        r.actual_size = Some(bytes);
        r.expected_size = Some(bytes);
        r.classified = true;
        return Ok(p);
    }
    let spec = current.spec.ok_or(bad("incremental GC pending node lacks actual spec"))?;
    let edges = node_edges(state, key.1, spec)?;
    let row = p.edit(&state.claim_local_gc_v3, key)?;
    row.edges = edges;
    row.activated = true;
    for edge in edges.into_iter().flatten() {
        p.reference(&state.claim_local_gc_v3, edge, true)?;
    }
    Ok(p)
}
fn completion(state: &PalwChainStateV2, key: Key) -> Result<GcPlanV3, ContextErrorV2> {
    let old = *state.claim_local_gc_v3.rows.get(&key).ok_or(bad("incremental GC completing absent node"))?;
    if key.0 != N || !old.present || !old.activated || old.classified {
        return Err(bad("incremental GC completion phase"));
    }
    let spec = old.spec.ok_or(bad("incremental GC completion profile"))?;
    let node = nodes::checked_gc_node_v3(&store::StateNodesV2(state), spec, key.1)?;
    let rows = match &*node {
        ContextNodeV2::Leaf { .. } => 1,
        ContextNodeV2::Branch { children, .. } => {
            let mut n = 0u64;
            for child in children {
                let c = state.claim_local_gc_v3.rows.get(&(N, *child)).ok_or(bad("incremental GC child cache absent"))?;
                if !c.classified || !c.present {
                    return Err(bad("incremental GC child not complete"));
                }
                n = n
                    .checked_add(c.actual_size.ok_or(bad("incremental GC child count absent"))?)
                    .ok_or(bad("incremental GC subtree count overflow"))?;
            }
            n
        }
    };
    for e in old.edges.into_iter().flatten() {
        let target = state.claim_local_gc_v3.rows.get(&e.key).ok_or(bad("incremental GC typed target absent"))?;
        if !target.present || !target.classified {
            return Err(bad("incremental GC typed target not complete"));
        }
        if e.expected_size.is_some_and(|n| target.actual_size != Some(n)) {
            return Err(bad("incremental GC typed target count/length differs"));
        }
    }
    if old.expected_size.is_some_and(|n| n != rows) {
        return Err(bad("incremental GC rooted row count differs"));
    }
    let mut p = GcPlanV3::new(1)?;
    let row = p.edit(&state.claim_local_gc_v3, key)?;
    row.actual_size = Some(rows);
    row.expected_size = Some(rows);
    row.classified = true;
    Ok(p)
}
#[derive(Clone, Copy)]
struct FrameV3 {
    key: Key,
    finish: bool,
}
fn unfinished_child(index: &ContextGcV3, row: EntryV3) -> Option<Key> {
    row.edges.into_iter().flatten().find(|e| index.rows.get(&e.key).is_some_and(|r| r.present && !r.classified)).map(|e| e.key)
}
fn stack_capacity(spec: Option<ComponentSpecV2>) -> Result<usize, ContextErrorV2> {
    // One DFS frame per ancestor; siblings are visited one at a time.
    // D3 profiles have at most three nested semantic levels, all other adopted
    // profiles use A2 external pins. Physical Patricia depth is key_bytes*9+1.
    let width = spec.map_or(0, |s| usize::from(s.key_bytes)).max(133);
    width
        .checked_mul(9)
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(16))
        .ok_or(bad("incremental GC stack bound"))
}
fn apply_builder_plan(b: &mut TransitionBuilder<'_>, plan: GcPlanV3) -> Result<(), ContextErrorV2> {
    let sequence = admit_builder_plan(b, &plan)?;
    let undo = apply_plan_v3(&mut b.state, plan);
    b.claim_local_gc_sequence_v3 = sequence;
    b.claim_local_gc_undo_v3.insert(sequence, undo);
    Ok(())
}
/// Resolve only newly referenced unknown nodes. Existing classified subtrees
/// are never revisited; subtree counts retain the old complete root checks.
pub(super) fn settle_builder_v3(b: &mut TransitionBuilder<'_>) -> Result<(), ContextErrorV2> {
    while let Some(key) = b.state.claim_local_gc_v3.pending.first().copied() {
        let entry = *b.state.claim_local_gc_v3.rows.get(&key).ok_or(bad("incremental GC pending row absent"))?;
        let capacity = if key.0 == B { 1 } else { stack_capacity(entry.spec)? };
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<FrameV3>(capacity as u64).map_err(ContextErrorV2::Resource)?;
        let _stack_owner = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut stack = Vec::with_capacity(capacity);
        stack.push(FrameV3 { key, finish: false });
        while let Some(frame) = stack.pop() {
            let Some(row) = b.state.claim_local_gc_v3.rows.get(&frame.key).copied() else {
                continue;
            };
            if row.classified || row.incoming == 0 || !row.present {
                continue;
            }
            if frame.finish {
                if let Some(child) = unfinished_child(&b.state.claim_local_gc_v3, row) {
                    if stack.capacity() - stack.len() < 2 {
                        return Err(ContextErrorV2::Resource(NativeCoverageV1::overflow::<FrameV3>("incremental GC DFS stack")));
                    }
                    stack.push(frame);
                    stack.push(FrameV3 { key: child, finish: false });
                    continue;
                }
                let p = completion(&b.state, frame.key)?;
                apply_builder_plan(b, p)?;
                continue;
            }
            if !row.activated {
                let p = activation(&b.state, frame.key)?;
                apply_builder_plan(b, p)?;
            }
            let r = *b.state.claim_local_gc_v3.rows.get(&frame.key).ok_or(bad("incremental GC active row absent"))?;
            if r.classified {
                continue;
            }
            if stack.capacity() - stack.len() < 2 {
                return Err(ContextErrorV2::Resource(NativeCoverageV1::overflow::<FrameV3>("incremental GC DFS stack")));
            }
            stack.push(FrameV3 { key: frame.key, finish: true });
            if let Some(child) = unfinished_child(&b.state.claim_local_gc_v3, r) {
                stack.push(FrameV3 { key: child, finish: false });
            }
        }
    }
    if !b.state.claim_local_gc_v3.missing.is_empty() {
        return Err(ContextErrorV2::Unavailable("incremental GC referenced content physically missing"));
    }
    Ok(())
}
/// Native snapshot/reorg may restore B/N before I/A (or old B-before-N frees).
/// The serialized delta owns physical changes; settle NEVER invents a free.
pub(super) fn settle_state_v3(state: &mut PalwChainStateV2) -> Result<(), ContextErrorV2> {
    if state.kernel_route.is_none() && !state.claim_local_gc_v3.rows.is_empty() {
        return Err(bad("incremental GC cache retained after route drop"));
    }
    while let Some(key) = state.claim_local_gc_v3.pending.first().copied() {
        let entry = *state.claim_local_gc_v3.rows.get(&key).ok_or(bad("incremental GC state pending absent"))?;
        let capacity = if key.0 == B { 1 } else { stack_capacity(entry.spec)? };
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<FrameV3>(capacity as u64).map_err(ContextErrorV2::Resource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut stack = Vec::with_capacity(capacity);
        stack.push(FrameV3 { key, finish: false });
        while let Some(f) = stack.pop() {
            let Some(row) = state.claim_local_gc_v3.rows.get(&f.key).copied() else {
                continue;
            };
            if row.classified || row.incoming == 0 || !row.present {
                continue;
            }
            if f.finish {
                if let Some(child) = unfinished_child(&state.claim_local_gc_v3, row) {
                    if stack.capacity() - stack.len() < 2 {
                        return Err(ContextErrorV2::Resource(NativeCoverageV1::overflow::<FrameV3>(
                            "incremental GC replay DFS stack",
                        )));
                    }
                    stack.push(f);
                    stack.push(FrameV3 { key: child, finish: false });
                    continue;
                }
                let plan = completion(state, f.key)?;
                drop(apply_plan_v3(state, plan));
                continue;
            }
            if !row.activated {
                let plan = activation(state, f.key)?;
                drop(apply_plan_v3(state, plan));
            }
            let row = *state.claim_local_gc_v3.rows.get(&f.key).ok_or(bad("incremental GC state active absent"))?;
            if row.classified {
                continue;
            }
            if stack.capacity() - stack.len() < 2 {
                return Err(ContextErrorV2::Resource(NativeCoverageV1::overflow::<FrameV3>("incremental GC replay DFS stack")));
            }
            stack.push(FrameV3 { key: f.key, finish: true });
            if let Some(child) = unfinished_child(&state.claim_local_gc_v3, row) {
                stack.push(FrameV3 { key: child, finish: false });
            }
        }
    }
    if !state.claim_local_gc_v3.missing.is_empty() {
        return Err(ContextErrorV2::Unavailable("incremental GC replay content missing"));
    }
    Ok(())
}
pub(super) struct PreparedWriterV3 {
    plan: GcPlanV3,
    sequence: u64,
}
fn admit_builder_plan(b: &mut TransitionBuilder<'_>, plan: &GcPlanV3) -> Result<u64, ContextErrorV2> {
    let resident = resident_requests_v3(plan.changes.len() as u64).map_err(ContextErrorV2::Resource)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<BTreeNodeEnvelopeV1<u64, GcUndoV3>>(1).map_err(ContextErrorV2::Resource)?;
    let bound = resident.checked_mul(2).and_then(|n| n.checked_add(q.bytes_v1())).ok_or(bad("incremental GC swap/rollback bound"))?;
    b.native_reservation.grow_v1(bound).map_err(ContextErrorV2::Resource)?;
    b.claim_local_gc_sequence_v3.checked_add(1).ok_or(bad("incremental GC sequence overflow"))
}
pub(super) fn prepare_writer_v3(
    b: &mut TransitionBuilder<'_>,
    table: u8,
    key: &[u8],
    new: &Option<Vec<u8>>,
) -> Result<Option<PreparedWriterV3>, PalwStateV2Error> {
    if table != crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 || !matches!(key.first().copied(), Some(N | B | I | A)) {
        return Ok(None);
    }
    if b.state.kernel_route.is_none() {
        return Ok(None);
    }
    let lease = reserve_native_requests_v1(key.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let plan = {
        let old = b.state.kernel_route.as_ref().and_then(|r| r.aux.get(&(table, key.to_vec()))).map(Vec::as_slice);
        prepare_row_v3(&b.state, table, key, old, new.as_deref()).map_err(err)?
    };
    drop(lease);
    let Some(plan) = plan else { return Ok(None) };
    let sequence = admit_builder_plan(b, &plan).map_err(err)?;
    Ok(Some(PreparedWriterV3 { plan, sequence }))
}
/// The central writer calls this only AFTER the actual row and native journal
/// have changed. All admission/codec/count checks happened in prepare_writer.
pub(super) fn commit_writer_v3(b: &mut TransitionBuilder<'_>, prepared: PreparedWriterV3) {
    debug_assert_eq!(b.claim_local_gc_sequence_v3.checked_add(1), Some(prepared.sequence));
    let undo = apply_plan_v3(&mut b.state, prepared.plan);
    b.claim_local_gc_sequence_v3 = prepared.sequence;
    b.claim_local_gc_undo_v3.insert(prepared.sequence, undo);
}
pub(super) fn checkpoint_v3(b: &TransitionBuilder<'_>) -> (u64, bool) {
    (b.claim_local_gc_sequence_v3, b.claim_local_gc_defer_v3)
}
pub(super) fn restore_observations_v3(b: &mut TransitionBuilder<'_>, point: (u64, bool)) {
    // A full builder checkpoint restored the entire derived state already. Drop
    // only newer inverse records; do not apply them a second time.
    while b.claim_local_gc_undo_v3.last_key_value().is_some_and(|(k, _)| *k > point.0) {
        b.claim_local_gc_undo_v3.pop_last();
    }
    b.claim_local_gc_sequence_v3 = point.0;
    b.claim_local_gc_defer_v3 = point.1;
}
fn undo_to(b: &mut TransitionBuilder<'_>, point: u64) {
    while b.claim_local_gc_undo_v3.last_key_value().is_some_and(|(k, _)| *k > point) {
        let (_, undo) = b.claim_local_gc_undo_v3.pop_last().expect("GC inverse stack checked");
        restore_plan(&mut b.state.claim_local_gc_v3, undo);
    }
    b.claim_local_gc_sequence_v3 = point;
}
/// Shared rollback for the caller's EXACT authorized aux namespace. N/B/I/A
/// update the derived cache; released D/C/V/Q/M are safely untracked. No new
/// decode, world copy, native admission or allocation buffers on refusal.
pub(super) fn rollback_aux_v3(
    b: &mut TransitionBuilder<'_>,
    first: usize,
    gc_point: u64,
    allowed: impl Fn(u8, &[u8]) -> bool,
) -> Result<(), ContextErrorV2> {
    for entry in &b.entries[first..] {
        let PalwDeltaEntryV2::KernelRouteRow { table, key, .. } = entry else {
            return Err(bad("GC aux rollback crossed native owner"));
        };
        if !allowed(*table, key) {
            return Err(bad("GC aux rollback crossed namespace"));
        }
    }
    super::palw_claim_local_execution_cohort_census_v15::rollback_compact_v15(b, first)?;
    super::palw_claim_local_legacy_terminal_v18::rollback_cert_relay_seals_v18(b, first);
    undo_to(b, gc_point);
    while b.entries.len() > first {
        let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = b.entries.pop().expect("GC aux rollback journal checked")
        else {
            unreachable!()
        };
        let route = b.state.kernel_route.as_mut().ok_or(ContextErrorV2::Unavailable("GC aux rollback lost route"))?;
        let k = (table, key);
        if route.aux.get(&k) != new.as_ref() {
            return Err(bad("GC aux rollback native row mismatch"));
        }
        match old {
            Some(raw) => {
                route.aux.insert(k, raw);
            }
            None => {
                route.aux.remove(&k);
            }
        }
    }
    Ok(())
}
/// A request inside a full block only defers. The actual outer boundary runs
/// after all sources had a chance to archive temporary roots in this fold.
pub(super) fn collect_v3(b: &mut TransitionBuilder<'_>) -> Result<usize, ContextErrorV2> {
    if b.state.kernel_route.is_none() {
        return Err(ContextErrorV2::Unavailable("context collection route missing"));
    }
    if b.claim_local_gc_defer_v3 {
        return Ok(0);
    }
    let first = b.entries.len();
    let point = b.claim_local_gc_sequence_v3;
    let result = (|| {
        settle_builder_v3(b)?;
        let mut freed = 0usize;
        while let Some(key) = b.state.claim_local_gc_v3.zero.first().copied() {
            let row = *b.state.claim_local_gc_v3.rows.get(&key).ok_or(bad("GC zero entry absent"))?;
            if !row.present || row.incoming != 0 {
                return Err(bad("GC zero queue stale"));
            }
            let raw = key_bytes(key);
            b.write_kernel_row_encoded_v1::<Vec<u8>>(crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, &raw, None)
                .map_err(map_native)?;
            freed = freed.checked_add(1).ok_or(bad("GC deletion count overflow"))?;
        }
        if !b.state.claim_local_gc_v3.pending.is_empty() || !b.state.claim_local_gc_v3.missing.is_empty() {
            return Err(bad("GC final queue closure"));
        }
        Ok(freed)
    })();
    if result.is_err() {
        rollback_aux_v3(b, first, point, |t, k| {
            t == crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 && matches!(k.first().copied(), Some(N | B))
        })?;
    }
    result
}
pub(super) fn finish_fold_v3(b: &mut TransitionBuilder<'_>) -> Result<usize, PalwStateV2Error> {
    b.claim_local_gc_defer_v3 = false;
    if b.state.kernel_route.is_none() {
        return Ok(0);
    }
    collect_v3(b).map_err(err)
}
/// Cold restore is the ONLY complete root inventory census. Ordinary native
/// writers/delta replay update counts from their exact old/new native rows.
pub(super) fn rebuild_cold_v3(state: &mut PalwChainStateV2) -> Result<(), ContextErrorV2> {
    let mut count = 0u64;
    let mut key_bytes_total = 0u64;
    if let Some(route) = &state.kernel_route {
        for (table, key) in route.aux.keys() {
            if *table == crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 && matches!(key.first().copied(), Some(N | B | I | A)) {
                count = count.checked_add(1).ok_or(bad("GC cold row count"))?;
                key_bytes_total = key_bytes_total.checked_add(key.len() as u64).ok_or(bad("GC cold key bytes"))?;
            }
        }
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Vec<u8>>(count).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<u8>(key_bytes_total).map_err(ContextErrorV2::Resource)?;
    let _keys_owner = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut keys = Vec::with_capacity(usize::try_from(count).map_err(|_| bad("GC cold row host count"))?);
    if let Some(route) = &state.kernel_route {
        for (table, key) in route.aux.keys() {
            if *table == crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 && matches!(key.first().copied(), Some(N | B | I | A)) {
                keys.push(key.clone());
            }
        }
    }
    state.claim_local_gc_v3 = ContextGcV3::default();
    // All physical presence exists before root pins; no missing is silently free.
    for content in [true, false] {
        for key in &keys {
            if matches!(key.first().copied(), Some(N | B)) != content {
                continue;
            }
            let _key = reserve_native_requests_v1(key.len() as u64).map_err(ContextErrorV2::Resource)?;
            let plan = {
                let raw = state
                    .kernel_route
                    .as_ref()
                    .and_then(|r| r.aux.get(&(crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, key.clone())))
                    .ok_or(bad("GC cold row disappeared"))?;
                prepare_row_v3(state, crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, key, None, Some(raw))?
            };
            if let Some(plan) = plan {
                drop(apply_plan_v3(state, plan));
            }
        }
    }
    settle_state_v3(state)?;
    Ok(())
}
/// Constant-time existing unknown population is a safe replay activation bound
/// even when an old I-only delta adopts an already-present unclassified DAG.
pub(super) fn unknown_nodes_v3(state: &PalwChainStateV2) -> u64 {
    state.claim_local_gc_v3.unknown_nodes
}
pub(crate) const fn resident_unit_v3() -> usize {
    std::mem::size_of::<BTreeNodeEnvelopeV1<Key, EntryV3>>() + 3 * std::mem::size_of::<BTreeNodeEnvelopeV1<Key, ()>>()
}
pub(crate) fn fixed_scratch_v3() -> Result<u64, NativeCoverageV1> {
    let depth = (nodes::CONTEXT_MAX_KEY_BYTES_V2 as u64)
        .checked_mul(9)
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(16))
        .ok_or_else(|| NativeCoverageV1::overflow::<ContextGcV3>("GC fixed stack"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<FrameV3>(depth)?;
    q.exact_v1::<ChangeV3>(7)?;
    q.exact_v1::<u8>(65 * 3)?;
    Ok(q.bytes_v1())
}
fn archive_pin_bound(raw: &[u8]) -> Result<u64, NativeCoverageV1> {
    let mut frame = crate::palw_native_resource_v1::NativeWireReaderV1::new(raw, raw.len() as u64);
    let shape = super::palw_scan_scoped_archive_wire_v2(&mut frame)
        .map_err(|_| NativeCoverageV1::unsupported::<ContextGcV3>("GC archive replay", "typed archive codec refused"))?;
    if frame.remaining() != 0 {
        return Err(NativeCoverageV1::unsupported::<ContextGcV3>("GC archive replay", "trailing archive bytes"));
    }
    let direct = match raw.get(..2) {
        Some([2, 0]) => 0,
        Some([3, 0]) | Some([5, 0]) => 1,
        Some([4, 0]) => 2,
        _ => {
            return Err(NativeCoverageV1::unsupported::<ContextGcV3>("GC archive replay", "unknown archive version"));
        }
    };
    shape.collection_entries.checked_add(direct).ok_or_else(|| NativeCoverageV1::overflow::<ContextGcV3>("GC archive pin count"))
}
/// Retained-delta lifetime coverage. No bytes-to-native proxy: fixed content
/// records, actual I/A reference counts, and cached unknown-node population.
pub(crate) fn replay_requests_v3(state: &PalwChainStateV2, delta: &PalwStateDeltaV2) -> Result<u64, NativeCoverageV1> {
    let mut affected = state
        .claim_local_gc_v3
        .unknown_nodes
        .checked_mul(5)
        .ok_or_else(|| NativeCoverageV1::overflow::<ContextGcV3>("GC unknown replay population"))?;
    let mut tracked = affected != 0;
    let mut archive_plan_members = 0u64;
    for entry in &delta.entries {
        if let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry {
            if *table != crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 {
                continue;
            }
            for raw in [old, new].into_iter().flatten() {
                let n = match key.first().copied() {
                    Some(N | B) => 5,
                    Some(I) => 1,
                    Some(A) => {
                        let n = archive_pin_bound(raw)?;
                        archive_plan_members = archive_plan_members
                            .checked_add(n)
                            .ok_or_else(|| NativeCoverageV1::overflow::<ContextGcV3>("GC archive plan members"))?;
                        n
                    }
                    _ => continue,
                };
                affected = affected.checked_add(n).ok_or_else(|| NativeCoverageV1::overflow::<ContextGcV3>("GC delta population"))?;
                tracked = true;
            }
        }
    }
    let mut temporary = TirNativeRequestsV1::new();
    temporary.exact_v1::<ChangeV3>(archive_plan_members)?;
    let requests = resident_requests_v3(affected)?
        .checked_add(temporary.bytes_v1())
        .ok_or_else(|| NativeCoverageV1::overflow::<ContextGcV3>("GC replay variable plan demand"))?;
    if tracked {
        requests.checked_add(fixed_scratch_v3()?).ok_or_else(|| NativeCoverageV1::overflow::<ContextGcV3>("GC replay requests"))
    } else {
        Ok(requests)
    }
}
pub(super) fn restore_cache_to_v3(b: &mut TransitionBuilder<'_>, point: u64) {
    undo_to(b, point)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 100, 10, 20, 50, 1000, Hash64::from_u64_word(1), 4, 1000, 100, 1000, 0)
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
        let ctx = PalwBlockContextV2 { block: Hash64::from_u64_word(5), daa_score: 1, blue_score: 1, subsidy: 0 };
        apply_palw_transition_v2_with_extras(&PalwChainStateV2::genesis(), p, &ctx, &[], None, false, false, false, false, e)
            .unwrap()
            .0
    }
    fn spec() -> ComponentSpecV2 {
        ComponentSpecV2 { owner: 1, table: 1, schema: 2, key_bytes: 8, value_bytes: 64 }
    }
    fn set(b: &mut TransitionBuilder<'_>, raw: &[u8]) -> ComponentRootV2 {
        let reader = store::StateNodesV2(&b.state);
        let root = reader.current(spec()).unwrap();
        let old = nodes::lookup(&reader, root, b"a").unwrap();
        let v = nodes::RowValueV2 { data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw), bytes: raw.len() as u64 };
        let plan = nodes::update(&reader, root, b"a", old, Some(v)).unwrap();
        store::publish_component_v2(b, &plan, Some(raw)).unwrap();
        plan.after()
    }
    #[test]
    fn rfc23s2_incremental_gc_archive_after_unpin_matches_mark_sweep_and_legacy_delta_order() {
        // Pure storage roots, not fabricated source admission, approval or credit.
        let p = params();
        let e = extras();
        let state = prepared(&p, &e);
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        b.claim_local_gc_defer_v3 = true;
        let old = set(&mut b, b"archived old");
        settle_builder_v3(&mut b).unwrap();
        let middle = set(&mut b, b"unarchived middle");
        settle_builder_v3(&mut b).unwrap();
        let current = set(&mut b, b"current");
        assert_eq!(store::collect_unreachable_v2(&mut b).unwrap(), 0, "root unpin never frees before archive publication boundary");
        let a = store::ArchiveReferencesV2 { version: 2, components: vec![old], literals: vec![] };
        let raw = borsh::to_vec(&a).unwrap();
        let mut key = [0; 65];
        key[0] = A;
        key[1..].copy_from_slice(crate::palw_claim_local_v1::palw_liability_material_id_v1(&raw).as_byte_slice());
        b.write_kernel_row_encoded_v1(crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&a)).unwrap();
        let before_gc = b.state.clone();
        let mut oracle = TransitionBuilder::new(&before_gc, &p, false, false, false, false, &e);
        let deleted = store::collect_unreachable_reference_v3(&mut oracle).unwrap();
        assert!(deleted > 0);
        settle_state_v3(&mut oracle.state).unwrap();
        assert!(finish_fold_v3(&mut b).unwrap() > 0);
        assert_eq!(
            b.state.kernel_route().unwrap().aux,
            oracle.state.kernel_route().unwrap().aux,
            "incremental live N/B/I/A closure equals complete mark/sweep"
        );
        assert_eq!(b.state.claim_local_gc_v3, oracle.state.claim_local_gc_v3);
        assert!(nodes::lookup(&store::StateNodesV2(&b.state), old, b"a").unwrap().is_some());
        assert!(nodes::lookup(&store::StateNodesV2(&b.state), current, b"a").unwrap().is_some());
        assert!(nodes::lookup(&store::StateNodesV2(&b.state), middle, b"a").is_err());
        let first_b = oracle
            .entries
            .iter()
            .position(
                |e| matches!(e,PalwDeltaEntryV2::KernelRouteRow{table,key,old:Some(_),new:None}if *table==144&&key.first()==Some(&B)),
            )
            .unwrap();
        let first_n = oracle
            .entries
            .iter()
            .position(
                |e| matches!(e,PalwDeltaEntryV2::KernelRouteRow{table,key,old:Some(_),new:None}if *table==144&&key.first()==Some(&N)),
            )
            .unwrap();
        assert!(first_b < first_n, "exercise legacy B-before-N deletion order");
        let delta = PalwStateDeltaV2 { point: *before_gc.last_point().unwrap(), entries: oracle.entries.clone() };
        let replayed = apply_delta_v2(&before_gc, &delta, &p).unwrap();
        assert_eq!(replayed.state_root(), oracle.state.state_root());
        assert_eq!(replayed.claim_local_gc_v3, oracle.state.claim_local_gc_v3);
        let reverted = revert_delta_v2(&replayed, &delta, &p).unwrap();
        assert_eq!(reverted.state_root(), before_gc.state_root());
        assert_eq!(
            reverted.kernel_route().unwrap().aux,
            before_gc.kernel_route().unwrap().aux,
            "native undo restores all physical versions before next GC"
        );
        // Cold rebuild can classify only actually rooted nodes; an intentionally
        // unrooted intermediate node is safely unknown until a real pin arrives.
        let mut cold = b.state.clone();
        rebuild_cold_v3(&mut cold).unwrap();
        assert_eq!(cold.claim_local_gc_v3, b.state.claim_local_gc_v3);
    }
    #[test]
    fn rfc23s2_incremental_gc_native_capacity_refusal_restores_mid_classification_exactly() {
        let p = params();
        let e = extras();
        let state = prepared(&p, &e);
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        set(&mut b, b"quota-tracked-body");
        let previous = b.state.state_root();
        let cache = b.state.claim_local_gc_v3.clone();
        let entries = b.entries.len();
        let sequence = b.claim_local_gc_sequence_v3;
        // Real local capacity permits the first N activation (N+B cache entries),
        // then refuses its body-classification admission. No test-only verdict hook.
        let first = resident_requests_v3(2).unwrap() * 2 + std::mem::size_of::<BTreeNodeEnvelopeV1<u64, GcUndoV3>>() as u64;
        let checker = crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
            crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 { memory_bytes: first, database_bytes: 0 },
        );
        b.native_reservation = crate::palw_native_resource_v1::NativeTransitionReservationV1::for_checker_v1(&checker);
        assert!(matches!(collect_v3(&mut b), Err(ContextErrorV2::Resource(_))));
        assert_eq!(b.state.state_root(), previous);
        assert_eq!(b.state.claim_local_gc_v3, cache);
        assert_eq!(b.entries.len(), entries);
        assert_eq!(b.claim_local_gc_sequence_v3, sequence);
        assert_eq!(checker.outstanding().0, first, "first actual cache mutation was admitted before later refusal");
        drop(b);
        assert_eq!(checker.outstanding(), (0, 0));
    }
}
pub(crate) const fn change_unit_v3() -> usize {
    std::mem::size_of::<ChangeV3>()
}
