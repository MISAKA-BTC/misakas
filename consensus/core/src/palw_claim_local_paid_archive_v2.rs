//! Fold-owned paid Kernel source archives. Distinct from legacy F1 and from a
//! full state carriage. These views authenticate only their explicit complete
//! paid-enrollment catalogue through the admitted native snapshot; they never
//! stand for a whole global state root or replace the selected-approval premise.
use super::palw_claim_local_context_effects_v2::{self as effects, ExecutedOwnerHintV2};
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_challenge_v2 as paid;
use crate::palw_claim_local_context_nodes_v2::{
    self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, ContextNodeV2, RowValueV2,
};
use crate::palw_claim_local_v1::{PALW_CLAIM_LOCAL_TABLE_V1, PalwClaimLocalWorkRefV1 as Work};
use crate::palw_native_resource_v1::{NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::ledger::ClaimRowV1;

// Owner bytes separate native ledger rows, aux rows, and archive-only control
// inputs. None is a new native consensus table or a reinterpretation of F1.
const LEDGER: u8 = 1;
const AUX: u8 = 2;
const CONTROL: u8 = 3;
const DIRECTORY: u8 = 4;
fn spec(owner: u8, table: u8) -> ComponentSpecV2 {
    ComponentSpecV2 { owner, table: table.into(), schema: 2, key_bytes: 65, value_bytes: u64::MAX }
}
fn bad(why: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(why)
}
pub(super) fn state_error(e: ContextErrorV2) -> PalwStateV2Error {
    match e {
        ContextErrorV2::Resource(r) => PalwStateV2Error::NativeResource(r),
        ContextErrorV2::Unavailable(s) | ContextErrorV2::Malformed(s) => PalwStateV2Error::CarriageInconsistent(s.into()),
        ContextErrorV2::Stale => PalwStateV2Error::CarriageInconsistent("stale paid archive component".into()),
    }
}
fn typed<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => bad("paid archive row codec"),
    })
}
fn streamed_id<T: BorshSerialize>(domain: &[u8], v: &T) -> Result<Hash64, ContextErrorV2> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(domain, v).map_err(|_| bad("paid archive identity codec"))
}
fn scope(claim: Hash64, birth: PalwBlockContextV2) -> Result<Hash64, ContextErrorV2> {
    streamed_id(b"misaka-palw/paid-source-cohort/v2", &(claim, birth))
}
fn value(raw: &[u8]) -> RowValueV2 {
    RowValueV2 { data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw), bytes: raw.len() as u64 }
}
fn archive_key(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = store::CONTEXT_ARCHIVE_PREFIX_V2;
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn native_row<'a>(state: &'a PalwChainStateV2, owner: u8, table: u8, key: &[u8]) -> Result<Option<&'a [u8]>, ContextErrorV2> {
    let Some(route) = state.kernel_route.as_ref() else { return Ok(None) };
    let _key = reserve_native_requests_v1(key.len() as u64).map_err(ContextErrorV2::Resource)?;
    let rows = if owner == LEDGER { &route.rows } else { &route.aux };
    Ok(rows.get(&(table, key.to_vec())).map(Vec::as_slice))
}
fn aux<'a>(state: &'a PalwChainStateV2, table: u8, key: &[u8]) -> Result<Option<&'a [u8]>, ContextErrorV2> {
    native_row(state, AUX, table, key)
}

#[derive(BorshSerialize, BorshDeserialize)]
struct PaidSourceManifestV2 {
    version: u16,
    claim: Hash64,
    birth: PalwBlockContextV2,
    point: PalwBlockContextV2,
    network: [u8; 32],
    fired_forks: [u8; 32],
    /// Actual parent full-state basis, explicitly not the root of this view.
    parent_full_state_basis: Hash64,
    /// Actual executing state before this entire archive publication batch.
    /// Every subject shares this same complete basis; it excludes all new A3
    /// and component rows published by the batch, including this archive.
    prepublication_full_state_basis: Hash64,
    enrollment: [u8; 64],
    components: Vec<ComponentRootV2>,
    /// Actual journal ordinal, and accepted-object ordinal/digest when owned.
    events: Vec<(u64, Option<u64>, Option<Hash64>)>,
}
crate::palw_native_resource_v1::wire::wire_struct!(PaidSourceManifestV2;u16,Hash64,PalwBlockContextV2,PalwBlockContextV2,[u8;32],[u8;32],Hash64,Hash64,[u8;64],Vec<ComponentRootV2>,Vec<(u64,Option<u64>,Option<Hash64>)>);

