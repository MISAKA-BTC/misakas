//! Isolated candidate for state_v2 child registration. NO live activation.
//! Exact executing financial metadata on the existing scoped N/B/I/A store.
//! Not a whole world, credit assertion, or model-computation proof.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::{PALW_CLAIM_LOCAL_TABLE_V1, prefix_v1::PalwPrefixExportReservationV1 as Export};
use crate::palw_native_resource_v1::{NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1 as Liability;
use borsh::{BorshDeserialize, BorshSerialize};
const RECORD: u32 = 1;
const SLOT: u32 = 2;
const LIABILITY: u32 = 3;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 5, table, schema: 4, key_bytes: 64, value_bytes: u64::MAX }
}
fn bad(s: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(s)
}
fn map(e: PalwStateV2Error) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(r) => ContextErrorV2::Resource(r),
        _ => bad("financial archive native writer refused"),
    }
}
pub(super) fn state_error(e: ContextErrorV2) -> PalwStateV2Error {
    super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("financial archive typed body codec"),
    })
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("financial encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out).map_err(|_| bad("financial body encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn row_value(raw: &[u8]) -> RowValueV2 {
    RowValueV2 { data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw), bytes: raw.len() as u64 }
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn native<'a>(state: &'a PalwChainStateV2, key: &[u8]) -> Result<Option<&'a [u8]>, ContextErrorV2> {
    let Some(route) = &state.kernel_route else {
        return Ok(None);
    };
    let _lease = reserve_native_requests_v1(key.len() as u64).map_err(ContextErrorV2::Resource)?;
    Ok(route.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).map(Vec::as_slice))
}
fn lookup(state: &PalwChainStateV2, table: u32, key: Hash64) -> Result<Option<RowValueV2>, ContextErrorV2> {
    let r = StateNodesV2(state);
    nodes::lookup(&r, r.current(spec(table))?, key.as_byte_slice())
}
fn publish(b: &mut TransitionBuilder<'_>, table: u32, key: Hash64, raw: Option<&[u8]>) -> Result<Option<RowValueV2>, ContextErrorV2> {
    let r = StateNodesV2(&b.state);
    let root = r.current(spec(table))?;
    let old = nodes::lookup(&r, root, key.as_byte_slice())?;
    let value = raw.map(row_value);
    let plan = nodes::update(&r, root, key.as_byte_slice(), old, value)?;
    store::publish_component_v2(b, &plan, raw)?;
    Ok(value)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) enum AttachmentV4 {
    NeverMinted,
    Attached,
    DetachedByReplacement,
    DeletedByNativeExpiry,
}
impl NativeWireScanV1 for AttachmentV4 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0..=3 => Ok(Default::default()),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "financial attachment tag")),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct RecordV4 {
    version: u16,
    budget: Hash64,
    source: Hash64,
    birth: PalwBlockContextV2,
    producer: PalwBondKeyV2,
    class: Hash64,
    execution: Hash64,
    output: Hash64,
    network: [u8; 32],
    forks: [u8; 32],
    terminal_g: Option<u128>,
    liability: Option<RowValueV2>,
    attachment: AttachmentV4,
    consumed: Option<(u64, PalwVoidReasonV2)>,
}
crate::palw_native_resource_v1::wire::wire_struct!(RecordV4;u16,Hash64,Hash64,PalwBlockContextV2,PalwBondKeyV2,Hash64,Hash64,Hash64,[u8;32],[u8;32],Option<u128>,Option<RowValueV2>,AttachmentV4,Option<(u64,PalwVoidReasonV2)>);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct SlotOwnerV4 {
    version: u16,
    budget: Hash64,
    source: Hash64,
    birth: PalwBlockContextV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(SlotOwnerV4;u16,Hash64,Hash64,PalwBlockContextV2);
/// A4 differs from both storage-only A2 and paid-source A3. Its record/body
/// inventory is fixed. No historical global state or population is copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PrefixFinancialArchiveV4 {
    pub version: u16,
    pub budget: Hash64,
    pub record: RowValueV2,
    pub liability: Option<RowValueV2>,
    pub literals: [RowValueV2; 2],
}
crate::palw_native_resource_v1::wire::wire_struct!(PrefixFinancialArchiveV4;u16,Hash64,RowValueV2,Option<RowValueV2>,[RowValueV2;2]);
fn archive(budget: Hash64, record: RowValueV2, liability: Option<RowValueV2>) -> PrefixFinancialArchiveV4 {
    PrefixFinancialArchiveV4 { version: 4, budget, record, liability, literals: [record, liability.unwrap_or(record)] }
}
fn archive_id(a: &PrefixFinancialArchiveV4) -> Result<Hash64, ContextErrorV2> {
    let raw = encode(a)?;
    Ok(crate::palw_claim_local_v1::palw_liability_material_id_v1(&raw))
}
fn record(state: &PalwChainStateV2, budget: Hash64) -> Result<Option<NativeRetainedV1<RecordV4>>, ContextErrorV2> {
    let Some(value) = lookup(state, RECORD, budget)? else {
        return Ok(None);
    };
    let raw = StateNodesV2(state).body(value)?;
    let r = decode::<RecordV4>(&raw)?;
    if r.version != 4
        || r.budget != budget
        || r.budget != super::palw_claim_local_prefix_reservation_v1::reservation_id(&r.birth, r.source)
        || r.network == [0; 32]
        || r.forks == [0; 32]
    {
        return Err(bad("financial exact birth/rules/budget"));
    }
    let a = archive(budget, value, r.liability);
    let id = archive_id(&a)?;
    let stored = native(state, &akey(id))?.ok_or(ContextErrorV2::Unavailable("financial A4 inventory missing"))?;
    crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&a, stored)
        .map_err(|_| bad("financial A4 inventory differs"))?;
    Ok(Some(r))
}
fn owner(state: &PalwChainStateV2, source: Hash64) -> Result<Option<SlotOwnerV4>, ContextErrorV2> {
    let Some(value) = lookup(state, SLOT, source)? else {
        return Ok(None);
    };
    let raw = StateNodesV2(state).body(value)?;
    let p = decode::<SlotOwnerV4>(&raw)?;
    if p.version != 4 || p.source != source {
        return Err(bad("financial main slot pointer"));
    }
    Ok(Some(*p))
}
fn put_record(b: &mut TransitionBuilder<'_>, r: RecordV4) -> Result<(), ContextErrorV2> {
    let old = lookup(&b.state, RECORD, r.budget)?;
    let old_record = record(&b.state, r.budget)?;
    if old_record.as_deref() == Some(&r) {
        return Ok(());
    }
    let raw = encode(&r)?;
    let value = row_value(&raw);
    let next = archive(r.budget, value, r.liability);
    let id = archive_id(&next)?;
    let key = akey(id);
    // The strict native GC writer permits a new A4 pin only after its B body
    // physically exists. Publish the typed body/current root first, then pin
    // this generation and finally unpin the old one. Every caller is inside
    // atomic_v4: a later collision/resource refusal restores all these writes.
    publish(b, RECORD, r.budget, Some(&raw))?;
    if let Some(existing) = native(&b.state, &key)? {
        crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&next, existing)
            .map_err(|_| bad("financial A4 immutable collision"))?;
    } else {
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&next)).map_err(map)?;
    }
    if let (Some(old), Some(previous)) = (old, old_record) {
        let old_id = archive_id(&archive(r.budget, old, previous.liability))?;
        if old_id != id {
            b.write_kernel_row_encoded_v1::<PrefixFinancialArchiveV4>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(old_id), None).map_err(map)?;
        }
    }
    Ok(())
}
fn put_owner(b: &mut TransitionBuilder<'_>, source: Hash64, value: Option<SlotOwnerV4>) -> Result<(), ContextErrorV2> {
    let raw = value.as_ref().map(encode).transpose()?;
    publish(b, SLOT, source, raw.as_deref().map(Vec::as_slice))?;
    Ok(())
}
fn same_claim(c: &PalwClaimStateV2, r: &RecordV4) -> bool {
    matches!(c.source, PalwClaimSourceV2::FreePrompt { .. })
        && c.bond == r.producer
        && c.class_id == r.class
        && c.execution_root == r.execution
        && c.output_root == r.output
        && (c.accepted_block, c.accepted_daa, c.accepted_blue_score) == (r.birth.block, r.birth.daa_score, r.birth.blue_score)
}
fn same_liability(l: &Liability, r: &RecordV4) -> bool {
    l.claim_id == r.source
        && l.executor_bond == r.producer
        && l.class_id == r.class
        && l.execution_root == r.execution
        && l.output_root == r.output
}

