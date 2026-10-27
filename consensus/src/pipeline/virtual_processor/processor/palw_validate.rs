//! **`VirtualStateProcessor`: palw validate methods** — moved out of `processor.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

mod bond_readiness;
mod class_registration;
mod court;
mod execution;
mod improvement;
mod kernel_material;
mod model_market;
mod panel;
mod reporting;
mod structural;

/// Admission data faults and local native capacity have different dispositions.
/// String-only compatibility callers must not drive checked trap accepted state.
#[derive(Debug)]
pub(in crate::pipeline::virtual_processor) enum PalwObjectAdmissionFaultV1 {
    Resource(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1),
    Semantic(String),
}
impl From<String> for PalwObjectAdmissionFaultV1 {
    fn from(why: String) -> Self {
        Self::Semantic(why)
    }
}
impl std::fmt::Display for PalwObjectAdmissionFaultV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(why) => write!(f, "{why}"),
            Self::Semantic(why) => write!(f, "{why}"),
        }
    }
}

impl VirtualStateProcessor {
    /// Accepted-state consumers use this entry point. Trap reveals do exactly
    /// one checked paid lookup; capacity never falls through a String validator.
    pub(in crate::pipeline::virtual_processor) fn palw_v2_validate_objects_classified_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
    ) -> Result<(), PalwObjectAdmissionFaultV1> {
        for object in objects {
            if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::TrapRevealedV1 { reveal } = object {
                if self.palw_panel_params_v2.is_none() {
                    return Err(PalwObjectAdmissionFaultV1::Semantic("PALW objects on a network with no V2 bundle".into()));
                }
                self.palw_validate_trap_reveal_classified_v1(state, state_params, point, reveal)?;
            } else {
                self.palw_v2_validate_objects(state, state_params, point, std::slice::from_ref(object))?;
            }
        }
        Ok(())
    }

    pub(crate) fn palw_v2_validate_objects(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        let Some(panel_params) = self.palw_panel_params_v2.as_ref() else {
            return if objects.is_empty() { Ok(()) } else { Err("PALW objects on a network with no V2 bundle".to_string()) };
        };
        for object in objects {
            // **RFC-0007's path rule** (spec 18 §18.6): a receipt-path licence for a claim whose panel bound at or after
            // `palw_verification_vertex_v1` is refused by name — it licenses by tally. The fold refuses it too.
            if let Some(claim) = kaspa_consensus_core::palw_vertex_v1::palw_receipt_object_claim_v1(object)
                && kaspa_consensus_core::palw_vertex_v1::palw_vertex_claim_licenses_by_tally_v1(
                    state,
                    state_params,
                    point.daa_score,
                    claim,
                )
            {
                return Err(format!(
                    "claim {claim}: a receipt licence for a claim bound at or after palw_verification_vertex_v1 (RFC-0007: it licenses by tally)"
                ));
            }
            match object {
                Obj::VerificationVertexV1 { .. }
                | Obj::TrapCommittedV1 { .. }
                | Obj::TrapRevealedV1 { .. }
                | Obj::PanelBeaconProofV3 { .. }
                | Obj::VertexEquivocationV1 { .. }
                | Obj::MaterialDisclosedV2 { .. }
                | Obj::PanelUnavailableQuorum { .. }
                | Obj::PanelBound { .. }
                | Obj::ReceiptLicensed { .. }
                | Obj::ProducerDefaulted { .. }
                | Obj::ReceiptLicensedV2 { .. }
                | Obj::ReceiptLicensedBatchV1 { .. }
                | Obj::AuditReceiptBatchV1 { .. }
                | Obj::AttemptRidersV1 { .. }
                | Obj::OptimisticLicensed { .. }
                | Obj::TirShardReceiptLicensed { .. }
                | Obj::ShardReceiptLicensed { .. }
                | Obj::MaterialDisclosed { .. } => {
                    self.palw_validate_panel_object_v1(state, state_params, point, panel_params, object)?
                }
                Obj::ExecWorkRootOpenedV2 { .. }
                | Obj::ExecOriginRecordedV1 { .. }
                | Obj::FreePromptCommitted { .. }
                | Obj::GenTensorCommitted { .. }
                | Obj::DerivedArtifactV1 { .. } => self.palw_validate_execution_object_v1(state, state_params, point, object)?,
                Obj::ReporterCommitted { .. } | Obj::ReporterRevealed { .. } | Obj::ObjectiveOffence { .. } => {
                    self.palw_validate_reporting_object_v1(state, state_params, point, object)?
                }
                Obj::CourtOpened { .. }
                | Obj::CourtCloseDeclared { .. }
                | Obj::CourtCloseChunk { .. }
                | Obj::CourtDisclosed { .. }
                | Obj::CourtVerdictPosted { .. }
                | Obj::CourtClosed { .. }
                | Obj::CourtAttnRootClaimed { .. }
                | Obj::CourtAttnRootClaimedAnchored { .. }
                | Obj::CourtAttnRootClaimedHeld { .. }
                | Obj::CourtAttnDissected { .. }
                | Obj::CourtAttnChildChosen { .. }
                | Obj::CourtTirRootClaimed { .. }
                | Obj::CourtGenRootClaimed { .. }
                | Obj::CourtEvalRootClaimed { .. }
                | Obj::CourtTirDissected { .. }
                | Obj::CourtTirChildChosen { .. }
                | Obj::DefaultAccused { .. }
                | Obj::TirShardCourtAccused { .. }
                | Obj::GenShardCourtAccused { .. }
                | Obj::HeldLeafChallengeDeclared { .. }
                | Obj::GdnStepAccusedV2 { .. }
                | Obj::MemoryRoutingV2 { .. }
                | Obj::TirDesignatedEventV1 { .. }
                | Obj::SeatReadinessRootV1 { .. }
                | Obj::SeatReadinessDemandedV1 { .. }
                | Obj::SeatReadinessAnsweredV1 { .. }
                | Obj::MaterialIntakeV1 { .. }
                | Obj::PanelDaPromptDisclosedV1 { .. }
                | Obj::TirPositionDissectedV2 { .. }
                | Obj::TirPositionClosedV2 { .. }
                | Obj::TirLookaheadDisclosedV2 { .. }
                | Obj::TirLookaheadChosenV2 { .. }
                | Obj::ShardCourtAccused { .. }
                | Obj::CheckpointAccused { .. }
                | Obj::DefaultAccusedTirStep { .. }
                | Obj::DefaultAccusedPipelineStep { .. }
                | Obj::DefaultAccusedHeld { .. }
                | Obj::MaterialDisclosedHeld { .. }
                | Obj::TirShardPlanDeclared { .. }
                | Obj::DisputeReservedV1 { .. }
                | Obj::DisputeReacquiredV1 { .. }
                | Obj::DisputeReleasedV1 { .. }
                | Obj::LegacyHeldDemandedV2 { .. }
                | Obj::LegacyHeldAnsweredV2 { .. }
                | Obj::PublicCourtStreamOpenedV1 { .. }
                | Obj::TirPublicControlV1 { .. }
                | Obj::SharedCourtOpV1 { .. }
                | Obj::TirPublicGrantReadyV1 { .. }
                | Obj::PublicCourtStreamOpenedNativeV1 { .. }
                | Obj::PublicCourtStreamLeafV1 { .. }
                | Obj::PublicCourtStreamMoveV1 { .. }
                | Obj::ObjectiveEvidenceDeclaredV1 { .. }
                | Obj::ObjectiveEvidenceChunkV1 { .. }
                | Obj::LegacyHeldAnswerDeclaredV2 { .. }
                | Obj::LegacyHeldAnswerChunkV2 { .. }
                | Obj::LegacyLeafRecomputedV2 { .. } => self.palw_validate_court_object_v1(state, state_params, point, object)?,
                Obj::ModelBuy { .. }
                | Obj::ModelSeed { .. }
                | Obj::ActivationPoolFunded { .. }
                | Obj::ModelSell { .. }
                | Obj::ModelLineFounded { .. }
                | Obj::ModelVersionPublished { .. }
                | Obj::ModelVersionPromoted { .. }
                | Obj::ModelVersionWithdrawn { .. }
                | Obj::ModelLineBenefitsDeclared { .. }
                | Obj::ModelLineRolesSet { .. }
                | Obj::ModelLineOwnerTransferred { .. }
                | Obj::ModelLineRetired { .. }
                | Obj::ModelProposalPosted { .. }
                | Obj::ModelProposalClosed { .. }
                | Obj::ModelEvaluationPosted { .. } => self.palw_validate_model_market_object_v1(state, point, object)?,
                Obj::ClassRegistered { .. }
                | Obj::ClassFrozen { .. }
                | Obj::ClassManifestV2 { .. }
                | Obj::ClassShardPlanDeclared { .. }
                | Obj::ClassRegisteredGenV1 { .. }
                | Obj::ClassRegisteredTirV1 { .. }
                | Obj::FamilyCertified { .. }
                | Obj::ClassLaneCertified { .. }
                | Obj::ClassLaneCertifiedTirV1 { .. }
                | Obj::TirClassLayoutPublishedV1 { .. }
                | Obj::TirClassContainerBoundV1 { .. }
                | Obj::ClassSemanticsDeclaredV1 { .. }
                | Obj::AdapterClassListed { .. } => {
                    self.palw_validate_class_registration_object_v1(state, state_params, point, object)?
                }
                Obj::BondRetireRequested { .. }
                | Obj::BondRegistered { .. }
                | Obj::BondCapabilityDeclared { .. }
                | Obj::SeatReadinessProvedV2 { .. }
                | Obj::SeatReadinessProved { .. }
                | Obj::RoundPermitEquivocated { .. }
                | Obj::TirSeatReadinessProved { .. }
                | Obj::BondShardsDeclared { .. }
                | Obj::BondCapitalAssignedV1 { .. } => self.palw_validate_bond_readiness_object_v1(state, point, object)?,
                Obj::ObjectChunk { .. }
                | Obj::KernelRouteV1 { .. }
                | Obj::KernelRouteChunkV1 { .. }
                | Obj::ArtifactBoundV1 { .. }
                | Obj::ArtifactBindingChallengedV1 { .. }
                | Obj::KernelBoundV1 { .. }
                | Obj::ConformanceCommittedV1 { .. }
                | Obj::ConformanceEvidenceV1 { .. }
                | Obj::MonetaryMaterialEnrolledV1 { .. }
                | Obj::MonetaryMaterialReplacedV1 { .. }
                | Obj::MonetaryMaterialActionV1 { .. }
                | Obj::ProviderLeaseV1 { .. }
                | Obj::ProviderChallengeV1 { .. }
                | Obj::ProviderAnswerV1 { .. }
                | Obj::DaTransferV1 { .. }
                | Obj::KernelConstraintReceiptV1 { .. } => {
                    self.palw_validate_kernel_material_object_v1(state, state_params, point, object)?
                }
                Obj::ModelLineImprovementPolicySet { .. }
                | Obj::ImprovementPoolFunded { .. }
                | Obj::LineageHeadRolledBack { .. }
                | Obj::HardCaseSubmitted { .. }
                | Obj::SetterSetCommitted { .. }
                | Obj::DatasetRegistered { .. }
                | Obj::TeachingArtifactCommitted { .. }
                | Obj::DataUseOptIn { .. }
                | Obj::SetterSetRevealed { .. }
                | Obj::SetterKeysRevealed { .. }
                | Obj::HardCaseKeyRevealed { .. }
                | Obj::TeachingArtifactRevealed { .. }
                | Obj::TeacherLicenceRegistered { .. }
                | Obj::CandidateSubmitted { .. } => self.palw_validate_improvement_object_v1(state, point, object)?,
                Obj::SignedRegistrationV1 { .. } | Obj::ReservedPipelineDa84(..) | Obj::ReservedPipelineDa85(..) => {
                    self.palw_validate_structural_object_v1(object)?
                }
                Obj::SourceBootstrapV1 { actor, action, signature } => {
                    use kaspa_consensus_core::palw_source_bootstrap_v1 as bootstrap;
                    let policy = self
                        .palw_selected_source_bootstrap_v1
                        .as_ref()
                        .filter(|f| {
                            f.at != kaspa_consensus_core::config::params::ForkActivation::never() && f.at.is_active(point.daa_score)
                        })
                        .map(|f| &f.policy)
                        .ok_or("selected-source bootstrap is dormant on this network")?;
                    if policy.network != self.palw_network_domain_v2() {
                        return Err("bootstrap policy network differs from native chain".into());
                    }
                    let bond = state.bond(actor).ok_or("bootstrap actor bond is missing")?;
                    let message = bootstrap::palw_source_bootstrap_message_v1(policy.network, *actor, action);
                    if !Self::verify_mldsa87_with_context_bool(
                        &bond.pubkey,
                        message.as_byte_slice(),
                        signature,
                        bootstrap::PALW_SELECTED_SOURCE_BOOTSTRAP_MLDSA87_CONTEXT_V1,
                    ) {
                        return Err(
                            "bootstrap carrier lacks the native actor signature over this exact epoch/work/slot statement".into()
                        );
                    }
                }
                Obj::ReaderRouterDeclaredV2 { declaration, signature } => {
                    use kaspa_consensus_core::palw_reader_router_v2 as routing;
                    if !self.palw_memory_routing_court_v2_at(point.daa_score) {
                        return Err("reader/router V2 declaration is dormant on this network".into());
                    }
                    let class = state.class(&declaration.class_id).ok_or("routing declaration names an unknown class")?;
                    if class.artifact_root != declaration.artifact_root {
                        return Err("routing declaration names another artifact".into());
                    }
                    let registrant = class.registrant_bond.as_ref().ok_or("sentinel class has no signing registrant")?;
                    let bond = state.bond(registrant).ok_or("routing registrant bond is absent")?;
                    if !matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                        return Err("routing registrant is not Active".into());
                    }
                    // Graph checks and typed allocation refusal run in the native fold.
                    // This signature-only String walk does not mask local capacity as peer fault.
                    let message = routing::reader_router_message_v2(self.palw_network_domain_v2(), declaration);
                    if !Self::verify_mldsa87_with_context_bool(
                        &bond.pubkey,
                        message.as_byte_slice(),
                        signature,
                        routing::PALW_READER_ROUTER_SIGNATURE_CONTEXT_V2,
                    ) {
                        return Err("routing declaration lacks its original class registrant signature".into());
                    }
                }
                Obj::RoutedMaterialAnsweredV2 { answer } => {
                    if !self.palw_memory_routing_court_v2_at(point.daa_score) {
                        return Err("routed material V2 is dormant on this network".into());
                    }
                    let court = self.palw_court_params_v2.as_ref().ok_or("routed material without native court parameters")?;
                    let claim = state.claim(&answer.claim).ok_or("routed material claim absent")?;
                    let ladder = state.class_step_ladder_v1(&claim.class_id, self.palw_court_step_ladder_at(point.daa_score, court));
                    answer.check_shape(ladder).map_err(str::to_string)?;
                    if borsh::object_length(answer).map_err(|e| e.to_string())? as u64 > court.max_close_bytes() {
                        return Err("routed material answer exceeds current close-byte ceiling".into());
                    }
                    let signer = state.bond(&answer.provider).ok_or("routed material provider bond absent")?;
                    if !Self::verify_mldsa87_with_context_bool(
                        &signer.pubkey,
                        answer.message(self.palw_network_domain_v2()).as_byte_slice(),
                        &answer.signature,
                        kaspa_consensus_core::palw_routed_material_v2::PALW_ROUTED_MATERIAL_CONTEXT_V2,
                    ) {
                        return Err("routed material answer lacks its exact provider signature".into());
                    }
                    // Native membership and local workspace admission run in the
                    // typed fold. A local refusal is not a peer-object verdict.
                }
                Obj::RoutedStepAccusedV2 { accusation } => {
                    if !self.palw_memory_routing_court_v2_at(point.daa_score) {
                        return Err("routing version-2 court is dormant on this network".into());
                    }
                    let court = self.palw_court_params_v2.as_ref().ok_or("routing accusation without V2 court parameters")?;
                    let claim = state.claim(&accusation.claim).ok_or("routing accusation names an absent native claim")?;
                    let ladder = state.class_step_ladder_v1(&claim.class_id, self.palw_court_step_ladder_at(point.daa_score, court));
                    accusation.validate_shape(ladder)?;
                    if accusation.encoded_bytes()? > court.max_close_bytes() {
                        return Err("routing accusation exceeds the current close-byte ceiling".into());
                    }
                    if claim.bond != accusation.executor_bond
                        || claim.execution_root != accusation.execution_root
                        || claim.trace_root != accusation.trace_root
                    {
                        return Err("routing accusation executor/roots are not the actual native claim's".into());
                    }
                    state.class(&claim.class_id).ok_or("routing accusation class is absent")?;
                    let signer = state.bond(&accusation.accuser_bond).ok_or("routing accuser bond is absent")?;
                    let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let message = kaspa_consensus_core::palw_routing_court_v2::palw_routing_court_session_id_v2(network, accusation);
                    if !Self::verify_mldsa87_with_context_bool(
                        &signer.pubkey,
                        message.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_routing_court_v2::PALW_ROUTING_COURT_MLDSA87_CONTEXT_V2,
                    ) {
                        return Err("routing accusation is not signed by its registered accuser".into());
                    }
                    // Full deterministic adjudication runs in the typed fold.
                    // This String-only walk must not turn a local workspace
                    // refusal into a skipped invalid peer object.
                }
            }
        }
        Ok(())
    }
}
