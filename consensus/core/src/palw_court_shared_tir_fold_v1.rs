//! Internal rooted shared-data candidate. The lifecycle dispatcher is `palw_court_service_lifecycle_fold_v1::apply_shared_court_op_v1`
//! (no wire object reaches it yet) and no production original coordinator installs the context yet.
use super::*;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key,PalwStagedCourtRowV1 as Row};
use crate::palw_court_shared_tir_state_v1 as shared;
use crate::palw_court_shared_tir_frontier_v1::{SignedSharedRequestV1,SharedRequestEffectV1};
fn refused(s:impl Into<String>)->PalwStateV2Error {PalwStateV2Error::CarriageInconsistent(s.into())}
#[derive(Debug,PartialEq,Eq)]
pub(super) enum NumericalTransportEffectV1 {
    Root(bool), Round(bool), Bottom(bool,crate::palw_court_shared_tir_bottom_v1::SharedTirBottomOutcomeV1),
}
/// Complete bundle adapter only. Incomplete prefix storage, block ingress and
/// paid reservation remain separate; no wire checked flag bypasses typed folds.
pub(super) fn numerical_transport(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,
    manifest:&crate::palw_court_numerical_transport_v1::NumericalManifestV1,
    parts:&[crate::palw_court_numerical_transport_v1::NumericalPartV1])->Result<NumericalTransportEffectV1,PalwStateV2Error> {
    use crate::palw_court_numerical_transport_v1::{decode_numerical_v1,DecodedNumericalV1 as D,TransportErrorV1 as E};
    let rules=b.extras.native_court_rules_v1.ok_or_else(||refused("numerical transport native rules missing"))?;
    if rules.at!=*ctx || b.extras.tir_public_network_domain_v1!=Some(rules.network_domain) {
        return Err(refused("numerical transport native point/network mismatch"));
    }
    let c=shared::context(&b.state,manifest.claim).map_err(refused)?;clock(c,ctx)?;
    let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state,manifest.claim)
        .ok_or_else(||refused("numerical transport original missing"))?;
    if manifest.leaf>=frame.header_v1().step_leaf_count {return Err(refused("numerical transport leaf outside original"));}
    let control=decode_numerical_v1(manifest,parts,c.scope_id,&rules).map_err(|e|match e {
        E::Resource(e)=>PalwStateV2Error::NativeResource(e),E::Invalid(e)=>refused(e)})?;
    match control {
        D::Root(c)=>root(b,ctx,&c).map(NumericalTransportEffectV1::Root),
        D::Round(c)=>round(b,ctx,&c).map(NumericalTransportEffectV1::Round),
        D::Bottom(c)=>bottom(b,ctx,&c).map(|(opened,outcome)|NumericalTransportEffectV1::Bottom(opened,outcome)),
    }
}
fn clock(c:&shared::SharedTirContextV1,ctx:&PalwBlockContextV2)->Result<(),PalwStateV2Error> {
    if ctx.daa_score<c.original_accepted_daa || ctx.daa_score>=c.hard_daa {
        return Err(refused("shared data operation outside original hard interval"));
    }
    Ok(())
}
pub(super) fn request(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,claim:Hash64,
    intent:&SignedSharedRequestV1)->Result<SharedRequestEffectV1,PalwStateV2Error> {
    let saved=b.try_checkpoint()?;let result=request_inner(b,ctx,claim,intent);
    if result.is_err() {b.restore(saved);}result
}
fn request_inner(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,claim:Hash64,
    intent:&SignedSharedRequestV1)->Result<SharedRequestEffectV1,PalwStateV2Error> {
    let c=shared::context(&b.state,claim).map_err(refused)?;clock(c,ctx)?;
    let source=c.source(&b.state).map_err(refused)?;
    let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state,claim).ok_or_else(||refused("shared original missing"))?;
    let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,c.profile_id).map_err(refused)?;
    if intent.scope!=c.scope_id || intent.request_id!=set.request_id(intent.ordinal).map_err(refused)? {
        return Err(refused("shared intent substitutes native scope/coordinate"));
    }
    let network=b.extras.tir_public_network_domain_v1.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    intent.validate_bond_signature(network,&b.state).map_err(refused)?;
    let bond=b.state.bond(&intent.signer).ok_or(PalwStateV2Error::MissingBond(intent.signer))?;
    if !matches!(bond.status,PalwBondStatusV2::Active) || bond.collateral<b.params.min_collateral_sompi() {
        return Err(refused("shared requester is not active/above existing bond floor"));
    }
    let key=Key::CourtSharedQuery(intent.request_id);
    if let Some(Row::CourtSharedQuery(q))=b.state.staged_court_row_v1(&key) {
        q.check_context(&b.state).map_err(refused)?;
        if ctx.daa_score<q.opened_daa {return Err(refused("shared replay precedes its accepted opening"));}
        return Ok(SharedRequestEffectV1::Coalesced);
    }
    // G14 lane court-f02 (RFC-0031 / ADR-0198 A): past the shared lifecycle fences a NEW request row is a native notice — the
    // claim's first is its initial notice (a <= d <= a+3,120), later ones are follow-ups bound to it (before hard). The original
    // must already be accepted at `a` earlier in this fold (same-DAA causal order is the block's object order). Signature and
    // dedupe (Coalesced) are checked above; the row is journalled, so reorg removes/restores it with the rooted delta.
    if super::palw_court_service_lifecycle_fold_v1::shared_court_lifecycle_active_v1(b,ctx.daa_score) {
        let prior=b.state.court_shared_initial_notice_daa_v1(claim);
        crate::palw_court_service_binding_v1::classify_shared_notice_v1(c.original_accepted_daa,ctx.daa_score,prior).map_err(refused)?;
        if !b.state.claim(&claim).is_some_and(|k|k.accepted_daa==c.original_accepted_daa) {
            return Err(refused("shared notice precedes or substitutes its original's acceptance"));
        }
    }
    // G14 lane F03b2 (ADR-0192 A/A addendum, PROVISIONAL): charge ⌈max bytes / 34,000⌉ packets of this coordinate against the
    // capacity reserved at acceptance and record its deterministic service slot. All shared deadlines are the original's hard
    // DAA, so earliest-deadline-first is arrival order (in-block: object order); the slot starts after the claim's last slot.
    // Size never refuses; only a slot that cannot end before hard (no reserved capacity left) is refused, with no state change.
    let packets=shared::shared_coordinate_packets_v1(&source,&set.request_at(intent.ordinal).map_err(refused)?).map_err(refused)?;
    let cursor=shared_service_cursor_v1(&b.state,claim);
    let start=ctx.daa_score.max(cursor.map_or(c.original_accepted_daa,|end|end.saturating_add(1)));
    let per=crate::palw_g14_values_v1::PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1;
    let end=start.checked_add(packets.div_ceil(per)-1).ok_or_else(||refused("shared service slot overflow"))?;
    if end>=c.hard_daa {return Err(refused("shared request has no reserved service capacity before the original's hard DAA"));}
    let row=shared::SharedTirQueryV1 {version:shared::SHARED_QUERY_SERVICE_VERSION_V2,claim,scope_id:c.scope_id,ordinal:intent.ordinal,request_id:intent.request_id,
        opened_block:ctx.block,opened_daa:ctx.daa_score,opened_by:intent.signer,intent_digest:intent.signing_message(network).map_err(refused)?,
        material_until_daa:c.material_until_daa,body:None,service:Some(shared::SharedServiceSlotV1 {packets,start_daa:start,end_daa:end})};
    b.write_staged_row_v1(key,Some(Row::CourtSharedQuery(Box::new(row))))?;
    Ok(SharedRequestEffectV1::Opened)
}
/// F03b2: the last recorded service DAA among the claim's shared requests (per-claim index, no global scan).
pub(super) fn shared_service_cursor_v1(state:&PalwChainStateV2,claim:Hash64)->Option<u64> {
    state.staged_court_service_by_claim.get(&claim)?.iter()
        .filter(|(kind,_,_)|*kind==crate::palw_staged_court_state_v1::COURT_INDEX_SHARED_NOTICE_V1)
        .filter_map(|(_,_,key)|match state.staged_court_row_v1(key) {
            Some(Row::CourtSharedQuery(q))=>q.service.map(|s|s.end_daa),_=>None})
        .max()
}
/// Data identity only, never a signed response/default/credit effect.
pub(super) fn share_body(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,claim:Hash64,
    ordinal:u64,bytes:&[u8])->Result<bool,PalwStateV2Error> {
    let saved=b.try_checkpoint()?;let result=share_body_inner(b,ctx,claim,ordinal,bytes);
    if result.is_err() {b.restore(saved);}result
}
fn share_body_inner(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,claim:Hash64,
    ordinal:u64,bytes:&[u8])->Result<bool,PalwStateV2Error> {
    let c=shared::context(&b.state,claim).map_err(refused)?;clock(c,ctx)?;
    let source=c.source(&b.state).map_err(refused)?;
    let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state,claim).ok_or_else(||refused("shared original missing"))?;
    let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,c.profile_id).map_err(refused)?;
    let query=set.request_at(ordinal).map_err(refused)?;let key=Key::CourtSharedQuery(set.request_id(ordinal).map_err(refused)?);
    let Some(Row::CourtSharedQuery(old))=b.state.staged_court_row_v1(&key) else {return Err(refused("shared data has no native request"));};
    shared::authenticate_body(&b.state,c,&source,query,bytes).map_err(refused)?;
    if old.body.is_some() {return Ok(false);}
    // Typed new byte-vector owner is reserved before its allocation. The native
    // map writer separately reserves row/index/journal ownership before cloning.
    let mut q=crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<u8>(bytes.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _raw=crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut row=(**old).clone();row.body=Some(shared::SharedTirBodyV1 {block:ctx.block,accepted_daa:ctx.daa_score,
        digest:shared::shared_body_digest_v1(bytes),bytes:bytes.to_vec()});
    b.write_staged_row_v1(key,Some(Row::CourtSharedQuery(Box::new(row))))?;Ok(true)
}
fn shared_ids_auth_fault_v2(why: crate::palw_native_resource_v1::NativeDecodeErrorV1<String>) -> PalwStateV2Error {
    match why {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(why) => PalwStateV2Error::NativeResource(why),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(why) => refused(why),
    }
}
pub(super) fn share_ids(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
    rows_root: Hash64,
    ids: &[u32],
) -> Result<bool, PalwStateV2Error> {
    let saved = b.try_checkpoint()?;
    let result = (|| {
        let c = shared::context(&b.state, claim).map_err(refused)?;
        clock(c, ctx)?;
        let source = c.source(&b.state).map_err(refused)?;
        source.validate_complete_ids_classified_v2(rows_root, ids).map_err(shared_ids_auth_fault_v2)?;
        let key = Key::CourtSharedIds(claim);
        if let Some(old) = b.state.staged_court_row_v1(&key) {
            let Row::CourtSharedIds(old) = old else {
                return Err(refused("shared ID key contains foreign row"));
            };
            old.check_context_classified_v2(&b.state).map_err(shared_ids_auth_fault_v2)?;
            if ctx.daa_score < old.accepted_daa {
                return Err(refused("shared replay precedes accepted IDs"));
            }
            return Ok(false);
        }
        let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
        q.exact_v1::<u32>(ids.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        let _owned =
            crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let row = shared::SharedTirIdsV1 {
            version: 1,
            claim,
            scope_id: c.scope_id,
            rows_root,
            ids: ids.to_vec(),
            block: ctx.block,
            accepted_daa: ctx.daa_score,
            material_until_daa: c.material_until_daa,
        };
        b.write_staged_row_v1(key, Some(Row::CourtSharedIds(Box::new(row))))?;
        Ok(true)
    })();
    if result.is_err() {
        b.restore(saved);
    }
    result
}

/// Private signed-data stream, not a production ingress or signed duty result.
pub(super) fn stream_control(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,
    control:&crate::palw_court_shared_tir_stream_v1::SignedSharedStreamV1)->Result<bool,PalwStateV2Error> {
    let saved=b.try_checkpoint()?;let result=stream_control_inner(b,ctx,control);
    if result.is_err() {b.restore(saved);}result
}
fn stream_control_inner(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,
    control:&crate::palw_court_shared_tir_stream_v1::SignedSharedStreamV1)->Result<bool,PalwStateV2Error> {
    use crate::palw_court_shared_tir_stream_v1::{SharedStreamActionV1 as Action,SharedStreamTargetV1 as Target};
    use crate::palw_court_shared_tir_stream_state_v1 as stream;
    let c=shared::context(&b.state,control.claim).map_err(refused)?;clock(c,ctx)?;
    if control.scope!=c.scope_id {return Err(refused("shared stream native scope mismatch"));}
    let source=c.source(&b.state).map_err(refused)?;
    stream::ready_at(&b.state,c,&control.signer,ctx.daa_score).map_err(refused)?;
    let network=b.extras.tir_public_network_domain_v1.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    control.validate_bond_signature(network,&b.state).map_err(refused)?;
    let bond=b.state.bond(&control.signer).ok_or(PalwStateV2Error::MissingBond(control.signer))?;
    if !matches!(bond.status,PalwBondStatusV2::Active) || bond.collateral<b.params.min_collateral_sompi() {
        return Err(refused("shared stream signer is not active/above existing floor"));
    }
    let (maximum,_)=stream::target_bound(&b.state,c,&source,&control.target).map_err(refused)?;
    let slot=control.slot_id().map_err(refused)?;let key=Key::CourtSharedStream(slot);
    let old=match b.state.staged_court_row_v1(&key) {
        Some(Row::CourtSharedStream(row))=>Some((**row).clone()),None=>None,_=>return Err(refused("shared stream foreign row")),
    };
    if old.as_ref().is_some_and(|s|ctx.daa_score<s.last_daa || s.finished.as_ref().is_some_and(|f|ctx.daa_score<f.daa)) {
        return Err(refused("shared stream replay predates accepted prefix/EOF"));
    }
    match &control.action {
        Action::Begin {total_bytes,body_hash}=>{
            if *total_bytes==0 || *total_bytes>maximum {return Err(refused("shared stream declaration exceeds original bound"));}
            let id=control.declared_stream_id(*total_bytes,*body_hash).map_err(refused)?;
            if let Some(old)=old {
                if old.stream_id!=id {return Err(refused("shared stream declaration is immutable"));}return Ok(false);
            }
            if let Target::Unit {request_id,..}=&control.target {
                let Some(Row::CourtSharedQuery(q))=b.state.staged_court_row_v1(&Key::CourtSharedQuery(*request_id)) else {return Err(refused("shared query missing"));};
                if q.opened_daa>ctx.daa_score {return Err(refused("shared stream precedes request"));}
            }
            let row=stream::SharedTirStreamV1 {version:1,claim:control.claim,scope_id:control.scope,target:control.target.clone(),signer:control.signer,
                slot_id:slot,stream_id:id,total_bytes:*total_bytes,body_hash:*body_hash,begun_block:ctx.block,begun_daa:ctx.daa_score,
                begin_intent:control.signing_message(network).map_err(refused)?,last_block:ctx.block,last_daa:ctx.daa_score,
                accepted_bytes:0,head:Hash64::default(),finished:None,material_until_daa:c.material_until_daa};
            b.write_staged_row_v1(key,Some(Row::CourtSharedStream(Box::new(row))))?;Ok(true)
        },
        Action::Part {stream_id,offset,bytes}=>{
            let mut old=old.ok_or_else(||refused("shared Part has no Begin"))?;
            if old.stream_id!=*stream_id {return Err(refused("shared Part stream substitution"));}
            let chunk_key=Key::CourtSharedChunk(slot,*offset);
            if let Some(Row::CourtSharedChunk(chunk))=b.state.staged_court_row_v1(&chunk_key) {
                chunk.check_context(&b.state).map_err(refused)?;
                if chunk.bytes!=*bytes {return Err(refused("shared Part retry changed bytes"));}return Ok(false);
            }
            if old.finished.is_some() {return Err(refused("shared Part after EOF"));}
            let end=crate::palw_court_shared_tir_stream_v1::check_shared_stream_part_v1(old.total_bytes,old.accepted_bytes,*offset,bytes.len()).map_err(refused)?;
            let mut q=crate::palw_native_resource_v1::TirNativeRequestsV1::new();q.exact_v1::<u8>(bytes.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
            let _raw=crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
            let chunk=stream::SharedTirChunkV1 {version:1,claim:control.claim,slot_id:slot,stream_id:*stream_id,offset:*offset,previous:old.head,
                bytes:bytes.to_vec(),block:ctx.block,daa:ctx.daa_score,intent:control.signing_message(network).map_err(refused)?,material_until_daa:old.material_until_daa};
            old.head=chunk.digest().map_err(refused)?;old.accepted_bytes=end;old.last_block=ctx.block;old.last_daa=ctx.daa_score;
            b.write_staged_row_v1(chunk_key,Some(Row::CourtSharedChunk(Box::new(chunk).into())))?;
            b.write_staged_row_v1(key,Some(Row::CourtSharedStream(Box::new(old))))?;Ok(true)
        },
        Action::Finish {stream_id}=>{
            let mut old=old.ok_or_else(||refused("shared Finish has no Begin"))?;
            if old.stream_id!=*stream_id {return Err(refused("shared Finish stream substitution"));}
            let bytes=old.body(&b.state).map_err(refused)?;
            old.authenticate_eof(&b.state,&source,&bytes).map_err(refused)?;
            if old.finished.is_some() {return Ok(false);}
            match &old.target {
                Target::Unit {ordinal,..}=>{share_body(b,ctx,old.claim,*ordinal,&bytes)?;},
                Target::FullIds=>{
                    let ids=stream::decode_ids_body(&source,&bytes).map_err(refused)?;
                    share_ids(b,ctx,old.claim,ids.rows_root_v1(),ids.ids_v1())?;
                },
            }
            old.finished=Some(stream::SharedStreamFinishedV1 {block:ctx.block,daa:ctx.daa_score});
            b.write_staged_row_v1(key,Some(Row::CourtSharedStream(Box::new(old))))?;Ok(true)
        },
    }
}

/// Records an explicit fresh data response by a literal Ready signer. It does
/// not infer completion from uploads, transfer a semantic duty, or permit fault.
pub(super) fn data_response(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,
    response:&crate::palw_court_shared_tir_response_v1::SignedSharedDataResponseV1)->Result<bool,PalwStateV2Error> {
    use crate::palw_court_shared_tir_response_v1 as reply;
    let saved=b.try_checkpoint()?;
    let result=(|| {
        let q=reply::query(&b.state,response.request_id).map_err(refused)?;
        if response.claim!=q.claim {return Err(refused("shared response claim substitution"));}
        let stage=reply::shared_data_response_stage_v1(&b.state,response.request_id,response.signer).map_err(refused)?;
        let c=shared::context(&b.state,q.claim).map_err(refused)?;clock(c,ctx)?;
        if response.scope_id!=stage.scope_id || response.query_block!=stage.query_block || response.query_daa!=stage.query_daa
            || stage.data.as_ref().is_none_or(|data|data.digest!=response.data_digest || ctx.daa_score<data.daa)
            || ctx.daa_score<stage.query_daa || ctx.daa_score<stage.ready_daa
            || ctx.block==stage.query_block {
            return Err(refused("shared response lacks fresh native query/data context"));
        }
        let network=b.extras.tir_public_network_domain_v1.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
        response.validate_bond_signature(network,&b.state).map_err(refused)?;
        let bond=b.state.bond(&response.signer).ok_or(PalwStateV2Error::MissingBond(response.signer))?;
        if !matches!(bond.status,PalwBondStatusV2::Active) || bond.collateral<b.params.min_collateral_sompi() {
            return Err(refused("shared data responder is not active/above existing floor"));
        }
        if let Some(old)=stage.response {
            if ctx.daa_score<old.accepted_daa {return Err(refused("shared response replay predates accepted fact"));}return Ok(false);
        }
        let row=reply::SharedDataResponseV1 {version:1,claim:response.claim,scope_id:response.scope_id,request_id:response.request_id,
            signer:response.signer,query_block:response.query_block,query_daa:response.query_daa,data_digest:response.data_digest,
            accepted_block:ctx.block,accepted_daa:ctx.daa_score,intent_digest:response.signing_message(network).map_err(refused)?,
            material_until_daa:c.material_until_daa};
        b.write_staged_row_v1(Key::CourtSharedResponse(row.request_id,row.signer),Some(Row::CourtSharedResponse(Box::new(row))))?;
        Ok(true)
    })();
    if result.is_err() {b.restore(saved);}result
}

/// Private canonical numerical root installer. Still no public dispatcher,
/// notice certificate, original all-branch funded admission or default right.
pub(super) fn root(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2,
    control: &crate::palw_court_shared_tir_root_v1::SignedSharedTirRootV1) -> Result<bool, PalwStateV2Error> {
    use crate::palw_court_shared_tir_root_v1::{check_shared_tir_root_v1, SharedTirRootRowV1};
    use crate::palw_native_resource_v1::NativeHeapV1;
    let saved = b.try_checkpoint()?;
    let result = (|| {
        let rules = b.extras.native_court_rules_v1.ok_or_else(|| refused("shared root lacks native accepting rules"))?;
        if rules.at != *ctx || b.extras.tir_public_network_domain_v1 != Some(rules.network_domain) {
            return Err(refused("shared root accepting point/network mismatch"));
        }
        // Covers the retained control and simultaneous native root-verifier
        // clone, including its temporary binding clone and registered program.
        // Source analysis/CPU/IO/full-profile work remains separately unclosed.
        let bond = b.state.bond(&control.signer).ok_or_else(|| refused("shared root producer missing"))?;
        let claim = b.state.claim(&control.claim).ok_or_else(|| refused("shared root claim missing"))?;
        let record = b.state.tir_class_v1(&claim.class_id).ok_or_else(|| refused("shared root registered program missing"))?;
        let bytes = control.native_heap_v1("canonical shared root copies").bytes()
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(control.root.finalize.binding.native_heap_v1("root temporary binding").bytes()?))
            .and_then(|n| n.checked_add(record.program.len() as u64))
            .and_then(|n| n.checked_add(std::mem::size_of::<SharedTirRootRowV1>() as u64))
            .and_then(|n| n.checked_add(bond.pubkey.len() as u64))
            .ok_or_else(|| refused("shared root clone inventory missing/overflow"))?;
        let _lease = crate::palw_native_resource_v1::reserve_native_requests_v1(bytes)
            .map_err(PalwStateV2Error::NativeResource)?;
        check_shared_tir_root_v1(&b.state, b.params, &rules, control).map_err(refused)?;
        let key = Key::CourtSharedRoot(control.claim, control.leaf);
        if let Some(old) = b.state.staged_court_row_v1(&key) {
            let Row::CourtSharedRoot(old) = old else { return Err(refused("shared root slot contains foreign row")); };
            old.check(&key).and_then(|()| old.check_context(&b.state)).map_err(refused)?;
            if ctx.daa_score < old.rules.at.daa_score || !old.same_numerical_root(control) {
                return Err(refused("canonical shared root cannot be replaced or forked"));
            }
            return Ok(false);
        }
        let c = shared::context(&b.state, control.claim).map_err(refused)?;
        let bond = b.state.bond(&control.signer).ok_or_else(|| refused("shared root producer missing"))?;
        let row = SharedTirRootRowV1 { version: 1, control: control.clone(), rules,
            signer_pubkey: bond.pubkey.clone(), material_until_daa: c.material_until_daa };
        b.write_staged_row_v1(key, Some(Row::CourtSharedRoot(Box::new(row).into())))?;
        Ok(true)
    })();
    if result.is_err() { b.restore(saved); }
    result
}

/// Private data disclosure only, never an accepted choice or duty completion.
pub(super) fn round(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,
    c:&crate::palw_court_shared_tir_round_v1::SignedSharedTirRoundV1)->Result<bool,PalwStateV2Error>{
    use crate::palw_court_shared_tir_round_v1 as rounds;
    use crate::palw_native_resource_v1::{NativeHeapV1,TirNativeRequestsV1};
    let saved=b.try_checkpoint()?;
    let result=(|| {
        let rules=b.extras.native_court_rules_v1.ok_or_else(||refused("shared round native rules missing"))?;
        if rules.at!=*ctx || b.extras.tir_public_network_domain_v1!=Some(rules.network_domain) {return Err(refused("shared round native point/network mismatch"));}
        let r=rounds::root(&b.state,c.claim,c.leaf).map_err(refused)?;
        let bond=b.state.bond(&c.signer).ok_or_else(||refused("shared round producer missing"))?;
        let record=b.state.tir_class_v1(&r.control.root.finalize.binding.job_context.shape_profile_id)
            .ok_or_else(||refused("shared round registered program missing"))?;
        let mut q=TirNativeRequestsV1::new();
        let heap=|x:crate::palw_native_resource_v1::NativeCoverageV1|x.bytes().ok_or_else(||refused("shared round typed clone inventory missing"));
        q.add_v1(heap(r.control.native_heap_v1("round root verifier"))?.checked_mul(2).ok_or_else(||refused("round root clone overflow"))?).map_err(PalwStateV2Error::NativeResource)?;
        q.add_v1(heap(r.control.root.finalize.binding.native_heap_v1("round temporary binding"))?).map_err(PalwStateV2Error::NativeResource)?;
        q.add_v1(record.program.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        q.add_v1(heap(c.native_heap_v1("retained round control"))?).map_err(PalwStateV2Error::NativeResource)?;
        q.exact_v1::<rounds::SharedTirRoundRowV1>(1).map_err(PalwStateV2Error::NativeResource)?;
        q.exact_v1::<u8>(bond.pubkey.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        // Two coexisting arithmetic phase views: no whole-tree frontier/map
        // clone. Shape terms come from this actual immutable root's vectors.
        for _ in 0..2 {
            q.exact_v1::<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>(1).map_err(PalwStateV2Error::NativeResource)?;
            q.add_v1(heap(r.control.root.elements.native_heap_v1("phase elements"))?).map_err(PalwStateV2Error::NativeResource)?;
            for _ in 0..2 {q.add_v1(heap(r.control.root.totals.native_heap_v1("phase root/claim"))?).map_err(PalwStateV2Error::NativeResource)?;}
            let n=r.control.root.elements.len() as u64;
            q.vec_v1::<u16>(n).map_err(PalwStateV2Error::NativeResource)?;
            q.vec_v1::<crate::palw_tir_dissect_v1::PalwTirFoldV1>(n).map_err(PalwStateV2Error::NativeResource)?;
            q.vec_v1::<(i128,i128)>(n).map_err(PalwStateV2Error::NativeResource)?;
        }
        q.vec_v1::<(u64,u64)>(u64::from(r.control.arity)).map_err(PalwStateV2Error::NativeResource)?;
        let _lease=crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        // Lane r6-p2-shared: a version-2 control is the terminal tile's POSITION round (RFC-0024 A), written only past
        // `palw_tir_bounded_door_v1`; below it the fold refuses it and nothing is stored.
        if c.version==rounds::SHARED_TIR_POSITION_ROUND_VERSION_V2 && !b.extras.tir_bounded_door_active {
            return Err(refused("shared position round below palw_tir_bounded_door_v1"));
        }
        rounds::check_shared_tir_round_v1(&b.state,b.params,&rules,c).map_err(refused)?;
        let key=Key::CourtSharedRound(c.id().map_err(refused)?);
        if let Some(old)=b.state.staged_court_row_v1(&key){
            let Row::CourtSharedRound(old)=old else{return Err(refused("shared round foreign slot"));};
            if ctx.daa_score<old.rules.at.daa_score || !old.same_statement(c){return Err(refused("shared round cannot replace canonical disclosure"));}
            return Ok(false);
        }
        let row=rounds::SharedTirRoundRowV1 {version:1,control:c.clone(),rules,signer_pubkey:bond.pubkey.clone(),material_until_daa:r.material_until_daa};
        b.write_staged_row_v1(key,Some(Row::CourtSharedRound(Box::new(row).into())))?;
        Ok(true)
    })();
    if result.is_err(){b.restore(saved);}result
}

/// Preserve a local comparison, without applying any claim/court/money effect.
pub(super) fn bottom(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,
    c:&crate::palw_court_shared_tir_bottom_v1::SignedSharedTirBottomV1)
    ->Result<(bool,crate::palw_court_shared_tir_bottom_v1::SharedTirBottomOutcomeV1),PalwStateV2Error>{
    bottom_inner(b,ctx,c,None)
}
/// **RFC-0024 A on the shared court (lane r6-p2-shared).** The single-position bottom: the same retention, resource
/// admission and replacement rule as [`bottom`], judged over the position the writer row names, stored as a row of
/// version 2 in its own slot. Refused (nothing written) unless `palw_tir_bounded_door_v1` is in force at this block.
pub(super) fn position_bottom(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,
    c:&crate::palw_court_shared_tir_bottom_v1::SignedSharedTirPositionBottomV2)
    ->Result<(bool,crate::palw_court_shared_tir_bottom_v1::SharedTirBottomOutcomeV1),PalwStateV2Error>{
    if !b.extras.tir_bounded_door_active {return Err(refused("shared position bottom below palw_tir_bounded_door_v1"));}
    bottom_inner(b,ctx,&c.control,Some(c.position))
}
fn bottom_inner(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,
    c:&crate::palw_court_shared_tir_bottom_v1::SignedSharedTirBottomV1,position:Option<u8>)
    ->Result<(bool,crate::palw_court_shared_tir_bottom_v1::SharedTirBottomOutcomeV1),PalwStateV2Error>{
    use crate::palw_court_shared_tir_bottom_v1 as bottom;
    use crate::palw_court_shared_tir_round_v1 as rounds;
    use crate::palw_native_resource_v1::{NativeHeapV1,TirNativeRequestsV1};
    let saved=b.try_checkpoint()?;
    let result=(|| {
        let rules=b.extras.native_court_rules_v1.ok_or_else(||refused("shared bottom native rules missing"))?;
        if rules.at!=*ctx || b.extras.tir_public_network_domain_v1!=Some(rules.network_domain){return Err(refused("shared bottom native point/network mismatch"));}
        bottom::check_shared_bottom_cost_v1(&c.bottom,&rules.court).map_err(refused)?;
        let r=rounds::root(&b.state,c.claim,c.leaf).map_err(refused)?;
        let bond=b.state.bond(&c.filer).ok_or_else(||refused("shared bottom filer missing"))?;
        let record=b.state.tir_class_v1(&r.control.root.finalize.binding.job_context.shape_profile_id).ok_or_else(||refused("shared bottom program missing"))?;
        let mut q=TirNativeRequestsV1::new();
        let heap=|v:crate::palw_native_resource_v1::NativeCoverageV1|v.bytes().ok_or_else(||refused("shared bottom clone inventory missing"));
        q.add_v1(heap(r.control.native_heap_v1("bottom root verifier"))?.checked_mul(2).ok_or_else(||refused("root copy overflow"))?).map_err(PalwStateV2Error::NativeResource)?;
        q.add_v1(heap(r.control.root.finalize.binding.native_heap_v1("root binding"))?).map_err(PalwStateV2Error::NativeResource)?;
        // A position bottom holds two more transient copies (its signed wrapper and its signing message).
        q.add_v1(heap(c.native_heap_v1("bottom verifier/retained copies"))?.checked_mul(if position.is_some(){4}else{2}).ok_or_else(||refused("bottom copy overflow"))?).map_err(PalwStateV2Error::NativeResource)?;
        q.add_v1(heap(c.bottom.binding.native_heap_v1("bottom binding"))?).map_err(PalwStateV2Error::NativeResource)?;
        q.add_v1((record.program.len() as u64).checked_mul(2).ok_or_else(||refused("program copy overflow"))?).map_err(PalwStateV2Error::NativeResource)?;
        q.exact_v1::<bottom::SharedTirBottomRowV1>(1).map_err(PalwStateV2Error::NativeResource)?;
        q.exact_v1::<u8>(bond.pubkey.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        for _ in 0..2 {
            q.exact_v1::<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>(1).map_err(PalwStateV2Error::NativeResource)?;
            q.add_v1(heap(r.control.root.elements.native_heap_v1("phase elements"))?).map_err(PalwStateV2Error::NativeResource)?;
            for _ in 0..2{q.add_v1(heap(r.control.root.totals.native_heap_v1("phase root/claim"))?).map_err(PalwStateV2Error::NativeResource)?;}
            let n=r.control.root.elements.len() as u64;
            q.vec_v1::<u16>(n).map_err(PalwStateV2Error::NativeResource)?;
            q.vec_v1::<crate::palw_tir_dissect_v1::PalwTirFoldV1>(n).map_err(PalwStateV2Error::NativeResource)?;
            q.vec_v1::<(i128,i128)>(n).map_err(PalwStateV2Error::NativeResource)?;
        }
        q.vec_v1::<(u64,u64)>(u64::from(r.control.arity)).map_err(PalwStateV2Error::NativeResource)?;
        let _lease=crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let positioned=position.map(|position|bottom::SignedSharedTirPositionBottomV2 {control:c.clone(),position});
        let checked=match &positioned {
            None=>bottom::check_shared_tir_bottom_v1(&b.state,b.params,&rules,c),
            Some(pc)=>bottom::check_shared_tir_position_bottom_v2(&b.state,b.params,&rules,pc),
        }.map_err(refused)?;
        let digest=checked.outcome_digest().map_err(refused)?;let outcome=checked.outcome().clone();
        let slot=match &positioned {None=>c.id(),Some(pc)=>pc.id()}.map_err(refused)?;
        let key=Key::CourtSharedBottom(slot);
        if let Some(old)=b.state.staged_court_row_v1(&key){
            let Row::CourtSharedBottom(old)=old else{return Err(refused("shared bottom foreign slot"));};
            if ctx.daa_score<old.rules.at.daa_score || old.control.parent_id!=c.parent_id || old.outcome_digest!=digest {
                return Err(refused("shared bottom cannot replace local comparison"));
            }
            return Ok((false,outcome));
        }
        let row=bottom::SharedTirBottomRowV1 {
            version:if position.is_some(){bottom::SHARED_TIR_BOTTOM_ROW_POSITION_VERSION_V2}else{1},
            control:c.clone(),rules,filer_pubkey:bond.pubkey.clone(),
            material_until_daa:r.material_until_daa,outcome_digest:digest,position};
        b.write_staged_row_v1(key,Some(Row::CourtSharedBottom(Box::new(row).into())))?;
        Ok((true,outcome))
    })();
    if result.is_err(){b.restore(saved);}result
}
