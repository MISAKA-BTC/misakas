//! Exact native legacy/current retirement ownership. Representation only:
//! immutable F1 birth remains genuine; no current params become historical.
//! Context registry reservation: owner18/schema1 (root audit 2026-11-02),
//! distinct from owner17 Source16 finance. No old aux tag/codec is redefined.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::palw_claim_local_execution_cohort_archive_v15::{self as cohort15, NativeCohortAliasV15};
use super::palw_claim_local_rewards_v1::{self as rewards, RewardReadinessV1};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeHeapV1, NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
const OWNER_V18: u8 = 18;
const SCHEMA_V18: u16 = 1;
const PENDING: u32 = 1;
const COHORT_INDEX: u32 = 2;
const COHORT_MEMBERS: u32 = 4;
const EXEC_INDEX: u32 = 5;
const EXEC_BODIES: u32 = 6;
const CURRENT: u32 = 3;
// Source-local reverse lookup for actual selected-certification journal progress.
const BY_CLAIM: u32 = 7;
const ORIGINAL_F1: u32 = 8;
const PAID_LEDGER: u32 = 9;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: OWNER_V18, table, schema: SCHEMA_V18, key_bytes: 64, value_bytes: u64::MAX }
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
        _ => bad("terminal18 native journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("terminal18 encode length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut raw = Vec::with_capacity(n);
    v.serialize(&mut raw).map_err(|_| bad("terminal18 encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(raw, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("terminal18 typed codec"),
    })
}
fn hash<T: BorshSerialize>(tag: u8, v: &T) -> Result<Hash64, ContextErrorV2> {
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
    let mut w = W(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/legacy-terminal/v18/schema1").to_state());
    tag.serialize(&mut w).map_err(|_| bad("terminal18 hash tag"))?;
    v.serialize(&mut w).map_err(|_| bad("terminal18 hash body"))?;
    let mut out = [0; 64];
    out.copy_from_slice(w.0.finalize().as_bytes());
    Ok(Hash64::from_bytes(out))
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
fn body<T: BorshDeserialize + NativeWireScanV1>(s: &PalwChainStateV2, rv: RowValueV2) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    let raw = StateNodesV2(s).body(rv)?;
    decode(&raw)
}
fn put<T: BorshSerialize>(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Option<Hash64>,
    key: Hash64,
    v: Option<&T>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let raw = v.map(encode).transpose()?;
    let before = root(&b.state, table, scope)?;
    let reader = StateNodesV2(&b.state);
    let old = nodes::lookup(&reader, before, key.as_byte_slice())?;
    let plan = nodes::update(&reader, before, key.as_byte_slice(), old, raw.as_ref().map(|r| value(r)))?;
    match scope {
        Some(scope) => store::publish_owned_component_v2(b, scope, &plan, raw.as_deref().map(Vec::as_slice))?,
        None => store::publish_component_v2(b, &plan, raw.as_deref().map(Vec::as_slice))?,
    };
    Ok(plan.after())
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn same_birth(a: &PalwClaimStateV2, b: &PalwClaimStateV2) -> bool {
    (a.accepted_block, a.accepted_daa, a.accepted_blue_score) == (b.accepted_block, b.accepted_daa, b.accepted_blue_score)
}
fn same_identity(a: &PalwClaimStateV2, b: &PalwClaimStateV2) -> bool {
    same_birth(a, b)
        && a.class_id == b.class_id
        && a.bond == b.bond
        && a.trace_root == b.trace_root
        && a.output_root == b.output_root
        && a.execution_root == b.execution_root
        && a.job_identity == b.job_identity
        && match (&a.source, &b.source) {
            (PalwClaimSourceV2::Attempt, PalwClaimSourceV2::Attempt) => true,
            (PalwClaimSourceV2::FreePrompt { quanta: a, .. }, PalwClaimSourceV2::FreePrompt { quanta: b, .. }) => a == b,
            _ => false,
        }
}
fn key(id: Hash64, c: &PalwClaimStateV2) -> Result<Hash64, ContextErrorV2> {
    hash(1, &(id, c.accepted_block, c.accepted_daa, c.accepted_blue_score))
}

#[derive(BorshSerialize, BorshDeserialize)]
struct ExecInputV18 {
    claim: Hash64,
    row: Option<misaka_palw_kernel::ledger::ClaimRowV1>,
    authority: Option<crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2>,
    final_row: Option<crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2>,
    lock: Option<crate::palw_claim_challenge_v2::PalwPaidClaimLockV2>,
}
crate::palw_native_resource_v1::wire::wire_struct!(ExecInputV18;Hash64,Option<misaka_palw_kernel::ledger::ClaimRowV1>,Option<crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2>,Option<crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2>,Option<crate::palw_claim_challenge_v2::PalwPaidClaimLockV2>);
crate::palw_native_resource_v1::native_struct!(ExecInputV18;claim,row,authority,final_row,lock);
#[derive(BorshSerialize, BorshDeserialize)]
struct ExecV18 {
    root: Option<crate::palw_work_slice_v2::PalwWorkRootV2>,
    reward: Option<crate::palw_work_slice_v2::PalwExecV2RewardRowV1>,
    policy: Option<misaka_palw_kernel::ledger::LedgerPolicyV1>,
    slices: Vec<(u32, crate::palw_work_slice_v2::PalwWorkSliceRowV2)>,
    inputs: Vec<ExecInputV18>,
}
crate::palw_native_resource_v1::wire::wire_struct!(ExecV18;Option<crate::palw_work_slice_v2::PalwWorkRootV2>,Option<crate::palw_work_slice_v2::PalwExecV2RewardRowV1>,Option<misaka_palw_kernel::ledger::LedgerPolicyV1>,Vec<(u32,crate::palw_work_slice_v2::PalwWorkSliceRowV2)>,Vec<ExecInputV18>);
crate::palw_native_resource_v1::native_struct!(ExecV18;root,reward,policy,slices,inputs);
#[derive(BorshSerialize, BorshDeserialize)]
struct PendingV18 {
    version: u16,
    completed: bool,
    id: Hash64,
    birth: PalwBlockContextV2,
    removed: PalwBlockContextV2,
    removal_ordinal: u64,
    ruleset: crate::Hash,
    original: PalwClaimStateV2,
    terminal: PalwClaimStateV2,
    approval: Option<Vec<u8>>,
    approval_attached: bool,
    winner: Option<Hash64>,
    material_until: u64,
    // 1 = exact genuine F1 birth. 2 = exact A8 directory/A2 remains owned.
    origin_kind: u8,
    origin_f1: Option<Hash64>,
    cohort: Hash64,
    native_cohort: Option<NativeCohortAliasV15>,
    cohort_complete: bool,
    members: ComponentRootV2,
    exec: RowValueV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(PendingV18;u16,bool,Hash64,PalwBlockContextV2,PalwBlockContextV2,u64,crate::Hash,PalwClaimStateV2,PalwClaimStateV2,Option<Vec<u8>>,bool,Option<Hash64>,u64,u8,Option<Hash64>,Hash64,Option<NativeCohortAliasV15>,bool,ComponentRootV2,RowValueV2);
crate::palw_native_resource_v1::native_struct!(PendingV18;version,completed,id,birth,removed,removal_ordinal,ruleset,original,terminal,approval,approval_attached,winner,material_until,origin_kind,origin_f1,cohort,native_cohort,cohort_complete,members,exec);
#[derive(BorshSerialize, BorshDeserialize)]
struct MemberV18 {
    id: Hash64,
    original: PalwClaimStateV2,
    birth: PalwBlockContextV2,
    origin_f1: Option<Hash64>,
}
crate::palw_native_resource_v1::wire::wire_struct!(MemberV18;Hash64,PalwClaimStateV2,PalwBlockContextV2,Option<Hash64>);
crate::palw_native_resource_v1::native_struct!(MemberV18;id,original,birth,origin_f1);
#[derive(BorshSerialize, BorshDeserialize)]
struct CurrentV18 {
    version: u16,
    id: Hash64,
    birth: PalwBlockContextV2,
    point: PalwBlockContextV2,
    origin_kind: u8,
    origin_f1: Option<Hash64>,
    current: PalwClaimStateV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(CurrentV18;u16,Hash64,PalwBlockContextV2,PalwBlockContextV2,u8,Option<Hash64>,PalwClaimStateV2);
crate::palw_native_resource_v1::native_struct!(CurrentV18;version,id,birth,point,origin_kind,origin_f1,current);
/// A native representation receipt, never source/credit/financial authority.
/// Call after genuine Source8 birth capture and before deciding to skip F1.
/// Current rows are replaced only for actual Claim mutations; system ticks do
/// not create per-tick history. Canonical live courts/certs remain their owners.
/// Representation dispatch from the actual signed funding terms consumed by
/// normal admission. Selection is not paid standing: MixedV2 still requires its
/// independently minted native row9 capacity. Legacy/OAR1 eligibility metadata
/// uses the existing native admission owner, never an archive/service upgrade.
fn actual_mixed_original_profile_v25(b: &TransitionBuilder<'_>, id: Hash64) -> Result<bool, PalwStateV2Error> {
    let Some(pair) = b.extras.tir_funded_originals.get(&id) else { return Ok(false) };
    let terms = &pair.envelope().funding.terms;
    if terms.claim != id {
        return Err(err(bad("native eligibility funding subject differs from actual admission")));
    }
    Ok(crate::palw_court_original_escrow_v1::original_archive_profile_kind_v2(terms)
        == Some(crate::palw_court_original_escrow_v1::OriginalArchiveProfileKindV2::MixedV2))
}
pub(super) fn capture_live_current_v18(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) || b.extras.native_fact_ruleset.is_none() {
        return Ok(());
    }
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(err(bad("terminal18 current capture lacks executing point")));
    }
    let end = b.entries.len();
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(end as u64)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut seen = BTreeSet::new();
    for at in (0..end).rev() {
        let PalwDeltaEntryV2::Claim { key: id, new: Some(current), .. } = &b.entries[at] else { continue };
        if !seen.insert(*id) {
            continue;
        }
        let id = *id;
        if b.state.claims.get(&id) != Some(current) {
            continue;
        } // later actual deletion/new birth owns it
        let scope = key(id, current).map_err(err)?;
        if current.accepted_daa == ctx.daa_score
            && current.accepted_blue_score == ctx.blue_score
            && actual_mixed_original_profile_v25(b, id)?
            && shared::<PaidLedgerV18>(&b.state, PAID_LEDGER, scope)?.is_none()
        {
            continue;
        }
        let Some(origin) = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&b.state, b.params, id, current)?
        else {
            continue;
        };
        let birth = origin.accepting_point();
        let scoped = super::palw_claim_local_source_admission_v8::has_source_admission_v8(&b.state, id, current)?;
        let origin_f1 = native_source_original_f1_v25(&b.state, b.params, id, birth, origin.original_claim(), scoped)?;
        let origin_kind = if origin_f1.is_some() { 1 } else { 2 };
        let previous = shared::<CurrentV18>(&b.state, CURRENT, scope)?;
        if let Some(old) = &previous {
            let same = old.origin_kind == origin_kind && old.origin_f1 == origin_f1;
            let migrated = old.origin_kind == 2 && old.origin_f1.is_none() && origin_kind == 1 && origin_f1.is_some();
            if old.version != 1 || old.id != id || old.birth != birth || !same_identity(&old.current, current) || (!same && !migrated)
            {
                return Err(err(bad("terminal18 current migration identity differs")));
            }
            if old.current == *current && same {
                continue;
            } // no per-tick history or repeated migration
        }
        if let Some(owner) = paid_ledger_v18(&b.state, id, current)?
            && borsh::object_length(current).map_err(|_| err(bad("owner18 current codec")))? as u64 > owner.envelope.claim_live
        {
            return Err(err(bad("owner18 current exceeds native owned envelope")));
        }
        let h = current.native_heap_v1("terminal18 actual current replacement");
        b.native_reservation
            .grow_v1(h.bytes().ok_or(PalwStateV2Error::NativeResource(h))?)
            .map_err(PalwStateV2Error::NativeResource)?;
        let current = current.clone();
        drop(origin);
        if previous.as_ref().is_none_or(|old| old.origin_f1.is_none())
            && let Some(hash) = origin_f1
        {
            retain_original_f1_v18(b, birth, hash)?;
        }
        put(b, CURRENT, None, scope, Some(&CurrentV18 { version: 1, id, birth, point: *ctx, origin_kind, origin_f1, current }))
            .map_err(err)?;
    }
    Ok(())
}
/// Authenticate the actual unchanged F1 at this exact Source8 birth. Absence
/// is unavailable history, never a licence to project today's world backwards.
fn verified_native_legacy_f1_v25(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    birth: PalwBlockContextV2,
    original: &PalwClaimStateV2,
) -> Result<Option<Hash64>, PalwStateV2Error> {
    let key = crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(birth.block);
    let present = {
        let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
        s.kernel_route.as_ref().is_some_and(|r| r.aux.contains_key(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())))
    };
    if !present {
        return Ok(None);
    }
    let Some(legacy) = s.claim_local_admission_context_v1(p, &birth, id)? else { return Ok(None) };
    if legacy.claim_id() != id || legacy.accepting_point() != birth || !same_identity(legacy.original_claim(), original) {
        return Err(err(bad("owner18 exact legacy migration substituted immutable native source")));
    }
    actual_f1_identity(s, birth).map(Some)
}
/// Representation selection is monotone: once this exact live owner has pinned
/// genuine F1, a later native alias cannot silently discard that reference.
fn native_source_original_f1_v25(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    birth: PalwBlockContextV2,
    original: &PalwClaimStateV2,
    scoped: bool,
) -> Result<Option<Hash64>, PalwStateV2Error> {
    if let Some(current) = shared::<CurrentV18>(s, CURRENT, key(id, original).map_err(err)?)? {
        if current.id != id || current.birth != birth || !same_identity(&current.current, original) {
            return Err(err(bad("owner18 legacy migration live owner differs")));
        }
        if current.origin_kind == 1 {
            let digest = verified_native_legacy_f1_v25(s, p, id, birth, original)?
                .ok_or_else(|| err(bad("owner18 pinned legacy source lost actual birth F1")))?;
            if current.origin_f1 != Some(digest) {
                return Err(err(bad("owner18 legacy migration changed immutable F1")));
            }
            return Ok(Some(digest));
        }
    }
    if !scoped {
        return verified_native_legacy_f1_v25(s, p, id, birth, original)?
            .map(Some)
            .ok_or_else(|| err(bad("owner18 legacy native original absent")));
    }
    if original.execution_root != Hash64::default() && cohort15::alias_original_snapshot_v15(s, id, original)?.is_none() {
        // A new paid modern birth must still supply its own authenticated15;
        // it cannot use this old-history migration to evade its signed shape.
        if paid_ledger_v18(s, id, original)?.is_some_and(|owner| owner.envelope.expects_modern_alias_v18()) {
            return Ok(None);
        }
        return verified_native_legacy_f1_v25(s, p, id, birth, original);
    }
    Ok(None)
}
fn actual_f1_identity(s: &PalwChainStateV2, birth: PalwBlockContextV2) -> Result<Hash64, PalwStateV2Error> {
    let k = crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(birth.block);
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(k.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let raw = s
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, k.to_vec())))
        .ok_or_else(|| err(bad("terminal18 genuine immutable F1 absent")))?;
    Ok(crate::palw_claim_local_v1::palw_liability_material_id_v1(raw))
}
#[derive(BorshSerialize, BorshDeserialize)]
struct OriginalF1V18 {
    version: u16,
    point: PalwBlockContextV2,
    digest: Hash64,
    users: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(OriginalF1V18;u16,PalwBlockContextV2,Hash64,u64);
fn retain_original_f1_v18(b: &mut TransitionBuilder<'_>, point: PalwBlockContextV2, digest: Hash64) -> Result<(), PalwStateV2Error> {
    if actual_f1_identity(&b.state, point)? != digest {
        return Err(err(bad("terminal18 immutable original F1 identity")));
    }
    let old = shared::<OriginalF1V18>(&b.state, ORIGINAL_F1, point.block)?;
    let users = if let Some(old) = old {
        if old.version != 1 || old.point != point || old.digest != digest || old.users == 0 {
            return Err(err(bad("terminal18 shared original F1 terms")));
        }
        old.users.checked_add(1).ok_or(PalwStateV2Error::Overflow("terminal18 original F1 references"))?
    } else {
        1
    };
    put(b, ORIGINAL_F1, None, point.block, Some(&OriginalF1V18 { version: 1, point, digest, users })).map_err(err)?;
    Ok(())
}
fn release_original_f1_v18(b: &mut TransitionBuilder<'_>, point: PalwBlockContextV2, digest: Hash64) -> Result<(), PalwStateV2Error> {
    let old = shared::<OriginalF1V18>(&b.state, ORIGINAL_F1, point.block)?
        .ok_or_else(|| err(bad("terminal18 original F1 reference absent")))?;
    if old.version != 1 || old.point != point || old.digest != digest || old.users == 0 {
        return Err(err(bad("terminal18 released original F1 differs")));
    }
    if old.users == 1 {
        put::<OriginalF1V18>(b, ORIGINAL_F1, None, point.block, None).map_err(err)?;
    } else {
        put(b, ORIGINAL_F1, None, point.block, Some(&OriginalF1V18 { version: 1, point, digest, users: old.users - 1 }))
            .map_err(err)?;
    }
    Ok(())
}
pub(super) fn immutable_frame_pinned_v18(s: &PalwChainStateV2, block: Hash64) -> Result<bool, PalwStateV2Error> {
    let Some(row) = shared::<OriginalF1V18>(s, ORIGINAL_F1, block)? else { return Ok(false) };
    if row.version != 1 || row.point.block != block || row.users == 0 || actual_f1_identity(s, row.point)? != row.digest {
        return Err(err(bad("terminal18 pinned original F1 differs")));
    }
    Ok(true)
}
fn shared_member_v18(
    s: &PalwChainStateV2,
    members: ComponentRootV2,
    key: Hash64,
) -> Result<NativeRetainedV1<MemberV18>, PalwStateV2Error> {
    let rv = nodes::lookup(&StateNodesV2(s), members, key.as_byte_slice())
        .map_err(err)?
        .ok_or_else(|| err(bad("terminal18 shared member missing")))?;
    body(s, rv).map_err(err)
}
// Immutable member/EXEC literals are shared. The small reference counts are
// native journalled current rows, not duplicated vectors or per-tick history.
#[derive(BorshSerialize, BorshDeserialize)]
struct CohortV18 {
    version: u16,
    execution_root: Hash64,
    birth: PalwBlockContextV2,
    complete: bool,
    members: ComponentRootV2,
    users: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(CohortV18;u16,Hash64,PalwBlockContextV2,bool,ComponentRootV2,u64);
#[derive(BorshSerialize, BorshDeserialize)]
struct ExecRefV18 {
    version: u16,
    material: RowValueV2,
    users: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(ExecRefV18;u16,RowValueV2,u64);
fn shared<T: BorshDeserialize + NativeWireScanV1>(
    s: &PalwChainStateV2,
    table: u32,
    k: Hash64,
) -> Result<Option<NativeRetainedV1<T>>, PalwStateV2Error> {
    let r = StateNodesV2(s);
    nodes::lookup(&r, root(s, table, None).map_err(err)?, k.as_byte_slice()).map_err(err)?.map(|v| body(s, v).map_err(err)).transpose()
}
fn cohort_key(root: Hash64, birth: PalwBlockContextV2) -> Result<Hash64, ContextErrorV2> {
    hash(2, &(root, birth.block, birth.daa_score, birth.blue_score))
}
fn retain_exec(b: &mut TransitionBuilder<'_>, exec: &ExecV18) -> Result<RowValueV2, PalwStateV2Error> {
    let raw = encode(exec).map_err(err)?;
    let rv = value(&raw);
    let users = if let Some(old) = shared::<ExecRefV18>(&b.state, EXEC_INDEX, rv.data)? {
        if old.version != 1 || old.material != rv || old.users == 0 {
            return Err(err(bad("terminal18 shared EXEC index")));
        }
        let literal = StateNodesV2(&b.state).body(rv).map_err(err)?;
        if literal.as_slice() != raw.as_slice() {
            return Err(err(bad("terminal18 shared EXEC collision")));
        }
        old.users.checked_add(1).ok_or(PalwStateV2Error::Overflow("terminal18 EXEC references"))?
    } else {
        put(b, EXEC_BODIES, None, rv.data, Some(exec)).map_err(err)?;
        1
    };
    put(b, EXEC_INDEX, None, rv.data, Some(&ExecRefV18 { version: 1, material: rv, users })).map_err(err)?;
    Ok(rv)
}
fn check_exec(s: &PalwChainStateV2, rv: RowValueV2) -> Result<NativeRetainedV1<ExecV18>, PalwStateV2Error> {
    let at = shared::<ExecRefV18>(s, EXEC_INDEX, rv.data)?.ok_or_else(|| err(bad("terminal18 EXEC owner absent")))?;
    if at.version != 1 || at.users == 0 || at.material != rv {
        return Err(err(bad("terminal18 EXEC owner differs")));
    }
    let r = StateNodesV2(s);
    if nodes::lookup(&r, root(s, EXEC_BODIES, None).map_err(err)?, rv.data.as_byte_slice()).map_err(err)? != Some(rv) {
        return Err(err(bad("terminal18 EXEC literal index differs")));
    }
    body(s, rv).map_err(err)
}
fn aux<T: BorshDeserialize + NativeWireScanV1>(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    table: u8,
    key: &[u8; 64],
) -> Result<Option<NativeRetainedV1<T>>, ContextErrorV2> {
    crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, table, key)
        .map_err(ContextErrorV2::Resource)?
        .map(decode)
        .transpose()
}
fn capture_exec(b: &mut TransitionBuilder<'_>, id: Hash64) -> Result<ExecV18, PalwStateV2Error> {
    let state = &b.state;
    let root = state.exec_v2.roots.get(&id);
    let reward = state.exec_v2_rewards.get(&id);
    let route = state.kernel_route.as_ref();
    let n = state.exec_v2.slices.range((id, 0)..=(id, u32::MAX)).count();
    let inputs_n = if root.is_some() { n.checked_add(1).ok_or(PalwStateV2Error::Overflow("terminal18 EXEC inputs"))? } else { 0 };
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<(u32, crate::palw_work_slice_v2::PalwWorkSliceRowV2)>(n as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<ExecInputV18>(inputs_n as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<u8>(65).map_err(PalwStateV2Error::NativeResource)?;
    for shape in [
        root.native_heap_v1("terminal18 EXEC root"),
        reward.native_heap_v1("terminal18 EXEC reward"),
        root.and(route).map(|r| &r.header.policy).native_heap_v1("terminal18 EXEC policy"),
    ] {
        q.add_v1(shape.bytes().ok_or(PalwStateV2Error::NativeResource(shape))?).map_err(PalwStateV2Error::NativeResource)?;
    }
    for (_, slice) in state.exec_v2.slices.range((id, 0)..=(id, u32::MAX)) {
        let h = slice.native_heap_v1("terminal18 EXEC slice");
        q.add_v1(h.bytes().ok_or(PalwStateV2Error::NativeResource(h))?).map_err(PalwStateV2Error::NativeResource)?;
    }
    b.native_reservation.grow_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut out = ExecV18 {
        root: root.cloned(),
        reward: reward.cloned(),
        policy: root.and(route).map(|r| r.header.policy),
        slices: Vec::with_capacity(n),
        inputs: Vec::with_capacity(inputs_n),
    };
    for ((_, index), slice) in state.exec_v2.slices.range((id, 0)..=(id, u32::MAX)) {
        out.slices.push((*index, slice.clone()));
    }
    let Some(root) = root else { return Ok(out) };
    for claim in std::iter::once(root.prefix_claim).chain(out.slices.iter().map(|(_, s)| s.evidence_root)) {
        let Some(route) = route else {
            out.inputs.push(ExecInputV18 { claim, row: None, authority: None, final_row: None, lock: None });
            continue;
        };
        let raw = route.rows.get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, claim.as_byte_slice().to_vec()));
        let row = raw.map(|r| decode::<misaka_palw_kernel::ledger::ClaimRowV1>(r)).transpose().map_err(err)?;
        let authority = route.paid_claim_authority_for_base_checked_v1(&claim.as_bytes()).map_err(|e| match e {
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
            _ => err(bad("terminal18 EXEC authority malformed")),
        })?;
        let final_row = authority
            .as_ref()
            .map(|a| {
                aux::<crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2>(
                    route,
                    crate::palw_claim_challenge_v2::PAID_CLAIM_FINALS_V2,
                    &a.enrollment.id(),
                )
            })
            .transpose()
            .map_err(err)?
            .flatten();
        let lock = authority
            .as_ref()
            .map(|a| {
                aux::<crate::palw_claim_challenge_v2::PalwPaidClaimLockV2>(
                    route,
                    crate::palw_claim_challenge_v2::PAID_CLAIM_LOCKS_V2,
                    &a.enrollment.id(),
                )
            })
            .transpose()
            .map_err(err)?
            .flatten();
        let mut q = TirNativeRequestsV1::new();
        for shape in [
            row.as_deref().native_heap_v1("terminal18 EXEC input"),
            authority.as_deref().native_heap_v1("terminal18 EXEC authority"),
            final_row.as_deref().native_heap_v1("terminal18 EXEC Final"),
            lock.as_deref().native_heap_v1("terminal18 EXEC selected lock"),
        ] {
            q.add_v1(shape.bytes().ok_or(PalwStateV2Error::NativeResource(shape))?).map_err(PalwStateV2Error::NativeResource)?;
        }
        b.native_reservation.grow_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        out.inputs.push(ExecInputV18 {
            claim,
            row: row.as_deref().cloned(),
            authority: authority.as_deref().cloned(),
            final_row: final_row.as_deref().cloned(),
            lock: lock.as_deref().cloned(),
        });
    }
    Ok(out)
}
impl ExecV18 {
    /// Literal owner-local counterpart of retired_exec_source_closed_v1. This
    /// is a named partial native snapshot, NEVER a synthetic full kernel route.
    fn closed(&self, params: &PalwStateParamsV2, id: Hash64, terminal: &PalwClaimStateV2, now: u64) -> Result<bool, PalwStateV2Error> {
        use crate::palw_work_slice_v2::{
            PalwWorkPrefixStageV2 as Prefix, PalwWorkRootPhaseV2 as Phase, PalwWorkSliceStageV2 as Slice,
        };
        let Some(root) = &self.root else { return Ok(self.reward.is_none() && self.slices.is_empty() && self.inputs.is_empty()) };
        let settled = match root.phase {
            Phase::Settled { settled_daa } => settled_daa,
            _ => return Ok(false),
        };
        if !matches!(terminal.phase,PalwClaimPhaseV2::Final{final_daa}if final_daa==settled)
            || !matches!(root.prefix, Prefix::Verified { .. })
            || root.root_bond != terminal.bond
            || root.class_id != terminal.class_id
            || root.anchor != terminal.accepted_block
            || root.canonical_job_id != terminal.job_identity
            || root.pending != 0
            || root.next_index != root.slice_count()
            || root.accepted_work != root.slice_work_total()
            || root.verified_work != root.accepted_work
        {
            return Ok(false);
        }
        let Some(policy) = &self.policy else { return Ok(false) };
        let floor = super::palw_kernel_route_fold_v1::exec_liability_floor_v1(root.expiry_daa, params.window_court(), policy)?;
        if now <= floor || self.reward.as_ref().is_some_and(|r| now <= r.liability_until) {
            return Ok(false);
        }
        if self.slices.len() != root.slice_count() as usize
            || self.inputs.len() != self.slices.len().checked_add(1).ok_or(PalwStateV2Error::Overflow("terminal18 EXEC population"))?
        {
            return Ok(false);
        }
        for (i, (index, slice)) in self.slices.iter().enumerate() {
            if *index != i as u32
                || slice.accepted_daa > settled
                || !matches!(slice.stage,Slice::Verified{verified_daa,..}if verified_daa<=settled)
            {
                return Ok(false);
            }
        }
        for (want, input) in
            std::iter::once(root.prefix_claim).chain(self.slices.iter().map(|(_, s)| s.evidence_root)).zip(&self.inputs)
        {
            if input.claim != want {
                return Err(err(bad("terminal18 EXEC input identity")));
            }
            let (Some(row), Some(authority), Some(final_row)) = (&input.row, &input.authority, &input.final_row) else {
                return Ok(false);
            };
            if row.liability_until.is_none_or(|until| now <= until)
                || !matches!(crate::palw_exec_v2_verify::palw_exec_v2_claim_outcome_v1(row,now),crate::palw_exec_v2_verify::PalwSliceVerificationV1::Verified{final_daa}if final_daa<=settled)
            {
                return Ok(false);
            }
            if authority.version != 2
                || authority.enrollment.version != 2
                || authority.enrollment.base_claim != want.as_bytes()
                || authority.class.base_class != row.class_binding_id
                || authority.job.base_job != row.job_id
                || authority.enrollment.producer != row.producer
                || authority.authorized_daa != row.committed_daa
                || authority.enrollment.class != authority.class.id()
                || authority.enrollment.job != authority.job.id()
                || authority.enrollment.lifetime.liability_until != row.liability_until.expect("checked native horizon")
            {
                return Ok(false);
            }
            // SAME selected paid Final and bootstrap exclusion as native credit.
            if final_row.enrollment != authority.enrollment.id()
                || final_row.base_claim != want.as_bytes()
                || final_row.verification_class != authority.enrollment.class
                || final_row.signed_lifetime != authority.enrollment.lifetime
                || final_row.complete_receipts == 0
                || final_row.completed_rounds == 0
            {
                return Ok(false);
            }
            if input
                .lock
                .as_ref()
                .is_some_and(|lock| lock.anchor == misaka_palw_kernel::claim_paid_v2::paid_claim_bootstrap_anchor_v2())
            {
                return Ok(false);
            }
        }
        let _ = id;
        Ok(true)
    }
}
fn lookup(
    s: &PalwChainStateV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<Option<(RowValueV2, NativeRetainedV1<PendingV18>)>, PalwStateV2Error> {
    let reader = StateNodesV2(s);
    let Some(rv) =
        nodes::lookup(&reader, root(s, PENDING, None).map_err(err)?, key(id, c).map_err(err)?.as_byte_slice()).map_err(err)?
    else {
        return Ok(None);
    };
    let row = body(s, rv).map_err(err)?;
    Ok(Some((rv, row)))
}
fn inventory(row: &PendingV18, rv: RowValueV2) -> Result<NativeRetainedV1<store::ArchiveReferencesV2>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentRootV2>(1).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<RowValueV2>(if row.native_cohort.is_some() { 4 } else { 2 }).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut literals = Vec::with_capacity(if row.native_cohort.is_some() { 4 } else { 2 });
    literals.extend([rv, row.exec]);
    if let Some(alias) = row.native_cohort {
        literals.extend(alias.literals_v15())
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        store::ArchiveReferencesV2 { version: 2, components: vec![row.members], literals },
        lease,
    ))
}
fn replace_pending(b: &mut TransitionBuilder<'_>, row: &PendingV18, old: Option<RowValueV2>) -> Result<(), PalwStateV2Error> {
    // Keep the old immutable pins alive until the replacement body AND its A2
    // are published. A2 is liveness only, never financial/credit authority.
    let old_key = old
        .map(|rv| -> Result<_, PalwStateV2Error> {
            let old_row = body::<PendingV18>(&b.state, rv).map_err(err)?;
            let raw = encode(&*inventory(&old_row, rv).map_err(err)?).map_err(err)?;
            Ok(akey(value(&raw).data))
        })
        .transpose()?;
    let raw = encode(row).map_err(err)?;
    let rv = value(&raw);
    put(b, PENDING, None, key(row.id, &row.terminal).map_err(err)?, Some(row)).map_err(err)?;
    let by = key(row.id, &row.terminal).map_err(err)?;
    if row.approval_attached {
        put(b, BY_CLAIM, Some(row.id), by, Some(row)).map_err(err)?;
    } else {
        put::<PendingV18>(b, BY_CLAIM, Some(row.id), by, None).map_err(err)?;
        if root(&b.state, BY_CLAIM, Some(row.id)).map_err(err)?.rows == 0 {
            clear_owned_root_v18(b, BY_CLAIM, row.id)?;
        }
    }
    let a = inventory(row, rv).map_err(err)?;
    let raw = encode(&*a).map_err(err)?;
    let new_key = akey(value(&raw).data);
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &new_key, Some(&*a))?;
    if let Some(old_key) = old_key.filter(|old_key| *old_key != new_key) {
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &old_key, None)?;
    }
    Ok(())
}
fn member_origin_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<(PalwBlockContextV2, Option<Hash64>), PalwStateV2Error> {
    if let Some(origin) = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(s, p, id, c)? {
        let point = origin.accepting_point();
        let scoped = super::palw_claim_local_source_admission_v8::has_source_admission_v8(s, id, c)?;
        let digest = native_source_original_f1_v25(s, p, id, point, origin.original_claim(), scoped)?;
        return Ok((point, digest));
    }
    if let Some(closed) = super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(s, id, c)?
        && same_identity(&closed.original, c)
    {
        return Ok((closed.birth, None));
    }
    if let Some(closed) = super::palw_claim_local_compact_sources_v1::read(s, id)?
        && same_identity(&closed.original, c)
    {
        return Ok((closed.birth, None));
    }
    Err(err(bad("terminal18 shared member original authority absent")))
}
fn retain_cohort(
    b: &mut TransitionBuilder<'_>,
    scope: Hash64,
    original: &PalwClaimStateV2,
    birth: PalwBlockContextV2,
) -> Result<(bool, ComponentRootV2), PalwStateV2Error> {
    if let Some(old) = shared::<CohortV18>(&b.state, COHORT_INDEX, scope)? {
        if old.version != 1
            || old.execution_root != original.execution_root
            || old.birth != birth
            || old.users == 0
            || root(&b.state, COHORT_MEMBERS, Some(scope)).map_err(err)? != old.members
        {
            return Err(err(bad("terminal18 shared cohort scope differs")));
        }
        let next = CohortV18 {
            version: 1,
            execution_root: old.execution_root,
            birth: old.birth,
            complete: old.complete,
            members: old.members,
            users: old.users.checked_add(1).ok_or(PalwStateV2Error::Overflow("terminal18 cohort references"))?,
        };
        put(b, COHORT_INDEX, None, scope, Some(&next)).map_err(err)?;
        return Ok((next.complete, next.members));
    }
    let mut count = 0u64;
    let mut q = TirNativeRequestsV1::new();
    let complete = if original.execution_root == Hash64::default() {
        true
    } else {
        super::palw_claim_local_execution_cohort_census_v15::visit_known_birth_members_v15(
            &b.state,
            original.execution_root,
            birth,
            |_, c| {
                count = count.checked_add(1).ok_or(PalwStateV2Error::Overflow("terminal18 cohort members"))?;
                let h = c.native_heap_v1("terminal18 actual same-root member");
                q.add_v1(h.bytes().ok_or(PalwStateV2Error::NativeResource(h))?).map_err(PalwStateV2Error::NativeResource)?;
                Ok(std::ops::ControlFlow::Continue(()))
            },
        )?
    };
    q.exact_v1::<MemberV18>(count).map_err(PalwStateV2Error::NativeResource)?;
    b.native_reservation.grow_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut members =
        Vec::with_capacity(usize::try_from(count).map_err(|_| PalwStateV2Error::Overflow("terminal18 cohort host count"))?);
    if original.execution_root != Hash64::default() {
        let second = super::palw_claim_local_execution_cohort_census_v15::visit_known_birth_members_v15(
            &b.state,
            original.execution_root,
            birth,
            |id, c| {
                let (point, origin_f1) = member_origin_v18(&b.state, b.params, id, c)?;
                members.push(MemberV18 { id, original: c.clone(), birth: point, origin_f1 });
                Ok(std::ops::ControlFlow::Continue(()))
            },
        )?;
        if second != complete || members.len() as u64 != count {
            return Err(err(bad("terminal18 census changed during native capture")));
        }
    }
    if root(&b.state, COHORT_MEMBERS, Some(scope)).map_err(err)?.rows != 0 {
        return Err(err(bad("terminal18 orphan owned cohort")));
    }
    for m in &members {
        if let Some(hash) = m.origin_f1 {
            retain_original_f1_v18(b, m.birth, hash)?;
        }
        put(b, COHORT_MEMBERS, Some(scope), key(m.id, &m.original).map_err(err)?, Some(m)).map_err(err)?;
    }
    let members = root(&b.state, COHORT_MEMBERS, Some(scope)).map_err(err)?;
    put(
        b,
        COHORT_INDEX,
        None,
        scope,
        Some(&CohortV18 { version: 1, execution_root: original.execution_root, birth, complete, members, users: 1 }),
    )
    .map_err(err)?;
    Ok((complete, members))
}
/// The owning retire_claim_at(ctx) method consumes this immediately. No raw
/// Claim->witness API; all fields came from the actual still-live writer state.
pub(super) struct CapturedRetirementV18 {
    key: Hash64,
    point: PalwBlockContextV2,
    first: usize,
    draft: RowValueV2,
    exec_root: bool,
}
pub(super) fn capture_before_retirement_v18(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    terminal: &PalwClaimStateV2,
) -> Result<Option<CapturedRetirementV18>, PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) || b.extras.native_fact_ruleset.is_none() {
        return Ok(None);
    }
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(err(bad("terminal18 capture lacks actual executing point")));
    }
    if b.state.claims.get(&id) != Some(terminal) {
        return Err(err(bad("terminal18 is not the actual live retirement row")));
    }
    if !matches!(terminal.phase, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. }) {
        return Ok(None);
    }
    if lookup(&b.state, id, terminal)?.is_some() {
        return Err(err(bad("terminal18 exact retirement already captured")));
    }
    let Some(origin) = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&b.state, b.params, id, terminal)? else {
        return Ok(None);
    };
    let birth = origin.accepting_point();
    let original = origin.original_claim();
    let scoped = super::palw_claim_local_source_admission_v8::has_source_admission_v8(&b.state, id, terminal)?;
    let origin_f1 = native_source_original_f1_v25(&b.state, b.params, id, birth, original, scoped)?;
    let origin_kind = if origin_f1.is_some() { 1 } else { 2 };
    let paid = paid_ledger_v18(&b.state, id, terminal)?.map(|owner| owner.envelope);
    let native_cohort = cohort15::alias_original_snapshot_v15(&b.state, id, original)?;
    let approval = rewards::cert_bytes(b.state.kernel_route.as_ref(), id);
    if let Some(e) = paid {
        let bytes = borsh::object_length(terminal).map_err(|_| err(bad("owner18 terminal shape")))? as u64;
        if bytes > e.claim_terminal
            || approval.is_some_and(|raw| raw.len() as u64 > e.cert)
            || b.state.exec_v2.roots.contains_key(&id)
            || e.modern_alias && native_cohort.is_none()
        {
            return Err(err(bad("owner18 retirement exceeds native owned finite envelope")));
        }
    }
    let winner = b
        .state
        .kernel_route
        .as_ref()
        .and_then(|r| {
            r.aux.iter().find_map(|((t, k), v)| {
                (*t == crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1 && k.as_slice() == id.as_byte_slice())
                    .then_some(v)
            })
        })
        .map(|v| borsh::from_slice::<Hash64>(v).map_err(|_| err(bad("terminal18 actual winner codec"))))
        .transpose()?;
    let mut material_until = original.trace_retention_daa.max(terminal.trace_retention_daa);
    if let Some(e) = b.state.prefix_export_reservation_v1(&birth, id)? {
        material_until = material_until.max(e.material_until)
    }
    let mut q = TirNativeRequestsV1::new();
    for h in [original.native_heap_v1("terminal18 original"), terminal.native_heap_v1("terminal18 terminal")] {
        q.add_v1(h.bytes().ok_or(PalwStateV2Error::NativeResource(h))?).map_err(PalwStateV2Error::NativeResource)?;
    }
    q.exact_v1::<PendingV18>(1).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<u8>(approval.map_or(0, |v| v.len()) as u64).map_err(PalwStateV2Error::NativeResource)?;
    b.native_reservation.grow_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let original = original.clone();
    let terminal = terminal.clone();
    let approval = approval.map(|v| v.to_vec());
    drop(origin);
    let (cohort, complete, members_root) = if let Some(alias) = native_cohort {
        (Hash64::default(), true, alias.members_v15())
    } else {
        let cohort = cohort_key(original.execution_root, birth).map_err(err)?;
        let (complete, members) = retain_cohort(b, cohort, &original, birth)?;
        (cohort, complete, members)
    };
    let scope = key(id, &terminal).map_err(err)?;
    let exec = capture_exec(b, id)?;
    let exec_root = exec.root.is_some();
    let native_exec = rewards::current_exec_source_closed_v1(&b.state, b.params, id, &terminal, ctx.daa_score)?;
    if exec.closed(b.params, id, &terminal, ctx.daa_score)? != native_exec {
        return Err(err(bad("terminal18 scoped EXEC snapshot differs from actual native owner")));
    }
    if let Some(hash) = origin_f1 {
        retain_original_f1_v18(b, birth, hash)?;
    }
    let row = PendingV18 {
        version: 1,
        completed: false,
        id,
        birth,
        removed: *ctx,
        removal_ordinal: 0,
        ruleset: b.extras.native_fact_ruleset.expect("checked actual ruleset"),
        original,
        terminal,
        approval_attached: approval.is_some(),
        approval,
        winner,
        material_until,
        origin_kind,
        origin_f1,
        cohort,
        native_cohort,
        cohort_complete: complete,
        members: members_root,
        exec: retain_exec(b, &exec)?,
    };
    if let Some(e) = paid
        && borsh::object_length(&row).map_err(|_| err(bad("owner18 actual Pending codec")))? as u64 > e.rows[1].body
    {
        return Err(err(bad("owner18 actual Pending exceeds native owned finite envelope")));
    }
    // Drafts are temporary inside the ONE retirement method; cold import must
    // reject them. Zero ordinal is never interpreted as a clock or authority.
    let first = b.entries.len();
    replace_pending(b, &row, None)?;
    let draft = lookup_key(&b.state, scope)?.ok_or_else(|| err(bad("terminal18 installed draft missing")))?.0;
    Ok(Some(CapturedRetirementV18 { key: scope, point: *ctx, first, draft, exec_root }))
}
fn lookup_key(s: &PalwChainStateV2, k: Hash64) -> Result<Option<(RowValueV2, NativeRetainedV1<PendingV18>)>, PalwStateV2Error> {
    let reader = StateNodesV2(s);
    let Some(rv) = nodes::lookup(&reader, root(s, PENDING, None).map_err(err)?, k.as_byte_slice()).map_err(err)? else {
        return Ok(None);
    };
    Ok(Some((rv, body(s, rv).map_err(err)?)))
}
/// Native row writer guard: only a genuine draft from the actual retirement
/// seam may remove a migrated live owner. Unmigrated historical writers retain
/// their released behavior, including below the stage2 fence.
pub(super) fn guard_actual_removal_v18(b: &TransitionBuilder<'_>, id: Hash64, c: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
    let at = b.claim_local_lock_point_v6.or(b.state.last_point).map_or(0, |p| p.daa_score);
    if !b.params.claim_local_settlement_active_at(at) {
        return Ok(());
    }
    let reader = StateNodesV2(&b.state);
    let migrated = nodes::lookup(&reader, root(&b.state, CURRENT, None).map_err(err)?, key(id, c).map_err(err)?.as_byte_slice())
        .map_err(err)?
        .is_some();
    if !migrated {
        return Ok(());
    }
    let Some((_, draft)) = lookup(&b.state, id, c)? else { return Err(err(bad("migrated retirement lacks actual native capture"))) };
    if draft.completed || draft.terminal != *c || draft.id != id {
        return Err(err(bad("migrated retirement capture belongs to another row")));
    }
    Ok(())
}
pub(super) fn finish_actual_retirement_v18(
    b: &mut TransitionBuilder<'_>,
    capture: CapturedRetirementV18,
) -> Result<(), PalwStateV2Error> {
    let (rv, row) = lookup_key(&b.state, capture.key)?.ok_or_else(|| err(bad("actual terminal draft disappeared")))?;
    if rv != capture.draft
        || b.claim_local_lock_point_v6 != Some(capture.point)
        || row.completed
        || row.removed != capture.point
        || capture.first > b.entries.len()
    {
        return Err(err(bad("actual terminal capture execution scope differs")));
    }
    let mut found = None;
    for (i, entry) in b.entries[capture.first..].iter().enumerate() {
        if matches!(entry,PalwDeltaEntryV2::Claim{key,old:Some(old),new:None}if *key==row.id&&old==&row.terminal) {
            if found.is_some() {
                return Err(err(bad("actual terminal removal duplicated")));
            }
            found = Some(capture.first + i);
        }
    }
    let ordinal = found.ok_or_else(|| err(bad("actual terminal capture lacks its exact removal")))?;
    if b.state.claims.get(&row.id).is_some_and(|c| same_birth(c, &row.terminal)) {
        return Err(err(bad("actual terminal birth is still live")));
    }
    let raw = encode(&*row).map_err(err)?;
    // Admit the independently owned mutable decode BEFORE allocation.
    let shape = row.native_heap_v1("actual terminal finalized copy");
    b.native_reservation
        .grow_v1(shape.bytes().ok_or(PalwStateV2Error::NativeResource(shape))?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let mut finalized: PendingV18 = borsh::from_slice(&raw).map_err(|_| err(bad("actual terminal owned copy")))?;
    finalized.completed = true;
    finalized.removal_ordinal = u64::try_from(ordinal).map_err(|_| PalwStateV2Error::Overflow("terminal18 journal ordinal"))?;
    replace_pending(b, &finalized, Some(rv))?;
    if let Some(current) = shared::<CurrentV18>(&b.state, CURRENT, capture.key)?
        && let Some(hash) = current.origin_f1
    {
        release_original_f1_v18(b, current.birth, hash)?;
    }
    put::<CurrentV18>(b, CURRENT, None, capture.key, None).map_err(err)?;
    if b.claim_local_capture_retired_exec_events_v24 {
        let key =
            (finalized.id, finalized.terminal.accepted_block, finalized.terminal.accepted_daa, finalized.terminal.accepted_blue_score);
        if b.claim_local_retired_claim_events_v25.contains_key(&key) {
            return Err(err(bad("duplicate actual Claim retirement observation")));
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<BTreeNodeEnvelopeV1<RetiredExecEventKeyV24, ActualRetiredClaimObservationV25>>(1)
            .map_err(PalwStateV2Error::NativeResource)?;
        b.native_reservation.grow_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let old = hash(27, &finalized.terminal).map_err(err)?;
        b.claim_local_retired_claim_events_v25.insert(
            key,
            ActualRetiredClaimObservationV25 {
                id: finalized.id,
                birth: finalized.birth,
                carrying: finalized.terminal.accepted_block,
                removed: capture.point,
                ordinal,
                old,
                sealed_end: b.entries.len(),
            },
        );
    }
    // Preserve actual predelete EXEC observation BEFORE the caller's native
    // resolver may replace this exact Pending with compact A5. FP's empty
    // eleven-byte EXEC shape creates no observation/map entry.
    if capture.exec_root
        && b.claim_local_capture_retired_exec_events_v24
        && let Some(token) =
            prepare_retired_exec_event_coverage_v24(&b.state, b.params, &capture.point, &finalized.terminal, finalized.id)?
    {
        let key = token.claim_birth_v24();
        if b.claim_local_retired_exec_events_v24.contains_key(&key) {
            return Err(err(bad("duplicate actual retired EXEC event observation")));
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<BTreeNodeEnvelopeV1<RetiredExecEventKeyV24, RetiredExecEventObservationV24>>(1)
            .map_err(PalwStateV2Error::NativeResource)?;
        b.native_reservation.grow_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        b.claim_local_retired_exec_events_v24.insert(key, RetiredExecEventObservationV24 { token, sealed_end: b.entries.len() });
    }
    drop(raw);
    Ok(())
}
fn check_record(s: &PalwChainStateV2, p: &PalwStateParamsV2, rv: RowValueV2, row: &PendingV18) -> Result<(), PalwStateV2Error> {
    let indexed = nodes::lookup(
        &StateNodesV2(s),
        root(s, BY_CLAIM, Some(row.id)).map_err(err)?,
        key(row.id, &row.terminal).map_err(err)?.as_byte_slice(),
    )
    .map_err(err)?;
    if indexed != row.approval_attached.then_some(rv) || row.approval_attached && row.approval.is_none() {
        return Err(err(bad("terminal18 selected canonical instance attachment differs")));
    }
    if row.version != 1
        || !row.completed
        || !same_identity(&row.original, &row.terminal)
        || !matches!(row.terminal.phase, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. })
        || row.birth.daa_score != row.original.accepted_daa
        || row.birth.blue_score != row.original.accepted_blue_score
        || row.removed.blue_score < row.birth.blue_score
        || row.removed.daa_score < row.birth.daa_score
        || row.material_until < row.original.trace_retention_daa.max(row.terminal.trace_retention_daa)
    {
        return Err(err(bad("terminal18 native source/birth/terminal scope")));
    }
    if !p.claim_local_settlement_active_at(row.removed.daa_score) {
        return Err(err(bad("terminal18 record outside actual history")));
    }
    check_exec(s, row.exec)?.validate_shape(&row.terminal)?;
    let origin = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(s, p, row.id, &row.original)?
        .ok_or_else(|| err(bad("terminal18 original authority absent")))?;
    if origin.accepting_point() != row.birth || !same_identity(origin.original_claim(), &row.original) {
        return Err(err(bad("terminal18 original authority substituted")));
    }
    match (row.origin_kind, row.origin_f1) {
        (1, Some(expected)) => {
            if verified_native_legacy_f1_v25(s, p, row.id, row.birth, &row.original)? != Some(expected) {
                return Err(err(bad("terminal18 pending legacy immutable source differs")));
            }
            let k = crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(row.birth.block);
            let raw = s
                .kernel_route
                .as_ref()
                .and_then(|r| {
                    r.aux.iter().find_map(|((t, key), raw)| (*t == PALW_CLAIM_LOCAL_TABLE_V1 && key.as_slice() == k).then_some(raw))
                })
                .ok_or_else(|| err(bad("terminal18 immutable F1 absent")))?;
            if crate::palw_claim_local_v1::palw_liability_material_id_v1(raw) != expected {
                return Err(err(bad("terminal18 immutable F1 changed")));
            }
        }
        (2, None) => {
            if !super::palw_claim_local_source_admission_v8::has_source_admission_v8(s, row.id, &row.original)? {
                return Err(err(bad("terminal18 genuine A8 alias absent")));
            }
        }
        _ => return Err(err(bad("terminal18 origin codec"))),
    }
    if let Some(alias) = row.native_cohort {
        if row.cohort != Hash64::default()
            || !row.cohort_complete
            || row.members != alias.members_v15()
            || cohort15::check_native_alias_v15(s, &alias, row.id, &row.original)? != row.birth
        {
            return Err(err(bad("terminal18 exact native cohort alias differs")));
        }
    } else {
        if root(s, COHORT_MEMBERS, Some(row.cohort)).map_err(err)? != row.members {
            return Err(err(bad("terminal18 owned cohort root differs")));
        }
        let shared =
            shared::<CohortV18>(s, COHORT_INDEX, row.cohort)?.ok_or_else(|| err(bad("terminal18 shared cohort owner absent")))?;
        if shared.version != 1
            || shared.users == 0
            || shared.execution_root != row.original.execution_root
            || shared.birth != row.birth
            || shared.complete != row.cohort_complete
            || shared.members != row.members
            || row.cohort != cohort_key(shared.execution_root, shared.birth).map_err(err)?
        {
            return Err(err(bad("terminal18 shared cohort descriptor differs")));
        }
        if row.cohort_complete && row.original.execution_root != Hash64::default() {
            let self_ref = nodes::lookup(&StateNodesV2(s), row.members, key(row.id, &row.original).map_err(err)?.as_byte_slice())
                .map_err(err)?
                .ok_or_else(|| err(bad("terminal18 complete cohort omitted original self")))?;
            let member = body::<MemberV18>(s, self_ref).map_err(err)?;
            if member.id != row.id || !same_identity(&member.original, &row.original) {
                return Err(err(bad("terminal18 cohort self substituted")));
            }
        }
    }
    let a = inventory(row, rv).map_err(err)?;
    let raw = encode(&*a).map_err(err)?;
    let k = akey(value(&raw).data);
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(k.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let present = s
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, k.to_vec())))
        .ok_or_else(|| err(bad("terminal18 A2 pin absent")))?;
    if present.as_slice() != raw.as_slice() {
        return Err(err(bad("terminal18 A2 pin changed")));
    }
    Ok(())
}
fn own_standing(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    now: u64,
    row: &PendingV18,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    if !matches!(row.terminal.phase, PalwClaimPhaseV2::Final { .. })
        || crate::palw_unresolved_bound_v1::palw_claim_lapsed_bound_v1(p, &row.terminal, row.removed.daa_score)
    {
        return Ok(RewardReadinessV1::Denied);
    }
    if !row.cohort_complete || !rewards::inputs_known(s, p, row.id, &row.original)? {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    }
    let exec = RetiredExecViewV18 { row: check_exec(s, row.exec)? };
    let standing =
        super::palw_claim_local_retired_checkpoint_v23::retired_exec_readiness_v23(s, p, row.id, &row.terminal, &exec, now)?;
    if standing != RewardReadinessV1::Ready {
        return Ok(standing);
    }
    // Missing selected approval stays unknown/Held. A persisted fact is not an
    // approval substitute, and no late same-raw-ID certificate may be borrowed.
    rewards::approval(p, row.id, &row.terminal, row.approval.as_deref())
}
pub(super) fn readiness_for_source_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    now: u64,
    id: Hash64,
    born: &PalwClaimStateV2,
    target: Hash64,
    target_birth: &PalwClaimStateV2,
) -> Result<Option<RewardReadinessV1>, PalwStateV2Error> {
    let Some((rv, row)) = lookup(s, id, born)? else { return Ok(None) };
    check_record(s, p, rv, &row)?;
    let own = own_standing(s, p, now, &row)?;
    if own != RewardReadinessV1::Ready {
        return Ok(Some(own));
    }
    // The target's whole matching-root resolver visits every peer's own status.
    // Cross-root causal consumers must evaluate THIS pending source's complete
    // original cohort before using its credit. Causal inputs remain source-local.
    if (id == target && same_birth(born, target_birth))
        || (born.execution_root != Hash64::default() && born.execution_root == target_birth.execution_root)
    {
        return Ok(Some(own));
    }
    Ok(Some(rewards::recorded_reward_readiness_v1(s, p, now, id, &row.terminal, false)?))
}
pub(super) fn visit_cohort_members_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    born: &PalwClaimStateV2,
    mut visit: impl FnMut(Hash64, &PalwClaimStateV2) -> Result<std::ops::ControlFlow<()>, PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let Some((rv, row)) = lookup(s, id, born)? else { return Ok(false) };
    check_record(s, p, rv, &row)?;
    if let Some(alias) = row.native_cohort {
        cohort15::visit_native_alias_members_v15(s, &alias, id, &row.original, visit)?;
        return Ok(true);
    }
    let mut stopped = false;
    let mut callback_error = None;
    let result = nodes::complete_rows(&StateNodesV2(s), row.members, |_, rv| {
        if stopped {
            return Ok(());
        }
        let m = body::<MemberV18>(s, rv)?;
        match visit(m.id, &m.original) {
            Ok(std::ops::ControlFlow::Continue(())) => {}
            Ok(std::ops::ControlFlow::Break(())) => stopped = true,
            Err(e) => {
                callback_error = Some(e);
                return Err(bad("terminal18 cohort callback refused"));
            }
        }
        Ok(())
    });
    if let Some(e) = callback_error {
        return Err(e);
    }
    result.map_err(err)?;
    Ok(row.cohort_complete)
}
/// Lazy exact-owner resolver. No all-pending polling or per-tick history is
/// created. Existing dependency reads may invoke the pure view above; this
/// writer is called for the actual retirement/owner progress being processed.
pub(super) fn resolve_exact_pending_v18(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    born: &PalwClaimStateV2,
) -> Result<bool, PalwStateV2Error> {
    let Some((rv, row)) = lookup(&b.state, id, born)? else { return Ok(false) };
    check_record(&b.state, b.params, rv, &row)?;
    let readiness = rewards::recorded_reward_readiness_v1(&b.state, b.params, ctx.daa_score, id, &row.terminal, false)?;
    let permitted = match readiness {
        RewardReadinessV1::Ready => true,
        RewardReadinessV1::Denied => false,
        RewardReadinessV1::Hold(_) => return Ok(false),
    };
    let approval = row.approval.as_deref().unwrap_or(&[]);
    let shape = row
        .original
        .native_heap_v1("terminal18 A5 original")
        .add::<super::palw_claim_local_compact_sources_v1::PalwClosedClaimSourceV1>(
        row.terminal.phase.native_heap_v1("terminal18 A5 phase"),
        "terminal18 A5 body",
    );
    let requests = shape
        .bytes()
        .ok_or(PalwStateV2Error::NativeResource(shape))?
        .checked_add(approval.len() as u64)
        .ok_or(PalwStateV2Error::Overflow("terminal18 A5 copy"))?;
    b.native_reservation.grow_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
    let closed = super::palw_claim_local_compact_sources_v1::PalwClosedClaimSourceV1 {
        version: 1,
        ruleset: row.ruleset,
        claim: id,
        birth: row.birth,
        original: row.original.clone(),
        terminal_phase: row.terminal.phase.clone(),
        closed_at: *ctx,
        material_until: row.material_until,
        winner: row.winner,
        approval: approval.to_vec(),
        permitted,
    };
    super::palw_claim_local_exact_closed_source_v5::publish_closed_source_v5(b, &closed)?;
    // Keep the compatibility S1 slot only if no different born source owns it.
    super::palw_claim_local_compact_sources_v1::publish_compatible_closed_v18(b, &closed)?;
    let a = inventory(&row, rv).map_err(err)?;
    let raw = encode(&*a).map_err(err)?;
    b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(value(&raw).data), None)?;
    release_shared(b, &row)?;
    if let Some(hash) = row.origin_f1 {
        release_original_f1_v18(b, row.birth, hash)?;
    }
    let scope = key(id, &row.terminal).map_err(err)?;
    put::<PendingV18>(b, BY_CLAIM, Some(row.id), scope, None).map_err(err)?;
    if root(&b.state, BY_CLAIM, Some(row.id)).map_err(err)?.rows == 0 {
        clear_owned_root_v18(b, BY_CLAIM, row.id)?;
    }
    put::<PendingV18>(b, PENDING, None, scope, None).map_err(err)?;
    put::<PaidLedgerV18>(b, PAID_LEDGER, None, scope, None).map_err(err)?;
    Ok(true)
}
fn clear_owned_root_v18(b: &mut TransitionBuilder<'_>, table: u32, scope: Hash64) -> Result<(), PalwStateV2Error> {
    let mut key = [0u8; 84];
    key[0] = store::CONTEXT_CURRENT_PREFIX_V2;
    key[1..3].copy_from_slice(&2u16.to_le_bytes());
    key[3..67].copy_from_slice(scope.as_byte_slice());
    spec(table).serialize(&mut &mut key[67..]).map_err(|_| err(bad("terminal18 released owned root key")))?;
    b.write_kernel_row_encoded_v1::<ComponentRootV2>(PALW_CLAIM_LOCAL_TABLE_V1, &key, None)
}
/// Advance only the exact persisted selected snapshot through actual canonical
/// journal transitions. Raw-ID equality alone never lends another birth's vote.
/// Capture precedes canonical GC, and resolves only these actual source owners.
/// Only the actual selected-cert relay can mint this transient observation.
/// A birth-specific owner key and exact native journal coordinate prevent raw
/// ID equality or an identical recreated canonical row from lending authority.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct CertRelaySealV18 {
    id: Hash64,
    birth: PalwBlockContextV2,
    scope: Hash64,
    point: PalwBlockContextV2,
    ordinal: usize,
    sealed_end: usize,
    old: Hash64,
    new: Option<Hash64>,
    original: Hash64,
    terminal: Hash64,
    phase: Hash64,
}
pub(super) fn rollback_cert_relay_seals_v18(b: &mut TransitionBuilder<'_>, first: usize) {
    b.claim_local_cert_relay_seals_v18.retain(|_, seal| seal.sealed_end <= first);
    b.claim_local_retired_exec_events_v24.retain(|_, event| event.sealed_end <= first);
    b.claim_local_retired_claim_events_v25.retain(|_, event| event.sealed_end <= first);
}
pub(super) fn capture_selected_cert_progress_v18(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_security_certification_v1 as cert;
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) || b.extras.native_fact_ruleset.is_none() {
        return Ok(());
    }
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(err(bad("terminal18 certification relay lacks actual point")));
    }
    let end = b.entries.len();
    for at in 0..end {
        let PalwDeltaEntryV2::KernelRouteRow { table, key, old: Some(old), new } = &b.entries[at] else { continue };
        if *table != cert::PALW_SECURITY_CERTIFICATION_TABLE_V1 || key.len() != 65 || key[0] != cert::CERTIFICATION_CLAIM_PREFIX_V1 {
            continue;
        }
        let before = decode::<cert::PalwClaimCertificationRowV1>(old).map_err(err)?;
        let id = Hash64::from_bytes(before.claim);
        let mut expected = [0u8; 65];
        expected[0] = cert::CERTIFICATION_CLAIM_PREFIX_V1;
        expected[1..].copy_from_slice(&cert::palw_certification_id_v1(&id));
        if key.as_slice() != expected {
            return Err(err(bad("terminal18 certification actual key differs")));
        }
        let after = new.as_ref().map(|raw| decode::<cert::PalwClaimCertificationRowV1>(raw).map_err(err)).transpose()?;
        if let Some(after) = &after
            && (
                before.claim,
                before.class_id,
                before.producer_bond,
                before.producer_principal,
                before.attempt,
                before.final_daa,
                before.committed_daa,
                before.committed_span,
                before.payout_payload,
            ) != (
                after.claim,
                after.class_id,
                after.producer_bond,
                after.producer_principal,
                after.attempt,
                after.final_daa,
                after.committed_daa,
                after.committed_span,
                after.payout_payload,
            )
        {
            return Err(err(bad("terminal18 selected certification identity changes")));
        }
        let index = root(&b.state, BY_CLAIM, Some(id)).map_err(err)?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<RowValueV2>(index.rows).map_err(PalwStateV2Error::NativeResource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let mut candidates = Vec::with_capacity(
            usize::try_from(index.rows).map_err(|_| PalwStateV2Error::Overflow("terminal18 selected candidate count"))?,
        );
        nodes::complete_rows(&StateNodesV2(&b.state), index, |_, rv| {
            candidates.push(rv);
            Ok(())
        })
        .map_err(err)?;
        // Actual bytes are admitted/copied while the journal borrow is live;
        // no current canonical lookup is substituted after native GC.
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<u8>(
            (old.len() as u64)
                .checked_add(new.as_ref().map_or(0, |raw| raw.len()) as u64)
                .ok_or(PalwStateV2Error::Overflow("terminal18 actual certification bytes"))?,
        )
        .map_err(PalwStateV2Error::NativeResource)?;
        let _journal_lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let old = old.clone();
        let new = new.clone();
        for rv in candidates {
            let row = body::<PendingV18>(&b.state, rv).map_err(err)?;
            if row.id != id {
                return Err(err(bad("terminal18 claim reverse index identity")));
            }
            if !row.approval_attached || row.approval.as_deref() != Some(old.as_slice()) {
                continue;
            }
            check_record(&b.state, b.params, rv, &row)?;
            if before.class_id != row.terminal.class_id.as_bytes()
                || before.producer_bond != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&row.terminal.bond)
                || before.attempt != matches!(row.terminal.source, PalwClaimSourceV2::Attempt)
                || before.committed_daa != before.final_daa
                || before.final_daa
                    != match row.terminal.phase {
                        PalwClaimPhaseV2::Final { final_daa } => final_daa,
                        _ => continue,
                    }
            {
                return Err(err(bad("terminal18 selected snapshot differs from actual source/Final")));
            }
            let scope = self::key(id, &row.original).map_err(err)?;
            let mut seal = CertRelaySealV18 {
                id,
                birth: row.birth,
                scope,
                point: *ctx,
                ordinal: at,
                sealed_end: 0,
                old: value(&old).data,
                new: new.as_ref().map(|raw| value(raw).data),
                original: hash(24, &row.original).map_err(err)?,
                terminal: hash(25, &row.terminal).map_err(err)?,
                phase: hash(26, &row.terminal.phase).map_err(err)?,
            };
            if !b.claim_local_cert_relay_seals_v18.contains_key(&(at, scope)) {
                let mut q = TirNativeRequestsV1::new();
                q.exact_v1::<BTreeNodeEnvelopeV1<(usize, Hash64), CertRelaySealV18>>(1).map_err(PalwStateV2Error::NativeResource)?;
                b.native_reservation.grow_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
            }
            if let Some(owner) = paid_ledger_v18(&b.state, id, &row.original)?
                && new.as_ref().is_some_and(|raw| raw.len() as u64 > owner.envelope.cert)
            {
                return Err(err(bad("owner18 certification exceeds its native owned envelope")));
            }
            let (mut owned, _owned_lease) = row.into_parts_v1();
            if let Some(new) = &new {
                let mut q = TirNativeRequestsV1::new();
                q.exact_v1::<u8>(new.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
                b.native_reservation.grow_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
                owned.approval = Some(new.clone());
                replace_pending(b, &owned, Some(rv))?;
            } else {
                // Any actual deletion severs this canonical instance, even if
                // undecided. An identical later recreation cannot reattach.
                owned.approval_attached = false;
                replace_pending(b, &owned, Some(rv))?;
            }
            // Deletion preserves the last actual decided snapshot as authority.
            resolve_exact_pending_v18(b, ctx, id, &owned.original)?;
            seal.sealed_end = b.entries.len();
            if let Some(old) = b.claim_local_cert_relay_seals_v18.get(&(at, scope)) {
                if *old != seal {
                    return Err(err(bad("owner18 certification observation replaced another instance")));
                }
            } else {
                b.claim_local_cert_relay_seals_v18.insert((at, scope), seal);
            }
        }
    }
    Ok(())
}
fn release_shared(b: &mut TransitionBuilder<'_>, row: &PendingV18) -> Result<(), PalwStateV2Error> {
    let old = shared::<ExecRefV18>(&b.state, EXEC_INDEX, row.exec.data)?.ok_or_else(|| err(bad("terminal18 released EXEC absent")))?;
    if old.material != row.exec || old.users == 0 {
        return Err(err(bad("terminal18 released EXEC differs")));
    }
    if old.users == 1 {
        put::<ExecRefV18>(b, EXEC_INDEX, None, row.exec.data, None).map_err(err)?;
        put::<ExecV18>(b, EXEC_BODIES, None, row.exec.data, None).map_err(err)?;
    } else {
        put(b, EXEC_INDEX, None, row.exec.data, Some(&ExecRefV18 { version: 1, material: row.exec, users: old.users - 1 }))
            .map_err(err)?;
    }
    if row.native_cohort.is_some() {
        return Ok(());
    }
    let old = shared::<CohortV18>(&b.state, COHORT_INDEX, row.cohort)?.ok_or_else(|| err(bad("terminal18 released cohort absent")))?;
    if old.users == 0 || old.members != row.members {
        return Err(err(bad("terminal18 released cohort differs")));
    }
    if old.users > 1 {
        put(
            b,
            COHORT_INDEX,
            None,
            row.cohort,
            Some(&CohortV18 {
                version: 1,
                execution_root: old.execution_root,
                birth: old.birth,
                complete: old.complete,
                members: old.members,
                users: old.users - 1,
            }),
        )
        .map_err(err)?;
        return Ok(());
    }
    // ONE traversal when the final owner releases; shared pending sources do
    // not each rebuild or delete the cohort population. Nodes remain immutable.
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Hash64>(row.members.rows).map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut keys = Vec::with_capacity(
        usize::try_from(row.members.rows).map_err(|_| PalwStateV2Error::Overflow("terminal18 shared removal count"))?,
    );
    nodes::complete_rows(&StateNodesV2(&b.state), row.members, |k, _| {
        keys.push(Hash64::from_bytes(k.try_into().map_err(|_| bad("terminal18 shared removal key"))?));
        Ok(())
    })
    .map_err(err)?;
    for k in keys {
        let member = shared_member_v18(&b.state, root(&b.state, COHORT_MEMBERS, Some(row.cohort)).map_err(err)?, k)?;
        if let Some(hash) = member.origin_f1 {
            release_original_f1_v18(b, member.birth, hash)?;
        }
        put::<MemberV18>(b, COHORT_MEMBERS, Some(row.cohort), k, None).map_err(err)?;
    }
    let mut owned = [0u8; 84];
    owned[0] = store::CONTEXT_CURRENT_PREFIX_V2;
    owned[1..3].copy_from_slice(&2u16.to_le_bytes());
    owned[3..67].copy_from_slice(row.cohort.as_byte_slice());
    spec(COHORT_MEMBERS).serialize(&mut &mut owned[67..]).map_err(|_| err(bad("terminal18 released current root key")))?;
    b.write_kernel_row_encoded_v1::<ComponentRootV2>(PALW_CLAIM_LOCAL_TABLE_V1, &owned, None)?;
    put::<CohortV18>(b, COHORT_INDEX, None, row.cohort, None).map_err(err)?;
    Ok(())
}
/// True only for the exact still-owned pending birth. Used by source lifetime
/// owners before releasing A8/Carrier9 original facts, never as paid authority.
pub(super) fn has_pending_v18(s: &PalwChainStateV2, id: Hash64, c: &PalwClaimStateV2) -> Result<bool, PalwStateV2Error> {
    Ok(lookup(s, id, c)?.is_some())
}
pub(super) fn validate_profiles_v18(s: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    // Registry18/schema1 allocates six global components and one owned cohort
    // component. Unknown profiles must not hide behind empty expected roots.
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&store::CONTEXT_CURRENT_PREFIX_V2) {
                continue;
            }
            let rooted = decode::<ComponentRootV2>(raw).map_err(err)?;
            let encoded_owner = match key.len() {
                18 => key.get(1),
                84 => key.get(67),
                _ => None,
            };
            if rooted.spec.owner != OWNER_V18 && encoded_owner != Some(&OWNER_V18) {
                continue;
            }
            let owned = matches!(rooted.spec.table, COHORT_MEMBERS | BY_CLAIM);
            if rooted.spec != spec(rooted.spec.table)
                || (!owned
                    && !matches!(
                        rooted.spec.table,
                        PENDING
                            | COHORT_INDEX
                            | CURRENT
                            | EXEC_INDEX
                            | EXEC_BODIES
                            | ORIGINAL_F1
                            | PAID_LEDGER
                            | super::palw_claim_local_native_cohort_baseline_v22::BASELINE
                            | super::palw_claim_local_native_cohort_baseline_v22::ROOTS
                    ))
            {
                return Err(err(bad("terminal18 unallocated component profile")));
            }
            let mut expected = [0u8; 84];
            expected[0] = store::CONTEXT_CURRENT_PREFIX_V2;
            let len = if owned {
                if key.len() != 84 {
                    return Err(err(bad("terminal18 owned key width")));
                }
                expected[1..3].copy_from_slice(&2u16.to_le_bytes());
                expected[3..67].copy_from_slice(&key[3..67]);
                84
            } else {
                18
            };
            let at = if owned { 67 } else { 1 };
            rooted.spec.serialize(&mut &mut expected[at..len]).map_err(|_| err(bad("terminal18 current key codec")))?;
            if key.as_slice() != &expected[..len] {
                return Err(err(bad("terminal18 current root grammar")));
            }
            if owned {
                let mut raw_scope = [0u8; 64];
                raw_scope.copy_from_slice(&key[3..67]);
                let scope = Hash64::from_bytes(raw_scope);
                if rooted.spec.table == BY_CLAIM {
                    if rooted.rows == 0 {
                        return Err(err(bad("terminal18 empty claim index root retained")));
                    }
                    nodes::complete_rows(&StateNodesV2(s), *rooted, |key, rv| {
                        let row = body::<PendingV18>(s, rv)?;
                        if !row.approval_attached
                            || row.id != scope
                            || key != self::key(row.id, &row.terminal)?.as_byte_slice()
                            || nodes::lookup(&StateNodesV2(s), root(s, PENDING, None)?, key)? != Some(rv)
                        {
                            return Err(bad("terminal18 orphan claim index row"));
                        }
                        Ok(())
                    })
                    .map_err(err)?;
                    continue;
                }
                let row =
                    shared::<CohortV18>(s, COHORT_INDEX, scope)?.ok_or_else(|| err(bad("terminal18 orphan owned cohort profile")))?;
                if row.members != *rooted || cohort_key(row.execution_root, row.birth).map_err(err)? != scope {
                    return Err(err(bad("terminal18 owned profile differs from indexed cohort")));
                }
            }
        }
    }
    Ok(())
}
pub(super) fn validate_pending_v18(s: &PalwChainStateV2, p: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    validate_profiles_v18(s)?;
    let mut callback_error = None;
    let result = nodes::complete_rows(&StateNodesV2(s), root(s, PENDING, None).map_err(err)?, |key, rv| {
        let row = body::<PendingV18>(s, rv)?;
        if key != self::key(row.id, &row.terminal)?.as_byte_slice() {
            return Err(bad("terminal18 directory key differs"));
        }
        if s.last_point.is_none_or(|point| row.removed.blue_score > point.blue_score || row.removed.daa_score > point.daa_score) {
            return Err(bad("terminal18 future removal fact"));
        }
        check_record(s, p, rv, &row).and_then(|()| check_attached_cert_current_v18(s, &row)).map_err(|e| {
            callback_error = Some(e);
            bad("terminal18 persisted semantic validation")
        })
    });
    if let Some(e) = callback_error {
        return Err(e);
    }
    result.map_err(err)?;
    validate_current_v18(s, p)?;
    validate_shared_v18(s, p)?;
    validate_original_refs_v18(s)?;
    validate_paid_ledgers_v18(s)?;
    super::palw_claim_local_native_cohort_baseline_v22::validate_native_baseline_v22(s, p)
}
pub(super) fn validate_network_context_v18(
    s: &PalwChainStateV2,
    global: &crate::config::params::Params,
) -> Result<(), PalwStateV2Error> {
    let mut callback = None;
    let result = nodes::complete_rows(&StateNodesV2(s), root(s, PENDING, None).map_err(err)?, |_, rv| {
        let row = body::<PendingV18>(s, rv)?;
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &global.palw_consensus_mode else {
            return Err(bad("terminal18 pending requires configured V2 network"));
        };
        if row.ruleset != global.consensus_params_id() {
            return Err(bad("terminal18 pending differs from actual configured native ruleset"));
        }
        if let Err(e) = check_record(s, &bundle.state, rv, &row) {
            callback = Some(e);
            return Err(bad("terminal18 configured native source invalid"));
        }
        Ok(())
    });
    if let Some(e) = callback {
        return Err(e);
    }
    result.map_err(err)?;
    nodes::complete_rows(&StateNodesV2(s), root(s, PAID_LEDGER, None).map_err(err)?, |_, rv| {
        let row = body::<PaidLedgerV18>(s, rv)?;
        if row.ruleset != global.consensus_params_id() {
            return Err(bad("owner18 paid native ledger belongs to another ruleset"));
        }
        Ok(())
    })
    .map_err(err)?;
    super::palw_claim_local_native_cohort_baseline_v22::validate_network_v22(s, global)
}
fn check_attached_cert_current_v18(s: &PalwChainStateV2, row: &PendingV18) -> Result<(), PalwStateV2Error> {
    if !row.approval_attached {
        return Ok(());
    }
    use crate::palw_security_certification_v1 as cert;
    let mut key = [0u8; 65];
    key[0] = cert::CERTIFICATION_CLAIM_PREFIX_V1;
    key[1..].copy_from_slice(&cert::palw_certification_id_v1(&row.id));
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(65).map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let actual = s.kernel_route.as_ref().and_then(|r| r.aux.get(&(cert::PALW_SECURITY_CERTIFICATION_TABLE_V1, key.to_vec())));
    if actual.map(Vec::as_slice) != row.approval.as_deref() {
        return Err(err(bad("terminal18 attached selected canonical instance lost actual current bytes")));
    }
    Ok(())
}
fn check_current_at_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    row: &CurrentV18,
    executing: Option<PalwBlockContextV2>,
) -> Result<(), PalwStateV2Error> {
    if row.version != 1
        || s.claims.get(&row.id) != Some(&row.current)
        || !p.claim_local_settlement_active_at(row.point.daa_score)
        || row.point.blue_score < row.birth.blue_score
        || row.point.daa_score < row.birth.daa_score
        || executing.or(s.last_point).is_none_or(|tip| row.point.blue_score > tip.blue_score || row.point.daa_score > tip.daa_score)
    {
        return Err(err(bad("terminal18 current actual row/clock differs")));
    }
    let origin = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(s, p, row.id, &row.current)?
        .ok_or_else(|| err(bad("terminal18 current original absent")))?;
    if origin.accepting_point() != row.birth || !same_identity(origin.original_claim(), &row.current) {
        return Err(err(bad("terminal18 current source differs")));
    }
    match (row.origin_kind, row.origin_f1) {
        (1, Some(want)) if verified_native_legacy_f1_v25(s, p, row.id, row.birth, origin.original_claim())? == Some(want) => {}
        (2, None) if super::palw_claim_local_source_admission_v8::has_source_admission_v8(s, row.id, &row.current)? => {}
        _ => return Err(err(bad("terminal18 current original codec/identity"))),
    }
    Ok(())
}
fn current_row_at_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
    executing: Option<PalwBlockContextV2>,
) -> Result<Option<(RowValueV2, NativeRetainedV1<CurrentV18>)>, PalwStateV2Error> {
    let reader = StateNodesV2(s);
    let Some(rv) =
        nodes::lookup(&reader, root(s, CURRENT, None).map_err(err)?, key(id, c).map_err(err)?.as_byte_slice()).map_err(err)?
    else {
        return Ok(None);
    };
    let row = body::<CurrentV18>(s, rv).map_err(err)?;
    check_current_at_v18(s, p, &row, executing)?;
    if row.id != id || row.current != *c {
        return Err(err(bad("terminal18 current requested subject differs")));
    }
    Ok(Some((rv, row)))
}
fn validate_current_v18(s: &PalwChainStateV2, p: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    let mut callback = None;
    let result = nodes::complete_rows(&StateNodesV2(s), root(s, CURRENT, None).map_err(err)?, |k, rv| {
        let row = body::<CurrentV18>(s, rv)?;
        if k != key(row.id, &row.current)?.as_byte_slice() {
            return Err(bad("terminal18 current directory key"));
        }
        if let Err(e) = check_current_at_v18(s, p, &row, None) {
            callback = Some(e);
            return Err(bad("terminal18 current native validation"));
        }
        Ok(())
    });
    if let Some(e) = callback {
        return Err(e);
    }
    result.map_err(err)
}
/// Typed current material supplements the exact immutable original. This is
/// never decoded as F1, nor does its point become the original accepting point.
pub(super) fn visit_current_checkpoint_material_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
    f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let Some((rv, row)) = current_row_at_v18(s, p, id, c, None)? else { return Ok(false) };
    current_material_v18(s, rv, &row, f)?;
    Ok(true)
}
fn current_material_v18(
    s: &PalwChainStateV2,
    rv: RowValueV2,
    row: &CurrentV18,
    mut f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let id = row.id;
    if row.origin_kind == 1 {
        let key = crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(row.birth.block);
        let _lease = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
        let original = s
            .kernel_route
            .as_ref()
            .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())))
            .ok_or_else(|| err(bad("terminal18 original F1 material absent")))?;
        f(original)?;
    }
    let header = encode(&(1u16, id, rv)).map_err(err)?;
    f(&header)?;
    let raw = StateNodesV2(s).body(rv).map_err(err)?;
    f(&raw)?;
    // Canonical cert bytes (or explicit missing) retain their own authority.
    let cert = rewards::cert_bytes(s.kernel_route.as_ref(), id);
    let material = encode(&(1u16, id, cert)).map_err(err)?;
    f(&material)?;
    Ok(())
}
/// Allocation inventory only: exact same retained byte outputs as the strict
/// material visitor, without performing params-dependent semantic admission.
/// Malformed rows keep their normal constructor error after capacity admission.
pub(super) fn quote_current_checkpoint_material_v18(
    s: &PalwChainStateV2,
    id: Hash64,
    c: &PalwClaimStateV2,
    f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let reader = StateNodesV2(s);
    let Some(rv) =
        nodes::lookup(&reader, root(s, CURRENT, None).map_err(err)?, key(id, c).map_err(err)?.as_byte_slice()).map_err(err)?
    else {
        return Ok(false);
    };
    let row = body::<CurrentV18>(s, rv).map_err(err)?;
    current_material_v18(s, rv, &row, f)?;
    Ok(true)
}