/// A real fold-owned paid source archive has version3. Version2 A rows remain
/// storage-reference inventories only and never become paid source authority.
#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct PaidSourceArchiveV3 {
    pub version: u16,
    pub manifest: RowValueV2,
    pub components: Vec<ComponentRootV2>,
}
crate::palw_native_resource_v1::wire::wire_struct!(PaidSourceArchiveV3;u16,RowValueV2,Vec<ComponentRootV2>);

/// An opaque typed source view. Its constructor accepts only a manifest/body
/// and reference inventory present in this same admitted state. Missing bodies
/// remain Unavailable; a missing native row inside a complete component is an
/// authenticated absent value, never a fabricated empty route.
pub(super) struct PaidSourceViewV2<'a> {
    state: &'a PalwChainStateV2,
    manifest: NativeRetainedV1<PaidSourceManifestV2>,
}
impl PaidSourceViewV2<'_> {
    fn component(&self, owner: u8, table: u8) -> Result<ComponentRootV2, ContextErrorV2> {
        let s = spec(owner, table);
        self.manifest.components.iter().find(|r| r.spec == s).copied().ok_or_else(|| bad("paid source catalogue component omitted"))
    }
    fn row(&self, owner: u8, table: u8, key: &[u8]) -> Result<Option<NativeRetainedV1<Vec<u8>>>, ContextErrorV2> {
        let reader = StateNodesV2(self.state);
        nodes::lookup(&reader, self.component(owner, table)?, key)?.map(|v| reader.body(v)).transpose()
    }
    pub(super) fn point(&self) -> PalwBlockContextV2 {
        self.manifest.point
    }
    pub(super) fn birth(&self) -> PalwBlockContextV2 {
        self.manifest.birth
    }
    /// Exact structural selected-approval consumer used by EXEC, narrowed to a
    /// genuine paid enrollment. Signature/selection acceptance occurred in the
    /// actual native fold; an optimistic Final or local replay cannot mint it.
    pub(super) fn selected_approval(&self) -> Result<bool, ContextErrorV2> {
        let claim = self.manifest.claim.as_bytes();
        let raw = self
            .row(LEDGER, misaka_palw_kernel::rows::TABLE_CLAIMS_V1, &claim)?
            .ok_or(ContextErrorV2::Unavailable("paid source claim body absent"))?;
        let row = typed::<ClaimRowV1>(&raw)?;
        if row.committed_daa != self.manifest.birth.daa_score {
            return Err(bad("paid source claim cohort differs"));
        }
        let mut k = [0; 65];
        k[0] = paid::PAID_CLAIM_BASE_INDEX_PREFIX_V3;
        k[1..].copy_from_slice(&claim);
        let index =
            self.row(AUX, paid::PAID_CLAIM_ENROLLMENTS_V2, &k)?.ok_or(ContextErrorV2::Unavailable("paid source base index absent"))?;
        let index = typed::<paid::PalwPaidClaimIndexV3>(&index)?;
        if index.version != 3 || index.kind != paid::PalwPaidClaimIndexKindV3::BaseClaim || index.owner != claim {
            return Err(bad("paid source base index substituted"));
        }
        let authority = self
            .row(AUX, paid::PAID_CLAIM_ENROLLMENTS_V2, &index.target)?
            .ok_or(ContextErrorV2::Unavailable("paid source authority absent"))?;
        // This whole immutable authority includes the COMPLETE frozen checker
        // population. The view does not replace it by winners or membership.
        let authority = typed::<paid::PalwPaidClaimAuthorityV2>(&authority)?;
        let enrollment = authority.enrollment.id();
        if authority.version != 2
            || authority.context.id() != index.target
            || authority.enrollment.base_claim != claim
            || enrollment != self.manifest.enrollment
            || authority.authorized_daa != row.committed_daa
        {
            return Err(bad("paid source authority/claim/birth substitution"));
        }
        let Some(final_raw) = self.row(AUX, paid::PAID_CLAIM_FINALS_V2, &enrollment)? else { return Ok(false) };
        let final_row = typed::<paid::PalwPaidClaimFinalV2>(&final_raw)?;
        Ok(final_row.enrollment == enrollment
            && final_row.base_claim == claim
            && final_row.verification_class == authority.enrollment.class
            && final_row.signed_lifetime == authority.enrollment.lifetime
            && final_row.complete_receipts > 0
            && final_row.completed_rounds > 0)
    }
}

