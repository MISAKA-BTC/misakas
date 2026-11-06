//! ISOLATED carrier→exact-born source provenance. Old table167 bytes unchanged.
//! Fixed native summaries survive E.H; no F1/carriage copied into a source.
use super::palw_claim_local_compact_sources_v1::PalwClosedClaimSourceV1 as Closed;
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_resource_v1::{NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1};
use borsh::{BorshDeserialize, BorshSerialize};
const SUMMARY: u32 = 1;
const REVERSE: u32 = 2;
const CONSUMER: u32 = 3;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 9, table, schema: 9, key_bytes: 64, value_bytes: u64::MAX }
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
        _ => bad("carrier lineage journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("carrier lineage encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out).map_err(|_| bad("carrier lineage encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("carrier lineage typed body codec"),
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
    let mut w = Writer(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/carrier-lineage/v9").to_state());
    domain.serialize(&mut w).map_err(|_| bad("carrier lineage domain"))?;
    v.serialize(&mut w).map_err(|_| bad("carrier lineage identity"))?;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
struct BirthV9 {
    block: Hash64,
    daa: u64,
    blue: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(BirthV9;Hash64,u64,u64);
impl BirthV9 {
    fn stored(c: &PalwClaimStateV2) -> Self {
        Self { block: c.accepted_block, daa: c.accepted_daa, blue: c.accepted_blue_score }
    }
    fn point(p: PalwBlockContextV2) -> Self {
        Self { block: p.block, daa: p.daa_score, blue: p.blue_score }
    }
}
/// Unhashed rebuilt coordinate, not a claim/status authority by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub(crate) struct LegacyCarrierLocatorV9 {
    kind: u8,
    source: Hash64,
    birth: BirthV9,
    frame: Hash64,
    claim_ordinal: u64,
    winner_ordinal: u64,
    body: Hash64,
    claim_start: u64,
    claim_len: u64,
    winner_start: u64,
    catalogue: Hash64,
}
crate::palw_native_resource_v1::wire::wire_struct!(LegacyCarrierLocatorV9;u8,Hash64,BirthV9,Hash64,u64,u64,Hash64,u64,u64,u64,Hash64);
impl crate::palw_native_resource_v1::NativeHeapV1 for LegacyCarrierLocatorV9 {
    fn native_heap_v1(&self, _: &str) -> crate::palw_native_resource_v1::NativeCoverageV1 {
        // Exhaustive fixed Copy coordinate; adding a field requires owner review.
        let Self {
            kind: _,
            source: _,
            birth: _,
            frame: _,
            claim_ordinal: _,
            winner_ordinal: _,
            body: _,
            claim_start: _,
            claim_len: _,
            winner_start: _,
            catalogue: _,
        } = self;
        crate::palw_native_resource_v1::NativeCoverageV1::zero()
    }
}
impl LegacyCarrierLocatorV9 {
    fn bound(high: bool) -> Self {
        let h = if high { Hash64::from_bytes([255; 64]) } else { Hash64::default() };
        let n = if high { u64::MAX } else { 0 };
        Self {
            kind: if high { 255 } else { 0 },
            source: h,
            birth: BirthV9 { block: h, daa: n, blue: n },
            frame: h,
            claim_ordinal: n,
            winner_ordinal: n,
            body: h,
            claim_start: n,
            claim_len: n,
            winner_start: n,
            catalogue: h,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) enum CarrierOutcomeV9 {
    Unresolved,
    Denied,
    PermittedClosed,
}
impl NativeWireScanV1 for CarrierOutcomeV9 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        r.step()?;
        if r.byte()? <= 2 {
            Ok(Default::default())
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "carrier summary outcome"))
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct SummaryV9 {
    version: u16,
    id: Hash64,
    txid: Hash64,
    source: Hash64,
    point: PalwBlockContextV2,
    stored: BirthV9,
    class: Hash64,
    bond: PalwBondKeyV2,
    execution: Hash64,
    output: Hash64,
    trace: Hash64,
    original: Option<RowValueV2>,
    closed_proof: Option<RowValueV2>,
    outcome: CarrierOutcomeV9,
    closed: Option<PalwBlockContextV2>,
}
crate::palw_native_resource_v1::wire::wire_struct!(SummaryV9;u16,Hash64,Hash64,Hash64,PalwBlockContextV2,BirthV9,Hash64,PalwBondKeyV2,Hash64,Hash64,Hash64,Option<RowValueV2>,Option<RowValueV2>,CarrierOutcomeV9,Option<PalwBlockContextV2>);
#[derive(BorshSerialize, BorshDeserialize)]
struct ReverseV9 {
    version: u16,
    txid: Hash64,
    owners: Vec<Hash64>,
}
crate::palw_native_resource_v1::wire::wire_struct!(ReverseV9;u16,Hash64,Vec<Hash64>);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) struct CarrierFactV9 {
    consumer: Hash64,
    consumer_point: PalwBlockContextV2,
    consumer_stored: BirthV9,
    spent_txid: Hash64,
    source: Hash64,
    source_point: PalwBlockContextV2,
    summary: Hash64,
}
crate::palw_native_resource_v1::wire::wire_struct!(CarrierFactV9;Hash64,PalwBlockContextV2,BirthV9,Hash64,Hash64,PalwBlockContextV2,Hash64);
#[derive(BorshSerialize, BorshDeserialize)]
struct FactsV9 {
    version: u16,
    consumer: Hash64,
    point: PalwBlockContextV2,
    stored: BirthV9,
    source_manifest: RowValueV2,
    facts: Vec<CarrierFactV9>,
}
crate::palw_native_resource_v1::wire::wire_struct!(FactsV9;u16,Hash64,PalwBlockContextV2,BirthV9,RowValueV2,Vec<CarrierFactV9>);
fn summary_id(txid: Hash64, source: Hash64, p: PalwBlockContextV2) -> Result<Hash64, ContextErrorV2> {
    hash(1, &(txid, source, BirthV9::point(p)))
}
fn consumer_key(source: Hash64, p: PalwBlockContextV2) -> Result<Hash64, ContextErrorV2> {
    hash(2, &(source, BirthV9::point(p)))
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn body<T: BorshDeserialize + NativeWireScanV1>(s: &PalwChainStateV2, v: RowValueV2) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    let r = StateNodesV2(s).body(v)?;
    decode(&r)
}
fn summary(s: &PalwChainStateV2, id: Hash64) -> Result<Option<NativeRetainedV1<SummaryV9>>, ContextErrorV2> {
    let r = lookup::<SummaryV9>(s, SUMMARY, None, id)?;
    if let Some(r) = &r
        && (r.version != 9
            || r.id != id
            || id != summary_id(r.txid, r.source, r.point)?
            || r.stored != BirthV9::point(r.point)
            || match r.outcome {
                CarrierOutcomeV9::Unresolved => r.original.is_none() || r.closed.is_some(),
                _ => r.original.is_some() || r.closed_proof.is_some() || r.closed.is_none(),
            })
    {
        return Err(bad("carrier exact summary identity/status"));
    }
    Ok(r)
}
fn literal<T: BorshSerialize>(b: &mut TransitionBuilder<'_>, v: &T) -> Result<RowValueV2, ContextErrorV2> {
    let raw = encode(v)?;
    let rv = value(&raw);
    put(b, 4, None, Hash64::default(), Some(v))?;
    put::<T>(b, 4, None, Hash64::default(), None)?;
    Ok(rv)
}
fn inventory(
    row: RowValueV2,
    original: Option<RowValueV2>,
    closed_proof: Option<RowValueV2>,
) -> Result<NativeRetainedV1<store::ArchiveReferencesV2>, ContextErrorV2> {
    let n = 1 + usize::from(original.is_some()) + usize::from(closed_proof.is_some());
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<RowValueV2>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut literals = Vec::with_capacity(n);
    literals.push(row);
    if let Some(o) = original {
        literals.push(o)
    }
    if let Some(o) = closed_proof {
        literals.push(o)
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(store::ArchiveReferencesV2 { version: 2, components: Vec::new(), literals }, lease))
}
fn inventory_id(v: &store::ArchiveReferencesV2) -> Result<Hash64, ContextErrorV2> {
    let raw = encode(v)?;
    Ok(value(&raw).data)
}
fn put_summary(b: &mut TransitionBuilder<'_>, r: &SummaryV9) -> Result<(), ContextErrorV2> {
    let old = summary(&b.state, r.id)?;
    let old_value = {
        let n = StateNodesV2(&b.state);
        nodes::lookup(&n, n.current(spec(SUMMARY))?, r.id.as_byte_slice())?
    };
    let raw = encode(r)?;
    let rv = value(&raw);
    let a = inventory(rv, r.original, r.closed_proof)?;
    let aid = inventory_id(&a)?;
    put(b, SUMMARY, None, r.id, Some(r))?;
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &akey(aid), Some(&*a)).map_err(native_err)?;
    if let (Some(old), Some(old_value)) = (old, old_value) {
        let a = inventory(old_value, old.original, old.closed_proof)?;
        let old_id = inventory_id(&a)?;
        if old_id != aid {
            b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(old_id), None)
                .map_err(native_err)?;
        }
    }
    Ok(())
}
fn add_reverse(b: &mut TransitionBuilder<'_>, txid: Hash64, id: Hash64) -> Result<(), ContextErrorV2> {
    let old = lookup::<ReverseV9>(&b.state, REVERSE, None, txid)?;
    if let Some(old) = &old {
        if old.version != 9 || old.txid != txid {
            return Err(bad("carrier reverse owner identity"));
        }
        if old.owners.contains(&id) {
            return Ok(());
        }
    }
    let n = old.as_ref().map_or(0, |r| r.owners.len());
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Hash64>((n + 1) as u64).map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut owners = Vec::with_capacity(n + 1);
    if let Some(old) = old {
        owners.extend_from_slice(&old.owners)
    }
    owners.push(id);
    owners.sort();
    owners.dedup();
    put(b, REVERSE, None, txid, Some(&ReverseV9 { version: 9, txid, owners }))?;
    Ok(())
}
fn mint_from_original(
    b: &mut TransitionBuilder<'_>,
    txid: Hash64,
    id: Hash64,
    p: PalwBlockContextV2,
    original: &PalwClaimStateV2,
) -> Result<Hash64, ContextErrorV2> {
    if !matches!(original.source, PalwClaimSourceV2::FreePrompt { .. }) || BirthV9::stored(original) != BirthV9::point(p) {
        return Err(bad("carrier winner original FP birth differs"));
    }
    let key = summary_id(txid, id, p)?;
    if let Some(old) = summary(&b.state, key)? {
        if old.source != id
            || old.point != p
            || old.txid != txid
            || old.execution != original.execution_root
            || old.bond != original.bond
            || old.class != original.class_id
            || old.output != original.output_root
            || old.trace != original.trace_root
        {
            return Err(bad("carrier winner reused exact authority"));
        }
        add_reverse(b, txid, key)?;
        return Ok(key);
    }
    let original = literal(b, original)?;
    let c = body::<PalwClaimStateV2>(&b.state, original)?;
    let row = SummaryV9 {
        version: 9,
        id: key,
        txid,
        source: id,
        point: p,
        stored: BirthV9::stored(&c),
        class: c.class_id,
        bond: c.bond,
        execution: c.execution_root,
        output: c.output_root,
        trace: c.trace_root,
        original: Some(original),
        closed_proof: None,
        outcome: CarrierOutcomeV9::Unresolved,
        closed: None,
    };
    put_summary(b, &row)?;
    add_reverse(b, txid, key)?;
    Ok(key)
}
/// Actual admitted FP winner writer. No source ID/current167 birth guess: find
/// this successful fold's exact None→Some native Claim before recording txid.
pub(super) fn write_actual_winner_v9(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    txid: Hash64,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1;
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        b.write_kernel_row_encoded_v1(PALW_FP_WINNING_CARRIER_TABLE_V1, id.as_byte_slice(), Some(&txid))?;
        b.state.claim_local_legacy_carriers_ready_v9 = false;
        return Ok(());
    }
    let born = b
        .entries
        .iter()
        .rev()
        .find_map(|e| match e {
            PalwDeltaEntryV2::Claim { key, old: None, new: Some(c) } if *key == id => Some(c),
            _ => None,
        })
        .ok_or_else(|| err(bad("actual winning carrier lacks successful native admission")))?;
    if BirthV9::stored(born) != BirthV9::point(*ctx) {
        return Err(err(bad("actual winning carrier accepting point differs")));
    }
    let raw = encode(born).map_err(err)?;
    let original = decode::<PalwClaimStateV2>(&raw).map_err(err)?;
    // Native167 and metadata are all aux; rollback below includes that exact table.
    atomic(b, |b| {
        mint_from_original(b, txid, id, *ctx, &original)?;
        b.write_kernel_row_encoded_v1(PALW_FP_WINNING_CARRIER_TABLE_V1, id.as_byte_slice(), Some(&txid)).map_err(native_err)
    })
    .map_err(err)
}
/// Only existing actual source closure factory may import a legacy A5/S1
/// original. A new current raw-ID row is never consulted as source authority.
pub(super) fn note_actual_closed_source_v9(b: &mut TransitionBuilder<'_>, row: &Closed) -> Result<(), PalwStateV2Error> {
    let Some(txid) = row.winner else {
        return Ok(());
    };
    atomic(b, |b| {
        let id = mint_from_original(b, txid, row.claim, row.birth, &row.original)?;
        attach_closed_proof(b, id, row)?;
        Ok(())
    })
    .map_err(err)
}