/// The ONLY in-flight clock exception is tied to the actual owning builder's
/// sealed executing point. Cold and readonly state getters retain the tip check.
/// No LastPoint is changed here, and no caller-provided DAA alone is authority.
pub(super) fn claim_checkpoint_overlay_ready_in_fold_v18(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<bool, PalwStateV2Error> {
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(err(bad("terminal18 current overlay lacks actual executing point")));
    }
    let Some((rv, row)) = current_row_at_v18(&b.state, b.params, id, c, Some(*ctx))? else { return Ok(false) };
    current_material_v18(&b.state, rv, &row, |_| Ok(()))?;
    Ok(true)
}
/// Pending metadata/EXEC/cohort are already A2-pinned; expose the exact local
/// immutable bodies needed by the held dependency reader, without world copies.
pub(super) fn visit_removed_checkpoint_material_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
    mut f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let Some((rv, row)) = lookup(s, id, c)? else { return Ok(false) };
    check_record(s, p, rv, &row)?;
    check_attached_cert_current_v18(s, &row)?;
    let pins = inventory(&row, rv).map_err(err)?;
    let header = encode(&(1u16, id, rv, &*pins)).map_err(err)?;
    f(&header)?;
    for value in &pins.literals {
        let raw = StateNodesV2(s).body(*value).map_err(err)?;
        f(&raw)?;
    }
    Ok(true)
}
// Live-current coverage uses the strict native checkpoint material visitor.
// Whole-fold suppression additionally needs every other native owner and the
// removed-source overlay; this getter cannot authorize suppressing F1 alone.
pub(super) fn claim_checkpoint_overlay_ready_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<bool, PalwStateV2Error> {
    visit_current_checkpoint_material_v18(s, p, id, c, |_| Ok(()))
}
pub(super) fn removed_checkpoint_overlay_ready_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<bool, PalwStateV2Error> {
    super::palw_claim_local_retired_checkpoint_v23::removed_checkpoint_overlay_ready_v23(s, p, id, c, ctx)
}

