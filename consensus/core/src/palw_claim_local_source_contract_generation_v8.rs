//! ISOLATED state_v2 child candidate; not registered or activated.
//! Actual material-income receipt lineages; not claim-admission authority.
//! A source population references shared physical instances, never raw slots.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{
    self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2,
};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_monetary_material_v1::{
    PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialRowV1 as Covenant,
};
use crate::palw_native_resource_v1::{
    NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
const SLOT: u32 = 1;
const GENERATION: u32 = 2;
const OWNERS: u32 = 3;
const SOURCE_MEMBERS: u32 = 4;
const SOURCE: u32 = 5;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 {
        owner: 10,
        table,
        schema: 8,
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
            .key(b"misaka-palw/contract-generation/v8")
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct SlotV8 {
    generation: Hash64,
}
crate::palw_native_resource_v1::wire::wire_struct!(SlotV8;Hash64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct GenerationV8 {
    id: Hash64,
    root: Hash64,
    observed_at: PalwBlockContextV2,
    ordinal: u64,
    initial_consent: Hash64,
    initial_sequence: u64,
    accepted_daa: u64,
    terms: Hash64,
    consent: Hash64,
    sequence: u64,
    binding: Option<Hash64>,
    closed: Option<PalwBlockContextV2>,
    references: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(GenerationV8;Hash64,Hash64,PalwBlockContextV2,u64,Hash64,u64,u64,Hash64,Hash64,u64,Option<Hash64>,Option<PalwBlockContextV2>,u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct OwnerV8 {
    budget: Hash64,
    source: Hash64,
    birth: PalwBlockContextV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(OwnerV8;Hash64,Hash64,PalwBlockContextV2);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct SourceV8 {
    version: u16,
    owner: OwnerV8,
    members: ComponentRootV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(SourceV8;u16,OwnerV8,ComponentRootV2);
fn native_covenant(
    s: &PalwChainStateV2,
    key: Hash64,
) -> Result<Option<NativeRetainedV1<Covenant>>, ContextErrorV2> {
    let Some(route) = &s.kernel_route else {
        return Ok(None);
    };
    let _lease = reserve_native_requests_v1(64).map_err(ContextErrorV2::Resource)?;
    route
        .aux
        .get(&(
            PALW_MONETARY_MATERIAL_TABLE_V1,
            key.as_byte_slice().to_vec(),
        ))
        .map(|raw| decode(raw))
        .transpose()
}
fn generation(s: &PalwChainStateV2, id: Hash64) -> Result<GenerationV8, ContextErrorV2> {
    let g = lookup::<GenerationV8>(s, GENERATION, None, id)?
        .ok_or(bad("contract generation absent"))?;
    if g.id != id
        || id
            != hash(
                1,
                &(
                    g.root,
                    g.observed_at,
                    g.ordinal,
                    g.initial_consent,
                    g.initial_sequence,
                    g.accepted_daa,
                    g.terms,
                ),
            )?
    {
        return Err(bad("contract generation identity differs"));
    }
    Ok(*g)
}
fn current(s: &PalwChainStateV2, key: Hash64) -> Result<Option<GenerationV8>, ContextErrorV2> {
    let Some(slot) = lookup::<SlotV8>(s, SLOT, None, key)? else {
        return Ok(None);
    };
    let g = generation(s, slot.generation)?;
    if g.root != key || g.closed.is_some() {
        return Err(bad("contract slot points to closed/different generation"));
    }
    Ok(Some(g))
}
fn matches_native(g: &GenerationV8, row: &Covenant) -> Result<bool, ContextErrorV2> {
    Ok(g.root == row.terms.root
        && g.accepted_daa == row.accepted_daa
        && g.terms == hash(2, &row.terms)?
        && g.consent == row.consent
        && g.sequence == row.sequence
        && g.binding
            == row
                .native_binding
                .as_ref()
                .map(|b| hash(3, b))
                .transpose()?)
}
fn observe_existing(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    row: &Covenant,
) -> Result<GenerationV8, ContextErrorV2> {
    if let Some(g) = current(&b.state, row.terms.root)? {
        if !matches_native(&g, row)? {
            return Err(bad("contract current native identity changed unobserved"));
        }
        return Ok(g);
    }
    let ordinal = b.entries.len() as u64;
    let terms = hash(2, &row.terms)?;
    let id = hash(
        1,
        &(
            row.terms.root,
            point,
            ordinal,
            row.consent,
            row.sequence,
            row.accepted_daa,
            terms,
        ),
    )?;
    if lookup::<GenerationV8>(&b.state, GENERATION, None, id)?.is_some() {
        return Err(bad("contract generation reused"));
    }
    let g = GenerationV8 {
        id,
        root: row.terms.root,
        observed_at: point,
        ordinal,
        initial_consent: row.consent,
        initial_sequence: row.sequence,
        accepted_daa: row.accepted_daa,
        terms,
        consent: row.consent,
        sequence: row.sequence,
        binding: row
            .native_binding
            .as_ref()
            .map(|b| hash(3, b))
            .transpose()?,
        closed: None,
        references: 0,
    };
    put(b, GENERATION, None, id, Some(&g))?;
    put(b, SLOT, None, g.root, Some(&SlotV8 { generation: id }))?;
    Ok(g)
}
/// Exact native provider-row writer hook. Replacements KEEP the physical
/// contractual generation only while native old terms/accepted clock persist.
/// A later new enrollment after real deletion gets a fresh generation.
pub(super) fn write_native_covenant_v8(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    key: Hash64,
    row: &Covenant,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return b.write_kernel_row_encoded_v1(
            PALW_MONETARY_MATERIAL_TABLE_V1,
            key.as_byte_slice(),
            Some(row),
        );
    }
    if current(&b.state,key).map_err(err)?.is_none(){
        return b.write_kernel_row_encoded_v1(PALW_MONETARY_MATERIAL_TABLE_V1,key.as_byte_slice(),Some(row));
    }
    atomic(b, |b| {
        if row.terms.root != key {
            return Err(bad("contract native root differs"));
        }
        let old = native_covenant(&b.state, key)?;
        let mut g = if let Some(old) = old {
            let mut g = observe_existing(b, point, &old)?;
            if row.terms != old.terms || row.accepted_daa != old.accepted_daa {
                return Err(bad("native replacement changed original contractual terms"));
            }
            if row.consent == old.consent {
                if row.sequence != old.sequence {
                    return Err(bad("contract update changed sequence without replacement"));
                }
            } else if old.sequence.checked_add(1) != Some(row.sequence) {
                return Err(bad("contract replacement sequence differs"));
            }
            let next_binding = row
                .native_binding
                .as_ref()
                .map(|binding| hash(3, binding))
                .transpose()?;
            if g.binding.is_some() && g.binding != next_binding {
                return Err(bad("contract replaced consumed native binding"));
            }
            g.consent = row.consent;
            g.sequence = row.sequence;
            g.binding = next_binding;
            g
        } else {
            observe_existing(b, point, row)?
        };
        g.closed = None;
        put(b, GENERATION, None, g.id, Some(&g))?;
        // Every fallible metadata operation is before the real native write. Its
        // normal writer preadmits bytes/old clones/slot before native mutation.
        b.write_kernel_row_encoded_v1(
            PALW_MONETARY_MATERIAL_TABLE_V1,
            key.as_byte_slice(),
            Some(row),
        )
        .map_err(native_err)
    })
    .map_err(err)
}
fn scope_contains(s: &PalwChainStateV2, row: &Covenant, claim: Hash64) -> bool {
    if row.terms.root == claim
        || row.terms.dependencies.contains(&claim)
        || row
            .native_binding
            .as_ref()
            .is_some_and(|n| n.claim == claim)
    {
        return true;
    }
    s.exec_v2
        .roots
        .get(&row.terms.root)
        .filter(|w| Some(w.plan_root) == row.terms.exec_plan_root)
        .is_some_and(|_| {
            s.exec_v2_source_roots
                .get(&(claim, row.terms.root))
                .copied()
                .unwrap_or(0)
                > 0
        })
}
fn actual_owner(
    s: &PalwChainStateV2,
    source: Hash64,
    birth: PalwBlockContextV2,
) -> Result<OwnerV8, ContextErrorV2> {
    let c = s
        .claims
        .get(&source)
        .ok_or(bad("contract owner requires actual live source birth"))?;
    if (c.accepted_block, c.accepted_daa, c.accepted_blue_score)
        != (birth.block, birth.daa_score, birth.blue_score)
        || !matches!(&c.source, PalwClaimSourceV2::FreePrompt { .. })
    {
        return Err(bad("contract owner birth/namespace differs"));
    }
    let e = s
        .prefix_export_reservation_v1(&birth, source)
        .map_err(native_err)?
        .ok_or(bad("contract owner lacks actual E"))?;
    Ok(OwnerV8 {
        budget: e.budget_id,
        source,
        birth,
    })
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn inventory(p: &SourceV8) -> Result<NativeRetainedV1<store::ArchiveReferencesV2>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ComponentRootV2>(1)
        .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<RowValueV2>(1)
        .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let encoded = encode(p)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        store::ArchiveReferencesV2 {
            version: 2,
            components: vec![p.members],
            literals: vec![value(&encoded)],
        },
        lease,
    ))
}
fn inventory_id(a: &store::ArchiveReferencesV2) -> Result<Hash64, ContextErrorV2> {
    Ok(value(&encode(a)?).data)
}
fn put_source(
    b: &mut TransitionBuilder<'_>,
    p: &SourceV8,
    old: Option<&SourceV8>,
) -> Result<(), ContextErrorV2> {
    let a = inventory(p)?;
    let id = inventory_id(&a)?;
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &akey(id), Some(&*a))
        .map_err(native_err)?;
    put(b, SOURCE, None, p.owner.budget, Some(p))?;
    if let Some(old) = old {
        let oldid = {
            let retained_inventory = inventory(old)?;
            inventory_id(&*retained_inventory)?
        };
        if oldid != id {
            b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(
                PALW_CLAIM_LOCAL_TABLE_V1,
                &akey(oldid),
                None,
            )
            .map_err(native_err)?;
        }
    }
    Ok(())
}
fn attach(
    b: &mut TransitionBuilder<'_>,
    owner: OwnerV8,
    g: &mut GenerationV8,
) -> Result<(), ContextErrorV2> {
    if let Some(old) = lookup::<OwnerV8>(&b.state, OWNERS, Some(g.id), owner.budget)? {
        if *old != owner {
            return Err(bad("contract source owner collision"));
        }
        return Ok(());
    }
    let old = lookup::<SourceV8>(&b.state, SOURCE, None, owner.budget)?;
    if old.as_ref().is_some_and(|p| p.owner != owner) {
        return Err(bad("contract source metadata birth differs"));
    }
    put(b, OWNERS, Some(g.id), owner.budget, Some(&owner))?;
    put(b, SOURCE_MEMBERS, Some(owner.budget), g.id, Some(&g.id))?;
    g.references = g
        .references
        .checked_add(1)
        .ok_or(bad("contract owner reference overflow"))?;
    put(b, GENERATION, None, g.id, Some(g))?;
    let p = SourceV8 {
        version: 8,
        owner,
        members: root(&b.state, SOURCE_MEMBERS, Some(owner.budget))?,
    };
    put_source(b, &p, old.as_deref())
}
/// Called at actual accepted source-birth association and again immediately
/// before retirement; only the live exact old birth can ADD source membership.
/// Post-detach income inheritance uses persisted generation owners instead.
pub(super) fn capture_source_contracts_v8(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    source: Hash64,
    birth: PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    super::palw_claim_local_material_income_lineage_v7::capture_source_v7(b, point, birth, source)?;
    atomic(b, |b| {
        let owner = actual_owner(&b.state, source, birth)?;
        let direct = super::palw_monetary_material_scope_v8::direct_roots_v8(&b.state, source)
            .count() as u64;
        let reverse = super::palw_exec_v2_material_scope_v7::reverse_roots_v7(&b.state, source)
            .count() as u64;
        let count = direct
            .checked_add(reverse)
            .ok_or(bad("source covenant candidate count overflow"))?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(count)
            .map_err(ContextErrorV2::Resource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut keys = BTreeSet::new();
        keys.extend(super::palw_monetary_material_scope_v8::direct_roots_v8(
            &b.state, source,
        ));
        // The old scope grammar admits ONLY each actual reverse composite
        // root's OWN covenant and only its matching plan, never every covenant
        // which happens to list that composite root as a dependency.
        for composite in super::palw_exec_v2_material_scope_v7::reverse_roots_v7(&b.state, source) {
            if let Some(row) = native_covenant(&b.state, composite)? {
                let work = b
                    .state
                    .exec_v2
                    .roots
                    .get(&composite)
                    .ok_or(bad("reverse composite root disappeared"))?;
                if row.terms.exec_plan_root == Some(work.plan_root) {
                    keys.insert(composite);
                }
            }
        }
        // Persist an empty population too: absence is not an attested complete zero.
        if lookup::<SourceV8>(&b.state, SOURCE, None, owner.budget)?.is_none() {
            put_source(
                b,
                &SourceV8 {
                    version: 8,
                    owner,
                    members: root(&b.state, SOURCE_MEMBERS, Some(owner.budget))?,
                },
                None,
            )?;
        }
        for key in keys {
            let row =
                native_covenant(&b.state, key)?.ok_or(bad("captured covenant disappeared"))?;
            if !scope_contains(&b.state, &row, source) {
                return Err(bad(
                    "derived covenant source candidate changed native scope",
                ));
            }
            let mut g = observe_existing(b, point, &row)?;
            attach(b, owner, &mut g)?;
        }
        Ok(())
    })
    .map_err(err)
}
/// Opaque native contractual provenance. It cannot be built from a raw root,
/// payment notification or financial record. V7 consumes it only after the
/// actual prepaid native reserve AND material Pending bind have succeeded.
pub(super) struct ContractIncomeTokenV8 {
    owner: OwnerV8,
    income: Hash64,
    generation: Hash64,
}
impl ContractIncomeTokenV8 {
    pub(super) fn source(&self) -> Hash64 {
        self.owner.source
    }
    pub(super) fn birth(&self) -> PalwBlockContextV2 {
        self.owner.birth
    }
    pub(super) fn budget(&self) -> Hash64 {
        self.owner.budget
    }
    pub(super) fn income(&self) -> Hash64 {
        self.income
    }
    pub(super) fn generation(&self) -> Hash64 {
        self.generation
    }
}
/// Hook AFTER provider fund's reserve_source_income + actual material bind.
/// Replacement's new fee contexts inherit the SAME existing covenant generation
/// owners even after the old source detached. Fresh same-root enrollment does not.
pub(super) fn reference_provider_income_v8(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    enrollment: &crate::palw_monetary_material_v1::PalwMonetaryMaterialEnrollmentV1,
    provider: PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(());
    }
    if current(&b.state,enrollment.terms.root).map_err(err)?.is_none(){return Ok(())}
    atomic(b, |b| {
        let row = native_covenant(&b.state, enrollment.terms.root)?
            .ok_or(bad("provider income has no actual covenant"))?;
        if row.terms != enrollment.terms {
            return Err(bad("provider income original terms differ"));
        }
        let g = current(&b.state, row.terms.root)?
            .ok_or(bad("provider income generation unobserved"))?;
        if !matches_native(&g, &row)? {
            return Err(bad("provider income native generation differs"));
        }
        let offer = enrollment
            .offers
            .iter()
            .find(|o| o.provider == provider)
            .ok_or(bad("provider offer absent"))?;
        let p = row
            .providers
            .iter()
            .find(|p| {
                p.provider == provider
                    && p.fee_context == enrollment.id()
                    && p.fee_accepted_daa == point.daa_score
            })
            .ok_or(bad("provider fee generation not accepted"))?;
        if p.prepaid_fee_sompi != offer.prepaid_fee_sompi {
            return Err(bad("provider income amount differs"));
        }
        let income = super::palw_bond_budget_fold_v1::source_income_id_v1(
            enrollment.id(),
            enrollment.terms.root,
            &provider,
        );
        let reserve = b
            .state
            .bond_budget
            .as_ref()
            .and_then(|book| book.claim_row(&income))
            .ok_or(bad("provider native source reserve absent"))?;
        if reserve.bond != provider
            || reserve.accepted_daa != point.daa_score
            || reserve.reserved.reward_sompi != offer.prepaid_fee_sompi
        {
            return Err(bad("provider native reserve identity differs"));
        }
        let count = root(&b.state, OWNERS, Some(g.id))?.rows;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<OwnerV8>(count)
            .map_err(ContextErrorV2::Resource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut owners =
            Vec::with_capacity(usize::try_from(count).map_err(|_| bad("provider owner capacity"))?);
        let reader = StateNodesV2(&b.state);
        nodes::complete_rows(&reader, root(&b.state, OWNERS, Some(g.id))?, |key, v| {
            let raw = reader.body(v)?;
            let owner = decode::<OwnerV8>(&raw)?;
            if key != owner.budget.as_byte_slice() {
                return Err(bad("provider source owner key differs"));
            }
            owners.push(*owner);
            Ok(())
        })?;
        for owner in owners {
            let token = ContractIncomeTokenV8 {
                owner,
                income,
                generation: g.id,
            };
            super::palw_claim_local_material_income_lineage_v7::reference_contract_income_v8(
                b, &token,
            )
            .map_err(native_err)?;
        }
        Ok(())
    })
    .map_err(err)
}
/// A native Release observation, not a new timer/fee rule. Caller is the actual
/// signed Release arm AFTER its existing consent/tail/challenge/raw-hold checks,
/// native finality admission and recovery::released success. Native removal is
/// the LAST operation, so failure before it rewinds only owned AUX metadata.
pub(super) fn delete_released_covenant_v8(
    b: &mut TransitionBuilder<'_>,
    point: PalwBlockContextV2,
    row: &Covenant,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return b.write_kernel_row_encoded_v1::<Covenant>(
            PALW_MONETARY_MATERIAL_TABLE_V1,
            row.terms.root.as_byte_slice(),
            None,
        );
    }
    if current(&b.state,row.terms.root).map_err(err)?.is_none(){
        return b.write_kernel_row_encoded_v1::<Covenant>(PALW_MONETARY_MATERIAL_TABLE_V1,row.terms.root.as_byte_slice(),None);
    }
    atomic(b, |b| {
        let actual = native_covenant(&b.state, row.terms.root)?
            .ok_or(bad("released native covenant absent"))?;
        if *actual != *row || row.tail_block.is_none() || !row.challenges.is_empty() {
            return Err(bad(
                "Release observation differs from actual native covenant",
            ));
        }
        if let Some(
            crate::palw_material_release_recovery_v2::PalwMaterialRecoveryRowV2::Terminal(receipt),
        ) = b.state.material_recovery_row_v2(row.terms.root)
        {
            if receipt.final_consent != row.consent
                || Some(receipt.tail_block) != row.tail_block
                || receipt.release_block != point.block
            {
                return Err(bad("Release terminal receipt substitutes generation"));
            }
        }
        let mut g = observe_existing(b, point, row)?;
        g.closed = Some(point);
        put(b, GENERATION, None, g.id, Some(&g))?;
        put::<SlotV8>(b, SLOT, None, g.root, None)?;
        if g.references == 0 {
            put::<GenerationV8>(b, GENERATION, None, g.id, None)?;
        }
        b.write_kernel_row_encoded_v1::<Covenant>(
            PALW_MONETARY_MATERIAL_TABLE_V1,
            row.terms.root.as_byte_slice(),
            None,
        )
        .map_err(native_err)
    })
    .map_err(err)
}
pub(super) fn source_covenants_pending_v8(
    s: &PalwChainStateV2,
    budget: Hash64,
    source: Hash64,
    birth: PalwBlockContextV2,
) -> Result<bool, ContextErrorV2> {
    let p = lookup::<SourceV8>(s, SOURCE, None, budget)?.ok_or(ContextErrorV2::Unavailable(
        "source covenant population not captured",
    ))?;
    if p.version != 8
        || p.owner
            != (OwnerV8 {
                budget,
                source,
                birth,
            })
        || p.members != root(s, SOURCE_MEMBERS, Some(budget))?
    {
        return Err(bad("source covenant population birth differs"));
    }
    let reader = StateNodesV2(s);
    let mut pending = false;
    nodes::complete_rows(&reader, p.members, |key, v| {
        let raw = reader.body(v)?;
        let id = decode::<Hash64>(&raw)?;
        if key != id.as_byte_slice() {
            return Err(bad("source contract member key differs"));
        }
        let g = generation(s, *id)?;
        let owner = lookup::<OwnerV8>(s, OWNERS, Some(g.id), budget)?
            .ok_or(bad("source contract reverse owner absent"))?;
        if *owner != p.owner {
            return Err(bad("source contract reverse birth differs"));
        }
        pending |= g.closed.is_none();
        Ok(())
    })?;
    Ok(pending)
}
/// V7's only post-detach attachment authority. No native current claim lookup.
pub(super) fn verify_income_token_v8(
    s: &PalwChainStateV2,
    t: &ContractIncomeTokenV8,
) -> Result<Hash64, ContextErrorV2> {
    let g = generation(s, t.generation)?;
    if g.closed.is_some() || current(s, g.root)?.map(|p| p.id) != Some(g.id) {
        return Err(bad("income token contractual generation already closed"));
    }
    let owner = lookup::<OwnerV8>(s, OWNERS, Some(g.id), t.owner.budget)?
        .ok_or(bad("income token lacks original source membership"))?;
    if *owner != t.owner {
        return Err(bad("income token original source differs"));
    }
    Ok(g.root)
}
fn drop_empty_owned(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Hash64,
) -> Result<(), ContextErrorV2> {
    let r = root(&b.state, table, Some(scope))?;
    if r.rows != 0 || r.node.is_some() {
        return Err(bad("contract owned directory still live"));
    }
    let mut key = [0u8; 84];
    key[0] = b'I';
    key[1..3].copy_from_slice(&2u16.to_le_bytes());
    key[3..67].copy_from_slice(scope.as_byte_slice());
    spec(table)
        .serialize(&mut &mut key[67..])
        .map_err(|_| bad("contract owned directory codec"))?;
    let _lease = reserve_native_requests_v1(84).map_err(ContextErrorV2::Resource)?;
    b.write_kernel_row(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec(), None)
        .map_err(native_err)
}
/// A narrow token for provider COVENANT closure only. It grants no conclusion
/// about finite staged clocks, source checker work creation or all material.
pub(super) struct ClosedProviderPopulationV8 {
    owner: OwnerV8,
}
pub(super) fn closed_provider_population_v8(
    s: &PalwChainStateV2,
    budget: Hash64,
    source: Hash64,
    birth: PalwBlockContextV2,
) -> Result<Option<ClosedProviderPopulationV8>, ContextErrorV2> {
    if source_covenants_pending_v8(s, budget, source, birth)? {
        return Ok(None);
    }
    Ok(Some(ClosedProviderPopulationV8 {
        owner: OwnerV8 {
            budget,
            source,
            birth,
        },
    }))
}
/// Final E collector invokes only after ALL other actual owners authorize GC.
/// Native provider rows and signatures are never deleted by this metadata GC.
pub(super) fn drop_source_contracts_v8(
    b: &mut TransitionBuilder<'_>,
    closed: &ClosedProviderPopulationV8,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let owner = closed.owner;
        if source_covenants_pending_v8(&b.state, owner.budget, owner.source, owner.birth)? {
            return Err(bad("source provider population reopened"));
        }
        let p = lookup::<SourceV8>(&b.state, SOURCE, None, owner.budget)?
            .ok_or(bad("source contract population disappeared"))?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Hash64>(p.members.rows)
            .map_err(ContextErrorV2::Resource)?;
        let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut ids = Vec::with_capacity(
            usize::try_from(p.members.rows).map_err(|_| bad("source contract GC capacity"))?,
        );
        let r = StateNodesV2(&b.state);
        nodes::complete_rows(&r, p.members, |k, _| {
            ids.push(borsh::from_slice(k).map_err(|_| bad("contract GC member key"))?);
            Ok(())
        })?;
        for id in ids {
            let mut g = generation(&b.state, id)?;
            g.references = g
                .references
                .checked_sub(1)
                .ok_or(bad("contract source references underflow"))?;
            put::<OwnerV8>(b, OWNERS, Some(id), owner.budget, None)?;
            put::<Hash64>(b, SOURCE_MEMBERS, Some(owner.budget), id, None)?;
            if g.closed.is_some() && g.references == 0 {
                drop_empty_owned(b, OWNERS, id)?;
                put::<GenerationV8>(b, GENERATION, None, id, None)?;
            } else {
                put(b, GENERATION, None, id, Some(&g))?;
            }
        }
        let aid = {
            let retained_inventory = inventory(&p)?;
            inventory_id(&*retained_inventory)?
        };
        drop_empty_owned(b, SOURCE_MEMBERS, owner.budget)?;
        put::<SourceV8>(b, SOURCE, None, owner.budget, None)?;
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(
            PALW_CLAIM_LOCAL_TABLE_V1,
            &akey(aid),
            None,
        )
        .map_err(native_err)?;
        Ok(())
    })
    .map_err(err)
}
/// Actual retire hook, before consumer/EXEC context deletion. Only the genuine
/// currently retiring birth is matched; older same-raw-ID E records are ignored.
pub(super) fn capture_retiring_source_v8(b:&mut TransitionBuilder<'_>,source:Hash64,c:&PalwClaimStateV2)->Result<(),PalwStateV2Error>{
    let Some(point)=b.claim_local_lock_point_v6 else{return Ok(())};
    if !matches!(c.source,PalwClaimSourceV2::FreePrompt{..}){return Ok(())}
    let Some(origin)=super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&b.state,b.params,source,c)? else{return Ok(())};
    let birth=origin.accepting_point();
    if b.state.prefix_export_reservation_v1(&birth,source)?.is_none(){return Ok(())}
    capture_source_contracts_v8(b,point,source,birth)
}

