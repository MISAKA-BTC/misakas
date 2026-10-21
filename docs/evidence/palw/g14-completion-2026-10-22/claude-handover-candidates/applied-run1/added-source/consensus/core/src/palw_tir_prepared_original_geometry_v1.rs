// Included only within native palw_tir_public_fold_v1. Not a carrier codec.
/// Actual native preparation origin, created only by prepare_paired_original_v1.
/// Frame memory retains the EXISTING caller-admitted preparation contract;
/// this wrapper does not post-hoc relabel that heap as NativeRetained coverage.
pub(super) struct PreparedOriginalFrameV1 {
    frame: Frame,
    at: PalwBlockContextV2,
    native: crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationRulesV1,
    registered: Option<(u64, crate::palw_tir_fence2_v1::PalwTirDemandRulesV1)>,
    network: Option<Hash64>,
    semantic_snapshot:Option<crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
}
impl PreparedOriginalFrameV1 {
    pub(super) fn frame_v1(&self) -> &Frame {
        &self.frame
    }
    /// Opt-in only: old paired preparation/quotes do not reconstruct Source or
    /// reserve a new geometry ticket. Native funded intake remains closed.
    pub(super) fn enrich_native_geometry_v1(
        self,
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
    ) -> Result<PreparedOriginalGeometryV1, PreparedGeometryErrorV1> {
        self.check_preclaim_context_v1(b, ctx)?;
        let job = &self.frame.original_v1().job;
        let record = b.state.tir_class_v1(&job.class_id).ok_or_else(|| {
            PreparedGeometryErrorV1::State(refused("prepared geometry class disappeared"))
        })?;
        let layout =
            crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, job.class_id)
                .ok_or_else(|| {
                    PreparedGeometryErrorV1::State(refused("prepared geometry layout disappeared"))
                })?;
        let original = self.frame.original_v1();
        let source = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
            record,
            layout,
            self.frame.header_v1(),
            original.execution_root,
            original.trace_root,
            self.native.maximum_leaves,
        )
        .map_err(PreparedGeometryErrorV1::Native)?;
        if source.source_id_v1() != self.frame.plan_v1().source_id
            || source.source_id_v1() != self.frame.custody_v1().pledge_v1().source_id
        {
            return Err(PreparedGeometryErrorV1::State(refused(
                "prepared geometry Source differs from actual native frame/pledge",
            )));
        }
        Ok(PreparedOriginalGeometryV1 {
            prepared: self,
            source,
        })
    }
    fn check_preclaim_context_v1(
        &self,
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
    ) -> Result<(), PreparedGeometryErrorV1> {
        if self.registered.is_none() {
            return Err(PreparedGeometryErrorV1::MissingNativeRegistrationPolicy);
        }
        if *ctx != self.at || b.extras.tir_public_network_domain_v1 != self.network {
            return Err(PreparedGeometryErrorV1::State(refused(
                "prepared geometry changes native acceptance point/network",
            )));
        }
        let class = self.frame.original_v1().job.class_id;
        if b.state.class_semantics_v1(&class)!=self.semantic_snapshot.as_ref(){return Err(PreparedGeometryErrorV1::State(refused("prepared geometry immutable class semantics changed")));}
        let class_row = b
            .state
            .class(&class)
            .ok_or(PreparedGeometryErrorV1::MissingNativeRegistrationPolicy)?;
        if class_row.registered_daa > ctx.daa_score
            || class_row.artifact_root != self.frame.header_v1().artifact_root
        {
            return Err(PreparedGeometryErrorV1::State(refused(
                "prepared geometry actual class time/artifact mismatch",
            )));
        }
        let mut native = rules(b).map_err(PreparedGeometryErrorV1::State)?;
        native.maximum_leaves = b.state.class_step_ladder_v1(&class, native.maximum_leaves);
        if native != self.native
            || Some((
                class_row.registered_daa,
                crate::palw_tir_fence2_v1::palw_tir_demand_rules_at_v1(
                    b.params,
                    class_row.registered_daa,
                ),
            )) != self.registered
            || b.state
                .claim(&self.frame.plan_v1().original_claim)
                .is_some()
            || b.state.staged_court_row_v1(&self.frame.key_v1()).is_some()
        {
            return Err(PreparedGeometryErrorV1::State(refused(
                "prepared geometry no longer uses actual uninstalled original/native policy",
            )));
        }
        Ok(())
    }
}
/// Distinguish local resource/out-of-coverage from malformed native preparation.
/// Neither success nor missing pricing bodies grants funded/installed standing.
#[derive(Debug)]
pub(super) enum PreparedGeometryErrorV1 {
    State(PalwStateV2Error),
    Native(crate::palw_native_resource_v1::NativeDecodeErrorV1<String>),
    MissingSharedServiceNamespace,
    MissingNativeRegistrationPolicy,
}
/// Owns genuine native Frame origin plus a separately metered Source. No Clone,
/// Deserialize, caller-frame constructor or independent Complete/paid standing.
pub(super) struct PreparedOriginalGeometryV1 {
    prepared: PreparedOriginalFrameV1,
    source: crate::palw_native_resource_v1::NativeRetainedV1<
        crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    >,
}
impl PreparedOriginalGeometryV1 {
    pub(super) fn frame_v1(&self) -> &Frame {
        self.prepared.frame_v1()
    }
    /// Optional geometry owner can reach the same existing native install seam;
    /// Source ticket stays live until that install attempt returns. No funded
    /// contract or missing profile is discharged by this local geometry result.
    pub(super) fn install_native_v1(
        self,
        b: &mut TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
        prompt: &PalwFpClaimPromptV1,
    ) -> Result<(), PalwStateV2Error> {
        let Self { prepared, source } = self;
        let result = install_prepared_original_v1(b, ctx, prepared, prompt);
        drop(source);
        result
    }

