//! Actual native original TIR source records; no world/frame projection or credit gate.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1 as finance_v17;
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1 as Escrow;
use crate::palw_native_resource_v1::{NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_tir_admission_v1::PalwTirClassRecordV1 as Class;
use crate::palw_tir_class_v1::PalwTirLayoutV1 as Layout;
use crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 as Frame;
use borsh::{BorshDeserialize, BorshSerialize};
const DIRECTORY: u32 = 1;
const BODIES: u32 = 2;
const BY_ALLOCATION: u32 = 3;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 16, table, schema: 10, key_bytes: 64, value_bytes: u64::MAX }
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
        _ => bad("original TIR source bundle journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("original TIR source bundle encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out).map_err(|_| bad("original TIR source bundle encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
        _ => bad("original TIR source bundle typed body codec"),
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
    let mut w = Writer(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/original-tir-source/v16").to_state());
    domain.serialize(&mut w).map_err(|_| bad("original TIR source bundle domain"))?;
    v.serialize(&mut w).map_err(|_| bad("original TIR source bundle identity"))?;
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
    #[cfg(test)]
    let marginal = if b.extras.original_archive_data_observer_v16 {
        Some(store::quote_component_publication_v14(&b.state, &plan, raw.as_deref().map(Vec::as_slice), scope)?)
    } else {
        None
    };
    #[cfg(test)]
    let marginal_first = b.entries.len();
    match scope {
        Some(id) => store::publish_owned_component_v2(b, id, &plan, raw.as_deref().map(Vec::as_slice))?,
        None => store::publish_component_v2(b, &plan, raw.as_deref().map(Vec::as_slice))?,
    };
    #[cfg(test)]
    if let Some(facts) = marginal {
        assert_publication_delta_v14(&b.entries[marginal_first..], &facts);
    }
    Ok(plan.after())
}
fn inventory(
    row: RowValueV2,
    extra: &[RowValueV2],
    component: Option<ComponentRootV2>,
) -> Result<NativeRetainedV1<store::ArchiveReferencesV2>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<RowValueV2>(1 + extra.len() as u64).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<ComponentRootV2>(u64::from(component.is_some())).map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut literals = Vec::with_capacity(1 + extra.len());
    literals.push(row);
    literals.extend_from_slice(extra);
    let mut components = Vec::with_capacity(usize::from(component.is_some()));
    if let Some(c) = component {
        components.push(c)
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(store::ArchiveReferencesV2 { version: 2, components, literals }, lease))
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn archive_id(row: RowValueV2, extra: &[RowValueV2], component: Option<ComponentRootV2>) -> Result<Hash64, ContextErrorV2> {
    let a = inventory(row, extra, component)?;
    let raw = encode(&*a)?;
    Ok(value(&raw).data)
}
fn retain_archive(
    b: &mut TransitionBuilder<'_>,
    row: RowValueV2,
    extra: &[RowValueV2],
    component: Option<ComponentRootV2>,
) -> Result<(), ContextErrorV2> {
    let a = inventory(row, extra, component)?;
    let raw = encode(&*a)?;
    b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &akey(value(&raw).data), Some(&*a)).map_err(native_err)
}
fn owned_key(scope: Hash64, table: u32) -> Result<[u8; 84], ContextErrorV2> {
    let mut k = [0; 84];
    k[0] = b'I';
    k[1..3].copy_from_slice(&2u16.to_le_bytes());
    k[3..67].copy_from_slice(scope.as_byte_slice());
    spec(table).serialize(&mut &mut k[67..]).map_err(|_| bad("original TIR owned root key"))?;
    Ok(k)
}
fn atomic<T>(
    b: &mut TransitionBuilder<'_>,
    f: impl FnOnce(&mut TransitionBuilder<'_>) -> Result<T, ContextErrorV2>,
) -> Result<T, ContextErrorV2> {
    let first = b.entries.len();
    let gc = super::palw_claim_local_context_gc_v3::checkpoint_v3(b);
    let answer = f(b);
    if answer.is_err() {
        super::palw_claim_local_context_gc_v3::rollback_aux_v3(b, first, gc.0, |table, key| {
            table == PALW_CLAIM_LOCAL_TABLE_V1 && matches!(key.first().copied(), Some(b'N' | b'B' | b'I' | b'A'))
        })?;
    }
    answer
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
struct Bundle {
    version: u16,
    source: Hash64,
    point: PalwBlockContextV2,
    contract: Hash64,
    scope: Hash64,
    bodies: ComponentRootV2,
    claim_entry: u64,
    escrow_entry: u64,
    completed: bool,
}
crate::palw_native_resource_v1::wire::wire_struct!(Bundle;u16,Hash64,PalwBlockContextV2,Hash64,Hash64,ComponentRootV2,u64,u64,bool);
fn bundle_key(id: Hash64, p: PalwBlockContextV2) -> Result<Hash64, ContextErrorV2> {
    hash(1, &(id, p))
}
fn body_key(tag: u64) -> Hash64 {
    Hash64::from_u64_word(tag)
}
fn save_raw(b: &mut TransitionBuilder<'_>, scope: Hash64, tag: u64, raw: &[u8]) -> Result<(), ContextErrorV2> {
    let reader = StateNodesV2(&b.state);
    let before = root(&b.state, BODIES, Some(scope))?;
    let key = body_key(tag);
    let old = nodes::lookup(&reader, before, key.as_byte_slice())?;
    let plan = nodes::update(&reader, before, key.as_byte_slice(), old, Some(value(raw)))?;
    #[cfg(test)]
    let marginal = if b.extras.original_archive_data_observer_v16 {
        Some(store::quote_component_publication_v14(&b.state, &plan, Some(raw), Some(scope))?)
    } else {
        None
    };
    #[cfg(test)]
    let marginal_first = b.entries.len();
    store::publish_owned_component_v2(b, scope, &plan, Some(raw))?;
    #[cfg(test)]
    if let Some(facts) = marginal {
        assert_publication_delta_v14(&b.entries[marginal_first..], &facts);
    }
    Ok(())
}
fn read_body<T: BorshDeserialize + NativeWireScanV1>(
    s: &PalwChainStateV2,
    bundle: &Bundle,
    tag: u64,
) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    let reader = StateNodesV2(s);
    let rv = nodes::lookup(&reader, bundle.bodies, body_key(tag).as_byte_slice())?.ok_or(bad("original TIR source body absent"))?;
    let raw = reader.body(rv)?;
    decode(&raw)
}
fn stored(s: &PalwChainStateV2, id: Hash64, p: PalwBlockContextV2) -> Result<Option<NativeRetainedV1<Bundle>>, ContextErrorV2> {
    let v = lookup::<Bundle>(s, DIRECTORY, None, bundle_key(id, p)?)?;
    if let Some(v) = &v
        && (v.version != 16
            || v.source != id
            || v.point != p
            || v.scope != hash(2, &(id, p, v.contract))?
            || v.bodies.spec != spec(BODIES)
            || v.bodies != root(s, BODIES, Some(v.scope))?)
    {
        return Err(bad("original TIR source bundle identity"));
    }
    Ok(v)
}
fn publish(b: &mut TransitionBuilder<'_>, row: &Bundle, old: Option<&Bundle>) -> Result<(), ContextErrorV2> {
    if let Some(old) = old {
        let raw = encode(old)?;
        let id = archive_id(value(&raw), &[], Some(old.bodies))?;
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(id), None).map_err(native_err)?;
    }
    let raw = encode(row)?;
    put(b, DIRECTORY, None, bundle_key(row.source, row.point)?, Some(row))?;
    retain_archive(b, value(&raw), &[], Some(row.bodies))
}
fn check_frame(frame: &Frame, c: &PalwClaimStateV2, id: Hash64) -> Result<(), ContextErrorV2> {
    frame.check_structure_v1(&Key::TirClaim(id)).map_err(|_| bad("original TIR source frame structure"))?;
    let job = frame.original_v1();
    if !matches!(c.source, PalwClaimSourceV2::FreePrompt { .. })
        || crate::palw_freeprompt_v3::fp_claim_id_v3(job) != id
        || job.job.class_id != c.class_id
        || job.job.executor_bond != c.bond.0
        || job.execution_root != c.execution_root
        || job.trace_root != c.trace_root
        || frame.header_v1().committed_execution_root != c.execution_root
        || frame.plan_v1().original_claim != id
    {
        return Err(bad("original TIR source changes its native born job"));
    }
    Ok(())
}
/// Called ONLY after successful coordinator final all-branch check. It is an
/// original-scope provenance producer, never an export retention capability.
/// The enclosing accepted object's existing checkpoint owns native+metadata rollback.
pub(super) fn capture_original_install_v16(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    contract: Hash64,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(());
    }
    atomic(b,|b|{
        if stored(&b.state,id,*ctx)?.is_some(){return Err(bad("original TIR install provenance repeated"))}
        let escrow_index=b.entries.iter().rposition(|e|matches!(e,PalwDeltaEntryV2::FundedEscrow{key,old:None,new:Some(r)}if *key==contract&&r.claim==id&&r.accepted_daa==ctx.daa_score))
            .ok_or(bad("original TIR provenance lacks actual fresh escrow effect"))?;
        let claim_index=b.entries[..escrow_index].iter().rposition(|e|matches!(e,PalwDeltaEntryV2::Claim{key,old:None,new:Some(c)}if *key==id&&c.accepted_daa==ctx.daa_score&&c.accepted_blue_score==ctx.blue_score))
            .ok_or(bad("original TIR provenance lacks actual fresh Claim effect"))?;
        let PalwDeltaEntryV2::Claim{new:Some(original),..}=&b.entries[claim_index]else{unreachable!()};
        let original_raw=encode(original)?;
        let original=decode::<PalwClaimStateV2>(&original_raw)?;
        let signed=b.entries[claim_index..=escrow_index].iter().find_map(|e|match e{
            PalwDeltaEntryV2::StagedCourtSeed{key:Key::TirClaim(c),old:None,new:Some(Row::TirClaim(f))}if *c==id=>Some(f.as_ref()),_=>None
        }).ok_or(bad("original TIR provenance lacks initial installed source Frame"))?;
        check_frame(signed,&original,id)?;
        let allocation=signed.plan_v1().allocation_id;
        let native_source=signed.plan_v1().source_id;
        let native_resource=signed.plan_v1().resource_plan_id;
        let signed_raw=encode(signed)?;
        let PalwDeltaEntryV2::FundedEscrow{new:Some(escrow),..}=&b.entries[escrow_index]else{unreachable!()};
        if escrow.contract_id!=contract||!escrow.liability_open||escrow.settled_by.is_some(){return Err(bad("original TIR provenance escrow differs"))}
        let pair=b.extras.tir_funded_originals.get(&id).ok_or(bad("original TIR actual paired carrier absent"))?;
        if pair.claim()!=id||pair.outpoint()!=escrow.funding_outpoint||pair.envelope().funding.terms.id_v1()!=contract{
            return Err(bad("original TIR provenance substitutes accepted funding carrier"));
        }
        let carrier=encode(pair.envelope())?;
        let carrier_raw=encode(&*carrier)?;
        let escrow_raw=encode(escrow)?;
        let service=b.state.court_service_state_v1().ok_or(bad("original TIR original service reservation absent"))?;
        let binding=service.claims.get(&id).ok_or(bad("original TIR source service binding absent"))?;
        if binding.claim!=id||binding.allocation_id!=allocation||binding.original_accepted_daa!=ctx.daa_score{
            return Err(bad("original TIR service binding differs from actual install"));
        }
        let binding_raw=encode(binding)?;
        let mut q=TirNativeRequestsV1::new();
        q.exact_v1::<&crate::palw_court_service_v1::ServicePacket>(binding.packet_ids.len() as u64).map_err(ContextErrorV2::Resource)?;
        let _packets=reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
        let mut packets=Vec::with_capacity(binding.packet_ids.len());
        for key in &binding.packet_ids{packets.push(service.book.packet(key).ok_or(bad("original TIR reserved source packet absent"))?)}
        let packets_raw=encode(&packets)?;drop(packets);
        let shared_raw=encode(b.state.staged_courts.get(&Key::CourtSharedContext(id)).ok_or(bad("original TIR shared source context absent"))?)?;
        let admitted_class=b.state.tir_class_v1(&original.class_id).ok_or(bad("original TIR admitted class absent"))?;
        let class_raw=encode(admitted_class)?;
        let layout_raw=match b.state.staged_courts.get(&Key::TirLayout(original.class_id)){Some(Row::TirLayout(layout))=>encode(layout.as_ref())?,_=>return Err(bad("original TIR admitted layout absent"))};
        let semantics=b.state.claim_semantics_v1(&id);
        let semantics_raw=encode(&semantics)?;
        let eof=semantics.and_then(|sem|b.state.staged_tokenizer_inventory_v1(sem.tokenizer_id));
        let eof_raw=encode(&eof)?;
        let rules=crate::palw_court_family_material_v1::FamilyOriginalRuleContextV2::from_native_v2(b.extras.tir_public_allocation_rules_v1.ok_or(bad("original TIR admitted native rules absent"))?);
        let rules_raw=encode(&rules)?;
        let cover=crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1().map_err(|_|bad("original TIR adopted cover schedule absent"))?.cover;
        let limits_raw=encode(&(cover.max_original_leaves,cover.max_total_edges,cover.max_total_coordinates,cover.max_root_memberships))?;
        let _table_key=reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
        let table_key=crate::palw_court_family_material_v1::material_aux_key_v1(4,id,None);
        let table=b.state.kernel_route().and_then(|r|r.aux.get(&(crate::palw_court_family_material_v1::FAMILY_MATERIAL_TABLE_V1,table_key)));
        let table=table.map(|raw|decode::<crate::palw_court_family_material_v1::FamilyTableInventoryRowV1>(raw)).transpose()?;
        if let Some(table)=&table&& (table.claim!=id||table.allocation_id!=allocation||table.carrier!=pair.outpoint()){return Err(bad("original TIR family table source provenance differs"))}
        if pair.semantic_table_v2().is_some()!=table.is_some(){return Err(bad("original TIR installed semantic table differs from actual signed carrier"))}
        let table_raw=encode(&table.as_deref())?;

        let scope=hash(2,&(id,*ctx,contract))?;
        let receipt=finance_v17::verified_original_archive_reservation_v1(&b.state,id,*ctx).map_err(native_err)?;
        if let Some(receipt)=receipt.as_ref()
            && (receipt.source_v1(),receipt.allocation_v1(),receipt.resource_plan_v1(),receipt.accepting_point_v1(),receipt.contract_v1())
                !=(native_source,allocation,native_resource,*ctx,contract) {
                return Err(bad("original archive receipt differs from actual initial native install"));
            }
        for (tag,raw) in [(1,&signed_raw),(3,&original_raw),(4,&class_raw),(5,&layout_raw),(6,&binding_raw),(7,&packets_raw),(8,&escrow_raw),(9,&shared_raw),(14,&carrier_raw),(15,&semantics_raw),(16,&eof_raw),(17,&table_raw),(18,&rules_raw),(19,&limits_raw)]{
            if let Some(receipt)=receipt.as_ref()&& raw.len() as u64>receipt.body_bounds_v1()[(tag-1) as usize] {return Err(bad("initial original archive body exceeds verified native reservation"));}
            save_raw(b,scope,tag,raw)?;
        }
        let row=Bundle{version:16,source:id,point:*ctx,contract,scope,bodies:root(&b.state,BODIES,Some(scope))?,claim_entry:claim_index as u64,escrow_entry:escrow_index as u64,completed:false};
        if lookup::<(Hash64,PalwBlockContextV2)>(&b.state,BY_ALLOCATION,None,allocation)?.is_some(){return Err(bad("original TIR allocation provenance already installed"))}
        put(b,BY_ALLOCATION,None,allocation,Some(&(id,*ctx)))?;
        publish(b,&row,None)
    }).map_err(err)
}
/// After actual successful object annotations and Source8 capture, before F1.
/// Reads only matching installed subjects. Does not scan the global source book.
pub(super) fn finalize_born_bundles_v16(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(());
    }
    let end = b.entries.len();
    for index in 0..end {
        let (contract, id) = match &b.entries[index] {
            PalwDeltaEntryV2::FundedEscrow { key, old: None, new: Some(r) } if r.accepted_daa == ctx.daa_score => (*key, r.claim),
            _ => continue,
        };
        let Some(old) = stored(&b.state, id, *ctx).map_err(err)? else {
            continue;
        };
        if old.contract != contract || old.completed {
            return Err(err(bad("original TIR completed install mismatch")));
        }
        let object = super::palw_claim_local_object_spans_v2::object_for_entry(b, objects, index)?
            .ok_or_else(|| err(bad("original TIR install lacks successful outer object")))?;
        if !matches!(object,PalwConsensusObjectV2::FreePromptCommitted{claim,..}if *claim==id) {
            return Err(err(bad("original TIR install belongs to another accepted object")));
        }
        let span = b
            .claim_local_object_spans
            .iter()
            .find(|span| span.first <= index && index < span.end)
            .ok_or_else(|| err(bad("original TIR completed span absent")))?;
        if old.escrow_entry != index as u64 || old.claim_entry < span.first as u64 || old.claim_entry >= index as u64 {
            return Err(err(bad("original TIR original Claim and escrow are not one successful object span")));
        }
        let control_raw = encode(object).map_err(err)?;
        let origin = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&b.state, id, *ctx)?
            .ok_or_else(|| err(bad("original TIR finalization lacks exact source admission")))?;
        let original = origin.original_claim();
        let stored_original = read_body::<PalwClaimStateV2>(&b.state, &old, 3).map_err(err)?;
        let original_raw = encode(original).map_err(err)?;
        let stored_original_raw = encode(&*stored_original).map_err(err)?;
        if original_raw.as_slice() != stored_original_raw.as_slice() {
            return Err(err(bad("original TIR completed source differs from install Claim")));
        }
        let frame = match b.state.staged_courts.get(&Key::TirClaim(id)) {
            Some(Row::TirClaim(f)) => f.as_ref(),
            _ => return Err(err(bad("original TIR postblock source Frame absent"))),
        };
        check_frame(frame, original, id).map_err(err)?;
        let born_raw = encode(frame).map_err(err)?;
        let class_raw = encode(b.state.tir_classes.get(&original.class_id).ok_or_else(|| err(bad("original TIR born class absent")))?)
            .map_err(err)?;
        let layout_raw = match b.state.staged_courts.get(&Key::TirLayout(original.class_id)) {
            Some(Row::TirLayout(layout)) => encode(layout.as_ref()).map_err(err)?,
            _ => return Err(err(bad("original TIR born layout absent"))),
        };
        let inputs_raw = encode(&origin.inputs()).map_err(err)?;
        let policy_raw = encode(origin.policy()).map_err(err)?;
        let params_raw = encode(&origin.original_params_bytes()).map_err(err)?;
        let receipt = finance_v17::verified_original_archive_reservation_v1(&b.state, id, *ctx)?;
        atomic(b, |b| {
            for (tag, raw) in [
                (2, &born_raw),
                (20, &class_raw),
                (21, &layout_raw),
                (10, &inputs_raw),
                (11, &policy_raw),
                (12, &params_raw),
                (13, &control_raw),
            ] {
                if let Some(receipt) = receipt.as_ref()
                    && raw.len() as u64 > receipt.body_bounds_v1()[(tag - 1) as usize]
                {
                    return Err(bad("completed original archive body exceeds verified native reservation"));
                }
                save_raw(b, old.scope, tag, raw)?;
            }
            let row = Bundle { completed: true, bodies: root(&b.state, BODIES, Some(old.scope))?, ..*old };
            publish(b, &row, Some(&old))
        })
        .map_err(err)?;
        if let Some(receipt) = finance_v17::verified_original_archive_reservation_v1(&b.state, id, *ctx)? {
            // MixedV2 marks only after native18 guard/write+actual envelope check.
            if receipt.profile_kind_v2() == crate::palw_court_original_escrow_v1::OriginalArchiveProfileKindV2::MaterialV1 {
                finance_v17::mark_original_archive_constructed_v1(b, ctx, id)?;
            }
        }
    }
    Ok(())
}
/// Opaque ORIGINAL-INSTALL provenance only. No paid-export standing, F1/fullroot,
/// selected-verifier credit or caller-created 'paid' flag can be obtained here.
pub(super) struct OriginalTirRetentionProvenanceV16 {
    bundle: NativeRetainedV1<Bundle>,
    original: NativeRetainedV1<PalwClaimStateV2>,
    admitted: NativeRetainedV1<Frame>,
    born: NativeRetainedV1<Frame>,
    class: NativeRetainedV1<Class>,
    layout: NativeRetainedV1<Layout>,
    born_class: NativeRetainedV1<Class>,
    born_layout: NativeRetainedV1<Layout>,
    inputs: NativeRetainedV1<Vec<crate::palw_claim_local_v1::PalwClaimLocalInputV1>>,
    policy: NativeRetainedV1<crate::palw_claim_local_v1::PalwClaimLocalPolicyContextV1>,
    params: NativeRetainedV1<Vec<u8>>,
    semantics: NativeRetainedV1<Option<crate::palw_class_semantics_v1::PalwClassSemanticsV1>>,
    eof: NativeRetainedV1<Option<crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>>,
    table: NativeRetainedV1<Option<crate::palw_court_family_material_v1::FamilyTableInventoryRowV1>>,
    rules: NativeRetainedV1<crate::palw_court_family_material_v1::FamilyOriginalRuleContextV2>,
    limits: NativeRetainedV1<(u64, u64, u64, u64)>,
}
impl OriginalTirRetentionProvenanceV16 {
    pub(super) fn source(&self) -> Hash64 {
        self.bundle.source
    }
    pub(super) fn accepting_point(&self) -> PalwBlockContextV2 {
        self.bundle.point
    }
    pub(super) fn original(&self) -> &PalwClaimStateV2 {
        &self.original
    }
    pub(super) fn admitted_signed_frame(&self) -> &Frame {
        &self.admitted
    }
    pub(super) fn born_postblock_frame(&self) -> &Frame {
        &self.born
    }
    pub(super) fn class(&self) -> &Class {
        &self.class
    }
    pub(super) fn layout(&self) -> &Layout {
        &self.layout
    }
}
pub(super) fn original_provenance_v16(
    s: &PalwChainStateV2,
    id: Hash64,
    p: PalwBlockContextV2,
) -> Result<Option<OriginalTirRetentionProvenanceV16>, PalwStateV2Error> {
    let Some(bundle) = stored(s, id, p).map_err(err)? else {
        return Ok(None);
    };
    if !bundle.completed {
        return Ok(None);
    }
    let original = read_body::<PalwClaimStateV2>(s, &bundle, 3).map_err(err)?;
    let admitted = read_body::<Frame>(s, &bundle, 1).map_err(err)?;
    let born = read_body::<Frame>(s, &bundle, 2).map_err(err)?;
    let class = read_body::<Class>(s, &bundle, 4).map_err(err)?;
    let layout = read_body::<Layout>(s, &bundle, 5).map_err(err)?;
    let born_class = read_body::<Class>(s, &bundle, 20).map_err(err)?;
    let born_layout = read_body::<Layout>(s, &bundle, 21).map_err(err)?;
    let inputs = read_body(s, &bundle, 10).map_err(err)?;
    let policy = read_body(s, &bundle, 11).map_err(err)?;
    let params = read_body(s, &bundle, 12).map_err(err)?;
    let semantics = read_body(s, &bundle, 15).map_err(err)?;
    let eof = read_body(s, &bundle, 16).map_err(err)?;
    let table = read_body(s, &bundle, 17).map_err(err)?;
    let rules = read_body(s, &bundle, 18).map_err(err)?;
    let limits = read_body(s, &bundle, 19).map_err(err)?;
    check_frame(&admitted, &original, id).map_err(err)?;
    check_frame(&born, &original, id).map_err(err)?;
    Ok(Some(OriginalTirRetentionProvenanceV16 {
        bundle,
        original,
        admitted,
        born,
        class,
        layout,
        born_class,
        born_layout,
        inputs,
        policy,
        params,
        semantics,
        eof,
        table,
        rules,
        limits,
    }))
}
// No FundedExportRetentionLease constructor belongs in this module. That
// authority must originate in the separate explicitly signed source-export plan.

