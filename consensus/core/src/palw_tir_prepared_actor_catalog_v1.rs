// Included in the private native original fold. No wire or caller constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreparedRoleContractMissingV1 {
    CanonicalWholeBranchActorAssignment,
    SharedServiceOperatorConsent,
    FeeCapitalRetentionRecoveryBodies,
    SemanticMaterialProviderConsent,
}
/// Only the existing source contract, not a new all-branch provider grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PreparedLegacySourceActorV1 {
    pub bond:PalwBondKeyV2,
    pub operator:Hash64,
    pub key_id:Hash64,
    pub pledge_id:Hash64,
    pub accept_until_daa:u64,
    pub material_until_daa:u64,
}
/// Fixed-size native-derived catalog snapshot. Anonymous graph role slots are
/// not entries or unnamed actors. No allocation or signature/material clone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PreparedActorCatalogV1 {
    source:PreparedLegacySourceActorV1,
    potential_source_slots:u64,
    semantic_required:bool,
}
impl PreparedActorCatalogV1 {
    fn derive_v1(g:&PreparedOriginalGeometryV1,b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2)->Result<Self,PreparedGeometryErrorV1>{
        g.prepared.check_preclaim_context_v1(b,ctx)?;
        let frame=g.frame_v1();let original=frame.original_v1();let plan=frame.plan_v1();
        let contract=frame.custody_v1();let pledge=contract.pledge_v1();
        contract.check_v1().map_err(|e|PreparedGeometryErrorV1::State(refused(e)))?;
        if pledge.producer!=PalwBondKeyV2(original.job.executor_bond)
            || pledge.claim!=plan.original_claim || pledge.execution_root!=original.execution_root
            || pledge.job_identity!=crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original)
            || pledge.source_id!=g.source_v1().source_id_v1()
            || pledge.resource_plan_id!=plan.resource_plan_id
            || pledge.maximum_custodians!=plan.acquisition.maximum_custodians
            || contract.terms_v1()!=plan.terms {
            return Err(PreparedGeometryErrorV1::State(refused("prepared actor source contract differs from actual native original")));
        }
        let bond=b.state.bond(&pledge.producer).ok_or_else(||PreparedGeometryErrorV1::State(refused("prepared source actor missing")))?;
        if !matches!(bond.status,PalwBondStatusV2::Active)
            || bond.pubkey!=original.job.executor_pubkey
            || bond.operator_id!=palw_operator_id_v2(&bond.pubkey) {
            return Err(PreparedGeometryErrorV1::State(refused("prepared source actor current authority changed")));
        }
        pledge.validate_signature_v1(&bond.pubkey,crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)
            .map_err(|e|PreparedGeometryErrorV1::State(refused(e)))?;
        Ok(Self{source:PreparedLegacySourceActorV1{bond:pledge.producer,operator:bond.operator_id,
            key_id:crate::mldsa87_primitives::mldsa87_key_id(&bond.pubkey),pledge_id:pledge.id_v1(),
            accept_until_daa:pledge.accept_until_daa,material_until_daa:pledge.material_until_daa},
            potential_source_slots:plan.acquisition.maximum_custodians,semantic_required:plan.semantic.is_some()})
    }
    pub(super) fn source_actor_v1(&self)->PreparedLegacySourceActorV1 {self.source}
    pub(super) fn potential_source_slots_v1(&self)->u64 {self.potential_source_slots}
    pub(super) fn missing_contracts_v1(&self)->impl Iterator<Item=PreparedRoleContractMissingV1>{
        use PreparedRoleContractMissingV1::*;
        [Some(CanonicalWholeBranchActorAssignment),Some(SharedServiceOperatorConsent),Some(FeeCapitalRetentionRecoveryBodies),
            self.semantic_required.then_some(SemanticMaterialProviderConsent)].into_iter().flatten()
    }
}
impl PreparedOriginalRequirementsV1 {
    /// Revalidate the genuine prepared origin and current authority, not a
    /// caller manifest. Rechecks signatures; never grants new duties/payments.
    pub(super) fn actor_catalog_v1(&self,b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2)->Result<&PreparedActorCatalogV1,PreparedGeometryErrorV1>{
        let current=PreparedActorCatalogV1::derive_v1(&self.geometry,b,ctx)?;
        if current!=self.actors{return Err(PreparedGeometryErrorV1::State(refused("prepared actor catalog changed")));}
        Ok(&self.actors)
    }
}