/// Partial event coverage for the F1 route selector. OTHER native owners must
/// independently prove their event coverage; this never authorizes a skip alone.
pub(super) fn covers_claim_changes_v18(b: &TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<bool, PalwStateV2Error> {
    let n = b.entries.len();
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(n as u64)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut seen = BTreeSet::new();
    for e in b.entries.iter().rev() {
        let PalwDeltaEntryV2::Claim { key: id, old, new } = e else { continue };
        if !seen.insert(*id) {
            continue;
        }
        match (old, new) {
            (_, Some(c)) => {
                let Some(row) = shared::<CurrentV18>(&b.state, CURRENT, key(*id, c).map_err(err)?)? else { return Ok(false) };
                if row.current != *c || row.point != *ctx || row.id != *id {
                    return Ok(false);
                }
            }
            (Some(c), None) => {
                if let Some((rv, row)) = lookup(&b.state, *id, c)? {
                    check_record(&b.state, b.params, rv, &row)?;
                    if row.removed != *ctx || row.terminal != *c {
                        return Ok(false);
                    }
                } else {
                    let Some(closed) = super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(&b.state, *id, c)?
                    else {
                        return Ok(false);
                    };
                    if closed.closed_at != *ctx || !same_identity(&closed.original, c) || closed.terminal_phase != c.phase {
                        return Ok(false);
                    }
                }
            }
            (None, None) => return Err(err(bad("terminal18 empty actual claim mutation"))),
        }
    }
    Ok(true)
}
// Cold/global integrity only: exact refcounts and each shared member population
// are checked once. A normal source read performs point checks, not N copies of
// the same cohort. This pass never mints receipts or upgrades incompleteness.
fn validate_shared_v18(s: &PalwChainStateV2, p: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    let n = root(s, PENDING, None).map_err(err)?.rows;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, u64>>(
        n.checked_mul(2).ok_or(PalwStateV2Error::Overflow("terminal18 restore references"))?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut cohorts = BTreeMap::<Hash64, u64>::new();
    let mut execs = BTreeMap::<Hash64, u64>::new();
    nodes::complete_rows(&StateNodesV2(s), root(s, PENDING, None).map_err(err)?, |_, rv| {
        let row = body::<PendingV18>(s, rv)?;
        for (map, k) in
            std::iter::once((&mut execs, row.exec.data)).chain(row.native_cohort.is_none().then_some((&mut cohorts, row.cohort)))
        {
            let n = map.entry(k).or_default();
            *n = n.checked_add(1).ok_or_else(|| bad("terminal18 reference count overflow"))?;
        }
        Ok(())
    })
    .map_err(err)?;
    let mut callback_error = None;
    let result = nodes::complete_rows(&StateNodesV2(s), root(s, COHORT_INDEX, None).map_err(err)?, |k, rv| {
        let row = body::<CohortV18>(s, rv)?;
        let id = cohort_key(row.execution_root, row.birth)?;
        if k != id.as_byte_slice()
            || row.version != 1
            || row.users == 0
            || cohorts.remove(&id) != Some(row.users)
            || root(s, COHORT_MEMBERS, Some(id))? != row.members
        {
            return Err(bad("terminal18 shared cohort census/refcount"));
        }
        nodes::complete_rows(&StateNodesV2(s), row.members, |k, rv| {
            let m = body::<MemberV18>(s, rv)?;
            if k != key(m.id, &m.original)?.as_byte_slice()
                || m.original.execution_root != row.execution_root
                || m.original.accepted_daa > row.birth.daa_score
                || m.original.accepted_blue_score > row.birth.blue_score
            {
                return Err(bad("terminal18 shared member scope"));
            }
            let authority = (|| {
                if let Some(origin) = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(s, p, m.id, &m.original)? {
                    return Ok(same_identity(origin.original_claim(), &m.original)
                        && origin.accepting_point().daa_score <= row.birth.daa_score
                        && origin.accepting_point().blue_score <= row.birth.blue_score);
                }
                if let Some(closed) = super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(s, m.id, &m.original)? {
                    return Ok(same_identity(&closed.original, &m.original)
                        && closed.birth.daa_score <= row.birth.daa_score
                        && closed.birth.blue_score <= row.birth.blue_score);
                }
                if let Some(closed) = super::palw_claim_local_compact_sources_v1::read(s, m.id)? {
                    return Ok(same_identity(&closed.original, &m.original)
                        && closed.birth.daa_score <= row.birth.daa_score
                        && closed.birth.blue_score <= row.birth.blue_score);
                }
                Ok(false)
            })();
            match authority {
                Ok(true) => {}
                Ok(false) => return Err(bad("terminal18 shared member original authority absent")),
                Err(e) => {
                    callback_error = Some(e);
                    return Err(bad("terminal18 shared member authority refused"));
                }
            }
            Ok(())
        })?;
        Ok(())
    });
    if let Some(e) = callback_error {
        return Err(e);
    }
    result.map_err(err)?;
    if !cohorts.is_empty() {
        return Err(err(bad("terminal18 pending cohort descriptor absent")));
    }
    nodes::complete_rows(&StateNodesV2(s), root(s, EXEC_INDEX, None).map_err(err)?, |k, rv| {
        let row = body::<ExecRefV18>(s, rv)?;
        if k != row.material.data.as_byte_slice()
            || row.version != 1
            || row.users == 0
            || execs.remove(&row.material.data) != Some(row.users)
        {
            return Err(bad("terminal18 shared EXEC refcount"));
        }
        let reader = StateNodesV2(s);
        if nodes::lookup(&reader, root(s, EXEC_BODIES, None)?, k)? != Some(row.material) {
            return Err(bad("terminal18 shared EXEC literal missing"));
        }
        let _: NativeRetainedV1<ExecV18> = body(s, row.material)?;
        Ok(())
    })
    .map_err(err)?;
    if !execs.is_empty() || root(s, EXEC_BODIES, None).map_err(err)?.rows != root(s, EXEC_INDEX, None).map_err(err)?.rows {
        return Err(err(bad("terminal18 orphan/shared EXEC population")));
    }
    Ok(())
}
// Cold-only exact census: a single map covers references, not one copied
// historical world per owner. Normal retain/release/GC use point lookups only.
fn note_original_ref_v18(
    refs: &mut BTreeMap<Hash64, (PalwBlockContextV2, Hash64, u64)>,
    point: PalwBlockContextV2,
    digest: Hash64,
) -> Result<(), ContextErrorV2> {
    let row = refs.entry(point.block).or_insert((point, digest, 0));
    if (row.0, row.1) != (point, digest) {
        return Err(bad("terminal18 original frame aliases disagree"));
    }
    row.2 = row.2.checked_add(1).ok_or_else(|| bad("terminal18 original frame count overflow"))?;
    Ok(())
}
fn validate_original_refs_v18(s: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    let mut count = root(s, CURRENT, None)
        .map_err(err)?
        .rows
        .checked_add(root(s, PENDING, None).map_err(err)?.rows)
        .ok_or(PalwStateV2Error::Overflow("terminal18 original owner count"))?;
    nodes::complete_rows(&StateNodesV2(s), root(s, COHORT_INDEX, None).map_err(err)?, |_, rv| {
        let row = body::<CohortV18>(s, rv)?;
        count = count.checked_add(row.members.rows).ok_or_else(|| bad("terminal18 original cohort count overflow"))?;
        Ok(())
    })
    .map_err(err)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, (PalwBlockContextV2, Hash64, u64)>>(count)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut refs = BTreeMap::new();
    nodes::complete_rows(&StateNodesV2(s), root(s, CURRENT, None).map_err(err)?, |_, rv| {
        let row = body::<CurrentV18>(s, rv)?;
        if let Some(digest) = row.origin_f1 {
            note_original_ref_v18(&mut refs, row.birth, digest)?;
        }
        Ok(())
    })
    .map_err(err)?;
    nodes::complete_rows(&StateNodesV2(s), root(s, PENDING, None).map_err(err)?, |_, rv| {
        let row = body::<PendingV18>(s, rv)?;
        if let Some(digest) = row.origin_f1 {
            note_original_ref_v18(&mut refs, row.birth, digest)?;
        }
        Ok(())
    })
    .map_err(err)?;
    nodes::complete_rows(&StateNodesV2(s), root(s, COHORT_INDEX, None).map_err(err)?, |_, rv| {
        let row = body::<CohortV18>(s, rv)?;
        nodes::complete_rows(&StateNodesV2(s), row.members, |_, rv| {
            let member = body::<MemberV18>(s, rv)?;
            if let Some(digest) = member.origin_f1 {
                note_original_ref_v18(&mut refs, member.birth, digest)?;
            }
            Ok(())
        })?;
        Ok(())
    })
    .map_err(err)?;
    let mut callback = None;
    let result = nodes::complete_rows(&StateNodesV2(s), root(s, ORIGINAL_F1, None).map_err(err)?, |key, rv| {
        let row = body::<OriginalF1V18>(s, rv)?;
        if row.version != 1
            || row.users == 0
            || key != row.point.block.as_byte_slice()
            || refs.remove(&row.point.block) != Some((row.point, row.digest, row.users))
        {
            return Err(bad("terminal18 exact original frame census differs"));
        }
        match actual_f1_identity(s, row.point) {
            Ok(digest) if digest == row.digest => {}
            Ok(_) => return Err(bad("terminal18 pinned original bytes changed")),
            Err(e) => {
                callback = Some(e);
                return Err(bad("terminal18 original body unavailable"));
            }
        }
        Ok(())
    });
    if let Some(e) = callback {
        return Err(e);
    }
    result.map_err(err)?;
    if !refs.is_empty() {
        return Err(err(bad("terminal18 original reference missing")));
    }
    Ok(())
}

impl ExecV18 {
    fn validate_shape(&self, terminal: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        let Some(root) = &self.root else {
            if !self.slices.is_empty() || !self.inputs.is_empty() {
                return Err(err(bad("terminal18 absent EXEC root has captured children")));
            }
            return Ok(());
        };
        if root.root_bond != terminal.bond
            || root.class_id != terminal.class_id
            || root.anchor != terminal.accepted_block
            || root.canonical_job_id != terminal.job_identity
            || self.slices.len() > root.slice_count() as usize
            || self.inputs.len()
                != self.slices.len().checked_add(1).ok_or(PalwStateV2Error::Overflow("terminal18 captured EXEC population"))?
        {
            return Err(err(bad("terminal18 captured EXEC owner/population differs")));
        }
        for (i, (index, _)) in self.slices.iter().enumerate() {
            if *index as usize != i {
                return Err(err(bad("terminal18 captured slice order")));
            }
        }
        for (want, input) in
            std::iter::once(root.prefix_claim).chain(self.slices.iter().map(|(_, s)| s.evidence_root)).zip(&self.inputs)
        {
            if want != input.claim {
                return Err(err(bad("terminal18 captured native claim differs")));
            }
            if let Some(a) = &input.authority {
                if a.version != 2
                    || a.enrollment.version != 2
                    || a.enrollment.base_claim != want.as_bytes()
                    || a.enrollment.class != a.class.id()
                    || a.enrollment.job != a.job.id()
                {
                    return Err(err(bad("terminal18 captured paid authority differs")));
                }
                if let Some(row) = &input.row
                    && (a.class.base_class != row.class_binding_id
                        || a.job.base_job != row.job_id
                        || a.enrollment.producer != row.producer
                        || a.authorized_daa != row.committed_daa
                        || row.liability_until != Some(a.enrollment.lifetime.liability_until))
                {
                    return Err(err(bad("terminal18 captured paid/native binding differs")));
                }
                if let Some(f) = &input.final_row
                    && (f.enrollment != a.enrollment.id()
                        || f.base_claim != want.as_bytes()
                        || f.verification_class != a.enrollment.class
                        || f.signed_lifetime != a.enrollment.lifetime
                        || f.complete_receipts == 0
                        || f.completed_rounds == 0)
                {
                    return Err(err(bad("terminal18 captured selected Final differs")));
                }
                if let Some(lock) = &input.lock
                    && lock.enrollment != a.enrollment.id()
                {
                    return Err(err(bad("terminal18 captured selected lock differs")));
                }
            } else if input.final_row.is_some() || input.lock.is_some() {
                return Err(err(bad("terminal18 selected data without actual paid authority")));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod profile_tests_v18 {
    use super::*;
    fn metadata_state(profile: ComponentSpecV2, owned: bool) -> PalwChainStateV2 {
        let mut s = PalwChainStateV2::genesis();
        let mut route = crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2)),
            None,
            misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 0, burned: 0 },
        );
        let mut key = if owned {
            let mut k = vec![0u8; 84];
            k[0] = b'I';
            k[1..3].copy_from_slice(&2u16.to_le_bytes());
            k
        } else {
            let mut k = vec![0u8; 18];
            k[0] = b'I';
            k
        };
        let at = if owned { 67 } else { 1 };
        profile.serialize(&mut &mut key[at..]).unwrap();
        route.aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, key), borsh::to_vec(&ComponentRootV2::empty(profile).unwrap()).unwrap());
        s.kernel_route = Some(route);
        s
    }
    #[test]
    fn empty_pending_does_not_hide_unknown_or_orphan_owner18_profiles() {
        for table in [PENDING, COHORT_INDEX, CURRENT, EXEC_INDEX, EXEC_BODIES, ORIGINAL_F1] {
            assert!(validate_profiles_v18(&metadata_state(spec(table), false)).is_ok());
        }
        for field in 0..4 {
            let mut unsupported = spec(PENDING);
            match field {
                0 => unsupported.table = 12,
                1 => unsupported.schema = 2,
                2 => unsupported.key_bytes = 63,
                _ => unsupported.value_bytes = 0,
            };
            let s = metadata_state(unsupported, false);
            assert_eq!(root(&s, PENDING, None).unwrap().rows, 0);
            assert!(validate_profiles_v18(&s).is_err());
        }
        assert!(
            validate_profiles_v18(&metadata_state(spec(COHORT_MEMBERS), false)).is_err(),
            "cohort members allocate only the owned-current grammar"
        );
        assert!(
            validate_profiles_v18(&metadata_state(spec(COHORT_MEMBERS), true)).is_err(),
            "a syntactically valid orphan root needs its actual indexed cohort"
        );
        assert!(
            validate_profiles_v18(&metadata_state(spec(PENDING), true)).is_err(),
            "global directories allocate no owned-current grammar"
        );
    }
}