pub(super) fn validate_v8(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    use crate::palw_native_resource_v1::BTreeNodeEnvelopeV1 as Node;
    let count = root(s, GENERATION, None)?.rows;
    let sources = root(s, SOURCE, None)?.rows;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Node<Hash64, u64>>(count)
        .map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<Node<Hash64, ()>>(sources)
        .map_err(ContextErrorV2::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut refs = BTreeMap::<Hash64, u64>::new();
    let mut inventories = BTreeSet::new();
    let reader = StateNodesV2(s);
    nodes::complete_rows(&reader, root(s, GENERATION, None)?, |k, v| {
        let raw = reader.body(v)?;
        let g = decode::<GenerationV8>(&raw)?;
        if k != g.id.as_byte_slice() || generation(s, g.id)? != *g {
            return Err(bad("contract restored generation identity differs"));
        }
        if g.closed.is_none() {
            let native = native_covenant(s, g.root)?
                .ok_or(bad("active tracked covenant native row missing"))?;
            if current(s, g.root)?.map(|r| r.id) != Some(g.id) || !matches_native(&g, &native)? {
                return Err(bad("tracked covenant/native current differs"));
            }
        } else if g.references == 0 || current(s, g.root)?.is_some_and(|r| r.id == g.id) {
            return Err(bad("released covenant still current or unreferenced"));
        }
        if root(s, OWNERS, Some(g.id))?.rows != g.references {
            return Err(bad("contract reference/member count differs"));
        }
        refs.insert(g.id, 0);
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, SLOT, None)?, |k, v| {
        let nativekey: Hash64 = borsh::from_slice(k).map_err(|_| bad("contract slot key codec"))?;
        let raw = reader.body(v)?;
        let slot = decode::<SlotV8>(&raw)?;
        let g = generation(s, slot.generation)?;
        if g.root != nativekey
            || g.closed.is_some()
            || current(s, nativekey)?.map(|r| r.id) != Some(g.id)
        {
            return Err(bad("contract slot reverse binding differs"));
        }
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, SOURCE, None)?, |k, v| {
        let raw = reader.body(v)?;
        let p = decode::<SourceV8>(&raw)?;
        if p.version != 8
            || k != p.owner.budget.as_byte_slice()
            || p.members != root(s, SOURCE_MEMBERS, Some(p.owner.budget))?
        {
            return Err(bad("contract source population root differs"));
        }
        let e = s
            .prefix_export_reservation_v1(&p.owner.birth, p.owner.source)
            .map_err(native_err)?
            .ok_or(bad("contract source exact E absent"))?;
        if e.budget_id != p.owner.budget || e.point != p.owner.birth {
            return Err(bad("contract source actual birth differs"));
        }
        let a = inventory(&p)?;
        let id = inventory_id(&a)?;
        inventories.insert(id);
        let route = s
            .kernel_route
            .as_ref()
            .ok_or(bad("contract source route absent"))?;
        let _lease = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
        let stored = route
            .aux
            .get(&(PALW_CLAIM_LOCAL_TABLE_V1, akey(id).to_vec()))
            .ok_or(bad("contract source A2 inventory absent"))?;
        crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&*a, stored)
            .map_err(|_| bad("contract source A2 differs"))?;
        nodes::complete_rows(&reader, p.members, |k, v| {
            let raw = reader.body(v)?;
            let id = decode::<Hash64>(&raw)?;
            if k != id.as_byte_slice() {
                return Err(bad("contract source generation key differs"));
            }
            let owner = lookup::<OwnerV8>(s, OWNERS, Some(*id), p.owner.budget)?
                .ok_or(bad("contract source reverse membership absent"))?;
            if *owner != p.owner {
                return Err(bad("contract source reverse birth differs"));
            }
            let n = refs
                .get_mut(&*id)
                .ok_or(bad("contract source names absent generation"))?;
            *n = n
                .checked_add(1)
                .ok_or(bad("contract restored references overflow"))?;
            Ok(())
        })?;
        Ok(())
    })?;
    nodes::complete_rows(&reader, root(s, GENERATION, None)?, |_, v| {
        let raw = reader.body(v)?;
        let g = decode::<GenerationV8>(&raw)?;
        if refs.get(&g.id).copied() != Some(g.references) {
            return Err(bad("contract restored source reference total differs"));
        }
        nodes::complete_rows(&reader, root(s, OWNERS, Some(g.id))?, |k, v| {
            let raw = reader.body(v)?;
            let owner = decode::<OwnerV8>(&raw)?;
            if k != owner.budget.as_byte_slice() {
                return Err(bad("contract owner key differs"));
            }
            let p = lookup::<SourceV8>(s, SOURCE, None, owner.budget)?
                .ok_or(bad("contract owner source population absent"))?;
            if p.owner != *owner
                || lookup::<Hash64>(s, SOURCE_MEMBERS, Some(owner.budget), g.id)?.as_deref()
                    != Some(&g.id)
            {
                return Err(bad("contract owner/source reverse closure differs"));
            }
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
                let ks = match k.len() {
                    18 => borsh::from_slice::<ComponentSpecV2>(&k[1..]).ok(),
                    84 if k[1..3] == [2, 0] => borsh::from_slice::<ComponentSpecV2>(&k[67..]).ok(),
                    _ => None,
                };
                if r.spec.owner != 10 && ks.is_none_or(|r| r.owner != 10) {
                    continue;
                }
                if ks != Some(r.spec)
                    || !matches!(
                        r.spec.table,
                        SLOT | GENERATION | OWNERS | SOURCE_MEMBERS | SOURCE
                    )
                    || r.spec != spec(r.spec.table)
                {
                    return Err(bad("unallocated contract owner10 root/key profile"));
                }
                match k.len() {
                    18 if matches!(r.spec.table, SLOT | GENERATION | SOURCE) => {}
                    84 if k[1..3] == [2, 0] => {
                        let scope = borsh::from_slice::<Hash64>(&k[3..67])
                            .map_err(|_| bad("contract owner scope codec"))?;
                        match r.spec.table {
                            OWNERS => {
                                if lookup::<GenerationV8>(s, GENERATION, None, scope)?.is_none() {
                                    return Err(bad("orphan contract generation-owner root"));
                                }
                            }
                            SOURCE_MEMBERS => {
                                let p = lookup::<SourceV8>(s, SOURCE, None, scope)?
                                    .ok_or(bad("orphan contract source root"))?;
                                if p.members != *r {
                                    return Err(bad("contract source rooted profile differs"));
                                }
                            }
                            _ => return Err(bad("contract global table at owned scope")),
                        }
                    }
                    _ => return Err(bad("contract current root key allocation")),
                }
            } else if k.first() == Some(&b'A') && raw.get(..2) == Some(&[2, 0]) {
                let a = decode::<store::ArchiveReferencesV2>(raw)?;
                if a.components.iter().any(|r| r.spec.owner == 10)
                    && (k.len() != 65
                        || !inventories.contains(
                            &borsh::from_slice::<Hash64>(&k[1..])
                                .map_err(|_| bad("contract A2 key codec"))?,
                        ))
                {
                    return Err(bad("orphan contract source A2 inventory"));
                }
            }
        }
    }
    Ok(())
}
/// Recheck under the ACTUAL old-source/native lease before combining with a
/// detached financial population's native two-clock expiry. A carried token is
/// never a timeless permission across a reorg or state change.
pub(super) fn verify_closed_provider_population_v8(
    s: &PalwChainStateV2,
    token: &ClosedProviderPopulationV8,
    budget: Hash64,
    source: Hash64,
    birth: PalwBlockContextV2,
) -> Result<bool, ContextErrorV2> {
    if token.owner
        != (OwnerV8 {
            budget,
            source,
            birth,
        })
    {
        return Err(bad("provider closed proof changed source birth"));
    }
    Ok(!source_covenants_pending_v8(s, budget, source, birth)?)
}
