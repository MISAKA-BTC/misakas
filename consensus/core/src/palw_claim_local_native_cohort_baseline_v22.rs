//! Native birth census provenance; data identity only. No approval, model
//! correctness assertion, material paid-alias standing or producer time duty.
//! Registry: owner18/schema1 table10 baseline, table11 per-execution-root tally.
//! Mint only at the actual native genesis boundary; unsupported legacy remains
//! Missing for its own queried root. Ordinary updates use actual native journal.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
pub(super) const BASELINE: u32 = 10;
pub(super) const ROOTS: u32 = 11;
fn spec(t: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 18, table: t, schema: 1, key_bytes: 64, value_bytes: u64::MAX }
}
fn bad(s: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(s)
}
fn err(e: ContextErrorV2) -> PalwStateV2Error {
    super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn root(s: &PalwChainStateV2, t: u32) -> Result<ComponentRootV2, ContextErrorV2> {
    StateNodesV2(s).current(spec(t))
}
// Canonical empty native genesis is fixed grammar, not a retained world view.
// Still admit both its typed value and actual root workspace before hashing.
fn canonical_genesis_root_v22() -> Result<Hash64, PalwStateV2Error> {
    let genesis = PalwChainStateV2::genesis();
    let _typed = reserve_native_requests_v1(
        crate::palw_native_resource_v1::native_typed_value_requests_v1(&genesis).map_err(PalwStateV2Error::NativeResource)?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    let bytes = borsh::object_length(&PalwStateCarriageV2::from_state(&genesis))
        .map_err(|_| err(bad("native genesis canonical carriage length")))? as u64;
    let _workspace =
        crate::palw_native_resource_v1::reserve_native_root_workspace_v1(bytes).map_err(PalwStateV2Error::NativeResource)?;
    Ok(genesis.state_root())
}

fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("native census baseline codec"),
    })
}
fn read<T: BorshDeserialize + NativeWireScanV1>(
    s: &PalwChainStateV2,
    t: u32,
    k: Hash64,
) -> Result<Option<NativeRetainedV1<T>>, ContextErrorV2> {
    let r = StateNodesV2(s);
    let Some(rv) = nodes::lookup(&r, root(s, t)?, k.as_byte_slice())? else { return Ok(None) };
    decode(&r.body(rv)?).map(Some)
}
fn put<T: BorshSerialize>(b: &mut TransitionBuilder<'_>, t: u32, k: Hash64, v: &T) -> Result<(), PalwStateV2Error> {
    let n = borsh::object_length(v).map_err(|_| err(bad("native census baseline encoding")))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut raw = Vec::with_capacity(n);
    v.serialize(&mut raw).map_err(|_| err(bad("native census baseline serialization")))?;
    let reader = StateNodesV2(&b.state);
    let before = root(&b.state, t).map_err(err)?;
    let old = nodes::lookup(&reader, before, k.as_byte_slice()).map_err(err)?;
    let rv = RowValueV2 { data: crate::palw_claim_local_v1::palw_liability_material_id_v1(&raw), bytes: raw.len() as u64 };
    let plan = nodes::update(&reader, before, k.as_byte_slice(), old, Some(rv)).map_err(err)?;
    store::publish_component_v2(b, &plan, Some(&raw)).map_err(err)
}
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
struct BaselineV22 {
    version: u16,
    genesis_native_root: Hash64,
    genesis_block: Hash64,
    ruleset: crate::Hash,
    observed: PalwBlockContextV2,
    roots: ComponentRootV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(BaselineV22;u16,Hash64,Hash64,crate::Hash,PalwBlockContextV2,ComponentRootV2);
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
struct RootV22 {
    version: u16,
    execution_root: Hash64,
    seen_sources: u64,
    ambiguous: bool,
    observed: PalwBlockContextV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(RootV22;u16,Hash64,u64,bool,PalwBlockContextV2);
/// Exactly once adjacent to native census observe, after all actual Claim
/// changes and before Source8/15 postbatch capture. No repeated full-world view.
pub(super) fn observe_native_cohort_baseline_v22(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(());
    }
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(err(bad("native census baseline lacks actual executing point")));
    }
    let previous = read::<BaselineV22>(&b.state, BASELINE, Hash64::default()).map_err(err)?;
    let mut baseline = if let Some(old) = previous.as_ref() {
        if old.version != 1
            || old.roots != root(&b.state, ROOTS).map_err(err)?
            || parent.last_point.is_some_and(|p| old.observed.blue_score > p.blue_score || old.observed.daa_score > p.daa_score)
        {
            return Err(err(bad("native census baseline previous owner/point differs")));
        }
        **old
    } else {
        // A partial/midchain empty view is NOT a genesis witness.
        if parent.last_point.is_some() || !parent.claims.is_empty() {
            return Ok(());
        }
        let Some(ruleset) = b.extras.native_fact_ruleset else { return Ok(()) };
        if ctx.blue_score != 0 || b.extras.kernel_route.as_ref().is_none_or(|native| native.chain_genesis != ctx.block) {
            return Ok(());
        }
        // The existing global GENESIS boundary reader admits its actual complete
        // parent/root scratch. This is once, never charged to an unrelated source.
        let _typed = reserve_native_requests_v1(
            crate::palw_native_resource_v1::native_typed_value_requests_v1(parent).map_err(PalwStateV2Error::NativeResource)?,
        )
        .map_err(PalwStateV2Error::NativeResource)?;
        let prepared = super::palw_claim_local_execution_cohort_census_v15::prepare_global_migration_v15(parent, b.params, ctx)?;
        if prepared.parent_root() != canonical_genesis_root_v22()? {
            return Ok(());
        }
        // The real genesis has no prior route. Publish its normal configured native
        // header before the first scoped row; never synthesize a tip or source.
        super::palw_kernel_route_fold_v1::ensure_route_header(b, ctx)?;
        BaselineV22 {
            version: 1,
            genesis_native_root: prepared.parent_root(),
            genesis_block: ctx.block,
            ruleset,
            observed: *ctx,
            roots: root(&b.state, ROOTS).map_err(err)?,
        }
    };
    let births = b
        .entries
        .iter()
        .filter(|e| matches!(e,PalwDeltaEntryV2::Claim{old:None,new:Some(c),..}if c.execution_root!=Hash64::default()))
        .count();
    if previous.is_some() && births == 0 {
        return Ok(());
    }
    if previous.as_ref().is_some_and(|old| old.observed.blue_score >= ctx.blue_score || old.observed.daa_score > ctx.daa_score) {
        return Err(err(bad("native census baseline observed twice or backwards")));
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, RootV22>>(births as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(births as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut updates = BTreeMap::<Hash64, RootV22>::new();
    let mut seen = BTreeSet::new();
    for e in &b.entries {
        let PalwDeltaEntryV2::Claim { key: id, old: None, new: Some(c) } = e else { continue };
        if c.execution_root == Hash64::default() {
            continue;
        }
        let k = c.execution_root;
        // The native lease above precedes even the Entry lookup. BTreeMap's
        // vacant entry allocates only at insert, after the exact old row read.
        let r = match updates.entry(k) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let old = read::<RootV22>(&b.state, ROOTS, k).map_err(err)?;
                entry.insert(old.as_deref().copied().unwrap_or(RootV22 {
                    version: 1,
                    execution_root: k,
                    seen_sources: 0,
                    ambiguous: false,
                    observed: *ctx,
                }))
            }
        };
        if r.version != 1 || r.execution_root != k {
            return Err(err(bad("native census root identity")));
        }
        let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/native-born-census/v22").to_state();
        hash.update(id.as_byte_slice());
        hash.update(c.accepted_block.as_byte_slice());
        hash.update(&c.accepted_daa.to_le_bytes());
        hash.update(&c.accepted_blue_score.to_le_bytes());
        let mut raw = [0; 64];
        raw.copy_from_slice(hash.finalize().as_bytes());
        if !seen.insert(Hash64::from_bytes(raw)) {
            r.ambiguous = true;
        }
        r.seen_sources = r.seen_sources.checked_add(1).ok_or(PalwStateV2Error::Overflow("native census birth count"))?;
        r.observed = *ctx;
    }
    for (k, row) in updates {
        put(b, ROOTS, k, &row)?;
    }
    baseline.observed = *ctx;
    baseline.roots = root(&b.state, ROOTS).map_err(err)?;
    put(b, BASELINE, Hash64::default(), &baseline)
}
/// Authenticated CURRENT population of exactly one queried root. It is data
/// identity only, not a historical scalar, paid standing or approval. Absence
/// of a per-root observation is a genesis-proven current empty population.
#[derive(Clone, Copy)]
pub(super) struct NativeRootPopulationV25 {
    total: u64,
    observed: Option<PalwBlockContextV2>,
}
impl NativeRootPopulationV25 {
    pub(super) fn total_v25(&self) -> u64 {
        self.total
    }
    pub(super) fn observed_v25(&self) -> Option<PalwBlockContextV2> {
        self.observed
    }
}
pub(super) fn current_native_root_population_v25(
    s: &PalwChainStateV2,
    execution_root: Hash64,
) -> Result<Option<NativeRootPopulationV25>, PalwStateV2Error> {
    let Some(b) = read::<BaselineV22>(s, BASELINE, Hash64::default()).map_err(err)? else { return Ok(None) };
    if b.version != 1 || b.roots != root(s, ROOTS).map_err(err)? {
        return Err(err(bad("native census baseline scope changed")));
    }
    let Some(r) = read::<RootV22>(s, ROOTS, execution_root).map_err(err)? else {
        return Ok(Some(NativeRootPopulationV25 { total: 0, observed: None }));
    };
    if r.version != 1
        || r.execution_root != execution_root
        || r.seen_sources == 0
        || r.observed.blue_score > b.observed.blue_score
        || r.observed.daa_score > b.observed.daa_score
    {
        return Err(err(bad("native census root invalid")));
    }
    if r.ambiguous {
        return Ok(None);
    }
    Ok(Some(NativeRootPopulationV25 { total: r.seen_sources, observed: Some(r.observed) }))
}
/// A scalar historical count is available only if THIS queried root has not
/// advanced past the consumer. Unrelated global-root observations do not alter
/// this scope. The stronger all-current-members proof is used by the visitor.
pub(super) fn expected_native_root_births_v22(
    s: &PalwChainStateV2,
    execution_root: Hash64,
    consumer: PalwBlockContextV2,
) -> Result<Option<u64>, PalwStateV2Error> {
    let Some(population) = current_native_root_population_v25(s, execution_root)? else { return Ok(None) };
    if population.observed_v25().is_some_and(|p| p.blue_score > consumer.blue_score || p.daa_score > consumer.daa_score) {
        return Ok(None);
    }
    Ok(Some(population.total_v25()))
}
pub(super) fn validate_native_baseline_v22(s: &PalwChainStateV2, p: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    let roots = root(s, ROOTS).map_err(err)?;
    let Some(b) = read::<BaselineV22>(s, BASELINE, Hash64::default()).map_err(err)? else {
        if roots.rows != 0 {
            return Err(err(bad("native census roots lack actual baseline")));
        }
        return Ok(());
    };
    if root(s, BASELINE).map_err(err)?.rows != 1
        || b.version != 1
        || b.genesis_native_root != canonical_genesis_root_v22()?
        || b.roots != roots
        || !p.claim_local_settlement_active_at(b.observed.daa_score)
        || s.last_point.is_none_or(|tip| b.observed.blue_score > tip.blue_score || b.observed.daa_score > tip.daa_score)
    {
        return Err(err(bad("native census baseline cold owner/clock")));
    }
    nodes::complete_rows(&StateNodesV2(s), roots, |key, rv| {
        let raw = StateNodesV2(s).body(rv)?;
        let r = decode::<RootV22>(&raw)?;
        if r.version != 1
            || key != r.execution_root.as_byte_slice()
            || r.execution_root == Hash64::default()
            || r.seen_sources == 0
            || r.observed.blue_score > b.observed.blue_score
            || r.observed.daa_score > b.observed.daa_score
        {
            return Err(bad("native census rooted cold identity/count/clock"));
        }
        Ok(())
    })
    .map_err(err)
}
pub(super) fn validate_network_v22(s: &PalwChainStateV2, global: &crate::config::params::Params) -> Result<(), PalwStateV2Error> {
    if let Some(b) = read::<BaselineV22>(s, BASELINE, Hash64::default()).map_err(err)?
        && (b.ruleset != global.consensus_params_id() || b.genesis_block != global.genesis.hash)
    {
        return Err(err(bad("native census baseline belongs to another ruleset")));
    }
    Ok(())
}
/// Canonical own birth/update shapes. Context VALUES are not a signed scope;
/// only fixed native field widths/specs are returned. Not an actual witness.
pub(super) fn contribution_shapes_v22() -> Result<[(ComponentSpecV2, u64); 2], PalwStateV2Error> {
    let zero = Hash64::default();
    let point = (&zero, 0u64, 0u64, 0u64);
    let component = (&spec(ROOTS), Some(&zero), 0u64);
    let header = borsh::object_length(&(1u16, &zero, &zero, crate::Hash::default(), &point, &component))
        .map_err(|_| err(bad("native census header shape")))? as u64;
    let member = borsh::object_length(&(1u16, &zero, 0u64, false, &point)).map_err(|_| err(bad("native census tally shape")))? as u64;
    Ok([(spec(BASELINE), header), (spec(ROOTS), member)])
}

/// Availability evidence borrowed from the actual parent. This carries no
/// payment, approval, material-standing or final no-F1 authority. The lifetime
/// prevents mutation of this parent while the evidence is retained; postbatch
/// Source15 capture must independently authenticate the actual resulting scope.
pub(super) struct NativeCohortParentContextV22<'a> {
    parent: &'a PalwChainStateV2,
    candidate: PalwBlockContextV2,
    execution_root: Hash64,
}
impl NativeCohortParentContextV22<'_> {
    pub(super) fn parent_point_v22(&self) -> Option<PalwBlockContextV2> {
        self.parent.last_point
    }
    pub(super) fn candidate_point_v22(&self) -> PalwBlockContextV2 {
        self.candidate
    }
    pub(super) fn execution_root_v22(&self) -> Hash64 {
        self.execution_root
    }
}
pub(super) fn preflight_native_cohort_context_v22<'a>(
    parent: &'a PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    execution_root: Hash64,
) -> Result<Option<NativeCohortParentContextV22<'a>>, PalwStateV2Error> {
    if !params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(None);
    }
    if parent.last_point.is_some_and(|p| p.blue_score >= ctx.blue_score || p.daa_score > ctx.daa_score) {
        return Err(err(bad("native cohort preflight candidate does not follow parent")));
    }
    if !super::palw_claim_local_execution_cohort_census_v15::visit_birth_eligible_members_v15(parent, execution_root, *ctx, |_, _| {
        Ok(())
    })? {
        return Ok(None);
    }
    Ok(Some(NativeCohortParentContextV22 { parent, candidate: *ctx, execution_root }))
}
