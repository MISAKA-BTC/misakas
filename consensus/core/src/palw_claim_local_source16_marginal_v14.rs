/// Actual archived ORIGINAL-Court escrow snapshot, never Source16 retention
/// payment/old-paid coverage. The existing signed contract pays its old scope.
pub(super) struct OriginalFundingEvidenceV14 {
    point:PalwBlockContextV2,
    escrow:NativeRetainedV1<Escrow>,
}
impl OriginalFundingEvidenceV14 {
    pub(super) fn accepting_point(&self)->PalwBlockContextV2{self.point}
    pub(super) fn original_court_escrow_snapshot(&self)->&Escrow{&self.escrow}
    pub(super) fn source_archive_paid_coverage(&self)->store::MissingFinancialCoverageV14{store::MissingFinancialCoverageV14::MissingFinancialCoverage}
    pub(super) fn is_current_settlement_state(&self)->bool{false}
}
pub(super) fn original_funding_evidence_v14(
    state:&PalwChainStateV2,id:Hash64,point:PalwBlockContextV2,
)->Result<Option<OriginalFundingEvidenceV14>,PalwStateV2Error>{
    let Some(bundle)=stored(state,id,point).map_err(err)? else{return Ok(None);};
    if !bundle.completed || bundle.bodies.rows!=21{return Err(err(bad("funding evidence incomplete native original archive")));}
    let escrow=read_body::<Escrow>(state,&bundle,8).map_err(err)?;
    if escrow.contract_id!=bundle.contract || escrow.claim!=id || escrow.accepted_daa!=point.daa_score {
        return Err(err(bad("funding evidence differs from actual original contract")));
    }
    Ok(Some(OriginalFundingEvidenceV14{point,escrow}))
}
// Native Data observer compares readonly prediction to the EXACT actual
// publication journal, including physical B Vec prefix and skipped dedup.
#[cfg(test)]
fn assert_publication_delta_v14(
    entries:&[PalwDeltaEntryV2],facts:&store::NativeContextMarginalFactsV14,
) {
    let mut count=0u64;let mut wire=0u64;let mut newrows=0u64;
    let mut newbytes=0u64;let mut written=0u64;let mut oldbytes=0u64;
    for entry in entries {
        let PalwDeltaEntryV2::KernelRouteRow{table,key,old,new}=entry else{panic!("context publication crossed native writer")};
        assert_eq!(*table,PALW_CLAIM_LOCAL_TABLE_V1);
        count+=1;wire+=borsh::object_length(entry).unwrap() as u64;
        if let Some(row)=new {written+=key.len() as u64+row.len() as u64;if old.is_none(){newrows+=1;newbytes+=key.len() as u64+row.len() as u64;}}
        if let Some(row)=old {oldbytes+=key.len() as u64+row.len() as u64;}
    }
    let q=facts.amounts();
    assert_eq!(q.delta_entries,count);assert_eq!(q.delta_encoded_bytes,wire);
    assert_eq!(q.new_rows,newrows);assert_eq!(q.new_key_and_value_bytes,newbytes);
    assert_eq!(q.written_key_and_value_bytes,written);assert_eq!(q.old_existing_key_and_value_bytes,oldbytes);
    assert_eq!(facts.financial_coverage(),store::MissingFinancialCoverageV14::MissingFinancialCoverage);
}
#[cfg(test)]
pub(super) fn assert_actual_marginal_dedup_v14(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,extras:&PalwTransitionExtrasV1,id:Hash64,point:PalwBlockContextV2,
) {
    let bundle=stored(state,id,point).unwrap().unwrap();
    let reader=StateNodesV2(state);let key=body_key(4);
    let current=nodes::lookup(&reader,bundle.bodies,key.as_byte_slice()).unwrap().unwrap();
    let actual=reader.body(current).unwrap();
    let plan=nodes::update(&reader,bundle.bodies,key.as_byte_slice(),Some(current),Some(current)).unwrap();
    let q=store::quote_component_publication_v14(state,&plan,Some(actual.as_slice()),Some(bundle.scope)).unwrap();
    assert_eq!(q.amounts().delta_entries,0);assert_eq!(q.amounts().new_rows,0);
    assert!(q.amounts().present_dedup_rows>=1);assert_eq!(q.amounts().unchanged_write_attempts,1);
    let mut b=TransitionBuilder::new(state,params,false,false,false,false,extras);let before=b.state.state_root();
    store::publish_owned_component_v2(&mut b,bundle.scope,&plan,Some(actual.as_slice())).unwrap();
    assert_publication_delta_v14(&b.entries,&q);assert_eq!(b.state.state_root(),before);
    let archived=original_funding_evidence_v14(state,id,point).unwrap().unwrap();
    assert_eq!(archived.accepting_point(),point);assert_eq!(archived.original_court_escrow_snapshot().contract_id,bundle.contract);
    assert_eq!(archived.source_archive_paid_coverage(),store::MissingFinancialCoverageV14::MissingFinancialCoverage);
    assert!(!archived.is_current_settlement_state());
    // Actual extra A2 pin already exists: no rewrite or second delta. Removal
    // quote reports physical pin loss but never a financial/closed-duty grant.
    let raw=encode(&*bundle).unwrap();let a=inventory(value(&raw),&[],Some(bundle.bodies)).unwrap();
    let pin=store::quote_archive_pin_v14(state,&a,false).unwrap();assert_eq!(pin.amounts().delta_entries,0);assert_eq!(pin.amounts().present_dedup_rows,1);
    let remove=store::quote_archive_pin_v14(state,&a,true).unwrap();assert_eq!(remove.amounts().delta_entries,1);assert_eq!(remove.amounts().archive_component_pins_removed,1);assert_eq!(remove.amounts().archive_literal_pins_removed,1);
    let _missing=reserve_native_requests_v1(65).unwrap();
    let mut absent=state.clone();let a_raw=encode(&*a).unwrap();absent.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1,akey(value(&a_raw).data).to_vec()));
    assert!(store::quote_archive_pin_v14(&absent,&a,true).is_err(),"missing required pin is unavailable, never zero-cost closed ownership");
    let loaded=PalwStateCarriageV2::from_state(state).into_state(params,Some(before)).unwrap();
    assert_eq!(store::quote_component_publication_v14(&loaded,&plan,Some(actual.as_slice()),Some(bundle.scope)).unwrap().amounts(),q.amounts());
}