/// Actual native TirClaim deletion only; caller passes its genuine OLD frame
/// after existing material/recovery guards. This is NOT financial/export closure.
/// A separately adopted funded export plan must pin its own source component.
fn collect_original_archive_rows_v19(b: &mut TransitionBuilder<'_>, old: &Frame) -> Result<(), PalwStateV2Error> {
    let allocation = old.plan_v1().allocation_id;
    let Some(binding) = lookup::<(Hash64, PalwBlockContextV2)>(&b.state, BY_ALLOCATION, None, allocation).map_err(err)? else {
        return Ok(());
    };
    let (id, p) = *binding;
    let bundle =
        stored(&b.state, id, p).map_err(err)?.ok_or_else(|| err(bad("original TIR deletion lost its exact allocation catalogue")))?;
    let admitted = read_body::<Frame>(&b.state, &bundle, 1).map_err(err)?;
    if old.key_v1() != Key::TirClaim(id) || admitted.plan_v1().allocation_id != allocation {
        return Err(err(bad("original TIR deletion changes physical source allocation")));
    }
    atomic(b, |b| {
        let raw = encode(&*bundle)?;
        let archive = archive_id(value(&raw), &[], Some(bundle.bodies))?;
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &akey(archive), None)
            .map_err(native_err)?;
        b.write_kernel_row_encoded_v1::<ComponentRootV2>(PALW_CLAIM_LOCAL_TABLE_V1, &owned_key(bundle.scope, BODIES)?, None)
            .map_err(native_err)?;
        put::<Bundle>(b, DIRECTORY, None, bundle_key(id, p)?, None)?;
        put::<(Hash64, PalwBlockContextV2)>(b, BY_ALLOCATION, None, allocation, None)?;

        Ok(())
    })
    .map_err(err)
}
/// Exact original-scope footprint for native requirement/profile review. No fee,
/// retention duration or full-export right is inferred from this inventory.
pub(super) fn visit_original_retention_inventory_v16(
    s: &PalwChainStateV2,
    id: Hash64,
    p: PalwBlockContextV2,
    mut visit: impl FnMut(ComponentRootV2, RowValueV2) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let Some(bundle) = stored(s, id, p).map_err(err)? else {
        return Ok(false);
    };
    let raw = encode(&*bundle).map_err(err)?;
    visit(bundle.bodies, value(&raw))?;
    Ok(true)
}
/// Full restore boundary: all local typed rows remain under native leases.
/// No global service book/world is reconstructed or cloned.
pub(super) fn validate_original_tir_bundles_v16(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    use crate::palw_claim_local_v1::{PalwClaimLocalInputV1 as Input, PalwClaimLocalPolicyContextV1 as Policy};
    use crate::palw_court_service_state_v2::ServiceClaimBinding;
    use crate::palw_court_service_v1::ServicePacket;
    let reader = StateNodesV2(s);
    let mut count = 0u64;
    nodes::complete_rows(&reader, root(s, DIRECTORY, None)?, |key, rv| {
        let body = reader.body(rv)?;
        let bundle = decode::<Bundle>(&body)?;
        let expected = bundle_key(bundle.source, bundle.point)?;
        if s.last_point.is_none_or(|tip| bundle.point.daa_score > tip.daa_score || bundle.point.blue_score > tip.blue_score) {
            return Err(bad("original TIR restore source point beyond actual tip"));
        }
        if key != expected.as_byte_slice()
            || !bundle.completed
            || bundle.bodies.rows != 21
            || bundle.claim_entry >= bundle.escrow_entry
        {
            return Err(bad("original TIR restore catalogue shape"));
        }
        let row = stored(s, bundle.source, bundle.point)?.ok_or(bad("original TIR restore lost catalogue"))?;
        let original = read_body::<PalwClaimStateV2>(s, &row, 3)?;
        if (original.accepted_block, original.accepted_daa, original.accepted_blue_score)
            != (row.point.block, row.point.daa_score, row.point.blue_score)
        {
            return Err(bad("original TIR restore changes actual acceptance tuple"));
        }
        let admitted = read_body::<Frame>(s, &row, 1)?;
        let born = read_body::<Frame>(s, &row, 2)?;
        check_frame(&admitted, &original, row.source)?;
        check_frame(&born, &original, row.source)?;
        if admitted.plan_v1() != born.plan_v1() {
            return Err(bad("original TIR restore changes immutable allocation plan"));
        }
        let _class = read_body::<Class>(s, &row, 4)?;
        let _layout = read_body::<Layout>(s, &row, 5)?;
        let _born_class = read_body::<Class>(s, &row, 20)?;
        let _born_layout = read_body::<Layout>(s, &row, 21)?;
        let semantics = read_body::<Option<crate::palw_class_semantics_v1::PalwClassSemanticsV1>>(s, &row, 15)?;
        if semantics.as_ref().is_some_and(|sem| sem.class_id != original.class_id || sem.bound_from_daa > row.point.daa_score) {
            return Err(bad("original TIR restore substitutes admitted semantics"));
        }
        let eof = read_body::<Option<crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>>(
            s, &row, 16,
        )?;
        if let Some(cert) = eof.as_ref() {
            cert.check_v1().map_err(|_| bad("original TIR restored EOF certificate shape"))?;
            if semantics.as_ref().is_none_or(|sem| cert.artifact_hash != sem.tokenizer_id) || cert.completed_daa > row.point.daa_score
            {
                return Err(bad("original TIR restored EOF differs from admitted tokenizer/time"));
            }
        }
        let table = read_body::<Option<crate::palw_court_family_material_v1::FamilyTableInventoryRowV1>>(s, &row, 17)?;
        let rules = read_body::<crate::palw_court_family_material_v1::FamilyOriginalRuleContextV2>(s, &row, 18)?;
        let _limits = read_body::<(u64, u64, u64, u64)>(s, &row, 19)?;
        if let Some(table) = table.as_ref() {
            if table.claim != row.source
                || table.allocation_id != admitted.plan_v1().allocation_id
                || table.allocation_rules_v2 != Some(*rules)
            {
                return Err(bad("original TIR restored table source/rules differ"));
            }
            let sem = semantics.as_ref().ok_or(bad("original TIR restored table lacks semantics"))?;
            crate::palw_court_family_material_v1::prepare_family_table_inventory_v1(&table.bytes, sem, _class.logits_vocab).map_err(
                |e| match e {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => ContextErrorV2::Resource(r),
                    _ => bad("original TIR restored canonical table authentication"),
                },
            )?;
        }

        let binding = read_body::<ServiceClaimBinding>(s, &row, 6)?;
        if binding.claim != row.source
            || binding.allocation_id != admitted.plan_v1().allocation_id
            || binding.source_id != admitted.plan_v1().source_id
            || binding.original_accepted_daa != row.point.daa_score
        {
            return Err(bad("original TIR restore substitutes original service binding"));
        }
        let packets = read_body::<Vec<ServicePacket>>(s, &row, 7)?;
        if packets.len() != binding.packet_ids.len() {
            return Err(bad("original TIR restore packet population differs"));
        }
        for (packet, key) in packets.iter().zip(&binding.packet_ids) {
            if packet.id != *key {
                return Err(bad("original TIR restore packet identity differs"));
            }
        }
        let escrow = read_body::<Escrow>(s, &row, 8)?;
        if escrow.claim != row.source
            || escrow.contract_id != row.contract
            || escrow.accepted_daa != row.point.daa_score
            || !escrow.liability_open
            || escrow.settled_by.is_some()
        {
            return Err(bad("original TIR restore changes actual installed escrow"));
        }
        if table.as_ref().is_some_and(|table| table.carrier != escrow.funding_outpoint) {
            return Err(bad("original TIR restored table changes actual funding carrier"));
        }
        let shared = read_body::<Row>(s, &row, 9)?;
        if !matches!(&*shared,Row::CourtSharedContext(c)if c.claim==row.source&&c.allocation_id==binding.allocation_id&&c.source_id==binding.source_id)
        {
            return Err(bad("original TIR restore shared context differs"));
        }
        let _inputs = read_body::<Vec<Input>>(s, &row, 10)?;
        let _policy = read_body::<Policy>(s, &row, 11)?;
        let _params = read_body::<Vec<u8>>(s, &row, 12)?;
        let control = read_body::<PalwConsensusObjectV2>(s, &row, 13)?;
        if !matches!(&*control,PalwConsensusObjectV2::FreePromptCommitted{claim,..}if *claim==row.source) {
            return Err(bad("original TIR restore changes actual accepted control"));
        }
        let _signed_funding_envelope = read_body::<Vec<u8>>(s, &row, 14)?;
        let reverse = lookup::<(Hash64, PalwBlockContextV2)>(s, BY_ALLOCATION, None, binding.allocation_id)?
            .ok_or(bad("original TIR restore allocation reverse absent"))?;
        if *reverse != (row.source, row.point) {
            return Err(bad("original TIR restore allocation reverse differs"));
        }
        let raw = encode(&*row)?;
        let id = archive_id(value(&raw), &[], Some(row.bodies))?;
        let references = inventory(value(&raw), &[], Some(row.bodies))?;
        let a_raw = encode(&*references)?;
        let _lookup = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
        let Some(actual) = s.kernel_route().and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, akey(id).to_vec()))) else {
            return Err(bad("original TIR restore source retention inventory absent"));
        };
        if actual.as_slice() != a_raw.as_slice() {
            return Err(bad("original TIR restore source retention inventory differs"));
        }
        count = count.checked_add(1).ok_or(bad("original TIR restore source count overflow"))?;
        Ok(())
    })?;

    // Every A2 pin of owner16 bodies must belong to the original Directory or
    // to a live paid child receipt with that exact ancestor inventory.
    if let Some(route) = s.kernel_route() {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'A') || !raw.starts_with(&[2, 0]) {
                continue;
            }
            let archive = decode::<store::ArchiveReferencesV2>(raw)?;
            if !archive.components.iter().any(|r| r.spec.owner == 16 && r.spec.schema == 10 && r.spec.table == BODIES) {
                continue;
            }
            if key.as_slice() != akey(value(raw).data) {
                return Err(bad("original TIR reserved A2 shape/key"));
            }
            if finance_v17::validate_ancestor_receipt_inventory_v19(s, &archive)? {
                continue;
            }
            if archive.components.len() != 1 || archive.components[0].spec != spec(BODIES) || archive.literals.len() != 1 {
                return Err(bad("original TIR reserved A2 shape/key"));
            }
            let manifest = archive.literals[0];
            let raw_manifest = reader.body(manifest)?;
            let bundle = decode::<Bundle>(&raw_manifest)?;
            if bundle.version != 16
                || !bundle.completed
                || bundle.bodies.rows != 21
                || bundle.claim_entry >= bundle.escrow_entry
                || bundle.bodies != archive.components[0]
                || bundle.scope != hash(2, &(bundle.source, bundle.point, bundle.contract))?
                || root(s, BODIES, Some(bundle.scope))? != bundle.bodies
            {
                return Err(bad("original TIR reserved A2 manifest/scope"));
            }
            let expected = bundle_key(bundle.source, bundle.point)?;
            if nodes::lookup(&reader, root(s, DIRECTORY, None)?, expected.as_byte_slice())? != Some(manifest) {
                return Err(bad("original TIR orphan A2 manifest"));
            }
            let canonical = inventory(manifest, &[], Some(bundle.bodies))?;
            crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&*canonical, raw)
                .map_err(|_| bad("original TIR reserved A2 canonical inventory"))?;
        }
    }

    let mut owned = 0u64;
    if let Some(route) = s.kernel_route() {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'I') {
                continue;
            }
            let rooted = decode::<ComponentRootV2>(raw)?;
            if rooted.spec.owner != 16 {
                continue;
            }
            if rooted.spec != spec(rooted.spec.table) {
                return Err(bad("original TIR substituted component spec"));
            }
            match rooted.spec.table {
                DIRECTORY | BY_ALLOCATION if key.len() == 18 => {}
                BODIES if key.len() == 84 && key[1..3] == [2, 0] => {
                    owned = owned.checked_add(1).ok_or(bad("original TIR owned scope count overflow"))?;
                }
                _ => return Err(bad("original TIR unallocated current component namespace")),
            }
        }
    }
    // Each directory row binds a distinct hash(source,point,contract) scope and
    // its exact owned current root. Equality excludes additional orphan scopes.
    if owned != count {
        return Err(bad("original TIR orphan owned source component"));
    }
    if root(s, BY_ALLOCATION, None)?.rows != count {
        return Err(bad("original TIR restore orphan allocation reverse"));
    }
    Ok(())
}