fn update_raw(
    b: &mut TransitionBuilder<'_>,
    owner_scope: Hash64,
    s: ComponentSpecV2,
    key: &[u8],
    raw: Option<&[u8]>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let reader = StateNodesV2(&b.state);
    let before = reader.owned_current(owner_scope, s)?;
    let old = nodes::lookup(&reader, before, key)?;
    let plan = nodes::update(&reader, before, key, old, raw.map(value))?;
    store::publish_owned_component_v2(b, owner_scope, &plan, raw)?;
    Ok(plan.after())
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("paid archive encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut raw = Vec::with_capacity(n);
    v.serialize(&mut raw).map_err(|_| bad("paid archive encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(raw, lease))
}
/// Install exactly one existing native row into its owner component. The
/// temporary clone holds a separately admitted lease, never a world/carriage.
fn install_native(b: &mut TransitionBuilder<'_>, owner_scope: Hash64, owner: u8, table: u8, key: &[u8]) -> Result<(), ContextErrorV2> {
    let native = native_row(&b.state, owner, table, key)?;
    let reader = StateNodesV2(&b.state);
    let current = reader.owned_current(owner_scope, spec(owner, table))?;
    if nodes::lookup(&reader, current, key)? == native.map(value) {
        return Ok(());
    }
    // One owned byte slice is retained while the journal mutates other maps.
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(native.map_or(0, |r| r.len()) as u64).map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let raw = native.map(|r| r.to_vec());
    update_raw(b, owner_scope, spec(owner, table), key, raw.as_deref())?;
    Ok(())
}
const AUX_TABLES: [u8; 8] = [
    paid::PAID_CLAIM_ENROLLMENTS_V2,
    paid::PAID_CLAIM_LOCKS_V2,
    paid::PAID_CLAIM_RECEIPTS_V2,
    paid::PAID_CLAIM_ROUNDS_V2,
    paid::PAID_CLAIM_FINALS_V2,
    crate::palw_paid_bootstrap_source_v6::PAID_BOOTSTRAP_SOURCE_TABLE_V6,
    crate::palw_claim_compact_v4::PAID_CLAIM_COMPACT_V4,
    crate::palw_security_certification_v1::PALW_SECURITY_CERTIFICATION_TABLE_V1,
];
fn catalogue(b: &TransitionBuilder<'_>, owner_scope: Hash64) -> Result<NativeRetainedV1<Vec<ComponentRootV2>>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentRootV2>(10).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(10);
    out.push(StateNodesV2(&b.state).owned_current(owner_scope, spec(LEDGER, misaka_palw_kernel::rows::TABLE_CLAIMS_V1))?);
    for t in AUX_TABLES {
        out.push(StateNodesV2(&b.state).owned_current(owner_scope, spec(AUX, t))?)
    }
    out.push(StateNodesV2(&b.state).owned_current(owner_scope, spec(CONTROL, 0))?);
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}

/// Read one precise actual source point, not a raw-ID "latest claim" alias.
/// The caller supplies the original birth and expected network/fork identities.
pub(super) fn read<'a>(
    state: &'a PalwChainStateV2,
    claim: Hash64,
    birth: PalwBlockContextV2,
    point: PalwBlockContextV2,
    network: [u8; 32],
    fired_forks: [u8; 32],
) -> Result<Option<PaidSourceViewV2<'a>>, ContextErrorV2> {
    let reader = StateNodesV2(state);
    let directory = reader.owned_current(scope(claim, birth)?, spec(DIRECTORY, 0))?;
    let Some(v) = nodes::lookup(&reader, directory, point.block.as_byte_slice())? else { return Ok(None) };
    let raw = reader.body(v)?;
    let manifest = typed::<PaidSourceManifestV2>(&raw)?;
    if manifest.version != 3
        || manifest.claim != claim
        || manifest.birth != birth
        || manifest.point != point
        || manifest.network != network
        || manifest.fired_forks != fired_forks
        || birth.daa_score > point.daa_score
        || birth.blue_score > point.blue_score
        || state.last_point.is_none_or(|p| point.daa_score > p.daa_score || point.blue_score > p.blue_score)
    {
        return Err(bad("paid source actual point/rules/cohort differs"));
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentSpecV2>(10).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<ComponentRootV2>(10).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<RowValueV2>(1).map_err(ContextErrorV2::Resource)?;
    let _catalogue_lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut expected = Vec::with_capacity(10);
    expected.push(spec(LEDGER, misaka_palw_kernel::rows::TABLE_CLAIMS_V1));
    for t in AUX_TABLES {
        expected.push(spec(AUX, t))
    }
    expected.push(spec(CONTROL, 0));
    if manifest.components.len() != expected.len() || manifest.components.iter().zip(expected).any(|(r, s)| r.spec != s) {
        return Err(bad("paid source complete catalogue differs"));
    }
    let references = PaidSourceArchiveV3 { version: 3, manifest: v, components: manifest.components.clone() };
    let encoded = encode(&references)?;
    let key = archive_key(crate::palw_claim_local_v1::palw_liability_material_id_v1(&encoded));
    let archived = aux(state, PALW_CLAIM_LOCAL_TABLE_V1, &key)?
        .ok_or(ContextErrorV2::Unavailable("paid source archive reference inventory missing"))?;
    crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&references, archived)
        .map_err(|_| bad("paid source archive reference inventory differs"))?;
    // EOF/count checked for every component, including empty receipt/round
    // tables. Missing committed nodes/bodies can never become empty-ready.
    for root in &manifest.components {
        nodes::visit_nodes(&reader, *root, |_, node| {
            if let ContextNodeV2::Leaf { value, .. } = node {
                reader.body(*value)?;
            }
            Ok(())
        })?;
    }
    let view = PaidSourceViewV2 { state, manifest };
    let mut previous = None;
    for (journal, ordinal, id) in &view.manifest.events {
        if previous.is_some_and(|old| old >= *journal) || ordinal.is_some() != id.is_some() {
            return Err(bad("paid source actual event/accepted ordinal catalogue"));
        }
        previous = Some(*journal);
        if let Some(id) = id {
            let raw = view
                .row(CONTROL, 0, id.as_byte_slice())?
                .ok_or(ContextErrorV2::Unavailable("paid source accepted control body missing"))?;
            let object = typed::<PalwConsensusObjectV2>(&raw)?;
            if streamed_id(b"misaka-palw/executed-object/v2", &*object)? != *id {
                return Err(bad("paid source accepted control digest differs"));
            }
        }
    }
    Ok(Some(view))
}

fn claim_identity(row: &ClaimRowV1) -> Result<Hash64, ContextErrorV2> {
    streamed_id(
        b"misaka-palw/paid-source-native-identity/v2",
        &(row.producer, row.class_binding_id, row.job_id, &row.body, row.committed_daa),
    )
}
fn latest_manifest<'a>(
    state: &'a PalwChainStateV2,
    claim: Hash64,
) -> Result<Option<NativeRetainedV1<PaidSourceManifestV2>>, ContextErrorV2> {
    let r = StateNodesV2(state);
    let root = r.current(spec(DIRECTORY, 1))?;
    let Some(v) = nodes::lookup(&r, root, claim.as_byte_slice())? else { return Ok(None) };
    let raw = r.body(v)?;
    let m = typed::<PaidSourceManifestV2>(&raw)?;
    if m.version != 3 || m.claim != claim {
        return Err(bad("paid source latest manifest identity"));
    }
    Ok(Some(m))
}
fn actual_birth(
    b: &TransitionBuilder<'_>,
    ctx: PalwBlockContextV2,
    claim: Hash64,
    current: &ClaimRowV1,
) -> Result<Option<PalwBlockContextV2>, ContextErrorV2> {
    // New actual admission takes precedence over a prior same-raw-ID pointer,
    // even when both births happen to share the same DAA clock.
    for e in &b.entries {
        if let PalwDeltaEntryV2::KernelRouteRow { table, key, old: None, new: Some(raw) } = e {
            if *table == misaka_palw_kernel::rows::TABLE_CLAIMS_V1 && key.as_slice() == claim.as_byte_slice() {
                let row = typed::<ClaimRowV1>(raw)?;
                if row.committed_daa == ctx.daa_score && claim_identity(&row)? == claim_identity(current)? {
                    return Ok(Some(ctx));
                }
            }
        }
    }
    let Some(previous) = latest_manifest(&b.state, claim)? else { return Ok(None) };
    let previous_claim = PaidSourceViewV2 { state: &b.state, manifest: previous };
    let raw = previous_claim
        .row(LEDGER, misaka_palw_kernel::rows::TABLE_CLAIMS_V1, claim.as_byte_slice())?
        .ok_or(ContextErrorV2::Unavailable("paid source previous native identity missing"))?;
    let row = typed::<ClaimRowV1>(&raw)?;
    if claim_identity(&row)? != claim_identity(current)? {
        return Err(bad("paid source raw ID reused without an actual admission observation"));
    }
    Ok(Some(previous_claim.birth()))
}
fn install_paid_catalogue(
    b: &mut TransitionBuilder<'_>,
    s: Hash64,
    claim: Hash64,
    a: &paid::PalwPaidClaimAuthorityV2,
) -> Result<(), ContextErrorV2> {
    let enrollment = a.enrollment.id();
    install_native(b, s, LEDGER, misaka_palw_kernel::rows::TABLE_CLAIMS_V1, claim.as_byte_slice())?;
    install_native(b, s, AUX, paid::PAID_CLAIM_ENROLLMENTS_V2, &a.context.id())?;
    let mut k = [0; 65];
    k[0] = paid::PAID_CLAIM_BASE_INDEX_PREFIX_V3;
    k[1..].copy_from_slice(claim.as_byte_slice());
    install_native(b, s, AUX, paid::PAID_CLAIM_ENROLLMENTS_V2, &k)?;
    k[0] = paid::PAID_CLAIM_ID_INDEX_PREFIX_V3;
    k[1..].copy_from_slice(&enrollment);
    install_native(b, s, AUX, paid::PAID_CLAIM_ENROLLMENTS_V2, &k)?;
    install_native(b, s, AUX, paid::PAID_CLAIM_FINALS_V2, &enrollment)?;
    install_native(b, s, AUX, paid::PAID_CLAIM_LOCKS_V2, &enrollment)?;
    for key in [
        crate::palw_paid_bootstrap_source_v6::binding_key_v6(&enrollment),
        crate::palw_paid_bootstrap_source_v6::lock_key_v6(&enrollment),
    ] {
        install_native(b, s, AUX, crate::palw_paid_bootstrap_source_v6::PAID_BOOTSTRAP_SOURCE_TABLE_V6, &key)?;
    }
    use crate::palw_security_certification_v1 as cert;
    let _key_lease = reserve_native_requests_v1(130).map_err(ContextErrorV2::Resource)?;
    let certification = cert::palw_kernel_claim_certification_id_v1(&claim.as_bytes());
    for key in [cert::certification_kernel_claim_key_v1(&certification), cert::certification_kernel_hold_key_v1(&certification)] {
        install_native(b, s, AUX, cert::PALW_SECURITY_CERTIFICATION_TABLE_V1, &key)?;
    }
    if let Some(plan) = &a.class.interactive_plan {
        // All plan positions, including not-yet-existing rounds, are accounted
        // for. Plan length and storage already belong to the signed contract.
        for round in 0..plan.rounds {
            let key = misaka_palw_kernel::claim_interactive_v3::round_id_v3(enrollment, round);
            install_native(b, s, AUX, paid::PAID_CLAIM_ROUNDS_V2, &key)?;
            if plan.version == 4 {
                let _key_lease = reserve_native_requests_v1(69).map_err(ContextErrorV2::Resource)?;
                let key = crate::palw_claim_compact_v4::compact_proof_binding_key_v4(enrollment, round);
                install_native(b, s, AUX, crate::palw_claim_compact_v4::PAID_CLAIM_COMPACT_V4, &key)?;
            }
        }
    } else {
        let ordinary = aux(&b.state, paid::PAID_CLAIM_LOCKS_V2, &enrollment)?.map(typed::<paid::PalwPaidClaimLockV2>).transpose()?;
        let bootstrap = aux(
            &b.state,
            crate::palw_paid_bootstrap_source_v6::PAID_BOOTSTRAP_SOURCE_TABLE_V6,
            &crate::palw_paid_bootstrap_source_v6::lock_key_v6(&enrollment),
        )?
        .map(typed::<crate::palw_paid_bootstrap_source_v6::PalwPaidBootstrapLockV6>)
        .transpose()?;
        let lock = bootstrap.as_ref().map(|x| &x.lock).or_else(|| ordinary.as_deref());
        if let Some(lock) = lock {
            for seat in &lock.seats {
                let signer = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(seat);
                let key = misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(enrollment, signer));
                install_native(b, s, AUX, paid::PAID_CLAIM_RECEIPTS_V2, &key)?;
            }
        }
    }
    Ok(())
}
fn capture_one(
    b: &mut TransitionBuilder<'_>,
    parent_full_state_basis: Hash64,
    prepublication_full_state_basis: Hash64,
    ctx: PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    hints: &[ExecutedOwnerHintV2],
    claim: Hash64,
) -> Result<bool, ContextErrorV2> {
    let Some(_route) = b.state.kernel_route.as_ref() else {
        return Err(ContextErrorV2::Unavailable("paid source actual kernel route missing"));
    };
    let Some(raw) = native_row(&b.state, LEDGER, misaka_palw_kernel::rows::TABLE_CLAIMS_V1, claim.as_byte_slice())? else {
        return Ok(false);
    };
    let current = typed::<ClaimRowV1>(raw)?;
    let Some(birth) = actual_birth(b, ctx, claim, &current)? else { return Ok(false) }; // unmigrated legacy source stays legacy.
    let mut key = [0; 65];
    key[0] = paid::PAID_CLAIM_BASE_INDEX_PREFIX_V3;
    key[1..].copy_from_slice(claim.as_byte_slice());
    let Some(raw) = aux(&b.state, paid::PAID_CLAIM_ENROLLMENTS_V2, &key)? else { return Ok(false) };
    let index = typed::<paid::PalwPaidClaimIndexV3>(raw)?;
    if index.version != 3 || index.kind != paid::PalwPaidClaimIndexKindV3::BaseClaim || index.owner != claim.as_bytes() {
        return Err(bad("paid archive executing base index differs"));
    }
    let a = typed::<paid::PalwPaidClaimAuthorityV2>(
        aux(&b.state, paid::PAID_CLAIM_ENROLLMENTS_V2, &index.target)?
            .ok_or(ContextErrorV2::Unavailable("paid archive actual authority missing"))?,
    )?;
    if a.version != 2
        || a.context.id() != index.target
        || a.enrollment.base_claim != claim.as_bytes()
        || a.authorized_daa != current.committed_daa
    {
        return Err(bad("paid archive executing source authority substituted"));
    }
    if birth == ctx && !hints.iter().any(|h| h.work == Work::KernelClaim { claim } && h.control.is_some()) {
        return Ok(false);
    }
    let owner_scope = scope(claim, birth)?;
    install_paid_catalogue(b, owner_scope, claim, &a)?;
    // Preserve the actual accepted signed input bytes, never a producer list of
    // arbitrary controls or a reconstructed transaction from a resulting row.
    for (i, h) in hints.iter().enumerate().filter(|(_, h)| h.work == Work::KernelClaim { claim }) {
        if let Some(control) = h.control {
            if hints[..i].iter().any(|p| p.work == h.work && p.control == Some(control)) {
                continue;
            }
            let object = objects.get(control.ordinal).ok_or_else(|| bad("paid source accepted control ordinal"))?;
            let raw = encode(object)?;
            if streamed_id(b"misaka-palw/executed-object/v2", object)? != control.object_id {
                return Err(bad("paid source accepted control identity"));
            }
            update_raw(b, owner_scope, spec(CONTROL, 0), control.object_id.as_byte_slice(), Some(&raw))?;
        }
    }
    let (network, fired_forks) =
        b.extras.claim_local_rule_context_v1.ok_or_else(|| bad("paid source executing rule context absent"))?;
    if network == [0; 32] || fired_forks == [0; 32] {
        return Err(bad("paid source executing rule context zero"));
    }
    let components = catalogue(b, owner_scope)?;
    // The manifest owns only ten fixed catalogue roots. All unchanged large
    // programs/population/material/control bodies remain content addressed.
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentRootV2>(components.len() as u64).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<RowValueV2>(1).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<(u64, Option<u64>, Option<Hash64>)>(hints.len() as u64).map_err(ContextErrorV2::Resource)?;
    let _copy_lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut events = Vec::with_capacity(hints.len());
    for h in hints.iter().filter(|h| h.work == Work::KernelClaim { claim }) {
        events.push((h.journal as u64, h.control.map(|c| c.ordinal as u64), h.control.map(|c| c.object_id)));
    }
    let manifest = PaidSourceManifestV2 {
        version: 3,
        claim,
        birth,
        point: ctx,
        network,
        fired_forks,
        parent_full_state_basis,
        prepublication_full_state_basis,
        enrollment: a.enrollment.id(),
        components: components.to_vec(),
        events,
    };
    let encoded = encode(&manifest)?;
    let meta = value(&encoded);
    update_raw(b, owner_scope, spec(DIRECTORY, 0), ctx.block.as_byte_slice(), Some(&encoded))?;
    let refs = PaidSourceArchiveV3 { version: 3, manifest: meta, components: manifest.components };
    let refs_encoded = encode(&refs)?;
    let archive = archive_key(crate::palw_claim_local_v1::palw_liability_material_id_v1(&refs_encoded));
    if let Some(old) = aux(&b.state, PALW_CLAIM_LOCAL_TABLE_V1, &archive)? {
        crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&refs, old)
            .map_err(|_| bad("paid archive immutable inventory collision"))?;
    } else {
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &archive, Some(&refs)).map_err(|e| match e {
            PalwStateV2Error::NativeResource(r) => ContextErrorV2::Resource(r),
            _ => bad("paid archive inventory journal refused"),
        })?;
    }
    // The global directory is only a latest-cohort locator, not source
    // authority. The typed reader still requires an explicit full birth.
    let before = StateNodesV2(&b.state).current(spec(DIRECTORY, 1))?;
    let old = nodes::lookup(&StateNodesV2(&b.state), before, claim.as_byte_slice())?;
    let plan = nodes::update(&StateNodesV2(&b.state), before, claim.as_byte_slice(), old, Some(meta))?;
    store::publish_component_v2(b, &plan, Some(&encoded))?;
    Ok(true)
}
/// Called only after the actual accepted-object fold and before checkpoint
/// derivation. Any error abandons the enclosing transition, as ordinary fold
/// validation does. No caller may continue using a failed builder.
pub(super) fn capture_executed_paid_sources(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
) -> Result<usize, PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(0);
    }
    let hints = effects::executed_owner_hints_v2(b, objects)?;
    if !hints.iter().any(|h| matches!(h.work, Work::KernelClaim { .. })) {
        return Ok(0);
    }
    // Hash each complete state once, before ANY archive/component publication.
    // Aux144 publication changes neither the source native rows nor their
    // actual accepted-object observations. All subjects therefore bind this
    // one executing pre-batch basis, never an intermediate/self-including root.
    let parent_full_state_basis = parent.state_root();
    let prepublication_full_state_basis = b.state.state_root();
    let mut count = 0;
    for (i, h) in hints.iter().enumerate() {
        let Work::KernelClaim { claim } = h.work else { continue };
        if hints[..i].iter().any(|p| p.work == h.work) {
            continue;
        }
        if capture_one(b, parent_full_state_basis, prepublication_full_state_basis, *ctx, objects, &hints, claim)
            .map_err(state_error)?
        {
            count += 1
        }
    }
    Ok(count)
}

