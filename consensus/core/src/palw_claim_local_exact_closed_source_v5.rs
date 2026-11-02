//! Isolated candidate. Birth-qualified closed Claim catalogue, separate from
//! A4 financial observations and A3 paid Kernel sources. It retains no world.
//! Factory input is the executing writer's actual old retirement journal row.
use super::palw_claim_local_compact_sources_v1::{
    self as legacy, PalwClosedClaimSourceV1 as Closed,
};
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{
    self as nodes, ComponentSpecV2, ContextErrorV2, RowValueV2,
};
use crate::palw_claim_local_v1::{PALW_CLAIM_LOCAL_TABLE_V1, PalwClaimLocalWorkRefV1 as Work};
use crate::palw_native_resource_v1::{
    NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
fn bad(s: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(s)
}
fn state_error(e: ContextErrorV2) -> PalwStateV2Error {
    super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn map(e: PalwStateV2Error) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(r) => ContextErrorV2::Resource(r),
        _ => bad("exact closed native writer refused"),
    }
}
fn component(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 {
        owner: 6,
        table,
        schema: 5,
        key_bytes: 64,
        value_bytes: u64::MAX,
    }
}
fn scope(id: Hash64, birth: PalwBlockContextV2) -> Result<Hash64, ContextErrorV2> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/closed-claim-source/birth/v5",
        &(Work::ChainClaim { claim: id }, birth),
    )
    .map_err(|_| bad("closed source scope codec"))
}
fn original_key(id: Hash64, c: &PalwClaimStateV2) -> Result<Hash64, ContextErrorV2> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/closed-claim-source/original-birth/v5",
        &(id, c.accepted_block, c.accepted_daa, c.accepted_blue_score),
    )
    .map_err(|_| bad("closed original birth key"))
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn decode<T: NativeWireScanV1 + BorshDeserialize>(
    raw: &[u8],
) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || {
        borsh::from_slice(raw)
    })
    .map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => {
            ContextErrorV2::Resource(r)
        }
        _ => bad("closed source typed body codec"),
    })
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("closed source length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64)
        .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut raw = Vec::with_capacity(n);
    v.serialize(&mut raw)
        .map_err(|_| bad("closed source encoding"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(raw, lease))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct ExactClosedSourceArchiveV5 {
    pub version: u16,
    pub scope: Hash64,
    pub source: Hash64,
    pub birth: PalwBlockContextV2,
    pub body: RowValueV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(ExactClosedSourceArchiveV5;u16,Hash64,Hash64,PalwBlockContextV2,RowValueV2);
fn archive(r: &Closed, value: RowValueV2) -> Result<ExactClosedSourceArchiveV5, ContextErrorV2> {
    Ok(ExactClosedSourceArchiveV5 {
        version: 5,
        scope: scope(r.claim, r.birth)?,
        source: r.claim,
        birth: r.birth,
        body: value,
    })
}
fn id(a: &ExactClosedSourceArchiveV5) -> Result<Hash64, ContextErrorV2> {
    let raw = encode(a)?;
    Ok(crate::palw_claim_local_v1::palw_liability_material_id_v1(
        &raw,
    ))
}
fn validate_body(
    r: &Closed,
    source: Hash64,
    birth: PalwBlockContextV2,
) -> Result<(), ContextErrorV2> {
    if r.version != 1
        || r.claim != source
        || r.birth != birth
        || r.original.accepted_block == ZERO_HASH64
        || r.birth.daa_score != r.original.accepted_daa
        || r.birth.blue_score != r.original.accepted_blue_score
        || r.material_until < r.original.trace_retention_daa
        || r.birth.blue_score > r.closed_at.blue_score
        || r.birth.daa_score > r.closed_at.daa_score
    {
        return Err(bad("exact closed source identity/birth/material"));
    }
    let terminal = match r.terminal_phase {
        PalwClaimPhaseV2::Final { final_daa } => final_daa,
        PalwClaimPhaseV2::Voided { voided_daa, .. } => voided_daa,
        _ => return Err(bad("exact closed source is not terminal")),
    };
    if terminal < birth.daa_score
        || terminal > r.closed_at.daa_score
        || r.permitted && !matches!(r.terminal_phase, PalwClaimPhaseV2::Final { .. })
    {
        return Err(bad("exact closed source terminal/outcome"));
    }
    Ok(())
}
/// New scoped catalogue first. Legacy S1 is accepted only when it names THIS
/// exact source birth. A current same-raw-ID row is never consulted as identity.
pub(super) fn read_exact_closed_source_v5(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    source: Hash64,
    birth: &PalwBlockContextV2,
) -> Result<Option<NativeRetainedV1<Closed>>, PalwStateV2Error> {
    if !params.claim_local_settlement_active_at(birth.daa_score) {
        return Ok(None);
    }
    if let Some(row)=read_native_exact_identity_v18(state,source,birth)?{return Ok(Some(row))}
    let old = legacy::read(state, source)?;
    Ok(old.filter(|r| r.birth == *birth))
}
/// Exact native A5 identity/inventory only, for bounded ledger capacity
/// verification. This does not grant credit, selected approval or paid use.
/// Existing credit readers retain their policy fence and approval checks.
pub(super) fn read_native_exact_identity_v18(state:&PalwChainStateV2,source:Hash64,birth:&PalwBlockContextV2)->Result<Option<NativeRetainedV1<Closed>>,PalwStateV2Error>{
    let scope = scope(source, *birth).map_err(state_error)?;
    let reader = StateNodesV2(state);
    let root = reader.current(component(1)).map_err(state_error)?;
    if let Some(value) = nodes::lookup(&reader, root, scope.as_byte_slice()).map_err(state_error)? {
        let raw = reader.body(value).map_err(state_error)?;
        let r = decode::<Closed>(&raw).map_err(state_error)?;
        validate_body(&r, source, *birth).map_err(state_error)?;
        let a = archive(&r, value).map_err(state_error)?;
        let k = akey(id(&a).map_err(state_error)?);
        let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
        let raw = state
            .kernel_route
            .as_ref()
            .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, k.to_vec())))
            .ok_or_else(|| {
                state_error(ContextErrorV2::Unavailable(
                    "exact closed A5 inventory absent",
                ))
            })?;
        crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&a, raw)
            .map_err(|_| state_error(bad("exact closed A5 inventory differs")))?;
        return Ok(Some(r));
    }
    Ok(None)
}
/// Only the existing consensus-derived closure factory may call this. A4
/// financial facts, absence from claims, and a producer DTO are not authority.
pub(super) fn publish_closed_source_v5(
    b: &mut TransitionBuilder<'_>,
    r: &Closed,
) -> Result<(), PalwStateV2Error> {
    super::palw_claim_local_carrier_lineage_v9::atomic_source_metadata_v9(b,|b|{
        publish_inner_v5(b,r,false)?;
        super::palw_claim_local_carrier_lineage_v9::note_actual_closed_source_v9(b,r)?;
        super::palw_claim_local_born_tir_source_v16::on_native_archive_ownership_changed_v19(b,r.claim,r.birth)
    })
}
fn publish_inner_v5(
    b: &mut TransitionBuilder<'_>,
    r: &Closed,
    deny: bool,
) -> Result<(), PalwStateV2Error> {
    validate_body(r, r.claim, r.birth).map_err(state_error)?;
    if !b.params.claim_local_settlement_active_at(r.birth.daa_score)
        || b.extras.native_fact_ruleset != Some(r.ruleset)
    {
        return Err(state_error(bad("exact closed source actual rules/fence")));
    }
    let raw = encode(r).map_err(state_error)?;
    let value = RowValueV2 {
        data: crate::palw_claim_local_v1::palw_liability_material_id_v1(&raw),
        bytes: raw.len() as u64,
    };
    let scoped = scope(r.claim, r.birth).map_err(state_error)?;
    let reader = StateNodesV2(&b.state);
    let root = reader.current(component(1)).map_err(state_error)?;
    let old = nodes::lookup(&reader, root, scoped.as_byte_slice()).map_err(state_error)?;
    if let Some(old) = old {
        if old == value {
            return Ok(());
        }
        if !deny || r.permitted {
            return Err(state_error(bad("closed source arbitrary replacement")));
        }
        let old_raw = reader.body(old).map_err(state_error)?;
        let prior = decode::<Closed>(&old_raw).map_err(state_error)?;
        let (mut prior, _lease) = prior.into_parts_v1();
        if !prior.permitted {
            return Err(state_error(bad(
                "closed denial rewrote already denied source",
            )));
        }
        prior.permitted = false;
        crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&prior, &raw)
            .map_err(|_| state_error(bad("closed denial changed original source fields")))?;
    }
    let original = original_key(r.claim, &r.original).map_err(state_error)?;
    let original_root = reader.current(component(2)).map_err(state_error)?;
    let original_prior =
        nodes::lookup(&reader, original_root, original.as_byte_slice()).map_err(state_error)?;
    if original_prior != old {
        return Err(state_error(bad(
            "closed source original/accepting directories disagree",
        )));
    }
    let plan = nodes::update(&reader, root, scoped.as_byte_slice(), old, Some(value))
        .map_err(state_error)?;
    store::publish_component_v2(b, &plan, Some(&raw)).map_err(state_error)?;
    let plan = nodes::update(
        &StateNodesV2(&b.state),
        original_root,
        original.as_byte_slice(),
        original_prior,
        Some(value),
    )
    .map_err(state_error)?;
    store::publish_component_v2(b, &plan, Some(&raw)).map_err(state_error)?;
    let a = archive(r, value).map_err(state_error)?;
    let next = id(&a).map_err(state_error)?;
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &akey(next), Some(&a))?;
    if let Some(old) = old {
        let previous = id(&archive(r, old).map_err(state_error)?).map_err(state_error)?;
        if previous != next {
            b.write_kernel_row_encoded_v1::<ExactClosedSourceArchiveV5>(
                PALW_CLAIM_LOCAL_TABLE_V1,
                &akey(previous),
                None,
            )?;
        }
    }
    Ok(())
}
/// The established exact-born E conviction can monotonically deny this source
/// while export liability is still open. No later raw-ID cohort is modified.
fn deny_exact_closed_source_inner_v5(
    b: &mut TransitionBuilder<'_>,
    e: &crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportReservationV1,
) -> Result<(), PalwStateV2Error> {
    if e.convicted_daa.is_none() {
        return Err(state_error(bad(
            "closed source denial lacks actual E conviction",
        )));
    }
    let Some(row) = read_exact_closed_source_v5(&b.state, b.params, e.source_claim, &e.point)?
    else {
        return Ok(());
    };
    let (mut row, _lease) = row.into_parts_v1();
    if row.original.bond != e.bond {
        return Err(state_error(bad("closed source denial changed producer")));
    }
    if row.permitted {
        row.permitted = false;
        publish_inner_v5(b, &row, true)?;
    }
    super::palw_claim_local_carrier_lineage_v9::note_actual_denied_source_v9(b,&row)?;
    // Literal S1 readers remain during migration. Synchronize ONLY the same
    // actual birth; a newer singleton must never receive the old denial.
    if let Some(compat) = legacy::read(&b.state, e.source_claim)? {
        if compat.birth == row.birth && compat.permitted {
            let (mut compat, _owner) = compat.into_parts_v1();
            compat.permitted = false;
            if compat != row {
                return Err(state_error(bad(
                    "same-birth S1 differs from exact denied source",
                )));
            }
            let mut key = [0u8; 65];
            key[0] = legacy::PALW_COMPACT_CLAIM_SOURCE_PREFIX_V1;
            key[1..].copy_from_slice(e.source_claim.as_byte_slice());
            b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&compat))?;
        }
    }
    Ok(())
}
pub(super) fn verify_archive_v5(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    a: &ExactClosedSourceArchiveV5,
) -> Result<(), ContextErrorV2> {
    if a.version != 5 || a.scope != scope(a.source, a.birth)? {
        return Err(bad("exact closed A5 identity"));
    }
    let reader = StateNodesV2(state);
    if nodes::lookup(
        &reader,
        reader.current(component(1))?,
        a.scope.as_byte_slice(),
    )? != Some(a.body)
    {
        return Err(bad("exact closed A5 current catalogue binding"));
    }
    let r = read_exact_closed_source_v5(state, params, a.source, &a.birth)
        .map_err(map)?
        .ok_or(ContextErrorV2::Unavailable("exact closed A5 source absent"))?;
    validate_body(&r, a.source, a.birth)?;
    if state.last_point.is_none_or(|p| {
        r.closed_at.blue_score > p.blue_score || r.closed_at.daa_score > p.daa_score
    }) || !params.claim_local_settlement_active_at(r.birth.daa_score)
    {
        return Err(bad("exact closed source future/unarmed context"));
    }
    let original = original_key(r.claim, &r.original)?;
    if nodes::lookup(
        &reader,
        reader.current(component(2))?,
        original.as_byte_slice(),
    )? != Some(a.body)
    {
        return Err(bad("exact closed original catalogue omitted"));
    }
    if r.permitted
        && super::palw_claim_local_rewards_v1::approval(
            params,
            r.claim,
            &r.original,
            Some(&r.approval),
        )
        .map_err(map)?
            != super::palw_claim_local_rewards_v1::RewardReadinessV1::Ready
    {
        return Err(bad(
            "exact closed permitted source lacks genuine selected approval",
        ));
    }
    Ok(())
}