/// Opaque borrowed view. The callback cannot retain a partial chain state or
/// manufacture one: real native leases pin both exact immutable typed values.
pub(super) struct FinancialViewV4<'a> {
    record: &'a RecordV4,
    liability: Option<&'a Liability>,
}
impl FinancialViewV4<'_> {
    pub(super) fn birth(&self) -> PalwBlockContextV2 {
        self.record.birth
    }
    pub(super) fn producer(&self) -> PalwBondKeyV2 {
        self.record.producer
    }
    pub(super) fn terminal_g(&self) -> Option<u128> {
        self.record.terminal_g
    }
    pub(super) fn liability(&self) -> Option<&Liability> {
        self.liability
    }
    pub(super) fn attachment(&self) -> AttachmentV4 {
        self.record.attachment
    }
    pub(super) fn consumed(&self) -> Option<(u64, PalwVoidReasonV2)> {
        self.record.consumed
    }
}
pub(super) fn with_financial_view_v4<T>(
    state: &PalwChainStateV2,
    e: &Export,
    read: impl FnOnce(FinancialViewV4<'_>) -> Result<T, PalwStateV2Error>,
) -> Result<Option<T>, PalwStateV2Error> {
    let Some(r) = record(state, e.budget_id).map_err(state_error)? else {
        return Ok(None);
    };
    if r.source != e.source_claim || r.birth != e.point || r.producer != e.bond {
        return Err(state_error(bad("financial view substituted E cohort")));
    }
    let liability = r
        .liability
        .map(|v| {
            let raw = StateNodesV2(state).body(v)?;
            decode::<Liability>(&raw)
        })
        .transpose()
        .map_err(state_error)?;
    if liability.as_ref().is_some_and(|l| !same_liability(l, &r)) {
        return Err(state_error(bad("financial view liability identity")));
    }
    match r.attachment {
        AttachmentV4::Attached => {
            let p =
                owner(state, r.source).map_err(state_error)?.ok_or_else(|| state_error(bad("attached financial pointer missing")))?;
            if p.budget != r.budget || p.birth != r.birth || p.source != r.source || liability.is_none() {
                return Err(state_error(bad("attached financial owner differs")));
            }
            let main =
                state.panel_liabilities.get(&r.source).ok_or_else(|| state_error(bad("attached financial native row missing")))?;
            if liability.as_deref() != Some(main) {
                return Err(state_error(bad("attached financial native row disagrees")));
            }
        }
        AttachmentV4::NeverMinted => {
            if liability.is_some() {
                return Err(state_error(bad("never-minted financial liability")));
            }
        }
        AttachmentV4::DetachedByReplacement | AttachmentV4::DeletedByNativeExpiry => {
            if owner(state, r.source).map_err(state_error)?.is_some_and(|p| p.budget == r.budget) {
                return Err(state_error(bad("detached financial still owns main slot")));
            }
        }
    }
    read(FinancialViewV4 { record: &r, liability: liability.as_deref() }).map(Some)
}

fn tracking(b: &TransitionBuilder<'_>) -> bool {
    // This transient point is set from the actual executing/rehearsal ctx at
    // every native entry, and restored by its private typed checkpoint.
    b.claim_local_lock_point_v6.is_some()
}
fn on_e_admission_inner_v4(b: &mut TransitionBuilder<'_>, e: &Export) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(e.point.daa_score) {
        return Ok(());
    }
    if record(&b.state, e.budget_id).map_err(state_error)?.is_some() {
        return Err(state_error(bad("duplicate financial E birth")));
    }
    let c = b.state.claims.get(&e.source_claim).ok_or_else(|| state_error(bad("financial E actual source missing")))?;
    let (network, forks) = b.extras.claim_local_rule_context_v1.ok_or_else(|| state_error(bad("financial E actual rules missing")))?;
    if network == [0; 32] || forks == [0; 32] {
        return Err(state_error(bad("financial E actual rules zero")));
    }
    let r = RecordV4 {
        version: 4,
        budget: e.budget_id,
        source: e.source_claim,
        birth: e.point,
        producer: e.bond,
        class: c.class_id,
        execution: c.execution_root,
        output: c.output_root,
        network,
        forks,
        terminal_g: None,
        liability: None,
        attachment: AttachmentV4::NeverMinted,
        consumed: None,
    };
    if !same_claim(c, &r) {
        return Err(state_error(bad("financial E actual birth differs")));
    }
    // Retain a typed copy under its own native lease before mutating the source
    // map. A source may already have real locks; E creation captures them all.
    let original_wire = encode(c).map_err(state_error)?;
    let original = decode::<PalwClaimStateV2>(&original_wire).map_err(state_error)?;
    put_record(b, r).map_err(state_error)?;
    capture_current_population_v4(b, &r, &original)
}
fn on_terminal_inner_v4(b: &mut TransitionBuilder<'_>, source: Hash64, c: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
    if !tracking(b) || !c.phase.is_terminal() || !matches!(c.source, PalwClaimSourceV2::FreePrompt { .. }) {
        return Ok(());
    }
    let birth =
        PalwBlockContextV2 { block: c.accepted_block, daa_score: c.accepted_daa, blue_score: c.accepted_blue_score, subsidy: 0 };
    // reservation_id intentionally uses only block/DAA/blue/source, not subsidy.
    let budget = super::palw_claim_local_prefix_reservation_v1::reservation_id(&birth, source);
    let Some(r) = record(&b.state, budget).map_err(state_error)? else {
        return Ok(());
    };
    if !same_claim(c, &r) {
        return Err(state_error(bad("terminal financial source differs")));
    }
    let mut r = *r;
    r.terminal_g = Some(
        palw_claim_frozen_g_res_v1(&crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(b.params), c)
            .saturating_add(u128::from(c.escrowed_reward)),
    );
    put_record(b, r).map_err(state_error)
}
/// Capture the complete actual physical population BEFORE persist_panel_liability
/// re-dates any lock. The E-owned record is the accepting authority; the stored
/// claim tuple merely checks exact native identity. No current raw-ID cohort is
/// used to reconstruct an old source population.
pub(super) fn before_native_lock_redate_v4(
    b: &mut TransitionBuilder<'_>,
    source: Hash64,
    c: &PalwClaimStateV2,
    keys: &[(PalwBondKeyV2, Hash64)],
) -> Result<(), PalwStateV2Error> {
    if b.claim_local_lock_point_v6.is_none() {
        return Ok(());
    }
    let stored =
        PalwBlockContextV2 { block: c.accepted_block, daa_score: c.accepted_daa, blue_score: c.accepted_blue_score, subsidy: 0 };
    let budget = super::palw_claim_local_prefix_reservation_v1::reservation_id(&stored, source);
    let Some(r) = record(&b.state, budget).map_err(state_error)? else {
        return Ok(());
    };
    let r = *r;
    if !same_claim(c, &r) {
        return Err(state_error(bad("physical capture actual financial birth differs")));
    }
    super::palw_claim_local_physical_lock_instances_v6::capture_population_v6(b, budget, source, r.birth, c, keys)?;
    Ok(())
}
/// Explicit actual mint. Call AFTER original no-liability return, BEFORE raw
/// writer equality suppression. Replacement preserves all old financial facts.
fn before_actual_mint_v4(b: &mut TransitionBuilder<'_>, source: Hash64, c: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
    if let Some(pointer) = owner(&b.state, source).map_err(state_error)? {
        let r = record(&b.state, pointer.budget)
            .map_err(state_error)?
            .ok_or_else(|| state_error(bad("financial mint dangling old owner")))?;
        let mut r = *r;
        if r.source != source || r.birth != pointer.birth || r.attachment != AttachmentV4::Attached {
            return Err(state_error(bad("financial mint old owner identity")));
        }
        let old = b.state.panel_liabilities.get(&source).ok_or_else(|| state_error(bad("financial mint old native row missing")))?;
        let raw = encode(old).map_err(state_error)?;
        if !same_liability(old, &r) || r.liability != Some(row_value(&raw)) {
            return Err(state_error(bad("financial mint old native row disagrees with owned body")));
        }
        r.attachment = AttachmentV4::DetachedByReplacement;
        put_record(b, r).map_err(state_error)?;
        put_owner(b, source, None).map_err(state_error)?;
    }
    let _ = c;
    Ok(())
}
fn after_actual_mint_v4(b: &mut TransitionBuilder<'_>, source: Hash64, c: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
    let birth =
        PalwBlockContextV2 { block: c.accepted_block, daa_score: c.accepted_daa, blue_score: c.accepted_blue_score, subsidy: 0 };
    let budget = super::palw_claim_local_prefix_reservation_v1::reservation_id(&birth, source);
    let Some(r) = record(&b.state, budget).map_err(state_error)? else {
        return Ok(());
    };
    if !same_claim(c, &r) {
        return Err(state_error(bad("financial mint new source identity")));
    }
    let mut r = *r;
    let l = b.state.panel_liabilities.get(&source).ok_or_else(|| state_error(bad("actual financial mint missing native row")))?;
    if !same_liability(l, &r) {
        return Err(state_error(bad("actual financial mint row subject")));
    }
    let raw = encode(l).map_err(state_error)?;
    r.liability = publish(b, LIABILITY, budget, Some(&raw)).map_err(state_error)?;
    r.attachment = AttachmentV4::Attached;
    put_record(b, r).map_err(state_error)?;
    put_owner(b, source, Some(SlotOwnerV4 { version: 4, budget, source, birth: r.birth })).map_err(state_error)
}
/// Only a genuine low-level write of the currently attached row arrives here;
/// actual new mints detached their old owner before invoking this writer.
fn before_generic_panel_write_inner_v4(
    b: &mut TransitionBuilder<'_>,
    source: Hash64,
    new: Option<&Liability>,
) -> Result<(), PalwStateV2Error> {
    if !tracking(b) {
        return Ok(());
    }
    let Some(pointer) = owner(&b.state, source).map_err(state_error)? else {
        return Ok(());
    };
    let r =
        record(&b.state, pointer.budget).map_err(state_error)?.ok_or_else(|| state_error(bad("generic financial owner missing")))?;
    let mut r = *r;
    let old = b.state.panel_liabilities.get(&source).ok_or_else(|| state_error(bad("generic financial native old missing")))?;
    let raw = encode(old).map_err(state_error)?;
    if r.attachment != AttachmentV4::Attached
        || r.birth != pointer.birth
        || r.liability != Some(row_value(&raw))
        || !same_liability(old, &r)
    {
        return Err(state_error(bad("generic financial old owner changed")));
    }
    if new == Some(old) {
        return Ok(());
    }
    if let Some(next) = new {
        if !same_liability(next, &r) {
            return Err(state_error(bad("generic panel attempted cohort transfer")));
        }
        let raw = encode(next).map_err(state_error)?;
        r.liability = publish(b, LIABILITY, r.budget, Some(&raw)).map_err(state_error)?;
    } else {
        return Err(state_error(bad("owned main deletion requires actual native expiry hook")));
    }
    put_record(b, r).map_err(state_error)
}
/// Consume only this born source's financial once state. Detached source never
/// touches the new cohort's native row. Caller still applies prescribed AG2.
fn consume_once_inner_v4(
    b: &mut TransitionBuilder<'_>,
    e: &Export,
    daa: u64,
    reason: PalwVoidReasonV2,
) -> Result<bool, PalwStateV2Error> {
    let Some(r) = record(&b.state, e.budget_id).map_err(state_error)? else {
        return Ok(false);
    };
    if r.source != e.source_claim || r.birth != e.point || r.producer != e.bond {
        return Err(state_error(bad("financial conviction wrong source")));
    }
    if r.consumed.is_some() {
        return Ok(false);
    }
    if r.attachment == AttachmentV4::Attached {
        let pointer = owner(&b.state, r.source)
            .map_err(state_error)?
            .ok_or_else(|| state_error(bad("financial conviction attached owner absent")))?;
        if pointer.budget != r.budget || pointer.birth != r.birth {
            return Err(state_error(bad("financial conviction attached owner substituted")));
        }
        let value = r.liability.ok_or_else(|| state_error(bad("financial conviction native body absent")))?;
        let raw = StateNodesV2(&b.state).body(value).map_err(state_error)?;
        let l = decode::<Liability>(&raw).map_err(state_error)?;
        if b.state.panel_liabilities.get(&r.source) != Some(&*l) {
            return Err(state_error(bad("financial conviction native row changed")));
        }
        let (mut marked, _lease) = l.into_parts_v1();
        if marked.voided_daa.is_none() && marked.void_reason.is_none() {
            marked.voided_daa = Some(daa);
            marked.void_reason = Some(reason);
            b.write_panel_liability(r.source, Some(marked))?;
        }
    }
    let mut r =
        *record(&b.state, e.budget_id).map_err(state_error)?.ok_or_else(|| state_error(bad("financial conviction record lost")))?;
    r.consumed = Some((daa, reason));
    put_record(b, r).map_err(state_error)?;
    Ok(true)
}
/// ONLY call from the actual E collector after all existing release checks.
fn collect_e_inner_v4(b: &mut TransitionBuilder<'_>, e: &Export) -> Result<(), PalwStateV2Error> {
    let Some(r) = record(&b.state, e.budget_id).map_err(state_error)? else {
        return Ok(());
    };
    if r.source != e.source_claim || r.birth != e.point || r.producer != e.bond {
        return Err(state_error(bad("financial collection wrong birth")));
    }
    if owner(&b.state, r.source).map_err(state_error)?.is_some_and(|p| p.budget == r.budget) {
        put_owner(b, r.source, None).map_err(state_error)?;
    }
    super::palw_claim_local_physical_lock_instances_v6::release_population_v6(b, e.budget_id)?;
    let old = lookup(&b.state, RECORD, r.budget)
        .map_err(state_error)?
        .ok_or_else(|| state_error(bad("financial collection record pointer absent")))?;
    let id = archive_id(&archive(r.budget, old, r.liability)).map_err(state_error)?;
    publish(b, RECORD, r.budget, None).map_err(state_error)?;
    publish(b, LIABILITY, r.budget, None).map_err(state_error)?;
    b.write_kernel_row_encoded_v1::<PrefixFinancialArchiveV4>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(id), None)?;
    Ok(())
}