/// Pre-sign finite capacity only. This does NOT assert an accepted birth,
/// Source15 alias, paid reservation, source ownership, approval or future block.
/// OAR2 signs these canonical widths/rules; actual capture verifies real rows.
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
pub(super) struct Owner18ComponentContributionV1 {
    slot: u8,
    spec: ComponentSpecV2,
    owned: bool,
    body: u64,
    alias_group: u8,
}
crate::palw_native_resource_v1::wire::wire_struct!(Owner18ComponentContributionV1;u8,ComponentSpecV2,bool,u64,u8);
crate::palw_native_resource_v1::native_struct!(Owner18ComponentContributionV1;slot,spec,owned,body,alias_group);
impl Owner18ComponentContributionV1 {
    pub(super) fn slot_v18(&self) -> u8 {
        self.slot
    }
    pub(super) fn spec_v18(&self) -> ComponentSpecV2 {
        self.spec
    }
    pub(super) fn owned_v18(&self) -> bool {
        self.owned
    }
    pub(super) fn body_bytes_v18(&self) -> u64 {
        self.body
    }
    // Only identical groups share the SAME canonical body in native writers.
    pub(super) fn body_alias_group_v18(&self) -> u8 {
        self.alias_group
    }
}
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
pub(super) struct NativeOwner18FpEnvelopeV1 {
    version: u16,
    fp_cap: u32,
    cert_draws: u16,
    claim_live: u64,
    claim_terminal: u64,
    cert: u64,
    rows: [Owner18ComponentContributionV1; 9],
    a2: u64,
    alias_width: u64,
    exec: u64,
    modern_alias: bool,
    cohort: cohort15::NativeBirthContributionGeometryV18,
}
crate::palw_native_resource_v1::wire::wire_struct!(NativeOwner18FpEnvelopeV1;u16,u32,u16,u64,u64,u64,[Owner18ComponentContributionV1;9],u64,u64,u64,bool,cohort15::NativeBirthContributionGeometryV18);
crate::palw_native_resource_v1::native_struct!(NativeOwner18FpEnvelopeV1;version,fp_cap,cert_draws,claim_live,claim_terminal,cert,rows,a2,alias_width,exec,modern_alias,cohort);
impl NativeOwner18FpEnvelopeV1 {
    pub(super) fn contributions_v18(&self) -> &[Owner18ComponentContributionV1; 9] {
        &self.rows
    }
    pub(super) fn birth_contribution_v18(&self) -> &cohort15::NativeBirthContributionGeometryV18 {
        &self.cohort
    }
    pub(super) fn a2_bytes_v18(&self) -> u64 {
        self.a2
    }
    pub(super) fn native_alias_codec_bytes_v18(&self) -> u64 {
        self.alias_width
    }
    pub(super) fn empty_exec_codec_bytes_v18(&self) -> u64 {
        self.exec
    }
    pub(super) fn current_claim_bytes_v18(&self) -> u64 {
        self.claim_live
    }
    pub(super) fn terminal_claim_bytes_v18(&self) -> u64 {
        self.claim_terminal
    }
    pub(super) fn selected_cert_bytes_v18(&self) -> u64 {
        self.cert
    }
    pub(super) fn expects_modern_alias_v18(&self) -> bool {
        self.modern_alias
    }
    pub(super) fn binding_v18(&self) -> Result<Hash64, PalwStateV2Error> {
        hash(18, self).map_err(err)
    }
}
/// Scope is the genuine opaque FP preparation plus configured native codec
/// caps. Actual point VALUES/allocation IDs/unknown future block are excluded
/// from the envelope: only their canonical fixed widths are measured.
pub(super) fn prepare_fp_owner18_envelope_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    req: &super::palw_tir_public_fold_v1::PreparedOriginalRequirementsV1,
) -> Result<NativeOwner18FpEnvelopeV1, PalwStateV2Error> {
    fn add(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
        a.checked_add(b).ok_or(PalwStateV2Error::Overflow("owner18 finite codec envelope"))
    }
    fn mul(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
        a.checked_mul(b).ok_or(PalwStateV2Error::Overflow("owner18 finite codec envelope"))
    }
    fn len<T: BorshSerialize>(v: &T) -> Result<u64, PalwStateV2Error> {
        borsh::object_length(v).map(|n| n as u64).map_err(|_| err(bad("owner18 finite codec shape")))
    }
    let original = req.frame_v1().original_v1();
    let plan = req.frame_v1().plan_v1();
    if crate::palw_freeprompt_v3::fp_claim_id_v3(original) != plan.original_claim {
        return Err(err(bad("owner18 envelope needs native FP original")));
    }
    let point = len(ctx)?;
    let rv = add(len(&Hash64::default())?, len(&0u64)?)?;
    let root = add(len(&spec(CURRENT))?, add(1 + 64, 8)?)?;
    let fresh = super::palw_tir_public_fold_v1::native_fresh_fp_claim_codec_v11();
    let cap = b.params.fp_max_quanta_per_receipt;
    if cap == 0 {
        return Err(err(bad("owner18 native FP cap absent")));
    }
    // Native spent entries are u32 and bounded by the accepted quanta<=cap.
    // Rebound adds8; the maximal native ShardPart licence-door adds3. DefaultDisputed cannot nest:
    // da_sessions rejects already-disputed (lines39–43), so one extra fixed DA
    // wrapper surrounds at most a nonrecursive9-byte resumed phase. Use the
    // larger10-byte terminal variant as conservative native enum maximum.
    let door = crate::palw_economic_safety_v1::PalwLicenceDoorTagV1::ShardPart { quorum_per_shard: 1 };
    let door_extra = len(&Some(door))?
        .checked_sub(len(&Option::<crate::palw_economic_safety_v1::PalwLicenceDoorTagV1>::None)?)
        .ok_or(PalwStateV2Error::Overflow("owner18 licence door shape"))?;
    let variable = add(mul(cap as u64, 4)?, add(8, door_extra)?)?;
    let phase = 1 + 8 + 4 + len(&PalwBondKeyV2(original.job.executor_bond))? + 16 + 10;
    let live = add(add(fresh, variable)?, phase - 1)?;
    let terminal = add(add(fresh, variable)?, 10 - 1)?;
    let m = b.params.rfc0018_v1().filter(|mirror| mirror.gate_c_governs(ctx.daa_score)).map_or(0, |mirror| mirror.certification.m);
    // Exact row field stream: four digests; attempt/final/commit/span; maximal
    // anchor; vectors; deadline pair; held reward/payout; decided outcome.
    // At most m draws and m distinct attestors (native attestation duplicate
    // rejection). Stored attestations have no signatures: digest,verdict,digest,DAA.
    let cert = if m == 0 { 0 } else { add(460, mul(m as u64, 64 + 137)?)? };
    let modern = original.execution_root != Hash64::default();
    let alias = add(mul(rv, 2)?, root)?;
    let current = add(2 + 64 + 1 + 1, add(mul(point, 2)?, live)?)?; // origin_f1=None for new native A8 FP
    let approval = if cert == 0 { 1 } else { add(1 + 4, cert)? };
    let pending_fixed = 2 + 1 + 64 + 8 + 32 + 1 + 65 + 8 + 1 + 1 + 64 + 1 + 1;
    let pending = add(
        add(pending_fixed, add(mul(point, 2)?, add(fresh, terminal)?)?)?,
        add(approval, add(if modern { alias } else { 0 }, add(root, rv)?)?)?,
    )?;
    // FP can never own native EXEC: open_root_v2 only accepts Attempt. Its
    // captured root/reward/policy are None, and slice/input vectors empty.
    let exec = 11;
    let exec_ref = 2 + rv + 8;
    let cohort = if modern { 0 } else { 2 + 64 + point + 1 + root + 8 };
    let baseline = super::palw_claim_local_native_cohort_baseline_v22::contribution_shapes_v22()?;
    let rows = [
        Owner18ComponentContributionV1 { slot: 1, spec: spec(CURRENT), owned: false, body: current, alias_group: 1 },
        Owner18ComponentContributionV1 { slot: 2, spec: spec(PENDING), owned: false, body: pending, alias_group: 2 },
        Owner18ComponentContributionV1 { slot: 3, spec: spec(BY_CLAIM), owned: true, body: pending, alias_group: 2 },
        Owner18ComponentContributionV1 { slot: 4, spec: spec(EXEC_INDEX), owned: false, body: exec_ref, alias_group: 3 },
        Owner18ComponentContributionV1 { slot: 5, spec: spec(EXEC_BODIES), owned: false, body: exec, alias_group: 4 },
        Owner18ComponentContributionV1 { slot: 6, spec: spec(COHORT_INDEX), owned: false, body: cohort, alias_group: 5 },
        Owner18ComponentContributionV1 { slot: 7, spec: spec(PAID_LEDGER), owned: false, body: 0, alias_group: 6 },
        Owner18ComponentContributionV1 {
            slot: 8,
            spec: baseline[0].0,
            owned: false,
            body: if modern { baseline[0].1 } else { 0 },
            alias_group: 7,
        },
        Owner18ComponentContributionV1 {
            slot: 9,
            spec: baseline[1].0,
            owned: false,
            body: if modern { baseline[1].1 } else { 0 },
            alias_group: 8,
        },
    ];
    let a2 = add(2 + 4 + 4, add(root, mul(if modern { 4 } else { 2 }, rv)?)?)?;
    let cohort = cohort15::quote_birth_contribution_scoped_geometry_v15(req)?;
    let mut envelope = NativeOwner18FpEnvelopeV1 {
        version: 1,
        fp_cap: cap,
        cert_draws: m,
        claim_live: live,
        claim_terminal: terminal,
        cert,
        rows,
        a2,
        alias_width: if modern { alias } else { 0 },
        exec,
        modern_alias: modern,
        cohort,
    };
    // Row9 embeds the envelope. All dimensions use fixed-width u64 encoding,
    // so measuring once with a zero width and filling that width does not
    // recurse or depend on a future value. This is canonical shape only.
    let encoded = len(&envelope)?;
    envelope.rows[6].body = add(2 + 64 + 32, add(point, add(4 * 64, encoded)?)?)?;
    if len(&envelope)? != encoded {
        return Err(err(bad("owner18 fixed envelope width changed")));
    }
    Ok(envelope)
}