/// Logical original-birth lookup for historical consumers whose Claim stores
/// a carrying block rather than the actual accepting block. This secondary
/// index was minted from the SAME actual origin, never from a raw-ID guess.
pub(super) fn read_original_birth_v5(
    state: &PalwChainStateV2,
    source: Hash64,
    original: &PalwClaimStateV2,
) -> Result<Option<NativeRetainedV1<Closed>>, PalwStateV2Error> {
    read_original_coordinates_v24(state,source,original.accepted_block,original.accepted_daa,original.accepted_blue_score)
}
/// Same strict native A5 original-birth directory, without fabricating a Claim.
pub(super) fn read_original_coordinates_v24(state:&PalwChainStateV2,source:Hash64,block:BlockHash,daa:u64,blue:u64)->Result<Option<NativeRetainedV1<Closed>>,PalwStateV2Error>{
    let k=crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/closed-claim-source/original-birth/v5",&(source,block,daa,blue)).map_err(|_|state_error(bad("closed original native coordinates")))?;
    let reader = StateNodesV2(state);
    let Some(value) = nodes::lookup(
        &reader,
        reader.current(component(2)).map_err(state_error)?,
        k.as_byte_slice(),
    )
    .map_err(state_error)?
    else {
        return Ok(None);
    };
    let raw = reader.body(value).map_err(state_error)?;
    let r = decode::<Closed>(&raw).map_err(state_error)?;
    if r.claim != source || original_key(source, &r.original).map_err(state_error)? != k {
        return Err(state_error(bad(
            "closed source original-birth substitution",
        )));
    }
    validate_body(&r, source, r.birth).map_err(state_error)?;
    let actual = scope(source, r.birth).map_err(state_error)?;
    if nodes::lookup(
        &reader,
        reader.current(component(1)).map_err(state_error)?,
        actual.as_byte_slice(),
    )
    .map_err(state_error)?
        != Some(value)
    {
        return Err(state_error(bad(
            "closed original/accepting birth directories disagree",
        )));
    }
    let a = archive(&r, value).map_err(state_error)?;
    let key = akey(id(&a).map_err(state_error)?);
    let _lease = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    let native = state
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())))
        .ok_or_else(|| state_error(ContextErrorV2::Unavailable("closed original A5 missing")))?;
    crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&a, native)
        .map_err(|_| state_error(bad("closed original A5 inventory differs")))?;
    Ok(Some(r))
}