/// Restore only this metadata adapter's aux writes and its actual panel write.
/// No cloned world or allocation is needed; journal-owned old buffers move back.
fn rollback_v4(b: &mut TransitionBuilder<'_>, first: usize, gc_point: u64) -> Result<(), PalwStateV2Error> {
    super::palw_claim_local_context_gc_v3::restore_cache_to_v3(b, gc_point);
    super::palw_claim_local_legacy_terminal_v18::rollback_cert_relay_seals_v18(b, first);
    while b.entries.len() > first {
        match b.entries.pop().expect("checked financial rollback") {
            PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } => {
                if table != PALW_CLAIM_LOCAL_TABLE_V1 || !matches!(key.first().copied(), Some(b'N' | b'B' | b'I' | b'A')) {
                    return Err(state_error(bad("financial rollback crossed owner")));
                }
                let r = b.state.kernel_route.as_mut().ok_or_else(|| state_error(bad("financial rollback route missing")))?;
                let k = (table, key);
                if r.aux.get(&k) != new.as_ref() {
                    return Err(state_error(bad("financial rollback current row differs")));
                }
                match old {
                    Some(v) => {
                        r.aux.insert(k, v);
                    }
                    None => {
                        r.aux.remove(&k);
                    }
                }
            }
            PalwDeltaEntryV2::PanelLiability { key, old, new } => {
                if b.state.panel_liabilities.get(&key) != new.as_ref() {
                    return Err(state_error(bad("financial rollback native row differs")));
                }
                match old {
                    Some(v) => {
                        b.state.panel_liabilities.insert(key, v);
                    }
                    None => {
                        b.state.panel_liabilities.remove(&key);
                    }
                }
            }
            _ => {
                return Err(state_error(bad("financial rollback crossed another native owner")));
            }
        }
    }
    Ok(())
}
fn atomic_v4<T>(
    b: &mut TransitionBuilder<'_>,
    f: impl FnOnce(&mut TransitionBuilder<'_>) -> Result<T, PalwStateV2Error>,
) -> Result<T, PalwStateV2Error> {
    let first = b.entries.len();
    let gc_point = b.claim_local_gc_sequence_v3;
    match f(b) {
        Ok(v) => Ok(v),
        Err(e) => {
            rollback_v4(b, first, gc_point)?;
            Err(e)
        }
    }
}
/// One actual mint, including equal serialized values and non-Prefix mints.
/// Original low-level writer remains authoritative for the native map itself.
pub(super) fn actual_mint_v4(
    b: &mut TransitionBuilder<'_>,
    source: Hash64,
    c: &PalwClaimStateV2,
    minted: Liability,
    daa: u64,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(daa) {
        return b.write_panel_liability(source, Some(minted));
    }
    atomic_v4(b, |b| {
        before_actual_mint_v4(b, source, c)?;
        b.write_panel_liability(source, Some(minted))?;
        after_actual_mint_v4(b, source, c)
    })
}
/// Invoked only inside the real sweep's verified pruned-liability loop after
/// all own locks passed their two-clock/duty predicates. Raw replacement never
/// calls this. Evidence stays rooted until E's actual material collection.
pub(super) fn actual_native_expiry_v4(b: &mut TransitionBuilder<'_>, source: Hash64) -> Result<(), PalwStateV2Error> {
    if !tracking(b) {
        return Ok(());
    }
    atomic_v4(b, |b| {
        let Some(p) = owner(&b.state, source).map_err(state_error)? else {
            return Ok(());
        };
        let mut r =
            *record(&b.state, p.budget).map_err(state_error)?.ok_or_else(|| state_error(bad("expiry financial owner lost")))?;
        if r.attachment != AttachmentV4::Attached || r.birth != p.birth {
            return Err(state_error(bad("expiry financial cohort differs")));
        }
        r.attachment = AttachmentV4::DeletedByNativeExpiry;
        put_record(b, r).map_err(state_error)?;
        put_owner(b, source, None).map_err(state_error)
    })
}

pub(super) fn consume_once_v4(
    b: &mut TransitionBuilder<'_>,
    e: &Export,
    daa: u64,
    reason: PalwVoidReasonV2,
) -> Result<bool, PalwStateV2Error> {
    atomic_v4(b, |b| consume_once_inner_v4(b, e, daa, reason))
}
/// Exact A4/current-index/E consistency. A4 versions superseded by an update
/// are removed atomically; its old native delta is the reorg restoration owner.
pub(super) fn verify_archive_v4(state: &PalwChainStateV2, a: &PrefixFinancialArchiveV4) -> Result<(), ContextErrorV2> {
    if a.version != 4
        || a.literals != [a.record, a.liability.unwrap_or(a.record)]
        || lookup(state, RECORD, a.budget)? != Some(a.record)
    {
        return Err(bad("financial A4/current record inventory"));
    }
    let r = record(state, a.budget)?.ok_or(ContextErrorV2::Unavailable("financial A4 current record missing"))?;
    if r.liability != a.liability || lookup(state, LIABILITY, a.budget)? != r.liability {
        return Err(bad("financial current liability/root binding"));
    }
    if super::palw_claim_local_physical_lock_instances_v6::population_matches_source_v6(state, r.budget, r.source, r.birth)?
        != Some(true)
    {
        return Err(bad("financial A4 exact physical population birth absent or substituted"));
    }
    let ekey = super::palw_claim_local_prefix_reservation_v1::row_key(a.budget);
    let raw = native(state, &ekey)?.ok_or(ContextErrorV2::Unavailable("financial A4 original E missing"))?;
    let e = decode::<Export>(raw)?;
    with_financial_view_v4(state, &e, |_| Ok(()))
        .map_err(map)?
        .ok_or(ContextErrorV2::Unavailable("financial A4 original view missing"))?;
    Ok(())
}
pub(super) fn validate_indices_v4(state: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    // Every owned namespace root must use its exact allocated native profile,
    // including otherwise orphan roots not reached by an expected directory.
    if let Some(route) = &state.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&store::CONTEXT_CURRENT_PREFIX_V2) {
                continue;
            }
            let rooted = decode::<nodes::ComponentRootV2>(raw)?;
            if rooted.spec.owner == 5
                && (key.len() != 18 || !matches!(rooted.spec.table, 1..=3) || rooted.spec != spec(rooted.spec.table))
            {
                return Err(bad("source index root has unallocated native profile"));
            }
        }
    }
    let reader = StateNodesV2(state);
    nodes::complete_rows(&reader, reader.current(spec(RECORD))?, |key, value| {
        let budget = Hash64::from_bytes(key.try_into().map_err(|_| bad("financial record index key"))?);
        let raw = reader.body(value)?;
        let r = decode::<RecordV4>(&raw)?;
        if r.budget != budget {
            return Err(bad("financial record index identity"));
        }
        verify_archive_v4(state, &archive(budget, value, r.liability))
    })?;
    nodes::complete_rows(&reader, reader.current(spec(SLOT))?, |key, value| {
        let source = Hash64::from_bytes(key.try_into().map_err(|_| bad("financial slot index key"))?);
        let raw = reader.body(value)?;
        let p = decode::<SlotOwnerV4>(&raw)?;
        let r = record(state, p.budget)?.ok_or(ContextErrorV2::Unavailable("financial slot owner missing"))?;
        if p.version != 4 || p.source != source || p.birth != r.birth || r.source != source || r.attachment != AttachmentV4::Attached {
            return Err(bad("financial reverse slot ownership"));
        }
        Ok(())
    })?;
    Ok(())
}