fn census_frame(
    state: &PalwChainStateV2,
    block: Hash64,
    mut add: impl FnMut(Hash64, LegacyCarrierLocatorV9) -> Result<(), ContextErrorV2>,
) -> Result<(), PalwStateV2Error> {
    use super::palw_claim_local_native_frame_events_v10 as stream;
    use crate::palw_claim_local_v1::PalwClaimLocalRowOwnerV1 as Owner;
    let n = stream::rooted_native_event_count_v10(state, block)?.ok_or_else(|| err(bad("native carrier frame disappeared")))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<Hash64, (u64, u64, u64, Hash64)>>(n)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _scratch = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut admitted = BTreeMap::new();
    let present = stream::visit_rooted_native_events_v10(state, block, |point, ordinal, event| {
        (|| -> Result<(), ContextErrorV2> {
            if event.owner == Owner::ChainClaims
                && event.old.is_none()
                && let (Some(new), Ok(bytes)) = (event.new, <[u8; 64]>::try_from(event.key.bytes))
            {
                let c = decode::<PalwClaimStateV2>(new.bytes)?;
                if matches!(c.source, PalwClaimSourceV2::FreePrompt { .. }) && BirthV9::stored(&c) == BirthV9::point(point) {
                    admitted.insert(Hash64::from_bytes(bytes), (ordinal, new.offset, new.bytes.len() as u64, value(new.bytes).data));
                }
            }
            if event.owner == Owner::KernelRoute
                && event.table == crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1
                && let (Some(new), Ok(bytes)) = (event.new, <[u8; 64]>::try_from(event.key.bytes))
                && let Ok(tx) = <[u8; 64]>::try_from(new.bytes)
            {
                let source = Hash64::from_bytes(bytes);
                if let Some((claim_ordinal, claim_start, claim_len, body)) = admitted.get(&source).copied() {
                    add(
                        Hash64::from_bytes(tx),
                        LegacyCarrierLocatorV9 {
                            kind: 3,
                            source,
                            birth: BirthV9::point(point),
                            frame: point.block,
                            claim_ordinal,
                            winner_ordinal: ordinal,
                            body,
                            claim_start,
                            claim_len,
                            winner_start: new.offset,
                            catalogue: point.block,
                        },
                    )?;
                }
            }
            Ok(())
        })()
        .map_err(err)
    })?;
    if !present {
        return Err(err(bad("native carrier frame disappeared during census")));
    }
    Ok(())
}
fn locator_for_closed(row: &Closed, kind: u8, body: Hash64) -> LegacyCarrierLocatorV9 {
    LegacyCarrierLocatorV9 {
        kind,
        source: row.claim,
        birth: BirthV9::point(row.birth),
        frame: row.birth.block,
        claim_ordinal: 0,
        winner_ordinal: 0,
        body,
        claim_start: 0,
        claim_len: 0,
        winner_start: 0,
        catalogue: row.claim,
    }
}
fn a5_spec() -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 6, table: 1, schema: 5, key_bytes: 64, value_bytes: u64::MAX }
}
/// Full-state boundary derived-index quote, not a source fee. Actual aux rows
/// bound S1/A5 directories; actual native changes bound F1 winner coordinates.
pub(super) fn legacy_index_requests_v9(
    route: Option<&crate::palw_kernel_route_v1::PalwKernelRouteStateV1>,
) -> Result<u64, PalwStateV2Error> {
    let Some(route) = route else { return Ok(0) };
    let mut count = 0u64;
    let mut leaves = 0u64;
    for ((table, key), raw) in &route.aux {
        if *table == crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1 && key.len() == 64 {
            count = count.checked_add(1).ok_or(PalwStateV2Error::Overflow("carrier hint count"))?;
        }
        if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.len() != 65 {
            continue;
        }
        if key[0] == b'S' {
            count = count.checked_add(1).ok_or(PalwStateV2Error::Overflow("carrier S1 count"))?;
        }
        if key[0] == b'F' {
            count = count
                .checked_add(super::palw_claim_local_native_frame_events_v10::native_event_count_v10(raw)?)
                .ok_or(PalwStateV2Error::Overflow("carrier F1 event count"))?;
        }
        if key[0] == b'N' && raw.first() == Some(&0) {
            leaves = leaves.checked_add(1).ok_or(PalwStateV2Error::Overflow("carrier actual N leaf count"))?;
        }
    }
    let mut key = [0u8; 18];
    key[0] = b'I';
    a5_spec().serialize(&mut &mut key[1..]).map_err(|_| err(bad("carrier A5 current key")))?;
    let _key = reserve_native_requests_v1(18).map_err(PalwStateV2Error::NativeResource)?;
    if let Some(raw) = route.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())) {
        let root = ComponentRootV2::deserialize_reader(&mut raw.as_slice()).map_err(|_| err(bad("carrier A5 root codec")))?;
        if root.spec != a5_spec() {
            return Err(err(bad("carrier A5 root component differs")));
        }
        // complete_rows checks count and child placement BEFORE each callback; an
        // invalid root cannot visit more than its declared count or actual leaves.
        count = count.checked_add(root.rows.min(leaves)).ok_or(PalwStateV2Error::Overflow("carrier bounded A5 source count"))?;
    }
    count
        .checked_mul(crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<(Hash64, LegacyCarrierLocatorV9), ()>() as u64)
        .ok_or(PalwStateV2Error::Overflow("legacy carrier locator bytes"))
}
pub(super) fn rebuild_legacy_index_v9(s: &mut PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    let _lease =
        reserve_native_requests_v1(legacy_index_requests_v9(s.kernel_route.as_ref())?).map_err(PalwStateV2Error::NativeResource)?;
    let mut index = BTreeSet::new();
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table == crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1 && key.len() == 64 && raw.len() == 64 {
                let mut loc = LegacyCarrierLocatorV9::bound(false);
                loc.kind = 0;
                loc.source = Hash64::from_bytes(key.as_slice().try_into().map_err(|_| err(bad("carrier hint key")))?);
                index.insert((Hash64::from_bytes(raw.as_slice().try_into().map_err(|_| err(bad("carrier hint txid")))?), loc));
            }
        }
    }
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.len() != 65 {
                continue;
            }
            if key[0] == b'S' {
                let source = Hash64::from_bytes(key[1..].try_into().map_err(|_| err(bad("legacy carrier S1 key")))?);
                if let Some(row) = super::palw_claim_local_compact_sources_v1::read(s, source)?
                    && let Some(tx) = row.winner
                {
                    index.insert((tx, locator_for_closed(&row, 1, value(raw).data)));
                }
            }
            if key[0] == b'F' {
                let block = Hash64::from_bytes(key[1..].try_into().map_err(|_| err(bad("carrier native frame key")))?);
                census_frame(s, block, |tx, loc| {
                    index.insert((tx, loc));
                    Ok(())
                })?;
            }
        }
    }
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, reader.current(a5_spec()).map_err(err)?, |key, v| {
        let raw = reader.body(v)?;
        let row = decode::<Closed>(&raw)?;
        if let Some(tx) = row.winner {
            let mut loc = locator_for_closed(&row, 2, v.data);
            loc.catalogue = Hash64::from_bytes(key.try_into().map_err(|_| bad("A5 carrier locator catalogue key"))?);
            index.insert((tx, loc));
        }
        Ok(())
    })
    .map_err(err)?;
    s.claim_local_legacy_carriers_v9 = index;
    s.claim_local_legacy_carriers_ready_v9 = true;
    Ok(())
}
struct AuthenticLegacyOwnerV9 {
    source: Hash64,
    point: PalwBlockContextV2,
    original: NativeRetainedV1<PalwClaimStateV2>,
    closed: Option<NativeRetainedV1<Closed>>,
}
fn legacy_original(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    txid: Hash64,
    loc: LegacyCarrierLocatorV9,
) -> Result<Option<AuthenticLegacyOwnerV9>, PalwStateV2Error> {
    if loc.kind <= 2 {
        let row = if loc.kind == 1 {
            super::palw_claim_local_compact_sources_v1::read(s, loc.source)?
        } else {
            // Read exact real A5 body via catalogue, with actual full point from its
            // decoded manifest. No subsidy=0 reconstruction/current raw claim lookup.
            let reader = StateNodesV2(s);
            let rv = nodes::lookup(&reader, reader.current(a5_spec()).map_err(err)?, loc.catalogue.as_byte_slice()).map_err(err)?;
            if let Some(rv) = rv {
                // Same exact A5 key can monotonically change permitted→denied.
                // Read the CURRENT born-qualified authority; a cache body hash
                // must not pin its older positive outcome.
                let raw = reader.body(rv).map_err(err)?;
                let decoded = decode::<Closed>(&raw).map_err(err)?;
                super::palw_claim_local_exact_closed_source_v5::read_exact_closed_source_v5(s, p, loc.source, &decoded.birth)?
            } else {
                None
            }
        };
        let Some(row) = row else { return Ok(None) };
        if row.claim != loc.source || BirthV9::point(row.birth) != loc.birth || row.winner != Some(txid) {
            return Ok(None);
        }
        let raw = encode(&row.original).map_err(err)?;
        return Ok(Some(AuthenticLegacyOwnerV9 {
            source: row.claim,
            point: row.birth,
            original: decode(&raw).map_err(err)?,
            closed: Some(row),
        }));
    }
    if loc.kind != 3 {
        return Err(err(bad("legacy carrier unknown authority locator")));
    }
    let mut key = [0; 65];
    key[0] = b'F';
    key[1..].copy_from_slice(loc.frame.as_byte_slice());
    let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    let raw = s.kernel_route.as_ref().and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())));
    let Some(raw) = raw else { return Ok(None) };
    let point = PalwBlockContextV2::deserialize_reader(&mut raw.as_slice()).map_err(|_| err(bad("legacy carrier frame point")))?;
    let start = usize::try_from(loc.claim_start).map_err(|_| err(bad("legacy carrier claim offset")))?;
    let n = usize::try_from(loc.claim_len).map_err(|_| err(bad("legacy carrier claim length")))?;
    let original = raw
        .get(start..start.checked_add(n).ok_or_else(|| err(bad("legacy carrier claim range overflow")))?)
        .ok_or_else(|| err(bad("legacy carrier claim locator range")))?;
    let winner = usize::try_from(loc.winner_start).map_err(|_| err(bad("legacy carrier winner offset")))?;
    if BirthV9::point(point) != loc.birth
        || value(original).data != loc.body
        || raw.get(winner..winner.checked_add(64).ok_or_else(|| err(bad("legacy carrier winner range overflow")))?)
            != Some(txid.as_byte_slice())
    {
        return Ok(None);
    }
    let c = decode::<PalwClaimStateV2>(original).map_err(err)?;
    if BirthV9::stored(&c) != loc.birth
        || !matches!(c.source, PalwClaimSourceV2::FreePrompt { .. })
        || !p.claim_local_settlement_active_at(point.daa_score)
    {
        return Err(err(bad("legacy carrier F1 admitted source identity")));
    }
    Ok(Some(AuthenticLegacyOwnerV9 { source: loc.source, point, original: c, closed: None }))
}

