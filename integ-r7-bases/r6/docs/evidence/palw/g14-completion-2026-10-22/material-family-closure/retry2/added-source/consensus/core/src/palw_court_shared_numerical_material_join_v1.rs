//! Private actual numerical-root readset join. No delivery/paid/complete authority.
use super::{SharedTirBuilderStoreV1 as Store,SignedSharedTerminalIntentV1};
use crate::{Hash64,palw_native_resource_v1::{NativeRetainedV1 as Held,NativeDecodeErrorV1 as E,NativeCoverageV1 as C,TirNativeRequestsV1,NativeHeapV1}};
use crate::palw_tir_public_source_v1::{PalwTirPublicSourceV1 as Source,PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
use crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1 as Cover;
use crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3 as Numerical;
use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as Graph,SourceMaterialTargetV3 as T};
use crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 as Frame;
use crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1 as I;
fn bad(s:&str)->E<String>{E::Decode(s.into())}
fn resource()->E<String>{E::Resource(C::unsupported::<NumericalMaterialJoinV1<'static>>("numerical material join","local traversal/allocation/checked counter"))}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum NativeMaterialTargetV1 {StepLeaf(u64),FullIds}
#[derive(Debug,PartialEq,Eq)]
pub(crate) struct SupplierDependencyV1 {pub root:u64,pub material:NativeMaterialTargetV1,pub role:u64,pub logical:Hash64,pub absolute:[u8;64]}
/// Exact unresolved operand coordinate, not a fabricated seven-family Unit.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum UnresolvedSupplyV1 {Parameter{root:u64,leaf:u32},Prompt{root:u64,position:u32}}
pub(crate) struct NumericalMaterialJoinV1<'a>{frame:&'a Frame,source:&'a Held<Source>,graph:&'a Held<Graph>,numerical:&'a Held<Numerical>,cover:&'a Held<Cover>,profile:Hash64,deps:Vec<SupplierDependencyV1>}
fn target(i:I)->Option<NativeMaterialTargetV1>{match i {I::PromptPosition(_)=>None,I::GeneratedId(_)=>Some(NativeMaterialTargetV1::FullIds),I::CarryStepLeaf(n)|I::CommittedStepLeaf(n)|I::FixedCheckpointStepLeaf(n)|I::HistoryStepLeaf(n)=>Some(NativeMaterialTargetV1::StepLeaf(n))}}
impl<'a> NumericalMaterialJoinV1<'a>{
    pub(crate) fn derive_native_v1(frame:&'a Frame,source:&'a Held<Source>,graph:&'a Held<Graph>,numerical:&'a Held<Numerical>,cover:&'a Held<Cover>,profile:Hash64,court:&crate::palw_mode_v2::PalwCourtParamsV2,demand:crate::palw_tir_fence2_v1::PalwTirDemandRulesV1,max_visits:u64)
        ->Result<Held<Self>,E<String>>{Self::derive_reserved_v1(frame,source,graph,numerical,cover,profile,court,demand,max_visits,crate::palw_native_resource_v1::reserve_native_requests_v1)}
    pub(crate) fn derive_reserved_v1<F>(frame:&'a Frame,source:&'a Held<Source>,graph:&'a Held<Graph>,numerical:&'a Held<Numerical>,cover:&'a Held<Cover>,profile:Hash64,court:&crate::palw_mode_v2::PalwCourtParamsV2,demand:crate::palw_tir_fence2_v1::PalwTirDemandRulesV1,max_visits:u64,reserve:F)
        ->Result<Held<Self>,E<String>> where F:FnOnce(u64)->Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,C>{
        // Count borrowed actual metadata before any output/request/context allocation.
        if numerical.arity_v3()!=court.dissection_arity()||numerical.demand_v3()!=demand||numerical.source_id_v3()!=source.source_id_v1()||cover.parameters_v1().source_id!=source.source_id_v1()
            ||numerical.original_leaves_v3()!=source.leaf_count_v1()||cover.parameters_v1().original_leaves!=source.leaf_count_v1()
            ||numerical.roots_v3().len()!=cover.roots_v1().len()||numerical.roots_v3().len()!=cover.parameters_v1().roots.len(){return Err(bad("numerical join owner source/root cardinality mismatch"));}
        let mut targets=0u64;
        for ((n,r),p) in numerical.roots_v3().iter().zip(cover.roots_v1()).zip(&cover.parameters_v1().roots){
            if n.leaf_v3()!=r.leaf||r.leaf!=p.leaf{return Err(bad("numerical join exact root relation mismatch"));}
            targets=targets.checked_add(1).ok_or_else(resource)?;
            for &i in &r.inputs {if target(i).is_some(){targets=targets.checked_add(1).ok_or_else(resource)?;}}
        }
        let total=targets.checked_mul(graph.role_slots_v3()).ok_or_else(resource)?;
        let mut q=TirNativeRequestsV1::new();q.exact_v1::<Self>(1).map_err(E::Resource)?;q.exact_v1::<SupplierDependencyV1>(total).map_err(E::Resource)?;
        let clone=frame.native_heap_v1("numerical join frame binding scratch");q.add_v1(clone.bytes().ok_or(E::Resource(clone))?).map_err(E::Resource)?;
        let bytes=borsh::object_length(frame).map_err(|e|bad(&e.to_string()))? as u64;
        q.exact_v1::<u8>(bytes.max(1024).checked_mul(2).ok_or_else(resource)?).map_err(E::Resource)?;
        let lease=reserve(q.bytes_v1()).map_err(E::Resource)?;
        graph.check_frame_binding_v3(frame,source,profile).map_err(E::Decode)?;
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,source,profile).map_err(E::Decode)?;
        let cap=usize::try_from(total).map_err(|_|resource())?;let mut deps=Vec::new();deps.try_reserve_exact(cap).map_err(|_|resource())?;
        let mut visits=0u64;
        for r in cover.roots_v1(){
            for material in std::iter::once(NativeMaterialTargetV1::StepLeaf(r.leaf)).chain(r.inputs.iter().copied().filter_map(target)){
                let before=deps.len();
                let native_target=match material{NativeMaterialTargetV1::FullIds=>T::FullIds,NativeMaterialTargetV1::StepLeaf(index)=>T::Unit(set.step_leaf_ordinal_v1(index).map_err(E::Decode)?)};
                // One work unit per checked canonical Finish lookup, retaining
                // exact target×all-role membership, not zero-cost scan elision.
                for role in 0..graph.role_slots_v3(){
                    visits=visits.checked_add(1).ok_or_else(resource)?;if visits>max_visits{return Err(resource());}
                    let node=graph.finish_node_v3(native_target,role).map_err(E::Decode)?.ok_or_else(||bad("numerical join canonical Finish absent"))?;
                    if deps.len()==cap{return Err(bad("numerical join excess finish dependencies"));}
                    deps.push(SupplierDependencyV1{root:r.leaf,material,role,logical:node.id_v3(),absolute:graph.absolute_packet_id_v3(node.id_v3())});
                }
                // Native graph has exact one Finish per role. Check, do not trust cardinality alone.
                let group=&mut deps[before..];group.sort_unstable_by_key(|d|d.role);
                if group.len() as u64!=graph.role_slots_v3()||group.iter().enumerate().any(|(n,d)|d.role!=n as u64){return Err(bad("numerical join lacks all-role exact supplier finishes"));}
            }
        }
        if deps.len()!=cap{return Err(bad("numerical join inventory mismatch"));}
        Ok(Held::from_owner_reserved_v1(Self{frame,source,graph,numerical,cover,profile,deps},lease))
    }
    pub(crate) fn numerical_v1(&self)->&Numerical {self.numerical}
    pub(crate) fn dependencies_v1(&self)->&[SupplierDependencyV1]{&self.deps}
    pub(crate) fn unresolved_v1(&self)->impl Iterator<Item=UnresolvedSupplyV1>+'_ {
        self.cover.parameters_v1().roots.iter().flat_map(|r|r.parameters.iter().map(move |&leaf|UnresolvedSupplyV1::Parameter{root:r.leaf,leaf}))
            .chain(self.cover.roots_v1().iter().flat_map(|r|r.inputs.iter().filter_map(move |i|if let I::PromptPosition(position)=*i {Some(UnresolvedSupplyV1::Prompt{root:r.leaf,position})}else{None})))
    }
    /// Existing authenticated Store readset guards, not future supply evidence.
    pub(crate) fn check_material_v1(&self,store:&Store<'_>)->Result<(),String>{
        if store.source.source_id_v1()!=self.source.source_id_v1()||store.source.header_v1()!=self.source.header_v1(){return Err("numerical join Store from another source".into());}
        store.check_root_input_cover_v1(self.cover)?;
        for root in self.cover.roots_v1(){store.check_root_parameters_v1(self.cover,root.leaf)?;
            if !matches!(store.unit(Q::StepLeaf{index:root.leaf}),Some(R::StepLeaf{..})){return Err("numerical join lacks authenticated numerical root StepLeaf".into());}}
        Ok(())
    }
    /// Selected-root material only; parameters are still the existing conservative
    /// allpotential set for this root, not selectedpath exact-operand optimization.
    pub(crate) fn check_named_material_v1(&self,store:&Store<'_>,leaf:u64)->Result<(),String>{
        if store.source.source_id_v1()!=self.source.source_id_v1()||store.source.header_v1()!=self.source.header_v1(){return Err("numerical join Store from another source".into());}
        if !self.numerical.roots_v3().iter().any(|r|r.leaf_v3()==leaf){return Err("numerical join selected root absent".into());}
        store.check_named_root_input_cover_v1(self.cover,leaf)?;store.check_root_parameters_v1(self.cover,leaf)?;
        if !matches!(store.unit(Q::StepLeaf{index:leaf}),Some(R::StepLeaf{..})){return Err("numerical join lacks authenticated selected root StepLeaf".into());}Ok(())
    }
    /// Real guarded finalizer, with current rules/ancestor/signature checks still
    /// owned by the existing implementation. Builder clones need separate quota.
    pub(crate) fn build_terminal_v1(&self,store:&Store<'_>,state:&crate::palw_state_v2::PalwChainStateV2,params:&crate::palw_state_v2::PalwStateParamsV2,
        rules:&crate::palw_court_close_receipt_v1::NativeCourtRulesV1,intent:&SignedSharedTerminalIntentV1,limit:crate::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1)
        ->Result<(crate::palw_tir_court_v1::PalwTirConeRefutationV1,crate::palw_tir_court_v1::PalwTirBuilderMeterV1),String>{
        let current=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,intent.claim).ok_or("numerical join current frame absent")?;
        self.graph.check_frame_binding_v3(current,&store.source,self.profile)?;
        if current!=self.frame||rules.court.dissection_arity()!=self.numerical.arity_v3()||rules.step_ladder!=self.frame.plan_v1().owner_work.rules.max_step_leaf_count{return Err("numerical join current frame/court geometry changed".into());}
        let root=self.numerical.roots_v3().iter().find(|r|r.leaf_v3()==intent.leaf).ok_or("numerical join terminal root outside native geometry")?;
        if !root.paths_v3().iter().any(|p|p.terminal_v3()&&p.path_v3()==intent.path.as_slice()){return Err("numerical join terminal path outside actual native geometry".into());}
        self.check_named_material_v1(store,intent.leaf)?;store.build_terminal_with_parameter_cover(state,params,rules,intent,limit,self.cover)
    }
}
