//! Fixed current Kernel admission coordinate from the actual native journal.
//! No paid enrollment, selected approval, historical world or retained body.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_resource_v1::{NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::ledger::ClaimRowV1;
const CURRENT: u32 = 1;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 13, table, schema: 9, key_bytes: 64, value_bytes: 217 }
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
        _ => bad("kernel admission journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("kernel admission encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out).map_err(|_| bad("kernel admission encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("kernel admission typed body codec"),
    })
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct CurrentBirthV9 {
    version: u8,
    claim: Hash64,
    point: PalwBlockContextV2,
    native_identity: Hash64,
}
crate::palw_native_resource_v1::wire::wire_struct!(CurrentBirthV9;u8,Hash64,PalwBlockContextV2,Hash64);
fn read(s: &PalwChainStateV2, id: Hash64) -> Result<Option<NativeRetainedV1<CurrentBirthV9>>, ContextErrorV2> {
    let row = lookup::<CurrentBirthV9>(s, CURRENT, None, id)?;
    if let Some(row) = &row
        && (row.version != 9 || row.claim != id)
    {
        return Err(bad("kernel admission coordinate identity"));
    }
    Ok(row)
}
fn native_identity(row: &ClaimRowV1) -> Result<Hash64, ContextErrorV2> {
    super::palw_claim_local_kernel_source_owner_v9::identity(row).map_err(native_err)
}
/// Only a coordinate for the CURRENT native Claim. Old work already owns its
/// exact token; removing/replacing this row cannot close or reassign that work.
/// There is no indefinitely retained historical catalogue or F1 mirror here.
pub(super) fn current_point_v9(
    s: &PalwChainStateV2,
    id: Hash64,
    current: &ClaimRowV1,
) -> Result<Option<PalwBlockContextV2>, PalwStateV2Error> {
    let Some(row) = read(s, id).map_err(err)? else { return Ok(None) };
    if row.point.daa_score != current.committed_daa || row.native_identity != native_identity(current).map_err(err)? {
        return Err(err(bad("current kernel row differs from actual admission")));
    }
    Ok(Some(row.point))
}
/// Every matching event is an ACTUAL successful native write. Birth is minted
/// only by None->Some. Mutable updates never mint a birth, and native deletion
/// removes only the current locator. The entire operation rolls its auxiliary
/// journal back on error; the outer fold owns the already-existing native rows.
pub(super) fn capture_native_admissions_v9(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(());
    }
    let end = b.entries.len();
    atomic(b, |b| {
        for ordinal in 0..end {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = &b.entries[ordinal] else { continue };
            if *table != misaka_palw_kernel::rows::TABLE_CLAIMS_V1 {
                continue;
            }
            let bytes: [u8; 64] = key.as_slice().try_into().map_err(|_| bad("kernel admission native key length"))?;
            let id = Hash64::from_bytes(bytes);
            let prior = read(&b.state, id)?;
            match (old, new) {
                (None, Some(raw)) => {
                    let row = decode::<ClaimRowV1>(raw)?;
                    if row.committed_daa != ctx.daa_score {
                        return Err(bad("kernel admission differs from executing clock"));
                    }
                    let value = CurrentBirthV9 { version: 9, claim: id, point: *ctx, native_identity: native_identity(&row)? };
                    // The native None->Some is the sole birth authority. A prior
                    // stale catalogue cannot silently assign this admission.
                    if prior.is_some() {
                        return Err(bad("kernel admission locator survived native deletion"));
                    }
                    put(b, CURRENT, None, id, Some(&value))?;
                }
                (Some(_), None) => {
                    if prior.is_some() {
                        put::<CurrentBirthV9>(b, CURRENT, None, id, None)?;
                    }
                }
                (Some(_), Some(raw)) => {
                    if let Some(prior) = prior {
                        let row = decode::<ClaimRowV1>(raw)?;
                        if row.committed_daa != prior.point.daa_score || native_identity(&row)? != prior.native_identity {
                            return Err(bad("kernel mutable update changed admission identity"));
                        }
                    }
                }
                (None, None) => return Err(bad("empty actual kernel native write")),
            }
        }
        Ok(())
    })
    .map_err(err)
}
/// Cold restore validates the fixed coordinate against the current native row.
/// Absence is legitimate for legacy pre-catalogue native state; malformed
/// PRESENT authority never falls back to F1 or to a guessed DAA/block tuple.
pub(super) fn validate_current_births_v9(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, reader.current(spec(CURRENT))?, |key, value| {
        let body = reader.body(value)?;
        let row = decode::<CurrentBirthV9>(&body)?;
        if key != row.claim.as_byte_slice() || row.version != 9 {
            return Err(bad("kernel current birth key/codec"));
        }
        let _native_key = reserve_native_requests_v1(64).map_err(ContextErrorV2::Resource)?;
        let native = super::palw_claim_local_rewards_v1::kernel_claim_row(s, &row.claim)
            .ok_or(bad("kernel current birth lost native claim"))?;
        let native = decode::<ClaimRowV1>(native)?;
        if native.committed_daa != row.point.daa_score || native_identity(&native)? != row.native_identity {
            return Err(bad("kernel current birth/native contradiction"));
        }
        if s.last_point.is_none_or(|tip| row.point.daa_score > tip.daa_score || row.point.blue_score > tip.blue_score) {
            return Err(bad("kernel current birth beyond actual tip"));
        }
        Ok(())
    })?;
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'I') {
                continue;
            }
            let root = decode::<ComponentRootV2>(raw)?;
            if root.spec.owner == 13 && (key.len() != 18 || root.spec != spec(CURRENT)) {
                return Err(bad("kernel current birth root specification"));
            }
        }
    }
    Ok(())
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