pub(super) fn validate_indices_v5(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
) -> Result<(), ContextErrorV2> {
    // Every owned namespace root must use its exact allocated native profile,
    // including otherwise orphan roots not reached by an expected directory.
    if let Some(route) = &state.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1
                || key.first() != Some(&store::CONTEXT_CURRENT_PREFIX_V2)
            {
                continue;
            }
            let rooted = decode::<nodes::ComponentRootV2>(raw)?;
            if rooted.spec.owner == 6
                && (key.len() != 18
                    || !matches!(rooted.spec.table, 1..=2)
                    || rooted.spec != component(rooted.spec.table))
            {
                return Err(bad("source index root has unallocated native profile"));
            }
        }
    }
    let reader = StateNodesV2(state);
    for table in [1, 2] {
        nodes::complete_rows(&reader, reader.current(component(table))?, |key, value| {
            let raw = reader.body(value)?;
            let r = decode::<Closed>(&raw)?;
            let expected = if table == 1 {
                scope(r.claim, r.birth)?
            } else {
                original_key(r.claim, &r.original)?
            };
            if key != expected.as_byte_slice() {
                return Err(bad("closed source current index key/birth"));
            }
            verify_archive_v5(state, params, &archive(&r, value)?)
        })?;
    }
    Ok(())
}