/// Place after existing staged-map/index reservations and guards, immediately
/// before its infallible native insert/remove. New metadata may consume an
/// admitted journal slot, so exact native append is readmitted atomically.
pub(super) fn before_native_staged_write_v16(
    b: &mut TransitionBuilder<'_>,
    key: Key,
    new: Option<&Row>,
) -> Result<(), PalwStateV2Error> {
    if new.is_some() || !matches!(key, Key::TirClaim(_)) {
        return Ok(());
    }
    let raw = match b.state.staged_courts.get(&key) {
        Some(Row::TirClaim(frame)) => {
            if lookup::<(Hash64, PalwBlockContextV2)>(&b.state, BY_ALLOCATION, None, frame.plan_v1().allocation_id)
                .map_err(err)?
                .is_none()
            {
                return Ok(());
            }
            encode(frame.as_ref()).map_err(err)?
        }
        _ => return Ok(()),
    };
    let old = decode::<Frame>(&raw).map_err(err)?;
    atomic(b, |b| {
        let first = b.entries.len();
        release_deleted_original_v16(b, &old).map_err(native_err)?;
        if b.entries.len() != first {
            b.reserve_mutation_sequence_v1(0, 1).map_err(native_err)?;
        }
        Ok(())
    })
    .map_err(err)
}