fn legacy_range<'a>(s: &'a PalwChainStateV2, txid: Hash64) -> impl Iterator<Item = LegacyCarrierLocatorV9> + 'a {
    s.claim_local_legacy_carriers_v9
        .range((txid, LegacyCarrierLocatorV9::bound(false))..=(txid, LegacyCarrierLocatorV9::bound(true)))
        .map(|(_, loc)| *loc)
}
/// Target-only legacy import. Full restore builds coordinates once; no source
/// payer is charged a global persistent census or a whole F1/state decode.
fn parent_source_ids(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    txid: Hash64,
) -> Result<NativeRetainedV1<BTreeSet<Hash64>>, PalwStateV2Error> {
    if !parent.claim_local_legacy_carriers_ready_v9 && !b.state.claim_local_legacy_carriers_ready_v9 {
        return Err(err(ContextErrorV2::Unavailable("legacy carrier coordinate index was not prepared at full-state boundary")));
    }
    let cache = if parent.claim_local_legacy_carriers_ready_v9 { parent } else { &b.state };
    let locator_count = legacy_range(cache, txid).count();
    let mut locq = TirNativeRequestsV1::new();
    locq.exact_v1::<LegacyCarrierLocatorV9>(locator_count as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _locations = reserve_native_requests_v1(locq.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut locations = Vec::with_capacity(locator_count);
    locations.extend(legacy_range(cache, txid));
    let canonical = lookup::<ReverseV9>(parent, REVERSE, None, txid).map_err(err)?;
    let count = (canonical.as_ref().map_or(0, |r| r.owners.len()) as u64)
        .checked_add(locations.len() as u64)
        .ok_or(PalwStateV2Error::Overflow("spent carrier exact candidates"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<Hash64, ()>>(
        count.checked_mul(2).ok_or(PalwStateV2Error::Overflow("carrier candidate sets"))?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut ids = BTreeSet::new();
    let mut pending = BTreeSet::new();
    let mut hint = false;
    if let Some(r) = canonical {
        if r.version != 9 || r.txid != txid {
            return Err(err(bad("parent carrier reverse index differs")));
        }
        for id in &r.owners {
            let owner = summary(parent, *id).map_err(err)?.ok_or_else(|| err(bad("parent carrier owner absent")))?;
            if owner.txid != txid {
                return Err(err(bad("parent carrier owner txid differs")));
            }
            ids.insert(*id);
        }
    }
    for loc in locations {
        if loc.kind == 0 {
            hint = true;
            continue;
        }
        let id = hash(1, &(txid, loc.source, loc.birth)).map_err(err)?;
        if ids.contains(&id) {
            continue;
        }
        pending.insert(id);
        if let Some(owner) = legacy_original(parent, b.params, txid, loc)? {
            // Existing A5/S1 original is read at exact birth/winner even when current
            // table167/current Claim already belong to a different cohort.
            let minted = atomic(b, |b| {
                let id = mint_from_original(b, txid, owner.source, owner.point, &owner.original)?;
                if let Some(closed) = &owner.closed {
                    attach_closed_proof(b, id, closed)?;
                }
                Ok(id)
            })
            .map_err(err)?;
            if minted != id {
                return Err(err(bad("legacy targeted import actual birth changed")));
            }
            ids.insert(id);
        }
    }
    if pending.iter().any(|id| !ids.contains(id)) || hint && ids.is_empty() {
        return Err(err(ContextErrorV2::Unavailable(
            "known historical winning carrier lost its original born authority; cannot reconstruct from current raw slot",
        )));
    }
    if ids.len() > 1 {
        return Err(err(bad("spent winning carrier has ambiguous exact source owners")));
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(ids, lease))
}
/// Borrowed exact-born authority. Terminal native summaries can be used without
/// original material. Unresolved keeps its actual original Claim body leased.
pub(super) struct CarrierSourceViewV9 {
    record: NativeRetainedV1<SummaryV9>,
    original: Option<NativeRetainedV1<PalwClaimStateV2>>,
    closed: Option<NativeRetainedV1<Closed>>,
}
impl CarrierSourceViewV9 {
    pub(super) fn id(&self) -> Hash64 {
        self.record.id
    }
    pub(super) fn source(&self) -> Hash64 {
        self.record.source
    }
    pub(super) fn accepting_point(&self) -> PalwBlockContextV2 {
        self.record.point
    }
    pub(super) fn winning_txid(&self) -> Hash64 {
        self.record.txid
    }
    pub(super) fn outcome(&self) -> CarrierOutcomeV9 {
        self.record.outcome
    }
    pub(super) fn original_claim(&self) -> Option<&PalwClaimStateV2> {
        self.original.as_deref()
    }
    pub(super) fn closed_source_proof(&self) -> Option<&Closed> {
        self.closed.as_deref()
    }
    /// Canonical actual native summary and its selected owned literal bodies.
    /// The caller binds this only through an authenticated actual consumer Fact.
    pub(super) fn visit_native_material_v24(
        &self,
        mut f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
    ) -> Result<(), PalwStateV2Error> {
        let raw = encode(&*self.record).map_err(err)?;
        f(&raw)?;
        if let Some(original) = self.original.as_ref() {
            let raw = encode(&**original).map_err(err)?;
            f(&raw)?;
        }
        if let Some(closed) = self.closed.as_ref() {
            let raw = encode(&**closed).map_err(err)?;
            f(&raw)?;
        }
        Ok(())
    }
}
fn view(s: &PalwChainStateV2, id: Hash64) -> Result<CarrierSourceViewV9, ContextErrorV2> {
    let r = summary(s, id)?.ok_or(ContextErrorV2::Unavailable("exact winning carrier summary absent"))?;
    if r.outcome == CarrierOutcomeV9::Denied {
        return Ok(CarrierSourceViewV9 { record: r, original: None, closed: None });
    }
    let original = r.original.map(|v| body::<PalwClaimStateV2>(s, v)).transpose()?;
    if let Some(c) = &original
        && (!matches!(c.source, PalwClaimSourceV2::FreePrompt { .. })
            || BirthV9::stored(c) != r.stored
            || c.class_id != r.class
            || c.bond != r.bond
            || c.execution_root != r.execution
            || c.output_root != r.output
            || c.trace_root != r.trace)
    {
        return Err(bad("carrier summary original identity differs"));
    }
    let closed = r.closed_proof.map(|v| body::<Closed>(s, v)).transpose()?;
    if closed.as_ref().is_some_and(|c| {
        c.claim != r.source || c.birth != r.point || c.winner != Some(r.txid) || original.as_deref() != Some(&c.original)
    }) {
        return Err(bad("carrier compact closure proof changed born owner"));
    }
    if let Some(current) = current_exact_a5(s, &r)?
        && (closed.as_ref().is_some_and(|old| old.permitted != current.permitted)
            || r.outcome == CarrierOutcomeV9::PermittedClosed && !current.permitted)
    {
        return Err(bad("carrier source proof contradicts current exact native A5 outcome"));
    }
    Ok(CarrierSourceViewV9 { record: r, original, closed })
}
/// The native semantic owner calls only after proving THIS born source's actual
/// full closure/outcome. Producer messages cannot call this API. PermittedClosed
/// is irreversible; E expiry is NOT such a witness and never calls this itself.
pub(super) fn note_native_closed_outcome_v9(
    b: &mut TransitionBuilder<'_>,
    owner: &CarrierSourceViewV9,
    at: PalwBlockContextV2,
    outcome: CarrierOutcomeV9,
) -> Result<(), PalwStateV2Error> {
    if b.claim_local_lock_point_v6 != Some(at) {
        return Err(err(bad("carrier closure lacks actual executing point")));
    }
    if outcome == CarrierOutcomeV9::Unresolved {
        return Err(err(bad("carrier closure cannot become unresolved")));
    }
    atomic(b, |b| {
        let r = summary(&b.state, owner.id())?.ok_or(bad("carrier closure exact owner absent"))?;
        if r.source != owner.source() || r.point != owner.accepting_point() || r.txid != owner.winning_txid() {
            return Err(bad("carrier closure substituted born source"));
        }
        if r.outcome != CarrierOutcomeV9::Unresolved {
            if r.outcome != outcome {
                return Err(bad("irreversible native carrier outcome changed"));
            }
            return Ok(());
        }
        if at.daa_score < r.point.daa_score || at.blue_score < r.point.blue_score {
            return Err(bad("carrier closure precedes born source"));
        }
        let mut r = *r;
        r.original = None;
        r.closed_proof = None;
        r.outcome = outcome;
        r.closed = Some(at);
        put_summary(b, &r)
    })
    .map_err(err)
}
/// Persist even an empty complete fact list at actual consumer admission. Old
/// raw ConsumedCarrier tags remain byte-for-byte but are not birth authority.
/// Source summaries/reverse rows are never collected by E.material_until.
fn capture_consumer_facts_inner_v9(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    point: PalwBlockContextV2,
    consumer: Hash64,
    original: &PalwClaimStateV2,
    source_manifest: RowValueV2,
) -> Result<(), PalwStateV2Error> {
    let count = b
        .extras
        .tir_funded_originals
        .get(&consumer)
        .map_or(0, |p| p.carrier().inputs.len())
        .checked_add(b.extras.carrier_input_spends.get(&consumer).map_or(0, |v| v.len()))
        .ok_or(PalwStateV2Error::Overflow("carrier actual spend pair count"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Hash64>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _pairs = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut spent = Vec::with_capacity(count);
    if let Some(p) = b.extras.tir_funded_originals.get(&consumer) {
        for i in &p.carrier().inputs {
            spent.push(i.previous_outpoint.transaction_id)
        }
    }
    if let Some(ids) = b.extras.carrier_input_spends.get(&consumer) {
        spent.extend_from_slice(ids)
    }
    spent.sort();
    spent.dedup();
    // One list per spent tx holds its own exact scoped set lease while facts are
    // admitted; no Vec/graph allocation happens after a producer count alone.
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<NativeRetainedV1<BTreeSet<Hash64>>>(spent.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _sets = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut owners = Vec::with_capacity(spent.len());
    let mut n = 0u64;
    for txid in &spent {
        let ids = parent_source_ids(b, parent, *txid)?;
        n = n.checked_add(ids.len() as u64).ok_or(PalwStateV2Error::Overflow("carrier admitted fact count"))?;
        owners.push(ids)
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<CarrierFactV9>(n).map_err(PalwStateV2Error::NativeResource)?;
    let _facts = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut facts = Vec::with_capacity(usize::try_from(n).map_err(|_| PalwStateV2Error::Overflow("carrier admitted fact capacity"))?);
    for (txid, ids) in spent.into_iter().zip(owners) {
        for id in ids.iter() {
            let owner = view(&b.state, *id).map_err(err)?;
            let birth = owner.accepting_point();
            if birth.blue_score >= point.blue_score || birth.daa_score > point.daa_score {
                return Err(err(bad("spent winning carrier source is not causally prior")));
            }
            // Same raw ID with an older actual birth is a REAL dependency, not self.
            facts.push(CarrierFactV9 {
                consumer,
                consumer_point: point,
                consumer_stored: BirthV9::stored(original),
                spent_txid: txid,
                source: owner.source(),
                source_point: birth,
                summary: *id,
            });
        }
    }
    let row = FactsV9 { version: 9, consumer, point, stored: BirthV9::stored(original), source_manifest, facts };
    #[cfg(test)]
    {
        let geometry = consumer_facts_codec_from_native_inputs_v18(b.extras, consumer, &point)?;
        assert!(row.facts.len() as u64 <= geometry.fact_count_max_v18());
        assert!(borsh::object_length(&row).unwrap() as u64 <= geometry.body_bytes_max_v18());
    }
    atomic(b, |b| {
        let key = consumer_key(consumer, point)?;
        if lookup::<FactsV9>(&b.state, CONSUMER, None, key)?.is_some() {
            return Err(bad("duplicate actual consumer carrier facts"));
        }
        put(b, CONSUMER, None, key, Some(&row))?;
        Ok(())
    })
    .map_err(err)
}
impl CarrierFactV9 {
    pub(super) fn consumer(&self) -> Hash64 {
        self.consumer
    }
    pub(super) fn consumer_point(&self) -> PalwBlockContextV2 {
        self.consumer_point
    }
    pub(super) fn spent_txid(&self) -> Hash64 {
        self.spent_txid
    }
    pub(super) fn source(&self) -> Hash64 {
        self.source
    }
    pub(super) fn source_point(&self) -> PalwBlockContextV2 {
        self.source_point
    }
    pub(super) fn summary_id(&self) -> Hash64 {
        self.summary
    }
}
/// Root/B actual consumer resolves every fact through its immutable born owner;
/// missing summary is Unavailable, never an ordinary independent transaction.
pub(super) fn visit_consumer_facts_v9(
    s: &PalwChainStateV2,
    consumer: Hash64,
    point: PalwBlockContextV2,
    mut f: impl FnMut(&CarrierFactV9, CarrierSourceViewV9) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let Some(row) = lookup::<FactsV9>(s, CONSUMER, None, consumer_key(consumer, point).map_err(err)?).map_err(err)? else {
        return Ok(false);
    };
    if row.version != 9 || row.consumer != consumer || row.point != point {
        return Err(err(bad("consumer carrier fact birth differs")));
    }
    for fact in &row.facts {
        let owner = view(s, fact.summary).map_err(err)?;
        if fact.consumer != consumer
            || fact.consumer_point != point
            || fact.consumer_stored != row.stored
            || fact.spent_txid != owner.winning_txid()
            || fact.source != owner.source()
            || fact.source_point != owner.accepting_point()
        {
            return Err(err(bad("consumer carrier fact substituted born source")));
        }
        f(fact, owner)?
    }
    Ok(true)
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
            (table == PALW_CLAIM_LOCAL_TABLE_V1 && matches!(key.first().copied(), Some(b'N' | b'B' | b'I' | b'A' | b'S')))
                || table == crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1
        })?;
    }
    answer
}

pub(super) fn capture_consumer_facts_v9(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    point: PalwBlockContextV2,
    consumer: Hash64,
    original: &PalwClaimStateV2,
    source_manifest: RowValueV2,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| capture_consumer_facts_inner_v9(b, parent, point, consumer, original, source_manifest).map_err(native_err))
        .map_err(err)
}

/// Exact live raw workref inclusion, after born authority was already resolved.
/// Carrier namespace is FP only, so its actual accepting block is its stored
/// block; Attempt RED blocks are never treated as carrier source authority.
pub(super) fn is_exact_live_source_v9(state: &PalwChainStateV2, owner: &CarrierSourceViewV9) -> bool {
    state.claims.get(&owner.source()).is_some_and(|c| {
        matches!(c.source, PalwClaimSourceV2::FreePrompt { .. })
            && BirthV9::stored(c) == owner.record.stored
            && c.class_id == owner.record.class
            && c.bond == owner.record.bond
            && c.execution_root == owner.record.execution
            && c.output_root == owner.record.output
            && c.trace_root == owner.record.trace
    })
}
/// Direct point query for Root/B consumers that already have an authenticated
/// exact fact tuple. It never resolves by whichever raw Claim is live now.
pub(super) fn with_exact_source_v9<T>(
    state: &PalwChainStateV2,
    source: Hash64,
    point: PalwBlockContextV2,
    winner: Hash64,
    read: impl FnOnce(CarrierSourceViewV9) -> Result<T, PalwStateV2Error>,
) -> Result<Option<T>, PalwStateV2Error> {
    let id = summary_id(winner, source, point).map_err(err)?;
    if summary(state, id).map_err(err)?.is_none() {
        return Ok(None);
    }
    let owner = view(state, id).map_err(err)?;
    if owner.source() != source || owner.accepting_point() != point || owner.winning_txid() != winner {
        return Err(err(bad("exact carrier source tuple differs")));
    }
    read(owner).map(Some)
}
/// Called only together with real consumer source-duty collection. Winner
/// summary/reverse rows intentionally survive; UTXO/native-summary retention
/// ownership is a DIFFERENT native callback, never inferred from E.H.
pub(super) fn release_consumer_facts_v9(
    b: &mut TransitionBuilder<'_>,
    consumer: Hash64,
    point: PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        put::<FactsV9>(b, CONSUMER, None, consumer_key(consumer, point)?, None)?;
        Ok(())
    })
    .map_err(err)
}
fn verify_summary_inventory(s: &PalwChainStateV2, row: &SummaryV9, value: RowValueV2) -> Result<(), ContextErrorV2> {
    let a = inventory(value, row.original, row.closed_proof)?;
    let id = inventory_id(&a)?;
    let _key = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
    let raw = s
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, akey(id).to_vec())))
        .ok_or(ContextErrorV2::Unavailable("carrier summary inventory missing"))?;
    crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&*a, raw).map_err(|_| bad("carrier summary inventory differs"))
}
pub(super) fn validate_carrier_lineage_v9(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, reader.current(spec(SUMMARY))?, |key, value| {
        let raw = reader.body(value)?;
        let row = decode::<SummaryV9>(&raw)?;
        if key != row.id.as_byte_slice() {
            return Err(bad("carrier summary leaf identity"));
        }
        let owner = view(s, row.id)?;
        verify_summary_inventory(s, &row, value)?;
        let reverse = lookup::<ReverseV9>(s, REVERSE, None, row.txid)?.ok_or(bad("carrier summary reverse owner absent"))?;
        if !reverse.owners.contains(&owner.id()) {
            return Err(bad("carrier reverse source membership differs"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, reader.current(spec(REVERSE))?, |key, value| {
        let raw = reader.body(value)?;
        let row = decode::<ReverseV9>(&raw)?;
        if row.version != 9 || key != row.txid.as_byte_slice() || row.owners.windows(2).any(|w| w[0] >= w[1]) {
            return Err(bad("carrier reverse owner codec/order"));
        }
        for id in &row.owners {
            let r = summary(s, *id)?.ok_or(bad("carrier reverse points absent source"))?;
            if r.txid != row.txid {
                return Err(bad("carrier reverse substituted txid"));
            }
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, reader.current(spec(CONSUMER))?, |key, value| {
        let raw = reader.body(value)?;
        let row = decode::<FactsV9>(&raw)?;
        if key != consumer_key(row.consumer, row.point)?.as_byte_slice()
            || row.version != 9
            || row.stored.daa != row.point.daa_score
            || row.stored.blue != row.point.blue_score
            || row.facts.windows(2).any(|w| w[0].spent_txid >= w[1].spent_txid)
        {
            return Err(bad("carrier consumer actual fact codec/order"));
        }
        let manifest = super::palw_claim_local_source_admission_v8::manifest_reference_v9(s, row.consumer, row.point)
            .map_err(native_err)?
            .ok_or(bad("carrier consumer source admission absent"))?;
        if manifest != row.source_manifest {
            return Err(bad("carrier consumer actual manifest changed"));
        }
        visit_consumer_facts_v9(s, row.consumer, row.point, |fact, _| {
            if fact.source_point.blue_score >= row.point.blue_score || fact.source_point.daa_score > row.point.daa_score {
                return Err(err(bad("carrier fact source is not prior")));
            }
            Ok(())
        })
        .map_err(native_err)?;
        Ok(())
    })?;
    if reader.current(spec(4))?.rows != 0 {
        return Err(bad("carrier literal scratch index not empty"));
    }
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'I') {
                continue;
            }
            let r = decode::<ComponentRootV2>(raw)?;
            if r.spec.owner != 9 || r.spec.schema != 9 {
                continue;
            }
            if key.len() != 18 || !matches!(r.spec.table, SUMMARY | REVERSE | CONSUMER | 4) || r.spec != spec(r.spec.table) {
                return Err(bad("carrier allocated component root specification"));
            }
        }
    }
    Ok(())
}

/// Global fork/preparation boundary only, before any accepted object/payer.
/// The actual copied parent native rows fund the derived migration once.
pub(super) fn prepare_global_legacy_index_v9(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) || b.state.claim_local_legacy_carriers_ready_v9 {
        return Ok(());
    }
    let bytes = legacy_index_requests_v9(b.state.kernel_route.as_ref())?;
    if b.native_writers_tracking_v1() {
        b.native_reservation.grow_v1(bytes).map_err(PalwStateV2Error::NativeResource)?;
    }
    rebuild_legacy_index_v9(&mut b.state)
}

fn attach_closed_proof(b: &mut TransitionBuilder<'_>, id: Hash64, closed: &Closed) -> Result<(), ContextErrorV2> {
    let r = summary(&b.state, id)?.ok_or(bad("carrier compact proof owner absent"))?;
    if r.source != closed.claim || r.point != closed.birth || closed.winner != Some(r.txid) {
        return Err(bad("carrier compact proof substituted exact owner"));
    }
    if r.outcome != CarrierOutcomeV9::Unresolved {
        return Ok(());
    }
    let proof = literal(b, closed)?;
    let mut r = *r;
    r.closed_proof = Some(proof);
    put_summary(b, &r)
}

/// Atomic same-owner metadata wrapper used by actual A5 semantic publication.
/// It is not callable by a producer payload and cannot infer any new outcome.
pub(super) fn atomic_source_metadata_v9<T>(
    b: &mut TransitionBuilder<'_>,
    f: impl FnOnce(&mut TransitionBuilder<'_>) -> Result<T, PalwStateV2Error>,
) -> Result<T, PalwStateV2Error> {
    atomic(b, |b| f(b).map_err(native_err)).map_err(err)
}
/// Actual A5 denial owner ONLY: invoked after its existing E-conviction check
/// and exact source/producer match, in the same metadata transaction as A5/S1.
pub(super) fn note_actual_denied_source_v9(b: &mut TransitionBuilder<'_>, row: &Closed) -> Result<(), PalwStateV2Error> {
    if row.permitted {
        return Err(err(bad("actual carrier denial has permitted source proof")));
    }
    let Some(tx) = row.winner else { return Ok(()) };
    let at = b.claim_local_lock_point_v6.ok_or_else(|| err(bad("actual carrier denial lacks executing point")))?;
    atomic(b, |b| {
        let id = mint_from_original(b, tx, row.claim, row.birth, &row.original)?;
        let old = summary(&b.state, id)?.ok_or(bad("actual denied carrier summary missing"))?;
        if old.outcome == CarrierOutcomeV9::PermittedClosed {
            return Err(bad("native denied source contradicts irreversible fully closed carrier"));
        }
        let mut next = *old;
        next.outcome = CarrierOutcomeV9::Denied;
        next.closed = Some(at);
        next.original = None;
        next.closed_proof = None;
        put_summary(b, &next)
    })
    .map_err(err)
}
fn current_exact_a5(s: &PalwChainStateV2, r: &SummaryV9) -> Result<Option<NativeRetainedV1<Closed>>, ContextErrorV2> {
    let key = crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/closed-claim-source/birth/v5",
        &(crate::palw_claim_local_v1::PalwClaimLocalWorkRefV1::ChainClaim { claim: r.source }, r.point),
    )
    .map_err(|_| bad("carrier exact A5 source key"))?;
    let reader = StateNodesV2(s);
    let Some(value) = nodes::lookup(&reader, reader.current(a5_spec())?, key.as_byte_slice())? else {
        return Ok(None);
    };
    let raw = reader.body(value)?;
    let c = decode::<Closed>(&raw)?;
    if c.claim != r.source || c.birth != r.point || c.original.bond != r.bond || c.original.execution_root != r.execution {
        return Err(bad("carrier current exact A5 source identity"));
    }
    Ok(Some(c))
}

/// Compatibility facts from a genuine consumer F1 and its ACTUAL parent
/// carriage. This never persists rows, projects a state, or infers source birth
/// from today's raw Claim/167 slot. False is incomplete legacy provenance, not
/// an authenticated complete-empty new admission.
pub(super) fn visit_legacy_consumer_facts_v9(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    consumer: &super::palw_claim_local_source_origin_v8::SourceOriginV8,
    visit: impl FnMut(&CarrierFactV9, CarrierSourceViewV9) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    consumer.visit_legacy_carrier_facts_v9(s, p, visit)
}
/// Called only by the sealed SourceOrigin dispatch after authenticating the
/// real consumer admission. It borrows one original prior carriage, retaining
/// that lease for lazy per-leg callbacks; no synthetic frame is constructed.
pub(super) fn visit_actual_legacy_context_v9(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    consumer: &crate::palw_claim_local_v1::PalwClaimLocalAdmissionContextV1,
    mut visit: impl FnMut(&CarrierFactV9, CarrierSourceViewV9) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    use crate::palw_claim_local_v1::PalwClaimLocalInputV1 as Input;
    let id = consumer.claim_id();
    let point = consumer.accepting_point();
    if !consumer.frame().inputs.iter().any(|x| matches!(x,Input::ConsumedCarrier{consumer,..}if *consumer==id)) {
        // A sealed actual F1 input inventory is genuine complete-empty legacy
        // authority. Missing scoped facts/F1 never reaches this branch.
        return Ok(true);
    }
    super::palw_claim_local_rewards_v1::frame_read(consumer.frame(), |prior, _| {
        let Some(route) = &prior.kernel_route else {
            return Ok(false);
        };
        for (ordinal, input) in consumer.frame().inputs.iter().enumerate() {
            let Input::ConsumedCarrier { consumer: owner, source } = input else {
                continue;
            };
            if *owner != id {
                continue;
            }
            if consumer.frame().inputs[..ordinal]
                .iter()
                .any(|x| matches!(x,Input::ConsumedCarrier{consumer,source:old}if *consumer==id&&old==source))
            {
                return Ok(false);
            }
            // Legacy tag8 was derived from a UNIQUE source→winning-tx row in
            // this actual parent, not the current raw namespace.
            let Some((_, raw)) = route.aux.iter().find(|((table, key), _)| {
                *table == crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1
                    && key.as_slice() == source.as_byte_slice()
            }) else {
                return Ok(false);
            };
            let Ok(bytes) = <[u8; 64]>::try_from(raw.as_slice()) else {
                return Err(err(bad("legacy carrier parent winning row codec")));
            };
            let txid = Hash64::from_bytes(bytes);
            if route
                .aux
                .iter()
                .filter(|((table, key), value)| {
                    *table == crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1
                        && key.len() == 64
                        && value.as_slice() == txid.as_byte_slice()
                })
                .count()
                != 1
            {
                return Ok(false);
            }
            let closed = {
                route
                    .aux
                    .iter()
                    .find(|((table, key), _)| {
                        *table == PALW_CLAIM_LOCAL_TABLE_V1 && key.len() == 65 && key[0] == b'S' && key[1..] == *source.as_byte_slice()
                    })
                    .map(|(_, raw)| raw.as_slice())
            };
            let Some(source_view) = legacy_leg_v9(s, p, *source, txid, point, closed)? else {
                return Ok(false);
            };
            let fact = CarrierFactV9 {
                consumer: id,
                consumer_point: point,
                consumer_stored: BirthV9::stored(consumer.original_claim()),
                spent_txid: txid,
                source: *source,
                source_point: source_view.accepting_point(),
                summary: source_view.id(),
            };
            // In particular, a known Denied callback returns immediately before
            // any later tag's source, original, E or approval is hydrated.
            visit(&fact, source_view)?;
        }
        Ok(true)
    })
}
fn legacy_leg_matches_v9(point: PalwBlockContextV2, consumer: PalwBlockContextV2) -> bool {
    point.daa_score <= consumer.daa_score && point.blue_score < consumer.blue_score
}
fn legacy_leg_v9(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    source: Hash64,
    txid: Hash64,
    consumer: PalwBlockContextV2,
    parent_closed: Option<&[u8]>,
) -> Result<Option<CarrierSourceViewV9>, PalwStateV2Error> {
    let mut found = None;
    if let Some(reverse) = lookup::<ReverseV9>(s, REVERSE, None, txid).map_err(err)? {
        if reverse.version != 9 || reverse.txid != txid {
            return Err(err(bad("legacy carrier canonical reverse identity")));
        }
        for id in &reverse.owners {
            let row = summary(s, *id).map_err(err)?.ok_or_else(|| err(bad("legacy carrier reverse missing owner")))?;
            if row.source != source || row.txid != txid || !legacy_leg_matches_v9(row.point, consumer) {
                continue;
            }
            if found.is_some() {
                return Ok(None);
            }
            found = Some(view(s, *id).map_err(err)?);
        }
    }
    if found.is_some() {
        return Ok(found);
    }
    if let Some(raw) = parent_closed {
        let closed = decode::<Closed>(raw).map_err(err)?;
        if closed.version != 1
            || closed.claim != source
            || closed.winner != Some(txid)
            || BirthV9::stored(&closed.original) != BirthV9::point(closed.birth)
            || !matches!(closed.terminal_phase, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. })
            || !legacy_leg_matches_v9(closed.birth, consumer)
        {
            return Ok(None);
        }
        return transient_legacy_view_v9(s, source, txid, closed.birth, &closed.original, Some(&closed)).map(Some);
    }
    // Only authenticated, rebuilt exact-winner coordinates may supply a source
    // whose original was no longer present in that consumer's parent Claim/S1.
    if !s.claim_local_legacy_carriers_ready_v9 {
        return Ok(None);
    }
    for loc in legacy_range(s, txid) {
        if loc.kind == 0 || loc.source != source {
            continue;
        }
        let Some(owner) = legacy_original(s, p, txid, loc)? else {
            continue;
        };
        if !legacy_leg_matches_v9(owner.point, consumer) {
            continue;
        }
        let id = summary_id(txid, source, owner.point).map_err(err)?;
        if let Some(old) = &found {
            if old.id() != id {
                return Ok(None);
            }
            continue;
        }
        found = Some(transient_legacy_view_v9(s, source, txid, owner.point, &owner.original, owner.closed.as_deref())?);
    }
    Ok(found)
}
/// A transient view is authenticated by the genuine source original/parent
/// proof above; it is NOT a fabricated rooted native Summary row. It cannot
/// mint a native closed status (that writer requires an existing exact row).
fn transient_legacy_view_v9(
    s: &PalwChainStateV2,
    source: Hash64,
    txid: Hash64,
    point: PalwBlockContextV2,
    original: &PalwClaimStateV2,
    closed: Option<&Closed>,
) -> Result<CarrierSourceViewV9, PalwStateV2Error> {
    if !matches!(original.source, PalwClaimSourceV2::FreePrompt { .. }) || BirthV9::stored(original) != BirthV9::point(point) {
        return Err(err(bad("legacy carrier original is not exact admitted FP")));
    }
    let raw = encode(original).map_err(err)?;
    let original_value = value(&raw);
    let original = decode::<PalwClaimStateV2>(&raw).map_err(err)?;
    let mut row = SummaryV9 {
        version: 9,
        id: summary_id(txid, source, point).map_err(err)?,
        txid,
        source,
        point,
        stored: BirthV9::point(point),
        class: original.class_id,
        bond: original.bond,
        execution: original.execution_root,
        output: original.output_root,
        trace: original.trace_root,
        original: Some(original_value),
        closed_proof: None,
        outcome: CarrierOutcomeV9::Unresolved,
        closed: None,
    };
    // Current exact A5 may contain an actual later denial. Never pin a stale
    // positive historical parent proof over that same-born native authority.
    let current = current_exact_a5(s, &row).map_err(err)?;
    let selected = current.as_deref().or(closed);
    let closed = if let Some(c) = selected {
        if c.claim != source || c.birth != point || c.winner != Some(txid) || c.original != *original {
            return Err(err(bad("legacy carrier proof differs from actual source original")));
        }
        let raw = encode(c).map_err(err)?;
        row.closed_proof = Some(value(&raw));
        Some(decode::<Closed>(&raw).map_err(err)?)
    } else {
        None
    };
    let record_raw = encode(&row).map_err(err)?;
    let record = decode::<SummaryV9>(&record_raw).map_err(err)?;
    Ok(CarrierSourceViewV9 { record, original: Some(original), closed })
}

