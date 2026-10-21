//! RFC-0029 rooted data candidate. No production installer, lifecycle tag,
//! complete reservation certificate, signer-duty completion or default right.
use crate::{Hash64,BlockHash};
use crate::palw_state_v2::{PalwBondKeyV2,PalwChainStateV2};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key,PalwStagedCourtRowV1 as Row};
use crate::palw_court_tir_requests_v1::TirSharedRequestSetV1;
use borsh::{BorshDeserialize,BorshSerialize};
pub fn shared_native_profile_id_v1(service_profile:Hash64)->Hash64 {
    Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64)
        .key(b"misaka-palw/shared-native-profile/v1").hash(service_profile.as_byte_slice()).as_bytes().try_into().expect("64"))
}
/// Context is NOT a paid-admission or timely-notice certificate. Only an
/// eventual complete original coordinator may install this on a real network.
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct SharedTirContextV1 {
    pub version:u16,pub claim:Hash64,pub allocation_id:Hash64,pub source_id:Hash64,
    pub rules_id:Hash64,pub profile_id:Hash64,pub scope_id:Hash64,pub queries:u64,
    pub original_accepted_daa:u64,pub hard_daa:u64,pub material_until_daa:u64,
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct SharedTirBodyV1 {
    pub block:BlockHash,pub accepted_daa:u64,pub digest:Hash64,pub bytes:Vec<u8>,
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct SharedTirQueryV1 {
    pub version:u16,pub claim:Hash64,pub scope_id:Hash64,pub ordinal:u64,pub request_id:Hash64,
    pub opened_block:BlockHash,pub opened_daa:u64,pub opened_by:PalwBondKeyV2,pub intent_digest:Hash64,
    pub material_until_daa:u64,pub body:Option<SharedTirBodyV1>,
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct SharedTirIdsV1 {
    pub version:u16,pub claim:Hash64,pub scope_id:Hash64,pub rows_root:Hash64,pub ids:Vec<u32>,
    pub block:BlockHash,pub accepted_daa:u64,pub material_until_daa:u64,
}
pub fn shared_body_digest_v1(bytes:&[u8])->Hash64 {
    Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64)
        .key(b"misaka-palw/shared-source-body/v1").hash(bytes).as_bytes().try_into().expect("64"))
}
impl SharedTirContextV1 {
    pub fn check(&self,key:&Key)->Result<(),String> {
        if self.version!=1 || *key!=Key::CourtSharedContext(self.claim) || self.queries==0
            || self.original_accepted_daa.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)!=Some(self.hard_daa) {
            return Err("invalid shared context version/key/clock".into());
        }
        Ok(())
    }
    pub(crate) fn check_binding(&self,service:&crate::palw_court_service_state_v2::CourtServiceStateV2)->Result<(),String> {
        let b=service.claims.get(&self.claim).ok_or("shared context lost original service binding")?;
        if self.profile_id!=shared_native_profile_id_v1(service.profile_id)
            || (self.allocation_id,self.source_id,self.original_accepted_daa,self.hard_daa)
                !=(b.allocation_id,b.source_id,b.original_accepted_daa,b.hard_deadline_daa) {
            return Err("shared context substitutes service namespace/original scope".into());
        }
        Ok(())
    }
    pub fn source(&self,state:&PalwChainStateV2)->Result<crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,String> {
        self.check(&Key::CourtSharedContext(self.claim))?;
        self.check_binding(state.court_service_state_v1().ok_or("shared context has no service book")?)?;
        let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,self.claim).ok_or("shared context lost original frame")?;
        let p=frame.plan_v1();
        if (p.allocation_id,p.source_id,p.rules_id,p.terms.accepted_daa,p.terms.material_until_daa)
            !=(self.allocation_id,self.source_id,self.rules_id,self.original_accepted_daa,self.material_until_daa) {
            return Err("shared context substitutes retained original".into());
        }
        let record=state.tir_class_v1(&frame.original_v1().job.class_id).ok_or("shared original class is absent")?;
        let layout=crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state,record.facts.class_id)
            .ok_or("shared original registered layout is absent")?;
        let source=frame.source_v1(record,layout,frame.descriptor_v1().maximum_leaves)?;
        let set=TirSharedRequestSetV1::new(frame,&source,self.profile_id)?;
        if self.scope_id!=set.scope_id()? || self.queries!=set.len() {return Err("shared context substitutes closed universe".into());}
        Ok(source)
    }
    /// Read-only original-bound parameter identity check. The caller admits source
    /// reconstruction/CPU/IO separately; known program/index owners are reserved here.
    /// No insertion, delivery/custody, timely notice, payer or eligibility follows.
    pub(crate) fn check_parameter_opening(&self,state:&PalwChainStateV2,scope:Hash64,
        opening:&crate::palw_artifact::PalwArtifactOpeningV1)->Result<(),String> {
        self.with_parameter_verifier(state,scope,|v|v.check(opening).map_err(|e|e.to_string()))
    }
    /// The input is one complete existing artifact opening, never an incomplete-prefix slot.
    pub(crate) fn decode_parameter_opening(&self,state:&PalwChainStateV2,scope:Hash64,leaf:u32,bytes:&[u8])
        ->Result<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_artifact::PalwArtifactOpeningV1>,String> {
        self.with_parameter_verifier(state,scope,|v|v.decode(leaf,bytes))
    }
    fn with_parameter_verifier<T>(&self,state:&PalwChainStateV2,scope:Hash64,
        check:impl FnOnce(&crate::palw_tir_court_v1::TirInventoryOpeningVerifierV1<'_>)->Result<T,String>)->Result<T,String> {
        if scope!=self.scope_id {return Err("shared parameter scope substitution".into());}
        // Require this exact retained row, not a caller-constructed context with valid fields.
        if context(state,self.claim)?!=self {return Err("shared parameter context is not retained".into());}
        let source=self.source(state)?;
        let record=state.tir_class_v1(&source.header_v1().job_context.shape_profile_id)
            .ok_or("shared parameter registered program absent")?;
        let program=crate::palw_native_resource_v1::decode_native_tir_program_v1(record.program.as_slice()).map_err(|e|e.to_string())?;
        let requests=crate::palw_tir_court_v1::TirInventoryOpeningVerifierV1::index_requests(&program).map_err(|e|e.to_string())?;
        let _index=crate::palw_native_resource_v1::reserve_native_requests_v1(requests).map_err(|e|e.to_string())?;
        let verifier=crate::palw_tir_court_v1::TirInventoryOpeningVerifierV1::new(&program,source.header_v1().artifact_root)
            .ok_or("shared parameter inventory is not derivable")?;
        check(&verifier)
    }
    pub fn check_context(&self,state:&PalwChainStateV2)->Result<(),String> {self.source(state).map(|_|())}
}
pub(crate) fn context(state:&PalwChainStateV2,claim:Hash64)->Result<&SharedTirContextV1,String> {
    match state.staged_court_row_v1(&Key::CourtSharedContext(claim)) {
        Some(Row::CourtSharedContext(c))=>Ok(c),_=>Err("no installed native shared context".into()),
    }
}
impl SharedTirQueryV1 {
    pub fn check(&self,key:&Key)->Result<(),String> {
        if self.version!=1 || *key!=Key::CourtSharedQuery(self.request_id)
            || self.body.as_ref().is_some_and(|b|b.bytes.is_empty() || b.accepted_daa<self.opened_daa
                || b.digest!=shared_body_digest_v1(&b.bytes)) {
            return Err("invalid shared query shape/key/body".into());
        }
        Ok(())
    }
    pub(crate) fn same_opening(&self,old:&Self)->bool {
        (self.version,self.claim,self.scope_id,self.ordinal,self.request_id,self.opened_block,self.opened_daa,self.opened_by,self.intent_digest,self.material_until_daa)
        ==(old.version,old.claim,old.scope_id,old.ordinal,old.request_id,old.opened_block,old.opened_daa,old.opened_by,old.intent_digest,old.material_until_daa)
    }
    pub fn check_context(&self,state:&PalwChainStateV2)->Result<(),String> {
        self.check(&Key::CourtSharedQuery(self.request_id))?;
        let c=context(state,self.claim)?;let source=c.source(state)?;
        let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,self.claim).ok_or("shared query lost frame")?;
        let set=TirSharedRequestSetV1::new(frame,&source,c.profile_id)?;
        if (self.scope_id,self.material_until_daa)!=(c.scope_id,c.material_until_daa)
            || self.opened_daa<c.original_accepted_daa || self.opened_daa>=c.hard_daa
            || self.request_id!=set.request_id(self.ordinal)? {
            return Err("shared query substitutes original coordinate/clock".into());
        }
        if let Some(body)=&self.body {
            if body.accepted_daa>=c.hard_daa {return Err("shared data exceeds original hard clock".into());}
            authenticate_body(state,c,&source,set.request_at(self.ordinal)?,&body.bytes)?;
        }
        // Historical requester identity was checked at acceptance. A requester
        // exiting later must not invalidate public data or transfer it to an owner.
        Ok(())
    }
}
impl SharedTirIdsV1 {
    pub fn check(&self,key:&Key)->Result<(),String> {
        if self.version!=1 || *key!=Key::CourtSharedIds(self.claim) {return Err("invalid shared ID row version/key".into());}
        Ok(())
    }
    pub fn check_context(&self,state:&PalwChainStateV2)->Result<(),String> {
        self.check(&Key::CourtSharedIds(self.claim))?;
        let c=context(state,self.claim)?;let source=c.source(state)?;
        if self.scope_id!=c.scope_id || self.material_until_daa!=c.material_until_daa
            || self.accepted_daa<c.original_accepted_daa || self.accepted_daa>=c.hard_daa {
            return Err("shared ID row substitutes context/clock".into());
        }
        source.validate_complete_ids_v1(self.rows_root,&self.ids)
    }
}
/// Step roots do not depend on decoded IDs. Trace roots still require a fresh
/// original outer-root check, but an owned copy of the ID sequence is unneeded.
fn rows_anchor_for_query(state:&PalwChainStateV2,c:&SharedTirContextV1,
    source:&crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    query:crate::palw_tir_public_source_v1::PalwTirPublicRequestV1)
    ->Result<Option<crate::palw_tir_public_source_v1::PalwTirPublicRowsAnchorV1>,String>{
    use crate::palw_tir_public_source_v1::PalwTirPublicRequestV1 as Q;
    match query {
        Q::StepNode {..}|Q::StepLeaf {..}|Q::StepRun {..}|Q::Ids {..}=>Ok(None),
        Q::RowsNode {..}|Q::TileNode {..}|Q::TraceTile {..}=>match state.staged_court_row_v1(&Key::CourtSharedIds(c.claim)) {
            Some(Row::CourtSharedIds(row))=>{
                row.check(&Key::CourtSharedIds(c.claim))?;
                if row.scope_id!=c.scope_id {return Err("shared body substitutes ID context".into());}
                source.rows_anchor_from_complete_ids_v1(row.rows_root,&row.ids).map(Some)
            },_=>Ok(None),
        },
    }
}
pub(crate) fn authenticate_body(state:&PalwChainStateV2,c:&SharedTirContextV1,
    source:&crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    query:crate::palw_tir_public_source_v1::PalwTirPublicRequestV1,bytes:&[u8])->Result<(),String> {
    let bound=source.unit_bound_v1(&query)?;
    let _scratch=crate::palw_native_resource_v1::reserve_native_requests_v1(bound.resident_ram_bytes).map_err(|e|e.to_string())?;
    let response=source.decode_response_v1(&query,bytes)?;
    let rows=rows_anchor_for_query(state,c,source,query)?;
    source.authenticate_with_rows_v1(&query,&response,None,rows.as_ref())?;
    Ok(())
}

