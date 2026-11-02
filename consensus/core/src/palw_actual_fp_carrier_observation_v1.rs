//! Readonly actual retained carrier/checkpoint observation, not paid authority.
//! A normal public core API is intentional: dependency cfg(test) does not
//! propagate from consensus tests. No writer, codec or protocol tag is added.
use super::*;
use crate::palw_native_resource_v1::{NativeRetainedV1,TirNativeRequestsV1,reserve_native_requests_v1};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PalwActualFpCarrierObservationV1 {
    point:PalwBlockContextV2,
    source:(Hash64,PalwBlockContextV2,Hash64),consumer:(Hash64,PalwBlockContextV2,Hash64),
    owner9_rows:[u64;3],owner9_seals:[(Hash64,u64);3],
    source_frame_events:u64,consumer_frame_events:u64,
    descriptor_roots:[(Option<Hash64>,u64);5],checkpoint_records:u64,exact_edges:u64,current_winners:[Option<Hash64>;2],
}
impl PalwActualFpCarrierObservationV1 {
    pub fn point(&self)->PalwBlockContextV2{self.point}
    pub fn source(&self)->(Hash64,PalwBlockContextV2,Hash64){self.source}
    pub fn consumer(&self)->(Hash64,PalwBlockContextV2,Hash64){self.consumer}
    pub fn owner9_rows(&self)->[u64;3]{self.owner9_rows}
    pub fn owner9_seals(&self)->[(Hash64,u64);3]{self.owner9_seals}
    pub fn native_frame_events(&self)->(u64,u64){(self.source_frame_events,self.consumer_frame_events)}
    pub fn descriptor_roots(&self)->[(Option<Hash64>,u64);5]{self.descriptor_roots}
    pub fn checkpoint_records(&self)->u64{self.checkpoint_records}
    pub fn current_winners(&self)->[Option<Hash64>;2]{self.current_winners}
    pub fn exact_edges(&self)->u64{self.exact_edges}
}
fn unavailable(s:&'static str)->PalwStateV2Error{PalwStateV2Error::CarriageInconsistent(s.into())}
/// Input tuples name an independently retained accepted transaction and actual
/// birth, never a quote point. Every returned scalar is obtained from a checked
/// native reader at THIS state. Absence/collected material is an error, not zero.
/// This function grants no payment, readiness or import authorization.
pub fn palw_actual_fp_carrier_observation_v1(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,
    source:(Hash64,PalwBlockContextV2,Hash64),consumer:(Hash64,PalwBlockContextV2,Hash64),
)->Result<NativeRetainedV1<PalwActualFpCarrierObservationV1>,PalwStateV2Error>{
    use super::palw_claim_local_carrier_lineage_v9 as lineage;
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<NativeRetainedV1<PalwActualFpCarrierObservationV1>>(1).map_err(PalwStateV2Error::NativeResource)?;
    let lease=reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let point=*state.last_point().ok_or_else(||unavailable("carrier observation actual point absent"))?;
    if !params.claim_local_settlement_active_at(point.daa_score){return Err(unavailable("carrier observation claimlocal inactive"))}
    for(id,born,winner)in[source,consumer]{
        if lineage::with_exact_source_v9(state,id,born,winner,|_|Ok(()))?.is_none(){
            return Err(PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable))
        }
    }
    let a=lineage::observe_sealed_winner_geometry_v9(state,source.0,source.1,source.2)?;
    let c=lineage::observe_sealed_winner_geometry_v9(state,consumer.0,consumer.1,consumer.2)?;
    let inputs=lineage::observe_sealed_consumer_geometry_v9(state,consumer.0,consumer.1)?;
    let mut exact_edges=0u64;
    lineage::visit_observed_sources_v9(state,&inputs,|fact,owner|{
        if (fact.source(),fact.source_point(),fact.spent_txid())==source {
            if (owner.source(),owner.accepting_point(),owner.winning_txid())!=source{return Err(unavailable("observed carrier edge owner differs"))}
            exact_edges=exact_edges.checked_add(1).ok_or(PalwStateV2Error::Overflow("observed carrier edge count"))?;
        }
        Ok(())
    })?;
    if exact_edges!=1{return Err(unavailable("actual exact spent winning source edge missing or duplicated"))}
    // Native v10 readers authenticate exact rooted frame key/point/codec. No
    // caller counts and no raw-prefix presence substitute for actual events.
    let count_events=|expected:PalwBlockContextV2|->Result<u64,PalwStateV2Error>{
        let mut n=0u64;
        let present=palw_claim_local_native_frame_events_v10::visit_rooted_native_events_v10(state,expected.block,|actual,_,_|{
            if actual!=expected{return Err(unavailable("actual rooted v10 full point differs"))}
            n=n.checked_add(1).ok_or(PalwStateV2Error::Overflow("native scoped event count"))?;Ok(())
        })?;
        if !present||n==0{return Err(PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable))}
        Ok(n)
    };
    let source_events=count_events(source.1)?;let consumer_events=count_events(consumer.1)?;
    // Descriptor/current-root observation only. This scoped helper does not
    // material-resolve a checkpoint or claim monetary/credit validity.
    let d=palw_claim_local_checkpoint_descriptor_v3::observe_current_descriptor_v1(state)?
        .ok_or(PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable))?;
    let mut current_winners=[None;2];
    for(i,id)in[source.0,consumer.0].into_iter().enumerate(){
        let _key=reserve_native_requests_v1(64).map_err(PalwStateV2Error::NativeResource)?;
        let bytes=state.kernel_route().and_then(|r|r.aux.get(&(crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1,id.as_byte_slice().to_vec())));
        current_winners[i]=bytes.map(|v|Hash64::try_from(v.as_slice()).map_err(|_|unavailable("actual current winner codec"))).transpose()?;
    }
    let roots=[d.records,d.literals,d.native,d.coordinates,d.liability];
    let rows=[a.referenced_rows()[0].component_rows_after,a.referenced_rows()[1].component_rows_after,inputs.referenced_rows()[2].component_rows_after];
    if rows.contains(&0)||d.records.rows==0||d.native.rows==0{return Err(unavailable("actual carrier9/D3 component inventory empty"))}
    let summary_a=a.sealed_summary();let summary_c=c.sealed_summary();let facts=inputs.scope().sealed_facts;
    Ok(NativeRetainedV1::from_owner_reserved_v1(PalwActualFpCarrierObservationV1{point,source,consumer,
        owner9_rows:rows,owner9_seals:[(summary_a.data,summary_a.bytes),(summary_c.data,summary_c.bytes),(facts.data,facts.bytes)],
        source_frame_events:source_events,consumer_frame_events:consumer_events,
        descriptor_roots:roots.map(|r|(r.node,r.rows)),checkpoint_records:d.records.rows,exact_edges,current_winners},lease))
}


/// Resolve an independently observed LIVE stored birth through the actual
/// bidirectionally checked source-admission alias. Stored carrying provenance
/// and actual accepting point (including subsidy) are distinct identities.
/// Historical consumers use the full point returned here, never a zero subsidy.
pub fn palw_actual_fp_accepting_point_from_live_stored_v1(
    state:&PalwChainStateV2,id:Hash64,stored:(Hash64,u64,u64),
)->Result<NativeRetainedV1<PalwBlockContextV2>,PalwStateV2Error>{
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<NativeRetainedV1<PalwBlockContextV2>>(1).map_err(PalwStateV2Error::NativeResource)?;
    let lease=reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let c=state.claim(&id).ok_or(PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable))?;
    if (c.accepted_block,c.accepted_daa,c.accepted_blue_score)!=stored{return Err(unavailable("observed stored birth changed before actual-point resolution"))}
    let view=palw_claim_local_source_admission_v8::for_stored_claim_v8(state,id,c)?
        .ok_or(PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(view.accepting_point(),lease))
}