/// Native one-time eligibility-ledger ownership. It conveys no material rent
/// credit, paid-alias standing or new use authority. Minted only by actual OAR2
/// admission's opaque capacity token; later updates use this native owner.
#[derive(BorshSerialize, BorshDeserialize)]
struct PaidLedgerV18 {
    version: u16,
    id: Hash64,
    birth: PalwBlockContextV2,
    source: Hash64,
    allocation: Hash64,
    resource: Hash64,
    contract: Hash64,
    ruleset: crate::Hash,
    envelope: NativeOwner18FpEnvelopeV1,
}
crate::palw_native_resource_v1::wire::wire_struct!(PaidLedgerV18;u16,Hash64,PalwBlockContextV2,Hash64,Hash64,Hash64,Hash64,crate::Hash,NativeOwner18FpEnvelopeV1);
crate::palw_native_resource_v1::native_struct!(PaidLedgerV18;version,id,birth,source,allocation,resource,contract,ruleset,envelope);

/// Initial OAR2 publication only. Opaque actual capacity is checked before any
/// new owner18 Current/ownership copy. Later native-ledger updates do not ask
/// for a live Source17 receipt and gain no material paid-alias authority.
pub(super) fn capture_initial_mixed_ledger_v21(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    cap: &super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::NativeMixedArchiveCapacityV21,
) -> Result<(), PalwStateV2Error> {
    if b.claim_local_lock_point_v6 != Some(*ctx) || cap.point_v21() != *ctx {
        return Err(err(bad("owner18 initial capacity lacks actual executing point")));
    }
    let id = cap.claim_v21();
    let source = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&b.state, id, *ctx)?
        .ok_or_else(|| err(bad("owner18 initial native Source8 birth absent")))?;
    if !matches!(source.original_claim().source, PalwClaimSourceV2::FreePrompt { .. }) {
        return Err(err(bad("owner18 OAR2 requires actual FP source")));
    }
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state, id)
        .ok_or_else(|| err(bad("owner18 initial native Frame absent")))?;
    let plan = frame.plan_v1();
    if plan.original_claim != id || frame.original_v1().execution_root != source.original_claim().execution_root {
        return Err(err(bad("owner18 initial Frame differs from native source")));
    }
    let e = *cap.owner18_envelope_v21();
    cap.covers_owner_scope_v21(id, *ctx, plan.source_id, plan.allocation_id, plan.resource_plan_id, &e)?;
    if e.expects_modern_alias_v18() {
        let alias = cohort15::alias_original_snapshot_v15(&b.state, id, source.original_claim())?
            .ok_or_else(|| err(bad("owner18 initial actual Source15 snapshot absent")))?;
        if cohort15::check_native_alias_v15(&b.state, &alias, id, source.original_claim())? != *ctx {
            return Err(err(bad("owner18 initial Source15 accepting point differs")));
        }
    }
    let scope = key(id, source.original_claim()).map_err(err)?;
    if let Some(old) = shared::<PaidLedgerV18>(&b.state, PAID_LEDGER, scope)? {
        if old.version != 1
            || old.id != id
            || old.birth != *ctx
            || old.source != cap.source_v21()
            || old.allocation != cap.allocation_v21()
            || old.resource != cap.resource_plan_v21()
            || old.contract != cap.contract_v21()
            || old.envelope.binding_v18()? != e.binding_v18()?
        {
            return Err(err(bad("owner18 existing paid native owner differs")));
        }
        return Ok(());
    }
    let m = source.policy().certification.map_or(0, |p| p.m);
    let quanta = match source.original_claim().source {
        PalwClaimSourceV2::FreePrompt { quanta, .. } => quanta,
        _ => unreachable!(),
    };
    if e.version != 1 || e.cert_draws != m || e.fp_cap != b.params.fp_max_quanta_per_receipt || quanta > e.fp_cap {
        return Err(err(bad("owner18 initial native codec policy differs from capacity")));
    }
    if !b
        .entries
        .iter()
        .any(|entry| matches!(entry,PalwDeltaEntryV2::Claim{key,old:None,new:Some(c)}if *key==id&&c==source.original_claim()))
    {
        return Err(err(bad("owner18 initial source lacks actual accepted birth journal")));
    }
    let row = PaidLedgerV18 {
        version: 1,
        id,
        birth: *ctx,
        source: cap.source_v21(),
        allocation: cap.allocation_v21(),
        resource: cap.resource_plan_v21(),
        contract: cap.contract_v21(),
        ruleset: b.extras.native_fact_ruleset.ok_or_else(|| err(bad("owner18 initial native ruleset absent")))?,
        envelope: e,
    };
    let n = borsh::object_length(&row).map_err(|_| err(bad("owner18 paid owner codec")))? as u64;
    if n > e.rows[6].body {
        return Err(err(bad("owner18 paid owner exceeds signed native shape")));
    }
    put(b, PAID_LEDGER, None, scope, Some(&row)).map_err(err)?;
    Ok(())
}
fn paid_ledger_v18(
    s: &PalwChainStateV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<Option<NativeRetainedV1<PaidLedgerV18>>, PalwStateV2Error> {
    let Some(row) = shared::<PaidLedgerV18>(s, PAID_LEDGER, key(id, c).map_err(err)?)? else { return Ok(None) };
    if row.version != 1
        || row.id != id
        || row.birth.daa_score != c.accepted_daa
        || row.birth.blue_score != c.accepted_blue_score
        || !matches!(c.source,PalwClaimSourceV2::FreePrompt{quanta,..}if quanta<=row.envelope.fp_cap)
    {
        return Err(err(bad("owner18 native paid ledger source differs")));
    }
    let source = super::palw_claim_local_source_admission_v8::for_stored_claim_v8(s, id, c)?
        .ok_or_else(|| err(bad("owner18 native paid ledger lost original Source8")))?;
    if source.accepting_point() != row.birth
        || !same_identity(source.original_claim(), c)
        || source.policy().certification.map_or(0, |p| p.m) != row.envelope.cert_draws
    {
        return Err(err(bad("owner18 native paid ledger original policy differs")));
    }
    Ok(Some(row))
}
fn verify_fp_envelope_at_v18(
    s: &PalwChainStateV2,
    id: Hash64,
    birth: PalwBlockContextV2,
    e: &NativeOwner18FpEnvelopeV1,
    executing: Option<PalwBlockContextV2>,
) -> Result<(), PalwStateV2Error> {
    let limit = executing.or(s.last_point).ok_or_else(|| err(bad("owner18 capacity has no actual native point")))?;
    if birth.blue_score > limit.blue_score || birth.daa_score > limit.daa_score {
        return Err(err(bad("owner18 capacity birth exceeds actual native point")));
    }
    if let Some(source) = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(s, id, birth)? {
        let original = source.original_claim();
        if let Some(owned) = paid_ledger_v18(s, id, original)? {
            if owned.birth != birth || owned.envelope.binding_v18()? != e.binding_v18()? {
                return Err(err(bad("owner18 actual paid native envelope differs")));
            }
            if let Some(current) = s.claims.get(&id).filter(|c| same_birth(c, original)) {
                let reader = StateNodesV2(s);
                let rv = nodes::lookup(&reader, root(s, CURRENT, None).map_err(err)?, key(id, current).map_err(err)?.as_byte_slice())
                    .map_err(err)?
                    .ok_or_else(|| err(bad("owner18 actual Current publication absent")))?;
                let row = body::<CurrentV18>(s, rv).map_err(err)?;
                if row.version != 1
                    || row.id != id
                    || row.current != *current
                    || row.birth != birth
                    || row.origin_kind != 2
                    || row.origin_f1.is_some()
                    || !same_identity(original, current)
                    || row.point.blue_score > limit.blue_score
                    || row.point.daa_score > limit.daa_score
                    || row.point.blue_score < birth.blue_score
                    || row.point.daa_score < birth.daa_score
                {
                    return Err(err(bad("owner18 actual Current native identity/point differs")));
                }
                if rv.bytes > e.rows[0].body
                    || borsh::object_length(current).map_err(|_| err(bad("owner18 actual current codec")))? as u64 > e.claim_live
                {
                    return Err(err(bad("owner18 actual Current exceeds signed finite shape")));
                }
                if e.modern_alias {
                    let alias = cohort15::alias_original_snapshot_v15(s, id, original)?
                        .ok_or_else(|| err(bad("owner18 actual live Source15 alias absent")))?;
                    cohort15::check_native_alias_v15(s, &alias, id, original)?;
                }
                return Ok(());
            }
            if let Some((rv, row)) = lookup(s, id, original)? {
                if row.version != 1
                    || !row.completed
                    || row.id != id
                    || row.birth != birth
                    || row.original != *original
                    || !same_identity(&row.terminal, original)
                    || row.origin_kind != 2
                    || row.origin_f1.is_some()
                    || row.removed.blue_score > limit.blue_score
                    || row.removed.daa_score > limit.daa_score
                    || !matches!(row.terminal.phase, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. })
                {
                    return Err(err(bad("owner18 actual Held native identity/removal differs")));
                }
                if rv.bytes > e.rows[1].body
                    || row.approval.as_ref().is_some_and(|raw| raw.len() as u64 > e.cert)
                    || row.exec.bytes != e.exec
                {
                    return Err(err(bad("owner18 held publication exceeds signed finite shape")));
                }
                let exec = check_exec(s, row.exec)?;
                if exec.root.is_some()
                    || exec.reward.is_some()
                    || exec.policy.is_some()
                    || !exec.slices.is_empty()
                    || !exec.inputs.is_empty()
                {
                    return Err(err(bad("owner18 FP incorrectly owns native EXEC")));
                }
                if e.modern_alias {
                    let alias = row.native_cohort.ok_or_else(|| err(bad("owner18 actual Held Source15 alias absent")))?;
                    if cohort15::check_native_alias_v15(s, &alias, id, original)? != birth || row.members != alias.members_v15() {
                        return Err(err(bad("owner18 actual Held Source15 alias differs")));
                    }
                }
                let a = inventory(&row, rv).map_err(err)?;
                let raw = encode(&*a).map_err(err)?;
                let _lease = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
                let actual = s
                    .kernel_route
                    .as_ref()
                    .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, akey(value(&raw).data).to_vec())))
                    .ok_or_else(|| err(bad("owner18 actual Held A2 absent")))?;
                if actual.as_slice() != raw.as_slice() {
                    return Err(err(bad("owner18 actual Held A2 differs")));
                }
                return Ok(());
            }
        }
    }
    // Native closure is its actual same-born A5 inventory. This tests data
    // ownership only; no permitted/approval boolean is turned into paid credit.
    if let Some(closed) = super::palw_claim_local_exact_closed_source_v5::read_native_exact_identity_v18(s, id, &birth)?
        && closed.closed_at.blue_score <= limit.blue_score
        && closed.closed_at.daa_score <= limit.daa_score
    {
        return Ok(());
    }
    Err(err(bad("owner18 same-born native current/held/closed owner absent")))
}
pub(super) fn verify_fp_owner18_envelope_v1(
    s: &PalwChainStateV2,
    id: Hash64,
    birth: PalwBlockContextV2,
    e: &NativeOwner18FpEnvelopeV1,
) -> Result<(), PalwStateV2Error> {
    verify_fp_envelope_at_v18(s, id, birth, e, None)
}
pub(super) fn verify_fp_owner18_envelope_in_fold_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    birth: PalwBlockContextV2,
    e: &NativeOwner18FpEnvelopeV1,
) -> Result<(), PalwStateV2Error> {
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(err(bad("owner18 capacity lacks sealed actual executing point")));
    }
    verify_fp_envelope_at_v18(&b.state, id, birth, e, Some(*ctx))
}