/// A runtime data capability/view, never a serializable answer or admission flag.
pub enum RootedSharedDataV1<'a> {
    Unit(crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1>),
    Ids {rows_root:Hash64,ids:&'a [u32]},
}
pub fn read_shared_data_v1(state:&PalwChainStateV2,claim:Hash64,ordinal:u64)->Result<Option<RootedSharedDataV1<'_>>,String> {
    let c=context(state,claim)?;let source=c.source(state)?;
    let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,claim).ok_or("shared read lost original")?;
    let set=TirSharedRequestSetV1::new(frame,&source,c.profile_id)?;let query=set.request_at(ordinal)?;
    let key=Key::CourtSharedQuery(set.request_id(ordinal)?);
    let Some(Row::CourtSharedQuery(row))=state.staged_court_row_v1(&key) else {return Ok(None);};
    row.check_context(state)?;
    if let crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::Ids {offset}=query {
        if let Some(Row::CourtSharedIds(ids))=state.staged_court_row_v1(&Key::CourtSharedIds(claim)) {
            ids.check_context(state)?;
            let start=offset as usize;let count=ids.ids.len().checked_sub(start).ok_or("shared read ID offset")?
                .min(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize);
            let end=start.checked_add(count).ok_or("shared read ID end overflow")?;
            return Ok(Some(RootedSharedDataV1::Ids {rows_root:ids.rows_root,ids:&ids.ids[start..end]}));
        }
        return Ok(None);
    }
    let Some(body)=&row.body else {return Ok(None);};
    let bound=source.unit_bound_v1(&query)?;
    let lease=crate::palw_native_resource_v1::reserve_native_requests_v1(bound.resident_ram_bytes).map_err(|e|e.to_string())?;
    let response=source.decode_response_v1(&query,&body.bytes)?;
    let rows=rows_anchor_for_query(state,c,&source,query)?;
    let unit=source.authenticate_with_rows_v1(&query,&response,None,rows.as_ref())?;
    Ok(Some(RootedSharedDataV1::Unit(crate::palw_native_resource_v1::NativeRetainedV1::from_owner_reserved_v1(unit,lease))))
}