/// Full configured rules authenticate the context independently of the record.
pub(super) fn validate_network_context_v4(
    state: &PalwChainStateV2,
    global: &crate::config::params::Params,
) -> Result<(), PalwStateV2Error> {
    let reader = StateNodesV2(state);
    nodes::complete_rows(&reader, reader.current(spec(RECORD)).map_err(state_error)?, |_, value| {
        let raw = reader.body(value)?;
        let r = decode::<RecordV4>(&raw)?;
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &global.palw_consensus_mode else {
            return Err(bad("financial archive requires configured V2 network"));
        };
        let fired = crate::palw_onboarding_v1::palw_envelope_fork_digest_v1(
            global.genesis.hash,
            &global.fence_schedule_v1(),
            r.birth.daa_score,
        )
        .as_bytes();
        if r.network != global.consensus_identity_id().as_bytes()
            || r.forks != fired
            || !bundle.state.claim_local_settlement_active_at(r.birth.daa_score)
        {
            return Err(bad("financial archive differs from configured network/fence context"));
        }
        Ok(())
    })
    .map_err(state_error)
}

fn capture_current_population_v4(b: &mut TransitionBuilder<'_>, r: &RecordV4, c: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
    let count = b.state.slashable_locks.keys().filter(|(_, source)| *source == r.source).count();
    let mut quote = TirNativeRequestsV1::new();
    quote.exact_v1::<(PalwBondKeyV2, Hash64)>(count as u64).map_err(ContextErrorV2::Resource).map_err(state_error)?;
    let _lease = reserve_native_requests_v1(quote.bytes_v1()).map_err(ContextErrorV2::Resource).map_err(state_error)?;
    let mut keys = Vec::with_capacity(count);
    for key in b.state.slashable_locks.keys().filter(|(_, source)| *source == r.source) {
        keys.push(*key);
    }
    super::palw_claim_local_physical_lock_instances_v6::capture_population_v6(b, r.budget, r.source, r.birth, c, &keys)?;
    Ok(())
}
/// The stored tuple is a locator into already authenticated E-owned records,
/// never a reconstructed accepting point for an unrelated/current raw cohort.
fn actual_record_v4(b: &TransitionBuilder<'_>, source: Hash64, c: &PalwClaimStateV2) -> Result<Option<RecordV4>, PalwStateV2Error> {
    if b.claim_local_lock_point_v6.is_none() {
        return Ok(None);
    }
    let stored =
        PalwBlockContextV2 { block: c.accepted_block, daa_score: c.accepted_daa, blue_score: c.accepted_blue_score, subsidy: 0 };
    let budget = super::palw_claim_local_prefix_reservation_v1::reservation_id(&stored, source);
    let Some(r) = record(&b.state, budget).map_err(state_error)? else {
        return Ok(None);
    };
    if !same_claim(c, &r) {
        return Err(state_error(bad("actual physical reference changed financial source birth")));
    }
    Ok(Some(*r))
}
/// Q4 uses an already-existing physical slot without minting or transferring it.
pub(super) fn on_actual_lock_reference_v4(
    b: &mut TransitionBuilder<'_>,
    key: (PalwBondKeyV2, Hash64),
    c: &PalwClaimStateV2,
) -> Result<(), PalwStateV2Error> {
    let Some(r) = actual_record_v4(b, key.1, c)? else {
        return Ok(());
    };
    super::palw_claim_local_physical_lock_instances_v6::reference_current_v6(b, r.budget, key, c)
}
/// Runs after the new physical instance is staged, BEFORE native mutation.
/// Any failure therefore remains within the V6 aux-only rollback transaction.
pub(super) fn reference_actual_mint_v4(
    b: &mut TransitionBuilder<'_>,
    key: (PalwBondKeyV2, Hash64),
    c: &PalwClaimStateV2,
    lock: crate::palw_panel_var_v1::PalwSlashableLockV1,
) -> Result<(), PalwStateV2Error> {
    let Some(r) = actual_record_v4(b, key.1, c)? else {
        return Ok(());
    };
    super::palw_claim_local_physical_lock_instances_v6::reference_minted_v6(b, r.budget, key, c, lock)
}

