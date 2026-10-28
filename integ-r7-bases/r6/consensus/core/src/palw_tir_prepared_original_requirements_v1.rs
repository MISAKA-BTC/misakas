// Included within the actual native fold; no caller-decoded holder constructor.
use crate::palw_court_shared_tir_builder_store_v1::original_aggregate::{OriginalFamilyShapeV1,AggregateErrorV1};
#[derive(Clone,Copy)]
pub(super) struct PreparedRequirementBudgetsV1 {
    pub numerical:crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3,
    pub input:crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1,
    pub source:crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3,
}
#[derive(Debug)]
pub(super) enum PreparedRequirementsErrorV1 {Geometry(PreparedGeometryErrorV1),Shape(AggregateErrorV1),Native(crate::palw_native_resource_v1::NativeDecodeErrorV1<String>),MissingSharedServiceNamespace}
enum PreparedSourceRequirementsV1 {
    MissingSharedServiceNamespace,
    Native {namespace:Hash64,graph:crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3>},
}
/// Real native-origin Source/numerical/all-root requirements before FP reserve.
/// Retains no forged Claim/context, no paid permit and no fully reservable draft.
pub(super) struct PreparedOriginalRequirementsV1 {
    geometry:PreparedOriginalGeometryV1,
    shape:OriginalFamilyShapeV1,
    actors:PreparedActorCatalogV1,
    numerical:crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3>,
    inputs:crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1>,
    source:PreparedSourceRequirementsV1,
    // New wrapper inline only. Moved old Frame payload retains its legacy native
    // preparation admission, and all new Source/geometry/cover have own leases.
    _wrapper:Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl PreparedOriginalGeometryV1 {
    /// Consuming actual native-origin entry only; public Frame decoders and
    /// arbitrary profile hashes cannot enter this API. Old object path is opt-in.
    pub(super) fn prepare_requirements_v1(self,b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2,budget:PreparedRequirementBudgetsV1)
        ->Result<PreparedOriginalRequirementsV1,PreparedRequirementsErrorV1>{
        self.prepared.check_preclaim_context_v1(b,ctx).map_err(PreparedRequirementsErrorV1::Geometry)?;
        let mut q=crate::palw_native_resource_v1::TirNativeRequestsV1::new();q.exact_v1::<PreparedOriginalRequirementsV1>(1).map_err(|c|PreparedRequirementsErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)))?;
        let wrapper=crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(|c|PreparedRequirementsErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)))?;
        // Same immutable snapshot that successful native preparation consumed,
        // not postclaim lookup, caller declaration or reconstructed fake frame.
        let shape=OriginalFamilyShapeV1::derive_v1(self.frame_v1(),self.prepared.semantic_snapshot.as_ref()).map_err(PreparedRequirementsErrorV1::Shape)?;
        let actors=PreparedActorCatalogV1::derive_v1(&self,b,ctx).map_err(PreparedRequirementsErrorV1::Geometry)?;
        let numerical=self.numerical_v3(b,ctx,budget.numerical).map_err(PreparedRequirementsErrorV1::Geometry)?;
        let class=self.frame_v1().original_v1().job.class_id;
        let record=b.state.tir_class_v1(&class).ok_or_else(||PreparedRequirementsErrorV1::Geometry(PreparedGeometryErrorV1::State(refused("prepared input class absent"))))?;
        let layout=crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state,class).ok_or_else(||PreparedRequirementsErrorV1::Geometry(PreparedGeometryErrorV1::State(refused("prepared input layout absent"))))?;
        let inputs=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::root_input_cover_native_v1(self.source_v1(),record,layout,budget.input).map_err(PreparedRequirementsErrorV1::Native)?;
        // This actual relation is checked even when no service namespace exists.
        if numerical.source_id_v3()!=inputs.parameters_v1().source_id||numerical.original_leaves_v3()!=inputs.parameters_v1().original_leaves
            ||numerical.roots_v3().len()!=inputs.roots_v1().len()||numerical.roots_v3().iter().zip(inputs.roots_v1()).any(|(n,i)|n.leaf_v3()!=i.leaf){
            return Err(PreparedRequirementsErrorV1::Geometry(PreparedGeometryErrorV1::State(refused("prepared numerical/input root relation mismatch"))));
        }
        let source=match self.source_graph_v3(b,ctx,budget.source){
            Ok(graph)=>{let book=b.state.court_service_state_v1().ok_or(PreparedRequirementsErrorV1::MissingSharedServiceNamespace)?;
                PreparedSourceRequirementsV1::Native{namespace:crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(book.profile_id),graph}},
            Err(PreparedGeometryErrorV1::MissingSharedServiceNamespace)=>PreparedSourceRequirementsV1::MissingSharedServiceNamespace,
            Err(e)=>return Err(PreparedRequirementsErrorV1::Geometry(e)),
        };
        Ok(PreparedOriginalRequirementsV1{geometry:self,shape,actors,numerical,inputs,source,_wrapper:wrapper})
    }
}
impl PreparedOriginalRequirementsV1 {
    pub(super) fn frame_v1(&self)->&Frame {self.geometry.frame_v1()}
    pub(super) fn shape_v1(&self)->OriginalFamilyShapeV1 {self.shape}
    pub(super) fn numerical_v1(&self)->&crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3 {&self.numerical}
    pub(super) fn inputs_v1(&self)->&crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1 {&self.inputs}
    pub(super) fn source_graph_v1(&self)->Result<&crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3,PreparedRequirementsErrorV1>{
        match &self.source{PreparedSourceRequirementsV1::Native{graph,..}=>Ok(graph),PreparedSourceRequirementsV1::MissingSharedServiceNamespace=>Err(PreparedRequirementsErrorV1::MissingSharedServiceNamespace)}
    }
    /// Borrowing join avoids a self-referential owner and keeps all original
    /// Source/num/cover tickets alive. No caller profile or surrogate Claim.
    pub(super) fn numerical_material_join_v1<'a>(&'a self,b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2,max_visits:u64)
        ->Result<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_shared_tir_builder_store_v1::numerical_material_join::NumericalMaterialJoinV1<'a>>,PreparedRequirementsErrorV1>{
        self.geometry.prepared.check_preclaim_context_v1(b,ctx).map_err(PreparedRequirementsErrorV1::Geometry)?;
        let PreparedSourceRequirementsV1::Native{namespace,graph}=&self.source else{return Err(PreparedRequirementsErrorV1::MissingSharedServiceNamespace)};
        let book=b.state.court_service_state_v1().ok_or(PreparedRequirementsErrorV1::MissingSharedServiceNamespace)?;
        if *namespace!=crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(book.profile_id){return Err(PreparedRequirementsErrorV1::Geometry(PreparedGeometryErrorV1::State(refused("prepared source native namespace changed"))));}
        crate::palw_court_shared_tir_builder_store_v1::numerical_material_join::NumericalMaterialJoinV1::derive_native_v1(self.frame_v1(),self.geometry.source_v1(),graph,&self.numerical,&self.inputs,*namespace,&self.geometry.prepared.native.court,self.geometry.demand_v1(),max_visits).map_err(PreparedRequirementsErrorV1::Native)
    }
    /// Old reservation/Claim/install work remains mandatory. No automatic funded
    /// install occurs by producing requirements or by missing namespace fallback.
    pub(super) fn into_geometry_v1(self)->PreparedOriginalGeometryV1 {self.geometry}
}

include!("palw_tir_prepared_actor_catalog_v1.rs");