/// Closed-source native ruleset uses full params ID, unlike A3/A4 identity ID.
pub(super) fn validate_network_context_v5(
    state: &PalwChainStateV2,
    global: &crate::config::params::Params,
) -> Result<(), PalwStateV2Error> {
    let reader = StateNodesV2(state);
    nodes::complete_rows(
        &reader,
        reader.current(component(1)).map_err(state_error)?,
        |_, value| {
            let raw = reader.body(value)?;
            let r = decode::<Closed>(&raw)?;
            let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) =
                &global.palw_consensus_mode
            else {
                return Err(bad("exact closed source requires configured V2 network"));
            };
            if r.ruleset != global.consensus_params_id() {
                return Err(bad(
                    "exact closed source differs from configured native ruleset",
                ));
            }
            verify_archive_v5(state, &bundle.state, &archive(&r, value)?)
        },
    )
    .map_err(state_error)
}

/// Migrate a validated literal old S1 BEFORE any new cohort can replace row167,
/// including when its original F1 frame has already been legitimately collected.
pub(super) fn migrate_legacy_before_reuse_v5(b:&mut TransitionBuilder<'_>,source:Hash64)->Result<(),PalwStateV2Error>{
    if b.claim_local_lock_point_v6.is_none(){return Ok(())}
    let Some(old)=legacy::read(&b.state,source)? else{return Ok(())};
    if let Some(exact)=read_original_birth_v5(&b.state,source,&old.original)?{
        if *exact!=*old{return Err(state_error(bad("legacy source contradicts existing exact birth body")))}
        return Ok(())
    }
    let first=b.entries.len();let gc_point=b.claim_local_gc_sequence_v3;
    let result=(||{
        publish_closed_source_v5(b,&old)?;
        b.reserve_mutation_sequence_v1(0,2)?;
        Ok(())
    })();
    if result.is_err(){super::palw_claim_local_context_gc_v3::rollback_aux_v3(b,first,gc_point,|table,key|table==PALW_CLAIM_LOCAL_TABLE_V1&&matches!(key.first().copied(),Some(b'N'|b'B'|b'I'|b'A'))).map_err(state_error)?;}

    result
}