pub(super) fn on_e_admission_v4(b: &mut TransitionBuilder<'_>, e: &Export) -> Result<(), PalwStateV2Error> {
    atomic_v4(b, |b| on_e_admission_inner_v4(b, e))
}
pub(super) fn on_terminal_v4(b: &mut TransitionBuilder<'_>, source: Hash64, c: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
    atomic_v4(b, |b| {
        let first = b.entries.len();
        on_terminal_inner_v4(b, source, c)?;
        // The original Claim writer can still append Claim and its paired
        // DefaultDisputed exposure adjustment after this metadata callback.
        if b.entries.len() != first {
            b.reserve_mutation_sequence_v1(0, 2)?;
        }
        Ok(())
    })
}
pub(super) fn collect_e_v4(b: &mut TransitionBuilder<'_>, e: &Export) -> Result<(), PalwStateV2Error> {
    atomic_v4(b, |b| collect_e_inner_v4(b, e))
}
pub(super) fn before_generic_panel_write_v4(
    b: &mut TransitionBuilder<'_>,
    source: Hash64,
    new: Option<&Liability>,
) -> Result<(), PalwStateV2Error> {
    if !tracking(b) || owner(&b.state, source).map_err(state_error)?.is_none() {
        return Ok(());
    }
    let changed = b.state.panel_liabilities.get(&source) != new;
    atomic_v4(b, |b| {
        before_generic_panel_write_inner_v4(b, source, new)?;
        // Metadata appended after the original native admission can consume its
        // journal slot. Re-admit the actual append before native mutation.
        if changed {
            b.reserve_mutation_sequence_v1(0, 1)?;
        }
        Ok(())
    })
}