/// First native consumer bridge. A migrated paid claim uses its explicit
/// scoped source; legacy claims return None for the unchanged route reader.
/// Current native outcome/revocation remains the caller's separate decision.
/// An inconsistent migrated source is an error, never a fallback approval.
pub(super) fn selected_approval_for_current_native(state: &PalwChainStateV2, claim: Hash64) -> Result<Option<bool>, ContextErrorV2> {
    let Some(manifest) = latest_manifest(state, claim)? else { return Ok(None) };
    let birth = manifest.birth;
    let point = manifest.point;
    let network = manifest.network;
    let fired = manifest.fired_forks;
    let view = read(state, claim, birth, point, network, fired)?
        .ok_or(ContextErrorV2::Unavailable("paid source latest locator lost its exact archive"))?;
    let current = native_row(state, LEDGER, misaka_palw_kernel::rows::TABLE_CLAIMS_V1, claim.as_byte_slice())?
        .ok_or(ContextErrorV2::Unavailable("paid source current native claim missing"))?;
    let current = typed::<ClaimRowV1>(current)?;
    let archived = view
        .row(LEDGER, misaka_palw_kernel::rows::TABLE_CLAIMS_V1, claim.as_byte_slice())?
        .ok_or(ContextErrorV2::Unavailable("paid source archived native claim missing"))?;
    let archived_claim = typed::<ClaimRowV1>(&archived)?;
    if claim_identity(&current)? != claim_identity(&archived_claim)? {
        return Err(bad("paid source current native cohort differs"));
    }
    // A later revocation/deletion cannot leave an earlier approval usable. This
    // comparison concerns only three fixed exact owner rows, never the world.
    let mut k = [0; 65];
    k[0] = paid::PAID_CLAIM_BASE_INDEX_PREFIX_V3;
    k[1..].copy_from_slice(claim.as_byte_slice());
    for (table, key) in [(paid::PAID_CLAIM_ENROLLMENTS_V2, k.as_slice()), (paid::PAID_CLAIM_FINALS_V2, manifest.enrollment.as_slice())]
    {
        let archived = view.row(AUX, table, key)?;
        match (aux(state, table, key)?, archived.as_deref()) {
            (None, None) => {}
            (Some(now), Some(old)) if now == old.as_slice() => {}
            _ => return Err(bad("paid source current approval changed without executing archive update")),
        }
    }
    Ok(Some(view.selected_approval()?))
}