use crate::palw_claim_local_v1::prefix_v1::{
    PalwPrefixDependencyRefusalV1 as R, PalwPrefixExportIdentityV1, PalwSignedPrefixExportV1,
};
impl OriginalTirRetentionProvenanceV16 {
    fn claim_id(&self) -> Hash64 {
        self.source()
    }
    fn original_claim(&self) -> &PalwClaimStateV2 {
        self.original()
    }
    pub(super) fn inputs(&self) -> &[crate::palw_claim_local_v1::PalwClaimLocalInputV1] {
        &self.inputs
    }
    pub(super) fn rules(&self) -> crate::palw_court_family_material_v1::FamilyOriginalRuleContextV2 {
        *self.rules
    }
    pub(super) fn source_inventory_fingerprint_v16(
        &self,
        profile: Hash64,
        role: crate::palw_court_family_supplier_v1::FamilySupplierRoleV1,
    ) -> Result<Hash64, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        use crate::palw_court_family_material_v1 as family;
        let table =
            match self.table.as_ref().filter(|_| role == crate::palw_court_family_supplier_v1::FamilySupplierRoleV1::SemanticTable) {
                Some(table) => {
                    let semantics = self.semantics.as_ref().ok_or_else(|| {
                        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(
                            "retained family table has no actual born semantics".into(),
                        )
                    })?;
                    Some(family::prepare_family_table_inventory_v1(&table.bytes, semantics, self.class.logits_vocab)?)
                }
                None => None,
            };
        let inputs = family::FamilySourceFingerprintInputsV16 {
            record: &self.class,
            layout: &self.layout,
            semantics: self.semantics.as_ref(),
            eof: self.eof.as_ref(),
            cover_limits: *self.limits,
        };
        family::family_inventory_source_inputs_typed_v16(&self.admitted, profile, role, table.as_ref(), &inputs)
    }
    pub(super) fn prefix_export_v1(&self, ordinal: u32) -> Result<Option<(&PalwSignedPrefixExportV1, PalwPrefixExportIdentityV1)>, R> {
        let born = self.original_claim();
        let point = self.accepting_point();
        if !matches!(born.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. })
            || born.accepted_block != point.block
            || born.accepted_daa != point.daa_score
            || born.accepted_blue_score != point.blue_score
        {
            return Err(R::Identity);
        }
        let mut envelopes = self.inputs().iter().filter_map(|input| match input {
            crate::palw_claim_local_v1::PalwClaimLocalInputV1::PrefixDependency { claim, envelope } if *claim == self.claim_id() => {
                Some(envelope.as_ref())
            }
            _ => None,
        });
        let Some(envelope) = envelopes.next() else {
            return Ok(None);
        };
        if envelopes.next().is_some() {
            return Err(R::Identity);
        }
        envelope.check_v1()?;
        let Some(export) = envelope.exports.iter().find(|e| e.intent.ordinal == ordinal) else {
            return Ok(None);
        };
        let mut jobs = self.inputs().iter().filter_map(|input| match input {
            crate::palw_claim_local_v1::PalwClaimLocalInputV1::FreePromptNative { claim, public_job: Some(job), .. }
                if *claim == self.claim_id() =>
            {
                Some(job.as_ref())
            }
            _ => None,
        });
        let Some(job) = jobs.next() else {
            return Err(R::Identity);
        };
        if jobs.next().is_some()
            || crate::palw_freeprompt_v3::fp_claim_id_v3(job) != self.claim_id()
            || export.intent.base_commitment != self.claim_id()
            || export.intent.network != job.job.network_domain
            || export.intent.prefix.class_id != self.original_claim().class_id
        {
            return Err(R::Identity);
        }
        if export.intent.material_until > self.original_claim().trace_retention_daa
            || export.intent.liability_until < self.accepting_point().daa_score
        {
            return Err(R::Retention);
        }
        export.intent.verify_signature_v1(&job.job.executor_pubkey, &export.signature)?;
        let point = self.accepting_point();
        Ok(Some((
            export,
            PalwPrefixExportIdentityV1 {
                intent: export.intent.id_v1(),
                source_claim: self.claim_id(),
                accepted_block: point.block,
                accepted_daa: point.daa_score,
                accepted_blue_score: point.blue_score,
            },
        )))
    }
}