/// Regression at the existing real E-admission ledger fixture. This exercises
/// metadata publication/replacement and native delta/GC/cold storage mechanics;
/// it does not create a source, adjudication verdict or approved computation.
#[cfg(test)]
pub(super) fn assert_native_body_pin_replace_gc_reorg_cold_v4(b: &mut TransitionBuilder<'_>, e: &Export) {
    let checkpoint = b.checkpoint();
    let parent = b.state.clone();
    let first = b.entries.len();
    let old = lookup(&b.state, RECORD, e.budget_id).unwrap().unwrap();
    let original = *record(&b.state, e.budget_id).unwrap().unwrap();
    assert_eq!(original.source, e.source_claim);
    assert_eq!(original.birth, e.point);
    assert!(original.consumed.is_none());
    let old_archive = archive(e.budget_id, old, original.liability);
    verify_archive_v4(&b.state, &old_archive).unwrap();
    // Bookkeeping boundary only, rolled back below. No court verdict is asserted.
    assert!(consume_once_v4(b, e, e.point.daa_score, PalwVoidReasonV2::CourtFraud).unwrap());
    let next = lookup(&b.state, RECORD, e.budget_id).unwrap().unwrap();
    assert_ne!(next, old, "a replacement publishes a new immutable native body");
    let changed = *record(&b.state, e.budget_id).unwrap().unwrap();
    assert_eq!(changed.consumed, Some((e.point.daa_score, PalwVoidReasonV2::CourtFraud)));
    let next_archive = archive(e.budget_id, next, changed.liability);
    assert!(
        native(&b.state, &akey(archive_id(&old_archive).unwrap())).unwrap().is_none(),
        "replacement removes only the superseded A4 pin"
    );
    verify_archive_v4(&b.state, &next_archive).unwrap();
    assert!(store::collect_unreachable_v2(b).unwrap() > 0);
    assert!(
        native(&b.state, &store_body_key_for_test_v4(old.data)).unwrap().is_none(),
        "unreferenced old body is physically reclaimable"
    );
    assert!(StateNodesV2(&b.state).body(next).is_ok(), "current A4 body survives GC");
    // The enclosing fixture's real source was born through the full fold under
    // these SAME active rules/extras. This direct adapter stops before the
    // normal end-of-block checkpoint seam; complete it at its recorded point.
    assert_eq!(b.claim_local_lock_point_v6, Some(e.point));
    assert_eq!(b.state.last_point, Some(e.point));
    super::palw_claim_local_checkpoint_fold_v1::write_claim_local_checkpoint_v1(b, &parent, &e.point, &[], PalwBlockWorkV3::None, &[])
        .unwrap();
    let child = b.state.clone();
    let delta = PalwStateDeltaV2 { point: e.point, entries: b.entries[first..].to_vec() };
    let replay = apply_delta_v2(&parent, &delta, b.params).unwrap();
    assert_eq!(replay.state_root(), child.state_root());
    verify_archive_v4(&replay, &next_archive).unwrap();
    let reverted = revert_delta_v2(&child, &delta, b.params).unwrap();
    assert_eq!(reverted.state_root(), parent.state_root());
    verify_archive_v4(&reverted, &old_archive).unwrap();
    // Full carriage codec + the ordinary restore authority rebuild native GC.
    let wire = borsh::to_vec(&PalwStateCarriageV2::from_state(&child)).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(wire.as_slice(), wire.len() as u64).unwrap();
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).unwrap();
    let cold = carriage.into_state_v3(b.params, Some(child.state_root()), false, None).unwrap();
    verify_archive_v4(&cold, &next_archive).unwrap();
    let mut missing = cold.clone();
    assert!(
        missing
            .kernel_route
            .as_mut()
            .unwrap()
            .aux
            .remove(&(PALW_CLAIM_LOCAL_TABLE_V1, store_body_key_for_test_v4(next.data).to_vec()))
            .is_some()
    );
    assert!(
        PalwStateCarriageV2::from_state(&missing).into_state_v3(b.params, Some(missing.state_root()), false, None).is_err(),
        "missing immutable financial body cannot restore despite a recomputed global root"
    );
    b.restore(checkpoint);
    assert_eq!(b.state.state_root(), parent.state_root());
    verify_archive_v4(&b.state, &old_archive).unwrap();
    assert!(record(&b.state, e.budget_id).unwrap().unwrap().consumed.is_none());
}
#[cfg(test)]
fn store_body_key_for_test_v4(id: Hash64) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = store::CONTEXT_BODY_PREFIX_V2;
    key[1..].copy_from_slice(id.as_byte_slice());
    key
}