impl PalwChainStateV2 {
    /// Read the fold-owned paid selected-approval archive in this admitted
    /// snapshot. This is a diagnostic/source read, not a lifecycle transition or
    /// an independent approval. `None` means this cohort is not migrated.
    pub fn claim_local_paid_archived_selected_approval_v2(&self, claim: Hash64) -> Result<Option<bool>, PalwStateV2Error> {
        selected_approval_for_current_native(self, claim).map_err(state_error)
    }
}

/// Strict version dispatcher shared by native carriage/row-stream preflight.
/// Version2 A rows are storage references; version3 A rows are typed sources.
pub(crate) fn scan_archive_wire_v2<R: std::io::Read>(
    frame: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
    match frame.short()? {
        2 => Vec::<ComponentRootV2>::scan_native_wire_v1(frame)?.merge(Vec::<RowValueV2>::scan_native_wire_v1(frame)?),
        3 => RowValueV2::scan_native_wire_v1(frame)?.merge(Vec::<ComponentRootV2>::scan_native_wire_v1(frame)?),
        4 => Hash64::scan_native_wire_v1(frame)?.merge(RowValueV2::scan_native_wire_v1(frame)?)?.merge(Option::<RowValueV2>::scan_native_wire_v1(frame)?)?.merge(<[RowValueV2;2]>::scan_native_wire_v1(frame)?),
        5 => Hash64::scan_native_wire_v1(frame)?.merge(Hash64::scan_native_wire_v1(frame)?)?.merge(PalwBlockContextV2::scan_native_wire_v1(frame)?)?.merge(RowValueV2::scan_native_wire_v1(frame)?)?.allocation(1,std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::Hash64,crate::palw_state_v2::PalwLegacyCarrierLocatorV9),()>>()),
        _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported scoped archive version")),
    }
}