/// Authenticated source DATA under native leases, not an Open/paid/export permit.
/// A future production export adapter must additionally require A4's opaque
/// successful NativeSourcePlan retention lease and bind its exact manifest.
pub(super) struct BornTirExportDataV16<'a> {
    provenance: &'a OriginalTirRetentionProvenanceV16,
    target: crate::palw_tir_prefix_export_relation_v2::TirExportTargetV2,
    spec: crate::palw_tir_prefix_export_relation_v2::TirPrefixExportSpecV2,
    signed_export: &'a PalwSignedPrefixExportV1,
    source: &'a crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    space: &'a crate::palw_tir_step_v1::PalwTirStepSpaceV1,
}
impl BornTirExportDataV16<'_> {
    pub(super) fn signed_export(&self) -> &PalwSignedPrefixExportV1 {
        self.signed_export
    }
    pub(super) fn inputs(&self) -> &[crate::palw_claim_local_v1::PalwClaimLocalInputV1] {
        &self.provenance.inputs
    }
    pub(super) fn policy(&self) -> &crate::palw_claim_local_v1::PalwClaimLocalPolicyContextV1 {
        &self.provenance.policy
    }
    pub(super) fn original_params_bytes(&self) -> &[u8] {
        &self.provenance.params
    }
    pub(super) fn target(&self) -> crate::palw_tir_prefix_export_relation_v2::TirExportTargetV2 {
        self.target
    }
    pub(super) fn spec(&self) -> crate::palw_tir_prefix_export_relation_v2::TirPrefixExportSpecV2 {
        self.spec
    }
    pub(super) fn original(&self) -> &PalwClaimStateV2 {
        self.provenance.original()
    }
    pub(super) fn frame(&self) -> &Frame {
        self.provenance.born_postblock_frame()
    }
    pub(super) fn source(&self) -> &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1 {
        self.source
    }
    pub(super) fn space(&self) -> &crate::palw_tir_step_v1::PalwTirStepSpaceV1 {
        self.space
    }
}
/// Scoped original data only, never export/Open paid standing. `None` means
/// absent provenance, retaining the unchanged genuine legacy F1 fallback.
pub(super) fn with_born_tir_export_data_v16<T>(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    identity: &PalwPrefixExportIdentityV1,
    ordinal: u32,
    reservation: &crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportReservationV1,
    read: impl FnOnce(BornTirExportDataV16<'_>) -> Result<T, PalwStateV2Error>,
) -> Result<Option<T>, PalwStateV2Error> {
    let point = state.claim_local_prefix_source_point_v1(identity)?;
    if !params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(None);
    }
    if reservation.source_claim != identity.source_claim
        || reservation.point != point
        || state.prefix_export_reservation_v1(&point, identity.source_claim)?.as_ref() != Some(reservation)
    {
        return Err(err(bad("retained TIR export changes exact born reservation")));
    }
    let Some(origin) = original_provenance_v16(state, identity.source_claim, point)? else {
        return Ok(None);
    };
    if origin.original().bond != reservation.bond {
        return Err(err(bad("retained TIR export substitutes born producer")));
    }
    let Some((export, actual_identity)) =
        origin.prefix_export_v1(ordinal).map_err(|_| err(bad("retained TIR original export signature or retention differs")))?
    else {
        return Err(err(bad("retained TIR source lacks requested signed export")));
    };
    if actual_identity != *identity
        || export.intent.ordinal != ordinal
        || export.intent.liability_until > reservation.liability_until
        || export.intent.material_until > reservation.material_until
    {
        return Err(err(bad("retained TIR export scope changes original finite duty")));
    }
    let frame = origin.born_postblock_frame();
    let maximum = frame.descriptor_v1().maximum_leaves;
    let source = frame.source_identity_native_v2(&origin.born_class, &origin.born_layout, maximum).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        _ => err(bad("retained TIR born class/source/layout identity differs")),
    })?;
    let job = frame.original_v1();
    let prepared = crate::palw_tir_prefix_export_relation_v2::prepare_export_source_v2(
        job,
        &source,
        &origin.born_class,
        &origin.born_layout,
        &job.job.executor_pubkey,
        maximum,
    )
    .map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        _ => err(bad("retained TIR export source preparation differs")),
    })?;
    let spec = prepared.prepare_export_v2(export).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        _ => err(bad("retained TIR export native geometry differs")),
    })?;
    let target = crate::palw_tir_prefix_export_relation_v2::TirExportTargetV2 {
        version: 2,
        source: actual_identity,
        export_ordinal: ordinal,
        prefix: export.intent.prefix,
        source_execution_root: origin.original().execution_root,
        liability_until: export.intent.liability_until,
        material_until: export.intent.material_until,
    };
    read(BornTirExportDataV16 {
        provenance: &origin,
        target,
        spec,
        signed_export: export,
        source: &source,
        space: prepared.space_v2(),
    })
    .map(Some)
}

/// Native actual-admission rederivation over THIS one source component. Counts
/// duplicate content references conservatively, never copies a global tree/book.
/// These are physical byte units; A4 separately authenticates signed limits and
/// adopted prices. This value is not a funding/retention or admission token.
pub(super) struct ActualOriginalArchiveFootprintV16 {
    pub body_rows: u64,
    pub body_bytes_with_keys_upper: u64,
    pub local_node_rows: u64,
    pub local_node_bytes_with_keys: u64,
    pub manifest_inventory_current_bytes: u64,
    pub directory_path_bytes_upper: u64,
    pub durable_bytes_upper: u64,
}
pub(super) fn actual_original_archive_footprint_v16(
    s: &PalwChainStateV2,
    id: Hash64,
    p: PalwBlockContextV2,
) -> Result<Option<ActualOriginalArchiveFootprintV16>, PalwStateV2Error> {
    fn add(a: u64, b: u64) -> Result<u64, ContextErrorV2> {
        a.checked_add(b).ok_or(bad("actual original source archive cost sum overflow"))
    }
    let Some(row) = stored(s, id, p).map_err(err)? else {
        return Ok(None);
    };
    let reader = StateNodesV2(s);
    let mut body_bytes = 0u64;
    let mut node_bytes = 0u64;
    let mut node_rows = 0u64;
    nodes::visit_nodes(&reader, row.bodies, |_, node| {
        node_rows = add(node_rows, 1)?;
        node_bytes = add(node_bytes, add(65, borsh::object_length(node).map_err(|_| bad("actual source node length"))? as u64)?)?;
        if let nodes::ContextNodeV2::Leaf { value, .. } = node {
            let body = reader.body(*value)?;
            body_bytes = add(body_bytes, add(65 + 4, body.len() as u64)?)?;
        }
        Ok(())
    })
    .map_err(err)?;
    let manifest = encode(&*row).map_err(err)?;
    let references = inventory(value(&manifest), &[], Some(row.bodies)).map_err(err)?;
    let reference_raw = encode(&*references).map_err(err)?;
    let root_bytes = borsh::object_length(&row.bodies).map_err(|_| err(bad("actual source root length")))? as u64;
    // Two global I keys (18), owned I key84, exact original manifest B key65
    // + Vec prefix4, A key65, allocation->(source,actualPoint) body B key65+4.
    let reverse_bytes = borsh::object_length(&(id, p)).map_err(|_| err(bad("actual source allocation reverse length")))? as u64;
    let fixed = add(
        add(add(add(84, root_bytes).map_err(err)?, 2 * (18 + root_bytes)).map_err(err)?, 65 + 4 + manifest.len() as u64)
            .map_err(err)?,
        65 + reference_raw.len() as u64 + 65 + 4 + reverse_bytes,
    )
    .map_err(err)?;
    // Update-only path upper of the two shared exact directories. Their other
    // rows are neither traversed nor counted against this source payer.
    let paths = 2 * (2 * (64 * 9 + 1) + 1) * (65 + 210);
    let durable = add(add(add(body_bytes, node_bytes).map_err(err)?, fixed).map_err(err)?, paths).map_err(err)?;
    Ok(Some(ActualOriginalArchiveFootprintV16 {
        body_rows: row.bodies.rows,
        body_bytes_with_keys_upper: body_bytes,
        local_node_rows: node_rows,
        local_node_bytes_with_keys: node_bytes,
        manifest_inventory_current_bytes: fixed,
        directory_path_bytes_upper: paths,
        durable_bytes_upper: durable,
    }))
}