/// Borrowed opaque retirement owned by the actual capture→delete→finish path.
/// Lifetimes are limited to the owner's admitted callback; no raw row constructor.
/// Complete data closure of exactly this captured Pending. Both semantic and
/// quote-only readers use this SAME output stream, including branch nodes,
/// leaf bodies and the actual immutable source-authority material. No unrelated
/// Pending or new live graph vertex is introduced.
fn visit_complete_pending_material_v18(
    s: &PalwChainStateV2,
    row: &PendingV18,
    rv: RowValueV2,
    mut f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let reader = StateNodesV2(s);
    let a = inventory(row, rv).map_err(err)?;
    let header = encode(&(1u16, row.id, rv, &*a)).map_err(err)?;
    f(&header)?;
    for value in &a.literals {
        let raw = reader.body(*value).map_err(err)?;
        f(&raw)?;
    }
    for component in &a.components {
        let mut callback = None;
        let result = nodes::visit_nodes(&reader, *component, |_, node| {
            let raw = encode(node)?;
            if let Err(e) = f(&raw) {
                callback = Some(e);
                return Err(bad("owner18 component node material callback"));
            }
            if let crate::palw_claim_local_context_nodes_v2::ContextNodeV2::Leaf { value, .. } = node {
                let raw = reader.body(*value)?;
                if let Err(e) = f(&raw) {
                    callback = Some(e);
                    return Err(bad("owner18 component leaf material callback"));
                }
            }
            Ok(())
        });
        if let Some(e) = callback {
            return Err(e);
        }
        result.map_err(err)?;
    }
    match row.origin_kind {
        2 => {
            if super::palw_claim_local_source_admission_v8::visit_checkpoint_material_v10(s, row.id, &row.original, &mut f)?
                != Some(row.birth)
            {
                return Err(err(bad("owner18 source material lost exact Source8 birth")));
            }
        }
        1 => {
            let digest = row.origin_f1.ok_or_else(|| err(bad("owner18 legacy source material lacks immutable identity")))?;
            let key = crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(row.birth.block);
            let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
            let raw = s
                .kernel_route
                .as_ref()
                .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())))
                .ok_or_else(|| err(bad("owner18 legacy source material missing actual F1")))?;
            if crate::palw_claim_local_v1::palw_liability_material_id_v1(raw) != digest {
                return Err(err(bad("owner18 legacy source material changed immutable F1")));
            }
            f(raw)?;
        }
        _ => return Err(err(bad("owner18 pending material has unsupported native origin"))),
    }
    Ok(())
}
pub(super) struct PendingCheckpointViewV18<'a> {
    row: &'a PendingV18,
    value: RowValueV2,
}
impl PendingCheckpointViewV18<'_> {
    pub(super) fn claim_v18(&self) -> Hash64 {
        self.row.id
    }
    pub(super) fn birth_v18(&self) -> PalwBlockContextV2 {
        self.row.birth
    }
    pub(super) fn removed_v18(&self) -> PalwBlockContextV2 {
        self.row.removed
    }
    pub(super) fn removal_ordinal_v18(&self) -> u64 {
        self.row.removal_ordinal
    }
    pub(super) fn original_v18(&self) -> &PalwClaimStateV2 {
        &self.row.original
    }
    pub(super) fn terminal_v18(&self) -> &PalwClaimStateV2 {
        &self.row.terminal
    }
    pub(super) fn selected_cert_v18(&self) -> Option<&[u8]> {
        self.row.approval.as_deref()
    }
    pub(super) fn cert_instance_attached_v18(&self) -> bool {
        self.row.approval_attached
    }
    pub(super) fn winner_v18(&self) -> Option<Hash64> {
        self.row.winner
    }
    pub(super) fn material_until_v18(&self) -> u64 {
        self.row.material_until
    }
    pub(super) fn native_ruleset_v18(&self) -> crate::Hash {
        self.row.ruleset
    }
    pub(super) fn actual_origin_kind_v18(&self) -> u8 {
        self.row.origin_kind
    }
    pub(super) fn immutable_f1_digest_v18(&self) -> Option<Hash64> {
        self.row.origin_f1
    }
    pub(super) fn native_value_v18(&self) -> RowValueV2 {
        self.value
    }
    pub(super) fn cohort_root_v18(&self) -> ComponentRootV2 {
        self.row.members
    }
    pub(super) fn cohort_complete_v18(&self) -> bool {
        self.row.cohort_complete
    }
    pub(super) fn native_cohort_alias_v18(&self) -> Option<NativeCohortAliasV15> {
        self.row.native_cohort
    }
    pub(super) fn exec_value_v18(&self) -> RowValueV2 {
        self.row.exec
    }
    pub(super) fn exec_material_v18(&self, s: &PalwChainStateV2) -> Result<RetiredExecViewV18, PalwStateV2Error> {
        Ok(RetiredExecViewV18 { row: check_exec(s, self.row.exec)? })
    }
    pub(super) fn own_readiness_v18(
        &self,
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        now: u64,
    ) -> Result<RewardReadinessV1, PalwStateV2Error> {
        own_standing(s, p, now, self.row)
    }
    pub(super) fn recorded_readiness_v18(
        &self,
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        now: u64,
    ) -> Result<RewardReadinessV1, PalwStateV2Error> {
        rewards::recorded_reward_readiness_v1(s, p, now, self.row.id, &self.row.terminal, false)
    }
    pub(super) fn visit_owned_material_v18(
        &self,
        s: &PalwChainStateV2,
        f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
    ) -> Result<(), PalwStateV2Error> {
        visit_complete_pending_material_v18(s, self.row, self.value, f)
    }
}
pub(super) struct RetiredExecViewV18 {
    row: NativeRetainedV1<ExecV18>,
}
impl RetiredExecViewV18 {
    pub(super) fn captured_closed_v18(
        &self,
        params: &PalwStateParamsV2,
        id: Hash64,
        terminal: &PalwClaimStateV2,
        now: u64,
    ) -> Result<bool, PalwStateV2Error> {
        self.row.closed(params, id, terminal, now)
    }
    pub(super) fn root_v18(&self) -> Option<&crate::palw_work_slice_v2::PalwWorkRootV2> {
        self.row.root.as_ref()
    }
    pub(super) fn reward_v18(&self) -> Option<&crate::palw_work_slice_v2::PalwExecV2RewardRowV1> {
        self.row.reward.as_ref()
    }
    pub(super) fn policy_v18(&self) -> Option<&misaka_palw_kernel::ledger::LedgerPolicyV1> {
        self.row.policy.as_ref()
    }
    pub(super) fn slices_v18(&self) -> &[(u32, crate::palw_work_slice_v2::PalwWorkSliceRowV2)] {
        &self.row.slices
    }
    pub(super) fn inputs_len_v18(&self) -> usize {
        self.row.inputs.len()
    }
    pub(super) fn input_v18(&self, index: usize) -> Option<RetiredExecInputViewV18<'_>> {
        self.row.inputs.get(index).map(|row| RetiredExecInputViewV18 { row })
    }
}
pub(super) struct RetiredExecInputViewV18<'a> {
    row: &'a ExecInputV18,
}
impl RetiredExecInputViewV18<'_> {
    pub(super) fn claim_v18(&self) -> Hash64 {
        self.row.claim
    }
    pub(super) fn native_v18(&self) -> Option<&misaka_palw_kernel::ledger::ClaimRowV1> {
        self.row.row.as_ref()
    }
    pub(super) fn authority_v18(&self) -> Option<&crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2> {
        self.row.authority.as_ref()
    }
    pub(super) fn final_v18(&self) -> Option<&crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2> {
        self.row.final_row.as_ref()
    }
    pub(super) fn lock_v18(&self) -> Option<&crate::palw_claim_challenge_v2::PalwPaidClaimLockV2> {
        self.row.lock.as_ref()
    }
}
/// Population pass only while constructing/restoring the whole checkpoint.
/// Ordinary source consumers use the exact keyed scope getter below.
pub(super) fn visit_pending_checkpoint_sources_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    mut f: impl FnMut(PendingCheckpointViewV18<'_>) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let mut callback = None;
    let result = nodes::complete_rows(&StateNodesV2(s), root(s, PENDING, None).map_err(err)?, |key, rv| {
        let row = body::<PendingV18>(s, rv)?;
        if key != self::key(row.id, &row.terminal)?.as_byte_slice() {
            return Err(bad("owner18 checkpoint Pending key differs"));
        }
        if let Err(e) = check_record(s, p, rv, &row)
            .and_then(|()| check_attached_cert_current_v18(s, &row))
            .and_then(|()| f(PendingCheckpointViewV18 { row: &row, value: rv }))
        {
            callback = Some(e);
            return Err(bad("owner18 checkpoint Pending source refused"));
        }
        Ok(())
    });
    if let Some(e) = callback {
        return Err(e);
    }
    result.map_err(err)
}
pub(super) fn with_pending_checkpoint_source_v18<T>(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    original: &PalwClaimStateV2,
    f: impl FnOnce(PendingCheckpointViewV18<'_>) -> Result<T, PalwStateV2Error>,
) -> Result<Option<T>, PalwStateV2Error> {
    let Some((rv, row)) = lookup(s, id, original)? else { return Ok(None) };
    check_record(s, p, rv, &row)?;
    check_attached_cert_current_v18(s, &row)?;
    f(PendingCheckpointViewV18 { row: &row, value: rv }).map(Some)
}

/// Allocation inventory only. It deliberately has no readiness, cert/paid
/// standing, retirement or F1-suppression method and accepts no guessed params.
pub(super) struct QuotePendingViewV18<'a> {
    row: &'a PendingV18,
    value: RowValueV2,
}
impl QuotePendingViewV18<'_> {
    pub(super) fn claim_v18(&self) -> Hash64 {
        self.row.id
    }
    pub(super) fn birth_v18(&self) -> PalwBlockContextV2 {
        self.row.birth
    }
    pub(super) fn original_v18(&self) -> &PalwClaimStateV2 {
        &self.row.original
    }
    pub(super) fn terminal_v18(&self) -> &PalwClaimStateV2 {
        &self.row.terminal
    }
    pub(super) fn native_value_v18(&self) -> RowValueV2 {
        self.value
    }
    pub(super) fn exec_value_v18(&self) -> RowValueV2 {
        self.row.exec
    }
    pub(super) fn cohort_root_v18(&self) -> ComponentRootV2 {
        self.row.members
    }
    pub(super) fn visit_material_v18(
        &self,
        s: &PalwChainStateV2,
        f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
    ) -> Result<(), PalwStateV2Error> {
        visit_complete_pending_material_v18(s, self.row, self.value, f)
    }
}
pub(super) fn quote_pending_checkpoint_material_v18(
    s: &PalwChainStateV2,
    mut f: impl FnMut(QuotePendingViewV18<'_>) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let mut callback = None;
    let result = nodes::complete_rows(&StateNodesV2(s), root(s, PENDING, None).map_err(err)?, |_, rv| {
        let row = body::<PendingV18>(s, rv)?;
        if let Err(e) = f(QuotePendingViewV18 { row: &row, value: rv }) {
            callback = Some(e);
            return Err(bad("owner18 Pending allocation quote refused"));
        }
        Ok(())
    });
    if let Some(e) = callback {
        return Err(e);
    }
    result.map_err(err)
}

pub(super) fn has_native_ledger_refs_v18(
    s: &PalwChainStateV2,
    id: Hash64,
    original: &PalwClaimStateV2,
) -> Result<bool, PalwStateV2Error> {
    let scope = key(id, original).map_err(err)?;
    if let Some(row) = shared::<CurrentV18>(s, CURRENT, scope)? {
        if row.id != id || !same_identity(&row.current, original) {
            return Err(err(bad("owner18 Source8 current hold substituted identity")));
        }
        return Ok(true);
    }
    if let Some((_, row)) = lookup(s, id, original)? {
        if row.id != id || !same_identity(&row.original, original) {
            return Err(err(bad("owner18 Source8 pending hold substituted identity")));
        }
        return Ok(true);
    }
    if let Some(row) = shared::<PaidLedgerV18>(s, PAID_LEDGER, scope)? {
        if row.id != id || row.birth.daa_score != original.accepted_daa || row.birth.blue_score != original.accepted_blue_score {
            return Err(err(bad("owner18 Source8 native owner substituted identity")));
        }
        return Ok(true);
    }
    Ok(false)
}
fn validate_paid_ledgers_v18(s: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    let mut callback = None;
    let result = nodes::complete_rows(&StateNodesV2(s), root(s, PAID_LEDGER, None).map_err(err)?, |key, rv| {
        let row = body::<PaidLedgerV18>(s, rv)?;
        let check = (|| {
            let source = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(s, row.id, row.birth)?
                .ok_or_else(|| err(bad("owner18 paid native owner lost Source8 birth")))?;
            if key != self::key(row.id, source.original_claim()).map_err(err)?.as_byte_slice() || rv.bytes > row.envelope.rows[6].body
            {
                return Err(err(bad("owner18 paid native owner key/width")));
            }
            verify_fp_owner18_envelope_v1(s, row.id, row.birth, &row.envelope)
        })();
        if let Err(e) = check {
            callback = Some(e);
            return Err(bad("owner18 paid native owner cold validation"));
        }
        Ok(())
    });
    if let Some(e) = callback {
        return Err(e);
    }
    result.map_err(err)
}

/// Native eligibility-ledger ownership only, not paid archival standing. Initial
/// funded new originals additionally require their OAR2 actual capacity; later
/// updates use the admitted native owner and their own local write tickets.
pub(super) fn claim_native_ledger_ownership_ready_in_fold_v21(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    current: &PalwClaimStateV2,
) -> Result<bool, PalwStateV2Error> {
    if !claim_checkpoint_overlay_ready_in_fold_v18(b, ctx, id, current)? {
        return Ok(false);
    }
    let Some(origin) = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&b.state, b.params, id, current)? else {
        return Ok(false);
    };
    let born = origin.accepting_point();
    let original = origin.original_claim();
    // A mutable latest census is not an immutable earlier birth horizon. Every
    // nonzero-root native owner must retain its actual birth-scoped Source15
    // snapshot before the old F1 representation may be omitted.
    if original.execution_root != Hash64::default() && origin.native_legacy_frame_key_v19().is_none() {
        if let Some(alias) = cohort15::alias_original_snapshot_v15(&b.state, id, original)? {
            if cohort15::check_native_alias_v15(&b.state, &alias, id, original)? != born {
                return Err(err(bad("owner18 native owner cohort birth differs")));
            }
        } else {
            let Some((_, row)) = current_row_at_v18(&b.state, b.params, id, current, Some(*ctx))? else { return Ok(false) };
            if row.origin_kind != 1
                || row.origin_f1.is_none()
                || verified_native_legacy_f1_v25(&b.state, b.params, id, born, original)? != row.origin_f1
            {
                return Ok(false);
            }
        }
    }
    if let Some(owner) = paid_ledger_v18(&b.state, id, current)? {
        verify_fp_owner18_envelope_in_fold_v1(b, ctx, id, born, &owner.envelope)?;
        return Ok(true);
    }
    if born == *ctx {
        if actual_mixed_original_profile_v25(b, id)? {
            return Ok(false);
        } // Only MixedV2 requires new row9; never borrow another profile's capacity.
        if !b.entries.iter().any(|e| matches!(e,PalwDeltaEntryV2::Claim{key,old:None,new:Some(c)}if *key==id&&c==original)) {
            return Ok(false);
        }
    }
    // The exact None→Some proof above is ordinary base admission ownership,
    // including legacy TFO2/OAR1; it is never OAR2 paid holding/service standing.
    // Source8 is the authenticated original native admission. Literal legacy F1
    // keeps its actual immutable identity. Neither route grants material income,
    // an old paid-alias credit or an arbitrary new producer service obligation.
    Ok(true)
}

