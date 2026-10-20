//! Internal rooted shared-data candidate. No lifecycle dispatcher calls these
//! functions and no production original coordinator installs the context yet.
use super::*;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key,PalwStagedCourtRowV1 as Row};
use crate::palw_court_shared_tir_state_v1 as shared;
use crate::palw_court_shared_tir_frontier_v1::{SignedSharedRequestV1,SharedRequestEffectV1};
fn refused(s:impl Into<String>)->PalwStateV2Error {PalwStateV2Error::CarriageInconsistent(s.into())}
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
    let row=shared::SharedTirQueryV1 {version:1,claim,scope_id:c.scope_id,ordinal:intent.ordinal,request_id:intent.request_id,
        opened_block:ctx.block,opened_daa:ctx.daa_score,opened_by:intent.signer,intent_digest:intent.signing_message(network).map_err(refused)?,
        material_until_daa:c.material_until_daa,body:None};
    b.write_staged_row_v1(key,Some(Row::CourtSharedQuery(Box::new(row))))?;
    Ok(SharedRequestEffectV1::Opened)
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
pub(super) fn share_ids(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,claim:Hash64,
    rows_root:Hash64,ids:&[u32])->Result<bool,PalwStateV2Error> {
    let saved=b.try_checkpoint()?;
    let result=(|| {
        let c=shared::context(&b.state,claim).map_err(refused)?;clock(c,ctx)?;
        let source=c.source(&b.state).map_err(refused)?;source.validate_complete_ids_v1(rows_root,ids).map_err(refused)?;
        let key=Key::CourtSharedIds(claim);
        if let Some(old)=b.state.staged_court_row_v1(&key) {
            let Row::CourtSharedIds(old)=old else {return Err(refused("shared ID key contains foreign row"));};
            old.check_context(&b.state).map_err(refused)?;
            if ctx.daa_score<old.accepted_daa {return Err(refused("shared replay precedes accepted IDs"));}
            return Ok(false);
        }
        let mut q=crate::palw_native_resource_v1::TirNativeRequestsV1::new();q.exact_v1::<u32>(ids.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        let _owned=crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let row=shared::SharedTirIdsV1 {version:1,claim,scope_id:c.scope_id,rows_root,ids:ids.to_vec(),block:ctx.block,
            accepted_daa:ctx.daa_score,material_until_daa:c.material_until_daa};
        b.write_staged_row_v1(key,Some(Row::CourtSharedIds(Box::new(row))))?;Ok(true)
    })();
    if result.is_err() {b.restore(saved);}result
}