// Readonly exact archived-data quote accessors; no grant/fee authority.
impl OriginalTirRetentionProvenanceV16 {
    pub(super) fn archive_quote_complete_v12(&self) -> bool {
        self.bundle.completed && self.bundle.bodies.rows == 21
    }
    pub(super) fn archive_quote_bundle_bytes_v12(&self) -> Result<u64, PalwStateV2Error> {
        borsh::object_length(&*self.bundle).map(|n| n as u64).map_err(|_| err(bad("born quote bundle codec")))
    }
    pub(super) fn archive_quote_bundle_id_v12(&self) -> Result<Hash64, PalwStateV2Error> {
        hash(251, &*self.bundle).map_err(err)
    }
    pub(super) fn archive_quote_born_class_v12(&self) -> &Class {
        &self.born_class
    }
    pub(super) fn archive_quote_born_layout_v12(&self) -> &Layout {
        &self.born_layout
    }
    pub(super) fn archive_quote_semantics_v12(&self) -> Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1> {
        self.semantics.as_ref()
    }
}

// These are negative corrupt-store variants of a GENUINELY installed archive.
// They never construct admitted Claims, Frames, statuses or source authority.
#[cfg(test)]
pub(super) fn assert_actual_archive_reverse_closure_v12(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    id: Hash64,
    point: PalwBlockContextV2,
) {
    validate_original_tir_bundles_v16(state).unwrap();
    let bundle = stored(state, id, point).unwrap().unwrap();
    assert!(bundle.completed && bundle.bodies.rows == 21);
    let manifest_raw = encode(&*bundle).unwrap();
    let manifest = value(&manifest_raw);
    let references = inventory(manifest, &[], Some(bundle.bodies)).unwrap();
    let encoded = encode(&*references).unwrap();
    let key = akey(value(&encoded).data);
    let allocation = read_body::<Frame>(state, &bundle, 1).unwrap().plan_v1().allocation_id;
    {
        let mut broken = state.clone();
        assert!(broken.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())).is_some());
        assert!(validate_original_tir_bundles_v16(&broken).is_err(), "required native A2 is never optional");
    }
    {
        let mut broken = state.clone();
        let mut wrong = key;
        wrong[1] ^= 1;
        broken.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, wrong.to_vec()), encoded.to_vec());
        assert!(
            matches!(validate_original_tir_bundles_v16(&broken), Err(ContextErrorV2::Malformed("original TIR reserved A2 shape/key"))),
            "extra A2 under non-content key refused by owner invariant"
        );
    }
    {
        // Remove actual Directory/reverse owners; leave the actual A2 pin.
        // Assert the reverse check fires before later owned-count validation.
        let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
        put::<Bundle>(&mut b, DIRECTORY, None, bundle_key(id, point).unwrap(), None).unwrap();
        put::<(Hash64, PalwBlockContextV2)>(&mut b, BY_ALLOCATION, None, allocation, None).unwrap();
        assert!(
            matches!(validate_original_tir_bundles_v16(&b.state), Err(ContextErrorV2::Malformed("original TIR orphan A2 manifest"))),
            "reverse A2 detects missing exact Directory target BEFORE aggregate owned-count checks"
        );
        b.write_kernel_row_encoded_v1::<ComponentRootV2>(PALW_CLAIM_LOCAL_TABLE_V1, &owned_key(bundle.scope, BODIES).unwrap(), None)
            .unwrap();
        assert!(validate_original_tir_bundles_v16(&b.state).is_err(), "A2 remains refused when every forward owner is also removed");
    }
    {
        let mut broken = state.clone();
        let mut body = [0; 65];
        body[0] = b'B';
        body[1..].copy_from_slice(manifest.data.as_byte_slice());
        assert!(broken.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1, body.to_vec())).is_some());
        assert!(validate_original_tir_bundles_v16(&broken).is_err(), "removed actual manifest is unreadable, never absent/closed");
    }
    for same_root in [true, false] {
        let mut broken = state.clone();
        let mut different = *bundle;
        // A different manifest on the same physical tree still has no Directory
        // authorization. Empty alternate BODIES tree tests cross-root binding.
        different.claim_entry = different.claim_entry.checked_add(1).unwrap();
        assert!(
            different.claim_entry < different.escrow_entry,
            "genuine INSTALL leaves intervening native rows; modified scalar alone remains shape-valid"
        );
        if !same_root {
            different.bodies = ComponentRootV2 { spec: spec(BODIES), node: None, rows: 0 };
        }
        let raw = encode(&different).unwrap();
        let ref_value = value(&raw);
        let mut body = [0; 65];
        body[0] = b'B';
        body[1..].copy_from_slice(ref_value.data.as_byte_slice());
        // Physical B stores a Borsh Vec<u8>, not inner Bundle bytes.
        // Retain exact outer-prefix+payload encoding under its own lease so
        // StateNodes.body reaches the intended typed reverse-binding check.
        let (physical_body, _physical_body_owner) = encode(&raw.as_slice()).unwrap().into_parts_v1();
        let _physical_key_owner = reserve_native_requests_v1(65).unwrap();
        broken.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, body.to_vec()), physical_body);
        let extra = inventory(ref_value, &[], Some(different.bodies)).unwrap();
        let extra_raw = encode(&*extra).unwrap();
        broken
            .kernel_route
            .as_mut()
            .unwrap()
            .aux
            .insert((PALW_CLAIM_LOCAL_TABLE_V1, akey(value(&extra_raw).data).to_vec()), extra_raw.to_vec());
        if same_root {
            assert!(
                matches!(
                    validate_original_tir_bundles_v16(&broken),
                    Err(ContextErrorV2::Malformed("original TIR orphan A2 manifest"))
                ),
                "same physical root does not authorize a different exact Directory manifest"
            );
        } else {
            assert!(
                matches!(
                    validate_original_tir_bundles_v16(&broken),
                    Err(ContextErrorV2::Malformed("original TIR reserved A2 manifest/scope"))
                ),
                "different physical root does not borrow old native scope"
            );
        }
    }
}

/// Narrow selector for the native data adapter. A present unfinished or broken
/// bundle is an error, not permission to disguise it as an absent legacy source.
/// Only this one DIRECTORY path, owned root and canonical A2 are read.
pub(super) fn has_completed_original_scope_v13(
    state: &PalwChainStateV2,
    id: Hash64,
    point: PalwBlockContextV2,
) -> Result<bool, PalwStateV2Error> {
    let Some(bundle) = stored(state, id, point).map_err(err)? else {
        return Ok(false);
    };
    if !bundle.completed || bundle.bodies.rows != 21 {
        return Err(err(bad("scoped TIR source has unfinished native bundle")));
    }
    let manifest = encode(&*bundle).map_err(err)?;
    let references = inventory(value(&manifest), &[], Some(bundle.bodies)).map_err(err)?;
    let canonical = encode(&*references).map_err(err)?;
    let key = akey(value(&canonical).data);
    let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    let actual = state
        .kernel_route()
        .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())))
        .ok_or_else(|| err(bad("scoped TIR source lacks native retention inventory")))?;
    if actual.as_slice() != canonical.as_slice() {
        return Err(err(bad("scoped TIR source retention inventory differs")));
    }
    Ok(true)
}

include!("palw_claim_local_source16_marginal_v14.rs");