// Read-only actual metadata evidence; no paid permit/prospective plan authority.
#[path = "palw_claim_local_carrier_geometry_v9.rs"]
mod actual_geometry_v9;
#[cfg(test)]
pub(super) use actual_geometry_v9::{arm_geometry_observer_failure_v9, geometry_observer_failure_hit_v9};
#[cfg(test)]
pub(super) use actual_geometry_v9::{capture_consumer_facts_observed_v9, write_actual_winner_observed_v9};
pub(super) use actual_geometry_v9::{
    observe_sealed_consumer_geometry_v9, observe_sealed_winner_geometry_v9, visit_observed_sources_v9,
};

/// Target-only equivalent of the old table167 projection, not source/birth
/// authority. Use the already initialized native/legacy reverse coordinates,
/// then verify each candidate against the ORIGINAL parent's actual current row.
/// Never rebuild/scan the whole winner population for an individual source.
pub(super) fn current_winner_claim_targeted_v17(
    parent: &PalwChainStateV2,
    index_state: &PalwChainStateV2,
    txid: Hash64,
) -> Result<Option<Hash64>, PalwStateV2Error> {
    let cache = if parent.claim_local_legacy_carriers_ready_v9 {
        parent
    } else if index_state.claim_local_legacy_carriers_ready_v9 {
        index_state
    } else {
        return Err(PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
            LegacyCarrierLocatorV9,
        >(
            "target-only winning carrier projection",
            "native full-state reverse initializer has not established coverage",
        )));
    };
    let _key = reserve_native_requests_v1(64).map_err(PalwStateV2Error::NativeResource)?;
    let (mut unique, mut ambiguous) = (None, false);
    let mut consider = |source: Hash64| {
        let current = parent.kernel_route.as_ref().and_then(|route| {
            route.aux.get(&(crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1, source.as_byte_slice().to_vec()))
        });
        if current.map(Vec::as_slice) == Some(txid.as_byte_slice()) {
            match unique {
                None => unique = Some(source),
                Some(old) if old != source => ambiguous = true,
                _ => {}
            }
        }
    };
    if let Some(reverse) = lookup::<ReverseV9>(parent, REVERSE, None, txid).map_err(err)? {
        if reverse.version != 9 || reverse.txid != txid {
            return Err(err(bad("targeted current winner reverse identity")));
        }
        for owner in &reverse.owners {
            let summary = summary(parent, *owner).map_err(err)?.ok_or_else(|| err(bad("targeted current winner summary absent")))?;
            if summary.txid != txid {
                return Err(err(bad("targeted current winner summary txid")));
            }
            consider(summary.source);
        }
    }
    for loc in legacy_range(cache, txid) {
        if loc.kind == 0 {
            consider(loc.source);
        }
    }
    Ok(if ambiguous { None } else { unique })
}