    pub(super) fn source_v1(
        &self,
    ) -> &crate::palw_native_resource_v1::NativeRetainedV1<
        crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    > {
        &self.source
    }
    pub(super) fn demand_v1(&self) -> crate::palw_tir_fence2_v1::PalwTirDemandRulesV1 {
        self.prepared
            .registered
            .expect("enrichment checked actual registration")
            .1
    }
    /// Namespace from current native service state, not a caller-signed profile
    /// hash. This is NOT the missing authoritative fee/capital/retention bodies.
    pub(super) fn source_graph_v3(
        &self,
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
        budget: crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3,
    ) -> Result<
        crate::palw_native_resource_v1::NativeRetainedV1<
            crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3,
        >,
        PreparedGeometryErrorV1,
    > {
        self.prepared.check_preclaim_context_v1(b, ctx)?;
        let book = b
            .state
            .court_service_state_v1()
            .ok_or(PreparedGeometryErrorV1::MissingSharedServiceNamespace)?;
        let namespace =
            crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(book.profile_id);
        crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3::derive_native_v3(
            &self.prepared.frame,
            &self.source,
            namespace,
            budget,
        )
        .map_err(PreparedGeometryErrorV1::Native)
    }
    /// Caller-local exploration bound is NOT an adopted paid profile or rule.
    /// Actual native court/demand are captured by successful preparation only.
    pub(super) fn numerical_v3(
        &self,
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
        budget: crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3,
    ) -> Result<
        crate::palw_native_resource_v1::NativeRetainedV1<
            crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3,
        >,
        PreparedGeometryErrorV1,
    > {
        self.prepared.check_preclaim_context_v1(b, ctx)?;
        let class = self.prepared.frame.original_v1().job.class_id;
        let record = b.state.tir_class_v1(&class).ok_or_else(|| {
            PreparedGeometryErrorV1::State(refused("prepared numerical class absent"))
        })?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, class)
            .ok_or_else(|| {
                PreparedGeometryErrorV1::State(refused("prepared numerical layout absent"))
            })?;
        crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3::derive_native_v3(
            &self.source,
            record,
            layout,
            &self.prepared.native.court,
            self.demand_v1(),
            budget,
        )
        .map_err(PreparedGeometryErrorV1::Native)
    }
}
/// Consume the native holder at the SAME ordinary FP install boundary. This
/// preserves old install behavior; no optional geometry reconstruction occurs.
pub(super) fn install_prepared_original_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    prepared: PreparedOriginalFrameV1,
    prompt: &PalwFpClaimPromptV1,
) -> Result<(), PalwStateV2Error> {
    // ctx is unchanged within this object; ordinary reservation/claim write has
    // now occurred, so do not call the intentionally preclaim-only enrich guard.
    if prepared.at != *ctx {
        return Err(refused(
            "native prepared original install changes accepting point",
        ));
    }
    install_original_v1(b, ctx, prepared.frame, prompt)
}

include!("palw_tir_prepared_original_requirements_v1.rs");
