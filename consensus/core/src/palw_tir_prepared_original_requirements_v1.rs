// Included within the actual native fold; no caller-decoded holder constructor.
use crate::palw_court_shared_tir_builder_store_v1::original_aggregate::{AggregateErrorV1, OriginalFamilyShapeV1};
#[derive(Clone, Copy)]
pub(super) struct PreparedRequirementBudgetsV1 {
    pub numerical: crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3,
    pub input: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1,
    pub source: crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3,
}
#[derive(Debug)]
pub(super) enum PreparedRequirementsErrorV1 {
    Geometry(PreparedGeometryErrorV1),
    Shape(AggregateErrorV1),
    Native(crate::palw_native_resource_v1::NativeDecodeErrorV1<String>),
    MissingSharedServiceNamespace,
}
enum PreparedSourceRequirementsV1 {
    MissingSharedServiceNamespace,
    Native {
        namespace: Hash64,
        graph:
            crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3>,
    },
}
/// Real native-origin Source/numerical/all-root requirements before FP reserve.
/// Retains no forged Claim/context, no paid permit and no fully reservable draft.
pub(super) struct PreparedOriginalRequirementsV1 {
    geometry: PreparedOriginalGeometryV1,
    shape: OriginalFamilyShapeV1,
    actors: PreparedActorCatalogV1,
    numerical: crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3>,
    inputs: crate::palw_native_resource_v1::NativeRetainedV1<
        crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1,
    >,
    input_material: crate::palw_court_shared_material_intake_v1::InputMaterialDemandV1,
    carrier_inputs_v3:Option<crate::palw_court_family_material_v1::NativeFamilyCarrierInputsV3>,
    source: PreparedSourceRequirementsV1,
    // New wrapper inline only. Moved old Frame payload retains its legacy native
    // preparation admission, and all new Source/geometry/cover have own leases.
    _wrapper: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl PreparedOriginalGeometryV1 {
    /// Consuming actual native-origin entry only; public Frame decoders and
    /// arbitrary profile hashes cannot enter this API. Old object path is opt-in.
    pub(super) fn prepare_requirements_v1(
        self,
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
        budget: PreparedRequirementBudgetsV1,
    ) -> Result<PreparedOriginalRequirementsV1, PreparedRequirementsErrorV1> {
        self.prepared.check_preclaim_context_v1(b, ctx).map_err(PreparedRequirementsErrorV1::Geometry)?;
        let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
        q.exact_v1::<PreparedOriginalRequirementsV1>(1)
            .map_err(|c| PreparedRequirementsErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)))?;
        let wrapper = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1())
            .map_err(|c| PreparedRequirementsErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)))?;
        // Same immutable snapshot that successful native preparation consumed,
        // not postclaim lookup, caller declaration or reconstructed fake frame.
        let shape = OriginalFamilyShapeV1::derive_v1(self.frame_v1(), self.prepared.semantic_snapshot.as_ref())
            .map_err(PreparedRequirementsErrorV1::Shape)?;
        let actors = PreparedActorCatalogV1::derive_v1(&self, b, ctx).map_err(PreparedRequirementsErrorV1::Geometry)?;
        let numerical = self.numerical_v3(b, ctx, budget.numerical).map_err(PreparedRequirementsErrorV1::Geometry)?;
        let class = self.frame_v1().original_v1().job.class_id;
        let record = b.state.tir_class_v1(&class).ok_or_else(|| {
            PreparedRequirementsErrorV1::Geometry(PreparedGeometryErrorV1::State(refused("prepared input class absent")))
        })?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, class).ok_or_else(|| {
            PreparedRequirementsErrorV1::Geometry(PreparedGeometryErrorV1::State(refused("prepared input layout absent")))
        })?;
        let inputs = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::root_input_cover_native_v1(
            self.source_v1(),
            record,
            layout,
            budget.input,
        )
        .map_err(PreparedRequirementsErrorV1::Native)?;
        // This actual relation is checked even when no service namespace exists.
        if numerical.source_id_v3() != inputs.parameters_v1().source_id
            || numerical.original_leaves_v3() != inputs.parameters_v1().original_leaves
            || numerical.roots_v3().len() != inputs.roots_v1().len()
            || numerical.roots_v3().iter().zip(inputs.roots_v1()).any(|(n, i)| n.leaf_v3() != i.leaf)
        {
            return Err(PreparedRequirementsErrorV1::Geometry(PreparedGeometryErrorV1::State(refused(
                "prepared numerical/input root relation mismatch",
            ))));
        }
        let program = crate::palw_native_resource_v1::decode_native_tir_program_v1(record.program.as_slice()).map_err(|e| {
            PreparedRequirementsErrorV1::Native(match e {
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)
                }
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e.to_string())
                }
            })
        })?;
        let input_material = crate::palw_court_shared_material_intake_v1::input_material_demand_v1(
            &program,
            &inputs.parameters_v1().union,
            self.frame_v1().plan_v1().owner_work.rules.prompt_form,
            self.source_v1().header_v1().job_context.declared_prefill_tokens,
        )
        .map_err(PreparedRequirementsErrorV1::Native)?;
        drop(program);
        let source = match self.source_graph_v3(b, ctx, budget.source) {
            Ok(graph) => {
                let profile = prepared_service_profile_v1(b, ctx).map_err(PreparedRequirementsErrorV1::Geometry)?;
                PreparedSourceRequirementsV1::Native {
                    namespace: crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(profile),
                    graph,
                }
            }
            Err(PreparedGeometryErrorV1::MissingSharedServiceNamespace) => PreparedSourceRequirementsV1::MissingSharedServiceNamespace,
            Err(e) => return Err(PreparedRequirementsErrorV1::Geometry(e)),
        };
        Ok(PreparedOriginalRequirementsV1 {
            geometry: self,
            shape,
            actors,
            numerical,
            inputs,
            input_material,
            carrier_inputs_v3:None,
            source,
            _wrapper: wrapper,
        })
    }
}
impl PreparedOriginalRequirementsV1 {
    pub(super) fn bind_carrier_inputs_v3(&mut self,inputs:&[crate::tx::TransactionInput],b:&TransitionBuilder<'_>)->Result<(),PalwStateV2Error>{
        let frame=self.frame_v1();let original=frame.original_v1();let claim=frame.plan_v1().original_claim;
        let prefix=b.extras.prefix_dependencies_v1.get(&claim).and_then(|e|e.consumed.as_ref());
        let facts=crate::palw_court_family_material_v1::NativeFamilyCarrierInputsV3::derive_v3(inputs,prefix,claim,original.job.network_domain,&original.job.executor_pubkey).map_err(|e|match e{crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)=>PalwStateV2Error::NativeResource(c),crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e)=>refused(e)})?;
        self.carrier_inputs_v3=Some(facts);Ok(())
    }
    pub(super) fn frame_v1(&self) -> &Frame {
        self.geometry.frame_v1()
    }
    pub(super) fn shape_v1(&self) -> OriginalFamilyShapeV1 {
        self.shape
    }
    pub(super) fn numerical_v1(&self) -> &crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3 {
        &self.numerical
    }
    pub(super) fn inputs_v1(&self) -> &crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1 {
        &self.inputs
    }
    pub(super) fn input_material_v1(&self) -> crate::palw_court_shared_material_intake_v1::InputMaterialDemandV1 {
        self.input_material
    }
    pub(super) fn prepare_family_plans_v1(
        &self,
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
        consents: &[crate::palw_court_family_supplier_v1::SignedFamilySupplierV1],
        semantic_table: Option<&[u8]>,
    ) -> Result<Vec<super::palw_court_family_supplier_fold_v1::FamilySupplierPlanV1>, PalwStateV2Error> {
        let (inventories, _lease) = self.family_inventories_v3(b, ctx, semantic_table)?;
        if consents.len() != inventories.len() || consents.iter().zip(&inventories).any(|(c, i)| c.terms.role != i.role_v1()) {
            return Err(refused("TFO2 lacks exact full native family supplier role set"));
        }
        let hard = ctx
            .daa_score
            .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
            .ok_or_else(|| refused("family hard overflow"))?;
        let material =
            crate::palw_court_service_state_v2::tir_service_required_material_until_v1(self.frame_v1(), hard).map_err(refused)?;
        let mut plans = Vec::with_capacity(consents.len());
        for (consent, inventory) in consents.iter().zip(&inventories) {
            plans.push(super::palw_court_family_supplier_fold_v1::derive_family_supplier_plan_v1(
                &b.state,
                ctx,
                inventory,
                self.frame_v1().plan_v1().allocation_id,
                material,
                consent,
            )?);
        }
        super::palw_court_family_supplier_fold_v1::check_family_funding_v1(b, ctx.daa_score, &plans)?;
        Ok(plans)
    }
    pub(super) fn family_inventories_v1(
        &self,
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
        semantic_table: Option<&[u8]>,
    ) -> Result<
        (
            Vec<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_family_material_v1::NativeFamilyInventoryV1>>,
            Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
        ),
        PalwStateV2Error,
    > {
        self.family_inventories_with_purpose_v3(b,ctx,semantic_table,false)
    }
    pub(super) fn family_inventories_v3(&self,b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2,semantic_table:Option<&[u8]>)->Result<(Vec<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_family_material_v1::NativeFamilyInventoryV1>>,Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>),PalwStateV2Error>{
        self.family_inventories_with_purpose_v3(b,ctx,semantic_table,true)
    }
    fn family_inventories_with_purpose_v3(&self,b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2,semantic_table:Option<&[u8]>,generated_semantic_v3:bool)->Result<(Vec<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_family_material_v1::NativeFamilyInventoryV1>>,Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>),PalwStateV2Error>{
        use crate::palw_court_family_supplier_v1::FamilySupplierRoleV1 as R;
        let bad = |e: PreparedRequirementsErrorV1| refused(format!("family native requirements: {e:?}"));
        self.geometry.prepared.check_preclaim_context_v1(b, ctx).map_err(|e| refused(format!("family native original: {e:?}")))?;
        let frame = self.frame_v1();
        let sem = self.geometry.prepared.semantic_snapshot.as_ref();
        let mut roles = vec![R::ParameterAndPrompt];
        if sem.is_some_and(|s| s.token_table_root.is_some()) {
            roles.push(R::SemanticTable);
        }
        if sem.is_some_and(|s| s.raw_tokenizer_bytes.is_some()) {
            roles.push(R::TokenizerArtifact);
        }
        if matches!(frame.original_v1().job.tail.as_ref(), Some(crate::palw_freeprompt_v3::PalwFpJobTailV1::Prefix(_))) {
            roles.push(R::PrefixGeneratedSelectionAndStop);
        }
        let class = frame.original_v1().job.class_id;
        let record = b.state.tir_class_v1(&class).ok_or_else(|| refused("family native class absent"))?;
        let program =
            crate::palw_native_resource_v1::decode_native_tir_program_v1(record.program.as_slice()).map_err(|e| match e {
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => PalwStateV2Error::NativeResource(c),
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e.to_string()),
            })?;
        let graph = self.source_graph_v1().map_err(bad)?;
        let profile = prepared_service_profile_v1(b, ctx).map_err(|e| refused(format!("family service namespace: {e:?}")))?;
        let profile = crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(profile);
        let checked_table = match (semantic_table, sem) {
            (Some(bytes), Some(sem)) if roles.contains(&R::SemanticTable) => Some(
                crate::palw_court_family_material_v1::prepare_family_table_inventory_v1(
                    bytes,
                    sem,
                    self.geometry.source_v1().vocab_v1(),
                )
                .map_err(|e| match e {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => PalwStateV2Error::NativeResource(c),
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
                })?,
            ),
            (None, _) if !roles.contains(&R::SemanticTable) => None,
            _ => return Err(refused("TFO2 requires exact canonical semantic table inventory iff native scope uses it")),
        };
        let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
        q.exact_v1::<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_family_material_v1::NativeFamilyInventoryV1>>(
            roles.len() as u64,
        )
        .map_err(PalwStateV2Error::NativeResource)?;
        let lease =
            crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let mut inventories = Vec::with_capacity(roles.len());
        for role in roles {
            let derive=if generated_semantic_v3{crate::palw_court_family_material_v1::derive_family_inventory_with_table_v3}else{crate::palw_court_family_material_v1::derive_family_inventory_with_table_v1};
            let inventory = derive(
                &b.state,
                frame,
                self.geometry.source_v1(),
                graph,
                profile,
                &program,
                &self.inputs,
                sem,
                role,
                checked_table.as_ref(),
            )
            .map_err(|e| match e {
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => PalwStateV2Error::NativeResource(c),
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
            })?;
            let inventory=if generated_semantic_v3{crate::palw_court_family_material_v1::bind_family_carrier_inputs_v3(inventory,self.carrier_inputs_v3.as_ref().ok_or_else(||refused("profile3 requires actual candidate carrier inputs before quote"))?).map_err(|e|match e{crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)=>PalwStateV2Error::NativeResource(c),crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e)=>refused(e)})?}else{inventory};
            inventories.push(inventory);
        }
        Ok((inventories, lease))
    }
    pub(super) fn source_graph_v1(
        &self,
    ) -> Result<&crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3, PreparedRequirementsErrorV1> {
        match &self.source {
            PreparedSourceRequirementsV1::Native { graph, .. } => Ok(graph),
            PreparedSourceRequirementsV1::MissingSharedServiceNamespace => {
                Err(PreparedRequirementsErrorV1::MissingSharedServiceNamespace)
            }
        }
    }
    /// Borrowing join avoids a self-referential owner and keeps all original
    /// Source/num/cover tickets alive. No caller profile or surrogate Claim.
    pub(super) fn numerical_material_join_v1<'a>(
        &'a self,
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
        max_visits: u64,
    ) -> Result<
        crate::palw_native_resource_v1::NativeRetainedV1<
            crate::palw_court_shared_tir_builder_store_v1::numerical_material_join::NumericalMaterialJoinV1<'a>,
        >,
        PreparedRequirementsErrorV1,
    > {
        self.geometry.prepared.check_preclaim_context_v1(b, ctx).map_err(PreparedRequirementsErrorV1::Geometry)?;
        let PreparedSourceRequirementsV1::Native { namespace, graph } = &self.source else {
            return Err(PreparedRequirementsErrorV1::MissingSharedServiceNamespace);
        };
        let profile = prepared_service_profile_v1(b, ctx).map_err(PreparedRequirementsErrorV1::Geometry)?;
        if *namespace != crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(profile) {
            return Err(PreparedRequirementsErrorV1::Geometry(PreparedGeometryErrorV1::State(refused(
                "prepared source native namespace changed",
            ))));
        }
        crate::palw_court_shared_tir_builder_store_v1::numerical_material_join::NumericalMaterialJoinV1::derive_native_v1(
            self.frame_v1(),
            self.geometry.source_v1(),
            graph,
            &self.numerical,
            &self.inputs,
            *namespace,
            &self.geometry.prepared.native.court,
            self.geometry.demand_v1(),
            max_visits,
        )
        .map_err(PreparedRequirementsErrorV1::Native)
    }
    /// Old reservation/Claim/install work remains mandatory. No automatic funded
    /// install occurs by producing requirements or by missing namespace fallback.
    pub(super) fn into_geometry_v1(self) -> PreparedOriginalGeometryV1 {
        self.geometry
    }
}

include!("palw_tir_prepared_actor_catalog_v1.rs");

include!("palw_tir_prepared_archive_adjunct_v11.rs");

// Prospective readonly quote inputs, kept on the actual native preparation.
// Neither getter constructs a paid permit or an accepting point.
impl PreparedOriginalRequirementsV1 {
    pub(super) fn archive_semantics_v10(&self)->Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>{self.geometry.prepared.semantic_snapshot.as_ref()}
    pub(super) fn archive_rules_v10(&self)->crate::palw_court_family_material_v1::FamilyOriginalRuleContextV2 {crate::palw_court_family_material_v1::FamilyOriginalRuleContextV2::from_native_v2(self.geometry.prepared.native)}
}