pub(super) fn deny_exact_closed_source_v5(b:&mut TransitionBuilder<'_>,e:&crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportReservationV1)->Result<(),PalwStateV2Error>{super::palw_claim_local_carrier_lineage_v9::atomic_source_metadata_v9(b,|b|deny_exact_closed_source_inner_v5(b,e))}

/// Metadata-only read-plan coverage. Not body validity, terminal status or a
/// paid/approval capability; semantic caller must read the actual Closed body.
pub(super) fn original_birth_present_v15(state:&PalwChainStateV2,source:Hash64,original:&PalwClaimStateV2)->Result<bool,PalwStateV2Error>{
 let key=original_key(source,original).map_err(state_error)?;let reader=StateNodesV2(state);Ok(nodes::lookup(&reader,reader.current(component(2)).map_err(state_error)?,key.as_byte_slice()).map_err(state_error)?.is_some())
}

/// Native immutable A5 literal at exact birth; no financial standing.
pub(super) fn native_closed_literal_v19(s:&PalwChainStateV2,source:Hash64,birth:PalwBlockContextV2)->Result<Option<RowValueV2>,PalwStateV2Error>{
 let reader=StateNodesV2(s);let key=scope(source,birth).map_err(state_error)?;
 let Some(value)=nodes::lookup(&reader,reader.current(component(1)).map_err(state_error)?,key.as_byte_slice()).map_err(state_error)?else{return Ok(None)};
 let raw=reader.body(value).map_err(state_error)?;let row=decode::<Closed>(&raw).map_err(state_error)?;validate_body(&row,source,birth).map_err(state_error)?;Ok(Some(value))
}