/// Exact completed body validation for the finance owner's mark transition.
/// Bounds are verification inputs, never an admission/payment constructor.
pub(super) fn verify_original_archive_body_bounds_v1(
    state: &PalwChainStateV2,
    id: Hash64,
    point: PalwBlockContextV2,
    bounds: &[u64; 21],
) -> Result<(), PalwStateV2Error> {
    let bundle = stored(state, id, point).map_err(err)?.ok_or_else(|| err(bad("funded original archive generation missing")))?;
    if !bundle.completed || bundle.bodies.rows != 21 {
        return Err(err(bad("funded original archive is not the completed 21-body generation")));
    }
    let reader = StateNodesV2(state);
    for tag in 1..=21u64 {
        let rv = nodes::lookup(&reader, bundle.bodies, body_key(tag).as_byte_slice())
            .map_err(err)?
            .ok_or_else(|| err(bad("funded original archive body missing")))?;
        if rv.bytes > bounds[(tag - 1) as usize] {
            return Err(err(bad("actual original archive body exceeds signed reserved capacity")));
        }
        reader.body(rv).map_err(err)?; // Exact committed bytes/identity, under native lease.
    }
    let original = read_body::<PalwClaimStateV2>(state, &bundle, 3).map_err(err)?;
    let admitted = read_body::<Frame>(state, &bundle, 1).map_err(err)?;
    let born = read_body::<Frame>(state, &bundle, 2).map_err(err)?;
    check_frame(&admitted, &original, id).map_err(err)?;
    check_frame(&born, &original, id).map_err(err)?;
    if admitted.plan_v1() != born.plan_v1() {
        return Err(err(bad("funded original archive changes native allocation")));
    }
    let source = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(state, id, point)?
        .ok_or_else(|| err(bad("funded original archive lacks exact native Source8 birth")))?;
    let inputs = read_body::<Vec<crate::palw_claim_local_v1::PalwClaimLocalInputV1>>(state, &bundle, 10).map_err(err)?;
    if source.original_claim() != &*original || source.inputs() != inputs.as_slice() {
        return Err(err(bad("funded original archive substitutes native source/inputs")));
    }
    Ok(())
}
/// Normal selector: both completed scoped data and its independently verified
/// persisted financial receipt are required. No observer/readiness flag grants it.
pub(super) fn has_verified_original_scope_v17(
    state: &PalwChainStateV2,
    id: Hash64,
    point: PalwBlockContextV2,
) -> Result<bool, PalwStateV2Error> {
    let tip = state.last_point.ok_or_else(|| err(bad("funded original archive lacks native tip")))?;
    if point.daa_score > tip.daa_score || point.blue_score > tip.blue_score {
        return Err(err(bad("funded original scope is after its native tip")));
    }
    has_verified_original_scope_at_v25(state, id, point, tip.daa_score)
}
/// The current full-fold owner supplies its sealed executing point, rather than
/// moving LastPoint early or borrowing the previous block's lease clock. This
/// entry only checks a NEW original accepted at that exact actual point.
pub(super) fn has_verified_new_original_scope_in_fold_v25(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
) -> Result<bool, PalwStateV2Error> {
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(err(bad("funded original scope lacks actual executing point")));
    }
    let Some(source) = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&b.state, id, *ctx)? else {
        return Ok(false);
    };
    if !matches!(source.original_claim().source, PalwClaimSourceV2::FreePrompt { .. }) {
        return Ok(false);
    }
    has_verified_original_scope_at_v25(&b.state, id, *ctx, ctx.daa_score)
}
fn has_verified_original_scope_at_v25(
    state: &PalwChainStateV2,
    id: Hash64,
    point: PalwBlockContextV2,
    now: u64,
) -> Result<bool, PalwStateV2Error> {
    let receipt = finance_v17::verified_original_archive_reservation_v1(state, id, point)?;
    if let Some(receipt) = receipt.as_ref().filter(|r| !r.released_v1()) {
        if !receipt.constructed_v1() {
            return Err(err(bad("funded original archive receipt is pending construction")));
        }
        let bundle = stored(state, id, point).map_err(err)?.ok_or_else(|| err(bad("funded original receipt lost scoped archive")))?;
        let frame = read_body::<Frame>(state, &bundle, 1).map_err(err)?;
        let plan = frame.plan_v1();
        if (
            receipt.source_v1(),
            receipt.allocation_v1(),
            receipt.resource_plan_v1(),
            receipt.accepting_point_v1(),
            receipt.contract_v1(),
        ) != (plan.source_id, plan.allocation_id, plan.resource_plan_id, point, bundle.contract)
        {
            return Err(err(bad("funded original receipt substitutes source/allocation/point/contract")));
        }
        verify_original_archive_body_bounds_v1(state, id, point, receipt.body_bounds_v1())?;
        return has_completed_original_scope_v13(state, id, point);
    }
    if matches!(
        finance_v17::archive_retention_disposition_v1(state, id, point, now)?,
        finance_v17::ArchiveRetentionDispositionV1::ConsumerOwned
    ) {
        native_original_archive_metadata_v19(state, id, point)?
            .ok_or_else(|| err(bad("paid transferred original lost native archive")))?;
        return has_completed_original_scope_v13(state, id, point);
    }
    if receipt.is_some() {
        return Err(err(bad("released original has no verified paid consumer lease")));
    }
    if let Some(bundle) = stored(state, id, point).map_err(err)?
        && source_bundle_selected_profile_v17(state, &bundle)?
    {
        return Err(err(bad("recognized funded original source lost its financial receipt or paid transfer")));
    }
    Ok(false)
}

/// Classify the already-authenticated stored funding literal without allocating
/// its nested envelope/key/signature. This negative guard grants no paid scope.
fn source_bundle_selected_profile_v17(state: &PalwChainStateV2, bundle: &Bundle) -> Result<bool, PalwStateV2Error> {
    let raw = read_body::<Vec<u8>>(state, bundle, 14).map_err(err)?;
    let mut wire = raw.as_slice();
    let version = u16::deserialize_reader(&mut wire).map_err(|_| err(bad("source stored funding version")))?;
    let length = u32::deserialize_reader(&mut wire).map_err(|_| err(bad("source stored funding envelope length")))? as usize;
    if version != 1 || length > crate::palw_tir_public_fp_envelope_v1::PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1 || length > wire.len()
    {
        return Err(err(bad("source stored funding envelope framing")));
    }
    wire = &wire[length..];
    // FundingTerms is entirely fixed-width; this deserialize allocates no Vec.
    let terms = crate::palw_court_original_escrow_v1::FundingTermsV1::deserialize_reader(&mut wire)
        .map_err(|_| err(bad("source stored funding terms")))?;
    if terms.claim != bundle.source || terms.id_v1() != bundle.contract {
        return Err(err(bad("source stored funding contract/claim differs")));
    }
    Ok(finance_v17::original_archive_terms_selected_v2(&terms))
}

/// Actual source metadata from its immutable native archive, independent of a
/// live original frame or escrow. Economic standing must be checked separately.
pub(super) struct NativeOriginalArchiveMetadataV19 {
    pub(super) allocation: Hash64,
    pub(super) scope: Hash64,
    pub(super) contract: Hash64,
    pub(super) source: Hash64,
    pub(super) resource: Hash64,
    pub(super) material_until: u64,
    pub(super) terms: crate::palw_court_original_escrow_v1::FundingTermsV1,
    pub(super) bounds: [u64; 21],
    pub(super) directory: Hash64,
    pub(super) inventory: Hash64,
}
pub(super) fn native_original_archive_metadata_v19(
    s: &PalwChainStateV2,
    id: Hash64,
    p: PalwBlockContextV2,
) -> Result<Option<NativeOriginalArchiveMetadataV19>, PalwStateV2Error> {
    let Some(bundle) = stored(s, id, p).map_err(err)? else { return Ok(None) };
    if !bundle.completed || bundle.bodies.rows != 21 {
        return Err(err(bad("ancestor original archive incomplete")));
    }
    let mut bounds = [0; 21];
    let reader = StateNodesV2(s);
    for tag in 1..=21u64 {
        bounds[(tag - 1) as usize] = nodes::lookup(&reader, bundle.bodies, body_key(tag).as_byte_slice())
            .map_err(err)?
            .ok_or_else(|| err(bad("ancestor original body missing")))?
            .bytes;
    }
    verify_original_archive_body_bounds_v1(s, id, p, &bounds)?;
    let frame = read_body::<Frame>(s, &bundle, 1).map_err(err)?;
    let plan = frame.plan_v1();
    let raw = read_body::<Vec<u8>>(s, &bundle, 14).map_err(err)?;
    let mut wire = raw.as_slice();
    let version = u16::deserialize_reader(&mut wire).map_err(|_| err(bad("ancestor funding version")))?;
    let n = u32::deserialize_reader(&mut wire).map_err(|_| err(bad("ancestor funding framing")))? as usize;
    if version != 1 || n > wire.len() || n > crate::palw_tir_public_fp_envelope_v1::PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1 {
        return Err(err(bad("ancestor funding framing")));
    }
    wire = &wire[n..];
    let terms = crate::palw_court_original_escrow_v1::FundingTermsV1::deserialize_reader(&mut wire)
        .map_err(|_| err(bad("ancestor original funding terms")))?;
    if terms.claim != id || terms.id_v1() != bundle.contract || terms.source_id != plan.source_id {
        return Err(err(bad("ancestor native funding scope differs")));
    }
    let raw = encode(&*bundle).map_err(err)?;
    Ok(Some(NativeOriginalArchiveMetadataV19 {
        allocation: plan.allocation_id,
        scope: bundle.scope,
        contract: bundle.contract,
        source: plan.source_id,
        resource: plan.resource_plan_id,
        material_until: terms.material_until_daa,
        terms,
        bounds,
        directory: bundle_key(id, p).map_err(err)?,
        inventory: archive_id(value(&raw), &[], Some(bundle.bodies)).map_err(err)?,
    }))
}
pub(super) fn native_original_archive_specs_v19() -> [ComponentSpecV2; 3] {
    [spec(DIRECTORY), spec(BODIES), spec(BY_ALLOCATION)]
}
/// Actual committed funding evidence references, independent of live escrow.
pub(super) fn original_funding_evidence_refs_v19(
    s: &PalwChainStateV2,
    id: Hash64,
    p: PalwBlockContextV2,
) -> Result<Option<(RowValueV2, RowValueV2)>, PalwStateV2Error> {
    let Some(bundle) = stored(s, id, p).map_err(err)? else { return Ok(None) };
    if !bundle.completed || bundle.bodies.rows != 21 {
        return Err(err(bad("original funding evidence generation incomplete")));
    }
    let reader = StateNodesV2(s);
    let get = |tag| {
        nodes::lookup(&reader, bundle.bodies, body_key(tag).as_byte_slice())
            .map_err(err)?
            .ok_or_else(|| err(bad("original funding evidence body absent")))
    };
    Ok(Some((get(8)?, get(14)?)))
}
pub(super) fn verify_original_funding_evidence_v19(
    s: &PalwChainStateV2,
    id: Hash64,
    p: PalwBlockContextV2,
    escrow_ref: RowValueV2,
    envelope_ref: RowValueV2,
) -> Result<NativeRetainedV1<Escrow>, PalwStateV2Error> {
    if original_funding_evidence_refs_v19(s, id, p)? != Some((escrow_ref, envelope_ref)) {
        return Err(err(bad("original funding evidence references changed")));
    }
    let bundle = stored(s, id, p).map_err(err)?.ok_or_else(|| err(bad("original funding source missing")))?;
    let row = read_body::<Escrow>(s, &bundle, 8).map_err(err)?;
    let literal = read_body::<Vec<u8>>(s, &bundle, 14).map_err(err)?;
    let envelope = decode::<crate::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1>(&literal).map_err(err)?;
    envelope.funding.verify_signature_v1(&envelope.payer_pubkey).map_err(|_| err(bad("archived original payer signature")))?;
    let terms = &envelope.funding.terms;
    let frame = read_body::<Frame>(s, &bundle, 1).map_err(err)?;
    let original = read_body::<PalwClaimStateV2>(s, &bundle, 3).map_err(err)?;
    check_frame(&frame, &original, id).map_err(err)?;
    if (original.accepted_block, original.accepted_daa, original.accepted_blue_score) != (p.block, p.daa_score, p.blue_score)
        || row.claim != id
        || row.contract_id != bundle.contract
        || row.accepted_daa != p.daa_score
        || !row.liability_open
        || row.settled_by.is_some()
        || terms.id_v1() != bundle.contract
        || terms.claim != id
        || terms.source_id != frame.plan_v1().source_id
        || terms.execution_root != frame.original_v1().execution_root
        || terms.job_identity != frame.custody_v1().pledge_v1().job_identity
        || terms.network_domain != frame.original_v1().job.network_domain
        || terms.funding_amount_sompi != row.amount_sompi
        || terms.funding_output_index != row.funding_outpoint.index
        || terms.payer_key_id != row.payer_key_id
        || terms.refund_payload != row.refund_payload
        || terms.material_until_daa < row.hard_deadline_daa
    {
        return Err(err(bad("archived original funding signature/native escrow scope differs")));
    }
    Ok(row)
}