/// Upper codec footprint for THIS actual consumer's native input pairs. Exact
/// ancestor identities remain the responsibility of parent_source_ids; its
/// existing >1-owner refusal makes successful facts <= distinct input txids.
/// This is geometry only, never a source readiness or paid-duty constructor.
/// Fixed owner schema getter: grants no body/count/payment authority.
pub(super) fn consumer_facts_component_spec_v18() -> ComponentSpecV2 {
    spec(CONSUMER)
}
pub(super) struct NativeConsumerFactsCodecV18 {
    bytes: u64,
    maximum_facts: u64,
}
impl NativeConsumerFactsCodecV18 {
    pub(super) fn body_bytes_max_v18(&self) -> u64 {
        self.bytes
    }
    pub(super) fn fact_count_max_v18(&self) -> u64 {
        self.maximum_facts
    }
    pub(super) fn component_spec_v18(&self) -> ComponentSpecV2 {
        spec(CONSUMER)
    }
}
pub(super) fn consumer_facts_codec_from_native_inputs_v18(
    extras: &PalwTransitionExtrasV1,
    consumer: Hash64,
    point: &PalwBlockContextV2,
) -> Result<NativeConsumerFactsCodecV18, PalwStateV2Error> {
    let count = extras
        .tir_funded_originals
        .get(&consumer)
        .map_or(0, |p| p.carrier().inputs.len())
        .checked_add(extras.carrier_input_spends.get(&consumer).map_or(0, Vec::len))
        .ok_or(PalwStateV2Error::Overflow("consumer facts native input geometry"))?;
    let width = |v: Result<usize, std::io::Error>| v.map(|n| n as u64).map_err(|_| err(bad("consumer facts codec geometry")));
    let birth = BirthV9::point(*point);
    let h = Hash64::default();
    let prefix = width(borsh::object_length(&(9u16, consumer, *point, birth, RowValueV2 { data: h, bytes: 0 }, 0u32)))?;
    let each = width(borsh::object_length(&(consumer, *point, birth, h, h, *point, h)))?;
    let maximum_facts = u64::try_from(count).map_err(|_| PalwStateV2Error::Overflow("consumer facts input count"))?;
    if maximum_facts > u32::MAX as u64 {
        return Err(PalwStateV2Error::Overflow("consumer facts canonical Vec count"));
    }
    let bytes = maximum_facts
        .checked_mul(each)
        .and_then(|n| n.checked_add(prefix))
        .ok_or(PalwStateV2Error::Overflow("consumer facts codec bound"))?;
    Ok(NativeConsumerFactsCodecV18 { bytes, maximum_facts })
}