pub(super) fn verify_archived_source_v3(state: &PalwChainStateV2, archive: &PaidSourceArchiveV3) -> Result<(), ContextErrorV2> {
    if archive.version != 3 {
        return Err(bad("paid source archive version"));
    }
    let raw = StateNodesV2(state).body(archive.manifest)?;
    let manifest = typed::<PaidSourceManifestV2>(&raw)?;
    if manifest.components != archive.components {
        return Err(bad("paid source manifest/archive catalogue differs"));
    }
    let view = read(state, manifest.claim, manifest.birth, manifest.point, manifest.network, manifest.fired_forks)?
        .ok_or(ContextErrorV2::Unavailable("paid source archive lacks its authenticated exact-point directory"))?;
    if view.manifest.enrollment != manifest.enrollment {
        return Err(bad("paid source directory substituted enrollment"));
    }
    // Check the entire typed authority/population and exact cohort even for an
    // unapproved source. The returned boolean itself does not affect restore.
    view.selected_approval()?;
    Ok(())
}

/// A self-consistent archive is not its own network authority. Validate its
/// source point against the configured full network after structural restore.
pub(super) fn validate_network_context_v3(
    state: &PalwChainStateV2,
    global: &crate::config::params::Params,
) -> Result<(), PalwStateV2Error> {
    let Some(route)=&state.kernel_route else { return Ok(()); };
    for ((table,key),raw) in &route.aux {
        if *table!=PALW_CLAIM_LOCAL_TABLE_V1 || key.first()!=Some(&store::CONTEXT_ARCHIVE_PREFIX_V2)
            || raw.get(..2)!=Some(&[3,0]) { continue; }
        let archive=typed::<PaidSourceArchiveV3>(raw).map_err(state_error)?;
        let bytes=StateNodesV2(state).body(archive.manifest).map_err(state_error)?;
        let m=typed::<PaidSourceManifestV2>(&bytes).map_err(state_error)?;
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle)=&global.palw_consensus_mode else {
            return Err(state_error(bad("paid archive requires configured V2 network")));
        };
        let fired=crate::palw_onboarding_v1::palw_envelope_fork_digest_v1(
            global.genesis.hash,&global.fence_schedule_v1(),m.point.daa_score).as_bytes();
        if m.network!=global.consensus_identity_id().as_bytes() || m.fired_forks!=fired
            || !bundle.state.claim_local_settlement_active_at(m.point.daa_score)
            || state.last_point.is_none_or(|p|m.point.daa_score>p.daa_score||m.point.blue_score>p.blue_score) {
            return Err(state_error(bad("paid archive differs from configured source network/fence context")));
        }
        verify_archived_source_v3(state,&archive).map_err(state_error)?;
    }
    Ok(())
}