fn cert_relay_scope_ready_v18(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    seal: &CertRelaySealV18,
) -> Result<bool, PalwStateV2Error> {
    if seal.point != *ctx || seal.sealed_end > b.entries.len() {
        return Ok(false);
    }
    if let Some(row) = shared::<PendingV18>(&b.state, PENDING, seal.scope)? {
        if row.id != seal.id
            || row.birth != seal.birth
            || key(row.id, &row.original).map_err(err)? != seal.scope
            || hash(24, &row.original).map_err(err)? != seal.original
            || hash(25, &row.terminal).map_err(err)? != seal.terminal
        {
            return Err(err(bad("owner18 certification relay substituted its native source instance")));
        }
        return super::palw_claim_local_retired_checkpoint_v23::removed_checkpoint_overlay_ready_v23(
            &b.state,
            b.params,
            row.id,
            &row.terminal,
            &row.removed,
        );
    }
    // A same-fold native resolver may have completed this exact Pending. Its
    // genuine A5 inventory remains an identity witness, never an approval bool.
    if let Some(closed) =
        super::palw_claim_local_exact_closed_source_v5::read_native_exact_identity_v18(&b.state, seal.id, &seal.birth)?
    {
        if closed.closed_at.blue_score > ctx.blue_score
            || closed.closed_at.daa_score > ctx.daa_score
            || hash(24, &closed.original).map_err(err)? != seal.original
            || hash(26, &closed.terminal_phase).map_err(err)? != seal.phase
        {
            return Err(err(bad("owner18 certification relay closure substituted source or phase")));
        }
        return Ok(true);
    }
    Ok(false)
}
/// Exact retired certification events only. Material availability is necessary
/// but the no-F1 coordinator must separately bind the related historical
/// closure into each actual consumer's checkpoint manifest.
pub(super) fn chain_subject_checkpoint_overlay_ready_in_fold_v21(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
) -> Result<bool, PalwStateV2Error> {
    use crate::palw_security_certification_v1 as cert;
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(err(bad("owner18 cert subject lacks actual executing point")));
    }
    let mut found = false;
    for (ordinal, entry) in b.entries.iter().enumerate() {
        let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else { continue };
        if *table != cert::PALW_SECURITY_CERTIFICATION_TABLE_V1 || key.len() != 65 || key[0] != cert::CERTIFICATION_CLAIM_PREFIX_V1 {
            continue;
        }
        let Some(raw) = old.as_ref().or(new.as_ref()) else { continue };
        let row = decode::<cert::PalwClaimCertificationRowV1>(raw).map_err(err)?;
        if row.claim != id.as_bytes() {
            continue;
        }
        let mut expected = [0u8; 65];
        expected[0] = cert::CERTIFICATION_CLAIM_PREFIX_V1;
        expected[1..].copy_from_slice(&cert::palw_certification_id_v1(&id));
        if key.as_slice() != expected {
            return Err(err(bad("owner18 cert subject native key differs")));
        }
        let Some(old) = old else { return Ok(false) };
        let mut observed = false;
        for (_, seal) in
            b.claim_local_cert_relay_seals_v18.range((ordinal, Hash64::default())..=(ordinal, Hash64::from_bytes([u8::MAX; 64])))
        {
            if seal.id != id
                || seal.ordinal != ordinal
                || seal.old != value(old).data
                || seal.new != new.as_ref().map(|raw| value(raw).data)
            {
                return Err(err(bad("owner18 cert subject differs from actual owner-minted journal seal")));
            }
            if !cert_relay_scope_ready_v18(b, ctx, seal)? {
                return Ok(false);
            }
            observed = true;
        }
        if !observed {
            return Ok(false);
        }
        found = true;
    }
    Ok(found)
}
/// Root/slice deletion identity from the actual pre-retirement capture. This
/// proves neither credit nor global event coverage and never reconstructs EXEC
/// from a supplied old row or a current-policy default.
/// Transient exact retirement-event data. This token retains its native decode
/// ticket; it is not credit, payment, material lease or whole-route readiness.
/// Mint once per actual removed birth and share across its slice deletions.
/// Exact successful native retirement observation. No Pending/A5 existence,
/// source availability, credit or payment standing is inferred by this marker.
/// Only finish_actual_retirement can mint it after matching the real Claim
/// deletion's complete old value to its private pre-retirement capture.
pub(super) struct ActualRetiredClaimObservationV25 {
    id: Hash64,
    birth: PalwBlockContextV2,
    carrying: Hash64,
    removed: PalwBlockContextV2,
    ordinal: usize,
    old: Hash64,
    sealed_end: usize,
}
impl ActualRetiredClaimObservationV25 {
    pub(super) fn covers_claim_deletion_v25(
        &self,
        ctx: &PalwBlockContextV2,
        ordinal: usize,
        id: Hash64,
        old: &PalwClaimStateV2,
    ) -> Result<bool, PalwStateV2Error> {
        if self.id != id
            || self.removed != *ctx
            || self.ordinal != ordinal
            || (self.carrying, self.birth.daa_score, self.birth.blue_score)
                != (old.accepted_block, old.accepted_daa, old.accepted_blue_score)
        {
            return Ok(false);
        }
        Ok(self.old == hash(27, old).map_err(err)?)
    }
}
pub(super) type RetiredExecEventKeyV24 = (Hash64, Hash64, u64, u64);
pub(super) struct RetiredExecEventObservationV24 {
    token: NativeRetainedV1<NativeRetiredExecEventCoverageV24>,
    sealed_end: usize,
}
impl RetiredExecEventObservationV24 {
    pub(super) fn token_v24(&self) -> &NativeRetiredExecEventCoverageV24 {
        &self.token
    }
}
pub(super) struct NativeRetiredExecEventCoverageV24 {
    id: Hash64,
    birth: PalwBlockContextV2,
    carrying: Hash64,
    removed: PalwBlockContextV2,
    exec: RetiredExecViewV18,
}
pub(super) fn prepare_retired_exec_event_coverage_v24(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    removed: &PalwClaimStateV2,
    id: Hash64,
) -> Result<Option<NativeRetainedV1<NativeRetiredExecEventCoverageV24>>, PalwStateV2Error> {
    let Some((rv, row)) = lookup(s, id, removed)? else { return Ok(None) };
    check_record(s, p, rv, &row)?;
    if row.removed != *ctx || row.terminal != *removed || !row.completed {
        return Ok(None);
    }
    if !super::palw_claim_local_retired_checkpoint_v23::removed_checkpoint_overlay_ready_v23(s, p, id, removed, ctx)? {
        return Ok(None);
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<NativeRetiredExecEventCoverageV24>(1).map_err(PalwStateV2Error::NativeResource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let exec = check_exec(s, row.exec)?;
    // Capture originates in the native BTreeMap range. Validate that invariant
    // once, including Held/Voided captures whose slices need not be contiguous.
    if exec.slices.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(err(bad("retired EXEC event slices are not uniquely ordered")));
    }
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(
        NativeRetiredExecEventCoverageV24 {
            id,
            birth: row.birth,
            carrying: row.original.accepted_block,
            removed: *ctx,
            exec: RetiredExecViewV18 { row: exec },
        },
        lease,
    )))
}
fn exact_retired_exec_bytes_v24<T: BorshSerialize>(value: &T, expected: &[u8]) -> Result<bool, PalwStateV2Error> {
    struct Compare<'a> {
        expected: &'a [u8],
        offset: usize,
        different: bool,
    }
    impl std::io::Write for Compare<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let end = self
                .offset
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "retired EXEC byte comparison overflow"))?;
            if self.expected.get(self.offset..end) != Some(bytes) {
                self.different = true;
            }
            self.offset = end;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Compare { expected, offset: 0, different: false };
    borsh::BorshSerialize::serialize(value, &mut out).map_err(|_| err(bad("retired EXEC event canonical codec")))?;
    Ok(!out.different && out.offset == expected.len())
}
impl NativeRetiredExecEventCoverageV24 {
    pub(super) fn birth_v24(&self) -> PalwBlockContextV2 {
        self.birth
    }
    pub(super) fn claim_birth_v24(&self) -> RetiredExecEventKeyV24 {
        (self.id, self.carrying, self.birth.daa_score, self.birth.blue_score)
    }
    pub(super) fn covers_event_v24(
        &self,
        ctx: &PalwBlockContextV2,
        id: Hash64,
        table: u8,
        key_bytes: &[u8],
        old: &[u8],
    ) -> Result<bool, PalwStateV2Error> {
        if self.id != id || self.removed != *ctx {
            return Ok(false);
        }
        match table {
            PALW_EXEC_V2_TABLE_ROOTS_V1 => {
                if key_bytes != id.as_byte_slice() {
                    return Ok(false);
                }
                let Some(root) = self.exec.root_v18() else { return Ok(false) };
                exact_retired_exec_bytes_v24(root, old)
            }
            PALW_EXEC_V2_TABLE_SLICES_V1 => {
                if key_bytes.len() != 68 || &key_bytes[..64] != id.as_byte_slice() {
                    return Ok(false);
                }
                let index = u32::from_le_bytes(key_bytes[64..].try_into().expect("checked native EXEC slice key"));
                let slices = self.exec.slices_v18();
                let Ok(at) = slices.binary_search_by_key(&index, |(i, _)| *i) else { return Ok(false) };
                exact_retired_exec_bytes_v24(&slices[at].1, old)
            }
            _ => Ok(false),
        }
    }
}
/// Compatibility convenience. Batch event coverage retains the opaque token
/// once per exact birth instead of repeatedly decoding its full slice vector.
pub(super) fn removed_exec_event_covered_v18(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    removed: &PalwClaimStateV2,
    id: Hash64,
    table: u8,
    key_bytes: &[u8],
    old: &[u8],
) -> Result<bool, PalwStateV2Error> {
    let Some(token) = prepare_retired_exec_event_coverage_v24(s, p, ctx, removed, id)? else { return Ok(false) };
    token.covers_event_v24(ctx, id, table, key_bytes, old)
}