/// Read-only exact ORIGINAL-parent lineage preparation. Unlike the admission
/// writer this never imports/mints a summary while quoting a consumer.
pub(super) fn visit_original_parent_source_v19(
    parent: &PalwChainStateV2,
    index_state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    txid: Hash64,
    visit: impl FnOnce(Hash64, PalwBlockContextV2) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let cache = if parent.claim_local_legacy_carriers_ready_v9 {
        parent
    } else if index_state.claim_local_legacy_carriers_ready_v9 {
        index_state
    } else {
        return Err(err(ContextErrorV2::Unavailable("original-parent lineage index not initialized")));
    };
    let (mut unique, mut hint) = (None, false);
    let mut add = |source: Hash64, point: PalwBlockContextV2| -> Result<(), PalwStateV2Error> {
        if unique.is_some_and(|old| old != (source, point)) {
            return Err(err(bad("spent winning carrier has ambiguous exact source owners")));
        }
        unique = Some((source, point));
        Ok(())
    };
    if let Some(reverse) = lookup::<ReverseV9>(parent, REVERSE, None, txid).map_err(err)? {
        if reverse.version != 9 || reverse.txid != txid {
            return Err(err(bad("original-parent reverse identity")));
        }
        for id in &reverse.owners {
            let owner = summary(parent, *id).map_err(err)?.ok_or_else(|| err(bad("original-parent summary missing")))?;
            if owner.txid != txid {
                return Err(err(bad("original-parent summary transaction differs")));
            }
            add(owner.source, owner.point)?;
        }
    }
    for loc in legacy_range(cache, txid) {
        if loc.kind == 0 {
            hint = true;
            continue;
        }
        let owner = legacy_original(parent, params, txid, loc)?
            .ok_or_else(|| err(ContextErrorV2::Unavailable("known legacy ancestor lost immutable original")))?;
        add(owner.source, owner.point)?;
    }
    if let Some((source, point)) = unique {
        visit(source, point)?;
    } else if hint {
        return Err(err(ContextErrorV2::Unavailable("known winning carrier lacks exact original birth")));
    }
    Ok(())
}
/// Exact native consumer fact literal/discovery key, not a maximum or a source
/// winner/UTXO standing grant. No global reverse/current index is pinned.
pub(super) fn native_consumer_fact_reference_v19(
    s: &PalwChainStateV2,
    consumer: Hash64,
    point: PalwBlockContextV2,
) -> Result<Option<(RowValueV2, Hash64)>, PalwStateV2Error> {
    let key = consumer_key(consumer, point).map_err(err)?;
    let reader = StateNodesV2(s);
    let Some(value) = nodes::lookup(&reader, reader.current(spec(CONSUMER)).map_err(err)?, key.as_byte_slice()).map_err(err)? else {
        return Ok(None);
    };
    let row =
        lookup::<FactsV9>(s, CONSUMER, None, key).map_err(err)?.ok_or_else(|| err(bad("consumer fact reference disappeared")))?;
    let expected = super::palw_claim_local_source_admission_v8::manifest_reference_v9(s, consumer, point)?
        .ok_or_else(|| err(bad("consumer facts lost actual source manifest")))?;
    if row.source_manifest != expected || row.stored != BirthV9::point(point) {
        return Err(err(bad("consumer fact immutable source manifest differs")));
    }
    if !visit_consumer_facts_v9(s, consumer, point, |_, _| Ok(()))? {
        return Err(err(bad("consumer fact generation missing")));
    }
    Ok(Some((value, key)))
}

