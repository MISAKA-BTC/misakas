//! ISOLATED state_v2 child candidate; not registered or activated.
//! Actual material-income receipt lineages; not claim-admission authority.
//! A source population references shared physical instances, never raw slots.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{
    self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2,
};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_resource_v1::{
    NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
const WORK: u32 = 1;
const DUTY: u32 = 2;
const MEMBER: u32 = 3;
const LINEAGE: u32 = 4;
const LINEAGE_DUTY: u32 = 5;
const OWNER_WORK: u32 = 6;
const OWNER: u32 = 7;
const WORK_DUTY: u32 = 8;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 {
        owner: 11,
        table,
        schema: 9,
        key_bytes: 64,
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
            .key(b"misaka-palw/kernel-contract-duty/v9")
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
    let plan = nodes::update(
        &r,
        before,
        key.as_byte_slice(),
        old,
        raw.as_ref().map(|r| value(r)),
    )?;
    match scope {
        Some(id) => {
            store::publish_owned_component_v2(b, id, &plan, raw.as_deref().map(Vec::as_slice))?
        }
        None => store::publish_component_v2(b, &plan, raw.as_deref().map(Vec::as_slice))?,
    };
    Ok(plan.after())
}

fn atomic<T>(
    b: &mut TransitionBuilder<'_>,
    f: impl FnOnce(&mut TransitionBuilder<'_>) -> Result<T, ContextErrorV2>,
) -> Result<T, ContextErrorV2> {
    let first=b.entries.len();let gc_point=b.claim_local_gc_sequence_v3;let answer=f(b);
    if answer.is_err(){super::palw_claim_local_context_gc_v3::rollback_aux_v3(b,first,gc_point,|table,key|table==PALW_CLAIM_LOCAL_TABLE_V1&&matches!(key.first().copied(),Some(b'N'|b'B'|b'I'|b'A')))?;}
    answer
}

use super::palw_claim_local_kernel_source_owner_v9::ActualKernelSourceOwnerV9 as OwnerToken;
use crate::palw_source_bootstrap_v1::{
    PalwSourceBootstrapWorkRowV1, PalwSourceBootstrapWorkV1 as Work,
};
use crate::palw_source_verifier_economics_v2::{
    PalwSourceVerifierEntitlementV2 as Entitlement, PalwSourceVerifierLiabilityV2 as Duty,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct KernelOwnerV9 {
    claim: Hash64,
    birth: PalwBlockContextV2,
    catalogue: Hash64,
}
crate::palw_native_resource_v1::wire::wire_struct!(KernelOwnerV9;Hash64,PalwBlockContextV2,Hash64);
impl KernelOwnerV9 {
    fn of(t: &OwnerToken) -> Self {
        Self {
            claim: t.claim(),
            birth: t.accepting_point(),
            catalogue: t.catalogue_id(),
        }
    }
    fn id(&self) -> Result<Hash64, ContextErrorV2> {
        hash(1, self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct WorkV9 {
    owner: KernelOwnerV9,
    work: Work,
    opened: PalwBlockContextV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(WorkV9;KernelOwnerV9,Work,PalwBlockContextV2);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct DutyV9 {
    work: Hash64,
    duty: Hash64,
    actor: PalwBondKeyV2,
    context: Hash64,
    funding: Hash64,
    income: Hash64,
    accepted_daa: u64,
    fee: u64,
    until: u64,
    linked: bool,
}
crate::palw_native_resource_v1::wire::wire_struct!(DutyV9;Hash64,Hash64,PalwBondKeyV2,Hash64,Hash64,Hash64,u64,u64,u64,bool);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct LineageV9 {
    closed: bool,
    references: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(LineageV9;bool,u64);
fn current_work(s: &PalwChainStateV2, w: &WorkV9) -> Result<(), ContextErrorV2> {
    let r = s
        .source_bootstrap_work_v1(w.work.id())
        .map_err(native_err)?
        .ok_or(bad("watched actual bootstrap work absent"))?;
    if r.work != w.work {
        return Err(bad("watched bootstrap work body changed"));
    }
    let mut key = [0; 65];
    key[0] = 2;
    key[1..].copy_from_slice(w.work.canonical_work.as_byte_slice());
    let _lease = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
    let raw = s
        .kernel_route
        .as_ref()
        .and_then(|r| {
            r.aux.get(&(
                crate::palw_source_bootstrap_v1::PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1,
                key.to_vec(),
            ))
        })
        .ok_or(bad("watched bootstrap once-canonical marker absent"))?;
    if *decode::<Hash64>(raw)? != w.work.id() {
        return Err(bad("watched bootstrap canonical owner differs"));
    }
    Ok(())
}
fn current_duty(
    s: &PalwChainStateV2,
    d: &DutyV9,
) -> Result<NativeRetainedV1<Duty>, ContextErrorV2> {
    let k = crate::palw_source_verifier_economics_v2::liability_key_v2(&d.actor, d.work);
    let _lease = reserve_native_requests_v1(k.len() as u64).map_err(ContextErrorV2::Resource)?;
    let raw = s
        .kernel_route
        .as_ref()
        .and_then(|r| {
            r.aux.get(&(
                crate::palw_source_verifier_economics_v2::SOURCE_VERIFIER_ECONOMICS_TABLE_V2,
                k.to_vec(),
            ))
        })
        .ok_or(bad("watched actual source checker duty absent"))?;
    let actual = decode::<Duty>(raw)?;
    let w =
        lookup::<WorkV9>(s, WORK, None, d.work)?.ok_or(bad("checker duty Open owner missing"))?;
    if actual.claim != w.work.claim
        || actual.id() != d.duty
        || actual.work != d.work
        || actual.actor != d.actor
        || actual.accepted_daa != d.accepted_daa
        || actual.fee != d.fee
        || actual.until_daa != d.until
        || actual.funding != d.funding
    {
        return Err(bad("watched checker contractual generation differs"));
    }
    Ok(actual)
}
/// Only the actual NEW Open branch calls this with the parent producer's opaque
/// unpaid-capable kernel birth token. Repeated native Open never rebinds owner.
pub(super) fn note_open_owner_v9(
    b: &mut TransitionBuilder<'_>,
    point: &PalwBlockContextV2,
    work: &Work,
    token: &OwnerToken,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(());
    }
    atomic(b, |b| {
        let owner = KernelOwnerV9::of(token);
        if work.claim != owner.claim || work.accepted_daa != owner.birth.daa_score {
            return Err(bad("Open owner actual kernel subject differs"));
        }
        let w = WorkV9 {
            owner,
            work: *work,
            opened: *point,
        };
        current_work(&b.state, &w)?;
        if let Some(old) = lookup::<WorkV9>(&b.state, WORK, None, work.id())? {
            if old.owner != owner || old.work != *work {
                return Err(bad("Open substituted prior actual owner"));
            }
            return Ok(());
        }
        let id = owner.id()?;
        if let Some(old) = lookup::<KernelOwnerV9>(&b.state, OWNER, None, id)? {
            if *old != owner {
                return Err(bad("kernel contract source identity collision"));
            }
        }
        put(b, OWNER, None, id, Some(&owner))?;
        put(b, WORK, None, work.id(), Some(&w))?;
        put(b, OWNER_WORK, Some(id), work.id(), Some(&work.id()))?;
        Ok(())
    })
    .map_err(err)
}
/// AFTER actual approve writes its native duty and upfront R reservation.
/// The source association is the persisted Open token, never d.claim lookup.
pub(super) fn note_checker_reserved_v9(
    b: &mut TransitionBuilder<'_>,
    point: &PalwBlockContextV2,
    duty: &Duty,
    context: Hash64,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(());
    }
    atomic(b, |b| {
        let w = lookup::<WorkV9>(&b.state, WORK, None, duty.work)?.ok_or(
            ContextErrorV2::Unavailable("checker Open actual kernel owner not captured"),
        )?;
        current_work(&b.state, &w)?;
        if duty.claim != w.work.claim
            || point.daa_score != duty.accepted_daa
            || point.daa_score > w.work.deadline_daa
        {
            return Err(bad("checker reserved outside actual work scope"));
        }
        let income =
            super::palw_bond_budget_fold_v1::source_income_id_v1(context, duty.id(), &duty.actor);
        let d = DutyV9 {
            work: duty.work,
            duty: duty.id(),
            actor: duty.actor,
            context,
            funding: duty.funding,
            income,
            accepted_daa: duty.accepted_daa,
            fee: duty.fee,
            until: duty.until_daa,
            linked: false,
        };
        if *current_duty(&b.state, &d)? != *duty {
            return Err(bad("checker native duty differs at reservation"));
        }
        let r = b
            .state
            .bond_budget
            .as_ref()
            .and_then(|r| r.claim_row(&income))
            .ok_or(bad("checker actual upfront R absent"))?;
        if r.origin != crate::palw_bond_budget_v1::PalwBudgetOriginV1::SourceFee
            || r.bond != duty.actor
            || r.accepted_daa != duty.accepted_daa
            || r.reserved.reward_sompi != duty.fee
        {
            return Err(bad("checker actual upfront R identity differs"));
        }
        if let Some(old) = lookup::<DutyV9>(&b.state, DUTY, None, d.duty)? {
            if old.work != d.work || old.income != d.income || old.accepted_daa != d.accepted_daa {
                return Err(bad("checker duty owner replaced"));
            }
            return Ok(());
        }
        put(b, DUTY, None, d.duty, Some(&d))?;
        put(b, WORK_DUTY, Some(d.work), d.duty, Some(&d.duty))?;
        Ok(())
    })
    .map_err(err)
}
/// AFTER real bind_material_income_root, BEFORE native queue transfer. Captures
/// physical lineage IDs through a typed V7 API; records no made-up payment time.
pub(super) fn note_checker_material_bound_v9(
    b: &mut TransitionBuilder<'_>,
    duty: Hash64,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let d = lookup::<DutyV9>(&b.state, DUTY, None, duty)?.ok_or(
            ContextErrorV2::Unavailable("checker upfront contract owner absent"),
        )?;
        current_duty(&b.state, &d)?;
        let w = lookup::<WorkV9>(&b.state, WORK, None, d.work)?
            .ok_or(bad("checker bound work owner missing"))?;
        let ids = super::palw_claim_local_material_income_lineage_v7::actual_pending_lineages_v9(
            b,
            d.income,
            w.owner.claim,
        )?;
        for id in ids.iter() {
            if lookup::<Hash64>(&b.state, MEMBER, Some(duty), *id)?.is_none() {
                let mut l = lookup::<LineageV9>(&b.state, LINEAGE, None, *id)?
                    .map(|l| *l)
                    .unwrap_or(LineageV9 {
                        closed: false,
                        references: 0,
                    });
                if l.closed {
                    return Err(bad("new checker material uses closed physical lineage"));
                }
                l.references = l
                    .references
                    .checked_add(1)
                    .ok_or(bad("checker lineage reference overflow"))?;
                put(b, LINEAGE, None, *id, Some(&l))?;
                put(b, MEMBER, Some(duty), *id, Some(id))?;
                put(b, LINEAGE_DUTY, Some(*id), duty, Some(&duty))?;
            }
        }
        put(b, DUTY, None, duty, Some(&DutyV9 { linked: true, ..*d }))?;
        Ok(())
    })
    .map_err(err)
}
/// V7 emits this ONLY at the END of a real native operation after all possible
/// child transfers, with actual lineage.active==0. No absence-as-closure rule.
pub(super) fn note_actual_lineage_closed_v9(
    b: &mut TransitionBuilder<'_>,
    proof: &super::palw_claim_local_material_income_lineage_v7::ActualLineageClosedV9,
) -> Result<(), PalwStateV2Error> {
    let id = proof.lineage();
    atomic(b, |b| {
        if let Some(l) = lookup::<LineageV9>(&b.state, LINEAGE, None, id)? {
            if !l.closed {
                put(
                    b,
                    LINEAGE,
                    None,
                    id,
                    Some(&LineageV9 { closed: true, ..*l }),
                )?;
            }
        }
        Ok(())
    })
    .map_err(err)
}
fn duty_pending(s: &PalwChainStateV2, d: &DutyV9) -> Result<bool, ContextErrorV2> {
    let actual = current_duty(s, d)?;
    match actual.entitlement {
        Entitlement::Held => return Ok(true),
        Entitlement::Forfeited { .. } => {}
        Entitlement::Queued { .. } | Entitlement::QueueAcknowledged { .. } => {
            if !d.linked {
                return Err(ContextErrorV2::Unavailable(
                    "checker queue lacks its actual bound physical population",
                ));
            }
        }
    }
    let r = StateNodesV2(s);
    let mut pending = false;
    if d.linked && root(s, MEMBER, Some(d.duty))?.rows == 0 {
        return Err(bad("checker linked empty physical population"));
    }
    nodes::complete_rows(&r, root(s, MEMBER, Some(d.duty))?, |k, v| {
        let raw = r.body(v)?;
        let id = decode::<Hash64>(&raw)?;
        if k != id.as_byte_slice() {
            return Err(bad("checker lineage member key differs"));
        }
        let l = lookup::<LineageV9>(s, LINEAGE, None, *id)?
            .ok_or(bad("checker observed lineage closure fact missing"))?;
        pending |= !l.closed;
        Ok(())
    })?;
    Ok(pending)
}
/// Known opened-work population only. None means no authenticated Open owner,
/// NOT false. Each work has native once-canonical marker and finite actual
/// approval deadline; unknown payments stay Pending until real native deletion.
pub(super) fn checker_pending_for_owner_v9(
    s: &PalwChainStateV2,
    token: &OwnerToken,
    now: u64,
) -> Result<Option<bool>, ContextErrorV2> {
    checker_pending_for_fields(s, KernelOwnerV9::of(token), now)
}
fn checker_pending_for_fields(
    s: &PalwChainStateV2,
    owner: KernelOwnerV9,
    now: u64,
) -> Result<Option<bool>, ContextErrorV2> {
    let oid = owner.id()?;
    let Some(saved) = lookup::<KernelOwnerV9>(s, OWNER, None, oid)? else {
        return Ok(None);
    };
    if *saved != owner {
        return Err(bad("checker source owner identity differs"));
    }
    let reader = StateNodesV2(s);
    let mut pending = false;
    let works = root(s, OWNER_WORK, Some(oid))?;
    if works.rows == 0 {
        return Err(bad("checker owner has no actual Open"));
    }
    nodes::complete_rows(&reader, works, |k, v| {
        let raw = reader.body(v)?;
        let id = decode::<Hash64>(&raw)?;
        if k != id.as_byte_slice() {
            return Err(bad("checker work membership key differs"));
        }
        let w =
            lookup::<WorkV9>(s, WORK, None, *id)?.ok_or(bad("checker owner lost opened work"))?;
        if w.owner != owner || w.work.id() != *id {
            return Err(bad("checker Open owner changed"));
        }
        current_work(s, &w)?;
        pending |= now <= w.work.deadline_daa;
        let mut native_count = 0u64;
        if let Some(route) = &s.kernel_route {
            for ((t, k), raw) in &route.aux {
                if *t
                    == crate::palw_source_verifier_economics_v2::SOURCE_VERIFIER_ECONOMICS_TABLE_V2
                    && k.len() == 133
                    && k[0] == 6
                    && &k[1..65] == id.as_byte_slice()
                {
                    let nativeid = *decode::<Hash64>(raw)?;
                    let d = lookup::<DutyV9>(s, DUTY, None, nativeid)?.ok_or(
                        ContextErrorV2::Unavailable(
                            "actual checker duty has no captured Open contract owner",
                        ),
                    )?;
                    let indexed_actor = borsh::from_slice::<PalwBondKeyV2>(&k[65..])
                        .map_err(|_| bad("checker actor index codec"))?;
                    if d.actor != indexed_actor
                        || d.work != *id
                        || lookup::<Hash64>(s, WORK_DUTY, Some(*id), nativeid)?.as_deref()
                            != Some(&nativeid)
                    {
                        return Err(bad("actual checker duty complete membership differs"));
                    }
                    native_count = native_count
                        .checked_add(1)
                        .ok_or(bad("checker native duty count overflow"))?;
                }
            }
        }
        if native_count != root(s, WORK_DUTY, Some(*id))?.rows {
            return Err(bad("checker actual duty population count differs"));
        }
        nodes::complete_rows(&reader, root(s, WORK_DUTY, Some(*id))?, |k, v| {
            let raw = reader.body(v)?;
            let did = decode::<Hash64>(&raw)?;
            if k != did.as_byte_slice() {
                return Err(bad("checker duty membership key differs"));
            }
            let d = lookup::<DutyV9>(s, DUTY, None, *did)?
                .ok_or(bad("checker opened work lost duty"))?;
            if d.work != *id {
                return Err(bad("checker duty refers to another work"));
            }
            pending |= duty_pending(s, &d)?;
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(Some(pending))
}
fn ids(
    s: &PalwChainStateV2,
    table: u32,
    scope: Hash64,
) -> Result<NativeRetainedV1<Vec<Hash64>>, ContextErrorV2> {
    let r = root(s, table, Some(scope))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Hash64>(r.rows)
        .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(
        usize::try_from(r.rows).map_err(|_| bad("checker population capacity"))?,
    );
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, r, |k, v| {
        let raw = reader.body(v)?;
        let id = decode::<Hash64>(&raw)?;
        if k != id.as_byte_slice() {
            return Err(bad("checker population key/value differs"));
        }
        out.push(*id);
        Ok(())
    })?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn drop_empty_owned(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Hash64,
) -> Result<(), ContextErrorV2> {
    let r = root(&b.state, table, Some(scope))?;
    if r.rows != 0 || r.node.is_some() {
        return Err(bad("checker current directory still live"));
    }
    let mut k = [0; 84];
    k[0] = b'I';
    k[1..3].copy_from_slice(&2u16.to_le_bytes());
    k[3..67].copy_from_slice(scope.as_byte_slice());
    spec(table)
        .serialize(&mut &mut k[67..])
        .map_err(|_| bad("checker current directory codec"))?;
    let _lease = reserve_native_requests_v1(84).map_err(ContextErrorV2::Resource)?;
    b.write_kernel_row(PALW_CLAIM_LOCAL_TABLE_V1, k.to_vec(), None)
        .map_err(native_err)
}
/// Metadata GC after real creation deadline AND rights/physical lineage closure.
/// Native source/canonical/duty/payment records remain owned by their old paths.
pub(super) fn drop_closed_checker_owner_v9(
    b: &mut TransitionBuilder<'_>,
    token: &OwnerToken,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    drop_closed_checker_fields(b, KernelOwnerV9::of(token), now)
}
/// Reads already captured old owner identity; never remints from current raw slot.
pub(super) fn drop_closed_checker_at_known_birth_v9(
    b: &mut TransitionBuilder<'_>,
    claim: Hash64,
    birth: PalwBlockContextV2,
    catalogue: Hash64,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    drop_closed_checker_fields(
        b,
        KernelOwnerV9 {
            claim,
            birth,
            catalogue,
        },
        now,
    )
}
fn drop_closed_checker_fields(
    b: &mut TransitionBuilder<'_>,
    owner: KernelOwnerV9,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        if checker_pending_for_fields(&b.state, owner, now)? != Some(false) {
            return Err(ContextErrorV2::Unavailable(
                "checker source population not actually closed",
            ));
        }
        let oid = owner.id()?;
        let works = ids(&b.state, OWNER_WORK, oid)?;
        for work in works.iter() {
            let duties = ids(&b.state, WORK_DUTY, *work)?;
            for duty in duties.iter() {
                let lineages = ids(&b.state, MEMBER, *duty)?;
                for id in lineages.iter() {
                    let l = lookup::<LineageV9>(&b.state, LINEAGE, None, *id)?
                        .ok_or(bad("checker GC lineage absent"))?;
                    if !l.closed {
                        return Err(bad("checker GC lineage still pending"));
                    }
                    let refs = l
                        .references
                        .checked_sub(1)
                        .ok_or(bad("checker lineage GC reference underflow"))?;
                    put::<Hash64>(b, MEMBER, Some(*duty), *id, None)?;
                    put::<Hash64>(b, LINEAGE_DUTY, Some(*id), *duty, None)?;
                    if refs == 0 {
                        drop_empty_owned(b, LINEAGE_DUTY, *id)?;
                        put::<LineageV9>(b, LINEAGE, None, *id, None)?;
                    } else {
                        put(
                            b,
                            LINEAGE,
                            None,
                            *id,
                            Some(&LineageV9 {
                                references: refs,
                                ..*l
                            }),
                        )?;
                    }
                }
                drop_empty_owned(b, MEMBER, *duty)?;
                put::<DutyV9>(b, DUTY, None, *duty, None)?;
                put::<Hash64>(b, WORK_DUTY, Some(*work), *duty, None)?;
            }
            drop_empty_owned(b, WORK_DUTY, *work)?;
            put::<WorkV9>(b, WORK, None, *work, None)?;
            put::<Hash64>(b, OWNER_WORK, Some(oid), *work, None)?;
        }
        drop_empty_owned(b, OWNER_WORK, oid)?;
        put::<KernelOwnerV9>(b, OWNER, None, oid, None)?;
        Ok(())
    })
    .map_err(err)
}
/// Owner11 complete typed restore. The parent's source-owner producer remains
/// responsible for catalogue authority/liveness; this validates all watcher
/// populations and exact native contract/physical closure observations.
pub(super) fn validate_v9(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    use crate::palw_native_resource_v1::BTreeNodeEnvelopeV1 as Node;
    let count = root(s, LINEAGE, None)?.rows;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Node<Hash64, u64>>(count)
        .map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut refs = BTreeMap::<Hash64, u64>::new();
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, root(s, LINEAGE, None)?, |k, v| {
        let id = borsh::from_slice::<Hash64>(k).map_err(|_| bad("checker lineage key codec"))?;
        let raw = reader.body(v)?;
        let l = decode::<LineageV9>(&raw)?;
        if l.references == 0 {
            return Err(bad("orphan checker lineage record"));
        }
        let actual =
            super::palw_claim_local_material_income_lineage_v7::observed_lineage_active_v9(s, id)?;
        if l.closed {
            if actual.is_some_and(|n| n != 0) {
                return Err(bad("checker closed fact contradicts actual native lineage"));
            }
        } else if actual.is_none_or(|n| n == 0) {
            return Err(bad(
                "checker pending lineage disappeared without actual close observation",
            ));
        }
        refs.insert(id, 0);
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, OWNER, None)?, |k, v| {
        let raw = reader.body(v)?;
        let owner = decode::<KernelOwnerV9>(&raw)?;
        if k != owner.id()?.as_byte_slice()
            || checker_pending_for_fields(s, *owner, s.last_point.map_or(0, |p| p.daa_score))?
                .is_none()
        {
            return Err(bad("checker source owner population incomplete"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, WORK, None)?, |k, v| {
        let raw = reader.body(v)?;
        let w = decode::<WorkV9>(&raw)?;
        let id = w.work.id();
        current_work(s, &w)?;
        if k != id.as_byte_slice()
            || lookup::<KernelOwnerV9>(s, OWNER, None, w.owner.id()?)?.as_deref() != Some(&w.owner)
            || lookup::<Hash64>(s, OWNER_WORK, Some(w.owner.id()?), id)?.as_deref() != Some(&id)
        {
            return Err(bad("checker work/source reverse closure differs"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, DUTY, None)?, |k, v| {
        let raw = reader.body(v)?;
        let d = decode::<DutyV9>(&raw)?;
        current_duty(s, &d)?;
        if k != d.duty.as_byte_slice()
            || lookup::<WorkV9>(s, WORK, None, d.work)?.is_none()
            || lookup::<Hash64>(s, WORK_DUTY, Some(d.work), d.duty)?.as_deref() != Some(&d.duty)
        {
            return Err(bad("checker duty/work reverse closure differs"));
        }
        let members = ids(s, MEMBER, d.duty)?;
        if d.linked == members.is_empty() {
            return Err(bad("checker linked physical membership differs"));
        }
        for id in members.iter() {
            if lookup::<Hash64>(s, LINEAGE_DUTY, Some(*id), d.duty)?.as_deref() != Some(&d.duty) {
                return Err(bad("checker physical/duty reverse member absent"));
            }
            let n = refs
                .get_mut(id)
                .ok_or(bad("checker physical lineage reference absent"))?;
            *n = n
                .checked_add(1)
                .ok_or(bad("checker lineage restored reference overflow"))?;
        }
        Ok(())
    })?;
    for (id, count) in refs {
        let l = lookup::<LineageV9>(s, LINEAGE, None, id)?
            .ok_or(bad("checker restored lineage absent"))?;
        if count != l.references || root(s, LINEAGE_DUTY, Some(id))?.rows != count {
            return Err(bad("checker lineage restored reference count differs"));
        }
        let ds = ids(s, LINEAGE_DUTY, id)?;
        for d in ds.iter() {
            if lookup::<Hash64>(s, MEMBER, Some(*d), id)?.as_deref() != Some(&id) {
                return Err(bad("checker lineage extra reverse membership"));
            }
        }
    }
    if let Some(route) = &s.kernel_route {
        for ((t, k), raw) in &route.aux {
            if *t != PALW_CLAIM_LOCAL_TABLE_V1 || k.first() != Some(&b'I') {
                continue;
            }
            let r = decode::<ComponentRootV2>(raw)?;
            let ks = match k.len() {
                18 => borsh::from_slice::<ComponentSpecV2>(&k[1..]).ok(),
                84 if k[1..3] == [2, 0] => borsh::from_slice::<ComponentSpecV2>(&k[67..]).ok(),
                _ => None,
            };
            if r.spec.owner != 11 && ks.is_none_or(|r| r.owner != 11) {
                continue;
            }
            if ks != Some(r.spec)
                || r.spec != spec(r.spec.table)
                || !matches!(
                    r.spec.table,
                    WORK | DUTY | MEMBER | LINEAGE | LINEAGE_DUTY | OWNER_WORK | OWNER | WORK_DUTY
                )
            {
                return Err(bad("unallocated checker owner11 profile"));
            }
            match k.len() {
                18 if matches!(r.spec.table, WORK | DUTY | LINEAGE | OWNER) => {}
                84 if k[1..3] == [2, 0] => {
                    let id = borsh::from_slice::<Hash64>(&k[3..67])
                        .map_err(|_| bad("checker owned scope codec"))?;
                    let exists = match r.spec.table {
                        MEMBER => lookup::<DutyV9>(s, DUTY, None, id)?.is_some(),
                        LINEAGE_DUTY => lookup::<LineageV9>(s, LINEAGE, None, id)?.is_some(),
                        OWNER_WORK => lookup::<KernelOwnerV9>(s, OWNER, None, id)?.is_some(),
                        WORK_DUTY => lookup::<WorkV9>(s, WORK, None, id)?.is_some(),
                        _ => false,
                    };
                    if !exists {
                        return Err(bad("orphan checker owned root"));
                    }
                }
                _ => return Err(bad("checker current root namespace differs")),
            }
        }
    }
    Ok(())
}
/// Legacy work with no actual Open token keeps its ORIGINAL native fee route.
/// Its typed watcher remains unavailable; this never reports closure for it.
pub(super) fn watches_work_v9(
    s: &PalwChainStateV2,
    work: Hash64,
) -> Result<bool, PalwStateV2Error> {
    lookup::<WorkV9>(s, WORK, None, work)
        .map(|r| r.is_some())
        .map_err(err)
}
pub(super) fn watches_duty_v9(
    s: &PalwChainStateV2,
    duty: Hash64,
) -> Result<bool, PalwStateV2Error> {
    lookup::<DutyV9>(s, DUTY, None, duty)
        .map(|r| r.is_some())
        .map_err(err)
}

/// Exact persisted Open population. Caller separately authenticates original
/// source catalogue before combining this with other material-duty closure.
pub(super) fn checker_pending_at_known_birth_v9(
    s: &PalwChainStateV2,
    claim: Hash64,
    birth: PalwBlockContextV2,
    catalogue: Hash64,
    now: u64,
) -> Result<Option<bool>, ContextErrorV2> {
    checker_pending_for_fields(
        s,
        KernelOwnerV9 {
            claim,
            birth,
            catalogue,
        },
        now,
    )
}