/// Exact stored birth lookup for source-local dependency traversal. Coordinates
/// must come from actual native Source8/15/Carrier facts; this getter itself
/// supplies only strict captured data, never a new dependency or credit edge.
pub(super) fn with_pending_stored_birth_v24<T>(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    id: Hash64,
    block: BlockHash,
    daa: u64,
    blue: u64,
    f: impl FnOnce(PendingCheckpointViewV18<'_>) -> Result<T, PalwStateV2Error>,
) -> Result<Option<T>, PalwStateV2Error> {
    let scope = hash(1, &(id, block, daa, blue)).map_err(err)?;
    let reader = StateNodesV2(s);
    let Some(rv) = nodes::lookup(&reader, root(s, PENDING, None).map_err(err)?, scope.as_byte_slice()).map_err(err)? else {
        return Ok(None);
    };
    let row = body::<PendingV18>(s, rv).map_err(err)?;
    if row.id != id
        || key(id, &row.original).map_err(err)? != scope
        || row.original.accepted_block != block
        || row.original.accepted_daa != daa
        || row.original.accepted_blue_score != blue
    {
        return Err(err(bad("owner18 targeted dependency substituted stored birth")));
    }
    check_record(s, p, rv, &row)?;
    check_attached_cert_current_v18(s, &row)?;
    f(PendingCheckpointViewV18 { row: &row, value: rv }).map(Some)
}
