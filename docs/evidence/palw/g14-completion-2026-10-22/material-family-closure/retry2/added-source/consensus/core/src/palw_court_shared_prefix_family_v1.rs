//! Private exact version11 Prefix boundary dependency geometry and native replay.
//! No public tail acceptance, paid supply or complete-original authority.
use super::{SharedTirBuilderStoreV1 as Store,prefix_terminal::{Budget,PrefixMaterialReplayV1}};
use crate::{Hash64,palw_state_v2::PalwChainStateV2,palw_native_resource_v1::{NativeRetainedV1 as Held,NativeDecodeErrorV1 as E,NativeCoverageV1 as C,TirNativeRequestsV1}};
use crate::palw_tir_public_source_v1::{PalwTirPublicSourceV1 as Source,PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
use crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 as Frame;
use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as Graph,SourceMaterialTargetV3 as T};
fn bad(s:&str)->E<String>{E::Decode(s.into())}
fn resource()->E<String>{E::Resource(C::unsupported::<NativePrefixFamilyV1<'static>>("private Prefix family","local traversal/allocation/checked count"))}
#[derive(Debug,PartialEq,Eq)]
pub(crate) struct PrefixBoundaryDependencyV1 {pub ordinal:u64,pub index:u64,pub role:u64,pub logical:Hash64,pub absolute:[u8;64]}
pub(crate) struct NativePrefixFamilyV1<'a>{frame:&'a Frame,source:&'a Held<Source>,graph:&'a Held<Graph>,profile:Hash64,total:u64,deps:Vec<PrefixBoundaryDependencyV1>}
impl<'a> NativePrefixFamilyV1<'a>{
    pub(crate) fn derive_native_v1(state:&PalwChainStateV2,frame:&'a Frame,source:&'a Held<Source>,graph:&'a Held<Graph>,profile:Hash64,max_leaves:u64,max_visits:u64)
        ->Result<Held<Self>,E<String>> {Self::derive_reserved_v1(state,frame,source,graph,profile,max_leaves,max_visits,crate::palw_native_resource_v1::reserve_native_requests_v1)}
    pub(crate) fn derive_reserved_v1<F>(state:&PalwChainStateV2,frame:&'a Frame,source:&'a Held<Source>,graph:&'a Held<Graph>,profile:Hash64,max_leaves:u64,max_visits:u64,reserve:F)
        ->Result<Held<Self>,E<String>> where F:FnOnce(u64)->Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,C>{
        let actual=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,frame.plan_v1().original_claim).ok_or_else(||bad("Prefix family current original absent"))?;
        if actual!=frame{return Err(bad("Prefix family substitutes current original frame"));}
        let job=&frame.original_v1().job;
        if job.version!=crate::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION||!matches!(job.tail.as_ref(),Some(crate::palw_freeprompt_v3::PalwFpJobTailV1::Prefix(_))){return Err(bad("Prefix family requires exact version11 Prefix tail"));}
        let named=crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e|bad(&e.to_string()))?;
        let record=state.tir_class_v1(&job.class_id).ok_or_else(||bad("Prefix family class absent"))?;
        let layout=crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state,job.class_id).ok_or_else(||bad("Prefix family layout absent"))?;
        let initial=super::prefix_terminal::prelude_requests(frame,record,layout)?;
        let mut lease=reserve(initial).map_err(E::Resource)?;
        graph.check_frame_binding_v3(frame,source,profile).map_err(E::Decode)?;
        {
            let c=crate::palw_court_shared_tir_state_v1::context(state,frame.plan_v1().original_claim).map_err(E::Decode)?;
            let current=c.source(state).map_err(E::Decode)?;
            if c.profile_id!=profile||current.source_id_v1()!=source.source_id_v1()||current.header_v1()!=source.header_v1(){return Err(bad("Prefix family current context/source mismatch"));}
        }
        // Exactly the existing native prefix context, under its conservative
        // Source/StepSpace/frame quote. It is dropped after output construction.
        let prefix=crate::palw_staged_court_state_v1::palw_tir_prefix_context_v1(record,layout,frame.header_v1(),job,frame.descriptor_v1().maximum_leaves).map_err(E::Decode)?;
        let total=prefix.template.total_v1();if total>max_leaves{return Err(resource());}
        let count=total.checked_mul(graph.role_slots_v3()).ok_or_else(resource)?;
        let mut q=TirNativeRequestsV1::new();q.exact_v1::<Self>(1).map_err(E::Resource)?;q.exact_v1::<PrefixBoundaryDependencyV1>(count).map_err(E::Resource)?;
        if let Some(ticket)=lease.as_mut(){ticket.grow_memory_v1(q.bytes_v1()).map_err(E::Resource)?;}
        let cap=usize::try_from(count).map_err(|_|resource())?;let mut deps=Vec::new();deps.try_reserve_exact(cap).map_err(|_|resource())?;
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,source,profile).map_err(E::Decode)?;
        let mut visits=0u64;
        for ordinal in 0..total {
            let leaf=prefix.space.prefix_state_leaf_v1(&prefix.job_context,named.prefix_tokens,ordinal).ok_or_else(||bad("Prefix family canonical ordinal missing"))?;
            let before=deps.len();
            let native_target=T::Unit(set.step_leaf_ordinal_v1(leaf.index).map_err(E::Decode)?);
            // Existing local traversal limit now counts actual keyed candidate
            // lookups; each target/role is charged once before map/hash access.
            for role in 0..graph.role_slots_v3(){
                visits=visits.checked_add(1).ok_or_else(resource)?;if visits>max_visits{return Err(resource());}
                let n=graph.finish_node_v3(native_target,role).map_err(E::Decode)?.ok_or_else(||bad("Prefix family canonical Finish absent"))?;
                if deps.len()==cap{return Err(bad("Prefix family excess supplier finishes"));}
                deps.push(PrefixBoundaryDependencyV1{ordinal,index:leaf.index,role,logical:n.id_v3(),absolute:graph.absolute_packet_id_v3(n.id_v3())});
            }
            let group=&mut deps[before..];group.sort_unstable_by_key(|d|d.role);
            if group.len() as u64!=graph.role_slots_v3()||group.iter().enumerate().any(|(n,d)|d.role!=n as u64){return Err(bad("Prefix family lacks all-role boundary finishes"));}
        }
        if deps.len()!=cap{return Err(bad("Prefix family final inventory mismatch"));}
        Ok(Held::from_owner_reserved_v1(Self{frame,source,graph,profile,total,deps},lease))
    }
    pub(crate) fn total_v1(&self)->u64{self.total}
    pub(crate) fn dependencies_v1(&self)->&[PrefixBoundaryDependencyV1]{&self.deps}
    /// This is actual material replay, not a provider completion/default token.
    pub(crate) fn replay_v1(&self,store:&Store<'_>,state:&PalwChainStateV2,budget:Budget)->Result<Held<PrefixMaterialReplayV1>,E<String>>{
        let claim=self.frame.plan_v1().original_claim;
        let current=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,claim).ok_or_else(||bad("Prefix family current original absent"))?;
        if current!=self.frame||store.source.source_id_v1()!=self.source.source_id_v1()||store.source.header_v1()!=self.source.header_v1(){return Err(bad("Prefix family current frame/material changed"));}
        self.graph.check_frame_binding_v3(current,&store.source,self.profile).map_err(E::Decode)?;
        for d in &self.deps {if d.role==0&&!matches!(store.unit(Q::StepLeaf{index:d.index}),Some(R::StepLeaf{..})){return Err(bad("Prefix family lacks actual authenticated boundary body"));}}
        // Existing guard re-resolves current context and canonical prefix, checks
        // body lengths before decode, authenticates stream and computes verdict.
        let replay=store.replay_prefix_material_v1(state,claim,budget)?;
        if replay.leaves()!=self.total{return Err(bad("Prefix family replay count changed"));}Ok(replay)
    }
}