/// Normal writer hook: current Frame removal is not itself permission to erase
/// Source8/16 birth evidence or to extend an old payer's financial horizon.
pub(super) fn release_deleted_original_v16(b: &mut TransitionBuilder<'_>, old: &Frame) -> Result<(), PalwStateV2Error> {
    let Some(binding) =
        lookup::<(Hash64, PalwBlockContextV2)>(&b.state, BY_ALLOCATION, None, old.plan_v1().allocation_id).map_err(err)?
    else {
        return Ok(());
    };
    let (id, point) = *binding;
    let meta = native_original_archive_metadata_v19(&b.state, id, point)?
        .ok_or_else(|| err(bad("native deleted original archive missing")))?;
    let own = finance_v17::verified_original_archive_reservation_v1(&b.state, id, point)?;
    if own.is_none() && !finance_v17::original_archive_terms_selected_v2(&meta.terms) && b.claim_local_lock_point_v6.is_none() {
        return collect_original_archive_rows_v19(b, old);
    }
    let now = b.claim_local_lock_point_v6.ok_or_else(|| err(bad("original archive cleanup lacks executing point")))?.daa_score;
    let transferred = matches!(
        finance_v17::archive_retention_disposition_v1(&b.state, id, point, now)?,
        finance_v17::ArchiveRetentionDispositionV1::ConsumerOwned
    );
    if own.is_none() && !finance_v17::original_archive_terms_selected_v2(&meta.terms) && !transferred {
        return collect_original_archive_rows_v19(b, old); // historical diagnostic cohort unchanged
    }
    joint_cleanup_native_archive_v19(b, id, point, Some(old), None)
}
/// Existing immutable original frame; no current-world/frame projection.
fn joint_cleanup_native_archive_v19(
    b: &mut TransitionBuilder<'_>,
    id: Hash64,
    point: PalwBlockContextV2,
    deleting: Option<&Frame>,
    ending: Option<&super::palw_original_archive_ancestors_v19::NativeAncestorArchiveDescriptorV19>,
) -> Result<(), PalwStateV2Error> {
    let Some(bundle) = stored(&b.state, id, point).map_err(err)? else { return Ok(()) };
    let meta =
        native_original_archive_metadata_v19(&b.state, id, point)?.ok_or_else(|| err(bad("joint archive native metadata absent")))?;
    let frame = read_body::<Frame>(&b.state, &bundle, 1).map_err(err)?;
    if let Some(old) = deleting
        && (old.plan_v1().allocation_id != meta.allocation
            || old.plan_v1().source_id != meta.source
            || old.plan_v1().resource_plan_id != meta.resource)
    {
        return Err(err(bad("native deletion witness changes original archive")));
    }
    if let Some(d) = ending {
        d.recheck_native_v19(&b.state)?;
        if (d.claim_v19(), d.point_v19()) != (id, point) {
            return Err(err(bad("ending lease changes native source scope")));
        }
    }
    let now = b.claim_local_lock_point_v6.ok_or_else(|| err(bad("joint archive cleanup lacks executing point")))?.daa_score;
    if now < meta.material_until {
        return Ok(());
    }
    let own = finance_v17::verified_original_archive_reservation_v1(&b.state, id, point)?;
    let transferred = matches!(
        finance_v17::archive_retention_disposition_v1(&b.state, id, point, now)?,
        finance_v17::ArchiveRetentionDispositionV1::ConsumerOwned
    );
    if own.is_none() && !transferred && ending.is_none() {
        return Err(err(bad("new original archive lost original or consumer financial ownership")));
    }
    if own.as_ref().is_some_and(|r| r.released_v1())
        && !transferred
        && ending.is_none()
        && !finance_v17::has_ancestor_archive_references_v19(&b.state, id, point)?
    {
        return Err(err(bad("released original lost its required consumer archive owner")));
    }
    if !finance_v17::can_release_original_archive_reservation_v19(b, &frame, now)? {
        return Ok(());
    }
    if transferred {
        return atomic(b, |b| finance_v17::release_original_archive_reservation_v1(b, &frame).map_err(native_err)).map_err(err);
    }
    if finance_v17::has_ancestor_archive_references_v19(&b.state, id, point)? {
        return Ok(());
    }
    if deleting.is_none() && crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state, id).is_some() {
        return Ok(());
    }
    if !super::palw_claim_local_prefix_reservation_v1::native_other_archive_material_closed_v19(&b.state, id, point, now)? {
        return Ok(());
    }
    atomic(b, |b| {
        // Source8/Facts eligibility is paid fixed ledger metadata, not an all21
        // material-serving promise. Preserve it for live and compact credit readers.
        collect_original_archive_rows_v19(b, &frame).map_err(native_err)?;
        finance_v17::release_original_archive_reservation_v1(b, &frame).map_err(native_err)?;
        Ok(())
    })
    .map_err(err)
}
/// Actual native closure/ownership callbacks, never a wall-clock or readiness
/// boolean. A pending native duty keeps its already-owned archive unchanged.
pub(super) fn on_native_archive_ownership_changed_v19(
    b: &mut TransitionBuilder<'_>,
    id: Hash64,
    point: PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let Some(meta) = native_original_archive_metadata_v19(&b.state, id, point)? else { return Ok(()) };
    let own = finance_v17::verified_original_archive_reservation_v1(&b.state, id, point)?;
    if own.is_none() && !finance_v17::original_archive_terms_selected_v2(&meta.terms) && b.claim_local_lock_point_v6.is_none() {
        return Ok(());
    }
    let now = b.claim_local_lock_point_v6.ok_or_else(|| err(bad("native ownership callback lacks executing point")))?.daa_score;
    let transferred = matches!(
        finance_v17::archive_retention_disposition_v1(&b.state, id, point, now)?,
        finance_v17::ArchiveRetentionDispositionV1::ConsumerOwned
    );
    if own.is_none() && !finance_v17::original_archive_terms_selected_v2(&meta.terms) && !transferred {
        return Ok(());
    }
    joint_cleanup_native_archive_v19(b, id, point, None, None)
}
/// Called from the real child's financial release after its exact index leaves
/// are removed, while its old A2 still pins every ancestor artifact.
pub(super) fn collect_ancestor_archive_after_lease_v19(
    b: &mut TransitionBuilder<'_>,
    d: &super::palw_original_archive_ancestors_v19::NativeAncestorArchiveDescriptorV19,
) -> Result<(), PalwStateV2Error> {
    d.recheck_native_v19(&b.state)?;
    if native_original_archive_metadata_v19(&b.state, d.claim_v19(), d.point_v19())?.is_none() {
        return Ok(());
    }
    joint_cleanup_native_archive_v19(b, d.claim_v19(), d.point_v19(), None, Some(d))
}
/// Exact committed body identity for native billing checks, not paid authority.
pub(super) fn original_archive_body_reference_v19(
    s: &PalwChainStateV2,
    id: Hash64,
    p: PalwBlockContextV2,
    tag: u64,
) -> Result<Option<RowValueV2>, PalwStateV2Error> {
    if !(1..=21).contains(&tag) {
        return Err(err(bad("original archive body tag outside schema")));
    }
    let Some(bundle) = stored(s, id, p).map_err(err)? else { return Ok(None) };
    if !bundle.completed || bundle.bodies.rows != 21 {
        return Err(err(bad("original archive body generation incomplete")));
    }
    let reader = StateNodesV2(s);
    let value = nodes::lookup(&reader, bundle.bodies, body_key(tag).as_byte_slice())
        .map_err(err)?
        .ok_or_else(|| err(bad("original archive body absent")))?;
    reader.body(value).map_err(err)?;
    Ok(Some(value))
}