/// Borrow actual immutable current-coordinate storage. The bytes are explicitly
/// a native coordinate body envelope, never a full liability frame/carriage.
pub(super) fn visit_current_birth_material_v9<'a>(
    s: &'a PalwChainStateV2,
    mut f: impl FnMut(Hash64, PalwBlockContextV2, &'a Vec<u8>) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, reader.current(spec(CURRENT)).map_err(err)?, |key, rv| {
        let body = reader.body(rv)?;
        let row = decode::<CurrentBirthV9>(&body)?;
        if row.version != 9 || key != row.claim.as_byte_slice() {
            return Err(bad("kernel borrowed coordinate identity"));
        }
        let _keys = reserve_native_requests_v1(65 + 64).map_err(ContextErrorV2::Resource)?;
        let native = super::palw_claim_local_rewards_v1::kernel_claim_row(s, &row.claim)
            .ok_or(bad("kernel borrowed coordinate native row missing"))?;
        let native = decode::<ClaimRowV1>(native)?;
        if row.point.daa_score != native.committed_daa || row.native_identity != native_identity(&native)? {
            return Err(bad("kernel borrowed coordinate source changed"));
        }
        let mut key = [0u8; 65];
        key[0] = b'B';
        key[1..].copy_from_slice(rv.data.as_byte_slice());
        let raw = s
            .kernel_route
            .as_ref()
            .and_then(|route| route.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())))
            .ok_or(bad("kernel borrowed coordinate body disappeared"))?;
        // reader.body already verified exact outer Vec codec, length and hash.
        f(row.claim, row.point, raw).map_err(native_err)
    })
    .map_err(err)
}
/// Current source-only native checkpoint projection. All credit/lifecycle
/// decisions stay in the native owning functions. This function merely retains
/// their exact current inputs, including an explicitly absent paid index/Final.
/// It binds only typed local rows and actual birth; it is not a partial world.
pub(super) fn visit_current_checkpoint_material_v10(
    s: &PalwChainStateV2,
    id: Hash64,
    mut f: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    use crate::palw_claim_challenge_v2 as paid;
    let Some(birth) = read(s, id).map_err(err)? else { return Ok(false) };
    let _keys = reserve_native_requests_v1(64 * 4 + 65).map_err(PalwStateV2Error::NativeResource)?;
    let raw = super::palw_claim_local_rewards_v1::kernel_claim_row(s, &id)
        .ok_or_else(|| err(bad("scoped kernel checkpoint native row missing")))?;
    let row = decode::<ClaimRowV1>(raw).map_err(err)?;
    if row.committed_daa != birth.point.daa_score || native_identity(&row).map_err(err)? != birth.native_identity {
        return Err(err(bad("scoped kernel checkpoint birth differs")));
    }
    let route = s.kernel_route.as_ref().ok_or_else(|| err(bad("scoped kernel checkpoint route absent")))?;
    // Each transient lookup Vec is admitted before construction. Returned rows
    // borrow already-owned native state, so unrelated authority books are never
    // scanned or cloned to find one source.
    let class = route.rows.get(&(misaka_palw_kernel::rows::TABLE_OPV_CLASSES_V1, row.class_binding_id.to_vec()));
    let mut index_key = [0u8; 65];
    index_key[0] = paid::PAID_CLAIM_BASE_INDEX_PREFIX_V3;
    index_key[1..].copy_from_slice(id.as_byte_slice());
    let index = route.aux.get(&(paid::PAID_CLAIM_ENROLLMENTS_V2, index_key.to_vec()));
    let mut context = None;
    let mut enrollment = None;
    let mut authority = None;
    let mut final_row = None;
    if let Some(raw_index) = index {
        let index = decode::<paid::PalwPaidClaimIndexV3>(raw_index).map_err(err)?;
        if index.version != 3 || index.kind != paid::PalwPaidClaimIndexKindV3::BaseClaim || index.owner != id.as_bytes() {
            return Err(err(bad("scoped kernel checkpoint index subject differs")));
        }
        context = Some(index.target);
        authority = route.aux.get(&(paid::PAID_CLAIM_ENROLLMENTS_V2, index.target.to_vec()));
        if let Some(raw_authority) = authority {
            let decoded = decode::<paid::PalwPaidClaimAuthorityV2>(raw_authority).map_err(err)?;
            if decoded.version != 2 || decoded.enrollment.base_claim != id.as_bytes() || decoded.context.id() != index.target {
                return Err(err(bad("scoped kernel checkpoint authority subject differs")));
            }
            let eid = decoded.enrollment.id();
            enrollment = Some(eid);
            final_row = route.aux.get(&(paid::PAID_CLAIM_FINALS_V2, eid.to_vec()));
        }
    }
    let rows = [Some(raw), class, index, authority, final_row];
    let refs = rows.map(|row| row.map(|raw| value(raw)));
    let proof = encode(&(10u16, *birth, row.class_binding_id, context, enrollment, refs)).map_err(err)?;
    f(&proof)?;
    for raw in rows.into_iter().flatten() {
        f(raw)?;
    }
    // Distinct supplemental grammar. The released proof10 row/ref array above
    // stays byte-for-byte unchanged; these actual Option refs cover the GateC
    // and selected paid-lock readers instead of lending a latest whole F1.
    use crate::palw_security_certification_v1 as cert;
    let _supplement_keys = reserve_native_requests_v1(65 * 2 + 64).map_err(PalwStateV2Error::NativeResource)?;
    let cert_id = cert::palw_kernel_claim_certification_id_v1(&id.as_bytes());
    let cert_row = route.aux.get(&(cert::PALW_SECURITY_CERTIFICATION_TABLE_V1, cert::certification_kernel_claim_key_v1(&cert_id)));
    let hold = route.aux.get(&(cert::PALW_SECURITY_CERTIFICATION_TABLE_V1, cert::certification_kernel_hold_key_v1(&cert_id)));
    let lock = enrollment.and_then(|eid| route.aux.get(&(paid::PAID_CLAIM_LOCKS_V2, eid.to_vec())));
    if let Some(raw) = cert_row {
        let actual = decode::<cert::PalwClaimCertificationRowV1>(raw).map_err(err)?;
        if actual.claim != id.as_bytes()
            || actual.class_id != row.class_binding_id
            || actual.producer_bond != row.producer
            || actual.attempt
        {
            return Err(err(bad("scoped kernel certification subject differs")));
        }
    }
    if let Some(raw) = hold {
        let actual = decode::<cert::PalwKernelCertifiedRewardHoldV1>(raw).map_err(err)?;
        if actual.version != 1 || actual.claim != id.as_bytes() || actual.producer_bond != row.producer {
            return Err(err(bad("scoped kernel certified hold subject differs")));
        }
    }
    if let Some(raw) = lock {
        let actual = decode::<paid::PalwPaidClaimLockV2>(raw).map_err(err)?;
        if Some(actual.enrollment) != enrollment {
            return Err(err(bad("scoped kernel selected paid lock subject differs")));
        }
    }
    if let Some(raw) = final_row {
        let actual = decode::<paid::PalwPaidClaimFinalV2>(raw).map_err(err)?;
        if Some(actual.enrollment) != enrollment || actual.base_claim != id.as_bytes() {
            return Err(err(bad("scoped kernel selected paid Final subject differs")));
        }
    }
    let supplement = [cert_row, hold, lock];
    let refs = supplement.map(|raw| raw.map(|raw| value(raw)));
    let material = encode(&(b"misaka-palw/kernel-current-supplement/v1".as_slice(), 1u16, id, refs)).map_err(err)?;
    f(&material)?;
    for raw in supplement.into_iter().flatten() {
        f(raw)?;
    }
    Ok(true)
}
/// Strictly scoped CURRENT coverage only. No selected-approval result is
/// inferred here, and a pre-catalogue source keeps literal F1 compatibility.
pub(super) fn kernel_current_checkpoint_overlay_ready_v18(s: &PalwChainStateV2, id: Hash64) -> Result<bool, PalwStateV2Error> {
    visit_current_checkpoint_material_v10(s, id, |_| Ok(()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::{
        ledger::{ClaimBodyV1, ClaimRowV1},
        lifecycle::{ClaimLifecycleV1, LifecyclePolicyV1},
        spec::{SpecClaimBodyV1, SpecClaimV1, retrieval::RetrievalClaimV1},
    };
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn point(n: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h(100 + n), daa_score: n, blue_score: n, subsidy: 77 }
    }
    fn row(n: u64) -> ClaimRowV1 {
        let body = SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: [7; 64], producer_bond: [8; 64], result: vec![] });
        ClaimRowV1 {
            producer: [8; 64],
            class_binding_id: [9; 64],
            job_id: [7; 64],
            body: ClaimBodyV1::Spec(Box::new(SpecClaimBodyV1 { claim: body, pre_state: vec![], pre_source: None })),
            committed_daa: n,
            life: ClaimLifecycleV1::new(LifecyclePolicyV1 { check_window_daa: 10, challenge_window_daa: 10 }),
            reserved: 0,
            liability_until: None,
            convicted: false,
            rewarded: false,
            sealed_daa: 0,
        }
    }
    fn fixture() -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1) {
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_claim_local_settlement_from_daa(Some(0));
        let mut state = PalwChainStateV2::genesis();
        state.last_point = Some(point(1));
        state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(1), h(2)),
            None,
            misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
        ));
        let extras = PalwTransitionExtrasV1 {
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(1),
                ruleset_digest: h(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        (state, params, extras)
    }
    #[test]
    fn rfc23s2_kernel_current_birth_actual_native_writer_cold_and_inverse() {
        let (state, params, extras) = fixture();
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let id = h(300);
        let original = row(2);
        let table = misaka_palw_kernel::rows::TABLE_CLAIMS_V1;
        b.write_kernel_row_encoded_v1(table, id.as_byte_slice(), Some(&original)).unwrap();
        capture_native_admissions_v9(&mut b, &point(2)).unwrap();
        assert_eq!(current_point_v9(&b.state, id, &original).unwrap(), Some(point(2)));
        b.state.last_point = Some(point(2));
        validate_current_births_v9(&b.state).unwrap();
        let root = b.state.state_root();
        let bytes = borsh::to_vec(b.state.kernel_route().unwrap()).unwrap();
        let mut cold = b.state.clone();
        cold.kernel_route = Some(borsh::from_slice(&bytes).unwrap());
        validate_current_births_v9(&cold).unwrap();
        assert_eq!(cold.state_root(), root);
        assert_eq!(current_point_v9(&cold, id, &original).unwrap(), Some(point(2)));
        // Actual native deletion + capture removes ONLY current authority. An
        // old work token is a different store and is not closed by this API.
        let prior = cold.clone();
        let mut d = TransitionBuilder::new(&prior, &params, false, false, false, false, &extras);
        d.write_kernel_row_encoded_v1::<ClaimRowV1>(table, id.as_byte_slice(), None).unwrap();
        capture_native_admissions_v9(&mut d, &point(3)).unwrap();
        assert!(read(&d.state, id).unwrap().is_none());
        for entry in d.entries.iter().rev() {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else { panic!("only actual kernel rows expected") };
            super::super::apply_kernel_route_row_v1(&mut d.state, *table, key, old, new, true).unwrap();
        }
        assert_eq!(d.state.state_root(), root);
        validate_current_births_v9(&d.state).unwrap();
    }
    #[test]
    fn rfc23s2_kernel_current_birth_mutable_update_cannot_remint_and_clock_error_rolls_back() {
        let (state, params, extras) = fixture();
        let id = h(300);
        let original = row(2);
        let table = misaka_palw_kernel::rows::TABLE_CLAIMS_V1;
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_kernel_row_encoded_v1(table, id.as_byte_slice(), Some(&original)).unwrap();
        capture_native_admissions_v9(&mut b, &point(2)).unwrap();
        let before = b.state.clone();
        let mut updated = original.clone();
        updated.convicted = true;
        let mut u = TransitionBuilder::new(&before, &params, false, false, false, false, &extras);
        u.write_kernel_row_encoded_v1(table, id.as_byte_slice(), Some(&updated)).unwrap();
        capture_native_admissions_v9(&mut u, &point(3)).unwrap();
        assert_eq!(current_point_v9(&u.state, id, &updated).unwrap(), Some(point(2)));
        let mut d = TransitionBuilder::new(&before, &params, false, false, false, false, &extras);
        d.write_kernel_row_encoded_v1(table, h(301).as_byte_slice(), Some(&row(9))).unwrap();
        let boundary = d.entries.len();
        let root = d.state.state_root();
        assert!(capture_native_admissions_v9(&mut d, &point(3)).is_err());
        assert_eq!(d.entries.len(), boundary);
        assert_eq!(d.state.state_root(), root);
    }
}