/// Summary/UTXO standing keeps its OWN exact immutable inventory. This check
/// never retires/compacts that standing or infers its economics from source H.
pub(super) fn verify_independent_source_inventory_v19(
    s: &PalwChainStateV2,
    source: Hash64,
    point: PalwBlockContextV2,
    txid: Hash64,
) -> Result<bool, PalwStateV2Error> {
    let id = summary_id(txid, source, point).map_err(err)?;
    let Some(row) = summary(s, id).map_err(err)? else { return Ok(false) };
    if (row.source, row.point, row.txid) != (source, point, txid) {
        return Err(err(bad("independent native source summary identity")));
    }
    let raw = encode(&*row).map_err(err)?;
    verify_summary_inventory(s, &row, value(&raw)).map_err(err)?;
    Ok(true)
}

/// Exact actual winner-row event coverage. Fresh Source8 mint owns its normal
/// winner; deletion keeps only the already authenticated exact Summary. A
/// raw-ID/txid coincidence, ambiguous birth or absent summary remains missing.
pub(super) fn covers_native_winner_event_v21(
    s: &PalwChainStateV2,
    point: PalwBlockContextV2,
    entry: &PalwDeltaEntryV2,
) -> Result<bool, PalwStateV2Error> {
    let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else { return Ok(false) };
    if *table != crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1 {
        return Ok(false);
    }
    let id = Hash64::from_bytes(key.as_slice().try_into().map_err(|_| err(bad("winner coverage native claim key")))?);
    let fresh_admission =
        if new.is_some() { super::palw_claim_local_source_admission_v8::for_accepting_point_v8(s, id, point)? } else { None };
    if let Some(admission) = &fresh_admission
        && !matches!(admission.original_claim().source, PalwClaimSourceV2::FreePrompt { .. })
    {
        return Err(err(bad("actual winning carrier belongs to a non-FP native source")));
    }
    if new.is_some() && fresh_admission.is_none() {
        return Ok(false);
    } // historical update, no guessed birth
    if old.is_none() && new.is_none() {
        return Err(err(bad("winner coverage empty mutation")));
    }
    for (raw, fresh) in old.iter().map(|v| (v, false)).chain(new.iter().map(|v| (v, true))) {
        let txid = Hash64::from_bytes(raw.as_slice().try_into().map_err(|_| err(bad("winner coverage native transaction key")))?);
        let Some(reverse) = lookup::<ReverseV9>(s, REVERSE, None, txid).map_err(err)? else {
            if fresh {
                return Err(err(bad("fresh native winner lost its actual summary reverse")));
            }
            return Ok(false);
        };
        if reverse.version != 9 || reverse.txid != txid {
            return Err(err(bad("winner coverage exact reverse scope")));
        }
        let mut found = None;
        for owner in &reverse.owners {
            let source = view(s, *owner).map_err(err)?;
            if source.record.txid != txid {
                return Err(err(bad("winner coverage summary transaction differs")));
            }
            if source.record.source != id {
                continue;
            }
            if found.is_some_and(|p| p != source.record.point) {
                return Ok(false);
            }
            if fresh {
                let admission = fresh_admission.as_ref().expect("new winner admission checked above");
                let Some(original) = source.record.original else { return Ok(false) };
                let original = body::<PalwClaimStateV2>(s, original).map_err(err)?;
                if source.record.point != point || &*original != admission.original_claim() {
                    return Err(err(bad("fresh winner summary substituted actual native birth")));
                }
            }
            found = Some(source.record.point);
        }
        if found.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}
